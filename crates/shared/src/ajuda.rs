//! The bag's HELP tab (owner, 11/11/2026): how a character's gear compares
//! with what the game expects at their level, which slots hold them back,
//! and where to hunt and which dungeon to run for their real strength.
//!
//! Pure: the client only draws it. "Expected" is the ladder
//! (`crate::ladder`): the character of that level, geared on level, at +0.

use crate::{dungeon, ladder, terreno, Equipment, PlayerStats};

/// The level the character's POWER matches: the highest level whose ladder
/// power is not above it (at least 1).
pub fn nivel_do_poder(poder: i32) -> u32 {
    (1..=200u32)
        .take_while(|&n| ladder::power(n) <= poder)
        .last()
        .unwrap_or(1)
}

/// Monster levels worth hunting: up to the character's level, but not above
/// what the gear handles (power level + 2), and never so low that the XP
/// penalty starts (`combat::XP_JANELA_SEM_PENALIDADE`).
pub fn faixa_de_caca(nivel: u32, poder: i32) -> (u32, u32) {
    let alto = nivel.min(nivel_do_poder(poder) + 2).max(1);
    let baixo = alto.saturating_sub(crate::combat::XP_JANELA_SEM_PENALIDADE).max(1);
    (baixo, alto)
}

/// The island whose level band holds `nivel` (the highest one that starts
/// at or below it).
pub fn ilha_para(nivel: u32) -> &'static terreno::DefIlha {
    terreno::ARQUIPELAGO
        .iter()
        .filter(|d| d.nivel.0 <= nivel && !crate::magica::e_magica(d.zona))
        .max_by_key(|d| d.nivel.0)
        .unwrap_or(&terreno::ARQUIPELAGO[0])
}

/// The highest dungeon the character can enter AND clear: level reached and
/// power at or above its stage-1 minimum.
pub fn dungeon_para(nivel: u32, poder: i32) -> Option<&'static dungeon::Conteudo> {
    dungeon::CONTEUDOS
        .iter()
        .filter(|c| dungeon::no_jogo(c) && c.nivel_min <= nivel && dungeon::poder_minimo(c, 1) <= poder)
        .max_by_key(|c| c.nivel_min)
}

/// One attribute against the ladder: (has, expected).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Comparacao {
    pub tem: i32,
    pub esperado: i32,
}

impl Comparacao {
    /// 0.0 .. 1.0+ (1.0 = on the ladder).
    pub fn fracao(&self) -> f32 {
        if self.esperado <= 0 {
            1.0
        } else {
            self.tem as f32 / self.esperado as f32
        }
    }
}

/// Attack, defense and health against the ladder of `nivel`.
pub fn comparar(nivel: u32, s: &PlayerStats) -> [Comparacao; 3] {
    let (a, d, h) = ladder::row(nivel);
    [
        Comparacao { tem: s.attack_damage, esperado: a },
        Comparacao { tem: s.defense, esperado: d },
        Comparacao { tem: s.hp_max, esperado: h },
    ]
}

/// How one gear slot is doing.
#[derive(Debug, Clone, PartialEq)]
pub enum Peca {
    Vazia,
    /// (item level, refinement, color 1..5).
    Com { item_level: u16, refino: u8, cor: u8 },
}

/// Slots that make the character's strength, in sheet order.
pub fn pecas(e: &Equipment) -> Vec<(&'static str, Peca)> {
    let p = |id: Option<u16>, inst: &Option<crate::items::ItemInstance>| match (id, inst) {
        (Some(_), Some(i)) => Peca::Com { item_level: i.item_level, refino: i.refinement, cor: i.rarity },
        (Some(_), None) => Peca::Com { item_level: 1, refino: 0, cor: 1 },
        _ => Peca::Vazia,
    };
    vec![
        ("Weapon", p(e.weapon, &e.weapon_inst)),
        ("Off-hand", p(e.offhand, &e.offhand_inst)),
        ("Armor", p(e.armor, &e.armor_inst)),
        ("Earring", p(e.earring, &e.earring_inst)),
        ("Necklace", p(e.necklace, &e.necklace_inst)),
        ("Bracelet", p(e.bracelet, &e.bracelet_inst)),
        ("Belt", p(e.belt, &e.belt_inst)),
    ]
}

/// How far a slot lags: 0 on level (item level within 5), 1 behind (within
/// 15), 2 far behind or empty.
pub fn atraso(p: &Peca, nivel: u32) -> u8 {
    match p {
        Peca::Vazia => 2,
        Peca::Com { item_level, .. } => {
            let falta = nivel.saturating_sub(*item_level as u32);
            if falta <= 5 {
                0
            } else if falta <= 15 {
                1
            } else {
                2
            }
        }
    }
}

/// Up to three short tips, most useful first.
pub fn dicas(nivel: u32, e: &Equipment) -> Vec<String> {
    let pecas = pecas(e);
    let mut v = Vec::new();
    let vazias: Vec<&str> = pecas.iter().filter(|(_, p)| *p == Peca::Vazia).map(|(n, _)| *n).collect();
    if !vazias.is_empty() {
        v.push(format!("Empty slot: {}. Any piece there adds power.", vazias.join(", ")));
    }
    if let Some((nome, Peca::Com { item_level, .. })) = pecas
        .iter()
        .filter(|(_, p)| matches!(p, Peca::Com { .. }))
        .min_by_key(|(_, p)| match p {
            Peca::Com { item_level, .. } => *item_level,
            Peca::Vazia => u16::MAX,
        })
    {
        if nivel.saturating_sub(*item_level as u32) > 5 {
            v.push(format!(
                "Weakest piece: {nome} (item level {item_level}). Craft or buy one near level {nivel}."
            ));
        }
    }
    let sem_refino = pecas
        .iter()
        .filter(|(_, p)| matches!(p, Peca::Com { refino: 0, .. }))
        .count();
    if sem_refino >= 3 {
        v.push(format!("{sem_refino} pieces at +0: refining up to +5 is safe."));
    }
    v.truncate(3);
    v
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn faixa_de_caca_respeita_o_equipamento_e_a_penalidade() {
        // On the ladder: hunt from 5 below up to the level.
        let n = 30;
        assert_eq!(faixa_de_caca(n, ladder::power(n)), (25, 30));
        // Gear far behind: the top of the range follows the gear, not the level.
        let (baixo, alto) = faixa_de_caca(34, ladder::power(20));
        assert!(alto <= 22 && alto - baixo == 5, "{baixo}-{alto}");
        // Never an empty or inverted range.
        let (b, a) = faixa_de_caca(1, 0);
        assert!(b >= 1 && b <= a);
    }

    #[test]
    fn nivel_do_poder_bate_com_a_ladder() {
        for n in [1u32, 10, 25, 40] {
            assert_eq!(nivel_do_poder(ladder::power(n)), n);
        }
    }

    #[test]
    fn ilha_e_dungeon_recomendadas() {
        assert_eq!(ilha_para(5).nivel.0, terreno::ARQUIPELAGO[0].nivel.0);
        assert!(ilha_para(35).nivel.0 <= 35);
        // A geared level 30 gets a Porão it can clear; nobody gets a Cavern.
        let d = dungeon_para(30, ladder::power(30)).expect("a dungeon at 30");
        assert!(dungeon::no_jogo(d) && d.nivel_min <= 30);
        assert!(dungeon_para(1, 0).is_none() || dungeon_para(1, 0).unwrap().nivel_min <= 1);
    }

    #[test]
    fn pecas_vazias_e_atrasadas() {
        let e = Equipment::default();
        assert!(pecas(&e).iter().all(|(_, p)| *p == Peca::Vazia));
        assert_eq!(atraso(&Peca::Vazia, 10), 2);
        assert_eq!(atraso(&Peca::Com { item_level: 28, refino: 0, cor: 2 }, 30), 0);
        assert_eq!(atraso(&Peca::Com { item_level: 18, refino: 0, cor: 1 }, 30), 1);
        assert_eq!(atraso(&Peca::Com { item_level: 5, refino: 0, cor: 1 }, 30), 2);
        assert!(dicas(30, &e)[0].starts_with("Empty slot"));
    }
}
