//! When the on-screen keyboard opens and closes, and how far the panel rises.
//!
//! Separate from the platform (`nativo`) so it can be tested: the rule is "a
//! text field with focus = keyboard open", and only changes become a request
//! — asking for `becomeFirstResponder` every frame would make iOS reopen the
//! keyboard endlessly.

#[derive(Default, Debug)]
pub struct TecladoVirtual {
    aberto: bool,
}

impl TecladoVirtual {
    /// `precisa` = some text field has focus this frame. Returns the request to
    /// make (`Some(true)` open, `Some(false)` close) only when it changes.
    pub fn quer(&mut self, precisa: bool) -> Option<bool> {
        (precisa != self.aberto).then(|| {
            self.aberto = precisa;
            precisa
        })
    }

    pub fn aberto(&self) -> bool {
        self.aberto
    }
}

/// Fraction of the bottom of the screen the keyboard covers on a phone in landscape.
const FRACAO_DO_TECLADO: f32 = 0.52;

/// How far to raise a panel so that `fundo_do_campo` (the y of the bottom
/// edge of the focused field) sits above the keyboard. Never rises beyond
/// leaving the panel's top (`topo_do_painel`) 8 px from the edge.
pub fn deslocamento(
    aberto: bool,
    altura_tela: f32,
    topo_do_painel: f32,
    fundo_do_campo: f32,
) -> f32 {
    if !aberto {
        return 0.0;
    }
    let topo_do_teclado = altura_tela * (1.0 - FRACAO_DO_TECLADO);
    let precisa = fundo_do_campo + 12.0 - topo_do_teclado;
    precisa.clamp(0.0, (topo_do_painel - 8.0).max(0.0))
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn so_pede_quando_o_foco_muda() {
        let mut t = TecladoVirtual::default();
        assert_eq!(t.quer(false), None);
        assert_eq!(t.quer(true), Some(true), "campo ganhou foco: abre");
        assert_eq!(t.quer(true), None, "nao repete o pedido todo quadro");
        assert!(t.aberto());
        assert_eq!(t.quer(false), Some(false), "perdeu o foco: fecha");
        assert_eq!(t.quer(false), None);
    }

    #[test]
    fn painel_sobe_so_o_bastante_e_nao_sai_da_tela() {
        assert_eq!(deslocamento(false, 800.0, 200.0, 700.0), 0.0);
        // Keyboard covers from 384 down; the field ends at 500 -> rises 128.
        assert_eq!(deslocamento(true, 800.0, 200.0, 500.0), 128.0);
        // Field already above the keyboard: leave it alone.
        assert_eq!(deslocamento(true, 800.0, 200.0, 300.0), 0.0);
        // Does not rise beyond the panel's top.
        assert_eq!(deslocamento(true, 800.0, 50.0, 760.0), 42.0);
    }
}
