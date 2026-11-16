//! PvP rules in one place: the PK modes, who may hit whom, who pays PK
//! points for a kill, open-PvP areas, the penalties of a red name (heavier
//! XP loss on death, decay, gold pardon) and the PvP damage balance.
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
    /// The sheet's power (`dungeon::poder_de_stats`); 0 = unknown (no
    /// power protection). Stage A of the reset plan (docs/RESET.md): PvP is
    /// protected by POWER, not level.
    pub poder: i32,
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
    // A much stronger player does not start a fight with a much weaker one
    // (a reset character in level-100 gear on a starter island), unless the
    // target chose PK or has a red name (`protegido_por_poder`).
    if protegido_por_poder(att, alvo) {
        return false;
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

/// May `att` hit `alvo` AT SEA (naval phase 4)? The PK modes as on land,
/// plus THE MARK: a ship carrying treasure (`navio::marcado`) can be attacked
/// by anyone — a Peaceful attacker included, and past the power protection
/// — except by its own party and clan. The harbour corridor is checked by
/// the caller, as every safe zone.
pub fn pode_atacar_no_mar(att: &Lado, alvo: &Lado, revide: bool, alvo_marcado: bool) -> bool {
    if !alvo_marcado {
        return pode_atacar(att, alvo, revide);
    }
    // A marked target reads as one in a PK mode: free for everyone, and
    // not protected by power (`protegido_por_poder` spares PK targets).
    let modo = if hostil(alvo.modo) { alvo.modo } else { GRUPO };
    pode_atacar(att, &Lado { modo, ..*alvo }, revide)
}

/// PK points for sinking or killing someone AT SEA: as on land, and none
/// for a MARKED carrier (carrying treasure is consenting to the fight) nor
/// in lawless waters (`area_aberta`).
pub fn pontos_por_abate_no_mar(vitima: &Lado, revide: bool, area_aberta: bool, marcado: bool) -> u32 {
    pontos_por_abate(vitima, revide, area_aberta || marcado)
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
///
/// At sea (naval phase 4) the LAWLESS WATERS (`mar::sem_lei`) are the same
/// kind of area: forced mode, no PK points.
pub fn zona_pvp_aberto(zona: &str, pos: Vec2) -> bool {
    (crate::magica::e_magica(zona) && !crate::magica::e_porto_seguro(pos))
        || (crate::mar::e_mar(zona) && crate::mar::sem_lei(pos))
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

// ─────────────────────────────── PvP by power ───────────────────────────────

/// A target with less than this share of the attacker's power cannot be
/// attacked first (owner, 14/11/2026: protect by power and "não deixar").
pub const PVP_PROTEGE_ABAIXO: f32 = 0.50;
/// The target may have up to this many times the attacker's power with no
/// damage penalty (the old "5 levels free").
pub const PVP_PODER_LIVRE: f32 = 1.25;
/// Damage lost per each 10% of extra power past `PVP_PODER_LIVRE`.
pub const PVP_PODER_PENALIDADE: f32 = 0.08;

/// Is `alvo` off-limits for `att` because it is far weaker? Not when the
/// target is in a PK mode or red, and not when either power is unknown.
pub fn protegido_por_poder(att: &Lado, alvo: &Lado) -> bool {
    att.poder > 0
        && alvo.poder > 0
        && !alvo.alvo_livre()
        && (alvo.poder as f32) < att.poder as f32 * PVP_PROTEGE_ABAIXO
}

/// Power-gap multiplier: only when the attacker is the WEAKER side, past
/// `PVP_PODER_LIVRE`; floor `PVP_NIVEL_PISO`. Unknown power (0) = no penalty.
pub fn pvp_mult_poder(atacante: i32, alvo: i32) -> f32 {
    if atacante <= 0 || alvo <= 0 {
        return 1.0;
    }
    let razao = alvo as f32 / atacante as f32;
    if razao <= PVP_PODER_LIVRE {
        return 1.0;
    }
    let passos = (razao - PVP_PODER_LIVRE) / 0.10;
    (1.0 - PVP_PODER_PENALIDADE * passos).clamp(PVP_NIVEL_PISO, 1.0)
}

// ─────────────────────────────── PvP damage ───────────────────────────────

/// Multiplier on a mitigated SKILL hit (skills, skill areas, skill bombs) between players.
pub const PVP_MULT_SKILL: f32 = 0.10;
/// Multiplier on a mitigated BASIC attack between players.
pub const PVP_MULT_BASICO: f32 = 0.15;
/// Levels the attacker may be below the target with no penalty.
pub const PVP_NIVEL_LIVRE: u32 = 5;
/// Damage lost per level past `PVP_NIVEL_LIVRE`.
pub const PVP_NIVEL_PENALIDADE: f32 = 0.08;
/// The level penalty never goes under this.
pub const PVP_NIVEL_PISO: f32 = 0.20;
/// Most of the target's max HP one hit may take (`None` = no cap).
pub const PVP_TETO: Option<f32> = None;
/// Katana execute (under 30% HP) against players.
pub const PVP_KATANA_EXECUTA: bool = false;

/// Level-gap multiplier: only when the attacker is BELOW the target.
pub fn pvp_mult_nivel(atacante: u32, alvo: u32, livre: u32, por_nivel: f32, piso: f32) -> f32 {
    let gap = alvo.saturating_sub(atacante);
    if gap <= livre || por_nivel <= 0.0 {
        return 1.0;
    }
    (1.0 - por_nivel * (gap - livre) as f32).max(piso).min(1.0)
}

/// A mitigated player-vs-player hit, with the live rules. `atacante` and
/// `alvo` are POWERS (since 14/11/2026; it was levels).
pub fn pvp_dano(mitigado: i32, skill: bool, atacante: i32, alvo: i32) -> i32 {
    let m = if skill { PVP_MULT_SKILL } else { PVP_MULT_BASICO } * pvp_mult_poder(atacante, alvo);
    if m == 1.0 {
        mitigado
    } else {
        ((mitigado as f32) * m).round().max(1.0) as i32
    }
}

// ─────────────────────────────── leaving with PK ───────────────────────────────
//
// Owner, 16/11/2026: "a character with PK has to stay logged in for at least
// 2 minutes, standing still, even after the person left — otherwise it is
// too easy to kill everyone, log out and that's it".

/// Seconds the body stays in the world, standing still and attackable,
/// after a character WITH PK leaves (closed the game, dropped, logged out).
pub const PERMANENCIA_S: f32 = 120.0;

/// Seconds a PvP hit (given or taken) keeps a player "in PK combat": logging
/// out lingers, and voluntary travel to another zone is refused.
pub const COMBATE_PK_S: f32 = 30.0;

/// Does leaving now leave the body behind for `PERMANENCIA_S`? A red name
/// (PK points) or a PvP fight in the last `COMBATE_PK_S`.
pub fn permanece_ao_sair(pontos: u32, em_combate: bool) -> bool {
    pontos > 0 || em_combate
}

/// The warning shown before leaving the game with PK.
pub const AVISO_DE_SAIDA: &str =
    "You have PK: your character stays in the world for 2 minutes after you leave and can still be attacked.";

/// Most names on a character's blacklist.
pub const MAX_LISTA_NEGRA: usize = 50;

/// Most kill letters kept per character (older ones are deleted first).
pub const MAX_CARTAS_DE_ABATE: usize = 20;

/// Brasília is UTC−3 all year (no daylight saving since 2019).
const BRASILIA_S: i64 = -3 * 3600;

/// "2026-11-16 21:04", Brasília time, for a unix timestamp.
pub fn data_brasilia(unix: i64) -> String {
    let t = unix + BRASILIA_S;
    let dias = t.div_euclid(86_400);
    let s = t.rem_euclid(86_400);
    // Civil date from days since 1970-01-01 (Howard Hinnant's algorithm).
    let z = dias + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!("{y:04}-{m:02}-{d:02} {:02}:{:02}", s / 3600, s / 60 % 60)
}

/// Where a kill happened, in words: the island (or "the open sea near X")
/// and the spot's coordinates, as the minimap shows them.
pub fn lugar(zona: &str, pos: Vec2) -> String {
    let onde = if crate::mar::e_mar(zona) {
        match crate::mar::ilhas()
            .iter()
            .min_by(|a, b| a.centro.distance(pos).total_cmp(&b.centro.distance(pos)))
        {
            Some(v) => format!("the open sea near {}", v.nome),
            None => "the open sea".to_string(),
        }
    } else {
        crate::terreno::def_da_zona(zona).map_or_else(|| zona.to_string(), |d| d.nome.to_string())
    };
    format!("{onde} ({:.0}, {:.0})", pos.x, pos.y)
}

/// The kill letter: (subject, text). `quem` is the killer's name, `renasc`
/// their rebirths, `nivel` the shown level; `matador_pk` = the killer has a
/// red name after the kill; `area_aberta` = it happened in an open-PvP area
/// (Magic Island combat islets, lawless waters).
///
/// One sentence per paragraph, so each one is a dictionary entry of its own.
pub fn carta_de_abate(
    quem: &str,
    renasc: u32,
    nivel: u32,
    onde: &str,
    quando_unix: i64,
    matador_pk: bool,
    area_aberta: bool,
) -> (String, String) {
    let assunto = format!("You were killed by {quem}");
    let ficha = format!("R{renasc} · Lv {}", crate::progressao::texto_de_nivel(nivel));
    let mut texto = format!(
        "{quem} [{ficha}] killed you at {onde} on {} (Brasília time).",
        data_brasilia(quando_unix)
    );
    if matador_pk {
        texto.push_str("\n\nThe killer has PK (red name): anyone can attack them without penalty.");
    }
    if area_aberta {
        texto.push_str("\n\nIt happened in an open PvP area, where kills give no PK points.");
    }
    texto.push_str(
        "\n\nAdd them to your blacklist to see where they are while they are online with PK.",
    );
    (assunto, texto)
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn sair_com_pk_e_a_carta_de_abate() {
        assert!(!permanece_ao_sair(0, false), "no PK: leaves at once");
        assert!(permanece_ao_sair(1, false), "red name");
        assert!(permanece_ao_sair(0, true), "PK combat");
        assert!(PERMANENCIA_S >= 120.0);
        // 2026-11-17 00:04 UTC = 2026-11-16 21:04 in Brasília.
        assert_eq!(data_brasilia(1_794_873_840), "2026-11-16 21:04");
        assert_eq!(data_brasilia(0), "1969-12-31 21:00");
        assert_eq!(data_brasilia(951_782_400 + 3 * 3600), "2000-02-29 00:00", "leap day");
        let (a, t) = carta_de_abate("Brunji", 1, 40, "Sunny Grove (10, 20)", 1_794_873_840, true, false);
        assert_eq!(a, "You were killed by Brunji");
        let esperado = format!(
            "Brunji [R1 · Lv {}] killed you at Sunny Grove (10, 20) on 2026-11-16 21:04 (Brasília time).",
            crate::progressao::texto_de_nivel(40)
        );
        assert!(t.starts_with(&esperado), "{t}");
        assert!(t.contains("The killer has PK") && !t.contains("open PvP area"));
        let (_, t) = carta_de_abate("X", 0, 3, "here", 0, false, true);
        assert!(!t.contains("The killer has PK") && t.contains("open PvP area"));
        assert!(crate::social::corpo_de_carta_valido(&t, 1, 1000));
        let l = lugar("ilha_inicial", Vec2::new(10.4, -20.6));
        assert!(l.ends_with("(10, -21)") && !l.starts_with("ilha_inicial"), "{l}");
        assert!(lugar(crate::mar::ZONA, Vec2::ZERO).starts_with("the open sea"));
    }

    fn lado(modo: u8, grupo: Option<u32>, cla: i64) -> Lado {
        Lado { modo, pontos: 0, grupo, cla, poder: 0 }
    }

    /// THE MARK at sea: a Peaceful carrier is fair game for a Peaceful
    /// attacker, even a much stronger one, but not for its own party or
    /// clan; unmarked, nothing changes. Sinking a marked carrier or anyone
    /// in lawless waters gives no PK points; an unmarked Peaceful one does.
    #[test]
    fn a_marca_do_mar() {
        let paz = lado(PACIFICO, None, 0);
        assert!(!pode_atacar_no_mar(&paz, &paz, false, false));
        assert!(pode_atacar_no_mar(&paz, &paz, false, true));
        let forte = Lado { poder: 10_000, ..paz };
        let fraco = Lado { poder: 1_000, ..paz };
        assert!(!pode_atacar_no_mar(&forte, &fraco, false, false), "power protection off the mark");
        assert!(pode_atacar_no_mar(&forte, &fraco, false, true), "the mark beats power protection");
        let amigo = lado(PACIFICO, Some(3), 9);
        assert!(!pode_atacar_no_mar(&amigo, &lado(PACIFICO, Some(3), 0), false, true), "own party");
        assert!(!pode_atacar_no_mar(&amigo, &lado(PACIFICO, None, 9), false, true), "own clan");
        assert_eq!(pontos_por_abate_no_mar(&paz, false, false, false), 1);
        assert_eq!(pontos_por_abate_no_mar(&paz, false, false, true), 0);
        assert_eq!(pontos_por_abate_no_mar(&paz, false, true, false), 0);
        // The lawless waters are an open-PvP area; the corridor never is.
        let z = crate::mar::zonas_sem_lei()[0];
        assert!(zona_pvp_aberto(crate::mar::ZONA, z));
        assert!(!zona_pvp_aberto("ilha_gelo", z));
        let cais = crate::mar::ilhas()[0].fundeadouro();
        assert!(!zona_pvp_aberto(crate::mar::ZONA, cais));
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

    #[test]
    fn dano_pvp_multiplica_e_pena_por_poder() {
        assert_eq!(pvp_dano(1000, true, 5_000, 5_000), 100);
        assert_eq!(pvp_dano(1000, false, 5_000, 5_000), 150);
        assert_eq!(pvp_dano(3, true, 5_000, 5_000), 1, "floor of 1");
        // Target up to 1.25x: free.
        assert_eq!(pvp_dano(1000, true, 4_000, 5_000), 100);
        // 1.75x: 5 steps of 10% past the free share, 5 x 8% = 60%.
        assert_eq!(pvp_dano(1000, true, 4_000, 7_000), 60);
        // Far stronger target sits on the floor.
        assert_eq!(pvp_dano(1000, true, 2_000, 12_000), 20);
        // Stronger attacker: no bonus, no penalty; unknown power: none.
        assert_eq!(pvp_dano(1000, true, 12_000, 2_000), 100);
        assert_eq!(pvp_dano(1000, true, 0, 9_000), 100);
    }

    /// A much stronger player cannot start on one under half his power,
    /// unless the target chose PK or is red; hitting back always works.
    #[test]
    fn forte_nao_caca_quem_tem_menos_da_metade_do_poder() {
        let forte = Lado { poder: 12_000, ..lado(LIVRE, None, 0) };
        let fraco = Lado { poder: 5_000, ..lado(PACIFICO, None, 0) };
        let parelho = Lado { poder: 7_000, ..lado(PACIFICO, None, 0) };
        assert!(!pode_atacar(&forte, &fraco, false), "under half: protected");
        assert!(pode_atacar(&forte, &parelho, false));
        assert!(pode_atacar(&forte, &fraco, true), "hitting back");
        assert!(pode_atacar(&forte, &Lado { modo: LIVRE, ..fraco }, false), "the weak one chose PK");
        assert!(pode_atacar(&forte, &Lado { pontos: 1, ..fraco }, false), "red name");
        assert!(pode_atacar(&fraco, &forte, false) == pode_atacar(&lado(PACIFICO, None, 0), &lado(LIVRE, None, 0), false));
    }
}
