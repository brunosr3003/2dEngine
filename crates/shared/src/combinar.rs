//! A aba "Combinar" do Craft: juntar material de uma cor pra TENTAR a cor de
//! cima.
//!
//! Duas familias, duas regras:
//!
//! - **Chaves** (Escama, Garra, Chifre, Couro — os itens UNICOS de cada
//!   receita): 5 da mesma cor tentam 1 da cor seguinte com 10% de chance, em
//!   todo degrau, ate' roxa → lendaria. A chave e' o regulador do craft (so'
//!   cai de chefe), entao subir de cor por aqui e' aposta, nao conta fechada.
//!   Falhar consome as 5.
//! - **Pets** (docs/PETS.md): 3 do mesmo grau tentam 1 do grau de cima,
//!   cobrando cobre. Aposta como a chave — falhar consome os tres — e a
//!   chance cai a cada degrau. O pet e' item de bolsa justamente pra caber
//!   aqui e no mercado.
//! - **Materiais** (os oito coloridos de 100/300): a sintese de
//!   `docs/ECONOMIA_DE_CRAFT.md` — 10 da cor viram 1 da seguinte, sempre, mas
//!   cobrando cobre, darksteel e Po Cintilante. E' a unica fonte de material
//!   roxo.
//!
//! Tudo aqui e' dado e conta pura: o cliente desenha a mesma tabela que o
//! servidor cobra. O id de uma receita e' o item de ENTRADA — cada item sobe
//! por um caminho so'.

use crate::constants::item_id;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReceitaDeCombinar {
    /// O item que entra (e o id da receita).
    pub entrada: u16,
    /// Quantos dele cada tentativa consome.
    pub qtd: u32,
    /// O que sai no sucesso (1 por tentativa).
    pub saida: u16,
    /// Chance de sucesso, em %.
    pub chance: u8,
    pub cobre: u32,
    pub darksteel: u32,
    pub po: u32,
    /// Cor da entrada (1 cinza .. 4 roxo).
    pub cor: u8,
    pub chave: bool,
}

/// Chaves consumidas por tentativa.
pub const CHAVES_POR_TENTATIVA: u32 = 5;
/// Chance de subir a chave da cor `cor` (1 cinza .. 4 roxa → lendaria): 10%
/// em todo degrau (decisao de 19/09/2026 — era 10/7/5/3).
pub const fn chance_da_chave(_cor: u8) -> u8 {
    10
}

/// Material consumido por sintese.
pub const MATERIAL_POR_SINTESE: u32 = 10;
/// (cobre, darksteel, po cintilante) da sintese a partir da cor `cor`. Os dois
/// primeiros degraus sao os do MIR4; o azul → roxo e' extrapolado (doc).
pub const fn custo_da_sintese(cor: u8) -> (u32, u32, u32) {
    match cor {
        1 => (2_000, 1_000, 2),
        2 => (20_000, 5_000, 25),
        _ => (200_000, 25_000, 300),
    }
}

/// Os oito materiais coloridos que nao sao chave.
pub const MATERIAIS: [u16; 8] = [
    item_id::STEEL,
    item_id::DARK_HEART_STONE,
    item_id::MOON_SHADOW_STONE,
    item_id::QUINTESSENCE,
    item_id::EXORCISM_BAUBLE,
    item_id::PLATINUM,
    item_id::ILLUMINATING_FRAGMENT,
    item_id::ANIMA_STONE,
];

/// A tabela inteira: as chaves primeiro (4 familias x 4 degraus), depois os
/// materiais (8 x 3 degraus — nao ha' material lendario).
pub fn receitas() -> Vec<ReceitaDeCombinar> {
    let mut v = Vec::new();
    for &base in &item_id::CHAVES {
        for cor in 1..=4u8 {
            v.push(ReceitaDeCombinar {
                entrada: item_id::chave_na_cor(base, cor),
                qtd: CHAVES_POR_TENTATIVA,
                saida: item_id::chave_na_cor(base, cor + 1),
                chance: chance_da_chave(cor),
                cobre: 0,
                darksteel: 0,
                po: 0,
                cor,
                chave: true,
            });
        }
    }
    for &base in &item_id::PETS {
        for grau in 1..crate::pets::GRAU_MAX {
            v.push(ReceitaDeCombinar {
                entrada: item_id::pet_no_grau(base, grau),
                qtd: crate::pets::PETS_POR_TENTATIVA,
                saida: item_id::pet_no_grau(base, grau + 1),
                chance: crate::pets::chance_de_combinar(grau + 1),
                cobre: crate::pets::cobre_de_combinar(grau),
                darksteel: 0,
                po: 0,
                cor: grau,
                chave: false,
            });
        }
    }
    for &base in &MATERIAIS {
        for cor in 1..=3u8 {
            let (cobre, darksteel, po) = custo_da_sintese(cor);
            v.push(ReceitaDeCombinar {
                entrada: item_id::na_cor(base, cor),
                qtd: MATERIAL_POR_SINTESE,
                saida: item_id::na_cor(base, cor + 1),
                chance: 100,
                cobre,
                darksteel,
                po,
                cor,
                chave: false,
            });
        }
    }
    v
}

pub fn receita(entrada: u16) -> Option<ReceitaDeCombinar> {
    receitas().into_iter().find(|r| r.entrada == entrada)
}

/// Tentativas de uma vez no maximo (o botao "Combinar tudo").
pub const MAX_VEZES: u16 = 50;

/// Quantas tentativas o que se tem paga: pelo item e por cada custo.
/// `tem` = quanto a bolsa tem de um item.
pub fn vezes_possiveis(r: &ReceitaDeCombinar, tem: &dyn Fn(u16) -> u32) -> u32 {
    let por = |id: u16, custo: u32| if custo == 0 { u32::MAX } else { tem(id) / custo };
    por(r.entrada, r.qtd)
        .min(por(item_id::COPPER, r.cobre))
        .min(por(item_id::DARKSTEEL, r.darksteel))
        .min(por(item_id::GLITTERING_POWDER, r.po))
        .min(MAX_VEZES as u32)
}

/// Uma tentativa deu certo? `sorte` 0..=99, sorteada por quem chama.
pub fn deu_certo(r: &ReceitaDeCombinar, sorte: u8) -> bool {
    sorte < r.chance
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn cinco_chifres_cinza_tentam_um_verde_a_dez_por_cento() {
        let r = receita(item_id::HORN).expect("chifre cinza combina");
        assert_eq!(r.qtd, 5);
        assert_eq!(r.saida, item_id::na_cor(item_id::HORN, 2));
        assert_eq!(r.chance, 10);
        assert!(deu_certo(&r, 9));
        assert!(!deu_certo(&r, 10));
    }

    #[test]
    fn toda_chave_sobe_de_cor_a_dez_por_cento_ate_a_lendaria() {
        for &base in &item_id::CHAVES {
            for cor in 1..=4u8 {
                let r = receita(item_id::chave_na_cor(base, cor)).unwrap();
                assert_eq!(r.saida, item_id::chave_na_cor(base, cor + 1));
                assert_eq!((r.qtd, r.chance), (5, 10), "cor {cor} de {base}");
            }
        }
        // Garra cinza x5 → garra verde, o exemplo do pedido.
        let garra = receita(item_id::CLAW).unwrap();
        assert_eq!(garra.saida, item_id::na_cor(item_id::CLAW, 2));
    }

    #[test]
    fn material_segue_a_sintese_do_documento() {
        let r = receita(item_id::STEEL).unwrap();
        assert_eq!((r.qtd, r.chance), (10, 100));
        assert_eq!((r.cobre, r.darksteel, r.po), (2_000, 1_000, 2));
        assert_eq!(r.saida, item_id::na_cor(item_id::STEEL, 2));
        // Roxo nao sobe: nao existe material lendario.
        assert!(receita(item_id::na_cor(item_id::STEEL, 4)).is_none());
    }

    #[test]
    fn cada_item_sobe_por_um_caminho_so_e_nunca_vira_ele_mesmo() {
        let v = receitas();
        // 4 chaves x 4 degraus, 5 pets x 4 degraus, 8 materiais x 3.
        assert_eq!(v.len(), 4 * 4 + 5 * 4 + 8 * 3);
        for (i, a) in v.iter().enumerate() {
            assert_ne!(a.entrada, a.saida);
            assert!(v[i + 1..].iter().all(|b| b.entrada != a.entrada));
        }
    }

    /// O pet sobe de grau pelo MESMO caminho da chave: aposta, 3 por
    /// tentativa, e o id guarda o grau.
    #[test]
    fn tres_pets_do_mesmo_grau_tentam_um_do_grau_de_cima() {
        let r = receita(item_id::PET_LOBO).expect("lobinho cinza combina");
        assert_eq!(r.qtd, crate::pets::PETS_POR_TENTATIVA);
        assert_eq!(r.saida, item_id::pet_no_grau(item_id::PET_LOBO, 2));
        assert_eq!(r.chance, crate::pets::chance_de_combinar(2));
        assert!(r.cobre > 0 && r.darksteel == 0 && r.po == 0);
        // O laranja e' o teto: nao ha' receita saindo dele.
        assert!(receita(item_id::pet_no_grau(item_id::PET_LOBO, 5)).is_none());
        // Toda especie tem os quatro degraus.
        for &base in &item_id::PETS {
            for grau in 1..crate::pets::GRAU_MAX {
                let r = receita(item_id::pet_no_grau(base, grau)).expect("degrau existe");
                assert_eq!(r.saida, item_id::pet_no_grau(base, grau + 1));
            }
        }
    }

    #[test]
    fn vezes_possiveis_para_no_que_falta_primeiro() {
        let r = receita(item_id::STEEL).unwrap();
        let bolsa = |id: u16| match id {
            item_id::STEEL => 100,
            item_id::COPPER => 5_000,
            item_id::DARKSTEEL => 9_000,
            item_id::GLITTERING_POWDER => 20,
            _ => 0,
        };
        // 100 aco = 10, cobre 5.000 = 2: o cobre manda.
        assert_eq!(vezes_possiveis(&r, &bolsa), 2);
        let chifre = receita(item_id::HORN).unwrap();
        let so_chifre = |id: u16| if id == item_id::HORN { 12 } else { 0 };
        assert_eq!(vezes_possiveis(&chifre, &so_chifre), 2);
    }
}
