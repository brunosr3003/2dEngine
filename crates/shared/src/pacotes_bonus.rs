//! BONUS PACKS: cent-priced flash offers that pop up when something happens
//! to the player (docs/OFERTAS_RELAMPAGO.md, "Pacotes bônus").
//!
//! The owner (15/11/2026): "packs in cents that show up when the player dies
//! … at the first dungeon, when he dies or can't make it … then as he levels
//! he keeps getting bonus packs … the promotions are always: the amount of TP
//! that purchase would give + some bonus (item, gold, drop, summon…)".
//!
//! They ride on the flash-offer machinery (`ofertas_relampago` rows, the same
//! Play products `oferta_<price>`, the same purchase and delivery). This file
//! is the pure part: the starting catalogue, THE RULE (TP worth of the price
//! + a bonus), the triggers and which blue pieces a gear pack builds.

use crate::components::Equipment;
use crate::constants::{item_id, EquipSlot};
use crate::forja::Grau;
use crate::skills::Conjunto;

/// What `centavos` buys in the TP shop (`loja::PACOTES`), rounded down: at
/// the rate of the biggest TP pack the money reaches (its bonus included),
/// or of the smallest pack below its price. This is "the TP that purchase
/// would give" — every bonus pack gives at least this much TP.
pub fn tp_do_preco(centavos: u32) -> u64 {
    let mut pacotes: Vec<&crate::loja::PacoteTp> = crate::loja::PACOTES.iter().collect();
    pacotes.sort_by_key(|p| p.centavos);
    let Some(menor) = pacotes.first() else {
        return 0;
    };
    let base = pacotes
        .iter()
        .rev()
        .find(|p| p.centavos <= centavos)
        .copied()
        .unwrap_or(menor);
    (centavos as u64 * base.total()) / base.centavos.max(1) as u64
}

/// The least TP any flash offer gives (owner, 15/11/2026: "every flash offer
/// has to give the same TP that purchase would normally give, 400 being the
/// minimum, plus the offer's bonus").
pub const TP_MINIMO_OFERTA: u64 = 400;

/// THE RULE for EVERY flash offer (level, campaign, bonus pack): the TP its
/// price buys in the TP shop, never under `TP_MINIMO_OFERTA`. The offer's
/// bonus (gold, Energy, items, gear, bonus TP) comes on top.
pub fn tp_base_da_oferta(centavos: u32) -> u64 {
    tp_do_preco(centavos).max(TP_MINIMO_OFERTA)
}

/// The TP an `ofertas_relampago` row really gives: what is written there,
/// never under the rule. The server shows and credits THIS, so a row edited
/// by hand (or a Panopticon campaign) can't go under it.
pub fn tp_da_oferta(centavos: u32, tp_escrita: u64) -> u64 {
    tp_escrita.max(tp_base_da_oferta(centavos))
}

/// A bonus pack's TP: the rule plus the pack's bonus TP.
pub fn tp_do_pacote(centavos: u32, tp_bonus: u64) -> u64 {
    tp_base_da_oferta(centavos) + tp_bonus
}

/// What `tp` TP costs in money at the TP shop's BEST rate (the pack with the
/// fewest cents per TP), in cents, rounded down: the lowest honest price of
/// that TP.
pub fn centavos_do_tp(tp: u64) -> u64 {
    let Some(melhor) = crate::loja::PACOTES
        .iter()
        .filter(|p| p.total() > 0)
        .min_by(|a, b| (a.centavos as u64 * b.total()).cmp(&(b.centavos as u64 * a.total())))
    else {
        return 0;
    };
    tp * melhor.centavos as u64 / melhor.total()
}

/// What an offer's contents cost at the TP shop, in TP, the cheapest way
/// (`precos::custos_em_tp`, gold by the Sack of Gold): the TP itself, gold,
/// Energy and every item the shop sells. Items the shop doesn't sell and
/// built gear count ZERO, so the number can only be low.
pub fn valor_em_tp(tp: u64, ouro: u64, energia: u64, itens: &[(u16, u32)]) -> u64 {
    let custos = crate::precos::custos_em_tp();
    let custo = |id: u16| custos.iter().find(|(i, _)| *i == id).map_or(0.0, |(_, c)| *c);
    let mut v = tp as f64;
    let ouro_por_tp = crate::precos::ouro_por_tp_da_loja();
    if ouro_por_tp > 0.0 {
        v += ouro as f64 / ouro_por_tp;
    }
    v += energia as f64 / crate::mercado::ENERGIA_POR_LOTE as f64 * custo(item_id::ENERGIA_MIL);
    for (id, q) in itens {
        v += custo(*id) * *q as f64;
    }
    v.max(0.0).floor() as u64
}

/// The "worth" of an offer in cents: its contents' TP-shop value
/// (`valor_em_tp`) at the TP shop's best rate (`centavos_do_tp`). What the
/// Offers tab crosses out and turns into "×N VALUE".
pub fn valor_em_centavos(tp: u64, ouro: u64, energia: u64, itens: &[(u16, u32)]) -> u64 {
    centavos_do_tp(valor_em_tp(tp, ouro, energia, itens))
}

/// What fires a triggered pack. Stored as text in `ofertas_relampago.gatilho`
/// (NULL there = an ordinary level/campaign offer).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Gatilho {
    /// Died, left or ran out of time in the FIRST Porão of the story
    /// (`PORAO_INICIAL`, "Shipwreck Cellar") without winning.
    PoraoInicial,
    /// Went down anywhere (field, dungeon, PvP).
    Morte,
}

impl Gatilho {
    pub fn chave(self) -> &'static str {
        match self {
            Gatilho::PoraoInicial => "porao_inicial",
            Gatilho::Morte => "morte",
        }
    }

    pub fn de_chave(s: &str) -> Option<Gatilho> {
        [Gatilho::PoraoInicial, Gatilho::Morte].into_iter().find(|g| g.chave() == s)
    }
}

/// The dungeon whose failure opens the starter pack (`dungeon::CONTEUDOS`).
pub const PORAO_INICIAL: u16 = 1;

/// A triggered pack stays open this long (minutes).
pub const DURACAO_MIN: i32 = 45;

/// Two triggered packs never open closer than this (minutes, from the
/// opening of the previous one): dying again right after closing the starter
/// does not pop the next one at once.
pub const INTERVALO_ENTRE_GATILHOS_MIN: i32 = 120;

/// Which gear a pack builds at the moment it opens, BLUE at the player's level.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pecas {
    Nenhuma,
    /// The player's current weapon type.
    Arma,
    /// Weapon + offhand + armor + the four accessories.
    Conjunto,
}

impl Pecas {
    pub fn chave(self) -> &'static str {
        match self {
            Pecas::Nenhuma => "",
            Pecas::Arma => "arma",
            Pecas::Conjunto => "conjunto",
        }
    }

    pub fn de_chave(s: &str) -> Pecas {
        match s.trim() {
            "arma" => Pecas::Arma,
            "conjunto" => Pecas::Conjunto,
            _ => Pecas::Nenhuma,
        }
    }
}

/// The armor a weapon set wears when the player has none on (same table as
/// the PvP archetypes).
pub fn armadura_do_conjunto(c: Conjunto) -> u16 {
    match c {
        Conjunto::EspadaEscudo => item_id::ARMADURA_PESADA,
        Conjunto::Katana => item_id::ARMADURA_MEDIA,
        Conjunto::Pistolas | Conjunto::AnelMagico => item_id::ARMADURA_LEVE,
    }
}

/// The blue pieces (`(slot, item_id, item level)`) a pack builds for this
/// equipment at internal level `nivel`. The weapon keeps the player's weapon
/// TYPE (empty hands = sword and shield); the set takes the weapon's offhand,
/// the armor he wears (or the set's), and the four accessories. Item level as
/// a chest piece of that grade (`dungeon::nivel_da_peca`).
pub fn pecas_azuis(pecas: Pecas, equip: &Equipment, nivel: u32) -> Vec<(EquipSlot, u16, u16)> {
    let ilvl = crate::dungeon::nivel_da_peca(Grau::Raro, nivel);
    let arma = equip.weapon.unwrap_or(item_id::ESPADA_E_ESCUDO);
    let conjunto = Conjunto::da_arma(arma);
    match pecas {
        Pecas::Nenhuma => Vec::new(),
        Pecas::Arma => vec![(EquipSlot::Weapon, conjunto.arma(), ilvl)],
        Pecas::Conjunto => vec![
            (EquipSlot::Weapon, conjunto.arma(), ilvl),
            (EquipSlot::Offhand, conjunto.secundaria(), ilvl),
            (
                EquipSlot::Armor,
                equip.armor.unwrap_or_else(|| armadura_do_conjunto(conjunto)),
                ilvl,
            ),
            (EquipSlot::Earring, item_id::BRINCO, ilvl),
            (EquipSlot::Necklace, item_id::AMULETO, ilvl),
            (EquipSlot::Bracelet, item_id::BRACELETE, ilvl),
            (EquipSlot::Belt, item_id::CINTO, ilvl),
        ],
    }
}

/// A test bot's character (`jogadorbot` names them `bot<digits>…`, `lojabot`
/// logs in as `bot0`). Bots never get a triggered pack.
pub fn e_bot_de_teste(nome: &str) -> bool {
    let n = nome.as_bytes();
    n.len() > 3 && n[..3].eq_ignore_ascii_case(b"bot") && n[3].is_ascii_digit()
}

/// The internal level at which a pack meant for SHOWN level `exibido` opens:
/// the first whole internal level that shows at least that number.
pub fn nivel_interno_do_exibido(exibido: u32) -> u32 {
    let mut i = 1;
    while crate::progressao::nivel_exibido_de(i) < exibido && i < 500 {
        i += 1;
    }
    i
}

/// One pack of the starting catalogue. The server seeds `ofertas_relampago`
/// from this once (ON CONFLICT DO NOTHING: the database is the truth after).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PacoteBonus {
    pub id: &'static str,
    pub titulo: &'static str,
    pub titulo_pt: &'static str,
    pub gatilho: Option<Gatilho>,
    /// SHOWN level: the minimum for a triggered pack, the opening level of a
    /// level pack. Converted with `nivel_interno_do_exibido`.
    pub nivel_exibido: u32,
    pub produto_play: &'static str,
    pub preco_centavos: u32,
    pub valor_de_centavos: u32,
    /// TP on top of what the price buys (`tp_do_pacote`).
    pub tp_bonus: u64,
    pub ouro: u64,
    pub energia: u64,
    pub itens: &'static [(u16, u32)],
    pub pecas: Pecas,
}

impl PacoteBonus {
    /// TP credited: the rule.
    pub fn tp(&self) -> u64 {
        tp_do_pacote(self.preco_centavos, self.tp_bonus)
    }

    pub fn nivel(&self) -> u32 {
        nivel_interno_do_exibido(self.nivel_exibido)
    }

    /// Something on top of the TP: bonus TP, gold, Energy, items or gear.
    pub fn tem_bonus(&self) -> bool {
        self.tp_bonus > 0
            || self.ouro > 0
            || self.energia > 0
            || !self.itens.is_empty()
            || self.pecas != Pecas::Nenhuma
    }

    pub fn itens_json(&self) -> String {
        let v: Vec<String> = self
            .itens
            .iter()
            .map(|(id, q)| format!(r#"{{"item_id":{id},"qtd":{q}}}"#))
            .collect();
        format!("[{}]", v.join(","))
    }
}

use item_id::{DARKSTEEL, PERGAMINHO_INVOCA_CHAVE as CHAVE, PERGAMINHO_INVOCA_MONTARIA as MONTARIA, PERGAMINHO_INVOCA_PET as PET};

/// The starting catalogue. Prices reuse the Play products `oferta_<cents>`
/// (`oferta_099`, `oferta_499` and `oferta_1500` already exist; `oferta_199`
/// and `oferta_999` are new — see docs/OFERTAS_RELAMPAGO.md).
pub const PACOTES_BONUS: &[PacoteBonus] = &[
    // The owner's own numbers: 400 TP + 10,000 gold + 10,000 Energy +
    // 10,000 Darksteel for 0.99.
    PacoteBonus {
        id: "bonus_porao_inicial",
        titulo: "Starter Pack",
        titulo_pt: "Pacote Inicial",
        gatilho: Some(Gatilho::PoraoInicial),
        nivel_exibido: 1,
        produto_play: "oferta_099",
        preco_centavos: 99,
        valor_de_centavos: 1990,
        tp_bonus: 0,
        ouro: 10_000,
        energia: 10_000,
        itens: &[(DARKSTEEL, 10_000)],
        pecas: Pecas::Nenhuma,
    },
    // Death: his weapon, blue, at his level + gold. Shown with the power gain.
    PacoteBonus {
        id: "bonus_morte_arma",
        titulo: "Blue Weapon Pack",
        titulo_pt: "Pacote Arma Azul",
        gatilho: Some(Gatilho::Morte),
        nivel_exibido: 10,
        produto_play: "oferta_199",
        preco_centavos: 199,
        valor_de_centavos: 2490,
        tp_bonus: 0,
        ouro: 20_000,
        energia: 0,
        itens: &[],
        pecas: Pecas::Arma,
    },
    // A later death: the whole set in blue + TP + gold.
    PacoteBonus {
        id: "bonus_morte_conjunto",
        titulo: "Full Blue Set",
        titulo_pt: "Conjunto Azul Completo",
        gatilho: Some(Gatilho::Morte),
        nivel_exibido: 20,
        produto_play: "oferta_499",
        preco_centavos: 499,
        valor_de_centavos: 7990,
        tp_bonus: 0,
        ouro: 30_000,
        energia: 0,
        itens: &[],
        pecas: Pecas::Conjunto,
    },
    // LEVEL PACKS. The old level offers (internal 5/15/25 = shown ~9/30/~43)
    // stay; these fill shown 20, 50, 70 and 100.
    PacoteBonus {
        id: "bonus_nivel_20",
        titulo: "Level 20 Pack",
        titulo_pt: "Pacote Nível 20",
        gatilho: None,
        nivel_exibido: 20,
        produto_play: "oferta_199",
        preco_centavos: 199,
        valor_de_centavos: 1990,
        tp_bonus: 0,
        ouro: 15_000,
        energia: 0,
        itens: &[(MONTARIA, 1)],
        pecas: Pecas::Nenhuma,
    },
    PacoteBonus {
        id: "bonus_nivel_50",
        titulo: "Level 50 Pack",
        titulo_pt: "Pacote Nível 50",
        gatilho: None,
        nivel_exibido: 50,
        produto_play: "oferta_499",
        preco_centavos: 499,
        valor_de_centavos: 3990,
        tp_bonus: 0,
        ouro: 0,
        energia: 20_000,
        itens: &[(PET, 1), (CHAVE, 2), (DARKSTEEL, 5_000)],
        pecas: Pecas::Nenhuma,
    },
    PacoteBonus {
        id: "bonus_nivel_70",
        titulo: "Level 70 Pack",
        titulo_pt: "Pacote Nível 70",
        gatilho: None,
        nivel_exibido: 70,
        produto_play: "oferta_999",
        preco_centavos: 999,
        valor_de_centavos: 7990,
        tp_bonus: 0,
        ouro: 0,
        energia: 50_000,
        itens: &[(MONTARIA, 2), (CHAVE, 3), (DARKSTEEL, 20_000)],
        pecas: Pecas::Nenhuma,
    },
    PacoteBonus {
        id: "bonus_nivel_100",
        titulo: "Level 100 Pack",
        titulo_pt: "Pacote Nível 100",
        gatilho: None,
        nivel_exibido: 100,
        produto_play: "oferta_1500",
        preco_centavos: 1500,
        valor_de_centavos: 11990,
        tp_bonus: 0,
        ouro: 100_000,
        energia: 0,
        itens: &[(PET, 2), (MONTARIA, 2), (CHAVE, 5), (DARKSTEEL, 50_000)],
        pecas: Pecas::Nenhuma,
    },
];

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn o_tp_do_preco_segue_a_tabela_de_pacotes() {
        // 100 TP for 4.90: 0.99 buys 20, 4.99 buys 101.
        assert_eq!(tp_do_preco(490), 100);
        assert_eq!(tp_do_preco(99), 20);
        assert_eq!(tp_do_preco(499), 101);
        // The biggest pack the money reaches sets the rate (bonus included).
        assert_eq!(tp_do_preco(9990), 2600);
        assert_eq!(tp_do_preco(0), 0);
        // More money never buys less TP.
        let mut antes = 0;
        for c in (0..20_000).step_by(7) {
            let t = tp_do_preco(c);
            assert!(t >= antes, "{c} centavos compra {t} < {antes}");
            antes = t;
        }
    }

    #[test]
    fn toda_oferta_da_ao_menos_400_tp_e_o_tp_do_preco() {
        // Every Play product an offer can use, and some prices beyond.
        for c in [99, 199, 499, 999, 1500, 2490, 4990, 9990, 19_990] {
            let t = tp_base_da_oferta(c);
            assert!(t >= TP_MINIMO_OFERTA, "{c}: {t}");
            assert!(t >= tp_do_preco(c), "{c}: {t} < {}", tp_do_preco(c));
            // A row written lower is lifted; a row written higher stays.
            assert_eq!(tp_da_oferta(c, 0), t);
            assert_eq!(tp_da_oferta(c, t + 77), t + 77);
        }
        assert_eq!(tp_base_da_oferta(99), 400, "0.99 gives the minimum");
        assert_eq!(tp_base_da_oferta(19_990), 5202, "big money buys what the shop sells");
        for p in PACOTES_BONUS {
            assert!(p.tp() >= TP_MINIMO_OFERTA, "{}", p.id);
            assert!(p.tp() >= tp_do_preco(p.preco_centavos), "{}", p.id);
            assert_eq!(p.tp(), tp_da_oferta(p.preco_centavos, p.tp()), "{}", p.id);
        }
    }

    #[test]
    fn o_valor_e_o_da_loja_de_tp_e_nunca_inventado() {
        // TP alone at the best pack's rate (2,600 TP for 99.90).
        assert_eq!(centavos_do_tp(2600), 9990);
        assert_eq!(centavos_do_tp(400), 1536);
        assert_eq!(valor_em_tp(400, 0, 0, &[]), 400);
        // Gold by the Sack of Gold (10,000 for 50 TP).
        assert_eq!(valor_em_tp(0, 10_000, 0, &[]), 50);
        // An item the shop doesn't sell is worth nothing here.
        assert_eq!(valor_em_tp(0, 0, 0, &[(item_id::GOLD, 5)]), 0);
        let p = &PACOTES_BONUS[0];
        let v = valor_em_tp(p.tp(), p.ouro, p.energia, p.itens);
        assert!(v > p.tp() && v < 2 * p.tp() + 1000, "starter pack worth {v} TP");
        assert!(valor_em_centavos(p.tp(), p.ouro, p.energia, p.itens) > p.preco_centavos as u64 * 10);
    }

    #[test]
    fn todo_pacote_e_o_tp_do_preco_mais_um_bonus() {
        let mut ids = Vec::new();
        for p in PACOTES_BONUS {
            assert!(p.tp() >= tp_do_preco(p.preco_centavos), "{}", p.id);
            assert!(p.tem_bonus(), "{} sem bonus", p.id);
            assert!(p.produto_play == format!("oferta_{:03}", p.preco_centavos), "{}: produto {} nao bate com o preco", p.id, p.produto_play);
            assert!(p.valor_de_centavos > p.preco_centavos, "{}", p.id);
            assert!(!ids.contains(&p.id), "id repetido {}", p.id);
            ids.push(p.id);
            assert!(p.itens_json().starts_with('['));
        }
    }

    #[test]
    fn o_pacote_inicial_e_o_do_dono() {
        let p = PACOTES_BONUS.iter().find(|p| p.gatilho == Some(Gatilho::PoraoInicial)).unwrap();
        assert_eq!(p.preco_centavos, 99);
        assert_eq!(p.tp(), 400);
        assert_eq!((p.ouro, p.energia), (10_000, 10_000));
        assert_eq!(p.itens, &[(DARKSTEEL, 10_000)]);
        assert_eq!(crate::dungeon::conteudo(PORAO_INICIAL).map(|c| c.nome), Some("Shipwreck Cellar"));
    }

    #[test]
    fn as_pecas_sao_azuis_e_da_arma_do_jogador() {
        let mut e = Equipment::default();
        e.weapon = Some(item_id::KATANA);
        let a = pecas_azuis(Pecas::Arma, &e, 8);
        assert_eq!(a.len(), 1);
        assert_eq!(a[0].1, item_id::KATANA);
        assert_eq!(crate::items::tier_from_ilvl(a[0].2), 3, "azul");
        let c = pecas_azuis(Pecas::Conjunto, &e, 40);
        assert_eq!(c.len(), 7);
        assert_eq!(c[1].1, item_id::BAINHA);
        assert_eq!(c[2].1, item_id::ARMADURA_MEDIA);
        assert!(c.iter().all(|(_, _, l)| crate::items::tier_from_ilvl(*l) == 3));
        assert!(pecas_azuis(Pecas::Nenhuma, &e, 8).is_empty());
        assert_eq!(pecas_azuis(Pecas::Arma, &Equipment::default(), 8)[0].1, item_id::ESPADA_E_ESCUDO);
    }

    #[test]
    fn niveis_exibidos_viram_internos() {
        for p in PACOTES_BONUS {
            let n = p.nivel();
            assert!(crate::progressao::nivel_exibido_de(n) >= p.nivel_exibido, "{}", p.id);
            assert!(n == 1 || crate::progressao::nivel_exibido_de(n - 1) < p.nivel_exibido, "{}", p.id);
        }
        assert_eq!(nivel_interno_do_exibido(100), 60);
    }

    #[test]
    fn chaves_e_bots() {
        for g in [Gatilho::PoraoInicial, Gatilho::Morte] {
            assert_eq!(Gatilho::de_chave(g.chave()), Some(g));
        }
        for p in [Pecas::Nenhuma, Pecas::Arma, Pecas::Conjunto] {
            assert_eq!(Pecas::de_chave(p.chave()), p);
        }
        assert!(e_bot_de_teste("bot0"));
        assert!(e_bot_de_teste("bot1234_3"));
        assert!(!e_bot_de_teste("Bottom"));
        assert!(!e_bot_de_teste("bot"));
    }
}
