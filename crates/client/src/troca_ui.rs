//! DIRECT TRADE window (`shared::troca`): invite popup, the two sides of the
//! table, your bag to pick from, gold, Lock → Confirm. The server decides;
//! this only shows the table it sends and asks for changes.

use std::collections::HashMap;

use macroquad::prelude::*;
use shared::protocol::ClientMessage;
use shared::troca::{AvisoTroca, Lado, PedidoTroca, MAX_ITENS};
use shared::InventorySlot;

#[allow(unused_imports)]
use crate::ui::Toque as _;
use crate::hud_estilo as estilo;

#[derive(Default)]
pub struct TrocaUi {
    /// Pending invite: (from, when it arrived).
    convite: Option<(String, f64)>,
    /// The open table: (me, them).
    mesa: Option<(Lado, Lado)>,
}

fn u(v: f32) -> f32 {
    estilo::u(v)
}

fn msg(p: PedidoTroca) -> ClientMessage {
    ClientMessage::Troca { pedido: p }
}

impl TrocaUi {
    pub fn aberto(&self) -> bool {
        self.mesa.is_some() || self.convite.is_some()
    }

    pub fn convidar(nome: String) -> ClientMessage {
        msg(PedidoTroca::Convidar { nome })
    }

    /// Returns a line for the chat, if any.
    pub fn receber(&mut self, a: AvisoTroca) -> Option<String> {
        match a {
            AvisoTroca::Convite { de } => {
                self.convite = Some((de.clone(), get_time()));
                Some(format!("{de} wants to trade with you."))
            }
            AvisoTroca::Estado { eu, outro } => {
                self.convite = None;
                self.mesa = Some((eu, outro));
                None
            }
            AvisoTroca::Fim { texto, .. } => {
                self.mesa = None;
                Some(format!("Trade: {texto}"))
            }
        }
    }

    pub fn desenha(
        &mut self,
        bolsa: &[InventorySlot],
        nomes: &HashMap<u16, String>,
        ouro: u64,
    ) -> Vec<ClientMessage> {
        let mut saida = Vec::new();
        // Invite popup (expires with the server's own 30 s).
        if let Some((de, quando)) = self.convite.clone() {
            if get_time() - quando > shared::troca::CONVITE_S as f64 {
                self.convite = None;
            } else if self.mesa.is_none() {
                estilo::no_painel(estilo::escala_do_painel(460.0, 180.0), || {
                    let seguro = crate::hud_layout::tela_segura();
                    let (w, h) = (u(460.0), u(170.0));
                    let p = Rect::new(seguro.center().x - w * 0.5, seguro.y + u(90.0), w, h);
                    estilo::painel(p);
                    estilo::texto_centro_forte(p.center().x, p.y + u(40.0), &format!("{de} wants to trade"), 18, estilo::OURO);
                    let m = Vec2::from(mouse_position());
                    let clicou = crate::foco::clique();
                    let bw = (w - u(60.0)) * 0.5;
                    for (i, (rot, aceita)) in [("Accept", true), ("Decline", false)].into_iter().enumerate() {
                        let b = Rect::new(p.x + u(20.0) + i as f32 * (bw + u(20.0)), p.y + h - u(70.0), bw, u(48.0));
                        let sobre = b.toca(m);
                        estilo::botao(b, rot, estilo::estado(sobre, is_mouse_button_down(MouseButton::Left), false, false), aceita);
                        if sobre && clicou {
                            saida.push(msg(PedidoTroca::Responder { de: de.clone(), aceita }));
                            self.convite = None;
                        }
                    }
                });
            }
        }
        let Some((eu, outro)) = self.mesa.clone() else {
            return saida;
        };
        estilo::no_painel(estilo::escala_do_painel(900.0, 700.0), || {
            let seguro = crate::hud_layout::tela_segura();
            let w = u(900.0).min(seguro.w - 16.0);
            let h = u(700.0).min(seguro.h - 16.0);
            let p = Rect::new(seguro.center().x - w * 0.5, seguro.center().y - h * 0.5, w, h);
            crate::hud_layout::escurece(0.45);
            estilo::painel(p);
            let m = Vec2::from(mouse_position());
            let clicou = crate::foco::clique();
            let x0 = p.x + u(20.0);
            estilo::texto_forte(x0, p.y + u(36.0), &format!("Trade with {}", outro.nome), 20, estilo::OURO);
            // Two columns: me | them.
            let col_w = (p.w - u(60.0)) * 0.5;
            let lado = u(52.0);
            let mut desenha_lado = |x: f32, l: &Lado, meu: bool, saida: &mut Vec<ClientMessage>| {
                let topo = p.y + u(60.0);
                let r = Rect::new(x, topo, col_w, u(250.0));
                estilo::cartao(r, false, l.travado);
                let estado = if l.confirmado {
                    "CONFIRMED"
                } else if l.travado {
                    "LOCKED"
                } else {
                    "choosing…"
                };
                estilo::texto_forte(r.x + u(12.0), r.y + u(26.0), if meu { "You" } else { &l.nome }, 16, estilo::TEXTO);
                estilo::texto(
                    r.x + r.w - u(12.0) - estilo::medir(estado, 13),
                    r.y + u(26.0),
                    estado,
                    13,
                    if l.confirmado { estilo::VERDE } else if l.travado { estilo::OURO } else { estilo::SUAVE },
                );
                for (i, it) in l.itens.iter().enumerate() {
                    let c = (i % 4) as f32;
                    let li = (i / 4) as f32;
                    let q = Rect::new(r.x + u(12.0) + c * (lado + u(10.0)), r.y + u(44.0) + li * (lado + u(10.0)), lado, lado);
                    crate::icones::icone(it.item_id, q, None, (it.qtd > 1).then_some(it.qtd));
                    if q.toca(m) {
                        let nome = nomes.get(&it.item_id).cloned().unwrap_or_default();
                        estilo::tooltip(q, &nome, false);
                        if meu && clicou {
                            saida.push(msg(PedidoTroca::Tirar { indice: i as u8 }));
                        }
                    }
                }
                estilo::texto(
                    r.x + u(12.0),
                    r.y + r.h - u(14.0),
                    &format!("{} gold", shared::idioma::milhar(l.ouro)),
                    15,
                    estilo::OURO,
                );
            };
            desenha_lado(x0, &eu, true, &mut saida);
            desenha_lado(x0 + col_w + u(20.0), &outro, false, &mut saida);
            // My bag: tap to put the whole stack on the table.
            let by = p.y + u(330.0);
            estilo::texto(x0, by, "Your bag — tap an item to offer it, tap it on the table to take it back", 13, estilo::SUAVE);
            let celula = u(46.0);
            let por_linha = ((p.w - u(40.0)) / (celula + u(6.0))).floor().max(1.0) as usize;
            let na_mesa = |slot: usize| eu.itens.iter().any(|i| i.slot as usize == slot);
            let livres: Vec<(usize, &InventorySlot)> = bolsa
                .iter()
                .enumerate()
                .filter(|(_, s)| s.qty > 0 && !s.instance.as_ref().is_some_and(|i| i.vinculado))
                .collect();
            for (n, (slot, s)) in livres.iter().enumerate() {
                let q = Rect::new(
                    x0 + (n % por_linha) as f32 * (celula + u(6.0)),
                    by + u(12.0) + (n / por_linha) as f32 * (celula + u(6.0)),
                    celula,
                    celula,
                );
                if q.y + q.h > p.y + p.h - u(130.0) {
                    break;
                }
                crate::icones::icone(s.item_id, q, None, (s.qty > 1).then_some(s.qty));
                if na_mesa(*slot) {
                    draw_rectangle(q.x, q.y, q.w, q.h, Color::new(0.0, 0.0, 0.0, 0.55));
                }
                if q.toca(m) && clicou && !na_mesa(*slot) && eu.itens.len() < MAX_ITENS {
                    saida.push(msg(PedidoTroca::Por { slot: *slot as u16, qtd: s.qty }));
                }
            }
            // Gold.
            let gy = p.y + p.h - u(118.0);
            estilo::texto(x0, gy + u(26.0), &format!("Gold: you have {}", shared::idioma::milhar(ouro)), 14, estilo::TEXTO);
            for (i, (rot, mais)) in [("+100", 100u64), ("+1k", 1_000), ("+10k", 10_000), ("+100k", 100_000), ("0", 0)].into_iter().enumerate() {
                let b = Rect::new(x0 + u(260.0) + i as f32 * u(92.0), gy, u(84.0), u(40.0));
                let sobre = b.toca(m);
                estilo::botao(b, rot, estilo::estado(sobre, false, false, false), false);
                if sobre && clicou {
                    let novo = if mais == 0 { 0 } else { (eu.ouro + mais).min(ouro) };
                    saida.push(msg(PedidoTroca::Ouro { qtd: novo }));
                }
            }
            // Lock → Confirm, Cancel.
            let ay = p.y + p.h - u(64.0);
            let bw = (p.w - u(80.0)) / 3.0;
            let pode_confirmar = eu.travado && outro.travado && !eu.confirmado;
            let botoes = [
                ("Lock", !eu.travado, PedidoTroca::Travar),
                ("Confirm", pode_confirmar, PedidoTroca::Confirmar),
                ("Cancel", true, PedidoTroca::Cancelar),
            ];
            for (i, (rot, ativo, pedido)) in botoes.into_iter().enumerate() {
                let b = Rect::new(x0 + i as f32 * (bw + u(20.0)), ay, bw, u(48.0));
                let sobre = ativo && b.toca(m);
                estilo::botao(b, rot, estilo::estado(sobre, is_mouse_button_down(MouseButton::Left), !ativo, false), i == 1);
                if sobre && clicou {
                    saida.push(msg(pedido));
                }
            }
        });
        saida
    }
}
