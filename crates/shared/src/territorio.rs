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

// ─────────────────── phase 4: sizes, levels, layout ───────────────────

/// Islet sizes (`IlhaVista::porte`).
pub const PEQUENA: u8 = 1;
pub const MEDIA: u8 = 2;
pub const GRANDE: u8 = 3;

/// An islet's radius for its size (`f` in 0..1 varies it a little).
pub fn raio_do_porte(porte: u8, f: f32) -> f32 {
    match porte {
        GRANDE => 100.0 + 12.0 * f,
        MEDIA => 72.0 + 8.0 * f,
        _ => 48.0 + 8.0 * f,
    }
}

pub fn nome_do_porte(porte: u8) -> &'static str {
    match porte {
        GRANDE => "Large",
        MEDIA => "Medium",
        _ => "Small",
    }
}

/// The VILLAGE levels (`Governador`): 1 Camp (on claim), 2 Village, 3 Fort,
/// 4 Fortress.
pub const NOMES_DA_VILA: [&str; 4] = ["Camp", "Village", "Fort", "Fortress"];

/// The highest village level of a size: small up to Village, medium up to
/// Fort, large up to Fortress.
pub fn vila_maxima(porte: u8) -> u8 {
    match porte {
        GRANDE => 4,
        MEDIA => 3,
        _ => 2,
    }
}

/// The highest level of every upgrade track (walls, gates, cannons,
/// defenders, wood, stone, energy) of a size.
pub fn trilha_maxima(porte: u8) -> u8 {
    match porte {
        GRANDE => 10,
        MEDIA => 7,
        _ => 4,
    }
}

/// Gold of an upgrade to level `n` (docs/TERRITORIOS.md: 20,000 × N²).
pub fn ouro_do_nivel(n: u8) -> u64 {
    20_000 * (n as u64) * (n as u64)
}

/// Gold of a village level (2 Village, 3 Fort, 4 Fortress).
pub fn ouro_da_vila(n: u8) -> u64 {
    match n {
        2 => 500_000,
        3 => 2_000_000,
        4 => 6_000_000,
        _ => 0,
    }
}

/// The war window of a size, minutes (phase 5): 45 / 60 / 90.
pub fn janela_da_guerra_min(porte: u8) -> u32 {
    match porte {
        GRANDE => 90,
        MEDIA => 60,
        _ => 45,
    }
}

/// The wall ring's radius round the heart, units.
pub fn raio_da_muralha(porte: u8) -> f32 {
    match porte {
        GRANDE => 40.0,
        MEDIA => 30.0,
        _ => 21.0,
    }
}

/// How many cannon plots an islet has on its coast.
pub fn canhoes_do_porte(porte: u8) -> usize {
    match porte {
        GRANDE => 6,
        MEDIA => 4,
        _ => 2,
    }
}
