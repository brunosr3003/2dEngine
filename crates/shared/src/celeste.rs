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
//! Layout (owner, 02/10/2026): "instead of a big island, several small and
//! medium islands connected through clouds". Fifteen islands spread over
//! the four level bands, the town island in the middle, joined by CLOUD
//! PATHS: curving, puffy, white, a thin deck rather than a stone bridge.
//! Every path climbs gently (the walk rule is 1 block per column).

use glam::Vec2;

pub const ZONA: &str = "ilha_celeste";
/// Bump on any change to the layout: it is in the server's height cache key.
pub const REVISAO: u32 = 1;
/// Planting seed: the relief does not depend on it, the vegetation does.
pub const SEMENTE: i32 = 0x5C1E_A7E0;
/// Zone radius in BLOCKS: the whole layout plus a margin of sky.
pub const RAIO_BLOCOS: i32 = 1200;
/// The floor under the cliffs, below sea level: a wall, drawn as clouds.
pub const NIVEL_FUNDO: i32 = -10;
/// Half width of a cloud path, in units, before its puffs: 11 u across, a
/// bit under three A* cells (4 u), so auto-walk always finds the way.
pub const MEIA_PONTE: f32 = 5.5;

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

/// The islands. Index 0 is the arrival with the town. ORDER IS THE ID:
/// paths and, later, spawn tables point at these indices. Tops stay within
/// 14-21 u: the ground's material goes by absolute height, and above that
/// band grass turns to bare rock and snow.
pub const PLATOS: [Plato; 15] = [
    plato("Cloudharbor", 0.0, 0.0, 62.0, 14.0, (0, 0)),
    // 50-52, west
    plato("Lantern Isle", -150.0, -60.0, 40.0, 15.0, (50, 51)),
    plato("Feather Rise", -230.0, 40.0, 46.0, 16.0, (51, 52)),
    plato("Dewdrop Isle", -130.0, 110.0, 30.0, 15.0, (50, 52)),
    // 52-55, south
    plato("Choir Steps", -210.0, 200.0, 44.0, 17.0, (52, 53)),
    plato("Forge of the Titan", -90.0, 270.0, 54.0, 18.0, (53, 55)),
    plato("Halo Garden", 30.0, 190.0, 36.0, 17.0, (52, 54)),
    // 54-57, south-east
    plato("Bellspire", 130.0, 300.0, 48.0, 19.0, (54, 55)),
    plato("Storm Gardens", 270.0, 240.0, 58.0, 19.0, (55, 57)),
    plato("Prism Isle", 200.0, 110.0, 32.0, 18.0, (54, 56)),
    // 57-60, north-east
    plato("Seraph Watch", 320.0, 60.0, 46.0, 20.0, (57, 58)),
    plato("Throne of the Sky", 380.0, -90.0, 64.0, 21.0, (58, 60)),
    plato("Aurora Islet", 230.0, -110.0, 30.0, 20.0, (57, 59)),
    // north of the town
    plato("Rookery", -30.0, -170.0, 34.0, 15.0, (50, 53)),
    plato("Shardfall", 110.0, -150.0, 36.0, 16.0, (52, 56)),
];

/// The cloud paths. A main road climbs the bands (0-1-2-4-5-7-8-10-11) and
/// side paths reach the small islands and close a few loops.
pub const PONTES: [(usize, usize); 20] = [
    (0, 1), (1, 2), (2, 4), (4, 5), (5, 7), (7, 8), (8, 10), (10, 11),
    (2, 3), (3, 6), (5, 6), (6, 9), (8, 9), (9, 10),
    (0, 13), (13, 1), (0, 14), (14, 12), (12, 11), (12, 10),
];

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

/// The point where the path from `a` toward `b` leaves `a`'s rim.
fn borda(a: &Plato, b: &Plato) -> Vec2 {
    let dir = (b.centro - a.centro).normalize();
    a.centro + dir * raio_na_direcao(a, dir.y.atan2(dir.x)) * 0.97
}

/// One cloud path, precomputed: a curve sampled as a polyline, with the deck
/// height at each sample and its bounding box for a cheap rejection.
struct Caminho {
    pontos: Vec<(Vec2, f32)>,
    min: Vec2,
    max: Vec2,
}

const AMOSTRAS: usize = 32;
/// Widest a path gets with its puffs, for the bounding box.
const LARGURA_MAX: f32 = MEIA_PONTE + 3.0;

fn caminhos() -> &'static [Caminho] {
    static C: std::sync::OnceLock<Vec<Caminho>> = std::sync::OnceLock::new();
    C.get_or_init(|| {
        PONTES
            .iter()
            .enumerate()
            .map(|(k, &(i, j))| {
                let (a, b) = (&PLATOS[i], &PLATOS[j]);
                let (pa, pb) = (borda(a, b), borda(b, a));
                // The curve: a quadratic Bezier pushed sideways by a fifth of
                // its length, alternating side, so paths drift like clouds
                // instead of running ruler-straight.
                let lado = Vec2::new(-(pb - pa).y, (pb - pa).x).normalize();
                let sinal = if k % 2 == 0 { 1.0 } else { -1.0 };
                let ctrl = (pa + pb) * 0.5 + lado * pa.distance(pb) * 0.2 * sinal;
                let pontos: Vec<(Vec2, f32)> = (0..=AMOSTRAS)
                    .map(|n| {
                        let t = n as f32 / AMOSTRAS as f32;
                        let q = pa * (1.0 - t) * (1.0 - t) + ctrl * 2.0 * t * (1.0 - t) + pb * t * t;
                        let s = t * t * (3.0 - 2.0 * t);
                        (q, a.altura + (b.altura - a.altura) * s)
                    })
                    .collect();
                let (mut min, mut max) = (Vec2::splat(f32::MAX), Vec2::splat(f32::MIN));
                for (q, _) in &pontos {
                    min = min.min(*q);
                    max = max.max(*q);
                }
                Caminho { pontos, min: min - Vec2::splat(LARGURA_MAX), max: max + Vec2::splat(LARGURA_MAX) }
            })
            .collect()
    })
}

/// The puffy half width of a path at `q`: the base plus slow bulges, so the
/// edge reads as cloud and not as a kerb.
fn meia_largura(q: Vec2) -> f32 {
    MEIA_PONTE + 1.6 * (q.x * 0.11 + 0.7).sin() * (q.y * 0.13 - 0.3).cos() + 0.9 * (q.x * 0.31 - q.y * 0.27).sin()
}

/// Distance to the nearest cloud path and the deck height there, in units.
fn ponte_em(q: Vec2) -> Option<(f32, f32)> {
    let mut melhor: Option<(f32, f32)> = None;
    for c in caminhos() {
        if q.x < c.min.x || q.y < c.min.y || q.x > c.max.x || q.y > c.max.y {
            continue;
        }
        for w in c.pontos.windows(2) {
            let ((a, ha), (b, hb)) = (w[0], w[1]);
            let ab = b - a;
            let t = ((q - a).dot(ab) / ab.length_squared()).clamp(0.0, 1.0);
            let d = (a + ab * t).distance(q);
            if melhor.is_none_or(|(m, _)| d < m) {
                melhor = Some((d, ha + (hb - ha) * t));
            }
        }
    }
    melhor
}

/// Do resource nodes (stone, ore, Energy) grow at `q`? Only on the small
/// islets — the resource islands; hunting islands and the town stay clear.
pub fn tem_recurso(q: Vec2) -> bool {
    plato_em(q).is_some_and(|(_, p)| p.nivel.0 > 0 && p.raio <= 36.0)
}

/// Is `q` on a cloud path (and not on an island)? Vegetation stays off, and
/// the ground there is cloud.
pub fn na_ponte(q: Vec2) -> bool {
    plato_em(q).is_none() && ponte_em(q).is_some_and(|(d, _)| d <= meia_largura(q))
}

/// Is `q` walkable ground (island or cloud path)?
pub fn e_chao(q: Vec2) -> bool {
    plato_em(q).is_some() || ponte_em(q).is_some_and(|(d, _)| d <= meia_largura(q))
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
    if p.nivel.0 == 0 || p.raio < 44.0 {
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
        let meia = meia_largura(q);
        if dist <= meia {
            // A soft swell across the deck, highest in the middle: a cloud,
            // not a plank. One block at most, so it never blocks a step.
            let inchaco = if dist < meia * 0.45 { 1 } else { 0 };
            return (h / BLOCO).round() as i32 - 1 + inchaco;
        }
    }
    NIVEL_FUNDO
}

/// Story place ids for Skyreach (`objective_kind::LUGAR`): `PONTO_BASE + i`
/// is the centre of island `i` of `PLATOS`. Past the Plateau's 40-44.
pub const PONTO_BASE: u16 = 60;

/// The world position of story place `p`, if it is one of Skyreach's.
pub fn ponto(p: u16) -> Option<Vec2> {
    p.checked_sub(PONTO_BASE).and_then(|i| PLATOS.get(i as usize)).map(|pl| pl.centro)
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
    bioma: crate::terreno::Bioma::Celeste,
    centro: [2600.0, -1400.0],
    nivel: (50, 60),
};

#[cfg(test)]
mod testes {
    use super::*;
    use crate::terreno::BLOCO;

    /// Every cloud path is walkable: from inside one island, along the
    /// curve, into the other, the ground never drops into the void and never
    /// changes by more than a block between neighbouring columns.
    #[test]
    fn toda_ponte_e_caminhavel() {
        for (k, c) in caminhos().iter().enumerate() {
            let (i, j) = PONTES[k];
            let mut rota = vec![PLATOS[i].centro.lerp(c.pontos[0].0, 0.85)];
            rota.extend(c.pontos.iter().map(|(q, _)| *q));
            rota.push(PLATOS[j].centro.lerp(c.pontos[AMOSTRAS].0, 0.85));
            let mut ant: Option<(i32, i32, i32)> = None;
            for w in rota.windows(2) {
                let passos = (w[0].distance(w[1]) / (BLOCO * 0.5)).ceil() as i32;
                for n in 0..=passos {
                    let q = w[0].lerp(w[1], n as f32 / passos as f32);
                    let (bx, bz) = ((q.x / BLOCO).round() as i32, (q.y / BLOCO).round() as i32);
                    let b = bloco_da_coluna(bx, bz);
                    assert!(b > 0, "path {i}-{j} drops into the void at {q}");
                    if let Some((x, z, h)) = ant {
                        if (x, z) != (bx, bz) {
                            assert!((b - h).abs() <= 1, "path {i}-{j} jumps {h} -> {b} at {q}");
                        }
                    }
                    ant = Some((bx, bz, b));
                }
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
