//! AUTO DUNGEON: a "missao" de completar a dungeon, ligada pela linha abaixo
//! da faixa da instancia.
//!
//! O que um jogador faria, na ordem: caiu, revive assim que pode; ainda ha'
//! inimigo, luta (o auto combate de sempre, com a area andando junto, e anda
//! ate' o inimigo mais perto quando nenhum esta' no alcance dele); venceu, vai
//! ate' o bau e abre; aberto, sai. Os andares o servidor avanca sozinho quando
//! o andar esvazia.
//!
//! Aqui so' a DECISAO, pura e testada. Quem manda mensagem e' o `main`.

use macroquad::prelude::Vec2;

/// O que a instancia diz neste quadro.
#[derive(Debug, Clone, Copy, Default)]
pub struct Estado {
    pub caido: bool,
    /// A espera de reviver acabou.
    pub reviver_pronto: bool,
    pub vitoria: bool,
    pub bau_aberto: bool,
    /// Posicao do bau, se ja' apareceu.
    pub bau: Option<Vec2>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Passo {
    /// Esperando (caido sem poder reviver, bau ainda nao apareceu).
    Esperar,
    Reviver,
    Lutar,
    /// Anda ate' o bau (longe) ou toca nele (perto).
    IrAoBau {
        perto: bool,
    },
    Sair,
}

/// Distancia pra tocar no bau.
pub const PERTO_DO_BAU: f32 = 2.5;
/// Espera depois de abrir o bau, pra o resultado aparecer antes de sair.
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
