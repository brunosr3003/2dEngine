//! Loja de NPC: clicar num vendedor abre, clicar num item compra.
//! Preco, ouro, espaco na bolsa e alcance continuam validados pelo servidor.
use macroquad::prelude::*;
use shared::protocol::{ClientMessage, ShopItem};
use shared::EntityId;
use std::collections::HashMap;
use crate::hud_estilo as estilo;

/// Chega a isto do NPC antes de pedir. Um pouco dentro do alcance do
/// `Interact`, pra arredondamento de posicao nao fazer o servidor recusar.
pub const PERTO: f32 = shared::INTERACT_RADIUS * 0.9;
/// Longe disto a loja fecha sozinha: o servidor ja' recusaria a compra.
const FECHA_LONGE: f32 = shared::INTERACT_RADIUS + 1.5;
/// Shift+clique compra este tanto.
const LOTE: usize = 5;
const LARGURA: f32 = 360.0;
const LINHA: f32 = 52.0;

/// Clique num NPC longe: o personagem anda ate' ele e interage ao chegar.
#[derive(Default)]
pub struct Pendente {
    pub npc: Option<EntityId>,
}

impl Pendente {
    pub fn ir(&mut self, id: EntityId) { self.npc = Some(id); }
    pub fn cancela(&mut self) { self.npc = None; }

    /// A cada quadro. Chegou perto: devolve o NPC uma vez, pra interagir.
    /// O NPC sumiu: desiste.
    pub fn acompanhar(&mut self, eu: Vec2, npc: Option<Vec2>) -> Option<EntityId> {
        let id = self.npc?;
        let Some(p) = npc else { self.npc = None; return None };
        if eu.distance(p) <= PERTO {
            self.npc = None;
            return Some(id);
        }
        None
    }
}

#[derive(Default)]
pub struct Loja {
    /// O NPC da loja aberta. `None` = fechada.
    pub vendedor: Option<EntityId>,
    itens: Vec<ShopItem>,
    aviso: Option<(String, f64)>,
}

/// Os pedidos de um clique: um `ShopBuy` por unidade. O servidor compra uma
/// por mensagem e revalida cada uma — ouro acabou no meio, as outras falham.
pub fn pedidos(slot: usize, qtd: usize) -> Vec<ClientMessage> {
    (0..qtd).map(|_| ClientMessage::ShopBuy { slot_idx: slot as u8 }).collect()
}

impl Loja {
    pub fn aberta(&self) -> bool { self.vendedor.is_some() }

    pub fn abre(&mut self, vendor_id: u32, itens: Vec<ShopItem>) {
        self.vendedor = Some(EntityId(vendor_id));
        self.itens = itens;
        self.aviso = None;
    }

    pub fn fecha(&mut self) { *self = Self::default(); }

    pub fn avisa(&mut self, s: String) { self.aviso = Some((s, get_time())); }

    /// Fecha se o vendedor sumiu ou o personagem se afastou dele.
    pub fn conferir_distancia(&mut self, eu: Option<Vec2>, npc: Option<Vec2>) {
        if !self.aberta() { return; }
        match (eu, npc) {
            (Some(a), Some(b)) if a.distance(b) <= FECHA_LONGE => {}
            _ => self.fecha(),
        }
    }

    fn painel(&self) -> Rect {
        let h = 118.0 + self.itens.len().max(1) as f32 * LINHA;
        Rect::new(crate::hud_layout::tela_segura().x + 24.0, screen_height() * 0.16, LARGURA, h)
    }

    /// Com o mouse em cima do painel o clique e' da loja, e nao do mundo.
    pub fn pega_mouse(&self) -> bool {
        self.aberta() && self.painel().contains(Vec2::from(mouse_position()))
    }

    /// Desenha e devolve os pedidos de compra do quadro.
    pub fn desenha(&mut self, nomes: &HashMap<u16, String>, ouro: u64, vendedor: &str) -> Vec<ClientMessage> {
        if !self.aberta() { return Vec::new(); }
        let p = self.painel();
        let mouse = Vec2::from(mouse_position());
        let clique = is_mouse_button_pressed(MouseButton::Left);
        estilo::painel(p);
        estilo::texto_ajustado(vendedor, p.x + 16.0, p.y + 30.0, p.w - 70.0, 22, estilo::OURO);
        if crate::ui::botao(Rect::new(p.x + p.w - 44.0, p.y + 8.0, 32.0, 28.0), "x", true) {
            self.fecha();
            return Vec::new();
        }
        draw_line(p.x + 12.0, p.y + 44.0, p.x + p.w - 12.0, p.y + 44.0, 1.0, estilo::BORDA);

        let mut saida = Vec::new();
        if self.itens.is_empty() {
            estilo::texto(p.x + 16.0, p.y + 78.0, "Nada à venda.", 16, estilo::SUAVE);
        }
        for (i, item) in self.itens.iter().enumerate() {
            let r = Rect::new(p.x + 10.0, p.y + 52.0 + i as f32 * LINHA, p.w - 20.0, LINHA - 6.0);
            let sobre = r.contains(mouse);
            let pode = ouro >= item.price as u64;
            if sobre {
                draw_rectangle(r.x, r.y, r.w, r.h, Color::new(1.0, 1.0, 1.0, 0.06));
                draw_rectangle_lines(r.x, r.y, r.w, r.h, 1.0, estilo::BORDA);
            }
            crate::bolsa::icone_do_item(Rect::new(r.x + 4.0, r.y + 3.0, 40.0, 40.0), item.item_id, 1.0);
            let nome = nomes.get(&item.item_id).cloned().unwrap_or_else(|| format!("item {}", item.item_id));
            estilo::texto_ajustado(&nome, r.x + 54.0, r.y + 29.0, r.w - 150.0, 17, estilo::TEXTO);
            let preco = format!("{} ouro", crate::bolsa::milhar(item.price as u64));
            let cor = if pode { estilo::OURO } else { Color::new(0.88, 0.38, 0.32, 1.0) };
            estilo::texto(r.x + r.w - 10.0 - estilo::medir(&preco, 16), r.y + 29.0, &preco, 16, cor);
            if sobre && clique {
                let qtd = if is_key_down(KeyCode::LeftShift) || is_key_down(KeyCode::RightShift) { LOTE } else { 1 };
                saida = pedidos(i, qtd);
            }
        }
        let rodape = p.y + p.h - 40.0;
        draw_line(p.x + 12.0, rodape, p.x + p.w - 12.0, rodape, 1.0, estilo::BORDA);
        estilo::texto(p.x + 16.0, rodape + 24.0, &format!("Ouro  {}", crate::bolsa::milhar(ouro)), 17, estilo::OURO);
        let dica = "clique: 1  ·  Shift: 5";
        estilo::texto(p.x + p.w - 16.0 - estilo::medir(dica, 14), rodape + 23.0, dica, 14, estilo::SUAVE);
        if let Some((msg, quando)) = &self.aviso {
            if get_time() - quando < 3.0 {
                estilo::texto_ajustado(msg, p.x + 16.0, p.y + p.h + 22.0, p.w - 32.0, 15, Color::new(0.88, 0.38, 0.32, 1.0));
            } else {
                self.aviso = None;
            }
        }
        saida
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn interacao_pendente_dispara_uma_vez_ao_chegar() {
        let mut p = Pendente::default();
        p.ir(EntityId(7));
        let npc = Some(vec2(10.0, 0.0));
        assert_eq!(p.acompanhar(vec2(0.0, 0.0), npc), None);
        assert_eq!(p.acompanhar(vec2(8.0, 0.0), npc), Some(EntityId(7)));
        assert_eq!(p.acompanhar(vec2(8.0, 0.0), npc), None);
    }

    #[test]
    fn interacao_pendente_desiste_se_o_npc_some() {
        let mut p = Pendente::default();
        p.ir(EntityId(7));
        assert_eq!(p.acompanhar(vec2(0.0, 0.0), None), None);
        assert!(p.npc.is_none());
    }

    #[test]
    fn loja_fecha_longe_do_vendedor() {
        let mut l = Loja::default();
        l.abre(3, vec![ShopItem { item_id: 2, price: 10 }]);
        l.conferir_distancia(Some(vec2(0.0, 0.0)), Some(vec2(4.0, 0.0)));
        assert!(l.aberta());
        l.conferir_distancia(Some(vec2(0.0, 0.0)), Some(vec2(5.0, 0.0)));
        assert!(!l.aberta());
    }

    #[test]
    fn shift_compra_um_lote() {
        assert_eq!(pedidos(2, 1).len(), 1);
        let lote = pedidos(2, LOTE);
        assert_eq!(lote.len(), 5);
        assert!(lote.iter().all(|m| matches!(m, ClientMessage::ShopBuy { slot_idx: 2 })));
    }
}
