//! THE SHIP (docs/NAVEGACAO.md, naval phases 1 and 2): pure numbers about
//! hulls, shared by client and server so the Captain's panel shows exactly
//! what the server will charge.
//!
//! The ship is an ITEM in its own equip slot (`EquipSlot::Navio`): one active
//! ship, no "which ship?" dialog at the dock. The item id is the hull class;
//! the hull points live in the instance (`ItemInstance::navio`), so trading
//! or selling a ship carries its state.
//!
//! At sea the HULL takes the damage, not the player (sea mobs gnaw it). At
//! zero the ship is damaged (`avariado`): the player is put back at the dock
//! they sailed from, and the ship cannot sail until the Harbour Captain
//! repairs it, for copper.
//!
//! PHASE 2, the ship as progression: the SHIPWRIGHT (the Harbour Captain's
//! "Shipwright" section) builds the next hull class from the previous one
//! (`RECEITAS`, `proximo_casco`) and puts STARS on three axes — hull, sail,
//! cannon (`custo_da_estrela`, never Rebirth Essence). At sea the skill bar
//! becomes the SHIP's (`HABILIDADES`: Broadside, Bow Cannon, Emergency
//! Repair, Smoke). Sail colours and figureheads (`VELAS`, `FIGURAS`) are
//! cosmetic items from the TP shop.

use serde::{Deserialize, Serialize};

use crate::constants::item_id;
use crate::items::{ItemInstance, NavioData};

/// One hull class.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Casco {
    pub item: u16,
    /// English name (the item's name in the database).
    pub nome: &'static str,
    /// Hull points when new, before stars.
    pub casco: u16,
    /// Speed at sea, units per second, before stars.
    pub velocidade: f32,
    /// Character level (internal) the Shipwright asks to BUILD it: the band
    /// of the island whose stretch of sea it is made for.
    pub nivel: u32,
    /// Cannon multiplier of the class (the ship skills' damage).
    pub canhao: f32,
    /// Drawn size (the collision radius stays `RAIO_DO_CASCO`).
    pub escala: f32,
    /// Masts drawn.
    pub mastros: u8,
}

/// The hulls, in order: each one is built from the one before it.
///
/// The hull is at SHIP scale, not person scale: 2,400 points. A Reef Shark
/// of the Bosque's stretch bites ~15 a hit, so a ship standing still in a
/// pack to gather lasts minutes, and one that keeps moving is never caught
/// (the sharks swim at ~2 u/s, the Sloop sails at 10). Each class adds hull
/// and a little speed; the fastest with five sail stars stays under the wire
/// (`nenhum_casco_passa_do_teto_do_fio`), and the levels are the islands'
/// bands: the Glacier's, the Waste's, the Plateau's.
pub const CASCOS: [Casco; 4] = [
    Casco { item: item_id::NAVIO_CHALUPA, nome: "Sloop", casco: 2400, velocidade: 10.0, nivel: 1, canhao: 1.0, escala: 1.0, mastros: 1 },
    Casco { item: item_id::NAVIO_ESCUNA, nome: "Schooner", casco: 3600, velocidade: 11.2, nivel: 15, canhao: 1.15, escala: 1.12, mastros: 2 },
    Casco { item: item_id::NAVIO_GALEAO, nome: "Galleon", casco: 5200, velocidade: 12.4, nivel: 30, canhao: 1.3, escala: 1.25, mastros: 3 },
    Casco { item: item_id::NAVIO_NAU, nome: "Man-o'-War", casco: 7200, velocidade: 13.4, nivel: 42, canhao: 1.5, escala: 1.38, mastros: 3 },
];

/// What the Shipwright asks to build hull `para`: the hull before it (taken
/// from the Ship slot, consumed) plus these materials.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Receita {
    pub de: u16,
    pub para: u16,
    /// (item, quantity): two woods, the Steel of the colour, Wreck Planks.
    pub materiais: [(u16, u32); 4],
}

/// The three hulls of phase 2. Wood T1..T4 by class, Steel by colour
/// (green, blue, purple), and the sea's own Wreck Plank.
pub const RECEITAS: [Receita; 3] = [
    Receita {
        de: item_id::NAVIO_CHALUPA,
        para: item_id::NAVIO_ESCUNA,
        materiais: [
            (item_id::WOOD_T1, 60),
            (item_id::WOOD_T2, 40),
            (item_id::na_cor(item_id::STEEL, 2), 40),
            (item_id::PRANCHA_DE_NAUFRAGIO, 20),
        ],
    },
    Receita {
        de: item_id::NAVIO_ESCUNA,
        para: item_id::NAVIO_GALEAO,
        materiais: [
            (item_id::WOOD_T2, 80),
            (item_id::WOOD_T3, 50),
            (item_id::na_cor(item_id::STEEL, 3), 80),
            (item_id::PRANCHA_DE_NAUFRAGIO, 50),
        ],
    },
    Receita {
        de: item_id::NAVIO_GALEAO,
        para: item_id::NAVIO_NAU,
        materiais: [
            (item_id::WOOD_T3, 100),
            (item_id::WOOD_T4, 60),
            (item_id::na_cor(item_id::STEEL, 4), 120),
            (item_id::PRANCHA_DE_NAUFRAGIO, 100),
        ],
    },
];

/// The recipe that builds the hull after `de`.
pub fn receita_depois_de(de: u16) -> Option<&'static Receita> {
    RECEITAS.iter().find(|r| r.de == de)
}

/// The hull's radius for the water test. Larger than a body on foot: a ship
/// grazing the coast has to stop before it looks inside the rock.
pub const RAIO_DO_CASCO: f32 = 1.4;

/// Copper per hull point repaired. A full Sloop repair is 240 copper.
pub const COBRE_POR_PONTO: f32 = 0.1;

/// Mail reason of the ship given to old characters (`dungeon::postar`).
pub const MOTIVO_CORREIO_NAVIO: u8 = 8;

/// Below this fraction of the hull the player is warned once.
pub const CRITICO: f32 = 0.3;

/// The hull class of an item, if it is a ship.
pub fn casco(id: u16) -> Option<&'static Casco> {
    CASCOS.iter().find(|c| c.item == id)
}

/// Index of the class in `CASCOS` (0 = Sloop).
pub fn classe(id: u16) -> usize {
    CASCOS.iter().position(|c| c.item == id).unwrap_or(0)
}

/// Hull points of a new ship of this class, before stars.
pub fn casco_max(id: u16) -> u16 {
    casco(id).map_or(0, |c| c.casco)
}

/// Speed at sea of this class, before stars.
pub fn velocidade(id: u16) -> f32 {
    casco(id).map_or(CASCOS[0].velocidade, |c| c.velocidade)
}

/// A brand-new hull.
pub fn novo(id: u16) -> NavioData {
    NavioData { casco: casco_max(id), ..Default::default() }
}

/// What the instance says about the ship. `None` (a ship without state yet)
/// reads as a brand-new hull.
pub fn dados(inst: Option<&ItemInstance>, id: u16) -> NavioData {
    inst.and_then(|i| i.navio).unwrap_or_else(|| novo(id))
}

/// The instance a new ship is born with: no stats, grey, bound to nothing.
pub fn instancia_nova(id: u16) -> ItemInstance {
    let mut i = ItemInstance::vazia_de_grau(1);
    i.navio = Some(novo(id));
    i
}

// ─────────────────────────── ship stars ───────────────────────────

/// Most stars on one axis.
pub const ESTRELAS_MAX: u8 = 5;

/// The three upgrade axes (`NavioData::estrelas` index). No Hold: the trade
/// phase was cut.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Eixo {
    Casco = 0,
    Vela = 1,
    Canhao = 2,
}

impl Eixo {
    pub const TODOS: [Eixo; 3] = [Eixo::Casco, Eixo::Vela, Eixo::Canhao];

    pub fn de_u8(v: u8) -> Option<Eixo> {
        Self::TODOS.get(v as usize).copied()
    }

    /// English (the client translates).
    pub fn nome(self) -> &'static str {
        match self {
            Eixo::Casco => "Hull",
            Eixo::Vela => "Sails",
            Eixo::Canhao => "Cannon",
        }
    }

    /// The card's line for the stars of this axis (English).
    pub fn rotulo_das_estrelas(self) -> &'static str {
        match self {
            Eixo::Casco => "Hull stars",
            Eixo::Vela => "Sail stars",
            Eixo::Canhao => "Cannon stars",
        }
    }

    /// What one star adds, as a fraction.
    pub fn bonus_por_estrela(self) -> f32 {
        match self {
            Eixo::Casco => 0.08,
            Eixo::Vela => 0.03,
            Eixo::Canhao => 0.08,
        }
    }
}

/// Stars of `d` on `eixo`, capped.
pub fn estrelas(d: &NavioData, eixo: Eixo) -> u8 {
    d.estrelas[eixo as usize].min(ESTRELAS_MAX)
}

/// The multiplier of the stars on `eixo` (1.0 with none).
pub fn mult_das_estrelas(d: &NavioData, eixo: Eixo) -> f32 {
    1.0 + estrelas(d, eixo) as f32 * eixo.bonus_por_estrela()
}

/// What one star costs: copper, Darksteel and Wreck Planks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CustoDaEstrela {
    pub cobre: u32,
    pub darksteel: u32,
    pub pranchas: u32,
}

/// The cost to go from `estrelas` to `estrelas + 1` on any axis; `None` at
/// the top. Copper 1k/2k/4k/8k/16k, Darksteel 500..8k, planks 5..25: 31k
/// copper, 15.5k Darksteel and 75 planks to fill one axis. It always
/// succeeds (like the gear's ascension) and NEVER costs Rebirth Essence
/// (owner: Essence is for item stars only).
pub fn custo_da_estrela(estrelas: u8) -> Option<CustoDaEstrela> {
    if estrelas >= ESTRELAS_MAX {
        return None;
    }
    let k = 1u32 << estrelas;
    Some(CustoDaEstrela { cobre: 1_000 * k, darksteel: 500 * k, pranchas: 5 * (estrelas as u32 + 1) })
}

/// The wire's speed ceiling (`EntityState::vel` is i8 at `POS_SCALE`).
pub const TETO_DO_FIO: f32 = i8::MAX as f32 / crate::components::POS_SCALE;
/// Any ship speed, Smoke included, is clamped here, a little under the wire.
pub const VELOCIDADE_MAX: f32 = 15.5;

/// Hull points of this ship when full: the class, times the hull stars.
pub fn casco_total(id: u16, d: &NavioData) -> u16 {
    ((casco_max(id) as f32) * mult_das_estrelas(d, Eixo::Casco)).round().min(u16::MAX as f32) as u16
}

/// The ship's speed at sea: the class, times the sail stars.
pub fn velocidade_do_navio(id: u16, d: &NavioData) -> f32 {
    (velocidade(id) * mult_das_estrelas(d, Eixo::Vela)).min(VELOCIDADE_MAX)
}

/// The cannon multiplier: the class, times the cannon stars.
pub fn mult_do_canhao(id: u16, d: &NavioData) -> f32 {
    casco(id).map_or(1.0, |c| c.canhao) * mult_das_estrelas(d, Eixo::Canhao)
}

/// Copper to repair the hull back to full. Zero when it is full.
pub fn custo_do_reparo(id: u16, d: &NavioData) -> u32 {
    let falta = casco_total(id, d).saturating_sub(d.casco) as f32;
    (falta * COBRE_POR_PONTO).ceil() as u32
}

/// Can this ship sail now?
pub fn pode_zarpar(d: &NavioData) -> bool {
    !d.avariado && d.casco > 0
}

/// The Harbour Captain's trip, in COPPER, to island `ilha` of the
/// `ARQUIPELAGO` (owner's decision, 15/11/2026: the quick trip costs copper
/// so the sea has a reason). Scales with the destination: 50 to the Bosque,
/// 100 the Glacier, 150 the Waste, 200 the Plateau, 250 Skyreach. For
/// reference the Teleport Scroll is 100. The story's own trip (the step
/// that sends you to a new island) stays free.
pub fn preco_da_viagem(ilha: usize) -> u32 {
    50 * (ilha as u32 + 1)
}

/// The hull the Shipwright builds after `id`. `None` after the Man-o'-War.
pub fn proximo_casco(id: u16) -> Option<u16> {
    receita_depois_de(id).map(|r| r.para)
}

/// The ship a rebuilt hull becomes (class `para`): hull full, the stars and
/// the cosmetics carried over — nothing invested in the old hull is lost.
pub fn reconstruido(para: u16, antigo: &NavioData) -> NavioData {
    let mut d = NavioData { estrelas: antigo.estrelas, vela: antigo.vela, figura: antigo.figura, ..Default::default() };
    d.casco = casco_total(para, &d);
    d
}

// ─────────────────────────── cosmetics ───────────────────────────

/// One sail colour.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Vela {
    pub item: u16,
    /// English name (the item's name in the database).
    pub nome: &'static str,
    pub cor: [u8; 3],
    pub preco_tp: u64,
}

/// Sail colours. Append only: the position + 1 is saved on the ship.
pub const VELAS: [Vela; 5] = [
    Vela { item: item_id::VELA_BASE, nome: "Crimson Sails", cor: [176, 38, 42], preco_tp: 150 },
    Vela { item: item_id::VELA_BASE + 1, nome: "Black Sails", cor: [34, 32, 38], preco_tp: 200 },
    Vela { item: item_id::VELA_BASE + 2, nome: "Royal Blue Sails", cor: [40, 72, 168], preco_tp: 150 },
    Vela { item: item_id::VELA_BASE + 3, nome: "Emerald Sails", cor: [36, 132, 84], preco_tp: 150 },
    Vela { item: item_id::VELA_BASE + 4, nome: "Golden Sails", cor: [214, 170, 58], preco_tp: 300 },
];

/// The plain canvas (no sail item applied).
pub const VELA_PADRAO: [u8; 3] = [232, 224, 202];

/// Figureheads: (item, English name, TP price). Append only.
pub const FIGURAS: [(u16, &str, u64); 3] = [
    (item_id::FIGURA_BASE, "Mermaid Figurehead", 250),
    (item_id::FIGURA_BASE + 1, "Dragon Figurehead", 300),
    (item_id::FIGURA_BASE + 2, "Eagle Figurehead", 250),
];

/// What a cosmetic item does to a ship.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Cosmetico {
    /// Sail colour, `VELAS` index + 1.
    Vela(u8),
    /// Figurehead, `FIGURAS` index + 1.
    Figura(u8),
}

/// The cosmetic of item `id`, if it is one.
pub fn cosmetico(id: u16) -> Option<Cosmetico> {
    if let Some(i) = VELAS.iter().position(|v| v.item == id) {
        return Some(Cosmetico::Vela(i as u8 + 1));
    }
    FIGURAS.iter().position(|f| f.0 == id).map(|i| Cosmetico::Figura(i as u8 + 1))
}

/// English name of a cosmetic item.
pub fn nome_do_cosmetico(id: u16) -> Option<&'static str> {
    VELAS.iter().find(|v| v.item == id).map(|v| v.nome).or_else(|| FIGURAS.iter().find(|f| f.0 == id).map(|f| f.1))
}

/// `d` with cosmetic `c` applied.
pub fn aplicar(d: &mut NavioData, c: Cosmetico) {
    match c {
        Cosmetico::Vela(v) => d.vela = v,
        Cosmetico::Figura(f) => d.figura = f,
    }
}

/// The sail's RGB for `vela` (0 = canvas).
pub fn cor_da_vela(vela: u8) -> [u8; 3] {
    vela.checked_sub(1).and_then(|i| VELAS.get(i as usize)).map_or(VELA_PADRAO, |v| v.cor)
}

/// How a ship looks, as other players see it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Visual {
    /// `CASCOS` index.
    pub classe: u8,
    pub vela: u8,
    pub figura: u8,
    /// Stars on all three axes (drawn as brass trim), 0..=15.
    pub estrelas: u8,
}

impl Visual {
    pub fn de(id: u16, d: &NavioData) -> Visual {
        Visual {
            classe: classe(id) as u8,
            vela: d.vela,
            figura: d.figura,
            estrelas: d.estrelas.iter().map(|e| (*e).min(ESTRELAS_MAX)).sum(),
        }
    }

    /// Into the free bytes 5-7 of `EntityMeta::skins` (bytes 0-4 are the
    /// weapon and mount skins): class + 1 with the stars in the high nibble,
    /// then the sail, then the figurehead. Sent only at sea.
    pub fn no_fio(self) -> u64 {
        let b5 = (self.classe as u64 + 1) | ((self.estrelas.min(15) as u64) << 4);
        (b5 << 40) | ((self.vela as u64) << 48) | ((self.figura as u64) << 56)
    }

    /// From `EntityMeta::skins`; byte 5 zero = no ship info (drawn as a
    /// plain Sloop).
    pub fn do_fio(skins: u64) -> Visual {
        let b5 = (skins >> 40) as u8;
        Visual {
            classe: (b5 & 0x0F).saturating_sub(1).min(CASCOS.len() as u8 - 1),
            estrelas: b5 >> 4,
            vela: (skins >> 48) as u8,
            figura: (skins >> 56) as u8,
        }
    }
}

// ─────────────────────────── ship skills ───────────────────────────

/// What a ship skill does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Efeito {
    /// Every enemy within `alcance` off the ship's beam (its sides).
    Bordada,
    /// One shot at the selected target within `alcance`.
    Canhonaco,
    /// Heals `REPARO_FRACAO` of the full hull.
    Reparo,
    /// Untargetable for `FUMACA_S`, faster for `FUMACA_VELOCIDADE_S`.
    Fumaca,
}

/// A ship skill. Mana-free: they use the ship, not the player.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HabilidadeDoNavio {
    /// 0..4: the slot (and `PedidoNavio::Habilidade::id`).
    pub id: u8,
    pub nome: &'static str,
    pub descricao: &'static str,
    pub efeito: Efeito,
    pub espera_s: f32,
    pub alcance: f32,
    /// Damage as a multiple of the expected attack at the captain's level
    /// (`ladder::attack`), before the hull class and the cannon stars.
    pub mult: f32,
    /// Can go on AUTO (the attacks).
    pub auto: bool,
}

/// THE SHIP'S BAR: skill slots 1-4 at sea.
pub const HABILIDADES: [HabilidadeDoNavio; 4] = [
    HabilidadeDoNavio {
        id: 0,
        nome: "Broadside",
        descricao: "Both sides fire: cannon damage to every enemy within 12 m off the ship's sides.",
        efeito: Efeito::Bordada,
        espera_s: 6.0,
        alcance: 12.0,
        mult: 1.4,
        auto: true,
    },
    HabilidadeDoNavio {
        id: 1,
        nome: "Bow Cannon",
        descricao: "One heavy shot at your target, up to 22 m away.",
        efeito: Efeito::Canhonaco,
        espera_s: 4.0,
        alcance: 22.0,
        mult: 2.2,
        auto: true,
    },
    HabilidadeDoNavio {
        id: 2,
        nome: "Emergency Repair",
        descricao: "Patches 30% of the full hull at once.",
        efeito: Efeito::Reparo,
        espera_s: 60.0,
        alcance: 0.0,
        mult: 0.0,
        auto: false,
    },
    HabilidadeDoNavio {
        id: 3,
        nome: "Smoke",
        descricao: "A smoke screen: nobody can target you for 3 s, and you sail 30% faster for 4 s.",
        efeito: Efeito::Fumaca,
        espera_s: 30.0,
        alcance: 0.0,
        mult: 0.0,
        auto: false,
    },
];

/// Cooldown ids of the ship skills in `SkillsState` and their FX (far from
/// the weapon skills 1-12 and the path tree's 200+).
pub const ID_SKILL_BASE: u32 = 300;

pub fn id_de_skill(h: u8) -> u32 {
    ID_SKILL_BASE + h as u32
}

/// The ship skill behind cooldown/FX id `id`, if it is one.
pub fn habilidade_do_id(id: u32) -> Option<&'static HabilidadeDoNavio> {
    id.checked_sub(ID_SKILL_BASE).and_then(|h| HABILIDADES.get(h as usize))
}

pub fn habilidade(h: u8) -> Option<&'static HabilidadeDoNavio> {
    HABILIDADES.get(h as usize)
}

/// Share of the full hull Emergency Repair patches.
pub const REPARO_FRACAO: f32 = 0.30;
/// Smoke: seconds untargetable, seconds faster, and how much faster.
pub const FUMACA_S: f32 = 3.0;
pub const FUMACA_VELOCIDADE_S: f32 = 4.0;
pub const FUMACA_MULT: f32 = 1.3;
/// Broadside: an enemy is "off the beam" when the angle between the heading
/// and it has |cos| at most this (the bow and the stern are blind).
pub const BORDADA_COS_MAX: f32 = 0.8;
/// Cannon fire against another player's HULL takes this share (hull vs
/// hull; the PK modes decide whether it may happen at all).
pub const PVP_CANHAO: f32 = 1.0;

/// Raw damage of attack skill `h` from a captain of level `nivel` on ship
/// `id`/`d`: the level's expected attack × the skill × the class × the
/// cannon stars. Mitigation and the level gap apply on top, as on any hit.
pub fn dano_do_canhao(h: &HabilidadeDoNavio, nivel: u32, id: u16, d: &NavioData) -> i32 {
    let base = crate::ladder::attack(nivel.max(1)) as f32;
    (base * h.mult * mult_do_canhao(id, d)).round().max(0.0) as i32
}

/// Hull points Emergency Repair gives back.
pub fn cura_do_reparo(id: u16, d: &NavioData) -> u16 {
    (casco_total(id, d) as f32 * REPARO_FRACAO).round() as u16
}

/// Is `alvo` off the beam of a ship at `de` heading `rumo`? A ship standing
/// still (no heading) fires all round.
pub fn no_costado(de: glam::Vec2, rumo: glam::Vec2, alvo: glam::Vec2) -> bool {
    let Some(r) = rumo.try_normalize() else {
        return true;
    };
    let Some(d) = (alvo - de).try_normalize() else {
        return true;
    };
    r.dot(d).abs() <= BORDADA_COS_MAX
}

/// Wreck Planks a drifting chest of level `nivel` gives.
pub fn pranchas_do_bau(nivel: u32) -> u32 {
    4 + nivel / 15
}
/// Wreck Planks one piece of wreckage gives (wood or a Steel crate).
pub const PRANCHAS_DO_DESTROCO: u32 = 1;
/// Wreck Planks a sea elite gives its killer.
pub const PRANCHAS_DO_ELITE: u32 = 6;
/// Chance a common sea mob drops one Wreck Plank to its killer.
pub const CHANCE_PRANCHA_DO_BICHO: f32 = 0.25;

/// What the player asks of the ship (`ClientMessage::Navio`).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum PedidoNavio {
    /// At the Harbour Captain: sail out to the open sea with my ship.
    Zarpar,
    /// At sea, at a dock: go ashore on island `ilha` (`ARQUIPELAGO` index).
    Atracar { ilha: u8 },
    /// At the Harbour Captain: repair the hull to full, for copper.
    Reparar,
    /// Send me the ship's state (`ServerMessage::Navio`).
    Estado,
    /// At the Shipwright (the Harbour Captain): build the next hull class
    /// from the one in the Ship slot (`RECEITAS`).
    Construir,
    /// At the Shipwright: one star on axis `eixo` (`Eixo`).
    Melhorar { eixo: u8 },
    /// At sea: ship skill `id` (`HABILIDADES`).
    Habilidade { id: u8 },
}

/// The ship's state, for the Captain's buttons and the hull bar.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
pub struct EstadoNavio {
    /// There is a ship in the ship slot.
    pub tem: bool,
    pub item_id: u16,
    pub casco: u16,
    pub casco_max: u16,
    pub avariado: bool,
    /// Copper for a full repair (0 = nothing to repair).
    pub reparo: u32,
    /// This island has a dock on the open sea (sailing is offered).
    pub zarpar: bool,
}

impl EstadoNavio {
    /// The state of the ship in `id`/`inst` (`None` = no ship).
    pub fn de(id: Option<u16>, inst: Option<&ItemInstance>, zarpar: bool) -> Self {
        let Some(id) = id.filter(|i| item_id::e_navio(*i)) else {
            return Self { zarpar, ..Default::default() };
        };
        let d = dados(inst, id);
        Self {
            tem: true,
            item_id: id,
            casco: d.casco,
            casco_max: casco_total(id, &d),
            avariado: d.avariado,
            reparo: custo_do_reparo(id, &d),
            zarpar,
        }
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    fn com_estrelas(id: u16, e: [u8; 3]) -> NavioData {
        let mut d = novo(id);
        d.estrelas = e;
        d
    }

    /// The fastest hull, with five sail stars and the Smoke on top, stays
    /// under what the wire carries (`EntityState::vel` is i8 at `POS_SCALE`,
    /// ~15.9 u/s).
    #[test]
    fn nenhum_casco_passa_do_teto_do_fio() {
        let teto = i8::MAX as f32 / crate::components::POS_SCALE;
        assert!((TETO_DO_FIO - teto).abs() < 1e-4);
        assert!(VELOCIDADE_MAX < teto);
        for c in CASCOS {
            let cheio = com_estrelas(c.item, [0, ESTRELAS_MAX, 0]);
            let v = c.velocidade * mult_das_estrelas(&cheio, Eixo::Vela);
            assert!(v < teto, "{} at {v} u/s passes the wire's {teto}", c.nome);
            assert!(v <= VELOCIDADE_MAX, "{} at {v} is clamped", c.nome);
            assert!(c.velocidade > crate::PLAYER_SPEED, "a ship slower than walking");
            let fumaca = (velocidade_do_navio(c.item, &cheio) * FUMACA_MULT).min(VELOCIDADE_MAX);
            assert!(fumaca < teto);
        }
    }

    /// HULL CLASSES: every class has more hull and more speed than the one
    /// before, a higher level band, and is built from it.
    #[test]
    fn as_classes_crescem_em_casco_e_velocidade() {
        for par in CASCOS.windows(2) {
            let (a, b) = (par[0], par[1]);
            assert!(b.casco > a.casco, "{} hull", b.nome);
            assert!(b.velocidade > a.velocidade, "{} speed", b.nome);
            assert!(b.velocidade - a.velocidade <= 1.5, "{}: a bit more speed, not a leap", b.nome);
            assert!(b.nivel > a.nivel && b.canhao > a.canhao);
            assert_eq!(proximo_casco(a.item), Some(b.item));
            let r = receita_depois_de(a.item).unwrap();
            assert!(r.materiais.iter().any(|(i, q)| *i == item_id::PRANCHA_DE_NAUFRAGIO && *q > 0));
        }
        assert_eq!(proximo_casco(item_id::NAVIO_NAU), None);
        assert!(CASCOS.iter().all(|c| item_id::e_navio(c.item)));
        assert_eq!(CASCOS.len(), (item_id::NAVIO_ULTIMO - item_id::NAVIO_CHALUPA + 1) as usize);
        // The crossing stays sensible: a 600 u crossing at the slowest and
        // the fastest hull.
        assert!(600.0 / CASCOS[0].velocidade <= 60.0);
        assert!(600.0 / VELOCIDADE_MAX >= 30.0);
    }

    /// STARS: rising cost, at most five, never Essence; each axis grows
    /// what it says.
    #[test]
    fn estrelas_custam_cada_vez_mais_e_param_em_cinco() {
        let mut antes = CustoDaEstrela { cobre: 0, darksteel: 0, pranchas: 0 };
        for e in 0..ESTRELAS_MAX {
            let c = custo_da_estrela(e).unwrap();
            assert!(c.cobre > antes.cobre && c.darksteel > antes.darksteel && c.pranchas > antes.pranchas);
            antes = c;
        }
        assert_eq!(custo_da_estrela(ESTRELAS_MAX), None);
        let id = item_id::NAVIO_ESCUNA;
        let cheio = com_estrelas(id, [5, 5, 5]);
        assert!(casco_total(id, &cheio) > casco_max(id));
        assert!(velocidade_do_navio(id, &cheio) > velocidade(id));
        assert!(mult_do_canhao(id, &cheio) > mult_do_canhao(id, &novo(id)));
        // Past the top is read as the top.
        let demais = com_estrelas(id, [9, 9, 9]);
        assert_eq!(casco_total(id, &demais), casco_total(id, &cheio));
    }

    /// An OLD ship (phase 1 JSON, no stars) loads with zero stars, and the
    /// stars survive a round trip of the instance.
    #[test]
    fn navio_antigo_carrega_e_estrelas_persistem() {
        let velho: NavioData = serde_json::from_str(r#"{"casco":1200,"avariado":false}"#).unwrap();
        assert_eq!(velho.casco, 1200);
        assert_eq!(velho.estrelas, [0, 0, 0]);
        assert_eq!((velho.vela, velho.figura), (0, 0));
        let mut inst = instancia_nova(item_id::NAVIO_GALEAO);
        inst.navio = Some(NavioData { estrelas: [3, 1, 5], vela: 2, figura: 1, ..novo(item_id::NAVIO_GALEAO) });
        let json = serde_json::to_string(&inst).unwrap();
        let volta: ItemInstance = serde_json::from_str(&json).unwrap();
        assert_eq!(volta.navio, inst.navio);
        let bytes = postcard::to_allocvec(&inst).unwrap();
        let volta: ItemInstance = postcard::from_bytes(&bytes).unwrap();
        assert_eq!(volta.navio.unwrap().estrelas, [3, 1, 5]);
    }

    #[test]
    fn reconstruir_leva_estrelas_e_cosmeticos_com_o_casco_cheio() {
        let velho = NavioData { casco: 10, avariado: true, estrelas: [2, 3, 4], vela: 1, figura: 2 };
        let d = reconstruido(item_id::NAVIO_ESCUNA, &velho);
        assert_eq!(d.estrelas, [2, 3, 4]);
        assert_eq!((d.vela, d.figura), (1, 2));
        assert!(!d.avariado);
        assert_eq!(d.casco, casco_total(item_id::NAVIO_ESCUNA, &d));
    }

    #[test]
    fn reparo_cobra_o_que_falta_e_nada_cheio() {
        let id = item_id::NAVIO_CHALUPA;
        assert_eq!(custo_do_reparo(id, &novo(id)), 0);
        let afundado = NavioData { casco: 0, avariado: true, ..Default::default() };
        assert_eq!(custo_do_reparo(id, &afundado), 240);
        assert!(!pode_zarpar(&afundado));
        assert!(pode_zarpar(&novo(id)));
        assert_eq!(dados(None, id), novo(id), "no state reads as a new hull");
        // The hull stars raise the full hull, and so the full repair.
        let estrelado = NavioData { casco: 0, avariado: true, estrelas: [5, 0, 0], ..Default::default() };
        assert_eq!(custo_do_reparo(id, &estrelado), 336);
    }

    #[test]
    fn a_viagem_cresce_com_o_destino() {
        assert_eq!(preco_da_viagem(0), 50);
        assert!(preco_da_viagem(3) > preco_da_viagem(1));
        assert!(preco_da_viagem(4) <= 300, "modest: the scroll is 100");
    }

    /// COSMETICS: every sail and figurehead item applies to the ship, and
    /// the look survives the wire.
    #[test]
    fn cosmeticos_se_aplicam_e_viajam_no_fio() {
        let mut d = novo(item_id::NAVIO_NAU);
        for (i, v) in VELAS.iter().enumerate() {
            let c = cosmetico(v.item).unwrap();
            aplicar(&mut d, c);
            assert_eq!(d.vela, i as u8 + 1);
            assert_eq!(cor_da_vela(d.vela), v.cor);
        }
        for (i, f) in FIGURAS.iter().enumerate() {
            aplicar(&mut d, cosmetico(f.0).unwrap());
            assert_eq!(d.figura, i as u8 + 1);
        }
        assert_eq!(cosmetico(item_id::NAVIO_NAU), None);
        assert_eq!(cor_da_vela(0), VELA_PADRAO);
        d.estrelas = [5, 5, 5];
        let v = Visual::de(item_id::NAVIO_NAU, &d);
        let fio = v.no_fio();
        assert_eq!(fio & 0xFF_FFFF_FFFF, 0, "bytes 0-4 are the weapon and mount skins");
        assert_eq!(Visual::do_fio(fio | 0x12_3456_789A), v);
        assert_eq!(Visual::do_fio(0).classe, 0, "no ship info draws a Sloop");
    }

    /// The cannons scale with the class and the stars, and the skills are
    /// mana-free with real cooldowns; only the attacks go on AUTO.
    #[test]
    fn as_skills_do_navio() {
        for (i, h) in HABILIDADES.iter().enumerate() {
            assert_eq!(h.id as usize, i);
            assert!(h.espera_s > 0.0);
            assert_eq!(habilidade_do_id(id_de_skill(h.id)), Some(h));
            assert_eq!(h.auto, h.mult > 0.0, "{}: AUTO only for the attacks", h.nome);
        }
        let bordada = &HABILIDADES[0];
        let sloop = dano_do_canhao(bordada, 30, item_id::NAVIO_CHALUPA, &novo(item_id::NAVIO_CHALUPA));
        let nau = dano_do_canhao(bordada, 30, item_id::NAVIO_NAU, &com_estrelas(item_id::NAVIO_NAU, [0, 0, 5]));
        assert!(nau > sloop * 2, "{sloop} vs {nau}");
        assert!(HABILIDADES[2].espera_s >= 45.0, "Emergency Repair has a long cooldown");
        assert!(no_costado(glam::Vec2::ZERO, glam::Vec2::Y, glam::Vec2::X * 5.0));
        assert!(!no_costado(glam::Vec2::ZERO, glam::Vec2::Y, glam::Vec2::Y * 5.0), "the bow is blind");
        assert!(no_costado(glam::Vec2::ZERO, glam::Vec2::ZERO, glam::Vec2::Y * 5.0), "standing still fires all round");
    }

    /// THE SEA MOBS AGAINST THE CANNONS: the Reef Shark (the wolf's
    /// profile) and the Tiger Eel (the tiger's: 95/120 life, defence 2) at
    /// the captain's level take a few Broadsides from a bare Sloop — a
    /// fight, not a one-shot nor a siege — and fewer from a starred
    /// Man-o'-War; the sea elite (×5 life) takes a real exchange.
    #[test]
    fn os_bichos_do_mar_aguentam_alguns_tiros() {
        use crate::ladder::{damage, mob, Profile};
        let bordada = &HABILIDADES[0];
        let tiros = |vida: i32, dano: i32, defesa: i32| (vida as f32 / damage(dano, defesa) as f32).ceil() as u32;
        for nivel in [5u32, 15, 30, 45, 60] {
            for perfil in [Profile::relativo_ao_lobo(120, 10, 0), Profile::relativo_ao_lobo(95, 15, 2)] {
                let m = mob(&perfil, nivel);
                let sloop = dano_do_canhao(bordada, nivel, item_id::NAVIO_CHALUPA, &novo(item_id::NAVIO_CHALUPA));
                let n = tiros(m.health, sloop, m.defense);
                assert!((2..=6).contains(&n), "level {nivel}: {n} broadsides from a Sloop for {} life", m.health);
                let nau = dano_do_canhao(bordada, nivel, item_id::NAVIO_NAU, &com_estrelas(item_id::NAVIO_NAU, [0, 0, 5]));
                assert!(tiros(m.health, nau, m.defense) < n, "level {nivel}: the Man-o'-War is no better");
                let elite = tiros(m.health * 5, sloop, m.defense);
                assert!((8..=30).contains(&elite), "level {nivel}: elite in {elite}");
            }
        }
    }
}
