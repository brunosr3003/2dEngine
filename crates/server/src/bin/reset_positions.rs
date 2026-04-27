//! Reseta a posicao salva de TODOS os personagens pro spawn default do
//! mapa atual + restaura HP cheio. Usa exatamente a mesma logica de
//! resolucao de spawn que o `server` usa na inicializacao, entao "via
//! server" significa que o spawn calculado bate com o que o server
//! escolheria pra um personagem novo.
//!
//! Por que isso e necessario: posicoes salvas em DB podem cair fora do
//! mapa atual (mapa regenerado, troca de MapFile, etc). O wall-check no
//! login do server cobre apenas tiles WALL/OOB; tiles validos mas em
//! regioes ilhas/inacessiveis nao sao detectados — esse script forca o
//! reset em massa.
//!
//! Uso:
//!   # mapa crafted (default — bate com `cargo run --bin server` sem env)
//!   cargo run --bin reset_positions
//!
//!   # mapa exportado pelo editor Unity (mesmo que o server usaria)
//!   MAP_FILE=path/to/file.mapfile cargo run --bin reset_positions
//!
//!   # DB diferente
//!   DATABASE_URL=postgres://... cargo run --bin reset_positions
//!
//! Idempotente — pode rodar quantas vezes quiser.

use anyhow::Result;
use shared::mapfile::MapFile;
use shared::world_gen::build_crafted_map;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info,reset_positions=debug".into()),
        )
        .init();

    let database_url = std::env::var("DATABASE_URL").unwrap_or_else(|_| {
        "postgres://solar:solar_dev_123@localhost:5432/mmo_dev".to_string()
    });

    // Resolve o spawn tile do mesmo jeito que o server escolhe na
    // inicializacao (tick.rs::run_world_loop).
    let (sx, sy) = match std::env::var("MAP_FILE") {
        Ok(path) if !path.is_empty() => {
            tracing::info!("usando MAP_FILE={}", path);
            let mf = MapFile::load(&path)?;
            (mf.spawn[0] as i32, mf.spawn[1] as i32)
        }
        _ => {
            tracing::info!("usando crafted map default");
            build_crafted_map().map.spawn_tile()
        }
    };
    // Centro do tile (mesma convencao de `default_spawn` no server).
    let x = sx as f32 + 0.5;
    let y = sy as f32 + 0.5;
    tracing::info!("spawn resolvido: tile=({}, {}) → world=({:.1}, {:.1})", sx, sy, x, y);

    let pool = sqlx::postgres::PgPool::connect(&database_url).await?;

    // Lista antes (pra log + sanity check)
    let before: Vec<(String, f32, f32, i32, i32)> = sqlx::query_as(
        "SELECT name, x, y, hp, max_hp FROM characters ORDER BY name",
    )
    .fetch_all(&pool)
    .await?;
    tracing::info!("encontrados {} personagens", before.len());
    for (name, bx, by, hp, max_hp) in &before {
        tracing::info!("  {:<16} pos=({:.1}, {:.1}) hp={}/{}", name, bx, by, hp, max_hp);
    }

    // Reset: pos = spawn, hp = max_hp (tira de dead state se aplicavel),
    // updated = now em ms (mesma convencao do save normal).
    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)?
        .as_millis() as i64;
    let res = sqlx::query(
        "UPDATE characters SET x = $1, y = $2, hp = max_hp, updated = $3",
    )
    .bind(x)
    .bind(y)
    .bind(now_ms)
    .execute(&pool)
    .await?;

    tracing::info!(
        "✓ {} personagens resetados pra ({:.1}, {:.1}) com HP cheio",
        res.rows_affected(),
        x,
        y,
    );
    if res.rows_affected() > 0 {
        tracing::info!(
            "Atencao: se houver players logados agora, o reset so vale no PROXIMO login \
             (server tem cache em memoria). Reinicie o `cargo run --bin server` ou peca \
             pros players relogarem."
        );
    }
    Ok(())
}
