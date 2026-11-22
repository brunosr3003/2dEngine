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
    /// Phase 4: every upgrade track's level (`Trilha` index). Appended.
    pub niveis: [u8; N_TRILHAS],
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

// ─────────────────── phase 4: the upgrade tracks ───────────────────

/// How many upgrade tracks an islet has.
pub const N_TRILHAS: usize = 8;

/// The tracks, `Dono::niveis` index. The GOVERNOR's first, then the FARMER's.
pub mod trilha {
    pub const VILA: usize = 0;
    pub const MUROS: usize = 1;
    pub const PORTOES: usize = 2;
    pub const CANHOES: usize = 3;
    pub const DEFENSORES: usize = 4;
    pub const MADEIRA: usize = 5;
    pub const PEDRA: usize = 6;
    pub const ENERGIA: usize = 7;
}

pub const NOMES_DAS_TRILHAS: [&str; N_TRILHAS] = [
    "Village", "Walls", "Gates", "Cannons", "Defenders", "Lumber", "Quarry", "Energy",
];

/// A freshly claimed islet: a Camp, nothing built.
pub const NIVEIS_INICIAIS: [u8; N_TRILHAS] = [1, 0, 0, 0, 0, 0, 0, 0];

/// The Governor handles the village and the defences; the Farmer the rest.
pub fn do_governador(t: usize) -> bool {
    t <= trilha::DEFENSORES
}

/// The highest a track may go now: the size's cap, and (for every track but
/// the village) what the village level allows — Camp 2, Village 4, Fort 7,
/// Fortress 10. Walls, gates and cannons need at least a Village.
pub fn teto(t: usize, porte: u8, vila: u8) -> u8 {
    if t == trilha::VILA {
        return vila_maxima(porte);
    }
    if matches!(t, trilha::MUROS | trilha::PORTOES | trilha::CANHOES) && vila < 2 {
        return 0;
    }
    let pela_vila = match vila {
        0 | 1 => 2,
        2 => 4,
        3 => 7,
        _ => 10,
    };
    trilha_maxima(porte).min(pela_vila)
}

/// What raising track `t` to level `n` costs: materials and gold.
pub fn custo(t: usize, n: u8) -> (Vec<(u16, u32)>, u64) {
    use crate::item_id as it;
    let n32 = n as u32;
    // The material's tier follows the level: 1-3 T1, 4-6 T2, 7-9 T3, 10 T4.
    let tier = (1 + (n.saturating_sub(1)) / 3).min(4);
    let madeira = it::WOOD_T1 + tier as u16 - 1;
    let aco = it::na_cor(it::STEEL, tier);
    if t == trilha::VILA {
        let tv = n.clamp(1, 4);
        return (
            vec![
                (it::WOOD_T1 + tv as u16 - 1, 1_000 * n32),
                (it::na_cor(it::STEEL, tv), 300 * n32),
                (it::PRANCHA_DE_NAUFRAGIO, 100 * n32),
            ],
            ouro_da_vila(n),
        );
    }
    let mats = match t {
        trilha::MUROS => vec![(madeira, 200 * n32), (aco, 60 * n32)],
        trilha::PORTOES => vec![(madeira, 150 * n32), (aco, 80 * n32)],
        trilha::CANHOES => vec![(aco, 100 * n32), (it::DARKSTEEL, 150 * n32)],
        trilha::DEFENSORES => vec![(it::DARKSTEEL, 200 * n32), (it::COPPER, 2_000 * n32)],
        trilha::MADEIRA => vec![(madeira, 300 * n32)],
        trilha::PEDRA => vec![(it::DARKSTEEL, 250 * n32)],
        _ => vec![(aco, 120 * n32), (it::PRANCHA_DE_NAUFRAGIO, 20 * n32)],
    };
    (mats, ouro_do_nivel(n))
}

/// Where a held islet's Governor and Farmer stand: beside the heart.
pub fn pos_do_governador(centro: glam::Vec2) -> glam::Vec2 {
    centro + glam::Vec2::new(4.5, 0.0)
}
pub fn pos_do_fazendeiro(centro: glam::Vec2) -> glam::Vec2 {
    centro + glam::Vec2::new(-4.5, 0.0)
}

/// How near the Governor or the Farmer one must stand to deal with them.
pub const ALCANCE_DO_NPC: f32 = 4.0;

#[cfg(test)]
mod testes_da_fase_4 {
    use super::*;

    #[test]
    fn tetos_e_custos_fazem_sentido() {
        // Small: up to Village, tracks up to 4; medium Fort/7; large Fortress/10.
        assert_eq!((teto(trilha::VILA, PEQUENA, 1), teto(trilha::VILA, GRANDE, 1)), (2, 4));
        assert_eq!(teto(trilha::MUROS, GRANDE, 1), 0, "no walls in a camp");
        assert_eq!(teto(trilha::MUROS, GRANDE, 2), 4);
        assert_eq!(teto(trilha::MUROS, PEQUENA, 2), 4);
        assert_eq!(teto(trilha::MUROS, MEDIA, 3), 7);
        assert_eq!(teto(trilha::MUROS, GRANDE, 4), 10);
        assert_eq!(teto(trilha::DEFENSORES, GRANDE, 1), 2);
        for t in 0..N_TRILHAS {
            for n in 1..=10 {
                let (mats, ouro) = custo(t, n);
                assert!(!mats.is_empty() && mats.iter().all(|(_, q)| *q > 0));
                if t != trilha::VILA {
                    assert_eq!(ouro, ouro_do_nivel(n));
                }
            }
        }
        assert_eq!(ouro_do_nivel(10), 2_000_000);
    }
}
