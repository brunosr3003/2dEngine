//! As receitas de equipamento (docs/ECONOMIA_DE_CRAFT.md).
//!
//! Cada craft pede a CHAVE (1), tres materiais na COR do item que se quer, e
//! darksteel e cobre pelo nivel. Arma e secundaria gastam a mesma linha de
//! baixo (so' a chave muda), as tres armaduras a mesma entre si, e os quatro
//! acessorios idem — poucos nomes girando muito.
//!
//! As quantidades NAO sao as do MIR4: a estrutura vale copiar, os valores nao
//! (ver `docs/ITENS.md`, "O numero que decide se copiamos os valores"). Aqui
//! elas cabem numa sessao de play test na faixa de cada ilha.
//!
//! O GRAU que sai e' a cor do material, e cada cor tem nivel minimo — as
//! mesmas faixas da chave (`shared::chaves`): verde 20, azul 40, Epico so' a
//! partir do 60 (docs/DUNGEONS_E_RAIDS.md). O servidor semeia estas
//! receitas no banco (`craft_recipes`, ids 1000+) sem apagar ajuste manual.

use crate::constants::item_id;
use crate::forja::Grau;
use crate::protocol::CraftRecipeNet;

/// Primeiro id das receitas de equipamento. Abaixo disto: barcos e legado.
pub const PRIMEIRO_ID: u16 = 1000;

/// `CraftRecipeNet::category`.
pub mod categoria {
    pub const OUTRO: u8 = 0;
    pub const ARMA: u8 = 1;
    pub const ARMADURA: u8 = 2;
    pub const MATERIAL: u8 = 3;
    /// O BARCO (docs/MAR_ABERTO.md). Reusa o 4, que era exatamente o valor
    /// da aba de barco apagada em 20/09/2026 — o buraco estava la' esperando.
    pub const BARCO: u8 = 4;
    pub const SECUNDARIA: u8 = 5;
    pub const ACESSORIO: u8 = 6;
}

pub fn nome_da_categoria(c: u8) -> &'static str {
    match c {
        categoria::ARMA => "Arma",
        categoria::ARMADURA => "Armadura",
        categoria::MATERIAL => "Material",
        categoria::BARCO => "Barco",
        categoria::SECUNDARIA => "Secundária",
        categoria::ACESSORIO => "Acessório",
        _ => "Outros",
    }
}

/// Uma faixa de receita: a cor do material pedido, o grau que sai, o nivel
/// minimo pra criar e as quantidades.
#[derive(Debug, Clone, Copy)]
pub struct Faixa {
    /// 1 cinza .. 4 roxo (`item_id::na_cor`).
    pub cor: u8,
    pub grau: Grau,
    pub nivel_min: u16,
    /// Nível da instância criada. `items::tier_from_ilvl` dele dá o grau.
    pub item_level: u16,
    pub principal: u32,
    pub secundario: u32,
    pub darksteel: u32,
    pub cobre: u32,
}

pub const FAIXAS: [Faixa; 4] = [
    Faixa {
        cor: 1,
        grau: Grau::Comum,
        nivel_min: 1,
        item_level: 5,
        principal: 30,
        secundario: 10,
        darksteel: 200,
        cobre: 300,
    },
    Faixa {
        cor: 2,
        grau: Grau::Fino,
        nivel_min: 20,
        item_level: 18,
        principal: 90,
        secundario: 30,
        darksteel: 1_500,
        cobre: 2_000,
    },
    Faixa {
        cor: 3,
        grau: Grau::Raro,
        nivel_min: 40,
        item_level: 35,
        principal: 300,
        secundario: 100,
        darksteel: 8_000,
        cobre: 10_000,
    },
    Faixa {
        cor: 4,
        grau: Grau::Epico,
        nivel_min: 60,
        item_level: 60,
        principal: 300,
        secundario: 100,
        darksteel: 60_000,
        cobre: 50_000,
    },
];

/// (peca, nome, categoria, chave, principal, secundario 1, secundario 2)
type Peca = (u16, &'static str, u8, u16, u16, u16, u16);

pub const PECAS: [Peca; 15] = {
    use categoria::*;
    use item_id::*;
    [
        (
            ESPADA_E_ESCUDO,
            "Espada e Escudo",
            ARMA,
            SCALE,
            STEEL,
            DARK_HEART_STONE,
            MOON_SHADOW_STONE,
        ),
        (
            KATANA,
            "Katana",
            ARMA,
            SCALE,
            STEEL,
            DARK_HEART_STONE,
            MOON_SHADOW_STONE,
        ),
        (
            PISTOLAS,
            "Duas Pistolas",
            ARMA,
            SCALE,
            STEEL,
            DARK_HEART_STONE,
            MOON_SHADOW_STONE,
        ),
        (
            ANEL_MAGICO,
            "Anel Mágico",
            ARMA,
            SCALE,
            STEEL,
            DARK_HEART_STONE,
            MOON_SHADOW_STONE,
        ),
        (
            MANTO_DO_GUERREIRO,
            "Manto do Guerreiro",
            SECUNDARIA,
            CLAW,
            STEEL,
            DARK_HEART_STONE,
            MOON_SHADOW_STONE,
        ),
        (
            BAINHA,
            "Bainha",
            SECUNDARIA,
            CLAW,
            STEEL,
            DARK_HEART_STONE,
            MOON_SHADOW_STONE,
        ),
        (
            COLDRE,
            "Coldre",
            SECUNDARIA,
            CLAW,
            STEEL,
            DARK_HEART_STONE,
            MOON_SHADOW_STONE,
        ),
        (
            MANTO_DO_MAGO,
            "Manto do Mago",
            SECUNDARIA,
            CLAW,
            STEEL,
            DARK_HEART_STONE,
            MOON_SHADOW_STONE,
        ),
        (
            ARMADURA_LEVE,
            "Armadura Leve",
            ARMADURA,
            HIDE,
            STEEL,
            QUINTESSENCE,
            EXORCISM_BAUBLE,
        ),
        (
            ARMADURA_MEDIA,
            "Armadura Média",
            ARMADURA,
            HIDE,
            STEEL,
            QUINTESSENCE,
            EXORCISM_BAUBLE,
        ),
        (
            ARMADURA_PESADA,
            "Armadura Pesada",
            ARMADURA,
            HIDE,
            STEEL,
            QUINTESSENCE,
            EXORCISM_BAUBLE,
        ),
        (
            BRINCO,
            "Brinco",
            ACESSORIO,
            HORN,
            PLATINUM,
            ILLUMINATING_FRAGMENT,
            ANIMA_STONE,
        ),
        (
            AMULETO,
            "Amuleto",
            ACESSORIO,
            HORN,
            PLATINUM,
            ILLUMINATING_FRAGMENT,
            ANIMA_STONE,
        ),
        (
            BRACELETE,
            "Bracelete",
            ACESSORIO,
            HORN,
            PLATINUM,
            ILLUMINATING_FRAGMENT,
            ANIMA_STONE,
        ),
        (
            CINTO,
            "Cinto",
            ACESSORIO,
            HORN,
            PLATINUM,
            ILLUMINATING_FRAGMENT,
            ANIMA_STONE,
        ),
    ]
};

/// As 60 receitas: 15 pecas × 4 cores. Id = `PRIMEIRO_ID + faixa*100 + peca`.
pub fn receitas_de_equipamento() -> Vec<CraftRecipeNet> {
    let mut v = Vec::with_capacity(FAIXAS.len() * PECAS.len());
    for (fi, f) in FAIXAS.iter().enumerate() {
        for (pi, &(peca, nome, cat, chave, principal, s1, s2)) in PECAS.iter().enumerate() {
            let cor = |base: u16| item_id::na_cor(base, f.cor) as u32;
            v.push(CraftRecipeNet {
                id: PRIMEIRO_ID + fi as u16 * 100 + pi as u16,
                name: format!("{nome} · {}", f.grau.nome()),
                category: cat,
                station: 0,
                tier: fi as u8 + 1,
                inputs: vec![
                    [cor(chave), 1],
                    [cor(principal), f.principal],
                    [cor(s1), f.secundario],
                    [cor(s2), f.secundario],
                    [item_id::DARKSTEEL as u32, f.darksteel],
                    [item_id::COPPER as u32, f.cobre],
                ],
                output_item_id: peca,
                output_qty: 1,
                output_item_level: f.item_level,
                roll_instance: true,
                nivel_min: f.nivel_min,
            });
        }
    }
    v
}

/// Id da receita do Selo da Tempestade.
pub const RECEITA_SELO: u16 = 1900;

/// Selo da Tempestade: entrada do estagio 5 de conteudo 60+ ⚠️. Marcas,
/// darksteel e po — o ralo de material do topo. Teto de 2 por semana POR
/// CONTA, conferido no servidor (`shared::dungeon::SELOS_POR_SEMANA`).
pub fn receita_do_selo() -> CraftRecipeNet {
    CraftRecipeNet {
        id: RECEITA_SELO,
        name: "Selo da Tempestade".into(),
        category: categoria::MATERIAL,
        station: 0,
        tier: 4,
        inputs: vec![
            [item_id::MARCAS_TEMPESTADE as u32, 120],
            [item_id::DARKSTEEL as u32, 5_000],
            [item_id::GLITTERING_POWDER as u32, 3],
        ],
        output_item_id: item_id::SELO_TEMPESTADE,
        output_qty: 1,
        output_item_level: 0,
        roll_instance: false,
        nivel_min: 60,
    }
}

/// As tres receitas de CASCO (docs/MAR_ABERTO.md).
///
/// **A Chalupa nao custa darksteel nem chave, de proposito.** Ela e' a porta
/// de saida da primeira ilha: toda outra receita do jogo comeca com uma chave
/// que so' cai de chefe, e isso e' certo pra equipamento e catastrofico pra
/// coisa que da' acesso a VIAJAR. Sessenta madeiras sao umas quinze arvores;
/// vinte acos e quinhentos cobres saem de uma encosta.
///
/// E ela ressuscita `WOOD_T1..T3`, que hoje caem da coleta e quase nada
/// consome — a floresta passa a alimentar o mar.
pub fn receitas_de_barco() -> Vec<CraftRecipeNet> {
    let uma = |id: u16, item: u16, nome: &str, tier: u8, inputs: Vec<[u32; 2]>| CraftRecipeNet {
        id,
        name: nome.into(),
        category: categoria::BARCO,
        station: 0,
        tier,
        inputs,
        output_item_id: item,
        output_qty: 1,
        output_item_level: 0,
        // O casco nao tem rolagem: `BarcoData` nasce no `add_to_inventory`.
        roll_instance: false,
        nivel_min: crate::barcos::nivel_de_craft(item) as u16,
    };
    vec![
        uma(
            1930,
            item_id::BARCO_BASE,
            "Chalupa",
            1,
            // Sem couro: `LEATHER_T1` E' o `HIDE`, uma das quatro chaves de
            // chefe. Foi o teste abaixo que pegou — a Chalupa pedia um item
            // que so' cai de boss, e a historia teria travado num passo que o
            // jogador nao tem como cumprir.
            vec![
                [item_id::WOOD_T1 as u32, 80],
                [item_id::na_cor(item_id::STEEL, 1) as u32, 20],
                [item_id::COPPER as u32, 500],
            ],
        ),
        uma(
            1931,
            item_id::BARCO_ESCUNA,
            "Escuna",
            2,
            vec![
                [item_id::WOOD_T2 as u32, 220],
                [item_id::na_cor(item_id::STEEL, 2) as u32, 80],
                [item_id::DARKSTEEL as u32, 1_200],
                [item_id::COPPER as u32, 6_000],
            ],
        ),
        uma(
            1932,
            item_id::BARCO_NAU,
            "Nau",
            3,
            vec![
                [item_id::WOOD_T3 as u32, 600],
                [item_id::na_cor(item_id::STEEL, 3) as u32, 240],
                [item_id::DARKSTEEL as u32, 9_000],
                [item_id::COPPER as u32, 30_000],
            ],
        ),
    ]
}

#[cfg(test)]
mod testes {
    use super::*;
    use crate::items::tier_from_ilvl;

    /// A CHALUPA tem que caber no bolso de quem acabou de chegar no nivel 5.
    ///
    /// E' a porta de saida da primeira ilha: se ela pedir chave de chefe ou
    /// darksteel, a historia trava num item que o jogador ainda nao tem como
    /// conseguir — e ele nao descobre isso ate' chegar no passo.
    #[test]
    fn a_chalupa_nao_pede_chave_nem_darksteel() {
        let r = receitas_de_barco()
            .into_iter()
            .find(|r| r.output_item_id == item_id::BARCO_BASE)
            .expect("a Chalupa existe");
        assert_eq!(r.nivel_min, 5);
        for [id, _] in &r.inputs {
            let id = *id as u16;
            assert_ne!(id, item_id::DARKSTEEL, "a Chalupa pediu darksteel");
            assert!(
                !item_id::CHAVES
                    .iter()
                    .any(|c| (1..=5).any(|cor| item_id::chave_na_cor(*c, cor) == id)),
                "a Chalupa pediu uma chave de chefe"
            );
        }
    }

    #[test]
    fn sessenta_receitas_com_ids_unicos_e_seis_ingredientes() {
        let r = receitas_de_equipamento();
        assert_eq!(r.len(), 60);
        let mut ids: Vec<u16> = r.iter().map(|x| x.id).collect();
        ids.sort();
        ids.dedup();
        assert_eq!(ids.len(), 60);
        assert!(r.iter().all(|x| x.inputs.len() == 6 && x.id >= PRIMEIRO_ID));
    }

    /// O grau que sai e' o da cor pedida, e Epico nao se cria antes do 60.
    #[test]
    fn a_cor_decide_o_grau_e_epico_so_no_sessenta() {
        for r in receitas_de_equipamento() {
            let f = FAIXAS[(r.tier - 1) as usize];
            assert_eq!(
                tier_from_ilvl(r.output_item_level),
                f.grau as u8,
                "{}",
                r.name
            );
            if f.grau >= Grau::Epico {
                assert!(
                    r.nivel_min >= 60,
                    "{} sai Epico no nivel {}",
                    r.name,
                    r.nivel_min
                );
            }
            // Todo material colorido na cor da faixa.
            for [id, _] in &r.inputs[..4] {
                let id = *id as u16;
                let base = PECAS
                    .iter()
                    .flat_map(|p| [p.3, p.4, p.5, p.6])
                    .find(|b| (*b..*b + 4).contains(&id))
                    .unwrap();
                assert_eq!(
                    id - base + 1,
                    f.cor as u16,
                    "{}: material fora da cor",
                    r.name
                );
            }
        }
        // Nivel 20: nada acima de Fino.
        assert!(receitas_de_equipamento()
            .iter()
            .filter(|r| r.nivel_min <= 20)
            .all(|r| tier_from_ilvl(r.output_item_level) <= 2));
    }
}
