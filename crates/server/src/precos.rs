//! Recounts the realm's stock and keeps the market's recommended prices
//! (`shared::precos`) current.
//!
//! A task reads the database every `PRECOS_INTERVALO_S` (300 s by default):
//! every purse, bag, vault and equipped slot, plus what sits on sale or in the
//! mail of the central market for this realm. The table lands in a global with
//! a version; the world loop sends it to every player when the version moves,
//! and to each player at login. Every channel of a realm reads the same
//! database, so they all arrive at the same table.

use std::sync::Arc;
use std::time::Duration;

use parking_lot::RwLock;
use shared::precos::{Estoque, TabelaDePrecos};
use sqlx::{PgPool, Row};

static TABELA: RwLock<(u64, Option<Arc<TabelaDePrecos>>)> = RwLock::new((0, None));

/// The version the world loop last sent to everyone.
static ENVIADA: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// The table, if it changed since the world loop last sent it.
pub fn novidade() -> Option<Arc<TabelaDePrecos>> {
    use std::sync::atomic::Ordering;
    let (v, t) = atual();
    (v != ENVIADA.swap(v, Ordering::Relaxed)).then_some(t).flatten()
}

/// The current table and its version (0 = none yet).
pub fn atual() -> (u64, Option<Arc<TabelaDePrecos>>) {
    let g = TABELA.read();
    (g.0, g.1.clone())
}

pub fn spawn(pool: PgPool) {
    tokio::spawn(async move {
        let passo = std::env::var("PRECOS_INTERVALO_S")
            .ok()
            .and_then(|v| v.parse::<u64>().ok())
            .unwrap_or(300)
            .max(10);
        loop {
            match levantar(&pool, crate::mercado::central(), &crate::canais::realm()).await {
                Ok(e) => {
                    let t = shared::precos::calcular(&e);
                    tracing::info!(
                        "precos: {} itens com preço ({} gold, {} tipos de item no estoque)",
                        t.precos.len(),
                        e.ouro,
                        e.unidades.len()
                    );
                    let mut g = TABELA.write();
                    if g.1.as_deref() != Some(&t) {
                        g.0 += 1;
                        g.1 = Some(Arc::new(t));
                    }
                }
                Err(e) => tracing::warn!("precos: não recontou o estoque ({e})"),
            }
            tokio::time::sleep(Duration::from_secs(passo)).await;
        }
    });
}

async fn levantar(pool: &PgPool, central: Option<PgPool>, realm: &str) -> anyhow::Result<Estoque> {
    let mut e = Estoque::default();
    let ouro: i64 = sqlx::query_scalar("SELECT COALESCE(SUM(gold), 0)::bigint FROM characters")
        .fetch_one(pool)
        .await?;
    e.ouro = ouro.max(0) as u64;
    let linhas = sqlx::query(
        "SELECT item_id, SUM(qty)::bigint FROM (
             SELECT item_id, qty FROM inventory WHERE qty > 0
             UNION ALL SELECT item_id, qty FROM vault WHERE qty > 0
             UNION ALL SELECT item_id, 1 FROM equipment WHERE item_id > 0
         ) t WHERE item_id > 0 GROUP BY item_id",
    )
    .fetch_all(pool)
    .await?;
    for l in linhas {
        let id: i32 = l.get(0);
        let n: i64 = l.get(1);
        *e.unidades.entry(id as u16).or_default() += n.max(0) as u64;
    }
    let bases = sqlx::query("SELECT id, buy_price FROM items WHERE active AND buy_price IS NOT NULL AND buy_price > 0")
        .fetch_all(pool)
        .await?;
    for l in bases {
        let id: i32 = l.get(0);
        let preco: i32 = l.get(1);
        e.base_em_cobre.insert(id as u16, preco.max(0) as u64);
    }
    // On sale and in the mail of the central market: still this realm's.
    if let Some(c) = central {
        let a_venda = sqlx::query(
            "SELECT item_id, SUM(qtd_restante)::bigint FROM mercado_anuncios
              WHERE estado = $1 AND tipo = $2 AND realm = $3 GROUP BY item_id",
        )
        .bind(shared::mercado::ESTADO_ATIVO as i16)
        .bind(shared::mercado::TIPO_ITEM as i16)
        .bind(realm)
        .fetch_all(&c)
        .await?;
        for l in a_venda {
            let id: i32 = l.get(0);
            let n: i64 = l.get(1);
            *e.unidades.entry(id as u16).or_default() += n.max(0) as u64;
        }
        let cartas = sqlx::query(
            "SELECT item_id, COALESCE(SUM(qtd), 0)::bigint, COALESCE(SUM(gold), 0)::bigint FROM mercado_cartas
              WHERE entregue IS NULL AND realm = $1 GROUP BY item_id",
        )
        .bind(realm)
        .fetch_all(&c)
        .await?;
        for l in cartas {
            let id: i32 = l.get(0);
            let n: i64 = l.get(1);
            let g: i64 = l.get(2);
            if id > 0 {
                *e.unidades.entry(id as u16).or_default() += n.max(0) as u64;
            }
            e.ouro += g.max(0) as u64;
        }
    }
    Ok(e)
}
