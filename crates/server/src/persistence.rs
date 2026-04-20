//! Persistencia de personagens em Postgres.
//!
//! Arquitetura:
//! - Server abre pool Postgres no startup, roda schema, carrega todos os
//!   personagens pra memoria.
//! - O tick loop le/escreve no cache sincronamente (sem locks complicados).
//! - Uma task background recebe `SaveBatch` via mpsc e escreve no DB, nunca
//!   bloqueando o tick.
//!
//! Fase 4 minimo: persiste nome, posicao e HP. Inventario/XP vem depois.

use anyhow::Result;
use glam::Vec2;
use shared::Health;
use sqlx::postgres::{PgPool, PgPoolOptions};
use std::collections::HashMap;
use tokio::sync::mpsc;

#[derive(Debug, Clone)]
pub struct CharacterRow {
    pub name: String,
    pub pos: Vec2,
    pub hp: Health,
    pub xp: u64,
}

/// Abre o pool Postgres, garante schema criado.
pub async fn open_pool(database_url: &str) -> Result<PgPool> {
    let pool = PgPoolOptions::new()
        .max_connections(8)
        .connect(database_url)
        .await?;

    // `accounts` e mantida pelo crate `web`; aqui so garantimos que existe
    // (idempotente) para o caso do game server subir antes do web.
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS accounts (
            id             BIGSERIAL PRIMARY KEY,
            username       TEXT NOT NULL UNIQUE,
            email          TEXT NOT NULL UNIQUE,
            password_hash  TEXT NOT NULL,
            created_at     TIMESTAMPTZ NOT NULL DEFAULT NOW()
        )",
    )
    .execute(&pool)
    .await?;

    sqlx::query(
        "CREATE TABLE IF NOT EXISTS characters (
            name     TEXT PRIMARY KEY,
            x        REAL NOT NULL,
            y        REAL NOT NULL,
            hp       INTEGER NOT NULL,
            max_hp   INTEGER NOT NULL,
            updated  BIGINT NOT NULL
        )",
    )
    .execute(&pool)
    .await?;

    // Migracao inline: colunas de progressao. IF NOT EXISTS para idempotencia.
    sqlx::query("ALTER TABLE characters ADD COLUMN IF NOT EXISTS xp BIGINT NOT NULL DEFAULT 0")
        .execute(&pool)
        .await?;

    Ok(pool)
}

pub async fn load_all(pool: &PgPool) -> Result<HashMap<String, CharacterRow>> {
    let rows = sqlx::query_as::<_, (String, f32, f32, i32, i32, i64)>(
        "SELECT name, x, y, hp, max_hp, xp FROM characters",
    )
    .fetch_all(pool)
    .await?;
    let mut out = HashMap::with_capacity(rows.len());
    for (name, x, y, hp, max_hp, xp) in rows {
        out.insert(
            name.clone(),
            CharacterRow {
                name,
                pos: Vec2::new(x, y),
                hp: Health { current: hp, max: max_hp },
                xp: xp.max(0) as u64,
            },
        );
    }
    Ok(out)
}

/// Mensagem enviada pela thread do mundo pro writer task.
#[derive(Debug)]
pub struct SaveBatch {
    pub rows: Vec<CharacterRow>,
}

/// Spawn de task background que consome SaveBatch e escreve no DB.
/// Consome o pool — se precisar acessar DB em outros lugares, clone antes.
pub fn spawn_writer(pool: PgPool) -> mpsc::UnboundedSender<SaveBatch> {
    let (tx, mut rx) = mpsc::unbounded_channel::<SaveBatch>();
    tokio::spawn(async move {
        while let Some(batch) = rx.recv().await {
            if let Err(e) = write_batch(&pool, &batch).await {
                tracing::warn!("persist write failed: {e:?}");
            }
        }
        tracing::debug!("persist writer task exiting");
    });
    tx
}

async fn write_batch(pool: &PgPool, batch: &SaveBatch) -> Result<()> {
    if batch.rows.is_empty() {
        return Ok(());
    }
    let mut tx = pool.begin().await?;
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    for row in &batch.rows {
        sqlx::query(
            "INSERT INTO characters (name, x, y, hp, max_hp, xp, updated)
             VALUES ($1, $2, $3, $4, $5, $6, $7)
             ON CONFLICT(name) DO UPDATE SET
               x = EXCLUDED.x,
               y = EXCLUDED.y,
               hp = EXCLUDED.hp,
               max_hp = EXCLUDED.max_hp,
               xp = EXCLUDED.xp,
               updated = EXCLUDED.updated",
        )
        .bind(&row.name)
        .bind(row.pos.x)
        .bind(row.pos.y)
        .bind(row.hp.current)
        .bind(row.hp.max)
        .bind(row.xp as i64)
        .bind(now)
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;
    Ok(())
}
