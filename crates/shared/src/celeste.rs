//! SKYREACH (`ilha_celeste`): the level 50-60 island, a ruined city in the
//! sky.
//!
//! The owner chose to split the Plateau (40-50) and give 50-60 an island of
//! its own, themed as a sky city. The terrain is a heightmap — one top block
//! per column, nothing walkable above the ground — so "floating" is drawn the
//! way the Magic Island does it: tall mesas whose cliffs drop to a floor
//! BELOW sea level. Below sea level is a wall for walking (`Ilha::subida`),
//! so nobody falls, and the client hides those columns and draws the cloud
//! sea in their place (`Terreno::sem_mar`, `ilhas_aereas`).
//!
//! The relief is DRAWN, not rolled: `platos()` and `pontes()` are the whole
//! map, and `bloco_da_coluna` is its only source, called by client and server
//! alike so they cannot disagree on where the ground is.
//!
//! Layout: the arrival plateau with the town in the middle, then one plateau
//! per level band, each a step higher than the last, chained by bridges that
//! climb gently between them (the walk rule is 1 block per column, 1.0 u/u;
//! every bridge here stays under 0.15 u/u). Two small resource islets hang
//! off the chain.

use glam::Vec2;

pub const ZONA: &str = "ilha_celeste";
/// Planting seed: the relief does not depend on it, the vegetation does.
pub const SEMENTE: i32 = 0x5C1E_A7E0;
/// Zone radius in BLOCKS: the whole layout plus a margin of sky.
pub const RAIO_BLOCOS: i32 = 1200;
/// The floor under the cliffs, below sea level: a wall, drawn as clouds.
pub const NIVEL_FUNDO: i32 = -10;
/// Half width of a bridge, in units. 8 u across: two A* cells (4 u), so
/// auto-walk finds the way, and still a choke point for fights.
pub const MEIA_PONTE: f32 = 4.0;

/// One plateau: a flat-topped mesa with a ragged rim.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Plato {
    pub nome: &'static str,
    pub centro: Vec2,
    pub raio: f32,
    /// Height of the top, in units.
    pub altura: f32,
    /// Mob levels on it (min, max). 0 = no hunting (the town plateau).
    pub nivel: (u16, u16),
}

const fn plato(nome: &'static str, x: f32, y: f32, raio: f32, altura: f32, nivel: (u16, u16)) -> Plato {
    Plato { nome, centro: Vec2::new(x, y), raio, altura, nivel }
}

/// The plateaus. Index 0 is the arrival with the town; 1-4 climb through the
/// level bands; 5-6 are the resource islets. Tops stay within 14-21 u (grass ends at 24 u, minus the swells): the
/// ground's material goes by absolute height, and above that band the
/// plateaus turned to bare rock and then snow. ORDER IS THE ID: bridges and,
/// later, spawn tables point at these indices.
pub const PLATOS: [Plato; 7] = [
    plato("Cloudharbor", 0.0, 0.0, 78.0, 14.0, (0, 0)),
    plato("Windmill Terraces", -235.0, -125.0, 95.0, 16.0, (50, 52)),
    plato("Forge of the Titan", -300.0, 175.0, 90.0, 18.0, (52, 55)),
    plato("Storm Gardens", 40.0, 305.0, 100.0, 20.0, (54, 57)),
    plato("Throne of the Sky", 305.0, 120.0, 105.0, 21.0, (57, 60)),
    plato("Shardfall Islet", 215.0, -205.0, 52.0, 17.0, (52, 56)),
    plato("Rookery Islet", -60.0, -330.0, 48.0, 15.0, (50, 53)),
];

/// Which plateaus a bridge joins. Progression is a chain (0-1-2-3-4) with
/// the islets on the side; the throne is reached only through the gardens.
pub const PONTES: [(usize, usize); 6] = [(0, 1), (1, 2), (2, 3), (3, 4), (0, 5), (1, 6)];

/// The rim radius of `p` in direction `ang`: three harmonics, the same recipe
/// as the Magic Island's islets (two read as a clover).
pub fn raio_na_direcao(p: &Plato, ang: f32) -> f32 {
    let fase = p.centro.x * 0.023 + p.centro.y * 0.031;
    p.raio
        * (1.0
            + 0.09 * (ang * 3.0 + fase).sin()
            + 0.06 * (ang * 5.0 - fase * 1.3).sin()
            + 0.04 * (ang * 9.0 + fase * 0.7).sin())
}

/// The plateau under `q`, if any.
pub fn plato_em(q: Vec2) -> Option<(usize, &'static Plato)> {
    PLATOS.iter().enumerate().find(|(_, p)| {
        let d = q - p.centro;
        let d2 = d.length_squared();
        if d2 > (p.raio * 1.25).powi(2) {
            return false;
        }
        if d2 <= (p.raio * 0.78).powi(2) {
            return true;
        }
        d2 <= raio_na_direcao(p, d.y.atan2(d.x)).powi(2)
    })
}

/// The point where the bridge from `a` toward `b` leaves `a`'s rim.
fn borda(a: &Plato, b: &Plato) -> Vec2 {
    let dir = (b.centro - a.centro).normalize();
    a.centro + dir * raio_na_direcao(a, dir.y.atan2(dir.x)) * 0.97
}

/// Distance to the nearest bridge and the deck height there, in units.
fn ponte_em(q: Vec2) -> Option<(f32, f32)> {
    PONTES
        .iter()
        .map(|&(i, j)| {
            let (a, b) = (&PLATOS[i], &PLATOS[j]);
            let (pa, pb) = (borda(a, b), borda(b, a));
            let ab = pb - pa;
            let t = ((q - pa).dot(ab) / ab.length_squared()).clamp(0.0, 1.0);
            let dist = (pa + ab * t).distance(q);
            // Smoothstep, so the deck meets each rim level instead of kinking.
            let s = t * t * (3.0 - 2.0 * t);
            (dist, a.altura + (b.altura - a.altura) * s)
        })
        .min_by(|x, y| x.0.total_cmp(&y.0))
}

/// Is `q` on a bridge (and not on a plateau)? Vegetation stays off bridges:
/// a tree in an 8 u deck closes it for auto-walk.
pub fn na_ponte(q: Vec2) -> bool {
    plato_em(q).is_none() && ponte_em(q).is_some_and(|(d, _)| d <= MEIA_PONTE)
}

/// Is `q` walkable ground (plateau or bridge)?
pub fn e_chao(q: Vec2) -> bool {
    plato_em(q).is_some() || ponte_em(q).is_some_and(|(d, _)| d <= MEIA_PONTE)
}

/// Small deterministic hash in 0..1, for the ruins' layout.
fn hash(x: i32, y: i32) -> f32 {
    let mut h = (x as u32).wrapping_mul(0x8DA6_B343) ^ (y as u32).wrapping_mul(0xD816_3841) ^ 0x5C1E_A7E0;
    h ^= h >> 13;
    h = h.wrapping_mul(0x85EB_CA6B);
    h ^= h >> 16;
    (h & 0xFFFF) as f32 / 65535.0
}

/// The ruined COLONNADE of a hunting plateau: a ring of thick columns at a
/// third of the radius, some fallen (missing), some broken short — the
/// footprint of the temple the city was built around. The town plateau has
/// none: the town stands there. Extra height in blocks.
fn colunata(q: Vec2, p: &Plato, i: usize) -> i32 {
    if p.nivel.0 == 0 {
        return 0;
    }
    const COLUNAS: usize = 12;
    let r = p.raio * 0.34;
    for k in 0..COLUNAS {
        // Fallen: about one in four never stood.
        if hash(i as i32 * 31 + k as i32, 7) < 0.25 {
            continue;
        }
        let a = k as f32 / COLUNAS as f32 * std::f32::consts::TAU + i as f32;
        let c = p.centro + Vec2::new(a.cos(), a.sin()) * r;
        // 2 u across (4x4 blocks): thick enough to read from the game camera.
        if (q.x - c.x).abs() <= 1.0 && (q.y - c.y).abs() <= 1.0 {
            return 10 + (hash(k as i32, i as i32 * 13) * 12.0) as i32;
        }
    }
    0
}

/// A ruined pillar here? Pillars stand on a 14 u grid, jittered, about one
/// cell in five — never near the plateau centre (the town, the arrival, the
/// fights), the rim, or a bridge landing. Returns the extra height in blocks.
fn pilar(q: Vec2, p: &Plato) -> i32 {
    const CELULA: f32 = 14.0;
    let (cx, cy) = ((q.x / CELULA).floor() as i32, (q.y / CELULA).floor() as i32);
    if hash(cx, cy) > 0.2 {
        return 0;
    }
    let base = Vec2::new(cx as f32 + 0.25 + hash(cy, cx) * 0.5, cy as f32 + 0.25 + hash(cx + 7, cy - 3) * 0.5) * CELULA;
    let d = base - p.centro;
    let rel = d.length() / p.raio;
    if !(0.42..=0.82).contains(&rel) {
        return 0;
    }
    if PONTES.iter().any(|&(i, j)| {
        let (a, b) = (&PLATOS[i], &PLATOS[j]);
        borda(a, b).distance(base) < 26.0 || borda(b, a).distance(base) < 26.0
    }) {
        return 0;
    }
    // A 3x3-block column (1.5 u across), 6 to 14 blocks tall: broken stumps
    // and the odd standing one.
    if (q.x - base.x).abs() <= 0.75 && (q.y - base.y).abs() <= 0.75 {
        6 + (hash(cx - 11, cy + 5) * 8.0) as i32
    } else {
        0
    }
}

/// Gentle relief on a plateau top, in units: low swells that keep every
/// slope walkable, fading to flat near the centre where the town and the
/// arrival need level ground.
fn relevo(q: Vec2, p: &Plato, t: f32) -> f32 {
    let ondas = (q.x * 0.037 + 0.9).sin() * (q.y * 0.033 - 0.4).cos() * 1.6
        + (q.x * 0.081 - q.y * 0.063).sin() * 0.6
        + (q.x * 0.41 + 0.2).sin() * (q.y * 0.37 - 0.8).sin() * 0.3;
    let sem_cidade = if p.nivel.0 == 0 { 0.0 } else { 1.0 };
    // Zero at the rim as well as low in the middle: the bridges land on the
    // plateau's nominal height, and a swell at the rim turned the landing
    // into a wall.
    let borda = (4.0 * (1.0 - t)).min(1.0);
    ondas * (0.2 + 0.8 * t) * borda * (0.5 + 0.5 * sem_cidade)
}

/// The top block of column `(bx, bz)`. The ONLY source of Skyreach's relief.
pub fn bloco_da_coluna(bx: i32, bz: i32) -> i32 {
    use crate::terreno::BLOCO;
    let q = Vec2::new(bx as f32, bz as f32) * BLOCO;
    // The plateau first: bridges run between rims, but testing them first
    // would let a deck cut a trench across a ragged rim.
    if let Some((i, p)) = plato_em(q) {
        let d = q - p.centro;
        let t = (d.length() / raio_na_direcao(p, d.y.atan2(d.x))).clamp(0.0, 1.0);
        let h = p.altura + relevo(q, p, t);
        return (h / BLOCO).round() as i32 - 1 + pilar(q, p).max(colunata(q, p, i));
    }
    if let Some((dist, h)) = ponte_em(q) {
        if dist <= MEIA_PONTE {
            let topo = (h / BLOCO).round() as i32 - 1;
            // A one-block curb along each edge: it reads as a bridge from the
            // game camera, and one block is still a step, never a wall.
            return if dist > MEIA_PONTE - BLOCO { topo + 1 } else { topo };
        }
    }
    NIVEL_FUNDO
}

pub fn e_celeste(zona: &str) -> bool {
    zona == ZONA
}

/// The zone as the rest of the game reads it. Not in the `ARQUIPELAGO` yet:
/// travel, story and the world map come with the next phase.
pub const DEF: crate::terreno::DefIlha = crate::terreno::DefIlha {
    zona: ZONA,
    nome: "Skyreach",
    semente: SEMENTE,
    raio_blocos: RAIO_BLOCOS,
    bioma: crate::terreno::Bioma::Floresta,
    centro: [2600.0, -1400.0],
    nivel: (50, 60),
};

#[cfg(test)]
mod testes {
    use super::*;
    use crate::terreno::BLOCO;

    /// Every bridge is walkable: the deck never climbs more than a block per
    /// column, and both ends meet their plateau at a step, not a wall.
    #[test]
    fn toda_ponte_e_caminhavel() {
        for &(i, j) in &PONTES {
            let (a, b) = (&PLATOS[i], &PLATOS[j]);
            let (pa, pb) = (borda(a, b), borda(b, a));
            let n = (pa.distance(pb) / BLOCO) as i32;
            let mut ant = None;
            for k in 0..=n + 8 {
                let q = pa + (pb - pa).normalize() * ((k - 4) as f32 * BLOCO);
                let b = bloco_da_coluna((q.x / BLOCO).round() as i32, (q.y / BLOCO).round() as i32);
                assert!(b > 0, "bridge {i}-{j} drops into the void at step {k}");
                if let Some(x) = ant {
                    assert!((b - x as i32).abs() <= 1, "bridge {i}-{j} jumps {x} -> {b} at step {k}");
                }
                ant = Some(b);
            }
        }
    }

    /// The layout fits the zone, and plateaus never touch (a touching pair
    /// would join without a bridge and break the progression chain).
    #[test]
    fn platos_cabem_e_nao_se_tocam() {
        let lim = RAIO_BLOCOS as f32 * BLOCO - 40.0;
        for (i, a) in PLATOS.iter().enumerate() {
            assert!(a.centro.length() + a.raio * 1.2 < lim, "{} outside the zone", a.nome);
            for b in &PLATOS[i + 1..] {
                assert!(a.centro.distance(b.centro) > (a.raio + b.raio) * 1.2 + 20.0, "{} touches {}", a.nome, b.nome);
            }
        }
    }

    /// Off the plateaus and bridges is the void: below sea level.
    #[test]
    fn fora_do_chao_e_vazio() {
        assert_eq!(bloco_da_coluna(0, 1150), NIVEL_FUNDO);
        assert!(bloco_da_coluna(0, 0) > 0);
    }
}
