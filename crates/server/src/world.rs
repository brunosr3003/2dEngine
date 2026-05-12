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
    /// Char criado via CharacterCreationUI — task async ja escreveu no DB
    /// e leu de volta o row recem-criado. World insere no cache + roda
    /// on_auth_result com AuthSuccess sintetico.
    CharCreated(SessionId, Box<crate::persistence::CharacterRow>, crate::auth::AuthSuccess),
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

/// Tag em NPC ambiental que anda por uma rota (waypoints em loop).
/// `current_idx` é o waypoint atual; `pause_until` é sim_time pra retomar
/// movimento depois de chegar num waypoint (pequena pausa pra naturalidade).
#[derive(Clone)]
pub struct WanderRouteTag {
    pub route_id:    u32,
    pub current_idx: u32,
    pub pause_until: f32,
    pub name:        String,
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
}

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
    pub target: Vec2,
    /// Cooldown pra evitar teletransporte infinito quando chega no destino.
    pub cooldown: f32,
}

/// Barco navegavel. Spawnado quando um player usa um item de barco numa
/// margem walkable; despawnado no dismount. Move-se exclusivamente em tiles
/// WATER. Sem rigid body — server move a Position direto e bloqueia em tiles
/// nao-water, sem sliding.
pub struct BoatTag {
    /// Kind visual do barco (0=Lylian Leutard). Cliente usa pra escolher
    /// quais sprite-sheets carregar.
    pub kind: u16,
    /// PlayerId do dono — o unico autorizado a montar/dirigir.
    pub owner_pid: PlayerId,
    /// EntityId do passageiro atual (player montado). None se ancorado vazio.
    pub passenger: Option<EntityId>,
    /// hecs::Entity do passageiro pra acesso direto sem query.
    pub passenger_entity: Option<Entity>,
    /// Direcao do casco (0=N, 1=NE, 2=E, 3=SE, 4=S, 5=SW, 6=W, 7=NW). Atualizado
    /// pelo movimento; congelado em idle quando velocidade ≈ 0.
    pub dir: u8,
    /// Animacao atual (0=idle, 1=movement, 2=shoot). Idle quando |vel|<eps.
    pub anim: u8,
    /// Velocidade max em tiles/s. Lylian Leutard = 4.5 (devagar pra exploracao).
    pub max_speed: f32,
    /// sim_time_s do ultimo tiro de canhao. Usado pra cooldown de cannon.
    pub last_shot_at_s: f32,
    /// Quando > sim_time_s, animacao 'shoot' fica ativa visualmente. Setado
    /// no tiro pra duracao da anim (~0.96s = 8 frames * 0.12s).
    pub shoot_anim_until: f32,
    /// Direcao do ULTIMO tiro (0..7), independente da direcao do barco.
    /// Cliente usa pra rotacionar o flash overlay pro mouse, nao pro casco.
    pub shoot_dir: u8,
}

/// Marca um player como montado num barco. Enquanto presente:
/// - Input do player redireciona pro barco (BoatTag.passenger)
/// - Player nao processa colisao terrestre (rigid body congelado)
/// - Position do player espelha pos do barco a cada tick
pub struct Mounted {
    pub boat_entity: Entity,
    pub boat_eid: EntityId,
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
    /// Farm skill levels (persistidos). Influenciam hits_required do cliente
    /// e são enviados via FarmSkillsUpdate no login. Default 1.
    pub woodcutting_lvl: u32,
    pub mining_lvl:      u32,
    pub gathering_lvl:   u32,
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
            save_pending: false,
            pending_shots: Vec::new(),
            pending_skill_hits: Vec::new(),
            last_chain_pts: Vec::new(),
            pending_heals: Vec::new(),
            pending_delayed_aoe: Vec::new(),
            hit_this_tick: HashMap::new(),
            crit_this_tick: HashMap::new(),
            damage_this_tick: HashMap::new(),
            attacker_weapon_this_tick: HashMap::new(),
            last_econ_version: 0,
            farm_nodes: HashMap::new(),
            farm_hit_cooldowns: HashMap::new(),
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
            save_pending: false,
            pending_shots: Vec::new(),
            pending_skill_hits: Vec::new(),
            last_chain_pts: Vec::new(),
            pending_heals: Vec::new(),
            pending_delayed_aoe: Vec::new(),
            hit_this_tick: HashMap::new(),
            crit_this_tick: HashMap::new(),
            damage_this_tick: HashMap::new(),
            attacker_weapon_this_tick: HashMap::new(),
            last_econ_version: 0,
            farm_nodes: HashMap::new(),
            farm_hit_cooldowns: HashMap::new(),
        };
        w.spawn_mapfile_entities(&mf);
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

        for zi in 0..self.spawn_zones.len() {
            let zone_size = self.spawn_zones[zi].size;
            let zone_orig = self.spawn_zones[zi].origin;
            let zone_id = self.spawn_zones[zi].id;

            // Modo level-range com slots Poisson: itera slots fixos. Spawna
            // num slot que esta livre (occupant=None) e respawn_at <= now.
            // Tipo (level+classe) sortado a cada spawn — diversidade visual.
            if let Some((lvl_min, lvl_max, _total_count)) = self.spawn_zones[zi].level_range {
                let _ = zone_orig; let _ = zone_size; // unused no slot path
                let n_slots = self.spawn_zones[zi].slots.len();
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
        );
        let hp_max = stats.hp_max;
        let tag = EnemyTag {
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
        };
        (tag, Health { current: hp_max, max: hp_max })
    }

    /// Tenta castar a primeira skill aprendida cujo cooldown + MP + range +
    /// weapon usable_with batem com a situacao. Retorna `Some(skill_id)` se
    /// castou (caller skipa auto-attack); `None` senao.
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
            let prof = shared::Proficiency::from_item(weapon_id).as_db_str();
            if let Some(uw) = &def.usable_with {
                if !uw.is_empty() && !uw.iter().any(|p| p == prof) { continue; }
            }
            let dist = enemy_pos.distance(target_pos);
            if dist > def.range_tiles + def.radius_tiles { continue; }
            let dmg = def.base_damage
                + ((attack_damage as f32 * def.scaling_atk) as i32)
                + (def.base_damage as f32 * def.per_rank_dmg_pct
                    * (ls.rank.saturating_sub(1) as f32)) as i32;
            match def.target_type.as_str() {
                "aoe_circle" | "cone" => {
                    let radius = def.radius_tiles.max(0.5);
                    let players = self.find_players_in_radius(target_pos, radius);
                    for pid in &players {
                        let hd = (-(target_pos - enemy_pos)).try_normalize().unwrap_or(Vec2::Y);
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
                skill_id: ls.skill_id, caster_pos: enemy_pos, target_pos,
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
            let center = zone.origin + zone.size * 0.5;
            let r = zone.size.x.max(zone.size.y) * 0.6;
            (center, r)
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
            // Remove physics body se existir.
            if let Ok(h) = self.ecs.get::<&shared::PhysicsHandle>(e).map(|h| h.0) {
                self.physics.rigid_body_set.remove(
                    h, &mut self.physics.island_manager,
                    &mut self.physics.collider_set,
                    &mut self.physics.impulse_joint_set,
                    &mut self.physics.multibody_joint_set,
                    true,
                );
            }
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
                            pause_until: self.sim_time_s + 1.0,
                            name:        name.clone(),
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
                MapEntity::SafeZone { size } => {
                    self.safe_zones.push((pos, Vec2::new(size[0], size[1])));
                    tracing::info!(
                        "mapfile: safe zone at ({:.1},{:.1}) {}x{}",
                        pos.x, pos.y, size[0], size[1]
                    );
                }
                MapEntity::EnemySpawner { size, quotas, respawn_delay_s, polygon, level_min, level_max, count } => {
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
                        // Min_dist adaptativo — narrow polygons reduzem spacing
                        // pra caber slots em corredores estreitos. Sem isso,
                        // bridson nao alcanca areas estreitas e mobs ficam
                        // todos na area larga (= clustered visualmente).
                        let usable_area = polygon_world.as_ref()
                            .map(|p| polygon_area(p))
                            .unwrap_or(zone_size.x * zone_size.y);
                        let min_dist = adaptive_min_dist(usable_area, target_c);
                        let slot_seed = ((zone_id as u64).wrapping_mul(0x9E37_79B9))
                            ^ 0xC0FFEE_BAD_DEED_u64;
                        let positions = poisson_disk_in_polygon(
                            polygon_world.as_deref(),
                            pos,
                            zone_size,
                            min_dist,
                            &self.map,
                            slot_seed,
                        );
                        tracing::info!(
                            "zona #{}: {} slots Poisson (min_dist={:.1} adapt, target={}, area={:.0})",
                            zone_id, positions.len(), min_dist, target_c, usable_area,
                        );
                        let mut v: Vec<SpawnSlot> = positions.iter()
                            .map(|&p| SpawnSlot { pos: p, occupant: None, respawn_at: 0.0 })
                            .collect();
                        v.truncate(target_c as usize);
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
            PortalTag { target: dungeon_target, cooldown: 0.0 },
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
            PortalTag { target: nexus_spawn, cooldown: 1.0 }, // cooldown inicial pra nao quicar
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
        s.stats = effective_stats(&s.equipment, &s.allocated_points, &s.proficiencies, &s.learned_skills);
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
        s.stats = effective_stats(&s.equipment, &s.allocated_points, &s.proficiencies, &s.learned_skills);
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
        s.stats = effective_stats(&s.equipment, &s.allocated_points, &s.proficiencies, &s.learned_skills);
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
             saved_visual, saved_char_name) = (
                row.pos, row.hp, row.xp, row.gold, row.inventory.clone(), row.equipment, row.vault.clone(),
                row.fame, row.aura, row.proficiencies, row.unspent_points, row.allocated_points,
                row.skill_points_earned, row.skill_points_spent, row.learned_skills.clone(),
                row.boat, row.visual.clone(), row.name.clone(),
            );
        // Stats efetivos considerando equipamento salvo + pontos + profs.
        let stats = effective_stats(&saved_equip, &saved_alloc, &saved_profs, &saved_learned_skills);
        // Re-sincroniza o max_hp (classe pode ter sido rebalanceada entre sessoes).
        health.max = stats.hp_max;
        if health.current > health.max { health.current = health.max; }
        if health.current <= 0 { health.current = stats.hp_max; }
        // Valida que a posicao salva nao esta dentro de uma parede (mapa
        // pode ter sido regenerado). Senao, volta pro spawn default.
        // Excecao: se tinha barco salvo, pos vai ser em agua — eh OK porque
        // vamos re-mountar o player no barco logo apos.
        let tx = spawn.x.floor() as i32;
        let ty = spawn.y.floor() as i32;
        if saved_boat.is_some() && self.map.is_water(tx, ty) {
            // Pos em agua eh esperado quando re-spawn em barco. Skip check.
        } else if !self.map.is_walkable(tx, ty) {
            tracing::warn!("saved pos ({tx},{ty}) nao-walkable (wall/water); usando spawn default");
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
            PlayerTag { name: saved_char_name.clone(), player_id: pid, attack_anim_pending: None, combo_step_pending: None },
        ));

        // Restaura barco se player desconectou montado. Spawna a entidade
        // Boat na pos salva e adiciona Mounted no player. Se a pos do barco
        // virou nao-water (mapa mudou), aborta restauracao — player continua
        // a pe na pos do default_spawn (que foi clampado acima).
        if let Some(b) = saved_boat {
            let bx = b.pos.x.floor() as i32;
            let by = b.pos.y.floor() as i32;
            if self.map.is_water(bx, by) {
                let boat_eid = self.alloc_entity_id();
                let boat_entity = self.ecs.spawn((
                    NetId(boat_eid),
                    Position(b.pos),
                    Velocity(Vec2::ZERO),
                    EntityKind::Boat(b.kind),
                    BoatTag {
                        kind: b.kind,
                        owner_pid: pid,
                        passenger: Some(entity_id),
                        passenger_entity: Some(e),
                        dir: b.dir,
                        anim: 0,
                        max_speed: 4.5,
                        last_shot_at_s: 0.0,
                        shoot_anim_until: 0.0,
                        shoot_dir: b.dir,
                    },
                ));
                let _ = self.ecs.insert_one(e, Mounted { boat_entity, boat_eid });
                tracing::info!("login: boat restaurado pid={:?} kind={} pos={:?}", pid, b.kind, b.pos);
            } else {
                tracing::warn!("login: boat pos ({bx},{by}) nao-water — restauracao abortada");
            }
        }

        if let Some(s) = self.sessions.get_mut(&sid) {
            s.entity = Some(e);
            s.logged_in = true;
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
            s.woodcutting_lvl = row.woodcutting_lvl.max(1);
            s.mining_lvl      = row.mining_lvl.max(1);
            s.gathering_lvl   = row.gathering_lvl.max(1);
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
            map_name: "overworld".to_string(),
            width: self.map.width,
            height: self.map.height,
            tiles: self.map.tiles.clone(),
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
                harpoon_target_eid: None,
                harpoon_until: 0.0,
                bloodthirst_until: 0.0,
                hunters_mark_until: 0.0,
                hunters_mark_charges: 0,
                hunters_mark_rank: 0,
                poise_current: 50.0, // base padrao; refresh via stats no login
                last_combat_at_s: 0.0,
                poise_last_sent: 0,
                woodcutting_lvl: 1,
                mining_lvl:      1,
                gathering_lvl:   1,
            },
        );
    }

    pub fn on_disconnect(&mut self, id: SessionId) {
        if let Some(s) = self.sessions.remove(&id) {
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
                    s.pending_input = Some(frame);
                }
            }
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
            ClientMessage::DismountBoat => {
                self.handle_dismount_boat(id);
            }
            ClientMessage::ResetPosition => {
                self.handle_reset_position(id);
            }
            ClientMessage::RequestDisconnect => self.on_disconnect(id),
            ClientMessage::SkillLearn { skill_id }   => self.handle_skill_learn(id, skill_id),
            ClientMessage::SkillRankUp { skill_id }  => self.handle_skill_rank_up(id, skill_id),
            ClientMessage::SkillEquip { skill_id, slot } => self.handle_skill_equip(id, skill_id, slot),
            ClientMessage::SkillCast { skill_id, target_pos } => self.handle_skill_cast(id, skill_id, target_pos),
            ClientMessage::CreateCharacter { name, visual, starting_weapon } => {
                self.handle_create_character(id, name, visual, starting_weapon);
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
        }
    }

    /// Player downed escolheu respawnar direto na cidade — pula o timer de
    /// stand-up. HP restaurado pra max, posicao = spawn_tile, downed limpa.
    fn handle_respawn_at_city(&mut self, sid: SessionId) {
        let (entity, hp_max, name) = {
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
            (entity, hp_max, session.name.clone())
        };
        let spawn = {
            let t = self.map.spawn_tile();
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
        // CRITICO: sincroniza physics rigid body (sem isso o player retorna
        // pra posicao do collider no proximo step). Mesma logica do
        // handle_reset_position.
        if let Ok(h) = self.ecs.get::<&shared::PhysicsHandle>(entity).map(|h| h.0) {
            if let Some(rb) = self.physics.rigid_body_set.get_mut(h) {
                rb.set_translation([spawn.x, spawn.y].into(), true);
                rb.set_linvel([0.0, 0.0].into(), true);
            }
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
        // Whitelist de armas iniciais — item_ids reais do shared::constants::items.
        // 3=Sword, 6=Staff, 12=Dagger, 13=GreatSword, 14=Bow, 15=Wand, 25=Axe, 26=Spear
        const ALLOWED_WEAPONS: &[u16] = &[3, 6, 12, 13, 14, 15, 25, 26];
        if !ALLOWED_WEAPONS.contains(&starting_weapon) {
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
        // Spawn = default tile do mapa.
        let spawn = {
            let t = self.map.spawn_tile();
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
                &auth_ctx.pool, account_id, &name_for_db, &visual_for_db, starting_weapon, spawn
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

        // 4. Weapon usable_with
        let weapon_id = session.equipment.weapon.unwrap_or(0);
        let weapon_prof = shared::Proficiency::from_item(weapon_id).as_db_str();
        if let Some(uw) = &def.usable_with {
            if !uw.is_empty() && !uw.iter().any(|p| p == weapon_prof) {
                tracing::debug!("skill_cast: weapon prof {} não permitido pra skill {}", weapon_prof, skill_id);
                return;
            }
        }

        // 5. Cost (MP / stamina). Aplica per_rank_cost_pct: cost final = base × (1 - per_rank × (rank-1)).
        let mut rank_factor = 1.0 - def.per_rank_cost_pct * (rank.saturating_sub(1) as f32);
        // Mana Conduit (1050) — Staff T1 P: -1%/rank mp/stam cost. Aplicado
        // multiplicativamente. Cap em -50% (rank 10 = -10% sozinho; com
        // per_rank_cost_pct da skill some adicional).
        if let Some(mc) = session.learned_skills.iter().find(|s| s.skill_id == 1050) {
            let mc_factor = (1.0 - 0.01 * mc.rank as f32).max(0.5);
            rank_factor *= mc_factor;
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
        // depois aplica (1 + per_rank_dmg × rank).
        let stats = session.stats;
        let base_dmg = def.base_damage as f32
            + stats.attack_damage as f32 * def.scaling_atk
            + stats.wis as f32 * def.scaling_wis
            + stats.dex as f32 * def.scaling_dex;
        let scaled = base_dmg * (1.0 + def.per_rank_dmg_pct * (rank.saturating_sub(1) as f32));
        let damage = scaled.round() as i32;

        // 8. Drena cost + set cd + casting state se cast_time > 0.
        session.mp_current -= mp_cost as f32;
        if session.mp_current < 0.0 { session.mp_current = 0.0; }
        session.stamina_current = (session.stamina_current - st_cost).max(0.0);
        let cd_factor = (1.0 - def.per_rank_cd_pct * (rank.saturating_sub(1) as f32)).max(0.1);
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
                        is_crit: false,
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
                    let heal = (base_heal * (1.0 + def.per_rank_dmg_pct * (rank.saturating_sub(1) as f32))).round() as i32;
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
                        hurt_dir: hd, is_crit: false, from_player: true,
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
                        is_crit: false,
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
                    let heal = (base_heal * (1.0 + def.per_rank_dmg_pct * (rank.saturating_sub(1) as f32))).round() as i32;
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
                            hurt_dir: hd, is_crit: false, from_player: true,
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
                        hurt_dir: hd, is_crit: false, from_player: true,
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
        for (_, (net, pos2, kind)) in self.ecs.query::<(&NetId, &Position, &EntityKind)>().iter() {
            if !matches!(kind, EntityKind::Enemy(_)) { continue; }
            if net.0 == attacker_net { continue; }
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
        for (_, (net, pos, kind)) in self.ecs.query::<(&NetId, &Position, &EntityKind)>().iter() {
            if !matches!(kind, EntityKind::Enemy(_)) { continue; }
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

    fn find_enemies_in_cone(&self, pos: Vec2, dir: Vec2, range: f32, half_angle: f32) -> Vec<EntityId> {
        let cos_half = half_angle.cos();
        let r2 = range * range;
        let mut out = Vec::new();
        for (_, (net, pos2, kind)) in self.ecs.query::<(&NetId, &Position, &EntityKind)>().iter() {
            if !matches!(kind, EntityKind::Enemy(_)) { continue; }
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
            for (_, (net, pos2, kind)) in self.ecs.query::<(&NetId, &Position, &EntityKind)>().iter() {
                if !matches!(kind, EntityKind::Enemy(_)) { continue; }
                if excluded.contains(&net.0) { continue; }
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
        session.skill_points_spent = session.skill_points_spent.saturating_add(cost);
        session.learned_skills[idx].rank = cur_rank + 1;
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
        for session in self.sessions.values_mut() {
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
            // Regen de poise: SO regenera fora de combate (>=5s sem hit).
            // Recovery rate: 10 / s (poise volta cheio em 5s).
            const POISE_REGEN_PER_SEC: f32 = 10.0;
            const COMBAT_TIMEOUT_S: f32 = 5.0;
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
            // Player montado em barco: input vai pro barco (capturado num
            // hashmap separado e processado apos o physics step). Skip TODO o
            // resto do input loop — sem ataques, sem skills, sem dash.
            // Captura aim+buttons tambem pra dar suporte a cannon shot.
            if self.ecs.get::<&Mounted>(entity).is_ok() {
                let mv = if frame.move_dir.length_squared() > 1.0 {
                    frame.move_dir.normalize()
                } else { frame.move_dir };
                mounted_inputs.insert(entity, MountedInput {
                    move_dir: mv,
                    aim:      frame.aim,
                    buttons:  frame.buttons,
                });
                // Zera velocity do player pra Rapier nao tentar puxar ele
                // pelo movimento residual quando o barco mover.
                if let Ok(mut v) = self.ecs.get::<&mut Velocity>(entity) {
                    v.0 = Vec2::ZERO;
                }
                continue;
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
            session.prev_buttons = frame.buttons;
            // RMB held = defesa ativa. Drena stamina apenas no hit. Disponivel
            // pra TODAS as armas — quem tem escudo no offhand bloqueia "fisico"
            // (visual ShieldBash); o resto bloqueia com "escudo de energia"
            // (visual Push + bolha) replicado pelo cliente. Stat bonuses do
            // shield (defense+8, hp+75) sao o incentivo pra equipar.
            let staggered = self.sim_time_s < session.stagger_until;
            let block_cost_now = shared::combat::block_stamina_cost(&session.stats);
            session.defending = (frame.buttons & buttons::SECONDARY != 0)
                && !session.downed
                && !staggered
                && session.stamina_current >= block_cost_now;

            // Stagger/Downed: ignora movimento e ataques. Player downed fica
            // travado na pose sentada — não pode andar até levantar.
            let in_hurt = session.hurt_until > self.sim_time_s;
            let dir = if in_hurt || session.downed || staggered {
                Vec2::ZERO
            } else if frame.move_dir.length_squared() > 1.0 {
                frame.move_dir.normalize()
            } else {
                frame.move_dir
            };
            // Quick Draw passive: rastreia ultimo movimento. Se mover (dir != 0),
            // atualiza last_movement_at_s e reseta quickdraw_consumed pra liberar
            // o crit da proxima "1ª flecha".
            if dir.length_squared() > 0.001 {
                session.last_movement_at_s = self.sim_time_s;
                session.quickdraw_consumed = false;
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
            let wants_attack = if session.downed || session.carrying.is_some() || in_hurt
                || was_dashing || session.defending || staggered {
                false
            } else {
                let has_stam = session.stamina_current >= shared::ATTACK_STAMINA_COST;
                let w = (frame.buttons & buttons::PRIMARY != 0)
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
                aim: frame.aim,
                damage: dmg_final,
                is_crit: crit,
                is_melee,
                proj_kind,
                attack_anim_code,
                combo_step,
                dash_started: wants_dash,
            });
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

        // ── C: IA dos inimigos ────────────────────────────────────────────────
        struct SpawnProj { owner_id: EntityId, from_player: bool, pos: Vec2, dir: Vec2, damage: i32, is_crit: bool, kind: u8 }
        let mut projs_to_spawn: Vec<SpawnProj> = Vec::new();
        // Melee swings — usado por player attacks (sec D) e por enemies melee aqui (sec C).
        struct MeleeSwing { attacker_eid: EntityId, pos: Vec2, dir: Vec2, damage: i32, is_crit: bool, from_player: bool, knockback: f32 }
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
            if enemy.hurt_until > now_sim {
                vel.0 = Vec2::ZERO;
                continue;
            }
            // Stun (Shield Bash 1003): enemy parado, sem ataque/movimento.
            if enemy.stunned_until > now_sim {
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
            // tauntador (forced_aggro_target). Senao escolhe o nearest player.
            let nearest = if enemy.forced_aggro_until > now_sim {
                if let Some(target_eid) = enemy.forced_aggro_target {
                    player_positions.iter()
                        .find(|(eid, _)| *eid == target_eid)
                        .or_else(|| player_positions.iter().min_by(|a, b| {
                            a.1.distance_squared(pos.0).partial_cmp(&b.1.distance_squared(pos.0)).unwrap()
                        }))
                } else {
                    player_positions.iter().min_by(|a, b| {
                        a.1.distance_squared(pos.0).partial_cmp(&b.1.distance_squared(pos.0)).unwrap()
                    })
                }
            } else {
                player_positions.iter().min_by(|a, b| {
                    a.1.distance_squared(pos.0).partial_cmp(&b.1.distance_squared(pos.0)).unwrap()
                })
            };

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

            if let Some((_, ppos)) = nearest {
                let dist = pos.0.distance(*ppos);
                // Line of sight: enemy nao "ve" player atraves de WALL.
                // Player atras de parede = no aggro/chase. Player atras de
                // agua/decoracao continua visivel (so WALL bloqueia).
                let has_los = self.map.has_line_of_sight(pos.0, *ppos);
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
                        if dist > stand + 0.3      { to_player }   // longe → aproxima
                        else if dist < stand - 0.3 { -to_player }  // muito perto → afasta
                        else                       { Vec2::ZERO }  // em range, parado
                    };
                    vel.0 = move_dir * enemy.locomotor_speed;

                    let attack_range = enemy.attack_range;
                    if dist < attack_range && enemy.attack_cooldown <= 0.0 {
                        enemy.aggro_timer = 0.0; // reset ao atacar com sucesso
                        enemy.attack_cooldown = enemy.attack_cooldown_base;
                        enemy.attack_pending = true; // cliente toca anim

                        // Tenta casting de skill ANTES de fall-through pra
                        // auto-attack. Skill eligibility check (sem borrow
                        // self): cd ready, mp ok, weapon usable_with bate,
                        // alvo dentro de range. Se passou, push intent
                        // (processada fora do loop) e SKIPA auto-attack.
                        let mut chose_skill = false;
                        let weapon_id = enemy.equipment.weapon.unwrap_or(0);
                        let prof = shared::Proficiency::from_item(weapon_id).as_db_str();
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
                            if let Some(uw) = &def_sk.usable_with {
                                if !uw.is_empty() && !uw.iter().any(|p| p == prof) { continue; }
                            }
                            if dist > def_sk.range_tiles + def_sk.radius_tiles { continue; }
                            // Skill apta — registra intent e pula auto-attack.
                            enemy_cast_intents.push((net.0, pos.0, *ppos));
                            chose_skill = true;
                            break;
                        }
                        if chose_skill { continue; }
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
                                knockback: 0.3, // enemy auto-attack: shove leve
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
                        &self.map, self.tick, net.0.0, pulling_home);
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
                    &self.map, self.tick, net.0.0, pulling_home);
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

        // ── C.2: IA dos NPCs caminhantes (rotas pré-definidas) ────────────────
        let now_npc = self.sim_time_s;
        const NPC_SPEED: f32 = 1.4;     // mais lento que player (4.0)
        const NPC_ARRIVE_DIST: f32 = 0.4;
        const NPC_PAUSE_MIN: f32 = 1.5;
        const NPC_PAUSE_MAX: f32 = 4.0;
        for (e, (pos, vel, wtag)) in self.ecs.query_mut::<(&Position, &mut Velocity, &mut WanderRouteTag)>() {
            // Pausa: parado até pause_until expirar
            if wtag.pause_until > now_npc {
                vel.0 = Vec2::ZERO;
                continue;
            }
            // Acha rota
            let Some(route) = self.npc_routes.get(&wtag.route_id) else {
                vel.0 = Vec2::ZERO;
                continue;
            };
            if route.waypoints.is_empty() {
                vel.0 = Vec2::ZERO;
                continue;
            }
            let wp = route.waypoints[(wtag.current_idx as usize) % route.waypoints.len()];
            let to = wp - pos.0;
            let dist = to.length();
            if dist < NPC_ARRIVE_DIST {
                // Chegou — avança e pausa.
                wtag.current_idx = (wtag.current_idx + 1) % route.waypoints.len() as u32;
                let seed = (e.id() as u64).wrapping_mul(0x9E37_79B9).wrapping_add(self.tick as u64);
                let r = lcg_f32(lcg(seed));
                wtag.pause_until = now_npc + NPC_PAUSE_MIN + r * (NPC_PAUSE_MAX - NPC_PAUSE_MIN);
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
                    });
                } else {
                    // Caster (proj_kind=1=fireball ou =5=electric) tem combo
                    // de 3 steps:
                    //   0 (Slash1) e 1 (Slash2) → swing em cone na frente,
                    //     dano melee igual a espada. Sem projetil.
                    //   2 (Thrust finisher) → spawna projetil (fireball/elec).
                    // Bow nao tem combo — sempre cai no projetil direto.
                    let is_caster_proj = matches!(ir.proj_kind, 1 | 5);
                    let last_combo_step = ir.combo_step + 1 == shared::COMBO_STEPS as u8;
                    if is_caster_proj && !last_combo_step {
                        // Wind-up: swing melee em cone (mesma infra de espada).
                        // Caster combo wind-up sao swings leves; kb minimo.
                        melee_swings.push(MeleeSwing {
                            attacker_eid: ir.owner_id, pos, dir,
                            damage: ir.damage, is_crit: ir.is_crit,
                            from_player: true,
                            knockback: 0.25,
                        });
                        continue;
                    }
                    // Ranged: queue com delay pro release coincidir com fim
                    // da animação de saque. Bow tem delay maior (anim de 560ms);
                    // wand/staff usa Thrust (320ms) → delay menor.
                    let fire_delay = if matches!(ir.proj_kind, 1 | 5) {
                        shared::MAGIC_FIRE_DELAY
                    } else {
                        shared::BOW_FIRE_DELAY
                    };
                    let release_in_ticks = (fire_delay / dt).round() as u32;
                    // Fireball/electric sai da ponta da varinha/cajado: pos
                    // peito + offset na direção do tiro. Flecha sai do peito.
                    let forward = if matches!(ir.proj_kind, 1 | 5) {
                        dir * shared::FIREBALL_FORWARD_OFFSET
                    } else {
                        Vec2::ZERO
                    };
                    let spawn_pos = pos + Vec2::new(0.0, shared::PROJ_SPAWN_OFFSET_Y) + forward;
                    // Bow finisher (combo step 2): 3 flechas em leque ±15°.
                    // Sem combo (combo_step==0 sem combo state): tiro unico.
                    let is_bow_finisher = ir.proj_kind == 0 // arrow
                        && ir.combo_step + 1 == shared::COMBO_STEPS as u8;
                    if is_bow_finisher {
                        for &spread_deg in &[-15.0_f32, 0.0, 15.0] {
                            let rad = spread_deg.to_radians();
                            let (s, c) = (rad.sin(), rad.cos());
                            let fan_dir = Vec2::new(
                                dir.x * c - dir.y * s,
                                dir.x * s + dir.y * c,
                            );
                            self.pending_shots.push(PendingShot {
                                pos: spawn_pos,
                                dir: fan_dir,
                                damage: ir.damage,
                                is_crit: ir.is_crit,
                                kind: ir.proj_kind,
                                owner_id: ir.owner_id,
                                from_player: true,
                                release_tick: self.tick.wrapping_add(release_in_ticks),
                            });
                        }
                    } else {
                        self.pending_shots.push(PendingShot {
                            pos: spawn_pos,
                            dir,
                            damage: ir.damage,
                            is_crit: ir.is_crit,
                            kind: ir.proj_kind,
                            owner_id: ir.owner_id,
                            from_player: true,
                            release_tick: self.tick.wrapping_add(release_in_ticks),
                        });
                    }
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
                    if let Ok(handle) = self.ecs.get::<&shared::PhysicsHandle>(ent) {
                        if let Some(rb) = self.physics.rigid_body_set.get_mut(handle.0) {
                            rb.set_translation([pos.x, pos.y].into(), true);
                            rb.set_linvel([0.0, 0.0].into(), true);
                        }
                    }
                }
            } else {
                // Leap completou — snap final + agenda hit.
                let landing = s.leap_target;
                if let Some(ent) = s.entity {
                    if let Ok(mut p) = self.ecs.get::<&mut Position>(ent) {
                        p.0 = landing;
                    }
                    if let Ok(handle) = self.ecs.get::<&shared::PhysicsHandle>(ent) {
                        if let Some(rb) = self.physics.rigid_body_set.get_mut(handle.0) {
                            rb.set_translation([landing.x, landing.y].into(), true);
                            rb.set_linvel([0.0, 0.0].into(), true);
                        }
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

        // ── F.1: barcos — movimento custom (sem Rapier) com tile-water-only ─
        // Cada barco com passenger usa o input do dono pra atualizar velocity.
        // Position += vel * dt; rejeita axis se a celula destino nao for water.
        // Mounted players sao sincronizados (pos = pos do barco) depois.
        // Tambem processa cannon shot quando PRIMARY pressed + cooldown OK.
        let mut boat_passenger_pairs: Vec<(Entity, Vec2)> = Vec::new();
        let mut cannon_shots: Vec<(EntityId, Vec2, Vec2)> = Vec::new(); // (owner, pos, dir)
        const CANNON_CD: f32 = 1.2_f32;
        const SHOOT_ANIM_DURATION: f32 = 0.96_f32; // 8 frames * 0.12 = 0.96s
        for (boat_e, (pos, vel, tag)) in self
            .ecs
            .query::<(&mut Position, &mut Velocity, &mut BoatTag)>()
            .iter()
        {
            // Acha intent do passageiro. Sem passageiro = barco para.
            let passenger_entity = tag.passenger_entity;
            let mounted = passenger_entity.and_then(|pe| mounted_inputs.get(&pe));
            let intent = mounted.map(|m| m.move_dir).unwrap_or(Vec2::ZERO);
            // Velocidade target. Suaviza pra dar inercia (lerp 8.0 por seg).
            let target_vel = if intent.length_squared() > 0.001 {
                intent.normalize() * tag.max_speed
            } else { Vec2::ZERO };
            let lerp_t = (8.0 * dt).min(1.0);
            vel.0 = vel.0 + (target_vel - vel.0) * lerp_t;
            // Integra com slide axis-aligned: tenta x primeiro, depois y.
            let mut new_pos = pos.0;
            // Eixo X
            let try_x = Vec2::new(pos.0.x + vel.0.x * dt, pos.0.y);
            let tx = try_x.x.floor() as i32;
            let ty = try_x.y.floor() as i32;
            if self.map.is_navigable(tx, ty) {
                new_pos.x = try_x.x;
            } else {
                vel.0.x = 0.0;
            }
            // Eixo Y (com X ja aplicado)
            let try_y = Vec2::new(new_pos.x, new_pos.y + vel.0.y * dt);
            let tx = try_y.x.floor() as i32;
            let ty = try_y.y.floor() as i32;
            if self.map.is_navigable(tx, ty) {
                new_pos.y = try_y.y;
            } else {
                vel.0.y = 0.0;
            }
            pos.0 = new_pos;

            // Cannon shot: PRIMARY + cooldown. Spawn simples — bola sai do
            // CENTRO do barco em direcao do mouse, sem trocar sheet/dir/anim.
            if let Some(m) = mounted {
                let pressed = (m.buttons & buttons::PRIMARY) != 0;
                if pressed && self.sim_time_s - tag.last_shot_at_s >= CANNON_CD {
                    let to_aim = m.aim - pos.0;
                    if to_aim.length_squared() > 0.04 {
                        let dir = to_aim.normalize();
                        tag.last_shot_at_s = self.sim_time_s;
                        if let Some(owner_eid) = tag.passenger {
                            cannon_shots.push((owner_eid, pos.0, dir));
                        }
                    }
                }
            }

            // Dir/anim baseado em velocity. Sem anim de shoot — boat continua
            // idle/movement como sempre.
            if vel.0.length_squared() > 0.05 {
                tag.dir = dir8_from_vec(vel.0);
                tag.anim = 1; // movement
            } else {
                tag.anim = 0; // idle (mantem dir antiga)
            }
            // Coleta pra sync de mounted player depois.
            if let Some(pe) = passenger_entity {
                boat_passenger_pairs.push((pe, pos.0));
            }
            let _ = boat_e;
        }
        // Spawna projeteis de cannon (kind=10) saindo do CENTRO do barco.
        for (owner, bpos, dir) in cannon_shots {
            const CANNON_DAMAGE: i32 = 30;
            const CANNON_SPEED_MULT: f32 = 1.5;
            const CANNON_TTL: f32 = 2.5;
            let proj_id = self.alloc_entity_id();
            self.ecs.spawn((
                NetId(proj_id),
                Position(bpos),
                Velocity(dir * shared::PROJ_SPEED * CANNON_SPEED_MULT),
                EntityKind::Projectile,
                ProjTag {
                    owner, from_player: true, ttl: CANNON_TTL,
                    damage: CANNON_DAMAGE, is_crit: false, kind: 10,
                },
            ));
        }
        // Sincroniza pos dos players montados com a pos do barco.
        for (pe, boat_pos) in boat_passenger_pairs {
            if let Ok(mut p) = self.ecs.get::<&mut Position>(pe) {
                p.0 = boat_pos;
            }
            if let Ok(handle) = self.ecs.get::<&shared::PhysicsHandle>(pe).map(|h| h.0) {
                if let Some(rb) = self.physics.rigid_body_set.get_mut(handle) {
                    rb.set_translation([boat_pos.x, boat_pos.y].into(), true);
                    rb.set_linvel([0.0, 0.0].into(), true);
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
                if let Ok(handle) = self.ecs.get::<&shared::PhysicsHandle>(te).map(|h| h.0) {
                    if let Some(rb) = self.physics.rigid_body_set.get_mut(handle) {
                        rb.set_translation([cp.x, cp.y].into(), true);
                        rb.set_linvel([0.0, 0.0].into(), true);
                    }
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
                    if sw.from_player == *is_player { continue; }
                    if *tnet == sw.attacker_eid { continue; }
                    // Escala hitbox pelo tamanho do alvo (boss=2.2× → hitbox 2.2×).
                    let r       = hit_target_radius * size;
                    let y_off   = hit_target_y_off  * size;
                    let melee_max = shared::MELEE_RANGE + r;
                    let range_sq = melee_max * melee_max;
                    // Hit-point do alvo: peito (Y+offset), não os pés.
                    let target_hit = *tpos + Vec2::new(0.0, y_off);
                    let delta = target_hit - sw.pos;
                    let d2 = delta.length_squared();
                    if d2 > range_sq { continue; }
                    if let Some(nd) = delta.try_normalize() {
                        if sw.dir.dot(nd) < cos_half { continue; }
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
                if *pfrom_player == *is_player { continue; }
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
                let mut found: Option<Entity> = None;
                for (e, net) in self.ecs.query::<&NetId>().iter() {
                    if net.0 == h.target_net { found = Some(e); break; }
                }
                if let Some(e) = found {
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
            // Defense flat primeiro, depois redução percentual (breakpoints).
            let after_def = (dmg - target_defense).max(1);
            let mut dmg = ((after_def as f32) * (1.0 - target_dmg_reduction_pct)).round() as i32;
            dmg = dmg.max(1);

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
            let mut absorbed_by_poise = false;
            let now_s_poise = self.sim_time_s;
            if let Some(target) = self.sessions.values_mut().find(|s| s.entity_id == target_id) {
                if target.poise_current > 0.0 && dmg > 0 {
                    let def_resist = (target.stats.defense as f32 * 0.04).min(0.8);
                    let defending_bonus = if target.defending { 0.4 } else { 0.0 };
                    let total_resist = (def_resist + defending_bonus).min(0.95);
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
            if let Ok(mut hp) = self.ecs.get::<&mut Health>(entity) {
                hp.current = (hp.current - dmg).max(0);
                if hp.current == 0 && attacker_is_player {
                    kill_credits.insert(target_id, attacker_id);
                }
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
                if kb_active {
                    tag.knockback_until = kb_until_ts;
                    tag.knockback_vel   = kb_vel;
                }
            } else if let Some(s) = self.sessions.values_mut()
                .find(|s| s.entity_id == target_id)
            {
                s.hurt_until = hurt_until_ts;
                s.hurt_dir   = hurt_dir;
                if kb_active {
                    s.knockback_until = kb_until_ts;
                    s.knockback_vel   = kb_vel;
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
                        session.xp = session.xp.saturating_add(share);
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
                        let new_level = shared::level_of_xp_with_mult(session.xp, crate::economy::xp_multiplier());
                        if new_level > session.last_level {
                            let gained = new_level - session.last_level;
                            session.unspent_points = session.unspent_points
                                .saturating_add(gained * shared::POINTS_PER_LEVEL);
                            session.stat_points_dirty = true;
                            // Skills: ganha SP por level (cap em CHAR_LEVEL_CAP).
                            // `level_of_xp` já clampa, mas o gain por delta
                            // preserva idempotência se chamado mais de uma vez.
                            session.skill_points_earned = session.skill_points_earned
                                .saturating_add(gained * shared::SP_PER_LEVEL);
                            session.skills_dirty = true;
                            tracing::info!(
                                "{} subiu pra L{} (+{} pontos livres, +{} SP)",
                                session.name, new_level,
                                gained * shared::POINTS_PER_LEVEL,
                                gained * shared::SP_PER_LEVEL,
                            );
                        }
                        session.last_level = new_level;
                        let _ = session
                            .handle
                            .to_client
                            .send(ServerMessage::ProgressUpdate {
                                xp: session.xp,
                                level: new_level,
                            });
                        tracing::info!(
                            "[XP] {} ganhou xp -> total={} L{} (xp_for_next L{}={})",
                            session.name, session.xp, new_level,
                            new_level + 1, shared::xp_for_level_with_mult(new_level + 1, crate::economy::xp_multiplier())
                        );
                        tracing::debug!(
                            "kill credit: {} -> xp {} (L{})",
                            session.name, session.xp, new_level
                        );
                        break;
                    }
                }
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
                    session.stats = effective_stats(&session.equipment, &session.allocated_points, &session.proficiencies, &session.learned_skills);
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
                                session.stats = effective_stats(&session.equipment, &session.allocated_points, &session.proficiencies, &session.learned_skills);
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
            PlayerTag { name: name.clone(), player_id: pid, attack_anim_pending: None, combo_step_pending: None },
        ));
        if let Some(s) = self.sessions.get_mut(&sid) {
            s.entity = Some(e);
        }
        tracing::info!("respawn: {name}");
    }

    pub fn send_snapshots(&mut self) {
        // Coleta attack_pending dos inimigos e zera pra mandar 1 vez só.
        let attacking_ids: std::collections::HashSet<EntityId> = self
            .ecs
            .query::<(&NetId, &mut EnemyTag)>()
            .iter()
            .filter_map(|(_, (net, t))| {
                if t.attack_pending {
                    t.attack_pending = false;
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
        let now_for_cast = self.sim_time_s;
        for s in self.sessions.values_mut() {
            if !s.logged_in { continue; }
            if s.parry_flash_pending {
                s.parry_flash_pending = false;
                parry_flash_ids.insert(s.entity_id);
            }
            let casting = s.casting_until > now_for_cast;
            let poise_active = s.poise_current > 0.5;
            let mut buffs_mask: u8 = 0;
            if s.bloodthirst_until  > now_for_cast { buffs_mask |= shared::components::buffs_mask::BLOODTHIRST;  }
            if s.hunters_mark_until > now_for_cast { buffs_mask |= shared::components::buffs_mask::HUNTERS_MARK; }
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

        // Conjunto de player_eids montados (pra `mounted: true` no snapshot).
        let mounted_player_eids: std::collections::HashSet<EntityId> = self.ecs
            .query::<(&NetId, &Mounted)>()
            .iter()
            .map(|(_, (net, _))| net.0)
            .collect();

        let all: Vec<EntitySnapshot> = self
            .ecs
            .query::<(&NetId, &Position, &Velocity, &EntityKind, Option<&Health>, Option<&PlayerTag>, Option<&ProjTag>, Option<&NpcSkin>, Option<&VendorTag>, Option<&WanderRouteTag>, Option<&BoatTag>, Option<&EnemyTag>, Option<&LootTag>)>()
            .iter()
            .map(|(_, (net, pos, vel, kind, hp, ptag, projtag, skin, vtag, wtag, boat, etag, ltag))| {
                let is_player = matches!(kind, EntityKind::Player);
                let overlay = if is_player { player_overlay.get(&net.0) } else { None };
                EntitySnapshot {
                    id: net.0,
                    kind: match kind {
                        EntityKind::Player      => "Player".to_string(),
                        EntityKind::Enemy(_)    => "Enemy".to_string(),
                        EntityKind::Projectile  => "Projectile".to_string(),
                        EntityKind::Loot(_)     => "Loot".to_string(),
                        EntityKind::Npc(_)      => "Npc".to_string(),
                        EntityKind::Portal      => "Portal".to_string(),
                        EntityKind::Boat(_)     => "Boat".to_string(),
                    },
                    pos: pos.0,
                    vel: vel.0,
                    hp: hp.map(|h| h.current),
                    hp_max: hp.map(|h| h.max),
                    name: ptag.map(|p| p.name.clone())
                        .or_else(|| vtag.map(|v| v.name.clone()))
                        .or_else(|| wtag.map(|w| w.name.clone()))
                        .or_else(|| etag.map(|t| {
                            let tier = crate::enemy_builds::tier_for_level(t.level);
                            if t.is_boss {
                                format!("[BOSS] {} Lv{}", tier.theme.display_name, t.level)
                            } else {
                                format!("{} Lv{}", tier.theme.display_name, t.level)
                            }
                        })),
                    sprite_id: match kind {
                        EntityKind::Enemy(n) | EntityKind::Loot(n) | EntityKind::Npc(n) => Some(*n as u32),
                        EntityKind::Boat(n)    => Some(*n as u32),
                        EntityKind::Projectile => projtag.map(|p| p.kind as u32),
                        _ => None,
                    },
                    is_self: None,
                    attacking: if attacking_ids.contains(&net.0) { Some(true) } else { None },
                    attack_anim: if parry_flash_ids.contains(&net.0) {
                        Some(shared::components::attack_anim::PARRY_FLASH)
                    } else {
                        player_attack_anim.get(&net.0).copied()
                    },
                    combo_step: player_combo_step.get(&net.0).copied(),
                    weapon_id: overlay.and_then(|o| o.weapon_id)
                        .or_else(|| etag.and_then(|t| t.equipment.weapon)),
                    offhand_id: overlay.and_then(|o| o.offhand_id)
                        .or_else(|| etag.and_then(|t| t.equipment.offhand)),
                    downed: overlay.map(|o| o.downed),
                    visual: overlay.map(|o| o.visual.clone())
                        .or_else(|| etag.map(|t| t.visual.clone())),
                    attack_speed_mult: overlay.map(|o| o.attack_speed_mult),
                    hurt_dir: self.hit_this_tick.get(&net.0).map(|v| [v.x, v.y]),
                    is_crit: self.crit_this_tick.get(&net.0).copied(),
                    last_damage: self.damage_this_tick.get(&net.0).copied(),
                    attacker_weapon_id: self.attacker_weapon_this_tick.get(&net.0).copied(),
                    owner_eid: projtag.map(|p| p.owner),
                    skin_preset: skin.map(|s| s.preset),
                    defending: overlay.and_then(|o| if o.defending { Some(true) } else { None }),
                    casting: overlay.and_then(|o| if o.casting { Some(true) } else { None }),
                    poisoned: if poisoned_ids.contains(&net.0) { Some(true) } else { None },
                    stunned: if stunned_ids.contains(&net.0) { Some(true) } else { None },
                    leap_y: leap_y_map.get(&net.0).copied(),
                    poise_active: overlay.and_then(|o| if o.poise_active { Some(true) } else { None }),
                    boat_dir: boat.map(|b| b.dir),
                    boat_anim: boat.map(|b| b.anim),
                    boat_shoot_dir: boat.and_then(|b| if b.anim == 2 { Some(b.shoot_dir) } else { None }),
                    passenger_eid: boat.and_then(|b| b.passenger),
                    mounted: if is_player && mounted_player_eids.contains(&net.0) { Some(true) } else { None },
                    is_boss: etag.and_then(|t| if t.is_boss { Some(true) } else { None }),
                    buffs: overlay.and_then(|o| if o.buffs_mask != 0 { Some(o.buffs_mask) } else { None }),
                    // Tier do loot — usado pra colorir halo no chao (estilo MIR4):
                    // resources (60-71) tier baked no item_id. Equipaveis com
                    // instance.item_level → 1-4 via threshold (10/30/60).
                    loot_tier: ltag.and_then(|l| compute_loot_tier(l)),
                }
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
            let my_entity_id = session.entity_id;
            let mut visible: Vec<EntitySnapshot> = Vec::new();
            for dy in -1..=1 {
                for dx in -1..=1 {
                    if let Some(idxs) = grid.get(&(ccx + dx, ccy + dy)) {
                        for &i in idxs {
                            let s = &all[i];
                            if s.pos.distance_squared(center) <= radius_sq {
                                let mut snap = s.clone();
                                if snap.id == my_entity_id {
                                    snap.is_self = Some(true);
                                }
                                visible.push(snap);
                            }
                        }
                    }
                }
            }
            let _ = session.handle.to_client.send(ServerMessage::Snapshot { snapshot: WorldSnapshot {
                tick: self.tick,
                server_time_ms: now_ms(),
                last_input_seq: session.last_input_seq,
                entities: visible,
                removed: removed.clone(),
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
        self.removed_this_tick.clear();
    }
}

impl GameWorld {
    /// Monta linhas de persistencia com o estado atual de TODOS os jogadores
    /// logados. Tambem atualiza o cache em memoria pra que o proximo login
    /// (antes do DB terminar de gravar) ja veja dados novos.
    pub fn collect_character_rows(&mut self) -> Vec<crate::persistence::CharacterRow> {
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
        }
        let mut entries: Vec<E> = Vec::new();
        for session in self.sessions.values() {
            if !session.logged_in { continue; }
            let Some(e) = session.entity else { continue };
            // Se montado, captura pos do barco + boat state.
            let (pos, boat) = if let Ok(m) = self.ecs.get::<&Mounted>(e).map(|m| m.boat_entity) {
                let bp = self.ecs.get::<&Position>(m).ok().map(|p| p.0).unwrap_or_default();
                let (kind, dir) = self.ecs.get::<&BoatTag>(m)
                    .map(|t| (t.kind, t.dir))
                    .unwrap_or((0, 4));
                (bp, Some(crate::persistence::PersistedBoat { kind, pos: bp, dir }))
            } else {
                let p = match self.ecs.get::<&Position>(e) { Ok(p) => p.0, Err(_) => continue };
                (p, None)
            };
            let hp = match self.ecs.get::<&Health>(e) { Ok(h) => *h, Err(_) => continue };
            entries.push(E {
                name: session.name.clone(),
                pos, hp, boat,
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
            });
        }
        for e in entries {
            let row = crate::persistence::CharacterRow {
                name: e.name.clone(),
                pos: e.pos,
                hp: e.hp,
                xp: e.xp,
                gold: e.gold,
                boat: e.boat,
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
                session.stats = effective_stats(&session.equipment, &session.allocated_points, &session.proficiencies, &session.learned_skills);
                session.stats_dirty = true;
                session.inventory_dirty = true;
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
                session.stats = effective_stats(&session.equipment, &session.allocated_points, &session.proficiencies, &session.learned_skills);
                session.stats_dirty = true;
                session.inventory_dirty = true;
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
        if let Ok(h) = self.ecs.get::<&shared::PhysicsHandle>(player_entity).map(|h| h.0) {
            if let Some(rb) = self.physics.rigid_body_set.get_mut(h) {
                rb.set_translation([dest.x, dest.y].into(), true);
                rb.set_linvel([0.0, 0.0].into(), true);
            }
        }
        self.save_pending = true;
        tracing::info!("{} reset position → spawn {:?}", name, dest);
    }

    fn handle_interact(&mut self, sid: SessionId) {
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

        // 2) Procura NPC. Captura (entity, npc_kind, vendor_eid, dist²).
        let mut best: Option<(hecs::Entity, u16, u32, f32)> = None;
        for (e, (net, p, k)) in self.ecs.query::<(&NetId, &Position, &EntityKind)>().iter() {
            if let EntityKind::Npc(n) = k {
                let d2 = p.0.distance_squared(player_pos);
                if d2 <= r_sq {
                    if best.map(|(_, _, _, bd)| d2 < bd).unwrap_or(true) {
                        best = Some((e, *n, net.0.0 as u32, d2));
                    }
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
                let _ = handle.to_client.send(ServerMessage::BlacksmithOpen);
            }
            _ => {} // outros tipos de NPC (3=wander) sem interação por ora
        }
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
                    session.stats = effective_stats(&session.equipment, &session.allocated_points, &session.proficiencies, &session.learned_skills);
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
                session.stats = effective_stats(&session.equipment, &session.allocated_points, &session.proficiencies, &session.learned_skills);
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
            UseAction::SpawnBoat { kind, item_id: _ } => {
                let player_pos = match self.ecs.get::<&Position>(player_entity) {
                    Ok(p) => p.0,
                    Err(_) => return,
                };
                // Player ja montado? Nao deixa "dobrar".
                if self.ecs.get::<&Mounted>(player_entity).is_ok() {
                    self.send_chat_to(sid, "[Sistema] Ja em uma embarcacao.");
                    return;
                }
                // Procura tile de agua mais proximo do player (BFS Chebyshev,
                // raio max 3). Se achar, spawna o barco la. Se nao, falha —
                // player muito longe da agua.
                let px = player_pos.x.floor() as i32;
                let py = player_pos.y.floor() as i32;
                let mut water_pos: Option<Vec2> = None;
                'search: for r in 1i32..=3 {
                    for dy in -r..=r {
                        for dx in -r..=r {
                            // So perimetro do anel r (skip interior ja varrido).
                            if dx.abs() != r && dy.abs() != r { continue; }
                            let tx = px + dx;
                            let ty = py + dy;
                            if self.map.is_water(tx, ty) {
                                water_pos = Some(Vec2::new(tx as f32 + 0.5, ty as f32 + 0.5));
                                break 'search;
                            }
                        }
                    }
                }
                let Some(water_pos) = water_pos else {
                    tracing::info!(
                        "boat use: sem agua adjacente. player_pos=({:.2},{:.2}) tile=({},{}) tile_id={}",
                        player_pos.x, player_pos.y, px, py, self.map.get(px, py)
                    );
                    self.send_chat_to(sid, "[Sistema] Aproxime-se mais da agua.");
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
                // Direcao inicial: aponta pro lado oposto da margem (away from
                // player). Calcula delta water - player e snap pra 8-dir.
                let init_dir = dir8_from_vec(water_pos - player_pos);
                let boat_entity = self.ecs.spawn((
                    NetId(boat_eid),
                    Position(water_pos),
                    Velocity(Vec2::ZERO),
                    EntityKind::Boat(kind),
                    BoatTag {
                        kind,
                        owner_pid,
                        passenger: Some(player_eid),
                        passenger_entity: Some(player_entity),
                        dir: init_dir,
                        anim: 0, // idle
                        max_speed: 4.5,
                        last_shot_at_s: 0.0,
                        shoot_anim_until: 0.0,
                        shoot_dir: 4, // S inicial
                    },
                ));
                // Marca o player como montado.
                let _ = self.ecs.insert_one(player_entity, Mounted {
                    boat_entity,
                    boat_eid,
                });
                // Item NAO eh consumido — barco e' permanente no inv. Spawn so'
                // gera entidade temporaria; dismount despawna sem devolver nada.
                // Mount eh evento critico — forca save IMEDIATO (sub-1s).
                self.save_pending = true;
                tracing::info!("boat spawn: pid={:?} kind={} pos={:?}", owner_pid, kind, water_pos);
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
            _ => return,
        };

        // Gate: distância máxima.
        if player_pos.distance(node.pos) > shared::FARM_MAX_RANGE { return }

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

            // XP de farm skill e incremento de nível (a cada 10 coletas = +1 lvl, cap 100).
            // TODO: persistir XP de farm separadamente; por ora incrementa lvl direto.
            if let Some(s) = self.sessions.get_mut(&sid) {
                let lvl_ref = match node_kind.as_str() {
                    "Tree"   => &mut s.woodcutting_lvl,
                    "Rock"   => &mut s.mining_lvl,
                    _        => &mut s.gathering_lvl,
                };
                if *lvl_ref < 100 {
                    *lvl_ref += 1; // 1 coleta = 1 lvl (temporário — substituir por XP)
                    let (wc, mn, gt) = (s.woodcutting_lvl, s.mining_lvl, s.gathering_lvl);
                    let _ = s.handle.to_client.send(ServerMessage::FarmSkillsUpdate {
                        woodcutting: wc, mining: mn, gathering: gt,
                    });
                }

                // Proficiência Farm: +30 XP por coleta. Aparece no painel C (StatsPanel).
                let farm_idx = shared::Proficiency::Farm as usize;
                s.proficiencies[farm_idx] = s.proficiencies[farm_idx].saturating_add(30);
                s.proficiencies_dirty = true;
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
    fn handle_dismount_boat(&mut self, sid: SessionId) {
        // Acha entity + Mounted do player.
        let (player_entity, mounted_boat_entity, mounted_boat_eid, boat_kind) = {
            let Some(session) = self.sessions.get(&sid) else { return };
            if !session.logged_in { return; }
            let Some(pe) = session.entity else { return };
            let Ok(m) = self.ecs.get::<&Mounted>(pe) else { return };
            // Pega kind do barco antes de despawn pra devolver o item certo.
            let kind = self.ecs.get::<&EntityKind>(m.boat_entity)
                .ok()
                .and_then(|k| if let EntityKind::Boat(n) = *k { Some(n) } else { None })
                .unwrap_or(0);
            (pe, m.boat_entity, m.boat_eid, kind)
        };
        // Pos do barco.
        let boat_pos = match self.ecs.get::<&Position>(mounted_boat_entity) {
            Ok(p) => p.0,
            Err(_) => return,
        };
        let bx = boat_pos.x.floor() as i32;
        let by = boat_pos.y.floor() as i32;
        // BFS ate 3 tiles procurando walkable. Cardinais primeiro pra evitar
        // saidas diagonais "estranhas" entre rochas.
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
            if depth >= 3 { continue; }
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
        if let Ok(h) = self.ecs.get::<&shared::PhysicsHandle>(player_entity).map(|h| h.0) {
            if let Some(rb) = self.physics.rigid_body_set.get_mut(h) {
                rb.set_translation([target.x, target.y].into(), true);
                rb.set_linvel([0.0, 0.0].into(), true);
            }
        }
        // Remove Mounted, despawna barco.
        let _ = self.ecs.remove_one::<Mounted>(player_entity);
        let _ = self.ecs.despawn(mounted_boat_entity);
        self.removed_this_tick.push(mounted_boat_eid);
        // Dismount eh evento critico — forca save IMEDIATO (sub-1s).
        self.save_pending = true;
        // Item de barco nunca foi consumido — fica no inv permanentemente.
        // Player pode usar de novo pra spawnar outra entidade temporaria.
        let _ = boat_kind; // suprime unused warning
        tracing::info!("boat dismount: target={:?}", target);
    }

    /// Captura o estado do personagem de uma sessao especifica (para persistir
    /// no disconnect). Retorna None se nao estiver logado ou ja morto.
    pub fn take_character_for_disconnect(&mut self, sid: &SessionId) -> Option<crate::persistence::CharacterRow> {
        let session = self.sessions.get(sid)?;
        if !session.logged_in { return None; }
        let e = session.entity?;
        let hp = *self.ecs.get::<&Health>(e).ok()?;
        // Se montado, captura o boat state (pos, kind, dir) e usa pos do
        // BARCO como pos do char (no login restauramos no mesmo lugar).
        let (pos, boat) = if let Ok(m) = self.ecs.get::<&Mounted>(e).map(|m| (m.boat_entity, m.boat_eid)) {
            let (boat_e, _boat_eid) = m;
            let bp = self.ecs.get::<&Position>(boat_e).ok().map(|p| p.0).unwrap_or_default();
            let (kind, dir) = self.ecs.get::<&BoatTag>(boat_e)
                .map(|t| (t.kind, t.dir))
                .unwrap_or((0, 4));
            (bp, Some(crate::persistence::PersistedBoat { kind, pos: bp, dir }))
        } else {
            let p = self.ecs.get::<&Position>(e).ok()?.0;
            (p, None)
        };
        let row = crate::persistence::CharacterRow {
            name: session.name.clone(),
            pos,
            hp,
            xp: session.xp,
            gold: session.gold,
            boat,
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

/// Calcula stats efetivos = base + pontos alocados + equip + scaling da
/// prof da arma equipada.
fn effective_stats(
    equip: &shared::Equipment,
    allocated: &[u32; shared::STAT_COUNT],
    proficiencies: &[u64; shared::PROF_COUNT],
    learned_skills: &[shared::LearnedSkill],
) -> shared::PlayerStats {
    let mut s = shared::base_player_stats();

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

    // Skill passives — sempre-ativas se aprendidas. Os usable_with são
    // checados antes: passiva com prof específica só vale se a arma
    // equipada bater. Passivas com usable_with=None aplicam sempre.
    for ls in learned_skills {
        if ls.rank == 0 { continue; }
        let Some(def) = crate::skills::skill_of(ls.skill_id) else { continue };
        if !def.is_passive { continue; }
        if let Some(uw) = &def.usable_with {
            if !uw.is_empty() && !uw.iter().any(|p| p == prof.as_db_str()) { continue; }
        }
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
            if (center - current).length() > 1.0 {
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
                map, pos, enemy.spawn_anchor, enemy.leash_max, seed, 30);
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
