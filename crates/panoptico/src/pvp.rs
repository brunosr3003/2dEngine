//! The "pvp" tab: two characters, one fight, CURRENT vs PROPOSED rules.
//!
//! The panel does not simulate anything itself. The evaluation lives in the
//! game server (`server::pvp_sim`), because it has to run the game's own
//! damage code; every game unit serves it on its admin endpoint as
//! `GET /pvp?token=..&a=..&b=..&<rules>`, reading the characters from the realm
//! database. Any live unit gives the same answer, so this proxies to the
//! first one that responds.

use std::collections::HashMap;

use axum::extract::{Query, State};
use axum::http::{header, StatusCode};
use axum::response::{IntoResponse, Response};

use crate::Estado;

/// Percent-encodes a query component (RFC 3986 unreserved set kept).
fn cod(s: &str) -> String {
    let mut o = String::with_capacity(s.len());
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'~') {
            o.push(b as char);
        } else {
            o.push_str(&format!("%{b:02X}"));
        }
    }
    o
}

/// The parameters the game route understands. Anything else is dropped.
const PARAMS: [&str; 13] = [
    "a",
    "b",
    "skill_mult",
    "basic_mult",
    "cap_pct",
    "level_penalty_per_level",
    "level_floor",
    "level_free_gap",
    "katana_execute_pvp",
    // The interleaved duel (`pvp_sim::OpcoesDoDuelo`).
    "potions",
    "hit_stun",
    "mc_runs",
    "seed",
];

/// What `/pvp_round_robin` understands.
const PARAMS_DO_TORNEIO: [&str; 4] = ["potions", "hit_stun", "mc_runs", "seed"];

/// `GET /api/pvp?a=<name>&b=<name>&<rules>` — the game unit's JSON, as is.
pub async fn avaliar(
    State(st): State<Estado>,
    Query(q): Query<HashMap<String, String>>,
) -> Response {
    repassa(&st, "pvp", &PARAMS, &q, 8).await
}

/// `GET /api/pvp_rr?<options>` — the archetype round-robin
/// (`pvp_sim::torneio_padrao`), computed by a game unit.
pub async fn torneio(
    State(st): State<Estado>,
    Query(q): Query<HashMap<String, String>>,
) -> Response {
    repassa(&st, "pvp_round_robin", &PARAMS_DO_TORNEIO, &q, 60).await
}

/// Proxies `rota` to the first live game unit that answers it.
async fn repassa(
    st: &Estado,
    rota: &str,
    params: &[&str],
    q: &HashMap<String, String>,
    prazo_s: u64,
) -> Response {
    use http_body_util::BodyExt;
    let Some(token) = st.token_jogo.as_deref() else {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            "MMO_ADMIN_TOKEN is not set: the panel cannot reach the game units",
        )
            .into_response();
    };
    let consulta: String = params
        .iter()
        .filter_map(|k| q.get(*k).map(|v| format!("&{k}={}", cod(v))))
        .collect();
    let paineis = crate::paineis_vivos(st).await;
    if paineis.is_empty() {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            "no live game unit publishes an admin endpoint",
        )
            .into_response();
    }
    for painel in paineis {
        let uri = format!("http://{painel}/{rota}?token={}{consulta}", cod(token));
        let Ok(req) = hyper::Request::builder().uri(uri).body(String::new()) else {
            continue;
        };
        let Ok(Ok(resp)) =
            tokio::time::timeout(std::time::Duration::from_secs(prazo_s), st.http.request(req)).await
        else {
            continue;
        };
        let status = resp.status();
        let Ok(corpo) = resp.into_body().collect().await.map(|c| c.to_bytes()) else {
            continue;
        };
        // An empty 404 is a unit on a build without the route: try another.
        if status == StatusCode::NOT_FOUND && corpo.is_empty() {
            continue;
        }
        let tipo = if status.is_success() {
            "application/json"
        } else {
            "text/plain; charset=utf-8"
        };
        return (status, [(header::CONTENT_TYPE, tipo)], corpo.to_vec()).into_response();
    }
    (
        StatusCode::BAD_GATEWAY,
        format!("no game unit answered /{rota} (all down, or on a build without it)"),
    )
        .into_response()
}

#[cfg(test)]
mod testes {
    #[test]
    fn codifica_nomes_e_numeros() {
        assert_eq!(super::cod("Lokiin"), "Lokiin");
        assert_eq!(super::cod("a b&c=d"), "a%20b%26c%3Dd");
        assert_eq!(super::cod("0.35"), "0.35");
    }
}
