//! A long press in one spot.
//!
//! On iOS there is no right button and no hover: what was "right click opens
//! the configuration" becomes HOLDING the button. One detector for everyone,
//! so the rule is the same everywhere: held still for `SEGURAR_S` = long;
//! released before that = short (the normal action); moved beyond
//! `TOLERANCIA_PX` = a drag (neither short nor long — dragging is another
//! gesture, like the bar's AUTO).
use macroquad::prelude::Vec2;

/// How long to hold still for it to become a long press.
pub const SEGURAR_S: f64 = 0.5;
/// More movement than this is a drag, not a press. The same threshold as the
/// AUTO drag (`habilidades_input`), so the two gestures do not fight.
pub const TOLERANCIA_PX: f32 = 12.0;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Toque {
    Nada,
    /// Soltou rapido, parado: a acao normal do botao.
    Curto(u32),
    /// Held still: fires ONCE, with the finger still down.
    Longo(u32),
    /// Moveu: nem curto nem longo.
    Arrasto(u32),
}

#[derive(Debug, Default)]
pub struct ToqueLongo {
    inicio: Option<(u32, Vec2, f64)>,
    disparou: bool,
    longe: bool,
}

impl ToqueLongo {
    /// One frame. `alvo` is the button under the finger at the moment of the
    /// PRESS (`None` = pressed outside: nothing starts).
    pub fn quadro(
        &mut self,
        apertou: bool,
        segurando: bool,
        soltou: bool,
        alvo: Option<u32>,
        pos: Vec2,
        agora: f64,
    ) -> Toque {
        if apertou {
            match alvo {
                Some(id) => {
                    self.inicio = Some((id, pos, agora));
                    self.disparou = false;
                    self.longe = false;
                }
                None => self.inicio = None,
            }
        }
        let Some((id, de, t0)) = self.inicio else {
            return Toque::Nada;
        };
        if pos.distance(de) > TOLERANCIA_PX {
            self.longe = true;
        }
        if !self.disparou && !self.longe && segurando && !soltou && agora - t0 >= SEGURAR_S {
            self.disparou = true;
            return Toque::Longo(id);
        }
        if soltou || !segurando {
            self.inicio = None;
            if self.disparou {
                return Toque::Nada;
            }
            if self.longe {
                return Toque::Arrasto(id);
            }
            return if soltou {
                Toque::Curto(id)
            } else {
                Toque::Nada
            };
        }
        Toque::Nada
    }

    /// Still holding something (for whoever needs to wait for the press to decide).
    pub fn ativo(&self) -> bool {
        self.inicio.is_some()
    }

    /// The long press has already fired in this press.
    pub fn disparou(&self) -> bool {
        self.inicio.is_some() && self.disparou
    }
}

#[cfg(test)]
mod testes {
    use super::*;
    use macroquad::prelude::vec2;

    #[test]
    fn segurar_parado_abre_uma_vez_e_nao_vira_clique() {
        let mut t = ToqueLongo::default();
        let p = vec2(50.0, 50.0);
        assert_eq!(t.quadro(true, true, false, Some(7), p, 0.0), Toque::Nada);
        assert_eq!(t.quadro(false, true, false, None, p, 0.3), Toque::Nada);
        assert_eq!(
            t.quadro(false, true, false, None, vec2(53.0, 51.0), 0.51),
            Toque::Longo(7),
            "tremida pequena ainda e' toque"
        );
        assert!(t.disparou());
        assert_eq!(
            t.quadro(false, true, false, None, p, 0.9),
            Toque::Nada,
            "so' uma vez"
        );
        assert_eq!(
            t.quadro(false, false, true, None, p, 1.0),
            Toque::Nada,
            "soltar depois do longo nao e' clique"
        );
        assert!(!t.ativo());
    }

    #[test]
    fn clique_curto_mantem_a_acao_normal() {
        let mut t = ToqueLongo::default();
        let p = vec2(10.0, 10.0);
        t.quadro(true, true, false, Some(2), p, 5.0);
        assert_eq!(t.quadro(false, false, true, None, p, 5.2), Toque::Curto(2));
        // Apertou e soltou no mesmo quadro.
        assert_eq!(
            t.quadro(true, false, true, Some(3), p, 6.0),
            Toque::Curto(3)
        );
        // Apertou fora de qualquer botao: nada.
        assert_eq!(t.quadro(true, true, false, None, p, 7.0), Toque::Nada);
        assert_eq!(t.quadro(false, false, true, None, p, 7.1), Toque::Nada);
    }

    #[test]
    fn arrastar_nao_abre_nem_clica() {
        let mut t = ToqueLongo::default();
        t.quadro(true, true, false, Some(1), vec2(40.0, 40.0), 0.0);
        assert_eq!(
            t.quadro(false, true, false, None, vec2(40.0, 0.0), 0.2),
            Toque::Nada
        );
        assert_eq!(
            t.quadro(false, true, false, None, vec2(40.0, 0.0), 0.8),
            Toque::Nada,
            "arrastou antes: nao vira longo"
        );
        assert_eq!(
            t.quadro(false, false, true, None, vec2(40.0, 0.0), 0.9),
            Toque::Arrasto(1)
        );
    }
}
