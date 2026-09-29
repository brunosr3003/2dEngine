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

    sqlx::query(
        "ALTER TABLE accounts ADD COLUMN IF NOT EXISTS class TEXT NOT NULL DEFAULT 'warrior'",
    )
    .execute(&pool)
    .await?;

    // Login with Google (docs/LOGIN_GOOGLE.md). `google_sub` is the stable id of
    // the Google account; unique only among those who have one.
    sqlx::query("ALTER TABLE accounts ADD COLUMN IF NOT EXISTS google_sub TEXT")
        .execute(&pool)
        .await?;
    sqlx::query(
        "CREATE UNIQUE INDEX IF NOT EXISTS accounts_google_sub_key
             ON accounts (google_sub) WHERE google_sub IS NOT NULL",
    )
    .execute(&pool)
    .await?;
    // A session issued by the Google login and accepted by the game server
    // (`ClientMessage::LoginToken`). Stores only the token's SHA-256.
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS login_tokens (
            token_hash  TEXT PRIMARY KEY,
            account_id  BIGINT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
            expires_at  TIMESTAMPTZ NOT NULL,
            created_at  TIMESTAMPTZ NOT NULL DEFAULT NOW()
        )",
    )
    .execute(&pool)
    .await?;

    // CONFIRMACAO DE E-MAIL e RESET DE SENHA (docs/EMAIL.md).
    //
    // `TRUE` in the DEFAULT is deliberate, and only applies to the new column:
    // without it EVERY existing account would be born "unconfirmed" and nobody
    // would get into the game any more. Whoever was already in stays in; the
    // requirement applies from now on.
    sqlx::query(
        "ALTER TABLE accounts ADD COLUMN IF NOT EXISTS email_confirmado BOOLEAN NOT NULL DEFAULT TRUE",
    )
    .execute(&pool)
    .await?;

    // A single place for both link types, because they are the same thing: a
    // single-use secret, with a deadline, that proves ownership of the email.
    // `tipo` = 0 confirmation, 1 reset.
    //
    // Stores only the SHA-256, like `login_tokens`: whoever reads the table
    // cannot use what they read.
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS email_tokens (
            token_hash  TEXT PRIMARY KEY,
            account_id  BIGINT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
            tipo        SMALLINT NOT NULL,
            expires_at  TIMESTAMPTZ NOT NULL,
            usado_em    TIMESTAMPTZ,
            created_at  TIMESTAMPTZ NOT NULL DEFAULT NOW()
        )",
    )
    .execute(&pool)
    .await?;
    sqlx::query(
        "CREATE INDEX IF NOT EXISTS email_tokens_conta ON email_tokens (account_id, tipo)",
    )
    .execute(&pool)
    .await?;

    Ok(pool)
}

/// The starting username of someone entering with Google for the first time:
/// the name (or the start of the email) with unaccented letters, digits and
/// `_` only, 3 to 20. Adjustable later; a collision gets a suffix in `conta_google`.
pub fn base_de_usuario(nome: Option<&str>, email: Option<&str>) -> String {
    let fonte = nome
        .filter(|n| !n.trim().is_empty())
        .map(str::to_string)
        .or_else(|| email.and_then(|e| e.split('@').next()).map(str::to_string))
        .unwrap_or_default();
    let mut s = String::new();
    for c in fonte.chars() {
        let c = match c {
            'á' | 'à' | 'â' | 'ã' | 'ä' | 'Á' | 'À' | 'Â' | 'Ã' => 'a',
            'é' | 'ê' | 'è' | 'É' | 'Ê' => 'e',
            'í' | 'Í' => 'i',
            'ó' | 'ô' | 'õ' | 'Ó' | 'Ô' | 'Õ' => 'o',
            'ú' | 'ü' | 'Ú' => 'u',
            'ç' | 'Ç' => 'c',
            ' ' | '.' | '-' => '_',
            c => c,
        };
        if c.is_ascii_alphanumeric() || (c == '_' && !s.ends_with('_') && !s.is_empty()) {
            s.push(c);
        }
        if s.len() >= 20 {
            break;
        }
    }
    let s = s.trim_end_matches('_').to_string();
    if s.len() < 3 {
        "jogador".into()
    } else {
        s
    }
}

/// A variation of the base when the name already exists: `base_1234`, with
/// the digits taken from the `sub` and the attempt (stable, no draw).
fn usuario_com_sufixo(base: &str, sub: &str, tentativa: u32) -> String {
    let mut h: u32 = 2166136261;
    for b in sub.bytes().chain(tentativa.to_le_bytes()) {
        h = (h ^ b as u32).wrapping_mul(16777619);
    }
    let raiz: String = base.chars().take(15).collect();
    format!("{raiz}_{:04}", h % 10000)
}

/// The account of whoever signed in with Google: the one linked to the `sub`, or a new one.
///
/// Does not link an existing password account by email: password signup does
/// not verify the email, so "same email" does not prove it is the same
/// person. If the email is already in use, the new account gets a placeholder email.
pub async fn conta_google(
    pool: &PgPool,
    sub: &str,
    email: Option<&str>,
    nome: Option<&str>,
) -> Result<(i64, String)> {
    let por_sub = |p: &PgPool| {
        let sub = sub.to_string();
        let p = p.clone();
        async move {
            sqlx::query_as::<_, (i64, String)>(
                "SELECT id, username FROM accounts WHERE google_sub = $1",
            )
            .bind(sub)
            .fetch_optional(&p)
            .await
        }
    };
    if let Some(conta) = por_sub(pool).await? {
        return Ok(conta);
    }
    let base = base_de_usuario(nome, email);
    let marcador = format!("google-{sub}@google.invalid");
    let mut email_final = match email.map(|e| e.trim().to_lowercase()) {
        Some(e) if e.contains('@') => {
            let usado: bool =
                sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM accounts WHERE email = $1)")
                    .bind(&e)
                    .fetch_one(pool)
                    .await?;
            if usado {
                marcador.clone()
            } else {
                e
            }
        }
        _ => marcador.clone(),
    };
    for tentativa in 0..20u32 {
        let usuario = if tentativa == 0 {
            base.clone()
        } else {
            usuario_com_sufixo(&base, sub, tentativa)
        };
        let r = sqlx::query_as::<_, (i64,)>(
            "INSERT INTO accounts (username, email, password_hash, class, google_sub)
             VALUES ($1, $2, '!google', 'none', $3)
             ON CONFLICT (google_sub) WHERE google_sub IS NOT NULL DO NOTHING
             RETURNING id",
        )
        .bind(&usuario)
        .bind(&email_final)
        .bind(sub)
        .fetch_optional(pool)
        .await;
        match r {
            Ok(Some((id,))) => return Ok((id, usuario)),
            // Outro pedido do mesmo `sub` criou a conta ao mesmo tempo.
            Ok(None) => {
                if let Some(conta) = por_sub(pool).await? {
                    return Ok(conta);
                }
            }
            Err(sqlx::Error::Database(e)) if e.is_unique_violation() => {
                if e.constraint().is_some_and(|c| c.contains("email")) {
                    email_final = marcador.clone();
                }
                // username in use: the next round tries with a suffix.
            }
            Err(e) => return Err(e.into()),
        }
    }
    anyhow::bail!("nao achei username livre pra conta Google")
}

/// Records the Google login's session (the hash only) and clears expired ones.
pub async fn grava_token_de_login(
    pool: &PgPool,
    account_id: i64,
    token_hash: &str,
    validade_horas: i64,
) -> Result<()> {
    sqlx::query("DELETE FROM login_tokens WHERE expires_at < NOW()")
        .execute(pool)
        .await?;
    sqlx::query(
        "INSERT INTO login_tokens (token_hash, account_id, expires_at)
         VALUES ($1, $2, NOW() + make_interval(hours => $3))",
    )
    .bind(token_hash)
    .bind(account_id)
    .bind(validade_horas as i32)
    .execute(pool)
    .await?;
    Ok(())
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn username_do_google_fica_valido() {
        assert_eq!(
            base_de_usuario(Some("João da Silva"), None),
            "Joao_da_Silva"
        );
        assert_eq!(
            base_de_usuario(None, Some("maria.souza@gmail.com")),
            "maria_souza"
        );
        assert_eq!(base_de_usuario(Some("  "), Some("x@y.com")), "jogador");
        assert_eq!(base_de_usuario(Some("李小龍"), None), "jogador");
        assert!(base_de_usuario(Some("Um Nome Muito Comprido Demais Pra Caber"), None).len() <= 20);
        let s = usuario_com_sufixo("Joao_da_Silva", "123", 1);
        assert!(s.starts_with("Joao_da_Silva_") && s.len() == "Joao_da_Silva_".len() + 4);
        assert_eq!(
            s,
            usuario_com_sufixo("Joao_da_Silva", "123", 1),
            "sufixo estavel"
        );
        assert_ne!(s, usuario_com_sufixo("Joao_da_Silva", "123", 2));
    }

    /// Idempotent creation by `sub`, against a real Postgres. Only runs with
    /// `DATABASE_URL_TESTE` (e.g. the local dev database) and cleans up what it created.
    #[tokio::test]
    #[ignore]
    async fn conta_google_idempotente_no_banco() {
        let Ok(url) = std::env::var("DATABASE_URL_TESTE") else {
            return;
        };
        let pool = open_pool(&url).await.unwrap();
        let sub = format!("teste-{}", std::process::id());
        let a = conta_google(
            &pool,
            &sub,
            Some("teste-google@example.com"),
            Some("Teste Google"),
        )
        .await
        .unwrap();
        let b = conta_google(&pool, &sub, Some("outro@example.com"), Some("Outro Nome"))
            .await
            .unwrap();
        assert_eq!(a, b, "mesmo sub = mesma conta");
        grava_token_de_login(&pool, a.0, "hash-de-teste", 1)
            .await
            .unwrap();
        sqlx::query("DELETE FROM accounts WHERE google_sub = $1")
            .bind(&sub)
            .execute(&pool)
            .await
            .unwrap();
    }
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
    class: &str,
) -> Result<i64, InsertError> {
    let result = sqlx::query_as::<_, (i64,)>(
        "INSERT INTO accounts (username, email, password_hash, class)
         VALUES ($1, $2, $3, $4)
         RETURNING id",
    )
    .bind(username)
    .bind(email)
    .bind(password_hash)
    .bind(class)
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

pub async fn find_account_by_username(pool: &PgPool, username: &str) -> Result<Option<AccountRow>> {
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
