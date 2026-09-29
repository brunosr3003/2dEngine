//! The size of the bag and the bank, and what it costs to grow them.
//!
//! Both start with `INVENTORY_SLOTS` (40) slots and go up in tens, paying
//! GOLD — the bag up to 100, the bank up to 160. The bank is with the
//! village Banker (every island has one). How many expansions each character
//! has bought lives on the character (`bolsa_extra`, `banco_extra`); the size
//! is only maths. See docs/BANCO.md.

use crate::constants::{item_id, INVENTORY_SLOTS};

/// The coins that live in the WALLET: the last two entries of the bag's list,
/// outside the grid (they take no space and do not appear as items). Gold not
/// even that: it is a separate balance (`gold`). See docs/BANCO.md.
pub const CARTEIRA: [u16; 2] = [item_id::COPPER, item_id::DARKSTEEL];

pub fn e_moeda(id: u16) -> bool {
    CARTEIRA.contains(&id)
}

/// Extra slots per expansion.
pub const PASSO: usize = 10;
pub const BOLSA_MAX: usize = 100;
pub const BANCO_MAX: usize = 160;

/// How many expansions there are up to the cap.
pub const fn expansoes_max(banco: bool) -> u8 {
    let teto = if banco { BANCO_MAX } else { BOLSA_MAX };
    ((teto - INVENTORY_SLOTS) / PASSO) as u8
}

/// Slots with `extra` expansions bought.
pub fn tamanho(banco: bool, extra: u8) -> usize {
    (INVENTORY_SLOTS + extra.min(expansoes_max(banco)) as usize * PASSO)
        .min(if banco { BANCO_MAX } else { BOLSA_MAX })
}

/// Gold for the NEXT expansion, having bought `extra`. `None` = already at the cap.
///
/// The bag rises fast (doubling each step: 2k, 4k ... 64k): carrying more is
/// a comfort for whoever hunts. The bank is cheaper per step and rises slowly
/// (1k, 4k, 9k ... squared): storing is what you do in the city, unhurried.
pub fn custo(banco: bool, extra: u8) -> Option<u64> {
    if extra >= expansoes_max(banco) {
        return None;
    }
    let n = extra as u64;
    Some(if banco {
        1_000 * (n + 1) * (n + 1)
    } else {
        2_000 << n
    })
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn tamanhos_sobem_de_dez_ate_o_teto() {
        assert_eq!(tamanho(false, 0), 40);
        assert_eq!(tamanho(false, 1), 50);
        assert_eq!(expansoes_max(false), 6);
        assert_eq!(tamanho(false, 6), 100);
        assert_eq!(tamanho(false, 200), 100, "nunca passa do teto");
        assert_eq!(expansoes_max(true), 12);
        assert_eq!(tamanho(true, 12), 160);
    }

    #[test]
    fn custo_cresce_e_para_no_teto() {
        assert_eq!(custo(false, 0), Some(2_000));
        assert_eq!(custo(false, 5), Some(64_000));
        assert_eq!(custo(false, 6), None);
        assert_eq!(custo(true, 0), Some(1_000));
        assert_eq!(custo(true, 2), Some(9_000));
        assert_eq!(custo(true, 12), None);
        for banco in [false, true] {
            for n in 1..expansoes_max(banco) {
                assert!(custo(banco, n) > custo(banco, n - 1));
            }
        }
    }
}
