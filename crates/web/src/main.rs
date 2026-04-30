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
mod econ_admin;
mod pixel;

use anyhow::Result;
use axum::{
    extract::State,
    http::StatusCode,
    response::IntoResponse,
    routing::post,
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
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info,web=debug,tower_http=info".into()),
        )
        .init();

    let database_url = std::env::var("DATABASE_URL").unwrap_or_else(|_| {
        "postgres://solar:solar_dev_123@localhost:5432/mmo_dev".to_string()
    });
    let bind_addr: SocketAddr = std::env::var("WEB_BIND")
        .unwrap_or_else(|_| "0.0.0.0:8080".to_string())
        .parse()?;
    let static_dir: PathBuf = std::env::var("WEB_STATIC")
        .unwrap_or_else(|_| "web/dist".to_string())
        .into();

    let pool = db::open_pool(&database_url).await?;
    tracing::info!("db ok");

    let state = AppState { pool: Arc::new(pool) };
    let pool_arc = state.pool.clone();

    let api = Router::new()
        .route("/register", post(register))
        .route("/login", post(login))
        .with_state(state);

    let pixel_state = pixel::PixelState::from_env();
    tracing::info!("pixel editor: gemini_key={}",
        if pixel_state.gemini_key.is_some() { "configurada" } else { "NAO configurada" });

    let econ_state = econ_admin::EconState::from_env(pool_arc);
    tracing::info!("econ admin: pronto (POST /api/econ/login)");

    let app = Router::new()
        .nest("/api", api)
        .nest("/api/pixel", pixel::router(pixel_state))
        .nest("/api/econ", econ_admin::router(econ_state))
        .fallback_service(ServeDir::new(&static_dir).append_index_html_on_directories(true))
        .layer(CorsLayer::permissive())
        .layer(TraceLayer::new_for_http());

    let listener = tokio::net::TcpListener::bind(&bind_addr).await?;
    tracing::info!("web listening on http://{bind_addr} (static: {})", static_dir.display());
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
        Ok(id) => Ok((StatusCode::CREATED, Json(RegisterRes { id, username: username.into() }))),
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
    let ok = auth::verify_password(&row.password_hash, &req.password)
        .unwrap_or(false);
    if !ok {
        return Err(err(StatusCode::UNAUTHORIZED, "credenciais invalidas"));
    }
    Ok(Json(LoginRes { id: row.id, username: row.username }))
}

// ── response helpers ──────────────────────────────────────────────────────

impl IntoResponse for RegisterRes {
    fn into_response(self) -> axum::response::Response {
        Json(self).into_response()
    }
}
