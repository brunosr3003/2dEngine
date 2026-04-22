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
pub const PROTOCOL_VERSION: u16 = 23;

/// Velocidade base do jogador em tiles/segundo.
pub const PLAYER_SPEED: f32 = 5.0;

/// Multiplicador de velocidade durante sprint (Shift + stamina > 0).
pub const SPRINT_SPEED_MULT: f32 = 1.65;

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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[repr(u8)]
pub enum Proficiency {
    Sword    = 0,
    Staff    = 1,
    Dagger   = 2,
    Bow      = 3,
    Wand     = 4,
    Unarmed  = 5,
}

impl Proficiency {
    pub fn all() -> &'static [Proficiency] {
        &[Self::Sword, Self::Staff, Self::Dagger, Self::Bow, Self::Wand, Self::Unarmed]
    }

    pub fn from_item(id: u16) -> Self {
        match id {
            _ if id == item_id::SWORD || id == item_id::GREAT_SWORD => Self::Sword,
            _ if id == item_id::STAFF    => Self::Staff,
            _ if id == item_id::DAGGER   => Self::Dagger,
            _ if id == item_id::BOW      => Self::Bow,
            _ if id == item_id::WAND     => Self::Wand,
            _                            => Self::Unarmed,
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

/// Definicao estatica de um tipo de inimigo.
#[derive(Debug, Clone, Copy)]
pub struct EnemyKindDef {
    pub hp_max: i32,
    pub speed: f32,
    pub attack_damage: i32,
    pub attack_cooldown: f32,
    pub detect_range: f32,
    pub xp_reward: u64,
    /// Resistencia flat; reduz dano recebido (`dano_real = max(1, dmg - defense)`).
    pub defense: i32,
    /// Tint multiplicativo aplicado no sprite pelo cliente pra diferenciar.
    pub tint_rgba: [f32; 4],
}

/// Tabela de kinds. O index no array == EntityKind::Enemy(u16).
/// Manter em sincronia com spawn_initial_enemies / respawn do servidor.
pub const ENEMY_KINDS: &[EnemyKindDef] = &[
    // 0 — Grunt: o basico. Tint branco.
    EnemyKindDef {
        hp_max: 50,  speed: 2.0, attack_damage: 10, attack_cooldown: 2.0,
        detect_range: 9.0,  xp_reward: 30, defense: 0, tint_rgba: [1.0, 1.0, 1.0, 1.0],
    },
    // 1 — Tank: mais HP, mais lento, dano maior. Tint vermelho escuro.
    EnemyKindDef {
        hp_max: 120, speed: 1.3, attack_damage: 18, attack_cooldown: 2.8,
        detect_range: 8.0,  xp_reward: 75, defense: 8, tint_rgba: [1.0, 0.35, 0.25, 1.0],
    },
    // 2 — Ranger: range longo, menos HP, mais rapido. Tint ciano forte.
    EnemyKindDef {
        hp_max: 35,  speed: 2.4, attack_damage: 12, attack_cooldown: 1.5,
        detect_range: 13.0, xp_reward: 50, defense: 0, tint_rgba: [0.3, 0.9, 1.0, 1.0],
    },
    // 3 — Ninja: muito rapido, melee, pouco HP. Tint roxo.
    EnemyKindDef {
        hp_max: 40,  speed: 4.2, attack_damage: 15, attack_cooldown: 1.0,
        detect_range: 11.0, xp_reward: 55, defense: 2, tint_rgba: [0.75, 0.2, 1.0, 1.0],
    },
    // 4 — Mago: lento, projéteis de longo alcance, alto dano. Tint azul-índigo.
    EnemyKindDef {
        hp_max: 45,  speed: 1.4, attack_damage: 22, attack_cooldown: 2.2,
        detect_range: 15.0, xp_reward: 70, defense: 1, tint_rgba: [0.4, 0.4, 1.0, 1.0],
    },
    // 5 — Berserker: muito HP, muito dano, lento. Tint laranja.
    EnemyKindDef {
        hp_max: 200, speed: 1.5, attack_damage: 28, attack_cooldown: 3.0,
        detect_range: 8.0,  xp_reward: 110, defense: 4, tint_rgba: [1.0, 0.5, 0.1, 1.0],
    },
    // 6 — Arqueiro: distancia media, projéteis rapidos, kite. Tint verde.
    EnemyKindDef {
        hp_max: 45,  speed: 2.8, attack_damage: 14, attack_cooldown: 1.6,
        detect_range: 13.0, xp_reward: 60, defense: 1, tint_rgba: [0.2, 0.9, 0.3, 1.0],
    },
    // 7 — Boss: enorme HP, ataque em cone, lento, detecta tudo. Tint dourado.
    EnemyKindDef {
        hp_max: 700, speed: 1.6, attack_damage: 40, attack_cooldown: 2.8,
        detect_range: 18.0, xp_reward: 600, defense: 20, tint_rgba: [1.0, 0.85, 0.15, 1.0],
    },
];

/// Tamanho do sprite relativo ao sprite padrao (0.95 tiles).
pub fn enemy_size_scale(kind: u16) -> f32 {
    match kind {
        1 => 1.3,   // tank maior
        2 => 0.85,  // ranger menor
        3 => 0.75,  // ninja pequeno/rapido
        5 => 1.5,   // berserker enorme
        7 => 2.2,   // boss gigante
        _ => 1.0,
    }
}

/// Range de ataque de um tipo de inimigo (tiles).
pub fn enemy_attack_range(kind: u16) -> f32 {
    match kind {
        2 | 6 => 9.0,    // ranger / arqueiro: alcance medio
        4     => 12.0,   // mago: longo alcance
        7     => 13.0,   // boss: muito longo
        _     => 1.8,    // melee
    }
}

/// Distancia de kite desejada (mob ranged se afasta se jogador muito perto).
/// None = melee, sem kite.
pub fn enemy_kite_dist(kind: u16) -> Option<f32> {
    match kind {
        2 => Some(5.0),
        4 => Some(8.0),
        6 => Some(7.0),
        7 => Some(10.0),
        _ => None,
    }
}

/// Numero de projéteis por ataque (boss dispara cone).
pub fn enemy_proj_count(kind: u16) -> u32 {
    if kind == 7 { 5 } else { 1 }
}

/// Abertura angular do cone de ataque do boss (radianos entre 1o e ultimo proj).
pub const BOSS_SPREAD_RAD: f32 = 1.0; // ~57 graus

/// Tempo de respawn do boss em segundos.
pub const BOSS_RESPAWN_DELAY: f32 = 120.0;

pub fn enemy_def(kind: u16) -> &'static EnemyKindDef {
    ENEMY_KINDS
        .get(kind as usize)
        .unwrap_or(&ENEMY_KINDS[0])
}

/// Retorna o level derivado a partir da XP acumulada.
/// Curva simples quadratica: L = 1 + floor(sqrt(xp / 100)).
///     L1: 0 xp  L2: 100  L3: 400  L4: 900  L5: 1600  L10: 8100
pub const fn level_of_xp(xp: u64) -> u32 {
    let mut lvl = 1u32;
    let mut need = 100u64;
    let mut remaining = xp;
    while remaining >= need {
        remaining -= need;
        lvl += 1;
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
pub const INVENTORY_SLOTS: usize = 24;

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
}

/// Limite de stack por item (1 = nao stackavel / equipamento).
pub const fn item_stack_max(id: u16) -> u32 {
    match id {
        item_id::GOLD           => 9999,
        item_id::HEALTH_POTION  | item_id::MANA_POTION
        | item_id::GREATER_HEAL | item_id::GREATER_MANA
        | item_id::STAMINA_POTION => 20,
        item_id::GEM | item_id::IRON_INGOT | item_id::DRAGON_SCALE => 99,
        _ => 1,
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
            || id == item_id::WAND           => Some(EquipSlot::Weapon),
        id if id == item_id::ARMOR
            || id == item_id::SHIELD
            || id == item_id::LEATHER_ARMOR
            || id == item_id::PLATE_ARMOR
            || id == item_id::ROBE           => Some(EquipSlot::Armor),
        id if id == item_id::RING
            || id == item_id::AMULET
            || id == item_id::LUCKY_RING     => Some(EquipSlot::Ring),
        _                                    => None,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum EquipSlot {
    Weapon,
    Armor,
    Ring,
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

/// Bonus absoluto aplicado por 1 ponto investido em cada stat.
/// Indices: 0=HP, 1=MP, 2=Atk, 3=Dex, 4=Wis, 5=Res.
pub const STAT_POINT_BONUS: [EquipBonus; 6] = [
    EquipBonus { hp_max: 5, mp_max: 0, attack_damage: 0, dex: 0, wis: 0, defense: 0 },
    EquipBonus { hp_max: 0, mp_max: 2, attack_damage: 0, dex: 0, wis: 0, defense: 0 },
    EquipBonus { hp_max: 0, mp_max: 0, attack_damage: 1, dex: 0, wis: 0, defense: 0 },
    EquipBonus { hp_max: 0, mp_max: 0, attack_damage: 0, dex: 1, wis: 0, defense: 0 },
    EquipBonus { hp_max: 0, mp_max: 0, attack_damage: 0, dex: 0, wis: 1, defense: 0 },
    EquipBonus { hp_max: 0, mp_max: 0, attack_damage: 0, dex: 0, wis: 0, defense: 1 },
];

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

/// Precos fixos da loja (item_id, preco em ouro). Ordem define o indice
/// usado em `ClientMessage::ShopBuy { slot_idx }`.
pub const SHOP_ITEMS: [(u16, u32); 9] = [
    (item_id::HEALTH_POTION, 10),
    (item_id::MANA_POTION,   15),
    (item_id::GREATER_HEAL,  40),
    (item_id::GREATER_MANA,  50),
    (item_id::STAMINA_POTION, 20),
    (item_id::DAGGER,        80),
    (item_id::LEATHER_ARMOR, 120),
    (item_id::BOW,           150),
    (item_id::AMULET,        180),
];

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
