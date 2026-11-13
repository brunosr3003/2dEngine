//! The Prime Merchant's price board (`shared::prime`): a ring buffer of the
//! last `HISTORICO_MAX` Prime prices of every catalogue item, one point per
//! recount of the market table (`crate::precos`, every `PRECOS_INTERVALO_S`).
//!
//! Kept in memory per process: a restarted zone starts with one point and
//! fills the sparkline as the recounts come in.

use std::collections::{HashMap, VecDeque};

use parking_lot::RwLock;
use shared::prime::{self, LinhaPrime, QuadroPrime};

#[derive(Default)]
struct Historico {
    /// Oldest first; each point has the Prime price of each catalogue item
    /// (None = no price in that table).
    pontos: VecDeque<HashMap<u16, u64>>,
    /// Unix s of the last recount, and the interval between recounts.
    ultimo: i64,
    passo: i64,
}

static HIST: RwLock<Historico> = RwLock::new(Historico {
    pontos: VecDeque::new(),
    ultimo: 0,
    passo: 300,
});

/// The Prime prices of every catalogue item in a market table.
pub fn precos_do_catalogo(tabela: &HashMap<u16, f64>) -> HashMap<u16, u64> {
    prime::catalogo()
        .iter()
        .filter_map(|i| prime::preco(tabela, i.item_id).map(|p| (i.item_id, p)))
        .collect()
}

/// Called after each recount of the market table.
pub fn registrar(tabela: &shared::precos::TabelaDePrecos, agora: i64, passo_s: u64) {
    let ponto = precos_do_catalogo(&tabela.mapa());
    let mut h = HIST.write();
    h.pontos.push_back(ponto);
    while h.pontos.len() > prime::HISTORICO_MAX {
        h.pontos.pop_front();
    }
    h.ultimo = agora;
    h.passo = passo_s as i64;
}

/// The current unit price of `id` (from the live table).
pub fn preco_atual(id: u16) -> Option<u64> {
    let (_, t) = crate::precos::atual();
    prime::preco(&t?.mapa(), id)
}

/// The board for a character: current prices from the live table, the
/// history from the ring buffer, and what they bought today.
pub fn quadro(diario: &prime::Diario, agora: i64) -> QuadroPrime {
    let (_, tabela) = crate::precos::atual();
    let atuais = tabela.map(|t| precos_do_catalogo(&t.mapa())).unwrap_or_default();
    let h = HIST.read();
    let dia = shared::dungeon::dia(agora);
    let linhas = prime::catalogo()
        .into_iter()
        .filter_map(|i| {
            let preco = *atuais.get(&i.item_id)?;
            let mut historico: Vec<u64> =
                h.pontos.iter().filter_map(|p| p.get(&i.item_id).copied()).collect();
            // The live price is always the last point.
            if historico.last() != Some(&preco) || historico.is_empty() {
                historico.push(preco);
            }
            let excesso = historico.len().saturating_sub(prime::HISTORICO_MAX);
            historico.drain(..excesso);
            Some(LinhaPrime {
                item_id: i.item_id,
                preco,
                historico,
                limite_dia: i.limite_dia,
                comprado_hoje: diario.comprado(dia, i.item_id),
            })
        })
        .collect();
    QuadroPrime {
        linhas,
        agora,
        atualizado: h.ultimo,
        proxima: if h.ultimo > 0 { h.ultimo + h.passo } else { 0 },
    }
}

#[cfg(test)]
pub(crate) fn zerar_para_teste() {
    *HIST.write() = Historico::default();
}

#[cfg(test)]
mod testes {
    use super::*;
    use shared::item_id;

    #[test]
    fn historico_guarda_os_ultimos_24() {
        let mut t = shared::precos::TabelaDePrecos::default();
        zerar_para_teste();
        for k in 0..30u64 {
            t.precos = vec![(item_id::DARKSTEEL, 100.0 + k as f64)];
            registrar(&t, 1_000 + k as i64 * 300, 300);
        }
        let h = HIST.read();
        assert_eq!(h.pontos.len(), prime::HISTORICO_MAX);
        assert_eq!(h.pontos.back().unwrap()[&item_id::DARKSTEEL], prime::com_markup(129));
        assert_eq!(h.ultimo, 1_000 + 29 * 300);
    }
}
