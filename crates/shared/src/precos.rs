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
//!   * a pet adds the skill books it has learned.
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
