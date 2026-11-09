//! FLASH OFFERS: a premium bundle that pops up when a character reaches a
//! level, open for a short time (owner, 08/11/2026: "reached level 5, a flash
//! offer shows on screen, available for 1 hour, R$ 0,99 or US$ 0.99 … as you
//! level up they grow … configurable in the DB so I can release them without
//! touching the code").
//!
//! Everything lives in the CENTRAL database (same as the cash shop):
//!
//! - `ofertas_relampago` — the catalogue, edited by hand (see
//!   docs/OFERTAS_RELAMPAGO.md). One row per offer: trigger level, duration,
//!   Google Play product, price shown, TP, energy, items, on/off.
//! - `ofertas_relampago_contas` — which offer each ACCOUNT got, until when,
//!   and whether it bought it. Per account, not per character: an alt
//!   reaching level 5 doesn't reopen the R$ 0,99 deal.
//!
//! Money: only Google Play today. The purchase is checked on the Android
//! Publisher API, credited once (unique Google order id), and CONSUMED on
//! Google afterwards so the same price product can be bought again by the
//! next offer.

use anyhow::Result;
use serde::Deserialize;
use sqlx::{PgPool, Row};

use crate::mercado_razao as razao;
use shared::loja::OfertaRelampago;

/// How long after closing a paid offer is still honoured: the timer can run
/// out while the Google Play sheet is open.
const TOLERANCIA_S: i64 = 30 * 60;

pub async fn criar_tabelas(central: &PgPool) -> Result<()> {
    for sql in [
        "CREATE TABLE IF NOT EXISTS ofertas_relampago (
            id                TEXT PRIMARY KEY,
            titulo            TEXT    NOT NULL,
            titulo_pt         TEXT,
            nivel             INTEGER NOT NULL,
            duracao_min       INTEGER NOT NULL DEFAULT 60,
            produto_play      TEXT    NOT NULL,
            preco_centavos    INTEGER NOT NULL,
            valor_de_centavos INTEGER,
            tp                INTEGER NOT NULL DEFAULT 0,
            energia           BIGINT  NOT NULL DEFAULT 0,
            itens             TEXT    NOT NULL DEFAULT '[]',
            ativa             BOOLEAN NOT NULL DEFAULT TRUE,
            nota              TEXT
        )",
        "CREATE TABLE IF NOT EXISTS ofertas_relampago_contas (
            conta       TEXT        NOT NULL,
            oferta      TEXT        NOT NULL REFERENCES ofertas_relampago(id),
            aberta_em   TIMESTAMPTZ NOT NULL DEFAULT NOW(),
            expira_em   TIMESTAMPTZ NOT NULL,
            comprada_em TIMESTAMPTZ,
            pedido      TEXT,
            PRIMARY KEY (conta, oferta)
        )",
    ] {
        sqlx::query(sql).execute(central).await?;
    }
    // Starting catalogue. ON CONFLICT DO NOTHING: once the row exists the
    // database is the truth — a deploy never overwrites what the owner edited.
    // Item ids: 363 pet summon scroll, 361 mount summon scroll, 360 craft key
    // summon scroll, 345 darksteel.
    for (id, titulo, titulo_pt, nivel, produto, preco, valor_de, tp, energia, itens) in [
        (
            "nivel_5",
            "Beginner's Flash Offer",
            "Oferta Relâmpago de Iniciante",
            5,
            "oferta_099",
            99,
            Some(990),
            400,
            0i64,
            r#"[{"item_id":363,"qtd":1},{"item_id":361,"qtd":1}]"#,
        ),
        (
            "nivel_15",
            "Adventurer's Flash Offer",
            "Oferta Relâmpago de Aventureiro",
            15,
            "oferta_499",
            499,
            Some(2990),
            0,
            6_000,
            r#"[{"item_id":363,"qtd":1},{"item_id":361,"qtd":1},{"item_id":360,"qtd":2},{"item_id":345,"qtd":2000}]"#,
        ),
        (
            "nivel_25",
            "Champion's Flash Offer",
            "Oferta Relâmpago de Campeão",
            25,
            "oferta_1500",
            1500,
            Some(8990),
            0,
            24_000,
            r#"[{"item_id":363,"qtd":2},{"item_id":361,"qtd":2},{"item_id":360,"qtd":5},{"item_id":345,"qtd":6000}]"#,
        ),
    ] {
        sqlx::query(
            "INSERT INTO ofertas_relampago
               (id, titulo, titulo_pt, nivel, produto_play, preco_centavos, valor_de_centavos, tp, energia, itens)
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10)
             ON CONFLICT (id) DO NOTHING",
        )
        .bind(id)
        .bind(titulo)
        .bind(titulo_pt)
        .bind(nivel)
        .bind(produto)
        .bind(preco)
        .bind(valor_de)
        .bind(tp)
        .bind(energia)
        .bind(itens)
        .execute(central)
        .await?;
    }
    Ok(())
}

#[derive(Deserialize)]
struct ItemJson {
    item_id: u16,
    qtd: u32,
}

/// `[{"item_id":345,"qtd":2000}, …]` — a typo in the DB drops the bad entry
/// (and logs) instead of breaking the whole offer list.
fn itens_de(json: &str, oferta: &str) -> Vec<(u16, u32)> {
    match serde_json::from_str::<Vec<ItemJson>>(json) {
        Ok(v) => v.into_iter().filter(|i| i.qtd > 0).map(|i| (i.item_id, i.qtd)).collect(),
        Err(e) => {
            tracing::warn!("ofertas: itens invalidos na oferta {oferta}: {e} ({json})");
            Vec::new()
        }
    }
}

/// Opens the next offer this level unlocks (at most ONE per call, the lowest
/// level first, and only when none is open right now) and returns every offer
/// still open for the account.
pub async fn abrir_e_listar(central: &PgPool, conta: &str, nivel: u32) -> Result<Vec<OfertaRelampago>> {
    let mut nova: Option<String> = None;
    let abertas: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM ofertas_relampago_contas
          WHERE conta = $1 AND comprada_em IS NULL AND expira_em > NOW()",
    )
    .bind(conta)
    .fetch_one(central)
    .await?;
    if abertas == 0 {
        nova = sqlx::query_scalar(
            "INSERT INTO ofertas_relampago_contas (conta, oferta, expira_em)
             SELECT $1, o.id, NOW() + make_interval(mins => GREATEST(o.duracao_min, 1))
               FROM ofertas_relampago o
              WHERE o.ativa AND o.nivel <= $2
                AND NOT EXISTS (SELECT 1 FROM ofertas_relampago_contas c
                                 WHERE c.conta = $1 AND c.oferta = o.id)
              ORDER BY o.nivel, o.id
              LIMIT 1
             ON CONFLICT DO NOTHING
             RETURNING oferta",
        )
        .bind(conta)
        .bind(nivel as i32)
        .fetch_optional(central)
        .await?;
    }
    let linhas = sqlx::query(
        "SELECT o.id, o.titulo, o.titulo_pt, o.produto_play, o.preco_centavos,
                o.valor_de_centavos, o.tp, o.energia, o.itens,
                GREATEST(EXTRACT(EPOCH FROM (c.expira_em - NOW())), 0)::BIGINT
           FROM ofertas_relampago_contas c
           JOIN ofertas_relampago o ON o.id = c.oferta
          WHERE c.conta = $1 AND c.comprada_em IS NULL AND c.expira_em > NOW()
          ORDER BY c.expira_em",
    )
    .bind(conta)
    .fetch_all(central)
    .await?;
    Ok(linhas
        .into_iter()
        .map(|r| {
            let id: String = r.get(0);
            let itens: String = r.get(8);
            OfertaRelampago {
                nova: nova.as_deref() == Some(id.as_str()),
                itens: itens_de(&itens, &id),
                titulo: r.get(1),
                titulo_pt: r.get(2),
                produto_play: r.get(3),
                preco_centavos: r.get::<i32, _>(4).max(0) as u32,
                valor_de_centavos: r.get::<Option<i32>, _>(5).map(|v| v.max(0) as u32),
                tp: r.get::<i32, _>(6).max(0) as u32,
                energia: r.get::<i64, _>(7).max(0) as u64,
                segundos: r.get::<i64, _>(9).clamp(0, u32::MAX as i64) as u32,
                id,
            }
        })
        .collect())
}

/// What a paid offer hands over in the world (the TP is already in the
/// ledger when this comes back).
#[derive(Debug, Default)]
pub struct Entrega {
    pub tp: u64,
    pub energia: u64,
    pub itens: Vec<(u16, u32)>,
    pub titulo: String,
}

pub enum Compra {
    Feita(Entrega, u64),
    JaFeita(u64),
    Recusada(String),
}

/// Buys `oferta` for `conta` with a Google Play `purchase_token`.
pub async fn comprar(
    central: &PgPool,
    conta: &str,
    oferta: &str,
    pedido: &str,
    purchase_token: &str,
) -> Result<Compra> {
    if !shared::loja::pedido_valido(pedido) || purchase_token.is_empty() || purchase_token.len() > 4096 {
        return Ok(Compra::Recusada("Invalid order.".into()));
    }
    let Some(o) = sqlx::query(
        "SELECT o.produto_play, o.preco_centavos, o.tp, o.energia, o.itens, o.titulo,
                c.comprada_em IS NOT NULL, c.pedido,
                EXTRACT(EPOCH FROM (NOW() - c.expira_em))::BIGINT
           FROM ofertas_relampago_contas c
           JOIN ofertas_relampago o ON o.id = c.oferta
          WHERE c.conta = $1 AND c.oferta = $2",
    )
    .bind(conta)
    .bind(oferta)
    .fetch_optional(central)
    .await?
    else {
        return Ok(Compra::Recusada("This offer is not available.".into()));
    };
    let produto: String = o.get(0);
    let preco: i32 = o.get(1);
    let tp = o.get::<i32, _>(2).max(0) as u64;
    let energia = o.get::<i64, _>(3).max(0) as u64;
    let itens = itens_de(&o.get::<String, _>(4), oferta);
    let titulo: String = o.get(5);
    let comprada: bool = o.get(6);
    let pedido_feito: Option<String> = o.get(7);
    let passou_s: i64 = o.get(8);
    if comprada {
        // Re-sent by the client after a lost reply: same order → already done
        // (and it still needs consuming on Google).
        if pedido_feito.as_deref() == Some(pedido) {
            crate::loja_google_play::consumir_best_effort(&produto, purchase_token).await.ok();
            return Ok(Compra::JaFeita(razao::saldo(central, conta).await?));
        }
        return Ok(Compra::Recusada("You already bought this offer.".into()));
    }
    if passou_s > TOLERANCIA_S {
        return Ok(Compra::Recusada("This offer has expired.".into()));
    }
    let externo = match crate::loja_google_play::verificar_produto(&produto, purchase_token).await {
        Ok(x) => x,
        Err(e) => {
            tracing::warn!("ofertas: compra recusada ({conta} {oferta}): {e}");
            return Ok(Compra::Recusada(e.to_string()));
        }
    };
    let mut tx = central.begin().await?;
    let repetido: Option<String> = sqlx::query_scalar(
        "SELECT id FROM loja_pedidos WHERE provedor = 'google_play' AND externo = $1 AND status = 'creditado'",
    )
    .bind(&externo)
    .fetch_optional(&mut *tx)
    .await?;
    if repetido.is_some() {
        return Ok(Compra::Recusada("This purchase has already been used.".into()));
    }
    let marcou = sqlx::query(
        "UPDATE ofertas_relampago_contas SET comprada_em = NOW(), pedido = $3
          WHERE conta = $1 AND oferta = $2 AND comprada_em IS NULL",
    )
    .bind(conta)
    .bind(oferta)
    .bind(pedido)
    .execute(&mut *tx)
    .await?
    .rows_affected();
    if marcou != 1 {
        return Ok(Compra::Recusada("You already bought this offer.".into()));
    }
    sqlx::query(
        "INSERT INTO loja_pedidos (id, conta, produto, tipo, valor, moeda, status, provedor, externo)
         VALUES ($1, $2, $3, 'oferta', $4, 'BRL_CENTAVOS', 'creditado', 'google_play', $5)",
    )
    .bind(pedido)
    .bind(conta)
    .bind(format!("oferta:{oferta}"))
    .bind(preco as i64)
    .bind(&externo)
    .execute(&mut *tx)
    .await?;
    let saldo = if tp > 0 {
        match razao::mover(&mut tx, conta, tp as i64, "loja:oferta", Some(&format!("loja:{pedido}"))).await? {
            razao::Movimento::Feito { saldo }
            | razao::Movimento::JaFeito { saldo }
            | razao::Movimento::SemSaldo { saldo } => saldo,
        }
    } else {
        razao::saldo(&mut *tx, conta).await?
    };
    tx.commit().await?;
    if let Err(e) = crate::loja_google_play::consumir_best_effort(&produto, purchase_token).await {
        tracing::warn!("ofertas: consume no Google falhou (ja' entregue): {e}");
    }
    Ok(Compra::Feita(Entrega { tp, energia, itens, titulo }, saldo))
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn itens_do_banco_aceitam_lista_e_ignoram_erro() {
        assert_eq!(
            itens_de(r#"[{"item_id":345,"qtd":2000},{"item_id":363,"qtd":0}]"#, "x"),
            vec![(345, 2000)]
        );
        assert!(itens_de("not json", "x").is_empty());
    }
}
