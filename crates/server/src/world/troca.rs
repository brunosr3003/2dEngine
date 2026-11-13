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
    pub energia: u64,
    pub travado: bool,
    pub confirmado: bool,
}

impl Mesa {
    fn nova(parceiro: SessionId) -> Self {
        Mesa { parceiro, itens: Vec::new(), ouro: 0, energia: 0, travado: false, confirmado: false }
    }
}

/// Whether this bag item may change hands: not bound (the kind in the
/// economy config, or this very piece) and not the Magic Island pass.
fn negociavel(slot: &shared::InventorySlot) -> bool {
    tr::negociavel(slot, crate::economy::item_vinculado(slot.item_id))
}

/// Takes `itens` out of `inv` (by slot, checking the item is still the same
/// and still tradable). `false` = something changed since it went on the table.
fn retira(
    inv: &mut [shared::InventorySlot],
    itens: &[ItemNaMesa],
    tipo_vinculado: &impl Fn(u16) -> bool,
) -> bool {
    for it in itens {
        let Some(s) = inv.get_mut(it.slot as usize) else { return false };
        if it.qtd == 0
            || s.item_id != it.item_id
            || s.qty < it.qtd
            || s.instance != it.instance
            || !tr::negociavel(s, tipo_vinculado(s.item_id))
        {
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

/// One side's holdings at the final check: bag, gold, Energy.
#[derive(Debug, Clone, PartialEq)]
struct Posses {
    inv: Vec<shared::InventorySlot>,
    ouro: u64,
    energia: u64,
}

/// Why the final check refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Falha {
    Ouro,
    Energia,
    Itens,
    /// Side A (0) or B (1) has no room in the bag.
    SemEspaco(u8),
}

/// THE SWAP, on copies: both sides' holdings after the trade, or why not.
/// Nothing is touched unless everything fits — the caller commits both at once.
fn calcula_troca(
    a: &Posses,
    ma: &Mesa,
    b: &Posses,
    mb: &Mesa,
    tipo_vinculado: &impl Fn(u16) -> bool,
) -> Result<(Posses, Posses), Falha> {
    if a.ouro < ma.ouro || b.ouro < mb.ouro {
        return Err(Falha::Ouro);
    }
    if a.energia < ma.energia || b.energia < mb.energia {
        return Err(Falha::Energia);
    }
    let mut inv_a = a.inv.clone();
    let mut inv_b = b.inv.clone();
    if !retira(&mut inv_a, &ma.itens, tipo_vinculado) || !retira(&mut inv_b, &mb.itens, tipo_vinculado) {
        return Err(Falha::Itens);
    }
    if !entrega(&mut inv_a, &mb.itens) {
        return Err(Falha::SemEspaco(0));
    }
    if !entrega(&mut inv_b, &ma.itens) {
        return Err(Falha::SemEspaco(1));
    }
    Ok((
        Posses {
            inv: inv_a,
            ouro: a.ouro - ma.ouro + mb.ouro,
            energia: (a.energia - ma.energia).saturating_add(mb.energia),
        },
        Posses {
            inv: inv_b,
            ouro: b.ouro - mb.ouro + ma.ouro,
            energia: (b.energia - mb.energia).saturating_add(ma.energia),
        },
    ))
}

/// Puts `qtd` of the bag slot `slot` on `m` (replacing what was there; 0
/// takes it off). `Err` = refused, with the text for the player.
fn por_na_mesa(m: &mut Mesa, slot: u16, item: &shared::InventorySlot, qtd: u32, livre: bool) -> Result<bool, &'static str> {
    if !livre {
        return Err("This item can't be traded.");
    }
    let qtd = tr::quantidade_na_mesa(item.qty, qtd);
    let pos = m.itens.iter().position(|i| i.slot == slot);
    match (pos, qtd) {
        (None, 0) => Ok(false),
        (Some(k), 0) => {
            m.itens.remove(k);
            Ok(true)
        }
        (Some(k), q) => {
            let e = &mut m.itens[k];
            // Same slot but a different item now: put the new one instead.
            let novo = ItemNaMesa { slot, item_id: item.item_id, qtd: q, instance: item.instance.clone() };
            if *e == novo {
                return Ok(false);
            }
            *e = novo;
            Ok(true)
        }
        (None, q) => {
            if m.itens.len() >= tr::MAX_ITENS {
                return Err("The table is full.");
            }
            m.itens.push(ItemNaMesa { slot, item_id: item.item_id, qtd: q, instance: item.instance.clone() });
            Ok(true)
        }
    }
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
                energia: m.energia,
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
                if !self.trocas.contains_key(&sid) {
                    return;
                }
                let Some(item) = self.sessions.get(&sid).and_then(|s| s.inventory.get(slot as usize)).copied() else {
                    return;
                };
                let livre = negociavel(&item);
                let m = self.trocas.get_mut(&sid).unwrap();
                match por_na_mesa(m, slot, &item, qtd, livre) {
                    Ok(true) => self.troca_mexeu(sid),
                    Ok(false) => {}
                    Err(texto) => self.troca_fim_sem_fechar(sid, texto),
                }
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
                let novo = tr::saldo_na_mesa(tem, qtd);
                if m.ouro != novo {
                    m.ouro = novo;
                    self.troca_mexeu(sid);
                } else {
                    self.troca_envia_estado(sid);
                }
            }
            PedidoTroca::Energia { qtd } => {
                let tem = self.sessions.get(&sid).map_or(0, |s| s.skill_progress.energia);
                let Some(m) = self.trocas.get_mut(&sid) else { return };
                let novo = tr::saldo_na_mesa(tem, qtd);
                if m.energia != novo {
                    m.energia = novo;
                    self.troca_mexeu(sid);
                } else {
                    self.troca_envia_estado(sid);
                }
            }
            PedidoTroca::Travar => {
                if !self.trocas.contains_key(&sid) {
                    return;
                }
                if !self.troca_oferta_valida(sid) {
                    self.troca_fim_sem_fechar(sid, "Your offer changed: check it and lock again.");
                    self.troca_mexeu(sid);
                    return;
                }
                let m = self.trocas.get_mut(&sid).unwrap();
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
                // Re-checked on confirm: an offer that went stale (the item
                // moved, the gold was spent) unlocks both instead of closing.
                if !self.troca_oferta_valida(sid) || !self.troca_oferta_valida(parceiro) {
                    for k in [sid, parceiro] {
                        self.troca_fim_sem_fechar(k, "An offer changed: check it and lock again.");
                    }
                    self.troca_mexeu(sid);
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

    /// The side of `sid` still holds what it offers: the same items in the
    /// same slots, still tradable, and the gold and Energy.
    fn troca_oferta_valida(&self, sid: SessionId) -> bool {
        let (Some(m), Some(s)) = (self.trocas.get(&sid), self.sessions.get(&sid)) else {
            return false;
        };
        let mut copia = s.inventory.clone();
        s.gold >= m.ouro
            && s.skill_progress.energia >= m.energia
            && retira(&mut copia, &m.itens, &crate::economy::item_vinculado)
    }

    /// A refusal that doesn't close the table.
    fn troca_fim_sem_fechar(&self, sid: SessionId, texto: &str) {
        self.troca_aviso(sid, false, texto);
    }

    /// A notice to the trade window (and the chat) that keeps the table open.
    fn troca_aviso(&self, sid: SessionId, ok: bool, texto: &str) {
        if let Some(s) = self.sessions.get(&sid) {
            let _ = s.handle.to_client.send(ServerMessage::Troca {
                aviso: AvisoTroca::Aviso { ok, texto: texto.into() },
            });
        }
    }

    fn troca_convidar(&mut self, sid: SessionId, nome: &str) {
        if self.trocas.contains_key(&sid) {
            self.troca_aviso(sid, false, "You are already trading.");
            return;
        }
        let alvo = self
            .sessions
            .iter()
            .find(|(k, s)| **k != sid && s.logged_in && s.name.eq_ignore_ascii_case(nome.trim()))
            .map(|(k, _)| *k);
        let Some(alvo) = alvo else {
            self.troca_aviso(sid, false, "The character is not on this channel.");
            return;
        };
        if self.trocas.contains_key(&alvo) {
            self.troca_aviso(sid, false, "That player is already trading.");
            return;
        }
        if !self.perto_pra_trocar(sid, alvo) {
            self.troca_aviso(sid, false, "Get closer to trade.");
            return;
        }
        let de = self.sessions[&sid].name.clone();
        self.troca_convites.insert(alvo, (sid, self.sim_time_s + tr::CONVITE_S));
        if let Some(s) = self.sessions.get(&alvo) {
            let _ = s.handle.to_client.send(ServerMessage::Troca { aviso: AvisoTroca::Convite { de } });
        }
        self.troca_aviso(sid, true, "Trade invite sent.");
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
        let pa = Posses { inv: sa.inventory.clone(), ouro: sa.gold, energia: sa.skill_progress.energia };
        let pb = Posses { inv: sb.inventory.clone(), ouro: sb.gold, energia: sb.skill_progress.energia };
        let (na, nb) = (sa.name.clone(), sb.name.clone());
        let (fa, fb) = match calcula_troca(&pa, &ma, &pb, &mb, &crate::economy::item_vinculado) {
            Ok(v) => v,
            Err(f) => {
                let texto = match f {
                    Falha::Ouro => "Trade cancelled: not enough gold.".to_string(),
                    Falha::Energia => "Trade cancelled: not enough Energy.".to_string(),
                    Falha::Itens => "Trade cancelled: the items changed.".to_string(),
                    Falha::SemEspaco(0) => format!("Trade cancelled: {na} has no room in the bag."),
                    Falha::SemEspaco(_) => format!("Trade cancelled: {nb} has no room in the bag."),
                };
                self.troca_encerra(a, false, &texto);
                return;
            }
        };
        tracing::info!(
            "troca: {na} -> {nb}: {:?} +{} ouro +{} energia | {nb} -> {na}: {:?} +{} ouro +{} energia",
            ma.itens.iter().map(|i| (i.item_id, i.qtd)).collect::<Vec<_>>(),
            ma.ouro,
            ma.energia,
            mb.itens.iter().map(|i| (i.item_id, i.qtd)).collect::<Vec<_>>(),
            mb.ouro,
            mb.energia
        );
        let energia_mexeu = ma.energia > 0 || mb.energia > 0;
        for (sid, p) in [(a, fa), (b, fb)] {
            if let Some(s) = self.sessions.get_mut(&sid) {
                s.inventory = p.inv;
                s.inventory_dirty = true;
                s.gold = p.ouro;
                if energia_mexeu {
                    s.skill_progress.energia = p.energia;
                    s.skills_dirty = true;
                    let _ = s.handle.to_client.send(ServerMessage::ProgressoDeSkills {
                        progresso: s.skill_progress.clone(),
                    });
                }
            }
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

    fn livre(_: u16) -> bool {
        false
    }

    fn ninguem() -> SessionId {
        SessionId("127.0.0.1:1".parse().unwrap())
    }

    fn mesa(itens: Vec<ItemNaMesa>, ouro: u64, energia: u64) -> Mesa {
        Mesa { parceiro: ninguem(), itens, ouro, energia, travado: true, confirmado: true }
    }

    fn na_mesa(slot: u16, item_id: u16, qtd: u32) -> ItemNaMesa {
        ItemNaMesa { slot, item_id, qtd, instance: None }
    }

    /// A bag of `n` grid slots plus the wallet (copper, darksteel).
    fn bolsa(n: usize, darksteel: u32) -> Vec<shared::InventorySlot> {
        let mut inv = vec![shared::InventorySlot::default(); n];
        inv.push(shared::InventorySlot::default());
        inv.push(if darksteel > 0 { slot(shared::item_id::DARKSTEEL, darksteel) } else { Default::default() });
        inv
    }

    #[test]
    fn retira_confere_que_o_item_ainda_esta_la() {
        let mut inv = vec![slot(10, 5), slot(0, 0)];
        let m = vec![na_mesa(0, 10, 3)];
        assert!(retira(&mut inv, &m, &livre));
        assert_eq!(inv[0].qty, 2);
        // The slot now holds something else: refused.
        let mut inv2 = vec![slot(11, 5)];
        assert!(!retira(&mut inv2, &m, &livre));
        // Not enough left.
        let mut inv3 = vec![slot(10, 2)];
        assert!(!retira(&mut inv3, &m, &livre));
        // A bound kind is refused at the final check too.
        let mut inv4 = vec![slot(10, 5)];
        assert!(!retira(&mut inv4, &m, &|id| id == 10));
    }

    #[test]
    fn item_vinculado_nao_negocia() {
        let mut inst = shared::items::ItemInstance::vazia_de_grau(0);
        inst.vinculado = true;
        let s = shared::InventorySlot { item_id: 1, qty: 1, instance: Some(inst) };
        assert!(!tr::negociavel(&s, false));
        assert!(tr::negociavel(&slot(10, 1), false));
        assert!(!tr::negociavel(&slot(10, 0), false));
        assert!(!tr::negociavel(&slot(shared::item_id::PASSE_MAGICO, 1), false));
    }

    #[test]
    fn por_na_mesa_define_a_quantidade_digitada() {
        let mut m = Mesa::nova(ninguem());
        let pilha = slot(shared::item_id::DARKSTEEL, 500);
        // Typed 120 of 500.
        assert_eq!(por_na_mesa(&mut m, 41, &pilha, 120, true), Ok(true));
        assert_eq!(m.itens[0].qtd, 120);
        // Typed again: REPLACES (not adds).
        assert_eq!(por_na_mesa(&mut m, 41, &pilha, 300, true), Ok(true));
        assert_eq!(m.itens.len(), 1);
        assert_eq!(m.itens[0].qtd, 300);
        // More than the stack: clamped to it.
        assert_eq!(por_na_mesa(&mut m, 41, &pilha, 9_999, true), Ok(true));
        assert_eq!(m.itens[0].qtd, 500);
        // The same amount again changes nothing (nobody gets unlocked).
        assert_eq!(por_na_mesa(&mut m, 41, &pilha, 500, true), Ok(false));
        // Zero takes it off.
        assert_eq!(por_na_mesa(&mut m, 41, &pilha, 0, true), Ok(true));
        assert!(m.itens.is_empty());
        // Bound: refused, nothing on the table.
        assert!(por_na_mesa(&mut m, 3, &slot(10, 1), 1, false).is_err());
        assert!(m.itens.is_empty());
    }

    #[test]
    fn mesa_cheia_recusa_o_proximo_item() {
        let mut m = Mesa::nova(ninguem());
        for k in 0..tr::MAX_ITENS as u16 {
            assert_eq!(por_na_mesa(&mut m, k, &slot(100 + k, 1), 1, true), Ok(true));
        }
        assert!(por_na_mesa(&mut m, 99, &slot(200, 1), 1, true).is_err());
        // Changing one already there still works.
        assert_eq!(por_na_mesa(&mut m, 0, &slot(100, 4), 3, true), Ok(true));
        assert_eq!(m.itens[0].qtd, 3);
    }

    #[test]
    fn troca_completa_com_darksteel_energia_e_ouro() {
        let mut inv_a = bolsa(4, 500);
        inv_a[0] = slot(10, 3);
        let a = Posses { inv: inv_a, ouro: 1_000, energia: 7_000 };
        let b = Posses { inv: bolsa(4, 0), ouro: 50_000, energia: 0 };
        let n = a.inv.len() as u16;
        let ma = mesa(vec![na_mesa(0, 10, 2), na_mesa(n - 1, shared::item_id::DARKSTEEL, 200)], 0, 5_000);
        let mb = mesa(vec![], 30_000, 0);
        let (fa, fb) = calcula_troca(&a, &ma, &b, &mb, &livre).expect("trade fits");
        assert_eq!(fa.inv[0].qty, 1);
        assert_eq!(fa.inv[fa.inv.len() - 1].qty, 300);
        assert_eq!((fa.ouro, fa.energia), (31_000, 2_000));
        assert_eq!((fb.ouro, fb.energia), (20_000, 5_000));
        // B got 2 of item 10 in the grid and 200 Darksteel in the wallet.
        assert_eq!(fb.inv.iter().filter(|s| s.item_id == 10).map(|s| s.qty).sum::<u32>(), 2);
        let w = fb.inv.len() - 1;
        assert_eq!((fb.inv[w].item_id, fb.inv[w].qty), (shared::item_id::DARKSTEEL, 200));
    }

    #[test]
    fn energia_ou_ouro_que_faltam_recusam_tudo() {
        let a = Posses { inv: bolsa(2, 0), ouro: 100, energia: 10 };
        let b = Posses { inv: bolsa(2, 0), ouro: 0, energia: 0 };
        let nada = mesa(vec![], 0, 0);
        assert_eq!(calcula_troca(&a, &mesa(vec![], 0, 11), &b, &nada, &livre), Err(Falha::Energia));
        assert_eq!(calcula_troca(&a, &mesa(vec![], 101, 0), &b, &nada, &livre), Err(Falha::Ouro));
        assert!(calcula_troca(&a, &mesa(vec![], 100, 10), &b, &nada, &livre).is_ok());
    }

    #[test]
    fn item_vinculado_ou_pilha_que_mudou_recusa_na_hora_de_trocar() {
        let mut inv = bolsa(2, 0);
        inv[0] = slot(10, 5);
        let a = Posses { inv, ouro: 0, energia: 0 };
        let b = Posses { inv: bolsa(2, 0), ouro: 0, energia: 0 };
        let nada = mesa(vec![], 0, 0);
        // More than the stack holds now.
        assert_eq!(calcula_troca(&a, &mesa(vec![na_mesa(0, 10, 6)], 0, 0), &b, &nada, &livre), Err(Falha::Itens));
        // The kind became bound.
        assert_eq!(
            calcula_troca(&a, &mesa(vec![na_mesa(0, 10, 5)], 0, 0), &b, &nada, &|id| id == 10),
            Err(Falha::Itens)
        );
        // The Magic Island pass never goes.
        let mut inv_p = bolsa(2, 0);
        inv_p[0] = slot(shared::item_id::PASSE_MAGICO, 1);
        let ap = Posses { inv: inv_p, ouro: 0, energia: 0 };
        assert_eq!(
            calcula_troca(&ap, &mesa(vec![na_mesa(0, shared::item_id::PASSE_MAGICO, 1)], 0, 0), &b, &nada, &livre),
            Err(Falha::Itens)
        );
    }

    #[test]
    fn bolsa_cheia_do_outro_lado_recusa_sem_mexer_em_nada() {
        let a = Posses { inv: vec![slot(10, 1)], ouro: 0, energia: 0 };
        let inv_b = vec![slot(20, 1)];
        let b = Posses { inv: inv_b, ouro: 0, energia: 0 };
        // B gives nothing back, so the full grid has no room for item 10.
        let r = calcula_troca(&a, &mesa(vec![na_mesa(0, 10, 1)], 0, 0), &b, &mesa(vec![], 0, 0), &livre);
        assert_eq!(r, Err(Falha::SemEspaco(1)));
    }
}
