//! DIRECT TRADE between two players (owner, 08/11/2026: "a trade to swap
//! directly between players"; redone 13/11/2026: "it doesn't show all the
//! items, Energy and Darksteel aren't tradable, there must be a way to type
//! the number of items").
//!
//! Server-authoritative, the classic two-step: each side puts items (up to
//! `MAX_ITENS`), gold and Energy on the table; any change un-locks BOTH
//! sides; each side LOCKS, then CONFIRMS; when both confirmed the server
//! re-checks everything (items still in the bag, room on both sides, not
//! bound, balances, both still close) and swaps it in one go — never half a
//! trade.
//!
//! Darksteel and Copper live in the bag's wallet (`armazem::CARTEIRA`), so
//! they are offered like any other stack, by their slot. Energy is a counter
//! on the character, not an item: it has its own amount (`Lado::energia`).

use serde::{Deserialize, Serialize};

use crate::constants::item_id;
use crate::InventorySlot;

pub const MAX_ITENS: usize = 12;
/// Both players must stay this close (world units) for the whole trade.
pub const DISTANCIA_MAX: f32 = 12.0;
/// How long an invite waits for an answer.
pub const CONVITE_S: f32 = 30.0;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum PedidoTroca {
    /// Invite the player with this name (must be close by).
    Convidar { nome: String },
    /// Answer the invite from `de`.
    Responder { de: String, aceita: bool },
    /// Put exactly `qtd` of the bag slot `slot` on the table (replaces the
    /// amount already there; 0 takes it off).
    Por { slot: u16, qtd: u32 },
    /// Take the table item `indice` back.
    Tirar { indice: u8 },
    /// Gold offered (replaces the previous amount).
    Ouro { qtd: u64 },
    /// Lock my side (I'm done changing it).
    Travar,
    /// Final confirm (both sides locked).
    Confirmar,
    Cancelar,
    /// Energy offered (replaces the previous amount).
    Energia { qtd: u64 },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ItemNaMesa {
    pub slot: u16,
    pub item_id: u16,
    pub qtd: u32,
    pub instance: Option<crate::items::ItemInstance>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Lado {
    pub nome: String,
    pub itens: Vec<ItemNaMesa>,
    pub ouro: u64,
    pub travado: bool,
    pub confirmado: bool,
    /// Energy offered.
    pub energia: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum AvisoTroca {
    /// `de` wants to trade with you.
    Convite { de: String },
    /// The table, as this player sees it.
    Estado { eu: Lado, outro: Lado },
    /// The trade ended (done, cancelled, refused…).
    Fim { ok: bool, texto: String },
    /// A notice that does NOT close the table (a refused change, an invite
    /// sent or refused).
    Aviso { ok: bool, texto: String },
}

/// Whether this bag slot may change hands. `tipo_vinculado` is the item
/// KIND's bound flag (`ItemsConfig`: chests, scrolls, reward potions); the
/// piece itself may be bound too (dungeon chest gear). The Magic Island pass
/// is bound whatever the config says (TP shop only, owner 12/11/2026), and
/// loose gold never sits in the bag (it is the gold balance).
pub fn negociavel(slot: &InventorySlot, tipo_vinculado: bool) -> bool {
    slot.qty > 0
        && slot.item_id != 0
        && slot.item_id != item_id::GOLD
        && slot.item_id != item_id::PASSE_MAGICO
        && !tipo_vinculado
        && !slot.instance.as_ref().is_some_and(|i| i.vinculado)
}

/// How much of a slot holding `tem` can be on the table when `pedida` is
/// asked: never more than the stack.
pub fn quantidade_na_mesa(tem: u32, pedida: u32) -> u32 {
    pedida.min(tem)
}

/// Gold or Energy offered: never more than the balance.
pub fn saldo_na_mesa(tem: u64, pedido: u64) -> u64 {
    pedido.min(tem)
}

#[cfg(test)]
mod testes {
    use super::*;

    fn s(item_id: u16, qty: u32) -> InventorySlot {
        InventorySlot { item_id, qty, instance: None }
    }

    #[test]
    fn darksteel_cobre_e_material_negociam() {
        assert!(negociavel(&s(item_id::DARKSTEEL, 500), false));
        assert!(negociavel(&s(item_id::COPPER, 3), false));
        assert!(negociavel(&s(item_id::DARKSTEEL, 1), false));
    }

    #[test]
    fn vinculado_passe_ouro_e_vazio_nao_negociam() {
        assert!(!negociavel(&s(item_id::PASSE_MAGICO, 1), false));
        assert!(!negociavel(&s(item_id::GOLD, 100), false));
        assert!(!negociavel(&s(10, 0), false));
        assert!(!negociavel(&s(0, 5), false));
        // The kind is bound.
        assert!(!negociavel(&s(10, 1), true));
        // This very piece is bound.
        let mut inst = crate::items::ItemInstance::vazia_de_grau(0);
        inst.vinculado = true;
        assert!(!negociavel(&InventorySlot { item_id: 10, qty: 1, instance: Some(inst) }, false));
    }

    #[test]
    fn quantidade_e_saldo_presos_ao_que_tem() {
        assert_eq!(quantidade_na_mesa(40, 25), 25);
        assert_eq!(quantidade_na_mesa(40, 999), 40);
        assert_eq!(quantidade_na_mesa(40, 0), 0);
        assert_eq!(saldo_na_mesa(5_000, 12_000), 5_000);
        assert_eq!(saldo_na_mesa(5_000, 1_234), 1_234);
    }
}
