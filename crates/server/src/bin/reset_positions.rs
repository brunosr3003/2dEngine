//! Resets the saved position of ALL characters to the current map's default
//! spawn + restores full HP. Uses exactly the same spawn resolution logic the
//! `server` uses at startup, so "via server" means the computed spawn matches
//! what the server would choose for a new character.
//!
//! Why this is necessary: positions saved in the DB can fall outside the
//! current map (regenerated map, MapFile change, etc). The wall-check at the
//! server's login only covers WALL/OOB tiles; valid tiles in island/
//! unreachable regions are not detected — this script forces a mass reset.
//!
//! Usage:
//! # crafted map (default — matches `cargo run --bin server` with no env)
//! cargo run --bin reset_positions
//!
//! # map exported by the Unity editor (the same one the server would use)
//! MAP_FILE=path/to/file.mapfile cargo run --bin reset_positions
//!
//!   # DB diferente
//!   DATABASE_URL=postgres://... cargo run --bin reset_positions
//!
//! Idempotent — run it as many times as you like.

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

    let database_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://solar:solar_dev_123@localhost:5432/mmo_dev".to_string());

    // Resolves the spawn tile the same way the server chooses at startup
    // (tick.rs::run_world_loop).
    let (sx, sy) = match std::env::var("MAP_FILE") {
        Ok(path) if !path.is_empty() => {
            tracing::info!("using MAP_FILE={}", path);
            let mf = MapFile::load(&path)?;
            (mf.spawn[0] as i32, mf.spawn[1] as i32)
        }
        _ => {
            tracing::info!("using the default crafted map");
            build_crafted_map().map.spawn_tile()
        }
    };
    // Centro do tile (mesma convencao de `default_spawn` no server).
    let x = sx as f32 + 0.5;
    let y = sy as f32 + 0.5;
    tracing::info!(
        "spawn resolved: tile=({}, {}) → world=({:.1}, {:.1})",
        sx,
        sy,
        x,
        y
    );

    let pool = sqlx::postgres::PgPool::connect(&database_url).await?;

    // List beforehand (for the log + a sanity check)
    let before: Vec<(String, f32, f32, i32, i32)> =
        sqlx::query_as("SELECT name, x, y, hp, max_hp FROM characters ORDER BY name")
            .fetch_all(&pool)
            .await?;
    tracing::info!("found {} characters", before.len());
    for (name, bx, by, hp, max_hp) in &before {
        tracing::info!(
            "  {:<16} pos=({:.1}, {:.1}) hp={}/{}",
            name,
            bx,
            by,
            hp,
            max_hp
        );
    }

    // Reset: pos = spawn, hp = max_hp (tira de dead state se aplicavel),
    // updated = now em ms (mesma convencao do save normal).
    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)?
        .as_millis() as i64;
    let res = sqlx::query("UPDATE characters SET x = $1, y = $2, hp = max_hp, updated = $3")
        .bind(x)
        .bind(y)
        .bind(now_ms)
        .execute(&pool)
        .await?;

    tracing::info!(
        "✓ {} characters reset to ({:.1}, {:.1}) with full HP",
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
