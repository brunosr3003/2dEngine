//! TERRITORIES (docs/TERRITORIOS.md): who holds which territory islet
//! (`mar::ilhotas`), and the rules both sides read.

use serde::{Deserialize, Serialize};

/// One held islet, as the sea and the map show it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Dono {
    /// Index in `mar::ilhotas`.
    pub ilhota: u8,
    pub cla_id: i64,
    /// Clan tag ("" = the clan has none).
    pub tag: String,
    /// Clan name.
    pub cla: String,
}

/// How near the islet's centre (its flag, later its core) a claim is made,
/// units.
pub const RAIO_DO_NUCLEO: f32 = 12.0;

/// A NOMAD clan (no territory) holds at most this many members; a clan with
/// an islet goes up to `social::MAX_CLA` (owner, 20/11/2026: a clan without
/// territory exists, but without the advantages).
pub const MAX_NOMADE: usize = 10;

/// Islets one clan may hold.
pub const ILHOTAS_POR_CLA: usize = 1;

/// The member cap of a clan, with or without territory.
pub fn max_membros(tem_territorio: bool) -> usize {
    if tem_territorio {
        crate::social::MAX_CLA
    } else {
        MAX_NOMADE
    }
}

/// THE CLAIM TAKES TIME (owner, 20/11/2026): two minutes inside the heart's
/// ring with only the clan there. An outsider inside pauses it; no clan
/// member inside cancels it. Seconds.
pub const TEMPO_DE_TOMADA_S: f32 = 120.0;

/// A claim under way, as everyone at sea sees it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TomadaInfo {
    pub ilhota: u8,
    pub tag: String,
    pub cla: String,
    /// Seconds done of `TEMPO_DE_TOMADA_S`.
    pub feito_s: f32,
    /// Someone outside the clan stands in the ring: the clock is paused.
    pub contestada: bool,
}
