//! The English -> Portuguese dictionary, split by area of origin.
//!
//! The key is the EXACT phrase that is in the code, and the code is English
//! now. Changed the English phrase? change the key here too, or the entry
//! stops matching and the screen falls back to English — breaking nothing,
//! but translating nothing.
//!
//! Rules the test in `crates/shared/tests/idioma.rs` enforces:
//!
//! - an English hole must have a pair in the Portuguese, and vice versa;
//! - a NAMED hole (`{n}`) may move in the translation; an anonymous hole
//! (`{}`, `{:.0}`) follows the order, so a translation that reorders is
//! obliged to name them;
//! - a repeated key is an error (the second would never be used).
//!
//! Split into five because a file of three thousand pairs cannot be reviewed:
//! each part follows the files the text came from, and can be read side by
//! side with the code. A phrase that serves both languages the same (a proper
//! noun, "OK", "XP", "PvP") does not go in: with no entry, the text comes out
//! as is.
//!
//! Ten entries were LOST when the dictionary was inverted, and the loss is in
//! the language, not in the code: English carries no gender and no adjective
//! plural, so Roxa/Roxo, Épica/Épico, Lendária/Lendário, Todas/Todos,
//! Concluída/Concluídas, Disponível/Disponíveis and seu/sua each collapse
//! onto one English word and cannot be told apart from it. The grade words
//! are recovered by `idioma::tr_f`, which lets a call site beside a feminine
//! noun ask for the feminine form; the rest are labels where one form reads
//! fine. See docs/TRANSLATION.md.

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
