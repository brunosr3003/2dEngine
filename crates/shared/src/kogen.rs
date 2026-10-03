//! KŌGEN-TŌ (`ilha_kogen`): the level 80-100 island, a FANTASY neon city at
//! night, reached only from Skyreach by the flying bus.
//!
//! The owner, 02/10/2026: "a futuristic island ... like Tokyo but in island
//! format ... robots, giant robot bosses ... much light, roads, cars, high
//! buildings". It was real Shinjuku + Shibuya for a day (OpenStreetMap,
//! `tools/kogen-osm`), and on 03/10/2026 he played it: "too much crowded and
//! confuse ... not as fantasy as i expect ... i barely can see my character
//! among the buildings". So it is DRAWN again, for a top-down camera:
//!
//! * a grand BOULEVARD runs the island north to south, its median lined with
//!   glowing sakura and lanterns, five curving CROSS boulevards and gently
//!   bent lanes cut it into big blocks;
//! * by the streets the buildings stay LOW (two to four characters tall),
//!   with open courtyards behind and gaps to walk into them; skyscrapers
//!   stand only at the island's EDGES and at the landmarks;
//! * the Shrine Forest is a park with a neon pagoda, torii gates spanning the
//!   boulevard (`vila` props), and an elevated RING road loops over the
//!   middle of the city, walkable on top and underneath (`terreno::Ilha`'s
//!   second floor), with four ramps.
//!
//! The districts run SOUTH to NORTH as the levels rise: the Docks (where the
//! bus lands), the market streets of Shibuya, the Shrine Forest, Kabukicho,
//! the towers, and the Tocho with the Kōgen Spire at the north tip.
//!
//! The terrain is a heightmap — one top block per column — so a building is a
//! SOLID block of columns that you walk round. `chao_em` / `bloco_da_coluna`
//! are the layout's only source, called by client and server alike.

use glam::Vec2;

pub const ZONA: &str = "ilha_kogen";
/// Bump on any change to the layout: it is in the server's height cache key.
pub const REVISAO: u32 = 12;
/// Planting seed: the relief does not depend on it, the decoration does.
pub const SEMENTE: i32 = 0x0C06_E170;
/// Zone radius in BLOCKS: the island plus a margin of sea.
pub const RAIO_BLOCOS: i32 = 1400;
/// The city's ground, in blocks (sea level is 0).
pub const NIVEL_CHAO: i32 = 20;
/// The sea floor past the shore, in blocks.
pub const NIVEL_FUNDO: i32 = -6;

// ── the island ──
/// The coast: a ragged superellipse, long north-south (half axes, units).
pub const COSTA_A: f32 = 300.0;
pub const COSTA_B: f32 = 640.0;
const COSTA_P: f32 = 3.2;
/// The shore's slope from the city down to the water, in units.
const ORLA: f32 = 10.0;

// ── the Docks' drawn grid (the waterfront where the bus lands) ──
/// The Docks start this far south, in units; the waterfront avenue runs
/// between `AVENIDA_DO_CAIS` and here.
pub const DOCAS_DE: f32 = 515.0;
pub const AVENIDA_DO_CAIS: f32 = 500.0;
pub const QUADRA: f32 = 40.0;
pub const RUA: f32 = 8.0;
pub const AVENIDA: f32 = 14.0;
pub const AVENIDA_A_CADA: i32 = 3;
pub const CALCADA: f32 = 2.5;

// ── the streets ──
/// The grand boulevard: half its width and its median's, in units.
pub const BOULEVARD_MEIA: f32 = 12.0;
pub const CANTEIRO_MEIO: f32 = 2.5;
/// The cross boulevards' resting latitudes (z, units), north to south, and
/// their half width.
pub const CRUZES: [f32; 5] = [-560.0, -350.0, -120.0, 120.0, 380.0];
pub const CRUZ_MEIA: f32 = 8.0;
/// The lanes' grid: one lane every `LOTE` units, `LANE` wide.
const LOTE: f32 = 56.0;
const LANE: f32 = 8.0;
/// A block's ring of shops by the street, in units deep; behind it, the
/// courtyard.
const ANEL: f32 = 10.0;

/// The boulevard's centre line at latitude `z` (it bends gently).
pub fn boulevard_x(z: f32) -> f32 {
    10.0 * (z * 0.006).sin()
}

/// Cross boulevard `k`'s centre line at `x`.
pub fn cruz_z(k: usize, x: f32) -> f32 {
    CRUZES[k] + 8.0 * (x * 0.012 + k as f32).sin()
}

/// A district: its name and its level band.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Distrito {
    pub nome: &'static str,
    pub nivel: (u32, u32),
}

/// The six districts, south to north. Index 0 is the arrival (the hub town).
pub const DISTRITOS: [Distrito; 6] = [
    Distrito { nome: "Harbor Docks", nivel: (80, 83) },
    Distrito { nome: "Shibuya Crossing", nivel: (83, 86) },
    Distrito { nome: "Shrine Forest", nivel: (86, 90) },
    Distrito { nome: "Kabukicho Neon", nivel: (90, 94) },
    Distrito { nome: "Nishi-Shinjuku Towers", nivel: (94, 97) },
    Distrito { nome: "Tocho Spire", nivel: (97, 100) },
];

/// Which district `q` is in, by latitude (south is +z).
pub fn distrito_em(q: Vec2) -> usize {
    match q.y {
        z if z >= AVENIDA_DO_CAIS => 0,
        z if z >= 250.0 => 1,
        z if z >= 20.0 => 2,
        z if z >= -230.0 => 3,
        z if z >= -470.0 => 4,
        _ => 5,
    }
}

/// Where the hub town sits: in the Docks, between the waterfront avenue and
/// the south shore.
pub fn centro_da_cidade() -> Vec2 {
    Vec2::new(0.0, 572.0)
}

/// The scramble crossing: where the boulevard meets the market's cross
/// boulevard, giant screens all round.
pub fn cruzamento() -> Vec2 {
    let z = CRUZES[4];
    Vec2::new(boulevard_x(z), z)
}
const RAIO_DO_CRUZAMENTO: f32 = 30.0;

/// The giant robots' arenas (`bosses`, kinds 63-65), weakest first: a market
/// plaza, a Kabukicho plaza, and the Tocho plaza on the boulevard.
pub fn arenas() -> [Vec2; 3] {
    [Vec2::new(-125.0, 315.0), Vec2::new(125.0, -60.0), Vec2::new(boulevard_x(-505.0), -505.0)]
}
pub const RAIO_ARENA: f32 = 20.0;

/// The Robot Foundry's portal (`porao`, id 7): on a market street.
pub fn portal_da_fundicao() -> Vec2 {
    rua_perto(Vec2::new(-70.0, 300.0))
}

/// Story place ids for Kōgen-tō (`objective_kind::LUGAR`): `PONTO_BASE + i`.
pub const PONTO_BASE: u16 = 80;
pub const NOMES_DOS_PONTOS: [&str; 8] = [
    "the Docks square",
    "the Shibuya crossing",
    "the Shrine Forest",
    "Kabukicho",
    "the towers",
    "the Tocho plaza",
    "Titan Mk-I's plaza",
    "the Neon Kaiju's plaza",
];

/// The street nearest `alvo` (story places must be walkable).
fn rua_perto(alvo: Vec2) -> Vec2 {
    (0..60)
        .flat_map(|r| (0..16).map(move |k| (r, k)))
        .map(|(r, k)| {
            let a = k as f32 / 16.0 * std::f32::consts::TAU;
            alvo + Vec2::new(a.cos(), a.sin()) * r as f32
        })
        .find(|p| matches!(chao_em(*p), Chao::Rua { .. } | Chao::Calcada) && deck_em(*p).is_none())
        .unwrap_or(alvo)
}

/// Where story place `p` is, if it is one of Kōgen-tō's.
pub fn ponto(p: u16) -> Option<Vec2> {
    let i = p.checked_sub(PONTO_BASE)?;
    Some(match i {
        0 => centro_da_cidade(),
        1 => rua_perto(cruzamento() + Vec2::new(0.0, -RAIO_DO_CRUZAMENTO - 6.0)),
        2 => rua_perto(Marco::Pagoda.centro() + Vec2::new(-30.0, 0.0)),
        3 => rua_perto(arenas()[1] + Vec2::new(-30.0, 0.0)),
        4 => rua_perto(Marco::Casulo.centro() + Vec2::new(0.0, Marco::Casulo.alcance() + 6.0)),
        5 => arenas()[2],
        6 => arenas()[0],
        7 => arenas()[1],
        _ => return None,
    })
}

fn hash(x: i32, y: i32) -> f32 {
    let mut h = (x as u32).wrapping_mul(73_856_093) ^ (y as u32).wrapping_mul(19_349_663) ^ 0x9E37_79B9;
    h ^= h >> 13;
    h = h.wrapping_mul(0x5bd1_e995);
    h ^= h >> 15;
    (h & 0xFFFF) as f32 / 65_535.0
}

// ─────────────────────────────── the coast ───────────────────────────────

/// How far out `q` is: below 1 on land. A ragged superellipse.
fn fora(q: Vec2) -> f32 {
    let ang = q.y.atan2(q.x);
    let ruido = 1.0 + 0.045 * (ang * 5.0 + 0.3).sin() + 0.035 * (ang * 9.0 - 1.0).sin() + 0.02 * (ang * 17.0 + 2.0).sin();
    ((q.x.abs() / COSTA_A).powf(COSTA_P) + (q.y.abs() / COSTA_B).powf(COSTA_P)).powf(1.0 / COSTA_P) / ruido
}

/// Units from `q` to the shore, along the ray from the centre.
fn ate_a_costa(q: Vec2) -> f32 {
    let s = fora(q).max(1e-4);
    q.length() * (1.0 / s - 1.0)
}

// ─────────────────────────────── the deck ───────────────────────────────
//
// THE RING ROAD: an ellipse over the middle of the city, sixteen blocks up,
// and four RAMPS leaving it outwards down to the street.

const ANEL_CENTRO: Vec2 = Vec2::new(0.0, -60.0);
const ANEL_A: f32 = 205.0;
const ANEL_B: f32 = 255.0;
const ANEL_MEIA: f32 = 5.0;
const DECK_ALTO: i32 = 16;
const RAMPA_COMPRIMENTO: f32 = 70.0;
const RAMPA_MEIA: f32 = 4.5;

/// The four ramps: (foot on the ring, outward direction).
fn rampas() -> [(Vec2, Vec2); 4] {
    std::array::from_fn(|k| {
        let t = (45.0 + 90.0 * k as f32).to_radians();
        let p = ANEL_CENTRO + Vec2::new(ANEL_A * t.cos(), ANEL_B * t.sin());
        let n = Vec2::new(t.cos() / ANEL_A, t.sin() / ANEL_B).normalize();
        (p, n)
    })
}

/// Distance from `q` to the ring's centre line (signed: + outside).
fn ao_anel(q: Vec2) -> f32 {
    let d = q - ANEL_CENTRO;
    let e = ((d.x / ANEL_A).powi(2) + (d.y / ANEL_B).powi(2)).sqrt().max(1e-4);
    d.length() * (1.0 - 1.0 / e)
}

/// The deck over `q`, in blocks above the street, if any.
pub fn deck_em(q: Vec2) -> Option<i32> {
    let mut h = None;
    if ao_anel(q).abs() <= ANEL_MEIA {
        h = Some(DECK_ALTO);
    }
    for (p, n) in rampas() {
        let t = (q - p).dot(n);
        let s = (q - p).perp_dot(n).abs();
        if (0.0..=RAMPA_COMPRIMENTO).contains(&t) && s <= RAMPA_MEIA {
            let r = (DECK_ALTO as f32 * (1.0 - t / RAMPA_COMPRIMENTO)).round().max(1.0) as i32;
            h = Some(h.map_or(r, |v: i32| v.max(r)));
        }
    }
    h
}

/// The ground under the deck (and a little round its ramps' feet) is street.
fn sob_o_deck(q: Vec2) -> bool {
    if ao_anel(q).abs() <= ANEL_MEIA + 1.5 {
        return true;
    }
    rampas().iter().any(|(p, n)| {
        let t = (q - *p).dot(*n);
        let s = (q - *p).perp_dot(*n).abs();
        (-2.0..=RAMPA_COMPRIMENTO + 8.0).contains(&t) && s <= RAMPA_MEIA + 1.5
    })
}

/// The deck's top block over column `(bx, bz)`, if an expressway passes
/// overhead (`terreno::Ilha`'s second floor).
pub fn deck_da_coluna(bx: i32, bz: i32) -> Option<i32> {
    use crate::terreno::BLOCO;
    let q = Vec2::new(bx as f32, bz as f32) * BLOCO;
    if marco_em(q).is_some() || fora(q) >= 1.0 {
        return None;
    }
    deck_em(q).map(|h| NIVEL_CHAO + h)
}

// ─────────────────────────────── the landmarks ───────────────────────────────

/// A famous building, in voxels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Marco {
    /// The Kōgen Spire (a Skytree): the tallest thing, at the north tip.
    Skytree,
    /// The Tocho Twin Hall: twin towers on a podium, on the Tocho plaza.
    Prefeitura,
    /// Tokyo Tower: red and white, in the Docks.
    TorreDeToquio,
    /// The Cocoon Tower: an egg with a white lattice, in the towers.
    Casulo,
    /// The 109: a silver cylinder at the scramble's corner.
    Cilindro109,
    /// The Neon Pagoda: five tiers in the Shrine Forest.
    Pagoda,
}

impl Marco {
    pub const TODOS: [Marco; 6] =
        [Marco::Skytree, Marco::Prefeitura, Marco::TorreDeToquio, Marco::Casulo, Marco::Cilindro109, Marco::Pagoda];

    pub fn nome(self) -> &'static str {
        match self {
            Marco::Skytree => "Kōgen Spire",
            Marco::Prefeitura => "Tocho Twin Hall",
            Marco::TorreDeToquio => "Harbor Tower",
            Marco::Casulo => "Cocoon Tower",
            Marco::Cilindro109 => "Shibuya 109",
            Marco::Pagoda => "Neon Pagoda",
        }
    }

    pub fn centro(self) -> Vec2 {
        match self {
            Marco::Skytree => Vec2::new(70.0, -590.0),
            Marco::Prefeitura => Vec2::new(-95.0, -540.0),
            Marco::TorreDeToquio => Vec2::new(-150.0, 580.0),
            Marco::Casulo => Vec2::new(120.0, -320.0),
            Marco::Cilindro109 => cruzamento() + Vec2::new(40.0, -38.0),
            Marco::Pagoda => Vec2::new(90.0, 60.0),
        }
    }

    /// How far its footprint reaches from its centre (a bounding radius).
    pub fn alcance(self) -> f32 {
        match self {
            Marco::Skytree => 14.0,
            Marco::Prefeitura => 18.0,
            Marco::Cilindro109 => 10.0,
            Marco::Pagoda => 14.0,
            _ => 16.0,
        }
    }

    /// Its height (blocks above the street) at offset `p` from its centre,
    /// or `None` outside its footprint.
    fn altura(self, p: Vec2) -> Option<i32> {
        let (ax, az) = (p.x.abs(), p.y.abs());
        let m = ax.max(az);
        let r = p.length();
        let h = match self {
            Marco::Skytree => {
                if r > 13.0 {
                    return None;
                }
                if r <= 1.0 {
                    420
                } else if r <= 3.0 {
                    370
                } else {
                    (350.0 * (1.0 - (r - 3.0) / 10.0).powf(1.4)) as i32 + 10
                }
            }
            Marco::Prefeitura => {
                if !(ax <= 15.0 && az <= 10.0) {
                    return None;
                }
                let torre = |cx: f32| (p.x - cx).abs() <= 4.5 && az <= 5.5;
                let topo = |cx: f32| (p.x - cx).abs() <= 2.5 && az <= 3.5;
                if topo(-8.5) || topo(8.5) {
                    200
                } else if torre(-8.5) || torre(8.5) {
                    170
                } else if ax <= 4.0 && az <= 7.0 {
                    110
                } else {
                    40
                }
            }
            Marco::TorreDeToquio => {
                if m > 11.0 {
                    return None;
                }
                if m <= 0.6 {
                    300
                } else {
                    (260.0 * (1.0 - m / 11.0).powf(1.8)) as i32 + 8
                }
            }
            Marco::Casulo => {
                let e = ((p.x / 9.0).powi(2) + (p.y / 6.5).powi(2)).sqrt();
                if e > 1.0 {
                    return None;
                }
                (180.0 * (1.0 - e.powf(2.2)).sqrt()) as i32 + 8
            }
            Marco::Cilindro109 => {
                if (p - Vec2::new(4.0, -4.0)).length() <= 2.6 {
                    56
                } else if r <= 6.5 {
                    40
                } else {
                    return None;
                }
            }
            Marco::Pagoda => {
                // Five square tiers, each smaller, a spire on top; the eaves
                // glow (`fachada`).
                if m > 13.0 {
                    return None;
                }
                if m <= 1.0 {
                    96
                } else {
                    let tier = ((13.0 - m) / 2.4).floor().clamp(0.0, 4.0) as i32;
                    16 + tier * 14
                }
            }
        };
        Some(h)
    }
}

/// The landmark standing at `q`, with its height, if any.
fn marco_em(q: Vec2) -> Option<(Marco, i32)> {
    Marco::TODOS.iter().find_map(|m| {
        let p = q - m.centro();
        if p.length() > m.alcance() {
            return None;
        }
        m.altura(p).map(|h| (*m, h))
    })
}

// ─────────────────────────────── the city ───────────────────────────────

/// A building's look: its facade palette, or a landmark.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Estilo {
    Concreto,
    Vidro,
    Tijolo,
    Branco,
    /// Neon screens: round the scramble and all over Kabukicho.
    Letreiros,
    /// Shipping containers in the Docks' yards, by colour.
    Conteiner(u8),
    /// A factory chimney (red and white bands) in the Docks.
    Chamine,
    /// A gas holder (a ribbed cylinder) in the Docks.
    Gasometro,
    Marco(Marco),
}

/// What stands on a piece of the city.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Chao {
    /// The sea, past the shore.
    Mar,
    /// The shore band, sloping down to the water: its height in blocks.
    Orla(i32),
    /// A street; `avenida` for the wide ones, `faixa` on a lane marking,
    /// `zebra` on a crosswalk stripe of the scramble.
    Rua { avenida: bool, faixa: bool, zebra: bool },
    /// The sidewalk or a footpath.
    Calcada,
    /// Open paved ground: plazas and courtyards.
    Praca,
    /// A park, a garden, the boulevard's median.
    Parque,
    /// A rail line (kept for the paint; the drawn city has none).
    Trilho,
    /// A building, its roof `altura` blocks above the street.
    Predio { altura: i32, estilo: Estilo },
}

/// A building's SHAPE inside its lot, in the Docks' drawn grid.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Forma {
    Caixa,
    Podio,
    ArmazemComChamine,
    Gasometro,
    Conteineres,
}

fn celula_da_quadra(q: Vec2) -> ((i32, i32), Vec2) {
    let cx = (q.x / QUADRA).floor() as i32;
    let cz = (q.y / QUADRA).floor() as i32;
    ((cx, cz), q - Vec2::new(cx as f32, cz as f32) * QUADRA)
}

fn largura_da_rua(i: i32) -> f32 {
    if i.rem_euclid(AVENIDA_A_CADA) == 0 { AVENIDA } else { RUA }
}

/// The height (and look) of the Docks building at `local` in its lot of size
/// `tam`, or `None` for open ground.
fn predio_das_docas(cx: i32, cz: i32, local: Vec2, tam: Vec2) -> Option<(i32, Estilo)> {
    let t = hash(cx * 3 + 11, cz * 5 - 3);
    let h = 10 + (8.0 * t * t) as i32;
    let r = hash(cx + 17, cz + 29);
    let forma = if r < 0.4 {
        Forma::Conteineres
    } else if r < 0.6 {
        Forma::ArmazemComChamine
    } else if r < 0.72 {
        Forma::Gasometro
    } else if r < 0.88 {
        Forma::Caixa
    } else {
        Forma::Podio
    };
    let estilo = if hash(cx - 5, cz + 13) < 0.5 { Estilo::Tijolo } else { Estilo::Concreto };
    let p = local - tam * 0.5;
    let f = (p.x.abs() / (tam.x * 0.5)).max(p.y.abs() / (tam.y * 0.5));
    let raio = p.length() / (tam.x.min(tam.y) * 0.5);
    let canto = tam * Vec2::new(0.32, -0.32);
    Some(match forma {
        Forma::Caixa => (h, estilo),
        Forma::Podio => (if f <= 0.6 { h + 8 } else { (h / 2).max(8) }, estilo),
        Forma::ArmazemComChamine => {
            if (p - canto).length() < 1.6 {
                (60 + (hash(cx, cz) * 24.0) as i32, Estilo::Chamine)
            } else {
                (h.min(16), estilo)
            }
        }
        Forma::Gasometro => {
            if raio > 0.9 {
                return None;
            }
            (20 + (hash(cx, cz + 3) * 10.0) as i32, Estilo::Gasometro)
        }
        Forma::Conteineres => {
            let (i, j) = ((local.x / 7.0).floor() as i32, (local.y / 3.5).floor() as i32);
            let (u, v) = (local.x.rem_euclid(7.0), local.y.rem_euclid(3.5));
            if u > 6.0 || v > 2.5 {
                return None;
            }
            let pilha = 1 + (hash(cx * 31 + i, cz * 17 + j) * 3.0) as i32;
            let cor = (hash(cx * 13 + i, cz * 7 + j) * 5.0) as u8;
            (pilha * 5, Estilo::Conteiner(cor))
        }
    })
}

/// The Docks: a drawn grid of warehouses, container yards and gas holders.
fn docas_em(q: Vec2) -> Chao {
    let ((cx, cz), local) = celula_da_quadra(q);
    let (lx, lz) = (largura_da_rua(cx), largura_da_rua(cz));
    if local.x < lx || local.y < lz {
        let avenida = (local.x < lx && lx > RUA) || (local.y < lz && lz > RUA);
        let faixa = avenida
            && ((local.x < lx && (local.x - lx * 0.5).abs() < 0.4 && q.y.rem_euclid(6.0) < 3.0)
                || (local.y < lz && (local.y - lz * 0.5).abs() < 0.4 && q.x.rem_euclid(6.0) < 3.0));
        return Chao::Rua { avenida, faixa, zebra: false };
    }
    let dentro = Vec2::new(local.x - lx, local.y - lz);
    let tamanho = Vec2::new(QUADRA - lx, QUADRA - lz);
    if dentro.x < CALCADA || dentro.y < CALCADA || dentro.x > tamanho.x - CALCADA || dentro.y > tamanho.y - CALCADA {
        return Chao::Calcada;
    }
    if hash(cx, cz) < 0.3 {
        return Chao::Praca;
    }
    match predio_das_docas(cx, cz, dentro - Vec2::splat(CALCADA), tamanho - Vec2::splat(2.0 * CALCADA)) {
        Some((altura, estilo)) => Chao::Predio { altura, estilo },
        None => Chao::Praca,
    }
}

/// The lanes' grid, bent a little: the point of the straight grid `q`
/// falls on.
fn torcer(q: Vec2) -> Vec2 {
    let a = 6.0;
    q + a * Vec2::new((q.y * 0.012 + 1.3).sin(), (q.x * 0.011 - 0.7).sin())
}

/// A block's building at `local` (from the block's inner corner) in a block
/// `tam` across: the low ring of shops by the street, gaps to walk in, the
/// courtyard behind, a setback tower only where the district allows it.
fn predio_do_quarteirao(d: usize, cx: i32, cz: i32, local: Vec2, tam: Vec2, q: Vec2) -> Chao {
    let borda = local.x.min(local.y).min(tam.x - local.x).min(tam.y - local.y);
    let centro = local - tam * 0.5;
    // The skyline: by the coast in the north half, towers fill the block.
    let costa = ate_a_costa(q);
    if d >= 3 && costa < 55.0 {
        let h = 80 + (hash(cx * 7 + 3, cz * 5 + 1) * 120.0) as i32;
        let estilo = if hash(cx + 9, cz - 4) < 0.35 { Estilo::Letreiros } else { Estilo::Vidro };
        return Chao::Predio { altura: h, estilo };
    }
    if borda < ANEL {
        // A gap every few shops: the courtyard is reached from the street.
        let ao_longo = if local.x.min(tam.x - local.x) < local.y.min(tam.y - local.y) { local.y } else { local.x };
        let segmento = (ao_longo / 13.0).floor() as i32;
        if ao_longo.rem_euclid(13.0) < 2.0 && hash(cx * 3 + segmento, cz * 5) < 0.5 {
            return Chao::Calcada;
        }
        let r = hash(cx * 11 + segmento, cz * 13 - segmento);
        let (lo, hi, neon) = match d {
            1 => (8, 14, 0.4),
            3 => (10, 18, 0.7),
            4 => (12, 20, 0.25),
            _ => (12, 22, 0.15),
        };
        let mut altura = lo + ((hi - lo) as f32 * r) as i32;
        let perto_da_cruz = q.distance(cruzamento()) < 75.0;
        if perto_da_cruz {
            altura += 14;
        }
        let estilo = if perto_da_cruz || r < neon {
            Estilo::Letreiros
        } else {
            match d {
                1 => if r < 0.7 { Estilo::Tijolo } else { Estilo::Branco },
                4 | 5 => if r < 0.7 { Estilo::Vidro } else { Estilo::Branco },
                _ => Estilo::Concreto,
            }
        };
        return Chao::Predio { altura, estilo };
    }
    // The courtyard; in the towers, a setback tower in some blocks, away
    // from the boulevard (tall things stay off the main way).
    if d == 4
        && centro.length() < 9.0
        && (q.x - boulevard_x(q.y)).abs() > 70.0
        && ao_anel(q).abs() > 24.0
        && hash(cx, cz + 7) < 0.55
    {
        let h = 70 + (hash(cx + 2, cz) * 90.0) as i32;
        return Chao::Predio { altura: h, estilo: Estilo::Vidro };
    }
    if hash(cx + 5, cz + 3) < 0.35 { Chao::Parque } else { Chao::Praca }
}

/// What is at `q`.
pub fn chao_em(q: Vec2) -> Chao {
    let s = fora(q);
    if s >= 1.0 {
        return Chao::Mar;
    }
    let costa = ate_a_costa(q);
    if costa < ORLA {
        let t = (costa / ORLA).clamp(0.0, 1.0);
        return Chao::Orla((1.0 + (NIVEL_CHAO - 1) as f32 * t).round() as i32);
    }
    if let Some((m, h)) = marco_em(q) {
        return Chao::Predio { altura: h, estilo: Estilo::Marco(m) };
    }
    // Open ground: the town's circle, the arenas, the landmarks' plazas.
    if q.distance(centro_da_cidade()) < crate::terreno::Cidade::RAIO + 4.0 {
        return Chao::Praca;
    }
    if arenas().iter().any(|a| a.distance(q) < RAIO_ARENA) {
        return Chao::Praca;
    }
    if Marco::TODOS.iter().any(|m| (q - m.centro()).length() < m.alcance() + 5.0) {
        return Chao::Praca;
    }
    // The scramble: the four crosswalks round the crossing and the two
    // diagonals across it, striped; asphalt between them.
    let d_cruz = q.distance(cruzamento());
    if d_cruz < RAIO_DO_CRUZAMENTO {
        let p = q - cruzamento();
        let (ax, az) = (p.x.abs(), p.y.abs());
        let borda = (ax - 17.0).abs() < 3.0 && az < 17.0 || (az - 17.0).abs() < 3.0 && ax < 17.0;
        let diagonal = (ax - az).abs() < 2.4 && ax < 17.0;
        let listra = if borda && (ax - 17.0).abs() < 3.0 { p.y.rem_euclid(1.6) < 0.8 } else { (p.x + p.y * 0.0).rem_euclid(1.6) < 0.8 };
        let zebra = (borda && listra) || (diagonal && (p.x + p.y).rem_euclid(1.6) < 0.8);
        return Chao::Rua { avenida: true, faixa: false, zebra };
    }
    // Under the ring road: street.
    if sob_o_deck(q) {
        return Chao::Rua { avenida: true, faixa: false, zebra: false };
    }
    if q.y >= DOCAS_DE {
        return docas_em(q);
    }
    if q.y >= AVENIDA_DO_CAIS {
        let meio = (AVENIDA_DO_CAIS + DOCAS_DE) * 0.5;
        let faixa = (q.y - meio).abs() < 0.4 && q.x.rem_euclid(6.0) < 3.0;
        return Chao::Rua { avenida: true, faixa, zebra: false };
    }
    // The grand boulevard and its sakura median.
    let db = (q.x - boulevard_x(q.y)).abs();
    if db <= CANTEIRO_MEIO {
        return Chao::Parque;
    }
    if db <= BOULEVARD_MEIA {
        let faixa = (db - 7.2).abs() < 0.4 && q.y.rem_euclid(6.0) < 3.0;
        return Chao::Rua { avenida: true, faixa, zebra: false };
    }
    // The cross boulevards.
    for k in 0..CRUZES.len() {
        let dc = (q.y - cruz_z(k, q.x)).abs();
        if dc <= CRUZ_MEIA {
            let faixa = dc < 0.4 && q.x.rem_euclid(6.0) < 3.0;
            return Chao::Rua { avenida: true, faixa, zebra: false };
        }
        if dc <= CRUZ_MEIA + CALCADA {
            return Chao::Calcada;
        }
    }
    if db <= BOULEVARD_MEIA + CALCADA {
        return Chao::Calcada;
    }
    let d = distrito_em(q);
    let g = torcer(q);
    let (cx, cz) = ((g.x / LOTE).floor() as i32, (g.y / LOTE).floor() as i32);
    let local = g - Vec2::new(cx as f32, cz as f32) * LOTE;
    // THE SHRINE FOREST: a park; the lanes are stone paths.
    if d == 2 {
        if local.x < LANE * 0.5 || local.y < LANE * 0.5 {
            return Chao::Calcada;
        }
        return Chao::Parque;
    }
    // The lanes, and their sidewalks.
    if local.x < LANE || local.y < LANE {
        return Chao::Rua { avenida: false, faixa: false, zebra: false };
    }
    let tam = Vec2::splat(LOTE - LANE - 2.0 * CALCADA);
    let dentro = local - Vec2::splat(LANE + CALCADA);
    if dentro.x < 0.0 || dentro.y < 0.0 || dentro.x > tam.x || dentro.y > tam.y {
        return Chao::Calcada;
    }
    // Kabukicho's alleys: one more down the middle of each block.
    if d == 3 && (dentro.x - tam.x * 0.5).abs() < 2.0 {
        return Chao::Rua { avenida: false, faixa: false, zebra: false };
    }
    // Some blocks are left open: a plaza or a garden.
    let aberto = match d {
        1 => 0.15,
        3 => 0.1,
        4 => 0.15,
        _ => 0.25,
    };
    if hash(cx * 19 + 1, cz * 23 - 5) < aberto {
        return if hash(cx, cz) < 0.5 { Chao::Praca } else { Chao::Parque };
    }
    predio_do_quarteirao(d, cx, cz, dentro, tam, q)
}

/// The top block of column `(bx, bz)`. The ONLY source of Kōgen-tō's relief.
pub fn bloco_da_coluna(bx: i32, bz: i32) -> i32 {
    use crate::terreno::BLOCO;
    let q = Vec2::new(bx as f32, bz as f32) * BLOCO;
    match chao_em(q) {
        Chao::Mar => NIVEL_FUNDO,
        Chao::Orla(h) => h.clamp(1, NIVEL_CHAO),
        Chao::Predio { altura, .. } => NIVEL_CHAO + altura,
        _ => NIVEL_CHAO,
    }
}

/// The traffic's paths (`client::transito`): (on the deck, lane offset,
/// centre line). The boulevard, the cross boulevards, the waterfront avenue,
/// the ring road and its ramps.
pub fn caminhos_de_transito() -> Vec<(bool, f32, Vec<Vec2>)> {
    let em_terra = |p: Vec2| fora(p) < 0.97;
    let mut v = Vec::new();
    let corta = |elevado: bool, faixa: f32, pts: Vec<Vec2>, v: &mut Vec<(bool, f32, Vec<Vec2>)>| {
        let mut atual = Vec::new();
        for p in pts {
            if em_terra(p) {
                atual.push(p);
            } else if atual.len() > 1 {
                v.push((elevado, faixa, std::mem::take(&mut atual)));
            } else {
                atual.clear();
            }
        }
        if atual.len() > 1 {
            v.push((elevado, faixa, atual));
        }
    };
    // The boulevard, south to north.
    let mut z = AVENIDA_DO_CAIS - 2.0;
    let mut pts = Vec::new();
    while z > -COSTA_B {
        pts.push(Vec2::new(boulevard_x(z), z));
        z -= 4.0;
    }
    corta(false, 7.0, pts, &mut v);
    for k in 0..CRUZES.len() {
        let pts: Vec<Vec2> = (-80..=80).map(|i| i as f32 * 4.0).map(|x| Vec2::new(x, cruz_z(k, x))).collect();
        corta(false, 4.0, pts, &mut v);
    }
    let cais = (AVENIDA_DO_CAIS + DOCAS_DE) * 0.5;
    corta(false, 3.5, (-80..=80).map(|i| Vec2::new(i as f32 * 4.0, cais)).collect(), &mut v);
    // The ring, closed, and the ramps.
    let anel: Vec<Vec2> = (0..=180)
        .map(|i| {
            let t = i as f32 / 180.0 * std::f32::consts::TAU;
            ANEL_CENTRO + Vec2::new(ANEL_A * t.cos(), ANEL_B * t.sin())
        })
        .collect();
    corta(true, 2.5, anel, &mut v);
    for (p, n) in rampas() {
        let pts: Vec<Vec2> = (0..=14).map(|i| p + n * (i as f32 * RAMPA_COMPRIMENTO / 14.0)).collect();
        corta(true, 2.0, pts, &mut v);
    }
    v
}

// ─────────────────────────────── the paint ───────────────────────────────

fn cor_do_conteiner(c: u8) -> crate::terreno::Material {
    use crate::terreno::Material as M;
    match c % 5 {
        0 => M::PetalaVermelha,
        1 => M::Vidro,
        2 => M::FaixaDePista,
        3 => M::GramaEscura,
        _ => M::Concreto,
    }
}

/// The paint of the ground at `q` (`terreno::pintura_do_chao`). `None` =
/// the biome's own (the parks' grass).
pub fn pintura(q: Vec2) -> Option<crate::terreno::Material> {
    use crate::terreno::Material as M;
    let quadriculado = |passo: f32| ((q.x / passo).floor() as i32 + (q.y / passo).floor() as i32).rem_euclid(2) == 0;
    Some(match chao_em(q) {
        Chao::Mar => return None,
        Chao::Orla(_) => M::Concreto,
        Chao::Rua { zebra: true, .. } => M::Nuvem,
        Chao::Rua { faixa: true, .. } => M::FaixaDePista,
        Chao::Rua { .. } => M::Asfalto,
        Chao::Calcada => if quadriculado(1.5) { M::Concreto } else { M::ConcretoEscuro },
        // Plazas: dark paving with glowing lines every few units.
        Chao::Praca => {
            if q.x.rem_euclid(9.0) < 0.5 || q.y.rem_euclid(9.0) < 0.5 {
                M::NeonCiano
            } else if quadriculado(2.0) {
                M::Concreto
            } else {
                M::ConcretoEscuro
            }
        }
        Chao::Trilho => M::ConcretoEscuro,
        Chao::Parque => return None,
        Chao::Predio { estilo, altura } => match estilo {
            Estilo::Conteiner(c) => cor_do_conteiner(c),
            Estilo::Marco(Marco::TorreDeToquio) | Estilo::Chamine => M::PetalaVermelha,
            Estilo::Marco(Marco::Skytree | Marco::Casulo) => M::Nuvem,
            Estilo::Marco(Marco::Pagoda) => M::NeonRosa,
            _ => {
                let borda = [Vec2::X, -Vec2::X, Vec2::Y, -Vec2::Y]
                    .iter()
                    .any(|d| !matches!(chao_em(q + *d * 1.0), Chao::Predio { .. }));
                if !borda {
                    return Some(M::ConcretoEscuro);
                }
                // A neon rim round every roof: low roofs are what the camera
                // sees, and the rim is what makes them read at night.
                match (estilo, distrito_em(q)) {
                    (Estilo::Letreiros, _) => if quadriculado(3.0) { M::NeonRosa } else { M::NeonAmarelo },
                    (_, 3) => if quadriculado(3.0) { M::NeonRosa } else { M::NeonCiano },
                    (_, 5) => M::NeonCiano,
                    (_, 4) => M::NeonAmarelo,
                    _ if altura > 30 => M::NeonCiano,
                    _ => if quadriculado(4.0) { M::NeonAmarelo } else { M::ConcretoEscuro },
                }
            }
        },
    })
}

/// The facade of a building at depth `prof` blocks below the roof of a
/// column whose roof stands `topo` blocks above the street. `(gx, gz)` is the
/// column (for patterns).
pub fn fachada(q: Vec2, gx: i32, gz: i32, prof: i32, topo: i32) -> crate::terreno::Material {
    use crate::terreno::Material as M;
    let estilo = match chao_em(q) {
        Chao::Predio { estilo, .. } => estilo,
        _ => Estilo::Concreto,
    };
    let alto = topo - prof; // this voxel's height above the street, in blocks
    // A storey every four blocks: two of window, two of wall.
    let janela = prof % 4 == 1 || prof % 4 == 2;
    let acesa = |chance: f32| hash(gx / 2, gz / 2 + prof / 4 * 7) < chance;
    let lit = |chance: f32| if acesa(chance) { M::JanelaAcesa } else { M::Vidro };
    match estilo {
        Estilo::Concreto => if janela { lit(0.45) } else { M::Concreto },
        Estilo::Vidro => if janela || prof % 8 != 0 { lit(0.3) } else { M::ConcretoEscuro },
        Estilo::Tijolo => if janela && (gx + gz).rem_euclid(3) != 0 { lit(0.55) } else { M::Arenito },
        Estilo::Branco => if janela { lit(0.4) } else { M::Calcada },
        Estilo::Letreiros => {
            let telas_ate = (topo * 2 / 3).clamp(6, 60);
            if alto > telas_ate {
                return if janela { lit(0.3) } else { M::Vidro };
            }
            let (col, lin) = ((gx + gz).rem_euclid(10), alto.rem_euclid(6));
            if col == 0 || lin == 0 {
                return M::ConcretoEscuro;
            }
            match (hash((gx + gz).div_euclid(10), alto.div_euclid(6) + 3) * 6.0) as i32 {
                0 => M::NeonRosa,
                1 => M::NeonCiano,
                2 => M::NeonAmarelo,
                3 => M::JanelaAcesa,
                4 => M::PetalaVermelha,
                _ => M::Nuvem,
            }
        }
        Estilo::Conteiner(c) => if prof % 5 == 0 { M::ConcretoEscuro } else { cor_do_conteiner(c) },
        Estilo::Chamine => if (alto / 8) % 2 == 0 { M::PetalaVermelha } else { M::Nuvem },
        Estilo::Gasometro => if (gx + gz).rem_euclid(4) == 0 { M::ConcretoEscuro } else { M::Concreto },
        Estilo::Marco(m) => match m {
            Marco::Skytree => if (gx + gz + prof).rem_euclid(3) == 0 { M::NeonCiano } else { M::Nuvem },
            Marco::Prefeitura => if janela && (gx + gz).rem_euclid(2) == 0 { lit(0.6) } else { M::ConcretoEscuro },
            Marco::TorreDeToquio => if (alto / 16) % 2 == 0 { M::PetalaVermelha } else { M::Nuvem },
            Marco::Casulo => {
                if (gx + alto).rem_euclid(5) == 0 || (gz - alto).rem_euclid(5) == 0 { M::Nuvem } else { lit(0.3) }
            }
            Marco::Cilindro109 => if prof < 4 { M::NeonRosa } else if janela { M::JanelaAcesa } else { M::Concreto },
            // The pagoda: red walls, glowing eaves at every tier's top.
            Marco::Pagoda => {
                if alto.rem_euclid(14) >= 12 {
                    M::NeonRosa
                } else if alto.rem_euclid(14) >= 10 {
                    M::ConcretoEscuro
                } else {
                    M::PetalaVermelha
                }
            }
        },
    }
}

/// Trees, plants and gathering nodes grow ONLY in parks (not on the
/// boulevard's median, where the sakura props stand): on a street a rock
/// closes the way, and nothing grows on a roof.
pub fn so_no_parque(q: Vec2) -> bool {
    chao_em(q) == Chao::Parque && (q.x - boulevard_x(q.y)).abs() > CANTEIRO_MEIO + 0.5
}

/// The mob level band at `q` (the district's), `None` at sea.
pub fn faixa_em(q: Vec2) -> Option<(u32, u32)> {
    (chao_em(q) != Chao::Mar).then(|| DISTRITOS[distrito_em(q)].nivel)
}

pub fn e_kogen(zona: &str) -> bool {
    zona == ZONA
}

/// The zone as the rest of the game reads it.
pub const DEF: crate::terreno::DefIlha = crate::terreno::DefIlha {
    zona: ZONA,
    nome: "Kōgen-tō",
    semente: SEMENTE,
    raio_blocos: RAIO_BLOCOS,
    bioma: crate::terreno::Bioma::Neon,
    centro: [-3400.0, -3600.0],
    nivel: (80, 100),
};

#[cfg(test)]
mod testes {
    use super::*;

    /// Six districts, each with streets, open ground and buildings (the
    /// forest: paths and park), and the hub town on dry ground in the Docks.
    #[test]
    fn seis_distritos_ruas_e_lotes_abertos() {
        let mut ruas = [0u32; 6];
        let mut abertos = [0u32; 6];
        let mut predios = [0u32; 6];
        let mut x = -300.0;
        while x < 300.0 {
            let mut z = -640.0;
            while z < 640.0 {
                let q = Vec2::new(x, z);
                let d = distrito_em(q);
                match chao_em(q) {
                    Chao::Rua { .. } => ruas[d] += 1,
                    Chao::Praca | Chao::Parque | Chao::Calcada => abertos[d] += 1,
                    Chao::Predio { .. } => predios[d] += 1,
                    _ => {}
                }
                z += 3.0;
            }
            x += 3.0;
        }
        for d in 0..6 {
            assert!(ruas[d] > 100, "{}: {} street samples", DISTRITOS[d].nome, ruas[d]);
            assert!(abertos[d] > 100, "{}: {} open samples", DISTRITOS[d].nome, abertos[d]);
            if d != 2 {
                assert!(predios[d] > 100, "{}: {} building samples", DISTRITOS[d].nome, predios[d]);
            }
        }
        assert_eq!(distrito_em(centro_da_cidade()), 0, "the hub is in the Docks");
        assert!(!matches!(chao_em(centro_da_cidade()), Chao::Mar | Chao::Orla(_)));
    }

    /// LOW BY THE STREETS: wherever a building touches a street, it is at
    /// most a few characters tall — except the coastal skyline and the
    /// landmarks. The owner: "I barely can see my character among the
    /// buildings".
    #[test]
    fn baixo_perto_da_rua() {
        let mut x = -290.0;
        while x < 290.0 {
            let mut z = -620.0;
            while z < 500.0 {
                let q = Vec2::new(x, z);
                if let Chao::Predio { altura, estilo } = chao_em(q) {
                    let na_rua = [Vec2::X, -Vec2::X, Vec2::Y, -Vec2::Y]
                        .iter()
                        .any(|d| matches!(chao_em(q + *d * 1.5), Chao::Calcada | Chao::Rua { .. }));
                    let excecao = matches!(estilo, Estilo::Marco(_)) || ate_a_costa(q) < 60.0;
                    if na_rua && !excecao {
                        assert!(altura <= 36, "a {altura}-block building on a street at {q}");
                    }
                }
                z += 2.0;
            }
            x += 2.0;
        }
    }

    /// Every landmark stands where it says, on land, in one piece, and the
    /// Spire is the tallest thing on the island.
    #[test]
    fn os_marcos_estao_de_pe() {
        let spire = match chao_em(Marco::Skytree.centro()) {
            Chao::Predio { altura, .. } => altura,
            o => panic!("no Spire: {o:?}"),
        };
        for m in Marco::TODOS {
            match chao_em(m.centro()) {
                Chao::Predio { estilo: Estilo::Marco(k), altura } => {
                    assert_eq!(k, m, "{} is covered by {:?}", m.nome(), k);
                    assert!(altura <= spire, "{} is taller than the Spire", m.nome());
                }
                o => panic!("{} missing: {o:?}", m.nome()),
            }
        }
    }

    /// Going north, the districts come in level order, and the places land
    /// in theirs.
    #[test]
    fn distritos_do_sul_ao_norte() {
        assert_eq!(distrito_em(centro_da_cidade()), 0);
        assert_eq!(distrito_em(cruzamento()), 1, "the scramble is in Shibuya");
        assert_eq!(distrito_em(Marco::Pagoda.centro()), 2, "the pagoda is in the forest");
        assert_eq!(distrito_em(arenas()[1]), 3, "the Kaiju's plaza is in Kabukicho");
        assert_eq!(distrito_em(Marco::Casulo.centro()), 4, "the Cocoon is in the towers");
        assert_eq!(distrito_em(Marco::Prefeitura.centro()), 5);
        assert!(matches!(chao_em(cruzamento()), Chao::Rua { .. }), "the crossing is a street");
    }

    /// The ring road is a deck over the streets, with ramps down to them.
    #[test]
    fn a_expressa_tem_deck_e_rampas() {
        let mut n = 0;
        let mut x = -300.0;
        while x < 300.0 {
            let mut z = -400.0;
            while z < 300.0 {
                n += deck_em(Vec2::new(x, z)).is_some() as u32;
                z += 2.0;
            }
            x += 2.0;
        }
        assert!(n > 500, "only {n} deck samples");
        for (p, d) in rampas() {
            let pe = p + d * (RAMPA_COMPRIMENTO - 1.0);
            assert_eq!(deck_em(pe), Some(1), "a ramp's foot at {pe} is not one block up");
            assert!(matches!(chao_em(pe), Chao::Rua { .. }), "a ramp lands off the street at {pe}");
        }
    }

    /// Each giant robot's arena is open ground in the district of its level.
    #[test]
    fn as_arenas_estao_nos_distritos_dos_chefes() {
        let chefes = crate::bosses::da_zona(ZONA);
        assert_eq!(chefes.len(), 3);
        for (a, c) in arenas().iter().zip(chefes) {
            let d = distrito_em(*a);
            let (lo, hi) = DISTRITOS[d].nivel;
            assert!(c.nivel >= lo && c.nivel <= hi, "{} (level {}) stands in {}", c.nome, c.nivel, DISTRITOS[d].nome);
            assert_eq!(chao_em(*a), Chao::Praca, "{}'s arena is not open", c.nome);
        }
    }

    /// The expressway is walkable on top AND underneath: up a ramp from the
    /// street onto the deck, along it between the rails, under it on the
    /// street, and the A* finds the way up.
    #[test]
    fn a_expressa_se_anda_em_cima_e_embaixo() {
        use crate::constants::{ENTITY_RADIUS as R, PLAYER_SPEED as V};
        use crate::terreno::{Ilha, CAMADA_CHAO, CAMADA_DECK, DEGRAU_BLOCOS};
        let ilha = Ilha::da_ilha(&DEF);
        let dt = 1.0 / 30.0;
        let andar = |de: Vec2, cam: u8, alvo: Vec2, s: f32| -> (Vec2, u8) {
            let (mut p, mut c) = (de, cam);
            let mut pulo = 0.0f32;
            for _ in 0..(s / dt) as u32 {
                let dir = (alvo - p).normalize_or_zero();
                if p.distance(alvo) < 0.3 {
                    break;
                }
                if pulo <= 0.0 && ilha.precisa_pular_na(p, dir * V, dt, R, c) {
                    pulo = crate::constants::PULO_DURACAO;
                }
                let degrau = if pulo > 0.0 { crate::terreno::PULO_BLOCOS } else { DEGRAU_BLOCOS };
                pulo -= dt;
                (p, c) = ilha.mover_na_camada(p, dir * V, dt, R, degrau, c);
            }
            (p, c)
        };
        // Up a ramp, foot to top, from the street.
        let (p0, n0) = rampas()[0];
        let pe = p0 + n0 * (RAMPA_COMPRIMENTO + 4.0);
        let topo = p0 + n0 * 6.0;
        let (p, c) = andar(pe, CAMADA_CHAO, topo, 20.0);
        assert_eq!(c, CAMADA_DECK, "walked up the ramp but is not on the deck (at {p})");
        assert!(p.distance(topo) < 1.5, "stopped on the ramp at {p}");
        // The rails: from the ramp's top, walking off sideways stops at the edge.
        let (p, c) = andar(topo, CAMADA_DECK, topo + n0.perp() * 30.0, 8.0);
        assert_eq!(c, CAMADA_DECK, "fell off the deck sideways (at {p})");
        assert!(deck_em(p).is_some(), "walked off the deck's edge to {p}");
        // Under it: the street.
        assert_eq!(ilha.altura_na(topo.x, topo.y, CAMADA_CHAO), ilha.altura(topo.x, topo.y));
        // A* from the street onto the deck, followed by the body.
        let alvo = ANEL_CENTRO + Vec2::new(0.0, -ANEL_B);
        let rota = ilha
            .caminho_na_camada(pe, CAMADA_CHAO, alvo, CAMADA_DECK, 40_000, &[])
            .expect("no route from the street onto the deck");
        let (mut p, mut c) = (pe, CAMADA_CHAO);
        for a in &rota {
            (p, c) = andar(p, c, *a, 30.0);
        }
        assert_eq!(c, CAMADA_DECK, "followed the route but ended on the street at {p}");
        assert!(p.distance(alvo) < 2.5, "followed the route but stopped at {p}, short of {alvo}");
    }
}

#[cfg(test)]
mod testes_de_rota {
    use super::*;

    /// FROM THE BUS STOP TO EVERY DISTRICT ON FOOT: the server's own A*
    /// reaches a street in each of the six.
    #[test]
    fn da_cidade_se_anda_a_todo_distrito() {
        let ilha = crate::terreno::Ilha::da_ilha(&DEF);
        let c = ilha.cidade().unwrap().centro();
        let alvos = [
            ("Shibuya", cruzamento() + Vec2::new(0.0, -40.0)),
            ("the forest", Marco::Pagoda.centro() + Vec2::new(-30.0, 0.0)),
            ("Kabukicho", arenas()[1]),
            ("the towers", Marco::Casulo.centro() + Vec2::new(0.0, 25.0)),
            ("the Tocho", arenas()[2]),
        ];
        for (nome, alvo) in alvos {
            let rua = rua_perto(alvo);
            let fim = ilha.caminho(c, rua, 60_000).and_then(|r| r.last().copied());
            assert!(fim.is_some_and(|f| f.distance(rua) < 1.5), "{nome}: the route from town stops at {fim:?}, short of {rua}");
        }
    }
}
