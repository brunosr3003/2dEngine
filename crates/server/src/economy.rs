//! Configuração econômica carregada do Postgres em vez de hardcoded.
//!
//! Contém: definições de itens (preços, stack), kinds de inimigo (HP, speed,
//! XP) e tabelas de loot. Acessada via static `OnceCell<RwLock<EconomyConfig>>`
//! pra que helpers como `sell_price_of(id)` funcionem como antes mas leiam do
//! DB. Hot-reload checa `economy_version` a cada 5s e troca atomicamente.

use anyhow::Result;
use once_cell::sync::OnceCell;
use parking_lot::RwLock;
use shared::items::{ItemTemplate, StatRange};
use sqlx::postgres::PgPool;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

#[derive(Clone, Debug)]
pub struct ItemDef {
    pub id:         u16,
    pub name:       String,
    pub sell_price: u32,
    pub buy_price:  Option<u32>,
    pub shop_order: Option<i32>,
    pub stack_max:  u32,
    /// Slot do equipamento ("Weapon", "Armor", "Helm", ...). None = não-equip.
    pub equip_slot: Option<String>,
    /// Item level base — usado se enemy_kinds.loot_item_level for NULL.
    pub item_level: u16,
    /// Posição (col, row) no spritesheet 16×16. Cliente usa como fallback
    /// quando icon_path está vazio.
    pub icon_col:   i32,
    pub icon_row:   i32,
    /// Path no Resources do cliente (ex: "Items/sword"). Se setado,
    /// cliente carrega via Resources.Load — bypass do spritesheet.
    pub icon_path:  Option<String>,
    /// Inativo: server não dropa, não equipa, não usa. Pode vender/guardar.
    pub active:     bool,
    /// Template de stat ranges. Usado por ItemInstance::roll_with_template
    /// no drop pra rolar stats aleatórios.
    pub template:   ItemTemplate,
}

#[derive(Clone, Debug, Default)]
pub struct EnemyKindDef {
    pub kind:            u16,
    pub name:            String,
    pub hp_max:          i32,
    pub speed:           f32,
    pub attack_damage:   i32,
    pub attack_cooldown: f32,
    pub detect_range:    f32,
    pub attack_range:    f32,
    pub kite_dist:       Option<f32>,
    pub proj_count:      u32,
    pub xp_reward:       u64,
    pub defense:         i32,
    pub size_scale:      f32,
    pub tint_rgba:       [f32; 4],
    /// Item level dos drops desse kind. None = usa items.item_level por item.
    pub loot_item_level: Option<u16>,
}

#[derive(Clone, Debug)]
pub struct LootEntry {
    pub item_id: u16,
    pub qty_min: u32,
    pub qty_max: u32,
    pub chance:  f32, // 0.0..=1.0; 1.0 = sempre dropa
}

#[derive(Clone, Debug)]
pub struct VendorShop {
    pub shop_id: u32,
    pub name:    String,
    pub items:   Vec<u16>, // item_ids em ordem (sort_order ASC)
}

#[derive(Default)]
pub struct EconomyConfig {
    pub version:       i64,
    pub items:         HashMap<u16, ItemDef>,
    pub shop_items:    Vec<u16>,                 // legacy global shop list
    pub enemy_kinds:   HashMap<u16, EnemyKindDef>,
    pub loot_tables:   HashMap<u16, Vec<LootEntry>>,
    /// Loot drops por (kind, tier) de farm nodes (Tree/Rock/Flower × T1..T4).
    /// Cada entry rola independente (mesma semântica de `loot_tables`).
    pub farm_loot_tables: HashMap<(String, u8), Vec<LootEntry>>,
    pub vendor_shops:  HashMap<u32, VendorShop>,
}

impl EconomyConfig {
    pub fn item(&self, id: u16) -> Option<&ItemDef> { self.items.get(&id) }

    pub fn sell_price(&self, id: u16) -> u32 {
        self.items.get(&id).map(|i| i.sell_price).unwrap_or(0)
    }

    pub fn stack_max(&self, id: u16) -> u32 {
        self.items.get(&id).map(|i| i.stack_max).unwrap_or(1)
    }

    /// Lista (item_id, buy_price) na ordem da loja (legacy global).
    pub fn shop_listing(&self) -> Vec<(u16, u32)> {
        self.shop_items.iter()
            .filter_map(|id| self.items.get(id).and_then(|i| i.buy_price.map(|p| (*id, p))))
            .collect()
    }

    /// Lista (item_id, buy_price) pra um vendor específico. Se o shop não
    /// existir, retorna vazio. Preço pega do `items.buy_price` (compartilhado);
    /// se item não tem buy_price, é pulado.
    pub fn shop_listing_for(&self, shop_id: u32) -> Vec<(u16, u32)> {
        let Some(shop) = self.vendor_shops.get(&shop_id) else { return Vec::new() };
        shop.items.iter()
            .filter_map(|id| self.items.get(id).and_then(|i| i.buy_price.map(|p| (*id, p))))
            .collect()
    }

    pub fn shop_name_for(&self, shop_id: u32) -> Option<&str> {
        self.vendor_shops.get(&shop_id).map(|s| s.name.as_str())
    }

    pub fn enemy_kind(&self, kind: u16) -> Option<&EnemyKindDef> {
        self.enemy_kinds.get(&kind)
    }

    /// Rola loot drops pra um kind. Cada entry independente: rand < chance →
    /// dropa (qty random entre min..=max). Determinístico via seed.
    /// Items com `active=false` são pulados (não dropam até admin reativar).
    pub fn roll_loot(&self, kind: u16, seed: u64) -> Vec<(u16, u32)> {
        // Match exato primeiro; senao, fallback no kind mais proximo
        // ABAIXO (level-range usa kind=level, ex: lv10 sem tabela cai
        // pra lv7 — mobs de level intermediario nao ficam sem loot).
        let table = self.loot_tables.get(&kind)
            .or_else(|| {
                (0..kind).rev().find_map(|k| self.loot_tables.get(&k))
            });
        let Some(table) = table else { return Vec::new(); };
        roll_entries(table, &self.items, seed)
    }

    /// Mesma semântica de `roll_loot` mas pra farm nodes (kind+tier).
    /// Tier acima de 4 cai pra 4; tier 0 vira 1 pra evitar lookup vazio.
    pub fn roll_farm_loot(&self, kind: &str, tier: u8, seed: u64) -> Vec<(u16, u32)> {
        let t = tier.max(1).min(4);
        let key = (kind.to_string(), t);
        let Some(table) = self.farm_loot_tables.get(&key) else { return Vec::new(); };
        roll_entries(table, &self.items, seed)
    }
}

fn roll_entries(table: &[LootEntry], items: &HashMap<u16, ItemDef>, seed: u64) -> Vec<(u16, u32)> {
    let mut out = Vec::with_capacity(table.len());
    let mut s = seed;
    for entry in table {
        s = lcg(s);
        let r1 = lcg_f32(s);
        if r1 >= entry.chance { continue; }
        if !items.get(&entry.item_id).map(|i| i.active).unwrap_or(true) { continue; }
        s = lcg(s);
        let r2 = lcg_f32(s);
        let span = entry.qty_max.saturating_sub(entry.qty_min) + 1;
        let qty = entry.qty_min + ((r2 * span as f32) as u32).min(span - 1);
        out.push((entry.item_id, qty));
    }
    out
}

/// Loot drops de um farm node (kind + tier). Retorna pares (item_id, qty).
/// Seed deve ser diferente a cada coleta pra evitar resultados previsíveis.
/// Tabela editável via admin (`farm_node_drops`). Default seedado em
/// `seed_economy_if_needed` replica o comportamento legado.
pub fn farm_node_loot(kind: &str, tier: u8, seed: u64) -> Vec<(u16, u32)> {
    cell().read().roll_farm_loot(kind, tier, seed)
}

// LCG dedicado pra rolagem de loot (não usa o do world.rs pra evitar dep cíclica)
fn lcg(seed: u64) -> u64 {
    seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407)
}
fn lcg_f32(seed: u64) -> f32 {
    (seed >> 11) as f32 / (1u64 << 53) as f32
}

// ── Static accessor ──────────────────────────────────────────────────────────

static ECONOMY: OnceCell<Arc<RwLock<EconomyConfig>>> = OnceCell::new();

fn cell() -> &'static RwLock<EconomyConfig> {
    ECONOMY.get().expect("economy não inicializada — chame economy::init() na boot")
}

/// Inicializa o singleton com a config carregada do DB. Chamar 1x na boot.
pub async fn init(pool: &PgPool) -> Result<()> {
    let cfg = load_from_db(pool).await?;
    let _ = ECONOMY.set(Arc::new(RwLock::new(cfg)));
    Ok(())
}

/// Spawn da tarefa de hot-reload. Verifica `economy_version` a cada 5s; se
/// mudou, recarrega tudo. Apenas troca atomicamente — leitores no momento
/// pegam ainda a versão antiga (sem dano, eventualmente consistente).
pub fn spawn_hot_reload(pool: PgPool) {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(5));
        interval.tick().await; // descarta o primeiro (imediato)
        loop {
            interval.tick().await;
            match check_and_reload(&pool).await {
                Ok(true) => {
                    let v = cell().read().version;
                    tracing::info!("economy hot-reload: v{v}");
                }
                Ok(false) => {}
                Err(e) => tracing::warn!("economy reload falhou: {e}"),
            }
        }
    });
}

async fn check_and_reload(pool: &PgPool) -> Result<bool> {
    let v: i64 = sqlx::query_scalar("SELECT version FROM economy_version WHERE id = 1")
        .fetch_one(pool).await?;
    if v == cell().read().version { return Ok(false); }
    let cfg = load_from_db(pool).await?;
    *cell().write() = cfg;
    Ok(true)
}

// ── Lookup helpers (mantém API antiga de shared::) ───────────────────────────

pub fn sell_price_of(id: u16) -> u32 { cell().read().sell_price(id) }

pub fn shop_listing_for(shop_id: u32) -> Vec<(u16, u32)> {
    cell().read().shop_listing_for(shop_id)
}

pub fn shop_name_for(shop_id: u32) -> Option<String> {
    cell().read().shop_name_for(shop_id).map(|s| s.to_string())
}

/// Lista (item_id, sell_price) de TODOS os itens com preço > 0. Usado na
/// abertura do shop pra cliente saber valor de cada item do inventário.
pub fn all_sell_prices() -> Vec<(u16, u32)> {
    cell().read().items.values()
        .filter(|i| i.sell_price > 0)
        .map(|i| (i.id, i.sell_price))
        .collect()
}

/// Multiplicadores de compra/venda de um vendor (buy_mult, sell_mult).
/// Stub: retorna (1.0, 1.0). Futuro: lookup numa tabela `vendor_relationships`
/// que considera reputação do player com aquele vendor específico.
pub fn vendor_modifiers(_vendor_id: u32) -> (f32, f32) {
    (1.0, 1.0)
}
pub fn item_stack_max(id: u16) -> u32 { cell().read().stack_max(id) }
pub fn shop_listing() -> Vec<(u16, u32)> { cell().read().shop_listing() }

/// Multiplier da curva de XP — configuravel via `server_config` table.
/// Carregado uma vez no startup; mudar em DB + restart pra eventos.
static XP_MULT: std::sync::atomic::AtomicU64 =
    std::sync::atomic::AtomicU64::new(shared::DEFAULT_XP_MULTIPLIER);

pub fn xp_multiplier() -> u64 {
    XP_MULT.load(std::sync::atomic::Ordering::Relaxed)
}

pub async fn load_server_config(pool: &sqlx::PgPool) -> anyhow::Result<()> {
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS server_config (
            key   TEXT PRIMARY KEY,
            value DOUBLE PRECISION NOT NULL
        )"
    ).execute(pool).await?;
    sqlx::query("INSERT INTO server_config (key, value) VALUES ('xp_multiplier', $1)
                 ON CONFLICT (key) DO NOTHING")
        .bind(shared::DEFAULT_XP_MULTIPLIER as f64)
        .execute(pool).await?;
    let row: Option<(f64,)> = sqlx::query_as(
        "SELECT value FROM server_config WHERE key='xp_multiplier'"
    ).fetch_optional(pool).await?;
    if let Some((v,)) = row {
        let mult = v.max(1.0) as u64;
        XP_MULT.store(mult, std::sync::atomic::Ordering::Relaxed);
        tracing::info!("[config] xp_multiplier loaded: {} (DB)", mult);
    }
    Ok(())
}

pub fn enemy_def(kind: u16) -> EnemyKindDef {
    cell().read().enemy_kinds.get(&kind).cloned().unwrap_or_default()
}

pub fn enemy_size_scale(kind: u16) -> f32 {
    cell().read().enemy_kinds.get(&kind).map(|e| e.size_scale).unwrap_or(1.0)
}

pub fn enemy_attack_range(kind: u16) -> f32 {
    cell().read().enemy_kinds.get(&kind).map(|e| e.attack_range).unwrap_or(1.8)
}

pub fn enemy_kite_dist(kind: u16) -> Option<f32> {
    cell().read().enemy_kinds.get(&kind).and_then(|e| e.kite_dist)
}

pub fn enemy_proj_count(kind: u16) -> u32 {
    cell().read().enemy_kinds.get(&kind).map(|e| e.proj_count).unwrap_or(1)
}

pub fn enemy_loot_drops(kind: u16, seed: u64) -> Vec<(u16, u32)> {
    cell().read().roll_loot(kind, seed)
}

/// Reverse-index das loot tables: pra cada item dropavel, lista as fontes
/// (mobs + farm nodes) com chance/quantidade. Usado pelo cliente na UI de
/// crafting pra mostrar "como conseguir esse material". Recomputa do
/// snapshot atual do economy cache.
pub fn resource_sources_snapshot() -> Vec<shared::protocol::ItemResourceSources> {
    use shared::protocol::{ItemResourceSources, ResourceSource};
    let cfg = cell().read();
    let mut by_item: HashMap<u16, Vec<ResourceSource>> = HashMap::new();
    for (&kind, table) in &cfg.loot_tables {
        let mob_name = cfg.enemy_kinds.get(&kind)
            .map(|e| e.name.clone())
            .filter(|n| !n.is_empty())
            .unwrap_or_else(|| format!("Mob #{kind}"));
        for entry in table {
            if !cfg.items.get(&entry.item_id).map(|i| i.active).unwrap_or(true) { continue; }
            by_item.entry(entry.item_id).or_default().push(ResourceSource {
                kind:    0,
                name:    mob_name.clone(),
                qty_min: entry.qty_min,
                qty_max: entry.qty_max,
                chance:  entry.chance,
            });
        }
    }
    for ((node_kind, tier), table) in &cfg.farm_loot_tables {
        let name = format!("{node_kind} T{tier}");
        for entry in table {
            if !cfg.items.get(&entry.item_id).map(|i| i.active).unwrap_or(true) { continue; }
            by_item.entry(entry.item_id).or_default().push(ResourceSource {
                kind:    1,
                name:    name.clone(),
                qty_min: entry.qty_min,
                qty_max: entry.qty_max,
                chance:  entry.chance,
            });
        }
    }
    let mut out: Vec<ItemResourceSources> = by_item.into_iter()
        .map(|(item_id, mut sources)| {
            sources.sort_by(|a, b| b.chance.partial_cmp(&a.chance).unwrap_or(std::cmp::Ordering::Equal));
            ItemResourceSources { item_id, sources }
        })
        .collect();
    out.sort_by_key(|e| e.item_id);
    out
}

/// Template de stat ranges do item (carregado do DB). Retorna default
/// (todos 0) se item não existir — `roll_for` checa has_any_range e devolve
/// None nesse caso (item não-equipável).
pub fn item_template_of(id: u16) -> ItemTemplate {
    cell().read().items.get(&id).map(|i| i.template).unwrap_or_default()
}

/// Versão atual do cache em memória — usada pelo world tick pra detectar
/// hot-reload e disparar broadcast de `ItemsConfig`.
pub fn current_version() -> i64 {
    cell().read().version
}

/// Snapshot dos items pro wire `ItemsConfig`. Cliente usa pra sobrescrever
/// nome/icone hardcoded em `ItemInfo.cs`.
pub fn items_config() -> Vec<shared::protocol::ItemConfigEntry> {
    let cfg = cell().read();
    let mut out: Vec<_> = cfg.items.values().map(|i| shared::protocol::ItemConfigEntry {
        id:         i.id,
        name:       i.name.clone(),
        icon_path:  i.icon_path.clone(),
        icon_col:   i.icon_col,
        icon_row:   i.icon_row,
        equip_slot: i.equip_slot.clone(),
        active:     i.active,
    }).collect();
    out.sort_by_key(|e| e.id);
    out
}

/// True se o item está ativo (default true). Usado pra bloquear equip/use
/// no servidor; client pode também consultar via `ItemsConfig.active`.
pub fn is_item_active(id: u16) -> bool {
    cell().read().items.get(&id).map(|i| i.active).unwrap_or(true)
}

/// Item level pra rolagem de drop por kind. Usa override do enemy_kinds
/// se setado, senão volta pro item_level base do próprio item.
pub fn loot_item_level(kind: u16, item_id: u16) -> u16 {
    let cfg = cell().read();
    if let Some(k) = cfg.enemy_kinds.get(&kind) {
        if let Some(lvl) = k.loot_item_level { return lvl; }
    }
    cfg.items.get(&item_id).map(|i| i.item_level).unwrap_or(1)
}

// ── DB load ──────────────────────────────────────────────────────────────────

async fn load_from_db(pool: &PgPool) -> Result<EconomyConfig> {
    let version: i64 = sqlx::query_scalar("SELECT version FROM economy_version WHERE id = 1")
        .fetch_one(pool).await?;

    #[derive(sqlx::FromRow)]
    struct ItemRow {
        id: i32, name: String, sell_price: i32, buy_price: Option<i32>,
        shop_order: Option<i32>, stack_max: i32,
        equip_slot: Option<String>, item_level: i32, icon_col: i32, icon_row: i32,
        icon_path: Option<String>, active: bool,
        hp_min: i32, hp_max: i32, mp_min: i32, mp_max: i32,
        atk_min: i32, atk_max: i32, def_min: i32, def_max: i32,
        dex_min: i32, dex_max: i32, wis_min: i32, wis_max: i32,
    }
    let item_rows: Vec<ItemRow> = sqlx::query_as(
        "SELECT id, name, sell_price, buy_price, shop_order, stack_max, \
                equip_slot, item_level, icon_col, icon_row, icon_path, active, \
                hp_min, hp_max, mp_min, mp_max, atk_min, atk_max, \
                def_min, def_max, dex_min, dex_max, wis_min, wis_max \
         FROM items"
    ).fetch_all(pool).await?;
    let mut items = HashMap::with_capacity(item_rows.len());
    let mut shop_collected: Vec<(i32, u16)> = Vec::new(); // (order, id)
    for r in item_rows {
        let id_u16 = r.id as u16;
        if let Some(ord) = r.shop_order {
            shop_collected.push((ord, id_u16));
        }
        items.insert(id_u16, ItemDef {
            id:         id_u16,
            name:       r.name,
            sell_price: r.sell_price.max(0) as u32,
            buy_price:  r.buy_price.map(|v| v.max(0) as u32),
            shop_order: r.shop_order,
            stack_max:  r.stack_max.max(1) as u32,
            equip_slot: r.equip_slot,
            item_level: r.item_level.max(1) as u16,
            icon_col:   r.icon_col,
            icon_row:   r.icon_row,
            icon_path:  r.icon_path,
            active:     r.active,
            template: ItemTemplate {
                hp_max:        StatRange::new(r.hp_min, r.hp_max),
                mp_max:        StatRange::new(r.mp_min, r.mp_max),
                attack_damage: StatRange::new(r.atk_min, r.atk_max),
                defense:       StatRange::new(r.def_min, r.def_max),
                dex:           StatRange::new(r.dex_min, r.dex_max),
                wis:           StatRange::new(r.wis_min, r.wis_max),
            },
        });
    }
    shop_collected.sort_by_key(|(o, _)| *o);
    let shop_items: Vec<u16> = shop_collected.into_iter().map(|(_, id)| id).collect();

    #[derive(sqlx::FromRow)]
    struct EnemyRow {
        kind: i32, name: String, hp_max: i32, speed: f32, attack_damage: i32,
        attack_cooldown: f32, detect_range: f32, attack_range: f32,
        kite_dist: Option<f32>, proj_count: i32, xp_reward: i64, defense: i32,
        size_scale: f32, tint_r: f32, tint_g: f32, tint_b: f32, tint_a: f32,
        loot_item_level: Option<i32>,
    }
    let enemy_rows: Vec<EnemyRow> = sqlx::query_as(
        "SELECT kind, name, hp_max, speed, attack_damage, attack_cooldown, detect_range, \
                attack_range, kite_dist, proj_count, xp_reward, defense, size_scale, \
                tint_r, tint_g, tint_b, tint_a, loot_item_level FROM enemy_kinds"
    ).fetch_all(pool).await?;
    let mut enemy_kinds = HashMap::with_capacity(enemy_rows.len());
    for r in enemy_rows {
        enemy_kinds.insert(r.kind as u16, EnemyKindDef {
            kind: r.kind as u16, name: r.name,
            hp_max: r.hp_max, speed: r.speed, attack_damage: r.attack_damage,
            attack_cooldown: r.attack_cooldown,
            detect_range: r.detect_range, attack_range: r.attack_range,
            kite_dist: r.kite_dist,
            proj_count: r.proj_count.max(1) as u32,
            xp_reward: r.xp_reward.max(0) as u64,
            defense: r.defense, size_scale: r.size_scale,
            tint_rgba: [r.tint_r, r.tint_g, r.tint_b, r.tint_a],
            loot_item_level: r.loot_item_level.map(|v| v.max(1) as u16),
        });
    }

    let loot_rows: Vec<(i32, i32, i32, i32, f32)> =
        sqlx::query_as("SELECT enemy_kind, item_id, qty_min, qty_max, chance FROM loot_drops ORDER BY id")
            .fetch_all(pool).await?;
    let mut loot_tables: HashMap<u16, Vec<LootEntry>> = HashMap::new();
    for (kind, item_id, qmin, qmax, chance) in loot_rows {
        loot_tables.entry(kind as u16).or_default().push(LootEntry {
            item_id: item_id as u16,
            qty_min: qmin.max(0) as u32,
            qty_max: qmax.max(qmin) as u32,
            chance:  chance.clamp(0.0, 1.0),
        });
    }

    let farm_rows: Vec<(String, i32, i32, i32, i32, f32)> = sqlx::query_as(
        "SELECT kind, tier, item_id, qty_min, qty_max, chance \
         FROM farm_node_drops ORDER BY kind, tier, id"
    ).fetch_all(pool).await?;
    let mut farm_loot_tables: HashMap<(String, u8), Vec<LootEntry>> = HashMap::new();
    for (kind, tier, item_id, qmin, qmax, chance) in farm_rows {
        let t = tier.max(1).min(4) as u8;
        farm_loot_tables.entry((kind, t)).or_default().push(LootEntry {
            item_id: item_id as u16,
            qty_min: qmin.max(0) as u32,
            qty_max: qmax.max(qmin) as u32,
            chance:  chance.clamp(0.0, 1.0),
        });
    }

    let shop_rows: Vec<(i32, String)> = sqlx::query_as(
        "SELECT shop_id, name FROM vendor_shops"
    ).fetch_all(pool).await?;
    let mut vendor_shops = HashMap::with_capacity(shop_rows.len());
    for (sid, name) in shop_rows {
        vendor_shops.insert(sid as u32, VendorShop {
            shop_id: sid as u32, name, items: Vec::new(),
        });
    }
    let item_rows: Vec<(i32, i32)> = sqlx::query_as(
        "SELECT shop_id, item_id FROM vendor_shop_items ORDER BY shop_id, sort_order"
    ).fetch_all(pool).await?;
    for (sid, item_id) in item_rows {
        if let Some(shop) = vendor_shops.get_mut(&(sid as u32)) {
            shop.items.push(item_id as u16);
        }
    }

    Ok(EconomyConfig {
        version, items, shop_items, enemy_kinds, loot_tables, farm_loot_tables, vendor_shops,
    })
}

/// O kind do CHEFE na tabela.
///
/// Um numero e nao um `build_boss`: o que faz o chefe ser chefe sao os numeros
/// dele (700 de vida contra 50 do Grunt), e nao uma classe procedural.
pub const KIND_CHEFE: u16 = 7;

/// Um bicho da tabela adequado ao nivel pedido.
///
/// A tabela e' pequena e ordenada por xp, que cresce junto com a dificuldade.
/// Entao "nivel" vira posicao na lista: nivel baixo pega os primeiros, nivel
/// alto abre a escolha ate' o fim. Sorteio simples de proposito — mob nao tem
/// classe nem build, e uma tabela de oito linhas nao pede mais que isto.
pub fn kind_para_nivel(nivel: u32, semente: u64) -> u16 {
    let mut comuns: Vec<u16> = cell()
        .read()
        .enemy_kinds
        .keys()
        .copied()
        .filter(|&k| k != KIND_CHEFE)
        .collect();
    // Ordem estavel: a tabela vem de um mapa, e sorteio sobre ordem de hash
    // daria um bicho diferente a cada reinicio do servidor.
    comuns.sort_unstable();
    if comuns.is_empty() {
        return 0;
    }
    // Ate' onde a escolha vai: um bicho novo a cada tres niveis.
    let teto = ((nivel as usize / 3) + 1).min(comuns.len());
    comuns[(semente as usize) % teto]
}
