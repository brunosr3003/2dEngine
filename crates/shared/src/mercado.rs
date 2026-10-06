//! Mercado global (docs/MERCADO.md): as regras puras e os tipos de rede.
//!
//! Um mercado so' pra todos os realms. O servidor do realm valida e tira o
//! item (ou o gold) do personagem; o banco CENTRAL guarda em custodia e devolve
//! por CARTA — entrega com id unico, aplicada uma vez so' no banco do realm.
//! Nada aqui fala com banco: e' a conta que os dois lados (e os testes) fazem
//! igual.
use std::collections::HashSet;

use serde::{Deserialize, Serialize};

use crate::items::ItemInstance;

/// Taxa do mercado, em %: sai do que o VENDEDOR recebe e e' queimada.
pub const TAXA_PCT: u64 = 5;
/// Nivel minimo pra ANUNCIAR (comprar e' livre). Contra conta descartavel.
pub const NIVEL_PARA_VENDER: u32 = 20;
/// Teto do preco por unidade, em gold.
pub const PRECO_MAX_UNIT: u64 = 1_000_000_000;
/// Anuncios ativos por personagem (itens e TP juntos).
pub const MAX_ANUNCIOS: usize = 20;
/// Linhas por pagina na busca.
pub const POR_PAGINA: usize = 20;
/// Teto de TP num anuncio so'.
pub const TP_MAX_POR_ANUNCIO: u64 = 1_000_000;
/// Energy is sold in lots of this much (one Energy is worth a fraction of a
/// gold, and the market's price floor is 1).
pub const ENERGIA_POR_LOTE: u64 = 1_000;
/// Most lots in one listing.
pub const LOTES_MAX_POR_ANUNCIO: u64 = 100_000;
/// Texto de busca: o que passa disto e' cortado.
pub const BUSCA_MAX_CHARS: usize = 32;

/// Anuncio de item (gold por item) ou de TP (gold por TP).
pub const TIPO_ITEM: u8 = 0;
pub const TIPO_TP: u8 = 1;

pub const ESTADO_ATIVO: u8 = 0;
pub const ESTADO_ESGOTADO: u8 = 1;
pub const ESTADO_CANCELADO: u8 = 2;

#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Categoria {
    Todas = 0,
    Equipamento = 1,
    Material = 2,
    Consumivel = 3,
    Tp = 4,
    /// Pets, their skill books, food and accessories.
    Pet = 5,
    /// Mounts and their accessories.
    Montaria = 6,
}

impl Categoria {
    /// As abas de filtro do Comprar (a TP tem aba propria).
    pub const FILTROS: [Categoria; 6] = [
        Categoria::Todas,
        Categoria::Equipamento,
        Categoria::Pet,
        Categoria::Montaria,
        Categoria::Material,
        Categoria::Consumivel,
    ];

    pub fn de_u8(v: u8) -> Categoria {
        match v {
            1 => Categoria::Equipamento,
            2 => Categoria::Material,
            3 => Categoria::Consumivel,
            4 => Categoria::Tp,
            5 => Categoria::Pet,
            6 => Categoria::Montaria,
            _ => Categoria::Todas,
        }
    }

    pub fn nome(self) -> &'static str {
        match self {
            Categoria::Todas => "All",
            Categoria::Equipamento => "Gear",
            Categoria::Material => "Material",
            Categoria::Consumivel => "Consumable",
            Categoria::Tp => "TP",
            Categoria::Pet => "Pets",
            Categoria::Montaria => "Mounts",
        }
    }
}

/// Em que prateleira um item cai.
pub fn categoria_do_item(item_id: u16, equipavel: bool) -> Categoria {
    use crate::constants::item_id as it;
    use crate::constants::EquipSlot;
    // Pets and mounts are equipable too: they get their own shelves first.
    match crate::constants::equip_slot_of(item_id) {
        Some(EquipSlot::Pet | EquipSlot::AcessorioPet) => return Categoria::Pet,
        Some(EquipSlot::Montaria | EquipSlot::AcessorioMontaria) => return Categoria::Montaria,
        _ => {}
    }
    if it::e_skill_de_pet(item_id) || item_id == it::RACAO_DE_PET {
        return Categoria::Pet;
    }
    if item_id == it::ENERGIA_MIL {
        return Categoria::Consumivel;
    }
    if equipavel {
        Categoria::Equipamento
    } else if crate::pocoes::cura_de(item_id).is_some()
        || matches!(
            item_id,
            it::HEALTH_POTION
                | it::MANA_POTION
                | it::GREATER_HEAL
                | it::GREATER_MANA
                | it::STAMINA_POTION
                | it::XP_POTION
                | it::FORTUNA_POTION
                | it::SORTE_POTION
                | it::PERGAMINHO_TELEPORTE
        )
    {
        Categoria::Consumivel
    } else {
        Categoria::Material
    }
}

/// Taxa de uma venda de `bruto` gold. Arredonda PRA CIMA: toda venda queima
/// alguma coisa, ate' a de 1 gold.
pub fn taxa(bruto: u64) -> u64 {
    bruto.saturating_mul(TAXA_PCT).div_ceil(100)
}

/// O que o vendedor recebe.
pub fn liquido(bruto: u64) -> u64 {
    bruto - taxa(bruto)
}

/// `qtd × preco`, sem estourar.
pub fn total(qtd: u64, preco_unit: u64) -> Option<u64> {
    qtd.checked_mul(preco_unit)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Recusa {
    NivelBaixo,
    Vinculado,
    Quantidade,
    Preco,
    MuitosAnuncios,
    SemGold,
    SemTp,
    Indisponivel,
    Esgotado,
    PrecoMudou,
    ProprioAnuncio,
}

impl Recusa {
    pub fn texto(&self) -> String {
        match self {
            Recusa::NivelBaixo => {
                format!("Precisa do nível {NIVEL_PARA_VENDER} para vender no mercado.")
            }
            Recusa::Vinculado => "Bound item: it cannot be sold.".into(),
            Recusa::Quantidade => "Invalid quantity.".into(),
            Recusa::Preco => format!("Preço inválido (1 a {PRECO_MAX_UNIT} por unidade)."),
            Recusa::MuitosAnuncios => format!("Limite de {MAX_ANUNCIOS} anúncios ativos."),
            Recusa::SemGold => "Not enough gold.".into(),
            Recusa::SemTp => "Not enough TP.".into(),
            Recusa::Indisponivel => {
                "Mercado indisponível agora. Tente de novo em instantes.".into()
            }
            Recusa::Esgotado => {
                "O anúncio acabou ou não tem essa quantidade; o gold volta em Entregas.".into()
            }
            Recusa::PrecoMudou => "O preço do anúncio mudou; o gold volta em Entregas.".into(),
            Recusa::ProprioAnuncio => {
                "Não dá para comprar o próprio anúncio; o gold volta em Entregas.".into()
            }
        }
    }
}

/// Preco por unidade aceito?
pub fn preco_valido(preco_unit: u64) -> bool {
    (1..=PRECO_MAX_UNIT).contains(&preco_unit)
}

/// Anunciar `qtd` de um slot que tem `no_slot`. O limite de anuncios ativos
/// e' conferido de novo no central (e' la' que a contagem e' verdadeira).
pub fn pode_anunciar(
    nivel: u32,
    vinculado: bool,
    no_slot: u32,
    qtd: u32,
    preco_unit: u64,
) -> Result<u64, Recusa> {
    if nivel < NIVEL_PARA_VENDER {
        return Err(Recusa::NivelBaixo);
    }
    if vinculado {
        return Err(Recusa::Vinculado);
    }
    if qtd == 0 || qtd > no_slot {
        return Err(Recusa::Quantidade);
    }
    if !preco_valido(preco_unit) {
        return Err(Recusa::Preco);
    }
    total(qtd as u64, preco_unit).ok_or(Recusa::Preco)
}

/// Anunciar TP: mesmo portao de nivel.
pub fn pode_anunciar_tp(nivel: u32, qtd: u64, preco_unit: u64) -> Result<u64, Recusa> {
    if nivel < NIVEL_PARA_VENDER {
        return Err(Recusa::NivelBaixo);
    }
    if qtd == 0 || qtd > TP_MAX_POR_ANUNCIO {
        return Err(Recusa::Quantidade);
    }
    if !preco_valido(preco_unit) {
        return Err(Recusa::Preco);
    }
    total(qtd, preco_unit).ok_or(Recusa::Preco)
}

/// Selling Energy: the level gate, enough Energy for the lots, a valid price.
pub fn pode_anunciar_energia(nivel: u32, energia: u64, lotes: u64, preco_unit: u64) -> Result<u64, Recusa> {
    if nivel < NIVEL_PARA_VENDER {
        return Err(Recusa::NivelBaixo);
    }
    if lotes == 0 || lotes > LOTES_MAX_POR_ANUNCIO || lotes.saturating_mul(ENERGIA_POR_LOTE) > energia {
        return Err(Recusa::Quantidade);
    }
    if !preco_valido(preco_unit) {
        return Err(Recusa::Preco);
    }
    total(lotes, preco_unit).ok_or(Recusa::Preco)
}

/// Comprar: devolve o gold que sai agora do comprador.
pub fn pode_comprar(gold: u64, qtd: u64, preco_unit: u64) -> Result<u64, Recusa> {
    if qtd == 0 {
        return Err(Recusa::Quantidade);
    }
    if !preco_valido(preco_unit) {
        return Err(Recusa::Preco);
    }
    let t = total(qtd, preco_unit).ok_or(Recusa::Preco)?;
    if t > gold {
        return Err(Recusa::SemGold);
    }
    Ok(t)
}

/// O central fechando uma compra. Nada de "compra o que sobrou": ou a
/// quantidade pedida inteira, ou nada (e o gold volta por carta).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Fechamento {
    pub vendida: u64,
    pub sobra: u64,
    pub bruto: u64,
    pub taxa: u64,
    pub liquido: u64,
}

pub fn fechar_compra(
    restante: u64,
    estado: u8,
    preco_anuncio: u64,
    pedida: u64,
    preco_pedido: u64,
) -> Result<Fechamento, Recusa> {
    if estado != ESTADO_ATIVO || pedida == 0 || pedida > restante {
        return Err(Recusa::Esgotado);
    }
    if preco_anuncio != preco_pedido {
        return Err(Recusa::PrecoMudou);
    }
    let bruto = total(pedida, preco_anuncio).ok_or(Recusa::Preco)?;
    let taxa = taxa(bruto);
    Ok(Fechamento {
        vendida: pedida,
        sobra: restante - pedida,
        bruto,
        taxa,
        liquido: bruto - taxa,
    })
}

/// Quais cartas ainda faltam aplicar: tira as ja' aplicadas e as repetidas
/// na propria lista. E' o que faz "aplicar duas vezes" valer uma.
pub fn cartas_a_aplicar<'a>(
    cartas: &'a [CartaNet],
    ja_aplicadas: &HashSet<String>,
) -> Vec<&'a CartaNet> {
    let mut vistas = HashSet::new();
    cartas
        .iter()
        .filter(|c| !ja_aplicadas.contains(&c.id) && vistas.insert(c.id.as_str()))
        .collect()
}

// ─────────────────────────────── rede ───────────────────────────────

/// Um anuncio como o cliente ve.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnuncioNet {
    pub id: String,
    pub tipo: u8,
    /// 0 no anuncio de TP.
    pub item_id: u16,
    pub nome: String,
    pub categoria: u8,
    pub instancia: Option<ItemInstance>,
    /// Quanto ainda esta' a' venda.
    pub qtd: u64,
    pub preco_unit: u64,
    pub realm: String,
    pub vendedor: String,
    /// Anuncio deste personagem.
    pub meu: bool,
}

/// Entrega esperando ser recebida: item, gold ou os dois.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CartaNet {
    pub id: String,
    /// 0 = so' gold.
    pub item_id: u16,
    pub qtd: u64,
    pub instancia: Option<ItemInstance>,
    pub gold: u64,
    pub motivo: String,
}

/// Uma linha do historico.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VendaNet {
    pub nome: String,
    pub qtd: u64,
    pub preco_unit: u64,
    pub taxa: u64,
    pub liquido: u64,
    /// Unix secs.
    pub quando: i64,
    /// true = eu vendi; false = eu comprei.
    pub vendi: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct FiltroNet {
    pub categoria: u8,
    pub texto: String,
    pub pagina: u16,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn carta(id: &str) -> CartaNet {
        CartaNet {
            id: id.into(),
            item_id: 5,
            qtd: 1,
            instancia: None,
            gold: 0,
            motivo: String::new(),
        }
    }

    #[test]
    fn taxa_de_cinco_por_cento_arredonda_pra_cima() {
        assert_eq!(taxa(100), 5);
        assert_eq!(taxa(1), 1, "toda venda queima alguma coisa");
        assert_eq!(taxa(19), 1);
        assert_eq!(taxa(21), 2);
        assert_eq!(taxa(0), 0);
        assert_eq!(liquido(1000), 950);
        assert_eq!(liquido(21), 19);
        assert_eq!(
            taxa(u64::MAX / 10),
            (u64::MAX / 10).saturating_mul(5).div_ceil(100)
        );
    }

    #[test]
    fn pets_e_montarias_tem_prateleira_propria() {
        use crate::constants::item_id as it;
        let pet = it::pet_no_grau(it::PETS[0], 2);
        let montaria = it::montaria_no_grau(it::MONTARIAS[0], 3);
        assert_eq!(categoria_do_item(pet, true), Categoria::Pet);
        assert_eq!(categoria_do_item(it::SKILL_PET_FARO, false), Categoria::Pet);
        assert_eq!(categoria_do_item(it::RACAO_DE_PET, false), Categoria::Pet);
        assert_eq!(categoria_do_item(520, true), Categoria::Pet, "pet accessory");
        assert_eq!(categoria_do_item(montaria, true), Categoria::Montaria);
        assert_eq!(categoria_do_item(528, true), Categoria::Montaria, "mount accessory");
        assert_eq!(categoria_do_item(it::KATANA, true), Categoria::Equipamento);
        assert_eq!(categoria_do_item(it::STEEL, false), Categoria::Material);
        for c in Categoria::FILTROS {
            assert_eq!(Categoria::de_u8(c as u8), c, "the shelf survives the wire");
        }
    }

    #[test]
    fn portao_de_nivel_e_vinculado() {
        assert_eq!(
            pode_anunciar(19, false, 10, 1, 100),
            Err(Recusa::NivelBaixo)
        );
        assert_eq!(pode_anunciar(20, true, 10, 1, 100), Err(Recusa::Vinculado));
        assert_eq!(pode_anunciar(20, false, 10, 3, 100), Ok(300));
        assert_eq!(pode_anunciar_tp(10, 5, 100), Err(Recusa::NivelBaixo));
    }

    #[test]
    fn quantidade_e_preco_fora_da_faixa() {
        assert_eq!(pode_anunciar(30, false, 2, 3, 100), Err(Recusa::Quantidade));
        assert_eq!(pode_anunciar(30, false, 2, 0, 100), Err(Recusa::Quantidade));
        assert_eq!(pode_anunciar(30, false, 2, 1, 0), Err(Recusa::Preco));
        assert_eq!(
            pode_anunciar(30, false, 2, 1, PRECO_MAX_UNIT + 1),
            Err(Recusa::Preco)
        );
        assert_eq!(
            pode_comprar(u64::MAX, u64::MAX, PRECO_MAX_UNIT),
            Err(Recusa::Preco),
            "estouro recusa"
        );
        assert_eq!(pode_comprar(299, 3, 100), Err(Recusa::SemGold));
        assert_eq!(pode_comprar(300, 3, 100), Ok(300));
    }

    #[test]
    fn compra_parcial_divide_certo() {
        let f = fechar_compra(10, ESTADO_ATIVO, 7, 4, 7).unwrap();
        assert_eq!(
            f,
            Fechamento {
                vendida: 4,
                sobra: 6,
                bruto: 28,
                taxa: 2,
                liquido: 26
            }
        );
        let f2 = fechar_compra(f.sobra, ESTADO_ATIVO, 7, 6, 7).unwrap();
        assert_eq!(f2.sobra, 0);
        assert_eq!(
            f.vendida + f2.vendida,
            10,
            "nada some nem duplica entre as duas"
        );
    }

    #[test]
    fn compra_recusada_nao_fecha_nada() {
        assert_eq!(
            fechar_compra(3, ESTADO_ATIVO, 7, 4, 7),
            Err(Recusa::Esgotado)
        );
        assert_eq!(
            fechar_compra(3, ESTADO_CANCELADO, 7, 1, 7),
            Err(Recusa::Esgotado)
        );
        assert_eq!(
            fechar_compra(3, ESTADO_ATIVO, 8, 1, 7),
            Err(Recusa::PrecoMudou)
        );
    }

    #[test]
    fn carta_aplicada_duas_vezes_vale_uma() {
        let cartas = vec![carta("a"), carta("b"), carta("a")];
        let mut aplicadas = HashSet::new();
        let primeira: Vec<String> = cartas_a_aplicar(&cartas, &aplicadas)
            .iter()
            .map(|c| c.id.clone())
            .collect();
        assert_eq!(primeira, vec!["a", "b"]);
        aplicadas.extend(primeira);
        assert!(cartas_a_aplicar(&cartas, &aplicadas).is_empty());
    }

    #[test]
    fn categorias() {
        use crate::constants::item_id as it;
        assert_eq!(categoria_do_item(it::KATANA, true), Categoria::Equipamento);
        assert_eq!(
            categoria_do_item(it::HEALTH_POTION, false),
            Categoria::Consumivel
        );
        assert_eq!(categoria_do_item(it::WOOD_T1, false), Categoria::Material);
        assert_eq!(Categoria::de_u8(9), Categoria::Todas);
    }
}
