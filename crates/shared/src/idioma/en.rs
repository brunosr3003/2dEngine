//! The Portuguese -> English dictionary, split by area of origin.
//!
//! The key is the EXACT phrase that is in the code. Changed the Portuguese
//! phrase? change the key here too, or the entry stops matching and the screen
//! falls back to Portuguese — breaking nothing, but translating nothing.
//!
//! Rules the test in `crates/shared/tests/idioma.rs` enforces:
//!
//! - a Portuguese hole must have a pair in the English, and vice versa;
//! - a NAMED hole (`{n}`) may move in the translation; an anonymous hole
//! (`{}`, `{:.0}`) follows the order, so a translation that reorders is
//! obliged to name them;
//! - a repeated key is an error (the second would never be used).
//!
//! Split into five because a file of three thousand pairs cannot be reviewed:
//! each part follows the files the text came from, and can be read side by
//! side with the code. A phrase that serves both languages the same (a proper
//! noun, "OK", "XP", "PvP") does not go in: with no entry, the text comes out as is.

pub mod cliente;
pub mod dados;
pub mod historia;
pub mod missoes;
pub mod servidor;

/// The parts, in review order. The dictionary reads them all as if they were one.
///
/// Slices instead of a single `const` only because `const` does not
/// concatenate: joining them in the constructor is free (once per process)
/// and keeps the files separate.
pub const PARTES: &[&[(&str, &str)]] = &[
    cliente::VERBETES,
    servidor::VERBETES,
    dados::VERBETES,
    missoes::VERBETES,
    historia::VERBETES,
];
