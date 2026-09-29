//! Camera by TOUCH (iOS): one finger dragging in the world rotates, a pinch zooms.
//!
//! On PC the camera is the right/middle button drag and the wheel. On touch
//! there is neither, and macroquad simulates the LEFT button from the finger
//! — the press arrives the instant the finger lands, before it is possible to
//! know whether it will become a drag. So, with touch, the click in the world
//! comes out on RELEASE, and only if the finger did not drag.
//!
//! The core does not know macroquad: it receives the frame's touches and says
//! what to do. `main` reads `touches()` and applies.
use macroquad::prelude::Vec2;

use crate::toque::TOLERANCIA_PX;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Fase {
    Comecou,
    Segurando,
    Acabou,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ToqueNoQuadro {
    pub id: u64,
    pub fase: Fase,
    pub pos: Vec2,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Acao {
    Nada,
    /// Toque curto, parado, fora do HUD: o clique normal no mundo.
    Clique(Vec2),
    /// Arrasto de um dedo: quanto o dedo andou neste quadro, em pixels.
    Gira(Vec2),
    /// Pinch: how much the distance between the fingers changed, in pixels (+ = opened).
    Zoom(f32),
}

#[derive(Debug, Clone, Copy)]
struct Dedo {
    id: u64,
    inicio: Vec2,
    ultimo: Vec2,
    sobre_hud: bool,
    arrastando: bool,
}

#[derive(Debug, Default)]
pub struct GestoCamera {
    dedo: Option<Dedo>,
    pinca: Option<f32>,
    /// Went through two fingers: no click until everything is released.
    cancelado: bool,
}

impl GestoCamera {
    /// Some finger in play (with touch, the mouse's simulated press does not count).
    pub fn ativo(&self) -> bool {
        self.dedo.is_some() || self.pinca.is_some() || self.cancelado
    }

    /// One frame. `sobre_hud` says whether the FIRST finger's point is over a
    /// button/panel — it is only read on the frame it lands.
    pub fn quadro(&mut self, toques: &[ToqueNoQuadro], sobre_hud: bool) -> Acao {
        let mut vivos: Vec<&ToqueNoQuadro> =
            toques.iter().filter(|t| t.fase != Fase::Acabou).collect();
        vivos.sort_by_key(|t| t.id);

        if vivos.len() >= 2 {
            // The second finger came in: the first one's click dies here.
            self.dedo = None;
            self.cancelado = true;
            let d = vivos[0].pos.distance(vivos[1].pos);
            let acao = match self.pinca {
                Some(antes) if (d - antes).abs() > f32::EPSILON => Acao::Zoom(d - antes),
                _ => Acao::Nada,
            };
            self.pinca = Some(d);
            return acao;
        }
        self.pinca = None;

        if toques.is_empty() {
            self.dedo = None;
            self.cancelado = false;
            return Acao::Nada;
        }
        let mut todos: Vec<&ToqueNoQuadro> = toques.iter().collect();
        todos.sort_by_key(|t| t.id);
        let t = todos[0];

        if self.cancelado {
            // After a pinch, releasing what is left neither clicks nor rotates.
            if vivos.is_empty() {
                self.cancelado = false;
                self.dedo = None;
            }
            return Acao::Nada;
        }

        match self.dedo {
            None => {
                if t.fase == Fase::Acabou {
                    // Encostou e soltou no mesmo quadro: toque curto.
                    return if sobre_hud {
                        Acao::Nada
                    } else {
                        Acao::Clique(t.pos)
                    };
                }
                self.dedo = Some(Dedo {
                    id: t.id,
                    inicio: t.pos,
                    ultimo: t.pos,
                    sobre_hud,
                    arrastando: false,
                });
                Acao::Nada
            }
            Some(mut d) => {
                if d.id != t.id {
                    // Another finger (the first left with no visible Acabou): starts over.
                    self.dedo = Some(Dedo {
                        id: t.id,
                        inicio: t.pos,
                        ultimo: t.pos,
                        sobre_hud,
                        arrastando: false,
                    });
                    return Acao::Nada;
                }
                if !d.sobre_hud && !d.arrastando && t.pos.distance(d.inicio) > TOLERANCIA_PX {
                    d.arrastando = true;
                }
                let delta = t.pos - d.ultimo;
                d.ultimo = t.pos;
                if t.fase == Fase::Acabou {
                    self.dedo = None;
                    if d.arrastando || d.sobre_hud {
                        return if d.arrastando && delta != Vec2::ZERO {
                            Acao::Gira(delta)
                        } else {
                            Acao::Nada
                        };
                    }
                    return Acao::Clique(t.pos);
                }
                self.dedo = Some(d);
                if d.arrastando && delta != Vec2::ZERO {
                    Acao::Gira(delta)
                } else {
                    Acao::Nada
                }
            }
        }
    }
}

#[cfg(test)]
mod testes {
    use super::*;
    use macroquad::prelude::vec2;

    fn t(id: u64, fase: Fase, x: f32, y: f32) -> ToqueNoQuadro {
        ToqueNoQuadro {
            id,
            fase,
            pos: vec2(x, y),
        }
    }

    #[test]
    fn toque_curto_e_clique_no_soltar() {
        let mut g = GestoCamera::default();
        assert_eq!(
            g.quadro(&[t(1, Fase::Comecou, 100.0, 100.0)], false),
            Acao::Nada,
            "encostar nao clica"
        );
        assert!(g.ativo());
        assert_eq!(
            g.quadro(&[t(1, Fase::Segurando, 104.0, 101.0)], false),
            Acao::Nada,
            "tremida nao gira"
        );
        assert_eq!(
            g.quadro(&[t(1, Fase::Acabou, 104.0, 101.0)], false),
            Acao::Clique(vec2(104.0, 101.0))
        );
        assert_eq!(g.quadro(&[], false), Acao::Nada);
        assert!(!g.ativo());
        // Encostou e soltou no mesmo quadro.
        assert_eq!(
            g.quadro(&[t(2, Fase::Acabou, 5.0, 5.0)], false),
            Acao::Clique(vec2(5.0, 5.0))
        );
    }

    #[test]
    fn arrastar_alem_do_limiar_gira_e_nao_clica() {
        let mut g = GestoCamera::default();
        g.quadro(&[t(1, Fase::Comecou, 100.0, 100.0)], false);
        assert_eq!(
            g.quadro(&[t(1, Fase::Segurando, 130.0, 110.0)], false),
            Acao::Gira(vec2(30.0, 10.0))
        );
        assert_eq!(
            g.quadro(&[t(1, Fase::Segurando, 140.0, 110.0)], false),
            Acao::Gira(vec2(10.0, 0.0))
        );
        assert_eq!(
            g.quadro(&[t(1, Fase::Segurando, 140.0, 110.0)], false),
            Acao::Nada,
            "parado nao gira"
        );
        let fim = g.quadro(&[t(1, Fase::Acabou, 140.0, 110.0)], false);
        assert!(
            !matches!(fim, Acao::Clique(_)),
            "soltar depois de arrastar nao clica: {fim:?}"
        );
    }

    #[test]
    fn comecar_em_cima_do_hud_nao_gira_nem_clica() {
        let mut g = GestoCamera::default();
        g.quadro(&[t(1, Fase::Comecou, 10.0, 10.0)], true);
        assert_eq!(
            g.quadro(&[t(1, Fase::Segurando, 80.0, 10.0)], false),
            Acao::Nada
        );
        assert_eq!(
            g.quadro(&[t(1, Fase::Acabou, 10.0, 10.0)], false),
            Acao::Nada,
            "o botao cuida do proprio toque"
        );
    }

    #[test]
    fn pinca_da_zoom_na_direcao_certa() {
        let mut g = GestoCamera::default();
        g.quadro(
            &[
                t(1, Fase::Comecou, 100.0, 100.0),
                t(2, Fase::Comecou, 200.0, 100.0),
            ],
            false,
        );
        match g.quadro(
            &[
                t(1, Fase::Segurando, 80.0, 100.0),
                t(2, Fase::Segurando, 220.0, 100.0),
            ],
            false,
        ) {
            Acao::Zoom(d) => assert!(d > 0.0, "abrir os dedos = positivo: {d}"),
            outra => panic!("esperava zoom, veio {outra:?}"),
        }
        match g.quadro(
            &[
                t(1, Fase::Segurando, 120.0, 100.0),
                t(2, Fase::Segurando, 180.0, 100.0),
            ],
            false,
        ) {
            Acao::Zoom(d) => assert!(d < 0.0, "fechar = negativo: {d}"),
            outra => panic!("esperava zoom, veio {outra:?}"),
        }
    }

    #[test]
    fn segundo_dedo_cancela_o_clique_do_primeiro() {
        let mut g = GestoCamera::default();
        g.quadro(&[t(1, Fase::Comecou, 100.0, 100.0)], false);
        g.quadro(
            &[
                t(1, Fase::Segurando, 100.0, 100.0),
                t(2, Fase::Comecou, 150.0, 100.0),
            ],
            false,
        );
        // Releases the second and then the first, both still: no click.
        assert_eq!(
            g.quadro(
                &[
                    t(1, Fase::Segurando, 100.0, 100.0),
                    t(2, Fase::Acabou, 150.0, 100.0)
                ],
                false
            ),
            Acao::Nada
        );
        assert_eq!(
            g.quadro(&[t(1, Fase::Acabou, 100.0, 100.0)], false),
            Acao::Nada
        );
        assert_eq!(g.quadro(&[], false), Acao::Nada);
        assert!(!g.ativo());
    }
}
