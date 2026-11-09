//! DIRECT TRADE between two players (`shared::troca`).
//!
//! The table lives here, in the world loop (both players are in this same
//! process: same channel, same instance, a few steps apart). Nothing leaves
//! a bag until both CONFIRMED; then everything is re-checked and swapped in
//! one go, on copies first — a trade either happens whole or not at all.

use super::*;
use shared::troca::{self as tr, AvisoTroca, ItemNaMesa, Lado, PedidoTroca};

/// One side of an open trade.
#[derive(Debug, Clone)]
pub(crate) struct Mesa {
    pub parceiro: SessionId,
    pub itens: Vec<ItemNaMesa>,
    pub ouro: u64,
    pub travado: bool,
    pub confirmado: bool,
}

impl Mesa {
    fn nova(parceiro: SessionId) -> Self {
        Mesa { parceiro, itens: Vec::new(), ouro: 0, travado: false, confirmado: false }
    }
}

/// Whether this bag item may change hands: not bound to its owner.
fn negociavel(slot: &shared::InventorySlot) -> bool {
    slot.qty > 0 && !slot.instance.as_ref().is_some_and(|i| i.vinculado)
}

/// Takes `itens` out of `inv` (by slot, checking the item is still the same).
/// `false` = something changed since it went on the table.
fn retira(inv: &mut [shared::InventorySlot], itens: &[ItemNaMesa]) -> bool {
    for it in itens {
        let Some(s) = inv.get_mut(it.slot as usize) else { return false };
        if s.item_id != it.item_id || s.qty < it.qtd || s.instance != it.instance || !negociavel(s) {
            return false;
        }
        s.qty -= it.qtd;
        if s.qty == 0 {
            s.item_id = 0;
            s.instance = None;
        }
    }
    true
}

/// Puts `itens` in `inv`. `false` = no room for all of them.
fn entrega(inv: &mut [shared::InventorySlot], itens: &[ItemNaMesa]) -> bool {
    itens
        .iter()
        .all(|it| add_to_inventory(inv, it.item_id, it.qtd, it.instance.clone()))
}

impl GameWorld {
    fn troca_fim(&self, sid: SessionId, ok: bool, texto: &str) {
        if let Some(s) = self.sessions.get(&sid) {
            let _ = s.handle.to_client.send(ServerMessage::Troca {
                aviso: AvisoTroca::Fim { ok, texto: texto.into() },
            });
        }
    }

    /// Position and instance (one process is one island, so the island is
    /// always the same).
    fn troca_pos(&self, sid: SessionId) -> Option<(Vec2, u32)> {
        let s = self.sessions.get(&sid).filter(|s| s.logged_in)?;
        let p = self.ecs.get::<&Position>(s.entity?).ok()?.0;
        Some((p, s.instancia))
    }

    fn perto_pra_trocar(&self, a: SessionId, b: SessionId) -> bool {
        match (self.troca_pos(a), self.troca_pos(b)) {
            (Some((pa, ia)), Some((pb, ib))) => ia == ib && pa.distance(pb) <= tr::DISTANCIA_MAX,
            _ => false,
        }
    }

    fn lado(&self, sid: SessionId) -> Lado {
        let nome = self.sessions.get(&sid).map(|s| s.name.clone()).unwrap_or_default();
        match self.trocas.get(&sid) {
            Some(m) => Lado {
                nome,
                itens: m.itens.clone(),
                ouro: m.ouro,
                travado: m.travado,
                confirmado: m.confirmado,
            },
            None => Lado { nome, ..Default::default() },
        }
    }

    fn troca_envia_estado(&self, sid: SessionId) {
        let Some(m) = self.trocas.get(&sid) else { return };
        let outro = m.parceiro;
        for (eu, ele) in [(sid, outro), (outro, sid)] {
            if let Some(s) = self.sessions.get(&eu) {
                let _ = s.handle.to_client.send(ServerMessage::Troca {
                    aviso: AvisoTroca::Estado { eu: self.lado(eu), outro: self.lado(ele) },
                });
            }
        }
    }

    /// Closes the trade of `sid` (and its partner's), telling both why.
    pub(super) fn troca_encerra(&mut self, sid: SessionId, ok: bool, texto: &str) {
        let Some(m) = self.trocas.remove(&sid) else { return };
        self.trocas.remove(&m.parceiro);
        self.troca_fim(sid, ok, texto);
        self.troca_fim(m.parceiro, ok, texto);
    }

    /// Any change to the table un-locks and un-confirms BOTH sides: nobody
    /// confirms a deal that changed after they looked at it.
    fn troca_mexeu(&mut self, sid: SessionId) {
        let Some(parceiro) = self.trocas.get(&sid).map(|m| m.parceiro) else { return };
        for k in [sid, parceiro] {
            if let Some(m) = self.trocas.get_mut(&k) {
                m.travado = false;
                m.confirmado = false;
            }
        }
        self.troca_envia_estado(sid);
    }

    pub(super) fn handle_troca(&mut self, sid: SessionId, pedido: PedidoTroca) {
        if !self.sessions.get(&sid).is_some_and(|s| s.logged_in) {
            return;
        }
        match pedido {
            PedidoTroca::Convidar { nome } => self.troca_convidar(sid, &nome),
            PedidoTroca::Responder { de, aceita } => self.troca_responder(sid, &de, aceita),
            PedidoTroca::Cancelar => {
                self.troca_convites.retain(|_, (de, _)| *de != sid);
                self.troca_encerra(sid, false, "Trade cancelled.");
            }
            PedidoTroca::Por { slot, qtd } => {
                let Some(m) = self.trocas.get(&sid) else { return };
                let Some(s) = self.sessions.get(&sid) else { return };
                let Some(item) = s.inventory.get(slot as usize) else { return };
                if !negociavel(item) {
                    self.troca_fim_sem_fechar(sid, "This item can't be traded.");
                    return;
                }
                let ja = m.itens.iter().filter(|i| i.slot == slot).map(|i| i.qtd).sum::<u32>();
                let qtd = qtd.min(item.qty.saturating_sub(ja));
                if qtd == 0 {
                    return;
                }
                let novo = ItemNaMesa { slot, item_id: item.item_id, qtd, instance: item.instance.clone() };
                let m = self.trocas.get_mut(&sid).unwrap();
                if let Some(e) = m.itens.iter_mut().find(|i| i.slot == slot) {
                    e.qtd += qtd;
                } else if m.itens.len() < tr::MAX_ITENS {
                    m.itens.push(novo);
                } else {
                    return;
                }
                self.troca_mexeu(sid);
            }
            PedidoTroca::Tirar { indice } => {
                let Some(m) = self.trocas.get_mut(&sid) else { return };
                if (indice as usize) < m.itens.len() {
                    m.itens.remove(indice as usize);
                    self.troca_mexeu(sid);
                }
            }
            PedidoTroca::Ouro { qtd } => {
                let tem = self.sessions.get(&sid).map_or(0, |s| s.gold);
                let Some(m) = self.trocas.get_mut(&sid) else { return };
                m.ouro = qtd.min(tem);
                self.troca_mexeu(sid);
            }
            PedidoTroca::Travar => {
                let Some(m) = self.trocas.get_mut(&sid) else { return };
                m.travado = true;
                self.troca_envia_estado(sid);
            }
            PedidoTroca::Confirmar => {
                let Some(parceiro) = self.trocas.get(&sid).map(|m| m.parceiro) else { return };
                let ambos_travados = self.trocas.get(&sid).is_some_and(|m| m.travado)
                    && self.trocas.get(&parceiro).is_some_and(|m| m.travado);
                if !ambos_travados {
                    return;
                }
                self.trocas.get_mut(&sid).unwrap().confirmado = true;
                if self.trocas.get(&parceiro).is_some_and(|m| m.confirmado) {
                    self.troca_executa(sid);
                } else {
                    self.troca_envia_estado(sid);
                }
            }
        }
    }

    /// A refusal that doesn't close the table.
    fn troca_fim_sem_fechar(&self, sid: SessionId, texto: &str) {
        self.social_resultado(sid, false, texto);
    }

    fn troca_convidar(&mut self, sid: SessionId, nome: &str) {
        if self.trocas.contains_key(&sid) {
            self.social_resultado(sid, false, "You are already trading.");
            return;
        }
        let alvo = self
            .sessions
            .iter()
            .find(|(k, s)| **k != sid && s.logged_in && s.name.eq_ignore_ascii_case(nome.trim()))
            .map(|(k, _)| *k);
        let Some(alvo) = alvo else {
            self.social_resultado(sid, false, "The character is not on this channel.");
            return;
        };
        if self.trocas.contains_key(&alvo) {
            self.social_resultado(sid, false, "That player is already trading.");
            return;
        }
        if !self.perto_pra_trocar(sid, alvo) {
            self.social_resultado(sid, false, "Get closer to trade.");
            return;
        }
        let de = self.sessions[&sid].name.clone();
        self.troca_convites.insert(alvo, (sid, self.sim_time_s + tr::CONVITE_S));
        if let Some(s) = self.sessions.get(&alvo) {
            let _ = s.handle.to_client.send(ServerMessage::Troca { aviso: AvisoTroca::Convite { de } });
        }
        self.social_resultado(sid, true, "Trade invite sent.");
    }

    fn troca_responder(&mut self, sid: SessionId, de: &str, aceita: bool) {
        let Some((quem, ate)) = self.troca_convites.remove(&sid) else { return };
        let nome_ok = self.sessions.get(&quem).is_some_and(|s| s.name.eq_ignore_ascii_case(de));
        if !nome_ok || ate < self.sim_time_s {
            return;
        }
        if !aceita {
            self.troca_fim(quem, false, "Trade declined.");
            return;
        }
        if self.trocas.contains_key(&sid) || self.trocas.contains_key(&quem) {
            self.troca_fim(sid, false, "That player is already trading.");
            return;
        }
        if !self.perto_pra_trocar(sid, quem) {
            self.troca_fim(sid, false, "Get closer to trade.");
            self.troca_fim(quem, false, "Get closer to trade.");
            return;
        }
        self.trocas.insert(sid, Mesa::nova(quem));
        self.trocas.insert(quem, Mesa::nova(sid));
        self.troca_envia_estado(sid);
    }

    /// Both confirmed: re-check everything on COPIES, then commit both bags
    /// and both purses at once.
    fn troca_executa(&mut self, a: SessionId) {
        let Some(ma) = self.trocas.get(&a).cloned() else { return };
        let b = ma.parceiro;
        let Some(mb) = self.trocas.get(&b).cloned() else { return };
        if !self.perto_pra_trocar(a, b) {
            self.troca_encerra(a, false, "Trade cancelled: too far apart.");
            return;
        }
        let (Some(sa), Some(sb)) = (self.sessions.get(&a), self.sessions.get(&b)) else {
            self.troca_encerra(a, false, "Trade cancelled.");
            return;
        };
        if sa.gold < ma.ouro || sb.gold < mb.ouro {
            self.troca_encerra(a, false, "Trade cancelled: not enough gold.");
            return;
        }
        let mut inv_a = sa.inventory.clone();
        let mut inv_b = sb.inventory.clone();
        if !retira(&mut inv_a, &ma.itens) || !retira(&mut inv_b, &mb.itens) {
            self.troca_encerra(a, false, "Trade cancelled: the items changed.");
            return;
        }
        if !entrega(&mut inv_a, &mb.itens) {
            self.troca_encerra(a, false, &format!("Trade cancelled: {} has no room in the bag.", sa.name));
            return;
        }
        if !entrega(&mut inv_b, &ma.itens) {
            self.troca_encerra(a, false, &format!("Trade cancelled: {} has no room in the bag.", sb.name));
            return;
        }
        let (na, nb) = (sa.name.clone(), sb.name.clone());
        tracing::info!(
            "troca: {na} -> {nb}: {:?} +{} ouro | {nb} -> {na}: {:?} +{} ouro",
            ma.itens.iter().map(|i| (i.item_id, i.qtd)).collect::<Vec<_>>(),
            ma.ouro,
            mb.itens.iter().map(|i| (i.item_id, i.qtd)).collect::<Vec<_>>(),
            mb.ouro
        );
        if let Some(s) = self.sessions.get_mut(&a) {
            s.inventory = inv_a;
            s.inventory_dirty = true;
            s.gold = s.gold - ma.ouro + mb.ouro;
        }
        if let Some(s) = self.sessions.get_mut(&b) {
            s.inventory = inv_b;
            s.inventory_dirty = true;
            s.gold = s.gold - mb.ouro + ma.ouro;
        }
        self.save_pending = true;
        crate::telemetria::conta_de(&na, "troca", "feita", 1);
        crate::telemetria::conta_de(&nb, "troca", "feita", 1);
        self.troca_encerra(a, true, "Trade complete!");
    }

    /// Once per tick: invites expire, and a trade closes when the two walk
    /// apart, change island/instance or one of them left.
    pub(super) fn troca_vigia(&mut self) {
        let agora = self.sim_time_s;
        self.troca_convites.retain(|_, (_, ate)| *ate >= agora);
        if self.trocas.is_empty() || self.tick % 10 != 0 {
            return;
        }
        let pares: Vec<(SessionId, SessionId)> =
            self.trocas.iter().map(|(k, m)| (*k, m.parceiro)).collect();
        for (a, b) in pares {
            // The partner's own entry may already be gone (closed with a).
            if self.trocas.contains_key(&a) && !self.perto_pra_trocar(a, b) {
                self.troca_encerra(a, false, "Trade cancelled: too far apart.");
            }
        }
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    fn slot(item_id: u16, qty: u32) -> shared::InventorySlot {
        shared::InventorySlot { item_id, qty, instance: None }
    }

    #[test]
    fn retira_confere_que_o_item_ainda_esta_la() {
        let mut inv = vec![slot(10, 5), slot(0, 0)];
        let mesa = vec![ItemNaMesa { slot: 0, item_id: 10, qtd: 3, instance: None }];
        assert!(retira(&mut inv, &mesa));
        assert_eq!(inv[0].qty, 2);
        // The slot now holds something else: refused.
        let mut inv2 = vec![slot(11, 5)];
        assert!(!retira(&mut inv2, &mesa));
        // Not enough left.
        let mut inv3 = vec![slot(10, 2)];
        assert!(!retira(&mut inv3, &mesa));
    }

    #[test]
    fn item_vinculado_nao_negocia() {
        let mut inst = shared::items::ItemInstance::vazia_de_grau(0);
        inst.vinculado = true;
        let s = shared::InventorySlot { item_id: 1, qty: 1, instance: Some(inst) };
        assert!(!negociavel(&s));
        assert!(negociavel(&slot(1, 1)));
        assert!(!negociavel(&slot(1, 0)));
    }
}
