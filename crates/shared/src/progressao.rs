//! PROGRESSION: how long a level takes, and where its XP comes from.
//!
//! The owner's rule (04/10/2026): in every ten levels the main story gives at
//! most 10-15% of the XP, side missions 10-20%, and the rest is fighting (or
//! the Magic Island). Before this the story's rewards were written by hand
//! and grew faster than the curve — from level 85 a single step paid most of
//! a level — and the Quest Master's ten-minute contracts paid several times
//! the kills they asked for.
//!
//! The target is the BEST case — pistols, always fighting, no gathering,
//! crafting or trading — reaching 120 in about 200 hours. Real players do the
//! rest of the game too, and take longer.
//!
//! `balanceamento::tempo_de_nivel` measures it; `calibrar_xp_do_mob` prints
//! the kill factor below.

/// Seconds level `n` takes in the best case: `SEGUNDOS_POR_NIVEL x n`. One
/// straight line, so every level is slower than the one before (the owner:
/// "it always has to get harder") and 1-120 sums to 200 hours. Bands come out
/// 1.3h, 4.1h, 6.9h … 32h, each ~2.8h longer than the last.
pub const SEGUNDOS_POR_NIVEL: f32 = 720_000.0 / 7_140.0;

/// Best-case seconds to go from `n` to `n + 1`.
pub fn segundos_do_nivel(n: u32) -> f32 {
    SEGUNDOS_POR_NIVEL * n as f32
}

/// THE PACE on top of the calibrated curve (owner, 12/11/2026). At launch
/// Anubis did 1-55 in 54 h online, ~1.55x the best case above: 1-100 in
/// ~215 h, under three weeks of his play. The owner wants the curve "a bit
/// longer" and the long game to come from a level reset at 100 (bonus still
/// to be designed), not from a wall of XP. Every XP a character gains is
/// divided by `ritmo(level)`:
///
/// - levels 1-19: 1 (the start stays quick);
/// - 20-49: a straight ramp from 1 up to `RITMO_MAX`;
/// - 50 on: `RITMO_MAX`.
///
/// With 2.0 the Anubis pace takes ~400 h online for 1-100 (about a month at
/// 12 h a day); the Lenda pace ~630 h (about 3.5 months at 6 h a day). It was
/// 5.7 and then 11.5 for a few hours before the reset idea.
///
/// 14/11/2026: down to 1.25. Six days of prod data: casual players (3-7 h a
/// day) were 3-5x slower than the best case and spent ~100 days of play on
/// 30-60, right where the ramp reaches its top; the 20 h/day auto farmers sat
/// at 5-7 h per level in the 40s-50s. The owner picked 1.25 (best case 1-100
/// ~170 h, between no divisor at 139 h and the old 2.0 at 259 h). Paying players go
/// faster through the XP potion, the Magic Island islet and bought gear: the
/// divisor is applied before those bonuses, so they keep their share. The
/// kill factor, quest shares and the 200 h best case above stay as they are:
/// this is one divisor at the door, easy to tune.
pub const RITMO_MAX: f32 = 1.25;
/// Where the ramp starts and where it reaches `RITMO_MAX`.
pub const RITMO_DE: u32 = 20;
pub const RITMO_ATE: u32 = 50;

/// The divisor of every XP gained at `nivel` (>= 1).
pub fn ritmo(nivel: u32) -> f32 {
    if nivel < RITMO_DE {
        1.0
    } else if nivel >= RITMO_ATE {
        RITMO_MAX
    } else {
        1.0 + (RITMO_MAX - 1.0) * (nivel - RITMO_DE) as f32 / (RITMO_ATE - RITMO_DE) as f32
    }
}

/// `xp` gained at `nivel`, after the pace (never 0 when `xp` > 0).
pub fn no_ritmo(xp: u64, nivel: u32) -> u64 {
    if xp == 0 {
        return 0;
    }
    ((xp as f64 / ritmo(nivel) as f64).round() as u64).max(1)
}

/// Share of each band's XP from the main story.
pub const PARCELA_DA_HISTORIA: f32 = 0.12;
/// Share from side missions (one-off, contracts and dailies together).
pub const PARCELA_SECUNDARIA: f32 = 0.15;
/// What is left: fighting.
pub const PARCELA_DOS_MOBS: f32 = 1.0 - PARCELA_DA_HISTORIA - PARCELA_SECUNDARIA;

use std::collections::HashMap;
use std::sync::OnceLock;

use crate::constants::{xp_for_level_with_mult, DEFAULT_XP_MULTIPLIER};
use crate::quests::{objective_kind as ok, QuestDef, ALVO_QUALQUER_CHEFE, QUESTS};

/// The level cap the targets are written for.
pub const TETO: u32 = 120;

/// THE LEVEL CAP, on the INTERNAL scale (docs/RESET.md, "Mundo novo",
/// 15/11/2026): characters stop at internal level 60 — the end of the
/// Plateau — which the screen shows as level 100 (`escala`). Everything past
/// it (Celeste, Kōgen, Abissal, Magic Island IV-VI) is the "100+" band,
/// gated by POWER. `TETO` stays as the length of the calibration tables
/// (mob kill factor, best-case targets), which still describe mobs above.
///
/// It was 100 on the internal scale for one day (stage A, 14/11) before the
/// world remap moved the shown 100 to internal 60.
pub const NIVEL_MAX: u32 = 60;

/// The level the screen shows at the cap.
pub const NIVEL_MAX_EXIBIDO: u32 = 100;

/// XP of a BOSS kill (field boss or a dungeon's boss) over the species'
/// own XP. Was 5 until 15/11/2026; the owner: "dungeon e boss darem mais
/// XP para compensar fazer" — in an auto game, the content you actually go
/// and play has to pay more than standing in a field.
pub const XP_MULT_CHEFE: u64 = 10;
/// XP of every kill INSIDE a dungeon instance (Gruta, Porão), on top of the
/// level-penalty exemption it already had. Same reason (15/11/2026).
pub const XP_MULT_DUNGEON: f32 = 2.0;

/// The XP of exactly level `NIVEL_MAX` under multiplier `mult`: a character
/// never holds more.
pub fn xp_maximo(mult: u64) -> u64 {
    xp_for_level_with_mult(NIVEL_MAX, mult)
}

// ─────────────────────── the shown scale (15/11/2026) ───────────────────────
//
// The owner: 1 to 100 fast, with Bosque 1-30, Glacier 30-50, Desert 50-70 and
// Plateau 70-100, and everything past that "100+" by power — with the same
// strength as today (option b). Those bands are exactly today's island bands
// renumbered, so the game keeps its INTERNAL levels (mobs, bosses, dungeons,
// quests, items, keys, the XP curve: all calibrated) and only the number on
// SCREEN changes. Every level a player reads goes through `nivel_exibido` /
// `texto_de_nivel` / `texto_de_faixa`; nothing else converts.

/// (internal level, shown level) at each island's edge; straight lines
/// between them. Past internal 60 every level shows as "100+".
pub const ANCORAS_DA_ESCALA: [(f32, f32); 5] =
    [(1.0, 1.0), (15.0, 30.0), (30.0, 50.0), (42.0, 70.0), (60.0, 100.0)];

/// The shown level of a (possibly fractional) internal level, unclamped
/// (past internal 60 it keeps one shown level per internal level).
pub fn exibido_f(interno: f32) -> f32 {
    let a = &ANCORAS_DA_ESCALA;
    if interno <= a[0].0 {
        return a[0].1;
    }
    for w in a.windows(2) {
        let ((x0, y0), (x1, y1)) = (w[0], w[1]);
        if interno <= x1 {
            return y0 + (y1 - y0) * (interno - x0) / (x1 - x0);
        }
    }
    let (x, y) = a[a.len() - 1];
    y + (interno - x)
}

/// The internal level (fractional) of a shown level: the inverse.
pub fn interno_f(exibido: f32) -> f32 {
    let a = &ANCORAS_DA_ESCALA;
    if exibido <= a[0].1 {
        return a[0].0;
    }
    for w in a.windows(2) {
        let ((x0, y0), (x1, y1)) = (w[0], w[1]);
        if exibido <= y1 {
            return x0 + (x1 - x0) * (exibido - y0) / (y1 - y0);
        }
    }
    let (x, y) = a[a.len() - 1];
    x + (exibido - y)
}

/// The shown number of an internal level (whole levels: content, mobs,
/// requirements). "100+" territory still returns a number: use
/// `texto_de_nivel` to print.
pub fn nivel_exibido_de(interno: u32) -> u32 {
    exibido_f(interno as f32).floor().max(1.0) as u32
}

/// Cumulative XP at a fractional INTERNAL level: the curve's own value at the
/// whole levels, straight in between.
fn xp_no_interno(interno: f32, mult: u64) -> u64 {
    let base = interno.floor().max(1.0) as u32;
    let a = xp_for_level_with_mult(base, mult);
    let b = xp_for_level_with_mult(base + 1, mult);
    a + ((b - a) as f64 * (interno - base as f32).clamp(0.0, 1.0) as f64) as u64
}

/// A CHARACTER's shown level from its XP: the internal level plus its
/// progress, converted, capped at `NIVEL_MAX_EXIBIDO`. A shown level-up
/// happens every time this number changes (twice per internal level in the
/// Bosque, where 15 internal levels show as 30).
pub fn nivel_exibido(xp: u64, mult: u64) -> u32 {
    // The largest shown level whose start (`xp_do_nivel_exibido`) is within
    // the XP: defined by the bar's own bounds, so the two never disagree on a
    // rounding.
    let (mut lo, mut hi) = (1u32, NIVEL_MAX_EXIBIDO);
    while lo < hi {
        let mid = (lo + hi + 1) / 2;
        if xp_do_nivel_exibido(mid, mult) <= xp {
            lo = mid;
        } else {
            hi = mid - 1;
        }
    }
    lo
}

/// XP at the START of shown level `n` (for the XP bar: from
/// `xp_do_nivel_exibido(n)` to `xp_do_nivel_exibido(n + 1)`).
pub fn xp_do_nivel_exibido(n: u32, mult: u64) -> u64 {
    xp_no_interno(interno_f(n.max(1) as f32), mult)
}

/// How full the XP bar of the character's SHOWN level is, 0..1 (the bar
/// runs from `xp_do_nivel_exibido(n)` to `xp_do_nivel_exibido(n + 1)`).
pub fn fracao_do_nivel_exibido(xp: u64, mult: u64) -> f32 {
    let n = nivel_exibido(xp, mult);
    let (a, b) = (xp_do_nivel_exibido(n, mult), xp_do_nivel_exibido(n + 1, mult));
    if b > a {
        (xp.saturating_sub(a) as f32 / (b - a) as f32).clamp(0.0, 1.0)
    } else {
        0.0
    }
}

/// How an INTERNAL level reads on screen: the shown number, or "100+" past
/// the cap (content and mobs keep their internal level for the maths).
pub fn texto_de_nivel(nivel: u32) -> String {
    if nivel > NIVEL_MAX {
        format!("{NIVEL_MAX_EXIBIDO}+")
    } else {
        nivel_exibido_de(nivel).to_string()
    }
}

/// An INTERNAL level band on screen: "70–100", "95–100+", or just "100+"
/// when the whole band is past the cap.
pub fn texto_de_faixa(lo: u32, hi: u32) -> String {
    if lo > NIVEL_MAX {
        format!("{NIVEL_MAX_EXIBIDO}+")
    } else {
        format!("{}–{}", nivel_exibido_de(lo), texto_de_nivel(hi))
    }
}

/// Side missions split into three parts. One-off missions share this much
/// of their band's XP between them.
pub const PARCELA_DAS_UNICAS: f32 = 0.05;
/// A repeatable contract pays this fraction of the kills it asks for, on top
/// of them. Done non-stop, contracts are ~7% of the XP; they never outrun
/// fighting, however often they are repeated.
pub const BONUS_DO_CONTRATO: f32 = 0.10;
/// Every daily of an island together pays this many levels (at the dailies'
/// level) per day.
pub const DIARIAS_POR_DIA: f32 = 0.15;

/// Kill-XP factor on the species' table XP, one per level (index = level;
/// past the end, the last). Generated by
/// `balanceamento::tempo_de_nivel::calibrar_xp_do_mob`: for every level, the
/// factor that makes the best case spend `segundos_do_nivel` on it with
/// fighting giving `PARCELA_DOS_MOBS` of the XP. Per level and not per band,
/// so an island whose mobs are worth more, or die faster, cannot make a level
/// shorter than the one before. Rerun it after touching mob XP, mob speed or
/// kill time. (It was `1 + 0.1 x (level - 1)`.)
pub const FATOR_DO_MOB: [f32; 121] = include!("progressao_fator.in");

/// The kill-XP factor at `nivel`.
pub fn fator_do_mob(nivel: u32) -> f32 {
    FATOR_DO_MOB[(nivel as usize).min(FATOR_DO_MOB.len() - 1)]
}

/// XP per point of table health of the "fair" species: the fragile ones
/// (archers 60/105, tigers 55/95, gunmen 50/85) pay about this.
pub const XP_POR_VIDA_JUSTO: f32 = 0.58;
/// The HP top-up never pays more than this times the table XP.
pub const TETO_DO_FATOR_DE_VIDA: f32 = 3.0;

/// TANKY SPECIES PAY FOR THEIR HEALTH (owner, 15/11/2026: "a xp do mob tem q
/// aumentar conforme o hp dele tbm, tem bicho no mesmo lvl que tem muito hp
/// e e' dificil de matar"). The table paid the tanks less than half per point
/// of health (wolf 30/120, bear 75/280, owlbear 110/460, scarab 55/260 =
/// 0.21-0.27) than the fragile ones (~0.58). This factor lifts a species'
/// XP until it pays `XP_POR_VIDA_JUSTO` per table HP, capped at
/// `TETO_DO_FATOR_DE_VIDA`; a species already paying that keeps 1.0. It
/// never lowers anyone.
pub fn fator_de_vida(hp_tabela: i32, xp_tabela: u64) -> f32 {
    if hp_tabela <= 0 {
        return 1.0;
    }
    let paga = xp_tabela.max(1) as f32 / hp_tabela as f32;
    (XP_POR_VIDA_JUSTO / paga).clamp(1.0, TETO_DO_FATOR_DE_VIDA)
}

/// Table XP with the health top-up (`fator_de_vida`), before the level.
pub fn xp_pela_vida(hp_tabela: i32, xp_tabela: u64) -> u64 {
    (xp_tabela as f32 * fator_de_vida(hp_tabela, xp_tabela)).round() as u64
}

/// XP of a kill of a species whose table XP is `base`, at `nivel`.
pub fn xp_do_abate(base: u64, nivel: u32) -> u64 {
    ((base as f32 * fator_do_mob(nivel)).round() as u64).max(1)
}

/// Table XP of a typical regular mob (the zones average 60-84), for the
/// rewards measured in kills.
const BASE_TIPICA: u64 = 70;

fn custo(nivel: u32) -> u64 {
    let m = DEFAULT_XP_MULTIPLIER;
    xp_for_level_with_mult(nivel + 1, m).saturating_sub(xp_for_level_with_mult(nivel, m))
}

fn xp_entre(a: u32, b: u32) -> u64 {
    let m = DEFAULT_XP_MULTIPLIER;
    xp_for_level_with_mult(b, m).saturating_sub(xp_for_level_with_mult(a, m))
}

/// The ten-level band of `nivel`: (first level, last level).
fn faixa(nivel: u32) -> (u32, u32) {
    let f = (nivel.max(1) / 10).min(TETO / 10 - 1);
    (f * 10, f * 10 + 10)
}

/// THE XP A QUEST PAYS, in the default curve. Every place that grants or
/// shows a quest's XP goes through here; the `reward_xp` written in the quest
/// is now only its WEIGHT against the others of its group, so a boss still
/// pays more than a conversation.
///
///   * main story: `PARCELA_DA_HISTORIA` of the XP between two level gates,
///     split among the steps between them;
///   * one-off side mission: `PARCELA_DAS_UNICAS` of its band, split among
///     the band's one-offs;
///   * contract (repeatable on a cooldown): `BONUS_DO_CONTRATO` of the kills
///     it costs;
///   * daily: its share of `DIARIAS_POR_DIA` levels, with the other dailies
///     of its island.
pub fn xp_da_quest(q: &QuestDef) -> u64 {
    if q.reward_xp == 0 {
        return 0;
    }
    if crate::historia::e_da_historia(q.id) {
        return da_historia().get(&q.id).copied().unwrap_or(0);
    }
    if q.daily {
        // The island's dailies together (the Master hands them out where he
        // lives: `quests::zona_da_missao`).
        let ilha = crate::quests::zona_da_missao(q.id);
        let soma: u64 = QUESTS
            .iter()
            .filter(|d| d.daily && crate::quests::zona_da_missao(d.id) == ilha)
            .map(|d| d.reward_xp)
            .sum();
        let dia = DIARIAS_POR_DIA as f64 * custo(q.min_level.max(1)) as f64;
        return (dia * q.reward_xp as f64 / soma.max(1) as f64).round() as u64;
    }
    if q.repeatable {
        let abates = match q.obj_kind {
            ok::KILL if q.obj_target == ALVO_QUALQUER_CHEFE => 20,
            ok::KILL | ok::PVP_KILL | ok::GATHER | ok::COLLECT => q.obj_count.max(1),
            ok::DUNGEON | ok::RAID => 60,
            _ => 10,
        } as f64;
        let nivel = q.min_level.max(1);
        return (BONUS_DO_CONTRATO as f64 * abates * xp_do_abate(BASE_TIPICA, nivel) as f64).round() as u64;
    }
    let (a, b) = faixa(q.min_level);
    let soma: u64 = QUESTS
        .iter()
        .filter(|d| !d.repeatable && !d.daily && !crate::historia::e_da_historia(d.id))
        // Only what a player can get: a mission with no island has no giver
        // in the world (`zona_da_missao`), and its share would go nowhere.
        .filter(|d| crate::quests::zona_da_missao(d.id).is_some())
        .filter(|d| faixa(d.min_level) == (a, b))
        .map(|d| d.reward_xp)
        .sum();
    let verba = PARCELA_DAS_UNICAS as f64 * xp_entre(a.max(1), b) as f64;
    (verba * q.reward_xp as f64 / soma.max(1) as f64).round() as u64
}

/// The story's XP per step: between two consecutive level gates, the
/// story's share of the XP from the first gate to the second, split by the
/// written weights. Before the first gate, the written XP. Steps after the
/// cap's gate pay nothing (no level to gain there).
fn da_historia() -> &'static HashMap<u16, u64> {
    static M: OnceLock<HashMap<u16, u64>> = OnceLock::new();
    M.get_or_init(|| {
        let mut m = HashMap::new();
        let mut trecho: Vec<(u16, u64)> = Vec::new();
        let mut desde = 1u32;
        let fecha = |trecho: &mut Vec<(u16, u64)>, de: u32, ate: u32, m: &mut HashMap<u16, u64>| {
            let soma: u64 = trecho.iter().map(|t| t.1).sum();
            let verba = PARCELA_DA_HISTORIA as f64 * xp_entre(de, ate.min(TETO)) as f64;
            for (id, w) in trecho.drain(..) {
                m.insert(id, (verba * w as f64 / soma.max(1) as f64).round() as u64);
            }
        };
        let mut i = 0u32;
        let mut antes_da_primeira_trava = true;
        while desde < TETO {
            let Some(id) = crate::historia::id_do_passo(i) else { break };
            let Some(q) = crate::historia::def_da_historia(id) else { break };
            i += 1;
            // Chapters I and II have no level gate: the player finishes them
            // as a tutorial, well before 20. The owner kept them as written
            // (04/10/2026) — the rule starts at the first gate.
            if antes_da_primeira_trava && q.obj_kind != ok::NIVEL {
                m.insert(q.id, q.reward_xp);
                continue;
            }
            if q.obj_kind == ok::NIVEL && antes_da_primeira_trava {
                antes_da_primeira_trava = false;
                desde = q.obj_count.max(1);
                continue;
            }
            if q.obj_kind == ok::NIVEL {
                let ate = q.obj_count.max(desde);
                fecha(&mut trecho, desde, ate, &mut m);
                desde = ate;
            } else if q.reward_xp > 0 {
                trecho.push((q.id, q.reward_xp));
            }
        }
        fecha(&mut trecho, desde, TETO, &mut m);
        m
    })
}

#[cfg(test)]
mod testes_do_fator_de_vida {
    use super::*;

    #[test]
    fn tanque_paga_a_vida_fragil_fica_igual_e_nunca_cai() {
        // Archer 60 XP / 105 HP already pays the fair rate: unchanged.
        assert_eq!(xp_pela_vida(105, 60), 61);
        // Wolf 30/120 -> 70 (0.58 x 120); bear 75/280 -> 162; owlbear 110/460 -> 267.
        assert_eq!(xp_pela_vida(120, 30), 70);
        assert_eq!(xp_pela_vida(280, 75), 162);
        assert_eq!(xp_pela_vida(460, 110), 267);
        // The cap: a 1 XP brute with huge HP gets at most 3x.
        assert_eq!(xp_pela_vida(10_000, 10), 30);
        // A generous species is never lowered.
        assert_eq!(xp_pela_vida(50, 100), 100);
        assert_eq!(fator_de_vida(0, 10), 1.0);
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn o_fator_do_mob_existe_em_todo_nivel() {
        for n in 1..=TETO {
            assert!(fator_do_mob(n) > 0.0, "level {n}");
        }
        assert_eq!(fator_do_mob(500), FATOR_DO_MOB[120]);
    }

    /// The owner's rule, measured on the data: in every gated stretch the
    /// story pays its share, and nothing more.
    #[test]
    fn a_historia_paga_a_parcela_de_cada_trecho() {
        let mut total = 0u64;
        let mut inicio = 0u64;
        let mut antes = true;
        for q in crate::historia::PASSOS {
            if q.obj_kind == ok::NIVEL {
                if antes {
                    inicio = xp_for_level_with_mult(q.obj_count, DEFAULT_XP_MULTIPLIER);
                }
                antes = false;
            } else if !antes {
                total += xp_da_quest(q);
            }
        }
        let teto = (xp_for_level_with_mult(TETO, DEFAULT_XP_MULTIPLIER) - inicio) as f64;
        let parcela = total as f64 / teto;
        assert!(parcela <= PARCELA_DA_HISTORIA as f64 + 0.005, "story pays {parcela:.3} of 1-120");
        assert!(parcela >= PARCELA_DA_HISTORIA as f64 * 0.8, "story pays only {parcela:.3} of 1-120");
    }

    #[test]
    fn contrato_nunca_paga_mais_que_os_abates() {
        for q in QUESTS.iter().filter(|q| q.repeatable && !q.daily && q.obj_kind == ok::KILL && q.obj_target == 0) {
            let abates = q.obj_count as u64 * xp_do_abate(BASE_TIPICA, q.min_level.max(1));
            assert!(xp_da_quest(q) * 5 < abates, "{} '{}' pays {} for kills worth {abates}", q.id, q.title, xp_da_quest(q));
        }
    }
}

#[cfg(test)]
mod testes_da_escala {
    use super::*;

    /// The owner's bands (15/11/2026) are today's island edges renumbered.
    #[test]
    fn ilhas_nas_faixas_novas_e_100_mais() {
        assert_eq!(nivel_exibido_de(1), 1);
        assert_eq!(nivel_exibido_de(15), 30, "Bosque ends at 30");
        assert_eq!(nivel_exibido_de(30), 50, "Glacier ends at 50");
        assert_eq!(nivel_exibido_de(42), 70, "Desert ends at 70");
        assert_eq!(nivel_exibido_de(60), 100, "Plateau ends at 100");
        assert_eq!(texto_de_nivel(60), "100");
        assert_eq!(texto_de_nivel(61), "100+");
        assert_eq!(texto_de_nivel(118), "100+");
        assert_eq!(texto_de_faixa(40, 60), "66–100");
        assert_eq!(texto_de_faixa(55, 70), "91–100+");
        assert_eq!(texto_de_faixa(65, 80), "100+");
        for n in 1..=100u32 {
            let i = interno_f(n as f32);
            assert!((exibido_f(i) - n as f32).abs() < 1e-3, "{n}");
        }
    }

    /// The shown level climbs with the XP, one step at a time, never past 100,
    /// and the XP bar's bounds bracket the character's XP.
    #[test]
    fn nivel_exibido_sobe_com_o_xp_e_para_no_100() {
        let m = DEFAULT_XP_MULTIPLIER;
        assert_eq!(nivel_exibido(0, m), 1);
        let fim = crate::constants::xp_for_level_with_mult(NIVEL_MAX, m);
        for n in 2..NIVEL_MAX_EXIBIDO {
            let ini = xp_do_nivel_exibido(n, m);
            assert!(ini > xp_do_nivel_exibido(n - 1, m), "{n}: the bar has length");
            assert_eq!(nivel_exibido(ini + 1, m), n, "start of {n}");
            assert_eq!(nivel_exibido(ini.saturating_sub(1), m), n - 1, "just before {n}");
        }
        assert_eq!(nivel_exibido(fim, m), NIVEL_MAX_EXIBIDO);
        assert_eq!(nivel_exibido(fim * 4, m), NIVEL_MAX_EXIBIDO);
    }
}

#[cfg(test)]
mod testes_do_ritmo {
    use super::*;

    #[test]
    fn ritmo_comeca_em_um_sobe_em_rampa_e_para_no_maximo() {
        assert_eq!(ritmo(1), 1.0);
        assert_eq!(ritmo(RITMO_DE - 1), 1.0);
        assert_eq!(ritmo(RITMO_DE), 1.0);
        for n in RITMO_DE..RITMO_ATE {
            assert!(ritmo(n + 1) > ritmo(n), "{n}");
        }
        assert_eq!(ritmo(RITMO_ATE), RITMO_MAX);
        assert_eq!(ritmo(TETO), RITMO_MAX);
        assert_eq!(no_ritmo(0, 60), 0);
        assert_eq!(no_ritmo(1, 60), 1, "never rounds a gain away");
        assert_eq!(no_ritmo(2_000, 60), 1_600);
    }
}
