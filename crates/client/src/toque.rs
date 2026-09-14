//! Toque longo num lugar so'.
//!
//! No iOS nao ha' botao direito nem hover: o que era "clique direito abre a
//! configuracao" vira SEGURAR o botao. Um detector pra todos, pra regra ser a
//! mesma em todo canto: segurou parado `SEGURAR_S` = longo; soltou antes =
//! curto (a acao normal); moveu alem de `TOLERANCIA_PX` = arrasto (nem curto
//! nem longo — quem arrasta e' outro gesto, como o AUTO da barra).
use macroquad::prelude::Vec2;

/// Quanto segurar parado pra virar toque longo.
pub const SEGURAR_S: f64 = 0.5;
/// Mais que isto de movimento e' arrasto, nao toque. O mesmo limiar do
/// arrasto do AUTO (`habilidades_input`), pra os dois gestos nao brigarem.
pub const TOLERANCIA_PX: f32 = 12.0;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Toque {
    Nada,
    /// Soltou rapido, parado: a acao normal do botao.
    Curto(u32),
    /// Segurou parado: dispara UMA vez, ainda com o dedo em cima.
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
    /// Um quadro. `alvo` e' o botao sob o dedo no momento em que APERTOU
    /// (`None` = apertou fora: nao comeca nada).
    pub fn quadro(&mut self, apertou: bool, segurando: bool, soltou: bool, alvo: Option<u32>, pos: Vec2, agora: f64) -> Toque {
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
        let Some((id, de, t0)) = self.inicio else { return Toque::Nada };
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
            return if soltou { Toque::Curto(id) } else { Toque::Nada };
        }
        Toque::Nada
    }

    /// Segurando algo ainda (pra quem precisa esperar o toque decidir).
    pub fn ativo(&self) -> bool {
        self.inicio.is_some()
    }

    /// Ja' disparou o longo neste aperto.
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
        assert_eq!(t.quadro(false, true, false, None, vec2(53.0, 51.0), 0.51), Toque::Longo(7), "tremida pequena ainda e' toque");
        assert!(t.disparou());
        assert_eq!(t.quadro(false, true, false, None, p, 0.9), Toque::Nada, "so' uma vez");
        assert_eq!(t.quadro(false, false, true, None, p, 1.0), Toque::Nada, "soltar depois do longo nao e' clique");
        assert!(!t.ativo());
    }

    #[test]
    fn clique_curto_mantem_a_acao_normal() {
        let mut t = ToqueLongo::default();
        let p = vec2(10.0, 10.0);
        t.quadro(true, true, false, Some(2), p, 5.0);
        assert_eq!(t.quadro(false, false, true, None, p, 5.2), Toque::Curto(2));
        // Apertou e soltou no mesmo quadro.
        assert_eq!(t.quadro(true, false, true, Some(3), p, 6.0), Toque::Curto(3));
        // Apertou fora de qualquer botao: nada.
        assert_eq!(t.quadro(true, true, false, None, p, 7.0), Toque::Nada);
        assert_eq!(t.quadro(false, false, true, None, p, 7.1), Toque::Nada);
    }

    #[test]
    fn arrastar_nao_abre_nem_clica() {
        let mut t = ToqueLongo::default();
        t.quadro(true, true, false, Some(1), vec2(40.0, 40.0), 0.0);
        assert_eq!(t.quadro(false, true, false, None, vec2(40.0, 0.0), 0.2), Toque::Nada);
        assert_eq!(t.quadro(false, true, false, None, vec2(40.0, 0.0), 0.8), Toque::Nada, "arrastou antes: nao vira longo");
        assert_eq!(t.quadro(false, false, true, None, vec2(40.0, 0.0), 0.9), Toque::Arrasto(1));
    }
}
