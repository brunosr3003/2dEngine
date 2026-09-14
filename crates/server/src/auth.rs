//! Verificacao de login contra a tabela `accounts` (argon2id).
//!
//! Roda assincrono (sqlx + blocking argon2) em task separada pra nao
//! travar o tick loop. O resultado volta via `IncomingMessage::AuthResult`.

use std::sync::OnceLock;

use anyhow::Result;
use argon2::{password_hash::PasswordHash, Argon2, PasswordVerifier};
use sqlx::postgres::PgPool;
use tokio::sync::Semaphore;

/// Fila de login.
///
/// argon2 e' caro DE PROPOSITO — e' isso que protege a senha. Medido: durante
/// a entrada de 200 jogadores o servidor foi a 113% de CPU, contra ~21% em
/// regime. Sem limite, uma onda de login (abertura de servidor, queda de rede,
/// evento) rouba o processador do world loop e o jogo trava pra quem JA esta
/// dentro.
///
/// A fila troca latencia de quem entra por estabilidade de quem joga: no pior
/// caso o jogador espera alguns segundos na tela de login, que e' o lugar
/// certo pra esperar.
///
/// `MMO_LOGIN_PARALELO` ajusta; o default deixa metade dos nucleos livre.
fn fila() -> &'static Semaphore {
    static FILA: OnceLock<Semaphore> = OnceLock::new();
    FILA.get_or_init(|| {
        let n = std::env::var("MMO_LOGIN_PARALELO")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or_else(|| (num_cpus().max(2) / 2).max(1));
        tracing::info!("fila de login: {n} verificacoes em paralelo");
        Semaphore::new(n)
    })
}

fn num_cpus() -> usize {
    std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4)
}

#[derive(Debug)]
pub struct AuthSuccess {
    pub account_id: i64,
    pub username: String,
    /// Classe persistida (warrior/wizard/archer). Usada pra defaultar
    /// `Session.visual` quando o player loga.
    pub class: String,
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
    let row = sqlx::query_as::<_, (i64, String, String, String)>(
        "SELECT id, username, password_hash, class FROM accounts WHERE username = $1",
    )
    .bind(username)
    .fetch_optional(pool)
    .await
    .map_err(|e| AuthError::Internal(format!("{e:?}")))?;

    let Some((id, uname, hash, class)) = row else {
        return Err(AuthError::InvalidCredentials);
    };

    // argon2 verify e CPU-bound; roda em blocking pool pra nao segurar o
    // runtime de io — e passa pela fila, pra nao roubar o processador do
    // world loop numa onda de login.
    let _vaga = fila()
        .acquire()
        .await
        .map_err(|e| AuthError::Internal(format!("fila: {e}")))?;
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
        Ok(AuthSuccess { account_id: id, username: uname, class })
    } else {
        Err(AuthError::InvalidCredentials)
    }
}

/// SHA-256 em hex. O `web` grava assim em `login_tokens` (mesma conta dos
/// dois lados — ver `crates/web/src/google.rs`).
pub fn hash_do_token(token: &str) -> String {
    use sha2::{Digest, Sha256};
    Sha256::digest(token.as_bytes()).iter().map(|b| format!("{b:02x}")).collect()
}

/// Login pela sessao que o login com Google emitiu (docs/LOGIN_GOOGLE.md).
///
/// Sem argon2: o token e' aleatorio de 256 bits, conferido pelo hash, entao nao
/// passa pela fila de login. Vale ate' vencer — o cliente reusa na troca de
/// zona, como reusa a senha hoje.
pub async fn authenticate_token(pool: &PgPool, token: &str) -> Result<AuthSuccess, AuthError> {
    if !(20..=128).contains(&token.len()) {
        return Err(AuthError::InvalidCredentials);
    }
    let row = sqlx::query_as::<_, (i64, String, String)>(
        "SELECT a.id, a.username, a.class
           FROM login_tokens t JOIN accounts a ON a.id = t.account_id
          WHERE t.token_hash = $1 AND t.expires_at > NOW()",
    )
    .bind(hash_do_token(token))
    .fetch_optional(pool)
    .await;
    match row {
        Ok(Some((id, username, class))) => Ok(AuthSuccess { account_id: id, username, class }),
        Ok(None) => Err(AuthError::InvalidCredentials),
        // Tabela ainda nao criada (o `web` e' quem cria): nao ha' sessao valida.
        Err(sqlx::Error::Database(e)) if e.code().as_deref() == Some("42P01") => {
            Err(AuthError::InvalidCredentials)
        }
        Err(e) => Err(AuthError::Internal(format!("{e:?}"))),
    }
}

#[cfg(test)]
mod testes {
    #[test]
    fn hash_do_token_e_sha256_hex() {
        assert_eq!(
            super::hash_do_token("abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }
}
