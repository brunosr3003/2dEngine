//! AUTO DUNGEON: the "quest" of completing the dungeon, switched on by the
//! line below the instance's band.
//!
//! What a player would do, in order: went down, revive as soon as possible;
//! there is still an enemy, fight (the usual auto combat, with the area moving
//! along, and walk to the nearest enemy when none is in range); won, go to the
//! chest and open it; opened, leave. The server advances the floors by itself
//! when a floor empties.
//!
//! Only the DECISION here, pure and tested. Sending messages is `main`'s job.

use macroquad::prelude::Vec2;

/// What the instance says this frame.
#[derive(Debug, Clone, Copy, Default)]
pub struct Estado {
    pub caido: bool,
    /// The wait to revive is over.
    pub reviver_pronto: bool,
    pub vitoria: bool,
    pub bau_aberto: bool,
    /// The chest's position, if it has appeared.
    pub bau: Option<Vec2>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Passo {
    /// Waiting (down and unable to revive, chest has not appeared yet).
    Esperar,
    Reviver,
    Lutar,
    /// Walks to the chest (far) or touches it (near).
    IrAoBau {
        perto: bool,
    },
    Sair,
}

/// Distance at which to touch the chest.
pub const PERTO_DO_BAU: f32 = 2.5;
/// Wait after opening the chest, so the result shows before leaving.
pub const SAIR_APOS_BAU_S: f64 = 4.0;

pub fn decide(e: Estado, eu: Vec2, bau_aberto_ha_s: Option<f64>) -> Passo {
    if e.caido {
        return if e.reviver_pronto {
            Passo::Reviver
        } else {
            Passo::Esperar
        };
    }
    if !e.vitoria {
        return Passo::Lutar;
    }
    if e.bau_aberto {
        return if bau_aberto_ha_s.is_some_and(|s| s >= SAIR_APOS_BAU_S) {
            Passo::Sair
        } else {
            Passo::Esperar
        };
    }
    match e.bau {
        Some(p) => Passo::IrAoBau {
            perto: p.distance(eu) <= PERTO_DO_BAU,
        },
        None => Passo::Esperar,
    }
}

#[cfg(test)]
mod testes {
    use super::*;
    use macroquad::prelude::vec2;

    #[test]
    fn a_ordem_de_um_jogador() {
        let eu = vec2(0.0, 0.0);
        let base = Estado::default();
        assert_eq!(decide(base, eu, None), Passo::Lutar);
        assert_eq!(
            decide(
                Estado {
                    caido: true,
                    ..base
                },
                eu,
                None
            ),
            Passo::Esperar
        );
        assert_eq!(
            decide(
                Estado {
                    caido: true,
                    reviver_pronto: true,
                    ..base
                },
                eu,
                None
            ),
            Passo::Reviver
        );
        let venceu = Estado {
            vitoria: true,
            ..base
        };
        assert_eq!(
            decide(venceu, eu, None),
            Passo::Esperar,
            "bau ainda nao apareceu"
        );
        assert_eq!(
            decide(
                Estado {
                    bau: Some(vec2(10.0, 0.0)),
                    ..venceu
                },
                eu,
                None
            ),
            Passo::IrAoBau { perto: false }
        );
        assert_eq!(
            decide(
                Estado {
                    bau: Some(vec2(1.0, 0.0)),
                    ..venceu
                },
                eu,
                None
            ),
            Passo::IrAoBau { perto: true }
        );
        let aberto = Estado {
            bau_aberto: true,
            ..venceu
        };
        assert_eq!(
            decide(aberto, eu, Some(1.0)),
            Passo::Esperar,
            "deixa ver o que saiu"
        );
        assert_eq!(decide(aberto, eu, Some(SAIR_APOS_BAU_S)), Passo::Sair);
    }
}
