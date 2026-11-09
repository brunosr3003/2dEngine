//! GLOBAL ANNOUNCEMENTS: a line the staff types in the Panopticon and every
//! online player sees at once, on every channel, with no restart (owner,
//! 09/11/2026: "a global message that will display for every online player,
//! whenever I want, and say it in Panopticon without needing restart").
//!
//! The Panopticon only INSERTs into `avisos_globais`. Each game process polls
//! the table every `INTERVALO` and queues what is new; the tick drains the
//! queue and sends `ServerMessage::AvisoGlobal` to everyone logged in. On
//! start a process skips what is already there, so a restart never replays
//! an old announcement.

use std::time::Duration;

use sqlx::PgPool;

const INTERVALO: Duration = Duration::from_secs(3);

static FILA: parking_lot::Mutex<Vec<String>> = parking_lot::Mutex::new(Vec::new());

pub const CRIAR_TABELA: &str = "CREATE TABLE IF NOT EXISTS avisos_globais (
    id BIGSERIAL PRIMARY KEY,
    texto TEXT NOT NULL,
    autor TEXT NOT NULL DEFAULT '',
    criado TIMESTAMPTZ NOT NULL DEFAULT NOW()
)";

/// Queues a message for this process's players (bonus-event reminders).
pub fn enfileirar(texto: String) {
    FILA.lock().push(texto);
}

/// What arrived since the last tick.
pub fn tomar() -> Vec<String> {
    std::mem::take(&mut *FILA.lock())
}

pub async fn iniciar(pool: PgPool) -> anyhow::Result<()> {
    sqlx::query(CRIAR_TABELA).execute(&pool).await?;
    let mut ultimo: i64 = sqlx::query_scalar("SELECT COALESCE(MAX(id), 0) FROM avisos_globais")
        .fetch_one(&pool)
        .await?;
    tokio::spawn(async move {
        let mut iv = tokio::time::interval(INTERVALO);
        loop {
            iv.tick().await;
            let novos: Result<Vec<(i64, String)>, _> = sqlx::query_as(
                "SELECT id, texto FROM avisos_globais WHERE id > $1 ORDER BY id LIMIT 20",
            )
            .bind(ultimo)
            .fetch_all(&pool)
            .await;
            match novos {
                Ok(v) => {
                    for (id, texto) in v {
                        ultimo = ultimo.max(id);
                        tracing::info!("global announcement #{id}: {texto}");
                        FILA.lock().push(texto);
                    }
                }
                Err(e) => tracing::warn!("avisos_globais: {e}"),
            }
        }
    });
    Ok(())
}
