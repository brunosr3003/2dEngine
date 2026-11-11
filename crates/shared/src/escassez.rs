//! What monsters stop dropping (owner, 11/11/2026): craft resources come from
//! the ROCK, so they are scarce and the market has something to trade.
//!
//! - Darksteel: only from rocks (plus the login calendar, the shop and gifts).
//!   Monsters, dungeon chests and the Stormkeep chests no longer give it.
//! - The eight colored craft materials (Steel, Platinum, Blackheart,
//!   Moonshadow, Quintessence, Exorcism, Illuminating, Anima), in any color:
//!   monsters drop them at a tenth of the old chance. Rocks are unchanged.
//!
//! One pure rule used by the loot roll AND by "Where to get", so the screen
//! never promises a drop that no longer happens.

use crate::item_id;

/// How much of a monster's table chance survives for `item` (0 = never).
pub fn fator_em_mob(item: u16) -> f32 {
    if item == item_id::DARKSTEEL {
        return 0.0;
    }
    if e_material_de_craft(item) {
        return MATERIAL_EM_MOB;
    }
    1.0
}

/// Monsters drop craft materials at 10% of what their table says.
pub const MATERIAL_EM_MOB: f32 = 0.10;

/// One of the eight colored craft materials, in any of its four colors.
pub fn e_material_de_craft(item: u16) -> bool {
    crate::combinar::MATERIAIS
        .iter()
        .any(|&b| item >= b && item < b + 4)
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn darksteel_nunca_e_material_cai_a_um_decimo() {
        assert_eq!(fator_em_mob(item_id::DARKSTEEL), 0.0);
        for b in crate::combinar::MATERIAIS {
            for cor in 1..=4 {
                assert_eq!(fator_em_mob(item_id::na_cor(b, cor)), MATERIAL_EM_MOB);
            }
        }
        // Copper, gold, keys and potions are untouched.
        for id in [item_id::COPPER, item_id::GOLD, item_id::SCALE, item_id::HIDE, item_id::GLITTERING_POWDER] {
            assert_eq!(fator_em_mob(id), 1.0, "{id}");
        }
    }
}
