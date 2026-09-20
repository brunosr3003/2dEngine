//! Configuracao do AUTO COLETA, no molde do MIR4: o que coletar (madeira e
//! cada cor de pedra) e o raio de busca a partir de onde foi ligado. Abre com
//! o botao direito no AUTO COLETA — nenhuma tecla abre. Salva nas
//! preferencias do personagem.
use macroquad::prelude::*;

use crate::hud_estilo as estilo;

const PASSO_RAIO: f32 = 10.0;

#[derive(Default)]
pub struct ConfigColeta {
    pub aberto: bool,
}

/// Liga/desliga um tipo. Nao deixa desmarcar o ultimo: sem tipo nenhum o
/// auto nunca acharia nada.
pub fn alterna_tipo(tipos: &mut [bool; 5], i: usize) -> bool {
    if i >= tipos.len() {
        return false;
    }
    if tipos[i] && tipos.iter().filter(|t| **t).count() == 1 {
        return false;
    }
    tipos[i] = !tipos[i];
    true
}

/// Raio em passos de 10, dentro da faixa permitida.
pub fn ajusta_raio(raio: f32, passos: i32) -> f32 {
    let r = ((raio / PASSO_RAIO).round() + passos as f32) * PASSO_RAIO;
    r.clamp(shared::COLETA_RAIO_AUTO_MIN, shared::COLETA_RAIO_AUTO_MAX)
}

impl ConfigColeta {
    pub fn abrir(&mut self) {
        self.aberto = true;
    }

    pub fn fechar(&mut self) {
        self.aberto = false;
    }

    /// Desenha e trata o clique. Devolve se a configuracao mudou.
    pub fn desenha(
        &mut self,
        tipos: &mut [bool; 5],
        energia: &mut bool,
        raio: &mut f32,
        defender: &mut bool,
    ) -> bool {
        let (sw, sh) = (screen_width(), screen_height());
        let r = Rect::new(sw * 0.5 - 200.0, sh * 0.5 - 207.0, 400.0, 414.0);
        estilo::painel(r);
        estilo::texto(r.x + 18.0, r.y + 32.0, "Auto coleta", 20, estilo::OURO);
        let fechar = Rect::new(r.x + r.w - 38.0, r.y + 10.0, 28.0, 28.0);
        estilo::texto_centro(
            fechar.center().x,
            fechar.center().y + 7.0,
            "X",
            18,
            estilo::TEXTO,
        );
        let m = Vec2::from(mouse_position());
        let clicou = is_mouse_button_pressed(MouseButton::Left);
        if clicou && fechar.contains(m) {
            self.fechar();
            return false;
        }
        estilo::texto(r.x + 18.0, r.y + 62.0, "O que coletar", 14, estilo::SUAVE);
        let mut mudou = false;
        for i in 0..5 {
            let linha = Rect::new(r.x + 18.0, r.y + 74.0 + i as f32 * 34.0, r.w - 36.0, 28.0);
            let caixa = Rect::new(linha.x, linha.y + 4.0, 20.0, 20.0);
            draw_rectangle_lines(caixa.x, caixa.y, caixa.w, caixa.h, 2.0, estilo::OURO);
            if tipos[i] {
                draw_rectangle(
                    caixa.x + 4.0,
                    caixa.y + 4.0,
                    caixa.w - 8.0,
                    caixa.h - 8.0,
                    estilo::AUTO,
                );
            }
            estilo::texto(
                linha.x + 32.0,
                linha.y + 20.0,
                shared::nome_do_no(i as u8),
                15,
                estilo::TEXTO,
            );
            if clicou && linha.contains(m) && alterna_tipo(tipos, i) {
                mudou = true;
            }
        }
        let linha_energia = Rect::new(r.x + 18.0, r.y + 244.0, r.w - 36.0, 28.0);
        let caixa = Rect::new(linha_energia.x, linha_energia.y + 4.0, 20.0, 20.0);
        draw_rectangle_lines(caixa.x, caixa.y, caixa.w, caixa.h, 2.0, estilo::OURO);
        if *energia {
            draw_rectangle(
                caixa.x + 4.0,
                caixa.y + 4.0,
                caixa.w - 8.0,
                caixa.h - 8.0,
                estilo::AUTO,
            );
        }
        estilo::texto(
            linha_energia.x + 32.0,
            linha_energia.y + 20.0,
            "Energia",
            15,
            estilo::TEXTO,
        );
        if clicou && linha_energia.contains(m) {
            *energia = !*energia;
            mudou = true;
        }
        let y = r.y + 296.0;
        estilo::texto(r.x + 18.0, y, "Raio de busca", 14, estilo::SUAVE);
        let menos = Rect::new(r.x + 160.0, y - 20.0, 30.0, 28.0);
        let mais = Rect::new(r.x + 280.0, y - 20.0, 30.0, 28.0);
        for (b, t) in [(menos, "-"), (mais, "+")] {
            estilo::painel(b);
            estilo::texto_centro(b.center().x, b.center().y + 7.0, t, 18, estilo::OURO);
        }
        estilo::texto_centro(
            r.x + 235.0,
            y,
            &format!("{:.0} m", *raio),
            16,
            estilo::TEXTO,
        );
        for (b, passos) in [(menos, -1), (mais, 1)] {
            if clicou && b.contains(m) {
                let novo = ajusta_raio(*raio, passos);
                if novo != *raio {
                    *raio = novo;
                    mudou = true;
                }
            }
        }
        // Defender: apanhou coletando, mata o bicho e volta a coletar.
        let linha = Rect::new(r.x + 18.0, y + 16.0, r.w - 36.0, 28.0);
        let caixa = Rect::new(linha.x, linha.y + 4.0, 20.0, 20.0);
        draw_rectangle_lines(caixa.x, caixa.y, caixa.w, caixa.h, 2.0, estilo::OURO);
        if *defender {
            draw_rectangle(
                caixa.x + 4.0,
                caixa.y + 4.0,
                caixa.w - 8.0,
                caixa.h - 8.0,
                estilo::AUTO,
            );
        }
        estilo::texto(
            linha.x + 32.0,
            linha.y + 20.0,
            "Defender-se: atacado, mata o bicho e volta",
            15,
            estilo::TEXTO,
        );
        if clicou && linha.contains(m) {
            *defender = !*defender;
            mudou = true;
        }
        estilo::texto(
            r.x + 18.0,
            r.y + r.h - 18.0,
            "Procura a partir de onde o AUTO foi ligado.",
            12,
            estilo::SUAVE,
        );
        mudou
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nao_desmarca_o_ultimo_e_raio_fica_na_faixa() {
        let mut t = [true, false, false, false, false];
        assert!(!alterna_tipo(&mut t, 0), "ultimo tipo nao sai");
        assert!(alterna_tipo(&mut t, 3));
        assert!(alterna_tipo(&mut t, 0));
        assert_eq!(t, [false, false, false, true, false]);
        assert!(!alterna_tipo(&mut t, 9));
        assert_eq!(ajusta_raio(60.0, 1), 70.0);
        assert_eq!(ajusta_raio(20.0, -1), shared::COLETA_RAIO_AUTO_MIN);
        assert_eq!(ajusta_raio(100.0, 3), shared::COLETA_RAIO_AUTO_MAX);
    }
}
