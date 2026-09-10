//! **Panoptico** — o olho de cima do Tempest.
//!
//! Um processo separado, so' de leitura, que junta o estado de TODOS os canais
//! e serve uma tela onde da' pra ver o mundo inteiro de uma vez: onde esta'
//! cada jogador, cada bicho, o que cada um esta' fazendo, e clicar pra ver
//! tudo que o servidor sabe sobre ele.
//!
//! ── Por que processo separado ────────────────────────────────────────────
//!
//! O jogo roda um processo por canal, e cada um simula 30 vezes por segundo
//! dentro de um orcamento de 33 ms. Servir painel de dentro desse laco seria
//! deixar uma aba de navegador competir com o tick. Aqui o caminho e' o
//! contrario: cada canal PUBLICA um retrato ja' pronto a cada 200 ms, e o
//! panoptico so' busca e junta. Painel lento nunca vira jogo lento.
//!
//! ── Por que ele depende de `shared` ──────────────────────────────────────
//!
//! O mapa nao e' imagem guardada em lugar nenhum: o panoptico GERA a ilha do
//! mesmo gerador que o jogo usa, da mesma semente, e desenha o PNG. Terreno
//! nunca viaja pela rede, nem pro jogo nem pra ca'. Se o gerador mudar, o mapa
//! muda junto — nao ha' como o painel mostrar uma ilha que nao existe mais.
//!
//! ── Cuidado ──────────────────────────────────────────────────────────────
//!
//! Ele ve' conta, ouro e posicao de todo mundo. Exige token, e nao sobe sem
//! um de pelo menos 16 caracteres.

use std::collections::HashMap;
use std::sync::Arc;

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::routing::get;
use tokio::sync::RwLock;

mod economia;
mod mapa;

#[derive(Clone)]
struct Estado {
    pool: Arc<sqlx::PgPool>,
    token: String,
    /// Token que os processos de jogo exigem.
    token_jogo: String,
    /// PNG por zona, gerado uma vez. A ilha grande leva ~1 s pra desenhar; nao
    /// da' pra refazer isso a cada F5.
    mapas: Arc<RwLock<HashMap<String, Arc<Vec<u8>>>>>,
    http: hyper_util::client::legacy::Client<
        hyper_util::client::legacy::connect::HttpConnector,
        String,
    >,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info".into()),
        )
        .init();

    let token = std::env::var("PANOPTICO_TOKEN").unwrap_or_default();
    if token.len() < 16 {
        anyhow::bail!(
            "PANOPTICO_TOKEN precisa de pelo menos 16 chars — este painel mostra \
             conta, ouro e posicao de todo mundo"
        );
    }
    let token_jogo = std::env::var("MMO_ADMIN_TOKEN").unwrap_or_default();
    if token_jogo.len() < 16 {
        anyhow::bail!("MMO_ADMIN_TOKEN precisa de pelo menos 16 chars (o token dos canais)");
    }

    let url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://solar:solar_dev_123@localhost:5432/mmo_dev".into());
    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(4)
        .connect(&url)
        .await?;

    let st = Estado {
        pool: Arc::new(pool),
        token,
        token_jogo,
        mapas: Arc::new(RwLock::new(HashMap::new())),
        http: hyper_util::client::legacy::Client::builder(hyper_util::rt::TokioExecutor::new())
            .build_http(),
    };

    let app = axum::Router::new()
        .route("/", get(pagina))
        .route("/api/mundo", get(mundo))
        .route("/api/mapa/:zona.png", get(mapa_png))
        .route("/api/economia", get(economia))
        .with_state(st);

    let bind = std::env::var("PANOPTICO_WEB_BIND").unwrap_or_else(|_| "127.0.0.1:8090".into());
    let addr: std::net::SocketAddr = bind.parse()?;
    tracing::info!("panoptico em http://{addr}/?token=...");
    let escuta = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(escuta, app).await?;
    Ok(())
}

fn autorizado(st: &Estado, q: &HashMap<String, String>) -> bool {
    q.get("token").map(String::as_str) == Some(st.token.as_str())
}

async fn pagina(
    State(st): State<Estado>,
    Query(q): Query<HashMap<String, String>>,
) -> impl IntoResponse {
    if !autorizado(&st, &q) {
        return (StatusCode::FORBIDDEN, "token").into_response();
    }
    (
        [(axum::http::header::CONTENT_TYPE, "text/html; charset=utf-8")],
        include_str!("../estatico/index.html"),
    )
        .into_response()
}

/// Onde o panoptico de um canal escuta.
///
/// Convencao e nao configuracao: a porta do painel e' a do jogo mais
/// `PANOPTICO_OFFSET` (1000 por padrao). Uma coluna a mais na tabela de canais
/// seria mais explicita e exigiria migracao pra ganhar pouco — a porta de um
/// canal ja' e' derivada assim em todo lugar.
///
/// `0.0.0.0` vira `127.0.0.1`: e' o endereco em que o processo ESCUTA, nao um
/// em que se possa falar com ele.
fn endereco_do_painel(host_do_jogo: &str) -> Option<String> {
    let offset: u16 = std::env::var("PANOPTICO_OFFSET")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(1000);
    let (h, p) = host_do_jogo.rsplit_once(':')?;
    let porta: u16 = p.parse().ok()?;
    let h = if h == "0.0.0.0" || h.is_empty() { "127.0.0.1" } else { h };
    Some(format!("{h}:{}", porta.checked_add(offset)?))
}

/// Junta o retrato de todos os canais vivos.
async fn mundo(
    State(st): State<Estado>,
    Query(q): Query<HashMap<String, String>>,
) -> impl IntoResponse {
    if !autorizado(&st, &q) {
        return (StatusCode::FORBIDDEN, "token").into_response();
    }
    let linhas = sqlx::query_as::<_, (String, String, i32, i32, String, f32)>(
        "SELECT id, host, players, capacity, zone, tick_p99_ms
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
    for (id, host, jogadores, capacidade, zona, tick) in linhas {
        let st = st.clone();
        tarefas.push(tokio::spawn(async move {
            let painel = endereco_do_painel(&host);
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
                // como MUDO em vez de somir com ele. Canal que some parece
                // canal que fechou, e a diferenca importa pra quem opera.
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
    axum::Json(serde_json::json!({ "canais": canais })).into_response()
}

/// A economia: o que o desenho preve ao lado do que o banco mede.
async fn economia(
    State(st): State<Estado>,
    Query(q): Query<HashMap<String, String>>,
) -> impl IntoResponse {
    if !autorizado(&st, &q) {
        return (StatusCode::FORBIDDEN, "token").into_response();
    }
    // Ouro em MAO: so' quem esta' logado. O do banco inclui quem esta' fora, e
    // a diferenca entre os dois e' informacao — ouro parado nao circula.
    let mut ouro_online = 0u64;
    let linhas = sqlx::query_as::<_, (String,)>(
        "SELECT host FROM channels WHERE updated > NOW() - INTERVAL '15 seconds'",
    )
    .fetch_all(&*st.pool)
    .await
    .unwrap_or_default();
    for (host,) in linhas {
        if let Some(p) = endereco_do_painel(&host) {
            if let Some(r) = buscar_retrato(&st, &p).await {
                if let Some(js) = r.get("jogadores").and_then(|v| v.as_array()) {
                    ouro_online += js
                        .iter()
                        .filter_map(|j| j.get("ouro").and_then(|v| v.as_u64()))
                        .sum::<u64>();
                }
            }
        }
    }
    axum::Json(economia::levantar(&st.pool, ouro_online).await).into_response()
}

async fn buscar_retrato(st: &Estado, painel: &str) -> Option<serde_json::Value> {
    use http_body_util::BodyExt;
    let uri = format!("http://{painel}/estado?token={}", st.token_jogo);
    let req = hyper::Request::builder()
        .uri(uri)
        .body(String::new())
        .ok()?;
    let resp = tokio::time::timeout(
        std::time::Duration::from_millis(800),
        st.http.request(req),
    )
    .await
    .ok()?
    .ok()?;
    if !resp.status().is_success() {
        return None;
    }
    let corpo = resp.into_body().collect().await.ok()?.to_bytes();
    serde_json::from_slice(&corpo).ok()
}

async fn mapa_png(
    State(st): State<Estado>,
    Path(zona): Path<String>,
    Query(q): Query<HashMap<String, String>>,
) -> impl IntoResponse {
    if !autorizado(&st, &q) {
        return (StatusCode::FORBIDDEN, "token").into_response();
    }
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

fn png_resposta(png: Arc<Vec<u8>>) -> axum::response::Response {
    (
        [
            (axum::http::header::CONTENT_TYPE, "image/png"),
            (axum::http::header::CACHE_CONTROL, "public, max-age=86400"),
        ],
        png.to_vec(),
    )
        .into_response()
}
