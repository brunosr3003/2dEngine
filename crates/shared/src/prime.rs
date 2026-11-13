//! The PRIME MERCHANT: a gold-only shop in every town square whose prices
//! follow the market (`crate::precos`).
//!
//! - It sells a short, curated list (`LINHAS`, the one place to edit it):
//!   the Experience Potion, craft keys up to purple, the coloured craft
//!   materials up to blue, Darksteel and Energy lots.
//! - Price = the market's recommended price × (1 + `MARKUP_PCT`%), rounded
//!   UP, at least 1 gold. An item the table has no price for is not shown.
//! - Each item has a per-character DAILY limit (game day, `dungeon::dia`).
//! - The gold is DESTROYED (a sink): nobody receives it.
//!
//! The server decides everything; the client only draws the board it was
//! sent and asks to buy.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::constants::item_id;

/// Markup over the market's recommended price, in percent.
pub const MARKUP_PCT: u64 = 15;

/// Price points kept per item for the board's sparkline (one per recount of
/// the market, `PRECOS_INTERVALO_S` = 5 min: two hours).
pub const HISTORICO_MAX: usize = 24;

/// Most units one purchase may ask for (the daily limits are lower anyway,
/// except Darksteel).
pub const QTD_MAX_POR_COMPRA: u32 = 50_000;

/// One line of the catalogue: these item families, in these colours, with
/// this daily limit PER ITEM.
#[derive(Debug, Clone, Copy)]
pub struct Linha {
    /// Item ids. With `cores`, the grey (colour 1) id of each family.
    pub bases: &'static [u16],
    /// `Some((from, to))`: every colour in the range (`item_id::chave_na_cor`
    /// for keys, `item_id::na_cor` for materials). `None`: the id itself.
    pub cores: Option<(u8, u8)>,
    /// Units one character may buy per game day, per item.
    pub limite_dia: u32,
}

/// THE CATALOGUE. Order here is the board's order.
///
/// - Keys: grey to purple. Legendary keys stay out: they are the top of the
///   Combine and the boss chase.
/// - Materials: grey, green and blue. PURPLE IS LEFT OUT on purpose: blue
///   to purple is where the Combine burns Glittering Powder, copper and
///   Darksteel; selling purple would skip that sink, and the purple price
///   is mostly the synthesis cost, so it would rarely be cheaper anyway.
/// - Glittering Powder is left out too: it is expensive on purpose and the
///   Magic Island is its shop.
pub const LINHAS: &[Linha] = &[
    Linha { bases: &[item_id::XP_POTION], cores: None, limite_dia: 5 },
    Linha { bases: &[item_id::ENERGIA_MIL], cores: None, limite_dia: 20 },
    Linha { bases: &[item_id::DARKSTEEL], cores: None, limite_dia: 50_000 },
    Linha { bases: &item_id::CHAVES, cores: Some((1, 4)), limite_dia: 3 },
    Linha { bases: &crate::combinar::MATERIAIS, cores: Some((1, 3)), limite_dia: 500 },
];

/// One sellable item.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ItemPrime {
    pub item_id: u16,
    pub limite_dia: u32,
}

fn na_cor(base: u16, cor: u8) -> u16 {
    if item_id::CHAVES.contains(&base) {
        item_id::chave_na_cor(base, cor)
    } else {
        item_id::na_cor(base, cor)
    }
}

/// Every sellable item, in board order.
pub fn catalogo() -> Vec<ItemPrime> {
    let mut v = Vec::new();
    for l in LINHAS {
        for &b in l.bases {
            match l.cores {
                None => v.push(ItemPrime { item_id: b, limite_dia: l.limite_dia }),
                Some((de, ate)) => {
                    for cor in de..=ate {
                        v.push(ItemPrime { item_id: na_cor(b, cor), limite_dia: l.limite_dia });
                    }
                }
            }
        }
    }
    v
}

/// The catalogue entry of `id`, if the Prime sells it.
pub fn item(id: u16) -> Option<ItemPrime> {
    catalogo().into_iter().find(|i| i.item_id == id)
}

/// Energy is sold in the market's lots (`item_id::ENERGIA_MIL`) and lands in
/// the character's Energy, never in the bag.
pub fn e_energia(id: u16) -> bool {
    id == item_id::ENERGIA_MIL
}

/// The Prime's price for a recommended price: +`MARKUP_PCT`%, rounded up,
/// at least 1 gold.
pub fn com_markup(recomendado: u64) -> u64 {
    let v = recomendado as u128 * (100 + MARKUP_PCT) as u128;
    (v.div_ceil(100).min(u64::MAX as u128) as u64).max(1)
}

/// The Prime's unit price of `id` from the market table (`precos::mapa`).
/// `None` = the table has no price for it: the board hides it.
pub fn preco(tabela: &HashMap<u16, f64>, id: u16) -> Option<u64> {
    crate::precos::recomendado(tabela, id, None).map(com_markup)
}

// ─────────────────────────────── per character ───────────────────────────────

/// What a character used up TODAY: the free XP potions of the dailies
/// (`quests::POCOES_XP_GRATIS_POR_DIA`) and the Prime purchases per item.
/// Persisted with the character (`characters.diario_json`). Resets by
/// itself: every read and write first checks the day.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Diario {
    /// Game day (`dungeon::dia`) the counters belong to.
    pub dia: i64,
    /// Free Experience Potions paid by dailies today.
    pub pocoes_xp: u32,
    /// (item, units bought today) at the Prime.
    pub compras: Vec<(u16, u32)>,
}

impl Diario {
    pub fn de_json(s: &str) -> Self {
        serde_json::from_str(s).unwrap_or_default()
    }

    pub fn para_json(&self) -> String {
        serde_json::to_string(self).unwrap_or_default()
    }

    /// Moves to `dia`, zeroing everything if it is a new day.
    fn no_dia(&mut self, dia: i64) {
        if self.dia != dia {
            *self = Diario { dia, ..Default::default() };
        }
    }

    /// Free XP potions already paid on `dia`.
    pub fn pocoes_xp_hoje(&self, dia: i64) -> u32 {
        if self.dia == dia { self.pocoes_xp } else { 0 }
    }

    /// Pays up to `qtd` free XP potions on `dia`, within the cap. Returns how
    /// many may be given (0 = cap reached).
    pub fn pagar_pocoes_xp(&mut self, dia: i64, qtd: u32) -> u32 {
        self.no_dia(dia);
        let cabe = crate::quests::POCOES_XP_GRATIS_POR_DIA.saturating_sub(self.pocoes_xp);
        let pagas = qtd.min(cabe);
        self.pocoes_xp += pagas;
        pagas
    }

    /// Units of `id` bought at the Prime on `dia`.
    pub fn comprado(&self, dia: i64, id: u16) -> u32 {
        if self.dia != dia {
            return 0;
        }
        self.compras.iter().find(|(i, _)| *i == id).map_or(0, |(_, q)| *q)
    }

    pub fn registrar_compra(&mut self, dia: i64, id: u16, qtd: u32) {
        self.no_dia(dia);
        match self.compras.iter_mut().find(|(i, _)| *i == id) {
            Some((_, q)) => *q = q.saturating_add(qtd),
            None => self.compras.push((id, qtd)),
        }
    }
}

/// Why a purchase was refused.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Recusa {
    NaoVende,
    SemPreco,
    Quantidade,
    /// Daily limit: how many are still allowed today.
    Limite { resta: u32, limite: u32 },
    /// The price went UP since the board was drawn: the current one.
    PrecoMudou { agora: u64 },
    OuroInsuficiente { falta: u64 },
}

impl Recusa {
    pub fn texto(&self) -> String {
        match self {
            Recusa::NaoVende => "The Prime Merchant does not sell that.".into(),
            Recusa::SemPreco => "No market price for that yet. Try again after the next update.".into(),
            Recusa::Quantidade => "Choose a quantity.".into(),
            Recusa::Limite { resta: 0, limite } => {
                format!("Daily limit reached ({limite}/{limite}). Come back tomorrow.")
            }
            Recusa::Limite { resta, limite } => {
                format!("Only {resta} more today (daily limit {limite}).")
            }
            Recusa::PrecoMudou { agora } => {
                format!("The price changed: now {agora} gold each. Check and buy again.")
            }
            Recusa::OuroInsuficiente { falta } => format!("{falta} gold short."),
        }
    }
}

/// A purchase the server accepted: what to charge.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Compra {
    pub item: ItemPrime,
    pub qtd: u32,
    pub preco_unit: u64,
    pub total: u64,
}

/// Checks a purchase. `preco_atual` is the price from the server's table
/// NOW; `preco_visto` the one the player's board showed. A price that went
/// up is refused (the player confirms the new one); one that went down is
/// simply charged at the current, lower price.
pub fn validar(
    diario: &Diario,
    dia: i64,
    ouro: u64,
    id: u16,
    qtd: u32,
    preco_atual: Option<u64>,
    preco_visto: u64,
) -> Result<Compra, Recusa> {
    let item = item(id).ok_or(Recusa::NaoVende)?;
    if qtd == 0 || qtd > QTD_MAX_POR_COMPRA {
        return Err(Recusa::Quantidade);
    }
    let preco_unit = preco_atual.ok_or(Recusa::SemPreco)?;
    let resta = item.limite_dia.saturating_sub(diario.comprado(dia, id));
    if qtd > resta {
        return Err(Recusa::Limite { resta, limite: item.limite_dia });
    }
    if preco_unit > preco_visto {
        return Err(Recusa::PrecoMudou { agora: preco_unit });
    }
    let total = preco_unit.saturating_mul(qtd as u64);
    if total > ouro {
        return Err(Recusa::OuroInsuficiente { falta: total - ouro });
    }
    Ok(Compra { item, qtd, preco_unit, total })
}

// ─────────────────────────────── wire ───────────────────────────────

/// Client → server.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum PedidoPrime {
    /// Open (or refresh) the board. Only near the Prime Merchant.
    Abrir,
    /// Buy `qtd` of `item_id`; `preco_visto` = the unit price on the board.
    Comprar { item_id: u16, qtd: u32, preco_visto: u64 },
}

/// One row of the board.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LinhaPrime {
    pub item_id: u16,
    /// Current unit price, in gold.
    pub preco: u64,
    /// Earlier prices, oldest first, the current one LAST (≤ `HISTORICO_MAX`).
    pub historico: Vec<u64>,
    pub limite_dia: u32,
    pub comprado_hoje: u32,
}

impl LinhaPrime {
    /// Change against the previous update, in percent (`None` = no previous
    /// point yet).
    pub fn variacao_pct(&self) -> Option<f64> {
        let n = self.historico.len();
        if n < 2 {
            return None;
        }
        let antes = self.historico[n - 2] as f64;
        (antes > 0.0).then(|| (self.historico[n - 1] as f64 - antes) / antes * 100.0)
    }

    pub fn resta_hoje(&self) -> u32 {
        self.limite_dia.saturating_sub(self.comprado_hoje)
    }
}

/// The whole board.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct QuadroPrime {
    pub linhas: Vec<LinhaPrime>,
    /// Server clock (unix s) when this board was made, when the prices were
    /// last recomputed (0 = never yet) and when the next recount is due.
    pub agora: i64,
    pub atualizado: i64,
    pub proxima: i64,
}

/// Server → client.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum AvisoPrime {
    /// Open/refresh the board.
    Quadro(QuadroPrime),
    Recusa(String),
    /// Bought: `total` gold left the purse.
    Comprou { item_id: u16, qtd: u32, total: u64 },
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn preco_e_recomendado_mais_15_por_cento_arredondado_pra_cima() {
        assert_eq!(com_markup(100), 115);
        assert_eq!(com_markup(1), 2, "1.15 rounds up");
        assert_eq!(com_markup(0), 1, "at least 1 gold");
        assert_eq!(com_markup(7), 9, "8.05 -> 9");
        assert_eq!(com_markup(20), 23, "exact stays exact");
        assert_eq!(com_markup(1_000_001), 1_150_002, "1,150,001.15 -> up");
        // From the table: recommended rounds first (100.4 -> 100), then +15%.
        let t: HashMap<u16, f64> = [(item_id::DARKSTEEL, 100.4), (item_id::XP_POTION, 0.2)].into_iter().collect();
        assert_eq!(preco(&t, item_id::DARKSTEEL), Some(115));
        assert_eq!(preco(&t, item_id::XP_POTION), Some(2), "recommended floor 1 -> 1.15 -> 2");
        assert_eq!(preco(&t, item_id::STEEL), None, "no price: hidden");
    }

    #[test]
    fn catalogo_tem_o_que_foi_pedido_e_nada_lendario() {
        let c = catalogo();
        let ids: Vec<u16> = c.iter().map(|i| i.item_id).collect();
        assert!(ids.contains(&item_id::XP_POTION));
        assert!(ids.contains(&item_id::DARKSTEEL));
        assert!(ids.contains(&item_id::ENERGIA_MIL));
        assert_eq!(c.len(), 3 + 4 * 4 + 8 * 3);
        for &k in &item_id::CHAVES {
            for cor in 1..=4 {
                assert_eq!(item(item_id::chave_na_cor(k, cor)).unwrap().limite_dia, 3);
            }
            assert!(item(item_id::chave_na_cor(k, 5)).is_none(), "legendary key on sale");
        }
        for &m in &crate::combinar::MATERIAIS {
            assert_eq!(item(item_id::na_cor(m, 3)).unwrap().limite_dia, 500);
            assert!(item(item_id::na_cor(m, 4)).is_none(), "purple material on sale");
        }
        assert!(item(item_id::GLITTERING_POWDER).is_none());
        let mut dup = ids.clone();
        dup.sort();
        dup.dedup();
        assert_eq!(dup.len(), ids.len(), "duplicated item");
    }

    #[test]
    fn limite_diario_por_item_e_vira_o_dia() {
        let mut d = Diario::default();
        let t = 1_000;
        // 5 XP potions a day: 3 then 2, the 6th is refused.
        assert!(validar(&d, t, 1_000, item_id::XP_POTION, 3, Some(10), 10).is_ok());
        d.registrar_compra(t, item_id::XP_POTION, 3);
        assert!(validar(&d, t, 1_000, item_id::XP_POTION, 2, Some(10), 10).is_ok());
        d.registrar_compra(t, item_id::XP_POTION, 2);
        assert_eq!(
            validar(&d, t, 1_000, item_id::XP_POTION, 1, Some(10), 10),
            Err(Recusa::Limite { resta: 0, limite: 5 })
        );
        // Another item is untouched.
        assert!(validar(&d, t, 1_000, item_id::DARKSTEEL, 50, Some(10), 10).is_ok());
        // The next day the counter is back.
        assert_eq!(d.comprado(t + 1, item_id::XP_POTION), 0);
        assert!(validar(&d, t + 1, 1_000, item_id::XP_POTION, 5, Some(10), 10).is_ok());
        d.registrar_compra(t + 1, item_id::DARKSTEEL, 7);
        assert_eq!(d.comprado(t + 1, item_id::XP_POTION), 0, "new day wiped yesterday");
        assert_eq!(d.comprado(t + 1, item_id::DARKSTEEL), 7);
        // Asking for more than what is left says how many remain.
        let k = item_id::chave_na_cor(item_id::CHAVES[0], 2);
        assert_eq!(
            validar(&d, t + 1, 1_000, k, 4, Some(10), 10),
            Err(Recusa::Limite { resta: 3, limite: 3 })
        );
    }

    #[test]
    fn validar_preco_ouro_e_catalogo() {
        let d = Diario::default();
        let ok = validar(&d, 0, 230, item_id::DARKSTEEL, 2, Some(115), 115).unwrap();
        assert_eq!((ok.preco_unit, ok.total), (115, 230));
        assert_eq!(
            validar(&d, 0, 229, item_id::DARKSTEEL, 2, Some(115), 115),
            Err(Recusa::OuroInsuficiente { falta: 1 })
        );
        // Price went up since the board: refused with the new price.
        assert_eq!(
            validar(&d, 0, 1_000, item_id::DARKSTEEL, 1, Some(120), 115),
            Err(Recusa::PrecoMudou { agora: 120 })
        );
        // Went down: charged at the lower current price.
        assert_eq!(validar(&d, 0, 1_000, item_id::DARKSTEEL, 1, Some(100), 115).unwrap().total, 100);
        assert_eq!(validar(&d, 0, 1_000, item_id::DARKSTEEL, 1, None, 115), Err(Recusa::SemPreco));
        assert_eq!(validar(&d, 0, 1_000, item_id::GLITTERING_POWDER, 1, Some(1), 1), Err(Recusa::NaoVende));
        assert_eq!(validar(&d, 0, 1_000, item_id::DARKSTEEL, 0, Some(1), 1), Err(Recusa::Quantidade));
    }

    #[test]
    fn pocoes_de_xp_gratis_tres_por_dia() {
        let mut d = Diario::default();
        let dia = 20_000;
        assert_eq!(d.pagar_pocoes_xp(dia, 1), 1);
        assert_eq!(d.pagar_pocoes_xp(dia, 1), 1);
        assert_eq!(d.pagar_pocoes_xp(dia, 1), 1);
        assert_eq!(d.pagar_pocoes_xp(dia, 1), 0, "4th daily of the day pays none");
        assert_eq!(d.pocoes_xp_hoje(dia), 3);
        assert_eq!(d.pocoes_xp_hoje(dia + 1), 0);
        assert_eq!(d.pagar_pocoes_xp(dia + 1, 5), 3, "never more than the cap at once");
        // Survives the save.
        let volta = Diario::de_json(&d.para_json());
        assert_eq!(volta, d);
        assert_eq!(Diario::de_json(""), Diario::default());
    }

    #[test]
    fn variacao_contra_a_atualizacao_anterior() {
        let mut l = LinhaPrime { item_id: 1, preco: 110, historico: vec![100, 110], limite_dia: 5, comprado_hoje: 2 };
        assert!((l.variacao_pct().unwrap() - 10.0).abs() < 1e-9);
        l.historico = vec![110];
        assert_eq!(l.variacao_pct(), None);
        assert_eq!(l.resta_hoje(), 3);
    }
}
