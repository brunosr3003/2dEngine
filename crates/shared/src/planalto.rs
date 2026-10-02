//! Storm Plateau: shared terrain, roads and progression.
use crate::terreno::{Cidade, SitioPorto};
use glam::Vec2;
pub const ZONA: &str = "ilha_planalto";
pub const REVISAO: u32 = 3;
pub const NOMES: [&str; 5] = [
    "Encostas dos Sentinelas",
    // Era "Monastery of the Winds", o MESMO nome da Gruta 13. No mapa o trecho
    // de trilha parecia uma dungeon — o dono pediu pra tirar. O trecho é um
    // lugar, e ganha nome de lugar, como os vizinhos.
    "Passo dos Ventos",
    "Vale do Trovão",
    "Forja Partida",
    "Olho da Tempestade",
];
pub const NIVEIS: [(u32, u32); 5] = [(40, 44), (44, 49), (48, 53), (52, 57), (57, 60)];
pub const PONTO_BASE: u16 = 40;
/// How far the region reaches: the end of the ramp.
pub const RAIO: f32 = 78.0;
/// The region's FLAT core. From there to `RAIO` the ground returns to the
/// relief by smoothstep, the way the city square does (`Cidade::RAIO_PLATO`/`RAIO`).
///
/// It was the whole `RAIO`: the weight `((RAIO + 40 - d)/40)` saturates at 1
/// within 78, so each region became a PERFECTLY flat disc 156 u across. Five
/// of them plus the city and the mountain disappeared — the 28/09/2026
/// captures show a car park, not a plateau. docs/MUNDO.md had already
/// measured that flat area was never the scarce resource; contrast was.
pub const RAIO_PLATO: f32 = 30.0;
pub const ESTRADA: f32 = 4.5;

// ── the castle of Last Refuge (`Plano::muralha`) ──
/// Inner face of the curtain wall, from the town's centre: just past the
/// square's flat core (`Cidade::RAIO_PLATO`, 28), clear of every house lot.
pub const MURO_RAIO: f32 = 31.0;
pub const MURO_ESPESSURA: f32 = 2.5;
/// Wall and tower heights over the ground, in blocks (5 u and 8 u).
pub const MURO_BLOCOS: i32 = 10;
pub const TORRE_BLOCOS: i32 = 16;
pub const TORRE_RAIO: f32 = 3.5;
/// A gate is the road plus this on each side: wide enough for the A*'s
/// 4-unit grid and for a crowd.
pub const PORTAO_FOLGA: f32 = 3.0;

// ── Stormkeep: the open-field castle in the Plateau's empty south ──
/// The owner, 02/10/2026: "the south of the planalto is kind of empty ... an
/// open map dungeon, a big castle with a lot of mobs and a bigger area for a
/// boss". Mobs 50-60, the Warlord 60 in the keep, and his chests.
pub const FORTE_NOME: &str = "Stormkeep";
pub const FORTE_NIVEIS: (u32, u32) = (50, 60);
/// How far south of the Passo-Forja line the keep stands, in units.
const FORTE_DESVIO: f32 = 240.0;
/// Flat core and ramp end of the castle's plateau.
pub const FORTE_RAIO_PLATO: f32 = 66.0;
pub const FORTE_RAIO: f32 = 100.0;
/// The outer curtain (inner face) and the keep wall round the boss arena.
pub const FORTE_MURO_RAIO: f32 = 56.0;
pub const FORTE_PATIO_RAIO: f32 = 24.0;
pub const FORTE_ESPESSURA: f32 = 3.0;
pub const FORTE_TORRE_RAIO: f32 = 4.5;
pub const FORTE_MURO_BLOCOS: i32 = 14;
pub const FORTE_TORRE_BLOCOS: i32 = 22;
/// The middle ring: between the curtain and the keep, its gate on the FAR
/// side from the other two — the owner asked for the castle to be "more
/// labyrinthine" (02/10/2026), so the way to the Warlord goes round twice.
pub const FORTE_MEIO_RAIO: f32 = 40.0;

/// The castle's corridors, as (inner face, outer face) radii: the outer one
/// between the middle ring and the curtain, the inner one between the keep
/// wall and the middle ring.
pub const FORTE_CORREDORES: [(f32, f32); 2] = [
    (FORTE_MEIO_RAIO + FORTE_ESPESSURA, FORTE_MURO_RAIO),
    (FORTE_PATIO_RAIO + FORTE_ESPESSURA, FORTE_MEIO_RAIO),
];

/// The radial walls that make the maze: (angle from the gate, from radius,
/// to radius). One per corridor seals it whole — the way round goes the
/// long side, past a dead end of hordes — and the others leave a 6 u gap at
/// alternating ends, so the path snakes.
pub const FORTE_DIVISORIAS: [(f32, f32, f32); 8] = [
    // outer corridor (43..56): sealed just past the gate, then a zigzag
    (-0.32, 43.0, 56.0),
    (0.90, 43.0, 50.0),
    (1.55, 49.0, 56.0),
    (2.25, 43.0, 50.0),
    // inner corridor (27..40): from the middle gate (pi) down to the keep's (0)
    (std::f32::consts::PI + 0.40, 27.0, 40.0),
    (2.30, 27.0, 34.0),
    (1.55, 33.0, 40.0),
    (0.80, 27.0, 34.0),
];

/// The castle of the south: its centre, floor and towers.
#[derive(Clone, Debug)]
pub struct Forte {
    pub centro: Vec2,
    pub nivel_chao: f32,
    /// Unit vector from the centre towards the gate (the road in).
    pub portao: Vec2,
    pub torres: Vec<Vec2>,
}

/// A piece of the castle.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Muro {
    Cortina { ameia: bool },
    Torre { ameia: bool },
}
pub const PERIODO: i64 = 1800;
pub const DURACAO: i64 = 600;
pub const ZONA_EVENTO: u32 = 19_000;
#[derive(Clone, Debug)]
pub struct Regiao {
    pub centro: Vec2,
    pub nivel_chao: f32,
}
#[derive(Clone, Debug)]
pub struct Estrada {
    pub a: Vec2,
    pub b: Vec2,
    pub ha: f32,
    pub hb: f32,
}
#[derive(Clone, Debug)]
pub struct Plano {
    pub cidade: Cidade,
    pub regioes: [Regiao; 5],
    pub estradas: Vec<Estrada>,
    /// The castle's towers, found once (`achar_torres`): the search walks the
    /// whole ring, and the relief asks per column.
    pub torres: Vec<Vec2>,
    /// Stormkeep, the castle of the south.
    pub forte: Forte,
}
impl Plano {
    pub fn novo(cidade: Cidade, porto: Option<SitioPorto>) -> Self {
        let c = cidade.centro();
        let frente = (-c).try_normalize().unwrap_or(Vec2::NEG_X);
        let lado = Vec2::new(-frente.y, frente.x);
        let fim = frente * 460.0;
        let regioes = std::array::from_fn(|i| {
            let t = (i + 1) as f32 / 5.0;
            Regiao {
                centro: c.lerp(fim, t) + lado * [65., -90., 85., -75., 0.][i],
                nivel_chao: cidade.nivel as f32 + [8., 20., 32., 44., 56.][i],
            }
        });
        let mut estradas = Vec::new();
        let mut a = c;
        let mut ha = cidade.nivel as f32;
        for r in &regioes {
            estradas.push(Estrada {
                a,
                b: r.centro,
                ha,
                hb: r.nivel_chao,
            });
            a = r.centro;
            ha = r.nivel_chao;
        }
        if let Some(p) = porto {
            estradas.push(Estrada {
                a: p.centro,
                b: c,
                ha: p.nivel as f32,
                hb: cidade.nivel as f32,
            });
        }
        estradas.push(Estrada {
            a: regioes[1].centro,
            b: regioes[3].centro,
            ha: regioes[1].nivel_chao,
            hb: regioes[3].nivel_chao,
        });
        // STORMKEEP: south of the Passo-Forja line, on its own plateau. The
        // road climbs from the Passo to the foot of the ramp at the castle's
        // floor level, then runs flat through both gates to the keep.
        let centro_forte = (regioes[1].centro + regioes[3].centro) * 0.5 - lado * FORTE_DESVIO;
        let nivel_forte = regioes[1].nivel_chao + 10.0;
        let portao = (regioes[1].centro - centro_forte).try_normalize().unwrap_or(Vec2::NEG_Y);
        let pe = centro_forte + portao * FORTE_RAIO;
        estradas.push(Estrada { a: regioes[1].centro, b: pe, ha: regioes[1].nivel_chao, hb: nivel_forte });
        // Only to the curtain's gate: inside is the maze, not a road.
        let portao_ext = centro_forte + portao * (FORTE_MURO_RAIO - 1.0);
        estradas.push(Estrada { a: pe, b: portao_ext, ha: nivel_forte, hb: nivel_forte });
        let mut plano = Self {
            cidade,
            regioes,
            estradas,
            torres: Vec::new(),
            forte: Forte { centro: centro_forte, nivel_chao: nivel_forte, portao, torres: Vec::new() },
        };
        plano.torres = plano.achar_torres();
        plano.forte.torres = plano.achar_torres_do_forte();
        plano
    }

    /// Distance from Stormkeep's centre.
    pub fn d_forte(&self, p: Vec2) -> f32 {
        self.forte.centro.distance(p)
    }

    /// Inside Stormkeep's outer wall (or on it)?
    pub fn no_forte(&self, p: Vec2) -> bool {
        self.d_forte(p) < FORTE_MURO_RAIO + FORTE_ESPESSURA + FORTE_TORRE_RAIO
    }

    /// The angle of `p` round Stormkeep, measured from the gate (0..TAU).
    fn angulo_no_forte(&self, p: Vec2) -> f32 {
        let f = &self.forte;
        let d = p - f.centro;
        (d.y.atan2(d.x) - f.portao.y.atan2(f.portao.x)).rem_euclid(std::f32::consts::TAU)
    }

    /// The rings, each with its gate's angle from the main gate: curtain at
    /// 0 (the road), middle at pi (the far side), keep at 0 again.
    fn aneis_do_forte() -> [(f32, f32); 3] {
        [(FORTE_MURO_RAIO, 0.0), (FORTE_MEIO_RAIO, std::f32::consts::PI), (FORTE_PATIO_RAIO, 0.0)]
    }

    /// Half the angular width of a gate in a ring of `raio`.
    fn meia_boca(raio: f32) -> f32 {
        (ESTRADA + PORTAO_FOLGA) / (raio + FORTE_ESPESSURA * 0.5)
    }

    /// The centres of the garrison's hordes: along the middle of both
    /// corridors, off the radial walls.
    pub fn hordas_do_forte(&self) -> Vec<Vec2> {
        let f = &self.forte;
        let base = f.portao.y.atan2(f.portao.x);
        let mut v = Vec::new();
        for (k, (a, b)) in FORTE_CORREDORES.iter().enumerate() {
            let r = (a + b) * 0.5;
            let n = if k == 0 { 9 } else { 6 };
            for i in 0..n {
                let ang = (i as f32 + 0.5) / n as f32 * std::f32::consts::TAU;
                let longe = FORTE_DIVISORIAS.iter().all(|(d, _, _)| {
                    let da = (ang - d).rem_euclid(std::f32::consts::TAU);
                    da.min(std::f32::consts::TAU - da) * r > 6.0
                });
                if longe {
                    v.push(f.centro + Vec2::new((base + ang).cos(), (base + ang).sin()) * r);
                }
            }
        }
        v
    }

    /// Stormkeep's towers: a pair at every gate, one every 30 degrees round
    /// the curtain and every 60 round the keep, and one at the foot of each
    /// radial wall.
    fn achar_torres_do_forte(&self) -> Vec<Vec2> {
        let f = &self.forte;
        let base = f.portao.y.atan2(f.portao.x);
        let polar = |a: f32, r: f32| f.centro + Vec2::new((base + a).cos(), (base + a).sin()) * r;
        let mut torres = Vec::new();
        for (raio, g) in Self::aneis_do_forte() {
            let meio = raio + FORTE_ESPESSURA * 0.5;
            let boca = Self::meia_boca(raio) + FORTE_TORRE_RAIO / meio;
            torres.push(polar(g - boca, meio));
            torres.push(polar(g + boca, meio));
            let passos = if raio == FORTE_MURO_RAIO { 12 } else if raio == FORTE_PATIO_RAIO { 6 } else { 0 };
            for k in 0..passos {
                let a = (k as f32 + 0.5) / passos as f32 * std::f32::consts::TAU;
                let da = (a - g).rem_euclid(std::f32::consts::TAU);
                if da.min(std::f32::consts::TAU - da) > boca * 2.0 {
                    torres.push(polar(a, meio));
                }
            }
        }
        torres
    }

    /// Which part of Stormkeep stands at `p`, if any.
    pub fn parte_do_forte(&self, p: Vec2) -> Option<Muro> {
        let d = self.d_forte(p);
        if d > FORTE_MURO_RAIO + FORTE_ESPESSURA + FORTE_TORRE_RAIO + 1.0 {
            return None;
        }
        let (bx, bz) = (
            (p.x / crate::terreno::BLOCO).round() as i32,
            (p.y / crate::terreno::BLOCO).round() as i32,
        );
        let xadrez = (bx.div_euclid(2) + bz.div_euclid(2)) % 2 == 0;
        for t in &self.forte.torres {
            let dt = t.distance(p);
            if dt <= FORTE_TORRE_RAIO {
                return Some(Muro::Torre { ameia: dt > FORTE_TORRE_RAIO - 1.0 && xadrez });
            }
        }
        let ang = self.angulo_no_forte(p);
        for (raio, g) in Self::aneis_do_forte() {
            if (raio..=raio + FORTE_ESPESSURA).contains(&d) {
                let da = (ang - g).rem_euclid(std::f32::consts::TAU);
                if da.min(std::f32::consts::TAU - da) <= Self::meia_boca(raio) {
                    return None; // the gate
                }
                let meio = raio + FORTE_ESPESSURA * 0.5;
                return Some(Muro::Cortina { ameia: d > meio + FORTE_ESPESSURA * 0.25 && xadrez });
            }
        }
        // The maze's radial walls.
        for (a, r0, r1) in FORTE_DIVISORIAS {
            let da = ang - a;
            if (r0..=r1).contains(&d) && da.cos() > 0.0 && (d * da.sin()).abs() <= FORTE_ESPESSURA * 0.5 {
                return Some(Muro::Cortina { ameia: xadrez });
            }
        }
        None
    }
    pub fn regiao(&self, p: Vec2) -> usize {
        self.regioes
            .iter()
            .enumerate()
            .min_by(|(_, a), (_, b)| {
                a.centro
                    .distance_squared(p)
                    .total_cmp(&b.centro.distance_squared(p))
            })
            .unwrap()
            .0
    }
    pub fn faixa(&self, p: Vec2) -> (u32, u32) {
        if self.no_forte(p) {
            return FORTE_NIVEIS;
        }
        NIVEIS[self.regiao(p)]
    }
    pub fn distancia_estrada(&self, p: Vec2) -> f32 {
        self.estradas
            .iter()
            .map(|e| e.amostra(p).0)
            .fold(f32::INFINITY, f32::min)
    }
    pub fn sem_obstaculo(&self, p: Vec2) -> bool {
        // Stormkeep's grounds are cleared: no trees or rocks in a castle.
        self.d_forte(p) < FORTE_MURO_RAIO + FORTE_ESPESSURA + 8.0
            || self.distancia_estrada(p) < ESTRADA + 5.0
            || self.regioes.iter().any(|r| {
                r.centro.distance(p) < 14.0
                    || (r.centro + Vec2::new(22., -22.)).distance(p) < 10.0
                    || (r.centro + Vec2::new(-10., -12.)).distance(p) < 6.0
            })
    }
    pub fn sem_spawn(&self, p: Vec2) -> bool {
        self.sem_obstaculo(p) || self.distancia_estrada(p) < 24.0
    }
    /// Smoothstep: the same curve `aplainar_sitio` uses to step the plateau down.
    fn suave(t: f32) -> f32 {
        t * t * (3.0 - 2.0 * t)
    }

    pub fn bloco(&self, p: Vec2, cru: i32) -> i32 {
        // The CITY does not come in here. `Cidade::aplainar` already flattens the
        // square in `bloco_em`, AFTER this drawing, with its own plateau and ramp.
        // Having a second calculation for the same question is what made Last
        // Refuge come out as a 140 u disc — and it is the defect the README calls
        //  "one truth per question".
        // The TERRACE steps away from the city — the ROAD does not.
        //
        // `Cidade::aplainar` runs AFTER this and is what makes the square;
        // terracing underneath it pushed the ground away from the requested level
        // and the flattening gave up (it refuses above `Cidade::MORRO`). But the
        // road has to keep counting all the way to the square: suppressing both
        // left a step in the middle of the harbour path, and the roads test caught
        // it immediately.
        let d_cidade = self.cidade.centro().distance(p);
        let fora = Self::suave(((d_cidade - (Cidade::RAIO + 6.0)) / 34.0).clamp(0.0, 1.0));
        let mut h = cru as f32;
        // Only the NEAREST region rules. Adding the five in sequence let the last
        // in the list overwrite the others where the discs touch, and they do touch:
        // the centers are ~100 u from each other.
        if let Some(r) = self.regioes.iter().min_by(|a, b| {
            a.centro
                .distance_squared(p)
                .total_cmp(&b.centro.distance_squared(p))
        }) {
            let d = r.centro.distance(p);
            let t = (1.0 - Self::suave(((d - RAIO_PLATO) / (RAIO - RAIO_PLATO)).clamp(0.0, 1.0)))
                * fora;
            h += (r.nivel_chao - h) * t;
        }
        // Stormkeep's plateau, over whatever region is nearest.
        let d = self.d_forte(p);
        if d < FORTE_RAIO {
            let t = 1.0 - Self::suave(((d - FORTE_RAIO_PLATO) / (FORTE_RAIO - FORTE_RAIO_PLATO)).clamp(0.0, 1.0));
            h += (self.forte.nivel_chao - h) * t;
        }
        if let Some((d, alvo)) = self
            .estradas
            .iter()
            .map(|e| e.amostra(p))
            .min_by(|a, b| a.0.total_cmp(&b.0))
        {
            let peso = ((ESTRADA + 18.0 - d) / 18.0).clamp(0.0, 1.0);
            h += (alvo - h) * peso;
        }
        h.round() as i32
    }
    /// THE CASTLE of Last Refuge: stone walls round the town, towers, and a
    /// gatehouse on every road. The owner, 30/09/2026: "about the lvl 40+ map,
    /// i want it to be a castle instead of just a normal town".
    ///
    /// Terrain, like the Porão cellars: a wall is columns raised above the
    /// ground, so it blocks by the step rule on server and client alike, the
    /// A* goes round it, and the gates are simply where no wall is — every
    /// road out of town keeps its opening (`distancia_estrada`).
    ///
    /// Runs AFTER the town square is flattened (`Gerador::bloco_em`), so the
    /// wall stands on the finished ground.
    pub fn muralha(&self, p: Vec2, b: i32) -> i32 {
        match self.parte_da_cidade(p) {
            Some(Muro::Cortina { ameia }) => b + MURO_BLOCOS + if ameia { 2 } else { 0 },
            Some(Muro::Torre { ameia }) => b + TORRE_BLOCOS + if ameia { 2 } else { 0 },
            None => match self.parte_do_forte(p) {
                Some(Muro::Cortina { ameia }) => b + FORTE_MURO_BLOCOS + if ameia { 2 } else { 0 },
                Some(Muro::Torre { ameia }) => b + FORTE_TORRE_BLOCOS + if ameia { 2 } else { 0 },
                None => b,
            },
        }
    }

    /// Which part of either castle stands at `p`, if any.
    pub fn parte_da_muralha(&self, p: Vec2) -> Option<Muro> {
        self.parte_da_cidade(p).or_else(|| self.parte_do_forte(p))
    }

    /// Which part of Last Refuge's castle stands at `p`, if any.
    fn parte_da_cidade(&self, p: Vec2) -> Option<Muro> {
        let c = self.cidade.centro();
        let d = c.distance(p);
        if !(MURO_RAIO - TORRE_RAIO - 1.0..=MURO_RAIO + MURO_ESPESSURA + TORRE_RAIO + 1.0).contains(&d) {
            return None;
        }
        let (bx, bz) = (
            (p.x / crate::terreno::BLOCO).round() as i32,
            (p.y / crate::terreno::BLOCO).round() as i32,
        );
        let xadrez = (bx.div_euclid(2) + bz.div_euclid(2)) % 2 == 0;
        // Towers: flanking every gate, and every 45 degrees round the ring
        // where no gate is.
        let meio = MURO_RAIO + MURO_ESPESSURA * 0.5;
        for t in &self.torres {
            let dt = t.distance(p);
            if dt <= TORRE_RAIO {
                return Some(Muro::Torre { ameia: dt > TORRE_RAIO - 1.0 && xadrez });
            }
        }
        // The gates: no wall on a road.
        if self.distancia_estrada(p) < ESTRADA + PORTAO_FOLGA {
            return None;
        }
        (MURO_RAIO..=MURO_RAIO + MURO_ESPESSURA)
            .contains(&d)
            .then_some(Muro::Cortina {
                ameia: d > meio + MURO_ESPESSURA * 0.25 && xadrez,
            })
    }

    /// Where the towers stand: a pair on each gate, and one every 45 degrees
    /// round the ring away from the gates.
    fn achar_torres(&self) -> Vec<Vec2> {
        let c = self.cidade.centro();
        let meio = MURO_RAIO + MURO_ESPESSURA * 0.5;
        let no_anel = |a: f32| c + Vec2::new(a.cos(), a.sin()) * meio;
        let mut torres = Vec::new();
        // The gates: where each road out of town crosses the ring.
        let mut portoes: Vec<f32> = Vec::new();
        for k in 0..720 {
            let a = k as f32 / 720.0 * std::f32::consts::TAU;
            let q = no_anel(a);
            let na_estrada = self.distancia_estrada(q) < ESTRADA;
            let antes = self.distancia_estrada(no_anel(a - std::f32::consts::TAU / 720.0)) < ESTRADA;
            if na_estrada && !antes {
                // Walk to the middle of this crossing.
                let mut fim = a;
                while self.distancia_estrada(no_anel(fim)) < ESTRADA && fim < a + 1.0 {
                    fim += std::f32::consts::TAU / 720.0;
                }
                portoes.push((a + fim) * 0.5);
            }
        }
        let meia_boca = (ESTRADA + PORTAO_FOLGA + TORRE_RAIO) / meio;
        for &g in &portoes {
            torres.push(no_anel(g - meia_boca));
            torres.push(no_anel(g + meia_boca));
        }
        for k in 0..8 {
            let a = k as f32 * std::f32::consts::FRAC_PI_4 + 0.2;
            let longe_do_portao = portoes.iter().all(|g| {
                let da = (a - g).rem_euclid(std::f32::consts::TAU);
                da.min(std::f32::consts::TAU - da) > meia_boca * 2.2
            });
            if longe_do_portao {
                torres.push(no_anel(a));
            }
        }
        torres
    }

    pub fn centro_campo(&self, i: usize) -> Vec2 {
        self.regioes[i].centro + Vec2::new(35., 30.)
    }
    pub fn campo(&self, unix: i64) -> Option<(usize, Vec2)> {
        (unix.rem_euclid(PERIODO) < DURACAO).then(|| {
            let i = if unix.div_euclid(PERIODO).rem_euclid(2) == 0 {
                2
            } else {
                3
            };
            (i, self.centro_campo(i))
        })
    }
    pub fn bonus_coleta(&self, p: Vec2, unix: i64) -> bool {
        self.campo(unix).is_some_and(|(_, c)| p.distance(c) <= 32.0)
    }
}
impl Estrada {
    pub fn amostra(&self, p: Vec2) -> (f32, f32) {
        let d = self.b - self.a;
        let t = ((p - self.a).dot(d) / d.length_squared().max(1.0)).clamp(0.0, 1.0);
        (
            p.distance(self.a + d * t),
            self.ha + (self.hb - self.ha) * t,
        )
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::terreno::{Gerador, ARQUIPELAGO, BLOCO};
    #[test]
    fn planalto_estradas_e_terracos_caminhaveis() {
        let g = Gerador::da_ilha(&ARQUIPELAGO[3]);
        let p = g.planalto().unwrap();
        for e in &p.estradas {
            let passos = (e.a.distance(e.b) / BLOCO).ceil() as usize;
            let mut anterior: Option<i32> = None;
            for i in 0..=passos {
                let q = e.a.lerp(e.b, i as f32 / passos as f32);
                let h = g.bloco_em((q.x / BLOCO).round() as i32, (q.y / BLOCO).round() as i32);
                assert!(
                    h as f32 * BLOCO > crate::terreno::NIVEL_DO_MAR,
                    "estrada submersa {q:?}"
                );
                if let Some(a) = anterior {
                    assert!((h - a).abs() <= 1, "degrau na estrada {q:?}: {a} -> {h}");
                }
                anterior = Some(h);
            }
        }
        for (i, r) in p.regioes.iter().enumerate() {
            assert_eq!(p.faixa(r.centro), NIVEIS[i]);
            assert!(r.centro.length() + RAIO < ARQUIPELAGO[3].raio_m());
        }
    }
    #[test]
    fn planalto_tempestade_tem_limites_e_alterna() {
        let p = Plano::novo(Cidade::nova(400., 0., 30), None);
        let c = p.campo(0).unwrap().1;
        assert!(p.bonus_coleta(c, 599));
        assert!(!p.bonus_coleta(c, 600));
        assert!(!p.bonus_coleta(c + Vec2::splat(100.), 0));
        assert_eq!(p.campo(1800).unwrap().0, 3);
        assert!(p.campo(1799).is_none());
    }

    /// STORMKEEP: a castle on its own plateau in the south, walls standing
    /// round both rings, and a walk from the Passo through both gates into
    /// the boss arena. Its hordes stand in the outer courtyard, off the walls.
    #[test]
    fn stormkeep_tem_muralha_portoes_e_arena_alcancavel() {
        let def = crate::terreno::def_da_zona(ZONA).expect("the Planalto exists");
        let ger = crate::terreno::Gerador::da_ilha(def);
        let pl = ger.planalto().expect("the Planalto has its plan");
        let f = &pl.forte;
        assert!(f.centro.length() + FORTE_RAIO < def.raio_m(), "Stormkeep off the island: {:?}", f.centro);
        for raio in [FORTE_MURO_RAIO, FORTE_MEIO_RAIO, FORTE_PATIO_RAIO] {
            let meio = raio + FORTE_ESPESSURA * 0.5;
            let mut de_pe = 0;
            for k in 0..360 {
                let a = k as f32 / 360.0 * std::f32::consts::TAU;
                let q = f.centro + Vec2::new(a.cos(), a.sin()) * meio;
                if ger.altura(q.x, q.y) >= f.nivel_chao * BLOCO + 5.0 {
                    de_pe += 1;
                }
            }
            assert!(de_pe > 300, "ring {raio}: only {de_pe}/360 of it has a wall");
        }
        for h in pl.hordas_do_forte() {
            assert!(pl.parte_da_muralha(h).is_none(), "a horde on the wall at {h:?}");
            assert_eq!(pl.faixa(h), FORTE_NIVEIS);
        }
        assert!(pl.hordas_do_forte().len() >= 8);
        let ilha = crate::terreno::Ilha::da_ilha(def);
        // From just outside the curtain's gate to the Warlord: through the
        // maze, and it WINDS — the owner asked for a labyrinth.
        let porta = f.centro + f.portao * (FORTE_MURO_RAIO + 8.0);
        let rota = ilha.caminho(porta, f.centro, 400_000).expect("no walk through Stormkeep's maze");
        assert!(rota.last().is_some_and(|p| p.distance(f.centro) < 8.0), "the maze does not reach the arena");
        let mut comprimento = porta.distance(rota[0]);
        for w in rota.windows(2) {
            comprimento += w[0].distance(w[1]);
        }
        assert!(comprimento > 4.0 * (FORTE_MURO_RAIO + 8.0), "the way in is only {comprimento:.0} u: not a maze");
        let rota = ilha.caminho(pl.regioes[1].centro, porta, 80_000);
        assert!(rota.is_some(), "no road from the Passo to Stormkeep's gate");
    }

    /// LAST REFUGE IS A CASTLE: a wall round most of the town, towers on it,
    /// and still a way out on every road — to the port and up the trail, by
    /// the server's own route from the town square.
    #[test]
    fn o_ultimo_abrigo_e_um_castelo_com_saida_em_toda_estrada() {
        let def = crate::terreno::def_da_zona(ZONA).expect("the Planalto exists");
        let ger = crate::terreno::Gerador::da_ilha(def);
        let pl = ger.planalto().expect("the Planalto has its plan");
        let c = pl.cidade.centro();
        // The wall stands round most of the ring.
        let meio = MURO_RAIO + MURO_ESPESSURA * 0.5;
        let mut de_pe = 0;
        for k in 0..360 {
            let a = k as f32 / 360.0 * std::f32::consts::TAU;
            let q = c + Vec2::new(a.cos(), a.sin()) * meio;
            let chao = ger.altura(q.x + a.cos() * -6.0, q.y + a.sin() * -6.0);
            if ger.altura(q.x, q.y) >= chao + 4.0 {
                de_pe += 1;
            }
        }
        assert!(de_pe > 270, "only {de_pe}/360 of the ring has a wall");
        assert!(pl.torres.len() >= 8, "only {} towers", pl.torres.len());
        // And a way out on every road.
        let ilha = crate::terreno::Ilha::da_ilha(def);
        let praca = c + Vec2::new(0.0, 4.0);
        let mut destinos = vec![("the trail", pl.regioes[0].centro)];
        if let Some(p) = ger.porto() {
            destinos.push(("the port", p.centro));
        }
        for (nome, alvo) in destinos {
            let rota = ilha.caminho(praca, alvo, 40_000);
            assert!(
                rota.as_ref().and_then(|r| r.last()).is_some_and(|f| f.distance(alvo) < 8.0),
                "no walk from the square to {nome}"
            );
        }
    }
}
