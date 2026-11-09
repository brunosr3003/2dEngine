//! The REAL money of every Google Play order, from Google itself (owner,
//! 09/11/2026: "a page just for the finances: everything bought, revenue,
//! taxes, the Play Console fee, how much I actually earn").
//!
//! `loja_pedidos` only knows the base price of what was sold. What the buyer
//! actually paid (in their currency, with their country's tax), and what
//! Google passes on after its fee, comes from the Android Publisher Orders API
//! (`orders.get`). Every 15 minutes this task fetches the orders it doesn't
//! have yet — and refreshes the last 30 days', so a refund shows up — and
//! stores them in `loja_financeiro` (central DB), with a BRL estimate at the
//! day's exchange rate. Panóptico's "financeiro" tab only reads that table.

use std::collections::HashMap;
use std::time::Duration;

use anyhow::Result;
use serde::Deserialize;
use sqlx::PgPool;

const INTERVALO: Duration = Duration::from_secs(15 * 60);

pub async fn criar_tabelas(central: &PgPool) -> Result<()> {
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS loja_financeiro (
            externo           TEXT PRIMARY KEY,
            pedido            TEXT        NOT NULL,
            pais              TEXT        NOT NULL DEFAULT '',
            moeda             TEXT        NOT NULL DEFAULT '',
            total_micros      BIGINT      NOT NULL DEFAULT 0,
            imposto_micros    BIGINT      NOT NULL DEFAULT 0,
            receita_micros    BIGINT      NOT NULL DEFAULT 0,
            estado            TEXT        NOT NULL DEFAULT '',
            teste             BOOLEAN     NOT NULL DEFAULT FALSE,
            cotacao_brl       DOUBLE PRECISION NOT NULL DEFAULT 0,
            total_brl_centavos   BIGINT   NOT NULL DEFAULT 0,
            imposto_brl_centavos BIGINT   NOT NULL DEFAULT 0,
            receita_brl_centavos BIGINT   NOT NULL DEFAULT 0,
            atualizado_em     TIMESTAMPTZ NOT NULL DEFAULT NOW()
        )",
    )
    .execute(central)
    .await?;
    Ok(())
}

#[derive(Deserialize, Default)]
struct Dinheiro {
    #[serde(rename = "currencyCode", default)]
    moeda: String,
    #[serde(default)]
    units: Option<String>,
    #[serde(default)]
    nanos: Option<i64>,
}

impl Dinheiro {
    fn micros(&self) -> i64 {
        let u: i64 = self.units.as_deref().and_then(|u| u.parse().ok()).unwrap_or(0);
        u.saturating_mul(1_000_000) + self.nanos.unwrap_or(0) / 1_000
    }
}

#[derive(Deserialize, Default)]
struct Comprador {
    #[serde(rename = "buyerCountry", default)]
    pais: String,
}

#[derive(Deserialize)]
struct Pedido {
    #[serde(default)]
    state: String,
    #[serde(rename = "buyerAddress", default)]
    comprador: Comprador,
    #[serde(default)]
    total: Dinheiro,
    #[serde(default)]
    tax: Dinheiro,
    #[serde(rename = "developerRevenueInBuyerCurrency", default)]
    receita: Dinheiro,
}

/// BRL per one unit of `moeda` (ECB rates, cached per run).
async fn cotacao(cache: &mut HashMap<String, f64>, moeda: &str) -> f64 {
    if moeda.is_empty() || moeda == "BRL" {
        return 1.0;
    }
    if let Some(c) = cache.get(moeda) {
        return *c;
    }
    let url = format!("https://api.frankfurter.app/latest?from={moeda}&to=BRL");
    let v = async {
        let r: serde_json::Value = reqwest::Client::new()
            .get(&url)
            .timeout(Duration::from_secs(10))
            .send()
            .await
            .ok()?
            .json()
            .await
            .ok()?;
        r["rates"]["BRL"].as_f64()
    }
    .await
    .unwrap_or(0.0);
    if v > 0.0 {
        cache.insert(moeda.to_string(), v);
    } else {
        tracing::warn!("financeiro: sem cotacao {moeda}->BRL");
    }
    v
}

fn centavos(micros: i64, cot: f64) -> i64 {
    ((micros as f64 / 1_000_000.0) * cot * 100.0).round() as i64
}

async fn uma_rodada(central: &PgPool) -> Result<()> {
    if !crate::loja_google_play::configurado() {
        return Ok(());
    }
    let pendentes: Vec<(String, String)> = sqlx::query_as(
        "SELECT p.externo, p.id FROM loja_pedidos p
           LEFT JOIN loja_financeiro f ON f.externo = p.externo
          WHERE p.provedor = 'google_play' AND p.status = 'creditado'
            AND (p.externo LIKE 'GPA.%' OR p.externo LIKE 'TESTE:GPA.%')
            AND (f.externo IS NULL
                 OR (f.atualizado_em < NOW() - INTERVAL '6 hours' AND p.criado_em > NOW() - INTERVAL '30 days'))
          ORDER BY p.criado_em DESC LIMIT 200",
    )
    .fetch_all(central)
    .await?;
    if pendentes.is_empty() {
        return Ok(());
    }
    let mut cache = HashMap::new();
    for (externo, pedido) in pendentes {
        let order_id = externo.trim_start_matches(crate::loja_google_play::PREFIXO_TESTE);
        let corpo = match crate::loja_google_play::pedido_bruto(order_id).await {
            Ok(c) => c,
            Err(e) => {
                tracing::warn!("financeiro: pedido {order_id}: {e}");
                continue;
            }
        };
        let Ok(p) = serde_json::from_str::<Pedido>(&corpo) else {
            tracing::warn!("financeiro: resposta inesperada para {order_id}: {corpo}");
            continue;
        };
        let cot = cotacao(&mut cache, &p.total.moeda).await;
        sqlx::query(
            "INSERT INTO loja_financeiro (externo, pedido, pais, moeda, total_micros, imposto_micros, receita_micros,
                 estado, teste, cotacao_brl, total_brl_centavos, imposto_brl_centavos, receita_brl_centavos, atualizado_em)
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,NOW())
             ON CONFLICT (externo) DO UPDATE SET pais = $3, moeda = $4, total_micros = $5, imposto_micros = $6,
                 receita_micros = $7, estado = $8, teste = $9, cotacao_brl = $10, total_brl_centavos = $11,
                 imposto_brl_centavos = $12, receita_brl_centavos = $13, atualizado_em = NOW()",
        )
        .bind(&externo)
        .bind(&pedido)
        .bind(&p.comprador.pais)
        .bind(&p.total.moeda)
        .bind(p.total.micros())
        .bind(p.tax.micros())
        .bind(p.receita.micros())
        .bind(&p.state)
        .bind(crate::loja_google_play::e_teste(&externo))
        .bind(cot)
        .bind(centavos(p.total.micros(), cot))
        .bind(centavos(p.tax.micros(), cot))
        .bind(centavos(p.receita.micros(), cot))
        .execute(central)
        .await?;
    }
    Ok(())
}

/// Background task: one round now, then every 15 minutes — in ONE process
/// only (every channel process starts it; a Postgres advisory lock held on a
/// dedicated connection picks the one that runs, and another takes over within
/// 15 minutes if it dies).
pub fn spawn(central: PgPool) {
    tokio::spawn(async move {
        loop {
            let Ok(mut conn) = central.acquire().await else {
                tokio::time::sleep(INTERVALO).await;
                continue;
            };
            let dono: bool = sqlx::query_scalar("SELECT pg_try_advisory_lock(735190)")
                .fetch_one(&mut *conn)
                .await
                .unwrap_or(false);
            if !dono {
                drop(conn);
                tokio::time::sleep(INTERVALO).await;
                continue;
            }
            loop {
                if let Err(e) = uma_rodada(&central).await {
                    tracing::warn!("financeiro: rodada falhou: {e:#}");
                }
                tokio::time::sleep(INTERVALO).await;
                // Still holding `conn` (and the lock) across rounds.
                if sqlx::query("SELECT 1").execute(&mut *conn).await.is_err() {
                    break; // connection lost: the lock went with it
                }
            }
        }
    });
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn le_o_pedido_do_google() {
        let json = r#"{"state":"PROCESSED","buyerAddress":{"buyerCountry":"ID"},
            "total":{"currencyCode":"IDR","units":"19980"},"tax":{"currencyCode":"IDR","units":"1980"},
            "developerRevenueInBuyerCurrency":{"currencyCode":"IDR","units":"12600"}}"#;
        let p: Pedido = serde_json::from_str(json).unwrap();
        assert_eq!(p.comprador.pais, "ID");
        assert_eq!(p.total.micros(), 19_980_000_000);
        assert_eq!(p.receita.micros(), 12_600_000_000);
        let br = Dinheiro { moeda: "BRL".into(), units: None, nanos: Some(990_000_000) };
        assert_eq!(br.micros(), 990_000);
        assert_eq!(centavos(br.micros(), 1.0), 99);
    }
}
