//! DIRECT TRADE between two players (owner, 08/11/2026: "a trade to swap
//! directly between players").
//!
//! Server-authoritative, the classic two-step: each side puts items (up to
//! `MAX_ITENS`) and gold on the table; any change un-confirms BOTH sides;
//! each side LOCKS, then CONFIRMS; when both confirmed the server re-checks
//! everything (items still in the bag, room on both sides, not bound, both
//! still close) and swaps it in one go — never half a trade.

use serde::{Deserialize, Serialize};

pub const MAX_ITENS: usize = 8;
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
    /// Put `qtd` of the bag slot `slot` on the table.
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
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum AvisoTroca {
    /// `de` wants to trade with you.
    Convite { de: String },
    /// The table, as this player sees it.
    Estado { eu: Lado, outro: Lado },
    /// The trade ended (done, cancelled, refused…).
    Fim { ok: bool, texto: String },
}
