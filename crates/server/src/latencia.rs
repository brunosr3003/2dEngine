//! Who plays with high latency, from where (owner, 11/11/2026, after a Play
//! review: "ping 450 ms, sometimes 850 or 1000; I lost dungeon runs to
//! disconnections"). Three numbers per sample tell network from device:
//!
//! - `rede_ms`: the WebSocket ping/pong measured HERE, in the connection's
//!   task — pure network (plus the client's network thread), no frames.
//! - `jogo_ms`: the ping the player SEES (client `Ping`/`Pong`, frame to
//!   frame), reported by the client (`ClientMessage::Latencia`).
//! - `fps`: the client's frame rate. High `jogo_ms` with low `rede_ms` and
//!   low `fps` = a slow phone, not the internet.
//!
//! Rows go to `latencias` (realm DB): every sample that is bad, and one
//! baseline per session every few minutes. The IP is the real one from
//! nginx (`X-Real-IP`); no lookup is made to any outside service.

use std::collections::HashMap;
use std::net::SocketAddr;
use std::time::Instant;

const CRIAR: &str = "CREATE TABLE IF NOT EXISTS latencias (
    id BIGSERIAL PRIMARY KEY,
    quando TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    conta BIGINT,
    personagem TEXT NOT NULL DEFAULT '',
    canal TEXT NOT NULL DEFAULT '',
    ip TEXT NOT NULL DEFAULT '',
    plataforma TEXT NOT NULL DEFAULT '',
    rede_ms INT,
    jogo_ms INT NOT NULL,
    fps INT NOT NULL,
    ruim BOOLEAN NOT NULL
)";

/// A sample is "bad" from these on.
pub const REDE_RUIM_MS: u32 = 250;
pub const JOGO_RUIM_MS: u32 = 300;
pub const FPS_RUIM: u32 = 20;
/// At most one bad row per session this often, and one baseline row.
const RUIM_A_CADA_S: u64 = 60;
const BASE_A_CADA_S: u64 = 300;

#[derive(Default)]
struct Conexao {
    ip: String,
    rede_ms: Option<u32>,
    ultimo_ruim: Option<Instant>,
    ultima_base: Option<Instant>,
}

static CONEXOES: parking_lot::Mutex<Option<HashMap<SocketAddr, Conexao>>> = parking_lot::Mutex::new(None);

fn com<R>(f: impl FnOnce(&mut HashMap<SocketAddr, Conexao>) -> R) -> R {
    f(CONEXOES.lock().get_or_insert_with(HashMap::new))
}

/// The connection task: a new connection and its real IP.
pub fn abriu(peer: SocketAddr, ip: String) {
    com(|m| {
        m.insert(peer, Conexao { ip, ..Default::default() });
    });
}

pub fn fechou(peer: SocketAddr) {
    com(|m| {
        m.remove(&peer);
    });
}

/// The connection task measured a WebSocket ping/pong.
pub fn rede(peer: SocketAddr, ms: u32) {
    com(|m| {
        if let Some(c) = m.get_mut(&peer) {
            c.rede_ms = Some(ms);
        }
    });
}

/// Is this sample bad?
pub fn ruim(rede_ms: Option<u32>, jogo_ms: u32, fps: u32) -> bool {
    rede_ms.is_some_and(|r| r >= REDE_RUIM_MS) || jogo_ms >= JOGO_RUIM_MS || (fps > 0 && fps < FPS_RUIM)
}

/// The client reported its numbers: maybe write a row.
pub fn registrar(
    peer: SocketAddr,
    conta: Option<i64>,
    personagem: String,
    canal: String,
    plataforma: String,
    jogo_ms: u32,
    fps: u32,
) {
    let agora = Instant::now();
    let decisao = com(|m| {
        let c = m.entry(peer).or_default();
        let e_ruim = ruim(c.rede_ms, jogo_ms, fps);
        let passou = |t: Option<Instant>, s: u64| t.is_none_or(|t| agora.duration_since(t).as_secs() >= s);
        let grava = if e_ruim && passou(c.ultimo_ruim, RUIM_A_CADA_S) {
            c.ultimo_ruim = Some(agora);
            true
        } else if passou(c.ultima_base, BASE_A_CADA_S) {
            c.ultima_base = Some(agora);
            true
        } else {
            false
        };
        grava.then(|| (c.ip.clone(), c.rede_ms, e_ruim))
    });
    let Some((ip, rede_ms, e_ruim)) = decisao else { return };
    let Some(pool) = crate::chat_global::pool() else { return };
    let plataforma: String = plataforma.chars().filter(|c| c.is_ascii_alphanumeric()).take(12).collect();
    tokio::spawn(async move {
        let r = async {
            sqlx::query(CRIAR).execute(&pool).await?;
            sqlx::query(
                "INSERT INTO latencias (conta, personagem, canal, ip, plataforma, rede_ms, jogo_ms, fps, ruim) \
                 VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)",
            )
            .bind(conta)
            .bind(&personagem)
            .bind(&canal)
            .bind(&ip)
            .bind(&plataforma)
            .bind(rede_ms.map(|v| v.min(60_000) as i32))
            .bind(jogo_ms.min(60_000) as i32)
            .bind(fps.min(1_000) as i32)
            .bind(e_ruim)
            .execute(&pool)
            .await
        }
        .await;
        if let Err(e) = r {
            tracing::warn!("latency sample not saved: {e}");
        }
    });
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn o_que_e_ruim() {
        assert!(!ruim(Some(40), 80, 60));
        assert!(ruim(Some(300), 80, 60), "network");
        assert!(ruim(Some(40), 450, 60), "what the player sees");
        assert!(ruim(Some(40), 80, 12), "slow phone");
        assert!(!ruim(None, 80, 0), "no fps reported yet");
    }
}
