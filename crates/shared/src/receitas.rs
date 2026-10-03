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
//! mesmas faixas da chave (`shared::chaves`), e agora LIDAS de la' por
//! `nivel_da_cor` em vez de copiadas a mao. Eram copiadas, e em 28/09/2026 a
//! ladder das chaves desceu (azul 40→30, epica 60→40) sem que esta tabela
//! soubesse: a chave azul ja' caia no 30 e a receita da katana Rara continuava
//! pedindo 40. Duas respostas pra mesma pergunta divergem; e' so' questao de
//! quando. O servidor semeia estas receitas no banco (`craft_recipes`, ids
//! 1000+) sem apagar ajuste manual.

use crate::constants::item_id;
use crate::forja::Grau;
use crate::protocol::CraftRecipeNet;

/// Primeiro id das receitas de equipamento. Abaixo disto: barcos e legado.
pub const PRIMEIRO_ID: u16 = 1000;
/// One past the last equipment recipe id: every colour (`FAIXAS`) has its
/// hundred. Salvage looks recipes up in `PRIMEIRO_ID..FIM_DOS_IDS`; it was a
/// hard-coded `PRIMEIRO_ID + 400`, which left out the fifth colour — no
/// legendary piece could be salvaged.
pub const FIM_DOS_IDS: u16 = PRIMEIRO_ID + FAIXAS.len() as u16 * 100;

/// `CraftRecipeNet::category`.
pub mod categoria {
    pub const OUTRO: u8 = 0;
    pub const ARMA: u8 = 1;
    pub const ARMADURA: u8 = 2;
    pub const MATERIAL: u8 = 3;
    pub const SECUNDARIA: u8 = 5;
    pub const ACESSORIO: u8 = 6;
}

pub fn nome_da_categoria(c: u8) -> &'static str {
    match c {
        categoria::ARMA => "Weapon",
        categoria::ARMADURA => "Armour",
        categoria::MATERIAL => "Material",
        categoria::SECUNDARIA => "Off-hand",
        categoria::ACESSORIO => "Accessory",
        _ => "Other",
    }
}

/// Uma faixa de receita: a cor do material pedido, o grau que sai, o nivel
/// minimo pra criar e as quantidades.
#[derive(Debug, Clone, Copy)]
pub struct Faixa {
    /// 1 cinza .. 5 lendaria (`item_id::chave_na_cor`).
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

impl Faixa {
    /// A cor dos tres MATERIAIS. Material colorido so' existe em quatro cores
    /// (`item_id::na_cor`): o id seguinte ao roxo ja' e' o proximo material.
    /// A chave, sim, tem a quinta (`item_id::chave_na_cor`, ids 353-356).
    ///
    /// Entao o que separa a receita Lendaria da Epica e' a CHAVE lendaria, que
    /// so' cai de conteudo 50+ e a 1% — mais o dobro de material roxo. Inventar
    /// uma cor 5 de Aco e de Platina seria inventar a cadeia de drop inteira.
    pub const fn cor_material(&self) -> u8 {
        if self.cor > 4 {
            4
        } else {
            self.cor
        }
    }
}

/// O nivel minimo pra criar na cor `cor`: a MESMA faixa da chave daquela cor.
///
/// `const fn` de proposito — assim `FAIXAS` continua `const` e nao ha' como
/// alguem "so' ajustar aqui" e deixar as duas tabelas discordando de novo.
pub const fn nivel_da_cor(cor: u8) -> u16 {
    let mut i = 0;
    while i < crate::chaves::FAIXAS.len() {
        if crate::chaves::FAIXAS[i].cor == cor {
            return crate::chaves::FAIXAS[i].nivel_min as u16;
        }
        i += 1;
    }
    1
}

pub const FAIXAS: [Faixa; 5] = [
    Faixa {
        cor: 1,
        grau: Grau::Comum,
        nivel_min: nivel_da_cor(1),
        item_level: 5,
        principal: 30,
        secundario: 10,
        darksteel: 200,
        cobre: 300,
    },
    Faixa {
        cor: 2,
        grau: Grau::Fino,
        nivel_min: nivel_da_cor(2),
        item_level: 18,
        principal: 90,
        secundario: 30,
        darksteel: 1_500,
        cobre: 2_000,
    },
    Faixa {
        cor: 3,
        grau: Grau::Raro,
        nivel_min: nivel_da_cor(3),
        item_level: 35,
        principal: 300,
        secundario: 100,
        darksteel: 8_000,
        cobre: 10_000,
    },
    Faixa {
        cor: 4,
        grau: Grau::Epico,
        nivel_min: nivel_da_cor(4),
        item_level: 60,
        principal: 300,
        secundario: 100,
        darksteel: 60_000,
        cobre: 50_000,
    },
    Faixa {
        // A LENDARIA, aberta em 28/09/2026. A cor existia no jogo inteiro —
        // grau, chave (ids 353-356), multiplicador de atributo, cor na bolsa —
        // e so' saia de bau e do Aprimorar: nao havia receita nenhuma de tier
        // 5, e o catalogo parava no Epico. Agora sao 75 receitas.
        //
        // `item_level` 80 de proposito: e' o mesmo que `forja` ja' da' pra
        // peca que sobe de cor pelo Aprimorar, entao criar e aprimorar
        // entregam a MESMA peca, e nao duas lendarias de forca diferente.
        cor: 5,
        grau: Grau::Lendario,
        nivel_min: nivel_da_cor(5),
        item_level: 80,
        principal: 600,
        secundario: 200,
        darksteel: 150_000,
        cobre: 120_000,
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
            "Sword and Shield",
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
            "Twin Pistols",
            ARMA,
            SCALE,
            STEEL,
            DARK_HEART_STONE,
            MOON_SHADOW_STONE,
        ),
        (
            ANEL_MAGICO,
            "Magic Ring",
            ARMA,
            SCALE,
            STEEL,
            DARK_HEART_STONE,
            MOON_SHADOW_STONE,
        ),
        (
            MANTO_DO_GUERREIRO,
            "Warrior's Mantle",
            SECUNDARIA,
            CLAW,
            STEEL,
            DARK_HEART_STONE,
            MOON_SHADOW_STONE,
        ),
        (
            BAINHA,
            "Scabbard",
            SECUNDARIA,
            CLAW,
            STEEL,
            DARK_HEART_STONE,
            MOON_SHADOW_STONE,
        ),
        (
            COLDRE,
            "Holster",
            SECUNDARIA,
            CLAW,
            STEEL,
            DARK_HEART_STONE,
            MOON_SHADOW_STONE,
        ),
        (
            MANTO_DO_MAGO,
            "Mage's Mantle",
            SECUNDARIA,
            CLAW,
            STEEL,
            DARK_HEART_STONE,
            MOON_SHADOW_STONE,
        ),
        (
            ARMADURA_LEVE,
            "Light Armour",
            ARMADURA,
            HIDE,
            STEEL,
            QUINTESSENCE,
            EXORCISM_BAUBLE,
        ),
        (
            ARMADURA_MEDIA,
            "Medium Armour",
            ARMADURA,
            HIDE,
            STEEL,
            QUINTESSENCE,
            EXORCISM_BAUBLE,
        ),
        (
            ARMADURA_PESADA,
            "Heavy Armour",
            ARMADURA,
            HIDE,
            STEEL,
            QUINTESSENCE,
            EXORCISM_BAUBLE,
        ),
        (
            BRINCO,
            "Earring",
            ACESSORIO,
            HORN,
            PLATINUM,
            ILLUMINATING_FRAGMENT,
            ANIMA_STONE,
        ),
        (
            AMULETO,
            "Amulet",
            ACESSORIO,
            HORN,
            PLATINUM,
            ILLUMINATING_FRAGMENT,
            ANIMA_STONE,
        ),
        (
            BRACELETE,
            "Bracelet",
            ACESSORIO,
            HORN,
            PLATINUM,
            ILLUMINATING_FRAGMENT,
            ANIMA_STONE,
        ),
        (
            CINTO,
            "Belt",
            ACESSORIO,
            HORN,
            PLATINUM,
            ILLUMINATING_FRAGMENT,
            ANIMA_STONE,
        ),
    ]
};

/// As 75 receitas: 15 pecas × 5 cores. Id = `PRIMEIRO_ID + faixa*100 + peca`.
pub fn receitas_de_equipamento() -> Vec<CraftRecipeNet> {
    let mut v = Vec::with_capacity(FAIXAS.len() * PECAS.len());
    for (fi, f) in FAIXAS.iter().enumerate() {
        for (pi, &(peca, nome, cat, chave, principal, s1, s2)) in PECAS.iter().enumerate() {
            // A chave vai na cor da faixa (tem cinco); o material, na cor de
            // material (tem quatro). Ate' a roxa as duas dao o mesmo id.
            let chave_da_faixa = item_id::chave_na_cor(chave, f.cor) as u32;
            let cor = |base: u16| item_id::na_cor(base, f.cor_material()) as u32;
            v.push(CraftRecipeNet {
                id: PRIMEIRO_ID + fi as u16 * 100 + pi as u16,
                name: format!("{nome} · {}", f.grau.nome()),
                category: cat,
                station: 0,
                tier: fi as u8 + 1,
                inputs: vec![
                    [chave_da_faixa, 1],
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
        name: "Storm Seal".into(),
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

/// A primeira receita de chave de Porão. `RECEITA_SELO` é 1900, e o
/// equipamento vai de 1000 a 1804 (`PRIMEIRO_ID + faixa*100 + peça`), então
/// 1910 em diante está livre e longe dos dois.
pub const RECEITA_CHAVE_BASE: u16 = 1910;

/// Uma receita por Porão: madeira e aço, nas quantidades que `porao` calcula.
///
/// Sai do catálogo de dungeons e não de uma lista à mão, pelo mesmo motivo que
/// o item da chave sai: Porão novo é receita nova sozinha, e um Porão sem
/// receita seria uma porta que ninguém consegue abrir nunca.
///
/// O id acompanha o ID DO CONTEÚDO, e não a ordem da lista — a mesma armadilha
/// que a chave já levou (`porao::chave_de`).
pub fn receitas_de_chave_de_porao() -> Vec<CraftRecipeNet> {
    crate::dungeon::CONTEUDOS
        .iter()
        .filter(|c| c.tipo == crate::dungeon::Tipo::Porao)
        .filter_map(|c| {
            let chave = crate::porao::chave_de(c)?;
            let r = crate::porao::receita_de(c)?;
            Some(CraftRecipeNet {
                id: RECEITA_CHAVE_BASE + c.id,
                name: format!("{} Key", c.nome),
                category: categoria::MATERIAL,
                station: 0,
                tier: crate::porao::cor_do_nivel(c.nivel_min),
                inputs: vec![
                    [r.madeira as u32, r.madeira_qtd],
                    [r.material as u32, r.material_qtd],
                ],
                output_item_id: chave,
                output_qty: 1,
                output_item_level: 0,
                // A chave não tem atributo pra rolar: ou abre a porta ou não.
                roll_instance: false,
                // O nível da dungeon, e não o da faixa: fabricar a chave de um
                // Porão que ainda não se pode entrar seria material no lixo.
                nivel_min: c.nivel_min as u16,
            })
        })
        .collect()
}

#[cfg(test)]
mod testes {
    use super::*;
    use crate::items::tier_from_ilvl;



    #[test]
    fn uma_receita_por_peca_e_por_cor_com_ids_unicos_e_seis_ingredientes() {
        let r = receitas_de_equipamento();
        let esperado = FAIXAS.len() * PECAS.len();
        assert_eq!(r.len(), esperado);
        let mut ids: Vec<u16> = r.iter().map(|x| x.id).collect();
        ids.sort();
        ids.dedup();
        assert_eq!(ids.len(), esperado);
        assert!(r.iter().all(|x| x.inputs.len() == 6 && x.id >= PRIMEIRO_ID));
        // Uma faixa por cor, sem buraco: a lendaria entrou em 28/09/2026 e o
        // catalogo tem que cobrir 1..=5, senao existe uma cor que o jogo
        // pinta, dropa e aprimora e ninguem consegue CRIAR.
        let mut cores: Vec<u8> = FAIXAS.iter().map(|f| f.cor).collect();
        cores.sort();
        assert_eq!(cores, vec![1, 2, 3, 4, 5]);
    }

    /// O grau que sai e' o da cor pedida, e Epico nao se cria antes do 60.
    #[test]
    fn a_cor_decide_o_grau_e_o_nivel_sai_da_faixa_da_chave() {
        // A ladder, explicita: 1 cinza · 20 verde · 30 azul · 40 epica ·
        // 50 lendaria. Se `chaves::FAIXAS` andar, isto anda junto — e se
        // alguem desencostar as duas tabelas, reprova aqui e nao no jogo do
        // dono.
        for (cor, esperado) in [(1u8, 1u16), (2, 20), (3, 30), (4, 40), (5, 50)] {
            assert_eq!(nivel_da_cor(cor), esperado, "cor {cor}");
            assert_eq!(
                nivel_da_cor(cor) as u32,
                crate::chaves::FAIXAS
                    .iter()
                    .find(|f| f.cor == cor)
                    .unwrap()
                    .nivel_min
            );
        }
        for r in receitas_de_equipamento() {
            let f = FAIXAS[(r.tier - 1) as usize];
            assert_eq!(
                tier_from_ilvl(r.output_item_level),
                f.grau as u8,
                "{}",
                r.name
            );
            // O nivel de cada cor sai da faixa da CHAVE, nao de um 60 escrito
            // aqui: quando a ladder desceu, este assert reprovava por estar
            // desatualizado e nao por a receita estar errada.
            assert_eq!(
                r.nivel_min,
                nivel_da_cor(f.cor),
                "{}: nivel fora da faixa da chave",
                r.name
            );
            // A chave na cor da FAIXA (cinco cores), pela peca que ela abre.
            let chave_da_peca = PECAS[(r.id - PRIMEIRO_ID) as usize % 100].3;
            assert_eq!(
                r.inputs[0],
                [item_id::chave_na_cor(chave_da_peca, f.cor) as u32, 1],
                "{}: chave fora da cor da faixa",
                r.name
            );
            // E os tres materiais na cor de MATERIAL (quatro cores).
            for [id, _] in &r.inputs[1..4] {
                let id = *id as u16;
                let base = PECAS
                    .iter()
                    .flat_map(|p| [p.4, p.5, p.6])
                    .find(|b| (*b..*b + 4).contains(&id))
                    .unwrap();
                assert_eq!(
                    id - base + 1,
                    f.cor_material() as u16,
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

    /// TODO PORÃO TEM RECEITA, COM ID QUE NÃO BATE EM NADA.
    ///
    /// Uma porta sem receita é uma dungeon que ninguém abre — o item da chave
    /// existiria no catálogo e não haveria como fabricá-lo.
    #[test]
    fn toda_chave_de_porao_tem_receita_e_o_id_nao_colide() {
        let chaves = receitas_de_chave_de_porao();
        let poroes: Vec<_> = crate::dungeon::CONTEUDOS
            .iter()
            .filter(|c| c.tipo == crate::dungeon::Tipo::Porao)
            .collect();
        assert_eq!(chaves.len(), poroes.len(), "sobrou Porão sem receita");

        let mut todos: Vec<u16> = receitas_de_equipamento().iter().map(|r| r.id).collect();
        todos.push(receita_do_selo().id);
        todos.extend(chaves.iter().map(|r| r.id));
        let antes = todos.len();
        todos.sort();
        todos.dedup();
        assert_eq!(antes, todos.len(), "duas receitas com o mesmo id");

        for (r, c) in chaves.iter().zip(&poroes) {
            assert_eq!(r.output_item_id, crate::porao::chave_de(c).unwrap());
            assert_eq!(r.output_qty, 1);
            assert_eq!(r.inputs.len(), 2, "{}: madeira e aço, e mais nada", r.name);
            assert!(
                r.inputs.iter().all(|[_, q]| *q > 0),
                "{}: ingrediente de quantidade zero",
                r.name
            );
            // Quem não pode ENTRAR não deve conseguir FABRICAR.
            assert_eq!(r.nivel_min as u32, c.nivel_min, "{}", r.name);
        }
    }

    /// Every equipment recipe — legendary ones included — is inside the
    /// salvage window.
    #[test]
    fn toda_receita_de_equipamento_se_desmantela() {
        for r in receitas_de_equipamento() {
            assert!(r.id >= PRIMEIRO_ID && r.id < FIM_DOS_IDS, "{} ({}) out of salvage", r.name, r.id);
        }
        assert!(receitas_de_equipamento().iter().any(|r| r.tier == 5), "no legendary recipe");
    }
}
