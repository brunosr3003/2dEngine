//! Combat helpers shared between client and server. (Block and parry went
//! with stamina on 09/11/2026.)

use crate::components::PlayerStats;

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
/// Every other weapon set dodges too, at HALF the pistol's rate and a lower
/// cap (owner, 08/11/2026: "DEX gives dodge to every class, but Twin Pistols
/// must get more, to make better use of that dodge").
pub const ESQUIVA_POR_DES_GERAL: f32 = 0.0025;
pub const ESQUIVA_MAXIMA_GERAL: f32 = 0.15;

/// How likely this character is to DODGE a mob's basic hit (melee or
/// projectile) outright: no damage, no stagger, no knockback.
///
/// Every weapon set dodges from the DEX the player put in themselves; the
/// pistol twice as much and up to a higher cap. Why the pistol: an
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
/// and asked for one free level only — then for three.
pub const GAP_LIVRE: u32 = 3;
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

/// See the dodge notes above `ESQUIVA_POR_DES`.
pub fn chance_de_esquiva(weapon_id: u16, des_alocada: u32) -> f32 {
    if weapon_id == crate::constants::item_id::PISTOLAS {
        (des_alocada as f32 * ESQUIVA_POR_DES).min(ESQUIVA_MAXIMA)
    } else {
        (des_alocada as f32 * ESQUIVA_POR_DES_GERAL).min(ESQUIVA_MAXIMA_GERAL)
    }
}

#[cfg(test)]
mod tests {

    #[test]
    fn mob_acima_do_nivel_doi_mais_e_apanha_menos() {
        assert_eq!(mult_dano_contra_mob(51, 51), 1.0);
        assert_eq!(mult_dano_do_mob(51, 54), 1.0, "three levels above are free");
        assert!((mult_dano_contra_mob(51, 60) - 0.58).abs() < 1e-5);
        assert!((mult_dano_do_mob(51, 60) - 1.72).abs() < 1e-5);
        assert!((mult_dano_contra_mob(40, 80) - 0.30).abs() < 1e-5, "capped at ten levels");
        assert_eq!(mult_dano_do_mob(60, 40), 1.0, "a lower mob is unchanged");
    }
    use super::*;
}

#[cfg(test)]
mod testes_da_esquiva {
    use super::*;
    use crate::constants::item_id;

    /// Everyone dodges from DEX; the pistol twice as much, with a higher cap.
    #[test]
    fn todos_esquivam_e_a_pistola_mais() {
        for arma in [item_id::KATANA, item_id::ANEL_MAGICO, 0] {
            assert_eq!(chance_de_esquiva(arma, 0), 0.0);
            let c = chance_de_esquiva(arma, 40);
            assert!((c - 0.10).abs() < 1e-6, "{arma}: {c}");
            assert!(c < chance_de_esquiva(item_id::PISTOLAS, 40), "pistol must dodge more");
            assert_eq!(chance_de_esquiva(arma, 10_000), ESQUIVA_MAXIMA_GERAL);
        }
        assert!(ESQUIVA_MAXIMA_GERAL < ESQUIVA_MAXIMA);
        assert_eq!(chance_de_esquiva(item_id::PISTOLAS, 0), 0.0);
        let quinze = chance_de_esquiva(item_id::PISTOLAS, 42); // all-DEX, level 15
        assert!((0.15..=ESQUIVA_MAXIMA).contains(&quinze), "{quinze}");
        assert_eq!(chance_de_esquiva(item_id::PISTOLAS, 10_000), ESQUIVA_MAXIMA);
    }
}

/// XP for a monster well BELOW the player (owner, 11/11/2026): up to 5 levels
/// below, full XP; then -8% per level, never under 20%. Only XP — drops,
/// gold and materials are untouched. Bosses and dungeons don't use it.
pub const XP_JANELA_SEM_PENALIDADE: u32 = 5;
pub const XP_QUEDA_POR_NIVEL: f32 = 0.08;
pub const XP_PISO: f32 = 0.20;

pub fn fator_xp_por_nivel(nivel_jogador: u32, nivel_mob: u32) -> f32 {
    let abaixo = nivel_jogador.saturating_sub(nivel_mob);
    if abaixo <= XP_JANELA_SEM_PENALIDADE {
        return 1.0;
    }
    (1.0 - XP_QUEDA_POR_NIVEL * (abaixo - XP_JANELA_SEM_PENALIDADE) as f32).max(XP_PISO)
}

#[cfg(test)]
mod testes_xp_por_nivel {
    use super::*;

    #[test]
    fn cheio_ate_cinco_abaixo_depois_cai_ate_o_piso() {
        assert_eq!(fator_xp_por_nivel(30, 40), 1.0, "mob above: no penalty");
        assert_eq!(fator_xp_por_nivel(30, 25), 1.0);
        assert!((fator_xp_por_nivel(30, 24) - 0.92).abs() < 1e-6);
        assert!((fator_xp_por_nivel(30, 20) - 0.60).abs() < 1e-6);
        assert_eq!(fator_xp_por_nivel(41, 22), XP_PISO, "Anubis in Geleira 22");
        assert_eq!(fator_xp_por_nivel(80, 1), XP_PISO);
    }
}

