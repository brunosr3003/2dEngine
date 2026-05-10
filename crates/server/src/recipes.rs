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
    CELL.get_or_init(|| Arc::new(RwLock::new(Vec::new()))).clone()
}

/// Versao corrente em cache — comparada ao `recipes_version` do DB pra detectar
/// mudancas. World tick chama `try_hot_reload` periodicamente.
static VERSION: std::sync::atomic::AtomicI64 = std::sync::atomic::AtomicI64::new(0);

/// Sentinela atomica setada quando reload aplicou mudancas. World tick ve,
/// faz broadcast pros clientes, e limpa.
static NEEDS_BROADCAST: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);

pub fn version() -> i64 { VERSION.load(std::sync::atomic::Ordering::Relaxed) }

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
        )"
    ).execute(pool).await?;
    // Tabela versao — admin bumpa pra forcar hot-reload sem restart.
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS recipes_version (
            id      INT PRIMARY KEY DEFAULT 1,
            version BIGINT NOT NULL DEFAULT 1
        )"
    ).execute(pool).await?;
    sqlx::query("INSERT INTO recipes_version (id, version) VALUES (1, 1) ON CONFLICT DO NOTHING")
        .execute(pool).await?;

    // Seed se vazio.
    let count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM craft_recipes")
        .fetch_one(pool).await?;
    if count.0 == 0 {
        seed_from_constants(pool).await?;
        tracing::info!("[recipes] seed inicial: {} recipes inseridas",
            shared::CRAFT_RECIPES.len());
    }

    reload(pool).await?;
    let v: i64 = sqlx::query_scalar("SELECT version FROM recipes_version WHERE id = 1")
        .fetch_one(pool).await.unwrap_or(1);
    VERSION.store(v, std::sync::atomic::Ordering::Relaxed);
    Ok(())
}

/// Hot-reload check — chamado periodicamente do world tick. Retorna true se
/// recarregou (caller deve broadcast `CraftRecipes` pra todos).
pub async fn try_hot_reload(pool: &PgPool) -> anyhow::Result<bool> {
    let v: i64 = sqlx::query_scalar("SELECT version FROM recipes_version WHERE id = 1")
        .fetch_one(pool).await?;
    if v == VERSION.load(std::sync::atomic::Ordering::Relaxed) { return Ok(false); }
    reload(pool).await?;
    VERSION.store(v, std::sync::atomic::Ordering::Relaxed);
    NEEDS_BROADCAST.store(true, std::sync::atomic::Ordering::Relaxed);
    tracing::info!("[recipes] hot-reload aplicado: v{v}");
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
                tracing::warn!("[recipes] hot-reload falhou: {e}");
            }
        }
    });
}

/// Recarrega o cache do DB. Pode ser chamado externamente (futuro hot-reload).
pub async fn reload(pool: &PgPool) -> anyhow::Result<()> {
    let rows: Vec<(i32, String, i16, i16, JsonValue, i32, i32, i32, bool)> = sqlx::query_as(
        "SELECT id, name, category, tier, inputs, output_item_id, output_qty, output_item_level, roll_instance \
         FROM craft_recipes WHERE active = TRUE ORDER BY id"
    ).fetch_all(pool).await?;

    let mut recipes: Vec<CraftRecipeNet> = Vec::with_capacity(rows.len());
    for (id, name, cat, tier, inputs_json, oid, oqty, olvl, roll) in rows {
        let inputs: Vec<[u32; 2]> = serde_json::from_value(inputs_json)
            .unwrap_or_default();
        recipes.push(CraftRecipeNet {
            id:                id as u16,
            name,
            category:          cat.max(0) as u8,
            tier:              tier.max(1) as u8,
            inputs,
            output_item_id:    oid as u16,
            output_qty:        oqty.max(1) as u32,
            output_item_level: olvl.max(0) as u16,
            roll_instance:     roll,
        });
    }
    *cell().write() = recipes;
    let n = cell().read().len();
    tracing::info!("[recipes] carregadas {} recipes do DB", n);
    Ok(())
}

async fn seed_from_constants(pool: &PgPool) -> anyhow::Result<()> {
    for r in shared::CRAFT_RECIPES.iter() {
        // Categoria heuristica baseada em output_item_id.
        let cat: i16 = match r.output_item_id {
            // Resources
            60..=71 => 3,
            // Weapons (Sword/Bow/Axe/Spear/Dagger/Staff/Wand)
            3 | 6 | 12 | 14 | 15 | 25 | 26 => 1,
            // Armor (chest/helm/legs/boots/gloves)
            4 | 7 | 16 | 17 | 18
            | 31 | 32 | 33 | 34 | 35 | 36
            | 37 | 38 | 39 | 40 | 41 | 42
            | 43 | 44 => 2,
            // Boats (naval) — barcos consumiveis.
            100..=109 => 4,
            _ => 0,
        };
        let tier: i16 = derive_tier(r.output_item_id, r.output_item_level) as i16;

        // Inputs validos (item_id != 0).
        let inputs_pairs: Vec<[u32; 2]> = r.inputs.iter()
            .filter_map(|&(id, qty)| if id == 0 { None } else { Some([id as u32, qty as u32]) })
            .collect();
        let inputs_json = serde_json::to_value(&inputs_pairs)?;

        sqlx::query(
            "INSERT INTO craft_recipes (id, name, category, tier, inputs, \
                output_item_id, output_qty, output_item_level, roll_instance) \
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9) \
             ON CONFLICT (id) DO NOTHING"
        )
        .bind(r.id as i32)
        .bind(r.name)
        .bind(cat)
        .bind(tier)
        .bind(inputs_json)
        .bind(r.output_item_id as i32)
        .bind(r.output_qty as i32)
        .bind(r.output_item_level as i32)
        .bind(r.roll_instance)
        .execute(pool).await?;
    }
    Ok(())
}

fn derive_tier(item_id: u16, item_level: u16) -> u8 {
    if item_id >= 60 && item_id <= 71 {
        return ((item_id - 60) % 4 + 1) as u8;
    }
    if item_level <= 10 { 1 }
    else if item_level <= 30 { 2 }
    else if item_level <= 60 { 3 }
    else { 4 }
}

/// Acessor pra cache. Cada call faz copy — uso esporadico (login + craft).
pub fn all() -> Vec<CraftRecipeNet> {
    cell().read().clone()
}

/// Lookup individual — usado em handle_craft.
pub fn find(id: u16) -> Option<CraftRecipeNet> {
    cell().read().iter().find(|r| r.id == id).cloned()
}
