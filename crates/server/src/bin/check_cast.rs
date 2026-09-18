use anyhow::Result;
use sqlx::postgres::PgPoolOptions;

#[tokio::main]
async fn main() -> Result<()> {
    let url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://solar:solar_dev_123@localhost:5432/mmo_dev".into());
    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(&url)
        .await?;

    // Frost Nova (1046): pre-cast 2s (player imovel) + 3s rain (player free).
    // Meteor (1045): cast 3s = rain inteiro com player imovel.
    let updated = sqlx::query("UPDATE skills SET cast_time_s = 2.0 WHERE id = 1046")
        .execute(&pool)
        .await?;
    println!("frost nova updated: {}", updated.rows_affected());

    // Smoke Bomb (1038): nuvem 5s com DOT + envenenado. Atualiza damage/scaling.
    let updated = sqlx::query(
        "UPDATE skills SET base_damage=5, scaling_dex=0.2, per_rank_dmg_pct=0.10,
            description='Radius 3; nuvem 5s, dano AoE/s + envenenado nos alvos dentro.'
         WHERE id = 1038",
    )
    .execute(&pool)
    .await?;
    println!("smoke bomb updated: {}", updated.rows_affected());

    // Riposte → Leap Strike (1001): trocou tipo. AoE r2 cone 6 tiles + stun.
    let updated = sqlx::query(
        "UPDATE skills SET name='Leap Strike', target_type='aoe_circle',
            cost_stamina=25, cooldown_s=8.0, range_tiles=6.0, radius_tiles=2.0,
            base_damage=15, scaling_atk=0.8, scaling_wis=0.0,
            description='Pula ate 6 tiles na direção do mouse; dano AoE r2 + stun 1.5s na queda.'
         WHERE id = 1001",
    )
    .execute(&pool)
    .await?;
    println!("leap strike updated: {}", updated.rows_affected());

    let rows: Vec<(i32, String, f32)> =
        sqlx::query_as("SELECT id, name, cast_time_s FROM skills WHERE prof='Wand' ORDER BY id")
            .fetch_all(&pool)
            .await?;
    for (id, n, c) in rows {
        println!("#{} {:<24} cast={:.2}s", id, n, c);
    }
    Ok(())
}
