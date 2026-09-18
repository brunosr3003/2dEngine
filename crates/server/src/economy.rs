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
    pub id: u16,
    pub name: String,
    pub sell_price: u32,
    pub buy_price: Option<u32>,
    pub shop_order: Option<i32>,
    pub stack_max: u32,
    /// Slot do equipamento ("Weapon", "Armor", "Helm", ...). None = não-equip.
    pub equip_slot: Option<String>,
    /// Item level base — usado se enemy_kinds.loot_item_level for NULL.
    pub item_level: u16,
    /// Posição (col, row) no spritesheet 16×16. Cliente usa como fallback
    /// quando icon_path está vazio.
    pub icon_col: i32,
    pub icon_row: i32,
    /// Path no Resources do cliente (ex: "Items/sword"). Se setado,
    /// cliente carrega via Resources.Load — bypass do spritesheet.
    pub icon_path: Option<String>,
    /// Inativo: server não dropa, não equipa, não usa. Pode vender/guardar.
    pub active: bool,
    /// Vinculado: nao entra no mercado global (docs/MERCADO.md).
    pub vinculado: bool,
    /// Template de stat ranges. Usado por ItemInstance::roll_with_template
    /// no drop pra rolar stats aleatórios.
    pub template: ItemTemplate,
}

#[derive(Clone, Debug, Default)]
pub struct EnemyKindDef {
    pub kind: u16,
    pub name: String,
    pub hp_max: i32,
    pub speed: f32,
    pub attack_damage: i32,
    pub attack_cooldown: f32,
    pub detect_range: f32,
    pub attack_range: f32,
    pub kite_dist: Option<f32>,
    pub proj_count: u32,
    pub xp_reward: u64,
    pub defense: i32,
    pub size_scale: f32,
    pub tint_rgba: [f32; 4],
    /// Item level dos drops desse kind. None = usa items.item_level por item.
    pub loot_item_level: Option<u16>,
}

#[derive(Clone, Debug)]
pub struct LootEntry {
    pub item_id: u16,
    pub qty_min: u32,
    pub qty_max: u32,
    pub chance: f32, // 0.0..=1.0; 1.0 = sempre dropa
}

#[derive(Clone, Debug)]
pub struct VendorShop {
    pub shop_id: u32,
    pub name: String,
    pub items: Vec<u16>, // item_ids em ordem (sort_order ASC)
}

#[derive(Default)]
pub struct EconomyConfig {
    pub version: i64,
    pub items: HashMap<u16, ItemDef>,
    pub shop_items: Vec<u16>, // legacy global shop list
    pub enemy_kinds: HashMap<u16, EnemyKindDef>,
    pub loot_tables: HashMap<u16, Vec<LootEntry>>,
    /// Loot drops por (kind, tier) de farm nodes (Tree/Rock/Flower × T1..T4).
    /// Cada entry rola independente (mesma semântica de `loot_tables`).
    pub farm_loot_tables: HashMap<(String, u8), Vec<LootEntry>>,
    pub vendor_shops: HashMap<u32, VendorShop>,
}

impl EconomyConfig {
    pub fn item(&self, id: u16) -> Option<&ItemDef> {
        self.items.get(&id)
    }

    pub fn sell_price(&self, id: u16) -> u32 {
        self.items.get(&id).map(|i| i.sell_price).unwrap_or(0)
    }

    pub fn stack_max(&self, id: u16) -> u32 {
        self.items.get(&id).map(|i| i.stack_max).unwrap_or(1)
    }

    /// Lista (item_id, buy_price) na ordem da loja (legacy global).
    pub fn shop_listing(&self) -> Vec<(u16, u32)> {
        self.shop_items
            .iter()
            .filter_map(|id| {
                self.items
                    .get(id)
                    .and_then(|i| i.buy_price.map(|p| (*id, p)))
            })
            .collect()
    }

    /// Lista (item_id, buy_price) pra um vendor específico. Se o shop não
    /// existir, retorna vazio. Preço pega do `items.buy_price` (compartilhado);
    /// se item não tem buy_price, é pulado.
    pub fn shop_listing_for(&self, shop_id: u32) -> Vec<(u16, u32)> {
        let Some(shop) = self.vendor_shops.get(&shop_id) else {
            return Vec::new();
        };
        shop.items
            .iter()
            .filter_map(|id| {
                self.items
                    .get(id)
                    .and_then(|i| i.buy_price.map(|p| (*id, p)))
            })
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
        self.roll_loot_com_sorte(kind, seed, 1.0)
    }

    /// `roll_loot` com a Pocao de Sorte: `mult` multiplica a chance de cada
    /// linha que nao e' garantida (1,0 = sem sorte).
    pub fn roll_loot_com_sorte(&self, kind: u16, seed: u64, mult: f32) -> Vec<(u16, u32)> {
        // Match exato primeiro; senao, fallback no kind mais proximo
        // ABAIXO (level-range usa kind=level, ex: lv10 sem tabela cai
        // pra lv7 — mobs de level intermediario nao ficam sem loot).
        let table = self
            .loot_tables
            .get(&kind)
            .or_else(|| (0..kind).rev().find_map(|k| self.loot_tables.get(&k)));
        let Some(table) = table else {
            return Vec::new();
        };
        roll_entries(table, &self.items, seed, mult)
            .into_iter()
            .filter(|(id, _)| self.permitido_em_mob(*id))
            .collect()
    }

    /// Vale tambem para tabelas antigas ou reintroduzidas pelo hot-reload.
    fn permitido_em_mob(&self, id: u16) -> bool {
        shared::equip_slot_of(id).is_none()
            && self
                .items
                .get(&id)
                .is_some_and(|i| i.active && i.equip_slot.as_deref().is_none_or(|s| s.is_empty()))
    }

    /// Mesma semântica de `roll_loot` mas pra farm nodes (kind+tier).
    /// Tier acima de 4 cai pra 4; tier 0 vira 1 pra evitar lookup vazio.
    pub fn roll_farm_loot(&self, kind: &str, tier: u8, seed: u64) -> Vec<(u16, u32)> {
        self.roll_farm_loot_com_sorte(kind, tier, seed, 1.0)
    }

    /// `roll_farm_loot` com a Pocao de Sorte (ver `roll_loot_com_sorte`).
    pub fn roll_farm_loot_com_sorte(
        &self,
        kind: &str,
        tier: u8,
        seed: u64,
        mult: f32,
    ) -> Vec<(u16, u32)> {
        let t = tier.max(1).min(4);
        let key = (kind.to_string(), t);
        let Some(table) = self.farm_loot_tables.get(&key) else {
            return Vec::new();
        };
        roll_entries(table, &self.items, seed, mult)
    }
}

fn roll_entries(
    table: &[LootEntry],
    items: &HashMap<u16, ItemDef>,
    seed: u64,
    mult: f32,
) -> Vec<(u16, u32)> {
    let mut out = Vec::with_capacity(table.len());
    let mut s = seed;
    for entry in table {
        s = lcg(s);
        let r1 = lcg_f32(s);
        // Sorte so' mexe no que nao e' garantido; garantido continua 100%.
        let chance = if entry.chance < 1.0 {
            (entry.chance * mult).min(1.0)
        } else {
            entry.chance
        };
        if chance < 1.0 && r1 >= chance {
            continue;
        }
        if !items.get(&entry.item_id).map(|i| i.active).unwrap_or(true) {
            continue;
        }
        s = lcg(s);
        let r2 = lcg_f32(s);
        let span = entry.qty_max.saturating_sub(entry.qty_min) + 1;
        let qty = entry.qty_min + ((r2 * span as f32) as u32).min(span - 1);
        out.push((entry.item_id, qty));
    }
    out
}

#[cfg(test)]
mod testes_de_sorte {
    use super::*;

    #[test]
    fn sorte_sobe_a_chance_e_nao_mexe_na_garantida() {
        let tabela = [
            LootEntry {
                item_id: 1,
                qty_min: 1,
                qty_max: 1,
                chance: 1.0,
            },
            LootEntry {
                item_id: 2,
                qty_min: 1,
                qty_max: 1,
                chance: 0.5,
            },
        ];
        let itens = HashMap::new();
        let conta = |mult: f32| {
            let (mut garantida, mut rara) = (0, 0);
            for s in 0..20_000u64 {
                for (id, _) in roll_entries(&tabela, &itens, lcg(s ^ 0xABCD), mult) {
                    if id == 1 {
                        garantida += 1
                    } else {
                        rara += 1
                    }
                }
            }
            (garantida, rara)
        };
        let (g1, r1) = conta(1.0);
        let (g2, r2) = conta(shared::mult_de_sorte(0, 10));
        assert_eq!((g1, g2), (20_000, 20_000), "garantida continua garantida");
        let (f1, f2) = (r1 as f32 / 20_000.0, r2 as f32 / 20_000.0);
        assert!((f1 - 0.5).abs() < 0.03, "sem sorte ~50%: {f1}");
        assert!((f2 - 0.6).abs() < 0.03, "com sorte ~60%: {f2}");
    }
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
    seed.wrapping_mul(6364136223846793005)
        .wrapping_add(1442695040888963407)
}
fn lcg_f32(seed: u64) -> f32 {
    (seed >> 11) as f32 / (1u64 << 53) as f32
}

// ── Static accessor ──────────────────────────────────────────────────────────

static ECONOMY: OnceCell<Arc<RwLock<EconomyConfig>>> = OnceCell::new();

fn cell() -> &'static RwLock<EconomyConfig> {
    ECONOMY
        .get()
        .expect("economy não inicializada — chame economy::init() na boot")
}

#[cfg(test)]
pub(crate) fn init_vazia_para_testes() {
    let _ = ECONOMY.set(Arc::new(RwLock::new(EconomyConfig::default())));
}

/// Poe um item no cache dos testes (o que o banco daria em producao).
#[cfg(test)]
pub(crate) fn por_item_para_testes(id: u16, stack_max: u32, template: ItemTemplate) {
    cell().write().items.insert(
        id,
        ItemDef {
            id,
            name: format!("item {id}"),
            sell_price: 0,
            buy_price: None,
            shop_order: None,
            stack_max,
            equip_slot: None,
            item_level: 1,
            icon_col: 0,
            icon_row: 0,
            icon_path: None,
            active: true,
            vinculado: false,
            template,
        },
    );
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
        .fetch_one(pool)
        .await?;
    if v == cell().read().version {
        return Ok(false);
    }
    let cfg = load_from_db(pool).await?;
    *cell().write() = cfg;
    Ok(true)
}

// ── Lookup helpers (mantém API antiga de shared::) ───────────────────────────

pub fn sell_price_of(id: u16) -> u32 {
    cell().read().sell_price(id)
}

pub fn shop_listing_for(shop_id: u32) -> Vec<(u16, u32)> {
    cell().read().shop_listing_for(shop_id)
}

pub fn shop_name_for(shop_id: u32) -> Option<String> {
    cell().read().shop_name_for(shop_id).map(|s| s.to_string())
}

/// Lista (item_id, sell_price) de TODOS os itens com preço > 0. Usado na
/// abertura do shop pra cliente saber valor de cada item do inventário.
pub fn all_sell_prices() -> Vec<(u16, u32)> {
    cell()
        .read()
        .items
        .values()
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
pub fn item_stack_max(id: u16) -> u32 {
    cell().read().stack_max(id)
}
pub fn shop_listing() -> Vec<(u16, u32)> {
    cell().read().shop_listing()
}

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
        )",
    )
    .execute(pool)
    .await?;
    sqlx::query(
        "INSERT INTO server_config (key, value) VALUES ('xp_multiplier', $1)
                 ON CONFLICT (key) DO NOTHING",
    )
    .bind(shared::DEFAULT_XP_MULTIPLIER as f64)
    .execute(pool)
    .await?;
    let row: Option<(f64,)> =
        sqlx::query_as("SELECT value FROM server_config WHERE key='xp_multiplier'")
            .fetch_optional(pool)
            .await?;
    if let Some((v,)) = row {
        let mult = v.max(1.0) as u64;
        XP_MULT.store(mult, std::sync::atomic::Ordering::Relaxed);
        tracing::info!("[config] xp_multiplier loaded: {} (DB)", mult);
    }
    Ok(())
}

pub fn enemy_def(kind: u16) -> EnemyKindDef {
    cell()
        .read()
        .enemy_kinds
        .get(&kind)
        .cloned()
        .unwrap_or_default()
}

/// Nome do item pra texto de UI. Item fora do cache vira "item N".
pub fn nome_do_item(id: u16) -> String {
    cell()
        .read()
        .item(id)
        .map(|i| i.name.clone())
        .unwrap_or_else(|| format!("item {id}"))
}

pub fn enemy_size_scale(kind: u16) -> f32 {
    cell()
        .read()
        .enemy_kinds
        .get(&kind)
        .map(|e| e.size_scale)
        .unwrap_or(1.0)
}

pub fn enemy_attack_range(kind: u16) -> f32 {
    cell()
        .read()
        .enemy_kinds
        .get(&kind)
        .map(|e| e.attack_range)
        .unwrap_or(1.8)
}

pub fn enemy_kite_dist(kind: u16) -> Option<f32> {
    cell()
        .read()
        .enemy_kinds
        .get(&kind)
        .and_then(|e| e.kite_dist)
}

pub fn enemy_proj_count(kind: u16) -> u32 {
    cell()
        .read()
        .enemy_kinds
        .get(&kind)
        .map(|e| e.proj_count)
        .unwrap_or(1)
}

/// Os kinds comuns (sem o chefe), na MESMA ordem do sorteio de
/// `kind_para_nivel`: o indice de um kind aqui diz a partir de que nivel ele
/// pode nascer.
pub fn kinds_comuns() -> Vec<u16> {
    let mut k: Vec<u16> = cell()
        .read()
        .enemy_kinds
        .keys()
        .copied()
        .filter(|&k| k != KIND_CHEFE && !KINDS_DE_PRAIA.contains(&k))
        .collect();
    k.sort_unstable();
    k
}

/// Kinds comuns cuja tabela de loot tem `item` (com chance).
pub fn kinds_que_dropam(item: u16) -> Vec<u16> {
    let c = cell().read();
    let mut k: Vec<u16> = c
        .loot_tables
        .iter()
        .filter(|(kind, t)| {
            **kind != KIND_CHEFE
                && !KINDS_DE_PRAIA.contains(kind)
                && t.iter().any(|e| e.item_id == item && e.chance > 0.0)
        })
        .map(|(kind, _)| *kind)
        .collect();
    k.sort_unstable();
    k
}

/// A coleta entrega `item`? `(tronco, pedra)`.
pub fn coleta_fornece(item: u16) -> (bool, bool) {
    let c = cell().read();
    let tem = |nome: &str| {
        c.farm_loot_tables
            .iter()
            .any(|((k, _), t)| k == nome && t.iter().any(|e| e.item_id == item && e.chance > 0.0))
    };
    (tem("Tree"), tem("Rock"))
}

/// As linhas da PEDRA em `farm_node_drops`: `(tier da tabela, item, min, max,
/// chance)`. Uma fonte so' pro seed (M25) e pros testes de proporcao — a
/// tabela do doc (`docs/ECONOMIA_DE_CRAFT.md`) nao pode morar em dois lugares.
///
/// O tier da TABELA e' o tier do MATERIAL, nao o da pedra: a pedra sorteia a
/// cor com `shared::tier_do_rendimento` e rola a linha daquela cor.
pub fn linhas_da_pedra() -> Vec<(u8, u16, i32, i32, f32)> {
    use shared::item_id::*;
    // A taxa segue o CUSTO: o que a receita pede em 300 cai mais que o que ela
    // pede em 100. As CHAVES (Escama, Garra, Chifre, Couro) nao estao aqui:
    // so' caem de chefe e de dungeon/raid (`shared::chaves`).
    const COLORIDOS: [(u16, i32, i32, f32); 8] = [
        (STEEL, 3, 6, 0.55),
        (PLATINUM, 3, 6, 0.30),
        (DARK_HEART_STONE, 2, 4, 0.12),
        (MOON_SHADOW_STONE, 2, 4, 0.12),
        (QUINTESSENCE, 2, 4, 0.12),
        (EXORCISM_BAUBLE, 2, 4, 0.12),
        (ILLUMINATING_FRAGMENT, 2, 4, 0.12),
        (ANIMA_STONE, 2, 4, 0.12),
    ];
    // Sem cor: caem igual em qualquer pedra.
    const INCOLORES: [(u16, i32, i32, f32); 3] = [
        (COPPER, 40, 120, 1.00),
        (DARKSTEEL, 10, 25, 0.35),
        (GLITTERING_POWDER, 1, 1, 0.03),
    ];
    let mut v = Vec::with_capacity(4 * (COLORIDOS.len() + INCOLORES.len()));
    for tier in 1..=4u8 {
        for &(base, mn, mx, chance) in &COLORIDOS {
            v.push((tier, na_cor(base, tier), mn, mx, chance));
        }
        for &(id, mn, mx, chance) in &INCOLORES {
            v.push((tier, id, mn, mx, chance));
        }
    }
    v
}

pub fn enemy_loot_drops(kind: u16, seed: u64) -> Vec<(u16, u32)> {
    cell().read().roll_loot(kind, seed)
}

/// Loot de bicho com a Pocao de Sorte de quem matou.
pub fn enemy_loot_drops_com_sorte(kind: u16, seed: u64, mult: f32) -> Vec<(u16, u32)> {
    cell().read().roll_loot_com_sorte(kind, seed, mult)
}

/// Coleta com a Pocao de Sorte de quem coletou.
pub fn farm_node_loot_com_sorte(kind: &str, tier: u8, seed: u64, mult: f32) -> Vec<(u16, u32)> {
    cell()
        .read()
        .roll_farm_loot_com_sorte(kind, tier, seed, mult)
}

/// O que o "Onde obter" usa e nao mora no `EconomyConfig`.
pub struct OutrasFontes<'a> {
    /// Kinds de bicho comum (zonas de spawn e praia).
    pub mobs: &'a [u16],
    /// Ilhas (indice de `ARQUIPELAGO`) onde cada bicho nasce.
    pub ilhas_do_bicho: HashMap<u16, Vec<u8>>,
    /// (kind, nome, nivel, ilha, itens com chance) dos chefes do mundo.
    pub chefes: Vec<(u16, String, u16, u8, Vec<(u16, f32)>)>,
    /// Lojas que existem de verdade num NPC da vila.
    pub lojas_da_vila: &'a [u32],
    pub receitas: &'a [shared::protocol::CraftRecipeNet],
    pub missoes: &'a [shared::quests::QuestDef],
}

/// "Onde obter" (docs/ONDE_OBTER.md): pra cada item, de onde ele sai. Pura:
/// o snapshot passa o cache da economia e o resto.
///
/// Coleta: a tabela da PEDRA e' por cor do MATERIAL; a pedra de cor `p`
/// entrega cada cor na proporcao de `RENDIMENTO_DA_PEDRA`, entao a fonte e'
/// a pedra (com a chance ja' multiplicada). Arvore rola sempre a linha 1.
pub fn fontes_de_itens(
    cfg: &EconomyConfig,
    o: &OutrasFontes,
) -> Vec<shared::protocol::ItemResourceSources> {
    use shared::protocol::{FonteDeItem, ItemResourceSources};
    fn junta(m: &mut HashMap<u16, Vec<FonteDeItem>>, cfg: &EconomyConfig, id: u16, f: FonteDeItem) {
        if id != 0 && cfg.items.get(&id).is_none_or(|i| i.active) {
            m.entry(id).or_default().push(f);
        }
    }
    let mut por_item: HashMap<u16, Vec<FonteDeItem>> = HashMap::new();
    for &kind in o.mobs {
        let Some(tabela) = cfg.loot_tables.get(&kind) else {
            continue;
        };
        let nome = cfg
            .enemy_kinds
            .get(&kind)
            .map(|e| e.name.clone())
            .filter(|n| !n.is_empty())
            .unwrap_or_else(|| format!("Bicho {kind}"));
        for e in tabela
            .iter()
            .filter(|e| e.chance > 0.0 && cfg.permitido_em_mob(e.item_id))
        {
            let ilhas = o.ilhas_do_bicho.get(&kind).cloned().unwrap_or_default();
            junta(
                &mut por_item,
                cfg,
                e.item_id,
                FonteDeItem::Mob {
                    kind,
                    nome: nome.clone(),
                    chance: e.chance,
                    qty_min: e.qty_min,
                    qty_max: e.qty_max,
                    ilhas,
                },
            );
        }
    }
    if let Some(tabela) = cfg.farm_loot_tables.get(&("Tree".to_string(), 1)) {
        for e in tabela.iter().filter(|e| e.chance > 0.0) {
            junta(
                &mut por_item,
                cfg,
                e.item_id,
                FonteDeItem::Coleta {
                    tipo: 0,
                    chance: e.chance,
                    qty_min: e.qty_min,
                    qty_max: e.qty_max,
                },
            );
        }
    }
    for pedra in 1..=4u8 {
        for cor in 1..=4u8 {
            let peso = shared::RENDIMENTO_DA_PEDRA[pedra as usize][cor as usize - 1];
            if peso == 0 {
                continue;
            }
            let Some(tabela) = cfg.farm_loot_tables.get(&("Rock".to_string(), cor)) else {
                continue;
            };
            for e in tabela.iter().filter(|e| e.chance > 0.0) {
                let chance = e.chance * peso as f32 / 100.0;
                let ja = por_item.get_mut(&e.item_id).and_then(|v| {
                    v.iter_mut()
                        .find(|f| matches!(f, FonteDeItem::Coleta { tipo, .. } if *tipo == pedra))
                });
                match ja {
                    // Item sem cor (cobre) sai em toda linha: soma na mesma pedra.
                    Some(FonteDeItem::Coleta { chance: c, .. }) => *c = (*c + chance).min(1.0),
                    _ => junta(
                        &mut por_item,
                        cfg,
                        e.item_id,
                        FonteDeItem::Coleta {
                            tipo: pedra,
                            chance,
                            qty_min: e.qty_min,
                            qty_max: e.qty_max,
                        },
                    ),
                }
            }
        }
    }
    for (kind, nome, nivel, ilha, itens) in &o.chefes {
        for &(id, chance) in itens.iter().filter(|x| x.1 > 0.0) {
            junta(
                &mut por_item,
                cfg,
                id,
                FonteDeItem::ChefeDoMundo {
                    kind: *kind,
                    nome: nome.clone(),
                    nivel: *nivel,
                    chance,
                    ilha: *ilha,
                },
            );
        }
    }
    for &loja in o.lojas_da_vila {
        let Some(shop) = cfg.vendor_shops.get(&loja) else {
            continue;
        };
        for (id, preco) in cfg.shop_listing_for(loja) {
            junta(
                &mut por_item,
                cfg,
                id,
                FonteDeItem::Vendedor {
                    loja,
                    nome: shop.name.clone(),
                    preco,
                },
            );
        }
    }
    for r in o.receitas {
        junta(
            &mut por_item,
            cfg,
            r.output_item_id,
            FonteDeItem::Craft {
                receita: r.id,
                nome: r.name.clone(),
                nivel_min: r.nivel_min,
            },
        );
    }
    for q in o.missoes {
        for id in [q.reward_item, q.reward_item2] {
            if id != 0 {
                junta(
                    &mut por_item,
                    cfg,
                    id,
                    FonteDeItem::Missao {
                        quest: q.id,
                        titulo: q.title.to_string(),
                        diaria: q.daily,
                    },
                );
            }
        }
    }
    for id in shared::item_id::todas_as_chaves() {
        junta(&mut por_item, cfg, id, FonteDeItem::DungeonRaid);
    }
    // Calendario de presenca (docs/CALENDARIO.md): os dias em que o item sai.
    for (id, dias) in shared::presenca::itens_com_dias(shared::presenca::EVENTOS) {
        junta(&mut por_item, cfg, id, FonteDeItem::Calendario { dias });
    }
    let mut out: Vec<ItemResourceSources> = por_item
        .into_iter()
        .map(|(item_id, mut sources)| {
            sources.dedup();
            ItemResourceSources { item_id, sources }
        })
        .collect();
    out.sort_by_key(|e| e.item_id);
    out
}

/// Le o cache da economia (pro "Onde obter" montar fora deste modulo, que
/// tambem entra nos binarios de auditoria sem o `world`).
pub fn com_config<R>(f: impl FnOnce(&EconomyConfig) -> R) -> R {
    f(&cell().read())
}

#[cfg(test)]
mod testes_onde_obter {
    use super::*;
    use shared::item_id::*;
    use shared::protocol::FonteDeItem;

    fn item(id: u16, vinc: bool) -> ItemDef {
        ItemDef {
            id,
            name: format!("i{id}"),
            sell_price: 1,
            buy_price: Some(10),
            shop_order: None,
            stack_max: 99,
            equip_slot: None,
            item_level: 1,
            icon_col: 0,
            icon_row: 0,
            icon_path: None,
            active: true,
            vinculado: vinc,
            template: Default::default(),
        }
    }

    fn cfg() -> EconomyConfig {
        let mut c = EconomyConfig::default();
        for id in [
            COPPER,
            na_cor(STEEL, 1),
            na_cor(STEEL, 2),
            HEALTH_POTION,
            WOOD_T1,
            na_cor(SCALE, 1),
        ] {
            c.items.insert(id, item(id, false));
        }
        c.enemy_kinds.insert(
            0,
            EnemyKindDef {
                kind: 0,
                name: "Lobo".into(),
                ..Default::default()
            },
        );
        c.loot_tables.insert(
            0,
            vec![
                LootEntry {
                    item_id: COPPER,
                    qty_min: 4,
                    qty_max: 14,
                    chance: 1.0,
                },
                LootEntry {
                    item_id: HEALTH_POTION,
                    qty_min: 1,
                    qty_max: 1,
                    chance: 0.08,
                },
            ],
        );
        c.farm_loot_tables.insert(
            ("Tree".into(), 1),
            vec![LootEntry {
                item_id: WOOD_T1,
                qty_min: 3,
                qty_max: 5,
                chance: 1.0,
            }],
        );
        c.farm_loot_tables.insert(
            ("Rock".into(), 1),
            vec![LootEntry {
                item_id: na_cor(STEEL, 1),
                qty_min: 3,
                qty_max: 6,
                chance: 0.5,
            }],
        );
        c.farm_loot_tables.insert(
            ("Rock".into(), 2),
            vec![LootEntry {
                item_id: na_cor(STEEL, 2),
                qty_min: 3,
                qty_max: 6,
                chance: 0.5,
            }],
        );
        c.vendor_shops.insert(
            3,
            VendorShop {
                shop_id: 3,
                name: "Alquimista".into(),
                items: vec![HEALTH_POTION],
            },
        );
        c
    }

    fn de(v: &[shared::protocol::ItemResourceSources], id: u16) -> Vec<FonteDeItem> {
        v.iter()
            .find(|e| e.item_id == id)
            .map(|e| e.sources.clone())
            .unwrap_or_default()
    }

    fn outras<'a>(chefes: Vec<(u16, String, u16, u8, Vec<(u16, f32)>)>) -> OutrasFontes<'a> {
        let ilhas_do_bicho = HashMap::from([(0u16, vec![0u8, 1])]);
        OutrasFontes {
            mobs: &[0],
            ilhas_do_bicho,
            chefes,
            lojas_da_vila: &[3],
            receitas: &[],
            missoes: &[],
        }
    }

    #[test]
    fn junta_mob_coleta_vendedor_e_chefe() {
        let c = cfg();
        let f = fontes_de_itens(
            &c,
            &outras(vec![(
                10,
                "Lobo Alfa".into(),
                8,
                0,
                vec![(na_cor(SCALE, 1), 0.0125)],
            )]),
        );
        let pocao = de(&f, HEALTH_POTION);
        assert!(pocao
            .iter()
            .any(|x| matches!(x, FonteDeItem::Mob { kind: 0, ilhas, .. } if *ilhas == vec![0, 1])));
        assert!(pocao.iter().any(|x| matches!(
            x,
            FonteDeItem::Vendedor {
                loja: 3,
                preco: 10,
                ..
            }
        )));
        assert_eq!(
            de(&f, WOOD_T1),
            vec![FonteDeItem::Coleta {
                tipo: 0,
                chance: 1.0,
                qty_min: 3,
                qty_max: 5
            }]
        );
        // Aco cinza: toda pedra da' cinza (cinza 100%, verde 80%...).
        let aco = de(&f, na_cor(STEEL, 1));
        let tipos: Vec<u8> = aco
            .iter()
            .filter_map(|x| match x {
                FonteDeItem::Coleta { tipo, .. } => Some(*tipo),
                _ => None,
            })
            .collect();
        assert_eq!(tipos, vec![1, 2, 3, 4]);
        // Aco verde: nao sai da pedra cinza.
        let verde = de(&f, na_cor(STEEL, 2));
        assert!(verde
            .iter()
            .all(|x| !matches!(x, FonteDeItem::Coleta { tipo: 1, .. })));
        assert!(verde.iter().any(|x| matches!(x, FonteDeItem::Coleta { tipo: 2, chance, .. } if (*chance - 0.1).abs() < 1e-4)));
    }

    #[test]
    fn chave_so_de_chefe_e_dungeon() {
        let c = cfg();
        let f = fontes_de_itens(
            &c,
            &outras(vec![(
                10,
                "Lobo Alfa".into(),
                8,
                2,
                vec![(na_cor(SCALE, 1), 0.0125)],
            )]),
        );
        let escama = de(&f, na_cor(SCALE, 1));
        assert!(escama.iter().any(|x| matches!(
            x,
            FonteDeItem::ChefeDoMundo {
                kind: 10,
                ilha: 2,
                ..
            }
        )));
        assert!(escama.contains(&FonteDeItem::DungeonRaid));
        assert!(escama
            .iter()
            .all(|x| !matches!(x, FonteDeItem::Mob { .. } | FonteDeItem::Coleta { .. })));
    }

    #[test]
    fn loja_fora_da_vila_e_item_inativo_ficam_de_fora() {
        let mut c = cfg();
        c.vendor_shops.insert(
            1,
            VendorShop {
                shop_id: 1,
                name: "Legado".into(),
                items: vec![na_cor(STEEL, 1)],
            },
        );
        c.items.get_mut(&WOOD_T1).unwrap().active = false;
        let f = fontes_de_itens(&c, &outras(Vec::new()));
        assert!(de(&f, na_cor(STEEL, 1))
            .iter()
            .all(|x| !matches!(x, FonteDeItem::Vendedor { .. })));
        assert!(de(&f, WOOD_T1).is_empty());
    }
}

/// Template de stat ranges do item (carregado do DB). Retorna default
/// (todos 0) se item não existir — `roll_for` checa has_any_range e devolve
/// None nesse caso (item não-equipável).
pub fn item_template_of(id: u16) -> ItemTemplate {
    cell()
        .read()
        .items
        .get(&id)
        .map(|i| i.template)
        .unwrap_or_default()
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
    let mut out: Vec<_> = cfg
        .items
        .values()
        .map(|i| shared::protocol::ItemConfigEntry {
            id: i.id,
            name: i.name.clone(),
            icon_path: i.icon_path.clone(),
            icon_col: i.icon_col,
            icon_row: i.icon_row,
            equip_slot: i.equip_slot.clone(),
            active: i.active,
            vinculado: i.vinculado,
        })
        .collect();
    out.sort_by_key(|e| e.id);
    out
}

/// Item vinculado (fora do mercado). Desconhecido conta como vinculado: o
/// mercado nunca vende o que o jogo nao conhece.
pub fn item_vinculado(id: u16) -> bool {
    cell().read().items.get(&id).is_none_or(|i| i.vinculado)
}

/// Nome do item (vazio se desconhecido).
pub fn item_nome(id: u16) -> String {
    cell()
        .read()
        .items
        .get(&id)
        .map(|i| i.name.clone())
        .unwrap_or_default()
}

/// Equipavel (tem slot de equipamento)?
pub fn item_equipavel(id: u16) -> bool {
    shared::equip_slot_of(id).is_some()
        || cell()
            .read()
            .items
            .get(&id)
            .is_some_and(|i| i.equip_slot.as_deref().is_some_and(|s| !s.is_empty()))
}

/// True se o item está ativo (default true). Usado pra bloquear equip/use
/// no servidor; client pode também consultar via `ItemsConfig.active`.
pub fn is_item_active(id: u16) -> bool {
    cell()
        .read()
        .items
        .get(&id)
        .map(|i| i.active)
        .unwrap_or(true)
}

/// Item level pra rolagem de drop por kind. Usa override do enemy_kinds
/// se setado, senão volta pro item_level base do próprio item.
pub fn loot_item_level(kind: u16, item_id: u16) -> u16 {
    let cfg = cell().read();
    if let Some(k) = cfg.enemy_kinds.get(&kind) {
        if let Some(lvl) = k.loot_item_level {
            return lvl;
        }
    }
    cfg.items.get(&item_id).map(|i| i.item_level).unwrap_or(1)
}

// ── DB load ──────────────────────────────────────────────────────────────────

pub(crate) async fn load_from_db(pool: &PgPool) -> Result<EconomyConfig> {
    let version: i64 = sqlx::query_scalar("SELECT version FROM economy_version WHERE id = 1")
        .fetch_one(pool)
        .await?;

    #[derive(sqlx::FromRow)]
    struct ItemRow {
        id: i32,
        name: String,
        sell_price: i32,
        buy_price: Option<i32>,
        shop_order: Option<i32>,
        stack_max: i32,
        equip_slot: Option<String>,
        item_level: i32,
        icon_col: i32,
        icon_row: i32,
        icon_path: Option<String>,
        active: bool,
        vinculado: bool,
        hp_min: i32,
        hp_max: i32,
        mp_min: i32,
        mp_max: i32,
        atk_min: i32,
        atk_max: i32,
        def_min: i32,
        def_max: i32,
        dex_min: i32,
        dex_max: i32,
        wis_min: i32,
        wis_max: i32,
    }
    let item_rows: Vec<ItemRow> = sqlx::query_as(
        "SELECT id, name, sell_price, buy_price, shop_order, stack_max, \
                equip_slot, item_level, icon_col, icon_row, icon_path, active, vinculado, \
                hp_min, hp_max, mp_min, mp_max, atk_min, atk_max, \
                def_min, def_max, dex_min, dex_max, wis_min, wis_max \
         FROM items",
    )
    .fetch_all(pool)
    .await?;
    let mut items = HashMap::with_capacity(item_rows.len());
    let mut shop_collected: Vec<(i32, u16)> = Vec::new(); // (order, id)
    for r in item_rows {
        let id_u16 = r.id as u16;
        if let Some(ord) = r.shop_order {
            shop_collected.push((ord, id_u16));
        }
        items.insert(
            id_u16,
            ItemDef {
                id: id_u16,
                name: r.name,
                sell_price: r.sell_price.max(0) as u32,
                buy_price: r.buy_price.map(|v| v.max(0) as u32),
                shop_order: r.shop_order,
                stack_max: r.stack_max.max(1) as u32,
                equip_slot: r.equip_slot,
                item_level: r.item_level.max(1) as u16,
                icon_col: r.icon_col,
                icon_row: r.icon_row,
                icon_path: r.icon_path,
                active: r.active,
                vinculado: r.vinculado,
                template: ItemTemplate {
                    hp_max: StatRange::new(r.hp_min, r.hp_max),
                    mp_max: StatRange::new(r.mp_min, r.mp_max),
                    attack_damage: StatRange::new(r.atk_min, r.atk_max),
                    defense: StatRange::new(r.def_min, r.def_max),
                    dex: StatRange::new(r.dex_min, r.dex_max),
                    wis: StatRange::new(r.wis_min, r.wis_max),
                },
            },
        );
    }
    shop_collected.sort_by_key(|(o, _)| *o);
    let shop_items: Vec<u16> = shop_collected.into_iter().map(|(_, id)| id).collect();

    #[derive(sqlx::FromRow)]
    struct EnemyRow {
        kind: i32,
        name: String,
        hp_max: i32,
        speed: f32,
        attack_damage: i32,
        attack_cooldown: f32,
        detect_range: f32,
        attack_range: f32,
        kite_dist: Option<f32>,
        proj_count: i32,
        xp_reward: i64,
        defense: i32,
        size_scale: f32,
        tint_r: f32,
        tint_g: f32,
        tint_b: f32,
        tint_a: f32,
        loot_item_level: Option<i32>,
    }
    let enemy_rows: Vec<EnemyRow> = sqlx::query_as(
        "SELECT kind, name, hp_max, speed, attack_damage, attack_cooldown, detect_range, \
                attack_range, kite_dist, proj_count, xp_reward, defense, size_scale, \
                tint_r, tint_g, tint_b, tint_a, loot_item_level FROM enemy_kinds",
    )
    .fetch_all(pool)
    .await?;
    let mut enemy_kinds = HashMap::with_capacity(enemy_rows.len());
    for r in enemy_rows {
        enemy_kinds.insert(
            r.kind as u16,
            EnemyKindDef {
                kind: r.kind as u16,
                name: r.name,
                hp_max: r.hp_max,
                speed: r.speed,
                attack_damage: r.attack_damage,
                attack_cooldown: r.attack_cooldown,
                detect_range: r.detect_range,
                attack_range: r.attack_range,
                kite_dist: r.kite_dist,
                proj_count: r.proj_count.max(1) as u32,
                xp_reward: r.xp_reward.max(0) as u64,
                defense: r.defense,
                size_scale: r.size_scale,
                tint_rgba: [r.tint_r, r.tint_g, r.tint_b, r.tint_a],
                loot_item_level: r.loot_item_level.map(|v| v.max(1) as u16),
            },
        );
    }

    let loot_rows: Vec<(i32, i32, i32, i32, f32)> = sqlx::query_as(
        "SELECT enemy_kind, item_id, qty_min, qty_max, chance FROM loot_drops ORDER BY id",
    )
    .fetch_all(pool)
    .await?;
    let mut loot_tables: HashMap<u16, Vec<LootEntry>> = HashMap::new();
    for (kind, item_id, qmin, qmax, chance) in loot_rows {
        loot_tables.entry(kind as u16).or_default().push(LootEntry {
            item_id: item_id as u16,
            qty_min: qmin.max(0) as u32,
            qty_max: qmax.max(qmin) as u32,
            chance: chance.clamp(0.0, 1.0),
        });
    }

    let farm_rows: Vec<(String, i32, i32, i32, i32, f32)> = sqlx::query_as(
        "SELECT kind, tier, item_id, qty_min, qty_max, chance \
         FROM farm_node_drops ORDER BY kind, tier, id",
    )
    .fetch_all(pool)
    .await?;
    let mut farm_loot_tables: HashMap<(String, u8), Vec<LootEntry>> = HashMap::new();
    for (kind, tier, item_id, qmin, qmax, chance) in farm_rows {
        let t = tier.max(1).min(4) as u8;
        farm_loot_tables
            .entry((kind, t))
            .or_default()
            .push(LootEntry {
                item_id: item_id as u16,
                qty_min: qmin.max(0) as u32,
                qty_max: qmax.max(qmin) as u32,
                chance: chance.clamp(0.0, 1.0),
            });
    }

    let shop_rows: Vec<(i32, String)> = sqlx::query_as("SELECT shop_id, name FROM vendor_shops")
        .fetch_all(pool)
        .await?;
    let mut vendor_shops = HashMap::with_capacity(shop_rows.len());
    for (sid, name) in shop_rows {
        vendor_shops.insert(
            sid as u32,
            VendorShop {
                shop_id: sid as u32,
                name,
                items: Vec::new(),
            },
        );
    }
    let item_rows: Vec<(i32, i32)> = sqlx::query_as(
        "SELECT shop_id, item_id FROM vendor_shop_items ORDER BY shop_id, sort_order",
    )
    .fetch_all(pool)
    .await?;
    for (sid, item_id) in item_rows {
        if let Some(shop) = vendor_shops.get_mut(&(sid as u32)) {
            shop.items.push(item_id as u16);
        }
    }

    Ok(EconomyConfig {
        version,
        items,
        shop_items,
        enemy_kinds,
        loot_tables,
        farm_loot_tables,
        vendor_shops,
    })
}

/// O kind do CHEFE na tabela.
///
/// Um numero e nao um `build_boss`: o que faz o chefe ser chefe sao os numeros
/// dele (700 de vida contra 50 do Grunt), e nao uma classe procedural.
pub const KIND_CHEFE: u16 = 7;

/// Bichos de PRAIA: caranguejo e caranguejo-rei. Nascem so' nas zonas de
/// praia (`world::ZONA_DE_PRAIA_ID`), com sorteio proprio; a escada por nivel
/// das zonas comuns (`kind_para_nivel`, `kinds_comuns`) nunca tira eles.
pub const KINDS_DE_PRAIA: [u16; 2] = [8, 9];
/// Um em quantos caranguejos e' rei.
pub const UM_REI_EM: u64 = 4;

/// O bicho de uma vaga de praia.
pub fn kind_de_praia(semente: u64) -> u16 {
    if semente % UM_REI_EM == 0 {
        KINDS_DE_PRAIA[1]
    } else {
        KINDS_DE_PRAIA[0]
    }
}

/// (kind, chance em %) de uma zona de praia, pro mapa — a mesma conta de
/// `kind_de_praia`.
pub fn bichos_de_praia() -> Vec<(u16, u8)> {
    let rei = (100 / UM_REI_EM) as u8;
    vec![(KINDS_DE_PRAIA[0], 100 - rei), (KINDS_DE_PRAIA[1], rei)]
}

#[cfg(test)]
mod loot_tests {
    use super::*;
    fn item(id: u16, slot: Option<&str>, active: bool) -> ItemDef {
        ItemDef {
            id,
            name: format!("Item {id}"),
            sell_price: 1,
            buy_price: None,
            shop_order: None,
            stack_max: 999,
            equip_slot: slot.map(str::to_owned),
            item_level: 1,
            icon_col: 0,
            icon_row: 0,
            icon_path: None,
            active,
            vinculado: false,
            template: Default::default(),
        }
    }
    #[test]
    fn tabela_legada_nao_volta_a_dropar_equipamento_nem_item_desconhecido() {
        use shared::item_id::*;
        let mut cfg = EconomyConfig::default();
        for i in [
            item(COPPER, None, true),
            item(KATANA, None, true),
            item(999, Some("Weapon"), true),
            item(HEALTH_POTION, None, true),
            item(STEEL, None, false),
        ] {
            cfg.items.insert(i.id, i);
        }
        let table: Vec<_> = [COPPER, KATANA, 999, HEALTH_POTION, STEEL, 998]
            .into_iter()
            .map(|id| LootEntry {
                item_id: id,
                qty_min: 1,
                qty_max: 1,
                chance: 1.0,
            })
            .collect();
        cfg.loot_tables.insert(0, table.clone());
        for seed in 0..1000 {
            assert_eq!(
                cfg.roll_loot(0, seed),
                vec![(COPPER, 1), (HEALTH_POTION, 1)]
            );
            assert_eq!(
                cfg.roll_loot(5, seed),
                vec![(COPPER, 1), (HEALTH_POTION, 1)]
            ); // fallback tem a mesma regra
        }
        cfg.farm_loot_tables.insert(("Rock".into(), 1), table);
        assert!(cfg.roll_farm_loot("Rock", 1, 10).contains(&(KATANA, 1))); // filtro restrito ao loot de mobs
    }
}

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
        .filter(|&k| k != KIND_CHEFE && !KINDS_DE_PRAIA.contains(&k))
        .collect();
    // Ordem estavel: a tabela vem de um mapa, e sorteio sobre ordem de hash
    // daria um bicho diferente a cada reinicio do servidor.
    comuns.sort_unstable();
    kind_para_nivel_em(&comuns, nivel, semente)
}

/// O sorteio de `kind_para_nivel` sobre uma lista ja' ordenada — separado pra
/// o simulador de balanceamento sortear igual ao jogo sem o banco.
pub(crate) fn kind_para_nivel_em(comuns: &[u16], nivel: u32, semente: u64) -> u16 {
    if comuns.is_empty() {
        return 0;
    }
    // Ate' onde a escolha vai: um bicho novo a cada tres niveis.
    let teto = ((nivel as usize / 3) + 1).min(comuns.len());
    comuns[(semente as usize) % teto]
}

/// Uma linha da tabela de mobs semeada no banco (`persistence`). Mora aqui
/// pra o simulador de balanceamento ler os MESMOS numeros.
pub(crate) struct KindInicial {
    pub kind: i32,
    pub name: &'static str,
    pub hp: i32,
    pub sp: f32,
    pub dmg: i32,
    pub cd: f32,
    pub det: f32,
    pub rng: f32,
    pub kite: Option<f32>,
    pub proj: i32,
    pub xp: i64,
    pub def: i32,
    pub sz: f32,
    pub t: [f32; 4],
}

/// Os oito mobs do jogo. A regra: quem MORDE e' bicho, quem ATIRA e' gente.
/// Os numeros de antes ficaram (sao o que o balanceamento ja' conhece); mudou
/// quem eles sao — e o chefe, que agora e' um lobo grande e por isso MORDE em
/// vez de atirar cinco projeteis.
pub(crate) const KINDS_INICIAIS: [KindInicial; 10] = [
    KindInicial {
        kind: 0,
        name: "Lobo",
        hp: 120,
        sp: 2.0,
        dmg: 10,
        cd: 2.0,
        det: 9.0,
        rng: 1.8,
        kite: None,
        proj: 1,
        xp: 30,
        def: 0,
        sz: 1.0,
        t: [1.0, 1.0, 1.0, 1.0],
    },
    KindInicial {
        kind: 1,
        name: "Urso",
        hp: 280,
        sp: 1.3,
        dmg: 18,
        cd: 2.8,
        det: 8.0,
        rng: 1.8,
        kite: None,
        proj: 1,
        xp: 75,
        def: 8,
        sz: 1.3,
        t: [1.0, 1.0, 1.0, 1.0],
    },
    KindInicial {
        kind: 2,
        name: "Pistoleiro",
        hp: 85,
        sp: 2.4,
        dmg: 12,
        cd: 1.5,
        det: 13.0,
        rng: 9.0,
        kite: Some(5.0),
        proj: 1,
        xp: 50,
        def: 0,
        sz: 1.0,
        t: [1.0, 1.0, 1.0, 1.0],
    },
    KindInicial {
        kind: 3,
        name: "Tigre",
        hp: 95,
        sp: 4.2,
        dmg: 15,
        cd: 1.0,
        det: 11.0,
        rng: 1.8,
        kite: None,
        proj: 1,
        xp: 55,
        def: 2,
        sz: 1.0,
        t: [1.0, 1.0, 1.0, 1.0],
    },
    KindInicial {
        kind: 4,
        name: "Mago",
        hp: 105,
        sp: 1.4,
        dmg: 22,
        cd: 2.2,
        det: 15.0,
        rng: 12.0,
        kite: Some(8.0),
        proj: 1,
        xp: 70,
        def: 1,
        sz: 1.0,
        t: [1.0, 1.0, 1.0, 1.0],
    },
    KindInicial {
        kind: 5,
        name: "Owlbear",
        hp: 460,
        sp: 1.5,
        dmg: 28,
        cd: 3.0,
        det: 8.0,
        rng: 1.8,
        kite: None,
        proj: 1,
        xp: 110,
        def: 4,
        sz: 1.5,
        t: [1.0, 1.0, 1.0, 1.0],
    },
    KindInicial {
        kind: 6,
        name: "Arqueiro",
        hp: 105,
        sp: 2.8,
        dmg: 14,
        cd: 1.6,
        det: 13.0,
        rng: 9.0,
        kite: Some(7.0),
        proj: 1,
        xp: 60,
        def: 1,
        sz: 1.0,
        t: [1.0, 1.0, 1.0, 1.0],
    },
    KindInicial {
        kind: 7,
        name: "Lobo Grande",
        hp: 700,
        sp: 1.6,
        dmg: 40,
        cd: 2.8,
        det: 18.0,
        rng: 2.6,
        kite: None,
        proj: 1,
        xp: 600,
        def: 20,
        sz: 2.2,
        t: [1.0, 1.0, 1.0, 1.0],
    },
    // Os de PRAIA (`KINDS_DE_PRAIA`): so' nascem em zona de praia. O
    // caranguejo e' mais fraco e mais lento que o lobo; o rei fica entre o
    // lobo e o urso.
    KindInicial {
        kind: 8,
        name: "Caranguejo",
        hp: 90,
        sp: 1.8,
        dmg: 8,
        cd: 1.8,
        det: 7.0,
        rng: 1.6,
        kite: None,
        proj: 1,
        xp: 25,
        def: 3,
        sz: 0.7,
        t: [1.0, 1.0, 1.0, 1.0],
    },
    KindInicial {
        kind: 9,
        name: "Caranguejo-rei",
        hp: 220,
        sp: 1.5,
        dmg: 15,
        cd: 2.4,
        det: 8.0,
        rng: 1.9,
        kite: None,
        proj: 1,
        xp: 60,
        def: 10,
        sz: 1.0,
        t: [1.0, 1.0, 1.0, 1.0],
    },
];

#[cfg(test)]
mod testes_de_praia {
    use super::*;

    #[test]
    fn caranguejo_e_fraco_e_o_rei_fica_entre_lobo_e_urso() {
        let (lobo, urso) = (&KINDS_INICIAIS[0], &KINDS_INICIAIS[1]);
        let (c, rei) = (&KINDS_INICIAIS[8], &KINDS_INICIAIS[9]);
        assert_eq!((c.kind, rei.kind), (8, 9));
        assert!(c.hp < lobo.hp && c.dmg < lobo.dmg && c.sp < lobo.sp);
        assert!(rei.hp > lobo.hp && rei.hp < urso.hp && rei.dmg > lobo.dmg && rei.dmg < urso.dmg);
        assert!(
            c.kite.is_none() && rei.kite.is_none(),
            "caranguejo belisca, nao atira"
        );
    }

    #[test]
    fn praia_sorteia_um_rei_em_quatro_e_nunca_bicho_comum() {
        let reis = (0..4000u64).filter(|s| kind_de_praia(*s) == 9).count();
        assert_eq!(reis, 1000);
        assert!((0..4000u64).all(|s| KINDS_DE_PRAIA.contains(&kind_de_praia(s))));
        assert_eq!(bichos_de_praia(), vec![(8, 75), (9, 25)]);
    }
}

#[cfg(test)]
mod testes_da_pedra {
    use super::*;
    use shared::item_id::*;

    fn config_da_pedra() -> EconomyConfig {
        let mut c = EconomyConfig::default();
        for (t, id, mn, mx, chance) in linhas_da_pedra() {
            c.farm_loot_tables
                .entry(("Rock".to_string(), t))
                .or_default()
                .push(LootEntry {
                    item_id: id,
                    qty_min: mn as u32,
                    qty_max: mx as u32,
                    chance,
                });
        }
        c
    }

    /// A pedra rende o que o planejamento diz, pelo MESMO caminho da coleta:
    /// sorteia a cor com `tier_do_rendimento` e rola a linha daquela cor.
    /// docs/ECONOMIA_DE_CRAFT.md (chances) e docs/COLETA.md (cor por pedra).
    #[test]
    fn cada_pedra_rende_os_materiais_do_planejamento() {
        let c = config_da_pedra();
        const N: u32 = 60_000;
        let taxa = |x: u32| x as f32 / N as f32;
        for pedra in 1..=4u8 {
            let mut vezes: HashMap<u16, u32> = HashMap::new();
            let mut cobre = (u32::MAX, 0u32);
            let mut s = 0x5EED_0000u64 ^ pedra as u64;
            for _ in 0..N {
                s = lcg(s);
                let cor = shared::tier_do_rendimento(pedra, lcg_f32(lcg(s ^ 0x5EED_C0DE)));
                for (id, q) in c.roll_farm_loot("Rock", cor, s) {
                    *vezes.entry(id).or_default() += 1;
                    if id == COPPER {
                        cobre = (cobre.0.min(q), cobre.1.max(q));
                    }
                }
            }
            let soma = |base: u16| {
                (1..=4)
                    .map(|k| vezes.get(&na_cor(base, k)).copied().unwrap_or(0))
                    .sum::<u32>()
            };
            assert_eq!(
                vezes.get(&COPPER).copied(),
                Some(N),
                "pedra {pedra}: cobre tem que cair sempre"
            );
            assert!(
                cobre.0 >= 40 && cobre.1 <= 120,
                "pedra {pedra}: cobre {cobre:?} fora de 40-120"
            );
            for (nome, base, esperado, tol) in [
                ("Aço", STEEL, 0.55, 0.015),
                ("Platina", PLATINUM, 0.30, 0.015),
                ("Coração Negro", DARK_HEART_STONE, 0.12, 0.01),
                ("Ânima", ANIMA_STONE, 0.12, 0.01),
            ] {
                let t = taxa(soma(base));
                assert!(
                    (t - esperado).abs() < tol,
                    "pedra {pedra}: {nome} a {t:.3}, doc diz {esperado}"
                );
            }
            assert!((taxa(vezes.get(&DARKSTEEL).copied().unwrap_or(0)) - 0.35).abs() < 0.015);
            assert!(
                (taxa(vezes.get(&GLITTERING_POWDER).copied().unwrap_or(0)) - 0.03).abs() < 0.005
            );
            // A COR do material segue a escada da pedra, e roxo nao cai.
            let aco = soma(STEEL);
            for cor in 1..=4u8 {
                let obtido =
                    vezes.get(&na_cor(STEEL, cor)).copied().unwrap_or(0) as f32 / aco as f32;
                let esperado =
                    shared::RENDIMENTO_DA_PEDRA[pedra as usize][cor as usize - 1] as f32 / 100.0;
                assert!(
                    (obtido - esperado).abs() < 0.02,
                    "pedra {pedra}: aço cor {cor} a {obtido:.3}, escada diz {esperado}"
                );
            }
            for base in MATERIAIS_COLORIDOS {
                assert!(
                    vezes.get(&na_cor(base, 4)).is_none(),
                    "pedra {pedra}: material roxo caiu ({base})"
                );
            }
            for chave in todas_as_chaves() {
                assert!(
                    vezes.get(&chave).is_none(),
                    "pedra {pedra}: chave {chave} caiu (so' chefe da)"
                );
            }
            println!(
                "pedra {pedra}: aço {:.1}% (cinza {:.0}/verde {:.0}/azul {:.0}), platina {:.1}%, darksteel {:.1}%, pó {:.2}%",
                taxa(aco) * 100.0,
                vezes.get(&na_cor(STEEL, 1)).copied().unwrap_or(0) as f32 / aco as f32 * 100.0,
                vezes.get(&na_cor(STEEL, 2)).copied().unwrap_or(0) as f32 / aco as f32 * 100.0,
                vezes.get(&na_cor(STEEL, 3)).copied().unwrap_or(0) as f32 / aco as f32 * 100.0,
                taxa(soma(PLATINUM)) * 100.0,
                taxa(vezes.get(&DARKSTEEL).copied().unwrap_or(0)) * 100.0,
                taxa(vezes.get(&GLITTERING_POWDER).copied().unwrap_or(0)) * 100.0,
            );
        }
    }

    /// Oito materiais coloridos + tres sem cor, nas quatro linhas de tier.
    #[test]
    fn a_tabela_da_pedra_tem_as_quatro_cores() {
        let l = linhas_da_pedra();
        assert_eq!(l.len(), 4 * 11);
        for t in 1..=4u8 {
            assert!(l
                .iter()
                .any(|&(tt, id, ..)| tt == t && id == na_cor(STEEL, t)));
            assert_eq!(
                l.iter()
                    .filter(|&&(tt, id, ..)| tt == t && id == COPPER)
                    .count(),
                1
            );
        }
    }
}
