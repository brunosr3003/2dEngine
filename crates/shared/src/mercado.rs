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
/// Anuncios ativos por personagem (itens e TP juntos), before any bought slots.
pub const MAX_ANUNCIOS: usize = 20;
/// Listing slots one purchase adds (`max_anuncios`).
pub const VAGAS_POR_COMPRA: usize = 5;
/// The price of each slot purchase, in TP or in gold (the player picks).
/// Their length is how many purchases a character can make: 20 + 6 × 5 = 50.
pub const CUSTO_VAGAS_TP: [u64; 6] = [100, 150, 200, 300, 400, 500];
pub const CUSTO_VAGAS_OURO: [u64; 6] = [5_000, 10_000, 20_000, 40_000, 80_000, 160_000];

/// Active listings allowed after `compras` slot purchases.
pub fn max_anuncios(compras: u8) -> usize {
    MAX_ANUNCIOS + VAGAS_POR_COMPRA * (compras as usize).min(CUSTO_VAGAS_TP.len())
}

/// The price of the next slot purchase, `(tp, gold)`; `None` at the cap.
pub fn proxima_vaga(compras: u8) -> Option<(u64, u64)> {
    let i = compras as usize;
    Some((*CUSTO_VAGAS_TP.get(i)?, *CUSTO_VAGAS_OURO.get(i)?))
}
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

// ─────────────────────── lot pricing (13/11/2026) ───────────────────────
//
// The owner: "quero vender x quantidade desse item com preço total tal, o
// que dá preço unitário tal" — and the buyer "pode escolher se quer comprar
// o lote todo ou uma parte dele, mas tem que ser número inteiro de gold".
//
// So a listing (and a buy order) carries a TOTAL in whole gold for the units
// still on it, and the unit price is `total / qtd`, fractions allowed: 1,000
// Copper for 3 gold is fine. A part costs `ceil(k × gold / units)` of what is
// LEFT, never less than 1 gold (nothing leaves for free); a part whose cost
// reaches all the gold left takes the whole rest for that same gold. Every
// purchase is whole gold and the seller ends with exactly the total asked.

/// Is `preco_total` gold for `qtd` units acceptable? At least 1 gold, at most
/// `PRECO_MAX_UNIT` per unit.
pub fn preco_total_valido(qtd: u64, preco_total: u64) -> bool {
    qtd > 0 && preco_total >= 1 && preco_total <= qtd.saturating_mul(PRECO_MAX_UNIT)
}

/// The unit price of `preco_total` gold for `qtd` units (fractional).
pub fn preco_unitario(qtd: u64, preco_total: u64) -> f64 {
    if qtd == 0 {
        0.0
    } else {
        preco_total as f64 / qtd as f64
    }
}

/// A unit price for the screen: "12" when whole (thousands separated), else
/// up to 3 decimals without trailing zeros ("0.003", "1.5").
pub fn fmt_unitario(u: f64) -> String {
    if u.fract() == 0.0 && u < 1e15 {
        return crate::idioma::milhar(u as u64);
    }
    let casas = if u >= 100.0 { 1 } else if u >= 1.0 { 2 } else { 3 };
    let s = format!("{u:.casas$}");
    let s = s.trim_end_matches('0').trim_end_matches('.');
    if s == "0" { "<0.001".into() } else { s.into() }
}

/// Buying `k` units out of a listing with `restante` units left for `gold`
/// gold: `(units received, whole gold paid)`. The cost is rounded UP and is
/// at least 1; when it would reach all the gold left (or `k` is the rest),
/// the buyer gets the whole rest for exactly that gold, so the listing never
/// keeps units with no gold on them. `None` = not a valid part.
pub fn compra_parcial(restante: u64, gold: u64, k: u64) -> Option<(u64, u64)> {
    if k == 0 || k > restante {
        return None;
    }
    if k == restante {
        return Some((restante, gold));
    }
    let c = u64::try_from((k as u128 * gold as u128).div_ceil(restante as u128)).ok()?.max(1);
    if c >= gold {
        Some((restante, gold))
    } else {
        Some((k, c))
    }
}

/// Just the gold of `compra_parcial`.
pub fn custo_parcial(restante: u64, gold: u64, k: u64) -> Option<u64> {
    compra_parcial(restante, gold, k).map(|(_, c)| c)
}

/// The most units `c` gold buys out of what is left (the hint "1 gold buys up
/// to 333"): the largest `k` whose `compra_parcial` costs no more than `c`.
pub fn maximo_por(restante: u64, gold: u64, c: u64) -> u64 {
    if gold == 0 || c >= gold {
        return restante;
    }
    // ceil(k·G/R) <= c  <=>  k <= c·R/G (floor).
    let k = (c as u128 * restante as u128 / gold as u128) as u64;
    k.min(restante)
}

/// What a seller gets for `k` units filled into a buy order with `restante`
/// units wanted and `gold` in escrow: rounded DOWN (the escrow never runs
/// short), and the fill that completes the order takes all that is left.
pub fn pagamento_parcial(restante: u64, gold: u64, k: u64) -> Option<u64> {
    if k == 0 || k > restante {
        return None;
    }
    if k == restante {
        return Some(gold);
    }
    u64::try_from(k as u128 * gold as u128 / restante as u128).ok()
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
    /// The slot purchase the player saw is no longer the next one (bought
    /// twice, or already at the cap).
    VagaMudou,
    /// Buy orders take only stackable, non-gear items without an instance.
    NaoPedivel,
    /// `MAX_PEDIDOS` open buy orders already.
    MuitosPedidos,
    /// The buy order is gone, filled or cancelled.
    PedidoFechado,
    /// Selling to your own buy order.
    ProprioPedido,
    /// A fill into a buy order worth less than 1 gold.
    MuitoPouco,
}

impl Recusa {
    pub fn texto(&self) -> String {
        match self {
            Recusa::NivelBaixo => {
                format!("Precisa do nível {NIVEL_PARA_VENDER} para vender no mercado.")
            }
            Recusa::Vinculado => "Bound item: it cannot be sold.".into(),
            Recusa::Quantidade => "Invalid quantity.".into(),
            Recusa::Preco => format!(
                "Preço inválido (pelo menos 1 gold no total, até {PRECO_MAX_UNIT} por unidade)."
            ),
            Recusa::MuitosAnuncios => {
                "All your listing slots are in use. Buy more in My listings.".into()
            }
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
            Recusa::VagaMudou => {
                "Those listing slots were already bought; anything paid comes back in Deliveries.".into()
            }
            Recusa::NaoPedivel => {
                "Buy orders take only stackable items (materials, potions, keys, scrolls, Energy).".into()
            }
            Recusa::MuitosPedidos => {
                format!("You already have {MAX_PEDIDOS} open buy orders. Cancel one first.")
            }
            Recusa::PedidoFechado => {
                "That buy order is already filled or cancelled; your items come back in Deliveries.".into()
            }
            Recusa::ProprioPedido => {
                "You can't sell to your own buy order; your items come back in Deliveries.".into()
            }
            Recusa::MuitoPouco => {
                "Too few for this buy order: it would pay less than 1 gold. Sell more; your items come back in Deliveries.".into()
            }
        }
    }
}

/// Preco por unidade aceito?
pub fn preco_valido(preco_unit: u64) -> bool {
    (1..=PRECO_MAX_UNIT).contains(&preco_unit)
}

/// How far from the recommended price (as a fraction) a listing gets the
/// "far from market value" warning. 0.50 = more than 50% above or below.
pub const DESVIO_ALERTA: f64 = 0.50;

/// Signed deviation of `preco` from `recomendado` (+0.8 = 80% above), or
/// `None` with no recommended price to compare.
/// `preco` is a UNIT price, fractional since lot pricing.
pub fn desvio(preco: f64, recomendado: Option<u64>) -> Option<f64> {
    let r = recomendado.filter(|r| *r > 0)?;
    Some(preco / r as f64 - 1.0)
}

/// The listing is far enough from the market value to warn.
pub fn longe_do_mercado(preco: f64, recomendado: Option<u64>) -> bool {
    desvio(preco, recomendado).is_some_and(|d| d.abs() > DESVIO_ALERTA)
}

/// Can this listing be repriced to `novo_total` gold for the `restante`
/// units still on it? (`dono` = it is mine, `estado` = its `ESTADO_*`). No
/// fee and no escrow change: the items are already held by the listing.
pub fn validar_ajuste(
    dono: bool,
    estado: u8,
    restante: u64,
    total_atual: u64,
    novo_total: u64,
) -> Result<(), &'static str> {
    if !dono {
        return Err("Listing not found.");
    }
    if estado != ESTADO_ATIVO {
        return Err("The listing is no longer active.");
    }
    if !preco_total_valido(restante, novo_total) {
        return Err("Invalid price.");
    }
    if novo_total == total_atual {
        return Err("The listing already has that price.");
    }
    Ok(())
}

/// Anunciar `qtd` de um slot que tem `no_slot`. O limite de anuncios ativos
/// e' conferido de novo no central (e' la' que a contagem e' verdadeira).
pub fn pode_anunciar(
    nivel: u32,
    vinculado: bool,
    no_slot: u32,
    qtd: u32,
    preco_total: u64,
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
    if !preco_total_valido(qtd as u64, preco_total) {
        return Err(Recusa::Preco);
    }
    Ok(preco_total)
}

/// Anunciar TP: mesmo portao de nivel.
pub fn pode_anunciar_tp(nivel: u32, qtd: u64, preco_total: u64) -> Result<u64, Recusa> {
    if nivel < NIVEL_PARA_VENDER {
        return Err(Recusa::NivelBaixo);
    }
    if qtd == 0 || qtd > TP_MAX_POR_ANUNCIO {
        return Err(Recusa::Quantidade);
    }
    if !preco_total_valido(qtd, preco_total) {
        return Err(Recusa::Preco);
    }
    Ok(preco_total)
}

/// Selling Energy: the level gate, enough Energy for the lots, a valid price.
pub fn pode_anunciar_energia(nivel: u32, energia: u64, lotes: u64, preco_total: u64) -> Result<u64, Recusa> {
    if nivel < NIVEL_PARA_VENDER {
        return Err(Recusa::NivelBaixo);
    }
    if lotes == 0 || lotes > LOTES_MAX_POR_ANUNCIO || lotes.saturating_mul(ENERGIA_POR_LOTE) > energia {
        return Err(Recusa::Quantidade);
    }
    if !preco_total_valido(lotes, preco_total) {
        return Err(Recusa::Preco);
    }
    Ok(preco_total)
}

/// Comprar `qtd` paying `pago` (what the client worked out from the listing
/// it saw, `custo_parcial`): the gold that leaves the buyer now. The central
/// recomputes the cost; any change comes back with the items.
pub fn pode_comprar(gold: u64, qtd: u64, pago: u64) -> Result<u64, Recusa> {
    if qtd == 0 {
        return Err(Recusa::Quantidade);
    }
    if pago == 0 || pago > qtd.saturating_mul(PRECO_MAX_UNIT) {
        return Err(Recusa::Preco);
    }
    if pago > gold {
        return Err(Recusa::SemGold);
    }
    Ok(pago)
}

/// O central fechando uma compra. Nada de "compra o que sobrou": ou a
/// quantidade pedida inteira, ou nada (e o gold volta por carta).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Fechamento {
    pub vendida: u64,
    pub sobra: u64,
    /// Gold still asked for the `sobra` units.
    pub gold_sobra: u64,
    pub bruto: u64,
    pub taxa: u64,
    pub liquido: u64,
    /// Paid beyond the cost (the listing got cheaper): back to the buyer.
    pub troco: u64,
}

/// `gold_anuncio` is the listing's total for its `restante` units; `pago`
/// is what left the buyer. Costing more than was paid is `PrecoMudou` (the
/// gold goes back); costing less sends the change with the items.
pub fn fechar_compra(
    restante: u64,
    estado: u8,
    gold_anuncio: u64,
    pedida: u64,
    pago: u64,
) -> Result<Fechamento, Recusa> {
    if estado != ESTADO_ATIVO || pedida == 0 || pedida > restante {
        return Err(Recusa::Esgotado);
    }
    let (vendida, bruto) = compra_parcial(restante, gold_anuncio, pedida).ok_or(Recusa::Preco)?;
    if bruto > pago {
        return Err(Recusa::PrecoMudou);
    }
    let taxa = taxa(bruto);
    Ok(Fechamento {
        vendida,
        sobra: restante - vendida,
        gold_sobra: gold_anuncio - bruto,
        bruto,
        taxa,
        liquido: bruto - taxa,
        troco: pago - bruto,
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

// ─────────────────────────── buy orders ───────────────────────────
//
// "I want N of X at P gold each". The buyer's gold (N × P) leaves the
// character when the order is placed and waits in escrow on the central
// database; each seller fills part of it, gets the gold (minus the same fee as
// a normal sale) and the buyer gets the items, both by letter. Cancelling
// refunds what is left. Same outbox/letter guarantees as listings.

/// Open buy orders per character.
pub const MAX_PEDIDOS: usize = 10;
/// Most units in one buy order.
pub const QTD_MAX_POR_PEDIDO: u64 = 1_000_000;
/// Buy orders in one search answer.
pub const PEDIDOS_POR_BUSCA: usize = 50;

pub const PEDIDO_ABERTO: u8 = 0;
pub const PEDIDO_CONCLUIDO: u8 = 1;
pub const PEDIDO_CANCELADO: u8 = 2;

/// Can `item_id` be the subject of a buy order? Only things that stack and
/// carry no instance: materials, darksteel, keys, scrolls, potions… and the
/// market's Energy lots. Never gear (pets and mounts are gear), bound items,
/// or the retired Teleport Scroll.
pub fn pode_pedir_item(item_id: u16, equipavel: bool, vinculado: bool, stack_max: u32) -> bool {
    use crate::constants::item_id as it;
    if item_id == it::ENERGIA_MIL {
        return true;
    }
    item_id != 0
        && item_id != it::PERGAMINHO_TELEPORTE
        && !equipavel
        && crate::constants::equip_slot_of(item_id).is_none()
        && !vinculado
        && stack_max > 1
}

/// Placing an order for `qtd` units at `preco_total` gold in all: the gold
/// that goes into escrow now.
pub fn pode_pedir(gold: u64, qtd: u64, preco_total: u64) -> Result<u64, Recusa> {
    if qtd == 0 || qtd > QTD_MAX_POR_PEDIDO {
        return Err(Recusa::Quantidade);
    }
    if !preco_total_valido(qtd, preco_total) {
        return Err(Recusa::Preco);
    }
    if preco_total > gold {
        return Err(Recusa::SemGold);
    }
    Ok(preco_total)
}

/// Selling `qtd` to an order with `tem` of the item on hand: the level gate
/// of any sale, and enough of the item.
pub fn pode_vender_ao_pedido(nivel: u32, tem: u64, qtd: u64) -> Result<(), Recusa> {
    if nivel < NIVEL_PARA_VENDER {
        return Err(Recusa::NivelBaixo);
    }
    if qtd == 0 || qtd > tem || qtd > QTD_MAX_POR_PEDIDO {
        return Err(Recusa::Quantidade);
    }
    Ok(())
}

/// The central database filling an order with what a seller offered.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Preenchimento {
    /// Units that go to the buyer.
    pub vendida: u64,
    /// What the order still wants afterwards.
    pub restante: u64,
    /// Gold left in escrow for those `restante` units.
    pub gold_restante: u64,
    /// Units that go back to the seller (the order wanted fewer).
    pub devolve: u64,
    pub bruto: u64,
    pub taxa: u64,
    /// Gold to the seller.
    pub liquido: u64,
}

/// Fills `min(oferecida, restante)` of an order holding `gold` for its
/// `restante` units. Unlike a purchase, a partial fill is fine: whatever the
/// order did not want goes back to the seller. A fill worth less than 1 gold
/// is refused (`MuitoPouco`): nobody sells for nothing.
pub fn preencher_pedido(
    restante: u64,
    estado: u8,
    gold: u64,
    oferecida: u64,
) -> Result<Preenchimento, Recusa> {
    if estado != PEDIDO_ABERTO || restante == 0 {
        return Err(Recusa::PedidoFechado);
    }
    if oferecida == 0 {
        return Err(Recusa::Quantidade);
    }
    let vendida = oferecida.min(restante);
    let bruto = pagamento_parcial(restante, gold, vendida).ok_or(Recusa::Preco)?;
    if bruto == 0 {
        return Err(Recusa::MuitoPouco);
    }
    let taxa = taxa(bruto);
    Ok(Preenchimento {
        vendida,
        restante: restante - vendida,
        gold_restante: gold - bruto,
        devolve: oferecida - vendida,
        bruto,
        taxa,
        liquido: bruto - taxa,
    })
}

/// Gold back to the buyer when an order is cancelled: all that is still in
/// escrow (it was paid for up front).
pub fn reembolso_do_pedido(gold_restante: u64) -> u64 {
    gold_restante
}

/// A buy order as the client sees it.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PedidoNet {
    pub id: String,
    pub item_id: u16,
    pub nome: String,
    pub categoria: u8,
    pub qtd_total: u64,
    /// Units the order still wants.
    pub qtd: u64,
    /// Gold still in escrow for those `qtd` units (lot pricing: the unit
    /// price is `preco_total / qtd`, fractions allowed).
    pub preco_total: u64,
    pub realm: String,
    pub comprador: String,
    /// This character's order.
    pub meu: bool,
}

impl PedidoNet {
    pub fn unitario(&self) -> f64 {
        preco_unitario(self.qtd, self.preco_total)
    }
    /// Gold a seller gets for `k` units now.
    pub fn pagamento(&self, k: u64) -> Option<u64> {
        pagamento_parcial(self.qtd, self.preco_total, k)
    }
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
    /// Gold asked for those `qtd` units (lot pricing: the unit price is
    /// `preco_total / qtd`, fractions allowed).
    pub preco_total: u64,
    pub realm: String,
    pub vendedor: String,
    /// Anuncio deste personagem.
    pub meu: bool,
}

impl AnuncioNet {
    pub fn unitario(&self) -> f64 {
        preco_unitario(self.qtd, self.preco_total)
    }
    /// Whole gold that buying `k` units costs now (`custo_parcial`).
    pub fn custo(&self, k: u64) -> Option<u64> {
        custo_parcial(self.qtd, self.preco_total, k)
    }
    /// `(units received, gold)` for asking `k` (`compra_parcial`).
    pub fn compra(&self, k: u64) -> Option<(u64, u64)> {
        compra_parcial(self.qtd, self.preco_total, k)
    }
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
    #[test]
    fn longe_do_mercado_e_ajuste_de_preco() {
        assert_eq!(desvio(150.0, Some(100)), Some(0.5));
        assert!(!longe_do_mercado(150.0, Some(100)), "exactly 50% is not past it");
        assert!(longe_do_mercado(151.0, Some(100)));
        assert!(longe_do_mercado(49.0, Some(100)));
        assert!(!longe_do_mercado(50.0, Some(100)));
        assert!(!longe_do_mercado(1_000_000.0, None), "nothing to compare");
        assert!(!longe_do_mercado(10.0, Some(0)));
        // (dono, estado, units left, current total, new total)
        assert_eq!(validar_ajuste(false, ESTADO_ATIVO, 1, 10, 20), Err("Listing not found."));
        assert_eq!(validar_ajuste(true, ESTADO_ATIVO + 1, 1, 10, 20), Err("The listing is no longer active."));
        assert_eq!(validar_ajuste(true, ESTADO_ATIVO, 1, 10, 0), Err("Invalid price."));
        assert_eq!(validar_ajuste(true, ESTADO_ATIVO, 1, 10, PRECO_MAX_UNIT + 1), Err("Invalid price."));
        assert!(validar_ajuste(true, ESTADO_ATIVO, 1, 10, 10).is_err());
        assert_eq!(validar_ajuste(true, ESTADO_ATIVO, 1, 10, 20), Ok(()));
        assert_eq!(validar_ajuste(true, ESTADO_ATIVO, 1_000, 10, 3), Ok(()), "under 1 gold each is fine");
    }

    /// The owner's case: 1,000 Copper for 3 gold. Any part costs whole gold
    /// (rounded up), and however it is bought the seller gets exactly 3.
    #[test]
    fn lote_com_preco_unitario_fracionado() {
        assert!(preco_total_valido(1_000, 3));
        assert!(!preco_total_valido(1_000, 0), "at least 1 gold");
        assert!(!preco_total_valido(0, 3));
        assert_eq!(fmt_unitario(preco_unitario(1_000, 3)), "0.003");
        assert_eq!(fmt_unitario(preco_unitario(4, 6)), "1.5");
        assert_eq!(fmt_unitario(preco_unitario(10, 1_200_000)), "120,000");
        assert_eq!(compra_parcial(1_000, 3, 1_000), Some((1_000, 3)), "the whole lot");
        assert_eq!(compra_parcial(1_000, 3, 1), Some((1, 1)), "one unit: rounded up to 1 gold");
        assert_eq!(compra_parcial(1_000, 3, 334), Some((334, 2)));
        assert_eq!(compra_parcial(1_000, 3, 700), Some((1_000, 3)), "3 gold anyway: the whole rest");
        assert_eq!(compra_parcial(1_000, 3, 0), None);
        assert_eq!(compra_parcial(1_000, 3, 1_001), None);
        assert_eq!(maximo_por(1_000, 3, 1), 333, "1 gold buys up to 333");
        assert_eq!(compra_parcial(1_000, 3, 333), Some((333, 1)));
        assert_eq!(maximo_por(1_000, 3, 3), 1_000);
        // Expensive units: 10 for 70 gold, one unit is 7.
        assert_eq!(compra_parcial(10, 70, 1), Some((1, 7)));
        assert_eq!(maximo_por(10, 70, 7), 1);
        // Bought in pieces, the seller still ends with exactly the total.
        for partes in [vec![1_000u64], vec![300, 300, 400], vec![1, 999], vec![333, 333, 333, 1], vec![1; 1_000]] {
            let (mut qtd, mut gold, mut pago) = (1_000u64, 3u64, 0u64);
            for k in partes {
                if qtd == 0 {
                    break;
                }
                let f = fechar_compra(qtd, ESTADO_ATIVO, gold, k.min(qtd), u64::MAX).unwrap();
                assert!(f.bruto >= 1, "nothing leaves for free");
                assert!(f.sobra == 0 || f.gold_sobra >= 1, "what is left still has gold on it");
                pago += f.bruto;
                qtd = f.sobra;
                gold = f.gold_sobra;
            }
            assert_eq!((qtd, gold), (0, 0));
            assert_eq!(pago, 3, "never more, never less than the asked total");
        }
    }

    /// A buy order of 1,000 for 3 gold pays sellers rounding DOWN, and the
    /// fill that completes it takes all that is left: escrow adds up exactly.
    #[test]
    fn pedido_com_preco_unitario_fracionado() {
        assert_eq!(preencher_pedido(1_000, PEDIDO_ABERTO, 3, 100), Err(Recusa::MuitoPouco), "0.3 gold");
        let a = preencher_pedido(1_000, PEDIDO_ABERTO, 3, 400).unwrap();
        assert_eq!((a.bruto, a.restante, a.gold_restante), (1, 600, 2));
        let b = preencher_pedido(a.restante, PEDIDO_ABERTO, a.gold_restante, 700).unwrap();
        assert_eq!((b.vendida, b.devolve, b.bruto, b.gold_restante), (600, 100, 2, 0));
        assert_eq!(a.bruto + b.bruto, 3);
        assert_eq!(pagamento_parcial(1_000, 3, 1_000), Some(3));
    }


    #[test]
    fn vagas_compradas_sobem_de_cinco_ate_50() {
        assert_eq!(max_anuncios(0), 20);
        assert_eq!(max_anuncios(1), 25);
        assert_eq!(max_anuncios(6), 50);
        assert_eq!(max_anuncios(200), 50);
        assert_eq!(proxima_vaga(0), Some((100, 5_000)));
        assert_eq!(proxima_vaga(5), Some((500, 160_000)));
        assert_eq!(proxima_vaga(6), None);
    }

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
        assert_eq!(pode_anunciar(20, false, 10, 3, 300), Ok(300));
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
        assert_eq!(pode_anunciar(30, false, 2, 2, 1), Ok(1), "half a gold each");
        assert_eq!(pode_comprar(1_000, 3, 0), Err(Recusa::Preco));
        assert_eq!(pode_comprar(299, 3, 300), Err(Recusa::SemGold));
        assert_eq!(pode_comprar(300, 3, 300), Ok(300));
    }

    #[test]
    fn compra_parcial_divide_certo() {
        // 10 at 7 each = 70 in all; 4 bought, paid exactly.
        let f = fechar_compra(10, ESTADO_ATIVO, 70, 4, 28).unwrap();
        assert_eq!(
            f,
            Fechamento {
                vendida: 4,
                sobra: 6,
                gold_sobra: 42,
                bruto: 28,
                taxa: 2,
                liquido: 26,
                troco: 0,
            }
        );
        let f2 = fechar_compra(f.sobra, ESTADO_ATIVO, f.gold_sobra, 6, 50).unwrap();
        assert_eq!((f2.sobra, f2.bruto, f2.troco), (0, 42, 8), "the listing got cheaper: change");
        assert_eq!(
            f.vendida + f2.vendida,
            10,
            "nada some nem duplica entre as duas"
        );
    }

    #[test]
    fn compra_recusada_nao_fecha_nada() {
        assert_eq!(
            fechar_compra(3, ESTADO_ATIVO, 21, 4, 28),
            Err(Recusa::Esgotado)
        );
        assert_eq!(
            fechar_compra(3, ESTADO_CANCELADO, 21, 1, 7),
            Err(Recusa::Esgotado)
        );
        assert_eq!(
            fechar_compra(3, ESTADO_ATIVO, 24, 1, 7),
            Err(Recusa::PrecoMudou),
            "now 8 each, paid 7"
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
    fn pedido_so_de_empilhavel_sem_equipamento() {
        use crate::constants::item_id as it;
        assert!(pode_pedir_item(it::DARKSTEEL, false, false, 999));
        assert!(pode_pedir_item(it::HEALTH_POTION, false, false, 99));
        assert!(pode_pedir_item(it::ENERGIA_MIL, false, false, 0), "Energy lots always");
        assert!(!pode_pedir_item(it::KATANA, true, false, 1), "gear");
        assert!(!pode_pedir_item(it::KATANA, false, false, 99), "gear by its slot");
        assert!(!pode_pedir_item(it::pet_no_grau(it::PETS[0], 2), false, false, 99), "pets are gear");
        assert!(!pode_pedir_item(it::DARKSTEEL, false, true, 999), "bound");
        assert!(!pode_pedir_item(it::DARKSTEEL, false, false, 1), "does not stack");
        assert!(!pode_pedir_item(it::PERGAMINHO_TELEPORTE, false, false, 99));
        assert!(!pode_pedir_item(0, false, false, 99));
    }

    #[test]
    fn pedir_reserva_o_total() {
        assert_eq!(pode_pedir(1_000, 10, 1_000), Ok(1_000));
        assert_eq!(pode_pedir(999, 10, 1_000), Err(Recusa::SemGold));
        assert_eq!(pode_pedir(1_000, 0, 100), Err(Recusa::Quantidade));
        assert_eq!(pode_pedir(u64::MAX, QTD_MAX_POR_PEDIDO + 1, 1), Err(Recusa::Quantidade));
        assert_eq!(pode_pedir(1_000, 1, 0), Err(Recusa::Preco));
        assert_eq!(pode_pedir(1_000, 1_000, 3), Ok(3), "under 1 gold each");
        assert_eq!(pode_pedir(u64::MAX, 1, PRECO_MAX_UNIT + 1), Err(Recusa::Preco));
        assert_eq!(pode_vender_ao_pedido(19, 10, 1), Err(Recusa::NivelBaixo));
        assert_eq!(pode_vender_ao_pedido(20, 10, 11), Err(Recusa::Quantidade));
        assert_eq!(pode_vender_ao_pedido(20, 10, 0), Err(Recusa::Quantidade));
        assert_eq!(pode_vender_ao_pedido(20, 10, 10), Ok(()));
    }

    #[test]
    fn preencher_pedido_divide_e_devolve_o_excesso() {
        // Order wants 10 at 7 (70 held); a seller offers 4: all 4 sell.
        let a = preencher_pedido(10, PEDIDO_ABERTO, 70, 4).unwrap();
        assert_eq!(
            a,
            Preenchimento { vendida: 4, restante: 6, gold_restante: 42, devolve: 0, bruto: 28, taxa: 2, liquido: 26 }
        );
        // Another offers 9: 6 sell, 3 go back.
        let b = preencher_pedido(a.restante, PEDIDO_ABERTO, a.gold_restante, 9).unwrap();
        assert_eq!((b.vendida, b.restante, b.devolve), (6, 0, 3));
        assert_eq!(b.bruto, 42);
        assert_eq!(b.liquido, liquido(42));
        assert_eq!(a.vendida + b.vendida, 10, "never more than asked");
        // Escrow adds up: what sellers got + fees burned = what the buyer paid.
        assert_eq!(a.liquido + a.taxa + b.liquido + b.taxa, 10 * 7);
        assert_eq!(preencher_pedido(0, PEDIDO_ABERTO, 0, 1), Err(Recusa::PedidoFechado));
        assert_eq!(preencher_pedido(5, PEDIDO_CANCELADO, 35, 1), Err(Recusa::PedidoFechado));
        assert_eq!(preencher_pedido(5, PEDIDO_CONCLUIDO, 35, 1), Err(Recusa::PedidoFechado));
        assert_eq!(preencher_pedido(5, PEDIDO_ABERTO, 35, 0), Err(Recusa::Quantidade));
    }

    #[test]
    fn cancelar_pedido_devolve_o_que_sobrou() {
        let pago = 10 * 7;
        let f = preencher_pedido(10, PEDIDO_ABERTO, pago, 4).unwrap();
        assert_eq!(f.bruto + reembolso_do_pedido(f.gold_restante), pago);
        assert_eq!(reembolso_do_pedido(0), 0);
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
