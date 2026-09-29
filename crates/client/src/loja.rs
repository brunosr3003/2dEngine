//! NPC shop: clicking a vendor opens it; touching an item SELECTS it, and the
//! block below says how many to buy (-/+, 1, 10, 50, Max) and what the batch
//! costs. Price, copper, bag space and range are still validated by the
//! server (`ShopComprar`, all or nothing).
use crate::hud_estilo as estilo;
use macroquad::prelude::*;
use shared::protocol::{ClientMessage, ShopItem};
use shared::{EntityId, InventorySlot};
use std::collections::HashMap;

/// Get this close to the NPC before asking. A little inside `Interact`'s
/// range, so a rounding of position does not make the server refuse.
pub const PERTO: f32 = shared::INTERACT_RADIUS * 0.9;
/// Beyond this the shop closes itself: the server would refuse the purchase anyway.
const FECHA_LONGE: f32 = shared::INTERACT_RADIUS + 1.5;

/// A click on a distant NPC: the character walks there and interacts on arrival.
#[derive(Default)]
pub struct Pendente {
    pub npc: Option<EntityId>,
}

impl Pendente {
    pub fn ir(&mut self, id: EntityId) {
        self.npc = Some(id);
    }
    pub fn cancela(&mut self) {
        self.npc = None;
    }

    /// Every frame. Got close: returns the NPC once, to interact.
    /// The NPC vanished: gives up.
    pub fn acompanhar(&mut self, eu: Vec2, npc: Option<Vec2>) -> Option<EntityId> {
        let id = self.npc?;
        let Some(p) = npc else {
            self.npc = None;
            return None;
        };
        if eu.distance(p) <= PERTO {
            self.npc = None;
            return Some(id);
        }
        None
    }
}

/// Largest batch at once (the same cap as the server's).
pub const MAX_LOTE: u32 = 999;
/// Atalhos de quantidade do seletor.
const ATALHOS: [u32; 3] = [1, 10, 50];
const LARGURA: f32 = 380.0;
const LINHA: f32 = 50.0;
/// Altura do bloco de compra (item escolhido, seletor, total, botao).
const DETALHE: f32 = 196.0;

#[derive(Default)]
pub struct Loja {
    /// O NPC da loja aberta. `None` = fechada.
    pub vendedor: Option<EntityId>,
    itens: Vec<ShopItem>,
    moeda_magica: bool,
    /// The chosen row and how much of it goes in the batch.
    sel: usize,
    qtd: u32,
    /// (text, succeeded, when).
    aviso: Option<(String, bool, f64)>,
    rolagem: crate::rolagem::Rolagem,
}

/// How many the copper pays for, at the batch cap (at least 1, so the button exists).
pub fn maximo(cobre: u64, preco: u32) -> u32 {
    if preco == 0 {
        return MAX_LOTE;
    }
    ((cobre / preco as u64).min(MAX_LOTE as u64) as u32).max(1)
}

/// Equipment goes one at a time (it may go straight onto the body); the rest in a batch.
pub fn em_lote(item_id: u16) -> bool {
    shared::equip_slot_of(item_id).is_none()
}

/// How much of `id` (stacked) the bag holds.
fn na_bolsa(slots: &[InventorySlot], id: u16) -> u64 {
    slots
        .iter()
        .filter(|s| s.item_id == id && s.qty > 0)
        .map(|s| s.qty as u64)
        .sum()
}

impl Loja {
    pub fn aberta(&self) -> bool {
        self.vendedor.is_some()
    }

    pub fn abre(&mut self, vendor_id: u32, itens: Vec<ShopItem>) {
        *self = Self {
            vendedor: Some(EntityId(vendor_id)),
            itens,
            qtd: 1,
            ..Self::default()
        };
    }

    pub fn abre_magica(&mut self, vendor_id: u32, itens: Vec<ShopItem>) {
        self.abre(vendor_id, itens);
        self.moeda_magica = true;
    }

    pub fn fecha(&mut self) {
        *self = Self::default();
    }

    pub fn avisa(&mut self, s: String) {
        self.aviso = Some((s, false, get_time()));
    }

    /// The batch went through: says how many and returns the selector to 1.
    pub fn sucesso(&mut self, s: String) {
        self.aviso = Some((s, true, get_time()));
        self.qtd = 1;
    }

    /// Closes if the vendor vanished or the character walked away from them.
    pub fn conferir_distancia(&mut self, eu: Option<Vec2>, npc: Option<Vec2>) {
        if !self.aberta() {
            return;
        }
        match (eu, npc) {
            (Some(a), Some(b)) if a.distance(b) <= FECHA_LONGE => {}
            _ => self.fecha(),
        }
    }

    /// (panel, list). The list shows at most what fits above the purchase block;
    /// the rest scrolls.
    fn escala() -> f32 {
        estilo::escala_do_painel(LARGURA, 640.0)
    }

    fn areas(&self) -> (Rect, Rect) {
        estilo::no_painel(Self::escala(), || self.areas_na_escala())
    }

    fn areas_na_escala(&self) -> (Rect, Rect) {
        let f = estilo::fator_texto();
        let t = crate::hud_layout::tela_segura();
        let fixo = (56.0 + DETALHE + 46.0) * f;
        let lista_h =
            (self.itens.len().max(1) as f32 * LINHA * f).min((t.h - 16.0 - fixo).max(LINHA * f));
        let w = LARGURA * f;
        let h = fixo + lista_h;
        let p = Rect::new(t.center().x - w * 0.5, t.center().y - h * 0.5, w, h);
        let lista = Rect::new(p.x + 8.0 * f, p.y + 52.0 * f, p.w - 16.0 * f, lista_h);
        (p, lista)
    }

    /// With the mouse over the panel the click belongs to the shop, not the world.
    pub fn pega_mouse(&self) -> bool {
        self.aberta() && self.areas().0.contains(Vec2::from(mouse_position()))
    }

    /// Desenha e devolve o pedido de compra do quadro.
    pub fn desenha(
        &mut self,
        nomes: &HashMap<u16, String>,
        slots: &[InventorySlot],
        ouro: u64,
        cobre: u64,
        vendedor: &str,
        palco: Option<(&crate::vox::VoxCache, &Material)>,
    ) -> Vec<ClientMessage> {
        estilo::no_painel(Self::escala(), || {
            self.desenha_na_escala(nomes, slots, ouro, cobre, vendedor, palco)
        })
    }

    fn desenha_na_escala(
        &mut self,
        nomes: &HashMap<u16, String>,
        slots: &[InventorySlot],
        ouro: u64,
        cobre: u64,
        vendedor: &str,
        palco: Option<(&crate::vox::VoxCache, &Material)>,
    ) -> Vec<ClientMessage> {
        if !self.aberta() {
            return Vec::new();
        }
        let saldo = if self.moeda_magica {
            na_bolsa(slots, shared::item_id::MOEDA_MAGICA)
        } else {
            cobre
        };
        let moeda = if self.moeda_magica { "moedas" } else { "cobre" };
        let pacote = |id: u16| -> u32 {
            if self.moeda_magica {
                shared::magica::TROCAS
                    .iter()
                    .find(|&&(i, _, _)| i == id)
                    .map_or(1, |&(_, q, _)| q)
            } else {
                1
            }
        };
        let f = estilo::fator_texto();
        let (p, lista) = self.areas();
        let mouse = Vec2::from(mouse_position());
        let nome_de = |id: u16| {
            nomes
                .get(&id)
                .cloned()
                .unwrap_or_else(|| format!("item {id}"))
        };
        estilo::painel(p);
        estilo::texto_ajustado(
            vendedor,
            p.x + 16.0 * f,
            p.y + 30.0 * f,
            p.w - 70.0 * f,
            22,
            estilo::OURO,
        );
        if crate::ui::botao(
            Rect::new(p.x + p.w - 44.0 * f, p.y + 8.0 * f, 32.0 * f, 28.0 * f),
            "x",
            true,
        ) {
            self.fecha();
            return Vec::new();
        }
        draw_line(
            p.x + 12.0,
            p.y + 44.0 * f,
            p.x + p.w - 12.0,
            p.y + 44.0 * f,
            1.0,
            estilo::BORDA,
        );

        // ── list: touching SELECTS (no more buying by accident) ──
        if self.itens.is_empty() {
            estilo::texto(
                p.x + 16.0 * f,
                lista.y + 28.0 * f,
                "Nothing for sale.",
                16,
                estilo::SUAVE,
            );
        }
        let linha_h = LINHA * f;
        let total = self.itens.len() as f32 * linha_h;
        let toque = self.rolagem.quadro(lista, total, linha_h);
        let arrastando = self.rolagem.arrastando();
        self.sel = self.sel.min(self.itens.len().saturating_sub(1));
        crate::rolagem::recortar(Some(lista));
        for (i, item) in self.itens.iter().enumerate() {
            let r = Rect::new(
                lista.x,
                lista.y + i as f32 * linha_h - self.rolagem.pos,
                lista.w - 12.0,
                linha_h - 4.0 * f,
            );
            if r.y + r.h < lista.y || r.y > lista.y + lista.h {
                continue;
            }
            let marcada = i == self.sel;
            let sobre = !arrastando && r.contains(mouse) && lista.contains(mouse);
            if marcada || sobre {
                draw_rectangle(
                    r.x,
                    r.y,
                    r.w,
                    r.h,
                    Color::new(1.0, 1.0, 1.0, if marcada { 0.10 } else { 0.05 }),
                );
            }
            if marcada {
                draw_rectangle_lines(r.x, r.y, r.w, r.h, 1.5, estilo::OURO);
            }
            let lado = r.h - 6.0 * f;
            crate::bolsa::icone_do_item_com(
                Rect::new(r.x + 4.0 * f, r.y + 3.0 * f, lado, lado),
                item.item_id,
                1.0,
                palco,
            );
            let nome = format!("{}x {}", pacote(item.item_id), nome_de(item.item_id));
            estilo::texto_ajustado(
                &nome,
                r.x + lado + 14.0 * f,
                r.y + r.h * 0.62,
                r.w - lado - 130.0 * f,
                16,
                estilo::TEXTO,
            );
            let preco = format!("{} {moeda}", crate::bolsa::milhar(item.price as u64));
            let cor = if saldo >= item.price as u64 {
                COBRE
            } else {
                VERMELHO
            };
            estilo::texto(
                r.x + r.w - 10.0 * f - estilo::medir(&preco, 15),
                r.y + r.h * 0.62,
                &preco,
                15,
                cor,
            );
            if toque.is_some_and(|c| r.contains(c) && lista.contains(c)) && i != self.sel {
                self.sel = i;
                self.qtd = 1;
            }
        }
        crate::rolagem::recortar(None);
        self.rolagem.desenha(lista, total);

        // ── compra: quanto, total e o botao ──
        let mut saida = Vec::new();
        let d = Rect::new(
            p.x + 12.0 * f,
            lista.y + lista.h + 8.0 * f,
            p.w - 24.0 * f,
            DETALHE * f,
        );
        draw_line(d.x, d.y, d.x + d.w, d.y, 1.0, estilo::BORDA);
        if let Some(item) = self.itens.get(self.sel).cloned() {
            let lote = em_lote(item.item_id);
            let max = if lote { maximo(saldo, item.price) } else { 1 };
            self.qtd = self.qtd.clamp(1, max.max(1));
            let tem = na_bolsa(slots, item.item_id);
            let titulo = format!("{}x {}", pacote(item.item_id), nome_de(item.item_id));
            estilo::texto_ajustado(
                &titulo,
                d.x + 4.0,
                d.y + 24.0 * f,
                d.w * 0.62,
                17,
                estilo::OURO,
            );
            let info = format!("in your bag: {}", crate::bolsa::milhar(tem));
            estilo::texto(
                d.x + d.w - estilo::medir(&info, 14),
                d.y + 24.0 * f,
                &info,
                14,
                estilo::SUAVE,
            );

            // Selector: [-]  qty  [+]   [1] [10] [50] [Max]
            let y = d.y + 38.0 * f;
            let h = 36.0 * f;
            let menos = Rect::new(d.x, y, 40.0 * f, h);
            let caixa = Rect::new(menos.x + menos.w + 4.0 * f, y, 64.0 * f, h);
            let mais = Rect::new(caixa.x + caixa.w + 4.0 * f, y, 40.0 * f, h);
            if crate::ui::botao(menos, "-", lote && self.qtd > 1) {
                self.qtd -= 1;
            }
            estilo::ret_arredondado(caixa, 6.0, estilo::alfa(estilo::FUNDO_BAIXO, 0.9));
            estilo::texto_centro(
                caixa.center().x,
                caixa.y + h * 0.68,
                &self.qtd.to_string(),
                19,
                estilo::TEXTO,
            );
            if crate::ui::botao(mais, "+", lote && self.qtd < max) {
                self.qtd += 1;
            }
            let mut x = mais.x + mais.w + 10.0 * f;
            let w = ((d.x + d.w - x) - 3.0 * 4.0 * f) / 4.0;
            for n in ATALHOS {
                if crate::ui::botao(Rect::new(x, y, w, h), &n.to_string(), lote && n <= max) {
                    self.qtd = n;
                }
                x += w + 4.0 * f;
            }
            if crate::ui::botao(Rect::new(x, y, w, h), "Max", lote && max > 1) {
                self.qtd = max;
            }

            // The batch total and what is left.
            let total_lote = item.price as u64 * self.qtd as u64;
            let falta = total_lote > saldo;
            let t = format!("Total: {} {moeda}", crate::bolsa::milhar(total_lote));
            estilo::texto(
                d.x + 4.0,
                d.y + 104.0 * f,
                &t,
                17,
                if falta { VERMELHO } else { COBRE },
            );
            let resto = if falta {
                format!("faltam {}", crate::bolsa::milhar(total_lote - saldo))
            } else {
                format!("sobra {}", crate::bolsa::milhar(saldo - total_lote))
            };
            estilo::texto(
                d.x + d.w - estilo::medir(&resto, 14),
                d.y + 104.0 * f,
                &resto,
                14,
                estilo::SUAVE,
            );
            if !lote {
                estilo::texto(
                    d.x + 4.0,
                    d.y + 124.0 * f,
                    "Gear: one at a time.",
                    13,
                    estilo::SUAVE,
                );
            }
            let b = Rect::new(d.x, d.y + 134.0 * f, d.w, 44.0 * f);
            let rotulo = if self.moeda_magica {
                format!("Trade for {}x", self.qtd)
            } else {
                format!("Buy {}x", self.qtd)
            };
            if crate::ui::botao(b, &rotulo, !falta && !self.itens.is_empty()) {
                saida.push(ClientMessage::ShopComprar {
                    slot_idx: self.sel as u8,
                    qtd: self.qtd as u16,
                });
            }
        }

        // ── rodape: saldo ──
        let rodape = p.y + p.h - 38.0 * f;
        draw_line(
            p.x + 12.0,
            rodape,
            p.x + p.w - 12.0,
            rodape,
            1.0,
            estilo::BORDA,
        );
        let rodape_texto = if self.moeda_magica {
            format!("Magic Coins: {}", crate::bolsa::milhar(saldo))
        } else {
            format!(
                "Copper {}  ·  Gold {}",
                crate::bolsa::milhar(cobre),
                crate::bolsa::milhar(ouro)
            )
        };
        estilo::texto(
            p.x + 16.0 * f,
            rodape + 25.0 * f,
            &rodape_texto,
            15,
            estilo::OURO,
        );
        if let Some((msg, ok, quando)) = &self.aviso {
            if get_time() - quando < 3.5 {
                estilo::texto_ajustado(
                    msg,
                    p.x + 16.0,
                    p.y + p.h + 22.0 * f,
                    p.w - 32.0,
                    15,
                    if *ok { VERDE } else { VERMELHO },
                );
            } else {
                self.aviso = None;
            }
        }
        saida
    }
}

const COBRE: Color = Color::new(0.85, 0.55, 0.32, 1.0);
const VERMELHO: Color = Color::new(0.88, 0.38, 0.32, 1.0);
const VERDE: Color = Color::new(0.45, 0.80, 0.42, 1.0);

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
        l.abre(
            3,
            vec![ShopItem {
                item_id: 2,
                price: 10,
            }],
        );
        l.conferir_distancia(Some(vec2(0.0, 0.0)), Some(vec2(4.0, 0.0)));
        assert!(l.aberta());
        l.conferir_distancia(Some(vec2(0.0, 0.0)), Some(vec2(5.0, 0.0)));
        assert!(!l.aberta());
    }

    #[test]
    fn maximo_e_o_que_o_cobre_paga_no_teto_do_lote() {
        assert_eq!(maximo(95, 10), 9);
        assert_eq!(maximo(5, 10), 1, "o botao existe; o total fica vermelho");
        assert_eq!(maximo(1_000_000, 10), MAX_LOTE);
        assert!(em_lote(shared::item_id::HEALTH_POTION));
        assert!(!em_lote(shared::item_id::KATANA));
    }

    #[test]
    fn abrir_escolhe_o_primeiro_com_um_no_seletor() {
        let mut l = Loja::default();
        l.abre(
            3,
            vec![ShopItem {
                item_id: 2,
                price: 10,
            }],
        );
        assert_eq!((l.sel, l.qtd), (0, 1));
        l.fecha();
        assert!(!l.aberta());
    }
}
