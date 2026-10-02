//! Combat helpers shared between client and server.
//!
//! 100% active mechanics: there is no passive chance. Block requires RMB held
//! + a ranged weapon; Parry requires an edge-press of LMB or RMB within
//! PARRY_WINDOW_S before the hit. RES (Resistance) changes the effectiveness:
//! - `block_dmg_reduction` up (more damage absorbed)
//! - `defense_stamina_cost_mult` down (lower costs on block + parry)

use crate::components::PlayerStats;
use crate::constants::{
    BLOCK_REDUCTION_MAX, BLOCK_STAMINA_COST, PARRY_STAGGER_S, PARRY_STAMINA_COST,
    STAMINA_COST_MULT_MIN,
};

/// Raw damage of the basic attack. Allocated INT already contributes to the
/// ring's `attack_damage` and adds a second point to the basic only. Skills
/// keep using `attack_damage` directly.
pub fn basic_attack_damage(stats: &PlayerStats, weapon_id: u16, allocated_int: u32) -> i32 {
    let bonus = if crate::constants::arma_magica(weapon_id) {
        allocated_int.min(i32::MAX as u32) as i32
    } else {
        0
    };
    stats.attack_damage.saturating_add(bonus).max(1)
}

/// Chance per allocated DEX point that a PISTOL wielder avoids a mob's hit.
pub const ESQUIVA_POR_DES: f32 = 0.005;
/// The most a pistol wielder ever dodges.
pub const ESQUIVA_MAXIMA: f32 = 0.25;

/// How likely this character is to DODGE a mob's basic hit (melee or
/// projectile) outright: no damage, no stagger, no knockback.
///
/// Pistols only, from the DEX the player put in themselves. The owner: an
/// all-DEX pistol is "too fragile" early and could not clear the level-14
/// Porão — the class with the least health and no way to recover mid-fight.
/// A wider range was tried first: any range over 9 made it kill bosses under
/// the 60 s floor (it was already the fastest killer). Dodging touches the
/// damage taken, not the damage dealt.
///
/// NEVER the telegraphed boss attacks: those arrive as skill hits and are
/// dodged by moving, by hand.
// ───────────────────────────── the level gap ─────────────────────────────

/// A mob ABOVE you is dangerous. The ladder grows player and mob almost in
/// step, so ten levels changed almost nothing: a level 51 pistol cleared
/// level 70 mobs at 82% health (02/10/2026, the owner: "he is able to kill
/// and level on level 60 mobs, should it happen?"). Past `GAP_LIVRE` levels
/// above you, each level costs `DANO_DADO_POR_NIVEL` of the damage you deal
/// and adds `DANO_RECEBIDO_POR_NIVEL` to what it deals you, up to `GAP_MAX`.
/// A mob at or below your level is unchanged.
///
/// Tightened the same day (3 free, 4%/6% → 1 free, 7%/12%): the owner said a
/// level 51 player "stays tanking level 60 mobs easily" with the first cut,
/// and asked for one free level only.
pub const GAP_LIVRE: u32 = 1;
pub const GAP_MAX: u32 = 10;
pub const DANO_DADO_POR_NIVEL: f32 = 0.07;
pub const DANO_RECEBIDO_POR_NIVEL: f32 = 0.12;

/// How many penalised levels the mob is above the player.
pub fn degraus_acima(nivel_jogador: u32, nivel_mob: u32) -> u32 {
    nivel_mob.saturating_sub(nivel_jogador).saturating_sub(GAP_LIVRE).min(GAP_MAX)
}

/// Multiplier on the player's damage against a mob of `nivel_mob`.
pub fn mult_dano_contra_mob(nivel_jogador: u32, nivel_mob: u32) -> f32 {
    1.0 - DANO_DADO_POR_NIVEL * degraus_acima(nivel_jogador, nivel_mob) as f32
}

/// Multiplier on a mob of `nivel_mob`'s damage against the player.
pub fn mult_dano_do_mob(nivel_jogador: u32, nivel_mob: u32) -> f32 {
    1.0 + DANO_RECEBIDO_POR_NIVEL * degraus_acima(nivel_jogador, nivel_mob) as f32
}

pub fn chance_de_esquiva(weapon_id: u16, des_alocada: u32) -> f32 {
    if weapon_id != crate::constants::item_id::PISTOLAS {
        return 0.0;
    }
    (des_alocada as f32 * ESQUIVA_POR_DES).min(ESQUIVA_MAXIMA)
}

/// Final stamina cost for a block, scaled by the target's
/// `defense_stamina_cost_mult` (RES reduces it). Capped by `STAMINA_COST_MULT_MIN`.
pub fn block_stamina_cost(stats: &PlayerStats) -> f32 {
    let mult = stats.defense_stamina_cost_mult.max(STAMINA_COST_MULT_MIN);
    BLOCK_STAMINA_COST * mult
}

/// Final stamina cost for a parry. The same law as block.
pub fn parry_stamina_cost(stats: &PlayerStats) -> f32 {
    let mult = stats.defense_stamina_cost_mult.max(STAMINA_COST_MULT_MIN);
    PARRY_STAMINA_COST * mult
}

/// Computes the residual damage after a block. Uses the target's
/// `block_dmg_reduction` (base 0.6 + RES), capped at `BLOCK_REDUCTION_MAX`.
/// Min 1 — it never reaches zero. `round()` instead of `ceil()` avoids a
/// 1-point drift from float precision (e.g. 50 * 0.1 = 5.0000004 which ceils to 6).
pub fn apply_block_damage(stats: &PlayerStats, raw_damage: i32) -> i32 {
    let reduction = stats.block_dmg_reduction.clamp(0.0, BLOCK_REDUCTION_MAX);
    let residual = ((raw_damage as f32) * (1.0 - reduction)).round() as i32;
    residual.max(1)
}

/// Tries to consume stamina for an ACTIVE block (RMB held + ranged +
/// stamina). Returns `Some((residual_damage, cost))` on success, None if
/// stamina was short. The caller must drain stamina_current by the returned `custo`.
pub fn try_block_active(
    stats: &PlayerStats,
    target_stamina: f32,
    raw_damage: i32,
) -> Option<(i32, f32)> {
    let cost = block_stamina_cost(stats);
    if target_stamina < cost {
        return None;
    }
    Some((apply_block_damage(stats, raw_damage), cost))
}

/// Tries to consume stamina for a parry. Returns `Some((cost, stagger_s))` on
/// success. Nullifies the damage + staggers the attacker.
pub fn try_parry_active(stats: &PlayerStats, target_stamina: f32) -> Option<(f32, f32)> {
    let cost = parry_stamina_cost(stats);
    if target_stamina < cost {
        return None;
    }
    Some((cost, PARRY_STAGGER_S))
}

#[cfg(test)]
mod tests {

    #[test]
    fn mob_acima_do_nivel_doi_mais_e_apanha_menos() {
        assert_eq!(mult_dano_contra_mob(51, 51), 1.0);
        assert_eq!(mult_dano_do_mob(51, 52), 1.0, "one level above is free");
        assert!((mult_dano_contra_mob(51, 60) - 0.44).abs() < 1e-5);
        assert!((mult_dano_do_mob(51, 60) - 1.96).abs() < 1e-5);
        assert!((mult_dano_contra_mob(40, 80) - 0.30).abs() < 1e-5, "capped at ten levels");
        assert_eq!(mult_dano_do_mob(60, 40), 1.0, "a lower mob is unchanged");
    }
    use super::*;
    use crate::components::base_player_stats;

    #[test]
    fn base_block_absorbs_60_percent() {
        let s = base_player_stats();
        // 50 * (1 - 0.6) = 20
        assert_eq!(apply_block_damage(&s, 50), 20);
    }

    #[test]
    fn res_increases_block_absorption() {
        let mut s = base_player_stats();
        s.block_dmg_reduction = 0.9; // tipo 100 RES
                                     // 50 * (1 - 0.9) = 5
        assert_eq!(apply_block_damage(&s, 50), 5);
    }

    #[test]
    fn block_capped_at_95_percent() {
        let mut s = base_player_stats();
        s.block_dmg_reduction = 0.99; // tentar bypass
                                      // capado em 0.95: 100 * 0.05 = 5
        assert_eq!(apply_block_damage(&s, 100), 5);
    }

    #[test]
    fn block_min_1_damage() {
        let mut s = base_player_stats();
        s.block_dmg_reduction = 0.95;
        // 1 * 0.05 = 0.05 → ceil = 1
        assert_eq!(apply_block_damage(&s, 1), 1);
    }

    #[test]
    fn res_reduces_stamina_costs() {
        let mut s = base_player_stats();
        s.defense_stamina_cost_mult = 0.6; // tipo 80 RES
        assert!((block_stamina_cost(&s) - 25.0 * 0.6).abs() < 0.001);
        assert!((parry_stamina_cost(&s) - 15.0 * 0.6).abs() < 0.001);
    }

    #[test]
    fn stamina_cost_capped_at_50_percent() {
        let mut s = base_player_stats();
        s.defense_stamina_cost_mult = 0.0; // tentar bypass
        assert!((block_stamina_cost(&s) - 25.0 * 0.5).abs() < 0.001);
    }

    #[test]
    fn try_block_active_succeeds_with_stamina() {
        let s = base_player_stats();
        let r = try_block_active(&s, 100.0, 50);
        assert_eq!(r, Some((20, 25.0)));
    }

    #[test]
    fn try_block_active_fails_without_stamina() {
        let s = base_player_stats();
        assert_eq!(try_block_active(&s, 5.0, 50), None);
    }
}

#[cfg(test)]
mod testes_da_esquiva {
    use super::*;
    use crate::constants::item_id;

    /// Pistols only, grows with allocated DEX, stops at the cap.
    #[test]
    fn so_a_pistola_esquiva_e_com_teto() {
        assert_eq!(chance_de_esquiva(item_id::KATANA, 200), 0.0);
        assert_eq!(chance_de_esquiva(item_id::ANEL_MAGICO, 200), 0.0);
        assert_eq!(chance_de_esquiva(item_id::PISTOLAS, 0), 0.0);
        let quinze = chance_de_esquiva(item_id::PISTOLAS, 42); // all-DEX, level 15
        assert!((0.15..=ESQUIVA_MAXIMA).contains(&quinze), "{quinze}");
        assert_eq!(chance_de_esquiva(item_id::PISTOLAS, 10_000), ESQUIVA_MAXIMA);
    }
}
