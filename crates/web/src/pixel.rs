//! Pixel-art editor API — geração via Gemini + salvar PNG em assets.
//!
//! Protegido por senha casual (ferramenta dev local). Senha configurada
//! via env `PIXEL_ADMIN_PASSWORD` ou fallback embutido (DEV).
//!
//! Endpoints:
//!   POST /api/pixel/auth          { password } -> 200 + cookie pix_auth
//!   POST /api/pixel/generate      { prompt, size } -> matriz hex 32x32
//!   POST /api/pixel/save          { name, png_base64 } -> arquivo salvo

use axum::{
    extract::State,
    http::{header, HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::post,
    Json, Router,
};
use std::error::Error as StdError;
use base64::{engine::general_purpose::STANDARD as B64, Engine};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::Arc;

#[derive(Clone)]
pub struct PixelState {
    pub admin_password: Arc<String>,
    pub gemini_key: Arc<Option<String>>,
    pub sprites_dir: Arc<PathBuf>,
    pub session_cookie: Arc<String>,
}

impl PixelState {
    pub fn from_env() -> Self {
        let admin_password = std::env::var("PIXEL_ADMIN_PASSWORD")
            .unwrap_or_else(|_| "Nop1nop2!".into());
        let gemini_key = std::env::var("GEMINI_API_KEY").ok();
        let sprites_dir = PathBuf::from(
            std::env::var("SPRITES_DIR").unwrap_or_else(|_| "assets/sprites".into()),
        );
        // Token simples derivado do hash SHA-ish da senha (sem crypto dep:
        // usamos o tamanho+chars pra autenticidade basica — e ferramenta
        // local, nao endpoint publico).
        let token = format!("pix_{}", fnv1a(&admin_password));
        Self {
            admin_password: Arc::new(admin_password),
            gemini_key: Arc::new(gemini_key),
            sprites_dir: Arc::new(sprites_dir),
            session_cookie: Arc::new(token),
        }
    }

    /// Verifica o cookie pix_auth. Retorna true se o token bate.
    fn is_authed(&self, headers: &HeaderMap) -> bool {
        let Some(cookie) = headers.get(header::COOKIE).and_then(|v| v.to_str().ok()) else {
            return false;
        };
        cookie
            .split(';')
            .map(|p| p.trim())
            .any(|p| p == format!("pix_auth={}", self.session_cookie))
    }
}

fn fnv1a(s: &str) -> u64 {
    let mut h: u64 = 0xcbf29ce484222325;
    for b in s.bytes() {
        h ^= b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    h
}

pub fn router(state: PixelState) -> Router {
    Router::new()
        .route("/auth", post(auth))
        .route("/generate", post(generate))
        .route("/save", post(save_sprite))
        .route("/list", post(list_sprites))
        .with_state(state)
}

// ── AUTH ──────────────────────────────────────────────────────────────────

#[derive(Deserialize)]
struct AuthReq { password: String }

async fn auth(
    State(state): State<PixelState>,
    Json(req): Json<AuthReq>,
) -> Response {
    if req.password == *state.admin_password {
        let cookie = format!(
            "pix_auth={}; Path=/; HttpOnly; SameSite=Strict; Max-Age=86400",
            state.session_cookie
        );
        (
            StatusCode::OK,
            [(header::SET_COOKIE, cookie)],
            Json(serde_json::json!({"ok": true})),
        ).into_response()
    } else {
        (StatusCode::UNAUTHORIZED,
         Json(serde_json::json!({"error": "senha invalida"}))).into_response()
    }
}

// ── GENERATE ──────────────────────────────────────────────────────────────

#[derive(Deserialize)]
struct GenerateReq {
    prompt: String,
    #[serde(default = "default_size")]
    size: u32,
}
fn default_size() -> u32 { 32 }

#[derive(Serialize)]
struct GenerateRes {
    /// Matriz de hex colors 'size x size'. "#00000000" = transparente.
    pixels: Vec<Vec<String>>,
}

async fn generate(
    State(state): State<PixelState>,
    headers: HeaderMap,
    Json(req): Json<GenerateReq>,
) -> Response {
    if !state.is_authed(&headers) {
        return (StatusCode::UNAUTHORIZED,
                Json(serde_json::json!({"error": "nao autenticado"}))).into_response();
    }
    let size = req.size.clamp(8, 64);

    let Some(key) = state.gemini_key.as_ref().clone() else {
        return (StatusCode::SERVICE_UNAVAILABLE,
                Json(serde_json::json!({"error": "GEMINI_API_KEY nao configurada"}))).into_response();
    };

    let system_prompt = format!(
        "Gere um sprite pixel-art {size}x{size} do tema: \"{}\".

Regras obrigatorias:
- Estilo: pixel art top-down dark fantasy, bordas nitidas, SEM anti-aliasing.
- Sprite centralizado ocupando a maior parte da area (use pelo menos
  60% das celulas — NAO gere uma matriz vazia).
- Fundo sempre transparente (\"#00000000\").
- Paleta limitada: no maximo 8 cores distintas + transparente.
- Linhas de contorno em preto ou cinza muito escuro pra dar definicao.

Saida OBRIGATORIA: JSON puro (sem markdown, sem explicacao, sem texto
antes ou depois), exatamente neste formato:

{{\"pixels\":[row0, row1, ..., row{last}]}}

Onde cada row e um array de {size} strings no formato \"#RRGGBBAA\".
A matriz toda deve ter exatamente {size} linhas e cada linha exatamente
{size} colunas.",
        req.prompt,
        last = size - 1,
    );

    let body = serde_json::json!({
        "contents": [{"parts": [{"text": system_prompt}]}],
        "generationConfig": {
            "temperature": 0.9,
            "response_mime_type": "application/json",
            // 32x32 tem 1024 celulas * ~12 chars + virgulas ≈ 15k tokens.
            // Damos folga. Pro 2.5-pro suporta bem.
            "maxOutputTokens": 32768,
            // Gemini 2.5 tem 'thinking' por default. Pro sprite art habilita
            // pensamento leve (melhora qualidade/coerencia do JSON grande).
            "thinkingConfig": {"thinkingBudget": 1024},
        }
    });

    // Modelo configuravel via env. Default: gemini-2.5-flash (barato/rapido).
    let model = std::env::var("GEMINI_MODEL")
        .unwrap_or_else(|_| "gemini-2.5-flash".into());
    let url = format!(
        "https://generativelanguage.googleapis.com/v1beta/models/{}:generateContent?key={}",
        model, key
    );
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(180))
        .connect_timeout(std::time::Duration::from_secs(30))
        .build()
        .unwrap();
    let resp = match client.post(&url).json(&body).send().await {
        Ok(r) => r,
        Err(e) => {
            let src = StdError::source(&e).map(|s| format!("{}", s)).unwrap_or_default();
            tracing::warn!("gemini request err: {e} | source: {src}");
            return (StatusCode::BAD_GATEWAY,
                    Json(serde_json::json!({"error": format!("gemini request: {e} ({src})")}))).into_response();
        }
    };
    if !resp.status().is_success() {
        let st = resp.status();
        let txt = resp.text().await.unwrap_or_default();
        tracing::warn!("gemini {st}: {txt}");
        return (StatusCode::BAD_GATEWAY,
                Json(serde_json::json!({"error": format!("gemini {st}: {txt}")}))).into_response();
    }
    let raw: serde_json::Value = match resp.json().await {
        Ok(v) => v,
        Err(e) => {
            return (StatusCode::BAD_GATEWAY,
                    Json(serde_json::json!({"error": format!("parse json: {e}")}))).into_response();
        }
    };
    // Extrai o texto gerado
    let gen_text = raw["candidates"][0]["content"]["parts"][0]["text"]
        .as_str()
        .unwrap_or("")
        .trim();
    let finish_reason = raw["candidates"][0]["finishReason"].as_str().unwrap_or("?");
    tracing::info!("gemini finish={} text_len={} preview={:?}",
        finish_reason, gen_text.len(),
        gen_text.chars().take(120).collect::<String>());
    if gen_text.is_empty() {
        tracing::warn!("gemini raw: {}", raw);
        return (StatusCode::BAD_GATEWAY,
                Json(serde_json::json!({
                    "error": format!("gemini retornou texto vazio (finish={finish_reason})"),
                    "raw": raw,
                }))).into_response();
    }
    // Caso tenha vindo com code fence, strip
    let json_str = gen_text
        .trim_start_matches("```json")
        .trim_start_matches("```")
        .trim_end_matches("```")
        .trim();
    let parsed: serde_json::Value = match serde_json::from_str(json_str) {
        Ok(v) => v,
        Err(e) => {
            tracing::warn!("gemini output nao-json: {gen_text}");
            return (StatusCode::BAD_GATEWAY,
                    Json(serde_json::json!({"error": format!("parse gemini output: {e}"), "raw": gen_text}))).into_response();
        }
    };
    let pixels_v = parsed.get("pixels").cloned().unwrap_or(parsed);
    let Some(arr) = pixels_v.as_array() else {
        return (StatusCode::BAD_GATEWAY,
                Json(serde_json::json!({"error": "formato inesperado (sem 'pixels')"}))).into_response();
    };
    let mut out: Vec<Vec<String>> = Vec::with_capacity(arr.len());
    for row_v in arr {
        let Some(row) = row_v.as_array() else { continue };
        let mut r = Vec::with_capacity(row.len());
        for cell in row {
            r.push(cell.as_str().unwrap_or("#00000000").to_string());
        }
        out.push(r);
    }
    // Normaliza pra tamanho exato — trunca/extende com transparente
    for row in out.iter_mut() {
        while row.len() < size as usize { row.push("#00000000".into()); }
        row.truncate(size as usize);
    }
    while out.len() < size as usize {
        out.push(vec!["#00000000".into(); size as usize]);
    }
    out.truncate(size as usize);

    Json(GenerateRes { pixels: out }).into_response()
}

// ── SAVE ──────────────────────────────────────────────────────────────────

#[derive(Deserialize)]
struct SaveReq {
    name: String,
    /// PNG bytes em base64 (sem prefix 'data:image/png;base64,').
    png_base64: String,
}

async fn save_sprite(
    State(state): State<PixelState>,
    headers: HeaderMap,
    Json(req): Json<SaveReq>,
) -> Response {
    if !state.is_authed(&headers) {
        return (StatusCode::UNAUTHORIZED,
                Json(serde_json::json!({"error": "nao autenticado"}))).into_response();
    }
    // Sanitiza nome: alfanumerico + '-' + '_' + '.png'
    let name = req.name.trim();
    if name.is_empty() || name.len() > 64 {
        return (StatusCode::BAD_REQUEST,
                Json(serde_json::json!({"error": "nome invalido"}))).into_response();
    }
    if !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_') {
        return (StatusCode::BAD_REQUEST,
                Json(serde_json::json!({"error": "nome so pode ter A-Z, 0-9, - e _"}))).into_response();
    }

    // Decodifica base64
    let raw = match B64.decode(req.png_base64.trim()) {
        Ok(b) => b,
        Err(e) => {
            return (StatusCode::BAD_REQUEST,
                    Json(serde_json::json!({"error": format!("base64: {e}")}))).into_response();
        }
    };
    // Cria dir se nao existe
    let dir: &Path = state.sprites_dir.as_path();
    if let Err(e) = std::fs::create_dir_all(dir) {
        return (StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": format!("mkdir: {e}")}))).into_response();
    }
    let path = dir.join(format!("{name}.png"));
    if let Err(e) = std::fs::write(&path, &raw) {
        return (StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": format!("write: {e}")}))).into_response();
    }
    tracing::info!("sprite salvo: {}", path.display());
    Json(serde_json::json!({"saved": path.display().to_string()})).into_response()
}

// ── LIST ──────────────────────────────────────────────────────────────────

async fn list_sprites(
    State(state): State<PixelState>,
    headers: HeaderMap,
) -> Response {
    if !state.is_authed(&headers) {
        return (StatusCode::UNAUTHORIZED,
                Json(serde_json::json!({"error": "nao autenticado"}))).into_response();
    }
    let dir: &Path = state.sprites_dir.as_path();
    let mut names: Vec<String> = Vec::new();
    if let Ok(entries) = std::fs::read_dir(dir) {
        for e in entries.flatten() {
            if let Some(n) = e.file_name().to_str() {
                if n.ends_with(".png") {
                    names.push(n.to_string());
                }
            }
        }
    }
    names.sort();
    Json(serde_json::json!({"sprites": names})).into_response()
}
