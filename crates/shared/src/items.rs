//! Sistema de itemização — Fase A.
//!
//! Modelo:
//!  - `ItemTemplate`: definição estática por item_id (ranges de stats).
//!  - `ItemInstance`: instância única gerada no drop, com stats RoLAdos
//!    + rarity + refinement.
//!  - `InventorySlot.instance`: Option<ItemInstance> — quando None, item
//!    usa stats base (compat com itens antigos pre-Fase A).
//!
//! Stats finais = base do item_id + rolls da instance × multiplier de
//! rarity × (1 + refinement × REFINE_BOOST_PER_LEVEL).
//!
//! Sincronizar: ItemInstance é serializada como JSONB na coluna
//! `inventory.instance_data` e replicada via wire pro cliente em
//! `InventorySlot.instance`.

use serde::{Deserialize, Serialize};

/// Tier de raridade. Cada tier multiplica os rolls por um fator e tem
/// chance de drop diferente.
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ItemRarity {
    Common    = 0, // cinza      — 60% drop
    Magic     = 1, // azul       — 25%
    Rare      = 2, // amarelo    — 10%
    Epic      = 3, // roxo       — 4%
    Legendary = 4, // laranja    — 1%
}

impl ItemRarity {
    /// Multiplier aplicado nos rolls de stats.
    pub fn stat_mult(self) -> f32 {
        match self {
            ItemRarity::Common    => 0.6,
            ItemRarity::Magic     => 0.9,
            ItemRarity::Rare      => 1.2,
            ItemRarity::Epic      => 1.5,
            ItemRarity::Legendary => 1.9,
        }
    }

    /// Cor RGB pra cliente (#RRGGBB).
    pub fn color_hex(self) -> &'static str {
        match self {
            ItemRarity::Common    => "#bfbfbf",
            ItemRarity::Magic     => "#5577ff",
            ItemRarity::Rare      => "#ffd84d",
            ItemRarity::Epic      => "#aa55ff",
            ItemRarity::Legendary => "#ff7733",
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            ItemRarity::Common    => "Comum",
            ItemRarity::Magic     => "Mágico",
            ItemRarity::Rare      => "Raro",
            ItemRarity::Epic      => "Épico",
            ItemRarity::Legendary => "Lendário",
        }
    }

    pub fn from_u8(v: u8) -> Self {
        match v {
            1 => ItemRarity::Magic,
            2 => ItemRarity::Rare,
            3 => ItemRarity::Epic,
            4 => ItemRarity::Legendary,
            _ => ItemRarity::Common,
        }
    }

    /// Roll uma rarity baseada num float [0,1).
    pub fn roll(r: f32) -> Self {
        // Tabela acumulada: Common 60, Magic 85, Rare 95, Epic 99, Leg 100.
        let p = (r * 100.0) as i32;
        if p < 60 { ItemRarity::Common }
        else if p < 85 { ItemRarity::Magic }
        else if p < 95 { ItemRarity::Rare }
        else if p < 99 { ItemRarity::Epic }
        else { ItemRarity::Legendary }
    }
}

/// Range de cada stat no template (min..=max inteiros, antes de
/// aplicar rarity mult).
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
pub struct StatRange {
    pub min: i32,
    pub max: i32,
}

impl StatRange {
    pub const fn new(min: i32, max: i32) -> Self { Self { min, max } }
    pub const fn zero() -> Self { Self { min: 0, max: 0 } }
    pub fn is_zero(&self) -> bool { self.min == 0 && self.max == 0 }

    /// Roll dentro do range. Retorna inteiro arredondado.
    pub fn roll(&self, r: f32, rarity_mult: f32) -> i32 {
        if self.is_zero() { return 0; }
        let lo = self.min as f32;
        let hi = self.max as f32;
        let raw = lo + r * (hi - lo); // [min, max)
        (raw * rarity_mult).round() as i32
    }
}

/// Template — ranges de stat por item_id. Cada item equipável tem o seu.
/// Não-equipáveis (gold, potions, materiais) não têm template e não geram
/// instances no drop.
#[derive(Debug, Clone, Copy, Default)]
pub struct ItemTemplate {
    pub hp_max:        StatRange,
    pub mp_max:        StatRange,
    pub attack_damage: StatRange,
    pub dex:           StatRange,
    pub wis:           StatRange,
    pub defense:       StatRange,
}

impl ItemTemplate {
    pub fn has_any_range(&self) -> bool {
        !(self.hp_max.is_zero() && self.mp_max.is_zero()
            && self.attack_damage.is_zero() && self.dex.is_zero()
            && self.wis.is_zero() && self.defense.is_zero())
    }
}

/// Lookup do template por item_id. Item não-equipável retorna template
/// vazio (sem ranges → drop não gera instance).
pub fn item_template(item_id: u16) -> ItemTemplate {
    use crate::constants::item_id::*;
    match item_id {
        // === Armas ===
        SWORD        => ItemTemplate { attack_damage: StatRange::new(8, 16),  ..Default::default() },
        DAGGER       => ItemTemplate { attack_damage: StatRange::new(5, 12),  dex: StatRange::new(6, 14), ..Default::default() },
        GREAT_SWORD  => ItemTemplate { attack_damage: StatRange::new(20, 36), ..Default::default() },
        BOW          => ItemTemplate { attack_damage: StatRange::new(10, 20), dex: StatRange::new(8, 16), ..Default::default() },
        STAFF        => ItemTemplate { attack_damage: StatRange::new(14, 26), mp_max: StatRange::new(20, 60),  wis: StatRange::new(3, 8), ..Default::default() },
        WAND         => ItemTemplate { attack_damage: StatRange::new(4, 10),  mp_max: StatRange::new(50, 110), wis: StatRange::new(5, 12), ..Default::default() },
        // === Armaduras ===
        ARMOR        => ItemTemplate { hp_max: StatRange::new(25, 60),  defense: StatRange::new(3, 8),  ..Default::default() },
        SHIELD       => ItemTemplate { hp_max: StatRange::new(50, 100), defense: StatRange::new(5, 12), ..Default::default() },
        LEATHER_ARMOR=> ItemTemplate { hp_max: StatRange::new(15, 35),  defense: StatRange::new(1, 5),  dex: StatRange::new(3, 9), ..Default::default() },
        PLATE_ARMOR  => ItemTemplate { hp_max: StatRange::new(80, 160), defense: StatRange::new(8, 16), ..Default::default() },
        ROBE         => ItemTemplate { hp_max: StatRange::new(5, 15),   mp_max: StatRange::new(30, 90), defense: StatRange::new(1, 4), wis: StatRange::new(4, 12), ..Default::default() },
        // === Acessórios ===
        RING         => ItemTemplate { dex: StatRange::new(2, 8),   wis: StatRange::new(1, 5), ..Default::default() },
        AMULET       => ItemTemplate { hp_max: StatRange::new(8, 25), mp_max: StatRange::new(15, 45), wis: StatRange::new(4, 12), defense: StatRange::new(0, 3), ..Default::default() },
        LUCKY_RING   => ItemTemplate { hp_max: StatRange::new(5, 18), mp_max: StatRange::new(10, 30), attack_damage: StatRange::new(1, 4), dex: StatRange::new(3, 9), wis: StatRange::new(1, 4), ..Default::default() },
        // === Fase D — novos ===
        SCIMITAR     => ItemTemplate { attack_damage: StatRange::new(6, 14),  dex: StatRange::new(4, 10), ..Default::default() },
        AXE          => ItemTemplate { attack_damage: StatRange::new(16, 32), hp_max: StatRange::new(15, 35), defense: StatRange::new(2, 6), ..Default::default() },
        SPEAR        => ItemTemplate { attack_damage: StatRange::new(12, 22), dex: StatRange::new(3, 9), ..Default::default() },
        CROSSBOW     => ItemTemplate { attack_damage: StatRange::new(14, 24), dex: StatRange::new(6, 12), ..Default::default() },
        HEAVY_SHIELD => ItemTemplate { hp_max: StatRange::new(70, 140), defense: StatRange::new(10, 20), ..Default::default() },
        PENDANT      => ItemTemplate { hp_max: StatRange::new(15, 35), mp_max: StatRange::new(20, 50), wis: StatRange::new(2, 6), defense: StatRange::new(0, 2), ..Default::default() },
        CHARM        => ItemTemplate { mp_max: StatRange::new(5, 20), attack_damage: StatRange::new(2, 6), dex: StatRange::new(2, 6), wis: StatRange::new(2, 6), ..Default::default() },
        // === Fase E — slots novos ===
        HELM_LEATHER  => ItemTemplate { hp_max: StatRange::new(8, 22),   defense: StatRange::new(1, 5),  dex: StatRange::new(2, 6), ..Default::default() },
        HELM_PLATE    => ItemTemplate { hp_max: StatRange::new(30, 60),  defense: StatRange::new(4, 10), ..Default::default() },
        LEGS_LEATHER  => ItemTemplate { hp_max: StatRange::new(12, 28),  defense: StatRange::new(1, 5),  dex: StatRange::new(3, 7), ..Default::default() },
        LEGS_PLATE    => ItemTemplate { hp_max: StatRange::new(40, 80),  defense: StatRange::new(6, 12), ..Default::default() },
        BOOTS_LEATHER => ItemTemplate { hp_max: StatRange::new(5, 15),   defense: StatRange::new(0, 3),  dex: StatRange::new(4, 8), ..Default::default() },
        BOOTS_PLATE   => ItemTemplate { hp_max: StatRange::new(20, 40),  defense: StatRange::new(3, 7), ..Default::default() },
        GLOVES_LEATHER=> ItemTemplate { attack_damage: StatRange::new(1, 5), defense: StatRange::new(0, 2), dex: StatRange::new(3, 7), ..Default::default() },
        GLOVES_PLATE  => ItemTemplate { hp_max: StatRange::new(12, 28),  attack_damage: StatRange::new(3, 8), defense: StatRange::new(2, 6), ..Default::default() },
        BELT_BASIC    => ItemTemplate { hp_max: StatRange::new(15, 35),  defense: StatRange::new(0, 3), ..Default::default() },
        BELT_MAGIC    => ItemTemplate { mp_max: StatRange::new(20, 50),  wis: StatRange::new(2, 6), ..Default::default() },
        CAPE_BASIC    => ItemTemplate { hp_max: StatRange::new(12, 28),  defense: StatRange::new(2, 6), ..Default::default() },
        CAPE_MAGIC    => ItemTemplate { mp_max: StatRange::new(25, 60),  wis: StatRange::new(3, 9), defense: StatRange::new(0, 3), ..Default::default() },
        NECKLACE_BASIC=> ItemTemplate { hp_max: StatRange::new(12, 28),  mp_max: StatRange::new(5, 15),  wis: StatRange::new(2, 5), ..Default::default() },
        NECKLACE_MAGIC=> ItemTemplate { mp_max: StatRange::new(25, 55),  wis: StatRange::new(4, 10), ..Default::default() },
        _ => ItemTemplate::default(),
    }
}

/// Instância única dropada. Substitui o uso de `item_bonus(id)` (estático)
/// pra itens que tenham instance — ItemBonus base ainda é fallback pra
/// itens sem instance (legacy).
///
/// Refinement: +0 a `MAX_REFINE`. Cada nível adiciona
/// `REFINE_BOOST_PER_LEVEL` × stats. NPC vendor (futuro) gasta ouro pra
/// upgrade; chance de falha cresce com o nível.
/// Maximo de affixes por instance (Magic=1, Rare=2, Epic=3, Legendary=4).
pub const MAX_AFFIXES: usize = 4;

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct ItemInstance {
    pub rarity:     u8,
    pub refinement: u8,
    /// Item Level — vem do enemy que dropou. Escala stats no roll
    /// (Fase C). Default 1 pra drops antigos.
    #[serde(default = "default_ilvl")]
    pub item_level: u16,
    /// Level mínimo do player pra equipar. None = sem requirement.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub level_req:  Option<u16>,
    pub hp_max:        i32,
    pub mp_max:        i32,
    pub attack_damage: i32,
    pub dex:           i32,
    pub wis:           i32,
    pub defense:       i32,
    /// Affixes (Fase B). Magic=1, Rare=2, Epic=3, Legendary=4. Posicoes
    /// vazias têm name_id=0.
    #[serde(default)]
    pub affixes: [AffixSlot; MAX_AFFIXES],
    /// Sockets disponíveis (vem da rarity). 0..3.
    #[serde(default)]
    pub sockets: u8,
    /// Gemas inseridas nos sockets (item_id da gema, 0 = vazio).
    /// Aplicado em ordem: socketed_gems[0] vai pro 1° socket, etc.
    #[serde(default)]
    pub socketed_gems: [u16; 3],
}

fn default_ilvl() -> u16 { 1 }

/// Slot de affix — name_id=0 = vazio.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
pub struct AffixSlot {
    pub name_id:    u16,  // 0 = vazio
    pub stat:       u8,   // AffixStat as u8
    pub tier:       u8,
    pub value:      i32,
    pub value_pct:  f32,
    pub is_prefix:  bool,
}

impl AffixSlot {
    pub fn from_affix(a: Affix) -> Self {
        AffixSlot {
            name_id: a.name_id,
            stat: a.stat as u8,
            tier: a.tier,
            value: a.value,
            value_pct: a.value_pct,
            is_prefix: a.is_prefix,
        }
    }
    pub fn is_empty(&self) -> bool { self.name_id == 0 }

    pub fn stat(&self) -> AffixStat {
        match self.stat {
            1 => AffixStat::Mp,
            2 => AffixStat::Attack,
            3 => AffixStat::Defense,
            4 => AffixStat::Dex,
            5 => AffixStat::Wis,
            6 => AffixStat::CritChance,
            7 => AffixStat::AttackSpeed,
            8 => AffixStat::MoveSpeed,
            9 => AffixStat::HpRegen,
            _ => AffixStat::Hp,
        }
    }
}

pub const MAX_REFINE: u8 = 15;
pub const REFINE_BOOST_PER_LEVEL: f32 = 0.05; // +5% por nível

// ── Fase B: Affix system ────────────────────────────────────────────────
// Cada item Magic+ tem affixes que adicionam stats extras além dos rolls
// base. Prefix = palavra antes do nome (ex: "Strong"), Suffix = palavra
// depois (ex: "of the Bear"). Magic=1 affix, Rare=2, Epic=3, Legendary=4.

/// Tipo do stat afetado pelo affix.
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AffixStat {
    Hp = 0, Mp = 1, Attack = 2, Defense = 3, Dex = 4, Wis = 5,
    /// % crit_chance flat add (ex: +0.02 = +2%).
    CritChance = 6,
    /// % attack_speed_mult flat add (ex: +0.10 = +10% atk speed).
    AttackSpeed = 7,
    /// % move_speed_mult flat add.
    MoveSpeed = 8,
    /// HP regen flat add.
    HpRegen = 9,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct Affix {
    pub stat: AffixStat,
    pub value: i32,        // pra stats inteiros (Hp/Mp/Atk/Def/Dex/Wis)
    pub value_pct: f32,    // pra % stats (Crit/AtkSpd/MoveSpd/HpRegen)
    pub tier: u8,          // 1..3 (T1=fraco, T3=forte)
    pub is_prefix: bool,
    /// Index na affix table — usado pra recuperar o nome em runtime.
    pub name_id: u16,
}

/// Tabela de affix. Cada entrada define um possível roll. ranges são
/// (min, max) por tier (T1/T2/T3). Affixes ID < 1000 são prefixes.
pub struct AffixDef {
    pub name_id:   u16,
    pub display:   &'static str,
    pub stat:      AffixStat,
    pub is_prefix: bool,
    /// Range (min, max) por tier (T1, T2, T3).
    pub tiers:     [(i32, i32); 3],
    /// Se for % stat, multiplicador pra dividir os tiers (caso seja x0.001 etc).
    pub pct_div:   f32,
}

pub const AFFIX_TABLE: &[AffixDef] = &[
    // ── Prefixes (name_id 1..999) ──
    AffixDef { name_id: 1,  display: "Forte",     stat: AffixStat::Attack,  is_prefix: true,
               tiers: [(2,5), (5,10), (10,18)],   pct_div: 0.0 },
    AffixDef { name_id: 2,  display: "Brutal",    stat: AffixStat::Attack,  is_prefix: true,
               tiers: [(8,15), (15,25), (25,40)], pct_div: 0.0 },
    AffixDef { name_id: 3,  display: "Resistente",stat: AffixStat::Hp,      is_prefix: true,
               tiers: [(10,25), (25,50), (50,90)],pct_div: 0.0 },
    AffixDef { name_id: 4,  display: "Fortificado",stat: AffixStat::Defense,is_prefix: true,
               tiers: [(2,5), (5,10), (10,18)],   pct_div: 0.0 },
    AffixDef { name_id: 5,  display: "Mágico",    stat: AffixStat::Mp,      is_prefix: true,
               tiers: [(15,30), (30,60), (60,110)], pct_div: 0.0 },
    AffixDef { name_id: 6,  display: "Ágil",      stat: AffixStat::Dex,     is_prefix: true,
               tiers: [(2,5), (5,9), (9,15)],     pct_div: 0.0 },
    AffixDef { name_id: 7,  display: "Sábio",     stat: AffixStat::Wis,     is_prefix: true,
               tiers: [(2,5), (5,9), (9,15)],     pct_div: 0.0 },
    AffixDef { name_id: 8,  display: "Implacável",stat: AffixStat::AttackSpeed, is_prefix: true,
               tiers: [(3,7), (7,12), (12,20)],   pct_div: 100.0 }, // %
    AffixDef { name_id: 9,  display: "Crítico",   stat: AffixStat::CritChance,  is_prefix: true,
               tiers: [(2,5), (5,8), (8,15)],     pct_div: 100.0 }, // %
    AffixDef { name_id: 10, display: "Veloz",     stat: AffixStat::MoveSpeed,   is_prefix: true,
               tiers: [(2,5), (5,8), (8,12)],     pct_div: 100.0 }, // %
    // ── Suffixes (name_id 1000..1999) ──
    AffixDef { name_id: 1001, display: "do Urso",     stat: AffixStat::Hp,      is_prefix: false,
               tiers: [(15,30), (30,60), (60,100)], pct_div: 0.0 },
    AffixDef { name_id: 1002, display: "do Touro",    stat: AffixStat::Attack,  is_prefix: false,
               tiers: [(3,7), (7,13), (13,22)],   pct_div: 0.0 },
    AffixDef { name_id: 1003, display: "da Tartaruga",stat: AffixStat::Defense, is_prefix: false,
               tiers: [(3,6), (6,11), (11,18)],   pct_div: 0.0 },
    AffixDef { name_id: 1004, display: "do Lince",    stat: AffixStat::Dex,     is_prefix: false,
               tiers: [(3,7), (7,11), (11,17)],   pct_div: 0.0 },
    AffixDef { name_id: 1005, display: "do Mago",     stat: AffixStat::Wis,     is_prefix: false,
               tiers: [(3,7), (7,11), (11,17)],   pct_div: 0.0 },
    AffixDef { name_id: 1006, display: "do Vento",    stat: AffixStat::MoveSpeed,is_prefix: false,
               tiers: [(2,5), (5,8), (8,12)],     pct_div: 100.0 },
    AffixDef { name_id: 1007, display: "da Fúria",    stat: AffixStat::AttackSpeed, is_prefix: false,
               tiers: [(3,7), (7,12), (12,20)],   pct_div: 100.0 },
    AffixDef { name_id: 1008, display: "do Assassino",stat: AffixStat::CritChance, is_prefix: false,
               tiers: [(2,5), (5,8), (8,15)],     pct_div: 100.0 },
    AffixDef { name_id: 1009, display: "do Manancial",stat: AffixStat::Mp,      is_prefix: false,
               tiers: [(15,40), (40,80), (80,140)],pct_div: 0.0 },
    AffixDef { name_id: 1010, display: "da Regeneração",stat: AffixStat::HpRegen, is_prefix: false,
               tiers: [(1,3), (3,5), (5,9)],      pct_div: 10.0 }, // 0.1/0.3/0.5/0.9 hp/s
];

pub fn affix_def(name_id: u16) -> Option<&'static AffixDef> {
    AFFIX_TABLE.iter().find(|a| a.name_id == name_id)
}

impl Affix {
    pub fn roll<F: FnMut() -> f32>(rng: &mut F, is_prefix: bool, rarity: ItemRarity) -> Option<Self> {
        let pool: Vec<&AffixDef> = AFFIX_TABLE.iter().filter(|a| a.is_prefix == is_prefix).collect();
        if pool.is_empty() { return None; }
        let pick = &pool[(rng() * pool.len() as f32) as usize];
        // Tier weighted by rarity: Magic→T1, Rare→T1/T2, Epic→T2/T3, Leg→T3
        let tier_idx = match rarity {
            ItemRarity::Common    => 0,
            ItemRarity::Magic     => 0,
            ItemRarity::Rare      => if rng() < 0.5 { 0 } else { 1 },
            ItemRarity::Epic      => if rng() < 0.6 { 1 } else { 2 },
            ItemRarity::Legendary => if rng() < 0.3 { 1 } else { 2 },
        };
        let (lo, hi) = pick.tiers[tier_idx];
        let raw = lo as f32 + rng() * (hi - lo) as f32;
        if pick.pct_div > 0.0 {
            Some(Affix {
                stat: pick.stat,
                value: 0,
                value_pct: raw / pick.pct_div,
                tier: (tier_idx + 1) as u8,
                is_prefix,
                name_id: pick.name_id,
            })
        } else {
            Some(Affix {
                stat: pick.stat,
                value: raw.round() as i32,
                value_pct: 0.0,
                tier: (tier_idx + 1) as u8,
                is_prefix,
                name_id: pick.name_id,
            })
        }
    }
}

impl ItemInstance {
    /// Roll uma instance fresh pra um item_id usando lookup hardcoded
    /// (legacy). Prefira `roll_with_template` em código novo — esse aqui
    /// só sobrevive pra testes/tools que não tem acesso ao economy cache.
    pub fn roll_for<F: FnMut() -> f32>(item_id: u16, item_level: u16, rng: F) -> Option<Self> {
        Self::roll_with_template(item_template(item_id), item_level, rng)
    }

    /// Roll uma instance usando um template já obtido (do DB cache no server).
    /// `item_level` define o nível (boss=alto, mob comum=baixo). None se
    /// o template não tem nenhum range (item não-equipável).
    pub fn roll_with_template<F: FnMut() -> f32>(tpl: ItemTemplate, item_level: u16, mut rng: F) -> Option<Self> {
        if !tpl.has_any_range() { return None; }
        let rarity = ItemRarity::roll(rng());
        let mult = rarity.stat_mult() * ilvl_scale(item_level);
        let mut inst = ItemInstance {
            rarity:     rarity as u8,
            refinement: 0,
            item_level,
            level_req:  if item_level > 5 { Some(item_level / 2) } else { None },
            hp_max:        tpl.hp_max.roll(rng(), mult),
            mp_max:        tpl.mp_max.roll(rng(), mult),
            attack_damage: tpl.attack_damage.roll(rng(), mult),
            dex:           tpl.dex.roll(rng(), mult),
            wis:           tpl.wis.roll(rng(), mult),
            defense:       tpl.defense.roll(rng(), mult),
            affixes: [AffixSlot::default(); MAX_AFFIXES],
            sockets: sockets_for_rarity(rarity),
            socketed_gems: [0; 3],
        };
        // Affixes: Magic=1, Rare=2 (1pre+1suf), Epic=3 (2pre+1suf),
        // Legendary=4 (2pre+2suf). Common não tem.
        let (n_pre, n_suf) = match rarity {
            ItemRarity::Common    => (0, 0),
            ItemRarity::Magic     => (if rng() < 0.5 { 1 } else { 0 }, if rng() < 0.5 { 0 } else { 1 }),
            ItemRarity::Rare      => (1, 1),
            ItemRarity::Epic      => (2, 1),
            ItemRarity::Legendary => (2, 2),
        };
        let mut idx = 0;
        for _ in 0..n_pre {
            if let Some(a) = Affix::roll(&mut rng, true, rarity) {
                inst.affixes[idx] = AffixSlot::from_affix(a);
                idx += 1;
            }
        }
        for _ in 0..n_suf {
            if let Some(a) = Affix::roll(&mut rng, false, rarity) {
                inst.affixes[idx] = AffixSlot::from_affix(a);
                idx += 1;
            }
        }
        Some(inst)
    }

    /// Multiplier de refinamento (1.0 + refinement × 0.05).
    pub fn refine_mult(&self) -> f32 {
        1.0 + (self.refinement as f32) * REFINE_BOOST_PER_LEVEL
    }

    /// Bonus completo de stats (rolls + affixes inteiros) × refinement.
    /// Affixes flat (Hp/Mp/Atk/Def/Dex/Wis) entram aqui; affixes %
    /// (Crit/AtkSpd/MoveSpd/HpRegen) saem em `effective_pct_bonus`.
    pub fn effective_bonus(&self) -> crate::constants::EquipBonus {
        let m = self.refine_mult();
        let mut b = crate::constants::EquipBonus {
            hp_max:        (self.hp_max as f32 * m).round() as i32,
            mp_max:        (self.mp_max as f32 * m).round() as i32,
            attack_damage: (self.attack_damage as f32 * m).round() as i32,
            dex:           (self.dex as f32 * m).round() as i32,
            wis:           (self.wis as f32 * m).round() as i32,
            defense:       (self.defense as f32 * m).round() as i32,
        };
        // Affixes flat também recebem refinement.
        for a in &self.affixes {
            if a.is_empty() { continue; }
            let v = (a.value as f32 * m).round() as i32;
            match a.stat() {
                AffixStat::Hp       => b.hp_max += v,
                AffixStat::Mp       => b.mp_max += v,
                AffixStat::Attack   => b.attack_damage += v,
                AffixStat::Defense  => b.defense += v,
                AffixStat::Dex      => b.dex += v,
                AffixStat::Wis      => b.wis += v,
                _ => {}
            }
        }
        // Gemas socketed (não recebem refinement — bonus fixo).
        for &gid in &self.socketed_gems {
            if gid == 0 { continue; }
            if let Some((stat, val, _)) = gem_bonus(gid) {
                match stat {
                    AffixStat::Hp      => b.hp_max += val,
                    AffixStat::Mp      => b.mp_max += val,
                    AffixStat::Attack  => b.attack_damage += val,
                    AffixStat::Defense => b.defense += val,
                    AffixStat::Dex     => b.dex += val,
                    AffixStat::Wis     => b.wis += val,
                    _ => {}
                }
            }
        }
        b
    }

    /// Bonus de % stats dos affixes (nao recebem refinement — % são
    /// fixos como rolaram). Retorna tuple
    /// (crit_chance, atk_speed, move_speed, hp_regen).
    pub fn effective_pct_bonus(&self) -> (f32, f32, f32, f32) {
        let mut crit = 0.0; let mut atks = 0.0;
        let mut mov  = 0.0; let mut hpr  = 0.0;
        for a in &self.affixes {
            if a.is_empty() { continue; }
            match a.stat() {
                AffixStat::CritChance  => crit += a.value_pct,
                AffixStat::AttackSpeed => atks += a.value_pct,
                AffixStat::MoveSpeed   => mov  += a.value_pct,
                AffixStat::HpRegen     => hpr  += a.value_pct,
                _ => {}
            }
        }
        (crit, atks, mov, hpr)
    }

    pub fn rarity(&self) -> ItemRarity {
        ItemRarity::from_u8(self.rarity)
    }
}

/// Multiplier de stats baseado em item_level. iLvl 1 = 1.0× (baseline),
/// cresce ~1% por level. Fórmula: 1.0 + (ilvl - 1) × 0.015.
pub fn ilvl_scale(item_level: u16) -> f32 {
    1.0 + (item_level.saturating_sub(1) as f32) * 0.015
}

// ── Fase D: Item Sets ───────────────────────────────────────────────────
// Items pertencem a um set_id. Equipar N peças do mesmo set ativa
// `set_bonus(set_id, n)`. Não-stackable: cada peça única (anel + amuleto
// contam como 2 peças se ambos do set).

#[derive(Debug, Clone, Copy, Default)]
pub struct SetBonus {
    pub hp:       i32,
    pub mp:       i32,
    pub atk:      i32,
    pub def:      i32,
    pub crit:     f32,
    pub atk_spd:  f32,
}

/// Set ID por item_id. 0 = sem set.
pub fn item_set_id(item_id: u16) -> u8 {
    use crate::constants::item_id::*;
    match item_id {
        SWORD       | ARMOR         | RING       => 1, // Set do Aventureiro
        STAFF       | ROBE          | AMULET     => 2, // Set do Sábio
        DAGGER      | LEATHER_ARMOR | LUCKY_RING => 3, // Set do Ladino
        GREAT_SWORD | PLATE_ARMOR   | SHIELD     => 4, // Set do Bárbaro
        WAND        | BOW                        => 5, // Set do Caçador (só pra suffixos)
        _ => 0,
    }
}

pub fn set_name(set_id: u8) -> &'static str {
    match set_id {
        1 => "Conjunto do Aventureiro",
        2 => "Conjunto do Sábio",
        3 => "Conjunto do Ladino",
        4 => "Conjunto do Bárbaro",
        5 => "Conjunto do Caçador",
        _ => "",
    }
}

/// Quantidade total de peças no set.
pub fn set_total_pieces(set_id: u8) -> u8 {
    match set_id {
        1 | 2 | 3 => 3,
        4         => 3,
        5         => 2,
        _ => 0,
    }
}

/// Bonus aplicado quando `pieces` peças do `set_id` estão equipadas.
/// Cumulativo (set_bonus_for(2) inclui bonus de 2-piece, etc.) — caller
/// chama uma vez com a contagem total e usa o valor retornado.
pub fn set_bonus_for(set_id: u8, pieces: u8) -> SetBonus {
    if pieces < 2 { return SetBonus::default(); }
    match (set_id, pieces) {
        // Aventureiro: balanced
        (1, 2) => SetBonus { hp: 50, atk: 3, ..Default::default() },
        (1, 3) => SetBonus { hp: 120, atk: 8, def: 4, crit: 0.03, ..Default::default() },
        // Sábio: caster
        (2, 2) => SetBonus { mp: 60, atk: 4, ..Default::default() },
        (2, 3) => SetBonus { mp: 150, atk: 10, atk_spd: 0.10, ..Default::default() },
        // Ladino: dex
        (3, 2) => SetBonus { atk: 5, crit: 0.04, ..Default::default() },
        (3, 3) => SetBonus { atk: 12, crit: 0.10, atk_spd: 0.08, ..Default::default() },
        // Bárbaro: tanky DPS
        (4, 2) => SetBonus { hp: 80, atk: 8, def: 4, ..Default::default() },
        (4, 3) => SetBonus { hp: 200, atk: 18, def: 10, ..Default::default() },
        // Caçador: 2-piece only
        (5, 2) => SetBonus { atk: 6, atk_spd: 0.08, crit: 0.05, ..Default::default() },
        _ => SetBonus::default(),
    }
}

// ── Fase D: Sockets/Gems ────────────────────────────────────────────────
// Gems têm um stat fixo. Inserir gema num socket adiciona o stat.
// Socket count vem do template do item (tier rarity define max sockets).

pub fn gem_bonus(gem_id: u16) -> Option<(AffixStat, i32, f32)> {
    use crate::constants::item_id::*;
    match gem_id {
        GEM         => Some((AffixStat::Attack, 5, 0.0)),
        IRON_INGOT  => Some((AffixStat::Defense, 3, 0.0)),
        DRAGON_SCALE => Some((AffixStat::Hp, 40, 0.0)),
        _ => None,
    }
}

/// Quantos sockets um item tem baseado em sua rarity (Magic=0,
/// Rare=1, Epic=2, Legendary=3). Common não tem socket.
pub fn sockets_for_rarity(r: ItemRarity) -> u8 {
    match r {
        ItemRarity::Common    => 0,
        ItemRarity::Magic     => 0,
        ItemRarity::Rare      => 1,
        ItemRarity::Epic      => 2,
        ItemRarity::Legendary => 3,
    }
}
