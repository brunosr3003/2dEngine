//! Death in the MIR4 mould: you go down, lose XP, revive in the island's
//! city and can RECOVER the lost XP — 3 times a day free, then paying gold.
//!
//! Pure rules here; applying and persisting is `world`'s job. See
//! docs/GAMEPLAY.md, Death section.

use serde::{Deserialize, Serialize};
use shared::{level_of_xp_with_mult, xp_for_level_with_mult};

/// Free XP recoveries per day (resets at UTC midnight).
pub const GRATIS_POR_DIA: u32 = 3;
/// How long a death stays recoverable.
pub const VALIDADE_S: i64 = 24 * 3600;
/// Recoverable deaths kept (the oldest drop off).
pub const MAX_MORTES: usize = 10;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct MorteRecuperavel {
    /// The instant of death (unix secs). It is its identity.
    pub quando: i64,
    pub xp: u64,
}

/// The XP a death takes: 10% of what the CURRENT level needs to level up,
/// but never below the level's start — dying drops no level and leaves no
/// negative XP.
pub fn perda_de_xp(xp: u64, mult: u64) -> u64 {
    let nivel = level_of_xp_with_mult(xp, mult);
    let piso = xp_for_level_with_mult(nivel, mult);
    let degrau = xp_for_level_with_mult(nivel + 1, mult).saturating_sub(piso);
    (degrau / 10).min(xp.saturating_sub(piso))
}

/// Gold to recover a death once the day's free ones are gone: a floor that
/// grows with the level plus half the XP returned.
pub fn custo_gold(xp: u64, nivel: u32) -> u64 {
    100 + nivel as u64 * 50 + xp / 2
}

pub fn dia(agora: i64) -> i64 {
    agora.div_euclid(86_400)
}

/// Free ones still left today. A new day zeroes the counter.
pub fn gratis_restantes(dia_salvo: i64, usadas: u32, agora: i64) -> u32 {
    if dia(agora) != dia_salvo {
        GRATIS_POR_DIA
    } else {
        GRATIS_POR_DIA.saturating_sub(usadas)
    }
}

/// Records a death (with no XP lost, nothing to recover) and drops the expired ones.
pub fn registrar(mortes: &mut Vec<MorteRecuperavel>, quando: i64, xp: u64) {
    mortes.retain(|m| m.quando + VALIDADE_S > quando);
    if xp == 0 {
        return;
    }
    mortes.push(MorteRecuperavel { quando, xp });
    if mortes.len() > MAX_MORTES {
        let sobra = mortes.len() - MAX_MORTES;
        mortes.drain(0..sobra);
    }
}

/// The ones still valid now.
pub fn validas(mortes: &[MorteRecuperavel], agora: i64) -> Vec<MorteRecuperavel> {
    mortes
        .iter()
        .copied()
        .filter(|m| m.quando + VALIDADE_S > agora)
        .collect()
}

/// Recovers the death at `quando`. Returns (XP returned, gold charged) and
/// already updates the list and the day's counter; the caller adds the XP and
/// deducts the gold.
pub fn recuperar(
    mortes: &mut Vec<MorteRecuperavel>,
    quando: i64,
    agora: i64,
    nivel: u32,
    gold: u64,
    dia_salvo: &mut i64,
    usadas: &mut u32,
) -> Result<(u64, u64), String> {
    let Some(i) = mortes
        .iter()
        .position(|m| m.quando == quando && m.quando + VALIDADE_S > agora)
    else {
        return Err("Essa morte não pode mais ser recuperada.".into());
    };
    let xp = mortes[i].xp;
    let custo = if gratis_restantes(*dia_salvo, *usadas, agora) > 0 {
        if dia(agora) != *dia_salvo {
            *dia_salvo = dia(agora);
            *usadas = 0;
        }
        *usadas += 1;
        0
    } else {
        let c = custo_gold(xp, nivel);
        if gold < c {
            return Err(format!("You need {c} gold."));
        }
        c
    };
    mortes.remove(i);
    Ok((xp, custo))
}

pub fn para_json(mortes: &[MorteRecuperavel]) -> String {
    serde_json::to_string(mortes).unwrap_or_default()
}

pub fn de_json(s: &str) -> Vec<MorteRecuperavel> {
    serde_json::from_str(s).unwrap_or_default()
}

#[cfg(test)]
mod testes {
    use super::*;

    const MULT: u64 = 100;

    #[test]
    fn perde_dez_por_cento_do_nivel_sem_cair_de_nivel() {
        // Level 5 starts at xp_for_level(5); the step to 6 is 25*MULT.
        let piso = xp_for_level_with_mult(5, MULT);
        let degrau = xp_for_level_with_mult(6, MULT) - piso;
        let xp = piso + degrau / 2;
        assert_eq!(perda_de_xp(xp, MULT), degrau / 10);
        // Newly arrived at the level: loses only what is above the floor.
        assert_eq!(perda_de_xp(piso + 3, MULT), 3);
        assert_eq!(perda_de_xp(piso, MULT), 0);
        assert_eq!(level_of_xp_with_mult(xp - perda_de_xp(xp, MULT), MULT), 5);
        assert_eq!(perda_de_xp(0, MULT), 0);
    }

    #[test]
    fn tres_gratis_por_dia_depois_cobra_ouro() {
        let agora = 10 * 86_400 + 500;
        let mut mortes = Vec::new();
        for k in 0..5 {
            registrar(&mut mortes, agora + k, 40);
        }
        let (mut dia_s, mut usadas) = (0i64, 0u32);
        for k in 0..3 {
            let r = recuperar(
                &mut mortes,
                agora + k,
                agora + 10,
                5,
                0,
                &mut dia_s,
                &mut usadas,
            );
            assert_eq!(r, Ok((40, 0)), "gratis {k}");
        }
        assert_eq!(gratis_restantes(dia_s, usadas, agora + 10), 0);
        let custo = custo_gold(40, 5);
        assert!(
            recuperar(
                &mut mortes,
                agora + 3,
                agora + 10,
                5,
                custo - 1,
                &mut dia_s,
                &mut usadas
            )
            .is_err(),
            "sem ouro nao recupera"
        );
        assert_eq!(mortes.len(), 2, "recusa nao consome a morte");
        assert_eq!(
            recuperar(
                &mut mortes,
                agora + 3,
                agora + 10,
                5,
                custo,
                &mut dia_s,
                &mut usadas
            ),
            Ok((40, custo))
        );
        // Dia seguinte: gratis de novo.
        assert_eq!(
            gratis_restantes(dia_s, usadas, agora + 86_400),
            GRATIS_POR_DIA
        );
        assert_eq!(
            recuperar(
                &mut mortes,
                agora + 4,
                agora + 86_400 - 600,
                5,
                0,
                &mut dia_s,
                &mut usadas
            )
            .map(|r| r.1)
            .ok(),
            None,
            "ainda no mesmo dia e sem ouro"
        );
    }

    #[test]
    fn morte_vence_em_24h_e_lista_tem_teto() {
        let mut mortes = Vec::new();
        registrar(&mut mortes, 1_000, 10);
        assert_eq!(validas(&mortes, 1_000 + VALIDADE_S - 1).len(), 1);
        assert!(validas(&mortes, 1_000 + VALIDADE_S).is_empty());
        let (mut d, mut u) = (0, 0);
        assert!(recuperar(
            &mut mortes,
            1_000,
            1_000 + VALIDADE_S,
            1,
            999_999,
            &mut d,
            &mut u
        )
        .is_err());
        for k in 0..(MAX_MORTES as i64 + 4) {
            registrar(&mut mortes, 5_000 + k, 1);
        }
        assert_eq!(mortes.len(), MAX_MORTES);
        registrar(&mut mortes, 9_000, 0);
        assert_eq!(mortes.len(), MAX_MORTES, "morte sem XP perdido nao entra");
    }

    #[test]
    fn persiste_ida_e_volta() {
        let mut mortes = Vec::new();
        registrar(&mut mortes, 77, 123);
        registrar(&mut mortes, 78, 456);
        assert_eq!(de_json(&para_json(&mortes)), mortes);
        assert!(de_json("").is_empty());
        assert!(de_json("lixo").is_empty());
    }
}
