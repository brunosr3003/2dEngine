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
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use jsonwebtoken::{encode, decode, Header, Validation, EncodingKey, DecodingKey};

#[derive(Clone)]
pub struct PixelState {
    pub admin_password: Arc<String>,
    pub gemini_key: Arc<Option<String>>,
    pub sprites_dir: Arc<PathBuf>,
    pub jwt_secret: Arc<String>,
}

#[derive(Serialize, Deserialize)]
struct JwtClaims {
    sub: String,  // "admin"
    exp: u64,     // unix ts
    iat: u64,
}

impl PixelState {
    pub fn from_env() -> Self {
        let admin_password = std::env::var("PIXEL_ADMIN_PASSWORD")
            .unwrap_or_else(|_| "Nop1nop2!".into());
        let gemini_key = std::env::var("GEMINI_API_KEY").ok();
        let sprites_dir = PathBuf::from(
            std::env::var("SPRITES_DIR").unwrap_or_else(|_| "assets/sprites".into()),
        );
        // Secret do JWT. Em prod, definir PIXEL_JWT_SECRET em env.
        let jwt_secret = std::env::var("PIXEL_JWT_SECRET")
            .unwrap_or_else(|_| format!("pix-jwt-{}", admin_password));
        Self {
            admin_password: Arc::new(admin_password),
            gemini_key: Arc::new(gemini_key),
            sprites_dir: Arc::new(sprites_dir),
            jwt_secret: Arc::new(jwt_secret),
        }
    }

    fn issue_token(&self) -> anyhow::Result<String> {
        let now = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs();
        let claims = JwtClaims {
            sub: "admin".into(),
            iat: now,
            exp: now + 7 * 24 * 3600, // 7 dias
        };
        let tok = encode(
            &Header::default(),
            &claims,
            &EncodingKey::from_secret(self.jwt_secret.as_bytes()),
        )?;
        Ok(tok)
    }

    /// Verifica token JWT em header `Authorization: Bearer <token>`.
    /// Fallback: cookie pix_auth (compat temporaria).
    fn is_authed(&self, headers: &HeaderMap) -> bool {
        // Prefere Authorization header
        if let Some(auth) = headers.get(header::AUTHORIZATION).and_then(|v| v.to_str().ok()) {
            if let Some(tok) = auth.strip_prefix("Bearer ") {
                let ok = decode::<JwtClaims>(
                    tok.trim(),
                    &DecodingKey::from_secret(self.jwt_secret.as_bytes()),
                    &Validation::default(),
                );
                if ok.is_ok() { return true; }
            }
        }
        // Fallback cookie
        if let Some(cookie) = headers.get(header::COOKIE).and_then(|v| v.to_str().ok()) {
            for part in cookie.split(';').map(|p| p.trim()) {
                if let Some(tok) = part.strip_prefix("pix_auth=") {
                    let ok = decode::<JwtClaims>(
                        tok,
                        &DecodingKey::from_secret(self.jwt_secret.as_bytes()),
                        &Validation::default(),
                    );
                    if ok.is_ok() { return true; }
                }
            }
        }
        false
    }
}

pub fn router(state: PixelState) -> Router {
    Router::new()
        .route("/auth", post(auth))
        .route("/verify", post(verify))
        .route("/generate", post(generate))
        .route("/generate-svg", post(generate_svg))
        .route("/save", post(save_sprite))
        .route("/load", post(load_sprite))
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
    if req.password != *state.admin_password {
        return (StatusCode::UNAUTHORIZED,
                Json(serde_json::json!({"error": "senha invalida"}))).into_response();
    }
    let token = match state.issue_token() {
        Ok(t) => t,
        Err(e) => return (StatusCode::INTERNAL_SERVER_ERROR,
                          Json(serde_json::json!({"error": format!("jwt: {e}")}))).into_response(),
    };
    // Tambem seta cookie (opcional, ajuda em casos de SSR/proxy). Mas o
    // frontend vai usar o campo `token` pra localStorage.
    let cookie = format!(
        "pix_auth={}; Path=/; SameSite=Lax; Max-Age=604800",
        token
    );
    (
        StatusCode::OK,
        [(header::SET_COOKIE, cookie)],
        Json(serde_json::json!({"ok": true, "token": token})),
    ).into_response()
}

/// Endpoint simples pra testar se o token ainda e valido.
async fn verify(
    State(state): State<PixelState>,
    headers: HeaderMap,
) -> Response {
    if state.is_authed(&headers) {
        Json(serde_json::json!({"ok": true})).into_response()
    } else {
        (StatusCode::UNAUTHORIZED,
         Json(serde_json::json!({"error": "token invalido"}))).into_response()
    }
}

// ── GENERATE SVG (modo economico: Gemini gera SVG -> frontend rasteriza) ──

#[derive(Deserialize)]
struct GenerateSvgReq {
    prompt: String,
    #[serde(default = "default_size")]
    size: u32,
    #[serde(default)]
    model: Option<String>,
}

#[derive(Serialize)]
struct GenerateSvgRes {
    svg: String,
}

async fn generate_svg(
    State(state): State<PixelState>,
    headers: HeaderMap,
    Json(req): Json<GenerateSvgReq>,
) -> Response {
    if !state.is_authed(&headers) {
        return (StatusCode::UNAUTHORIZED,
                Json(serde_json::json!({"error": "nao autenticado"}))).into_response();
    }
    let size = req.size.clamp(8, 128);
    let Some(key) = state.gemini_key.as_ref().clone() else {
        return (StatusCode::SERVICE_UNAVAILABLE,
                Json(serde_json::json!({"error": "GEMINI_API_KEY nao configurada"}))).into_response();
    };

    const ALLOWED: &[&str] = &[
        "gemini-2.5-flash-lite",
        "gemini-2.5-flash",
        "gemini-2.5-pro",
        "gemini-3-flash-preview",
        "gemini-3-pro-preview",
        "gemini-3.1-flash-lite-preview",
        "gemini-3.1-pro-preview",
    ];
    let requested = req.model.clone()
        .or_else(|| std::env::var("GEMINI_MODEL").ok())
        .unwrap_or_else(|| "gemini-2.5-flash".into());
    let model = if ALLOWED.contains(&requested.as_str()) {
        requested
    } else {
        "gemini-2.5-flash".to_string()
    };

    let system_prompt = format!(
        "Gere um SVG simples e geometricamente COMPACTO de pixel-art {size}x{size} do tema: \"{}\".

### Estrutura do SVG:
<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 {size} {size}\" shape-rendering=\"crispEdges\">
  ...shapes...
</svg>

### REGRA #1 (MAIS IMPORTANTE):
NUNCA use <rect> 1x1. Pense em BLOCOS de pixels, nao pixel por pixel.
Cada <rect> deve cobrir uma AREA de pelo menos 2x2 pixels (preferencia 3x3
ou maior). Use formas que cubram areas continuas da mesma cor.
A arte inteira deve caber em 20 a 60 shapes no maximo.

### Paleta e cor:
- fill=\"#RRGGBB\" hex 6 digitos (fundo ja e transparente por default)
- Paleta com 8-14 cores. Shading em 2-3 tons por area.
- Contorno do sprite em preto escuro (#0a0a0a) como rects/polygons de
  area definida cobrindo a silhueta externa.

### Coordenadas:
- TUDO inteiro (sem decimais). Snap-to-pixel.
- Sprite centralizado ocupando 70-90% do viewBox.

### Formas permitidas:
<rect x=\"..\" y=\"..\" width=\"..\" height=\"..\" fill=\"#rrggbb\"/>  ← prefira esta
<circle cx=\"..\" cy=\"..\" r=\"..\" fill=\"#rrggbb\"/>
<ellipse ..>
<polygon points=\"x1,y1 x2,y2 ...\" fill=\"#rrggbb\"/>
<path d=\"M x y L x y L x y Z\" fill=\"#rrggbb\"/>  ← usar so se necessario

### PROIBIDO:
- <text>, <filter>, <feGaussianBlur>, <linearGradient>, <radialGradient>
- opacity, fill-opacity, stroke-opacity
- shape-rendering diferente de crispEdges
- decimais em coordenadas
- rects menores que 2x2 (exceto bordas/contornos 1px justificados)

### Para humanoides (se aplicavel):
- cabeca ~1/5 altura (ou ~1/3 se chibi)
- tronco ~2/5
- pernas ~2/5
- monte com poucos rects grandes, nao desenhe pixel a pixel

### Saida OBRIGATORIA — JSON puro, sem markdown:

{{\"svg\":\"<svg xmlns=...>...<\\/svg>\"}}",
        req.prompt,
    );

    let body = serde_json::json!({
        "contents": [{"parts": [{"text": system_prompt}]}],
        "generationConfig": {
            "temperature": 0.7,
            "response_mime_type": "application/json",
            "maxOutputTokens": 32768,
            "thinkingConfig": {"thinkingBudget": 1024},
        }
    });

    let url = format!(
        "https://generativelanguage.googleapis.com/v1beta/models/{}:generateContent?key={}",
        model, key
    );
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(300))
        .connect_timeout(std::time::Duration::from_secs(30))
        .build()
        .unwrap();
    let resp = match client.post(&url).json(&body).send().await {
        Ok(r) => r,
        Err(e) => {
            let src = StdError::source(&e).map(|s| format!("{}", s)).unwrap_or_default();
            return (StatusCode::BAD_GATEWAY,
                    Json(serde_json::json!({"error": format!("gemini: {e} ({src})")}))).into_response();
        }
    };
    if !resp.status().is_success() {
        let st = resp.status();
        let txt = resp.text().await.unwrap_or_default();
        return (StatusCode::BAD_GATEWAY,
                Json(serde_json::json!({"error": format!("gemini {st}: {txt}")}))).into_response();
    }
    let raw: serde_json::Value = match resp.json().await {
        Ok(v) => v,
        Err(e) => return (StatusCode::BAD_GATEWAY,
                          Json(serde_json::json!({"error": format!("parse json: {e}")}))).into_response(),
    };
    let gen_text = raw["candidates"][0]["content"]["parts"][0]["text"]
        .as_str().unwrap_or("").trim();
    let finish = raw["candidates"][0]["finishReason"].as_str().unwrap_or("?");
    tracing::info!("gemini-svg finish={finish} len={} model={model}", gen_text.len());

    if gen_text.is_empty() {
        return (StatusCode::BAD_GATEWAY,
                Json(serde_json::json!({"error": format!("texto vazio (finish={finish})"), "raw": raw}))).into_response();
    }
    let json_str = gen_text
        .trim_start_matches("```json")
        .trim_start_matches("```xml")
        .trim_start_matches("```")
        .trim_end_matches("```")
        .trim();
    let svg = match serde_json::from_str::<serde_json::Value>(json_str) {
        Ok(parsed) => parsed.get("svg").and_then(|v| v.as_str()).unwrap_or("").to_string(),
        Err(_) => {
            // Fallback: tenta extrair `<svg...>...</svg>` do texto cru (caso o
            // JSON tenha estourado o limite ou vindo mal-formado).
            let start = json_str.find("<svg");
            let end = json_str.rfind("</svg>").map(|i| i + "</svg>".len());
            match (start, end) {
                (Some(s), Some(e)) if e > s => {
                    // Unescape de \" -> "
                    json_str[s..e].replace("\\\"", "\"").replace("\\/", "/")
                }
                _ => {
                    tracing::warn!("svg parse falhou (preview): {}", &gen_text[..gen_text.len().min(400)]);
                    return (StatusCode::BAD_GATEWAY,
                            Json(serde_json::json!({
                                "error": "svg nao encontrado na saida",
                                "raw_preview": &gen_text[..gen_text.len().min(400)],
                            }))).into_response();
                }
            }
        }
    };
    if svg.is_empty() || !svg.contains("<svg") {
        return (StatusCode::BAD_GATEWAY,
                Json(serde_json::json!({"error": "svg ausente ou invalido"}))).into_response();
    }
    // Detecta truncamento: finishReason=MAX_TOKENS ou falta </svg>
    let truncated = finish == "MAX_TOKENS" || !svg.contains("</svg>");
    if truncated {
        tracing::warn!("svg truncado (finish={finish}, len={})", svg.len());
        return (StatusCode::BAD_GATEWAY,
                Json(serde_json::json!({
                    "error": format!("resposta truncada — o modelo estourou o limite (finish={finish}). \
                                      Tenta: (1) modelo mais rapido (2.5-flash), \
                                      (2) prompt mais curto, ou (3) tamanho menor (64×64)."),
                    "svg_partial_len": svg.len(),
                }))).into_response();
    }
    Json(GenerateSvgRes { svg }).into_response()
}

// ── GENERATE ──────────────────────────────────────────────────────────────

#[derive(Deserialize)]
struct GenerateReq {
    prompt: String,
    #[serde(default = "default_size")]
    size: u32,
    /// Override opcional do modelo (senao cai no env GEMINI_MODEL ou default).
    #[serde(default)]
    model: Option<String>,
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
    let size = req.size.clamp(8, 128);

    let Some(key) = state.gemini_key.as_ref().clone() else {
        return (StatusCode::SERVICE_UNAVAILABLE,
                Json(serde_json::json!({"error": "GEMINI_API_KEY nao configurada"}))).into_response();
    };

    let system_prompt = format!(
        "Gere um sprite pixel-art {size}x{size} do tema: \"{}\".

Regras tecnicas obrigatorias (estilo/tema vem do prompt do usuario):
- Pixel art puro: bordas nitidas, SEM anti-aliasing, SEM blur.
- Sprite centralizado ocupando 70-90% da area (use MUITAS celulas de
  cor — NAO gere matriz vazia nem sprite minusculo num canto).
- Fundo sempre transparente (\"#00000000\").
- Paleta rica: 10-18 cores distintas + transparente. Shading em 2-3
  tons por area (cor-base + sombra + highlight).
- Contorno preto ou escuro em volta do sprite pra definicao.
- Proporcoes anatomicas corretas pra humanoides: cabeca ~1/5 da altura,
  tronco ~2/5, pernas ~2/5. NAO use chibi/boneco-de-palito a menos
  que o usuario peça explicitamente.

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
            // 64x64 = 4096 celulas * ~13 chars ≈ 55k tokens de saida.
            // Gemini 2.5/3.1 suportam ate 65k de output.
            "maxOutputTokens": 65536,
            // Gemini 2.5 tem 'thinking' por default. Pro sprite art habilita
            // pensamento leve (melhora qualidade/coerencia do JSON grande).
            "thinkingConfig": {"thinkingBudget": 1024},
        }
    });

    // Modelo: override do cliente > env GEMINI_MODEL > default.
    // Whitelist pra evitar abuso.
    const ALLOWED: &[&str] = &[
        "gemini-2.5-flash-lite",
        "gemini-2.5-flash",
        "gemini-2.5-pro",
        "gemini-3-flash-preview",
        "gemini-3-pro-preview",
        "gemini-3.1-flash-lite-preview",
        "gemini-3.1-pro-preview",
    ];
    let requested = req.model.clone()
        .or_else(|| std::env::var("GEMINI_MODEL").ok())
        .unwrap_or_else(|| "gemini-2.5-flash".into());
    let model = if ALLOWED.contains(&requested.as_str()) {
        requested
    } else {
        "gemini-2.5-flash".to_string()
    };
    let url = format!(
        "https://generativelanguage.googleapis.com/v1beta/models/{}:generateContent?key={}",
        model, key
    );
    // Modelos 'pro' podem gerar 4096 celulas x 12 chars = 50k tokens + thinking.
    // 64x64 com pro pode bater 4-5 minutos.
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(420))      // 7 min total
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

// ── FORMATO TXT (XPM-like) + PNG ──────────────────────────────────────────

/// Mapeia cores unicas da matriz pra caracteres legiveis e gera texto:
///
/// ```text
/// # sprite 64x64
/// # palette:
/// #   . = #00000000  (transparent)
/// #   K = #0a0a0a
/// #   S = #e6b37a
/// #   ...
/// # pixels:
/// .........KKKKK...
/// .........KSSSK...
/// ...
/// ```
///
/// Edita-se trocando 1 carac por celula. Re-salva regera PNG.
fn matrix_to_xpm(matrix: &[Vec<String>]) -> (String, std::collections::BTreeMap<String, char>) {
    use std::collections::BTreeMap;
    const POOL: &str = "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789+-=*^$%&@!?<>";
    let mut pool_iter = POOL.chars();
    let mut palette: BTreeMap<String, char> = BTreeMap::new();
    // Transparente SEMPRE e '.'
    palette.insert("#00000000".to_string(), '.');

    // Coleta cores unicas na ordem de aparicao
    let mut seen_order: Vec<String> = Vec::new();
    for row in matrix {
        for c in row {
            let key = normalize_hex_str(c);
            if key == "#00000000" { continue; }
            if !palette.contains_key(&key) {
                if let Some(ch) = pool_iter.next() {
                    palette.insert(key.clone(), ch);
                    seen_order.push(key);
                } else {
                    // Sem mais caracteres — cor cai no '.' (transparente). Raro
                    // em paletas normais (~75 cores ja no pool).
                }
            }
        }
    }

    let height = matrix.len();
    let width = matrix.first().map(|r| r.len()).unwrap_or(0);

    let mut out = String::new();
    out.push_str(&format!("# sprite {width}x{height}\n"));
    out.push_str("# palette:\n");
    // Paleta em ordem: transparente primeiro, depois na ordem de aparicao.
    out.push_str("#   . = #00000000  (transparent)\n");
    for hex in &seen_order {
        let ch = palette[hex];
        out.push_str(&format!("#   {ch} = {hex}\n"));
    }
    out.push_str("#\n# pixels:\n");

    for row in matrix {
        for c in row {
            let key = normalize_hex_str(c);
            let ch = palette.get(&key).copied().unwrap_or('.');
            out.push(ch);
        }
        out.push('\n');
    }
    (out, palette)
}

/// Parseia texto XPM-like de volta pra matriz.
fn xpm_to_matrix(txt: &str) -> anyhow::Result<Vec<Vec<String>>> {
    use std::collections::HashMap;
    let mut palette: HashMap<char, String> = HashMap::new();
    let mut in_pixels = false;
    let mut rows: Vec<Vec<String>> = Vec::new();
    let mut declared_width: Option<usize> = None;

    for raw_line in txt.lines() {
        if !in_pixels {
            let line = raw_line.trim();
            if line.starts_with("# pixels") {
                in_pixels = true;
                continue;
            }
            // Header com 'WxH' pra saber largura esperada (opcional)
            if line.starts_with("# sprite") {
                if let Some(rest) = line.strip_prefix("# sprite").map(|s| s.trim()) {
                    if let Some((w_s, _)) = rest.split_once('x') {
                        if let Ok(w) = w_s.trim().parse::<usize>() {
                            declared_width = Some(w);
                        }
                    }
                }
                continue;
            }
            // Entrada de paleta: "#   X = #RRGGBBAA"
            if let Some(body) = line.strip_prefix('#') {
                let body = body.trim();
                if let Some((lhs, rhs)) = body.split_once('=') {
                    let lhs = lhs.trim();
                    let rhs = rhs.trim();
                    let ch = lhs.chars().next();
                    let hex = rhs.split_whitespace().next().unwrap_or("");
                    if hex.starts_with('#') {
                        if let Some(c) = ch {
                            palette.insert(c, normalize_hex_str(hex));
                        }
                    }
                }
            }
        } else {
            if raw_line.trim_start().starts_with('#') { continue; }
            if raw_line.is_empty() { continue; }
            let mut row: Vec<String> = Vec::new();
            for ch in raw_line.chars() {
                if ch == '\n' || ch == '\r' { continue; }
                let hex = palette.get(&ch).cloned().unwrap_or_else(|| "#00000000".to_string());
                row.push(hex);
            }
            // Remove padding de linha (se vier menor, completa com transparente;
            // se vier maior, trunca).
            if let Some(w) = declared_width {
                while row.len() < w { row.push("#00000000".into()); }
                row.truncate(w);
            }
            rows.push(row);
        }
    }
    // Normaliza altura se nao bateu
    if rows.is_empty() {
        anyhow::bail!("arquivo .txt sem pixels");
    }
    Ok(rows)
}

fn normalize_hex_str(s: &str) -> String {
    let s = s.trim().to_lowercase();
    let s = if s.starts_with('#') { s } else { format!("#{s}") };
    match s.len() {
        4 => { // #rgb
            let b = s.as_bytes();
            format!("#{0}{0}{1}{1}{2}{2}ff",
                b[1] as char, b[2] as char, b[3] as char)
        }
        7 => format!("{}ff", s),
        9 => s,
        _ => "#00000000".to_string(),
    }
}

fn hex_to_rgba(hex: &str) -> (u8, u8, u8, u8) {
    let h = normalize_hex_str(hex);
    // h = #rrggbbaa
    let b = h.as_bytes();
    let p = |i: usize| {
        let hi = (b[i] as char).to_digit(16).unwrap_or(0) as u8;
        let lo = (b[i + 1] as char).to_digit(16).unwrap_or(0) as u8;
        (hi << 4) | lo
    };
    (p(1), p(3), p(5), p(7))
}

/// Converte matriz de hex em bytes PNG.
fn matrix_to_png_bytes(matrix: &[Vec<String>]) -> anyhow::Result<Vec<u8>> {
    use image::{ImageBuffer, Rgba};
    let height = matrix.len() as u32;
    let width = matrix.first().map(|r| r.len()).unwrap_or(0) as u32;
    if width == 0 || height == 0 { anyhow::bail!("matriz vazia"); }
    let mut img: ImageBuffer<Rgba<u8>, Vec<u8>> = ImageBuffer::new(width, height);
    for (y, row) in matrix.iter().enumerate() {
        for (x, hex) in row.iter().enumerate() {
            let (r, g, b, a) = hex_to_rgba(hex);
            img.put_pixel(x as u32, y as u32, Rgba([r, g, b, a]));
        }
    }
    let mut buf = Vec::new();
    img.write_to(&mut std::io::Cursor::new(&mut buf), image::ImageFormat::Png)?;
    Ok(buf)
}

fn valid_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 64
        && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

// ── SAVE ──────────────────────────────────────────────────────────────────

#[derive(Deserialize)]
struct SaveReq {
    name: String,
    /// Matriz HxW de strings hex. A fonte canonica fica no .txt; o .png
    /// e regerado a cada save pra o jogo carregar rapido.
    pixels: Vec<Vec<String>>,
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
    let name = req.name.trim();
    if !valid_name(name) {
        return (StatusCode::BAD_REQUEST,
                Json(serde_json::json!({"error": "nome so pode ter A-Z, 0-9, - e _ (max 64)"}))).into_response();
    }
    if req.pixels.is_empty() {
        return (StatusCode::BAD_REQUEST,
                Json(serde_json::json!({"error": "pixels vazio"}))).into_response();
    }

    let dir: &Path = state.sprites_dir.as_path();
    if let Err(e) = std::fs::create_dir_all(dir) {
        return (StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": format!("mkdir: {e}")}))).into_response();
    }

    // Gera .txt (XPM-like) — fonte canonica pro dev editar
    let (txt, _palette) = matrix_to_xpm(&req.pixels);
    let txt_path = dir.join(format!("{name}.txt"));
    if let Err(e) = std::fs::write(&txt_path, &txt) {
        return (StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": format!("write txt: {e}")}))).into_response();
    }

    // Gera .png — consumido pelo jogo
    let png_bytes = match matrix_to_png_bytes(&req.pixels) {
        Ok(b) => b,
        Err(e) => return (StatusCode::INTERNAL_SERVER_ERROR,
                          Json(serde_json::json!({"error": format!("png: {e}")}))).into_response(),
    };
    let png_path = dir.join(format!("{name}.png"));
    if let Err(e) = std::fs::write(&png_path, &png_bytes) {
        return (StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": format!("write png: {e}")}))).into_response();
    }

    tracing::info!("sprite '{name}' salvo: {} + {}", txt_path.display(), png_path.display());
    Json(serde_json::json!({
        "saved_txt": txt_path.display().to_string(),
        "saved_png": png_path.display().to_string(),
    })).into_response()
}

// ── LOAD ──────────────────────────────────────────────────────────────────

#[derive(Deserialize)]
struct LoadReq { name: String }

async fn load_sprite(
    State(state): State<PixelState>,
    headers: HeaderMap,
    Json(req): Json<LoadReq>,
) -> Response {
    if !state.is_authed(&headers) {
        return (StatusCode::UNAUTHORIZED,
                Json(serde_json::json!({"error": "nao autenticado"}))).into_response();
    }
    let name = req.name.trim();
    if !valid_name(name) {
        return (StatusCode::BAD_REQUEST,
                Json(serde_json::json!({"error": "nome invalido"}))).into_response();
    }
    let dir: &Path = state.sprites_dir.as_path();
    let txt_path = dir.join(format!("{name}.txt"));
    let txt = match std::fs::read_to_string(&txt_path) {
        Ok(s) => s,
        Err(e) => return (StatusCode::NOT_FOUND,
                          Json(serde_json::json!({"error": format!("{}: {e}", txt_path.display())}))).into_response(),
    };
    match xpm_to_matrix(&txt) {
        Ok(m) => Json(serde_json::json!({"pixels": m})).into_response(),
        Err(e) => (StatusCode::UNPROCESSABLE_ENTITY,
                   Json(serde_json::json!({"error": format!("parse: {e}")}))).into_response(),
    }
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
    // Lista basenames (sem extensao) + quais formatos tem
    use std::collections::BTreeMap;
    let mut map: BTreeMap<String, (bool, bool)> = BTreeMap::new();
    if let Ok(entries) = std::fs::read_dir(dir) {
        for e in entries.flatten() {
            if let Some(n) = e.file_name().to_str() {
                if let Some(stem) = n.strip_suffix(".txt") {
                    map.entry(stem.to_string()).or_default().0 = true;
                } else if let Some(stem) = n.strip_suffix(".png") {
                    map.entry(stem.to_string()).or_default().1 = true;
                }
            }
        }
    }
    let sprites: Vec<serde_json::Value> = map.into_iter()
        .map(|(name, (has_txt, has_png))| serde_json::json!({
            "name": name,
            "has_txt": has_txt,
            "has_png": has_png,
        }))
        .collect();
    Json(serde_json::json!({"sprites": sprites})).into_response()
}
