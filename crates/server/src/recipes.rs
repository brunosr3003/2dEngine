//! Recipes de crafting carregadas do DB. Schema em `craft_recipes`.
//! Seed inicial vem de `shared::CRAFT_RECIPES` se a tabela estiver vazia.
//! Admin pode UPDATE/INSERT pra mudar custos/outputs sem rebuild.
//!
//! Cache estatico — recarrega no startup. Hot-reload pode ser adicionado
//! depois (mesmo padrao da economia).

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

/// Versao corrente em cache — comparada ao `recipes_version` do DB pra detectar
/// mudancas. World tick chama `try_hot_reload` periodicamente.
static VERSION: std::sync::atomic::AtomicI64 = std::sync::atomic::AtomicI64::new(0);

/// Sentinela atomica setada quando reload aplicou mudancas. World tick ve,
/// faz broadcast pros clientes, e limpa.
static NEEDS_BROADCAST: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

pub fn version() -> i64 {
    VERSION.load(std::sync::atomic::Ordering::Relaxed)
}

pub fn take_broadcast_flag() -> bool {
    NEEDS_BROADCAST.swap(false, std::sync::atomic::Ordering::Relaxed)
}

/// Inicializa schema, seed se vazio, e carrega recipes pra cache.
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
    // Tabela versao — admin bumpa pra forcar hot-reload sem restart.
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

/// Hot-reload check — chamado periodicamente do world tick. Retorna true se
/// recarregou (caller deve broadcast `CraftRecipes` pra todos).
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

/// Spawn detached — checa version a cada 5s + seta NEEDS_BROADCAST quando
/// muda. World tick checa o flag + faz broadcast pra logged-in clients.
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

/// Recarrega o cache do DB. Pode ser chamado externamente (futuro hot-reload).
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
            // `craft_station_of` so' existia pra mandar barco pra CARPENTRY;
            // sem barco, toda receita e' da FORGE. O cliente nem le' o campo.
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
    // Nivel minimo alinhado com as faixas da chave (verde 15->20, azul
    // 30->40). So' mexe em quem ainda esta' no valor antigo: ajuste manual fica.
    let mut ajustadas = 0u64;
    for (faixa, antigo) in [(1i32, 15i16), (2, 30)] {
        let novo = shared::receitas::FAIXAS[faixa as usize].nivel_min as i16;
        let de = shared::receitas::PRIMEIRO_ID as i32 + faixa * 100;
        ajustadas += sqlx::query(
            "UPDATE craft_recipes SET nivel_min = $1 WHERE id BETWEEN $2 AND $3 AND nivel_min = $4",
        )
        .bind(novo)
        .bind(de)
        .bind(de + 99)
        .bind(antigo)
        .execute(pool)
        .await?
        .rows_affected();
    }
    if ajustadas > 0 {
        tracing::info!("[recipes] {ajustadas} recipes with a new minimum level (green 20, blue 40)");
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
