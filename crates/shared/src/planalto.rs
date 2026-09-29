//! Storm Plateau: shared terrain, roads and progression.
use crate::terreno::{Cidade, SitioPorto};
use glam::Vec2;
pub const ZONA: &str = "ilha_planalto";
pub const REVISAO: u32 = 1;
pub const NOMES: [&str; 5] = [
    "Encostas dos Sentinelas",
    "Mosteiro dos Ventos",
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
        Self {
            cidade,
            regioes,
            estradas,
        }
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
        NIVEIS[self.regiao(p)]
    }
    pub fn distancia_estrada(&self, p: Vec2) -> f32 {
        self.estradas
            .iter()
            .map(|e| e.amostra(p).0)
            .fold(f32::INFINITY, f32::min)
    }
    pub fn sem_obstaculo(&self, p: Vec2) -> bool {
        self.distancia_estrada(p) < ESTRADA + 5.0
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
}
