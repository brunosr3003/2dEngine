//! Servidor autoritativo do MMO.
//!
//! Arquitetura:
//! - Tokio runtime multi-thread.
//! - 1 task `accept` por listener TCP; aceita conexoes WebSocket.
//! - 1 task por sessao (I/O ws).
//! - 1 task `world` que roda o loop de tick — recebe mensagens via mpsc,
//!   aplica inputs, simula, envia snapshots.
//!
//! Todo o ECS vive no task `world` — sem locks, sem sync primitives nas
//! entidades. As sessoes se comunicam com o mundo SOMENTE via mpsc.

mod auth;
mod economy;
mod enemy_builds;
mod persistence;
mod session;
mod skills;
mod tick;
mod world;

use anyhow::Result;
use shared::TICK_RATE_HZ;
use tokio::net::TcpListener;
use tokio::sync::{mpsc, oneshot};

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info,server=debug".into()),
        )
        .init();

    let addr = std::env::var("BIND_ADDR").unwrap_or_else(|_| "0.0.0.0:9000".to_string());
    let database_url = std::env::var("DATABASE_URL").unwrap_or_else(|_| {
        "postgres://solar:solar_dev_123@localhost:5432/mmo_dev".to_string()
    });

    // Abre Postgres, carrega personagens existentes, sobe task de escrita.
    let pool = persistence::open_pool(&database_url).await?;
    let characters = persistence::load_all(&pool).await?;
    tracing::info!("db conectado: {} personagens carregados", characters.len());
    let save_tx = persistence::spawn_writer(pool.clone());

    // Inicializa economia + spawn da tarefa de hot-reload.
    economy::init(&pool).await?;
    economy::spawn_hot_reload(pool.clone());
    tracing::info!("economia carregada (hot-reload a cada 5s; bumpa economy_version pra forçar)");

    // Skills (Phase 1). Compartilha o mesmo `economy_version`.
    skills::init(&pool).await?;
    skills::spawn_hot_reload(pool.clone());
    tracing::info!("skills carregadas: {} entradas", skills::all_skills().len());

    let listener = TcpListener::bind(&addr).await?;
    tracing::info!("server listening on ws://{addr} ({TICK_RATE_HZ} Hz)");

    let (tx_incoming, rx_incoming) = mpsc::unbounded_channel();
    let (shutdown_tx, shutdown_rx) = oneshot::channel::<()>();

    let auth_pool = pool.clone();
    tokio::spawn(async move {
        if let Err(e) = tick::run_world_loop(rx_incoming, characters, save_tx, auth_pool, shutdown_rx).await {
            tracing::error!("world loop exited: {e:?}");
        }
    });

    // Aguarda SIGTERM ou SIGINT para shutdown gracioso.
    tokio::spawn(async move {
        let mut sigterm = tokio::signal::unix::signal(
            tokio::signal::unix::SignalKind::terminate(),
        ).expect("failed to register SIGTERM handler");
        tokio::select! {
            _ = sigterm.recv() => tracing::info!("SIGTERM received"),
            _ = tokio::signal::ctrl_c() => tracing::info!("SIGINT received"),
        }
        let _ = shutdown_tx.send(());
        // Aguarda o world loop salvar (max 2s) antes de deixar o processo sair.
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
        std::process::exit(0);
    });

    loop {
        let (stream, peer) = listener.accept().await?;
        let tx = tx_incoming.clone();
        tokio::spawn(async move {
            if let Err(e) = session::handle_connection(stream, peer, tx).await {
                tracing::debug!("session {peer} ended: {e:?}");
            }
        });
    }
}
