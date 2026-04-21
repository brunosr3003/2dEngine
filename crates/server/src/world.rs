//! Mundo autoritativo — Fase 3: combate, morte, respawn, inimigos com IA.

use glam::Vec2;
use hecs::{Entity, World};
use shared::protocol::{buttons, ClientMessage, InputFrame, InvSpot, ServerMessage, WorldSnapshot};
use shared::mapfile::{MapEntity, MapFile};
use shared::{
    EntityId, EntityKind, EntitySnapshot, Health, PlayerId, Position, Velocity,
    AOI_RADIUS, ATTACK_COOLDOWN, ENEMY_START_COUNT, ENTITY_RADIUS, PLAYER_SPEED,
    PROJ_RADIUS, PROJ_SPEED, PROJ_TTL, RESPAWN_DELAY, SPATIAL_CELL_SIZE,
    BOSS_RESPAWN_DELAY,
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

/// Identifica um item dropado no chao.
#[derive(Debug, Clone, Copy)]
pub struct LootTag {
    pub item_id: u16,
    pub qty: u32,
}

pub struct PlayerTag {
    pub name: String,
    pub player_id: PlayerId,
}

/// Tag que marca uma entidade como invisivel aos sistemas de targeting.
/// Usado hoje pra Downed State; reutilizavel pra stealth/invisibilidade
/// e outras mecanicas futuras. Inimigos ignoram entidades com esta tag.
pub struct Untargetable;

/// Portal estatico — ao encostar, jogador e teleportado para `target`.
pub struct PortalTag {
    pub target: Vec2,
    /// Cooldown pra evitar teletransporte infinito quando chega no destino.
    pub cooldown: f32,
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
    pub secondary_cooldown: f32,
    pub respawn_timer: Option<f32>,
    /// True enquanto HP<=0 e player esta incapacitado aguardando se levantar.
    pub downed: bool,
    /// Timer regressivo (segundos) ate auto-revival com 5% HP.
    pub downed_heal_timer: f32,
    /// HP da barra de Downed State. So outros jogadores reduzem — se zerar,
    /// morte real (despawn + respawn_timer).
    pub downed_hp: i32,
    /// Ultimo snapshot enviado de downed (ativo/timer/hp) — usado pra
    /// nao spammar DownedUpdate quando nada muda.
    pub downed_last_sent: (bool, i32, i32),
    pub name: String,
    pub player_id: PlayerId,
    pub account_id: Option<i64>,
    pub class: shared::PlayerClass,
    pub stats: shared::PlayerStats,
    pub equipment: shared::Equipment,
    /// MP atual (volatil, nao persiste). Regenera por segundo ate stats.mp_max.
    pub mp_current: f32,
    /// Ultimo MP inteiro enviado ao cliente — usado pra trigger de ManaUpdate
    /// quando muda pelo menos 1 ponto.
    pub mp_last_sent: i32,
    /// Stamina atual (volatil). Drena sprintando, regenera senao.
    pub stamina_current: f32,
    pub stamina_last_sent: i32,
    /// Progresso acumulado da conta (persistido em `characters.xp`).
    pub xp: u64,
    /// Inventario do jogador. Tamanho fixo = shared::INVENTORY_SLOTS.
    pub inventory: Vec<shared::InventorySlot>,
    /// True quando o inventario mudou e precisa ser enviado pro cliente
    /// no fim do tick.
    pub inventory_dirty: bool,
    /// True quando o equipamento mudou (envia StatsUpdate no proximo tick).
    pub stats_dirty: bool,
    /// Slots do vault (INVENTORY_SLOTS); carregado no login, salvo no save.
    pub vault: Vec<shared::InventorySlot>,
    /// True quando vault mudou — envia VaultUpdate no proximo tick.
    pub vault_dirty: bool,
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
    /// Timer em segundos desde a ultima tentativa de respawn de inimigo.
    enemy_spawn_timer: f32,
    /// Entidade atual do boss (None se morto/nao spawnado ainda).
    boss_entity: Option<Entity>,
    /// Contador regressivo em segundos ate o proximo spawn do boss.
    boss_respawn_timer: f32,
    /// Se true, ataques/dano sao desabilitados em toda a area.
    /// Vindo de MapFile.safe_zone quando o mapa e carregado de arquivo.
    pub safe_zone: bool,
    /// True se o mapa foi carregado de MapFile (suprime spawns procedurais).
    from_mapfile: bool,
}

impl GameWorld {
    pub fn new(characters: HashMap<String, crate::persistence::CharacterRow>) -> Self {
        // Tenta carregar mapa de arquivo (editor); fallback pra proc-gen.
        let (map, from_mapfile, safe_zone, mapfile) = match MapFile::load("assets/map.bin") {
            Ok(mf) => {
                tracing::info!("mapa carregado de assets/map.bin ({} x {}, {} entidades)",
                               mf.width, mf.height, mf.entities.len());
                let wm = mf.to_world_map();
                let safe = mf.safe_zone;
                (wm, true, safe, Some(mf))
            }
            Err(e) => {
                tracing::info!("sem assets/map.bin ({e}); gerando mapa procedural");
                (shared::world_gen::generate(42, 128, 128), false, false, None)
            }
        };
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
            enemy_spawn_timer: 0.0,
            boss_entity: None,
            boss_respawn_timer: 30.0, // primeiro boss aparece em 30s
            safe_zone,
            from_mapfile,
        };
        if let Some(mf) = mapfile {
            w.spawn_mapfile_entities(&mf);
        } else {
            w.spawn_initial_enemies();
            w.spawn_vendor();
        }
        w
    }

    /// Raio em tiles para considerar que o jogador "encostou" no portal.
    const PORTAL_TRIGGER_RADIUS: f32 = 0.9;

    /// Checa cada jogador contra cada portal. Se proximo e sem cooldown,
    /// move o rigid body pro destino. Portais em cooldown (acabou de
    /// teleportar alguem) nao disparam pra evitar loop.
    fn process_portal_teleports(&mut self, dt: f32) {
        // Decrementa cooldowns
        for (_, pt) in self.ecs.query_mut::<&mut PortalTag>() {
            if pt.cooldown > 0.0 { pt.cooldown = (pt.cooldown - dt).max(0.0); }
        }

        // Snapshot de portais (pos, target, cooldown restante)
        let portals: Vec<(Entity, Vec2, Vec2, f32)> = self.ecs
            .query::<(&Position, &PortalTag)>()
            .iter()
            .map(|(e, (p, pt))| (e, p.0, pt.target, pt.cooldown))
            .collect();
        if portals.is_empty() { return; }

        let r_sq = Self::PORTAL_TRIGGER_RADIUS * Self::PORTAL_TRIGGER_RADIUS;

        // Jogadores: (entity, pos, handle)
        let players: Vec<(Entity, Vec2, shared::PhysicsHandle)> = self.ecs
            .query::<(&Position, &EntityKind, &shared::PhysicsHandle)>()
            .iter()
            .filter_map(|(e, (p, k, h))| match k {
                EntityKind::Player => Some((e, p.0, *h)),
                _ => None,
            })
            .collect();

        for (pe, ppos, handle) in players {
            for (portal_entity, portal_pos, target, cd) in &portals {
                if *cd > 0.0 { continue; }
                if ppos.distance_squared(*portal_pos) < r_sq {
                    // Teleporta o rigid body
                    if let Some(rb) = self.physics.rigid_body_set.get_mut(handle.0) {
                        rb.set_translation([target.x, target.y].into(), true);
                        rb.set_linvel([0.0, 0.0].into(), true);
                    }
                    // Atualiza Position tb pra evitar 1 frame de lag
                    if let Ok(mut pos) = self.ecs.get::<&mut Position>(pe) {
                        pos.0 = *target;
                    }
                    // Cooldown no portal (evita pingue-pongue)
                    if let Ok(mut pt) = self.ecs.get::<&mut PortalTag>(*portal_entity) {
                        pt.cooldown = 1.0;
                    }
                    tracing::debug!("portal teleport {:?} -> {:?}", ppos, target);
                    break;
                }
            }
        }
    }

    /// Spawna todas as entidades pre-posicionadas de um MapFile.
    fn spawn_mapfile_entities(&mut self, mf: &MapFile) {
        for placement in &mf.entities {
            let pos = Vec2::new(placement.pos[0], placement.pos[1]);
            match &placement.entity {
                MapEntity::Enemy { kind } => {
                    self.place_enemy(pos, *kind, 0.0);
                }
                MapEntity::Boss { kind } => {
                    let def = shared::enemy_def(*kind);
                    let hp_max = def.hp_max;
                    let net_id = self.alloc_entity_id();
                    let handle = self.spawn_entity_body(pos);
                    let e = self.ecs.spawn((
                        NetId(net_id),
                        Position(pos),
                        Velocity(Vec2::ZERO),
                        Health { current: hp_max, max: hp_max },
                        EntityKind::Enemy(*kind),
                        EnemyTag {
                            attack_cooldown: 0.0,
                            wander_timer: 0.0,
                            wander_dir: Vec2::X,
                        },
                        handle,
                    ));
                    if *kind == 7 { self.boss_entity = Some(e); }
                }
                MapEntity::Npc { .. } => {
                    let eid = self.alloc_entity_id();
                    self.ecs.spawn((
                        NetId(eid),
                        Position(pos),
                        Velocity(Vec2::ZERO),
                        EntityKind::Npc(1),
                    ));
                }
                MapEntity::Vault => {
                    let eid = self.alloc_entity_id();
                    self.ecs.spawn((
                        NetId(eid),
                        Position(pos),
                        Velocity(Vec2::ZERO),
                        EntityKind::Npc(2), // npc_id=2 = vault
                    ));
                }
                MapEntity::Portal { target_spawn, .. } => {
                    let eid = self.alloc_entity_id();
                    let target = Vec2::new(target_spawn[0], target_spawn[1]);
                    self.ecs.spawn((
                        NetId(eid),
                        Position(pos),
                        Velocity(Vec2::ZERO),
                        EntityKind::Portal,
                        PortalTag { target, cooldown: 0.0 },
                    ));
                }
            }
        }
        tracing::info!("mapfile: spawned {} entidades pre-posicionadas", mf.entities.len());
    }

    /// Spawna um vendedor estatico proximo ao spawn tile (decoracao + interacao).
    fn spawn_vendor(&mut self) {
        let t = self.map.spawn_tile();
        let pos = Vec2::new(t.0 as f32 + 1.5, t.1 as f32 + 1.5);
        // Acha o primeiro tile de chao adjacente se o exato esta em parede.
        let pos = if self.map.get(pos.x.floor() as i32, pos.y.floor() as i32)
            == shared::constants::tile_id::WALL
        {
            Vec2::new(t.0 as f32 + 0.5, t.1 as f32 + 0.5)
        } else {
            pos
        };
        let eid = self.alloc_entity_id();
        self.ecs.spawn((
            NetId(eid),
            Position(pos),
            Velocity(Vec2::ZERO),
            EntityKind::Npc(1),
        ));
        tracing::info!("vendor spawnado em {:?}", pos);
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

        // Se a mesma conta ja esta logada em outra sessao, expulsa a antiga
        // (padrao MMO: novo login vence, evita travar após disconnect zumbi).
        let stale_sids: Vec<SessionId> = self
            .sessions
            .iter()
            .filter_map(|(k, s)| {
                if *k != sid && s.logged_in && s.account_id == Some(success.account_id) {
                    Some(*k)
                } else {
                    None
                }
            })
            .collect();
        for stale in stale_sids {
            if let Some(old) = self.sessions.get(&stale) {
                let _ = old.handle.to_client.send(ServerMessage::Kick {
                    reason: "conta conectada em outro lugar".into(),
                });
            }
            // Persiste + limpa a sessao antiga.
            if let Some(row) = self.take_character_for_disconnect(&stale) {
                // Nao temos tx pra save aqui; usa cache em memoria suficiente
                // pra o novo login pegar dados atualizados. DB writer periodico
                // (ou disconnect real) grava no DB.
                self.characters.insert(row.name.clone(), row);
            }
            self.on_disconnect(stale);
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
        let (mut spawn, mut health, saved_xp, saved_inv, saved_equip, saved_vault) = match self.characters.get(&success.username) {
            Some(row) => (
                row.pos, row.hp, row.xp, row.inventory.clone(), row.equipment, row.vault.clone(),
            ),
            None => {
                let base = success.class.base_stats();
                (
                    default_spawn,
                    Health { current: base.hp_max, max: base.hp_max },
                    0u64,
                    vec![shared::InventorySlot::default(); shared::INVENTORY_SLOTS],
                    shared::Equipment::default(),
                    vec![shared::InventorySlot::default(); shared::INVENTORY_SLOTS],
                )
            }
        };
        // Stats efetivos considerando equipamento salvo.
        let stats = effective_stats(success.class, &saved_equip);
        // Re-sincroniza o max_hp (classe pode ter sido rebalanceada entre sessoes).
        health.max = stats.hp_max;
        if health.current > health.max { health.current = health.max; }
        if health.current <= 0 { health.current = stats.hp_max; }
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
            s.class = success.class;
            s.stats = stats;
            s.equipment = saved_equip;
            s.xp = saved_xp;
            s.inventory = saved_inv.clone();
            s.inventory_dirty = false;
            s.stats_dirty = false;
            s.vault = saved_vault;
            s.vault_dirty = false;
            s.mp_current = stats.mp_max as f32;
            s.mp_last_sent = stats.mp_max;
            s.stamina_current = shared::STAMINA_MAX as f32;
            s.stamina_last_sent = shared::STAMINA_MAX;
        }
        tracing::info!(
            "login ok: {} ({}, acc {}, xp {}) -> {:?} / {:?}",
            success.username, success.class.as_str(), success.account_id, saved_xp, pid, entity_id
        );
        let _ = handle.to_client.send(ServerMessage::LoginOk {
            player_id: pid,
            entity_id,
            spawn,
        });
        let _ = handle.to_client.send(ServerMessage::StatsUpdate {
            class: success.class,
            stats,
            equipment: saved_equip,
        });
        let _ = handle.to_client.send(ServerMessage::ManaUpdate {
            current: stats.mp_max,
        });
        let _ = handle.to_client.send(ServerMessage::StaminaUpdate {
            current: shared::STAMINA_MAX,
        });
        let _ = handle.to_client.send(ServerMessage::ProgressUpdate {
            xp: saved_xp,
            level: shared::level_of_xp(saved_xp),
        });
        let _ = handle.to_client.send(ServerMessage::InventoryUpdate {
            slots: saved_inv,
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

    /// Raio minimo entre um inimigo novo e um ponto "seguro" (spawn default
    /// ou posicao de um jogador).
    const ENEMY_SAFE_RADIUS: f32 = 12.0;

    /// Cria um inimigo no tile `pos` com kind e cooldown de ataque iniciais.
    fn place_enemy(&mut self, pos: Vec2, kind: u16, attack_cd: f32) {
        let def = shared::enemy_def(kind);
        let eid = self.alloc_entity_id();
        let handle = self.spawn_entity_body(pos);
        self.ecs.spawn((
            NetId(eid),
            handle,
            Position(pos),
            Velocity(Vec2::ZERO),
            Health { current: def.hp_max, max: def.hp_max },
            EntityKind::Enemy(kind),
            EnemyTag {
                attack_cooldown: attack_cd,
                wander_timer: 0.0,
                wander_dir: Vec2::X,
            },
        ));
    }

    /// Acha um tile de chao aleatorio longe de todos os jogadores e do spawn
    /// default. Retorna None se nao achou em `attempts` tentativas.
    fn pick_enemy_tile(&self, seed: u64, attempts: u32) -> Option<Vec2> {
        let floor = shared::constants::tile_id::FLOOR;
        let w = self.map.width as i32;
        let h = self.map.height as i32;
        let default = self.map.spawn_tile();
        let default_center = Vec2::new(default.0 as f32 + 0.5, default.1 as f32 + 0.5);

        let mut centers: Vec<Vec2> = vec![default_center];
        for s in self.sessions.values() {
            if let Some(e) = s.entity {
                if let Ok(p) = self.ecs.get::<&Position>(e) {
                    centers.push(p.0);
                }
            }
        }
        let safe_sq = Self::ENEMY_SAFE_RADIUS * Self::ENEMY_SAFE_RADIUS;

        let mut s = seed;
        for _ in 0..attempts {
            s = s.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            let tx = (s % w as u64) as i32;
            s = s.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            let ty = (s % h as u64) as i32;
            if self.map.get(tx, ty) != floor { continue; }
            let pos = Vec2::new(tx as f32 + 0.5, ty as f32 + 0.5);
            if centers.iter().any(|c| c.distance_squared(pos) < safe_sq) { continue; }
            return Some(pos);
        }
        None
    }

    /// Sorteia um kind com distribuicao ponderada (sem boss — boss tem spawn proprio).
    fn random_enemy_kind(seed: u64) -> u16 {
        let r = lcg_f32(seed);
        // grunt 40%, tank 12%, ranger 12%, ninja 12%, mago 8%, berserker 8%, arqueiro 8%
        if      r < 0.40 { 0 }
        else if r < 0.52 { 1 }
        else if r < 0.64 { 2 }
        else if r < 0.76 { 3 }
        else if r < 0.84 { 4 }
        else if r < 0.92 { 5 }
        else             { 6 }
    }

    fn spawn_initial_enemies(&mut self) {
        let seed_base: u64 = 0xBEEF_1337;
        let mut placed = 0usize;
        let mut counts = [0usize; 7];
        while placed < ENEMY_START_COUNT {
            let seed = seed_base ^ (placed as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15);
            let Some(pos) = self.pick_enemy_tile(seed, 60) else { break };
            let kind = Self::random_enemy_kind(lcg(seed));
            counts[kind as usize] += 1;
            let cd = (placed as f32 * 0.3) % shared::enemy_def(kind).attack_cooldown;
            self.place_enemy(pos, kind, cd);
            placed += 1;
        }
        tracing::info!(
            "spawned {placed} inimigos (g={} tk={} rng={} ninja={} mago={} berserk={} arc={}) r={}",
            counts[0], counts[1], counts[2], counts[3], counts[4], counts[5], counts[6],
            Self::ENEMY_SAFE_RADIUS
        );
    }

    fn spawn_boss(&mut self) {
        let seed = self.tick as u64 ^ 0xB055_B055;
        if let Some(pos) = self.pick_enemy_tile(seed, 100) {
            let def = shared::enemy_def(7);
            let hp_max = def.hp_max;
            let net_id = self.alloc_entity_id();
            let handle = self.spawn_entity_body(pos);
            let e = self.ecs.spawn((
                NetId(net_id),
                Position(pos),
                Velocity(Vec2::ZERO),
                Health { current: hp_max, max: hp_max },
                EntityKind::Enemy(7),
                EnemyTag {
                    attack_cooldown: 0.0,
                    wander_timer: 0.0,
                    wander_dir: Vec2::X,
                },
                handle,
            ));
            self.boss_entity = Some(e);
            tracing::info!("BOSS spawnado em ({:.1},{:.1})", pos.x, pos.y);
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
                auth_in_flight: false,
                attack_cooldown: 0.0,
                secondary_cooldown: 0.0,
                respawn_timer: None,
                downed: false,
                downed_heal_timer: 0.0,
                downed_hp: 0,
                downed_last_sent: (false, 0, 0),
                name: String::new(),
                player_id: PlayerId(0),
                account_id: None,
                class: shared::PlayerClass::Warrior,
                stats: shared::PlayerClass::Warrior.base_stats(),
                equipment: shared::Equipment::default(),
                mp_current: 0.0,
                mp_last_sent: 0,
                stamina_current: shared::STAMINA_MAX as f32,
                stamina_last_sent: shared::STAMINA_MAX,
                xp: 0,
                inventory: vec![shared::InventorySlot::default(); shared::INVENTORY_SLOTS],
                inventory_dirty: false,
                stats_dirty: false,
                vault: vec![shared::InventorySlot::default(); shared::INVENTORY_SLOTS],
                vault_dirty: false,
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
            ClientMessage::UseItem { slot } => {
                self.handle_use_item(id, slot as usize);
            }
            ClientMessage::Interact => {
                self.handle_interact(id);
            }
            ClientMessage::ShopBuy { slot_idx } => {
                self.handle_shop_buy(id, slot_idx as usize);
            }
            ClientMessage::VaultDeposit { inv_slot } => {
                self.handle_vault_deposit(id, inv_slot as usize);
            }
            ClientMessage::VaultWithdraw { vault_slot } => {
                self.handle_vault_withdraw(id, vault_slot as usize);
            }
            ClientMessage::VaultClose => {
                if let Some(s) = self.sessions.get(&id) {
                    let _ = s.handle.to_client.send(ServerMessage::VaultClose);
                }
            }
            ClientMessage::InventorySwap { a, b } => {
                self.handle_inventory_swap(id, a, b);
            }
            ClientMessage::StandUp => {
                self.handle_stand_up(id);
            }
            ClientMessage::RequestDisconnect => self.on_disconnect(id),
        }
    }

    pub fn step(&mut self, dt: f32) {
        self.tick = self.tick.wrapping_add(1);

        // --- Teleporte via portais ---
        self.process_portal_teleports(dt);

        // Respawn procedural e desabilitado quando o mapa vem de MapFile
        // (o editor define exatamente quais inimigos existem e onde).
        if !self.from_mapfile {
            // Respawn de inimigos: mantem populacao proxima de ENEMY_START_COUNT.
            self.enemy_spawn_timer += dt;
            if self.enemy_spawn_timer >= 1.0 {
                self.enemy_spawn_timer = 0.0;
                let count = self.ecs
                    .query::<(&EnemyTag, &EntityKind)>()
                    .iter()
                    .filter(|(_, (_, k))| !matches!(k, EntityKind::Enemy(7)))
                    .count();
                if count < ENEMY_START_COUNT {
                    let seed = lcg(self.tick as u64 ^ 0x51ED_BEEF_DEAD_BEEF);
                    if let Some(pos) = self.pick_enemy_tile(seed, 80) {
                        let kind = Self::random_enemy_kind(lcg(seed ^ 0xDEAD));
                        self.place_enemy(pos, kind, 0.0);
                        tracing::debug!("enemy respawn kind={kind} {:?} (total {})", pos, count + 1);
                    }
                }
            }

            // Respawn do boss (so em proc-gen)
            if self.boss_entity.is_none() {
                self.boss_respawn_timer -= dt;
                if self.boss_respawn_timer <= 0.0 {
                    self.spawn_boss();
                }
            }
        }
        self.removed_this_tick.clear();

        // ── A: processar inputs de jogadores ──────────────────────────────────
        struct InputResult {
            entity: Entity,
            new_vel: Vec2,
            wants_attack: bool,
            wants_secondary: bool,
            owner_id: EntityId,
            aim: Vec2,
            damage: i32,
        }
        let mut input_results: Vec<InputResult> = Vec::new();
        for session in self.sessions.values_mut() {
            if session.attack_cooldown > 0.0 { session.attack_cooldown -= dt; }
            if session.secondary_cooldown > 0.0 { session.secondary_cooldown -= dt; }

            // Regen de MP (continua mesmo sem input pendente).
            let mp_max = session.stats.mp_max as f32;
            if session.mp_current < mp_max {
                session.mp_current = (session.mp_current + shared::MP_REGEN_PER_SEC * dt).min(mp_max);
            }

            // Stamina default: regenera. Se sprint, drena abaixo.
            let wants_sprint_ambient = session
                .pending_input
                .as_ref()
                .map(|f| (f.buttons & buttons::DASH != 0) && f.move_dir.length_squared() > 0.0)
                .unwrap_or(false);
            if !wants_sprint_ambient {
                let stam_max = shared::STAMINA_MAX as f32;
                if session.stamina_current < stam_max {
                    session.stamina_current = (session.stamina_current
                        + shared::STAMINA_REGEN_PER_SEC * dt)
                        .min(stam_max);
                }
            }

            let Some(entity) = session.entity else { continue };
            let Some(frame) = session.pending_input.take() else { continue };
            session.last_input_seq = frame.seq;
            let dir = if frame.move_dir.length_squared() > 1.0 {
                frame.move_dir.normalize()
            } else {
                frame.move_dir
            };
            // Downed: sem ataques, rastejando
            let wants_attack = if session.downed {
                false
            } else {
                let w = (frame.buttons & buttons::PRIMARY != 0) && session.attack_cooldown <= 0.0;
                if w { session.attack_cooldown = ATTACK_COOLDOWN; }
                w
            };

            let has_mp = (session.mp_current as i32) >= shared::SECONDARY_MP_COST;
            let wants_secondary = !session.downed
                && (frame.buttons & buttons::SECONDARY != 0)
                && session.secondary_cooldown <= 0.0
                && has_mp;
            if wants_secondary {
                session.secondary_cooldown = shared::SECONDARY_COOLDOWN;
                session.mp_current -= shared::SECONDARY_MP_COST as f32;
            }

            // Sprint (DASH): drena stamina e multiplica velocidade se stamina
            // > 0 e houver movimento. Regen ambiental ja foi aplicado acima.
            let wants_sprint = !session.downed
                && (frame.buttons & buttons::DASH != 0)
                && session.stamina_current > 0.0
                && dir.length_squared() > 0.0;
            let base_speed = if session.downed {
                PLAYER_SPEED * shared::DOWNED_SPEED_MULT
            } else {
                PLAYER_SPEED
            };
            let speed = if wants_sprint {
                session.stamina_current = (session.stamina_current
                    - shared::STAMINA_DRAIN_PER_SEC * dt)
                    .max(0.0);
                base_speed * shared::SPRINT_SPEED_MULT
            } else {
                base_speed
            };

            input_results.push(InputResult {
                entity,
                new_vel: dir * speed,
                wants_attack,
                wants_secondary,
                owner_id: session.entity_id,
                aim: frame.aim,
                damage: session.stats.attack_damage,
            });
        }

        // ── B: snapshot de posições de jogadores para IA dos inimigos ─────────
        // Coleta entidades untargetable pra que inimigos ignorem.
        let untargetable: std::collections::HashSet<Entity> = self
            .ecs
            .query::<&Untargetable>()
            .iter()
            .map(|(e, _)| e)
            .collect();
        let player_positions: Vec<(EntityId, Vec2)> = self
            .ecs
            .query::<(&NetId, &Position, &EntityKind)>()
            .iter()
            .filter_map(|(e, (net, pos, kind))| {
                if matches!(kind, EntityKind::Player) && !untargetable.contains(&e) {
                    Some((net.0, pos.0))
                } else { None }
            })
            .collect();

        // ── C: IA dos inimigos ────────────────────────────────────────────────
        struct SpawnProj { owner_id: EntityId, from_player: bool, pos: Vec2, dir: Vec2, damage: i32 }
        let mut projs_to_spawn: Vec<SpawnProj> = Vec::new();

        for (_, (net, pos, vel, enemy, kind)) in
            self.ecs.query_mut::<(&NetId, &Position, &mut Velocity, &mut EnemyTag, &EntityKind)>()
        {
            let kind_id = match kind {
                EntityKind::Enemy(k) => *k,
                _ => 0,
            };
            let def = shared::enemy_def(kind_id);

            if enemy.attack_cooldown > 0.0 { enemy.attack_cooldown -= dt; }
            enemy.wander_timer -= dt;

            let nearest = player_positions.iter().min_by(|a, b| {
                a.1.distance_squared(pos.0).partial_cmp(&b.1.distance_squared(pos.0)).unwrap()
            });

            if let Some((_, ppos)) = nearest {
                let dist = pos.0.distance(*ppos);
                if dist < def.detect_range {
                    let to_player = (*ppos - pos.0).try_normalize().unwrap_or(Vec2::X);
                    // Comportamento de movimento por kind
                    let move_dir = if let Some(kite) = shared::enemy_kite_dist(kind_id) {
                        if dist > kite + 0.5      { to_player }
                        else if dist < kite - 0.5 { -to_player }
                        else                      { Vec2::ZERO }
                    } else {
                        to_player
                    };
                    vel.0 = move_dir * def.speed;

                    let attack_range = shared::enemy_attack_range(kind_id);
                    if dist < attack_range && enemy.attack_cooldown <= 0.0 {
                        enemy.attack_cooldown = def.attack_cooldown;
                        let proj_count = shared::enemy_proj_count(kind_id);
                        if proj_count <= 1 {
                            projs_to_spawn.push(SpawnProj {
                                owner_id: net.0,
                                from_player: false,
                                pos: pos.0,
                                dir: to_player,
                                damage: def.attack_damage,
                            });
                        } else {
                            // Cone attack (boss): distribui proj_count projéteis
                            let spread = shared::BOSS_SPREAD_RAD;
                            for i in 0..proj_count {
                                let t = if proj_count <= 1 { 0.0 }
                                    else { i as f32 / (proj_count - 1) as f32 };
                                let angle = (t - 0.5) * spread;
                                let (s, c) = angle.sin_cos();
                                let dir = Vec2::new(
                                    to_player.x * c - to_player.y * s,
                                    to_player.x * s + to_player.y * c,
                                );
                                projs_to_spawn.push(SpawnProj {
                                    owner_id: net.0,
                                    from_player: false,
                                    pos: pos.0,
                                    dir,
                                    damage: def.attack_damage,
                                });
                            }
                        }
                    }
                } else {
                    // Vagar aleatoriamente
                    if enemy.wander_timer <= 0.0 {
                        let seed = lcg(self.tick as u64 ^ net.0.0 as u64 ^ 0xCAFE);
                        let angle = lcg_f32(seed) * std::f32::consts::TAU;
                        enemy.wander_dir = Vec2::new(angle.cos(), angle.sin());
                        enemy.wander_timer = 1.5 + lcg_f32(lcg(seed)) * 2.5;
                    }
                    vel.0 = enemy.wander_dir * def.speed * 0.4;
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
                    damage: ir.damage,
                });
            }
            if ir.wants_secondary {
                let pos = self.ecs.get::<&Position>(ir.entity).map(|p| p.0).unwrap_or(Vec2::ZERO);
                let base_dir = (ir.aim - pos).try_normalize().unwrap_or(Vec2::X);
                let n = shared::SECONDARY_PROJ_COUNT;
                let spread = shared::SECONDARY_SPREAD_RAD;
                for i in 0..n {
                    // Distribui simetricamente: -spread/2 .. +spread/2
                    let t = if n <= 1 { 0.0 } else { i as f32 / (n - 1) as f32 };
                    let angle = -spread * 0.5 + spread * t;
                    let (sin, cos) = angle.sin_cos();
                    let d = Vec2::new(
                        base_dir.x * cos - base_dir.y * sin,
                        base_dir.x * sin + base_dir.y * cos,
                    );
                    projs_to_spawn.push(SpawnProj {
                        owner_id: ir.owner_id,
                        from_player: true,
                        pos,
                        dir: d,
                        damage: (ir.damage as f32 * 0.8) as i32,
                    });
                }
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
                ProjTag {
                    owner: sp.owner_id,
                    from_player: sp.from_player,
                    ttl: PROJ_TTL,
                    damage: sp.damage,
                },
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

        let combat_disabled = self.safe_zone;
        'outer: for (pe, pnet, ppos, powner, pfrom_player, pdmg) in &projs {
            for (te, tnet, tpos, is_player) in &targets {
                if tnet == powner { continue; } // sem auto-dano
                // Projetil de jogador só acerta inimigo; de inimigo só acerta jogador
                if *pfrom_player == *is_player { continue; }
                if ppos.distance_squared(*tpos) < hit_dist_sq {
                    if !combat_disabled {
                        damage_events.push((*te, *tnet, *pdmg, *powner, *pfrom_player));
                    }
                    hit_projs.push((*pe, *pnet));
                    continue 'outer;
                }
            }
        }

        // Credita o golpe fatal ao atacante: alvo_net_id -> atacante_net_id
        let mut kill_credits: HashMap<EntityId, EntityId> = HashMap::new();
        // Entidades que devem morrer de verdade (downed_hp zerou por player)
        let mut pending_real_death: Vec<(Entity, EntityId)> = Vec::new();
        // Lookup rapido: target entity_id pra checar downed
        let downed_targets: std::collections::HashSet<EntityId> = self.sessions
            .values()
            .filter(|s| s.downed)
            .map(|s| s.entity_id)
            .collect();
        for (entity, target_id, dmg, attacker_id, attacker_is_player) in damage_events {
            if downed_targets.contains(&target_id) {
                // Player ja esta downed. So dano de outro jogador drena a
                // barra de Downed. Mobs nao afetam.
                if attacker_is_player && attacker_id != target_id {
                    if let Some(s) = self.sessions.values_mut()
                        .find(|s| s.entity_id == target_id) {
                        s.downed_hp = (s.downed_hp - dmg).max(0);
                        if s.downed_hp <= 0 {
                            kill_credits.insert(target_id, attacker_id);
                            pending_real_death.push((entity, target_id));
                        }
                    }
                }
                continue;
            }
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
        let dead_enemies: Vec<(Entity, EntityId, Vec2, u16)> = self
            .ecs
            .query::<(&NetId, &Position, &Health, &EnemyTag, &EntityKind)>()
            .iter()
            .filter_map(|(e, (net, pos, hp, _, kind))| {
                if hp.current <= 0 {
                    let kid = match kind { EntityKind::Enemy(k) => *k, _ => 0 };
                    Some((e, net.0, pos.0, kid))
                } else { None }
            })
            .collect();

        for (e, eid, pos, kind_id) in dead_enemies {
            // Rastrear se era o boss
            if self.boss_entity == Some(e) {
                self.boss_entity = None;
                self.boss_respawn_timer = BOSS_RESPAWN_DELAY;
                tracing::info!("Boss morreu! Respawn em {BOSS_RESPAWN_DELAY}s");
            }
            self.free_entity_body(e);
            let _ = self.ecs.despawn(e);
            self.removed_this_tick.push(eid);

            // Loot table por kind
            let seed = lcg(self.tick as u64 ^ eid.0 as u64 ^ 0xBADA_55);
            let drops = enemy_loot_drops(kind_id, seed);
            for (item_id, qty) in drops {
                let loot_id = self.alloc_entity_id();
                // Espalha levemente os drops do boss
                let offset = if kind_id == 7 {
                    let a = lcg_f32(lcg(seed ^ item_id as u64)) * std::f32::consts::TAU;
                    Vec2::new(a.cos(), a.sin()) * lcg_f32(seed ^ (qty as u64)) * 1.5
                } else { Vec2::ZERO };
                self.ecs.spawn((
                    NetId(loot_id),
                    Position(pos + offset),
                    Velocity(Vec2::ZERO),
                    EntityKind::Loot(item_id),
                    LootTag { item_id, qty },
                ));
                tracing::debug!("loot drop: kind={kind_id} item={item_id} qty={qty}");
            }

            // Creditar XP para o jogador que matou
            if let Some(attacker_eid) = kill_credits.get(&eid).copied() {
                let xp_reward = shared::enemy_def(kind_id).xp_reward;
                for session in self.sessions.values_mut() {
                    if session.entity_id == attacker_eid && session.logged_in {
                        session.xp = session.xp.saturating_add(xp_reward);
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

        // ── I: transicao para Downed State ─────────────────────────────────
        // Jogadores com HP<=0 que nao estao downed entram no estado agora.
        // Dano vindo de OUTROS jogadores ja drena `downed_hp` no damage
        // handler acima; quando zera, ja foi empurrado para real_deaths
        // via `pending_real_death`.
        let hp_zero: Vec<(Entity, EntityId)> = self
            .ecs
            .query::<(&NetId, &Health, &PlayerTag)>()
            .iter()
            .filter_map(|(e, (net, hp, _))| {
                if hp.current <= 0 { Some((e, net.0)) } else { None }
            })
            .collect();
        for (entity, eid) in hp_zero {
            for session in self.sessions.values_mut() {
                if session.entity_id == eid && !session.downed {
                    session.downed = true;
                    session.downed_heal_timer = shared::DOWNED_HEAL_TIME;
                    session.downed_hp = shared::DOWNED_HP_MAX;
                    tracing::info!("{} foi derrubado (dHP={})",
                                   session.name, session.downed_hp);
                    let _ = self.ecs.insert_one(entity, Untargetable);
                    break;
                }
            }
        }
        // Aplica mortes reais acumuladas (downed_hp zerou por player).
        let deaths: Vec<(Entity, EntityId)> = std::mem::take(&mut pending_real_death);
        for (entity, eid) in deaths {
            self.free_entity_body(entity);
            let _ = self.ecs.despawn(entity);
            self.removed_this_tick.push(eid);
            for session in self.sessions.values_mut() {
                if session.entity_id == eid {
                    session.entity = None;
                    session.respawn_timer = Some(RESPAWN_DELAY);
                    session.downed = false;
                    session.downed_hp = 0;
                    tracing::info!("{} MORREU (barra downed zerou) — respawn em {}s",
                                   session.name, RESPAWN_DELAY);
                    break;
                }
            }
        }

        // Tick do timer de readiness. Quando zera, player pode apertar E
        // pra se levantar (nao auto-revive mais).
        for session in self.sessions.values_mut() {
            if session.downed && session.downed_heal_timer > 0.0 {
                session.downed_heal_timer = (session.downed_heal_timer - dt).max(0.0);
            }
        }

        // ── J.5: pickup de loot ───────────────────────────────────────────────
        // Coleta pares (session_id, player_pos) e todos os loots proximos.
        let pickup_players: Vec<(SessionId, Vec2)> = self
            .sessions
            .values()
            .filter_map(|s| {
                let e = s.entity?;
                let pos = self.ecs.get::<&Position>(e).ok()?.0;
                Some((s.handle.id, pos))
            })
            .collect();

        let loots: Vec<(Entity, EntityId, Vec2, LootTag)> = self
            .ecs
            .query::<(&NetId, &Position, &LootTag)>()
            .iter()
            .map(|(e, (net, pos, l))| (e, net.0, pos.0, *l))
            .collect();

        let pick_r_sq = shared::PICKUP_RADIUS * shared::PICKUP_RADIUS;
        let mut picked: Vec<(Entity, EntityId)> = Vec::new();
        // (player_entity, new_hp_max) — para ajustar Health.max apos equipar.
        let mut hp_max_updates: Vec<(Entity, i32)> = Vec::new();
        'loot_loop: for (le, leid, lpos, ltag) in loots {
            for (sid, ppos) in &pickup_players {
                if ppos.distance_squared(lpos) < pick_r_sq {
                    if let Some(session) = self.sessions.get_mut(sid) {
                        // Se for equipavel e o slot esta vazio, equipa direto.
                        if let Some(slot) = shared::equip_slot_of(ltag.item_id) {
                            let empty = match slot {
                                shared::EquipSlot::Weapon => session.equipment.weapon.is_none(),
                                shared::EquipSlot::Armor  => session.equipment.armor.is_none(),
                                shared::EquipSlot::Ring   => session.equipment.ring.is_none(),
                            };
                            if empty {
                                match slot {
                                    shared::EquipSlot::Weapon => session.equipment.weapon = Some(ltag.item_id),
                                    shared::EquipSlot::Armor  => session.equipment.armor  = Some(ltag.item_id),
                                    shared::EquipSlot::Ring   => session.equipment.ring   = Some(ltag.item_id),
                                }
                                session.stats = effective_stats(session.class, &session.equipment);
                                session.stats_dirty = true;
                                if let Some(pe) = session.entity {
                                    hp_max_updates.push((pe, session.stats.hp_max));
                                }
                                picked.push((le, leid));
                                continue 'loot_loop;
                            }
                        }
                        // Senao, inventario normal.
                        if add_to_inventory(&mut session.inventory, ltag.item_id, ltag.qty) {
                            session.inventory_dirty = true;
                            picked.push((le, leid));
                            continue 'loot_loop;
                        }
                    }
                }
            }
        }
        for (pe, new_max) in hp_max_updates {
            if let Ok(mut hp) = self.ecs.get::<&mut Health>(pe) {
                hp.max = new_max;
            }
        }
        for (e, eid) in picked {
            let _ = self.ecs.despawn(e);
            self.removed_this_tick.push(eid);
        }

        // ── K: TTL de projeteis ───────────────────────────────────────────────
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
            if session.inventory_dirty {
                session.inventory_dirty = false;
                let _ = session
                    .handle
                    .to_client
                    .send(ServerMessage::InventoryUpdate {
                        slots: session.inventory.clone(),
                    });
            }
            if session.vault_dirty {
                session.vault_dirty = false;
                let _ = session
                    .handle
                    .to_client
                    .send(ServerMessage::VaultUpdate {
                        slots: session.vault.clone(),
                    });
            }
            // DownedUpdate: envia entrada/saida + updates com quantizacao do timer
            // pra evitar spam (quantizado em inteiro de segundo).
            let timer_q = session.downed_heal_timer.ceil() as i32;
            let snap = (session.downed, session.downed_hp, timer_q);
            if snap != session.downed_last_sent {
                session.downed_last_sent = snap;
                let _ = session.handle.to_client.send(ServerMessage::DownedUpdate {
                    active: session.downed,
                    dhp: session.downed_hp,
                    dhp_max: shared::DOWNED_HP_MAX,
                    timer_s: session.downed_heal_timer.max(0.0),
                });
            }
            if session.stats_dirty {
                session.stats_dirty = false;
                let _ = session
                    .handle
                    .to_client
                    .send(ServerMessage::StatsUpdate {
                        class: session.class,
                        stats: session.stats,
                        equipment: session.equipment,
                    });
            }
            let cur_i = session.mp_current as i32;
            if cur_i != session.mp_last_sent {
                session.mp_last_sent = cur_i;
                let _ = session.handle.to_client.send(ServerMessage::ManaUpdate {
                    current: cur_i,
                });
            }
            let stam_i = session.stamina_current as i32;
            if stam_i != session.stamina_last_sent {
                session.stamina_last_sent = stam_i;
                let _ = session.handle.to_client.send(ServerMessage::StaminaUpdate {
                    current: stam_i,
                });
            }
        }
    }
}

impl GameWorld {
    /// Monta linhas de persistencia com o estado atual de TODOS os jogadores
    /// logados. Tambem atualiza o cache em memoria pra que o proximo login
    /// (antes do DB terminar de gravar) ja veja dados novos.
    pub fn collect_character_rows(&mut self) -> Vec<crate::persistence::CharacterRow> {
        let mut out = Vec::with_capacity(self.sessions.len());
        let mut entries: Vec<(String, Vec2, Health, u64, Vec<shared::InventorySlot>, shared::Equipment, Vec<shared::InventorySlot>)> = Vec::new();
        for session in self.sessions.values() {
            if !session.logged_in { continue; }
            let Some(e) = session.entity else { continue };
            let pos = match self.ecs.get::<&Position>(e) { Ok(p) => p.0, Err(_) => continue };
            let hp = match self.ecs.get::<&Health>(e) { Ok(h) => *h, Err(_) => continue };
            entries.push((
                session.name.clone(),
                pos,
                hp,
                session.xp,
                session.inventory.clone(),
                session.equipment,
                session.vault.clone(),
            ));
        }
        for (name, pos, hp, xp, inventory, equipment, vault) in entries {
            let row = crate::persistence::CharacterRow {
                name: name.clone(),
                pos,
                hp,
                xp,
                inventory,
                equipment,
                vault,
            };
            self.characters.insert(name, row.clone());
            out.push(row);
        }
        out
    }

    /// Swap generico entre dois spots (inv slot <-> inv slot, ou inv <-> equip).
    /// Valida compatibilidade quando um dos lados e equipamento. Se inv slot
    /// tem stack >1 tentando equipar, split nao e suportado — rejeita.
    fn handle_inventory_swap(&mut self, sid: SessionId, a: InvSpot, b: InvSpot) {
        let Some(session) = self.sessions.get_mut(&sid) else { return };
        if !session.logged_in { return; }
        if a == b { return; }

        // Le valores atuais
        let read_inv = |idx: u16| -> shared::InventorySlot {
            session.inventory.get(idx as usize).copied().unwrap_or_default()
        };
        let read_eq  = |s: shared::EquipSlot| -> Option<u16> {
            match s {
                shared::EquipSlot::Weapon => session.equipment.weapon,
                shared::EquipSlot::Armor  => session.equipment.armor,
                shared::EquipSlot::Ring   => session.equipment.ring,
            }
        };

        let va = match a {
            InvSpot::Inv(i)   => (Some(read_inv(i)), None),
            InvSpot::Equip(s) => (None, Some((s, read_eq(s)))),
        };
        let vb = match b {
            InvSpot::Inv(i)   => (Some(read_inv(i)), None),
            InvSpot::Equip(s) => (None, Some((s, read_eq(s)))),
        };

        // Helpers de validacao
        let can_go_into_equip = |slot: shared::EquipSlot, item: &shared::InventorySlot| -> bool {
            if item.qty == 0 { return true; } // tirar do equip pra slot vazio e OK
            if item.qty > 1 { return false; } // stacks nao equipaveis
            shared::equip_slot_of(item.item_id) == Some(slot)
        };

        match (a, b) {
            // inv <-> inv: swap direto
            (InvSpot::Inv(ai), InvSpot::Inv(bi)) => {
                let (Some(ia), _) = va else { return };
                let (Some(ib), _) = vb else { return };
                // Stack-merge quando ambos tem o mesmo item_id: junta b em a.
                if ia.qty > 0 && ib.qty > 0 && ia.item_id == ib.item_id {
                    let cap = shared::item_stack_max(ia.item_id);
                    let move_qty = (cap - ib.qty).min(ia.qty);
                    if move_qty > 0 {
                        let na = ia.qty - move_qty;
                        let nb = ib.qty + move_qty;
                        session.inventory[ai as usize] = if na == 0 {
                            shared::InventorySlot::default()
                        } else {
                            shared::InventorySlot { item_id: ia.item_id, qty: na }
                        };
                        session.inventory[bi as usize] = shared::InventorySlot { item_id: ia.item_id, qty: nb };
                        session.inventory_dirty = true;
                        return;
                    }
                }
                session.inventory[ai as usize] = ib;
                session.inventory[bi as usize] = ia;
                session.inventory_dirty = true;
            }
            // inv -> equip
            (InvSpot::Inv(ai), InvSpot::Equip(bs)) => {
                let (Some(ia), _) = va else { return };
                let (_, Some((_, cur_eq))) = vb else { return };
                if !can_go_into_equip(bs, &ia) { return; }
                // Coloca item do inv no equip; devolve o antigo do equip pro inv.
                let new_inv_slot = match cur_eq {
                    Some(old_id) => shared::InventorySlot { item_id: old_id, qty: 1 },
                    None         => shared::InventorySlot::default(),
                };
                set_equip(session, bs, if ia.qty > 0 { Some(ia.item_id) } else { None });
                session.inventory[ai as usize] = new_inv_slot;
                session.stats = effective_stats(session.class, &session.equipment);
                session.stats_dirty = true;
                session.inventory_dirty = true;
            }
            // equip -> inv (simetrico)
            (InvSpot::Equip(as_), InvSpot::Inv(bi)) => {
                let (Some(ib), _) = vb else { return };
                let (_, Some((_, cur_eq))) = va else { return };
                if !can_go_into_equip(as_, &ib) { return; }
                let new_inv_slot = match cur_eq {
                    Some(old_id) => shared::InventorySlot { item_id: old_id, qty: 1 },
                    None         => shared::InventorySlot::default(),
                };
                set_equip(session, as_, if ib.qty > 0 { Some(ib.item_id) } else { None });
                session.inventory[bi as usize] = new_inv_slot;
                session.stats = effective_stats(session.class, &session.equipment);
                session.stats_dirty = true;
                session.inventory_dirty = true;
            }
            // equip <-> equip: so faz sentido se slots sao iguais (no-op)
            _ => {}
        }
    }

    /// Move um item do inv[inv_slot] pro primeiro slot livre (ou stack) do vault.
    fn handle_vault_deposit(&mut self, sid: SessionId, inv_slot: usize) {
        let Some(session) = self.sessions.get_mut(&sid) else { return };
        if !session.logged_in { return; }
        if inv_slot >= session.inventory.len() { return; }
        let src = session.inventory[inv_slot];
        if src.qty == 0 { return; }
        // Tenta stackar em slot existente do vault com mesmo item_id
        let stack_max = shared::item_stack_max(src.item_id);
        let mut moved = false;
        for slot in session.vault.iter_mut() {
            if slot.qty > 0 && slot.item_id == src.item_id && slot.qty < stack_max {
                let can_add = (stack_max - slot.qty).min(src.qty);
                slot.qty += can_add;
                if let Some(iv) = session.inventory.get_mut(inv_slot) {
                    iv.qty = iv.qty.saturating_sub(can_add);
                    if iv.qty == 0 { *iv = shared::InventorySlot::default(); }
                }
                moved = true;
                break;
            }
        }
        if !moved {
            // Primeiro slot livre
            if let Some(empty) = session.vault.iter_mut().find(|s| s.qty == 0) {
                *empty = src;
                session.inventory[inv_slot] = shared::InventorySlot::default();
                moved = true;
            }
        }
        if moved {
            session.vault_dirty = true;
            session.inventory_dirty = true;
        }
    }

    /// Move um item do vault[vault_slot] pro primeiro slot livre (ou stack) do inv.
    fn handle_vault_withdraw(&mut self, sid: SessionId, vault_slot: usize) {
        let Some(session) = self.sessions.get_mut(&sid) else { return };
        if !session.logged_in { return; }
        if vault_slot >= session.vault.len() { return; }
        let src = session.vault[vault_slot];
        if src.qty == 0 { return; }
        let stack_max = shared::item_stack_max(src.item_id);
        let mut moved = false;
        for slot in session.inventory.iter_mut() {
            if slot.qty > 0 && slot.item_id == src.item_id && slot.qty < stack_max {
                let can_add = (stack_max - slot.qty).min(src.qty);
                slot.qty += can_add;
                if let Some(vv) = session.vault.get_mut(vault_slot) {
                    vv.qty = vv.qty.saturating_sub(can_add);
                    if vv.qty == 0 { *vv = shared::InventorySlot::default(); }
                }
                moved = true;
                break;
            }
        }
        if !moved {
            if let Some(empty) = session.inventory.iter_mut().find(|s| s.qty == 0) {
                *empty = src;
                session.vault[vault_slot] = shared::InventorySlot::default();
                moved = true;
            }
        }
        if moved {
            session.vault_dirty = true;
            session.inventory_dirty = true;
        }
    }

    /// Processa pedido do cliente pra sair do Downed State. So aceito
    /// quando o timer de readiness ja zerou.
    fn handle_stand_up(&mut self, sid: SessionId) {
        let Some(session) = self.sessions.get_mut(&sid) else { return };
        if !session.logged_in { return; }
        if !session.downed { return; }
        if session.downed_heal_timer > 0.0 { return; }
        let Some(entity) = session.entity else { return };
        let revive_hp = ((session.stats.hp_max as f32)
            * shared::DOWNED_REVIVE_HP_PCT)
            .round()
            .max(1.0) as i32;
        session.downed = false;
        session.downed_heal_timer = 0.0;
        session.downed_hp = 0;
        tracing::info!("{} se levantou ({}hp)", session.name, revive_hp);
        if let Ok(mut hp) = self.ecs.get::<&mut Health>(entity) {
            hp.current = revive_hp;
        }
        let _ = self.ecs.remove_one::<Untargetable>(entity);
    }

    fn handle_interact(&mut self, sid: SessionId) {
        let Some(session) = self.sessions.get(&sid) else { return };
        if !session.logged_in { return; }
        let Some(player_entity) = session.entity else { return };
        let player_pos = match self.ecs.get::<&Position>(player_entity) {
            Ok(p) => p.0,
            Err(_) => return,
        };
        let handle = session.handle.clone();
        // Procura NPC mais proximo em INTERACT_RADIUS
        let r_sq = shared::INTERACT_RADIUS * shared::INTERACT_RADIUS;
        let mut best: Option<(u16, f32)> = None;
        for (_, (p, k)) in self.ecs.query::<(&Position, &EntityKind)>().iter() {
            if let EntityKind::Npc(n) = k {
                let d2 = p.0.distance_squared(player_pos);
                if d2 <= r_sq {
                    if best.map(|(_, bd)| d2 < bd).unwrap_or(true) {
                        best = Some((*n, d2));
                    }
                }
            }
        }
        match best {
            Some((2, _)) => {
                // Vault NPC — abre o vault.
                let slots = self
                    .sessions.get(&sid)
                    .map(|s| s.vault.clone())
                    .unwrap_or_default();
                let _ = handle.to_client.send(ServerMessage::VaultOpen { slots });
            }
            Some(_) => {
                // Qualquer outro NPC = vendedor.
                let items: Vec<(u16, u32)> = shared::SHOP_ITEMS.to_vec();
                let _ = handle.to_client.send(ServerMessage::ShopOpen { items });
            }
            None => {}
        }
    }

    fn handle_shop_buy(&mut self, sid: SessionId, slot_idx: usize) {
        if slot_idx >= shared::SHOP_ITEMS.len() { return; }
        let (item_id, price) = shared::SHOP_ITEMS[slot_idx];

        // 1) Valida sessao + proximidade (borrow imutavel da ECS)
        let (player_entity, player_pos) = match self.sessions.get(&sid) {
            Some(s) if s.logged_in => match s.entity {
                Some(e) => match self.ecs.get::<&Position>(e) {
                    Ok(p) => (e, p.0),
                    Err(_) => return,
                },
                None => return,
            },
            _ => return,
        };
        let r_sq = shared::INTERACT_RADIUS * shared::INTERACT_RADIUS;
        let near_vendor = self
            .ecs
            .query::<(&Position, &EntityKind)>()
            .iter()
            .any(|(_, (p, k))| {
                matches!(k, EntityKind::Npc(_))
                    && p.0.distance_squared(player_pos) <= r_sq
            });
        if !near_vendor { return; }

        // 2) Muta sessao + calcula se precisa atualizar Health.max
        let new_hp_max: Option<i32> = {
            let Some(session) = self.sessions.get_mut(&sid) else { return };

            // 2a) Verifica ouro
            let gold_idx = session
                .inventory
                .iter()
                .position(|s| s.qty > 0 && s.item_id == shared::item_id::GOLD);
            let Some(gi) = gold_idx else {
                let _ = session.handle.to_client.send(ServerMessage::Chat {
                    from: "SHOP".into(),
                    text: "sem ouro".into(),
                });
                return;
            };
            if session.inventory[gi].qty < price {
                let _ = session.handle.to_client.send(ServerMessage::Chat {
                    from: "SHOP".into(),
                    text: format!("precisa de {price} ouro"),
                });
                return;
            }

            // 2b) Coloca o item (equipa se puder, senao inventario)
            let equip = shared::equip_slot_of(item_id);
            let mut new_max: Option<i32> = None;
            let placed = if let Some(es) = equip {
                let empty = match es {
                    shared::EquipSlot::Weapon => session.equipment.weapon.is_none(),
                    shared::EquipSlot::Armor  => session.equipment.armor.is_none(),
                    shared::EquipSlot::Ring   => session.equipment.ring.is_none(),
                };
                if empty {
                    match es {
                        shared::EquipSlot::Weapon => session.equipment.weapon = Some(item_id),
                        shared::EquipSlot::Armor  => session.equipment.armor  = Some(item_id),
                        shared::EquipSlot::Ring   => session.equipment.ring   = Some(item_id),
                    }
                    session.stats = effective_stats(session.class, &session.equipment);
                    session.stats_dirty = true;
                    new_max = Some(session.stats.hp_max);
                    true
                } else {
                    add_to_inventory(&mut session.inventory, item_id, 1)
                }
            } else {
                add_to_inventory(&mut session.inventory, item_id, 1)
            };
            if !placed {
                let _ = session.handle.to_client.send(ServerMessage::Chat {
                    from: "SHOP".into(),
                    text: "inventario cheio".into(),
                });
                return;
            }

            // 2c) Cobra ouro e sinaliza update
            session.inventory[gi].qty -= price;
            if session.inventory[gi].qty == 0 {
                session.inventory[gi] = shared::InventorySlot::default();
            }
            session.inventory_dirty = true;
            let _ = session.handle.to_client.send(ServerMessage::Chat {
                from: "SHOP".into(),
                text: format!("comprou item {item_id} por {price} ouro"),
            });
            new_max
        };

        // 3) Atualiza Health.max no ECS se equipou algo que mudou hp_max
        if let Some(nmax) = new_hp_max {
            if let Ok(mut hp) = self.ecs.get::<&mut Health>(player_entity) {
                hp.max = nmax;
            }
        }
    }

    fn handle_use_item(&mut self, sid: SessionId, slot_idx: usize) {
        // Extrai estado + identifica a acao fora do borrow mutavel do ECS.
        enum UseAction {
            Equip { new_hp_max: i32 },
            HealHp(i32),
            HealMp(i32),
            HealStam(i32),
        }
        let (player_entity, action) = {
            let Some(session) = self.sessions.get_mut(&sid) else { return };
            if !session.logged_in { return; }
            if slot_idx >= session.inventory.len() { return; }
            let slot = session.inventory[slot_idx];
            if slot.qty == 0 { return; }
            let Some(player_entity) = session.entity else { return };

            // Equipavel: swap entre inventario e slot de equip correspondente.
            if let Some(es) = shared::equip_slot_of(slot.item_id) {
                let old = match es {
                    shared::EquipSlot::Weapon => session.equipment.weapon,
                    shared::EquipSlot::Armor  => session.equipment.armor,
                    shared::EquipSlot::Ring   => session.equipment.ring,
                };
                let new_id = slot.item_id;
                match es {
                    shared::EquipSlot::Weapon => session.equipment.weapon = Some(new_id),
                    shared::EquipSlot::Armor  => session.equipment.armor  = Some(new_id),
                    shared::EquipSlot::Ring   => session.equipment.ring   = Some(new_id),
                }
                session.inventory[slot_idx] = match old {
                    Some(old_id) => shared::InventorySlot { item_id: old_id, qty: 1 },
                    None         => shared::InventorySlot::default(),
                };
                session.stats = effective_stats(session.class, &session.equipment);
                session.stats_dirty = true;
                session.inventory_dirty = true;
                (player_entity, UseAction::Equip { new_hp_max: session.stats.hp_max })
            } else {
                let a = match slot.item_id {
                    id if id == shared::item_id::HEALTH_POTION  => UseAction::HealHp(50),
                    id if id == shared::item_id::GREATER_HEAL   => UseAction::HealHp(150),
                    id if id == shared::item_id::MANA_POTION    => UseAction::HealMp(50),
                    id if id == shared::item_id::GREATER_MANA   => UseAction::HealMp(100),
                    id if id == shared::item_id::STAMINA_POTION => UseAction::HealStam(100),
                    _ => return,
                };
                (player_entity, a)
            }
        };

        // Helper: consome 1 do slot (usado apos heal bem-sucedido).
        let consume_slot = |sessions: &mut HashMap<SessionId, Session>| {
            if let Some(session) = sessions.get_mut(&sid) {
                if slot_idx < session.inventory.len() {
                    let s = &mut session.inventory[slot_idx];
                    if s.qty > 0 {
                        s.qty -= 1;
                        if s.qty == 0 { *s = shared::InventorySlot::default(); }
                        session.inventory_dirty = true;
                    }
                }
            }
        };

        match action {
            UseAction::Equip { new_hp_max } => {
                if let Ok(mut hp) = self.ecs.get::<&mut Health>(player_entity) {
                    hp.max = new_hp_max;
                }
            }
            UseAction::HealHp(amount) => {
                let healed = if let Ok(mut hp) = self.ecs.get::<&mut Health>(player_entity) {
                    if hp.current < hp.max {
                        hp.current = (hp.current + amount).min(hp.max);
                        true
                    } else { false }
                } else { false };
                if healed { consume_slot(&mut self.sessions); }
            }
            UseAction::HealMp(amount) => {
                let mp_max = self.sessions.get(&sid).map(|s| s.stats.mp_max).unwrap_or(100);
                let healed = if let Some(session) = self.sessions.get_mut(&sid) {
                    if (session.mp_current as i32) < mp_max {
                        session.mp_current = ((session.mp_current as i32 + amount).min(mp_max)) as f32;
                        let _ = session.handle.to_client.send(
                            ServerMessage::ManaUpdate { current: session.mp_current as i32 });
                        true
                    } else { false }
                } else { false };
                if healed { consume_slot(&mut self.sessions); }
            }
            UseAction::HealStam(amount) => {
                let stam_max = shared::STAMINA_MAX;
                let healed = if let Some(session) = self.sessions.get_mut(&sid) {
                    if (session.stamina_current as i32) < stam_max {
                        session.stamina_current = ((session.stamina_current as i32 + amount).min(stam_max)) as f32;
                        let _ = session.handle.to_client.send(
                            ServerMessage::StaminaUpdate { current: session.stamina_current as i32 });
                        true
                    } else { false }
                } else { false };
                if healed { consume_slot(&mut self.sessions); }
            }
        }
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
            inventory: session.inventory.clone(),
            equipment: session.equipment,
            vault: session.vault.clone(),
        };
        self.characters.insert(session.name.clone(), row.clone());
        Some(row)
    }
}

/// Aplica `item_id` (ou None para remover) num slot de equipamento da sessao.
fn set_equip(session: &mut Session, slot: shared::EquipSlot, item_id: Option<u16>) {
    match slot {
        shared::EquipSlot::Weapon => session.equipment.weapon = item_id,
        shared::EquipSlot::Armor  => session.equipment.armor  = item_id,
        shared::EquipSlot::Ring   => session.equipment.ring   = item_id,
    }
}

/// Calcula stats efetivos = stats base da classe + bonus dos equipamentos.
fn effective_stats(class: shared::PlayerClass, equip: &shared::Equipment) -> shared::PlayerStats {
    let mut s = class.base_stats();
    for opt in [equip.weapon, equip.armor, equip.ring] {
        if let Some(id) = opt {
            let b = shared::item_bonus(id);
            s.hp_max += b.hp_max;
            s.mp_max += b.mp_max;
            s.attack_damage += b.attack_damage;
            s.dex += b.dex;
            s.wis += b.wis;
        }
    }
    // Garantir minimos
    s.hp_max = s.hp_max.max(1);
    s.mp_max = s.mp_max.max(0);
    s.attack_damage = s.attack_damage.max(1);
    s
}

/// Tenta adicionar um item ao inventario. Stacka em slots existentes primeiro;
/// se nao couber, procura slot vazio. Retorna true se coube (parcial ou total
/// dentro do stack do primeiro slot achado — se nao couber NADA, retorna false).
fn add_to_inventory(inv: &mut [shared::InventorySlot], item_id: u16, mut qty: u32) -> bool {
    let max_stack = shared::item_stack_max(item_id);
    // 1) stacka em slots existentes
    for slot in inv.iter_mut() {
        if slot.qty > 0 && slot.item_id == item_id && slot.qty < max_stack {
            let room = max_stack - slot.qty;
            let add = qty.min(room);
            slot.qty += add;
            qty -= add;
            if qty == 0 { return true; }
        }
    }
    // 2) slot vazio
    while qty > 0 {
        let Some(empty) = inv.iter_mut().find(|s| s.qty == 0) else { break };
        let add = qty.min(max_stack);
        empty.item_id = item_id;
        empty.qty = add;
        qty -= add;
    }
    qty == 0
}

/// Retorna lista de (item_id, qty) a dropar quando o inimigo de `kind` morre.
fn enemy_loot_drops(kind: u16, seed: u64) -> Vec<(u16, u32)> {
    use shared::item_id;
    let r  = lcg_f32(seed);
    let r2 = lcg_f32(lcg(seed));
    let r3 = lcg_f32(lcg(lcg(seed)));
    let r4 = lcg_f32(lcg(lcg(lcg(seed))));
    let r5 = lcg_f32(lcg(lcg(lcg(lcg(seed)))));
    match kind {
        // Boss: ouro alto + varios equipaveis + raridades
        7 => {
            let mut drops = vec![
                (item_id::GOLD,           200 + (r * 300.0) as u32),
                (item_id::DRAGON_SCALE,   1 + (r2 * 3.0) as u32),
                (item_id::GREATER_HEAL,   2 + (r3 * 3.0) as u32),
                (item_id::GREATER_MANA,   2),
            ];
            // Equipamentos poderosos
            if r  < 0.55 { drops.push((item_id::GREAT_SWORD, 1)); }
            if r2 < 0.55 { drops.push((item_id::WAND, 1)); }
            if r3 < 0.50 { drops.push((item_id::PLATE_ARMOR, 1)); }
            if r4 < 0.55 { drops.push((item_id::ROBE, 1)); }
            if r5 < 0.45 { drops.push((item_id::LUCKY_RING, 1)); }
            drops
        }
        // Berserker: pesado, drops de armadura
        5 => {
            let mut drops = vec![(item_id::GOLD, 25 + (r * 35.0) as u32)];
            if r2 < 0.22 { drops.push((item_id::PLATE_ARMOR, 1)); }
            else if r2 < 0.45 { drops.push((item_id::ARMOR, 1)); }
            if r3 < 0.30 { drops.push((item_id::GREATER_HEAL, 1)); }
            if r4 < 0.20 { drops.push((item_id::IRON_INGOT, 1 + (r5 * 2.0) as u32)); }
            drops
        }
        // Mago: staff/wand + mana potions
        4 => {
            let mut drops = vec![(item_id::GOLD, 15 + (r * 20.0) as u32)];
            if r2 < 0.22 { drops.push((item_id::WAND, 1)); }
            else if r2 < 0.45 { drops.push((item_id::STAFF, 1)); }
            if r3 < 0.35 { drops.push((item_id::MANA_POTION, 1 + (r4 * 2.0) as u32)); }
            if r4 < 0.18 { drops.push((item_id::ROBE, 1)); }
            if r5 < 0.12 { drops.push((item_id::GEM, 1)); }
            drops
        }
        // Tank: armadura pesada
        1 => {
            let mut drops = vec![(item_id::GOLD, 15 + (r * 25.0) as u32)];
            if r2 < 0.18 { drops.push((item_id::SHIELD, 1)); }
            if r3 < 0.25 { drops.push((item_id::ARMOR, 1)); }
            else if r3 < 0.35 { drops.push((item_id::SWORD, 1)); }
            if r4 < 0.30 { drops.push((item_id::HEALTH_POTION, 1 + (r5 * 2.0) as u32)); }
            if r5 < 0.15 { drops.push((item_id::IRON_INGOT, 1)); }
            drops
        }
        // Ranger/Arqueiro: bow + acessorios
        2 | 6 => {
            let mut drops = vec![(item_id::GOLD, 10 + (r * 18.0) as u32)];
            if r2 < 0.20 { drops.push((item_id::BOW, 1)); }
            if r3 < 0.25 {
                if r4 < 0.5 { drops.push((item_id::RING, 1)); }
                else        { drops.push((item_id::AMULET, 1)); }
            }
            if r4 < 0.35 { drops.push((item_id::STAMINA_POTION, 1)); }
            drops
        }
        // Ninja: rapidez, drops leves
        3 => {
            let mut drops = vec![(item_id::GOLD, 8 + (r * 14.0) as u32)];
            if r2 < 0.25 { drops.push((item_id::DAGGER, 1)); }
            if r3 < 0.22 { drops.push((item_id::LEATHER_ARMOR, 1)); }
            if r4 < 0.35 { drops.push((item_id::MANA_POTION, 1)); }
            if r5 < 0.10 { drops.push((item_id::LUCKY_RING, 1)); }
            drops
        }
        // Grunt + fallback: base
        _ => {
            let mut drops = vec![(item_id::GOLD, 4 + (r * 10.0) as u32)];
            if r2 < 0.25 { drops.push((item_id::HEALTH_POTION, 1)); }
            if r3 < 0.15 { drops.push((item_id::MANA_POTION, 1)); }
            if r4 < 0.08 { drops.push((item_id::IRON_INGOT, 1)); }
            drops
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
