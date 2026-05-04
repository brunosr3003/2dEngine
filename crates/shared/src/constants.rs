//! Constantes do mundo. Centralizadas aqui para garantir que cliente e
//! servidor concordem sobre a simulacao (tickrate, AOI, etc.).

/// Taxa de tick do servidor em Hz. O cliente tambem simula a este rate
/// para predicao client-side; divergir disso quebra a reconciliacao.
pub const TICK_RATE_HZ: u32 = 30;

/// Delta de tempo de um tick em segundos.
pub const TICK_DT: f32 = 1.0 / TICK_RATE_HZ as f32;

/// Raio de interesse (Area Of Interest) em tiles. Apenas entidades dentro
/// deste raio do jogador sao replicadas para cada cliente. Manter pequeno
/// reduz banda mas aumenta pop-in.
pub const AOI_RADIUS: f32 = 24.0;

/// Tamanho de uma celula do grid espacial (em tiles). Deve ser >= AOI/2
/// para que a busca de vizinhos acesse no maximo 4 celulas.
pub const SPATIAL_CELL_SIZE: f32 = 16.0;

/// Limite de jogadores por shard/mapa. Acima disso, spawn novo shard.
pub const MAX_PLAYERS_PER_SHARD: usize = 256;

/// Versao do protocolo. INCREMENTAR sempre que mensagens/layouts mudarem
/// em shared::protocol — clientes com versao errada sao rejeitados.
pub const PROTOCOL_VERSION: u16 = 42;

/// Velocidade base do jogador em tiles/segundo.
pub const PLAYER_SPEED: f32 = 5.0;

/// Multiplicador de velocidade durante sprint (Shift + stamina > 0).
pub const SPRINT_SPEED_MULT: f32 = 1.65;

// ── Dash ───────────────────────────────────────────────────────────────
/// Duracao do impulso de dash em segundos.
pub const DASH_DURATION: f32 = 0.20;
/// Velocidade durante o dash (substitui PLAYER_SPEED * speed_mult).
pub const DASH_SPEED: f32 = 14.0;
/// Cooldown entre dashes em segundos. SPD reduz via divisao por
/// `speed_mult` — clampado em `DASH_COOLDOWN_MIN`.
pub const DASH_COOLDOWN: f32 = 1.5;
/// Cooldown minimo de dash apos reducao por SPD (clamp).
pub const DASH_COOLDOWN_MIN: f32 = 0.2;
/// Custo de stamina pra dash.
pub const DASH_STAMINA_COST: i32 = 30;

/// Capacidade maxima de stamina (pontos). Fixa para todas as classes por ora.
pub const STAMINA_MAX: i32 = 100;

/// Drena stamina por segundo enquanto sprintando.
pub const STAMINA_DRAIN_PER_SEC: f32 = 40.0;

/// Regenera stamina por segundo quando NAO sprintando.
pub const STAMINA_REGEN_PER_SEC: f32 = 25.0;

/// Velocidade dos projeteis em tiles/segundo.
pub const PROJ_SPEED: f32 = 15.0;

/// Tempo de vida de um projetil em segundos.
pub const PROJ_TTL: f32 = 1.5;

/// Cooldown entre ataques em segundos.
pub const ATTACK_COOLDOWN: f32 = 0.25;

/// Tempo (s) sem atacar antes do combo melee resetar pra step 0 (Slash1).
/// Mantém em sync com client `_comboResetTime` em CharacterAnimator.
pub const COMBO_RESET_TIME: f32 = 1.0;
/// Quantidade de steps no combo melee. 0=Slash1, 1=Slash2, 2=Finisher.
pub const COMBO_STEPS: u8 = 3;

/// Cooldown estendido para Bow. Sincroniza com a anim de saque do arco
/// (`ShootStraight`: 8 frames × 70ms = 560ms).
pub const BOW_ATTACK_COOLDOWN: f32 = 0.55;
/// Delay entre input e spawn real da flecha — frame de release do saque.
pub const BOW_FIRE_DELAY: f32 = 0.40;

/// Cooldown para Wand/Staff. Anim usada é `Thrust` (4 frames × 80ms = 320ms).
pub const MAGIC_ATTACK_COOLDOWN: f32 = 0.32;
/// Delay entre input e spawn da bola de fogo — durante a extensão do thrust
/// (frame ~3 de 4, ~70% da anim). Match com o feel do bow (0.40/0.56 = 71%).
/// O gate de PRIMARY no cliente (InputHandler) impede shots fantasmas pós-
/// depleção, então não precisa empilhar o delay no fim da anim.
pub const MAGIC_FIRE_DELAY: f32 = 0.22;

/// Offset vertical (Y mundo) do spawn de projétil em relação à pos da entidade.
/// Pos fica nos pés (PaperDoll pivot 0.40); arco/cajado é segurado próximo ao
/// peito, então projétil sai ~0.5 unidade acima do pé.
pub const PROJ_SPAWN_OFFSET_Y: f32 = 0.5;

/// Offset adicional (na direção do tiro) para fireball — faz a bola sair da
/// ponta da varinha em vez do peito do char. Aplicado só pra projéteis de
/// magia; flechas continuam saindo do peito (saem do arco visualmente).
pub const FIREBALL_FORWARD_OFFSET: f32 = 0.6;

/// Stamina consumida por cada ataque primario (LMB).
pub const ATTACK_STAMINA_COST: f32 = 15.0;

/// Velocidade dos inimigos em tiles/segundo.
pub const ENEMY_SPEED: f32 = 2.0;

/// Raio em que o inimigo detecta e persegue jogadores.
pub const ENEMY_DETECT_RANGE: f32 = 9.0;

/// Raio de ataque do inimigo.
pub const ENEMY_ATTACK_RANGE: f32 = 7.0;

/// Cooldown de ataque dos inimigos.
pub const ENEMY_ATTACK_COOLDOWN: f32 = 2.0;

/// Raio de colisao de jogadores/inimigos (para hit detection).
pub const ENTITY_RADIUS: f32 = 0.35;

/// Raio de colisao de projeteis.
pub const PROJ_RADIUS: f32 = 0.15;

/// Duração do stagger ao tomar dano (s). Durante esse período a entidade
/// não pode andar nem atacar — sincroniza com a anim de Hurt no cliente.
pub const HURT_STAGGER_DURATION: f32 = 0.25;

/// Tempo (s) que o cadáver de um inimigo fica no mapa antes de despawnar.
/// Cliente exibe pose deitada + tint escuro durante esse período.
pub const ENEMY_CORPSE_LINGER: f32 = 2.0;

/// Duração do "spawn grace" — período após criar o enemy em que ele fica
/// invisível no cliente (rodando VFX de invocação) e estático no servidor
/// (sem mover, atacar ou tomar dano). Casa com a duração da spawn-VFX no
/// cliente (Magic Bursts/round_sparkle_burst_001 = 14 frames × 60ms).
pub const ENEMY_SPAWN_GRACE: f32 = 0.84;

/// Raio do "hitbox" da entidade (envolve TORSO/CABEÇA, não só os pés).
/// Usado por melee e projétil pra decidir se atinge — separado do
/// ENTITY_RADIUS (que continua pequeno pra física/collisão).
pub const HIT_TARGET_RADIUS: f32 = 0.6;
/// Offset vertical do centro do hitbox em relação ao Y da entidade (que fica
/// nos pés). 0.6 ≈ altura do tronco/peito do paper-doll Mana Seed.
pub const HIT_TARGET_Y_OFFSET: f32 = 0.6;

/// Tempo de respawn do jogador em segundos (nao usado em PvE puro — player
/// entra em Downed State; respawn so ocorre em PvP apos execucao).
pub const RESPAWN_DELAY: f32 = 3.0;

/// Tempo em segundos pra um jogador derrubado se levantar sozinho (PvE).
/// Durante esse periodo, dano de monstros NAO mata mas reseta o timer.
pub const DOWNED_HEAL_TIME: f32 = 10.0;

/// Velocidade do jogador enquanto derrubado (fracao de PLAYER_SPEED).
/// Rasteja lento, pode se esconder.
pub const DOWNED_SPEED_MULT: f32 = 0.2;

/// HP que o jogador recebe ao se levantar do Downed State (fracao do max).
pub const DOWNED_REVIVE_HP_PCT: f32 = 0.05;

/// HP maximo da barra do Downed State. So jogadores (nao monstros)
/// conseguem reduzir — quando zera, morte real com respawn.
pub const DOWNED_HP_MAX: i32 = 100;

/// Tipos de proficiencia (classless). XP se acumula ao usar arma do tipo.
///
/// Variantes adicionadas no Skills Phase 1 (M11): `Axe` e `Spear`. Antes
/// HAMMER (item 25) caía em Unarmed; agora vai pra Axe. SPEAR (item 26) idem.
/// O array de proficiencies em CharacterRow cresceu de 6 pra `PROF_COUNT`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[repr(u8)]
pub enum Proficiency {
    Sword    = 0,
    Staff    = 1,
    Dagger   = 2,
    Bow      = 3,
    Wand     = 4,
    Unarmed  = 5,
    Axe      = 6,
    Spear    = 7,
}

/// Total de proficiências (tamanho do array em CharacterRow.proficiencies).
pub const PROF_COUNT: usize = 8;

impl Proficiency {
    pub fn all() -> &'static [Proficiency] {
        &[
            Self::Sword, Self::Staff, Self::Dagger, Self::Bow,
            Self::Wand, Self::Unarmed, Self::Axe, Self::Spear,
        ]
    }

    pub fn from_item(id: u16) -> Self {
        match id {
            _ if id == item_id::SWORD || id == item_id::GREAT_SWORD => Self::Sword,
            _ if id == item_id::STAFF    => Self::Staff,
            _ if id == item_id::DAGGER   => Self::Dagger,
            _ if id == item_id::BOW      => Self::Bow,
            _ if id == item_id::WAND     => Self::Wand,
            _ if id == item_id::AXE      => Self::Axe,
            _ if id == item_id::SPEAR    => Self::Spear,
            _                            => Self::Unarmed,
        }
    }

    /// Identificador estável usado em DB e protocolo (campo `prof` da skill).
    /// Mantém em sync com [`Self::from_str`].
    pub fn as_db_str(&self) -> &'static str {
        match self {
            Self::Sword   => "Sword",
            Self::Staff   => "Staff",
            Self::Dagger  => "Dagger",
            Self::Bow     => "Bow",
            Self::Wand    => "Wand",
            Self::Unarmed => "Unarmed",
            Self::Axe     => "Axe",
            Self::Spear   => "Spear",
        }
    }

    /// Inverso de `as_db_str`. Retorna None pra strings desconhecidas.
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "Sword"   => Some(Self::Sword),
            "Staff"   => Some(Self::Staff),
            "Dagger"  => Some(Self::Dagger),
            "Bow"     => Some(Self::Bow),
            "Wand"    => Some(Self::Wand),
            "Unarmed" => Some(Self::Unarmed),
            "Axe"     => Some(Self::Axe),
            "Spear"   => Some(Self::Spear),
            _         => None,
        }
    }

    pub fn display_name(&self) -> &'static str {
        match self {
            Self::Sword   => "Espada",
            Self::Staff   => "Cajado",
            Self::Dagger  => "Adaga",
            Self::Bow     => "Arco",
            Self::Wand    => "Varinha",
            Self::Unarmed => "Desarmado",
            Self::Axe     => "Machado",
            Self::Spear   => "Lança",
        }
    }
}

/// Nivel de proficiencia dado XP acumulado (curva quadratica similar ao XP do player).
pub const fn proficiency_level(prof_xp: u64) -> u32 {
    let mut lvl = 1u32;
    let mut need = 50u64;
    let mut rem = prof_xp;
    while rem >= need {
        rem -= need;
        lvl += 1;
        need = (lvl as u64) * 50;
        if lvl >= 100 { break; }
    }
    lvl
}

/// XP ganho por matar um inimigo (fallback — helpers por kind mais abaixo).
pub const XP_PER_KILL: u64 = 30;

// Stats/loot/shop de inimigos e itens vivem no DB (server crate::economy).
// Constantes de gameplay puras (cones, delays sem balancing) ficam aqui.

/// Abertura angular do cone de ataque do boss (radianos entre 1o e ultimo proj).
pub const BOSS_SPREAD_RAD: f32 = 1.0; // ~57 graus

/// Tempo de respawn do boss em segundos.
pub const BOSS_RESPAWN_DELAY: f32 = 120.0;

/// Cap máximo de level do personagem. Acima disso, XP continua acumulando
/// mas `level_of_xp` clamp no valor; nenhum SP/stat point novo é gerado.
pub const CHAR_LEVEL_CAP: u32 = 100;

/// Retorna o level derivado a partir da XP acumulada, clamped em
/// `CHAR_LEVEL_CAP`. Curva stair-step: precisa `lvl² × 100` xp pra avançar
/// do lvl pro lvl+1. Cumulativa via loop.
///     L→L+1 cost: 100, 400, 900, 1600, ..., L²×100
///     Total acumulado pra reach L: 100·(L-1)·L·(2L-1)/6
///     L1: 0  L2: 100  L3: 500  L4: 1400  L5: 3000  L10: 28500  L30: 855500  L100: ~33M
pub const fn level_of_xp(xp: u64) -> u32 {
    let mut lvl = 1u32;
    let mut need = 100u64;
    let mut remaining = xp;
    while remaining >= need {
        remaining -= need;
        lvl += 1;
        if lvl >= CHAR_LEVEL_CAP { return CHAR_LEVEL_CAP; }
        need = (lvl as u64) * (lvl as u64) * 100;
    }
    lvl
}

/// XP total necessaria para atingir `level` (acumulada desde L1).
pub const fn xp_for_level(level: u32) -> u64 {
    let mut sum = 0u64;
    let mut l = 1u32;
    while l < level {
        sum += (l as u64) * (l as u64) * 100;
        l += 1;
    }
    sum
}

/// Quantidade de inimigos gerados no inicio.
pub const ENEMY_START_COUNT: usize = 24;

/// Numero de slots do inventario do jogador.
pub const INVENTORY_SLOTS: usize = 40;

/// Raio em tiles pra coletar um loot.
pub const PICKUP_RADIUS: f32 = 0.8;

/// Itens conhecidos. Numeric id vai pro DB e rede. Manter sincronizado com
/// o cliente para sprite/cor por item.
pub mod item_id {
    // Moeda / consumíveis
    pub const GOLD:            u16 = 1;
    pub const HEALTH_POTION:   u16 = 2;
    pub const MANA_POTION:     u16 = 8;
    pub const GREATER_HEAL:    u16 = 9;   // +150 HP
    pub const GREATER_MANA:    u16 = 10;  // +100 MP
    pub const STAMINA_POTION:  u16 = 11;  // restaura 100 stamina
    // Armas
    pub const SWORD:           u16 = 3;
    pub const STAFF:           u16 = 6;
    pub const DAGGER:          u16 = 12;  // rapido, menos dano, +dex
    pub const GREAT_SWORD:     u16 = 13;  // muito dano, -mp
    pub const BOW:             u16 = 14;  // ranged, +dex
    pub const WAND:            u16 = 15;  // fraca mas muito mp
    // Armaduras
    pub const ARMOR:           u16 = 4;
    pub const SHIELD:          u16 = 7;
    pub const LEATHER_ARMOR:   u16 = 16;  // leve, +dex
    pub const PLATE_ARMOR:     u16 = 17;  // pesada, muito HP, -dex
    pub const ROBE:            u16 = 18;  // mago, +mp
    // Acessórios
    pub const RING:            u16 = 5;
    pub const AMULET:          u16 = 19;  // +wis +mp
    pub const LUCKY_RING:      u16 = 20;  // +dex +mp
    // Materiais / loot raro
    pub const GEM:             u16 = 21;  // valioso, vendavel
    pub const IRON_INGOT:      u16 = 22;
    pub const DRAGON_SCALE:    u16 = 23;  // raro de boss
    // === Fase D — novas armas + acessórios ===
    pub const SCIMITAR:        u16 = 24;  // espada curva, atk+atk_spd
    pub const AXE:             u16 = 25;  // machado, atk alto + def (era HAMMER pré-M11)
    pub const SPEAR:           u16 = 26;  // lança, atk+dex
    pub const CROSSBOW:        u16 = 27;  // besta, ranged + crit
    pub const HEAVY_SHIELD:    u16 = 28;  // escudo pesado, def alta
    pub const PENDANT:         u16 = 29;  // pingente, hp+mp
    pub const CHARM:           u16 = 30;  // amuleto crit

    // === Fase E — slots novos (helm/legs/boots/gloves/belt/cape/necklace) ===
    pub const HELM_LEATHER:    u16 = 31;  // capacete leve, def+dex
    pub const HELM_PLATE:      u16 = 32;  // elmo pesado, hp+def
    pub const LEGS_LEATHER:    u16 = 33;  // calça leve, dex+def
    pub const LEGS_PLATE:      u16 = 34;  // calça pesada, hp+def
    pub const BOOTS_LEATHER:   u16 = 35;  // botas leves, mov+dex
    pub const BOOTS_PLATE:     u16 = 36;  // botas pesadas, def+hp
    pub const GLOVES_LEATHER:  u16 = 37;  // luvas leves, atk_spd+dex
    pub const GLOVES_PLATE:    u16 = 38;  // manoplas, atk+def
    pub const BELT_BASIC:      u16 = 39;  // cinto, hp+def
    pub const BELT_MAGIC:      u16 = 40;  // faixa magica, mp+wis
    pub const CAPE_BASIC:      u16 = 41;  // capa, def+hp
    pub const CAPE_MAGIC:      u16 = 42;  // manto magico, mp+wis
    pub const NECKLACE_BASIC:  u16 = 43;  // colar, hp+wis
    pub const NECKLACE_MAGIC:  u16 = 44;  // colar magico, mp+wis

    // === Fase Naval — barcos (consumiveis usados na margem) ===
    /// Lylian Leutard — barco basico de exploracao costeira. Spawn na agua
    /// adjacente quando usado a partir de uma margem walkable.
    pub const BOAT_LYLIAN_LEUTARD: u16 = 100;
}

/// True se o item_id e' um barco (consumido ao usar; spawna entidade Boat).
pub fn is_boat_item(id: u16) -> bool {
    id == item_id::BOAT_LYLIAN_LEUTARD
}

/// Mapeia item_id de barco pra boat_kind do EntityKind::Boat. Mantenha em
/// sync com o cliente (BoatRenderer escolhe sheets pelo kind).
pub fn boat_kind_of(id: u16) -> Option<u16> {
    match id {
        item_id::BOAT_LYLIAN_LEUTARD => Some(0), // 0 = Lylian Leutard
        _ => None,
    }
}

// item_stack_max vive no DB (server crate::economy).

/// Codigo de animacao (`attack_anim::*`) que o cliente deve tocar quando o
/// player ataca com a arma indicada. Mantém em sync com
/// `ItemInfo.AttackAnimOf` no cliente C#.
pub fn weapon_attack_anim(weapon_id: u16) -> u8 {
    use crate::components::attack_anim::*;
    match weapon_id {
        id if id == item_id::BOW       => SHOOT,
        id if id == item_id::CROSSBOW  => SHOOT,
        id if id == item_id::STAFF     => THRUST,
        id if id == item_id::WAND      => THRUST,
        id if id == item_id::SPEAR     => THRUST,
        _ => SLASH,
    }
}

/// Retorna o slot de equipamento para um item_id, ou None se nao for
/// equipavel.
pub fn equip_slot_of(item_id: u16) -> Option<EquipSlot> {
    match item_id {
        id if id == item_id::SWORD
            || id == item_id::STAFF
            || id == item_id::DAGGER
            || id == item_id::GREAT_SWORD
            || id == item_id::BOW
            || id == item_id::WAND
            || id == item_id::SCIMITAR
            || id == item_id::AXE
            || id == item_id::SPEAR
            || id == item_id::CROSSBOW       => Some(EquipSlot::Weapon),
        id if id == item_id::SHIELD
            || id == item_id::HEAVY_SHIELD   => Some(EquipSlot::Offhand),
        id if id == item_id::ARMOR
            || id == item_id::LEATHER_ARMOR
            || id == item_id::PLATE_ARMOR
            || id == item_id::ROBE           => Some(EquipSlot::Armor),
        id if id == item_id::RING
            || id == item_id::LUCKY_RING     => Some(EquipSlot::Ring),
        // Fase E — Amulet/Pendant/Charm migram pra Necklace (slot dedicado)
        id if id == item_id::AMULET
            || id == item_id::PENDANT
            || id == item_id::CHARM
            || id == item_id::NECKLACE_BASIC
            || id == item_id::NECKLACE_MAGIC => Some(EquipSlot::Necklace),
        id if id == item_id::HELM_LEATHER
            || id == item_id::HELM_PLATE     => Some(EquipSlot::Helm),
        id if id == item_id::LEGS_LEATHER
            || id == item_id::LEGS_PLATE     => Some(EquipSlot::Legs),
        id if id == item_id::BOOTS_LEATHER
            || id == item_id::BOOTS_PLATE    => Some(EquipSlot::Boots),
        id if id == item_id::GLOVES_LEATHER
            || id == item_id::GLOVES_PLATE   => Some(EquipSlot::Gloves),
        id if id == item_id::BELT_BASIC
            || id == item_id::BELT_MAGIC     => Some(EquipSlot::Belt),
        id if id == item_id::CAPE_BASIC
            || id == item_id::CAPE_MAGIC     => Some(EquipSlot::Cape),
        _                                    => None,
    }
}

/// True se este item_id eh um escudo (vai no slot Offhand).
pub fn is_shield(item_id: u16) -> bool {
    item_id == item_id::SHIELD || item_id == item_id::HEAVY_SHIELD
}

/// Quais armas permitem equipar um item no Offhand (escudo). One-handed melee
/// (sword, dagger) + unarmed (item_id=0). Two-handed (great_sword) e ranged
/// (bow/staff/wand) NAO permitem — o offhand fica trancado pelo server quando
/// uma dessas armas esta equipada.
pub fn weapon_allows_offhand(weapon_id: u16) -> bool {
    weapon_id == 0
        || weapon_id == item_id::SWORD
        || weapon_id == item_id::DAGGER
        || weapon_id == item_id::SCIMITAR
        || weapon_id == item_id::AXE
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum EquipSlot {
    Weapon,
    Armor,
    Offhand,
    Ring,
    // Fase E
    Helm,
    Legs,
    Boots,
    Gloves,
    Belt,
    Cape,
    Necklace,
}

/// Bonus aplicado por um equipamento. Somado aos stats base.
#[derive(Debug, Clone, Copy, Default)]
pub struct EquipBonus {
    pub hp_max: i32,
    pub mp_max: i32,
    pub attack_damage: i32,
    pub dex: i32,
    pub wis: i32,
    pub defense: i32,
}

/// Pontos de atributo ganhos por level-up.
pub const POINTS_PER_LEVEL: u32 = 3;

/// Skill points ganhos por level-up. Pool global compartilhado entre profs.
pub const SP_PER_LEVEL: u32 = 1;

/// Rank máximo de uma skill (0 = não aprendida; 1..=10 = ranks).
pub const MAX_SKILL_RANK: u8 = 10;

/// Custo em SP de cada rank, indexado por rank (0 = unlock cost = 1, idx 1..10).
/// Total para maxar uma skill = 1+1+1+2+2+2+3+3+3+5 = **23 SP**.
pub const SP_COST_PER_RANK: [u32; 11] = [
    0, // rank 0 = não aprendida
    1, 1, 1,  // rank 1-3: cheap unlock + early
    2, 2, 2,  // rank 4-6: meio (rank 5 = milestone)
    3, 3, 3,  // rank 7-9
    5,        // rank 10 = capstone
];

/// Custo em SP pra subir de `current_rank` para `current_rank + 1`.
/// Retorna 0 se já está no max.
pub const fn sp_cost_for_next_rank(current_rank: u8) -> u32 {
    if current_rank >= MAX_SKILL_RANK { return 0; }
    SP_COST_PER_RANK[(current_rank + 1) as usize]
}

/// Custo total acumulado pra alcançar `rank` (somando rank 1..=rank).
pub const fn sp_cost_to_reach_rank(rank: u8) -> u32 {
    let mut sum = 0u32;
    let mut i = 1u8;
    while i <= rank && i <= MAX_SKILL_RANK {
        sum += SP_COST_PER_RANK[i as usize];
        i += 1;
    }
    sum
}

/// Slots de skill ativa equipados na barra do HUD (teclas 1..=N).
pub const SKILL_BAR_SLOTS: usize = 6;

/// Tiers de skill — define unlock_char_lvl e unlock_prof_lvl recomendados.
/// Skills no DB usam (char_lvl, prof_lvl) explícitos; estes são guidelines pro seed.
pub const SKILL_TIER_UNLOCKS: [(u8, u8); 4] = [
    (5, 10),   // T1 — Aprendiz
    (15, 20),  // T2 — Adepto
    (30, 40),  // T3 — Mestre
    (60, 70),  // T4 — Lendário
];

/// Quantidade de stats alocaveis. Indices: 0=FOR, 1=DES, 2=INT, 3=VIT, 4=SPD, 5=RES.
pub const STAT_COUNT: usize = 6;

/// Aliases de indices pra deixar o codigo legivel.
pub mod stat_idx {
    pub const FOR: usize = 0;
    pub const DES: usize = 1;
    pub const INT: usize = 2;
    pub const VIT: usize = 3;
    pub const SPD: usize = 4;
    pub const RES: usize = 5;
}

/// Multiplicador de velocidade adicional por ponto em SPD (somado a 1.0).
/// Zerado: SPD nao escala mais movement speed (movement vira default
/// uniforme). SPD continua dando stamina/regen + dash CD reduction.
pub const MOVE_SPEED_PCT_PER_SPD: f32 = 0.0;
/// % redução de cooldown do dash por ponto em SPD. Final dash CD =
/// DASH_COOLDOWN / (1 + DASH_CD_REDUCTION_PER_SPD * spd_points).
pub const DASH_CD_REDUCTION_PER_SPD: f32 = 0.02; // +2% redução / ponto

/// Chance de crit adicionada por ponto em DES (somada a 0.0).
pub const CRIT_CHANCE_PER_DES: f32 = 0.005; // +0.5% por ponto

/// Velocidade de ataque adicional por ponto em DES (somada a 1.0).
/// Aplicada como divisor no cooldown — 1.5 = ataques 50% mais rapidos.
pub const ATTACK_SPEED_PCT_PER_DES: f32 = 0.015; // +1.5% por ponto

/// Stamina maxima adicional por ponto em SPD (somada ao base 100).
pub const STAMINA_MAX_PER_SPD: i32 = 2;

/// Regen de stamina/seg adicional por ponto em SPD (somado ao base 25).
pub const STAMINA_REGEN_PER_SPD: f32 = 0.2;

/// HP regenerado/seg adicionado por ponto em VIT.
pub const HP_REGEN_PER_VIT: f32 = 0.2;

/// Multiplicador de dano em hit critico.
pub const CRIT_DAMAGE_MULT: f32 = 1.5;

/// Defesa adicional por ponto em RES (somada ao base 0).
pub const DEFENSE_PER_RES: i32 = 1;

/// Aumento da fração de dano absorvido por block, por ponto em RES (somado
/// ao base `BLOCK_DAMAGE_REDUCTION_BASE = 0.6`). Ex.: +0.003 × 100 pontos =
/// +30% absorvido → 90% total. Capado em `BLOCK_REDUCTION_MAX`.
pub const BLOCK_REDUCTION_PER_RES: f32 = 0.003;

/// Reducao do multiplicador de custo de stamina por ponto em RES (subtraido
/// do base 1.0). Aplica em block E parry. Ex.: -0.005 × 100 = -50% → custos
/// caem pra 50%. Capado em `STAMINA_COST_MULT_MIN`.
pub const STAMINA_COST_REDUCTION_PER_RES: f32 = 0.005;

/// Custo BASE de stamina ao bloquear um ataque com sucesso. RES reduz via
/// `defense_stamina_cost_mult`.
pub const BLOCK_STAMINA_COST: f32 = 25.0;

/// Custo BASE de stamina ao executar um parry com sucesso. RES reduz via
/// `defense_stamina_cost_mult`.
pub const PARRY_STAMINA_COST: f32 = 15.0;

/// Fração base de dano absorvido por block (0.0 = sem efeito, 1.0 = anula tudo).
/// RES soma `BLOCK_REDUCTION_PER_RES` por ponto.
pub const BLOCK_DAMAGE_REDUCTION_BASE: f32 = 0.6;

/// Cap maximo de absorcao de dano por block (player nunca toma menos que
/// 5% do dano original quando bloqueia).
pub const BLOCK_REDUCTION_MAX: f32 = 0.95;

/// Cap minimo do multiplicador de custo de stamina (player nunca paga
/// menos que 50% do custo base de block/parry).
pub const STAMINA_COST_MULT_MIN: f32 = 0.5;

/// Janela em segundos durante a qual o atacante fica em stagger apos um parry.
pub const PARRY_STAGGER_S: f32 = 0.6;

/// Multiplicador de velocidade enquanto o player segura RMB (defesa ativa).
pub const MOVE_SPEED_DEFENDING_MULT: f32 = 0.4;

/// Janela em segundos apos um press de PRIMARY/SECONDARY pra contar como
/// tentativa de parry contra um hit incoming. ~250ms em 60fps = 15 frames.
pub const PARRY_WINDOW_S: f32 = 0.25;

/// Bonus aplicado por 1 ponto em cada stat (6 stats — design FOR/DES/INT/VIT/SPD/RES).
/// Indices alinhados com `stat_idx::*`.
///
/// FOR (Forca):        +1 atk, +2 hp_max
/// DES (Destreza):     +1 dex, +CRIT_CHANCE_PER_DES crit, +ATTACK_SPEED_PCT_PER_DES atk speed
/// INT (Inteligencia): +1 wis, +2 mp_max
/// VIT (Vitalidade):   +5 hp_max, +HP_REGEN_PER_VIT hp regen
/// SPD (Velocidade):   +MOVE_SPEED_PCT_PER_SPD move speed, +STAMINA_MAX_PER_SPD stamina, +STAMINA_REGEN_PER_SPD st regen
/// RES (Resistencia):  +DEFENSE_PER_RES def, +BLOCK_REDUCTION_PER_RES dmg absorvido em block,
///                     -STAMINA_COST_REDUCTION_PER_RES no custo de block/parry
pub const STAT_POINT_BONUS: [StatAllocBonus; STAT_COUNT] = [
    /* FOR */ StatAllocBonus { hp_max: 2, mp_max: 0, attack_damage: 1, dex: 0, wis: 0, defense: 0,                speed_pct: 0.0,                      crit_chance: 0.0,                  hp_regen: 0.0,               attack_speed_pct: 0.0,                       stamina_max: 0,                  stamina_regen: 0.0,                  block_reduction_bonus: 0.0,             stamina_cost_reduction: 0.0, dash_cd_reduction_pct: 0.0 },
    /* DES */ StatAllocBonus { hp_max: 0, mp_max: 0, attack_damage: 0, dex: 1, wis: 0, defense: 0,                speed_pct: 0.0,                      crit_chance: CRIT_CHANCE_PER_DES,  hp_regen: 0.0,               attack_speed_pct: ATTACK_SPEED_PCT_PER_DES,  stamina_max: 0,                  stamina_regen: 0.0,                  block_reduction_bonus: 0.0,             stamina_cost_reduction: 0.0, dash_cd_reduction_pct: 0.0 },
    /* INT */ StatAllocBonus { hp_max: 0, mp_max: 2, attack_damage: 0, dex: 0, wis: 1, defense: 0,                speed_pct: 0.0,                      crit_chance: 0.0,                  hp_regen: 0.0,               attack_speed_pct: 0.0,                       stamina_max: 0,                  stamina_regen: 0.0,                  block_reduction_bonus: 0.0,             stamina_cost_reduction: 0.0, dash_cd_reduction_pct: 0.0 },
    /* VIT */ StatAllocBonus { hp_max: 5, mp_max: 0, attack_damage: 0, dex: 0, wis: 0, defense: 0,                speed_pct: 0.0,                      crit_chance: 0.0,                  hp_regen: HP_REGEN_PER_VIT,  attack_speed_pct: 0.0,                       stamina_max: 0,                  stamina_regen: 0.0,                  block_reduction_bonus: 0.0,             stamina_cost_reduction: 0.0, dash_cd_reduction_pct: 0.0 },
    /* SPD */ StatAllocBonus { hp_max: 0, mp_max: 0, attack_damage: 0, dex: 0, wis: 0, defense: 0,                speed_pct: 0.0,                      crit_chance: 0.0,                  hp_regen: 0.0,               attack_speed_pct: 0.0,                       stamina_max: STAMINA_MAX_PER_SPD,stamina_regen: STAMINA_REGEN_PER_SPD,    block_reduction_bonus: 0.0,             stamina_cost_reduction: 0.0, dash_cd_reduction_pct: DASH_CD_REDUCTION_PER_SPD },
    /* RES */ StatAllocBonus { hp_max: 0, mp_max: 0, attack_damage: 0, dex: 0, wis: 0, defense: DEFENSE_PER_RES,  speed_pct: 0.0,                      crit_chance: 0.0,                  hp_regen: 0.0,               attack_speed_pct: 0.0,                       stamina_max: 0,                  stamina_regen: 0.0,                  block_reduction_bonus: BLOCK_REDUCTION_PER_RES, stamina_cost_reduction: STAMINA_COST_REDUCTION_PER_RES, dash_cd_reduction_pct: 0.0 },
];

/// Bonus de alocacao de pontos. Difere de `EquipBonus` por incluir
/// modificadores de velocidade / crit / regen / atk speed — campos que ainda
/// nao sao expostos em equipamento (pode-se unificar futuramente).
#[derive(Debug, Clone, Copy, Default)]
pub struct StatAllocBonus {
    pub hp_max: i32,
    pub mp_max: i32,
    pub attack_damage: i32,
    pub dex: i32,
    pub wis: i32,
    pub defense: i32,
    pub speed_pct: f32,
    pub crit_chance: f32,
    pub hp_regen: f32,
    pub attack_speed_pct: f32,
    pub stamina_max: i32,
    pub stamina_regen: f32,
    /// Adiciona ao base BLOCK_DAMAGE_REDUCTION_BASE — fração extra absorvida.
    pub block_reduction_bonus: f32,
    /// Subtrai do multiplicador de custo de stamina (1.0 = base).
    pub stamina_cost_reduction: f32,
    /// Adiciona ao bonus % de redução de cooldown do dash. Final mult =
    /// 1.0 + dash_cd_reduction_pct (clampado em [1, X]). Cooldown final =
    /// DASH_COOLDOWN / mult.
    pub dash_cd_reduction_pct: f32,
}

/// Escalamento por level de proficiencia, aplicado quando a arma correspondente
/// esta equipada. Tudo em f32 e truncado depois de multiplicar pelo level.
#[derive(Debug, Clone, Copy, Default)]
pub struct WeaponScaling {
    pub hp_max: f32,
    pub mp_max: f32,
    pub attack_damage: f32,
    pub dex: f32,
    pub wis: f32,
    pub defense: f32,
}

/// Scaling por arma equipada. level vem da Proficiency::from_item(weapon_id).
/// Unarmed (sem arma): usa `unarmed_scaling()`.
pub const fn weapon_scaling(item_id: u16) -> WeaponScaling {
    match item_id {
        // Espada (sword & board): tanque — +HP, +Res
        id if id == item_id::SWORD => WeaponScaling {
            hp_max: 1.0, mp_max: 0.0, attack_damage: 0.0, dex: 0.0, wis: 0.0, defense: 0.1,
        },
        // Espadao: DPS puro
        id if id == item_id::GREAT_SWORD => WeaponScaling {
            hp_max: 0.0, mp_max: 0.0, attack_damage: 0.5, dex: 0.0, wis: 0.0, defense: 0.0,
        },
        // Adaga: duelista
        id if id == item_id::DAGGER => WeaponScaling {
            hp_max: 0.0, mp_max: 0.0, attack_damage: 0.3, dex: 0.2, wis: 0.0, defense: 0.0,
        },
        // Cajado: caster hibrido
        id if id == item_id::STAFF => WeaponScaling {
            hp_max: 0.0, mp_max: 1.0, attack_damage: 0.0, dex: 0.0, wis: 0.2, defense: 0.0,
        },
        // Arco: ranger
        id if id == item_id::BOW => WeaponScaling {
            hp_max: 0.0, mp_max: 0.0, attack_damage: 0.1, dex: 0.5, wis: 0.0, defense: 0.0,
        },
        // Varinha: caster puro
        id if id == item_id::WAND => WeaponScaling {
            hp_max: 0.0, mp_max: 1.0, attack_damage: 0.0, dex: 0.0, wis: 0.3, defense: 0.0,
        },
        // Machado: heavy hitter — atk alto + um pouco de hp
        id if id == item_id::AXE => WeaponScaling {
            hp_max: 0.5, mp_max: 0.0, attack_damage: 0.4, dex: 0.0, wis: 0.0, defense: 0.0,
        },
        // Lança: zoning — atk médio + dex (alcance)
        id if id == item_id::SPEAR => WeaponScaling {
            hp_max: 0.0, mp_max: 0.0, attack_damage: 0.25, dex: 0.15, wis: 0.0, defense: 0.0,
        },
        _ => WeaponScaling {
            hp_max: 0.0, mp_max: 0.0, attack_damage: 0.0, dex: 0.0, wis: 0.0, defense: 0.0,
        },
    }
}

/// Scaling quando sem arma (Unarmed). Aplicado com Proficiency::Unarmed level.
pub const fn unarmed_scaling() -> WeaponScaling {
    WeaponScaling {
        hp_max: 0.0, mp_max: 0.0, attack_damage: 0.2, dex: 0.0, wis: 0.0, defense: 0.0,
    }
}

/// True se a arma e de corpo-a-corpo (gera dano em cone na frente ao atacar,
/// nao projetil). Sem arma = melee (soco). BOW/WAND/STAFF disparam projetil.
pub const fn weapon_is_melee(item_id: u16) -> bool {
    item_id == 0
        || item_id == item_id::SWORD
        || item_id == item_id::GREAT_SWORD
        || item_id == item_id::DAGGER
        || item_id == item_id::SCIMITAR
        || item_id == item_id::AXE
        // SPEAR e melee mas usa anim Thrust — atualmente damage gen e
        // controlado pela melee path baseado em is_melee, então mantém
        // como melee aqui (cone na frente).
        || item_id == item_id::SPEAR
}

/// True se o inimigo desse kind ataca em melee (cone de dano direto na frente)
/// ao inves de spawnar projetil. Kinds sem kite_dist são melee:
/// 0=Grunt, 1=Tank, 3=Ninja, 5=Berserker. Ranger/Mago/Arqueiro/Boss = ranged.
pub const fn enemy_is_melee(kind: u16) -> bool {
    matches!(kind, 0 | 1 | 3 | 5)
}

/// Raio do golpe melee em tiles.
pub const MELEE_RANGE: f32 = 1.8;
/// Meio-angulo do cone em radianos (cone total = 2x).
pub const MELEE_CONE_HALF_ANGLE: f32 = std::f32::consts::FRAC_PI_3; // 60 graus -> 120 total

pub const fn item_bonus(item_id: u16) -> EquipBonus {
    match item_id {
        // Armas
        id if id == item_id::SWORD        => EquipBonus { hp_max:  0,  mp_max:   0, attack_damage: 10, dex: 0,  wis: 0, defense: 0 },
        id if id == item_id::STAFF        => EquipBonus { hp_max:  0,  mp_max:  40, attack_damage: 20, dex: 0,  wis: 5, defense: 0 },
        id if id == item_id::DAGGER       => EquipBonus { hp_max:  0,  mp_max:   0, attack_damage:  7, dex: 10, wis: 0, defense: 0 },
        id if id == item_id::GREAT_SWORD  => EquipBonus { hp_max:  0,  mp_max: -20, attack_damage: 28, dex: -3, wis: 0, defense: 0 },
        id if id == item_id::BOW          => EquipBonus { hp_max:  0,  mp_max:   0, attack_damage: 14, dex: 12, wis: 0, defense: 0 },
        id if id == item_id::WAND         => EquipBonus { hp_max:  0,  mp_max:  80, attack_damage:  6, dex: 0,  wis: 8, defense: 0 },
        // Armaduras
        id if id == item_id::ARMOR        => EquipBonus { hp_max: 40,  mp_max:   0, attack_damage:  0, dex: 0,  wis: 0, defense:  5 },
        id if id == item_id::SHIELD       => EquipBonus { hp_max: 75,  mp_max:   0, attack_damage: -5, dex: 0,  wis: 0, defense:  8 },
        id if id == item_id::LEATHER_ARMOR=> EquipBonus { hp_max: 25,  mp_max:   0, attack_damage:  0, dex: 6,  wis: 0, defense:  3 },
        id if id == item_id::PLATE_ARMOR  => EquipBonus { hp_max:120,  mp_max: -10, attack_damage:  0, dex: -5, wis: 0, defense: 12 },
        id if id == item_id::ROBE         => EquipBonus { hp_max: 10,  mp_max:  60, attack_damage:  0, dex: 0,  wis: 8, defense:  2 },
        // Acessorios
        id if id == item_id::RING         => EquipBonus { hp_max:  0,  mp_max:   0, attack_damage:  0, dex: 5,  wis: 3, defense: 0 },
        id if id == item_id::AMULET       => EquipBonus { hp_max: 15,  mp_max:  30, attack_damage:  0, dex: 0,  wis: 8, defense: 1 },
        id if id == item_id::LUCKY_RING   => EquipBonus { hp_max: 10,  mp_max:  20, attack_damage:  2, dex: 6,  wis: 2, defense: 0 },
        // === Fase D — novas armas / armaduras / acessórios ===
        id if id == item_id::SCIMITAR     => EquipBonus { hp_max:  0,  mp_max:   0, attack_damage:  8, dex: 6,  wis: 0, defense: 0 },
        id if id == item_id::AXE       => EquipBonus { hp_max: 20,  mp_max:   0, attack_damage: 22, dex: -2, wis: 0, defense: 3 },
        id if id == item_id::SPEAR        => EquipBonus { hp_max:  0,  mp_max:   0, attack_damage: 16, dex: 5,  wis: 0, defense: 0 },
        id if id == item_id::CROSSBOW     => EquipBonus { hp_max:  0,  mp_max:   0, attack_damage: 18, dex: 8,  wis: 0, defense: 0 },
        id if id == item_id::HEAVY_SHIELD => EquipBonus { hp_max: 110, mp_max:   0, attack_damage: -8, dex: -3, wis: 0, defense: 14 },
        id if id == item_id::PENDANT      => EquipBonus { hp_max: 25,  mp_max:  35, attack_damage:  0, dex: 0,  wis: 4, defense: 1 },
        id if id == item_id::CHARM        => EquipBonus { hp_max:  0,  mp_max:  10, attack_damage:  3, dex: 4,  wis: 4, defense: 0 },
        // === Fase E — slots novos ===
        id if id == item_id::HELM_LEATHER => EquipBonus { hp_max: 15,  mp_max:   0, attack_damage:  0, dex: 4,  wis: 0, defense:  3 },
        id if id == item_id::HELM_PLATE   => EquipBonus { hp_max: 45,  mp_max:   0, attack_damage:  0, dex: -2, wis: 0, defense:  7 },
        id if id == item_id::LEGS_LEATHER => EquipBonus { hp_max: 20,  mp_max:   0, attack_damage:  0, dex: 5,  wis: 0, defense:  3 },
        id if id == item_id::LEGS_PLATE   => EquipBonus { hp_max: 60,  mp_max:   0, attack_damage:  0, dex: -3, wis: 0, defense:  9 },
        id if id == item_id::BOOTS_LEATHER=> EquipBonus { hp_max: 10,  mp_max:   0, attack_damage:  0, dex: 6,  wis: 0, defense:  2 },
        id if id == item_id::BOOTS_PLATE  => EquipBonus { hp_max: 30,  mp_max:   0, attack_damage:  0, dex: -2, wis: 0, defense:  5 },
        id if id == item_id::GLOVES_LEATHER=>EquipBonus { hp_max:  5,  mp_max:   0, attack_damage:  3, dex: 5,  wis: 0, defense:  1 },
        id if id == item_id::GLOVES_PLATE => EquipBonus { hp_max: 20,  mp_max:   0, attack_damage:  6, dex: -1, wis: 0, defense:  4 },
        id if id == item_id::BELT_BASIC   => EquipBonus { hp_max: 25,  mp_max:   0, attack_damage:  0, dex: 0,  wis: 0, defense:  2 },
        id if id == item_id::BELT_MAGIC   => EquipBonus { hp_max:  5,  mp_max:  35, attack_damage:  0, dex: 0,  wis: 4, defense:  1 },
        id if id == item_id::CAPE_BASIC   => EquipBonus { hp_max: 20,  mp_max:   0, attack_damage:  0, dex: 0,  wis: 0, defense:  4 },
        id if id == item_id::CAPE_MAGIC   => EquipBonus { hp_max:  0,  mp_max:  45, attack_damage:  0, dex: 0,  wis: 6, defense:  2 },
        id if id == item_id::NECKLACE_BASIC=>EquipBonus { hp_max: 20,  mp_max:  10, attack_damage:  0, dex: 0,  wis: 3, defense:  0 },
        id if id == item_id::NECKLACE_MAGIC=>EquipBonus { hp_max:  0,  mp_max:  40, attack_damage:  0, dex: 0,  wis: 7, defense:  0 },
        _                                 => EquipBonus { hp_max:  0,  mp_max:   0, attack_damage:  0, dex: 0,  wis: 0, defense: 0 },
    }
}

/// Quanto HP uma pocao de vida restaura.
pub const HEALTH_POTION_HEAL: i32 = 50;

/// Regen de MP por segundo (inteiro).
pub const MP_REGEN_PER_SEC: f32 = 4.0;

/// Custo em MP da habilidade secundaria (triple-shot).
pub const SECONDARY_MP_COST: i32 = 25;

/// Cooldown entre ataques secundarios em segundos.
pub const SECONDARY_COOLDOWN: f32 = 0.5;

/// Quantidade de projeteis disparados pela habilidade secundaria e abertura
/// angular entre o primeiro e o ultimo (radianos).
pub const SECONDARY_PROJ_COUNT: i32 = 3;
pub const SECONDARY_SPREAD_RAD: f32 = 0.35; // ~20 graus

/// Raio em tiles pra interagir com NPC vendedor.
pub const INTERACT_RADIUS: f32 = 3.0;

// Loja e preços de venda vivem no DB (server crate::economy).

/// IDs logicos de tile — usados no WorldMap e no TileDef lookup.
pub mod tile_id {
    pub const FLOOR: u16 = 1;
    pub const WALL:  u16 = 2;
    pub const DIRT:  u16 = 3;
    pub const WATER: u16 = 4;
    /// Piso de dungeon — visualmente distinto, colisoes iguais a FLOOR.
    pub const DUNGEON_FLOOR: u16 = 5;
    pub const WOOD:  u16 = 5;
}
