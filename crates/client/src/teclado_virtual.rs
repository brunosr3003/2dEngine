//! Quando o teclado da tela abre e fecha, e quanto o painel sobe.
//!
//! Separado da plataforma (`nativo`) pra ser testado: a regra e' "campo de
//! texto com foco = teclado aberto", e so' mudancas viram pedido — pedir
//! `becomeFirstResponder` todo quadro faria o iOS reabrir o teclado sem parar.

#[derive(Default, Debug)]
pub struct TecladoVirtual {
    aberto: bool,
}

impl TecladoVirtual {
    /// `precisa` = algum campo de texto esta' com foco neste quadro. Devolve o
    /// pedido a fazer (`Some(true)` abrir, `Some(false)` fechar) so' quando muda.
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

/// Fracao de baixo da tela que o teclado cobre no celular deitado.
const FRACAO_DO_TECLADO: f32 = 0.52;

/// Quanto subir um painel pra que `fundo_do_campo` (y da borda de baixo do
/// campo com foco) fique acima do teclado. Nunca sobe alem de deixar o topo
/// do painel (`topo_do_painel`) a 8 px da borda.
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
        // Teclado cobre de 384 pra baixo; campo termina em 500 -> sobe 128.
        assert_eq!(deslocamento(true, 800.0, 200.0, 500.0), 128.0);
        // Campo ja' acima do teclado: nao mexe.
        assert_eq!(deslocamento(true, 800.0, 200.0, 300.0), 0.0);
        // Nao sobe alem do topo do painel.
        assert_eq!(deslocamento(true, 800.0, 50.0, 760.0), 42.0);
    }
}
