//! INVISIBILITY on screen (the Assassin's path-tree active, rebirth phase 4,
//! `ServerMessage::Invisibilidade`).
//!
//! The server owns the effect (monsters drop the body, players cannot hit
//! it); this only keeps, per entity, until when it is unseen. The OWNER
//! draws itself faded (every other frame: a dithered ghost, the world
//! material has no per-body alpha), the others do not draw it, its shadow
//! or its name plate, and cannot click it.

use macroquad::prelude::get_time;
use shared::EntityId;
use std::cell::RefCell;
use std::collections::HashMap;

thread_local! {
    static ATE: RefCell<HashMap<EntityId, f64>> = RefCell::new(HashMap::new());
}

/// `segundos` > 0 starts (or refreshes) it; 0 ends it.
pub fn marcar(eid: EntityId, segundos: f32) {
    marcar_em(eid, segundos, get_time());
}

fn marcar_em(eid: EntityId, segundos: f32, agora: f64) {
    ATE.with(|m| {
        let mut m = m.borrow_mut();
        if segundos > 0.0 {
            m.insert(eid, agora + segundos as f64);
        } else {
            m.remove(&eid);
        }
        m.retain(|_, ate| *ate > agora);
    });
}

fn ativa_em(eid: EntityId, agora: f64) -> bool {
    ATE.with(|m| m.borrow().get(&eid).is_some_and(|&ate| ate > agora))
}

/// Is `eid` invisible right now?
pub fn ativa(eid: EntityId) -> bool {
    ativa_em(eid, get_time())
}

/// Hidden from THIS client: invisible and not me.
pub fn oculto(eid: EntityId, eu: Option<EntityId>) -> bool {
    Some(eid) != eu && ativa(eid)
}

/// Skip this frame's body: the others never see it, the owner sees a
/// ghost (half the frames).
pub fn pular_corpo(eid: EntityId, eu: Option<EntityId>) -> bool {
    if !ativa(eid) {
        return false;
    }
    Some(eid) != eu || (get_time() * 60.0) as u64 % 2 == 0
}

/// Forget everything (logout, zone change).
pub fn limpar() {
    ATE.with(|m| m.borrow_mut().clear());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn marca_expira_e_desliga() {
        let a = EntityId(41);
        marcar_em(a, 6.0, 100.0);
        assert!(ativa_em(a, 105.9));
        assert!(!ativa_em(a, 106.1), "runs out by itself");
        marcar_em(a, 6.0, 100.0);
        marcar_em(a, 0.0, 101.0);
        assert!(!ativa_em(a, 101.5), "the server ended it");
        assert!(!ativa_em(EntityId(42), 100.0));
    }
}
