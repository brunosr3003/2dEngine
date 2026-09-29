//! NPC shop: buying in a BATCH. The rules, pure and testable; `world.rs`
//! finds the vendor nearby, calls in and answers.
//!
//! The shop charges in COPPER (docs/ECONOMIA.md). A batch is all or nothing:
//! the copper for the whole batch and the space for it are checked before the
//! bag is touched — before, buying 50 was 50 requests, and if the copper ran
//! out halfway dozens of refusals arrived in the chat.

use shared::{item_id, InventorySlot};

/// Largest batch at once.
pub const MAX_LOTE: u32 = 999;

/// Buys `qtd` of `item` at `preco` copper each: takes the copper and puts
/// the item in (stacked up to `cap`). Returns the total paid. On refusal the
/// bag does not change and the `Err` is the sentence the player reads.
pub fn comprar_lote(
    inv: &mut [InventorySlot],
    item: u16,
    preco: u32,
    qtd: u32,
    cap: u32,
) -> Result<u64, String> {
    if qtd == 0 || qtd > MAX_LOTE {
        return Err(format!("choose from 1 to {MAX_LOTE}"));
    }
    let total = preco as u64 * qtd as u64;
    let tem = crate::craft::tem(inv, item_id::COPPER) as u64;
    if tem < total {
        return Err(format!(
            "{} copper short ({} per unit)",
            total - tem,
            preco
        ));
    }
    let mut sim = inv.to_vec();
    crate::craft::consumir(&mut sim, item_id::COPPER, total as u32);
    if !crate::craft::por_empilhavel(&mut sim, item, qtd, cap) {
        return Err("bag too full for that batch".into());
    }
    inv.copy_from_slice(&sim);
    Ok(total)
}

#[cfg(test)]
mod testes {
    use super::*;

    fn bolsa(itens: &[(u16, u32)], vagas: usize) -> Vec<InventorySlot> {
        let mut v: Vec<InventorySlot> = itens
            .iter()
            .map(|&(id, q)| InventorySlot {
                item_id: id,
                qty: q,
                instance: None,
            })
            .collect();
        v.extend(std::iter::repeat_n(InventorySlot::default(), vagas));
        v
    }

    const POCAO: u16 = item_id::HEALTH_POTION;

    #[test]
    fn lote_paga_tudo_e_empilha() {
        let mut inv = bolsa(&[(item_id::COPPER, 1_000), (POCAO, 3)], 2);
        assert_eq!(comprar_lote(&mut inv, POCAO, 10, 50, 999), Ok(500));
        assert_eq!(crate::craft::tem(&inv, item_id::COPPER), 500);
        assert_eq!(crate::craft::tem(&inv, POCAO), 53);
    }

    #[test]
    fn sem_cobre_pro_lote_inteiro_nao_compra_nada() {
        let mut inv = bolsa(&[(item_id::COPPER, 95)], 2);
        let e = comprar_lote(&mut inv, POCAO, 10, 10, 999).unwrap_err();
        assert!(e.contains("faltam 5 de cobre"), "{e}");
        assert_eq!(crate::craft::tem(&inv, item_id::COPPER), 95);
        assert_eq!(crate::craft::tem(&inv, POCAO), 0);
    }

    #[test]
    fn sem_espaco_nao_cobra() {
        // A stack of 20: 45 potions need 3 slots and only 1 is free (+ the copper's,
        // which does not empty).
        let mut inv = bolsa(&[(item_id::COPPER, 10_000)], 1);
        let e = comprar_lote(&mut inv, POCAO, 10, 45, 20).unwrap_err();
        assert!(e.contains("bag full"), "{e}");
        assert_eq!(crate::craft::tem(&inv, item_id::COPPER), 10_000);
        // The copper the batch spends entirely frees its slot.
        let mut justo = bolsa(&[(item_id::COPPER, 200)], 1);
        assert_eq!(comprar_lote(&mut justo, POCAO, 10, 20, 10), Ok(200));
        assert_eq!(crate::craft::tem(&justo, POCAO), 20);
    }

    #[test]
    fn quantidade_fora_da_faixa() {
        let mut inv = bolsa(&[(item_id::COPPER, 10)], 1);
        assert!(comprar_lote(&mut inv, POCAO, 1, 0, 99).is_err());
        assert!(comprar_lote(&mut inv, POCAO, 1, MAX_LOTE + 1, 99).is_err());
    }
}
