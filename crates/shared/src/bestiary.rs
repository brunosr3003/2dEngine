//! Island variants of the common mobs.
//!
//! Each island has its own roster (`server::economy::kinds_do_bioma`). Until
//! 01/10/2026 the Morganeers (Gunman, Mage, Archer) and a few beasts spawned
//! on several islands under the same name and look. The owner asked for every
//! map to have mobs of its own, so each island now gets its own version of
//! them.
//!
//! A variant is a new `kind` with its own name and model, tied to the
//! SPECIES it varies on (the archetype kind). The species is what everything
//! below the look keys on:
//!
//! - quests: "Defeat 8 archers" counts the Frostcoat Archer on the Glacier
//!   (`quests::kill_conta`), and the quest map looks for any of them;
//! - AI and attack: the mage's magic bolt, the archer's draw;
//! - client: rig, pose, sound.
//!
//! Numbers are the archetype's (server `economy::KINDS_INICIAIS`), so the
//! ladder and the balance sim see the same mob they already measure.

/// One island variant.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Variant {
    /// The new kind (`enemy_kinds.kind`). From `FIRST_VARIANT_KIND` up, clear
    /// of the boss catalogue (10-19) and of the retired sea kinds (20-24).
    pub kind: u16,
    /// The archetype kind it varies on (0-9).
    pub species: u16,
    pub name: &'static str,
    /// Where it spawns, for docs and tests: the island's zone.
    pub zone: &'static str,
}

/// The first kind used by variants.
pub const FIRST_VARIANT_KIND: u16 = 30;

/// Archetype kinds, as in `quests::mob_kind`.
mod species {
    pub const WOLF: u16 = 0;
    pub const BEAR: u16 = 1;
    pub const GUNMAN: u16 = 2;
    pub const TIGER: u16 = 3;
    pub const MAGE: u16 = 4;
    pub const OWLBEAR: u16 = 5;
    pub const ARCHER: u16 = 6;
}

pub const VARIANTS: &[Variant] = &[
    // Glacier
    Variant { kind: 30, species: species::ARCHER, name: "Frostcoat Archer", zone: "ilha_gelo" },
    Variant { kind: 31, species: species::MAGE, name: "Frostcoat Mage", zone: "ilha_gelo" },
    Variant { kind: 32, species: species::OWLBEAR, name: "Snow Owlbear", zone: "ilha_gelo" },
    // Waste
    Variant { kind: 33, species: species::GUNMAN, name: "Dune Raider", zone: "ilha_deserto" },
    Variant { kind: 34, species: species::ARCHER, name: "Sand Archer", zone: "ilha_deserto" },
    Variant { kind: 35, species: species::MAGE, name: "Sun Mage", zone: "ilha_deserto" },
    // Plateau
    Variant { kind: 36, species: species::TIGER, name: "Crag Lynx", zone: "ilha_planalto" },
    Variant { kind: 37, species: species::ARCHER, name: "Cliff Archer", zone: "ilha_planalto" },
    Variant { kind: 38, species: species::BEAR, name: "Cave Bear", zone: "ilha_planalto" },
    Variant { kind: 39, species: species::MAGE, name: "Storm Mage", zone: "ilha_planalto" },
    // Quest 766 ("The beasts of the wind") sends the player after owlbears ON
    // the Plateau, and until this variant no owlbear spawned there: the quest
    // map had nowhere to point.
    Variant { kind: 40, species: species::OWLBEAR, name: "Storm Owlbear", zone: "ilha_planalto" },
    // Skyreach: the winged ones (white and gold, hippogriff wings).
    Variant { kind: 41, species: species::TIGER, name: "Seraph Lynx", zone: "ilha_celeste" },
    Variant { kind: 42, species: species::ARCHER, name: "Seraph Archer", zone: "ilha_celeste" },
    Variant { kind: 43, species: species::BEAR, name: "Seraph Bear", zone: "ilha_celeste" },
    Variant { kind: 44, species: species::MAGE, name: "Seraph Mage", zone: "ilha_celeste" },
    Variant { kind: 45, species: species::OWLBEAR, name: "Seraph Owlbear", zone: "ilha_celeste" },
    Variant { kind: 46, species: species::WOLF, name: "Seraph Wolf", zone: "ilha_celeste" },
    // Kōgen-tō: the robots (gunmetal, plate seams, neon; `bichos.py:
    // BICHOS_ROBO`, `humanoides.py: capacete`). Kinds 47-55 are bosses.
    Variant { kind: 56, species: species::WOLF, name: "Mech Hound", zone: "ilha_kogen" },
    Variant { kind: 57, species: species::GUNMAN, name: "Gunner Bot", zone: "ilha_kogen" },
    Variant { kind: 58, species: species::TIGER, name: "Volt Panther", zone: "ilha_kogen" },
    Variant { kind: 59, species: species::ARCHER, name: "Laser Sentry", zone: "ilha_kogen" },
    Variant { kind: 60, species: species::BEAR, name: "Iron Bear", zone: "ilha_kogen" },
    Variant { kind: 61, species: species::MAGE, name: "Tesla Unit", zone: "ilha_kogen" },
    Variant { kind: 62, species: species::OWLBEAR, name: "Dynamo Owlbear", zone: "ilha_kogen" },
];

pub fn variant(kind: u16) -> Option<&'static Variant> {
    VARIANTS.iter().find(|v| v.kind == kind)
}

/// The species of a common mob: the archetype for a variant, the kind itself
/// for everything else. Not for bosses — their kinds share numbers with the
/// island mobs (10-19), and the body goes through `bosses::kind_do_corpo`.
pub fn species_of(kind: u16) -> u16 {
    variant(kind).map_or(kind, |v| v.species)
}

/// Every kind of a species: the archetype and its variants.
pub fn kinds_of_species(species: u16) -> Vec<u16> {
    std::iter::once(species)
        .chain(VARIANTS.iter().filter(|v| v.species == species).map(|v| v.kind))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn variant_kinds_are_unique_and_clear_of_bosses() {
        for (i, v) in VARIANTS.iter().enumerate() {
            assert!(v.kind >= FIRST_VARIANT_KIND, "{}", v.name);
            assert!(!crate::bosses::e_chefe(v.kind), "{} collides with a boss", v.name);
            assert!(VARIANTS[..i].iter().all(|o| o.kind != v.kind), "{} repeated", v.name);
            assert!(v.species < 10, "{} must vary on an archetype", v.name);
        }
    }

    #[test]
    fn species_resolves_variants_and_keeps_the_rest() {
        assert_eq!(species_of(30), species::ARCHER);
        assert_eq!(species_of(6), species::ARCHER);
        assert_eq!(species_of(13), 13);
        assert_eq!(kinds_of_species(species::ARCHER), vec![6, 30, 34, 37, 42, 59]);
    }

    #[test]
    fn variant_zones_exist() {
        for v in VARIANTS {
            assert!(crate::terreno::def_da_zona(v.zone).is_some(), "{}: {}", v.name, v.zone);
        }
    }
}
