//! ABYSSIA — the bubble kingdom on the sea floor, levels 100-120.
//!
//! A kingdom inside a giant air bubble at the bottom of the sea (the owner:
//! "a submarine map like the fish-man island in One Piece"), and round it
//! the open seafloor, in RINGS that get harder outwards:
//!
//! * the BUBBLE (the town): the Coral Palace on its rock pillar, Mermaid
//!   Cove, the submarine dock — no mobs;
//! * the KELP FOREST, 100-105;
//! * the SHIPWRECK GRAVEYARD, 105-110;
//! * the LANTERN TRENCH, 110-115: sunk below the rest, lit only by what glows;
//! * the ABYSS RIM, 115-120: rock spires before the basin's wall.
//!
//! Everyone walks the seafloor (no oxygen): the one rule the owner wanted is
//! that every character wears a BUBBLE HELMET here (`client::luzes`).
//!
//! Like `kogen`, the relief is drawn here (`bloco_da_coluna`), one top block
//! per column, and so is the ground's paint (`pintura`).

use glam::Vec2;

pub const ZONA: &str = "ilha_abissal";
/// Bump when the relief changes: it names the client's height cache.
pub const REVISAO: u32 = 3;
pub const SEMENTE: i32 = 0x0AB1_5510;
pub const RAIO_BLOCOS: i32 = 1400;
/// The seafloor's level, in blocks (above the water line: the only water is
/// Mermaid Cove's).
pub const NIVEL_CHAO: i32 = 8;

/// The bubble kingdom's radius (units), centred on the map.
pub const BOLHA_RAIO: f32 = 125.0;
/// Where the rings end (units from the centre).
pub const KELP_ATE: f32 = 250.0;
pub const NAUFRAGIOS_ATE: f32 = 380.0;
pub const FOSSA_ATE: f32 = 500.0;
/// The basin's wall starts about here.
pub const PAREDE_DE: f32 = 640.0;

/// A ring of the seafloor: name and level band.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Anel {
    pub nome: &'static str,
    pub nivel: (u32, u32),
}

pub const ANEIS: [Anel; 5] = [
    Anel { nome: "the Bubble Kingdom", nivel: (100, 100) },
    Anel { nome: "the Kelp Forest", nivel: (100, 105) },
    Anel { nome: "the Shipwreck Graveyard", nivel: (105, 110) },
    Anel { nome: "the Lantern Trench", nivel: (110, 115) },
    Anel { nome: "the Abyss Rim", nivel: (115, 120) },
];

/// The ring `q` is in (0 = the bubble).
pub fn anel_em(q: Vec2) -> usize {
    let r = q.length();
    if r < BOLHA_RAIO {
        0
    } else if r < KELP_ATE {
        1
    } else if r < NAUFRAGIOS_ATE {
        2
    } else if r < FOSSA_ATE {
        3
    } else {
        4
    }
}

pub fn dentro_da_bolha(q: Vec2) -> bool {
    q.length() < BOLHA_RAIO
}

// ─────────────────────────────── the kingdom ───────────────────────────────

/// The town's square (the hub's NPCs ring it).
pub fn centro_da_cidade() -> Vec2 {
    Vec2::new(0.0, 62.0)
}

/// The Coral Palace's pillar: its centre and radius, and how high its top
/// stands above the floor (blocks).
pub const PALACIO: Vec2 = Vec2::new(0.0, -46.0);
pub const PALACIO_RAIO: f32 = 20.0;
pub const PALACIO_ALTO: i32 = 26;
/// The ramp up the pillar, from the town side: (foot, top), width.
const RAMPA_PE: Vec2 = Vec2::new(0.0, 8.0);
const RAMPA_TOPO: Vec2 = Vec2::new(0.0, -24.0);
const RAMPA_MEIA: f32 = 3.5;

/// Mermaid Cove: a lagoon inside the bubble, the only water on the map.
pub const ENSEADA: Vec2 = Vec2::new(72.0, -18.0);
pub const ENSEADA_RAIO: f32 = 26.0;

/// The submarine's berth, by the town (the way in and out).
pub fn doca_do_submarino() -> Vec2 {
    Vec2::new(-40.0, 92.0)
}

/// Story place ids for Abyssia (`objective_kind::LUGAR`): `PONTO_BASE + i`.
pub const PONTO_BASE: u16 = 90;
pub const NOMES_DOS_PONTOS: [&str; 6] = [
    "the kingdom's square",
    "the Kelp Forest",
    "the Shipwreck Graveyard",
    "the Lantern Trench",
    "the Abyss Rim",
    "the Coral Palace",
];

/// Where story place `p` is, if it is one of Abyssia's: open floor along
/// the ways out (`pinaculo_em` keeps four clear).
pub fn ponto(p: u16) -> Option<Vec2> {
    let i = p.checked_sub(PONTO_BASE)?;
    let caminho = |r: f32| {
        let a = 0.4f32;
        Vec2::new(a.cos(), a.sin()) * r
    };
    Some(match i {
        0 => centro_da_cidade(),
        1 => caminho(185.0),
        2 => caminho(310.0),
        3 => caminho(440.0),
        4 => caminho(570.0),
        5 => PALACIO + Vec2::new(0.0, 9.0),
        _ => return None,
    })
}

/// Small safe settlements, with real shops and residents.
#[derive(Clone, Copy, Debug)]
pub struct Povoado { pub nome: &'static str, pub centro: Vec2 }
pub const POVOADOS: [Povoado; 3] = [
    Povoado {nome:"Pearl Haven",centro:Vec2::new(-185.0,75.0)},
    Povoado {nome:"Shellwatch",centro:Vec2::new(285.0,100.0)},
    Povoado {nome:"Lantern Refuge",centro:Vec2::new(-80.0,-420.0)},
];
pub const RAIO_POVOADO: f32 = 27.0;
pub const CAMPOS_ENERGIA: [Vec2;4] = [Vec2::new(-30.0,190.0),Vec2::new(205.0,-200.0),Vec2::new(-360.0,-190.0),Vec2::new(300.0,480.0)];
pub const RAIO_CAMPO_ENERGIA: f32 = 23.0;
pub fn campo_energia(q:Vec2)->bool { CAMPOS_ENERGIA.iter().any(|c|c.distance(q)<RAIO_CAMPO_ENERGIA) }
pub fn no_povoado(q:Vec2)->bool { POVOADOS.iter().any(|p|p.centro.distance(q)<RAIO_POVOADO+5.0) }
fn distancia_segmento(q:Vec2,a:Vec2,b:Vec2)->f32 {
    let d=b-a;let t=((q-a).dot(d)/d.length_squared()).clamp(0.0,1.0);q.distance(a+d*t)
}
fn no_caminho(q:Vec2)->bool { POVOADOS.iter().any(|p|distancia_segmento(q,Vec2::ZERO,p.centro)<4.0) }

// ─────────────────────────────── the seafloor ───────────────────────────────

fn hash(x: i32, y: i32) -> f32 {
    let mut h = (x as u32).wrapping_mul(73_856_093) ^ (y as u32).wrapping_mul(19_349_663) ^ 0x51ED_270B;
    h ^= h >> 13;
    h = h.wrapping_mul(0x5bd1_e995);
    h ^= h >> 15;
    (h & 0xFFFF) as f32 / 65_535.0
}

/// Smooth value noise in 0..1.
fn ruido(p: Vec2) -> f32 {
    let (x0, z0) = (p.x.floor(), p.y.floor());
    let (fx, fz) = (p.x - x0, p.y - z0);
    let (sx, sz) = (fx * fx * (3.0 - 2.0 * fx), fz * fz * (3.0 - 2.0 * fz));
    let (ix, iz) = (x0 as i32, z0 as i32);
    let a = hash(ix, iz);
    let b = hash(ix + 1, iz);
    let c = hash(ix, iz + 1);
    let d = hash(ix + 1, iz + 1);
    let ab = a + (b - a) * sx;
    let cd = c + (d - c) * sx;
    ab + (cd - ab) * sz
}

fn fbm(p: Vec2) -> f32 {
    0.55 * ruido(p) + 0.3 * ruido(p * 2.03 + Vec2::splat(17.0)) + 0.15 * ruido(p * 4.1 - Vec2::splat(9.0))
}

fn suave(a: f32, b: f32, x: f32) -> f32 {
    let t = ((x - a) / (b - a)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Where the basin's wall starts along the direction of `q` (a ragged
/// circle).
fn parede_em(q: Vec2) -> f32 {
    let a = q.y.atan2(q.x);
    PAREDE_DE + 18.0 * (a * 3.0 + 0.7).sin() + 10.0 * (a * 7.0 - 1.1).sin() + 6.0 * (a * 13.0 + 2.0).sin()
}

/// The field bosses' arenas, weakest first (`bosses::da_zona` order).
pub fn arenas() -> [Vec2; 5] {
    let polar = |graus: f32, r: f32| {
        let a = graus.to_radians();
        Vec2::new(a.cos(), a.sin()) * r
    };
    [polar(35.0, 315.0), polar(150.0, 445.0), polar(255.0, 560.0), polar(215.0, 590.0), polar(335.0, 585.0)]
}
pub const RAIO_ARENA: f32 = 20.0;

/// The Sunken Galleon's portal (a Porão), by the biggest wreck.
pub fn portal_do_galeao() -> Vec2 {
    let a = 200f32.to_radians();
    Vec2::new(a.cos(), a.sin()) * 268.0
}

/// The wrecks of the Shipwreck Graveyard: (where, heading in quarter turns,
/// seed). Kept off the arenas and the portal.
pub fn naufragios() -> &'static [(Vec2, u8, i32)] {
    static N: std::sync::OnceLock<Vec<(Vec2, u8, i32)>> = std::sync::OnceLock::new();
    N.get_or_init(|| {
        let mut v = Vec::new();
        for k in 0..22 {
            let a = (k as f32 / 22.0 + hash(k, 3) * 0.03) * std::f32::consts::TAU;
            let r = KELP_ATE + 25.0 + hash(k, 9) * (NAUFRAGIOS_ATE - KELP_ATE - 50.0);
            let p = Vec2::new(a.cos(), a.sin()) * r;
            if arenas().iter().any(|c| c.distance(p) < RAIO_ARENA + 14.0) || p.distance(portal_do_galeao()) < 18.0 {
                continue;
            }
            v.push((p, (hash(k, 5) * 4.0) as u8 % 4, k));
        }
        v
    })
}

/// A ROCK SPIRE of the Abyss Rim at `q`, if any: its height in blocks over
/// the floor. Clear round the arenas and along four ways out.
fn pinaculo_em(q: Vec2) -> Option<i32> {
    let r = q.length();
    if no_povoado(q) || campo_energia(q) || no_caminho(q) { return None; }
    if r < FOSSA_ATE + 10.0 {
        return None;
    }
    if arenas().iter().any(|a| a.distance(q) < RAIO_ARENA + 8.0) {
        return None;
    }
    let ang = q.y.atan2(q.x);
    for k in 0..4 {
        let caminho = (k as f32 * std::f32::consts::FRAC_PI_2 + 0.4).rem_euclid(std::f32::consts::TAU);
        let da = (ang - caminho).rem_euclid(std::f32::consts::TAU);
        if da.min(std::f32::consts::TAU - da) * r < 9.0 {
            return None;
        }
    }
    let n = ruido(q / 22.0 + Vec2::splat(41.0));
    (n > 0.68).then(|| 14 + ((n - 0.68) * 140.0) as i32)
}

/// Blocks above `NIVEL_CHAO` of the open seafloor at `q` (dunes, the trench).
fn relevo_do_fundo(q: Vec2) -> f32 {
    let r = q.length();
    let mut h = (fbm(q / 70.0) - 0.5) * 7.0 + (ruido(q / 16.0) - 0.5) * 1.6;
    // The trench: a sunken ring, its floor six blocks down.
    let dentro = suave(NAUFRAGIOS_ATE - 5.0, NAUFRAGIOS_ATE + 25.0, r) * (1.0 - suave(FOSSA_ATE - 25.0, FOSSA_ATE + 5.0, r));
    h -= 6.0 * dentro;
    // The bubble's floor: flat (the town), easing into the dunes past it.
    h *= suave(BOLHA_RAIO - 15.0, BOLHA_RAIO + 10.0, r);
    for p in POVOADOS { h *= suave(RAIO_POVOADO,RAIO_POVOADO+10.0,p.centro.distance(q)); }
    for c in CAMPOS_ENERGIA { h *= suave(24.0,34.0,c.distance(q)); }
    h
}

/// The top block of column `(bx, bz)`. The ONLY source of Abyssia's relief.
pub fn bloco_da_coluna(bx: i32, bz: i32) -> i32 {
    use crate::terreno::BLOCO;
    let q = Vec2::new(bx as f32, bz as f32) * BLOCO;
    altura_em(q)
}

/// The top block at `q`, in blocks.
pub fn altura_em(q: Vec2) -> i32 {
    let r = q.length();
    // The basin's wall.
    let w = parede_em(q);
    if r > w {
        return NIVEL_CHAO + (((r - w) * 2.4) as i32).min(90) + 2;
    }
    // The Coral Palace: its pillar, the ramp up, the towers on top.
    if let Some(h) = palacio_em(q) {
        return NIVEL_CHAO + h;
    }
    // Mermaid Cove: banks down to the water.
    let de = q.distance(ENSEADA);
    if de < ENSEADA_RAIO {
        let t = (de / ENSEADA_RAIO).clamp(0.0, 1.0);
        return (-3.0 + (NIVEL_CHAO + 3) as f32 * t.powf(1.6)).round() as i32;
    }
    if let Some(h) = pinaculo_em(q) {
        return NIVEL_CHAO + h;
    }
    let mut h = relevo_do_fundo(q);
    // Flat arenas.
    for a in arenas() {
        let d = a.distance(q);
        if d < RAIO_ARENA + 8.0 {
            let t = suave(RAIO_ARENA, RAIO_ARENA + 8.0, d);
            h *= t;
        }
    }
    (NIVEL_CHAO + h.round() as i32).max(1)
}

/// The palace at `q`: blocks above the floor, if it is the palace.
fn palacio_em(q: Vec2) -> Option<i32> {
    // The ramp: from the town side up to the pillar's rim.
    let eixo = RAMPA_TOPO - RAMPA_PE;
    let comp = eixo.length();
    let t = (q - RAMPA_PE).dot(eixo / comp);
    let lado = (q - RAMPA_PE).perp_dot(eixo / comp).abs();
    let p = q - PALACIO;
    let r = p.length();
    if r > PALACIO_RAIO {
        if (0.0..=comp).contains(&t) && lado <= RAMPA_MEIA {
            return Some(((PALACIO_ALTO as f32) * t / comp).round().max(1.0) as i32);
        }
        return None;
    }
    // The pillar's top: a courtyard with the palace's towers.
    let mut h = PALACIO_ALTO;
    if r < 5.0 {
        h += 34; // the great spire
    } else {
        for k in 0..6 {
            let a = k as f32 / 6.0 * std::f32::consts::TAU + 0.5;
            let c = Vec2::new(a.cos(), a.sin()) * 13.0;
            if p.distance(c) < 2.8 {
                h += 20;
                break;
            }
        }
    }
    Some(h)
}

/// The palace column at `q`, for its walls' look.
pub fn e_palacio(q: Vec2) -> bool {
    q.distance(PALACIO) <= PALACIO_RAIO
}

// ─────────────────────────────── the paint ───────────────────────────────

/// The ground's paint at `q` (`terreno::pintura_do_chao`).
pub fn pintura(q: Vec2) -> Option<crate::terreno::Material> {
    use crate::terreno::Material as M;
    let r = q.length();
    if no_povoado(q) { return Some(M::MarmoreSombra); }
    if campo_energia(q) { return Some(M::RochaAbissal); }
    if no_caminho(q) { return Some(M::AreiaFunda); }
    let xadrez = |passo: f32| ((q.x / passo).floor() as i32 + (q.y / passo).floor() as i32).rem_euclid(2) == 0;
    if r > parede_em(q) {
        return Some(if (q.x * 0.7 + q.y).rem_euclid(9.0) < 1.0 { M::RochaEscura } else { M::RochaAbissal });
    }
    if pinaculo_em(q).is_some() {
        return Some(if hash(q.x as i32, q.y as i32) < 0.06 { M::NeonCiano } else { M::RochaAbissal });
    }
    if dentro_da_bolha(q) {
        if q.distance(ENSEADA) < ENSEADA_RAIO {
            return Some(M::Areia);
        }
        if e_palacio(q) {
            let p = q - PALACIO;
            return Some(if p.length() < 5.0 {
                M::PetalaRosa
            } else if (p.length() - 10.0).abs() < 0.5 {
                M::NeonCiano
            } else if xadrez(1.5) {
                M::Marmore
            } else {
                M::MarmoreSombra
            });
        }
        // Shell paving with six curved streets and a coral rim. Broad neon
        // grid lines looked like a circuit board instead of a sea kingdom.
        if r > BOLHA_RAIO - 8.0 {
            return Some(if hash((q.x / 4.0) as i32, (q.y / 4.0) as i32) < 0.25 { M::Coral } else { M::PetalaRosa });
        }
        let radial = (q.y.atan2(q.x) * 3.0 / std::f32::consts::PI).rem_euclid(1.0);
        if radial < 0.035 || radial > 0.965 || (r - 72.0).abs() < 2.5 {
            return Some(M::Calcada);
        }
        return Some(if xadrez(4.0) { M::Marmore } else { M::MarmoreSombra });
    }
    if arenas().iter().any(|a| a.distance(q) < RAIO_ARENA) {
        let a = arenas().into_iter().min_by(|a, b| a.distance(q).total_cmp(&b.distance(q))).unwrap();
        return Some(if (a.distance(q) % 6.0) < 0.5 { M::NeonCiano } else { M::RochaAbissal });
    }
    // The open floor, by ring.
    let n = ruido(q / 9.0);
    Some(match anel_em(q) {
        1 => if n < 0.35 { M::Alga } else { M::AreiaFunda },
        2 => if n < 0.25 { M::RochaAbissal } else { M::AreiaFunda },
        // The trench glows where the rock is cracked.
        3 => {
            if hash(q.x as i32 * 3, q.y as i32 * 3) < 0.012 {
                M::NeonCiano
            } else if n < 0.5 {
                M::RochaAbissal
            } else {
                M::AreiaFunda
            }
        }
        _ => if n < 0.55 { M::RochaAbissal } else { M::AreiaFunda },
    })
}

/// The palace's walls: white marble courses, coral pink bands, glowing
/// windows. `alto` is the voxel's height above the floor, in blocks.
pub fn fachada(q: Vec2, gx: i32, gz: i32, alto: i32) -> crate::terreno::Material {
    use crate::terreno::Material as M;
    let p = q - PALACIO;
    if alto <= PALACIO_ALTO {
        // The pillar: dark rock with veins.
        return if (gx + gz + alto).rem_euclid(11) == 0 { M::NeonCiano } else { M::RochaAbissal };
    }
    let acima = alto - PALACIO_ALTO;
    if acima % 6 == 5 {
        return M::PetalaRosa;
    }
    if p.length() < 5.0 && acima % 6 == 2 && (gx + gz).rem_euclid(3) == 0 {
        return M::NeonCiano;
    }
    if acima % 6 == 2 && (gx - gz).rem_euclid(4) == 0 {
        return M::JanelaAcesa;
    }
    M::Marmore
}

// ─────────────────────────────── what grows ───────────────────────────────

/// Trees here are KELP (the forest ring) and CORAL (everywhere else on the
/// open floor); `f` is the species draw. `None`: nothing grows at `q`.
pub fn arvore_em(q: Vec2, f: f32) -> Option<crate::terreno::Arvore> {
    use crate::terreno::Arvore;
    if dentro_da_bolha(q) || no_povoado(q) || campo_energia(q) || no_caminho(q) || pinaculo_em(q).is_some() || q.length() > parede_em(q) - 4.0 {
        return None;
    }
    if arenas().iter().any(|a| a.distance(q) < RAIO_ARENA + 6.0)
        || naufragios().iter().any(|(p, _, _)| p.distance(q) < 9.0)
        || q.distance(portal_do_galeao()) < 10.0
    {
        return None;
    }
    Some(match anel_em(q) {
        1 => if f < 0.85 { Arvore::Alga } else { Arvore::Coral },
        3 => {
            // The trench is sparse: one in three draws grows anything.
            if f > 0.35 {
                return None;
            }
            Arvore::Coral
        }
        _ => if f < 0.3 { Arvore::Alga } else if f < 0.75 { Arvore::Coral } else { return None },
    })
}

/// How dense the trees are at `q`, against the biome's base.
pub fn densidade_em(q: Vec2) -> f32 {
    match anel_em(q) {
        0 => 0.0,
        1 => 2.6,
        3 => 0.7,
        _ => 1.0,
    }
}

/// Plants grow on the open floor, not in the kingdom.
pub fn planta_livre(q: Vec2) -> bool {
    !dentro_da_bolha(q) && !no_povoado(q) && !no_caminho(q) && pinaculo_em(q).is_none() && q.length() < parede_em(q) - 2.0
}

/// The mob level band at `q`; `None` in the kingdom and in the wall.
pub fn faixa_em(q: Vec2) -> Option<(u32, u32)> {
    let a = anel_em(q);
    (a != 0 && !no_povoado(q) && !campo_energia(q) && q.length() < parede_em(q)).then(|| ANEIS[a].nivel)
}

pub fn e_abissal(zona: &str) -> bool {
    zona == ZONA
}

// ─────────────────────────────── the lights ───────────────────────────────

/// One light of the deep (`client::luzes`): shell lanterns in the kingdom,
/// glowing coral and jellyfish outside.
#[derive(Debug, Clone, Copy)]
pub struct LuzDoFundo {
    pub pos: Vec2,
    /// Height above the floor (units).
    pub alto: f32,
    pub cor: [f32; 3],
    pub raio: f32,
    /// A jellyfish drifts (bobs up and down) and is drawn as one.
    pub agua_viva: bool,
    pub coral: bool,
    pub seed: i32,
}

pub fn luzes() -> &'static [LuzDoFundo] {
    static L: std::sync::OnceLock<Vec<LuzDoFundo>> = std::sync::OnceLock::new();
    L.get_or_init(|| {
        let mut v = Vec::new();
        // The kingdom: shell lanterns every ~16 units on the paving's seams.
        let mut k = 0;
        let mut z = -BOLHA_RAIO;
        while z < BOLHA_RAIO {
            let mut x = -BOLHA_RAIO;
            while x < BOLHA_RAIO {
                k += 1;
                let q = Vec2::new(x + 5.0, z + 5.0);
                x += 20.0;
                if q.length() > BOLHA_RAIO - 10.0 || e_palacio(q) || q.distance(ENSEADA) < ENSEADA_RAIO + 2.0 {
                    continue;
                }
                let cor = if k % 2 == 0 { [0.45, 0.95, 1.0] } else { [1.0, 0.55, 0.8] };
                v.push(LuzDoFundo { pos: q, alto: 5.6, cor, raio: 20.0, agua_viva: false, coral: false, seed: k });
            }
            z += 20.0;
        }
        // Bioluminescent coral gardens: low lights illuminate the ground.
        let mut coral_at = |q:Vec2,seed:i32| {
            if e_palacio(q) || pinaculo_em(q).is_some() || campo_energia(q) { return; }
            let cor=match seed.rem_euclid(3) {0=>[0.25,1.0,0.85],1=>[1.0,0.35,0.65],_=>[0.50,0.65,1.0]};
            v.push(LuzDoFundo {pos:q,alto:1.2,cor,raio:12.0,agua_viva:false,coral:true,seed});
        };
        for p in POVOADOS {
            for k in 0..8 {let a=k as f32*std::f32::consts::TAU/8.0;coral_at(p.centro+Vec2::new(a.cos(),a.sin())*23.0,k+1200);}
            let n=(p.centro.length()/22.0) as i32;
            for k in 0..n {let q=p.centro*(k as f32/n as f32);let d=p.centro.normalize().perp();coral_at(q+d*6.0,1400+k);}
        }
        for k in 0..36 {let a=k as f32*std::f32::consts::TAU/36.0;coral_at(Vec2::new(a.cos(),a.sin())*105.0,k+1600);}
        for x in -24..=24 {for z in -24..=24 {
            let q=Vec2::new(x as f32*24.0+hash(x,z)*8.0,z as f32*24.0);
            if q.length()<PAREDE_DE-20.0 && q.length()>BOLHA_RAIO+10.0 && hash(x+200,z)>0.84 {coral_at(q,2000+x*49+z);}
        }}
        // Outside: jellyfish drifting over the floor, thickest in the trench.
        for i in 0..900 {
            let a = hash(i, 1) * std::f32::consts::TAU;
            let r = BOLHA_RAIO + 15.0 + hash(i, 2) * (PAREDE_DE - BOLHA_RAIO - 30.0);
            let q = Vec2::new(a.cos(), a.sin()) * r;
            let anel = anel_em(q);
            let chance = match anel {
                3 => 1.0,
                4 => 0.5,
                _ => 0.25,
            };
            if hash(i, 7) > chance || pinaculo_em(q).is_some() {
                continue;
            }
            let cor = match i % 3 {
                0 => [0.5, 0.9, 1.0],
                1 => [0.85, 0.5, 1.0],
                _ => [0.4, 1.0, 0.75],
            };
            v.push(LuzDoFundo { pos: q, alto: 3.0 + hash(i, 4) * 5.0, cor, raio: 9.0, agua_viva: true, coral: false, seed: i });
        }
        v
    })
}

/// The zone as the rest of the game reads it.
pub const DEF: crate::terreno::DefIlha = crate::terreno::DefIlha {
    zona: ZONA,
    nome: "Abyssia",
    semente: SEMENTE,
    raio_blocos: RAIO_BLOCOS,
    bioma: crate::terreno::Bioma::Abissal,
    centro: [-1500.0, -4500.0],
    nivel: (100, 120),
};

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn campos_de_energia_tem_depositos_acessiveis() {
        use crate::terreno::{energia_da_coluna, Bioma, Gerador, BLOCO};
        let ger = Gerador::da_ilha_abissal();
        for centro in CAMPOS_ENERGIA {
            let mut depositos = 0;
            let raio = (RAIO_CAMPO_ENERGIA / BLOCO).ceil() as i32;
            let bx = (centro.x / BLOCO).round() as i32;
            let bz = (centro.y / BLOCO).round() as i32;
            for z in bz-raio..=bz+raio {
                for x in bx-raio..=bx+raio {
                    let p = Vec2::new(x as f32, z as f32) * BLOCO;
                    let topo = ger.bloco_em(x, z);
                    if let Some(no) = energia_da_coluna(Bioma::Abissal, x, z, topo, &ger, false) {
                        assert!(campo_energia(no.centro));
                        assert!(!no_povoado(no.centro));
                        assert!(altura_em(no.centro) >= 1);
                        assert!((altura_em(no.centro) - NIVEL_CHAO).abs() <= 1);
                        depositos += 1;
                    }
                }
            }
            assert!(depositos >= 8, "energy field at {centro:?} has only {depositos} deposits");
        }
    }

    /// The rings run outwards in level, the kingdom is flat, the wall closes
    /// the basin, and the only water is the cove's.
    #[test]
    fn aneis_bolha_parede_e_enseada() {
        for w in ANEIS.windows(2) {
            assert!(w[1].nivel.0 >= w[0].nivel.0);
        }
        assert_eq!(ANEIS[4].nivel.1, 120);
        assert_eq!(altura_em(centro_da_cidade()), NIVEL_CHAO, "the town is not flat");
        assert!(altura_em(Vec2::new(0.0, 690.0)) > NIVEL_CHAO + 40, "no wall at the edge");
        assert!(altura_em(ENSEADA) < 0, "the cove has no water");
        let mut seco = 0;
        for i in 0..2000 {
            let a = hash(i, 11) * std::f32::consts::TAU;
            let r = hash(i, 12) * PAREDE_DE;
            let q = Vec2::new(a.cos(), a.sin()) * r;
            if q.distance(ENSEADA) > ENSEADA_RAIO + 1.0 && altura_em(q) >= 1 {
                seco += 1;
            } else if q.distance(ENSEADA) > ENSEADA_RAIO + 1.0 {
                panic!("water outside the cove at {q}");
            }
        }
        assert!(seco > 1500);
    }

    /// The palace stands on its pillar and the ramp climbs it a block at a time.
    #[test]
    fn o_palacio_e_a_rampa() {
        assert!(altura_em(PALACIO) >= NIVEL_CHAO + PALACIO_ALTO + 30);
        let eixo = RAMPA_TOPO - RAMPA_PE;
        let mut ultima = altura_em(RAMPA_PE);
        for i in 1..=200 {
            let p = RAMPA_PE + eixo * (i as f32 / 200.0);
            let h = altura_em(p);
            assert!(h - ultima <= 1, "a step of {} blocks on the ramp at {p}", h - ultima);
            ultima = h;
        }
        assert!(ultima >= NIVEL_CHAO + PALACIO_ALTO - 1);
    }

    /// The arenas are flat open ground in the ring of their boss's level.
    #[test]
    fn as_arenas_sao_planas() {
        let chefes = crate::bosses::da_zona(ZONA);
        assert_eq!(chefes.len(), 5, "five field bosses including the sanctuary Hydra");
        for (a, c) in arenas().iter().zip(chefes) {
            let (lo, hi) = faixa_em(*a).expect("an arena off the floor");
            assert!(c.nivel >= lo && c.nivel <= hi + 1, "{} ({}) in a {lo}-{hi} ring", c.nome, c.nivel);
            for d in [Vec2::X, Vec2::Y, -Vec2::X, -Vec2::Y] {
                assert!((altura_em(*a + d * (RAIO_ARENA - 2.0)) - altura_em(*a)).abs() <= 1, "{}'s arena is not flat", c.nome);
            }
        }
    }
}
