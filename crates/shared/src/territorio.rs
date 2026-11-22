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

// ─────────────────── phase 4.3: the structures ───────────────────

/// The wall ring is cut into this many segments, each with its own HP; a
/// broken one is a breach. The gate takes one segment's place.
pub const SEGMENTOS_DA_MURALHA: usize = 16;

/// Which segment is the gate: the one an islet's `fase` points at (a fixed,
/// varied side per islet).
pub fn segmento_do_portao(fase: f32) -> usize {
    let a = fase.rem_euclid(std::f32::consts::TAU);
    ((a / std::f32::consts::TAU * SEGMENTOS_DA_MURALHA as f32).round() as usize) % SEGMENTOS_DA_MURALHA
}

/// The angle of segment `i`'s middle (radians, `atan2(y, x)`).
pub fn angulo_do_segmento(i: usize) -> f32 {
    i as f32 / SEGMENTOS_DA_MURALHA as f32 * std::f32::consts::TAU
}

/// The middle of segment `i` of the ring round `centro`.
pub fn pos_do_segmento(centro: glam::Vec2, porte: u8, i: usize) -> glam::Vec2 {
    centro + glam::Vec2::from_angle(angulo_do_segmento(i)) * raio_da_muralha(porte)
}

/// The segment a point at `p` (round `centro`) falls in.
pub fn segmento_em(centro: glam::Vec2, p: glam::Vec2) -> usize {
    let d = p - centro;
    let a = d.y.atan2(d.x).rem_euclid(std::f32::consts::TAU);
    ((a / std::f32::consts::TAU * SEGMENTOS_DA_MURALHA as f32).round() as usize) % SEGMENTOS_DA_MURALHA
}

/// Half the wall's thickness, units (a step into it is a step across it).
pub const MEIA_ESPESSURA_DO_MURO: f32 = 0.9;

/// Does a step from `de` to `para` cross (or enter) the wall ring round
/// `centro`? The segment it meets, if so.
pub fn cruza_a_muralha(centro: glam::Vec2, porte: u8, de: glam::Vec2, para: glam::Vec2) -> Option<usize> {
    let r = raio_da_muralha(porte);
    let (a, b) = (de.distance(centro) - r, para.distance(centro) - r);
    let dentro_do_muro = |x: f32| x.abs() <= MEIA_ESPESSURA_DO_MURO;
    // Moving into the wall's band, or jumping across it.
    let entra = dentro_do_muro(b) && !dentro_do_muro(a) || (a < 0.0) != (b < 0.0);
    if !entra || (dentro_do_muro(a) && dentro_do_muro(b)) {
        return None;
    }
    Some(segmento_em(centro, para))
}

/// Where the cannon plots stand: evenly round the coast, inside it.
pub fn pos_dos_canhoes(centro: glam::Vec2, raio_da_ilha: f32, porte: u8, fase: f32) -> Vec<glam::Vec2> {
    let n = canhoes_do_porte(porte);
    (0..n)
        .map(|k| {
            let a = fase + 0.4 + k as f32 / n as f32 * std::f32::consts::TAU;
            centro + glam::Vec2::from_angle(a) * (raio_da_ilha * 0.72)
        })
        .collect()
}

/// HP of a wall segment, the gate and a cannon at a level (0 = not built).
pub fn hp_do_muro(n: u8) -> i32 {
    if n == 0 { 0 } else { 2_000 + 1_500 * n as i32 }
}
pub fn hp_do_portao(n: u8) -> i32 {
    if n == 0 { 0 } else { 3_000 + 2_500 * n as i32 }
}
pub fn hp_do_canhao(n: u8) -> i32 {
    if n == 0 { 0 } else { 1_500 + 1_000 * n as i32 }
}

#[cfg(test)]
mod testes_das_estruturas {
    use super::*;
    use glam::Vec2;

    #[test]
    fn a_muralha_barra_quem_cruza_e_o_portao_tem_lugar() {
        let c = Vec2::ZERO;
        let r = raio_da_muralha(MEDIA);
        // From inside to outside along +x: crosses segment 0.
        assert_eq!(cruza_a_muralha(c, MEDIA, Vec2::new(r - 2.0, 0.0), Vec2::new(r, 0.0)), Some(0));
        // Walking inside, or outside, never crosses.
        assert_eq!(cruza_a_muralha(c, MEDIA, Vec2::new(5.0, 0.0), Vec2::new(6.0, 0.0)), None);
        assert_eq!(cruza_a_muralha(c, MEDIA, Vec2::new(r + 5.0, 0.0), Vec2::new(r + 6.0, 0.0)), None);
        // A big jump across is caught too.
        assert!(cruza_a_muralha(c, MEDIA, Vec2::new(r - 3.0, 0.0), Vec2::new(r + 3.0, 0.0)).is_some());
        for f in [0.0, 1.0, 2.5, 6.0] {
            assert!(segmento_do_portao(f) < SEGMENTOS_DA_MURALHA);
        }
        assert_eq!(segmento_em(c, pos_do_segmento(c, GRANDE, 5)), 5);
        assert_eq!(pos_dos_canhoes(c, 100.0, GRANDE, 0.0).len(), 6);
        assert!(hp_do_portao(1) > hp_do_muro(1) && hp_do_muro(0) == 0);
    }
}

/// The `EntityKind::Enemy` kinds of the structures (far past any bestiary
/// kind): the client draws a wall, a gate or a cannon for them.
pub const KIND_MURO: u16 = 60_000;
pub const KIND_PORTAO: u16 = 60_001;
pub const KIND_CANHAO: u16 = 60_002;

/// Is enemy kind `k` a territory structure?
pub fn e_estrutura(k: u16) -> bool {
    (KIND_MURO..=KIND_CANHAO).contains(&k)
}

// ─────────────────── phase 4.4: the defenders ───────────────────

/// A defender's role: they rotate warrior, warrior, archer, mage.
pub mod papel_defensor {
    pub const GUERREIRO: u8 = 0;
    pub const ARQUEIRO: u8 = 1;
    pub const MAGO: u8 = 2;
}

/// The role of defender number `i`.
pub fn papel_do_defensor(i: usize) -> u8 {
    [
        papel_defensor::GUERREIRO,
        papel_defensor::GUERREIRO,
        papel_defensor::ARQUEIRO,
        papel_defensor::MAGO,
    ][i % 4]
}

/// How many defenders guard an islet: none at level 0; then the size's
/// base (2/3/4) plus the level.
pub fn quantos_defensores(porte: u8, n: u8) -> usize {
    if n == 0 {
        return 0;
    }
    let base = match porte {
        GRANDE => 4,
        MEDIA => 3,
        _ => 2,
    };
    base + n as usize
}

/// A defender's level: the waters' band, +5 per village level, +3 per
/// defence level (capped at 150).
pub fn nivel_dos_defensores(faixa: u32, vila: u8, n: u8) -> u32 {
    (faixa + 5 * vila as u32 + 3 * n as u32).min(150)
}

/// Where defender `i` of `total` keeps watch: a ring inside the walls.
pub fn pos_do_defensor(centro: glam::Vec2, porte: u8, i: usize, total: usize) -> glam::Vec2 {
    let a = 0.3 + i as f32 / total.max(1) as f32 * std::f32::consts::TAU;
    centro + glam::Vec2::from_angle(a) * (raio_da_muralha(porte) * 0.62)
}

/// How long one stays an INVADER of an islet after a hostile act there
/// (hitting a structure, a defender or a clan member), seconds. Only
/// invaders are attacked by the defenders; gathering is not invading.
pub const INVASOR_S: f32 = 120.0;

#[cfg(test)]
mod testes_dos_defensores {
    use super::*;

    #[test]
    fn defensores_crescem_com_o_nivel() {
        assert_eq!(quantos_defensores(PEQUENA, 0), 0);
        assert_eq!(quantos_defensores(PEQUENA, 1), 3);
        assert_eq!(quantos_defensores(GRANDE, 10), 14);
        assert!(nivel_dos_defensores(100, 4, 10) <= 150);
        assert_eq!(nivel_dos_defensores(40, 2, 1), 53);
        let c = glam::Vec2::ZERO;
        for i in 0..6 {
            assert!(pos_do_defensor(c, MEDIA, i, 6).length() < raio_da_muralha(MEDIA));
        }
    }
}

// ─────────────────── phase 4.5: the cannons fire ───────────────────

/// How far an islet's cannon reaches a ship, units.
pub const ALCANCE_DO_CANHAO: f32 = 45.0;
/// Seconds between two shots of one cannon.
pub const RECARGA_DO_CANHAO_S: f32 = 4.0;

/// A cannonball's hull damage at cannon level `n` (180 at 1, 720 at 10).
pub fn dano_do_canhao(n: u8) -> i32 {
    120 + 60 * n as i32
}
