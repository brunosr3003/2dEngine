//! Loop do mundo em tick fixo. Recebe IncomingMessages por mpsc e avanca
//! a simulacao em `TICK_RATE_HZ`. Single-threaded (proprio tokio task).

use crate::persistence::{CharacterRow, SaveBatch};
use crate::world::{AuthCtx, GameWorld, IncomingMessage};
use anyhow::Result;
use shared::TICK_DT;
use sqlx::postgres::PgPool;
use std::collections::HashMap;
use std::time::{Duration, Instant};
use tokio::sync::mpsc;

/// Intervalo em ticks entre persistencias periodicas (30s @ 30Hz).
const SAVE_INTERVAL_TICKS: u32 = 30 * 30;

pub async fn run_world_loop(
    mut rx: mpsc::UnboundedReceiver<IncomingMessage>,
    characters: HashMap<String, CharacterRow>,
    save_tx: mpsc::UnboundedSender<SaveBatch>,
    auth_pool: PgPool,
) -> Result<()> {
    // Precisa de um tx pra devolver AuthResult pro loop. Criamos um par
    // interno que e fundido com o rx original via tarefa de forward.
    let (auth_tx, mut auth_rx) = mpsc::unbounded_channel::<IncomingMessage>();
    let mut world = GameWorld::new(characters);
    world.set_auth_ctx(AuthCtx {
        pool: auth_pool,
        tx: auth_tx,
    });
    let step = Duration::from_secs_f32(TICK_DT);
    let mut next = Instant::now() + step;
    let mut save_counter: u32 = 0;
    tracing::info!("world loop started ({}ms/tick)", step.as_millis());

    loop {
        // Drena mensagens da rede.
        loop {
            match rx.try_recv() {
                Ok(IncomingMessage::Connected(h)) => world.on_connect(h),
                Ok(IncomingMessage::Disconnected(id)) => {
                    // Persiste o personagem antes de descartar a sessao.
                    if let Some(row) = world.take_character_for_disconnect(&id) {
                        let _ = save_tx.send(SaveBatch { rows: vec![row] });
                    }
                    world.on_disconnect(id);
                }
                Ok(IncomingMessage::Message(id, m)) => world.on_message(id, m),
                Ok(IncomingMessage::AuthResult(id, r)) => world.on_auth_result(id, r),
                Err(mpsc::error::TryRecvError::Empty) => break,
                Err(mpsc::error::TryRecvError::Disconnected) => {
                    tracing::warn!("all senders dropped, exiting world loop");
                    return Ok(());
                }
            }
        }

        // Drena resultados de auth (do canal interno).
        loop {
            match auth_rx.try_recv() {
                Ok(IncomingMessage::AuthResult(id, r)) => world.on_auth_result(id, r),
                Ok(_) => {} // outros variantes nao devem chegar aqui
                Err(mpsc::error::TryRecvError::Empty) => break,
                Err(mpsc::error::TryRecvError::Disconnected) => break,
            }
        }

        world.step(TICK_DT);
        world.send_snapshots();

        save_counter = save_counter.wrapping_add(1);
        if save_counter % SAVE_INTERVAL_TICKS == 0 {
            let rows = world.collect_character_rows();
            if !rows.is_empty() {
                let _ = save_tx.send(SaveBatch { rows });
            }
        }

        let now = Instant::now();
        if next > now {
            tokio::time::sleep(next - now).await;
        } else {
            // Atrasamos — pular para catch-up sem acumular infinito.
            let lag = now - next;
            if lag > step * 10 {
                tracing::warn!("world loop lag {lag:?} — resetando timer");
                next = now;
            }
        }
        next += step;
    }
}
