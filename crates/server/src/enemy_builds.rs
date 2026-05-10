//! Builds proceduralmente gerados de inimigos por (level, classe).
//!
//! Substitui o lookup hardcoded de 8 kinds por uma escala 1-100 dividida em
//! 10 tiers tematicos (goblin verde, goblin amarelo, humano vilao, demonio
//! menor, etc.). Cada tier define classes permitidas, tema visual e tint;
//! `build_for_level` deriva stats (HP/atk/def via allocated_points), equip
//! progression, skills aprendidas e parametros AI (range, speed, kite, etc).
//!
//! Stats efetivos sao calculados via `effective_stats` no spawn — mesmo
//! caminho dos players. AI fields ficam cacheados no EnemyTag pra evitar
//! lookup repetido no tick.

use shared::{EquipSlot, Equipment, LearnedSkill, VisualConfig, PROF_COUNT, STAT_COUNT};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EnemyClass {
    Sword,        // melee 1H
    Dagger,       // melee fast 1H
    Bow,          // ranged
    Staff,        // ranged magic (fireball)
    Wand,         // ranged magic (electric)
    Axe,          // melee 1H heavy
    Spear,        // melee reach
    GreatSword,   // melee 2H
    SwordShield,  // melee + offhand shield (tanky)
}

impl EnemyClass {
    pub fn as_str(self) -> &'static str {
        match self {
            EnemyClass::Sword => "Sword",
            EnemyClass::Dagger => "Dagger",
            EnemyClass::Bow => "Bow",
            EnemyClass::Staff => "Staff",
            EnemyClass::Wand => "Wand",
            EnemyClass::Axe => "Axe",
            EnemyClass::Spear => "Spear",
            EnemyClass::GreatSword => "GreatSword",
            EnemyClass::SwordShield => "SwordShield",
        }
    }

    pub fn is_melee(self) -> bool {
        matches!(self,
            EnemyClass::Sword | EnemyClass::Dagger | EnemyClass::Axe
            | EnemyClass::Spear | EnemyClass::GreatSword | EnemyClass::SwordShield)
    }

    /// Tipo de projetil pro spawn de tiro: 0=arrow, 1=fireball, 5=electric.
    pub fn proj_kind(self) -> u8 {
        match self {
            EnemyClass::Bow => 0,
            EnemyClass::Staff => 1,
            EnemyClass::Wand => 5,
            _ => 0,
        }
    }

    /// Locomotor speed base — varia por classe (Dagger rapido, GreatSword lento).
    pub fn base_speed(self) -> f32 {
        match self {
            EnemyClass::Dagger => 4.0,
            EnemyClass::Bow | EnemyClass::Wand => 2.4,
            EnemyClass::Sword | EnemyClass::Spear => 2.2,
            EnemyClass::Axe | EnemyClass::SwordShield => 1.8,
            EnemyClass::Staff => 1.6,
            EnemyClass::GreatSword => 1.5,
        }
    }

    /// Cooldown base entre ataques.
    pub fn base_cooldown(self) -> f32 {
        match self {
            EnemyClass::Dagger => 1.0,
            EnemyClass::Sword => 1.6,
            EnemyClass::Spear => 1.7,
            EnemyClass::Axe => 2.4,
            EnemyClass::SwordShield => 2.2,
            EnemyClass::GreatSword => 2.8,
            EnemyClass::Bow | EnemyClass::Wand => 1.5,
            EnemyClass::Staff => 2.0,
        }
    }

    pub fn attack_range(self) -> f32 {
        if self.is_melee() { 1.8 } else { 9.0 }
    }

    pub fn kite_dist(self) -> Option<f32> {
        match self {
            EnemyClass::Bow | EnemyClass::Wand => Some(6.0),
            EnemyClass::Staff => Some(7.0),
            _ => None,
        }
    }

    /// Raio em tiles em que o inimigo "ve" e persegue o player. Ranged tem
    /// detect_range maior pra alcancar o kite_dist + margem.
    pub fn detect_range(self) -> f32 {
        match self {
            EnemyClass::Staff => 13.0,
            EnemyClass::Bow | EnemyClass::Wand => 12.0,
            EnemyClass::Dagger => 10.0,
            _ => 9.0,
        }
    }
}

/// Tema visual de um tier — paper-doll preset + tint RGBA aplicado por
/// cima dos sprites. Permite distinguir goblin verde (tint verde) de goblin
/// amarelo (tint amarelo) sem ter sprites separados.
#[derive(Debug, Clone, Copy)]
pub struct TierTheme {
    pub display_name: &'static str,
    pub visual_class: &'static str,    // "warrior" | "wizard" | "archer"
    pub body_tint: [f32; 4],
    pub size_scale: f32,
}

#[derive(Debug, Clone)]
pub struct TierConfig {
    pub id: u8,
    pub level_min: u32,
    pub level_max: u32,
    pub theme: TierTheme,
    pub allowed_classes: &'static [EnemyClass],
}

/// Os 10 tiers. Bosses (a cada 10 levels) sao um caminho separado mas
/// reutilizam o tema visual do tier do level.
pub fn tier_for_level(level: u32) -> TierConfig {
    use EnemyClass::*;
    let level = level.max(1).min(100);
    match level {
        1..=10 => TierConfig {
            id: 1, level_min: 1, level_max: 10,
            theme: TierTheme {
                display_name: "Green Goblin",
                visual_class: "warrior",
                body_tint: [0.55, 0.85, 0.50, 1.0],   // verde claro
                size_scale: 1.0,
            },
            allowed_classes: &[Sword, Dagger, Bow, Staff],
        },
        11..=20 => TierConfig {
            id: 2, level_min: 11, level_max: 20,
            theme: TierTheme {
                display_name: "Yellow Goblin",
                visual_class: "warrior",
                body_tint: [1.0, 0.9, 0.4, 1.0],
                size_scale: 1.0,
            },
            allowed_classes: &[Sword, Dagger, Bow, Staff, Axe],
        },
        21..=30 => TierConfig {
            id: 3, level_min: 21, level_max: 30,
            theme: TierTheme {
                display_name: "Human Bandit",
                visual_class: "warrior",
                body_tint: [0.7, 0.55, 0.55, 1.0],   // tom escuro avermelhado
                size_scale: 1.0,
            },
            allowed_classes: &[Sword, Bow, Axe, Spear],
        },
        31..=40 => TierConfig {
            id: 4, level_min: 31, level_max: 40,
            theme: TierTheme {
                display_name: "Gray Goblin",
                visual_class: "warrior",
                body_tint: [0.6, 0.6, 0.65, 1.0],
                size_scale: 1.0,
            },
            allowed_classes: &[Dagger, Axe, Staff, Spear],
        },
        41..=50 => TierConfig {
            id: 5, level_min: 41, level_max: 50,
            theme: TierTheme {
                display_name: "Lesser Green Demon",
                visual_class: "wizard",
                body_tint: [0.4, 0.85, 0.4, 1.0],
                size_scale: 1.0,
            },
            allowed_classes: &[Sword, Bow, Staff, Spear],
        },
        51..=60 => TierConfig {
            id: 6, level_min: 51, level_max: 60,
            theme: TierTheme {
                display_name: "Purple Demon",
                visual_class: "wizard",
                body_tint: [0.7, 0.4, 0.95, 1.0],
                size_scale: 1.0,
            },
            allowed_classes: &[Sword, Axe, Staff, Dagger],
        },
        61..=70 => TierConfig {
            id: 7, level_min: 61, level_max: 70,
            theme: TierTheme {
                display_name: "Greater Red Demon",
                visual_class: "warrior",
                body_tint: [1.0, 0.4, 0.35, 1.0],
                size_scale: 1.0,
            },
            allowed_classes: &[GreatSword, Axe, Staff],
        },
        71..=80 => TierConfig {
            id: 8, level_min: 71, level_max: 80,
            theme: TierTheme {
                display_name: "Radiant Human Elite",
                visual_class: "warrior",
                body_tint: [1.05, 1.0, 0.7, 1.0],   // brilho dourado leve
                size_scale: 1.0,
            },
            allowed_classes: &[Sword, Bow, Staff, Dagger, Spear, Axe],
        },
        81..=90 => TierConfig {
            id: 9, level_min: 81, level_max: 90,
            theme: TierTheme {
                display_name: "Golden Demon",
                visual_class: "warrior",
                body_tint: [1.2, 1.05, 0.45, 1.0],
                size_scale: 1.0,
            },
            allowed_classes: &[GreatSword, Staff, Bow, SwordShield],
        },
        _ /* 91-100 */ => TierConfig {
            id: 10, level_min: 91, level_max: 100,
            theme: TierTheme {
                display_name: "World Boss Tier",
                visual_class: "warrior",
                body_tint: [0.95, 0.85, 1.0, 1.0],   // brilho azulado celestial
                size_scale: 1.0,
            },
            allowed_classes: &[GreatSword, Staff, SwordShield],
        },
    }
}

// ── Equipment progression por classe ────────────────────────────────────

fn equip_for_class_level(class: EnemyClass, level: u32) -> Equipment {
    use shared::item_id::*;
    let mut e = Equipment::default();
    let weapon = match class {
        EnemyClass::Sword       => if level <= 30 { SWORD } else { SCIMITAR },
        EnemyClass::Dagger      => DAGGER,
        EnemyClass::Bow         => BOW,
        EnemyClass::Staff       => if level <= 30 { WAND } else { STAFF },
        EnemyClass::Wand        => WAND,
        EnemyClass::Axe         => AXE,
        EnemyClass::Spear       => SPEAR,
        EnemyClass::GreatSword  => GREAT_SWORD,
        EnemyClass::SwordShield => if level <= 40 { SWORD } else { GREAT_SWORD },
    };
    e.set(EquipSlot::Weapon, Some(weapon), None);

    if matches!(class, EnemyClass::SwordShield) {
        let off = if level <= 40 { SHIELD } else { HEAVY_SHIELD };
        e.set(EquipSlot::Offhand, Some(off), None);
    }

    // Armadura por level.
    let armor = match class {
        EnemyClass::Staff | EnemyClass::Wand => Some(ROBE),
        _ if level >= 60 => Some(PLATE_ARMOR),
        _ if level >= 30 => Some(LEATHER_ARMOR),
        _ => None,
    };
    if let Some(a) = armor {
        e.set(EquipSlot::Armor, Some(a), None);
    }
    e
}

// ── Stat scaling ────────────────────────────────────────────────────────

/// Aloca pontos baseado no level (escalonamento). Tier 8+ (humanos elite)
/// ganha 50% mais pontos pra ser threat real.
fn allocated_for_level(class: EnemyClass, level: u32) -> [u32; STAT_COUNT] {
    let level_mult = if level >= 71 { 1.5 } else { 1.0 };
    // Hardcore: 5 pontos/level (era 3) — mobs significativamente mais fortes
    // pra que XP curva 5x lenta nao deixe player sub-leveled vs mobs.
    let total = ((level as f32) * 5.0 * level_mult) as u32;
    // Distribuicao por classe — emula prioridade de cada arquetipo.
    // Indices: [FOR=0, DES=1, INT=2, VIT=3, SPD=4, RES=5]
    let weights = match class {
        EnemyClass::Sword       => [0.30, 0.10, 0.0, 0.40, 0.05, 0.15],
        EnemyClass::Dagger      => [0.15, 0.40, 0.0, 0.20, 0.20, 0.05],
        EnemyClass::Bow         => [0.10, 0.45, 0.0, 0.30, 0.10, 0.05],
        EnemyClass::Staff       => [0.0, 0.05, 0.55, 0.30, 0.0, 0.10],
        EnemyClass::Wand        => [0.0, 0.10, 0.50, 0.30, 0.0, 0.10],
        EnemyClass::Axe         => [0.45, 0.05, 0.0, 0.35, 0.0, 0.15],
        EnemyClass::Spear       => [0.30, 0.20, 0.0, 0.30, 0.10, 0.10],
        EnemyClass::GreatSword  => [0.50, 0.05, 0.0, 0.30, 0.0, 0.15],
        EnemyClass::SwordShield => [0.20, 0.05, 0.0, 0.45, 0.0, 0.30],
    };
    let mut out = [0u32; STAT_COUNT];
    for i in 0..STAT_COUNT {
        out[i] = (total as f32 * weights[i]) as u32;
    }
    out
}

// ── Skills aprendidas por tier ──────────────────────────────────────────

fn learned_for_tier_class(tier: u8, class: EnemyClass) -> Vec<LearnedSkill> {
    // Skills basicas por classe + escalonamento de rank por tier.
    // tier 1-3: 0 skills
    // tier 4-5: 1 skill rank 1-2
    // tier 6-7: 2 skills rank 2-3
    // tier 8: 3 skills rank 3-5
    // tier 9-10: 4-5 skills rank 5-7
    let class_skills: &[u32] = match class {
        EnemyClass::Sword | EnemyClass::SwordShield  => &[1003, 1005, 1008, 1006],
        EnemyClass::Dagger      => &[1003, 1005, 1006],
        EnemyClass::Bow         => &[1033, 1003],
        EnemyClass::Staff | EnemyClass::Wand => &[1045, 1046, 1054],
        EnemyClass::Axe         => &[1003, 1006, 1008],
        EnemyClass::Spear       => &[1003, 1006],
        EnemyClass::GreatSword  => &[1005, 1003, 1008],
    };
    let (n, base_rank) = match tier {
        1..=3 => (0usize, 0u8),
        4..=5 => (1, 1),
        6..=7 => (2, 2),
        8     => (3, 3),
        9     => (4, 5),
        _     => (5, 7),
    };
    let n = n.min(class_skills.len());
    let mut out = Vec::with_capacity(n);
    for (i, &sid) in class_skills.iter().take(n).enumerate() {
        let rank = (base_rank + i as u8).min(10).max(1);
        out.push(LearnedSkill { skill_id: sid, rank, equipped_slot: Some(i as u8) });
    }
    out
}

// ── Visual ──────────────────────────────────────────────────────────────

/// Pool de outfits/cores aleatorios — espelha os codigos disponiveis em
/// Resources/Character (cliente PaperDoll). Cada spawn sortea um pra ter
/// variedade visual entre mobs do mesmo tier.
const OUTFIT_POOL: &[&str] = &["fstr", "pfpn", "bksm", "angl", "pfdr", "alch"];

/// Counter global incrementado a cada build pra dar seed unica de outfit.
static OUTFIT_SEED_COUNTER: std::sync::atomic::AtomicU64 =
    std::sync::atomic::AtomicU64::new(1);

fn random_outfit() -> (String, u8) {
    let n = OUTFIT_SEED_COUNTER
        .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let s = n.wrapping_mul(0x9E37_79B9_7F4A_7C15);
    let outfit = OUTFIT_POOL[(s as usize) % OUTFIT_POOL.len()].to_string();
    let color = ((s >> 32) % 5) as u8 + 1; // 1..=5
    (outfit, color)
}

fn visual_for_tier(theme: &TierTheme) -> VisualConfig {
    let mut v = VisualConfig::for_class(theme.visual_class);
    v.body_tint = Some(theme.body_tint);
    // TODOS os enemies: sempre carecas (sem cabelo). Cliente trata "" como
    // null sprite (PaperDoll.string.IsNullOrEmpty).
    v.hair = Some(String::new());
    // Outfit aleatorio pra variedade visual entre mobs do mesmo tier.
    let (outfit, color) = random_outfit();
    v.outfit = Some(outfit);
    v.outfit_color = Some(color);
    v
}

// ── EnemyBuild ──────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct EnemyBuild {
    pub kind: u16,
    pub name: String,
    pub level: u32,
    pub class: EnemyClass,
    pub equipment: Equipment,
    pub allocated_points: [u32; STAT_COUNT],
    pub proficiencies: [u64; PROF_COUNT],
    pub learned_skills: Vec<LearnedSkill>,
    pub visual: VisualConfig,
    // AI / combat behaviors (cacheados pro tick loop)
    pub attack_cooldown: f32,
    pub attack_range: f32,
    pub detect_range: f32,
    pub locomotor_speed: f32,
    pub kite_dist: Option<f32>,
    pub proj_count: u32,
    pub proj_kind: u8,
    pub size_scale: f32,
    pub xp_reward: u64,
    pub is_melee: bool,
    pub is_boss: bool,
}

impl EnemyBuild {
    pub fn weapon_id(&self) -> u16 {
        self.equipment.weapon.unwrap_or(0)
    }
}

/// Gera build proceduralmente pra (level, class). Stats/equip/skills/visual
/// sao todos derivados da combinacao.
pub fn build_for_level(level: u32, class: EnemyClass) -> EnemyBuild {
    let level = level.max(1).min(100);
    let tier = tier_for_level(level);
    let equipment = equip_for_class_level(class, level);
    let allocated_points = allocated_for_level(class, level);
    let learned_skills = learned_for_tier_class(tier.id, class);
    let visual = visual_for_tier(&tier.theme);
    EnemyBuild {
        kind: level as u16,   // sem mais distincao por kind id; level vira o "id"
        name: format!("{} Lv{}", tier.theme.display_name, level),
        level,
        class,
        equipment,
        allocated_points,
        proficiencies: [0u64; PROF_COUNT],
        learned_skills,
        visual,
        attack_cooldown: class.base_cooldown(),
        attack_range: class.attack_range(),
        detect_range: class.detect_range(),
        locomotor_speed: class.base_speed(),
        kite_dist: class.kite_dist(),
        proj_count: 1,
        proj_kind: class.proj_kind(),
        size_scale: tier.theme.size_scale,
        xp_reward: (level as u64) * 12 + 20,
        is_melee: class.is_melee(),
        is_boss: false,
    }
}

/// Boss build — alvo: ~10-15x mais forte que mob comum do mesmo level.
/// HP via VIT × 12, dano via stat primario × 4, def via RES × 3 + 30.
/// Kit completo de skills da classe (4-5 skills rank 8-10), inclusive
/// 1003 Shield Bash garantido (boss faz melee mesmo ranged).
pub fn build_boss(level: u32, class: EnemyClass) -> EnemyBuild {
    let mut b = build_for_level(level, class);

    // HP × ~12 (VIT × 12)
    b.allocated_points[3] = b.allocated_points[3].saturating_mul(12).max(60);
    // Dano × ~4 — stat primario por classe
    // Indices: [FOR=0, DES=1, INT=2, VIT=3, SPD=4, RES=5]
    let dmg_idx = match class {
        EnemyClass::Staff | EnemyClass::Wand => 2, // INT
        EnemyClass::Bow | EnemyClass::Dagger  => 1, // DES
        _                                     => 0, // FOR
    };
    b.allocated_points[dmg_idx] = b.allocated_points[dmg_idx].saturating_mul(4).max(40);
    // Defesa × 3 + flat 30
    b.allocated_points[5] = b.allocated_points[5].saturating_mul(3).saturating_add(30);

    // Kit completo de skills (sempre tier 10): inclui 1003 Shield Bash pra
    // garantir melee mesmo em classes ranged (Bow/Staff/Wand).
    let class_skills: &[u32] = match class {
        EnemyClass::Sword | EnemyClass::SwordShield  => &[1003, 1005, 1008, 1006],
        EnemyClass::Dagger      => &[1003, 1005, 1006, 1038],
        EnemyClass::Bow         => &[1033, 1003, 1038],
        EnemyClass::Staff | EnemyClass::Wand => &[1045, 1046, 1054, 1003],
        EnemyClass::Axe         => &[1003, 1006, 1008],
        EnemyClass::Spear       => &[1003, 1006, 1008],
        EnemyClass::GreatSword  => &[1005, 1003, 1008],
    };
    b.learned_skills = class_skills.iter().enumerate()
        .map(|(i, &sid)| LearnedSkill {
            skill_id: sid,
            rank: (8 + (i as u8 % 3)).min(10), // rank 8-10
            equipped_slot: Some(i as u8),
        })
        .collect();

    b.size_scale = 2.0;
    b.xp_reward = (level as u64) * 200 + 500;
    b.is_boss = true;
    b.name = format!("[BOSS] {}", b.name);
    b
}

/// Sortea uma classe permitida pelo tier do level. Pesos: melee 4×, ranged 1×
/// — bias significativo pra melee (~75% spawns) pra fechar a distancia faster.
pub fn random_class_for_level(level: u32, rng_seed: u64) -> EnemyClass {
    let tier = tier_for_level(level);
    let pool = tier.allowed_classes;
    let weight = |c: EnemyClass| -> u32 {
        match c {
            EnemyClass::Bow | EnemyClass::Staff | EnemyClass::Wand => 1,
            _ => 4,
        }
    };
    let total: u32 = pool.iter().map(|c| weight(*c)).sum();
    if total == 0 { return pool[(rng_seed as usize) % pool.len()]; }
    let mut r = (rng_seed as u32) % total;
    for &c in pool.iter() {
        let w = weight(c);
        if r < w { return c; }
        r -= w;
    }
    pool[0]
}

/// Top-level: sortea level no range + classe permitida + retorna build.
pub fn random_build_for_level_range(
    level_min: u32,
    level_max: u32,
    seed_level: u64,
    seed_class: u64,
) -> EnemyBuild {
    let level = if level_max > level_min {
        level_min + (seed_level as u32 % (level_max - level_min + 1))
    } else { level_min };
    let class = random_class_for_level(level, seed_class);
    build_for_level(level, class)
}

// ── Legacy compat: kinds 0-7 antigos via aproximacao ────────────────────

/// Mantem assinatura antiga `enemy_build(kind: u16)` retornando builds
/// equivalentes. Usado pra zonas legacy (sem level_range) e mapas antigos.
pub fn enemy_build(kind: u16) -> EnemyBuild {
    match kind {
        0 => build_for_level(3,  EnemyClass::Sword),
        1 => build_for_level(5,  EnemyClass::SwordShield),
        2 => build_for_level(4,  EnemyClass::Bow),
        3 => build_for_level(4,  EnemyClass::Dagger),
        4 => build_for_level(5,  EnemyClass::Staff),
        5 => build_for_level(7,  EnemyClass::GreatSword),
        6 => build_for_level(4,  EnemyClass::Bow),
        7 => build_boss(15,      EnemyClass::SwordShield),
        _ => build_for_level(3,  EnemyClass::Sword),
    }
}

/// Pra tick_spawn_zones legacy (level-range). Mantem assinatura compat.
pub fn random_kind_for_level(level: u32, rng_seed: u64) -> u16 {
    // Retorna o LEVEL como kind id pra que EntityKind::Enemy(level) carregue
    // o build via build_for_level no spawn. Pode haver multiplas calls — o
    // build real usa esse level via spawn helper.
    level as u16
}
