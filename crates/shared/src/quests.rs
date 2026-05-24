//! Sistema de quests — modelo de dados compartilhado (server + client via wire).
//!
//! 3 fontes de quest (`quest_source`):
//!   - BOARD  : quadro de requests no centro da cidade (recurso/mob/farm,
//!              repetível; alimenta o upgrade de cidade futuro).
//!   - NPC    : quests dadas pelos NPCs da cidade (variadas: explorar/entregar/
//!              transportar/matar).
//!   - FACTION: quests dos NPCs de facção (Morganeers/Peacemain) — focadas em
//!              PvP; recompensam XP + PONTOS DE FACÇÃO (sem ouro).
//!
//! O `Objective` é ACHATADO (obj_kind + campos genéricos) de propósito: cabe em
//! 1 linha de DB, 1 objeto JSON plano e parseia trivial no Newtonsoft (C#).
//! Mesma filosofia do `CRAFT_RECIPES` — defs vivem aqui e são seedadas no
//! Postgres (`quest_defs`), com hot-reload.

use serde::{Deserialize, Serialize};

/// Fonte da quest — define quem oferece e o estilo.
pub mod quest_source {
    pub const BOARD: u8 = 0;
    pub const NPC: u8 = 1;
    pub const FACTION: u8 = 2;
}

/// Giver dos quests de NPC dados pelos moradores ambientes (WanderNpc, kind 3).
/// Valor fora da faixa de shop_id (1,3,4,7,8) e faction_id (1,2) pra não colidir.
pub const NPC_TOWN_GIVER: u16 = 99;

/// Facção (espelha o client-side: 1=Morganeers, 2=Peacemain).
pub mod faction_id {
    pub const NONE: u8 = 0;
    pub const MORGANEERS: u8 = 1;
    pub const PEACEMAIN: u8 = 2;
}

/// Tipo de objetivo.
pub mod objective_kind {
    /// Ter/entregar N de `obj_target` (item_id). Checado + CONSUMIDO no turn-in.
    pub const COLLECT: u8 = 0;
    /// Matar N inimigos de `obj_target` (enemy_kind; 0 = qualquer). Live track.
    pub const KILL: u8 = 1;
    /// Matar N players da facção rival (PvP). Live track.
    pub const PVP_KILL: u8 = 2;
    /// Alcançar a área (obj_x, obj_y, obj_radius) — ex.: ilhota. Live track.
    pub const EXPLORE: u8 = 3;
    /// Levar N de `obj_target` (item de quest) ao NPC `giver`. Consome no turn-in.
    pub const DELIVER: u8 = 4;
    /// Transportar N de `obj_target` até a área (obj_x,obj_y,obj_radius) — ex.:
    /// outra ilha. Consome os itens AO CHEGAR (live track) → READY; turn-in no
    /// giver dá a recompensa (sem reconsumir).
    pub const TRANSPORT: u8 = 5;
    /// Caça ao tesouro: vá ao baú em (obj_x,obj_y) e ABRA-O (interagir com a
    /// entidade baú). A interação concede a recompensa/relíquia e conclui — não
    /// completa só por chegar nem pelo turn-in do painel.
    pub const TREASURE: u8 = 6;
}

/// Status de uma quest por personagem.
pub mod quest_status {
    pub const ACTIVE: u8 = 0;     // aceita, em progresso
    pub const READY: u8 = 1;      // objetivo cumprido, falta entregar (turn-in)
    pub const TURNED_IN: u8 = 2;  // concluída (repetível volta a poder aceitar após cooldown)
}

/// Definição estática de uma quest. Achatada de propósito (DB/wire/C#).
#[derive(Debug, Clone, Copy)]
pub struct QuestDef {
    pub id: u16,
    pub source: u8,   // quest_source
    /// Quem dá a quest: shop_id do NPC (NPC), 0 (BOARD), ou faction_id (FACTION).
    pub giver: u16,
    pub faction: u8,  // faction_id (gating + recompensa de pontos)
    pub title: &'static str,
    pub desc: &'static str,
    // --- objetivo ---
    pub obj_kind: u8,    // objective_kind
    pub obj_target: u16, // item_id (Collect/Deliver) ou enemy_kind (Kill; 0=any)
    pub obj_count: u32,
    pub obj_x: f32,
    pub obj_y: f32,
    pub obj_radius: f32,
    // --- recompensas ---
    pub reward_gold: u32,
    pub reward_xp: u64,
    pub reward_item: u16,
    pub reward_item_qty: u16,
    pub reward_faction_points: u32,
    // --- gating / repetição ---
    pub min_level: u32,
    pub repeatable: bool,
    pub daily: bool,
    pub cooldown_secs: u64,
}

/// Versão de rede (server -> client). Inclui o estado por-personagem
/// (status/progress) quando enviada no log; em ofertas vem status=ACTIVE/0.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuestNet {
    pub id: u16,
    pub source: u8,
    pub giver: u16,
    pub faction: u8,
    pub title: String,
    pub desc: String,
    pub obj_kind: u8,
    pub obj_target: u16,
    pub obj_count: u32,
    #[serde(with = "crate::vec2_arr")]
    pub obj_pos: glam::Vec2,
    pub obj_radius: f32,
    pub reward_gold: u32,
    pub reward_xp: u64,
    pub reward_item: u16,
    pub reward_item_qty: u16,
    pub reward_faction_points: u32,
    pub min_level: u32,
    pub repeatable: bool,
    // estado por-personagem (0 quando é só uma oferta)
    pub status: u8,    // quest_status
    pub progress: u32, // contagem atual rumo a obj_count
    /// Unix secs até a quest repetível ficar disponível de novo (0 = já disponível).
    /// Usado pra mostrar contagem regressiva no painel quando em cooldown.
    #[serde(default)]
    pub cooldown_until: i64,
}

impl QuestNet {
    pub fn from_def(d: &QuestDef, status: u8, progress: u32) -> Self {
        QuestNet {
            id: d.id,
            source: d.source,
            giver: d.giver,
            faction: d.faction,
            title: d.title.to_string(),
            desc: d.desc.to_string(),
            obj_kind: d.obj_kind,
            obj_target: d.obj_target,
            obj_count: d.obj_count,
            obj_pos: glam::Vec2::new(d.obj_x, d.obj_y),
            obj_radius: d.obj_radius,
            reward_gold: d.reward_gold,
            reward_xp: d.reward_xp,
            reward_item: d.reward_item,
            reward_item_qty: d.reward_item_qty,
            reward_faction_points: d.reward_faction_points,
            min_level: d.min_level,
            repeatable: d.repeatable,
            status,
            progress,
            cooldown_until: 0,
        }
    }
}

/// Lookup por id.
pub fn quest_by_id(id: u16) -> Option<&'static QuestDef> {
    QUESTS.iter().find(|q| q.id == id)
}

/// Loja de facção: (item_id, custo em PONTOS DE FACÇÃO). Mesma oferta pras duas
/// facções por ora (recompensas premium compradas com pontos das quests PvP).
pub const FACTION_SHOP: &[(u16, u32)] = &[
    (item_id::GREATER_HEAL,   15),
    (item_id::GREATER_MANA,   15),
    (item_id::GEM,            40),
    (item_id::PLATE_ARMOR,    90),
    (item_id::VETERAN_SWORD, 140),
    (item_id::ENHANCED_BOW,  140),
];

/// Preço em pontos de um item na loja de facção, ou None se não vendido.
pub fn faction_shop_price(item_id: u16) -> Option<u32> {
    FACTION_SHOP.iter().find(|(id, _)| *id == item_id).map(|(_, p)| *p)
}

// Helper de construção (mantém os literais legíveis sem repetir todo campo).
const fn q() -> QuestDef {
    QuestDef {
        id: 0, source: quest_source::BOARD, giver: 0, faction: faction_id::NONE,
        title: "", desc: "",
        obj_kind: objective_kind::COLLECT, obj_target: 0, obj_count: 1,
        obj_x: 0.0, obj_y: 0.0, obj_radius: 0.0,
        reward_gold: 0, reward_xp: 0, reward_item: 0, reward_item_qty: 0,
        reward_faction_points: 0,
        min_level: 1, repeatable: false, daily: false, cooldown_secs: 0,
    }
}

use crate::constants::item_id;

/// Registry inicial de quests. Seedado no Postgres (`quest_defs`) e hot-reload.
/// IDs por faixa: 1xx = Board, 2xx = NPC, 3xx = Facção.
pub const QUESTS: &[QuestDef] = &[
    // ===================== BOARD (quadro da cidade) — DIÁRIAS =====================
    QuestDef { id: 101, title: "Madeireiro", desc: "O quadro pede madeira para as obras da cidade.",
        obj_kind: objective_kind::COLLECT, obj_target: item_id::WOOD_T1, obj_count: 20,
        reward_gold: 150, reward_xp: 80, repeatable: true, daily: true, ..q() },
    QuestDef { id: 102, title: "Minerador", desc: "Entregue minério para a fundição da cidade.",
        obj_kind: objective_kind::COLLECT, obj_target: item_id::MINERAL_T1, obj_count: 15,
        reward_gold: 160, reward_xp: 90, repeatable: true, daily: true, ..q() },
    QuestDef { id: 103, title: "Curtume", desc: "A cidade precisa de couro para equipamentos.",
        obj_kind: objective_kind::COLLECT, obj_target: item_id::LEATHER_T1, obj_count: 12,
        reward_gold: 140, reward_xp: 80, repeatable: true, daily: true, ..q() },
    QuestDef { id: 104, title: "Limpeza da costa", desc: "Reduza os monstros que rondam a cidade.",
        obj_kind: objective_kind::KILL, obj_target: 0, obj_count: 10,
        reward_gold: 200, reward_xp: 150, repeatable: true, daily: true, ..q() },

    // ===================== NPC (moradores — DIÁRIAS) =====================
    // 12 quests, 1 por arauto (givers 101..112). 2 arautos por cidade são
    // promovidos no load do mapa. Todas repetíveis com reset DIÁRIO.
    QuestDef { id: 201, source: quest_source::NPC, giver: 101,
        title: "Reforço de madeira", desc: "Um morador precisa de madeira para as obras.",
        obj_kind: objective_kind::COLLECT, obj_target: item_id::WOOD_T1, obj_count: 20,
        reward_gold: 150, reward_xp: 90, repeatable: true, daily: true, ..q() },
    QuestDef { id: 202, source: quest_source::NPC, giver: 102,
        title: "Caça aos saqueadores", desc: "Bandidos rondam a cidade. Elimine-os.",
        obj_kind: objective_kind::KILL, obj_target: 0, obj_count: 10,
        reward_gold: 200, reward_xp: 150, repeatable: true, daily: true, ..q() },
    QuestDef { id: 203, source: quest_source::NPC, giver: 103,
        title: "Minério para a fundição", desc: "A fundição precisa de minério bruto.",
        obj_kind: objective_kind::COLLECT, obj_target: item_id::MINERAL_T1, obj_count: 15,
        reward_gold: 160, reward_xp: 100, repeatable: true, daily: true, ..q() },
    QuestDef { id: 204, source: quest_source::NPC, giver: 104,
        title: "Couro para o curtume", desc: "O curtume precisa de couro fresco.",
        obj_kind: objective_kind::COLLECT, obj_target: item_id::LEATHER_T1, obj_count: 12,
        reward_gold: 150, reward_xp: 90, repeatable: true, daily: true, ..q() },
    QuestDef { id: 205, source: quest_source::NPC, giver: 105,
        title: "Lenha resistente", desc: "Precisamos de madeira tier 2 para barcos.",
        obj_kind: objective_kind::COLLECT, obj_target: item_id::WOOD_T2, obj_count: 10,
        reward_gold: 250, reward_xp: 160, repeatable: true, daily: true, min_level: 5, ..q() },
    QuestDef { id: 206, source: quest_source::NPC, giver: 106,
        title: "Pragas no campo", desc: "Criaturas atrapalham a plantação.",
        obj_kind: objective_kind::KILL, obj_target: 0, obj_count: 12,
        reward_gold: 180, reward_xp: 130, repeatable: true, daily: true, ..q() },
    QuestDef { id: 207, source: quest_source::NPC, giver: 107,
        title: "Aço para a forja", desc: "O ferreiro precisa de minério tier 2.",
        obj_kind: objective_kind::COLLECT, obj_target: item_id::MINERAL_T2, obj_count: 8,
        reward_gold: 280, reward_xp: 200, reward_item: item_id::IRON_INGOT, reward_item_qty: 2,
        repeatable: true, daily: true, min_level: 5, ..q() },
    QuestDef { id: 208, source: quest_source::NPC, giver: 108,
        title: "Suprimentos para a vila distante",
        desc: "Leve 10 de madeira até a vila a leste (siga a bússola).",
        obj_kind: objective_kind::TRANSPORT, obj_target: item_id::WOOD_T1, obj_count: 10,
        obj_x: 8900.0, obj_y: 1359.0, obj_radius: 45.0,
        reward_gold: 320, reward_xp: 240, repeatable: true, daily: true, ..q() },
    QuestDef { id: 209, source: quest_source::NPC, giver: 109,
        title: "Couro reforçado", desc: "Couro tier 2 para armaduras melhores.",
        obj_kind: objective_kind::COLLECT, obj_target: item_id::LEATHER_T2, obj_count: 8,
        reward_gold: 260, reward_xp: 170, repeatable: true, daily: true, min_level: 5, ..q() },
    QuestDef { id: 210, source: quest_source::NPC, giver: 110,
        title: "Caça ao tesouro",
        desc: "Um baú aguarda numa vila distante. Abra-o (siga a bússola).",
        obj_kind: objective_kind::TREASURE, obj_count: 1,
        obj_x: 6820.0, obj_y: 624.0, obj_radius: 60.0,
        reward_gold: 500, reward_xp: 400, reward_item: item_id::GEM, reward_item_qty: 1,
        repeatable: true, daily: true, min_level: 3, ..q() },
    QuestDef { id: 211, source: quest_source::NPC, giver: 111,
        title: "Limpeza pesada", desc: "Reduza os monstros mais perigosos da região.",
        obj_kind: objective_kind::KILL, obj_target: 0, obj_count: 15,
        reward_gold: 350, reward_xp: 280, repeatable: true, daily: true, min_level: 8, ..q() },
    QuestDef { id: 212, source: quest_source::NPC, giver: 112,
        title: "Minério raro", desc: "A cidade precisa de minério tier 3.",
        obj_kind: objective_kind::COLLECT, obj_target: item_id::MINERAL_T3, obj_count: 6,
        reward_gold: 400, reward_xp: 300, repeatable: true, daily: true, min_level: 10, ..q() },

    // ===================== FACÇÃO (PvP; XP + pontos de facção, SEM ouro) ====
    QuestDef { id: 301, source: quest_source::FACTION, faction: faction_id::MORGANEERS,
        giver: faction_id::MORGANEERS as u16,
        title: "Domínio Morganeer", desc: "Derrote membros da Peacemain em combate.",
        obj_kind: objective_kind::PVP_KILL, obj_count: 3,
        reward_xp: 500, reward_faction_points: 100, repeatable: true, cooldown_secs: 1800,
        min_level: 10, ..q() },
    QuestDef { id: 302, source: quest_source::FACTION, faction: faction_id::PEACEMAIN,
        giver: faction_id::PEACEMAIN as u16,
        title: "Defesa Peacemain", desc: "Derrote membros da Morganeers em combate.",
        obj_kind: objective_kind::PVP_KILL, obj_count: 3,
        reward_xp: 500, reward_faction_points: 100, repeatable: true, cooldown_secs: 1800,
        min_level: 10, ..q() },
    // Reconhecimento (EXPLORE): chegue à sede INIMIGA. Coords = faction spawns do
    // mapa atual (morganeer (600,1350) / peacemain (850,620)). Sabor PvP: vai ao
    // território rival. Completa ao chegar (live-track), entrega na própria sede.
    QuestDef { id: 303, source: quest_source::FACTION, faction: faction_id::MORGANEERS,
        giver: faction_id::MORGANEERS as u16,
        title: "Reconhecimento inimigo", desc: "Infiltre a sede da Peacemain e volte com informações.",
        obj_kind: objective_kind::EXPLORE, obj_count: 1,
        obj_x: 850.0, obj_y: 620.0, obj_radius: 40.0,
        reward_xp: 400, reward_faction_points: 80, repeatable: true, cooldown_secs: 1800,
        min_level: 10, ..q() },
    QuestDef { id: 304, source: quest_source::FACTION, faction: faction_id::PEACEMAIN,
        giver: faction_id::PEACEMAIN as u16,
        title: "Reconhecimento inimigo", desc: "Infiltre a sede da Morganeers e volte com informações.",
        obj_kind: objective_kind::EXPLORE, obj_count: 1,
        obj_x: 600.0, obj_y: 1350.0, obj_radius: 40.0,
        reward_xp: 400, reward_faction_points: 80, repeatable: true, cooldown_secs: 1800,
        min_level: 10, ..q() },
];
