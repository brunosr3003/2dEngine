use std::collections::HashMap;

use macroquad::prelude::*;

use crate::hud_estilo as estilo;

pub struct Recompensas {
    titulo: String,
    detalhe: String,
    itens: Vec<(u16, u32)>,
    cobre: u32,
    xp: u64,
    faccao: u32,
    ate: f64,
}

#[derive(Default)]
pub struct Ui {
    atual: Option<Recompensas>,
}

impl Ui {
    /// The result holds the entrance until it is closed. The touch that closes
    /// the panel must not pass through it and also become a click on the ground.
    pub fn captura_entrada(&self) -> bool {
        self.atual.as_ref().is_some_and(|r| get_time() <= r.ate)
    }

    pub fn mostrar(&mut self, titulo: String, detalhe: String, itens: Vec<(u16, u32)>, cobre: u32, xp: u64, faccao: u32) {
        self.atual = Some(Recompensas { titulo, detalhe, itens, cobre, xp, faccao, ate: get_time() + 14.0 });
    }

    pub fn desenhar(&mut self, nomes: &HashMap<u16, String>, palco: Option<(&crate::vox::VoxCache, &macroquad::material::Material)>) {
        let Some(r) = &self.atual else { return; };
        if get_time() > r.ate { self.atual = None; return; }
        let f = estilo::fator_texto();
        let seguro = crate::hud_layout::tela_segura();
        let w = (610.0 * f).min(seguro.w - 20.0);
        let cols = ((w / (130.0 * f)).floor() as usize).clamp(2, 4);
        let linhas = r.itens.len().min(12).div_ceil(cols);
        let h = (158.0 + linhas as f32 * 88.0) * f;
        let h = h.min(seguro.h - 20.0);
        let boxr = Rect::new(seguro.center().x - w * 0.5, seguro.center().y - h * 0.5, w, h);
        estilo::painel_destaque(boxr, estilo::OURO);
        estilo::texto_centro_forte(boxr.center().x, boxr.y + 34.0 * f, &r.titulo, 22, estilo::OURO);
        estilo::texto_centro(boxr.center().x, boxr.y + 58.0 * f, &r.detalhe, 13, estilo::SUAVE);
        let mut y = boxr.y + 83.0 * f;
        if r.xp > 0 || r.cobre > 0 || r.faccao > 0 {
            let mut partes = Vec::new();
            if r.xp > 0 { partes.push(format!("+{} XP", r.xp)); }
            if r.cobre > 0 { partes.push(format!("+{} cobre", r.cobre)); }
            if r.faccao > 0 { partes.push(format!("+{} faction", r.faccao)); }
            estilo::texto_centro(boxr.center().x, y, &partes.join("   ·   "), 14, estilo::AUTO);
            y += 28.0 * f;
        }
        let card_w = (w - 38.0 * f) / cols as f32;
        for (i, (id, qtd)) in r.itens.iter().take(12).enumerate() {
            let x = boxr.x + 19.0 * f + (i % cols) as f32 * card_w;
            let cy = y + (i / cols) as f32 * 88.0 * f;
            if cy + 78.0 * f > boxr.y + boxr.h - 30.0 * f { break; }
            let ir = Rect::new(x + (card_w - 48.0 * f) * 0.5, cy, 48.0 * f, 48.0 * f);
            crate::icones::icone_com_3d(*id, ir, None, Some(*qtd), palco);
            let nome = nomes.get(id).map(String::as_str).unwrap_or("Item");
            let nome = if nome.chars().count() > 18 { format!("{}…", nome.chars().take(17).collect::<String>()) } else { nome.to_string() };
            estilo::texto_centro(x + card_w * 0.5, cy + 66.0 * f, &nome, 11, estilo::TEXTO);
        }
        let fechar = Rect::new(boxr.x + boxr.w - 42.0 * f, boxr.y + 8.0 * f, 32.0 * f, 32.0 * f);
        let mouse = Vec2::from(mouse_position());
        let hover = fechar.contains(mouse);
        estilo::botao(fechar, "×", estilo::estado(hover, hover && is_mouse_button_down(MouseButton::Left), false, false), false);
        if hover && crate::foco::clique() { self.atual = None; }
    }
}
