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
//! becomes the SHIP's (`HABILIDADES`: Broadside, Long Gun, Emergency
//! Repair, Smoke). Every gun fires from the sides. Sail colours and figureheads (`VELAS`, `FIGURAS`) are
//! cosmetic items from the TP shop and the Naval Quartermaster.
//!
//! PHASE 4, the Corsair and naval PvP: every world boss drops a COLOSSUS
//! CHEST that fits only on the ship's DECK (`NavioData::conves`). Opening it
//! where it dropped pays x1.0; docking at other islands with it on deck pays
//! more (`mult_do_tesouro`, x1.6 for one island up to x3.0). A ship with
//! treasure is MARKED (`marcado`): anyone may attack it at sea, outside the
//! harbour corridors. Sunk, the treasure floats; killed in PvP, it goes to
//! the killer. Never destroyed.

use serde::{Deserialize, Serialize};

use crate::constants::item_id;
use crate::items::{ItemInstance, NavioData};

// ═══════════════════════════════════════════════════════════════════════════
// NOVO SISTEMA DE BARCOS (18/11/2026) - docs/BARCOS.md
// ═══════════════════════════════════════════════════════════════════════════

/// Tamanho do barco (determina capacidade de passageiros).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Tamanho {
    Pequeno = 0, // 1-2 passageiros
    Medio = 1,   // 3-4 passageiros
    Grande = 2,  // 6-8 passageiros
}

/// Variante do barco (determina afinidade de stats base).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Variante {
    Veloz = 0,  // Afinidade: velocidade + virada
    Ataque = 1, // Afinidade: dano
    Tanque = 2, // Afinidade: HP
}

/// Tipo de barco (3 tamanhos × 3 variantes = 9 tipos).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct TipoBarco {
    pub tamanho: Tamanho,
    pub variante: Variante,
}

impl TipoBarco {
    /// ID único do tipo (0-8).
    pub fn id(self) -> u8 {
        (self.tamanho as u8) * 3 + (self.variante as u8)
    }

    /// Tipo a partir do ID.
    pub fn de_id(id: u8) -> Option<Self> {
        if id >= 9 {
            return None;
        }
        let tamanho = match id / 3 {
            0 => Tamanho::Pequeno,
            1 => Tamanho::Medio,
            2 => Tamanho::Grande,
            _ => return None,
        };
        let variante = match id % 3 {
            0 => Variante::Veloz,
            1 => Variante::Ataque,
            2 => Variante::Tanque,
            _ => return None,
        };
        Some(TipoBarco { tamanho, variante })
    }

    /// Nome em inglês do tipo.
    pub fn nome(self) -> &'static str {
        match (self.tamanho, self.variante) {
            (Tamanho::Pequeno, Variante::Veloz) => "Swift Sloop",
            (Tamanho::Pequeno, Variante::Ataque) => "Corsair Sloop",
            (Tamanho::Pequeno, Variante::Tanque) => "Ironclad Sloop",
            (Tamanho::Medio, Variante::Veloz) => "Swift Brigantine",
            (Tamanho::Medio, Variante::Ataque) => "Corsair Brigantine",
            (Tamanho::Medio, Variante::Tanque) => "Ironclad Brigantine",
            (Tamanho::Grande, Variante::Veloz) => "Swift Galleon",
            (Tamanho::Grande, Variante::Ataque) => "Corsair Galleon",
            (Tamanho::Grande, Variante::Tanque) => "Ironclad Galleon",
        }
    }

    /// Capacidade de passageiros (fixo por tamanho).
    pub fn capacidade(self) -> u8 {
        match self.tamanho {
            Tamanho::Pequeno => 2,
            Tamanho::Medio => 4,
            Tamanho::Grande => 8,
        }
    }
}

/// Stats em unidades reais: u/s, pontos de casco, multiplicador e rad/s.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StatsBase {
    pub velocidade: f32,
    pub hp: f32,
    pub dano: f32,
    pub virada: f32,
}

/// Pontos de stat alocados pelo player (máximo por tier).
pub const PONTOS_POR_TIER: [u8; 5] = [0, 10, 18, 26, 34];
pub const PONTO_VELOCIDADE: f32 = 0.02;
pub const PONTO_HP: f32 = 0.04;
pub const PONTO_DANO: f32 = 0.04;
pub const PONTO_VIRADA: f32 = 0.03;

/// Multiplicador de casco e dano por tier.
pub fn tier_mult(tier: u8) -> f32 {
    match tier {
        1 => 0.6,
        2 => 0.9,
        3 => 1.2,
        4 => 1.5,
        _ => 1.0,
    }
}

/// O tier pesa menos na manobra para não deixar o T1 lento demais.
pub fn tier_mult_manobra(tier: u8) -> f32 {
    match tier {
        1 => 0.85,
        2 => 0.95,
        3 => 1.05,
        4 => 1.15,
        _ => 1.0,
    }
}

pub fn pontos_no_tier(tier: u8) -> Option<u8> {
    (1..=4)
        .contains(&tier)
        .then(|| PONTOS_POR_TIER[tier as usize])
}

pub fn valida_pontos(tier: u8, pontos: [u8; 4]) -> bool {
    pontos_no_tier(tier)
        .is_some_and(|max| pontos.iter().map(|&p| p as u16).sum::<u16>() <= max as u16)
}

/// Calcula stats finais de um barco novo. Cada ponto incide sobre o stat base.
pub fn stats_finais_novo(tipo: TipoBarco, tier: u8, pontos: [u8; 4]) -> StatsBase {
    let base = stats_base(tipo);
    let combate = tier_mult(tier);
    let manobra = tier_mult_manobra(tier);
    StatsBase {
        velocidade: (base.velocidade * manobra * (1.0 + pontos[0] as f32 * PONTO_VELOCIDADE))
            .min(VELOCIDADE_MAX),
        hp: base.hp * combate * (1.0 + pontos[1] as f32 * PONTO_HP),
        dano: base.dano * combate * (1.0 + pontos[2] as f32 * PONTO_DANO),
        virada: base.virada * manobra * (1.0 + pontos[3] as f32 * PONTO_VIRADA),
    }
}

/// Stats base por tipo de barco.
pub fn stats_base(tipo: TipoBarco) -> StatsBase {
    match (tipo.tamanho, tipo.variante) {
        (Tamanho::Pequeno, Variante::Veloz) => StatsBase {
            velocidade: 11.0,
            hp: 2400.0,
            dano: 0.95,
            virada: 1.60,
        },
        (Tamanho::Pequeno, Variante::Ataque) => StatsBase {
            velocidade: 10.2,
            hp: 2600.0,
            dano: 1.10,
            virada: 1.45,
        },
        (Tamanho::Pequeno, Variante::Tanque) => StatsBase {
            velocidade: 9.8,
            hp: 3000.0,
            dano: 1.00,
            virada: 1.35,
        },
        (Tamanho::Medio, Variante::Veloz) => StatsBase {
            velocidade: 11.2,
            hp: 3600.0,
            dano: 1.10,
            virada: 1.25,
        },
        (Tamanho::Medio, Variante::Ataque) => StatsBase {
            velocidade: 10.7,
            hp: 3800.0,
            dano: 1.25,
            virada: 1.15,
        },
        (Tamanho::Medio, Variante::Tanque) => StatsBase {
            velocidade: 10.2,
            hp: 4500.0,
            dano: 1.15,
            virada: 1.05,
        },
        (Tamanho::Grande, Variante::Veloz) => StatsBase {
            velocidade: 11.0,
            hp: 5200.0,
            dano: 1.30,
            virada: 0.90,
        },
        (Tamanho::Grande, Variante::Ataque) => StatsBase {
            velocidade: 10.3,
            hp: 6000.0,
            dano: 1.50,
            virada: 0.80,
        },
        (Tamanho::Grande, Variante::Tanque) => StatsBase {
            velocidade: 9.8,
            hp: 7200.0,
            dano: 1.40,
            virada: 0.70,
        },
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// SISTEMA ANTIGO (mantido para compatibilidade)
// ═══════════════════════════════════════════════════════════════════════════

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
    /// TURNING INERTIA (16/11/2026): the most the heading turns, radians per
    /// second, before sail stars (`giro_max`). The bigger the hull, the
    /// slower it comes round.
    pub giro: f32,
    /// Seconds from standing still to full speed, and from full speed to a
    /// stop (`Leme::governa`).
    pub arranque_s: f32,
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
    Casco {
        item: item_id::NAVIO_CHALUPA,
        nome: "Sloop",
        casco: 2400,
        velocidade: 10.0,
        nivel: 1,
        canhao: 1.0,
        escala: 1.0,
        mastros: 1,
        giro: 1.5,
        arranque_s: 0.6,
    },
    Casco {
        item: item_id::NAVIO_ESCUNA,
        nome: "Schooner",
        casco: 3600,
        velocidade: 11.2,
        nivel: 15,
        canhao: 1.15,
        escala: 1.12,
        mastros: 2,
        giro: 1.2,
        arranque_s: 0.75,
    },
    Casco {
        item: item_id::NAVIO_GALEAO,
        nome: "Galleon",
        casco: 5200,
        velocidade: 12.4,
        nivel: 30,
        canhao: 1.3,
        escala: 1.25,
        mastros: 3,
        giro: 0.9,
        arranque_s: 0.9,
    },
    Casco {
        item: item_id::NAVIO_NAU,
        nome: "Man-o'-War",
        casco: 7200,
        velocidade: 13.4,
        nivel: 42,
        canhao: 1.5,
        escala: 1.38,
        mastros: 3,
        giro: 0.7,
        arranque_s: 1.05,
    },
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

/// THE COAST'S PUSH (owner, 20/11/2026: "precisa ter um sistema que
/// empurra um pouco o barco de volta pro mar"): a hull may nose onto a beach
/// (its collision is a small circle at the centre), but it does not stay
/// buried there. Units/s, out to sea.
pub const EMPURRAO_DA_COSTA: f32 = 4.0;

/// Which way the coast pushes a hull of `classe` at `centro` heading `rumo`:
/// the unit sum of "away from each outline point on land" (bow, stern, both
/// sides, the four shoulders); zero when the whole outline floats.
pub fn empurrao_da_costa(
    centro: glam::Vec2,
    rumo: glam::Vec2,
    classe: usize,
    agua: &dyn Fn(f32, f32) -> bool,
) -> glam::Vec2 {
    let mc = crate::conves::meio_comp(classe) * 0.9;
    let mb = crate::conves::meia_boca(classe) * 0.9;
    let locais = [
        (0.0, mc),
        (0.0, -mc),
        (mb, 0.0),
        (-mb, 0.0),
        (mb * 0.7, mc * 0.5),
        (-mb * 0.7, mc * 0.5),
        (mb * 0.7, -mc * 0.5),
        (-mb * 0.7, -mc * 0.5),
    ];
    let mut soma = glam::Vec2::ZERO;
    for (x, y) in locais {
        let p = crate::conves::do_mundo(centro, rumo, glam::Vec2::new(x, y));
        if !agua(p.x, p.y) {
            soma += (centro - p).normalize_or_zero();
        }
    }
    soma.normalize_or_zero()
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
    NavioData {
        casco: casco_max(id),
        ..Default::default()
    }
}

/// What the instance says about the ship. `None` (a ship without state yet)
/// reads as a brand-new hull.
pub fn dados(inst: Option<&ItemInstance>, id: u16) -> NavioData {
    let d = inst.and_then(|i| i.navio).unwrap_or_else(|| novo(id));
    migrar_antigo(id, &d)
}

/// The instance a new ship is born with: no stats, grey, bound to nothing.
pub fn instancia_nova(id: u16) -> ItemInstance {
    let mut i = ItemInstance::vazia_de_grau(1);
    i.navio = Some(novo(id));
    i
}

pub fn tipo_novo(d: &NavioData) -> Option<TipoBarco> {
    d.tipo_novo
        .and_then(TipoBarco::de_id)
        .filter(|_| (1..=4).contains(&d.tier))
}

pub fn nome_do_navio(id: u16, d: &NavioData) -> &'static str {
    tipo_novo(d).map_or_else(|| casco(id).map_or("Ship", |c| c.nome), TipoBarco::nome)
}

pub fn classe_do_navio(id: u16, d: &NavioData) -> usize {
    tipo_novo(d).map_or_else(|| classe(id), |t| t.tamanho as usize)
}

pub fn capacidade_do_navio(id: u16, d: &NavioData) -> usize {
    tipo_novo(d).map_or_else(
        || [2, 4, 6, 8][classe(id).min(3)],
        |t| t.capacidade() as usize,
    )
}

pub fn stats_finais(id: u16, d: &NavioData) -> StatsBase {
    if let Some(tipo) = tipo_novo(d) {
        return stats_finais_novo(tipo, d.tier, d.pontos);
    }
    let c = casco(id).unwrap_or(&CASCOS[0]);
    StatsBase {
        velocidade: (c.velocidade * mult_das_estrelas(d, Eixo::Vela)).min(VELOCIDADE_MAX),
        hp: c.casco as f32 * mult_das_estrelas(d, Eixo::Casco),
        dano: c.canhao * mult_das_estrelas(d, Eixo::Canhao),
        virada: c.giro * mult_das_estrelas(d, Eixo::Vela),
    }
}

pub fn arranque_do_navio(id: u16, d: &NavioData) -> f32 {
    tipo_novo(d).map_or_else(|| arranque_s(id), |t| [0.6, 0.8, 1.0][t.tamanho as usize])
}

fn pontos_migrados(tier: u8, estrelas: [u8; 3]) -> [u8; 4] {
    let max = pontos_no_tier(tier).unwrap_or(0) as usize;
    let brutos = [
        estrelas[1] as usize,
        estrelas[0] as usize,
        estrelas[2] as usize,
        estrelas[1] as usize,
    ];
    let total: usize = brutos.iter().sum();
    if total == 0 || max == 0 {
        return [0; 4];
    }
    let mut pontos = [0usize; 4];
    for _ in 0..max {
        let mut melhor = 0;
        for i in 1..4 {
            if brutos[i] * (pontos[melhor] + 1) > brutos[melhor] * (pontos[i] + 1) {
                melhor = i;
            }
        }
        pontos[melhor] += 1;
    }
    pontos.map(|p| p as u8)
}

/// Converte o casco e as estrelas antigas sem remover o item nem cosméticos.
pub fn migrar_antigo(id: u16, antigo: &NavioData) -> NavioData {
    if tipo_novo(antigo).is_some() {
        return *antigo;
    }
    let (tipo, tier) = match classe(id) {
        0 => (
            TipoBarco {
                tamanho: Tamanho::Pequeno,
                variante: Variante::Veloz,
            },
            1,
        ),
        1 => (
            TipoBarco {
                tamanho: Tamanho::Medio,
                variante: Variante::Ataque,
            },
            2,
        ),
        2 => (
            TipoBarco {
                tamanho: Tamanho::Grande,
                variante: Variante::Ataque,
            },
            3,
        ),
        _ => (
            TipoBarco {
                tamanho: Tamanho::Grande,
                variante: Variante::Tanque,
            },
            4,
        ),
    };
    let total_antigo = casco_max(id).max(1) as f32;
    let fracao = antigo.casco as f32 / total_antigo;
    let mut d = NavioData {
        casco: 0,
        avariado: antigo.avariado,
        estrelas: antigo.estrelas,
        vela: antigo.vela,
        figura: antigo.figura,
        conves: antigo.conves,
        tipo_novo: Some(tipo.id()),
        pontos: pontos_migrados(tier, antigo.estrelas),
        tier,
    };
    d.casco = (stats_finais_novo(tipo, tier, d.pontos).hp * fracao)
        .round()
        .clamp(0.0, u16::MAX as f32) as u16;
    d
}

pub fn novo_tipo(tipo: TipoBarco, tier: u8) -> NavioData {
    let mut d = NavioData {
        tipo_novo: Some(tipo.id()),
        tier: tier.clamp(1, 4),
        ..Default::default()
    };
    d.casco = stats_finais_novo(tipo, d.tier, d.pontos).hp.round() as u16;
    d
}

pub fn instancia_nova_tipo(tipo: TipoBarco, tier: u8) -> ItemInstance {
    let mut inst = ItemInstance::vazia_de_grau(tier.clamp(1, 4));
    inst.tier = tier.clamp(1, 4);
    inst.navio = Some(novo_tipo(tipo, tier));
    inst
}

pub fn custo_respec(tier: u8) -> u64 {
    250 * tier.clamp(1, 4) as u64
}

pub fn custo_upgrade(tier: u8) -> Option<u64> {
    match tier {
        1 => Some(500),
        2 => Some(2_000),
        3 => Some(6_000),
        _ => None,
    }
}

/// Materiais do upgrade de tier, SEM precisar de outro barco: a receita do
/// casco T1 vezes 2^(tier-1), o mesmo que custaria craftar o barco duplicado
/// (T1→T2 = 1 casco, T2→T3 = 2, T3→T4 = 4). Vazio no T4.
pub fn materiais_upgrade(tipo: TipoBarco, tier: u8) -> Vec<(u16, u32)> {
    if custo_upgrade(tier).is_none() {
        return Vec::new();
    }
    let mult = 1u32 << (tier.clamp(1, 4) - 1);
    receita_tipo(tipo)
        .into_iter()
        .filter(|(_, q)| *q > 0)
        .map(|(item, q)| (item, q * mult))
        .collect()
}

/// Materiais de um casco T1. A variante escolhe os stats; o tamanho escolhe
/// a receita. Todo barco criado aqui usa o item 470 e guarda tipo/tier na instância.
pub fn receita_tipo(tipo: TipoBarco) -> [(u16, u32); 4] {
    match tipo.tamanho {
        Tamanho::Pequeno => [
            (item_id::WOOD_T1, 80),
            (item_id::na_cor(item_id::STEEL, 1), 30),
            (item_id::PRANCHA_DE_NAUFRAGIO, 10),
            (item_id::DARKSTEEL, 0),
        ],
        Tamanho::Medio => [
            (item_id::WOOD_T2, 120),
            (item_id::na_cor(item_id::STEEL, 2), 60),
            (item_id::PRANCHA_DE_NAUFRAGIO, 30),
            (item_id::DARKSTEEL, 20),
        ],
        Tamanho::Grande => [
            (item_id::WOOD_T3, 180),
            (item_id::na_cor(item_id::STEEL, 3), 100),
            (item_id::PRANCHA_DE_NAUFRAGIO, 60),
            (item_id::DARKSTEEL, 50),
        ],
    }
}

/// Alocação inicial e pontos novos de tier são grátis; tirar ou mover pontos
/// exige o respec, que zera a build antes de uma nova alocação.
pub fn alocacao_valida(antigos: [u8; 4], novos: [u8; 4], tier: u8) -> bool {
    valida_pontos(tier, novos) && (0..4).all(|i| novos[i] >= antigos[i])
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
    Some(CustoDaEstrela {
        cobre: 1_000 * k,
        darksteel: 500 * k,
        pranchas: 5 * (estrelas as u32 + 1),
    })
}

/// The wire's speed ceiling (`EntityState::vel` is i8 at `POS_SCALE`).
pub const TETO_DO_FIO: f32 = i8::MAX as f32 / crate::components::POS_SCALE;
/// Any ship speed, Smoke included, is clamped here, a little under the wire.
pub const VELOCIDADE_MAX: f32 = 15.5;

/// Hull points of this ship when full: the class, times the hull stars.
pub fn casco_total(id: u16, d: &NavioData) -> u16 {
    stats_finais(id, d).hp.round().clamp(0.0, u16::MAX as f32) as u16
}

/// The ship's speed at sea: the class, times the sail stars.
pub fn velocidade_do_navio(id: u16, d: &NavioData) -> f32 {
    stats_finais(id, d).velocidade.min(VELOCIDADE_MAX)
}

// ─────────────── turning inertia (16/11/2026, "sensação de barco") ───────────────

/// While turning hard the ship slows: the target speed is multiplied by
/// `1 - FREIO_NA_CURVA * |angle to the wanted heading| / π` (a full
/// about-turn sails at 40%). At speed the ship carves a wide arc; slowed
/// down it turns tighter.
pub const FREIO_NA_CURVA: f32 = 0.6;
/// A route waypoint (not the last) counts as reached this close: a hull
/// with a turning circle cannot thread a 0.6 m needle at speed.
pub const FOLGA_DA_ROTA: f32 = 2.5;
/// A bend of the route is taken on an arc of about this radius times
/// `1/sin(φ/2)` (`direcao_na_rota`): ~5.7 m for a right angle.
pub const CURVA_DA_ROTA: f32 = 4.0;

/// The most the heading of ship `id`/`d` turns, rad/s: the class, plus 3%
/// per sail star (the same bonus as the speed's).
pub fn giro_max(id: u16, d: &NavioData) -> f32 {
    stats_finais(id, d).virada
}

/// Seconds from standing still to full speed (and back down) of class `id`.
pub fn arranque_s(id: u16) -> f32 {
    casco(id).map_or(CASCOS[0].arranque_s, |c| c.arranque_s)
}

/// The ship card's word for how ship `id`/`d` turns (English; the client
/// translates): agile, steady or heavy.
pub fn rotulo_do_giro(id: u16, d: &NavioData) -> &'static str {
    let g = giro_max(id, d);
    if g >= 1.35 {
        "Agile"
    } else if g >= 1.0 {
        "Steady"
    } else {
        "Heavy"
    }
}

/// THE HELM of one hull at sea: the heading it points (`proa`, radians,
/// `atan2(y, x)` of the world plane) and the speed it carries. The SERVER
/// owns one per sea session (`Session::leme_do_mar`); the velocity it sends
/// is always `proa` times `vel`, so the hull on screen never snaps round.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Leme {
    pub proa: f32,
    pub vel: f32,
    /// False until the first order: a ship that never sailed has no heading
    /// yet (`no_costado` then fires all round).
    pub pronto: bool,
}

fn ang_dif(a: f32, b: f32) -> f32 {
    let mut d = (a - b) % std::f32::consts::TAU;
    if d > std::f32::consts::PI {
        d -= std::f32::consts::TAU;
    } else if d < -std::f32::consts::PI {
        d += std::f32::consts::TAU;
    }
    d
}

impl Leme {
    /// Where the bow points, if the ship has a heading.
    pub fn rumo(&self) -> Option<glam::Vec2> {
        self.pronto
            .then(|| glam::Vec2::new(self.proa.cos(), self.proa.sin()))
    }

    /// The hull's velocity now.
    pub fn velocidade(&self) -> glam::Vec2 {
        self.rumo().unwrap_or(glam::Vec2::ZERO) * self.vel
    }

    /// Angle (0..π) between the bow and direction `dir`; 0 with no heading.
    pub fn angulo_para(&self, dir: glam::Vec2) -> f32 {
        match (self.pronto, dir.try_normalize()) {
            (true, Some(d)) => ang_dif(d.y.atan2(d.x), self.proa).abs(),
            _ => 0.0,
        }
    }

    /// One tick at the helm: turn toward `dir` (any length; zero = no
    /// steering) at most `giro` rad/s, and bring the speed toward `quer`
    /// (slowed by the turn still to make, `FREIO_NA_CURVA`) over
    /// `arranque_s` seconds for a ship of top speed `v_navio`. Returns the
    /// velocity, never above `VELOCIDADE_MAX` (the wire).
    pub fn governa(
        &mut self,
        dir: glam::Vec2,
        quer: f32,
        v_navio: f32,
        giro: f32,
        arranque_s: f32,
        dt: f32,
    ) -> glam::Vec2 {
        let mut alvo_v = 0.0;
        if let Some(d) = dir.try_normalize() {
            let alvo = d.y.atan2(d.x);
            if !self.pronto {
                // The very first order: the ship was lying that way.
                self.proa = alvo;
                self.pronto = true;
            }
            let passo = giro.max(0.0) * dt;
            self.proa = ang_dif(
                self.proa + ang_dif(alvo, self.proa).clamp(-passo, passo),
                0.0,
            );
            let falta = ang_dif(alvo, self.proa).abs();
            alvo_v = quer.max(0.0) * (1.0 - FREIO_NA_CURVA * falta / std::f32::consts::PI);
        }
        let taxa = v_navio.max(1.0) / arranque_s.max(0.05) * dt;
        self.vel = (self.vel + (alvo_v - self.vel).clamp(-taxa, taxa)).clamp(0.0, VELOCIDADE_MAX);
        self.velocidade()
    }
}

/// THE ROUTE AT SEA with a turning circle: the stick for this tick (its
/// length is the throttle, 0.02..1) toward the follower's next point, for a
/// hull of top speed `v`, turning `giro` rad/s, `arranque_s` to stop.
///
/// - A point at distance `d`, `θ` off the bow is reachable only if
///   `d ≥ 2·r·sin θ` with the turning radius `r = v / giro`: the throttle
///   drops until that holds, so the ship slows into a sharp corner instead
///   of orbiting the waypoint forever.
/// - The CORNER ahead (the route bends by `φ` at the next point) is taken
///   at the speed whose arc stays within `CURVA_DA_ROTA` of it, braking in
///   time (`v² = v_c² + 2·a·d`): a Man-o'-War shortens sail before a bend
///   instead of carving into the coast after it.
/// - While it is still coming round (`θ` > ~30°) the follower is told it is
///   not stuck (`SeguidorDeRota::aguenta`): getting further from the point
///   for a few seconds is the turn, not a wall.
pub fn direcao_na_rota(
    seg: &mut crate::terreno::SeguidorDeRota,
    leme: &Leme,
    p: glam::Vec2,
    v: f32,
    giro: f32,
    arranque_s: f32,
) -> Option<glam::Vec2> {
    let virando = seg
        .pontos()
        .next()
        .is_some_and(|q| leme.angulo_para(q - p) > 0.5);
    if virando {
        seg.aguenta();
    }
    let dir = seg.direcao(p)?;
    let mut pontos = seg.pontos();
    let Some(q) = pontos.next() else {
        return Some(dir);
    };
    let giro = giro.max(0.01);
    let v = v.max(0.1);
    let theta = leme.angulo_para(q - p).min(std::f32::consts::FRAC_PI_2);
    let d = p.distance(q);
    let mut pode = giro * d * 0.85 / (2.0 * theta.sin()).max(1e-3);
    if let Some(q2) = pontos.next() {
        let phi = match ((q - p).try_normalize(), (q2 - q).try_normalize()) {
            (Some(a), Some(b)) => a.dot(b).clamp(-1.0, 1.0).acos(),
            _ => 0.0,
        };
        if phi > 0.2 {
            let na_curva = giro * CURVA_DA_ROTA / (phi * 0.5).sin().max(0.15);
            let freio = v / arranque_s.max(0.05);
            let ate = (d - FOLGA_DA_ROTA).max(0.0);
            pode = pode.min((na_curva * na_curva + 2.0 * freio * ate).sqrt());
        }
    }
    Some(dir * (pode / v).clamp(0.02, 1.0))
}

/// A HULL GRAZING THE COAST scrapes along it instead of sticking dead
/// (turning inertia, 16/11/2026): a ship that comes out of a bend a metre
/// off the line can meet a spit of land head-on, and its heading only turns
/// at `giro_max` — before, it pressed into the rock forever. When
/// `terreno::mover_casco` refuses the step, the same step turned ±20° and
/// ±40° is tried at half speed; still nothing, it stays.
pub fn mover_raspando(
    agua: &dyn Fn(f32, f32) -> bool,
    pos: glam::Vec2,
    vel: glam::Vec2,
    dt: f32,
    raio: f32,
) -> glam::Vec2 {
    let q = crate::terreno::mover_casco(agua, pos, vel, dt, raio);
    if q != pos || vel == glam::Vec2::ZERO {
        return q;
    }
    for a in [0.35f32, -0.35, 0.7, -0.7] {
        let (s, c) = a.sin_cos();
        let v = glam::Vec2::new(vel.x * c - vel.y * s, vel.x * s + vel.y * c) * 0.5;
        let q = crate::terreno::mover_casco(agua, pos, v, dt, raio);
        if q != pos {
            return q;
        }
    }
    pos
}

/// The cannon multiplier: the class, times the cannon stars.
pub fn mult_do_canhao(id: u16, d: &NavioData) -> f32 {
    stats_finais(id, d).dano
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
    let tipo_novo = tipo_novo(antigo).map(|tipo| {
        TipoBarco {
            tamanho: match classe(para) {
                0 => Tamanho::Pequeno,
                1 => Tamanho::Medio,
                _ => Tamanho::Grande,
            },
            variante: tipo.variante,
        }
        .id()
    });
    let mut d = NavioData {
        estrelas: antigo.estrelas,
        vela: antigo.vela,
        figura: antigo.figura,
        // The treasure on deck moves to the new hull (never destroyed).
        conves: antigo.conves,
        tipo_novo,
        pontos: antigo.pontos,
        tier: (classe(para) as u8 + 1).clamp(1, 4),
        ..Default::default()
    };
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
    Vela {
        item: item_id::VELA_BASE,
        nome: "Crimson Sails",
        cor: [176, 38, 42],
        preco_tp: 150,
    },
    Vela {
        item: item_id::VELA_BASE + 1,
        nome: "Black Sails",
        cor: [34, 32, 38],
        preco_tp: 200,
    },
    Vela {
        item: item_id::VELA_BASE + 2,
        nome: "Royal Blue Sails",
        cor: [40, 72, 168],
        preco_tp: 150,
    },
    Vela {
        item: item_id::VELA_BASE + 3,
        nome: "Emerald Sails",
        cor: [36, 132, 84],
        preco_tp: 150,
    },
    Vela {
        item: item_id::VELA_BASE + 4,
        nome: "Golden Sails",
        cor: [214, 170, 58],
        preco_tp: 300,
    },
];

/// The plain canvas (no sail item applied).
pub const VELA_PADRAO: [u8; 3] = [232, 224, 202];

/// Figureheads: (item, English name, TP price). Append only.
pub const FIGURAS: [(u16, &str, u64); 3] = [
    (item_id::FIGURA_BASE, "Mermaid Figurehead", 250),
    (item_id::FIGURA_BASE + 1, "Dragon Figurehead", 300),
    (item_id::FIGURA_BASE + 2, "Eagle Figurehead", 250),
];

/// The Naval Quartermaster's vendor id and (item, Naval Token price) catalog.
pub const LOJA_NAVAL: u32 = 90_476;
pub const TROCAS_NAVAIS: &[(u16, u32)] = &[
    (item_id::VELA_BASE, 15),
    (item_id::VELA_BASE + 1, 20),
    (item_id::VELA_BASE + 2, 15),
    (item_id::VELA_BASE + 3, 15),
    (item_id::VELA_BASE + 4, 30),
    (item_id::FIGURA_BASE, 25),
    (item_id::FIGURA_BASE + 1, 30),
    (item_id::FIGURA_BASE + 2, 25),
    (item_id::PERGAMINHO_INVOCA_PET, 50),
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
    FIGURAS
        .iter()
        .position(|f| f.0 == id)
        .map(|i| Cosmetico::Figura(i as u8 + 1))
}

/// English name of a cosmetic item.
pub fn nome_do_cosmetico(id: u16) -> Option<&'static str> {
    VELAS
        .iter()
        .find(|v| v.item == id)
        .map(|v| v.nome)
        .or_else(|| FIGURAS.iter().find(|f| f.0 == id).map(|f| f.1))
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
    vela.checked_sub(1)
        .and_then(|i| VELAS.get(i as usize))
        .map_or(VELA_PADRAO, |v| v.cor)
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
            classe: classe_do_navio(id, d) as u8,
            vela: d.vela,
            figura: d.figura,
            estrelas: if tipo_novo(d).is_some() {
                d.tier.saturating_sub(1).saturating_mul(5)
            } else {
                d.estrelas.iter().map(|e| (*e).min(ESTRELAS_MAX)).sum()
            },
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

/// World bosses drop a Colossus Chest for the ship's deck? OFF since
/// 17/11/2026: bosses drop only their normal loot again.
pub const BAU_DO_COLOSSO_LIGADO: bool = false;

/// What a ship skill does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Efeito {
    /// Every enemy within `alcance` off the ship's beam (its sides), in a
    /// fan of `LEQUE_DA_BORDADA` balls (`aberturas_do_leque`).
    Bordada,
    /// One shot at the selected target within `alcance`, flying fast
    /// (`tempo_de_voo_rapido`).
    Canhonaco,
    /// Heals `REPARO_FRACAO` of the full hull.
    Reparo,
    /// Untargetable for `FUMACA_S`, faster for `FUMACA_VELOCIDADE_S`.
    Fumaca,
    /// One low-damage shot; on a hull hit the enemy ship sails at
    /// `CORRENTE_MULT` speed for `CORRENTE_S` (PvP naval, docs/PVP-NAVAL.md).
    Corrente,
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

/// THE SHIP'S BAR: skill slots 1-5 at sea.
pub const HABILIDADES: [HabilidadeDoNavio; 5] = [
    HabilidadeDoNavio {
        id: 0,
        nome: "Broadside",
        descricao: "Both sides fire a fan of five balls per enemy within 12 m off the ship's sides.",
        efeito: Efeito::Bordada,
        espera_s: 6.0,
        alcance: 12.0,
        // Gameplay remake (16/11/2026): 1.5 with the sea mobs' tougher hide
        // (`VIDA_DO_BICHO_DO_MAR`): a same-level mob takes 3-5 Broadsides.
        // PvP naval (18/11/2026): the damage is the SUM of the fan's balls
        // (`LEQUE_DA_BORDADA`) — all five landing is the old full hit.
        mult: 1.5,
        auto: true,
    },
    HabilidadeDoNavio {
        id: 1,
        // 16/11/2026 (owner: "tem algum ataque que sai da frente do barco?
        // se sim tira, tudo tem que sair de lado"): no more bow gun. This is
        // a long SIDE shot: it needs the target off one of the ship's sides.
        nome: "Long Gun",
        descricao: "A fast long-range side shot at your target, up to 22 m away, when it is off one of your sides.",
        efeito: Efeito::Canhonaco,
        // Gameplay remake: the opener and the chaser, not the main gun. At
        // 2.2 every 4 s it out-damaged the Broadside three to one and a
        // fight lasted four seconds (`a_luta_no_mar_dura_dez_a_vinte_e_cinco_s`).
        // 18/11/2026: the ball flies fast (`tempo_de_voo_rapido`), so at
        // range it is a poke you aim, not an arc you wait on.
        espera_s: 5.0,
        alcance: 22.0,
        mult: 0.6,
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
    HabilidadeDoNavio {
        id: 4,
        nome: "Chain Shot",
        descricao: "A chained ball at your target: little damage, but a hull hit slows the enemy ship to half speed for 5 s.",
        efeito: Efeito::Corrente,
        espera_s: 20.0,
        alcance: 18.0,
        mult: 0.4,
        auto: true,
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
    id.checked_sub(ID_SKILL_BASE)
        .and_then(|h| HABILIDADES.get(h as usize))
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
///
/// 0.85 = ±58° round the beam, the front and rear QUARTERS included; the
/// blind cones at the bow and the stern are ±32° (17/11/2026, with the sea
/// mobs coming at the sides; it was 0.8, ±53°).
pub const BORDADA_COS_MAX: f32 = 0.85;
/// Cannon fire against another player's HULL takes this share (hull vs
/// hull; the PK modes decide whether it may happen at all).
pub const PVP_CANHAO: f32 = 1.0;

/// CHAIN SHOT (PvP naval, 18/11/2026): a hull hit slows the enemy ship to
/// this share of its speed for `CORRENTE_S`.
pub const CORRENTE_MULT: f32 = 0.5;
pub const CORRENTE_S: f32 = 5.0;

/// NAVAL DEFEAT (PvP naval phases 2–6).
pub const AFUNDAMENTO_S: f32 = 12.0 * 60.0;
pub const REPARAR_VELA_S: f32 = 45.0;
pub const CASCO_APOS_REPARO: f32 = 0.10;
pub const DERIVA_DERROTADO: f32 = 0.35;
pub const SAQUE_NAVAL_S: f32 = 10.0;
/// Generated PvP wreck loot: gold and the naval items. It is created by the
/// system on the defeated hull and never taken from the victim's inventory.
pub const SAQUE_OURO_MIN: u64 = 500;
pub const SAQUE_OURO_MAX: u64 = 2_000;
pub const SAQUE_TOKEN_MIN: u32 = 1;
pub const SAQUE_TOKEN_MAX: u32 = 3;
pub const SAQUE_FRAGMENTO_MIN: u32 = 1;
pub const SAQUE_FRAGMENTO_MAX: u32 = 2;
/// PvE token source: every sea elite pays one; common sea mobs pay one at this chance.
pub const TOKENS_DO_ELITE: u32 = 1;
pub const CHANCE_TOKEN_DO_BICHO: f32 = 0.08;
/// An unlooted wreck leaves this barrel for anyone who sails close enough.
pub const BARRIL_COLETA_S: f32 = SAQUE_NAVAL_S;
pub const BARRIL_DURA_S: f32 = 15.0 * 60.0;
/// Party ships/decks inside this radius share a completed wreck salvage.
pub const ALCANCE_DIVISAO_SAQUE: f32 = 30.0;
/// NPC role carried in `EntityMeta.kind`, also used by the sea map.
pub const PAPEL_BARRIL_NAVAL: u8 = 124;
pub const RESPAWN_NAVAL_BASE_S: f32 = 5.0;
pub const RESPAWN_NAVAL_RESET_S: f32 = 90.0;
pub const ABORDAGEM_ALCANCE: f32 = 9.0;
/// Reconnection grace before an abandoned hull returns its crew to a dock.
pub const AUTONOMIA_NAVAL_S: f32 = 120.0;
pub const AUTONOMIA_VELOCIDADE: f32 = 0.55;

/// THE NAVEL PvP LOOT ROLL for a defeated hull of character level `nivel`.
/// `rng` returns 0..1 (so tests can pin it). Gold scales a little with the
/// defeated ship's level; tokens and fragments stay in the design's tight
/// 1-3 / 1-2 range.
pub fn saque_naval(nivel: u32, rng: &mut dyn FnMut() -> f32) -> (u64, u32, u32) {
    let mut t = |min: u64, max: u64| {
        let f = rng().clamp(0.0, 1.0) as f64;
        (min as f64 + (max - min) as f64 * f).round() as u64
    };
    let escala = 1.0 + ((nivel.saturating_sub(1)) as f64 / 80.0).min(0.5);
    let ouro = ((t(SAQUE_OURO_MIN, SAQUE_OURO_MAX) as f64) * escala).round() as u64;
    let tokens = t(SAQUE_TOKEN_MIN as u64, SAQUE_TOKEN_MAX as u64) as u32;
    let fragmentos = t(SAQUE_FRAGMENTO_MIN as u64, SAQUE_FRAGMENTO_MAX as u64) as u32;
    (ouro, tokens, fragmentos)
}

/// BROADSIDE FAN (PvP naval): the broadside fires this many balls per
/// target, spread by these angles (radians) around the aimed point. Each
/// ball carries `1/n` of the skill's damage: all five landing is the old
/// full hit, and a turn sheds the outer balls.
pub const LEQUE_DA_BORDADA: usize = 5;

/// The fan's angles around the aim, `LEQUE_DA_BORDADA` of them.
pub fn aberturas_do_leque() -> [f32; LEQUE_DA_BORDADA] {
    [-0.16, -0.08, 0.0, 0.08, 0.16]
}

// ─────────────────────── gameplay remake (16/11/2026) ───────────────────────

/// CANNONBALLS FLY (docs/NAVEGACAO.md, "gameplay remake"): the hit lands
/// when the ball does, not when the button is pressed. Units per second.
pub const VELOCIDADE_DA_BALA: f32 = 30.0;
/// Shortest and longest flight (a ball point-blank still arcs; a long shot
/// never hangs in the air).
pub const VOO_MIN_S: f32 = 0.3;
pub const VOO_MAX_S: f32 = 1.1;
/// A ball hits whoever is within this of where it lands (a ship adds its
/// hull radius). Further than that it is a splash: the target sailed out of
/// it, and that is what dodging a cannon looks like.
pub const RAIO_DO_ACERTO: f32 = 2.4;

/// Seconds a cannonball takes to cross `dist` units. Client and server read
/// the same number: the ball on screen lands when the hit does.
pub fn tempo_de_voo(dist: f32) -> f32 {
    (VOO_MIN_S + dist.max(0.0) / VELOCIDADE_DA_BALA).min(VOO_MAX_S)
}

/// THE LONG GUN'S FAST BALL (18/11/2026): twice the speed, a lower ceiling,
/// so the long poke reads as a flat, quick shot rather than a high arc.
pub const VELOCIDADE_DA_BALA_RAPIDA: f32 = 60.0;
pub const VOO_MAX_RAPIDO_S: f32 = 0.7;

/// Flight time of the Long Gun's ball over `dist` units. Same rule as
/// `tempo_de_voo`: client and server read the same number.
pub fn tempo_de_voo_rapido(dist: f32) -> f32 {
    (VOO_MIN_S + dist.max(0.0) / VELOCIDADE_DA_BALA_RAPIDA).min(VOO_MAX_RAPIDO_S)
}

/// Flight time of skill `hab`'s ball over `dist` units.
pub fn tempo_de_voo_da_skill(hab: &HabilidadeDoNavio, dist: f32) -> f32 {
    match hab.efeito {
        Efeito::Canhonaco => tempo_de_voo_rapido(dist),
        _ => tempo_de_voo(dist),
    }
}

/// Where to aim at a target at `alvo` moving at `vel`: where it will be when
/// the ball lands (one step of lead, enough for a mob or a ship holding its
/// course; turning still dodges).
pub fn mira_com_avanco(de: glam::Vec2, alvo: glam::Vec2, vel: glam::Vec2) -> glam::Vec2 {
    let t = tempo_de_voo(de.distance(alvo));
    alvo + vel * t
}

/// The same for skill `hab`: the Long Gun's fast ball leads less.
pub fn mira_com_avanco_da_skill(
    hab: &HabilidadeDoNavio,
    de: glam::Vec2,
    alvo: glam::Vec2,
    vel: glam::Vec2,
) -> glam::Vec2 {
    let t = tempo_de_voo_da_skill(hab, de.distance(alvo));
    alvo + vel * t
}

/// Did a ball landing at `onde` hit a body at `corpo` of radius `raio`?
pub fn acertou(onde: glam::Vec2, corpo: glam::Vec2, raio: f32) -> bool {
    onde.distance(corpo) <= RAIO_DO_ACERTO + raio.max(0.0)
}

/// THE BITE IS TELEGRAPHED: a sea mob stops and winds up this long before
/// its jaws land (the land mobs' `MOB_ATTACK_IMPACT_S` is 0.46 s, too quick
/// to read on a ship). The client draws the red ring over the same time;
/// sailing out of reach before it closes dodges the bite.
pub const MORDIDA_PREPARO_S: f32 = 0.9;
/// Radius of the bite telegraph ring on the water (units).
pub const RAIO_DA_MORDIDA: f32 = 2.6;

/// The sea mobs' tougher hide: their life at sea is the land profile's
/// times this. With the Broadside at x1.5 a same-level shark or eel takes
/// 3-5 Broadsides (`os_bichos_do_mar_aguentam_alguns_tiros`).
pub const VIDA_DO_BICHO_DO_MAR: f32 = 1.6;

/// AUTO COMBAT AT SEA stops this far from its target: inside the
/// Broadside's 12 m and the Long Gun's 22 m, outside a bite.
pub const DISTANCIA_DE_COMBATE: f32 = 8.5;

// ─────────── the sea's AUTO COMBAT pilot (16/11/2026, "kite and broadside") ───────────

/// AUTO COMBAT AT SEA is steered by the SERVER (`world/piloto_do_mar.rs`):
/// the client switches it on with `PedidoNavio::Piloto` and keeps choosing
/// the target; the hull circles it at this radius with its side to it
/// (inside the Broadside's 12 m, far outside a bite).
pub const PILOTO_RAIO: f32 = 7.5;
/// The widest orbit the pilot sails (`raio_do_piloto`): a heavy hull that
/// cannot come round at `PILOTO_RAIO` widens up to here, still inside the
/// Broadside's 12 m with the ball's lead.
pub const PILOTO_RAIO_MAX: f32 = 10.0;

/// The pilot's ORBIT for ship `id`/`d`: its turning radius at full speed
/// (`v / giro`), kept between `PILOTO_RAIO` and `PILOTO_RAIO_MAX`. Sloop
/// 7.5 m, Schooner 9.3 m, Galleon and Man-o'-War 10 m.
pub fn raio_do_piloto(id: u16, d: &NavioData) -> f32 {
    (velocidade_do_navio(id, d) / giro_max(id, d).max(0.01)).clamp(PILOTO_RAIO, PILOTO_RAIO_MAX)
}

/// The pilot's speed on that orbit: what the helm can hold round it with a
/// margin (92% of `giro · r`), never above the ship's own. A Man-o'-War
/// slows to ~6.4 u/s to keep its side to the target; a Sloop sails 10.
pub fn velocidade_do_piloto(id: u16, d: &NavioData) -> f32 {
    velocidade_do_navio(id, d).min(giro_max(id, d) * raio_do_piloto(id, d) * 0.92)
}
/// Under this share of the hull the pilot patches it (Emergency Repair).
pub const PILOTO_REPARA: f32 = 0.35;
/// Under this share, with no repair ready, it pops Smoke and runs for the
/// nearest harbour corridor...
pub const PILOTO_RECUA: f32 = 0.20;
/// ...and only comes back above this one.
pub const PILOTO_VOLTA: f32 = 0.50;

/// Is the sea's AUTO COMBAT retreating? Hull share `fracao`, Emergency
/// Repair ready or not, and whether it was already retreating (the client
/// reads the same rule to stop hunting while the server sails home).
pub fn piloto_recua(fracao: f32, reparo_pronto: bool, recuando: bool) -> bool {
    if recuando {
        fracao < PILOTO_VOLTA
    } else {
        fracao < PILOTO_RECUA && !reparo_pronto
    }
}

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

/// Is `alvo` off the beam of a ship at `de` heading `rumo`? Only a ship
/// with no heading at all (`rumo` zero: it never sailed) fires all round;
/// the server passes the hull's own heading (`Leme::rumo`), so a ship lying
/// still fires off its sides too.
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

// ─────────────────────────── phase 4: the treasure ───────────────────────────

/// The payout of a Colossus Chest opened where it dropped.
pub const MULT_NA_ORIGEM: f32 = 1.0;
/// Each distinct island docked at by sea with it on deck adds this...
pub const MULT_POR_ILHA: f32 = 0.6;
/// ...up to this (the Corsair's treasure passives add on top).
pub const MULT_MAX: f32 = 3.0;
/// After the deck empties at sea (sunk, killed, given away) the ship stays
/// marked this long: dropping the treasure a second before the hit is not
/// a dodge.
pub const TRILHA_DA_MARCA_S: f32 = 60.0;
/// A sunk carrier's treasure floats at the wreck this long; unclaimed, it
/// drifts to the nearest drifting-chest spot and waits there (never
/// destroyed).
pub const BOIA_S: f32 = 120.0;
/// Seconds to haul a floating treasure aboard (the Corsair shortens it).
pub const PEGAR_S: f32 = 4.0;
/// How close to the floating treasure the ship has to stay while hauling.
pub const ALCANCE_DO_TESOURO: f32 = 6.0;
/// `ColetaEstado.tipo` while hauling a treasure aboard.
pub const TIPO_COLETA_TESOURO: u8 = crate::forte::TIPO_COLETA + 10;
/// `ColetaEstado.tipo` while repairing a defeated ship's sail.
pub const TIPO_COLETA_REPARO_VELA: u8 = crate::forte::TIPO_COLETA + 11;
/// `ColetaEstado.tipo` while looting a defeated enemy hull.
pub const TIPO_COLETA_SAQUE: u8 = crate::forte::TIPO_COLETA + 12;
/// Channel names for the sea HUD.
pub const NOME_DO_REPARO_VELA: &str = "Sail repair";
pub const NOME_DO_SAQUE: &str = "Wreck salvage";
/// Wreck Planks in a Colossus Chest at x1.0.
pub const PRANCHAS_DO_COLOSSO: u32 = 10;
/// English name of the chest.
pub const NOME_DO_COLOSSO: &str = "Colossus Chest";

/// A new Colossus Chest from world boss `chefe` (level `nivel`) on island
/// `origem` (`ARQUIPELAGO` index; `None` = not an island of the map).
pub fn tesouro_novo(chefe: u16, nivel: u32, origem: Option<usize>) -> crate::items::Tesouro {
    crate::items::Tesouro {
        chefe,
        nivel: nivel.min(u16::MAX as u32) as u16,
        origem: origem.filter(|i| *i < 255).map_or(255, |i| i as u8),
        ilhas: 0,
    }
}

/// Distinct islands counted on the chest.
pub fn ilhas_cruzadas(t: &crate::items::Tesouro) -> u32 {
    t.ilhas.count_ones()
}

/// THE PAYOUT MULTIPLIER: x1.0 where it dropped, +0.6 per distinct island
/// docked at by sea with it on deck (x1.6, x2.2, x2.8), capped at x3.0;
/// `bonus` (the Corsair's `Passivas::mult_tesouro`) on top of the cap.
pub fn mult_do_tesouro(t: &crate::items::Tesouro, bonus: f32) -> f32 {
    (MULT_NA_ORIGEM + MULT_POR_ILHA * ilhas_cruzadas(t) as f32).min(MULT_MAX) + bonus.max(0.0)
}

/// DOCKING with the chest on deck at island `ilha`, having sailed from
/// zone `de` (the zone the ship cast off from: `characters.zona_volta`).
/// The island counts only after a real CROSSING: docking back where you
/// cast off from counts nothing. Returns whether it counted now.
pub fn conta_atracacao(t: &mut crate::items::Tesouro, ilha: usize, de: &str) -> bool {
    let Some(zona) = crate::mar::zona_do_indice(ilha) else {
        return false;
    };
    if ilha >= 8 || zona == de {
        return false;
    }
    let bit = 1u8 << ilha;
    let novo = t.ilhas & bit == 0;
    t.ilhas |= bit;
    novo
}

/// IS THE SHIP MARKED? Derived, never stored: treasure on deck, or the
/// trail of `TRILHA_DA_MARCA_S` after the deck emptied (`marca_ate`, in the
/// sea process's clock), or the Corsair's Pirate's Code. Only at sea.
pub fn marcado(
    no_mar: bool,
    conves: bool,
    agora: f32,
    marca_ate: f32,
    codigo_pirata: bool,
) -> bool {
    no_mar && (conves || agora < marca_ate || codigo_pirata)
}

/// Is this item a ship with TREASURE ON DECK? Such a ship never leaves the
/// Ship slot: the bag, the bank, the market, a trade and the Mail all refuse
/// it. That is what makes "take it to another island" mean sailing.
pub fn preso_ao_conves(item_id: u16, inst: Option<&ItemInstance>) -> bool {
    item_id::e_navio(item_id)
        && inst
            .and_then(|i| i.navio)
            .is_some_and(|d| d.conves.is_some())
}

/// Seconds to haul a floating treasure aboard with the Corsair's
/// `Passivas::pegar_tesouro` speed bonus.
pub fn tempo_de_pegar(bonus: f32) -> f32 {
    PEGAR_S / (1.0 + bonus.max(0.0))
}

/// WHAT A COLOSSUS CHEST HOLDS at multiplier `mult`: the boss's chest table
/// (`forte::conteudo_do_chefe`, the green stage, like the field boss's own
/// chests) with every stackable SCALED by `mult` (gold, copper, Darksteel,
/// Energy, materials, Storm Marks), the gear piece the table may roll (one,
/// unscaled), a craft key at the purple chest's 50% (a second roll from
/// x2.0), and `PRANCHAS_DO_COLOSSO` x mult Wreck Planks.
pub fn conteudo_do_colosso(
    t: &crate::items::Tesouro,
    mult: f32,
    rng: &mut dyn FnMut() -> f32,
) -> Vec<crate::dungeon::Premio> {
    use crate::dungeon::Premio;
    let escala = |q: u32| ((q as f32 * mult).round() as u32).max(q.min(1));
    let pranchas = Premio {
        item_id: item_id::PRANCHA_DE_NAUFRAGIO,
        qtd: escala(PRANCHAS_DO_COLOSSO),
        peca: None,
    };
    let Some(c) = crate::dungeon::conteudo(crate::forte::conteudo_do_chefe(t.chefe)) else {
        return vec![pranchas];
    };
    let bau = crate::dungeon::rolar_bau(c, 1, false, false, rng);
    let chaves = item_id::todas_as_chaves();
    let mut v: Vec<Premio> = bau
        .itens
        .into_iter()
        .filter(|p| !chaves.contains(&p.item_id))
        .map(|p| {
            if p.peca.is_some() {
                p
            } else {
                Premio {
                    qtd: escala(p.qtd),
                    ..p
                }
            }
        })
        .collect();
    if bau.marcas > 0 {
        v.push(Premio {
            item_id: item_id::MARCAS_TEMPESTADE,
            qtd: escala(bau.marcas),
            peca: None,
        });
    }
    let rolagens = if mult >= 2.0 { 2 } else { 1 };
    for _ in 0..rolagens {
        if let Some(k) = crate::forte::chave_do_bau(4, c.nivel_min, rng(), rng()) {
            v.push(Premio {
                item_id: k,
                qtd: 1,
                peca: None,
            });
        }
    }
    v.push(pranchas);
    v
}

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
    /// On land: open the Colossus Chest on deck (naval phase 4), paying its
    /// multiplier (`mult_do_tesouro`). Appended.
    AbrirTesouro,
    /// At sea: AUTO COMBAT on/off. On, the server steers the hull around the
    /// selected target and fires the cannons (`world/piloto_do_mar.rs`).
    /// Appended (protocol stays 199).
    Piloto { ligado: bool },
    /// At sea, on my own deck: take (`pegar`) or leave the HELM ("Convés
    /// andável"). Only the captain, near the wheel. Appended.
    Leme { pegar: bool },
    /// At sea: board the ship of party member `capitao` (their ship's
    /// entity), close by. My own ship is stowed. Appended.
    Embarcar {
        capitao: crate::components::EntityId,
    },
    /// Craft de um dos nove cascos T1 (`TipoBarco::de_id`). Appended.
    ConstruirTipo { tipo: u8 },
    /// Aplica uma build sem remover pontos já alocados. Appended.
    Alocar { pontos: [u8; 4] },
    /// Zera a build por ouro, permitindo realocar. Appended.
    Respec,
    /// Equipado + um casco idêntico na bolsa + ouro = próximo tier. Appended.
    UpgradeTier,
    /// Give up a defeated hull immediately. The crew takes the real naval
    /// death penalty and returns to the last dock.
    Afundar,
    /// Start/cancel the 45 s sail repair on my defeated hull.
    RepararVela { iniciar: bool },
    /// Loot the defeated enemy hull whose deck I am standing on.
    Saquear,
    /// TERRITORIES: step off the deck onto the territory islet's shore
    /// beside it (the ship near still). Appended.
    Desembarcar,
    /// TERRITORIES: from the islet's shore back onto my ship (or a party
    /// ship) beside it. Appended.
    VoltarAoNavio,
    /// TERRITORIES (phase 3): claim the free islet I stand on, at its
    /// centre, for my clan (leader or officer). Appended.
    ReivindicarIlhota,
    /// TERRITORIES (phase 4): raise track `trilha` (`territorio::trilha`) of
    /// my clan's islet, standing by its Governor or Farmer. Appended.
    MelhorarIlha { trilha: u8 },
    /// TERRITORIES (4.6): an invader, by an enemy islet's Governor in its
    /// siege window, loots a tenth of that clan's bank (once per siege).
    /// Appended.
    SaquearBanco,
    /// TERRITORIES (phase 5): declare war on the enemy islet whose Governor
    /// I stand by (leader or officer; Monday to Friday noon). Appended.
    DeclararGuerra,
    /// TERRITORIES (phase 5): set my clan's islet's war slot (6 Sat / 0
    /// Sun, hour 14-22), at its Governor. Appended.
    AgendarGuerra { dia: u8, hora: u8 },
    /// TERRITORIES (phase 5): put `qtd` of my gold into my clan's bank for
    /// the weekly upkeep, at my clan's islet's Governor. Appended.
    DepositarOuro { qtd: u32 },
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
            return Self {
                zarpar,
                ..Default::default()
            };
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
            assert!(
                c.velocidade > crate::PLAYER_SPEED,
                "a ship slower than walking"
            );
            let fumaca = (velocidade_do_navio(c.item, &cheio) * FUMACA_MULT).min(VELOCIDADE_MAX);
            assert!(fumaca < teto);
        }
    }

    #[test]
    fn os_nove_tipos_usam_tiers_e_pontos_reais() {
        for id in 0..9 {
            let tipo = TipoBarco::de_id(id).unwrap();
            assert_eq!(tipo.id(), id);
            assert_eq!(tipo.capacidade(), [2, 4, 8][tipo.tamanho as usize]);
            let base = stats_finais_novo(tipo, 1, [0; 4]);
            assert!(base.velocidade > crate::PLAYER_SPEED && base.velocidade < VELOCIDADE_MAX);
            assert!(base.hp > 0.0 && base.dano > 0.0 && base.virada > 0.0);
            for tier in 1..=4 {
                let limite = pontos_no_tier(tier).unwrap() as usize;
                let pontos = [limite as u8, 0, 0, 0];
                assert!(valida_pontos(tier, pontos));
                assert!(!valida_pontos(tier, [limite as u8 + 1, 0, 0, 0]));
                let stats = stats_finais_novo(tipo, tier, pontos);
                assert!(stats.velocidade < TETO_DO_FIO);
                assert!(stats.hp <= u16::MAX as f32);
                if tier < 4 {
                    assert!(
                        stats_finais_novo(tipo, tier + 1, [0; 4]).hp
                            > stats_finais_novo(tipo, tier, [0; 4]).hp
                    );
                }
            }
        }
        assert!(!valida_pontos(0, [0; 4]));
        assert!(!valida_pontos(5, [0; 4]));
        assert_eq!(custo_upgrade(4), None);
    }

    #[test]
    fn migracao_preserva_cosmeticos_tesouro_e_a_fracao_do_casco() {
        for id in [
            item_id::NAVIO_CHALUPA,
            item_id::NAVIO_ESCUNA,
            item_id::NAVIO_GALEAO,
            item_id::NAVIO_NAU,
        ] {
            let carga = tesouro_novo(1, 1, Some(0));
            let antigo = NavioData {
                casco: casco_max(id) / 2,
                avariado: true,
                estrelas: [2, 3, 4],
                vela: 2,
                figura: 1,
                conves: Some(carga),
                ..Default::default()
            };
            let d = migrar_antigo(id, &antigo);
            assert!(tipo_novo(&d).is_some());
            assert!(valida_pontos(d.tier, d.pontos));
            assert_eq!(
                d.pontos.iter().map(|p| *p as u16).sum::<u16>(),
                pontos_no_tier(d.tier).unwrap() as u16
            );
            assert_eq!((d.vela, d.figura, d.conves), (2, 1, Some(carga)));
            assert!(d.avariado);
            let esperado = (casco_total(id, &d) / 2) as f32;
            assert!((d.casco as f32 - esperado).abs() <= 1.0);
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
            assert!(
                b.velocidade - a.velocidade <= 1.5,
                "{}: a bit more speed, not a leap",
                b.nome
            );
            assert!(b.nivel > a.nivel && b.canhao > a.canhao);
            assert_eq!(proximo_casco(a.item), Some(b.item));
            let r = receita_depois_de(a.item).unwrap();
            assert!(r
                .materiais
                .iter()
                .any(|(i, q)| *i == item_id::PRANCHA_DE_NAUFRAGIO && *q > 0));
        }
        assert_eq!(proximo_casco(item_id::NAVIO_NAU), None);
        assert!(CASCOS.iter().all(|c| item_id::e_navio(c.item)));
        assert_eq!(
            CASCOS.len(),
            (item_id::NAVIO_ULTIMO - item_id::NAVIO_CHALUPA + 1) as usize
        );
        // The crossing stays sensible: a 600 u crossing at the slowest and
        // the fastest hull.
        assert!(600.0 / CASCOS[0].velocidade <= 60.0);
        assert!(600.0 / VELOCIDADE_MAX >= 30.0);
    }

    /// STARS: rising cost, at most five, never Essence; each axis grows
    /// what it says.
    #[test]
    fn estrelas_custam_cada_vez_mais_e_param_em_cinco() {
        let mut antes = CustoDaEstrela {
            cobre: 0,
            darksteel: 0,
            pranchas: 0,
        };
        for e in 0..ESTRELAS_MAX {
            let c = custo_da_estrela(e).unwrap();
            assert!(
                c.cobre > antes.cobre
                    && c.darksteel > antes.darksteel
                    && c.pranchas > antes.pranchas
            );
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
        inst.navio = Some(NavioData {
            estrelas: [3, 1, 5],
            vela: 2,
            figura: 1,
            ..novo(item_id::NAVIO_GALEAO)
        });
        let json = serde_json::to_string(&inst).unwrap();
        let volta: ItemInstance = serde_json::from_str(&json).unwrap();
        assert_eq!(volta.navio, inst.navio);
        let bytes = postcard::to_allocvec(&inst).unwrap();
        let volta: ItemInstance = postcard::from_bytes(&bytes).unwrap();
        assert_eq!(volta.navio.unwrap().estrelas, [3, 1, 5]);
    }

    #[test]
    fn reconstruir_leva_estrelas_e_cosmeticos_com_o_casco_cheio() {
        let carga = tesouro_novo(10, 12, Some(0));
        let velho = NavioData {
            casco: 10,
            avariado: true,
            estrelas: [2, 3, 4],
            vela: 1,
            figura: 2,
            conves: Some(carga),
            ..Default::default()
        };
        let d = reconstruido(item_id::NAVIO_ESCUNA, &velho);
        assert_eq!(d.estrelas, [2, 3, 4]);
        assert_eq!(d.conves, Some(carga), "the treasure moves to the new hull");
        assert_eq!((d.vela, d.figura), (1, 2));
        assert!(!d.avariado);
        assert_eq!(d.casco, casco_total(item_id::NAVIO_ESCUNA, &d));
    }

    #[test]
    fn reparo_cobra_o_que_falta_e_nada_cheio() {
        let id = item_id::NAVIO_CHALUPA;
        assert_eq!(custo_do_reparo(id, &novo(id)), 0);
        let afundado = NavioData {
            casco: 0,
            avariado: true,
            ..Default::default()
        };
        assert_eq!(custo_do_reparo(id, &afundado), 240);
        assert!(!pode_zarpar(&afundado));
        assert!(pode_zarpar(&novo(id)));
        assert_eq!(
            dados(None, id),
            migrar_antigo(id, &novo(id)),
            "no state reads as a migrated new hull"
        );
        // Old hull stars become build points, which raise the full repair.
        let estrelado = NavioData {
            casco: 0,
            avariado: true,
            estrelas: [5, 0, 0],
            ..Default::default()
        };
        let migrado = migrar_antigo(id, &estrelado);
        assert_eq!(custo_do_reparo(id, &migrado), 202);
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
        assert_eq!(
            fio & 0xFF_FFFF_FFFF,
            0,
            "bytes 0-4 are the weapon and mount skins"
        );
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
            assert_eq!(
                h.auto,
                h.mult > 0.0,
                "{}: AUTO only for the attacks",
                h.nome
            );
        }
        let bordada = &HABILIDADES[0];
        let sloop = dano_do_canhao(
            bordada,
            30,
            item_id::NAVIO_CHALUPA,
            &novo(item_id::NAVIO_CHALUPA),
        );
        let nau = dano_do_canhao(
            bordada,
            30,
            item_id::NAVIO_NAU,
            &com_estrelas(item_id::NAVIO_NAU, [0, 0, 5]),
        );
        assert!(nau > sloop * 2, "{sloop} vs {nau}");
        assert!(
            HABILIDADES[2].espera_s >= 45.0,
            "Emergency Repair has a long cooldown"
        );
        assert!(no_costado(
            glam::Vec2::ZERO,
            glam::Vec2::Y,
            glam::Vec2::X * 5.0
        ));
        assert!(
            !no_costado(glam::Vec2::ZERO, glam::Vec2::Y, glam::Vec2::Y * 5.0),
            "the bow is blind"
        );
        assert!(
            !no_costado(glam::Vec2::ZERO, glam::Vec2::Y, -glam::Vec2::Y * 5.0),
            "the stern is blind"
        );
        // The arc: ±58° round the beam, so the quarters count (35° off the
        // bow or the stern bears, 28° does not).
        let a = |graus: f32| {
            glam::Vec2::from_angle(std::f32::consts::FRAC_PI_2 - graus.to_radians()) * 6.0
        };
        for graus in [35.0f32, 60.0, 90.0, 120.0, 145.0, -35.0, -145.0] {
            assert!(
                no_costado(glam::Vec2::ZERO, glam::Vec2::Y, a(graus)),
                "{graus}° off the bow should bear"
            );
        }
        for graus in [0.0f32, 28.0, 152.0, 180.0, -28.0, -152.0] {
            assert!(
                !no_costado(glam::Vec2::ZERO, glam::Vec2::Y, a(graus)),
                "{graus}° off the bow should be blind"
            );
        }
        assert!(
            no_costado(glam::Vec2::ZERO, glam::Vec2::ZERO, glam::Vec2::Y * 5.0),
            "standing still fires all round"
        );
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
        let tiros = |vida: i32, dano: i32, defesa: i32| {
            (vida as f32 / damage(dano, defesa) as f32).ceil() as u32
        };
        for nivel in [1u32, 5, 15, 30, 45, 60] {
            for perfil in [
                Profile::relativo_ao_lobo(120, 10, 0),
                Profile::relativo_ao_lobo(95, 15, 2),
            ] {
                let mut m = mob(&perfil, nivel);
                m.health = (m.health as f32 * VIDA_DO_BICHO_DO_MAR).round() as i32;
                let sloop = dano_do_canhao(
                    bordada,
                    nivel,
                    item_id::NAVIO_CHALUPA,
                    &novo(item_id::NAVIO_CHALUPA),
                );
                let n = tiros(m.health, sloop, m.defense);
                assert!(
                    (3..=5).contains(&n),
                    "level {nivel}: {n} broadsides from a Sloop for {} life",
                    m.health
                );
                let nau = dano_do_canhao(
                    bordada,
                    nivel,
                    item_id::NAVIO_NAU,
                    &com_estrelas(item_id::NAVIO_NAU, [0, 0, 5]),
                );
                assert!(
                    tiros(m.health, nau, m.defense) < n,
                    "level {nivel}: the Man-o'-War is no better"
                );
                let elite = tiros(m.health * 5, sloop, m.defense);
                assert!((8..=30).contains(&elite), "level {nivel}: elite in {elite}");
            }
        }
    }

    /// THE FIGHT AT SEA LASTS 10-25 s (gameplay remake): a bare Sloop firing
    /// both cannons the moment they come off cooldown sinks a same-level
    /// shark or eel in that window — long enough to read, short enough to
    /// keep sailing.
    #[test]
    fn a_luta_no_mar_dura_dez_a_vinte_e_cinco_s() {
        use crate::ladder::{damage, mob, Profile};
        let id = item_id::NAVIO_CHALUPA;
        for nivel in [1u32, 5, 15, 30, 45, 60] {
            for perfil in [
                Profile::relativo_ao_lobo(120, 10, 0),
                Profile::relativo_ao_lobo(95, 15, 2),
            ] {
                let m = mob(&perfil, nivel);
                let mut vida = (m.health as f32 * VIDA_DO_BICHO_DO_MAR).round() as i32;
                let b = damage(
                    dano_do_canhao(&HABILIDADES[0], nivel, id, &novo(id)),
                    m.defense,
                );
                let c = damage(
                    dano_do_canhao(&HABILIDADES[1], nivel, id, &novo(id)),
                    m.defense,
                );
                let (mut pronta_b, mut pronta_c, mut t) = (0.0f32, 0.0f32, 0.0f32);
                loop {
                    if t >= pronta_b - 1e-4 {
                        vida -= b;
                        pronta_b = t + HABILIDADES[0].espera_s;
                    }
                    if vida > 0 && t >= pronta_c - 1e-4 {
                        vida -= c;
                        pronta_c = t + HABILIDADES[1].espera_s;
                    }
                    if vida <= 0 {
                        break;
                    }
                    t += 0.5;
                }
                assert!(
                    (10.0..=25.0).contains(&t),
                    "level {nivel}: the fight took {t:.1} s"
                );
            }
        }
    }

    /// Cannonballs fly: longer shots take longer, within the bounds, and the
    /// lead puts the aim where a moving target will be.
    #[test]
    fn a_bala_voa_e_a_mira_adianta() {
        assert!((tempo_de_voo(0.0) - VOO_MIN_S).abs() < 1e-6);
        assert!(tempo_de_voo(12.0) > tempo_de_voo(4.0));
        assert!(tempo_de_voo(500.0) <= VOO_MAX_S);
        let de = glam::Vec2::ZERO;
        let alvo = glam::Vec2::new(10.0, 0.0);
        let vel = glam::Vec2::new(0.0, 5.0);
        let mira = mira_com_avanco(de, alvo, vel);
        assert!((mira.y - 5.0 * tempo_de_voo(10.0)).abs() < 1e-4);
        assert!(acertou(mira, mira + glam::Vec2::X * 2.0, 0.0));
        assert!(
            !acertou(mira, mira + glam::Vec2::X * 5.0, 0.0),
            "a dodge is a splash"
        );
        assert!(
            acertou(mira, mira + glam::Vec2::X * 3.5, RAIO_DO_CASCO),
            "a hull is a bigger target"
        );
        assert!(
            MORDIDA_PREPARO_S > crate::constants::MOB_ATTACK_IMPACT_S,
            "the bite telegraph is readable"
        );
        assert!(
            DISTANCIA_DE_COMBATE < HABILIDADES[0].alcance && DISTANCIA_DE_COMBATE > RAIO_DA_MORDIDA
        );
    }

    /// THE BROADSIDE FAN (PvP naval): five evenly spread balls round the
    /// aim, symmetric, none wider than the blind-cone geometry makes
    /// pointless, and each carries its share of the damage.
    #[test]
    fn o_leque_da_bordada_abre_em_cinco() {
        let ab = aberturas_do_leque();
        assert_eq!(ab.len(), LEQUE_DA_BORDADA);
        assert!((ab[0] + ab[LEQUE_DA_BORDADA - 1]).abs() < 1e-6, "symmetric");
        assert!(ab[1..].windows(1).count() + 1 == LEQUE_DA_BORDADA);
        for w in ab.windows(2) {
            assert!(w[1] > w[0], "sorted and distinct");
            assert!((w[1] - w[0] - 0.08).abs() < 1e-6, "even spread");
        }
        // The outermost ball still lands within a hull's reach of the aim at
        // the Broadside's own range: at 12 m, ±0.16 rad is ~1.9 m of spread.
        let spread = (0.16f32).sin() * 12.0;
        assert!(spread < RAIO_DO_ACERTO + RAIO_DO_CASCO);
    }

    /// THE LONG GUN'S FAST BALL: quicker than the arc at every distance and
    /// never hanging past its own ceiling; the skill table picks it.
    #[test]
    fn a_longa_voa_plana_e_rapida() {
        for d in [0.0f32, 5.0, 12.0, 22.0, 100.0] {
            assert!(tempo_de_voo_rapido(d) <= tempo_de_voo(d));
            assert!(tempo_de_voo_rapido(d) <= VOO_MAX_RAPIDO_S + 1e-6);
        }
        let longa = &HABILIDADES[1];
        assert_eq!(longa.efeito, Efeito::Canhonaco);
        assert!((tempo_de_voo_da_skill(longa, 22.0) - tempo_de_voo_rapido(22.0)).abs() < 1e-6);
        assert!((tempo_de_voo_da_skill(&HABILIDADES[0], 22.0) - tempo_de_voo(22.0)).abs() < 1e-6);
        // Less lead: a fast ball aims closer to where the target is.
        let de = glam::Vec2::ZERO;
        let alvo = glam::Vec2::new(18.0, 0.0);
        let vel = glam::Vec2::new(0.0, 5.0);
        let mira = mira_com_avanco_da_skill(longa, de, alvo, vel);
        assert!((mira.y - 5.0 * tempo_de_voo_rapido(18.0)).abs() < 1e-4);
        assert!(mira.y < mira_com_avanco(de, alvo, vel).y);
    }

    /// CHAIN SHOT: the fifth slot, an AUTO attack with low damage, a long
    /// cooldown, and a slow of exactly half speed for five seconds.
    #[test]
    fn o_tiro_de_corrente_trava_o_casco() {
        let c = &HABILIDADES[4];
        assert_eq!(c.efeito, Efeito::Corrente);
        assert_eq!(c.id, 4);
        assert!(c.auto, "an attack, so AUTO");
        assert!(
            c.mult < HABILIDADES[1].mult,
            "less damage than the Long Gun"
        );
        assert!(c.espera_s >= 15.0, "a long cooldown");
        assert!(c.alcance > HABILIDADES[0].alcance && c.alcance <= HABILIDADES[1].alcance);
        assert_eq!(CORRENTE_MULT, 0.5);
        assert_eq!(CORRENTE_S, 5.0);
        assert_eq!(habilidade_do_id(id_de_skill(4)), Some(c));
        // Five skills, five distinct ids.
        for (i, h) in HABILIDADES.iter().enumerate() {
            assert_eq!(h.id as usize, i);
        }
    }

    /// THE PvP WRECK REWARD stays inside the design band and scales a little
    /// with the defeated sailor's level. The three rolls are gold, tokens and
    /// fragments (tests pin them in that order).
    #[test]
    fn o_saque_naval_fica_na_faixa_do_design() {
        let mut vals = [0.0f32, 1.0, 0.5].into_iter();
        let (ouro, tokens, fragmentos) = saque_naval(1, &mut || vals.next().unwrap());
        assert_eq!(ouro, SAQUE_OURO_MIN);
        assert_eq!(tokens, SAQUE_TOKEN_MAX);
        assert_eq!(
            fragmentos,
            (SAQUE_FRAGMENTO_MIN + SAQUE_FRAGMENTO_MAX + 1) / 2
        );
        let mut vals = [1.0f32, 0.0, 0.0].into_iter();
        let (ouro, tokens, fragmentos) = saque_naval(80, &mut || vals.next().unwrap());
        assert!(
            ouro > SAQUE_OURO_MAX,
            "a high-level wreck pays a little more"
        );
        assert_eq!(tokens, SAQUE_TOKEN_MIN);
        assert_eq!(fragmentos, SAQUE_FRAGMENTO_MIN);
        assert!(TIPO_COLETA_REPARO_VELA > TIPO_COLETA_TESOURO);
        assert!(TIPO_COLETA_SAQUE > TIPO_COLETA_REPARO_VELA);
        assert_eq!(NOME_DO_REPARO_VELA, "Sail repair");
        assert_eq!(NOME_DO_SAQUE, "Wreck salvage");
    }

    // ───────────────────── phase 4 ─────────────────────

    /// THE PAYOUT grows with each DISTINCT island docked at after a real
    /// crossing, and is capped; the Corsair bonus goes on top.
    #[test]
    fn o_multiplicador_cresce_com_as_ilhas_e_tem_teto() {
        let mut t = tesouro_novo(10, 12, Some(1));
        assert_eq!(mult_do_tesouro(&t, 0.0), 1.0, "opened where it dropped");
        // Docking back where you cast off counts nothing.
        assert!(!conta_atracacao(&mut t, 0, "ilha_inicial"));
        assert_eq!(mult_do_tesouro(&t, 0.0), 1.0);
        assert!(conta_atracacao(&mut t, 0, "ilha_gelo"));
        assert!((mult_do_tesouro(&t, 0.0) - 1.6).abs() < 1e-5);
        // The same island twice is one island.
        assert!(!conta_atracacao(&mut t, 0, "ilha_deserto"));
        assert!((mult_do_tesouro(&t, 0.0) - 1.6).abs() < 1e-5);
        assert!(conta_atracacao(&mut t, 2, "ilha_inicial"));
        assert!((mult_do_tesouro(&t, 0.0) - 2.2).abs() < 1e-5);
        assert!(conta_atracacao(&mut t, 3, "ilha_deserto"));
        assert!(
            conta_atracacao(&mut t, 1, "ilha_planalto"),
            "back at the origin after a crossing counts"
        );
        assert_eq!(ilhas_cruzadas(&t), 4);
        assert_eq!(mult_do_tesouro(&t, 0.0), MULT_MAX, "capped");
        assert!(
            (mult_do_tesouro(&t, 0.2) - 3.2).abs() < 1e-5,
            "the Corsair adds on top"
        );
        assert!(!conta_atracacao(&mut t, 99, "x"));
    }

    /// THE MARK is derived: treasure on deck, the 60 s trail, or the
    /// Pirate's Code, and only at sea.
    #[test]
    fn a_marca_e_derivada_e_o_rastro_dura_60_s() {
        assert!(marcado(true, true, 0.0, 0.0, false));
        assert!(!marcado(false, true, 0.0, 0.0, false), "never on land");
        assert!(!marcado(true, false, 10.0, 0.0, false));
        let ate = 10.0 + TRILHA_DA_MARCA_S;
        assert!(marcado(true, false, 10.0 + 59.9, ate, false));
        assert!(!marcado(true, false, 10.0 + 60.1, ate, false));
        assert!(marcado(true, false, 1e6, 0.0, true), "the Pirate's Code");
        assert_eq!(TRILHA_DA_MARCA_S, 60.0);
        assert!(tempo_de_pegar(0.45) < PEGAR_S && tempo_de_pegar(-1.0) == PEGAR_S);
    }

    /// Only a ship WITH treasure is stuck on deck; the chest survives the
    /// wire and the database.
    #[test]
    fn so_navio_com_tesouro_fica_preso() {
        let id = item_id::NAVIO_CHALUPA;
        let mut inst = instancia_nova(id);
        assert!(!preso_ao_conves(id, Some(&inst)));
        inst.navio.as_mut().unwrap().conves = Some(tesouro_novo(12, 20, Some(1)));
        assert!(preso_ao_conves(id, Some(&inst)));
        assert!(!preso_ao_conves(item_id::COPPER, Some(&inst)));
        let volta: ItemInstance =
            postcard::from_bytes(&postcard::to_allocvec(&inst).unwrap()).unwrap();
        assert_eq!(volta.navio.unwrap().conves, inst.navio.unwrap().conves);
        let volta: ItemInstance =
            serde_json::from_str(&serde_json::to_string(&inst).unwrap()).unwrap();
        assert_eq!(volta.navio.unwrap().conves.unwrap().chefe, 12);
    }

    /// THE CONTENTS scale with the multiplier: more gold and planks at x3
    /// than at x1 for the same roll.
    #[test]
    fn o_conteudo_escala_com_o_multiplicador() {
        let c = &crate::bosses::CHEFES[0];
        let t = tesouro_novo(c.kind, c.nivel, Some(0));
        let rolar = |mult: f32| {
            let mut k = 0u32;
            let mut rng = move || {
                k += 1;
                ((k * 37) % 100) as f32 / 100.0
            };
            conteudo_do_colosso(&t, mult, &mut rng)
        };
        let soma = |v: &[crate::dungeon::Premio], id: u16| {
            v.iter()
                .filter(|p| p.item_id == id)
                .map(|p| p.qtd)
                .sum::<u32>()
        };
        let (um, tres) = (rolar(1.0), rolar(3.0));
        assert!(soma(&um, item_id::GOLD) > 0, "{um:?}");
        assert!(soma(&tres, item_id::GOLD) >= soma(&um, item_id::GOLD) * 2);
        assert_eq!(
            soma(&um, item_id::PRANCHA_DE_NAUFRAGIO),
            PRANCHAS_DO_COLOSSO
        );
        assert_eq!(
            soma(&tres, item_id::PRANCHA_DE_NAUFRAGIO),
            PRANCHAS_DO_COLOSSO * 3
        );
        assert!(soma(&tres, item_id::DARKSTEEL) > soma(&um, item_id::DARKSTEEL));
    }

    /// TURNING INERTIA: each bigger hull turns slower and takes longer to
    /// reach speed; the sail stars help a little; the helm never turns
    /// faster than the class allows, slows in a hard turn, ramps its speed
    /// and stays under the wire.
    #[test]
    fn o_navio_vira_com_peso() {
        let dt = crate::TICK_DT;
        for par in CASCOS.windows(2) {
            assert!(
                par[1].giro < par[0].giro,
                "{} turns as fast as {}",
                par[1].nome,
                par[0].nome
            );
            assert!(par[1].arranque_s > par[0].arranque_s);
        }
        for c in CASCOS {
            let d = novo(c.item);
            let g = giro_max(c.item, &d);
            assert!(giro_max(c.item, &com_estrelas(c.item, [0, 5, 0])) > g);
            let v = velocidade_do_navio(c.item, &d);
            // Sailing east at full speed, ordered north: the heading turns
            // at most `g` a tick, and the speed drops while it turns.
            let mut l = Leme {
                proa: 0.0,
                vel: v,
                pronto: true,
            };
            let mut antes = l.proa;
            let mut min_v = v;
            for _ in 0..(3.0 / dt) as usize {
                let vel = l.governa(glam::Vec2::Y, v, v, g, c.arranque_s, dt);
                assert!(
                    ang_dif(l.proa, antes).abs() <= g * dt + 1e-4,
                    "{} snapped",
                    c.nome
                );
                assert!(vel.length() <= VELOCIDADE_MAX + 1e-4);
                antes = l.proa;
                min_v = min_v.min(l.vel);
            }
            assert!(
                (l.proa - std::f32::consts::FRAC_PI_2).abs() < 1e-3,
                "{} never came round",
                c.nome
            );
            assert!(min_v < v * 0.95, "{} did not slow in the turn", c.nome);
            // From a standstill: full speed in about `arranque_s`, not one tick.
            let mut l = Leme::default();
            l.governa(glam::Vec2::X, v, v, g, c.arranque_s, dt);
            assert!(l.vel < v * 0.2, "{} jumped to speed", c.nome);
            for _ in 0..((c.arranque_s + 0.1) / dt) as usize {
                l.governa(glam::Vec2::X, v, v, g, c.arranque_s, dt);
            }
            assert!(
                (l.vel - v).abs() < 0.05,
                "{}: {} after the ramp",
                c.nome,
                l.vel
            );
            // Let go: it coasts to a stop over the same ramp, keeping its heading.
            for _ in 0..((c.arranque_s + 0.1) / dt) as usize {
                l.governa(glam::Vec2::ZERO, 0.0, v, g, c.arranque_s, dt);
            }
            assert_eq!(l.vel, 0.0);
            assert!(l.rumo().is_some_and(|r| r.x > 0.99));
            // The pilot's orbit fits the helm, inside the Broadside's reach.
            let r = raio_do_piloto(c.item, &d);
            assert!((PILOTO_RAIO..=PILOTO_RAIO_MAX).contains(&r));
            assert!(
                velocidade_do_piloto(c.item, &d) / r <= g,
                "{}: the orbit is tighter than the helm",
                c.nome
            );
            assert!(PILOTO_RAIO_MAX + 1.0 < HABILIDADES[0].alcance);
        }
        assert_eq!(
            rotulo_do_giro(CASCOS[0].item, &novo(CASCOS[0].item)),
            "Agile"
        );
        assert_eq!(
            rotulo_do_giro(CASCOS[3].item, &novo(CASCOS[3].item)),
            "Heavy"
        );
    }

    /// THE ROUTE with a turning circle: a point right beside the hull is not
    /// orbited forever — the throttle drops until the turn fits, and the
    /// heaviest hull still reaches every point of a zig-zag.
    #[test]
    fn a_rota_nao_fica_orbitando_o_ponto() {
        let dt = crate::TICK_DT;
        for c in CASCOS {
            let d = novo(c.item);
            let (v, g) = (velocidade_do_navio(c.item, &d), giro_max(c.item, &d));
            let pontos = vec![
                glam::Vec2::new(0.0, 4.0),
                glam::Vec2::new(20.0, 4.0),
                glam::Vec2::new(20.0, -10.0),
                glam::Vec2::new(-5.0, -10.0),
            ];
            let fim = *pontos.last().unwrap();
            let mut seg =
                crate::terreno::SeguidorDeRota::nova(pontos, fim).com_folga(FOLGA_DA_ROTA);
            // Full speed, east, with the first point 4 m off the beam.
            let mut l = Leme {
                proa: 0.0,
                vel: v,
                pronto: true,
            };
            let mut p = glam::Vec2::ZERO;
            let mut t = 0.0;
            while !seg.vazia() && t < 60.0 {
                let Some(m) = direcao_na_rota(&mut seg, &l, p, v, g, c.arranque_s) else {
                    break;
                };
                assert!(
                    !seg.travado(),
                    "{}: the turn read as stuck at {p:?}",
                    c.nome
                );
                p += l.governa(m, m.length() * v, v, g, c.arranque_s, dt) * dt;
                t += dt;
            }
            assert!(
                seg.vazia(),
                "{}: still orbiting after {t:.0} s at {p:?}",
                c.nome
            );
            assert!(p.distance(fim) < 1.0, "{}: ended at {p:?}", c.nome);
            assert!(t < 25.0, "{}: {t:.1} s for a 60 m zig-zag", c.nome);
        }
    }
}
