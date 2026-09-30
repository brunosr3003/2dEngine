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

/// Where to walk when NO enemy is in sight, in a planned Porão
/// (`shared::planta`): the room of the step the run is on — the boss room at
/// the end.
///
/// The client only knows what is within `AOI_RADIUS` (24 u), and in the
/// bigger cellars the next room's horde spawns farther than that when its
/// gate opens. "Walk to the nearest enemy" then had no enemy to walk to, and
/// auto stood in the cleared room forever — "the auto quest in dungeun isnt
/// work very well". The room is always known: it comes from the plan and the
/// step the server sends.
pub fn sala_da_vez(conteudo: u16, andar: u8) -> Option<Vec2> {
    let p = shared::planta::da(conteudo)?;
    let c = p.centro(p.sala_da_etapa(andar)?);
    Some(Vec2::new(c.x, c.y))
}

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

    /// WITH NOTHING IN SIGHT, AUTO KNOWS WHERE THE NEXT FIGHT IS: every step
    /// of every Porão has a room to walk to, and the last step is the boss's.
    #[test]
    fn sem_inimigo_a_vista_vai_para_a_sala_da_vez() {
        for p in &shared::planta::PLANTAS {
            for andar in 0..=p.lutas() {
                let alvo = sala_da_vez(p.conteudo, andar)
                    .unwrap_or_else(|| panic!("plan {} step {andar}: nowhere to go", p.conteudo));
                // Reachable once that step's gate is open.
                let e = p.centro(p.entrada());
                assert!(
                    p.caminho(e, ::glam::Vec2::new(alvo.x, alvo.y), andar).is_some(),
                    "plan {} step {andar}: the room isn't reachable",
                    p.conteudo
                );
            }
        }
        assert!(sala_da_vez(999, 0).is_none(), "a non-Porão has no plan");
    }

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
