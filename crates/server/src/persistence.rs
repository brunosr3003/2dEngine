//! Persistencia de personagens em Postgres.
//!
//! Arquitetura:
//! - Server abre pool Postgres no startup, roda schema, carrega todos os
//!   personagens pra memoria.
//! - O tick loop le/escreve no cache sincronamente (sem locks complicados).
//! - Uma task background recebe `SaveBatch` via mpsc e escreve no DB, nunca
//!   bloqueando o tick.
//!
//! Fase 4 minimo: persiste nome, posicao e HP. Inventario/XP vem depois.

use anyhow::Result;
use glam::Vec2;
use shared::Health;
use sqlx::postgres::{PgPool, PgPoolOptions};
use std::collections::HashMap;
use tokio::sync::mpsc;

#[derive(Debug, Clone)]
pub struct CharacterRow {
    pub name: String,
    pub pos: Vec2,
    pub hp: Health,
    pub xp: u64,
    /// Vec com INVENTORY_SLOTS entradas (slots vazios = qty==0).
    pub inventory: Vec<shared::InventorySlot>,
    pub equipment: shared::Equipment,
    /// Vault persistente (INVENTORY_SLOTS slots igual o inv do player).
    pub vault: Vec<shared::InventorySlot>,
    /// Fame: pontuacao de prestigio; ganha matando players/bosses.
    pub fame: u64,
    /// Aura/Poise: pontos ganhos SOMENTE em vitorias PvP. Perde ao ser
    /// morto por outro player. Base pra sistema de stagger futuro.
    pub aura: u64,
}

/// Abre o pool Postgres, garante schema criado.
pub async fn open_pool(database_url: &str) -> Result<PgPool> {
    let pool = PgPoolOptions::new()
        .max_connections(8)
        .connect(database_url)
        .await?;

    // `accounts` e mantida pelo crate `web`; aqui so garantimos que existe
    // (idempotente) para o caso do game server subir antes do web.
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS accounts (
            id             BIGSERIAL PRIMARY KEY,
            username       TEXT NOT NULL UNIQUE,
            email          TEXT NOT NULL UNIQUE,
            password_hash  TEXT NOT NULL,
            created_at     TIMESTAMPTZ NOT NULL DEFAULT NOW()
        )",
    )
    .execute(&pool)
    .await?;

    sqlx::query(
        "ALTER TABLE accounts ADD COLUMN IF NOT EXISTS class TEXT NOT NULL DEFAULT 'warrior'",
    )
    .execute(&pool)
    .await?;

    sqlx::query(
        "CREATE TABLE IF NOT EXISTS characters (
            name     TEXT PRIMARY KEY,
            x        REAL NOT NULL,
            y        REAL NOT NULL,
            hp       INTEGER NOT NULL,
            max_hp   INTEGER NOT NULL,
            updated  BIGINT NOT NULL
        )",
    )
    .execute(&pool)
    .await?;

    // Migracao inline: colunas de progressao. IF NOT EXISTS para idempotencia.
    sqlx::query("ALTER TABLE characters ADD COLUMN IF NOT EXISTS xp BIGINT NOT NULL DEFAULT 0")
        .execute(&pool)
        .await?;
    sqlx::query("ALTER TABLE characters ADD COLUMN IF NOT EXISTS fame BIGINT NOT NULL DEFAULT 0")
        .execute(&pool)
        .await?;
    sqlx::query("ALTER TABLE characters ADD COLUMN IF NOT EXISTS aura BIGINT NOT NULL DEFAULT 0")
        .execute(&pool)
        .await?;

    sqlx::query(
        "CREATE TABLE IF NOT EXISTS inventory (
            character_name TEXT NOT NULL REFERENCES characters(name) ON DELETE CASCADE,
            slot           INTEGER NOT NULL,
            item_id        INTEGER NOT NULL,
            qty            INTEGER NOT NULL,
            PRIMARY KEY (character_name, slot)
        )",
    )
    .execute(&pool)
    .await?;

    sqlx::query(
        "CREATE TABLE IF NOT EXISTS equipment (
            character_name TEXT NOT NULL REFERENCES characters(name) ON DELETE CASCADE,
            slot           TEXT NOT NULL,
            item_id        INTEGER NOT NULL,
            PRIMARY KEY (character_name, slot)
        )",
    )
    .execute(&pool)
    .await?;

    // Vault: bau persistente por personagem. Estrutura igual a inventory.
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS vault (
            character_name TEXT NOT NULL REFERENCES characters(name) ON DELETE CASCADE,
            slot           INTEGER NOT NULL,
            item_id        INTEGER NOT NULL,
            qty            INTEGER NOT NULL,
            PRIMARY KEY (character_name, slot)
        )",
    )
    .execute(&pool)
    .await?;

    Ok(pool)
}

pub async fn load_all(pool: &PgPool) -> Result<HashMap<String, CharacterRow>> {
    let rows = sqlx::query_as::<_, (String, f32, f32, i32, i32, i64, i64, i64)>(
        "SELECT name, x, y, hp, max_hp, xp, fame, aura FROM characters",
    )
    .fetch_all(pool)
    .await?;
    let mut out = HashMap::with_capacity(rows.len());
    for (name, x, y, hp, max_hp, xp, fame, aura) in rows {
        let inv = load_inventory(pool, &name).await?;
        let equip = load_equipment(pool, &name).await?;
        let vault = load_vault(pool, &name).await?;
        out.insert(
            name.clone(),
            CharacterRow {
                name,
                pos: Vec2::new(x, y),
                hp: Health { current: hp, max: max_hp },
                xp: xp.max(0) as u64,
                inventory: inv,
                equipment: equip,
                vault,
                fame: fame.max(0) as u64,
                aura: aura.max(0) as u64,
            },
        );
    }
    Ok(out)
}

async fn load_vault(pool: &PgPool, char_name: &str) -> Result<Vec<shared::InventorySlot>> {
    let mut slots = vec![shared::InventorySlot::default(); shared::INVENTORY_SLOTS];
    let rows = sqlx::query_as::<_, (i32, i32, i32)>(
        "SELECT slot, item_id, qty FROM vault WHERE character_name = $1",
    )
    .bind(char_name)
    .fetch_all(pool)
    .await?;
    for (slot, item_id, qty) in rows {
        if slot < 0 || (slot as usize) >= shared::INVENTORY_SLOTS { continue; }
        if qty <= 0 { continue; }
        slots[slot as usize] = shared::InventorySlot {
            item_id: item_id as u16,
            qty: qty as u32,
        };
    }
    Ok(slots)
}

async fn load_equipment(pool: &PgPool, char_name: &str) -> Result<shared::Equipment> {
    let rows = sqlx::query_as::<_, (String, i32)>(
        "SELECT slot, item_id FROM equipment WHERE character_name = $1",
    )
    .bind(char_name)
    .fetch_all(pool)
    .await?;
    let mut eq = shared::Equipment::default();
    for (slot, item_id) in rows {
        let iid = item_id as u16;
        match slot.as_str() {
            "weapon" => eq.weapon = Some(iid),
            "armor"  => eq.armor  = Some(iid),
            "ring"   => eq.ring   = Some(iid),
            _ => {}
        }
    }
    Ok(eq)
}

async fn load_inventory(pool: &PgPool, char_name: &str) -> Result<Vec<shared::InventorySlot>> {
    let mut slots = vec![shared::InventorySlot::default(); shared::INVENTORY_SLOTS];
    let rows = sqlx::query_as::<_, (i32, i32, i32)>(
        "SELECT slot, item_id, qty FROM inventory WHERE character_name = $1",
    )
    .bind(char_name)
    .fetch_all(pool)
    .await?;
    for (slot, item_id, qty) in rows {
        if slot < 0 || (slot as usize) >= shared::INVENTORY_SLOTS { continue; }
        if qty <= 0 { continue; }
        slots[slot as usize] = shared::InventorySlot {
            item_id: item_id as u16,
            qty: qty as u32,
        };
    }
    Ok(slots)
}

/// Mensagem enviada pela thread do mundo pro writer task.
#[derive(Debug)]
pub struct SaveBatch {
    pub rows: Vec<CharacterRow>,
}

/// Spawn de task background que consome SaveBatch e escreve no DB.
/// Consome o pool — se precisar acessar DB em outros lugares, clone antes.
pub fn spawn_writer(pool: PgPool) -> mpsc::UnboundedSender<SaveBatch> {
    let (tx, mut rx) = mpsc::unbounded_channel::<SaveBatch>();
    tokio::spawn(async move {
        while let Some(batch) = rx.recv().await {
            if let Err(e) = write_batch(&pool, &batch).await {
                tracing::warn!("persist write failed: {e:?}");
            }
        }
        tracing::debug!("persist writer task exiting");
    });
    tx
}

async fn write_batch(pool: &PgPool, batch: &SaveBatch) -> Result<()> {
    if batch.rows.is_empty() {
        return Ok(());
    }
    let mut tx = pool.begin().await?;
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    for row in &batch.rows {
        sqlx::query(
            "INSERT INTO characters (name, x, y, hp, max_hp, xp, fame, aura, updated)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
             ON CONFLICT(name) DO UPDATE SET
               x = EXCLUDED.x,
               y = EXCLUDED.y,
               hp = EXCLUDED.hp,
               max_hp = EXCLUDED.max_hp,
               xp = EXCLUDED.xp,
               fame = EXCLUDED.fame,
               aura = EXCLUDED.aura,
               updated = EXCLUDED.updated",
        )
        .bind(&row.name)
        .bind(row.pos.x)
        .bind(row.pos.y)
        .bind(row.hp.current)
        .bind(row.hp.max)
        .bind(row.xp as i64)
        .bind(row.fame as i64)
        .bind(row.aura as i64)
        .bind(now)
        .execute(&mut *tx)
        .await?;

        // Inventario: delete-all + insert-rows pra ser simples. O FK cascade
        // ja garante que deletar a linha do character limpa a inventory.
        sqlx::query("DELETE FROM inventory WHERE character_name = $1")
            .bind(&row.name)
            .execute(&mut *tx)
            .await?;
        for (i, slot) in row.inventory.iter().enumerate() {
            if slot.qty == 0 { continue; }
            sqlx::query(
                "INSERT INTO inventory (character_name, slot, item_id, qty)
                 VALUES ($1, $2, $3, $4)",
            )
            .bind(&row.name)
            .bind(i as i32)
            .bind(slot.item_id as i32)
            .bind(slot.qty as i32)
            .execute(&mut *tx)
            .await?;
        }

        // Equipment: mesmo padrao delete-all + insert.
        sqlx::query("DELETE FROM equipment WHERE character_name = $1")
            .bind(&row.name)
            .execute(&mut *tx)
            .await?;
        for (slot_name, item_opt) in [
            ("weapon", row.equipment.weapon),
            ("armor",  row.equipment.armor),
            ("ring",   row.equipment.ring),
        ] {
            if let Some(iid) = item_opt {
                sqlx::query(
                    "INSERT INTO equipment (character_name, slot, item_id) VALUES ($1, $2, $3)",
                )
                .bind(&row.name)
                .bind(slot_name)
                .bind(iid as i32)
                .execute(&mut *tx)
                .await?;
            }
        }

        // Vault: mesmo padrao do inventory.
        sqlx::query("DELETE FROM vault WHERE character_name = $1")
            .bind(&row.name)
            .execute(&mut *tx)
            .await?;
        for (i, slot) in row.vault.iter().enumerate() {
            if slot.qty == 0 { continue; }
            sqlx::query(
                "INSERT INTO vault (character_name, slot, item_id, qty)
                 VALUES ($1, $2, $3, $4)",
            )
            .bind(&row.name)
            .bind(i as i32)
            .bind(slot.item_id as i32)
            .bind(slot.qty as i32)
            .execute(&mut *tx)
            .await?;
        }
    }
    tx.commit().await?;
    Ok(())
}
