//! Configurador da barra de itens, no molde do MIR4: escolhe o consumivel de
//! cada espaco, liga o AUTO e ajusta o limiar. Abre pelo Menu (Sistema →
//! Barra), clicando num espaco vazio ou com o botao direito num espaco —
//! nenhuma tecla abre.
//!
//! Escolher e' dois cliques: um item da lista e um espaco (ou o contrario).
use macroquad::prelude::*;
use shared::InventorySlot;

use crate::barra::{self, Barra, ESPACOS};
use crate::hud_estilo as estilo;

const TECLAS: [&str; ESPACOS] = ["C", "8", "9", "0"];

#[derive(Default)]
pub struct ConfigBarra {
    pub aberto: bool,
    /// Item escolhido na lista, esperando um espaco.
    selecionado: Option<u16>,
    /// Espaco escolhido, esperando um item.
    pub espaco_alvo: Option<usize>,
    /// A lupa de um item pediu o "Onde obter" (main abre o popup).
    pub onde_obter: Option<u16>,
}

/// Consumiveis da bolsa que cabem na barra, sem repetir, com a quantidade.
pub fn consumiveis(slots: &[InventorySlot]) -> Vec<(u16, u32)> {
    let mut v: Vec<(u16, u32)> = Vec::new();
    for s in slots
        .iter()
        .filter(|s| s.qty > 0 && s.instance.is_none() && barra::categoria(s.item_id).is_some())
    {
        match v.iter_mut().find(|(id, _)| *id == s.item_id) {
            Some((_, q)) => *q += s.qty,
            None => v.push((s.item_id, s.qty)),
        }
    }
    v.sort_by_key(|(id, _)| *id);
    v
}

impl ConfigBarra {
    /// `espaco`: ja' abre com esse espaco escolhido (clique num vazio).
    pub fn abrir(&mut self, espaco: Option<usize>) {
        *self = Self {
            aberto: true,
            selecionado: None,
            espaco_alvo: espaco,
            onde_obter: None,
        };
    }

    pub fn fechar(&mut self) {
        *self = Self::default();
    }

    /// Clique num item da lista. Devolve se mudou a barra.
    pub fn escolhe_item(&mut self, barra: &mut Barra, id: u16) -> bool {
        match self.espaco_alvo.take() {
            Some(i) => {
                barra.atribuir(i, id);
                true
            }
            None => {
                self.selecionado = Some(id);
                false
            }
        }
    }

    /// Clique num espaco. Devolve se mudou a barra.
    pub fn escolhe_espaco(&mut self, barra: &mut Barra, i: usize) -> bool {
        match self.selecionado.take() {
            Some(id) => {
                barra.atribuir(i, id);
                true
            }
            None => {
                self.espaco_alvo = if self.espaco_alvo == Some(i) {
                    None
                } else {
                    Some(i)
                };
                false
            }
        }
    }

    /// Desenha e trata o clique. Devolve `true` quando a barra mudou (salvar).
    pub fn desenha(
        &mut self,
        barra: &mut Barra,
        slots: &[InventorySlot],
        nome: &dyn Fn(u16) -> String,
    ) -> bool {
        let (sw, sh) = (screen_width(), screen_height());
        let w = 780.0f32.min(sw - 40.0);
        let h = 470.0f32.min(sh - 40.0);
        let p = Rect::new((sw - w) * 0.5, (sh - h) * 0.5, w, h);
        estilo::painel(p);
        let m = Vec2::from(mouse_position());
        let clique = is_mouse_button_pressed(MouseButton::Left);
        let mut mudou = false;

        estilo::texto(p.x + 20.0, p.y + 34.0, "Barra de itens", 22, estilo::OURO);
        estilo::texto(p.x + 20.0, p.y + 56.0, "Escolha um item e depois um espaço (ou o contrário). Na tela, arraste o botão ↑ pra ligar o AUTO.", 13, estilo::SUAVE);
        let x_fechar = Rect::new(p.x + p.w - 38.0, p.y + 12.0, 26.0, 26.0);
        estilo::painel(x_fechar);
        estilo::texto_centro(
            x_fechar.center().x,
            x_fechar.center().y + 6.0,
            "X",
            16,
            estilo::TEXTO,
        );
        if clique && x_fechar.contains(m) {
            self.fechar();
            return false;
        }

        // ── espacos ──
        let col = (p.w * 0.56).min(440.0);
        for i in 0..ESPACOS {
            let linha = Rect::new(p.x + 16.0, p.y + 74.0 + i as f32 * 94.0, col - 16.0, 86.0);
            let alvo = self.espaco_alvo == Some(i);
            estilo::painel(linha);
            if alvo {
                draw_rectangle_lines(linha.x, linha.y, linha.w, linha.h, 2.0, estilo::OURO);
            }
            let esp = barra.espacos[i];
            let caixa = Rect::new(linha.x + 10.0, linha.y + 11.0, 64.0, 64.0);
            estilo::painel(caixa);
            if esp.item_id == 0 {
                if !crate::icones_ui::ui("mais", caixa.center(), 28.0, estilo::SUAVE) {
                    estilo::texto_centro(
                        caixa.center().x,
                        caixa.center().y + 8.0,
                        "+",
                        26,
                        estilo::SUAVE,
                    );
                }
            } else {
                let q = barra::quantidade(slots, esp.item_id);
                crate::bolsa::icone_do_item(
                    Rect::new(caixa.x + 6.0, caixa.y + 6.0, 52.0, 52.0),
                    esp.item_id,
                    if q > 0 { 1.0 } else { 0.35 },
                );
                estilo::texto(
                    caixa.x + caixa.w - 20.0,
                    caixa.y + caixa.h - 4.0,
                    &q.to_string(),
                    12,
                    estilo::TEXTO,
                );
            }
            estilo::texto(caixa.x + 4.0, caixa.y - 1.0, TECLAS[i], 11, estilo::SUAVE);
            if clique && caixa.contains(m) {
                mudou |= self.escolhe_espaco(barra, i);
            }
            let tx = caixa.x + caixa.w + 12.0;
            if esp.item_id == 0 {
                estilo::texto(
                    tx,
                    linha.y + 38.0,
                    if alvo {
                        "Escolha um item na lista →"
                    } else {
                        "Vazio"
                    },
                    15,
                    estilo::SUAVE,
                );
                continue;
            }
            estilo::texto(tx, linha.y + 26.0, &nome(esp.item_id), 15, estilo::TEXTO);
            // Lupa: onde conseguir mais desse consumivel.
            if crate::onde_obter::botao(Rect::new(
                linha.x + linha.w - 44.0,
                linha.y + 44.0,
                34.0,
                34.0,
            )) {
                self.onde_obter = Some(esp.item_id);
            }
            let cat = barra::categoria(esp.item_id);
            // AUTO liga/desliga.
            let b_auto = Rect::new(tx, linha.y + 38.0, 118.0, 26.0);
            estilo::painel(b_auto);
            let (txt, cor) = if esp.auto {
                ("AUTO · ligado", estilo::AUTO)
            } else {
                ("AUTO · desligado", estilo::SUAVE)
            };
            estilo::texto_centro(b_auto.center().x, b_auto.center().y + 5.0, txt, 13, cor);
            if clique && b_auto.contains(m) {
                barra.alterna_auto(i);
                mudou = true;
            }
            // Limiar so' pra recurso.
            if let Some(c) = cat {
                if c.limiar_padrao().is_some() {
                    let menos = Rect::new(b_auto.x + b_auto.w + 10.0, b_auto.y, 26.0, 26.0);
                    let mais = Rect::new(menos.x + 86.0, b_auto.y, 26.0, 26.0);
                    for (r, s) in [(menos, "−"), (mais, "+")] {
                        estilo::painel(r);
                        estilo::texto_centro(
                            r.center().x,
                            r.center().y + 6.0,
                            s,
                            16,
                            estilo::TEXTO,
                        );
                    }
                    estilo::texto_centro(
                        menos.x + 56.0,
                        b_auto.center().y + 5.0,
                        &format!("< {}%", esp.limiar),
                        14,
                        estilo::OURO,
                    );
                    if clique && menos.contains(m) {
                        barra.ajustar_limiar(i, -5);
                        mudou = true;
                    }
                    if clique && mais.contains(m) {
                        barra.ajustar_limiar(i, 5);
                        mudou = true;
                    }
                }
                estilo::texto(
                    tx,
                    linha.y + 80.0,
                    &format!("AUTO usa com {}", c.regra()),
                    12,
                    estilo::SUAVE,
                );
            }
            let limpar = Rect::new(linha.x + linha.w - 74.0, linha.y + 10.0, 64.0, 24.0);
            estilo::painel(limpar);
            estilo::texto_centro(
                limpar.center().x,
                limpar.center().y + 5.0,
                "Limpar",
                12,
                estilo::TEXTO,
            );
            if clique && limpar.contains(m) {
                barra.limpar(i);
                mudou = true;
            }
        }

        // ── consumiveis da bolsa ──
        let lx = p.x + col + 12.0;
        let lw = p.w - col - 28.0;
        estilo::texto(lx, p.y + 90.0, "Consumíveis na bolsa", 15, estilo::OURO);
        let lista = consumiveis(slots);
        if lista.is_empty() {
            estilo::texto(
                lx,
                p.y + 120.0,
                "Nenhum consumível na bolsa.",
                13,
                estilo::SUAVE,
            );
        }
        for (k, (id, q)) in lista.iter().enumerate() {
            let r = Rect::new(lx, p.y + 102.0 + k as f32 * 50.0, lw, 44.0);
            if r.y + r.h > p.y + p.h - 10.0 {
                break;
            }
            estilo::painel(r);
            if self.selecionado == Some(*id) {
                draw_rectangle_lines(r.x, r.y, r.w, r.h, 2.0, estilo::OURO);
            }
            crate::bolsa::icone_do_item(Rect::new(r.x + 4.0, r.y + 4.0, 36.0, 36.0), *id, 1.0);
            estilo::texto(r.x + 48.0, r.y + 27.0, &nome(*id), 14, estilo::TEXTO);
            let lupa = Rect::new(r.x + r.w - 40.0, r.y + 5.0, 34.0, 34.0);
            let t = format!("×{q}");
            estilo::texto(
                lupa.x - estilo::medir(&t, 13) - 8.0,
                r.y + 27.0,
                &t,
                13,
                estilo::SUAVE,
            );
            if crate::onde_obter::botao(lupa) {
                self.onde_obter = Some(*id);
            } else if clique && r.contains(m) {
                mudou |= self.escolhe_item(barra, *id);
            }
        }
        mudou
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use shared::constants::item_id as it;
    use shared::protocol::EspacoDaBarra;

    fn slot(id: u16, qty: u32) -> InventorySlot {
        InventorySlot {
            item_id: id,
            qty,
            instance: None,
        }
    }

    #[test]
    fn lista_so_consumiveis_somados() {
        let slots = vec![
            slot(it::SORTE_POTION, 2),
            slot(it::STEEL, 9),
            slot(it::SORTE_POTION, 3),
            slot(it::HEALTH_POTION, 1),
        ];
        assert_eq!(
            consumiveis(&slots),
            vec![(it::HEALTH_POTION, 1), (it::SORTE_POTION, 5)]
        );
    }

    #[test]
    fn item_depois_espaco_ou_espaco_depois_item_atribui_e_limpar_esvazia() {
        let mut b = Barra::default();
        let mut c = ConfigBarra::default();
        c.abrir(None);
        assert!(!c.escolhe_item(&mut b, it::FORTUNA_POTION));
        assert!(c.escolhe_espaco(&mut b, 2));
        assert_eq!(b.espacos[2].item_id, it::FORTUNA_POTION);
        assert!(!c.escolhe_espaco(&mut b, 3));
        assert!(c.escolhe_item(&mut b, it::GREATER_MANA));
        assert_eq!(b.espacos[3].item_id, it::GREATER_MANA);
        assert_eq!(b.espacos[3].limiar, 40, "limiar padrao da categoria");
        b.limpar(3);
        assert_eq!(b.espacos[3], EspacoDaBarra::default());
        // Abrir por um espaco vazio ja' deixa ele escolhido.
        c.abrir(Some(1));
        assert!(c.escolhe_item(&mut b, it::STAMINA_POTION));
        assert_eq!(b.espacos[1].item_id, it::STAMINA_POTION);
    }
}
