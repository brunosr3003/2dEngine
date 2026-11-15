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
//! (`No::ativa`): the actives themselves are phase 4 (they will take skill
//! slots 4–6), and for now those nodes give their passive like any other.
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
/// first `nos[c]` of path `c`, since the paths are linear).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Arvore {
    pub nos: [u8; CAMINHOS],
}

/// Why spending a point was refused (English: the client translates).
pub const SEM_PONTOS: &str = "No free path points.";
pub const CAMINHO_COMPLETO: &str = "This path is complete.";
pub const FORA_DE_ORDEM: &str = "Buy the path's previous node first.";
pub const CAMINHO_INVALIDO: &str = "Unknown path.";

impl Arvore {
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
        self
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

    /// `characters.arvore_json`: `"[a,b,c,d]"`.
    pub fn para_json(&self) -> String {
        serde_json::to_string(&self.nos).unwrap_or_default()
    }

    /// From `para_json`; empty or broken = nothing allocated. Not validated
    /// against the budget: `validada` does that with the character's level.
    pub fn de_json(s: &str) -> Arvore {
        serde_json::from_str::<[u8; CAMINHOS]>(s.trim())
            .map(|nos| Arvore { nos })
            .unwrap_or_default()
    }
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
}

/// The tree's state for the client (`ServerMessage::Arvore`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct EstadoArvore {
    pub arvore: Arvore,
    /// Points earned so far (`pontos_ganhos`).
    pub ganhos: u32,
    /// Gold to reset now (`custo_de_redistribuir`).
    pub custo_redistribuir: u64,
}

impl EstadoArvore {
    pub fn de(arvore: Arvore, ganhos: u32) -> EstadoArvore {
        EstadoArvore { arvore, ganhos, custo_redistribuir: custo_de_redistribuir(arvore.gastos()) }
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
        let a = Arvore { nos: [200, 3, 0, 1] };
        assert_eq!(a.validada(100).nos, [15, 3, 0, 1], "no path past 15");
        assert_eq!(Arvore { nos: [5, 5, 0, 2] }.validada(9).nos, [4, 3, 0, 2]);
        assert_eq!(Arvore { nos: [5, 5, 0, 2] }.validada(0), Arvore::default());
        let ok = Arvore { nos: [4, 0, 6, 0] };
        assert_eq!(ok.validada(10), ok, "a valid one stays as it is");
    }

    #[test]
    fn json_ida_e_volta() {
        let a = Arvore { nos: [3, 0, 15, 2] };
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
        let p = Arvore { nos: [0, 0, 3, 0] }.passivas();
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
        let e = EstadoArvore::de(Arvore { nos: [2, 0, 0, 1] }, 10);
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
}
