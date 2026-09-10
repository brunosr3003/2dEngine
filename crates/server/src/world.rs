//! Mundo autoritativo — Fase 3: combate, morte, respawn, inimigos com IA.

use glam::Vec2;
use hecs::{Entity, World};
use shared::protocol::{buttons, ClientMessage, InputFrame, InvSpot, ServerMessage, WorldSnapshot};
use shared::mapfile::{MapEntity, MapFile};
use shared::{
    EntityId, EntityKind, EntityMeta, EntityState, Health, PlayerId, Position, Velocity,
    AOI_RADIUS, ATTACK_COOLDOWN, ENEMY_START_COUNT, ENTITY_RADIUS, PLAYER_SPEED,
    PROJ_RADIUS, PROJ_SPEED, PROJ_TTL, RESPAWN_DELAY, SPATIAL_CELL_SIZE,
    BOSS_RESPAWN_DELAY,
};
use std::collections::HashMap;

/// De quantos em quantos ticks um mob reescolhe o alvo.
///
/// 6 ticks = 5 decisoes por segundo. Mob nao precisa de 30: o mercado roda IA
/// de MMO entre 2 e 5 Hz. O `+ id` no modulo espalha as decisoes entre os
/// ticks pra nao concentrar tudo no mesmo quadro.
const AI_DECISAO_TICKS: u32 = 6;
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
    /// Char criado via CharacterCreationUI — task async ja escreveu no DB
    /// e leu de volta o row recem-criado. World insere no cache + roda
    /// on_auth_result com AuthSuccess sintetico.
    CharCreated(SessionId, Box<crate::persistence::CharacterRow>, crate::auth::AuthSuccess),
    /// SelectCharacter de um char que NÃO estava no cache (ex: criado em outro
    /// processo — char novo criado no :9000 e selecionado no :9001 de tutorial).
    /// Recarregado do DB; world insere no cache e spawna direto.
    CharReloadedForSelect(SessionId, Box<crate::persistence::CharacterRow>, crate::auth::AuthSuccess),
}

#[derive(Debug, Clone, Copy)]
pub struct NetId(pub EntityId);

pub struct ProjTag {
    pub owner: EntityId,
    pub from_player: bool,
    pub ttl: f32,
    pub damage: i32,
    /// True se o roll de crit ocorreu na hora do disparo. Aplica multiplier
    /// `damage` e setta is_crit no snapshot do alvo quando hit.
    pub is_crit: bool,
    /// Visual: 0 = arrow (default), 1 = fireball/magic.
    /// Replicado pro cliente via EntitySnapshot.sprite_id.
    pub kind: u8,
}

/// Tiro ranged pendente — agendado no input mas só spawna como projétil real
/// no tick em que `release_tick` for atingido. Usado pra sincronizar o
/// surgimento do projétil com o frame de release da animação de saque do arco
/// /cajado, em vez de cuspir o projétil instantâneo no clique.
struct PendingShot {
    pos: Vec2,
    dir: Vec2,
    damage: i32,
    is_crit: bool,
    /// Visual + behavior kind:
    ///   0 = arrow (bow basic), 1 = fireball, 2 = lightning, 3 = big fireball,
    ///   4 = frost bolt, 5 = elec, 6 = power shot arrow, 7 = spear (harpoon —
    ///   ao acertar set caster.harpoon state pra recast pull).
    kind: u8,
    owner_id: EntityId,
    from_player: bool,
    release_tick: u32,
}

/// Hit instant de skill (line/aoe/cone) enfileirado durante on_message;
/// processado no step() junto com damage_events das outras fontes.
#[derive(Clone, Copy)]
struct PendingSkillHit {
    target_net: EntityId,
    damage: i32,
    attacker_net: EntityId,
    hurt_dir: Vec2,
    is_crit: bool,
    from_player: bool,
    /// Knockback em tiles vindo do SkillDef.knockback (ou hardcoded para
    /// skills que nao consultam DB). Aplicado como -hurt_dir × kb.
    knockback: f32,
}

/// Heal aplicado em players (self/aliado) por skill; processado em step().
#[derive(Clone, Copy)]
struct PendingHeal {
    target_net: EntityId,
    amount: i32,
}

/// AOE skill agendada com cast_time_s — aplica damage só quando release_tick
/// for atingido (sincroniza com animação de queda no cliente, ex: Meteor).
#[derive(Clone, Copy)]
struct DelayedAoe {
    target_pos: Vec2,
    radius: f32,
    damage: i32,
    owner_eid: EntityId,
    release_tick: u32,
    /// Quando true, hits aplicam status `poisoned_until = now + poison_dur_s`
    /// no EnemyTag (replicado pra client por snapshot pra render tint verde).
    /// Set por skills de DOT-tipo Smoke Bomb. Default false (sem status).
    poison_dur_s: f32,
    /// Knockback em tiles vindo do SkillDef.knockback. Aplicado em cada hit
    /// na direcao oposta ao centro do AoE.
    knockback: f32,
    /// Quando true, target_pos eh ignorado e o AoE usa a pos ATUAL do owner
    /// no momento do release. Permite Whirlwind/Sword Dance seguirem o
    /// player enquanto ele anda durante o spin (ataque rotativo movel).
    follow_owner: bool,
    /// Stun aplicado nos hits (Caltrops trap). 0 = sem stun.
    stun_dur_s: f32,
}

pub struct EnemyTag {
    /// Alvo escolhido pela IA, revalidado a cada `AI_DECISAO_TICKS`.
    ///
    /// Escolher alvo e' um `min_by` sobre TODOS os jogadores. Rodando por mob
    /// e por tick isso e' O(mobs x jogadores) — com 1000 mobs e 200 jogadores
    /// sao 200 mil distancias por tick, 6 milhoes por segundo. A escolha e'
    /// cara e muda pouco; a POSICAO do alvo escolhido continua sendo lida
    /// todo tick, por id, em O(1).
    pub ai_target: Option<EntityId>,
    pub attack_cooldown: f32,
    /// sim_time absoluto até o qual o enemy fica em stagger (sem mover/atacar).
    /// Setado em damage hits. 0 = não está em hurt.
    pub hurt_until: f32,
    /// Vetor unitário do alvo TOWARD o último atacante. Persiste pra knockback
    /// e outros efeitos direcionais. Reset somente em novo hit.
    pub hurt_dir: Vec2,
    /// True quando HP=0 (enemy entrou em estado de cadáver). Skipa IA, dano
    /// e drops adicionais. Real despawn quando sim_time >= despawn_at.
    pub dead: bool,
    /// sim_time alvo pra despawn real do cadáver (HP_zero_time + ENEMY_CORPSE_LINGER).
    pub despawn_at: f32,
    pub wander_timer: f32,
    pub wander_dir: Vec2,
    /// Setado true no tick em que o inimigo dispara um swing. Snapshot loop
    /// lê e clear pra que o cliente toque a anim de ataque uma vez.
    pub attack_pending: bool,
    /// Waypoint atual durante walk phase. Inimigo anda em direção a esse ponto
    /// até chegar (< 0.3 tiles) ou wander_timer expirar.
    pub wander_waypoint: Vec2,
    /// Ponto "home" — centro da spawn zone ou posicao de spawn inicial.
    /// Usado como ancora pelo leash: inimigo sempre volta se afastar demais.
    pub spawn_anchor: Vec2,
    /// Raio max do leash em tiles (0 = desabilitado, wander livre).
    /// Inimigos podem se afastar até 2x esse valor antes de serem forçados de volta.
    pub leash_max: f32,
    /// 0 = walking, 1 = paused. Alterna a cada wander_timer expirado.
    pub wander_phase: u8,
    /// Quando true, inimigo dropou aggro e ignora players até chegar bem perto
    /// do anchor. Setado quando atravessa leash_hard ou aggro_timer estoura.
    pub returning_home: bool,
    /// Segundos chase ativos sem dar dano. Reset quando ataca com sucesso.
    /// Ao passar de AGGRO_DROP_TIME, inimigo desiste e volta pra casa.
    pub aggro_timer: f32,
    /// sim_time absoluto até o qual o enemy fica em "spawn grace" — cliente
    /// roda VFX de invocação (~0.84s) com o enemy invisível e estático.
    /// Durante esse intervalo: sem mover, sem atacar, sem tomar dano.
    pub spawn_grace_until: f32,
    /// sim_time até quando o inimigo está envenenado (visual tint verde
    /// no cliente). Setado por hits de Smoke Bomb (1038). 0 = não envenenado.
    /// Status visual; o dano de fato vem dos pulses da DelayedAoe queue.
    pub poisoned_until: f32,
    /// sim_time ate quando o inimigo esta atordoado (Shield Bash 1003).
    /// Bloqueia AI/movement/atacks no while_active. Cliente renderiza tint
    /// amarelo + parado.
    pub stunned_until: f32,
    /// sim_time ate quando o inimigo tem aggro forçado em outro target
    /// (Taunt 1006). Enemy AI ignora target preferido e mira esse player.
    /// 0 = sem taunt ativo.
    pub forced_aggro_until: f32,
    /// EntityId do player que tauntou (alvo forçado durante forced_aggro_until).
    /// None = sem taunt ativo.
    pub forced_aggro_target: Option<EntityId>,
    /// Stats efetivos derivados do EnemyBuild (level + equip + alocados +
    /// profs + skills aprendidas) via `effective_stats()`. Cacheados no spawn
    /// — fonte da verdade pra HP_max, attack_damage, defense, etc. de combat.
    /// Substitui leitura direta de `enemy_def(kind).attack_damage`/`defense`.
    pub stats: shared::PlayerStats,
    /// Equipamento do enemy (paper-doll ja vai puxar daqui na fase 3).
    pub equipment: shared::Equipment,
    /// Skills aprendidas — fase 4 AI escolhe entre auto-attack e skill cast.
    pub learned_skills: Vec<shared::LearnedSkill>,
    /// Level total do build (pra escalonamento futuro de loot/xp).
    pub level: u32,
    /// Visual do paper-doll. Replicado pro client renderizar enemy como
    /// humanoide com a equipa certa em vez de sprite/quadrado tinted.
    pub visual: shared::VisualConfig,
    /// MP atual (regen passivo, gasto em skill cast). Cap = stats.mp_max.
    pub mp_current: f32,
    /// Cooldowns por skill_id — sim_time absoluto quando a skill volta.
    /// Skill so pode ser usada se now >= ready_at.
    pub skill_cds: std::collections::HashMap<u32, f32>,
    // ── Comportamento AI cacheado do EnemyBuild ─────────────────────────
    pub attack_cooldown_base: f32,
    pub attack_range: f32,
    pub detect_range: f32,
    pub locomotor_speed: f32,
    pub kite_dist: Option<f32>,
    pub proj_count: u32,
    pub proj_kind: u8,
    pub is_melee: bool,
    pub size_scale: f32,
    pub xp_reward: u64,
    pub is_boss: bool,
    /// sim_time absoluto ate quando o enemy esta sendo empurrado por
    /// knockback. Enquanto > now, vel forcada = knockback_vel.
    pub knockback_until: f32,
    /// Velocidade do empurrao (tiles/s). Aplicada enquanto knockback_until
    /// > now. Calculada como `-hurt_dir × strength / KNOCKBACK_DURATION`.
    pub knockback_vel: Vec2,
    /// Stamina atual do enemy. Cap = stats.stamina_max. Drena ao bloquear
    /// projetil com escudo (so' enemies com offhand=Shield). Regen passivo
    /// fora de combate. 0 = sem stamina, escudo nao bloqueia mais ate regen.
    pub stamina_current: f32,
    // ── Boss AI "PvP-like" (só roda quando is_boss — mobs comuns ignoram) ──
    /// sim_time até quando o boss está em dash (vel = ai_dash_dir × DASH).
    pub ai_dash_until: f32,
    pub ai_dash_dir: Vec2,
    /// Cooldown do dash do boss (gap-closer / sidestep).
    pub ai_dash_cd_until: f32,
    /// sim_time até quando o boss está BLOQUEANDO (dano recebido -75%,
    /// anda devagar, não ataca; counter logo após).
    pub ai_block_until: f32,
    pub ai_block_cd_until: f32,
    /// Strafe circular em range: direção (±1) e quando flipar.
    pub ai_strafe_sign: f32,
    pub ai_strafe_flip_at: f32,
    /// Hit bloqueado neste tick → snapshot manda PARRY_FLASH (drenado em
    /// send_snapshots, mesma mecânica do parry de player).
    pub parry_flash_pending: bool,
    /// Dash iniciado neste tick → snapshot manda attack_anim=DASH (anim de
    /// jump, igual o dash do player). Drenado em send_snapshots.
    pub dash_anim_pending: bool,
    /// Hits recebidos recentemente (decai ~1.5/s) — dispara o block REATIVO
    /// do boss: apanhou seguido → levanta a guarda.
    pub ai_recent_hits: f32,
    /// Nome custom de BOSS (ex "[BOSS] Cavaleiro Radiante Lv10") — snapshot
    /// usa este em vez do nome derivado do tier. None = mobs comuns.
    pub boss_name: Option<String>,
    // ── Leap do boss (Leap Strike 1001) — MESMA mecânica do player: arco
    // (leap_y no snapshot), posição lerpada no pass de leap, dano AoE no
    // POUSO. Boss não age no ar (IA pula o tick).
    pub leap_until: f32,
    pub leap_start_pos: Vec2,
    pub leap_target: Vec2,
    pub leap_damage: i32,
    pub leap_radius: f32,
    /// Direção do golpe no tick em que attack_pending é setado (auto ou
    /// skill). Snapshot replica como `aim_dir` pro cliente setar facing —
    /// sem isso o boss strafando golpeia "pro lado" (facing vinha da vel).
    pub attack_dir: Vec2,
}

/// Peixe nadando no oceano. Wander AI simples, sem combate, sem physics body —
/// movido manualmente em `tick_fish`. Spawnado/culado dinamicamente perto dos
/// players (ver tick_fish). Quando fisgado por uma boia, gruda nela e para de
/// vaguear até o reel/cancel.
pub struct FishTag {
    /// Espécie 1-4 (= EntityKind::Fish(n) e fish_item_for_species).
    pub species: u16,
    /// Direção atual de natação (unit). Re-sorteada a cada wander_timer.
    pub wander_dir: Vec2,
    /// Segundos até re-sortear a direção de wander.
    pub wander_timer: f32,
    /// Sessão que fisgou este peixe (None = livre). Enquanto Some, o peixe não
    /// vagueia, não é atraído por outras boias e não pode ser re-fisgado.
    pub hooked_by: Option<SessionId>,
}

/// Tag em inimigo spawnado por uma ServerSpawnZone — usado pra decrementar
/// o contador vivo e enfileirar respawn quando o inimigo morre.
#[derive(Clone, Copy)]
pub struct SpawnedByZone {
    pub zone_id: u32,
    pub kind: u16,
    /// Indice do slot fixo (Poisson disk) que ocupa nesta zona. u32::MAX = legacy
    /// (zona quotas sem slots).
    pub slot_idx: u32,
}

/// Slot fixo de spawn — gerado por Poisson disk sampling no init da zona.
/// Cada slot e um spot pre-determinado; tipo de inimigo varia a cada spawn.
#[derive(Clone, Copy)]
pub struct SpawnSlot {
    pub pos: Vec2,
    /// EntityId do mob ocupando — None = livre (pronto pra spawn ou em respawn).
    pub occupant: Option<EntityId>,
    /// Sim_time absoluto quando o slot fica disponivel pra respawn.
    pub respawn_at: f32,
}

/// Tag em vendor NPC — referencia o shop_id (lookup em vendor_shops).
/// Cliente recebe os items + sell_prices baseado nesse shop_id.
#[derive(Clone)]
pub struct VendorTag {
    pub shop_id: u32,
    pub name:    String,
}

/// Tag em NPC ferreiro — interagir abre painel de refinamento + socket gem.
#[derive(Clone)]
pub struct BlacksmithTag {
    pub name: String,
}

/// Tag em NPC de facção (kind 6) — dá quests PvP da facção + loja de pontos.
/// Só serve players da MESMA facção.
#[derive(Clone)]
pub struct FactionGiverTag {
    pub faction: shared::Faction,
    pub name: String,
}

/// Tag em baú de tesouro (kind 7) — abrir (interagir) conclui a quest TREASURE
/// `quest_id` e concede a relíquia. Trancado sem a quest ativa.
#[derive(Clone)]
pub struct TreasureChestTag {
    pub quest_id: u16,
}

/// Tag em NPC ambiental que anda por uma rota (waypoints em loop).
/// `current_idx` é o waypoint atual; `pause_until` é sim_time pra retomar
/// movimento depois de chegar num waypoint (pequena pausa pra naturalidade).
#[derive(Clone)]
pub struct WanderRouteTag {
    pub route_id:    u32,
    pub current_idx: u32,
    pub pause_until: f32,
    pub name:        String,
    /// Giver de quest deste morador (0 = ambiente, não dá quest). Um morador por
    /// cidade é promovido a "arauto" com giver único (101+) no load do mapa.
    pub giver:       u16,
    /// Ponto de origem (spawn) — perambula num raio em volta dele.
    pub home:        Vec2,
    /// Alvo atual da perambulação (ponto aleatório perto de `home`).
    pub target:      Vec2,
}

/// Rota nomeada do mapa: lista ordenada de waypoints. NPCs com WanderRouteTag
/// referenciam por `route_id`. Loop A→B→C→...→A.
#[derive(Clone)]
pub struct NpcRoute {
    pub id:         u32,
    pub waypoints:  Vec<Vec2>,
}

/// Visual config de um NPC — cliente usa pra render do paper-doll. Codifica
/// o preset (farmer/merchant/guard/etc) num u8.
#[derive(Clone, Copy)]
pub struct NpcSkin {
    pub preset: u8,
}

/// Zona de spawn gerenciada no server. Carregada do MapFile via
/// MapEntity::EnemySpawner. Mantem o mundo vivo: sempre tenta atingir
/// `quotas`; quando um inimigo da zona morre, enfileira respawn apos `respawn_delay_s`.
pub struct ServerSpawnZone {
    pub id: u32,
    /// Canto inferior-esquerdo do AABB em tile coords.
    pub origin: Vec2,
    /// Extensao do AABB em tiles.
    pub size: Vec2,
    pub respawn_delay_s: f32,
    /// (kind, quantidade alvo) — quantos desse tipo manter vivos.
    pub quotas: Vec<(u16, u32)>,
    /// (kind, quantidade viva atualmente) — mantido em sync.
    pub live: Vec<(u16, u32)>,
    /// Fila de respawns pendentes: (game_time_s quando pronto, kind).
    pub respawn_queue: Vec<(f32, u16)>,
    /// Vertices do poligono em WORLD coords (ja somado origin).
    /// None = zona e' o rect AABB inteiro (legacy).
    pub polygon: Option<Vec<Vec2>>,
    /// Modo level-range: quando Some, ignora `quotas` e sortea kinds
    /// aleatorios pra preencher `count` vivos. (level_min, level_max, count).
    pub level_range: Option<(u32, u32, u32)>,
    /// Quantos vivos no modo level-range (sem distincao de kind). 0 quando
    /// level_range = None.
    pub level_range_live: u32,
    /// Fila de respawn pendente no modo level-range — apenas timestamps
    /// (kind e' sorteado na hora do spawn).
    pub level_range_queue: Vec<f32>,
    /// Slots fixos pre-computados via Poisson disk sampling. Cada slot e um
    /// spot pre-determinado com spacing minimo garantido (~10 tiles). Tipo
    /// de inimigo (classe/level) varia a cada (re)spawn no slot. Vazio em
    /// zonas legacy quotas.
    pub slots: Vec<SpawnSlot>,
    /// Lazy spawn: zona so spawna/tica quando algum player esta dentro do
    /// AABB+SPAWN_WAKE_MARGIN. Quando ninguem perto por SPAWN_SLEEP_DELAY_S,
    /// despawna todos os mobs vivos e libera slots — reduz tick cost de IA.
    pub active: bool,
    /// sim_time_s da ultima vez que algum player estava dentro do wake radius.
    pub last_player_near_at: f32,
}

/// Raio do disco plano que um mob comum exige pra nascer, em unidades.
pub const MOB_RAIO_SITIO_UN: f32 = 3.0;
/// Espacamento minimo entre centros de zona. Sem isso a mesma clareira recebe
/// tres hordas e o resto da ilha fica vazio.
pub const MOB_ZONA_ESPACO_UN: f32 = 90.0;
/// Raio de uma zona: de onde ela tira os slots.
pub const MOB_ZONA_RAIO_UN: f32 = 45.0;
/// Espacamento minimo entre dois mobs da mesma zona.
pub const MOB_ESPACO_UN: f32 = 7.0;
/// Teto de mobs por zona.
pub const MOB_POR_ZONA: u32 = 18;
/// Teto de zonas por ilha. Vezes `MOB_POR_ZONA` da' a ordem de grandeza do
/// mundo povoado — e o `lazy_spawn` garante que so' as perto do jogador
/// custam alguma coisa.
pub const MOB_ZONAS_MAX: u32 = 60;

/// Wake radius (em tiles) ao redor do AABB da zona. Player precisa entrar
/// nessa margem pra zona acordar e comecar a spawnar.
pub const SPAWN_WAKE_MARGIN: f32 = 60.0;
/// Tempo sem player no wake radius pra zona dormir e despawnar mobs.
pub const SPAWN_SLEEP_DELAY_S: f32 = 30.0;
/// Stagger entre spawns ao acordar — evita N spawns no mesmo tick (hitch).
/// 0.033s = 1 spawn por tick a 30Hz. Zona com 100 slots leva ~3s pra povoar.
pub const SPAWN_WAKE_STAGGER_S: f32 = 0.033;

/// Area de spawn dedicada a UM boss. Sem quotas: 1 boss alive por vez.
/// Respawna apos morte com delay (boss-specific). Independente de
/// ServerSpawnZone — sem mistura de hordas com boss.
pub struct BossSpawnArea {
    pub id: u32,
    pub origin: Vec2,
    pub size: Vec2,
    pub polygon: Option<Vec<Vec2>>,
    pub level: u32,
    pub respawn_s: f32,
    /// EntityId do boss vivo no momento (None = morto/aguardando respawn).
    pub current_boss: Option<EntityId>,
    /// Sim_time absoluto pra respawnar (quando current_boss vira None).
    pub respawn_at: f32,
}

/// Identifica um item dropado no chao. `instance` é Some pra
/// equipáveis dropados (rarity + rolls), None pra stackáveis (gold,
/// poções, materiais).
#[derive(Debug, Clone, Copy)]
pub struct LootTag {
    pub item_id: u16,
    pub qty: u32,
    pub instance: Option<shared::items::ItemInstance>,
    /// `sim_time_s` no momento do spawn. Auto-pickup só roda depois de
    /// `LOOT_PICKUP_DELAY_S` pra dar tempo do player VER o drop.
    pub spawn_at: f32,
}

/// Bola de canhao em voo. Trajetoria parabolica determinada na hora do tiro:
/// pos = lerp(spawn_pos, target_pos, t/t_max); height = arc(t/t_max) * peak.
/// Sem colisao no voo — explode no impacto (t >= t_max) com AoE no
/// blast_radius. Damages players de outras faccoes e enemies.
#[derive(Debug, Clone, Copy)]
pub struct CannonBombTag {
    pub spawn_pos: Vec2,
    pub target_pos: Vec2,
    pub peak_height: f32,
    pub t_elapsed: f32,
    pub t_max: f32,
    pub damage: i32,
    pub blast_radius: f32,
    pub owner_pid: PlayerId,
    /// Eid do barco que disparou — pra ignorar damage no proprio barco
    /// (e nos passageiros) se quisermos no futuro.
    pub owner_boat_eid: EntityId,
    pub owner_player_eid: EntityId,
}

pub struct PlayerTag {
    pub name: String,
    pub player_id: PlayerId,
    /// Setado quando o player executa swing/shoot/thrust no tick. O codigo
    /// (`shared::attack_anim::*`) é coletado em `send_snapshots` pra
    /// `EntitySnapshot.attack_anim`, que faz outros clientes tocarem a anim
    /// de ataque correta. None nos demais ticks.
    pub attack_anim_pending: Option<u8>,
    /// Step do combo no tick em que attack_anim_pending=SLASH. Usado pelos
    /// clientes remotos pra escolher Slash1/Slash2/Finisher. None fora do
    /// tick de attack ou em SHOOT/THRUST.
    pub combo_step_pending: Option<u8>,
}

/// Tag que marca uma entidade como invisivel aos sistemas de targeting.
/// Usado hoje pra Downed State; reutilizavel pra stealth/invisibilidade
/// e outras mecanicas futuras. Inimigos ignoram entidades com esta tag.
pub struct Untargetable;

/// Portal estatico — ao encostar, jogador e teleportado para `target`.
pub struct PortalTag {
    /// Zona de destino. Igual a' local = teleporte dentro do mapa; diferente =
    /// o jogador reconecta no processo que serve aquela zona.
    pub target_map: String,
    pub target: Vec2,
    /// Cooldown pra evitar teletransporte infinito quando chega no destino.
    pub cooldown: f32,
}

/// Barco navegavel (Sea-of-Thieves style). Spawnado quando um player usa um
/// item de barco em margem walkable; despawnado quando todos saem.
///
/// **Modelo de movimento (NAO mais "single-piloto + tile-snap"):**
/// - Velocidade derivada de `sail_position * wind_intensity * sail_alignment`,
///   nao do input do player.
/// - Yaw vem do `helm_input` (player na estacao HELM ajusta -1..1).
/// - Ancora dropada multiplica drag → barco trava rapido.
/// - Multiplos players a bordo (passengers); cada um pode pegar uma estacao.
/// - Posicao continua (sem snap a tile), mas colisao com tiles nao-water.
pub struct BoatTag {
    /// Kind visual do barco (0=Lylian Leutard).
    pub kind: u16,
    /// PlayerId que SPAWNOU o barco (dono). Outros podem entrar livremente
    /// — donho usado pra persistencia (qual char "tem" o barco).
    pub owner_pid: PlayerId,

    // ── Heading e leme ─────────────────────────────────────────────────────
    /// Heading do barco em rad world-space (0 = +X / leste).
    pub yaw: f32,
    /// Yaw rate em rad/s. Integrado por `rudder_angle`.
    pub ang_vel: f32,
    /// Angulo acumulado da roda do leme em rad. Persiste mesmo sem player
    /// na estacao HELM — barco continua virando ate alguem centralizar.
    /// Clampado em [-BOAT_MAX_RUDDER_ANGLE, +].
    pub rudder_angle: f32,

    // ── Vela ───────────────────────────────────────────────────────────────
    /// 0=raised (sem propulsao), 1=half (50%), 2=full (100%).
    pub sail_position: u8,
    /// Angulo da vela em rad relativo ao casco (-PI/2..PI/2). 0 = vela
    /// perpendicular ao casco (catch wind from behind).
    pub sail_angle: f32,

    // ── Ancora ─────────────────────────────────────────────────────────────
    /// True se a ancora esta dropada (drag elevado, barco para).
    pub anchor_dropped: bool,
    /// Anim de drop/raise em [0, 1]. 1 = totalmente dropada (drag full),
    /// 0 = totalmente recolhida (sem drag extra). Cresce/decresce com
    /// `BOAT_ANCHOR_ANIM_TIME` segundos.
    pub anchor_progress: f32,

    // ── Estacoes ocupadas (player_eids) ────────────────────────────────────
    pub helm_eid: Option<EntityId>,
    pub sail_eid: Option<EntityId>,
    pub anchor_eid: Option<EntityId>,
    /// Canhoes: 1 entry por slot, na mesma ordem do boat_config.cannons.
    /// `cannon_eids[i]` = Some(eid) quando ocupado.
    /// `cannon_aim[i]` = angulo de mira persistido (-CANNON_AIM_MAX_RAD..+).
    /// `cannon_cd_until[i]` = sim_time_s em que o canhao volta a poder atirar.
    pub cannon_eids: Vec<Option<EntityId>>,
    pub cannon_aim: Vec<f32>,
    pub cannon_cd_until: Vec<f32>,

    /// Lista de players a bordo (em estacao OU livre andando no deck).
    pub passengers: Vec<EntityId>,

    // ── Compat com BoatRenderer 2D atual ───────────────────────────────────
    /// Direcao 8-cardeais (0..7) computada do `yaw`. Snapshot popula isso
    /// pro BoatRenderer existente continuar funcionando ate o renderer 3D.
    pub dir: u8,
    /// Anim 0=idle / 1=movement, derivada de |vel|. Sem 'shoot' (canhoes
    /// nao implementados nesta refatoracao — virao depois).
    pub anim: u8,
}

/// Marca um player como montado num barco. Position do player no mundo =
/// barco.pos + rotate(local_pos, barco.yaw), recalculado a cada tick.
pub struct Mounted {
    pub boat_entity: Entity,
    pub boat_eid: EntityId,
    /// Posicao do player no deck local (relativa ao centro do barco, sem
    /// rotacao). Player anda no deck integrando isso com `move_dir` enquanto
    /// `station` for None.
    pub local_pos: Vec2,
    /// Estacao operada pelo player. None = livre andando no deck.
    /// Quando setada, player fica fixo na local_pos da estacao e o input
    /// nao move ele — mas pode ajustar a estacao (helm/sail/anchor).
    pub station: Option<u8>,
    /// Ultimo move_dir recebido do client. Persistido entre ticks (input
    /// vem ~60Hz, tick server e' 30Hz — sem persistir, ticks sem input
    /// fariam vel=ZERO e walk anim flickaria).
    pub last_move_dir: Vec2,
}

/// Estado global do vento. Constante por enquanto; futuro: drift suave +
/// eventos de tempestade. Broadcastado no login + quando muda.
#[derive(Debug, Clone, Copy)]
pub struct WindState {
    /// Direcao em rad world-space (0 = vento soprando pra +X / leste).
    pub direction: f32,
    /// Intensidade [0, 1]. 0 = calmaria (barco nao sai do lugar mesmo com
    /// vela full); 1 = vendaval.
    pub intensity: f32,
}

impl Default for WindState {
    fn default() -> Self {
        // Vento padrao: noroeste moderado. Ajustavel via admin no futuro.
        Self {
            direction: std::f32::consts::FRAC_PI_4, // 45° (nordeste)
            intensity: 0.85,
        }
    }
}

// ── Helpers de geometria do barco ──────────────────────────────────────────
// Os valores vem do `boat_config::registry()` carregado de JSONs em
// `data/boats/`. JSONs sao exportados do prefab Unity (Tools / Boat /
// Export Prefab Data). Fallback hardcoded em boat_config.rs se faltar arquivo.

/// Gating de PvP entre dois players. Regras:
///  - Safe zone (cidade) protege TODOS: se atacante ou alvo está em safe
///    zone, nunca há dano (mesmo cross-facção).
///  - Facções diferentes: PvP SEMPRE ON (Morganeers vs Peacemain).
///  - Mesma facção: opt-in — ambos precisam de PK Mode ON.
impl GameWorld {
    pub fn can_damage_player(&self, attacker_eid: EntityId, target_eid: EntityId) -> bool {
        if attacker_eid == target_eid { return false; }
        let att = match self.sessions.values().find(|s| s.entity_id == attacker_eid) {
            Some(s) => s, None => return false,
        };
        let tgt = match self.sessions.values().find(|s| s.entity_id == target_eid) {
            Some(s) => s, None => return false,
        };
        // Safe zone protege todos — atacante OU alvo dentro = sem dano.
        for sess in [att, tgt] {
            if let Some(e) = sess.entity {
                if let Ok(p) = self.ecs.get::<&Position>(e) {
                    if self.in_safe_zone(p.0) { return false; }
                }
            }
        }
        // Cross-facção: sempre PvP. Mesma facção: opt-in (ambos PK ON).
        if att.faction != tgt.faction { return true; }
        att.pk_mode_on && tgt.pk_mode_on
    }
}

/// Espelha a BFS de handle_dismount_boat: ha terra walkable a ate
/// max_radius tiles do centro do barco? Usado pra esconder o botao
/// "Sair do Barco" no cliente quando dismount nao eh possivel.
/// Mesmos limites do handler — assim UI e regra batem.
pub fn boat_can_dismount(
    boat_pos: Vec2,
    kind: u16,
    map: &shared::world_gen::WorldMap,
) -> bool {
    let cfg = crate::boat_config::get(kind);
    let max_radius = cfg.deck_half_w.max(cfg.deck_half_h).ceil() as u32 + 25;
    let bx = boat_pos.x.floor() as i32;
    let by = boat_pos.y.floor() as i32;
    let mut visited: std::collections::HashSet<(i32, i32)> = std::collections::HashSet::new();
    let mut queue: std::collections::VecDeque<(i32, i32, u32)> = std::collections::VecDeque::new();
    let cardinals = [(0i32, 1i32), (1, 0), (0, -1), (-1, 0)];
    for (dx, dy) in cardinals { queue.push_back((bx + dx, by + dy, 1)); }
    while let Some((tx, ty, depth)) = queue.pop_front() {
        if !visited.insert((tx, ty)) { continue; }
        if map.is_walkable(tx, ty) { return true; }
        if depth >= max_radius { continue; }
        for (dx, dy) in cardinals {
            queue.push_back((tx + dx, ty + dy, depth + 1));
        }
    }
    false
}

pub fn boat_deck_half(kind: u16) -> Vec2 {
    crate::boat_config::get(kind).deck_half()
}
pub fn boat_helm_local(kind: u16)   -> Vec2 { crate::boat_config::get(kind).helm_local() }
pub fn boat_sail_local(kind: u16)   -> Vec2 { crate::boat_config::get(kind).sail_local() }
pub fn boat_anchor_local(kind: u16) -> Vec2 { crate::boat_config::get(kind).anchor_local() }

/// Distancia maxima entre player e estacao pra interagir (em tiles).
/// Escala com tamanho do deck — boat grande precisa de reach proporcional.
pub const BOAT_STATION_REACH: f32 = 3.0;
/// Velocidade do player andando no deck (tiles/s, deck-local).
pub const BOAT_DECK_WALK_SPEED: f32 = 4.5;

/// Forward unitario do barco (proa) dado yaw. Convencao: yaw=0 → +Y world
/// (norte). +Y do FRAME LOCAL do barco aponta pro forward (proa). Helm
/// (popa) eh -Y local; anchor (proa) eh +Y local.
pub fn boat_forward(yaw: f32) -> Vec2 {
    Vec2::new(-yaw.sin(), yaw.cos())
}

/// Rotaciona vetor 2D por angulo em rad.
pub fn rotate_vec(v: Vec2, angle: f32) -> Vec2 {
    let (s, c) = angle.sin_cos();
    Vec2::new(v.x * c - v.y * s, v.x * s + v.y * c)
}

/// Sortea uma posicao continua aleatoria dentro do polygon (ou do AABB se
/// sem polygon). Sem checagem de tile/wall — caller assume que o collider
/// foi desenhado em area valida. Retorna None se nao achar em `tries`.
fn pick_random_in_polygon(
    origin: Vec2,
    size: Vec2,
    seed: u64,
    polygon: Option<&[Vec2]>,
    tries: u32,
) -> Option<Vec2> {
    pick_random_in_polygon_with_map(origin, size, seed, polygon, tries, None)
}

/// Variante que tambem valida que o tile e' walkable (ground, nao water/wall).
/// Quando `map` eh Some, descarta candidatos em water/wall e tenta de novo.
fn pick_random_in_polygon_with_map(
    origin: Vec2,
    size: Vec2,
    seed: u64,
    polygon: Option<&[Vec2]>,
    tries: u32,
    map: Option<&shared::world_gen::WorldMap>,
) -> Option<Vec2> {
    let mut s = seed;
    for _ in 0..tries {
        s = lcg(s);
        let rx = lcg_f32(s);
        s = lcg(s);
        let ry = lcg_f32(s);
        let pos = Vec2::new(origin.x + rx * size.x, origin.y + ry * size.y);
        if let Some(verts) = polygon {
            if !point_in_polygon(pos, verts) { continue; }
        }
        if let Some(m) = map {
            let tx = pos.x.floor() as i32;
            let ty = pos.y.floor() as i32;
            if !m.is_walkable(tx, ty) { continue; }
        }
        return Some(pos);
    }
    None
}

/// Point-in-polygon via ray casting horizontal. O & e' usado pra detectar
/// cruzamentos pares=fora, impares=dentro. Funciona pra poligonos convexos
/// e concavos. Vertices em CCW ou CW (irrelevante).
fn point_in_polygon(p: Vec2, verts: &[Vec2]) -> bool {
    let n = verts.len();
    if n < 3 { return false; }
    let mut inside = false;
    let mut j = n - 1;
    for i in 0..n {
        let vi = verts[i];
        let vj = verts[j];
        if (vi.y > p.y) != (vj.y > p.y) {
            let x_at_p_y = (vj.x - vi.x) * (p.y - vi.y) / (vj.y - vi.y) + vi.x;
            if p.x < x_at_p_y { inside = !inside; }
        }
        j = i;
    }
    inside
}

/// Snap de um vetor 2D pra uma das 8 direcoes do boat sprite-sheet.
/// 0=N, 1=NE, 2=E, 3=SE, 4=S, 5=SW, 6=W, 7=NW.
/// Default = 4 (S) quando o vetor e' zero/quase-zero.
fn dir8_from_vec(v: Vec2) -> u8 {
    if v.length_squared() < 0.01 { return 4; }
    // angle = atan2(y, x) em radianos. Conversao pra "compass":
    // angulo 90deg = N, 0deg = E, -90deg = S, etc.
    let angle = v.y.atan2(v.x);
    // Normaliza pra [0, 2pi) onde 0 corresponde a E.
    let mut a = angle;
    if a < 0.0 { a += std::f32::consts::TAU; }
    // Discretiza em 8 setores de 45deg cada, deslocado meio-setor pra que
    // E (a=0) caia no centro do setor 0. Ordem (em sentido anti-horario,
    // y+ pra cima): E=0, NE=1, N=2, NW=3, W=4, SW=5, S=6, SE=7.
    let sector = ((a + std::f32::consts::FRAC_PI_8) / std::f32::consts::FRAC_PI_4) as i32 & 7;
    // Mapeia pra ordem de sprite-sheet do cliente (N=0, NE=1, E=2, ...).
    match sector {
        0 => 2, // E
        1 => 1, // NE
        2 => 0, // N
        3 => 7, // NW
        4 => 6, // W
        5 => 5, // SW
        6 => 4, // S
        7 => 3, // SE
        _ => 4,
    }
}

pub struct Session {
    pub handle: SessionHandle,
    /// Dados guardados enquanto o jogador espera na fila de entrada.
    pub entrada_pendente: Option<(crate::auth::AuthSuccess, crate::persistence::CharacterRow)>,
    /// Alvo atual do combate por target. O auto-ataque dispara sozinho
    /// enquanto houver alvo vivo no alcance — sem botao, como em MMO de
    /// target. `None` = sem alvo, sem ataque.
    pub target: Option<EntityId>,
    /// Rota do toque no chao, calculada pelo SERVIDOR. O seguidor tambem
    /// DESISTE de ponto inalcancavel — sem isso o boneco empurra a quina pra
    /// sempre. Ver `shared::terreno::SeguidorDeRota`.
    pub rota: shared::terreno::SeguidorDeRota,
    /// `sim_time_s` ate' quando o corpo esta' no ar. Enquanto isso, o degrau
    /// que ele aceita sobe de um bloco pra dois.
    pub pulo_ate: f32,
    /// Quando o proximo pulo fica disponivel.
    pub pulo_pronto_em: f32,
    /// `sim_time_s` do ultimo pedido de rota, pro limite de frequencia.
    pub rota_pedida_em: f32,
    /// Ultimo ESTADO enviado por entidade nesta sessao. E' a base do delta.
    ///
    /// Guarda `EntityState`, que e' `Copy` e tem 13 bytes — nao o struct de 58
    /// campos com duas `String` dentro que existia antes. Com 100 jogadores
    /// vendo 100 entidades, a diferenca e' 10 mil clones com alocacao por tick
    /// contra 10 mil copias de bloco.
    pub last_sent: HashMap<EntityId, EntityState>,
    pub entity: Option<Entity>,
    pub entity_id: EntityId,
    pub last_input_seq: u32,
    pub pending_input: Option<InputFrame>,
    pub logged_in: bool,
    /// True enquanto a verificacao de senha esta rodando — recusa logins
    /// duplicados da mesma sessao.
    pub auth_in_flight: bool,
    pub attack_cooldown: f32,
    /// sim_time ate o fim do dash atual. Player imovel/invuln enquanto > sim_time.
    pub dash_until: f32,
    /// Direcao unitaria do dash em curso.
    pub dash_dir: Vec2,
    /// Cooldown ate o proximo dash poder ser usado.
    pub dash_cooldown: f32,
    /// sim_time absoluto até o qual o player fica em stagger (sem mover/atacar).
    pub hurt_until: f32,
    /// sim_time em que o player comecou a ficar idle (sem mover/cast/hit).
    /// Apos 2s de idle continuo, regen de HP eh boostado 4x ("descansando").
    /// 0 = nao idle.
    pub idle_since: f32,
    /// Vetor unitário do alvo TOWARD o último atacante (pra knockback futuro).
    pub hurt_dir: Vec2,
    /// sim_time ate quando o player esta sendo empurrado. Enquanto > now,
    /// vel forcada = knockback_vel (override do input).
    pub knockback_until: f32,
    /// Vel do knockback (tiles/s).
    pub knockback_vel: Vec2,
    /// Step do combo melee atual (0=Slash1, 1=Slash2, 2=Finisher/Thrust). Server
    /// cicla a cada SLASH attack e replica via EntitySnapshot.combo_step pra que
    /// outros clientes vejam o alternance correto. Reset após
    /// shared::COMBO_RESET_TIME sem ataque OU ao tomar hurt.
    pub combo_step: u8,
    /// sim_time do último ataque melee — usado pra reset do combo.
    pub combo_last_attack: f32,
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
    /// Setado quando login conclui auth mas account nao tem character ainda.
    /// Cliente abre CharacterCreationUI; ao receber CreateCharacter, o
    /// server consome este id pra criar a row. None apos creation.
    pub pending_char_creation_account_id: Option<i64>,
    /// Username da conta + class (defaults), salvos durante o gate pra
    /// reconstruir o AuthSuccess ao receber CreateCharacter.
    pub pending_auth_username: Option<String>,
    pub pending_auth_class: Option<String>,
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
    /// Moeda. Currency separado — não ocupa slot de inventário. Persistido
    /// em `characters.gold`. Ver `LOOT_PICKUP` (gold loot vai pra cá),
    /// shop buy/sell, refining.
    pub gold: u64,
    pub gold_last_sent: u64,
    /// Fame acumulada. Ganha matando players + bosses. Perde ao ser morto.
    pub fame: u64,
    pub fame_last_sent: u64,
    /// Aura (poise). Ganha/perde apenas em PvP. Base pro sistema de stagger.
    pub aura: u64,
    pub aura_last_sent: u64,
    /// Party atual (None = solo). Guarda o ID unico da party; membros
    /// sao buscados iterando sessoes.
    pub party_id: Option<u32>,
    /// Ultimo convite pendente (nome do convidante).
    pub party_invite_from: Option<String>,
    /// Se Some, esse jogador esta carregando outro (pelo entity_id dele).
    /// Enquanto carrega: velocidade 50%, nao pode atacar.
    pub carrying: Option<EntityId>,
    /// Se Some, esse jogador esta sendo carregado por outro.
    pub carried_by: Option<EntityId>,
    /// Mapa logico onde o player esta. Hoje sempre "overworld"; reservado
    /// pra refactor futuro com physics/ECS isolados por mapa.
    pub current_map: String,
    /// XP por proficiencia. Indice = Proficiency as u8.
    pub proficiencies: [u64; shared::PROF_COUNT],
    pub proficiencies_dirty: bool,
    // ── Skills (Phase 1) ────────────────────────────────────────────────
    /// SP totais ganhos. Cresce em level-up via SP_PER_LEVEL.
    pub skill_points_earned: u32,
    /// SP gastos em learn + rank-up.
    pub skill_points_spent: u32,
    /// Skills aprendidas + rank + slot equipado (None = passiva ou ativa
    /// não-equipada).
    pub learned_skills: Vec<shared::LearnedSkill>,
    /// Marca que precisa enviar PlayerSkillsUpdate no próximo tick.
    pub skills_dirty: bool,
    /// Cooldown timestamps por skill_id. Valor = `sim_time_s` quando a skill
    /// fica disponível de novo. Cast só é permitido se sim_time >= valor.
    pub skill_cds: HashMap<u32, f32>,
    /// Sim_time em que o cast atual termina. 0 = não tá castando. Durante cast:
    /// bloqueia movimento, ataque, defesa, e novos casts. Setado em
    /// handle_skill_cast quando skill tem cast_time_s > 0.
    pub casting_until: f32,
    /// Sim_time em que o cast atual começou. Usado pra grace period no
    /// cancel-por-movimento (player que clica skill enquanto andava nao
    /// cancela de imediato — tem 0.3s pra parar).
    pub casting_started_at_s: f32,
    /// Ticks consecutivos com `dir != 0` durante cast. Cast cancela so'
    /// depois de >= 2 ticks pra evitar race: na tick que o player toma
    /// hit, `hurt_until` ainda nao foi setado (damage_events roda DEPOIS
    /// do input), entao `in_hurt` eh false e dir nao eh zerado → 1 tick
    /// nao basta. Resetado quando dir == 0.
    pub cast_movement_ticks: u8,
    /// PK Mode (opt-in PvP). Quando ON, player aparece como targetavel
    /// por outros players com PK ON. Default OFF.
    pub pk_mode_on: bool,
    /// Facção do char ativo. Carregada do CharacterRow ao spawnar. Define
    /// PvP cross-facção (sempre ON) e cor no mapa.
    pub faction: shared::Faction,
    /// Skill_id do cast em progresso (usado pra notificar cliente da pose).
    pub casting_skill_id: u32,
    /// MP gasto no cast atual (pra refund se cancelar).
    pub casting_mp_paid: f32,
    /// Stamina gasta no cast atual (pra refund se cancelar).
    pub casting_st_paid: f32,
    /// Storm Caller (1056) — sim_time da proxima auto-invocacao de
    /// lightning bolt. 0 = nunca disparou ainda; setado por tick passive
    /// proc se a skill estiver learned com rank > 0.
    pub storm_caller_next: f32,
    /// Pontos de atributo disponiveis (ganhos por level-up, POINTS_PER_LEVEL cada).
    pub unspent_points: u32,
    /// Pontos ja alocados em cada stat [FOR, DES, INT, VIT, SPD].
    pub allocated_points: [u32; shared::STAT_COUNT],
    /// Marca que precisa enviar StatPointsUpdate no proximo tick.
    pub stat_points_dirty: bool,
    /// Ultimo level conhecido pra detectar level-up.
    pub last_level: u32,
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
    /// Configuracao visual do paper-doll (skin/race/outfit/hair) replicada
    /// em EntitySnapshot.visual pra todos os clientes que enxergam esse
    /// player. Defaultada por classe no login.
    pub visual: shared::VisualConfig,

    // ── Defesa ativa (block + parry) ────────────────────────────────────
    /// True enquanto o player segura RMB. Aplica reducao de dano +
    /// drain de stamina nos hits, e reduz movement speed.
    pub defending: bool,
    /// `sim_time_s` do ultimo edge-press de PRIMARY (LMB). Usado pra detectar
    /// parry: se um hit chegar dentro de PARRY_WINDOW_S, conta como parry.
    pub last_press_primary_at: f32,
    /// `sim_time_s` do ultimo edge-press de SECONDARY (RMB). Mesma logica
    /// que primary — apertar RMB no timing perfeito tambem da parry.
    pub last_press_secondary_at: f32,
    /// Mascara de buttons do tick anterior (pra detectar edge transitions).
    pub prev_buttons: u32,
    /// `sim_time_s` ate quando o player esta em stagger (recebeu parry).
    /// Enquanto > sim_time, ataque/dash/block sao bloqueados.
    pub stagger_until: f32,
    /// Pendente: dispara um attack_anim PARRY_FLASH no proximo snapshot.
    /// Setado quando esse player parryou um hit recebido.
    pub parry_flash_pending: bool,

    // ── Bow passives ─────────────────────────────────────────────────────
    /// `sim_time_s` da última vez em que o player se moveu (move_dir != 0).
    /// Usado por Quick Draw (1036): se ficar parado >= 1s, o próximo tiro
    /// é guaranteed crit.
    pub last_movement_at_s: f32,
    /// True após um crit forçado por Quick Draw, falso ao se mover. Garante
    /// que só a "1ª flecha" após ficar parado é crit (não todas enquanto
    /// stationary).
    pub quickdraw_consumed: bool,
    /// Hunter's Mark (1040): mapa de target_eid → expires_at_sim_time. Hits
    /// em targets marcados aplicam +20% damage. Hit refresh do mark.
    pub hunter_marks: HashMap<EntityId, f32>,

    // ── Sword skills state ──────────────────────────────────────────────
    /// Riposte (1001): timer ate quando o player tem o buff de "+50% dmg"
    /// do proximo SLASH. Setado quando ele parry-flasha um hit recebido.
    /// Consumido no proximo swing (zerado depois de aplicar o bonus).
    pub riposte_until: f32,
    /// Sword Dance (1005): janela durante a qual swings sao parte do combo
    /// forçado de 3 hits. Reset quando expira ou completa 3 hits.
    pub sword_dance_until: f32,
    /// Step atual do Sword Dance (0..2). Quando == 2, proximo SLASH e
    /// guaranteed crit (e zera o stance).
    pub sword_dance_step: u8,
    /// Master's Counter (1007): timer ate quando o player auto-parry
    /// qualquer hit recebido + dispara contra-ataque livre.
    pub counter_stance_until: f32,

    // Leap Strike (1001) — animacao de pulo: server interpola posicao do
    // caster de leap_start_pos → leap_target durante LEAP_DURATION segundos.
    // Quando termina, aplica AoE damage + stun no leap_target.
    pub leap_until: f32,
    pub leap_start_pos: Vec2,
    pub leap_target: Vec2,
    /// Damage que sera aplicado no fim do leap (foi computado no cast).
    pub leap_damage: i32,
    /// Radius do AoE no fim do leap.
    pub leap_radius: f32,

    /// Climb-jump: tempo acumulado (s) empurrando contra uma borda escalavel
    /// (cliff). Ao passar de CLIMB_HOLD_TIME, dispara um leap por cima da wall.
    pub climb_timer: f32,
    /// sim_time ate quando nao pode disparar outro climb-jump (anti-spam).
    pub climb_cooldown: f32,

    // Spear Throw (1021) Harpoon state — quando o projetil de spear acerta um
    // alvo, set harpoon_target_eid + harpoon_until. No proximo cast da skill
    // 1021 (recast) dentro da janela, dasha o player ate o alvo (hook).
    /// EntityId do alvo cravado pela lanca. None = sem harpoon ativo.
    pub harpoon_target_eid: Option<EntityId>,
    /// sim_time em que o harpoon expira (limpa state). 5s default.
    pub harpoon_until: f32,

    // Bloodthirst (1011) — buff 4s com +20% atk speed + 10% lifesteal.
    /// sim_time ate quando o buff esta ativo (0 = inativo).
    pub bloodthirst_until: f32,

    // Hunter's Mark (1040) — buff 8s + 3 charges com +50% crit + +5%/rank dmg.
    /// sim_time ate quando o buff esta ativo.
    pub hunters_mark_until: f32,
    /// Charges restantes (decrementa em cada hit projetil; expira em 0 ou tempo).
    pub hunters_mark_charges: u8,
    /// Rank da skill 1040 aprendida (1..10) pra calcular bonus_dmg += 5%/rank.
    pub hunters_mark_rank: u8,

    // ── Poise ───────────────────────────────────────────────────────────
    /// Barra de poise atual (0..stats.poise_max). Drena com hits, absorve
    /// damage e previne stagger enquanto > 0. Regen fora de combate.
    pub poise_current: f32,
    /// sim_time do ultimo hit recebido. Usado pra gate de regen out-of-combat.
    pub last_combat_at_s: f32,
    /// Ultimo poise inteiro enviado pro cliente — evita spam de PoiseUpdate.
    pub poise_last_sent: i32,
    /// Buffer temporario de poise que defending (RMB) absorve. Setado no
    /// edge-press de RMB via `defending_poise_max()` (escala com char_lvl
    /// e bonus de escudo). Drena por hit. Quando zera, defending cancela
    /// e o proximo hit vira HP+stagger normal.
    pub defending_poise_buffer: f32,
    /// Farm skill levels (persistidos). Influenciam hits_required do cliente
    /// e são enviados via FarmSkillsUpdate no login. Default 1.
    pub woodcutting_lvl: u32,
    pub mining_lvl:      u32,
    pub gathering_lvl:   u32,

    /// Quests — estado por personagem (ativas/prontas/concluídas+cooldown).
    pub quests: Vec<crate::quests::CharQuest>,
    pub quests_dirty: bool,
    /// Pontos de facção (moeda das quests de facção; compra itens de facção).
    pub faction_points: u32,
    pub faction_points_last_sent: u32,

    // ── Pesca ────────────────────────────────────────────────────────────
    /// Posição da boia na água enquanto o player está pescando (None = não
    /// está pescando). Server atrai peixes pra cá e detecta a fisgada.
    pub fishing_bobber: Option<Vec2>,
    /// EntityId do peixe atualmente fisgado (None = nenhum). Setado quando um
    /// peixe encosta na boia; limpo no reel/cancel. Espelha FishTag.hooked_by.
    pub hooked_fish: Option<EntityId>,
    /// Índice da lane de tutorial ocupada por esta sessão (só em TUTORIAL_MODE).
    /// None = não está numa lane (mundo normal ou ainda sem char). Liberada no
    /// disconnect pra outro player poder usar.
    pub tutorial_slot: Option<usize>,
    /// Índice da lane de dungeon ocupada por esta sessão (só em DUNGEON_MODE).
    /// None = não está numa dungeon. Liberada no disconnect/complete.
    pub dungeon_slot: Option<usize>,
    /// Modo RAID escolhido pelo client (SelectDungeonMode antes do select).
    /// true → run só de boss, sem waves. Só relevante em DUNGEON_MODE.
    pub pending_dungeon_raid: bool,
}

impl Session {
    /// Concede XP ao jogador — FONTE ÚNICA pra toda fonte de XP (combate,
    /// quests, etc). Soma o XP, processa level-up (pontos de status + skill
    /// points, idempotente via `last_level`) e envia `ProgressUpdate` pro
    /// cliente na hora. Não cobre o bônus de proficiência de arma do combate
    /// (esse é específico do kill e fica inline lá).
    fn grant_xp(&mut self, amount: u64) {
        if amount == 0 { return; }
        self.xp = self.xp.saturating_add(amount);
        let new_level = shared::level_of_xp_with_mult(self.xp, crate::economy::xp_multiplier());
        if new_level > self.last_level {
            let gained = new_level - self.last_level;
            self.unspent_points = self.unspent_points
                .saturating_add(gained * shared::POINTS_PER_LEVEL);
            self.stat_points_dirty = true;
            self.skill_points_earned = self.skill_points_earned
                .saturating_add(gained * shared::SP_PER_LEVEL);
            self.skills_dirty = true;
            tracing::info!(
                "{} subiu pra L{} (+{} pontos livres, +{} SP)",
                self.name, new_level,
                gained * shared::POINTS_PER_LEVEL,
                gained * shared::SP_PER_LEVEL,
            );
        }
        self.last_level = new_level;
        let _ = self.handle.to_client.send(ServerMessage::ProgressUpdate {
            xp: self.xp,
            level: new_level,
        });
    }
}

/// Recursos compartilhados para autenticacao assincrona.
#[derive(Clone)]
pub struct AuthCtx {
    pub pool: sqlx::postgres::PgPool,
    pub tx: mpsc::UnboundedSender<IncomingMessage>,
}

/// Mundo autoritativo.
///
/// ## Multi-mapa (atual vs futuro)
///
/// HOJE (v1): um unico `map` + um unico `physics`. Multiplos "mapas"
/// coexistem como regioes do mesmo espaco, separadas geograficamente
/// (AOI isola entidades naturalmente). Portais teleportam jogadores
/// dentro desse espaco.
///
/// TODO (v2): `maps: HashMap<String, MapInstance>` onde cada MapInstance
/// tem `map + physics + boss_state`. Refactor grande (~50 sites de
/// self.map/self.physics). Entidades tagueadas com `shared::MapId`,
/// sessoes com `current_map`. Portais entre mapas dispararao o ja
/// reservado `ServerMessage::MapChange`. Motivacao: escalar pra muitas
/// dungeons simultaneas sem colisao geografica ou risco de overlap.
pub struct GameWorld {
    /// Contador de jogadores dentro, lido pelo heartbeat do canal.
    pub populacao: Option<crate::canais::Populacao>,
    /// p99 do tick desta instancia. E' o que segura a fila por CARGA e nao so'
    /// por cabeca contada — ver `atualiza_trava`.
    pub saude: Option<crate::canais::Saude>,
    /// Admissao pausada porque o tick esta' perto de estourar. So' pausa
    /// entrada; ninguem e' expulso.
    pub admissao_travada: bool,
    /// `MMO_IMORTAL=1`: dano em JOGADOR nao entra. E' ferramenta de
    /// construcao de mundo — sem isso nao da' pra andar trinta segundos pra
    /// olhar terreno sem morrer pro mob. Mob continua morrendo normalmente.
    ///
    /// Lido uma vez no boot e nao por evento de dano: e' o caminho mais
    /// quente do tick.
    pub imortal: bool,
    /// Relevo desta zona, quando ela e' uma ilha do arquipelago. `None` nas
    /// zonas antigas de tile — enquanto as duas convivem, quem manda e' o
    /// nome da zona.
    pub ilha: Option<shared::terreno::Ilha>,
    /// Onde cada zona do realm esta rodando. Ver `canais::Diretorio`.
    pub diretorio: Option<crate::canais::Diretorio>,
    /// Zona servida por ESTE processo. Portal que aponta pra outra zona vira
    /// troca de servidor, nao teleporte.
    pub zona: String,
    /// Fila de entrada, em ordem de chegada.
    ///
    /// Canal comum nao usa: quando enche, o supervisor abre outro e o cliente
    /// entra la'. Area de canal UNICO nao tem essa saida — abrir uma segunda
    /// cidade quebraria o proposito de ter cidade. Ai a fila e' a resposta
    /// honesta: espera, com a posicao na tela.
    pub fila: std::collections::VecDeque<SessionId>,
    pub ecs: World,
    pub sessions: HashMap<SessionId, Session>,
    pub tick: u32,
    pub removed_this_tick: Vec<EntityId>,
    pub map: shared::world_gen::WorldMap,
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
    /// Contador monotonico pra atribuir IDs de party.
    next_party_id: u32,
    /// Decoracoes estaticas enviadas ao cliente no login.
    pub decorations: Vec<shared::world_gen::DecoPlacement>,
    /// Zonas de spawn carregadas do MapFile — mantem quotas + respawn por delay.
    pub spawn_zones: Vec<ServerSpawnZone>,
    /// Areas dedicadas de boss spawn — independente das spawn_zones, 1 boss
    /// por area, respawn timer separado.
    pub boss_areas: Vec<BossSpawnArea>,
    /// Zonas seguras: combate desabilitado, enemies dropam aggro de quem
    /// entra. (origin_xy, size_xy). Lookup linear — esperado <10 zonas/mapa.
    pub safe_zones: Vec<(Vec2, Vec2)>,
    /// Rotas pré-definidas pra NPCs caminhantes — id → lista de waypoints.
    pub npc_routes: HashMap<u32, NpcRoute>,
    /// Tempo acumulado de simulacao em segundos (pra timer de respawn).
    pub sim_time_s: f32,
    /// Estado global do vento (afeta TODOS os barcos no mundo). Constante
    /// por enquanto — futuro: drift suave + storm events. Broadcast no
    /// login + quando muda significativamente.
    pub wind: WindState,
    /// True quando alguma mudanca critica aconteceu desde o ultimo save —
    /// equip change, inventory swap, mount/dismount, gold transaction, etc.
    /// Tick loop checa isso pra disparar save fora do intervalo periodico,
    /// garantindo persistencia sub-segundo em eventos relevantes.
    pub save_pending: bool,
    /// Tiros ranged em andamento — drenados a cada tick e spawnados quando
    /// `release_tick` é atingido. Sincroniza projétil com fim da anim de saque.
    pending_shots: Vec<PendingShot>,
    pending_skill_hits: Vec<PendingSkillHit>,
    /// Posicoes do ultimo chain dentro do handle_skill_cast — populado pelas
    /// skills com bounce (Chain Lightning 1054, Lightning Bolt 1051 r5+).
    /// Lido pelo broadcast de SkillCastFx no fim do mesmo handle_skill_cast,
    /// depois resetado.
    last_chain_pts: Vec<Vec2>,
    pending_heals: Vec<PendingHeal>,
    pending_delayed_aoe: Vec<DelayedAoe>,
    /// Mapa por net_id → hurt_dir aplicado neste tick. Populado em step() ao
    /// processar damage_events; lido em send_snapshots() pra preencher
    /// EntitySnapshot.hurt_dir; limpo após envio.
    pub hit_this_tick: HashMap<EntityId, Vec2>,
    /// Mapa por net_id → true se o hit do tick foi critico. Lido em
    /// send_snapshots pra `EntitySnapshot.is_crit`. Limpo após envio.
    pub crit_this_tick: HashMap<EntityId, bool>,
    /// Dano REAL do hit (sem clamp pelo HP atual). Cliente usa pra exibir
    /// no floating damage text quando o hit mata o alvo: HP antes era 5,
    /// dano real foi 50 → mostra "50" mesmo. Lido em send_snapshots.
    pub damage_this_tick: HashMap<EntityId, i32>,
    /// Hits de bomba de canhao pendentes — coletados em tick_cannon_bombs ao
    /// explodir, drenados no inicio do processamento de damage_events pra
    /// passar pelo pipeline normal (PvP rules + hit feedback + crit + xp).
    /// (target_entity, target_eid, dmg, attacker_eid_player, hurt_dir,
    ///  attacker_pos_for_dir_calc).
    pub pending_bomb_hits: Vec<(Entity, EntityId, i32, EntityId, Vec2, Vec2)>,
    /// Item_id da arma do atacante no tick em que cada alvo foi ferido.
    /// Cliente usa pra escolher VFX de impacto por arma. Lido em send_snapshots
    /// pra EntitySnapshot.attacker_weapon_id; limpo apos envio.
    pub attacker_weapon_this_tick: HashMap<EntityId, u16>,
    /// Última versão de economy vista no broadcast — quando muda (admin
    /// editou via web), reenviamos `ItemsConfig` pra todos os clientes.
    pub last_econ_version: i64,
    /// Farm nodes carregados do mapfile. Chave = ID sequencial (1-based).
    /// HP reduz a cada FarmHit validado; quando chega a 0, node entra em
    /// respawn_at > 0 e loot é spawnado.
    farm_nodes: HashMap<u32, FarmNodeState>,
    /// Cooldown por (SessionId, node_id) — impede spam de FarmHit acima da
    /// cadência da animação. Valor = sim_time_s quando o cooldown expira.
    farm_hit_cooldowns: HashMap<(SessionId, u32), f32>,
    /// Processo de tutorial (env TUTORIAL_MODE=1). Quando true, players
    /// spawnam numa lane isolada (ver `tutorial_slots`) em vez da pos salva,
    /// não persistem posição, e o chat é restrito à própria lane.
    tutorial_mode: bool,
    /// Lanes isoladas do mapa de tutorial. Cada player ocupa uma; o AOI
    /// (raio 24 tiles) garante que uma lane nunca vê a outra (espaçamento
    /// TUTORIAL_LANE_SPACING). `base` = ponto de spawn da lane. O conteúdo
    /// (NPC guia, nodes, água/barco, walls) é ESTÁTICO no tutorial.json,
    /// replicado por lane — o server só aloca/libera o slot.
    tutorial_slots: Vec<TutorialSlot>,
    /// Processo de dungeon (env DUNGEON_MODE=1). Quando true, players entram
    /// numa lane de dungeon isolada (ver `dungeon_slots`) e percorrem um caminho
    /// predefinido matando salas de mobs até o boss. Espelha `tutorial_mode`.
    dungeon_mode: bool,
    /// Lanes de dungeon — cada player (MVP solo) ocupa uma; o layout (salas,
    /// corredores, gates) é pintado por lane no startup. AOI isola as runs.
    dungeon_slots: Vec<DungeonSlot>,
    /// Runs ativas — máquina de estado por player (sala atual, mobs vivos,
    /// colliders dos gates). Criada no spawn, removida no disconnect/complete.
    dungeon_runs: Vec<DungeonRun>,
}

/// Lane isolada do mapa de tutorial.
struct TutorialSlot {
    base: Vec2,
    occupant: Option<SessionId>,
}

/// Lane de dungeon (DUNGEON_MODE). `base` = canto inferior-esquerdo (tiles) da
/// caixa da dungeon dessa lane.
struct DungeonSlot {
    base: Vec2,
    occupant: Option<SessionId>,
}

/// Run ativa de dungeon — máquina de estado do caminho predefinido.
struct DungeonRun {
    lane: usize,
    occupant: SessionId,
    /// Sala atual sendo combatida (0..NUM_ROOMS-1). A última é o boss.
    room_idx: usize,
    /// Kills acumulados na sala atual (sistema de waves) — gate abre em
    /// DUNGEON_ROOM_KILLS. Resetado ao avançar de sala.
    kills_this_room: u32,
    /// EntityIds dos mobs vivos da WAVE atual. Nova wave quando <= 1 vivo.
    live_enemies: Vec<EntityId>,
    /// Collider Rapier de cada gate (len = NUM_ROOMS-1). Some = fechado;
    /// None = já aberto (removido do collider_set ao limpar a sala).
    /// Tiles que fecham cada ponte: `(coluna, base_y)`. Era handle de
    /// collider do rapier; virou parede de verdade no mapa.
    gate_handles: Vec<Option<(i32, i32)>>,
    /// sim_time pra disparar o DungeonComplete depois do boss (deixa o player
    /// pegar o loot antes de voltar pro mundo). None = boss ainda vivo.
    complete_at: Option<f32>,
    /// sim_time limite da run (DUNGEON_TIME_LIMIT_S após o início). Estourou
    /// → player é mandado de volta pro mundo (run falhou).
    deadline_at: f32,
}

// ── Layout da dungeon lvl 10 (relativo à base da lane, em tiles) ───────────
// ARQUIPÉLAGO: vestíbulo seguro → ponte → 3 ilhas de combate (WAVES até
// DUNGEON_ROOM_KILLS) → ilha do BOSS (bem maior). Pontes com gates (collider
// Rapier em coluna fixa + pedras visuais no client) abrem ao limpar a sala.
// ⚠️ O client (DungeonBuilder.cs) replica EXATAMENTE estas coords.
const DUNGEON_NUM_ROOMS: i32 = 4;       // 3 salas de combate + boss
const DUNGEON_BOX_W: i32 = 88;
const DUNGEON_BOX_H: i32 = 19;
/// Linhas (relativas) das pontes/gates (3 tiles, centro vertical).
const DUNGEON_GATE_ROWS: [i32; 3] = [8, 9, 10];
/// Coluna da ponte vestíbulo→sala0 (sempre ABERTA, sem collider).
const DUNGEON_ENTRANCE_DIV: i32 = 13;
/// Colunas dos gates de combate (sobre as pontes entre as ilhas).
const DUNGEON_COMBAT_GATES: [i32; 3] = [29, 45, 61];
/// Spawn do player (centro de tile, relativo à base) — centro do vestíbulo.
const DUNGEON_ENTRY: (f32, f32) = (6.5, 9.5);
/// Origem das lanes (mar aberto far-right do game.json 12000×2000).
const DUNGEON_ORIGIN: (i32, i32) = (11000, 200);
const DUNGEON_LANE_COUNT: usize = 8;
const DUNGEON_LANE_SPACING: i32 = 80;
/// Segundos entre o boss morrer e o player voltar pro mundo (pegar loot).
const DUNGEON_LOOT_GRACE_S: f32 = 10.0;
/// Kills necessários pra abrir o gate de cada sala de combate (sistema de
/// WAVES: ondas de 3-4 mobs espalhados pela ilha até bater a cota).
const DUNGEON_ROOM_KILLS: u32 = 20;
/// Tempo limite da run (segundos). Estourou sem matar o boss → o player é
/// mandado de volta pro mundo (morrer respawna no vestíbulo, o relógio segue).
const DUNGEON_TIME_LIMIT_S: f32 = 600.0;

fn dungeon_combat_gate_x(i: i32) -> i32 { DUNGEON_COMBAT_GATES[i as usize] }

// ── Noise determinístico da caverna ─────────────────────────────────────
// Hash inteiro + value-noise 1D suave. ⚠️ O client (DungeonBuilder.cs)
// replica ESTAS funções com as MESMAS ops (u32 wrapping + f32 IEEE-754)
// — paredes são colisão server-side, o visual TEM que bater bit a bit
// (mesma técnica da ilha do tutorial).
fn dgn_hash(x: i32, y: i32, seed: u32) -> u32 {
    let mut h = (x as u32).wrapping_mul(374_761_393)
        ^ (y as u32).wrapping_mul(668_265_263) ^ seed;
    h = (h ^ (h >> 13)).wrapping_mul(1_274_126_177);
    h ^ (h >> 16)
}
fn dgn_n01(x: i32, y: i32, seed: u32) -> f32 {
    (dgn_hash(x, y, seed) & 0xFFFF) as f32 / 65535.0
}
/// Value-noise 2D suavizado (lattice 3×3 + smoothstep bilinear). coords >= 0.
fn dgn_smooth2(x: i32, y: i32, seed: u32) -> f32 {
    let cx = x / 3;
    let cy = y / 3;
    let tx = (x % 3) as f32 / 3.0;
    let ty = (y % 3) as f32 / 3.0;
    let sx = tx * tx * (3.0 - 2.0 * tx);
    let sy = ty * ty * (3.0 - 2.0 * ty);
    let a = dgn_n01(cx, cy, seed);
    let b = dgn_n01(cx + 1, cy, seed);
    let c = dgn_n01(cx, cy + 1, seed);
    let d = dgn_n01(cx + 1, cy + 1, seed);
    let top = a + (b - a) * sx;
    let bot = c + (d - c) * sx;
    top + (bot - top) * sy
}

/// As 5 ILHOTAS da dungeon: (cx, cy, rx, ry) relativos à base da lane —
/// vestíbulo + 3 salas de combate + boss. Mesma estética do mundo real:
/// terra de grama cercada de OCEANO (a água é a "parede" — WATER já
/// bloqueia movimento) com pontes de terra entre as ilhas.
const DUNGEON_ISLES: [(f32, f32, f32, f32); 5] = [
    (6.5,  9.5, 5.5, 5.8),    // vestíbulo (spawn 6.5,9.5)
    (21.0, 9.5, 7.2, 6.8),    // sala 0 (waves melee)
    (37.0, 9.5, 7.2, 6.8),    // sala 1 (waves mescladas)
    (53.0, 9.5, 7.2, 6.8),    // sala 2 (waves ranged)
    (73.0, 9.5, 10.5, 8.6),   // boss — ilha bem maior (arena)
];

/// True se o tile (relativo) é TERRA: pontes (faixas de 3 rows nos gates/
/// entrada, ±2 colunas) ou dentro de uma ilhota (elipse + wobble de noise).
/// ⚠️ Espelhado bit-a-bit no client (DungeonBuilder.IsLand).
fn dungeon_is_land(lx: i32, ly: i32) -> bool {
    // Pontes: sempre terra (o bloqueio do gate é collider Rapier no x fixo).
    if DUNGEON_GATE_ROWS.contains(&ly) {
        if (lx - DUNGEON_ENTRANCE_DIV).abs() <= 2 { return true; }
        for i in 0..(DUNGEON_NUM_ROOMS - 1) {
            if (lx - dungeon_combat_gate_x(i)).abs() <= 2 { return true; }
        }
    }
    // Ilhotas: elipse com borda ondulada (wobble ±0.225 no raio normalizado).
    let w = (dgn_smooth2(lx, ly, 0x15E5) - 0.5) * 0.45;
    for (cx, cy, rx, ry) in DUNGEON_ISLES {
        let nx = (lx as f32 + 0.5 - cx) / rx;
        let ny = (ly as f32 + 0.5 - cy) / ry;
        if nx * nx + ny * ny + w < 1.0 { return true; }
    }
    false
}

/// Tile (relativo) da lane: ILHOTAS de grama no oceano. Terra = DUNGEON_FLOOR,
/// resto = WATER (bloqueia movimento + collider Rapier no bake — o mar é a
/// parede natural, igual ao arquipélago do mundo). Gates de combate continuam
/// sendo colliders removíveis em colunas retas sobre as pontes.
fn dungeon_tile(lx: i32, ly: i32) -> u16 {
    use shared::constants::tile_id::{WATER, DUNGEON_FLOOR};
    if lx <= 0 || lx >= DUNGEON_BOX_W - 1 || ly <= 0 || ly >= DUNGEON_BOX_H - 1 {
        return WATER;
    }
    if dungeon_is_land(lx, ly) { DUNGEON_FLOOR } else { WATER }
}

/// Composição das salas de combate: (level dos mobs, pool de classes da wave).
/// Progressão de arquétipo E de level: sala 0 só MELEE lvl 9 → sala 1 MESCLA
/// lvl 10 → sala 2 só RANGED lvl 11. A sala do boss não usa isto.
fn dungeon_room_spec(room: usize) -> (u32, &'static [crate::enemy_builds::EnemyClass]) {
    use crate::enemy_builds::EnemyClass::*;
    match room {
        0 => (9,  &[Sword, Axe, SwordShield]),
        1 => (10, &[Sword, Dagger, Spear, Bow, Staff]),
        2 => (11, &[Bow, Staff, Wand]),
        _ => (10, &[SwordShield]),
    }
}

/// ILHA DE TUTORIAL: criada em RUNTIME (só no processo de tutorial) bem à
/// esquerda do mundo, ANTES das ilhas iniciais (far-left era tudo água). Terra
/// + árvores T1 pintadas em memória; o client pinta a mesma ilha quando
/// InTutorial. Não toca o mapa do mundo (game.json/máscara). Centro = spawn.
/// Centro do conteúdo (Matteo + craft + mob) — ao NORTE da pedra/cliff.
const TUTORIAL_AREA: (f32, f32) = (195.5, 1030.5);
/// Bounding box da ilha de tutorial em tiles (xmin,ymin,xmax,ymax).
const TUTORIAL_ISLAND: (i32, i32, i32, i32) = (150, 990, 240, 1050);
/// Formato ORGÂNICO da ilha = união de elipses (cx,cy,rx,ry). Não é retângulo.
/// DEVE bater com o client (TutorialIslandBuilder.IslandBlobs).
const TUTORIAL_ISLAND_BLOBS: [(f32, f32, f32, f32); 6] = [
    (195.0, 1021.0, 39.0, 27.0), // corpo principal
    (160.0, 1008.0, 15.0, 13.0), // oeste
    (232.0, 1010.0, 13.0, 14.0), // leste
    (176.0, 1041.0, 15.0, 11.0), // lóbulo sudoeste (norte)
    (216.0, 1039.0, 14.0, 12.0), // lóbulo sudeste (norte)
    (198.0, 1001.0, 17.0, 11.0), // sul
];
/// Spawn do player: ao SUL da pedra (precisa pular pra chegar no Matteo).
const TUTORIAL_SPAWN: (f32, f32) = (195.5, 996.5);
/// "Pedra"/cliff: linha de WALL que o player PULA (mecânica climb). y da linha
/// + intervalo x (atravessa a ilha inteira → força o pulo).
const TUTORIAL_CLIFF_Y: i32 = 1012;
// Intervalo largo (além de qualquer borda da ilha) — o guard `contains` no loop
// só pinta WALL onde há ilha, então auto-veda a linha do cliff seja qual for o
// formato (sem brecha pra dar a volta).
const TUTORIAL_CLIFF_X: (i32, i32) = (146, 246);
/// Cais (pier de FLOOR) conectando a ilha ao barco, atravessando a costa murada.
const TUTORIAL_DOCK_X: (i32, i32) = (193, 197);
const TUTORIAL_DOCK_Y: (i32, i32) = (1049, 1051);
/// Canal do barco: água NÃO-murada (gap na costa) onde o barco fica + embarca.
const TUTORIAL_BOAT_CHANNEL: (i32, i32, i32, i32) = (192, 198, 1052, 1055);
/// Barco (água no fim do cais) — dado na quest 905.
const TUTORIAL_BOAT: (f32, f32) = (195.5, 1052.5);
/// Arena de combate (leste) — 2 inimigos lvl 1 espaçados, mantidos vivos
/// enquanto a quest 904 (matar 3) está ativa.
const TUTORIAL_ENEMY_SPOTS: [(f32, f32); 2] = [(214.5, 1032.5), (226.5, 1039.5)];
/// Farm nodes T1 (tile center) — DEVE bater com o client. Árvores p/ a quest
/// de madeira + flores/rochas decorativos/funcionais pra preencher a ilha.
const TUTORIAL_TREES: [(f32, f32); 7] = [
    (172.5, 1038.5), (186.5, 1042.5), (204.5, 1038.5), (216.5, 1042.5),
    // (195.5,1046.5) removida: tapava a vista do cais.
    (168.5, 1024.5), (224.5, 1028.5), (180.5, 1034.5),
];
const TUTORIAL_FLOWERS: [(f32, f32); 8] = [
    (178.5, 1020.5), (210.5, 1018.5), (165.5, 1018.5), (228.5, 1016.5),
    (190.5, 1036.5), (208.5, 1044.5), (174.5, 1044.5), (200.5, 1022.5),
];
const TUTORIAL_ROCKS: [(f32, f32); 5] = [
    (160.5, 1026.5), (230.5, 1022.5), (218.5, 1034.5), (176.5, 1008.5), (212.5, 1006.5),
];

/// True se o tile (x,y) está dentro da ilha de tutorial (união de elipses).
fn tutorial_island_contains(x: i32, y: i32) -> bool {
    let (fx, fy) = (x as f32, y as f32);
    TUTORIAL_ISLAND_BLOBS.iter().any(|&(cx, cy, rx, ry)| {
        let nx = (fx - cx) / rx;
        let ny = (fy - cy) / ry;
        nx * nx + ny * ny <= 1.0
    })
}

/// FLOOR final da ilha = dentro da elipse E com >= 2 dos 8 vizinhos também dentro
/// (erosão). Remove tiles ISOLADOS — o RuleTile junta 2+ tiles, então um tile
/// sozinho vira "grama flutuando". Usa 8-conectividade (com diagonais): só tiles
/// realmente soltos somem; os "polos" da borda (topo/base/laterais da elipse, que
/// têm 1 vizinho cardinal mas 3 diagonais) sobrevivem — senão a fileira do topo
/// inteira erode e o cais perde a conexão e flutua. DEVE bater com o client.
fn tutorial_island_floor(x: i32, y: i32) -> bool {
    if !tutorial_island_contains(x, y) { return false; }
    let n = [(1, 0), (-1, 0), (0, 1), (0, -1), (1, 1), (1, -1), (-1, 1), (-1, -1)].iter()
        .filter(|(dx, dy)| tutorial_island_contains(x + dx, y + dy)).count();
    n >= 2
}

struct FarmNodeState {
    kind:           String,
    tier:           u8,
    pos:            Vec2,
    hp:             i32,
    hp_max:         i32,
    respawn_at:     f32, // 0.0 = vivo; >0 = em respawn até esse sim_time_s
    respawn_seconds: f32, // tempo de respawn configurado por node (default = FARM_NODE_RESPAWN_S)
}

impl GameWorld {
    /// Cria mundo com mapa crafted procedural pequeno (40x30). DEAD CODE em
    /// produção — `tick.rs` sempre chama `new_from_mapfile`. Mantido só pra
    /// testes/REPL local que não querem carregar o game.json. Não use em main.
    #[allow(dead_code)]
    pub fn new(characters: HashMap<String, crate::persistence::CharacterRow>) -> Self {
        // Mapa crafted pequeno (demo do tileset Gentle Forest). Sem inimigos,
        // sem dungeon procedural — apenas vendor + vault + decoracoes.
        let crafted = shared::world_gen::build_crafted_map();
        tracing::info!(
            "mapa crafted gerado ({}x{}, {} decoracoes)",
            crafted.map.width, crafted.map.height, crafted.decorations.len(),
        );
        let decorations = crafted.decorations.clone();
        let vendor_pos = crafted.vendor_pos;
        let vault_pos = crafted.vault_pos;
        let map = crafted.map;
        let safe_zone = false;
        let from_mapfile = true; // evita respawn de enemies (suprime procgen)

        let mut w = Self {
            populacao: None,
            saude: None,
            admissao_travada: false,
            imortal: std::env::var("MMO_IMORTAL").as_deref() == Ok("1"),
            ilha: None,
            diretorio: None,
            zona: "overworld".to_string(),
            fila: std::collections::VecDeque::new(),
            ecs: World::new(),
            sessions: HashMap::new(),
            tick: 0,
            removed_this_tick: Vec::new(),
            map,
            next_entity_id: 1,
            next_player_id: 1,
            characters,
            auth_ctx: None,
            enemy_spawn_timer: 0.0,
            boss_entity: None,
            boss_respawn_timer: f32::INFINITY, // desabilita boss
            safe_zone,
            from_mapfile,
            next_party_id: 1,
            decorations,
            spawn_zones: Vec::new(),
            boss_areas: Vec::new(),
            safe_zones: Vec::new(),
            npc_routes: HashMap::new(),
            sim_time_s: 0.0,
            wind: WindState::default(),
            save_pending: false,
            pending_shots: Vec::new(),
            pending_skill_hits: Vec::new(),
            last_chain_pts: Vec::new(),
            pending_heals: Vec::new(),
            pending_delayed_aoe: Vec::new(),
            hit_this_tick: HashMap::new(),
            crit_this_tick: HashMap::new(),
            damage_this_tick: HashMap::new(),
            pending_bomb_hits: Vec::new(),
            attacker_weapon_this_tick: HashMap::new(),
            last_econ_version: 0,
            farm_nodes: HashMap::new(),
            farm_hit_cooldowns: HashMap::new(),
            tutorial_mode: false,
            tutorial_slots: Vec::new(),
            dungeon_mode: false,
            dungeon_slots: Vec::new(),
            dungeon_runs: Vec::new(),
        };
        w.spawn_vendor_at(vendor_pos);
        w.spawn_vault_at(vault_pos);
        w
    }

    /// Constroi o mundo a partir de um MapFile (exportado pelo editor Unity).
    /// Suprime procgen, vendor/vault crafted e dungeon — tudo vem do arquivo.
    pub fn new_from_mapfile(
        characters: HashMap<String, crate::persistence::CharacterRow>,
        mf: MapFile,
    ) -> Self {
        let (sx, sy) = (mf.spawn[0] as i32, mf.spawn[1] as i32);
        let mut map = mf.to_world_map();
        map.override_spawn = Some((sx, sy));
        tracing::info!(
            "mapfile '{}' carregado ({}x{}, spawn=({},{}), {} entidades)",
            mf.name, map.width, map.height, sx, sy, mf.entities.len(),
        );

        let safe_zone = mf.safe_zone;
        let tutorial_mode = std::env::var("TUTORIAL_MODE")
            .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
            .unwrap_or(false);
        let dungeon_mode = std::env::var("DUNGEON_MODE")
            .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
            .unwrap_or(false);
        // Tutorial: pinta a ilha (FLOOR) ANTES de bakear os colliders Rapier —
        // senão a água original vira collider e prende o player na ilha nova.
        if tutorial_mode {
            use shared::constants::tile_id::{FLOOR, WALL, WATER};
            let (x0, y0, x1, y1) = TUTORIAL_ISLAND;
            for y in y0..=y1 { for x in x0..=x1 {
                if tutorial_island_floor(x, y) { map.set(x, y, FLOOR); }
            }}
            // Cais (pier): FLOOR conectando a ilha ao barco através da costa.
            for y in TUTORIAL_DOCK_Y.0..=TUTORIAL_DOCK_Y.1 {
                for x in TUTORIAL_DOCK_X.0..=TUTORIAL_DOCK_X.1 { map.set(x, y, FLOOR); }
            }
            // Pescoço sólido (3 tiles) ligando o cais ao CORPO da ilha. A erosão
            // come o topo da elipse (só o polo x=195 sobra em y1048), então sem
            // isto o cais vira ilhota flutuante. Liga y1047(corpo)→y1048→cais.
            for y in (TUTORIAL_DOCK_Y.0 - 2)..TUTORIAL_DOCK_Y.0 {
                for x in 194..=196 { map.set(x, y, FLOOR); }
            }
            // Pedra/cliff: a "crista" elevada (ground1) que o player PULA por cima.
            // WALL em 2 linhas (y1011-1012) batendo com o visual (grass_cliff_rule
            // 2 tiles no client) — senão dava pra andar em cima de meia pedra.
            // Jumpável: player em y1010 → FLOOR em y1013 (k=3) dispara o leap.
            for x in TUTORIAL_CLIFF_X.0..=TUTORIAL_CLIFF_X.1 {
                if tutorial_island_contains(x, TUTORIAL_CLIFF_Y) || tutorial_island_contains(x, TUTORIAL_CLIFF_Y - 1) {
                    map.set(x, TUTORIAL_CLIFF_Y, WALL);
                    map.set(x, TUTORIAL_CLIFF_Y - 1, WALL);
                }
            }
            // AUTO-WALL da costa: parede no anel MAIS EXTERNO de FLOOR (a borda da
            // costa) — igual a borda 0/1 tem parede, pro player NÃO ficar em cima da
            // beira ground/ocean (fica bloqueado 1 tile pra dentro). Vale pra todo
            // FLOOR vizinho (8-conn) de água, EXCETO o cais/canal do barco.
            let (dkx0, dkx1) = (TUTORIAL_DOCK_X.0 - 1, TUTORIAL_DOCK_X.1 + 1);
            let (dky0, dky1) = (TUTORIAL_DOCK_Y.0 - 2, TUTORIAL_BOAT_CHANNEL.3);
            let mut coast: Vec<(i32, i32)> = Vec::new();
            for y in (y0 - 1)..=(y1 + 3) {
                for x in (x0 - 1)..=(x1 + 1) {
                    if map.get(x, y) != FLOOR { continue; }
                    if x >= dkx0 && x <= dkx1 && y >= dky0 && y <= dky1 { continue; } // cais/canal
                    let touches_water = (-1..=1i32).any(|dy| (-1..=1i32).any(|dx|
                        (dx != 0 || dy != 0) && map.get(x + dx, y + dy) == WATER));
                    if touches_water { coast.push((x, y)); }
                }
            }
            for (x, y) in coast { map.set(x, y, WALL); }
        }
        // Dungeon: pinta a caixa de TODAS as lanes ANTES de bakear os colliders
        // — as WALLs (perímetro + divisórias) viram colliders Rapier estáticos.
        // Os gaps dos gates ficam FLOOR (sem collider); o bloqueio do gate é um
        // collider separado adicionado por-run no spawn (removido ao limpar a sala).
        let mut dungeon_slots: Vec<DungeonSlot> = Vec::new();
        if dungeon_mode {
            for i in 0..DUNGEON_LANE_COUNT {
                let base = (DUNGEON_ORIGIN.0, DUNGEON_ORIGIN.1 + i as i32 * DUNGEON_LANE_SPACING);
                for ly in 0..DUNGEON_BOX_H {
                    for lx in 0..DUNGEON_BOX_W {
                        map.set(base.0 + lx, base.1 + ly, dungeon_tile(lx, ly));
                    }
                }
                dungeon_slots.push(DungeonSlot {
                    base: Vec2::new(base.0 as f32, base.1 as f32),
                    occupant: None,
                });
            }
        }
        let mut w = Self {
            populacao: None,
            saude: None,
            admissao_travada: false,
            imortal: std::env::var("MMO_IMORTAL").as_deref() == Ok("1"),
            ilha: None,
            diretorio: None,
            zona: "overworld".to_string(),
            fila: std::collections::VecDeque::new(),
            ecs: World::new(),
            sessions: HashMap::new(),
            tick: 0,
            removed_this_tick: Vec::new(),
            map,
            next_entity_id: 1,
            next_player_id: 1,
            characters,
            auth_ctx: None,
            enemy_spawn_timer: 0.0,
            boss_entity: None,
            boss_respawn_timer: f32::INFINITY,
            safe_zone,
            from_mapfile: true,
            next_party_id: 1,
            decorations: Vec::new(),
            spawn_zones: Vec::new(),
            boss_areas: Vec::new(),
            safe_zones: Vec::new(),
            npc_routes: HashMap::new(),
            sim_time_s: 0.0,
            wind: WindState::default(),
            save_pending: false,
            pending_shots: Vec::new(),
            pending_skill_hits: Vec::new(),
            last_chain_pts: Vec::new(),
            pending_heals: Vec::new(),
            pending_delayed_aoe: Vec::new(),
            hit_this_tick: HashMap::new(),
            crit_this_tick: HashMap::new(),
            damage_this_tick: HashMap::new(),
            pending_bomb_hits: Vec::new(),
            attacker_weapon_this_tick: HashMap::new(),
            last_econ_version: 0,
            farm_nodes: HashMap::new(),
            farm_hit_cooldowns: HashMap::new(),
            tutorial_mode,
            tutorial_slots: Vec::new(),
            dungeon_mode,
            dungeon_slots,
            dungeon_runs: Vec::new(),
        };
        if w.tutorial_mode {
            tracing::info!("[tutorial] modo tutorial ON — ilha única, spawn fixo no ponto do mapa");
        }
        if w.dungeon_mode {
            // Safe zone cobrindo o vestíbulo de cada lane — o player nasce aqui
            // e fica intocável enquanto o client carrega o mapa (inimigos só
            // miram players FORA de safe zone). Sai dela ao entrar na sala 0.
            for slot in 0..DUNGEON_LANE_COUNT {
                let base = (DUNGEON_ORIGIN.0, DUNGEON_ORIGIN.1 + slot as i32 * DUNGEON_LANE_SPACING);
                w.safe_zones.push((
                    Vec2::new(base.0 as f32 + 1.0, base.1 as f32 + 1.0),
                    Vec2::new(DUNGEON_ENTRANCE_DIV as f32, (DUNGEON_BOX_H - 2) as f32),
                ));
            }
            tracing::info!("[dungeon] modo dungeon ON — {} lanes (lvl 10, solo, vestíbulo seguro)", DUNGEON_LANE_COUNT);
        }
        w.spawn_mapfile_entities(&mf);
        if w.tutorial_mode { w.spawn_tutorial_island(); w.spawn_tutorial_content(); }
        w
    }

    /// Insere as árvores T1 do tutorial. O chão (FLOOR) + a pedra (WALL) da ilha
    /// são pintados ANTES do build_colliders (em new_from_mapfile) — NÃO repintar
    /// aqui, senão a pedra (WALL no map.get) viraria FLOOR e o climb não dispara.
    /// O client pinta a MESMA ilha quando InTutorial (TutorialIslandBuilder).
    fn spawn_tutorial_island(&mut self) {
        for (tx, ty) in TUTORIAL_TREES.iter()   { self.add_tutorial_farm("Tree", *tx, *ty); }
        for (tx, ty) in TUTORIAL_FLOWERS.iter() { self.add_tutorial_farm("Flower", *tx, *ty); }
        for (tx, ty) in TUTORIAL_ROCKS.iter()   { self.add_tutorial_farm("Rock", *tx, *ty); }
        tracing::info!("[tutorial] {} farm nodes T1 ({} árvores/{} flores/{} rochas)",
            TUTORIAL_TREES.len() + TUTORIAL_FLOWERS.len() + TUTORIAL_ROCKS.len(),
            TUTORIAL_TREES.len(), TUTORIAL_FLOWERS.len(), TUTORIAL_ROCKS.len());
    }

    fn add_tutorial_farm(&mut self, kind: &str, tx: f32, ty: f32) {
        let node_id = self.farm_nodes.len() as u32 + 1;
        let hp_max = shared::farm_node_hp_max(kind, 1);
        self.farm_nodes.insert(node_id, FarmNodeState {
            kind: kind.to_string(), tier: 1, pos: Vec2::new(tx, ty),
            hp: hp_max, hp_max, respawn_at: 0.0, respawn_seconds: shared::FARM_NODE_RESPAWN_S,
        });
    }

    /// Conteúdo estático da ilha de tutorial (1x no boot). Só o Matteo (guia).
    /// O barco NÃO nasce aqui — é dado na quest 905 (spawn_tutorial_boat).
    fn spawn_tutorial_content(&mut self) {
        let (ax, ay) = TUTORIAL_AREA;
        // Matteo: NPC guia do tutorial. Interagir (no tutorial) abre o diálogo
        // custom (dá machado / recebe madeira) — ver handle_interact.
        let eid = self.alloc_entity_id();
        self.ecs.spawn((
            NetId(eid), Position(Vec2::new(ax, ay)), Velocity(Vec2::ZERO),
            EntityKind::Npc(4), BlacksmithTag { name: "Matteo".to_string() }, NpcSkin { preset: 1 },
        ));
        tracing::info!("[tutorial] Matteo (guia) em ({:.0},{:.0})", ax, ay);
    }

    /// Spawna o barco do tutorial (recompensa da quest 905). No-op se já existe.
    fn spawn_tutorial_boat(&mut self) {
        if self.ecs.query::<&BoatTag>().iter().next().is_some() { return; }
        let beid = self.alloc_entity_id();
        let cn = crate::boat_config::get(0).cannons.len();
        self.ecs.spawn((
            NetId(beid), Position(Vec2::new(TUTORIAL_BOAT.0, TUTORIAL_BOAT.1)),
            Velocity(Vec2::ZERO), EntityKind::Boat(0),
            BoatTag {
                kind: 0, owner_pid: PlayerId(0), yaw: 0.0, ang_vel: 0.0, rudder_angle: 0.0,
                sail_position: 0, sail_angle: 0.0, anchor_dropped: true, anchor_progress: 1.0,
                helm_eid: None, sail_eid: None, anchor_eid: None,
                cannon_eids: vec![None; cn], cannon_aim: vec![0.0; cn], cannon_cd_until: vec![0.0; cn],
                passengers: Vec::new(), dir: 4, anim: 0,
            },
        ));
        tracing::info!("[tutorial] barco dado (quest 905) em ({:.0},{:.0})", TUTORIAL_BOAT.0, TUTORIAL_BOAT.1);
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

        // Snapshot de portais (pos, zona destino, target, cooldown restante)
        let portals: Vec<(Entity, Vec2, String, Vec2, f32)> = self.ecs
            .query::<(&Position, &PortalTag)>()
            .iter()
            .map(|(e, (p, pt))| (e, p.0, pt.target_map.clone(), pt.target, pt.cooldown))
            .collect();
        if portals.is_empty() { return; }

        let r_sq = Self::PORTAL_TRIGGER_RADIUS * Self::PORTAL_TRIGGER_RADIUS;

        // Jogadores: (entity, pos, handle)
        let players: Vec<(Entity, Vec2, shared::Solido)> = self.ecs
            .query::<(&Position, &EntityKind, &shared::Solido)>()
            .iter()
            .filter_map(|(e, (p, k, h))| match k {
                EntityKind::Player => Some((e, p.0, *h)),
                _ => None,
            })
            .collect();

        for (pe, ppos, handle) in players {
            for (portal_entity, portal_pos, zona_destino, target, cd) in &portals {
                if *cd > 0.0 { continue; }
                if ppos.distance_squared(*portal_pos) < r_sq {
                    // Outra zona = outro processo. Manda o jogador reconectar
                    // em vez de teleportar dentro deste mapa.
                    if !zona_destino.is_empty() && *zona_destino != self.zona {
                        let host = self.diretorio.as_ref().and_then(|d| d.melhor(zona_destino));
                        match host {
                            Some(h) => {
                                // A posicao de chegada e' gravada AQUI, antes
                                // de mandar o jogador embora. As zonas sao
                                // processos separados que compartilham o
                                // banco: quem recebe le' a posicao salva, e
                                // sem isso o jogador apareceria na cidade nas
                                // coordenadas do campo.
                                if let Ok(mut pos) = self.ecs.get::<&mut Position>(pe) {
                                    pos.0 = *target;
                                }
                                self.save_pending = true;
                                let sid = self.sessions.iter()
                                    .find(|(_, s)| s.entity == Some(pe))
                                    .map(|(k, _)| *k);
                                if let Some(sid) = sid {
                                    if let Some(s) = self.sessions.get(&sid) {
                                        let _ = s.handle.to_client.send(ServerMessage::TrocarZona {
                                            zona: zona_destino.clone(),
                                            host: h,
                                        });
                                    }
                                }
                                tracing::info!(
                                    "troca de zona: {} -> {} em ({:.0},{:.0})",
                                    self.zona, zona_destino, target.x, target.y
                                );
                            }
                            // Zona fora do ar: NAO teleporta pra lugar nenhum.
                            // Sumir o jogador num portal quebrado e' pior que
                            // o portal nao funcionar.
                            None => tracing::warn!(
                                "portal pra zona '{}' sem canal no ar", zona_destino
                            ),
                        }
                        if let Ok(mut pt) = self.ecs.get::<&mut PortalTag>(*portal_entity) {
                            pt.cooldown = 1.0;
                        }
                        continue;
                    }
                    // Teleporta o rigid body
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

    /// Tenta escolher uma posicao FLOOR aleatoria dentro do AABB. Se
    /// `polygon` Some, alem do FLOOR/AABB, valida point-in-polygon (ray
    /// casting). Faz `tries` tentativas; None se todas falham.
    fn pick_tile_in_zone(
        &self,
        origin: Vec2,
        size: Vec2,
        seed: u64,
        tries: u32,
        polygon: Option<&[Vec2]>,
    ) -> Option<Vec2> {
        let mut s = seed;
        for _ in 0..tries {
            s = lcg(s);
            let rx = lcg_f32(s);
            s = lcg(s);
            let ry = lcg_f32(s);
            let px = origin.x + rx * size.x;
            let py = origin.y + ry * size.y;
            let tx = px.floor() as i32;
            let ty = py.floor() as i32;
            if self.map.get(tx, ty) != shared::constants::tile_id::FLOOR {
                continue;
            }
            // Margem anti-wall: rejeita se algum dos 4 cardinais e' WALL.
            // Body do enemy tem raio ~0.4 tile, so' o tile center dentro do
            // FLOOR nao basta — sprite/colisao penetra na wall vizinha.
            let mut blocked = false;
            for (dx, dy) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
                if self.map.get(tx + dx, ty + dy) == shared::constants::tile_id::WALL {
                    blocked = true;
                    break;
                }
            }
            if blocked { continue; }
            // Centro da tile pra teste de poligono.
            let center = Vec2::new(tx as f32 + 0.5, ty as f32 + 0.5);
            if let Some(verts) = polygon {
                if !point_in_polygon(center, verts) { continue; }
            }
            return Some(center);
        }
        None
    }

    /// True se a posição está dentro de qualquer zona segura. Combate
    /// (dano dado/recebido, swing/shoot do player) é bloqueado nessa zona.
    pub fn in_safe_zone(&self, pos: Vec2) -> bool {
        if self.safe_zone { return true; } // mapa inteiro safe (legacy)
        for (origin, size) in &self.safe_zones {
            if pos.x >= origin.x && pos.x < origin.x + size.x
                && pos.y >= origin.y && pos.y < origin.y + size.y
            {
                return true;
            }
        }
        false
    }

    /// Processa todas as spawn zones: preenche quotas iniciais + respawn por delay.
    /// Rodado 1x por tick.
    fn tick_spawn_zones(&mut self) {
        let now = self.sim_time_s;
        // Coleta ações (spawns) primeiro pra não ter borrow conflict com self.ecs.
        // build = None pra spawns legacy (kinds 0-7); Some pra level-range.
        struct PendingSpawn {
            zone_id: u32, kind: u16, pos: Vec2,
            build: Option<crate::enemy_builds::EnemyBuild>,
            /// Indice do slot Poisson; u32::MAX = legacy quotas (sem slot).
            slot_idx: u32,
        }
        let mut pending: Vec<PendingSpawn> = Vec::new();
        // Snapshot de NetIds vivos no ECS — usado pelo orphan cleanup
        // O(1) em vez de query por slot. ~300 entries por tick.
        let alive_eids: std::collections::HashSet<EntityId> = self.ecs
            .query::<&NetId>()
            .iter()
            .map(|(_, n)| n.0)
            .collect();

        // ── Lazy spawn: wake/sleep das zonas por proximidade de player ──────
        // Posicoes de TODOS os players (inclusive em safe zone — wake e
        // geografico, nao ligado a aggro). Cheap: ~poucos players por server.
        let player_pos: Vec<Vec2> = self.ecs
            .query::<(&Position, &PlayerTag)>()
            .iter()
            .map(|(_, (p, _))| p.0)
            .collect();
        // Zonas que vao adormecer agora — coleta primeiro pra evitar borrow
        // conflict ao despawnar mobs no ecs.
        let mut zones_to_sleep: Vec<u32> = Vec::new();
        for z in self.spawn_zones.iter_mut() {
            let near = {
                // AABB com margem; se polygon existir, usa o bbox dele
                // (mais apertado que origin/size em zonas irregulares).
                let (mn, mx) = if let Some(poly) = z.polygon.as_ref() {
                    let mut mn = Vec2::new(f32::INFINITY, f32::INFINITY);
                    let mut mx = Vec2::new(f32::NEG_INFINITY, f32::NEG_INFINITY);
                    for v in poly { mn = mn.min(*v); mx = mx.max(*v); }
                    (mn, mx)
                } else {
                    (z.origin, z.origin + z.size)
                };
                let mn = mn - Vec2::splat(SPAWN_WAKE_MARGIN);
                let mx = mx + Vec2::splat(SPAWN_WAKE_MARGIN);
                player_pos.iter().any(|p|
                    p.x >= mn.x && p.x <= mx.x && p.y >= mn.y && p.y <= mx.y
                )
            };
            if near {
                if !z.active {
                    z.active = true;
                    // Escalona o spawn inicial: 1 mob a cada SPAWN_WAKE_STAGGER_S
                    // (~33ms = 1 tick). Pra zona com 120 slots, povoa em ~4s sem
                    // criar hitch de 120 spawns num tick so. Slots com occupant
                    // (raro, mas possivel se algo ficou pendurado) sao ignorados.
                    // Caminho level-range: stagger por slot_idx.
                    let mut idx = 0u32;
                    for slot in z.slots.iter_mut() {
                        if slot.occupant.is_some() { continue; }
                        if !slot.respawn_at.is_finite() || slot.respawn_at <= now + 0.01 {
                            slot.respawn_at = now + (idx as f32) * SPAWN_WAKE_STAGGER_S;
                            idx += 1;
                        }
                    }
                    // Caminho legacy quotas: stagger pelos itens da queue.
                    for (i, q) in z.respawn_queue.iter_mut().enumerate() {
                        if q.0 <= now + 0.01 {
                            q.0 = now + (i as f32) * SPAWN_WAKE_STAGGER_S;
                        }
                    }
                    tracing::debug!("zona #{} ACORDOU (player perto)", z.id);
                }
                z.last_player_near_at = now;
            } else if z.active && now - z.last_player_near_at > SPAWN_SLEEP_DELAY_S {
                zones_to_sleep.push(z.id);
            }
        }
        // Adormece: despawna mobs vivos da zona + libera slots + zera respawn_queue.
        if !zones_to_sleep.is_empty() {
            let sleep_set: std::collections::HashSet<u32> = zones_to_sleep.iter().copied().collect();
            let to_kill: Vec<(Entity, EntityId, u32)> = self.ecs
                .query::<(&NetId, &SpawnedByZone)>()
                .iter()
                .filter_map(|(e, (n, by))| {
                    if sleep_set.contains(&by.zone_id) { Some((e, n.0, by.zone_id)) } else { None }
                })
                .collect();
            let killed = to_kill.len();
            for (e, eid, _zid) in to_kill {
                let _ = self.ecs.despawn(e);
                self.removed_this_tick.push(eid);
            }
            for z in self.spawn_zones.iter_mut() {
                if !sleep_set.contains(&z.id) { continue; }
                z.active = false;
                for slot in z.slots.iter_mut() {
                    slot.occupant = None;
                    slot.respawn_at = 0.0;
                }
                z.level_range_live = 0;
                if let Some((_, _, c)) = z.level_range {
                    z.level_range_queue = (0..c).map(|_| 0.0_f32).collect();
                }
                for (_, lv) in z.live.iter_mut() { *lv = 0; }
                z.respawn_queue.clear();
                for &(kind, target) in z.quotas.iter() {
                    for _ in 0..target { z.respawn_queue.push((0.0, kind)); }
                }
                tracing::debug!("zona #{} DORMIU (sem player {:.0}s)", z.id, SPAWN_SLEEP_DELAY_S);
            }
            tracing::info!("lazy_spawn: {} zonas dormiram, {} mobs despawnados", zones_to_sleep.len(), killed);
        }

        for zi in 0..self.spawn_zones.len() {
            if !self.spawn_zones[zi].active { continue; }
            let zone_size = self.spawn_zones[zi].size;
            let zone_orig = self.spawn_zones[zi].origin;
            let zone_id = self.spawn_zones[zi].id;

            // Modo level-range com slots Poisson: itera slots fixos. Spawna
            // num slot que esta livre (occupant=None) e respawn_at <= now.
            // Tipo (level+classe) sortado a cada spawn — diversidade visual.
            if let Some((lvl_min, lvl_max, _total_count)) = self.spawn_zones[zi].level_range {
                let _ = zone_orig; let _ = zone_size; // unused no slot path
                let n_slots = self.spawn_zones[zi].slots.len();
                // Orphan cleanup: se occupant referencia entidade que sumiu
                // do ECS (despawn fora do death handler ou state corrompido),
                // libera o slot pra respawn. O(1) via HashSet.
                for slot_idx in 0..n_slots {
                    let occupant = self.spawn_zones[zi].slots[slot_idx].occupant;
                    if let Some(eid) = occupant {
                        if !alive_eids.contains(&eid) {
                            tracing::warn!(
                                "zona #{} slot {} orphan (eid {:?} sumiu) — limpando",
                                self.spawn_zones[zi].id, slot_idx, eid
                            );
                            self.spawn_zones[zi].slots[slot_idx].occupant = None;
                            self.spawn_zones[zi].slots[slot_idx].respawn_at = 0.0;
                            if self.spawn_zones[zi].level_range_live > 0 {
                                self.spawn_zones[zi].level_range_live -= 1;
                            }
                        }
                    }
                }
                for slot_idx in 0..n_slots {
                    let slot = self.spawn_zones[zi].slots[slot_idx];
                    if slot.occupant.is_some() { continue; }
                    if slot.respawn_at > now { continue; }
                    let s0 = (self.tick as u64
                              ^ (zone_id as u64).wrapping_mul(0xC0FFEE)
                              ^ ((slot_idx as u64).wrapping_mul(0xBADC0DE)))
                        .wrapping_mul(0x9E37_79B9);
                    let s_lvl = lcg(s0);
                    let lvl = if lvl_max > lvl_min {
                        lvl_min + (lcg_f32(s_lvl) * (lvl_max - lvl_min + 1) as f32) as u32
                    } else { lvl_min };
                    let class = crate::enemy_builds::random_class_for_level(lvl, lcg(s_lvl));
                    let build = crate::enemy_builds::build_for_level(lvl, class);
                    pending.push(PendingSpawn {
                        zone_id, kind: lvl as u16, pos: slot.pos,
                        build: Some(build),
                        slot_idx: slot_idx as u32,
                    });
                    // Marca slot como pendente (será setado occupant=eid após
                    // place_enemy_in_zone_with_build retornar). Pra evitar
                    // double-spawn no mesmo slot, ja seta um placeholder.
                    self.spawn_zones[zi].slots[slot_idx].respawn_at = f32::INFINITY;
                    self.spawn_zones[zi].level_range_live += 1;
                }
                continue; // skip caminho legacy quotas
            }

            // ── Caminho legacy: quotas por kind ─────────────────────────────
            let mut idx = 0;
            while idx < self.spawn_zones[zi].respawn_queue.len() {
                let ready_at = self.spawn_zones[zi].respawn_queue[idx].0;
                if ready_at > now { idx += 1; continue; }
                let kind = self.spawn_zones[zi].respawn_queue[idx].1;
                self.spawn_zones[zi].respawn_queue.swap_remove(idx);
                let target = self.spawn_zones[zi].quotas.iter()
                    .find(|(k, _)| *k == kind).map(|(_, c)| *c).unwrap_or(0);
                let live = self.spawn_zones[zi].live.iter()
                    .find(|(k, _)| *k == kind).map(|(_, c)| *c).unwrap_or(0);
                if live >= target { continue; }
                let seed = (self.tick as u64 ^ (zone_id as u64 * 0x1357) ^ (kind as u64 * 0x2468))
                    .wrapping_mul(0x9E37_79B9);
                let polygon_ref = self.spawn_zones[zi].polygon.as_deref();
                if let Some(pos) = self.pick_tile_in_zone(zone_orig, zone_size, seed, 40, polygon_ref) {
                    pending.push(PendingSpawn { zone_id, kind, pos, build: None, slot_idx: u32::MAX });
                    if let Some(entry) = self.spawn_zones[zi].live.iter_mut()
                        .find(|(k, _)| *k == kind) { entry.1 += 1; }
                }
            }
        }

        // Executa os spawns (agora que o borrow em spawn_zones soltou).
        // Pra slots Poisson: bind eid → slot.occupant + atualiza SpawnedByZone
        // tag pra ter slot_idx correto (death handler usa pra liberar slot).
        for ps in pending {
            let eid = match ps.build {
                Some(b) => self.place_enemy_in_zone_with_build(ps.pos, b, ps.zone_id, ps.kind),
                None    => { self.place_enemy_in_zone(ps.pos, ps.kind, ps.zone_id); EntityId(0) }
            };
            if eid.0 != 0 && ps.slot_idx != u32::MAX {
                if let Some(zone) = self.spawn_zones.iter_mut().find(|z| z.id == ps.zone_id) {
                    if let Some(slot) = zone.slots.get_mut(ps.slot_idx as usize) {
                        slot.occupant = Some(eid);
                        slot.respawn_at = 0.0;
                    }
                }
                // Patch SpawnedByZone tag com slot_idx (placeholder no spawn).
                let entity_for_eid = self.ecs.query::<(&NetId,)>().iter()
                    .find_map(|(e, (n,))| if n.0 == eid { Some(e) } else { None });
                if let Some(e) = entity_for_eid {
                    if let Ok(mut tag) = self.ecs.get::<&mut SpawnedByZone>(e) {
                        tag.slot_idx = ps.slot_idx;
                    }
                }
            }
        }
    }

    /// Constroi o EnemyTag + Health a partir do EnemyBuild do kind. Stats
    /// efetivos vem de `effective_stats` — mesmo caminho dos players. Leash
    /// configuravel pelo callsite (zona usa raio da zona; boss/Map fixed).
    fn build_enemy_tag(&self, kind: u16, spawn_anchor: Vec2, leash_max: f32, pos: Vec2) -> (EnemyTag, Health) {
        self.build_enemy_tag_from_build(crate::enemy_builds::enemy_build(kind), spawn_anchor, leash_max, pos)
    }

    /// Variante que recebe o build pronto — usado pelas zonas com level-range
    /// que sorteiam (level, classe) → build_for_level → placement.
    fn build_enemy_tag_from_build(&self, build: crate::enemy_builds::EnemyBuild, spawn_anchor: Vec2, leash_max: f32, pos: Vec2) -> (EnemyTag, Health) {
        let stats = effective_stats(
            &build.equipment,
            &build.allocated_points,
            &build.proficiencies,
            &build.learned_skills,
            0, // enemy nao tem xp (poise so' afeta players de qualquer forma)
        );
        let hp_max = stats.hp_max;
        let tag = EnemyTag {
            ai_target: None,
            attack_cooldown: 0.0,
            hurt_until: 0.0,
            hurt_dir: Vec2::ZERO,
            dead: false,
            despawn_at: 0.0,
            wander_timer: 0.0,
            wander_dir: Vec2::X,
            attack_pending: false,
            wander_waypoint: pos,
            spawn_anchor,
            leash_max,
            wander_phase: 1,
            returning_home: false,
            aggro_timer: 0.0,
            spawn_grace_until: self.sim_time_s + shared::ENEMY_SPAWN_GRACE,
            poisoned_until: 0.0,
            stunned_until: 0.0,
            forced_aggro_until: 0.0,
            forced_aggro_target: None,
            mp_current: stats.mp_max as f32,
            skill_cds: std::collections::HashMap::new(),
            stats,
            equipment: build.equipment,
            learned_skills: build.learned_skills,
            level: build.level,
            visual: build.visual,
            attack_cooldown_base: build.attack_cooldown,
            attack_range: build.attack_range,
            detect_range: build.detect_range,
            locomotor_speed: build.locomotor_speed,
            kite_dist: build.kite_dist,
            proj_count: build.proj_count,
            proj_kind: build.proj_kind,
            is_melee: build.is_melee,
            size_scale: build.size_scale,
            xp_reward: build.xp_reward,
            is_boss: build.is_boss,
            knockback_until: 0.0,
            knockback_vel: Vec2::ZERO,
            stamina_current: stats.stamina_max as f32,
            ai_dash_until: 0.0,
            ai_dash_dir: Vec2::ZERO,
            ai_dash_cd_until: 0.0,
            ai_block_until: 0.0,
            ai_block_cd_until: 0.0,
            ai_strafe_sign: 1.0,
            ai_strafe_flip_at: 0.0,
            parry_flash_pending: false,
            dash_anim_pending: false,
            ai_recent_hits: 0.0,
            boss_name: if build.is_boss { Some(build.name.clone()) } else { None },
            leap_until: 0.0,
            leap_start_pos: Vec2::ZERO,
            leap_target: Vec2::ZERO,
            leap_damage: 0,
            leap_radius: 0.0,
            attack_dir: Vec2::X,
        };
        (tag, Health { current: hp_max, max: hp_max })
    }

    /// Tenta castar a primeira skill aprendida cujo cooldown + MP + range
    /// batem com a situacao. Retorna `Some(skill_id)` se castou (caller skipa
    /// auto-attack); `None` senao.
    ///
    /// Implementacao MVP: aplica DAMAGE INSTANT em alvos via cone/aoe da
    /// skill, sem replicar o full handle_skill_cast (que e' player-only).
    /// Skills com `cast_time_s > 0.05` (channeled / wind-up) sao puladas —
    /// enemy AI nao tem framework pra "ficar parado castando" ainda.
    fn try_enemy_cast_skill(
        &mut self,
        enemy_eid: EntityId,
        enemy_pos: Vec2,
        target_pos: Vec2,
    ) -> Option<u32> {
        let now = self.sim_time_s;
        // Resolve hecs::Entity via query (hecs nao tem find_by_id direto).
        let mut enemy_entity: Option<Entity> = None;
        for (e, net) in self.ecs.query::<&NetId>().iter() {
            if net.0 == enemy_eid { enemy_entity = Some(e); break; }
        }
        let entity = enemy_entity?;
        let (learned, mp_current, weapon_id, attack_damage, cd_map) = {
            let tag = self.ecs.get::<&EnemyTag>(entity).ok()?;
            (
                tag.learned_skills.clone(),
                tag.mp_current,
                tag.equipment.weapon.unwrap_or(0),
                tag.stats.attack_damage,
                tag.skill_cds.clone(),
            )
        };
        for ls in &learned {
            let Some(def) = crate::skills::skill_of(ls.skill_id) else { continue };
            if def.is_passive { continue; }
            if def.cast_time_s > 0.05 { continue; }
            let cd_ready = cd_map.get(&ls.skill_id).copied().unwrap_or(0.0);
            if now < cd_ready { continue; }
            let rank_factor = 1.0 - def.per_rank_cost_pct * (ls.rank.saturating_sub(1) as f32);
            let mp_cost = (def.cost_mp as f32 * rank_factor).max(0.0);
            if mp_current < mp_cost { continue; }
            let _prof = shared::Proficiency::from_item(weapon_id).as_db_str();
            // M13: skills universais — sem check de usable_with.
            let dist = enemy_pos.distance(target_pos);
            if dist > def.range_tiles + def.radius_tiles { continue; }
            let dmg = def.base_damage
                + ((attack_damage as f32 * def.scaling_atk) as i32)
                + (def.base_damage as f32 * def.per_rank_dmg_pct
                    * (ls.rank.saturating_sub(1) as f32)) as i32;
            // Leap Strike (1001): IGUAL AO PLAYER — pulo em ARCO (leap_until;
            // client renderiza via leap_y) com dano AoE NO POUSO. Nada de
            // dano instantâneo nem dash+swing simultâneo (era a "estranheza").
            if ls.skill_id == 1001 {
                if let Ok(mut tag) = self.ecs.get::<&mut EnemyTag>(entity) {
                    tag.mp_current -= mp_cost;
                    tag.skill_cds.insert(ls.skill_id, now + def.cooldown_s);
                    tag.leap_until = now + 0.5; // = LEAP_DURATION do player
                    tag.leap_start_pos = enemy_pos;
                    tag.leap_target = target_pos;
                    tag.leap_damage = dmg;
                    tag.leap_radius = def.radius_tiles.max(0.5);
                    tag.ai_dash_until = 0.0; // garante: sem dash junto
                }
                let fx = ServerMessage::SkillCastFx {
                    skill_id: 1001, caster_pos: enemy_pos, target_pos,
                    target_eid: None, caster_eid: Some(enemy_eid),
                    chain_points: None,
                };
                for s in self.sessions.values() {
                    if s.logged_in { let _ = s.handle.to_client.send(fx.clone()); }
                }
                return Some(1001);
            }
            // Skills SELF-CENTERED (range 0, ex Sword Dance): área em volta
            // do CASTER, não do alvo — corrige dano E ancora o FX no corpo
            // do boss (antes o giro tocava em cima do player = "bugado").
            let center = if def.range_tiles <= 0.05 { enemy_pos } else { target_pos };
            match def.target_type.as_str() {
                "aoe_circle" | "cone" => {
                    let radius = def.radius_tiles.max(0.5);
                    let players = self.find_players_in_radius(center, radius);
                    for pid in &players {
                        let hd = (-(center - enemy_pos)).try_normalize().unwrap_or(Vec2::Y);
                        self.pending_skill_hits.push(PendingSkillHit {
                            target_net: *pid, damage: dmg, attacker_net: enemy_eid,
                            hurt_dir: hd, is_crit: false, from_player: false,
                            knockback: def.knockback,
                        });
                    }
                }
                _ => continue, // line/projectile nao suportados em enemy AI ainda
            }
            if let Ok(mut tag) = self.ecs.get::<&mut EnemyTag>(entity) {
                tag.mp_current -= mp_cost;
                tag.skill_cds.insert(ls.skill_id, now + def.cooldown_s);
            }
            let fx = ServerMessage::SkillCastFx {
                skill_id: ls.skill_id, caster_pos: enemy_pos, target_pos: center,
                target_eid: None, caster_eid: Some(enemy_eid),
                chain_points: None,
            };
            for s in self.sessions.values() {
                if s.logged_in { let _ = s.handle.to_client.send(fx.clone()); }
            }
            return Some(ls.skill_id);
        }
        None
    }

    /// Spawna enemy a partir de um EnemyBuild (procedural por level/classe).
    /// Usado pelas zonas level-range. `tag_kind` vai pro EntityKind::Enemy(_)
    /// — pra zonas level-range, e' o level do mob (0..100).
    fn place_enemy_in_zone_with_build(
        &mut self,
        pos: Vec2,
        build: crate::enemy_builds::EnemyBuild,
        zone_id: u32,
        tag_kind: u16,
    ) -> EntityId {
        let (spawn_anchor, leash_max) = if let Some(zone) = self.spawn_zones.iter().find(|z| z.id == zone_id) {
            if zone.level_range.is_some() {
                // Zonas level-range nascem em PACKS. Ancora cada mob no PROPRIO
                // spawn com leash curto pra o pack ficar "camped" (nao se
                // dissolver pela ilha nem migrar pra cidade — o anchor fica
                // dentro da banda). Chase curto + volta pro pack.
                (pos, 12.0)
            } else {
                // Legacy quotas: roam a zona inteira (anchor no centro do AABB).
                let center = zone.origin + zone.size * 0.5;
                let r = zone.size.x.max(zone.size.y) * 0.6;
                (center, r)
            }
        } else {
            (pos, 6.0)
        };
        let class_str = build.class.as_str().to_string();
        let level = build.level;
        let (tag, health) = self.build_enemy_tag_from_build(build, spawn_anchor, leash_max, pos);
        let hp_max = health.max;
        tracing::info!(
            "spawn zona #{}: lv{} {} hp={} pos=({:.1},{:.1})",
            zone_id, level, class_str, hp_max, pos.x, pos.y
        );
        let net_id = self.alloc_entity_id();
        let handle = self.spawn_entity_body(pos);
        self.ecs.spawn((
            NetId(net_id),
            Position(pos),
            Velocity(Vec2::ZERO),
            health,
            EntityKind::Enemy(tag_kind),
            tag,
            // slot_idx setado externamente (caller atualiza apos spawn pra
            // bind correto). Default u32::MAX = legacy / no-slot.
            SpawnedByZone { zone_id, kind: tag_kind, slot_idx: u32::MAX },
            handle,
        ));
        net_id
    }

    /// Wrapper legacy — chama o flow novo passando build derivado de kind.
    fn place_enemy_in_zone(&mut self, pos: Vec2, kind: u16, zone_id: u32) {
        let build = crate::enemy_builds::enemy_build(kind);
        let _ = self.place_enemy_in_zone_with_build(pos, build, zone_id, kind);
    }

    /// Admin: despawna TODOS os enemies (incluindo bosses) e reseta as filas
    /// pra que spawn zones repopulem imediato no proximo tick. Comando /despawn.
    fn admin_despawn_all_enemies(&mut self) {
        let to_despawn: Vec<(Entity, EntityId)> = self.ecs
            .query::<(&NetId, &EntityKind)>()
            .iter()
            .filter_map(|(e, (n, k))| if matches!(k, EntityKind::Enemy(_)) { Some((e, n.0)) } else { None })
            .collect();
        let count = to_despawn.len();
        for (e, eid) in to_despawn {
            let _ = self.ecs.despawn(e);
            self.removed_this_tick.push(eid);
        }
        // Reseta level_range_live + repopula queue pra spawn imediato.
        for zone in self.spawn_zones.iter_mut() {
            zone.level_range_live = 0;
            if let Some((_, _, c)) = zone.level_range {
                zone.level_range_queue = (0..c).map(|_| 0.0_f32).collect();
            }
            // Legacy quotas: reset live + repopular respawn_queue.
            for (k, lv) in zone.live.iter_mut() {
                let _ = k; *lv = 0;
            }
            zone.respawn_queue.clear();
            for &(kind, target) in zone.quotas.iter() {
                for _ in 0..target { zone.respawn_queue.push((0.0, kind)); }
            }
        }
        // Boss areas: forca respawn imediato.
        for area in self.boss_areas.iter_mut() {
            area.current_boss = None;
            area.respawn_at = 0.0;
        }
        tracing::info!("admin /despawn: {count} enemies removidos, zones reabastecidas");
    }

    /// Tick boss areas: spawn quando current_boss == None E sim_time >= respawn_at.
    /// Detect morte: se current_boss aponta pra entity que nao existe mais,
    /// reseta pra None + agenda respawn.
    fn tick_boss_areas(&mut self) {
        let now = self.sim_time_s;

        // Coleta entity_ids vivos — usado pra detectar boss morto.
        let alive_ids: std::collections::HashSet<EntityId> = self.ecs
            .query::<&NetId>()
            .iter()
            .map(|(_, n)| n.0)
            .collect();

        // Detect mortes + coleta intents de spawn.
        struct BossSpawnIntent { area_id: u32, pos: Vec2, level: u32 }
        let mut intents: Vec<BossSpawnIntent> = Vec::new();
        let map_ref = &self.map;
        for area in self.boss_areas.iter_mut() {
            if let Some(boss_eid) = area.current_boss {
                if !alive_ids.contains(&boss_eid) {
                    // Boss morreu — agenda respawn.
                    area.current_boss = None;
                    area.respawn_at = now + area.respawn_s;
                    tracing::info!(
                        "boss area #{}: morto, respawn em {:.0}s",
                        area.id, area.respawn_s
                    );
                }
            }
            // Spawn novo boss se vazio E timer chegou.
            if area.current_boss.is_none() && now >= area.respawn_at {
                let seed = (now.to_bits() as u64) ^ (area.id as u64 * 0xBAD_F00D);
                let polygon = area.polygon.as_deref();
                if let Some(pos) = pick_random_in_polygon_with_map(area.origin, area.size, seed, polygon, 30, Some(map_ref)) {
                    intents.push(BossSpawnIntent { area_id: area.id, pos, level: area.level });
                } else {
                    // Sem pos válida — tenta de novo no próximo tick.
                    area.respawn_at = now + 1.0;
                }
            }
        }

        // Aplica spawns (fora do borrow).
        for it in intents {
            let class = crate::enemy_builds::random_class_for_level(
                it.level,
                (it.area_id as u64).wrapping_mul(0xC0FFEE) ^ self.tick as u64,
            );
            let build = crate::enemy_builds::build_boss(it.level, class);
            let class_str = build.class.as_str().to_string();
            let level = build.level;
            // Anchor = centro da area, leash grande pra boss errar pelo polygon
            let (anchor, leash) = {
                let area = self.boss_areas.iter().find(|a| a.id == it.area_id).unwrap();
                let center = area.origin + area.size * 0.5;
                let r = area.size.x.max(area.size.y) * 0.7;
                (center, r)
            };
            let (tag, health) = self.build_enemy_tag_from_build(build, anchor, leash, it.pos);
            let hp_max = health.max;
            let net_id = self.alloc_entity_id();
            let handle = self.spawn_entity_body(it.pos);
            // tag_kind = level (consistente com level-range zones)
            let tag_kind = level as u16;
            self.ecs.spawn((
                NetId(net_id),
                Position(it.pos),
                Velocity(Vec2::ZERO),
                health,
                EntityKind::Enemy(tag_kind),
                tag,
                handle,
            ));
            // Marca current_boss na area.
            if let Some(area) = self.boss_areas.iter_mut().find(|a| a.id == it.area_id) {
                area.current_boss = Some(net_id);
            }
            tracing::info!(
                "boss area #{}: SPAWN lv{} {} hp={} pos=({:.1},{:.1})",
                it.area_id, level, class_str, hp_max, it.pos.x, it.pos.y
            );
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
                    if self.tutorial_mode || self.dungeon_mode { continue; }
                    let (tag, health) = self.build_enemy_tag(*kind, Vec2::ZERO, 0.0, pos);
                    let net_id = self.alloc_entity_id();
                    let handle = self.spawn_entity_body(pos);
                    let e = self.ecs.spawn((
                        NetId(net_id),
                        Position(pos),
                        Velocity(Vec2::ZERO),
                        health,
                        EntityKind::Enemy(*kind),
                        tag,
                        handle,
                    ));
                    if *kind == 7 { self.boss_entity = Some(e); }
                }
                MapEntity::Npc { name } => {
                    // Legacy: vendor genérico. Vai pra shop_id=1.
                    let eid = self.alloc_entity_id();
                    self.ecs.spawn((
                        NetId(eid),
                        Position(pos),
                        Velocity(Vec2::ZERO),
                        EntityKind::Npc(1),
                        VendorTag { shop_id: 1, name: name.clone() },
                        NpcSkin { preset: 1 }, // merchant default
                    ));
                }
                MapEntity::Vendor { name, shop_id, skin } => {
                    let eid = self.alloc_entity_id();
                    self.ecs.spawn((
                        NetId(eid),
                        Position(pos),
                        Velocity(Vec2::ZERO),
                        EntityKind::Npc(1),
                        VendorTag { shop_id: *shop_id, name: name.clone() },
                        NpcSkin { preset: *skin },
                    ));
                    tracing::info!(
                        "mapfile: vendor '{}' (shop {}) at ({:.1},{:.1})",
                        name, shop_id, pos.x, pos.y
                    );
                }
                MapEntity::Blacksmith { name, skin } => {
                    let eid = self.alloc_entity_id();
                    self.ecs.spawn((
                        NetId(eid),
                        Position(pos),
                        Velocity(Vec2::ZERO),
                        EntityKind::Npc(4), // 4 = blacksmith
                        BlacksmithTag { name: name.clone() },
                        NpcSkin { preset: *skin },
                    ));
                    tracing::info!(
                        "mapfile: blacksmith '{}' at ({:.1},{:.1})",
                        name, pos.x, pos.y
                    );
                }
                MapEntity::WanderNpc { name, route_id, skin } => {
                    let eid = self.alloc_entity_id();
                    self.ecs.spawn((
                        NetId(eid),
                        Position(pos),
                        Velocity(Vec2::ZERO),
                        EntityKind::Npc(3), // 3 = wander NPC
                        WanderRouteTag {
                            route_id:    *route_id,
                            current_idx: 0,
                            // Pausa inicial escalonada (hash do nome) pra dessincronizar.
                            pause_until: self.sim_time_s + 0.5 + (name.len() as f32 % 5.0) * 0.4,
                            name:        name.clone(),
                            giver:       0, // ambiente por default; arauto é promovido no load
                            home:        pos,
                            target:      pos,
                        },
                        NpcSkin { preset: *skin },
                    ));
                }
                MapEntity::NpcRoute { id, waypoints } => {
                    let wp: Vec<Vec2> = waypoints.iter()
                        .map(|w| Vec2::new(w[0], w[1])).collect();
                    self.npc_routes.insert(*id, NpcRoute { id: *id, waypoints: wp });
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
                MapEntity::Portal { target_map, target_spawn } => {
                    let eid = self.alloc_entity_id();
                    let target = Vec2::new(target_spawn[0], target_spawn[1]);
                    self.ecs.spawn((
                        NetId(eid),
                        Position(pos),
                        Velocity(Vec2::ZERO),
                        EntityKind::Portal,
                        PortalTag { target_map: target_map.clone(), target, cooldown: 0.0 },
                    ));
                }
                MapEntity::SafeZone { size } => {
                    self.safe_zones.push((pos, Vec2::new(size[0], size[1])));
                    tracing::info!(
                        "mapfile: safe zone at ({:.1},{:.1}) {}x{}",
                        pos.x, pos.y, size[0], size[1]
                    );
                }
                MapEntity::EnemySpawner { size, quotas, respawn_delay_s, polygon, level_min, level_max, count } => {
                    // Tutorial/dungeon: NÃO cria as spawn zones do arquipélago
                    // (senão mobs do mundo aggrariam o player na área isolada).
                    if self.tutorial_mode || self.dungeon_mode { continue; }
                    let zone_id = self.spawn_zones.len() as u32;
                    let polygon_world: Option<Vec<Vec2>> = polygon.as_ref().map(|verts| {
                        verts.iter()
                            .map(|v| Vec2::new(v[0] + pos.x, v[1] + pos.y))
                            .collect()
                    });
                    let n_verts = polygon_world.as_ref().map(|v| v.len()).unwrap_or(0);
                    // Density agora vem direto do client (AreaPerMob = 210 tiles²).
                    // Sem divider — quantidade exata do count exportado.
                    // Modo level-range: prioritario sobre quotas se ambos setados.
                    let level_range = match (*level_min, *level_max, *count) {
                        (Some(min), Some(max), Some(c)) if min > 0 && max >= min && c > 0 =>
                            Some((min, max, c)),
                        _ => None,
                    };
                    let (quotas_vec, live_vec, respawn_queue) = if let Some((_, _, _)) = level_range {
                        (Vec::new(), Vec::new(), Vec::new())
                    } else {
                        let qv: Vec<(u16, u32)> = quotas.iter()
                            .map(|q| (q.kind, q.count)).collect();
                        let lv: Vec<(u16, u32)> = qv.iter().map(|(k, _)| (*k, 0u32)).collect();
                        let mut rq = Vec::new();
                        for &(kind, count) in &qv {
                            for _ in 0..count { rq.push((0.0f32, kind)); }
                        }
                        (qv, lv, rq)
                    };
                    // Initial fill (level-range): pre-popular queue com `count`
                    // items ready_at=0 pra spawn imediato no primeiro tick.
                    let level_range_queue: Vec<f32> = if let Some((_, _, c)) = level_range {
                        (0..c).map(|_| 0.0_f32).collect()
                    } else { Vec::new() };
                    let zone_size = Vec2::new(size[0], size[1]);

                    // Compute spawn slots via Poisson disk (so' pra zonas
                    // level-range — quotas legacy continua usando random pos).
                    // Garante spacing minimo entre mobs e distribuicao uniforme.
                    let slots: Vec<SpawnSlot> = if let Some((_, _, target_c)) = level_range {
                        let usable_area = polygon_world.as_ref()
                            .map(|p| polygon_area(p))
                            .unwrap_or(zone_size.x * zone_size.y);
                        let slot_seed = ((zone_id as u64).wrapping_mul(0x9E37_79B9))
                            ^ 0xC0FFEE_BAD_DEED_u64;

                        // Spawn em GRUPOS (packs): em vez de `target_c` mobs
                        // soltos uniformes (encontro = 1 mob, sensacao de vazio),
                        // gera target_c/GROUP_SIZE "centros de pack" BEM espalhados
                        // (Poisson + shuffle) e cola GROUP_SIZE mobs colados em
                        // cada centro. Mesmo total de mobs, mas encontros viram
                        // packs — bem mais interessante.
                        const GROUP_SIZE: u32 = 3;
                        const GROUP_RADIUS: f32 = 2.5; // raio do pack em tiles
                        let n_groups = (target_c + GROUP_SIZE - 1) / GROUP_SIZE;

                        // Centros: min_dist dimensionado p/ n_groups → packs
                        // espacados entre si. Bridson gera muitos; shuffle+trunca
                        // p/ n_groups uniformes (mesmo motivo do fix anti-blob).
                        let center_min_dist = adaptive_min_dist(usable_area, n_groups);
                        let mut centers = poisson_disk_in_polygon(
                            polygon_world.as_deref(), pos, zone_size,
                            center_min_dist, &self.map, slot_seed,
                        );
                        if centers.len() > n_groups as usize {
                            fastrand::seed(slot_seed.wrapping_add(0x5EED_5EED));
                            let n = centers.len();
                            for i in (1..n).rev() { let j = fastrand::usize(..=i); centers.swap(i, j); }
                            centers.truncate(n_groups as usize);
                        }

                        // Validacao de membro do pack: dentro do AABB+polygon e
                        // fora de WALL/WATER (cidade ja excluida pelo polygon).
                        let map = &self.map;
                        let amin = pos;
                        let asize = zone_size;
                        let poly_ref = polygon_world.as_deref();
                        let member_ok = |p: Vec2| -> bool {
                            if p.x < amin.x || p.x > amin.x + asize.x { return false; }
                            if p.y < amin.y || p.y > amin.y + asize.y { return false; }
                            if let Some(poly) = poly_ref { if !point_in_polygon(p, poly) { return false; } }
                            let t = map.get(p.x.floor() as i32, p.y.floor() as i32);
                            t != shared::constants::tile_id::WALL && t != shared::constants::tile_id::WATER
                        };

                        fastrand::seed(slot_seed.wrapping_add(0xBEEF_F00D));
                        let mut v: Vec<SpawnSlot> = Vec::with_capacity(target_c as usize);
                        'groups: for c in &centers {
                            for m in 0..GROUP_SIZE {
                                if v.len() >= target_c as usize { break 'groups; }
                                let p = if m == 0 {
                                    *c // lider no centro (ja valido pelo Poisson)
                                } else {
                                    let mut pick = *c; // fallback: empilha no centro
                                    for _ in 0..10 {
                                        let ang = fastrand::f32() * std::f32::consts::TAU;
                                        let dist = 1.0 + fastrand::f32() * GROUP_RADIUS;
                                        let cand = *c + Vec2::new(ang.cos() * dist, ang.sin() * dist);
                                        if member_ok(cand) { pick = cand; break; }
                                    }
                                    pick
                                };
                                v.push(SpawnSlot { pos: p, occupant: None, respawn_at: 0.0 });
                            }
                        }
                        tracing::info!(
                            "zona #{}: {} packs de {} -> {} slots (center_min_dist={:.1}, target={}, area={:.0})",
                            zone_id, centers.len(), GROUP_SIZE, v.len(), center_min_dist, target_c, usable_area,
                        );
                        v
                    } else { Vec::new() };

                    self.spawn_zones.push(ServerSpawnZone {
                        id: zone_id,
                        origin: pos,
                        size: zone_size,
                        respawn_delay_s: *respawn_delay_s,
                        quotas: quotas_vec,
                        live: live_vec,
                        respawn_queue,
                        polygon: polygon_world,
                        level_range,
                        level_range_live: 0,
                        level_range_queue,
                        slots,
                        // Comeca dormente — vai acordar quando o primeiro player
                        // chegar perto. Economiza tick de IA pra zonas distantes.
                        active: false,
                        last_player_near_at: 0.0,
                    });
                    if let Some((mn, mx, c)) = level_range {
                        tracing::info!(
                            "mapfile: spawn zone #{} at ({:.1},{:.1}) {}x{} (LEVEL {}-{}, count {}, {} polygon verts)",
                            zone_id, pos.x, pos.y, size[0], size[1], mn, mx, c, n_verts
                        );
                    } else {
                        tracing::info!(
                            "mapfile: spawn zone #{} at ({:.1},{:.1}) {}x{} ({} kinds legacy, {} polygon verts)",
                            zone_id, pos.x, pos.y, size[0], size[1], quotas.len(), n_verts
                        );
                    }
                }
                MapEntity::BossSpawn { size, level, respawn_s, polygon } => {
                    if self.tutorial_mode || self.dungeon_mode { continue; }
                    let area_id = self.boss_areas.len() as u32;
                    let polygon_world: Option<Vec<Vec2>> = polygon.as_ref().map(|verts|
                        verts.iter().map(|pp| pos + Vec2::new(pp[0], pp[1])).collect()
                    );
                    let n_verts = polygon_world.as_ref().map(|v| v.len()).unwrap_or(0);
                    self.boss_areas.push(BossSpawnArea {
                        id: area_id,
                        origin: pos,
                        size: Vec2::new(size[0], size[1]),
                        polygon: polygon_world,
                        level: *level,
                        respawn_s: *respawn_s,
                        current_boss: None,
                        // Spawn imediato no primeiro tick (respawn_at no passado).
                        respawn_at: 0.0,
                    });
                    tracing::info!(
                        "mapfile: BOSS area #{} at ({:.1},{:.1}) {}x{} (lv{}, respawn {:.0}s, {} verts)",
                        area_id, pos.x, pos.y, size[0], size[1], level, respawn_s, n_verts
                    );
                }
                MapEntity::FarmNode { kind, tier, respawn_seconds } => {
                    let node_id = self.farm_nodes.len() as u32 + 1;
                    let hp_max  = shared::farm_node_hp_max(kind, *tier as u8);
                    let rs      = respawn_seconds.unwrap_or(shared::FARM_NODE_RESPAWN_S).max(1.0);
                    self.farm_nodes.insert(node_id, FarmNodeState {
                        kind:            kind.clone(),
                        tier:            *tier as u8,
                        pos:             pos,
                        hp:              hp_max,
                        hp_max,
                        respawn_at:      0.0,
                        respawn_seconds: rs,
                    });
                    tracing::debug!("farm_node id={node_id} kind={kind} tier={tier} pos=({:.1},{:.1}) respawn={rs}s", pos.x, pos.y);
                }
            }
        }
        tracing::info!("mapfile: spawned {} entidades pre-posicionadas", mf.entities.len());

        // NPCs de facção (kind 6): um em cada sede, no spawn da facção (offset
        // pra não ficar em cima do ponto de spawn do player). Dão quests PvP +
        // loja de pontos. Spawnados server-side (sem placement no mapa).
        let faction_npcs: [(Option<[f32;2]>, shared::Faction, &str); 2] = [
            (mf.morganeer_spawn, shared::Faction::Morganeers, "Capitão Morganeer"),
            (mf.peacemain_spawn, shared::Faction::Peacemain,  "Guardião Peacemain"),
        ];
        for (sp, fac, name) in faction_npcs.iter() {
            if let Some(p) = sp {
                let pos = Vec2::new(p[0] + 4.0, p[1]);
                let eid = self.alloc_entity_id();
                self.ecs.spawn((
                    NetId(eid),
                    Position(pos),
                    Velocity(Vec2::ZERO),
                    EntityKind::Npc(6), // 6 = NPC de facção
                    FactionGiverTag { faction: *fac, name: (*name).to_string() },
                    NpcSkin { preset: 1 },
                ));
                tracing::info!("faction NPC '{}' ({:?}) at ({:.1},{:.1})", name, fac, pos.x, pos.y);
            }
        }

        // Baús de tesouro (kind 7): um por quest TREASURE, no obj_pos da quest.
        for def in shared::quests::QUESTS.iter() {
            if def.obj_kind != shared::quests::objective_kind::TREASURE { continue; }
            let eid = self.alloc_entity_id();
            let pos = Vec2::new(def.obj_x, def.obj_y);
            self.ecs.spawn((
                NetId(eid),
                Position(pos),
                Velocity(Vec2::ZERO),
                EntityKind::Npc(7), // 7 = baú de tesouro
                TreasureChestTag { quest_id: def.id },
            ));
            tracing::info!("treasure chest quest={} at ({:.1},{:.1})", def.id, pos.x, pos.y);
        }

        // Promove 2 moradores (WanderNpc) por cidade a "arautos" com givers
        // únicos (101..112) → cada cidade oferece quests de NPC distintas.
        // Centroides das cidades (devem bater com a cena GameArchipelago).
        const CITY_CENTROIDS: [(f32, f32); 6] = [
            (6800.0, 629.0), (8900.0, 1359.0), (4700.0, 1379.0),
            (850.0, 599.0), (11100.0, 639.0), (600.0, 1329.0),
        ];
        const PER_CITY: usize = 2;
        let mut taken: Vec<hecs::Entity> = Vec::new();
        for (i, (cx, cy)) in CITY_CENTROIDS.iter().enumerate() {
            let c = Vec2::new(*cx, *cy);
            for slot in 0..PER_CITY {
                let mut best: Option<(hecs::Entity, f32)> = None;
                for (e, (p, k)) in self.ecs.query::<(&Position, &EntityKind)>().iter() {
                    if !matches!(k, EntityKind::Npc(3)) { continue; }
                    if taken.contains(&e) { continue; } // não reusar um já promovido
                    let d = p.0.distance_squared(c);
                    if best.map(|(_, bd)| d < bd).unwrap_or(true) { best = Some((e, d)); }
                }
                if let Some((e, _)) = best {
                    taken.push(e);
                    let giver = 101 + (i * PER_CITY + slot) as u16;
                    if let Ok(mut tag) = self.ecs.get::<&mut WanderRouteTag>(e) {
                        tag.giver = giver;
                        tracing::info!("arauto cidade#{} slot{} giver={} ({})", i, slot, giver, tag.name);
                    }
                }
            }
        }
    }

    /// Spawna um vendedor estatico proximo ao spawn tile (decoracao + interacao).
    /// Cria 2 portais — nexus -> dungeon e dungeon -> nexus. So em modo procedural.
    fn spawn_dungeon_portals(&mut self) {
        let w = self.map.width;
        let h = self.map.height;
        let (dx, dy) = shared::world_gen::dungeon_center(w, h);
        let dungeon_target = Vec2::new(dx as f32 + 0.5, dy as f32 + 0.5);

        // Portal no nexus (perto do spawn, lado esquerdo do vendor).
        let nexus_tile = self.map.spawn_tile();
        let nexus_portal_pos = Vec2::new(nexus_tile.0 as f32 - 3.0, nexus_tile.1 as f32 + 1.5);
        // Fallback se a posicao cair em parede.
        let nexus_portal_pos = if self.map.get(nexus_portal_pos.x.floor() as i32, nexus_portal_pos.y.floor() as i32)
            == shared::constants::tile_id::WALL
        {
            Vec2::new(nexus_tile.0 as f32 + 0.5, nexus_tile.1 as f32 + 3.0)
        } else {
            nexus_portal_pos
        };
        let nexus_eid = self.alloc_entity_id();
        self.ecs.spawn((
            NetId(nexus_eid),
            Position(nexus_portal_pos),
            Velocity(Vec2::ZERO),
            EntityKind::Portal,
            PortalTag { target_map: String::new(), target: dungeon_target, cooldown: 0.0 },
        ));
        tracing::info!("portal nexus->dungeon em {:?} -> {:?}", nexus_portal_pos, dungeon_target);

        // Portal na dungeon (centro, teleporta de volta pro spawn do nexus).
        let nexus_spawn = Vec2::new(nexus_tile.0 as f32 + 0.5, nexus_tile.1 as f32 + 0.5);
        let dungeon_portal_pos = Vec2::new(dx as f32 + 0.5, dy as f32 - 5.0);
        let dungeon_portal_pos = if self.map.get(dungeon_portal_pos.x.floor() as i32, dungeon_portal_pos.y.floor() as i32)
            == shared::constants::tile_id::WALL
        {
            dungeon_target
        } else {
            dungeon_portal_pos
        };
        let dung_eid = self.alloc_entity_id();
        self.ecs.spawn((
            NetId(dung_eid),
            Position(dungeon_portal_pos),
            Velocity(Vec2::ZERO),
            EntityKind::Portal,
            PortalTag { target_map: String::new(), target: nexus_spawn, cooldown: 1.0 }, // cooldown inicial pra nao quicar
        ));
        tracing::info!("portal dungeon->nexus em {:?} -> {:?}", dungeon_portal_pos, nexus_spawn);
    }

    /// Spawna inimigos mais fortes na dungeon (Tanks, Berserkers, Boss).
    fn spawn_dungeon_enemies(&mut self) {
        let w = self.map.width;
        let h = self.map.height;
        let (rx, ry, rw, rh) = shared::world_gen::dungeon_room_rect(w, h);
        // 3 tanks + 2 berserkers em posicoes espalhadas dentro da sala.
        let spots = [
            (rx + 3, ry + 3, 1u16),
            (rx + rw - 4, ry + 3, 1),
            (rx + 3, ry + rh - 4, 5),
            (rx + rw - 4, ry + rh - 4, 5),
            (rx + rw / 2, ry + rh / 2 + 3, 1),
        ];
        for (x, y, kind) in spots {
            if self.map.get(x, y) != shared::constants::tile_id::DUNGEON_FLOOR { continue; }
            self.place_enemy(Vec2::new(x as f32 + 0.5, y as f32 + 0.5), kind, 0.0);
        }
        tracing::info!("dungeon: spawn de {} inimigos", spots.len());
    }

    fn handle_alloc_stat_point(&mut self, sid: SessionId, stat: u8) {
        let Some(s) = self.sessions.get_mut(&sid) else { return; };
        if !s.logged_in { return; }
        let idx = stat as usize;
        if idx >= s.allocated_points.len() { return; }
        if s.unspent_points == 0 { return; }
        s.unspent_points -= 1;
        s.allocated_points[idx] = s.allocated_points[idx].saturating_add(1);
        s.stat_points_dirty = true;
        // Recalcula stats. max_hp pode ter subido — HP nao sobe automatico.
        s.stats = effective_stats(&s.equipment, &s.allocated_points, &s.proficiencies, &s.learned_skills, s.xp);
        s.stats_dirty = true;
    }

    /// Refunda todos os pontos alocados pro unspent. Sem custo nem cooldown
    /// — feature de teste/respec livre.
    /// Refina um item — gasta gold, +1 refinement (com chance crescente
    /// de falhar a partir de +5; falha reseta refinement pra 0).
    fn handle_refine_item(&mut self, sid: SessionId, slot_idx: u16) {
        let Some(s) = self.sessions.get_mut(&sid) else { return; };
        if !s.logged_in { return; }
        let idx = slot_idx as usize;
        if idx >= s.inventory.len() { return; }
        let cur_inst = match s.inventory[idx].instance {
            Some(i) => i,
            None => {
                let _ = s.handle.to_client.send(ServerMessage::Chat {
                    from: "[refine]".into(),
                    text: "this item cannot be refined".into(),
                });
                return;
            }
        };
        if cur_inst.refinement >= shared::items::MAX_REFINE {
            let _ = s.handle.to_client.send(ServerMessage::Chat {
                from: "[refine]".into(),
                text: format!("item already at +{} (max)", shared::items::MAX_REFINE),
            });
            return;
        }
        // Custo: 100g × (refinement+1)^2. +0→+1 = 100g, +5→+6 = 3600g, etc.
        let cost = 100u64 * (cur_inst.refinement as u64 + 1).pow(2);
        if s.gold < cost {
            let _ = s.handle.to_client.send(ServerMessage::Chat {
                from: "[refine]".into(),
                text: format!("need {}g (you have {}g)", cost, s.gold),
            });
            return;
        }
        s.gold -= cost;
        // Roll: 100% sucesso até +4. Depois cai 8% por nível (+5=92%, +14=20%).
        let cur_lvl = cur_inst.refinement;
        let success_chance = if cur_lvl < 4 { 1.0 } else { 1.0 - (cur_lvl as f32 - 3.0) * 0.08 };
        let r = fastrand::f32();
        if r < success_chance {
            // Sucesso
            if let Some(inst) = s.inventory[idx].instance.as_mut() {
                inst.refinement += 1;
            }
            let _ = s.handle.to_client.send(ServerMessage::Chat {
                from: "[refine]".into(),
                text: format!("✓ success! item is now +{} (-{}g)", cur_lvl + 1, cost),
            });
        } else {
            // Falha — reseta refinement pra 0
            if let Some(inst) = s.inventory[idx].instance.as_mut() {
                inst.refinement = 0;
            }
            let _ = s.handle.to_client.send(ServerMessage::Chat {
                from: "[refine]".into(),
                text: format!("✗ failed! refinement reset to +0 (-{}g)", cost),
            });
        }
        s.inventory_dirty = true;
        // Se era item equipado (não é o caso aqui — só refina inv), recompute stats
    }

    /// Encrava uma gema (`gem_slot`) num socket livre do item em `item_slot`.
    /// Consome a gema do inventário. Item precisa ter ItemInstance com pelo
    /// menos 1 socket livre. Gema precisa ser id GEM/IRON_INGOT/DRAGON_SCALE
    /// (ver `shared::items::gem_bonus`).
    fn handle_socket_gem(&mut self, sid: SessionId, item_slot: u16, gem_slot: u16) {
        let Some(s) = self.sessions.get_mut(&sid) else { return; };
        if !s.logged_in { return; }
        let item_idx = item_slot as usize;
        let gem_idx  = gem_slot  as usize;
        if item_idx >= s.inventory.len() || gem_idx >= s.inventory.len() {
            return;
        }
        if item_idx == gem_idx { return; }
        // Valida gema
        let gem_id = s.inventory[gem_idx].item_id;
        if s.inventory[gem_idx].qty == 0
            || (gem_id != shared::item_id::GEM
                && gem_id != shared::item_id::IRON_INGOT
                && gem_id != shared::item_id::DRAGON_SCALE)
        {
            let _ = s.handle.to_client.send(ServerMessage::Chat {
                from: "[blacksmith]".into(),
                text: "that's not a gem".into(),
            });
            return;
        }
        // Valida item destino
        let Some(mut inst) = s.inventory[item_idx].instance else {
            let _ = s.handle.to_client.send(ServerMessage::Chat {
                from: "[blacksmith]".into(),
                text: "this item does not support sockets".into(),
            });
            return;
        };
        if inst.sockets == 0 {
            let _ = s.handle.to_client.send(ServerMessage::Chat {
                from: "[blacksmith]".into(),
                text: "this item has no sockets".into(),
            });
            return;
        }
        // Procura socket livre
        let free = inst.socketed_gems.iter().position(|&g| g == 0);
        let Some(slot_pos) = free else {
            let _ = s.handle.to_client.send(ServerMessage::Chat {
                from: "[blacksmith]".into(),
                text: "all sockets are already filled".into(),
            });
            return;
        };
        if slot_pos >= inst.sockets as usize {
            let _ = s.handle.to_client.send(ServerMessage::Chat {
                from: "[blacksmith]".into(),
                text: "all sockets are already filled".into(),
            });
            return;
        }
        inst.socketed_gems[slot_pos] = gem_id;
        s.inventory[item_idx].instance = Some(inst);
        // Consome a gema
        s.inventory[gem_idx].qty -= 1;
        if s.inventory[gem_idx].qty == 0 {
            s.inventory[gem_idx] = shared::InventorySlot::default();
        }
        s.inventory_dirty = true;
        let _ = s.handle.to_client.send(ServerMessage::Chat {
            from: "[blacksmith]".into(),
            text: format!("✓ gem socketed ({}/{})", slot_pos + 1, inst.sockets),
        });
    }

    fn handle_reset_stats(&mut self, sid: SessionId) {
        let Some(s) = self.sessions.get_mut(&sid) else { return; };
        if !s.logged_in { return; }
        let total: u32 = s.allocated_points.iter().sum();
        if total == 0 { return; }
        s.unspent_points = s.unspent_points.saturating_add(total);
        s.allocated_points = [0u32; shared::STAT_COUNT];
        s.stat_points_dirty = true;
        s.stats = effective_stats(&s.equipment, &s.allocated_points, &s.proficiencies, &s.learned_skills, s.xp);
        s.stats_dirty = true;
        tracing::info!("{} resetou atributos (refund {})", s.name, total);
    }

    /// Reseta TODAS as skills aprendidas — refunda SP de volta. Limpa
    /// learned_skills, equipped slots e cooldowns. Tambem limpa estados
    /// passivos (riposte_until, sword_dance_until, etc).
    fn handle_reset_skills(&mut self, sid: SessionId) {
        let Some(s) = self.sessions.get_mut(&sid) else { return; };
        if !s.logged_in { return; }
        // Total de SP gasto = soma dos ranks aprendidos (1 SP por rank).
        let total_spent: u32 = s.learned_skills.iter()
            .map(|ls| ls.rank as u32).sum();
        if total_spent == 0 { return; }
        s.skill_points_spent = s.skill_points_spent.saturating_sub(total_spent);
        s.learned_skills.clear();
        s.skill_cds.clear();
        s.skills_dirty = true;
        // Limpa estados de skills ativas/passivas.
        s.casting_until = 0.0;
        s.casting_skill_id = 0;
        s.riposte_until = 0.0;
        s.sword_dance_until = 0.0;
        s.sword_dance_step = 0;
        s.counter_stance_until = 0.0;
        s.hunter_marks.clear();
        // Recompute stats — sem passivas que escalavam (ex: Combat Stance,
        // Bulwark, Mana Pool, Eagle Eye).
        s.stats = effective_stats(&s.equipment, &s.allocated_points, &s.proficiencies, &s.learned_skills, s.xp);
        s.stats_dirty = true;
        tracing::info!("{} resetou skills (refund {} SP)", s.name, total_spent);
    }

    fn spawn_vendor_at(&mut self, pos: (f32, f32)) {
        let eid = self.alloc_entity_id();
        self.ecs.spawn((
            NetId(eid),
            Position(Vec2::new(pos.0, pos.1)),
            Velocity(Vec2::ZERO),
            EntityKind::Npc(1),
            VendorTag { shop_id: 1, name: "Merchant".into() },
            NpcSkin { preset: 1 },
        ));
        tracing::info!("vendor crafted em {:?}", pos);
    }

    fn spawn_vault_at(&mut self, pos: (f32, f32)) {
        let eid = self.alloc_entity_id();
        self.ecs.spawn((
            NetId(eid),
            Position(Vec2::new(pos.0, pos.1)),
            Velocity(Vec2::ZERO),
            EntityKind::Npc(2),
        ));
        tracing::info!("vault crafted em {:?}", pos);
    }

    #[allow(dead_code)]
    fn spawn_vendor(&mut self) {
        let t = self.map.spawn_tile();
        let pos = Vec2::new(t.0 as f32 + 1.5, t.1 as f32 + 1.5);
        let eid = self.alloc_entity_id();
        self.ecs.spawn((
            NetId(eid),
            Position(pos),
            Velocity(Vec2::ZERO),
            EntityKind::Npc(1),
        ));
        tracing::info!("vendor spawnado (legado) em {:?}", pos);
    }

    #[allow(dead_code)]
    fn spawn_vault_npc(&mut self) {
        let t = self.map.spawn_tile();
        let pos = Vec2::new(t.0 as f32 + 3.0, t.1 as f32 + 1.5);
        let eid = self.alloc_entity_id();
        self.ecs.spawn((
            NetId(eid),
            Position(pos),
            Velocity(Vec2::ZERO),
            EntityKind::Npc(2),
        ));
        tracing::info!("vault spawnado (legado) em {:?}", pos);
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
                        "invalid credentials".to_string()
                    }
                    crate::auth::AuthError::Internal(msg) => format!("internal error: {msg}"),
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

        // Multi-char: salva auth info na sessao e envia CharacterList.
        // Cliente decide selecionar/criar; spawn de fato so acontece em
        // handle_select_character (ou apos CreateCharacter via select).
        if let Some(s) = self.sessions.get_mut(&sid) {
            s.pending_char_creation_account_id = Some(success.account_id);
            s.pending_auth_username = Some(success.username.clone());
            s.pending_auth_class = Some(success.class.clone());
            s.account_id = Some(success.account_id);
        }
        self.send_character_list(sid, success.account_id);
        return;
    }

    /// Envia ao cliente a lista de personagens da conta. Filtra `self.characters`
    /// por account_id e converte em CharacterListEntry. Sempre disparado apos
    /// auth success ou apos CreateCharacter. Inclui tambem available_weapons
    /// (filtrado por items.active=TRUE) pra cliente exibir so armas validas.
    fn send_character_list(&self, sid: SessionId, account_id: i64) {
        let chars: Vec<shared::protocol::CharacterListEntry> = self.characters.values()
            .filter(|r| r.account_id == Some(account_id))
            .map(|r| shared::protocol::CharacterListEntry {
                name: r.name.clone(),
                level: shared::level_of_xp_with_mult(r.xp, crate::economy::xp_multiplier()),
                visual: r.visual.clone().unwrap_or_else(|| shared::VisualConfig::for_class("warrior")),
                weapon_id: r.equipment.weapon,
                faction: r.faction,
            })
            .collect();
        // Whitelist de armas iniciais — filtrada por items.active. Mantem em
        // sync com handle_create_character::ALLOWED_WEAPONS.
        const ALL_STARTING_WEAPONS: &[u16] = &[3, 6, 12, 13, 14, 15, 25, 26];
        let available_weapons: Vec<u16> = ALL_STARTING_WEAPONS.iter()
            .filter(|id| crate::economy::is_item_active(**id))
            .copied()
            .collect();
        if let Some(s) = self.sessions.get(&sid) {
            let _ = s.handle.to_client.send(ServerMessage::CharacterList {
                chars, available_weapons,
            });
        }
    }

    /// Spawn do char selecionado no mundo. Body extraido do antigo on_auth_result.
    /// Validacao de ownership eh responsabilidade do caller (handle_select_character).
    /// Substitui as zonas de spawn do mapfile por zonas achadas NO RELEVO.
    ///
    /// As do mapfile foram desenhadas num mapa de tiles de 180x140; soltas
    /// numa ilha de 1,6 km, a horda inteira nasce empilhada num canto. Pior:
    /// nascem em declive, e mob em ladeira escorrega pro pe' dela.
    ///
    /// Aqui cada posicao de mob e' um sitio plano VALIDADO — `sitio_plano`
    /// exige um disco sem degrau maior que um bloco e com o pe' seco. E o
    /// BICHO nao aparece nesta funcao: o que ela decide e' o NIVEL, por
    /// distancia do desembarque, e quem escolhe a criatura e' a tabela de
    /// nivel. Mob novo entra sem tocar em codigo de mundo.
    pub fn povoar_ilha(&mut self, centro_jogador: Vec2) {
        let (Some(ilha), Some(def)) = (
            self.ilha.as_ref(),
            shared::terreno::def_da_zona(&self.zona),
        ) else {
            return;
        };
        use shared::terreno::BLOCO;
        let raio_un = def.raio_blocos as f32 * BLOCO;

        // ── 1. sitios ────────────────────────────────────────────────────
        // Grade grossa: testar coluna a coluna seriam dez milhoes de discos.
        // 12 blocos (6 unidades) e' mais fino que a menor clareira util.
        let passo = 12i32;
        let raio_mob = (MOB_RAIO_SITIO_UN / BLOCO) as i32;
        let mut sitios: Vec<Vec2> = Vec::new();
        let mut b = -def.raio_blocos;
        while b < def.raio_blocos {
            let mut a = -def.raio_blocos;
            while a < def.raio_blocos {
                let (ix, iz) = (a + def.raio_blocos, b + def.raio_blocos);
                if ilha.sitio_plano(ix, iz, raio_mob) {
                    sitios.push(Vec2::new(a as f32 * BLOCO, b as f32 * BLOCO));
                }
                a += passo;
            }
            b += passo;
        }
        if sitios.is_empty() {
            tracing::warn!("ilha '{}' sem sitio plano — spawn do mapfile mantido", self.zona);
            return;
        }

        // ── 2. zonas ─────────────────────────────────────────────────────
        // Centros espacados: sem isso as zonas se sobrepoem e a mesma
        // clareira recebe tres hordas.
        let mut centros: Vec<Vec2> = Vec::new();
        let mut semente = def.semente as u64 ^ 0x5A17_E5;
        // Embaralha os sitios pra escolha nao virar varredura de cima pra
        // baixo, que agruparia tudo no norte da ilha.
        for i in (1..sitios.len()).rev() {
            semente = lcg(semente);
            sitios.swap(i, (semente % (i as u64 + 1)) as usize);
        }
        for s in &sitios {
            if centros.len() as u32 >= MOB_ZONAS_MAX {
                break;
            }
            if centros.iter().all(|c| c.distance(*s) >= MOB_ZONA_ESPACO_UN) {
                centros.push(*s);
            }
        }

        let mut zonas: Vec<ServerSpawnZone> = Vec::new();
        for (i, c) in centros.iter().enumerate() {
            // Nivel pela distancia do desembarque: perto e' o minimo da ilha,
            // a ponta mais longe e' o maximo. E' a progressao inteira, e ela
            // sai do relevo em vez de uma lista escrita a mao.
            let t = (c.distance(centro_jogador) / raio_un).clamp(0.0, 1.0);
            let faixa = (def.nivel.1 - def.nivel.0) as f32;
            let lv_min = def.nivel.0 + (t * faixa * 0.8) as u32;
            let lv_max = (lv_min + 2 + (t * faixa * 0.2) as u32).min(def.nivel.1);

            // Slots: os sitios dentro do raio da zona, cada um ja' validado
            // como plano. Nenhum mob nasce em ladeira porque nenhum SLOT esta'
            // em ladeira.
            let mut slots: Vec<SpawnSlot> = Vec::new();
            for s in &sitios {
                if slots.len() as u32 >= MOB_POR_ZONA {
                    break;
                }
                if s.distance(*c) > MOB_ZONA_RAIO_UN {
                    continue;
                }
                if slots.iter().any(|o: &SpawnSlot| o.pos.distance(*s) < MOB_ESPACO_UN) {
                    continue;
                }
                slots.push(SpawnSlot { pos: *s, occupant: None, respawn_at: 0.0 });
            }
            if slots.len() < 3 {
                continue;
            }
            let n = slots.len() as u32;
            zonas.push(ServerSpawnZone {
                id: 10_000 + i as u32,
                origin: *c - Vec2::splat(MOB_ZONA_RAIO_UN),
                size: Vec2::splat(MOB_ZONA_RAIO_UN * 2.0),
                respawn_delay_s: 20.0,
                quotas: Vec::new(),
                live: Vec::new(),
                respawn_queue: Vec::new(),
                polygon: None,
                level_range: Some((lv_min, lv_max, n)),
                level_range_live: 0,
                level_range_queue: (0..n).map(|_| 0.0_f32).collect(),
                slots,
                active: false,
                last_player_near_at: -1e9,
            });
        }

        let total: u32 = zonas.iter().map(|z| z.slots.len() as u32).sum();
        tracing::info!(
            "ilha '{}': {} sitios planos, {} zonas, {} mobs nivel {}..{} (mapfile descartado)",
            self.zona, sitios.len(), zonas.len(), total, def.nivel.0, def.nivel.1
        );
        self.spawn_zones = zonas;
        // As areas de boss do mapfile tem o mesmo problema de coordenada.
        self.boss_areas.clear();

        // E TODO o resto do mapfile junto: NPC, vendedor, ferreiro, portal,
        // baus e os 220 nos de recurso nasceram em coordenadas de um mapa de
        // tiles de 180x140 que nao existe mais aqui. Espalhados numa ilha de
        // 1,6 km eles nao ficam "no lugar errado" — ficam em lugar nenhum, e
        // aparecem como cubo cinza andando no meio do nada.
        //
        // Voltam depois, colocados pelo gerador nos sitios planos, do mesmo
        // jeito que os mobs voltaram.
        let restos: Vec<Entity> = self
            .ecs
            .query::<()>()
            .iter()
            .map(|(e, _)| e)
            .filter(|e| self.ecs.get::<&PlayerTag>(*e).is_err())
            .collect();
        let n = restos.len();
        for e in restos {
            if let Ok(net) = self.ecs.get::<&NetId>(e).map(|n| n.0) {
                self.removed_this_tick.push(net);
            }
            let _ = self.ecs.despawn(e);
        }
        self.decorations.clear();
        // Os nos de recurso nao sao entidade do ECS — vivem num mapa proprio e
        // vao pro cliente por outra mensagem. Sem limpar aqui, os 220 do
        // mapfile continuariam aparecendo, e o `n` acima diria "6 descartadas"
        // enquanto o jogador ve' duzentas.
        let nos = self.farm_nodes.len();
        self.farm_nodes.clear();
        tracing::info!(
            "ilha '{}': {} entidades e {} nos de recurso do mapfile descartados",
            self.zona, n, nos
        );
    }

    /// Toque no chao: calcula a rota no SERVIDOR e guarda na sessao.
    ///
    /// Tres travas, e as tres sao contra abuso e nao contra o jogador:
    ///
    ///   * **distancia** — destino do outro lado da ilha faria o A* varrer
    ///     milhoes de nos por clique;
    ///   * **frequencia** — um clique por `ROTA_INTERVALO_S` por sessao, senao
    ///     um cliente modificado pede rota a 30Hz e ocupa o tick inteiro;
    ///   * **orcamento de nos** — teto duro dentro do proprio A*.
    ///
    /// Nada disso confia no cliente: ele manda um ponto, e o ponto so' vira
    /// movimento se o relevo do servidor concordar.
    pub fn handle_mover_para(&mut self, sid: SessionId, destino: Vec2) {
        const ROTA_ALCANCE: f32 = 220.0;
        const ROTA_INTERVALO_S: f32 = 0.2;
        const ROTA_ORCAMENTO: usize = 6_000;

        let agora = self.sim_time_s;
        let Some(pos_atual) = self
            .sessions
            .get(&sid)
            .and_then(|s| s.entity)
            .and_then(|e| self.ecs.get::<&Position>(e).ok().map(|p| p.0))
        else {
            return;
        };
        {
            let Some(s) = self.sessions.get_mut(&sid) else { return };
            if agora - s.rota_pedida_em < ROTA_INTERVALO_S {
                return;
            }
            s.rota_pedida_em = agora;
            s.rota.limpa();
        }
        if !destino.is_finite() || pos_atual.distance(destino) > ROTA_ALCANCE {
            return;
        }
        let Some(ilha) = self.ilha.as_ref() else { return };
        let Some(rota) = ilha.caminho(pos_atual, destino, ROTA_ORCAMENTO) else { return };
        tracing::debug!(
            "rota: {:.0},{:.0} -> {:.0},{:.0} em {} pontos",
            pos_atual.x, pos_atual.y, destino.x, destino.y, rota.len()
        );
        if let Some(s) = self.sessions.get_mut(&sid) {
            s.rota = shared::terreno::SeguidorDeRota::nova(rota, destino);
        }
    }

    /// Poe uma posicao em terra firme.
    ///
    /// As posicoes herdadas (spawn de personagem, zona de mob do mapfile)
    /// foram escolhidas num mapa de tiles plano. Soltas na ilha, muitas caem
    /// no mar. Melhor mover um pouco do que nascer boiando.
    pub fn pousar(&self, p: Vec2) -> Vec2 {
        match &self.ilha {
            Some(i) => i.terra_mais_proxima(p.x, p.y, 400.0),
            None => p,
        }
    }

    /// Teto de jogadores desta instancia. `0` = sem teto.
    fn capacidade_canal(&self) -> usize {
        std::env::var("MMO_CANAL_CAPACIDADE")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(0)
    }

    fn dentro(&self) -> usize {
        self.sessions.values().filter(|s| s.logged_in).count()
    }

    pub fn dentro_pub(&self) -> usize {
        self.dentro()
    }

    /// Fracao do orcamento de tick consumida (p99). `0.0` quando nao ha
    /// medicao ainda — nos primeiros 10s a instancia admite normalmente.
    fn carga_tick(&self) -> f32 {
        self.saude.as_ref().map(|s| s.carga()).unwrap_or(0.0)
    }

    /// Pausa a admissao quando o tick encosta no orcamento, e libera quando
    /// folga de novo.
    ///
    /// Dois limites e nao um: com um so', admitir faz a carga subir, travar faz
    /// cair, e a instancia fica abrindo e fechando a porta. Os 15 pontos entre
    /// travar e destravar sao o que impede esse serrote.
    ///
    /// Nunca SOBE o teto de populacao — so' desce o efetivo. Um teto que sobe
    /// numa hora calma desaba quando o vizinho da maquina acorda, e esta VPS
    /// divide CPU com outros servicos.
    fn atualiza_trava(&mut self) {
        let limite = |nome: &str, padrao: f32| -> f32 {
            std::env::var(nome).ok().and_then(|v| v.parse().ok()).unwrap_or(padrao)
        };
        let trava = limite("MMO_TICK_TRAVA", 0.75);
        let destrava = limite("MMO_TICK_DESTRAVA", 0.60);
        let carga = self.carga_tick();
        if self.admissao_travada {
            if carga < destrava {
                self.admissao_travada = false;
                tracing::info!(
                    "tick folgou ({:.0}% do orcamento); admitindo de novo",
                    carga * 100.0
                );
            }
        } else if carga >= trava {
            self.admissao_travada = true;
            tracing::warn!(
                "tick em {:.0}% do orcamento com {} jogadores; pausando admissao (entra na fila)",
                carga * 100.0,
                self.dentro()
            );
        }
    }

    fn canal_cheio(&self) -> bool {
        let cap = self.capacidade_canal();
        // Duas razoes pra mandar pra fila, e as duas importam: cabeca contada
        // (previsivel, e' o que o jogador ve no "91/100") e carga real (o que
        // de fato quebra). A contagem sozinha nao distingue 60 pessoas
        // espalhadas de 60 num boss com 300 mobs acordados.
        (cap > 0 && self.dentro() >= cap) || self.admissao_travada
    }

    /// Manda a posicao pra cada um na fila. Sem isso o jogador olha pra uma
    /// tela parada sem saber se esta esperando ou travado.
    fn avisa_fila(&self) {
        let total = self.fila.len() as u32;
        for (i, sid) in self.fila.iter().enumerate() {
            if let Some(s) = self.sessions.get(sid) {
                let _ = s.handle.to_client.send(ServerMessage::FilaDeEntrada {
                    posicao: i as u32 + 1,
                    total,
                });
            }
        }
    }

    /// Manda `InfoCanal` pra todo mundo dentro. Chamado a cada poucos
    /// segundos — o numero muda devagar e nao merece um lugar no snapshot.
    pub fn avisa_info_canal(&self) {
        let realm = crate::canais::realm();
        let canal = std::env::var("MMO_CANAL").unwrap_or_else(|_| "1".into());
        let zona = self.zona.clone();
        let jogadores = self.dentro() as u32;
        let capacidade = self.capacidade_canal() as u32;
        for s in self.sessions.values().filter(|s| s.logged_in) {
            let _ = s.handle.to_client.send(ServerMessage::InfoCanal {
                realm: realm.clone(),
                canal: canal.clone(),
                zona: zona.clone(),
                jogadores,
                capacidade,
            });
        }
    }

    /// Admite da fila enquanto houver vaga. Roda uma vez por segundo — nao ha
    /// motivo pra checar 30 vezes.
    pub fn tick_fila(&mut self) {
        // Antes do early-return: a trava tem que ser reavaliada mesmo com a
        // fila vazia, senao ela nunca destrava num canal sem ninguem esperando.
        self.atualiza_trava();
        if self.fila.is_empty() {
            return;
        }
        // Quem desconectou enquanto esperava sai da fila.
        self.fila.retain(|sid| self.sessions.contains_key(sid));
        while !self.canal_cheio() {
            let Some(sid) = self.fila.pop_front() else { break };
            let pendente = self
                .sessions
                .get_mut(&sid)
                .and_then(|s| s.entrada_pendente.take());
            if let Some((success, row)) = pendente {
                self.spawn_for_char(sid, success, row);
            }
        }
        self.avisa_fila();
    }

    fn spawn_for_char(
        &mut self,
        sid: SessionId,
        success: crate::auth::AuthSuccess,
        row: crate::persistence::CharacterRow,
    ) {
        let handle = match self.sessions.get(&sid) {
            Some(s) => s.handle.clone(),
            None => return,
        };
        // Canal cheio: entra na fila em vez de entrar no mundo. O `tick_fila`
        // admite quando abrir vaga, na ordem de chegada.
        if self.canal_cheio() && !self.fila.contains(&sid) {
            self.fila.push_back(sid);
            self.avisa_fila();
            // Guarda o que precisa pra spawnar quando a vez chegar.
            if let Some(s) = self.sessions.get_mut(&sid) {
                s.entrada_pendente = Some((success, row));
            }
            return;
        }
        self.fila.retain(|x| *x != sid);
        let entity_id = match self.sessions.get(&sid) {
            Some(s) => s.entity_id,
            None => return,
        };
        let pid = self.alloc_player_id();
        let default_spawn = {
            let t = self.map.spawn_tile();
            Vec2::new(t.0 as f32 + 0.5, t.1 as f32 + 0.5)
        };
        let (mut spawn, mut health, saved_xp, saved_gold, saved_inv, saved_equip, saved_vault,
             saved_fame, saved_aura, saved_profs, saved_unspent, saved_alloc,
             saved_sp_earned, saved_sp_spent, saved_learned_skills, saved_boat,
             saved_visual, saved_char_name, saved_mounted_local) = (
                row.pos, row.hp, row.xp, row.gold, row.inventory.clone(), row.equipment, row.vault.clone(),
                row.fame, row.aura, row.proficiencies, row.unspent_points, row.allocated_points,
                row.skill_points_earned, row.skill_points_spent, row.learned_skills.clone(),
                row.boat, row.visual.clone(), row.name.clone(), row.mounted_local,
            );
        // Tutorial: spawna numa área ISOLADA do arquipélago (game.json), perto do
        // cluster de árvores. Ignora a pos salva. O BFS abaixo valida walkable.
        let tutorial_slot_idx: Option<usize> = None;
        if self.tutorial_mode {
            spawn = Vec2::new(TUTORIAL_SPAWN.0, TUTORIAL_SPAWN.1);
        }
        // Dungeon: aloca a 1ª lane livre e spawna na entrada da caixa. Kick se
        // todas ocupadas. A run em si (mobs + gates) inicia no fim de spawn_for_char.
        let mut dungeon_lane_idx: Option<usize> = None;
        let dungeon_raid = self.sessions.get(&sid)
            .map(|s| s.pending_dungeon_raid).unwrap_or(false);
        if self.dungeon_mode {
            match self.dungeon_slots.iter().position(|s| s.occupant.is_none()) {
                Some(idx) => {
                    self.dungeon_slots[idx].occupant = Some(sid);
                    dungeon_lane_idx = Some(idx);
                    let base = self.dungeon_slots[idx].base;
                    // RAID: spawna na ponte antes da arena do boss (gates
                    // abertos, sem waves). Normal: vestíbulo.
                    spawn = if dungeon_raid {
                        Vec2::new(base.x + 58.5, base.y + 9.5)
                    } else {
                        Vec2::new(base.x + DUNGEON_ENTRY.0, base.y + DUNGEON_ENTRY.1)
                    };
                }
                None => {
                    if let Some(s) = self.sessions.get(&sid) {
                        let _ = s.handle.to_client.send(ServerMessage::Kick {
                            reason: "Dungeon cheia, tente novamente em instantes".to_string(),
                        });
                    }
                    return;
                }
            }
        }
        // Stats efetivos considerando equipamento salvo + pontos + profs.
        let stats = effective_stats(&saved_equip, &saved_alloc, &saved_profs, &saved_learned_skills, saved_xp);
        // Re-sincroniza o max_hp (classe pode ter sido rebalanceada entre sessoes).
        health.max = stats.hp_max;
        if health.current > health.max { health.current = health.max; }
        if health.current <= 0 { health.current = stats.hp_max; }
        // Valida que a posicao salva nao esta dentro de uma parede ou na
        // agua (mapa pode ter sido regenerado, OU player desconectou montado
        // — agora boats nao auto-mountam, entao precisamos garantir terra).
        // BFS curto procurando walkable proximo ANTES de cair pro spawn default.
        let tx = spawn.x.floor() as i32;
        let ty = spawn.y.floor() as i32;
        if !self.map.is_walkable(tx, ty) {
            // BFS Chebyshev raio 6 procurando tile walkable.
            let mut found: Option<Vec2> = None;
            'bfs: for r in 1i32..=6 {
                for dy in -r..=r {
                    for dx in -r..=r {
                        if dx.abs() != r && dy.abs() != r { continue; }
                        if self.map.is_walkable(tx + dx, ty + dy) {
                            found = Some(Vec2::new(
                                (tx + dx) as f32 + 0.5,
                                (ty + dy) as f32 + 0.5,
                            ));
                            break 'bfs;
                        }
                    }
                }
            }
            if let Some(p) = found {
                tracing::warn!(
                    "saved pos ({tx},{ty}) nao-walkable; teleportando pra terra adjacente {:?}",
                    p
                );
                spawn = p;
            } else {
                tracing::warn!("saved pos ({tx},{ty}) sem terra adjacente; usando spawn default");
                spawn = default_spawn;
            }
        }
        // Numa ilha o walkable de tile acima nao vale nada — o mapa de tiles
        // e' outro mundo. Quem decide e' o relevo, e ele pode empurrar a
        // posicao salva varias dezenas de metros: as coordenadas antigas
        // foram escolhidas num mapa plano de 180x140 e a ilha tem 1,6 km.
        if self.ilha.is_some() {
            let antes = spawn;
            spawn = self.pousar(spawn);
            if antes.distance(spawn) > 0.5 {
                tracing::info!(
                    "spawn {:?} caia na agua; pousado em {:?} ({:.0}m de distancia)",
                    antes, spawn, antes.distance(spawn)
                );
            }
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
            PlayerTag { name: saved_char_name.clone(), player_id: pid, attack_anim_pending: None, combo_step_pending: None },
        ));

        // Restaura barco se player desconectou em cima dele. Spawna entidade
        // Boat na pos salva com sail/anchor restaurados. Se mounted_local
        // tambem foi salvo (player estava em cima do barco), re-monta o
        // player no mesmo local_pos do deck — assim ao logar, voce ta DE
        // VOLTA na mesma posicao do barco.
        if let Some(b) = saved_boat.filter(|_| !self.tutorial_mode && !self.dungeon_mode) {
            let bx = b.pos.x.floor() as i32;
            let by = b.pos.y.floor() as i32;
            if self.map.is_water(bx, by) {
                let boat_eid = self.alloc_entity_id();
                // Inclui o player como passenger se vamos re-mountar.
                let player_eid_for_passenger = entity_id;
                let initial_passengers = if saved_mounted_local.is_some() {
                    vec![player_eid_for_passenger]
                } else { Vec::new() };
                let boat_entity = self.ecs.spawn((
                    NetId(boat_eid),
                    Position(b.pos),
                    Velocity(Vec2::ZERO),
                    EntityKind::Boat(b.kind),
                    BoatTag {
                        kind: b.kind,
                        owner_pid: pid,
                        yaw: b.yaw,
                        ang_vel: 0.0,
                        rudder_angle: 0.0,
                        sail_position: b.sail_position,
                        sail_angle: b.sail_angle,
                        anchor_dropped: b.anchor_dropped,
                        anchor_progress: if b.anchor_dropped { 1.0 } else { 0.0 },
                        helm_eid: None,
                        sail_eid: None,
                        anchor_eid: None,
                        cannon_eids: vec![None; crate::boat_config::get(b.kind).cannons.len()],
                        cannon_aim:  vec![0.0; crate::boat_config::get(b.kind).cannons.len()],
                        cannon_cd_until: vec![0.0; crate::boat_config::get(b.kind).cannons.len()],
                        passengers: initial_passengers,
                        dir: b.dir,
                        anim: 0,
                    },
                ));
                if let Some(local) = saved_mounted_local {
                    let _ = self.ecs.insert_one(e, Mounted {
                        boat_entity,
                        boat_eid,
                        local_pos: local,
                        station: None, // estacao nao persistida — solta ao relogar
                        last_move_dir: Vec2::ZERO,
                    });
                    tracing::info!("login: boat + player re-mountado pid={:?} kind={} local={:?}", pid, b.kind, local);
                } else {
                    tracing::info!("login: boat restaurado VAZIO pid={:?} kind={} pos={:?}", pid, b.kind, b.pos);
                }
            } else {
                tracing::warn!("login: boat pos ({bx},{by}) nao-water — restauracao abortada");
            }
        }

        if let Some(s) = self.sessions.get_mut(&sid) {
            s.entity = Some(e);
            s.logged_in = true;
            s.tutorial_slot = tutorial_slot_idx;
            s.dungeon_slot = dungeon_lane_idx;
            s.name = saved_char_name.clone();
            s.pending_char_creation_account_id = None;
            s.player_id = pid;
            s.account_id = Some(success.account_id);
            s.stats = stats;
            s.equipment = saved_equip;
            s.xp = saved_xp;
            s.gold = saved_gold;
            s.gold_last_sent = u64::MAX; // forca envio inicial
            s.fame = saved_fame;
            s.fame_last_sent = u64::MAX;
            s.aura = saved_aura;
            s.aura_last_sent = u64::MAX;
            s.proficiencies = saved_profs;
            s.proficiencies_dirty = true;
            // Níveis de farm derivam do XP de prof (Mineração/Lenhador/Coleta).
            // As colunas *_lvl no DB são só cache legado.
            s.woodcutting_lvl = shared::proficiency_level(
                saved_profs[shared::Proficiency::Woodcutting as usize]).max(1);
            s.mining_lvl      = shared::proficiency_level(
                saved_profs[shared::Proficiency::Mining as usize]).max(1);
            s.gathering_lvl   = shared::proficiency_level(
                saved_profs[shared::Proficiency::Gathering as usize]).max(1);
            s.faction         = row.faction;
            s.quests          = row.quests.clone();
            s.quests_dirty    = false;
            s.faction_points  = row.faction_points;
            s.faction_points_last_sent = u32::MAX;
            s.unspent_points = saved_unspent;
            s.allocated_points = saved_alloc;
            s.stat_points_dirty = true;
            s.last_level = shared::level_of_xp_with_mult(saved_xp, crate::economy::xp_multiplier());
            s.inventory = saved_inv.clone();
            s.inventory_dirty = false;
            s.stats_dirty = false;
            // Skills (Phase 1) — copia o estado salvo pra session.
            s.skill_points_earned = saved_sp_earned;
            s.skill_points_spent = saved_sp_spent;
            s.learned_skills = saved_learned_skills.clone();
            s.skills_dirty = false; // já enviamos PlayerSkillsUpdate no fim do login
            s.vault = saved_vault;
            s.vault_dirty = false;
            s.mp_current = stats.mp_max as f32;
            s.mp_last_sent = stats.mp_max;
            s.stamina_current = stats.stamina_max as f32;
            s.stamina_last_sent = stats.stamina_max;
            s.poise_current = stats.poise_max as f32;
            s.poise_last_sent = stats.poise_max;
            // Visual: usa o salvo (escolhido na criacao). Fallback pra class
            // default pra contas legacy sem visual_json.
            s.visual = saved_visual.clone()
                .unwrap_or_else(|| shared::VisualConfig::for_class(&success.class));
        }
        tracing::info!(
            "login ok: {} (acc {}, xp {}) -> {:?} / {:?}",
            success.username, success.account_id, saved_xp, pid, entity_id
        );
        let _ = handle.to_client.send(ServerMessage::LoginOk {
            player_id: pid,
            entity_id,
            spawn: [spawn.x, spawn.y],
        });
        let _ = handle.to_client.send(ServerMessage::MapChange {
            map_name: self.zona.clone(),
            width: self.map.width,
            height: self.map.height,
            // Cliente renderiza da própria scene (GameArchipelago) e NÃO consome
            // este array. Enviar 24M tiles do arquipélago seriam ~72MB de JSON
            // por login — mandamos vazio. (Se um dia o cliente precisar dos tiles
            // do server, reverter pra self.map.tiles.clone()).
            tiles: Vec::new(),
            spawn: [spawn.x, spawn.y],
            safe_zone: self.safe_zone,
            decorations: self.decorations.clone(),
            safe_zones: self.safe_zones.iter()
                .map(|(o, s)| shared::protocol::SafeZoneRect {
                    x: o.x, y: o.y, width: s.x, height: s.y,
                })
                .collect(),
        });
        let _ = handle.to_client.send(ServerMessage::StatsUpdate {
            stats,
            equipment: saved_equip,
        });
        // Estado do vento (Boat 2.5D) — cliente desenha indicador na HUD.
        let _ = handle.to_client.send(ServerMessage::WindUpdate {
            direction: self.wind.direction,
            intensity: self.wind.intensity,
        });
        // Farm nodes — envia lista completa pra cliente associar IDs.
        let farm_nodes_list: Vec<shared::protocol::FarmNodeInfo> = self.farm_nodes.iter()
            .map(|(&id, n)| shared::protocol::FarmNodeInfo {
                id, x: n.pos.x, y: n.pos.y, kind: n.kind.clone(), tier: n.tier,
            })
            .collect();
        let _ = handle.to_client.send(ServerMessage::FarmNodesConfig {
            nodes: farm_nodes_list,
        });
        // Farm skill levels do player.
        if let Some(s_ref) = self.sessions.get(&sid) {
            let _ = handle.to_client.send(ServerMessage::FarmSkillsUpdate {
                woodcutting: s_ref.woodcutting_lvl,
                mining:      s_ref.mining_lvl,
                gathering:   s_ref.gathering_lvl,
            });
        }
        let _ = handle.to_client.send(ServerMessage::ManaUpdate {
            current: stats.mp_max,
        });
        let _ = handle.to_client.send(ServerMessage::StaminaUpdate {
            current: stats.stamina_max,
        });
        let _ = handle.to_client.send(ServerMessage::ProgressUpdate {
            xp: saved_xp,
            level: shared::level_of_xp_with_mult(saved_xp, crate::economy::xp_multiplier()),
        });
        let _ = handle.to_client.send(ServerMessage::InventoryUpdate {
            slots: saved_inv,
        });
        let _ = handle.to_client.send(ServerMessage::ItemsConfig {
            items: crate::economy::items_config(),
        });
        // Skills (Phase 1): catálogo + estado do player.
        let _ = handle.to_client.send(ServerMessage::SkillsConfig {
            skills: crate::skills::all_skills(),
        });
        // Recipes (Phase crafting): catalogo do DB. Cliente reconstroi UI.
        let _ = handle.to_client.send(ServerMessage::CraftRecipes {
            recipes: crate::recipes::all(),
        });
        // Reverse-index das loot tables — cliente usa pra mostrar "como obter"
        // ao clicar num material faltante na UI de crafting.
        let _ = handle.to_client.send(ServerMessage::ResourceSources {
            items: crate::economy::resource_sources_snapshot(),
        });
        let _ = handle.to_client.send(ServerMessage::PlayerSkillsUpdate {
            state: shared::skills::PlayerSkillsState {
                sp_earned: saved_sp_earned,
                sp_spent: saved_sp_spent,
                skills: saved_learned_skills.clone(),
            },
        });
        // Quests ativas + pontos de facção + givers disponíveis (indicador "!").
        self.send_quest_log(sid);
        self.send_quest_givers(sid);

        // Dungeon: inicia a run (fecha gates + spawna mobs da sala 0). DEPOIS do
        // "despawn close enemies" do spawn (senão limparia os mobs da sala 0).
        if self.dungeon_mode {
            if let Some(lane) = dungeon_lane_idx {
                self.begin_dungeon_run(sid, lane, entity_id, dungeon_raid);
            }
        }
    }

    // ── Dungeon (DUNGEON_MODE) ────────────────────────────────────────────

    /// Inicia a run de dungeon de um player recém-spawnado na lane: fecha todos
    /// os gates (collider Rapier no gap) e spawna os mobs da sala 0.
    fn begin_dungeon_run(&mut self, sid: SessionId, lane: usize, occupant_eid: EntityId, raid: bool) {
        let base = self.dungeon_slots[lane].base;
        let bx = base.x as i32;
        let by = base.y as i32;
        // RAID BOSS: sem waves/gates — player spawna na ponte da arena e luta
        // só contra o boss (room_idx=3 direto; pontes todas abertas).
        let (gate_handles, live_enemies, room_idx) = if raid {
            let handles: Vec<Option<(i32, i32)>> =
                vec![None; (DUNGEON_NUM_ROOMS - 1) as usize];
            let boss = self.spawn_dungeon_boss_unit(lane);
            (handles, boss, (DUNGEON_NUM_ROOMS - 1) as usize)
        } else {
            // Gate de cada ponte: 3 tiles de parede fechando o gap (linhas
            // 8-10 da ponte). Era um collider avulso do rapier; virou o mesmo
            // WALL que o resto do mundo usa, entao `move_and_slide` ja
            // resolve — e o cliente enxerga o gate, que antes era invisivel
            // porque so' existia na fisica.
            let mut handles: Vec<Option<(i32, i32)>> = Vec::new();
            for i in 0..(DUNGEON_NUM_ROOMS - 1) {
                let gx = bx + dungeon_combat_gate_x(i);
                for dy in 8..11 {
                    self.map.set(gx, by + dy, shared::constants::tile_id::WALL);
                }
                handles.push(Some((gx, by)));
            }
            (handles, self.spawn_dungeon_wave(lane, 0, 4), 0)
        };
        let deadline_at = self.sim_time_s + DUNGEON_TIME_LIMIT_S;
        self.dungeon_runs.push(DungeonRun {
            lane, occupant: sid, room_idx, kills_this_room: 0,
            live_enemies, gate_handles, complete_at: None, deadline_at,
        });
        tracing::info!("[dungeon] run iniciada lane={} occupant_eid={:?} raid={}",
            lane, occupant_eid, raid);
    }

    /// Spawna uma WAVE de `count` mobs ESPALHADOS pela ilhota da sala (pontos
    /// aleatórios dentro da elipse, validados contra dungeon_is_land). Classe
    /// sorteada do pool da sala (dungeon_room_spec). Devolve os EntityIds.
    fn spawn_dungeon_wave(&mut self, lane: usize, room: usize, count: u32) -> Vec<EntityId> {
        let base = self.dungeon_slots[lane].base;
        let (level, pool) = dungeon_room_spec(room);
        let isle = DUNGEON_ISLES[room + 1];
        let mut s = lcg(self.tick as u64 ^ ((lane as u64) << 8) ^ 0x3A7E);
        let mut ids = Vec::new();
        let mut attempts = 0;
        while ids.len() < count as usize && attempts < 150 {
            attempts += 1;
            s = lcg(s);
            let ang = lcg_f32(s) * std::f32::consts::TAU;
            s = lcg(s);
            let rad = lcg_f32(s).sqrt() * 0.75; // disco uniforme, margem da costa
            let lx = (isle.0 + ang.cos() * rad * isle.2).floor() as i32;
            let ly = (isle.1 + ang.sin() * rad * isle.3).floor() as i32;
            if !dungeon_is_land(lx, ly) { continue; }
            s = lcg(s);
            let class = pool[(s % pool.len() as u64) as usize];
            let build = crate::enemy_builds::build_for_level(level, class);
            let pos = Vec2::new(base.x + lx as f32 + 0.5, base.y + ly as f32 + 0.5);
            ids.push(self.spawn_dungeon_enemy(pos, build, level as u16));
        }
        ids
    }

    /// Spawna o BOSS no centro da ilha final (kind 7 = loot de world-boss).
    fn spawn_dungeon_boss_unit(&mut self, lane: usize) -> Vec<EntityId> {
        let base = self.dungeon_slots[lane].base;
        let isle = DUNGEON_ISLES[4];
        let pos = Vec2::new(base.x + isle.0 + 0.5, base.y + isle.1);
        let build = crate::enemy_builds::build_dungeon_boss(
            10, crate::enemy_builds::EnemyClass::SwordShield);
        vec![self.spawn_dungeon_enemy(pos, build, 7)]
    }

    /// Espelha `place_enemy` mas aceita um build pronto + o kind a guardar no
    /// EntityKind (loot/sprite) e devolve o EntityId. leash alto (a sala já
    /// confina) pra o mob não "voltar pra casa" no meio.
    fn spawn_dungeon_enemy(&mut self, pos: Vec2, build: crate::enemy_builds::EnemyBuild, stored_kind: u16) -> EntityId {
        let kind = stored_kind;
        let (tag, health) = self.build_enemy_tag_from_build(build, pos, 14.0, pos);
        let eid = self.alloc_entity_id();
        let handle = self.spawn_entity_body(pos);
        self.ecs.spawn((
            NetId(eid), handle, Position(pos), Velocity(Vec2::ZERO),
            health, EntityKind::Enemy(kind), tag,
        ));
        eid
    }

    /// Tick das runs: detecta sala limpa → abre gate + avança; boss morto →
    /// agenda DungeonComplete (deixa pegar loot). Só roda em DUNGEON_MODE.
    fn tick_dungeon_runs(&mut self) {
        // NetIds de inimigos AINDA vivos (não-dead) neste tick.
        let alive: std::collections::HashSet<EntityId> = self.ecs
            .query::<(&NetId, &EnemyTag)>()
            .iter()
            .filter(|(_, (_, t))| !t.dead)
            .map(|(_, (n, _))| n.0)
            .collect();
        let now = self.sim_time_s;
        let n = self.dungeon_runs.len();
        let mut completes: Vec<SessionId> = Vec::new();
        for ri in 0..n {
            if let Some(t) = self.dungeon_runs[ri].complete_at {
                if now >= t { completes.push(self.dungeon_runs[ri].occupant); }
                continue;
            }
            // TEMPO ESGOTADO (10 min): run falhou → manda de volta pro mundo.
            if now >= self.dungeon_runs[ri].deadline_at {
                tracing::info!("[dungeon] tempo esgotado lane={} — expulsando",
                    self.dungeon_runs[ri].lane);
                completes.push(self.dungeon_runs[ri].occupant);
                continue;
            }
            // Drena os mortos da wave atual e credita os kills.
            let before = self.dungeon_runs[ri].live_enemies.len();
            self.dungeon_runs[ri].live_enemies.retain(|e| alive.contains(e));
            let alive_n = self.dungeon_runs[ri].live_enemies.len();
            self.dungeon_runs[ri].kills_this_room += (before - alive_n) as u32;
            let room_idx = self.dungeon_runs[ri].room_idx;
            let lane = self.dungeon_runs[ri].lane;
            let occupant = self.dungeon_runs[ri].occupant;
            let kills = self.dungeon_runs[ri].kills_this_room;
            let is_boss_room = room_idx + 1 >= DUNGEON_NUM_ROOMS as usize;
            if !is_boss_room {
                if kills >= DUNGEON_ROOM_KILLS && alive_n == 0 {
                    // Cota batida e wave limpa → gate abre, próxima sala.
                    self.open_dungeon_gate(ri, room_idx);
                    let next = room_idx + 1;
                    let ids = if next + 1 >= DUNGEON_NUM_ROOMS as usize {
                        self.spawn_dungeon_boss_unit(lane)
                    } else {
                        self.spawn_dungeon_wave(lane, next, 4)
                    };
                    self.dungeon_runs[ri].room_idx = next;
                    self.dungeon_runs[ri].live_enemies = ids;
                    self.dungeon_runs[ri].kills_this_room = 0;
                    self.send_dungeon_room_cleared(occupant, room_idx as u32);
                } else if alive_n <= 1 && kills + (alive_n as u32) < DUNGEON_ROOM_KILLS {
                    // Wave quase limpa → próxima onda (3-4, capada pelo que
                    // falta da cota). Mantém o fluxo contínuo de inimigos.
                    let mut nw = 3 + (lcg(self.tick as u64 ^ ri as u64) % 2) as u32;
                    nw = nw.min(DUNGEON_ROOM_KILLS - kills - alive_n as u32);
                    if nw > 0 {
                        let mut ids = self.spawn_dungeon_wave(lane, room_idx, nw);
                        self.dungeon_runs[ri].live_enemies.append(&mut ids);
                    }
                }
            } else if alive_n == 0 {
                // Boss morto: anuncia vitória + agenda a volta pro mundo.
                self.dungeon_runs[ri].complete_at = Some(now + DUNGEON_LOOT_GRACE_S);
                self.send_dungeon_room_cleared(occupant, room_idx as u32);
                tracing::info!("[dungeon] boss derrotado lane={} — completa em {}s", lane, DUNGEON_LOOT_GRACE_S);
            }
        }
        for sid in completes { self.complete_dungeon_run(sid); }
    }

    /// Remove o collider de um gate do collider_set (abre fisicamente a passagem).
    fn open_dungeon_gate(&mut self, run_idx: usize, gate_idx: usize) {
        let gate = self.dungeon_runs[run_idx].gate_handles
            .get_mut(gate_idx).and_then(|o| o.take());
        if let Some((gx, by)) = gate {
            for dy in 8..11 {
                self.map.set(gx, by + dy, shared::constants::tile_id::DUNGEON_FLOOR);
            }
        }
    }

    fn send_dungeon_room_cleared(&self, sid: SessionId, room_idx: u32) {
        if let Some(s) = self.sessions.get(&sid) {
            let _ = s.handle.to_client.send(ServerMessage::DungeonRoomCleared {
                room_idx, total_rooms: DUNGEON_NUM_ROOMS as u32,
            });
        }
    }

    /// Boss derrotado + grace expirado: manda o client voltar pro mundo e limpa
    /// a run. O XP/loot do boss já foi concedido no caminho normal de morte.
    fn complete_dungeon_run(&mut self, sid: SessionId) {
        if let Some(s) = self.sessions.get(&sid) {
            let _ = s.handle.to_client.send(ServerMessage::DungeonComplete {
                world_host: String::new(), world_path: String::new(),
            });
        }
        self.end_dungeon_run(sid);
        tracing::info!("[dungeon] run completa → DungeonComplete enviado");
    }

    /// Limpa uma run: remove gates restantes, despawna mobs vivos e libera a lane.
    /// Chamado no complete e no disconnect.
    fn end_dungeon_run(&mut self, sid: SessionId) {
        let Some(pos) = self.dungeon_runs.iter().position(|r| r.occupant == sid) else { return; };
        let run = self.dungeon_runs.remove(pos);
        for (gx, by) in run.gate_handles.into_iter().flatten() {
            for dy in 8..11 {
                self.map.set(gx, by + dy, shared::constants::tile_id::DUNGEON_FLOOR);
            }
        }
        let ids: std::collections::HashSet<EntityId> = run.live_enemies.iter().copied().collect();
        let to_despawn: Vec<(Entity, EntityId)> = self.ecs.query::<&NetId>().iter()
            .filter(|(_, n)| ids.contains(&n.0))
            .map(|(e, n)| (e, n.0))
            .collect();
        for (e, eid) in to_despawn {
            self.free_entity_body(e);
            let _ = self.ecs.despawn(e);
            self.removed_this_tick.push(eid);
        }
        if let Some(slot) = self.dungeon_slots.get_mut(run.lane) {
            if slot.occupant == Some(sid) { slot.occupant = None; }
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

    /// Marcador de corpo solido.
    ///
    /// Existia pra criar rigid body + collider no rapier. Hoje `Position` no
    /// ECS e' a unica verdade e a colisao e' resolvida no passo F por
    /// `move_and_slide` + separacao de circulos, entao aqui so' sobra a marca
    /// de "esta entidade empurra e e' empurrada".
    fn spawn_entity_body(&mut self, _pos: Vec2) -> shared::Solido {
        shared::Solido
    }

    /// Sem corpo paralelo pra liberar: o despawn do ECS basta.
    fn free_entity_body(&mut self, _e: Entity) {}

    /// Raio minimo entre um inimigo novo e um ponto "seguro" (spawn default
    /// ou posicao de um jogador).
    const ENEMY_SAFE_RADIUS: f32 = 12.0;

    /// Inimigo de TUTORIAL: level 1 (fraco), visual goblin (kind 0), ancorado no
    /// próprio spawn com leash curto (fica na arena, não persegue pela ilha).
    fn place_tutorial_enemy(&mut self, pos: Vec2) {
        let build = crate::enemy_builds::build_for_level(1, crate::enemy_builds::EnemyClass::Sword);
        let (mut tag, health) = self.build_enemy_tag_from_build(build, pos, 10.0, pos);
        tag.attack_cooldown = 0.0;
        let eid = self.alloc_entity_id();
        let handle = self.spawn_entity_body(pos);
        self.ecs.spawn((
            NetId(eid), handle, Position(pos), Velocity(Vec2::ZERO), health,
            EntityKind::Enemy(0), tag,
        ));
    }

    /// Cria um inimigo no tile `pos` com kind e cooldown de ataque iniciais.
    fn place_enemy(&mut self, pos: Vec2, kind: u16, attack_cd: f32) {
        let (mut tag, health) = self.build_enemy_tag(kind, Vec2::ZERO, 0.0, pos);
        tag.attack_cooldown = attack_cd;
        let eid = self.alloc_entity_id();
        let handle = self.spawn_entity_body(pos);
        self.ecs.spawn((
            NetId(eid),
            handle,
            Position(pos),
            Velocity(Vec2::ZERO),
            health,
            EntityKind::Enemy(kind),
            tag,
        ));
    }

    // ── Pesca: peixes do oceano ──────────────────────────────────────────

    /// True se o tile que contém `p` é água. Free helper pra usar dentro de
    /// loops que já têm `self.ecs` emprestado mutável (não pega `&self`).
    fn tile_is_water(map: &shared::world_gen::WorldMap, p: Vec2) -> bool {
        map.get(p.x.floor() as i32, p.y.floor() as i32) == shared::constants::tile_id::WATER
    }

    /// Sorteia a espécie de um peixe (1-4) com viés pra comuns.
    fn random_fish_species(seed: u64) -> u16 {
        let r = lcg_f32(seed);
        if r < 0.50 { 1 } else if r < 0.78 { 2 } else if r < 0.93 { 3 } else { 4 }
    }

    /// Acha um tile de água perto de `center` (entre 6 e FISH_VIEW_RADIUS).
    /// None se não achou em 12 tentativas (player longe de água).
    fn pick_water_near(&self, center: Vec2, seed: u64) -> Option<Vec2> {
        let mut s = seed;
        for _ in 0..12 {
            s = lcg(s);
            let ang = lcg_f32(s) * std::f32::consts::TAU;
            s = lcg(s);
            let rad = 6.0 + lcg_f32(s) * (shared::FISH_VIEW_RADIUS - 6.0);
            let p = center + Vec2::new(ang.cos(), ang.sin()) * rad;
            if self.map.get(p.x.floor() as i32, p.y.floor() as i32)
                == shared::constants::tile_id::WATER
            {
                return Some(p);
            }
        }
        None
    }

    /// Cria um peixe nadando em `pos` (sem physics body — movido em tick_fish).
    fn place_fish(&mut self, pos: Vec2, species: u16, seed: u64) {
        let eid = self.alloc_entity_id();
        let ang = lcg_f32(lcg(seed)) * std::f32::consts::TAU;
        self.ecs.spawn((
            NetId(eid),
            Position(pos),
            Velocity(Vec2::ZERO),
            EntityKind::Fish(species),
            FishTag {
                species,
                wander_dir: Vec2::new(ang.cos(), ang.sin()),
                wander_timer: 0.0,
                hooked_by: None,
            },
        ));
    }

    /// Mantém a população de peixes perto dos players, move (wander + atração
    /// leve pela boia), e detecta a fisgada (peixe encosta na boia → FishingBite).
    fn tick_fish(&mut self, dt: f32) {
        // Centros = posições dos players logados (âncora pra spawn/cull).
        let mut centers: Vec<Vec2> = Vec::new();
        for s in self.sessions.values() {
            if !s.logged_in { continue; }
            if let Some(e) = s.entity {
                if let Ok(p) = self.ecs.get::<&Position>(e) { centers.push(p.0); }
            }
        }
        if centers.is_empty() { return; }

        // Boias ativas: (sid, pos). Usadas pra atração e (separadamente) fisgada.
        let bobbers: Vec<(SessionId, Vec2)> = self.sessions.iter()
            .filter_map(|(sid, s)| s.fishing_bobber.map(|b| (*sid, b)))
            .collect();

        let tick = self.tick;
        let cull_sq = shared::FISH_CULL_RADIUS * shared::FISH_CULL_RADIUS;
        let swim_step = shared::FISH_SWIM_SPEED * dt;

        // ── Move + cull ──────────────────────────────────────────────────
        let mut to_despawn: Vec<(Entity, EntityId)> = Vec::new();
        {
            let map = &self.map;
            let mut seq: u64 = 0;
            for (e, (net, pos, vel, tag)) in self.ecs
                .query_mut::<(&NetId, &mut Position, &mut Velocity, &mut FishTag)>()
            {
                // Fisgado: gruda na boia da sessão dona. Se a boia sumiu, solta.
                if let Some(sid) = tag.hooked_by {
                    if let Some((_, bpos)) = bobbers.iter().find(|(s, _)| *s == sid) {
                        let to = *bpos - pos.0;
                        vel.0 = to * 4.0;
                        pos.0 += vel.0 * dt;
                    } else {
                        tag.hooked_by = None;
                        vel.0 = Vec2::ZERO;
                    }
                    continue;
                }

                // Re-sorteia direção de wander periodicamente.
                tag.wander_timer -= dt;
                if tag.wander_timer <= 0.0 {
                    seq += 1;
                    let s = (tick as u64)
                        .wrapping_mul(0x9E37_79B9)
                        .wrapping_add((e.id() as u64).wrapping_mul(0x2545_F491))
                        .wrapping_add(seq);
                    let ang = lcg_f32(lcg(s)) * std::f32::consts::TAU;
                    tag.wander_dir = Vec2::new(ang.cos(), ang.sin());
                    tag.wander_timer = 1.0 + lcg_f32(lcg(s ^ 0xABCD)) * 2.5;
                }

                // Atração LEVE pela boia mais próxima dentro do raio (+ sorte:
                // mistura com o wander_dir em vez de mirar direto).
                let mut dir = tag.wander_dir;
                let mut nearest: Option<(f32, Vec2)> = None;
                for (_, bpos) in bobbers.iter() {
                    let d = bpos.distance(pos.0);
                    if d < shared::FISH_ATTRACT_RADIUS
                        && nearest.map_or(true, |(bd, _)| d < bd)
                    {
                        nearest = Some((d, *bpos));
                    }
                }
                if let Some((_, bpos)) = nearest {
                    let to_b = (bpos - pos.0).normalize_or_zero();
                    let k = shared::FISH_ATTRACT_STRENGTH;
                    dir = (dir * (1.0 - k) + to_b * k).normalize_or_zero();
                }

                // Integra mantendo o peixe na água (reflete na borda water/land).
                let mut newpos = pos.0 + dir * swim_step;
                if !Self::tile_is_water(map, newpos) {
                    tag.wander_dir = -tag.wander_dir;
                    newpos = pos.0 + tag.wander_dir * swim_step;
                    if !Self::tile_is_water(map, newpos) { newpos = pos.0; }
                }
                vel.0 = (newpos - pos.0) / dt.max(1e-4);
                pos.0 = newpos;

                // Cull: longe de todos os players.
                let mut min_d_sq = f32::MAX;
                for c in centers.iter() { min_d_sq = min_d_sq.min(c.distance_squared(pos.0)); }
                if min_d_sq > cull_sq { to_despawn.push((e, net.0)); }
            }
        }
        for (e, eid) in to_despawn {
            let _ = self.ecs.despawn(e);
            self.removed_this_tick.push(eid);
        }

        // ── Repopula até o alvo ──────────────────────────────────────────
        let fish_count = self.ecs.query::<&FishTag>().iter().count();
        let target = (shared::FISH_PER_PLAYER * centers.len()).min(64);
        if fish_count < target {
            let missing = target - fish_count;
            for i in 0..missing {
                let seed = (tick as u64)
                    .wrapping_mul(0x2545_F491)
                    .wrapping_add((i as u64).wrapping_mul(0x9E37_79B9));
                let c = centers[(seed as usize) % centers.len()];
                if let Some(p) = self.pick_water_near(c, seed) {
                    let species = Self::random_fish_species(seed ^ 0xF00D);
                    self.place_fish(p, species, seed);
                }
            }
        }

        // ── Fisgada: boia sem peixe fisgado pega o peixe livre mais próximo ──
        let pending: Vec<(SessionId, Vec2)> = self.sessions.iter()
            .filter(|(_, s)| s.fishing_bobber.is_some() && s.hooked_fish.is_none())
            .map(|(sid, s)| (*sid, s.fishing_bobber.unwrap()))
            .collect();
        for (sid, bpos) in pending {
            let mut best: Option<(Entity, EntityId, u16, f32)> = None;
            for (e, (net, pos, tag)) in self.ecs
                .query::<(&NetId, &Position, &FishTag)>().iter()
            {
                if tag.hooked_by.is_some() { continue; }
                let d = bpos.distance(pos.0);
                if d <= shared::FISH_HOOK_RADIUS
                    && best.map_or(true, |(_, _, _, bd)| d < bd)
                {
                    best = Some((e, net.0, tag.species, d));
                }
            }
            if let Some((fe, feid, species, _)) = best {
                if let Ok(mut t) = self.ecs.get::<&mut FishTag>(fe) {
                    t.hooked_by = Some(sid);
                }
                if let Some(s) = self.sessions.get_mut(&sid) {
                    s.hooked_fish = Some(feid);
                    let _ = s.handle.to_client.send(ServerMessage::FishingBite {
                        fish_eid: feid,
                        species,
                    });
                }
            }
        }
    }

    /// Solta o peixe fisgado (se houver) sem removê-lo do mundo — volta a nadar.
    /// Limpa o estado da sessão e o `hooked_by` do peixe.
    fn release_hooked_fish(&mut self, sid: SessionId) {
        let feid = self.sessions.get(&sid).and_then(|s| s.hooked_fish);
        if let Some(feid) = feid {
            let ent = self.ecs.query::<&NetId>().iter()
                .find(|(_, n)| n.0 == feid).map(|(e, _)| e);
            if let Some(e) = ent {
                if let Ok(mut t) = self.ecs.get::<&mut FishTag>(e) { t.hooked_by = None; }
            }
        }
        if let Some(s) = self.sessions.get_mut(&sid) { s.hooked_fish = None; }
    }

    /// Player lançou a boia em `pos`. Valida vara equipada + água no alcance,
    /// e passa a atrair peixes pra esse ponto.
    fn handle_fishing_cast(&mut self, sid: SessionId, pos: Vec2) {
        let player_pos = {
            let Some(s) = self.sessions.get(&sid) else { return };
            if !s.logged_in { return; }
            let Some(e) = s.entity else { return };
            if shared::item_id::tool_kind(s.equipment.tool_rod.unwrap_or(0))
                != Some(shared::ToolKind::FishingRod)
            { return; }
            let Ok(p) = self.ecs.get::<&Position>(e) else { return };
            p.0
        };
        if player_pos.distance(pos) > shared::FISH_CAST_MAX_RANGE { return; }
        if self.map.get(pos.x.floor() as i32, pos.y.floor() as i32)
            != shared::constants::tile_id::WATER
        { return; }
        // Re-cast solta qualquer peixe que estava fisgado.
        self.release_hooked_fish(sid);
        if let Some(s) = self.sessions.get_mut(&sid) {
            s.fishing_bobber = Some(pos);
        }
    }

    /// Resultado do minigame. success → remove o peixe e concede o item.
    /// Em qualquer caso limpa a boia e o estado de fisga.
    fn handle_fishing_reel(&mut self, sid: SessionId, success: bool) {
        let feid = self.sessions.get(&sid).and_then(|s| s.hooked_fish);
        let found = feid.and_then(|feid| {
            self.ecs.query::<(&NetId, &FishTag)>().iter()
                .find(|(_, (n, _))| n.0 == feid)
                .map(|(e, (n, t))| (e, n.0, t.species))
        });
        if success {
            if let Some((e, eid, species)) = found {
                let _ = self.ecs.despawn(e);
                self.removed_this_tick.push(eid);
                let item = shared::item_id::fish_item_for_species(species);
                if let Some(s) = self.sessions.get_mut(&sid) {
                    add_to_inventory(&mut s.inventory, item, 1, None);
                    s.inventory_dirty = true;
                }
            }
        } else if let Some((e, _, _)) = found {
            if let Ok(mut t) = self.ecs.get::<&mut FishTag>(e) { t.hooked_by = None; }
        }
        if let Some(s) = self.sessions.get_mut(&sid) {
            s.hooked_fish = None;
            s.fishing_bobber = None;
        }
    }

    /// Cancela a pesca: solta o peixe fisgado e limpa a boia.
    fn handle_fishing_cancel(&mut self, sid: SessionId) {
        self.release_hooked_fish(sid);
        if let Some(s) = self.sessions.get_mut(&sid) { s.fishing_bobber = None; }
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
            let cd = (placed as f32 * 0.3) % crate::economy::enemy_def(kind).attack_cooldown;
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
            let (tag, health) = self.build_enemy_tag(7, Vec2::ZERO, 0.0, pos);
            let net_id = self.alloc_entity_id();
            let handle = self.spawn_entity_body(pos);
            let e = self.ecs.spawn((
                NetId(net_id),
                Position(pos),
                Velocity(Vec2::ZERO),
                health,
                EntityKind::Enemy(7),
                tag,
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
                entrada_pendente: None,
                target: None,
                rota: shared::terreno::SeguidorDeRota::default(),
                pulo_ate: 0.0,
                pulo_pronto_em: 0.0,
                rota_pedida_em: -1e9,
                last_sent: HashMap::new(),
                entity: None,
                entity_id,
                last_input_seq: 0,
                pending_input: None,
                logged_in: false,
                auth_in_flight: false,
                attack_cooldown: 0.0,
                dash_until: 0.0,
                dash_dir: Vec2::ZERO,
                dash_cooldown: 0.0,
                hurt_until: 0.0,
            idle_since: 0.0,
                hurt_dir: Vec2::ZERO,
                knockback_until: 0.0,
                knockback_vel: Vec2::ZERO,
                combo_step: 0,
                combo_last_attack: 0.0,
                respawn_timer: None,
                downed: false,
                downed_heal_timer: 0.0,
                downed_hp: 0,
                downed_last_sent: (false, 0, 0),
                name: String::new(),
                player_id: PlayerId(0),
                account_id: None,
                pending_char_creation_account_id: None,
                pending_auth_username: None,
                pending_auth_class: None,
                stats: shared::base_player_stats(),
                equipment: shared::Equipment::default(),
                unspent_points: 0,
                allocated_points: [0; shared::STAT_COUNT],
                stat_points_dirty: false,
                last_level: 1,
                mp_current: 0.0,
                mp_last_sent: 0,
                stamina_current: shared::STAMINA_MAX as f32,
                stamina_last_sent: shared::STAMINA_MAX,
                xp: 0,
                gold: 0,
                gold_last_sent: u64::MAX,
                fame: 0,
                fame_last_sent: u64::MAX,
                aura: 0,
                aura_last_sent: u64::MAX,
                party_id: None,
                party_invite_from: None,
                carrying: None,
                carried_by: None,
                current_map: "overworld".into(),
                proficiencies: [0; shared::PROF_COUNT],
                proficiencies_dirty: false,
                skill_points_earned: 0,
                skill_points_spent: 0,
                learned_skills: Vec::new(),
                skills_dirty: false,
                skill_cds: HashMap::new(),
                casting_until: 0.0,
                casting_started_at_s: 0.0,
                cast_movement_ticks: 0,
                pk_mode_on: false,
                faction: shared::Faction::default(),
                casting_skill_id: 0,
                casting_mp_paid: 0.0,
                casting_st_paid: 0.0,
                storm_caller_next: 0.0,
                inventory: vec![shared::InventorySlot::default(); shared::INVENTORY_SLOTS],
                inventory_dirty: false,
                stats_dirty: false,
                vault: vec![shared::InventorySlot::default(); shared::INVENTORY_SLOTS],
                vault_dirty: false,
                visual: shared::VisualConfig::for_class("warrior"),
                defending: false,
                last_press_primary_at: f32::NEG_INFINITY,
                last_press_secondary_at: f32::NEG_INFINITY,
                prev_buttons: 0,
                stagger_until: 0.0,
                parry_flash_pending: false,
                last_movement_at_s: 0.0,
                quickdraw_consumed: false,
                hunter_marks: HashMap::new(),
                riposte_until: 0.0,
                sword_dance_until: 0.0,
                sword_dance_step: 0,
                counter_stance_until: 0.0,
                leap_until: 0.0,
                leap_start_pos: Vec2::ZERO,
                leap_target: Vec2::ZERO,
                leap_damage: 0,
                leap_radius: 0.0,
                climb_timer: 0.0,
                climb_cooldown: 0.0,
                harpoon_target_eid: None,
                harpoon_until: 0.0,
                bloodthirst_until: 0.0,
                hunters_mark_until: 0.0,
                hunters_mark_charges: 0,
                hunters_mark_rank: 0,
                poise_current: 50.0, // base padrao; refresh via stats no login
                defending_poise_buffer: 0.0,
                last_combat_at_s: 0.0,
                poise_last_sent: 0,
                woodcutting_lvl: 1,
                mining_lvl:      1,
                gathering_lvl:   1,
                quests: Vec::new(),
                quests_dirty: false,
                faction_points: 0,
                faction_points_last_sent: u32::MAX,
                fishing_bobber: None,
                hooked_fish: None,
                tutorial_slot: None,
                dungeon_slot: None,
                pending_dungeon_raid: false,
            },
        );
    }

    pub fn on_disconnect(&mut self, id: SessionId) {
        if let Some(s) = self.sessions.remove(&id) {
            // Tutorial: libera a lane pra outro player poder usar.
            if let Some(idx) = s.tutorial_slot {
                if let Some(slot) = self.tutorial_slots.get_mut(idx) {
                    if slot.occupant == Some(id) { slot.occupant = None; }
                }
            }
            // Dungeon: limpa a run (gates + mobs) e libera a lane.
            if self.dungeon_mode { self.end_dungeon_run(id); }
            if let Some(e) = s.entity {
                // Se estava montado num barco, despawna o barco do mundo
                // (mas o estado ja foi capturado em take_character_for_disconnect
                // antes desse on_disconnect — re-spawn no proximo login no
                // mesmo lugar com mesmo kind/dir).
                if let Ok(m) = self.ecs.get::<&Mounted>(e).map(|m| (m.boat_entity, m.boat_eid)) {
                    let (boat_e, boat_eid) = m;
                    let _ = self.ecs.despawn(boat_e);
                    self.removed_this_tick.push(boat_eid);
                }
                self.free_entity_body(e);
                let _ = self.ecs.despawn(e);
            }
            // Libera carry se aplicavel
            if let Some(target_eid) = s.carrying {
                for ts in self.sessions.values_mut() {
                    if ts.entity_id == target_eid { ts.carried_by = None; break; }
                }
            }
            if let Some(carrier_eid) = s.carried_by {
                for cs in self.sessions.values_mut() {
                    if cs.entity_id == carrier_eid { cs.carrying = None; break; }
                }
            }
            // Se estava numa party, notifica remanescentes
            if let Some(pid) = s.party_id {
                let remaining = self.party_members(pid);
                if remaining.len() <= 1 {
                    for sess in self.sessions.values_mut() {
                        if sess.party_id == Some(pid) {
                            sess.party_id = None;
                            let _ = sess.handle.to_client.send(ServerMessage::PartyUpdate { members: vec![] });
                        }
                    }
                } else {
                    self.broadcast_party_update(pid);
                }
            }
            tracing::info!("{:?} disconnected", s.entity_id);
        }
    }

    /// Player concluiu o tutorial: marca `last_tutorial_completed` no DB e
    /// manda o client reconectar no mundo aberto. No-op fora do processo de
    /// tutorial (TUTORIAL_MODE). host/path vazios → client usa o default de
    /// produção (/game).
    fn handle_finish_tutorial(&mut self, sid: SessionId) {
        if !self.tutorial_mode { return; }
        let (to_client, name, pool) = {
            let Some(s) = self.sessions.get(&sid) else { return };
            if !s.logged_in { return; }
            let pool = self.auth_ctx.as_ref().map(|c| c.pool.clone());
            (s.handle.to_client.clone(), s.name.clone(), pool)
        };
        let _ = to_client.send(ServerMessage::TutorialComplete {
            world_host: String::new(),
            world_path: String::new(),
        });
        if let Some(pool) = pool {
            let name_for_db = name.clone();
            tokio::spawn(async move {
                if let Err(e) = sqlx::query(
                    "UPDATE characters SET last_tutorial_completed = NOW() WHERE name = $1"
                ).bind(&name_for_db).execute(&pool).await {
                    tracing::warn!("[tutorial] falha ao marcar last_tutorial_completed de '{}': {}",
                        name_for_db, e);
                }
            });
        }
        tracing::info!("[tutorial] '{}' concluiu o tutorial → TutorialComplete enviado", name);
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
                    xp_multiplier: crate::economy::xp_multiplier(),
                });
            }
            ClientMessage::SetTarget { target } => {
                let Some(session) = self.sessions.get_mut(&id) else { return };
                if !session.logged_in { return; }
                // Alvo tem que existir e nao pode ser o proprio player. O
                // resto (vivo, no alcance, PvP permitido) e' reavaliado a cada
                // tick no auto-ataque — alvo pode morrer ou fugir.
                session.target = match target {
                    Some(t) if t != session.entity_id => Some(t),
                    _ => None,
                };
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
            ClientMessage::Input { input: frame } => {
                if let Some(s) = self.sessions.get_mut(&id) {
                    // Mexeu no joystick? A rota morre. Comando manual sempre
                    // ganha do automatico — nada irrita mais que o boneco
                    // insistir em ir pra onde o jogador desistiu de ir.
                    if frame.move_dir.length_squared() > 0.01 {
                        s.rota.limpa();
                    }
                    s.pending_input = Some(frame);
                }
            }
            ClientMessage::MoverPara { x, z } => self.handle_mover_para(id, Vec2::new(x, z)),
            ClientMessage::Chat { text } => {
                // Comandos de slash
                let trimmed = text.trim();
                if let Some(rest) = trimmed.strip_prefix("/party ") {
                    let rest = rest.trim();
                    if let Some(name) = rest.strip_prefix("invite ") {
                        self.handle_party_invite(id, name.trim().to_string());
                    } else if rest == "accept" {
                        self.handle_party_accept(id);
                    } else if rest == "decline" {
                        if let Some(s) = self.sessions.get_mut(&id) {
                            s.party_invite_from = None;
                        }
                    } else if rest == "leave" {
                        self.handle_party_leave(id);
                    }
                    return;
                }
                if trimmed == "/despawn" {
                    self.admin_despawn_all_enemies();
                    self.send_chat_to(id, "[Sistema] Todos os mobs despawnados. Spawn zones reabastecendo...");
                    return;
                }
                if trimmed == "/who" {
                    let mult = crate::economy::xp_multiplier();
                    let mut list: Vec<(String, u32)> = self.sessions.values()
                        .filter(|s| s.logged_in && !s.name.is_empty())
                        .map(|s| (s.name.clone(), shared::level_of_xp_with_mult(s.xp, mult)))
                        .collect();
                    list.sort();
                    let txt = if list.is_empty() {
                        "[/who] ninguém online".to_string()
                    } else {
                        format!("[/who] {} online: {}", list.len(),
                            list.iter().map(|(n, l)| format!("{} (lv{})", n, l))
                                .collect::<Vec<_>>().join(", "))
                    };
                    self.send_chat_to(id, &txt);
                    return;
                }
                if trimmed == "/tutorial" {
                    // Replay do tutorial: só no mundo aberto. Manda o client
                    // reconectar no servidor de tutorial.
                    if !self.tutorial_mode {
                        if let Some(s) = self.sessions.get(&id) {
                            let _ = s.handle.to_client.send(ServerMessage::GoToTutorial {
                                host: String::new(), path: String::new(),
                            });
                        }
                    }
                    return;
                }
                let from = self
                    .sessions
                    .get(&id)
                    .and_then(|s| s.entity)
                    .and_then(|e| self.ecs.get::<&PlayerTag>(e).ok().map(|p| p.name.clone()))
                    .unwrap_or_else(|| "?".into());
                // Tutorial: chat fica restrito à própria lane (isolamento) —
                // como cada lane tem 1 player, na prática só ecoa pro autor.
                let sender_slot = self.sessions.get(&id).and_then(|s| s.tutorial_slot);
                for s in self.sessions.values() {
                    if self.tutorial_mode && s.tutorial_slot != sender_slot { continue; }
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
            ClientMessage::Interact { target_eid } => {
                self.handle_interact(id, target_eid);
            }
            ClientMessage::FishingCast { pos } => {
                self.handle_fishing_cast(id, pos);
            }
            ClientMessage::FishingReel { success } => {
                self.handle_fishing_reel(id, success);
            }
            ClientMessage::FishingCancel => {
                self.handle_fishing_cancel(id);
            }
            ClientMessage::FinishTutorial => self.handle_finish_tutorial(id),
            ClientMessage::SelectDungeonMode { raid } => {
                // Só faz sentido no processo de dungeon, antes do spawn.
                if self.dungeon_mode {
                    if let Some(s) = self.sessions.get_mut(&id) {
                        s.pending_dungeon_raid = raid;
                    }
                }
            }
            ClientMessage::AcceptQuest { quest_id } => {
                self.handle_accept_quest(id, quest_id);
                self.send_quest_givers(id);
            }
            ClientMessage::AbandonQuest { quest_id } => {
                self.handle_abandon_quest(id, quest_id);
                self.send_quest_givers(id);
            }
            ClientMessage::TurnInQuest { quest_id } => {
                self.handle_turn_in_quest(id, quest_id);
                self.send_quest_givers(id);
            }
            ClientMessage::RequestQuestOffer { source, giver } => {
                self.send_quest_offer(id, source, giver, String::new());
            }
            ClientMessage::FactionShopBuy { item_id } => {
                self.handle_faction_shop_buy(id, item_id);
            }
            ClientMessage::ShopBuy { slot_idx } => {
                self.handle_shop_buy(id, slot_idx as usize);
            }
            ClientMessage::ShopSell { inv_slot } => {
                self.handle_shop_sell(id, inv_slot as usize);
            }
            ClientMessage::ShopTrade { buying, selling } => {
                self.handle_shop_trade(id, buying, selling);
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
            ClientMessage::InventoryAutoArrange => {
                self.handle_inventory_auto_arrange(id);
            }
            ClientMessage::VaultAutoArrange => {
                self.handle_vault_auto_arrange(id);
            }
            ClientMessage::Craft { recipe_id } => {
                self.handle_craft(id, recipe_id);
            }
            ClientMessage::StandUp => {
                self.handle_stand_up(id);
            }
            ClientMessage::TeleportToVendor => {
                self.handle_teleport_to_vendor(id);
            }
            ClientMessage::PartyInvite { target_name } => {
                self.handle_party_invite(id, target_name);
            }
            ClientMessage::PartyAccept => {
                self.handle_party_accept(id);
            }
            ClientMessage::PartyDecline => {
                if let Some(s) = self.sessions.get_mut(&id) {
                    s.party_invite_from = None;
                }
            }
            ClientMessage::PartyLeave => {
                self.handle_party_leave(id);
            }
            ClientMessage::AllocStatPoint { stat } => {
                self.handle_alloc_stat_point(id, stat);
            }
            ClientMessage::ResetStats => {
                self.handle_reset_stats(id);
            }
            ClientMessage::ResetSkills => {
                self.handle_reset_skills(id);
            }
            ClientMessage::RefineItem { slot } => {
                self.handle_refine_item(id, slot);
            }
            ClientMessage::SocketGem { item_slot, gem_slot } => {
                self.handle_socket_gem(id, item_slot, gem_slot);
            }
            ClientMessage::DismountBoat | ClientMessage::LeaveBoat => {
                self.handle_dismount_boat(id);
            }
            ClientMessage::BoardBoat { boat_eid } => {
                self.handle_board_boat(id, boat_eid);
            }
            ClientMessage::GrabStation { station } => {
                self.handle_grab_station(id, station);
            }
            ClientMessage::ReleaseStation => {
                self.handle_release_station(id);
            }
            ClientMessage::SailAdjust { delta_position, delta_angle } => {
                self.handle_sail_adjust(id, delta_position, delta_angle);
            }
            ClientMessage::AnchorToggle => {
                self.handle_anchor_toggle(id);
            }
            ClientMessage::HelmAdjust { delta_angle } => {
                self.handle_helm_adjust(id, delta_angle);
            }
            ClientMessage::CannonAim { slot, angle } => {
                self.handle_cannon_aim(id, slot, angle);
            }
            ClientMessage::CannonFire { slot, power } => {
                self.handle_cannon_fire(id, slot, power);
            }
            ClientMessage::TogglePkMode { on } => {
                if let Some(s) = self.sessions.get_mut(&id) {
                    if s.logged_in { s.pk_mode_on = on; }
                }
            }
            ClientMessage::ResetPosition => {
                self.handle_reset_position(id);
            }
            ClientMessage::DropItem { slot } => {
                self.handle_drop_item(id, slot);
            }
            ClientMessage::RequestDisconnect => self.on_disconnect(id),
            ClientMessage::SkillLearn { skill_id }   => self.handle_skill_learn(id, skill_id),
            ClientMessage::SkillRankUp { skill_id }  => self.handle_skill_rank_up(id, skill_id),
            ClientMessage::SkillEquip { skill_id, slot } => self.handle_skill_equip(id, skill_id, slot),
            ClientMessage::SkillCast { skill_id, target_pos } => self.handle_skill_cast(id, skill_id, target_pos),
            ClientMessage::CreateCharacter { name, visual, starting_weapon, faction } => {
                self.handle_create_character(id, name, visual, starting_weapon, faction);
            }
            ClientMessage::SelectCharacter { name } => {
                self.handle_select_character(id, name);
            }
            ClientMessage::RespawnAtCity => {
                tracing::info!("[debug] RespawnAtCity recebido de sessao {:?}", id);
                self.handle_respawn_at_city(id);
            }
            ClientMessage::FarmHit { node_id } => {
                self.handle_farm_hit(id, node_id);
            }
            ClientMessage::UpdateVisual { visual } => {
                self.handle_update_visual(id, visual);
            }
            ClientMessage::AdminCommand { secret, target_char, action } => {
                self.handle_admin_command(id, secret, target_char, action);
            }
        }
    }

    /// Aplica admin command. Se `target_char` Some, busca a sessao pelo
    /// nome do char (precisa estar online); senao aplica no sender (sid).
    /// Drop silencioso se `MMORPG_ADMIN_SECRET` nao setada ou invalido.
    fn handle_admin_command(
        &mut self,
        sid: SessionId,
        secret: String,
        target_char: Option<String>,
        action: shared::protocol::AdminAction,
    ) {
        let expected = std::env::var("MMORPG_ADMIN_SECRET").unwrap_or_default();
        if expected.is_empty() || secret != expected {
            tracing::warn!("AdminCommand rejeitado: secret invalido (sid={:?})", sid);
            return;
        }
        // Resolve sid alvo
        let target_sid = match &target_char {
            Some(name) => {
                let found = self.sessions.iter()
                    .find(|(_, s)| s.logged_in && s.name == *name)
                    .map(|(k, _)| *k);
                match found {
                    Some(s) => s,
                    None => {
                        // OFFLINE: aplica direto no DB por nome (level/xp/gold/pontos).
                        // Próximo login do char recarrega do DB (CharReloadedForSelect).
                        self.admin_db_apply(name.clone(), action);
                        self.send_chat_to(sid, &format!(
                            "[ADMIN] '{}' offline → aplicado no DB (vale no próximo login).", name));
                        return;
                    }
                }
            }
            None => sid,
        };
        let Some(session) = self.sessions.get_mut(&target_sid) else { return; };
        let name = session.name.clone();
        let ecs_entity = session.entity;
        tracing::info!("AdminCommand from {}: {:?}", name, action);
        match action {
            shared::protocol::AdminAction::SetXp { xp } => {
                session.xp = xp;
                session.stats = effective_stats(
                    &session.equipment, &session.allocated_points,
                    &session.proficiencies, &session.learned_skills, session.xp);
                session.poise_current = session.stats.poise_max as f32;
            }
            shared::protocol::AdminAction::SetGold { gold } => {
                session.gold = gold.max(0) as u64;
            }
            shared::protocol::AdminAction::GiveItem { item_id, qty } => {
                add_to_inventory(&mut session.inventory, item_id, qty as u32, None);
                session.inventory_dirty = true;
            }
            shared::protocol::AdminAction::ClearInventory => {
                session.inventory.clear();
                session.inventory_dirty = true;
            }
            shared::protocol::AdminAction::HealFull => {
                session.stamina_current = session.stats.stamina_max as f32;
                session.mp_current = session.stats.mp_max as f32;
                session.poise_current = session.stats.poise_max as f32;
                if let Some(e) = ecs_entity {
                    if let Ok(mut hp) = self.ecs.get::<&mut Health>(e) {
                        hp.current = hp.max;
                    }
                }
            }
            shared::protocol::AdminAction::GrantSp { amount } => {
                session.skill_points_earned = session.skill_points_earned.saturating_add(amount);
            }
            shared::protocol::AdminAction::GrantStatPoints { amount } => {
                session.unspent_points = session.unspent_points.saturating_add(amount);
                session.stat_points_dirty = true;
            }
            shared::protocol::AdminAction::SetLevel { level } => {
                let lvl = level.clamp(1, shared::CHAR_LEVEL_CAP);
                session.xp = shared::xp_for_level_with_mult(lvl, crate::economy::xp_multiplier());
                let lvl_gained = lvl.saturating_sub(1);
                session.unspent_points = lvl_gained * shared::POINTS_PER_LEVEL;
                session.skill_points_earned = lvl_gained * shared::SP_PER_LEVEL;
                session.skill_points_spent = 0;
                session.allocated_points = [0; shared::STAT_COUNT];
                session.learned_skills.clear();
                session.last_level = lvl;
                session.stats = effective_stats(
                    &session.equipment, &session.allocated_points,
                    &session.proficiencies, &session.learned_skills, session.xp);
                session.poise_current = session.stats.poise_max as f32;
                session.stat_points_dirty = true;
                session.skills_dirty = true;
            }
        }
    }

    /// Aplica uma AdminAction direto no DB por NOME (char OFFLINE). Suporta
    /// level/xp/gold/pontos de atributo+skill; ações de runtime (item/heal/
    /// clear_inv) só valem online. O próximo login do char recarrega do DB.
    fn admin_db_apply(&self, name: String, action: shared::protocol::AdminAction) {
        use shared::protocol::AdminAction::*;
        let Some(pool) = self.auth_ctx.as_ref().map(|c| c.pool.clone()) else {
            tracing::warn!("AdminCommand offline '{}': sem pool DB", name);
            return;
        };
        let mult = crate::economy::xp_multiplier();
        tokio::spawn(async move {
            let res = match action {
                SetXp { xp } => sqlx::query("UPDATE characters SET xp=$1 WHERE name=$2")
                    .bind(xp as i64).bind(&name).execute(&pool).await,
                SetGold { gold } => sqlx::query("UPDATE characters SET gold=$1 WHERE name=$2")
                    .bind(gold.max(0)).bind(&name).execute(&pool).await,
                GrantSp { amount } => sqlx::query(
                    "UPDATE characters SET skill_points_earned=skill_points_earned+$1 WHERE name=$2")
                    .bind(amount as i32).bind(&name).execute(&pool).await,
                GrantStatPoints { amount } => sqlx::query(
                    "UPDATE characters SET unspent_points=unspent_points+$1 WHERE name=$2")
                    .bind(amount as i32).bind(&name).execute(&pool).await,
                SetLevel { level } => {
                    let lvl = level.clamp(1, shared::CHAR_LEVEL_CAP);
                    let xp = shared::xp_for_level_with_mult(lvl, mult);
                    let g = lvl.saturating_sub(1);
                    sqlx::query("UPDATE characters SET xp=$1, unspent_points=$2, \
                        skill_points_earned=$3, skill_points_spent=0, \
                        allocated_points='{0,0,0,0,0,0}' WHERE name=$4")
                        .bind(xp as i64)
                        .bind((g * shared::POINTS_PER_LEVEL) as i32)
                        .bind((g * shared::SP_PER_LEVEL) as i32)
                        .bind(&name).execute(&pool).await
                }
                other => {
                    tracing::warn!("AdminCommand offline {:?} não suportado p/ char offline", other);
                    return;
                }
            };
            match res {
                Ok(r) => tracing::info!("AdminCommand offline DB: '{}' rows={}", name, r.rows_affected()),
                Err(e) => tracing::error!("AdminCommand offline DB '{}' err: {e:?}", name),
            }
        });
    }

    /// Atualiza o visual do player (wardrobe in-game). Sobrescreve o
    /// VisualConfig no Session — o proximo snapshot ja replica o estado novo
    /// pra todos os clientes (a flag dirty é via comparacao no client; server
    /// sempre envia visual no snapshot).
    ///
    /// Persistencia: o save periodico (`flush_player_persistence`) escreve o
    /// `Session.visual` no DB como `visual_json`, entao reload do player ja
    /// vem com o wardrobe aplicado.
    fn handle_update_visual(&mut self, sid: SessionId, visual: shared::VisualConfig) {
        if let Some(s) = self.sessions.get_mut(&sid) {
            // Aceita tudo que o client enviou — sanitizacao basica de
            // strings (clamp comprimento). Validacao de codes especificos
            // (outfit em whitelist, cores em range) e deixada pro cliente
            // por enquanto — server confia mas trunca pra evitar abuso.
            let mut v = visual;
            const MAX_STR: usize = 16;
            if let Some(s) = v.skin_race.as_mut() { s.truncate(MAX_STR); }
            if let Some(s) = v.outfit.as_mut()    { s.truncate(MAX_STR); }
            if let Some(s) = v.hair.as_mut()      { s.truncate(MAX_STR); }
            if let Some(s) = v.hat.as_mut()       { s.truncate(MAX_STR); }
            s.visual = v;
            tracing::info!("UpdateVisual sid={:?} → {:?}", sid, s.visual);
        }
    }

    /// Player downed escolheu respawnar direto na cidade — pula o timer de
    /// stand-up. HP restaurado pra max, posicao = spawn_tile, downed limpa.
    fn handle_respawn_at_city(&mut self, sid: SessionId) {
        let (entity, hp_max, name, faction, dungeon_slot) = {
            let Some(session) = self.sessions.get_mut(&sid) else {
                tracing::warn!("RespawnAtCity ignorado: sessao {:?} nao existe", sid);
                return;
            };
            if !session.logged_in {
                tracing::warn!("RespawnAtCity ignorado: sessao {:?} nao logada (name={})", sid, session.name);
                return;
            }
            if !session.downed {
                tracing::warn!("RespawnAtCity ignorado: {} nao esta em downed (logged_in={}, downed=false)", session.name, session.logged_in);
                return;
            }
            let Some(entity) = session.entity else {
                tracing::warn!("RespawnAtCity ignorado: {} sem entity associada", session.name);
                return;
            };
            let hp_max = session.stats.hp_max;
            session.downed = false;
            session.downed_heal_timer = 0.0;
            session.downed_hp = 0;
            (entity, hp_max, session.name.clone(), session.faction, session.dungeon_slot)
        };
        // Tutorial: respawna na área do Matteo. Dungeon: respawna no INÍCIO
        // da dungeon (vestíbulo da própria lane) — o countdown da run continua
        // correndo. Mundo: cidade-sede da facção.
        let spawn = if self.tutorial_mode {
            Vec2::new(TUTORIAL_AREA.0, TUTORIAL_AREA.1)
        } else if self.dungeon_mode {
            let base = dungeon_slot
                .and_then(|i| self.dungeon_slots.get(i))
                .map(|s| s.base)
                .unwrap_or(Vec2::new(DUNGEON_ORIGIN.0 as f32, DUNGEON_ORIGIN.1 as f32));
            Vec2::new(base.x + DUNGEON_ENTRY.0, base.y + DUNGEON_ENTRY.1)
        } else {
            let t = self.map.faction_spawn_tile(faction);
            Vec2::new(t.0 as f32 + 0.5, t.1 as f32 + 0.5)
        };
        if let Ok(mut hp) = self.ecs.get::<&mut Health>(entity) {
            hp.current = hp_max;
        }
        if let Ok(mut pos) = self.ecs.get::<&mut Position>(entity) {
            pos.0 = spawn;
        }
        if let Ok(mut vel) = self.ecs.get::<&mut Velocity>(entity) {
            vel.0 = Vec2::ZERO;
        }
        let _ = self.ecs.remove_one::<Untargetable>(entity);
        self.save_pending = true;
        tracing::info!("{} respawnou na cidade ({}hp, pos={:?})", name, hp_max, spawn);
    }

    /// Cliente clicou num char na lista — valida que pertence a conta e
    /// faz spawn no mundo.
    fn handle_select_character(&mut self, sid: SessionId, name: String) {
        let (account_id, username, class) = match self.sessions.get(&sid) {
            Some(s) if s.account_id.is_some() => (
                s.account_id.unwrap(),
                s.pending_auth_username.clone().unwrap_or_else(|| name.clone()),
                s.pending_auth_class.clone().unwrap_or_else(|| "warrior".to_string()),
            ),
            _ => {
                tracing::warn!("SelectCharacter: sessao sem auth");
                return;
            }
        };
        let row = match self.characters.get(&name).cloned() {
            Some(r) => r,
            None => {
                // Char não está no cache local. Pode ter sido criado em OUTRO
                // processo (char novo criado no :9000, selecionado aqui no
                // tutorial :9001). Recarrega do DB async e re-tenta o spawn.
                if let Some(ctx) = self.auth_ctx.clone() {
                    let to_client = self.sessions.get(&sid).map(|s| s.handle.to_client.clone());
                    let name2 = name.clone();
                    let succ = crate::auth::AuthSuccess { account_id, username, class };
                    tracing::info!("SelectCharacter '{}': fora do cache — recarregando do DB", name);
                    tokio::spawn(async move {
                        match crate::persistence::load_all(&ctx.pool).await {
                            Ok(map) => {
                                if let Some(r) = map.get(&name2).cloned() {
                                    let _ = ctx.tx.send(IncomingMessage::CharReloadedForSelect(
                                        sid, Box::new(r), succ));
                                    return;
                                }
                            }
                            Err(e) => tracing::error!("SelectCharacter reload load_all err: {e:?}"),
                        }
                        if let Some(tc) = to_client {
                            let _ = tc.send(ServerMessage::Kick { reason: "char nao encontrado".into() });
                        }
                    });
                    return;
                }
                tracing::warn!("SelectCharacter '{}': char nao encontrado", name);
                let _ = self.sessions.get(&sid).map(|s| s.handle.to_client.send(
                    ServerMessage::Kick { reason: "char nao encontrado".into() }));
                return;
            }
        };
        if row.account_id != Some(account_id) {
            tracing::warn!("SelectCharacter '{}': char nao pertence a acc {}", name, account_id);
            let _ = self.sessions.get(&sid).map(|s| s.handle.to_client.send(
                ServerMessage::Kick { reason: "char nao autorizado".into() }));
            return;
        }
        // Gate de tutorial: no MUNDO, char que nunca concluiu o tutorial é
        // redirecionado pra lá (vale pra char novo E existente). Quem concluiu
        // (ou pulou) entra normal. O processo de tutorial não aplica o gate.
        if !self.tutorial_mode && !self.dungeon_mode && row.last_tutorial_completed.is_none()
            && std::env::var("SKIP_TUTORIAL_GATE").is_err() {
            tracing::info!("SelectCharacter '{}': tutorial nao concluido → GoToTutorial", row.name);
            let _ = self.sessions.get(&sid).map(|s| s.handle.to_client.send(
                ServerMessage::GoToTutorial { host: String::new(), path: String::new() }));
            return;
        }
        let success = crate::auth::AuthSuccess { account_id, username, class };
        self.spawn_for_char(sid, success, row);
    }

    /// Cria personagem inicial pra conta. Disparado depois que login enviou
    /// `NeedsCharacterCreation`. Valida nome, escreve row + inventory + equip
    /// no DB sincronamente, recarrega o cache e re-dispara o login flow.
    fn handle_create_character(
        &mut self,
        sid: SessionId,
        name: String,
        visual: shared::VisualConfig,
        starting_weapon: u16,
        faction: shared::Faction,
    ) {
        // Validacao basica
        let name = name.trim().to_string();
        if name.len() < 2 || name.len() > 24 {
            let _ = self.sessions.get(&sid).map(|s| s.handle.to_client.send(
                ServerMessage::CharacterCreationFailed { reason: "nome 2-24 chars".into() }));
            return;
        }
        if !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
            let _ = self.sessions.get(&sid).map(|s| s.handle.to_client.send(
                ServerMessage::CharacterCreationFailed { reason: "nome so letras/numeros/_".into() }));
            return;
        }
        // Arma inicial NÃO é mais escolhida na criação — o tutorial entrega a
        // arma T1. 0 = sem arma. (Mantém compat: se um client antigo mandar um
        // id de arma válido, ainda aceita.)
        const ALLOWED_WEAPONS: &[u16] = &[3, 6, 12, 13, 14, 15, 25, 26];
        if starting_weapon != 0 && !ALLOWED_WEAPONS.contains(&starting_weapon) {
            let _ = self.sessions.get(&sid).map(|s| s.handle.to_client.send(
                ServerMessage::CharacterCreationFailed { reason: "arma invalida".into() }));
            return;
        }
        // Pega account_id da sessao (setado em handle_auth_result).
        let Some(account_id) = self.sessions.get(&sid).and_then(|s| s.pending_char_creation_account_id)
        else {
            let _ = self.sessions.get(&sid).map(|s| s.handle.to_client.send(
                ServerMessage::CharacterCreationFailed { reason: "sessao invalida".into() }));
            return;
        };
        // Spawn = ilha-sede da facção (cai no spawn_tile() se o mapa ainda não
        // tem markers de spawn por facção — ver WorldMap::faction_spawn_tile).
        let spawn = {
            let t = self.map.faction_spawn_tile(faction);
            Vec2::new(t.0 as f32 + 0.5, t.1 as f32 + 0.5)
        };
        // Snapshot do que precisamos da sessao antes de spawnar a task async.
        let (auth_ctx, to_client, username, class) = {
            let Some(s) = self.sessions.get(&sid) else { return };
            let Some(ctx) = self.auth_ctx.clone() else {
                let _ = s.handle.to_client.send(ServerMessage::CharacterCreationFailed {
                    reason: "auth nao disponivel".into() });
                return;
            };
            (
                ctx,
                s.handle.to_client.clone(),
                s.pending_auth_username.clone().unwrap_or_else(|| name.clone()),
                s.pending_auth_class.clone().unwrap_or_else(|| "warrior".to_string()),
            )
        };
        let visual_for_db = visual.clone();
        let name_for_db = name.clone();
        // DB write async + recarga do cache + re-dispatch via AuthResult.
        // Sucesso: gera AuthResult sintetico que faz on_auth_result rodar de
        // novo (agora encontrando o char). Falha: manda CharacterCreationFailed.
        tokio::spawn(async move {
            match crate::persistence::create_character(
                &auth_ctx.pool, account_id, &name_for_db, &visual_for_db, starting_weapon, spawn, faction
            ).await {
                Ok(true) => {
                    tracing::info!("CreateCharacter: '{}' criado (acc {}) weapon={}",
                        name_for_db, account_id, starting_weapon);
                    // Reload todos os chars pra pegar o row novo + pertencer
                    // ao cache. Pega o specific row pra entregar inline.
                    match crate::persistence::load_all(&auth_ctx.pool).await {
                        Ok(map) => {
                            if let Some(row) = map.get(&name_for_db).cloned() {
                                let success = crate::auth::AuthSuccess {
                                    account_id, username, class
                                };
                                let _ = auth_ctx.tx.send(
                                    IncomingMessage::CharCreated(sid, Box::new(row), success)
                                );
                            } else {
                                tracing::error!("CreateCharacter: row '{}' some apos load_all",
                                    name_for_db);
                                let _ = to_client.send(ServerMessage::CharacterCreationFailed {
                                    reason: "erro interno".into() });
                            }
                        }
                        Err(e) => {
                            tracing::error!("CreateCharacter load_all err: {e:?}");
                            let _ = to_client.send(ServerMessage::CharacterCreationFailed {
                                reason: "erro interno".into() });
                        }
                    }
                }
                Ok(false) => {
                    let _ = to_client.send(ServerMessage::CharacterCreationFailed {
                        reason: "nome ja em uso".into() });
                }
                Err(e) => {
                    tracing::error!("CreateCharacter db err: {e:?}");
                    let _ = to_client.send(ServerMessage::CharacterCreationFailed {
                        reason: "erro interno".into() });
                }
            }
        });
    }

    /// Recebido apos CreateCharacter ser completed pelo task async. Insere
    /// a row no cache de chars e re-envia CharacterList (cliente vai mostrar
    /// novo char na lista; usuario clica pra selecionar).
    pub fn on_char_created(
        &mut self,
        sid: SessionId,
        row: crate::persistence::CharacterRow,
        success: crate::auth::AuthSuccess,
    ) {
        self.characters.insert(row.name.clone(), row);
        // Sessao ja foi auth'd no Login original — apenas reenvia a lista
        // atualizada com o novo char incluido.
        self.send_character_list(sid, success.account_id);
    }

    /// Char recarregado do DB pra um SelectCharacter que deu cache-miss (criado
    /// em outro processo). Insere no cache, valida o dono e spawna direto.
    pub fn on_char_reloaded_for_select(
        &mut self,
        sid: SessionId,
        row: crate::persistence::CharacterRow,
        success: crate::auth::AuthSuccess,
    ) {
        self.characters.insert(row.name.clone(), row.clone());
        if row.account_id != Some(success.account_id) {
            tracing::warn!("SelectCharacter(reload) '{}': char nao pertence a acc {}",
                row.name, success.account_id);
            let _ = self.sessions.get(&sid).map(|s| s.handle.to_client.send(
                ServerMessage::Kick { reason: "char nao autorizado".into() }));
            return;
        }
        if !self.tutorial_mode && !self.dungeon_mode && row.last_tutorial_completed.is_none()
            && std::env::var("SKIP_TUTORIAL_GATE").is_err() {
            let _ = self.sessions.get(&sid).map(|s| s.handle.to_client.send(
                ServerMessage::GoToTutorial { host: String::new(), path: String::new() }));
            return;
        }
        self.spawn_for_char(sid, success, row);
    }

    /// Cast de skill ativa. Valida tudo, drena cost, dispara efeito.
    /// Por enquanto suporta `target_type=projectile` (Fireball, Frost Bolt etc.)
    /// e `self` (heal). Outros tipos serão adicionados na Phase 2.x.
    fn handle_skill_cast(&mut self, sid: SessionId, skill_id: u32, target_pos: Vec2) {
        // 1. Lookup
        let Some(session) = self.sessions.get_mut(&sid) else { return };
        if !session.logged_in { return; }
        let Some(def) = crate::skills::skill_of(skill_id) else { return };
        if def.is_passive { return; } // passivas não castam

        // 1b. Player montado em barco: skills bloqueadas. Cliente deveria ja
        // ter bloqueado o trigger local, mas o server e' a autoridade.
        if let Some(pe) = session.entity {
            if self.ecs.get::<&Mounted>(pe).is_ok() {
                tracing::debug!("skill_cast: bloqueado — player montado em barco");
                return;
            }
        }

        // 2. Skill aprendida + equipada em algum slot?
        let learned = session.learned_skills.iter().find(|s| s.skill_id == skill_id).copied();
        let Some(ls) = learned else { return };
        if ls.equipped_slot.is_none() { return; }
        let rank = ls.rank;

        // 3. Cooldown
        let now = self.sim_time_s;
        // Spear Throw (1021): segunda casta dentro da janela harpoon (5s) eh
        // recast — bypass CD pra permitir o pull mesmo se cooldown nao
        // expirou. CD do skill (14s) so' afeta primeira casta nova.
        let is_spear_recast = skill_id == 1021
            && session.harpoon_until > now
            && session.harpoon_target_eid.is_some();
        if let Some(&ready_at) = session.skill_cds.get(&skill_id) {
            if now < ready_at && !is_spear_recast { return; }
        }
        // 3b. Já castando outra skill? Rejeita (player travado).
        if session.casting_until > now { return; }

        // 4. Weapon affinity — TODAS as armas podem castar QUALQUER skill.
        // Quando a arma equipada bate com `def.prof` (arma recomendada),
        // os bonus `affinity_*` entram em damage/cooldown/crit/cost.
        let weapon_id = session.equipment.weapon.unwrap_or(0);
        let weapon_prof = shared::Proficiency::from_item(weapon_id).as_db_str();
        let is_affinity = weapon_prof == def.prof;

        // 5. Cost (MP / stamina). Aplica per_rank_cost_pct: cost final = base × (1 - per_rank × (rank-1)).
        let mut rank_factor = 1.0 - def.per_rank_cost_pct * (rank.saturating_sub(1) as f32);
        // Mana Conduit (1050) — Staff T1 P: -1%/rank mp/stam cost. Aplicado
        // multiplicativamente. Cap em -50% (rank 10 = -10% sozinho; com
        // per_rank_cost_pct da skill some adicional).
        if let Some(mc) = session.learned_skills.iter().find(|s| s.skill_id == 1050) {
            let mc_factor = (1.0 - 0.01 * mc.rank as f32).max(0.5);
            rank_factor *= mc_factor;
        }
        // Affinity cost reduction (multiplicativa, antes do clamp).
        if is_affinity && def.affinity_cost_red_pct > 0.0 {
            rank_factor *= 1.0 - def.affinity_cost_red_pct / 100.0;
        }
        let rank_factor = rank_factor.max(0.1);
        let mp_cost = ((def.cost_mp as f32) * rank_factor).round() as i32;
        let st_cost = ((def.cost_stamina as f32) * rank_factor).round() as f32;
        if (session.mp_current as i32) < mp_cost { return; }
        if session.stamina_current < st_cost { return; }

        // 6. Posição do player + direção pro target
        let Some(e) = session.entity else { return };
        let pos = match self.ecs.get::<&Position>(e) { Ok(p) => p.0, Err(_) => return };
        let to_target = target_pos - pos;
        let dir = if to_target.length_squared() > 0.001 {
            to_target.normalize()
        } else {
            Vec2::new(1.0, 0.0)
        };

        // 7. Damage scaling: base + atk*scal_atk + wis*scal_wis + dex*scal_dex,
        // depois aplica (1 + per_rank_dmg × rank). No max rank, aplica também
        // bonus de damage_pct definido na skill. Afinidade soma +X% no final.
        let stats = session.stats;
        let base_dmg = def.base_damage as f32
            + stats.attack_damage as f32 * def.scaling_atk
            + stats.wis as f32 * def.scaling_wis
            + stats.dex as f32 * def.scaling_dex;
        let mut scaled = base_dmg * (1.0 + def.per_rank_dmg_pct * (rank.saturating_sub(1) as f32));
        let is_max_rank = rank >= shared::MAX_SKILL_RANK;
        if is_max_rank && def.max_rank_damage_pct > 0.0 {
            scaled *= 1.0 + def.max_rank_damage_pct / 100.0;
        }
        if is_affinity && def.affinity_damage_pct > 0.0 {
            scaled *= 1.0 + def.affinity_damage_pct / 100.0;
        }
        // Affinity crit roll: chance extra de crit quando arma certa.
        // Aplica só em skills com base_damage > 0 (utility skills ignoram).
        let mut is_crit = false;
        if is_affinity && def.affinity_crit_pct > 0.0 && def.base_damage > 0 {
            let extra = def.affinity_crit_pct / 100.0;
            if fastrand::f32() < extra {
                is_crit = true;
                scaled *= shared::CRIT_DAMAGE_MULT;
            }
        }
        let damage = scaled.round() as i32;

        // 8. Drena cost + set cd + casting state se cast_time > 0.
        session.mp_current -= mp_cost as f32;
        if session.mp_current < 0.0 { session.mp_current = 0.0; }
        session.stamina_current = (session.stamina_current - st_cost).max(0.0);
        let mut cd_factor = (1.0 - def.per_rank_cd_pct * (rank.saturating_sub(1) as f32)).max(0.1);
        if is_max_rank && def.max_rank_cooldown_red_pct > 0.0 {
            cd_factor *= 1.0 - def.max_rank_cooldown_red_pct / 100.0;
        }
        if is_affinity && def.affinity_cooldown_red_pct > 0.0 {
            cd_factor *= 1.0 - def.affinity_cooldown_red_pct / 100.0;
        }
        let cd_final = def.cooldown_s * cd_factor;
        session.skill_cds.insert(skill_id, now + cd_final);
        if def.cast_time_s > 0.05 {
            session.casting_until = now + def.cast_time_s;
            session.casting_started_at_s = now;
            session.casting_skill_id = skill_id;
            // Salva o que foi pago pra refund no cancel-por-movimento.
            session.casting_mp_paid = mp_cost as f32;
            session.casting_st_paid = st_cost;
            // Para movimento e qualquer estado ativo durante o cast
            session.defending = false;
        }

        // 9. Dispatch por target_type
        let owner_eid = session.entity_id;
        let owner_entity = session.entity;
        // Trigger anim de "lancamento" pra skills com componente fisico
        // (projectile/line/cone). Self toca via predicao no input handler;
        // server replica pra outros clients via attack_anim_pending.
        // AoE/self/heal nao usam anim de saque (cast wind-up tem visual proprio
        // tipo Inferno crescendo o circulo, nao precisa de Thrust).
        let casts_attack_anim = matches!(
            def.target_type.as_str(),
            "projectile" | "line" | "cone"
        );
        if casts_attack_anim {
            if let Some(ent) = owner_entity {
                if let Ok(mut tag) = self.ecs.get::<&mut PlayerTag>(ent) {
                    // Shield Bash (1003): anim dedicada de ShieldBash. Outras
                    // skills cone melee usam THRUST/SHOOT default.
                    let anim = if skill_id == 1003 {
                        shared::components::attack_anim::SHIELD_BASH
                    } else {
                        match shared::Proficiency::from_item(weapon_id) {
                            shared::Proficiency::Bow => shared::components::attack_anim::SHOOT,
                            _                        => shared::components::attack_anim::THRUST,
                        }
                    };
                    tag.attack_anim_pending = Some(anim);
                }
            }
        }
        // Earthshatter (1015) é aoe_circle (não cai no `casts_attack_anim`)
        // mas a gente quer animacao de jump (DASH) parado durante o wind-up
        // pra que outros clients vejam o caster "saltando" antes do impacto.
        if skill_id == 1015 {
            if let Some(ent) = owner_entity {
                if let Ok(mut tag) = self.ecs.get::<&mut PlayerTag>(ent) {
                    tag.attack_anim_pending = Some(shared::components::attack_anim::DASH);
                }
            }
        }
        // Caltrops (1035): trap real — spawna 10 DelayedAoe ticks com 1s
        // de gap, cada um com damage + stun_dur_s=1.5s nos enemies em radius.
        // Skip do dispatch generico aoe_circle (que daria dano instantaneo
        // unico). Nao usa `return` aqui pra que o broadcast SkillCastFx
        // ainda fire no fim da funcao (cliente precisa pra desenhar visuais).
        let mut skip_dispatch = false;
        if skill_id == 1035 {
            let radius = def.radius_tiles.max(1.0);
            let trap_pos = target_pos;
            const TRAP_DURATION_S: f32 = 10.0;
            const TICK_INTERVAL_S: f32 = 1.0;
            let ticks = (TRAP_DURATION_S / TICK_INTERVAL_S) as i32;
            let now_tick = self.tick;
            let ticks_per_sec = 30.0_f32;
            for i in 0..ticks {
                let release_tick = now_tick.wrapping_add(((i as f32) * TICK_INTERVAL_S * ticks_per_sec) as u32);
                self.pending_delayed_aoe.push(DelayedAoe {
                    target_pos: trap_pos,
                    radius,
                    damage,
                    owner_eid,
                    release_tick,
                    poison_dur_s: 0.0,
                    knockback: 0.0,
                    follow_owner: false,
                    stun_dur_s: 1.5,
                });
            }
            tracing::info!("Caltrops cast: {} ticks × {}s @ ({:.1},{:.1}) r{} dmg={}",
                ticks, TICK_INTERVAL_S, trap_pos.x, trap_pos.y, radius, damage);
            skip_dispatch = true;
        }
        if !skip_dispatch {
        match def.target_type.as_str() {
            "projectile" => {
                let spawn_pos = pos + Vec2::new(0.0, shared::PROJ_SPAWN_OFFSET_Y)
                    + dir * shared::FIREBALL_FORWARD_OFFSET;
                // kind: 0 = arrow (bow), 1 = fireball legacy (default),
                // 2 = lightning, 3 = big fireball (skill), 4 = frost bolt,
                // 6 = power shot, 7 = spear (harpoon), 8 = wave slash,
                // 9 = throwing axe.
                let vfx = def.vfx_id.as_deref().unwrap_or("");
                let proj_kind: u8 = if skill_id == 1002 { 8 }    // Wave Slash
                    else if skill_id == 1010 { 9 }                // Throwing Axe
                    else { match shared::Proficiency::from_item(weapon_id) {
                        shared::Proficiency::Bow => {
                            // Power Shot (1033): proj_kind=6 — flecha 2x scale.
                            if skill_id == 1033 { 6 } else { 0 }
                        }
                        _ => {
                            if vfx.contains("lightning") { 2 }
                            else if vfx.contains("fireball") { 3 }
                            else if vfx.contains("frost") { 4 }
                            else { 1 }
                        }
                    }};
                // Multishot (1037): 5 flechas em leque ±30° (15° entre cada).
                // Outras skills projectile: 1 projetil unico.
                let dirs: Vec<Vec2> = if skill_id == 1037 {
                    [-30.0_f32, -15.0, 0.0, 15.0, 30.0].iter().map(|&deg| {
                        let rad = deg.to_radians();
                        let (s, c) = (rad.sin(), rad.cos());
                        Vec2::new(dir.x * c - dir.y * s, dir.x * s + dir.y * c)
                    }).collect()
                } else {
                    vec![dir]
                };
                for d in &dirs {
                    self.pending_shots.push(PendingShot {
                        pos: spawn_pos,
                        dir: *d,
                        damage,
                        is_crit,
                        kind: proj_kind,
                        owner_id: owner_eid,
                        from_player: true,
                        release_tick: self.tick.wrapping_add(1),
                    });
                }
                tracing::info!(
                    "skill cast: {} (skill {}, r{}) dmg={} kind={} arrows={} dir=({:.2},{:.2})",
                    def.name, skill_id, rank, damage, proj_kind, dirs.len(), dir.x, dir.y
                );
            }
            "self" => {
                // Taunt (1006): forca enemies em radius a mirar o caster
                // pelos proximos 4s. Override de AI target.
                if skill_id == 1006 {
                    let radius = def.radius_tiles.max(1.0);
                    let now_s = self.sim_time_s;
                    let dur = 4.0_f32;
                    let enemies = self.find_enemies_in_radius(pos, radius);
                    let target_set: std::collections::HashSet<EntityId> =
                        enemies.iter().copied().collect();
                    for (_, (net, tag)) in self.ecs.query_mut::<(&NetId, &mut EnemyTag)>() {
                        if target_set.contains(&net.0) {
                            tag.forced_aggro_until = now_s + dur;
                            tag.forced_aggro_target = Some(owner_eid);
                        }
                    }
                    tracing::info!("Taunt cast: {} enemies aggro'd r{:.1}", target_set.len(), radius);
                }
                // Master's Counter (1007): 2s auto-parry stance no caster.
                else if skill_id == 1007 {
                    if let Some(s) = self.sessions.get_mut(&sid) {
                        s.counter_stance_until = self.sim_time_s + 2.0;
                    }
                    tracing::info!("Master's Counter cast (2s stance)");
                }
                // Bloodthirst (1011): buff 4s com +20% atk speed + 10% lifesteal.
                else if skill_id == 1011 {
                    if let Some(s) = self.sessions.get_mut(&sid) {
                        s.bloodthirst_until = self.sim_time_s + 4.0;
                    }
                    // Broadcast BuffApplied{kind=1 dmg_buff} pro cliente.
                    let bmsg = ServerMessage::BuffApplied {
                        target_eid: owner_eid,
                        kind: 1,
                    };
                    for s in self.sessions.values() {
                        if s.logged_in {
                            let _ = s.handle.to_client.send(bmsg.clone());
                        }
                    }
                    tracing::info!("Bloodthirst cast: 4s atk speed + lifesteal");
                }
                // Hunter's Mark (1040): buff 8s + 3 charges com +50% crit + dmg.
                else if skill_id == 1040 {
                    if let Some(s) = self.sessions.get_mut(&sid) {
                        s.hunters_mark_until = self.sim_time_s + 8.0;
                        s.hunters_mark_charges = 3;
                        s.hunters_mark_rank = rank;
                    }
                    let bmsg = ServerMessage::BuffApplied {
                        target_eid: owner_eid,
                        kind: 1, // dmg_buff visual
                    };
                    for s in self.sessions.values() {
                        if s.logged_in {
                            let _ = s.handle.to_client.send(bmsg.clone());
                        }
                    }
                    tracing::info!("Hunter's Mark cast: 8s, 3 charges, rank {}", rank);
                }
                else {
                    // Heal padrão: aplica base_heal + scaling × wis.
                    let base_heal = def.base_heal as f32 + stats.wis as f32 * def.scaling_wis;
                    let mut heal_scaled = base_heal * (1.0 + def.per_rank_dmg_pct * (rank.saturating_sub(1) as f32));
                    if rank >= shared::MAX_SKILL_RANK && def.max_rank_heal_pct > 0.0 {
                        heal_scaled *= 1.0 + def.max_rank_heal_pct / 100.0;
                    }
                    // Affinity damage_pct também atua como heal bonus pra
                    // skills de cura (não duplicamos campo no SkillDef).
                    if is_affinity && def.affinity_damage_pct > 0.0 {
                        heal_scaled *= 1.0 + def.affinity_damage_pct / 100.0;
                    }
                    let heal = heal_scaled.round() as i32;
                    if heal > 0 {
                        if let Ok(mut hp) = self.ecs.get::<&mut Health>(e) {
                            hp.current = (hp.current + heal).min(hp.max);
                        }
                    }
                    tracing::info!("skill cast: {} self-heal +{}", def.name, heal);
                }
            }
            "line" => {
                // Resurrection (1055): pega ALIADO downed mais proximo em range
                // e ressuscita com HP%. Custo: 80 mp, cd: 120s. Range: 5 tiles.
                if skill_id == 1055 {
                    let range = def.range_tiles.max(1.0);
                    let r2 = range * range;
                    // Acha o player downed mais proximo (excluindo o caster).
                    let mut best: Option<(SessionId, f32)> = None;
                    for (other_sid, other) in self.sessions.iter() {
                        if *other_sid == sid { continue; }
                        if !other.logged_in || !other.downed { continue; }
                        let Some(oent) = other.entity else { continue };
                        let opos = match self.ecs.get::<&Position>(oent) {
                            Ok(p) => p.0, Err(_) => continue,
                        };
                        let d2 = opos.distance_squared(pos);
                        if d2 > r2 { continue; }
                        if best.map(|(_, bd)| d2 < bd).unwrap_or(true) {
                            best = Some((*other_sid, d2));
                        }
                    }
                    if let Some((target_sid, _)) = best {
                        let revive_pct = 0.5_f32; // 50% HP no resurrect (vs 30% do StandUp)
                        let target_session = self.sessions.get_mut(&target_sid).unwrap();
                        let revive_hp = ((target_session.stats.hp_max as f32) * revive_pct)
                            .round().max(1.0) as i32;
                        target_session.downed = false;
                        target_session.downed_heal_timer = 0.0;
                        target_session.downed_hp = 0;
                        let target_entity = target_session.entity;
                        let target_name = target_session.name.clone();
                        if let Some(te) = target_entity {
                            if let Ok(mut hp) = self.ecs.get::<&mut Health>(te) {
                                hp.current = revive_hp;
                            }
                            let _ = self.ecs.remove_one::<Untargetable>(te);
                        }
                        tracing::info!("skill cast: Resurrection — {} ressuscitado (+{}hp)",
                            target_name, revive_hp);
                    } else {
                        tracing::info!("skill cast: Resurrection — sem aliado downed em range");
                    }
                    return; // skip damage logic abaixo
                }
                // Linha (default): pega 1º enemy hostile no caminho até range_tiles.
                let range = def.range_tiles.max(1.0);
                let nearest = self.find_nearest_enemy_in_line(pos, dir, range, owner_eid);
                if let Some((target_net, target_pos2, _dist)) = nearest {
                    let hd = (-dir).try_normalize().unwrap_or(Vec2::new(-1.0, 0.0));
                    self.pending_skill_hits.push(PendingSkillHit {
                        target_net, damage, attacker_net: owner_eid,
                        hurt_dir: hd, is_crit, from_player: true,
                        knockback: def.knockback,
                    });
                    // Chain Lightning (1054): bounce até 4 alvos extras com falloff 25%.
                    if skill_id == 1054 {
                        let pts = self.chain_lightning_bounces(target_pos2, target_net, owner_eid, damage, 4);
                        self.last_chain_pts = pts;
                    }
                    // Lightning Bolt (1051): a partir do rank 5 ganha bounce
                    // pro proximo enemy. r5=+1, r6=+2 ... r10=+6 alvos extras.
                    if skill_id == 1051 && rank >= 5 {
                        let bonus = rank.saturating_sub(4) as u32;
                        let pts = self.chain_lightning_bounces(target_pos2, target_net, owner_eid, damage, bonus);
                        self.last_chain_pts = pts;
                    }
                    // Soul Drain (1042): cura caster por 75% do dano causado.
                    // Vampiric — converte HP do alvo em vida do caster.
                    if skill_id == 1042 {
                        let heal_amt = ((damage as f32) * 0.75).round() as i32;
                        if heal_amt > 0 {
                            self.pending_heals.push(PendingHeal {
                                target_net: owner_eid,
                                amount: heal_amt,
                            });
                        }
                    }
                    tracing::info!("skill cast: {} (line, r{}) dmg={}", def.name, rank, damage);
                }
            }
            "aoe_circle" => {
                let radius = def.radius_tiles.max(0.5);
                let is_heal = def.base_heal > 0 || (def.scaling_wis > 0.0 && def.base_damage == 0);
                // Leap Strike (1001): seta estado de leap (interpola posicao
                // do caster por LEAP_DURATION antes de teleportar). Damage +
                // stun aplicados no FIM do leap (no tick step). Anim de jump
                // replicada via attack_anim_pending=DASH.
                // ── Spear Throw (1021) — Harpoon 2-cast hook ──────────────
                // Cast 1: throw projectile (kind=7). Ao acertar, server seta
                // session.harpoon_target_eid = target.
                // Cast 2 (recast dentro de 5s): leap ate o target cravado.
                if skill_id == 1021 {
                    // Check se eh recast (harpoon ativo + alvo ainda vivo).
                    let harpoon_target = self.sessions.get(&sid)
                        .filter(|s| s.harpoon_until > self.sim_time_s)
                        .and_then(|s| s.harpoon_target_eid);
                    let target_world_pos = harpoon_target.and_then(|teid| {
                        // Find target entity pos pelo EntityId (NetId)
                        let mut found: Option<Vec2> = None;
                        for (_, (net, p)) in self.ecs.query_mut::<(&NetId, &Position)>() {
                            if net.0 == teid { found = Some(p.0); break; }
                        }
                        found
                    });
                    if let Some(land_pos) = target_world_pos {
                        // RECAST: leap ate o alvo. Duracao DEVE bater com
                        // LEAP_DURATION=0.5 do step (linha ~4908) — senao o
                        // step calcula leap_start errado e char comeca o
                        // leap ja adiantado.
                        if let Some(s) = self.sessions.get_mut(&sid) {
                            s.harpoon_target_eid = None;
                            s.harpoon_until = 0.0;
                            s.leap_until = self.sim_time_s + 0.5;
                            s.leap_start_pos = pos;
                            s.leap_target = land_pos;
                            s.leap_damage = 0; // dano ja foi no projetil hit
                            s.leap_radius = 0.0;
                        }
                        if let Ok(mut tag) = self.ecs.get::<&mut PlayerTag>(e) {
                            tag.attack_anim_pending = Some(shared::components::attack_anim::DASH);
                        }
                        tracing::info!("Spear Throw RECAST: hooking to target eid={:?} at {:?}",
                            harpoon_target, land_pos);
                        return;
                    }
                    // FIRST CAST: spawn projetil de spear (kind=7).
                    let spawn_pos = pos + Vec2::new(0.0, shared::PROJ_SPAWN_OFFSET_Y)
                        + dir * shared::FIREBALL_FORWARD_OFFSET;
                    self.pending_shots.push(PendingShot {
                        pos: spawn_pos,
                        dir,
                        damage,
                        is_crit,
                        kind: 7, // SPEAR harpoon
                        owner_id: owner_eid,
                        from_player: true,
                        release_tick: self.tick.wrapping_add(1),
                    });
                    tracing::info!("Spear Throw THROW: dmg={} dir=({:.2},{:.2})",
                        damage, dir.x, dir.y);
                    return;
                }
                // Spear movement skills (1017 Lunge, 1022 Charge, 1023
                // Dragon Tail) usam mesmo leap mechanism do Leap Strike —
                // player se move ate o target_pos com damage no landing.
                // Diferenca: range/radius/stun por skill.
                if skill_id == 1001 || skill_id == 1017 || skill_id == 1022 || skill_id == 1023 {
                    // Mesma duracao do leap step (4736) — ambos compartilham
                    // o mecanismo de interpolacao.
                    let leap_duration: f32 = 0.5;
                    // Clampa target_pos ao range tiles do caster.
                    let to = target_pos - pos;
                    let dist = to.length();
                    let mut landing = if dist > def.range_tiles && dist > 0.01 {
                        pos + to.normalize() * def.range_tiles
                    } else {
                        target_pos
                    };
                    // Raycast de pos -> landing pra evitar atravessar walls/agua.
                    // Sample em steps de 0.25 tiles. Se algum step bater em
                    // tile nao-walkable, encurta o landing pro ultimo ponto
                    // walkable (com pequeno recuo pra nao ficar grudado).
                    let total = (landing - pos).length();
                    if total > 0.01 {
                        let dir_n = (landing - pos) / total;
                        let mut last_ok = pos;
                        let step_size = 0.25_f32;
                        let steps = (total / step_size).ceil() as i32;
                        for i in 1..=steps {
                            let t = (i as f32 * step_size).min(total);
                            let p = pos + dir_n * t;
                            let tx = p.x.floor() as i32;
                            let ty = p.y.floor() as i32;
                            if !self.map.is_walkable(tx, ty) {
                                // Recua um pouco pra nao colar na parede.
                                let pulled_back = t - 0.4;
                                if pulled_back > 0.01 {
                                    landing = pos + dir_n * pulled_back;
                                } else {
                                    landing = pos;
                                }
                                tracing::info!(
                                    "Leap Strike: parede no caminho — landing encurtado de {:.1} pra {:.1} tiles",
                                    total, (landing - pos).length()
                                );
                                break;
                            }
                            last_ok = p;
                        }
                        let _ = last_ok;
                    }
                    // Set leap state — atualiza Position progressivamente no step.
                    if let Some(s) = self.sessions.get_mut(&sid) {
                        s.leap_until = self.sim_time_s + leap_duration;
                        s.leap_start_pos = pos;
                        s.leap_target = landing;
                        s.leap_damage = damage;
                        s.leap_radius = radius;
                    }
                    // Anim por skill: Leap Strike (1001) = DASH (jump), spear
                    // skills (1021, 1017, 1022, 1023) = THRUST (estocada).
                    if let Ok(mut tag) = self.ecs.get::<&mut PlayerTag>(e) {
                        let anim = match skill_id {
                            1021 | 1017 | 1022 | 1023 => shared::components::attack_anim::THRUST,
                            _                         => shared::components::attack_anim::DASH,
                        };
                        tag.attack_anim_pending = Some(anim);
                    }
                    let skill_name = match skill_id {
                        1001 => "Leap Strike",
                        1017 => "Lunge",
                        1021 => "Spear Throw",
                        1022 => "Charge",
                        1023 => "Dragon Tail",
                        _    => "Unknown",
                    };
                    tracing::info!("{} start: jumping {:.1} tiles in {:.2}s, will hit r{:.1} dmg={}",
                        skill_name, dist.min(def.range_tiles), leap_duration, radius, damage);
                    // SkillCastFx (com explosao VFX) e broadcastado apenas no
                    // LANDING (em E.1 step), nao no cast — cliente sincroniza
                    // com a chegada do char.
                    return;
                }
                // Sustained rain — pulses de dano ao longo de N segundos.
                //   Frost Nova (1046): cast_time_s>0 → wind-up parado, depois rain.
                //   Meteor (1045):     cast_time_s=3s → player imovel durante o rain.
                //   Rain of Arrows (1039): cast 0.5s, depois 8 flechas ao longo de 3s.
                //   Smoke Bomb (1038): nuvem 5s, 5 pulses de dano + envenenado.
                let is_rain = matches!(skill_id, 1045 | 1046 | 1039 | 1038 | 1013 | 1005);
                if is_rain {
                    let (pulses, total_s) = match skill_id {
                        1039 => (8u32, 3.0f32),     // Rain of Arrows
                        1038 => (5u32, 5.0f32),     // Smoke Bomb (1 pulse/s × 5s)
                        1013 => (10u32, 4.0f32),    // Whirlwind — spin 10p × 4s
                        1005 => (6u32, 1.8f32),     // Sword Dance — spin 6 hits × 1.8s
                        _    => (6u32, 3.0f32),     // Frost Nova / Meteor
                    };
                    let per_pulse = (damage / pulses as i32).max(1);
                    let interval_ticks = (total_s / pulses as f32 * shared::TICK_RATE_HZ as f32) as u32;
                    // Frost Nova / Rain of Arrows: offset todos os pulses pelo cast_time_s.
                    // Meteor / Smoke Bomb: offset = 0 (rain começa imediatamente).
                    let pulse_offset_ticks = if skill_id == 1046 || skill_id == 1039 {
                        (def.cast_time_s * shared::TICK_RATE_HZ as f32).round() as u32
                    } else { 0 };
                    // Smoke Bomb (1038) aplica poisoned 5s nos hits (visual tint).
                    let poison_dur = if skill_id == 1038 { 5.0_f32 } else { 0.0 };
                    // Whirlwind (1013) e Sword Dance (1005) seguem o caster:
                    // a cada pulse, o AoE eh aplicado na pos ATUAL do owner
                    // (player pode andar durante o spin).  Outros rain skills
                    // sao position-targeted (Meteor cai onde o user mirou).
                    let follow = skill_id == 1013 || skill_id == 1005;
                    let pulse_center = if follow { pos } else { target_pos };
                    for i in 0..pulses {
                        self.pending_delayed_aoe.push(DelayedAoe {
                            target_pos: pulse_center, radius, damage: per_pulse, owner_eid,
                            release_tick: self.tick.wrapping_add(pulse_offset_ticks + i * interval_ticks),
                            poison_dur_s: poison_dur,
                            knockback: def.knockback,
                            follow_owner: follow,
                            stun_dur_s: 0.0,
                        });
                    }
                    tracing::info!(
                        "skill cast: {} (sustained rain {}p × {} dmg over {:.1}s, r{:.1}, wind-up {:.1}s)",
                        def.name, pulses, per_pulse, total_s, radius,
                        pulse_offset_ticks as f32 / shared::TICK_RATE_HZ as f32
                    );
                } else if is_heal {
                    let base_heal = def.base_heal as f32 + stats.wis as f32 * def.scaling_wis;
                    let mut heal_scaled = base_heal * (1.0 + def.per_rank_dmg_pct * (rank.saturating_sub(1) as f32));
                    if rank >= shared::MAX_SKILL_RANK && def.max_rank_heal_pct > 0.0 {
                        heal_scaled *= 1.0 + def.max_rank_heal_pct / 100.0;
                    }
                    let heal = heal_scaled.round() as i32;
                    if heal > 0 {
                        let players = self.find_players_in_radius(target_pos, radius);
                        for tn in players {
                            self.pending_heals.push(PendingHeal { target_net: tn, amount: heal });
                        }
                    }
                    tracing::info!("skill cast: {} (aoe heal r{:.1}) +{}", def.name, radius, heal);
                } else if def.cast_time_s > 0.05 {
                    // Damage delayed pelo cast_time_s — sincroniza com visual
                    // de queda (Meteor) ou wind-up similar.
                    let delay_ticks = (def.cast_time_s * shared::TICK_RATE_HZ as f32).round() as u32;
                    self.pending_delayed_aoe.push(DelayedAoe {
                        target_pos, radius, damage, owner_eid,
                        release_tick: self.tick.wrapping_add(delay_ticks.max(1)),
                        poison_dur_s: 0.0,
                        knockback: def.knockback,
                        follow_owner: false,
                        stun_dur_s: 0.0,
                    });
                    tracing::info!(
                        "skill cast: {} (aoe r{:.1}) dmg={} delayed {:.2}s ({} ticks)",
                        def.name, radius, damage, def.cast_time_s, delay_ticks
                    );
                } else {
                    let enemies = self.find_enemies_in_radius(target_pos, radius);
                    for tn in enemies {
                        let hd = calc_hurt_dir_from_eid(&self.ecs, tn, target_pos);
                        self.pending_skill_hits.push(PendingSkillHit {
                            target_net: tn, damage, attacker_net: owner_eid,
                            hurt_dir: hd, is_crit, from_player: true,
                            knockback: def.knockback,
                        });
                    }
                    // PvP: tambem inclui outros players elegiveis (PK ON ambos).
                    let pvp_players = self.find_pvp_players_in_radius(target_pos, radius, owner_eid);
                    for tn in pvp_players {
                        let hd = calc_hurt_dir_from_eid(&self.ecs, tn, target_pos);
                        self.pending_skill_hits.push(PendingSkillHit {
                            target_net: tn, damage, attacker_net: owner_eid,
                            hurt_dir: hd, is_crit, from_player: true,
                            knockback: def.knockback,
                        });
                    }
                    tracing::info!("skill cast: {} (aoe r{:.1}) dmg={}", def.name, radius, damage);
                }
            }
            "cone" => {
                let range = def.range_tiles.max(shared::MELEE_RANGE);
                let enemies = self.find_enemies_in_cone(pos, dir, range, shared::MELEE_CONE_HALF_ANGLE);
                // Shield Bash (1003): aplica stun nos hits + dispara o swing.
                // Stun = 1.2s no EnemyTag.stunned_until (replicado no snap).
                let is_shield_bash = skill_id == 1003;
                let stun_dur: f32 = if is_shield_bash { 1.2 } else { 0.0 };
                for tn in &enemies {
                    let hd = (-dir).try_normalize().unwrap_or(Vec2::new(-1.0, 0.0));
                    self.pending_skill_hits.push(PendingSkillHit {
                        target_net: *tn, damage, attacker_net: owner_eid,
                        hurt_dir: hd, is_crit, from_player: true,
                        knockback: def.knockback,
                    });
                }
                // PvP: include outros players elegiveis no cone.
                let pvp_players = self.find_pvp_players_in_cone(pos, dir, range, shared::MELEE_CONE_HALF_ANGLE, owner_eid);
                for tn in &pvp_players {
                    let hd = (-dir).try_normalize().unwrap_or(Vec2::new(-1.0, 0.0));
                    self.pending_skill_hits.push(PendingSkillHit {
                        target_net: *tn, damage, attacker_net: owner_eid,
                        hurt_dir: hd, is_crit, from_player: true,
                        knockback: def.knockback,
                    });
                }
                // Apply stun status nos enemies hit (Shield Bash).
                if stun_dur > 0.0 {
                    let now_s = self.sim_time_s;
                    let target_set: std::collections::HashSet<EntityId> =
                        enemies.iter().copied().collect();
                    for (_, (net, tag)) in self.ecs.query_mut::<(&NetId, &mut EnemyTag)>() {
                        if target_set.contains(&net.0) {
                            let exp = now_s + stun_dur;
                            if tag.stunned_until < exp { tag.stunned_until = exp; }
                        }
                    }
                }
                // Sword Dance (1005): seta stance de 4s — proximos 3 SLASHes
                // sao parte do combo, ultimo guaranteed crit. Stance reseta
                // ao completar 3 swings ou expirar.
                if skill_id == 1005 {
                    if let Some(s) = self.sessions.get_mut(&sid) {
                        s.sword_dance_until = self.sim_time_s + 4.0;
                        s.sword_dance_step = 0;
                    }
                }
                tracing::info!("skill cast: {} (cone r{:.1}) dmg={}", def.name, range, damage);
            }
            _ => {
                // Outros target_types implementados na Phase 2.x.
                tracing::debug!("skill cast: target_type '{}' não implementado ainda", def.target_type);
            }
        }
        } // end if !skip_dispatch

        // Broadcast SkillCastFx pra todos clientes logados (gizmos no cliente).
        // session já não está borrowed aqui — NLL drop após `let owner_eid`.
        let chain_points = if self.last_chain_pts.is_empty() {
            None
        } else {
            Some(self.last_chain_pts.iter().map(|v| [v.x, v.y]).collect::<Vec<_>>())
        };
        self.last_chain_pts.clear();
        let fx = ServerMessage::SkillCastFx {
            skill_id, caster_pos: pos, target_pos, target_eid: None,
            caster_eid: Some(owner_eid),
            chain_points,
        };
        for s in self.sessions.values() {
            if s.logged_in {
                let _ = s.handle.to_client.send(fx.clone());
            }
        }
    }

    /// Encontra inimigo mais próximo em linha do `pos` na direção `dir` até `range`.
    /// Retorna (net_id, posição, distância). Filtra por hostilidade (player→enemy).
    fn find_nearest_enemy_in_line(&self, pos: Vec2, dir: Vec2, range: f32, attacker_net: EntityId)
        -> Option<(EntityId, Vec2, f32)> {
        let mut best: Option<(EntityId, Vec2, f32)> = None;
        let cos_half = (15f32.to_radians()).cos(); // lateral 15° de tolerância
        for (e, (net, pos2, kind)) in self.ecs.query::<(&NetId, &Position, &EntityKind)>().iter() {
            if !matches!(kind, EntityKind::Enemy(_)) { continue; }
            if net.0 == attacker_net { continue; }
            // Ignora cadaveres em despawn anim — auto-aim n deve re-mirar morto.
            if let Ok(hp) = self.ecs.get::<&Health>(e) { if hp.current <= 0 { continue; } }
            let delta = pos2.0 - pos;
            let dist = delta.length();
            if dist > range || dist < 0.01 { continue; }
            let d_norm = delta / dist;
            if dir.dot(d_norm) < cos_half { continue; }
            if best.map_or(true, |(_,_,bd)| dist < bd) {
                best = Some((net.0, pos2.0, dist));
            }
        }
        best
    }

    fn find_enemies_in_radius(&self, center: Vec2, radius: f32) -> Vec<EntityId> {
        let r2 = radius * radius;
        let mut out = Vec::new();
        for (e, (net, pos, kind)) in self.ecs.query::<(&NetId, &Position, &EntityKind)>().iter() {
            if !matches!(kind, EntityKind::Enemy(_)) { continue; }
            if let Ok(hp) = self.ecs.get::<&Health>(e) { if hp.current <= 0 { continue; } }
            if pos.0.distance_squared(center) > r2 { continue; }
            // LOS: AoE skill nao acerta atras de WALL.
            if !self.map.has_line_of_sight(center, pos.0) { continue; }
            out.push(net.0);
        }
        out
    }

    fn find_players_in_radius(&self, center: Vec2, radius: f32) -> Vec<EntityId> {
        let r2 = radius * radius;
        let mut out = Vec::new();
        for (_, (net, pos, kind)) in self.ecs.query::<(&NetId, &Position, &EntityKind)>().iter() {
            if !matches!(kind, EntityKind::Player) { continue; }
            if pos.0.distance_squared(center) > r2 { continue; }
            if !self.map.has_line_of_sight(center, pos.0) { continue; }
            out.push(net.0);
        }
        out
    }

    /// Players (PvP-eligible vs `attacker_eid`) em radius. Usado por skills
    /// AOE/cone do player pra incluir outros players com PK Mode compativel.
    fn find_pvp_players_in_radius(&self, center: Vec2, radius: f32, attacker_eid: EntityId) -> Vec<EntityId> {
        let r2 = radius * radius;
        let mut out = Vec::new();
        for (_, (net, pos, kind)) in self.ecs.query::<(&NetId, &Position, &EntityKind)>().iter() {
            if !matches!(kind, EntityKind::Player) { continue; }
            if net.0 == attacker_eid { continue; }
            if pos.0.distance_squared(center) > r2 { continue; }
            if !self.map.has_line_of_sight(center, pos.0) { continue; }
            if !self.can_damage_player(attacker_eid, net.0) { continue; }
            out.push(net.0);
        }
        out
    }

    /// Players (PvP-eligible) em cone (mesma geometria de find_enemies_in_cone).
    fn find_pvp_players_in_cone(&self, origin: Vec2, dir: Vec2, range: f32, half_angle: f32, attacker_eid: EntityId) -> Vec<EntityId> {
        let cos_half = half_angle.cos();
        let r2 = range * range;
        let mut out = Vec::new();
        for (_, (net, pos, kind)) in self.ecs.query::<(&NetId, &Position, &EntityKind)>().iter() {
            if !matches!(kind, EntityKind::Player) { continue; }
            if net.0 == attacker_eid { continue; }
            let delta = pos.0 - origin;
            let d2 = delta.length_squared();
            if d2 > r2 { continue; }
            if let Some(nd) = delta.try_normalize() {
                if dir.dot(nd) < cos_half { continue; }
            }
            if !self.map.has_line_of_sight(origin, pos.0) { continue; }
            if !self.can_damage_player(attacker_eid, net.0) { continue; }
            out.push(net.0);
        }
        out
    }

    fn find_enemies_in_cone(&self, pos: Vec2, dir: Vec2, range: f32, half_angle: f32) -> Vec<EntityId> {
        let cos_half = half_angle.cos();
        let r2 = range * range;
        let mut out = Vec::new();
        for (e, (net, pos2, kind)) in self.ecs.query::<(&NetId, &Position, &EntityKind)>().iter() {
            if !matches!(kind, EntityKind::Enemy(_)) { continue; }
            if let Ok(hp) = self.ecs.get::<&Health>(e) { if hp.current <= 0 { continue; } }
            let delta = pos2.0 - pos;
            let d2 = delta.length_squared();
            if d2 > r2 { continue; }
            if let Some(nd) = delta.try_normalize() {
                if dir.dot(nd) < cos_half { continue; }
            }
            if !self.map.has_line_of_sight(pos, pos2.0) { continue; }
            out.push(net.0);
        }
        out
    }

    /// Chain Lightning: a partir do alvo principal, bounce até `bounces` alvos
    /// extras, com falloff de 25% por bounce. Retorna as posicoes encadeadas
    /// pra cliente renderizar o feixe entre cada salto (Vec vazio = sem chain).
    fn chain_lightning_bounces(&mut self, start_pos: Vec2, exclude: EntityId, attacker: EntityId, base_dmg: i32, bounces: u32) -> Vec<Vec2> {
        let mut chain: Vec<Vec2> = Vec::new();
        let mut excluded = std::collections::HashSet::new();
        excluded.insert(exclude);
        let mut current_pos = start_pos;
        let mut current_dmg = (base_dmg as f32 * 0.75) as i32;
        for _ in 0..bounces {
            if current_dmg < 1 { break; }
            // Próximo enemy mais próximo, raio max 8 tiles, não-excluido.
            let mut best: Option<(EntityId, Vec2, f32)> = None;
            for (e, (net, pos2, kind)) in self.ecs.query::<(&NetId, &Position, &EntityKind)>().iter() {
                if !matches!(kind, EntityKind::Enemy(_)) { continue; }
                if excluded.contains(&net.0) { continue; }
                if let Ok(hp) = self.ecs.get::<&Health>(e) { if hp.current <= 0 { continue; } }
                let dist = pos2.0.distance(current_pos);
                if dist > 8.0 { continue; }
                if best.map_or(true, |(_,_,bd)| dist < bd) {
                    best = Some((net.0, pos2.0, dist));
                }
            }
            if let Some((tn, tp, _)) = best {
                let hd = calc_hurt_dir_from_eid(&self.ecs, tn, current_pos);
                self.pending_skill_hits.push(PendingSkillHit {
                    target_net: tn, damage: current_dmg, attacker_net: attacker,
                    hurt_dir: hd, is_crit: false, from_player: true,
                    knockback: 0.4, // Chain Lightning bounce: leve
                });
                excluded.insert(tn);
                chain.push(tp);
                current_pos = tp;
                current_dmg = (current_dmg as f32 * 0.75) as i32;
            } else {
                break;
            }
        }
        chain
    }

    /// Aprende rank 1 da skill — gasta 1 SP. Valida unlock_char_lvl,
    /// unlock_prof_lvl, e SP suficiente. No-op silencioso se não pode.
    fn handle_skill_learn(&mut self, sid: SessionId, skill_id: u32) {
        let Some(session) = self.sessions.get_mut(&sid) else { return };
        if !session.logged_in { return; }
        // Já aprendida?
        if session.learned_skills.iter().any(|s| s.skill_id == skill_id) {
            tracing::debug!("skill_learn: {} já aprendida", skill_id);
            return;
        }
        let Some(def) = crate::skills::skill_of(skill_id) else {
            tracing::warn!("skill_learn: skill_id {} não existe", skill_id);
            return;
        };
        let char_lvl = shared::level_of_xp_with_mult(session.xp, crate::economy::xp_multiplier());
        if !crate::skills::can_unlock(&def, char_lvl, &session.proficiencies) {
            tracing::debug!(
                "skill_learn: req não atendido (skill={} char_lvl={} need char>={} prof>={})",
                skill_id, char_lvl, def.unlock_char_lvl, def.unlock_prof_lvl
            );
            return;
        }
        let cost = shared::sp_cost_for_next_rank(0); // rank 0→1
        let avail = session.skill_points_earned.saturating_sub(session.skill_points_spent);
        if avail < cost { return; }
        session.skill_points_spent = session.skill_points_spent.saturating_add(cost);
        session.learned_skills.push(shared::LearnedSkill {
            skill_id,
            rank: 1,
            equipped_slot: None,
        });
        session.skills_dirty = true;
    }

    /// Sobe rank +1 (até MAX_SKILL_RANK). Custo varia por rank.
    fn handle_skill_rank_up(&mut self, sid: SessionId, skill_id: u32) {
        let Some(session) = self.sessions.get_mut(&sid) else { return };
        if !session.logged_in { return; }
        let Some(idx) = session.learned_skills.iter().position(|s| s.skill_id == skill_id) else {
            return; // não aprendida ainda
        };
        let cur_rank = session.learned_skills[idx].rank;
        if cur_rank >= shared::MAX_SKILL_RANK { return; }
        let cost = shared::sp_cost_for_next_rank(cur_rank);
        let avail = session.skill_points_earned.saturating_sub(session.skill_points_spent);
        if avail < cost { return; }

        // Gate: rank-up exige char_lvl + prof_lvl escalados por rank.
        // Lookup da skill no cache pra pegar unlock_char/unlock_prof + prof_kind.
        let target_rank = cur_rank + 1;
        let skill_def = crate::skills::skill_of(skill_id);
        if let Some(def) = skill_def {
            let (need_char, need_prof) = shared::skill_rank_requirements(
                def.unlock_char_lvl as u8, def.unlock_prof_lvl as u8, target_rank,
            );
            let char_lvl = shared::level_of_xp_with_mult(session.xp, crate::economy::xp_multiplier());
            if (char_lvl as u8) < need_char { return; }
            let prof_xp = shared::Proficiency::from_str(&def.prof)
                .map(|p| session.proficiencies[p as usize])
                .unwrap_or(0);
            let prof_lvl = shared::proficiency_level(prof_xp);
            if (prof_lvl as u8) < need_prof { return; }
        }

        session.skill_points_spent = session.skill_points_spent.saturating_add(cost);
        session.learned_skills[idx].rank = target_rank;
        session.skills_dirty = true;
    }

    /// Equipa skill ativa em slot 0..=5. `slot=None` ou `skill_id=0` desequipa.
    /// Passivas ignoram (o slot fica None permanentemente).
    fn handle_skill_equip(&mut self, sid: SessionId, skill_id: u32, slot: Option<u8>) {
        let Some(session) = self.sessions.get_mut(&sid) else { return };
        if !session.logged_in { return; }

        // Caso 1: desequipar (skill_id=0 ou slot=None)
        if skill_id == 0 {
            if let Some(target_slot) = slot {
                for s in session.learned_skills.iter_mut() {
                    if s.equipped_slot == Some(target_slot) {
                        s.equipped_slot = None;
                    }
                }
                session.skills_dirty = true;
            }
            return;
        }
        if slot.is_none() {
            // Desequipa essa skill de qualquer slot.
            for s in session.learned_skills.iter_mut() {
                if s.skill_id == skill_id {
                    s.equipped_slot = None;
                }
            }
            session.skills_dirty = true;
            return;
        }

        let target_slot = slot.unwrap();
        if (target_slot as usize) >= shared::SKILL_BAR_SLOTS {
            return;
        }

        // Skill aprendida?
        let Some(idx) = session.learned_skills.iter().position(|s| s.skill_id == skill_id) else {
            return;
        };

        // Passivas não vão pra slot — ignoram silenciosamente.
        let Some(def) = crate::skills::skill_of(skill_id) else { return };
        if def.is_passive { return; }

        // Tira quem estiver no slot alvo (swap implícito).
        for (i, s) in session.learned_skills.iter_mut().enumerate() {
            if i != idx && s.equipped_slot == Some(target_slot) {
                s.equipped_slot = None;
            }
        }
        session.learned_skills[idx].equipped_slot = Some(target_slot);
        session.skills_dirty = true;
    }

    pub fn step(&mut self, dt: f32) {
        self.tick = self.tick.wrapping_add(1);

        // Detecta hot-reload da economy (admin editou via web) e re-emite
        // ItemsConfig pra todos os clientes logados — assim nomes/icones
        // sobem ao vivo no inventario.
        let v = crate::economy::current_version();
        if v != self.last_econ_version {
            self.last_econ_version = v;
            let cfg = crate::economy::items_config();
            let skills_cfg = crate::skills::all_skills();
            let res_src = crate::economy::resource_sources_snapshot();
            for s in self.sessions.values() {
                if !s.logged_in { continue; }
                let _ = s.handle.to_client.send(ServerMessage::ItemsConfig {
                    items: cfg.clone(),
                });
                // Skills compartilham o mesmo `economy_version` — re-broadcast
                // junto pra UI atualizar nome/icon/scaling sem relogar.
                let _ = s.handle.to_client.send(ServerMessage::SkillsConfig {
                    skills: skills_cfg.clone(),
                });
                // Reverse-index das loot tables muda junto com economy (mob
                // drops, farm drops, item.active). Re-broadcast pra UI.
                let _ = s.handle.to_client.send(ServerMessage::ResourceSources {
                    items: res_src.clone(),
                });
            }
        }

        // Hot-reload de receitas — admin pode INSERT/UPDATE em `craft_recipes`,
        // bumpar `recipes_version.version` e em ate 5s todos os clientes
        // logados recebem o catalogo novo sem relogar.
        if crate::recipes::take_broadcast_flag() {
            let recipes_cfg = crate::recipes::all();
            for s in self.sessions.values() {
                if !s.logged_in { continue; }
                let _ = s.handle.to_client.send(ServerMessage::CraftRecipes {
                    recipes: recipes_cfg.clone(),
                });
            }
            tracing::info!("[recipes] broadcast pra {} sessoes", self.sessions.len());
        }

        // Skills dirty: jogadores que aprenderam/upgrade/equiparam recebem o
        // estado novo. Resta-se após o broadcast.
        let dirty: Vec<SessionId> = self.sessions.iter()
            .filter(|(_, s)| s.logged_in && s.skills_dirty)
            .map(|(id, _)| *id)
            .collect();
        for sid in dirty {
            if let Some(s) = self.sessions.get_mut(&sid) {
                let state = shared::skills::PlayerSkillsState {
                    sp_earned: s.skill_points_earned,
                    sp_spent: s.skill_points_spent,
                    skills: s.learned_skills.clone(),
                };
                let _ = s.handle.to_client.send(ServerMessage::PlayerSkillsUpdate { state });
                s.skills_dirty = false;
            }
        }

        // --- Teleporte via portais ---
        self.process_portal_teleports(dt);

        // Tempo de simulação acumulado — usado pelas spawn zones pra calcular
        // delay de respawn.
        self.sim_time_s += dt;

        // Bombas de canhao em voo (parabolicas, AoE no impacto).
        self.tick_cannon_bombs(dt);

        // Spawn zones do MapFile: preenche quotas + respawna com delay.
        if self.from_mapfile && !self.spawn_zones.is_empty() {
            self.tick_spawn_zones();
        }
        if self.from_mapfile && !self.boss_areas.is_empty() {
            self.tick_boss_areas();
        }
        // Farm node respawn.
        if self.from_mapfile && !self.farm_nodes.is_empty() {
            self.tick_farm_respawn();
        }
        // Quests EXPLORE: marca READY quando o player chega na área-alvo.
        self.tick_quest_explore();

        // Tutorial: auto-grant da 1a quest, auto-avanço da cadeia e dummy de treino.
        if self.tutorial_mode { self.tick_tutorial_quests(); }
        if self.dungeon_mode { self.tick_dungeon_runs(); }

        // Peixes do oceano: mantém população perto dos players, move wander +
        // atração pela boia, detecta fisgada. Só faz algo se houver água.
        self.tick_fish(dt);

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
        // NOTE: removed_this_tick NAO eh limpo aqui. O clear() agora roda no
        // FIM de send_snapshots(), depois de despachar pra todos os clients.
        // Limpar aqui apagava ids pushed em on_message (dismount, spawn boat)
        // antes do snapshot conseguir incluir.

        // ── A: processar inputs de jogadores ──────────────────────────────────
        struct InputResult {
            entity: Entity,
            new_vel: Vec2,
            wants_attack: bool,
            owner_id: EntityId,
            /// Alvo do combate por target. `None` = ataque sem alvo (nao
            /// acontece hoje, mas mantem a estrutura honesta).
            target: Option<EntityId>,
            aim: Vec2,
            damage: i32,
            is_crit: bool,
            is_melee: bool,
            /// Step do combo (0=Slash1, 1=Slash2, 2=Finisher). Só relevante
            /// pra SLASH attacks; ignorado em SHOOT/THRUST.
            combo_step: u8,
            /// Visual do projétil: 0 = arrow (Bow), 1 = fireball (Staff/Wand).
            proj_kind: u8,
            /// Codigo de animacao (`shared::attack_anim::*`) que outros
            /// clientes devem tocar nesse player no tick em que ataca.
            attack_anim_code: u8,
            /// True no tick em que o dash COMECA — usado pra marcar
            /// PlayerTag.attack_anim_pending=DASH e replicar pra todos.
            dash_started: bool,
        }
        // ── Alvos (combate por target) ────────────────────────────────────
        // Posicao de tudo que pode ser alvo, montado UMA vez por tick. Sem
        // isto cada player varreria o ECS atras do proprio alvo, o que vira
        // O(jogadores x entidades) — o oposto do que "servidor aguentar
        // muitos mobs" pede.
        let target_pos: HashMap<EntityId, Vec2> = self
            .ecs
            .query::<(&NetId, &Position)>()
            .iter()
            .map(|(_, (net, pos))| (net.0, pos.0))
            .collect();

        let mut input_results: Vec<InputResult> = Vec::new();
        // Inputs de players montados em barco — processado depois do physics
        // step pra mover os barcos (tile-water-only) e disparar cannon.
        struct MountedInput {
            move_dir: Vec2,
            aim:      Vec2,
            buttons:  u32,
        }
        let mut mounted_inputs: HashMap<Entity, MountedInput> = HashMap::new();
        // Casters que tiveram o cast cancelado por movimento neste tick:
        // (entity_id, skill_id). Depois do loop de sessoes, dropa
        // pending_delayed_aoe deles e broadcasta SkillCastCancel pra clientes.
        let mut cancelled_cast_owners: Vec<(EntityId, u32)> = Vec::new();
        // Quem travou seguindo rota: o A* roda DEPOIS do laco, que aqui
        // `self` esta' emprestado pelas sessoes.
        let mut refazer_rota: Vec<(SessionId, Vec2)> = Vec::new();
        for (sid_da_sessao, session) in self.sessions.iter_mut() {
            let sid_da_sessao = *sid_da_sessao;
            if session.attack_cooldown > 0.0 { session.attack_cooldown -= dt; }
            if session.dash_cooldown > 0.0 { session.dash_cooldown -= dt; }

            // Regen de MP (continua mesmo sem input pendente).
            let mp_max = session.stats.mp_max as f32;
            if session.mp_current < mp_max {
                session.mp_current = (session.mp_current + shared::MP_REGEN_PER_SEC * dt).min(mp_max);
            }

            // Regen de HP (escalado por VIT via stats.hp_regen). Roda no
            // tick em vez de em UseItem pra dar regen passivo continuo.
            // Skipado se downed (player caido nao se cura sozinho).
            // Resting boost: se player tah parado (sem mover/cast/hit recente)
            // por 2s, multiplica regen por 4x = "sentou pra recuperar".
            if !session.downed && session.stats.hp_regen > 0.0 {
                let now_s = self.sim_time_s;
                let move_sq = session.pending_input.as_ref()
                    .map(|f| f.move_dir.length_squared()).unwrap_or(0.0);
                let is_idle = move_sq < 0.01
                    && session.casting_until <= now_s
                    && session.hurt_until <= now_s
                    && session.leap_until <= now_s;
                if is_idle {
                    if session.idle_since == 0.0 { session.idle_since = now_s; }
                } else {
                    session.idle_since = 0.0;
                }
                let resting = session.idle_since > 0.0 && (now_s - session.idle_since) > 2.0;
                let regen_mult = if resting { 4.0 } else { 1.0 };
                if let Some(e) = session.entity {
                    if let Ok(mut h) = self.ecs.get::<&mut shared::Health>(e) {
                        if h.current < h.max && h.current > 0 {
                            let new_hp_f = h.current as f32 + session.stats.hp_regen * regen_mult * dt;
                            h.current = (new_hp_f as i32).min(h.max);
                        }
                    }
                }
            }

            // Regen de stamina (continua sempre — dash agora e tap, nao drena continuo).
            let stam_max = session.stats.stamina_max as f32;
            if session.stamina_current < stam_max {
                session.stamina_current = (session.stamina_current
                    + session.stats.stamina_regen * dt)
                    .min(stam_max);
            }
            // Regen de poise: SO regenera fora de combate (>=2s sem hit).
            // Recovery rate: 10 / s (poise volta cheio em 5s).
            const POISE_REGEN_PER_SEC: f32 = 10.0;
            const COMBAT_TIMEOUT_S: f32 = 2.0;
            let poise_max = session.stats.poise_max as f32;
            if session.poise_current < poise_max
                && (self.sim_time_s - session.last_combat_at_s) >= COMBAT_TIMEOUT_S
            {
                session.poise_current = (session.poise_current
                    + POISE_REGEN_PER_SEC * dt)
                    .min(poise_max);
            }

            let Some(entity) = session.entity else { continue };
            let Some(mut frame) = session.pending_input.take() else { continue };
            session.last_input_seq = frame.seq;
            // Se esta sendo carregado, posicao vem do carregador — ignora input
            if session.carried_by.is_some() {
                continue;
            }
            // Player montado em barco (Boat 2.5D): rota o move_dir pro
            // mounted_inputs (boat le pra helm/walk on deck). Zera o
            // move_dir do frame pra que o physics-loop normal NAO mova o
            // player (boat fisica controla a posicao via local_pos+rotate).
            // Mantem buttons/aim — assim ataques, skills, dash continuam
            // funcionando normalmente em cima do barco.
            if self.ecs.get::<&Mounted>(entity).is_ok() {
                let mv = if frame.move_dir.length_squared() > 1.0 {
                    frame.move_dir.normalize()
                } else { frame.move_dir };
                mounted_inputs.insert(entity, MountedInput {
                    move_dir: mv,
                    aim:      frame.aim,
                    buttons:  frame.buttons,
                });
                // Persiste o ultimo move_dir no Mounted — input vem ~60Hz,
                // tick server e' 30Hz. Sem persistir, ticks sem input
                // fariam vel=ZERO e walk anim flickaria.
                if let Ok(mut m) = self.ecs.get::<&mut Mounted>(entity) {
                    m.last_move_dir = mv;
                }
                // Zera move_dir do frame — boat fisica handla o player.
                frame.move_dir = glam::Vec2::ZERO;
                // NAO continue — segue pro processamento de attacks/skills/etc.
            }
            // CASTING: durante cast_time_s o player fica travado na pose. A
            // checagem de cancel-por-movimento eh feita ABAIXO, depois do dir
            // ser processado (in_hurt zera dir → não cancela durante stagger).
            // Aqui só zeramos buttons pra bloquear ataque/defesa/novo cast.
            // LEAP: similar — durante leap_until, sem input (movimento e atacks
            // bloqueados; pos eh interpolada no step E.1).
            let casting = session.casting_until > self.sim_time_s;
            let in_leap = session.leap_until > self.sim_time_s;
            if casting || in_leap {
                frame.buttons = 0;
            }
            if in_leap {
                frame.move_dir = glam::Vec2::ZERO;
            }
            // Edge-detect dos botoes pra parry window. Press = bit foi 0 no
            // frame anterior e ficou 1 agora. Salva sim_time pra que o loop
            // de damage_events compare com PARRY_WINDOW_S.
            let pressed_primary   = (frame.buttons & buttons::PRIMARY   != 0)
                                 && (session.prev_buttons & buttons::PRIMARY   == 0);
            let pressed_secondary = (frame.buttons & buttons::SECONDARY != 0)
                                 && (session.prev_buttons & buttons::SECONDARY == 0);
            if pressed_primary   { session.last_press_primary_at   = self.sim_time_s; }
            if pressed_secondary { session.last_press_secondary_at = self.sim_time_s; }
            // ── PULO ──
            // Borda de subida, e nao "botao segurado": pulo contínuo enquanto
            // se segura a tecla vira voo rasante em terreno de degraus.
            let pressed_pulo = (frame.buttons & buttons::PULO != 0)
                && (session.prev_buttons & buttons::PULO == 0);
            if pressed_pulo
                && self.sim_time_s >= session.pulo_pronto_em
                && !session.downed
                && self.sim_time_s >= session.stagger_until
            {
                session.pulo_ate = self.sim_time_s + shared::PULO_DURACAO;
                session.pulo_pronto_em =
                    self.sim_time_s + shared::PULO_DURACAO + shared::PULO_ESPERA;
            }
            session.prev_buttons = frame.buttons;
            // RMB held = defesa ativa. Drena stamina apenas no hit. Disponivel
            // pra TODAS as armas — quem tem escudo no offhand bloqueia "fisico"
            // (visual ShieldBash); o resto bloqueia com "escudo de energia"
            // (visual Push + bolha) replicado pelo cliente. Stat bonuses do
            // shield (defense+8, hp+75) sao o incentivo pra equipar.
            let staggered = self.sim_time_s < session.stagger_until;
            let block_cost_now = shared::combat::block_stamina_cost(&session.stats);
            let was_defending = session.defending;
            session.defending = (frame.buttons & buttons::SECONDARY != 0)
                && !session.downed
                && !staggered
                && session.stamina_current >= block_cost_now;
            // Edge-press: ao iniciar defesa, recarrega o buffer de poise
            // que defending absorve (escala com lvl + bonus de escudo).
            if session.defending && !was_defending {
                session.defending_poise_buffer = defending_poise_max(session);
            }

            // Stagger/Downed: ignora movimento e ataques. Player downed fica
            // travado na pose sentada — não pode andar até levantar.
            let in_hurt = session.hurt_until > self.sim_time_s;
            // Rota do toque no chao: o SERVIDOR conduz. Ela so' entra quando
            // o joystick esta' parado — comando manual ja' limpou a fila la'
            // no recebimento da mensagem.
            // `target_pos` ja' tem a posicao de toda entidade deste tick.
            let aqui = target_pos
                .get(&session.entity_id)
                .copied()
                .unwrap_or(Vec2::ZERO);
            let mut veio_da_rota = false;
            if frame.move_dir.length_squared() <= 0.01 && !session.rota.vazia() {
                if let Some(dir) = session.rota.direcao(aqui) {
                    frame.move_dir = dir;
                    veio_da_rota = true;
                }
                // Rota velha nao se remenda, se REFAZ. O seguidor avisa quando
                // para de se aproximar; quem sabe achar caminho e' o A*, e ele
                // vai achar a partir de onde o corpo esta' agora — que nao e'
                // mais onde estava quando a rota saiu.
                if session.rota.travado() {
                    refazer_rota.push((sid_da_sessao, session.rota.destino()));
                }
            }
            let dir = if in_hurt || session.downed || staggered {
                Vec2::ZERO
            } else if frame.move_dir.length_squared() > 1.0 {
                frame.move_dir.normalize()
            } else {
                frame.move_dir
            };
            // ── PULO AUTOMATICO NA ROTA ──
            //
            // O A* pode escolher um trecho que so' se vence pulando (ele paga
            // um custo a mais por isso, entao e' excecao). Quem anda no
            // teclado aperta espaco; quem esta' seguindo rota nao tem mao
            // nenhuma no teclado, e sem isto o boneco iria bater na quina ate'
            // a paciencia do seguidor acabar.
            if veio_da_rota && dir.length_squared() > 0.01 {
                if let Some(ilha) = self.ilha.as_ref() {
                    // A pergunta e' feita ao proprio mover: "pular me faria
                    // andar mais que andar?". Qualquer outra formulacao ja'
                    // discordou dele e deixou o corpo empurrando a quina.
                    let passo = dir.normalize_or_zero() * shared::PLAYER_SPEED;
                    if ilha.precisa_pular(aqui, passo, dt, ENTITY_RADIUS)
                        && self.sim_time_s >= session.pulo_pronto_em
                        && !session.downed
                        && self.sim_time_s >= session.stagger_until
                    {
                        session.pulo_ate = self.sim_time_s + shared::PULO_DURACAO;
                        session.pulo_pronto_em =
                            self.sim_time_s + shared::PULO_DURACAO + shared::PULO_ESPERA;
                    }
                }
            }

            // Quick Draw passive: rastreia ultimo movimento. Se mover (dir != 0),
            // atualiza last_movement_at_s e reseta quickdraw_consumed pra liberar
            // o crit da proxima "1ª flecha".
            if dir.length_squared() > 0.001 {
                session.last_movement_at_s = self.sim_time_s;
                session.quickdraw_consumed = false;
            }

            // ── Climb-jump: empurrar contra um cliff escalavel por
            // CLIMB_HOLD_TIME dispara um pulo por cima da wall. Reusa o
            // mecanismo de leap (move A→B ignorando colisao + anima via
            // leap_y no snapshot). So' detecta aqui; o leap roda na pass E.1.
            {
                const CLIMB_HOLD_TIME: f32 = 0.25;     // s segurando contra a borda
                const CLIMB_MAX_DIST: i32 = 3;         // tiles ate o chao do outro lado
                const CLIMB_LEAP_DURATION: f32 = 0.5;  // = LEAP_DURATION (pass E.1)
                const CLIMB_COOLDOWN: f32 = 0.2;       // anti double-fire
                let now_s = self.sim_time_s;
                // Em ilha nao existe: o relevo e' campo de altura, nao tem
                // tile WALL pra escalar, e o pulo de verdade ja' cobre o caso.
                // Deixar rodando so' abriria porta pra um leap sem colisao.
                let busy = self.ilha.is_some()
                    || in_hurt || session.downed || staggered
                    || session.leap_until > now_s
                    || now_s < session.dash_until
                    || session.casting_until > now_s
                    || now_s < session.climb_cooldown;
                if busy || dir.length_squared() < 0.5 {
                    session.climb_timer = 0.0;
                } else if let Some(e) = session.entity {
                    let pos = self.ecs.get::<&Position>(e).map(|p| p.0).unwrap_or(Vec2::ZERO);
                    // Cardinal dominante do input (evita ambiguidade diagonal).
                    let (cdx, cdy) = if dir.x.abs() >= dir.y.abs() {
                        (dir.x.signum() as i32, 0)
                    } else {
                        (0, dir.y.signum() as i32)
                    };
                    let px = pos.x.floor() as i32;
                    let py = pos.y.floor() as i32;
                    // Tile imediatamente a frente precisa ser WALL (colado num
                    // cliff). Procura o 1o FLOOR depois da(s) wall(s). WATER no
                    // caminho = borda pro mar → nao escala. FLOOR em safe zone =
                    // muro de cidade → nao escala. Sobra: cliffs entre niveis.
                    let mut landing: Option<Vec2> = None;
                    if self.map.get(px + cdx, py + cdy) == shared::constants::tile_id::WALL {
                        let mut k = 2;
                        while k <= CLIMB_MAX_DIST {
                            let (tx, ty) = (px + cdx * k, py + cdy * k);
                            let t = self.map.get(tx, ty);
                            if t == shared::constants::tile_id::WATER { break; }
                            if t == shared::constants::tile_id::FLOOR {
                                let lp = Vec2::new(tx as f32 + 0.5, ty as f32 + 0.5);
                                let in_safe = self.safe_zones.iter().any(|(o, sz)|
                                    lp.x >= o.x && lp.x <= o.x + sz.x &&
                                    lp.y >= o.y && lp.y <= o.y + sz.y);
                                if !in_safe { landing = Some(lp); }
                                break;
                            }
                            k += 1; // wall mais grosso, continua procurando
                        }
                    }
                    if let Some(lp) = landing {
                        session.climb_timer += dt;
                        if session.climb_timer >= CLIMB_HOLD_TIME {
                            session.leap_until = now_s + CLIMB_LEAP_DURATION;
                            session.leap_start_pos = pos;
                            session.leap_target = lp;
                            session.leap_damage = 0;
                            session.leap_radius = 0.0;
                            session.climb_timer = 0.0;
                            session.climb_cooldown = now_s + CLIMB_COOLDOWN;
                        }
                    } else {
                        session.climb_timer = 0.0;
                    }
                }
            }
            // CAST CANCEL POR MOVIMENTO: usa `dir` processado (zerado em
            // hurt/staggered/downed → cast NAO cancela durante stagger).
            // Grace period 0.3s do inicio do cast → permite player que tava
            // andando começar o cast sem cancelar de imediato.
            // No cancel: REFUND mp/stamina + remove cooldown PRA MAIORIA das
            // skills. Excecao: skills high-impact (rain AoE + Group Heal)
            // mantem cd pra evitar exploit de cast → preview/reposicionar →
            // cancel/re-cast.
            //   1045 Meteor, 1046 Frost Nova, 1039 Rain of Arrows,
            //   1038 Smoke Bomb, 1053 Group Heal.
            if casting && dir.length_squared() > 0.001 {
                session.cast_movement_ticks = session.cast_movement_ticks.saturating_add(1);
            } else if casting {
                session.cast_movement_ticks = 0;
            }
            // Cancela apos >= 2 ticks consecutivos de movimento. Na tick
            // em que o player toma hit, `hurt_until` ainda nao foi setado
            // (damage_events roda DEPOIS do input neste step), entao
            // `in_hurt` eh false e `dir` nao eh zerado mesmo em hurt —
            // 1 tick basta pra um falso-positive cancelar cast. 2 ticks
            // garantem que a tick seguinte ja viu `in_hurt=true` e zerou
            // dir, descartando o cancelamento.
            if casting && session.cast_movement_ticks >= 2 {
                let cast_age = self.sim_time_s - session.casting_started_at_s;
                if cast_age >= 0.3 {
                    let cancelled_skill = session.casting_skill_id;
                    let mp_max = session.stats.mp_max as f32;
                    let st_max = session.stats.stamina_max as f32;
                    session.mp_current = (session.mp_current + session.casting_mp_paid).min(mp_max);
                    session.stamina_current = (session.stamina_current + session.casting_st_paid).min(st_max);
                    let keep_cd = matches!(cancelled_skill, 1045 | 1046 | 1039 | 1038 | 1053);
                    if !keep_cd {
                        session.skill_cds.remove(&cancelled_skill);
                    }
                    session.casting_until = 0.0;
                    session.casting_skill_id = 0;
                    session.casting_mp_paid = 0.0;
                    session.casting_st_paid = 0.0;
                    session.cast_movement_ticks = 0;
                    cancelled_cast_owners.push((session.entity_id, cancelled_skill));
                    tracing::info!(
                        "cast cancelado por movimento: skill={} player={:?} age={:.2}s (refund mp+st{})",
                        cancelled_skill, session.entity_id, cast_age,
                        if keep_cd { ", cd MANTIDO" } else { ", cd reset" }
                    );
                }
            }
            // Downed/Carregando/Hurt/Dashing/Defending/Staggered: sem ataques
            let was_dashing = self.sim_time_s < session.dash_until;
            // Preenchida pelo bloco abaixo quando o auto-ataque dispara.
            let mut attack_aim: Option<Vec2> = None;
            let wants_attack = if session.downed || session.carrying.is_some() || in_hurt
                || was_dashing || session.defending || staggered {
                false
            } else {
                let has_stam = session.stamina_current >= shared::ATTACK_STAMINA_COST;
                // ── Auto-ataque por alvo ──────────────────────────────────
                // O ataque basico nao depende mais de botao nem de mira: com
                // alvo vivo dentro do alcance da arma, o servidor bate no
                // ritmo do cooldown. `auto_aim` e' a direcao ate o alvo, e e'
                // ela que alimenta o cone/projetil la embaixo — o pipeline de
                // dano continua o mesmo, so' mudou quem aponta.
                let auto_aim = session.target.and_then(|t| {
                    let me = target_pos.get(&session.entity_id)?;
                    let tp = target_pos.get(&t)?;
                    let d = *tp - *me;
                    let wid = session.equipment.weapon.unwrap_or(0);
                    let range = if shared::weapon_is_melee(wid) {
                        shared::MELEE_RANGE
                    } else {
                        shared::RANGED_ATTACK_RANGE
                    };
                    let dist_sq = d.length_squared();
                    if dist_sq > 1e-6 && dist_sq <= range * range {
                        Some(d.normalize())
                    } else {
                        None
                    }
                });
                let w = auto_aim.is_some()
                     && session.attack_cooldown <= 0.0
                     && has_stam;
                if w {
                    let weapon_id = session.equipment.weapon.unwrap_or(0);
                    let base_cd = if shared::weapon_is_melee(weapon_id) {
                        ATTACK_COOLDOWN
                    } else {
                        match shared::Proficiency::from_item(weapon_id) {
                            shared::Proficiency::Wand | shared::Proficiency::Staff =>
                                shared::MAGIC_ATTACK_COOLDOWN,
                            _ => shared::BOW_ATTACK_COOLDOWN,
                        }
                    };
                    // Atk speed (DES + items) divide o cooldown. Pra equilibrar:
                    //   - Magia (Wand/Staff): scaling reduzido a 30% — DEX ajuda
                    //     pouco no cast rate.
                    //   - Two-handed nao-magia (GreatSword/Bow/Crossbow/Spear):
                    //     scaling a 60% — armas pesadas/longas ganham menos.
                    //   - 1H melee (Sword/Dagger/etc): full scaling.
                    // Bonus = atk_speed_mult - 1; aplica fator e re-soma a 1.
                    let raw_mult = session.stats.attack_speed_mult.max(0.5);
                    let bonus = raw_mult - 1.0;
                    let is_caster = matches!(
                        shared::Proficiency::from_item(weapon_id),
                        shared::Proficiency::Wand | shared::Proficiency::Staff
                    );
                    let is_two_handed = !shared::weapon_allows_offhand(weapon_id)
                        && weapon_id != 0;
                    let scale_factor = if is_caster { 0.30 }
                        else if is_two_handed { 0.60 }
                        else { 1.0 };
                    // Bloodthirst (1011) buff: +20% atk speed enquanto ativo.
                    let bt_mult = if session.bloodthirst_until > self.sim_time_s { 1.20 } else { 1.0 };
                    let atk_speed = (1.0 + bonus * scale_factor).max(0.5) * bt_mult;
                    session.attack_cooldown = base_cd / atk_speed;
                    session.stamina_current =
                        (session.stamina_current - shared::ATTACK_STAMINA_COST).max(0.0);
                    attack_aim = auto_aim;
                }
                w
            };


            // SPD escala speed_mult (1.0 + 0.02×SPD por ponto).
            let spd_scale = session.stats.speed_mult.max(0.1);
            // Sprint: hold Shift (bit SPRINT) → 1.65× a velocidade base, drena
            // stamina por segundo. Bloqueado se stamina vazia, defending,
            // downed ou carrying.
            let wants_sprint = !session.downed
                && session.carrying.is_none()
                && !session.defending
                && (frame.buttons & buttons::SPRINT != 0)
                && session.stamina_current > 0.0
                && dir.length_squared() > 0.01; // só sprinta enquanto mexendo
            let sprint_mult = if wants_sprint { shared::SPRINT_SPEED_MULT } else { 1.0 };
            if wants_sprint {
                session.stamina_current =
                    (session.stamina_current - shared::STAMINA_DRAIN_PER_SEC * dt).max(0.0);
            }
            let base_speed = if session.downed {
                PLAYER_SPEED * shared::DOWNED_SPEED_MULT * spd_scale
            } else if session.carrying.is_some() {
                PLAYER_SPEED * 0.5 * spd_scale
            } else if session.defending {
                PLAYER_SPEED * shared::MOVE_SPEED_DEFENDING_MULT * spd_scale
            } else {
                PLAYER_SPEED * spd_scale * sprint_mult
            };

            // Dash: tap-button (Space/Shift). Impulso linear na direcao do
            // movimento atual (ou facing se parado), bloqueia movimento e
            // da i-frames durante a duracao. Bloqueado enquanto defending
            // (RMB held) ou em stagger pos-parry.
            let wants_dash = !session.downed
                && !in_hurt
                && !session.defending
                && !staggered
                && (frame.buttons & buttons::DASH != 0)
                && session.dash_cooldown <= 0.0
                && session.stamina_current >= shared::DASH_STAMINA_COST as f32
                && session.dash_until <= self.sim_time_s;
            let dashing = self.sim_time_s < session.dash_until;
            let speed = if dashing {
                // Player ja esta dashando — mantem direcao/velocidade fixas
                shared::DASH_SPEED
            } else if wants_dash {
                // Inicia dash agora
                let dash_dir = if dir.length_squared() > 0.01 {
                    dir.normalize()
                } else {
                    // sem input → usa hurt_dir oposto se existe, senao Vec2::Y
                    Vec2::new(0.0, -1.0) // S como fallback
                };
                session.dash_until = self.sim_time_s + shared::DASH_DURATION;
                session.dash_dir = dash_dir;
                // SPD reduz dash cooldown via dash_cd_mult (independente do
                // speed_mult de movimento). Final cd = DASH_COOLDOWN / mult.
                // Cooldown comeca a contar a partir do FIM do dash — isso e
                // garantido porque o decremento so roda no proximo tick e
                // em sec A `dash_until <= sim_time` ja terminou.
                let cd_mult = session.stats.dash_cd_mult.max(0.1);
                session.dash_cooldown = (shared::DASH_COOLDOWN / cd_mult)
                    .max(shared::DASH_COOLDOWN_MIN)
                    + shared::DASH_DURATION; // cd corre durante o dash; soma a duracao
                                             // pra que o "0.2s minimo" seja apos o fim
                session.stamina_current =
                    (session.stamina_current - shared::DASH_STAMINA_COST as f32).max(0.0);
                shared::DASH_SPEED
            } else {
                base_speed
            };
            // Override de direcao se em dash
            let dir = if dashing || wants_dash {
                if wants_dash { session.dash_dir } else { session.dash_dir }
            } else {
                dir
            };

            let weapon_id = session.equipment.weapon.unwrap_or(0);
            let proj_kind: u8 = match shared::Proficiency::from_item(weapon_id) {
                shared::Proficiency::Wand  => 1, // fireball
                shared::Proficiency::Staff => 5, // electric ball
                _                          => 0, // arrow
            };
            // Crit roll por ataque: roll uniforme; se hit, multiplica
            // damage. Roll feito no SOURCE pra gerar exatamente 1 valor de
            // dano por swing — todos os alvos no cone recebem o mesmo
            // (consistente com regras tipo Diablo/RoTMG).
            let mut crit = session.stats.crit_chance > 0.0
                && fastrand::f32() < session.stats.crit_chance;
            // Quick Draw passive (Bow T2): se Bow equipada e player parado
            // >= 1.0s, força crit no próximo tiro (consome ate o player
            // mover de novo). Rank ainda nao da bonus extra (r5/r10 TODO).
            let quickdraw_rank = session.learned_skills.iter()
                .find(|s| s.skill_id == 1036)
                .map(|s| s.rank).unwrap_or(0);
            if !crit
                && wants_attack
                && quickdraw_rank > 0
                && matches!(shared::Proficiency::from_item(weapon_id), shared::Proficiency::Bow)
                && self.sim_time_s - session.last_movement_at_s >= 1.0
                && !session.quickdraw_consumed
            {
                crit = true;
                session.quickdraw_consumed = true;
            }
            let mut dmg_final = if crit {
                (session.stats.attack_damage as f32 * shared::CRIT_DAMAGE_MULT).round() as i32
            } else {
                session.stats.attack_damage
            };
            // (Riposte removida — skill 1001 agora e Leap Strike.)
            // Sword Dance (1005): se stance ativo, força crit no step final
            // (terceiro hit). Step 0/1 sao normais; step 2 vira crit + reset.
            if wants_attack
                && self.sim_time_s < session.sword_dance_until
                && shared::weapon_attack_anim(weapon_id) == shared::components::attack_anim::SLASH
            {
                if session.sword_dance_step >= 2 {
                    // Hit final — força crit + reset stance.
                    if !crit {
                        crit = true;
                        dmg_final = (session.stats.attack_damage as f32 * shared::CRIT_DAMAGE_MULT).round() as i32;
                    }
                    session.sword_dance_until = 0.0;
                    session.sword_dance_step = 0;
                } else {
                    session.sword_dance_step += 1;
                }
            }
            // Combo step: incrementa em SLASH (melee), THRUST (Wand/Staff)
            // e SHOOT (Bow). Para caster, o step troca o overlay de spell
            // visual + o ultimo step lança projetil (resto melee cone).
            // Para Bow, o ultimo step lança 3 flechas em leque (finisher
            // fluido) em vez de 1.
            // Reset se passou COMBO_RESET_TIME desde o último attack.
            let is_melee = shared::weapon_is_melee(weapon_id);
            let attack_anim_code = shared::weapon_attack_anim(weapon_id);
            let prof = shared::Proficiency::from_item(weapon_id);
            let is_caster_thrust = matches!(
                prof,
                shared::Proficiency::Wand | shared::Proficiency::Staff
            );
            let is_bow = matches!(prof, shared::Proficiency::Bow);
            let combo_eligible = wants_attack
                && (
                    (is_melee && attack_anim_code == shared::components::attack_anim::SLASH)
                    || is_caster_thrust
                    || is_bow
                );
            let mut combo_step: u8 = 0;
            if combo_eligible {
                if self.sim_time_s - session.combo_last_attack > shared::COMBO_RESET_TIME {
                    session.combo_step = 0;
                }
                combo_step = session.combo_step;
                session.combo_step = (session.combo_step + 1) % shared::COMBO_STEPS;
                session.combo_last_attack = self.sim_time_s;
            }
            // Knockback override: enquanto knockback_until > now, vel forcada
            // pra knockback_vel (ignora input do player).
            let final_vel = if session.knockback_until > self.sim_time_s {
                session.knockback_vel
            } else {
                dir * speed
            };
            input_results.push(InputResult {
                entity,
                new_vel: final_vel,
                wants_attack,
                owner_id: session.entity_id,
                target: session.target,
                // Mira do alvo quando o auto-ataque disparou; senao a do
                // frame, que ainda serve pra skill mirada.
                aim: attack_aim.unwrap_or(frame.aim),
                damage: dmg_final,
                is_crit: crit,
                is_melee,
                proj_kind,
                attack_anim_code,
                combo_step,
                dash_started: wants_dash,
            });
        }

        // Rota nova pra quem travou. Fora do laco porque o A* precisa da ilha
        // e o laco esta' com as sessoes emprestadas.
        //
        // `handle_mover_para` ja' tem o limite de um pedido por sessao a cada
        // 0,2 s, entao um corpo genuinamente preso pede rota cinco vezes por
        // segundo e nao trinta.
        for (sid, destino) in refazer_rota {
            self.handle_mover_para(sid, destino);
        }

        // Cleanup pos-loop: drop DelayedAoe pendente dos casters que cancelaram
        // por movimento, e broadcast SkillCastCancel pra clientes despawnarem
        // gizmos e parar coroutines de visual.
        if !cancelled_cast_owners.is_empty() {
            let cancelled_eids: std::collections::HashSet<EntityId> =
                cancelled_cast_owners.iter().map(|(eid, _)| *eid).collect();
            self.pending_delayed_aoe
                .retain(|d| !cancelled_eids.contains(&d.owner_eid));
            for (caster_eid, skill_id) in &cancelled_cast_owners {
                let msg = ServerMessage::SkillCastCancel {
                    caster_eid: *caster_eid,
                    skill_id: *skill_id,
                };
                for s in self.sessions.values() {
                    if s.logged_in {
                        let _ = s.handle.to_client.send(msg.clone());
                    }
                }
            }
        }

        // ── B: snapshot de posições de jogadores para IA dos inimigos ─────────
        // Coleta entidades untargetable pra que inimigos ignorem.
        let untargetable: std::collections::HashSet<Entity> = self
            .ecs
            .query::<&Untargetable>()
            .iter()
            .map(|(e, _)| e)
            .collect();
        // Players em zona segura são "invisíveis" pra IA — enemies dropam aggro
        // automaticamente quando o alvo entra (lista some daqui no próximo tick).
        // Players Mounted em barco tambem ficam invisiveis: barco anda na agua,
        // inimigos terrestres nao alcancam — sumem do radar pra evitar AI presa.
        // Indice por id pra IA ler a posicao do alvo em O(1) todo tick.
        let player_pos_by_id: HashMap<EntityId, Vec2>;
        let player_positions: Vec<(EntityId, Vec2)> = self
            .ecs
            .query::<(&NetId, &Position, &EntityKind)>()
            .iter()
            .filter_map(|(e, (net, pos, kind))| {
                if matches!(kind, EntityKind::Player)
                    && !untargetable.contains(&e)
                    && !self.in_safe_zone(pos.0)
                    && self.ecs.get::<&Mounted>(e).is_err()
                {
                    Some((net.0, pos.0))
                } else { None }
            })
            .collect();
        player_pos_by_id = player_positions.iter().copied().collect();

        // ── C: IA dos inimigos ────────────────────────────────────────────────
        struct SpawnProj { owner_id: EntityId, from_player: bool, pos: Vec2, dir: Vec2, damage: i32, is_crit: bool, kind: u8 }
        let mut projs_to_spawn: Vec<SpawnProj> = Vec::new();
        // Melee swings — usado por player attacks (sec D) e por enemies melee aqui (sec C).
        /// Um golpe a resolver neste tick.
        ///
        /// `target` Some = ataque POR ALVO: acerta so' aquela entidade, por
        /// distancia, sem cone e sem projetil. E' o modelo do jogo agora; o
        /// cone sobrou pros mobs, que ainda batem em area.
        struct MeleeSwing {
            attacker_eid: EntityId,
            pos: Vec2,
            dir: Vec2,
            damage: i32,
            is_crit: bool,
            from_player: bool,
            knockback: f32,
            target: Option<EntityId>,
            max_range: f32,
        }
        let mut melee_swings: Vec<MeleeSwing> = Vec::new();
        // Pending shots de enemies — coletados no loop de IA (que tem mut borrow do
        // ecs) e fundidos em self.pending_shots logo depois.
        let mut pending_enemy_shots: Vec<PendingShot> = Vec::new();
        // Skill cast intents — coletadas no AI loop (com snapshot do estado),
        // processadas DEPOIS do loop pra evitar borrow conflict com self.
        // Tuple: (enemy_eid, enemy_pos, target_pos).
        let mut enemy_cast_intents: Vec<(EntityId, Vec2, Vec2)> = Vec::new();
        let now_sim = self.sim_time_s;

        for (_, (net, pos, vel, enemy, kind)) in
            self.ecs.query_mut::<(&NetId, &Position, &mut Velocity, &mut EnemyTag, &EntityKind)>()
        {
            let kind_id = match kind {
                EntityKind::Enemy(k) => *k,
                _ => 0,
            };
            let def = crate::economy::enemy_def(kind_id);

            if enemy.attack_cooldown > 0.0 { enemy.attack_cooldown -= dt; }
            enemy.wander_timer -= dt;
            // Stamina regen passivo (igual player base): recupera fora de combate
            // e devagar dentro. Cap em stats.stamina_max. Drena via block de
            // projetil com escudo (20 por bloqueio).
            let stam_max = enemy.stats.stamina_max as f32;
            if enemy.stamina_current < stam_max {
                let regen = enemy.stats.stamina_regen.max(5.0); // floor 5/s
                enemy.stamina_current = (enemy.stamina_current + regen * dt).min(stam_max);
            }
            // Cadáver: vel=0, skipa IA, espera o despawn loop limpar.
            if enemy.dead {
                vel.0 = Vec2::ZERO;
                continue;
            }
            // Knockback: enquanto knockback_until > now, vel = knockback_vel.
            // Sobrepoe o stagger normal — alvo desliza pra tras antes de cair
            // em hurt_until parado. Walls/colliders param naturalmente.
            if enemy.knockback_until > now_sim {
                vel.0 = enemy.knockback_vel;
                continue;
            }
            // Stagger: durante hurt_until, vel=0 e skipa o resto da IA.
            // BOSS tem HYPERARMOR: não trava em stagger — senão spam de atk
            // stun-locka e ele NUNCA chega a esquivar/bloquear/responder.
            // Knockback e stun (Shield Bash) continuam valendo pra ele.
            if enemy.hurt_until > now_sim && !enemy.is_boss {
                vel.0 = Vec2::ZERO;
                continue;
            }
            // Stun (Shield Bash 1003): enemy parado, sem ataque/movimento.
            if enemy.stunned_until > now_sim {
                vel.0 = Vec2::ZERO;
                continue;
            }
            // Leap (boss, Leap Strike): NO AR — posição integrada no pass de
            // leap (igual player). IA não anda/ataca/decide durante o pulo.
            if enemy.leap_until > now_sim {
                vel.0 = Vec2::ZERO;
                continue;
            }
            // Spawn grace: enemy ainda em VFX de invocação no cliente. Não move
            // nem ataca — fica plantado no spawn anchor.
            if enemy.spawn_grace_until > now_sim {
                vel.0 = Vec2::ZERO;
                continue;
            }
            // aggro_timer só corre quando em chase ativo (gerenciado abaixo)

            // Taunt (1006): se forced_aggro ativo, mira EXCLUSIVAMENTE o
            // tauntador. Senao, o jogador mais perto.
            //
            // A ESCOLHA e' periodica (cara: varre todos os jogadores); a
            // POSICAO do escolhido e' lida todo tick por id (O(1)), pra o mob
            // nao perseguir um fantasma de 200ms atras.
            let decide = (self.tick + net.0.0) % AI_DECISAO_TICKS == 0;
            let forcado = if enemy.forced_aggro_until > now_sim {
                enemy.forced_aggro_target
            } else {
                None
            };
            if decide || enemy.ai_target.is_none() || forcado.is_some() {
                enemy.ai_target = forcado
                    .filter(|t| player_pos_by_id.contains_key(t))
                    .or_else(|| {
                        player_positions
                            .iter()
                            .min_by(|a, b| {
                                a.1.distance_squared(pos.0)
                                    .partial_cmp(&b.1.distance_squared(pos.0))
                                    .unwrap()
                            })
                            .map(|(eid, _)| *eid)
                    });
            }
            // Alvo que desconectou ou morreu sai do cache na hora.
            let nearest: Option<(EntityId, Vec2)> = enemy
                .ai_target
                .and_then(|eid| player_pos_by_id.get(&eid).map(|p| (eid, *p)));
            if nearest.is_none() {
                enemy.ai_target = None;
            }

            // Leash + state machine:
            //  1. Inside leash_max → wander/chase livre.
            //  2. Cruzou leash_hard (2× leash_max) → seta returning_home=true.
            //  3. returning_home: ignora players, vai pro anchor até ficar
            //     dentro de 50% de leash_max → returning_home=false (de-aggro
            //     completo). Isso quebra o loop chase↔pull na borda.
            let home_dist = if enemy.leash_max > 0.0 { pos.0.distance(enemy.spawn_anchor) } else { 0.0 };
            let leash_hard = enemy.leash_max * 2.0;

            if enemy.leash_max > 0.0 {
                if enemy.returning_home {
                    if home_dist <= enemy.leash_max * 0.5 {
                        enemy.returning_home = false;
                    }
                } else if home_dist > leash_hard {
                    enemy.returning_home = true;
                }
            }
            let pulling_home = enemy.returning_home;

            if let Some((_, ppos)) = nearest.as_ref() {
                let dist = pos.0.distance(*ppos);
                // Idle skip: player MUITO longe (alem do AOI) — congela mob,
                // pula raycast LoS e toda decisao. Mob fica parado, mas ninguem
                // ve (fora do AOI=24 tiles). Reduz drasticamente o custo de
                // IA quando ha mobs em zonas vizinhas com poucos players.
                const IDLE_SKIP_DIST: f32 = 50.0;
                if dist > IDLE_SKIP_DIST && !pulling_home {
                    vel.0 = Vec2::ZERO;
                    continue;
                }
                // Line of sight: enemy nao "ve" player atraves de WALL.
                // Player atras de parede = no aggro/chase. Player atras de
                // agua/decoracao continua visivel (so WALL bloqueia).
                // Conditional: dist >= detect_range ja desqualifica chase,
                // entao nao gasta o raycast nesses casos (caso comum).
                let has_los = dist < enemy.detect_range
                    && self.map.has_line_of_sight(pos.0, *ppos);
                // Chase só se NÃO estiver returning home E tiver visao.
                let can_chase = dist < enemy.detect_range && !pulling_home && has_los;
                if can_chase {
                    // Aggro timer: corre durante chase, reset ao acertar attack.
                    enemy.aggro_timer += dt;
                    const AGGRO_DROP_TIME: f32 = 5.0; // 5s sem dano = desiste
                    if enemy.aggro_timer > AGGRO_DROP_TIME && enemy.leash_max > 0.0 {
                        // Desistiu — volta pra casa.
                        enemy.returning_home = true;
                        enemy.aggro_timer = 0.0;
                    }
                    let to_player = (*ppos - pos.0).try_normalize().unwrap_or(Vec2::X);
                    let perp = Vec2::new(-to_player.y, to_player.x);
                    // ── BOSS AI "PvP-like": dash, esquiva, block, strafe ────
                    // Gated em is_boss — mobs comuns mantêm a IA barata.
                    if enemy.is_boss {
                        // Mid-dash: direção travada, velocidade alta, sem atacar.
                        if now_sim < enemy.ai_dash_until {
                            vel.0 = enemy.ai_dash_dir * 11.0;
                            continue;
                        }
                        // GUARD-WALK em andamento: avançando ATRÁS DA GUARDA
                        // (-75% dano, ver damage_events). CHEGOU em range de
                        // golpe → baixa a guarda e ataca NA HORA (cai pro fluxo
                        // de ataque deste mesmo tick). Senão segue marchando.
                        if now_sim < enemy.ai_block_until {
                            if dist <= enemy.attack_range {
                                enemy.ai_block_until = now_sim; // baixa a guarda
                                enemy.attack_cooldown = 0.0;    // golpe imediato
                            } else {
                                vel.0 = to_player * (enemy.locomotor_speed * 0.85);
                                continue;
                            }
                        }
                        // Decai o contador de hits recentes (alimenta block e
                        // esquiva REATIVOS). Decay LENTO (0.3/s): com decay 1.0
                        // o threshold de 1.5 exigia 2 hits em <0.5s — block era
                        // matematicamente impossível pra armas normais (~1.6s
                        // entre swings). Agora 2 hits em ≤~3s disparam a guarda.
                        enemy.ai_recent_hits = (enemy.ai_recent_hits - 0.3 * dt).max(0.0);
                        // (Dashes REMOVIDOS do boss — feedback: boss é lento e
                        // parrudo; a mobilidade dele é o Leap Strike. A defesa
                        // agora é a STAGGER BAR: hyperarmor até a barra quebrar
                        // → stun de punição → recarrega. Ver damage_events.)
                        // GUARD-WALK (escudeiro): só na APROXIMAÇÃO FINAL
                        //    (até 4.5 tiles) — ergue o escudo e marcha firme
                        //    (-75% dano, 0.85× speed) até o range de golpe,
                        //    aí baixa e ataca. Longe disso anda a velocidade
                        //    CHEIA (sem guarda) — kitar tem que custar chão,
                        //    e o Leap pune quem abre distância (cd próprio).
                        if dist > enemy.attack_range + 0.4 && dist < 4.5
                            && now_sim >= enemy.ai_block_cd_until {
                            enemy.ai_block_until = now_sim + 2.0;
                            enemy.ai_block_cd_until = now_sim + 4.0;
                            enemy.parry_flash_pending = true; // "ergueu o escudo"
                            vel.0 = to_player * (enemy.locomotor_speed * 0.85);
                            continue;
                        }
                    }
                    // Comportamento de movimento — kite_dist e speed cacheados
                    // no EnemyTag a partir do EnemyBuild (procedural por classe).
                    // Melee usa "stand_dist" = attack_range - 0.3 pra parar dentro
                    // do range sem colidir com o body do player (evita jitter
                    // ida-e-volta quando ambos os bodies se sobrepoem).
                    let move_dir = if let Some(kite) = enemy.kite_dist {
                        if dist > kite + 0.5      { to_player }
                        else if dist < kite - 0.5 { -to_player }
                        else                      { Vec2::ZERO }
                    } else {
                        let stand = (enemy.attack_range - 0.3).max(0.8);
                        if dist > stand + 0.3 {
                            // Boss aproxima em zigue (weave); mob comum em reta.
                            if enemy.is_boss {
                                (to_player + perp * enemy.ai_strafe_sign * 0.5)
                                    .try_normalize().unwrap_or(to_player)
                            } else { to_player }
                        }
                        else if dist < stand - 0.3 { -to_player }  // muito perto → afasta
                        else if enemy.is_boss {
                            // Em range esperando cooldown: strafe circular em
                            // volta do player (flipa o lado num timer).
                            if now_sim >= enemy.ai_strafe_flip_at {
                                enemy.ai_strafe_sign = -enemy.ai_strafe_sign;
                                enemy.ai_strafe_flip_at = now_sim + 0.9
                                    + lcg_f32(lcg(self.tick as u64 ^ net.0.0 as u64)) * 1.1;
                            }
                            perp * enemy.ai_strafe_sign * 0.85
                        }
                        else { Vec2::ZERO }                        // em range, parado
                    };
                    vel.0 = move_dir * enemy.locomotor_speed;

                    let attack_range = enemy.attack_range;
                    // Boss "engaja" de mais longe: tenta skills até 7 tiles
                    // (cada skill valida o próprio range) — luta com o kit
                    // inteiro em vez de só auto-attack colado.
                    let engage_range = if enemy.is_boss { attack_range.max(8.0) } else { attack_range };
                    let can_auto = enemy.attack_cooldown <= 0.0;
                    // BOSS: skills têm cadência PRÓPRIA (cada uma tem o seu cd)
                    // — não esperam o ciclo do auto-attack (2.85s). Sem isso o
                    // kit quase não saía e a luta ficava passiva: agora o Leap
                    // pune o kite NA HORA, Shield Bash sai assim que você cola.
                    if dist < engage_range && (can_auto || enemy.is_boss) {
                        // Tenta casting de skill ANTES de fall-through pra
                        // auto-attack. Skill eligibility check (sem borrow
                        // self): cd ready, mp ok, alvo dentro de range. No
                        // design novo (M13) todas as skills são universais
                        // — sem mais check de usable_with.
                        let mut chose_skill = false;
                        let mut chosen_skill_id = 0u32;
                        let weapon_id = enemy.equipment.weapon.unwrap_or(0);
                        let _prof = shared::Proficiency::from_item(weapon_id).as_db_str();
                        for ls in &enemy.learned_skills {
                            let Some(def_sk) = crate::skills::skill_of(ls.skill_id) else { continue };
                            if def_sk.is_passive { continue; }
                            if def_sk.cast_time_s > 0.05 { continue; }
                            let cd_ready = enemy.skill_cds.get(&ls.skill_id).copied().unwrap_or(0.0);
                            if now_sim < cd_ready { continue; }
                            let rank_factor = 1.0 - def_sk.per_rank_cost_pct
                                * (ls.rank.saturating_sub(1) as f32);
                            let mp_cost = (def_sk.cost_mp as f32 * rank_factor).max(0.0);
                            if enemy.mp_current < mp_cost { continue; }
                            if dist > def_sk.range_tiles + def_sk.radius_tiles { continue; }
                            // Skill apta — registra intent e pula auto-attack.
                            enemy_cast_intents.push((net.0, pos.0, *ppos));
                            chose_skill = true;
                            chosen_skill_id = ls.skill_id;
                            break;
                        }
                        if chose_skill {
                            enemy.aggro_timer = 0.0;
                            // Boss: GCD curto pós-skill (a anim respira mas o
                            // ritmo do auto não é resetado pro ciclo inteiro).
                            enemy.attack_cooldown = if enemy.is_boss { 0.8 }
                                else { enemy.attack_cooldown_base };
                            // Leap Strike (1001) NÃO toca swing — a anim é o
                            // próprio arco do pulo (leap_y), igual ao player.
                            enemy.attack_pending = chosen_skill_id != 1001;
                            enemy.attack_dir = to_player;
                            continue;
                        }
                        // Auto-attack só com o ciclo pronto e dentro do range
                        // REAL da arma (boss fora disso: re-tenta próximo tick).
                        if !can_auto || dist >= attack_range { continue; }
                        enemy.aggro_timer = 0.0; // reset ao atacar com sucesso
                        enemy.attack_cooldown = enemy.attack_cooldown_base;
                        enemy.attack_pending = true; // cliente toca anim
                        enemy.attack_dir = to_player;
                        if enemy.is_melee {
                            // Melee enemy: cone de dano direto na frente, sem projetil.
                            // O snapshot leva attack_pending pra cliente animar.
                            // Damage é aplicado via melee_swings junto com player swings.
                            melee_swings.push(MeleeSwing {
                                attacker_eid: net.0,
                                pos: pos.0,
                                dir: to_player,
                                damage: enemy.stats.attack_damage,
                                is_crit: false, // enemies não fazem crit hoje
                                from_player: false,
                                // ZERO, e nao um "shove leve".
                                //
                                // O knockback sobrescreve a velocidade do
                                // jogador (`Session.knockback_vel`), entao
                                // cada mordida move o personagem. Um lobo
                                // sozinho e' um cutucao; uma matilha e' o
                                // jogador sendo carregado pelo mapa sem
                                // conseguir andar pra onde quer.
                                //
                                // Isto e' o auto-attack. Knockback de SKILL
                                // (`SkillDef.knockback`) continua valendo:
                                // aquele e' efeito desenhado, este era
                                // incidental.
                                knockback: 0.0,
                                target: None,
                                max_range: shared::MELEE_RANGE,
                            });
                        } else {
                        // proj_kind e proj_count cacheados a partir do EnemyBuild.
                        let enemy_proj_kind: u8 = enemy.proj_kind;
                        let proj_count = enemy.proj_count;
                        // Mago = anim de swing curta (~340ms) → delay menor.
                        let fire_delay = if enemy_proj_kind == 1 {
                            shared::MAGIC_FIRE_DELAY
                        } else {
                            shared::BOW_FIRE_DELAY
                        };
                        let release_in_ticks = (fire_delay / dt).round() as u32;
                        // Mago: fireball sai da varinha (offset na direção to_player).
                        let forward = if enemy_proj_kind == 1 {
                            to_player * shared::FIREBALL_FORWARD_OFFSET
                        } else {
                            Vec2::ZERO
                        };
                        let spawn_pos = pos.0 + Vec2::new(0.0, shared::PROJ_SPAWN_OFFSET_Y) + forward;
                        let release_tick = self.tick.wrapping_add(release_in_ticks);
                        if proj_count <= 1 {
                            pending_enemy_shots.push(PendingShot {
                                owner_id: net.0,
                                from_player: false,
                                pos: spawn_pos,
                                dir: to_player,
                                damage: enemy.stats.attack_damage,
                                is_crit: false,
                                kind: enemy_proj_kind,
                                release_tick,
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
                                pending_enemy_shots.push(PendingShot {
                                    owner_id: net.0,
                                    from_player: false,
                                    pos: spawn_pos,
                                    dir,
                                    damage: enemy.stats.attack_damage,
                                    is_crit: false,
                                    kind: enemy_proj_kind,
                                    release_tick,
                                });
                            }
                        }
                        } // close else for is_melee
                    }
                } else {
                    // Sem chase (player fora de detect/LOS, OR returning_home).
                    // Se estava em chase ativo (aggro_timer > 0) E saiu do
                    // anchor, volta pra casa imediatamente em vez de wander
                    // random — evita inimigos longe do spawn parados/wandering.
                    let was_chasing = enemy.aggro_timer > 0.5;
                    let far_from_home = enemy.leash_max > 0.0
                        && pos.0.distance(enemy.spawn_anchor) > enemy.leash_max * 0.4;
                    if was_chasing && far_from_home {
                        enemy.returning_home = true;
                    }
                    enemy.aggro_timer = 0.0;
                    let pulling_home = enemy.returning_home;
                    apply_wander(enemy, &mut vel.0, pos.0, enemy.locomotor_speed,
                        &self.map, &self.safe_zones, self.tick, net.0.0, pulling_home);
                }
            } else {
                // Sem player algum — mesma logic, mas sem chase tracking.
                let far_from_home = enemy.leash_max > 0.0
                    && pos.0.distance(enemy.spawn_anchor) > enemy.leash_max * 0.4;
                let was_chasing = enemy.aggro_timer > 0.5;
                if was_chasing && far_from_home {
                    enemy.returning_home = true;
                }
                enemy.aggro_timer = 0.0;
                let pulling_home = enemy.returning_home;
                apply_wander(enemy, &mut vel.0, pos.0, enemy.locomotor_speed,
                    &self.map, &self.safe_zones, self.tick, net.0.0, pulling_home);
            }
        }

        // ── C.0b: Storm Caller passive (1056) — auto-cast lightning bolt ─────
        // Cada player com a passiva learned (rank > 0) dispara lightning bolt
        // no enemy mais proximo a cada N segundos. Interval reduz com rank.
        // Damage scaling com WIS (igual Lightning Bolt manual).
        {
            let now_s = self.sim_time_s;
            // Coleta intents (player_pos, target_pos, dmg, owner_eid).
            struct StormShot { pos: Vec2, target_pos: Vec2, dir: Vec2, damage: i32, owner_eid: EntityId }
            let mut shots: Vec<StormShot> = Vec::new();
            for session in self.sessions.values_mut() {
                if !session.logged_in || session.downed { continue; }
                let Some(ls) = session.learned_skills.iter().find(|s| s.skill_id == 1056).copied()
                    else { continue };
                if ls.rank == 0 { continue; }
                if now_s < session.storm_caller_next { continue; }
                // Cd minimo 5s pra evitar que rank 10 vire DPS automatico OP.
                let interval = match ls.rank {
                    1..=2 => 8.0,
                    3..=4 => 7.0,
                    5..=6 => 6.0,
                    _ => 5.0, // rank 7-10 = floor de 5s
                };
                session.storm_caller_next = now_s + interval;

                // Pega player pos via ECS.
                let Some(entity) = session.entity else { continue };
                let player_pos = match self.ecs.get::<&Position>(entity) {
                    Ok(p) => p.0, Err(_) => continue,
                };
                // Acha enemy mais proximo no raio 10 com LOS.
                let mut best: Option<(Vec2, f32)> = None;
                let r = 10.0_f32;
                for (_, (pos2, ek)) in self.ecs.query::<(&Position, &EntityKind)>().iter() {
                    if !matches!(ek, EntityKind::Enemy(_)) { continue; }
                    let d = pos2.0.distance(player_pos);
                    if d > r { continue; }
                    if !self.map.has_line_of_sight(player_pos, pos2.0) { continue; }
                    if best.map(|(_, bd)| d < bd).unwrap_or(true) {
                        best = Some((pos2.0, d));
                    }
                }
                let Some((target_pos, _)) = best else { continue };
                let dir = (target_pos - player_pos).normalize_or_zero();
                if dir == Vec2::ZERO { continue; }

                // Damage: base 15 + WIS scaling 0.4 + rank scaling.
                let base_dmg = 15;
                let wis_dmg = (session.stats.wis as f32 * 0.4) as i32;
                let rank_mult = 1.0 + (ls.rank as f32 - 1.0) * 0.05; // +5%/rank (anti-OP)
                let damage = ((base_dmg + wis_dmg) as f32 * rank_mult).round() as i32;

                shots.push(StormShot {
                    pos: player_pos + Vec2::new(0.0, shared::PROJ_SPAWN_OFFSET_Y),
                    target_pos, dir, damage, owner_eid: session.entity_id,
                });
            }
            for s in shots {
                self.pending_shots.push(PendingShot {
                    pos: s.pos,
                    dir: s.dir,
                    damage: s.damage,
                    is_crit: false,
                    kind: 2, // lightning
                    owner_id: s.owner_eid,
                    from_player: true,
                    release_tick: self.tick.wrapping_add(1),
                });
                tracing::info!("storm caller proc: dmg={} → ({:.1},{:.1})",
                    s.damage, s.target_pos.x, s.target_pos.y);
            }
        }

        // ── C.1: transitions de Untargetable + heal ao re-engajar ─────────────
        // Detecta enemies que entraram/saíram do estado returning_home pra
        // adicionar/remover Untargetable e curar HP ao chegar em casa.
        let mut entered_evade: Vec<Entity> = Vec::new();
        let mut exited_evade:  Vec<Entity> = Vec::new();
        for (e, (enemy, _kind)) in self.ecs.query::<(&EnemyTag, &EntityKind)>().iter() {
            let has_un = self.ecs.get::<&Untargetable>(e).is_ok();
            if enemy.returning_home && !has_un { entered_evade.push(e); }
            else if !enemy.returning_home && has_un { exited_evade.push(e); }
        }
        for e in entered_evade {
            let _ = self.ecs.insert_one(e, Untargetable);
        }
        for e in exited_evade {
            let _ = self.ecs.remove_one::<Untargetable>(e);
            // Heal ao chegar em casa (estilo WoW Classic).
            if let Ok(mut hp) = self.ecs.get::<&mut Health>(e) {
                hp.current = hp.max;
            }
            if let Ok(mut tag) = self.ecs.get::<&mut EnemyTag>(e) {
                tag.attack_cooldown = 0.0;
                tag.aggro_timer = 0.0;
            }
        }
        // Funde shots agendados pelos enemies neste tick.
        self.pending_shots.append(&mut pending_enemy_shots);

        // Processa skill cast intents dos enemies coletadas no AI loop.
        // try_enemy_cast_skill aplica damage/AoE, drena MP, seta cd,
        // broadcasta SkillCastFx pros clients renderizarem o VFX.
        for (eid, e_pos, target_pos) in enemy_cast_intents {
            let _ = self.try_enemy_cast_skill(eid, e_pos, target_pos);
        }

        // ── C.2: IA dos NPCs caminhantes (perambulação aleatória por raio) ────
        // Cada morador anda pra pontos aleatórios em volta da própria `home` (sem
        // rota compartilhada) → não empilham nem seguem o mesmo caminho.
        let now_npc = self.sim_time_s;
        let tick = self.tick;
        const NPC_SPEED: f32 = 1.2;     // mais lento que player (4.0)
        const NPC_ARRIVE_DIST: f32 = 0.35;
        const NPC_PAUSE_MIN: f32 = 1.5;
        const NPC_PAUSE_MAX: f32 = 5.0;
        const WANDER_RADIUS: f32 = 5.0;
        for (e, (pos, vel, wtag)) in self.ecs.query_mut::<(&Position, &mut Velocity, &mut WanderRouteTag)>() {
            // Pausa: parado até pause_until expirar (inclui pausa ao interagir).
            if wtag.pause_until > now_npc { vel.0 = Vec2::ZERO; continue; }
            let to = wtag.target - pos.0;
            let dist = to.length();
            if dist < NPC_ARRIVE_DIST {
                // Chegou — escolhe novo alvo aleatório perto da home + pausa.
                let seed = (e.id() as u64).wrapping_mul(0x9E37_79B9)
                    .wrapping_add((tick as u64).wrapping_mul(0x2545_F491));
                let ang = lcg_f32(lcg(seed)) * std::f32::consts::TAU;
                let rad = lcg_f32(lcg(seed ^ 0xABCD)).sqrt() * WANDER_RADIUS;
                wtag.target = wtag.home + Vec2::new(ang.cos(), ang.sin()) * rad;
                let pr = lcg_f32(lcg(seed ^ 0x1234_5678));
                wtag.pause_until = now_npc + NPC_PAUSE_MIN + pr * (NPC_PAUSE_MAX - NPC_PAUSE_MIN);
                vel.0 = Vec2::ZERO;
            } else {
                vel.0 = to / dist * NPC_SPEED;
            }
        }
        // Aplica integração da velocity no Position (NPCs não usam physics body
        // — colisão com paredes seria via leash dos waypoints; rota é responsabilidade
        // do designer ficar dentro do walkable).
        for (_, (pos, vel, _)) in self.ecs.query_mut::<(&mut Position, &Velocity, &WanderRouteTag)>() {
            pos.0 += vel.0 * dt;
        }

        // ── D: aplicar velocidades de jogadores + coletar ataques ────────────
        for ir in input_results {
            if let Ok(mut vel) = self.ecs.get::<&mut Velocity>(ir.entity) {
                vel.0 = ir.new_vel;
            }
            // Dash inicio: marca attack_anim_pending=DASH pra replicar a anim
            // de jump pra todos os clientes que enxergam esse player.
            if ir.dash_started {
                if let Ok(mut tag) = self.ecs.get::<&mut PlayerTag>(ir.entity) {
                    tag.attack_anim_pending = Some(shared::attack_anim::DASH);
                }
            }
            if ir.wants_attack {
                let pos = self.ecs.get::<&Position>(ir.entity).map(|p| p.0).unwrap_or(Vec2::ZERO);
                let dir = (ir.aim - pos).try_normalize().unwrap_or(Vec2::X);
                // Player em zona segura não pode atacar — silenciosamente ignora.
                if self.in_safe_zone(pos) {
                    tracing::debug!("attack blocked: player at ({:.1},{:.1}) in safe zone", pos.x, pos.y);
                    continue;
                }
                // Marca o tag pra que send_snapshots replique o trigger de
                // animacao pros outros clientes neste tick.
                if let Ok(mut tag) = self.ecs.get::<&mut PlayerTag>(ir.entity) {
                    tag.attack_anim_pending = Some(ir.attack_anim_code);
                    // SLASH e THRUST (caster) usam combo_step pra cyclar
                    // Slash1/Slash2/Thrust no cliente.
                    if ir.attack_anim_code == shared::components::attack_anim::SLASH
                        || ir.attack_anim_code == shared::components::attack_anim::THRUST
                    {
                        tag.combo_step_pending = Some(ir.combo_step);
                    }
                }
                if ir.is_melee {
                    // Combo melee: knockback escala com step. Finisher (step 2)
                    // empurra forte; Slash1/2 sao pouco mais que stagger.
                    let kb = match ir.combo_step {
                        0 => 0.3,
                        1 => 0.4,
                        _ => 1.4, // finisher
                    };
                    melee_swings.push(MeleeSwing {
                        attacker_eid: ir.owner_id, pos, dir, damage: ir.damage,
                        is_crit: ir.is_crit,
                        from_player: true,
                        knockback: kb,
                        target: ir.target,
                        max_range: shared::MELEE_RANGE,
                    });
                } else {
                    // Arma a distancia. O projetil-entidade morreu: ele existia
                    // pro combate de acao, onde a flecha podia errar. Com alvo,
                    // o acerto e' decidido por distancia no mesmo tick — o que
                    // tambem apaga N entidades ticando a 30Hz por tiro dado.
                    melee_swings.push(MeleeSwing {
                        attacker_eid: ir.owner_id,
                        pos,
                        dir,
                        damage: ir.damage,
                        is_crit: ir.is_crit,
                        from_player: true,
                        knockback: 0.0,
                        target: ir.target,
                        max_range: shared::RANGED_ATTACK_RANGE,
                    });
                }
            }
        }

        // ── E: spawnar projeteis ──────────────────────────────────────────────
        // Drena pending shots cujo release_tick chegou; injeta como spawns regulares.
        let now_tick = self.tick;
        let mut still_pending: Vec<PendingShot> = Vec::with_capacity(self.pending_shots.len());
        for ps in self.pending_shots.drain(..) {
            if now_tick.wrapping_sub(ps.release_tick) < u32::MAX / 2 {
                projs_to_spawn.push(SpawnProj {
                    owner_id: ps.owner_id,
                    from_player: ps.from_player,
                    pos: ps.pos,
                    dir: ps.dir,
                    damage: ps.damage,
                    is_crit: ps.is_crit,
                    kind: ps.kind,
                });
            } else {
                still_pending.push(ps);
            }
        }
        self.pending_shots = still_pending;

        for sp in projs_to_spawn {
            let proj_id = self.alloc_entity_id();
            // Eagle Eye passive (Bow T1, +5%/rank range): aplica bonus no TTL
            // do projetil quando arrow (kind=0 ou =6 power shot) lançada por
            // player com a passiva.
            let mut ttl = PROJ_TTL;
            if matches!(sp.kind, 0 | 6) && sp.from_player {
                let bonus = self.sessions.values()
                    .find(|s| s.entity_id == sp.owner_id)
                    .map(|s| s.stats.bow_range_bonus_pct)
                    .unwrap_or(0.0);
                if bonus > 0.0 {
                    ttl *= 1.0 + bonus;
                }
            }
            // Power Shot (kind=6): 2x velocidade. Outros tem speed normal.
            let speed_mult: f32 = if sp.kind == 6 { 2.0 } else { 1.0 };
            self.ecs.spawn((
                NetId(proj_id),
                Position(sp.pos),
                Velocity(sp.dir * PROJ_SPEED * speed_mult),
                EntityKind::Projectile,
                ProjTag {
                    owner: sp.owner_id,
                    from_player: sp.from_player,
                    ttl,
                    damage: sp.damage,
                    is_crit: sp.is_crit,
                    kind: sp.kind,
                },
            ));
        }

        // ── E.1: Leap Strike — interpola posicao + aplica AoE no fim ──────────
        // Player em leap_until > now: move suavemente de leap_start_pos pra
        // leap_target. No tick em que leap_until expira: snap final + aplica
        // damage + stun.
        let now_sim = self.sim_time_s;
        const LEAP_DURATION: f32 = 0.5;
        let mut leap_landings: Vec<(EntityId, Vec2, i32, f32)> = Vec::new();
        for s in self.sessions.values_mut() {
            if !s.logged_in || s.leap_until <= 0.0 { continue; }
            let leap_start = s.leap_until - LEAP_DURATION;
            let elapsed = now_sim - leap_start;
            if now_sim < s.leap_until {
                // Em curso: lerp da posicao.
                let t = (elapsed / LEAP_DURATION).clamp(0.0, 1.0);
                let pos = s.leap_start_pos.lerp(s.leap_target, t);
                if let Some(ent) = s.entity {
                    if let Ok(mut p) = self.ecs.get::<&mut Position>(ent) {
                        p.0 = pos;
                    }
                }
            } else {
                // Leap completou — snap final + agenda hit.
                let landing = s.leap_target;
                if let Some(ent) = s.entity {
                    if let Ok(mut p) = self.ecs.get::<&mut Position>(ent) {
                        p.0 = landing;
                    }
                }
                leap_landings.push((s.entity_id, landing, s.leap_damage, s.leap_radius));
                s.leap_until = 0.0;
                s.leap_damage = 0;
                s.leap_radius = 0.0;
            }
        }
        // Aplica AoE damage + stun nos enemies do landing point.
        for (owner_eid, landing, damage, radius) in leap_landings {
            // radius == 0 → climb-jump (pulo de escalada), nao a skill Leap
            // Strike. Sem AoE, sem stun e SEM a explosao de impacto (SkillCastFx).
            if radius <= 0.0 { continue; }
            let stun_dur = 1.5_f32;
            let enemies = self.find_enemies_in_radius(landing, radius);
            for tn in &enemies {
                let hd = calc_hurt_dir_from_eid(&self.ecs, *tn, landing);
                self.pending_skill_hits.push(PendingSkillHit {
                    target_net: *tn, damage, attacker_net: owner_eid,
                    hurt_dir: hd, is_crit: false, from_player: true,
                    knockback: 1.5, // Leap Strike landing — alvos voam
                });
            }
            let target_set: std::collections::HashSet<EntityId> =
                enemies.iter().copied().collect();
            for (_, (net, tag)) in self.ecs.query_mut::<(&NetId, &mut EnemyTag)>() {
                if target_set.contains(&net.0) {
                    let exp = now_sim + stun_dur;
                    if tag.stunned_until < exp { tag.stunned_until = exp; }
                }
            }
            tracing::info!("Leap Strike landed: {} enemies hit r{:.1} dmg={}",
                enemies.len(), radius, damage);
            // Broadcast SkillCastFx no LANDING — cliente spawna explosao agora.
            let fx = ServerMessage::SkillCastFx {
                skill_id: 1001, caster_pos: landing, target_pos: landing,
                target_eid: None, caster_eid: Some(owner_eid),
                chain_points: None,
            };
            for s in self.sessions.values() {
                if s.logged_in {
                    let _ = s.handle.to_client.send(fx.clone());
                }
            }
        }

        // Leap de ENEMIES (boss Leap Strike) — mesma mecânica do player:
        // lerp da posição durante o arco; no pouso, AoE nos PLAYERS no raio.
        let mut enemy_leap_landings: Vec<(EntityId, Vec2, i32, f32)> = Vec::new();
        for (_, (net, tag, pos, ph)) in self.ecs
            .query_mut::<(&NetId, &mut EnemyTag, &mut Position, &shared::Solido)>()
        {
            if tag.leap_until <= 0.0 { continue; }
            let leap_start = tag.leap_until - LEAP_DURATION;
            let elapsed = now_sim - leap_start;
            if now_sim < tag.leap_until {
                let t = (elapsed / LEAP_DURATION).clamp(0.0, 1.0);
                let p = tag.leap_start_pos.lerp(tag.leap_target, t);
                pos.0 = p;
            } else {
                let landing = tag.leap_target;
                pos.0 = landing;
                enemy_leap_landings.push((net.0, landing, tag.leap_damage, tag.leap_radius));
                tag.leap_until = 0.0;
                tag.leap_damage = 0;
                tag.leap_radius = 0.0;
            }
        }
        // AoE do pouso do boss: dano + knockback nos players (sem stun — o
        // enemy-cast não aplica status em players, consistente com o resto).
        for (owner_eid, landing, damage, radius) in enemy_leap_landings {
            if radius <= 0.0 { continue; }
            let players = self.find_players_in_radius(landing, radius);
            for pn in &players {
                let hd = calc_hurt_dir_from_eid(&self.ecs, *pn, landing);
                self.pending_skill_hits.push(PendingSkillHit {
                    target_net: *pn, damage, attacker_net: owner_eid,
                    hurt_dir: hd, is_crit: false, from_player: false,
                    knockback: 1.5,
                });
            }
            // Explosão de impacto no pouso (mesmo FX do player).
            let fx = ServerMessage::SkillCastFx {
                skill_id: 1001, caster_pos: landing, target_pos: landing,
                target_eid: None, caster_eid: Some(owner_eid),
                chain_points: None,
            };
            for s in self.sessions.values() {
                if s.logged_in { let _ = s.handle.to_client.send(fx.clone()); }
            }
        }

        // ── F: integrar movimento e resolver colisao ──────────────────────
        //
        // Duas etapas, no lugar do motor de fisica: `move_and_slide` empurra o
        // circulo contra a grade de tiles (deslizando na parede em vez de
        // grudar), e depois um passe afasta quem ficou sobreposto. Ver
        // `shared::physics`.
        // A terceira coluna e' a MOBILIDADE: 0 = nao e' empurrado. Jogador
        // entra com zero porque uma horda de vinte lobos, cada um cedendo
        // metade, carrega o personagem pelo mapa — e quem joga sente que
        // perdeu o controle do boneco. Ele empurra os mobs; eles nao a ele.
        // Quem esta' no ar, e QUE ALTURA o arco ja' alcancou.
        //
        // O degrau que o corpo aceita e' o que a altura do pulo cobre NESTE
        // instante — nao o degrau maximo durante o pulo inteiro. E' a
        // diferenca entre subir e colar: liberando tres blocos desde o
        // primeiro quadro, o corpo se transporta pro piso de cima assim que
        // encosta nele, sem ter subido. Agora ele sobe, e o barranco alto so'
        // e' vencido perto do pico.
        let no_ar: std::collections::HashMap<EntityId, i32> = self
            .sessions
            .values()
            .filter(|s| s.pulo_ate > self.sim_time_s)
            .map(|s| {
                let t = shared::PULO_DURACAO - (s.pulo_ate - self.sim_time_s);
                let alcanca = (shared::altura_do_pulo(t) / shared::terreno::BLOCO).floor() as i32;
                (
                    s.entity_id,
                    alcanca.clamp(
                        shared::terreno::DEGRAU_BLOCOS,
                        shared::terreno::PULO_BLOCOS,
                    ),
                )
            })
            .collect();
        let mut corpos: Vec<(Entity, Vec2, f32)> = Vec::new();
        for (e, (pos, vel, _)) in self
            .ecs
            .query::<(&Position, &Velocity, &shared::Solido)>()
            .iter()
        {
            let mobilidade = if self.ecs.get::<&PlayerTag>(e).is_ok() { 0.0 } else { 1.0 };
            let degrau = self
                .ecs
                .get::<&NetId>(e)
                .ok()
                .and_then(|n| no_ar.get(&n.0).copied())
                .unwrap_or(shared::terreno::DEGRAU_BLOCOS);
            // Numa ilha a parede nao e' tile, e' desnivel: quem barra e' a
            // regra de degrau contra o campo de altura.
            corpos.push((
                e,
                match &self.ilha {
                    Some(i) => i.mover_com_degrau(pos.0, vel.0, dt, ENTITY_RADIUS, degrau),
                    None => self.map.move_and_slide(pos.0, vel.0, dt, ENTITY_RADIUS),
                },
                mobilidade,
            ));
        }
        let mut circulos: Vec<(Vec2, f32, f32)> =
            corpos.iter().map(|(_, p, m)| (*p, ENTITY_RADIUS, *m)).collect();
        shared::physics::separar(&mut circulos);
        for ((e, _, _), (p, _, _)) in corpos.iter().zip(circulos.iter()) {
            if let Ok(mut pos) = self.ecs.get::<&mut Position>(*e) {
                pos.0 = *p;
            }
        }

        // ── F.1: barcos (Sea-of-Thieves) — fisica autonoma + walkable deck ─
        //
        // Modelo:
        //   1. Helm input do player na estacao HELM vira `helm_input` no boat.
        //   2. Yaw rate = helm_input * MAX_YAW * (|vel| / MAX_SPEED). Leme
        //      so funciona com movimento — barco parado nao gira.
        //   3. Sail thrust = sail_factor * wind_intensity * alignment.
        //      Alignment = max(0, dot(wind_local, sail_normal_local)).
        //   4. Drag d'agua decai vel; ancora dropada multiplica drag.
        //   5. Players montados livres no deck integram local_pos por
        //      input.move_dir; players em estacao ficam fixos na local
        //      pos da estacao.
        //   6. World pos do player = boat.pos + rotate(local_pos, boat.yaw).
        //
        // Recebe `mounted_inputs: HashMap<Entity, MountedInput>` com
        // {move_dir, aim, buttons} de cada session deste tick.
        let wind = self.wind;

        // (helm_input via move_dir foi removido — agora o leme usa
        // rudder_angle persistido em BoatTag, controlado por HelmAdjust msgs)

        // ── F.1.a: Boat physics + helm input pickup ────────────────────────
        let mut boat_world_state: HashMap<Entity, (Vec2, f32)> = HashMap::new(); // boat_e → (pos, yaw)
        for (boat_e, (pos, vel, tag)) in self
            .ecs
            .query::<(&mut Position, &mut Velocity, &mut BoatTag)>()
            .iter()
        {
            // (rudder_angle ja persistido em tag — atualizado por HelmAdjust)

            // 2. Atualiza anchor_progress (anim de drop/raise).
            let anchor_step = dt / shared::constants::BOAT_ANCHOR_ANIM_TIME;
            if tag.anchor_dropped {
                tag.anchor_progress = (tag.anchor_progress + anchor_step).min(1.0);
            } else {
                tag.anchor_progress = (tag.anchor_progress - anchor_step).max(0.0);
            }

            // 3. Calcula thrust da vela.
            let sail_factor = match tag.sail_position {
                0 => 0.0, 1 => 0.5, _ => 1.0,
            };
            let forward = boat_forward(tag.yaw);
            let target_speed = if sail_factor > 0.0 && wind.intensity > 0.0 {
                // Vento no frame do barco. Convencao Y: direction=0 → vento
                // pra +Y (norte). wind_w = (-sin, cos) (mesmo formato de
                // boat_forward). Mantem coerencia com visual da seta de vento.
                let wind_w = Vec2::new(-wind.direction.sin(), wind.direction.cos());
                let wind_b = rotate_vec(wind_w, -tag.yaw);
                // Normal da vela no frame do barco. sail_angle=0 → vela
                // perpendicular ao casco (catch wind from behind = -Y local).
                // Convencao Y: forward = +Y, normal aponta forward quando angle=0.
                let sail_normal_b = Vec2::new(-tag.sail_angle.sin(), tag.sail_angle.cos());
                let raw_alignment = wind_b.dot(sail_normal_b).max(0.0);
                // Vento como BOOST, nao multiplicador. Barco com vela up sempre
                // tem BASE (55%); vento contribui ate +BOOST (45%) extra quando
                // a-favor com intensidade total. Contra-vento = base only.
                let wind_term = shared::constants::BOAT_WIND_BOOST
                    * wind.intensity * raw_alignment;
                let factor = shared::constants::BOAT_SAIL_BASE + wind_term;
                sail_factor * factor * shared::constants::BOAT_MAX_SPEED
            } else { 0.0 };
            let target_vel = forward * target_speed;

            // 4. Lerp vel → target (inercia do barco).
            let lerp_k = 0.6; // s^-1; barco com inercia bem alta — accel/decel lentos
            let lerp_t = (lerp_k * dt).min(1.0);
            vel.0 = vel.0 + (target_vel - vel.0) * lerp_t;

            // 5. Drag d'agua + ancora.
            let mut drag = shared::constants::BOAT_WATER_DRAG;
            if tag.anchor_progress > 0.0 {
                drag += shared::constants::BOAT_WATER_DRAG
                    * shared::constants::BOAT_ANCHOR_DRAG_MULT
                    * tag.anchor_progress;
            }
            let drag_factor = (1.0 - drag * dt).max(0.0);
            vel.0 *= drag_factor;

            // 6. Yaw rate proporcional ao rudder_angle (persistido).
            // Cap em ±BOAT_MAX_YAW_RATE quando rudder = ±MAX_RUDDER_ANGLE.
            let speed = vel.0.length();
            let rudder_t = (tag.rudder_angle / shared::constants::BOAT_MAX_RUDDER_ANGLE)
                .clamp(-1.0, 1.0);
            let yaw_rate = rudder_t * shared::constants::BOAT_MAX_YAW_RATE;
            tag.ang_vel = yaw_rate;
            let old_yaw = tag.yaw;
            tag.yaw += yaw_rate * dt;
            // Normaliza yaw em [-PI, PI]
            if tag.yaw > std::f32::consts::PI { tag.yaw -= std::f32::consts::TAU; }
            if tag.yaw < -std::f32::consts::PI { tag.yaw += std::f32::consts::TAU; }
            // Repel: se a rotacao jogou o casco em terra, empurra o barco
            // pra fora ate caber. Se nem com push couber, reverte yaw.
            let cfg_rot = crate::boat_config::get(tag.kind);
            if !cfg_rot.is_hull_navigable(pos.0, tag.yaw, &self.map) {
                let repelled = cfg_rot.repel_from_land(pos.0, tag.yaw, &self.map);
                if cfg_rot.is_hull_navigable(repelled, tag.yaw, &self.map) {
                    pos.0 = repelled;
                } else {
                    tag.yaw = old_yaw;
                }
            }

            // 7. Integra posicao com slide axis-aligned + hull collision.
            // Checa TODOS os vertices do casco (deck_polygon rotacionado por
            // yaw) contra is_navigable — assim o barco para na margem da
            // ilha em vez de afundar metade da quilha em terra.
            let cfg = crate::boat_config::get(tag.kind);
            let mut new_pos = pos.0;
            let try_x = Vec2::new(pos.0.x + vel.0.x * dt, pos.0.y);
            if cfg.is_hull_navigable(try_x, tag.yaw, &self.map) {
                new_pos.x = try_x.x;
            } else {
                vel.0.x = 0.0;
            }
            let try_y = Vec2::new(new_pos.x, new_pos.y + vel.0.y * dt);
            if cfg.is_hull_navigable(try_y, tag.yaw, &self.map) {
                new_pos.y = try_y.y;
            } else {
                vel.0.y = 0.0;
            }
            pos.0 = new_pos;

            // 8. Atualiza dir/anim derivados (compat com BoatRenderer 2D).
            tag.dir = dir8_from_vec(forward);
            tag.anim = if speed > 0.05 { 1 } else { 0 };

            boat_world_state.insert(boat_e, (pos.0, tag.yaw));
            let _ = boat_e;
        }

        // ── F.1.b: Walkable deck — integra local_pos dos players montados ─
        // Coletamos primeiro pra evitar conflito de borrows.
        let mounted_player_data: Vec<(Entity, Entity, Vec2, Option<u8>, u16, Vec2)> = self.ecs
            .query::<(&Mounted,)>()
            .iter()
            .filter_map(|(player_e, (m,))| {
                // Usa last_move_dir persistido (sobrevive a ticks sem input
                // novo do client — evita flicker da walk anim).
                let kind = self.ecs.get::<&BoatTag>(m.boat_entity).ok().map(|t| t.kind).unwrap_or(0);
                Some((player_e, m.boat_entity, m.local_pos, m.station, kind, m.last_move_dir))
            })
            .collect();

        for (player_e, boat_e, mut local_pos, station, kind, move_input) in mounted_player_data {
            // Estacao ocupada → snap na pos da estacao, sem walking.
            if let Some(st) = station {
                local_pos = match st {
                    s if s == shared::constants::station::HELM   => boat_helm_local(kind),
                    s if s == shared::constants::station::SAIL   => boat_sail_local(kind),
                    s if s == shared::constants::station::ANCHOR => boat_anchor_local(kind),
                    _ => local_pos,
                };
            } else {
                // Livre — anda no deck baseado no input. `move_input` vem do
                // cliente em WORLD frame (W = norte). Converte pra LOCAL frame
                // do barco rotando por -yaw — assim WASD sempre eh intuitivo
                // (W = norte do mundo, independente da orientacao do barco).
                if move_input.length_squared() > 0.001 {
                    let world_step = move_input.normalize_or_zero() * BOAT_DECK_WALK_SPEED * dt;
                    let yaw = self.ecs.get::<&BoatTag>(boat_e).map(|t| t.yaw).unwrap_or(0.0);
                    let local_step = rotate_vec(world_step, -yaw);
                    local_pos += local_step;
                }
                // Clamp pra dentro do deck. Usa polygon se config tem
                // (deck_polygon do prefab), senao bbox.
                local_pos = crate::boat_config::get(kind).clamp_to_deck(local_pos);
            }
            // Velocidade do player em WORLD frame, pro client triggar walk anim.
            // Quando station = Some, parado (vel ZERO). Quando livre, vel =
            // move_input * walk_speed (worldframe ja).
            let player_world_vel = if station.is_some() {
                Vec2::ZERO
            } else if move_input.length_squared() > 0.001 {
                move_input.normalize_or_zero() * BOAT_DECK_WALK_SPEED
            } else {
                Vec2::ZERO
            };
            if let Ok(mut v) = self.ecs.get::<&mut Velocity>(player_e) {
                v.0 = player_world_vel;
            }

            // Salva local_pos no Mounted.
            if let Ok(mut m) = self.ecs.get::<&mut Mounted>(player_e) {
                m.local_pos = local_pos;
            }

            // World pos = boat.pos + rotate(local_pos, boat.yaw)
            if let Some((bpos, byaw)) = boat_world_state.get(&boat_e).copied() {
                let world_offset = rotate_vec(local_pos, byaw);
                let world_pos = bpos + world_offset;
                if let Ok(mut p) = self.ecs.get::<&mut Position>(player_e) {
                    p.0 = world_pos;
                }
            }
        }

        // Sincroniza posicao do carregado com a do carregador.
        let carry_pairs: Vec<(EntityId, EntityId)> = self.sessions.values()
            .filter_map(|s| s.carrying.map(|t| (s.entity_id, t)))
            .collect();
        for (carrier_eid, target_eid) in carry_pairs {
            let carrier_pos = self.sessions.values()
                .find(|s| s.entity_id == carrier_eid)
                .and_then(|s| s.entity)
                .and_then(|e| self.ecs.get::<&Position>(e).ok().map(|p| p.0));
            let target_entity = self.sessions.values()
                .find(|s| s.entity_id == target_eid)
                .and_then(|s| s.entity);
            if let (Some(cp), Some(te)) = (carrier_pos, target_entity) {
                if let Ok(mut pos) = self.ecs.get::<&mut Position>(te) {
                    pos.0 = cp;
                }
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
        // Inclui Velocity pra calcular hurt_dir = -vel (direção OPOSTA ao voo
        // = TOWARD atacante). Sem offset Y artificial do spawn no peito.
        let projs: Vec<(Entity, EntityId, Vec2, Vec2, EntityId, bool, i32, bool, u8)> = self
            .ecs
            .query::<(&NetId, &Position, &Velocity, &ProjTag)>()
            .iter()
            .map(|(e, (net, pos, vel, p))| (e, net.0, pos.0, vel.0, p.owner, p.from_player, p.damage, p.is_crit, p.kind))
            .collect();

        // I-frames de dash: players dashando ficam imunes a dano (mesmo
        // criterio de safe zone). Set lookup por net_id pra O(1).
        let dashing_players: std::collections::HashSet<EntityId> = self.sessions
            .values()
            .filter(|s| s.logged_in && self.sim_time_s < s.dash_until)
            .map(|s| s.entity_id)
            .collect();

        // (entity, net_id, pos, is_player, size_scale) — size_scale escala
        // hit_radius e Y-offset por target pra que enemies grandes (boss
        // 2.2×) tenham hitbox proporcional ao corpo visivel.
        let targets: Vec<(Entity, EntityId, Vec2, bool, f32)> = self
            .ecs
            .query::<(&NetId, &Position, &EntityKind, Option<&EnemyTag>)>()
            .iter()
            .filter_map(|(e, (net, pos, kind, tag))| match kind {
                // Player em zona segura é imune a dano. Dashing tambem.
                EntityKind::Player if !self.in_safe_zone(pos.0) && !dashing_players.contains(&net.0)
                    => Some((e, net.0, pos.0, true, 1.0_f32)),
                // Cadáveres e enemies em spawn-grace (invocação) não tomam dano.
                // Enemies em zona segura também são imunes (caso entrem por bug).
                EntityKind::Enemy(k) if tag.map(|t| !t.dead && t.spawn_grace_until <= now_sim).unwrap_or(true)
                                     && !self.in_safe_zone(pos.0)
                    => Some((e, net.0, pos.0, false, crate::economy::enemy_size_scale(*k))),
                _ => None,
            })
            .collect();

        // Hitbox base (~0.6 raio centrado no peito), escalado por size_scale
        // do alvo dentro dos loops abaixo — boss 2.2× tem hitbox 2.2× maior.
        let hit_target_radius = shared::HIT_TARGET_RADIUS;
        let hit_target_y_off  = shared::HIT_TARGET_Y_OFFSET;
        let mut hit_projs: Vec<(Entity, EntityId)> = Vec::new();
        // Tipo de ataque — usado pelo parry handler pra decidir se faz
        // contra-ataque melee ou refletir projectile.
        #[derive(Clone, Copy)]
        enum AttackInfo {
            /// Melee swing — `attacker_pos` pra apontar contra-ataque do parryador.
            Melee { attacker_pos: Vec2 },
            /// Projectile — `vel` pra spawn de proj refletido (-vel) + `kind` visual.
            Projectile { vel: Vec2, kind: u8 },
            /// Skill hit (line/aoe) — sem refletir (so anula damage no parry).
            Skill,
        }
        // damage: (target_entity, target_net_id, dmg, attacker_net_id,
        //         attacker_is_player, hurt_dir TOWARD attacker, is_crit, info,
        //         knockback_tiles)
        let mut damage_events: Vec<(Entity, EntityId, i32, EntityId, bool, Vec2, bool, AttackInfo, f32)> = Vec::new();

        let combat_disabled = self.safe_zone;

        // Helper: direção unitária do alvo TOWARD o ponto de origem do hit.
        // Reusada por melee (sw.pos) e projétil (proj.pos). Knockback futuro:
        // empurra o alvo na direção -hurt_dir.
        fn calc_hurt_dir(target_pos: Vec2, attacker_pos: Vec2) -> Vec2 {
            (attacker_pos - target_pos).try_normalize().unwrap_or(Vec2::ZERO)
        }

        // Aplica golpes melee: cada swing acerta inimigos em cone na frente.
        // Range estendido por hit_target_radius escalado pelo size do alvo.
        if !combat_disabled {
            let cos_half = shared::MELEE_CONE_HALF_ANGLE.cos();
            for sw in &melee_swings {
                for (te, tnet, tpos, is_player, size) in &targets {
                    // PvP gating: player→player so' se can_damage_player.
                    // player→enemy e enemy→player sempre permitidos.
                    if sw.from_player && *is_player {
                        if !self.can_damage_player(sw.attacker_eid, *tnet) { continue; }
                    } else if !sw.from_player && !*is_player {
                        // enemy→enemy: skip (sem friendly fire entre mobs)
                        continue;
                    }
                    if *tnet == sw.attacker_eid { continue; }
                    // Ataque POR ALVO: acerta so' quem o jogador marcou. Sem
                    // cone, sem area — o cone abaixo so' vale pros mobs.
                    if let Some(alvo) = sw.target {
                        if *tnet != alvo { continue; }
                    }
                    // Escala hitbox pelo tamanho do alvo (boss=2.2× → hitbox 2.2×).
                    let r       = hit_target_radius * size;
                    let y_off   = hit_target_y_off  * size;
                    let melee_max = sw.max_range + r;
                    let range_sq = melee_max * melee_max;
                    // Hit-point do alvo: peito (Y+offset), não os pés.
                    let target_hit = *tpos + Vec2::new(0.0, y_off);
                    let delta = target_hit - sw.pos;
                    let d2 = delta.length_squared();
                    if d2 > range_sq { continue; }
                    // Cone: so' quando NAO ha alvo (mob batendo em area).
                    if sw.target.is_none() {
                        if let Some(nd) = delta.try_normalize() {
                            if sw.dir.dot(nd) < cos_half { continue; }
                        }
                    }
                    // LOS: melee/cone nao atravessa WALL.
                    if !self.map.has_line_of_sight(sw.pos, target_hit) { continue; }
                    let hd = calc_hurt_dir(*tpos, sw.pos);
                    damage_events.push((*te, *tnet, sw.damage, sw.attacker_eid, sw.from_player, hd, sw.is_crit,
                        AttackInfo::Melee { attacker_pos: sw.pos }, sw.knockback));
                }
            }
        }

        // Eventos de impacto: (target_eid, dir, kind) — broadcastados depois
        // do loop pra evitar dupla mut borrow de self.sessions.
        let mut projectile_impacts: Vec<(EntityId, Vec2, u8)> = Vec::new();
        'outer: for (pe, pnet, ppos, pvel, powner, pfrom_player, pdmg, pcrit, pkind) in &projs {
            for (te, tnet, tpos, is_player, size) in &targets {
                if tnet == powner { continue; }
                // PvP gating: player→player so' se can_damage_player.
                if *pfrom_player && *is_player {
                    if !self.can_damage_player(*powner, *tnet) { continue; }
                } else if !*pfrom_player && !*is_player {
                    // enemy proj vs enemy: skip (sem friendly fire).
                    continue;
                }
                let r     = hit_target_radius * size;
                let y_off = hit_target_y_off  * size;
                let dist_sq = (r + PROJ_RADIUS) * (r + PROJ_RADIUS);
                let target_hit = *tpos + Vec2::new(0.0, y_off);
                if ppos.distance_squared(target_hit) < dist_sq {
                    // BLOCK: enemy com escudo + stamina absorve projetil sem
                    // tomar dano. Drena 20 stamina por bloqueio. Sem stamina,
                    // recebe dano normal. So' aplica em projeteis vindos do
                    // player (proj de enemy nao bloqueia entre enemies).
                    let mut blocked_by_shield = false;
                    if !*is_player && *pfrom_player {
                        if let Ok(tag) = self.ecs.get::<&EnemyTag>(*te) {
                            let oh = tag.equipment.offhand.unwrap_or(0);
                            let has_shield = oh == shared::item_id::SHIELD
                                          || oh == shared::item_id::HEAVY_SHIELD;
                            if has_shield && tag.stamina_current >= 20.0 {
                                blocked_by_shield = true;
                            }
                        }
                    }
                    if blocked_by_shield {
                        if let Ok(mut tag) = self.ecs.get::<&mut EnemyTag>(*te) {
                            tag.stamina_current = (tag.stamina_current - 20.0).max(0.0);
                            // Trigger defending pose curta — usa hurt_until com
                            // duracao breve pra cliente exibir parry/block flash.
                            tag.hurt_until = self.sim_time_s + 0.3;
                            tag.hurt_dir = (-*pvel).try_normalize().unwrap_or(Vec2::ZERO);
                        }
                        hit_projs.push((*pe, *pnet));
                        continue 'outer;
                    }
                    if !combat_disabled {
                        // Hurt_dir = -vel (TOWARD atacante). Evita Y artificial
                        // do spawn no peito que distorce a direção pro Norte.
                        // Fallback pra calc_hurt_dir se vel ≈ 0 (não deveria).
                        let hd = (-*pvel).try_normalize()
                            .unwrap_or_else(|| calc_hurt_dir(*tpos, *ppos));
                        // Knockback projetil: pequeno (0.3) pra arrows, medio (0.5)
                        // pra fireball/lightning, ja eh modesto pra nao virar boomerang.
                        let proj_kb = match *pkind {
                            0 | 6 => 0.3, // arrow / power shot arrow
                            1 | 5 => 0.5, // fireball / electric
                            _     => 0.4,
                        };
                        damage_events.push((*te, *tnet, *pdmg, *powner, *pfrom_player, hd, *pcrit,
                            AttackInfo::Projectile { vel: *pvel, kind: *pkind }, proj_kb));
                    }
                    // Arrow (kind=0 ou =6 power shot) e Spear (kind=7):
                    // broadcast ProjectileImpact pra cliente spawnar visual
                    // do projetil cravado no alvo. Outros projeteis (fireball
                    // /lightning) tem VFX no OnDestroy.
                    if matches!(*pkind, 0 | 6 | 7) {
                        let pdir = pvel.try_normalize().unwrap_or(Vec2::X);
                        projectile_impacts.push((*tnet, pdir, *pkind));
                    }
                    // Spear Throw harpoon (kind=7): seta caster.harpoon state
                    // pra permitir recast pull. Caster eh achado pelo owner
                    // entity_id. 5s de janela; se alvo morrer antes a checagem
                    // no recast retorna None e cai em first-cast novamente.
                    if *pkind == 7 {
                        for s in self.sessions.values_mut() {
                            if s.logged_in && s.entity_id == *powner {
                                s.harpoon_target_eid = Some(*tnet);
                                s.harpoon_until = self.sim_time_s + 5.0;
                                tracing::info!(
                                    "Spear Throw HIT: harpoon attached owner_eid={:?} target_eid={:?}",
                                    powner, tnet);
                                break;
                            }
                        }
                    }
                    hit_projs.push((*pe, *pnet));
                    continue 'outer;
                }
            }
        }
        // Broadcast ProjectileImpact pra todos clientes logados.
        for (target_eid, dir, kind) in &projectile_impacts {
            let msg = ServerMessage::ProjectileImpact {
                target_eid: *target_eid,
                dir: *dir,
                kind: *kind,
            };
            for s in self.sessions.values() {
                if s.logged_in {
                    let _ = s.handle.to_client.send(msg.clone());
                }
            }
        }

        // Drena delayed_aoe cujo release_tick chegou — converte em pending_skill_hits.
        if !combat_disabled && !self.pending_delayed_aoe.is_empty() {
            let now_tick = self.tick;
            // Particiona em ready (executa agora) e ainda-aguardando.
            let queue = std::mem::take(&mut self.pending_delayed_aoe);
            let mut still: Vec<DelayedAoe> = Vec::with_capacity(queue.len());
            let mut to_fire: Vec<DelayedAoe> = Vec::new();
            for d in queue {
                let ready = d.release_tick.wrapping_sub(now_tick) > u32::MAX / 2
                    || now_tick >= d.release_tick;
                if ready { to_fire.push(d); } else { still.push(d); }
            }
            self.pending_delayed_aoe = still;
            for d in to_fire {
                // follow_owner: looka up pos atual do owner pra Whirlwind/
                // Sword Dance acompanharem o player enquanto ele anda.
                let center = if d.follow_owner {
                    let mut found = d.target_pos;
                    for (_, (net, p)) in self.ecs.query::<(&NetId, &Position)>().iter() {
                        if net.0 == d.owner_eid { found = p.0; break; }
                    }
                    found
                } else {
                    d.target_pos
                };
                let enemies = self.find_enemies_in_radius(center, d.radius);
                let now_s = self.sim_time_s;
                for tn in &enemies {
                    let hd = calc_hurt_dir_from_eid(&self.ecs, *tn, center);
                    self.pending_skill_hits.push(PendingSkillHit {
                        target_net: *tn, damage: d.damage, attacker_net: d.owner_eid,
                        hurt_dir: hd, is_crit: false, from_player: true,
                        knockback: d.knockback,
                    });
                }
                // PvP: delayed AOE (Meteor/Rain/Smoke) tambem hita outros
                // players elegiveis.
                let pvp_players = self.find_pvp_players_in_radius(center, d.radius, d.owner_eid);
                for tn in &pvp_players {
                    let hd = calc_hurt_dir_from_eid(&self.ecs, *tn, center);
                    self.pending_skill_hits.push(PendingSkillHit {
                        target_net: *tn, damage: d.damage, attacker_net: d.owner_eid,
                        hurt_dir: hd, is_crit: false, from_player: true,
                        knockback: d.knockback,
                    });
                }
                // Aplica poisoned status (visual) nos hits se a DelayedAoe
                // foi marcada com poison_dur_s > 0 (Smoke Bomb).
                if d.poison_dur_s > 0.0 {
                    let expires = now_s + d.poison_dur_s;
                    let target_set: std::collections::HashSet<EntityId> =
                        enemies.iter().copied().collect();
                    for (_, (net, tag)) in self.ecs.query_mut::<(&NetId, &mut EnemyTag)>() {
                        if target_set.contains(&net.0) && tag.poisoned_until < expires {
                            tag.poisoned_until = expires;
                        }
                    }
                }
                // Aplica stun status (Caltrops trap). Inimigo fica preso o
                // tempo do trap tick — root effetivo enquanto pisa nele.
                if d.stun_dur_s > 0.0 {
                    let expires = now_s + d.stun_dur_s;
                    let target_set: std::collections::HashSet<EntityId> =
                        enemies.iter().copied().collect();
                    for (_, (net, tag)) in self.ecs.query_mut::<(&NetId, &mut EnemyTag)>() {
                        if target_set.contains(&net.0) && tag.stunned_until < expires {
                            tag.stunned_until = expires;
                        }
                    }
                }
            }
        }

        // Drena pending_skill_hits — instant hits de skill (line/aoe/cone).
        // Resolve net_id → Entity uma vez por hit (linear scan; pequeno).
        if !combat_disabled && !self.pending_skill_hits.is_empty() {
            let queue = std::mem::take(&mut self.pending_skill_hits);
            for h in queue {
                let mut found: Option<(Entity, bool)> = None;
                for (e, (net, kind)) in self.ecs.query::<(&NetId, &EntityKind)>().iter() {
                    if net.0 == h.target_net {
                        let target_is_player = matches!(kind, EntityKind::Player);
                        found = Some((e, target_is_player));
                        break;
                    }
                }
                if let Some((e, target_is_player)) = found {
                    // PvP gating em skill hits: player→player so' se
                    // can_damage_player (PK Mode ambos ON, ou futuramente
                    // zona PvP / faccoes). enemy→enemy: skip (sem ff).
                    if h.from_player && target_is_player {
                        if !self.can_damage_player(h.attacker_net, h.target_net) { continue; }
                    } else if !h.from_player && !target_is_player {
                        continue;
                    }
                    damage_events.push((e, h.target_net, h.damage, h.attacker_net, h.from_player,
                        h.hurt_dir, h.is_crit, AttackInfo::Skill, h.knockback));
                }
            }
        }

        // Drena pending_heals — heals em players (self+ally) por skills.
        if !self.pending_heals.is_empty() {
            let queue = std::mem::take(&mut self.pending_heals);
            let mut healed_targets: Vec<EntityId> = Vec::new();
            for h in queue {
                let mut e: Option<Entity> = None;
                for (en, net) in self.ecs.query::<&NetId>().iter() {
                    if net.0 == h.target_net { e = Some(en); break; }
                }
                if let Some(e) = e {
                    if let Ok(mut hp) = self.ecs.get::<&mut Health>(e) {
                        if h.amount > 0 {
                            hp.current = (hp.current + h.amount).min(hp.max);
                            healed_targets.push(h.target_net);
                        }
                    }
                }
            }
            // Broadcast BuffApplied {kind=0 heal} pra cada alvo curado —
            // cliente renderiza music_burst em volta do char.
            for tnet in healed_targets {
                let msg = ServerMessage::BuffApplied { target_eid: tnet, kind: 0 };
                for s in self.sessions.values() {
                    if s.logged_in {
                        let _ = s.handle.to_client.send(msg.clone());
                    }
                }
            }
        }

        // Drena hits pendentes de bombas de canhao. Cada hit vira um
        // damage_event normal — PvP gating + hit feedback + crit + xp passam
        // automatico pelo pipeline. AttackInfo::Skill (sem refletir parry),
        // knockback fixo pequeno pra dar feel de impacto.
        if !self.pending_bomb_hits.is_empty() {
            let queue = std::mem::take(&mut self.pending_bomb_hits);
            for (e, eid, dmg, attacker_eid, hurt_dir, _impact_pos) in queue {
                // Target kind = player? (sabemos pelo lookup do entity)
                let target_is_player = self.ecs.get::<&PlayerTag>(e).is_ok();
                // PvP gating: player→player so' se can_damage_player.
                if target_is_player {
                    if !self.can_damage_player(attacker_eid, eid) { continue; }
                }
                damage_events.push((
                    e, eid, dmg, attacker_eid, true,
                    hurt_dir, false, AttackInfo::Skill, 0.6,
                ));
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
        // hit_this_tick é campo de GameWorld (acessado no send_snapshots).
        // Limpa antes de popular este tick.
        self.hit_this_tick.clear();
        self.crit_this_tick.clear();
        self.damage_this_tick.clear();
        self.attacker_weapon_this_tick.clear();
        // Dano causado por ENEMIES neste tick — lifesteal do boss (25%).
        let mut enemy_dealt: HashMap<EntityId, i32> = HashMap::new();
        for (entity, target_id, dmg, attacker_id, attacker_is_player, hurt_dir, is_crit, attack_info, kb_strength) in damage_events {
            // Resistencia do alvo reduz dano recebido (min 1).
            let (target_defense, target_dmg_reduction_pct) = {
                let mut d = 0i32;
                let mut pct = 0.0_f32;
                if let Some(s) = self.sessions.values().find(|s| s.entity_id == target_id) {
                    d = s.stats.defense;
                    pct = s.stats.damage_reduction_pct;
                } else if let Ok(tag) = self.ecs.get::<&EnemyTag>(entity) {
                    d = tag.stats.defense;
                    pct = tag.stats.damage_reduction_pct;
                }
                (d, pct.clamp(0.0, 0.75))
            };
            // Defesa eh % redux (soulslike feel) em vez de subtracao flat.
            // Cada ponto de defense = 1.5% redux, cap 75%. Reducao por
            // breakpoints (damage_reduction_pct) soma em cima, cap final 90%.
            let def_resist_pct = (target_defense as f32 * 0.015).clamp(0.0, 0.75);
            let total_resist = (def_resist_pct + target_dmg_reduction_pct).min(0.90);
            let mut dmg = ((dmg as f32) * (1.0 - total_resist)).round() as i32;
            dmg = dmg.max(1);

            // Boss bloqueando (AI PvP): -75% de dano + flash de parry no
            // snapshot (feedback visual de "blocked!"). Também alimenta o
            // contador de hits recentes que dispara o block reativo.
            if let Ok(mut btag) = self.ecs.get::<&mut EnemyTag>(entity) {
                if btag.is_boss {
                    btag.ai_recent_hits += 1.0;
                    if self.sim_time_s < btag.ai_block_until {
                        dmg = ((dmg as f32) * 0.25).round().max(1.0) as i32;
                        btag.parry_flash_pending = true;
                    }
                    // (Stagger bar REMOVIDA — boss tem hyperarmor permanente,
                    // nunca flincha nem toma stun de quebra. Stun de skill
                    // tipo Shield Bash continua valendo via stunned_until.)
                }
            }

            // Iron Will passive (Sword T4): se alvo eh player com rank>0
            // E HP <= 30% do max → -30% dmg recebido.
            if let Some(target_session) = self.sessions.values()
                .find(|s| s.entity_id == target_id)
            {
                let iron_will_rank = target_session.learned_skills.iter()
                    .find(|s| s.skill_id == 1008)
                    .map(|s| s.rank).unwrap_or(0);
                if iron_will_rank > 0 {
                    if let Ok(hp) = self.ecs.get::<&Health>(entity) {
                        let hp_pct = (hp.current as f32) / (hp.max.max(1) as f32);
                        if hp_pct <= 0.30 {
                            dmg = ((dmg as f32) * 0.70).round() as i32;
                            dmg = dmg.max(1);
                        }
                    }
                }
            }

            // Hunter's Mark active (skill 1040): buff 8s + 3 charges.
            // Cada projetil consome 1 charge + aplica +5%/rank dmg.
            // Bloodthirst (skill 1011): buff 4s — cura caster por 10% do
            // dmg em hits (lifesteal).
            // Coletamos info dos buffs primeiro (evita borrow conflict
            // com self.pending_heals.push depois).
            let now_for_buff = self.sim_time_s;
            let mut bt_lifesteal_target: Option<EntityId> = None;
            if attacker_is_player {
                if let Some(att) = self.sessions.values_mut()
                    .find(|s| s.entity_id == attacker_id)
                {
                    // Hunter's Mark
                    if att.hunters_mark_until > now_for_buff
                        && att.hunters_mark_charges > 0
                        && matches!(attack_info, AttackInfo::Projectile { .. })
                    {
                        let bonus_pct = 0.05 * att.hunters_mark_rank as f32;
                        dmg = ((dmg as f32) * (1.0 + bonus_pct)).round() as i32;
                        att.hunters_mark_charges -= 1;
                        if att.hunters_mark_charges == 0 {
                            att.hunters_mark_until = 0.0;
                        }
                    }
                    // Bloodthirst
                    if att.bloodthirst_until > now_for_buff
                        && !matches!(attack_info, AttackInfo::Skill)
                    {
                        bt_lifesteal_target = Some(att.entity_id);
                    }
                }
            }
            if let Some(owner_net) = bt_lifesteal_target {
                let heal = ((dmg as f32) * 0.10).round().max(1.0) as i32;
                self.pending_heals.push(PendingHeal {
                    target_net: owner_net,
                    amount: heal,
                });
            }

            // ── Defesa ativa do alvo (player) ────────────────────────────────
            // (1) Parry: edge-press de PRIMARY ou SECONDARY dentro de PARRY_WINDOW_S
            //     → anula o dano + drena custo de parry (escala c/ RES) + staggera atacante.
            //     Funciona p/ todas as armas (melee inclui).
            // (2) Block: apenas armas ranged + RMB held + stamina suficiente
            //     → reduz dano em block_dmg_reduction (escala c/ RES) + drena custo.
            //     Melee NAO bloqueia (gateado em session.defending no input loop).
            let mut parried = false;
            let now_s = self.sim_time_s;
            if let Some(target) = self.sessions.values_mut()
                .find(|s| s.entity_id == target_id)
            {
                let in_parry_window =
                    (now_s - target.last_press_primary_at)   <= shared::PARRY_WINDOW_S
                 || (now_s - target.last_press_secondary_at) <= shared::PARRY_WINDOW_S;
                let parry_cost = shared::combat::parry_stamina_cost(&target.stats);
                let block_cost = shared::combat::block_stamina_cost(&target.stats);
                // Master's Counter (1007): durante 2s, qualquer hit recebido
                // e auto-parryado independente de timing/stamina (sem cost).
                let counter_active = target.counter_stance_until > now_s;
                let normal_parry = in_parry_window && target.stamina_current >= parry_cost;
                if counter_active || normal_parry {
                    if normal_parry {
                        target.stamina_current = (target.stamina_current - parry_cost).max(0.0);
                    }
                    target.parry_flash_pending = true;
                    // Consome a janela pra impedir parry sequencial sem novo press.
                    target.last_press_primary_at   = f32::NEG_INFINITY;
                    target.last_press_secondary_at = f32::NEG_INFINITY;
                    parried = true;
                    // dmg sera lido pelo counter handler abaixo (precisa do
                    // valor original pra reflexo de projectile). Zerado depois.
                } else if target.defending && target.stamina_current >= block_cost {
                    target.stamina_current = (target.stamina_current - block_cost).max(0.0);
                    dmg = shared::combat::apply_block_damage(&target.stats, dmg);
                }
            }

            // Aplica stagger no atacante em caso de parry — funciona pra
            // player (Session.stagger_until) e pra enemy (EnemyTag.hurt_until).
            if parried {
                let stagger_ts = now_s + shared::PARRY_STAGGER_S;
                if attacker_is_player {
                    if let Some(att) = self.sessions.values_mut()
                        .find(|s| s.entity_id == attacker_id)
                    {
                        att.stagger_until = stagger_ts;
                    }
                } else {
                    // Atacante eh enemy — encontra a Entity pelo EntityId.
                    let attacker_entity = self.ecs
                        .query::<&NetId>()
                        .iter()
                        .find(|(_, n)| n.0 == attacker_id)
                        .map(|(e, _)| e);
                    if let Some(ae) = attacker_entity {
                        if let Ok(mut tag) = self.ecs.get::<&mut EnemyTag>(ae) {
                            tag.hurt_until = stagger_ts;
                        }
                    }
                }

                // Counter automatico: melee gera contra-ataque na direcao do
                // atacante; projectile e' refletido (dir = -vel original).
                // Pega pos+stats do parryador (target_id == quem parryou).
                let parryer_data = self.sessions.values()
                    .find(|s| s.entity_id == target_id)
                    .and_then(|s| {
                        let entity = s.entity?;
                        let pos = self.ecs.get::<&Position>(entity).ok()?.0;
                        Some((pos, s.stats.attack_damage))
                    });
                if let Some((parryer_pos, atk_dmg)) = parryer_data {
                    match attack_info {
                        AttackInfo::Melee { attacker_pos } => {
                            let dir = (attacker_pos - parryer_pos)
                                .try_normalize().unwrap_or(Vec2::X);
                            // Counter swing: 70% do ataque base do parryador,
                            // dispara como melee swing parryador→atacante.
                            let counter_dmg = ((atk_dmg as f32) * 0.70).round() as i32;
                            melee_swings.push(MeleeSwing {
                                attacker_eid: target_id,
                                pos: parryer_pos,
                                dir,
                                damage: counter_dmg.max(1),
                                is_crit: false,
                                from_player: true,
                                knockback: 1.0, // parry counter — empurra
                                target: None,
                                max_range: shared::MELEE_RANGE,
                            });
                            tracing::info!("parry counter (melee): dmg={} from {} → {}",
                                counter_dmg, target_id.0, attacker_id.0);
                        }
                        AttackInfo::Projectile { vel, kind } => {
                            // Reflexo: spawn novo projectile na pos do parryador
                            // com dir oposta (saindo de volta pra direcao do
                            // atacante). Damage = 100% do dmg recebido.
                            let reflect_dir = (-vel).try_normalize().unwrap_or(Vec2::X);
                            let reflect_pos = parryer_pos
                                + Vec2::new(0.0, shared::PROJ_SPAWN_OFFSET_Y);
                            self.pending_shots.push(PendingShot {
                                pos: reflect_pos,
                                dir: reflect_dir,
                                damage: dmg.max(1),
                                is_crit: false,
                                kind,
                                owner_id: target_id, // parryador agora e' o "dono"
                                from_player: true,
                                release_tick: self.tick.wrapping_add(1),
                            });
                            tracing::info!("parry reflect (proj): kind={} from {} → reflected",
                                kind, target_id.0);
                        }
                        AttackInfo::Skill => {
                            // Skill hit: so anula damage, sem counter (skills
                            // costumam ser AoE/instantaneas — refletir e' awkward).
                        }
                    }
                }

                continue; // sem dano nem stagger no alvo
            }

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
            // Poise: se alvo eh player com poise > 0, absorve TODO o dano —
            // sem HP drain, sem stagger, sem hurt anim. Defesa + defending
            // reduzem o poise damage. Set last_combat_at pra gate de regen.
            //
            // Iframe: durante dash/leap/defending o player tem poise automatico.
            // Hits absorvidos sem custar poise_current. Defending drena stamina
            // por hit; sem stamina, defending auto-cancela em outro loop, então
            // iframe acaba naturalmente.
            let mut absorbed_by_poise = false;
            let now_s_poise = self.sim_time_s;
            if let Some(target) = self.sessions.values_mut().find(|s| s.entity_id == target_id) {
                let in_iframe = target.dash_until > now_s_poise
                             || target.leap_until > now_s_poise;
                if in_iframe && dmg > 0 {
                    // Dash/leap = iframe puro: absorve sem custo (janelas curtas).
                    target.last_combat_at_s = now_s_poise;
                    absorbed_by_poise = true;
                } else if target.defending && target.defending_poise_buffer > 0.0 && dmg > 0 {
                    // Defending: drena o buffer de poise (escalado por lvl+escudo)
                    // + stamina por hit. Reducao de damage por defense + bonus
                    // de block. Quando buffer zera, defending cancela e proximo
                    // hit cai no caminho de poise normal / HP+stagger.
                    let def_resist = (target.stats.defense as f32 * 0.04).min(0.8);
                    let block_bonus = 0.4;
                    let total_resist = (def_resist + block_bonus).min(0.95);
                    let poise_dmg = ((dmg as f32) * (1.0 - total_resist)).max(1.0);
                    target.defending_poise_buffer = (target.defending_poise_buffer - poise_dmg).max(0.0);
                    let block_cost = shared::combat::block_stamina_cost(&target.stats);
                    target.stamina_current = (target.stamina_current - block_cost).max(0.0);
                    target.last_combat_at_s = now_s_poise;
                    absorbed_by_poise = true;
                    // Buffer zerou → defending falha; proximo hit nao usa essa branch
                    if target.defending_poise_buffer <= 0.0 {
                        target.defending = false;
                    }
                } else if target.poise_current > 0.0 && dmg > 0 {
                    let def_resist = (target.stats.defense as f32 * 0.04).min(0.8);
                    let total_resist = def_resist.min(0.95);
                    let poise_dmg = ((dmg as f32) * (1.0 - total_resist)).max(1.0);
                    target.poise_current = (target.poise_current - poise_dmg).max(0.0);
                    target.last_combat_at_s = now_s_poise;
                    absorbed_by_poise = true;
                } else if target.poise_current <= 0.0 {
                    // Poise quebrado — hits chegam no HP, mas ainda registra
                    // combat pra evitar regen prematuro.
                    target.last_combat_at_s = now_s_poise;
                }
            }
            if absorbed_by_poise {
                // Display de dano segue (numero floating mostra absorbed) +
                // facing direction. Mas SKIP HP drain + stagger.
                let prev_dmg = self.damage_this_tick.get(&target_id).copied().unwrap_or(0);
                self.damage_this_tick.insert(target_id, prev_dmg + dmg);
                self.hit_this_tick.insert(target_id, hurt_dir);
                continue;
            }
            // Modo imortal: o dano segue sendo calculado e o numero flutuante
            // continua aparecendo — so' o HP nao cai. Curto-circuitar antes
            // esconderia justamente o que se quer ver enquanto se constroi
            // mundo: se o mob ACERTA, e quanto.
            let protegido = self.imortal && self.ecs.get::<&PlayerTag>(entity).is_ok();
            if !protegido {
                if let Ok(mut hp) = self.ecs.get::<&mut Health>(entity) {
                    hp.current = (hp.current - dmg).max(0);
                    if hp.current == 0 && attacker_is_player {
                        kill_credits.insert(target_id, attacker_id);
                    }
                }
            }
            // Registra dano de enemy → lifesteal de boss aplicado pós-loop.
            if !attacker_is_player {
                *enemy_dealt.entry(attacker_id).or_insert(0) += dmg;
            }
            // Healing Touch (1052) — Staff T2 P: auto-attacks healam self
            // 1%/rank do dmg dealt. Aplica em damage events de player → enemy.
            if attacker_is_player {
                if let Some(s) = self.sessions.values_mut().find(|s| s.entity_id == attacker_id) {
                    if let Some(ht) = s.learned_skills.iter().find(|sk| sk.skill_id == 1052).copied() {
                        let heal_amt = (dmg as f32 * 0.01 * ht.rank as f32).round() as i32;
                        if heal_amt > 0 {
                            if let Some(ae) = s.entity {
                                if let Ok(mut hp2) = self.ecs.get::<&mut Health>(ae) {
                                    hp2.current = (hp2.current + heal_amt).min(hp2.max);
                                }
                            }
                        }
                    }
                }
            }
            // Stagger: aplica hurt_until = sim_time + HURT_STAGGER_DURATION.
            // Cliente recebe HP drop + hurt_dir no próximo snapshot, dispara
            // TriggerHurt e seta facing TOWARD o atacante.
            let hurt_until_ts = self.sim_time_s + shared::HURT_STAGGER_DURATION;
            // Knockback: empurra alvo na direcao -hurt_dir (TOWARD atacante,
            // entao -hurt_dir = AWAY from attacker). Velocidade calculada pra
            // cobrir kb_strength tiles em KNOCKBACK_DURATION (0.18s).
            const KNOCKBACK_DURATION: f32 = 0.18;
            let kb_active = kb_strength > 0.05;
            let kb_until_ts = if kb_active { self.sim_time_s + KNOCKBACK_DURATION } else { 0.0 };
            let kb_vel = if kb_active {
                (-hurt_dir).try_normalize().unwrap_or(Vec2::ZERO) * (kb_strength / KNOCKBACK_DURATION)
            } else { Vec2::ZERO };
            if let Ok(mut tag) = self.ecs.get::<&mut EnemyTag>(entity) {
                tag.hurt_until = hurt_until_ts;
                tag.hurt_dir   = hurt_dir;
                // BOSS: hyperarmor de knockback — só hits PESADOS (skills com
                // kb >= 1.0, ex Shield Bash) empurram. O shove do auto-attack
                // (0.3-0.6) NÃO trava a IA dele — senão spam de ataque deixa
                // ele knockback-locked e ele nunca bloqueia/esquiva/reage.
                let kb_immune = tag.is_boss && kb_strength < 1.0;
                if kb_active && !kb_immune {
                    tag.knockback_until = kb_until_ts;
                    tag.knockback_vel   = kb_vel;
                }
            } else if let Some(s) = self.sessions.values_mut()
                .find(|s| s.entity_id == target_id)
            {
                s.hurt_dir = hurt_dir;
                // ── MOB NAO TRAVA NEM EMPURRA O JOGADOR ──
                //
                // `hurt_until` zera a velocidade e `knockback_vel` sobrescreve
                // ela. Num jogo de um mob por vez isso e' peso do golpe; com
                // matilha em cima, cada mordida rouba um pedaco do controle e
                // o jogador passa a assistir o boneco em vez de guia-lo — que
                // e' o pior que um MMO de celular pode fazer.
                //
                // Vale so' pra dano vindo de MOB. Dano de jogador (PvP) e
                // skill continuam travando e empurrando: la' o golpe e' um
                // evento, nao um chuvisco.
                if attacker_is_player {
                    s.hurt_until = hurt_until_ts;
                    if kb_active {
                        s.knockback_until = kb_until_ts;
                        s.knockback_vel   = kb_vel;
                    }
                }
                // Reset combo: levar dano interrompe o flow do combo.
                s.combo_step = 0;
                s.combo_last_attack = 0.0;
            }
            // Também marca pra snapshot deste tick (cliente lê e seta facing).
            self.hit_this_tick.insert(target_id, hurt_dir);
            // Marca crit pra mostrar floating number diferenciado no cliente.
            // Usa OR pra que multiple hits no mesmo tick (raro, mas possivel)
            // mostrem crit se qualquer dos hits foi crit.
            let prev = self.crit_this_tick.get(&target_id).copied().unwrap_or(false);
            self.crit_this_tick.insert(target_id, prev || is_crit);
            // Dano REAL acumulado (pre-clamp pelo HP). Cliente exibe esse
            // valor mesmo se o alvo for morto — não fica clampado em "5/50".
            let prev_dmg = self.damage_this_tick.get(&target_id).copied().unwrap_or(0);
            self.damage_this_tick.insert(target_id, prev_dmg + dmg);
            // Proficiency XP: atacante ganha XP na arma equipada por hit no alvo.
            if attacker_is_player {
                if let Some(attacker) = self.sessions.values_mut()
                    .find(|s| s.entity_id == attacker_id)
                {
                    let weapon_id = attacker.equipment.weapon.unwrap_or(0);
                    // Replica weapon do atacante pro snapshot do alvo —
                    // cliente escolhe VFX de impacto baseado nisso.
                    if weapon_id != 0 {
                        self.attacker_weapon_this_tick.insert(target_id, weapon_id);
                    }
                    let prof = shared::Proficiency::from_item(weapon_id);
                    let idx = prof as usize;
                    if idx < attacker.proficiencies.len() {
                        let old_lvl = shared::proficiency_level(attacker.proficiencies[idx]);
                        // 1 XP por hit; mais com kill (tratado no bloco de kill).
                        attacker.proficiencies[idx] = attacker.proficiencies[idx].saturating_add(1);
                        let new_lvl = shared::proficiency_level(attacker.proficiencies[idx]);
                        if new_lvl != old_lvl {
                            attacker.proficiencies_dirty = true;
                        }
                    }
                }
            }
        }
        for (e, eid) in hit_projs {
            let _ = self.ecs.despawn(e);
            self.removed_this_tick.push(eid);
        }

        // Lifesteal de BOSS (AI PvP): hits do boss curam 25% do dano causado
        // — "roubo de vida com ataque". Pressiona o player a não trocar burro.
        if !enemy_dealt.is_empty() {
            for (_, (net, hp, tag)) in self.ecs.query_mut::<(&NetId, &mut Health, &EnemyTag)>() {
                if !tag.is_boss || tag.dead { continue; }
                if let Some(d) = enemy_dealt.get(&net.0) {
                    let heal = ((*d as f32) * 0.25).round() as i32;
                    if heal > 0 { hp.current = (hp.current + heal).min(hp.max); }
                }
            }
        }

        // ── H: morte de inimigos → loot ───────────────────────────────────────
        // Filtra só os FRESCOS (HP=0 mas ainda não marcados dead) — drop e
        // bookkeeping rolam UMA vez. Cadáver fica em cena por
        // ENEMY_CORPSE_LINGER segundos antes do despawn real (loop H.2 abaixo).
        let dead_enemies: Vec<(Entity, EntityId, Vec2, u16)> = self
            .ecs
            .query::<(&NetId, &Position, &Health, &EnemyTag, &EntityKind)>()
            .iter()
            .filter_map(|(e, (net, pos, hp, tag, kind))| {
                if hp.current <= 0 && !tag.dead {
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
            // Se pertencia a uma spawn zone, decrementa live + enfileira respawn.
            let zone_info = self.ecs.get::<&SpawnedByZone>(e).ok()
                .map(|t| (t.zone_id, t.kind, t.slot_idx));
            if let Some((zid, zkind, slot_idx)) = zone_info {
                if let Some(zone) = self.spawn_zones.iter_mut().find(|z| z.id == zid) {
                    let ready_at = self.sim_time_s + zone.respawn_delay_s;
                    if zone.level_range.is_some() {
                        if zone.level_range_live > 0 { zone.level_range_live -= 1; }
                        // Slots Poisson: libera slot pra respawn no mesmo lugar
                        // depois do delay. Tipo varia (sortado a cada spawn).
                        if slot_idx != u32::MAX {
                            if let Some(slot) = zone.slots.get_mut(slot_idx as usize) {
                                slot.occupant = None;
                                slot.respawn_at = ready_at;
                            }
                        }
                    } else {
                        if let Some(entry) = zone.live.iter_mut().find(|(k, _)| *k == zkind) {
                            if entry.1 > 0 { entry.1 -= 1; }
                        }
                        zone.respawn_queue.push((ready_at, zkind));
                    }
                }
            }
            // Marca cadáver — entidade segue na cena pra cliente exibir pose
            // de morto. Real despawn no loop H.2.
            if let Ok(mut tag) = self.ecs.get::<&mut EnemyTag>(e) {
                tag.dead = true;
                tag.despawn_at = self.sim_time_s + shared::ENEMY_CORPSE_LINGER;
            }
            // Tira o body físico imediato (bate na entidade não faz sentido).
            self.free_entity_body(e);

            // Loot table por kind. Boss (7) ganha raio maior pelo volume
            // de drops; demais usam ~3 tiles pra dar respiro visual.
            let seed = lcg(self.tick as u64 ^ eid.0 as u64 ^ 0xBADA_55);
            let drops = crate::economy::enemy_loot_drops(kind_id, seed);
            let spread = if kind_id == 7 { 5.0 } else { 3.0 };
            self.spawn_loot_drops(pos, &drops, seed, spread, kind_id);

            // Creditar XP (e Fame, se mob grande) para o jogador que matou
            if let Some(attacker_eid) = kill_credits.get(&eid).copied() {
                // XP/fame: bosses dao bonus, demais usam xp_reward cacheado
                // no EnemyTag (procedural por level, level*12+20).
                let (xp_reward, fame_reward) = self.ecs.get::<&EnemyTag>(e)
                    .ok()
                    .map(|t| {
                        let xp = if t.is_boss { t.xp_reward * 5 } else { t.xp_reward };
                        let fame = if t.is_boss { 50 }
                                   else if t.level >= 50 { 5 }
                                   else if t.level >= 30 { 3 }
                                   else { 0 };
                        (xp, fame)
                    })
                    .unwrap_or((0, 0));
                // Descobre party do matador + membros proximos (mesmo AOI do kill)
                let (party_id, killer_pos) = {
                    let mut p: Option<u32> = None;
                    let mut pos = Vec2::ZERO;
                    for s in self.sessions.values() {
                        if s.entity_id == attacker_eid && s.logged_in {
                            p = s.party_id;
                            if let Some(e) = s.entity {
                                if let Ok(pp) = self.ecs.get::<&Position>(e) { pos = pp.0; }
                            }
                            break;
                        }
                    }
                    (p, pos)
                };
                // Lista de alvos a receber XP: matador + aliados dentro de PARTY_SHARE_RADIUS
                const PARTY_SHARE_RADIUS_SQ: f32 = 25.0 * 25.0;
                let mut recipients: Vec<EntityId> = vec![attacker_eid];
                if let Some(pid) = party_id {
                    for s in self.sessions.values() {
                        if s.party_id == Some(pid) && s.entity_id != attacker_eid && s.logged_in {
                            if let Some(e) = s.entity {
                                if let Ok(p) = self.ecs.get::<&Position>(e) {
                                    if p.0.distance_squared(killer_pos) <= PARTY_SHARE_RADIUS_SQ {
                                        recipients.push(s.entity_id);
                                    }
                                }
                            }
                        }
                    }
                }
                // Com party, +20% bonus total; divide igual entre todos
                let share = if recipients.len() > 1 {
                    ((xp_reward as f32 * 1.2) / recipients.len() as f32) as u64
                } else { xp_reward };
                let fame_share = fame_reward;
                for session in self.sessions.values_mut() {
                    if recipients.contains(&session.entity_id) && session.logged_in {
                        if session.entity_id == attacker_eid {
                            session.fame = session.fame.saturating_add(fame_share);
                            // Bonus de proficiencia ao matar: 10 XP na arma atual
                            let weapon_id = session.equipment.weapon.unwrap_or(0);
                            let prof = shared::Proficiency::from_item(weapon_id);
                            let idx = prof as usize;
                            if idx < session.proficiencies.len() {
                                let old = shared::proficiency_level(session.proficiencies[idx]);
                                session.proficiencies[idx] = session.proficiencies[idx].saturating_add(10);
                                if shared::proficiency_level(session.proficiencies[idx]) != old {
                                    session.proficiencies_dirty = true;
                                }
                            }
                        }
                        // XP + level-up + ProgressUpdate via fonte única.
                        session.grant_xp(share);
                        break;
                    }
                }
                // Progresso de quests de KILL (mob) — credita todos os recipients.
                for r in recipients.clone() { self.quest_on_kill(r, None); }
            }
        }

        // ── H.2: despawn de cadáveres com linger expirado ────────────────────
        let despawn_now: Vec<(Entity, EntityId)> = self
            .ecs
            .query::<(&NetId, &EnemyTag)>()
            .iter()
            .filter_map(|(e, (net, tag))| {
                if tag.dead && tag.despawn_at <= self.sim_time_s {
                    Some((e, net.0))
                } else { None }
            })
            .collect();
        for (e, eid) in despawn_now {
            let _ = self.ecs.despawn(e);
            self.removed_this_tick.push(eid);
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
        // Transfere fame + aura por kill de player ANTES do despawn.
        // Fame: +20 + 50% da vitima; vitima perde 30%.
        // Aura: +10 + 50% da aura da vitima; vitima perde 40% da sua.
        {
            let mut transfers: Vec<(EntityId, EntityId, u64, u64, u64, u64)> = Vec::new();
            // (killer, target, fame_gain, fame_loss, aura_gain, aura_loss)
            for (_, target_id) in &pending_real_death {
                if let Some(attacker_id) = kill_credits.get(target_id).copied() {
                    if let Some(target_sess) = self.sessions.values().find(|s| s.entity_id == *target_id) {
                        let vf = target_sess.fame;
                        let va = target_sess.aura;
                        let fame_gain = 20 + vf / 2;
                        let fame_loss = (vf * 3) / 10;
                        let aura_gain = 10 + va / 2;
                        let aura_loss = (va * 4) / 10;
                        transfers.push((attacker_id, *target_id, fame_gain, fame_loss, aura_gain, aura_loss));
                    }
                }
            }
            for (killer_id, target_id, fg, fl, ag, al) in transfers {
                for session in self.sessions.values_mut() {
                    if session.entity_id == killer_id {
                        session.fame = session.fame.saturating_add(fg);
                        session.aura = session.aura.saturating_add(ag);
                    } else if session.entity_id == target_id {
                        session.fame = session.fame.saturating_sub(fl);
                        session.aura = session.aura.saturating_sub(al);
                    }
                }
            }
        }
        // Aplica mortes reais acumuladas (downed_hp zerou por player).
        let deaths: Vec<(Entity, EntityId)> = std::mem::take(&mut pending_real_death);
        for (entity, eid) in deaths {
            // Captura pos + inventario + equip antes de despawn pra dropar.
            let death_pos = self.ecs.get::<&Position>(entity).map(|p| p.0).unwrap_or(Vec2::ZERO);
            let mut drops: Vec<(u16, u32)> = Vec::new();
            for session in self.sessions.values_mut() {
                if session.entity_id == eid {
                    // Drop todos os slots de inventario com qty>0
                    for slot in &session.inventory {
                        if slot.qty > 0 { drops.push((slot.item_id, slot.qty)); }
                    }
                    // Drop equipamento tambem
                    for opt in [session.equipment.weapon, session.equipment.armor, session.equipment.ring, session.equipment.offhand] {
                        if let Some(iid) = opt { drops.push((iid, 1)); }
                    }
                    // Limpa inv + equip do player morto
                    for slot in &mut session.inventory {
                        *slot = shared::InventorySlot::default();
                    }
                    session.equipment = shared::Equipment::default();
                    session.stats = effective_stats(&session.equipment, &session.allocated_points, &session.proficiencies, &session.learned_skills, session.xp);
                    session.inventory_dirty = true;
                    session.stats_dirty = true;
                    break;
                }
            }

            self.free_entity_body(entity);
            let _ = self.ecs.despawn(entity);
            self.removed_this_tick.push(eid);

            if !drops.is_empty() {
                let seed = lcg(self.tick as u64 ^ eid.0 as u64 ^ 0xDEAD_DEAD);
                // Player death: drops do inv+equip espalhados; kind=0 (sem
                // override de loot_item_level — usa item_level base).
                self.spawn_loot_drops(death_pos, &drops, seed, 3.0, 0);
            }

            // Libera carry se o morto estava sendo carregado ou carregando
            let (was_carried_by, was_carrying) = self.sessions.values()
                .find(|s| s.entity_id == eid)
                .map(|s| (s.carried_by, s.carrying))
                .unwrap_or((None, None));
            if let Some(carrier_eid) = was_carried_by {
                for s in self.sessions.values_mut() {
                    if s.entity_id == carrier_eid { s.carrying = None; break; }
                }
            }
            if let Some(target_eid) = was_carrying {
                for s in self.sessions.values_mut() {
                    if s.entity_id == target_eid { s.carried_by = None; break; }
                }
            }
            for session in self.sessions.values_mut() {
                if session.entity_id == eid {
                    session.entity = None;
                    session.respawn_timer = Some(RESPAWN_DELAY);
                    session.downed = false;
                    session.downed_hp = 0;
                    session.carrying = None;
                    session.carried_by = None;
                    tracing::info!("{} MORREU (barra downed zerou) — {} itens dropados, respawn em {}s",
                                   session.name, drops.len(), RESPAWN_DELAY);
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
        let now = self.sim_time_s;
        let mut picked: Vec<(Entity, EntityId)> = Vec::new();
        // (player_entity, new_hp_max) — para ajustar Health.max apos equipar.
        let mut hp_max_updates: Vec<(Entity, i32)> = Vec::new();
        'loot_loop: for (le, leid, lpos, ltag) in loots {
            // Janela de "ver o drop" antes do auto-pickup.
            if now - ltag.spawn_at < shared::LOOT_PICKUP_DELAY_S { continue; }
            for (sid, ppos) in &pickup_players {
                if ppos.distance_squared(lpos) < pick_r_sq {
                    if let Some(session) = self.sessions.get_mut(sid) {
                        // Se for equipavel e o slot esta vazio, equipa direto.
                        // Gate offhand+weapon: se incompativel, fallback pro inv.
                        if let Some(slot) = shared::equip_slot_of(ltag.item_id) {
                            let empty = session.equipment.get(slot).is_none();
                            let allowed = can_equip_in_slot(&session.equipment, slot, ltag.item_id);
                            if empty && allowed {
                                session.equipment.set(slot, Some(ltag.item_id), ltag.instance);
                                session.stats = effective_stats(&session.equipment, &session.allocated_points, &session.proficiencies, &session.learned_skills, session.xp);
                                session.stats_dirty = true;
                                if let Some(pe) = session.entity {
                                    hp_max_updates.push((pe, session.stats.hp_max));
                                }
                                picked.push((le, leid));
                                continue 'loot_loop;
                            }
                        }
                        // Gold é currency: vai pro contador, não ocupa inventário.
                        if ltag.item_id == shared::item_id::GOLD {
                            session.gold = session.gold.saturating_add(ltag.qty as u64);
                            picked.push((le, leid));
                            continue 'loot_loop;
                        }
                        // Senao, inventario normal.
                        if add_to_inventory(&mut session.inventory, ltag.item_id, ltag.qty, ltag.instance) {
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
        let (entity_id, faction) = match self.sessions.get(&sid) {
            Some(s) => (s.entity_id, s.faction),
            None => return,
        };
        // Tutorial: respawna na área do Matteo (depois da pedra, perto da arena) —
        // NÃO na cidade-sede do mundo.
        let spawn = if self.tutorial_mode {
            Vec2::new(TUTORIAL_AREA.0, TUTORIAL_AREA.1)
        } else {
            let spawn_tile = self.map.faction_spawn_tile(faction);
            Vec2::new(spawn_tile.0 as f32 + 0.5, spawn_tile.1 as f32 + 0.5)
        };
        let handle = self.spawn_entity_body(spawn);
        let e = self.ecs.spawn((
            NetId(entity_id),
            handle,
            Position(spawn),
            Velocity(Vec2::ZERO),
            Health { current: 100, max: 100 },
            EntityKind::Player,
            PlayerTag { name: name.clone(), player_id: pid, attack_anim_pending: None, combo_step_pending: None },
        ));
        if let Some(s) = self.sessions.get_mut(&sid) {
            s.entity = Some(e);
        }
        tracing::info!("respawn: {name}");
    }

    pub fn send_snapshots(&mut self) {
        // Coleta attack_pending dos inimigos e zera pra mandar 1 vez só.
        // Junto vai a direção do golpe (attack_dir) — cliente seta facing
        // do swing pra MIRAR O ALVO (boss strafando olhava pro lado).
        let mut enemy_aim_dirs: HashMap<EntityId, [f32; 2]> = HashMap::new();
        let attacking_ids: std::collections::HashSet<EntityId> = self
            .ecs
            .query::<(&NetId, &mut EnemyTag)>()
            .iter()
            .filter_map(|(_, (net, t))| {
                if t.attack_pending {
                    t.attack_pending = false;
                    enemy_aim_dirs.insert(net.0, [t.attack_dir.x, t.attack_dir.y]);
                    Some(net.0)
                } else { None }
            })
            .collect();

        // Mesma logica pra players: drena attack_anim_pending por PlayerTag.
        // Também drena combo_step_pending no MESMO walk pra evitar dois queries.
        let mut player_attack_anim: HashMap<EntityId, u8> = HashMap::new();
        let mut player_combo_step: HashMap<EntityId, u8> = HashMap::new();
        for (_, (net, t)) in self.ecs.query::<(&NetId, &mut PlayerTag)>().iter()
        {
            if let Some(a) = t.attack_anim_pending.take() {
                player_attack_anim.insert(net.0, a);
            }
            if let Some(s) = t.combo_step_pending.take() {
                player_combo_step.insert(net.0, s);
            }
        }

        // Lookup rapido por entity_id pra popular weapon_id/downed/visual em
        // snapshots de Player. Sessao não autenticada não entra (logged_in=false).
        struct PlayerOverlay {
            weapon_id: Option<u16>,
            offhand_id: Option<u16>,
            downed: bool,
            visual: shared::VisualConfig,
            attack_speed_mult: f32,
            defending: bool,
            casting: bool,
            poise_active: bool,
            buffs_mask: u8,
        }
        // Drena parry_flash_pending no mesmo passo — uma vez por tick.
        let mut player_overlay: HashMap<EntityId, PlayerOverlay> = HashMap::new();
        let mut parry_flash_ids: std::collections::HashSet<EntityId> = std::collections::HashSet::new();
        // Bosses: drena parry flash (block) e dash anim deste tick.
        let mut enemy_dash_anim_ids: std::collections::HashSet<EntityId> = std::collections::HashSet::new();
        for (_, (net, tag)) in self.ecs.query_mut::<(&NetId, &mut EnemyTag)>() {
            if tag.parry_flash_pending {
                tag.parry_flash_pending = false;
                parry_flash_ids.insert(net.0);
            }
            if tag.dash_anim_pending {
                tag.dash_anim_pending = false;
                enemy_dash_anim_ids.insert(net.0);
            }
        }
        let now_for_cast = self.sim_time_s;
        for s in self.sessions.values_mut() {
            if !s.logged_in { continue; }
            if s.parry_flash_pending {
                s.parry_flash_pending = false;
                parry_flash_ids.insert(s.entity_id);
            }
            let casting = s.casting_until > now_for_cast;
            // poise_active inclui iframes de dash/leap/defending — bubble
            // dourada durante pulos (Leap Strike, Spear), dash e defesa ativa.
            let poise_active = s.poise_current > 0.5
                || s.dash_until > now_for_cast
                || s.leap_until > now_for_cast
                || s.defending;
            let mut buffs_mask: u8 = 0;
            if s.bloodthirst_until  > now_for_cast { buffs_mask |= shared::components::buffs_mask::BLOODTHIRST;  }
            if s.hunters_mark_until > now_for_cast { buffs_mask |= shared::components::buffs_mask::HUNTERS_MARK; }
            if let Some(ring_id) = s.equipment.ring {
                match ring_id {
                    shared::constants::item_id::RING_TIDE => {
                        buffs_mask |= shared::components::buffs_mask::AURA_TIDE;
                    }
                    shared::constants::item_id::RING_IGNITION => {
                        buffs_mask |= shared::components::buffs_mask::AURA_IGNITION;
                    }
                    shared::constants::item_id::RING_MIST => {
                        buffs_mask |= shared::components::buffs_mask::AURA_MIST;
                    }
                    shared::constants::item_id::RING_TEMPEST => {
                        buffs_mask |= shared::components::buffs_mask::AURA_TEMPEST;
                    }
                    _ => {}
                }
            }
            player_overlay.insert(s.entity_id, PlayerOverlay {
                weapon_id: s.equipment.weapon,
                offhand_id: s.equipment.offhand,
                downed: s.downed,
                visual: s.visual.clone(),
                attack_speed_mult: s.stats.attack_speed_mult,
                defending: s.defending,
                casting,
                poise_active,
                buffs_mask,
            });
        }

        // Coleta enemies com status visuais (poisoned, stunned) pra replicar
        // tints no client. Pre-built maps pra evitar lookup repetido.
        let now_sim_for_status = self.sim_time_s;
        let poisoned_ids: std::collections::HashSet<EntityId> = self.ecs
            .query::<(&NetId, &EnemyTag)>()
            .iter()
            .filter_map(|(_, (net, tag))| {
                if tag.poisoned_until > now_sim_for_status { Some(net.0) } else { None }
            })
            .collect();
        let stunned_ids: std::collections::HashSet<EntityId> = self.ecs
            .query::<(&NetId, &EnemyTag)>()
            .iter()
            .filter_map(|(_, (net, tag))| {
                if tag.stunned_until > now_sim_for_status { Some(net.0) } else { None }
            })
            .collect();
        // Computa leap_y por player que esta em leap. y = 4 * peak * t * (1-t)
        // → parabola, peak no meio do leap (t=0.5).
        const LEAP_PEAK_HEIGHT: f32 = 1.5; // pico ~1.5 tiles acima do chao
        const LEAP_DURATION_SNAP: f32 = 0.5;
        let mut leap_y_map: HashMap<EntityId, f32> = HashMap::new();
        for s in self.sessions.values() {
            if !s.logged_in || s.leap_until <= 0.0 { continue; }
            if s.leap_until > now_sim_for_status {
                let elapsed = LEAP_DURATION_SNAP - (s.leap_until - now_sim_for_status);
                let t = (elapsed / LEAP_DURATION_SNAP).clamp(0.0, 1.0);
                let y = 4.0 * LEAP_PEAK_HEIGHT * t * (1.0 - t);
                if y > 0.01 {
                    leap_y_map.insert(s.entity_id, y);
                }
            }
        }
        // Enemies em leap (boss Leap Strike) — mesmo arco visual do player.
        for (_, (net, tag)) in self.ecs.query::<(&NetId, &EnemyTag)>().iter() {
            if tag.leap_until > now_sim_for_status {
                let elapsed = LEAP_DURATION_SNAP - (tag.leap_until - now_sim_for_status);
                let t = (elapsed / LEAP_DURATION_SNAP).clamp(0.0, 1.0);
                let y = 4.0 * LEAP_PEAK_HEIGHT * t * (1.0 - t);
                if y > 0.01 { leap_y_map.insert(net.0, y); }
            }
        }

        // Conjunto de player_eids montados (pra `mounted: true` no snapshot).
        let mounted_player_eids: std::collections::HashSet<EntityId> = self.ecs
            .query::<(&NetId, &Mounted)>()
            .iter()
            .map(|(_, (net, _))| net.0)
            .collect();

        // Altura atual de cada bomba (offset Y de render do arco parabolico).
        // y = 4 * peak * t * (1-t).
        let cannon_height_map: HashMap<EntityId, f32> = self.ecs
            .query::<(&NetId, &CannonBombTag)>()
            .iter()
            .map(|(_, (net, b))| {
                let t = (b.t_elapsed / b.t_max).clamp(0.0, 1.0);
                let h = 4.0 * b.peak_height * t * (1.0 - t);
                (net.0, h)
            })
            .collect();

        // Detalhes de cada player montado pra popular boat_2.5d snapshot fields.
        let mounted_player_info: HashMap<EntityId, (EntityId, Vec2, Option<u8>)> = self.ecs
            .query::<(&NetId, &Mounted)>()
            .iter()
            .map(|(_, (net, m))| (net.0, (m.boat_eid, m.local_pos, m.station)))
            .collect();

        // Quem esta' no ar neste tick, pra marcar a flag no estado.
        let no_ar_agora: std::collections::HashSet<EntityId> = self
            .sessions
            .values()
            .filter(|s| s.pulo_ate > self.sim_time_s)
            .map(|s| s.entity_id)
            .collect();

        // ── Estado do tick ────────────────────────────────────────────────
        //
        // Uma varredura do ECS produz META (dado estavel: tipo, nome, hp_max)
        // e ESTADO (posicao, velocidade, hp, flags — quantizados). O meta so'
        // vai pro wire quando a entidade ENTRA no campo de visao de alguem; o
        // estado vai quando muda. Antes, tudo isso era um struct de 58 campos
        // remandado inteiro por tick.
        let all: Vec<(EntityMeta, EntityState)> = self
            .ecs
            .query::<(
                &NetId, &Position, &Velocity, &EntityKind,
                Option<&Health>, Option<&PlayerTag>, Option<&VendorTag>,
                Option<&WanderRouteTag>, Option<&EnemyTag>,
            )>()
            .iter()
            .map(|(_, (net, pos, vel, kind, hp, ptag, vtag, wtag, etag))| {
                let tag = match kind {
                    EntityKind::Player     => shared::EntityTag::Player,
                    EntityKind::Enemy(_)   => shared::EntityTag::Enemy,
                    EntityKind::Projectile => shared::EntityTag::Projectile,
                    EntityKind::Loot(_)    => shared::EntityTag::Loot,
                    EntityKind::Npc(_)     => shared::EntityTag::Npc,
                    EntityKind::Portal     => shared::EntityTag::Portal,
                    EntityKind::Boat(_)    => shared::EntityTag::Boat,
                    _                      => shared::EntityTag::Other,
                };
                let name = ptag.map(|p| p.name.clone())
                    .or_else(|| vtag.map(|v| v.name.clone()))
                    .or_else(|| wtag.map(|w| w.name.clone()))
                    .or_else(|| etag.map(|t| {
                        if let Some(bn) = &t.boss_name { return bn.clone(); }
                        let tier = crate::enemy_builds::tier_for_level(t.level);
                        if t.is_boss {
                            format!("[BOSS] {} Lv{}", tier.theme.display_name, t.level)
                        } else {
                            format!("{} Lv{}", tier.theme.display_name, t.level)
                        }
                    }));
                let mut flags = 0u8;
                if etag.map_or(false, |t| t.is_boss) {
                    flags |= shared::ent_flags::BOSS;
                }
                // No ar: o cliente desenha o arco. Quem decide que o pulo
                // aconteceu e' este lado.
                if no_ar_agora.contains(&net.0) {
                    flags |= shared::ent_flags::PULANDO;
                }
                let meta = EntityMeta {
                    id: net.0,
                    tag,
                    name,
                    hp_max: hp.map(|h| h.max.max(0) as u16).unwrap_or(0),
                    faction: None,
                };
                let state = EntityState::quantize(
                    net.0,
                    pos.0,
                    vel.0,
                    hp.map(|h| h.current).unwrap_or(0),
                    flags,
                );
                (meta, state)
            })
            .collect();

        let removed = self.removed_this_tick.clone();

        // Hash espacial pro AOI: celula do tamanho do raio, busca em 3x3.
        let cell = AOI_RADIUS.max(SPATIAL_CELL_SIZE);
        let mut grid: HashMap<(i32, i32), Vec<usize>> = HashMap::new();
        for (idx, (_, st)) in all.iter().enumerate() {
            let p = st.pos_f32();
            let key = ((p.x / cell).floor() as i32, (p.y / cell).floor() as i32);
            grid.entry(key).or_default().push(idx);
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
        // Reaproveitado entre sessoes: com 200 jogadores isto seria 200
        // alocacoes por tick.
        let mut candidatos: Vec<(f32, usize)> = Vec::with_capacity(256);
        for (sid, session) in &mut self.sessions {
            if !session.logged_in { continue; }
            let center = centers.get(sid).copied().unwrap_or(Vec2::ZERO);
            let ccx = (center.x / cell).floor() as i32;
            let ccy = (center.y / cell).floor() as i32;
            let my_entity_id = session.entity_id;

            let mut entered: Vec<EntityMeta> = Vec::new();
            let mut states: Vec<EntityState> = Vec::new();

            // ── Candidatos, por distancia ─────────────────────────────────
            // Sem cap, um jogador no meio de uma horda recebia TODAS as
            // entidades mudando por tick — medido em 214 estados por snapshot
            // com 200 jogadores, ou 66 KB/s cada (238 MB por hora de dado
            // movel). O gargalo nao e' a codificacao: sao 10 bytes por
            // entidade, e' a quantidade.
            candidatos.clear();
            for dy in -1..=1 {
                for dx in -1..=1 {
                    let Some(idxs) = grid.get(&(ccx + dx, ccy + dy)) else { continue };
                    for &i in idxs {
                        let d2 = all[i].1.pos_f32().distance_squared(center);
                        if d2 <= radius_sq {
                            candidatos.push((d2, i));
                        }
                    }
                }
            }
            // Mais perto primeiro: se algo vai ficar de fora, que seja o que o
            // jogador menos enxerga.
            candidatos.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
            // Histerese: entidade JA conhecida sobrevive um pouco alem do cap,
            // senao ela fica entrando e saindo a cada passo do jogador e o
            // custo de `entered` (24 bytes) come o que o cap economizou.
            let limite_conhecida = shared::AOI_MAX_ENTIDADES + shared::AOI_HISTERESE;
            let mut visiveis: std::collections::HashSet<EntityId> =
                std::collections::HashSet::with_capacity(shared::AOI_MAX_ENTIDADES);

            for (rank, (d2, i)) in candidatos.iter().enumerate() {
                let (meta, st) = &all[*i];
                let conhecida = session.last_sent.contains_key(&st.id);
                let cabe = rank < shared::AOI_MAX_ENTIDADES
                    || (conhecida && rank < limite_conhecida)
                    || st.id == my_entity_id;
                if !cabe { continue; }
                visiveis.insert(st.id);

                let mut st = *st;
                if st.id == my_entity_id {
                    st.flags |= shared::ent_flags::SELF;
                }

                // ── Taxa por distancia ────────────────────────────────────
                // Perto atualiza todo tick; longe, de N em N. O cliente
                // interpola, entao um mob a 20 tiles andando a 5Hz continua
                // liso na tela. O `+ id` espalha os vencimentos entre os
                // ticks — sem isso todos os distantes vencem juntos e o
                // trafego vira serrote.
                let periodo = if st.id == my_entity_id {
                    1
                } else if *d2 <= shared::AOI_PERTO * shared::AOI_PERTO {
                    1
                } else if *d2 <= shared::AOI_MEIO * shared::AOI_MEIO {
                    3
                } else {
                    6
                };
                let vencido = (self.tick as u64 + st.id.0 as u64) % periodo == 0;

                match session.last_sent.get(&st.id) {
                    // Entidade nova pro jogador: meta + estado, na hora.
                    None => {
                        entered.push(meta.clone());
                        states.push(st);
                        session.last_sent.insert(st.id, st);
                    }
                    // Ja conhecida: so' vai se mudou E se e' a vez dela.
                    Some(anterior) if *anterior != st && vencido => {
                        states.push(st);
                        session.last_sent.insert(st.id, st);
                    }
                    _ => {}
                }
            }

            // Saiu do AOI ou morreu. O `removed` global so' cobre destruicao.
            let mut removed_for_me = removed.clone();
            session.last_sent.retain(|id, _| {
                let fica = visiveis.contains(id);
                if !fica { removed_for_me.push(*id); }
                fica
            });

            let _ = session.handle.to_client.send(ServerMessage::Snapshot { snapshot: WorldSnapshot {
                tick: self.tick,
                server_time_ms: now_ms(),
                last_input_seq: session.last_input_seq,
                entered,
                states,
                removed: removed_for_me,
            }});
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
            if session.gold != session.gold_last_sent {
                session.gold_last_sent = session.gold;
                let _ = session.handle.to_client.send(ServerMessage::GoldUpdate {
                    gold: session.gold,
                });
            }
            if session.fame != session.fame_last_sent {
                session.fame_last_sent = session.fame;
                let _ = session.handle.to_client.send(ServerMessage::FameUpdate {
                    fame: session.fame,
                });
            }
            if session.aura != session.aura_last_sent {
                session.aura_last_sent = session.aura;
                let _ = session.handle.to_client.send(ServerMessage::AuraUpdate {
                    aura: session.aura,
                });
            }
            if session.proficiencies_dirty {
                session.proficiencies_dirty = false;
                let _ = session.handle.to_client.send(ServerMessage::ProficienciesUpdate {
                    xp: session.proficiencies,
                });
            }
            if session.stat_points_dirty {
                session.stat_points_dirty = false;
                let _ = session.handle.to_client.send(ServerMessage::StatPointsUpdate {
                    unspent: session.unspent_points,
                    allocated: session.allocated_points,
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
            let poise_i = session.poise_current as i32;
            if poise_i != session.poise_last_sent {
                session.poise_last_sent = poise_i;
                let _ = session.handle.to_client.send(ServerMessage::PoiseUpdate {
                    current: poise_i,
                });
            }
        }

        // Limpa removed_this_tick DEPOIS de despachar pra todos os clients.
        // Antes, o clear() ficava no inicio de step(), o que apagava ids de
        // dismount/spawn/etc. processados em on_message ANTES do step rodar
        // — fazia o cliente nunca receber a remocao do barco.
        if let Some(p) = &self.populacao {
            p.set(self.sessions.values().filter(|s| s.logged_in).count());
        }
        self.removed_this_tick.clear();
    }
}

impl GameWorld {
    /// Monta linhas de persistencia com o estado atual de TODOS os jogadores
    /// logados. Tambem atualiza o cache em memoria pra que o proximo login
    /// (antes do DB terminar de gravar) ja veja dados novos.
    pub fn collect_character_rows(&mut self) -> Vec<crate::persistence::CharacterRow> {
        // Tutorial: PERSISTE o progresso (xp/level/inventário/arma craftada) pro
        // mundo — senão o lvl 2 e a arma sumiriam ao entrar no mundo. A POSIÇÃO é
        // mascarada (spawna na ilha-sede da facção, não na água do tutorial) — ver
        // a construção da row abaixo.
        let mut out = Vec::with_capacity(self.sessions.len());
        // Tuple grande pra escapar do borrow do ECS por sessão. Os campos extras
        // (skill_points_*, learned_skills) vão direto no constructor abaixo
        // pra não inflar mais o tuple.
        struct E {
            name: String,
            pos: Vec2,
            hp: Health,
            xp: u64,
            gold: u64,
            boat: Option<crate::persistence::PersistedBoat>,
            mounted_local: Option<Vec2>,
            inventory: Vec<shared::InventorySlot>,
            equipment: shared::Equipment,
            vault: Vec<shared::InventorySlot>,
            fame: u64,
            aura: u64,
            proficiencies: [u64; shared::PROF_COUNT],
            unspent_points: u32,
            allocated_points: [u32; shared::STAT_COUNT],
            sp_earned: u32,
            sp_spent: u32,
            learned: Vec<shared::LearnedSkill>,
            account_id: Option<i64>,
            visual: shared::VisualConfig,
            woodcutting_lvl: u32,
            mining_lvl:      u32,
            gathering_lvl:   u32,
            faction: shared::Faction,
            quests: Vec<crate::quests::CharQuest>,
            faction_points: u32,
        }
        let mut entries: Vec<E> = Vec::new();
        for session in self.sessions.values() {
            if !session.logged_in { continue; }
            let Some(e) = session.entity else { continue };
            // Boat 2.5D: captura estado completo do barco + posicao do
            // player no deck (mounted_local) pra restaurar no login.
            let mounted_data = self.ecs.get::<&Mounted>(e).ok()
                .map(|m| (m.boat_entity, m.local_pos));
            let (pos, boat, mounted_local) = if let Some((boat_e, lp)) = mounted_data {
                let bp = self.ecs.get::<&Position>(boat_e).ok().map(|p| p.0).unwrap_or_default();
                let (kind, dir, yaw, sp, sa, ad) = self.ecs.get::<&BoatTag>(boat_e)
                    .map(|t| (t.kind, t.dir, t.yaw, t.sail_position, t.sail_angle, t.anchor_dropped))
                    .unwrap_or((0, 4, 0.0, 0, 0.0, true));
                (bp, Some(crate::persistence::PersistedBoat {
                    kind, pos: bp, dir, yaw,
                    sail_position: sp, sail_angle: sa, anchor_dropped: ad,
                }), Some(lp))
            } else {
                let p = match self.ecs.get::<&Position>(e) { Ok(p) => p.0, Err(_) => continue };
                (p, None, None)
            };
            let hp = match self.ecs.get::<&Health>(e) { Ok(h) => *h, Err(_) => continue };
            entries.push(E {
                name: session.name.clone(),
                pos, hp, boat, mounted_local,
                xp: session.xp,
                gold: session.gold,
                inventory: session.inventory.clone(),
                equipment: session.equipment,
                vault: session.vault.clone(),
                fame: session.fame,
                aura: session.aura,
                proficiencies: session.proficiencies,
                unspent_points: session.unspent_points,
                allocated_points: session.allocated_points,
                sp_earned: session.skill_points_earned,
                sp_spent: session.skill_points_spent,
                learned: session.learned_skills.clone(),
                account_id: session.account_id,
                visual: session.visual.clone(),
                woodcutting_lvl: session.woodcutting_lvl,
                mining_lvl:      session.mining_lvl,
                gathering_lvl:   session.gathering_lvl,
                faction:         session.faction,
                quests: session.quests.clone(),
                faction_points: session.faction_points,
            });
        }
        for e in entries {
            // Tutorial/dungeon: mascara posição/barco — o player volta pra
            // ilha-sede da facção no mundo (e NÃO pra dentro da dungeon/tutorial).
            // inv/xp/loot SÃO persistidos normalmente (o loot da dungeon é o ponto).
            let (pos, boat, mounted_local) = if self.tutorial_mode || self.dungeon_mode {
                let t = self.map.faction_spawn_tile(e.faction);
                (Vec2::new(t.0 as f32 + 0.5, t.1 as f32 + 0.5), None, None)
            } else {
                (e.pos, e.boat, e.mounted_local)
            };
            let row = crate::persistence::CharacterRow {
                name: e.name.clone(),
                pos,
                hp: e.hp,
                xp: e.xp,
                gold: e.gold,
                boat,
                mounted_local,
                inventory: e.inventory,
                equipment: e.equipment,
                vault: e.vault,
                fame: e.fame,
                aura: e.aura,
                proficiencies: e.proficiencies,
                unspent_points: e.unspent_points,
                allocated_points: e.allocated_points,
                skill_points_earned: e.sp_earned,
                skill_points_spent: e.sp_spent,
                learned_skills: e.learned,
                account_id: e.account_id,
                visual: Some(e.visual),
                woodcutting_lvl: e.woodcutting_lvl,
                mining_lvl:      e.mining_lvl,
                gathering_lvl:   e.gathering_lvl,
                faction:         e.faction,
                quests:          e.quests,
                faction_points:  e.faction_points,
                last_tutorial_completed: None, // save não escreve essa coluna (preservada no DB)
            };
            self.characters.insert(e.name, row.clone());
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
        let read_eq = |s: shared::EquipSlot| -> Option<u16> { session.equipment.get(s) };

        let va = match a {
            InvSpot::Inv(i)   => (Some(read_inv(i)), None),
            InvSpot::Equip(s) => (None, Some((s, read_eq(s)))),
        };
        let vb = match b {
            InvSpot::Inv(i)   => (Some(read_inv(i)), None),
            InvSpot::Equip(s) => (None, Some((s, read_eq(s)))),
        };

        // Helpers de validacao — slot vazio ou stack-de-um item compativel.
        // Offhand+weapon e validado via `can_equip_in_slot`.
        let equip_now = session.equipment;
        let char_level_now = shared::level_of_xp_with_mult(session.xp, crate::economy::xp_multiplier());
        let prof_xp_now = session.proficiencies;
        let can_go_into_equip = |slot: shared::EquipSlot, item: &shared::InventorySlot| -> bool {
            if item.qty == 0 { return true; }
            if item.qty > 1 { return false; }
            // Item desativado pelo admin: bloqueado de ser equipado.
            if !crate::economy::is_item_active(item.item_id) { return false; }
            // Char level gate por item_id (e.g., ENHANCED_SWORD requer char lvl 10).
            if let Some(min_lvl) = shared::item_char_level_req(item.item_id) {
                if (char_level_now as u16) < min_lvl { return false; }
            }
            // Proficiency level gate por item_id (e.g., ENHANCED_SWORD requer Sword prof 5).
            if let Some((prof, min_prof_lvl)) = shared::item_prof_req(item.item_id) {
                let prof_xp = prof_xp_now[prof as usize];
                let prof_lvl = shared::proficiency_level(prof_xp);
                if (prof_lvl as u16) < min_prof_lvl { return false; }
            }
            can_equip_in_slot(&equip_now, slot, item.item_id)
        };

        match (a, b) {
            // inv <-> inv: swap direto
            (InvSpot::Inv(ai), InvSpot::Inv(bi)) => {
                let (Some(ia), _) = va else { return };
                let (Some(ib), _) = vb else { return };
                // Stack-merge quando ambos tem o mesmo item_id: junta b em a.
                if ia.qty > 0 && ib.qty > 0 && ia.item_id == ib.item_id {
                    let cap = crate::economy::item_stack_max(ia.item_id);
                    let move_qty = (cap - ib.qty).min(ia.qty);
                    if move_qty > 0 {
                        let na = ia.qty - move_qty;
                        let nb = ib.qty + move_qty;
                        session.inventory[ai as usize] = if na == 0 {
                            shared::InventorySlot::default()
                        } else {
                            shared::InventorySlot { item_id: ia.item_id, qty: na, instance: None }
                        };
                        session.inventory[bi as usize] = shared::InventorySlot { item_id: ia.item_id, qty: nb, instance: None };
                        session.inventory_dirty = true;
                        return;
                    }
                }
                session.inventory[ai as usize] = ib;
                session.inventory[bi as usize] = ia;
                session.inventory_dirty = true;
            }
            // inv -> equip: preserva instance em ambos os sentidos.
            (InvSpot::Inv(ai), InvSpot::Equip(bs)) => {
                let (Some(ia), _) = va else { return };
                let (_, Some((_, cur_eq))) = vb else { return };
                if !can_go_into_equip(bs, &ia) { return; }
                if bs == shared::EquipSlot::Weapon && ia.qty > 0 {
                    if !maybe_unequip_offhand_for_weapon(session, ia.item_id) {
                        return;
                    }
                }
                // Antiga instance do equipment vai pro inv junto com o item_id.
                let cur_inst = read_equip_instance(&session.equipment, bs);
                let new_inv_slot = match cur_eq {
                    Some(old_id) => shared::InventorySlot { item_id: old_id, qty: 1, instance: cur_inst },
                    None         => shared::InventorySlot::default(),
                };
                let new_id = if ia.qty > 0 { Some(ia.item_id) } else { None };
                let new_inst = if ia.qty > 0 { ia.instance } else { None };
                set_equip(session, bs, new_id, new_inst);
                session.inventory[ai as usize] = new_inv_slot;
                session.stats = effective_stats(&session.equipment, &session.allocated_points, &session.proficiencies, &session.learned_skills, session.xp);
                session.stats_dirty = true;
                session.inventory_dirty = true;
                let new_hp_max = session.stats.hp_max;
                let ent = session.entity;
                if let Some(e) = ent {
                    if let Ok(mut hp) = self.ecs.get::<&mut Health>(e) {
                        hp.max = new_hp_max;
                        if hp.current > hp.max { hp.current = hp.max; }
                    }
                }
            }
            (InvSpot::Equip(as_), InvSpot::Inv(bi)) => {
                let (Some(ib), _) = vb else { return };
                let (_, Some((_, cur_eq))) = va else { return };
                if !can_go_into_equip(as_, &ib) { return; }
                let cur_inst = read_equip_instance(&session.equipment, as_);
                let new_inv_slot = match cur_eq {
                    Some(old_id) => shared::InventorySlot { item_id: old_id, qty: 1, instance: cur_inst },
                    None         => shared::InventorySlot::default(),
                };
                let new_id = if ib.qty > 0 { Some(ib.item_id) } else { None };
                let new_inst = if ib.qty > 0 { ib.instance } else { None };
                set_equip(session, as_, new_id, new_inst);
                session.inventory[bi as usize] = new_inv_slot;
                session.stats = effective_stats(&session.equipment, &session.allocated_points, &session.proficiencies, &session.learned_skills, session.xp);
                session.stats_dirty = true;
                session.inventory_dirty = true;
                let new_hp_max = session.stats.hp_max;
                let ent = session.entity;
                if let Some(e) = ent {
                    if let Ok(mut hp) = self.ecs.get::<&mut Health>(e) {
                        hp.max = new_hp_max;
                        if hp.current > hp.max { hp.current = hp.max; }
                    }
                }
            }
            // equip <-> equip: so faz sentido se slots sao iguais (no-op)
            _ => {}
        }
    }

    /// Auto-arrange: agrupa stackaveis por item_id (respeitando stack_max),
    /// ordena ascendente, mantem itens com instance ao final. Mesma logica
    /// pra inv e vault — `auto_arrange_slots` faz o trabalho.
    fn handle_inventory_auto_arrange(&mut self, sid: SessionId) {
        let Some(session) = self.sessions.get_mut(&sid) else { return };
        if !session.logged_in { return; }
        auto_arrange_slots(&mut session.inventory);
        session.inventory_dirty = true;
    }

    fn handle_vault_auto_arrange(&mut self, sid: SessionId) {
        let Some(session) = self.sessions.get_mut(&sid) else { return };
        if !session.logged_in { return; }
        auto_arrange_slots(&mut session.vault);
        session.vault_dirty = true;
    }

    /// Crafta receita: valida inputs no inv, consome, deposita output. Output
    /// equipavel ganha ItemInstance rolado (rarity Common/Magic/Rare baseada em
    /// rng) — qualidade emerge do roll. Falha silenciosa se faltam materiais ou
    /// inv cheio. Sem retorno explicito — InventoryUpdate seguinte espelha.
    fn handle_craft(&mut self, sid: SessionId, recipe_id: u16) {
        let Some(session) = self.sessions.get_mut(&sid) else { return };
        if !session.logged_in { return; }
        // Recipes vem do DB cache (admin pode mudar custos sem rebuild).
        // Fallback pro hardcoded se cache vazio (boot inicial).
        let Some(recipe) = crate::recipes::find(recipe_id) else { return };
        let craft_output_id = recipe.output_item_id; // p/ o gatilho da quest 903
        // Gate: nível de proficiência de craft pro tier (T2=5/T3=20/T4=30).
        // Fundição/Marcenaria têm T2=1 (sem receita T1, senão deadlock).
        // O cliente desabilita a receita; isto é a rede de segurança.
        let prof = shared::Proficiency::from_craft_station(recipe.station);
        let craft_req = shared::tier_level_req_for(prof, recipe.tier);
        if craft_req > 1 {
            let cur = shared::proficiency_level(session.proficiencies[prof as usize]);
            if cur < craft_req { return; }
        }
        // Valida materiais.
        for pair in recipe.inputs.iter() {
            let in_id = pair[0] as u16;
            let in_qty = pair[1];
            if in_id == 0 { continue; }
            let total: u32 = session.inventory.iter()
                .filter(|s| s.item_id == in_id && s.instance.is_none())
                .map(|s| s.qty)
                .sum();
            if total < in_qty { return; }
        }
        // Acha slot livre OU stack-merge slot pro output (precisa antes de consumir).
        let cap = crate::economy::item_stack_max(recipe.output_item_id).max(1);
        let mut place_idx: Option<usize> = None;
        if !recipe.roll_instance {
            // Stackavel: tenta stack existente primeiro.
            for (i, s) in session.inventory.iter().enumerate() {
                if s.item_id == recipe.output_item_id
                    && s.instance.is_none()
                    && s.qty + recipe.output_qty <= cap
                {
                    place_idx = Some(i); break;
                }
            }
        }
        if place_idx.is_none() {
            for (i, s) in session.inventory.iter().enumerate() {
                if s.qty == 0 { place_idx = Some(i); break; }
            }
        }
        let Some(idx) = place_idx else { return }; // inv cheio
        // Consome inputs.
        for pair in recipe.inputs.iter() {
            let in_id = pair[0] as u16;
            let mut need = pair[1];
            if in_id == 0 { continue; }
            for s in session.inventory.iter_mut() {
                if need == 0 { break; }
                if s.item_id != in_id || s.instance.is_some() || s.qty == 0 { continue; }
                let take = need.min(s.qty);
                s.qty -= take;
                need -= take;
                if s.qty == 0 { *s = shared::InventorySlot::default(); }
            }
        }
        // Gera output.
        let new_slot = if recipe.roll_instance {
            let tpl = crate::economy::item_template_of(recipe.output_item_id);
            let mut rng = || fastrand::f32();
            let inst = shared::ItemInstance::roll_with_template(tpl, recipe.output_item_level, &mut rng);
            shared::InventorySlot {
                item_id:  recipe.output_item_id,
                qty:      1,
                instance: inst,
            }
        } else {
            // Stack — soma se ja tinha.
            let prev_qty = session.inventory[idx].qty;
            shared::InventorySlot {
                item_id:  recipe.output_item_id,
                qty:      prev_qty + recipe.output_qty,
                instance: None,
            }
        };
        session.inventory[idx] = new_slot;
        session.inventory_dirty = true;

        // XP de proficiência de craft (escala com o tier). Permite subir o nível
        // que destrava tiers maiores — é o "leveling" de craft.
        let prof = shared::Proficiency::from_craft_station(recipe.station);
        let pi = prof as usize;
        session.proficiencies[pi] = session.proficiencies[pi]
            .saturating_add(40u64 * recipe.tier.max(1) as u64);
        session.proficiencies_dirty = true;
        let _ = session.handle.to_client.send(ServerMessage::ProficienciesUpdate {
            xp: session.proficiencies,
        });
        self.save_pending = true;

        // Tutorial: o GATILHO da quest 903 ("Forje sua Arma") é o ATO de craftar
        // uma ARMA — checa pelo slot Weapon (os ids craftados são T1 migrados,
        // 221..=268, NÃO os base 3/6/12...). Não ter/equipar (isso completaria
        // sem craftar).
        if self.tutorial_mode
            && shared::constants::equip_slot_of(craft_output_id) == Some(shared::EquipSlot::Weapon)
        {
            let on_903 = self.sessions.get(&sid).map(|s| s.quests.iter().any(|c|
                c.quest_id == 903 && c.status == shared::quests::quest_status::ACTIVE)).unwrap_or(false);
            if on_903 { self.tutorial_advance(sid, 903); }
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
        let stack_max = crate::economy::item_stack_max(src.item_id);
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
        let stack_max = crate::economy::item_stack_max(src.item_id);
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

    /// Spawna uma lista de (item_id, qty) como loots no mundo em `pos`,
    /// espalhados num anel ao redor de `pos`. `spread_max` define o raio
    /// externo do anel; cada drop fica em [0.3·spread_max, spread_max] com
    /// ângulo distribuído + jitter pra evitar empilhamento. Posições que
    /// caem em WALL/WATER são realocadas pra tile walkable mais próximo.
    /// `kind_id` é usado pro `loot_item_level` e log; passe 0 quando não
    /// houver enemy_kind (player death, farm node).
    fn spawn_loot_drops(&mut self, pos: Vec2, drops: &[(u16, u32)], seed: u64, spread_max: f32, kind_id: u16) {
        let n = drops.len().max(1);
        let r_min = spread_max * 0.3;
        let r_jit = spread_max * 0.7;
        let now = self.sim_time_s;
        for (i, (item_id, qty)) in drops.iter().enumerate() {
            if *qty == 0 { continue; }
            let loot_id = self.alloc_entity_id();
            let a = (i as f32 / n as f32) * std::f32::consts::TAU
                + lcg_f32(seed ^ (*item_id as u64)) * 0.6;
            let r = r_min + lcg_f32(seed ^ (i as u64)) * r_jit;
            let mut offset = Vec2::new(a.cos(), a.sin()) * r;
            // Reje­ita offsets em tile não-walkable; tenta raios menores
            // e em último caso cai no `pos` original.
            let walkable = |p: Vec2, m: &shared::world_gen::WorldMap| {
                m.is_walkable(p.x.floor() as i32, p.y.floor() as i32)
            };
            if !walkable(pos + offset, &self.map) {
                let mut found = false;
                for k in 1..6u32 {
                    let shrink = 1.0 - (k as f32) * 0.15;
                    let try_off = Vec2::new(a.cos(), a.sin()) * (r * shrink);
                    if walkable(pos + try_off, &self.map) {
                        offset = try_off; found = true; break;
                    }
                }
                if !found {
                    // 8 direções a meio raio do anel
                    for d in 0..8 {
                        let ang = (d as f32) * std::f32::consts::TAU / 8.0;
                        let try_off = Vec2::new(ang.cos(), ang.sin()) * (spread_max * 0.4);
                        if walkable(pos + try_off, &self.map) {
                            offset = try_off; found = true; break;
                        }
                    }
                }
                if !found { offset = Vec2::ZERO; }
            }
            let item_lvl = crate::economy::loot_item_level(kind_id, *item_id);
            let instance = shared::items::ItemInstance::roll_with_template(
                crate::economy::item_template_of(*item_id),
                item_lvl,
                || fastrand::f32(),
            );
            self.ecs.spawn((
                NetId(loot_id),
                Position(pos + offset),
                Velocity(Vec2::ZERO),
                EntityKind::Loot(*item_id),
                LootTag { item_id: *item_id, qty: *qty, instance, spawn_at: now },
            ));
            if let Some(ctx) = &self.auth_ctx {
                let r = instance.map(|i| i.rarity).unwrap_or(0);
                let rf = instance.map(|i| i.refinement).unwrap_or(0);
                crate::persistence::log_drop(
                    ctx.pool.clone(), kind_id, *item_id, *qty, r, item_lvl, rf,
                );
            }
        }
    }

    /// Retorna nomes dos membros de uma party_id.
    fn party_members(&self, pid: u32) -> Vec<String> {
        self.sessions.values()
            .filter(|s| s.party_id == Some(pid) && s.logged_in)
            .map(|s| s.name.clone())
            .collect()
    }

    /// Envia PartyUpdate pra todos os membros da party.
    fn broadcast_party_update(&self, pid: u32) {
        let members = self.party_members(pid);
        for s in self.sessions.values() {
            if s.party_id == Some(pid) {
                let _ = s.handle.to_client.send(ServerMessage::PartyUpdate {
                    members: members.clone(),
                });
            }
        }
    }

    fn handle_party_invite(&mut self, sid: SessionId, target_name: String) {
        let Some(inviter) = self.sessions.get(&sid) else { return };
        if !inviter.logged_in { return; }
        let inviter_name = inviter.name.clone();
        if inviter_name.eq_ignore_ascii_case(&target_name) { return; }
        // Procura o alvo
        let target_sid = self.sessions.iter()
            .find(|(_, s)| s.logged_in && s.name.eq_ignore_ascii_case(&target_name))
            .map(|(k, _)| *k);
        let Some(target_sid) = target_sid else {
            if let Some(s) = self.sessions.get(&sid) {
                let _ = s.handle.to_client.send(ServerMessage::Chat {
                    from: "PARTY".into(),
                    text: format!("player {target_name} is not online"),
                });
            }
            return;
        };
        if let Some(ts) = self.sessions.get_mut(&target_sid) {
            ts.party_invite_from = Some(inviter_name.clone());
            let _ = ts.handle.to_client.send(ServerMessage::PartyInviteReceived {
                from: inviter_name,
            });
        }
    }

    fn handle_party_accept(&mut self, sid: SessionId) {
        let (invite_from, accepter_name) = {
            let Some(s) = self.sessions.get(&sid) else { return };
            let Some(from) = s.party_invite_from.clone() else { return };
            (from, s.name.clone())
        };
        // Find inviter session
        let inviter_sid = self.sessions.iter()
            .find(|(_, s)| s.name == invite_from && s.logged_in)
            .map(|(k, _)| *k);
        let Some(inviter_sid) = inviter_sid else {
            if let Some(s) = self.sessions.get_mut(&sid) {
                s.party_invite_from = None;
            }
            return;
        };
        // Descobre party_id (cria uma nova se inviter nao tem)
        let party_id = match self.sessions.get(&inviter_sid).and_then(|s| s.party_id) {
            Some(id) => id,
            None => {
                let id = self.next_party_id;
                self.next_party_id = self.next_party_id.wrapping_add(1).max(1);
                if let Some(s) = self.sessions.get_mut(&inviter_sid) {
                    s.party_id = Some(id);
                }
                id
            }
        };
        // Accepter entra na party
        if let Some(s) = self.sessions.get_mut(&sid) {
            s.party_id = Some(party_id);
            s.party_invite_from = None;
        }
        tracing::info!("{} entrou na party de {}", accepter_name, invite_from);
        self.broadcast_party_update(party_id);
    }

    fn handle_party_leave(&mut self, sid: SessionId) {
        let old_party = match self.sessions.get_mut(&sid) {
            Some(s) => {
                let p = s.party_id;
                s.party_id = None;
                let _ = s.handle.to_client.send(ServerMessage::PartyUpdate { members: vec![] });
                p
            }
            None => return,
        };
        if let Some(pid) = old_party {
            // Se sobrou so 1 membro, dissolve.
            let remaining = self.party_members(pid);
            if remaining.len() <= 1 {
                for s in self.sessions.values_mut() {
                    if s.party_id == Some(pid) {
                        s.party_id = None;
                        let _ = s.handle.to_client.send(ServerMessage::PartyUpdate { members: vec![] });
                    }
                }
            } else {
                self.broadcast_party_update(pid);
            }
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

    fn handle_teleport_to_vendor(&mut self, sid: SessionId) {
        let Some(session) = self.sessions.get(&sid) else { return };
        if !session.logged_in || session.downed { return; }
        let Some(player_entity) = session.entity else { return };

        // Acha a posição do NPC vendor (npc_id=1)
        let vendor_pos = self
            .ecs
            .query::<(&Position, &EntityKind)>()
            .iter()
            .find_map(|(_, (p, k))| {
                if matches!(k, EntityKind::Npc(1)) { Some(p.0) } else { None }
            });

        let Some(vpos) = vendor_pos else { return };
        let dest = vpos + glam::Vec2::new(1.0, 0.0);

        if let Ok(mut pos) = self.ecs.get::<&mut Position>(player_entity) {
            pos.0 = dest;
        }
        tracing::info!("{} teletransportado para loja {:?}", session.name, dest);
    }

    /// Teletransporta o player para o spawn do mapa. Escape de bug
    /// (player preso em wall, fora do mapa). Permite mesmo se montado: o
    /// barco vira item de volta no inventário antes do tp. Zera velocidade
    /// e força save imediato.
    fn handle_reset_position(&mut self, sid: SessionId) {
        let (player_entity, name, was_mounted) = {
            let Some(session) = self.sessions.get(&sid) else { return };
            if !session.logged_in { return; }
            let Some(pe) = session.entity else { return };
            let mounted = self.ecs.get::<&Mounted>(pe).is_ok();
            (pe, session.name.clone(), mounted)
        };

        if was_mounted {
            // Coleta dados do mount, devolve item, despawna barco — mesma
            // logica do dismount mas sem BFS (vai pro spawn de qq jeito).
            let (boat_entity, boat_eid, boat_kind) = {
                let Ok(m) = self.ecs.get::<&Mounted>(player_entity) else {
                    return;
                };
                let kind = self.ecs.get::<&EntityKind>(m.boat_entity)
                    .ok()
                    .and_then(|k| if let EntityKind::Boat(n) = *k { Some(n) } else { None })
                    .unwrap_or(0);
                (m.boat_entity, m.boat_eid, kind)
            };
            let _ = self.ecs.remove_one::<Mounted>(player_entity);
            let _ = self.ecs.despawn(boat_entity);
            self.removed_this_tick.push(boat_eid);
            let item_id = match boat_kind {
                _ => shared::item_id::BOAT_LYLIAN_LEUTARD,
            };
            if let Some(session) = self.sessions.get_mut(&sid) {
                for slot in session.inventory.iter_mut() {
                    if slot.qty == 0 {
                        *slot = shared::InventorySlot { item_id, qty: 1, instance: None };
                        session.inventory_dirty = true;
                        break;
                    }
                }
            }
        }

        let spawn_tile = self.map.spawn_tile();
        let dest = Vec2::new(spawn_tile.0 as f32 + 0.5, spawn_tile.1 as f32 + 0.5);
        if let Ok(mut pos) = self.ecs.get::<&mut Position>(player_entity) {
            pos.0 = dest;
        }
        if let Ok(mut v) = self.ecs.get::<&mut Velocity>(player_entity) {
            v.0 = Vec2::ZERO;
        }
        self.save_pending = true;
        tracing::info!("{} reset position → spawn {:?}", name, dest);
    }

    // ============================ QUESTS ============================

    /// Facção da sessão -> id usado nas quests (1=Morganeers, 2=Peacemain).
    fn faction_qid(f: shared::Faction) -> u8 {
        match f {
            shared::Faction::Morganeers => shared::quests::faction_id::MORGANEERS,
            shared::Faction::Peacemain  => shared::quests::faction_id::PEACEMAIN,
        }
    }

    /// COLLECT/DELIVER são checados/consumidos no turn-in; resto é live-track.
    fn quest_is_turnin_objective(obj_kind: u8) -> bool {
        obj_kind == shared::quests::objective_kind::COLLECT
            || obj_kind == shared::quests::objective_kind::DELIVER
    }

    /// Log das quests ativas + pontos de facção (login + ressync).
    pub fn send_quest_log(&self, sid: SessionId) {
        let Some(s) = self.sessions.get(&sid) else { return };
        let active: Vec<shared::quests::QuestNet> = s.quests.iter()
            .filter(|c| c.status != shared::quests::quest_status::TURNED_IN)
            .filter_map(|c| shared::quests::quest_by_id(c.quest_id)
                .map(|d| shared::quests::QuestNet::from_def(d, c.status, c.progress)))
            .collect();
        let _ = s.handle.to_client.send(ServerMessage::QuestLog { quests: active });
        let _ = s.handle.to_client.send(ServerMessage::FactionPoints { points: s.faction_points });
    }

    /// Joga item do slot do inventario no chao perto do player. Spawna
    /// LootTag com a instance EXATA do slot (preserva rarity/refinement),
    /// zera o slot e marca save pendente. Drop no proximo tile walkable
    /// num pequeno offset (~1 tile) do player.
    fn handle_drop_item(&mut self, sid: SessionId, slot: u16) {
        let (player_entity, item_id, qty, instance) = {
            let Some(s) = self.sessions.get(&sid) else { return };
            if !s.logged_in { return; }
            let Some(pe) = s.entity else { return };
            let idx = slot as usize;
            if idx >= s.inventory.len() { return; }
            let inv = &s.inventory[idx];
            if inv.item_id == 0 || inv.qty == 0 { return; }
            (pe, inv.item_id, inv.qty, inv.instance)
        };
        let pos = match self.ecs.get::<&Position>(player_entity) {
            Ok(p) => p.0,
            Err(_) => return,
        };
        // Acha tile walkable proximo (1 tile na direcao S ou nas 8 cardinais).
        let pick = {
            let candidates = [
                Vec2::new(0.0, -1.0), Vec2::new(1.0, -1.0), Vec2::new(-1.0, -1.0),
                Vec2::new(1.0, 0.0), Vec2::new(-1.0, 0.0),
                Vec2::new(0.0, 1.0), Vec2::new(1.0, 1.0), Vec2::new(-1.0, 1.0),
            ];
            let mut chosen = pos;
            for c in candidates {
                let p = pos + c;
                if self.map.is_walkable(p.x.floor() as i32, p.y.floor() as i32) {
                    chosen = p;
                    break;
                }
            }
            chosen
        };
        let loot_eid = self.alloc_entity_id();
        let now = self.sim_time_s;
        self.ecs.spawn((
            NetId(loot_eid),
            Position(pick),
            Velocity(Vec2::ZERO),
            EntityKind::Loot(item_id),
            LootTag { item_id, qty, instance, spawn_at: now },
        ));
        // Zera o slot e marca save pendente.
        if let Some(s) = self.sessions.get_mut(&sid) {
            let idx = slot as usize;
            if idx < s.inventory.len() {
                s.inventory[idx] = shared::InventorySlot::default();
                s.inventory_dirty = true;
            }
        }
        self.save_pending = true;
    }

    /// Givers com quest aceitável agora — cliente mostra o "!" só sobre esses.
    pub fn send_quest_givers(&self, sid: SessionId) {
        let now = (now_ms() / 1000) as i64;
        let xpmult = crate::economy::xp_multiplier();
        let Some(s) = self.sessions.get(&sid) else { return };
        let level = shared::level_of_xp_with_mult(s.xp, xpmult);
        let fac = Self::faction_qid(s.faction);
        let available = crate::quests::available_givers(level, fac, &s.quests, now);
        let _ = s.handle.to_client.send(ServerMessage::QuestGivers { available });
    }

    /// Oferta de quests de um giver (chamado pelo handle_interact ao chegar perto).
    fn send_quest_offer(&self, sid: SessionId, giver_source: u8, giver_id: u16, giver_name: String) {
        let now = (now_ms() / 1000) as i64;
        let xpmult = crate::economy::xp_multiplier();
        let Some(s) = self.sessions.get(&sid) else { return };
        let level = shared::level_of_xp_with_mult(s.xp, xpmult);
        let fac = Self::faction_qid(s.faction);
        let mut offer: Vec<shared::quests::QuestNet> =
            crate::quests::offerable(giver_source, giver_id, level, fac, &s.quests, now)
                .into_iter()
                .map(|d| shared::quests::QuestNet::from_def(d, shared::quests::quest_status::ACTIVE, 0))
                .collect();
        // Inclui as repetíveis deste giver que estão EM COOLDOWN, pra o painel
        // mostrar a contagem regressiva (em vez de a quest simplesmente sumir).
        for cq in s.quests.iter() {
            if cq.status != shared::quests::quest_status::TURNED_IN || cq.cooldown_until <= now { continue; }
            let Some(def) = shared::quests::quest_by_id(cq.quest_id) else { continue };
            if def.source != giver_source || def.giver != giver_id || !def.repeatable { continue; }
            let mut qn = shared::quests::QuestNet::from_def(def, shared::quests::quest_status::TURNED_IN, def.obj_count);
            qn.cooldown_until = cq.cooldown_until;
            offer.push(qn);
        }
        let _ = s.handle.to_client.send(ServerMessage::QuestOffer { giver_source, giver_id, giver_name, quests: offer });
    }

    fn handle_accept_quest(&mut self, sid: SessionId, quest_id: u16) {
        let now = (now_ms() / 1000) as i64;
        let Some(def) = shared::quests::quest_by_id(quest_id) else { return };
        let xpmult = crate::economy::xp_multiplier();
        let Some(s) = self.sessions.get_mut(&sid) else { return };
        if !s.logged_in { return; }
        let level = shared::level_of_xp_with_mult(s.xp, xpmult);
        if level < def.min_level { return; }
        let fac = Self::faction_qid(s.faction);
        if def.faction != shared::quests::faction_id::NONE && def.faction != fac { return; }
        if let Some(c) = s.quests.iter().find(|c| c.quest_id == quest_id) {
            if c.status != shared::quests::quest_status::TURNED_IN { return; }   // já ativa/pronta
            if !def.repeatable || now < c.cooldown_until { return; }            // concluída/cooldown
        }
        let active_count = s.quests.iter()
            .filter(|c| c.status == shared::quests::quest_status::ACTIVE
                     || c.status == shared::quests::quest_status::READY)
            .count();
        if active_count >= 12 { return; } // limite de quests ativas
        let st = shared::quests::quest_status::ACTIVE;
        if let Some(c) = s.quests.iter_mut().find(|c| c.quest_id == quest_id) {
            c.status = st; c.progress = 0; c.cooldown_until = 0;
        } else {
            s.quests.push(crate::quests::CharQuest { quest_id, status: st, progress: 0, cooldown_until: 0 });
        }
        s.quests_dirty = true;
        let _ = s.handle.to_client.send(ServerMessage::QuestUpdate { quest_id, progress: 0, status: st });
    }

    fn handle_abandon_quest(&mut self, sid: SessionId, quest_id: u16) {
        let Some(s) = self.sessions.get_mut(&sid) else { return };
        let before = s.quests.len();
        s.quests.retain(|c| c.quest_id != quest_id);
        if s.quests.len() != before {
            s.quests_dirty = true; // save_char reconcilia (deleta linhas ausentes)
            let _ = s.handle.to_client.send(ServerMessage::QuestUpdate { quest_id, progress: 0, status: 255 });
        }
    }

    fn handle_turn_in_quest(&mut self, sid: SessionId, quest_id: u16) {
        let now = (now_ms() / 1000) as i64;
        let Some(def) = shared::quests::quest_by_id(quest_id) else { return };
        let Some(s) = self.sessions.get_mut(&sid) else { return };
        if !s.logged_in { return; }
        let Some(cq) = s.quests.iter().find(|c| c.quest_id == quest_id).cloned() else { return };
        if cq.status == shared::quests::quest_status::TURNED_IN { return; }
        if Self::quest_is_turnin_objective(def.obj_kind) {
            let have: u32 = s.inventory.iter()
                .filter(|sl| sl.item_id == def.obj_target && sl.instance.is_none())
                .map(|sl| sl.qty).sum();
            if have < def.obj_count { return; } // não tem o suficiente
            let mut need = def.obj_count;
            for sl in s.inventory.iter_mut() {
                if need == 0 { break; }
                if sl.item_id != def.obj_target || sl.instance.is_some() || sl.qty == 0 { continue; }
                let take = need.min(sl.qty);
                sl.qty -= take; need -= take;
                if sl.qty == 0 { *sl = shared::InventorySlot::default(); }
            }
            s.inventory_dirty = true;
        } else if cq.status != shared::quests::quest_status::READY {
            return; // live-track ainda não concluído
        }
        // Recompensas
        if def.reward_gold > 0 { s.gold = s.gold.saturating_add(def.reward_gold as u64); }
        if def.reward_xp > 0 { s.grant_xp(def.reward_xp); }
        if def.reward_faction_points > 0 { s.faction_points = s.faction_points.saturating_add(def.reward_faction_points); }
        if def.reward_item != 0 && def.reward_item_qty > 0 {
            add_to_inventory(&mut s.inventory, def.reward_item, def.reward_item_qty as u32, None);
            s.inventory_dirty = true;
        }
        let cd = if def.repeatable {
            if def.daily { ((now / 86400) + 1) * 86400 } // próxima meia-noite UTC
            else { now + def.cooldown_secs as i64 }
        } else { i64::MAX };
        let new_status = shared::quests::quest_status::TURNED_IN;
        if let Some(c) = s.quests.iter_mut().find(|c| c.quest_id == quest_id) {
            c.status = new_status; c.progress = def.obj_count; c.cooldown_until = cd;
        }
        s.quests_dirty = true;
        let pts = s.faction_points;
        let h = s.handle.clone();
        let _ = h.to_client.send(ServerMessage::QuestUpdate { quest_id, progress: def.obj_count, status: new_status });
        let _ = h.to_client.send(ServerMessage::FactionPoints { points: pts });
    }

    /// Hook de KILL/PVP_KILL. `pvp_victim_faction`: Some(f) se foi PvP, None se mob.
    fn quest_on_kill(&mut self, killer_eid: EntityId, pvp_victim_faction: Option<u8>) {
        let mut updates: Vec<(u16, u32, u8)> = Vec::new();
        let handle = {
            let Some(s) = self.sessions.values_mut()
                .find(|s| s.entity_id == killer_eid && s.logged_in) else { return };
            for c in s.quests.iter_mut() {
                if c.status != shared::quests::quest_status::ACTIVE { continue; }
                let Some(def) = shared::quests::quest_by_id(c.quest_id) else { continue };
                let hit = if def.obj_kind == shared::quests::objective_kind::KILL {
                    pvp_victim_faction.is_none()
                } else if def.obj_kind == shared::quests::objective_kind::PVP_KILL {
                    pvp_victim_faction.map(|vf| vf != def.faction).unwrap_or(false)
                } else { false };
                if !hit { continue; }
                c.progress = (c.progress + 1).min(def.obj_count);
                if c.progress >= def.obj_count { c.status = shared::quests::quest_status::READY; }
                updates.push((c.quest_id, c.progress, c.status));
            }
            if updates.is_empty() { return; }
            s.quests_dirty = true;
            s.handle.clone()
        };
        for (qid, pr, st) in updates {
            let _ = handle.to_client.send(ServerMessage::QuestUpdate { quest_id: qid, progress: pr, status: st });
        }
    }

    /// Tutorial (TUTORIAL_MODE): concede a 1a quest da cadeia, avança quando o
    /// objetivo é cumprido (READY) e finaliza ao concluir a última. Mantém 1
    /// boneco de treino vivo na ilha pra quest de combate.
    /// IDs das armas T1 craftáveis — usado pra detectar "forjou uma arma" (903).
    const TUTORIAL_T1_WEAPONS: [u16; 8] = [
        shared::constants::item_id::SWORD, shared::constants::item_id::STAFF,
        shared::constants::item_id::DAGGER, shared::constants::item_id::GREAT_SWORD,
        shared::constants::item_id::BOW, shared::constants::item_id::WAND,
        shared::constants::item_id::AXE, shared::constants::item_id::SPEAR,
    ];

    fn tick_tutorial_quests(&mut self) {
        use shared::constants::item_id;
        // Arena de combate (quest 904 = matar 3): mantém 2 inimigos lvl 1
        // espaçados nos TUTORIAL_ENEMY_SPOTS enquanto alguém está em 904/ACTIVE.
        // Quando um morre, o tick respawna no spot livre. Despawna todos quando
        // ninguém está em 904 (pra não atacar o player antes da hora).
        let anyone_on_combat = self.sessions.values().any(|s| s.logged_in
            && s.quests.iter().any(|c| c.quest_id == 904
                && c.status == shared::quests::quest_status::ACTIVE));
        if anyone_on_combat {
            for (sx, sy) in TUTORIAL_ENEMY_SPOTS.iter() {
                let spot = Vec2::new(*sx, *sy);
                let occupied = self.ecs.query::<(&Position, &EnemyTag)>().iter()
                    .any(|(_, (p, _))| p.0.distance(spot) < 4.0);
                if !occupied { self.place_tutorial_enemy(spot); }
            }
        } else {
            let mobs: Vec<(hecs::Entity, EntityId)> = self.ecs
                .query::<(&NetId, &EnemyTag)>().iter().map(|(e, (n, _))| (e, n.0)).collect();
            for (e, eid) in mobs {
                self.free_entity_body(e);
                let _ = self.ecs.despawn(e);
                self.removed_this_tick.push(eid);
            }
        }

        // (Barco NÃO é mais pré-spawnado — o player ganha o ITEM na 905 e USA
        // ele na costa pra colocar na água; ver tutorial_advance + handle_use_item.)

        let sids: Vec<SessionId> = self.sessions.keys().copied().collect();
        for sid in sids {
            let (has_tut, cur, entity) = match self.sessions.get(&sid) {
                Some(s) if s.logged_in => {
                    let has = s.quests.iter().any(|c| shared::quests::is_tutorial_quest(c.quest_id));
                    let cur = s.quests.iter()
                        .find(|c| shared::quests::is_tutorial_quest(c.quest_id)
                               && c.status != shared::quests::quest_status::TURNED_IN)
                        .map(|c| (c.quest_id, c.status));
                    (has, cur, s.entity)
                }
                _ => continue,
            };
            if !has_tut {
                self.tutorial_give_starter_kit(sid);
                self.tutorial_grant_quest(sid, shared::quests::tutorial_first());
                continue;
            }
            let Some((qid, status)) = cur else { continue };
            // 902: atualiza o progresso (madeira coletada) pro HUD; a CONCLUSÃO é
            // por interação com o Matteo (entrega) — ver tutorial_npc_interact.
            if qid == 902 {
                let wood: u32 = self.sessions.get(&sid).map(|s| s.inventory.iter()
                    .filter(|sl| sl.item_id == item_id::WOOD_T1).map(|sl| sl.qty).sum()).unwrap_or(0);
                let pr = wood.min(5);
                if let Some(s) = self.sessions.get_mut(&sid) {
                    if let Some(c) = s.quests.iter_mut().find(|c| c.quest_id == 902) {
                        if c.progress != pr {
                            c.progress = pr; s.quests_dirty = true;
                            let st = c.status;
                            let _ = s.handle.to_client.send(ServerMessage::QuestUpdate {
                                quest_id: 902, progress: pr, status: st });
                        }
                    }
                }
                continue;
            }
            // 905: garante que o player TEM o item do barco (cobre relog e a
            // transição do fluxo antigo que spawnava o barco).
            if qid == 905 {
                let has_boat = self.sessions.get(&sid).map(|s| s.inventory.iter()
                    .any(|sl| shared::constants::is_boat_item(sl.item_id) && sl.qty > 0)).unwrap_or(false);
                if !has_boat {
                    if let Some(s) = self.sessions.get_mut(&sid) {
                        add_to_inventory(&mut s.inventory, shared::constants::item_id::BOAT_LYLIAN_LEUTARD, 1, None);
                        s.inventory_dirty = true;
                    }
                }
            }
            let complete = match qid {
                // 900 (falar com Matteo) → concluída via interação, não aqui.
                900 => false,
                // 901: equipou o machado no slot de ferramenta.
                901 => self.sessions.get(&sid)
                    .map(|s| s.equipment.get(shared::EquipSlot::ToolAxe) == Some(item_id::WOODCUTTER_AXE_T1))
                    .unwrap_or(false),
                // 903: concluída pelo GATILHO do craft (handle_craft), não aqui.
                903 => false,
                // 904: derrotou o mob (motor marca READY no kill).
                904 => status == shared::quests::quest_status::READY,
                // 905: usou o item do barco (deploy) → handle_use_item, não aqui.
                905 => false,
                // 906/907: montado e PERTO da estação (vela/leme) no convés —
                // detecção DECK-RELATIVA (local_pos), então funciona em qualquer
                // lugar/movimento do barco.
                906 => self.tutorial_at_boat_station(entity, true),
                907 => self.tutorial_at_boat_station(entity, false),
                // 908: navegou até o mar aberto (motor marca READY no EXPLORE).
                908 => status == shared::quests::quest_status::READY,
                _ => false,
            };
            if complete { self.tutorial_advance(sid, qid); }
        }
    }

    /// True se o player (montado) está PERTO da estação do convés (vela ou leme).
    /// Deck-relativo: usa local_pos vs a posição da estação no boat_config, então
    /// independe de onde o barco está/anda.
    fn tutorial_at_boat_station(&self, entity: Option<Entity>, sail: bool) -> bool {
        let Some(e) = entity else { return false };
        let Ok(m) = self.ecs.get::<&Mounted>(e) else { return false };
        let kind = self.ecs.get::<&BoatTag>(m.boat_entity).map(|t| t.kind).unwrap_or(0);
        let station = if sail { boat_sail_local(kind) } else { boat_helm_local(kind) };
        m.local_pos.distance(station) < 1.8
    }

    /// Kit inicial do tutorial: NADA. O player nasce sem arma (criação) e sem
    /// ferramentas — o machado vem do Matteo (quest 900) e os materiais de craft
    /// na entrega da madeira (quest 902). Mantida pra compat com o call-site.
    fn tutorial_give_starter_kit(&mut self, _sid: SessionId) {}

    /// Interação com o Matteo (NPC guia) DENTRO do tutorial: dá o machado ao
    /// falar (900), recebe a madeira e entrega os materiais de craft (902), ou
    /// dá uma dica contextual. Cada conclusão dispara um balão de diálogo.
    fn tutorial_npc_interact(&mut self, sid: SessionId) {
        use shared::constants::item_id;
        let cur = self.sessions.get(&sid).and_then(|s| s.quests.iter()
            .find(|c| shared::quests::is_tutorial_quest(c.quest_id)
                   && c.status != shared::quests::quest_status::TURNED_IN)
            .map(|c| c.quest_id));
        let Some(qid) = cur else { return };
        match qid {
            900 => {
                if let Some(s) = self.sessions.get_mut(&sid) {
                    add_to_inventory(&mut s.inventory, item_id::WOODCUTTER_AXE_T1, 1, None);
                    s.inventory_dirty = true;
                }
                self.tutorial_say(sid, "Matteo", "Boa, pulou a pedra direitinho! Toma esse machado. Abre o inventario e EQUIPA ele, depois corta umas arvores ali e me traz 5 madeiras.");
                self.tutorial_advance(sid, 900);
            }
            902 => {
                let wood: u32 = self.sessions.get(&sid).map(|s| s.inventory.iter()
                    .filter(|sl| sl.item_id == item_id::WOOD_T1).map(|sl| sl.qty).sum()).unwrap_or(0);
                if wood < 5 {
                    self.tutorial_say(sid, "Matteo", "Ainda falta madeira. Corta as arvores ate juntar 5 e volta aqui.");
                    return;
                }
                if let Some(s) = self.sessions.get_mut(&sid) {
                    // Consome 5 madeiras e devolve um kit de craft completo (dá pra
                    // forjar QUALQUER arma T1: 8 madeira / 5 couro / 7 minério).
                    let mut need = 5u32;
                    for sl in s.inventory.iter_mut() {
                        if need == 0 { break; }
                        if sl.item_id != item_id::WOOD_T1 || sl.qty == 0 { continue; }
                        let take = need.min(sl.qty); sl.qty -= take; need -= take;
                        if sl.qty == 0 { *sl = shared::InventorySlot::default(); }
                    }
                    add_to_inventory(&mut s.inventory, item_id::WOOD_T1, 8, None);
                    add_to_inventory(&mut s.inventory, item_id::LEATHER_T1, 5, None);
                    add_to_inventory(&mut s.inventory, item_id::MINERAL_T1, 7, None);
                    s.inventory_dirty = true;
                }
                self.tutorial_say(sid, "Matteo", "Otimo! Aqui, leva esse material. Vai na estacao de craft e forja a arma T1 que voce quiser.");
                self.tutorial_advance(sid, 902);
            }
            _ => {
                let hint = match qid {
                    901 => "Equipa o machado pelo inventario que ai a gente continua.",
                    903 => "Vai na estacao de craft (a bigorna/bancada ali) e forja sua arma. Os materiais ja sao seus.",
                    904 => "Cuidado, marujo! Derrota aquele inimigo ali primeiro.",
                    905 => "Sua jornada comeca agora. Sobe no barco e zarpa pro mundo!",
                    _ => "Continua firme, marujo.",
                };
                self.tutorial_say(sid, "Matteo", hint);
            }
        }
    }

    /// Envia um balão de diálogo do tutorial pro client.
    fn tutorial_say(&self, sid: SessionId, speaker: &str, text: &str) {
        if let Some(s) = self.sessions.get(&sid) {
            let _ = s.handle.to_client.send(ServerMessage::TutorialSay {
                speaker: speaker.to_string(), text: text.to_string() });
        }
    }

    fn tutorial_grant_quest(&mut self, sid: SessionId, qid: u16) {
        if let Some(s) = self.sessions.get_mut(&sid) {
            if let Some(c) = s.quests.iter_mut().find(|c| c.quest_id == qid) {
                c.status = shared::quests::quest_status::ACTIVE; c.progress = 0; c.cooldown_until = 0;
            } else {
                s.quests.push(crate::quests::CharQuest {
                    quest_id: qid, status: shared::quests::quest_status::ACTIVE, progress: 0, cooldown_until: 0 });
            }
            s.quests_dirty = true;
            let _ = s.handle.to_client.send(ServerMessage::QuestUpdate {
                quest_id: qid, progress: 0, status: shared::quests::quest_status::ACTIVE });
        }
        self.send_quest_log(sid);
    }

    fn tutorial_advance(&mut self, sid: SessionId, qid: u16) {
        let count = shared::quests::quest_by_id(qid).map(|d| d.obj_count).unwrap_or(1);
        let reward_xp = shared::quests::quest_by_id(qid).map(|d| d.reward_xp).unwrap_or(0);
        if let Some(s) = self.sessions.get_mut(&sid) {
            if let Some(c) = s.quests.iter_mut().find(|c| c.quest_id == qid) {
                c.status = shared::quests::quest_status::TURNED_IN; c.progress = count; c.cooldown_until = i64::MAX;
            }
            if reward_xp > 0 { s.grant_xp(reward_xp); }
            // Quest de combate (904) GARANTE level 2 — "sobe o personagem pro lvl 2".
            if qid == 904 {
                let lvl2 = shared::constants::xp_for_level_with_mult(2, crate::economy::xp_multiplier());
                if s.xp < lvl2 { s.grant_xp(lvl2 - s.xp); }
            }
            s.quests_dirty = true;
            let _ = s.handle.to_client.send(ServerMessage::QuestUpdate {
                quest_id: qid, progress: count, status: shared::quests::quest_status::TURNED_IN });
        }
        match shared::quests::tutorial_next(qid) {
            Some(next) => {
                self.tutorial_grant_quest(sid, next);
                if next == 905 {
                    // Recompensa: o ITEM do barco. O player vai à costa e USA o
                    // item pra colocá-lo na água (handle_use_item).
                    if let Some(s) = self.sessions.get_mut(&sid) {
                        add_to_inventory(&mut s.inventory, shared::constants::item_id::BOAT_LYLIAN_LEUTARD, 1, None);
                        s.inventory_dirty = true;
                    }
                    self.tutorial_say(sid, "Matteo", "Voce esta pronto, marujo! Toma o teu barco. Vai ate o cais ao norte e USA ele no inventario pra colocar na agua. Depois sobe e aprende a navegar!");
                }
                // Explicações do barco — disparam ao CHEGAR em cada estação.
                if next == 907 {
                    self.tutorial_say(sid, "Matteo", "Essa e a VELA. Clique nela pra alternar: ERGUIDA (parado) → MEIA (50%) → CHEIA (100%). Mais vela = mais velocidade (com vento a favor).");
                }
                if next == 908 {
                    self.tutorial_say(sid, "Matteo", "Esse e o LEME. Segura e arrasta pra girar o barco esquerda/direita. Combine VELA (velocidade) + LEME (direcao) pra navegar ate o mar aberto!");
                }
            }
            None => self.handle_finish_tutorial(sid), // última → finaliza o tutorial
        }
    }

    /// Hook por tick: quests EXPLORE viram READY quando o player entra na área.
    fn tick_quest_explore(&mut self) {
        // Pré-coleta posições dos players (evita conflito de borrow com sessions).
        let mut ppos: std::collections::HashMap<EntityId, Vec2> = std::collections::HashMap::new();
        // Pro player MONTADO, "chegar no alvo" é o BARCO chegar — não a pos do
        // player no deck (que tem offset do local_pos). Pré-coleta pos dos barcos.
        let mut boatpos: std::collections::HashMap<Entity, Vec2> = std::collections::HashMap::new();
        for (be, (p, _)) in self.ecs.query::<(&Position, &BoatTag)>().iter() {
            boatpos.insert(be, p.0);
        }
        let mut mounted_boat: std::collections::HashMap<EntityId, Entity> = std::collections::HashMap::new();
        for (_, (net, m)) in self.ecs.query::<(&NetId, &Mounted)>().iter() {
            mounted_boat.insert(net.0, m.boat_entity);
        }
        for (_, (net, p, _)) in self.ecs.query::<(&NetId, &Position, &PlayerTag)>().iter() {
            let pos = mounted_boat.get(&net.0)
                .and_then(|be| boatpos.get(be).copied())
                .unwrap_or(p.0);
            ppos.insert(net.0, pos);
        }
        if ppos.is_empty() { return; }
        let mut updates: Vec<(SessionHandle, u16, u32, u8)> = Vec::new();
        for s in self.sessions.values_mut() {
            if !s.logged_in { continue; }
            let Some(pos) = ppos.get(&s.entity_id).copied() else { continue };
            // 1) Determina quais quests EXPLORE/TRANSPORT completam (sem mutar ainda).
            //    (quest_id, is_transport, obj_target, obj_count)
            let mut to_complete: Vec<(u16, bool, u16, u32)> = Vec::new();
            for c in s.quests.iter() {
                if c.status != shared::quests::quest_status::ACTIVE { continue; }
                let Some(def) = shared::quests::quest_by_id(c.quest_id) else { continue };
                let is_explore   = def.obj_kind == shared::quests::objective_kind::EXPLORE;
                let is_transport = def.obj_kind == shared::quests::objective_kind::TRANSPORT;
                if !is_explore && !is_transport { continue; }
                if pos.distance(Vec2::new(def.obj_x, def.obj_y)) > def.obj_radius.max(1.0) { continue; }
                if is_transport {
                    // Precisa ter a carga ao chegar; senão espera.
                    let have: u32 = s.inventory.iter()
                        .filter(|sl| sl.item_id == def.obj_target && sl.instance.is_none())
                        .map(|sl| sl.qty).sum();
                    if have < def.obj_count { continue; }
                }
                to_complete.push((c.quest_id, is_transport, def.obj_target, def.obj_count));
            }
            // 2) Aplica: consome carga (transport) + marca READY.
            for (qid, is_transport, target, count) in to_complete {
                if is_transport {
                    let mut need = count;
                    for sl in s.inventory.iter_mut() {
                        if need == 0 { break; }
                        if sl.item_id != target || sl.instance.is_some() || sl.qty == 0 { continue; }
                        let take = need.min(sl.qty); sl.qty -= take; need -= take;
                        if sl.qty == 0 { *sl = shared::InventorySlot::default(); }
                    }
                    s.inventory_dirty = true;
                }
                if let Some(c) = s.quests.iter_mut().find(|c| c.quest_id == qid) {
                    c.progress = count.max(1);
                    c.status = shared::quests::quest_status::READY;
                }
                s.quests_dirty = true;
                updates.push((s.handle.clone(), qid, count.max(1), shared::quests::quest_status::READY));
            }
        }
        for (h, qid, pr, st) in updates {
            let _ = h.to_client.send(ServerMessage::QuestUpdate { quest_id: qid, progress: pr, status: st });
        }
    }

    fn handle_interact(&mut self, sid: SessionId, clicked_eid: Option<u64>) {
        let Some(session) = self.sessions.get(&sid) else { return };
        if !session.logged_in { return; }
        // Se ja esta carregando alguem: larga.
        if let Some(target_eid) = session.carrying {
            self.release_carried(sid, target_eid);
            return;
        }
        let Some(player_entity) = session.entity else { return };
        let player_pos = match self.ecs.get::<&Position>(player_entity) {
            Ok(p) => p.0,
            Err(_) => return,
        };
        let handle = session.handle.clone();
        // 1) Procura jogador downed aliado pra carregar (pre NPC)
        let r_sq = shared::INTERACT_RADIUS * shared::INTERACT_RADIUS;
        let mut closest_downed: Option<(EntityId, f32)> = None;
        for s in self.sessions.values() {
            if !s.logged_in || !s.downed { continue; }
            if s.entity_id == session.entity_id { continue; }
            if s.carried_by.is_some() { continue; }
            if let Some(e) = s.entity {
                if let Ok(p) = self.ecs.get::<&Position>(e) {
                    let d2 = p.0.distance_squared(player_pos);
                    if d2 <= r_sq {
                        if closest_downed.map(|(_, bd)| d2 < bd).unwrap_or(true) {
                            closest_downed = Some((s.entity_id, d2));
                        }
                    }
                }
            }
        }
        if let Some((target_eid, _)) = closest_downed {
            self.start_carrying(sid, target_eid);
            return;
        }

        // 2) Procura NPC. Se veio um alvo clicado, prioriza-o (dentro do alcance);
        //    senão pega o NPC mais próximo. Assim clicar no arauto bate nele, não
        //    no morador casualmente mais perto.
        let mut best: Option<(hecs::Entity, u16, u32, f32)> = None;
        for (e, (net, p, k)) in self.ecs.query::<(&NetId, &Position, &EntityKind)>().iter() {
            if let EntityKind::Npc(n) = k {
                let d2 = p.0.distance_squared(player_pos);
                if d2 > r_sq { continue; }
                if let Some(want) = clicked_eid {
                    if net.0.0 as u64 == want { best = Some((e, *n, net.0.0 as u32, 0.0)); break; }
                } else if best.map(|(_, _, _, bd)| d2 < bd).unwrap_or(true) {
                    best = Some((e, *n, net.0.0 as u32, d2));
                }
            }
        }
        match best {
            Some((_, 2, _, _)) => {
                let slots = self
                    .sessions.get(&sid)
                    .map(|s| s.vault.clone())
                    .unwrap_or_default();
                let _ = handle.to_client.send(ServerMessage::VaultOpen { slots });
            }
            Some((entity, 1, vendor_eid, _)) => {
                // Vendor — usa VendorTag.shop_id pra pegar listing específico.
                let shop_id = self.ecs.get::<&VendorTag>(entity)
                    .map(|t| t.shop_id).unwrap_or(1);
                let items: Vec<shared::protocol::ShopItem> = crate::economy::shop_listing_for(shop_id)
                    .into_iter()
                    .map(|(item_id, price)| shared::protocol::ShopItem { item_id, price })
                    .collect();
                let sell_prices: Vec<shared::protocol::SellPrice> = crate::economy::all_sell_prices()
                    .into_iter()
                    .map(|(item_id, price)| shared::protocol::SellPrice { item_id, price })
                    .collect();
                let (buy_mult, sell_mult) = crate::economy::vendor_modifiers(vendor_eid);
                let _ = handle.to_client.send(ServerMessage::ShopOpen {
                    items, sell_prices, vendor_id: vendor_eid, buy_mult, sell_mult,
                });
            }
            Some((_, 4, _, _)) => {
                // No tutorial, o "ferreiro" é o Matteo: interagir abre o diálogo
                // guiado (dá machado / recebe madeira), não a UI de craft.
                if self.tutorial_mode {
                    self.tutorial_npc_interact(sid);
                } else {
                    let _ = handle.to_client.send(ServerMessage::BlacksmithOpen);
                }
            }
            Some((entity, 3, _, _)) => {
                // Para de andar enquanto interage + pega giver/nome.
                let (giver, gname) = {
                    if let Ok(mut t) = self.ecs.get::<&mut WanderRouteTag>(entity) {
                        t.pause_until = self.sim_time_s + 5.0;
                        (t.giver, t.name.clone())
                    } else { (0u16, String::new()) }
                };
                // Sempre abre o diálogo: arauto (giver != 0) traz quests; morador
                // comum (giver 0) traz oferta vazia → diálogo com saudação.
                self.send_quest_offer(sid, shared::quests::quest_source::NPC, giver, gname);
            }
            Some((entity, 7, _, _)) => {
                // Baú de tesouro: abrir conclui a quest TREASURE + dá a relíquia.
                if let Ok(qid) = self.ecs.get::<&TreasureChestTag>(entity).map(|t| t.quest_id) {
                    self.open_treasure(sid, qid);
                    self.send_quest_givers(sid);
                }
            }
            Some((entity, 6, _, _)) => {
                // NPC de facção: dá quests PvP + loja de pontos — só pra MESMA facção.
                let npc = self.ecs.get::<&FactionGiverTag>(entity).map(|t| (t.faction, t.name.clone())).ok();
                let player_fac = self.sessions.get(&sid).map(|s| s.faction);
                if let (Some((npc_fac, npc_name)), Some(player_fac)) = (npc, player_fac) {
                    if npc_fac == player_fac {
                        self.send_quest_offer(sid, shared::quests::quest_source::FACTION,
                                              Self::faction_qid(player_fac) as u16, npc_name);
                        self.send_faction_shop(sid);
                    } else {
                        let _ = handle.to_client.send(ServerMessage::Chat {
                            from: "SYS".into(),
                            text: "Este representante não atende sua facção.".into(),
                        });
                    }
                }
            }
            Some((_, 8, _, _)) => {
                // Mestre do Treinamento — manda o player (re)fazer o tutorial.
                // Só no mundo aberto; no próprio tutorial não faz sentido.
                if !self.tutorial_mode {
                    let _ = handle.to_client.send(ServerMessage::GoToTutorial {
                        host: String::new(), path: String::new(),
                    });
                }
            }
            _ => {} // demais NPCs sem interação
        }
    }

    /// Abre o baú de tesouro: concede recompensa/relíquia + conclui a quest TREASURE.
    fn open_treasure(&mut self, sid: SessionId, quest_id: u16) {
        let now = (now_ms() / 1000) as i64;
        let Some(def) = shared::quests::quest_by_id(quest_id) else { return };
        let Some(s) = self.sessions.get_mut(&sid) else { return };
        if !s.logged_in { return; }
        let has_active = s.quests.iter().any(|c| c.quest_id == quest_id
            && c.status == shared::quests::quest_status::ACTIVE);
        if !has_active {
            let _ = s.handle.to_client.send(ServerMessage::Chat {
                from: "SYS".into(),
                text: "O baú está trancado. Aceite a Caça ao tesouro com um morador.".into(),
            });
            return;
        }
        if def.reward_gold > 0 { s.gold = s.gold.saturating_add(def.reward_gold as u64); }
        if def.reward_xp > 0 { s.grant_xp(def.reward_xp); }
        if def.reward_faction_points > 0 { s.faction_points = s.faction_points.saturating_add(def.reward_faction_points); }
        if def.reward_item != 0 && def.reward_item_qty > 0 {
            add_to_inventory(&mut s.inventory, def.reward_item, def.reward_item_qty as u32, None);
            s.inventory_dirty = true;
        }
        let cd = if def.repeatable {
            if def.daily { ((now / 86400) + 1) * 86400 } // próxima meia-noite UTC
            else { now + def.cooldown_secs as i64 }
        } else { i64::MAX };
        let new_status = shared::quests::quest_status::TURNED_IN;
        if let Some(c) = s.quests.iter_mut().find(|c| c.quest_id == quest_id) {
            c.status = new_status; c.progress = def.obj_count; c.cooldown_until = cd;
        }
        s.quests_dirty = true;
        let pts = s.faction_points;
        let fac_reward = def.reward_faction_points > 0;
        let h = s.handle.clone();
        let _ = h.to_client.send(ServerMessage::QuestUpdate { quest_id, progress: def.obj_count, status: new_status });
        let _ = h.to_client.send(ServerMessage::Chat { from: "SYS".into(), text: "Você abriu o baú e encontrou a relíquia!".into() });
        if fac_reward { let _ = h.to_client.send(ServerMessage::FactionPoints { points: pts }); }
    }

    /// Envia a loja de facção (itens em pontos) ao player.
    fn send_faction_shop(&self, sid: SessionId) {
        let Some(s) = self.sessions.get(&sid) else { return };
        let items: Vec<shared::protocol::FactionShopItemNet> = shared::quests::FACTION_SHOP.iter()
            .map(|(item_id, points)| shared::protocol::FactionShopItemNet { item_id: *item_id, points: *points })
            .collect();
        let _ = s.handle.to_client.send(ServerMessage::FactionShopOpen {
            faction: Self::faction_qid(s.faction),
            points: s.faction_points,
            faction_items: items,
        });
    }

    fn handle_faction_shop_buy(&mut self, sid: SessionId, item_id: u16) {
        // Valida proximidade de um NPC de facção da MESMA facção.
        let Some(s) = self.sessions.get(&sid) else { return };
        if !s.logged_in { return; }
        let player_fac = s.faction;
        let Some(player_entity) = s.entity else { return };
        let Ok(ppos) = self.ecs.get::<&Position>(player_entity).map(|p| p.0) else { return };
        let r_sq = shared::INTERACT_RADIUS * shared::INTERACT_RADIUS;
        let mut near = false;
        for (_, (p, k, fg)) in self.ecs.query::<(&Position, &EntityKind, &FactionGiverTag)>().iter() {
            if matches!(k, EntityKind::Npc(6)) && fg.faction == player_fac
                && p.0.distance_squared(ppos) <= r_sq { near = true; break; }
        }
        if !near { return; }
        let Some(price) = shared::quests::faction_shop_price(item_id) else { return };
        let Some(s) = self.sessions.get_mut(&sid) else { return };
        if s.faction_points < price { return; }
        s.faction_points -= price;
        add_to_inventory(&mut s.inventory, item_id, 1, None);
        s.inventory_dirty = true;
        s.quests_dirty = true; // persiste faction_points no batch
        let pts = s.faction_points;
        let h = s.handle.clone();
        let _ = h.to_client.send(ServerMessage::FactionPoints { points: pts });
        // Reenvia a loja com os pontos atualizados.
        self.send_faction_shop(sid);
    }

    fn start_carrying(&mut self, carrier_sid: SessionId, target_eid: EntityId) {
        let carrier_eid = match self.sessions.get(&carrier_sid) {
            Some(s) => s.entity_id,
            None => return,
        };
        if let Some(s) = self.sessions.get_mut(&carrier_sid) {
            s.carrying = Some(target_eid);
        }
        for s in self.sessions.values_mut() {
            if s.entity_id == target_eid {
                s.carried_by = Some(carrier_eid);
                let _ = s.handle.to_client.send(ServerMessage::Chat {
                    from: "SYS".into(),
                    text: "voce esta sendo carregado".into(),
                });
                tracing::info!("{} esta carregando {}", carrier_eid.0, target_eid.0);
                break;
            }
        }
    }

    fn release_carried(&mut self, carrier_sid: SessionId, target_eid: EntityId) {
        if let Some(s) = self.sessions.get_mut(&carrier_sid) {
            s.carrying = None;
        }
        for s in self.sessions.values_mut() {
            if s.entity_id == target_eid {
                s.carried_by = None;
                break;
            }
        }
    }

    fn handle_shop_buy(&mut self, sid: SessionId, slot_idx: usize) {
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
        // Acha o vendor mais próximo + seu shop_id (VendorTag).
        let mut nearest_shop_id: Option<u32> = None;
        let mut best_d = f32::INFINITY;
        for (entity, (pos, kind)) in self.ecs.query::<(&Position, &EntityKind)>().iter() {
            if !matches!(kind, EntityKind::Npc(n) if *n != 2) { continue; }
            let d = pos.0.distance_squared(player_pos);
            if d > r_sq { continue; }
            if d < best_d {
                best_d = d;
                let sid_v = self.ecs.get::<&VendorTag>(entity).map(|t| t.shop_id).unwrap_or(1);
                nearest_shop_id = Some(sid_v);
            }
        }
        let Some(shop_id) = nearest_shop_id else { return; };

        // Listing específico do vendor próximo (não global).
        let listing = crate::economy::shop_listing_for(shop_id);
        let Some(&(item_id, price)) = listing.get(slot_idx) else { return };

        // 2) Muta sessao + calcula se precisa atualizar Health.max
        let new_hp_max: Option<i32> = {
            let Some(session) = self.sessions.get_mut(&sid) else { return };

            // 2a) Verifica gold (currency, não item)
            if session.gold < price as u64 {
                let _ = session.handle.to_client.send(ServerMessage::Chat {
                    from: "SHOP".into(),
                    text: format!("need {price} gold"),
                });
                return;
            }

            // 2b) Coloca o item (equipa se puder, senao inventario)
            let equip = shared::equip_slot_of(item_id);
            let mut new_max: Option<i32> = None;
            let placed = if let Some(es) = equip {
                let empty = session.equipment.get(es).is_none();
                let allowed = can_equip_in_slot(&session.equipment, es, item_id);
                if empty && allowed {
                    session.equipment.set(es, Some(item_id), None);
                    session.stats = effective_stats(&session.equipment, &session.allocated_points, &session.proficiencies, &session.learned_skills, session.xp);
                    session.stats_dirty = true;
                    new_max = Some(session.stats.hp_max);
                    true
                } else {
                    add_to_inventory(&mut session.inventory, item_id, 1, None)
                }
            } else {
                add_to_inventory(&mut session.inventory, item_id, 1, None)
            };
            if !placed {
                let _ = session.handle.to_client.send(ServerMessage::Chat {
                    from: "SHOP".into(),
                    text: "inventory full".into(),
                });
                return;
            }

            // 2c) Cobra gold (currency)
            session.gold = session.gold.saturating_sub(price as u64);
            session.inventory_dirty = true;
            let _ = session.handle.to_client.send(ServerMessage::Chat {
                from: "SHOP".into(),
                text: format!("bought item {item_id} for {price} gold"),
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

    fn handle_shop_sell(&mut self, sid: SessionId, inv_slot: usize) {
        // 1) Valida sessao + proximidade do vendor (igual ShopBuy).
        let player_pos = match self.sessions.get(&sid) {
            Some(s) if s.logged_in => match s.entity {
                Some(e) => match self.ecs.get::<&Position>(e) {
                    Ok(p) => p.0,
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

        let Some(session) = self.sessions.get_mut(&sid) else { return };
        if inv_slot >= session.inventory.len() { return; }
        let slot = session.inventory[inv_slot];
        if slot.qty == 0 { return; }
        let price = crate::economy::sell_price_of(slot.item_id);
        if price == 0 {
            let _ = session.handle.to_client.send(ServerMessage::Chat {
                from: "SHOP".into(),
                text: "this item cannot be sold".into(),
            });
            return;
        }

        // Vende 1 unidade por click. Pra stacks (potions, gem) o jogador
        // clica N vezes — UX simples sem precisar de input numérico.
        session.inventory[inv_slot].qty -= 1;
        if session.inventory[inv_slot].qty == 0 {
            session.inventory[inv_slot] = shared::InventorySlot::default();
        }
        session.gold = session.gold.saturating_add(price as u64);
        session.inventory_dirty = true;
        let _ = session.handle.to_client.send(ServerMessage::Chat {
            from: "SHOP".into(),
            text: format!("sold item {} for {price} gold", slot.item_id),
        });
    }

    /// Trade atômico: lista de compras + lista de vendas, executadas juntas.
    /// Validação completa servidor-side (cliente é só preview). Em qualquer
    /// erro: rollback total + envia ShopTradeResult { ok: false, reason }.
    fn handle_shop_trade(
        &mut self,
        sid: SessionId,
        buying: Vec<shared::protocol::TradeBuyEntry>,
        selling: Vec<shared::protocol::TradeSellEntry>,
    ) {
        // 1) Sessão + proximidade do vendor.
        let (player_pos, vendor_id) = match self.sessions.get(&sid) {
            Some(s) if s.logged_in => match s.entity {
                Some(e) => match self.ecs.get::<&Position>(e) {
                    Ok(p) => (p.0, 0u32), // vendor_id será descoberto abaixo
                    Err(_) => return,
                },
                None => return,
            },
            _ => return,
        };
        let r_sq = shared::INTERACT_RADIUS * shared::INTERACT_RADIUS;
        let _ = vendor_id; // unused — vendor real é resolvido por proximidade abaixo
        // Acha vendor mais próximo + shop_id correspondente.
        let mut nearest: Option<(u32, u32, f32)> = None; // (vendor_eid, shop_id, dist²)
        for (entity, (net, p, k)) in self.ecs.query::<(&NetId, &Position, &EntityKind)>().iter() {
            if !matches!(k, EntityKind::Npc(n) if *n != 2) { continue; }
            let d = p.0.distance_squared(player_pos);
            if d > r_sq { continue; }
            let sid_v = self.ecs.get::<&VendorTag>(entity).map(|t| t.shop_id).unwrap_or(1);
            if nearest.map_or(true, |(_,_,bd)| d < bd) {
                nearest = Some((net.0.0 as u32, sid_v, d));
            }
        }
        let Some((vendor_eid, shop_id, _)) = nearest else {
            self.send_trade_result(sid, false, "longe demais do vendedor");
            return;
        };
        let vendor_id = vendor_eid;

        let listing = crate::economy::shop_listing_for(shop_id);
        let (buy_mult, sell_mult) = crate::economy::vendor_modifiers(vendor_id);

        // 2) Calcula custo de compra + valida slots.
        let mut total_buy: u64 = 0;
        let mut buys: Vec<(u16, u32)> = Vec::with_capacity(buying.len());
        for entry in &buying {
            if entry.qty == 0 { continue; }
            let Some(&(item_id, base_price)) = listing.get(entry.shop_slot as usize) else {
                self.send_trade_result(sid, false, "item de loja inválido");
                return;
            };
            let unit_price = (base_price as f32 * buy_mult).round() as u64;
            total_buy = total_buy.saturating_add(unit_price * entry.qty as u64);
            buys.push((item_id, entry.qty));
        }

        // 3) Calcula valor de venda + valida slots do inventário.
        // Snapshot do inventário pra validação (sem mutar ainda).
        let inv_snapshot: Vec<shared::InventorySlot> = match self.sessions.get(&sid) {
            Some(s) => s.inventory.clone(),
            None => return,
        };
        let mut total_sell: u64 = 0;
        // Agrega qty por inv_slot pra detectar duplicatas no payload.
        let mut sell_by_slot: std::collections::HashMap<u16, u32> = std::collections::HashMap::new();
        for entry in &selling {
            if entry.qty == 0 { continue; }
            *sell_by_slot.entry(entry.inv_slot).or_insert(0) += entry.qty;
        }
        let mut sells: Vec<(u16, u16, u32)> = Vec::new(); // (inv_slot, item_id, qty)
        for (&slot_idx, &qty) in &sell_by_slot {
            let idx = slot_idx as usize;
            if idx >= inv_snapshot.len() {
                self.send_trade_result(sid, false, "slot de inventário inválido");
                return;
            }
            let slot = &inv_snapshot[idx];
            if slot.qty < qty {
                self.send_trade_result(sid, false, "quantidade indisponível no inventário");
                return;
            }
            let unit_price = crate::economy::sell_price_of(slot.item_id);
            if unit_price == 0 {
                self.send_trade_result(sid, false, "item não vendável no basket");
                return;
            }
            let unit_price_mod = (unit_price as f32 * sell_mult).round() as u64;
            total_sell = total_sell.saturating_add(unit_price_mod * qty as u64);
            sells.push((slot_idx, slot.item_id, qty));
        }

        // 4) Verifica saldo: session.gold + total_sell >= total_buy.
        let gold_before: u64 = self.sessions.get(&sid).map(|s| s.gold).unwrap_or(0);
        let Some(gold_after) = gold_before.saturating_add(total_sell).checked_sub(total_buy) else {
            self.send_trade_result(sid, false, "ouro insuficiente");
            return;
        };

        // 5) Simula o trade num clone do inventário pra validar inventory space.
        let mut sim = inv_snapshot.clone();
        // 5a) Remove vendidos
        for &(slot_idx, _item_id, qty) in &sells {
            let s = &mut sim[slot_idx as usize];
            s.qty -= qty;
            if s.qty == 0 { *s = shared::InventorySlot::default(); }
        }
        // 5b) Adiciona itens comprados. Loja sempre vende stackáveis sem
        // instance — pode ser estendido pra vender raros no futuro.
        for &(item_id, qty) in &buys {
            if !add_to_inventory(&mut sim, item_id, qty, None) {
                self.send_trade_result(sid, false, "inventário cheio pros itens comprados");
                return;
            }
        }

        // 6) Tudo validou — commita: inventário e gold.
        let Some(session) = self.sessions.get_mut(&sid) else { return };
        session.inventory = sim;
        session.gold = gold_after;
        session.inventory_dirty = true;

        let summary = format!(
            "trade ok: spent {total_buy}, received {total_sell} (balance {:+})",
            total_sell as i64 - total_buy as i64
        );
        let _ = session.handle.to_client.send(ServerMessage::ShopTradeResult {
            ok: true, reason: summary.clone(),
        });
        let _ = session.handle.to_client.send(ServerMessage::Chat {
            from: "SHOP".into(),
            text:  summary,
        });
    }

    fn send_trade_result(&self, sid: SessionId, ok: bool, reason: &str) {
        if let Some(s) = self.sessions.get(&sid) {
            let _ = s.handle.to_client.send(ServerMessage::ShopTradeResult {
                ok, reason: reason.into(),
            });
        }
    }

    fn handle_use_item(&mut self, sid: SessionId, slot_idx: usize) {
        // Extrai estado + identifica a acao fora do borrow mutavel do ECS.
        enum UseAction {
            Equip { new_hp_max: i32 },
            HealHp(i32),
            HealMp(i32),
            HealStam(i32),
            /// Player tentou usar um item de barco. Server tenta spawnar
            /// um barco em agua adjacente; consome o item se sucesso.
            SpawnBoat { kind: u16, item_id: u16 },
        }
        let (player_entity, action) = {
            let Some(session) = self.sessions.get_mut(&sid) else { return };
            if !session.logged_in { return; }
            if slot_idx >= session.inventory.len() { return; }
            let slot = session.inventory[slot_idx];
            if slot.qty == 0 { return; }
            // Item desativado pelo admin: nao equipa, nao usa. Silent skip
            // (cliente pode manter no inv pra vender/guardar).
            if !crate::economy::is_item_active(slot.item_id) { return; }
            let Some(player_entity) = session.entity else { return };

            // Equipavel: swap entre inventario e slot de equip correspondente.
            if let Some(es) = shared::equip_slot_of(slot.item_id) {
                // Gate: offhand exige weapon compativel; weapon two-handed
                // auto-unequipa shield (se houver) pro inventario.
                if !can_equip_in_slot(&session.equipment, es, slot.item_id) {
                    return;
                }
                if es == shared::EquipSlot::Weapon
                    && !maybe_unequip_offhand_for_weapon(session, slot.item_id)
                {
                    return; // inv cheio, abort equip
                }
                let old      = session.equipment.get(es);
                let old_inst = session.equipment.get_inst(es);
                let new_id   = slot.item_id;
                let new_inst = slot.instance;
                session.equipment.set(es, Some(new_id), new_inst);
                session.inventory[slot_idx] = match old {
                    Some(old_id) => shared::InventorySlot { item_id: old_id, qty: 1, instance: old_inst },
                    None         => shared::InventorySlot::default(),
                };
                session.stats = effective_stats(&session.equipment, &session.allocated_points, &session.proficiencies, &session.learned_skills, session.xp);
                session.stats_dirty = true;
                session.inventory_dirty = true;
                (player_entity, UseAction::Equip { new_hp_max: session.stats.hp_max })
            } else if let Some(boat_kind) = shared::boat_kind_of(slot.item_id) {
                (player_entity, UseAction::SpawnBoat { kind: boat_kind, item_id: slot.item_id })
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
                let healed = if let Some(session) = self.sessions.get_mut(&sid) {
                    let stam_max = session.stats.stamina_max;
                    if (session.stamina_current as i32) < stam_max {
                        session.stamina_current = ((session.stamina_current as i32 + amount).min(stam_max)) as f32;
                        let _ = session.handle.to_client.send(
                            ServerMessage::StaminaUpdate { current: session.stamina_current as i32 });
                        true
                    } else { false }
                } else { false };
                if healed { consume_slot(&mut self.sessions); }
            }
            UseAction::SpawnBoat { kind, item_id: boat_item } => {
                let player_pos = match self.ecs.get::<&Position>(player_entity) {
                    Ok(p) => p.0,
                    Err(_) => return,
                };
                // Player ja montado? Nao deixa "dobrar".
                if self.ecs.get::<&Mounted>(player_entity).is_ok() {
                    self.send_chat_to(sid, "[Sistema] Ja em uma embarcacao.");
                    return;
                }
                // Procura tile de agua onde o CASCO INTEIRO cabe. Raio max
                // dimensionado pelo tamanho do barco (deck pode ter 13+
                // tiles de altura; precisa procurar longe pra achar agua
                // aberta). Direcao inicial calculada por tile candidato
                // pra testar hull_navigable com yaw correto.
                let cfg = crate::boat_config::get(kind);
                let max_radius = cfg.deck_half_w.max(cfg.deck_half_h).ceil() as i32 + 4;
                let px = player_pos.x.floor() as i32;
                let py = player_pos.y.floor() as i32;
                let mut spawn: Option<(Vec2, f32)> = None;
                'search: for r in 1i32..=max_radius {
                    for dy in -r..=r {
                        for dx in -r..=r {
                            if dx.abs() != r && dy.abs() != r { continue; }
                            let tx = px + dx;
                            let ty = py + dy;
                            if !self.map.is_water(tx, ty) { continue; }
                            let candidate = Vec2::new(tx as f32 + 0.5, ty as f32 + 0.5);
                            // Yaw inicial: barco aponta away from player
                            // (proa pra agua aberta). Y-forward convention.
                            let d = (candidate - player_pos).normalize_or_zero();
                            let yaw = if d.length_squared() > 0.001 {
                                (-d.x).atan2(d.y)
                            } else { 0.0 };
                            if cfg.is_hull_navigable(candidate, yaw, &self.map) {
                                spawn = Some((candidate, yaw));
                                break 'search;
                            }
                        }
                    }
                }
                let Some((water_pos, init_yaw)) = spawn else {
                    tracing::info!(
                        "boat use: sem agua livre pra casco. player=({:.2},{:.2}) max_r={}",
                        player_pos.x, player_pos.y, max_radius
                    );
                    self.send_chat_to(sid, "[Sistema] Sem agua aberta suficiente pra ancorar.");
                    return;
                };
                // Spawna o barco. Sem rigid body — movimento custom em step E.
                let boat_eid = self.alloc_entity_id();
                let player_eid = match self.ecs.get::<&NetId>(player_entity) {
                    Ok(n) => n.0,
                    Err(_) => return,
                };
                let owner_pid = match self.ecs.get::<&PlayerTag>(player_entity) {
                    Ok(p) => p.player_id,
                    Err(_) => return,
                };
                let dir_vec = (water_pos - player_pos).normalize_or_zero();
                let init_dir = dir8_from_vec(dir_vec);
                let boat_entity = self.ecs.spawn((
                    NetId(boat_eid),
                    Position(water_pos),
                    Velocity(Vec2::ZERO),
                    EntityKind::Boat(kind),
                    BoatTag {
                        kind,
                        owner_pid,
                        yaw: init_yaw,
                        ang_vel: 0.0,
                        rudder_angle: 0.0,
                        sail_position: 0,        // raised — barco para
                        sail_angle: 0.0,
                        anchor_dropped: true,    // recem-spawnado: ancorado
                        anchor_progress: 1.0,
                        helm_eid: None,
                        sail_eid: None,
                        anchor_eid: None,
                        cannon_eids: vec![None; crate::boat_config::get(kind).cannons.len()],
                        cannon_aim:  vec![0.0; crate::boat_config::get(kind).cannons.len()],
                        cannon_cd_until: vec![0.0; crate::boat_config::get(kind).cannons.len()],
                        // Sea-of-Thieves: barco vazio. Player precisa andar
                        // ate ele e clicar EMBARCAR pra subir.
                        passengers: Vec::new(),
                        dir: init_dir,
                        anim: 0,
                    },
                ));
                let _ = boat_entity; // suprime unused: ficamos com o eid
                let _ = player_eid;
                self.save_pending = true;
                tracing::info!("boat spawn (vazio): pid={:?} kind={} pos={:?}", owner_pid, kind, water_pos);
                // CONSOME o item do barco — 1 barco por item (sem spawnar
                // infinitos). Volta ao inventário ao desembarcar (handle_dismount).
                if let Some(s) = self.sessions.get_mut(&sid) {
                    if slot_idx < s.inventory.len()
                        && s.inventory[slot_idx].item_id == boat_item
                        && s.inventory[slot_idx].qty > 0
                    {
                        s.inventory[slot_idx].qty -= 1;
                        if s.inventory[slot_idx].qty == 0 {
                            s.inventory[slot_idx] = shared::InventorySlot::default();
                        }
                        s.inventory_dirty = true;
                    }
                }
                // Tutorial: a quest 905 ("Convoque o Barco") completa ao USAR o
                // item (deploy). Depois o player embarca e aprende vela/leme.
                if self.tutorial_mode {
                    // 905 é EXPLORE(cais): pisar no cais já a vira READY antes de
                    // usar o barco. Aceita ACTIVE **ou** READY — o gatilho real é
                    // USAR o item (deploy), não só chegar no cais.
                    let on_905 = self.sessions.get(&sid).map(|s| s.quests.iter().any(|c|
                        c.quest_id == 905
                        && (c.status == shared::quests::quest_status::ACTIVE
                            || c.status == shared::quests::quest_status::READY))).unwrap_or(false);
                    if on_905 { self.tutorial_advance(sid, 905); }
                }
            }
        }
    }

    /// Helper: envia mensagem de sistema (Chat from="System") pro cliente.
    // ── Farm Nodes ────────────────────────────────────────────────────────────

    /// Valida e aplica um hit de coleta. Gateia: login, distância, cooldown,
    /// node vivo. Em caso de sucesso, reduz HP, replica anim e, se HP=0,
    /// spawna loot + agenda respawn + notifica clientes próximos.
    fn handle_farm_hit(&mut self, sid: SessionId, node_id: u32) {
        // Gate: player logado com entidade no mapa.
        let player_pos = {
            let Some(s) = self.sessions.get(&sid) else { return };
            if !s.logged_in { return }
            let Some(e) = s.entity else { return };
            let Ok(p) = self.ecs.get::<&Position>(e) else { return };
            p.0
        };

        // Gate: node existe e está vivo.
        let node = match self.farm_nodes.get(&node_id) {
            Some(n) if n.respawn_at <= 0.0 => n,
            Some(_) => { tracing::info!("[farm diag] node {} em respawn — ignora", node_id); return }
            None    => { tracing::info!("[farm diag] node {} NAO EXISTE (total={})", node_id, self.farm_nodes.len()); return }
        };

        // Gate: distância máxima.
        let ndist = player_pos.distance(node.pos);
        if ndist > shared::FARM_MAX_RANGE {
            tracing::info!("[farm diag] node {} fora de alcance (dist={:.1} max={})", node_id, ndist, shared::FARM_MAX_RANGE);
            return
        }
        tracing::info!("[farm diag] FarmHit OK node={} dist={:.1}", node_id, ndist);

        // Gate: nível de proficiência mínimo pro tier do recurso (T2=10/T3=20/T4=30).
        // O cliente também pré-checa e nem inicia a coleta; isto é a rede de segurança.
        let req_lvl = shared::tier_level_req(node.tier);
        if req_lvl > 1 {
            let prof = shared::Proficiency::from_farm_kind(&node.kind);
            let cur_lvl = self.sessions.get(&sid)
                .map(|s| shared::proficiency_level(s.proficiencies[prof as usize]))
                .unwrap_or(1);
            if cur_lvl < req_lvl {
                tracing::info!("[farm diag] node {} tier{} bloqueado: requer {} lv{} (player lv{})",
                    node_id, node.tier, prof.as_db_str(), req_lvl, cur_lvl);
                return;
            }
        }

        // Gate: precisa de TOOL do tipo certo equipada no slot dedicado
        // (axe→Tree, sickle→Flower, pickaxe→Rock) com tier >= node.tier.
        let req_tool_kind = shared::item_id::tool_kind_for_farm(&node.kind);
        if let Some(needed) = req_tool_kind {
            let (eq_id, eq_kind, eq_tier) = self.sessions.get(&sid).map(|s| {
                let id = match needed {
                    shared::ToolKind::Axe        => s.equipment.tool_axe.unwrap_or(0),
                    shared::ToolKind::Sickle     => s.equipment.tool_sickle.unwrap_or(0),
                    shared::ToolKind::Pickaxe    => s.equipment.tool_pickaxe.unwrap_or(0),
                    shared::ToolKind::FishingRod => s.equipment.tool_rod.unwrap_or(0),
                };
                (id, shared::item_id::tool_kind(id), shared::item_id::tool_tier(id))
            }).unwrap_or((0, None, 0));
            if eq_kind != Some(needed) {
                tracing::info!("[farm diag] node {} bloqueado: precisa de {:?}, tem item={} ({:?})",
                    node_id, needed, eq_id, eq_kind);
                return;
            }
            if eq_tier < node.tier {
                tracing::info!("[farm diag] node {} T{} bloqueado: tool tier {} < node tier",
                    node_id, node.tier, eq_tier);
                return;
            }
        }

        // Gate: cooldown por (player, node).
        let cd_key = (sid, node_id);
        if self.farm_hit_cooldowns.get(&cd_key)
            .map(|&t| t > self.sim_time_s)
            .unwrap_or(false)
        { return }
        self.farm_hit_cooldowns.insert(cd_key, self.sim_time_s + shared::FARM_HIT_COOLDOWN_S);

        // Todos os tipos usam ToolSwing (cliente difere visualmente pelo tool code).
        let anim_code = shared::attack_anim::TOOL_SWING;

        // Modelo countdown: cada FarmHit = depleção completa do node.
        // Cliente envia 1 hit ao final da duração calculada por skill+tool.
        let node = self.farm_nodes.get_mut(&node_id).unwrap();
        node.hp = 0;
        let (hp, hp_max) = (node.hp, node.hp_max);
        let node_pos = node.pos;
        let node_kind = node.kind.clone();
        let node_tier = node.tier;

        // Replica anim de ferramenta no snapshot do player.
        if let Some(s) = self.sessions.get(&sid) {
            if let Some(e) = s.entity {
                if let Ok(mut tag) = self.ecs.get::<&mut PlayerTag>(e) {
                    tag.attack_anim_pending = Some(anim_code);
                }
            }
        }

        // Broadcast FarmNodeUpdate pra todos os players na AOI.
        let aoi_sq = shared::AOI_RADIUS * shared::AOI_RADIUS;
        let update_msg = ServerMessage::FarmNodeUpdate { node_id, hp, hp_max };
        for s in self.sessions.values() {
            if !s.logged_in { continue }
            let Some(e) = s.entity else { continue };
            let Ok(p) = self.ecs.get::<&Position>(e) else { continue };
            if p.0.distance_squared(node_pos) <= aoi_sq {
                let _ = s.handle.to_client.send(update_msg.clone());
            }
        }

        // Node esgotado: spawna loot e agenda respawn (usa tempo do mapfile ou default).
        if hp <= 0 {
            let node = self.farm_nodes.get_mut(&node_id).unwrap();
            node.respawn_at = self.sim_time_s + node.respawn_seconds;

            let seed = (self.tick as u64)
                .wrapping_mul(0xDEAD_BEEF)
                .wrapping_add(node_id as u64);
            let drops = crate::economy::farm_node_loot(&node_kind, node_tier, seed);
            tracing::info!("[farm diag] node {} ({} t{}) depletou — drops={}", node_id, node_kind, node_tier, drops.len());
            // Farm drops caem nos arredores do node (raio 1.2-2.0 tiles)
            // para não sobrepor o sprite de toco/entulho que fica no centro.
            let drop_origin = {
                let a = lcg_f32(seed ^ 0xF4_21) * std::f32::consts::TAU;
                let r = 1.2 + lcg_f32(seed ^ 0xB3_11) * 0.8;
                node_pos + Vec2::new(a.cos() * r, a.sin() * r * 0.5)
            };
            // Farm: drop_origin já está 1.2-2.0 do node; spread interno menor
            // (1.0) pra não voar longe demais do toco. kind=0 (não é mob).
            self.spawn_loot_drops(drop_origin, &drops, seed, 1.0, 0);

            // Broadcast FarmNodeDepleted.
            let depleted_msg = ServerMessage::FarmNodeDepleted { node_id };
            for s in self.sessions.values() {
                if !s.logged_in { continue }
                let Some(e) = s.entity else { continue };
                let Ok(p) = self.ecs.get::<&Position>(e) else { continue };
                if p.0.distance_squared(node_pos) <= aoi_sq {
                    let _ = s.handle.to_client.send(depleted_msg.clone());
                }
            }

            // XP de proficiência de farm por TIPO (Mineração/Lenhador/Coleta).
            // XP escala com o tier do recurso. Os níveis *_lvl viram cache
            // derivado do XP de prof (usados pelo cliente em FarmSkillsUpdate).
            if let Some(s) = self.sessions.get_mut(&sid) {
                let prof = shared::Proficiency::from_farm_kind(&node_kind);
                let gain = 30u64 * node_tier.max(1) as u64;
                let pi = prof as usize;
                s.proficiencies[pi] = s.proficiencies[pi].saturating_add(gain);
                s.proficiencies_dirty = true;

                s.woodcutting_lvl = shared::proficiency_level(
                    s.proficiencies[shared::Proficiency::Woodcutting as usize]);
                s.mining_lvl = shared::proficiency_level(
                    s.proficiencies[shared::Proficiency::Mining as usize]);
                s.gathering_lvl = shared::proficiency_level(
                    s.proficiencies[shared::Proficiency::Gathering as usize]);

                let _ = s.handle.to_client.send(ServerMessage::FarmSkillsUpdate {
                    woodcutting: s.woodcutting_lvl,
                    mining:      s.mining_lvl,
                    gathering:   s.gathering_lvl,
                });
                let _ = s.handle.to_client.send(ServerMessage::ProficienciesUpdate {
                    xp: s.proficiencies,
                });
            }
            self.save_pending = true;
        }
    }

    /// Verifica todos os farm nodes com respawn_at expirado e os restaura.
    /// Chamado a cada tick em `step()`.
    fn tick_farm_respawn(&mut self) {
        let aoi_sq = shared::AOI_RADIUS * shared::AOI_RADIUS;
        let now = self.sim_time_s;
        let mut respawned: Vec<(u32, Vec2)> = Vec::new();
        for (&id, node) in self.farm_nodes.iter_mut() {
            if node.respawn_at > 0.0 && now >= node.respawn_at {
                node.hp         = node.hp_max;
                node.respawn_at = 0.0;
                respawned.push((id, node.pos));
            }
        }
        for (node_id, node_pos) in respawned {
            let msg = ServerMessage::FarmNodeRespawned { node_id };
            for s in self.sessions.values() {
                if !s.logged_in { continue }
                let Some(e) = s.entity else { continue };
                let Ok(p) = self.ecs.get::<&Position>(e) else { continue };
                if p.0.distance_squared(node_pos) <= aoi_sq {
                    let _ = s.handle.to_client.send(msg.clone());
                }
            }
        }
        // Limpa cooldowns expirados pra não crescer indefinidamente.
        self.farm_hit_cooldowns.retain(|_, &mut t| t > now);
    }

    fn send_chat_to(&self, sid: SessionId, text: &str) {
        if let Some(s) = self.sessions.get(&sid) {
            let _ = s.handle.to_client.send(ServerMessage::Chat {
                from: "System".to_string(),
                text: text.to_string(),
            });
        }
    }

    /// Desmonta o player do barco e teleporta pra terra walkable mais
    /// proxima (BFS ate 3 tiles a partir do barco). Falha se sem terra
    /// alcancavel — player precisa mover o barco antes.
    ///
    /// Multi-passenger aware: remove player de `passengers` + clear
    /// estacao se ocupando alguma. Despawna o barco APENAS se o owner
    /// foi o ultimo a sair (legacy: spawn-via-item flow).
    fn handle_dismount_boat(&mut self, sid: SessionId) {
        let (player_entity, player_eid, mounted_boat_entity, mounted_boat_eid) = {
            let Some(session) = self.sessions.get(&sid) else { return };
            if !session.logged_in { return; }
            let Some(pe) = session.entity else { return };
            let Ok(m) = self.ecs.get::<&Mounted>(pe) else { return };
            let player_eid = self.ecs.get::<&NetId>(pe).map(|n| n.0).unwrap_or(EntityId(0));
            (pe, player_eid, m.boat_entity, m.boat_eid)
        };
        // Pos do barco.
        let boat_pos = match self.ecs.get::<&Position>(mounted_boat_entity) {
            Ok(p) => p.0,
            Err(_) => return,
        };
        let bx = boat_pos.x.floor() as i32;
        let by = boat_pos.y.floor() as i32;
        // BFS procurando walkable. Range = hull max_extent + margem — o
        // casco grande (deck_half_h ~7) afasta o centro do barco bastante
        // da margem quando colidindo.
        let boat_kind = self.ecs.get::<&BoatTag>(mounted_boat_entity).ok().map(|t| t.kind).unwrap_or(0);
        let cfg = Some(crate::boat_config::get(boat_kind));
        let max_radius = cfg.as_ref()
            .map(|c| c.deck_half_w.max(c.deck_half_h).ceil() as u32 + 25)
            .unwrap_or(32);
        let mut visited: std::collections::HashSet<(i32, i32)> = std::collections::HashSet::new();
        let mut queue: std::collections::VecDeque<(i32, i32, u32)> = std::collections::VecDeque::new();
        let cardinals = [(0i32, 1i32), (1, 0), (0, -1), (-1, 0)];
        for (dx, dy) in cardinals { queue.push_back((bx + dx, by + dy, 1)); }
        let mut land_target: Option<Vec2> = None;
        while let Some((tx, ty, depth)) = queue.pop_front() {
            if !visited.insert((tx, ty)) { continue; }
            if self.map.is_walkable(tx, ty) {
                land_target = Some(Vec2::new(tx as f32 + 0.5, ty as f32 + 0.5));
                break;
            }
            if depth >= max_radius { continue; }
            for (dx, dy) in cardinals {
                queue.push_back((tx + dx, ty + dy, depth + 1));
            }
        }
        let Some(target) = land_target else {
            self.send_chat_to(sid, "[Sistema] Sem terra acessivel. Mova o barco pra outra margem.");
            return;
        };
        // Teleporta o player.
        if let Ok(mut p) = self.ecs.get::<&mut Position>(player_entity) {
            p.0 = target;
        }
        if let Ok(mut v) = self.ecs.get::<&mut Velocity>(player_entity) {
            v.0 = Vec2::ZERO;
        }
        // Remove player do boat.passengers + clear estacao se ocupando.
        let (was_owner, became_empty) = if let Ok(mut tag) = self.ecs.get::<&mut BoatTag>(mounted_boat_entity) {
            tag.passengers.retain(|eid| *eid != player_eid);
            if tag.helm_eid == Some(player_eid) {
                tag.helm_eid = None;
                // rudder_angle NAO eh zerado — persiste.
            }
            if tag.sail_eid == Some(player_eid) { tag.sail_eid = None; }
            if tag.anchor_eid == Some(player_eid) { tag.anchor_eid = None; }
            // Owner check: dono e' o que SPAWNOU o barco (PlayerTag.player_id).
            let player_pid = self.ecs.get::<&PlayerTag>(player_entity).map(|p| p.player_id).ok();
            let is_owner = player_pid == Some(tag.owner_pid);
            (is_owner, tag.passengers.is_empty())
        } else { (false, false) };
        let _ = self.ecs.remove_one::<Mounted>(player_entity);
        // Se owner foi o ultimo a sair → despawna barco (legacy flow).
        // Senao barco continua flutuando pros outros usarem.
        if was_owner && became_empty {
            let _ = self.ecs.despawn(mounted_boat_entity);
            self.removed_this_tick.push(mounted_boat_eid);
            // DEVOLVE o item do barco ao inventário (deploy consome; desembarque
            // devolve) → sempre 1 barco por item, sem infinitos.
            let boat_item = match boat_kind {
                1 => shared::constants::item_id::BOAT_ESQUIFE,
                _ => shared::constants::item_id::BOAT_LYLIAN_LEUTARD,
            };
            if let Some(s) = self.sessions.get_mut(&sid) {
                add_to_inventory(&mut s.inventory, boat_item, 1, None);
                s.inventory_dirty = true;
            }
            tracing::info!("boat dismount + despawn (owner last): item {} devolvido, target={:?}", boat_item, target);
        } else {
            tracing::info!("boat dismount (boat fica): target={:?}", target);
        }
        self.save_pending = true;
    }

    // ── Boat 2.5D — handlers ────────────────────────────────────────────────

    /// Player tenta subir num barco proximo. Valida proximidade (≤ 1.5 tiles
    /// do centro do barco) e adiciona o player como passenger no centro do deck.
    fn handle_board_boat(&mut self, sid: SessionId, boat_eid: EntityId) {
        let (player_entity, player_eid, player_pos) = {
            let Some(s) = self.sessions.get(&sid) else { return };
            if !s.logged_in { return; }
            let Some(pe) = s.entity else { return };
            // Ja montado? skip.
            if self.ecs.get::<&Mounted>(pe).is_ok() { return; }
            let pid = self.ecs.get::<&NetId>(pe).map(|n| n.0).unwrap_or(EntityId(0));
            let pos = self.ecs.get::<&Position>(pe).map(|p| p.0).unwrap_or(Vec2::ZERO);
            (pe, pid, pos)
        };
        // Acha o boat entity por eid.
        let boat_entity = self.ecs.query::<(&NetId, &BoatTag)>().iter()
            .find(|(_, (n, _))| n.0 == boat_eid).map(|(e, _)| e);
        let Some(boat_entity) = boat_entity else { return };
        let boat_pos = match self.ecs.get::<&Position>(boat_entity) {
            Ok(p) => p.0, Err(_) => return,
        };
        let kind = self.ecs.get::<&BoatTag>(boat_entity).map(|t| t.kind).unwrap_or(0);
        let half = boat_deck_half(kind);
        // Margem generosa: barco grande spawnando longe da margem precisa
        // de reach > half + spawn-search-radius pra dar pra embarcar.
        let max_reach = half.x.max(half.y) + 14.0;
        if (player_pos - boat_pos).length() > max_reach { return; }
        // Adiciona ao boat.passengers.
        if let Ok(mut tag) = self.ecs.get::<&mut BoatTag>(boat_entity) {
            if !tag.passengers.contains(&player_eid) {
                tag.passengers.push(player_eid);
            }
        }
        let _ = self.ecs.insert_one(player_entity, Mounted {
            boat_entity, boat_eid,
            local_pos: Vec2::ZERO,
            station: None,
            last_move_dir: Vec2::ZERO,
        });
        self.save_pending = true;
        tracing::info!("boat board: player_eid={:?} boat_eid={:?}", player_eid, boat_eid);
    }

    /// Player na estacao tenta pega-la. Valida proximidade do hotspot da
    /// estacao em local-space (BOAT_STATION_REACH).
    /// Canhoes: station >= CANNON_BASE; slot = station - CANNON_BASE.
    fn handle_grab_station(&mut self, sid: SessionId, station: u8) {
        use shared::constants::station as st;
        let is_cannon = station >= st::CANNON_BASE
            && station < st::CANNON_BASE + st::MAX_CANNONS;
        if station != st::HELM && station != st::SAIL && station != st::ANCHOR && !is_cannon {
            return;
        }
        let (player_entity, player_eid, mounted) = {
            let Some(s) = self.sessions.get(&sid) else { return };
            if !s.logged_in { return; }
            let Some(pe) = s.entity else { return };
            let Ok(m) = self.ecs.get::<&Mounted>(pe) else { return };
            let pid = self.ecs.get::<&NetId>(pe).map(|n| n.0).unwrap_or(EntityId(0));
            (pe, pid, (m.boat_entity, m.local_pos))
        };
        let (boat_entity, local_pos) = mounted;
        let kind = self.ecs.get::<&BoatTag>(boat_entity).map(|t| t.kind).unwrap_or(0);
        let cfg = crate::boat_config::get(kind);
        let target_local = if is_cannon {
            let slot = (station - st::CANNON_BASE) as usize;
            let Some(base) = cfg.cannon_base(slot) else {
                self.send_chat_to(sid, "[Sistema] Canhao inexistente.");
                return;
            };
            let side_angle = cfg.cannon_side_angle(slot);
            // Player fica ATRAS do canhao (oposto da direcao do tiro), pra
            // nao sobrepor o sprite. forward com theta_local = -side_angle.
            let theta_local = -side_angle;
            let fwd = Vec2::new(-theta_local.sin(), theta_local.cos());
            const STAND_OFFSET: f32 = 1.4;
            base - fwd * STAND_OFFSET
        } else {
            match station {
                s if s == st::HELM   => boat_helm_local(kind),
                s if s == st::SAIL   => boat_sail_local(kind),
                s if s == st::ANCHOR => boat_anchor_local(kind),
                _ => return,
            }
        };
        if (local_pos - target_local).length() > BOAT_STATION_REACH {
            self.send_chat_to(sid, "[Sistema] Aproxime-se da estacao.");
            return;
        }
        // Set station eid no boat (se vazia).
        if let Ok(mut tag) = self.ecs.get::<&mut BoatTag>(boat_entity) {
            if is_cannon {
                let slot = (station - st::CANNON_BASE) as usize;
                if let Some(slot_ref) = tag.cannon_eids.get_mut(slot) {
                    if slot_ref.is_some() && *slot_ref != Some(player_eid) {
                        self.send_chat_to(sid, "[Sistema] Canhao ocupado.");
                        return;
                    }
                    *slot_ref = Some(player_eid);
                }
            } else {
                let slot = match station {
                    s if s == st::HELM   => &mut tag.helm_eid,
                    s if s == st::SAIL   => &mut tag.sail_eid,
                    _                    => &mut tag.anchor_eid,
                };
                if slot.is_some() && *slot != Some(player_eid) {
                    self.send_chat_to(sid, "[Sistema] Estacao ocupada.");
                    return;
                }
                *slot = Some(player_eid);
            }
        }
        // Set Mounted.station + snap local_pos.
        if let Ok(mut m) = self.ecs.get::<&mut Mounted>(player_entity) {
            m.station = Some(station);
            m.local_pos = target_local;
        }
    }

    /// Player solta a estacao atual. Idempotente.
    fn handle_release_station(&mut self, sid: SessionId) {
        let (player_entity, player_eid, boat_entity, station) = {
            let Some(s) = self.sessions.get(&sid) else { return };
            if !s.logged_in { return; }
            let Some(pe) = s.entity else { return };
            let Ok(m) = self.ecs.get::<&Mounted>(pe) else { return };
            let Some(st) = m.station else { return };
            let pid = self.ecs.get::<&NetId>(pe).map(|n| n.0).unwrap_or(EntityId(0));
            (pe, pid, m.boat_entity, st)
        };
        use shared::constants::station as st;
        if let Ok(mut tag) = self.ecs.get::<&mut BoatTag>(boat_entity) {
            if station >= st::CANNON_BASE && station < st::CANNON_BASE + st::MAX_CANNONS {
                let slot = (station - st::CANNON_BASE) as usize;
                if let Some(slot_ref) = tag.cannon_eids.get_mut(slot) {
                    if *slot_ref == Some(player_eid) { *slot_ref = None; }
                }
            } else {
                match station {
                    s if s == st::HELM => {
                        if tag.helm_eid == Some(player_eid) {
                            tag.helm_eid = None;
                            // rudder_angle NAO eh zerado — persiste.
                        }
                    }
                    s if s == st::SAIL => {
                        if tag.sail_eid == Some(player_eid) { tag.sail_eid = None; }
                    }
                    s if s == st::ANCHOR => {
                        if tag.anchor_eid == Some(player_eid) { tag.anchor_eid = None; }
                    }
                    _ => {}
                }
            }
        }
        if let Ok(mut m) = self.ecs.get::<&mut Mounted>(player_entity) {
            m.station = None;
        }
    }

    /// Ajusta posicao da vela. Player precisa estar na estacao SAIL.
    /// `delta_angle` ignorado — vela fica sempre perpendicular ao casco
    /// (sail_angle=0 fixo). Simplifica gameplay vs SoT.
    fn handle_sail_adjust(&mut self, sid: SessionId, delta_position: i8, _delta_angle: f32) {
        let (boat_entity, player_eid) = {
            let Some(s) = self.sessions.get(&sid) else { return };
            if !s.logged_in { return; }
            let Some(pe) = s.entity else { return };
            let Ok(m) = self.ecs.get::<&Mounted>(pe) else { return };
            if m.station != Some(shared::constants::station::SAIL) { return; }
            let pid = self.ecs.get::<&NetId>(pe).map(|n| n.0).unwrap_or(EntityId(0));
            (m.boat_entity, pid)
        };
        if let Ok(mut tag) = self.ecs.get::<&mut BoatTag>(boat_entity) {
            // Confirma que ele ainda eh o sail_eid (anti-race).
            if tag.sail_eid != Some(player_eid) { return; }
            let new_pos = (tag.sail_position as i16 + delta_position as i16).clamp(0, 2) as u8;
            tag.sail_position = new_pos;
            tag.sail_angle = 0.0; // vela perpendicular fixa
        }
    }

    /// Ajusta o angulo da roda do leme. Player precisa estar na estacao HELM.
    /// delta_angle vem em rad — server soma ao rudder_angle e clampa.
    /// rudder_angle PERSISTE quando o player solta o leme.
    fn handle_helm_adjust(&mut self, sid: SessionId, delta_angle: f32) {
        let (boat_entity, player_eid) = {
            let Some(s) = self.sessions.get(&sid) else { return };
            if !s.logged_in { return; }
            let Some(pe) = s.entity else { return };
            let Ok(m) = self.ecs.get::<&Mounted>(pe) else { return };
            if m.station != Some(shared::constants::station::HELM) { return; }
            let pid = self.ecs.get::<&NetId>(pe).map(|n| n.0).unwrap_or(EntityId(0));
            (m.boat_entity, pid)
        };
        if let Ok(mut tag) = self.ecs.get::<&mut BoatTag>(boat_entity) {
            if tag.helm_eid != Some(player_eid) { return; }
            let max_r = shared::constants::BOAT_MAX_RUDDER_ANGLE;
            tag.rudder_angle = (tag.rudder_angle + delta_angle).clamp(-max_r, max_r);
        }
    }

    /// Integra trajetoria das bombas em voo. Quando t >= t_max → explode
    /// (AoE damage no blast_radius, dano cai linear com distancia). Despawn
    /// no fim.
    fn tick_cannon_bombs(&mut self, dt: f32) {
        struct Explode {
            entity: Entity,
            eid: EntityId,
            pos: Vec2,
            damage: i32,
            blast: f32,
            owner_player_eid: EntityId,
        }
        let mut explosions: Vec<Explode> = Vec::new();
        for (e, (net, pos, bomb)) in self.ecs.query_mut::<(&NetId, &mut Position, &mut CannonBombTag)>() {
            bomb.t_elapsed += dt;
            let t = (bomb.t_elapsed / bomb.t_max).clamp(0.0, 1.0);
            // Trajetoria XY linear; altura parabolica.
            pos.0 = bomb.spawn_pos.lerp(bomb.target_pos, t);
            if bomb.t_elapsed >= bomb.t_max {
                explosions.push(Explode {
                    entity: e, eid: net.0, pos: bomb.target_pos,
                    damage: bomb.damage, blast: bomb.blast_radius,
                    owner_player_eid: bomb.owner_player_eid,
                });
            }
        }
        for ex in explosions {
            // Coleta alvos no AoE: enemies + outros players. Push pra
            // pending_bomb_hits — o pipeline de damage_events vai aplicar
            // PvP rules, hit feedback, crit, knockback, xp etc.
            let mut targets: Vec<(Entity, EntityId, f32, Vec2)> = Vec::new();
            for (e, (net, p, kind)) in self.ecs.query::<(&NetId, &Position, &EntityKind)>().iter() {
                let d = p.0.distance(ex.pos);
                if d > ex.blast { continue; }
                match kind {
                    EntityKind::Enemy(_) | EntityKind::Player => {
                        targets.push((e, net.0, d, p.0));
                    }
                    _ => {}
                }
            }
            for (e, eid, d, tpos) in targets {
                let falloff = (1.0 - d / ex.blast).clamp(0.0, 1.0);
                let dmg = (ex.damage as f32 * falloff).round() as i32;
                if dmg <= 0 { continue; }
                // hurt_dir = TOWARD o ponto de impacto (mesma convencao do
                // pipeline normal: alvo encara o "atacante" e o knockback
                // empurra na direcao oposta).
                let hurt_dir = (ex.pos - tpos).try_normalize().unwrap_or(Vec2::ZERO);
                self.pending_bomb_hits.push((e, eid, dmg, ex.owner_player_eid, hurt_dir, ex.pos));
            }
            // Despawna bomba.
            let _ = self.ecs.despawn(ex.entity);
            self.removed_this_tick.push(ex.eid);
        }
    }

    /// Ajusta angulo de mira do canhao do `slot`. Player precisa estar
    /// na estacao do canhao correspondente. Clamp em [-CANNON_AIM_MAX_RAD, +].
    fn handle_cannon_aim(&mut self, sid: SessionId, slot: u8, angle: f32) {
        use shared::constants::station as st;
        let (boat_entity, player_eid, expected_station) = {
            let Some(s) = self.sessions.get(&sid) else { return };
            if !s.logged_in { return; }
            let Some(pe) = s.entity else { return };
            let Ok(m) = self.ecs.get::<&Mounted>(pe) else { return };
            let pid = self.ecs.get::<&NetId>(pe).map(|n| n.0).unwrap_or(EntityId(0));
            (m.boat_entity, pid, m.station)
        };
        let want_station = st::CANNON_BASE + slot;
        if expected_station != Some(want_station) { return; }
        if let Ok(mut tag) = self.ecs.get::<&mut BoatTag>(boat_entity) {
            if tag.cannon_eids.get(slot as usize).copied().flatten() != Some(player_eid) {
                return;
            }
            let max = shared::constants::CANNON_AIM_MAX_RAD;
            if let Some(a) = tag.cannon_aim.get_mut(slot as usize) {
                *a = angle.clamp(-max, max);
            }
        }
    }

    /// Dispara o canhao com `power` 0..1. Spawn CannonBombTag no slot;
    /// sem colisao ao longo do voo, AoE no impacto.
    fn handle_cannon_fire(&mut self, sid: SessionId, slot: u8, power: f32) {
        use shared::constants::station as st;
        let now = self.sim_time_s;
        let (boat_entity, player_eid, expected_station, owner_pid) = {
            let Some(s) = self.sessions.get(&sid) else { return };
            if !s.logged_in { return; }
            let Some(pe) = s.entity else { return };
            let Ok(m) = self.ecs.get::<&Mounted>(pe) else { return };
            let pid = self.ecs.get::<&NetId>(pe).map(|n| n.0).unwrap_or(EntityId(0));
            let owner_pid = self.ecs.get::<&PlayerTag>(pe).map(|p| p.player_id).unwrap_or(PlayerId(0));
            (m.boat_entity, pid, m.station, owner_pid)
        };
        let want_station = st::CANNON_BASE + slot;
        if expected_station != Some(want_station) { return; }
        let power = power.clamp(0.0, 1.0);
        // Pega dados do barco + canhao.
        let (boat_pos, boat_yaw, kind, aim_angle, on_cd) = {
            let Ok(tag) = self.ecs.get::<&BoatTag>(boat_entity) else { return };
            if tag.cannon_eids.get(slot as usize).copied().flatten() != Some(player_eid) {
                return;
            }
            let aim = tag.cannon_aim.get(slot as usize).copied().unwrap_or(0.0);
            let cd  = tag.cannon_cd_until.get(slot as usize).copied().unwrap_or(0.0);
            let pos = self.ecs.get::<&Position>(boat_entity).map(|p| p.0).unwrap_or(Vec2::ZERO);
            (pos, tag.yaw, tag.kind, aim, cd)
        };
        if now < on_cd {
            return; // cooldown — ignora silenciosamente, UI ja deve estar bloqueando
        }
        let cfg = crate::boat_config::get(kind);
        let Some(base) = cfg.cannon_base(slot as usize) else { return };
        let side_angle = cfg.cannon_side_angle(slot as usize);
        let muzzle_local = cfg.cannon_muzzle(slot as usize);
        // Espelha o muzzle ao redor do canhao pelo aim — assim a bola "sai"
        // de onde o cano aponta apos a rotacao do canhao.
        let offset = muzzle_local - base;
        let (sa, ca) = aim_angle.sin_cos();
        let muzzle_aimed_local = base + Vec2::new(
            offset.x * ca - offset.y * sa,
            offset.x * sa + offset.y * ca,
        );
        // Local → world (rotaciona pelo yaw do barco).
        let (sin_y, cos_y) = boat_yaw.sin_cos();
        let cannon_world = boat_pos + Vec2::new(
            muzzle_aimed_local.x * cos_y - muzzle_aimed_local.y * sin_y,
            muzzle_aimed_local.x * sin_y + muzzle_aimed_local.y * cos_y,
        );
        let _ = base;
        // Direcao de tiro = forward do canhao (yaw - side_angle + aim).
        // Convencao boat_forward(t) = (-sin t, cos t) → t=0 vira +Y (proa).
        // Pra canhao direito (side_angle=+PI/2) queremos +X → theta = yaw - PI/2.
        let theta = boat_yaw - side_angle + aim_angle;
        let dir = Vec2::new(-theta.sin(), theta.cos());
        let range = shared::constants::CANNON_MIN_RANGE
            + (shared::constants::CANNON_MAX_RANGE - shared::constants::CANNON_MIN_RANGE) * power;
        let target = cannon_world + dir * range;
        let t_flight = shared::constants::CANNON_FLIGHT_TIME_MIN
            + (shared::constants::CANNON_FLIGHT_TIME_MAX - shared::constants::CANNON_FLIGHT_TIME_MIN) * power;
        let peak_h = shared::constants::CANNON_PEAK_HEIGHT_MIN
            + (shared::constants::CANNON_PEAK_HEIGHT_MAX - shared::constants::CANNON_PEAK_HEIGHT_MIN) * power;
        let boat_eid = self.ecs.get::<&NetId>(boat_entity).map(|n| n.0).unwrap_or(EntityId(0));
        let bomb_eid = self.alloc_entity_id();
        self.ecs.spawn((
            NetId(bomb_eid),
            Position(cannon_world),
            Velocity(Vec2::ZERO),
            EntityKind::CannonBomb,
            CannonBombTag {
                spawn_pos: cannon_world,
                target_pos: target,
                peak_height: peak_h,
                t_elapsed: 0.0,
                t_max: t_flight,
                damage: shared::constants::CANNON_DAMAGE_BASE,
                blast_radius: shared::constants::CANNON_BLAST_RADIUS,
                owner_pid,
                owner_boat_eid: boat_eid,
                owner_player_eid: player_eid,
            },
        ));
        // Cooldown.
        if let Ok(mut tag) = self.ecs.get::<&mut BoatTag>(boat_entity) {
            if let Some(cd) = tag.cannon_cd_until.get_mut(slot as usize) {
                *cd = now + shared::constants::CANNON_COOLDOWN_S;
            }
        }
        tracing::debug!(
            "cannon fire: boat_eid={:?} slot={} power={:.2} range={:.1} target=({:.1},{:.1})",
            boat_eid, slot, power, range, target.x, target.y
        );
    }

    /// Toggle ancora. Player precisa estar na estacao ANCHOR.
    fn handle_anchor_toggle(&mut self, sid: SessionId) {
        let (boat_entity, player_eid) = {
            let Some(s) = self.sessions.get(&sid) else { return };
            if !s.logged_in { return; }
            let Some(pe) = s.entity else { return };
            let Ok(m) = self.ecs.get::<&Mounted>(pe) else { return };
            if m.station != Some(shared::constants::station::ANCHOR) { return; }
            let pid = self.ecs.get::<&NetId>(pe).map(|n| n.0).unwrap_or(EntityId(0));
            (m.boat_entity, pid)
        };
        if let Ok(mut tag) = self.ecs.get::<&mut BoatTag>(boat_entity) {
            if tag.anchor_eid != Some(player_eid) { return; }
            tag.anchor_dropped = !tag.anchor_dropped;
        }
    }

    /// Captura o estado do personagem de uma sessao especifica (para persistir
    /// no disconnect). Retorna None se nao estiver logado ou ja morto.
    pub fn take_character_for_disconnect(&mut self, sid: &SessionId) -> Option<crate::persistence::CharacterRow> {
        // Tutorial: PERSISTE o progresso (xp/level/inventário/arma) — a posição é
        // mascarada pra ilha-sede do mundo (ver abaixo), pra o player não cair na
        // água do tutorial ao reconectar.
        let session = self.sessions.get(sid)?;
        if !session.logged_in { return None; }
        let e = session.entity?;
        let hp = *self.ecs.get::<&Health>(e).ok()?;
        // Boat 2.5D: captura estado completo do barco + posicao do player
        // no deck (mounted_local). No login restauramos o barco + re-mount
        // do player no mesmo local_pos.
        let mounted_data = self.ecs.get::<&Mounted>(e).ok()
            .map(|m| (m.boat_entity, m.local_pos));
        let (pos, boat, mounted_local) = if let Some((boat_e, local_pos)) = mounted_data {
            let bp = self.ecs.get::<&Position>(boat_e).ok().map(|p| p.0).unwrap_or_default();
            let (kind, dir, yaw, sail_position, sail_angle, anchor_dropped) =
                self.ecs.get::<&BoatTag>(boat_e)
                    .map(|t| (t.kind, t.dir, t.yaw, t.sail_position, t.sail_angle, t.anchor_dropped))
                    .unwrap_or((0, 4, 0.0, 0, 0.0, true));
            let pb = crate::persistence::PersistedBoat {
                kind, pos: bp, dir, yaw,
                sail_position, sail_angle, anchor_dropped,
            };
            // pos do player salva ainda como pos do barco (legacy fallback —
            // no login se restaurar no barco, BFS pra walkable nao roda;
            // se barco nao puder ser restaurado, pos vira o spawn default).
            (bp, Some(pb), Some(local_pos))
        } else {
            let p = self.ecs.get::<&Position>(e).ok()?.0;
            (p, None, None)
        };
        // Tutorial/dungeon: NÃO persiste a pos/barco da lane — senão ao reconectar
        // no mundo o player cairia dentro da instância. Mantém a pos do mundo já
        // salva. inv/xp (loot da dungeon) SÃO persistidos abaixo normalmente.
        let (pos, boat, mounted_local) = if self.tutorial_mode || self.dungeon_mode {
            let world_pos = self.characters.get(&session.name).map(|r| r.pos).unwrap_or(pos);
            (world_pos, None, None)
        } else { (pos, boat, mounted_local) };
        let row = crate::persistence::CharacterRow {
            name: session.name.clone(),
            pos,
            hp,
            xp: session.xp,
            gold: session.gold,
            boat,
            mounted_local,
            inventory: session.inventory.clone(),
            equipment: session.equipment,
            vault: session.vault.clone(),
            fame: session.fame,
            aura: session.aura,
            proficiencies: session.proficiencies,
            unspent_points: session.unspent_points,
            allocated_points: session.allocated_points,
            skill_points_earned: session.skill_points_earned,
            skill_points_spent: session.skill_points_spent,
            learned_skills: session.learned_skills.clone(),
            account_id: session.account_id,
            visual: Some(session.visual.clone()),
            woodcutting_lvl: session.woodcutting_lvl,
            mining_lvl:      session.mining_lvl,
            gathering_lvl:   session.gathering_lvl,
            faction:         session.faction,
            quests:          session.quests.clone(),
            faction_points:  session.faction_points,
            last_tutorial_completed: None, // save não escreve essa coluna (preservada no DB)
        };
        self.characters.insert(session.name.clone(), row.clone());
        Some(row)
    }
}

/// Aplica `item_id` (ou None para remover) num slot de equipamento da sessao.
/// Também atualiza `*_inst` em paralelo — passar `instance` é a forma de
/// preservar rolls/rarity/refinement quando movendo de inventory pra equipment.
fn set_equip(
    session: &mut Session,
    slot: shared::EquipSlot,
    item_id: Option<u16>,
    instance: Option<shared::items::ItemInstance>,
) {
    session.equipment.set(slot, item_id, instance);
}

fn read_equip_instance(equip: &shared::Equipment, slot: shared::EquipSlot) -> Option<shared::items::ItemInstance> {
    equip.get_inst(slot)
}

/// Pode colocar `item_id` no `slot`, dado o estado atual de `equipment`?
/// Cobre dois casos: (1) item vai pro slot certo; (2) Offhand exige weapon
/// que permita two-handed. Centraliza a regra pra todos os caminhos de
/// equip (UseItem, loot pickup, shop buy, drag-drop).
fn can_equip_in_slot(equipment: &shared::Equipment, slot: shared::EquipSlot, item_id: u16) -> bool {
    if shared::equip_slot_of(item_id) != Some(slot) { return false; }
    if slot == shared::EquipSlot::Offhand {
        let weapon = equipment.weapon.unwrap_or(0);
        if !shared::weapon_allows_offhand(weapon) { return false; }
    }
    true
}

/// Auto-unequip do offhand quando o player troca pra weapon two-handed.
/// Move o item do offhand pro inventario; se o inv estiver cheio, retorna
/// false e o caller deve abortar o equip da weapon (mantendo estado consistente).
/// No-op se a weapon nova permite offhand ou se offhand já está vazio.
fn maybe_unequip_offhand_for_weapon(session: &mut Session, new_weapon: u16) -> bool {
    if shared::weapon_allows_offhand(new_weapon) { return true; }
    let Some(oh) = session.equipment.offhand else { return true; };
    let oh_inst = session.equipment.offhand_inst;
    if !add_to_inventory(&mut session.inventory, oh, 1, oh_inst) { return false; }
    session.equipment.offhand = None;
    session.equipment.offhand_inst = None;
    session.inventory_dirty = true;
    true
}

/// Quanto poise o buffer de defending tem ao iniciar um block. Escala com
/// char_lvl (30 por 10 lvls) e ganha 1.25x se equipou escudo no offhand.
fn defending_poise_max(session: &Session) -> f32 {
    let char_lvl = shared::level_of_xp_with_mult(session.xp, crate::economy::xp_multiplier());
    let base = 30.0 * ((char_lvl as f32) / 10.0).floor();
    let shield_mult = if session.equipment.offhand
        .map_or(false, |id| shared::constants::is_shield(id)) { 1.25 } else { 1.0 };
    base * shield_mult
}

/// Calcula stats efetivos = base + pontos alocados + equip + scaling da
/// prof da arma equipada.
fn effective_stats(
    equip: &shared::Equipment,
    allocated: &[u32; shared::STAT_COUNT],
    proficiencies: &[u64; shared::PROF_COUNT],
    learned_skills: &[shared::LearnedSkill],
    char_xp: u64,
) -> shared::PlayerStats {
    let mut s = shared::base_player_stats();

    // Poise base — concedido a partir de POISE_BASE_UNLOCK_LEVEL (lvl 60).
    // Skills T4 (Iron Will, Unstoppable, etc) somam +50/rank em cima disso.
    let char_lvl = shared::level_of_xp_with_mult(char_xp, crate::economy::xp_multiplier());
    if (char_lvl as u8) >= shared::constants::POISE_BASE_UNLOCK_LEVEL {
        s.poise_max += shared::constants::POISE_BASE_VALUE;
    }

    // Pontos alocados pelo player (FOR/DES/INT/VIT/SPD).
    for (i, &pts) in allocated.iter().enumerate() {
        if i >= shared::STAT_POINT_BONUS.len() || pts == 0 { continue; }
        let b = shared::STAT_POINT_BONUS[i];
        let p = pts as i32;
        s.hp_max += b.hp_max * p;
        s.mp_max += b.mp_max * p;
        s.attack_damage += b.attack_damage * p;
        s.dex += b.dex * p;
        s.wis += b.wis * p;
        s.defense += b.defense * p;
        s.speed_mult              += b.speed_pct             * pts as f32;
        s.crit_chance             += b.crit_chance           * pts as f32;
        s.hp_regen                += b.hp_regen              * pts as f32;
        s.attack_speed_mult       += b.attack_speed_pct      * pts as f32;
        s.stamina_max             += b.stamina_max           * p;
        s.stamina_regen           += b.stamina_regen         * pts as f32;
        s.block_dmg_reduction     += b.block_reduction_bonus * pts as f32;
        s.defense_stamina_cost_mult -= b.stamina_cost_reduction * pts as f32;
        s.dash_cd_mult            += b.dash_cd_reduction_pct * pts as f32;
    }

    // Breakpoints de stat — milestones que dão build identity.
    // VIT: cada 25 pontos → +5% damage reduction (cap 75% via clamp downstream).
    let vit = allocated[shared::stat_idx::VIT];
    s.damage_reduction_pct += (vit / 25) as f32 * 0.05;
    // RES: cada 30 pontos → +5% damage reduction (stack com VIT).
    let res = allocated[shared::stat_idx::RES];
    s.damage_reduction_pct += (res / 30) as f32 * 0.05;

    // Bonus do equipamento. Cada slot tem item_id (base bonus via
    // item_bonus) + Option<ItemInstance> (rolls aleatorios × rarity ×
    // refinement). Instance None = item legacy → só base bonus.
    let slot_pairs = equip.iter_equipped();
    // Set tracking — count quantas peças de cada set_id estão equipadas.
    let mut set_pieces: std::collections::HashMap<u8, u8> = std::collections::HashMap::new();
    for (id_opt, _) in &slot_pairs {
        if let Some(id) = id_opt {
            let sid = shared::items::item_set_id(*id);
            if sid > 0 {
                *set_pieces.entry(sid).or_insert(0) += 1;
            }
        }
    }
    for (id_opt, inst_opt) in slot_pairs {
        if let Some(id) = id_opt {
            // Base bonus: stats fixos do item_id (legado / fallback)
            let b = shared::item_bonus(id);
            s.hp_max        += b.hp_max;
            s.mp_max        += b.mp_max;
            s.attack_damage += b.attack_damage;
            s.dex           += b.dex;
            s.wis           += b.wis;
            s.defense       += b.defense;
            // Instance rolls (rolled at drop time × refinement) + affixes
            if let Some(inst) = inst_opt {
                let ib = inst.effective_bonus();
                s.hp_max        += ib.hp_max;
                s.mp_max        += ib.mp_max;
                s.attack_damage += ib.attack_damage;
                s.dex           += ib.dex;
                s.wis           += ib.wis;
                s.defense       += ib.defense;
                let (crit, atks, mov, hpr) = inst.effective_pct_bonus();
                s.crit_chance       += crit;
                s.attack_speed_mult += atks;
                s.speed_mult        += mov;
                s.hp_regen          += hpr;
            }
        }
    }

    // Aplica set bonuses (Fase D). Cada set ativa em 2+ peças.
    for (&sid, &pieces) in set_pieces.iter() {
        let sb = shared::items::set_bonus_for(sid, pieces);
        s.hp_max            += sb.hp;
        s.mp_max            += sb.mp;
        s.attack_damage     += sb.atk;
        s.defense           += sb.def;
        s.crit_chance       += sb.crit;
        s.attack_speed_mult += sb.atk_spd;
    }

    // Scaling da proficiencia da arma EQUIPADA.
    let weapon_id = equip.weapon.unwrap_or(0);
    let prof = shared::Proficiency::from_item(weapon_id);
    let prof_idx = prof as usize;
    let prof_lvl = if prof_idx < proficiencies.len() {
        shared::proficiency_level(proficiencies[prof_idx])
    } else { 1 };
    let scaling = if weapon_id == 0 {
        shared::unarmed_scaling()
    } else {
        shared::weapon_scaling(weapon_id)
    };
    let lvl = prof_lvl as f32;
    s.hp_max += (scaling.hp_max * lvl) as i32;
    s.mp_max += (scaling.mp_max * lvl) as i32;
    s.attack_damage += (scaling.attack_damage * lvl) as i32;
    s.dex += (scaling.dex * lvl) as i32;
    s.wis += (scaling.wis * lvl) as i32;
    s.defense += (scaling.defense * lvl) as i32;

    // Atk-scaling extra por stat secundario:
    //   Bow/Crossbow: cada DEX = +0.5 atk (ranger usa destreza) + atk_speed
    //   Spear:        DEX bonus baixo (1/5), FOR bonus alto (1/1)
    let for_pts = allocated[shared::stat_idx::FOR] as i32;
    match weapon_id {
        id if id == shared::item_id::BOW
            || id == shared::item_id::ENHANCED_BOW
            || id == shared::item_id::CROSSBOW => {
            s.attack_damage += s.dex / 2;
            s.attack_speed_mult += 0.25; // bow disparo rapido
        }
        id if id == shared::item_id::SPEAR
            || id == shared::item_id::ENHANCED_SPEAR => {
            s.attack_damage += s.dex / 5 + for_pts;
            s.attack_speed_mult -= 0.15; // lanca eh lenta (alcance compensa)
        }
        _ => {}
    }

    // Skill passives — sempre-ativas se aprendidas. M13: skills viraram
    // universais (não dependem mais da arma equipada bater com `prof`).
    // Passives que checam arma específica (ex: Hardened Fists exige unarmed)
    // fazem o check inline no match abaixo.
    for ls in learned_skills {
        if ls.rank == 0 { continue; }
        let Some(def) = crate::skills::skill_of(ls.skill_id) else { continue };
        if !def.is_passive { continue; }
        let r = ls.rank as i32;
        // Mapeamento per-skill dos efeitos. Por enquanto hardcoded;
        // futuramente migra pra effect_payload no DB.
        match ls.skill_id {
            // Mana Pool — Wand T1 P: +5 mp_max/rank, r5: +0.5 mp_regen, r10: -5% spell cost
            1042 => {
                s.mp_max += 5 * r;
                // r5 / r10 milestones quando implementarmos mp_regen/cost runtime.
            }
            // Iron Will — Sword T4 P: +50 poise + (placeholder) <30% HP -30% dmg taken
            1008 => { s.poise_max += 50 * r; }
            // Unstoppable / Spearman's Resolve / Master's Form — T4 P de defesa
            // tambem concedem poise (gateamento de poise: lvl 60 + prof 40).
            1016 | 1024 | 1064 => { s.poise_max += 50 * r; }
            // Combat Stance — Sword T1 P: +1.5%/rank atk speed (Sword/Dagger)
            1002 => { s.attack_speed_mult += 0.015 * r as f32; }
            // Bulwark — Sword T2 P: +1%/rank block reduction. Soma direto
            // ao block_dmg_reduction (cap em BLOCK_REDUCTION_MAX = 0.95).
            1004 => { s.block_dmg_reduction += 0.01 * r as f32; }
            // Heavy Hands — Axe T1 P: +1%/rank atk dmg (Axe/Sword)
            1010 => { s.attack_damage += s.attack_damage * r / 100; }
            // Sharp Edge — Dagger T1 P: +0.3%/rank crit chance
            1026 => { s.crit_chance += 0.003 * r as f32; }
            // Eagle Eye — Bow T1 P: +5%/rank range. Aplicado no spawn do
            // projetil escalando o TTL (mais tempo voando = mais range).
            1034 => { s.bow_range_bonus_pct += 0.05 * r as f32; }
            // Mana Conduit — Staff T1 P: -1%/rank mp cost (aplicado em handle_skill_cast)
            1050 => { /* aplicado em cast cost */ }
            // Hardened Fists — Unarmed T1 P: +2/rank atk dmg unarmed
            1058 => {
                if equip.weapon.is_none() || equip.weapon == Some(0) {
                    s.attack_damage += 2 * r;
                }
            }
            // Outras passivas (T2/T3/T4) implementadas progressivamente.
            _ => {}
        }
    }

    // Garantir minimos / clamps
    s.hp_max = s.hp_max.max(1);
    s.mp_max = s.mp_max.max(0);
    s.attack_damage = s.attack_damage.max(1);
    s.defense = s.defense.max(0);
    s.block_dmg_reduction = s.block_dmg_reduction.clamp(0.0, shared::BLOCK_REDUCTION_MAX);
    s.defense_stamina_cost_mult = s.defense_stamina_cost_mult.max(shared::STAMINA_COST_MULT_MIN);
    s
}

/// Calcula hurt_dir (TOWARD attacker) a partir do net_id do alvo.
/// Free fn pra ser usada de fora do step() (ex: skill cast handler).
fn calc_hurt_dir_from_eid(ecs: &World, target_net: EntityId, attacker_pos: Vec2) -> Vec2 {
    for (_, (net, pos)) in ecs.query::<(&NetId, &Position)>().iter() {
        if net.0 == target_net {
            return (attacker_pos - pos.0).try_normalize().unwrap_or(Vec2::ZERO);
        }
    }
    Vec2::ZERO
}

/// Tenta adicionar um item ao inventario. Stacka em slots existentes primeiro;
/// se nao couber, procura slot vazio. Retorna true se coube (parcial ou total
/// dentro do stack do primeiro slot achado — se nao couber NADA, retorna false).
fn add_to_inventory(
    inv: &mut [shared::InventorySlot],
    item_id: u16,
    mut qty: u32,
    instance: Option<shared::items::ItemInstance>,
) -> bool {
    let max_stack = crate::economy::item_stack_max(item_id);
    // Equipáveis com instance NÃO stackam — cada drop é único. Vai
    // direto pra slot vazio.
    if instance.is_some() {
        if let Some(empty) = inv.iter_mut().find(|s| s.qty == 0) {
            empty.item_id = item_id;
            empty.qty = qty.max(1);
            empty.instance = instance;
            return true;
        }
        return false;
    }
    // Stackáveis: combina com slots existentes (instance None) primeiro.
    for slot in inv.iter_mut() {
        if slot.qty > 0 && slot.item_id == item_id && slot.qty < max_stack && slot.instance.is_none() {
            let room = max_stack - slot.qty;
            let add = qty.min(room);
            slot.qty += add;
            qty -= add;
            if qty == 0 { return true; }
        }
    }
    while qty > 0 {
        let Some(empty) = inv.iter_mut().find(|s| s.qty == 0) else { break };
        let add = qty.min(max_stack);
        empty.item_id = item_id;
        empty.qty = add;
        empty.instance = None;
        qty -= add;
    }
    qty == 0
}

// LCG deterministico para wander de inimigos (sem dep de rand)
fn lcg(seed: u64) -> u64 {
    seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407)
}

fn lcg_f32(seed: u64) -> f32 {
    (seed >> 11) as f32 / (1u64 << 53) as f32
}

/// Escolhe um waypoint random walkable dentro de raio em torno da ancora.
/// Tenta `tries` vezes; fallback retorna `current` (fica parado).
fn pick_waypoint(
    map: &shared::world_gen::WorldMap,
    current: glam::Vec2,
    anchor: glam::Vec2,
    radius: f32,
    seed_in: u64,
    tries: u32,
    safe_zones: &[(Vec2, Vec2)],
) -> glam::Vec2 {
    let r = if radius > 0.0 { radius } else { 5.0 };
    let mut s = seed_in;
    for _ in 0..tries {
        s = lcg(s);
        let angle = lcg_f32(s) * std::f32::consts::TAU;
        s = lcg(s);
        let dist = lcg_f32(s) * r;
        let target = anchor + glam::Vec2::new(angle.cos(), angle.sin()) * dist;
        let tx = target.x.floor() as i32;
        let ty = target.y.floor() as i32;
        if map.get(tx, ty) == shared::constants::tile_id::FLOOR {
            // Não escolhe waypoint a < 1 tile da pos atual (precisa andar algo)
            let center = glam::Vec2::new(tx as f32 + 0.5, ty as f32 + 0.5);
            // Nem dentro de safe zone (cidade) — evita mob vagando pro muro.
            let in_safe = safe_zones.iter().any(|(o, sz)|
                center.x >= o.x && center.x <= o.x + sz.x &&
                center.y >= o.y && center.y <= o.y + sz.y);
            if !in_safe && (center - current).length() > 1.0 {
                return center;
            }
        }
    }
    current
}

/// Aplica wander com waypoints aleatórios + fases walk/pause alternadas.
/// Se pulling_home, EVADE: vai direto pro anchor com speed 1.5×.
fn apply_wander(
    enemy: &mut EnemyTag,
    vel: &mut glam::Vec2,
    pos: glam::Vec2,
    speed: f32,
    map: &shared::world_gen::WorldMap,
    safe_zones: &[(Vec2, Vec2)],
    tick: u32,
    net_id: u32,
    pulling_home: bool,
) {
    if pulling_home {
        // EVADE: speed 1.5× direto pro anchor (estilo WoW Classic).
        let home_dir = (enemy.spawn_anchor - pos).try_normalize().unwrap_or(glam::Vec2::X);
        enemy.wander_dir = home_dir;
        enemy.wander_phase = 0;
        enemy.wander_timer = 0.5;
        *vel = home_dir * speed * 1.5;
        return;
    }

    let seed = tick as u64 ^ net_id as u64 ^ 0xCAFE;

    // Walk phase: vai em direção ao waypoint até chegar ou timeout.
    if enemy.wander_phase == 0 {
        let to_wp = enemy.wander_waypoint - pos;
        let dist = to_wp.length();
        let arrived = dist < 0.3;
        let timed_out = enemy.wander_timer <= 0.0;
        if arrived || timed_out {
            enemy.wander_phase = 1;
            enemy.wander_timer = 1.2 + lcg_f32(lcg(seed)) * 2.0; // 1.2-3.2s pause
            *vel = glam::Vec2::ZERO;
        } else {
            let dir = to_wp / dist;
            *vel = dir * speed * 0.4;
        }
    } else {
        // Pause phase: parado até timer expirar, aí escolhe novo waypoint.
        if enemy.wander_timer <= 0.0 {
            enemy.wander_waypoint = pick_waypoint(
                map, pos, enemy.spawn_anchor, enemy.leash_max, seed, 30, safe_zones);
            enemy.wander_phase = 0;
            enemy.wander_timer = 4.0; // safety timeout (caso não consiga chegar)
        }
        *vel = glam::Vec2::ZERO;
    }
}

/// Floor absoluto pro min_dist do Poisson — slots nunca menos que 3 tiles
/// apart pra evitar mobs literalmente em cima.
const SPAWN_SLOT_MIN_DIST_FLOOR: f32 = 3.0;
/// Ceiling pro min_dist — limita spacing maximo em zonas grandes pra densidade
/// nao virar microscopica.
const SPAWN_SLOT_MIN_DIST_MAX: f32 = 14.0;

/// Computa min_dist adaptativo: spacing alvo pra que `target_count` slots
/// caibam na area usavel (polygon ou AABB). Resolve o problema de polygon
/// estreito onde min_dist constante deixa narrows sem slots.
fn adaptive_min_dist(usable_area: f32, target_count: u32) -> f32 {
    let target = target_count.max(1) as f32;
    let raw = (usable_area / target).sqrt() * 0.85;
    raw.clamp(SPAWN_SLOT_MIN_DIST_FLOOR, SPAWN_SLOT_MIN_DIST_MAX)
}

/// Area de polygon via shoelace formula. Vertices em ordem (CW ou CCW).
fn polygon_area(verts: &[Vec2]) -> f32 {
    if verts.len() < 3 { return 0.0; }
    let mut area = 0.0_f32;
    for i in 0..verts.len() {
        let j = (i + 1) % verts.len();
        area += verts[i].x * verts[j].y - verts[j].x * verts[i].y;
    }
    (area * 0.5).abs()
}

/// Bridson's Poisson Disk Sampling — gera pontos uniformemente espaçados
/// dentro de um polygon (ou AABB se polygon=None). Garantia: cada par de
/// pontos tem distância ≥ `min_dist`. Spawning resultante e' visualmente
/// uniforme (sem clusters), simulando spawns "manuais" de level designer.
///
/// Filtra: tiles WALL/WATER (mob nao spawna em parede ou agua).
/// Algoritmo O(n) onde n=slots gerados — grid acelera neighbor lookup.
fn poisson_disk_in_polygon(
    polygon: Option<&[Vec2]>,
    aabb_min: Vec2,
    aabb_size: Vec2,
    min_dist: f32,
    map: &shared::world_gen::WorldMap,
    seed: u64,
) -> Vec<Vec2> {
    fastrand::seed(seed);
    let cell_size = min_dist / 1.4142135f32;
    let cols = ((aabb_size.x / cell_size).ceil() as i32).max(1);
    let rows = ((aabb_size.y / cell_size).ceil() as i32).max(1);
    let mut grid: Vec<Option<Vec2>> = vec![None; (cols * rows) as usize];
    let to_cell = |p: Vec2| -> (i32, i32) {
        let cx = ((p.x - aabb_min.x) / cell_size) as i32;
        let cy = ((p.y - aabb_min.y) / cell_size) as i32;
        (cx.clamp(0, cols - 1), cy.clamp(0, rows - 1))
    };
    let valid_at = |p: Vec2| -> bool {
        if p.x < aabb_min.x || p.x > aabb_min.x + aabb_size.x { return false; }
        if p.y < aabb_min.y || p.y > aabb_min.y + aabb_size.y { return false; }
        if let Some(poly) = polygon {
            if !point_in_polygon(p, poly) { return false; }
        }
        let tx = p.x.floor() as i32;
        let ty = p.y.floor() as i32;
        let t = map.get(tx, ty);
        if t == shared::constants::tile_id::WALL || t == shared::constants::tile_id::WATER { return false; }
        true
    };
    let no_neighbor_too_close = |p: Vec2, grid: &Vec<Option<Vec2>>| -> bool {
        let (cx, cy) = to_cell(p);
        for dy in -2i32..=2 {
            for dx in -2i32..=2 {
                let nx = cx + dx; let ny = cy + dy;
                if nx < 0 || nx >= cols || ny < 0 || ny >= rows { continue; }
                if let Some(other) = grid[(ny * cols + nx) as usize] {
                    if p.distance(other) < min_dist { return false; }
                }
            }
        }
        true
    };

    let mut result: Vec<Vec2> = Vec::new();
    let mut active: Vec<Vec2> = Vec::new();

    // Initial point — tenta ate 200 tiros pra encontrar um spot valido.
    for _ in 0..200 {
        let p = Vec2::new(
            aabb_min.x + fastrand::f32() * aabb_size.x,
            aabb_min.y + fastrand::f32() * aabb_size.y,
        );
        if valid_at(p) {
            let (cx, cy) = to_cell(p);
            grid[(cy * cols + cx) as usize] = Some(p);
            result.push(p);
            active.push(p);
            break;
        }
    }

    const K: u32 = 30;
    while !active.is_empty() {
        let i = fastrand::usize(..active.len());
        let p = active[i];
        let mut placed = false;
        for _ in 0..K {
            let angle = fastrand::f32() * std::f32::consts::TAU;
            let dist = min_dist + fastrand::f32() * min_dist;
            let cand = Vec2::new(p.x + dist * angle.cos(), p.y + dist * angle.sin());
            if !valid_at(cand) { continue; }
            if !no_neighbor_too_close(cand, &grid) { continue; }
            let (cx, cy) = to_cell(cand);
            grid[(cy * cols + cx) as usize] = Some(cand);
            result.push(cand);
            active.push(cand);
            placed = true;
            break;
        }
        if !placed {
            active.swap_remove(i);
        }
    }
    result
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// Computa tier 1-4 pra exibicao client-side do halo de loot. Resources
/// (60-71) tem tier embutido no item_id; equipaveis com instance derivam
/// do item_level. None pra itens sem tier (gold, pocoes, etc.).
fn compute_loot_tier(l: &LootTag) -> Option<u8> {
    if l.item_id >= 60 && l.item_id <= 71 {
        return Some(((l.item_id - 60) % 4 + 1) as u8);
    }
    if let Some(inst) = l.instance {
        let lvl = inst.item_level;
        let t = if lvl <= 10 { 1 }
        else if lvl <= 30 { 2 }
        else if lvl <= 60 { 3 }
        else { 4 };
        return Some(t);
    }
    None
}

/// Sort + merge in-place de um conjunto de inventory slots. Itens stackaveis
/// (instance==None) sao agrupados por item_id (respeitando stack_max), ordem
/// ascendente. Itens com instance (rolls/refinement) sao mantidos individuais
/// e ordenados ao final por item_id. Slots vazios ficam no fim.
fn auto_arrange_slots(slots: &mut Vec<shared::InventorySlot>) {
    use std::collections::BTreeMap;
    let n = slots.len();
    let mut totals: BTreeMap<u16, u32> = BTreeMap::new();
    let mut instances: Vec<shared::InventorySlot> = Vec::new();
    for s in slots.iter() {
        if s.qty == 0 { continue; }
        if s.instance.is_some() {
            instances.push(*s);
        } else {
            *totals.entry(s.item_id).or_insert(0) += s.qty;
        }
    }
    instances.sort_by_key(|s| s.item_id);
    // Limpa o vetor.
    for s in slots.iter_mut() {
        *s = shared::InventorySlot::default();
    }
    let mut idx = 0usize;
    for (id, mut qty) in totals {
        let cap = crate::economy::item_stack_max(id).max(1);
        while qty > 0 && idx < n {
            let take = qty.min(cap);
            slots[idx] = shared::InventorySlot { item_id: id, qty: take, instance: None };
            qty -= take;
            idx += 1;
        }
    }
    for ii in instances {
        if idx >= n { break; }
        slots[idx] = ii;
        idx += 1;
    }
}
