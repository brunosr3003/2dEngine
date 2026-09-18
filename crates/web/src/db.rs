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

    // Login com Google (docs/LOGIN_GOOGLE.md). `google_sub` e' o id estavel da
    // conta Google; unico so' entre quem tem.
    sqlx::query("ALTER TABLE accounts ADD COLUMN IF NOT EXISTS google_sub TEXT")
        .execute(&pool)
        .await?;
    sqlx::query(
        "CREATE UNIQUE INDEX IF NOT EXISTS accounts_google_sub_key
             ON accounts (google_sub) WHERE google_sub IS NOT NULL",
    )
    .execute(&pool)
    .await?;
    // Sessao emitida pelo login com Google e aceita pelo servidor de jogo
    // (`ClientMessage::LoginToken`). Guarda so' o SHA-256 do token.
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

    Ok(pool)
}

/// Username inicial de quem entra pela primeira vez com Google: o nome (ou o
/// comeco do e-mail) so' com letras sem acento, numeros e `_`, de 3 a 20.
/// Ajustavel depois; colisao ganha sufixo em `conta_google`.
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

/// Variacao da base quando o nome ja' existe: `base_1234`, com os digitos
/// tirados do `sub` e da tentativa (estavel, sem sorteio).
fn usuario_com_sufixo(base: &str, sub: &str, tentativa: u32) -> String {
    let mut h: u32 = 2166136261;
    for b in sub.bytes().chain(tentativa.to_le_bytes()) {
        h = (h ^ b as u32).wrapping_mul(16777619);
    }
    let raiz: String = base.chars().take(15).collect();
    format!("{raiz}_{:04}", h % 10000)
}

/// A conta de quem entrou com Google: a vinculada ao `sub`, ou uma nova.
///
/// Nao vincula por e-mail a conta de senha existente: o cadastro por senha nao
/// verifica e-mail, entao "mesmo e-mail" nao prova que e' a mesma pessoa. Se o
/// e-mail ja' esta' em uso, a conta nova fica com um e-mail marcador.
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
                // username em uso: a proxima volta tenta com sufixo.
            }
            Err(e) => return Err(e.into()),
        }
    }
    anyhow::bail!("nao achei username livre pra conta Google")
}

/// Grava a sessao do login com Google (so' o hash) e limpa as vencidas.
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

    /// Criacao idempotente pelo `sub`, contra um Postgres de verdade. So' roda
    /// com `DATABASE_URL_TESTE` (ex. o banco local de dev) e limpa o que criou.
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
