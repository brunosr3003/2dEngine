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
#[cfg(test)]
mod balanceamento;
mod barra;
mod canais;
mod coleta;
mod correio_admin;
mod craft;
mod loja_npc;
mod economy;
mod loja;
mod loot_mobs;
mod mapa_ilha;
mod mercado;
mod mercado_razao;
mod mesa;
mod morte;
mod panoptico;
mod persistence;
mod preferencias;
mod presenca;
mod quests;
mod recipes;
mod rumo;
mod session;
mod skills;
mod social;
mod telemetria;
mod tick;
mod world;

use anyhow::Result;
use shared::TICK_RATE_HZ;
use tokio::net::TcpListener;
use tokio::sync::{mpsc, oneshot};

#[tokio::main]
async fn main() -> Result<()> {
    {
        use tracing_subscriber::layer::SubscriberExt;
        use tracing_subscriber::util::SubscriberInitExt;
        // WARN e ERROR tambem vao pro panoptico (`telemetria::CamadaDeErros`).
        tracing_subscriber::registry()
            .with(
                tracing_subscriber::EnvFilter::try_from_default_env()
                    .unwrap_or_else(|_| "info,server=debug".into()),
            )
            .with(tracing_subscriber::fmt::layer())
            .with(telemetria::CamadaDeErros)
            .init();
    }

    let addr = std::env::var("BIND_ADDR").unwrap_or_else(|_| "0.0.0.0:9000".to_string());
    let database_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://solar:solar_dev_123@localhost:5432/mmo_dev".to_string());

    // Abre Postgres, carrega personagens existentes, sobe task de escrita.
    let pool = persistence::open_pool(&database_url).await?;
    social::init(&pool).await?;
    // Telemetria agregada por minuto pro panoptico. Tabelas antes de tudo:
    // o que acontecer no boot ja' conta.
    telemetria::init(&pool).await?;
    telemetria::spawn(pool.clone());
    // Schema de quests ANTES do load_all (cria character_quests + coluna
    // faction_points em characters, que o load_all lê por personagem).
    quests::init(&pool).await?;
    let characters = persistence::load_all(&pool).await?;
    tracing::info!("db conectado: {} personagens carregados", characters.len());
    let save_tx = persistence::spawn_writer(pool.clone());

    // Canal: este processo se anuncia e bate o coracao. Ver `canais`.
    canais::init(&pool).await?;
    // Mercado global: tabelas do realm e, com DATABASE_URL_CENTRAL, o banco
    // central. Sem ele o mercado fica desligado e o jogo segue.
    mercado::init(&pool).await?;
    // Loja de cash: pedidos e posses no mesmo banco central.
    if let Some(central) = mercado::central() {
        loja::criar_tabelas(&central).await?;
    }
    let populacao = canais::Populacao::default();
    let saude = canais::Saude::default();
    let diretorio = canais::Diretorio::default();
    canais::spawn_heartbeat(
        pool.clone(),
        populacao.clone(),
        saude.clone(),
        diretorio.clone(),
    );

    // Olho de cima. So' sobe se PANOPTICO_BIND existir — sem ele o processo
    // nao abre porta nenhuma a mais.
    if panoptico::ativo() {
        tokio::spawn(async {
            if let Err(e) = panoptico::servir().await {
                tracing::error!("panoptico caiu: {e:#}");
            }
        });
    }

    // Inicializa economia + spawn da tarefa de hot-reload.
    economy::init(&pool).await?;
    economy::spawn_hot_reload(pool.clone());
    economy::load_server_config(&pool).await?;
    recipes::init(&pool).await?;
    recipes::spawn_hot_reload_task(pool.clone());
    tracing::info!("economia carregada (hot-reload a cada 5s; bumpa economy_version pra forçar)");

    // Skills (Phase 1). Compartilha o mesmo `economy_version`.
    skills::init(&pool).await?;
    skills::recarregar(&pool).await;

    let listener = TcpListener::bind(&addr).await?;
    tracing::info!("server listening on ws://{addr} ({TICK_RATE_HZ} Hz)");

    let (tx_incoming, rx_incoming) = mpsc::unbounded_channel();
    let (shutdown_tx, shutdown_rx) = oneshot::channel::<()>();

    let auth_pool = pool.clone();
    tokio::spawn(async move {
        if let Err(e) = tick::run_world_loop(
            rx_incoming,
            characters,
            save_tx,
            auth_pool,
            shutdown_rx,
            populacao,
            saude,
            diretorio,
        )
        .await
        {
            tracing::error!("world loop exited: {e:?}");
        }
    });

    // Aguarda SIGTERM ou SIGINT para shutdown gracioso.
    tokio::spawn(async move {
        let mut sigterm = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("failed to register SIGTERM handler");
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
