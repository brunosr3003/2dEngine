//! PvP evaluation: two characters, one fight, measured before balance moves.
//!
//! Everything a hit goes through comes from the live code, not a copy:
//! stats from `world::effective_stats`, the basic swing from
//! `shared::combat::basic_attack_damage` and `world::cooldown_do_ataque`, the
//! crit from `world::golpe_critico`, the katana from `world::golpe_da_katana`,
//! skills from `world::habilidades::dano_da_skill` and the mitigation from
//! `world::dano_mitigado`. The live damage loop (`damage_events`) applies no
//! PvP scaling of its own — `can_damage_player` only gates and the level gap
//! only exists against mobs — so CURRENT rules are exactly that chain.
//!
//! PROPOSED rules multiply the mitigated hit (basic / skill multipliers and a
//! level-gap penalty), then cap it at a share of the target's max health.
//!
//! The fight is a deterministic duel against a target that stands still: no
//! movement, no block, no Shield Wall, no heals. That is the question being
//! asked — "how fast does A delete B if B does nothing" — and it is the
//! ceiling of what a real fight can be.

use std::collections::HashMap;

use serde::Serialize;
use shared::skills::{Conjunto, Forma, Skill};

use crate::persistence::CharacterRow;
use crate::world::habilidades::{dano_da_skill, sempre_crita};
use crate::world::{
    cooldown_do_ataque, dano_mitigado, effective_stats, em_execucao, golpe_critico,
    golpe_da_katana,
};

/// Longest fight the rotation simulates.
const LIMITE_S: f32 = 300.0;
/// Events kept in the timeline of each rotation.
const LINHA_DO_TEMPO_MAX: usize = 40;

/// One side of the fight, built from what the session would have.
#[derive(Debug, Clone)]
pub struct Lutador {
    pub nome: String,
    pub nivel: u32,
    pub arma: u16,
    pub stats: shared::PlayerStats,
    pub alocados: [u32; shared::STAT_COUNT],
    /// Unlocked skills of the equipped set, geometry adjusted to the tier.
    pub skills: Vec<(Skill, u8)>,
    pub equipamento: Vec<Peca>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Peca {
    pub slot: &'static str,
    pub item_id: u16,
    pub item: String,
    pub grade: u8,
    pub tier: u8,
    pub refine: u8,
    pub item_level: u16,
}

impl Lutador {
    /// A character as it loads from the realm: the same `effective_stats`
    /// a session computes on login, the skills its weapon and level unlock.
    pub fn do_personagem(row: &CharacterRow, catalogo: &[Skill]) -> Lutador {
        let stats = effective_stats(
            &row.equipment,
            &row.allocated_points,
            &row.proficiencies,
            row.xp,
        );
        let nivel = shared::level_of_xp_with_mult(row.xp, crate::economy::xp_multiplier());
        let arma = row.equipment.weapon.unwrap_or(0);
        let equipamento = shared::EquipSlot::TODOS
            .iter()
            .filter_map(|slot| {
                let id = row.equipment.get(*slot)?;
                let inst = row.equipment.get_inst(*slot);
                Some(Peca {
                    slot: slot.as_db_str(),
                    item_id: id,
                    item: crate::economy::nome_do_item(id),
                    grade: inst.map_or(0, |i| i.rarity),
                    tier: inst.map_or(0, |i| i.tier),
                    refine: inst.map_or(0, |i| i.refinement),
                    item_level: inst.map_or(0, |i| i.item_level),
                })
            })
            .collect();
        Lutador::montar(
            row.name.clone(),
            nivel,
            arma,
            stats,
            row.allocated_points,
            catalogo,
            |id| row.skill_progress.tier(id),
            equipamento,
        )
    }

    /// Shared tail of `do_personagem` and the tests: the skills of the set.
    #[allow(clippy::too_many_arguments)]
    pub fn montar(
        nome: String,
        nivel: u32,
        arma: u16,
        stats: shared::PlayerStats,
        alocados: [u32; shared::STAT_COUNT],
        catalogo: &[Skill],
        tier_de: impl Fn(u32) -> u8,
        equipamento: Vec<Peca>,
    ) -> Lutador {
        let conjunto = Conjunto::da_arma(arma);
        let mut skills: Vec<(Skill, u8)> = catalogo
            .iter()
            .filter(|s| s.conjunto == conjunto && s.destravada(nivel))
            .map(|s| {
                let tier = tier_de(s.id);
                (shared::skills::ajustada_ao_tier(s.clone(), tier), tier)
            })
            .collect();
        skills.sort_by_key(|(s, _)| s.ordem);
        Lutador {
            nome,
            nivel,
            arma,
            stats,
            alocados,
            skills,
            equipamento,
        }
    }

    fn cd_basico(&self) -> f32 {
        cooldown_do_ataque(self.arma, &self.stats, false)
    }

    fn hp_max(&self) -> i32 {
        self.stats.hp_max.max(1)
    }

    fn chance_de_crit(&self) -> f32 {
        self.stats.crit_chance.clamp(0.0, 1.0)
    }

    fn esquiva(&self) -> f32 {
        shared::combat::chance_de_esquiva(self.arma, self.alocados[shared::stat_idx::DES])
    }
}

/// The catalogue the game runs on, or the seeded twelve when it is not loaded
/// (unit tests, a process that has not read the table yet).
pub fn catalogo() -> Vec<Skill> {
    let v = crate::skills::all_skills();
    if v.is_empty() {
        shared::skills::playtest()
    } else {
        v
    }
}

// ─────────────────────────────── rules ───────────────────────────────

#[derive(Debug, Clone, Copy, Serialize)]
pub struct Regras {
    pub label: &'static str,
    /// Multiplier on the mitigated basic hit.
    pub basic_mult: f32,
    /// Multiplier on the mitigated skill hit.
    pub skill_mult: f32,
    /// Most of the target's max HP one hit can take (`None` = no cap).
    pub cap_pct: Option<f32>,
    /// Damage lost per level the attacker is below the target, past
    /// `level_free_gap` levels.
    pub level_penalty_per_level: f32,
    pub level_free_gap: u32,
    /// The level penalty never goes under this multiplier.
    pub level_floor: f32,
    /// Katana execute (x`KATANA_EXECUTE_MULT` under 30% HP) against players.
    pub katana_execute_pvp: bool,
}

impl Regras {
    /// What the live damage loop does in PvP today.
    pub fn atuais() -> Regras {
        Regras {
            label: "current",
            basic_mult: 1.0,
            skill_mult: 1.0,
            cap_pct: None,
            level_penalty_per_level: 0.0,
            level_free_gap: 5,
            level_floor: 1.0,
            katana_execute_pvp: true,
        }
    }

    pub fn propostas_padrao() -> Regras {
        Regras {
            label: "proposed",
            basic_mult: 0.6,
            skill_mult: 0.35,
            // Owner, 12/11/2026: no per-hit cap; skills no longer scale with
            // attack speed, multipliers do the rest.
            cap_pct: None,
            level_penalty_per_level: 0.08,
            level_free_gap: 5,
            level_floor: 0.20,
            katana_execute_pvp: false,
        }
    }

    /// Proposed rules from query params; anything missing or invalid keeps
    /// its default. `cap_pct` <= 0 or >= 1 means no cap.
    pub fn propostas_da_query(q: &HashMap<String, String>) -> Regras {
        let mut r = Regras::propostas_padrao();
        let f = |k: &str| q.get(k).and_then(|v| v.trim().parse::<f32>().ok()).filter(|v| v.is_finite());
        if let Some(v) = f("skill_mult") {
            r.skill_mult = v.max(0.0);
        }
        if let Some(v) = f("basic_mult") {
            r.basic_mult = v.max(0.0);
        }
        if let Some(v) = f("cap_pct") {
            r.cap_pct = (v > 0.0 && v < 1.0).then_some(v);
        }
        if let Some(v) = f("level_penalty_per_level") {
            r.level_penalty_per_level = v.max(0.0);
        }
        if let Some(v) = f("level_floor") {
            r.level_floor = v.clamp(0.0, 1.0);
        }
        if let Some(v) = f("level_free_gap") {
            r.level_free_gap = v.max(0.0) as u32;
        }
        if let Some(v) = q.get("katana_execute_pvp") {
            r.katana_execute_pvp = matches!(v.trim(), "1" | "true" | "on" | "yes");
        }
        r
    }

    /// Level-gap multiplier: only when the attacker is BELOW the target.
    pub fn mult_de_nivel(&self, atacante: u32, alvo: u32) -> f32 {
        let gap = alvo.saturating_sub(atacante);
        if gap <= self.level_free_gap || self.level_penalty_per_level <= 0.0 {
            return 1.0;
        }
        let m = 1.0 - self.level_penalty_per_level * (gap - self.level_free_gap) as f32;
        m.max(self.level_floor).min(1.0)
    }
}

// ─────────────────────────────── one hit ───────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq)]
enum Acao {
    Basico,
    Skill(usize),
}

/// Raw damage before the target's defence, as the live code builds it.
fn bruto(at: &Lutador, acao: Acao, crit: bool, alvo_em_execucao: bool, r: &Regras) -> i32 {
    match acao {
        Acao::Basico => {
            let b = shared::combat::basic_attack_damage(
                &at.stats,
                at.arma,
                at.alocados[shared::stat_idx::INT],
            );
            let b = if crit { golpe_critico(b) } else { b };
            if at.arma == shared::item_id::KATANA {
                // A player always "has a target": the opener is mob-only.
                golpe_da_katana(b, false, alvo_em_execucao && r.katana_execute_pvp)
            } else {
                b
            }
        }
        Acao::Skill(i) => {
            let (skill, tier) = &at.skills[i];
            dano_da_skill(skill, *tier, at.stats.attack_damage, crate::world::cadencia_base_da_arma(at.arma), crit)
        }
    }
}

/// What lands on the target's health.
fn golpe(at: &Lutador, alvo: &Lutador, acao: Acao, crit: bool, execucao: bool, r: &Regras) -> i32 {
    let raw = bruto(at, acao, crit, execucao, r);
    // Same clamp the live loop applies before `dano_mitigado`.
    let reducao = alvo.stats.damage_reduction_pct.clamp(0.0, 0.75);
    let d = dano_mitigado(raw, alvo.stats.defense, reducao);
    let mult = match acao {
        Acao::Basico => r.basic_mult,
        Acao::Skill(_) => r.skill_mult,
    } * r.mult_de_nivel(at.nivel, alvo.nivel);
    let d = if mult != 1.0 {
        ((d as f32) * mult).round().max(1.0) as i32
    } else {
        d
    };
    match r.cap_pct {
        Some(cap) => d.min(((alvo.hp_max() as f32) * cap).floor().max(1.0) as i32),
        None => d,
    }
}

fn acoes(at: &Lutador) -> Vec<Acao> {
    let mut v = vec![Acao::Basico];
    for (i, (s, _)) in at.skills.iter().enumerate() {
        if s.dano > 0 && s.forma != Forma::EmSi {
            v.push(Acao::Skill(i));
        }
    }
    v
}

fn nome_da_acao(at: &Lutador, a: Acao) -> String {
    match a {
        Acao::Basico => "Basic attack".into(),
        Acao::Skill(i) => {
            let (s, t) = &at.skills[i];
            format!("{} ({})", s.nome, shared::skills::tier_romano(*t))
        }
    }
}

fn crit_de(at: &Lutador, a: Acao) -> f32 {
    match a {
        Acao::Basico => at.chance_de_crit(),
        Acao::Skill(i) => {
            let (s, t) = &at.skills[i];
            if sempre_crita(s.id, *t) {
                1.0
            } else {
                at.chance_de_crit()
            }
        }
    }
}

/// Cooldown and how long the body is locked doing it.
fn tempos(at: &Lutador, a: Acao) -> (f32, f32, f32) {
    match a {
        Acao::Basico => (at.cd_basico(), 0.0, shared::PLAYER_ATTACK_IMPACT_S),
        Acao::Skill(i) => {
            let s = &at.skills[i].0;
            (
                shared::skills::espera_efetiva(s.espera_s, at.stats.wis),
                s.trava_s(),
                s.impacto_em(),
            )
        }
    }
}

// ─────────────────────────────── report ───────────────────────────────

#[derive(Debug, Clone, Serialize)]
pub struct Ficha {
    pub name: String,
    pub level: u32,
    pub weapon: &'static str,
    pub weapon_item: u16,
    pub hp: i32,
    pub mp: i32,
    pub mp_regen: f32,
    /// Basic hit before defence (attack + allocated INT on the ring).
    pub basic_damage: i32,
    pub attack: i32,
    pub defense: i32,
    pub crit_chance: f32,
    pub dodge_chance: f32,
    pub basic_cooldown_s: f32,
    pub attacks_per_s: f32,
    /// As stored in the stats; the ladder caps it at `MAX_REDUCTION`.
    pub damage_reduction: f32,
    pub damage_reduction_effective: f32,
    pub allocated: Atributos,
    pub gear: Vec<Peca>,
    pub skills: Vec<SkillFicha>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Atributos {
    pub str_: u32,
    pub dex: u32,
    pub int: u32,
    pub vit: u32,
    pub res: u32,
}

#[derive(Debug, Clone, Serialize)]
pub struct SkillFicha {
    pub id: u32,
    pub name: String,
    pub order: u8,
    pub tier: u8,
    pub shape: &'static str,
    pub cooldown_s: f32,
    pub lock_s: f32,
    pub impact_s: f32,
    pub mp_cost: i32,
    /// Raw damage before the target's defence (0 = support skill).
    pub raw: i32,
    pub raw_crit: i32,
}

pub fn ficha(l: &Lutador) -> Ficha {
    let cd = l.cd_basico();
    let r = Regras::atuais();
    let a = &l.alocados;
    Ficha {
        name: l.nome.clone(),
        level: l.nivel,
        weapon: Conjunto::da_arma(l.arma).nome(),
        weapon_item: l.arma,
        hp: l.stats.hp_max,
        mp: l.stats.mp_max,
        mp_regen: l.stats.mp_regen,
        basic_damage: bruto(l, Acao::Basico, false, false, &r),
        attack: l.stats.attack_damage,
        defense: l.stats.defense,
        crit_chance: l.chance_de_crit(),
        dodge_chance: l.esquiva(),
        basic_cooldown_s: cd,
        attacks_per_s: 1.0 / cd.max(0.05),
        damage_reduction: l.stats.damage_reduction_pct,
        damage_reduction_effective: l
            .stats
            .damage_reduction_pct
            .clamp(0.0, shared::ladder::MAX_REDUCTION),
        allocated: Atributos {
            str_: a[shared::stat_idx::FOR],
            dex: a[shared::stat_idx::DES],
            int: a[shared::stat_idx::INT],
            vit: a[shared::stat_idx::VIT],
            res: a[shared::stat_idx::RES],
        },
        gear: l.equipamento.clone(),
        skills: l
            .skills
            .iter()
            .enumerate()
            .map(|(i, (s, t))| {
                let (cd, trava, impacto) = tempos(l, Acao::Skill(i));
                let dano = s.dano > 0 && s.forma != Forma::EmSi;
                SkillFicha {
                    id: s.id,
                    name: s.nome.clone(),
                    order: s.ordem,
                    tier: *t,
                    shape: s.forma.chave(),
                    cooldown_s: cd,
                    lock_s: trava,
                    impact_s: impacto,
                    mp_cost: s.custo_mp,
                    raw: if dano { bruto(l, Acao::Skill(i), false, false, &r) } else { 0 },
                    raw_crit: if dano { bruto(l, Acao::Skill(i), true, false, &r) } else { 0 },
                }
            })
            .collect(),
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct LinhaDeAcao {
    pub action: String,
    pub kind: &'static str,
    pub cooldown_s: f32,
    pub crit_chance: f32,
    pub raw: i32,
    pub raw_crit: i32,
    /// After mitigation (and the proposed multipliers/cap).
    pub hit: i32,
    pub hit_crit: i32,
    /// Katana basic under 30% HP (only when it applies).
    pub hit_execute: Option<i32>,
    pub hit_pct: f32,
    pub hit_crit_pct: f32,
    /// Expected value per use: crit chance, and target dodge on basics.
    pub expected: f32,
    /// Non-crit hits to take the health to 0.
    pub hits_to_down: u32,
    /// Non-crit hits to finish, Downed bar included (execute counted).
    pub hits_to_kill: u32,
    /// Expected damage per second if spammed alone (cooldown-bound).
    pub dps_alone: f32,
}

#[derive(Debug, Clone, Serialize)]
pub struct Evento {
    pub t: f32,
    pub action: String,
    pub damage: f32,
    pub hp: f32,
    pub downed_hp: f32,
}

#[derive(Debug, Clone, Serialize)]
pub struct Rotacao {
    /// Seconds from the first input to the Downed bar emptied (`None` = not
    /// within the limit).
    pub ttk_s: Option<f32>,
    /// Seconds until the health reaches 0 (Downed).
    pub down_s: Option<f32>,
    pub hits: u32,
    pub skills_cast: u32,
    pub timeline: Vec<Evento>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Duelo {
    pub attacker: String,
    pub target: String,
    pub target_hp: i32,
    pub downed_hp: i32,
    pub level_mult: f32,
    pub actions: Vec<LinhaDeAcao>,
    /// Biggest single hit as a share of the target's health (a crit).
    pub max_hit_pct: f32,
    /// One hit (crit) takes the whole health bar.
    pub one_shot: bool,
    /// Expected value: crit chance as average damage, dodge as average miss.
    pub expected: Rotacao,
    /// Every hit crits, nothing dodged.
    pub burst: Rotacao,
}

#[derive(Debug, Clone, Serialize)]
pub struct Lado {
    pub rules: Regras,
    pub a_vs_b: Duelo,
    pub b_vs_a: Duelo,
    pub verdict: String,
    /// "a", "b", "tie" or "none".
    pub winner: &'static str,
}

#[derive(Debug, Clone, Serialize)]
pub struct Avaliacao {
    pub a: Ficha,
    pub b: Ficha,
    pub current: Lado,
    pub proposed: Lado,
    pub notes: Vec<&'static str>,
}

pub const NOTAS: [&str; 9] = [
    "Target stands still: no movement, block, parry, Master's Counter or Shield Wall (-50..60% for 5-8 s).",
    "No healing on either side: Life Drain, Aura, Bloodthirst and the katana's Thirst lifesteal are ignored.",
    "No buffs: Hunter's Mark, Sword Dance forced crit and Bloodthirst attack speed are ignored.",
    "Range, line of sight, travel and mana are respected only as cooldown/lock/impact time and mana pool + regen; both sides are always in range.",
    "Expected-value run: crit as average damage per hit, target dodge (DEX, basics only) as average miss. Burst run: every hit crits, nothing dodged.",
    "Rotation: highest-damage skill off cooldown first; basics in between; casting locks the body (no basics) until the skill's lock time ends.",
    "Downed: after health reaches 0 the attacker must still drain the flat 100-point Downed bar; overkill does not carry over, and the downed heal timer is ignored.",
    "Katana execute (x1.6 under 30% HP) applies to its basic strike only, including on the Downed bar (health is 0 there).",
    "Proposed rules multiply the hit AFTER defence, then apply the per-hit cap (share of target max HP); the level penalty counts levels past the free gap.",
];

fn linha(at: &Lutador, alvo: &Lutador, a: Acao, r: &Regras) -> LinhaDeAcao {
    let hp = alvo.hp_max();
    let hit = golpe(at, alvo, a, false, false, r);
    let hit_crit = golpe(at, alvo, a, true, false, r);
    let c = crit_de(at, a);
    let esquiva = if a == Acao::Basico { alvo.esquiva() } else { 0.0 };
    let expected = ((1.0 - c) * hit as f32 + c * hit_crit as f32) * (1.0 - esquiva);
    let katana = a == Acao::Basico && at.arma == shared::item_id::KATANA && r.katana_execute_pvp;
    let (cd, trava, _) = tempos(at, a);
    // Hits to kill, stepping through the execute window and the Downed bar.
    let mut vida = hp;
    let mut caido = shared::DOWNED_HP_MAX;
    let mut golpes_derruba = 0u32;
    let mut golpes = 0u32;
    while caido > 0 && golpes < 100_000 {
        let d = golpe(at, alvo, a, false, em_execucao(vida, hp), r);
        golpes += 1;
        if vida > 0 {
            vida = (vida - d).max(0);
            if vida == 0 {
                golpes_derruba = golpes;
            }
        } else {
            caido = (caido - d).max(0);
        }
    }
    LinhaDeAcao {
        action: nome_da_acao(at, a),
        kind: if a == Acao::Basico { "basic" } else { "skill" },
        cooldown_s: cd,
        crit_chance: c,
        raw: bruto(at, a, false, false, r),
        raw_crit: bruto(at, a, true, false, r),
        hit,
        hit_crit,
        hit_execute: katana.then(|| golpe(at, alvo, a, false, true, r)),
        hit_pct: hit as f32 / hp as f32,
        hit_crit_pct: hit_crit as f32 / hp as f32,
        expected,
        hits_to_down: golpes_derruba,
        hits_to_kill: golpes,
        dps_alone: expected / cd.max(trava).max(0.05),
    }
}

/// The deterministic rotation against a target that does nothing.
fn rotacao(at: &Lutador, alvo: &Lutador, r: &Regras, burst: bool) -> Rotacao {
    let hp_max = alvo.hp_max();
    let mut vida = hp_max as f32;
    let mut caido = shared::DOWNED_HP_MAX as f32;
    let esquiva = if burst { 0.0 } else { alvo.esquiva() };
    let dano_esperado = |a: Acao, vida: f32| -> f32 {
        let exec = em_execucao(vida.ceil() as i32, hp_max);
        let n = golpe(at, alvo, a, false, exec, r) as f32;
        let c = golpe(at, alvo, a, true, exec, r) as f32;
        let ch = if burst { 1.0 } else { crit_de(at, a) };
        let d = (1.0 - ch) * n + ch * c;
        if a == Acao::Basico {
            d * (1.0 - esquiva)
        } else {
            d
        }
    };
    // Skills by damage at full health, biggest first.
    let mut ordem: Vec<usize> = acoes(at)
        .into_iter()
        .filter_map(|a| match a {
            Acao::Skill(i) => Some(i),
            Acao::Basico => None,
        })
        .collect();
    ordem.sort_by(|x, y| {
        dano_esperado(Acao::Skill(*y), hp_max as f32)
            .total_cmp(&dano_esperado(Acao::Skill(*x), hp_max as f32))
    });
    let cd_basico = at.cd_basico();
    let mut pronta: Vec<f32> = vec![0.0; at.skills.len()];
    let mut mp = at.stats.mp_max.max(0) as f32;
    let mp_max = mp;
    let regen = at.stats.mp_regen.max(0.0);
    let mut proximo_basico = 0.0f32;
    let mut travado_ate = 0.0f32;
    let mut t = 0.0f32;
    // (impact time, action), applied in time order.
    let mut impactos: Vec<(f32, Acao)> = Vec::new();
    let mut out = Rotacao {
        ttk_s: None,
        down_s: None,
        hits: 0,
        skills_cast: 0,
        timeline: Vec::new(),
    };
    loop {
        // Decide at `t`.
        if t >= travado_ate {
            let skill = ordem.iter().copied().find(|&i| {
                pronta[i] <= t + 1e-4 && mp + 1e-3 >= at.skills[i].0.custo_mp as f32
            });
            if let Some(i) = skill {
                let (cd, trava, impacto) = tempos(at, Acao::Skill(i));
                pronta[i] = t + cd;
                mp -= at.skills[i].0.custo_mp as f32;
                travado_ate = t + trava;
                impactos.push((t + impacto, Acao::Skill(i)));
                out.skills_cast += 1;
            } else if t + 1e-4 >= proximo_basico {
                proximo_basico = t + cd_basico;
                impactos.push((t + shared::PLAYER_ATTACK_IMPACT_S, Acao::Basico));
            }
        }
        // Next moment something can be decided.
        let mut proximo = proximo_basico.max(travado_ate);
        for &i in &ordem {
            let p = pronta[i].max(travado_ate);
            if p > t + 1e-4 && p < proximo {
                proximo = p;
            }
        }
        if proximo <= t + 1e-4 {
            proximo = t + 0.01;
        }
        // Land every impact up to then.
        impactos.sort_by(|a, b| a.0.total_cmp(&b.0));
        while let Some(&(ti, a)) = impactos.first() {
            if ti > proximo {
                break;
            }
            impactos.remove(0);
            let d = dano_esperado(a, vida);
            out.hits += 1;
            if vida > 0.0 {
                vida = (vida - d).max(0.0);
                if vida <= 0.0 {
                    out.down_s = Some(ti);
                }
            } else {
                caido = (caido - d).max(0.0);
            }
            if out.timeline.len() < LINHA_DO_TEMPO_MAX {
                out.timeline.push(Evento {
                    t: ti,
                    action: nome_da_acao(at, a),
                    damage: d,
                    hp: vida,
                    downed_hp: caido,
                });
            }
            if caido <= 0.0 {
                out.ttk_s = Some(ti);
                return out;
            }
        }
        mp = (mp + regen * (proximo - t)).min(mp_max);
        t = proximo;
        if t > LIMITE_S {
            return out;
        }
    }
}

fn duelo(at: &Lutador, alvo: &Lutador, r: &Regras) -> Duelo {
    let actions: Vec<LinhaDeAcao> = acoes(at).into_iter().map(|a| linha(at, alvo, a, r)).collect();
    let max_hit = actions.iter().map(|l| l.hit_crit).max().unwrap_or(0);
    Duelo {
        attacker: at.nome.clone(),
        target: alvo.nome.clone(),
        target_hp: alvo.hp_max(),
        downed_hp: shared::DOWNED_HP_MAX,
        level_mult: r.mult_de_nivel(at.nivel, alvo.nivel),
        max_hit_pct: max_hit as f32 / alvo.hp_max() as f32,
        one_shot: max_hit >= alvo.hp_max(),
        actions,
        expected: rotacao(at, alvo, r, false),
        burst: rotacao(at, alvo, r, true),
    }
}

fn lado(a: &Lutador, b: &Lutador, r: Regras) -> Lado {
    let ab = duelo(a, b, &r);
    let ba = duelo(b, a, &r);
    let (winner, verdict) = match (ab.expected.ttk_s, ba.expected.ttk_s) {
        (None, None) => (
            "none",
            format!("Nobody finishes the other within {LIMITE_S:.0} s."),
        ),
        (Some(x), None) => ("a", format!("{} kills {} in {x:.1} s; {} cannot finish within {LIMITE_S:.0} s.", a.nome, b.nome, b.nome)),
        (None, Some(y)) => ("b", format!("{} kills {} in {y:.1} s; {} cannot finish within {LIMITE_S:.0} s.", b.nome, a.nome, a.nome)),
        (Some(x), Some(y)) if (x - y).abs() < 0.05 => (
            "tie",
            format!("Trade: both finish in about {x:.1} s."),
        ),
        (Some(x), Some(y)) if x < y => (
            "a",
            format!("{} kills {} first: {x:.1} s (the other needs {y:.1} s).", a.nome, b.nome),
        ),
        (Some(x), Some(y)) => (
            "b",
            format!("{} kills {} first: {y:.1} s (the other needs {x:.1} s).", b.nome, a.nome),
        ),
    };
    Lado {
        rules: r,
        a_vs_b: ab,
        b_vs_a: ba,
        verdict,
        winner,
    }
}

/// The whole evaluation: both sheets, both directions, both rule sets.
pub fn avaliar(a: &Lutador, b: &Lutador, propostas: Regras) -> Avaliacao {
    Avaliacao {
        a: ficha(a),
        b: ficha(b),
        current: lado(a, b, Regras::atuais()),
        proposed: lado(a, b, propostas),
        notes: NOTAS.to_vec(),
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    /// A synthetic character through the real `effective_stats`: points in
    /// one attribute, no gear but the weapon.
    pub(super) fn sintetico(nome: &str, nivel: u32, arma: u16, pontos: [u32; shared::STAT_COUNT]) -> Lutador {
        let xp = shared::xp_for_level_with_mult(nivel, crate::economy::xp_multiplier());
        let equip = shared::Equipment {
            weapon: Some(arma),
            ..Default::default()
        };
        let stats = effective_stats(&equip, &pontos, &[0; shared::PROF_COUNT], xp);
        Lutador::montar(
            nome.into(),
            nivel,
            arma,
            stats,
            pontos,
            &shared::skills::playtest(),
            |_| 1,
            Vec::new(),
        )
    }

    /// Stats set by hand: what a geared, all-in character looks like.
    fn com_stats(nome: &str, nivel: u32, arma: u16, hp: i32, atk: i32, def: i32, crit: f32) -> Lutador {
        let mut l = sintetico(nome, nivel, arma, [0; shared::STAT_COUNT]);
        l.stats.hp_max = hp;
        l.stats.attack_damage = atk;
        l.stats.defense = def;
        l.stats.crit_chance = crit;
        l.stats.damage_reduction_pct = 0.0;
        l
    }

    #[test]
    fn regras_atuais_sao_a_cadeia_do_jogo() {
        let a = com_stats("A", 30, shared::item_id::ESPADA_E_ESCUDO, 1000, 300, 50, 0.0);
        let b = com_stats("B", 30, shared::item_id::ESPADA_E_ESCUDO, 1000, 300, 50, 0.0);
        let r = Regras::atuais();
        assert_eq!(golpe(&a, &b, Acao::Basico, false, false, &r), dano_mitigado(300, 50, 0.0));
        assert_eq!(
            golpe(&a, &b, Acao::Basico, true, false, &r),
            dano_mitigado(golpe_critico(300), 50, 0.0)
        );
        assert_eq!(r.mult_de_nivel(18, 48), 1.0, "no level gap in PvP today");
    }

    /// Equal levels, glass cannons: under CURRENT rules a skill crit takes
    /// the whole bar — the one-shot the owner saw.
    #[test]
    fn mesmo_nivel_one_shot_nas_regras_atuais() {
        let a = com_stats("A", 40, shared::item_id::PISTOLAS, 900, 420, 30, 0.35);
        let b = com_stats("B", 40, shared::item_id::PISTOLAS, 900, 420, 30, 0.35);
        let av = avaliar(&a, &b, Regras::propostas_padrao());
        let d = &av.current.a_vs_b;
        assert!(d.one_shot, "max hit {:.0}% of HP", d.max_hit_pct * 100.0);
        assert!(d.expected.ttk_s.unwrap() < 2.0, "{:?}", d.expected.ttk_s);
        // The proposed defaults turn it into a multi-hit fight.
        let p = &av.proposed.a_vs_b;
        assert!(!p.one_shot);
        assert!(p.expected.hits >= 3, "{} hits", p.expected.hits);
        assert!(p.expected.ttk_s.unwrap() > d.expected.ttk_s.unwrap() * 2.0);
    }

    /// Level 18 vs level 48: the proposed level penalty sits on the floor,
    /// and the low side cannot burst the high side down quickly.
    #[test]
    fn grande_diferenca_de_nivel_nao_mata_rapido() {
        let baixo = com_stats("Low", 18, shared::item_id::KATANA, 500, 260, 15, 0.30);
        let alto = com_stats("High", 48, shared::item_id::ESPADA_E_ESCUDO, 1400, 300, 60, 0.10);
        let r = Regras::propostas_padrao();
        assert!((r.mult_de_nivel(18, 48) - 0.20).abs() < 1e-6);
        assert_eq!(r.mult_de_nivel(48, 18), 1.0, "only the lower side is penalised");
        let av = avaliar(&baixo, &alto, r);
        let p = &av.proposed.a_vs_b;
        eprintln!(
            "current burst {:?} ev {:?} / proposed burst {:?} ev {:?}",
            av.current.a_vs_b.burst.ttk_s,
            av.current.a_vs_b.expected.ttk_s,
            p.burst.ttk_s,
            p.expected.ttk_s
        );
        assert!(p.burst.ttk_s.is_none_or(|t| t > 10.0), "burst {:?}", p.burst.ttk_s);
        assert!(p.expected.ttk_s.is_none_or(|t| t > 15.0), "{:?}", p.expected.ttk_s);
        // Today it is much faster.
        let c = &av.current.a_vs_b;
        assert!(c.burst.ttk_s.unwrap() < p.burst.ttk_s.unwrap_or(f32::MAX) / 3.0);
        assert_eq!(av.proposed.winner, "b");
    }

    #[test]
    fn execucao_da_katana_e_a_barra_de_caido() {
        let a = com_stats("K", 30, shared::item_id::KATANA, 1000, 200, 0, 0.0);
        let b = com_stats("T", 30, shared::item_id::ESPADA_E_ESCUDO, 1000, 200, 0, 0.0);
        let atual = linha(&a, &b, Acao::Basico, &Regras::atuais());
        assert_eq!(atual.hit, 200);
        assert_eq!(atual.hit_execute, Some(golpe_da_katana(200, false, true)));
        // 4 hits to 200 HP (<30%), the 5th executes; then the Downed bar.
        assert_eq!(atual.hits_to_down, 5);
        assert_eq!(atual.hits_to_kill, 6);
        let mut sem = Regras::atuais();
        sem.katana_execute_pvp = false;
        let l = linha(&a, &b, Acao::Basico, &sem);
        assert_eq!(l.hit_execute, None);
        assert_eq!((l.hits_to_down, l.hits_to_kill), (5, 6));
    }

    #[test]
    fn query_vazia_da_os_padroes_e_parametros_mudam() {
        let r = Regras::propostas_da_query(&HashMap::new());
        assert_eq!(r.skill_mult, 0.35);
        assert_eq!(r.cap_pct, None);
        assert!(!r.katana_execute_pvp);
        let q: HashMap<String, String> = [
            ("skill_mult", "0.5"),
            ("cap_pct", "0"),
            ("katana_execute_pvp", "true"),
            ("level_floor", "abc"),
        ]
        .into_iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect();
        let r = Regras::propostas_da_query(&q);
        assert_eq!(r.skill_mult, 0.5);
        assert_eq!(r.cap_pct, None);
        assert!(r.katana_execute_pvp);
        assert_eq!(r.level_floor, 0.20);
    }

    /// Prints a sample evaluation (run with `--nocapture`).
    #[test]
    fn amostra_json() {
        let a = sintetico("Alpha", 48, shared::item_id::ESPADA_E_ESCUDO, [40, 0, 0, 30, 0, 20]);
        let b = sintetico("Beta", 18, shared::item_id::KATANA, [30, 10, 0, 0, 0, 0]);
        let av = avaliar(&a, &b, Regras::propostas_padrao());
        let j = serde_json::to_string_pretty(&av).unwrap();
        assert!(j.contains("\"verdict\""));
        println!("{j}");
    }
}
