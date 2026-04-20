//! Cliente do MMO. Conecta no servidor, replica entidades, renderiza.
//!
//! Por enquanto: cada entidade e um quadrado colorido. O objetivo do
//! scaffold e provar o loop end-to-end: input -> servidor -> snapshot
//! -> render. Sprites do atlas chegam nas proximas fases (ver ROADMAP).

mod net_client;

use engine::app::{run, AppConfig, AppContext, Game};
use engine::glam::{Vec2, Vec4};
use engine::render::{Sprite, SpriteBatch};
use engine::winit::keyboard::KeyCode;
use net_client::NetClient;
use shared::protocol::{ClientMessage, InputFrame, ServerMessage};
use shared::{EntityId, EntityKind, EntitySnapshot};
use std::collections::HashMap;

fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info,client=debug,engine=info".into()),
        )
        .init();

    let server_url = std::env::var("SERVER_URL").unwrap_or_else(|_| "ws://127.0.0.1:9000".into());
    let username = std::env::var("USERNAME").unwrap_or_else(|_| format!("Guest{}", pid_suffix()));

    run(
        AppConfig { title: "MMORPG 2D (dev)", tick_hz: 30, initial_size: (1280, 720) },
        MmoClient::new(server_url, username),
    )
}

pub struct MmoClient {
    server_url: String,
    username: String,
    net: Option<NetClient>,
    entities: HashMap<EntityId, EntitySnapshot>,
    self_entity: Option<EntityId>,
    input_seq: u32,
    connected: bool,
}

impl MmoClient {
    pub fn new(server_url: String, username: String) -> Self {
        Self {
            server_url,
            username,
            net: None,
            entities: HashMap::new(),
            self_entity: None,
            input_seq: 0,
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
        // Drenar mensagens do servidor.
        if let Some(net) = &mut self.net {
            while let Ok(msg) = net.incoming.try_recv() {
                match msg {
                    ServerMessage::HandshakeAck { .. } => {}
                    ServerMessage::LoginOk { entity_id, spawn, .. } => {
                        tracing::info!("logged in as {:?}", entity_id);
                        self.self_entity = Some(entity_id);
                        ctx.camera.position = spawn;
                        self.connected = true;
                    }
                    ServerMessage::LoginDenied { reason } => {
                        tracing::error!("login denied: {reason}");
                        ctx.should_exit = true;
                    }
                    ServerMessage::Snapshot(snap) => {
                        self.entities.clear();
                        for e in snap.entities {
                            self.entities.insert(e.id, e);
                        }
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

        // Gerar e enviar input.
        if self.connected {
            self.input_seq = self.input_seq.wrapping_add(1);
            let mv = ctx.input.move_vector();
            let aim = ctx.camera.screen_to_world(ctx.input.mouse_pos());
            let frame = InputFrame {
                seq: self.input_seq,
                tick: 0,
                move_dir: mv,
                aim,
                buttons: 0,
            };
            if let Some(net) = &self.net {
                net.send(ClientMessage::Input(frame));
            }
        }

        // Camera acompanha o proprio player (smooth follow).
        if let Some(id) = self.self_entity {
            if let Some(e) = self.entities.get(&id) {
                let lerp = (dt * 10.0).clamp(0.0, 1.0);
                ctx.camera.position = ctx.camera.position.lerp(e.pos, lerp);
            }
        }

        if ctx.input.key_pressed(KeyCode::Escape) {
            ctx.should_exit = true;
        }
    }

    fn render(&mut self, _ctx: &mut AppContext, batch: &mut SpriteBatch, _alpha: f32) {
        // Render placeholder: quadrado colorido por tipo de entidade.
        for (_id, e) in &self.entities {
            let color = match e.kind {
                EntityKind::Player     => Vec4::new(0.4, 1.0, 0.4, 1.0),
                EntityKind::Enemy(_)   => Vec4::new(1.0, 0.4, 0.4, 1.0),
                EntityKind::Projectile => Vec4::new(1.0, 0.9, 0.3, 1.0),
                EntityKind::Loot       => Vec4::new(0.6, 0.8, 1.0, 1.0),
            };
            batch.push(&Sprite {
                position: e.pos,
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

fn pid_suffix() -> u32 {
    std::process::id() & 0xFFFF
}
