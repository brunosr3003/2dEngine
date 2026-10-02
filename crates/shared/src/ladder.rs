//! THE LADDER: what is expected of a character at each level, and everything
//! that derives from it (docs/ESCADA.md).
//!
//! Owner's request (27/09/2026): *"level does not have to say anything, the
//! attributes do; the big flaw is in how defense is calculated today, in % and
//! not def - attack; re-think the best possible plan so it stays balanced and
//! scalable as the game flows - level 35 needing so many items with so much
//! defense and attack, level 40 that much more, and so on."*
//!
//! ## The flaw this fixes
//!
//! Until now defense was `1.5% per point, cap 75%`, added to fixed percentages
//! (VIT and RES steps, armor weight, shield) up to 90%. A fixed percentage does
//! not look at who is hitting: 50 defense cut 75% off a level 1 wolf and 75%
//! off a level 60 Colossus. The cap was reached at 50 points - an F2P with no
//! refinement was already there - and from then on the mob's attack going up
//! changed nothing. It was measured (27/09): from 20 to 45 the owner's character
//! cleared the zone on 94-98% health with no potion, and the "reference" the
//! game used to state recommended power had a third of the defense of any real
//! player.
//!
//! ## The rule
//!
//! ```text
//! damage = max(attack - defense, attack x FLOOR)
//! ```
//!
//! Both ways (mob -> player, player -> mob, PvP). It looks at nobody's level.
//! The same defense yields different results depending on who is hitting, and
//! that is what makes equipment AGE: what leaves you nearly immune at 31 takes
//! half the blow at 51. The floor prevents immunity - a weak mob always nicks
//! you, and twenty of them add up.
//!
//! A consequence that comes for free: a big blow pierces armor, a small one
//! does not. Ten little mobs hitting 40 against defense 70 give 10 x 8; a boss
//! hitting 400 gives 330. The telegraphed boss hurts again for someone who
//! refined everything, with no special rule.
//!
//! ## The ladder
//!
//! Three straight lines: the attack, defense and health EXPECTED of a character
//! at the level, equipped with their own tier at +0 and points spread the way
//! `balanceamento::build_do_nivel` already assumes (a third in the primary, two
//! in VIT). Straight, so the ratio between two levels never changes: the mob at
//! 40 is to the player at 40 exactly as the one at 20 is to the one at 20.
//!
//! Everything else derives from here, and nothing has a number of its own:
//!
//! * **item**: the template (in the database) is only the PROPORTION between
//!   pieces; the scale comes from `*_scale` - a reference set at +0, in the
//!   level's natural grade, adds up to exactly the items' slice of the ladder;
//! * **common mob**: `mob()` - attack, defense and health come from goals
//!   against the expected player, and the species enters as a PROFILE relative
//!   to the wolf;
//! * **boss**: `bosses::damage/defense` read from here;
//! * **recommended power**: `dungeon::poder_referencia` is the expected
//!   character's power - which is why it compares against the sheet;
//! * **refinement**: only a percentage of the piece itself
//!   (`items::REFINE_BOOST_PER_LEVEL`), with no fixed part: +12 of one tier is
//!   about +0 of the tier fifteen levels above. Cash buys a head start, not
//!   immunity.
//!
//! What stays a percentage: only what is IDENTITY (shield, heavy armor),
//! applied AFTER the subtraction and capped at `MAX_REDUCTION`. The per-point
//! steps (VIT/25, RES/30) are gone - they were the same flaw.
//!
//! ## The guard
//!
//! `balanceamento::metas_da_escada` (server) runs the simulator every five
//! levels from 20 to 60, in three profiles (right tier +0, one tier behind,
//! refined) and three places (zone, fort, magic islet). Touch a mob, an item, a
//! refinement or a point and break the ratio, and `cargo test -p server --bins`
//! fails. "Always balanced" is a test, not an intention.

use serde::{Deserialize, Serialize};

// ───────────────────────────── the ruler ─────────────────────────────

/// The straight lines. The base absorbs what every character has with no level
/// at all: the 20 attack and 100 health from `base_player_stats` and the LEGACY
/// of the pieces (`legacy()`, below).
pub const BASE_ATTACK: f32 = 45.0;
pub const ATTACK_PER_LEVEL: f32 = 5.0;
pub const BASE_DEFENSE: f32 = 10.0;
pub const DEFENSE_PER_LEVEL: f32 = 2.0;
pub const BASE_HEALTH: f32 = 200.0;
pub const HEALTH_PER_LEVEL: f32 = 20.0;

/// Expected attack at the level (equipped in tier, +0).
pub fn attack(level: u32) -> i32 {
    (BASE_ATTACK + ATTACK_PER_LEVEL * level as f32).round() as i32
}

/// Expected defense at the level.
pub fn defense(level: u32) -> i32 {
    (BASE_DEFENSE + DEFENSE_PER_LEVEL * level as f32).round() as i32
}

/// Expected health at the level.
pub fn health(level: u32) -> i32 {
    (BASE_HEALTH + HEALTH_PER_LEVEL * level as f32).round() as i32
}

// ───────────────────────────── the blow ─────────────────────────────

/// The fraction of a blow that gets through any defense. It is what prevents
/// immunity: a mob fifteen levels below still nicks you, and a horde of them
/// still adds up.
///
/// **From 0.10 to 0.06 on 27/09/2026**, at the owner's request, while playing
/// the Magic Island. The floor is the ONLY thing a horde delivers to someone
/// who already has defense to spare, which is why it decides whether the horde
/// is playable: at 0.10 a level 20-30 mob (attack 58-81) took 6 to 8 per blow
/// from someone on the floor, and twelve of them added up to more than any
/// defense could answer. At 0.06 the same blow takes 3 to 5.
///
/// The floor is a FRACTION, not a number: against a level 60 mob (attack 151)
/// it still takes 9. That is what makes equipment age, and it is on purpose.
///
/// Measured before touching it: lowering the floor **does not move the ladder**.
/// The islet table came out identical at 0.10 and at 0.04, because whoever is
/// IN TIER never touches the floor - only someone with too much defense for
/// what they are facing does. The price is in refinement, and it is written in `metas_da_escada`.
///
/// **Corrected to 0.085 the same day**, still playing: 0.06 took too much. The
/// owner asked for "4-7 the minimal damage" against the same mobs where 0.10
/// took 5-8 - 4/5 to 7/8 is 0.85 of what it was, and `0.10 x 0.85 = 0.085`.
/// Against attack 50-80 it gives 4.3 to 6.8; against a level 60 mob it gives 13.
pub const FLOOR: f32 = 0.085;

/// Cap on the IDENTITY percentage reductions (shield, heavy armor), applied
/// after the subtraction. Shield (0.40) plus heavy (0.10) land exactly here:
/// the shield-and-heavy-armor tank is the cap.
pub const MAX_REDUCTION: f32 = 0.50;

/// The blow: `max(attack - defense, attack x FLOOR)`, never below 1.
pub fn damage(attack: i32, defense: i32) -> i32 {
    let a = attack.max(0) as f32;
    let liquido = (a - defense.max(0) as f32).max(a * FLOOR);
    (liquido.round() as i32).max(1)
}

/// The blow with the target's identity reduction on top (cap `MAX_REDUCTION`).
pub fn damage_with_reduction(attack: i32, defense: i32, reduction: f32) -> i32 {
    let base = damage(attack, defense) as f32;
    let r = reduction.clamp(0.0, MAX_REDUCTION);
    ((base * (1.0 - r)).round() as i32).max(1)
}

// ─────────────────────── what the items have to give ───────────────────────

/// THE REFERENCE SET: katana, scabbard, medium armor and the four accessories.
/// The ladder is its own; sword and shield with heavy sits above it in defense
/// and health, pistol and ring with light below - that is the armor weight, and
/// it is a choice (`metas_da_escada::a_referencia_anda_na_escada` holds the
/// corridor).
pub const REFERENCE_SET: [u16; 7] = [
    crate::constants::item_id::KATANA,
    crate::constants::item_id::BAINHA,
    crate::constants::item_id::ARMADURA_MEDIA,
    crate::constants::item_id::BRINCO,
    crate::constants::item_id::AMULETO,
    crate::constants::item_id::BRACELETE,
    crate::constants::item_id::CINTO,
];

/// The sum of the reference set's template MIDPOINTS (`items::item_template`,
/// which the database mirrors): the proportion that the scale turns into the
/// ladder's number. Dexterity counts because it becomes attack on the katana
/// (`dex / 5`).
pub const TEMPLATE_ATTACK: f32 = 20.0;
pub const TEMPLATE_DEX: f32 = 18.0;
pub const TEMPLATE_DEFENSE: f32 = 10.5;
pub const TEMPLATE_HEALTH: f32 = 77.5;
/// How much attack one point of dexterity is worth in the reference set.
pub const ATTACK_PER_DEX: f32 = 0.2;

/// THE LEGACY: `effective_stats` still adds, per equipped piece, the fixed
/// bonus from `constants::item_bonus` - the pre-instance item, which stayed as
/// a floor. It is worth the same at level 1 and at 60, so it goes into the
/// ladder's BASE and not into the items' scale. Read from the table, not
/// copied: if legacy leaves the game, this becomes zero and the ladder closes.
pub fn legacy() -> crate::constants::EquipBonus {
    let mut l = crate::constants::EquipBonus::default();
    for id in REFERENCE_SET {
        let b = crate::constants::item_bonus(id);
        l.hp_max += b.hp_max;
        l.mp_max += b.mp_max;
        l.attack_damage += b.attack_damage;
        l.dex += b.dex;
        l.wis += b.wis;
        l.defense += b.defense;
    }
    l
}

/// What the character brings WITHOUT tier items at the level, on the typical
/// build: the base 20, the legacy, and per level one STR (a third of the
/// points), half of STR and the katana's proficiency (`weapon_scaling`).
pub fn character_attack(level: u32) -> f32 {
    let l = legacy();
    20.0 + l.attack_damage as f32 + l.dex as f32 * ATTACK_PER_DEX + 1.8 * level as f32
}

/// No point of the typical build gives defense: only the legacy.
pub fn character_defense(_nivel: u32) -> f32 {
    legacy().defense as f32
}

/// The base 100, the legacy, two per STR and five per VIT (two thirds).
pub fn character_health(level: u32) -> f32 {
    100.0 + legacy().hp_max as f32 + 12.0 * level as f32
}

/// The slice of the ladder the items have to supply at the level.
pub fn item_attack(level: u32) -> f32 {
    (attack(level) as f32 - character_attack(level)).max(0.0)
}

pub fn item_defense(level: u32) -> f32 {
    (defense(level) as f32 - character_defense(level)).max(0.0)
}

pub fn item_health(level: u32) -> f32 {
    (health(level) as f32 - character_health(level)).max(0.0)
}

/// How much one point of ATTACK template (or dexterity, or wisdom) is worth on
/// a piece at this item level.
pub fn attack_scale(item_level: u16) -> f32 {
    item_attack(item_level as u32) / (TEMPLATE_ATTACK + TEMPLATE_DEX * ATTACK_PER_DEX)
}

pub fn defense_scale(item_level: u16) -> f32 {
    item_defense(item_level as u32) / TEMPLATE_DEFENSE
}

/// Health and mana.
pub fn health_scale(item_level: u16) -> f32 {
    item_health(item_level as u32) / TEMPLATE_HEALTH
}

// ───────────────────────────── the mobs ─────────────────────────────

/// How much of the species' ATTACK personality enters the net: the owlbear has
/// 2.8 times the wolf's damage in the table, but against someone on the ladder
/// it takes only 1 + 1.8 x 0.6 = 2.1 times the net. Without the flattening the
/// heavy level 10 creatures (owlbear, mage) took 30% more than they took in the
/// old game, and the level 10 goal died. HEALTH does not flatten: a big
/// creature still takes a while.
pub const ATTACK_FLATTENING: f32 = 0.6;

/// What a common mob of profile 1.0 TAKES from someone on the ladder, per blow.
/// It is the part of its attack that gets past the expected defense: whoever is
/// one tier behind takes this plus the defense they are missing; whoever
/// refined lands on the floor.
///
/// Calibrated by the level 10 goal (`metas_de_balanceamento`, the zone through
/// the center): the wolf takes from the expected player what it took in the
/// game measured on 19/09 - 1.5% of health - and the owlbear (2.1x after the
/// flattening) about 3%. The ZONE is easy on purpose, at 10 and at 60: what
/// weighs is density (fort, islet), and the piece that ages.
pub fn mob_net_damage(level: u32) -> f32 {
    1.5 + 0.33 * level as f32
}

/// Blows from the expected player to kill a profile 1.0 mob - the WOLF, the
/// weakest in the table. The bear takes 2.3 times that, the owlbear 3.8 and the
/// queen five. Calibrated to reproduce the time per kill the game measured on
/// 19/09 had (3 to 4.5 s at level 10): the wolf in three blows, the owlbear in
/// eleven. At 3.5 the mage (which backs off after every blow) needed one blow
/// more than the katana's charge one tier behind gives, and it ended up chasing
/// the mage without killing it.
pub const STRIKES_PER_MOB: f32 = 3.0;

/// Fraction of the expected attack that a profile 1.0 defense mob holds back.
/// Whoever is on the ladder delivers the rest.
pub const MOB_DEFENSE: f32 = 0.15;

/// The species, RELATIVE TO THE WOLF in the table (`economy::KINDS_INICIAIS`):
/// a bear has 2.3 times the health and 1.8 times the attack; defense becomes a
/// fraction of the player's expected attack (wolf 0, bear 0.08, rocky 0.22).
///
/// The profile comes from the SAME numbers that were always in the database
/// table: the table stops being an absolute value and becomes a proportion.
/// Nobody has to rewrite the bestiary for the ladder to hold.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Profile {
    pub health: f32,
    pub attack: f32,
    /// Fraction of `attack(level)` that the creature's defense holds back.
    pub defense: f32,
}

/// (health, damage, defense) of the Wolf in the table: the profile's unit.
pub const WOLF: (i32, i32, i32) = (120, 10, 0);
/// How much table defense one point of `MOB_DEFENSE` is worth... as a fraction:
/// each point of `def` in the table holds 1% of the expected attack, up to 25%.
/// It was 2% up to 45%: with that the queen held 40% and sword and shield,
/// which already pays for the tank in attack, hit the floor against her.
pub const DEFENSE_PER_TABLE_POINT: f32 = 0.01;
pub const MOB_MAX_DEFENSE: f32 = 0.25;

impl Profile {
    pub const fn novo(health: f32, attack: f32, defense: f32) -> Profile {
        Profile { health, attack, defense }
    }

    /// The profile of a row in the mob table (health, damage, base defense).
    pub fn relativo_ao_lobo(hp: i32, dmg: i32, def: i32) -> Profile {
        Profile {
            health: (hp.max(1) as f32 / WOLF.0 as f32).max(0.05),
            attack: (dmg.max(1) as f32 / WOLF.1 as f32).max(0.05),
            defense: (def.max(0) as f32 * DEFENSE_PER_TABLE_POINT).min(MOB_MAX_DEFENSE),
        }
    }
}

/// A mob's attributes at the level: what the screen shows and the maths uses.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Mob {
    pub health: i32,
    pub attack: i32,
    pub defense: i32,
}

/// In the first levels a freshly created character has no armor - the opening
/// journey (`metas_do_inicio`) is measured with the weapon alone - so the part
/// of the mob's attack that the expected defense would absorb ramps up to here
/// instead of arriving whole. From level 12 on the full ladder applies. (The
/// old game had the same thing in `CURVA_DO_INICIO`.)
pub const EARLY_RAMP: u32 = 12;

/// How much of the expected defense the level's mob "pierces": all of it, after the ramp.
pub fn pierceable(level: u32) -> f32 {
    defense(level) as f32 * (level as f32 / EARLY_RAMP as f32).min(1.0)
}

/// A common mob of this profile born at the level.
///
/// * defense: `MOB_DEFENSE`-ths of the expected attack, by the profile's
///   fraction;
/// * health: `STRIKES_PER_MOB` blows from the expected player against a defense
///   1.0 creature - health does not depend on the species' defense, so a
///   hard-shelled creature takes a while from both sides, and that is what it
///   is;
/// * attack: the expected defense PLUS the profile's net. The part that
///   "pierces" is the same for every species at the level; personality scales
///   only what is left. Without this a 2.8x owlbear would have 2.8x the whole
///   attack and would go through any armor.
pub fn mob(p: &Profile, level: u32) -> Mob {
    let a = attack(level) as f32;
    let golpe_esperado = a * (1.0 - MOB_DEFENSE);
    Mob {
        health: (STRIKES_PER_MOB * golpe_esperado * p.health).round().max(1.0) as i32,
        attack: (pierceable(level)
            + mob_net_damage(level) * (1.0 + (p.attack - 1.0) * ATTACK_FLATTENING))
            .round()
            .max(1.0) as i32,
        defense: (a * p.defense).round().max(0.0) as i32,
    }
}

// ───────────────────────────── the bosses ─────────────────────────────

/// The boss's COMMON blow takes this from someone on the ladder (the
/// telegraphed one is a fraction of health, `bosses::dano_telegrafado`). Three
/// wolves: it hurts, but it is not what kills - what kills is standing still.
pub const BOSS_NET: f32 = 2.5;
/// The boss's defense, as a fraction of the expected attack: more than any
/// common mob. Whoever is one tier behind hits the floor.
pub const BOSS_DEFENSE: f32 = 0.40;

pub fn boss_attack(level: u32) -> i32 {
    (pierceable(level) + mob_net_damage(level) * BOSS_NET).round() as i32
}

/// Blows from the expected player to bring a boss down: sized for a one to four
/// minute fight while dodging (`metas_dos_chefes`), which is what the old
/// health (`8,960 + 269 x level`, cap 20,720) gave against the old player. No
/// cap: the expected attack is a straight line, so the fight does not shorten
/// with level.
pub const BOSS_STRIKES: f32 = 250.0;

pub fn boss_health(level: u32) -> i32 {
    let golpe = damage(attack(level), boss_defense(level)) as f32;
    // The wire sends health as u32 since protocol 172 (it was u16, which
    // capped bosses at level 79).
    // Past 60 the pistol kit's damage outgrows the strike count (the boss
    // sims dropped under the 60 s floor at 65..80), so each level above 60
    // adds 1.2% of health.
    let alem = level.saturating_sub(60) as f32 * 0.012;
    (BOSS_STRIKES * golpe * (1.0 + alem)).round() as i32
}

pub fn boss_defense(level: u32) -> i32 {
    (attack(level) as f32 * BOSS_DEFENSE).round() as i32
}

// ───────────────────────────── the power ─────────────────────────────

/// The power (the sheet's maths, `dungeon::poder_de_stats`) of the expected
/// character at the level - only the three lines, no mana and no dexterity.
pub fn power(level: u32) -> i32 {
    attack(level) * 10 + defense(level) * 8 + health(level)
}

/// The ladder's row at a level, for the sheet and for the doc: (attack, defense, health).
pub fn row(level: u32) -> (i32, i32, i32) {
    (attack(level), defense(level), health(level))
}

#[cfg(test)]
mod testes {
    use super::*;

    /// The doc's table comes from here (`cargo test -p shared ladder -- --nocapture`).
    #[test]
    fn a_escada_e_reta_e_cresce() {
        let mut before = row(0);
        println!("| Nível | Ataque | Defesa | Vida | Mob comum (health/attack) | Poder |");
        for n in [1u32, 5, 10, 15, 20, 25, 30, 35, 40, 45, 50, 55, 60] {
            let l = row(n);
            let m = mob(&Profile::novo(1.0, 1.0, 0.0), n);
            println!("| {n} | {} | {} | {} | {}/{} | {} |", l.0, l.1, l.2, m.health, m.attack, power(n));
            assert!(l.0 > before.0 && l.1 > before.1 && l.2 > before.2, "level {n} did not grow");
            before = l;
        }
        // Straight lines: the difference between two levels does not depend on where you are.
        assert_eq!(attack(40) - attack(30), attack(60) - attack(50));
        assert_eq!(defense(40) - defense(30), defense(60) - defense(50));
    }

    /// The floor comes from `FLOOR`, not from a 10 written by hand: when it dropped
    /// to 0.06 this test failed without anything being wrong.
    #[test]
    fn o_golpe_subtrai_e_tem_piso() {
        let piso_de_100 = (100.0 * FLOOR).round() as i32;
        assert_eq!(damage(100, 30), 70);
        assert_eq!(damage(100, 100), piso_de_100, "defense igual ao attack: o floor");
        assert_eq!(damage(100, 1000), piso_de_100, "defense absurda: o floor, nunca imune");
        assert_eq!(damage(3, 1000), 1, "nunca abaixo de 1");
        assert_eq!(damage(100, 0), 100);
        assert_eq!(damage(100, -5), 100, "negative defense does not amplify");
        // What the floor IS: never zero, and never the whole defense.
        assert!(piso_de_100 > 0 && piso_de_100 < 100);
    }

    #[test]
    fn a_reducao_de_identidade_vem_depois_e_tem_teto() {
        assert_eq!(damage_with_reduction(100, 30, 0.40), 42);
        assert_eq!(damage_with_reduction(100, 30, 0.90), 35, "cap");
        // Two steps, like `damage_with_reduction`: the blow rounds FIRST, and the
        // reduction comes on top of the integer. Computing `100 x FLOOR x 0.5` in one
        // go is off by one when the floor lands on half a point.
        let com_teto = ((damage(100, 100) as f32) * (1.0 - MAX_REDUCTION)).round().max(1.0) as i32;
        assert_eq!(damage_with_reduction(100, 100, 0.50), com_teto, "floor e cap juntos: ainda nao e' zero");
        assert!(com_teto >= 1, "floor mais cap nunca chega a zero");
        assert_eq!(damage_with_reduction(100, 30, 0.0), 70);
    }

    /// What the owner felt, as a number: the same defense takes more and more as
    /// the mob goes up - the equipment ages.
    #[test]
    fn a_mesma_defesa_envelhece() {
        let p = Profile::novo(1.0, 1.0, 0.0);
        let def_do_31 = defense(31);
        let toma = |n: u32| damage(mob(&p, n).attack, def_do_31);
        assert!(toma(31) < toma(41) && toma(41) < toma(51), "{} {} {}", toma(31), toma(41), toma(51));
        // Refined (+48%, the refinement cap) lands on the floor: at most ~4x less
        // than expected, never zero.
        let refinado = (def_do_31 as f32 * 1.48) as i32;
        let m = mob(&p, 31);
        assert_eq!(damage(m.attack, refinado), (m.attack as f32 * FLOOR).round() as i32);
    }

    /// The mobs' goals, on top of the expected player.
    #[test]
    fn o_mob_comum_sai_das_metas() {
        for n in [12u32, 20, 30, 40, 50, 60] {
            let m = mob(&Profile::novo(1.0, 1.0, 0.0), n);
            // The expected one takes exactly the net (after the ramp).
            assert_eq!(damage(m.attack, defense(n)), mob_net_damage(n).round() as i32, "nível {n}");
            // And kills in STRIKES_PER_MOB blows against defense 1.0.
            let def_um = (attack(n) as f32 * MOB_DEFENSE).round() as i32;
            let strikes = (m.health as f32 / damage(attack(n), def_um) as f32).ceil();
            assert!((strikes - STRIKES_PER_MOB).abs() <= 1.0, "nível {n}: {strikes} strikes");
        }
        // The profile comes from the table as it always was.
        let urso = Profile::relativo_ao_lobo(280, 18, 8);
        assert!((urso.health - 2.333).abs() < 0.01 && (urso.attack - 1.8).abs() < 0.01);
        assert!((urso.defense - 0.08).abs() < 0.001);
        let rochoso = Profile::relativo_ao_lobo(560, 30, 22);
        assert!((rochoso.defense - 0.22).abs() < 0.001);
        assert_eq!(Profile::relativo_ao_lobo(9999, 99, 999).defense, MOB_MAX_DEFENSE);
    }

    /// Personality scales only the net: the owlbear (2.8x) does not pierce armor
    /// 2.8 times more.
    #[test]
    fn a_especie_escala_o_liquido_e_nao_o_ataque_inteiro() {
        let n = 30;
        let wolf = mob(&Profile::relativo_ao_lobo(120, 10, 0), n);
        let owl = mob(&Profile::relativo_ao_lobo(460, 28, 4), n);
        let d = defense(n);
        let (tl, to) = (damage(wolf.attack, d), damage(owl.attack, d));
        let expected = 1.0 + 1.8 * ATTACK_FLATTENING;
        assert!((to as f32 / tl as f32 - expected).abs() < 0.2, "{tl} {to}");
        assert!((owl.attack as f32) < wolf.attack as f32 * 2.0, "the whole attack does not double");
    }

    #[test]
    fn a_escala_dos_itens_fecha_no_conjunto_de_referencia() {
        for ilvl in [5u16, 18, 35, 60] {
            let n = ilvl as u32;
            let atk = (TEMPLATE_ATTACK + TEMPLATE_DEX * ATTACK_PER_DEX) * attack_scale(ilvl)
                + character_attack(n);
            let def = TEMPLATE_DEFENSE * defense_scale(ilvl) + character_defense(n);
            let hp = TEMPLATE_HEALTH * health_scale(ilvl) + character_health(n);
            assert!((atk - attack(n) as f32).abs() < 1.0, "ilvl {ilvl}: {atk} vs {}", attack(n));
            assert!((def - defense(n) as f32).abs() < 1.0);
            assert!((hp - health(n) as f32).abs() < 1.0);
        }
    }

    const CHAR_LEVEL_CAP_TESTE: u32 = crate::constants::CHAR_LEVEL_CAP;

    #[test]
    fn o_chefe_bate_mais_e_segura_mais_que_o_comum() {
        for n in [12u32, 30, 60] {
            let c = boss_attack(n);
            let comum = mob(&Profile::novo(1.0, 1.0, 0.0), n);
            assert!(c > comum.attack);
            assert_eq!(damage(c, defense(n)), (mob_net_damage(n) * BOSS_NET).round() as i32);
            assert!(boss_defense(n) > comum.defense);
            assert!(boss_health(n) > 20 * comum.health);
        }
        // Health is u32 on the wire since protocol 172: a level-80 boss
        // (Skyreach) has its real health, not the old u16 ceiling.
        assert!(boss_health(80) > u16::MAX as i32, "level 80 passes the old u16 cap");
        assert!(boss_health(CHAR_LEVEL_CAP_TESTE) < i32::MAX / 4, "and never overflows");
    }

    /// In the first levels the mob pierces less: the freshly created character has no armor.
    #[test]
    fn a_rampa_do_inicio() {
        let p = Profile::novo(1.0, 1.0, 0.0);
        let sem_armadura = 0;
        let toma = |n: u32| damage(mob(&p, n).attack, sem_armadura);
        assert!(toma(1) < 10, "wolf do nível 1 contra quem só tem a arma: {}", toma(1));
        assert!(toma(3) < toma(6) && toma(6) < toma(12));
        assert_eq!(pierceable(EARLY_RAMP), defense(EARLY_RAMP) as f32);
        assert_eq!(pierceable(60), defense(60) as f32);
    }
}
