//! REBIRTH ("renascimento", docs/RESET.md, rewards decided 15/11/2026).
//!
//! At the level cap (`progressao::NIVEL_MAX`, internal 60 = shown 100) a
//! character can be reborn: level, XP and allocated attribute points go back
//! to a new character's, and everything else stays (items, gear, gold, TP,
//! pets, mounts, proficiencies, story, skill tiers). Each rebirth:
//!
//! - divides every XP gained afterwards by `divisor_de_renascimento` (1.5^r);
//! - multiplies the attribute-derived stats by `mult_de_atributos`
//!   (+5% per rebirth, stacking, no cap);
//! - pays Rebirth Essence (`essencia_do_renascimento`) and the rebirth chest
//!   (`bau_do_renascimento`);
//! - may reach a MILESTONE (`e_marco`, `marco_de`), announced to everyone.
//!
//! Every rule lives here so the server, the client sheet and the tests read
//! the same numbers. Phase 1 only: the path tree, the Essence shop, gear
//! ascension and skill slots 4-6 come later; they can read what a character
//! reached from its count alone (`marcos_ate`), so no extra column is needed.

use crate::constants::item_id;

/// Base of the XP divisor: each rebirth makes the next climb 1.5x heavier
/// (2nd climb 1.5x, 3rd 2.25x, 4th 3.4x...). The owner's suggestion over
/// `2^r`, which doubled too fast for "many rebirths".
pub const DIVISOR_BASE: f64 = 1.5;

/// Attribute bonus per rebirth: +5%, stacking, no cap (+50% at the 10th).
pub const BONUS_POR_RENASCIMENTO: f32 = 0.05;

/// Essence per rebirth number: the n-th rebirth pays `ESSENCIA_POR_NUMERO * n`
/// (10, 20, 30...): the high rebirths pay more, as the design asks.
pub const ESSENCIA_POR_NUMERO: u32 = 10;

/// The rebirth on which the bag grows (+1 bag expansion = +10 slots).
pub const MARCO_BOLSA: u32 = 2;
/// The rebirth on which the bank grows (+1 bank expansion = +10 slots).
pub const MARCO_BANCO: u32 = 7;

/// Mail reason (`CartaDeCorreio::motivo`) of the part of the rebirth chest
/// that did not fit the bag (1-2 dungeon, 3 attendance, 4 shop).
pub const MOTIVO_CORREIO_RENASCIMENTO: u8 = 5;

/// Can a character of INTERNAL level `nivel` be reborn?
pub fn pode_renascer(nivel: u32) -> bool {
    nivel >= crate::progressao::NIVEL_MAX
}

/// The XP divisor of a character with `r` rebirths (1 with none).
pub fn divisor_de_renascimento(r: u32) -> f64 {
    DIVISOR_BASE.powi(r.min(200) as i32)
}

/// `xp` after the rebirth divisor (never rounds a gain away to 0).
pub fn xp_depois_do_divisor(xp: u64, r: u32) -> u64 {
    if xp == 0 || r == 0 {
        return xp;
    }
    ((xp as f64 / divisor_de_renascimento(r)).round() as u64).max(1)
}

/// The multiplier of the attribute-derived stats (attack, defense, max HP,
/// max MP, dexterity, wisdom) with `r` rebirths.
pub fn mult_de_atributos(r: u32) -> f32 {
    1.0 + BONUS_POR_RENASCIMENTO * r as f32
}

/// The bonus in whole percent, for the sheet ("+15%").
pub fn bonus_em_pct(r: u32) -> u32 {
    (BONUS_POR_RENASCIMENTO * 100.0).round() as u32 * r
}

/// Rebirth Essence paid by the `n`-th rebirth (n >= 1).
pub fn essencia_do_renascimento(n: u32) -> u32 {
    ESSENCIA_POR_NUMERO.saturating_mul(n.max(1))
}

/// Is the `n`-th rebirth a milestone announced to everyone online? 1, 3, 5,
/// 7, 10, then every 5 from 15 (15, 20, 25...).
pub fn e_marco(n: u32) -> bool {
    matches!(n, 1 | 3 | 5 | 7 | 10) || (n >= 15 && n % 5 == 0)
}

/// Every milestone rebirth up to `r` (later phases grant titles, skins and
/// skill slots from this, retroactively).
pub fn marcos_ate(r: u32) -> impl Iterator<Item = u32> {
    (1..=r).filter(|&n| e_marco(n))
}

/// The next milestone after `r` rebirths (the bag one included).
pub fn proximo_marco(r: u32) -> u32 {
    (r + 1..).find(|&n| e_marco(n) || n == MARCO_BOLSA).unwrap_or(r + 1)
}

/// What the `n`-th rebirth unlocks in THIS phase, in English (the client
/// translates): the aura tier and the bag/bank slots. `None` = nothing new
/// besides the fixed rewards. The table's titles, name color, skins, the
/// path tree and skill slots 4-6 are later phases (docs/RESET.md).
pub fn marco_de(n: u32) -> Option<&'static str> {
    Some(match n {
        1 => "Green aura",
        MARCO_BOLSA => "+10 bag slots",
        3 => "Blue aura",
        5 => "Purple aura",
        MARCO_BANCO => "+10 bank slots",
        10 => "Golden aura",
        15 => "Black hole aura",
        n if n >= 20 && n % 5 == 0 => "Stronger aura",
        _ => return None,
    })
}

/// The aura tier of `r` rebirths: 0 none, 1 green (1-2), 2 blue (3-4),
/// 3 purple (5-9), 4 gold (10-14), 5 black hole (15+).
pub fn tier_da_aura(r: u32) -> u8 {
    match r {
        0 => 0,
        1..=2 => 1,
        3..=4 => 2,
        5..=9 => 3,
        10..=14 => 4,
        _ => 5,
    }
}

/// How strong the aura shines: 1 up to 19 rebirths, then +25% every 5
/// (20 → 1.25, 25 → 1.5...), capped at 3 so it never swallows the body.
pub fn intensidade_da_aura(r: u32) -> f32 {
    if r < 20 {
        1.0
    } else {
        (1.0 + 0.25 * ((r - 15) / 5) as f32).min(3.0)
    }
}

/// The COLOR of the `n`-th rebirth's chest (the keys' scale: 2 green, 3 blue,
/// 4 purple, 5 legendary), following the aura: green 1-2, blue 3-4, purple
/// 5-9, legendary from 10.
pub fn cor_do_bau(n: u32) -> u8 {
    match n {
        0..=2 => 2,
        3..=4 => 3,
        5..=9 => 4,
        _ => 5,
    }
}

/// The craft materials the chest rotates through (the eight non-key ones of
/// `MATERIAIS_COLORIDOS`).
const MATERIAIS_DO_BAU: [u16; 8] = [
    item_id::STEEL,
    item_id::DARK_HEART_STONE,
    item_id::MOON_SHADOW_STONE,
    item_id::QUINTESSENCE,
    item_id::EXORCISM_BAUBLE,
    item_id::PLATINUM,
    item_id::ILLUMINATING_FRAGMENT,
    item_id::ANIMA_STONE,
];

/// THE REBIRTH CHEST of the `n`-th rebirth, as (item, amount), deterministic:
///
/// - Darksteel: the top of a cap-level boss chest (`darksteel_do_bau`
///   at `NIVEL_MAX`), +50% per rebirth after the first;
/// - one craft key in the chest's color (two on a milestone), rotating
///   Scale → Claw → Horn → Hide;
/// - `20 + 5n` (max 120) of one craft material in the chest's color (purple
///   at most: materials have four colors), rotating through the eight.
///
/// Essence is paid apart (`essencia_do_renascimento`).
pub fn bau_do_renascimento(n: u32) -> Vec<(u16, u32)> {
    let n = n.max(1);
    let cor = cor_do_bau(n);
    let (_, ds) = crate::dungeon::darksteel_do_bau(crate::progressao::NIVEL_MAX);
    let darksteel = ds.saturating_add(ds.saturating_mul(n - 1) / 2);
    let i = (n - 1) as usize;
    let chave = item_id::chave_na_cor(item_id::CHAVES[i % 4], cor);
    let material = item_id::na_cor(MATERIAIS_DO_BAU[i % MATERIAIS_DO_BAU.len()], cor.min(4));
    vec![
        (item_id::DARKSTEEL, darksteel),
        (chave, if e_marco(n) { 2 } else { 1 }),
        (material, (20 + 5 * n).min(120)),
    ]
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn divisor_cresce_1_5_por_renascimento() {
        assert_eq!(divisor_de_renascimento(0), 1.0);
        assert_eq!(divisor_de_renascimento(1), 1.5);
        assert!((divisor_de_renascimento(3) - 3.375).abs() < 1e-9);
        assert_eq!(xp_depois_do_divisor(300, 0), 300);
        assert_eq!(xp_depois_do_divisor(300, 1), 200);
        assert_eq!(xp_depois_do_divisor(1, 5), 1, "never rounds a gain away");
        assert_eq!(xp_depois_do_divisor(0, 5), 0);
        assert!(divisor_de_renascimento(10_000).is_finite());
    }

    #[test]
    fn bonus_de_5_pct_soma_sem_teto() {
        assert_eq!(mult_de_atributos(0), 1.0);
        assert!((mult_de_atributos(1) - 1.05).abs() < 1e-6);
        assert!((mult_de_atributos(10) - 1.5).abs() < 1e-6);
        assert!((mult_de_atributos(40) - 3.0).abs() < 1e-6);
        assert_eq!(bonus_em_pct(7), 35);
    }

    #[test]
    fn so_renasce_no_teto() {
        assert!(!pode_renascer(crate::progressao::NIVEL_MAX - 1));
        assert!(pode_renascer(crate::progressao::NIVEL_MAX));
    }

    #[test]
    fn essencia_cresce_com_o_numero() {
        assert_eq!(essencia_do_renascimento(1), 10);
        assert_eq!(essencia_do_renascimento(7), 70);
        for n in 1..50 {
            assert!(essencia_do_renascimento(n + 1) > essencia_do_renascimento(n));
        }
    }

    #[test]
    fn marcos_da_tabela() {
        let ate_30: Vec<u32> = marcos_ate(30).collect();
        assert_eq!(ate_30, [1, 3, 5, 7, 10, 15, 20, 25, 30]);
        assert!(!e_marco(2) && !e_marco(11) && !e_marco(16));
        assert_eq!(marco_de(2), Some("+10 bag slots"));
        assert_eq!(marco_de(7), Some("+10 bank slots"));
        assert_eq!(marco_de(4), None);
        assert_eq!(marco_de(35), Some("Stronger aura"));
        assert_eq!(proximo_marco(0), 1);
        assert_eq!(proximo_marco(1), 2);
        assert_eq!(proximo_marco(7), 10);
        assert_eq!(proximo_marco(15), 20);
    }

    #[test]
    fn aura_por_faixa() {
        let t: Vec<u8> = [0, 1, 2, 3, 4, 5, 9, 10, 14, 15, 30].iter().map(|&r| tier_da_aura(r)).collect();
        assert_eq!(t, [0, 1, 1, 2, 2, 3, 3, 4, 4, 5, 5]);
        assert_eq!(intensidade_da_aura(19), 1.0);
        assert_eq!(intensidade_da_aura(20), 1.25);
        assert_eq!(intensidade_da_aura(25), 1.5);
        assert_eq!(intensidade_da_aura(1_000), 3.0);
    }

    #[test]
    fn bau_tem_darksteel_chave_e_material_na_cor() {
        let b1 = bau_do_renascimento(1);
        assert_eq!(b1[0].0, item_id::DARKSTEEL);
        assert!(b1[0].1 > 0);
        assert_eq!(b1[1], (item_id::chave_na_cor(item_id::SCALE, 2), 2), "r1 is a milestone: two keys");
        assert_eq!(item_id::cor_de_id(b1[2].0), Some(2));
        let b10 = bau_do_renascimento(10);
        assert_eq!(item_id::cor_de_id(b10[1].0), Some(5), "legendary key from 10");
        assert_eq!(item_id::cor_de_id(b10[2].0), Some(4), "materials stop at purple");
        assert!(b10[0].1 > b1[0].1, "more Darksteel on higher rebirths");
        assert_eq!(bau_do_renascimento(4)[1].1, 1);
    }
}
