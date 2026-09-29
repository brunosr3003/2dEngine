//! One-off: puts ALL active items in shop_id=1 (Klaus). Idempotent —
//! reinserts without duplicating; also ensures a non-NULL buy_price for all.
//!
//! For items with no buy_price set, uses `sell_price * 2` as the default.

use anyhow::Result;
use sqlx::postgres::PgPoolOptions;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt().with_env_filter("info").init();

    let database_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://solar:solar_dev_123@localhost:5432/mmo_dev".to_string());
    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(&database_url)
        .await?;

    // 1. Ensure buy_price on every active item. If NULL, use sell x 2.
    let updated = sqlx::query(
        "UPDATE items SET buy_price = GREATEST(sell_price * 2, 1)
         WHERE active = TRUE AND buy_price IS NULL",
    )
    .execute(&pool)
    .await?;
    tracing::info!("buy_price backfilled on {} items", updated.rows_affected());

    // 2. List every active item.
    let items: Vec<(i32,)> = sqlx::query_as("SELECT id FROM items WHERE active = TRUE ORDER BY id")
        .fetch_all(&pool)
        .await?;

    // 3. Insert into shop_id=1 (Klaus). ON CONFLICT DO NOTHING (already there).
    // sort_order = position in the current list + index.
    let max_order: Option<i32> =
        sqlx::query_scalar("SELECT MAX(sort_order) FROM vendor_shop_items WHERE shop_id = 1")
            .fetch_one(&pool)
            .await?;
    let mut next = max_order.unwrap_or(-1) + 1;

    let mut inserted = 0usize;
    for (id,) in &items {
        let r = sqlx::query(
            "INSERT INTO vendor_shop_items (shop_id, item_id, sort_order)
             VALUES (1, $1, $2)
             ON CONFLICT (shop_id, item_id) DO NOTHING",
        )
        .bind(id)
        .bind(next)
        .execute(&pool)
        .await?;
        if r.rows_affected() > 0 {
            inserted += 1;
            next += 1;
        }
    }

    // 4. Bump economy_version to force a hot-reload on the game server.
    let v: i64 = sqlx::query_scalar(
        "UPDATE economy_version SET version = version + 1, updated_at = NOW()
         WHERE id = 1 RETURNING version",
    )
    .fetch_one(&pool)
    .await?;

    tracing::info!(
        "✓ {} new items added to shop=1 ({} active in total); economy v{}",
        inserted,
        items.len(),
        v
    );
    Ok(())
}
