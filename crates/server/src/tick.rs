//! Loop do mundo em tick fixo. Recebe IncomingMessages por mpsc e avanca
//! a simulacao em `TICK_RATE_HZ`. Single-threaded (proprio tokio task).

use crate::world::{GameWorld, IncomingMessage};
use anyhow::Result;
use shared::TICK_DT;
use std::time::{Duration, Instant};
use tokio::sync::mpsc;

pub async fn run_world_loop(
    mut rx: mpsc::UnboundedReceiver<IncomingMessage>,
) -> Result<()> {
    let mut world = GameWorld::new();
    let step = Duration::from_secs_f32(TICK_DT);
    let mut next = Instant::now() + step;
    tracing::info!("world loop started ({}ms/tick)", step.as_millis());

    loop {
        // Drena todas as mensagens pendentes sem bloquear.
        loop {
            match rx.try_recv() {
                Ok(IncomingMessage::Connected(h)) => world.on_connect(h),
                Ok(IncomingMessage::Disconnected(id)) => world.on_disconnect(id),
                Ok(IncomingMessage::Message(id, m)) => world.on_message(id, m),
                Err(mpsc::error::TryRecvError::Empty) => break,
                Err(mpsc::error::TryRecvError::Disconnected) => {
                    tracing::warn!("all senders dropped, exiting world loop");
                    return Ok(());
                }
            }
        }

        world.step(TICK_DT);
        world.send_snapshots();

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
