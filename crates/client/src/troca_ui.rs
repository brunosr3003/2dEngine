//! DIRECT TRADE window (`shared::troca`), redone 13/11/2026 (owner: "it
//! doesn't show all the items, you can't trade from the menu, it's ugly, no
//! item icons, Energy and Darksteel aren't tradable, there must be a way to
//! type the number of items").
//!
//! Three pieces:
//! - the PICKER (Menu → Trade): the players around you, tap one to invite;
//! - the INVITE popup when someone asks you;
//! - the TABLE: your whole tradable bag on the left (scrolled by finger with
//!   glide, the bag's own cells), the two offers on the right, the amount of
//!   a stack, the gold and the Energy typed with the market's number picker
//!   (`mercado_ui::seletor`), then Lock → Confirm.
//!
//! The server decides everything; this only shows the table it sends and
//! asks for changes.

use std::collections::{HashMap, HashSet};

use macroquad::prelude::*;
use shared::protocol::ClientMessage;
use shared::troca::{AvisoTroca, ItemNaMesa, Lado, PedidoTroca, MAX_ITENS};
use shared::InventorySlot;

use crate::economia::milhar;
use crate::hud_estilo as estilo;
use crate::mercado_ui::{botao, seletor, Edicao};
#[allow(unused_imports)]
use crate::ui::Toque as _;

/// What the window reads from the game.
pub struct Contexto<'a> {
    pub slots: &'a [InventorySlot],
    pub nomes: &'a HashMap<u16, String>,
    /// Item kinds the server marks as bound (`ItemsConfig`).
    pub vinculados: &'a HashSet<u16>,
    pub ouro: u64,
    pub energia: u64,
    pub digitado: &'a [char],
    /// The on-screen keyboard is up (phones).
    pub teclado: bool,
    pub vox: &'a crate::vox::VoxCache,
    pub solido: &'a macroquad::material::Material,
    /// Players around: (name, distance), nearest first.
    pub perto: &'a [(String, f32)],
}

#[derive(Default)]
pub struct TrocaUi {
    /// Pending invite: (from, when it arrived).
    convite: Option<(String, f64)>,
    /// The open table: (me, them).
    mesa: Option<(Lado, Lado)>,
    /// The picker of nearby players (Menu → Trade).
    escolhendo: bool,
    /// Invite sent and not answered yet: (to, when).
    convidado: Option<(String, f64)>,
    /// The bag slot picked on the left, and the amount typed for it.
    sel: Option<usize>,
    qtd: u64,
    edicao: Edicao,
    rolagem_bolsa: crate::rolagem::Rolagem,
    rolagem_perto: crate::rolagem::Rolagem,
    /// (text, ok, when)
    aviso: Option<(String, bool, f64)>,
    /// How far the table is raised above the on-screen keyboard.
    subida: f32,
}

fn msg(p: PedidoTroca) -> ClientMessage {
    ClientMessage::Troca { pedido: p }
}

/// The tradable slots of the bag, in the order the left grid shows them:
/// the wallet (Darksteel, Copper) first — it lives at the end of the list,
/// outside the bag's grid, and used to be the part nobody found — then the
/// bag in its own order.
pub fn negociaveis(slots: &[InventorySlot], vinculados: &HashSet<u16>) -> Vec<usize> {
    let livre = |s: &InventorySlot| shared::troca::negociavel(s, vinculados.contains(&s.item_id));
    let moeda = |s: &InventorySlot| s.instance.is_none() && shared::armazem::e_moeda(s.item_id);
    let mut v: Vec<usize> = (0..slots.len()).filter(|&i| livre(&slots[i]) && moeda(&slots[i])).collect();
    v.sort_by_key(|&i| std::cmp::Reverse(slots[i].item_id == shared::item_id::DARKSTEEL));
    v.extend((0..slots.len()).filter(|&i| livre(&slots[i]) && !moeda(&slots[i])));
    v
}

/// What the table says to do next, from my side's point of view.
pub fn passo(eu: &Lado, outro: &Lado) -> &'static str {
    match (eu.travado, outro.travado, eu.confirmado, outro.confirmado) {
        (false, _, _, _) => "Choose your offer, then Lock it.",
        (true, false, _, _) => "Waiting for {} to lock.",
        (true, true, false, false) => "Both locked: check the offers and Confirm.",
        (true, true, false, true) => "{} confirmed: Confirm to trade.",
        (true, true, true, _) => "Waiting for {} to confirm.",
    }
}

fn estado_do_lado(l: &Lado) -> (&'static str, Color) {
    if l.confirmado {
        ("CONFIRMED", estilo::VERDE)
    } else if l.travado {
        ("LOCKED", estilo::OURO)
    } else {
        ("CHOOSING", estilo::SUAVE)
    }
}

fn nome_do_item(nomes: &HashMap<u16, String>, id: u16, inst: Option<&shared::items::ItemInstance>) -> String {
    let base = nomes.get(&id).cloned().unwrap_or_else(|| format!("Item {id}"));
    crate::mercado_ui::nome_com_degrau(&base, inst)
}

fn como_slot(it: &ItemNaMesa) -> InventorySlot {
    InventorySlot { item_id: it.item_id, qty: it.qtd, instance: it.instance }
}

impl TrocaUi {
    pub fn aberto(&self) -> bool {
        self.mesa.is_some() || self.convite.is_some() || self.escolhendo
    }

    pub fn convidar(nome: String) -> ClientMessage {
        msg(PedidoTroca::Convidar { nome })
    }

    /// Menu → Trade: the players around.
    pub fn abrir(&mut self) {
        if self.mesa.is_none() {
            self.escolhendo = true;
            self.rolagem_perto.zera();
        }
    }

    /// A number is being typed (the on-screen keyboard stays up).
    pub fn digitando(&self) -> bool {
        self.mesa.is_some() && self.edicao.ativa()
    }

    /// Escape: cancels the number being typed; then cancels the trade
    /// (asking the server), declines an invite or closes the picker.
    pub fn esc(&mut self) -> Option<ClientMessage> {
        if self.mesa.is_some() {
            if self.edicao.cancela() {
                return None;
            }
            return Some(msg(PedidoTroca::Cancelar));
        }
        if self.escolhendo {
            self.escolhendo = false;
            return None;
        }
        if let Some((de, _)) = self.convite.take() {
            return Some(msg(PedidoTroca::Responder { de, aceita: false }));
        }
        None
    }

    /// Returns a line for the chat, if any.
    pub fn receber(&mut self, a: AvisoTroca) -> Option<String> {
        self.receber_em(a, get_time())
    }

    /// `receber` with the clock passed in (tests run without a window).
    fn receber_em(&mut self, a: AvisoTroca, agora: f64) -> Option<String> {
        match a {
            AvisoTroca::Convite { de } => {
                self.convite = Some((de.clone(), agora));
                Some(format!("{de} wants to trade with you."))
            }
            AvisoTroca::Estado { eu, outro } => {
                if self.mesa.is_none() {
                    // A new table: nothing picked, the bag from the top.
                    self.sel = None;
                    self.edicao = Edicao::default();
                    self.rolagem_bolsa.zera();
                    self.aviso = None;
                }
                self.convite = None;
                self.convidado = None;
                self.escolhendo = false;
                self.mesa = Some((eu, outro));
                None
            }
            AvisoTroca::Fim { ok, texto } => {
                self.mesa = None;
                self.sel = None;
                self.edicao = Edicao::default();
                self.convidado = None;
                self.aviso = Some((texto.clone(), ok, agora));
                Some(format!("Trade: {texto}"))
            }
            AvisoTroca::Aviso { ok, texto } => {
                if !ok {
                    self.convidado = None;
                }
                self.aviso = Some((texto.clone(), ok, agora));
                Some(texto)
            }
        }
    }

    pub fn desenha(&mut self, c: &Contexto) -> Vec<ClientMessage> {
        let mut saida = Vec::new();
        let agora = get_time();
        if self.convidado.as_ref().is_some_and(|(_, t)| agora - t > shared::troca::CONVITE_S as f64) {
            self.convidado = None;
        }
        // Invite popup (expires with the server's own 30 s).
        if let Some((de, quando)) = self.convite.clone() {
            if agora - quando > shared::troca::CONVITE_S as f64 {
                self.convite = None;
            } else if self.mesa.is_none() {
                let resta = (shared::troca::CONVITE_S as f64 - (agora - quando)).max(0.0);
                estilo::no_painel(estilo::escala_do_painel(480.0, 200.0), || {
                    self.convite_popup(&de, resta, &mut saida)
                });
            }
        }
        if self.mesa.is_some() {
            estilo::no_painel(estilo::escala_do_painel(980.0, 660.0), || self.mesa_aberta(c, &mut saida));
            // Nothing else in this window holds the keyboard.
            let _ = self.edicao.fim_do_quadro();
        } else if self.escolhendo {
            estilo::no_painel(estilo::escala_do_painel(560.0, 520.0), || self.escolha(c, &mut saida));
        }
        saida
    }

    fn convite_popup(&mut self, de: &str, resta: f64, saida: &mut Vec<ClientMessage>) {
        let f = estilo::fator_texto();
        let seguro = crate::hud_layout::tela_segura();
        let (w, h) = ((480.0 * f).min(seguro.w - 16.0), 190.0 * f);
        let p = Rect::new(seguro.center().x - w * 0.5, seguro.y + 90.0 * f, w, h);
        estilo::painel(p);
        estilo::texto_centro_forte(p.center().x, p.y + 40.0 * f, &format!("{de} wants to trade"), 19, estilo::OURO);
        estilo::texto_centro(
            p.center().x,
            p.y + 68.0 * f,
            &format!("Answer within {} s", resta.ceil() as u32),
            13,
            estilo::SUAVE,
        );
        let bw = (w - 60.0 * f) * 0.5;
        for (i, (rot, aceita)) in [("Accept", true), ("Decline", false)].into_iter().enumerate() {
            let b = Rect::new(p.x + 20.0 * f + i as f32 * (bw + 20.0 * f), p.y + h - 70.0 * f, bw, 48.0 * f);
            if botao(b, rot, true, aceita) {
                saida.push(msg(PedidoTroca::Responder { de: de.to_string(), aceita }));
                self.convite = None;
            }
        }
    }

    /// Menu → Trade: who is around, and an Invite on each.
    fn escolha(&mut self, c: &Contexto, saida: &mut Vec<ClientMessage>) {
        let f = estilo::fator_texto();
        let agora = get_time();
        let seguro = crate::hud_layout::tela_segura();
        let (w, h) = ((560.0 * f).min(seguro.w - 16.0), (520.0 * f).min(seguro.h - 16.0));
        let p = Rect::new(seguro.center().x - w * 0.5, seguro.center().y - h * 0.5, w, h);
        crate::hud_layout::escurece(0.45);
        estilo::painel(p);
        estilo::texto(p.x + 18.0 * f, p.y + 34.0 * f, "Direct trade", 22, estilo::OURO);
        let fechar = Rect::new(p.x + p.w - 48.0 * f, p.y + 10.0 * f, 38.0 * f, 34.0 * f);
        if crate::ui::botao(fechar, "x", true) {
            self.escolhendo = false;
            return;
        }
        estilo::texto_ajustado(
            "Players near you. Stand next to someone and tap Invite.",
            p.x + 18.0 * f,
            p.y + 62.0 * f,
            p.w - 36.0 * f,
            14,
            estilo::SUAVE,
        );
        let lista = Rect::new(p.x + 18.0 * f, p.y + 80.0 * f, p.w - 36.0 * f, p.h - 80.0 * f - 56.0 * f);
        let alt = 58.0 * f;
        let n = c.perto.len();
        let toque = self.rolagem_perto.quadro(lista, n as f32 * alt, alt);
        crate::rolagem::recortar(Some(lista));
        if n == 0 {
            estilo::texto_ajustado(
                "Nobody around. Walk up to a player (or tap them → Actions → Trade items).",
                lista.x,
                lista.y + 30.0 * f,
                lista.w,
                15,
                estilo::SUAVE,
            );
        }
        let esperando = self.convidado.as_ref().map(|(n, _)| n.clone());
        for (i, (nome, dist)) in c.perto.iter().enumerate() {
            let r = Rect::new(lista.x, lista.y + i as f32 * alt - self.rolagem_perto.pos, lista.w - 20.0 * f, alt - 6.0 * f);
            if r.y + r.h < lista.y || r.y > lista.y + lista.h {
                continue;
            }
            estilo::cartao(r, false, esperando.as_deref() == Some(nome.as_str()));
            estilo::texto_forte(r.x + 14.0 * f, r.y + r.h * 0.5 + 6.0 * f, nome, 16, estilo::TEXTO);
            let perto = *dist <= shared::troca::DISTANCIA_MAX;
            let b = Rect::new(r.x + r.w - 140.0 * f, r.y + (r.h - 40.0 * f) * 0.5, 130.0 * f, 40.0 * f);
            let rotulo = if esperando.as_deref() == Some(nome.as_str()) {
                "Waiting…"
            } else if perto {
                "Invite"
            } else {
                "Get closer"
            };
            let ativo = perto && esperando.is_none();
            let sobre = ativo && b.toca(Vec2::from(mouse_position()));
            estilo::botao(b, rotulo, estilo::estado(sobre, false, !ativo, false), ativo);
            if ativo && toque.is_some_and(|t| b.toca(t)) {
                saida.push(TrocaUi::convidar(nome.clone()));
                self.convidado = Some((nome.clone(), agora));
            }
        }
        crate::rolagem::recortar(None);
        self.rolagem_perto.desenha(lista, n as f32 * alt);
        if let Some((nome, _)) = &self.convidado {
            estilo::texto_ajustado(
                &format!("Invite sent to {nome}: waiting for an answer."),
                p.x + 18.0 * f,
                p.y + p.h - 22.0 * f,
                p.w - 36.0 * f,
                14,
                estilo::OURO,
            );
        } else if let Some((texto, ok, quando)) = &self.aviso {
            if agora - quando < 6.0 {
                estilo::texto_ajustado(
                    texto,
                    p.x + 18.0 * f,
                    p.y + p.h - 22.0 * f,
                    p.w - 36.0 * f,
                    14,
                    if *ok { estilo::VERDE } else { estilo::VERMELHO },
                );
            }
        }
    }

    fn mesa_aberta(&mut self, c: &Contexto, saida: &mut Vec<ClientMessage>) {
        let Some((eu, outro)) = self.mesa.clone() else { return };
        let f = estilo::fator_texto();
        let agora = get_time();
        let seguro = crate::hud_layout::tela_segura();
        let (w, h) = ((980.0 * f).min(seguro.w - 16.0), (660.0 * f).min(seguro.h - 16.0));
        let mut p = Rect::new(seguro.center().x - w * 0.5, seguro.center().y - h * 0.5, w, h);
        // A number being typed on a phone: the window rises above the keyboard.
        self.subida = if self.edicao.ativa() {
            crate::teclado_virtual::deslocamento(c.teclado, screen_height(), f32::INFINITY, self.edicao.fundo() + self.subida)
        } else {
            0.0
        };
        p.y -= self.subida;
        crate::hud_layout::escurece(0.55);
        estilo::painel(p);
        estilo::texto(p.x + 18.0 * f, p.y + 34.0 * f, &format!("Trade with {}", outro.nome), 22, estilo::OURO);
        let fechar = Rect::new(p.x + p.w - 48.0 * f, p.y + 10.0 * f, 38.0 * f, 34.0 * f);
        // Balances: what I can still offer.
        let txt_ouro = milhar(c.ouro);
        let txt_en = milhar(c.energia);
        let x_en = fechar.x - 16.0 * f - estilo::medir_forte(&txt_en, 15);
        estilo::texto_forte(x_en, p.y + 32.0 * f, &txt_en, 15, estilo::TEXTO);
        estilo::icone_energia(vec2(x_en - 12.0 * f, p.y + 27.0 * f), 20.0 * f);
        let x_ouro = x_en - 34.0 * f - estilo::largura_ouro_texto(&txt_ouro, 15, true);
        estilo::ouro_texto(x_ouro, p.y + 32.0 * f, &txt_ouro, 15, estilo::OURO, true);
        if crate::ui::botao(fechar, "x", true) {
            saida.push(msg(PedidoTroca::Cancelar));
            return;
        }

        let topo = p.y + 56.0 * f;
        let rodape = p.y + p.h - 62.0 * f;
        let col_w = (p.w - 36.0 * f - 16.0 * f) * 0.5;
        let esq = Rect::new(p.x + 18.0 * f, topo, col_w, rodape - 34.0 * f - topo);
        let dir = Rect::new(esq.x + col_w + 16.0 * f, topo, col_w, esq.h);

        self.bolsa(esq, c, &eu, f, saida);

        // ── the two offers ──
        let alto_outro = 190.0 * f;
        let r_outro = Rect::new(dir.x, dir.y, dir.w, alto_outro);
        self.oferta(r_outro, &outro, false, c, f, saida);
        let r_eu = Rect::new(dir.x, dir.y + alto_outro + 10.0 * f, dir.w, dir.h - alto_outro - 10.0 * f);
        self.oferta(r_eu, &eu, true, c, f, saida);

        // ── what to do next ──
        let texto = passo(&eu, &outro).replace("{}", &outro.nome);
        let aviso = self.aviso.as_ref().filter(|(_, _, q)| agora - q < 5.0);
        match aviso {
            Some((t, ok, _)) => estilo::texto_ajustado(
                t,
                p.x + 18.0 * f,
                rodape - 12.0 * f,
                p.w - 36.0 * f,
                14,
                if *ok { estilo::VERDE } else { estilo::VERMELHO },
            ),
            None => estilo::texto_ajustado(&texto, p.x + 18.0 * f, rodape - 12.0 * f, p.w - 36.0 * f, 15, estilo::TEXTO),
        }

        // ── Cancel · Lock · Confirm ──
        let bw = (p.w - 36.0 * f - 24.0 * f) / 3.0;
        let pode_confirmar = eu.travado && outro.travado && !eu.confirmado;
        let rot_travar = if eu.travado { "Offer locked" } else { "Lock offer" };
        let rot_confirmar = if eu.confirmado { "Confirmed" } else { "Confirm trade" };
        let botoes = [
            ("Cancel", true, false, PedidoTroca::Cancelar),
            (rot_travar, !eu.travado, !eu.travado, PedidoTroca::Travar),
            (rot_confirmar, pode_confirmar, pode_confirmar, PedidoTroca::Confirmar),
        ];
        for (i, (rot, ativo, primario, pedido)) in botoes.into_iter().enumerate() {
            let b = Rect::new(p.x + 18.0 * f + i as f32 * (bw + 12.0 * f), rodape, bw, 46.0 * f);
            if botao(b, rot, ativo, primario) {
                saida.push(msg(pedido));
            }
        }
    }

    /// The left column: every tradable thing in the bag, and the amount of
    /// the one picked.
    fn bolsa(&mut self, esq: Rect, c: &Contexto, eu: &Lado, f: f32, saida: &mut Vec<ClientMessage>) {
        let m = Vec2::from(mouse_position());
        let lista = negociaveis(c.slots, c.vinculados);
        estilo::texto_forte(esq.x, esq.y + 14.0 * f, "Your bag", 14, estilo::SUAVE);
        let presos = c
            .slots
            .iter()
            .filter(|s| s.qty > 0 && s.item_id != 0 && !shared::troca::negociavel(s, c.vinculados.contains(&s.item_id)))
            .count();
        if presos > 0 {
            let t = format!("{presos} bound item(s) can't be traded");
            estilo::texto(esq.x + esq.w - estilo::medir(&t, 12), esq.y + 14.0 * f, &t, 12, estilo::SUAVE);
        }
        // The picked slot went away (used, moved): drop it.
        if self.sel.is_some_and(|i| !lista.contains(&i)) {
            self.sel = None;
        }
        let alto_form = 140.0 * f;
        let grade = Rect::new(esq.x, esq.y + 26.0 * f, esq.w, esq.h - 26.0 * f - alto_form - 8.0 * f);
        let lado = 62.0 * f;
        let por_linha = ((grade.w - 14.0 * f) / lado).floor().max(1.0) as usize;
        let total = lista.len().div_ceil(por_linha) as f32 * lado;
        let toque = self.rolagem_bolsa.quadro(grade, total, lado);
        if lista.is_empty() {
            estilo::texto(grade.x, grade.y + 30.0 * f, "Nothing tradable in your bag.", 15, estilo::SUAVE);
        }
        crate::rolagem::recortar(Some(grade));
        let mut dica = None;
        for (k, &i) in lista.iter().enumerate() {
            let r = Rect::new(
                grade.x + (k % por_linha) as f32 * lado,
                grade.y + (k / por_linha) as f32 * lado - self.rolagem_bolsa.pos,
                lado - 6.0 * f,
                lado - 6.0 * f,
            );
            if r.y + r.h < grade.y || r.y > grade.y + grade.h {
                continue;
            }
            let s = &c.slots[i];
            crate::bolsa::celula_do_slot(r, s, Some((c.vox, c.solido)));
            let na_mesa = eu.itens.iter().find(|it| it.slot as usize == i).map(|it| it.qtd);
            if let Some(q) = na_mesa {
                // On the table: a green rim and how many went.
                estilo::borda_arredondada(r, estilo::RAIO_PEQUENO + 1.0, 2.0, estilo::VERDE);
                let t = format!("↑{}", crate::bolsa::curta(q));
                estilo::texto_sombra(r.x + 3.0 * f, r.y + r.h - 4.0 * f, &t, 11, estilo::VERDE, true);
            }
            if self.sel == Some(i) {
                estilo::borda_arredondada(
                    Rect::new(r.x - 3.0 * f, r.y - 3.0 * f, r.w + 6.0 * f, r.h + 6.0 * f),
                    estilo::RAIO_PEQUENO + 4.0,
                    2.0,
                    estilo::OURO,
                );
            }
            if r.toca(m) && grade.toca(m) && !self.rolagem_bolsa.arrastando() {
                dica = Some((r, nome_do_item(c.nomes, s.item_id, s.instance.as_ref())));
            }
            if toque.is_some_and(|t| r.toca(t)) {
                self.escolhe(i, s, eu);
            }
        }
        crate::rolagem::recortar(None);
        self.rolagem_bolsa.desenha(grade, total);
        if let Some((r, t)) = dica {
            estilo::tooltip(r, &t, true);
        }

        // ── the picked item: how many ──
        let form = Rect::new(esq.x, esq.y + esq.h - alto_form, esq.w, alto_form);
        estilo::cartao(form, false, false);
        let (fx, fw) = (form.x + 12.0 * f, form.w - 24.0 * f);
        let Some(i) = self.sel else {
            estilo::texto_ajustado(
                "Tap an item to offer it. Stacks (Darksteel, materials) take any amount: tap the number to type it.",
                fx,
                form.y + 34.0 * f,
                fw,
                14,
                estilo::SUAVE,
            );
            return;
        };
        let s = c.slots[i];
        let icone = Rect::new(fx, form.y + 10.0 * f, 44.0 * f, 44.0 * f);
        crate::bolsa::celula_do_slot(icone, &s, Some((c.vox, c.solido)));
        let na_mesa = eu.itens.iter().position(|it| it.slot as usize == i);
        let tira_w = 120.0 * f;
        let nome = nome_do_item(c.nomes, s.item_id, s.instance.as_ref());
        estilo::texto_ajustado(&nome, icone.x + icone.w + 10.0 * f, form.y + 28.0 * f, fw - icone.w - tira_w - 20.0 * f, 16, estilo::TEXTO);
        estilo::texto(
            icone.x + icone.w + 10.0 * f,
            form.y + 48.0 * f,
            &format!("In your bag: {}", milhar(s.qty as u64)),
            13,
            estilo::SUAVE,
        );
        if let Some(k) = na_mesa {
            let b = Rect::new(fx + fw - tira_w, form.y + 12.0 * f, tira_w, 36.0 * f);
            if botao(b, "Take back", true, false) {
                saida.push(msg(PedidoTroca::Tirar { indice: k as u8 }));
                self.sel = None;
                return;
            }
        }
        let max = s.qty.max(1) as u64;
        let oferta_w = 150.0 * f;
        let linha = Rect::new(fx, form.y + 84.0 * f, fw - oferta_w - 10.0 * f, 42.0 * f);
        if s.qty > 1 {
            seletor(&mut self.edicao, "troca_qtd", c.digitado, linha, "Amount", &mut self.qtd, 1, max, false, f);
        } else {
            self.qtd = 1;
        }
        let ja = na_mesa.map(|k| eu.itens[k].qtd as u64);
        let cheia = ja.is_none() && eu.itens.len() >= MAX_ITENS;
        let rotulo = match ja {
            Some(q) if q == self.qtd => format!("On table ×{}", milhar(q)),
            Some(_) => format!("Update ×{}", milhar(self.qtd)),
            None if cheia => "Table full".to_string(),
            None => format!("Offer ×{}", milhar(self.qtd)),
        };
        let ativo = !cheia && ja != Some(self.qtd);
        let b = Rect::new(fx + fw - oferta_w, linha.y, oferta_w, linha.h);
        // An amount being typed commits on this tap first (the seletor
        // closes on a tap elsewhere), so "type 25, tap Offer" offers 25.
        if botao(b, &rotulo, ativo, true) {
            saida.push(msg(PedidoTroca::Por { slot: i as u16, qtd: self.qtd.min(u32::MAX as u64) as u32 }));
        }
    }

    fn escolhe(&mut self, i: usize, s: &InventorySlot, eu: &Lado) {
        self.sel = Some(i);
        // Already on the table: start from what is there; otherwise the
        // whole stack (the old window always offered the whole stack).
        self.qtd = eu
            .itens
            .iter()
            .find(|it| it.slot as usize == i)
            .map_or(s.qty as u64, |it| it.qtd as u64)
            .max(1);
    }

    /// One side's offer: state, items, gold and Energy. Mine has the gold
    /// and Energy pickers; theirs only shows.
    fn oferta(&mut self, r: Rect, l: &Lado, meu: bool, c: &Contexto, f: f32, saida: &mut Vec<ClientMessage>) {
        let m = Vec2::from(mouse_position());
        let clicou = crate::foco::clique();
        let (rotulo, cor) = estado_do_lado(l);
        estilo::cartao(r, false, l.travado);
        if l.confirmado {
            estilo::borda_arredondada(r, estilo::RAIO_PEQUENO, 2.0, estilo::VERDE);
        }
        let titulo = if meu { "Your offer".to_string() } else { format!("{}'s offer", l.nome) };
        estilo::texto_forte(r.x + 12.0 * f, r.y + 24.0 * f, &titulo, 16, estilo::TEXTO);
        // The state chip.
        let cw = estilo::medir_forte(rotulo, 13) + 22.0 * f;
        let chip = Rect::new(r.x + r.w - cw - 10.0 * f, r.y + 8.0 * f, cw, 24.0 * f);
        estilo::ret_arredondado(chip, estilo::RAIO_PEQUENO, estilo::alfa(cor, 0.16));
        estilo::borda_arredondada(chip, estilo::RAIO_PEQUENO, 1.0, cor);
        estilo::texto_centro_forte(chip.center().x, chip.y + 17.0 * f, rotulo, 13, cor);

        let lado = ((r.w - 24.0 * f) / 6.0).min(54.0 * f);
        let y0 = r.y + 36.0 * f;
        let mut dica = None;
        for k in 0..MAX_ITENS {
            let q = Rect::new(
                r.x + 12.0 * f + (k % 6) as f32 * lado,
                y0 + (k / 6) as f32 * lado,
                lado - 6.0 * f,
                lado - 6.0 * f,
            );
            match l.itens.get(k) {
                Some(it) => {
                    let s = como_slot(it);
                    crate::bolsa::celula_do_slot(q, &s, Some((c.vox, c.solido)));
                    if q.toca(m) {
                        dica = Some((q, format!("{} ×{}", nome_do_item(c.nomes, it.item_id, it.instance.as_ref()), milhar(it.qtd as u64))));
                        if meu && clicou {
                            if let Some(b) = c.slots.get(it.slot as usize) {
                                self.escolhe(it.slot as usize, b, l);
                            }
                        }
                    }
                }
                None => estilo::slot(q, None, false, false),
            }
        }
        let y = y0 + 2.0 * lado + 6.0 * f;
        if meu {
            let x = r.x + 12.0 * f;
            let w = r.w - 24.0 * f;
            let mut ouro = l.ouro;
            seletor(&mut self.edicao, "troca_ouro", c.digitado, Rect::new(x, y + 18.0 * f, w, 40.0 * f), "Gold", &mut ouro, 0, c.ouro, false, f);
            if !self.edicao.editando("troca_ouro") && ouro != l.ouro {
                saida.push(msg(PedidoTroca::Ouro { qtd: ouro }));
                if let Some((eu, _)) = self.mesa.as_mut() {
                    eu.ouro = ouro;
                }
            }
            let mut energia = l.energia;
            seletor(&mut self.edicao, "troca_energia", c.digitado, Rect::new(x, y + 82.0 * f, w, 40.0 * f), "Energy", &mut energia, 0, c.energia, false, f);
            if !self.edicao.editando("troca_energia") && energia != l.energia {
                saida.push(msg(PedidoTroca::Energia { qtd: energia }));
                if let Some((eu, _)) = self.mesa.as_mut() {
                    eu.energia = energia;
                }
            }
        } else {
            let x = r.x + 12.0 * f;
            estilo::texto(x, y + 20.0 * f, "Gold", 14, estilo::SUAVE);
            let t = milhar(l.ouro);
            estilo::ouro_texto(r.x + r.w - 12.0 * f - estilo::largura_ouro_texto(&t, 15, true), y + 20.0 * f, &t, 15, estilo::OURO, true);
            estilo::texto(x, y + 44.0 * f, "Energy", 14, estilo::SUAVE);
            let t = milhar(l.energia);
            let xt = r.x + r.w - 12.0 * f - estilo::medir_forte(&t, 15);
            estilo::texto_forte(xt, y + 44.0 * f, &t, 15, estilo::TEXTO);
            estilo::icone_energia(vec2(xt - 12.0 * f, y + 39.0 * f), 18.0 * f);
        }
        if let Some((q, t)) = dica {
            estilo::tooltip(q, &t, true);
        }
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    fn s(item_id: u16, qty: u32) -> InventorySlot {
        InventorySlot { item_id, qty, instance: None }
    }

    #[test]
    fn lista_tudo_que_negocia_com_a_carteira_na_frente() {
        // 60 items in the bag: the old window cut the grid at three rows.
        let mut slots: Vec<InventorySlot> = (0..60).map(|k| s(1000 + k, 1)).collect();
        slots[5] = s(shared::item_id::PASSE_MAGICO, 1);
        slots[7] = s(777, 3);
        slots.push(s(shared::item_id::COPPER, 9));
        slots.push(s(shared::item_id::DARKSTEEL, 500));
        let vinculados: HashSet<u16> = [777].into_iter().collect();
        let n = slots.len();
        let v = negociaveis(&slots, &vinculados);
        assert_eq!(v[0], n - 1, "Darksteel first");
        assert_eq!(v[1], n - 2, "then Copper");
        assert_eq!(v.len(), 60 - 2 + 2, "every tradable slot, minus the pass and the bound kind");
        assert!(!v.contains(&5) && !v.contains(&7));
    }

    #[test]
    fn o_passo_diz_o_que_fazer() {
        let mut eu = Lado::default();
        let mut ele = Lado { nome: "Ana".into(), ..Default::default() };
        assert_eq!(passo(&eu, &ele), "Choose your offer, then Lock it.");
        eu.travado = true;
        assert_eq!(passo(&eu, &ele), "Waiting for {} to lock.");
        ele.travado = true;
        assert_eq!(passo(&eu, &ele), "Both locked: check the offers and Confirm.");
        ele.confirmado = true;
        assert_eq!(passo(&eu, &ele), "{} confirmed: Confirm to trade.");
        eu.confirmado = true;
        assert_eq!(passo(&eu, &ele), "Waiting for {} to confirm.");
    }

    #[test]
    fn esc_cancela_o_numero_antes_da_troca() {
        let mut t = TrocaUi::default();
        t.receber_em(AvisoTroca::Estado { eu: Lado::default(), outro: Lado::default() }, 0.0);
        assert!(matches!(t.esc(), Some(ClientMessage::Troca { pedido: PedidoTroca::Cancelar })));
        t.escolhendo = false;
        t.receber_em(AvisoTroca::Fim { ok: false, texto: "x".into() }, 0.0);
        assert!(!t.aberto());
        t.abrir();
        assert!(t.aberto());
        assert!(t.esc().is_none());
        assert!(!t.aberto());
        t.receber_em(AvisoTroca::Convite { de: "Ana".into() }, 0.0);
        assert!(matches!(
            t.esc(),
            Some(ClientMessage::Troca { pedido: PedidoTroca::Responder { aceita: false, .. } })
        ));
    }
}
