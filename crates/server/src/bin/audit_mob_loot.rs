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
            .ok_or_else(|| anyhow::anyhow!("Tabela ausente: {kind}"))?;
        for e in table {
            let item = cfg
                .items
                .get(&e.item_id)
                .ok_or_else(|| anyhow::anyhow!("Item ausente: {}", e.item_id))?;
            anyhow::ensure!(
                shared::equip_slot_of(e.item_id).is_none() && item.equip_slot.is_none(),
                "Equipamento no loot: {}",
                e.item_id
            );
            anyhow::ensure!(item.active, "Item inativo no loot: {}", e.item_id);
        }
        for seed in 0..10000 {
            let drops = cfg.roll_loot(kind, seed);
            anyhow::ensure!(
                drops
                    .iter()
                    .any(|d| d.0 == shared::item_id::COPPER && d.1 > 0),
                "Cobre ausente: tipo {kind}, seed {seed}"
            );
            anyhow::ensure!(
                drops.iter().all(|d| shared::equip_slot_of(d.0).is_none()),
                "Equipamento sorteado"
            );
            total += 1;
        }
        let nome = cfg.enemy_kind(kind).map_or("?", |e| e.name.as_str());
        println!(
            "{kind} {nome}: {} entradas, 10000 mortes verificadas",
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
    println!("OK: {total} mortes simuladas com o banco atual; zero equipamento e cobre em todas.");
    pool.close().await;
    Ok(())
}
