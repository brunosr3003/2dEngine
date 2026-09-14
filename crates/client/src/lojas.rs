//! "Vendedores" (Menu → Comércio): os vendedores NPC da ilha, so' com "Ir".
//!
//! Decisao do usuario: vendedor NPC NUNCA vende de longe — sem compra remota e
//! sem taxa. O menu so' leva ate' ele: o personagem vai sozinho e a loja abre
//! ao falar com ele. A "Loja" do Menu e' outra coisa: a de cash (TP), em breve.
use macroquad::prelude::*;

use crate::hud_estilo as estilo;

const LARGURA: f32 = 520.0;
const LINHA: f32 = 54.0;

#[derive(Default)]
pub struct Lojas {
    pub aberto: bool,
}

impl Lojas {
    pub fn abrir(&mut self) {
        self.aberto = true;
    }

    pub fn fechar(&mut self) {
        self.aberto = false;
    }

    fn painel(n: usize) -> Rect {
        let h = (96.0 + LINHA * n.max(1) as f32).min(screen_height() - 60.0);
        let w = LARGURA.min(screen_width() - 40.0);
        Rect::new((screen_width() - w) * 0.5, (screen_height() - h) * 0.5, w, h)
    }

    pub fn pega_mouse(&self) -> bool {
        self.aberto
    }

    /// Desenha e devolve o vendedor escolhido com "Ir": (nome, posicao).
    pub fn desenha(&mut self, lojas: &[(String, Vec2)], eu: Option<Vec2>) -> Option<(String, Vec2)> {
        if !self.aberto {
            return None;
        }
        crate::hud_layout::escurece(0.55);
        let p = Self::painel(lojas.len());
        estilo::painel(p);
        estilo::texto(p.x + 18.0, p.y + 32.0, "Vendedores da ilha", 22, estilo::OURO);
        estilo::texto_ajustado("Só se compra falando com o vendedor: \"Ir\" leva você até ele.", p.x + 18.0, p.y + 54.0, p.w - 36.0, 13, estilo::SUAVE);
        if crate::ui::botao(Rect::new(p.x + p.w - 44.0, p.y + 10.0, 32.0, 28.0), "x", true) {
            self.aberto = false;
            return None;
        }
        if lojas.is_empty() {
            estilo::texto(p.x + 18.0, p.y + 92.0, "Nenhum vendedor nesta ilha.", 15, estilo::SUAVE);
            return None;
        }
        let mut saida = None;
        for (i, (nome, pos)) in lojas.iter().enumerate() {
            let y = p.y + 70.0 + i as f32 * LINHA;
            if y + LINHA > p.y + p.h {
                break;
            }
            let linha = Rect::new(p.x + 10.0, y, p.w - 20.0, LINHA - 6.0);
            estilo::painel(linha);
            estilo::texto_ajustado(nome, linha.x + 12.0, linha.y + 22.0, linha.w - 120.0, 17, estilo::TEXTO);
            let dist = eu.map_or("—".to_string(), |e| format!("{:.0} m", e.distance(*pos)));
            estilo::texto(linha.x + 12.0, linha.y + 40.0, &dist, 13, estilo::SUAVE);
            if crate::ui::botao(Rect::new(linha.x + linha.w - 84.0, linha.y + 9.0, 72.0, 30.0), "Ir", true) {
                saida = Some((nome.clone(), *pos));
            }
        }
        if saida.is_some() {
            self.aberto = false;
        }
        saida
    }
}
