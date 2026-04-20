//! Cliente do MMO. Conecta no servidor, replica entidades, renderiza.
//!
//! Novidades vs scaffold inicial:
//! - Interpolacao de entidades remotas (InterpolationBuffer).
//! - Predicao client-side do proprio jogador (PredictionBuffer).
//! - Pronto para receber atlas, tilemap e animacoes (Fase 2).

mod interpolation;
mod net_client;
mod prediction;

use engine::app::{run, AppConfig, AppContext, Game};
use engine::glam::{Vec2, Vec4};
use engine::render::{Sprite, SpriteBatch};
use engine::winit::keyboard::KeyCode;
use interpolation::{InterpolationBuffer, RENDER_DELAY_MS};
use net_client::NetClient;
use prediction::PredictionBuffer;
use shared::protocol::{ClientMessage, InputFrame, ServerMessage, buttons};
use shared::{EntityId, EntityKind, EntitySnapshot};
use std::time::{SystemTime, UNIX_EPOCH};

fn now_ms() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0)
}

fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info,client=debug,engine=info".into()),
        )
        .init();

    let server_url = std::env::var("SERVER_URL").unwrap_or_else(|_| "ws://127.0.0.1:9000".into());
    let username = std::env::var("USERNAME").unwrap_or_else(|_| format!("Guest{}", std::process::id() & 0xFFFF));

    run(
        AppConfig { title: "MMORPG 2D", tick_hz: 30, initial_size: (1280, 720) },
        MmoClient::new(server_url, username),
    )
}

pub struct MmoClient {
    server_url: String,
    username: String,
    net: Option<NetClient>,

    // Estado replicado do mundo
    interp: InterpolationBuffer,
    visible_entities: Vec<EntitySnapshot>,

    // Proprio jogador
    self_entity: Option<EntityId>,
    prediction: Option<PredictionBuffer>,
    input_seq: u32,
    server_time_offset_ms: i64,
    connected: bool,
}

impl MmoClient {
    pub fn new(server_url: String, username: String) -> Self {
        Self {
            server_url,
            username,
            net: None,
            interp: InterpolationBuffer::new(),
            visible_entities: Vec::new(),
            self_entity: None,
            prediction: None,
            input_seq: 0,
            server_time_offset_ms: 0,
            connected: false,
        }
    }
}

impl Game for MmoClient {
    fn init(&mut self, _ctx: &mut AppContext) {
        tracing::info!("connecting to {}", self.server_url);
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
            Err(e) => tracing::error!("connect failed: {e}"),
        }
    }

    fn update(&mut self, ctx: &mut AppContext, dt: f32) {
        // --- Drenar mensagens do servidor ---
        if let Some(net) = &mut self.net {
            while let Ok(msg) = net.incoming.try_recv() {
                match msg {
                    ServerMessage::HandshakeAck { server_time_ms, .. } => {
                        let local_ms = now_ms() as i64;
                        self.server_time_offset_ms = server_time_ms as i64 - local_ms;
                    }
                    ServerMessage::LoginOk { entity_id, spawn, .. } => {
                        tracing::info!("logged in as {:?} @ {:?}", entity_id, spawn);
                        self.self_entity = Some(entity_id);
                        self.prediction = Some(PredictionBuffer::new(spawn));
                        ctx.camera.position = spawn;
                        self.connected = true;
                    }
                    ServerMessage::LoginDenied { reason } => {
                        tracing::error!("login denied: {reason}");
                        ctx.should_exit = true;
                    }
                    ServerMessage::Snapshot(snap) => {
                        let server_ms = snap.server_time_ms;
                        let last_seq = snap.last_input_seq;

                        // Reconciliacao do proprio jogador
                        if let (Some(self_id), Some(pred)) =
                            (self.self_entity, &mut self.prediction)
                        {
                            if let Some(e) = snap.entities.iter().find(|e| e.id == self_id) {
                                pred.reconcile(e.pos, last_seq);
                            }
                        }

                        self.interp.push(server_ms, snap.entities);
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

        // --- Gerar + enviar input ---
        if self.connected {
            self.input_seq = self.input_seq.wrapping_add(1);
            let mv = ctx.input.move_vector();
            let aim = ctx.camera.screen_to_world(ctx.input.mouse_pos());

            let mut btns = 0u32;
            if ctx.input.mouse_pressed(engine::winit::event::MouseButton::Left) {
                btns |= buttons::PRIMARY;
            }

            let frame = InputFrame {
                seq: self.input_seq,
                tick: 0,
                move_dir: mv,
                aim,
                buttons: btns,
            };

            // Aplicar predicao local
            if let Some(pred) = &mut self.prediction {
                pred.push_input(frame);
            }

            if let Some(net) = &self.net {
                net.send(ClientMessage::Input(frame));
            }
        }

        // --- Amostrar interpolacao para render_time ---
        let server_now = now_ms() as i64 + self.server_time_offset_ms;
        let render_time = (server_now as u64).saturating_sub(RENDER_DELAY_MS);
        self.visible_entities = self.interp.sample(render_time);

        // --- Camera segue o proprio jogador ---
        if let (Some(_id), Some(pred)) = (self.self_entity, &mut self.prediction) {
            let smooth = pred.smooth_position();
            let lerp = (dt * 10.0).clamp(0.0, 1.0);
            ctx.camera.position = ctx.camera.position.lerp(smooth, lerp);
        }

        if ctx.input.key_pressed(KeyCode::Escape) {
            ctx.should_exit = true;
        }
    }

    fn render(&mut self, _ctx: &mut AppContext, batch: &mut SpriteBatch, _alpha: f32) {
        let self_id = self.self_entity;

        for e in &self.visible_entities {
            // O proprio jogador usa posicao predita (sem delay de interp)
            let pos = if Some(e.id) == self_id {
                self.prediction.as_ref().map(|p| p.predicted_pos).unwrap_or(e.pos)
            } else {
                e.pos
            };

            let color = match e.kind {
                EntityKind::Player     => {
                    if Some(e.id) == self_id {
                        Vec4::new(0.2, 0.9, 0.2, 1.0) // proprio jogador: verde mais brilhante
                    } else {
                        Vec4::new(0.4, 1.0, 0.4, 1.0)
                    }
                }
                EntityKind::Enemy(_)   => Vec4::new(1.0, 0.3, 0.3, 1.0),
                EntityKind::Projectile => Vec4::new(1.0, 0.9, 0.2, 1.0),
                EntityKind::Loot       => Vec4::new(0.5, 0.8, 1.0, 1.0),
            };

            batch.push(&Sprite {
                position: pos,
                size: Vec2::splat(0.9),
                tint: color,
                ..Default::default()
            });
        }
    }

    fn shutdown(&mut self, _ctx: &mut AppContext) {
        if let Some(net) = &self.net {
            net.send(ClientMessage::RequestDisconnect);
        }
    }
}
