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
    std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4)
}

#[derive(Debug)]
pub struct AuthSuccess {
    pub account_id: i64,
    pub username: String,
    /// Classe persistida (warrior/wizard/archer). Usada pra defaultar
    /// `Session.visual` quando o player loga.
    pub class: String,
    /// Sessao recem-emitida, pro cliente guardar e nao pedir senha de novo.
    ///
    /// So' vem preenchida no login por SENHA e so' quando o cliente pediu
    /// (`lembrar`). Quem entrou por token ja' tem a sua e nao ganha outra —
    /// renovar a cada troca de zona encheria a tabela de linha morta.
    pub sessao: Option<String>,
}

#[derive(Debug, Clone)]
pub enum AuthError {
    InvalidCredentials,
    /// Senha certa, e-mail ainda nao confirmado.
    NaoConfirmado,
    Internal(String),
}

/// `lembrar`: o cliente marcou "lembrar de mim". So' nesse caso uma sessao e'
/// emitida — emitir sempre gastaria uma linha de tabela por login de quem nao
/// pediu nada.
pub async fn authenticate(
    pool: &PgPool,
    username: &str,
    password: &str,
    lembrar: bool,
) -> Result<AuthSuccess, AuthError> {
    // `COALESCE` na coluna nova: o servidor pode subir contra um banco que
    // ainda nao rodou a migracao do `web` (e' ele quem cria o schema). Sem
    // isto, a consulta falharia e NINGUEM entraria.
    let row = sqlx::query_as::<_, (i64, String, String, String, bool)>(
        "SELECT id, username, password_hash, class,
                COALESCE(email_confirmado, TRUE)
           FROM accounts WHERE username = $1",
    )
    .bind(username)
    .fetch_optional(pool)
    .await;
    let row = match row {
        Ok(r) => r,
        // Coluna ainda nao existe (42703): cai pra consulta antiga em vez de
        // trancar o jogo inteiro por causa de uma migracao atrasada.
        Err(sqlx::Error::Database(e)) if e.code().as_deref() == Some("42703") => {
            sqlx::query_as::<_, (i64, String, String, String)>(
                "SELECT id, username, password_hash, class FROM accounts WHERE username = $1",
            )
            .bind(username)
            .fetch_optional(pool)
            .await
            .map_err(|e| AuthError::Internal(format!("{e:?}")))?
            .map(|(a, b, c, d)| (a, b, c, d, true))
        }
        Err(e) => return Err(AuthError::Internal(format!("{e:?}"))),
    };

    let Some((id, uname, hash, class, confirmado)) = row else {
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
        // A CONFIRMACAO e' conferida DEPOIS da senha, nunca antes.
        //
        // Dizer "confirme seu e-mail" pra quem errou a senha entregaria que
        // aquele usuario existe — e a conferencia de senha e' justamente o
        // que separa quem tem direito a essa informacao de quem nao tem.
        if !confirmado {
            return Err(AuthError::NaoConfirmado);
        }
        // A sessao sai DEPOIS de a senha conferir, nunca antes.
        let sessao = if lembrar {
            emite_sessao(pool, id).await
        } else {
            None
        };
        Ok(AuthSuccess {
            account_id: id,
            username: uname,
            class,
            sessao,
        })
    } else {
        Err(AuthError::InvalidCredentials)
    }
}

/// Quanto tempo a sessao de "lembrar de mim" vale.
///
/// Trinta dias, e nao as 12 h do login com Google: aquela e' a janela de uma
/// sessao de jogo, esta e' a promessa de nao pedir senha de novo. E' o mesmo
/// prazo que um app de banco usa pro "manter conectado", e o token vence
/// sozinho — que e' justamente a vantagem dele sobre guardar a senha.
const VALIDADE_DA_SESSAO_HORAS: i32 = 24 * 30;

/// Emite uma sessao nova pra essa conta e devolve o token EM CLARO.
///
/// O banco guarda so' o SHA-256, igual ao que o `web` faz no login com Google
/// (`crates/web/src/google.rs`): quem ler a tabela nao consegue entrar com o
/// que leu.
///
/// Os 256 bits vem do `OsRng`, e NAO do `fastrand` que o resto do servidor
/// usa. `fastrand` e' um PRNG de jogo: rapido, semeado de forma previsivel e
/// bom pra sortear dano. Quem adivinha a semente dele adivinha a sequencia
/// inteira — e aqui a sequencia E' a credencial. Mesma fonte que o `web` usa
/// no login com Google.
pub async fn emite_sessao(pool: &PgPool, account_id: i64) -> Option<String> {
    use argon2::password_hash::rand_core::{OsRng, RngCore};
    let mut bruto = [0u8; 32];
    OsRng.fill_bytes(&mut bruto);
    let token: String = bruto.iter().map(|b| format!("{b:02x}")).collect();
    // Faxina antes de inserir: sem isto a tabela so' cresce, e com 30 dias de
    // validade ela cresce por 30 dias.
    let _ = sqlx::query("DELETE FROM login_tokens WHERE expires_at < NOW()")
        .execute(pool)
        .await;
    let r = sqlx::query(
        "INSERT INTO login_tokens (token_hash, account_id, expires_at)
         VALUES ($1, $2, NOW() + make_interval(hours => $3))",
    )
    .bind(hash_do_token(&token))
    .bind(account_id)
    .bind(VALIDADE_DA_SESSAO_HORAS)
    .execute(pool)
    .await;
    match r {
        Ok(_) => Some(token),
        // A tabela e' criada pelo `web`. Sem ela, nao ha' o que lembrar — e
        // isso nao pode derrubar um login que ja' deu certo.
        Err(e) => {
            tracing::warn!("nao consegui emitir sessao: {e:?}");
            None
        }
    }
}

/// SHA-256 em hex. O `web` grava assim em `login_tokens` (mesma conta dos
/// dois lados — ver `crates/web/src/google.rs`).
pub fn hash_do_token(token: &str) -> String {
    use sha2::{Digest, Sha256};
    Sha256::digest(token.as_bytes())
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
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
        // Quem entrou POR token nao ganha outro: ele ja' tem o seu, e
        // renovar a cada troca de zona encheria a tabela de linha morta.
        Ok(Some((id, username, class))) => Ok(AuthSuccess {
            account_id: id,
            username,
            class,
            sessao: None,
        }),
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
