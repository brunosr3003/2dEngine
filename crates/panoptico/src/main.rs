//! **Panoptico** — o olho de cima do Tempest.
//!
//! Um processo separado, so' de leitura, que junta o estado de TODOS os canais
//! e o que os bancos sabem, e serve uma tela onde da' pra ver o jogo inteiro:
//! o mapa ao vivo, jogadores, economia, mercado, dungeons, atividade
//! (telemetria agregada por minuto), missoes e infraestrutura.
//!
//! ── Por que processo separado ────────────────────────────────────────────
//!
//! O jogo roda um processo por canal, e cada um simula 30 vezes por segundo
//! dentro de um orcamento de 33 ms. Servir painel de dentro desse laco seria
//! deixar uma aba de navegador competir com o tick. Aqui o caminho e' o
//! contrario: cada canal PUBLICA um retrato ja' pronto a cada 200 ms e grava a
//! telemetria por minuto, e o panoptico so' busca e junta.
//!
//! ── Por que ele depende de `shared` ──────────────────────────────────────
//!
//! O mapa nao e' imagem guardada em lugar nenhum: o panoptico GERA a ilha do
//! mesmo gerador que o jogo usa. E os nomes de desenho (dungeons, missoes,
//! chaves) saem do mesmo codigo do jogo.
//!
//! ── Cuidado ──────────────────────────────────────────────────────────────
//!
//! Ele ve' conta, ouro e posicao de todo mundo. Sem sessao (senha forte,
//! cookie HttpOnly) nada passa — ver `auth`.

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Instant;

use axum::extract::{ConnectInfo, Path, Query, Request, State};
use axum::http::{header, HeaderMap, HeaderValue, StatusCode};
use axum::middleware::Next;
use axum::response::{Html, IntoResponse, Redirect, Response};
use axum::routing::{get, post};
use axum::Form;
use tokio::sync::RwLock;

mod auth;
mod economia;
mod mapa;
mod observa;

#[derive(Clone)]
struct Estado {
    pool: Arc<sqlx::PgPool>,
    /// Banco central (mercado global, TP). `None` = sem `DATABASE_URL_CENTRAL`.
    central: Option<Arc<sqlx::PgPool>>,
    auth: Arc<auth::Auth>,
    /// Token que os processos de jogo exigem no `/estado`. Sem ele o mapa ao
    /// vivo fica mudo, mas o resto (bancos, telemetria) funciona.
    token_jogo: Option<String>,
    /// Onde falar com os canais quando o `host` anunciado e' o publico
    /// (producao: "mmo.brunji.com.br:9000" e o painel do canal em 127.0.0.1).
    host_dos_canais: Option<String>,
    /// PNG por zona, gerado uma vez.
    mapas: Arc<RwLock<HashMap<String, Arc<Vec<u8>>>>>,
    http: hyper_util::client::legacy::Client<
        hyper_util::client::legacy::connect::HttpConnector,
        String,
    >,
}

fn env(nome: &str) -> Option<String> {
    std::env::var(nome)
        .ok()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
        )
        .init();

    // PANOPTICO_TOKEN continua valendo como senha: e' o que o run-panoptico.sh
    // antigo exporta.
    let senha = env("PANOPTICO_SENHA")
        .or_else(|| env("PANOPTICO_TOKEN"))
        .unwrap_or_default();
    let prefixo = env("PANOPTICO_PREFIXO").unwrap_or_default();
    let cookie_seguro = env("PANOPTICO_COOKIE_SEGURO").as_deref() != Some("0");
    let confiar_proxy = env("PANOPTICO_CONFIAR_PROXY").as_deref() == Some("1");
    let auth = Arc::new(auth::Auth::novo(
        senha,
        &prefixo,
        cookie_seguro,
        confiar_proxy,
    )?);

    let token_jogo = env("MMO_ADMIN_TOKEN").filter(|t| t.len() >= 16);
    if token_jogo.is_none() {
        tracing::warn!(
            "MMO_ADMIN_TOKEN ausente ou curto: o mapa ao vivo nao vai buscar retrato dos canais"
        );
    }

    let url = env("DATABASE_URL")
        .unwrap_or_else(|| "postgres://solar:solar_dev_123@localhost:5432/mmo_dev".into());
    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(6)
        .connect(&url)
        .await?;
    let central = match env("DATABASE_URL_CENTRAL") {
        Some(u) => match sqlx::postgres::PgPoolOptions::new()
            .max_connections(3)
            .connect(&u)
            .await
        {
            Ok(p) => Some(Arc::new(p)),
            Err(e) => {
                tracing::warn!(
                    "banco central indisponivel ({e}): aba de mercado sem o lado central"
                );
                None
            }
        },
        None => None,
    };

    let st = Estado {
        pool: Arc::new(pool),
        central,
        auth: auth.clone(),
        token_jogo,
        host_dos_canais: env("PANOPTICO_HOST_CANAIS"),
        mapas: Arc::new(RwLock::new(HashMap::new())),
        http: hyper_util::client::legacy::Client::builder(hyper_util::rt::TokioExecutor::new())
            .build_http(),
    };

    let p = |c: &str| auth.caminho(c);
    let mut app = axum::Router::new()
        .route(&p("/"), get(pagina))
        .route(&p("/camera.js"), get(camera_js))
        .route(&p("/login"), get(login_form).post(login))
        .route(&p("/sair"), post(sair))
        .route(&p("/api/mundo"), get(mundo))
        .route(&p("/api/mapa/:zona"), get(mapa_png))
        .route(&p("/api/terreno/:zona"), get(terreno_3d))
        .route(&p("/api/economia"), get(economia))
        .route(&p("/api/jogadores"), get(jogadores))
        .route(&p("/api/mercado"), get(mercado))
        .route(&p("/api/loja"), get(loja))
        .route(&p("/api/dungeons"), get(dungeons))
        .route(&p("/api/atividade"), get(atividade))
        .route(&p("/api/missoes"), get(missoes))
        .route(&p("/api/itens"), get(itens))
        .route(&p("/api/infra"), get(infra))
        .route(&p("/api/correio"), post(correio))
        .route(&p("/api/catalogo"), get(catalogo))
        .route(&p("/api/icones.png"), get(icones_png));
    if !auth.prefixo.is_empty() {
        // "/panoptico" sem barra: a pagina usa caminhos relativos e precisa da barra.
        let destino = p("/");
        app = app.route(
            &auth.prefixo,
            get(move || async move { Redirect::permanent(&destino) }),
        );
    }
    let app = app
        .layer(axum::middleware::from_fn_with_state(st.clone(), porteiro))
        .with_state(st);

    let bind = env("PANOPTICO_WEB_BIND").unwrap_or_else(|| "127.0.0.1:8090".into());
    let addr: SocketAddr = bind.parse()?;
    if !addr.ip().is_loopback() {
        tracing::warn!("panoptico escutando fora do loopback ({addr}): deixe atras de proxy HTTPS");
    }
    tracing::info!("panoptico em http://{addr}{}", p("/"));
    let escuta = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(
        escuta,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .await?;
    Ok(())
}

// ─────────────────────────────── porta ───────────────────────────────

/// Nada passa sem sessao, exceto a tela de login. E toda resposta leva os
/// cabecalhos de defesa.
async fn porteiro(State(st): State<Estado>, req: Request, next: Next) -> Response {
    let caminho = req.uri().path().to_string();
    let livre = caminho == st.auth.caminho("/login")
        || (!st.auth.prefixo.is_empty() && caminho == st.auth.prefixo);
    let dentro =
        auth::token_do_cookie(req.headers()).is_some_and(|t| st.auth.valida(&t, Instant::now()));
    let mut resp = if livre || dentro {
        next.run(req).await
    } else if caminho.starts_with(&st.auth.caminho("/api/")) {
        (StatusCode::UNAUTHORIZED, "entre primeiro").into_response()
    } else {
        Redirect::to(&st.auth.caminho("/login")).into_response()
    };
    let h = resp.headers_mut();
    let fixos = [
        ("x-frame-options", "DENY"),
        ("x-content-type-options", "nosniff"),
        ("referrer-policy", "no-referrer"),
        (
            "content-security-policy",
            "default-src 'self'; img-src 'self' data:; style-src 'self' 'unsafe-inline'; script-src 'self' 'unsafe-inline'; connect-src 'self'; frame-ancestors 'none'; form-action 'self'; base-uri 'none'",
        ),
    ];
    for (k, v) in fixos {
        h.insert(k, HeaderValue::from_static(v));
    }
    let e_png = h
        .get(header::CONTENT_TYPE)
        .is_some_and(|v| v.as_bytes() == b"image/png");
    if !e_png {
        h.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    }
    resp
}

async fn login_form(State(st): State<Estado>) -> Html<String> {
    Html(auth::pagina_de_login(&st.auth.caminho("/login"), None))
}

async fn login(
    State(st): State<Estado>,
    ConnectInfo(par): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Form(f): Form<HashMap<String, String>>,
) -> Response {
    let ip = auth::ip_do_pedido(&headers, par, st.auth.confiar_proxy);
    let agora = Instant::now();
    let acao = st.auth.caminho("/login");
    if st.auth.bloqueado(&ip, agora) {
        tracing::warn!("panoptico: login travado para {ip}");
        return (
            StatusCode::TOO_MANY_REQUESTS,
            Html(auth::pagina_de_login(
                &acao,
                Some("Muitas tentativas. Espere 15 minutos."),
            )),
        )
            .into_response();
    }
    let senha = f.get("senha").map(String::as_str).unwrap_or("");
    if !st.auth.confere(senha) {
        st.auth.falhou(&ip, agora);
        tracing::warn!("panoptico: senha errada de {ip}");
        return (
            StatusCode::UNAUTHORIZED,
            Html(auth::pagina_de_login(&acao, Some("Senha errada."))),
        )
            .into_response();
    }
    st.auth.limpar_falhas(&ip);
    match st.auth.nova_sessao(agora) {
        Ok(token) => {
            tracing::info!("panoptico: sessao aberta para {ip}");
            (
                [(header::SET_COOKIE, st.auth.cookie(&token))],
                Redirect::to(&st.auth.caminho("/")),
            )
                .into_response()
        }
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("sem aleatoriedade: {e}"),
        )
            .into_response(),
    }
}

async fn sair(State(st): State<Estado>, headers: HeaderMap) -> Response {
    if let Some(t) = auth::token_do_cookie(&headers) {
        st.auth.encerrar(&t);
    }
    (
        [(header::SET_COOKIE, st.auth.cookie_apagar())],
        Redirect::to(&st.auth.caminho("/login")),
    )
        .into_response()
}

async fn pagina() -> Html<&'static str> {
    Html(include_str!("../estatico/index.html"))
}

async fn camera_js() -> impl IntoResponse {
    ([(header::CONTENT_TYPE, "text/javascript; charset=utf-8")], include_str!("../estatico/camera.js"))
}

// ─────────────────────────────── canais ───────────────────────────────

/// Onde o panoptico de um canal escuta.
///
/// Primeiro o que o PROPRIO canal publicou no heartbeat (`channels.painel`):
/// ele sabe em que porta subiu, e ninguem precisa deduzir. Vazio quando o
/// canal e' velho (ainda nao publica a coluna) ou subiu sem painel.
///
/// Vazio cai na deducao antiga: porta do jogo mais `PANOPTICO_OFFSET`. Ela so'
/// funciona enquanto o endereco anunciado termina em `:porta` — com a zona
/// atras de um proxy (`host/z/ilha/1`) nao ha' porta pra somar, e ai o painel
/// depende mesmo do que o canal publicou.
///
/// Nos dois casos `PANOPTICO_HOST_CANAIS` troca o host, porque o anunciado e'
/// o publico; e `0.0.0.0` vira `127.0.0.1`, que e' onde se ESCUTA, nao um
/// endereco pra onde falar.
fn endereco_do_painel(
    publicado: &str,
    host_do_jogo: &str,
    host_override: Option<&str>,
) -> Option<String> {
    if !publicado.is_empty() {
        let (h, porta) = publicado.rsplit_once(':')?;
        let h = match host_override {
            Some(o) => o,
            None if h == "0.0.0.0" || h.is_empty() => "127.0.0.1",
            None => h,
        };
        return Some(format!("{h}:{porta}"));
    }
    endereco_por_convencao(host_do_jogo, host_override)
}

fn endereco_por_convencao(host_do_jogo: &str, host_override: Option<&str>) -> Option<String> {
    let offset: u16 = std::env::var("PANOPTICO_OFFSET")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(1000);
    let (h, porta) = host_do_jogo.rsplit_once(':')?;
    let porta: u16 = porta.parse().ok()?;
    let h = match host_override {
        Some(o) => o,
        None if h == "0.0.0.0" || h.is_empty() => "127.0.0.1",
        None => h,
    };
    Some(format!("{h}:{}", porta.checked_add(offset)?))
}

/// Canais vivos com o retrato de cada um (em paralelo).
async fn canais_com_retrato(st: &Estado) -> Vec<serde_json::Value> {
    let linhas = sqlx::query_as::<_, (String, String, i32, i32, String, f32, String)>(
        "SELECT id, host, players, capacity, zone, tick_p99_ms, painel
           FROM channels
          WHERE updated > NOW() - INTERVAL '15 seconds'
          ORDER BY id ASC",
    )
    .fetch_all(&*st.pool)
    .await
    .unwrap_or_default();

    // Todos os canais em PARALELO. Em serie, onze canais com um lento no meio
    // dariam um painel que anda no ritmo do pior deles.
    let mut tarefas = Vec::new();
    for (id, host, jogadores, capacidade, zona, tick, publicado) in linhas {
        let st = st.clone();
        tarefas.push(tokio::spawn(async move {
            let painel = endereco_do_painel(&publicado, &host, st.host_dos_canais.as_deref());
            let retrato = match &painel {
                Some(p) => buscar_retrato(&st, p).await,
                None => None,
            };
            serde_json::json!({
                "id": id,
                "host": host,
                "painel": painel,
                "jogadores_db": jogadores,
                "capacidade": capacidade,
                "zona": zona,
                "tick_ms": tick,
                // `null` quando o canal nao respondeu: a tela mostra o canal
                // como MUDO em vez de somir com ele.
                "retrato": retrato,
            })
        }));
    }
    let mut canais = Vec::new();
    for t in tarefas {
        if let Ok(v) = t.await {
            canais.push(v);
        }
    }
    canais
}

async fn mundo(State(st): State<Estado>) -> impl IntoResponse {
    axum::Json(serde_json::json!({ "canais": canais_com_retrato(&st).await }))
}

/// Nome -> canal de quem esta' logado agora.
async fn quem_esta_online(st: &Estado) -> HashMap<String, String> {
    let mut online = HashMap::new();
    for c in canais_com_retrato(st).await {
        let canal = c
            .get("id")
            .and_then(|v| v.as_str())
            .unwrap_or("?")
            .to_string();
        if let Some(js) = c
            .get("retrato")
            .and_then(|r| r.get("jogadores"))
            .and_then(|v| v.as_array())
        {
            for j in js {
                if let Some(n) = j.get("nome").and_then(|v| v.as_str()) {
                    online.insert(n.to_string(), canal.clone());
                }
            }
        }
    }
    online
}

async fn buscar_retrato(st: &Estado, painel: &str) -> Option<serde_json::Value> {
    use http_body_util::BodyExt;
    let token = st.token_jogo.as_deref()?;
    let uri = format!("http://{painel}/estado?token={token}");
    let req = hyper::Request::builder()
        .uri(uri)
        .body(String::new())
        .ok()?;
    let resp = tokio::time::timeout(std::time::Duration::from_millis(800), st.http.request(req))
        .await
        .ok()?
        .ok()?;
    if !resp.status().is_success() {
        return None;
    }
    let corpo = resp.into_body().collect().await.ok()?.to_bytes();
    serde_json::from_slice(&corpo).ok()
}

// ─────────────────────────────── abas ───────────────────────────────

fn horas(q: &HashMap<String, String>, padrao: i32) -> i32 {
    q.get("horas")
        .and_then(|v| v.parse().ok())
        .unwrap_or(padrao)
}

/// A economia: o que o desenho preve ao lado do que o banco mede.
async fn economia(State(st): State<Estado>) -> impl IntoResponse {
    // Ouro em MAO: so' quem esta' logado. O do banco inclui quem esta' fora, e
    // a diferenca entre os dois e' informacao — ouro parado nao circula.
    let mut ouro_online = 0u64;
    for c in canais_com_retrato(&st).await {
        if let Some(js) = c
            .get("retrato")
            .and_then(|r| r.get("jogadores"))
            .and_then(|v| v.as_array())
        {
            ouro_online += js
                .iter()
                .filter_map(|j| j.get("ouro").and_then(|v| v.as_u64()))
                .sum::<u64>();
        }
    }
    axum::Json(economia::levantar(&st.pool, ouro_online).await)
}

async fn jogadores(State(st): State<Estado>) -> impl IntoResponse {
    let online = quem_esta_online(&st).await;
    axum::Json(observa::jogadores(&st.pool, &online).await)
}

/// **Correio oficial pelo painel.** Manda carta com anexo pra um personagem
/// (ou pra todos), pela MESMA transacao auditavel do envio de dentro do jogo
/// (`correio::enviar`): campanha registrada, destinatarios fotografados,
/// `envio` como chave de idempotencia e anexo que nunca entrega metade.
///
/// Autor e conta: o painel nao tem personagem logado, entao ele assina como
/// `autor` do corpo — e esse nome precisa ter cargo (`social_staff`), igual
/// no jogo. O panoptico ja' esta' atras de senha, sessao e freio de
/// tentativas (`auth.rs`); o cargo e' a SEGUNDA tranca, a que diz de quem foi
/// o envio no registro.
async fn correio(
    State(st): State<Estado>,
    axum::Json(c): axum::Json<CorreioPedido>,
) -> impl IntoResponse {
    let conta: Option<i64> = match sqlx::query_scalar(
        "SELECT account_id FROM characters WHERE lower(name) = lower($1)",
    )
    .bind(&c.autor)
    .fetch_optional(&*st.pool)
    .await
    {
        Ok(v) => v.flatten(),
        Err(e) => return erro_do_correio(format!("banco: {e}")),
    };
    let Some(conta) = conta else {
        return erro_do_correio(format!("personagem '{}' nao encontrado", c.autor));
    };
    let pedido = shared::social::Pedido::EnviarOficial {
        envio: c.envio,
        para: c.para.filter(|p| !p.trim().is_empty()),
        assunto: c.assunto,
        texto: c.texto,
        anexos: c
            .anexos
            .into_iter()
            .map(|a| shared::social::Anexo {
                item_id: a.item_id,
                qtd: a.qtd,
                // O painel manda item PELADO de proposito: ele nao rola
                // instancia, e um equipamento sem ela nasce Comum Tier I no
                // primeiro login (`craft::instancia_inicial`).
                instance: None,
            })
            .collect(),
    };
    match correio::enviar(&st.pool, &c.autor, conta, &pedido).await {
        Ok(n) => (
            axum::http::StatusCode::OK,
            axum::Json(serde_json::json!({ "ok": true, "destinatarios": n })),
        ),
        Err(e) => erro_do_correio(e.to_string()),
    }
}

/// The CLIENT's item icon atlas, same PNG and same index (generated by
/// `tools/icones/gerar_icones.py`): the mail attachment picker shows the
/// drawing the player will see in the bag.
#[path = "../../client/src/icones_indice.rs"]
#[allow(dead_code)]
mod icones_indice;

async fn icones_png() -> impl IntoResponse {
    (
        [
            (header::CONTENT_TYPE, "image/png"),
            (header::CACHE_CONTROL, "private, max-age=3600"),
        ],
        &include_bytes!("../../../assets/icones/itens.png")[..],
    )
}

/// Catalog for the attachment picker: id, DB name (English, the source), the
/// Portuguese name from the game's own dictionary, and the icon's atlas cell
/// (`null` = no drawing; the client falls back to its substitute too).
async fn catalogo(State(st): State<Estado>) -> impl IntoResponse {
    use shared::idioma::{tr_em, Idioma};
    let linhas: Vec<(i32, String, i32, Option<String>)> = sqlx::query_as(
        "SELECT id, name, stack_max, equip_slot FROM items WHERE active ORDER BY id",
    )
    .fetch_all(&*st.pool)
    .await
    .unwrap_or_default();
    let itens: Vec<serde_json::Value> = linhas
        .into_iter()
        .map(|(id, nome, pilha, slot)| {
            let celula = icones_indice::ICONES
                .binary_search_by_key(&(id as u16), |e| e.0)
                .ok()
                .map(|k| icones_indice::ICONES[k].1);
            serde_json::json!({
                "id": id,
                "nome": nome,
                "pt": tr_em(Idioma::Pt, &nome),
                "pilha": pilha,
                "slot": slot,
                "celula": celula,
            })
        })
        .collect();
    axum::Json(serde_json::json!({
        "lado": icones_indice::LADO,
        "colunas": icones_indice::COLUNAS,
        "largura": icones_indice::LARGURA,
        "altura": icones_indice::ALTURA,
        "itens": itens,
    }))
}

fn erro_do_correio(texto: String) -> (axum::http::StatusCode, axum::Json<serde_json::Value>) {
    tracing::warn!("correio pelo painel recusado: {texto}");
    (
        axum::http::StatusCode::BAD_REQUEST,
        axum::Json(serde_json::json!({ "ok": false, "erro": texto })),
    )
}

#[derive(serde::Deserialize)]
struct CorreioPedido {
    /// Personagem que ASSINA o envio — precisa ter cargo em `social_staff`.
    autor: String,
    /// Chave de idempotencia: reenviar o mesmo id nao duplica.
    envio: String,
    /// `None` ou vazio = todo mundo.
    para: Option<String>,
    assunto: String,
    texto: String,
    #[serde(default)]
    anexos: Vec<CorreioAnexo>,
}

#[derive(serde::Deserialize)]
struct CorreioAnexo {
    item_id: u16,
    qtd: u32,
}

async fn mercado(State(st): State<Estado>) -> impl IntoResponse {
    axum::Json(observa::mercado(&st.pool, st.central.as_deref()).await)
}

async fn loja(State(st): State<Estado>) -> impl IntoResponse {
    axum::Json(observa::loja(&st.pool, st.central.as_deref()).await)
}

async fn dungeons(
    State(st): State<Estado>,
    Query(q): Query<HashMap<String, String>>,
) -> impl IntoResponse {
    axum::Json(observa::dungeons(&st.pool, horas(&q, 168)).await)
}

async fn atividade(
    State(st): State<Estado>,
    Query(q): Query<HashMap<String, String>>,
) -> impl IntoResponse {
    axum::Json(observa::atividade(&st.pool, horas(&q, 24)).await)
}

async fn missoes(
    State(st): State<Estado>,
    Query(q): Query<HashMap<String, String>>,
) -> impl IntoResponse {
    axum::Json(observa::missoes(&st.pool, horas(&q, 168)).await)
}

async fn itens(
    State(st): State<Estado>,
    Query(q): Query<HashMap<String, String>>,
) -> impl IntoResponse {
    axum::Json(observa::itens(&st.pool, horas(&q, 168)).await)
}

async fn infra(State(st): State<Estado>) -> impl IntoResponse {
    axum::Json(observa::infra(&st.pool, st.central.as_deref()).await)
}

async fn mapa_png(State(st): State<Estado>, Path(zona): Path<String>) -> impl IntoResponse {
    let zona = zona.trim_end_matches(".png").to_string();
    if let Some(png) = st.mapas.read().await.get(&zona).cloned() {
        return png_resposta(png);
    }
    let Some(def) = shared::terreno::def_da_zona(&zona) else {
        return (StatusCode::NOT_FOUND, "zona desconhecida").into_response();
    };
    // Desenhar a ilha grande leva ~1 s e queima um nucleo. Fora do executor
    // async, senao ele para de atender enquanto pinta.
    let png = match tokio::task::spawn_blocking(move || mapa::pintar(def)).await {
        Ok(Ok(p)) => Arc::new(p),
        Ok(Err(e)) => return (StatusCode::INTERNAL_SERVER_ERROR, format!("{e}")).into_response(),
        Err(e) => return (StatusCode::INTERNAL_SERVER_ERROR, format!("{e}")).into_response(),
    };
    st.mapas.write().await.insert(zona, png.clone());
    png_resposta(png)
}

async fn terreno_3d(
    Path(zona): Path<String>,
    Query(q): Query<HashMap<String, String>>,
) -> impl IntoResponse {
    let Some(def) = shared::terreno::def_da_zona(&zona) else {
        return (StatusCode::NOT_FOUND, "zona desconhecida").into_response();
    };
    let pos = |nome: &str| -> i32 {
        q.get(nome)
            .and_then(|v| v.parse::<f32>().ok())
            .filter(|v| v.is_finite())
            .unwrap_or(0.0)
            .clamp(-(def.raio_blocos as f32), def.raio_blocos as f32)
            .round() as i32
    };
    let (x, z) = (pos("x"), pos("z"));
    match tokio::task::spawn_blocking(move || mapa::recorte_3d(def, x, z)).await {
        Ok(recorte) => axum::Json(recorte).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, format!("{e}")).into_response(),
    }
}

fn png_resposta(png: Arc<Vec<u8>>) -> Response {
    (
        [
            (header::CONTENT_TYPE, "image/png"),
            (header::CACHE_CONTROL, "private, max-age=86400"),
        ],
        png.to_vec(),
    )
        .into_response()
}

#[cfg(test)]
mod testes {
    use super::endereco_do_painel;

    /// Sem a coluna publicada (canal velho), a deducao antiga vale.
    #[test]
    fn painel_do_canal_usa_o_host_certo() {
        assert_eq!(
            endereco_do_painel("", "0.0.0.0:9200", None).as_deref(),
            Some("127.0.0.1:10200")
        );
        assert_eq!(
            endereco_do_painel("", "10.0.0.5:9000", None).as_deref(),
            Some("10.0.0.5:10000")
        );
        assert_eq!(
            endereco_do_painel("", "mmo.brunji.com.br:9000", Some("127.0.0.1")).as_deref(),
            Some("127.0.0.1:10000")
        );
        assert_eq!(endereco_do_painel("", "sem-porta", None), None);
    }

    /// O que o canal publicou vence a deducao — e' o unico caminho que sobra
    /// quando o endereco publico e' um CAMINHO, sem porta pra somar offset.
    #[test]
    fn o_publicado_vence_a_deducao() {
        assert_eq!(
            endereco_do_painel("127.0.0.1:10000", "mmo.brunji.com.br/z/ilha_inicial/1", None)
                .as_deref(),
            Some("127.0.0.1:10000")
        );
        assert_eq!(
            endereco_do_painel("0.0.0.0:10100", "mmo.brunji.com.br/z/ilha_gelo", None).as_deref(),
            Some("127.0.0.1:10100"),
            "0.0.0.0 e' onde se escuta, nao um endereco"
        );
        assert_eq!(
            endereco_do_painel("127.0.0.1:10000", "qualquer", Some("10.0.0.7")).as_deref(),
            Some("10.0.0.7:10000")
        );
    }

    /// Sem painel dos dois lados o canal fica MUDO no painel — melhor que
    /// bater numa porta que nunca respondeu (a Geleira e a Colonia sobem sem
    /// `PANOPTICO_BIND`).
    #[test]
    fn sem_painel_e_sem_porta_da_none() {
        assert_eq!(
            endereco_do_painel("", "mmo.brunji.com.br/z/ilha_gelo", None),
            None
        );
    }
}
