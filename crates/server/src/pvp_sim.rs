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
//! Two kinds of fight:
//!
//! * the ONE-SIDED runs (`Duelo`, expected and all-crit burst): a target
//!   that stands still and does nothing — "how fast does A delete B" — the
//!   ceiling of what a real fight can be;
//! * the INTERLEAVED duel (`duelo_intercalado`): both act on one 30 Hz clock
//!   with Bulwark, the Leap shield, Life Drain, Aura, the katana's Thirst,
//!   HP/MP regen (resting rule included), the hit-stun and, optionally, AUTO
//!   potions, through the helpers the live loop calls (`golpe_na_muralha`,
//!   `cura_do_dreno`, `cura_evoluida`, `cura_da_sede`, `regen_de_hp`...).
//!   An expected-value run plus a seeded Monte Carlo (win %, TTK quantiles).
//!   Hunter's Mark, Sword Dance, Bloodthirst, Master's Counter and block are
//!   dead code in the live game and are not modeled (`NOTAS_DO_DUELO`).
//!
//! `torneio_padrao` runs the four level-55 archetypes (`arquetipos`, one
//! power budget: `Orcamento`) against each other under every rule set of
//! `regras_do_torneio`; the `#[ignore]` test `torneio_de_arquetipos` prints it.

use std::collections::HashMap;

use serde::Serialize;
use shared::skills::{Conjunto, Forma, Skill};

use crate::persistence::CharacterRow;
use crate::world::habilidades::{
    cura_do_dreno, cura_evoluida, dano_da_skill, duracao_do_baluarte, escudo_do_salto, sempre_crita,
};
use crate::world::{
    cooldown_do_ataque, cura_da_sede, dano_mitigado, effective_stats, em_execucao, folego_de_iniciante,
    golpe_critico, golpe_da_katana, golpe_na_muralha, regen_de_hp,
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
    /// Bulwark's (3) tier: the reduction behind it AND behind the Leap's
    /// tier VIII shield (`world::reducao_da_muralha`), like the live loop.
    pub tier_do_baluarte: u8,
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
        let equipamento = pecas_de(&row.equipment);
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
            tier_do_baluarte: tier_de(3),
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

fn pecas_de(e: &shared::Equipment) -> Vec<Peca> {
    shared::EquipSlot::TODOS
        .iter()
        .filter_map(|slot| {
            let id = e.get(*slot)?;
            let inst = e.get_inst(*slot);
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
        .collect()
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
            // Owner, 12/11/2026: skill x0.10, basic x0.15; no per-hit cap
            // (skills no longer scale with attack speed, multipliers do the rest).
            basic_mult: 0.15,
            skill_mult: 0.10,
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
    /// Health per second (VIT, gear, pets), before the resting x4.
    pub hp_regen: f32,
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
        hp_regen: l.stats.hp_regen,
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
    /// The interleaved duel (both act, heals, Bulwark, hit-stun) per rule set.
    pub duel_current: DueloIntercalado,
    pub duel_proposed: DueloIntercalado,
    pub notes: Vec<&'static str>,
    pub duel_notes: Vec<&'static str>,
}

/// Notes on the one-sided `current` / `proposed` runs (the ceiling).
pub const NOTAS: [&str; 9] = [
    "One-sided runs: the target stands still and does nothing — no Bulwark, heals, regen or hit-stun. The interleaved duel below models those.",
    "One-sided runs ignore Life Drain, Aura, the katana's Thirst lifesteal and HP regen.",
    "Hunter's Mark, Sword Dance, Bloodthirst, Master's Counter and block/parry are dead code in the live game (nothing sets them since the 12-skill catalogue / the 09/11 stamina removal), so no run models them.",
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
#[cfg(test)]
pub fn avaliar(a: &Lutador, b: &Lutador, propostas: Regras) -> Avaliacao {
    avaliar_com(a, b, propostas, OpcoesDoDuelo::default())
}

pub fn avaliar_com(a: &Lutador, b: &Lutador, propostas: Regras, op: OpcoesDoDuelo) -> Avaliacao {
    Avaliacao {
        a: ficha(a),
        b: ficha(b),
        current: lado(a, b, Regras::atuais()),
        proposed: lado(a, b, propostas),
        duel_current: duelo_intercalado(a, b, &Regras::atuais(), &op),
        duel_proposed: duelo_intercalado(a, b, &propostas, &op),
        notes: NOTAS.to_vec(),
        duel_notes: NOTAS_DO_DUELO.to_vec(),
    }
}

// ─────────────────────────── interleaved duel ───────────────────────────
//
// Both fighters act on the same clock, one server tick (`TICK_DT`) at a
// time, with everything the live loop does to a player-vs-player hit that is
// still reachable in the game:
//
//   * Bulwark (3) and the Leap's (1) tier VIII shield: `muralha_ate`, cut by
//     `world::golpe_na_muralha` (Bulwark tier), lasting
//     `habilidades::duracao_do_baluarte` / `escudo_do_salto`;
//   * Life Drain (10): `habilidades::cura_do_dreno` of the damage the bolt
//     dealt (after defence, the rules and Bulwark), paid only on health;
//   * Aura (11): `habilidades::cura_evoluida` on the caster;
//   * the katana's Thirst, opened by Dance (5) for `KATANA_THIRST_S`:
//     `world::cura_da_sede` of every basic strike (even a dodged one — the
//     live loop pushes the heal before the dodge roll);
//   * HP regen (`stats.hp_regen`, x4 after 2 s not casting nor hurt — the
//     live "resting" rule, both stand still) through `world::regen_de_hp`,
//     the beginner breath through `world::folego_de_iniciante`; MP regen;
//   * the hit-stun: a player's hit puts the target in `HURT_STAGGER_DURATION`,
//     and a hurt player neither swings nor casts (`in_hurt` / `ocupado`);
//   * casting a skill drops the caster's own melee swing still in wind-up
//     (`pending_melee.retain`), so the rotation waits for it to land;
//   * downed: the downed side stops acting, its casts not yet landed are
//     cancelled, heals no longer reach it, and only the other player's hits
//     drain the 100-point bar.
//
// Optional: AUTO potions (Greater Heal / Greater Mana at the client's default
// 60% / 40% thresholds) through `shared::pocoes::EstadoDePocoes`.

/// Longest interleaved duel; past it the fight is called on remaining HP.
pub const DUELO_LIMITE_S: f32 = 120.0;
/// Events kept in the duel timeline.
const DUELO_EVENTOS_MAX: usize = 80;
/// Bulwark goes up when off cooldown once health is under this share.
pub const BALUARTE_ABAIXO_DE: f32 = 0.90;
/// Aura is cast under this share of health, and only if the heal fits.
pub const AURA_ABAIXO_DE: f32 = 0.70;
/// The client's AUTO defaults (`barra::Categoria::limiar_padrao`).
pub const POCAO_DE_VIDA_ABAIXO_DE: f32 = 0.60;
pub const POCAO_DE_MANA_ABAIXO_DE: f32 = 0.40;
/// Most Monte Carlo runs one request may ask for.
pub const MC_MAX: u32 = 2000;

pub const NOTAS_DO_DUELO: [&str; 9] = [
    "Both act on one clock (30 Hz ticks, like the server); the verdict is who empties the other's health AND 100-point downed bar first. Both stand in range; no movement, line of sight or travel time.",
    "Expected run: crits as average damage per hit; DEX dodge as an exact-rate pattern (every 1/p-th basic misses), so hit-stun and Thirst follow the same hits. Monte Carlo: real rolls, seeded, N runs.",
    "Modeled exactly from live code: Bulwark (-50..60% for 5/6/8 s by tier) and the Leap's tier VIII 2 s shield; Life Drain heal (share of the RAW bolt, before the target's defence, so PvP multipliers do not touch it); Aura self-heal; katana Thirst 25% lifesteal on basics for 3 s after Dance (dodged strikes heal too); HP/MP regen with the x4 resting rule; hit-stun 0.25 s after every landed player hit (no swing, no cast).",
    "Not in the live game, so not modeled: Hunter's Mark, Sword Dance forced crit, Bloodthirst (attack speed / lifesteal), Master's Counter, block and parry — the fields exist but nothing sets them. Leap's stun only reaches mobs.",
    "Rotation: Bulwark when off cooldown and health under 90%; Aura under 70% health if the heal fits; then the damage skill with the highest expected damage (Life Drain counts its heal while hurt), but only if it beats the basics its lock displaces; basics in between. Never casts over its own melee swing in wind-up. Mana respected.",
    "AUTO potions (optional): Greater Heal under 60% health, Greater Mana under 40% mana, whenever the group is off cooldown (8 s); no body lock.",
    "Proposed multipliers apply after defence, before Bulwark; the Thirst heal is computed on the hit after both.",
    "Past the time limit the fight is a timeout: no winner, remaining health shown.",
    "Downed: the downed side stops acting and its casts in flight are cancelled; in-flight basics still land.",
];

#[derive(Debug, Clone, Copy, Serialize)]
pub struct OpcoesDoDuelo {
    /// AUTO health/mana potions.
    pub potions: bool,
    /// Player hits stun the target for `HURT_STAGGER_DURATION` (live: on).
    pub hit_stun: bool,
    /// Monte Carlo runs (0 = expected run only).
    pub mc_runs: u32,
    pub seed: u64,
}

impl Default for OpcoesDoDuelo {
    fn default() -> Self {
        OpcoesDoDuelo {
            potions: false,
            hit_stun: true,
            mc_runs: 200,
            seed: 0x7E57_D0E1,
        }
    }
}

impl OpcoesDoDuelo {
    pub fn da_query(q: &HashMap<String, String>) -> OpcoesDoDuelo {
        let mut o = OpcoesDoDuelo::default();
        let liga = |k: &str| q.get(k).map(|v| matches!(v.trim(), "1" | "true" | "on" | "yes"));
        if let Some(v) = liga("potions") {
            o.potions = v;
        }
        if let Some(v) = liga("hit_stun") {
            o.hit_stun = v;
        }
        if let Some(v) = q.get("mc_runs").and_then(|v| v.trim().parse::<u32>().ok()) {
            o.mc_runs = v.min(MC_MAX);
        }
        if let Some(v) = q.get("seed").and_then(|v| v.trim().parse::<u64>().ok()) {
            o.seed = v;
        }
        o
    }
}

/// Crit and dodge: the average (expected run) or a roll (Monte Carlo).
enum Sorte {
    Media,
    Dado(fastrand::Rng),
}

impl Sorte {
    /// Weight of the crit outcome: the chance itself, or 0/1 rolled.
    fn crit(&mut self, p: f32) -> f32 {
        match self {
            Sorte::Media => p,
            Sorte::Dado(r) => {
                if p >= 1.0 || (p > 0.0 && r.f32() < p) {
                    1.0
                } else {
                    0.0
                }
            }
        }
    }

    fn esquiva(&mut self, p: f32, acc: &mut f32) -> bool {
        match self {
            Sorte::Media => {
                *acc += p;
                if *acc >= 1.0 - 1e-5 {
                    *acc -= 1.0;
                    true
                } else {
                    false
                }
            }
            Sorte::Dado(r) => p > 0.0 && r.f32() < p,
        }
    }
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct Curas {
    pub life_drain: f32,
    pub aura: f32,
    pub thirst: f32,
    pub regen: f32,
    pub potions: f32,
}

impl Curas {
    pub fn total(&self) -> f32 {
        self.life_drain + self.aura + self.thirst + self.regen + self.potions
    }
}

struct Corpo<'a> {
    l: &'a Lutador,
    hp_max: f32,
    hp: f32,
    caido: f32,
    downed_em: Option<f32>,
    morto_em: Option<f32>,
    mp: f32,
    pronta: Vec<f32>,
    proximo_basico: f32,
    cd_basico: f32,
    travado_ate: f32,
    ferido_ate: f32,
    muralha_ate: f32,
    sede_ate: f32,
    parado_desde: Option<f32>,
    ultimo_combate: f32,
    resto_regen: f32,
    pocoes: shared::pocoes::EstadoDePocoes,
    acc_esquiva: f32,
    hp_min: f32,
    dano_feito: f32,
    absorvido: f32,
    curas: Curas,
    basicos: u32,
    skills: u32,
    esquivas: u32,
    preso_s: f32,
}

impl<'a> Corpo<'a> {
    fn novo(l: &'a Lutador) -> Corpo<'a> {
        let hp_max = l.hp_max() as f32;
        Corpo {
            l,
            hp_max,
            hp: hp_max,
            caido: shared::DOWNED_HP_MAX as f32,
            downed_em: None,
            morto_em: None,
            mp: l.stats.mp_max.max(0) as f32,
            pronta: vec![0.0; l.skills.len()],
            proximo_basico: 0.0,
            cd_basico: l.cd_basico(),
            travado_ate: 0.0,
            ferido_ate: 0.0,
            muralha_ate: 0.0,
            sede_ate: 0.0,
            parado_desde: None,
            ultimo_combate: 0.0,
            resto_regen: 0.0,
            pocoes: Default::default(),
            acc_esquiva: 0.0,
            hp_min: hp_max,
            dano_feito: 0.0,
            absorvido: 0.0,
            curas: Curas::default(),
            basicos: 0,
            skills: 0,
            esquivas: 0,
            preso_s: 0.0,
        }
    }

    fn caido(&self) -> bool {
        self.downed_em.is_some()
    }

    /// A heal lands (only on a body with health, like `pending_heals`);
    /// returns what it really restored.
    fn curar(&mut self, quanto: f32) -> f32 {
        if self.hp <= 0.0 || quanto <= 0.0 {
            return 0.0;
        }
        let g = quanto.min(self.hp_max - self.hp).max(0.0);
        self.hp += g;
        g
    }
}

#[derive(Debug, Clone, Copy)]
struct Impacto {
    t: f32,
    seq: u32,
    lado: usize,
    acao: Acao,
}

#[derive(Debug, Clone, Serialize)]
pub struct EventoDoDuelo {
    pub t: f32,
    /// "a" or "b": who acted.
    pub by: &'static str,
    pub action: String,
    pub damage: f32,
    pub heal: f32,
    pub hp_a: f32,
    pub hp_b: f32,
}

#[derive(Debug, Clone, Serialize)]
pub struct LadoDoDuelo {
    pub name: String,
    pub hp_max: i32,
    pub hp_final: f32,
    pub hp_final_pct: f32,
    pub hp_min_pct: f32,
    pub downed_s: Option<f32>,
    pub dead_s: Option<f32>,
    pub damage_dealt: f32,
    /// Damage Bulwark / the Leap shield took off incoming hits.
    pub bulwark_absorbed: f32,
    pub heals: Curas,
    pub heal_total: f32,
    pub basics: u32,
    pub skills_cast: u32,
    pub dodges: u32,
    /// Seconds it had an action ready but was held by the hit-stun.
    pub stunned_s: f32,
}

#[derive(Debug, Clone, Serialize)]
pub struct ResultadoDoDuelo {
    /// "a", "b", "tie" (both down) or "none" (timeout).
    pub winner: &'static str,
    /// When the loser's downed bar emptied.
    pub ttk_s: Option<f32>,
    pub timeout: bool,
    pub a: LadoDoDuelo,
    pub b: LadoDoDuelo,
    pub timeline: Vec<EventoDoDuelo>,
}

#[derive(Debug, Clone, Serialize)]
pub struct MonteCarlo {
    pub runs: u32,
    pub a_wins: u32,
    pub b_wins: u32,
    /// Timeouts and double downs.
    pub draws: u32,
    pub a_win_pct: f32,
    pub b_win_pct: f32,
    pub median_ttk_s: Option<f32>,
    pub p10_ttk_s: Option<f32>,
    pub p90_ttk_s: Option<f32>,
    /// Median remaining health of the winner, share of max.
    pub median_winner_hp_pct: Option<f32>,
}

#[derive(Debug, Clone, Serialize)]
pub struct DueloIntercalado {
    pub rules: Regras,
    pub options: OpcoesDoDuelo,
    pub expected: ResultadoDoDuelo,
    pub monte_carlo: Option<MonteCarlo>,
    pub verdict: String,
}

struct Luta<'a> {
    c: [Corpo<'a>; 2],
    r: &'a Regras,
    op: &'a OpcoesDoDuelo,
    sorte: Sorte,
    impactos: Vec<Impacto>,
    seq: u32,
    eventos: Vec<EventoDoDuelo>,
    grava: bool,
}

const LADOS: [&str; 2] = ["a", "b"];

impl<'a> Luta<'a> {
    fn par(&mut self, i: usize) -> (&mut Corpo<'a>, &mut Corpo<'a>) {
        let (x, y) = self.c.split_at_mut(1);
        if i == 0 {
            (&mut x[0], &mut y[0])
        } else {
            (&mut y[0], &mut x[0])
        }
    }

    fn evento(&mut self, t: f32, lado: usize, acao: String, dano: f32, cura: f32) {
        if self.grava && self.eventos.len() < DUELO_EVENTOS_MAX {
            self.eventos.push(EventoDoDuelo {
                t,
                by: LADOS[lado],
                action: acao,
                damage: dano,
                heal: cura,
                hp_a: self.c[0].hp,
                hp_b: self.c[1].hp,
            });
        }
    }

    fn agenda(&mut self, t: f32, lado: usize, acao: Acao) {
        self.seq += 1;
        self.impactos.push(Impacto {
            t,
            seq: self.seq,
            lado,
            acao,
        });
    }

    /// Expected basic damage on the other side right now (for the rotation).
    fn basico_esperado(&self, i: usize) -> f32 {
        let (at, alvo) = (&self.c[i], &self.c[1 - i]);
        let exec = em_execucao(alvo.hp.ceil() as i32, alvo.hp_max as i32);
        let n = golpe(at.l, alvo.l, Acao::Basico, false, exec, self.r) as f32;
        let k = golpe(at.l, alvo.l, Acao::Basico, true, exec, self.r) as f32;
        let c = at.l.chance_de_crit();
        ((1.0 - c) * n + c * k) * (1.0 - alvo.l.esquiva())
    }

    /// Expected value of a damage skill now: damage, plus Life Drain's heal
    /// while hurt.
    fn valor_da_skill(&self, i: usize, k: usize) -> f32 {
        let (at, alvo) = (&self.c[i], &self.c[1 - i]);
        let a = Acao::Skill(k);
        let c = crit_de(at.l, a);
        let n = golpe(at.l, alvo.l, a, false, false, self.r) as f32;
        let x = golpe(at.l, alvo.l, a, true, false, self.r) as f32;
        let mut v = (1.0 - c) * n + c * x;
        let (s, tier) = &at.l.skills[k];
        if s.id == 10 {
            // The heal is a share of what lands, like the live loop.
            let cura = (1.0 - c) * cura_do_dreno(n as i32, *tier) as f32 + c * cura_do_dreno(x as i32, *tier) as f32;
            v += cura.min(at.hp_max - at.hp);
        }
        v
    }

    fn lancar(&mut self, i: usize, k: usize, t: f32) {
        let (s, _) = &self.c[i].l.skills[k];
        let (cd, trava, impacto) = tempos(self.c[i].l, Acao::Skill(k));
        let custo = s.custo_mp as f32;
        let c = &mut self.c[i];
        c.pronta[k] = t + cd;
        c.mp -= custo;
        c.travado_ate = t + trava;
        c.skills += 1;
        c.ultimo_combate = t;
        self.agenda(t + impacto, i, Acao::Skill(k));
    }

    fn decidir(&mut self, i: usize, t: f32) {
        if self.c[i].caido() || self.c[1 - i].morto_em.is_some() {
            return;
        }
        // AUTO potions: no body lock, not held by the hit-stun.
        if self.op.potions {
            let c = &mut self.c[i];
            for (id, frac, limiar) in [
                (shared::item_id::GREATER_HEAL, c.hp / c.hp_max, POCAO_DE_VIDA_ABAIXO_DE),
                (
                    shared::item_id::GREATER_MANA,
                    c.mp / (c.l.stats.mp_max.max(1) as f32),
                    POCAO_DE_MANA_ABAIXO_DE,
                ),
            ] {
                if frac >= limiar {
                    continue;
                }
                let Some(cura) = shared::pocoes::cura_de(id) else { continue };
                if let Ok(na_hora) = c.pocoes.beber(&cura, t, false) {
                    if cura.grupo == shared::pocoes::Grupo::Vida {
                        let g = c.curar(na_hora * c.hp_max);
                        c.curas.potions += g;
                    } else {
                        let mp_max = c.l.stats.mp_max as f32;
                        c.mp = (c.mp + na_hora * mp_max).min(mp_max);
                    }
                }
            }
        }
        if t + 1e-4 < self.c[i].travado_ate {
            return;
        }
        let basico_pronto = t + 1e-4 >= self.c[i].proximo_basico;
        let algum_pronto = basico_pronto || self.c[i].pronta.iter().any(|p| *p <= t + 1e-4);
        if self.op.hit_stun && t + 1e-4 < self.c[i].ferido_ate {
            if algum_pronto {
                self.c[i].preso_s += shared::TICK_DT;
            }
            return;
        }
        if !algum_pronto {
            return;
        }
        let melee = shared::weapon_is_melee(self.c[i].l.arma);
        let swing_no_ar = melee
            && self
                .impactos
                .iter()
                .any(|m| m.lado == i && m.acao == Acao::Basico);
        if !swing_no_ar {
            if let Some(k) = self.escolher_skill(i, t) {
                self.lancar(i, k, t);
                return;
            }
        }
        if basico_pronto {
            let c = &mut self.c[i];
            c.proximo_basico = t + c.cd_basico;
            c.basicos += 1;
            self.agenda(t + shared::PLAYER_ATTACK_IMPACT_S, i, Acao::Basico);
        }
    }

    fn escolher_skill(&self, i: usize, t: f32) -> Option<usize> {
        let c = &self.c[i];
        let pronta = |k: usize| {
            c.pronta[k] <= t + 1e-4 && c.mp + 1e-3 >= c.l.skills[k].0.custo_mp as f32
        };
        let frac = c.hp / c.hp_max;
        // Defensive first.
        for (k, (s, tier)) in c.l.skills.iter().enumerate() {
            if !pronta(k) {
                continue;
            }
            if s.id == 3 && frac < BALUARTE_ABAIXO_DE && c.muralha_ate <= t {
                return Some(k);
            }
            if s.forma != Forma::Projetil && s.dano <= 0 && s.cura > 0 && frac < AURA_ABAIXO_DE {
                let cura = cura_evoluida(s.cura, *tier, s.id) as f32;
                if cura <= c.hp_max - c.hp {
                    return Some(k);
                }
            }
        }
        // Then damage, if it beats the basics its lock displaces.
        let basico = self.basico_esperado(i);
        let mut melhor: Option<(usize, f32)> = None;
        for (k, (s, _)) in c.l.skills.iter().enumerate() {
            if s.dano <= 0 || s.forma == Forma::EmSi || !pronta(k) {
                continue;
            }
            let v = self.valor_da_skill(i, k);
            let (_, trava, _) = tempos(c.l, Acao::Skill(k));
            let deslocado = basico * trava / c.cd_basico.max(0.05);
            if v > deslocado && melhor.is_none_or(|(_, m)| v > m) {
                melhor = Some((k, v));
            }
        }
        melhor.map(|(k, _)| k)
    }

    /// A hit's final damage for one crit outcome: mitigation and rules
    /// (`golpe`), then the target's Bulwark.
    fn final_do_golpe(&self, i: usize, a: Acao, crit: bool, t: f32) -> (i32, i32) {
        let (at, alvo) = (&self.c[i], &self.c[1 - i]);
        let exec = em_execucao(alvo.hp.ceil() as i32, alvo.hp_max as i32);
        let d = golpe(at.l, alvo.l, a, crit, exec, self.r);
        if alvo.muralha_ate > t {
            (golpe_na_muralha(d, alvo.l.tier_do_baluarte), d)
        } else {
            (d, d)
        }
    }

    fn pousar(&mut self, imp: Impacto, t: f32) {
        let i = imp.lado;
        let j = 1 - i;
        match imp.acao {
            Acao::Basico => {
                let w = self.sorte.crit(self.c[i].l.chance_de_crit());
                let (n, n0) = self.final_do_golpe(i, Acao::Basico, false, t);
                let (k, k0) = self.final_do_golpe(i, Acao::Basico, true, t);
                let d = (1.0 - w) * n as f32 + w * k as f32;
                let absorvido = (1.0 - w) * (n0 - n) as f32 + w * (k0 - k) as f32;
                // Thirst: pushed before the dodge roll in the live loop.
                let mut cura = 0.0;
                if self.c[i].l.arma == shared::item_id::KATANA && self.c[i].sede_ate > t {
                    let q = (1.0 - w) * cura_da_sede(n) as f32 + w * cura_da_sede(k) as f32;
                    cura = self.c[i].curar(q);
                    self.c[i].curas.thirst += cura;
                }
                let p = self.c[j].l.esquiva();
                let mut acc = self.c[j].acc_esquiva;
                let esquivou = self.sorte.esquiva(p, &mut acc);
                self.c[j].acc_esquiva = acc;
                if esquivou {
                    self.c[j].esquivas += 1;
                    self.c[j].ultimo_combate = t;
                    self.evento(t, i, "Basic attack (dodged)".into(), 0.0, cura);
                    return;
                }
                self.c[j].absorvido += absorvido;
                self.ferir(i, d, t);
                self.evento(t, i, "Basic attack".into(), d, cura);
            }
            Acao::Skill(k) => {
                let (s, tier) = self.c[i].l.skills[k].clone();
                if s.id == 3 {
                    let c = &mut self.c[i];
                    c.muralha_ate = c.muralha_ate.max(t + duracao_do_baluarte(&s, tier));
                    self.evento(t, i, nome_da_acao(self.c[i].l, imp.acao), 0.0, 0.0);
                    return;
                }
                if s.id == 5 {
                    let c = &mut self.c[i];
                    c.sede_ate = c.sede_ate.max(t + shared::KATANA_THIRST_S);
                }
                if let Some(e) = escudo_do_salto(tier).filter(|_| s.id == 1) {
                    let c = &mut self.c[i];
                    c.muralha_ate = c.muralha_ate.max(t + e);
                }
                let mut cura = 0.0;
                if s.dano <= 0 {
                    if s.cura > 0 {
                        cura = self.c[i].curar(cura_evoluida(s.cura, tier, s.id) as f32);
                        self.c[i].curas.aura += cura;
                    }
                    self.evento(t, i, nome_da_acao(self.c[i].l, imp.acao), 0.0, cura);
                    return;
                }
                let w = self.sorte.crit(crit_de(self.c[i].l, imp.acao));
                let (n, n0) = self.final_do_golpe(i, imp.acao, false, t);
                let (x, x0) = self.final_do_golpe(i, imp.acao, true, t);
                let d = (1.0 - w) * n as f32 + w * x as f32;
                self.c[j].absorvido += (1.0 - w) * (n0 - n) as f32 + w * (x0 - x) as f32;
                // Life Drain heals a share of the damage that landed (after
                // defence, the rules and Bulwark), only when it reaches health:
                // the live loop pays it where HP drops, not on the downed bar.
                let alvo_de_pe = !self.c[j].caido();
                if s.id == 10 && alvo_de_pe {
                    let q = (1.0 - w) * cura_do_dreno(n, tier) as f32 + w * cura_do_dreno(x, tier) as f32;
                    cura = self.c[i].curar(q);
                    self.c[i].curas.life_drain += cura;
                }
                self.ferir(i, d, t);
                self.evento(t, i, nome_da_acao(self.c[i].l, imp.acao), d, cura);
            }
        }
    }

    /// `i` lands `d` on the other side: health, or the downed bar.
    fn ferir(&mut self, i: usize, d: f32, t: f32) {
        let hit_stun = self.op.hit_stun;
        let (at, alvo) = self.par(i);
        at.dano_feito += d;
        at.ultimo_combate = t;
        alvo.ultimo_combate = t;
        // A body that went down this tick takes nothing more this tick
        // ("não reatribui um golpe fatal" in the live loop).
        if alvo.downed_em == Some(t) {
            return;
        }
        if alvo.caido() {
            alvo.caido = (alvo.caido - d).max(0.0);
            if alvo.caido <= 0.0 && alvo.morto_em.is_none() {
                alvo.morto_em = Some(t);
            }
            return;
        }
        alvo.hp = (alvo.hp - d).max(0.0);
        alvo.hp_min = alvo.hp_min.min(alvo.hp);
        if hit_stun {
            alvo.ferido_ate = t + shared::HURT_STAGGER_DURATION;
        }
        if alvo.hp <= 0.0 {
            alvo.downed_em = Some(t);
            alvo.pocoes.encerrar_curas();
            let j = 1 - i;
            // `avancar_habilidades` cancels a downed caster's casts.
            self.impactos
                .retain(|m| !(m.lado == j && matches!(m.acao, Acao::Skill(_))));
        }
    }

    fn regenerar(&mut self, t: f32) {
        let dt = shared::TICK_DT;
        for c in self.c.iter_mut() {
            let mp_max = c.l.stats.mp_max as f32;
            if c.mp < mp_max {
                c.mp = (c.mp + c.l.stats.mp_regen * dt).min(mp_max);
            }
            if c.caido() {
                continue;
            }
            let parado = t >= c.travado_ate && t >= c.ferido_ate;
            if parado {
                c.parado_desde.get_or_insert(t);
            } else {
                c.parado_desde = None;
            }
            if c.l.stats.hp_regen > 0.0 && c.hp < c.hp_max && c.hp > 0.0 {
                let descansa = c.parado_desde.is_some_and(|p| t - p > 2.0);
                let mult = if descansa { 4.0 } else { 1.0 };
                let folego = folego_de_iniciante(c.l.nivel, c.hp_max as i32, t - c.ultimo_combate);
                let pontos = regen_de_hp(0, i32::MAX, &mut c.resto_regen, (c.l.stats.hp_regen * mult + folego) * dt);
                let g = c.curar(pontos as f32);
                c.curas.regen += g;
            }
            if self.op.potions {
                let ganho = c.pocoes.tick(t);
                if ganho[0] > 0.0 {
                    let g = c.curar(ganho[0] * c.hp_max);
                    c.curas.potions += g;
                }
                if ganho[1] > 0.0 {
                    c.mp = (c.mp + ganho[1] * mp_max).min(mp_max);
                }
            }
        }
    }

    fn rodar(mut self) -> ResultadoDoDuelo {
        let dt = shared::TICK_DT;
        let mut t = 0.0f32;
        let mut fim: Option<f32> = None;
        while t <= DUELO_LIMITE_S {
            self.decidir(0, t);
            self.decidir(1, t);
            // Everything due this tick lands, like the server's one damage
            // pass per tick: a cast that reached its impact is not cancelled
            // by its caster going down in the same tick.
            self.impactos.sort_by(|x, y| x.t.total_cmp(&y.t).then(x.seq.cmp(&y.seq)));
            let n = self.impactos.iter().take_while(|m| m.t <= t + 1e-4).count();
            let agora: Vec<Impacto> = self.impactos.drain(..n).collect();
            for m in agora {
                if self.c.iter().any(|c| c.morto_em.is_some()) {
                    break;
                }
                self.pousar(m, t);
            }
            if self.c.iter().any(|c| c.morto_em.is_some()) {
                fim = Some(t);
                break;
            }
            if self.c.iter().all(|c| c.caido()) && self.impactos.is_empty() {
                fim = Some(t);
                break;
            }
            self.regenerar(t);
            t += dt;
        }
        let morto = [self.c[0].morto_em, self.c[1].morto_em];
        let (winner, ttk) = match (morto[0], morto[1]) {
            (None, Some(x)) => ("a", Some(x)),
            (Some(x), None) => ("b", Some(x)),
            (Some(x), Some(_)) => ("tie", Some(x)),
            (None, None) if fim.is_some() => ("tie", fim),
            (None, None) => ("none", None),
        };
        let lado = |c: &Corpo| LadoDoDuelo {
            name: c.l.nome.clone(),
            hp_max: c.hp_max as i32,
            hp_final: c.hp,
            hp_final_pct: c.hp / c.hp_max,
            hp_min_pct: c.hp_min / c.hp_max,
            downed_s: c.downed_em,
            dead_s: c.morto_em,
            damage_dealt: c.dano_feito,
            bulwark_absorbed: c.absorvido,
            heal_total: c.curas.total(),
            heals: c.curas.clone(),
            basics: c.basicos,
            skills_cast: c.skills,
            dodges: c.esquivas,
            stunned_s: c.preso_s,
        };
        ResultadoDoDuelo {
            winner,
            ttk_s: ttk,
            timeout: winner == "none",
            a: lado(&self.c[0]),
            b: lado(&self.c[1]),
            timeline: self.eventos,
        }
    }
}

fn uma_luta(a: &Lutador, b: &Lutador, r: &Regras, op: &OpcoesDoDuelo, sorte: Sorte, grava: bool) -> ResultadoDoDuelo {
    Luta {
        c: [Corpo::novo(a), Corpo::novo(b)],
        r,
        op,
        sorte,
        impactos: Vec::new(),
        seq: 0,
        eventos: Vec::new(),
        grava,
    }
    .rodar()
}

fn quantil(v: &[f32], q: f32) -> Option<f32> {
    if v.is_empty() {
        return None;
    }
    let i = ((v.len() - 1) as f32 * q).round() as usize;
    Some(v[i.min(v.len() - 1)])
}

pub fn monte_carlo(a: &Lutador, b: &Lutador, r: &Regras, op: &OpcoesDoDuelo) -> MonteCarlo {
    let mut rng = fastrand::Rng::with_seed(op.seed);
    let (mut wa, mut wb, mut empate) = (0u32, 0u32, 0u32);
    let mut ttks = Vec::new();
    let mut hp_do_vencedor = Vec::new();
    for _ in 0..op.mc_runs {
        let x = uma_luta(a, b, r, op, Sorte::Dado(rng.fork()), false);
        match x.winner {
            "a" => {
                wa += 1;
                hp_do_vencedor.push(x.a.hp_final_pct);
            }
            "b" => {
                wb += 1;
                hp_do_vencedor.push(x.b.hp_final_pct);
            }
            _ => empate += 1,
        }
        if matches!(x.winner, "a" | "b") {
            ttks.extend(x.ttk_s);
        }
    }
    ttks.sort_by(f32::total_cmp);
    hp_do_vencedor.sort_by(f32::total_cmp);
    let n = op.mc_runs.max(1) as f32;
    MonteCarlo {
        runs: op.mc_runs,
        a_wins: wa,
        b_wins: wb,
        draws: empate,
        a_win_pct: wa as f32 / n,
        b_win_pct: wb as f32 / n,
        median_ttk_s: quantil(&ttks, 0.5),
        p10_ttk_s: quantil(&ttks, 0.1),
        p90_ttk_s: quantil(&ttks, 0.9),
        median_winner_hp_pct: quantil(&hp_do_vencedor, 0.5),
    }
}

/// The interleaved duel under one rule set: the expected run, plus the
/// Monte Carlo when `op.mc_runs > 0`.
pub fn duelo_intercalado(a: &Lutador, b: &Lutador, r: &Regras, op: &OpcoesDoDuelo) -> DueloIntercalado {
    let expected = uma_luta(a, b, r, op, Sorte::Media, true);
    let mc = (op.mc_runs > 0).then(|| monte_carlo(a, b, r, op));
    let pct = |x: f32| x * 100.0;
    let mut verdict = match expected.winner {
        "a" | "b" => {
            let (v, p) = if expected.winner == "a" { (&expected.a, &expected.b) } else { (&expected.b, &expected.a) };
            format!(
                "{} kills {} in {:.1} s with {:.0}% health left.",
                v.name,
                p.name,
                expected.ttk_s.unwrap_or(0.0),
                pct(v.hp_final_pct)
            )
        }
        "tie" => "Both go down.".to_string(),
        _ => format!(
            "Nobody dies within {DUELO_LIMITE_S:.0} s ({} {:.0}% / {} {:.0}%).",
            expected.a.name,
            pct(expected.a.hp_final_pct),
            expected.b.name,
            pct(expected.b.hp_final_pct)
        ),
    };
    if let Some(m) = &mc {
        verdict.push_str(&format!(
            " Monte Carlo ({} runs): {} wins {:.0}%, {} wins {:.0}%{}.",
            m.runs,
            a.nome,
            pct(m.a_win_pct),
            b.nome,
            pct(m.b_win_pct),
            m.median_ttk_s.map_or(String::new(), |x| format!(", median TTK {x:.1} s"))
        ));
    }
    DueloIntercalado {
        rules: *r,
        options: *op,
        expected,
        monte_carlo: mc,
        verdict,
    }
}

// ─────────────────────────── archetypes ───────────────────────────

/// The power budget of the round-robin archetypes: same level, same points
/// (`POINTS_PER_LEVEL` x (level - 1)), weapon proficiency at the level, the
/// set's seven pieces (weapon, its off-hand, the set's armour, four
/// accessories) at the same grade / tier / refine / item level through
/// `effective_stats`, every skill at the same tier. Only WHERE the points go
/// differs.
#[derive(Debug, Clone, Copy, Serialize)]
pub struct Orcamento {
    pub level: u32,
    pub grade: u8,
    pub tier_in_grade: u8,
    pub refine: u8,
    pub item_level: u16,
    pub skill_tier: u8,
}

impl Orcamento {
    pub fn nivel_55() -> Orcamento {
        Orcamento {
            level: 55,
            grade: 4,
            tier_in_grade: 2,
            refine: 5,
            item_level: 55,
            skill_tier: 9,
        }
    }

    pub fn pontos(&self) -> u32 {
        shared::POINTS_PER_LEVEL * self.level.saturating_sub(1)
    }
}

/// One archetype through the real stat code, on the budget.
pub fn arquetipo(nome: &str, conjunto: Conjunto, pontos: [u32; shared::STAT_COUNT], o: &Orcamento) -> Lutador {
    use shared::item_id as it;
    use shared::EquipSlot as E;
    let xp = shared::xp_for_level_with_mult(o.level, crate::economy::xp_multiplier());
    let mut profs = [0u64; shared::PROF_COUNT];
    profs[conjunto as usize] = shared::proficiency_xp_for_level(o.level);
    let armadura = match conjunto {
        Conjunto::EspadaEscudo => it::ARMADURA_PESADA,
        Conjunto::Katana => it::ARMADURA_MEDIA,
        Conjunto::Pistolas | Conjunto::AnelMagico => it::ARMADURA_LEVE,
    };
    let mut e = shared::Equipment::default();
    for (slot, id) in [
        (E::Weapon, conjunto.arma()),
        (E::Offhand, conjunto.secundaria()),
        (E::Armor, armadura),
        (E::Earring, it::BRINCO),
        (E::Necklace, it::AMULETO),
        (E::Bracelet, it::BRACELETE),
        (E::Belt, it::CINTO),
    ] {
        let inst = shared::items::ItemInstance::roll_em(
            shared::items::item_template(id),
            o.item_level,
            o.grade,
            o.tier_in_grade,
            || 0.5,
        )
        .map(|mut i| {
            i.refinement = o.refine;
            i
        });
        e.set(slot, Some(id), inst);
    }
    let stats = effective_stats(&e, &pontos, &profs, xp);
    Lutador::montar(
        nome.into(),
        o.level,
        conjunto.arma(),
        stats,
        pontos,
        &catalogo(),
        |_| o.skill_tier,
        pecas_de(&e),
    )
}

/// The four round-robin archetypes on one budget.
pub fn arquetipos(o: &Orcamento) -> Vec<Lutador> {
    use shared::stat_idx as si;
    let p = o.pontos();
    let com = |pares: &[(usize, u32)]| {
        let mut a = [0u32; shared::STAT_COUNT];
        for &(i, v) in pares {
            a[i] = v;
        }
        a
    };
    vec![
        // Anubis-like: every point in DEX (dodge capped at 25% from 50).
        arquetipo("Pistols", Conjunto::Pistolas, com(&[(si::DES, p)]), o),
        // Tank/caster: half INT (attack), half VIT (health, regen).
        arquetipo("Ring", Conjunto::AnelMagico, com(&[(si::INT, p / 2), (si::VIT, p - p / 2)]), o),
        // The shield already brings the tankiness: every point in STR.
        arquetipo("Sword&Shield", Conjunto::EspadaEscudo, com(&[(si::FOR, p)]), o),
        arquetipo("Katana", Conjunto::Katana, com(&[(si::FOR, p)]), o),
    ]
}

/// The multiplier sets of the round-robin: current, then skill/basic pairs.
pub fn regras_do_torneio() -> Vec<Regras> {
    let mut v = vec![Regras::atuais()];
    for (label, s, b) in [
        ("skill 0.10 / basic 0.15 (proposed)", 0.10, 0.15),
        ("skill 0.15 / basic 0.20", 0.15, 0.20),
        ("skill 0.10 / basic 0.20", 0.10, 0.20),
    ] {
        v.push(Regras {
            label,
            basic_mult: b,
            skill_mult: s,
            cap_pct: None,
            level_penalty_per_level: 0.0,
            level_free_gap: 5,
            level_floor: 1.0,
            katana_execute_pvp: false,
        });
    }
    v
}

#[derive(Debug, Clone, Serialize)]
pub struct LinhaDoTorneio {
    pub rules: &'static str,
    pub a: String,
    pub b: String,
    /// Expected run.
    pub winner: String,
    pub ttk_s: Option<f32>,
    pub winner_hp_pct: Option<f32>,
    pub a_hp_pct: f32,
    pub b_hp_pct: f32,
    pub a_win_pct: Option<f32>,
    pub b_win_pct: Option<f32>,
    pub draw_pct: Option<f32>,
    pub median_ttk_s: Option<f32>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Torneio {
    pub budget: Orcamento,
    pub options: OpcoesDoDuelo,
    pub fighters: Vec<Ficha>,
    pub rows: Vec<LinhaDoTorneio>,
    pub notes: Vec<&'static str>,
}

/// Every pair of `lutadores` under every rule set.
pub fn torneio(lutadores: &[Lutador], regras: &[Regras], op: &OpcoesDoDuelo) -> Vec<LinhaDoTorneio> {
    let mut v = Vec::new();
    for r in regras {
        for i in 0..lutadores.len() {
            for j in i + 1..lutadores.len() {
                let (a, b) = (&lutadores[i], &lutadores[j]);
                let d = duelo_intercalado(a, b, r, op);
                let e = &d.expected;
                let (winner, hp) = match e.winner {
                    "a" => (a.nome.clone(), Some(e.a.hp_final_pct)),
                    "b" => (b.nome.clone(), Some(e.b.hp_final_pct)),
                    "tie" => ("both down".into(), None),
                    _ => ("timeout".into(), None),
                };
                let n = d.monte_carlo.as_ref().map(|m| m.runs.max(1) as f32);
                v.push(LinhaDoTorneio {
                    rules: r.label,
                    a: a.nome.clone(),
                    b: b.nome.clone(),
                    winner,
                    ttk_s: e.ttk_s,
                    winner_hp_pct: hp,
                    a_hp_pct: e.a.hp_final_pct,
                    b_hp_pct: e.b.hp_final_pct,
                    a_win_pct: d.monte_carlo.as_ref().map(|m| m.a_win_pct),
                    b_win_pct: d.monte_carlo.as_ref().map(|m| m.b_win_pct),
                    draw_pct: d.monte_carlo.as_ref().zip(n).map(|(m, n)| m.draws as f32 / n),
                    median_ttk_s: d.monte_carlo.as_ref().and_then(|m| m.median_ttk_s),
                });
            }
        }
    }
    v
}

/// The archetype round-robin on the level-55 budget, every rule set.
pub fn torneio_padrao(op: &OpcoesDoDuelo) -> Torneio {
    let o = Orcamento::nivel_55();
    let l = arquetipos(&o);
    Torneio {
        budget: o,
        options: *op,
        fighters: l.iter().map(ficha).collect(),
        rows: torneio(&l, &regras_do_torneio(), op),
        notes: NOTAS_DO_DUELO.to_vec(),
    }
}

/// The round-robin as plain text, one table per rule set.
#[cfg(test)]
pub fn tabela_do_torneio(linhas: &[LinhaDoTorneio]) -> String {
    let mut out = String::new();
    let mut atual = "";
    let s = |x: Option<f32>| x.map_or("-".to_string(), |v| format!("{v:.1}s"));
    let p = |x: Option<f32>| x.map_or("-".to_string(), |v| format!("{:.0}%", v * 100.0));
    for l in linhas {
        if l.rules != atual {
            atual = l.rules;
            out.push_str(&format!(
                "\n== {atual} ==\n{:<26} {:<13} {:>7} {:>8} {:>13} {:>16} {:>8}\n",
                "matchup", "winner (EV)", "TTK", "win HP", "HP a / b", "MC win a/b/draw", "MC TTK"
            ));
        }
        out.push_str(&format!(
            "{:<26} {:<13} {:>7} {:>8} {:>13} {:>16} {:>8}\n",
            format!("{} vs {}", l.a, l.b),
            l.winner,
            s(l.ttk_s),
            p(l.winner_hp_pct),
            format!("{:.0}% / {:.0}%", l.a_hp_pct * 100.0, l.b_hp_pct * 100.0),
            format!("{}/{}/{}", p(l.a_win_pct), p(l.b_win_pct), p(l.draw_pct)),
            s(l.median_ttk_s),
        ));
    }
    out
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
        assert_eq!(r.skill_mult, 0.10);
        assert_eq!(r.basic_mult, 0.15);
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

    fn sem_mc() -> OpcoesDoDuelo {
        OpcoesDoDuelo {
            mc_runs: 0,
            ..Default::default()
        }
    }

    /// Two copies of one build: the expected run is a mirror and the Monte
    /// Carlo splits close to even.
    #[test]
    fn espelho_empata_e_monte_carlo_divide() {
        crate::economy::init_vazia_para_testes();
        let o = Orcamento::nivel_55();
        let k = arquetipo("K", Conjunto::Katana, [o.pontos(), 0, 0, 0, 0, 0], &o);
        let d = duelo_intercalado(&k, &k, &Regras::atuais(), &OpcoesDoDuelo { mc_runs: 200, ..Default::default() });
        assert_eq!(d.expected.a.damage_dealt.round(), d.expected.b.damage_dealt.round(), "{}", d.verdict);
        let m = d.monte_carlo.unwrap();
        assert_eq!(m.runs, 200);
        assert!((m.a_win_pct - m.b_win_pct).abs() < 0.2, "{m:?}");
        // Seeded: the same options give the same answer.
        let m2 = monte_carlo(&k, &k, &Regras::atuais(), &OpcoesDoDuelo { mc_runs: 200, ..Default::default() });
        assert_eq!((m.a_wins, m.b_wins), (m2.a_wins, m2.b_wins));
    }

    /// The live helpers, unchanged: Bulwark's table and the Thirst share.
    #[test]
    fn auxiliares_do_combate_batem_com_o_loop() {
        assert_eq!(golpe_na_muralha(1000, 1), 500);
        assert_eq!(golpe_na_muralha(1000, 10), 400);
        assert_eq!(golpe_na_muralha(1, 10), 1);
        assert_eq!(cura_da_sede(100), 25);
        let baluarte = shared::skills::playtest().into_iter().find(|s| s.id == 3).unwrap();
        assert_eq!(duracao_do_baluarte(&baluarte, 1), 5.0);
        assert_eq!(duracao_do_baluarte(&baluarte, 7), 6.0);
        assert_eq!(duracao_do_baluarte(&baluarte, 10), 8.0);
        assert_eq!(escudo_do_salto(7), None);
        assert_eq!(escudo_do_salto(8), Some(2.0));
    }

    /// Every heal and defence the kits have shows up in the duel.
    #[test]
    fn curas_baluarte_e_atordoamento_aparecem() {
        crate::economy::init_vazia_para_testes();
        let o = Orcamento::nivel_55();
        let l = arquetipos(&o);
        let (pist, anel, espada, katana) = (&l[0], &l[1], &l[2], &l[3]);
        // Under current rules most fights end in one or two skills; the
        // proposed multipliers give the kits time to show.
        let r = regras_do_torneio()[1];
        let d = duelo_intercalado(espada, katana, &r, &sem_mc()).expected;
        assert!(d.a.bulwark_absorbed > 0.0, "Bulwark never cut a hit: {:?}", d.a);
        assert!(d.a.skills_cast > 0);
        assert!(d.b.heals.thirst > 0.0, "Thirst never healed: {:?}", d.b.heals);
        let d = duelo_intercalado(anel, pist, &r, &sem_mc()).expected;
        assert!(d.a.heals.life_drain > 0.0, "{:?}", d.a.heals);
        // Life Drain heals a share of what lands, never more than the damage
        // the ring dealt (the raw-bolt bug healed 526 for 344 dealt).
        assert!(
            d.a.heals.life_drain <= shared::skills::roubo_do_dreno(10) * d.a.damage_dealt + 1.0,
            "drain {} vs dealt {}",
            d.a.heals.life_drain,
            d.a.damage_dealt
        );
        assert!(d.a.heals.regen > 0.0, "VIT regen: {:?}", d.a.heals);
        assert!(d.a.stunned_s > 0.0, "the pistol's hits stun the ring");
        let d = duelo_intercalado(espada, pist, &r, &sem_mc()).expected;
        assert!(d.b.dodges > 0, "DEX dodge: {:?}", d.b);
        // Without the hit-stun nobody is held.
        let sem = OpcoesDoDuelo { hit_stun: false, ..sem_mc() };
        let d2 = duelo_intercalado(anel, pist, &r, &sem).expected;
        assert_eq!(d2.a.stunned_s, 0.0);
        // Potions heal when on.
        let com = OpcoesDoDuelo { potions: true, ..sem_mc() };
        let d3 = duelo_intercalado(anel, pist, &r, &com).expected;
        assert!(d3.a.heals.potions > 0.0 || d3.b.heals.potions > 0.0);
    }

    #[test]
    fn opcoes_da_query() {
        let q: HashMap<String, String> = [("potions", "true"), ("hit_stun", "0"), ("mc_runs", "99999")]
            .into_iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        let o = OpcoesDoDuelo::da_query(&q);
        assert!(o.potions && !o.hit_stun);
        assert_eq!(o.mc_runs, MC_MAX);
        let o = OpcoesDoDuelo::da_query(&HashMap::new());
        assert!(!o.potions && o.hit_stun && o.mc_runs == 200);
    }

    /// The archetype round-robin, every rule set (the owner's table):
    /// `cargo test -p server --bin server --release torneio_de_arquetipos -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn torneio_de_arquetipos() {
        crate::economy::init_vazia_para_testes();
        for potions in [false, true] {
            let op = OpcoesDoDuelo { potions, mc_runs: 200, ..Default::default() };
            let t = torneio_padrao(&op);
            if !potions {
                println!("budget: {:?} ({} points each)", t.budget, t.budget.pontos());
                for f in &t.fighters {
                    println!(
                        "{:<13} HP {:>5} MP {:>4} (+{:.1}/s) atk {:>4} def {:>4} DR {:>3.0}% crit {:>4.1}% dodge {:>4.1}% cd {:.3}s regen {:.1}/s pts STR/DEX/INT/VIT {}/{}/{}/{}",
                        f.name, f.hp, f.mp, f.mp_regen, f.attack, f.defense, f.damage_reduction_effective * 100.0,
                        f.crit_chance * 100.0, f.dodge_chance * 100.0, f.basic_cooldown_s, f.hp_regen,
                        f.allocated.str_, f.allocated.dex, f.allocated.int, f.allocated.vit
                    );
                }
            }
            println!("\n######## potions {} ########", if potions { "ON (AUTO)" } else { "off" });
            println!("{}", tabela_do_torneio(&t.rows));
        }
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
