//! Verificacao de login contra a tabela `accounts` (argon2id).
//!
//! Roda assincrono (sqlx + blocking argon2) em task separada pra nao
//! travar o tick loop. O resultado volta via `IncomingMessage::AuthResult`.

use anyhow::Result;
use argon2::{password_hash::PasswordHash, Argon2, PasswordVerifier};
use sqlx::postgres::PgPool;

#[derive(Debug)]
pub struct AuthSuccess {
    pub account_id: i64,
    pub username: String,
}

#[derive(Debug, Clone)]
pub enum AuthError {
    InvalidCredentials,
    Internal(String),
}

pub async fn authenticate(
    pool: &PgPool,
    username: &str,
    password: &str,
) -> Result<AuthSuccess, AuthError> {
    let row = sqlx::query_as::<_, (i64, String, String)>(
        "SELECT id, username, password_hash FROM accounts WHERE username = $1",
    )
    .bind(username)
    .fetch_optional(pool)
    .await
    .map_err(|e| AuthError::Internal(format!("{e:?}")))?;

    let Some((id, uname, hash)) = row else {
        return Err(AuthError::InvalidCredentials);
    };

    // argon2 verify e CPU-bound; roda em blocking pool pra nao segurar o
    // runtime de io.
    let password = password.to_string();
    let ok = tokio::task::spawn_blocking(move || {
        let parsed = match PasswordHash::new(&hash) {
            Ok(p) => p,
            Err(_) => return false,
        };
        Argon2::default()
            .verify_password(password.as_bytes(), &parsed)
            .is_ok()
    })
    .await
    .map_err(|e| AuthError::Internal(format!("join: {e}")))?;

    if ok {
        Ok(AuthSuccess { account_id: id, username: uname })
    } else {
        Err(AuthError::InvalidCredentials)
    }
}
