//! Tamanho da bolsa e do banco, e o que custa aumentar.
//!
//! Os dois comecam com `INVENTORY_SLOTS` (40) espacos e sobem de 10 em 10,
//! pagando OURO — a bolsa ate' 100, o banco ate' 160. O banco fica com o
//! Estivador da vila (toda ilha tem um). Quantas expansoes cada personagem ja'
//! comprou mora no personagem (`bolsa_extra`, `banco_extra`); o tamanho e' so'
//! conta. Ver docs/BANCO.md.

use crate::constants::{item_id, INVENTORY_SLOTS};

/// As moedas que moram na CARTEIRA: os dois ultimos espacos da lista da
/// bolsa, fora da grade (nao ocupam espaco nem aparecem como item). O ouro
/// nem isso: e' um saldo a parte (`gold`). Ver docs/BANCO.md.
pub const CARTEIRA: [u16; 2] = [item_id::COPPER, item_id::DARKSTEEL];

pub fn e_moeda(id: u16) -> bool {
    CARTEIRA.contains(&id)
}

/// Espacos a mais por expansao.
pub const PASSO: usize = 10;
pub const BOLSA_MAX: usize = 100;
pub const BANCO_MAX: usize = 160;

/// Quantas expansoes ha' ate' o teto.
pub const fn expansoes_max(banco: bool) -> u8 {
    let teto = if banco { BANCO_MAX } else { BOLSA_MAX };
    ((teto - INVENTORY_SLOTS) / PASSO) as u8
}

/// Espacos com `extra` expansoes compradas.
pub fn tamanho(banco: bool, extra: u8) -> usize {
    (INVENTORY_SLOTS + extra.min(expansoes_max(banco)) as usize * PASSO)
        .min(if banco { BANCO_MAX } else { BOLSA_MAX })
}

/// Ouro da PROXIMA expansao, tendo `extra` compradas. `None` = ja' no teto.
///
/// A bolsa sobe rapido (dobra a cada passo: 2 mil, 4 mil ... 64 mil): carregar
/// mais e' conforto de quem caça. O banco e' mais barato por passo e sobe
/// devagar (mil, 4 mil, 9 mil ... ao quadrado): guardar e' o que se faz na
/// cidade, sem pressa.
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
