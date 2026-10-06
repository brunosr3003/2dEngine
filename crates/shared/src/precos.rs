//! The market's RECOMMENDED price (docs/ECONOMIA.md).
//!
//! The owner's rule (05/10/2026): "how hard is it to get 1 gold, how much gold
//! is in the system, gold per hour, item per hour". Gold per hour divided by
//! items per hour is the gold there is divided by the units there are — the
//! time cancels out — so the price of an item is
//!
//! ```text
//! all the gold the characters hold / all the units of the item they hold
//! ```
//!
//! counted over EVERY character, and recomputed as the world changes. Then:
//!
//!   * anything the Combine makes (purple from blue, keys, pets, mounts) costs
//!     at most what making it costs: its inputs, the copper, the darksteel
//!     and the Powder, divided by the chance. An item nobody holds yet gets
//!     that price too;
//!   * a piece of gear is its place on the forge ladder (`forja::custo_total`:
//!     base pieces for its colour and tier, the refine attempts on top) at the
//!     price of one Common I piece, which the NPC shop sells for copper;
//!   * a pet adds the skill books it has learned;
//!   * a CASH item (the TP shop) costs at most its TP price in gold, and gold
//!     per TP follows the same rule as the items: all the gold there is over
//!     all the TP the accounts hold — but never under what the shop's own
//!     Sack of Gold pays for a TP (`OURO_POR_TP_DA_LOJA`). What a summon
//!     rolls costs the scroll over the chance of that very roll.
//!
//! It is only a suggestion: the seller types any price, 1 included.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::constants::{equip_slot_of, item_id, EquipSlot};
use crate::forja::{self, Degrau, Grau};
use crate::items::ItemInstance;

/// Below this many units in the world, the ratio says more about luck than
/// about the item: no recommendation from the stock (a crafted price may
/// still exist).
pub const AMOSTRA_MIN: u64 = 20;

/// The table's line for ONE TP, in gold (item 0 is no item).
pub const ID_DO_TP: u16 = 0;

/// Gold one TP buys at the shop (Sack of Gold): the floor of the TP's price.
pub fn ouro_por_tp_da_loja() -> f64 {
    crate::loja::MOEDAS
        .iter()
        .filter(|m| m.item_id == item_id::GOLD && m.preco_tp > 0)
        .map(|m| m.qtd as f64 / m.preco_tp as f64)
        .fold(0.0, f64::max)
}

/// What each cash item costs in TP at the shop, the cheapest way. A summon's
/// outcome costs the scroll over the chance of rolling exactly that.
pub fn custos_em_tp() -> Vec<(u16, f64)> {
    use crate::loja;
    let mut v: Vec<(u16, f64)> = Vec::new();
    let mut poe = |id: u16, tp: f64| {
        if tp.is_finite() && tp > 0.0 {
            match v.iter_mut().find(|(i, _)| *i == id) {
                Some((_, x)) => *x = x.min(tp),
                None => v.push((id, tp)),
            }
        }
    };
    for i in loja::ITENS_DA_LOJA {
        poe(i.item_id, i.preco_tp as f64);
    }
    for i in loja::itens_de_pet() {
        poe(i.item_id, i.preco_tp as f64);
    }
    for m in loja::MOEDAS.iter().filter(|m| m.item_id != item_id::GOLD && m.qtd > 0) {
        poe(m.item_id, m.preco_tp as f64 / m.qtd as f64);
    }
    // Energy, by the lot the market sells it in.
    for e in loja::ENERGIAS.iter().filter(|e| e.qtd > 0) {
        poe(item_id::ENERGIA_MIL, e.preco_tp as f64 * crate::mercado::ENERGIA_POR_LOTE as f64 / e.qtd as f64);
    }
    for p in loja::PASSES {
        poe(item_id::PASSE_MAGICO, p.tp_por_passe() as f64);
    }
    // The scrolls themselves (the bound ones never reach the market anyway).
    for b in loja::BAUS_CRAFT {
        poe(item_id::PERGAMINHO_INVOCA_CHAVE, b.preco_tp as f64);
        // A key: one of four kinds, in a colour by the scroll's odds.
        for (k, &chance) in b.chances_cor.iter().enumerate() {
            for &base in &item_id::CHAVES {
                poe(item_id::chave_na_cor(base, k as u8 + 1), b.preco_tp as f64 * 4.0 * 100.0 / chance.max(1) as f64);
            }
        }
    }
    for t in loja::PERGAMINHOS_TOMO {
        poe(item_id::PERGAMINHO_INVOCA_TOMO, t.preco_tp as f64);
    }
    for m in loja::PERGAMINHOS_MONTARIA {
        poe(item_id::PERGAMINHO_INVOCA_MONTARIA, m.preco_tp as f64);
        for (k, &chance) in crate::montarias::CHANCES_DO_PERGAMINHO.iter().enumerate() {
            for &base in &item_id::MONTARIAS {
                poe(item_id::montaria_no_grau(base, k as u8 + 1), m.preco_tp as f64 * 100.0 / chance.max(1) as f64);
            }
        }
    }
    for p in loja::PERGAMINHOS_PET {
        poe(item_id::PERGAMINHO_INVOCA_PET, p.preco_tp as f64);
        for (k, &chance) in crate::pets::CHANCES_DO_PERGAMINHO.iter().enumerate() {
            for &base in &item_id::PETS {
                poe(item_id::pet_no_grau(base, k as u8 + 1), p.preco_tp as f64 * 100.0 / chance.max(1) as f64);
            }
        }
    }
    v
}

/// What the realm holds, summed over every character.
#[derive(Debug, Clone, Default)]
pub struct Estoque {
    /// Gold in every purse.
    pub ouro: u64,
    /// Units of each item: bags, vaults, equipped, on sale.
    pub unidades: HashMap<u16, u64>,
    /// What an NPC shop charges for a Common I piece of each gear item, in
    /// copper (`items.buy_price`).
    pub base_em_cobre: HashMap<u16, u64>,
    /// TP every account holds (and on sale).
    pub tp: u64,
}

/// Gold per unit, item by item. Fractional on purpose: copper is worth a
/// fraction of a gold, and the gear price multiplies it by thousands.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct TabelaDePrecos {
    pub precos: Vec<(u16, f64)>,
}

impl TabelaDePrecos {
    pub fn mapa(&self) -> HashMap<u16, f64> {
        self.precos.iter().copied().collect()
    }
}

/// The table, from the stock.
pub fn calcular(e: &Estoque) -> TabelaDePrecos {
    let mut p: HashMap<u16, f64> = HashMap::new();
    if e.ouro > 0 {
        for (&id, &n) in &e.unidades {
            if id != item_id::GOLD && n >= AMOSTRA_MIN {
                p.insert(id, e.ouro as f64 / n as f64);
            }
        }
    }
    // THE TP: gold per TP like any item (gold over TP held), floored by the
    // shop's own exchange. Then every cash item is at most its TP in gold.
    let por_estoque = if e.tp > 0 { e.ouro as f64 / e.tp as f64 } else { 0.0 };
    let ouro_por_tp = por_estoque.max(ouro_por_tp_da_loja());
    if ouro_por_tp > 0.0 {
        p.insert(ID_DO_TP, ouro_por_tp);
        for (id, tp) in custos_em_tp() {
            let v = tp * ouro_por_tp;
            p.entry(id).and_modify(|x| *x = x.min(v)).or_insert(v);
        }
    }
    // THE COMBINE CAPS (and fills): lower colours first, so a green made from
    // grey is known before the blue made from it.
    let mut receitas = crate::combinar::receitas();
    receitas.sort_by_key(|r| r.cor);
    for r in receitas {
        let Some(&entrada) = p.get(&r.entrada) else {
            continue;
        };
        let custo_de = |id: u16, qtd: u32| -> Option<f64> {
            if qtd == 0 {
                Some(0.0)
            } else {
                p.get(&id).map(|v| v * qtd as f64)
            }
        };
        let (Some(cu), Some(ds), Some(po)) = (
            custo_de(item_id::COPPER, r.cobre),
            custo_de(item_id::DARKSTEEL, r.darksteel),
            custo_de(item_id::GLITTERING_POWDER, r.po),
        ) else {
            continue;
        };
        if r.chance == 0 {
            continue;
        }
        let custo = (entrada * r.qtd as f64 + cu + ds + po) * 100.0 / r.chance as f64;
        p.entry(r.saida)
            .and_modify(|v| *v = v.min(custo))
            .or_insert(custo);
    }
    // GEAR: the value is the Common I piece; the instance adds the ladder.
    if let Some(&cobre) = p.get(&item_id::COPPER) {
        for (&id, &em_cobre) in &e.base_em_cobre {
            if e_peca_da_forja(id) && em_cobre > 0 {
                p.insert(id, em_cobre as f64 * cobre);
            }
        }
    }
    let mut precos: Vec<(u16, f64)> = p.into_iter().filter(|(_, v)| v.is_finite() && *v > 0.0).collect();
    precos.sort_by_key(|(id, _)| *id);
    TabelaDePrecos { precos }
}

/// Weapon, offhand, armour, jewellery: the pieces that climb the forge ladder.
pub fn e_peca_da_forja(id: u16) -> bool {
    matches!(
        equip_slot_of(id),
        Some(
            EquipSlot::Weapon
                | EquipSlot::Offhand
                | EquipSlot::Armor
                | EquipSlot::Earring
                | EquipSlot::Necklace
                | EquipSlot::Bracelet
                | EquipSlot::Belt
        )
    )
}

/// One gear piece: its colour, tier and refine on the forge ladder, at the
/// price of a Common I piece (`base`), with the refine attempts' darksteel
/// and copper at what the game charges for them.
pub fn preco_da_peca(t: &HashMap<u16, f64>, base: f64, inst: &ItemInstance) -> f64 {
    let grau = Grau::de_u8(inst.rarity.clamp(1, 5)).unwrap_or(Grau::Comum);
    let degrau = Degrau::novo(grau, inst.tier.max(1));
    let c = forja::custo_total(degrau, inst.refinement, Degrau::novo(Grau::Comum, 1));
    let div = forja::DIVISOR_DO_CUSTO_EM_JOGO as f64;
    let ds = t.get(&item_id::DARKSTEEL).copied().unwrap_or(0.0);
    let cu = t.get(&item_id::COPPER).copied().unwrap_or(0.0);
    c.pecas_base * base + c.darksteel / div * ds + c.cobre / div * cu
}

/// The recommended price of what sits in a bag slot, in whole gold (at
/// least 1). `None` when the world does not say enough about it yet.
pub fn recomendado(t: &HashMap<u16, f64>, id: u16, inst: Option<&ItemInstance>) -> Option<u64> {
    let base = *t.get(&id)?;
    let mut ouro = match inst {
        Some(i) if e_peca_da_forja(id) => preco_da_peca(t, base, i),
        _ => base,
    };
    // A pet carries the skill books it learned.
    if let Some(pet) = inst.and_then(|i| i.pet.as_ref()) {
        for &s in pet.skills.iter().filter(|&&s| s != 0) {
            ouro += t.get(&s).copied().unwrap_or(0.0);
        }
    }
    ouro.is_finite().then(|| ouro.round().max(1.0) as u64)
}

#[cfg(test)]
mod testes {
    use super::*;

    fn estoque(ouro: u64, itens: &[(u16, u64)]) -> Estoque {
        Estoque {
            ouro,
            unidades: itens.iter().copied().collect(),
            base_em_cobre: HashMap::new(),
            tp: 0,
        }
    }

    /// The owner's rule: gold there is / units there are.
    #[test]
    fn o_preco_e_o_ouro_dividido_pelas_unidades() {
        let t = calcular(&estoque(100_000, &[(item_id::STEEL, 1_000), (item_id::COPPER, 4_000_000)])).mapa();
        assert!((t[&item_id::STEEL] - 100.0).abs() < 1e-9);
        assert!((t[&item_id::COPPER] - 0.025).abs() < 1e-9);
        // Copper is worth a fortieth of a gold, but the market's floor is 1.
        assert_eq!(recomendado(&t, item_id::COPPER, None), Some(1));
    }

    /// Two units in the world say nothing: no price from the stock.
    #[test]
    fn amostra_pequena_nao_tem_preco() {
        let t = calcular(&estoque(100_000, &[(item_id::STEEL, AMOSTRA_MIN - 1)])).mapa();
        assert!(!t.contains_key(&item_id::STEEL));
    }

    /// Purple is never dearer than the synthesis that makes it, and exists
    /// even when nobody holds one.
    #[test]
    fn roxo_custa_no_maximo_a_sintese() {
        let azul = item_id::na_cor(item_id::STEEL, 3);
        let roxo = item_id::na_cor(item_id::STEEL, 4);
        let t = calcular(&estoque(
            1_000_000,
            &[
                (azul, 10_000),
                (item_id::COPPER, 100_000_000),
                (item_id::DARKSTEEL, 1_000_000),
                (item_id::GLITTERING_POWDER, 10_000),
            ],
        ))
        .mapa();
        let (cu, ds, po) = crate::combinar::custo_da_sintese(3);
        let esperado = 10.0 * t[&azul]
            + cu as f64 * t[&item_id::COPPER]
            + ds as f64 * t[&item_id::DARKSTEEL]
            + po as f64 * t[&item_id::GLITTERING_POWDER];
        assert!((t[&roxo] - esperado).abs() < 1e-6, "{} vs {esperado}", t[&roxo]);
    }

    /// The forge ladder multiplies: a Rare IV +7 is ~34 thousand Common I.
    #[test]
    fn a_peca_segue_a_escada_da_forja() {
        let mut e = estoque(1_000_000, &[(item_id::COPPER, 1_000_000)]);
        e.tp = 1;
        e.base_em_cobre.insert(item_id::KATANA, 1_000);
        let t = calcular(&e).mapa();
        let base = t[&item_id::KATANA];
        assert!((base - 1_000.0).abs() < 1e-9, "Common I = shop copper x copper price");
        let mut i = ItemInstance::vazia_de_grau(1);
        i.tier = 1;
        let comum = recomendado(&t, item_id::KATANA, Some(&i)).unwrap();
        i.rarity = 3;
        i.tier = 4;
        i.refinement = 7;
        let raro = recomendado(&t, item_id::KATANA, Some(&i)).unwrap();
        assert_eq!(comum, 1_000);
        assert!(raro as f64 > 34_000.0 * base * 0.99, "{raro}");
    }

    /// Cash items: their TP in gold. Gold per TP is the stock's ratio, never
    /// under the shop's Sack of Gold; a rare summon costs the scroll over its
    /// chance.
    #[test]
    fn itens_de_cash_valem_o_tp_em_ouro() {
        let loja = ouro_por_tp_da_loja();
        assert!((loja - 200.0).abs() < 1e-9, "Sack of Gold: 10,000 for 50 TP");
        // Little gold per TP in the world: the shop's floor holds.
        let t = calcular(&Estoque { ouro: 1_000, tp: 1_000, ..Default::default() }).mapa();
        assert!((t[&ID_DO_TP] - 200.0).abs() < 1e-9);
        assert!((t[&item_id::RACAO_DE_PET] - 30.0 * 200.0).abs() < 1e-6, "Pet Feed: 30 TP");
        // Much gold per TP: the stock's ratio rules.
        let t = calcular(&Estoque { ouro: 1_000_000, tp: 1_000, ..Default::default() }).mapa();
        assert!((t[&ID_DO_TP] - 1_000.0).abs() < 1e-9);
        // The orange mount: 500 TP at 1% = 50,000 TP.
        let laranja = item_id::montaria_no_grau(item_id::MONTARIAS[0], 5);
        assert!((t[&laranja] - 50_000.0 * 1_000.0).abs() < 1e-3, "{}", t[&laranja]);
        // A material held in bulk stays at its own (lower) ratio.
        let t = calcular(&Estoque { ouro: 1_000_000, tp: 1_000, unidades: [(item_id::COPPER, 10_000_000)].into_iter().collect(), ..Default::default() }).mapa();
        assert!((t[&item_id::COPPER] - 0.1).abs() < 1e-9);
    }

    /// A pet is worth its kind plus the books it learned.
    #[test]
    fn pet_leva_as_skills() {
        let pet = item_id::pet_no_grau(item_id::PETS[0], 1);
        let livro = item_id::SKILL_PET_FARO;
        let t = calcular(&estoque(100_000, &[(pet, 100), (livro, 100)])).mapa();
        let mut i = ItemInstance::vazia_de_grau(1);
        let sem = recomendado(&t, pet, Some(&i)).unwrap();
        i.pet = Some(crate::items::PetData { skills: [livro, 0, 0], ..Default::default() });
        let com = recomendado(&t, pet, Some(&i)).unwrap();
        assert_eq!(com, sem + 1_000);
    }
}
