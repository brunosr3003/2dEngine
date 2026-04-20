//! Mundo autoritativo. Owned inteiramente pela task `world_loop` em tick.rs —
//! sem locks, sem Arc<Mutex>.

use glam::Vec2;
use hecs::{Entity, World};
use shared::protocol::{buttons, ClientMessage, InputFrame, ServerMessage, WorldSnapshot};
use shared::{
    EntityId, EntityKind, EntitySnapshot, Health, PlayerId, Position, Velocity, AOI_RADIUS,
    ATTACK_COOLDOWN, PLAYER_SPEED, PROJ_SPEED, PROJ_TTL,
};
use std::collections::HashMap;
use std::net::SocketAddr;
use tokio::sync::mpsc;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SessionId(pub SocketAddr);

#[derive(Debug, Clone)]
pub struct SessionHandle {
    pub id: SessionId,
    pub to_client: mpsc::UnboundedSender<ServerMessage>,
}

pub enum IncomingMessage {
    Connected(SessionHandle),
    Disconnected(SessionId),
    Message(SessionId, ClientMessage),
}

#[derive(Debug, Clone, Copy)]
pub struct NetId(pub EntityId);

/// Tag de projetil.
pub struct ProjTag {
    pub owner: EntityId,
    pub ttl: f32,
    pub damage: i32,
}

/// Estado por sessao ativa.
pub struct Session {
    pub handle: SessionHandle,
    pub entity: Option<Entity>,
    pub entity_id: EntityId,
    pub last_input_seq: u32,
    pub pending_input: Option<InputFrame>,
    pub logged_in: bool,
    pub attack_cooldown: f32,
}

/// Tag de jogador (so no servidor — nao serializa).
pub struct PlayerTag {
    pub name: String,
    pub player_id: PlayerId,
}

pub struct GameWorld {
    pub ecs: World,
    pub sessions: HashMap<SessionId, Session>,
    pub tick: u32,
    pub removed_this_tick: Vec<EntityId>,
    next_entity_id: u32,
    next_player_id: u64,
}

impl GameWorld {
    pub fn new() -> Self {
        Self {
            ecs: World::new(),
            sessions: HashMap::new(),
            tick: 0,
            removed_this_tick: Vec::new(),
            next_entity_id: 1,
            next_player_id: 1,
        }
    }

    fn alloc_entity_id(&mut self) -> EntityId {
        let id = EntityId(self.next_entity_id);
        self.next_entity_id = self.next_entity_id.wrapping_add(1).max(1);
        id
    }

    fn alloc_player_id(&mut self) -> PlayerId {
        let id = PlayerId(self.next_player_id);
        self.next_player_id = self.next_player_id.wrapping_add(1).max(1);
        id
    }

    pub fn on_connect(&mut self, handle: SessionHandle) {
        let entity_id = self.alloc_entity_id();
        self.sessions.insert(
            handle.id,
            Session {
                handle,
                entity: None,
                entity_id,
                last_input_seq: 0,
                pending_input: None,
                logged_in: false,
                attack_cooldown: 0.0,
            },
        );
    }

    pub fn on_disconnect(&mut self, id: SessionId) {
        if let Some(s) = self.sessions.remove(&id) {
            if let Some(e) = s.entity {
                let _ = self.ecs.despawn(e);
            }
            tracing::info!("{:?} disconnected", s.entity_id);
        }
    }

    pub fn on_message(&mut self, id: SessionId, msg: ClientMessage) {
        match msg {
            ClientMessage::Handshake { protocol_version, .. } => {
                let Some(session) = self.sessions.get(&id) else { return };
                if protocol_version != shared::PROTOCOL_VERSION {
                    let _ = session.handle.to_client.send(ServerMessage::Kick {
                        reason: format!(
                            "protocol mismatch: server={}, client={}",
                            shared::PROTOCOL_VERSION,
                            protocol_version
                        ),
                    });
                    return;
                }
                let _ = session.handle.to_client.send(ServerMessage::HandshakeAck {
                    protocol_version: shared::PROTOCOL_VERSION,
                    server_time_ms: now_ms(),
                });
            }
            ClientMessage::Login { username, .. } => {
                let (entity_id, handle) = {
                    let Some(session) = self.sessions.get(&id) else { return };
                    (session.entity_id, session.handle.clone())
                };
                let pid = self.alloc_player_id();
                let spawn = Vec2::new(0.0, 0.0);
                let e = self.ecs.spawn((
                    NetId(entity_id),
                    Position(spawn),
                    Velocity(Vec2::ZERO),
                    Health { current: 100, max: 100 },
                    EntityKind::Player,
                    PlayerTag { name: username.clone(), player_id: pid },
                ));
                if let Some(s) = self.sessions.get_mut(&id) {
                    s.entity = Some(e);
                    s.logged_in = true;
                }
                tracing::info!("login ok: {username} -> {:?} / {:?}", pid, entity_id);
                let _ = handle.to_client.send(ServerMessage::LoginOk {
                    player_id: pid,
                    entity_id,
                    spawn,
                });
            }
            ClientMessage::Input(frame) => {
                if let Some(s) = self.sessions.get_mut(&id) {
                    s.pending_input = Some(frame);
                }
            }
            ClientMessage::Chat(text) => {
                let from = self
                    .sessions
                    .get(&id)
                    .and_then(|s| s.entity)
                    .and_then(|e| self.ecs.get::<&PlayerTag>(e).ok().map(|p| p.name.clone()))
                    .unwrap_or_else(|| "?".into());
                for s in self.sessions.values() {
                    let _ = s.handle.to_client.send(ServerMessage::Chat {
                        from: from.clone(),
                        text: text.clone(),
                    });
                }
            }
            ClientMessage::RequestDisconnect => self.on_disconnect(id),
        }
    }

    pub fn step(&mut self, dt: f32) {
        self.tick = self.tick.wrapping_add(1);
        self.removed_this_tick.clear();

        // Phase 1: processar inputs — coletar intenções de movimento e ataque.
        // Split-borrow manual: sessions e ecs são campos separados.
        struct InputResult {
            entity: Entity,
            new_vel: Vec2,
            wants_attack: bool,
            owner_id: EntityId,
            aim: Vec2,
        }
        let mut input_results: Vec<InputResult> = Vec::new();

        for session in self.sessions.values_mut() {
            if session.attack_cooldown > 0.0 {
                session.attack_cooldown -= dt;
            }
            let Some(entity) = session.entity else { continue };
            let Some(frame) = session.pending_input.take() else { continue };
            session.last_input_seq = frame.seq;

            let dir = if frame.move_dir.length_squared() > 1.0 {
                frame.move_dir.normalize()
            } else {
                frame.move_dir
            };
            let wants_attack =
                (frame.buttons & buttons::PRIMARY != 0) && session.attack_cooldown <= 0.0;
            if wants_attack {
                session.attack_cooldown = ATTACK_COOLDOWN;
            }
            input_results.push(InputResult {
                entity,
                new_vel: dir * PLAYER_SPEED,
                wants_attack,
                owner_id: session.entity_id,
                aim: frame.aim,
            });
        }

        // Phase 2: aplicar velocidades ao ECS e coletar posições para spawn de projeteis.
        struct AttackIntent {
            owner_id: EntityId,
            pos: Vec2,
            dir: Vec2,
        }
        let mut attack_intents: Vec<AttackIntent> = Vec::new();

        for ir in input_results {
            if let Ok(mut vel) = self.ecs.get::<&mut Velocity>(ir.entity) {
                vel.0 = ir.new_vel;
            }
            if ir.wants_attack {
                let pos = self.ecs.get::<&Position>(ir.entity).map(|p| p.0).unwrap_or(Vec2::ZERO);
                let shoot_dir = (ir.aim - pos).try_normalize().unwrap_or(Vec2::X);
                attack_intents.push(AttackIntent { owner_id: ir.owner_id, pos, dir: shoot_dir });
            }
        }

        // Phase 3: spawnar projeteis.
        for intent in attack_intents {
            let proj_id = self.alloc_entity_id();
            self.ecs.spawn((
                NetId(proj_id),
                Position(intent.pos),
                Velocity(intent.dir * PROJ_SPEED),
                EntityKind::Projectile,
                ProjTag { owner: intent.owner_id, ttl: PROJ_TTL, damage: 10 },
            ));
        }

        // Phase 4: integrar movimento.
        for (_e, (pos, vel)) in self.ecs.query_mut::<(&mut Position, &Velocity)>() {
            pos.0 += vel.0 * dt;
        }

        // Phase 5: decrementar TTL de projeteis, coletar expirados.
        let mut expired: Vec<(Entity, EntityId)> = Vec::new();
        for (e, (net, proj)) in self.ecs.query_mut::<(&NetId, &mut ProjTag)>() {
            proj.ttl -= dt;
            if proj.ttl <= 0.0 {
                expired.push((e, net.0));
            }
        }
        for (e, eid) in expired {
            let _ = self.ecs.despawn(e);
            self.removed_this_tick.push(eid);
        }
    }

    pub fn send_snapshots(&mut self) {
        let all: Vec<EntitySnapshot> = self
            .ecs
            .query::<(&NetId, &Position, &Velocity, &EntityKind)>()
            .iter()
            .map(|(_e, (net, pos, vel, kind))| EntitySnapshot {
                id: net.0,
                kind: *kind,
                pos: pos.0,
                vel: vel.0,
                hp: None,
                name: None,
            })
            .collect();

        let removed = self.removed_this_tick.clone();

        let mut centers: HashMap<SessionId, Vec2> = HashMap::new();
        for (sid, session) in &self.sessions {
            if let Some(e) = session.entity {
                if let Ok(p) = self.ecs.get::<&Position>(e) {
                    centers.insert(*sid, p.0);
                }
            }
        }

        let radius_sq = AOI_RADIUS * AOI_RADIUS;
        for (sid, session) in &mut self.sessions {
            if !session.logged_in {
                continue;
            }
            let center = centers.get(sid).copied().unwrap_or(Vec2::ZERO);
            let visible: Vec<EntitySnapshot> = all
                .iter()
                .filter(|s| s.pos.distance_squared(center) <= radius_sq)
                .cloned()
                .collect();
            let _ = session.handle.to_client.send(ServerMessage::Snapshot(WorldSnapshot {
                tick: self.tick,
                server_time_ms: now_ms(),
                last_input_seq: session.last_input_seq,
                entities: visible,
                removed: removed.clone(),
            }));
        }
    }
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}
