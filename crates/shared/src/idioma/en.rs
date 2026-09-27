//! O dicionário português → inglês, partido por área de origem.
//!
//! A chave é a frase EXATA que está no código. Mudou a frase em português?
//! muda a chave aqui também, senão o verbete deixa de casar e a tela volta pro
//! português — sem quebrar nada, mas sem traduzir.
//!
//! Regras que o teste em `crates/shared/tests/idioma.rs` cobra:
//!
//! - o buraco do português tem que ter par no inglês, e vice-versa;
//! - buraco NOMEADO (`{n}`) pode mudar de lugar na tradução; buraco anônimo
//!   (`{}`, `{:.0}`) segue a ordem, então tradução que reordena é obrigada a
//!   nomear;
//! - chave repetida é erro (a segunda nunca seria usada).
//!
//! Partido em cinco porque um arquivo de três mil pares não se revisa: cada
//! parte acompanha os arquivos de onde o texto saiu, e dá pra ler lado a lado
//! com o código. Frase que serve os dois idiomas igual (nome próprio, "OK",
//! "XP", "PvP") não entra: sem verbete, o texto sai como está.

pub mod cliente;
pub mod dados;
pub mod historia;
pub mod missoes;
pub mod servidor;

/// As partes, na ordem de revisão. O dicionário lê todas como se fossem uma.
///
/// Fatias em vez de um `const` só porque `const` não concatena: juntar no
/// construtor sai de graça (uma vez por processo) e mantém os arquivos
/// separados.
pub const PARTES: &[&[(&str, &str)]] = &[
    cliente::VERBETES,
    servidor::VERBETES,
    dados::VERBETES,
    missoes::VERBETES,
    historia::VERBETES,
];
