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

    // HP + posicao do tick anterior — pra detectar dano e spawnar numero
    // (mesmo quando a entidade morre no mesmo tick e some do AOI).
    last_hp: HashMap<EntityId, i32>,
    last_pos: HashMap<EntityId, Vec2>,
    damage_numbers: Vec<DamageNumber>,

    // Controle on-screen (joystick + botoes). Toggle com F2; ligado por
    // padrao nas plataformas touch e testavel com mouse-drag no desktop.
    touch_mode: bool,
    joystick: VirtualJoystick,

    // Settings persistidos em .mmo2d-settings.txt
    hud_scale: f32,
    camera_zoom: f32,
    menu_state: MenuState,

    // Chat log (display-only): ultimas mensagens recebidas
    chat_log: std::collections::VecDeque<String>,
    chat_typing: bool,
    chat_input_buf: String,

    // Debug
    show_debug: bool,

    // Tela de login
    app_state:       AppState,
    login_username:  String,
    login_password:  String,
    login_field:     LoginField,
    login_error:     Option<String>,
    login_submitted: bool,
    login_remember:  bool,
}

const CHAT_LOG_MAX: usize = 6;
const PING_INTERVAL_S: f32 = 1.0;
const DAMAGE_NUM_TTL: f32 = 0.9;
const SETTINGS_FILE: &str = ".mmo2d-settings.txt";
const CREDS_FILE: &str = ".mmo2d-creds.txt";

fn load_saved_creds() -> Option<(String, String)> {
    let txt = std::fs::read_to_string(CREDS_FILE).ok()?;
    let mut user = String::new();
    let mut pass = String::new();
    for line in txt.lines() {
        if let Some(v) = line.strip_prefix("u=") { user = v.to_string(); }
        if let Some(v) = line.strip_prefix("p=") { pass = v.to_string(); }
    }
    if user.is_empty() { None } else { Some((user, pass)) }
}

fn save_creds(user: &str, pass: &str) {
    let _ = std::fs::write(CREDS_FILE, format!("u={}\np={}\n", user, pass));
}

fn clear_creds() {
    let _ = std::fs::remove_file(CREDS_FILE);
}

#[derive(Clone, Copy)]
struct Settings {
    hud_scale: f32,
    touch_mode: bool,
    camera_zoom: f32,
}

impl Default for Settings {
    fn default() -> Self {
        Self { hud_scale: 2.0, touch_mode: false, camera_zoom: 32.0 }
    }
}

fn load_settings() -> Settings {
    let mut s = Settings::default();
    if let Ok(txt) = std::fs::read_to_string(SETTINGS_FILE) {
        for line in txt.lines() {
            let Some((k, v)) = line.split_once('=') else { continue };
            match k.trim() {
                "hud_scale"   => if let Ok(v) = v.trim().parse::<f32>() {
                    s.hud_scale = v.clamp(0.8, 5.0);
                },
                "touch_mode"  => s.touch_mode = v.trim() == "true",
                "camera_zoom" => if let Ok(v) = v.trim().parse::<f32>() {
                    s.camera_zoom = v.clamp(12.0, 80.0);
                },
                _ => {}
            }
        }
    }
    s
}

fn save_settings(s: &Settings) {
    let body = format!(
        "hud_scale={:.2}\ntouch_mode={}\ncamera_zoom={:.1}\n",
        s.hud_scale, s.touch_mode, s.camera_zoom
    );
    if let Err(e) = std::fs::write(SETTINGS_FILE, body) {
        tracing::warn!("falhou salvar settings: {e}");
    }
}

#[derive(Clone, Copy, PartialEq)]
enum MenuState {
    Closed,
    Main,
    Settings,
}

#[derive(Clone, Copy, PartialEq)]
enum AppState {
    Login,
    Playing,
}

#[derive(Clone, Copy, PartialEq)]
enum LoginField {
    Username,
    Password,
}

/// Joystick virtual usado em modo touch/mouse-drag. Desenhado em coordenadas
/// de mundo (via camera.visible_rect) perto do canto inferior-esquerdo.
struct VirtualJoystick {
    center_pixel: Vec2,  // posicao em pixels da tela onde o toque iniciou
    radius: f32,         // raio do base em unidades de mundo
    active: bool,
    thumb_offset: Vec2, // vetor do centro ate o thumb (clampeado)
}

impl VirtualJoystick {
    fn new() -> Self {
        Self {
            center_pixel: Vec2::ZERO,
            radius: 1.6,
            active: false,
            thumb_offset: Vec2::ZERO,
        }
    }

    /// Direcao normalizada [-1..1] no eixo X/Y. Vec2::ZERO quando inativo.
    fn direction(&self) -> Vec2 {
        if !self.active || self.thumb_offset.length_squared() < 0.01 {
            return Vec2::ZERO;
        }
        (self.thumb_offset / self.radius).clamp_length_max(1.0)
    }
}

#[derive(Clone)]
struct DamageNumber {
    world_pos: Vec2,
    amount: i32,
    is_self: bool,
    ttl: f32,
}

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
            last_hp: HashMap::new(),
            last_pos: HashMap::new(),
            damage_numbers: Vec::new(),
            touch_mode: load_settings().touch_mode,
            joystick: VirtualJoystick::new(),
            hud_scale: load_settings().hud_scale,
            camera_zoom: load_settings().camera_zoom,
            menu_state: MenuState::Closed,
            chat_log: std::collections::VecDeque::with_capacity(CHAT_LOG_MAX),
            chat_typing: false,
            chat_input_buf: String::new(),
            show_debug: false,
            app_state: AppState::Login,
            login_username: String::new(),
            login_password: String::new(),
            login_field: LoginField::Username,
            login_error: None,
            login_submitted: false,
            login_remember: false,
        }
    }

    fn do_connect(&mut self) {
        tracing::info!("conectando em {}", self.server_url);
        match net_client::connect(self.server_url.clone()) {
            Ok(n) => {
                n.send(ClientMessage::Handshake {
                    protocol_version: shared::PROTOCOL_VERSION,
                    client_version: env!("CARGO_PKG_VERSION").to_string(),
                });
                n.send(ClientMessage::Login {
                    username: self.login_username.trim().to_string(),
                    password: self.login_password.clone(),
                });
                self.username = self.login_username.trim().to_string();
                self.net = Some(n);
            }
            Err(e) => {
                self.login_error = Some(format!("Erro de conexao: {e}"));
                self.login_submitted = false;
            }
        }
    }

    fn update_login(&mut self, ctx: &mut AppContext) {
        // Tab / click muda campo ativo
        if ctx.input.key_pressed(KeyCode::Tab) {
            self.login_field = match self.login_field {
                LoginField::Username => LoginField::Password,
                LoginField::Password => LoginField::Username,
            };
        }

        // Texto (inclui repeticao de teclas — backspace via \x08, delete via \x7f)
        let incoming = ctx.input.take_text_input();
        for ch in incoming.chars() {
            if ch == '\x08' || ch == '\x7f' {
                // Backspace / Delete — funciona com auto-repeat
                match self.login_field {
                    LoginField::Username => { self.login_username.pop(); }
                    LoginField::Password => { self.login_password.pop(); }
                }
            } else if ch == '\t' || ch == '\r' || ch == '\n' {
                // Ignora tab/enter (tratados por key_pressed acima/abaixo)
            } else if !ch.is_control() {
                match self.login_field {
                    LoginField::Username if self.login_username.len() < 32 => {
                        self.login_username.push(ch);
                    }
                    LoginField::Password if self.login_password.len() < 64 => {
                        self.login_password.push(ch);
                    }
                    _ => {}
                }
            }
        }

        // Enter submete
        if ctx.input.key_pressed(KeyCode::Enter) && !self.login_submitted {
            if self.login_field == LoginField::Username {
                self.login_field = LoginField::Password;
            } else if !self.login_username.trim().is_empty() && !self.login_password.is_empty() {
                self.login_error = None;
                self.login_submitted = true;
                self.do_connect();
            }
        }

        // Clique nos campos / botao — calculado em screen coords normalizadas
        let vis = ctx.camera.visible_rect();
        let cx = (vis.min.x + vis.max.x) * 0.5;
        let cy = (vis.min.y + vis.max.y) * 0.5;
        let h = (vis.max.y - vis.min.y) / 12.0;
        let field_w = 8.0 * h;
        let user_y  = cy + 1.5 * h;
        let pass_y  = cy + 0.0 * h;
        let btn_y     = cy - 1.8 * h;
        let field_h   = 0.8 * h;
        let remember_y = cy - 2.8 * h;

        // Link "Cadastre-se aqui" — mesmos calculos do render
        let fs_hint = 0.5 * h;
        let hint_pre  = "Sem conta? ";
        let hint_link = "Cadastre-se aqui";
        let (pre_w, link_w) = if let Some(font) = &self.font {
            (font.measure_width(hint_pre) * fs_hint,
             font.measure_width(hint_link) * fs_hint)
        } else {
            (hint_pre.len() as f32 * fs_hint * 0.6,
             hint_link.len() as f32 * fs_hint * 0.6)
        };
        let total_w = pre_w + link_w;
        let hint_y = cy - 3.7 * h;
        let link_cx = cx - total_w * 0.5 + pre_w + link_w * 0.5;
        let chk_size = 0.5 * h;

        if ctx.input.mouse_pressed(engine::winit::event::MouseButton::Left) {
            let mw = ctx.camera.screen_to_world(ctx.input.mouse_pos());
            if (mw.x - cx).abs() < field_w * 0.5 && (mw.y - user_y).abs() < field_h * 0.5 {
                self.login_field = LoginField::Username;
            } else if (mw.x - cx).abs() < field_w * 0.5 && (mw.y - pass_y).abs() < field_h * 0.5 {
                self.login_field = LoginField::Password;
            } else if (mw.x - cx).abs() < field_w * 0.5 && (mw.y - btn_y).abs() < field_h * 0.5 {
                if !self.login_submitted && !self.login_username.trim().is_empty() && !self.login_password.is_empty() {
                    self.login_error = None;
                    self.login_submitted = true;
                    self.do_connect();
                }
            } else if (mw.x - (cx - field_w * 0.5 + chk_size * 0.5)).abs() < chk_size
                   && (mw.y - remember_y).abs() < chk_size * 0.5 {
                self.login_remember = !self.login_remember;
            } else if (mw.x - link_cx).abs() < link_w * 0.5 && (mw.y - hint_y).abs() < fs_hint {
                let _ = std::process::Command::new("open")
                    .arg("http://localhost:5173")
                    .spawn();
            }
        }

        // Se ja submeteu, processar respostas do servidor
        if self.login_submitted {
            if let Some(net) = &mut self.net {
                while let Ok(msg) = net.incoming.try_recv() {
                    match msg {
                        ServerMessage::HandshakeAck { server_time_ms, .. } => {
                            let local_ms = now_ms() as i64;
                            self.server_time_offset_ms = server_time_ms as i64 - local_ms;
                        }
                        ServerMessage::LoginOk { entity_id, spawn, .. } => {
                            if self.login_remember {
                                save_creds(&self.login_username, &self.login_password);
                            } else {
                                clear_creds();
                            }
                            self.self_entity = Some(entity_id);
                            if let Some(map) = &self.world_map {
                                self.prediction = Some(PredictionBuffer::new(spawn, map));
                            }
                            ctx.camera.position = spawn;
                            self.entity_names.insert(entity_id, self.username.clone());
                            self.connected = true;
                            self.app_state = AppState::Playing;
                            self.login_submitted = false;
                        }
                        ServerMessage::LoginDenied { reason } => {
                            self.login_error = Some(reason);
                            self.login_submitted = false;
                        }
                        _ => {}
                    }
                }
            }
        }
    }

    fn render_login(&self, ctx: &AppContext, batch: &mut SpriteBatch) {
        let Some(font) = &self.font else { return };
        let vis = ctx.camera.visible_rect();
        let cx = (vis.min.x + vis.max.x) * 0.5;
        let cy = (vis.min.y + vis.max.y) * 0.5;
        // h escala com a altura visivel: painel ocupa ~55% da tela verticalmente
        let h = (vis.max.y - vis.min.y) / 12.0;
        let field_w    = 8.0 * h;
        let field_h    = 0.8 * h;
        let user_y     = cy + 1.5 * h;
        let pass_y     = cy + 0.0 * h;
        let btn_y      = cy - 1.8 * h;
        let remember_y = cy - 2.8 * h;

        // Fundo escuro
        let sw = vis.max.x - vis.min.x;
        let sh = vis.max.y - vis.min.y;
        batch.push(&Sprite {
            position: Vec2::new(cx, cy),
            size: Vec2::new(sw, sh),
            uv_min: Vec2::ZERO, uv_max: Vec2::splat(0.004),
            tint: Vec4::new(0.04, 0.05, 0.10, 1.0),
            depth: 0.0,
            ..Default::default()
        });

        // Painel central (altura cresceu para acomodar checkbox)
        batch.push(&Sprite {
            position: Vec2::new(cx, cy - 0.35 * h),
            size: Vec2::new(field_w + 2.0 * h, 8.5 * h),
            uv_min: Vec2::ZERO, uv_max: Vec2::splat(0.004),
            tint: Vec4::new(0.08, 0.10, 0.16, 0.97),
            depth: 0.05,
            ..Default::default()
        });
        // Borda topo do painel
        batch.push(&Sprite {
            position: Vec2::new(cx, cy - 0.35 * h + 4.25 * h - 0.04),
            size: Vec2::new(field_w + 2.0 * h, 0.07 * h),
            uv_min: Vec2::ZERO, uv_max: Vec2::splat(0.004),
            tint: Vec4::new(1.0, 0.85, 0.3, 1.0),
            depth: 0.06,
            ..Default::default()
        });

        // Titulo
        let title = "MMORPG 2D";
        let ts = 1.1 * h;
        let tw = font.measure_width(title) * ts;
        font.draw_depth(title,
            Vec2::new(cx - tw * 0.5, cy + 2.8 * h),
            ts, Vec4::new(1.0, 0.85, 0.3, 1.0), 0.1, batch);

        let draw_field = |label: &str, value: &str, y: f32, active: bool,
                          is_pass: bool, batch: &mut SpriteBatch| {
            // Label
            let lw = font.measure_width(label) * (0.6 * h);
            font.draw_depth(label,
                Vec2::new(cx - field_w * 0.5, y + field_h * 0.5 + 0.25 * h),
                0.6 * h, Vec4::new(0.75, 0.8, 0.95, 1.0), 0.1, batch);
            // Fundo campo
            let bg = if active { Vec4::new(0.14, 0.18, 0.28, 1.0) }
                     else      { Vec4::new(0.10, 0.12, 0.18, 1.0) };
            batch.push(&Sprite {
                position: Vec2::new(cx, y),
                size: Vec2::new(field_w, field_h),
                uv_min: Vec2::ZERO, uv_max: Vec2::splat(0.004),
                tint: bg, depth: 0.1, ..Default::default()
            });
            // Borda se ativo
            if active {
                batch.push(&Sprite {
                    position: Vec2::new(cx, y - field_h * 0.5 + 0.03),
                    size: Vec2::new(field_w, 0.06),
                    uv_min: Vec2::ZERO, uv_max: Vec2::splat(0.004),
                    tint: Vec4::new(0.4, 0.7, 1.0, 1.0), depth: 0.15, ..Default::default()
                });
            }
            // Texto + cursor
            let display = if is_pass {
                "*".repeat(value.chars().count())
            } else {
                value.to_string()
            };
            let cursor = if active { "_" } else { "" };
            let text = format!("{}{}", display, cursor);
            let fs = 0.65 * h;
            font.draw_depth(&text,
                Vec2::new(cx - field_w * 0.5 + 0.2, y + field_h * 0.2),
                fs, Vec4::ONE, 0.2, batch);
            let _ = lw; // suppress warning
        };

        draw_field("USUARIO", &self.login_username, user_y,
            self.login_field == LoginField::Username, false, batch);
        draw_field("SENHA", &self.login_password, pass_y,
            self.login_field == LoginField::Password, true, batch);

        // Botao entrar
        let btn_label = if self.login_submitted { "Conectando..." } else { "ENTRAR" };
        let btn_color = if self.login_submitted {
            Vec4::new(0.4, 0.4, 0.4, 1.0)
        } else {
            Vec4::new(1.0, 0.75, 0.2, 1.0)
        };
        batch.push(&Sprite {
            position: Vec2::new(cx, btn_y),
            size: Vec2::new(field_w, field_h),
            uv_min: Vec2::ZERO, uv_max: Vec2::splat(0.004),
            tint: btn_color, depth: 0.1, ..Default::default()
        });
        let bw = font.measure_width(btn_label) * (0.75 * h);
        font.draw_depth(btn_label,
            Vec2::new(cx - bw * 0.5, btn_y + field_h * 0.2),
            0.75 * h, Vec4::new(0.05, 0.05, 0.1, 1.0), 0.2, batch);

        // Checkbox "Lembrar acesso"
        let chk_size = 0.5 * h;
        let chk_x = cx - field_w * 0.5 + chk_size * 0.5;
        let chk_color = if self.login_remember {
            Vec4::new(0.3, 0.8, 0.4, 1.0)
        } else {
            Vec4::new(0.25, 0.28, 0.35, 1.0)
        };
        batch.push(&Sprite {
            position: Vec2::new(chk_x, remember_y),
            size: Vec2::splat(chk_size),
            uv_min: Vec2::ZERO, uv_max: Vec2::splat(0.004),
            tint: chk_color, depth: 0.1, ..Default::default()
        });
        let check_label = if self.login_remember { "[x] Lembrar acesso" } else { "[ ] Lembrar acesso" };
        font.draw_depth(check_label,
            Vec2::new(chk_x + chk_size * 0.7, remember_y - chk_size * 0.3),
            0.5 * h, Vec4::new(0.75, 0.8, 0.85, 1.0), 0.2, batch);

        // Mensagem de erro
        if let Some(err) = &self.login_error {
            let ew = font.measure_width(err) * (0.6 * h);
            font.draw_depth(err,
                Vec2::new(cx - ew * 0.5, remember_y - 0.7 * h),
                0.6 * h, Vec4::new(1.0, 0.35, 0.3, 1.0), 0.2, batch);
        }

        // Link de cadastro (clicavel — highlight azul)
        let hint_pre  = "Sem conta? ";
        let hint_link = "Cadastre-se aqui";
        let fs_hint = 0.5 * h;
        let pre_w  = font.measure_width(hint_pre)  * fs_hint;
        let link_w = font.measure_width(hint_link) * fs_hint;
        let total_w = pre_w + link_w;
        let hint_y = cy - 3.7 * h;
        font.draw_depth(hint_pre,
            Vec2::new(cx - total_w * 0.5, hint_y),
            fs_hint, Vec4::new(0.65, 0.68, 0.75, 1.0), 0.2, batch);
        font.draw_depth(hint_link,
            Vec2::new(cx - total_w * 0.5 + pre_w, hint_y),
            fs_hint, Vec4::new(0.35, 0.7, 1.0, 1.0), 0.2, batch);
        // Sublinhado do link
        batch.push(&Sprite {
            position: Vec2::new(cx - total_w * 0.5 + pre_w + link_w * 0.5, hint_y - fs_hint * 0.15),
            size: Vec2::new(link_w, fs_hint * 0.07),
            uv_min: Vec2::ZERO, uv_max: Vec2::splat(0.004),
            tint: Vec4::new(0.35, 0.7, 1.0, 0.8),
            depth: 0.25, ..Default::default()
        });

        // Tab para trocar campo
        let tip = "Tab = trocar campo   Enter = confirmar";
        let tiw = font.measure_width(tip) * (0.4 * h);
        font.draw_depth(tip,
            Vec2::new(cx - tiw * 0.5, cy - 3.6 * h),
            0.4 * h, Vec4::new(0.4, 0.45, 0.55, 1.0), 0.2, batch);
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
        ctx.camera.zoom = self.camera_zoom;

        self.tilemap = Some(map);
        self.world_map = Some(world);

        // 5. Pre-preencher credenciais salvas
        if let Some((u, p)) = load_saved_creds() {
            self.login_username = u;
            self.login_password = p;
            self.login_remember = true;
            self.login_field    = LoginField::Password; // cursor no campo senha
        }
    }

    fn update(&mut self, ctx: &mut AppContext, dt: f32) {
        // --- Tela de login ---
        if self.app_state == AppState::Login {
            self.update_login(ctx);
            return;
        }

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
                        self.app_state = AppState::Playing;
                        self.login_submitted = false;
                    }
                    ServerMessage::LoginDenied { reason } => {
                        tracing::error!("login negado: {reason}");
                        self.login_error = Some(reason);
                        self.login_submitted = false;
                        self.app_state = AppState::Login;
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

        // Flags de bloqueio de input — precisam estar disponiveis em todos
        // os blocos abaixo (chat, menu, input, joystick).
        let menu_open = self.menu_state != MenuState::Closed;
        let input_blocked = self.chat_typing || menu_open || self.shop_items.is_some();
        // Zoom da camera reflete config salva (live-update via slider).
        ctx.camera.zoom = self.camera_zoom;

        // --- Ping periódico para medir RTT ---
        self.ping_timer_s += dt;
        if self.connected && self.ping_timer_s >= PING_INTERVAL_S {
            self.ping_timer_s = 0.0;
            if let Some(net) = &self.net {
                net.send(ClientMessage::Ping { client_time_ms: now_ms() });
            }
        }

        // --- Chat: entrar/sair do modo digitando ---
        let opened_chat_this_frame = self.connected
            && !self.chat_typing
            && ctx.input.key_pressed(KeyCode::KeyT);
        if opened_chat_this_frame {
            self.chat_typing = true;
            self.chat_input_buf.clear();
        }
        // Sempre consome text_input — distribui para chat ou descarta
        let mut text_this_frame = ctx.input.take_text_input();
        // Se acabou de abrir o chat, descarta o 't' que ativou
        if opened_chat_this_frame {
            text_this_frame.retain(|c| c != 't' && c != 'T');
        }
        if self.chat_typing {
            for ch in text_this_frame.chars() {
                if ch == '\x08' || ch == '\x7f' {
                    self.chat_input_buf.pop();
                } else if ch == '\r' || ch == '\n' {
                    // Enter — envia mensagem
                    let msg = self.chat_input_buf.trim().to_string();
                    if !msg.is_empty() {
                        if let Some(net) = &self.net {
                            net.send(ClientMessage::Chat(msg));
                        }
                    }
                    self.chat_typing = false;
                    self.chat_input_buf.clear();
                    break;
                } else if !ch.is_control() && self.chat_input_buf.chars().count() < 200 {
                    self.chat_input_buf.push(ch);
                }
            }
            // ESC fecha chat (tratado no bloco acima via key_pressed)
        }

        // --- Gerar e enviar input ---
        if self.connected {
            self.input_seq = self.input_seq.wrapping_add(1);
            let (mv, aim, mut btns) = if input_blocked {
                (Vec2::ZERO, ctx.camera.position, 0u32)
            } else if self.touch_mode {
                // Mover: direcao do joystick. Mira: inimigo mais proximo
                // dentro de 10 tiles; fallback pra direcao de movimento.
                let move_dir = self.joystick.direction();
                let player_pos = self.prediction.as_ref()
                    .map(|p| p.predicted_pos)
                    .unwrap_or(ctx.camera.position);
                let aim = self.visible_entities.iter()
                    .filter(|e| matches!(e.kind, EntityKind::Enemy(_)))
                    .min_by(|a, b| {
                        a.pos.distance_squared(player_pos)
                            .partial_cmp(&b.pos.distance_squared(player_pos))
                            .unwrap()
                    })
                    .filter(|e| e.pos.distance_squared(player_pos) < 100.0)
                    .map(|e| e.pos)
                    .unwrap_or_else(|| {
                        if move_dir.length_squared() > 0.01 {
                            player_pos + move_dir * 5.0
                        } else {
                            player_pos + Vec2::X * 5.0
                        }
                    });
                (move_dir, aim, 0u32)
            } else {
                (
                    ctx.input.move_vector(),
                    ctx.camera.screen_to_world(ctx.input.mouse_pos()),
                    0u32,
                )
            };
            if !input_blocked {
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

        // --- Detectar dano recebido (diff de HP) e spawnar floaters ---
        let visible_ids: std::collections::HashSet<EntityId> =
            self.visible_entities.iter().map(|e| e.id).collect();

        // Caso 1: entidade visivel, HP caiu
        for e in &self.visible_entities {
            let Some(hp) = e.hp else { continue };
            if let Some(prev) = self.last_hp.get(&e.id).copied() {
                if hp.current < prev {
                    let dmg = prev - hp.current;
                    self.damage_numbers.push(DamageNumber {
                        world_pos: e.pos + Vec2::new(0.0, 0.6),
                        amount: dmg,
                        is_self: Some(e.id) == self.self_entity,
                        ttl: DAMAGE_NUM_TTL,
                    });
                }
            }
            self.last_hp.insert(e.id, hp.current);
            self.last_pos.insert(e.id, e.pos);
        }
        // Caso 2: entidade sumiu (morreu no mesmo tick do dano). Se tinhamos
        // HP > 0 rastreado, spawnamos o valor do HP que "levou" como dano.
        let vanished: Vec<EntityId> = self
            .last_hp
            .keys()
            .filter(|id| !visible_ids.contains(id))
            .copied()
            .collect();
        for id in vanished {
            let prev_hp = self.last_hp.remove(&id).unwrap_or(0);
            let pos = self.last_pos.remove(&id).unwrap_or(Vec2::ZERO);
            if prev_hp > 0 {
                self.damage_numbers.push(DamageNumber {
                    world_pos: pos + Vec2::new(0.0, 0.6),
                    amount: prev_hp,
                    is_self: Some(id) == self.self_entity,
                    ttl: DAMAGE_NUM_TTL,
                });
            }
        }

        // Tick + cull floaters
        for d in &mut self.damage_numbers {
            d.ttl -= dt;
            d.world_pos.y += dt * 1.4; // flutua pra cima
        }
        self.damage_numbers.retain(|d| d.ttl > 0.0);

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

        // --- Atalhos / ESC ---
        if ctx.input.key_pressed(KeyCode::Escape) {
            if self.chat_typing {
                self.chat_typing = false;
                self.chat_input_buf.clear();
            } else if self.shop_items.is_some() {
                self.shop_items = None;
            } else {
                self.menu_state = match self.menu_state {
                    MenuState::Closed   => MenuState::Main,
                    MenuState::Settings => MenuState::Main,
                    MenuState::Main     => MenuState::Closed,
                };
            }
        }

        // --- Clicks no menu (Main / Settings) ---
        if self.menu_state != MenuState::Closed {
            use engine::winit::event::MouseButton;
            if ctx.input.mouse_pressed(MouseButton::Left) {
                let vis = ctx.camera.visible_rect();
                let cx = vis.min.x + (vis.max.x - vis.min.x) * 0.5;
                let cy = vis.min.y + (vis.max.y - vis.min.y) * 0.5;
                let mouse_world = ctx.camera.screen_to_world(ctx.input.mouse_pos());
                let h = self.hud_scale;
                let btn_w = 4.0 * h;
                let btn_h = 0.8 * h;
                let gap = 0.25 * h;
                let in_btn = |row_y: f32| {
                    (mouse_world.x - cx).abs() < btn_w * 0.5
                        && (mouse_world.y - row_y).abs() < btn_h * 0.5
                };
                match self.menu_state {
                    MenuState::Main => {
                        // 2 botoes verticais centrados em cy
                        let y0 = cy + (btn_h + gap) * 0.5;
                        let y1 = cy - (btn_h + gap) * 0.5;
                        if in_btn(y0) { self.menu_state = MenuState::Settings; }
                        else if in_btn(y1) { ctx.should_exit = true; }
                    }
                    MenuState::Settings => {
                        // Replica o layout do render (veja MenuState::Settings em
                        // render pra detalhes).
                        let ctrl_line_h = 0.3 * h;
                        let panel_h_s = 0.9 * h
                            + 8.0 * ctrl_line_h
                            + 0.45 * h + btn_h
                            + gap
                            + 0.45 * h + btn_h
                            + gap * 1.5
                            + btn_h
                            + gap * 1.5
                            + btn_h
                            + 0.4 * h;
                        let bottom = cy - panel_h_s * 0.5;
                        let row_back_y     = bottom + 0.4 * h + btn_h * 0.5;
                        let row_joystick_y = row_back_y + btn_h + gap * 1.5;
                        let row_zoom_y     = row_joystick_y + btn_h + gap * 1.5;
                        let row_hud_y      = row_zoom_y + btn_h + gap + 0.45 * h;

                        let minus_cx = cx - btn_w * 0.35;
                        let plus_cx  = cx + btn_w * 0.35;
                        let sub_w = btn_h * 0.9;
                        let in_sub = |target_y: f32, sub_cx: f32| {
                            (mouse_world.x - sub_cx).abs() < sub_w * 0.5
                                && (mouse_world.y - target_y).abs() < btn_h * 0.5
                        };
                        let mut changed = false;
                        if in_sub(row_hud_y, minus_cx) {
                            self.hud_scale = (self.hud_scale - 0.2).max(0.8);
                            changed = true;
                        } else if in_sub(row_hud_y, plus_cx) {
                            self.hud_scale = (self.hud_scale + 0.2).min(5.0);
                            changed = true;
                        } else if in_sub(row_zoom_y, minus_cx) {
                            self.camera_zoom = (self.camera_zoom - 4.0).max(12.0);
                            changed = true;
                        } else if in_sub(row_zoom_y, plus_cx) {
                            self.camera_zoom = (self.camera_zoom + 4.0).min(80.0);
                            changed = true;
                        } else if in_btn(row_joystick_y) {
                            self.touch_mode = !self.touch_mode;
                            changed = true;
                        } else if in_btn(row_back_y) {
                            self.menu_state = MenuState::Main;
                        }
                        if changed {
                            save_settings(&Settings {
                                hud_scale: self.hud_scale,
                                touch_mode: self.touch_mode,
                                camera_zoom: self.camera_zoom,
                            });
                        }
                    }
                    MenuState::Closed => {}
                }
            }
        }

        // --- Joystick virtual (mobile-style: ativa onde clicar na esquerda) ---
        if self.touch_mode && !menu_open && !self.chat_typing {
            let mouse_pixel  = ctx.input.mouse_pos();
            let left_pressed = ctx.input.mouse_pressed(engine::winit::event::MouseButton::Left);
            let left_down    = ctx.input.mouse_down(engine::winit::event::MouseButton::Left);
            if self.joystick.active {
                if left_down {
                    // thumb offset em pixels -> converte p/ world scale via camera zoom
                    let dp = mouse_pixel - self.joystick.center_pixel;
                    let vis = ctx.camera.visible_rect();
                    let tiles_w = vis.max.x - vis.min.x;
                    let ppt = ctx.viewport.x / tiles_w;
                    // Y de tela cresce para baixo, Y de mundo cresce para cima
                    let off_world = Vec2::new(dp.x / ppt, -dp.y / ppt);
                    self.joystick.thumb_offset = off_world.clamp_length_max(self.joystick.radius);
                } else {
                    self.joystick.active = false;
                    self.joystick.thumb_offset = Vec2::ZERO;
                }
            } else if left_pressed && mouse_pixel.x < ctx.viewport.x * 0.5 {
                // Spawn joystick onde o usuario tocou (metade esquerda da tela)
                self.joystick.center_pixel = mouse_pixel;
                self.joystick.active = true;
                self.joystick.thumb_offset = Vec2::ZERO;
            }
        } else {
            self.joystick.active = false;
            self.joystick.thumb_offset = Vec2::ZERO;
        }

        // Interacao com NPC (E)
        if self.connected && !input_blocked && ctx.input.key_pressed(KeyCode::KeyE) {
            if let Some(net) = &self.net {
                net.send(ClientMessage::Interact);
            }
        }

        // Teclas 1..9: se loja aberta, compra slot N-1; senao usa item N-1.
        if self.connected && !self.chat_typing && !menu_open {
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
        if self.app_state == AppState::Login {
            self.render_login(ctx, batch);
            return;
        }

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
                    shared::item_id::STAFF         => Vec4::new(0.55, 0.4, 1.0, 1.0),
                    shared::item_id::SHIELD        => Vec4::new(0.4, 0.6, 0.9, 1.0),
                    shared::item_id::MANA_POTION   => Vec4::new(0.3, 0.5, 1.0, 1.0),
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

        // 2.5 Numeros de dano flutuantes
        if let Some(font) = &self.font {
            for d in &self.damage_numbers {
                let t = (d.ttl / DAMAGE_NUM_TTL).clamp(0.0, 1.0);
                let alpha = t; // fade-out linear
                let (r, g, b) = if d.is_self {
                    (1.0, 0.35, 0.35) // vermelho: dano no proprio
                } else {
                    (1.0, 0.95, 0.4)  // amarelo: dano em inimigo
                };
                let text = format!("-{}", d.amount);
                let w = font.measure_width(&text) * 0.6;
                font.draw_depth(
                    &text,
                    d.world_pos + Vec2::new(-w * 0.5, 0.0),
                    0.6,
                    Vec4::new(r, g, b, alpha),
                    layer::NAMEPLATE - d.world_pos.y + 100.0,
                    batch,
                );
            }
        }

        // 2.75 Minimapa (canto superior-direito) — escondido quando menu aberto.
        if self.menu_state == MenuState::Closed {
        if let Some(world) = &self.world_map {
            let vis = ctx.camera.visible_rect();
            let h = self.hud_scale;
            let mm_size = 3.2 * h;         // lado do painel em unidades de mundo
            let mm_tiles = 40i32;          // tiles mostrados por lado
            let cell = mm_size / mm_tiles as f32;
            // Canto inferior-direito — barras do jogador agora ficam no superior-direito.
            let mm_max = Vec2::new(vis.max.x - 0.3 * h, vis.min.y + mm_size + 0.3 * h);
            let mm_center = Vec2::new(
                mm_max.x - mm_size * 0.5,
                mm_max.y - mm_size * 0.5,
            );
            let player_world = self
                .prediction
                .as_ref()
                .map(|p| p.predicted_pos)
                .unwrap_or(ctx.camera.position);
            let px = player_world.x.floor() as i32;
            let py = player_world.y.floor() as i32;
            // Fundo do painel
            batch.push(&Sprite {
                position: mm_center,
                size: Vec2::splat(mm_size + 0.06 * h),
                uv_min: Vec2::ZERO, uv_max: Vec2::splat(0.004),
                tint: Vec4::new(1.0, 0.85, 0.35, 0.8),
                depth: layer::HUD,
                ..Default::default()
            });
            batch.push(&Sprite {
                position: mm_center,
                size: Vec2::splat(mm_size),
                uv_min: Vec2::ZERO, uv_max: Vec2::splat(0.004),
                tint: Vec4::new(0.08, 0.10, 0.14, 0.92),
                depth: layer::HUD + 0.05,
                ..Default::default()
            });
            // Paredes (sparse — amostra de 2 em 2 tiles pra cortar custo).
            let half = mm_tiles / 2;
            for dy in (-half..half).step_by(1) {
                for dx in (-half..half).step_by(1) {
                    let tx = px + dx;
                    let ty = py + dy;
                    if world.get(tx, ty) == shared::constants::tile_id::WALL {
                        let cx = mm_center.x + (dx as f32 + 0.5) * cell;
                        let cy = mm_center.y + (dy as f32 + 0.5) * cell;
                        batch.push(&Sprite {
                            position: Vec2::new(cx, cy),
                            size: Vec2::splat(cell * 0.95),
                            uv_min: Vec2::ZERO, uv_max: Vec2::splat(0.004),
                            tint: Vec4::new(0.35, 0.35, 0.4, 0.85),
                            depth: layer::HUD + 0.1,
                            ..Default::default()
                        });
                    }
                }
            }
            // Entidades (dots) — so pinta se cair dentro do painel
            let dot_half = cell * 1.5;
            for e in &self.visible_entities {
                let dx = e.pos.x - player_world.x;
                let dy = e.pos.y - player_world.y;
                if dx.abs() > half as f32 || dy.abs() > half as f32 { continue; }
                let cx = mm_center.x + dx * cell;
                let cy = mm_center.y + dy * cell;
                let (color, is_self) = match e.kind {
                    EntityKind::Player => {
                        if Some(e.id) == self.self_entity {
                            (Vec4::new(1.0, 1.0, 0.4, 1.0), true)
                        } else {
                            (Vec4::new(1.0, 1.0, 1.0, 1.0), false)
                        }
                    }
                    EntityKind::Enemy(k) => {
                        let [r, g, b, _] = shared::enemy_def(k).tint_rgba;
                        (Vec4::new(r, g, b, 1.0), false)
                    }
                    EntityKind::Npc(_) => (Vec4::new(1.0, 0.85, 0.35, 1.0), false),
                    EntityKind::Loot(_) => (Vec4::new(0.6, 0.9, 0.6, 1.0), false),
                    _ => continue,
                };
                let sz = if is_self { dot_half * 1.4 } else { dot_half };
                batch.push(&Sprite {
                    position: Vec2::new(cx, cy),
                    size: Vec2::splat(sz),
                    uv_min: Vec2::ZERO, uv_max: Vec2::splat(0.004),
                    tint: color,
                    depth: layer::HUD + 0.2,
                    ..Default::default()
                });
            }
        }
        } // menu_state == Closed guard do minimapa

        // 3. HUD (posicionado no canto superior-esquerdo em espaco de mundo)
        if let Some(font) = &self.font {
            let vis = ctx.camera.visible_rect();
            let h = self.hud_scale; // multiplicador global
            let margin = 0.3 * h;
            let top_left = Vec2::new(vis.min.x + margin, vis.max.y - margin);
            let bottom_left = Vec2::new(vis.min.x + margin, vis.min.y + 1.2 * h);

            // Coordenadas, ping e dicas no topo
            let class_name = match self.class {
                shared::PlayerClass::Warrior => "WAR",
                shared::PlayerClass::Archer  => "ARC",
                shared::PlayerClass::Wizard  => "WIZ",
            };
            let pos_text = format!(
                "[{class_name}] dmg={}  ({:.0}, {:.0})  ping={}ms  ESC=menu",
                self.stats.attack_damage, ctx.camera.position.x, ctx.camera.position.y, self.last_ping_ms
            );
            font.draw_depth(&pos_text, top_left, 0.85 * h, Vec4::new(0.8, 0.8, 0.8, 1.0), layer::HUD, batch);

            // Chat log (display-only) sobreposto acima do status
            if !self.chat_log.is_empty() {
                let line_h = 0.35 * h;
                let base = Vec2::new(vis.min.x + margin, vis.min.y + 1.2 * h + line_h);
                for (i, msg) in self.chat_log.iter().rev().enumerate() {
                    let y = base.y + i as f32 * line_h;
                    font.draw_depth(
                        msg,
                        Vec2::new(base.x, y),
                        0.75 * h,
                        Vec4::new(0.85, 0.9, 1.0, 0.9),
                        layer::HUD,
                        batch,
                    );
                }
            }

            // Painel da loja (overlay central)
            if let Some(items) = self.shop_items.clone() {
                let panel_w = 6.5 * h;
                let line_h = 0.5 * h;
                let panel_h = 0.8 * h + items.len() as f32 * line_h + 0.4 * h;
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
                    Vec2::new(cx - panel_w * 0.5 + 0.3 * h, cy + panel_h * 0.5 - 0.25 * h),
                    0.85 * h,
                    Vec4::new(1.0, 0.85, 0.35, 1.0),
                    layer::HUD + 1.2,
                    batch,
                );
                for (i, (iid, price)) in items.iter().enumerate() {
                    let iname = match *iid {
                        shared::item_id::HEALTH_POTION => "Pocao de Vida",
                        shared::item_id::MANA_POTION   => "Pocao de Mana",
                        shared::item_id::SWORD         => "Espada",
                        shared::item_id::STAFF         => "Cajado",
                        shared::item_id::ARMOR         => "Armadura",
                        shared::item_id::SHIELD        => "Escudo",
                        shared::item_id::RING          => "Anel",
                        _                              => "Item",
                    };
                    let row = format!("[{}] {:<18} {} ouro", i + 1, iname, price);
                    let y = cy + panel_h * 0.5 - 0.8 * h - i as f32 * line_h;
                    font.draw_depth(
                        &row,
                        Vec2::new(cx - panel_w * 0.5 + 0.3 * h, y),
                        0.75 * h,
                        Vec4::new(0.9, 0.95, 1.0, 1.0),
                        layer::HUD + 1.2,
                        batch,
                    );
                }
            }

            // Joystick virtual (modo touch / F2) — aparece apenas quando ativo
            if self.touch_mode && self.joystick.active {
                let jcenter = ctx.camera.screen_to_world(self.joystick.center_pixel);
                let r = self.joystick.radius;
                // Base (anel externo semi-transparente)
                batch.push(&Sprite {
                    position: jcenter,
                    size: Vec2::splat(r * 2.0),
                    uv_min: Vec2::ZERO, uv_max: Vec2::splat(0.004),
                    tint: Vec4::new(0.08, 0.1, 0.15, 0.55),
                    depth: layer::HUD + 0.3,
                    ..Default::default()
                });
                // Borda circular (linha horizontal fina)
                batch.push(&Sprite {
                    position: jcenter,
                    size: Vec2::new(r * 2.0 + 0.08, 0.08),
                    uv_min: Vec2::ZERO, uv_max: Vec2::splat(0.004),
                    tint: Vec4::new(0.9, 0.9, 0.9, 0.45),
                    depth: layer::HUD + 0.31,
                    ..Default::default()
                });
                // Thumb
                let thumb_pos = jcenter + self.joystick.thumb_offset;
                batch.push(&Sprite {
                    position: thumb_pos,
                    size: Vec2::splat(r * 0.55),
                    uv_min: Vec2::ZERO, uv_max: Vec2::splat(0.004),
                    tint: Vec4::new(1.0, 0.85, 0.35, 0.92),
                    depth: layer::HUD + 0.4,
                    ..Default::default()
                });
            }

            // Menu (ESC) — sobrepoe tudo menos o cursor
            if self.menu_state != MenuState::Closed {
                let cx = vis.min.x + (vis.max.x - vis.min.x) * 0.5;
                let cy = vis.min.y + (vis.max.y - vis.min.y) * 0.5;
                // Fundo escurecido
                let full_w = vis.max.x - vis.min.x;
                let full_h = vis.max.y - vis.min.y;
                batch.push(&Sprite {
                    position: Vec2::new(cx, cy),
                    size: Vec2::new(full_w, full_h),
                    uv_min: Vec2::ZERO, uv_max: Vec2::splat(0.004),
                    tint: Vec4::new(0.0, 0.0, 0.0, 0.55),
                    depth: layer::HUD + 5.0,
                    ..Default::default()
                });

                let btn_w = 4.0 * h;
                let btn_h = 0.8 * h;
                let gap = 0.25 * h;

                // Painel central — alturas calculadas pra nao sobrepor textos.
                //   Main:     titulo + 2 botoes
                //   Settings: titulo + 8 linhas de controles + label + slider + voltar
                let panel_w = btn_w + 1.0 * h;
                let ctrl_line_h = 0.3 * h;
                let settings_controls_rows = 8.0;
                let panel_h = match self.menu_state {
                    MenuState::Settings => {
                        0.9 * h                                  // titulo + padding
                        + settings_controls_rows * ctrl_line_h   // lista de controles
                        + 0.45 * h                               // label HUD
                        + btn_h                                  // slider HUD
                        + gap                                    // gap
                        + 0.45 * h                               // label Zoom
                        + btn_h                                  // slider Zoom
                        + gap * 1.5
                        + btn_h                                  // botao Joystick
                        + gap * 1.5
                        + btn_h                                  // botao Voltar
                        + 0.4 * h                                // padding inferior
                    }
                    _ => btn_h * 2.0 + gap + 1.3 * h,
                };
                batch.push(&Sprite {
                    position: Vec2::new(cx, cy),
                    size: Vec2::new(panel_w, panel_h),
                    uv_min: Vec2::ZERO, uv_max: Vec2::splat(0.004),
                    tint: Vec4::new(0.08, 0.09, 0.14, 0.97),
                    depth: layer::HUD + 5.1,
                    ..Default::default()
                });
                batch.push(&Sprite {
                    position: Vec2::new(cx, cy + panel_h * 0.5 - 0.03 * h),
                    size: Vec2::new(panel_w, 0.06 * h),
                    uv_min: Vec2::ZERO, uv_max: Vec2::splat(0.004),
                    tint: Vec4::new(1.0, 0.85, 0.35, 0.9),
                    depth: layer::HUD + 5.2,
                    ..Default::default()
                });

                let draw_btn = |pos: Vec2, label: &str, font: &BitmapFont, batch: &mut SpriteBatch| {
                    batch.push(&Sprite {
                        position: pos,
                        size: Vec2::new(btn_w, btn_h),
                        uv_min: Vec2::ZERO, uv_max: Vec2::splat(0.004),
                        tint: Vec4::new(0.15, 0.17, 0.23, 1.0),
                        depth: layer::HUD + 5.3,
                        ..Default::default()
                    });
                    let lw = font.measure_width(label) * (0.85 * h);
                    font.draw_depth(
                        label,
                        pos + Vec2::new(-lw * 0.5, btn_h * 0.25),
                        0.85 * h,
                        Vec4::ONE,
                        layer::HUD + 5.4,
                        batch,
                    );
                };

                match self.menu_state {
                    MenuState::Main => {
                        // Titulo
                        let title = "MENU";
                        let tw = font.measure_width(title) * (1.0 * h);
                        font.draw_depth(
                            title,
                            Vec2::new(cx - tw * 0.5, cy + panel_h * 0.5 - 0.45 * h),
                            1.0 * h,
                            Vec4::new(1.0, 0.85, 0.35, 1.0),
                            layer::HUD + 5.4,
                            batch,
                        );
                        let y0 = cy + (btn_h + gap) * 0.5;
                        let y1 = cy - (btn_h + gap) * 0.5;
                        draw_btn(Vec2::new(cx, y0), "Configuracoes", font, batch);
                        draw_btn(Vec2::new(cx, y1), "Sair do Jogo",  font, batch);
                    }
                    MenuState::Settings => {
                        // Cursor vertical comeca no topo e empilha pra baixo.
                        let top = cy + panel_h * 0.5;
                        let bottom = cy - panel_h * 0.5;
                        let title_size = 0.85 * h;
                        // Titulo
                        let title = "CONFIGURACOES";
                        let tw = font.measure_width(title) * title_size;
                        font.draw_depth(
                            title,
                            Vec2::new(cx - tw * 0.5, top - title_size * 0.55),
                            title_size,
                            Vec4::new(1.0, 0.85, 0.35, 1.0),
                            layer::HUD + 5.4,
                            batch,
                        );

                        // Controles (8 linhas)
                        let controls = [
                            "WASD   mover",
                            "SHIFT  correr (stamina)",
                            "SPACE  atacar",
                            "Q      triple-shot (25 MP)",
                            "E      interagir com NPC",
                            "T      abrir chat",
                            "1-9    usar / equipar item",
                            "ESC    abrir/fechar menu",
                        ];
                        let ctrl_size = 0.55 * h;
                        // Primeira linha comeca logo abaixo do titulo (com gap)
                        let ctrl_start_y = top - 0.9 * h - ctrl_line_h * 0.5;
                        for (i, line) in controls.iter().enumerate() {
                            let y = ctrl_start_y - i as f32 * ctrl_line_h;
                            font.draw_depth(
                                line,
                                Vec2::new(cx - btn_w * 0.5 + 0.15 * h, y),
                                ctrl_size,
                                Vec4::new(0.82, 0.88, 1.0, 1.0),
                                layer::HUD + 5.4,
                                batch,
                            );
                        }

                        // Posicoes dos 3 widgets empilhados no fundo:
                        //   row_back_y  → botao Voltar (mais baixo)
                        //   row_zoom_y  → slider Zoom
                        //   row_hud_y   → slider HUD
                        // Cada slider tem seu label 0.18h acima.
                        let row_back_y      = bottom + 0.4 * h + btn_h * 0.5;
                        let row_joystick_y  = row_back_y + btn_h + gap * 1.5;
                        let row_zoom_y      = row_joystick_y + btn_h + gap * 1.5;
                        let row_hud_y       = row_zoom_y + btn_h + gap + 0.45 * h;

                        // Helper render de um slider (label + base + -/+/valor)
                        let draw_slider = |
                            label: &str,
                            value_text: String,
                            y: f32,
                            font: &BitmapFont,
                            batch: &mut SpriteBatch,
                        | {
                            font.draw_depth(
                                label,
                                Vec2::new(cx - btn_w * 0.5 + 0.1 * h, y + btn_h * 0.5 + 0.2 * h),
                                0.6 * h,
                                Vec4::new(0.9, 0.95, 1.0, 1.0),
                                layer::HUD + 5.4,
                                batch,
                            );
                            batch.push(&Sprite {
                                position: Vec2::new(cx, y),
                                size: Vec2::new(btn_w, btn_h),
                                uv_min: Vec2::ZERO, uv_max: Vec2::splat(0.004),
                                tint: Vec4::new(0.1, 0.12, 0.17, 1.0),
                                depth: layer::HUD + 5.3,
                                ..Default::default()
                            });
                            let minus_cx = cx - btn_w * 0.35;
                            let plus_cx  = cx + btn_w * 0.35;
                            let sub_w = btn_h * 0.9;
                            for (pos, sym) in [(minus_cx, "-"), (plus_cx, "+")] {
                                batch.push(&Sprite {
                                    position: Vec2::new(pos, y),
                                    size: Vec2::new(sub_w, btn_h * 0.8),
                                    uv_min: Vec2::ZERO, uv_max: Vec2::splat(0.004),
                                    tint: Vec4::new(1.0, 0.85, 0.35, 0.9),
                                    depth: layer::HUD + 5.4,
                                    ..Default::default()
                                });
                                let sw = font.measure_width(sym) * (1.0 * h);
                                font.draw_depth(
                                    sym,
                                    Vec2::new(pos - sw * 0.5, y + btn_h * 0.3),
                                    1.0 * h,
                                    Vec4::new(0.05, 0.05, 0.1, 1.0),
                                    layer::HUD + 5.5,
                                    batch,
                                );
                            }
                            let vw = font.measure_width(&value_text) * (0.9 * h);
                            font.draw_depth(
                                &value_text,
                                Vec2::new(cx - vw * 0.5, y + btn_h * 0.3),
                                0.9 * h,
                                Vec4::ONE,
                                layer::HUD + 5.5,
                                batch,
                            );
                        };

                        draw_slider("Tamanho HUD",
                            format!("{:.1}", self.hud_scale),
                            row_hud_y, font, batch);
                        draw_slider("Zoom Camera",
                            format!("{:.0}", self.camera_zoom),
                            row_zoom_y, font, batch);
                        {
                            let joy_label = if self.touch_mode {
                                "Joystick: LIGADO"
                            } else {
                                "Joystick: DESLIGADO"
                            };
                            let joy_color = if self.touch_mode {
                                Vec4::new(0.3, 1.0, 0.4, 1.0)
                            } else {
                                Vec4::new(0.7, 0.7, 0.75, 1.0)
                            };
                            // Fundo do botao
                            batch.push(&Sprite {
                                position: Vec2::new(cx, row_joystick_y),
                                size: Vec2::new(btn_w, btn_h),
                                uv_min: Vec2::ZERO, uv_max: Vec2::splat(0.004),
                                tint: Vec4::new(0.12, 0.14, 0.2, 1.0),
                                depth: layer::HUD + 5.3,
                                ..Default::default()
                            });
                            let tw = font.measure_width(joy_label) * (0.7 * h);
                            font.draw_depth(
                                joy_label,
                                Vec2::new(cx - tw * 0.5, row_joystick_y + btn_h * 0.25),
                                0.7 * h,
                                joy_color,
                                layer::HUD + 5.5,
                                batch,
                            );
                        }
                        draw_btn(Vec2::new(cx, row_back_y), "Voltar", font, batch);
                    }
                    MenuState::Closed => {}
                }
            }

            // Campo de input de chat (aparece quando typing)
            if self.chat_typing {
                let box_w = (vis.max.x - vis.min.x) * 0.55;
                let box_h = 0.45 * h;
                let box_pos = Vec2::new(
                    vis.min.x + margin + box_w * 0.5,
                    vis.min.y + margin + 0.25 * h,
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
                    position: box_pos + Vec2::new(0.0, box_h * 0.5 - 0.02 * h),
                    size: Vec2::new(box_w, 0.04 * h),
                    uv_min: Vec2::ZERO, uv_max: Vec2::splat(0.004),
                    tint: Vec4::new(1.0, 0.85, 0.35, 0.9),
                    depth: layer::HUD + 0.05,
                    ..Default::default()
                });
                let prompt = format!("> {}_", self.chat_input_buf);
                font.draw_depth(
                    &prompt,
                    box_pos + Vec2::new(-box_w * 0.5 + 0.15 * h, 0.12 * h),
                    0.7 * h,
                    Vec4::ONE,
                    layer::HUD + 0.1,
                    batch,
                );
            }

            // Status de conexao em baixo
            // Quando nao conectado, exibe "Conectando..." central. Depois do
            // login nao tem status permanente — controles estao no menu.
            if !self.connected {
                let msg = "Conectando...";
                let cx = vis.min.x + (vis.max.x - vis.min.x) / 2.0;
                let cy = vis.min.y + (vis.max.y - vis.min.y) / 2.0;
                let lw = font.measure_width(msg) * (1.0 * h);
                font.draw_depth(msg, Vec2::new(cx - lw * 0.5, cy), 1.0 * h,
                    Vec4::new(1.0, 0.85, 0.35, 1.0), layer::HUD, batch);
            }
            let _ = bottom_left; // suprime unused warning

            // Painel de debug (F3)
            if self.show_debug {
                let dbg = format!(
                    "entities: {}\ninterp_delay: {}ms\nzoom: {:.0}",
                    self.visible_entities.len(),
                    RENDER_DELAY_MS,
                    ctx.camera.zoom,
                );
                font.draw_depth(&dbg, top_left - Vec2::Y * 0.45 * h, 0.8 * h, Vec4::new(0.5, 1.0, 0.5, 1.0), layer::HUD, batch);
            }

            // HUD do jogador (HP/MP/XP + equip + hotbar). Esconde quando o
            // menu do jogo esta aberto pra nao visualmente poluir.
            let self_hp = self.visible_entities.iter()
                .find(|e| Some(e.id) == self.self_entity)
                .and_then(|e| e.hp);
            let show_game_hud = self.menu_state == MenuState::Closed;

            if show_game_hud {
            if let Some(hp) = self_hp {
                // HUD de stats empilhado no canto SUPERIOR-DIREITO.
                // Ordem (cima->baixo): HP, MP (se houver), Stamina, XP.
                let bar_w = 3.4 * h;
                let bar_h = 0.32 * h;
                let gap = 0.08 * h;
                // Ancora: canto superior direito, abaixo do texto de status.
                let right_x = vis.max.x - margin;
                let bar_cx = right_x - bar_w * 0.5;
                let mut top_y = vis.max.y - margin - 0.7 * h - bar_h * 0.5;

                let draw_bar = |
                    center: Vec2,
                    fill: f32,
                    bg_tint: Vec4,
                    fg_tint: Vec4,
                    label: String,
                    font: &BitmapFont,
                    batch: &mut SpriteBatch,
                | {
                    batch.push(&Sprite {
                        position: center,
                        size: Vec2::new(bar_w, bar_h),
                        uv_min: Vec2::ZERO, uv_max: Vec2::splat(0.004),
                        tint: bg_tint,
                        depth: layer::HUD,
                        ..Default::default()
                    });
                    let f = fill.clamp(0.0, 1.0);
                    if f > 0.0 {
                        batch.push(&Sprite {
                            position: center + Vec2::new((f - 1.0) * bar_w * 0.5, 0.0),
                            size: Vec2::new(bar_w * f, bar_h),
                            uv_min: Vec2::ZERO, uv_max: Vec2::splat(0.004),
                            tint: fg_tint,
                            depth: layer::HUD + 0.1,
                            ..Default::default()
                        });
                    }
                    let lw = font.measure_width(&label) * (0.55 * h);
                    font.draw_depth(
                        &label,
                        center + Vec2::new(-lw * 0.5, 0.1 * h),
                        0.55 * h,
                        Vec4::ONE,
                        layer::HUD + 0.2,
                        batch,
                    );
                };

                // HP
                draw_bar(
                    Vec2::new(bar_cx, top_y),
                    hp.current as f32 / hp.max.max(1) as f32,
                    Vec4::new(0.2, 0.05, 0.05, 0.9),
                    Vec4::new(0.9, 0.2, 0.2, 1.0),
                    format!("HP {}/{}", hp.current, hp.max),
                    font, batch,
                );
                top_y -= bar_h + gap;

                // MP (se tem mana)
                if self.stats.mp_max > 0 {
                    draw_bar(
                        Vec2::new(bar_cx, top_y),
                        self.mp_current as f32 / self.stats.mp_max as f32,
                        Vec4::new(0.05, 0.1, 0.25, 0.9),
                        Vec4::new(0.25, 0.5, 1.0, 0.95),
                        format!("MP {}/{}", self.mp_current, self.stats.mp_max),
                        font, batch,
                    );
                    top_y -= bar_h + gap;
                }

                // Stamina
                let stam_max = shared::STAMINA_MAX;
                draw_bar(
                    Vec2::new(bar_cx, top_y),
                    self.stamina_current as f32 / stam_max as f32,
                    Vec4::new(0.05, 0.15, 0.05, 0.9),
                    Vec4::new(0.4, 0.9, 0.35, 0.95),
                    format!("SP {}/{}", self.stamina_current, stam_max),
                    font, batch,
                );
                top_y -= bar_h + gap;

                // XP
                let next = shared::xp_for_level(self.level + 1);
                let cur_floor = shared::xp_for_level(self.level);
                let span = (next - cur_floor).max(1);
                let progress = ((self.xp.saturating_sub(cur_floor)) as f32 / span as f32).clamp(0.0, 1.0);
                draw_bar(
                    Vec2::new(bar_cx, top_y),
                    progress,
                    Vec4::new(0.08, 0.08, 0.12, 0.9),
                    Vec4::new(0.35, 0.75, 1.0, 0.95),
                    format!("L{}  {}/{}", self.level, self.xp.saturating_sub(cur_floor), span),
                    font, batch,
                );
            }

            // Slots de equipamento (arma / armadura / anel) — acima da hotbar
            let eq_slot_size = 0.55 * h;
            let eq_gap = 0.12 * h;
            let eq_slots: [(Option<u16>, &str); 3] = [
                (self.equipment.weapon, "W"),
                (self.equipment.armor,  "A"),
                (self.equipment.ring,   "R"),
            ];
            let eq_total_w = 3.0 * eq_slot_size + 2.0 * eq_gap;
            let eq_center_x = vis.min.x + (vis.max.x - vis.min.x) / 2.0;
            let eq_y = vis.min.y + margin + 1.7 * h;
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
                        shared::item_id::SWORD   => Vec4::new(0.8, 0.85, 0.95, 1.0),
                        shared::item_id::STAFF   => Vec4::new(0.55, 0.4, 1.0, 1.0),
                        shared::item_id::ARMOR   => Vec4::new(0.6, 0.6, 0.7, 1.0),
                        shared::item_id::SHIELD  => Vec4::new(0.4, 0.6, 0.9, 1.0),
                        shared::item_id::RING    => Vec4::new(1.0, 0.7, 0.9, 1.0),
                        _                        => Vec4::ONE,
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
                    pos + Vec2::new(-0.08 * h, -eq_slot_size * 0.55),
                    0.6 * h,
                    Vec4::new(0.7, 0.7, 0.75, 0.8),
                    layer::HUD + 0.2,
                    batch,
                );
            }

            // Hotbar do inventario — 24 slots em linha na base da tela
            let slot_size = 0.45 * h;
            let slot_gap = 0.06 * h;
            let cols = shared::INVENTORY_SLOTS as f32;
            let total_w = cols * slot_size + (cols - 1.0) * slot_gap;
            let center_x = vis.min.x + (vis.max.x - vis.min.x) / 2.0;
            let row_y = vis.min.y + margin + 0.9 * h;
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
                        shared::item_id::MANA_POTION   => Vec4::new(0.3, 0.5, 1.0, 1.0),
                        shared::item_id::SWORD         => Vec4::new(0.8, 0.85, 0.95, 1.0),
                        shared::item_id::STAFF         => Vec4::new(0.55, 0.4, 1.0, 1.0),
                        shared::item_id::ARMOR         => Vec4::new(0.6, 0.6, 0.7, 1.0),
                        shared::item_id::SHIELD        => Vec4::new(0.4, 0.6, 0.9, 1.0),
                        shared::item_id::RING          => Vec4::new(1.0, 0.7, 0.9, 1.0),
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
                        let lw = font.measure_width(&label) * (0.5 * h);
                        font.draw_depth(
                            &label,
                            pos + Vec2::new(slot_size * 0.5 - lw - 0.02 * h, -slot_size * 0.5 + 0.18 * h),
                            0.5 * h,
                            Vec4::ONE,
                            layer::HUD + 0.2,
                            batch,
                        );
                    }
                }
            }
            } // show_game_hud
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
