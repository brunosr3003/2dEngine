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
    /// XP por proficiencia (sword/staff/etc). Indice = Proficiency as u8.
    pub proficiencies: [u64; 6],
    /// Pontos de atributo ainda nao distribuidos (ganhos via level-up).
    pub unspent_points: u32,
    /// Pontos ja alocados em cada stat [HP, MP, Atk, Dex, Wis, Res].
    pub allocated_points: [u32; 6],
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
    // Pontos de atributo: unspent counter + array de 6 alocados.
    sqlx::query("ALTER TABLE characters ADD COLUMN IF NOT EXISTS unspent_points INTEGER NOT NULL DEFAULT 0")
        .execute(&pool)
        .await?;
    sqlx::query("ALTER TABLE characters ADD COLUMN IF NOT EXISTS allocated_points INTEGER[] NOT NULL DEFAULT '{0,0,0,0,0,0}'")
        .execute(&pool)
        .await?;

    sqlx::query(
        "CREATE TABLE IF NOT EXISTS proficiencies (
            character_name TEXT NOT NULL REFERENCES characters(name) ON DELETE CASCADE,
            prof_kind      INTEGER NOT NULL,
            xp             BIGINT  NOT NULL DEFAULT 0,
            PRIMARY KEY (character_name, prof_kind)
        )",
    )
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

    // ── Economy tables ──────────────────────────────────────────────────────
    // Bumpa `economy_version.version` em qualquer ferramenta SQL pra forçar
    // hot-reload no servidor.
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS economy_version (
            id          SMALLINT PRIMARY KEY DEFAULT 1,
            version     BIGINT NOT NULL DEFAULT 1,
            updated_at  TIMESTAMPTZ NOT NULL DEFAULT NOW(),
            CHECK (id = 1)
        )",
    ).execute(&pool).await?;
    sqlx::query("INSERT INTO economy_version (id, version) VALUES (1, 1) ON CONFLICT DO NOTHING")
        .execute(&pool).await?;

    sqlx::query(
        "CREATE TABLE IF NOT EXISTS items (
            id          INTEGER PRIMARY KEY,
            name        TEXT    NOT NULL,
            sell_price  INTEGER NOT NULL DEFAULT 0,
            buy_price   INTEGER,
            shop_order  INTEGER,
            stack_max   INTEGER NOT NULL DEFAULT 1
        )",
    ).execute(&pool).await?;

    sqlx::query(
        "CREATE TABLE IF NOT EXISTS enemy_kinds (
            kind             INTEGER PRIMARY KEY,
            name             TEXT    NOT NULL,
            hp_max           INTEGER NOT NULL,
            speed            REAL    NOT NULL,
            attack_damage    INTEGER NOT NULL,
            attack_cooldown  REAL    NOT NULL,
            detect_range     REAL    NOT NULL,
            attack_range     REAL    NOT NULL,
            kite_dist        REAL,
            proj_count       INTEGER NOT NULL DEFAULT 1,
            xp_reward        BIGINT  NOT NULL,
            defense          INTEGER NOT NULL DEFAULT 0,
            size_scale       REAL    NOT NULL DEFAULT 1.0,
            tint_r           REAL    NOT NULL DEFAULT 1.0,
            tint_g           REAL    NOT NULL DEFAULT 1.0,
            tint_b           REAL    NOT NULL DEFAULT 1.0,
            tint_a           REAL    NOT NULL DEFAULT 1.0
        )",
    ).execute(&pool).await?;

    sqlx::query(
        "CREATE TABLE IF NOT EXISTS loot_drops (
            id          SERIAL PRIMARY KEY,
            enemy_kind  INTEGER NOT NULL REFERENCES enemy_kinds(kind) ON DELETE CASCADE,
            item_id     INTEGER NOT NULL,
            qty_min     INTEGER NOT NULL,
            qty_max     INTEGER NOT NULL,
            chance      REAL    NOT NULL DEFAULT 1.0
        )",
    ).execute(&pool).await?;
    sqlx::query("CREATE INDEX IF NOT EXISTS idx_loot_drops_kind ON loot_drops(enemy_kind)")
        .execute(&pool).await?;

    // Vendor shops — cada vendor tem um shop_id que aponta pra uma lista
    // curada de itens. Permite "espadeiro" que só vende espadas, "alquimista"
    // que só vende poções, etc., independente da categoria genérica.
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS vendor_shops (
            shop_id  INTEGER PRIMARY KEY,
            name     TEXT NOT NULL
        )",
    ).execute(&pool).await?;
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS vendor_shop_items (
            shop_id     INTEGER NOT NULL REFERENCES vendor_shops(shop_id) ON DELETE CASCADE,
            item_id     INTEGER NOT NULL,
            sort_order  INTEGER NOT NULL DEFAULT 0,
            PRIMARY KEY (shop_id, item_id)
        )",
    ).execute(&pool).await?;

    seed_economy_if_needed(&pool).await?;

    Ok(pool)
}

/// Seed inicial: insere defaults pros itens/enemies que ainda não estão no DB.
/// Usa ON CONFLICT DO NOTHING — preserva tweaks manuais. Loot só seeda se a
/// tabela inteira estiver vazia (evita duplicar drops).
async fn seed_economy_if_needed(pool: &PgPool) -> Result<()> {
    use shared::item_id;

    // Itens — (id, name, sell, buy, shop_order, stack_max)
    let items: &[(i32, &str, i32, Option<i32>, Option<i32>, i32)] = &[
        (item_id::GOLD as i32,           "Ouro",            0,    None,           None,    9999),
        (item_id::HEALTH_POTION as i32,  "Poção de Vida",   5,    Some(10),       Some(0), 20),
        (item_id::MANA_POTION as i32,    "Poção de Mana",   7,    Some(15),       Some(1), 20),
        (item_id::GREATER_HEAL as i32,   "Vida Maior",      20,   Some(40),       Some(2), 20),
        (item_id::GREATER_MANA as i32,   "Mana Maior",      25,   Some(50),       Some(3), 20),
        (item_id::STAMINA_POTION as i32, "Poção Stamina",   10,   Some(20),       Some(4), 20),
        (item_id::SWORD as i32,          "Espada",          30,   None,           None,    1),
        (item_id::STAFF as i32,          "Cajado",          30,   None,           None,    1),
        (item_id::DAGGER as i32,         "Adaga",           40,   Some(80),       Some(5), 1),
        (item_id::GREAT_SWORD as i32,    "Espadão",         80,   None,           None,    1),
        (item_id::BOW as i32,            "Arco",            75,   Some(150),      Some(7), 1),
        (item_id::WAND as i32,           "Varinha",         60,   None,           None,    1),
        (item_id::ARMOR as i32,          "Armadura",        25,   None,           None,    1),
        (item_id::SHIELD as i32,         "Escudo",          25,   None,           None,    1),
        (item_id::LEATHER_ARMOR as i32,  "Couro",           60,   Some(120),      Some(6), 1),
        (item_id::PLATE_ARMOR as i32,    "Placa",           100,  None,           None,    1),
        (item_id::ROBE as i32,           "Manto",           50,   None,           None,    1),
        (item_id::RING as i32,           "Anel",            20,   None,           None,    1),
        (item_id::AMULET as i32,         "Amuleto",         90,   Some(180),      Some(8), 1),
        (item_id::LUCKY_RING as i32,     "Anel da Sorte",   80,   None,           None,    1),
        (item_id::GEM as i32,            "Gema",            50,   None,           None,    99),
        (item_id::IRON_INGOT as i32,     "Lingote de Ferro",30,   None,           None,    99),
        (item_id::DRAGON_SCALE as i32,   "Escama de Dragão",200,  None,           None,    99),
    ];
    for (id, name, sell, buy, ord, stack) in items {
        sqlx::query(
            "INSERT INTO items (id, name, sell_price, buy_price, shop_order, stack_max) \
             VALUES ($1, $2, $3, $4, $5, $6) ON CONFLICT (id) DO NOTHING"
        )
        .bind(id).bind(*name).bind(sell).bind(*buy).bind(*ord).bind(stack)
        .execute(pool).await?;
    }

    // Enemy kinds — espelha o array hardcoded antigo. Tuple muito grande;
    // usa struct local pra clareza.
    struct E { kind:i32,name:&'static str,hp:i32,sp:f32,dmg:i32,cd:f32,det:f32,rng:f32,kite:Option<f32>,proj:i32,xp:i64,def:i32,sz:f32,t:[f32;4] }
    let kinds: &[E] = &[
        E{ kind:0, name:"Grunt",     hp:50,  sp:2.0, dmg:10, cd:2.0, det:9.0,  rng:1.8,  kite:None,        proj:1, xp:30,  def:0,  sz:1.0,  t:[1.0,1.0,1.0,1.0] },
        E{ kind:1, name:"Tank",      hp:120, sp:1.3, dmg:18, cd:2.8, det:8.0,  rng:1.8,  kite:None,        proj:1, xp:75,  def:8,  sz:1.3,  t:[1.0,0.35,0.25,1.0] },
        E{ kind:2, name:"Ranger",    hp:35,  sp:2.4, dmg:12, cd:1.5, det:13.0, rng:9.0,  kite:Some(5.0),   proj:1, xp:50,  def:0,  sz:0.85, t:[0.3,0.9,1.0,1.0] },
        E{ kind:3, name:"Ninja",     hp:40,  sp:4.2, dmg:15, cd:1.0, det:11.0, rng:1.8,  kite:None,        proj:1, xp:55,  def:2,  sz:0.75, t:[0.75,0.2,1.0,1.0] },
        E{ kind:4, name:"Mago",      hp:45,  sp:1.4, dmg:22, cd:2.2, det:15.0, rng:12.0, kite:Some(8.0),   proj:1, xp:70,  def:1,  sz:1.0,  t:[0.4,0.4,1.0,1.0] },
        E{ kind:5, name:"Berserker", hp:200, sp:1.5, dmg:28, cd:3.0, det:8.0,  rng:1.8,  kite:None,        proj:1, xp:110, def:4,  sz:1.5,  t:[1.0,0.5,0.1,1.0] },
        E{ kind:6, name:"Arqueiro",  hp:45,  sp:2.8, dmg:14, cd:1.6, det:13.0, rng:9.0,  kite:Some(7.0),   proj:1, xp:60,  def:1,  sz:1.0,  t:[0.2,0.9,0.3,1.0] },
        E{ kind:7, name:"Boss",      hp:700, sp:1.6, dmg:40, cd:2.8, det:18.0, rng:13.0, kite:Some(10.0),  proj:5, xp:600, def:20, sz:2.2,  t:[1.0,0.85,0.15,1.0] },
    ];
    for e in kinds {
        sqlx::query(
            "INSERT INTO enemy_kinds (kind, name, hp_max, speed, attack_damage, attack_cooldown, \
             detect_range, attack_range, kite_dist, proj_count, xp_reward, defense, size_scale, \
             tint_r, tint_g, tint_b, tint_a) VALUES \
             ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17) \
             ON CONFLICT (kind) DO NOTHING"
        )
        .bind(e.kind).bind(e.name).bind(e.hp).bind(e.sp).bind(e.dmg).bind(e.cd)
        .bind(e.det).bind(e.rng).bind(e.kite).bind(e.proj).bind(e.xp).bind(e.def).bind(e.sz)
        .bind(e.t[0]).bind(e.t[1]).bind(e.t[2]).bind(e.t[3])
        .execute(pool).await?;
    }

    // Loot — só seeda se totalmente vazio (rebalanceios manuais não são
    // sobrescritos).
    let loot_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM loot_drops")
        .fetch_one(pool).await?;
    if loot_count == 0 {
        // (enemy_kind, item_id, qty_min, qty_max, chance)
        let drops: &[(i32, u16, i32, i32, f32)] = &[
            // Boss (7) — alto valor + raridades
            (7, item_id::GOLD,           200, 500, 1.0),
            (7, item_id::DRAGON_SCALE,   1,   4,   1.0),
            (7, item_id::GREATER_HEAL,   2,   5,   1.0),
            (7, item_id::GREATER_MANA,   2,   2,   1.0),
            (7, item_id::GREAT_SWORD,    1,   1,   0.55),
            (7, item_id::WAND,           1,   1,   0.55),
            (7, item_id::PLATE_ARMOR,    1,   1,   0.50),
            (7, item_id::ROBE,           1,   1,   0.55),
            (7, item_id::LUCKY_RING,     1,   1,   0.45),
            // Berserker (5) — armadura
            (5, item_id::GOLD,           25,  60,  1.0),
            (5, item_id::PLATE_ARMOR,    1,   1,   0.22),
            (5, item_id::ARMOR,          1,   1,   0.23),
            (5, item_id::GREATER_HEAL,   1,   1,   0.30),
            (5, item_id::IRON_INGOT,     1,   3,   0.20),
            // Mago (4) — staff/wand + mana
            (4, item_id::GOLD,           15,  35,  1.0),
            (4, item_id::WAND,           1,   1,   0.22),
            (4, item_id::STAFF,          1,   1,   0.23),
            (4, item_id::MANA_POTION,    1,   3,   0.35),
            (4, item_id::ROBE,           1,   1,   0.18),
            (4, item_id::GEM,            1,   1,   0.12),
            // Tank (1)
            (1, item_id::GOLD,           15,  40,  1.0),
            (1, item_id::SHIELD,         1,   1,   0.18),
            (1, item_id::ARMOR,          1,   1,   0.25),
            (1, item_id::SWORD,          1,   1,   0.10),
            (1, item_id::HEALTH_POTION,  1,   3,   0.30),
            (1, item_id::IRON_INGOT,     1,   1,   0.15),
            // Ranger/Arqueiro (2 e 6)
            (2, item_id::GOLD,           10,  28,  1.0),
            (2, item_id::BOW,            1,   1,   0.20),
            (2, item_id::RING,           1,   1,   0.13),
            (2, item_id::AMULET,         1,   1,   0.12),
            (2, item_id::STAMINA_POTION, 1,   1,   0.35),
            (6, item_id::GOLD,           10,  28,  1.0),
            (6, item_id::BOW,            1,   1,   0.20),
            (6, item_id::RING,           1,   1,   0.13),
            (6, item_id::AMULET,         1,   1,   0.12),
            (6, item_id::STAMINA_POTION, 1,   1,   0.35),
            // Ninja (3) — leve + dagger
            (3, item_id::GOLD,           8,   22,  1.0),
            (3, item_id::DAGGER,         1,   1,   0.25),
            (3, item_id::LEATHER_ARMOR,  1,   1,   0.22),
            (3, item_id::MANA_POTION,    1,   1,   0.35),
            (3, item_id::LUCKY_RING,     1,   1,   0.10),
            // Grunt (0)
            (0, item_id::GOLD,           4,   14,  1.0),
            (0, item_id::HEALTH_POTION,  1,   1,   0.25),
            (0, item_id::MANA_POTION,    1,   1,   0.15),
            (0, item_id::IRON_INGOT,     1,   1,   0.08),
        ];
        for (kind, item, qmin, qmax, chance) in drops {
            sqlx::query(
                "INSERT INTO loot_drops (enemy_kind, item_id, qty_min, qty_max, chance) \
                 VALUES ($1, $2, $3, $4, $5)"
            )
            .bind(kind).bind(*item as i32).bind(qmin).bind(qmax).bind(chance)
            .execute(pool).await?;
        }
        tracing::info!("economy seed: {} loot drops inseridos", drops.len());
    }

    // Seed dos shops dos vendors. shop_id=1 fica como generalista (legacy
    // do shop antigo). Demais são especializados.
    let shop_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM vendor_shops")
        .fetch_one(pool).await?;
    if shop_count == 0 {
        let shops: &[(i32, &str, &[u16])] = &[
            (1, "Mercador", &[
                item_id::HEALTH_POTION, item_id::MANA_POTION, item_id::GREATER_HEAL,
                item_id::GREATER_MANA, item_id::STAMINA_POTION, item_id::DAGGER,
                item_id::LEATHER_ARMOR, item_id::BOW, item_id::AMULET,
            ]),
            (2, "Espadeiro", &[
                item_id::SWORD, item_id::DAGGER, item_id::GREAT_SWORD,
            ]),
            (3, "Alquimista", &[
                item_id::HEALTH_POTION, item_id::MANA_POTION, item_id::GREATER_HEAL,
                item_id::GREATER_MANA, item_id::STAMINA_POTION,
            ]),
            (4, "Ferreiro", &[
                item_id::ARMOR, item_id::SHIELD, item_id::LEATHER_ARMOR,
                item_id::PLATE_ARMOR,
            ]),
            (5, "Mística", &[
                item_id::WAND, item_id::STAFF, item_id::ROBE, item_id::AMULET,
                item_id::LUCKY_RING, item_id::RING,
            ]),
            (6, "Arqueiro", &[
                item_id::BOW, item_id::STAMINA_POTION,
            ]),
        ];
        for (sid, name, items) in shops {
            sqlx::query(
                "INSERT INTO vendor_shops (shop_id, name) VALUES ($1, $2) ON CONFLICT DO NOTHING"
            ).bind(sid).bind(*name).execute(pool).await?;
            for (i, &item) in items.iter().enumerate() {
                sqlx::query(
                    "INSERT INTO vendor_shop_items (shop_id, item_id, sort_order) \
                     VALUES ($1, $2, $3) ON CONFLICT DO NOTHING"
                )
                .bind(sid).bind(item as i32).bind(i as i32)
                .execute(pool).await?;
            }
        }
        tracing::info!("economy seed: {} vendor shops inseridos", shops.len());
    }
    Ok(())
}

pub async fn load_all(pool: &PgPool) -> Result<HashMap<String, CharacterRow>> {
    let rows = sqlx::query_as::<_, (String, f32, f32, i32, i32, i64, i64, i64, i32, Vec<i32>)>(
        "SELECT name, x, y, hp, max_hp, xp, fame, aura, unspent_points, allocated_points FROM characters",
    )
    .fetch_all(pool)
    .await?;
    let mut out = HashMap::with_capacity(rows.len());
    for (name, x, y, hp, max_hp, xp, fame, aura, unspent, allocated_vec) in rows {
        let inv = load_inventory(pool, &name).await?;
        let equip = load_equipment(pool, &name).await?;
        let vault = load_vault(pool, &name).await?;
        let profs = load_proficiencies(pool, &name).await?;
        let mut allocated = [0u32; 6];
        for (i, v) in allocated_vec.into_iter().enumerate().take(6) {
            allocated[i] = v.max(0) as u32;
        }
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
                proficiencies: profs,
                unspent_points: unspent.max(0) as u32,
                allocated_points: allocated,
            },
        );
    }
    Ok(out)
}

async fn load_proficiencies(pool: &PgPool, char_name: &str) -> Result<[u64; 6]> {
    let mut arr = [0u64; 6];
    let rows = sqlx::query_as::<_, (i32, i64)>(
        "SELECT prof_kind, xp FROM proficiencies WHERE character_name = $1",
    )
    .bind(char_name)
    .fetch_all(pool)
    .await?;
    for (kind, xp) in rows {
        if kind < 0 { continue; }
        let idx = kind as usize;
        if idx < arr.len() {
            arr[idx] = xp.max(0) as u64;
        }
    }
    Ok(arr)
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
        let allocated_vec: Vec<i32> = row.allocated_points.iter().map(|&v| v as i32).collect();
        sqlx::query(
            "INSERT INTO characters (name, x, y, hp, max_hp, xp, fame, aura, unspent_points, allocated_points, updated)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)
             ON CONFLICT(name) DO UPDATE SET
               x = EXCLUDED.x,
               y = EXCLUDED.y,
               hp = EXCLUDED.hp,
               max_hp = EXCLUDED.max_hp,
               xp = EXCLUDED.xp,
               fame = EXCLUDED.fame,
               aura = EXCLUDED.aura,
               unspent_points = EXCLUDED.unspent_points,
               allocated_points = EXCLUDED.allocated_points,
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
        .bind(row.unspent_points as i32)
        .bind(&allocated_vec)
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

        // Proficiencias: upsert por prof_kind.
        for (i, xp) in row.proficiencies.iter().enumerate() {
            if *xp == 0 { continue; }
            sqlx::query(
                "INSERT INTO proficiencies (character_name, prof_kind, xp)
                 VALUES ($1, $2, $3)
                 ON CONFLICT(character_name, prof_kind) DO UPDATE SET xp = EXCLUDED.xp",
            )
            .bind(&row.name)
            .bind(i as i32)
            .bind(*xp as i64)
            .execute(&mut *tx)
            .await?;
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
