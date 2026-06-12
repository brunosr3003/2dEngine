//! Loop do mundo em tick fixo. Recebe IncomingMessages por mpsc e avanca
//! a simulacao em `TICK_RATE_HZ`. Single-threaded (proprio tokio task).

use crate::persistence::{CharacterRow, SaveBatch};
use crate::world::{AuthCtx, GameWorld, IncomingMessage};
use anyhow::Result;
use shared::TICK_DT;
use sqlx::postgres::PgPool;
use std::collections::HashMap;
use std::time::{Duration, Instant};
use tokio::sync::{mpsc, oneshot};

/// Intervalo em ticks entre persistencias periodicas (1s @ 30Hz).
/// Mudancas criticas (mount/dismount/equip/inventory) tambem disparam save
/// IMMEDIATE via `world.save_dirty_now()`. Combinacao garante < 1s de perda
/// em qualquer crash, evento ou desconexao.
const SAVE_INTERVAL_TICKS: u32 = 30;

pub async fn run_world_loop(
    mut rx: mpsc::UnboundedReceiver<IncomingMessage>,
    characters: HashMap<String, CharacterRow>,
    save_tx: mpsc::UnboundedSender<SaveBatch>,
    auth_pool: PgPool,
    mut shutdown: oneshot::Receiver<()>,
) -> Result<()> {
    // Precisa de um tx pra devolver AuthResult pro loop. Criamos um par
    // interno que e fundido com o rx original via tarefa de forward.
    let (auth_tx, mut auth_rx) = mpsc::unbounded_channel::<IncomingMessage>();
    // Map loading: ENV override OR default ao mapfile do projeto. Sem fallback
    // procedural — se o load falhar, o server aborta. O mapa procedural antigo
    // (GameWorld::new) gerava um mundinho 40x30 que confundia (parecia "parede
    // invisivel" pro player). Mantido em codigo so pra testes/legacy.
    let map_path = std::env::var("MAP_FILE")
        .ok()
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "data/maps/game.json".to_string());
    tracing::info!("loading map: {}", map_path);
    let mf = shared::mapfile::MapFile::load(&map_path)
        .map_err(|e| anyhow::anyhow!(
            "failed to load mapfile '{}': {}. \
             Set MAP_FILE env var ou garanta que data/maps/game.json existe.",
            map_path, e
        ))?;
    let mut world = GameWorld::new_from_mapfile(characters, mf);
    world.set_auth_ctx(AuthCtx {
        pool: auth_pool,
        tx: auth_tx,
    });
    let step = Duration::from_secs_f32(TICK_DT);
    let mut next = Instant::now() + step;
    let mut save_counter: u32 = 0;
    tracing::info!("world loop started ({}ms/tick)", step.as_millis());

    loop {
        // Verifica shutdown antes de processar mensagens.
        if shutdown.try_recv().is_ok() {
            tracing::info!("shutdown signal received — saving all characters...");
            let rows = world.collect_character_rows();
            if !rows.is_empty() {
                let _ = save_tx.send(SaveBatch { rows });
            }
            // Aguarda o writer consumir o batch (drena o canal).
            drop(save_tx);
            tokio::time::sleep(Duration::from_millis(500)).await;
            tracing::info!("graceful shutdown complete");
            return Ok(());
        }

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
                Ok(IncomingMessage::CharCreated(id, row, success)) => {
                    world.on_char_created(id, *row, success);
                }
                Ok(IncomingMessage::CharReloadedForSelect(id, row, success)) => {
                    world.on_char_reloaded_for_select(id, *row, success);
                }
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
                Ok(IncomingMessage::CharCreated(id, row, success)) => {
                    world.on_char_created(id, *row, success);
                }
                Ok(IncomingMessage::CharReloadedForSelect(id, row, success)) => {
                    world.on_char_reloaded_for_select(id, *row, success);
                }
                Ok(_) => {} // outros variantes nao devem chegar aqui
                Err(mpsc::error::TryRecvError::Empty) => break,
                Err(mpsc::error::TryRecvError::Disconnected) => break,
            }
        }

        world.step(TICK_DT);
        world.send_snapshots();

        save_counter = save_counter.wrapping_add(1);
        // Trigger imediato (`save_pending`) ou periodico (1s). save_pending eh
        // setado em mudancas criticas: mount/dismount, equip, inventario, etc.
        let should_save = world.save_pending || (save_counter % SAVE_INTERVAL_TICKS == 0);
        if should_save {
            let rows = world.collect_character_rows();
            if !rows.is_empty() {
                let _ = save_tx.send(SaveBatch { rows });
            }
            world.save_pending = false;
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
