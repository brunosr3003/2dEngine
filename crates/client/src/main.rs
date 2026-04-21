//! Cliente MMORPG 2D — Fase 2.
//!
//! O que ha de novo vs fase 1:
//! - Atlas procedural carregado no GPU no init.
//! - Tilemap com mundo gerado proceduralmente (rooms + corridors).
//! - AnimPlayer ligado a cada jogador/inimigo visivel.
//! - Nomes acima dos jogadores com BitmapFont.
//! - HUD: coordenadas, ping e dica de teclas.

mod atlas_gen;
mod interpolation;
mod net_client;
mod prediction;


use atlas_gen::AtlasLayout;
use engine::anim::{AnimClip, AnimPlayer, AnimRegistry};
use engine::app::{run, AppConfig, AppContext, Game};
use engine::glam::{Vec2, Vec4};
use engine::render::{layer, BitmapFont, Renderer, Sprite, SpriteBatch};
use engine::tilemap::{TileDef, Tilemap};
use engine::winit::keyboard::KeyCode;
use interpolation::{InterpolationBuffer, RENDER_DELAY_MS};
use net_client::NetClient;
use prediction::PredictionBuffer;
use shared::protocol::{buttons, ClientMessage, InputFrame, ServerMessage};
use shared::{EntityId, EntityKind, EntitySnapshot};
use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};

fn now_ms() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0)
}

// IDs de clip de animacao (indexados no AnimRegistry)
const ANIM_PLAYER_WALK: &str = "player_walk";
const ANIM_PLAYER_IDLE: &str = "player_idle";
const ANIM_ENEMY_WALK:  &str = "enemy_walk";

fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info,client=debug,engine=info".into()),
        )
        .init();

    let server_url = std::env::var("SERVER_URL")
        .unwrap_or_else(|_| "ws://127.0.0.1:9000".into());
    // PLAYER_NAME tem prioridade; depois USER (POSIX) / USERNAME (Windows)
    // como fallback. Em macOS USER=bruno mas USERNAME nao e exportado.
    let username = std::env::var("PLAYER_NAME")
        .or_else(|_| std::env::var("USER"))
        .or_else(|_| std::env::var("USERNAME"))
        .unwrap_or_else(|_| format!("Guest{}", std::process::id() & 0xFFFF));
    let password = std::env::var("PLAYER_PASSWORD").unwrap_or_default();
    if password.is_empty() {
        tracing::warn!(
            "PLAYER_PASSWORD nao definido — o servidor vai recusar o login. \
             Cadastre a conta em http://localhost:5173 e exporte PLAYER_PASSWORD."
        );
    }

    run(
        AppConfig { title: "MMORPG 2D — Fase 2", tick_hz: 30, initial_size: (1280, 720) },
        MmoClient::new(server_url, username, password),
    )
}

// ---------------------------------------------------------------------------

pub struct MmoClient {
    server_url: String,
    username:   String,
    password:   String,
    net:        Option<NetClient>,

    // Mundo
    tilemap:        Option<Tilemap>,
    world_map:      Option<shared::world_gen::WorldMap>,
    anim_registry:  AnimRegistry,
    // AnimPlayer por EntityId (remoto) ou proprio jogador
    anim_players:   HashMap<EntityId, AnimPlayer>,
    // Nomes dos jogadores remotos (id -> nome)
    entity_names:   HashMap<EntityId, String>,
    font:           Option<BitmapFont>,

    // Estado replicado
    interp:           InterpolationBuffer,
    visible_entities: Vec<EntitySnapshot>,

    // Proprio jogador
    self_entity:          Option<EntityId>,
    prediction:           Option<PredictionBuffer>,
    input_seq:            u32,
    server_time_offset_ms: i64,
    last_ping_ms:         u64,
    ping_timer_s:         f32,
    connected:            bool,
    xp:                   u64,
    level:                u32,
    inventory:            Vec<shared::InventorySlot>,
    class:                shared::PlayerClass,
    stats:                shared::PlayerStats,
    equipment:            shared::Equipment,
    mp_current:           i32,
    stamina_current:      i32,

    // Loja aberta quando shop_items = Some
    shop_items:           Option<Vec<(u16, u32)>>,

    // Chat log (display-only): ultimas mensagens recebidas
    chat_log: std::collections::VecDeque<String>,
    chat_typing: bool,
    chat_input_buf: String,

    // Debug
    show_debug: bool,
}

const CHAT_LOG_MAX: usize = 6;
const PING_INTERVAL_S: f32 = 1.0;

impl MmoClient {
    pub fn new(server_url: String, username: String, password: String) -> Self {
        Self {
            server_url,
            username,
            password,
            net: None,
            tilemap: None,
            world_map: None,
            anim_registry: AnimRegistry::new(),
            anim_players: HashMap::new(),
            entity_names: HashMap::new(),
            font: None,
            interp: InterpolationBuffer::new(),
            visible_entities: Vec::new(),
            self_entity: None,
            prediction: None,
            input_seq: 0,
            server_time_offset_ms: 0,
            last_ping_ms: 0,
            ping_timer_s: 0.0,
            connected: false,
            xp: 0,
            level: 0,
            inventory: vec![shared::InventorySlot::default(); shared::INVENTORY_SLOTS],
            class: shared::PlayerClass::Warrior,
            stats: shared::PlayerClass::Warrior.base_stats(),
            equipment: shared::Equipment::default(),
            mp_current: 0,
            stamina_current: shared::STAMINA_MAX,
            shop_items: None,
            chat_log: std::collections::VecDeque::with_capacity(CHAT_LOG_MAX),
            chat_typing: false,
            chat_input_buf: String::new(),
            show_debug: false,
        }
    }

}

impl Game for MmoClient {
    fn init(&mut self, ctx: &mut AppContext, renderer: &mut Renderer) {
        // 1. Gerar e fazer upload do atlas
        let (pixels, layout) = atlas_gen::generate();
        renderer.set_atlas(&pixels, atlas_gen::ATLAS_W, atlas_gen::ATLAS_H)
            .expect("set_atlas");
        tracing::info!("atlas {}x{} carregado", atlas_gen::ATLAS_W, atlas_gen::ATLAS_H);

        // 2. Registrar clips de animacao
        register_anims(&mut self.anim_registry, &layout);

        // 3. Fonte bitmap
        self.font = Some(BitmapFont::from_grid(
            atlas_gen::ATLAS_W,
            atlas_gen::ATLAS_H,
            layout.font_origin_x,
            layout.font_origin_y,
            layout.font_char_px_w,
            layout.font_char_px_h,
            layout.font_cols,
            Vec2::new(0.35, 0.35), // tamanho em tiles de mundo
        ));

        // 4. Gerar tilemap procedural
        let world = shared::world_gen::generate(42, 128, 128);
        let mut map = Tilemap::new(world.width, world.height, 1.0);

        // Adicionar TileDefs na ordem dos tile_id (1=FLOOR, 2=WALL...)
        for uv in layout.tile_uvs.iter().skip(1) {
            let solid = map.tile_defs.len() == 2; // id=2 e parede
            map.add_tile_def(TileDef { uv_min: uv.0, uv_max: uv.1, solid });
        }

        // Copiar tiles do world gen para o tilemap da engine
        for y in 0..world.height as i32 {
            for x in 0..world.width as i32 {
                map.set(x, y, world.get(x, y));
            }
        }

        // Posicionar camera no ponto de spawn
        let (sx, sy) = world.spawn_tile();
        ctx.camera.position = Vec2::new(sx as f32 + 0.5, sy as f32 + 0.5);

        self.tilemap = Some(map);
        self.world_map = Some(world);

        // 5. Conectar ao servidor
        tracing::info!("conectando em {}", self.server_url);
        match net_client::connect(self.server_url.clone()) {
            Ok(n) => {
                n.send(ClientMessage::Handshake {
                    protocol_version: shared::PROTOCOL_VERSION,
                    client_version: env!("CARGO_PKG_VERSION").to_string(),
                });
                n.send(ClientMessage::Login {
                    username: self.username.clone(),
                    password: self.password.clone(),
                });
                self.net = Some(n);
            }
            Err(e) => tracing::error!("connect: {e}"),
        }
    }

    fn update(&mut self, ctx: &mut AppContext, dt: f32) {
        // --- Processar mensagens do servidor ---
        if let Some(net) = &mut self.net {
            while let Ok(msg) = net.incoming.try_recv() {
                match msg {
                    ServerMessage::HandshakeAck { server_time_ms, .. } => {
                        let local_ms = now_ms() as i64;
                        self.server_time_offset_ms = server_time_ms as i64 - local_ms;
                    }
                    ServerMessage::InventoryUpdate { slots } => {
                        self.inventory = slots;
                    }
                    ServerMessage::StatsUpdate { class, stats, equipment } => {
                        self.class = class;
                        self.stats = stats;
                        self.equipment = equipment;
                    }
                    ServerMessage::ManaUpdate { current } => {
                        self.mp_current = current;
                    }
                    ServerMessage::StaminaUpdate { current } => {
                        self.stamina_current = current;
                    }
                    ServerMessage::ShopOpen { items } => {
                        self.shop_items = Some(items);
                    }
                    ServerMessage::ShopClose => {
                        self.shop_items = None;
                    }
                    ServerMessage::ProgressUpdate { xp, level } => {
                        let leveled_up = level > self.level && self.level > 0;
                        self.xp = xp;
                        self.level = level;
                        if leveled_up {
                            if self.chat_log.len() == CHAT_LOG_MAX {
                                self.chat_log.pop_front();
                            }
                            self.chat_log.push_back(format!("LEVEL UP! L{level}"));
                        }
                    }
                    ServerMessage::Pong { client_time_ms, server_time_ms } => {
                        // RTT = agora - quando enviamos o Ping
                        let rtt = now_ms().saturating_sub(client_time_ms);
                        self.last_ping_ms = rtt;
                        // Reestima offset assumindo latência simétrica
                        let one_way = (rtt / 2) as i64;
                        self.server_time_offset_ms =
                            server_time_ms as i64 - (client_time_ms as i64 + one_way);
                    }
                    ServerMessage::LoginOk { entity_id, spawn, .. } => {
                        tracing::info!("login ok — {:?} @ {spawn}", entity_id);
                        self.self_entity = Some(entity_id);
                        if let Some(map) = &self.world_map {
                            self.prediction = Some(PredictionBuffer::new(spawn, map));
                        }
                        ctx.camera.position = spawn;
                        self.entity_names.insert(entity_id, self.username.clone());
                        self.connected = true;
                    }
                    ServerMessage::LoginDenied { reason } => {
                        tracing::error!("login negado: {reason}");
                        ctx.should_exit = true;
                    }
                    ServerMessage::Snapshot(snap) => {
                        // Reconciliacao do proprio jogador
                        if let (Some(sid), Some(pred), Some(map)) =
                            (self.self_entity, &mut self.prediction, &self.world_map)
                        {
                            if let Some(e) = snap.entities.iter().find(|e| e.id == sid) {
                                pred.reconcile(e.pos, snap.last_input_seq, map);
                            }
                        }
                        // Nomes de jogadores que chegam no snapshot
                        for e in &snap.entities {
                            if let Some(name) = &e.name {
                                self.entity_names.insert(e.id, name.clone());
                            }
                        }
                        self.interp.push(snap.server_time_ms, snap.entities);
                    }
                    ServerMessage::Chat { from, text } => {
                        tracing::info!("[chat] {from}: {text}");
                        if self.chat_log.len() == CHAT_LOG_MAX {
                            self.chat_log.pop_front();
                        }
                        self.chat_log.push_back(format!("{from}: {text}"));
                    }
                    ServerMessage::Kick { reason } => {
                        tracing::warn!("kicked: {reason}");
                        ctx.should_exit = true;
                    }
                }
            }
        }

        // --- Ping periódico para medir RTT ---
        self.ping_timer_s += dt;
        if self.connected && self.ping_timer_s >= PING_INTERVAL_S {
            self.ping_timer_s = 0.0;
            if let Some(net) = &self.net {
                net.send(ClientMessage::Ping { client_time_ms: now_ms() });
            }
        }

        // --- Chat: entrar/sair do modo digitando ---
        if self.connected && !self.chat_typing && ctx.input.key_pressed(KeyCode::KeyT) {
            self.chat_typing = true;
            self.chat_input_buf.clear();
            // Consome qualquer texto que o KeyT gerou no mesmo frame
            let _ = ctx.input.take_text_input();
        }
        if self.chat_typing {
            // Esc cancela sem enviar
            if ctx.input.key_pressed(KeyCode::Escape) {
                self.chat_typing = false;
                self.chat_input_buf.clear();
            } else {
                // Backspace remove ultimo char
                if ctx.input.key_pressed(KeyCode::Backspace) {
                    self.chat_input_buf.pop();
                }
                // Acumula texto do frame, filtrando control chars
                let incoming = ctx.input.take_text_input();
                for ch in incoming.chars() {
                    if !ch.is_control() && self.chat_input_buf.chars().count() < 200 {
                        self.chat_input_buf.push(ch);
                    }
                }
                // Enter envia
                if ctx.input.key_pressed(KeyCode::Enter) {
                    let msg = self.chat_input_buf.trim().to_string();
                    if !msg.is_empty() {
                        if let Some(net) = &self.net {
                            net.send(ClientMessage::Chat(msg));
                        }
                    }
                    self.chat_typing = false;
                    self.chat_input_buf.clear();
                }
            }
        } else {
            // Descarta texto digitado fora do modo chat para nao vazar pra
            // proximos frames.
            let _ = ctx.input.take_text_input();
        }

        // --- Gerar e enviar input ---
        if self.connected {
            self.input_seq = self.input_seq.wrapping_add(1);
            let (mv, aim, mut btns) = if self.chat_typing {
                // Em modo digitando, jogador para de se mover/atacar
                (Vec2::ZERO, ctx.camera.position, 0u32)
            } else {
                (
                    ctx.input.move_vector(),
                    ctx.camera.screen_to_world(ctx.input.mouse_pos()),
                    0u32,
                )
            };
            if !self.chat_typing {
                if ctx.input.key_down(KeyCode::Space)         { btns |= buttons::PRIMARY; }
                if ctx.input.key_down(KeyCode::KeyQ)          { btns |= buttons::SECONDARY; }
                if ctx.input.key_down(KeyCode::ShiftLeft)     { btns |= buttons::DASH; }
            }

            let frame = InputFrame { seq: self.input_seq, tick: 0, move_dir: mv, aim, buttons: btns };
            if let (Some(pred), Some(map)) = (&mut self.prediction, &self.world_map) { 
                pred.push_input(frame, map); 
            }
            if let Some(net)  = &self.net            { net.send(ClientMessage::Input(frame)); }
        }

        // --- Amostrar interpolacao ---
        let server_now  = now_ms() as i64 + self.server_time_offset_ms;
        let render_time = (server_now as u64).saturating_sub(RENDER_DELAY_MS);
        self.visible_entities = self.interp.sample(render_time);

        // --- Atualizar animacoes ---
        let moving = ctx.input.move_vector().length_squared() > 0.01;
        let player_clip_name = if moving { ANIM_PLAYER_WALK } else { ANIM_PLAYER_IDLE };
        let player_clip = self.anim_registry.id_of(player_clip_name).unwrap_or(0);
        let enemy_clip  = self.anim_registry.id_of(ANIM_ENEMY_WALK).unwrap_or(0);

        for e in &self.visible_entities {
            let clip = match e.kind {
                EntityKind::Player    => player_clip,
                EntityKind::Enemy(_)  => enemy_clip,
                _                     => 0,
            };
            let is_self = Some(e.id) == self.self_entity;
            let vel = if is_self {
                ctx.input.move_vector()
            } else {
                e.vel
            };
            let anim = self.anim_players.entry(e.id)
                .or_insert_with(|| AnimPlayer::new(clip));
            anim.set_clip(clip);
            anim.flip_x = vel.x < -0.01;
            anim.update(dt, &self.anim_registry);
        }

        // Remove AnimPlayers de entidades que saíram do AOI
        let visible_ids: std::collections::HashSet<EntityId> =
            self.visible_entities.iter().map(|e| e.id).collect();
        self.anim_players.retain(|id, _| visible_ids.contains(id));

        // --- Camera smooth-follow ---
        if let Some(pred) = &mut self.prediction {
            let smooth = pred.smooth_position();
            let t = (dt * 10.0).clamp(0.0, 1.0);
            ctx.camera.position = ctx.camera.position.lerp(smooth, t);
        }

        // --- Atalhos ---
        // Esc so fecha o jogo se NAO estiver digitando chat e NAO tiver loja
        // aberta — nesses casos o Esc e consumido como "cancelar".
        if !self.chat_typing && self.shop_items.is_none() {
            if ctx.input.key_pressed(KeyCode::F3)    { self.show_debug = !self.show_debug; }
            if ctx.input.key_pressed(KeyCode::Escape) { ctx.should_exit = true; }
        }

        // Interacao com NPC (E)
        if self.connected && !self.chat_typing && ctx.input.key_pressed(KeyCode::KeyE) {
            if let Some(net) = &self.net {
                net.send(ClientMessage::Interact);
            }
        }
        // Fecha loja com Esc (client-side — nao precisa do server)
        if self.shop_items.is_some() && ctx.input.key_pressed(KeyCode::Escape) {
            self.shop_items = None;
        }

        // Teclas 1..9: se loja aberta, compra slot N-1; senao usa item N-1.
        if self.connected && !self.chat_typing {
            if let Some(net) = &self.net {
                let digits = [
                    KeyCode::Digit1, KeyCode::Digit2, KeyCode::Digit3,
                    KeyCode::Digit4, KeyCode::Digit5, KeyCode::Digit6,
                    KeyCode::Digit7, KeyCode::Digit8, KeyCode::Digit9,
                ];
                for (i, code) in digits.iter().enumerate() {
                    if ctx.input.key_pressed(*code) {
                        if let Some(items) = &self.shop_items {
                            if i < items.len() {
                                net.send(ClientMessage::ShopBuy { slot_idx: i as u8 });
                            }
                        } else {
                            net.send(ClientMessage::UseItem { slot: i as u16 });
                        }
                    }
                }
            }
        }
    }

    fn render(&mut self, ctx: &mut AppContext, batch: &mut SpriteBatch, _alpha: f32) {
        // 1. Tilemap
        if let Some(map) = &self.tilemap {
            map.fill_batch(&ctx.camera, batch);
        }

        // 2. Entidades
        let self_id = self.self_entity;
        let reg = &self.anim_registry;

        for e in &self.visible_entities {
            let pos = if Some(e.id) == self_id {
                self.prediction.as_ref().map(|p| p.predicted_pos).unwrap_or(e.pos)
            } else {
                e.pos
            };

            // Projeteis: ponto amarelo pequeno, sem sombra ou nome
            if matches!(e.kind, EntityKind::Projectile) {
                batch.push(&Sprite {
                    position: pos,
                    size: Vec2::splat(0.3),
                    uv_min: Vec2::ZERO,
                    uv_max: Vec2::splat(0.004),
                    tint: Vec4::new(1.0, 0.9, 0.2, 1.0),
                    depth: layer::ENTITY - pos.y,
                    ..Default::default()
                });
                continue;
            }

            let (uv_min, uv_max) = self.anim_players
                .get(&e.id)
                .map(|ap| ap.uvs(reg))
                .unwrap_or((Vec2::ZERO, Vec2::ONE));

            let tint = match e.kind {
                EntityKind::Player      => Vec4::ONE,
                EntityKind::Enemy(k)    => {
                    let [r, g, b, a] = shared::enemy_def(k).tint_rgba;
                    Vec4::new(r, g, b, a)
                }
                EntityKind::Loot(iid)   => match iid {
                    shared::item_id::GOLD          => Vec4::new(1.0, 0.85, 0.2, 1.0),
                    shared::item_id::HEALTH_POTION => Vec4::new(0.9, 0.3, 0.35, 1.0),
                    shared::item_id::SWORD         => Vec4::new(0.8, 0.85, 0.95, 1.0),
                    shared::item_id::ARMOR         => Vec4::new(0.6, 0.6, 0.7, 1.0),
                    shared::item_id::RING          => Vec4::new(1.0, 0.7, 0.9, 1.0),
                    _                              => Vec4::new(0.6, 0.9, 1.0, 1.0),
                },
                EntityKind::Npc(_)      => Vec4::new(0.95, 0.8, 0.4, 1.0),
                _                       => Vec4::ONE,
            };

            let base_size = match e.kind {
                EntityKind::Enemy(k) => 0.95 * shared::enemy_size_scale(k),
                _ => 0.95,
            };
            batch.push(&Sprite {
                position: pos,
                size: Vec2::splat(base_size),
                uv_min,
                uv_max,
                tint,
                depth: layer::ENTITY - pos.y,
                ..Default::default()
            });

            // Sombra circular no chao (atras da entidade)
            batch.push(&Sprite {
                position: pos - Vec2::Y * 0.38,
                size: Vec2::new(0.5, 0.12),
                uv_min: Vec2::ZERO,
                uv_max: Vec2::splat(0.004),
                tint: Vec4::new(0.0, 0.0, 0.0, 0.35),
                depth: layer::SHADOW - pos.y,
                ..Default::default()
            });

            // Barra de HP
            if let Some(hp) = e.hp {
                let fill = (hp.current as f32 / hp.max as f32).clamp(0.0, 1.0);
                let bar_w = 0.75f32;
                let bar_h = 0.07f32;
                let bar_pos = pos + Vec2::new(0.0, 0.58);
                // fundo
                batch.push(&Sprite {
                    position: bar_pos,
                    size: Vec2::new(bar_w, bar_h),
                    uv_min: Vec2::ZERO, uv_max: Vec2::splat(0.004),
                    tint: Vec4::new(0.2, 0.05, 0.05, 0.85),
                    depth: layer::HP_BAR - pos.y,
                    ..Default::default()
                });
                // preenchimento
                if fill > 0.0 {
                    let fill_color = Vec4::new(1.0 - fill * 0.8, fill * 0.85, 0.1, 0.9);
                    batch.push(&Sprite {
                        position: bar_pos + Vec2::new((fill - 1.0) * bar_w * 0.5, 0.0),
                        size: Vec2::new(bar_w * fill, bar_h),
                        uv_min: Vec2::ZERO, uv_max: Vec2::splat(0.004),
                        tint: fill_color,
                        depth: layer::HP_BAR - pos.y + 0.1,
                        ..Default::default()
                    });
                }
            }

            // Nome acima do sprite
            if let Some(font) = &self.font {
                let (maybe_label, color) = match e.kind {
                    EntityKind::Player => {
                        let n = self.entity_names.get(&e.id).cloned();
                        let c = if Some(e.id) == self_id {
                            Vec4::new(1.0, 1.0, 0.4, 1.0)
                        } else {
                            Vec4::new(0.9, 0.9, 0.9, 1.0)
                        };
                        (n, c)
                    }
                    EntityKind::Npc(_) => (
                        Some("SHOP [E]".into()),
                        Vec4::new(0.95, 0.85, 0.45, 1.0),
                    ),
                    _ => (None, Vec4::ONE),
                };
                if let Some(label) = maybe_label {
                    let label_w = font.measure_width(&label) * 0.35;
                    let label_pos = pos + Vec2::new(-label_w * 0.5, 0.68);
                    font.draw_depth(&label, label_pos, 1.0, color, layer::NAMEPLATE - pos.y, batch);
                }
            }
        }

        // 3. HUD (posicionado no canto superior-esquerdo em espaco de mundo)
        if let Some(font) = &self.font {
            let vis = ctx.camera.visible_rect();
            let margin = 0.3;
            let top_left = Vec2::new(vis.min.x + margin, vis.max.y - margin);
            let bottom_left = Vec2::new(vis.min.x + margin, vis.min.y + 1.2);

            // Coordenadas, ping e dicas no topo
            let class_name = match self.class {
                shared::PlayerClass::Warrior => "WAR",
                shared::PlayerClass::Archer  => "ARC",
                shared::PlayerClass::Wizard  => "WIZ",
            };
            let pos_text = format!(
                "[{class_name}] dmg={}  ({:.0}, {:.0})  ping={}ms  F3=debug  ESC=sair",
                self.stats.attack_damage, ctx.camera.position.x, ctx.camera.position.y, self.last_ping_ms
            );
            font.draw_depth(&pos_text, top_left, 0.85, Vec4::new(0.8, 0.8, 0.8, 1.0), layer::HUD, batch);

            // Chat log (display-only) sobreposto acima do status
            if !self.chat_log.is_empty() {
                let line_h = 0.35;
                let base = Vec2::new(vis.min.x + margin, vis.min.y + 1.2 + line_h);
                for (i, msg) in self.chat_log.iter().rev().enumerate() {
                    let y = base.y + i as f32 * line_h;
                    font.draw_depth(
                        msg,
                        Vec2::new(base.x, y),
                        0.75,
                        Vec4::new(0.85, 0.9, 1.0, 0.9),
                        layer::HUD,
                        batch,
                    );
                }
            }

            // Painel da loja (overlay central)
            if let Some(items) = self.shop_items.clone() {
                let panel_w = 6.5f32;
                let line_h = 0.5;
                let panel_h = 0.8 + items.len() as f32 * line_h + 0.4;
                let cx = vis.min.x + (vis.max.x - vis.min.x) / 2.0;
                let cy = vis.min.y + (vis.max.y - vis.min.y) / 2.0;
                batch.push(&Sprite {
                    position: Vec2::new(cx, cy),
                    size: Vec2::new(panel_w, panel_h),
                    uv_min: Vec2::ZERO, uv_max: Vec2::splat(0.004),
                    tint: Vec4::new(0.05, 0.06, 0.10, 0.95),
                    depth: layer::HUD + 1.0,
                    ..Default::default()
                });
                // Borda
                batch.push(&Sprite {
                    position: Vec2::new(cx, cy + panel_h * 0.5 - 0.03),
                    size: Vec2::new(panel_w, 0.06),
                    uv_min: Vec2::ZERO, uv_max: Vec2::splat(0.004),
                    tint: Vec4::new(1.0, 0.85, 0.35, 0.9),
                    depth: layer::HUD + 1.1,
                    ..Default::default()
                });
                font.draw_depth(
                    "LOJA — ESC fecha",
                    Vec2::new(cx - panel_w * 0.5 + 0.3, cy + panel_h * 0.5 - 0.25),
                    0.85,
                    Vec4::new(1.0, 0.85, 0.35, 1.0),
                    layer::HUD + 1.2,
                    batch,
                );
                for (i, (iid, price)) in items.iter().enumerate() {
                    let iname = match *iid {
                        shared::item_id::HEALTH_POTION => "Pocao de Vida",
                        shared::item_id::SWORD         => "Espada",
                        shared::item_id::ARMOR         => "Armadura",
                        shared::item_id::RING          => "Anel",
                        _                              => "Item",
                    };
                    let row = format!("[{}] {:<18} {} ouro", i + 1, iname, price);
                    let y = cy + panel_h * 0.5 - 0.8 - i as f32 * line_h;
                    font.draw_depth(
                        &row,
                        Vec2::new(cx - panel_w * 0.5 + 0.3, y),
                        0.75,
                        Vec4::new(0.9, 0.95, 1.0, 1.0),
                        layer::HUD + 1.2,
                        batch,
                    );
                }
            }

            // Campo de input de chat (aparece quando typing)
            if self.chat_typing {
                let box_w = (vis.max.x - vis.min.x) * 0.55;
                let box_h = 0.45;
                let box_pos = Vec2::new(
                    vis.min.x + margin + box_w * 0.5,
                    vis.min.y + margin + 0.25,
                );
                batch.push(&Sprite {
                    position: box_pos,
                    size: Vec2::new(box_w, box_h),
                    uv_min: Vec2::ZERO, uv_max: Vec2::splat(0.004),
                    tint: Vec4::new(0.05, 0.07, 0.12, 0.95),
                    depth: layer::HUD,
                    ..Default::default()
                });
                // borda
                batch.push(&Sprite {
                    position: box_pos + Vec2::new(0.0, box_h * 0.5 - 0.02),
                    size: Vec2::new(box_w, 0.04),
                    uv_min: Vec2::ZERO, uv_max: Vec2::splat(0.004),
                    tint: Vec4::new(1.0, 0.85, 0.35, 0.9),
                    depth: layer::HUD + 0.05,
                    ..Default::default()
                });
                let prompt = format!("> {}_", self.chat_input_buf);
                font.draw_depth(
                    &prompt,
                    box_pos + Vec2::new(-box_w * 0.5 + 0.15, 0.12),
                    0.7,
                    Vec4::ONE,
                    layer::HUD + 0.1,
                    batch,
                );
            }

            // Status de conexao em baixo
            let status = if self.connected {
                format!(
                    "{} jogadores  WASD=mover  SHIFT=sprint  SPACE=atk  Q=triple  E=npc  T=chat  1-9=item",
                    self.visible_entities.len()
                )
            } else {
                "Conectando...".to_string()
            };
            font.draw_depth(&status, bottom_left, 0.85, Vec4::new(0.7, 0.8, 0.7, 1.0), layer::HUD, batch);

            // Painel de debug (F3)
            if self.show_debug {
                let dbg = format!(
                    "entities: {}\ninterp_delay: {}ms\nzoom: {:.0}",
                    self.visible_entities.len(),
                    RENDER_DELAY_MS,
                    ctx.camera.zoom,
                );
                font.draw_depth(&dbg, top_left - Vec2::Y * 0.45, 0.8, Vec4::new(0.5, 1.0, 0.5, 1.0), layer::HUD, batch);
            }

            // HUD do jogador (HP bar grande no meio da tela)
            let self_hp = self.visible_entities.iter()
                .find(|e| Some(e.id) == self.self_entity)
                .and_then(|e| e.hp);
                
            if let Some(hp) = self_hp {
                let bar_w = 5.0;
                let bar_h = 0.4;
                let fill = (hp.current as f32 / hp.max as f32).clamp(0.0, 1.0);
                let bar_pos = Vec2::new(vis.min.x + (vis.max.x - vis.min.x) / 2.0, vis.min.y + margin + 0.3);
                
                // Fundo da barra
                batch.push(&Sprite {
                    position: bar_pos,
                    size: Vec2::new(bar_w, bar_h),
                    uv_min: Vec2::ZERO, uv_max: Vec2::splat(0.004),
                    tint: Vec4::new(0.2, 0.05, 0.05, 0.9),
                    depth: layer::HUD,
                    ..Default::default()
                });
                // Preenchimento
                if fill > 0.0 {
                    batch.push(&Sprite {
                        position: bar_pos + Vec2::new((fill - 1.0) * bar_w * 0.5, 0.0),
                        size: Vec2::new(bar_w * fill, bar_h),
                        uv_min: Vec2::ZERO, uv_max: Vec2::splat(0.004),
                        tint: Vec4::new(0.9, 0.2, 0.2, 1.0),
                        depth: layer::HUD + 0.1,
                        ..Default::default()
                    });
                }

                // Texto do HP centralizado
                let hp_label = format!("{}/{}", hp.current, hp.max);
                let label_w = font.measure_width(&hp_label) * 0.8;
                font.draw_depth(&hp_label, bar_pos + Vec2::new(-label_w * 0.5, 0.15), 0.8, Vec4::ONE, layer::HUD + 0.2, batch);

                // Barras finas abaixo da HP (MP + Stamina empilhadas)
                let thin_h = 0.18;
                let thin_gap = 0.05;
                let mut thin_offset = bar_h * 0.5 + 0.06 + thin_h * 0.5;
                // MP
                if self.stats.mp_max > 0 {
                    let mp_pos = bar_pos - Vec2::new(0.0, thin_offset);
                    let mp_fill = (self.mp_current as f32 / self.stats.mp_max as f32).clamp(0.0, 1.0);
                    batch.push(&Sprite {
                        position: mp_pos,
                        size: Vec2::new(bar_w, thin_h),
                        uv_min: Vec2::ZERO, uv_max: Vec2::splat(0.004),
                        tint: Vec4::new(0.05, 0.1, 0.25, 0.9),
                        depth: layer::HUD,
                        ..Default::default()
                    });
                    if mp_fill > 0.0 {
                        batch.push(&Sprite {
                            position: mp_pos + Vec2::new((mp_fill - 1.0) * bar_w * 0.5, 0.0),
                            size: Vec2::new(bar_w * mp_fill, thin_h),
                            uv_min: Vec2::ZERO, uv_max: Vec2::splat(0.004),
                            tint: Vec4::new(0.25, 0.5, 1.0, 0.95),
                            depth: layer::HUD + 0.1,
                            ..Default::default()
                        });
                    }
                    let mp_label = format!("{}/{} MP", self.mp_current, self.stats.mp_max);
                    let mp_lw = font.measure_width(&mp_label) * 0.55;
                    font.draw_depth(
                        &mp_label,
                        mp_pos + Vec2::new(-mp_lw * 0.5, 0.08),
                        0.55,
                        Vec4::new(0.9, 0.95, 1.0, 1.0),
                        layer::HUD + 0.2,
                        batch,
                    );
                    thin_offset += thin_h + thin_gap;
                }
                // Stamina
                {
                    let stam_max = shared::STAMINA_MAX;
                    let stam_pos = bar_pos - Vec2::new(0.0, thin_offset);
                    let stam_fill = (self.stamina_current as f32 / stam_max as f32).clamp(0.0, 1.0);
                    batch.push(&Sprite {
                        position: stam_pos,
                        size: Vec2::new(bar_w, thin_h),
                        uv_min: Vec2::ZERO, uv_max: Vec2::splat(0.004),
                        tint: Vec4::new(0.05, 0.15, 0.05, 0.9),
                        depth: layer::HUD,
                        ..Default::default()
                    });
                    if stam_fill > 0.0 {
                        batch.push(&Sprite {
                            position: stam_pos + Vec2::new((stam_fill - 1.0) * bar_w * 0.5, 0.0),
                            size: Vec2::new(bar_w * stam_fill, thin_h),
                            uv_min: Vec2::ZERO, uv_max: Vec2::splat(0.004),
                            tint: Vec4::new(0.4, 0.9, 0.35, 0.95),
                            depth: layer::HUD + 0.1,
                            ..Default::default()
                        });
                    }
                    let stam_label = format!("{}/{} SP", self.stamina_current, stam_max);
                    let slw = font.measure_width(&stam_label) * 0.55;
                    font.draw_depth(
                        &stam_label,
                        stam_pos + Vec2::new(-slw * 0.5, 0.08),
                        0.55,
                        Vec4::new(0.85, 1.0, 0.85, 1.0),
                        layer::HUD + 0.2,
                        batch,
                    );
                    thin_offset += thin_h + thin_gap;
                }

                // Barra de XP abaixo da MP (ou abaixo da HP se mp_max=0)
                let next = shared::xp_for_level(self.level + 1);
                let cur_floor = shared::xp_for_level(self.level);
                let span = (next - cur_floor).max(1);
                let progress = ((self.xp.saturating_sub(cur_floor)) as f32 / span as f32).clamp(0.0, 1.0);
                // XP sempre fica abaixo das barras finas (MP opcional + Stamina)
                let xp_y_offset = thin_offset + 0.15;
                let xp_bar_pos = bar_pos - Vec2::new(0.0, xp_y_offset);
                let xp_bar_h = 0.18;
                batch.push(&Sprite {
                    position: xp_bar_pos,
                    size: Vec2::new(bar_w, xp_bar_h),
                    uv_min: Vec2::ZERO, uv_max: Vec2::splat(0.004),
                    tint: Vec4::new(0.08, 0.08, 0.12, 0.9),
                    depth: layer::HUD,
                    ..Default::default()
                });
                if progress > 0.0 {
                    batch.push(&Sprite {
                        position: xp_bar_pos + Vec2::new((progress - 1.0) * bar_w * 0.5, 0.0),
                        size: Vec2::new(bar_w * progress, xp_bar_h),
                        uv_min: Vec2::ZERO, uv_max: Vec2::splat(0.004),
                        tint: Vec4::new(0.35, 0.75, 1.0, 0.95),
                        depth: layer::HUD + 0.1,
                        ..Default::default()
                    });
                }
                let xp_label = format!(
                    "L{}  {}/{} XP",
                    self.level,
                    self.xp.saturating_sub(cur_floor),
                    span
                );
                let xp_label_w = font.measure_width(&xp_label) * 0.7;
                font.draw_depth(
                    &xp_label,
                    xp_bar_pos + Vec2::new(-xp_label_w * 0.5, 0.08),
                    0.7,
                    Vec4::new(0.9, 0.95, 1.0, 1.0),
                    layer::HUD + 0.2,
                    batch,
                );
            }

            // Slots de equipamento (arma / armadura / anel) — acima da hotbar
            let eq_slot_size = 0.55f32;
            let eq_gap = 0.12f32;
            let eq_slots: [(Option<u16>, &str); 3] = [
                (self.equipment.weapon, "W"),
                (self.equipment.armor,  "A"),
                (self.equipment.ring,   "R"),
            ];
            let eq_total_w = 3.0 * eq_slot_size + 2.0 * eq_gap;
            let eq_center_x = vis.min.x + (vis.max.x - vis.min.x) / 2.0;
            let eq_y = vis.min.y + margin + 1.7;
            for (i, (item_opt, label)) in eq_slots.iter().enumerate() {
                let x = eq_center_x - eq_total_w * 0.5 + eq_slot_size * 0.5
                    + i as f32 * (eq_slot_size + eq_gap);
                let pos = Vec2::new(x, eq_y);
                let bg = if item_opt.is_some() {
                    Vec4::new(0.20, 0.18, 0.10, 0.95)
                } else {
                    Vec4::new(0.10, 0.10, 0.14, 0.70)
                };
                batch.push(&Sprite {
                    position: pos,
                    size: Vec2::splat(eq_slot_size),
                    uv_min: Vec2::ZERO, uv_max: Vec2::splat(0.004),
                    tint: bg,
                    depth: layer::HUD,
                    ..Default::default()
                });
                if let Some(iid) = item_opt {
                    let tint = match *iid {
                        shared::item_id::SWORD => Vec4::new(0.8, 0.85, 0.95, 1.0),
                        shared::item_id::ARMOR => Vec4::new(0.6, 0.6, 0.7, 1.0),
                        shared::item_id::RING  => Vec4::new(1.0, 0.7, 0.9, 1.0),
                        _                      => Vec4::ONE,
                    };
                    batch.push(&Sprite {
                        position: pos,
                        size: Vec2::splat(eq_slot_size * 0.7),
                        uv_min: Vec2::ZERO, uv_max: Vec2::splat(0.004),
                        tint,
                        depth: layer::HUD + 0.1,
                        ..Default::default()
                    });
                }
                // Rotulo pequeno abaixo do slot (W/A/R)
                font.draw_depth(
                    label,
                    pos + Vec2::new(-0.08, -eq_slot_size * 0.55),
                    0.6,
                    Vec4::new(0.7, 0.7, 0.75, 0.8),
                    layer::HUD + 0.2,
                    batch,
                );
            }

            // Hotbar do inventario — 24 slots em linha na base da tela
            let slot_size = 0.45f32;
            let slot_gap = 0.06f32;
            let cols = shared::INVENTORY_SLOTS as f32;
            let total_w = cols * slot_size + (cols - 1.0) * slot_gap;
            let center_x = vis.min.x + (vis.max.x - vis.min.x) / 2.0;
            let row_y = vis.min.y + margin + 0.9;
            for (i, slot) in self.inventory.iter().enumerate() {
                let x = center_x - total_w * 0.5 + slot_size * 0.5
                    + i as f32 * (slot_size + slot_gap);
                let pos = Vec2::new(x, row_y);
                let bg = if slot.qty == 0 {
                    Vec4::new(0.08, 0.09, 0.14, 0.75)
                } else {
                    Vec4::new(0.14, 0.16, 0.22, 0.9)
                };
                batch.push(&Sprite {
                    position: pos,
                    size: Vec2::splat(slot_size),
                    uv_min: Vec2::ZERO, uv_max: Vec2::splat(0.004),
                    tint: bg,
                    depth: layer::HUD,
                    ..Default::default()
                });
                if slot.qty > 0 {
                    let icon_tint = match slot.item_id {
                        shared::item_id::GOLD          => Vec4::new(1.0, 0.85, 0.2, 1.0),
                        shared::item_id::HEALTH_POTION => Vec4::new(0.9, 0.3, 0.35, 1.0),
                        _                              => Vec4::new(0.7, 0.8, 0.9, 1.0),
                    };
                    batch.push(&Sprite {
                        position: pos,
                        size: Vec2::splat(slot_size * 0.65),
                        uv_min: Vec2::ZERO, uv_max: Vec2::splat(0.004),
                        tint: icon_tint,
                        depth: layer::HUD + 0.1,
                        ..Default::default()
                    });
                    if slot.qty > 1 {
                        let label = if slot.qty < 1000 {
                            format!("{}", slot.qty)
                        } else {
                            format!("{}k", slot.qty / 1000)
                        };
                        let lw = font.measure_width(&label) * 0.5;
                        font.draw_depth(
                            &label,
                            pos + Vec2::new(slot_size * 0.5 - lw - 0.02, -slot_size * 0.5 + 0.18),
                            0.5,
                            Vec4::ONE,
                            layer::HUD + 0.2,
                            batch,
                        );
                    }
                }
            }
        }
    }

    fn shutdown(&mut self, _ctx: &mut AppContext) {
        if let Some(net) = &self.net {
            net.send(ClientMessage::RequestDisconnect);
        }
    }
}

// ---------------------------------------------------------------------------
// Registro de animacoes
// ---------------------------------------------------------------------------

fn register_anims(reg: &mut AnimRegistry, layout: &AtlasLayout) {
    // Player idle: frame 0 apenas (sem movimento perceptivel)
    let idle_frames = vec![engine::anim::AnimFrame::uniform(
        layout.player_uvs[0].0,
        layout.player_uvs[0].1,
        4.0,
    )];
    reg.add(ANIM_PLAYER_IDLE, AnimClip::new(idle_frames, true));

    // Player walk: frames 0-3 a 8 fps
    let walk_frames: Vec<_> = (0..4)
        .map(|i| engine::anim::AnimFrame::uniform(
            layout.player_uvs[i].0,
            layout.player_uvs[i].1,
            8.0,
        ))
        .collect();
    reg.add(ANIM_PLAYER_WALK, AnimClip::new(walk_frames, true));

    // Enemy walk: frames 0-3 a 6 fps
    let enemy_frames: Vec<_> = (0..4)
        .map(|i| engine::anim::AnimFrame::uniform(
            layout.enemy_uvs[i].0,
            layout.enemy_uvs[i].1,
            6.0,
        ))
        .collect();
    reg.add(ANIM_ENEMY_WALK, AnimClip::new(enemy_frames, true));
}
