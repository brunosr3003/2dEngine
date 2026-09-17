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
    /// A historia principal (`crate::historia`): ninguem oferece nem se aceita
    /// — o servidor da' o passo e passa pro proximo sozinho.
    pub const HISTORIA: u8 = 3;
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
    /// Falar com um NPC da vila: `obj_target` = `construcao::Papel as u16`.
    /// Conclui (READY) ao interagir com ele; a entrega e' no giver.
    pub const TALK: u8 = 7;
    /// Quebrar N corpos de coleta. `obj_target`: 0 qualquer, 1 pedra, 2 arvore.
    /// Conta o ATO de coletar, nao o item — e' diaria de area.
    pub const GATHER: u8 = 8;
    /// Criar N equipamentos no painel de Craft. `obj_target` 0 = qualquer.
    pub const CRAFT: u8 = 9;
    /// Tentar refinar N vezes na Forja (sucesso ou nao).
    pub const REFINE: u8 = 10;
    /// Encantar (sistema ainda nao existe: missao `em_breve`).
    pub const ENCHANT: u8 = 11;
    /// Concluir dungeon. FUNCIONA: `dg_terminar` chama `quest_on_evento` com
    /// este tipo, por membro presente, so' no caminho de vitoria.
    pub const DUNGEON: u8 = 12;
    /// Derrotar chefe de raid. Este sim ainda nao existe: nao ha' conteudo de
    /// raid no catalogo (so' Porao e Gruta), e nada avanca este tipo — por isso
    /// a Cacada (607) fica `em_breve`.
    pub const RAID: u8 = 13;
    /// Ir a um PONTO-CHAVE da ilha (`historia::ponto`, em `obj_target`). A
    /// posicao sai do relevo e da vila da ilha em que se esta' — o servidor
    /// resolve. Conclui ao chegar.
    pub const LUGAR: u8 = 14;
    /// Trava da historia: alcancar o nivel `obj_count`. `progress` = nivel
    /// atual; conclui sozinha no level-up.
    pub const NIVEL: u8 = 15;
    /// Ir pra outra ilha (`obj_target` = indice em `terreno::ARQUIPELAGO`)
    /// pelo Capitao do Porto. Conclui ao entrar no mundo daquela ilha.
    pub const VIAGEM: u8 = 16;
}

/// `obj_target` de GATHER.
pub mod alvo_de_coleta {
    pub const QUALQUER: u16 = 0;
    pub const PEDRA: u16 = 1;
    pub const ARVORE: u16 = 2;

    /// A coleta de um corpo de `tier` (0 = arvore, 1..4 = pedra) conta pra
    /// este alvo?
    pub fn conta(alvo: u16, tier: u8) -> bool {
        match alvo {
            PEDRA => tier > 0,
            ARVORE => tier == 0,
            _ => true,
        }
    }
}

/// A proxima meia-noite UTC depois de `agora` (unix secs): o reset diario.
pub fn proxima_meia_noite(agora: i64) -> i64 {
    (agora.div_euclid(86_400) + 1) * 86_400
}

/// Missao "de area": matar ou juntar numa zona. Ela paga Pocao de Experiencia.
pub fn e_de_area(d: &QuestDef) -> bool {
    matches!(d.obj_kind, objective_kind::KILL | objective_kind::GATHER | objective_kind::COLLECT)
}

/// Quem da' as missoes da ilha: o Mestre de Missoes da praca
/// (`construcao::Papel::Missoes`). Fora das faixas de loja (1..8), facção
/// (1..2), arauto (101..112) e morador (99).
pub const GIVER_MESTRE_DA_ILHA: u16 = 121;

/// Em que ilha (zona) a missao existe de verdade — quem a da' mora la'.
///
/// So' a cadeia do Mestre (5xx) tem giver no mundo de ilhas; o resto do
/// registro vem do mapa de tiles antigo (quadro, NPCs e faccao que nao
/// existem nas ilhas) e fica FORA do menu de missoes ate' ganhar um giver.
pub fn zona_da_missao(id: u16) -> Option<&'static str> {
    // Os passos escritos da historia moram na ilha do capitulo; as cronicas
    // do epilogo valem em qualquer ilha (None).
    if crate::historia::e_da_historia(id) {
        return crate::historia::zona_do_passo(id);
    }
    match id {
        500..=609 => Some("ilha_inicial"),
        610..=619 => Some("ilha_gelo"),
        620..=629 => Some("ilha_deserto"),
        630..=639 => Some("ilha_planalto"),
        _ => None,
    }
}

/// O que o servidor responde em `QuestDestino`: pra onde a auto missao vai e o
/// que faz ao chegar.
pub mod destino_tipo {
    /// Sem destino conhecido nesta ilha.
    pub const NENHUM: u8 = 0;
    /// Falar com um NPC ("fale com").
    pub const NPC: u8 = 1;
    /// Zona onde o bicho nasce: auto combate.
    pub const COMBATE: u8 = 2;
    /// Spot de coleta: auto coleta.
    pub const COLETA: u8 = 3;
    /// Objetivo cumprido: entregar a quem deu.
    pub const ENTREGA: u8 = 4;
    /// Objetivo e' criar um item: abre o painel de Craft, nao anda.
    pub const PAINEL_CRAFT: u8 = 5;
    /// Objetivo e' refinar: abre a Forja, nao anda.
    pub const PAINEL_FORJA: u8 = 6;
    /// Um ponto da ilha: ir ate' la' e esperar (o servidor conclui ao chegar).
    pub const LUGAR: u8 = 7;
    /// Trava de nivel da historia: nao ha' pra onde ir.
    pub const TRAVA: u8 = 8;
    /// Objetivo e' vencer uma dungeon: abre o painel de Dungeon, nao anda.
    /// `raio` leva o id do conteudo (0 = qualquer).
    pub const PAINEL_DUNGEON: u8 = 9;
}

/// Em que ponto da missao a fala acontece.
pub mod momento {
    /// Quem da' a missao, oferecendo.
    pub const OFERTA: u8 = 0;
    /// O NPC de uma missao "fale com".
    pub const CONVERSA: u8 = 1;
    /// Quem deu, recebendo a entrega.
    pub const ENTREGA: u8 = 2;
}

/// As falas do dialogo de uma missao, em ordem. Sem texto escrito, cai numa
/// fala generica (a descricao, na oferta).
pub fn falas(quest_id: u16, m: u8) -> Vec<&'static str> {
    if let Some(f) = crate::historia::falas(quest_id, m) {
        return f;
    }
    let escritas: &[&str] = match (quest_id, m) {
        (501, momento::OFERTA) => &[
            "Ah, sangue novo na praça! Bem-vindo à ilha.",
            "Antes de pisar fora da cidade, você precisa de poções no bolso.",
            "Vá falar com o Alquimista — a loja de toldo verde. Diga que fui eu que mandei.",
        ],
        (501, momento::CONVERSA) => &[
            "O Mestre te mandou? Então você é o novato de quem falaram.",
            "Sem poção, a mata engole qualquer um. Aqui eu vendo vida, mana e fôlego.",
            "Guarde a lição: bebe antes de cair, não depois. Agora volte ao Mestre.",
        ],
        (501, momento::ENTREGA) => &[
            "Conheceu o velho Alquimista? Ótimo.",
            "Tome algumas poções por conta da casa. Não gaste todas de uma vez.",
        ],
        (502, momento::OFERTA) => &[
            "Os lobos voltaram a rondar as trilhas fora da cidade.",
            "Os mercadores não passam mais sem escolta.",
            "Derrote seis deles e a estrada respira de novo.",
        ],
        (502, momento::ENTREGA) => &[
            "Seis lobos a menos na estrada. Os mercadores agradecem.",
            "Você luta melhor do que parece. Aqui está sua paga.",
        ],
        (503, momento::OFERTA) => &[
            "O Ferreiro está sem metal pras ferramentas da vila.",
            "Todo bicho da ilha carrega um pouco de cobre — ninguém sabe por quê.",
            "Traga trinta de Cobre e a forja volta a cantar.",
        ],
        (503, momento::ENTREGA) => &[
            "Trinta de Cobre, bem contados. O Ferreiro vai ficar contente.",
            "Leve estas poções de mana. Vai precisar delas mais adiante.",
        ],
        (504, momento::OFERTA) => &[
            "Ursos desceram a encosta e assustam os lenhadores.",
            "Não é serviço pra novato — mas você já não é mais um.",
            "Derrote três ursos e volte inteiro.",
        ],
        (504, momento::ENTREGA) => &[
            "Três ursos! Os lenhadores já voltaram pra mata.",
            "A ilha começa a falar seu nome. Tome sua recompensa.",
        ],
        (505, momento::OFERTA) => &[
            "O Capitão do Porto quer conhecer quem protege a ilha.",
            "Siga a estrada até o cais — o mapa mostra a âncora.",
            "Fale com ele e depois me conte o que ele disse.",
        ],
        (505, momento::CONVERSA) => &[
            "Então você é o herói da praça. Chegue mais, o vento aqui é forte.",
            "Daqui saem os barcos pras outras ilhas: gelo, deserto e planalto.",
            "Quando estiver pronto, o mar espera por você. Diga ao Mestre que o porto está aberto.",
        ],
        (505, momento::ENTREGA) => &[
            "O Capitão gostou de você? Então o mar é o seu próximo passo.",
            "Estas poções maiores vão te manter de pé longe de casa. Boa viagem.",
        ],
        _ => &[],
    };
    if !escritas.is_empty() {
        return escritas.to_vec();
    }
    match m {
        momento::OFERTA => vec![quest_by_id(quest_id).map_or("Tenho uma tarefa pra você.", |d| d.desc)],
        momento::CONVERSA => vec!["Ah, você veio. Obrigado por passar aqui."],
        _ => vec!["Bom trabalho. Aqui está sua recompensa."],
    }
}

/// `obj_target` de KILL que quer UM tipo de mob. O 0 continua sendo "qualquer
/// um" — e o kind 0 da tabela e' o Lobo, dai' o deslocamento de um.
pub const fn alvo_de_mob(kind: u16) -> u16 {
    kind + 1
}

/// Kinds da tabela de mobs (`enemy_kinds`) usados nas missoes.
pub mod mob_kind {
    pub const LOBO: u16 = 0;
    pub const URSO: u16 = 1;
    pub const PISTOLEIRO: u16 = 2;
    pub const TIGRE: u16 = 3;
    pub const MAGO: u16 = 4;
    pub const OWLBEAR: u16 = 5;
    pub const ARQUEIRO: u16 = 6;
}

/// Os NPCs com quem uma missao TALK pode mandar conversar. O servidor so'
/// conhece o NOME do NPC da vila; o papel sai daqui. O Mestre de Missoes
/// entra pela historia ("fale com o Mestre").
pub const PAPEIS_DE_CONVERSA: &[crate::construcao::Papel] = {
    use crate::construcao::Papel::*;
    &[Alquimista, Estaleiro, Ferreiro, Armaduras, Taberna, Alfaiate, Treinador, Identificador, Deposito, Cartografo, Missoes]
};

/// Papel (como `obj_target` de TALK) do NPC da vila com este nome.
pub fn papel_de_conversa(nome: &str) -> Option<u16> {
    PAPEIS_DE_CONVERSA.iter().find(|p| p.nome() == nome).map(|p| *p as u16)
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
    /// Recompensa em COBRE, a moeda do dia a dia (docs/ECONOMIA.md). Era ouro
    /// ate' 17/09/2026: o ouro virou moeda rara, de chefe, dungeon e mercado.
    pub reward_cobre: u32,
    pub reward_xp: u64,
    pub reward_item: u16,
    pub reward_item_qty: u16,
    pub reward_faction_points: u32,
    // --- gating / repetição ---
    pub min_level: u32,
    pub repeatable: bool,
    pub daily: bool,
    pub cooldown_secs: u64,
    /// Missao que precisa estar ENTREGUE antes desta ser oferecida (0 = nenhuma).
    /// E' o que faz a cadeia do Mestre de Missoes andar em ordem.
    pub requires: u16,
    /// Existe no catalogo mas o sistema dela ainda nao: aparece com cadeado
    /// "Em breve" e o servidor nao deixa aceitar.
    pub em_breve: bool,
    /// Segunda recompensa (a Pocao de Experiencia das missoes de area).
    pub reward_item2: u16,
    pub reward_item2_qty: u16,
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
    pub reward_cobre: u32,
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
    #[serde(default)]
    pub daily: bool,
    #[serde(default)]
    pub em_breve: bool,
    #[serde(default)]
    pub reward_item2: u16,
    #[serde(default)]
    pub reward_item2_qty: u16,
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
            reward_cobre: d.reward_cobre,
            reward_xp: d.reward_xp,
            reward_item: d.reward_item,
            reward_item_qty: d.reward_item_qty,
            reward_faction_points: d.reward_faction_points,
            min_level: d.min_level,
            repeatable: d.repeatable,
            status,
            progress,
            cooldown_until: 0,
            daily: d.daily,
            em_breve: d.em_breve,
            reward_item2: d.reward_item2,
            reward_item2_qty: d.reward_item2_qty,
        }
    }
}

/// Lookup por id.
pub fn quest_by_id(id: u16) -> Option<&'static QuestDef> {
    QUESTS.iter().find(|q| q.id == id).or_else(|| crate::historia::def_da_historia(id))
}

/// Uma `QuestDef` zerada, pra quem monta definicoes fora deste arquivo (a
/// historia).
pub const fn quest_vazia() -> QuestDef {
    q()
}

/// Loja de facção: (item_id, custo em PONTOS DE FACÇÃO). Mesma oferta pras duas
/// facções por ora (recompensas premium compradas com pontos das quests PvP).
pub const FACTION_SHOP: &[(u16, u32)] = &[
    (item_id::GREATER_HEAL,   15),
    (item_id::GREATER_MANA,   15),
    (item_id::GLITTERING_POWDER,            40),
    (item_id::ARMADURA_PESADA,    90),
    (item_id::ESPADA_E_ESCUDO, 140),
    (item_id::PISTOLAS,  140),
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
        reward_cobre: 0, reward_xp: 0, reward_item: 0, reward_item_qty: 0,
        reward_faction_points: 0,
        min_level: 1, repeatable: false, daily: false, cooldown_secs: 0,
        requires: 0,
        em_breve: false, reward_item2: 0, reward_item2_qty: 0,
    }
}

/// Uma DIARIA do Mestre de Missoes: repetivel, volta a meia-noite UTC. As de
/// area pagam tambem 1 Pocao de Experiencia.
#[allow(clippy::too_many_arguments)]
const fn diaria(
    id: u16, title: &'static str, desc: &'static str,
    obj_kind: u8, obj_target: u16, obj_count: u32,
    reward_cobre: u32, reward_xp: u64, reward_item: u16, reward_item_qty: u16,
    min_level: u32, area: bool, em_breve: bool,
) -> QuestDef {
    QuestDef {
        id, title, desc, obj_kind, obj_target, obj_count,
        reward_cobre, reward_xp, reward_item, reward_item_qty,
        // As de oficina pagam as pocoes de drop: criar da' Fortuna, refinar
        // da' Sorte. As de area continuam com a de Experiencia.
        reward_item2: if area {
            item_id::XP_POTION
        } else if obj_kind == objective_kind::CRAFT {
            item_id::FORTUNA_POTION
        } else if obj_kind == objective_kind::REFINE {
            item_id::SORTE_POTION
        } else {
            0
        },
        reward_item2_qty: if area || obj_kind == objective_kind::CRAFT || obj_kind == objective_kind::REFINE { 1 } else { 0 },
        min_level, repeatable: true, daily: true, em_breve,
        ..mestre()
    }
}

// Atalho pro Mestre de Missoes da ilha inicial.
const fn mestre() -> QuestDef {
    QuestDef { source: quest_source::NPC, giver: GIVER_MESTRE_DA_ILHA, ..q() }
}

use crate::constants::item_id;

/// Registry inicial de quests. Seedado no Postgres (`quest_defs`) e hot-reload.
/// IDs por faixa: 1xx = Board, 2xx = NPC, 3xx = Facção.
pub const QUESTS: &[QuestDef] = &[
    // ===================== MESTRE DE MISSOES (5xx) — ilha inicial =====================
    // Cadeia de boas-vindas, na ordem: conhecer a cidade, a primeira caca, o
    // loot dela, a primeira viagem longa. Tudo com o que a ilha tem de verdade:
    // lobo e urso da tabela, cobre que todo mob dropa, NPCs da vila e do porto.
    QuestDef { id: 501, title: "Conheça o Alquimista",
        desc: "Toda jornada começa com poções no bolso. Fale com o Alquimista, na porta da loja de toldo verde.",
        obj_kind: objective_kind::TALK, obj_target: crate::construcao::Papel::Alquimista as u16, obj_count: 1,
        reward_cobre: 30, reward_xp: 40, reward_item: item_id::HEALTH_POTION, reward_item_qty: 3,
        ..mestre() },
    QuestDef { id: 502, title: "Lobos na estrada",
        desc: "Os lobos rondam as trilhas fora da cidade. Derrote 6 lobos e volte aqui.",
        obj_kind: objective_kind::KILL, obj_target: alvo_de_mob(mob_kind::LOBO), obj_count: 6,
        reward_cobre: 80, reward_xp: 120, reward_item: item_id::HEALTH_POTION, reward_item_qty: 2,
        reward_item2: item_id::XP_POTION, reward_item2_qty: 1,
        requires: 501, ..mestre() },
    QuestDef { id: 503, title: "Cobre pra forja",
        desc: "O Ferreiro precisa de metal pras ferramentas da vila. Traga 30 de Cobre — todo bicho da ilha carrega um pouco.",
        obj_kind: objective_kind::COLLECT, obj_target: item_id::COPPER, obj_count: 30,
        reward_cobre: 120, reward_xp: 150, reward_item: item_id::MANA_POTION, reward_item_qty: 3,
        reward_item2: item_id::XP_POTION, reward_item2_qty: 1,
        requires: 502, ..mestre() },
    QuestDef { id: 504, title: "Ursos na mata",
        desc: "Ursos desceram a encosta e assustam os lenhadores. Derrote 3 ursos.",
        obj_kind: objective_kind::KILL, obj_target: alvo_de_mob(mob_kind::URSO), obj_count: 3,
        reward_cobre: 160, reward_xp: 220, reward_item: item_id::HEALTH_POTION, reward_item_qty: 3,
        reward_item2: item_id::XP_POTION, reward_item2_qty: 1,
        requires: 503, min_level: 3, ..mestre() },
    QuestDef { id: 505, title: "Visite o Porto",
        desc: "O Capitão do Porto quer conhecer quem protege a ilha. Siga pela estrada até o cais (M abre o mapa) e fale com ele.",
        obj_kind: objective_kind::TALK, obj_target: crate::construcao::Papel::Estaleiro as u16, obj_count: 1,
        reward_cobre: 250, reward_xp: 300, reward_item: item_id::GREATER_HEAL, reward_item_qty: 2,
        requires: 504, ..mestre() },

    // ===== A oficina do Mestre (506-509) — de onde vem cada material =====
    // A historia manda "crie sua primeira peca" e nunca diz ONDE se acha o que
    // a receita pede. Uma peca cinza custa 1 chave + 30 aco + 10 quintessencia
    // + 10 berloque + 200 darksteel + 300 cobre, e o inicio so' apresentava
    // pedra e cobre. Esta cadeia apresenta o resto, cada quest com uma fonte.
    QuestDef { id: 506, title: "Lenha para o cais",
        desc: "O cais perdeu meio píer na tempestade e o Capitão precisa de madeira. Derrube 8 árvores — é o único lugar de onde sai madeira, e ninguém tinha te dito.",
        obj_kind: objective_kind::GATHER, obj_target: alvo_de_coleta::ARVORE, obj_count: 8,
        reward_cobre: 200, reward_xp: 350, reward_item: item_id::HEALTH_POTION, reward_item_qty: 3,
        reward_item2: item_id::XP_POTION, reward_item2_qty: 1,
        requires: 505, ..mestre() },
    QuestDef { id: 507, title: "O que a pedra guarda",
        desc: "Dentro da pedra há um metal escuro que não enferruja: Darksteel. Toda peça pede 200 dele. Traga 20 para o Ferreiro ver a qualidade do veio daqui.",
        obj_kind: objective_kind::COLLECT, obj_target: item_id::DARKSTEEL, obj_count: 20,
        reward_cobre: 260, reward_xp: 450, reward_item: item_id::STEEL, reward_item_qty: 10,
        reward_item2: item_id::XP_POTION, reward_item2_qty: 1,
        requires: 506, ..mestre() },
    QuestDef { id: 508, title: "Quintessência",
        desc: "A pedra dá quintessência a conta-gotas; tigre dá bem mais. Traga 6 — sem ela nenhuma armadura fecha.",
        obj_kind: objective_kind::COLLECT, obj_target: item_id::QUINTESSENCE, obj_count: 6,
        reward_cobre: 320, reward_xp: 600, reward_item: item_id::STEEL, reward_item_qty: 12,
        reward_item2: item_id::GREATER_HEAL, reward_item2_qty: 2,
        requires: 507, min_level: 8, ..mestre() },
    QuestDef { id: 509, title: "O berloque do owlbear",
        desc: "Falta a última peça da receita: o berloque, que os owlbears carregam preso ao pelo. Traga 6 e o Mestre abre o baú da vila para você.",
        obj_kind: objective_kind::COLLECT, obj_target: item_id::EXORCISM_BAUBLE, obj_count: 6,
        reward_cobre: 400, reward_xp: 900, reward_item: item_id::HIDE, reward_item_qty: 1,
        reward_item2: item_id::STEEL, reward_item2_qty: 15,
        requires: 508, min_level: 12, ..mestre() },
    QuestDef { id: 510, title: "O que há sob o naufrágio",
        desc: "O Porão do Naufrágio guarda o que a maré não levou, e quem manda lá dentro cai com a chave no bolso: chefe de dungeon larga chave três vezes mais que chefe de campo. Entre e limpe uma — sozinho já dá.",
        obj_kind: objective_kind::DUNGEON, obj_target: 0, obj_count: 1,
        reward_cobre: 500, reward_xp: 1_200, reward_item: item_id::GREATER_HEAL, reward_item_qty: 3,
        reward_item2: item_id::STEEL, reward_item2_qty: 20,
        requires: 509, min_level: 6, ..mestre() },

    // ===================== DIARIAS do Mestre (6xx) — uma serie por ilha =====================
    // Mesmas sete tarefas em cada ilha, na escala da faixa dela (nivel de
    // entrada da ilha, contagem e paga crescendo): quebrar pedras e cacar
    // (area: +1 Pocao de Experiencia), criar e refinar (painel pelo menu), e
    // as tres que dependem de sistema que ainda nao existe — com cadeado.
    // --- Bosque (ilha_inicial, 1-15) ---
    diaria(601, "Pedreira do dia", "Quebre 20 pedras em qualquer veio da ilha.", objective_kind::GATHER, alvo_de_coleta::PEDRA, 20, 150, 150, item_id::HEALTH_POTION, 3, 1, true, false),
    diaria(602, "Caça do dia", "Derrote 30 bichos da ilha.", objective_kind::KILL, 0, 30, 180, 200, item_id::HEALTH_POTION, 3, 1, true, false),
    diaria(603, "Mãos à obra", "Crie 1 equipamento no Craft (botão do HUD).", objective_kind::CRAFT, 0, 1, 120, 120, item_id::MANA_POTION, 2, 1, false, false),
    diaria(604, "Forja quente", "Tente refinar 1 vez uma peça na Forja (botão do HUD ou o Ferreiro).", objective_kind::REFINE, 0, 1, 120, 120, item_id::MANA_POTION, 2, 1, false, false),
    diaria(605, "Encanto do dia", "Encante uma peça.", objective_kind::ENCHANT, 0, 1, 150, 150, 0, 0, 1, false, true),
    diaria(606, "Porão do dia", "Conclua uma dungeon (Porão ou Gruta).", objective_kind::DUNGEON, 0, 1, 250, 250, 0, 0, 10, false, false),
    diaria(607, "Caçada do dia", "Derrote o chefe da Caçada.", objective_kind::RAID, 0, 1, 400, 400, 0, 0, 10, false, true),
    // --- Geleira (ilha_gelo, 15-30) ---
    diaria(611, "Pedreira do dia", "Quebre 25 pedras em qualquer veio da ilha.", objective_kind::GATHER, alvo_de_coleta::PEDRA, 25, 450, 600, item_id::GREATER_HEAL, 3, 15, true, false),
    diaria(612, "Caça do dia", "Derrote 40 bichos da ilha.", objective_kind::KILL, 0, 40, 540, 800, item_id::GREATER_HEAL, 3, 15, true, false),
    diaria(613, "Mãos à obra", "Crie 1 equipamento no Craft (botão do HUD).", objective_kind::CRAFT, 0, 1, 360, 480, item_id::GREATER_MANA, 2, 15, false, false),
    diaria(614, "Forja quente", "Tente refinar 1 vez uma peça na Forja (botão do HUD ou o Ferreiro).", objective_kind::REFINE, 0, 1, 360, 480, item_id::GREATER_MANA, 2, 15, false, false),
    diaria(615, "Encanto do dia", "Encante uma peça.", objective_kind::ENCHANT, 0, 1, 450, 600, 0, 0, 15, false, true),
    diaria(616, "Porão do dia", "Conclua uma dungeon (Porão ou Gruta).", objective_kind::DUNGEON, 0, 1, 750, 1_000, 0, 0, 15, false, false),
    diaria(617, "Caçada do dia", "Derrote o chefe da Caçada.", objective_kind::RAID, 0, 1, 1_200, 1_600, 0, 0, 15, false, true),
    // --- Ermo (ilha_deserto, 28-42) ---
    diaria(621, "Pedreira do dia", "Quebre 30 pedras em qualquer veio da ilha.", objective_kind::GATHER, alvo_de_coleta::PEDRA, 30, 750, 1_200, item_id::GREATER_HEAL, 4, 28, true, false),
    diaria(622, "Caça do dia", "Derrote 50 bichos da ilha.", objective_kind::KILL, 0, 50, 900, 1_600, item_id::GREATER_HEAL, 4, 28, true, false),
    diaria(623, "Mãos à obra", "Crie 1 equipamento no Craft (botão do HUD).", objective_kind::CRAFT, 0, 1, 600, 960, item_id::GREATER_MANA, 3, 28, false, false),
    diaria(624, "Forja quente", "Tente refinar 1 vez uma peça na Forja (botão do HUD ou o Ferreiro).", objective_kind::REFINE, 0, 1, 600, 960, item_id::GREATER_MANA, 3, 28, false, false),
    diaria(625, "Encanto do dia", "Encante uma peça.", objective_kind::ENCHANT, 0, 1, 750, 1_200, 0, 0, 28, false, true),
    diaria(626, "Porão do dia", "Conclua uma dungeon (Porão ou Gruta).", objective_kind::DUNGEON, 0, 1, 1_250, 2_000, 0, 0, 28, false, false),
    diaria(627, "Caçada do dia", "Derrote o chefe da Caçada.", objective_kind::RAID, 0, 1, 2_000, 3_200, 0, 0, 30, false, true),
    // --- Planalto (ilha_planalto, 40-60) ---
    diaria(631, "Pedreira do dia", "Quebre 35 pedras em qualquer veio da ilha.", objective_kind::GATHER, alvo_de_coleta::PEDRA, 35, 1_200, 2_400, item_id::GREATER_HEAL, 5, 40, true, false),
    diaria(632, "Caça do dia", "Derrote 60 bichos da ilha.", objective_kind::KILL, 0, 60, 1_440, 3_200, item_id::GREATER_HEAL, 5, 40, true, false),
    diaria(633, "Mãos à obra", "Crie 1 equipamento no Craft (botão do HUD).", objective_kind::CRAFT, 0, 1, 960, 1_920, item_id::GREATER_MANA, 4, 40, false, false),
    diaria(634, "Forja quente", "Tente refinar 1 vez uma peça na Forja (botão do HUD ou o Ferreiro).", objective_kind::REFINE, 0, 1, 960, 1_920, item_id::GREATER_MANA, 4, 40, false, false),
    diaria(635, "Encanto do dia", "Encante uma peça.", objective_kind::ENCHANT, 0, 1, 1_200, 2_400, 0, 0, 40, false, true),
    diaria(636, "Porão do dia", "Conclua uma dungeon (Porão ou Gruta).", objective_kind::DUNGEON, 0, 1, 2_000, 4_000, 0, 0, 40, false, false),
    diaria(637, "Caçada do dia", "Derrote o chefe da Caçada.", objective_kind::RAID, 0, 1, 3_200, 6_400, 0, 0, 40, false, true),

    // ===================== BOARD (quadro da cidade) — DIÁRIAS =====================
    QuestDef { id: 101, title: "Madeireiro", desc: "O quadro pede madeira para as obras da cidade.",
        obj_kind: objective_kind::COLLECT, obj_target: item_id::WOOD_T1, obj_count: 20,
        reward_cobre: 150, reward_xp: 80, repeatable: true, daily: true, ..q() },
    QuestDef { id: 102, title: "Minerador", desc: "Entregue minério para a fundição da cidade.",
        obj_kind: objective_kind::COLLECT, obj_target: item_id::STEEL, obj_count: 15,
        reward_cobre: 160, reward_xp: 90, repeatable: true, daily: true, ..q() },
    QuestDef { id: 103, title: "Curtume", desc: "A cidade precisa de couro para equipamentos.",
        obj_kind: objective_kind::COLLECT, obj_target: item_id::LEATHER_T1, obj_count: 12,
        reward_cobre: 140, reward_xp: 80, repeatable: true, daily: true, ..q() },
    QuestDef { id: 104, title: "Limpeza da costa", desc: "Reduza os monstros que rondam a cidade.",
        obj_kind: objective_kind::KILL, obj_target: 0, obj_count: 10,
        reward_cobre: 200, reward_xp: 150, repeatable: true, daily: true, ..q() },

    // ===================== NPC (arautos das ILHAS — DIÁRIAS) =====================
    // 12 quests, 1 por arauto (givers 101..112), 2 por cidade. As cidades têm
    // dificuldades MUITO diferentes (nível dos inimigos ao redor), então cada
    // par é escalado pro TIER da ilha: começo barato/fácil → endgame caro/difícil.
    // Mapa giver→cidade→tier (cidade# = índice em CITY_CENTROIDS no world.rs):
    //   T1 ilha lv1-5    : cidade#3, givers 107/108 (q207/208)  — início
    //   T2 ilha lv1-5    : cidade#5, givers 111/112 (q211/212)  — início
    //   T3 ilha lv31-40  : cidade#2, givers 105/106 (q205/206)
    //   T4 ilha lv51-60  : cidade#0, givers 101/102 (q201/202)
    //   T5 ilha lv71-80  : cidade#1, givers 103/104 (q203/204)  — +TRANSPORTE
    //   T6 ilha lv91-100 : cidade#4, givers 109/110 (q209/210)  — +TESOURO
    // (ids mantidos em ordem crescente por legibilidade; o tier vem do giver.)

    // --- T4 · cidade#0 (lv51-60) ---
    QuestDef { id: 201, source: quest_source::NPC, giver: 101,
        title: "Madeira nobre", desc: "A fortaleza precisa de madeira tier 3 reforçada.",
        obj_kind: objective_kind::COLLECT, obj_target: item_id::WOOD_T3, obj_count: 10,
        reward_cobre: 600, reward_xp: 520, reward_item: item_id::GREATER_HEAL, reward_item_qty: 5,
        repeatable: true, daily: true, min_level: 45, ..q() },
    QuestDef { id: 202, source: quest_source::NPC, giver: 102,
        title: "Caça aos saqueadores", desc: "Saqueadores veteranos cercam a região. Elimine-os.",
        obj_kind: objective_kind::KILL, obj_target: 0, obj_count: 22,
        reward_cobre: 680, reward_xp: 600, repeatable: true, daily: true, min_level: 45, ..q() },

    // --- T5 · cidade#1 (lv71-80) — inclui TRANSPORTE ---
    QuestDef { id: 203, source: quest_source::NPC, giver: 103,
        title: "Suprimentos para a fortaleza do norte",
        desc: "Leve 10 de madeira tier 3 até a ilha do norte (siga a bússola).",
        obj_kind: objective_kind::TRANSPORT, obj_target: item_id::WOOD_T3, obj_count: 10,
        obj_x: 11100.0, obj_y: 639.0, obj_radius: 45.0,
        reward_cobre: 1000, reward_xp: 900, reward_item: item_id::GLITTERING_POWDER, reward_item_qty: 2,
        repeatable: true, daily: true, min_level: 65, ..q() },
    QuestDef { id: 204, source: quest_source::NPC, giver: 104,
        title: "Ameaça crescente", desc: "Bestas poderosas ameaçam a ilha. Reduza-as.",
        obj_kind: objective_kind::KILL, obj_target: 0, obj_count: 28,
        reward_cobre: 1100, reward_xp: 980, repeatable: true, daily: true, min_level: 65, ..q() },

    // --- T3 · cidade#2 (lv31-40) ---
    QuestDef { id: 205, source: quest_source::NPC, giver: 105,
        title: "Minério para a forja", desc: "O ferreiro precisa de minério tier 2.",
        obj_kind: objective_kind::COLLECT, obj_target: item_id::na_cor(item_id::STEEL, 2), obj_count: 12,
        reward_cobre: 350, reward_xp: 300, reward_item: item_id::DARKSTEEL, reward_item_qty: 2,
        repeatable: true, daily: true, min_level: 25, ..q() },
    QuestDef { id: 206, source: quest_source::NPC, giver: 106,
        title: "Limpeza da floresta", desc: "Criaturas hostis tomaram a mata. Faça a limpeza.",
        obj_kind: objective_kind::KILL, obj_target: 0, obj_count: 18,
        reward_cobre: 400, reward_xp: 340, repeatable: true, daily: true, min_level: 25, ..q() },

    // --- T1 · cidade#3 (ilha inicial lv1-5) ---
    QuestDef { id: 207, source: quest_source::NPC, giver: 107,
        title: "Madeira para o porto", desc: "O porto da vila precisa de madeira para os reparos.",
        obj_kind: objective_kind::COLLECT, obj_target: item_id::WOOD_T1, obj_count: 15,
        reward_cobre: 80, reward_xp: 60, repeatable: true, daily: true, min_level: 1, ..q() },
    QuestDef { id: 208, source: quest_source::NPC, giver: 108,
        title: "Bichos na praia", desc: "Pequenas criaturas incomodam os pescadores.",
        obj_kind: objective_kind::KILL, obj_target: 0, obj_count: 8,
        reward_cobre: 100, reward_xp: 70, repeatable: true, daily: true, min_level: 1, ..q() },

    // --- T6 · cidade#4 (ilha endgame lv91-100) — inclui TESOURO ---
    QuestDef { id: 209, source: quest_source::NPC, giver: 109,
        title: "Tesouro lendário",
        desc: "Um mapa aponta um baú lendário numa ilha distante. Abra-o (siga a bússola).",
        obj_kind: objective_kind::TREASURE, obj_count: 1,
        obj_x: 6820.0, obj_y: 624.0, obj_radius: 60.0,
        reward_cobre: 1800, reward_xp: 1600, reward_item: item_id::na_cor(item_id::SCALE, 3), reward_item_qty: 1,
        repeatable: true, daily: true, min_level: 88, ..q() },
    QuestDef { id: 210, source: quest_source::NPC, giver: 110,
        title: "Senhores da guerra", desc: "Os monstros mais letais do arquipélago rondam aqui.",
        obj_kind: objective_kind::KILL, obj_target: 0, obj_count: 35,
        reward_cobre: 2000, reward_xp: 1750, reward_item: item_id::GLITTERING_POWDER, reward_item_qty: 3,
        repeatable: true, daily: true, min_level: 88, ..q() },

    // --- T2 · cidade#5 (ilha inicial lv1-5) ---
    QuestDef { id: 211, source: quest_source::NPC, giver: 111,
        title: "Couro para o curtume", desc: "O curtume precisa de couro fresco para começar.",
        obj_kind: objective_kind::COLLECT, obj_target: item_id::LEATHER_T1, obj_count: 12,
        reward_cobre: 110, reward_xp: 90, repeatable: true, daily: true, min_level: 1, ..q() },
    QuestDef { id: 212, source: quest_source::NPC, giver: 112,
        title: "Ronda da vila", desc: "Mantenha os arredores da vila seguros.",
        obj_kind: objective_kind::KILL, obj_target: 0, obj_count: 12,
        reward_cobre: 140, reward_xp: 110, repeatable: true, daily: true, min_level: 1, ..q() },

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

    // ===================== TUTORIAL (9xx) — storyline auto-grant em TUTORIAL_MODE ====
    // Concedidas/avançadas automaticamente pelo servidor de tutorial; não vêm
    // de NPC/board. Ao concluir a última, o tutorial finaliza (teleporta pro mundo).
    // Ordem da storyline: pular pedra+falar com Matteo → equipar machado →
    // colher+entregar madeira → forjar arma T1 → matar o inimigo → embarcar.
    // Conclusão é CUSTOM no servidor de tutorial (tick_tutorial_quests +
    // handle_interact); obj_kind/coords aqui são pro display/HUD.
    QuestDef { id: 900, title: "Pule a Pedra",
        desc: "Ande contra a pedra segurando a direcao pra PULAR por cima dela, e fale com Matteo (chegue perto e aperte E / toque nele).",
        obj_kind: objective_kind::EXPLORE, obj_count: 1,
        obj_x: 195.5, obj_y: 1030.5, obj_radius: 4.0,
        reward_xp: 40, ..q() },
    QuestDef { id: 901, title: "Fique Perto das Arvores",
        desc: "Coleta e' automatica: nao precisa de ferramenta nem clique. Fique parado perto das arvores ate a primeira Madeira cair.",
        obj_kind: objective_kind::COLLECT, obj_target: item_id::WOOD_T1, obj_count: 1,
        reward_xp: 40, ..q() },
    QuestDef { id: 902, title: "Colha e Entregue Madeira",
        desc: "Quanto mais arvore em volta, mais rapido vem. Junte 5 Madeiras e ENTREGUE pro Matteo (fale com ele).",
        obj_kind: objective_kind::DELIVER, obj_target: item_id::WOOD_T1, obj_count: 5,
        reward_xp: 60, ..q() },
    QuestDef { id: 903, title: "Forje sua Arma",
        desc: "Va ate a estacao de craft e forje a arma T1 da sua escolha com os materiais do Matteo.",
        obj_kind: objective_kind::COLLECT, obj_target: 0, obj_count: 1,
        reward_xp: 70, ..q() },
    QuestDef { id: 904, title: "Prove seu Valor",
        desc: "Va ate a arena (a leste) e derrote 3 inimigos com sua arma nova. Vai te dar XP pra subir de nivel!",
        obj_kind: objective_kind::KILL, obj_target: 0, obj_count: 3,
        reward_xp: 120, ..q() },
    QuestDef { id: 905, title: "Convoque o Barco",
        desc: "Va ate o cais (ao norte) e USE o item do barco no inventario pra coloca-lo na agua.",
        obj_kind: objective_kind::EXPLORE, obj_count: 1,
        obj_x: 195.5, obj_y: 1050.5, obj_radius: 6.0,
        reward_xp: 50, ..q() },
    // 906/907 são deck-relativos (detectados pela pos do player no convés vs a
    // estação) — obj_kind COLLECT só pra não mostrar marcador de EXPLORE no mundo.
    QuestDef { id: 906, title: "Vá até a Vela",
        desc: "Suba no barco (interaja com ele) e ande pelo convés ate a VELA.",
        obj_kind: objective_kind::COLLECT, obj_target: 0, obj_count: 1,
        reward_xp: 40, ..q() },
    QuestDef { id: 907, title: "Vá até o Leme",
        desc: "Agora ande pelo convés ate o LEME (a roda do timao).",
        obj_kind: objective_kind::COLLECT, obj_target: 0, obj_count: 1,
        reward_xp: 40, ..q() },
    QuestDef { id: 908, title: "Navegue até o Mar",
        desc: "Use a VELA pra ganhar velocidade e o LEME pra virar. Navegue rumo ao mar aberto, ao norte.",
        obj_kind: objective_kind::EXPLORE, obj_count: 1,
        obj_x: 195.5, obj_y: 1085.5, obj_radius: 9.0,
        reward_xp: 100, ..q() },

    // ===================== STORYLINE (4xx) — Lvl 1-10 (Chapter 1) =====================
    QuestDef { id: 401, source: quest_source::NPC, giver: 107,
        title: "O Sal da Terra", desc: "A Armada Real bloqueou o porto. Fale com os pescadores e ajude coletando 15 Madeiras para reparar as barricadas.",
        obj_kind: objective_kind::COLLECT, obj_target: item_id::WOOD_T1, obj_count: 15,
        reward_cobre: 200, reward_xp: 100, reward_item: item_id::HEALTH_POTION, reward_item_qty: 5,
        min_level: 1, ..q() },
    QuestDef { id: 402, source: quest_source::NPC, giver: 107,
        title: "A Força da Forja", desc: "O ferreiro precisa de minério para forjar armas de defesa. Traga 10 Minérios de Ferro T1.",
        obj_kind: objective_kind::COLLECT, obj_target: item_id::STEEL, obj_count: 10,
        reward_cobre: 250, reward_xp: 120, reward_item: item_id::MANA_POTION, reward_item_qty: 5,
        min_level: 2, ..q() },
    QuestDef { id: 403, source: quest_source::NPC, giver: 108,
        title: "O Confronto no Cais", desc: "Derrote 10 capangas da guarnição corrupta da Armada que estão aterrorizando a praia do porto.",
        obj_kind: objective_kind::KILL, obj_target: 0, obj_count: 10,
        reward_cobre: 400, reward_xp: 200, reward_item: item_id::BRACELETE, reward_item_qty: 1,
        min_level: 3, ..q() },
    QuestDef { id: 404, source: quest_source::NPC, giver: 108,
        title: "O Juramento do Mar", desc: "A Armada recuou temporariamente. Vá até as docas ao norte para avaliar a situação e reivindicar seu barco.",
        obj_kind: objective_kind::EXPLORE, obj_count: 1,
        obj_x: 850.0, obj_y: 620.0, obj_radius: 20.0, // Doca da ilha inicial (Perto da cidade 3)
        reward_cobre: 500, reward_xp: 300, reward_item: item_id::BOAT_ESQUIFE, reward_item_qty: 1,
        min_level: 4, ..q() },

    // ===================== STORYLINE (4xx) — Lvl 10-30 (Chapter 2) =====================
    QuestDef { id: 405, source: quest_source::NPC, giver: 107,
        title: "Seguindo o Vento", desc: "Leve 10 Madeiras T1 para o posto avançado na ilha a leste (siga a bússola até a área indicada).",
        obj_kind: objective_kind::TRANSPORT, obj_target: item_id::WOOD_T1, obj_count: 10,
        obj_x: 1200.0, obj_y: 800.0, obj_radius: 30.0,
        reward_cobre: 300, reward_xp: 200, min_level: 10, ..q() },
    QuestDef { id: 406, source: quest_source::NPC, giver: 107,
        title: "A Relíquia Submersa", desc: "Elimine 12 criaturas hostis na praia para limpar as cavernas de contrabando e recuperar o primeiro Anel do Abismo.",
        obj_kind: objective_kind::KILL, obj_target: 0, obj_count: 12,
        reward_cobre: 400, reward_xp: 350, reward_item: item_id::BRACELETE, reward_item_qty: 1,
        min_level: 12, ..q() },
    QuestDef { id: 407, source: quest_source::NPC, giver: 108,
        title: "O Forte Costeiro", desc: "Invada a praia do forte e derrote 15 soldados da Armada Real do Sol que controlam o primeiro fragmento do mapa.",
        obj_kind: objective_kind::KILL, obj_target: 0, obj_count: 15,
        reward_cobre: 600, reward_xp: 500, reward_item: item_id::GLITTERING_POWDER, reward_item_qty: 2,
        min_level: 15, ..q() },

    // ===================== STORYLINE (4xx) — Lvl 30-50 (Chapter 3) =====================
    QuestDef { id: 408, source: quest_source::NPC, giver: 105,
        title: "Calor e Cinzas", desc: "Colete 15 Minérios de Ferro T2 nas encostas do vulcão para forjar escudos de proteção contra calor.",
        obj_kind: objective_kind::COLLECT, obj_target: item_id::na_cor(item_id::STEEL, 2), obj_count: 15,
        reward_cobre: 800, reward_xp: 1500, reward_item: item_id::DARKSTEEL, reward_item_qty: 5,
        min_level: 30, ..q() },
    QuestDef { id: 409, source: quest_source::NPC, giver: 105,
        title: "O Despertar da Terra", desc: "Sabote as operações inimigas eliminando 20 criaturas vulcânicas nas profundezas das minas.",
        obj_kind: objective_kind::KILL, obj_target: 0, obj_count: 20,
        reward_cobre: 1000, reward_xp: 2500, min_level: 40, ..q() },
    QuestDef { id: 410, source: quest_source::NPC, giver: 106,
        title: "O Coração de Pedra", desc: "Derrote 15 guardiões de magma e recupere o Anel da Ignição Negra e o segundo fragmento do mapa.",
        obj_kind: objective_kind::KILL, obj_target: 0, obj_count: 15,
        reward_cobre: 1500, reward_xp: 4000, reward_item: item_id::BRINCO, reward_item_qty: 1,
        min_level: 45, ..q() },

    // ===================== STORYLINE (4xx) — Lvl 50-70 (Chapter 4) =====================
    QuestDef { id: 411, source: quest_source::NPC, giver: 101,
        title: "Fogo Cruzado", desc: "Destrua a guarnição externa da Fortaleza de Ferro eliminando 25 soldados da Armada Real.",
        obj_kind: objective_kind::KILL, obj_target: 0, obj_count: 25,
        reward_cobre: 2000, reward_xp: 8000, min_level: 50, ..q() },
    QuestDef { id: 412, source: quest_source::NPC, giver: 101,
        title: "Resgate Ousado", desc: "Infiltre-se nas masmorras e alcance a cela onde o historiador real está preso (siga a bússola).",
        obj_kind: objective_kind::EXPLORE, obj_count: 1,
        obj_x: 11100.0, obj_y: 639.0, obj_radius: 30.0,
        reward_cobre: 2500, reward_xp: 12000, reward_item: item_id::GLITTERING_POWDER, reward_item_qty: 5,
        min_level: 60, ..q() },
    QuestDef { id: 413, source: quest_source::NPC, giver: 102,
        title: "Fuga dos Redemoinhos", desc: "Navegue através da frota de bloqueio e transporte o historiador ferido até a ilha segura ao norte.",
        obj_kind: objective_kind::TRANSPORT, obj_target: item_id::WOOD_T3, obj_count: 5,
        obj_x: 6820.0, obj_y: 624.0, obj_radius: 50.0,
        reward_cobre: 3000, reward_xp: 18000, min_level: 65, ..q() },

    // ===================== STORYLINE (4xx) — Lvl 70-85 (Chapter 5) =====================
    QuestDef { id: 414, source: quest_source::NPC, giver: 103,
        title: "Pelas Névoas", desc: "Navegue no mar enevoado e colete 20 couros T4 para preparar velas resistentes à umidade do cemitério de navios.",
        obj_kind: objective_kind::COLLECT, obj_target: item_id::LEATHER_T4, obj_count: 20,
        reward_cobre: 4000, reward_xp: 30000, min_level: 70, ..q() },
    QuestDef { id: 415, source: quest_source::NPC, giver: 103,
        title: "Exorcismo Marítimo", desc: "Derrote 30 marinheiros fantasmas que assombram as brumas do leste para purificar o caminho.",
        obj_kind: objective_kind::KILL, obj_target: 0, obj_count: 30,
        reward_cobre: 5000, reward_xp: 45000, reward_item: item_id::AMULETO, reward_item_qty: 1,
        min_level: 78, ..q() },
    QuestDef { id: 416, source: quest_source::NPC, giver: 104,
        title: "O Guardião das Profundezas", desc: "Confronte os monstros marinhos mutantes nas brumas profundas e derrote 15 criaturas de elite.",
        obj_kind: objective_kind::KILL, obj_target: 0, obj_count: 15,
        reward_cobre: 6000, reward_xp: 60000, reward_item: item_id::na_cor(item_id::SCALE, 3), reward_item_qty: 2,
        min_level: 82, ..q() },

    // ===================== STORYLINE (4xx) — Lvl 85-100 (Chapter 6) =====================
    QuestDef { id: 417, source: quest_source::NPC, giver: 109,
        title: "A Quebra do Bloqueio", desc: "Enfrente a frota de elite da Armada Real derrotando 35 dos seus melhores combatentes no Abismo Central.",
        obj_kind: objective_kind::KILL, obj_target: 0, obj_count: 35,
        reward_cobre: 8000, reward_xp: 100000, min_level: 85, ..q() },
    QuestDef { id: 418, source: quest_source::NPC, giver: 109,
        title: "Desbravando o Furacão", desc: "Alcance o topo da montanha do Templo das Marés no centro do Maelstrom para consagrar a busca.",
        obj_kind: objective_kind::EXPLORE, obj_count: 1,
        obj_x: 6820.0, obj_y: 624.0, obj_radius: 30.0,
        reward_cobre: 10000, reward_xp: 150000, min_level: 92, ..q() },
    QuestDef { id: 419, source: quest_source::NPC, giver: 110,
        title: "O Coração da Tempestade", desc: "Confronte e elimine os generais de elite da guarnição final do Almirante Vane e recupere o Coração da Tempestade.",
        obj_kind: objective_kind::KILL, obj_target: 0, obj_count: 20,
        reward_cobre: 15000, reward_xp: 250000, reward_item: item_id::CINTO, reward_item_qty: 1,
        min_level: 96, ..q() },
];

/// Cadeia ordenada das quests de tutorial. Concluir uma concede a próxima
/// (auto em TUTORIAL_MODE); concluir a última finaliza o tutorial.
/// Ordem: pular+falar → equipar → colher+entregar → forjar → combater →
/// convocar barco → ir à vela → ir ao leme → navegar.
pub const TUTORIAL_CHAIN: &[u16] = &[900, 901, 902, 903, 904, 905, 906, 907, 908];
/// Primeira quest da cadeia (concedida no spawn do tutorial).
pub fn tutorial_first() -> u16 { TUTORIAL_CHAIN[0] }
/// Próxima quest após `id` na cadeia (None se foi a última).
pub fn tutorial_next(id: u16) -> Option<u16> {
    let i = TUTORIAL_CHAIN.iter().position(|&q| q == id)?;
    TUTORIAL_CHAIN.get(i + 1).copied()
}
/// `id` faz parte da cadeia de tutorial?
pub fn is_tutorial_quest(id: u16) -> bool { TUTORIAL_CHAIN.contains(&id) }
