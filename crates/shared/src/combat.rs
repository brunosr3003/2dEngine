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
