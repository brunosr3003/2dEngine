//! The staff announcement banner (`ServerMessage::AvisoGlobal`, typed in the
//! Panopticon): a wide gold strip at the top of the screen, over everything,
//! for `DURACAO` seconds, fading in and out. It never takes a tap — the game
//! under it keeps working. Several in a row queue up.

use macroquad::prelude::*;

use crate::hud_estilo as estilo;

const DURACAO: f64 = 9.0;

#[derive(Default)]
pub struct AvisoGlobal {
    fila: std::collections::VecDeque<String>,
    atual: Option<(String, f64)>,
}

impl AvisoGlobal {
    pub fn receber(&mut self, texto: String) {
        self.fila.push_back(texto);
    }

    pub fn desenha(&mut self, agora: f64) {
        if self.atual.as_ref().is_none_or(|(_, t0)| agora - t0 > DURACAO) {
            self.atual = self.fila.pop_front().map(|t| (t, agora));
        }
        let Some((texto, t0)) = &self.atual else {
            return;
        };
        let t = (agora - t0) as f32;
        let a = (t / 0.35).min(((DURACAO as f32) - t) / 0.6).clamp(0.0, 1.0);
        let seguro = crate::hud_layout::tela_segura();
        let k = (seguro.w / 900.0).min(seguro.h / 420.0).clamp(0.6, 2.4);
        estilo::no_painel(k, || {
            let w = (720.0 * k).min(seguro.w - 16.0);
            let h = 74.0 * k;
            let desce = (1.0 - a) * -20.0 * k;
            let r = Rect::new(seguro.center().x - w * 0.5, seguro.y + 70.0 * k + desce, w, h);
            draw_rectangle(r.x, r.y, r.w, r.h, Color::new(0.07, 0.05, 0.04, 0.92 * a));
            draw_rectangle(r.x, r.y, r.w, 3.0 * k, estilo::alfa(estilo::OURO, a));
            draw_rectangle(r.x, r.y + r.h - 3.0 * k, r.w, 3.0 * k, estilo::alfa(estilo::OURO, a));
            draw_rectangle_lines(r.x, r.y, r.w, r.h, 1.5 * k, estilo::alfa(estilo::OURO, 0.6 * a));
            estilo::texto_centro_forte(
                r.center().x,
                r.y + 24.0 * k,
                "ANNOUNCEMENT",
                13,
                estilo::alfa(estilo::OURO, a),
            );
            estilo::texto_ajustado(
                texto,
                r.x + 16.0 * k,
                r.y + 54.0 * k,
                r.w - 32.0 * k,
                19,
                estilo::alfa(WHITE, a),
            );
        });
    }
}
