//! What REBIRTH ESSENCE buys (docs/RESET.md, rebirth phase 3): GEAR
//! ASCENSION (stars on a piece) and the ESSENCE SHOP.
//!
//! Essence (`item_id::ESSENCIA_RENASCIMENTO`, bound) is paid by every rebirth
//! (`renascimento::essencia_do_renascimento`, 10·n). It is spent from the BAG
//! only, like the Forge's Darksteel and Copper.
//!
//! **Ascension.** A gear piece (weapon, offhand, armour and the four
//! accessories — never a pet or a mount: their power comes from their own
//! level and affinity, not from the piece's stats) climbs from 0 to
//! `ESTRELAS_MAX` stars at the Forge. Each star adds `BONUS_POR_ESTRELA`
//! (8%) of the piece's own base stats, summed with the refine's share
//! (`ItemInstance::effective_bonus`: base × (1 + 4%·refine + 8%·stars)), so 5
//! stars = +40%, ten refines' worth. It ALWAYS succeeds (unlike the refine)
//! and costs Essence doubling per star plus Darksteel (`custo_da_estrela`).
//! The stars live on the instance (`ItemInstance::estrelas`), so trade, the
//! market, the bank and the mail keep them; upgrading two pieces into one
//! keeps the higher count of the two; a piece destroyed by a refine takes its
//! stars with it.
//!
//! **Shop.** `OFERTAS`: the path-tree reset for Essence instead of gold,
//! the four legendary craft keys, Ward Charms and four existing cosmetics.
//! Bought from the Character Sheet's ESSENCE button; the server checks and
//! delivers bag-first, mail for what does not fit.

use crate::aparencia;
use crate::constants::item_id;

/// Most stars a piece can have.
pub const ESTRELAS_MAX: u8 = 5;
/// Share of the piece's base stats each star adds (+8%; 5 stars = +40%).
pub const BONUS_POR_ESTRELA: f32 = 0.08;

/// Essence to go from `estrelas` to `estrelas + 1`, and the Darksteel with
/// it: 10/20/40/80/160 Essence (310 to fill a piece = the Essence of the
/// first ~7 rebirths) and 2k/4k/8k/16k/32k Darksteel. `None` at the top.
pub fn custo_da_estrela(estrelas: u8) -> Option<(u32, u32)> {
    if estrelas >= ESTRELAS_MAX {
        return None;
    }
    let k = 1u32 << estrelas;
    Some((10 * k, 2_000 * k))
}

/// Total Essence from 0 to `ESTRELAS_MAX` stars on one piece.
pub fn essencia_ate_o_topo() -> u32 {
    (0..ESTRELAS_MAX).filter_map(custo_da_estrela).map(|c| c.0).sum()
}

/// The multiplier share of `estrelas` stars (0.08 per star, capped at the top).
pub fn bonus_das_estrelas(estrelas: u8) -> f32 {
    estrelas.min(ESTRELAS_MAX) as f32 * BONUS_POR_ESTRELA
}

/// Can item `id` be ascended? Gear with an equip slot, not a pet nor a mount.
pub fn ascendivel(id: u16) -> bool {
    crate::equip_slot_of(id).is_some()
        && crate::pets::de_item(id).is_none()
        && crate::montarias::de_item(id).is_none()
        && !crate::item_id::e_navio(id)
}

/// "★★★☆☆" for the item card. Empty with no star, so pieces never
/// ascended look as they always did.
pub fn texto_das_estrelas(estrelas: u8) -> String {
    let n = estrelas.min(ESTRELAS_MAX) as usize;
    if n == 0 {
        return String::new();
    }
    let mut s = "★".repeat(n);
    s.push_str(&"☆".repeat(ESTRELAS_MAX as usize - n));
    s
}

/// Mail reason (`CartaDeCorreio::motivo`) of a shop purchase that did not
/// fit the bag (5 is the rebirth chest).
pub const MOTIVO_CORREIO_LOJA_ESSENCIA: u8 = 6;

/// What an offer delivers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Entrega {
    /// Every path point back (`arvore`), like the gold reset.
    RedistribuirArvore,
    /// `qtd` of item `id` to the bag (skins arrive as their bag item and
    /// unlock when used, as the TP shop's do).
    Item { id: u16, qtd: u32 },
}

/// One row of the shop. `id` is what the client sends: stable, never reused.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Oferta {
    pub id: u8,
    /// English (the client translates).
    pub nome: &'static str,
    pub descricao: &'static str,
    pub preco: u32,
    pub entrega: Entrega,
}

/// Essence to reset the path tree (flat; the gold way costs 1,000 a point).
pub const ESSENCIA_REDISTRIBUIR: u32 = 20;
/// Essence for one legendary craft key.
pub const PRECO_CHAVE_LENDARIA: u32 = 60;

const fn chave(id: u8, nome: &'static str, base: u16) -> Oferta {
    Oferta {
        id,
        nome,
        descricao: "A legendary craft key, for the best Forge recipes.",
        preco: PRECO_CHAVE_LENDARIA,
        entrega: Entrega::Item { id: item_id::chave_na_cor(base, 5), qtd: 1 },
    }
}

/// THE SHOP, in display order. Append only (the id is on the wire).
pub const OFERTAS: [Oferta; 10] = [
    Oferta {
        id: 1,
        nome: "Path reset",
        descricao: "Every path point back to spend again, without the gold.",
        preco: ESSENCIA_REDISTRIBUIR,
        entrega: Entrega::RedistribuirArvore,
    },
    chave(2, "Legendary Scale", item_id::SCALE),
    chave(3, "Legendary Claw", item_id::CLAW),
    chave(4, "Legendary Horn", item_id::HORN),
    chave(5, "Legendary Leather", item_id::HIDE),
    Oferta {
        id: 6,
        nome: "Ward Charms ×3",
        descricao: "Refining to +6 or +7, a failure spends a charm and the piece survives.",
        preco: 15,
        entrega: Entrega::Item { id: item_id::AMULETO_DE_PROTECAO, qtd: 3 },
    },
    Oferta {
        id: 7,
        nome: "Crown",
        descricao: "Hat for your wardrobe: use it from the bag to unlock.",
        preco: 40,
        entrega: Entrega::Item { id: aparencia::CHAPEU_BASE + 13, qtd: 1 },
    },
    Oferta {
        id: 8,
        nome: "Knight",
        descricao: "Outfit for your wardrobe: use it from the bag to unlock.",
        preco: 40,
        entrega: Entrega::Item { id: aparencia::ITEM_ROUPA_BASE + 8, qtd: 1 },
    },
    Oferta {
        id: 9,
        nome: "Void Ring",
        descricao: "Magic ring skin for your wardrobe: use it from the bag to unlock.",
        preco: 50,
        entrega: Entrega::Item { id: aparencia::ARMA_SKIN_BASE + 13, qtd: 1 },
    },
    Oferta {
        id: 10,
        nome: "Spectral Coat",
        descricao: "Mount skin for your wardrobe: use it from the bag to unlock.",
        preco: 50,
        entrega: Entrega::Item { id: aparencia::MONTARIA_SKIN_BASE + 4, qtd: 1 },
    },
];

pub fn oferta(id: u8) -> Option<&'static Oferta> {
    OFERTAS.iter().find(|o| o.id == id)
}

/// Why an ascension was refused (English: the client translates).
pub const NAO_ASCENDIVEL: &str = "Only gear can be ascended (not pets or mounts).";
pub const NO_TOPO: &str = "This piece already has every star.";

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn custo_dobra_e_para_no_topo() {
        assert_eq!(custo_da_estrela(0), Some((10, 2_000)));
        assert_eq!(custo_da_estrela(1), Some((20, 4_000)));
        assert_eq!(custo_da_estrela(4), Some((160, 32_000)));
        assert_eq!(custo_da_estrela(5), None);
        assert_eq!(custo_da_estrela(200), None);
        assert_eq!(essencia_ate_o_topo(), 310);
        // The first star is paid by the first rebirth alone.
        assert!(custo_da_estrela(0).unwrap().0 <= crate::renascimento::essencia_do_renascimento(1));
    }

    #[test]
    fn estrelas_somam_8_pct_ate_40() {
        assert_eq!(bonus_das_estrelas(0), 0.0);
        assert!((bonus_das_estrelas(1) - 0.08).abs() < 1e-6);
        assert!((bonus_das_estrelas(5) - 0.40).abs() < 1e-6);
        assert!((bonus_das_estrelas(9) - 0.40).abs() < 1e-6, "capped");
        assert_eq!(texto_das_estrelas(0), "");
        assert_eq!(texto_das_estrelas(3), "★★★☆☆");
        assert_eq!(texto_das_estrelas(5), "★★★★★");
    }

    #[test]
    fn estrela_sobe_os_atributos_da_peca() {
        let mut i = crate::ItemInstance::roll_for(item_id::KATANA, 30, || 0.5).unwrap();
        let antes = i.effective_bonus();
        assert!(antes.attack_damage > 0);
        i.estrelas = 5;
        let depois = i.effective_bonus();
        let esperado = (i.attack_damage as f32 * (i.refine_mult() + 0.40)).round() as i32;
        assert_eq!(depois.attack_damage, esperado);
        assert!(depois.attack_damage > antes.attack_damage);
        // Refine and stars add, both on the base.
        i.refinement = 5;
        let ambos = i.effective_bonus();
        assert_eq!(ambos.attack_damage, (i.attack_damage as f32 * 1.60).round() as i32);
    }

    #[test]
    fn so_equipamento_ascende() {
        assert!(ascendivel(item_id::KATANA));
        assert!(!ascendivel(item_id::DARKSTEEL));
        assert!(!ascendivel(item_id::pet_no_grau(item_id::PET_BASE, 3)));
        assert!(!ascendivel(item_id::montaria_no_grau(item_id::MONTARIA_BASE, 3)));
    }

    #[test]
    fn loja_tem_ids_unicos_e_entregas_validas() {
        let mut ids: Vec<u8> = OFERTAS.iter().map(|o| o.id).collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), OFERTAS.len(), "repeated offer id");
        for o in &OFERTAS {
            assert!(o.preco > 0, "{} is free", o.nome);
            assert_eq!(oferta(o.id), Some(o));
            if let Entrega::Item { id, qtd } = o.entrega {
                assert!(qtd > 0);
                assert!(id != 0);
            }
        }
        assert_eq!(oferta(1).unwrap().entrega, Entrega::RedistribuirArvore);
        assert_eq!(oferta(1).unwrap().preco, 20);
        for (o, base) in [(2, item_id::SCALE), (3, item_id::CLAW), (4, item_id::HORN), (5, item_id::HIDE)] {
            let Entrega::Item { id, .. } = oferta(o).unwrap().entrega else { panic!() };
            assert_eq!(id, item_id::chave_na_cor(base, 5));
            assert_eq!(item_id::cor_de_id(id), Some(5), "legendary");
        }
        // The cosmetics are existing, paid skins (no new art).
        for o in 7..=10 {
            let Entrega::Item { id, .. } = oferta(o).unwrap().entrega else { panic!() };
            let skin = aparencia::skin_do_item(id).expect("a skin item");
            assert!(aparencia::preco_da_skin(skin).is_some(), "skin {skin} is a paid one");
            assert_eq!(aparencia::nome_da_skin(skin), Some(oferta(o).unwrap().nome));
        }
        assert!(oferta(0).is_none() && oferta(99).is_none());
    }
}
