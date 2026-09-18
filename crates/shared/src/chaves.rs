//! As CHAVES de craft — Escama, Garra, Chifre e Couro — so' caem de CHEFE:
//! chefe do mundo (raro) e chefe de dungeon/raid (docs/LOOT_DOS_MOBS.md).
//!
//! A cor segue o nivel do CONTEUDO (o chefe, a dungeon), nao o de quem mata:
//! ate' o 19 cinza, 20–39 verde, 40–59 azul, 60–79 epica, 80+ lendaria. E a
//! chance cai conforme a faixa sobe — chave boa e' rara de proposito, e e' ela
//! que decide quantos itens o mundo produz.
//!
//! Pedra e mob comum nao dao chave nenhuma.

use crate::constants::item_id;

/// De onde a chave sai.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Fonte {
    /// Chefe que nasce no mundo aberto: chance menor, qualquer um farma.
    ChefeDoMundo,
    /// Chefe de dungeon ou raid: a chance cheia da tabela.
    Dungeon,
    Raid,
}

/// Uma faixa: a partir de que nivel, a cor que cai e a chance por morte.
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
        nivel_min: 40,
        cor: 3,
        chance: 0.06,
        chance_mundo: 0.02,
    },
    FaixaDeChave {
        nivel_min: 60,
        cor: 4,
        chance: 0.03,
        chance_mundo: 0.01,
    },
    FaixaDeChave {
        nivel_min: 80,
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

/// Rola a chave de um chefe de nivel `nivel`. `mult` multiplica a chance
/// (Pocao de Sorte); `r_chance` e `r_qual` sao dois sorteios em [0, 1): um
/// decide se cai, o outro qual das quatro.
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
            (39, 2),
            (40, 3),
            (59, 3),
            (60, 4),
            (79, 4),
            (80, 5),
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
        assert_eq!(rolar(45, d, 1.0, 0.0, 0.99), Some(na_cor(HIDE, 3)));
        assert_eq!(rolar(65, Fonte::Raid, 1.0, 0.0, 0.3), Some(na_cor(CLAW, 4)));
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
