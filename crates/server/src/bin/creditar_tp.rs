//! Credits TEST TP to an account, in the central database (docs/MERCADO.md).
//!
//! By character (finds the account in the realm's database):
//!
//! DATABASE_URL_CENTRAL=… DATABASE_URL=… MMO_REALM=SA01 \
//! cargo run --bin creditar_tp -- Fulano 500 "teste do mercado"
//!
//! By the account key in the central database (`REALM:account_id`):
//!
//! DATABASE_URL_CENTRAL=… cargo run --bin creditar_tp -- --conta SA01:12 500
//!
//! Every credit is a new row in the ledger (`tp_razao`); nothing is
//! overwritten. While there is no TP shop, this is the only way TP gets in.

#[path = "../mercado_razao.rs"]
mod mercado_razao;

use anyhow::{bail, Context, Result};
use sqlx::postgres::PgPoolOptions;

#[tokio::main]
async fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (conta, resto) = match args.first().map(String::as_str) {
        Some("--conta") => (args.get(1).context("faltou a conta depois de --conta")?.clone(), &args[2.min(args.len())..]),
        Some(personagem) => (conta_do_personagem(personagem).await?, &args[1..]),
        None => bail!("uso: creditar_tp <personagem> <qtd> [motivo]  |  creditar_tp --conta REALM:ID <qtd> [motivo]"),
    };
    let qtd: u64 = resto
        .first()
        .context("faltou a quantidade")?
        .parse()
        .context("quantidade invalida")?;
    if qtd == 0 {
        bail!("quantidade precisa ser maior que zero");
    }
    let motivo = resto
        .get(1)
        .cloned()
        .unwrap_or_else(|| "TP de teste (admin)".into());
    let url = std::env::var("DATABASE_URL_CENTRAL").context("DATABASE_URL_CENTRAL nao setada")?;
    let central = PgPoolOptions::new()
        .max_connections(2)
        .connect(&url)
        .await?;
    mercado_razao::criar_tabela(&central).await?;
    match mercado_razao::creditar(&central, &conta, qtd, &motivo, None).await? {
        mercado_razao::Movimento::Feito { saldo } => {
            println!("✓ +{qtd} TP on {conta} — balance now {saldo}")
        }
        outro => println!("nothing changed: {outro:?}"),
    }
    Ok(())
}

async fn conta_do_personagem(nome: &str) -> Result<String> {
    let url = std::env::var("DATABASE_URL").context("DATABASE_URL (banco do realm) nao setada")?;
    let realm = std::env::var("MMO_REALM").unwrap_or_else(|_| "SA01".into());
    let pool = PgPoolOptions::new()
        .max_connections(1)
        .connect(&url)
        .await?;
    let conta: Option<Option<i64>> =
        sqlx::query_scalar("SELECT account_id FROM characters WHERE name = $1")
            .bind(nome)
            .fetch_optional(&pool)
            .await?;
    // The same key the server uses (`mercado::conta_global`).
    match conta {
        None => bail!("personagem '{nome}' nao existe neste realm"),
        Some(Some(id)) => Ok(format!("{realm}:{id}")),
        Some(None) => Ok(format!("{realm}:personagem:{nome}")),
    }
}
