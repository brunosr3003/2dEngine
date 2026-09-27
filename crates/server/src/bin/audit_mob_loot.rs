//! Auditoria somente leitura: DATABASE_URL deve apontar para o banco do jogo.
#[allow(dead_code)]
#[path = "../economy.rs"]
mod economy;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(1)
        .connect(&std::env::var("DATABASE_URL")?)
        .await?;
    let cfg = economy::load_from_db(&pool).await?;
    let mut total = 0;
    for kind in 0..=7 {
        let table = cfg
            .loot_tables
            .get(&kind)
            .ok_or_else(|| anyhow::anyhow!("Missing table: {kind}"))?;
        for e in table {
            let item = cfg
                .items
                .get(&e.item_id)
                .ok_or_else(|| anyhow::anyhow!("Missing item: {}", e.item_id))?;
            anyhow::ensure!(
                shared::equip_slot_of(e.item_id).is_none() && item.equip_slot.is_none(),
                "Gear in the loot: {}",
                e.item_id
            );
            anyhow::ensure!(item.active, "Inactive item in the loot: {}", e.item_id);
        }
        for seed in 0..10000 {
            let drops = cfg.roll_loot(kind, seed);
            anyhow::ensure!(
                drops
                    .iter()
                    .any(|d| d.0 == shared::item_id::COPPER && d.1 > 0),
                "Missing copper: kind {kind}, seed {seed}"
            );
            anyhow::ensure!(
                drops.iter().all(|d| shared::equip_slot_of(d.0).is_none()),
                "Gear rolled"
            );
            total += 1;
        }
        let nome = cfg.enemy_kind(kind).map_or("?", |e| e.name.as_str());
        println!(
            "{kind} {nome}: {} entries, 10000 kills checked",
            table.len()
        );
        for e in table {
            println!(
                "  {}: {}–{} ({:.0}%)",
                cfg.items[&e.item_id].name,
                e.qty_min,
                e.qty_max,
                e.chance * 100.0
            );
        }
    }
    println!("OK: {total} kills simulated against the current database; zero gear and copper in all of them.");
    pool.close().await;
    Ok(())
}
