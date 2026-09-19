//! Loja do NPC: comprar em LOTE. As regras, puras e testaveis; o `world.rs`
//! acha o vendedor por perto, chama e responde.
//!
//! A loja cobra em COBRE (docs/ECONOMIA.md). Um lote e' tudo ou nada: o cobre
//! do lote inteiro e o espaco pra ele sao conferidos antes de mexer na bolsa —
//! antes, comprar 50 eram 50 pedidos, e se o cobre acabava no meio chegavam
//! dezenas de recusas no chat.

use shared::{item_id, InventorySlot};

/// Maior lote de uma vez.
pub const MAX_LOTE: u32 = 999;

/// Compra `qtd` de `item` a `preco` cobre cada: tira o cobre e poe o item
/// (empilhado ate' `cap`). Devolve o total pago. Na recusa a bolsa nao muda e
/// o `Err` e' a frase que o jogador le.
pub fn comprar_lote(
    inv: &mut [InventorySlot],
    item: u16,
    preco: u32,
    qtd: u32,
    cap: u32,
) -> Result<u64, String> {
    if qtd == 0 || qtd > MAX_LOTE {
        return Err(format!("escolha de 1 a {MAX_LOTE}"));
    }
    let total = preco as u64 * qtd as u64;
    let tem = crate::craft::tem(inv, item_id::COPPER) as u64;
    if tem < total {
        return Err(format!(
            "faltam {} de cobre ({} por unidade)",
            total - tem,
            preco
        ));
    }
    let mut sim = inv.to_vec();
    crate::craft::consumir(&mut sim, item_id::COPPER, total as u32);
    if !crate::craft::por_empilhavel(&mut sim, item, qtd, cap) {
        return Err("bolsa cheia pra esse lote".into());
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
        // Pilha de 20: 45 pocoes pedem 3 slots e so' ha' 1 vago (+ o do
        // cobre, que nao esvazia).
        let mut inv = bolsa(&[(item_id::COPPER, 10_000)], 1);
        let e = comprar_lote(&mut inv, POCAO, 10, 45, 20).unwrap_err();
        assert!(e.contains("bolsa cheia"), "{e}");
        assert_eq!(crate::craft::tem(&inv, item_id::COPPER), 10_000);
        // O cobre que o lote gasta inteiro libera o slot dele.
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
