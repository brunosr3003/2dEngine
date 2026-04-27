//! Helpers de combate compartilhados entre cliente e servidor.
//!
//! Mecanica 100% ativa: nao ha chance passiva. Block exige RMB segurado +
//! arma ranged; Parry exige edge-press de LMB ou RMB dentro de PARRY_WINDOW_S
//! antes do hit. RES (Resistencia) altera a efetividade:
//!   - `block_dmg_reduction` ↑ (mais dano absorvido)
//!   - `defense_stamina_cost_mult` ↓ (custos menores em block + parry)

use crate::components::PlayerStats;
use crate::constants::{
    BLOCK_REDUCTION_MAX, BLOCK_STAMINA_COST, PARRY_STAGGER_S, PARRY_STAMINA_COST,
    STAMINA_COST_MULT_MIN,
};

/// Custo final de stamina pra um block, escalado pelo `defense_stamina_cost_mult`
/// do alvo (RES reduz). Capado por `STAMINA_COST_MULT_MIN`.
pub fn block_stamina_cost(stats: &PlayerStats) -> f32 {
    let mult = stats.defense_stamina_cost_mult.max(STAMINA_COST_MULT_MIN);
    BLOCK_STAMINA_COST * mult
}

/// Custo final de stamina pra um parry. Mesma lei do block.
pub fn parry_stamina_cost(stats: &PlayerStats) -> f32 {
    let mult = stats.defense_stamina_cost_mult.max(STAMINA_COST_MULT_MIN);
    PARRY_STAMINA_COST * mult
}

/// Calcula o dano residual apos block. Usa `block_dmg_reduction` do alvo
/// (base 0.6 + RES), capado em `BLOCK_REDUCTION_MAX`. Min 1 — nunca chega a zero.
/// `round()` em vez de `ceil()` evita drift de 1ponto por causa de float
/// precision (ex.: 50 * 0.1 = 5.0000004 que ceil pra 6).
pub fn apply_block_damage(stats: &PlayerStats, raw_damage: i32) -> i32 {
    let reduction = stats.block_dmg_reduction.clamp(0.0, BLOCK_REDUCTION_MAX);
    let residual = ((raw_damage as f32) * (1.0 - reduction)).round() as i32;
    residual.max(1)
}

/// Tenta consumir stamina pra um block ATIVO (RMB held + ranged + stamina).
/// Retorna `Some((dano_residual, custo))` se conseguiu, None se faltou stamina.
/// Caller deve drenar stamina_current pelo `custo` retornado.
pub fn try_block_active(stats: &PlayerStats, target_stamina: f32, raw_damage: i32) -> Option<(i32, f32)> {
    let cost = block_stamina_cost(stats);
    if target_stamina < cost { return None; }
    Some((apply_block_damage(stats, raw_damage), cost))
}

/// Tenta consumir stamina pra um parry. Retorna `Some((custo, stagger_s))`
/// se conseguiu. Anula o dano + staggera atacante.
pub fn try_parry_active(stats: &PlayerStats, target_stamina: f32) -> Option<(f32, f32)> {
    let cost = parry_stamina_cost(stats);
    if target_stamina < cost { return None; }
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
