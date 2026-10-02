//! KŌGEN-TŌ (`ilha_kogen`): the level 80-100 island, a futuristic city at
//! night, reached only from Skyreach by a flying bus.
//!
//! The owner, 02/10/2026: "a futuristic island ... like Tokyo but in island
//! format ... robots, giant robot bosses, electrical and mechanical animals,
//! much light, roads, cars, high buildings". Always night, six districts.
//! Then, on the first preview: "everything is practically the same ... copy
//! famous buildings of Tokyo and New York in voxel style, I want something
//! like a Times Square in the centre".
//!
//! The terrain is a heightmap — one top block per column — so a building is
//! a SOLID block of columns that you walk round, like Stormkeep's walls: the
//! streets are the gaps. A silhouette can only narrow going up (setbacks,
//! tapers, spires), which suits the landmarks: Empire State, Chrysler, One
//! World Trade, the Flatiron, the Tokyo Metropolitan Government's twin
//! towers, Tokyo Tower, the Skytree, the Cocoon Tower, Shibuya's 109.
//!
//! The layout is drawn, not rolled: `chao_em` / `bloco_da_coluna` are its
//! only source, called by client and server alike.

use glam::Vec2;

pub const ZONA: &str = "ilha_kogen";
/// Bump on any change to the layout: it is in the server's height cache key.
pub const REVISAO: u32 = 2;
/// Planting seed: the relief does not depend on it, the decoration does.
pub const SEMENTE: i32 = 0x0C06_E170;
/// Zone radius in BLOCKS: the island plus a margin of sea.
pub const RAIO_BLOCOS: i32 = 1300;
/// The city's ground, in blocks (sea level is 0).
pub const NIVEL_CHAO: i32 = 20;
/// The sea floor past the shore, in blocks.
pub const NIVEL_FUNDO: i32 = -6;

/// One grid cell, in units: the street plus the lot.
pub const QUADRA: f32 = 40.0;
/// A street's width, in units: two A* cells.
pub const RUA: f32 = 8.0;
/// An avenue's width, in units, every `AVENIDA_A_CADA` cells.
pub const AVENIDA: f32 = 14.0;
pub const AVENIDA_A_CADA: i32 = 3;
/// The sidewalk ring inside each lot, in units.
pub const CALCADA: f32 = 2.5;

/// The coast's mean radius, in units.
pub const RAIO_COSTA: f32 = 560.0;
/// The Central Spire's district radius, in units.
pub const RAIO_CENTRO: f32 = 140.0;

/// "Broadway": the diagonal boulevard across the grid, through the centre.
pub const BROADWAY_ANGULO_GRAUS: f32 = -18.0;
pub const BROADWAY_MEIA: f32 = 7.0;
/// The bowtie ("Kōgen Square", the Times Square of the island): from -this
/// to +this along Broadway, narrow at the knot and wide at both ends.
pub const PRACA_COMPRIMENTO: f32 = 70.0;

fn broadway() -> Vec2 {
    let a = BROADWAY_ANGULO_GRAUS.to_radians();
    Vec2::new(a.cos(), a.sin())
}

/// A district: its name, its level band and how tall it builds.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Distrito {
    pub nome: &'static str,
    pub nivel: (u32, u32),
    /// Building height range, in blocks above the street.
    pub altura: (i32, i32),
    /// Fraction of lots left open (plazas and parks: where the robots roam).
    pub abertos: f32,
}

/// The six districts. Index 0 is the arrival (the hub town). ORDER IS THE ID.
pub const DISTRITOS: [Distrito; 6] = [
    Distrito { nome: "Harbor Docks", nivel: (80, 83), altura: (6, 14), abertos: 0.42 },
    Distrito { nome: "Market Streets", nivel: (83, 86), altura: (8, 22), abertos: 0.38 },
    Distrito { nome: "Industrial Ring", nivel: (86, 90), altura: (8, 20), abertos: 0.40 },
    Distrito { nome: "Neon District", nivel: (90, 94), altura: (24, 64), abertos: 0.34 },
    Distrito { nome: "Corporate Heights", nivel: (94, 97), altura: (40, 96), abertos: 0.32 },
    Distrito { nome: "Central Spire", nivel: (97, 100), altura: (50, 110), abertos: 0.30 },
];

/// The Docks sector is centred on the south (+z); the outer districts follow
/// round the island (east, north-east, north-west, west of the map).
const ANGULO_DOCAS: f32 = std::f32::consts::FRAC_PI_2;

/// Where the hub town sits: the Docks, near the south shore.
pub fn centro_da_cidade() -> Vec2 {
    Vec2::new(0.0, RAIO_COSTA * 0.72)
}

/// The coast's radius in direction `ang` (a ragged shore, never a circle).
pub fn raio_da_costa(ang: f32) -> f32 {
    RAIO_COSTA * (1.0 + 0.07 * (ang * 3.0 + 0.4).sin() + 0.05 * (ang * 5.0 - 1.1).sin() + 0.03 * (ang * 11.0).sin())
}

/// Which district `q` is in.
pub fn distrito_em(q: Vec2) -> usize {
    if q.length() < RAIO_CENTRO {
        return 5;
    }
    let a = (ANGULO_DOCAS - q.y.atan2(q.x)).rem_euclid(std::f32::consts::TAU);
    ((a + std::f32::consts::TAU / 10.0) / (std::f32::consts::TAU / 5.0)) as usize % 5
}

fn hash(x: i32, y: i32) -> f32 {
    let mut h = (x as u32).wrapping_mul(73_856_093) ^ (y as u32).wrapping_mul(19_349_663) ^ 0x9E37_79B9;
    h ^= h >> 13;
    h = h.wrapping_mul(0x5bd1_e995);
    h ^= h >> 15;
    (h & 0xFFFF) as f32 / 65_535.0
}

/// The grid cell of `q`, and where inside it `q` is (0..QUADRA on each axis).
fn celula(q: Vec2) -> ((i32, i32), Vec2) {
    let cx = (q.x / QUADRA).floor() as i32;
    let cz = (q.y / QUADRA).floor() as i32;
    ((cx, cz), q - Vec2::new(cx as f32, cz as f32) * QUADRA)
}

/// The street width on the edge of cell index `i` (an avenue every few).
fn largura_da_rua(i: i32) -> f32 {
    if i.rem_euclid(AVENIDA_A_CADA) == 0 { AVENIDA } else { RUA }
}

/// The centre of the lot of the cell containing `q`.
pub fn centro_do_lote(q: Vec2) -> Vec2 {
    let ((cx, cz), _) = celula(q);
    let (lx, lz) = (largura_da_rua(cx), largura_da_rua(cz));
    Vec2::new(cx as f32 * QUADRA + lx + (QUADRA - lx) * 0.5, cz as f32 * QUADRA + lz + (QUADRA - lz) * 0.5)
}

/// The Shibuya-style scramble crossing, in the Neon District: the street
/// corner next to the Cocoon Tower.
pub fn cruzamento() -> Vec2 {
    let ((cx, cz), _) = celula(Marco::Casulo.centro());
    Vec2::new(cx as f32 * QUADRA, cz as f32 * QUADRA) + Vec2::splat(largura_da_rua(cx).max(largura_da_rua(cz)) * 0.5)
}

// ─────────────────────────────── the landmarks ───────────────────────────────

/// A famous building, copied in voxels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Marco {
    /// Tokyo Skytree: the tallest thing on the island, at the centre.
    Skytree,
    /// One Times Square: the slim tower of screens at the square's end.
    UmaKogen,
    /// Empire State: setbacks, a crown and a mast.
    Empire,
    /// Chrysler: a stepped crown of arches and a needle.
    Chrysler,
    /// Tokyo Metropolitan Government Building: twin towers on a podium.
    Prefeitura,
    /// One World Trade Center: a tapering square tower and a spire.
    UmWtc,
    /// The Flatiron: a wedge beside Broadway.
    Flatiron,
    /// Tokyo Tower: red and white, tapering, in the Docks.
    TorreDeToquio,
    /// Mode Gakuen Cocoon Tower: an egg with a white lattice.
    Casulo,
    /// Shibuya 109: the silver cylinder at a corner.
    Cilindro109,
}

impl Marco {
    pub const TODOS: [Marco; 10] = [
        Marco::Skytree,
        Marco::UmaKogen,
        Marco::Empire,
        Marco::Chrysler,
        Marco::Prefeitura,
        Marco::UmWtc,
        Marco::Flatiron,
        Marco::TorreDeToquio,
        Marco::Casulo,
        Marco::Cilindro109,
    ];

    pub fn nome(self) -> &'static str {
        match self {
            Marco::Skytree => "Kōgen Skytree",
            Marco::UmaKogen => "One Kōgen",
            Marco::Empire => "Empire Tower",
            Marco::Chrysler => "Crown Building",
            Marco::Prefeitura => "Twin Hall",
            Marco::UmWtc => "Freedom Spire",
            Marco::Flatiron => "Flatiron",
            Marco::TorreDeToquio => "Harbor Tower",
            Marco::Casulo => "Cocoon Tower",
            Marco::Cilindro109 => "Shibuya 109",
        }
    }

    /// Where it stands (its lot's centre, or a point beside Broadway).
    pub fn centro(self) -> Vec2 {
        let u = broadway();
        match self {
            Marco::Skytree => centro_do_lote(Vec2::new(10.0, -110.0)),
            Marco::UmaKogen => u * -(PRACA_COMPRIMENTO + 12.0),
            Marco::Empire => centro_do_lote(Vec2::new(-314.0, 102.0)),
            Marco::Chrysler => centro_do_lote(Vec2::new(-270.0, 20.0)),
            Marco::Prefeitura => centro_do_lote(Vec2::new(-330.0, 190.0)),
            Marco::UmWtc => centro_do_lote(Vec2::new(-400.0, 60.0)),
            Marco::Flatiron => u * -205.0 + Vec2::new(-u.y, u.x) * (BROADWAY_MEIA + 7.0),
            Marco::TorreDeToquio => centro_do_lote(Vec2::new(110.0, 320.0)),
            Marco::Casulo => centro_do_lote(Vec2::new(-176.0, -243.0)),
            Marco::Cilindro109 => centro_do_lote(Vec2::new(285.0, 93.0)),
        }
    }

    /// How far its footprint reaches from its centre (a bounding radius).
    fn alcance(self) -> f32 {
        match self {
            Marco::Skytree => 14.0,
            Marco::Prefeitura => 18.0,
            _ => 16.0,
        }
    }

    /// Its height (blocks above the street) at offset `p` from its centre,
    /// or `None` outside its footprint.
    fn altura(self, p: Vec2) -> Option<i32> {
        let (ax, az) = (p.x.abs(), p.y.abs());
        let m = ax.max(az);
        let r = p.length();
        let caixa = |hx: f32, hz: f32| ax <= hx && az <= hz;
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
            Marco::UmaKogen => {
                if !caixa(6.0, 5.0) {
                    return None;
                }
                if caixa(1.0, 1.0) { 112 } else { 96 }
            }
            Marco::Empire => {
                if !caixa(12.0, 9.0) {
                    return None;
                }
                if caixa(0.6, 0.6) {
                    196
                } else if caixa(1.5, 1.5) {
                    176
                } else if caixa(3.5, 3.0) {
                    158
                } else if caixa(6.0, 4.5) {
                    146
                } else if caixa(9.0, 7.0) {
                    110
                } else {
                    20
                }
            }
            Marco::Chrysler => {
                if !caixa(11.0, 11.0) {
                    return None;
                }
                if m <= 0.6 {
                    188
                } else if m <= 8.0 {
                    // The crown: arches stepping in every block and a half.
                    let degrau = ((8.0 - m) / 1.4).floor() as i32;
                    120 + degrau * 7
                } else {
                    24
                }
            }
            Marco::Prefeitura => {
                if !caixa(15.0, 10.0) {
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
            Marco::UmWtc => {
                if !caixa(11.0, 11.0) {
                    return None;
                }
                if m <= 0.6 {
                    250
                } else if m <= 4.0 {
                    176
                } else {
                    // Tapering from the base square to the top square.
                    (24.0 + 152.0 * (11.0 - m) / 7.0) as i32
                }
            }
            Marco::Flatiron => {
                // A wedge, its point towards the square, along Broadway.
                let u = broadway();
                let t = p.dot(u);
                let s = p.dot(Vec2::new(-u.y, u.x));
                if !(-10.0..=10.0).contains(&t) {
                    return None;
                }
                let largura = 6.0 * (t + 10.0) / 20.0;
                if s.abs() > largura {
                    return None;
                }
                if s.abs() > largura - 0.6 { 50 } else { 48 }
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
                if (p - Vec2::new(5.5, -5.5)).length() <= 3.5 {
                    48
                } else if r <= 9.0 {
                    34
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
    /// Neon billboards: round the square and here and there in the Neon District.
    Letreiros,
    /// Shipping containers in the Docks' yards, by colour.
    Conteiner(u8),
    /// A factory chimney (red and white bands) in the Industrial Ring.
    Chamine,
    /// A gas holder (a ribbed cylinder) in the Industrial Ring.
    Gasometro,
    Marco(Marco),
}

/// What stands on a piece of the city.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Chao {
    /// The sea, past the shore.
    Mar,
    /// The sea wall band at the shore.
    Orla,
    /// A street; `avenida` for the wide ones, `faixa` on a lane marking,
    /// `zebra` on a crosswalk stripe of the scramble.
    Rua { avenida: bool, faixa: bool, zebra: bool },
    /// The sidewalk round a lot.
    Calcada,
    /// An open lot: a paved plaza.
    Praca,
    /// An open lot: a park.
    Parque,
    /// Kōgen Square, the bowtie on Broadway.
    Largo,
    /// A building, its roof `altura` blocks above the street.
    Predio { altura: i32, estilo: Estilo },
}

/// A building's SHAPE inside its lot (the generic ones; landmarks have their own).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Forma {
    Caixa,
    /// A podium the whole lot, a tower on part of it.
    Podio,
    /// Three setbacks, the "wedding cake".
    Bolo,
    /// Twin towers on a low podium.
    Gemeas,
    /// A round tower.
    Redonda,
    /// A slim tower on a small podium.
    Fina,
    /// A low warehouse with a tall chimney in a corner.
    ArmazemComChamine,
    /// A gas holder.
    Gasometro,
    /// A yard of container stacks.
    Conteineres,
}

/// The shape, palette and height of the generic building in cell (cx, cz).
fn projeto(cx: i32, cz: i32, d: usize, centro_do_lote: Vec2) -> (Forma, Estilo, i32) {
    let dist = DISTRITOS[d];
    let t = hash(cx * 3 + 11, cz * 5 - 3);
    let altura = dist.altura.0 + ((dist.altura.1 - dist.altura.0) as f32 * t * t) as i32;
    let h = hash(cx + 17, cz + 29);
    let paleta = hash(cx - 5, cz + 13);
    let forma = match d {
        0 => if h < 0.45 { Forma::Conteineres } else if h < 0.8 { Forma::Caixa } else { Forma::Podio },
        1 => if h < 0.35 { Forma::Caixa } else if h < 0.6 { Forma::Podio } else if h < 0.8 { Forma::Bolo } else { Forma::Redonda },
        2 => if h < 0.35 { Forma::ArmazemComChamine } else if h < 0.55 { Forma::Gasometro } else { Forma::Caixa },
        3 => if h < 0.3 { Forma::Fina } else if h < 0.55 { Forma::Podio } else if h < 0.8 { Forma::Bolo } else { Forma::Redonda },
        4 => if h < 0.3 { Forma::Podio } else if h < 0.5 { Forma::Gemeas } else if h < 0.7 { Forma::Fina } else if h < 0.85 { Forma::Redonda } else { Forma::Bolo },
        _ => if h < 0.35 { Forma::Bolo } else if h < 0.65 { Forma::Podio } else { Forma::Fina },
    };
    // Round the square, every building carries billboards.
    let na_praca = centro_do_lote.length() < RAIO_CENTRO;
    let estilo = match forma {
        Forma::Conteineres => Estilo::Conteiner(0),
        Forma::Gasometro => Estilo::Gasometro,
        _ if na_praca => Estilo::Letreiros,
        _ => match d {
            0 => if paleta < 0.5 { Estilo::Tijolo } else { Estilo::Concreto },
            1 => if paleta < 0.55 { Estilo::Tijolo } else if paleta < 0.8 { Estilo::Branco } else { Estilo::Letreiros },
            2 => Estilo::Concreto,
            3 => if paleta < 0.4 { Estilo::Letreiros } else if paleta < 0.75 { Estilo::Vidro } else { Estilo::Concreto },
            4 => if paleta < 0.6 { Estilo::Vidro } else if paleta < 0.85 { Estilo::Branco } else { Estilo::Concreto },
            _ => if paleta < 0.5 { Estilo::Vidro } else { Estilo::Branco },
        },
    };
    (forma, estilo, altura)
}

/// The height (and look) of the generic building at `local` in its lot of
/// size `tam` (the area inside the sidewalk), or `None` for open ground.
fn predio_no_lote(cx: i32, cz: i32, d: usize, local: Vec2, tam: Vec2, centro_do_lote: Vec2) -> Option<(i32, Estilo)> {
    let (forma, estilo, h) = projeto(cx, cz, d, centro_do_lote);
    let p = local - tam * 0.5;
    let (fx, fz) = (p.x.abs() / (tam.x * 0.5), p.y.abs() / (tam.y * 0.5));
    let f = fx.max(fz);
    let r = p.length() / (tam.x.min(tam.y) * 0.5);
    let canto = tam * Vec2::new(0.32, -0.32);
    let extra = |h: i32| -> i32 {
        // A rooftop antenna on tall ones, a water tank on low ones.
        if h > 30 && hash(cx, cz + 99) < 0.4 && p.length() < 0.8 {
            h + 18
        } else if h <= 30 && hash(cx + 7, cz) < 0.5 && (p - tam * 0.22).length() < 2.0 {
            h + 4
        } else {
            h
        }
    };
    let alt = match forma {
        Forma::Caixa => extra(h),
        Forma::Podio => if f <= 0.6 { extra(h) } else { (h / 4).max(6) },
        Forma::Bolo => {
            if f <= 0.45 {
                extra(h)
            } else if f <= 0.72 {
                h * 7 / 10
            } else {
                h * 4 / 10
            }
        }
        Forma::Gemeas => {
            let torre = (fx - 0.55).abs() <= 0.3 && fz <= 0.55;
            if torre { extra(h) } else { 8 }
        }
        Forma::Redonda => if r <= 0.85 { extra(h) } else { return None },
        Forma::Fina => {
            if f <= 0.4 {
                extra(h)
            } else if f <= 0.7 {
                10
            } else {
                return None;
            }
        }
        Forma::ArmazemComChamine => {
            if (p - canto).length() < 1.6 {
                return Some((44 + (hash(cx, cz) * 20.0) as i32, Estilo::Chamine));
            }
            h.min(12)
        }
        Forma::Gasometro => if r <= 0.9 { 14 + (hash(cx, cz + 3) * 10.0) as i32 } else { return None },
        Forma::Conteineres => {
            // Rows of 6 x 2.5 u containers with 1 u gaps, stacked 1-3 high.
            let (i, j) = ((local.x / 7.0).floor() as i32, (local.y / 3.5).floor() as i32);
            let (u, v) = (local.x.rem_euclid(7.0), local.y.rem_euclid(3.5));
            if u > 6.0 || v > 2.5 {
                return None;
            }
            let pilha = 1 + (hash(cx * 31 + i, cz * 17 + j) * 3.0) as i32;
            let cor = (hash(cx * 13 + i, cz * 7 + j) * 5.0) as u8;
            return Some((pilha * 5, Estilo::Conteiner(cor)));
        }
    };
    Some((alt, estilo))
}

/// What is at `q`.
pub fn chao_em(q: Vec2) -> Chao {
    let r = q.length();
    let costa = raio_da_costa(q.y.atan2(q.x));
    if r > costa {
        return Chao::Mar;
    }
    if r > costa - 10.0 {
        return Chao::Orla;
    }
    if let Some((m, h)) = marco_em(q) {
        return Chao::Predio { altura: h, estilo: Estilo::Marco(m) };
    }
    // Broadway and its bowtie square.
    let u = broadway();
    let t = q.dot(u);
    let s = q.dot(Vec2::new(-u.y, u.x)).abs();
    if t.abs() <= PRACA_COMPRIMENTO && s <= BROADWAY_MEIA + t.abs() * 0.45 {
        return Chao::Largo;
    }
    if s <= BROADWAY_MEIA {
        let faixa = s < 0.4 && t.rem_euclid(6.0) < 3.0;
        return Chao::Rua { avenida: true, faixa, zebra: false };
    }
    let ((cx, cz), local) = celula(q);
    let (lx, lz) = (largura_da_rua(cx), largura_da_rua(cz));
    if local.x < lx || local.y < lz {
        let avenida = (local.x < lx && lx > RUA) || (local.y < lz && lz > RUA);
        let faixa = avenida
            && ((local.x < lx && (local.x - lx * 0.5).abs() < 0.4 && (q.y.rem_euclid(6.0)) < 3.0)
                || (local.y < lz && (local.y - lz * 0.5).abs() < 0.4 && (q.x.rem_euclid(6.0)) < 3.0));
        // The scramble: diagonal zebra stripes over the whole crossing.
        let zebra = (q - cruzamento()).length() < 13.0
            && ((q.x + q.y).rem_euclid(2.4) < 1.0 || (q.x - q.y).rem_euclid(2.4) < 1.0);
        return Chao::Rua { avenida, faixa: faixa && !zebra, zebra };
    }
    // Inside the lot: sidewalk ring, then the lot itself.
    let dentro = Vec2::new(local.x - lx, local.y - lz);
    let tamanho = Vec2::new(QUADRA - lx, QUADRA - lz);
    if dentro.x < CALCADA || dentro.y < CALCADA || dentro.x > tamanho.x - CALCADA || dentro.y > tamanho.y - CALCADA {
        return Chao::Calcada;
    }
    let d = distrito_em(q);
    if hash(cx, cz) < DISTRITOS[d].abertos {
        return if hash(cx + 101, cz - 77) < 0.5 { Chao::Praca } else { Chao::Parque };
    }
    let lote = dentro - Vec2::splat(CALCADA);
    let tam = tamanho - Vec2::splat(2.0 * CALCADA);
    let centro = Vec2::new(cx as f32 * QUADRA + lx, cz as f32 * QUADRA + lz) + Vec2::splat(CALCADA) + tam * 0.5;
    match predio_no_lote(cx, cz, d, lote, tam, centro) {
        Some((altura, estilo)) => Chao::Predio { altura, estilo },
        None => Chao::Praca,
    }
}

/// The top block of column `(bx, bz)`. The ONLY source of Kōgen-tō's relief.
pub fn bloco_da_coluna(bx: i32, bz: i32) -> i32 {
    use crate::terreno::BLOCO;
    let q = Vec2::new(bx as f32, bz as f32) * BLOCO;
    match chao_em(q) {
        Chao::Mar => NIVEL_FUNDO,
        // The shore slopes from the city down to the water in ten units.
        Chao::Orla => {
            let costa = raio_da_costa(q.y.atan2(q.x));
            let t = ((costa - q.length()) / 10.0).clamp(0.0, 1.0);
            (1.0 + (NIVEL_CHAO - 1) as f32 * t).round() as i32
        }
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
        Chao::Orla => M::Concreto,
        Chao::Rua { zebra: true, .. } => M::Nuvem,
        Chao::Rua { faixa: true, .. } => M::FaixaDePista,
        Chao::Rua { .. } => M::Asfalto,
        Chao::Calcada => if quadriculado(1.5) { M::Concreto } else { M::ConcretoEscuro },
        Chao::Praca => if quadriculado(2.0) { M::Concreto } else { M::ConcretoEscuro },
        // The square: dark paving with glowing lines every few units.
        Chao::Largo => {
            if q.x.rem_euclid(8.0) < 0.5 || q.y.rem_euclid(8.0) < 0.5 {
                M::NeonCiano
            } else if quadriculado(2.0) {
                M::ConcretoEscuro
            } else {
                M::Asfalto
            }
        }
        Chao::Parque => return None,
        Chao::Predio { estilo, .. } => match estilo {
            Estilo::Conteiner(c) => cor_do_conteiner(c),
            Estilo::Marco(Marco::TorreDeToquio) | Estilo::Chamine => M::PetalaVermelha,
            Estilo::Marco(Marco::Skytree | Marco::Casulo) => M::Nuvem,
            Estilo::Marco(Marco::UmaKogen) => M::NeonRosa,
            _ => {
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
        Estilo::Letreiros | Estilo::Marco(Marco::UmaKogen) => {
            // Times Square: SCREENS on the lower floors — framed panels of
            // 10 x 6 blocks in neon colours — and dark office glass with lit
            // windows above. One Kōgen is screens all the way up.
            let telas_ate = if estilo == Estilo::Marco(Marco::UmaKogen) { topo } else { (topo / 2).min(36) };
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
            Marco::Empire => {
                if topo > 170 {
                    M::Nuvem
                } else if (gx + gz).rem_euclid(3) == 0 && janela {
                    M::JanelaAcesa
                } else {
                    M::Concreto
                }
            }
            Marco::Chrysler => {
                if topo > 120 && prof < 9 {
                    // The crown: silver arches with triangular windows.
                    if (gx + gz + prof).rem_euclid(4) == 0 { M::JanelaAcesa } else { M::Nuvem }
                } else if janela {
                    lit(0.5)
                } else {
                    M::Calcada
                }
            }
            Marco::Prefeitura => if janela && (gx + gz).rem_euclid(2) == 0 { lit(0.6) } else { M::ConcretoEscuro },
            Marco::UmWtc => if (gx - gz).rem_euclid(6) == 0 { M::Nuvem } else { lit(0.2) },
            Marco::Flatiron => if janela { lit(0.55) } else { M::Arenito },
            Marco::TorreDeToquio => if (alto / 10) % 2 == 0 { M::PetalaVermelha } else { M::Nuvem },
            Marco::Casulo => {
                if (gx + alto).rem_euclid(5) == 0 || (gz - alto).rem_euclid(5) == 0 { M::Nuvem } else { lit(0.3) }
            }
            Marco::Cilindro109 => if prof < 4 { M::NeonRosa } else if janela { M::JanelaAcesa } else { M::Concreto },
            Marco::UmaKogen => M::NeonRosa,
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

    /// Six districts on the island, each with streets and open lots to hunt
    /// on, Kōgen Square at the centre, and the hub town on dry ground.
    #[test]
    fn seis_distritos_ruas_e_lotes_abertos() {
        let mut ruas = [0u32; 6];
        let mut abertos = [0u32; 6];
        let mut predios = [0u32; 6];
        let passo = 4.0;
        let mut x = -RAIO_COSTA;
        while x < RAIO_COSTA {
            let mut z = -RAIO_COSTA;
            while z < RAIO_COSTA {
                let q = Vec2::new(x, z);
                let d = distrito_em(q);
                match chao_em(q) {
                    Chao::Rua { .. } => ruas[d] += 1,
                    Chao::Praca | Chao::Parque | Chao::Largo => abertos[d] += 1,
                    Chao::Predio { .. } => predios[d] += 1,
                    _ => {}
                }
                z += passo;
            }
            x += passo;
        }
        for d in 0..6 {
            assert!(ruas[d] > 100, "{}: {} street samples", DISTRITOS[d].nome, ruas[d]);
            assert!(abertos[d] > 100, "{}: {} open lot samples", DISTRITOS[d].nome, abertos[d]);
            assert!(predios[d] > 100, "{}: {} building samples", DISTRITOS[d].nome, predios[d]);
        }
        assert_eq!(distrito_em(Vec2::ZERO), 5);
        assert_eq!(distrito_em(centro_da_cidade()), 0, "the hub is in the Docks");
        assert!(chao_em(centro_da_cidade()) != Chao::Mar);
        assert_eq!(chao_em(Vec2::ZERO), Chao::Largo, "Kōgen Square is the centre");
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

    /// Going round the island, the outer districts come in level order.
    #[test]
    fn distritos_em_ordem_de_nivel() {
        for (d, ang) in [(0usize, 90.0f32), (1, 18.0), (2, -54.0), (3, -126.0), (4, 162.0)] {
            let a = ang.to_radians();
            let q = Vec2::new(a.cos(), a.sin()) * 350.0;
            assert_eq!(distrito_em(q), d, "angle {ang}");
        }
    }
}
