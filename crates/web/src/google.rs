//! Login com Google (OAuth 2.0 + PKCE S256) pro cliente do jogo.
//!
//!   GET /api/auth/google/config    -> { enabled }
//!   GET /api/auth/google/start     -> { state, url }
//!   GET /api/auth/google/callback  <- redirect do Google (pagina HTML)
//!   GET /api/auth/google/poll      -> { status: pendente|ok|erro, username?, token? }
//!
//! O cliente abre `url` no navegador e fica no `poll`. O callback troca o
//! `code` pelo `id_token`, valida, acha/cria a conta e deixa pronta uma sessao
//! (`login_tokens`) que o servidor de jogo aceita em `ClientMessage::LoginToken`.
//! Nada do Google e' guardado. Ver docs/LOGIN_GOOGLE.md.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use argon2::password_hash::rand_core::{OsRng, RngCore};
use axum::{
    extract::{Query, State},
    http::{HeaderMap, StatusCode},
    response::{Html, IntoResponse, Response},
    routing::get,
    Json, Router,
};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use serde::Deserialize;
use serde_json::json;
use sha2::{Digest, Sha256};
use sqlx::postgres::PgPool;

use crate::db;

const URL_AUTORIZACAO: &str = "https://accounts.google.com/o/oauth2/v2/auth";
const URL_TOKEN: &str = "https://oauth2.googleapis.com/token";
const URL_JWKS: &str = "https://www.googleapis.com/oauth2/v3/certs";
const REDIRECT_PADRAO: &str = "https://mmo.brunji.com.br/api/auth/google/callback";

/// Quanto tempo o jogador tem pra terminar no navegador.
const VALIDADE_STATE: Duration = Duration::from_secs(10 * 60);
/// Sessao do jogo emitida no fim.
const VALIDADE_SESSAO_HORAS: i64 = 12;
const MAX_PENDENTES: usize = 5_000;
const INICIOS_POR_MINUTO: u32 = 10;
const JWKS_VALIDADE: Duration = Duration::from_secs(60 * 60);

#[derive(Clone)]
struct Config {
    client_id: String,
    client_secret: String,
    redirect_uri: String,
}

#[derive(Clone)]
pub struct GoogleState {
    cfg: Option<Config>,
    pool: Arc<PgPool>,
    http: reqwest::Client,
    pendentes: Arc<Mutex<Pendentes>>,
    limite: Arc<Mutex<HashMap<String, (Instant, u32)>>>,
    jwks: Arc<Mutex<Option<(Instant, Vec<Jwk>)>>>,
}

impl GoogleState {
    pub fn from_env(pool: Arc<PgPool>) -> Self {
        let var = |k: &str| std::env::var(k).ok().filter(|v| !v.trim().is_empty());
        let cfg = match (var("GOOGLE_CLIENT_ID"), var("GOOGLE_CLIENT_SECRET")) {
            (Some(client_id), Some(client_secret)) => Some(Config {
                client_id,
                client_secret,
                redirect_uri: var("GOOGLE_REDIRECT_URI").unwrap_or_else(|| REDIRECT_PADRAO.into()),
            }),
            _ => None,
        };
        Self {
            cfg,
            pool,
            http: reqwest::Client::builder().timeout(Duration::from_secs(10)).build().unwrap_or_default(),
            pendentes: Arc::default(),
            limite: Arc::default(),
            jwks: Arc::default(),
        }
    }

    pub fn ligado(&self) -> bool {
        self.cfg.is_some()
    }
}

pub fn router(state: GoogleState) -> Router {
    Router::new()
        .route("/config", get(config))
        .route("/start", get(start))
        .route("/callback", get(callback))
        .route("/poll", get(poll))
        .with_state(state)
}

// ── pedidos pendentes (state -> PKCE verifier -> resultado) ──────────────

#[derive(Clone, Debug, PartialEq)]
pub enum Resultado {
    Pendente,
    Ok { username: String, token: String },
    Erro(String),
}

struct Pendente {
    verifier: String,
    nonce: String,
    criado: Instant,
    /// O callback ja' usou o verifier (nao aceita um segundo callback).
    usado: bool,
    resultado: Resultado,
}

/// O que o `poll` responde.
#[derive(Debug, PartialEq)]
pub enum Consulta {
    Desconhecido,
    Pendente,
    Final(Resultado),
}

#[derive(Default)]
pub struct Pendentes {
    mapa: HashMap<String, Pendente>,
}

impl Pendentes {
    fn limpa(&mut self, agora: Instant) {
        self.mapa.retain(|_, p| agora.duration_since(p.criado) < VALIDADE_STATE);
    }

    /// Guarda um pedido novo. `None` se ha' pendentes demais.
    pub fn novo(&mut self, state: String, verifier: String, nonce: String, agora: Instant) -> Option<()> {
        self.limpa(agora);
        if self.mapa.len() >= MAX_PENDENTES {
            return None;
        }
        self.mapa.insert(state, Pendente { verifier, nonce, criado: agora, usado: false, resultado: Resultado::Pendente });
        Some(())
    }

    /// Callback: entrega (verifier, nonce) uma vez so'.
    pub fn para_callback(&mut self, state: &str, agora: Instant) -> Option<(String, String)> {
        self.limpa(agora);
        let p = self.mapa.get_mut(state)?;
        if p.usado {
            return None;
        }
        p.usado = true;
        Some((p.verifier.clone(), p.nonce.clone()))
    }

    pub fn conclui(&mut self, state: &str, resultado: Resultado) {
        if let Some(p) = self.mapa.get_mut(state) {
            p.resultado = resultado;
        }
    }

    /// Poll: resultado final sai uma vez e o state morre.
    pub fn consulta(&mut self, state: &str, agora: Instant) -> Consulta {
        self.limpa(agora);
        match self.mapa.get(state).map(|p| &p.resultado) {
            None => Consulta::Desconhecido,
            Some(Resultado::Pendente) => Consulta::Pendente,
            Some(_) => Consulta::Final(self.mapa.remove(state).unwrap().resultado),
        }
    }
}

fn aleatorio(bytes: usize) -> String {
    let mut b = vec![0u8; bytes];
    OsRng.fill_bytes(&mut b);
    URL_SAFE_NO_PAD.encode(b)
}

pub fn desafio_pkce(verifier: &str) -> String {
    URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()))
}

/// Mesmo hash que `server::auth::hash_do_token` confere.
pub fn hash_do_token(token: &str) -> String {
    Sha256::digest(token.as_bytes()).iter().map(|b| format!("{b:02x}")).collect()
}

fn codifica(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => out.push(b as char),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

fn formulario(pares: &[(&str, &str)]) -> String {
    pares.iter().map(|(k, v)| format!("{}={}", codifica(k), codifica(v))).collect::<Vec<_>>().join("&")
}

// ── id_token ─────────────────────────────────────────────────────────────

#[derive(Clone, Debug, Deserialize)]
pub struct Jwk {
    pub kid: String,
    pub n: String,
    pub e: String,
}

#[derive(Deserialize)]
struct Jwks {
    keys: Vec<Jwk>,
}

#[derive(Deserialize)]
struct Claims {
    sub: String,
    email: Option<String>,
    /// O Google manda bool; ja' mandou string "true".
    email_verified: Option<serde_json::Value>,
    name: Option<String>,
    nonce: Option<String>,
}

#[derive(Debug, PartialEq)]
pub struct Identidade {
    pub sub: String,
    pub email: Option<String>,
    pub nome: Option<String>,
}

#[derive(Debug, PartialEq)]
pub enum ErroToken {
    /// `kid` fora do JWKS em cache (pode ser rotacao: vale rebuscar).
    ChaveDesconhecida,
    Invalido(String),
}

/// Assinatura (RS256 com a chave do JWKS), `aud`, `iss`, `exp`, `nonce` e
/// `email_verified`.
pub fn valida_id_token(token: &str, chaves: &[Jwk], client_id: &str, nonce: &str) -> Result<Identidade, ErroToken> {
    use jsonwebtoken::{decode, decode_header, Algorithm, DecodingKey, Validation};
    let cab = decode_header(token).map_err(|e| ErroToken::Invalido(format!("cabecalho: {e}")))?;
    if cab.alg != Algorithm::RS256 {
        return Err(ErroToken::Invalido("alg".into()));
    }
    let kid = cab.kid.ok_or_else(|| ErroToken::Invalido("sem kid".into()))?;
    let jwk = chaves.iter().find(|k| k.kid == kid).ok_or(ErroToken::ChaveDesconhecida)?;
    let chave = DecodingKey::from_rsa_components(&jwk.n, &jwk.e).map_err(|e| ErroToken::Invalido(format!("jwk: {e}")))?;
    let mut v = Validation::new(Algorithm::RS256);
    v.set_audience(&[client_id]);
    v.set_issuer(&["accounts.google.com", "https://accounts.google.com"]);
    v.set_required_spec_claims(&["exp", "iss", "aud", "sub"]);
    v.leeway = 60;
    let dados = decode::<Claims>(token, &chave, &v).map_err(|e| ErroToken::Invalido(format!("{e}")))?;
    let c = dados.claims;
    if c.nonce.as_deref() != Some(nonce) {
        return Err(ErroToken::Invalido("nonce".into()));
    }
    let verificado = match c.email_verified {
        Some(serde_json::Value::Bool(b)) => b,
        Some(serde_json::Value::String(s)) => s == "true",
        _ => false,
    };
    if !verificado {
        return Err(ErroToken::Invalido("email nao verificado".into()));
    }
    if c.sub.is_empty() {
        return Err(ErroToken::Invalido("sub vazio".into()));
    }
    Ok(Identidade { sub: c.sub, email: c.email, nome: c.name })
}

impl GoogleState {
    async fn chaves(&self, forcar: bool) -> Result<Vec<Jwk>, String> {
        {
            let cache = self.jwks.lock().unwrap();
            if let Some((quando, chaves)) = cache.as_ref() {
                let idade = quando.elapsed();
                // Rotacao: rebusca, mas no maximo 1x por minuto.
                if idade < JWKS_VALIDADE && !(forcar && idade > Duration::from_secs(60)) {
                    return Ok(chaves.clone());
                }
            }
        }
        let jwks: Jwks = self
            .http
            .get(URL_JWKS)
            .send()
            .await
            .and_then(|r| r.error_for_status())
            .map_err(|e| format!("jwks: {e}"))?
            .json()
            .await
            .map_err(|e| format!("jwks: {e}"))?;
        *self.jwks.lock().unwrap() = Some((Instant::now(), jwks.keys.clone()));
        Ok(jwks.keys)
    }

    async fn valida(&self, token: &str, client_id: &str, nonce: &str) -> Result<Identidade, String> {
        match valida_id_token(token, &self.chaves(false).await?, client_id, nonce) {
            Err(ErroToken::ChaveDesconhecida) => valida_id_token(token, &self.chaves(true).await?, client_id, nonce)
                .map_err(|e| format!("{e:?}")),
            r => r.map_err(|e| format!("{e:?}")),
        }
    }

    fn pode_iniciar(&self, ip: &str) -> bool {
        let mut mapa = self.limite.lock().unwrap();
        let agora = Instant::now();
        mapa.retain(|_, (inicio, _)| agora.duration_since(*inicio) < Duration::from_secs(60));
        let (_, n) = mapa.entry(ip.to_string()).or_insert((agora, 0));
        *n += 1;
        *n <= INICIOS_POR_MINUTO
    }
}

fn ip_de(headers: &HeaderMap) -> String {
    let h = |k: &str| headers.get(k).and_then(|v| v.to_str().ok()).map(str::to_string);
    h("x-real-ip")
        .or_else(|| h("x-forwarded-for").and_then(|v| v.split(',').next().map(|s| s.trim().to_string())))
        .unwrap_or_else(|| "local".into())
}

// ── endpoints ────────────────────────────────────────────────────────────

async fn config(State(st): State<GoogleState>) -> impl IntoResponse {
    Json(json!({ "enabled": st.ligado() }))
}

async fn start(State(st): State<GoogleState>, headers: HeaderMap) -> Response {
    let Some(cfg) = st.cfg.clone() else {
        return (StatusCode::SERVICE_UNAVAILABLE, Json(json!({ "error": "login com Google não configurado" }))).into_response();
    };
    if !st.pode_iniciar(&ip_de(&headers)) {
        return (StatusCode::TOO_MANY_REQUESTS, Json(json!({ "error": "muitas tentativas, espere um minuto" }))).into_response();
    }
    let state = aleatorio(24);
    let verifier = aleatorio(32);
    let nonce = aleatorio(16);
    let desafio = desafio_pkce(&verifier);
    if st.pendentes.lock().unwrap().novo(state.clone(), verifier, nonce.clone(), Instant::now()).is_none() {
        return (StatusCode::SERVICE_UNAVAILABLE, Json(json!({ "error": "servidor ocupado, tente de novo" }))).into_response();
    }
    let url = format!(
        "{URL_AUTORIZACAO}?{}",
        formulario(&[
            ("client_id", &cfg.client_id),
            ("redirect_uri", &cfg.redirect_uri),
            ("response_type", "code"),
            ("scope", "openid email profile"),
            ("state", &state),
            ("nonce", &nonce),
            ("code_challenge", &desafio),
            ("code_challenge_method", "S256"),
            ("prompt", "select_account"),
        ])
    );
    Json(json!({ "state": state, "url": url })).into_response()
}

#[derive(Deserialize)]
struct TokenRes {
    id_token: Option<String>,
}

fn pagina(ok: bool, msg: &str) -> Html<String> {
    let (titulo, cor) = if ok { ("Login concluído", "#e8c170") } else { ("Não deu certo", "#e86a6a") };
    Html(format!(
        "<!doctype html><html lang=pt-BR><meta charset=utf-8><meta name=viewport content='width=device-width,initial-scale=1'>\
         <title>Tempest</title><body style='margin:0;min-height:100vh;display:grid;place-items:center;\
         background:#0e131d;color:#dfe6f0;font:16px system-ui,sans-serif;text-align:center;padding:0 16px'>\
         <div><h1 style='color:{cor};font-size:24px'>{titulo}</h1><p>{msg}</p></div></body></html>"
    ))
}

async fn callback(State(st): State<GoogleState>, Query(q): Query<HashMap<String, String>>) -> Response {
    let Some(cfg) = st.cfg.clone() else {
        return (StatusCode::NOT_FOUND, pagina(false, "Login com Google não está configurado.")).into_response();
    };
    let Some(state) = q.get("state").cloned() else {
        return (StatusCode::BAD_REQUEST, pagina(false, "Pedido inválido.")).into_response();
    };
    let Some((verifier, nonce)) = st.pendentes.lock().unwrap().para_callback(&state, Instant::now()) else {
        return (StatusCode::BAD_REQUEST, pagina(false, "Este pedido expirou ou já foi usado. Tente de novo pelo jogo.")).into_response();
    };
    let falha = |publico: &str, detalhe: String| {
        tracing::warn!("login google falhou: {detalhe}");
        st.pendentes.lock().unwrap().conclui(&state, Resultado::Erro(publico.into()));
        (StatusCode::BAD_REQUEST, pagina(false, &format!("{publico} Volte ao jogo e tente de novo."))).into_response()
    };
    if q.contains_key("error") {
        return falha("Login cancelado no Google.", format!("google error={:?}", q.get("error")));
    }
    let Some(code) = q.get("code") else {
        return falha("Resposta do Google sem código.", "sem code".into());
    };

    let corpo = formulario(&[
        ("grant_type", "authorization_code"),
        ("code", code),
        ("client_id", &cfg.client_id),
        ("client_secret", &cfg.client_secret),
        ("redirect_uri", &cfg.redirect_uri),
        ("code_verifier", &verifier),
    ]);
    let troca = st
        .http
        .post(URL_TOKEN)
        .header("content-type", "application/x-www-form-urlencoded")
        .body(corpo)
        .send()
        .await;
    let resposta = match troca {
        Ok(r) if r.status().is_success() => r.json::<TokenRes>().await.map_err(|e| e.to_string()),
        Ok(r) => Err(format!("token endpoint {}: {}", r.status(), r.text().await.unwrap_or_default())),
        Err(e) => Err(e.to_string()),
    };
    let id_token = match resposta {
        Ok(TokenRes { id_token: Some(t) }) => t,
        Ok(_) => return falha("O Google não confirmou o login.", "sem id_token".into()),
        Err(e) => return falha("O Google não confirmou o login.", e),
    };
    let ident = match st.valida(&id_token, &cfg.client_id, &nonce).await {
        Ok(i) => i,
        Err(e) => return falha("Não foi possível confirmar sua conta Google (o e-mail precisa estar verificado).", e),
    };
    let (account_id, username) =
        match db::conta_google(&st.pool, &ident.sub, ident.email.as_deref(), ident.nome.as_deref()).await {
            Ok(c) => c,
            Err(e) => return falha("Erro interno ao abrir sua conta.", format!("{e:?}")),
        };
    let token = aleatorio(32);
    if let Err(e) = db::grava_token_de_login(&st.pool, account_id, &hash_do_token(&token), VALIDADE_SESSAO_HORAS).await {
        return falha("Erro interno ao abrir sua conta.", format!("{e:?}"));
    }
    tracing::info!("login google ok: conta {account_id} ({username})");
    st.pendentes.lock().unwrap().conclui(&state, Resultado::Ok { username, token });
    pagina(true, "Pode fechar esta página e voltar ao jogo.").into_response()
}

async fn poll(State(st): State<GoogleState>, Query(q): Query<HashMap<String, String>>) -> Response {
    let state = q.get("state").map(String::as_str).unwrap_or_default();
    match st.pendentes.lock().unwrap().consulta(state, Instant::now()) {
        Consulta::Desconhecido => (StatusCode::NOT_FOUND, Json(json!({ "status": "expirado" }))).into_response(),
        Consulta::Pendente => Json(json!({ "status": "pendente" })).into_response(),
        Consulta::Final(Resultado::Ok { username, token }) => {
            Json(json!({ "status": "ok", "username": username, "token": token })).into_response()
        }
        Consulta::Final(Resultado::Erro(e)) => Json(json!({ "status": "erro", "error": e })).into_response(),
        Consulta::Final(Resultado::Pendente) => unreachable!(),
    }
}

#[cfg(test)]
mod testes {
    use super::*;
    use jsonwebtoken::{encode, EncodingKey, Header};

    /// Chave RSA gerada so' pra este teste (nao e' de nenhum ambiente).
    const CHAVE_TESTE: &str = "-----BEGIN PRIVATE KEY-----
MIIEvQIBADANBgkqhkiG9w0BAQEFAASCBKcwggSjAgEAAoIBAQC8j2d+NaQ4Btdt
zdzim3lmVHelEFp08w48G/V4xDHMn48L4GcuseQWg9lJpLcBgVLx52zt4WfSiJ5U
CQcC40qTxKY79ZZdFI7it0sv2sBmERj/wkhI6WTn1fsozELb0FPlM8/4dHQw40/t
zmbj/1ay5M4xYwzqC/DM4r/J4OTqObsD/4pXKdsklL/f3eLZAv1taN2da8422ihw
tsXAOOol0bNHDNkC3ZcKWG92uzOMV636vhoxGe1qCBujdcwgW99u/aZNpf6iCzpu
0+Ll5X3yJj6osumiyMfVXLQtGijbDjC3do/G2DtMpfSp9pWbOOXHTGhhL0yoP3VJ
Rl4AT2WHAgMBAAECggEAT0KIYkjyCor1qzZJjweDdDw1PrEtFNPGPAYUnPr7Y+P+
ISPmu34TPlOV1priQkO2NfDtnOCO37SFuRVCWThpdMvIhJOq7N1jYnFhrW/qR/0P
4RItiFhzDfCobnHN+NANM98foHRzEsm6I2ALZ/NA4lnDQoc8OnVidv0pWRaroriA
xTqm5fq0d6xN5VZFv22oLxL6jLKkWDQ2MbfydqILNEMR5qK2wpDiqJMJbFsqRDNg
sPNnmDHLsO6JM7M27bALcT41qrbpov3XujHmNbyaCf1eB5aDBhihfk82dE2ho+jx
RGHKlcGdYqpaqLq1/2WOTeRwhfUljOdiTpYvkwZSAQKBgQDkqQBWjp70A05rm4K6
teMATvTkw0adRSUFm7bJAuD2SV0AGN5TPe8yf8Mst4zQ24Aof9AhxslW4wri+pom
yY1HmL+qpV7fj5SoYdPER8AUtv2OxeSErhnBnL1LGvYFL03R6Zjc153cLG7dya4X
8wd9+akpLV1OyeKgHdzSqfesxwKBgQDTGv4yjQQiLGG/RLnqV6de8o+v2bAqaaee
2vbz8XQJgr1spnaCvxk9deZjNAF1PsjUk/t1Y7j3cfdwnnu4EKyIlajvfmFS8iD5
+122sI6bwp4JiOIlM3gzPuXq6lRoM2dCZJFlQBL6UG3UpSFveZ8LqQkTBTqYx8Dx
XUq1hAJBQQKBgDNLrmW8jPAUpc0CD+uuzgPNsqZ3ICo7zrhZXEHvwWO+xXw2F/n4
lOZxPumTK6XW+AMd2SUaoSQ6vsB4k4hMAsOjAqxXeNcSMLktaQJJTk/XVT5oKoGR
RPnoHZbxr2suV3jVvJMeU62G0kAy9DkvLekWztoL8TixlYNx743az1MVAoGAfEGk
I78LEs5Kzpk2UTA8zM87MgeRALXlusQpnZaedUamFoC4uuaehaWS8QtYXFmTPkTI
OuVypvtG6Nvv+HygAVkN8cHSqU7piBqjo3eyyQ25leUjL0BnXMqF7Er3Wcn/2n11
c0JOqVWUABkeYA4XRna9Z5upTOousCL4aXUFGsECgYEA0lhyKVcOJgWDqOdacjEU
8TQe5p/1onIs5PCBzM9ZUnZGNRLI0IkywfxvF5OAPe6SZZqw3gajA/+kMhSCkZG7
9C+ryWyrBuOdohW1NLRqLUGKnSZQD5yLqmNFr7HbT1+8AjqdDf4zaxNesWx6sec7
Hx2PooAsxPNlnATcYqGFpDo=
-----END PRIVATE KEY-----";
    const MODULO_TESTE: &str = "vI9nfjWkOAbXbc3c4pt5ZlR3pRBadPMOPBv1eMQxzJ-PC-BnLrHkFoPZSaS3AYFS8eds7eFn0oieVAkHAuNKk8SmO_WWXRSO4rdLL9rAZhEY_8JISOlk59X7KMxC29BT5TPP-HR0MONP7c5m4_9WsuTOMWMM6gvwzOK_yeDk6jm7A_-KVynbJJS_393i2QL9bWjdnWvONtoocLbFwDjqJdGzRwzZAt2XClhvdrszjFet-r4aMRntaggbo3XMIFvfbv2mTaX-ogs6btPi5eV98iY-qLLposjH1Vy0LRoo2w4wt3aPxtg7TKX0qfaVmzjlx0xoYS9MqD91SUZeAE9lhw";
    const CLIENTE: &str = "123.apps.googleusercontent.com";

    fn jwks() -> Vec<Jwk> {
        vec![Jwk { kid: "k1".into(), n: MODULO_TESTE.into(), e: "AQAB".into() }]
    }

    fn agora() -> i64 {
        std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs() as i64
    }

    fn token(mudar: impl FnOnce(&mut serde_json::Value)) -> String {
        let mut claims = json!({
            "iss": "https://accounts.google.com",
            "aud": CLIENTE,
            "sub": "1100220033",
            "email": "fulano@gmail.com",
            "email_verified": true,
            "name": "Fulano",
            "nonce": "n1",
            "iat": agora(),
            "exp": agora() + 3600,
        });
        mudar(&mut claims);
        let mut cab = Header::new(jsonwebtoken::Algorithm::RS256);
        cab.kid = Some("k1".into());
        encode(&cab, &claims, &EncodingKey::from_rsa_pem(CHAVE_TESTE.as_bytes()).unwrap()).unwrap()
    }

    fn valida(t: &str) -> Result<Identidade, ErroToken> {
        valida_id_token(t, &jwks(), CLIENTE, "n1")
    }

    #[test]
    fn id_token_valido_passa() {
        let id = valida(&token(|_| {})).unwrap();
        assert_eq!(id.sub, "1100220033");
        assert_eq!(id.email.as_deref(), Some("fulano@gmail.com"));
        assert_eq!(id.nome.as_deref(), Some("Fulano"));
        // iss sem https tambem e' do Google; email_verified como string tambem.
        assert!(valida(&token(|c| {
            c["iss"] = json!("accounts.google.com");
            c["email_verified"] = json!("true");
        }))
        .is_ok());
    }

    #[test]
    fn id_token_recusado_quando_nao_bate() {
        let erro = |mudar: fn(&mut serde_json::Value)| valida(&token(mudar)).unwrap_err();
        assert!(matches!(erro(|c| c["aud"] = json!("outro-app")), ErroToken::Invalido(_)), "aud");
        assert!(matches!(erro(|c| c["iss"] = json!("https://evil.example")), ErroToken::Invalido(_)), "iss");
        assert!(matches!(erro(|c| c["exp"] = json!(agora() - 3600)), ErroToken::Invalido(_)), "exp");
        assert!(matches!(erro(|c| c["email_verified"] = json!(false)), ErroToken::Invalido(_)), "email_verified");
        assert!(matches!(erro(|c| { c.as_object_mut().unwrap().remove("email_verified"); }), ErroToken::Invalido(_)));
        assert!(matches!(erro(|c| c["nonce"] = json!("outro")), ErroToken::Invalido(_)), "nonce");
        // Chave que nao esta' no JWKS.
        let outro_kid = valida_id_token(&token(|_| {}), &[Jwk { kid: "k2".into(), ..jwks()[0].clone() }], CLIENTE, "n1");
        assert_eq!(outro_kid, Err(ErroToken::ChaveDesconhecida));
        // Assinatura adulterada.
        let t = token(|_| {});
        let (resto, assinatura) = t.rsplit_once('.').unwrap();
        let mut a = assinatura.to_string();
        let ultimo = if a.ends_with('A') { 'B' } else { 'A' };
        a.pop();
        a.push(ultimo);
        assert!(valida(&format!("{resto}.{a}")).is_err());
    }

    #[test]
    fn state_e_de_uso_unico_e_expira() {
        let t0 = Instant::now();
        let mut p = Pendentes::default();
        p.novo("s1".into(), "v1".into(), "n1".into(), t0).unwrap();
        assert_eq!(p.consulta("s1", t0), Consulta::Pendente);
        assert_eq!(p.consulta("nao-existe", t0), Consulta::Desconhecido);

        assert_eq!(p.para_callback("s1", t0), Some(("v1".into(), "n1".into())));
        assert_eq!(p.para_callback("s1", t0), None, "callback repetido nao reusa o verifier");

        let ok = Resultado::Ok { username: "Fulano".into(), token: "tok".into() };
        p.conclui("s1", ok.clone());
        assert_eq!(p.consulta("s1", t0), Consulta::Final(ok));
        assert_eq!(p.consulta("s1", t0), Consulta::Desconhecido, "sessao sai uma vez so'");

        p.novo("s2".into(), "v2".into(), "n2".into(), t0).unwrap();
        let depois = t0 + VALIDADE_STATE + Duration::from_secs(1);
        assert_eq!(p.para_callback("s2", depois), None, "expirado");
        assert_eq!(p.consulta("s2", depois), Consulta::Desconhecido);
    }

    #[test]
    fn pkce_hash_e_url() {
        // Exemplo do RFC 7636, apendice B.
        assert_eq!(
            desafio_pkce("dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk"),
            "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM"
        );
        // Igual ao `server::auth::hash_do_token`.
        assert_eq!(hash_do_token("abc"), "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad");
        assert_eq!(formulario(&[("scope", "openid email"), ("r", "https://a/b?c")]), "scope=openid%20email&r=https%3A%2F%2Fa%2Fb%3Fc");
        let s = aleatorio(32);
        assert_eq!(s.len(), 43);
        assert!(s.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_'));
    }
}
