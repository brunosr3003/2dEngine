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
    nivel_mob
        .saturating_sub(nivel_jogador)
        .saturating_sub(GAP_LIVRE)
        .min(GAP_MAX)
}

/// Multiplier on the player's damage against a mob of `nivel_mob`.
pub fn mult_dano_contra_mob(nivel_jogador: u32, nivel_mob: u32) -> f32 {
    1.0 - DANO_DADO_POR_NIVEL * degraus_acima(nivel_jogador, nivel_mob) as f32
}

/// Multiplier on a mob of `nivel_mob`'s damage against the player.
pub fn mult_dano_do_mob(nivel_jogador: u32, nivel_mob: u32) -> f32 {
    1.0 + DANO_RECEBIDO_POR_NIVEL * degraus_acima(nivel_jogador, nivel_mob) as f32
}

/// See the dodge notes above `ESQUIVA_POR_DES`. `nivel` is the character's
/// INTERNAL level: it only matters to the pistol's early-game bonus
/// (`bonus_inicial_da_pistola`), which adds to the DEX dodge under the same
/// `ESQUIVA_MAXIMA` cap.
pub fn chance_de_esquiva(weapon_id: u16, des_alocada: u32, nivel: u32) -> f32 {
    if weapon_id == crate::constants::item_id::PISTOLAS {
        let (_, extra) = bonus_inicial_da_pistola(nivel);
        (des_alocada as f32 * ESQUIVA_POR_DES + extra).min(ESQUIVA_MAXIMA)
    } else {
        (des_alocada as f32 * ESQUIVA_POR_DES_GERAL).min(ESQUIVA_MAXIMA_GERAL)
    }
}

// ─────────────────────── the pistol's early game ───────────────────────

/// The owner, 15/11/2026: "deixa pistola um pouco mais forte no early game
/// com mais dodge e mais dano e do jeito q ta no late game". An all-DEX
/// pistol starts with the least health and almost no DEX dodge (a handful
/// of points at level 1), so the early levels are where it feels weakest.
///
/// At internal level 1 the pistol gets `+PISTOLA_INICIO_DANO` attack
/// (multiplier on `attack_damage`, so basics AND skills) and
/// `+PISTOLA_INICIO_ESQUIVA` flat dodge; both fall in a straight line to
/// ZERO at internal `PISTOLA_INICIO_ATE` (shown 50, the end of the Glacier)
/// and stay zero after. Why 30: the pistol's own late-game level bonus
/// (`world::effective_stats`, `nivel/3` up to 12) is what the boss guards
/// measure from internal 36 to 60 — fading before 36 leaves every boss
/// number and the whole late game exactly as they were.
///
/// It applies in PvP too: it is tied to low level, and PvP already pays a
/// level gap; two level-60 pistols duel exactly as before.
pub const PISTOLA_INICIO_DANO: f32 = 0.20;
pub const PISTOLA_INICIO_ESQUIVA: f32 = 0.06;
pub const PISTOLA_INICIO_ATE: u32 = 30;

/// `(extra damage fraction, extra dodge chance)` of a pistol wielder at
/// INTERNAL level `nivel`. Never negative; zero from `PISTOLA_INICIO_ATE` on.
pub fn bonus_inicial_da_pistola(nivel: u32) -> (f32, f32) {
    let n = nivel.max(1);
    if n >= PISTOLA_INICIO_ATE {
        return (0.0, 0.0);
    }
    let f = (PISTOLA_INICIO_ATE - n) as f32 / (PISTOLA_INICIO_ATE - 1) as f32;
    (PISTOLA_INICIO_DANO * f, PISTOLA_INICIO_ESQUIVA * f)
}

#[cfg(test)]
mod testes_do_inicio_da_pistola {
    use super::*;
    use crate::constants::item_id;

    #[test]
    fn bonus_cheio_no_um_some_no_trinta_e_nunca_e_negativo() {
        let (d, e) = bonus_inicial_da_pistola(1);
        assert!(
            (d - PISTOLA_INICIO_DANO).abs() < 1e-6 && (e - PISTOLA_INICIO_ESQUIVA).abs() < 1e-6
        );
        assert_eq!(bonus_inicial_da_pistola(0), bonus_inicial_da_pistola(1));
        let mut antes = (f32::MAX, f32::MAX);
        for n in 0..=200 {
            let (d, e) = bonus_inicial_da_pistola(n);
            assert!(d >= 0.0 && e >= 0.0, "nv{n}: {d} {e}");
            assert!(d <= antes.0 && e <= antes.1, "nv{n}: must only fall");
            antes = (d, e);
            if n >= PISTOLA_INICIO_ATE {
                assert_eq!((d, e), (0.0, 0.0), "nv{n}: late game untouched");
            }
        }
    }

    #[test]
    fn so_a_pistola_ganha_a_esquiva_do_inicio() {
        assert!((chance_de_esquiva(item_id::PISTOLAS, 0, 1) - PISTOLA_INICIO_ESQUIVA).abs() < 1e-6);
        assert_eq!(chance_de_esquiva(item_id::KATANA, 0, 1), 0.0);
        assert_eq!(
            chance_de_esquiva(item_id::PISTOLAS, 10, 60),
            chance_de_esquiva(item_id::PISTOLAS, 10, 30)
        );
        assert_eq!(
            chance_de_esquiva(item_id::PISTOLAS, 10_000, 1),
            ESQUIVA_MAXIMA,
            "same cap"
        );
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
        assert!(
            (mult_dano_contra_mob(40, 80) - 0.30).abs() < 1e-5,
            "capped at ten levels"
        );
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
            assert_eq!(chance_de_esquiva(arma, 0, 60), 0.0);
            let c = chance_de_esquiva(arma, 40, 60);
            assert!((c - 0.10).abs() < 1e-6, "{arma}: {c}");
            assert!(
                c < chance_de_esquiva(item_id::PISTOLAS, 40, 60),
                "pistol must dodge more"
            );
            assert_eq!(chance_de_esquiva(arma, 10_000, 60), ESQUIVA_MAXIMA_GERAL);
        }
        assert!(ESQUIVA_MAXIMA_GERAL < ESQUIVA_MAXIMA);
        assert_eq!(chance_de_esquiva(item_id::PISTOLAS, 0, 60), 0.0);
        let quinze = chance_de_esquiva(item_id::PISTOLAS, 42, 15); // all-DEX, level 15
        assert!((0.15..=ESQUIVA_MAXIMA).contains(&quinze), "{quinze}");
        assert_eq!(
            chance_de_esquiva(item_id::PISTOLAS, 10_000, 60),
            ESQUIVA_MAXIMA
        );
    }
}

/// XP for a monster well BELOW the player (owner, 11/11/2026; eased
/// 15/11/2026: "a penalidade por matar mob de nível menor tem que diminuir"):
/// up to 8 (internal) levels below, full XP; then -5% per level, never under
/// 40%. It was 5 levels, -8% and a 20% floor. Only XP — drops, gold and
/// materials are untouched. Bosses and dungeons don't use it.
pub const XP_JANELA_SEM_PENALIDADE: u32 = 8;
pub const XP_QUEDA_POR_NIVEL: f32 = 0.05;
pub const XP_PISO: f32 = 0.40;

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
    fn cheio_ate_oito_abaixo_depois_cai_ate_o_piso() {
        assert_eq!(fator_xp_por_nivel(30, 40), 1.0, "mob above: no penalty");
        assert_eq!(fator_xp_por_nivel(30, 22), 1.0, "8 below: still full");
        assert!((fator_xp_por_nivel(30, 21) - 0.95).abs() < 1e-6);
        assert!((fator_xp_por_nivel(30, 15) - 0.65).abs() < 1e-6);
        assert!(
            (fator_xp_por_nivel(41, 22) - 0.45).abs() < 1e-6,
            "Anubis in Geleira 22"
        );
        assert_eq!(fator_xp_por_nivel(80, 1), XP_PISO);
    }
}
