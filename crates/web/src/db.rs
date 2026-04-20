//! Acesso ao Postgres: schema + CRUD de `accounts`.

use anyhow::Result;
use sqlx::postgres::{PgPool, PgPoolOptions};

pub async fn open_pool(database_url: &str) -> Result<PgPool> {
    let pool = PgPoolOptions::new()
        .max_connections(8)
        .connect(database_url)
        .await?;

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

    Ok(pool)
}

#[derive(Debug, thiserror::Error)]
pub enum InsertError {
    #[error("duplicate {0}")]
    Duplicate(&'static str),
    #[error(transparent)]
    Other(#[from] sqlx::Error),
}

pub async fn insert_account(
    pool: &PgPool,
    username: &str,
    email: &str,
    password_hash: &str,
) -> Result<i64, InsertError> {
    let result = sqlx::query_as::<_, (i64,)>(
        "INSERT INTO accounts (username, email, password_hash)
         VALUES ($1, $2, $3)
         RETURNING id",
    )
    .bind(username)
    .bind(email)
    .bind(password_hash)
    .fetch_one(pool)
    .await;

    match result {
        Ok((id,)) => Ok(id),
        Err(sqlx::Error::Database(err)) if err.is_unique_violation() => {
            let field = match err.constraint() {
                Some(c) if c.contains("username") => "username",
                Some(c) if c.contains("email") => "email",
                _ => "username ou email",
            };
            Err(InsertError::Duplicate(field))
        }
        Err(e) => Err(InsertError::Other(e)),
    }
}

pub struct AccountRow {
    pub id: i64,
    pub username: String,
    pub password_hash: String,
}

pub async fn find_account_by_username(
    pool: &PgPool,
    username: &str,
) -> Result<Option<AccountRow>> {
    let row = sqlx::query_as::<_, (i64, String, String)>(
        "SELECT id, username, password_hash FROM accounts WHERE username = $1",
    )
    .bind(username)
    .fetch_optional(pool)
    .await?;
    Ok(row.map(|(id, username, password_hash)| AccountRow {
        id,
        username,
        password_hash,
    }))
}
