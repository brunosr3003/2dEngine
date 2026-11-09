//! Bonus events (`shared::eventos`), live from the `eventos_bonus` table.
//!
//! A task re-reads the active ones every `INTERVALO`; the kill code asks
//! `mult(...)`, and the tick sends the list to the players whenever it
//! changes (`versao`). Announcements: when an event starts, and again every
//! `LEMBRETE_S` while it runs — each process tells its own players, so every
//! online player hears it once, through the same queue as the Panopticon's
//! global messages.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use shared::eventos::{Abate, Escopo, Evento, Tipo, LEMBRETE_S};
use sqlx::PgPool;

const INTERVALO: Duration = Duration::from_secs(5);

static EVENTOS: parking_lot::RwLock<Vec<Evento>> = parking_lot::RwLock::new(Vec::new());
static VERSAO: AtomicU64 = AtomicU64::new(0);

pub const CRIAR_TABELA: &str = "CREATE TABLE IF NOT EXISTS eventos_bonus (
    id BIGSERIAL PRIMARY KEY,
    tipo TEXT NOT NULL,
    escopo TEXT NOT NULL,
    alvo TEXT NOT NULL DEFAULT '',
    alvo_nome TEXT NOT NULL DEFAULT '',
    mult REAL NOT NULL,
    inicio TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    fim TIMESTAMPTZ NOT NULL,
    autor TEXT NOT NULL DEFAULT '',
    cancelado BOOLEAN NOT NULL DEFAULT FALSE
)";

pub fn ativos() -> Vec<Evento> {
    EVENTOS.read().clone()
}

pub fn versao() -> u64 {
    VERSAO.load(Ordering::Relaxed)
}

fn agora() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as i64)
}

/// The multiplier of this kind for one kill (1.0 with no event).
pub fn mult(tipo: Tipo, a: &Abate) -> f32 {
    let v = EVENTOS.read();
    if v.is_empty() {
        return 1.0;
    }
    shared::eventos::mult(&v, tipo, a, agora())
}

pub async fn iniciar(pool: PgPool) -> anyhow::Result<()> {
    sqlx::query(CRIAR_TABELA).execute(&pool).await?;
    tokio::spawn(async move {
        let mut iv = tokio::time::interval(INTERVALO);
        // id -> the last reminder slot announced by THIS process.
        let mut anunciado: HashMap<i64, i64> = HashMap::new();
        loop {
            iv.tick().await;
            let linhas: Result<Vec<(i64, String, String, String, String, f32, i64, i64)>, _> = sqlx::query_as(
                "SELECT id, tipo, escopo, alvo, alvo_nome, mult, \
                 EXTRACT(EPOCH FROM inicio)::BIGINT, EXTRACT(EPOCH FROM fim)::BIGINT \
                 FROM eventos_bonus WHERE NOT cancelado AND fim > NOW() AND inicio <= NOW() ORDER BY id",
            )
            .fetch_all(&pool)
            .await;
            let linhas = match linhas {
                Ok(l) => l,
                Err(e) => {
                    tracing::warn!("eventos_bonus: {e}");
                    continue;
                }
            };
            let novos: Vec<Evento> = linhas
                .into_iter()
                .filter_map(|(id, tipo, escopo, alvo, alvo_nome, mult, ini, fim)| {
                    Some(Evento {
                        id,
                        tipo: Tipo::de_texto(&tipo)?,
                        escopo: Escopo::de_texto(&escopo)?,
                        alvo,
                        alvo_nome,
                        mult,
                        inicio_unix: ini,
                        fim_unix: fim,
                    })
                })
                .collect();
            let t = agora();
            for e in &novos {
                let fatia = (t - e.inicio_unix).max(0) / LEMBRETE_S;
                match anunciado.get(&e.id) {
                    // First time this process sees it: announce only if it
                    // JUST started (a restart mid-event waits for the next
                    // reminder instead of repeating).
                    None => {
                        if t - e.inicio_unix < 90 {
                            crate::avisos_globais::enfileirar(e.anuncio(t));
                        }
                        anunciado.insert(e.id, fatia);
                    }
                    Some(&f) if fatia > f => {
                        crate::avisos_globais::enfileirar(e.anuncio(t));
                        anunciado.insert(e.id, fatia);
                    }
                    _ => {}
                }
            }
            anunciado.retain(|id, _| novos.iter().any(|e| e.id == *id));
            let mut atual = EVENTOS.write();
            if *atual != novos {
                tracing::info!("bonus events now: {}", novos.iter().map(|e| e.titulo()).collect::<Vec<_>>().join(" | "));
                *atual = novos;
                VERSAO.fetch_add(1, Ordering::Relaxed);
            }
        }
    });
    Ok(())
}
