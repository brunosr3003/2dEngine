//! Configuração de skills carregada do Postgres.
//!
//! Análogo a `economy.rs`. Compartilha o `economy_version` pra hot-reload
//! (admin bumpa um único contador, tanto items/enemies quanto skills
//! recarregam atomicamente).
//!
//! Acesso global via `OnceCell<RwLock<SkillsConfig>>` — leitores pegam
//! snapshot consistente; troca atômica no reload.

use anyhow::Result;
use once_cell::sync::OnceCell;
use parking_lot::RwLock;
use shared::skills::SkillDef;
use sqlx::postgres::PgPool;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

#[derive(Default)]
pub struct SkillsConfig {
    pub version: i64,
    pub by_id:   HashMap<u32, SkillDef>,
    /// Index por arma RECOMENDADA (`Sword`/`Axe`/...) → skills com essa
    /// afinidade, ordenado por (tier asc, is_passive false-first, id).
    /// Skills nesse index ainda podem ser castadas por outras armas.
    pub by_prof: HashMap<String, Vec<u32>>,
    /// Index por categoria (`offensive`, `support`, `control`, `mobility`,
    /// `passive`) — usado pela UI nova.
    pub by_category: HashMap<String, Vec<u32>>,
}

impl SkillsConfig {
    pub fn skill(&self, id: u32) -> Option<&SkillDef> { self.by_id.get(&id) }

    pub fn skills_for_prof(&self, prof: &str) -> Vec<&SkillDef> {
        self.by_prof.get(prof)
            .map(|ids| ids.iter().filter_map(|i| self.by_id.get(i)).collect())
            .unwrap_or_default()
    }

    pub fn skills_for_category(&self, cat: &str) -> Vec<&SkillDef> {
        self.by_category.get(cat)
            .map(|ids| ids.iter().filter_map(|i| self.by_id.get(i)).collect())
            .unwrap_or_default()
    }

    /// Lista TODAS as skills (snapshot pra `SkillsConfig` wire), ordenadas
    /// estavelmente por id pra delta-encoding eventual.
    pub fn all_sorted(&self) -> Vec<SkillDef> {
        let mut v: Vec<_> = self.by_id.values().cloned().collect();
        v.sort_by_key(|s| s.id.0);
        v
    }
}

static SKILLS: OnceCell<Arc<RwLock<SkillsConfig>>> = OnceCell::new();

fn cell() -> &'static RwLock<SkillsConfig> {
    SKILLS.get().expect("skills não inicializada — chame skills::init() na boot")
}

pub async fn init(pool: &PgPool) -> Result<()> {
    let cfg = load_from_db(pool).await?;
    let _ = SKILLS.set(Arc::new(RwLock::new(cfg)));
    Ok(())
}

/// Hot-reload: checa `economy_version` a cada 5s (mesmo contador que items/enemies).
/// Mantém em sync com o cache em `economy.rs`.
pub fn spawn_hot_reload(pool: PgPool) {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(5));
        interval.tick().await;
        loop {
            interval.tick().await;
            match check_and_reload(&pool).await {
                Ok(true) => {
                    let v = cell().read().version;
                    tracing::info!("skills hot-reload: v{v}");
                }
                Ok(false) => {}
                Err(e) => tracing::warn!("skills reload falhou: {e}"),
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

// ── Lookup helpers ──────────────────────────────────────────────────────────

pub fn skill_of(id: u32) -> Option<SkillDef> {
    cell().read().by_id.get(&id).cloned()
}

pub fn skills_for_prof(prof: &str) -> Vec<SkillDef> {
    let cfg = cell().read();
    cfg.skills_for_prof(prof).into_iter().cloned().collect()
}

pub fn all_skills() -> Vec<SkillDef> { cell().read().all_sorted() }

pub fn current_version() -> i64 { cell().read().version }

/// Pode aprender a skill? Checa unlock_char_lvl + unlock_prof_lvl.
/// `prof_levels` indexado por `Proficiency as usize` (size = PROF_COUNT).
pub fn can_unlock(skill: &SkillDef, char_level: u32, prof_levels: &[u64; shared::PROF_COUNT]) -> bool {
    if char_level < skill.unlock_char_lvl { return false; }
    let Some(p) = shared::Proficiency::from_str(&skill.prof) else { return false };
    let prof_xp = prof_levels[p as usize];
    let prof_lvl = shared::proficiency_level(prof_xp);
    prof_lvl >= skill.unlock_prof_lvl
}

// ── DB load ─────────────────────────────────────────────────────────────────

async fn load_from_db(pool: &PgPool) -> Result<SkillsConfig> {
    let version: i64 = sqlx::query_scalar("SELECT version FROM economy_version WHERE id = 1")
        .fetch_one(pool).await?;

    #[derive(sqlx::FromRow)]
    struct SkillRow {
        id: i32,
        name: String,
        description: String,
        prof: String,
        tier: i16,
        is_passive: bool,
        path: Option<String>,
        unlock_char_lvl: i16,
        unlock_prof_lvl: i16,
        usable_with: Option<Vec<String>>,
        cost_mp: i32,
        cost_stamina: i32,
        cooldown_s: f32,
        cast_time_s: f32,
        target_type: String,
        range_tiles: f32,
        radius_tiles: f32,
        base_damage: i32,
        base_heal: i32,
        scaling_atk: f32,
        scaling_wis: f32,
        scaling_dex: f32,
        per_rank_dmg_pct: f32,
        per_rank_cd_pct: f32,
        per_rank_cost_pct: f32,
        icon_path: Option<String>,
        vfx_id: Option<String>,
        active: bool,
        knockback: f32,
        max_rank_damage_pct: f32,
        max_rank_heal_pct: f32,
        max_rank_radius_bonus: f32,
        max_rank_range_bonus: f32,
        max_rank_cooldown_red_pct: f32,
        max_rank_crit_chance: f32,
        category: String,
        affinity_damage_pct: f32,
        affinity_cooldown_red_pct: f32,
        affinity_crit_pct: f32,
        affinity_cost_red_pct: f32,
    }

    let rows: Vec<SkillRow> = sqlx::query_as(
        "SELECT id, name, description, prof, tier, is_passive, path,
                unlock_char_lvl, unlock_prof_lvl, usable_with,
                cost_mp, cost_stamina, cooldown_s, cast_time_s,
                target_type, range_tiles, radius_tiles,
                base_damage, base_heal, scaling_atk, scaling_wis, scaling_dex,
                per_rank_dmg_pct, per_rank_cd_pct, per_rank_cost_pct,
                icon_path, vfx_id, active, knockback,
                max_rank_damage_pct, max_rank_heal_pct, max_rank_radius_bonus,
                max_rank_range_bonus, max_rank_cooldown_red_pct, max_rank_crit_chance,
                category, affinity_damage_pct, affinity_cooldown_red_pct,
                affinity_crit_pct, affinity_cost_red_pct
         FROM skills WHERE active = TRUE"
    ).fetch_all(pool).await?;

    let mut by_id = HashMap::with_capacity(rows.len());
    let mut by_prof: HashMap<String, Vec<u32>> = HashMap::new();
    let mut by_category: HashMap<String, Vec<u32>> = HashMap::new();
    for r in rows {
        let id_u32 = r.id.max(0) as u32;
        let def = SkillDef {
            id: shared::SkillId(id_u32),
            name: r.name,
            description: r.description,
            prof: r.prof.clone(),
            category: r.category.clone(),
            tier: r.tier.clamp(1, 4) as u8,
            is_passive: r.is_passive,
            path: r.path,
            unlock_char_lvl: r.unlock_char_lvl.max(1) as u32,
            unlock_prof_lvl: r.unlock_prof_lvl.max(1) as u32,
            usable_with: r.usable_with,
            cost_mp: r.cost_mp,
            cost_stamina: r.cost_stamina,
            cooldown_s: r.cooldown_s,
            cast_time_s: r.cast_time_s,
            target_type: r.target_type,
            range_tiles: r.range_tiles,
            radius_tiles: r.radius_tiles,
            base_damage: r.base_damage,
            base_heal: r.base_heal,
            scaling_atk: r.scaling_atk,
            scaling_wis: r.scaling_wis,
            scaling_dex: r.scaling_dex,
            per_rank_dmg_pct: r.per_rank_dmg_pct,
            per_rank_cd_pct: r.per_rank_cd_pct,
            per_rank_cost_pct: r.per_rank_cost_pct,
            icon_path: r.icon_path,
            vfx_id: r.vfx_id,
            knockback: r.knockback,
            max_rank_damage_pct: r.max_rank_damage_pct,
            max_rank_heal_pct: r.max_rank_heal_pct,
            max_rank_radius_bonus: r.max_rank_radius_bonus,
            max_rank_range_bonus: r.max_rank_range_bonus,
            max_rank_cooldown_red_pct: r.max_rank_cooldown_red_pct,
            max_rank_crit_chance: r.max_rank_crit_chance,
            affinity_damage_pct: r.affinity_damage_pct,
            affinity_cooldown_red_pct: r.affinity_cooldown_red_pct,
            affinity_crit_pct: r.affinity_crit_pct,
            affinity_cost_red_pct: r.affinity_cost_red_pct,
        };
        by_prof.entry(r.prof).or_default().push(id_u32);
        by_category.entry(r.category).or_default().push(id_u32);
        by_id.insert(id_u32, def);
    }
    // Sort por (tier asc, !is_passive primeiro, id). Ativas vêm antes de
    // passivas dentro do mesmo tier — bom pra UI.
    for ids in by_prof.values_mut() {
        ids.sort_by_key(|id| {
            let s = &by_id[id];
            (s.tier, s.is_passive as u8, s.id.0)
        });
    }
    for ids in by_category.values_mut() {
        ids.sort_by_key(|id| {
            let s = &by_id[id];
            (s.tier, s.is_passive as u8, s.id.0)
        });
    }

    Ok(SkillsConfig { version, by_id, by_prof, by_category })
}
