//! The craft KEYS — Scale, Claw, Horn and Hide — drop from world and
//! dungeon/raid bosses, and some side quests hand over a one-off stock for
//! the first craft of each tier.
//!
//! The color follows the level of the CONTENT (the boss, the dungeon), not
//! that of whoever kills it: up to 19 grey, 20-29 green, 30-39 blue, 40-49
//! epic, 50+ legendary. And the chance falls as the tier rises — a good key
//! is rare on purpose, and it is what decides how many items the world produces.
//!
//! Stone and common mobs give no key at all.

use crate::constants::item_id;

/// Where the key comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Fonte {
    /// A boss that spawns in the open world: a lower chance, anyone can farm it.
    ChefeDoMundo,
    /// Chefe de dungeon ou raid: a chance cheia da tabela.
    Dungeon,
    Raid,
}

/// One tier: from what level, the color that drops and the chance per kill.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FaixaDeChave {
    pub nivel_min: u32,
    /// 1 cinza, 2 verde, 3 azul, 4 epica (roxa), 5 lendaria.
    pub cor: u8,
    /// Chefe de dungeon e de raid.
    pub chance: f32,
    /// Chefe do mundo.
    pub chance_mundo: f32,
}

impl FaixaDeChave {
    pub fn chance_de(&self, fonte: Fonte) -> f32 {
        match fonte {
            Fonte::ChefeDoMundo => self.chance_mundo,
            Fonte::Dungeon | Fonte::Raid => self.chance,
        }
    }
}

pub const FAIXAS: [FaixaDeChave; 5] = [
    FaixaDeChave {
        nivel_min: 1,
        cor: 1,
        chance: 0.30,
        chance_mundo: 0.12,
    },
    FaixaDeChave {
        nivel_min: 20,
        cor: 2,
        chance: 0.10,
        chance_mundo: 0.03,
    },
    FaixaDeChave {
        // From 40 to 30 on 28/09/2026, at the owner's request: the BLUE (Rare) key
        // is what unlocks crafting a Rare piece, and waiting for the level 40 boss
        // left the 30-39 tier with nothing to build. The green one shortened to 20-29.
        nivel_min: 30,
        cor: 3,
        chance: 0.06,
        chance_mundo: 0.02,
    },
    FaixaDeChave {
        // 60 -> 40 on 28/09/2026, along with the legendary. The keys' ladder became
        // ten by ten from 20 on (20 green, 30 blue, 40 epic, 50 legendary): before,
        // it opened twenty by twenty and the top two colors were out of reach of the
        // game that actually exists.
        nivel_min: 40,
        cor: 4,
        chance: 0.03,
        chance_mundo: 0.01,
    },
    FaixaDeChave {
        // 80 -> 50. The highest boss in the catalogue is level 60, so the legendary
        // was a color that existed (ids 353-356) and had nowhere to drop FROM. At
        // 50 it starts dropping from the level 50-60 content already in the game.
        nivel_min: 50,
        cor: 5,
        chance: 0.01,
        chance_mundo: 0.003,
    },
];

/// A faixa de um conteudo de nivel `nivel`.
pub fn faixa(nivel: u32) -> FaixaDeChave {
    *FAIXAS
        .iter()
        .rev()
        .find(|f| nivel >= f.nivel_min)
        .unwrap_or(&FAIXAS[0])
}

pub fn nome_da_cor(cor: u8) -> &'static str {
    match cor {
        1 => "Cinza",
        2 => "Verde",
        3 => "Azul",
        4 => "Épica",
        _ => "Lendária",
    }
}

/// Rolls the key of a level `nivel` boss. `mult` multiplies the chance (Luck
/// Potion); `r_chance` and `r_qual` are two draws in [0, 1): one decides
/// whether it drops, the other which of the four.
pub fn rolar(nivel: u32, fonte: Fonte, mult: f32, r_chance: f32, r_qual: f32) -> Option<u16> {
    let f = faixa(nivel);
    if r_chance >= (f.chance_de(fonte) * mult.max(0.0)).min(1.0) {
        return None;
    }
    let i = ((r_qual.clamp(0.0, 0.999_999) * 4.0) as usize).min(3);
    Some(item_id::chave_na_cor(item_id::CHAVES[i], f.cor))
}

#[cfg(test)]
mod tests {
    use super::*;
    use item_id::*;

    #[test]
    fn cor_pela_faixa_do_conteudo() {
        for (nivel, cor) in [
            (1, 1),
            (19, 1),
            (20, 2),
            (29, 2),
            (30, 3),
            (39, 3),
            (40, 4),
            (49, 4),
            (50, 5),
            (120, 5),
        ] {
            assert_eq!(faixa(nivel).cor, cor, "nivel {nivel}");
        }
    }

    #[test]
    fn chance_cai_conforme_sobe_e_mundo_e_menor() {
        for par in FAIXAS.windows(2) {
            assert!(par[1].chance < par[0].chance);
            assert!(par[1].chance_mundo < par[0].chance_mundo);
            assert!(par[1].nivel_min > par[0].nivel_min);
        }
        for f in FAIXAS {
            assert!(
                f.chance_mundo < f.chance,
                "faixa {}: chefe do mundo tem que dar menos",
                f.nivel_min
            );
            assert_eq!(f.chance_de(Fonte::Dungeon), f.chance_de(Fonte::Raid));
        }
        assert_eq!(FAIXAS[0].chance, 0.30);
        assert_eq!(FAIXAS[0].chance_mundo, 0.12);
    }

    #[test]
    fn rola_uma_das_quatro_na_cor_certa() {
        let d = Fonte::Dungeon;
        assert_eq!(rolar(10, d, 1.0, 0.0, 0.0), Some(na_cor(SCALE, 1)));
        assert_eq!(rolar(35, d, 1.0, 0.0, 0.99), Some(na_cor(HIDE, 3)));
        assert_eq!(rolar(45, Fonte::Raid, 1.0, 0.0, 0.3), Some(na_cor(CLAW, 4)));
        assert_eq!(rolar(90, d, 1.0, 0.0, 0.6), Some(HORN_LENDARIA));
        assert_eq!(
            rolar(10, d, 1.0, 0.30, 0.0),
            None,
            "no limite da chance nao cai"
        );
        assert_eq!(
            rolar(10, Fonte::ChefeDoMundo, 1.0, 0.12, 0.0),
            None,
            "no limite da chance do mundo nao cai"
        );
    }

    #[test]
    fn taxa_bate_com_a_tabela() {
        let mut s = 0x5EEDu64;
        let mut r = || {
            s = s
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            (s >> 11) as f32 / (1u64 << 53) as f32
        };
        const N: u32 = 200_000;
        for f in FAIXAS {
            for fonte in [Fonte::ChefeDoMundo, Fonte::Dungeon] {
                let caiu = (0..N)
                    .filter(|_| rolar(f.nivel_min, fonte, 1.0, r(), r()).is_some())
                    .count() as f32
                    / N as f32;
                let esperado = f.chance_de(fonte);
                assert!(
                    (caiu - esperado).abs() < 0.004,
                    "faixa {} {fonte:?}: {caiu}",
                    f.nivel_min
                );
            }
        }
    }

    #[test]
    fn lendarias_sao_ids_proprios() {
        let todas: Vec<u16> = CHAVES.iter().map(|&b| chave_na_cor(b, 5)).collect();
        assert_eq!(
            todas,
            vec![SCALE_LENDARIA, CLAW_LENDARIA, HORN_LENDARIA, HIDE_LENDARIA]
        );
        for b in CHAVES {
            for cor in 1..=4 {
                assert!(!todas.contains(&chave_na_cor(b, cor)));
            }
        }
    }
}
