//! One-off: bruno char L100 + todas as 8 profs L100 + 100 SP.
//! Usado pra testar skills sem grinding. Idempotente.
//!
//! Uso: cargo run --bin set_max_bruno

use anyhow::Result;
use sqlx::postgres::PgPoolOptions;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter("info,set_max_bruno=debug")
        .init();

    let database_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://solar:solar_dev_123@localhost:5432/mmo_dev".to_string());

    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(&database_url)
        .await?;

    // Char L100: cumulative xp = 100 × (99·100·199)/6 = 32,835,000.
    // Adicionamos 1M de margem.
    let char_xp: i64 = 33_835_000;

    // Prof L100 cumulative = 50 × (1+2+...+99) = 50 × 4950 = 247_500.
    let prof_xp: i64 = 250_000;

    sqlx::query(
        "UPDATE characters
         SET xp = $1,
             skill_points_earned = 100,
             skill_points_spent = 0
         WHERE name = 'bruno'",
    )
    .bind(char_xp)
    .execute(&pool)
    .await?;

    // 8 proficiências (Sword=0, Staff=1, Dagger=2, Bow=3, Wand=4, Unarmed=5, Axe=6, Spear=7).
    for prof_kind in 0..8i32 {
        sqlx::query(
            "INSERT INTO proficiencies (character_name, prof_kind, xp)
             VALUES ('bruno', $1, $2)
             ON CONFLICT (character_name, prof_kind) DO UPDATE SET xp = EXCLUDED.xp",
        )
        .bind(prof_kind)
        .bind(prof_xp)
        .execute(&pool)
        .await?;
    }

    // Limpa skills aprendidas pra começar do zero (100 SP frescos).
    sqlx::query("DELETE FROM player_skills WHERE character_name = 'bruno'")
        .execute(&pool)
        .await?;

    tracing::info!(
        "✓ bruno set: char xp={} (L100), 8 profs with {} xp (L100), 100 SP, learned_skills cleared",
        char_xp,
        prof_xp,
    );
    Ok(())
}
