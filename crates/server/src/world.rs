//! Mundo autoritativo — Fase 3: combate, morte, respawn, inimigos com IA.

use glam::Vec2;
use hecs::{Entity, World};
use shared::protocol::{buttons, ClientMessage, InputFrame, ServerMessage, WorldSnapshot};
use shared::{
    EntityId, EntityKind, EntitySnapshot, Health, PlayerId, Position, Velocity,
    AOI_RADIUS, ATTACK_COOLDOWN, ENEMY_ATTACK_COOLDOWN, ENEMY_ATTACK_RANGE,
    ENEMY_DETECT_RANGE, ENEMY_SPEED, ENEMY_START_COUNT, ENTITY_RADIUS, PLAYER_SPEED,
    PROJ_RADIUS, PROJ_SPEED, PROJ_TTL, RESPAWN_DELAY, SPATIAL_CELL_SIZE,
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
    /// Resultado de uma autenticacao async (ver `auth::authenticate`).
    /// Enviado pela task de auth de volta ao world loop.
    AuthResult(SessionId, Result<crate::auth::AuthSuccess, crate::auth::AuthError>),
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
    /// True enquanto a verificacao de senha esta rodando — recusa logins
    /// duplicados da mesma sessao.
    pub auth_in_flight: bool,
    pub attack_cooldown: f32,
    pub respawn_timer: Option<f32>,
    pub name: String,
    pub player_id: PlayerId,
    pub account_id: Option<i64>,
    /// Progresso acumulado da conta (persistido em `characters.xp`).
    pub xp: u64,
}

/// Recursos compartilhados para autenticacao assincrona.
#[derive(Clone)]
pub struct AuthCtx {
    pub pool: sqlx::postgres::PgPool,
    pub tx: mpsc::UnboundedSender<IncomingMessage>,
}

pub struct GameWorld {
    pub ecs: World,
    pub sessions: HashMap<SessionId, Session>,
    pub tick: u32,
    pub removed_this_tick: Vec<EntityId>,
    pub map: shared::world_gen::WorldMap,
    pub physics: shared::physics::PhysicsWorld,
    next_entity_id: u32,
    next_player_id: u64,
    /// Cache de personagens persistidos (login lookup / save batch).
    characters: HashMap<String, crate::persistence::CharacterRow>,
    /// Contexto de auth: pool Postgres + canal pra mandar AuthResult.
    /// None = auth desabilitado (compat/testing).
    auth_ctx: Option<AuthCtx>,
}

impl GameWorld {
    pub fn new(characters: HashMap<String, crate::persistence::CharacterRow>) -> Self {
        let map = shared::world_gen::generate(42, 128, 128);
        let mut physics = shared::physics::PhysicsWorld::new();
        map.build_colliders(&mut physics);
        let mut w = Self {
            ecs: World::new(),
            sessions: HashMap::new(),
            tick: 0,
            removed_this_tick: Vec::new(),
            map,
            physics,
            next_entity_id: 1,
            next_player_id: 1,
            characters,
            auth_ctx: None,
        };
        w.spawn_initial_enemies();
        w
    }

    pub fn set_auth_ctx(&mut self, ctx: AuthCtx) {
        self.auth_ctx = Some(ctx);
    }

    /// Processa o resultado de autenticacao async. Se OK, faz o spawn completo
    /// do jogador (carregando character salvo se existir).
    pub fn on_auth_result(
        &mut self,
        sid: SessionId,
        result: Result<crate::auth::AuthSuccess, crate::auth::AuthError>,
    ) {
        let handle = match self.sessions.get_mut(&sid) {
            Some(s) => {
                s.auth_in_flight = false;
                s.handle.clone()
            }
            None => return,
        };

        let success = match result {
            Ok(s) => s,
            Err(e) => {
                let reason = match e {
                    crate::auth::AuthError::InvalidCredentials => {
                        "credenciais invalidas".to_string()
                    }
                    crate::auth::AuthError::Internal(msg) => format!("erro interno: {msg}"),
                };
                tracing::info!("auth fail: {reason}");
                let _ = handle.to_client.send(ServerMessage::LoginDenied { reason });
                return;
            }
        };

        // Recusa se o mesmo username ja esta logado em outra sessao.
        let already_logged = self.sessions.values().any(|s| {
            s.handle.id != sid
                && s.logged_in
                && s.account_id == Some(success.account_id)
        });
        if already_logged {
            let _ = handle.to_client.send(ServerMessage::LoginDenied {
                reason: "conta ja conectada".into(),
            });
            return;
        }

        let entity_id = match self.sessions.get(&sid) {
            Some(s) => s.entity_id,
            None => return,
        };
        let pid = self.alloc_player_id();
        let default_spawn = {
            let t = self.map.spawn_tile();
            Vec2::new(t.0 as f32 + 0.5, t.1 as f32 + 0.5)
        };
        let (mut spawn, health, saved_xp) = match self.characters.get(&success.username) {
            Some(row) => (row.pos, row.hp, row.xp),
            None => (default_spawn, Health { current: 100, max: 100 }, 0u64),
        };
        // Valida que a posicao salva nao esta dentro de uma parede (mapa
        // pode ter sido regenerado). Senao, volta pro spawn default.
        let tx = spawn.x.floor() as i32;
        let ty = spawn.y.floor() as i32;
        if self.map.get(tx, ty) == shared::constants::tile_id::WALL {
            tracing::warn!("saved pos ({tx},{ty}) em parede; usando spawn default");
            spawn = default_spawn;
        }
        // Dá respiro ao jogador: despawna inimigos muito proximos do spawn.
        let clear_r_sq: f32 = 6.0 * 6.0;
        let close_enemies: Vec<(Entity, EntityId)> = self
            .ecs
            .query::<(&NetId, &Position, &EnemyTag)>()
            .iter()
            .filter_map(|(e, (net, pos, _))| {
                if pos.0.distance_squared(spawn) < clear_r_sq {
                    Some((e, net.0))
                } else { None }
            })
            .collect();
        for (e, eid) in close_enemies {
            self.free_entity_body(e);
            let _ = self.ecs.despawn(e);
            self.removed_this_tick.push(eid);
        }
        let body = self.spawn_entity_body(spawn);
        let e = self.ecs.spawn((
            NetId(entity_id),
            body,
            Position(spawn),
            Velocity(Vec2::ZERO),
            health,
            EntityKind::Player,
            PlayerTag { name: success.username.clone(), player_id: pid },
        ));

        if let Some(s) = self.sessions.get_mut(&sid) {
            s.entity = Some(e);
            s.logged_in = true;
            s.name = success.username.clone();
            s.player_id = pid;
            s.account_id = Some(success.account_id);
            s.xp = saved_xp;
        }
        tracing::info!(
            "login ok: {} (account {}, xp {}) -> {:?} / {:?}",
            success.username, success.account_id, saved_xp, pid, entity_id
        );
        let _ = handle.to_client.send(ServerMessage::LoginOk {
            player_id: pid,
            entity_id,
            spawn,
        });
        let _ = handle.to_client.send(ServerMessage::ProgressUpdate {
            xp: saved_xp,
            level: shared::level_of_xp(saved_xp),
        });
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

    /// Cria rigid body + collider dinâmico para jogador/inimigo em `pos`.
    fn spawn_entity_body(&mut self, pos: Vec2) -> shared::PhysicsHandle {
        let rb = rapier2d::prelude::RigidBodyBuilder::dynamic()
            .translation([pos.x, pos.y].into())
            .lock_rotations()
            .build();
        let rb_handle = self.physics.rigid_body_set.insert(rb);
        let col = rapier2d::prelude::ColliderBuilder::ball(shared::constants::ENTITY_RADIUS)
            .restitution(0.0)
            .friction(0.0)
            .collision_groups(rapier2d::prelude::InteractionGroups::new(
                rapier2d::prelude::Group::GROUP_2,
                rapier2d::prelude::Group::GROUP_1,
                Default::default(),
            ))
            .build();
        self.physics
            .collider_set
            .insert_with_parent(col, rb_handle, &mut self.physics.rigid_body_set);
        shared::PhysicsHandle(rb_handle)
    }

    /// Remove o rigid body do ECS entity (se houver) antes de despawn.
    fn free_entity_body(&mut self, e: Entity) {
        if let Ok(h) = self.ecs.get::<&shared::PhysicsHandle>(e).map(|h| h.0) {
            self.physics.remove_body(h);
        }
    }

    fn spawn_initial_enemies(&mut self) {
        // Zona segura ao redor do spawn — nenhum inimigo nasce perto demais.
        const SAFE_RADIUS: f32 = 14.0;
        let spawn_tile = self.map.spawn_tile();
        let safe_center = Vec2::new(spawn_tile.0 as f32 + 0.5, spawn_tile.1 as f32 + 0.5);
        let safe_sq = SAFE_RADIUS * SAFE_RADIUS;
        let w = self.map.width as i32;
        let h = self.map.height as i32;
        let floor = shared::constants::tile_id::FLOOR;

        let mut seed: u64 = (self.tick as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ 0xBEEF_1337;
        let mut next_rand = || -> u64 {
            seed = seed
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            seed
        };

        let mut placed = 0usize;
        let mut attempts = 0usize;
        while placed < ENEMY_START_COUNT && attempts < ENEMY_START_COUNT * 50 {
            attempts += 1;
            let tx = (next_rand() % w as u64) as i32;
            let ty = (next_rand() % h as u64) as i32;
            if self.map.get(tx, ty) != floor { continue; }
            let pos = Vec2::new(tx as f32 + 0.5, ty as f32 + 0.5);
            if pos.distance_squared(safe_center) < safe_sq { continue; }

            let eid = self.alloc_entity_id();
            let handle = self.spawn_entity_body(pos);
            self.ecs.spawn((
                NetId(eid),
                handle,
                Position(pos),
                Velocity(Vec2::ZERO),
                Health { current: 50, max: 50 },
                EntityKind::Enemy(0),
                EnemyTag {
                    attack_cooldown: (placed as f32 * 0.3) % ENEMY_ATTACK_COOLDOWN,
                    wander_timer: 0.0,
                    wander_dir: Vec2::X,
                },
            ));
            placed += 1;
        }
        tracing::info!("spawned {placed} inimigos (safe radius {SAFE_RADIUS})");
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
                auth_in_flight: false,
                attack_cooldown: 0.0,
                respawn_timer: None,
                name: String::new(),
                player_id: PlayerId(0),
                account_id: None,
                xp: 0,
            },
        );
    }

    pub fn on_disconnect(&mut self, id: SessionId) {
        if let Some(s) = self.sessions.remove(&id) {
            if let Some(e) = s.entity {
                self.free_entity_body(e);
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
            ClientMessage::Login { username, password } => {
                // Nao autentica sincronamente — dispara task e marca sessao
                // como auth_in_flight. O resultado volta via AuthResult.
                let handle = {
                    let Some(session) = self.sessions.get_mut(&id) else { return };
                    if session.logged_in {
                        return; // ja logado, ignorar duplicata
                    }
                    if session.auth_in_flight {
                        return; // ja tem auth em andamento
                    }
                    session.auth_in_flight = true;
                    session.name = username.clone();
                    session.handle.clone()
                };

                let Some(auth_ctx) = self.auth_ctx.clone() else {
                    let _ = handle.to_client.send(ServerMessage::LoginDenied {
                        reason: "auth nao disponivel".into(),
                    });
                    return;
                };

                tokio::spawn(async move {
                    let result = crate::auth::authenticate(
                        &auth_ctx.pool,
                        &username,
                        &password,
                    )
                    .await;
                    let _ = auth_ctx.tx.send(IncomingMessage::AuthResult(id, result));
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
            ClientMessage::Ping { client_time_ms } => {
                if let Some(session) = self.sessions.get(&id) {
                    let _ = session.handle.to_client.send(ServerMessage::Pong {
                        client_time_ms,
                        server_time_ms: now_ms(),
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
            let damage = if sp.from_player { 25 } else { 10 };
            self.ecs.spawn((
                NetId(proj_id),
                Position(sp.pos),
                Velocity(sp.dir * PROJ_SPEED),
                EntityKind::Projectile,
                ProjTag { owner: sp.owner_id, from_player: sp.from_player, ttl: PROJ_TTL, damage },
            ));
        }

        // ── F: integrar movimento e colisao com Rapier ────────────────────────
        for (_, (handle, vel)) in self.ecs.query_mut::<(&shared::PhysicsHandle, &Velocity)>() {
            if let Some(rb) = self.physics.rigid_body_set.get_mut(handle.0) {
                rb.set_linvel([vel.0.x, vel.0.y].into(), true);
            }
        }

        self.physics.step(dt);

        for (_, (handle, pos)) in self.ecs.query_mut::<(&shared::PhysicsHandle, &mut Position)>() {
            if let Some(rb) = self.physics.rigid_body_set.get(handle.0) {
                pos.0.x = rb.translation().x;
                pos.0.y = rb.translation().y;
            }
        }

        // Projeteis nao tem rigidbody ainda, usam AABB manual
        let mut proj_hit_wall: Vec<(Entity, EntityId)> = Vec::new();
        for (e, (net, pos, vel, kind)) in self.ecs.query_mut::<(&NetId, &mut Position, &Velocity, &EntityKind)>() {
            if matches!(kind, EntityKind::Projectile) {
                let next_pos = pos.0 + vel.0 * dt;
                let tx = next_pos.x.floor() as i32;
                let ty = next_pos.y.floor() as i32;
                if self.map.get(tx, ty) != shared::constants::tile_id::WALL {
                    pos.0 = next_pos;
                } else {
                    proj_hit_wall.push((e, net.0));
                }
            }
        }

        for (e, eid) in proj_hit_wall {
            let _ = self.ecs.despawn(e);
            self.removed_this_tick.push(eid);
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
        // damage: (target_entity, target_net_id, dmg, attacker_net_id, attacker_is_player)
        let mut damage_events: Vec<(Entity, EntityId, i32, EntityId, bool)> = Vec::new();

        'outer: for (pe, pnet, ppos, powner, pfrom_player, pdmg) in &projs {
            for (te, tnet, tpos, is_player) in &targets {
                if tnet == powner { continue; } // sem auto-dano
                // Projetil de jogador só acerta inimigo; de inimigo só acerta jogador
                if *pfrom_player == *is_player { continue; }
                if ppos.distance_squared(*tpos) < hit_dist_sq {
                    damage_events.push((*te, *tnet, *pdmg, *powner, *pfrom_player));
                    hit_projs.push((*pe, *pnet));
                    continue 'outer;
                }
            }
        }

        // Credita o golpe fatal ao atacante: alvo_net_id -> atacante_net_id
        let mut kill_credits: HashMap<EntityId, EntityId> = HashMap::new();
        for (entity, target_id, dmg, attacker_id, attacker_is_player) in damage_events {
            if let Ok(mut hp) = self.ecs.get::<&mut Health>(entity) {
                hp.current = (hp.current - dmg).max(0);
                if hp.current == 0 && attacker_is_player {
                    kill_credits.insert(target_id, attacker_id);
                }
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
            self.free_entity_body(e);
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

            // Creditar XP para o jogador que matou
            if let Some(attacker_eid) = kill_credits.get(&eid).copied() {
                for session in self.sessions.values_mut() {
                    if session.entity_id == attacker_eid && session.logged_in {
                        session.xp = session.xp.saturating_add(shared::XP_PER_KILL);
                        let new_level = shared::level_of_xp(session.xp);
                        let _ = session
                            .handle
                            .to_client
                            .send(ServerMessage::ProgressUpdate {
                                xp: session.xp,
                                level: new_level,
                            });
                        tracing::debug!(
                            "kill credit: {} -> xp {} (L{})",
                            session.name, session.xp, new_level
                        );
                        break;
                    }
                }
            }
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
            self.free_entity_body(e);
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
        let entity_id = match self.sessions.get(&sid) {
            Some(s) => s.entity_id,
            None => return,
        };
        let spawn_tile = self.map.spawn_tile();
        let spawn = Vec2::new(spawn_tile.0 as f32 + 0.5, spawn_tile.1 as f32 + 0.5);
        let handle = self.spawn_entity_body(spawn);
        let e = self.ecs.spawn((
            NetId(entity_id),
            handle,
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

        // Spatial hash: índices de `all` por célula. Cell size = AOI_RADIUS ⇒
        // basta varrer 3×3 células ao redor do centro (ceil(AOI/cell) = 1).
        let cell = AOI_RADIUS.max(SPATIAL_CELL_SIZE);
        let mut grid: HashMap<(i32, i32), Vec<usize>> = HashMap::new();
        for (idx, snap) in all.iter().enumerate() {
            let cx = (snap.pos.x / cell).floor() as i32;
            let cy = (snap.pos.y / cell).floor() as i32;
            grid.entry((cx, cy)).or_default().push(idx);
        }

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
            let ccx = (center.x / cell).floor() as i32;
            let ccy = (center.y / cell).floor() as i32;
            let mut visible: Vec<EntitySnapshot> = Vec::new();
            for dy in -1..=1 {
                for dx in -1..=1 {
                    if let Some(idxs) = grid.get(&(ccx + dx, ccy + dy)) {
                        for &i in idxs {
                            let s = &all[i];
                            if s.pos.distance_squared(center) <= radius_sq {
                                visible.push(s.clone());
                            }
                        }
                    }
                }
            }
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

impl GameWorld {
    /// Monta linhas de persistencia com o estado atual de TODOS os jogadores
    /// logados. Tambem atualiza o cache em memoria pra que o proximo login
    /// (antes do DB terminar de gravar) ja veja dados novos.
    pub fn collect_character_rows(&mut self) -> Vec<crate::persistence::CharacterRow> {
        let mut out = Vec::with_capacity(self.sessions.len());
        let mut entries: Vec<(String, Vec2, Health, u64)> = Vec::new();
        for session in self.sessions.values() {
            if !session.logged_in { continue; }
            let Some(e) = session.entity else { continue };
            let pos = match self.ecs.get::<&Position>(e) { Ok(p) => p.0, Err(_) => continue };
            let hp = match self.ecs.get::<&Health>(e) { Ok(h) => *h, Err(_) => continue };
            entries.push((session.name.clone(), pos, hp, session.xp));
        }
        for (name, pos, hp, xp) in entries {
            let row = crate::persistence::CharacterRow {
                name: name.clone(),
                pos,
                hp,
                xp,
            };
            self.characters.insert(name, row.clone());
            out.push(row);
        }
        out
    }

    /// Captura o estado do personagem de uma sessao especifica (para persistir
    /// no disconnect). Retorna None se nao estiver logado ou ja morto.
    pub fn take_character_for_disconnect(&mut self, sid: &SessionId) -> Option<crate::persistence::CharacterRow> {
        let session = self.sessions.get(sid)?;
        if !session.logged_in { return None; }
        let e = session.entity?;
        let pos = self.ecs.get::<&Position>(e).ok()?.0;
        let hp = *self.ecs.get::<&Health>(e).ok()?;
        let row = crate::persistence::CharacterRow {
            name: session.name.clone(),
            pos,
            hp,
            xp: session.xp,
        };
        self.characters.insert(session.name.clone(), row.clone());
        Some(row)
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
