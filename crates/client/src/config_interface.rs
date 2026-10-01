//! Menu → Sistema → Interface: o tamanho da interface (HUD e textos), o modo
//! economia de energia e o idioma. (Shadows and anti-aliasing live in the
//! Graphics panel, `config_graficos`.) No celular a tela e' densa e 100% fica miudo, entao o
//! padrao e' 160%. Muda na hora e salva nas preferencias do personagem.
use macroquad::prelude::*;

use crate::economia;
use crate::hud_estilo as estilo;
use crate::hud_layout;

pub const PASSO: f32 = 0.1;

#[derive(Default)]
pub struct ConfigInterface {
    pub aberto: bool,
    /// The tall layout scrolls when it doesn't fit the screen.
    rolagem: crate::rolagem::Rolagem,
}

/// What changed this frame.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Mudanca {
    Escala(f32),
    /// Minutes idle before entering power-saving mode on its own (0 = never).
    EconomiaAuto(u16),
    EconomiaAgora,
    /// O idioma da interface. Vale na hora e fica salvo no APARELHO — ver
    /// `lembranca::Prefs::idioma`.
    Idioma(shared::idioma::Idioma),
}

/// Em passos de 10%, dentro da faixa.
pub fn ajusta(escala: f32, passos: i32) -> f32 {
    (((escala / PASSO).round() + passos as f32) * PASSO)
        .clamp(hud_layout::ESCALA_UI_MIN, hud_layout::ESCALA_UI_MAX)
}

impl ConfigInterface {
    pub fn abrir(&mut self) {
        self.aberto = true;
        self.rolagem.zera();
    }

    pub fn fechar(&mut self) {
        self.aberto = false;
    }

    /// For the HUD preview: the content scrolled all the way down (the draw
    /// clamps it to the real end).
    #[cfg(debug_assertions)]
    pub fn rolar_ao_fim(&mut self) {
        self.rolagem.pos = f32::MAX / 4.0;
    }

    /// Desenha e trata o clique.
    pub fn desenha(&mut self, atual: f32, economia_auto: u16) -> Option<Mudanca> {
        // The panel grows along with the text it shows.
        let f = estilo::fator_texto();
        let seguro = hud_layout::tela_segura();
        if seguro.h < 500.0 * f {
            let k = ((seguro.h - 16.0) / 352.0).min(f);
            return estilo::no_painel(k, || self.desenha_compacto(atual, economia_auto));
        }
        // 482: size, battery saver and language. Shadows and anti-aliasing
        // moved to their own Graphics panel (`config_graficos`).
        let conteudo_h = 482.0 * f;
        let (w, h) = (
            (420.0 * f).min(seguro.w - 16.0),
            conteudo_h.min(seguro.h - 16.0),
        );
        let r = Rect::new(
            seguro.center().x - w * 0.5,
            seguro.center().y - h * 0.5,
            w,
            h,
        );
        estilo::painel(r);
        estilo::texto(
            r.x + 18.0 * f,
            r.y + 34.0 * f,
            "Interface",
            20,
            estilo::OURO,
        );
        crate::sons::controle(Rect::new(r.x + r.w - 196.0*f, r.y + 6.0*f, 144.0*f, 34.0*f));
        let fechar = Rect::new(r.x + r.w - 44.0 * f, r.y + 8.0 * f, 36.0 * f, 36.0 * f);
        estilo::texto_centro(
            fechar.center().x,
            fechar.center().y + 7.0 * f,
            "X",
            18,
            estilo::TEXTO,
        );
        let m = Vec2::from(mouse_position());
        let clicou = crate::foco::clique();
        if clicou && fechar.contains(m) {
            self.fechar();
            return None;
        }
        // THE CONTENT SCROLLS under the fixed title bar. The owner, 30/09/2026,
        // on a Mac at 160%: the panel was taller than the screen and the
        // anti-aliasing row, at the bottom, simply wasn't there — "this hud
        // needs a scrollbar". Wheel, drag and a bar, like every other list
        // (`rolagem`). A tap on a control counts when the finger lifts without
        // dragging, so a drag that starts on a button scrolls instead.
        let topo = 46.0 * f;
        let area = Rect::new(r.x, r.y + topo, r.w, r.h - topo - 4.0 * f);
        let total = conteudo_h - topo;
        let toque = self.rolagem.quadro(area, total, 60.0 * f);
        let em = |b: Rect| toque.is_some_and(|p| b.contains(p) && area.contains(p));
        let r = Rect::new(r.x, r.y - self.rolagem.pos, r.w, conteudo_h);
        crate::rolagem::recortar(Some(area));
        estilo::texto(
            r.x + 18.0 * f,
            r.y + 70.0 * f,
            "Size of the HUD and text",
            14,
            estilo::SUAVE,
        );
        let y = r.y + 88.0 * f;
        let menos = Rect::new(r.x + 18.0 * f, y, 60.0 * f, 50.0 * f);
        let mais = Rect::new(r.x + r.w - 78.0 * f, y, 60.0 * f, 50.0 * f);
        for (b, t) in [(menos, "-"), (mais, "+")] {
            estilo::painel(b);
            estilo::texto_centro(b.center().x, b.center().y + 9.0 * f, t, 26, estilo::OURO);
        }
        estilo::texto_centro_forte(
            r.center().x,
            y + 34.0 * f,
            &format!("{:.0}%", atual * 100.0),
            24,
            estilo::TEXTO,
        );
        let padrao = hud_layout::escala_ui_padrao();
        let botao_padrao = Rect::new(r.x + 18.0 * f, y + 64.0 * f, r.w - 36.0 * f, 42.0 * f);
        estilo::painel(botao_padrao);
        estilo::texto_centro(
            botao_padrao.center().x,
            botao_padrao.center().y + 6.0 * f,
            &format!("Back to default ({:.0}%)", padrao * 100.0),
            15,
            estilo::TEXTO,
        );

        // ── economia de energia ──
        let ye = botao_padrao.y + botao_padrao.h + 34.0 * f;
        estilo::texto(r.x + 18.0 * f, ye, "Battery saver", 14, estilo::SUAVE);
        let agora = Rect::new(r.x + 18.0 * f, ye + 12.0 * f, r.w - 36.0 * f, 42.0 * f);
        estilo::painel(agora);
        economia::bateria(
            vec2(agora.x + 26.0 * f, agora.center().y),
            9.0 * f,
            Color::new(0.45, 0.85, 0.52, 1.0),
        );
        estilo::texto_centro(
            agora.center().x,
            agora.center().y + 6.0 * f,
            "Enable now",
            15,
            estilo::TEXTO,
        );
        estilo::texto(
            r.x + 18.0 * f,
            agora.y + agora.h + 26.0 * f,
            "Turn on by itself after idling for",
            12,
            estilo::SUAVE,
        );
        let n = economia::OPCOES_AUTO_MIN.len() as f32;
        let vao = 8.0 * f;
        let cw = (r.w - 36.0 * f - vao * (n - 1.0)) / n;
        let yc = agora.y + agora.h + 36.0 * f;
        let chips: Vec<(Rect, u16)> = economia::OPCOES_AUTO_MIN
            .iter()
            .enumerate()
            .map(|(i, &min)| {
                (
                    Rect::new(r.x + 18.0 * f + i as f32 * (cw + vao), yc, cw, 38.0 * f),
                    min,
                )
            })
            .collect();
        for &(c, min) in &chips {
            let marcado = min == economia_auto;
            estilo::cartao(c, c.contains(m), marcado);
            let t = if min == 0 {
                "Never".to_string()
            } else {
                format!("{min} min")
            };
            estilo::texto_centro(
                c.center().x,
                c.center().y + 5.0 * f,
                &t,
                14,
                if marcado { estilo::OURO } else { estilo::TEXTO },
            );
        }

        // ── idioma ──
        //
        // It lives here, and not on a screen of its own, because someone looking
        // for language looks in options — and because the row is the same as the
        // shadows one: two exclusive choices, one ticked.
        let yi = yc + 56.0 * f;
        estilo::texto(r.x + 18.0 * f, yi, "Game language", 14, estilo::SUAVE);
        let atual_idioma = shared::idioma::atual();
        let li = (r.w - 44.0 * f) / 2.0;
        let idiomas: Vec<_> = shared::idioma::Idioma::TODAS
            .iter()
            .enumerate()
            .map(|(i, &lang)| {
                let b = Rect::new(
                    r.x + 18.0 * f + i as f32 * (li + 8.0 * f),
                    yi + 10.0 * f,
                    li,
                    42.0 * f,
                );
                estilo::cartao(b, b.contains(m), lang == atual_idioma);
                // Each language's name IN ITS OWN LANGUAGE, and therefore outside the
                // dictionary: someone looking for English looks for "English".
                estilo::texto_centro(
                    b.center().x,
                    b.center().y + 6.0 * f,
                    lang.nome(),
                    15,
                    if lang == atual_idioma {
                        estilo::OURO
                    } else {
                        estilo::TEXTO
                    },
                );
                (b, lang)
            })
            .collect();
        estilo::texto(
            r.x + 18.0 * f,
            yi + 70.0 * f,
            "Untranslated lines show in Portuguese.",
            11,
            estilo::SUAVE,
        );


        estilo::texto(
            r.x + 18.0 * f,
            r.y + r.h - 18.0 * f,
            "Changes apply at once and are saved on your character.",
            12,
            estilo::SUAVE,
        );
        crate::rolagem::recortar(None);
        self.rolagem.desenha(area, total);
        if toque.is_none() {
            return None;
        }
        if let Some(&(_, lang)) = idiomas.iter().find(|(b, _)| em(*b)) {
            return (lang != atual_idioma).then_some(Mudanca::Idioma(lang));
        }
        if em(agora) {
            self.fechar();
            return Some(Mudanca::EconomiaAgora);
        }
        if let Some(&(_, min)) = chips.iter().find(|(c, _)| em(*c)) {
            return (min != economia_auto).then_some(Mudanca::EconomiaAuto(min));
        }
        let nova = if em(menos) {
            ajusta(atual, -1)
        } else if em(mais) {
            ajusta(atual, 1)
        } else if em(botao_padrao) {
            padrao
        } else {
            return None;
        };
        ((nova - atual).abs() > 1e-3).then_some(Mudanca::Escala(nova))
    }

    /// Em tela horizontal baixa, os controles ocupam duas colunas.
    fn desenha_compacto(&mut self, atual: f32, economia_auto: u16) -> Option<Mudanca> {
        let f = estilo::fator_texto();
        let seguro = hud_layout::tela_segura();
        let w = (760.0 * f).min(seguro.w - 16.0);
        let r = Rect::new(
            seguro.center().x - w * 0.5,
            seguro.center().y - 176.0 * f,
            w,
            352.0 * f,
        );
        estilo::painel(r);
        estilo::texto(
            r.x + 16.0 * f,
            r.y + 30.0 * f,
            "Interface",
            20,
            estilo::OURO,
        );
        crate::sons::controle(Rect::new(r.x + r.w - 196.0*f, r.y + 6.0*f, 144.0*f, 34.0*f));
        let fechar = Rect::new(r.x + r.w - 43.0 * f, r.y + 5.0 * f, 36.0 * f, 36.0 * f);
        estilo::texto_centro(
            fechar.center().x,
            fechar.center().y + 7.0 * f,
            "X",
            18,
            estilo::TEXTO,
        );
        let m = Vec2::from(mouse_position());
        let clicou = crate::foco::clique();
        if clicou && fechar.contains(m) {
            self.fechar();
            return None;
        }
        let col = (r.w - 48.0 * f) * 0.5;
        let lx = r.x + 16.0 * f;
        let rx = lx + col + 16.0 * f;
        estilo::texto(lx, r.y + 61.0 * f, "HUD size", 14, estilo::SUAVE);
        let menos = Rect::new(lx, r.y + 75.0 * f, 50.0 * f, 42.0 * f);
        let mais = Rect::new(lx + col - 50.0 * f, menos.y, 50.0 * f, 42.0 * f);
        for (b, nome) in [(menos, "-"), (mais, "+")] {
            estilo::painel(b);
            estilo::texto_centro(b.center().x, b.center().y + 8.0 * f, nome, 23, estilo::OURO);
        }
        estilo::texto_centro(
            lx + col * 0.5,
            menos.center().y + 7.0 * f,
            &format!("{:.0}%", atual * 100.0),
            22,
            estilo::TEXTO,
        );
        let padrao = Rect::new(lx, r.y + 132.0 * f, col, 38.0 * f);
        estilo::painel(padrao);
        estilo::texto_centro(
            padrao.center().x,
            padrao.center().y + 6.0 * f,
            "Default size",
            14,
            estilo::TEXTO,
        );
        estilo::texto(
            lx,
            r.y + 194.0 * f,
            "Battery saver",
            14,
            estilo::SUAVE,
        );
        let agora = Rect::new(lx, r.y + 204.0 * f, col, 38.0 * f);
        estilo::painel(agora);
        estilo::texto_centro(
            agora.center().x,
            agora.center().y + 6.0 * f,
            "Enable now",
            14,
            estilo::TEXTO,
        );
        estilo::texto(
            lx,
            r.y + 263.0 * f,
            "Auto after idling",
            12,
            estilo::SUAVE,
        );
        let n = economia::OPCOES_AUTO_MIN.len();
        let cw = (col - 6.0 * f * (n as f32 - 1.0)) / n as f32;
        let chips: Vec<_> = economia::OPCOES_AUTO_MIN
            .iter()
            .enumerate()
            .map(|(i, &min)| {
                let b = Rect::new(
                    lx + i as f32 * (cw + 6.0 * f),
                    r.y + 271.0 * f,
                    cw,
                    32.0 * f,
                );
                estilo::cartao(b, b.contains(m), min == economia_auto);
                estilo::texto_centro(
                    b.center().x,
                    b.center().y + 5.0 * f,
                    &if min == 0 {
                        "Never".to_string()
                    } else {
                        format!("{min} min")
                    },
                    12,
                    if min == economia_auto {
                        estilo::OURO
                    } else {
                        estilo::TEXTO
                    },
                );
                (b, min)
            })
            .collect();
        // The language in the compact layout too: without this, a phone held
        // upright with a short screen would have no way to change language.
        estilo::texto(rx, r.y + 61.0 * f, "Language", 14, estilo::SUAVE);
        let atual_idioma = shared::idioma::atual();
        let iw = (col - 6.0 * f) / 2.0;
        let idiomas: Vec<_> = shared::idioma::Idioma::TODAS
            .iter()
            .enumerate()
            .map(|(i, &lang)| {
                let b = Rect::new(rx + i as f32 * (iw + 6.0 * f), r.y + 75.0 * f, iw, 42.0 * f);
                estilo::cartao(b, b.contains(m), lang == atual_idioma);
                estilo::texto_centro(
                    b.center().x,
                    b.center().y + 6.0 * f,
                    lang.nome(),
                    13,
                    if lang == atual_idioma {
                        estilo::OURO
                    } else {
                        estilo::TEXTO
                    },
                );
                (b, lang)
            })
            .collect();
        estilo::texto(
            rx,
            r.y + 333.0 * f,
            "The choice is saved on this device.",
            12,
            estilo::SUAVE,
        );
        if !clicou {
            return None;
        }
        if let Some(&(_, lang)) = idiomas.iter().find(|(b, _)| b.contains(m)) {
            return (lang != atual_idioma).then_some(Mudanca::Idioma(lang));
        }
        if agora.contains(m) {
            self.fechar();
            return Some(Mudanca::EconomiaAgora);
        }
        if let Some(&(_, min)) = chips.iter().find(|(b, _)| b.contains(m)) {
            return (min != economia_auto).then_some(Mudanca::EconomiaAuto(min));
        }
        let nova = if menos.contains(m) {
            ajusta(atual, -1)
        } else if mais.contains(m) {
            ajusta(atual, 1)
        } else if padrao.contains(m) {
            hud_layout::escala_ui_padrao()
        } else {
            return None;
        };
        ((nova - atual).abs() > 1e-3).then_some(Mudanca::Escala(nova))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn passos_de_dez_por_cento_na_faixa() {
        assert!((ajusta(1.0, 1) - 1.1).abs() < 1e-5);
        assert!((ajusta(1.3, -2) - 1.1).abs() < 1e-5);
        assert_eq!(
            ajusta(hud_layout::ESCALA_UI_MAX, 1),
            hud_layout::ESCALA_UI_MAX
        );
        assert_eq!(
            ajusta(hud_layout::ESCALA_UI_MIN, -1),
            hud_layout::ESCALA_UI_MIN
        );
    }
}
