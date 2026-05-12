//! Tipos de skills compartilhados entre cliente e servidor.
//!
//! `SkillDef` é a definição estática (carregada do DB no server, replicada
//! ao cliente via `SkillsConfig`). `LearnedSkill` é o estado por player.
//!
//! O catálogo das 8 profs × 8 skills (= 64) vive na tabela `skills`. Cliente
//! consulta `SkillsConfigCache` pra mostrar nome/icon/descrição na UI.

use serde::{Deserialize, Serialize};

/// Identificador estável de uma skill (matches `skills.id` no DB).
/// Range escolhido (1000+) pra não conflitar com `item_id` (1..99).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SkillId(pub u32);

/// Categoria de target — dispatch do effect engine.
/// `kebab-case` na wire pra match com web admin / DB enum.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TargetType {
    None,         // passiva ou self-only sem alvo
    SelfTarget,   // buff em si mesmo
    Projectile,   // dispara projétil aim mouse
    Cone,         // cone na frente
    AoeCircle,    // circle no ground (AoE position-targeted)
    Line,         // linha do caster até range
}

impl TargetType {
    pub fn as_db_str(&self) -> &'static str {
        match self {
            Self::None => "none",
            Self::SelfTarget => "self",
            Self::Projectile => "projectile",
            Self::Cone => "cone",
            Self::AoeCircle => "aoe_circle",
            Self::Line => "line",
        }
    }
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "none"        => Some(Self::None),
            "self"        => Some(Self::SelfTarget),
            "projectile"  => Some(Self::Projectile),
            "cone"        => Some(Self::Cone),
            "aoe_circle"  => Some(Self::AoeCircle),
            "line"        => Some(Self::Line),
            _ => None,
        }
    }
}

/// Definição estática de uma skill (vinda do DB). Compatível com cliente —
/// só os campos que a UI/lógica precisam ver. Payloads `effect_payload` /
/// `rank5/rank10` ficam só no server (não enviados pro cliente nas Fase 1).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillDef {
    pub id: SkillId,
    pub name: String,
    pub description: String,
    /// String do enum Proficiency (`Sword`, `Axe`, `Wand`, ...) — match com
    /// `Proficiency::as_db_str`.
    pub prof: String,
    pub tier: u8,
    pub is_passive: bool,
    pub path: Option<String>,
    pub unlock_char_lvl: u32,
    pub unlock_prof_lvl: u32,
    /// Lista de profs com cujas armas a skill funciona quando equipada.
    /// `None` = ALL (qualquer arma). Usado pra cross-weapon (warrior cura).
    pub usable_with: Option<Vec<String>>,
    pub cost_mp: i32,
    pub cost_stamina: i32,
    pub cooldown_s: f32,
    pub cast_time_s: f32,
    pub target_type: String,   // wire-friendly string; converter via TargetType::from_str
    pub range_tiles: f32,
    pub radius_tiles: f32,
    pub base_damage: i32,
    pub base_heal: i32,
    pub scaling_atk: f32,
    pub scaling_wis: f32,
    pub scaling_dex: f32,
    pub per_rank_dmg_pct: f32,
    pub per_rank_cd_pct: f32,
    pub per_rank_cost_pct: f32,
    pub icon_path: Option<String>,
    pub vfx_id: Option<String>,
    /// Knockback em tiles aplicado ao alvo na direcao OPOSTA ao atacante
    /// (`-hurt_dir`). 0.0 = sem knockback. Direcional skills (line/aoe/cone)
    /// usam, target/none ignoram.
    #[serde(default)]
    pub knockback: f32,

    // ── Max-rank passive bonuses (aplicados quando rank == MAX_SKILL_RANK) ──
    /// +X% damage (30 = +30%). 0 = sem bônus.
    #[serde(default)] pub max_rank_damage_pct: f32,
    /// +X% heal.
    #[serde(default)] pub max_rank_heal_pct: f32,
    /// +N tiles de AoE radius.
    #[serde(default)] pub max_rank_radius_bonus: f32,
    /// +N tiles de range.
    #[serde(default)] pub max_rank_range_bonus: f32,
    /// -X% cooldown (25 = 25% off).
    #[serde(default)] pub max_rank_cooldown_red_pct: f32,
    /// +X% crit chance.
    #[serde(default)] pub max_rank_crit_chance: f32,
}

/// Defaults procedurais usados pelo SEED — popula colunas DB com bônus
/// derivados do tipo de skill (dano, heal, utility). Admin pode UPDATE pra
/// customizar depois sem mexer no código.
///   - Skill de dano  → +30% damage, +1 radius se AoE
///   - Skill de heal  → +30% heal, +1 radius se AoE
///   - Utility/buff  → -25% cooldown
///   - Long cd (≥10s) → +5% crit chance bonus (acumula)
pub fn default_max_rank_bonus_for_seed(
    base_damage: i32, base_heal: i32, radius_tiles: f32, cooldown_s: f32,
) -> (f32, f32, f32, f32, f32, f32) {
    // Retorna (damage_pct, heal_pct, radius_bonus, range_bonus, cd_red_pct, crit)
    let mut dmg = 0.0; let mut heal = 0.0; let mut rad = 0.0;
    let range = 0.0; let mut cd = 0.0; let mut crit = 0.0;
    if base_damage > 0 {
        dmg = 30.0;
        if radius_tiles > 0.0 { rad = 1.0; }
    } else if base_heal > 0 {
        heal = 30.0;
        if radius_tiles > 0.0 { rad = 1.0; }
    } else {
        cd = 25.0;
    }
    if cooldown_s >= 10.0 { crit = 5.0; }
    (dmg, heal, rad, range, cd, crit)
}

/// Strings descritivas do bônus de max rank pra mostrar no tooltip cliente.
pub fn max_rank_bonus_lines(skill: &SkillDef) -> Vec<String> {
    let mut out = Vec::new();
    if skill.max_rank_damage_pct > 0.0       { out.push(format!("+{:.0}% damage", skill.max_rank_damage_pct)); }
    if skill.max_rank_heal_pct > 0.0         { out.push(format!("+{:.0}% heal", skill.max_rank_heal_pct)); }
    if skill.max_rank_radius_bonus > 0.0     { out.push(format!("+{:.0} radius", skill.max_rank_radius_bonus)); }
    if skill.max_rank_range_bonus > 0.0      { out.push(format!("+{:.0} range", skill.max_rank_range_bonus)); }
    if skill.max_rank_cooldown_red_pct > 0.0 { out.push(format!("-{:.0}% cooldown", skill.max_rank_cooldown_red_pct)); }
    if skill.max_rank_crit_chance > 0.0      { out.push(format!("+{:.0}% crit", skill.max_rank_crit_chance)); }
    out
}

/// Estado por player de uma skill aprendida.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct LearnedSkill {
    pub skill_id: u32,
    /// 1..=10 (já aprendida; 0 não aparece nessa lista).
    pub rank: u8,
    /// Slot 0..=5 da skill bar; None se não equipada (ativa) ou se passiva
    /// (passivas ignoram slot).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub equipped_slot: Option<u8>,
}

/// Snapshot completo do estado de skills do player — enviado no login e
/// após qualquer mudança (learn, rank-up, equip).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlayerSkillsState {
    /// SP totais ganhos (cumulative ao longo dos levels).
    pub sp_earned: u32,
    /// SP já gastos.
    pub sp_spent: u32,
    pub skills: Vec<LearnedSkill>,
}

impl PlayerSkillsState {
    pub fn empty() -> Self {
        Self { sp_earned: 0, sp_spent: 0, skills: Vec::new() }
    }
    pub fn sp_available(&self) -> u32 { self.sp_earned.saturating_sub(self.sp_spent) }
    pub fn rank_of(&self, skill_id: u32) -> u8 {
        self.skills.iter().find(|s| s.skill_id == skill_id).map(|s| s.rank).unwrap_or(0)
    }
}
