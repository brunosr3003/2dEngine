//! PvP rules in one place: the PK modes, who may hit whom, who pays PK
//! points for a kill, open-PvP areas, the penalties of a red name (heavier
//! XP loss on death, decay, gold pardon).
//!
//! Every number the owner may want to tune is a `pub const` here. The server
//! decides with these functions; the client uses the same ones to draw the
//! nameplate colour and explain the modes, so the two never disagree.

use glam::Vec2;

// ─────────────────────────────── modes ───────────────────────────────

/// Attacks no player (only hits back for `REVIDE_S` after being attacked).
pub const PACIFICO: u8 = 0;
/// Attacks anyone, own party and own clan included.
pub const LIVRE: u8 = 1;
/// Attacks everyone except own party and own clan.
pub const GRUPO: u8 = 2;
/// Attacks everyone except own clan (party members outside the clan can be hit).
pub const CLA: u8 = 3;

/// In the order the picker shows them.
pub const MODOS: [u8; 4] = [PACIFICO, GRUPO, CLA, LIVRE];

/// Seconds a player who was attacked may hit the attacker back, whatever their mode.
pub const REVIDE_S: f32 = 15.0;

pub fn modo_valido(m: u8) -> u8 {
    if m <= CLA {
        m
    } else {
        PACIFICO
    }
}

pub fn nome(m: u8) -> &'static str {
    match m {
        LIVRE => "Free",
        GRUPO => "Party",
        CLA => "Clan",
        _ => "Peaceful",
    }
}

pub fn explicacao(m: u8) -> &'static str {
    match m {
        LIVRE => "Attack anyone, including your party and clan.",
        GRUPO => "Attack everyone except your party and clan.",
        CLA => "Attack everyone except your clan.",
        _ => "Attack no one. Others in a PK mode can still attack you.",
    }
}

/// Any mode but Peaceful: the player takes part in PvP and may be attacked
/// by anyone without the attacker getting PK points.
pub fn hostil(m: u8) -> bool {
    modo_valido(m) != PACIFICO
}

/// One side of a PvP check.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Lado {
    pub modo: u8,
    pub pontos: u32,
    /// Party id; `None` = solo.
    pub grupo: Option<u32>,
    /// Clan id; 0 = no clan.
    pub cla: i64,
}

impl Lado {
    /// A red name: free to attack for everyone.
    pub fn vermelho(&self) -> bool {
        self.pontos > 0
    }
    /// May be attacked by anyone (Peaceful attackers included).
    pub fn alvo_livre(&self) -> bool {
        hostil(self.modo) || self.vermelho()
    }
}

/// May `att` hit `alvo`? Safe zones are checked by the caller (they win over
/// everything). `revide` = `alvo` attacked `att` in the last `REVIDE_S`:
/// hitting back is always allowed, whatever the mode.
///
/// The ATTACKER's mode decides. Free hits anyone. Party spares own party and
/// own clan; Clan spares own clan. Peaceful starts nothing, but may hit a
/// target that is in a PK mode or has a red name — except own party/clan.
pub fn pode_atacar(att: &Lado, alvo: &Lado, revide: bool) -> bool {
    if revide {
        return true;
    }
    let mesmo_grupo = att.grupo.is_some() && att.grupo == alvo.grupo;
    let mesmo_cla = att.cla != 0 && att.cla == alvo.cla;
    match modo_valido(att.modo) {
        LIVRE => true,
        GRUPO => !mesmo_grupo && !mesmo_cla,
        CLA => !mesmo_cla,
        _ => alvo.alvo_livre() && !mesmo_grupo && !mesmo_cla,
    }
}

/// PK points the killer gets. Only killing a Peaceful player with a clean
/// name, outside open PvP, when not hitting back, costs a point.
pub fn pontos_por_abate(vitima: &Lado, revide: bool, area_aberta: bool) -> u32 {
    if area_aberta || revide || vitima.alvo_livre() {
        0
    } else {
        1
    }
}

// ─────────────────────────────── open PvP ───────────────────────────────

/// Open-PvP area: the Magic Island's combat islets (every grade), not its
/// arrival harbour. Inside, the PK mode is forced to `MODO_NA_AREA_ABERTA`
/// and kills give no PK points.
pub fn zona_pvp_aberto(zona: &str, pos: Vec2) -> bool {
    crate::magica::e_magica(zona) && !crate::magica::e_porto_seguro(pos)
}

/// The conservative mode: never hits own party or clan.
pub const MODO_NA_AREA_ABERTA: u8 = GRUPO;

// ─────────────────────────────── penalties ───────────────────────────────

/// XP lost on death, as a share of the current level's step: the base...
pub const PERDA_BASE: f32 = 0.10;
/// ...plus this much per PK point...
pub const PERDA_POR_PONTO: f32 = 0.05;
/// ...never above this in total.
pub const PERDA_MAX: f32 = 0.50;

/// (base share, extra share from PK points) of the level step a death takes.
/// Only the base is recoverable.
pub fn fracao_perda(pontos: u32) -> (f32, f32) {
    let total = (PERDA_BASE + PERDA_POR_PONTO * pontos as f32).min(PERDA_MAX);
    (PERDA_BASE, (total - PERDA_BASE).max(0.0))
}

/// One PK point fades per this many seconds ONLINE (logout keeps the count).
pub const SEGUNDOS_POR_PONTO: u32 = 3600;

/// (points, accumulated seconds) after `dt` more seconds online. Time with no
/// points is not banked.
pub fn decair(pontos: u32, acumulado: u32, dt: u32) -> (u32, u32) {
    if pontos == 0 {
        return (0, 0);
    }
    let total = acumulado.saturating_add(dt);
    let saem = (total / SEGUNDOS_POR_PONTO).min(pontos);
    let pontos = pontos - saem;
    (pontos, if pontos == 0 { 0 } else { total - saem * SEGUNDOS_POR_PONTO })
}

/// Gold to pardon ONE PK point, per character level.
pub const PERDAO_OURO_POR_NIVEL: u64 = 1_000;

pub fn custo_perdao(nivel: u32) -> u64 {
    PERDAO_OURO_POR_NIVEL * nivel.max(1) as u64
}

#[cfg(test)]
mod testes {
    use super::*;

    fn lado(modo: u8, grupo: Option<u32>, cla: i64) -> Lado {
        Lado { modo, pontos: 0, grupo, cla }
    }

    /// Attacker mode × (same party, same clan, both, none), target Peaceful.
    #[test]
    fn matriz_dos_modos() {
        // (mode, same party, same clan, both, none)
        let casos = [
            (LIVRE, true, true, true, true),
            (GRUPO, false, false, false, true),
            (CLA, true, false, false, true),
            (PACIFICO, false, false, false, false),
        ];
        for (modo, grupo, cla, ambos, nada) in casos {
            let att = lado(modo, Some(7), 3);
            let alvo = |g: bool, c: bool| lado(PACIFICO, g.then_some(7), if c { 3 } else { 9 });
            assert_eq!(pode_atacar(&att, &alvo(true, false), false), grupo, "{} same party", nome(modo));
            assert_eq!(pode_atacar(&att, &alvo(false, true), false), cla, "{} same clan", nome(modo));
            assert_eq!(pode_atacar(&att, &alvo(true, true), false), ambos, "{} both", nome(modo));
            assert_eq!(pode_atacar(&att, &alvo(false, false), false), nada, "{} none", nome(modo));
        }
        // No clan / no party is never "the same".
        assert!(pode_atacar(&lado(CLA, None, 0), &lado(PACIFICO, None, 0), false));
        assert!(pode_atacar(&lado(GRUPO, None, 0), &lado(PACIFICO, None, 0), false));
    }

    #[test]
    fn revide_vale_em_qualquer_modo() {
        for modo in MODOS {
            let att = lado(modo, Some(1), 1);
            assert!(pode_atacar(&att, &lado(PACIFICO, Some(1), 1), true), "{}", nome(modo));
        }
    }

    #[test]
    fn pacifico_bate_em_hostil_e_vermelho_sem_pontos() {
        let paz = lado(PACIFICO, None, 0);
        assert!(!pode_atacar(&paz, &lado(PACIFICO, None, 0), false), "peaceful vs peaceful");
        for m in [LIVRE, GRUPO, CLA] {
            let alvo = lado(m, None, 0);
            assert!(pode_atacar(&paz, &alvo, false), "peaceful vs {}", nome(m));
            assert_eq!(pontos_por_abate(&alvo, false, false), 0);
        }
        let vermelho = Lado { pontos: 2, ..lado(PACIFICO, None, 0) };
        assert!(pode_atacar(&paz, &vermelho, false), "red name is free to hit");
        assert_eq!(pontos_por_abate(&vermelho, false, false), 0);
        // Own party/clan stays off-limits for a Peaceful attacker even when hostile.
        assert!(!pode_atacar(&lado(PACIFICO, Some(4), 0), &lado(LIVRE, Some(4), 0), false));
        assert!(!pode_atacar(&lado(CLA, None, 5), &Lado { pontos: 3, ..lado(LIVRE, None, 5) }, false));
        assert!(pode_atacar(&lado(LIVRE, None, 5), &lado(PACIFICO, None, 5), false));
        // The only kill that costs: a clean Peaceful victim, no revenge, outside open PvP.
        assert_eq!(pontos_por_abate(&lado(PACIFICO, None, 0), false, false), 1);
        assert_eq!(pontos_por_abate(&lado(PACIFICO, None, 0), true, false), 0);
        assert_eq!(pontos_por_abate(&lado(PACIFICO, None, 0), false, true), 0);
    }

    #[test]
    fn area_aberta_e_a_ilha_magica_fora_do_porto() {
        let combate = crate::magica::centros_de_combate()[0];
        let porto = crate::magica::posto_de_pocoes();
        for z in ["ilha_magica", "ilha_magica_2", "ilha_magica_3"] {
            assert!(zona_pvp_aberto(z, combate), "{z}");
            assert!(!zona_pvp_aberto(z, porto), "{z} harbour is safe");
        }
        assert!(!zona_pvp_aberto("ilha_inicial", combate));
    }

    #[test]
    fn perda_cresce_com_pontos_ate_o_teto() {
        assert_eq!(fracao_perda(0), (0.10, 0.0));
        let (b, e) = fracao_perda(1);
        assert!((b - 0.10).abs() < 1e-6 && (e - 0.05).abs() < 1e-6);
        let (_, e) = fracao_perda(4);
        assert!((e - 0.20).abs() < 1e-6);
        for p in [8, 9, 100, u32::MAX] {
            let (b, e) = fracao_perda(p);
            assert!((b + e - PERDA_MAX).abs() < 1e-6, "{p}");
        }
    }

    #[test]
    fn pontos_decaem_com_tempo_online() {
        assert_eq!(decair(0, 500, 3600), (0, 0), "no banking without points");
        assert_eq!(decair(3, 0, 1800), (3, 1800));
        assert_eq!(decair(3, 1800, 1800), (2, 0));
        assert_eq!(decair(3, 3599, 2), (2, 1));
        assert_eq!(decair(2, 0, 3 * 3600 + 5), (0, 0), "never below zero");
        assert_eq!(custo_perdao(40), 40_000);
        assert_eq!(custo_perdao(0), 1_000);
    }
}
