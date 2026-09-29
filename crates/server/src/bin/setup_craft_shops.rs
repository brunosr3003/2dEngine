//! One-off: reconfigures the vendors' shops by role (the craft square).
//! 1=Merchant (basic weapons/armor), 3=Alchemist (potions),
//! 7=Noble (jewellery/amulets/boats), 8=Peasant (food — a placeholder for now).
//! Ensures buy_price on the Noble's items (gem/boat/necklaces). Bumps
//! economy_version for a hot-reload. Run: cargo run --bin setup_craft_shops
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

    // Ensures the shops exist (7 and 8 are new).
    for (sid, name) in [
        (1, "Mercador"),
        (3, "Alchemist"),
        (7, "Nobre"),
        (8, "Camponês"),
    ] {
        sqlx::query(
            "INSERT INTO vendor_shops (shop_id, name) VALUES ($1,$2) \
                     ON CONFLICT (shop_id) DO UPDATE SET name = EXCLUDED.name",
        )
        .bind(sid)
        .bind(name)
        .execute(&pool)
        .await?;
    }

    // Content by role.
    let shops: &[(i32, &[i32])] = &[
        // Merchant: basic weapons/armor + basic potions
        (1, &[3, 12, 14, 4, 7, 16, 2, 8]),
        // Alchemist: potions
        (3, &[2, 8, 9, 10, 11]),
        // Noble: jewellery and amulets. Item 100 was the old boat, deleted on
        // 20/09/2026 — the Noble started advertising an id that no longer exists.
        (7, &[5, 19, 20, 29, 30, 43, 44, 21]),
        // Peasant: food (placeholder — swap for food items when they exist)
        (8, &[2, 11]),
    ];
    for (sid, items) in shops {
        sqlx::query("DELETE FROM vendor_shop_items WHERE shop_id = $1")
            .bind(sid)
            .execute(&pool)
            .await?;
        for (i, &item) in items.iter().enumerate() {
            sqlx::query(
                "INSERT INTO vendor_shop_items (shop_id, item_id, sort_order) VALUES ($1,$2,$3)",
            )
            .bind(sid)
            .bind(item)
            .bind(i as i32)
            .execute(&pool)
            .await?;
        }
        println!("shop {} ← {} items", sid, items.len());
    }

    // buy_price of the Noble's items that may have no price (gem/necklaces).
    for (item, price) in [
        (21, 500i32),
        (43, 250),
        (44, 400),
        (5, 200),
        (19, 250),
        (20, 250),
        (29, 300),
        (30, 300),
    ] {
        let r = sqlx::query(
            "UPDATE items SET buy_price = $2 \
                             WHERE id = $1 AND (buy_price IS NULL OR buy_price = 0)",
        )
        .bind(item)
        .bind(price)
        .execute(&pool)
        .await?;
        if r.rows_affected() > 0 {
            println!("buy_price item {} = {}", item, price);
        }
    }

    let v: i64 = sqlx::query_scalar(
        "UPDATE economy_version SET version = version + 1, updated_at = NOW() \
         WHERE id = 1 RETURNING version",
    )
    .fetch_one(&pool)
    .await?;
    println!("✓ shops configured. economy_version → v{}", v);
    Ok(())
}
