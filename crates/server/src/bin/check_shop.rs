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

    let rows: Vec<(i32, String, Option<i32>, bool)> = sqlx::query_as(
        "SELECT i.id, i.name, i.buy_price, i.active
         FROM vendor_shop_items vsi
         JOIN items i ON i.id = vsi.item_id
         WHERE vsi.shop_id = 1
         ORDER BY vsi.sort_order",
    )
    .fetch_all(&pool)
    .await?;

    println!("Shop 1 (Klaus) tem {} itens:", rows.len());
    for (id, name, buy, active) in rows {
        let buy_s = buy.map(|p| p.to_string()).unwrap_or_else(|| "NULL".into());
        println!(
            "  #{:3}  {:25}  buy={:>6}  active={}",
            id, name, buy_s, active
        );
    }
    Ok(())
}
