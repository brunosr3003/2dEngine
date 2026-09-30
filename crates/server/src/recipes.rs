//! Crafting recipes loaded from the DB. Schema in `craft_recipes`.
//! The initial seed comes from `shared::CRAFT_RECIPES` if the table is empty.
//! An admin can UPDATE/INSERT to change costs/outputs without a rebuild.
//!
//! Static cache — reloads at startup. Hot-reload can be added later (the
//! same pattern as the economy).

use parking_lot::RwLock;
use serde_json::Value as JsonValue;
use shared::protocol::CraftRecipeNet;
use sqlx::PgPool;
use std::sync::Arc;
use std::sync::OnceLock;

static CELL: OnceLock<Arc<RwLock<Vec<CraftRecipeNet>>>> = OnceLock::new();

fn cell() -> Arc<RwLock<Vec<CraftRecipeNet>>> {
    CELL.get_or_init(|| Arc::new(RwLock::new(Vec::new())))
        .clone()
}

/// The version currently cached — compared against the DB's `recipes_version`
/// to detect changes. The world tick calls `try_hot_reload` periodically.
static VERSION: std::sync::atomic::AtomicI64 = std::sync::atomic::AtomicI64::new(0);

/// An atomic sentinel set when a reload applied changes. The world tick sees
/// it, broadcasts to the clients, and clears it.
static NEEDS_BROADCAST: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

pub fn version() -> i64 {
    VERSION.load(std::sync::atomic::Ordering::Relaxed)
}

pub fn take_broadcast_flag() -> bool {
    NEEDS_BROADCAST.swap(false, std::sync::atomic::Ordering::Relaxed)
}

/// Initialises the schema, seeds if empty, and loads the recipes into the cache.
pub async fn init(pool: &PgPool) -> anyhow::Result<()> {
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS craft_recipes (
            id                  INT PRIMARY KEY,
            name                TEXT NOT NULL,
            category            SMALLINT NOT NULL DEFAULT 0,
            tier                SMALLINT NOT NULL DEFAULT 1,
            inputs              JSONB NOT NULL,
            output_item_id      INT NOT NULL,
            output_qty          INT NOT NULL DEFAULT 1,
            output_item_level   INT NOT NULL DEFAULT 0,
            roll_instance       BOOLEAN NOT NULL DEFAULT FALSE,
            active              BOOLEAN NOT NULL DEFAULT TRUE
        )",
    )
    .execute(pool)
    .await?;
    // Version table — an admin bumps it to force a hot-reload with no restart.
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS recipes_version (
            id      INT PRIMARY KEY DEFAULT 1,
            version BIGINT NOT NULL DEFAULT 1
        )",
    )
    .execute(pool)
    .await?;
    sqlx::query("INSERT INTO recipes_version (id, version) VALUES (1, 1) ON CONFLICT DO NOTHING")
        .execute(pool)
        .await?;
    sqlx::query(
        "ALTER TABLE craft_recipes ADD COLUMN IF NOT EXISTS nivel_min SMALLINT NOT NULL DEFAULT 1",
    )
    .execute(pool)
    .await?;
    seed_equipamento(pool).await?;

    reload(pool).await?;
    let v: i64 = sqlx::query_scalar("SELECT version FROM recipes_version WHERE id = 1")
        .fetch_one(pool)
        .await
        .unwrap_or(1);
    VERSION.store(v, std::sync::atomic::Ordering::Relaxed);
    Ok(())
}

/// Hot-reload check — called periodically from the world tick. Returns true
/// if it reloaded (the caller must broadcast `CraftRecipes` to everyone).
pub async fn try_hot_reload(pool: &PgPool) -> anyhow::Result<bool> {
    let v: i64 = sqlx::query_scalar("SELECT version FROM recipes_version WHERE id = 1")
        .fetch_one(pool)
        .await?;
    if v == VERSION.load(std::sync::atomic::Ordering::Relaxed) {
        return Ok(false);
    }
    reload(pool).await?;
    VERSION.store(v, std::sync::atomic::Ordering::Relaxed);
    NEEDS_BROADCAST.store(true, std::sync::atomic::Ordering::Relaxed);
    tracing::info!("[recipes] hot-reload applied: v{v}");
    Ok(true)
}

/// Detached spawn — checks the version every 5s + sets NEEDS_BROADCAST when
/// it changes. The world tick checks the flag + broadcasts to logged-in clients.
pub fn spawn_hot_reload_task(pool: PgPool) {
    tokio::spawn(async move {
        let mut iv = tokio::time::interval(std::time::Duration::from_secs(5));
        iv.tick().await;
        loop {
            iv.tick().await;
            if let Err(e) = try_hot_reload(&pool).await {
                tracing::warn!("[recipes] hot-reload failed: {e}");
            }
        }
    });
}

/// Reloads the cache from the DB. Can be called externally (future hot-reload).
pub async fn reload(pool: &PgPool) -> anyhow::Result<()> {
    let rows: Vec<(i32, String, i16, i16, JsonValue, i32, i32, i32, bool, i16)> = sqlx::query_as(
        "SELECT id, name, category, tier, inputs, output_item_id, output_qty, output_item_level, roll_instance, nivel_min \
         FROM craft_recipes WHERE active = TRUE ORDER BY id"
    ).fetch_all(pool).await?;

    let mut recipes: Vec<CraftRecipeNet> = Vec::with_capacity(rows.len());
    for (id, name, cat, tier, inputs_json, oid, oqty, olvl, roll, nivel_min) in rows {
        let inputs: Vec<[u32; 2]> = serde_json::from_value(inputs_json).unwrap_or_default();
        recipes.push(CraftRecipeNet {
            id: id as u16,
            name,
            category: cat.max(0) as u8,
            // `craft_station_of` only existed to send the boat to CARPENTRY; with no
            // boat, every recipe belongs to the FORGE. The client does not even read the field.
            station: shared::craft_station::FORGE,
            tier: tier.max(1) as u8,
            inputs,
            output_item_id: oid as u16,
            output_qty: oqty.max(1) as u32,
            output_item_level: olvl.max(0) as u16,
            roll_instance: roll,
            nivel_min: nivel_min.max(1) as u16,
        });
    }
    *cell().write() = recipes;
    let n = cell().read().len();
    tracing::info!("[recipes] loaded {} recipes from the DB", n);
    Ok(())
}

async fn seed_equipamento(pool: &PgPool) -> anyhow::Result<()> {
    let mut novas = 0u64;
    let mut todas = shared::receitas::receitas_de_equipamento();
    todas.push(shared::receitas::receita_do_selo());
    // As chaves de Porão: uma por dungeon física, madeira e aço.
    todas.extend(shared::receitas::receitas_de_chave_de_porao());
    for r in todas {
        novas += sqlx::query(
            "INSERT INTO craft_recipes (id, name, category, tier, inputs, \
                output_item_id, output_qty, output_item_level, roll_instance, nivel_min) \
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10) \
             ON CONFLICT (id) DO NOTHING",
        )
        .bind(r.id as i32)
        .bind(&r.name)
        .bind(r.category as i16)
        .bind(r.tier as i16)
        .bind(serde_json::to_value(&r.inputs)?)
        .bind(r.output_item_id as i32)
        .bind(r.output_qty as i32)
        .bind(r.output_item_level as i32)
        .bind(r.roll_instance)
        .bind(r.nivel_min as i16)
        .execute(pool)
        .await?
        .rows_affected();
    }
    if novas > 0 {
        tracing::info!("[recipes] {novas} gear recipes seeded");
    }
    // As chaves de Porão são 100% derivadas do código (`porao::receita_de`),
    // então o código MANDA: o seed acima não alcança receita já semeada, e o
    // custo da chave caiu ~3x em 29/09/2026 ("1187 woods + 543 steel ... its
    // too much"). Sem isto a produção continuaria pedindo o custo velho.
    let mut chaves = 0u64;
    for r in shared::receitas::receitas_de_chave_de_porao() {
        let inputs = serde_json::to_value(&r.inputs)?;
        chaves += sqlx::query(
            "UPDATE craft_recipes SET inputs = $2 WHERE id = $1 AND inputs IS DISTINCT FROM $2",
        )
        .bind(r.id as i32)
        .bind(&inputs)
        .execute(pool)
        .await?
        .rows_affected();
    }
    if chaves > 0 {
        tracing::info!("[recipes] {chaves} Porão key recipes brought to the code's cost");
    }
    // Nivel minimo alinhado com as faixas da chave. So' mexe em quem ainda
    // esta' num valor ANTIGO conhecido: ajuste manual fica.
    //
    // O seed acima e' `ON CONFLICT DO NOTHING`, entao mudar a constante NAO
    // alcanca receita ja' semeada — e foi por isso que a katana Rara continuou
    // pedindo 40 depois de a chave azul passar a cair no 30. Cada vez que a
    // ladder anda, o valor velho entra nesta lista.
    //
    // (indice da faixa, valores que aquela faixa ja' teve)
    let mut ajustadas = 0u64;
    for (faixa, antigos) in [
        (1i32, &[15i16][..]),
        (2, &[30, 40][..]), // azul: 30->40 em 19/09, 40->30 em 28/09
        (3, &[60][..]),     // epico: 60->40 em 28/09
    ] {
        let novo = shared::receitas::FAIXAS[faixa as usize].nivel_min as i16;
        let de = shared::receitas::PRIMEIRO_ID as i32 + faixa * 100;
        for antigo in antigos {
            if *antigo == novo {
                continue;
            }
            ajustadas += sqlx::query(
                "UPDATE craft_recipes SET nivel_min = $1 WHERE id BETWEEN $2 AND $3 AND nivel_min = $4",
            )
            .bind(novo)
            .bind(de)
            .bind(de + 99)
            .bind(*antigo)
            .execute(pool)
            .await?
            .rows_affected();
        }
    }
    if ajustadas > 0 {
        // Sem citar numero: a mensagem envelhece junto com a ladder, e uma
        // linha de log que mente e' pior do que uma linha vaga.
        tracing::info!("[recipes] {ajustadas} recipes realigned to the key bands");
    }
    Ok(())
}

fn derive_tier(item_id: u16, item_level: u16) -> u8 {
    if item_id >= 60 && item_id <= 71 {
        return ((item_id - 60) % 4 + 1) as u8;
    }
    if item_level <= 10 {
        1
    } else if item_level <= 30 {
        2
    } else if item_level <= 60 {
        3
    } else {
        4
    }
}

/// Acessor pra cache. Cada call faz copy — uso esporadico (login + craft).
pub fn all() -> Vec<CraftRecipeNet> {
    cell().read().clone()
}

/// Lookup individual — usado em handle_craft.
pub fn find(id: u16) -> Option<CraftRecipeNet> {
    cell().read().iter().find(|r| r.id == id).cloned()
}
