//! Mundo autoritativo — Fase 3: combate, morte, respawn, inimigos com IA.

use glam::Vec2;
use hecs::{Entity, World};
use shared::protocol::{buttons, ClientMessage, InputFrame, ServerMessage, WorldSnapshot};
use shared::{
    EntityId, EntityKind, EntitySnapshot, Health, PlayerId, Position, Velocity,
    AOI_RADIUS, ATTACK_COOLDOWN, ENEMY_ATTACK_COOLDOWN, ENEMY_ATTACK_RANGE,
    ENEMY_DETECT_RANGE, ENEMY_SPEED, ENEMY_START_COUNT, ENTITY_RADIUS, PLAYER_SPEED,
    PROJ_RADIUS, PROJ_SPEED, PROJ_TTL, RESPAWN_DELAY,
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

pub struct ProjTag {
    pub owner: EntityId,
    pub from_player: bool,
    pub ttl: f32,
    pub damage: i32,
}

pub struct EnemyTag {
    pub attack_cooldown: f32,
    pub wander_timer: f32,
    pub wander_dir: Vec2,
}

pub struct PlayerTag {
    pub name: String,
    pub player_id: PlayerId,
}

pub struct Session {
    pub handle: SessionHandle,
    pub entity: Option<Entity>,
    pub entity_id: EntityId,
    pub last_input_seq: u32,
    pub pending_input: Option<InputFrame>,
    pub logged_in: bool,
    pub attack_cooldown: f32,
    pub respawn_timer: Option<f32>,
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
        let mut w = Self {
            ecs: World::new(),
            sessions: HashMap::new(),
            tick: 0,
            removed_this_tick: Vec::new(),
            next_entity_id: 1,
            next_player_id: 1,
        };
        w.spawn_initial_enemies();
        w
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

    fn spawn_initial_enemies(&mut self) {
        // Inimigos distribuidos em grid ao redor da origem
        for i in 0..ENEMY_START_COUNT {
            let angle = (i as f32 / ENEMY_START_COUNT as f32) * std::f32::consts::TAU;
            let dist = 8.0 + (i % 3) as f32 * 5.0;
            let pos = Vec2::new(angle.cos() * dist, angle.sin() * dist);
            let eid = self.alloc_entity_id();
            self.ecs.spawn((
                NetId(eid),
                Position(pos),
                Velocity(Vec2::ZERO),
                Health { current: 50, max: 50 },
                EntityKind::Enemy(0),
                EnemyTag { attack_cooldown: (i as f32 * 0.3) % ENEMY_ATTACK_COOLDOWN, wander_timer: 0.0, wander_dir: Vec2::X },
            ));
        }
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
                respawn_timer: None,
                name: String::new(),
                player_id: PlayerId(0),
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
                let spawn = Vec2::new(2.0, 2.0);
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
                    s.name = username.clone();
                    s.player_id = pid;
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

        // ── A: processar inputs de jogadores ──────────────────────────────────
        struct InputResult {
            entity: Entity,
            new_vel: Vec2,
            wants_attack: bool,
            owner_id: EntityId,
            aim: Vec2,
        }
        let mut input_results: Vec<InputResult> = Vec::new();
        for session in self.sessions.values_mut() {
            if session.attack_cooldown > 0.0 { session.attack_cooldown -= dt; }
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
            if wants_attack { session.attack_cooldown = ATTACK_COOLDOWN; }
            input_results.push(InputResult {
                entity,
                new_vel: dir * PLAYER_SPEED,
                wants_attack,
                owner_id: session.entity_id,
                aim: frame.aim,
            });
        }

        // ── B: snapshot de posições de jogadores para IA dos inimigos ─────────
        let player_positions: Vec<(EntityId, Vec2)> = self
            .ecs
            .query::<(&NetId, &Position, &EntityKind)>()
            .iter()
            .filter_map(|(_, (net, pos, kind))| {
                if matches!(kind, EntityKind::Player) { Some((net.0, pos.0)) } else { None }
            })
            .collect();

        // ── C: IA dos inimigos ────────────────────────────────────────────────
        struct SpawnProj { owner_id: EntityId, from_player: bool, pos: Vec2, dir: Vec2 }
        let mut projs_to_spawn: Vec<SpawnProj> = Vec::new();

        for (_, (net, pos, vel, enemy)) in
            self.ecs.query_mut::<(&NetId, &Position, &mut Velocity, &mut EnemyTag)>()
        {
            if enemy.attack_cooldown > 0.0 { enemy.attack_cooldown -= dt; }
            enemy.wander_timer -= dt;

            let nearest = player_positions.iter().min_by(|a, b| {
                a.1.distance_squared(pos.0).partial_cmp(&b.1.distance_squared(pos.0)).unwrap()
            });

            if let Some((_, ppos)) = nearest {
                let dist = pos.0.distance(*ppos);
                if dist < ENEMY_DETECT_RANGE {
                    let chase_dir = (*ppos - pos.0).try_normalize().unwrap_or(Vec2::X);
                    vel.0 = chase_dir * ENEMY_SPEED;
                    if dist < ENEMY_ATTACK_RANGE && enemy.attack_cooldown <= 0.0 {
                        enemy.attack_cooldown = ENEMY_ATTACK_COOLDOWN;
                        projs_to_spawn.push(SpawnProj {
                            owner_id: net.0,
                            from_player: false,
                            pos: pos.0,
                            dir: chase_dir,
                        });
                    }
                } else {
                    // Vagar aleatoriamente
                    if enemy.wander_timer <= 0.0 {
                        let seed = lcg(self.tick as u64 ^ net.0.0 as u64 ^ 0xCAFE);
                        let angle = lcg_f32(seed) * std::f32::consts::TAU;
                        enemy.wander_dir = Vec2::new(angle.cos(), angle.sin());
                        enemy.wander_timer = 1.5 + lcg_f32(lcg(seed)) * 2.5;
                    }
                    vel.0 = enemy.wander_dir * ENEMY_SPEED * 0.4;
                }
            } else {
                vel.0 = Vec2::ZERO;
            }
        }

        // ── D: aplicar velocidades de jogadores + coletar ataques ────────────
        for ir in input_results {
            if let Ok(mut vel) = self.ecs.get::<&mut Velocity>(ir.entity) {
                vel.0 = ir.new_vel;
            }
            if ir.wants_attack {
                let pos = self.ecs.get::<&Position>(ir.entity).map(|p| p.0).unwrap_or(Vec2::ZERO);
                let dir = (ir.aim - pos).try_normalize().unwrap_or(Vec2::X);
                projs_to_spawn.push(SpawnProj {
                    owner_id: ir.owner_id,
                    from_player: true,
                    pos,
                    dir,
                });
            }
        }

        // ── E: spawnar projeteis ──────────────────────────────────────────────
        for sp in projs_to_spawn {
            let proj_id = self.alloc_entity_id();
            self.ecs.spawn((
                NetId(proj_id),
                Position(sp.pos),
                Velocity(sp.dir * PROJ_SPEED),
                EntityKind::Projectile,
                ProjTag { owner: sp.owner_id, from_player: sp.from_player, ttl: PROJ_TTL, damage: 10 },
            ));
        }

        // ── F: integrar movimento ─────────────────────────────────────────────
        for (_, (pos, vel)) in self.ecs.query_mut::<(&mut Position, &Velocity)>() {
            pos.0 += vel.0 * dt;
        }

        // ── G: deteccao de colisao projetil → entidade ────────────────────────
        let projs: Vec<(Entity, EntityId, Vec2, EntityId, bool, i32)> = self
            .ecs
            .query::<(&NetId, &Position, &ProjTag)>()
            .iter()
            .map(|(e, (net, pos, p))| (e, net.0, pos.0, p.owner, p.from_player, p.damage))
            .collect();

        let targets: Vec<(Entity, EntityId, Vec2, bool)> = self
            .ecs
            .query::<(&NetId, &Position, &EntityKind)>()
            .iter()
            .filter_map(|(e, (net, pos, kind))| match kind {
                EntityKind::Player => Some((e, net.0, pos.0, true)),
                EntityKind::Enemy(_) => Some((e, net.0, pos.0, false)),
                _ => None,
            })
            .collect();

        let hit_dist_sq = (ENTITY_RADIUS + PROJ_RADIUS) * (ENTITY_RADIUS + PROJ_RADIUS);
        let mut hit_projs: Vec<(Entity, EntityId)> = Vec::new();
        let mut damage_events: Vec<(Entity, i32)> = Vec::new();

        'outer: for (pe, pnet, ppos, powner, pfrom_player, pdmg) in &projs {
            for (te, tnet, tpos, is_player) in &targets {
                if tnet == powner { continue; } // sem auto-dano
                // Projetil de jogador só acerta inimigo; de inimigo só acerta jogador
                if *pfrom_player == *is_player { continue; }
                if ppos.distance_squared(*tpos) < hit_dist_sq {
                    damage_events.push((*te, *pdmg));
                    hit_projs.push((*pe, *pnet));
                    continue 'outer;
                }
            }
        }

        for (entity, dmg) in damage_events {
            if let Ok(mut hp) = self.ecs.get::<&mut Health>(entity) {
                hp.current = (hp.current - dmg).max(0);
            }
        }
        for (e, eid) in hit_projs {
            let _ = self.ecs.despawn(e);
            self.removed_this_tick.push(eid);
        }

        // ── H: morte de inimigos → loot ───────────────────────────────────────
        let dead_enemies: Vec<(Entity, EntityId, Vec2)> = self
            .ecs
            .query::<(&NetId, &Position, &Health, &EnemyTag)>()
            .iter()
            .filter_map(|(e, (net, pos, hp, _))| {
                if hp.current <= 0 { Some((e, net.0, pos.0)) } else { None }
            })
            .collect();

        for (e, eid, pos) in dead_enemies {
            let _ = self.ecs.despawn(e);
            self.removed_this_tick.push(eid);
            // Dropar loot
            let loot_id = self.alloc_entity_id();
            self.ecs.spawn((
                NetId(loot_id),
                Position(pos),
                Velocity(Vec2::ZERO),
                EntityKind::Loot,
            ));
            tracing::debug!("enemy {:?} morreu, loot {:?}", eid, loot_id);
        }

        // ── I: morte de jogadores → respawn timer ─────────────────────────────
        let dead_players: Vec<(Entity, EntityId)> = self
            .ecs
            .query::<(&NetId, &Health, &PlayerTag)>()
            .iter()
            .filter_map(|(e, (net, hp, _))| {
                if hp.current <= 0 { Some((e, net.0)) } else { None }
            })
            .collect();

        for (e, eid) in dead_players {
            let _ = self.ecs.despawn(e);
            self.removed_this_tick.push(eid);
            for session in self.sessions.values_mut() {
                if session.entity_id == eid {
                    session.entity = None;
                    session.respawn_timer = Some(RESPAWN_DELAY);
                    tracing::info!("{} morreu, respawn em {}s", session.name, RESPAWN_DELAY);
                    break;
                }
            }
        }

        // ── J: TTL de projeteis ───────────────────────────────────────────────
        let mut expired: Vec<(Entity, EntityId)> = Vec::new();
        for (e, (net, proj)) in self.ecs.query_mut::<(&NetId, &mut ProjTag)>() {
            proj.ttl -= dt;
            if proj.ttl <= 0.0 { expired.push((e, net.0)); }
        }
        for (e, eid) in expired {
            let _ = self.ecs.despawn(e);
            self.removed_this_tick.push(eid);
        }

        // ── K: timers de respawn ──────────────────────────────────────────────
        let mut respawns: Vec<(SessionId, String, PlayerId)> = Vec::new();
        for (sid, session) in self.sessions.iter_mut() {
            if let Some(timer) = &mut session.respawn_timer {
                *timer -= dt;
                if *timer <= 0.0 {
                    session.respawn_timer = None;
                    respawns.push((*sid, session.name.clone(), session.player_id));
                }
            }
        }
        for (sid, name, pid) in respawns {
            self.respawn_player(sid, name, pid);
        }
    }

    fn respawn_player(&mut self, sid: SessionId, name: String, pid: PlayerId) {
        let Some(session) = self.sessions.get(&sid) else { return };
        let entity_id = session.entity_id;
        let spawn = Vec2::new(2.0, 2.0);
        let e = self.ecs.spawn((
            NetId(entity_id),
            Position(spawn),
            Velocity(Vec2::ZERO),
            Health { current: 100, max: 100 },
            EntityKind::Player,
            PlayerTag { name: name.clone(), player_id: pid },
        ));
        if let Some(s) = self.sessions.get_mut(&sid) {
            s.entity = Some(e);
        }
        tracing::info!("respawn: {name}");
    }

    pub fn send_snapshots(&mut self) {
        let all: Vec<EntitySnapshot> = self
            .ecs
            .query::<(&NetId, &Position, &Velocity, &EntityKind, Option<&Health>, Option<&PlayerTag>)>()
            .iter()
            .map(|(_, (net, pos, vel, kind, hp, ptag))| EntitySnapshot {
                id: net.0,
                kind: *kind,
                pos: pos.0,
                vel: vel.0,
                hp: hp.copied(),
                name: ptag.map(|p| p.name.clone()),
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
            if !session.logged_in { continue; }
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

// LCG deterministico para wander de inimigos (sem dep de rand)
fn lcg(seed: u64) -> u64 {
    seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407)
}

fn lcg_f32(seed: u64) -> f32 {
    (seed >> 11) as f32 / (1u64 << 53) as f32
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}
