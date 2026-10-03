//! KŌGEN-TŌ (`ilha_kogen`): the level 80-100 island, a futuristic city at
//! night, reached only from Skyreach by a flying bus.
//!
//! The owner, 02/10/2026: "a futuristic island ... like Tokyo but in island
//! format ... robots, giant robot bosses ... much light, roads, cars, high
//! buildings". Always night, six districts. On the previews the drawn grid
//! was "too much square ... not very organic", and on 03/10/2026 he asked for
//! a real city instead: Kōgen-tō is **real Shinjuku + Shibuya**, baked from
//! OpenStreetMap (© OpenStreetMap contributors, ODbL) by
//! `tools/kogen-osm/rasterizar.py` into `assets/kogen_mapa.bin` — every
//! street, rail line, park and building footprint where it really is, 4 m of
//! Tokyo to a game unit, half a block of height per metre.
//!
//! The districts run SOUTH to NORTH as the levels rise: the Docks (a drawn
//! waterfront where the bus lands), Shibuya, the Meiji Shrine forest,
//! Kabukicho, the Nishi-Shinjuku towers and the Tocho.
//!
//! The terrain is a heightmap — one top block per column — so a building is a
//! SOLID block of columns that you walk round. The Shuto expressways' decks
//! are baked as their own layer (walking on them is a later step).
//!
//! `chao_em` / `bloco_da_coluna` are the layout's only source, called by
//! client and server alike.

use glam::Vec2;
use std::sync::OnceLock;

pub const ZONA: &str = "ilha_kogen";
/// Bump on any change to the layout: it is in the server's height cache key.
pub const REVISAO: u32 = 4;
/// Planting seed: the relief does not depend on it, the decoration does.
pub const SEMENTE: i32 = 0x0C06_E170;
/// Zone radius in BLOCKS: the island plus a margin of sea.
pub const RAIO_BLOCOS: i32 = 1400;
/// The city's ground, in blocks (sea level is 0).
pub const NIVEL_CHAO: i32 = 20;
/// The sea floor past the shore, in blocks.
pub const NIVEL_FUNDO: i32 = -6;

// ── the projection (mirror of tools/kogen-osm/rasterizar.py) ──
/// Metres of Tokyo per game unit.
pub const METROS_POR_UNIDADE: f32 = 4.0;
/// The point of Tokyo at the island's origin.
pub const LAT0: f64 = 35.6754;
pub const LON0: f64 = 139.7000;
const M_POR_GRAU_LAT: f64 = 110_540.0;

/// Where a point of Tokyo falls on the island, in units (+z is south).
pub fn de_latlon(lat: f64, lon: f64) -> Vec2 {
    let m_por_grau_lon = 111_320.0 * LAT0.to_radians().cos();
    let x = (lon - LON0) * m_por_grau_lon;
    let z = -(lat - LAT0) * M_POR_GRAU_LAT;
    Vec2::new(x as f32, z as f32) / METROS_POR_UNIDADE
}

/// The Docks start this far south, in units (row 1030 of the bake).
pub const DOCAS_DE: f32 = 515.0;

// ── the Docks' drawn grid (the only part not from the map) ──
/// One grid cell, in units: the street plus the lot.
pub const QUADRA: f32 = 40.0;
pub const RUA: f32 = 8.0;
pub const AVENIDA: f32 = 14.0;
pub const AVENIDA_A_CADA: i32 = 3;
pub const CALCADA: f32 = 2.5;

/// A district: its name, its level band.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Distrito {
    pub nome: &'static str,
    pub nivel: (u32, u32),
}

/// The six districts, south to north. Index 0 is the arrival (the hub town).
/// ORDER IS THE ID (it is baked into the map).
pub const DISTRITOS: [Distrito; 6] = [
    Distrito { nome: "Harbor Docks", nivel: (80, 83) },
    Distrito { nome: "Shibuya Crossing", nivel: (83, 86) },
    Distrito { nome: "Shrine Forest", nivel: (86, 90) },
    Distrito { nome: "Kabukicho Neon", nivel: (90, 94) },
    Distrito { nome: "Nishi-Shinjuku Towers", nivel: (94, 97) },
    Distrito { nome: "Tocho Spire", nivel: (97, 100) },
];

/// Where the hub town sits: in the Docks, between the waterfront avenue and
/// the south shore.
pub fn centro_da_cidade() -> Vec2 {
    Vec2::new(0.0, 572.0)
}

/// The Shibuya scramble crossing (where it really is).
pub fn cruzamento() -> Vec2 {
    de_latlon(35.65950, 139.70050)
}

// ─────────────────────────────── the baked map ───────────────────────────────

/// A cell's kind, as baked (mirror of the rasterizer's).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
enum Tipo {
    Mar = 0,
    Orla,
    Rua,
    Avenida,
    Faixa,
    Zebra,
    Calcada,
    Praca,
    Parque,
    Bosque,
    Trilho,
    Predio,
    Docas,
}

impl Tipo {
    fn de(b: u8) -> Tipo {
        match b {
            1 => Tipo::Orla,
            2 => Tipo::Rua,
            3 => Tipo::Avenida,
            4 => Tipo::Faixa,
            5 => Tipo::Zebra,
            6 => Tipo::Calcada,
            7 => Tipo::Praca,
            8 => Tipo::Parque,
            9 => Tipo::Bosque,
            10 => Tipo::Trilho,
            11 => Tipo::Predio,
            12 => Tipo::Docas,
            _ => Tipo::Mar,
        }
    }
}

/// One baked cell.
#[derive(Debug, Clone, Copy)]
struct Celula {
    tipo: Tipo,
    distrito: u8,
    /// A building's roof above the street, or the shore's height, in blocks.
    altura: u8,
    estilo: u8,
    /// The expressway deck above the street here, in blocks (0 = none).
    deck: u8,
}

struct Mapa {
    x0: i32,
    z0: i32,
    w: i32,
    h: i32,
    planos: Vec<u8>,
}

static MAPA_BRUTO: &[u8] = include_bytes!("../../../assets/kogen_mapa.bin");

fn mapa() -> &'static Mapa {
    static M: OnceLock<Mapa> = OnceLock::new();
    M.get_or_init(|| {
        let b = MAPA_BRUTO;
        assert_eq!(&b[0..4], b"KOG2", "kogen_mapa.bin: wrong format");
        let i32_em = |i: usize| i32::from_le_bytes([b[i], b[i + 1], b[i + 2], b[i + 3]]);
        let (x0, z0, w, h) = (i32_em(4), i32_em(8), i32_em(12), i32_em(16));
        let planos = miniz_oxide::inflate::decompress_to_vec_zlib(&b[20..]).expect("kogen_mapa.bin: corrupt");
        assert_eq!(planos.len(), (w * h * 4) as usize, "kogen_mapa.bin: wrong size");
        Mapa { x0, z0, w, h, planos }
    })
}

/// The baked cell under `q`; `None` past the baked window (open sea).
fn celula_em(q: Vec2) -> Option<Celula> {
    use crate::terreno::BLOCO;
    let m = mapa();
    let c = (q.x / BLOCO).floor() as i32 - m.x0;
    let r = (q.y / BLOCO).floor() as i32 - m.z0;
    if c < 0 || r < 0 || c >= m.w || r >= m.h {
        return None;
    }
    let n = (m.w * m.h) as usize;
    let i = (r * m.w + c) as usize;
    let t = m.planos[i];
    Some(Celula {
        tipo: Tipo::de(t & 0x0F),
        distrito: (t >> 4).min(5),
        altura: m.planos[n + i],
        estilo: m.planos[2 * n + i],
        deck: m.planos[3 * n + i],
    })
}

/// Which district `q` is in (0 at sea).
pub fn distrito_em(q: Vec2) -> usize {
    celula_em(q).map_or(0, |c| c.distrito as usize)
}

/// The Shuto expressway deck over `q`, in blocks above the street.
pub fn deck_em(q: Vec2) -> Option<i32> {
    celula_em(q).and_then(|c| (c.deck > 0).then_some(c.deck as i32))
}

fn hash(x: i32, y: i32) -> f32 {
    let mut h = (x as u32).wrapping_mul(73_856_093) ^ (y as u32).wrapping_mul(19_349_663) ^ 0x9E37_79B9;
    h ^= h >> 13;
    h = h.wrapping_mul(0x5bd1_e995);
    h ^= h >> 15;
    (h & 0xFFFF) as f32 / 65_535.0
}

// ─────────────────────────────── the landmarks ───────────────────────────────

/// A famous building, copied in voxels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Marco {
    /// Tokyo Skytree: the tallest thing on the island, beside Yoyogi Park.
    Skytree,
    /// Tokyo Metropolitan Government Building: twin towers on a podium —
    /// where it really is, and the top district's heart.
    Prefeitura,
    /// Tokyo Tower: red and white, tapering, in the Docks.
    TorreDeToquio,
    /// Mode Gakuen Cocoon Tower: an egg with a white lattice (real spot).
    Casulo,
    /// Shibuya 109: the silver cylinder at its corner (real spot).
    Cilindro109,
}

impl Marco {
    pub const TODOS: [Marco; 5] = [Marco::Skytree, Marco::Prefeitura, Marco::TorreDeToquio, Marco::Casulo, Marco::Cilindro109];

    pub fn nome(self) -> &'static str {
        match self {
            Marco::Skytree => "Kōgen Skytree",
            Marco::Prefeitura => "Tocho Twin Hall",
            Marco::TorreDeToquio => "Harbor Tower",
            Marco::Casulo => "Cocoon Tower",
            Marco::Cilindro109 => "Shibuya 109",
        }
    }

    pub fn centro(self) -> Vec2 {
        match self {
            Marco::Skytree => de_latlon(35.6690, 139.6975),
            Marco::Prefeitura => de_latlon(35.68955, 139.69175),
            Marco::TorreDeToquio => Vec2::new(-150.0, 580.0),
            Marco::Casulo => de_latlon(35.69160, 139.69670),
            Marco::Cilindro109 => de_latlon(35.65955, 139.69870),
        }
    }

    /// How far its footprint reaches from its centre (a bounding radius).
    fn alcance(self) -> f32 {
        match self {
            Marco::Skytree => 14.0,
            Marco::Prefeitura => 18.0,
            Marco::Cilindro109 => 10.0,
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
                // Round, tapering from r 13 to r 3, a crown of decks and a needle.
                if r > 13.0 {
                    return None;
                }
                if r <= 1.0 {
                    300
                } else if r <= 3.0 {
                    260
                } else {
                    (250.0 * (1.0 - (r - 3.0) / 10.0).powf(1.4)) as i32 + 6
                }
            }
            Marco::Prefeitura => {
                if !(ax <= 15.0 && az <= 10.0) {
                    return None;
                }
                let torre = |cx: f32| (p.x - cx).abs() <= 4.5 && az <= 5.5;
                let topo = |cx: f32| (p.x - cx).abs() <= 2.5 && az <= 3.5;
                if topo(-8.5) || topo(8.5) {
                    124
                } else if torre(-8.5) || torre(8.5) {
                    104
                } else if ax <= 4.0 && az <= 7.0 {
                    70
                } else {
                    30
                }
            }
            Marco::TorreDeToquio => {
                if m > 11.0 {
                    return None;
                }
                if m <= 0.6 {
                    190
                } else {
                    (160.0 * (1.0 - m / 11.0).powf(1.8)) as i32 + 4
                }
            }
            Marco::Casulo => {
                // An egg: an ellipse in plan, a dome in elevation.
                let e = ((p.x / 9.0).powi(2) + (p.y / 6.5).powi(2)).sqrt();
                if e > 1.0 {
                    return None;
                }
                (130.0 * (1.0 - e.powf(2.2)).sqrt()) as i32 + 4
            }
            Marco::Cilindro109 => {
                if (p - Vec2::new(4.0, -4.0)).length() <= 2.6 {
                    44
                } else if r <= 6.5 {
                    32
                } else {
                    return None;
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
    /// Neon billboards: round the Shibuya crossing and all over Kabukicho.
    Letreiros,
    /// Shipping containers in the Docks' yards, by colour.
    Conteiner(u8),
    /// A factory chimney (red and white bands) in the Docks.
    Chamine,
    /// A gas holder (a ribbed cylinder) in the Docks.
    Gasometro,
    Marco(Marco),
}

impl Estilo {
    /// The baked palette byte (mirror of the rasterizer's order).
    fn de(b: u8) -> Estilo {
        match b {
            1 => Estilo::Vidro,
            2 => Estilo::Tijolo,
            3 => Estilo::Branco,
            4 => Estilo::Letreiros,
            _ => Estilo::Concreto,
        }
    }
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
    /// The sidewalk, a footpath or a pedestrian street.
    Calcada,
    /// Open paved ground: plazas, car parks, the gaps between buildings.
    Praca,
    /// A park or garden.
    Parque,
    /// A rail line or rail yard.
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
    let h = 6 + (8.0 * t * t) as i32;
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
        Forma::Podio => (if f <= 0.6 { h + 10 } else { (h / 2).max(5) }, estilo),
        Forma::ArmazemComChamine => {
            if (p - canto).length() < 1.6 {
                (44 + (hash(cx, cz) * 20.0) as i32, Estilo::Chamine)
            } else {
                (h.min(12), estilo)
            }
        }
        Forma::Gasometro => {
            if raio > 0.9 {
                return None;
            }
            (14 + (hash(cx, cz + 3) * 10.0) as i32, Estilo::Gasometro)
        }
        Forma::Conteineres => {
            // Rows of 6 x 2.5 u containers with 1 u gaps, stacked 1-3 high.
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

/// What is at `q`.
pub fn chao_em(q: Vec2) -> Chao {
    let Some(c) = celula_em(q) else {
        return Chao::Mar;
    };
    match c.tipo {
        Tipo::Mar => return Chao::Mar,
        Tipo::Orla => return Chao::Orla(c.altura as i32),
        _ => {}
    }
    if let Some((m, h)) = marco_em(q) {
        return Chao::Predio { altura: h, estilo: Estilo::Marco(m) };
    }
    // Every landmark stands in its own plaza: the real buildings round it
    // give way.
    if Marco::TODOS.iter().any(|m| (q - m.centro()).length() < m.alcance() + 4.0) {
        return Chao::Praca;
    }
    match c.tipo {
        Tipo::Rua => Chao::Rua { avenida: false, faixa: false, zebra: false },
        Tipo::Avenida => Chao::Rua { avenida: true, faixa: false, zebra: false },
        Tipo::Faixa => Chao::Rua { avenida: true, faixa: true, zebra: false },
        Tipo::Zebra => Chao::Rua { avenida: true, faixa: false, zebra: true },
        Tipo::Calcada => Chao::Calcada,
        Tipo::Parque | Tipo::Bosque => Chao::Parque,
        Tipo::Trilho => Chao::Trilho,
        Tipo::Predio => Chao::Predio { altura: c.altura as i32, estilo: Estilo::de(c.estilo) },
        Tipo::Docas => docas_em(q),
        Tipo::Praca | Tipo::Mar | Tipo::Orla => Chao::Praca,
    }
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
        Chao::Praca => if quadriculado(2.0) { M::Concreto } else { M::ConcretoEscuro },
        // Ballast with two dark rails every few units.
        Chao::Trilho => if q.x.rem_euclid(3.0) < 0.5 || q.y.rem_euclid(3.0) < 0.5 { M::Concreto } else { M::ConcretoEscuro },
        Chao::Parque => return None,
        Chao::Predio { estilo, altura } => match estilo {
            Estilo::Conteiner(c) => cor_do_conteiner(c),
            Estilo::Marco(Marco::TorreDeToquio) | Estilo::Chamine => M::PetalaVermelha,
            Estilo::Marco(Marco::Skytree | Marco::Casulo) => M::Nuvem,
            _ => {
                // A neon rim round the roof, by district — on the billboard
                // blocks and the towers only: on every low roof it was noise.
                if estilo != Estilo::Letreiros && altura < 40 {
                    return Some(M::ConcretoEscuro);
                }
                let borda = [Vec2::X, -Vec2::X, Vec2::Y, -Vec2::Y]
                    .iter()
                    .any(|d| !matches!(chao_em(q + *d * 1.0), Chao::Predio { .. }));
                match (borda, distrito_em(q)) {
                    (true, _) if estilo == Estilo::Letreiros => if quadriculado(3.0) { M::NeonRosa } else { M::NeonAmarelo },
                    (true, 3) => if quadriculado(3.0) { M::NeonRosa } else { M::NeonCiano },
                    (true, 5) => M::NeonCiano,
                    (true, 4) => M::NeonAmarelo,
                    _ => M::ConcretoEscuro,
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
        Estilo::Concreto => if janela { lit(0.4) } else { M::Concreto },
        Estilo::Vidro => if janela || prof % 8 != 0 { lit(0.25) } else { M::ConcretoEscuro },
        Estilo::Tijolo => if janela && (gx + gz).rem_euclid(3) != 0 { lit(0.5) } else { M::Arenito },
        Estilo::Branco => if janela { lit(0.35) } else { M::Calcada },
        Estilo::Letreiros => {
            // SCREENS on the lower floors — framed panels of 10 x 6 blocks in
            // neon colours — and dark office glass with lit windows above.
            let telas_ate = (topo / 2).clamp(6, 36);
            if alto > telas_ate {
                return if janela { lit(0.3) } else { M::Vidro };
            }
            let (col, lin) = ((gx + gz).rem_euclid(10), alto.rem_euclid(6));
            if col == 0 || lin == 0 {
                return M::ConcretoEscuro; // the frame
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
        Estilo::Chamine => if (alto / 6) % 2 == 0 { M::PetalaVermelha } else { M::Nuvem },
        Estilo::Gasometro => if (gx + gz).rem_euclid(4) == 0 { M::ConcretoEscuro } else { M::Concreto },
        Estilo::Marco(m) => match m {
            Marco::Skytree => if (gx + gz + prof).rem_euclid(3) == 0 { M::Concreto } else { M::Nuvem },
            Marco::Prefeitura => if janela && (gx + gz).rem_euclid(2) == 0 { lit(0.6) } else { M::ConcretoEscuro },
            Marco::TorreDeToquio => if (alto / 10) % 2 == 0 { M::PetalaVermelha } else { M::Nuvem },
            Marco::Casulo => {
                if (gx + alto).rem_euclid(5) == 0 || (gz - alto).rem_euclid(5) == 0 { M::Nuvem } else { lit(0.3) }
            }
            Marco::Cilindro109 => if prof < 4 { M::NeonRosa } else if janela { M::JanelaAcesa } else { M::Concreto },
        },
    }
}

/// Trees, plants and gathering nodes grow ONLY in parks: on a street a rock
/// closes the way, and nothing grows on a roof.
pub fn so_no_parque(q: Vec2) -> bool {
    chao_em(q) == Chao::Parque
}

/// The mob level band at `q` (the district's), `None` at sea.
pub fn faixa_em(q: Vec2) -> Option<(u32, u32)> {
    (chao_em(q) != Chao::Mar).then(|| DISTRITOS[distrito_em(q)].nivel)
}

pub fn e_kogen(zona: &str) -> bool {
    zona == ZONA
}

/// The zone as the rest of the game reads it. Not in the `ARQUIPELAGO`: it is
/// reached only from Skyreach, by the flying bus (a later step).
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

    /// Six districts, each with streets, open ground and buildings, and the
    /// hub town on dry ground in the Docks.
    #[test]
    fn seis_distritos_ruas_e_lotes_abertos() {
        let mut ruas = [0u32; 6];
        let mut abertos = [0u32; 6];
        let mut predios = [0u32; 6];
        let passo = 3.0;
        let mut x = -340.0;
        while x < 340.0 {
            let mut z = -650.0;
            while z < 650.0 {
                let q = Vec2::new(x, z);
                let d = distrito_em(q);
                match chao_em(q) {
                    Chao::Rua { .. } => ruas[d] += 1,
                    Chao::Praca | Chao::Parque | Chao::Calcada => abertos[d] += 1,
                    Chao::Predio { .. } => predios[d] += 1,
                    _ => {}
                }
                z += passo;
            }
            x += passo;
        }
        for d in 0..6 {
            assert!(ruas[d] > 100, "{}: {} street samples", DISTRITOS[d].nome, ruas[d]);
            assert!(abertos[d] > 100, "{}: {} open samples", DISTRITOS[d].nome, abertos[d]);
            assert!(predios[d] > 100, "{}: {} building samples", DISTRITOS[d].nome, predios[d]);
        }
        assert_eq!(distrito_em(centro_da_cidade()), 0, "the hub is in the Docks");
        assert!(!matches!(chao_em(centro_da_cidade()), Chao::Mar | Chao::Orla(_)));
    }

    /// Every landmark stands where it says, on land, in one piece, and the
    /// Skytree is the tallest thing on the island.
    #[test]
    fn os_marcos_estao_de_pe() {
        let skytree = match chao_em(Marco::Skytree.centro()) {
            Chao::Predio { altura, .. } => altura,
            o => panic!("no Skytree: {o:?}"),
        };
        for m in Marco::TODOS {
            match chao_em(m.centro()) {
                Chao::Predio { estilo: Estilo::Marco(k), altura } => {
                    assert_eq!(k, m, "{} is covered by {:?}", m.nome(), k);
                    assert!(altura <= skytree, "{} is taller than the Skytree", m.nome());
                }
                o => panic!("{} missing: {o:?}", m.nome()),
            }
        }
    }

    /// Going north, the districts come in level order, and the real places
    /// land in theirs.
    #[test]
    fn distritos_do_sul_ao_norte() {
        assert_eq!(distrito_em(centro_da_cidade()), 0);
        assert_eq!(distrito_em(cruzamento()), 1, "the scramble is in Shibuya");
        assert_eq!(distrito_em(de_latlon(35.6764, 139.6993)), 2, "Meiji Shrine is in the forest");
        assert_eq!(distrito_em(de_latlon(35.6945, 139.7030)), 3, "Kabukicho is neon");
        assert_eq!(distrito_em(de_latlon(35.6930, 139.6950)), 4, "the towers are west of the station");
        assert_eq!(distrito_em(Marco::Prefeitura.centro()), 5);
        assert!(matches!(chao_em(cruzamento()), Chao::Rua { .. }), "the crossing is a street");
    }

    /// The Shuto expressway is baked as a deck over the streets.
    #[test]
    fn a_expressa_tem_deck() {
        let mut n = 0;
        let mut x = -340.0;
        while x < 340.0 {
            let mut z = -650.0;
            while z < 650.0 {
                n += deck_em(Vec2::new(x, z)).is_some() as u32;
                z += 2.0;
            }
            x += 2.0;
        }
        assert!(n > 500, "only {n} deck samples");
    }
}
