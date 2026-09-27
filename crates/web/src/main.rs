//! Servidor web de autenticação/cadastro.
//!
//! Expõe:
//!   POST /api/register   { username, email, password } -> 201 | 409
//!   POST /api/login      { username, password }        -> 200 | 401
//! Serve os arquivos estaticos do frontend (Vite build) em /.
//!
//! Compartilha o mesmo Postgres usado pelo servidor do jogo (tabela
//! `accounts` para contas; `characters` continua sendo do server).

mod auth;
mod db;
mod contas;
mod econ_admin;
mod email;
mod google;
mod pixel;

use anyhow::Result;
use axum::{
    extract::State,
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use sqlx::postgres::PgPool;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use tower_http::{cors::CorsLayer, services::ServeDir, trace::TraceLayer};

#[derive(Clone)]
struct AppState {
    pool: Arc<PgPool>,
    /// `None` = envio desligado. O cadastro continua criando conta; só não
    /// manda o link de confirmação.
    email: Option<email::Email>,
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info,web=debug,tower_http=info".into()),
        )
        .init();

    let database_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://solar:solar_dev_123@localhost:5432/mmo_dev".to_string());
    let bind_addr: SocketAddr = std::env::var("WEB_BIND")
        .unwrap_or_else(|_| "0.0.0.0:8080".to_string())
        .parse()?;
    let static_dir: PathBuf = std::env::var("WEB_STATIC")
        .unwrap_or_else(|_| "web/dist".to_string())
        .into();

    let pool = db::open_pool(&database_url).await?;
    tracing::info!("db ok");

    let http = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()?;
    let mail = email::Email::do_ambiente(http.clone());
    tracing::info!(
        "e-mail: {}",
        if mail.is_some() {
            "configurado (Resend)"
        } else {
            "desligado (sem RESEND_API_KEY) — cadastro nao manda confirmacao"
        }
    );

    let state = AppState {
        pool: Arc::new(pool),
        email: mail.clone(),
    };
    let pool_arc = state.pool.clone();

    let api = Router::new()
        .route("/register", post(register))
        .route("/login", post(login))
        .route("/version", get(version))
        .route("/client-release", get(client_release))
        .route("/channels", get(channels))
        .with_state(state);

    let pixel_state = pixel::PixelState::from_env();
    tracing::info!(
        "pixel editor: gemini_key={}",
        if pixel_state.gemini_key.is_some() {
            "configurada"
        } else {
            "NAO configurada"
        }
    );

    let google_state = google::GoogleState::from_env(pool_arc.clone());
    tracing::info!(
        "login com Google: {}",
        if google_state.ligado() {
            "configurado"
        } else {
            "desligado (sem GOOGLE_CLIENT_ID/SECRET)"
        }
    );

    let pool_arc2 = pool_arc.clone();
    let econ_state = econ_admin::EconState::from_env(pool_arc);
    tracing::info!("econ admin: pronto (POST /api/econ/login)");

    let app = Router::new()
        .nest("/api", api)
        .nest("/api/auth/google", google::router(google_state))
        .nest(
            "/api/auth",
            Router::new()
                .route("/confirmar", get(contas::confirmar))
                .route("/esqueci", post(contas::esqueci))
                .route("/reset", get(contas::reset_form).post(contas::reset))
                .with_state(contas::Contas {
                    pool: (*pool_arc2).clone(),
                    email: mail,
                }),
        )
        .nest("/api/pixel", pixel::router(pixel_state))
        .nest("/api/econ", econ_admin::router(econ_state))
        .fallback_service(ServeDir::new(&static_dir).append_index_html_on_directories(true))
        .layer(CorsLayer::permissive())
        .layer(TraceLayer::new_for_http());

    let listener = tokio::net::TcpListener::bind(&bind_addr).await?;
    tracing::info!(
        "web listening on http://{bind_addr} (static: {})",
        static_dir.display()
    );
    axum::serve(listener, app).await?;
    Ok(())
}

// ── endpoints ─────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct RegisterReq {
    username: String,
    email: String,
    password: String,
    /// Campo legado, ignorado (classes foram removidas do jogo).
    #[serde(default)]
    class: Option<String>,
}

#[derive(Debug, Serialize)]
struct RegisterRes {
    id: i64,
    username: String,
}

#[derive(Debug, Serialize)]
struct ErrorRes {
    error: String,
}

fn err(status: StatusCode, msg: impl Into<String>) -> (StatusCode, Json<ErrorRes>) {
    (status, Json(ErrorRes { error: msg.into() }))
}

async fn register(
    State(s): State<AppState>,
    Json(req): Json<RegisterReq>,
) -> Result<(StatusCode, Json<RegisterRes>), (StatusCode, Json<ErrorRes>)> {
    let username = req.username.trim();
    let email = req.email.trim().to_lowercase();
    if username.is_empty() || username.len() > 32 {
        return Err(err(StatusCode::BAD_REQUEST, "username deve ter 1-32 chars"));
    }
    if !email.contains('@') || email.len() > 254 {
        return Err(err(StatusCode::BAD_REQUEST, "email invalido"));
    }
    if req.password.len() < 6 || req.password.len() > 128 {
        return Err(err(StatusCode::BAD_REQUEST, "senha deve ter 6-128 chars"));
    }
    // Class é campo legado — mantido na coluna só pra satisfazer o DEFAULT da
    // coluna existente. Pode ser removido numa migration futura.
    let _ = &req.class;

    let hash = auth::hash_password(&req.password)
        .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, format!("hash: {e}")))?;

    match db::insert_account(&s.pool, username, &email, &hash, "none").await {
        Ok(id) => {
            // A CONFIRMACAO sai aqui, e a falha dela NAO derruba o cadastro.
            //
            // A conta ja' existe no banco neste ponto; responder erro faria o
            // jogador tentar de novo e bater em "username ja existe", sem
            // conta nenhuma na mao dele. Quem nao recebeu pede de novo pela
            // tela de login.
            // DOMÍNIO DE TESTE: conta de bot nasce confirmada.
            //
            // O `jogadorbot` é um cliente de verdade — ele não mexe no banco,
            // passa por `/api/register` como o jogo. Mas confirmar e-mail
            // exige ler uma CAIXA DE ENTRADA, que é fora de banda por
            // natureza, e um bot não tem uma.
            //
            // A saída é o servidor conhecer um domínio cujas contas já nascem
            // confirmadas, declarado em `EMAIL_DOMINIOS_AUTOCONFIRMA`. Fica
            // explícito, fica no ambiente (não no código), e não abre atalho
            // nenhum pro bot: o caminho dele continua idêntico ao do jogador.
            //
            // Sem a variável, nada é autoconfirmado — produção de verdade não
            // pode herdar isto por esquecimento.
            let autoconfirma = std::env::var("EMAIL_DOMINIOS_AUTOCONFIRMA")
                .ok()
                .map(|v| {
                    v.split(',')
                        .map(|d| d.trim().to_lowercase())
                        .filter(|d| !d.is_empty())
                        .any(|d| email.ends_with(&format!("@{d}")))
                })
                .unwrap_or(false);
            if autoconfirma {
                tracing::info!("conta {id} ({email}) autoconfirmada: domínio de teste");
            } else if let Some(mail) = s.email.as_ref() {
                match contas::emite(&s.pool, id, contas::TIPO_CONFIRMACAO).await {
                    Ok(t) => {
                        if let Err(e) = mail.confirmacao(&email, username, &t).await {
                            tracing::warn!("cadastro {id}: confirmacao nao saiu: {e:?}");
                        }
                    }
                    Err(e) => tracing::error!("cadastro {id}: emitir token: {e:?}"),
                }
                // Conta nova com envio ligado nasce NAO confirmada. Sem envio
                // ela nasce confirmada (o DEFAULT da coluna), senao ligar o
                // e-mail depois trancaria quem entrou antes.
                let _ = sqlx::query("UPDATE accounts SET email_confirmado = FALSE WHERE id = $1")
                    .bind(id)
                    .execute(&*s.pool)
                    .await;
            }
            Ok((
                StatusCode::CREATED,
                Json(RegisterRes {
                    id,
                    username: username.into(),
                }),
            ))
        }
        Err(db::InsertError::Duplicate(field)) => {
            Err(err(StatusCode::CONFLICT, format!("{field} ja existe")))
        }
        Err(db::InsertError::Other(e)) => {
            tracing::error!("register db err: {e:?}");
            Err(err(StatusCode::INTERNAL_SERVER_ERROR, "erro interno"))
        }
    }
}

#[derive(Debug, Deserialize)]
struct LoginReq {
    username: String,
    password: String,
}

#[derive(Debug, Serialize)]
struct LoginRes {
    id: i64,
    username: String,
}

async fn login(
    State(s): State<AppState>,
    Json(req): Json<LoginReq>,
) -> Result<Json<LoginRes>, (StatusCode, Json<ErrorRes>)> {
    let row = db::find_account_by_username(&s.pool, &req.username)
        .await
        .map_err(|e| {
            tracing::error!("login db err: {e:?}");
            err(StatusCode::INTERNAL_SERVER_ERROR, "erro interno")
        })?;
    let row = row.ok_or_else(|| err(StatusCode::UNAUTHORIZED, "credenciais invalidas"))?;
    let ok = auth::verify_password(&row.password_hash, &req.password).unwrap_or(false);
    if !ok {
        return Err(err(StatusCode::UNAUTHORIZED, "credenciais invalidas"));
    }
    Ok(Json(LoginRes {
        id: row.id,
        username: row.username,
    }))
}

// ── version endpoint ──────────────────────────────────────────────────────

/// O manifesto só avança depois que os downloads foram publicados.
async fn client_release() -> Result<Json<serde_json::Value>, axum::http::StatusCode> {
    let pasta = std::env::var("WEB_STATIC").unwrap_or_else(|_| "static".into());
    let texto = std::fs::read_to_string(std::path::Path::new(&pasta).join("releases.json"))
        .map_err(|_| axum::http::StatusCode::SERVICE_UNAVAILABLE)?;
    serde_json::from_str(&texto).map(Json)
        .map_err(|_| axum::http::StatusCode::SERVICE_UNAVAILABLE)
}

#[derive(Debug, Serialize)]
struct VersionRes {
    /// Protocol version atual do servidor (espelho de shared::PROTOCOL_VERSION).
    /// Cliente compara contra a sua propria const pra detectar mismatch antes
    /// de tentar conectar no game server (preflight).
    protocol_version: u16,
    /// Versao "marketing" do cliente — opcional, pode usar pra changelog UI.
    client_version: String,
    /// URLs de download por plataforma. Cliente escolhe baseado em
    /// Application.platform. Mobile (Android/iOS) tipicamente vai pra
    /// store/TestFlight; desktop puxa o zip e auto-aplica.
    downloads: VersionDownloads,
}

#[derive(Debug, Serialize)]
struct VersionDownloads {
    win: String,
    mac: String,
    linux: String,
    android: String,
    ios_testflight: String,
}

async fn version() -> Json<VersionRes> {
    let base = std::env::var("DOWNLOAD_BASE_URL")
        .unwrap_or_else(|_| "https://mmo.brunji.com.br/downloads".to_string());
    // Mobile distribuido via stores — auto-update e gerenciado pelo Play
    // Store / TestFlight, cliente so abre o URL.
    let android = std::env::var("ANDROID_URL").unwrap_or_else(|_| {
        "https://play.google.com/store/apps/details?id=com.brunji.tempest".to_string()
    });
    let testflight = std::env::var("TESTFLIGHT_URL")
        .unwrap_or_else(|_| "https://mmo.brunji.com.br/downloads/ios-testflight.html".to_string());
    Json(VersionRes {
        protocol_version: shared::PROTOCOL_VERSION,
        client_version: env!("CARGO_PKG_VERSION").to_string(),
        downloads: VersionDownloads {
            win: format!("{base}/MMORPG-Windows.zip"),
            mac: format!("{base}/MMORPG-Mac.zip"),
            linux: format!("{base}/MMORPG-Linux.zip"),
            android,
            ios_testflight: testflight,
        },
    })
}

// ── response helpers ──────────────────────────────────────────────────────

impl IntoResponse for RegisterRes {
    fn into_response(self) -> axum::response::Response {
        Json(self).into_response()
    }
}

/// Lista de canais vivos, do mais vazio pro mais cheio.
///
/// A vivacidade vem do `updated`: quem parou de bater o coracao ha mais de 15s
/// nao entra na lista. Servidor que caiu deixa de ser oferecido sozinho, sem
/// ninguem precisar limpar a tabela.
///
/// A COLONIA fica de fora (docs/COLONIA.md): ela nao e' um lugar que se
/// escolhe na lista, e' pra onde o porto te manda. Ela continua no diretorio
/// interno (`canais::Diretorio`), que e' quem o `TrocarZona` consulta.
async fn channels(State(st): State<AppState>) -> impl IntoResponse {
    let rows = sqlx::query_as::<_, (String, String, i32, i32, String, String, bool, f32)>(
        "SELECT id, host, players, capacity, map_name, zone, single, tick_p99_ms
           FROM channels
          WHERE updated > NOW() - INTERVAL '15 seconds'
            AND zone <> 'colonia'
          ORDER BY players ASC, id ASC",
    )
    .fetch_all(&*st.pool)
    .await
    .unwrap_or_default();

    let lista: Vec<serde_json::Value> = rows
        .into_iter()
        .map(
            |(id, host, players, capacity, map_name, zone, single, tick_ms)| {
                serde_json::json!({
                    "id": id,
                    "host": host,
                    "players": players,
                    "capacity": capacity,
                    "map": map_name,
                    "zone": zone,
                    "full": capacity > 0 && players >= capacity,
                    // Instancia unica: nao vai abrir outro canal, quem chega cheio
                    // entra na fila. O cliente diz isso na tela.
                    "single": single,
                    // Saude, nao lotacao: p99 do trabalho por tick e quanto isso
                    // consome do orcamento de 33ms. Canal pode estar com meia
                    // lotacao e ja' sem folga.
                    "tick_ms": ((tick_ms as f64) * 100.0).round() / 100.0,
                    "tick_load": ((tick_ms as f64) / 33.33 * 10000.0).round() / 100.0,
                })
            },
        )
        .collect();
    axum::Json(serde_json::json!({ "channels": lista }))
}
