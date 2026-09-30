//! THE OASIS of the Ermo: a pond, a green shore and a grove of T3 trees.
//!
//! The owner, 30/09/2026: "there is not much wood t3 in desert, which is
//! correct by the biome, but i want you to crate a oasis whit water and a more
//! wood and tress t3". The desert keeps its sparse trees (`densidade_de_arvore`
//! is 0.05 there on purpose); the oasis is the ONE place on the island where
//! wood grows thick — a landmark worth walking to, not a forest everywhere.
//!
//! The wood tier needs nothing here: an Ermo tree already drops T3
//! (`porao::tier_da_arvore` reads the island's level band).
//!
//! Everything is a pure function of the generator, like the city: the server's
//! heightfield, the client's voxels and the tree placement all come out of the
//! same `Gerador`, so the pond the player sees is the pond that stops them.

use crate::terreno::{suave, BLOCO};
use glam::Vec2;

/// The island that has the oasis.
pub const ZONA: &str = "ilha_deserto";

/// Water out to here, in world units.
pub const RAIO_AGUA: f32 = 9.0;
/// A flat, low shore ring out to here: where the grass and trees start.
pub const RAIO_MARGEM: f32 = 13.0;
/// The ground eases back to the dunes by here.
pub const RAIO_TOTAL: f32 = 32.0;
/// Grass out to here. Short of `RAIO_TOTAL` so the green fades into sand
/// on the slope, instead of stopping at a line on the flat.
pub const RAIO_VERDE: f32 = 25.0;
/// Trees per 100 m² in the grove: twice a Bosque forest. It's one small ring,
/// and it has to read as "the wood is HERE" from across the dunes.
pub const DENSIDADE_DO_BOSQUE: f32 = 2.4;

/// The shore's top block: one block above the sea, low enough to look like
/// the water's edge, high enough to be dry land.
const NIVEL_MARGEM: i32 = 1;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Oasis {
    /// Centre of the pond, in world units.
    pub centro: Vec2,
}

impl Oasis {
    fn distancia(&self, bx: i32, bz: i32) -> f32 {
        Vec2::new(bx as f32 * BLOCO, bz as f32 * BLOCO).distance(self.centro)
    }

    /// The top block of column `(bx, bz)` with the oasis carved in. Only ever
    /// LOWERS the ground: the pond is dug, never built up.
    pub fn moldar(&self, bx: i32, bz: i32, b: i32) -> i32 {
        let d = self.distancia(bx, bz);
        if d >= RAIO_TOTAL {
            return b;
        }
        let alvo = if d < RAIO_AGUA {
            // A bowl: one block under the surface at the rim, three in the
            // middle. `(b + 1) * BLOCO <= NIVEL_DO_MAR` is water.
            -1 - (2.0 * (1.0 - d / RAIO_AGUA)).round() as i32
        } else if d < RAIO_MARGEM {
            NIVEL_MARGEM
        } else {
            let t = suave((d - RAIO_MARGEM) / (RAIO_TOTAL - RAIO_MARGEM));
            (NIVEL_MARGEM as f32 + (b - NIVEL_MARGEM) as f32 * t).round() as i32
        };
        alvo.min(b)
    }

    /// Where the green ends in this direction. Not a circle: a ring of grass
    /// cut with a compass reads as a painted decal, not as a place where
    /// water feeds the ground.
    fn fim_do_verde(&self, bx: i32, bz: i32) -> f32 {
        let v = Vec2::new(bx as f32 * BLOCO, bz as f32 * BLOCO) - self.centro;
        let a = v.y.atan2(v.x);
        RAIO_VERDE + 2.6 * (3.0 * a + 0.7).sin() + 1.4 * (7.0 * a + 2.1).sin()
    }

    /// In the green ring (shore and grove), out of the water?
    pub fn no_verde(&self, bx: i32, bz: i32) -> bool {
        let d = self.distancia(bx, bz);
        d >= RAIO_AGUA + 1.0 && d < self.fim_do_verde(bx, bz)
    }

    /// The wet sand right at the water's edge.
    pub fn na_beira(&self, bx: i32, bz: i32) -> bool {
        let d = self.distancia(bx, bz);
        (RAIO_AGUA..RAIO_AGUA + 1.0).contains(&d)
    }

    /// Where the grove grows: past the wet edge, out to the end of the green.
    pub fn no_bosque(&self, bx: i32, bz: i32) -> bool {
        let d = self.distancia(bx, bz);
        d >= RAIO_AGUA + 1.5 && d < self.fim_do_verde(bx, bz) - 0.5
    }
}

#[cfg(test)]
mod testes {
    use super::*;
    use crate::terreno::{self, Gerador, Ilha};

    fn ermo() -> &'static terreno::DefIlha {
        terreno::def_da_zona(ZONA).expect("the Ermo exists")
    }

    /// THE ERMO HAS AN OASIS, AND ONLY THE ERMO.
    #[test]
    fn so_o_ermo_tem_oasis() {
        for def in terreno::ARQUIPELAGO.iter() {
            let tem = Gerador::da_ilha(def).oasis().is_some();
            assert_eq!(tem, def.zona == ZONA, "{}: oasis = {tem}", def.zona);
        }
    }

    /// THE POND IS WATER, THE SHORE IS DRY LAND YOU CAN WALK ON, AND THE POND
    /// IS A POND — not a bay the sea reaches.
    #[test]
    fn a_lagoa_e_agua_e_a_margem_e_chao() {
        let def = ermo();
        let ilha = Ilha::da_ilha(def);
        let o = Gerador::da_ilha(def).oasis().expect("the Ermo has an oasis");
        assert!(ilha.agua(o.centro.x, o.centro.y), "the middle of the pond is dry");
        let mut secos = 0;
        for k in 0..16 {
            let a = k as f32 / 16.0 * std::f32::consts::TAU;
            let p = o.centro + Vec2::new(a.cos(), a.sin()) * (RAIO_MARGEM - 1.0);
            if !ilha.agua(p.x, p.y) {
                secos += 1;
            }
        }
        assert_eq!(secos, 16, "the shore is under water in {} places", 16 - secos);
        assert!(!ilha.mar_aberto(o.centro), "the pond is connected to the ocean");
        // Away from the city and the port: a place to walk to.
        let cidade = ilha.cidade().expect("the Ermo has a city").centro();
        assert!(o.centro.distance(cidade) >= 150.0, "the oasis is in town");
        if let Some(p) = ilha.porto() {
            assert!(o.centro.distance(p.centro) >= 150.0, "the oasis is at the port");
        }
    }

    /// THE GROVE IS THICK, AND ITS WOOD IS T3.
    ///
    /// The number that matters: trees in the oasis against trees in the SAME
    /// area of plain desert. It has to be many times more, or the oasis is
    /// scenery and the complaint ("not much wood t3") stands.
    #[test]
    fn o_bosque_e_denso_e_da_madeira_t3() {
        let def = ermo();
        let ilha = Ilha::da_ilha(def);
        let o = Gerador::da_ilha(def).oasis().unwrap();
        // Trunks are the gatherable tier-0 bodies (`Coletavel::tier`).
        let conta = |centro: Vec2| {
            let mut v = Vec::new();
            ilha.coletaveis_em(centro, RAIO_VERDE, &mut v);
            v.iter().filter(|c| c.tier == 0).count()
        };
        let no_oasis = conta(o.centro);
        let fora = conta(o.centro + Vec2::new(90.0, 0.0));
        assert!(no_oasis >= 25, "only {no_oasis} trees in the oasis");
        assert!(
            no_oasis >= fora * 8 + 10,
            "the oasis has {no_oasis} trees, plain desert nearby {fora}"
        );
        assert_eq!(
            crate::porao::tier_da_arvore(def, None, o.centro),
            3,
            "an oasis tree doesn't drop T3 wood"
        );
    }
}
