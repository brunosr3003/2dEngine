use anyhow::Result;
use sqlx::postgres::PgPoolOptions;

#[tokio::main]
async fn main() -> Result<()> {
    let url = std::env::var("DATABASE_URL").unwrap_or_else(|_|
        "postgres://solar:solar_dev_123@localhost:5432/mmo_dev".into());
    let pool = PgPoolOptions::new().max_connections(2).connect(&url).await?;

    // Frost Nova (1046): pre-cast 2s (player imovel) + 3s rain (player free).
    // Meteor (1045): cast 3s = rain inteiro com player imovel.
    let updated = sqlx::query(
        "UPDATE skills SET cast_time_s = 2.0 WHERE id = 1046"
    ).execute(&pool).await?;
    println!("frost nova updated: {}", updated.rows_affected());

    let rows: Vec<(i32, String, f32)> = sqlx::query_as(
        "SELECT id, name, cast_time_s FROM skills WHERE prof='Wand' ORDER BY id"
    ).fetch_all(&pool).await?;
    for (id, n, c) in rows { println!("#{} {:<24} cast={:.2}s", id, n, c); }
    Ok(())
}
