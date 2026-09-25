//! Escolha de caminhos de XP quando a história espera um nível mínimo.
use macroquad::prelude::*;

use crate::hud_estilo as estilo;

#[derive(Clone, Copy)]
pub enum Escolha {
    IlhaMagica,
    Missoes,
    Caca,
}

#[derive(Default)]
pub struct NivelUi {
    alvo: Option<u32>,
}

impl NivelUi {
    pub fn abrir(&mut self, alvo: u32) {
        self.alvo = Some(alvo);
    }

    pub fn captura_entrada(&self) -> bool {
        self.alvo.is_some()
    }

    pub fn desenhar(&mut self) -> Option<Escolha> {
        let alvo = self.alvo?;
        let f = estilo::fator_texto();
        let seguro = crate::hud_layout::tela_segura();
        let w = (620.0 * f).min(seguro.w - 16.0);
        let h = (380.0 * f).min(seguro.h - 16.0);
        let r = Rect::new(seguro.center().x - w * 0.5, seguro.center().y - h * 0.5, w, h);
        estilo::painel_destaque(r, estilo::OURO);
        estilo::texto_centro_forte(r.center().x, r.y + 34.0 * f, &format!("CAMINHOS PARA O NÍVEL {alvo}"), 20, estilo::OURO);
        estilo::texto_centro(r.center().x, r.y + 58.0 * f, "Escolha como ganhar XP para continuar a história.", 13, estilo::SUAVE);
        let mouse = Vec2::from(mouse_position());
        let fechar = Rect::new(r.x + r.w - 40.0 * f, r.y + 8.0 * f, 30.0 * f, 30.0 * f);
        if botao(fechar, "×", mouse) {
            self.alvo = None;
            return None;
        }
        let opcoes = [
            (Escolha::IlhaMagica, "ILHA MÁGICA", "PvP nas ilhotas de combate; XP e coleta até 2× nas ilhotas de bônus."),
            (Escolha::Missoes, "MISSÕES SECUNDÁRIAS", "XP, equipamentos e recursos. Toque Pegar e depois Ir para usar auto missão."),
            (Escolha::Caca, "CAÇAR EM ÁREAS DENSAS", "No mapa, toque um círculo FORTE. Ir leva até lá e liga o auto combate."),
        ];
        let gap = 8.0 * f;
        let top = r.y + 77.0 * f;
        let usable = (r.y + r.h - 50.0 * f - top).max(0.0);
        let card_h = ((usable - 2.0 * gap) / 3.0).max(45.0 * f);
        for (i, (escolha, titulo, dica)) in opcoes.into_iter().enumerate() {
            let c = Rect::new(r.x + 16.0 * f, top + i as f32 * (card_h + gap), r.w - 32.0 * f, card_h);
            let sobre = c.contains(mouse);
            estilo::ret_arredondado(c, 8.0 * f, Color::new(0.18, 0.22, 0.29, if sobre { 0.95 } else { 0.78 }));
            estilo::texto_forte(c.x + 14.0 * f, c.y + 22.0 * f, titulo, 15, estilo::OURO);
            estilo::texto_ajustado(dica, c.x + 14.0 * f, c.y + 42.0 * f, c.w - 26.0 * f, 12, estilo::TEXTO);
            if sobre && crate::foco::clique() {
                self.alvo = None;
                return Some(escolha);
            }
        }
        let sair = Rect::new(r.center().x - 52.0 * f, r.y + r.h - 40.0 * f, 104.0 * f, 29.0 * f);
        if botao(sair, "Fechar", mouse) {
            self.alvo = None;
        }
        None
    }
}

fn botao(r: Rect, texto: &str, mouse: Vec2) -> bool {
    let sobre = r.contains(mouse);
    estilo::botao(r, texto, estilo::estado(sobre, sobre && is_mouse_button_down(MouseButton::Left), false, false), false);
    sobre && crate::foco::clique()
}
