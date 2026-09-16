//! "Tem certeza?" antes de gastar uma poção de efeito que já está ativa.
//!
//! O servidor RENOVA a hora cheia em vez de somar (`renovar_bonus_xp`,
//! `renovar_buff`): beber a segunda com 55 minutos restantes joga esses 55
//! minutos fora. Quem usa no automático nunca cai aqui — a barra já pula buff
//! ativo; isto é só pro toque na bolsa e no slot.

use macroquad::prelude::*;

use crate::hud_estilo as estilo;

/// O uso que está esperando resposta.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Pendente {
    /// Duplo toque na bolsa.
    Bolsa { slot: u16, item: u16 },
    /// Slot da barra de itens.
    Barra { i: usize, forte: bool, item: u16 },
}

impl Pendente {
    pub fn item(self) -> u16 {
        match self {
            Pendente::Bolsa { item, .. } | Pendente::Barra { item, .. } => item,
        }
    }
}

/// "1h 02m" / "12m" / "40s": o que ainda resta do efeito.
pub fn resta(segundos: i64) -> String {
    let s = segundos.max(0);
    let (h, m) = (s / 3600, s / 60 % 60);
    if h > 0 {
        format!("{h}h {m:02}m")
    } else if m > 0 {
        format!("{m}m")
    } else {
        format!("{s}s")
    }
}

/// Desenha a janela. `None` = ainda esperando; `Some(true)` = usar assim
/// mesmo; `Some(false)` = cancelou.
pub fn desenha(p: Pendente, nome: &str, restante_s: i64) -> Option<bool> {
    let f = estilo::fator_texto();
    let seguro = crate::hud_layout::tela_segura();
    let (w, h) = ((520.0 * f).min(seguro.w - 24.0), (250.0 * f).min(seguro.h - 24.0));
    let r = Rect::new(seguro.center().x - w * 0.5, seguro.center().y - h * 0.5, w, h);
    crate::hud_layout::escurece(0.5);
    estilo::painel(r);
    let x = r.x + 24.0 * f;
    estilo::texto_forte(x, r.y + 40.0 * f, "Usar de novo?", 20, estilo::OURO);
    estilo::texto_ajustado(
        &format!("{nome} já está ativa, com {} restando.", resta(restante_s)),
        x,
        r.y + 78.0 * f,
        w - 48.0 * f,
        15,
        estilo::TEXTO,
    );
    estilo::texto_ajustado(
        "Usar outra recomeça a hora do zero: o tempo que resta é perdido, e o efeito não acumula.",
        x,
        r.y + 108.0 * f,
        w - 48.0 * f,
        14,
        estilo::SUAVE,
    );
    let _ = p;
    let bw = (w - 60.0 * f) * 0.5;
    let bh = 46.0 * f;
    let by = r.y + r.h - 24.0 * f - bh;
    let cancelar = Rect::new(x, by, bw, bh);
    let usar = Rect::new(cancelar.x + bw + 12.0 * f, by, bw, bh);
    let m = Vec2::from(mouse_position());
    estilo::cartao(cancelar, cancelar.contains(m), false);
    estilo::texto_centro(cancelar.center().x, cancelar.center().y + 6.0 * f, "Cancelar", 16, estilo::TEXTO);
    estilo::cartao(usar, usar.contains(m), true);
    estilo::texto_centro_forte(usar.center().x, usar.center().y + 6.0 * f, "Usar assim mesmo", 16, estilo::OURO);
    if !is_mouse_button_pressed(MouseButton::Left) {
        return None;
    }
    if usar.contains(m) {
        return Some(true);
    }
    if cancelar.contains(m) || !r.contains(m) {
        return Some(false);
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn texto_do_tempo_restante() {
        assert_eq!(resta(3725), "1h 02m");
        assert_eq!(resta(720), "12m");
        assert_eq!(resta(40), "40s");
        assert_eq!(resta(-5), "0s");
    }

    #[test]
    fn guarda_o_item_de_cada_origem() {
        assert_eq!(Pendente::Bolsa { slot: 3, item: 350 }.item(), 350);
        assert_eq!(Pendente::Barra { i: 1, forte: false, item: 352 }.item(), 352);
    }
}
