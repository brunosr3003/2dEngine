//! Executa um único UPDATE/INSERT SQL via env var. Uso pontual pra fixes.
//!
//!   SQL='UPDATE skills SET target_type='line' WHERE id=1054' cargo run --bin sql_one
//!
//! Bumpa economy_version automaticamente pra trigger hot-reload.

use anyhow::{Context, Result};
use sqlx::postgres::PgPoolOptions;

#[tokio::main]
async fn main() -> Result<()> {
    let url = std::env::var("DATABASE_URL").unwrap_or_else(|_|
        "postgres://solar:solar_dev_123@localhost:5432/mmo_dev".into());
    let sql = std::env::var("SQL").context("env SQL não setada")?;
    let pool = PgPoolOptions::new().max_connections(2).connect(&url).await?;
    let r = sqlx::query(&sql).execute(&pool).await?;
    println!("✓ {} rows affected", r.rows_affected());
    let v: i64 = sqlx::query_scalar(
        "UPDATE economy_version SET version = version + 1, updated_at = NOW()
         WHERE id = 1 RETURNING version"
    ).fetch_one(&pool).await?;
    println!("economy_version → v{}", v);
    Ok(())
}
