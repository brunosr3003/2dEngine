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
mod world_gen;

use atlas_gen::AtlasLayout;
use engine::anim::{AnimClip, AnimPlayer, AnimRegistry};
use engine::app::{run, AppConfig, AppContext, Game};
use engine::glam::{Vec2, Vec4};
use engine::render::{BitmapFont, Renderer, Sprite, SpriteBatch};
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
    let username = std::env::var("USERNAME")
        .unwrap_or_else(|_| format!("Guest{}", std::process::id() & 0xFFFF));

    run(
        AppConfig { title: "MMORPG 2D — Fase 2", tick_hz: 30, initial_size: (1280, 720) },
        MmoClient::new(server_url, username),
    )
}

// ---------------------------------------------------------------------------

pub struct MmoClient {
    server_url: String,
    username:   String,
    net:        Option<NetClient>,

    // Mundo
    tilemap:        Option<Tilemap>,
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
    connected:            bool,

    // Debug
    show_debug: bool,
}

impl MmoClient {
    pub fn new(server_url: String, username: String) -> Self {
        Self {
            server_url,
            username,
            net: None,
            tilemap: None,
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
            connected: false,
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
        let world = world_gen::generate(42, 80, 80);
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
                    token: "dev".into(),
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
                        self.last_ping_ms = (now_ms() as i64 - local_ms).unsigned_abs();
                    }
                    ServerMessage::LoginOk { entity_id, spawn, .. } => {
                        tracing::info!("login ok — {:?} @ {spawn}", entity_id);
                        self.self_entity = Some(entity_id);
                        self.prediction  = Some(PredictionBuffer::new(spawn));
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
                        if let (Some(sid), Some(pred)) =
                            (self.self_entity, &mut self.prediction)
                        {
                            if let Some(e) = snap.entities.iter().find(|e| e.id == sid) {
                                pred.reconcile(e.pos, snap.last_input_seq);
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
                    }
                    ServerMessage::Kick { reason } => {
                        tracing::warn!("kicked: {reason}");
                        ctx.should_exit = true;
                    }
                }
            }
        }

        // --- Gerar e enviar input ---
        if self.connected {
            self.input_seq = self.input_seq.wrapping_add(1);
            let mv  = ctx.input.move_vector();
            let aim = ctx.camera.screen_to_world(ctx.input.mouse_pos());
            let mut btns = 0u32;
            if ctx.input.key_down(KeyCode::Space)          { btns |= buttons::PRIMARY; }
            if ctx.input.key_pressed(KeyCode::ShiftLeft)  { btns |= buttons::DASH; }

            let frame = InputFrame { seq: self.input_seq, tick: 0, move_dir: mv, aim, buttons: btns };
            if let Some(pred) = &mut self.prediction { pred.push_input(frame); }
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
        if ctx.input.key_pressed(KeyCode::F3)    { self.show_debug = !self.show_debug; }
        if ctx.input.key_pressed(KeyCode::Escape) { ctx.should_exit = true; }
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
                    ..Default::default()
                });
                continue;
            }

            let (uv_min, uv_max) = self.anim_players
                .get(&e.id)
                .map(|ap| ap.uvs(reg))
                .unwrap_or((Vec2::ZERO, Vec2::ONE));

            let tint = match e.kind {
                EntityKind::Player   => Vec4::ONE,
                EntityKind::Enemy(_) => Vec4::ONE,
                EntityKind::Loot     => Vec4::new(0.6, 0.9, 1.0, 1.0),
                _                    => Vec4::ONE,
            };

            batch.push(&Sprite {
                position: pos,
                size: Vec2::splat(0.95),
                uv_min,
                uv_max,
                tint,
                ..Default::default()
            });

            // Sombra circular no chao
            batch.push(&Sprite {
                position: pos - Vec2::Y * 0.38,
                size: Vec2::new(0.5, 0.12),
                uv_min: Vec2::ZERO,
                uv_max: Vec2::splat(0.004),
                tint: Vec4::new(0.0, 0.0, 0.0, 0.35),
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
                        ..Default::default()
                    });
                }
            }

            // Nome acima do sprite
            if let Some(font) = &self.font {
                if matches!(e.kind, EntityKind::Player) {
                    let name = self.entity_names.get(&e.id)
                        .map(|s| s.as_str())
                        .unwrap_or("?");
                    let name_w = font.measure_width(name) * 0.35;
                    let name_pos = pos + Vec2::new(-name_w * 0.5, 0.68);
                    let color = if Some(e.id) == self_id {
                        Vec4::new(1.0, 1.0, 0.4, 1.0)
                    } else {
                        Vec4::new(0.9, 0.9, 0.9, 1.0)
                    };
                    font.draw(name, name_pos, 1.0, color, batch);
                }
            }
        }

        // 3. HUD (posicionado no canto superior-esquerdo em espaco de mundo)
        if let Some(font) = &self.font {
            let vis = ctx.camera.visible_rect();
            let margin = 0.3;
            let top_left = Vec2::new(vis.min.x + margin, vis.max.y - margin);
            let bottom_left = Vec2::new(vis.min.x + margin, vis.min.y + 1.2);

            // Coordenadas e dicas no topo
            let pos_text = format!(
                "({:.0}, {:.0})  F3=debug  ESC=sair",
                ctx.camera.position.x, ctx.camera.position.y
            );
            font.draw(&pos_text, top_left, 0.85, Vec4::new(0.8, 0.8, 0.8, 1.0), batch);

            // Status de conexao em baixo
            let status = if self.connected {
                format!("{} jogadores  WASD=mover  SPACE=atacar", self.visible_entities.len())
            } else {
                "Conectando...".to_string()
            };
            font.draw(&status, bottom_left, 0.85, Vec4::new(0.7, 0.8, 0.7, 1.0), batch);

            // Painel de debug (F3)
            if self.show_debug {
                let dbg = format!(
                    "entities: {}\ninterp_delay: {}ms\nzoom: {:.0}",
                    self.visible_entities.len(),
                    RENDER_DELAY_MS,
                    ctx.camera.zoom,
                );
                font.draw(&dbg, top_left - Vec2::Y * 0.45, 0.8, Vec4::new(0.5, 1.0, 0.5, 1.0), batch);
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
