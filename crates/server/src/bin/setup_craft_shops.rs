//! One-off: reconfigura as lojas dos vendors por papel (praça de craft).
//!   1=Mercador (armas/armaduras básicas), 3=Alquimista (poções),
//!   7=Nobre (joias/amuletos/barcos), 8=Camponês (comida — placeholder por ora).
//! Garante buy_price dos itens do Nobre (gem/boat/colares). Bumpa
//! economy_version pra hot-reload. Rodar: cargo run --bin setup_craft_shops
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

    // Garante as lojas (7 e 8 são novas).
    for (sid, name) in [
        (1, "Mercador"),
        (3, "Alquimista"),
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

    // Conteúdo por papel.
    let shops: &[(i32, &[i32])] = &[
        // Mercador: armas/armaduras básicas + poções básicas
        (1, &[3, 12, 14, 4, 7, 16, 2, 8]),
        // Alquimista: poções
        (3, &[2, 8, 9, 10, 11]),
        // Nobre: joias/amuletos/barcos
        (7, &[5, 19, 20, 29, 30, 43, 44, 21, 100]),
        // Camponês: comida (placeholder — trocar por itens de comida quando existirem)
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
        println!("loja {} ← {} itens", sid, items.len());
    }

    // buy_price dos itens do Nobre que podem estar sem preço (gem/boat/colares).
    for (item, price) in [
        (21, 500i32),
        (100, 6000),
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
    println!("✓ lojas configuradas. economy_version → v{}", v);
    Ok(())
}
