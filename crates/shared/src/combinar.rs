//! A aba "Combinar" tem duas formas de subir a cor:
//!
//! Chaves, pets e montarias continuam na aposta:
//! - **cinco** entram;
//! - **10%** de chance;
//! - a saida e' **sorteada** entre os itens daquela FAMILIA na cor de cima —
//!   cinco Escamas Azuis podem virar qualquer chave roxa, nao a Escama Roxa.
//!   E' o que faz combinar ser uma aposta de verdade e nao uma conversao com
//!   passo extra;
//! - falhar consome os cinco.
//!
//! Material comum (Aco, Quintessencia e os outros seis) usa SINTESE:
//! dez da mesma cor viram um da proxima, com sucesso garantido e os custos
//! de cobre, darksteel e Po. Assim o material roxo tem fonte no jogo.
//!
//! Tudo aqui e' dado e conta pura: o cliente desenha a mesma tabela que o
//! servidor cobra. O id de uma receita e' o item de ENTRADA.

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

/// Quantos entram por tentativa — cinco, pra tudo.
pub const POR_TENTATIVA: u32 = 5;
/// Nome velho, pros chamadores que ainda falam em chave.
pub const CHAVES_POR_TENTATIVA: u32 = POR_TENTATIVA;
/// A chance de subir, em %: dez, em todo degrau e pra toda familia.
pub const CHANCE: u8 = 10;
pub const fn chance_da_chave(_cor: u8) -> u8 {
    CHANCE
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

/// A tabela inteira: chaves, materiais comuns, montaria e pet.
pub fn receitas() -> Vec<ReceitaDeCombinar> {
    let mut v = Vec::new();
    // CHAVES: quatro familias, quatro degraus (cinza→lendaria).
    for &base in &item_id::CHAVES {
        for cor in 1..=4u8 {
            v.push(ReceitaDeCombinar {
                entrada: item_id::chave_na_cor(base, cor),
                qtd: POR_TENTATIVA,
                saida: item_id::chave_na_cor(base, cor + 1),
                chance: CHANCE,
                cobre: 0,
                darksteel: 0,
                po: 0,
                cor,
                chave: true,
            });
        }
    }
    // MATERIAIS: dez iguais viram um da proxima cor, sem roleta.
    // O po' e os metais sao gastos por tentativa e aparecem no cliente.
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
    // MONTARIAS e PETS: mesma regra. A `saida` aqui e' so' o representante da
    // familia — quem sorteia de verdade e' `sorteia_saida`, no servidor.
    for &base in &item_id::MONTARIAS {
        for grau in 1..crate::montarias::GRAU_MAX {
            v.push(ReceitaDeCombinar {
                entrada: item_id::montaria_no_grau(base, grau),
                qtd: POR_TENTATIVA,
                saida: item_id::montaria_no_grau(base, grau + 1),
                chance: CHANCE,
                cobre: 0,
                darksteel: 0,
                po: 0,
                cor: grau,
                chave: false,
            });
        }
    }
    for &base in &item_id::PETS {
        for grau in 1..crate::pets::GRAU_MAX {
            v.push(ReceitaDeCombinar {
                entrada: item_id::pet_no_grau(base, grau),
                qtd: POR_TENTATIVA,
                saida: item_id::pet_no_grau(base, grau + 1),
                chance: CHANCE,
                cobre: 0,
                darksteel: 0,
                po: 0,
                cor: grau,
                chave: false,
            });
        }
    }
    v
}

/// Os candidatos de SAIDA de uma receita: os itens da mesma familia na cor de
/// cima.
///
/// O dono pediu "algo aleatorio da proxima cor naquele tipo de recurso": cinco
/// Escamas Azuis podem virar qualquer chave roxa, e nao a Escama Roxa. Sem
/// isso combinar e' uma conversao com um passo a mais; com isso e' aposta.
///
/// Devolve a lista pro SERVIDOR sortear (o cliente desenha "uma chave roxa",
/// sem prometer qual) — e nunca vazia: o primeiro e' a `saida` da receita.
pub fn saidas_possiveis(r: &ReceitaDeCombinar) -> Vec<u16> {
    let proxima = r.cor + 1;
    if r.chave {
        return item_id::CHAVES
            .iter()
            .map(|&b| item_id::chave_na_cor(b, proxima))
            .collect();
    }
    if item_id::montaria_de_id(r.entrada).is_some() {
        return item_id::MONTARIAS
            .iter()
            .map(|&b| item_id::montaria_no_grau(b, proxima))
            .collect();
    }
    if item_id::pet_de_id(r.entrada).is_some() {
        return item_id::PETS
            .iter()
            .map(|&b| item_id::pet_no_grau(b, proxima))
            .collect();
    }
    vec![r.saida]
}

pub fn receita(entrada: u16) -> Option<ReceitaDeCombinar> {
    receitas().into_iter().find(|r| r.entrada == entrada)
}

/// Tentativas de uma vez no maximo (o botao "Combinar tudo").
pub const MAX_VEZES: u16 = 50;

/// Quantas tentativas o que se tem paga: pelo item e por cada custo.
/// `tem` = quanto a bolsa tem de um item.
pub fn vezes_possiveis(r: &ReceitaDeCombinar, tem: &dyn Fn(u16) -> u32) -> u32 {
    let por = |id: u16, custo: u32| {
        if custo == 0 {
            u32::MAX
        } else {
            tem(id) / custo
        }
    };
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

    /// Material comum usa sintese garantida de dez para um.
    #[test]
    fn material_comum_sobe_por_sintese() {
        for base in MATERIAIS {
            for cor in 1..=3 {
                let r = receita(item_id::na_cor(base, cor)).unwrap();
                assert_eq!(r.saida, item_id::na_cor(base, cor + 1));
                assert_eq!((r.qtd, r.chance), (10, 100));
                assert_eq!((r.cobre, r.darksteel, r.po), custo_da_sintese(cor));
            }
        }
    }

    #[test]
    fn cada_item_sobe_por_um_caminho_so_e_nunca_vira_ele_mesmo() {
        let v = receitas();
        // 4 chaves x 4 degraus; UMA ladder de montaria e UMA de pet, de 4
        // degraus cada (a cor E' a criatura desde 20/09/2026).
        assert_eq!(v.len(), 4 * 4 + 8 * 3 + 4 + 4);
        for (i, a) in v.iter().enumerate() {
            assert_ne!(a.entrada, a.saida);
            assert!(v[i + 1..].iter().all(|b| b.entrada != a.entrada));
        }
    }

    /// Pet, montaria e chave sobem pela MESMA regra: cinco, 10%, e o que sai
    /// e' sorteado na familia.
    ///
    /// Eram tres regras diferentes na mesma aba — chave 5 a 10%, pet e
    /// montaria 3 com chance decrescente e cobre. Uma so' e' o que o jogador
    /// consegue guardar na cabeca.
    #[test]
    fn chaves_pets_e_montarias_seguem_a_mesma_regra() {
        for r in receitas().into_iter().filter(|r| r.chance < 100) {
            assert_eq!(r.qtd, POR_TENTATIVA, "{} pede {} ", r.entrada, r.qtd);
            assert_eq!(r.chance, CHANCE, "{}", r.entrada);
            assert_eq!(
                (r.cobre, r.darksteel, r.po),
                (0, 0, 0),
                "{} cobra material alem dos cinco",
                r.entrada
            );
            // A saida sorteada e' da cor de cima, e a receita esta' entre elas.
            let saidas = saidas_possiveis(&r);
            assert!(!saidas.is_empty());
            assert!(saidas.contains(&r.saida), "{}", r.entrada);
        }
        // O laranja e' o teto: nao ha' receita saindo dele.
        assert!(receita(item_id::pet_no_grau(item_id::PET_BASE, 5)).is_none());
    }

    /// Na aposta de chaves, so' a pilha limita as tentativas.
    #[test]
    fn vezes_possiveis_e_so_a_pilha() {
        let chifre = receita(item_id::HORN).unwrap();
        let so_chifre = |id: u16| if id == item_id::HORN { 12 } else { 0 };
        assert_eq!(vezes_possiveis(&chifre, &so_chifre), 2, "12 chifres = 2x5");
        assert_eq!(vezes_possiveis(&chifre, &|_| 4), 0, "4 nao pagam uma");
        // Cobre no bolso nao muda nada para a chave; sintese cobra separadamente.
        let com_cobre = |id: u16| match id {
            item_id::HORN => 12,
            item_id::COPPER => 999_999,
            _ => 0,
        };
        assert_eq!(vezes_possiveis(&chifre, &com_cobre), 2);
    }
}
