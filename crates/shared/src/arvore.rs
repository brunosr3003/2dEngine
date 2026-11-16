//! THE PATH TREE ("árvore de caminhos", docs/RESET.md, rebirth phases 2–4).
//!
//! From the first rebirth on, every 10 SHOWN levels of a climb earn one
//! point (`pontos_ganhos`): 10 a climb, so a character with `r` rebirths at
//! shown level `n` has `10·(r−1) + floor(n/10)` points. The points are
//! DERIVED, never stored; what is stored is the allocation (`Arvore`, the SET
//! of bought node ids: `characters.arvore_json`).
//!
//! THE WEB (redesign of 15/11/2026, owner: "a árvore de passiva pode ser
//! mais extensa", he had maxed the old 4 × 15 lines by rebirth 7). A radial
//! web around a free START node (`INICIO`): each of the four paths owns one
//! diagonal arm (`ANGULO_DO_CAMINHO`) built from the same 41-node template
//! (`MOLDE`), and BRIDGES (`PONTES`) join neighbouring arms at their edges so
//! hybrid builds are possible. 173 nodes (`NOS_TOTAL`):
//!
//! - **minor** (`TipoNo::Menor`): one small stat;
//! - **notable** (`TipoNo::Notavel`): named, two effects, bigger;
//! - **keystone** (`TipoNo::Chave`): named, a big bonus with a real cost
//!   (negative values: "+30% crit damage, −15% max health");
//! - **active** (`TipoNo::Ativa`): one of the 12 actives (`ATIVAS`), learned
//!   when the node is bought, plus a small passive.
//!
//! A node can be bought only next to one already bought (or to the start),
//! and costs more the farther out it sits (`No::custo`: ring 1/2/3, +1 for a
//! notable or an active, 6 for a keystone). The whole web costs
//! `custo_total()` = 460 points, about rebirth 47.
//!
//! NODE IDS ARE PERSISTED (`arvore_json`) and the table is built by code
//! (`tabela`): never reorder `MOLDE`, `PONTES` or `CONTEUDO`; only append.
//!
//! Combat passives (`Passivas`) apply in the server's `effective_stats_rv`
//! (crit damage in `Session::extra_de_dano_crit`); economy ones (drop, ore,
//! Energy, Darksteel, gold, XP, better loot) add to the accessories'
//! `acessorios::Bonus` (`Passivas::somar_economia`).
//!
//! Respec (`custo_de_redistribuir`) refunds every point for gold.

use serde::{Deserialize, Serialize};

/// How many paths.
pub const CAMINHOS: usize = 4;
/// Shown levels per point.
pub const NIVEIS_POR_PONTO: u32 = 10;
/// Gold per allocated point to reset the tree (1,000 × points).
pub const OURO_POR_PONTO_REDISTRIBUIR: u64 = 1_000;

/// The four paths, in the actives' (`ATIVAS`) order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Caminho {
    Assassino = 0,
    Mago = 1,
    Guardiao = 2,
    Corsario = 3,
}

impl Caminho {
    pub const TODOS: [Caminho; CAMINHOS] =
        [Caminho::Assassino, Caminho::Mago, Caminho::Guardiao, Caminho::Corsario];

    pub fn de_indice(i: u8) -> Option<Caminho> {
        Self::TODOS.get(i as usize).copied()
    }

    /// English (the client translates).
    pub fn nome(self) -> &'static str {
        match self {
            Caminho::Assassino => "Assassin",
            Caminho::Mago => "Mage",
            Caminho::Guardiao => "Guardian",
            Caminho::Corsario => "Corsair",
        }
    }

    /// One line under the path's name.
    pub fn resumo(self) -> &'static str {
        match self {
            Caminho::Assassino => "Critical hits, speed and dexterity",
            Caminho::Mago => "Wisdom, mana and attack power",
            Caminho::Guardiao => "Defense, health and damage reduction",
            Caminho::Corsario => "Loot, gold and gathering",
        }
    }
}

/// What a node gives. The value is in the unit the effect names: fractions
/// for percentages (0.02 = +2%), points per second for regen, whole points
/// for poise. A NEGATIVE value is a keystone's cost.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Efeito {
    /// Crit chance, absolute (+0.01 = +1 percentage point).
    Critico,
    /// Attack speed (`attack_speed_mult`).
    VelocidadeDeAtaque,
    /// Dexterity, % of the final value.
    DestrezaPct,
    /// Movement speed (`speed_mult`).
    Movimento,
    /// Dash cooldown (`dash_cd_mult`: the cooldown is divided by it).
    Esquiva,
    /// Wisdom, % of the final value.
    SabedoriaPct,
    /// Max mana, %.
    ManaPct,
    /// Mana regen, per second.
    RegenMana,
    /// Attack, %.
    AtaquePct,
    /// Defense, %.
    DefesaPct,
    /// Max health, %.
    VidaPct,
    /// Damage reduction, absolute (`damage_reduction_pct`).
    Reducao,
    /// Health regen, per second.
    RegenVida,
    /// Poise (absorbs hits before health, no stagger while it lasts).
    Firmeza,
    /// Drop chance (`Bonus::drop`).
    Drop,
    /// Gold from kills (`Bonus::ouro`).
    Ouro,
    /// Ore from rocks (`Bonus::minerio`).
    Minerio,
    /// Energy from veins (`Bonus::energia`).
    Energia,
    /// Darksteel from rocks (`Bonus::darksteel`).
    Darksteel,
    /// Chance of a dropped item one color up (`Bonus::loot_melhor`).
    LootMelhor,
    /// Critical damage, over `CRIT_DAMAGE_MULT` (the same extra Lethal Fury
    /// adds: `golpe_critico_com`). Server-side only.
    DanoCritico,
    /// XP from monsters (`Bonus::xp`).
    Xp,
}

impl Efeito {
    /// The node's line, in English with one `{}` hole for the number (the
    /// client translates the template, then fills it).
    pub fn modelo(self) -> &'static str {
        match self {
            Efeito::Critico => "Crit chance +{}%",
            Efeito::VelocidadeDeAtaque => "Attack speed +{}%",
            Efeito::DestrezaPct => "Dexterity +{}%",
            Efeito::Movimento => "Movement speed +{}%",
            Efeito::Esquiva => "Dash cooldown -{}%",
            Efeito::SabedoriaPct => "Wisdom +{}%",
            Efeito::ManaPct => "Max mana +{}%",
            Efeito::RegenMana => "Mana regen +{}/s",
            Efeito::AtaquePct => "Attack +{}%",
            Efeito::DefesaPct => "Defense +{}%",
            Efeito::VidaPct => "Max health +{}%",
            Efeito::Reducao => "Damage taken -{}%",
            Efeito::RegenVida => "Health regen +{}/s",
            Efeito::Firmeza => "Poise +{}",
            Efeito::Drop => "Drop chance +{}%",
            Efeito::Ouro => "Gold from monsters +{}%",
            Efeito::Minerio => "Ore gathering +{}%",
            Efeito::Energia => "Energy gathering +{}%",
            Efeito::Darksteel => "Darksteel gathering +{}%",
            Efeito::LootMelhor => "Better loot chance +{}%",
            Efeito::DanoCritico => "Crit damage +{}%",
            Efeito::Xp => "XP from monsters +{}%",
        }
    }

    /// The same line for a NEGATIVE value (a keystone's cost), with the
    /// number shown positive.
    pub fn modelo_negativo(self) -> &'static str {
        match self {
            Efeito::Critico => "Crit chance -{}%",
            Efeito::VelocidadeDeAtaque => "Attack speed -{}%",
            Efeito::DestrezaPct => "Dexterity -{}%",
            Efeito::Movimento => "Movement speed -{}%",
            Efeito::Esquiva => "Dash cooldown +{}%",
            Efeito::SabedoriaPct => "Wisdom -{}%",
            Efeito::ManaPct => "Max mana -{}%",
            Efeito::RegenMana => "Mana regen -{}/s",
            Efeito::AtaquePct => "Attack -{}%",
            Efeito::DefesaPct => "Defense -{}%",
            Efeito::VidaPct => "Max health -{}%",
            Efeito::Reducao => "Damage taken +{}%",
            Efeito::RegenVida => "Health regen -{}/s",
            Efeito::Firmeza => "Poise -{}",
            Efeito::Drop => "Drop chance -{}%",
            Efeito::Ouro => "Gold from monsters -{}%",
            Efeito::Minerio => "Ore gathering -{}%",
            Efeito::Energia => "Energy gathering -{}%",
            Efeito::Darksteel => "Darksteel gathering -{}%",
            Efeito::LootMelhor => "Better loot chance -{}%",
            Efeito::DanoCritico => "Crit damage -{}%",
            Efeito::Xp => "XP from monsters -{}%",
        }
    }

    /// Is the value shown ×100 (a fraction) or as is (per second, points)?
    fn em_pct(self) -> bool {
        !matches!(self, Efeito::RegenMana | Efeito::RegenVida | Efeito::Firmeza)
    }

    /// The number for `modelo`'s hole ("2", "0.5", "7.5"), always positive.
    pub fn numero(self, valor: f32) -> String {
        let v = if self.em_pct() { valor.abs() * 100.0 } else { valor.abs() };
        let r = (v * 10.0).round() / 10.0;
        if (r - r.round()).abs() < 1e-4 {
            format!("{}", r.round() as i64)
        } else {
            format!("{r:.1}")
        }
    }

    /// The English template for `valor`'s sign (the client translates it).
    pub fn modelo_de(self, valor: f32) -> &'static str {
        if valor < 0.0 {
            self.modelo_negativo()
        } else {
            self.modelo()
        }
    }

    /// "Crit chance +1%", "Max health -15%": the English line with the
    /// number in its hole.
    pub fn linha(self, valor: f32) -> String {
        self.modelo_de(valor).replace("{}", &self.numero(valor))
    }

    /// Is this a GOOD change for the player (green) or a cost (red)?
    pub fn bom(self, valor: f32) -> bool {
        valor >= 0.0
    }
}

// ── The web ─────────────────────────────────────────────────────────────

/// A node's kind.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TipoNo {
    /// The free centre every path starts from (always owned, gives nothing).
    Inicio,
    /// A small stat.
    Menor,
    /// Named, a bigger effect.
    Notavel,
    /// KEYSTONE: named, a big bonus with a real cost.
    Chave,
    /// One of the 12 actives (`ATIVAS`), plus a small passive.
    Ativa,
}

/// Effects a node can carry at most.
pub const MAX_EFEITOS: usize = 3;

/// One node of the web.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct No {
    pub id: u16,
    pub tipo: TipoNo,
    /// The arm it belongs to; `None` for the start and the bridges.
    pub caminho: Option<Caminho>,
    /// A bridge's two paths (`PONTES`).
    pub ponte: Option<(Caminho, Caminho)>,
    /// Position in tree units (the start is at the origin, y down).
    pub x: f32,
    pub y: f32,
    /// 0 = the start, then 1..=3 outward.
    pub anel: u8,
    /// Points to buy it.
    pub custo: u8,
    /// English name of a notable, keystone or active ("" for a minor).
    pub nome: &'static str,
    efeitos: [(Efeito, f32); MAX_EFEITOS],
    n_efeitos: u8,
    /// The ACTIVE this node teaches (its id in `ATIVAS`).
    pub ativa: Option<u8>,
}

impl No {
    /// What the node gives.
    pub fn efeitos(&self) -> &[(Efeito, f32)] {
        &self.efeitos[..self.n_efeitos as usize]
    }

    /// The node's English lines ("Crit chance +1%", …).
    pub fn linhas(&self) -> Vec<String> {
        self.efeitos().iter().map(|&(e, v)| e.linha(v)).collect()
    }
}

/// Where each path's arm points, in degrees (screen axes, y down):
/// Assassin up-left, Mage up-right, Guardian down-left, Corsair down-right.
pub const ANGULO_DO_CAMINHO: [f32; CAMINHOS] = [-135.0, -45.0, 135.0, 45.0];
/// The arms clockwise around the centre: a bridge joins `RODA[b]`'s right
/// side to `RODA[b + 1]`'s left side.
pub const RODA: [Caminho; CAMINHOS] =
    [Caminho::Assassino, Caminho::Mago, Caminho::Corsario, Caminho::Guardiao];

/// The template's role for a slot: which of the arm's tables fills it.
#[derive(Clone, Copy)]
enum Papel {
    /// The arm's central line (`Conteudo::espinha`, alternating).
    Espinha,
    /// The left (−y) side.
    Esquerda,
    /// The right (+y) side.
    Direita,
    /// Connectors and edge nodes.
    Ligacao,
    /// Notable `k` (`Conteudo::notaveis`).
    Notavel(u8),
    /// Active `k` of the path (0, 1, 2 = the capstone).
    Ativa(u8),
    /// Keystone `k` (`Conteudo::chaves`).
    Chave(u8),
}

/// One slot of the arm template: local position (x outward along the arm,
/// y to its side), parents (template indexes; `INICIO_LOCAL` = the start),
/// ring and role.
struct Molde {
    x: f32,
    y: f32,
    pais: [u8; 2],
    anel: u8,
    papel: Papel,
}

/// "The start" in `Molde::pais`; `SEM` = no second parent.
const INICIO_LOCAL: u8 = 254;
const SEM: u8 = 255;

const fn m(x: f32, y: f32, p1: u8, p2: u8, anel: u8, papel: Papel) -> Molde {
    Molde { x, y, pais: [p1, p2], anel, papel }
}

use Papel as P;

/// THE ARM TEMPLATE (41 slots), the same for the four paths. Indexes are
/// persisted through the node ids: append only.
///
/// ```text
///                  L-side: wheel → notable → chain → keystone
///   start ─ S0 ─ S1 ─ S2 ─ A1 ─ S3 ─ S4 ─ S5 ─ A2 ─ S6 ─ S7 ─ S8 ─ A3 ─ KC
///                  R-side: mirrored
/// ```
const MOLDE: [Molde; 41] = [
    // ring 1 — the spine to the first active, and two small wheels
    m(1.5, 0.0, INICIO_LOCAL, SEM, 1, P::Espinha), // 0 S0
    m(2.4, 0.0, 0, SEM, 1, P::Espinha),            // 1 S1
    m(3.3, 0.0, 1, SEM, 1, P::Espinha),            // 2 S2
    m(4.3, 0.0, 2, SEM, 1, P::Ativa(0)),           // 3 A1
    m(2.7, -1.0, 1, SEM, 1, P::Esquerda),          // 4 L0
    m(3.3, -1.9, 4, SEM, 1, P::Esquerda),          // 5 L1
    m(4.3, -2.4, 5, SEM, 1, P::Notavel(0)),        // 6 L2
    m(2.7, 1.0, 1, SEM, 1, P::Direita),            // 7 R0
    m(3.3, 1.9, 7, SEM, 1, P::Direita),            // 8 R1
    m(4.3, 2.4, 8, SEM, 1, P::Notavel(1)),         // 9 R2
    // ring 2 — the spine to the second active, the side chains, connectors
    m(5.3, 0.0, 3, SEM, 2, P::Espinha),     // 10 S3
    m(6.2, 0.0, 10, SEM, 2, P::Espinha),    // 11 S4
    m(7.1, 0.0, 11, SEM, 2, P::Espinha),    // 12 S5
    m(8.1, 0.0, 12, SEM, 2, P::Ativa(1)),   // 13 A2
    m(5.3, -2.9, 6, SEM, 2, P::Esquerda),   // 14 L3
    m(6.2, -3.4, 14, SEM, 2, P::Esquerda),  // 15 L4
    m(7.1, -3.9, 15, SEM, 2, P::Notavel(2)), // 16 L5
    m(5.3, 2.9, 9, SEM, 2, P::Direita),     // 17 R3
    m(6.2, 3.4, 17, SEM, 2, P::Direita),    // 18 R4
    m(7.1, 3.9, 18, SEM, 2, P::Notavel(3)), // 19 R5
    m(6.7, -1.6, 11, 15, 2, P::Ligacao),    // 20 ML (loop spine ↔ left)
    m(6.7, 1.6, 11, 18, 2, P::Ligacao),     // 21 MR (loop spine ↔ right)
    m(7.0, -5.4, 16, SEM, 2, P::Ligacao),   // 22 EL (toward the left bridge)
    m(7.0, 5.4, 19, SEM, 2, P::Ligacao),    // 23 ER (toward the right bridge)
    // ring 3 — the capstone, the keystones, the deep notables
    m(9.1, 0.0, 13, SEM, 3, P::Espinha),     // 24 S6
    m(10.0, 0.0, 24, SEM, 3, P::Espinha),    // 25 S7
    m(10.9, 0.0, 25, SEM, 3, P::Espinha),    // 26 S8
    m(12.0, 0.0, 26, SEM, 3, P::Ativa(2)),   // 27 A3 (capstone)
    m(13.3, 0.0, 27, SEM, 3, P::Chave(0)),   // 28 KC
    m(8.3, -4.2, 16, SEM, 3, P::Esquerda),   // 29 L6
    m(9.4, -4.4, 29, SEM, 3, P::Esquerda),   // 30 L7
    m(10.5, -4.3, 30, SEM, 3, P::Notavel(4)), // 31 L8
    m(11.5, -3.9, 31, SEM, 3, P::Esquerda),  // 32 L9
    m(12.5, -3.3, 32, SEM, 3, P::Chave(1)),  // 33 KL
    m(8.3, 4.2, 19, SEM, 3, P::Direita),     // 34 R6
    m(9.4, 4.4, 34, SEM, 3, P::Direita),     // 35 R7
    m(10.5, 4.3, 35, SEM, 3, P::Notavel(5)), // 36 R8
    m(11.5, 3.9, 36, SEM, 3, P::Direita),    // 37 R9
    m(12.5, 3.3, 37, SEM, 3, P::Chave(2)),   // 38 KR
    m(11.4, -1.8, 25, 32, 3, P::Notavel(6)), // 39 IL (loop spine ↔ left)
    m(11.4, 1.8, 25, 37, 3, P::Notavel(7)),  // 40 IR (loop spine ↔ right)
];

/// Nodes in one arm.
pub const NOS_POR_BRACO: usize = MOLDE.len();
/// Template slots of the three actives (`Papel::Ativa`).
const LOCAL_DAS_ATIVAS: [usize; 3] = [3, 13, 27];
/// The arm's central line in order (`Arvore::com_nos`, tests).
const ESPINHA: [usize; 13] = [0, 1, 2, 3, 10, 11, 12, 13, 24, 25, 26, 27, 28];

/// A named node's content.
struct Nomeado {
    nome: &'static str,
    efeitos: &'static [(Efeito, f32)],
}

const fn nm(nome: &'static str, efeitos: &'static [(Efeito, f32)]) -> Nomeado {
    Nomeado { nome, efeitos }
}

/// What fills one path's arm. Minor roles alternate their two entries
/// (even/odd slot of that role).
struct Conteudo {
    espinha: [(Efeito, f32); 2],
    esquerda: [(Efeito, f32); 2],
    direita: [(Efeito, f32); 2],
    ligacao: [(Efeito, f32); 2],
    /// The small passive of the active nodes 1, 2, 3.
    ativas: [(Efeito, f32); 3],
    /// L2, R2, L5, R5, L8, R8, IL, IR.
    notaveis: [Nomeado; 8],
    /// Centre (past the capstone), left, right.
    chaves: [Nomeado; 3],
}

use Efeito as E;

/// THE CONTENT, by `Caminho` index. Modest per node on purpose: the tree's
/// percentages ADD to the rebirth multiplier (`1 + 5%·r + tree%`), and the
/// whole web bought is the rebirth-50 power fantasy, not a new tier.
const CONTEUDO: [Conteudo; CAMINHOS] = [
    // Assassin
    Conteudo {
        espinha: [(E::Critico, 0.005), (E::Critico, 0.005)],
        esquerda: [(E::VelocidadeDeAtaque, 0.015), (E::VelocidadeDeAtaque, 0.015)],
        direita: [(E::DestrezaPct, 0.03), (E::DestrezaPct, 0.03)],
        ligacao: [(E::Movimento, 0.01), (E::Esquiva, 0.03)],
        ativas: [(E::Movimento, 0.01), (E::Critico, 0.01), (E::DanoCritico, 0.05)],
        notaveis: [
            nm("Quick Hands", &[(E::VelocidadeDeAtaque, 0.03), (E::Esquiva, 0.05)]),
            nm("Keen Eye", &[(E::Critico, 0.01), (E::DestrezaPct, 0.03)]),
            nm("Flurry", &[(E::VelocidadeDeAtaque, 0.04), (E::Movimento, 0.01)]),
            nm("Precision", &[(E::DanoCritico, 0.06), (E::Critico, 0.01)]),
            nm("Shadow Dancer", &[(E::Esquiva, 0.08), (E::Movimento, 0.02)]),
            nm("Assassinate", &[(E::DanoCritico, 0.08), (E::DestrezaPct, 0.03)]),
            nm("Cutthroat", &[(E::AtaquePct, 0.03), (E::Critico, 0.01)]),
            nm("Twin Fangs", &[(E::VelocidadeDeAtaque, 0.03), (E::DanoCritico, 0.04)]),
        ],
        chaves: [
            nm("Death's Edge", &[(E::DanoCritico, 0.30), (E::VidaPct, -0.15)]),
            nm("Whirlwind", &[(E::VelocidadeDeAtaque, 0.12), (E::DefesaPct, -0.15)]),
            nm("Opportunist", &[(E::Critico, 0.06), (E::DanoCritico, -0.10)]),
        ],
    },
    // Mage
    Conteudo {
        espinha: [(E::AtaquePct, 0.012), (E::AtaquePct, 0.012)],
        esquerda: [(E::SabedoriaPct, 0.03), (E::SabedoriaPct, 0.03)],
        direita: [(E::ManaPct, 0.04), (E::ManaPct, 0.04)],
        ligacao: [(E::RegenMana, 0.3), (E::RegenMana, 0.3)],
        ativas: [(E::SabedoriaPct, 0.03), (E::AtaquePct, 0.02), (E::AtaquePct, 0.03)],
        notaveis: [
            nm("Arcane Mind", &[(E::SabedoriaPct, 0.05), (E::RegenMana, 0.5)]),
            nm("Deep Reserves", &[(E::ManaPct, 0.08), (E::RegenMana, 0.3)]),
            nm("Insight", &[(E::SabedoriaPct, 0.06), (E::AtaquePct, 0.02)]),
            nm("Mana Spring", &[(E::RegenMana, 1.0), (E::ManaPct, 0.04)]),
            nm("Archmage", &[(E::AtaquePct, 0.05), (E::SabedoriaPct, 0.03)]),
            nm("Ley Lines", &[(E::ManaPct, 0.06), (E::RegenMana, 0.8)]),
            nm("Spellblade", &[(E::AtaquePct, 0.03), (E::Critico, 0.01)]),
            nm("Focus", &[(E::SabedoriaPct, 0.04), (E::VidaPct, 0.03)]),
        ],
        chaves: [
            nm("Overcharge", &[(E::AtaquePct, 0.15), (E::ManaPct, -0.20)]),
            nm("Mind over Matter", &[(E::SabedoriaPct, 0.15), (E::VidaPct, -0.10)]),
            nm("Eldritch Battery", &[(E::ManaPct, 0.30), (E::RegenMana, 2.0), (E::AtaquePct, -0.05)]),
        ],
    },
    // Guardian
    Conteudo {
        espinha: [(E::DefesaPct, 0.02), (E::DefesaPct, 0.02)],
        esquerda: [(E::VidaPct, 0.02), (E::VidaPct, 0.02)],
        direita: [(E::RegenVida, 0.5), (E::RegenVida, 0.5)],
        ligacao: [(E::Firmeza, 8.0), (E::Firmeza, 8.0)],
        ativas: [(E::Reducao, 0.005), (E::DefesaPct, 0.03), (E::Reducao, 0.01)],
        notaveis: [
            nm("Thick Skin", &[(E::VidaPct, 0.04), (E::Reducao, 0.005)]),
            nm("Second Wind", &[(E::RegenVida, 1.5), (E::VidaPct, 0.02)]),
            nm("Iron Body", &[(E::DefesaPct, 0.05), (E::VidaPct, 0.03)]),
            nm("Steadfast", &[(E::Firmeza, 25.0), (E::Reducao, 0.005)]),
            nm("Colossus", &[(E::VidaPct, 0.06), (E::Firmeza, 10.0)]),
            nm("Bastion", &[(E::Reducao, 0.01), (E::DefesaPct, 0.04)]),
            nm("Juggernaut", &[(E::Firmeza, 30.0), (E::VidaPct, 0.03)]),
            nm("Troll Blood", &[(E::RegenVida, 2.5), (E::DefesaPct, 0.02)]),
        ],
        chaves: [
            nm("Immovable", &[(E::Reducao, 0.04), (E::Movimento, -0.08)]),
            nm("Titan", &[(E::VidaPct, 0.18), (E::VelocidadeDeAtaque, -0.08)]),
            nm("Fortress", &[(E::DefesaPct, 0.25), (E::AtaquePct, -0.08)]),
        ],
    },
    // Corsair
    Conteudo {
        espinha: [(E::Drop, 0.02), (E::Drop, 0.02)],
        esquerda: [(E::Ouro, 0.03), (E::Minerio, 0.04)],
        direita: [(E::Energia, 0.04), (E::Darksteel, 0.04)],
        ligacao: [(E::Xp, 0.01), (E::Ouro, 0.02)],
        ativas: [(E::Drop, 0.02), (E::Ouro, 0.04), (E::LootMelhor, 0.01)],
        notaveis: [
            nm("Pickpocket", &[(E::Ouro, 0.06), (E::Drop, 0.02)]),
            nm("Prospector", &[(E::Minerio, 0.06), (E::Darksteel, 0.04)]),
            nm("Treasure Hunter", &[(E::Drop, 0.05), (E::LootMelhor, 0.005)]),
            nm("Vein Seeker", &[(E::Energia, 0.08), (E::Minerio, 0.03)]),
            nm("Golden Touch", &[(E::Ouro, 0.08), (E::LootMelhor, 0.01)]),
            nm("Deep Miner", &[(E::Darksteel, 0.08), (E::Minerio, 0.06)]),
            nm("Lucky Star", &[(E::LootMelhor, 0.01), (E::Drop, 0.03)]),
            nm("Seasoned Sailor", &[(E::Xp, 0.03), (E::Movimento, 0.02)]),
        ],
        chaves: [
            nm("Pirate King", &[(E::Drop, 0.15), (E::LootMelhor, 0.02), (E::VidaPct, -0.10)]),
            nm("Greed", &[(E::Ouro, 0.40), (E::DefesaPct, -0.10)]),
            nm("Master Gatherer", &[(E::Minerio, 0.25), (E::Darksteel, 0.25), (E::AtaquePct, -0.05)]),
        ],
    },
];

/// A bridge between two neighbouring arms (`RODA[b]` and `RODA[b + 1]`):
/// a minor one near the centre and a named notable at the edge.
struct Ponte {
    interna: [(Efeito, f32); 2],
    externa: Nomeado,
}

/// The four bridges, by `RODA` boundary (Assassin|Mage, Mage|Corsair,
/// Corsair|Guardian, Guardian|Assassin). Append only.
const PONTES: [Ponte; CAMINHOS] = [
    Ponte {
        interna: [(E::Critico, 0.005), (E::SabedoriaPct, 0.02)],
        externa: nm("Arcane Blade", &[(E::DanoCritico, 0.05), (E::AtaquePct, 0.02)]),
    },
    Ponte {
        interna: [(E::ManaPct, 0.03), (E::Drop, 0.02)],
        externa: nm("Alchemist", &[(E::Xp, 0.03), (E::Energia, 0.06)]),
    },
    Ponte {
        interna: [(E::Ouro, 0.03), (E::VidaPct, 0.02)],
        externa: nm("Sea Legs", &[(E::Firmeza, 20.0), (E::Drop, 0.04)]),
    },
    Ponte {
        interna: [(E::DefesaPct, 0.02), (E::VelocidadeDeAtaque, 0.015)],
        externa: nm("Duelist", &[(E::Reducao, 0.005), (E::Critico, 0.01)]),
    },
];
/// Radius of the inner and outer bridges.
const RAIO_DAS_PONTES: [f32; 2] = [4.2, 9.6];
/// Template slots each bridge touches: (right side of `RODA[b]`, left side
/// of `RODA[b + 1]`), inner then outer.
const LADOS_DAS_PONTES: [(usize, usize); 2] = [(8, 5), (23, 22)];

/// The start node's id.
pub const INICIO: u16 = 0;
/// First id of the bridges.
const PRIMEIRA_PONTE: u16 = 1 + (CAMINHOS * NOS_POR_BRACO) as u16;
/// Every node: the start, the four arms, the eight bridge nodes.
pub const NOS_TOTAL: usize = 1 + CAMINHOS * NOS_POR_BRACO + 2 * CAMINHOS;
/// 64-bit words of the allocation's bitset.
pub const PALAVRAS: usize = 3;
const _: () = assert!(NOS_TOTAL <= PALAVRAS * 64);
/// Points a keystone costs, wherever it sits.
pub const CUSTO_DA_CHAVE: u8 = 6;

/// The id of template slot `local` of path `c`'s arm.
pub fn id_no_braco(c: Caminho, local: usize) -> u16 {
    1 + (c as usize * NOS_POR_BRACO + local) as u16
}

/// The built web: nodes by id, neighbours, links (each once, low id first).
pub struct Tabela {
    pub nos: Vec<No>,
    pub vizinhos: Vec<Vec<u16>>,
    pub ligacoes: Vec<(u16, u16)>,
}

fn efeitos_de(lista: &[(Efeito, f32)]) -> ([(Efeito, f32); MAX_EFEITOS], u8) {
    let mut v = [(Efeito::Critico, 0.0); MAX_EFEITOS];
    for (i, e) in lista.iter().take(MAX_EFEITOS).enumerate() {
        v[i] = *e;
    }
    (v, lista.len().min(MAX_EFEITOS) as u8)
}

fn custo_de(tipo: TipoNo, anel: u8) -> u8 {
    match tipo {
        TipoNo::Inicio => 0,
        TipoNo::Menor => anel,
        TipoNo::Notavel | TipoNo::Ativa => anel + 1,
        TipoNo::Chave => CUSTO_DA_CHAVE,
    }
}

fn girar(x: f32, y: f32, graus: f32) -> (f32, f32) {
    let (s, c) = graus.to_radians().sin_cos();
    (x * c - y * s, x * s + y * c)
}

fn construir() -> Tabela {
    let mut nos = Vec::with_capacity(NOS_TOTAL);
    let mut ligacoes: Vec<(u16, u16)> = Vec::new();
    let vazio = efeitos_de(&[]);
    nos.push(No {
        id: INICIO,
        tipo: TipoNo::Inicio,
        caminho: None,
        ponte: None,
        x: 0.0,
        y: 0.0,
        anel: 0,
        custo: 0,
        nome: "Origin",
        efeitos: vazio.0,
        n_efeitos: vazio.1,
        ativa: None,
    });
    for c in Caminho::TODOS {
        let cont = &CONTEUDO[c as usize];
        let mut conta = [0usize; 4];
        for (local, mo) in MOLDE.iter().enumerate() {
            let id = id_no_braco(c, local);
            let (x, y) = girar(mo.x, mo.y, ANGULO_DO_CAMINHO[c as usize]);
            let (tipo, nome, lista, ativa): (TipoNo, &'static str, &[(Efeito, f32)], Option<u8>) = match mo.papel {
                P::Espinha | P::Esquerda | P::Direita | P::Ligacao => {
                    let (k, tabela) = match mo.papel {
                        P::Espinha => (0, &cont.espinha),
                        P::Esquerda => (1, &cont.esquerda),
                        P::Direita => (2, &cont.direita),
                        _ => (3, &cont.ligacao),
                    };
                    let e = &tabela[conta[k] % 2];
                    conta[k] += 1;
                    (TipoNo::Menor, "", std::slice::from_ref(e), None)
                }
                P::Notavel(k) => {
                    let n = &cont.notaveis[k as usize];
                    (TipoNo::Notavel, n.nome, n.efeitos, None)
                }
                P::Chave(k) => {
                    let n = &cont.chaves[k as usize];
                    (TipoNo::Chave, n.nome, n.efeitos, None)
                }
                P::Ativa(k) => {
                    let a = c as u8 * 3 + k;
                    (TipoNo::Ativa, ATIVAS[a as usize].1, std::slice::from_ref(&cont.ativas[k as usize]), Some(a))
                }
            };
            let (efeitos, n_efeitos) = efeitos_de(lista);
            nos.push(No {
                id,
                tipo,
                caminho: Some(c),
                ponte: None,
                x,
                y,
                anel: mo.anel,
                custo: custo_de(tipo, mo.anel),
                nome,
                efeitos,
                n_efeitos,
                ativa,
            });
            for p in mo.pais {
                match p {
                    SEM => {}
                    INICIO_LOCAL => ligacoes.push((INICIO, id)),
                    p => ligacoes.push((id_no_braco(c, p as usize), id)),
                }
            }
        }
    }
    for (b, ponte) in PONTES.iter().enumerate() {
        let (a, d) = (RODA[b], RODA[(b + 1) % CAMINHOS]);
        let angulo = ANGULO_DO_CAMINHO[a as usize] + 45.0;
        for (k, raio) in RAIO_DAS_PONTES.iter().enumerate() {
            let id = PRIMEIRA_PONTE + (2 * b + k) as u16;
            let (x, y) = girar(*raio, 0.0, angulo);
            let (tipo, anel, nome, lista): (TipoNo, u8, &'static str, &[(Efeito, f32)]) = if k == 0 {
                (TipoNo::Menor, 2, "", &ponte.interna)
            } else {
                (TipoNo::Notavel, 3, ponte.externa.nome, ponte.externa.efeitos)
            };
            let (efeitos, n_efeitos) = efeitos_de(lista);
            nos.push(No {
                id,
                tipo,
                caminho: None,
                ponte: Some((a, d)),
                x,
                y,
                anel,
                custo: custo_de(tipo, anel),
                nome,
                efeitos,
                n_efeitos,
                ativa: None,
            });
            let (la, ld) = LADOS_DAS_PONTES[k];
            ligacoes.push((id_no_braco(a, la), id));
            ligacoes.push((id_no_braco(d, ld), id));
        }
    }
    let mut vizinhos = vec![Vec::new(); nos.len()];
    for l in &mut ligacoes {
        if l.0 > l.1 {
            *l = (l.1, l.0);
        }
        vizinhos[l.0 as usize].push(l.1);
        vizinhos[l.1 as usize].push(l.0);
    }
    Tabela { nos, vizinhos, ligacoes }
}

/// The web (built once).
pub fn tabela() -> &'static Tabela {
    static T: std::sync::OnceLock<Tabela> = std::sync::OnceLock::new();
    T.get_or_init(construir)
}

/// Every node, by id.
pub fn nos() -> &'static [No] {
    &tabela().nos
}

/// Node `id`.
pub fn no(id: u16) -> Option<&'static No> {
    tabela().nos.get(id as usize)
}

/// The nodes linked to `id`.
pub fn vizinhos(id: u16) -> &'static [u16] {
    tabela().vizinhos.get(id as usize).map_or(&[], |v| v.as_slice())
}

/// Every link once (low id first).
pub fn ligacoes() -> &'static [(u16, u16)] {
    &tabela().ligacoes
}

/// Points to buy the whole web.
pub fn custo_total() -> u32 {
    nos().iter().map(|n| n.custo as u32).sum()
}

/// The colour key of a node: its path, or for a bridge the first of its two.
pub fn caminho_de(n: &No) -> Option<Caminho> {
    n.caminho.or(n.ponte.map(|p| p.0))
}

/// The actives of phase 4, by id (`No::ativa`): (path, English name).
pub const ATIVAS: [(Caminho, &str); 12] = [
    (Caminho::Assassino, "Shadowstep"),
    (Caminho::Assassino, "Invisibility"),
    (Caminho::Assassino, "Lethal Fury"),
    (Caminho::Mago, "Arcane Nova"),
    (Caminho::Mago, "Meteor"),
    (Caminho::Mago, "Time Warp"),
    (Caminho::Guardiao, "Taunt"),
    (Caminho::Guardiao, "Bulwark Shield"),
    (Caminho::Guardiao, "Unbreakable"),
    (Caminho::Corsario, "Treasure Sense"),
    (Caminho::Corsario, "Plunder"),
    (Caminho::Corsario, "Golden Fleet"),
];

// ── Phase 4: the actives in skill slots 4–6 ─────────────────────────────

/// Extra skill slots (4, 5, 6) the tree's actives go in.
pub const SLOTS_EXTRAS: usize = 3;
/// The rebirth that opens each extra slot (docs/RESET.md, "Marcos").
pub const ABRE_SLOT_EM: [u32; SLOTS_EXTRAS] = [1, 3, 5];
/// Lethal Fury's critical chance, before the supreme factor (0.25 = +25
/// points).
pub const FURIA_CRIT: f32 = 0.25;
/// Lethal Fury's extra critical damage, before the supreme factor (0.15 =
/// a crit deals 15% of the hit more).
pub const FURIA_DANO_CRIT: f32 = 0.15;

/// SUPREME: `MULT_SUPREMA` stronger (the "supreme active" of rebirth 10).
pub const SUPREMA_EM: u32 = 10;
/// How much stronger a supreme active is (damage, heal, buff, cut).
pub const MULT_SUPREMA: f32 = 1.5;
/// An active's id on the wire for cooldowns (`SkillsState`) and refusals
/// (`SkillRejected`): `ID_SKILL_BASE + ativa`. Far from the weapon skills
/// (1–12) and from the monsters' own ids.
pub const ID_SKILL_BASE: u32 = 200;

/// How many extra slots `renascimentos` open (0..=3).
pub fn slots_abertos(renascimentos: u32) -> usize {
    ABRE_SLOT_EM.iter().filter(|&&r| renascimentos >= r).count()
}

/// The skill id of active `ativa` (`ID_SKILL_BASE + ativa`).
pub fn id_de_skill(ativa: u8) -> u32 {
    ID_SKILL_BASE + ativa as u32
}

/// The active behind skill id `id`, if it is one.
pub fn ativa_do_id(id: u32) -> Option<u8> {
    id.checked_sub(ID_SKILL_BASE)
        .filter(|&a| (a as usize) < ATIVAS.len())
        .map(|a| a as u8)
}

/// Where active `ativa` sits: (path, node id).
pub fn no_da_ativa(ativa: u8) -> Option<(Caminho, u16)> {
    let c = Caminho::de_indice(ativa / 3).filter(|_| (ativa as usize) < ATIVAS.len())?;
    Some((c, id_no_braco(c, LOCAL_DAS_ATIVAS[(ativa % 3) as usize])))
}

/// Is `ativa` one of the CAPSTONES (the third active of its path, the
/// deepest one on the arm's spine)?
pub fn e_capstone(ativa: u8) -> bool {
    (ativa as usize) < ATIVAS.len() && ativa % 3 == 2
}

/// Is `ativa` supreme for a character with `renascimentos`? Only the
/// capstones, from `SUPREMA_EM`.
pub fn e_suprema(ativa: u8, renascimentos: u32) -> bool {
    renascimentos >= SUPREMA_EM && e_capstone(ativa)
}

/// What an active does (the server's `world/ativas.rs` applies it).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TipoAtiva {
    /// Blink `alcance` toward the target (or where the body is moving).
    Passo,
    /// `duracao_s` unseen: monsters lose aggro, players cannot target;
    /// attacking or casting ends it.
    Invisibilidade,
    /// LETHAL FURY (owner, 15/11/2026: it was a guaranteed critical strike;
    /// "troca para + de crit e 15% de dano crítico por x segundos"): for
    /// `duracao_s`, `FURIA_CRIT` × `fator` more critical chance and
    /// `FURIA_DANO_CRIT` × `fator` more critical damage, on every hit.
    Furia,
    /// `fator` × attack to every enemy within `raio` of the caster.
    Nova,
    /// `fator` × attack in `raio` around the target; stuns monsters
    /// (not bosses) for `duracao_s`.
    Meteoro,
    /// Cuts `fator` of what is left of the weapon skills' cooldowns.
    Tempo,
    /// Monsters within `raio` attack only the caster for `duracao_s`; the
    /// caster gets 2 s of Bulwark.
    Provocar,
    /// Bulwark (the shield skill's damage cut) for `duracao_s`, and a heal
    /// of `fator` × max health.
    Escudo,
    /// `Escudo` plus a full poise refill.
    Inquebravel,
    /// An economy buff for `duracao_s` (`bonus_de_saque`).
    Saque,
}

/// One active's numbers. `fator` is the potency in the unit `tipo` says;
/// a supreme active multiplies it by `MULT_SUPREMA`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DefAtiva {
    pub tipo: TipoAtiva,
    pub custo_mp: i32,
    /// Cooldown before wisdom (`skills::espera_efetiva`, same floor).
    pub espera_s: f32,
    pub duracao_s: f32,
    pub fator: f32,
    /// Radius of the area (Nova, Meteor, Taunt).
    pub raio: f32,
    /// How far the target may be (Death Mark, Meteor) or the blink goes.
    pub alcance: f32,
    /// English, player-facing (the client translates).
    pub descricao: &'static str,
}

const fn d(
    tipo: TipoAtiva,
    custo_mp: i32,
    espera_s: f32,
    duracao_s: f32,
    fator: f32,
    raio: f32,
    alcance: f32,
    descricao: &'static str,
) -> DefAtiva {
    DefAtiva { tipo, custo_mp, espera_s, duracao_s, fator, raio, alcance, descricao }
}

use TipoAtiva as T;

/// THE ACTIVES, by `ATIVAS` id (docs/RESET.md, phase 4 table). Modest on
/// purpose and uncalibrated: the weapon skills stay the main rotation.
pub const DEFS: [DefAtiva; 12] = [
    d(T::Passo, 20, 12.0, 0.0, 0.0, 0.0, 6.0,
        "Blink up to 6 m toward your target, or where you are moving. Does not break invisibility."),
    d(T::Invisibilidade, 30, 30.0, 6.0, 0.0, 0.0, 0.0,
        "Vanish for 6 s: monsters lose track of you and players cannot target you. Attacking or using a skill ends it."),
    d(T::Furia, 35, 25.0, 8.0, 1.0, 0.0, 0.0,
        "Lethal Fury: for 8 s, +25% critical chance and +15% critical damage."),
    d(T::Nova, 30, 10.0, 0.0, 1.3, 4.0, 0.0,
        "A blast around you: 130% of your attack to every enemy within 4 m."),
    d(T::Meteoro, 45, 18.0, 1.0, 1.8, 3.0, 10.0,
        "A meteor on your target: 180% of your attack in an area, stunning monsters for 1 s (not bosses)."),
    d(T::Tempo, 40, 45.0, 0.0, 0.5, 0.0, 0.0,
        "Halves the remaining cooldown of your weapon skills."),
    d(T::Provocar, 20, 15.0, 4.0, 0.0, 6.0, 0.0,
        "Monsters within 6 m attack only you for 4 s, and you take half damage for 2 s."),
    d(T::Escudo, 30, 25.0, 5.0, 0.08, 0.0, 0.0,
        "Take half damage for 5 s and heal 8% of your max health."),
    d(T::Inquebravel, 40, 60.0, 6.0, 0.20, 0.0, 0.0,
        "Take half damage for 6 s, heal 20% of your max health and refill your poise."),
    d(T::Saque, 25, 120.0, 60.0, 1.0, 0.0, 0.0,
        "+20% drop chance for 60 s."),
    d(T::Saque, 25, 120.0, 60.0, 1.0, 0.0, 0.0,
        "+30% gold from monsters for 60 s."),
    d(T::Saque, 50, 300.0, 90.0, 1.0, 0.0, 0.0,
        "For 90 s: +15% drop chance, +25% gold from monsters and +2% better loot chance."),
];

/// Active `ativa`'s numbers.
pub fn def(ativa: u8) -> Option<&'static DefAtiva> {
    DEFS.get(ativa as usize)
}

/// Active `ativa`'s English name.
pub fn nome_da_ativa(ativa: u8) -> &'static str {
    ATIVAS.get(ativa as usize).map_or("", |a| a.1)
}

/// The potency of `ativa` for `renascimentos` (`fator`, × `MULT_SUPREMA`
/// when supreme).
pub fn fator(ativa: u8, renascimentos: u32) -> f32 {
    let f = def(ativa).map_or(0.0, |d| d.fator);
    if e_suprema(ativa, renascimentos) {
        f * MULT_SUPREMA
    } else {
        f
    }
}

/// The economy buff of a Corsair active (`TipoAtiva::Saque`), supreme
/// included. Zero for every other active.
pub fn bonus_de_saque(ativa: u8, renascimentos: u32) -> crate::acessorios::Bonus {
    let mut b = crate::acessorios::Bonus::default();
    match ativa {
        9 => b.drop = 0.20,
        10 => b.ouro = 0.30,
        11 => {
            b.drop = 0.15;
            b.ouro = 0.25;
            b.loot_melhor = 0.02;
        }
        _ => return b,
    }
    let m = fator(ativa, renascimentos);
    b.drop *= m;
    b.ouro *= m;
    b.loot_melhor *= m;
    b
}


/// Points a character has EARNED: none before the first rebirth, then one
/// per `NIVEIS_POR_PONTO` SHOWN levels of every climb. `nivel_exibido` is
/// `progressao::nivel_exibido(xp, mult)` (1..=100).
pub fn pontos_ganhos(renascimentos: u32, nivel_exibido: u32) -> u32 {
    if renascimentos == 0 {
        return 0;
    }
    let por_subida = crate::progressao::NIVEL_MAX_EXIBIDO / NIVEIS_POR_PONTO;
    por_subida
        .saturating_mul(renascimentos - 1)
        .saturating_add(nivel_exibido.min(crate::progressao::NIVEL_MAX_EXIBIDO) / NIVEIS_POR_PONTO)
}

/// Gold to reset the whole tree with `gastos` points allocated.
pub fn custo_de_redistribuir(gastos: u32) -> u64 {
    OURO_POR_PONTO_REDISTRIBUIR.saturating_mul(gastos as u64)
}

/// A character's allocation: the SET of bought node ids (a bitset; the
/// start is always owned and never stored), and which owned active sits in
/// each extra skill slot (4, 5, 6).
///
/// `slots` is `serde(skip)`: they travel in `EstadoArvore::slots`. The
/// database keeps both in `arvore_json` (`para_json` / `de_json`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Arvore {
    /// Bit `id` set = node `id` bought.
    pub bits: [u64; PALAVRAS],
    /// The active (`ATIVAS` id) in extra slot 4, 5 and 6, or `None`.
    #[serde(skip)]
    pub slots: [Option<u8>; SLOTS_EXTRAS],
}

/// Why learning a node was refused (English: the client translates).
pub const SEM_PONTOS: &str = "Not enough free path points.";
pub const JA_APRENDIDO: &str = "You already know this node.";
pub const NAO_ALCANCAVEL: &str = "Learn a connected node first.";
pub const NO_INVALIDO: &str = "Unknown node.";
/// Why equipping an active was refused (English: the client translates).
pub const SLOT_FECHADO: &str = "This skill slot is still locked.";
pub const ATIVA_NAO_APRENDIDA: &str = "Learn this active in the path tree first.";

impl Arvore {
    /// An allocation with these node ids (unknown ids and the start are
    /// ignored; not validated: `validada` does that).
    pub fn com_ids(ids: &[u16]) -> Arvore {
        let mut a = Arvore::default();
        for &id in ids {
            a.marcar(id, true);
        }
        a
    }

    /// The first `nos[c]` nodes of each path's central line (start → S0 →
    /// … → capstone → centre keystone): the old "n nodes per path" shape,
    /// kept for tests and tools. Active 1 is the 4th, active 2 the 8th,
    /// the capstone the 12th.
    pub fn com_nos(nos: [u8; CAMINHOS]) -> Arvore {
        let mut a = Arvore::default();
        for c in Caminho::TODOS {
            for &local in ESPINHA.iter().take(nos[c as usize] as usize) {
                a.marcar(id_no_braco(c, local), true);
            }
        }
        a
    }

    fn marcar(&mut self, id: u16, sim: bool) {
        if id == INICIO || id as usize >= NOS_TOTAL {
            return;
        }
        let (w, b) = (id as usize / 64, id as usize % 64);
        if sim {
            self.bits[w] |= 1 << b;
        } else {
            self.bits[w] &= !(1 << b);
        }
    }

    /// Is node `id` bought? The start always is.
    pub fn tem(&self, id: u16) -> bool {
        if id == INICIO {
            return true;
        }
        (id as usize) < NOS_TOTAL && self.bits[id as usize / 64] & (1 << (id % 64)) != 0
    }

    /// The bought node ids (the start not included), ascending.
    pub fn ids(&self) -> Vec<u16> {
        (1..NOS_TOTAL as u16).filter(|&id| self.tem(id)).collect()
    }

    /// How many nodes are bought (the start not included).
    pub fn quantos(&self) -> usize {
        self.bits.iter().map(|w| w.count_ones() as usize).sum()
    }

    /// Points allocated (the bought nodes' costs).
    pub fn gastos(&self) -> u32 {
        self.ids().iter().filter_map(|&id| no(id)).map(|n| n.custo as u32).sum()
    }

    /// Points still free out of `ganhos`.
    pub fn livres(&self, ganhos: u32) -> u32 {
        ganhos.saturating_sub(self.gastos())
    }

    /// Is `id` not bought yet but linked to a bought node (or the start)?
    pub fn alcancavel(&self, id: u16) -> bool {
        (id as usize) < NOS_TOTAL && !self.tem(id) && vizinhos(id).iter().any(|&v| self.tem(v))
    }

    /// Can node `id` be learned with `ganhos` earned? Same rule as `aprender`.
    pub fn pode_aprender(&self, id: u16, ganhos: u32) -> Result<&'static No, &'static str> {
        let n = no(id).filter(|n| n.tipo != TipoNo::Inicio).ok_or(NO_INVALIDO)?;
        if self.tem(id) {
            return Err(JA_APRENDIDO);
        }
        if !self.alcancavel(id) {
            return Err(NAO_ALCANCAVEL);
        }
        if self.livres(ganhos) < n.custo as u32 {
            return Err(SEM_PONTOS);
        }
        Ok(n)
    }

    /// Buy node `id` with `ganhos` earned: it must be linked to a bought
    /// node and cost no more than the free points (a double tap on the same
    /// node is refused as already known). Returns the node bought.
    pub fn aprender(&mut self, id: u16, ganhos: u32) -> Result<&'static No, &'static str> {
        let n = self.pode_aprender(id, ganhos)?;
        self.marcar(id, true);
        Ok(n)
    }

    /// The cheapest way from what is owned to `id`: the nodes to buy, in
    /// order (empty when `id` is owned or unknown).
    pub fn rota_ate(&self, id: u16) -> Vec<u16> {
        if (id as usize) >= NOS_TOTAL || self.tem(id) {
            return Vec::new();
        }
        // Dijkstra over node costs from every owned node.
        let mut dist = vec![u32::MAX; NOS_TOTAL];
        let mut de = vec![u16::MAX; NOS_TOTAL];
        let mut fila = std::collections::BinaryHeap::new();
        for v in 0..NOS_TOTAL as u16 {
            if self.tem(v) {
                dist[v as usize] = 0;
                fila.push(std::cmp::Reverse((0u32, v)));
            }
        }
        while let Some(std::cmp::Reverse((d, v))) = fila.pop() {
            if d > dist[v as usize] {
                continue;
            }
            if v == id {
                break;
            }
            for &w in vizinhos(v) {
                let nd = d + no(w).map_or(0, |n| n.custo as u32);
                if nd < dist[w as usize] {
                    dist[w as usize] = nd;
                    de[w as usize] = v;
                    fila.push(std::cmp::Reverse((nd, w)));
                }
            }
        }
        let mut rota = Vec::new();
        let mut v = id;
        while !self.tem(v) {
            rota.push(v);
            v = de[v as usize];
            if v == u16::MAX {
                return Vec::new();
            }
        }
        rota.reverse();
        rota
    }

    /// Steps from the start to every owned node through owned nodes
    /// (`u32::MAX` = not owned or cut off).
    fn profundidades(&self) -> Vec<u32> {
        let mut prof = vec![u32::MAX; NOS_TOTAL];
        prof[INICIO as usize] = 0;
        let mut fila = std::collections::VecDeque::from([INICIO]);
        while let Some(v) = fila.pop_front() {
            for &w in vizinhos(v) {
                if self.tem(w) && prof[w as usize] == u32::MAX {
                    prof[w as usize] = prof[v as usize] + 1;
                    fila.push_back(w);
                }
            }
        }
        prof
    }

    /// The allocation made valid for `ganhos`: unknown ids and nodes cut off
    /// from the start are dropped, and while it costs more than earned the
    /// node FARTHEST from the start (by steps; ties: the dearer, then the
    /// higher id) goes — a deepest node never disconnects the others. A
    /// slot whose active is no longer owned is emptied.
    pub fn validada(mut self, ganhos: u32) -> Arvore {
        let validos = Arvore::com_ids(&self.ids());
        self.bits = validos.bits;
        let prof = self.profundidades();
        for id in self.ids() {
            if prof[id as usize] == u32::MAX {
                self.marcar(id, false);
            }
        }
        let mut gastos = self.gastos();
        while gastos > ganhos {
            let prof = self.profundidades();
            let Some(fora) = self
                .ids()
                .into_iter()
                .max_by_key(|&id| (prof[id as usize], no(id).map_or(0, |n| n.custo), id))
            else {
                break;
            };
            self.marcar(fora, false);
            gastos -= no(fora).map_or(0, |n| n.custo as u32);
        }
        self.limpar_slots();
        self
    }

    /// Empty every slot whose active is not owned (or is repeated).
    pub fn limpar_slots(&mut self) {
        for i in 0..SLOTS_EXTRAS {
            if let Some(a) = self.slots[i] {
                if !self.possui_ativa(a) || self.slots[..i].contains(&Some(a)) {
                    self.slots[i] = None;
                }
            }
        }
    }

    /// Is active `ativa` (an `ATIVAS` id) learned, i.e. its node bought?
    pub fn possui_ativa(&self, ativa: u8) -> bool {
        no_da_ativa(ativa).is_some_and(|(_, id)| self.tem(id))
    }

    /// The learned actives, in `ATIVAS` order.
    pub fn ativas_possuidas(&self) -> Vec<u8> {
        (0..ATIVAS.len() as u8).filter(|&a| self.possui_ativa(a)).collect()
    }

    /// The active in extra slot `slot` (0 = skill 4) that can be USED now:
    /// the slot is open for `renascimentos` and the active is owned.
    pub fn ativa_no_slot(&self, slot: u8, renascimentos: u32) -> Option<u8> {
        if slot as usize >= slots_abertos(renascimentos) {
            return None;
        }
        self.slots.get(slot as usize).copied().flatten().filter(|&a| self.possui_ativa(a))
    }

    /// Put `ativa` in extra slot `slot` (`None` empties it). Only an owned
    /// active, only an open slot; an active already in another slot moves.
    pub fn equipar(&mut self, slot: u8, ativa: Option<u8>, renascimentos: u32) -> Result<(), &'static str> {
        if slot as usize >= slots_abertos(renascimentos) {
            return Err(SLOT_FECHADO);
        }
        if let Some(a) = ativa {
            if !self.possui_ativa(a) {
                return Err(ATIVA_NAO_APRENDIDA);
            }
            for s in &mut self.slots {
                if *s == Some(a) {
                    *s = None;
                }
            }
        }
        self.slots[slot as usize] = ativa;
        Ok(())
    }

    /// A node just bought unlocked `ativa`: it goes in the first open EMPTY
    /// slot, if any. Returns that slot.
    pub fn encaixar_nova(&mut self, ativa: u8, renascimentos: u32) -> Option<usize> {
        if self.slots.contains(&Some(ativa)) || !self.possui_ativa(ativa) {
            return None;
        }
        let i = (0..slots_abertos(renascimentos)).find(|&i| self.slots[i].is_none())?;
        self.slots[i] = Some(ativa);
        Some(i)
    }

    /// Fill every open empty slot with owned actives not equipped yet (a
    /// rebirth that opens a slot). Returns how many were placed.
    pub fn preencher_slots(&mut self, renascimentos: u32) -> usize {
        let mut n = 0;
        for a in self.ativas_possuidas() {
            if self.encaixar_nova(a, renascimentos).is_some() {
                n += 1;
            }
        }
        n
    }

    /// The passives of every bought node, summed.
    pub fn passivas(&self) -> Passivas {
        let mut p = Passivas::default();
        for (w, &palavra) in self.bits.iter().enumerate() {
            let mut resto = palavra;
            while resto != 0 {
                let b = resto.trailing_zeros() as usize;
                resto &= resto - 1;
                if let Some(n) = no((w * 64 + b) as u16) {
                    for &(e, v) in n.efeitos() {
                        p.somar(e, v);
                    }
                }
            }
        }
        p
    }

    /// `characters.arvore_json`: `{"ids":[3,7,…],"slots":[x,null,y]}`.
    pub fn para_json(&self) -> String {
        serde_json::to_string(&Gravado { ids: self.ids(), slots: self.slots }).unwrap_or_default()
    }

    /// From `para_json`. Empty or broken = nothing allocated.
    ///
    /// MIGRATION (web redesign, 15/11/2026): the old shapes — `"[a,b,c,d]"`
    /// (phase 2, nodes per linear path) and `{"nos":[a,b,c,d],"slots":…}`
    /// (phase 4) — cannot map onto the web, so they load as NOTHING bought:
    /// every point comes back for free and the slots start empty
    /// (`preencher_slots` refills them as actives are learned again).
    /// `e_formato_antigo` tells them apart. Not validated against the
    /// budget: `validada` does that with the character's level.
    pub fn de_json(s: &str) -> Arvore {
        let Ok(g) = serde_json::from_str::<Gravado>(s.trim()) else {
            return Arvore::default();
        };
        let mut a = Arvore::com_ids(&g.ids);
        a.slots = g.slots;
        for v in &mut a.slots {
            if v.is_some_and(|id| id as usize >= ATIVAS.len()) {
                *v = None;
            }
        }
        a
    }

    /// Is `s` a row from before the web (refunded on load)?
    pub fn e_formato_antigo(s: &str) -> bool {
        let s = s.trim();
        serde_json::from_str::<[u8; CAMINHOS]>(s).is_ok()
            || serde_json::from_str::<GravadoAntigo>(s).is_ok_and(|g| g.nos.iter().any(|&n| n > 0))
    }
}

/// `arvore_json`'s shape.
#[derive(Serialize, Deserialize)]
struct Gravado {
    ids: Vec<u16>,
    #[serde(default)]
    slots: [Option<u8>; SLOTS_EXTRAS],
}

/// The phase-4 shape (before the web), only to recognise it.
#[derive(Deserialize)]
struct GravadoAntigo {
    nos: [u8; CAMINHOS],
}

/// The tree's passives, summed (each field in its effect's unit).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Passivas {
    pub critico: f32,
    pub velocidade_de_ataque: f32,
    pub destreza_pct: f32,
    pub movimento: f32,
    pub esquiva: f32,
    pub sabedoria_pct: f32,
    pub mana_pct: f32,
    pub regen_mana: f32,
    pub ataque_pct: f32,
    pub defesa_pct: f32,
    pub vida_pct: f32,
    pub reducao: f32,
    pub regen_vida: f32,
    pub firmeza: f32,
    pub drop: f32,
    pub ouro: f32,
    pub minerio: f32,
    pub energia: f32,
    pub darksteel: f32,
    pub loot_melhor: f32,
    /// Extra critical damage (server: `Session::extra_de_dano_crit`).
    pub dano_critico: f32,
    pub xp: f32,
}

impl Passivas {
    pub fn somar(&mut self, e: Efeito, v: f32) {
        *self.campo(e) += v;
    }

    /// The field effect `e` adds to.
    pub fn campo(&mut self, e: Efeito) -> &mut f32 {
        match e {
            E::Critico => &mut self.critico,
            E::VelocidadeDeAtaque => &mut self.velocidade_de_ataque,
            E::DestrezaPct => &mut self.destreza_pct,
            E::Movimento => &mut self.movimento,
            E::Esquiva => &mut self.esquiva,
            E::SabedoriaPct => &mut self.sabedoria_pct,
            E::ManaPct => &mut self.mana_pct,
            E::RegenMana => &mut self.regen_mana,
            E::AtaquePct => &mut self.ataque_pct,
            E::DefesaPct => &mut self.defesa_pct,
            E::VidaPct => &mut self.vida_pct,
            E::Reducao => &mut self.reducao,
            E::RegenVida => &mut self.regen_vida,
            E::Firmeza => &mut self.firmeza,
            E::Drop => &mut self.drop,
            E::Ouro => &mut self.ouro,
            E::Minerio => &mut self.minerio,
            E::Energia => &mut self.energia,
            E::Darksteel => &mut self.darksteel,
            E::LootMelhor => &mut self.loot_melhor,
            E::DanoCritico => &mut self.dano_critico,
            E::Xp => &mut self.xp,
        }
    }

    /// The economy share on top of the accessories' bonus.
    pub fn somar_economia(&self, b: &mut crate::acessorios::Bonus) {
        b.drop += self.drop;
        b.ouro += self.ouro;
        b.minerio += self.minerio;
        b.energia += self.energia;
        b.darksteel += self.darksteel;
        b.loot_melhor += self.loot_melhor;
        b.xp += self.xp;
    }
}

/// What the client asks of the tree (`ClientMessage::Arvore`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum PedidoArvore {
    /// Send me the state (`ServerMessage::Arvore`).
    Painel,
    /// RETIRED with the linear paths (15/11/2026): the server ignores it
    /// and resends the state. Kept for the postcard variant index.
    Gastar { caminho: u8, no: u8 },
    /// Reset every point for gold (`custo_de_redistribuir`).
    Redistribuir,
    /// RETIRED (15/11/2026, owner: Rebirth Essence is only for item
    /// ascension): the server refuses it politely; the reset is gold only.
    /// Kept for the postcard variant index.
    RedistribuirComEssencia,
    /// Put learned active `ativa` (an `ATIVAS` id) in extra skill slot
    /// `slot` (0 = skill 4); `None` empties the slot (rebirth phase 4).
    Equipar { slot: u8, ativa: Option<u8> },
    /// Buy web node `no` (`Arvore::aprender`): appended with the web.
    Aprender { no: u16 },
}

/// The tree's state for the client (`ServerMessage::Arvore`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct EstadoArvore {
    /// The bought node set (`Arvore::bits`).
    pub arvore: Arvore,
    /// Points earned so far (`pontos_ganhos`).
    pub ganhos: u32,
    /// Gold to reset now (`custo_de_redistribuir`).
    pub custo_redistribuir: u64,
    /// Phase 4 (appended): the active in extra slot 4, 5 and 6
    /// (`Arvore::slots`, which does not travel inside `arvore`).
    pub slots: [Option<u8>; SLOTS_EXTRAS],
    /// How many extra slots are open (`slots_abertos`).
    pub slots_abertos: u8,
    /// The character's rebirths (supreme actives, the slot hints).
    pub renascimentos: u32,
}

impl EstadoArvore {
    pub fn de(arvore: Arvore, ganhos: u32) -> EstadoArvore {
        EstadoArvore {
            arvore,
            ganhos,
            custo_redistribuir: custo_de_redistribuir(arvore.gastos()),
            slots: arvore.slots,
            slots_abertos: 0,
            renascimentos: 0,
        }
    }

    /// The same with the rebirth count (open slots, supreme actives).
    pub fn com_renascimentos(mut self, renascimentos: u32) -> EstadoArvore {
        self.renascimentos = renascimentos;
        self.slots_abertos = slots_abertos(renascimentos) as u8;
        self
    }

    pub fn livres(&self) -> u32 {
        self.arvore.livres(self.ganhos)
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    fn id(c: Caminho, local: usize) -> u16 {
        id_no_braco(c, local)
    }

    /// Everything bought.
    fn cheia() -> Arvore {
        Arvore::com_ids(&(1..NOS_TOTAL as u16).collect::<Vec<_>>())
    }

    /// Every node of path `c`'s arm.
    fn braco(c: Caminho) -> Arvore {
        Arvore::com_ids(&(0..NOS_POR_BRACO).map(|l| id(c, l)).collect::<Vec<_>>())
    }

    fn perto(a: f32, b: f32) -> bool {
        (a - b).abs() < 1e-4
    }

    #[test]
    fn pontos_a_partir_do_primeiro_renascimento() {
        assert_eq!(pontos_ganhos(0, 100), 0, "none before the first rebirth");
        assert_eq!(pontos_ganhos(1, 9), 0);
        assert_eq!(pontos_ganhos(1, 10), 1);
        assert_eq!(pontos_ganhos(1, 100), 10);
        assert_eq!(pontos_ganhos(3, 35), 23);
        assert_eq!(pontos_ganhos(1, 500), 10, "the shown level caps at 100");
        for r in 1..60 {
            assert_eq!(pontos_ganhos(r, 100), pontos_ganhos(r + 1, 1));
        }
    }

    #[test]
    fn contagem_e_custo_da_teia() {
        let t = tabela();
        assert_eq!(t.nos.len(), NOS_TOTAL);
        assert!((150..=180).contains(&NOS_TOTAL), "{NOS_TOTAL}");
        for (i, n) in t.nos.iter().enumerate() {
            assert_eq!(n.id as usize, i, "ids are the table's order");
        }
        let conta = |k: TipoNo| t.nos.iter().filter(|n| n.tipo == k).count();
        assert_eq!(conta(TipoNo::Inicio), 1);
        assert_eq!(conta(TipoNo::Ativa), 12);
        assert_eq!(conta(TipoNo::Chave), 12, "three keystones a path");
        assert_eq!(conta(TipoNo::Notavel), 36);
        assert_eq!(conta(TipoNo::Menor), 112);
        // The whole web: about rebirth 40–50 (10 points a climb).
        let total = custo_total();
        assert_eq!(total, 460);
        assert!((400..=500).contains(&total));
        assert!(pontos_ganhos(55, 100) >= total, "the owner's R55 test character buys it all");
        assert!(pontos_ganhos(7, 100) < total / 5, "rebirth 7 is far from done");
        assert_eq!(cheia().gastos(), total);
    }

    #[test]
    fn toda_casa_e_alcancavel_do_inicio() {
        let mut visto = vec![false; NOS_TOTAL];
        let mut pilha = vec![INICIO];
        visto[0] = true;
        while let Some(v) = pilha.pop() {
            for &w in vizinhos(v) {
                if !visto[w as usize] {
                    visto[w as usize] = true;
                    pilha.push(w);
                }
            }
        }
        assert!(visto.iter().all(|&v| v), "every node connects to the start");
        for &(a, b) in ligacoes() {
            assert!(a < b && (b as usize) < NOS_TOTAL);
        }
        // Each arm is reachable on its own, and the bridges join neighbours.
        for b in 0..CAMINHOS {
            let p = PRIMEIRA_PONTE + 2 * b as u16;
            let (a, d) = (RODA[b], RODA[(b + 1) % CAMINHOS]);
            for k in 0..2u16 {
                let v = vizinhos(p + k);
                assert!(v.iter().any(|&w| no(w).unwrap().caminho == Some(a)));
                assert!(v.iter().any(|&w| no(w).unwrap().caminho == Some(d)));
            }
        }
    }

    #[test]
    fn casas_nao_se_sobrepoem() {
        // Visual radius in tree units, per kind (the client draws at these).
        let raio = |n: &No| match n.tipo {
            TipoNo::Inicio => 0.55,
            TipoNo::Menor => 0.26,
            TipoNo::Notavel => 0.36,
            TipoNo::Ativa => 0.40,
            TipoNo::Chave => 0.48,
        };
        let nos = nos();
        for a in nos {
            for b in nos.iter().filter(|b| b.id > a.id) {
                let d = ((a.x - b.x).powi(2) + (a.y - b.y).powi(2)).sqrt();
                assert!(d >= raio(a) + raio(b) + 0.15, "{} and {} overlap ({d:.2})", a.id, b.id);
            }
        }
    }

    #[test]
    fn cada_tipo_tem_o_que_deve() {
        for n in nos() {
            match n.tipo {
                TipoNo::Inicio => assert!(n.efeitos().is_empty() && n.custo == 0),
                TipoNo::Menor => {
                    assert!(n.nome.is_empty() && !n.efeitos().is_empty());
                    assert!(n.efeitos().iter().all(|&(_, v)| v > 0.0));
                }
                TipoNo::Notavel => {
                    assert!(!n.nome.is_empty() && n.efeitos().len() >= 2, "{}", n.nome);
                    assert!(n.efeitos().iter().all(|&(_, v)| v > 0.0), "{}", n.nome);
                }
                TipoNo::Chave => {
                    assert!(!n.nome.is_empty());
                    assert!(n.efeitos().iter().any(|&(_, v)| v > 0.0), "{}: a bonus", n.nome);
                    assert!(n.efeitos().iter().any(|&(_, v)| v < 0.0), "{}: a cost", n.nome);
                    assert_eq!(n.custo, CUSTO_DA_CHAVE);
                }
                TipoNo::Ativa => {
                    let a = n.ativa.expect("an active node teaches an active");
                    assert_eq!(Some(ATIVAS[a as usize].0), n.caminho, "the active belongs to its path");
                    assert_eq!(n.nome, ATIVAS[a as usize].1);
                }
            }
            for &(e, v) in n.efeitos() {
                assert_eq!(e.modelo_de(v).matches("{}").count(), 1);
            }
        }
        let mut ids: Vec<u8> = nos().iter().filter_map(|n| n.ativa).collect();
        ids.sort();
        assert_eq!(ids, (0..12).collect::<Vec<u8>>(), "every active once");
        // Actives at meaningful depths: ring 1, 2 and 3 of their arm.
        for a in 0..12u8 {
            let (_, nid) = no_da_ativa(a).unwrap();
            assert_eq!(no(nid).unwrap().anel, a % 3 + 1);
            assert_eq!(no(nid).unwrap().ativa, Some(a));
        }
    }

    #[test]
    fn so_aprende_vizinho_do_que_ja_tem() {
        let mut a = Arvore::default();
        let s0 = id(Caminho::Assassino, 0);
        let s1 = id(Caminho::Assassino, 1);
        assert_eq!(a.aprender(s1, 50).err(), Some(NAO_ALCANCAVEL), "not next to anything owned");
        assert_eq!(a.aprender(s0, 0).err(), Some(SEM_PONTOS));
        assert!(a.aprender(s0, 50).is_ok(), "next to the start");
        assert_eq!(a.aprender(s0, 50).err(), Some(JA_APRENDIDO), "a double tap does not buy twice");
        assert!(a.aprender(s1, 50).is_ok());
        assert_eq!(a.aprender(INICIO, 50).err(), Some(NO_INVALIDO));
        assert_eq!(a.aprender(999, 50).err(), Some(NO_INVALIDO));
        // Every arm opens from the start.
        for c in Caminho::TODOS {
            assert!(a.alcancavel(id(c, 0)) || a.tem(id(c, 0)));
        }
        // Costs grow outward: ring 3 needs its 3 points.
        let mut b = Arvore::com_nos([12, 0, 0, 0]);
        let kc = id(Caminho::Assassino, 28);
        let gasto = b.gastos();
        assert_eq!(b.pode_aprender(kc, gasto + CUSTO_DA_CHAVE as u32 - 1).err(), Some(SEM_PONTOS));
        assert!(b.aprender(kc, gasto + CUSTO_DA_CHAVE as u32).is_ok());
        assert_eq!(b.livres(gasto + CUSTO_DA_CHAVE as u32), 0);
        // A bridge is learned from either side.
        let ponte = PRIMEIRA_PONTE; // inner Assassin|Mage
        let mut c = Arvore::default();
        for l in [0, 1, 7, 8] {
            c.aprender(id(Caminho::Assassino, l), 99).unwrap();
        }
        assert!(c.aprender(ponte, 99).is_ok());
        assert!(c.alcancavel(id(Caminho::Mago, 5)), "the bridge leads into the Mage arm");
    }

    #[test]
    fn aprender_e_redistribuir_devolvem_os_pontos() {
        let mut a = Arvore::default();
        let rota = a.rota_ate(id(Caminho::Guardiao, 13));
        assert_eq!(rota.len(), 8, "start → S0…S5, A1 → A2");
        for &n in &rota {
            a.aprender(n, 100).unwrap();
        }
        assert!(a.possui_ativa(7), "Bulwark Shield");
        let gasto = a.gastos();
        assert_eq!(gasto, 1 + 1 + 1 + 2 + 2 + 2 + 2 + 3);
        assert_eq!(a.livres(100), 100 - gasto);
        assert_eq!(custo_de_redistribuir(gasto), gasto as u64 * 1_000);
        // The reset is the empty allocation: every point free again.
        let zerada = Arvore::default();
        assert_eq!(zerada.livres(100), 100);
        assert!(a.rota_ate(id(Caminho::Guardiao, 13)).is_empty(), "owned: nothing to buy");
    }

    #[test]
    fn validar_corta_pela_ponta_e_mantem_conexo() {
        let a = Arvore::com_nos([12, 0, 5, 0]);
        let v = a.validada(a.gastos());
        assert_eq!(v, a, "a valid one stays as it is");
        let cortada = a.validada(10);
        assert!(cortada.gastos() <= 10);
        // What is left still hangs from the start.
        for n in cortada.ids() {
            assert!(cortada.profundidades()[n as usize] != u32::MAX);
        }
        // The capstone (deepest) went first.
        assert!(!cortada.possui_ativa(2));
        assert_eq!(Arvore::com_nos([5, 5, 0, 2]).validada(0), Arvore::default());
        // Nodes cut off from the start are dropped.
        let solta = Arvore::com_ids(&[id(Caminho::Mago, 27)]);
        assert_eq!(solta.validada(500), Arvore::default());
    }

    #[test]
    fn json_novo_ida_e_volta() {
        let mut a = Arvore::com_nos([4, 0, 3, 2]);
        a.slots = [Some(0), None, None];
        let j = a.para_json();
        assert!(j.contains("\"ids\""), "{j}");
        assert_eq!(Arvore::de_json(&j), a);
        assert!(!Arvore::e_formato_antigo(&j));
        assert_eq!(Arvore::de_json(""), Arvore::default());
        assert_eq!(Arvore::de_json("lixo"), Arvore::default());
        assert_eq!(Arvore::de_json(r#"{"ids":[1,9999],"slots":[0,99,null]}"#).ids(), vec![1]);
        assert_eq!(Arvore::de_json(r#"{"ids":[1],"slots":[0,99,null]}"#).slots, [Some(0), None, None]);
    }

    /// Saves from before the web (both shapes) load as a FREE RESPEC:
    /// nothing bought, slots empty, every point back.
    #[test]
    fn json_antigo_vira_redistribuicao_gratis() {
        for antigo in ["[5,0,10,3]", " [1,2,3,4] ", r#"{"nos":[15,5,0,0],"slots":[2,0,null]}"#, r#"{"nos":[1,0,0,0]}"#] {
            let a = Arvore::de_json(antigo);
            assert_eq!(a, Arvore::default(), "{antigo}");
            assert_eq!(a.gastos(), 0);
            assert_eq!(a.livres(37), 37, "every point refunded");
            assert!(Arvore::e_formato_antigo(antigo), "{antigo}");
        }
        assert!(!Arvore::e_formato_antigo(r#"{"nos":[0,0,0,0]}"#), "nothing to refund");
        // The next save writes the new shape.
        assert!(Arvore::de_json("[5,0,10,3]").para_json().starts_with("{\"ids\""));
    }

    #[test]
    fn ativa_e_aprendida_pela_casa() {
        let mut a = Arvore::default();
        let (c, nid) = no_da_ativa(4).unwrap();
        assert_eq!(c, Caminho::Mago);
        assert!(!a.possui_ativa(4));
        for n in a.rota_ate(nid) {
            a.aprender(n, 999).unwrap();
        }
        assert!(a.possui_ativa(4), "Meteor learned with its node");
        assert!(a.possui_ativa(3), "Arcane Nova sits on the way");
        assert_eq!(a.ativas_possuidas(), vec![3, 4]);
        assert!(!a.possui_ativa(77));
    }

    #[test]
    fn passivas_somam_certo() {
        assert_eq!(Arvore::default().passivas(), Passivas::default());
        // Three Assassin spine nodes: crit 0.5 pp each.
        let p = Arvore::com_nos([3, 0, 0, 0]).passivas();
        assert!(perto(p.critico, 0.015), "{}", p.critico);
        // The sum over the whole web equals the sum of every node's effects.
        let mut esperado = Passivas::default();
        for n in nos() {
            for &(e, v) in n.efeitos() {
                esperado.somar(e, v);
            }
        }
        assert_eq!(cheia().passivas(), esperado);
        // Nothing leaks across arms.
        let corsario = braco(Caminho::Corsario).passivas();
        assert!(corsario.critico == 0.0 && corsario.dano_critico == 0.0 && corsario.sabedoria_pct == 0.0);
        let mut b = crate::acessorios::Bonus::default();
        braco(Caminho::Corsario).passivas().somar_economia(&mut b);
        assert!(b.drop > 0.2 && b.ouro > 0.2 && b.xp > 0.0 && b.loot_melhor > 0.0);
        // Modest: the whole web is the R50 fantasy, not a new tier.
        let t = cheia().passivas();
        assert!(t.critico <= 0.18 && t.dano_critico <= 0.80, "{t:?}");
        assert!(t.vida_pct <= 0.40 && t.ataque_pct <= 0.45 && t.defesa_pct <= 0.60, "{t:?}");
        assert!(t.reducao <= 0.10 && t.velocidade_de_ataque <= 0.40, "{t:?}");
    }

    #[test]
    fn chaves_cobram_o_preco() {
        let antes = Arvore::com_nos([12, 0, 0, 0]);
        let mut depois = antes;
        depois.aprender(id(Caminho::Assassino, 28), 999).unwrap(); // Death's Edge
        let (p0, p1) = (antes.passivas(), depois.passivas());
        assert!(perto(p1.dano_critico - p0.dano_critico, 0.30));
        assert!(perto(p1.vida_pct - p0.vida_pct, -0.15));
        let n = no(id(Caminho::Assassino, 28)).unwrap();
        assert_eq!(n.linhas(), vec!["Crit damage +30%".to_string(), "Max health -15%".to_string()]);
        // Titan: health up, attack speed down.
        let titan = no(id(Caminho::Guardiao, 33)).unwrap();
        assert_eq!(titan.nome, "Titan");
        let mut p = Passivas::default();
        for &(e, v) in titan.efeitos() {
            p.somar(e, v);
        }
        assert!(p.vida_pct > 0.0 && p.velocidade_de_ataque < 0.0);
    }

    #[test]
    fn numeros_e_linhas() {
        assert_eq!(Efeito::Critico.numero(0.01), "1");
        assert_eq!(Efeito::Minerio.numero(0.075), "7.5");
        assert_eq!(Efeito::RegenMana.numero(0.5), "0.5");
        assert_eq!(Efeito::Firmeza.numero(25.0), "25");
        assert_eq!(Efeito::VidaPct.linha(-0.15), "Max health -15%");
        assert_eq!(Efeito::Reducao.linha(0.01), "Damage taken -1%");
        assert_eq!(Efeito::Reducao.linha(-0.01), "Damage taken +1%");
        assert_eq!(Efeito::Critico.linha(0.005), "Crit chance +0.5%");
    }

    #[test]
    fn redistribuir_cresce_com_os_pontos() {
        assert_eq!(custo_de_redistribuir(0), 0);
        assert_eq!(custo_de_redistribuir(37), 37_000);
        let a = Arvore::com_nos([2, 0, 0, 1]);
        let e = EstadoArvore::de(a, 10);
        assert_eq!((e.livres(), e.custo_redistribuir), (7, 3_000));
    }

    // ── phase 4: actives in slots 4–6 ──

    #[test]
    fn slots_abrem_nos_renascimentos_1_3_e_5() {
        let esperado = [(0, 0), (1, 1), (2, 1), (3, 2), (4, 2), (5, 3), (9, 3), (40, 3)];
        for (r, n) in esperado {
            assert_eq!(slots_abertos(r), n, "rebirth {r}");
        }
        let mut a = Arvore::com_nos([12, 0, 0, 0]);
        assert_eq!(a.equipar(0, Some(1), 0), Err(SLOT_FECHADO));
        assert!(a.equipar(0, Some(1), 1).is_ok());
        assert_eq!(a.equipar(1, Some(0), 2), Err(SLOT_FECHADO));
        assert!(a.equipar(1, Some(0), 3).is_ok());
        assert_eq!(a.equipar(2, Some(2), 4), Err(SLOT_FECHADO));
        assert!(a.equipar(2, Some(2), 5).is_ok());
        assert_eq!(a.equipar(3, Some(2), 99), Err(SLOT_FECHADO), "there is no 7th slot");
        assert_eq!(a.ativa_no_slot(2, 5), Some(2));
        assert_eq!(a.ativa_no_slot(2, 4), None);
    }

    #[test]
    fn so_equipa_ativa_aprendida() {
        let mut a = Arvore::com_nos([3, 0, 0, 0]);
        assert_eq!(a.equipar(0, Some(0), 5), Err(ATIVA_NAO_APRENDIDA), "active node not bought");
        a = Arvore::com_nos([4, 0, 0, 0]);
        assert!(a.possui_ativa(0) && !a.possui_ativa(1) && !a.possui_ativa(3));
        assert!(a.equipar(0, Some(0), 5).is_ok());
        assert_eq!(a.equipar(1, Some(3), 5), Err(ATIVA_NAO_APRENDIDA), "another path's active");
        assert_eq!(a.equipar(1, Some(77), 5), Err(ATIVA_NAO_APRENDIDA), "unknown id");
        assert!(a.equipar(2, Some(0), 5).is_ok());
        assert_eq!(a.slots, [None, None, Some(0)], "equipping elsewhere MOVES it");
        // Losing the node (budget cut) empties the slot.
        let mut b = Arvore::com_nos([12, 0, 0, 0]);
        b.equipar(0, Some(2), 1).unwrap();
        let v = b.validada(10);
        assert_eq!(v.slots[0], None, "Lethal Fury went with the cut");
    }

    #[test]
    fn comprar_ativa_ocupa_o_proximo_slot_aberto() {
        let mut a = Arvore::com_nos([4, 0, 0, 0]);
        assert_eq!(a.encaixar_nova(0, 0), None, "no slot before the first rebirth");
        assert_eq!(a.encaixar_nova(0, 1), Some(0));
        assert_eq!(a.encaixar_nova(0, 1), None, "already in a slot");
        a = Arvore { slots: a.slots, ..Arvore::com_nos([4, 4, 0, 0]) };
        assert_eq!(a.encaixar_nova(3, 1), None, "the only open slot is taken");
        assert_eq!(a.preencher_slots(3), 1);
        assert_eq!(a.slots, [Some(0), Some(3), None]);
    }

    #[test]
    fn fio_do_estado() {
        let mut a = Arvore::com_nos([4, 8, 0, 1]);
        a.slots = [Some(0), Some(4), None];
        let e = EstadoArvore::de(a, 40).com_renascimentos(3);
        let volta: EstadoArvore = postcard::from_bytes(&postcard::to_allocvec(&e).unwrap()).unwrap();
        assert_eq!(volta.arvore.ids(), a.ids());
        assert_eq!(volta.slots, a.slots);
        assert_eq!((volta.slots_abertos, volta.renascimentos), (2, 3));
        let p = PedidoArvore::Aprender { no: 170 };
        let v: PedidoArvore = postcard::from_bytes(&postcard::to_allocvec(&p).unwrap()).unwrap();
        assert_eq!(v, p);
    }

    #[test]
    fn tabela_das_ativas() {
        for (i, d) in DEFS.iter().enumerate() {
            assert!(d.custo_mp > 0 && d.custo_mp <= 50, "{}", ATIVAS[i].1);
            assert!(d.espera_s >= crate::skills::RECARGA_MINIMA_S, "{}", ATIVAS[i].1);
            assert!(!d.descricao.is_empty() && !d.descricao.contains("{}"));
            assert_eq!(ativa_do_id(id_de_skill(i as u8)), Some(i as u8));
        }
        assert_eq!(ativa_do_id(12), None, "a weapon skill is not an active");
        assert_eq!(ativa_do_id(ID_SKILL_BASE + 12), None);
        assert_eq!(DEFS[1].tipo, TipoAtiva::Invisibilidade);
        assert_eq!(no_da_ativa(1), Some((Caminho::Assassino, id(Caminho::Assassino, 13))));
        // Supreme: only capstones, only from rebirth 10.
        assert!(!e_suprema(2, 9) && e_suprema(2, 10));
        assert!(!e_suprema(0, 50), "the first active is never supreme");
        assert!((fator(2, 10) - DEFS[2].fator * MULT_SUPREMA).abs() < 1e-6);
        assert!((bonus_de_saque(11, 10).ouro - 0.375).abs() < 1e-6);
        assert_eq!(bonus_de_saque(3, 10), crate::acessorios::Bonus::default());
        assert!((bonus_de_saque(9, 1).drop - 0.20).abs() < 1e-6);
    }
}
