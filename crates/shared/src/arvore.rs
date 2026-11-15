//! THE PATH TREE ("árvore de caminhos", docs/RESET.md, rebirth phase 2).
//!
//! From the first rebirth on, every 10 SHOWN levels of a climb earn one
//! point (`pontos_ganhos`): 10 a climb, so a character with `r` rebirths at
//! shown level `n` has `10·(r−1) + floor(n/10)` points. The points are
//! DERIVED, never stored; what is stored is the allocation (`Arvore`, how
//! many nodes of each path are bought: `characters.arvore_json`).
//!
//! Four paths of 15 nodes each (`NOS`), linear: node k needs node k−1 of the
//! same path, and the points spread freely across paths. Every node is a
//! small passive; nodes 5, 10 and 15 are the paths' ACTIVE nodes
//! (`No::ativa`): besides their passive they unlock an ACTIVE skill (phase
//! 4, `DEFS`) that goes in one of the extra skill slots 4–6
//! (`Arvore::slots`). The slots open at rebirths 1, 3 and 5
//! (`ABRE_SLOT_EM`); from rebirth 10 the capstone actives (node 15) are
//! SUPREME (`e_suprema`, `MULT_SUPREMA`).
//!
//! Combat passives (`Passivas`) apply in the server's `effective_stats_rv`;
//! economy ones (drop, ore, Energy, Darksteel, gold, better loot) add to the
//! accessories' `acessorios::Bonus` (`Passivas::somar_economia`).
//!
//! Respec (`custo_de_redistribuir`) refunds every point for gold, or for
//! Rebirth Essence (`PedidoArvore::RedistribuirComEssencia`, phase 3).

use serde::{Deserialize, Serialize};

/// How many paths.
pub const CAMINHOS: usize = 4;
/// Nodes in each path.
pub const NOS_POR_CAMINHO: u8 = 15;
/// Shown levels per point.
pub const NIVEIS_POR_PONTO: u32 = 10;
/// Gold per allocated point to reset the tree (1,000 × points).
pub const OURO_POR_PONTO_REDISTRIBUIR: u64 = 1_000;

/// The four paths, in the tree's (and the allocation's) order.
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
/// for poise.
#[derive(Clone, Copy, Debug, PartialEq)]
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
        }
    }

    /// Is the value shown ×100 (a fraction) or as is (per second, points)?
    fn em_pct(self) -> bool {
        !matches!(self, Efeito::RegenMana | Efeito::RegenVida | Efeito::Firmeza)
    }

    /// The number for `modelo`'s hole ("2", "0.5", "7.5").
    pub fn numero(self, valor: f32) -> String {
        let v = if self.em_pct() { valor * 100.0 } else { valor };
        let r = (v * 10.0).round() / 10.0;
        if (r - r.round()).abs() < 1e-4 {
            format!("{}", r.round() as i64)
        } else {
            format!("{r:.1}")
        }
    }
}

/// One node of a path.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct No {
    pub efeito: Efeito,
    pub valor: f32,
    /// The ACTIVE this node unlocks in phase 4 (its id in `ATIVAS`), or
    /// `None`. Nodes 5, 10 and 15 of every path.
    pub ativa: Option<u8>,
}

const fn n(efeito: Efeito, valor: f32) -> No {
    No { efeito, valor, ativa: None }
}
const fn a(efeito: Efeito, valor: f32, ativa: u8) -> No {
    No { efeito, valor, ativa: Some(ativa) }
}

/// The actives of phase 4, by id (`No::ativa`): (path, English name).
pub const ATIVAS: [(Caminho, &str); 12] = [
    (Caminho::Assassino, "Shadowstep"),
    (Caminho::Assassino, "Invisibility"),
    (Caminho::Assassino, "Death Mark"),
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
/// From this rebirth on the capstone actives (node 15 of every path) are
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

/// Where active `ativa` sits: (path, node 0-based).
pub fn no_da_ativa(ativa: u8) -> Option<(Caminho, u8)> {
    for c in Caminho::TODOS {
        for (k, no) in NOS[c as usize].iter().enumerate() {
            if no.ativa == Some(ativa) {
                return Some((c, k as u8));
            }
        }
    }
    None
}

/// Is `ativa` supreme for a character with `renascimentos`? Only the
/// capstones (node 15), from `SUPREMA_EM`.
pub fn e_suprema(ativa: u8, renascimentos: u32) -> bool {
    renascimentos >= SUPREMA_EM
        && no_da_ativa(ativa).is_some_and(|(_, k)| k + 1 == NOS_POR_CAMINHO)
}

/// What an active does (the server's `world/ativas.rs` applies it).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TipoAtiva {
    /// Blink `alcance` toward the target (or where the body is moving).
    Passo,
    /// `duracao_s` unseen: monsters lose aggro, players cannot target;
    /// attacking or casting ends it.
    Invisibilidade,
    /// One strike on the target: `fator` × attack, always critical.
    Golpe,
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
    d(T::Golpe, 35, 20.0, 0.0, 2.0, 0.0, 8.0,
        "A guaranteed critical strike on your target: 200% of your attack."),
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

use Efeito as E;

/// THE TABLE: four paths × 15 nodes, in order. Full-path totals (tested in
/// `totais_de_cada_caminho`):
///
/// - Assassin: crit +5 pp, attack speed +8%, dexterity +12%, movement +4%,
///   dash cooldown −5%.
/// - Mage: wisdom +16%, max mana +20%, attack +8%, mana regen +1.5/s.
/// - Guardian: defense +20%, max health +16%, damage taken −4%, health
///   regen +2/s, poise +25.
/// - Corsair: drop +16%, gold +20%, ore +15%, Energy +15%, Darksteel +15%,
///   better loot +2%.
pub const NOS: [[No; NOS_POR_CAMINHO as usize]; CAMINHOS] = [
    // Assassin
    [
        n(E::Critico, 0.01),
        n(E::VelocidadeDeAtaque, 0.02),
        n(E::DestrezaPct, 0.04),
        n(E::Critico, 0.01),
        a(E::Movimento, 0.02, 0),
        n(E::VelocidadeDeAtaque, 0.02),
        n(E::Critico, 0.01),
        n(E::DestrezaPct, 0.04),
        n(E::Esquiva, 0.05),
        a(E::Critico, 0.01, 1),
        n(E::VelocidadeDeAtaque, 0.02),
        n(E::DestrezaPct, 0.04),
        n(E::Critico, 0.01),
        n(E::VelocidadeDeAtaque, 0.02),
        a(E::Movimento, 0.02, 2),
    ],
    // Mage
    [
        n(E::SabedoriaPct, 0.04),
        n(E::ManaPct, 0.05),
        n(E::AtaquePct, 0.02),
        n(E::RegenMana, 0.5),
        a(E::SabedoriaPct, 0.04, 3),
        n(E::ManaPct, 0.05),
        n(E::AtaquePct, 0.02),
        n(E::RegenMana, 0.5),
        n(E::SabedoriaPct, 0.04),
        a(E::AtaquePct, 0.02, 4),
        n(E::ManaPct, 0.05),
        n(E::RegenMana, 0.5),
        n(E::SabedoriaPct, 0.04),
        n(E::ManaPct, 0.05),
        a(E::AtaquePct, 0.02, 5),
    ],
    // Guardian
    [
        n(E::DefesaPct, 0.04),
        n(E::VidaPct, 0.04),
        n(E::RegenVida, 1.0),
        n(E::DefesaPct, 0.04),
        a(E::Reducao, 0.01, 6),
        n(E::VidaPct, 0.04),
        n(E::DefesaPct, 0.04),
        n(E::Firmeza, 25.0),
        n(E::VidaPct, 0.04),
        a(E::Reducao, 0.01, 7),
        n(E::DefesaPct, 0.04),
        n(E::RegenVida, 1.0),
        n(E::VidaPct, 0.04),
        n(E::DefesaPct, 0.04),
        a(E::Reducao, 0.02, 8),
    ],
    // Corsair
    [
        n(E::Drop, 0.04),
        n(E::Ouro, 0.06),
        n(E::Minerio, 0.08),
        n(E::Energia, 0.08),
        a(E::Drop, 0.04, 9),
        n(E::Darksteel, 0.08),
        n(E::Ouro, 0.07),
        n(E::LootMelhor, 0.01),
        n(E::Minerio, 0.07),
        a(E::Drop, 0.04, 10),
        n(E::Energia, 0.07),
        n(E::Darksteel, 0.07),
        n(E::Ouro, 0.07),
        n(E::LootMelhor, 0.01),
        a(E::Drop, 0.04, 11),
    ],
];

/// The node `k` (0-based) of path `c`.
pub fn no(c: Caminho, k: u8) -> Option<&'static No> {
    NOS[c as usize].get(k as usize)
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

/// A character's allocation: how many nodes of each path are bought (the
/// first `nos[c]` of path `c`, since the paths are linear), and which owned
/// active sits in each extra skill slot (4, 5, 6).
///
/// `slots` is `serde(skip)`: the WIRE shape of `Arvore` (inside
/// `EstadoArvore`) stays the phase-2 one, and the slots travel in
/// `EstadoArvore::slots`, appended at the end. The database keeps both in
/// `arvore_json` (`para_json` / `de_json`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Arvore {
    pub nos: [u8; CAMINHOS],
    /// The active (`ATIVAS` id) in extra slot 4, 5 and 6, or `None`.
    #[serde(skip)]
    pub slots: [Option<u8>; SLOTS_EXTRAS],
}

/// Why spending a point was refused (English: the client translates).
pub const SEM_PONTOS: &str = "No free path points.";
pub const CAMINHO_COMPLETO: &str = "This path is complete.";
pub const FORA_DE_ORDEM: &str = "Buy the path's previous node first.";
pub const CAMINHO_INVALIDO: &str = "Unknown path.";
/// Why equipping an active was refused (English: the client translates).
pub const SLOT_FECHADO: &str = "This skill slot is still locked.";
pub const ATIVA_NAO_APRENDIDA: &str = "Learn this active in the path tree first.";

impl Arvore {
    /// An allocation with no slot filled.
    pub const fn com_nos(nos: [u8; CAMINHOS]) -> Arvore {
        Arvore { nos, slots: [None; SLOTS_EXTRAS] }
    }

    /// Points allocated in all paths.
    pub fn gastos(&self) -> u32 {
        self.nos.iter().map(|&v| v as u32).sum()
    }

    /// Points still free out of `ganhos`.
    pub fn livres(&self, ganhos: u32) -> u32 {
        ganhos.saturating_sub(self.gastos())
    }

    /// Is node `k` of path `c` bought?
    pub fn tem(&self, c: Caminho, k: u8) -> bool {
        k < self.nos[c as usize]
    }

    /// Buy node `k` of path `caminho` with `ganhos` earned. `k` must be the
    /// NEXT node of that path (the order, and a guard against a double tap
    /// spending twice). Returns the node bought.
    pub fn gastar(&mut self, caminho: u8, k: u8, ganhos: u32) -> Result<&'static No, &'static str> {
        let c = Caminho::de_indice(caminho).ok_or(CAMINHO_INVALIDO)?;
        let atual = self.nos[c as usize];
        if atual >= NOS_POR_CAMINHO {
            return Err(CAMINHO_COMPLETO);
        }
        if k != atual {
            return Err(FORA_DE_ORDEM);
        }
        if self.livres(ganhos) == 0 {
            return Err(SEM_PONTOS);
        }
        self.nos[c as usize] = atual + 1;
        Ok(&NOS[c as usize][atual as usize])
    }

    /// The allocation made valid for `ganhos`: no path past 15, and never
    /// more points than earned — the excess comes off the fullest path's
    /// LAST nodes (ties: the later path), which keeps every path linear.
    /// A slot whose active is no longer owned is emptied.
    pub fn validada(mut self, ganhos: u32) -> Arvore {
        for v in &mut self.nos {
            *v = (*v).min(NOS_POR_CAMINHO);
        }
        while self.gastos() > ganhos {
            let (i, _) = self
                .nos
                .iter()
                .enumerate()
                .max_by_key(|(i, v)| (**v, *i))
                .expect("four paths");
            self.nos[i] -= 1;
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
        no_da_ativa(ativa).is_some_and(|(c, k)| self.tem(c, k))
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
        for c in Caminho::TODOS {
            for k in 0..self.nos[c as usize].min(NOS_POR_CAMINHO) {
                let no = &NOS[c as usize][k as usize];
                p.somar(no.efeito, no.valor);
            }
        }
        p
    }

    /// `characters.arvore_json`: `"[a,b,c,d]"` (phase 2) while no slot is
    /// filled, else `{"nos":[a,b,c,d],"slots":[x,null,y]}` (phase 4).
    pub fn para_json(&self) -> String {
        if self.slots.iter().all(Option::is_none) {
            return serde_json::to_string(&self.nos).unwrap_or_default();
        }
        serde_json::to_string(&Gravado { nos: self.nos, slots: self.slots }).unwrap_or_default()
    }

    /// From `para_json`, either shape; empty or broken = nothing allocated.
    /// Not validated against the budget: `validada` does that with the
    /// character's level (and empties slots of actives no longer owned).
    pub fn de_json(s: &str) -> Arvore {
        let s = s.trim();
        if let Ok(nos) = serde_json::from_str::<[u8; CAMINHOS]>(s) {
            return Arvore { nos, slots: [None; SLOTS_EXTRAS] };
        }
        let Ok(g) = serde_json::from_str::<Gravado>(s) else {
            return Arvore::default();
        };
        let mut a = Arvore { nos: g.nos, slots: g.slots };
        for v in &mut a.slots {
            if v.is_some_and(|id| id as usize >= ATIVAS.len()) {
                *v = None;
            }
        }
        a
    }
}

/// `arvore_json`'s phase-4 shape.
#[derive(Serialize, Deserialize)]
struct Gravado {
    nos: [u8; CAMINHOS],
    #[serde(default)]
    slots: [Option<u8>; SLOTS_EXTRAS],
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
}

impl Passivas {
    fn somar(&mut self, e: Efeito, v: f32) {
        let campo = match e {
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
        };
        *campo += v;
    }

    /// The economy share on top of the accessories' bonus.
    pub fn somar_economia(&self, b: &mut crate::acessorios::Bonus) {
        b.drop += self.drop;
        b.ouro += self.ouro;
        b.minerio += self.minerio;
        b.energia += self.energia;
        b.darksteel += self.darksteel;
        b.loot_melhor += self.loot_melhor;
    }
}

/// What the client asks of the tree (`ClientMessage::Arvore`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum PedidoArvore {
    /// Send me the state (`ServerMessage::Arvore`).
    Painel,
    /// Buy node `no` (0-based, must be the path's next) of path `caminho`.
    Gastar { caminho: u8, no: u8 },
    /// Reset every point for gold (`custo_de_redistribuir`).
    Redistribuir,
    /// Reset every point for Rebirth Essence instead
    /// (`essencia::ESSENCIA_REDISTRIBUIR`, rebirth phase 3; also the Essence
    /// shop's first offer).
    RedistribuirComEssencia,
    /// Put learned active `ativa` (an `ATIVAS` id) in extra skill slot
    /// `slot` (0 = skill 4); `None` empties the slot (rebirth phase 4).
    Equipar { slot: u8, ativa: Option<u8> },
}

/// The tree's state for the client (`ServerMessage::Arvore`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct EstadoArvore {
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

    fn cheio(c: Caminho) -> Arvore {
        let mut a = Arvore::default();
        a.nos[c as usize] = NOS_POR_CAMINHO;
        a
    }

    #[test]
    fn pontos_a_partir_do_primeiro_renascimento() {
        assert_eq!(pontos_ganhos(0, 100), 0, "none before the first rebirth");
        assert_eq!(pontos_ganhos(1, 1), 0);
        assert_eq!(pontos_ganhos(1, 9), 0);
        assert_eq!(pontos_ganhos(1, 10), 1);
        assert_eq!(pontos_ganhos(1, 59), 5);
        assert_eq!(pontos_ganhos(1, 100), 10);
        assert_eq!(pontos_ganhos(2, 1), 10, "a rebirth keeps the climb's points");
        assert_eq!(pontos_ganhos(3, 35), 23);
        assert_eq!(pontos_ganhos(1, 500), 10, "the shown level caps at 100");
        // Never goes down across a rebirth: the cap of climb r = the start of r+1.
        for r in 1..20 {
            assert_eq!(pontos_ganhos(r, 100), pontos_ganhos(r + 1, 1));
        }
        // Six climbs fill all four paths (60 nodes).
        assert_eq!(pontos_ganhos(6, 100), CAMINHOS as u32 * NOS_POR_CAMINHO as u32);
    }

    #[test]
    fn gastar_na_ordem_e_no_orcamento() {
        let mut a = Arvore::default();
        assert_eq!(a.gastar(0, 0, 0), Err(SEM_PONTOS));
        assert_eq!(a.gastar(0, 1, 3), Err(FORA_DE_ORDEM), "node 2 before node 1");
        assert!(a.gastar(0, 0, 3).is_ok());
        assert_eq!(a.gastar(0, 0, 3), Err(FORA_DE_ORDEM), "a double tap does not buy twice");
        assert!(a.gastar(0, 1, 3).is_ok());
        assert!(a.gastar(3, 0, 3).is_ok(), "points spread across paths");
        assert_eq!(a.gastar(1, 0, 3), Err(SEM_PONTOS));
        assert_eq!(a.gastar(9, 0, 30), Err(CAMINHO_INVALIDO));
        assert_eq!(a.nos, [2, 0, 0, 1]);
        assert_eq!((a.gastos(), a.livres(3), a.livres(10)), (3, 0, 7));
        assert!(a.tem(Caminho::Assassino, 1) && !a.tem(Caminho::Assassino, 2));
        let mut c = cheio(Caminho::Mago);
        assert_eq!(c.gastar(1, 15, 60), Err(CAMINHO_COMPLETO));
    }

    #[test]
    fn validar_corta_o_excesso_pelo_fim() {
        let a = Arvore::com_nos([200, 3, 0, 1]);
        assert_eq!(a.validada(100).nos, [15, 3, 0, 1], "no path past 15");
        assert_eq!(Arvore::com_nos([5, 5, 0, 2]).validada(9).nos, [4, 3, 0, 2]);
        assert_eq!(Arvore::com_nos([5, 5, 0, 2]).validada(0), Arvore::default());
        let ok = Arvore::com_nos([4, 0, 6, 0]);
        assert_eq!(ok.validada(10), ok, "a valid one stays as it is");
    }

    #[test]
    fn json_ida_e_volta() {
        let a = Arvore::com_nos([3, 0, 15, 2]);
        assert_eq!(a.para_json(), "[3,0,15,2]");
        assert_eq!(Arvore::de_json(&a.para_json()), a);
        assert_eq!(Arvore::de_json(""), Arvore::default());
        assert_eq!(Arvore::de_json("lixo"), Arvore::default());
        assert_eq!(Arvore::de_json("[1,2]"), Arvore::default());
    }

    #[test]
    fn ativas_nos_nos_5_10_e_15() {
        for c in Caminho::TODOS {
            for k in 0..NOS_POR_CAMINHO {
                let no = no(c, k).unwrap();
                let deve = matches!(k + 1, 5 | 10 | 15);
                assert_eq!(no.ativa.is_some(), deve, "{c:?} node {}", k + 1);
                if let Some(id) = no.ativa {
                    assert_eq!(ATIVAS[id as usize].0, c, "the active belongs to its path");
                }
                assert!(no.valor > 0.0);
            }
        }
        let mut ids: Vec<u8> = NOS.iter().flatten().filter_map(|n| n.ativa).collect();
        ids.sort();
        assert_eq!(ids, (0..12).collect::<Vec<u8>>(), "every active once");
    }

    fn perto(a: f32, b: f32) -> bool {
        (a - b).abs() < 1e-4
    }

    #[test]
    fn totais_de_cada_caminho() {
        let p = cheio(Caminho::Assassino).passivas();
        assert!(perto(p.critico, 0.05) && perto(p.velocidade_de_ataque, 0.08));
        assert!(perto(p.destreza_pct, 0.12) && perto(p.movimento, 0.04) && perto(p.esquiva, 0.05));
        let p = cheio(Caminho::Mago).passivas();
        assert!(perto(p.sabedoria_pct, 0.16) && perto(p.mana_pct, 0.20));
        assert!(perto(p.ataque_pct, 0.08) && perto(p.regen_mana, 1.5));
        let p = cheio(Caminho::Guardiao).passivas();
        assert!(perto(p.defesa_pct, 0.20) && perto(p.vida_pct, 0.16));
        assert!(perto(p.reducao, 0.04) && perto(p.regen_vida, 2.0) && perto(p.firmeza, 25.0));
        let p = cheio(Caminho::Corsario).passivas();
        assert!(perto(p.drop, 0.16) && perto(p.ouro, 0.20) && perto(p.minerio, 0.15));
        assert!(perto(p.energia, 0.15) && perto(p.darksteel, 0.15) && perto(p.loot_melhor, 0.02));
        // Nothing leaks across paths.
        assert_eq!(cheio(Caminho::Corsario).passivas().defesa_pct, 0.0);
        assert_eq!(Arvore::default().passivas(), Passivas::default());
        // Partial: the first 3 Guardian nodes.
        let p = Arvore::com_nos([0, 0, 3, 0]).passivas();
        assert!(perto(p.defesa_pct, 0.04) && perto(p.vida_pct, 0.04) && perto(p.regen_vida, 1.0));
        let mut b = crate::acessorios::Bonus::default();
        cheio(Caminho::Corsario).passivas().somar_economia(&mut b);
        assert!(perto(b.drop, 0.16) && perto(b.ouro, 0.20));
    }

    #[test]
    fn redistribuir_cresce_com_os_pontos() {
        assert_eq!(custo_de_redistribuir(0), 0);
        assert_eq!(custo_de_redistribuir(1), 1_000);
        assert_eq!(custo_de_redistribuir(37), 37_000);
        let e = EstadoArvore::de(Arvore::com_nos([2, 0, 0, 1]), 10);
        assert_eq!((e.livres(), e.custo_redistribuir), (7, 3_000));
    }

    #[test]
    fn numeros_dos_nos() {
        assert_eq!(Efeito::Critico.numero(0.01), "1");
        assert_eq!(Efeito::Minerio.numero(0.075), "7.5");
        assert_eq!(Efeito::RegenMana.numero(0.5), "0.5");
        assert_eq!(Efeito::Firmeza.numero(25.0), "25");
        for c in Caminho::TODOS {
            for no in &NOS[c as usize] {
                assert_eq!(no.efeito.modelo().matches("{}").count(), 1);
            }
        }
    }

    // ── phase 4: actives in slots 4–6 ──

    #[test]
    fn slots_abrem_nos_renascimentos_1_3_e_5() {
        let esperado = [(0, 0), (1, 1), (2, 1), (3, 2), (4, 2), (5, 3), (9, 3), (40, 3)];
        for (r, n) in esperado {
            assert_eq!(slots_abertos(r), n, "rebirth {r}");
        }
        // Only an open slot takes an active.
        let mut a = cheio(Caminho::Assassino);
        assert_eq!(a.equipar(0, Some(1), 0), Err(SLOT_FECHADO));
        assert!(a.equipar(0, Some(1), 1).is_ok());
        assert_eq!(a.equipar(1, Some(0), 2), Err(SLOT_FECHADO));
        assert!(a.equipar(1, Some(0), 3).is_ok());
        assert_eq!(a.equipar(2, Some(2), 4), Err(SLOT_FECHADO));
        assert!(a.equipar(2, Some(2), 5).is_ok());
        assert_eq!(a.equipar(3, Some(2), 99), Err(SLOT_FECHADO), "there is no 7th slot");
        // A slot past the open ones is not usable, even if stored.
        assert_eq!(a.ativa_no_slot(2, 5), Some(2));
        assert_eq!(a.ativa_no_slot(2, 4), None);
    }

    #[test]
    fn so_equipa_ativa_aprendida() {
        let mut a = Arvore::com_nos([4, 0, 0, 0]);
        assert_eq!(a.equipar(0, Some(0), 5), Err(ATIVA_NAO_APRENDIDA), "node 5 not bought");
        a.nos[0] = 5;
        assert!(a.possui_ativa(0) && !a.possui_ativa(1) && !a.possui_ativa(3));
        assert_eq!(a.ativas_possuidas(), vec![0]);
        assert!(a.equipar(0, Some(0), 5).is_ok());
        assert_eq!(a.equipar(1, Some(3), 5), Err(ATIVA_NAO_APRENDIDA), "another path's active");
        assert_eq!(a.equipar(1, Some(77), 5), Err(ATIVA_NAO_APRENDIDA), "unknown id");
        // Equipping the same active elsewhere MOVES it.
        assert!(a.equipar(2, Some(0), 5).is_ok());
        assert_eq!(a.slots, [None, None, Some(0)]);
        assert!(a.equipar(2, None, 5).is_ok(), "empty a slot");
        assert_eq!(a.slots, [None; SLOTS_EXTRAS]);
        // Losing the node (reset, budget cut) empties the slot.
        a.nos = [15, 0, 0, 0];
        a.equipar(0, Some(2), 1).unwrap();
        let v = a.validada(12);
        assert_eq!(v.nos[0], 12);
        assert_eq!(v.slots[0], None, "Death Mark (node 15) went with the cut");
    }

    #[test]
    fn comprar_ativa_ocupa_o_proximo_slot_aberto() {
        let mut a = Arvore::com_nos([5, 0, 0, 0]);
        assert_eq!(a.encaixar_nova(0, 0), None, "no slot before the first rebirth");
        assert_eq!(a.encaixar_nova(0, 1), Some(0));
        assert_eq!(a.encaixar_nova(0, 1), None, "already in a slot");
        a.nos[1] = 5;
        assert_eq!(a.encaixar_nova(3, 1), None, "the only open slot is taken");
        // A rebirth that opens slot 5 fills it.
        assert_eq!(a.preencher_slots(3), 1);
        assert_eq!(a.slots, [Some(0), Some(3), None]);
    }

    #[test]
    fn json_antigo_da_arvore_carrega() {
        // Phase 2 rows: the bare array, no slots.
        let a = Arvore::de_json("[5,0,10,3]");
        assert_eq!(a.nos, [5, 0, 10, 3]);
        assert_eq!(a.slots, [None; SLOTS_EXTRAS]);
        assert_eq!(Arvore::de_json(" [1,2,3,4] ").nos, [1, 2, 3, 4]);
        // With no slot filled it still writes the old shape.
        assert_eq!(a.para_json(), "[5,0,10,3]");
        // Phase 4 rows round-trip.
        let mut b = a;
        b.slots = [Some(0), None, Some(7)];
        let j = b.para_json();
        assert!(j.starts_with('{'), "{j}");
        assert_eq!(Arvore::de_json(&j), b);
        // A row without "slots", or with junk ids, still loads.
        assert_eq!(Arvore::de_json(r#"{"nos":[1,0,0,0]}"#), Arvore::com_nos([1, 0, 0, 0]));
        assert_eq!(Arvore::de_json(r#"{"nos":[5,0,0,0],"slots":[0,99,null]}"#).slots, [Some(0), None, None]);
        // The wire form (postcard) did not grow: slots travel in the state.
        let fio = postcard::to_allocvec(&b).unwrap();
        assert_eq!(fio, postcard::to_allocvec(&Arvore::com_nos(b.nos)).unwrap());
        let e = EstadoArvore::de(b, 20).com_renascimentos(3);
        assert_eq!((e.slots, e.slots_abertos, e.renascimentos), (b.slots, 2, 3));
        let volta: EstadoArvore = postcard::from_bytes(&postcard::to_allocvec(&e).unwrap()).unwrap();
        assert_eq!(volta.slots, b.slots);
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
        // Assassin path: node 10 is Invisibility.
        assert_eq!(DEFS[1].tipo, TipoAtiva::Invisibilidade);
        assert_eq!(no_da_ativa(1), Some((Caminho::Assassino, 9)));
        // Supreme: only capstones, only from rebirth 10.
        assert!(!e_suprema(2, 9) && e_suprema(2, 10));
        assert!(!e_suprema(0, 50), "node 5 is never supreme");
        assert!((fator(2, 10) - DEFS[2].fator * MULT_SUPREMA).abs() < 1e-6);
        assert!((bonus_de_saque(11, 10).ouro - 0.375).abs() < 1e-6);
        assert_eq!(bonus_de_saque(3, 10), crate::acessorios::Bonus::default());
        assert!((bonus_de_saque(9, 1).drop - 0.20).abs() < 1e-6);
    }
}
