//! Menu → Sistema → Interface: o tamanho da interface (HUD e textos). No
//! celular a tela e' densa e 100% fica miudo, entao o padrao la' e' 130%.
//! Muda na hora e salva nas preferencias do personagem.
use macroquad::prelude::*;

use crate::hud_estilo as estilo;
use crate::hud_layout;

pub const PASSO: f32 = 0.1;

#[derive(Default)]
pub struct ConfigInterface {
    pub aberto: bool,
}

/// Em passos de 10%, dentro da faixa.
pub fn ajusta(escala: f32, passos: i32) -> f32 {
    (((escala / PASSO).round() + passos as f32) * PASSO).clamp(hud_layout::ESCALA_UI_MIN, hud_layout::ESCALA_UI_MAX)
}

impl ConfigInterface {
    pub fn abrir(&mut self) {
        self.aberto = true;
    }

    pub fn fechar(&mut self) {
        self.aberto = false;
    }

    /// Desenha e trata o clique. Devolve a escala nova quando mudou.
    pub fn desenha(&mut self, atual: f32) -> Option<f32> {
        // O painel cresce junto com o texto que ele mostra.
        let f = estilo::fator_texto();
        let seguro = hud_layout::tela_segura();
        let (w, h) = ((420.0 * f).min(seguro.w - 16.0), 250.0 * f);
        let r = Rect::new(seguro.center().x - w * 0.5, seguro.center().y - h * 0.5, w, h);
        estilo::painel(r);
        estilo::texto(r.x + 18.0 * f, r.y + 34.0 * f, "Interface", 20, estilo::OURO);
        let fechar = Rect::new(r.x + r.w - 44.0 * f, r.y + 8.0 * f, 36.0 * f, 36.0 * f);
        estilo::texto_centro(fechar.center().x, fechar.center().y + 7.0 * f, "X", 18, estilo::TEXTO);
        let m = Vec2::from(mouse_position());
        let clicou = is_mouse_button_pressed(MouseButton::Left);
        if clicou && fechar.contains(m) {
            self.fechar();
            return None;
        }
        estilo::texto(r.x + 18.0 * f, r.y + 70.0 * f, "Tamanho do HUD e dos textos", 14, estilo::SUAVE);
        let y = r.y + 88.0 * f;
        let menos = Rect::new(r.x + 18.0 * f, y, 60.0 * f, 50.0 * f);
        let mais = Rect::new(r.x + r.w - 78.0 * f, y, 60.0 * f, 50.0 * f);
        for (b, t) in [(menos, "-"), (mais, "+")] {
            estilo::painel(b);
            estilo::texto_centro(b.center().x, b.center().y + 9.0 * f, t, 26, estilo::OURO);
        }
        estilo::texto_centro_forte(r.center().x, y + 34.0 * f, &format!("{:.0}%", atual * 100.0), 24, estilo::TEXTO);
        let padrao = hud_layout::escala_ui_padrao();
        let botao_padrao = Rect::new(r.x + 18.0 * f, y + 64.0 * f, r.w - 36.0 * f, 42.0 * f);
        estilo::painel(botao_padrao);
        estilo::texto_centro(
            botao_padrao.center().x,
            botao_padrao.center().y + 6.0 * f,
            &format!("Voltar ao padrão ({:.0}%)", padrao * 100.0),
            15,
            estilo::TEXTO,
        );
        estilo::texto(r.x + 18.0 * f, r.y + r.h - 18.0 * f, "Muda na hora e fica salvo no personagem.", 12, estilo::SUAVE);
        if !clicou {
            return None;
        }
        let nova = if menos.contains(m) {
            ajusta(atual, -1)
        } else if mais.contains(m) {
            ajusta(atual, 1)
        } else if botao_padrao.contains(m) {
            padrao
        } else {
            return None;
        };
        ((nova - atual).abs() > 1e-3).then_some(nova)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn passos_de_dez_por_cento_na_faixa() {
        assert!((ajusta(1.0, 1) - 1.1).abs() < 1e-5);
        assert!((ajusta(1.3, -2) - 1.1).abs() < 1e-5);
        assert_eq!(ajusta(hud_layout::ESCALA_UI_MAX, 1), hud_layout::ESCALA_UI_MAX);
        assert_eq!(ajusta(hud_layout::ESCALA_UI_MIN, -1), hud_layout::ESCALA_UI_MIN);
    }
}
