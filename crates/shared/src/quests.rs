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
    /// Criar N no painel de Craft. `obj_target` 0 = qualquer equipamento;
    /// outro valor = ESTE item_id (`conta_craft`).
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
    /// Passo TUTORIAL da historia: fazer uma acao da interface uma vez
    /// (`obj_target` = `tutorial::*`). O cliente avisa com
    /// `ClientMessage::Tutorial` quando o jogador faz o gesto.
    pub const TUTORIAL: u8 = 17;
}

/// As acoes que os passos tutoriais ensinam (`obj_target` de TUTORIAL).
pub mod tutorial {
    /// Ajustou a % de vida em que a pocao e' bebida (Menu › Sistema › Barra).
    pub const POCAO_LIMIAR: u16 = 1;
    /// Arrastou uma skill pra CIMA: uso automatico ligado.
    pub const SKILL_AUTO: u16 = 2;
    /// Ligou o AUTO COMBATE.
    pub const AUTO_COMBATE: u16 = 3;
    /// Ligou o AUTO COLETA.
    pub const AUTO_COLETA: u16 = 4;
    /// Tocou num lugar do mapa e o personagem foi sozinho.
    pub const MAPA_IR: u16 = 5;
    /// Coletou um cristal de Energia (docs/SKILLS.md).
    pub const COLETA_ENERGIA: u16 = 6;
    /// Distribuiu um ponto de atributo na Ficha (docs/PERSONAGEM.md).
    pub const PONTO_ATRIBUTO: u16 = 7;
    /// Evoluiu uma habilidade de tier, gastando Energia.
    pub const EVOLUIR_SKILL: u16 = 8;
    /// Abriu Missões, escolheu o passo na lista e tocou Fazer.
    pub const MISSAO_MENU: u16 = 14;

    // ── A PROPRIA ILHA (docs/COLONIA.md) ──
    //
    // A ilha tem regra propria e nenhuma delas esta' em outro lugar do jogo:
    // o painel abre num MURAL (e nao no Menu), a colheita cai num BAU (e nao
    // na bolsa) e ela so' rende com MORADOR. Tres coisas que o jogador nao
    // tem como adivinhar — e ele chega la' uma vez, no meio do capitulo I.
    /// Abriu o painel da ilha (Menu › Minha Ilha).
    ///
    /// Era "interagiu com o mural", de quando a ilha era uma zona em que se
    /// andava. Ela virou painel: o gesto passou a ser ABRIR.
    pub const COLONIA_PAINEL: u16 = 9;
    /// Subiu o assentamento (casa → vila).
    pub const COLONIA_ASSENTAMENTO: u16 = 10;
    /// Contratou o primeiro morador.
    pub const COLONIA_CONTRATAR: u16 = 11;
    /// Colheu pro bau da ilha.
    pub const COLONIA_COLHER: u16 = 12;
    /// Tirou do bau pra bolsa.
    pub const COLONIA_RETIRAR: u16 = 13;

    /// Este tutorial tem condicao de ESTADO, e nao so' de acao?
    ///
    /// Os cinco primeiros sao toques na interface: quem ja' os fez pode
    /// faze-los de novo, entao esperar o evento sempre funciona. Os tres
    /// ultimos mexem em SALDO — Energia, ponto de atributo, tier de skill —
    /// e o saldo pode ja' estar gasto quando o passo abre.
    ///
    /// Foi assim que o dono travou a historia em 21/09/2026: chegou ao nivel
    /// 4 com os pontos JA' gastos, o passo "coloque um ponto" abriu sem haver
    /// ponto pra gastar, e nao havia como fechar ele antes do nivel 5.
    pub fn tem_estado(acao: u16) -> bool {
        matches!(
            acao,
            COLETA_ENERGIA
                | PONTO_ATRIBUTO
                | EVOLUIR_SKILL
                // Os da ilha tambem sao de SALDO, e pela mesma razao: quem ja'
                // subiu o assentamento ou ja' tem morador nao teria como
                // refazer o gesto, e a historia travaria na propria ilha —
                // longe do Capitao, que e' a unica saida de la'.
                | COLONIA_ASSENTAMENTO
                | COLONIA_CONTRATAR
        )
    }

    /// O que fazer, curto, pro rastreador.
    pub fn instrucao(acao: u16) -> &'static str {
        match acao {
            POCAO_LIMIAR => "Menu › System › Bar: set the potion %",
            SKILL_AUTO => "Drag a skill UP",
            AUTO_COMBATE => "Tap COMBAT",
            AUTO_COLETA => "Tap GATHER",
            MAPA_IR => "Open the map and tap a place",
            COLETA_ENERGIA => "Collect Energy from the blue crystals",
            PONTO_ATRIBUTO => "Menu › Sheet: spend a point",
            EVOLUIR_SKILL => "Menu › Skills: evolve one tier",
            MISSAO_MENU => "Open Menu › Quests, pick the quest and tap Do",
            COLONIA_PAINEL => "Open Menu › My Island",
            COLONIA_ASSENTAMENTO => "On the board: upgrade the Settlement",
            COLONIA_CONTRATAR => "No mural: escolha um ofício na casa vazia",
            COLONIA_COLHER => "On the board: Harvest",
            COLONIA_RETIRAR => "On the board: Withdraw, from the island chest",
            _ => "Follow the hint",
        }
    }
}

/// `obj_target` de GATHER.
/// Does crafting `feito` count for a CRAFT objective aimed at `alvo`?
/// 0 is "any piece" (every daily and the first armour); any other value is
/// the one item the step asks for.
pub fn conta_craft(alvo: u16, feito: u16) -> bool {
    alvo == 0 || alvo == feito
}

pub mod alvo_de_coleta {
    pub const QUALQUER: u16 = 0;
    pub const PEDRA: u16 = 1;
    pub const ARVORE: u16 = 2;

    /// A coleta de um corpo de `tier` (0 = arvore, 1..4 = pedra, 5 = Energia)
    /// conta pra este alvo?
    pub fn conta(alvo: u16, tier: u8) -> bool {
        match alvo {
            PEDRA => (1..=4).contains(&tier),
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
    matches!(
        d.obj_kind,
        objective_kind::KILL | objective_kind::GATHER | objective_kind::COLLECT
    )
}

/// Quem da' as missoes da ilha: o Mestre de Missoes da praca
/// (`construcao::Papel::Missoes`). Fora das faixas de loja (1..8), facção
/// (1..2), arauto (101..112) e morador (99).
pub const GIVER_MESTRE_DA_ILHA: u16 = 121;

/// Os outros NPCs da vila tambem DAO missao: cada oficio e' um giver,
/// `140 + papel` (fora das faixas acima). Quem da' e' quem recebe a entrega —
/// as cadeias do Bosque passam de um NPC pro outro (511-538).
pub const GIVER_DA_VILA: u16 = 140;

/// NPCs das cabanas espalhadas pelo Bosque e pela Geleira.
pub const POSTOS: [(u16, &str); 10] = [
    (201, "Woodcutter of the Trail"),
    (202, "Glade Watch"),
    (203, "Lookout Guide"),
    (204, "Walrus Watch"),
    (205, "Guardian of the Forest"),
    (206, "Bear Scout"),
    (207, "Explorer of the Ice"),
    (208, "Provisioner of the Waste"),
    (209, "Researcher of the Dunes"),
    (210, "Shipwreck Watch"),
];

pub fn posto_por_nome(nome: &str) -> Option<u16> {
    POSTOS.iter().find(|(_, n)| *n == nome).map(|(id, _)| *id)
}

pub fn nome_do_posto(giver: u16) -> Option<&'static str> {
    POSTOS.iter().find(|(id, _)| *id == giver).map(|(_, n)| *n)
}

/// O giver de um NPC da vila. O Mestre continua com o dele.
pub const fn giver_do_papel(p: crate::construcao::Papel) -> u16 {
    match p {
        crate::construcao::Papel::Missoes => GIVER_MESTRE_DA_ILHA,
        _ => GIVER_DA_VILA + p as u16,
    }
}

/// O giver do NPC da vila de papel `papel` (como u16: `Papel as u16`, o que
/// o servidor tira do nome e o cliente do `kind`). Armas e Armaduras sao o
/// mesmo "Armourer": os dois dao o giver de Armaduras.
pub fn giver_do_npc(papel: u16) -> Option<u16> {
    use crate::construcao::Papel;
    let p = PAPEIS_DE_CONVERSA
        .iter()
        .chain([Papel::Armas].iter())
        .copied()
        .find(|p| *p as u16 == papel)?;
    Some(giver_do_papel(if p == Papel::Armas {
        Papel::Armaduras
    } else {
        p
    }))
}

/// O NPC da vila por tras de um giver (o Mestre inclusive).
pub fn papel_do_giver(g: u16) -> Option<crate::construcao::Papel> {
    use crate::construcao::Papel;
    if nome_do_posto(g).is_some() {
        return Some(Papel::Missoes);
    }
    if g == GIVER_MESTRE_DA_ILHA {
        return Some(Papel::Missoes);
    }
    PAPEIS_DE_CONVERSA
        .iter()
        .chain([Papel::Armas].iter())
        .copied()
        .find(|p| *p != Papel::Missoes && GIVER_DA_VILA + *p as u16 == g)
}

/// Nome do NPC que da' (e recebe) a missao, pra dizer "fale com o X".
pub fn quem_da(d: &QuestDef) -> Option<&'static str> {
    if let Some(nome) = nome_do_posto(d.giver) {
        return Some(nome);
    }
    papel_do_giver(d.giver).map(|p| p.nome())
}

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
    // A "coming soon" quest stays out of every island's menu: it cannot be
    // completed yet, and showing it is a broken promise.
    if quest_by_id(id).is_some_and(|q| q.em_breve) {
        return None;
    }
    match id {
        500..=609 | 912 => Some("ilha_inicial"),
        810..=833 | 913 => Some("ilha_gelo"),
        610..=619 => Some("ilha_gelo"),
        620..=629 | 840..=854 | 914 => Some("ilha_deserto"),
        630..=639 | 860..=864 => Some("ilha_planalto"),
        640..=649 | 865..=869 | 906..=911 => Some("ilha_celeste"),
        650..=669 => Some("ilha_kogen"),
        670..=689 => Some("ilha_abissal"),
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
    /// Passo tutorial: nao anda; o cliente abre o painel ou mostra a dica da
    /// acao. `raio` leva a acao (`tutorial::*`).
    pub const TUTORIAL: u8 = 10;
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
            "Ah, fresh blood in the square! Welcome to the island.",
            "Before you set foot outside the city, you need potions in your pocket.",
            "Go talk to the Alchemist — the shop with the green awning. Tell him I sent you.",
        ],
        (501, momento::CONVERSA) => &[
            "The Master sent you? Then you're the newcomer they mentioned.",
            "Without potions, the woods swallow anyone. Here I sell health, mana and stamina.",
            "Remember this: drink before you fall, not after. Now go back to the Master.",
        ],
        (501, momento::ENTREGA) => &[
            "Met the old Alchemist? Good.",
            "Take a few potions on the house. Don't spend them all at once.",
        ],
        (502, momento::OFERTA) => &[
            "The wolves are prowling the trails outside the city again.",
            "The merchants no longer travel without an escort.",
            "Defeat six of them and the road breathes again.",
        ],
        (502, momento::ENTREGA) => &[
            "Six fewer wolves on the road. The merchants thank you.",
            "You fight better than you look. Here's your pay.",
        ],
        (503, momento::OFERTA) => &[
            "The Blacksmith is out of metal for the village's tools.",
            "Every beast on this island carries a little copper — nobody knows why.",
            "Bring thirty Copper and the forge sings again.",
        ],
        (503, momento::ENTREGA) => &[
            "Thirty Copper, counted out. The Blacksmith will be pleased.",
            "Take these mana potions. You'll need them further on.",
        ],
        (504, momento::OFERTA) => &[
            "Bears have come down the slope and are frightening the woodcutters.",
            "This isn't a job for a novice — and you aren't one any more.",
            "Defeat three bears and come back whole.",
        ],
        (504, momento::ENTREGA) => &[
            "Three bears! The woodcutters are back in the woods already.",
            "The island is starting to speak your name. Take your reward.",
        ],
        (505, momento::OFERTA) => &[
            "The Harbour Captain wants to meet whoever protects the island.",
            "Follow the road to the quay — the map shows the anchor.",
            "Talk to him and then tell me what he said.",
        ],
        (505, momento::CONVERSA) => &[
            "So you're the hero from the square. Come closer, the wind is strong here.",
            "The boats to the other islands leave from here: ice, desert and plateau.",
            "When you're ready, the sea is waiting for you. Tell the Master the port is open.",
        ],
        (505, momento::ENTREGA) => &[
            "The Captain took a liking to you? Then the sea is your next step.",
            "These larger potions will keep you standing far from home. Safe travels.",
        ],
        _ => &[],
    };
    if !escritas.is_empty() {
        return escritas.to_vec();
    }
    match m {
        momento::OFERTA => {
            vec![quest_by_id(quest_id).map_or("I have a task for you.", |d| d.desc)]
        }
        momento::CONVERSA => vec!["Ah, you came. Thank you for stopping by."],
        _ => vec!["Good work. Here is your reward."],
    }
}

/// `obj_target` de KILL que quer UM tipo de mob. O 0 continua sendo "qualquer
/// um" — e o kind 0 da tabela e' o Lobo, dai' o deslocamento de um.
pub const fn alvo_de_mob(kind: u16) -> u16 {
    kind + 1
}

/// `obj_target` de KILL que aceita QUALQUER chefe de campo (`bosses::CHEFES`).
/// Longe de todo kind real (`alvo_de_mob` de chefe fica em 11..18).
pub const ALVO_QUALQUER_CHEFE: u16 = 1_000;

/// A morte de um bicho de `kind` conta pra uma KILL com este `obj_target`?
pub fn kill_conta(obj_target: u16, kind: u16) -> bool {
    obj_target == 0
        || alvo_de_mob(kind) == obj_target
        // An island variant counts for its species: "Defeat 8 archers" on the
        // Glacier is the Frostcoat Archer (`bestiary`).
        || alvo_de_mob(crate::bestiary::species_of(kind)) == obj_target
        || (obj_target == ALVO_QUALQUER_CHEFE && crate::bosses::e_chefe(kind))
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
    &[
        Alquimista,
        Estaleiro,
        Ferreiro,
        Armaduras,
        Taberna,
        Alfaiate,
        Treinador,
        Identificador,
        Deposito,
        Cartografo,
        Missoes,
    ]
};

/// Papel (como `obj_target` de TALK) do NPC da vila com este nome.
pub fn papel_de_conversa(nome: &str) -> Option<u16> {
    PAPEIS_DE_CONVERSA
        .iter()
        .find(|p| p.nome() == nome)
        .map(|p| *p as u16)
}

/// Status de uma quest por personagem.
pub mod quest_status {
    pub const ACTIVE: u8 = 0; // aceita, em progresso
    pub const READY: u8 = 1; // objetivo cumprido, falta entregar (turn-in)
    pub const TURNED_IN: u8 = 2; // concluída (repetível volta a poder aceitar após cooldown)
}

/// Definição estática de uma quest. Achatada de propósito (DB/wire/C#).
#[derive(Debug, Clone, Copy)]
pub struct QuestDef {
    pub id: u16,
    pub source: u8, // quest_source
    /// Quem dá a quest: shop_id do NPC (NPC), 0 (BOARD), ou faction_id (FACTION).
    pub giver: u16,
    pub faction: u8, // faction_id (gating + recompensa de pontos)
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
    /// "Coming soon" e o servidor nao deixa aceitar.
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
            reward_xp: crate::progressao::xp_da_quest(d),
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
    QUESTS
        .iter()
        .find(|q| q.id == id)
        .or_else(|| crate::historia::def_da_historia(id))
}

/// Uma `QuestDef` zerada, pra quem monta definicoes fora deste arquivo (a
/// historia).
pub const fn quest_vazia() -> QuestDef {
    q()
}

/// Loja de facção: (item_id, custo em PONTOS DE FACÇÃO). Mesma oferta pras duas
/// facções por ora (recompensas premium compradas com pontos das quests PvP).
pub const FACTION_SHOP: &[(u16, u32)] = &[
    (item_id::GREATER_HEAL, 15),
    (item_id::GREATER_MANA, 15),
    (item_id::GLITTERING_POWDER, 40),
    (item_id::ARMADURA_PESADA, 90),
    (item_id::ESPADA_E_ESCUDO, 140),
    (item_id::PISTOLAS, 140),
];

/// Preço em pontos de um item na loja de facção, ou None se não vendido.
pub fn faction_shop_price(item_id: u16) -> Option<u32> {
    FACTION_SHOP
        .iter()
        .find(|(id, _)| *id == item_id)
        .map(|(_, p)| *p)
}

// Helper de construção (mantém os literais legíveis sem repetir todo campo).
const fn q() -> QuestDef {
    QuestDef {
        id: 0,
        source: quest_source::BOARD,
        giver: 0,
        faction: faction_id::NONE,
        title: "",
        desc: "",
        obj_kind: objective_kind::COLLECT,
        obj_target: 0,
        obj_count: 1,
        obj_x: 0.0,
        obj_y: 0.0,
        obj_radius: 0.0,
        reward_cobre: 0,
        reward_xp: 0,
        reward_item: 0,
        reward_item_qty: 0,
        reward_faction_points: 0,
        min_level: 1,
        repeatable: false,
        daily: false,
        cooldown_secs: 0,
        requires: 0,
        em_breve: false,
        reward_item2: 0,
        reward_item2_qty: 0,
    }
}

/// Uma DIARIA do Mestre de Missoes: repetivel, volta a meia-noite UTC. As de
/// area pagam tambem 1 Pocao de Experiencia.
#[allow(clippy::too_many_arguments)]
const fn diaria(
    id: u16,
    title: &'static str,
    desc: &'static str,
    obj_kind: u8,
    obj_target: u16,
    obj_count: u32,
    reward_cobre: u32,
    reward_xp: u64,
    reward_item: u16,
    reward_item_qty: u16,
    min_level: u32,
    area: bool,
    em_breve: bool,
) -> QuestDef {
    QuestDef {
        id,
        title,
        desc,
        obj_kind,
        obj_target,
        obj_count,
        reward_cobre,
        reward_xp,
        reward_item,
        reward_item_qty,
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
        reward_item2_qty: if area
            || obj_kind == objective_kind::CRAFT
            || obj_kind == objective_kind::REFINE
        {
            1
        } else {
            0
        },
        min_level,
        repeatable: true,
        daily: true,
        em_breve,
        ..mestre()
    }
}

// Atalho pra um NPC da vila como quem da' a missao.
const fn de(p: crate::construcao::Papel) -> QuestDef {
    QuestDef {
        source: quest_source::NPC,
        giver: giver_do_papel(p),
        ..q()
    }
}

const fn posto(giver: u16) -> QuestDef {
    QuestDef {
        source: quest_source::NPC,
        giver,
        ..q()
    }
}

// Atalho pro Mestre de Missoes da ilha inicial.
const fn mestre() -> QuestDef {
    QuestDef {
        source: quest_source::NPC,
        giver: GIVER_MESTRE_DA_ILHA,
        ..q()
    }
}

use crate::constants::item_id;

/// Registry inicial de quests. Seedado no Postgres (`quest_defs`) e hot-reload.
/// IDs por faixa: 1xx = Board, 2xx = NPC, 3xx = Facção.
pub const QUESTS: &[QuestDef] = &[
    // ===================== MESTRE DE MISSOES (5xx) — ilha inicial =====================
    // Cadeia de boas-vindas, na ordem: conhecer a cidade, a primeira caca, o
    // loot dela, a primeira viagem longa. Tudo com o que a ilha tem de verdade:
    // lobo e urso da tabela, cobre que todo mob dropa, NPCs da vila e do porto.
    QuestDef { id: 501, title: "Meet the Alchemist",
        desc: "Every journey starts with potions in your pocket. Talk to the Alchemist, at the door of the green-awning shop.",
        obj_kind: objective_kind::TALK, obj_target: crate::construcao::Papel::Alquimista as u16, obj_count: 1,
        reward_cobre: 30, reward_xp: 40, reward_item: item_id::HEALTH_POTION, reward_item_qty: 3,
        ..mestre() },
    QuestDef { id: 502, title: "Wolves on the road",
        desc: "The wolves prowl the trails outside the city. Defeat 6 wolves and come back here.",
        obj_kind: objective_kind::KILL, obj_target: alvo_de_mob(mob_kind::LOBO), obj_count: 6,
        reward_cobre: 80, reward_xp: 120, reward_item: item_id::HEALTH_POTION, reward_item_qty: 2,
        reward_item2: item_id::XP_POTION, reward_item2_qty: 1,
        requires: 501, ..mestre() },
    QuestDef { id: 503, title: "Copper for the forge",
        desc: "The Blacksmith needs metal for the village's tools. Defeat 15 beasts — every one carries a little copper.",
        obj_kind: objective_kind::KILL, obj_target: 0, obj_count: 15,
        reward_cobre: 120, reward_xp: 150, reward_item: item_id::MANA_POTION, reward_item_qty: 3,
        reward_item2: item_id::XP_POTION, reward_item2_qty: 1,
        requires: 502, ..mestre() },
    QuestDef { id: 504, title: "Bears in the woods",
        desc: "Bears have come down the slope and are frightening the woodcutters. Defeat 3 bears.",
        obj_kind: objective_kind::KILL, obj_target: alvo_de_mob(mob_kind::URSO), obj_count: 3,
        reward_cobre: 160, reward_xp: 220, reward_item: item_id::HEALTH_POTION, reward_item_qty: 3,
        reward_item2: item_id::XP_POTION, reward_item2_qty: 1,
        requires: 503, min_level: 3, ..mestre() },
    QuestDef { id: 505, title: "Visit the Port",
        desc: "The Harbour Captain wants to meet whoever protects the island. Follow the road to the quay (M opens the map) and talk to him.",
        obj_kind: objective_kind::TALK, obj_target: crate::construcao::Papel::Estaleiro as u16, obj_count: 1,
        reward_cobre: 250, reward_xp: 300, reward_item: item_id::GREATER_HEAL, reward_item_qty: 2,
        requires: 504, ..mestre() },

    // ===== A oficina do Mestre (506-509) — de onde vem cada material =====
    // A historia manda "crie sua primeira peca" e nunca diz ONDE se acha o que
    // a receita pede. Uma peca cinza custa 1 chave + 30 aco + 10 quintessencia
    // + 10 berloque + 200 darksteel + 300 cobre, e o inicio so' apresentava
    // pedra e cobre. Esta cadeia apresenta o resto, cada quest com uma fonte.
    QuestDef { id: 506, title: "Firewood for the quay",
        desc: "The quay lost half a pier in the storm and the Captain needs wood. Fell 8 trees — it's the only place wood comes from, and nobody had told you.",
        obj_kind: objective_kind::GATHER, obj_target: alvo_de_coleta::ARVORE, obj_count: 8,
        reward_cobre: 200, reward_xp: 350, reward_item: item_id::HEALTH_POTION, reward_item_qty: 3,
        reward_item2: item_id::XP_POTION, reward_item2_qty: 1,
        requires: 505, ..mestre() },
    QuestDef { id: 507, title: "What the stone keeps",
        desc: "Inside the rock there's a dark metal that doesn't rust: Darksteel. Break 10 rocks so the Blacksmith can judge the quality of the vein here.",
        obj_kind: objective_kind::GATHER, obj_target: alvo_de_coleta::PEDRA, obj_count: 10,
        reward_cobre: 260, reward_xp: 450, reward_item: item_id::STEEL, reward_item_qty: 10,
        reward_item2: item_id::XP_POTION, reward_item2_qty: 1,
        requires: 506, ..mestre() },
    QuestDef { id: 508, title: "Quintessence",
        desc: "Rock gives quintessence a drop at a time; tigers give far more. Defeat 8 tigers — without it no armour closes.",
        obj_kind: objective_kind::KILL, obj_target: alvo_de_mob(3), obj_count: 8,
        reward_cobre: 320, reward_xp: 600, reward_item: item_id::STEEL, reward_item_qty: 12,
        reward_item2: item_id::GREATER_HEAL, reward_item2_qty: 2,
        requires: 507, min_level: 8, ..mestre() },
    QuestDef { id: 509, title: "The owlbear's charm",
        desc: "One last piece of the recipe is missing: the charm the owlbears carry tangled in their fur. Defeat 6 owlbears and the Master opens the village chest — and lets out the cub that's been prowling around the storehouse.",
        obj_kind: objective_kind::KILL, obj_target: alvo_de_mob(5), obj_count: 6,
        reward_cobre: 400, reward_xp: 900, reward_item: item_id::HIDE, reward_item_qty: 1,
        // O primeiro PET (docs/PETS.md). Todo mundo ganha o cinza pela historia
        // principal: o auto-loot e' mecanica do jogo, nao privilegio de loja.
        reward_item2: item_id::PET_BASE, reward_item2_qty: 1,
        requires: 508, min_level: 12, ..mestre() },
    QuestDef { id: 510, title: "What lies under the wreck",
        desc: "The Shipwreck Cellar keeps what the tide didn't take, and whoever rules down there falls with the key in his pocket: a dungeon boss drops keys three times as often as a field boss. Go in and clear one — solo is enough.",
        obj_kind: objective_kind::DUNGEON, obj_target: 0, obj_count: 1,
        reward_cobre: 500, reward_xp: 1_200, reward_item: item_id::GREATER_HEAL, reward_item_qty: 3,
        reward_item2: item_id::STEEL, reward_item2_qty: 20,
        requires: 509, min_level: 6, ..mestre() },

    // ===== Mais Bosque (511-538) — seis cadeias, de NPC em NPC =====
    // O capitulo I da historia acaba no nivel 15 e a cadeia de cima no 12; o
    // resto da ilha (tres chefes de campo, sete bichos, duas dungeons, a
    // oficina) ficava sem missao nenhuma. Cada cadeia e' uma linha de
    // subquests em SEQUENCIA, e cada passo se pega (e se entrega) com um NPC
    // diferente da vila — `de(Papel)`. Eram todas do Mestre, que oferecia as
    // seis de uma vez (pedido do dono em 19/09/2026: "tem que ser quest line
    // pegando com diferentes NPCs"). XP na escala do nivel pedido.
    //
    // A) Os chefes de campo do Bosque, do mais fraco pro mais forte.
    QuestDef { id: 511, title: "The howl in the glade",
        desc: "A wolf the size of a horse leads the pack in the glade. Defeat the Alpha Wolf of the Glade — the map marks where he prowls. Bring someone: bosses are fought in a party, and everyone who fights nearby counts.",
        obj_kind: objective_kind::KILL, obj_target: alvo_de_mob(10), obj_count: 1,
        reward_cobre: 600, reward_xp: 6_000, reward_item: item_id::GREATER_HEAL, reward_item_qty: 3,
        reward_item2: item_id::XP_POTION, reward_item2_qty: 1,
        min_level: 8, ..posto(203) },
    QuestDef { id: 512, title: "Stormbeard's flag",
        desc: "Captain Stormbeard has landed with his crew and planted a flag on the island. Bring him down and the port breathes again.",
        obj_kind: objective_kind::KILL, obj_target: alvo_de_mob(11), obj_count: 1,
        reward_cobre: 900, reward_xp: 14_000, reward_item: item_id::GREATER_HEAL, reward_item_qty: 3,
        reward_item2: item_id::SORTE_POTION, reward_item2_qty: 1,
        requires: 511, min_level: 11, ..posto(203) },
    QuestDef { id: 513, title: "The old man on the slope",
        desc: "The Elder Bear has slept on that slope since before the village. The thunder woke him in a foul mood. Defeat him.",
        obj_kind: objective_kind::KILL, obj_target: alvo_de_mob(12), obj_count: 1,
        reward_cobre: 1_300, reward_xp: 30_000, reward_item: item_id::GLITTERING_POWDER, reward_item_qty: 2,
        reward_item2: item_id::FORTUNA_POTION, reward_item2_qty: 1,
        requires: 512, min_level: 14, ..posto(203) },
    QuestDef { id: 514, title: "Boss hunter",
        desc: "The three bosses of the Grove come back after a while. Defeat 3 field bosses — any of them, as many times as you need.",
        obj_kind: objective_kind::KILL, obj_target: ALVO_QUALQUER_CHEFE, obj_count: 3,
        reward_cobre: 2_000, reward_xp: 45_000, reward_item: item_id::GLITTERING_POWDER, reward_item_qty: 3,
        reward_item2: item_id::XP_POTION, reward_item2_qty: 1,
        requires: 513, min_level: 14, ..posto(203) },
    QuestDef { id: 515, title: "The legend of the Grove",
        desc: "The village already tells stories about you. Defeat 5 more field bosses and the legend becomes true.",
        obj_kind: objective_kind::KILL, obj_target: ALVO_QUALQUER_CHEFE, obj_count: 5,
        reward_cobre: 3_000, reward_xp: 70_000, reward_item: item_id::GREATER_HEAL, reward_item_qty: 5,
        reward_item2: item_id::XP_POTION, reward_item2_qty: 2,
        requires: 514, min_level: 15, ..posto(203) },

    // B) O bestiario: cada bicho da ilha, na ordem em que ele aparece.
    QuestDef { id: 516, title: "The grey pack",
        desc: "The wolves are multiplying near the village. Defeat 12.",
        obj_kind: objective_kind::KILL, obj_target: alvo_de_mob(mob_kind::LOBO), obj_count: 12,
        reward_cobre: 150, reward_xp: 900, reward_item: item_id::HEALTH_POTION, reward_item_qty: 3,
        reward_item2: item_id::XP_POTION, reward_item2_qty: 1,
        min_level: 2, ..posto(202) },
    QuestDef { id: 517, title: "Claws on the trail",
        desc: "The bears have taken the woodcutters' trail. Defeat 10.",
        obj_kind: objective_kind::KILL, obj_target: alvo_de_mob(mob_kind::URSO), obj_count: 10,
        reward_cobre: 250, reward_xp: 2_500, reward_item: item_id::HEALTH_POTION, reward_item_qty: 4,
        reward_item2: item_id::XP_POTION, reward_item2_qty: 1,
        requires: 516, min_level: 4, ..posto(202) },
    QuestDef { id: 518, title: "Gunpowder in the woods",
        desc: "Morgan's gunmen are camped in the woods. Defeat 12 and put out their fires.",
        obj_kind: objective_kind::KILL, obj_target: alvo_de_mob(mob_kind::PISTOLEIRO), obj_count: 12,
        reward_cobre: 400, reward_xp: 6_000, reward_item: item_id::GREATER_HEAL, reward_item_qty: 2,
        reward_item2: item_id::XP_POTION, reward_item2_qty: 1,
        requires: 517, min_level: 7, ..posto(202) },
    QuestDef { id: 519, title: "Stripes on the road",
        desc: "Tigers are hunting on the port road. Defeat 12.",
        obj_kind: objective_kind::KILL, obj_target: alvo_de_mob(mob_kind::TIGRE), obj_count: 12,
        reward_cobre: 600, reward_xp: 12_000, reward_item: item_id::GREATER_HEAL, reward_item_qty: 3,
        reward_item2: item_id::XP_POTION, reward_item2_qty: 1,
        requires: 518, min_level: 10, ..posto(202) },
    QuestDef { id: 520, title: "The mages of thunder",
        desc: "Morgan's mages pull lightning from the sky into the lighthouse stones. Defeat 12.",
        obj_kind: objective_kind::KILL, obj_target: alvo_de_mob(mob_kind::MAGO), obj_count: 12,
        reward_cobre: 800, reward_xp: 20_000, reward_item: item_id::GREATER_MANA, reward_item_qty: 3,
        reward_item2: item_id::XP_POTION, reward_item2_qty: 1,
        requires: 519, min_level: 12, ..posto(202) },
    QuestDef { id: 521, title: "What came down from the ice",
        desc: "Glacier owlbears have crossed the frozen sea and prowl the island's heights. Defeat 5.",
        obj_kind: objective_kind::KILL, obj_target: alvo_de_mob(mob_kind::OWLBEAR), obj_count: 5,
        reward_cobre: 1_200, reward_xp: 40_000, reward_item: item_id::GREATER_HEAL, reward_item_qty: 4,
        reward_item2: item_id::XP_POTION, reward_item2_qty: 1,
        requires: 520, min_level: 15, ..posto(202) },
    QuestDef { id: 522, title: "Trail-clearing",
        desc: "The whole island is still boiling. Defeat 80 beasts of any kind.",
        obj_kind: objective_kind::KILL, obj_target: 0, obj_count: 80,
        reward_cobre: 1_500, reward_xp: 45_000, reward_item: item_id::GREATER_HEAL, reward_item_qty: 5,
        reward_item2: item_id::XP_POTION, reward_item2_qty: 2,
        requires: 520, min_level: 13, ..posto(202) },

    // C) A oficina: refino, craft e o estoque que ele come.
    QuestDef { id: 523, title: "Hammer and anvil",
        desc: "The Blacksmith says an unrefined piece is half a piece. Try refining 3 times at the Forge.",
        obj_kind: objective_kind::REFINE, obj_target: 0, obj_count: 3,
        reward_cobre: 300, reward_xp: 3_000, reward_item: item_id::SORTE_POTION, reward_item_qty: 1,
        min_level: 5, ..de(crate::construcao::Papel::Ferreiro) },
    QuestDef { id: 524, title: "Your own armourer",
        desc: "Anyone who relies on shops won't survive the storm. Create 2 pieces of gear in Craft.",
        obj_kind: objective_kind::CRAFT, obj_target: 0, obj_count: 2,
        reward_cobre: 400, reward_xp: 4_000, reward_item: item_id::FORTUNA_POTION, reward_item_qty: 1,
        requires: 523, min_level: 6, ..de(crate::construcao::Papel::Armaduras) },
    QuestDef { id: 525, title: "Stronger than steel",
        desc: "Try refining 5 more times. Up to +5, failure only takes the material.",
        obj_kind: objective_kind::REFINE, obj_target: 0, obj_count: 5,
        reward_cobre: 700, reward_xp: 12_000, reward_item: item_id::GLITTERING_POWDER, reward_item_qty: 1,
        reward_item2: item_id::SORTE_POTION, reward_item2_qty: 1,
        requires: 524, min_level: 10, ..de(crate::construcao::Papel::Ferreiro) },
    QuestDef { id: 526, title: "Darksteel stock",
        desc: "The village forge is out of dark metal. Break 25 rocks — the darksteel comes out of them.",
        obj_kind: objective_kind::GATHER, obj_target: alvo_de_coleta::PEDRA, obj_count: 25,
        reward_cobre: 900, reward_xp: 15_000, reward_item: item_id::GLITTERING_POWDER, reward_item_qty: 2,
        reward_item2: item_id::XP_POTION, reward_item2_qty: 1,
        requires: 525, min_level: 12, ..de(crate::construcao::Papel::Ferreiro) },
    QuestDef { id: 527, title: "Steel for the wall",
        desc: "The village wall needs reinforcing before the next storm. Break 20 rocks for the steel.",
        obj_kind: objective_kind::GATHER, obj_target: alvo_de_coleta::PEDRA, obj_count: 20,
        reward_cobre: 1_000, reward_xp: 18_000, reward_item: item_id::QUINTESSENCE, reward_item_qty: 10,
        reward_item2: item_id::XP_POTION, reward_item2_qty: 1,
        requires: 526, min_level: 13, ..de(crate::construcao::Papel::Armaduras) },

    // D) Coleta.
    QuestDef { id: 528, title: "Woodcutter",
        desc: "Winter is coming and the village wants firewood. Fell 20 trees.",
        obj_kind: objective_kind::GATHER, obj_target: alvo_de_coleta::ARVORE, obj_count: 20,
        reward_cobre: 200, reward_xp: 1_500, reward_item: item_id::HEALTH_POTION, reward_item_qty: 3,
        reward_item2: item_id::XP_POTION, reward_item2_qty: 1,
        min_level: 3, ..posto(201) },
    QuestDef { id: 529, title: "Deep vein",
        desc: "The vein on the slope still holds plenty. Break 30 rocks.",
        obj_kind: objective_kind::GATHER, obj_target: alvo_de_coleta::PEDRA, obj_count: 30,
        reward_cobre: 350, reward_xp: 4_500, reward_item: item_id::STEEL, reward_item_qty: 15,
        reward_item2: item_id::XP_POTION, reward_item2_qty: 1,
        requires: 528, min_level: 6, ..posto(201) },
    QuestDef { id: 530, title: "Calloused hands",
        desc: "Break rock or fell tree: 60 gathers of any kind.",
        obj_kind: objective_kind::GATHER, obj_target: alvo_de_coleta::QUALQUER, obj_count: 60,
        reward_cobre: 700, reward_xp: 15_000, reward_item: item_id::GLITTERING_POWDER, reward_item_qty: 1,
        reward_item2: item_id::XP_POTION, reward_item2_qty: 1,
        requires: 529, min_level: 11, ..posto(201) },

    // E) As duas dungeons da ilha, de novo e de novo.
    QuestDef { id: 531, title: "Back to the cellar",
        desc: "The Morganeers are back in the Shipwreck Cellar. Clear it again (Dungeons, in the Menu).",
        obj_kind: objective_kind::DUNGEON, obj_target: 1, obj_count: 1,
        reward_cobre: 600, reward_xp: 8_000, reward_item: item_id::GREATER_HEAL, reward_item_qty: 3,
        min_level: 8, ..de(crate::construcao::Papel::Estaleiro) },
    QuestDef { id: 532, title: "The cellar, again",
        desc: "The smuggling is back in the cellar under the quay. Clear the Smuggler's Cellar.",
        obj_kind: objective_kind::DUNGEON, obj_target: 2, obj_count: 1,
        reward_cobre: 1_000, reward_xp: 25_000, reward_item: item_id::GREATER_HEAL, reward_item_qty: 5,
        requires: 531, min_level: 13, ..de(crate::construcao::Papel::Taberna) },
    QuestDef { id: 533, title: "Dungeon rat",
        desc: "Complete 3 dungeons, any of them. Every boss inside can drop a key.",
        obj_kind: objective_kind::DUNGEON, obj_target: 0, obj_count: 3,
        reward_cobre: 1_800, reward_xp: 50_000, reward_item: item_id::XP_POTION, reward_item_qty: 2,
        reward_item2: item_id::GLITTERING_POWDER, reward_item2_qty: 2,
        requires: 532, min_level: 14, ..de(crate::construcao::Papel::Missoes) },

    // E.2) A CACADA: matar MUITO, e o alvo e' a zona cheia.
    //
    // O dono pediu "missoes para matar mais inimigos, tipo 30, 60 etc, e que
    // leva pras zonas q tem maior densidade de mobs". Nao sao diarias: e' uma
    // CADEIA que sobe junto com o jogador — cada uma pede mais que a anterior
    // e exige nivel pra abrir, entao ela acompanha o capitulo em vez de virar
    // tarefa repetida.
    //
    // `obj_target: 0` = qualquer bicho. E' de proposito: o que se quer aqui e'
    // VOLUME, e prender a espécie mandaria o jogador procurar um bicho em vez
    // de procurar um LUGAR CHEIO — que e' onde o auto-caminho leva
    // (`spot_de_coleta_longe` / a zona forte do mapa).
    //
    // A ladder de XP acompanha a da historia no mesmo nivel: 30 mortes valem
    // mais que a caça de 6 do começo e menos que uma dungeon.
    QuestDef { id: 539, title: "Hunting routine",
        desc: "The island is far too crowded. Defeat 30 enemies, any species — look for the dense patches on the map.",
        obj_kind: objective_kind::KILL, obj_target: 0, obj_count: 30,
        reward_cobre: 500, reward_xp: 12_000, reward_item: item_id::GREATER_HEAL, reward_item_qty: 3,
        min_level: 8, ..de(crate::construcao::Papel::Treinador) },
    QuestDef { id: 540, title: "Clearing the woods",
        desc: "That wasn't enough. Defeat 60 enemies — the zones marked on the map pay more per minute.",
        obj_kind: objective_kind::KILL, obj_target: 0, obj_count: 60,
        reward_cobre: 1_200, reward_xp: 30_000, reward_item: item_id::XP_POTION, reward_item_qty: 1,
        reward_item2: item_id::GREATER_HEAL, reward_item2_qty: 3,
        requires: 539, min_level: 11, ..de(crate::construcao::Papel::Treinador) },
    QuestDef { id: 541, title: "Heavy hand",
        desc: "The Trainer wants to see stamina: defeat 100 enemies.",
        obj_kind: objective_kind::KILL, obj_target: 0, obj_count: 100,
        reward_cobre: 2_500, reward_xp: 70_000, reward_item: item_id::XP_POTION, reward_item_qty: 2,
        reward_item2: item_id::GLITTERING_POWDER, reward_item2_qty: 3,
        requires: 540, min_level: 14, ..de(crate::construcao::Papel::Treinador) },
    // A caçada TEMÁTICA: mesma ideia, alvo fechado, pra quem quer o drop
    // daquele bicho. Abre em paralelo — nao depende da cadeia acima.
    QuestDef { id: 542, title: "Bear leather",
        desc: "The Tailor pays well for leather in bulk. Defeat 40 bears.",
        obj_kind: objective_kind::KILL, obj_target: alvo_de_mob(mob_kind::URSO), obj_count: 40,
        reward_cobre: 1_400, reward_xp: 34_000, reward_item: item_id::HIDE, reward_item_qty: 4,
        min_level: 12, ..de(crate::construcao::Papel::Alfaiate) },
    QuestDef { id: 543, title: "Wolf fangs",
        desc: "The wolves have cost this island dearly. Defeat 50 of them.",
        obj_kind: objective_kind::KILL, obj_target: alvo_de_mob(mob_kind::LOBO), obj_count: 50,
        reward_cobre: 1_600, reward_xp: 38_000, reward_item: item_id::QUINTESSENCE, reward_item_qty: 6,
        min_level: 12, ..de(crate::construcao::Papel::Treinador) },

    // As 15 receitas cinzas pedem 4 Escamas, 4 Garras, 4 Chifres e 3 Couros.
    // IDs novos deixam a cadeia disponivel a quem concluiu as antigas.
    QuestDef { id: 544, title: "The first scale",
        desc: "The Blacksmith set aside four Grey Scales: one for each weapon in the workshop. Defeat 15 beasts to earn them.",
        obj_kind: objective_kind::KILL, obj_target: 0, obj_count: 15,
        reward_cobre: 80, reward_xp: 200, reward_item: item_id::SCALE, reward_item_qty: 4,
        min_level: 1, ..de(crate::construcao::Papel::Ferreiro) },
    QuestDef { id: 545, title: "A claw for the off-hand",
        desc: "The Trainer keeps four Grey Claws for off-hands. Defeat 6 wolves on the trail.",
        obj_kind: objective_kind::KILL, obj_target: alvo_de_mob(mob_kind::LOBO), obj_count: 6,
        reward_cobre: 100, reward_xp: 350, reward_item: item_id::CLAW, reward_item_qty: 4,
        requires: 544, min_level: 2, ..de(crate::construcao::Papel::Treinador) },
    QuestDef { id: 546, title: "The apprentice's horn",
        desc: "The Appraiser offers four Grey Horns for accessories. Break 8 rocks and bring back whatever you find.",
        obj_kind: objective_kind::GATHER, obj_target: alvo_de_coleta::PEDRA, obj_count: 8,
        reward_cobre: 120, reward_xp: 500, reward_item: item_id::HORN, reward_item_qty: 4,
        requires: 545, min_level: 3, ..de(crate::construcao::Papel::Identificador) },
    QuestDef { id: 547, title: "Leather for the armour",
        desc: "The Tailor set aside three Grey Leathers, one for each armour. Drive 3 bears off the village road.",
        obj_kind: objective_kind::KILL, obj_target: alvo_de_mob(mob_kind::URSO), obj_count: 3,
        reward_cobre: 160, reward_xp: 700, reward_item: item_id::HIDE, reward_item_qty: 3,
        requires: 546, min_level: 4, ..de(crate::construcao::Papel::Alfaiate) },

    // F) A vila: quem e' quem.
    QuestDef { id: 534, title: "News from the tavern",
        desc: "The one who hears everything on this island is the Innkeeper. Stop by and listen.",
        obj_kind: objective_kind::TALK, obj_target: crate::construcao::Papel::Taberna as u16, obj_count: 1,
        reward_cobre: 60, reward_xp: 200, reward_item: item_id::HEALTH_POTION, reward_item_qty: 2,
        ..de(crate::construcao::Papel::Missoes) },
    QuestDef { id: 535, title: "The Cartographer's map",
        desc: "The Cartographer at the port draws the island's trails. Talk to him.",
        obj_kind: objective_kind::TALK, obj_target: crate::construcao::Papel::Cartografo as u16, obj_count: 1,
        reward_cobre: 100, reward_xp: 600, reward_item: item_id::HEALTH_POTION, reward_item_qty: 3,
        requires: 534, min_level: 3, ..de(crate::construcao::Papel::Taberna) },
    QuestDef { id: 536, title: "Cargo on the quay",
        desc: "The Banker knows what comes in and out of the port — money tells everything. Ask him about Morgan's ships.",
        obj_kind: objective_kind::TALK, obj_target: crate::construcao::Papel::Deposito as u16, obj_count: 1,
        reward_cobre: 150, reward_xp: 1_200, reward_item: item_id::MANA_POTION, reward_item_qty: 3,
        requires: 535, min_level: 5, ..de(crate::construcao::Papel::Cartografo) },
    QuestDef { id: 537, title: "What the stone hides",
        desc: "The Appraiser reads what the lighthouse stones carry. Take him what you heard on the quay.",
        obj_kind: objective_kind::TALK, obj_target: crate::construcao::Papel::Identificador as u16, obj_count: 1,
        reward_cobre: 250, reward_xp: 3_000, reward_item: item_id::GREATER_HEAL, reward_item_qty: 2,
        requires: 536, min_level: 8, ..de(crate::construcao::Papel::Deposito) },
    QuestDef { id: 538, title: "Storm line",
        desc: "The Tailor weaves with thread that holds the wind. Ask whether he'll sew for you.",
        obj_kind: objective_kind::TALK, obj_target: crate::construcao::Papel::Alfaiate as u16, obj_count: 1,
        reward_cobre: 400, reward_xp: 6_000, reward_item: item_id::GREATER_MANA, reward_item_qty: 3,
        requires: 537, min_level: 10, ..de(crate::construcao::Papel::Identificador) },

    // ===================== DIARIAS do Mestre (6xx) — uma serie por ilha =====================
    // Mesmas sete tarefas em cada ilha, na escala da faixa dela (nivel de
    // entrada da ilha, contagem e paga crescendo): quebrar pedras e cacar
    // (area: +1 Pocao de Experiencia), criar e refinar (painel pelo menu), e
    // as tres que dependem de sistema que ainda nao existe — com cadeado.
    // --- Bosque (ilha_inicial, 1-15) ---
    diaria(601, "Quarry of the day", "Break 20 rocks at any vein on the island.", objective_kind::GATHER, alvo_de_coleta::PEDRA, 20, 150, 150, item_id::HEALTH_POTION, 3, 1, true, false),
    diaria(602, "Hunt of the day", "Defeat 30 beasts on the island.", objective_kind::KILL, 0, 30, 180, 200, item_id::HEALTH_POTION, 3, 1, true, false),
    diaria(603, "To work", "Create 1 piece of gear in Craft (the HUD button).", objective_kind::CRAFT, 0, 1, 120, 120, item_id::MANA_POTION, 2, 1, false, false),
    diaria(604, "Hot forge", "Try refining a piece once at the Forge (the HUD button or the Blacksmith).", objective_kind::REFINE, 0, 1, 120, 120, item_id::MANA_POTION, 2, 1, false, false),
    diaria(605, "Enchant of the day", "Enchant a piece.", objective_kind::ENCHANT, 0, 1, 150, 150, 0, 0, 1, false, true),
    diaria(606, "Cellar of the day", "Complete a dungeon (Cellar or Cavern).", objective_kind::DUNGEON, 0, 1, 250, 250, 0, 0, 10, false, false),
    diaria(607, "Chase of the day", "Defeat the boss of the Hunt.", objective_kind::RAID, 0, 1, 400, 400, 0, 0, 10, false, true),
    diaria(608, "Boss of the day", "Defeat a field boss on the island (the map shows where).", objective_kind::KILL, ALVO_QUALQUER_CHEFE, 1, 500, 3_000, item_id::GREATER_HEAL, 3, 8, true, false),
    // --- Geleira (ilha_gelo, 15-30) ---
    // Duas chaves verdes de uso unico: arma e armadura T2, liberadas no
    // nivel 20 como as receitas verdes. Materiais continuam vindo da coleta.
    QuestDef { id: 610, title: "Glacier scale",
        desc: "The Master keeps a Green Scale for your first T2 weapon. Defeat 20 enemies on the Glacier.",
        obj_kind: objective_kind::KILL, obj_target: 0, obj_count: 20,
        reward_cobre: 1_000, reward_xp: 8_000,
        reward_item: item_id::na_cor(item_id::SCALE, 2), reward_item_qty: 1,
        min_level: 20, ..mestre() },
    diaria(611, "Quarry of the day", "Break 25 rocks at any vein on the island.", objective_kind::GATHER, alvo_de_coleta::PEDRA, 25, 450, 600, item_id::GREATER_HEAL, 3, 15, true, false),
    diaria(612, "Hunt of the day", "Defeat 40 beasts on the island.", objective_kind::KILL, 0, 40, 540, 800, item_id::GREATER_HEAL, 3, 15, true, false),
    diaria(613, "To work", "Create 1 piece of gear in Craft (the HUD button).", objective_kind::CRAFT, 0, 1, 360, 480, item_id::GREATER_MANA, 2, 15, false, false),
    diaria(614, "Hot forge", "Try refining a piece once at the Forge (the HUD button or the Blacksmith).", objective_kind::REFINE, 0, 1, 360, 480, item_id::GREATER_MANA, 2, 15, false, false),
    diaria(615, "Enchant of the day", "Enchant a piece.", objective_kind::ENCHANT, 0, 1, 450, 600, 0, 0, 15, false, true),
    diaria(616, "Cellar of the day", "Complete a dungeon (Cellar or Cavern).", objective_kind::DUNGEON, 0, 1, 750, 1_000, 0, 0, 15, false, false),
    diaria(617, "Chase of the day", "Defeat the boss of the Hunt.", objective_kind::RAID, 0, 1, 1_200, 1_600, 0, 0, 15, false, true),
    diaria(618, "Boss of the day", "Defeat a field boss on the island (the map shows where).", objective_kind::KILL, ALVO_QUALQUER_CHEFE, 1, 1_500, 12_000, item_id::GREATER_HEAL, 4, 20, true, false),
    QuestDef { id: 619, title: "Glacier leather",
        desc: "The Master keeps a Green Leather for your first T2 armour. Break 20 rocks on the Glacier.",
        obj_kind: objective_kind::GATHER, obj_target: alvo_de_coleta::PEDRA, obj_count: 20,
        reward_cobre: 1_000, reward_xp: 8_000,
        reward_item: item_id::na_cor(item_id::HIDE, 2), reward_item_qty: 1,
        requires: 610, min_level: 20, ..mestre() },

    // Cinco trilhas unicas da Geleira. Cada uma sai de uma cabana no interior
    // ou na costa: explorar a ilha deixa de ser uma ida e volta ao porto.
    QuestDef { id: 810, title: "The shelter on the coast", desc: "The Walrus Watch needs help keeping the shelter. Bring down five walruses on the beach.",
        obj_kind: objective_kind::KILL, obj_target: alvo_de_mob(10), obj_count: 5,
        reward_cobre: 650, reward_xp: 4_000, reward_item: item_id::GREATER_HEAL, reward_item_qty: 3,
        min_level: 15, ..posto(204) },
    QuestDef { id: 811, title: "Stones under the snow", desc: "Break ten veins near the coast to reinforce the shelter.",
        obj_kind: objective_kind::GATHER, obj_target: alvo_de_coleta::PEDRA, obj_count: 10,
        reward_cobre: 700, reward_xp: 4_500, reward_item: item_id::STEEL, reward_item_qty: 20,
        requires: 810, min_level: 16, ..posto(204) },
    QuestDef { id: 812, title: "White trail", desc: "The white tigers have come close to the shelter. Drive eight away.",
        obj_kind: objective_kind::KILL, obj_target: alvo_de_mob(12), obj_count: 8,
        reward_cobre: 800, reward_xp: 5_500, reward_item: item_id::GREATER_MANA, reward_item_qty: 3,
        requires: 811, min_level: 17, ..posto(204) },
    QuestDef { id: 813, title: "Firewood for the watch", desc: "The fire on the coast cannot go out. Fell twelve trees on the Glacier.",
        obj_kind: objective_kind::GATHER, obj_target: alvo_de_coleta::ARVORE, obj_count: 12,
        reward_cobre: 950, reward_xp: 6_000, reward_item: item_id::XP_POTION, reward_item_qty: 1,
        requires: 812, min_level: 18, ..posto(204) },
    QuestDef { id: 814, title: "A guarded coast", desc: "Make one last patrol and defeat twenty enemies on the Glacier.",
        obj_kind: objective_kind::KILL, obj_target: 0, obj_count: 20,
        reward_cobre: 1_200, reward_xp: 8_000, reward_item: item_id::QUINTESSENCE, reward_item_qty: 10,
        requires: 813, min_level: 19, ..posto(204) },

    QuestDef { id: 815, title: "Forest trail", desc: "The Guardian of the Forest found tiger tracks. Defeat eight white tigers.",
        obj_kind: objective_kind::KILL, obj_target: alvo_de_mob(12), obj_count: 8,
        reward_cobre: 750, reward_xp: 5_000, reward_item: item_id::GREATER_HEAL, reward_item_qty: 3,
        min_level: 16, ..posto(205) },
    QuestDef { id: 816, title: "Eyes on the woods", desc: "The owlbears prowl the frozen trees too. Defeat six.",
        obj_kind: objective_kind::KILL, obj_target: alvo_de_mob(5), obj_count: 6,
        reward_cobre: 850, reward_xp: 6_000, reward_item: item_id::EXORCISM_BAUBLE, reward_item_qty: 6,
        requires: 815, min_level: 18, ..posto(205) },
    QuestDef { id: 817, title: "Frozen branches", desc: "Fell fifteen trees to open a safe trail.",
        obj_kind: objective_kind::GATHER, obj_target: alvo_de_coleta::ARVORE, obj_count: 15,
        reward_cobre: 1_000, reward_xp: 7_000, reward_item: item_id::STEEL, reward_item_qty: 30,
        requires: 816, min_level: 19, ..posto(205) },
    QuestDef { id: 818, title: "Winter predators", desc: "Drive twelve white bears away from the Guardian's cabin.",
        obj_kind: objective_kind::KILL, obj_target: alvo_de_mob(11), obj_count: 12,
        reward_cobre: 1_200, reward_xp: 9_000, reward_item: item_id::GREATER_HEAL, reward_item_qty: 4,
        requires: 817, min_level: 21, ..posto(205) },
    QuestDef { id: 819, title: "The forest breathes", desc: "Defeat twenty-five creatures on the Glacier before the trail closes again.",
        obj_kind: objective_kind::KILL, obj_target: 0, obj_count: 25,
        reward_cobre: 1_500, reward_xp: 12_000, reward_item: item_id::XP_POTION, reward_item_qty: 1,
        requires: 818, min_level: 23, ..posto(205) },

    QuestDef { id: 820, title: "Metal for the shelter", desc: "The Bear Scout needs steel to reinforce the cabin. Break 20 rocks.",
        obj_kind: objective_kind::GATHER, obj_target: alvo_de_coleta::PEDRA, obj_count: 20,
        reward_cobre: 1_000, reward_xp: 7_000, reward_item: item_id::DARKSTEEL, reward_item_qty: 150,
        min_level: 18, ..posto(206) },
    QuestDef { id: 821, title: "Protective leather", desc: "Bring down ten white bears surrounding the outpost.",
        obj_kind: objective_kind::KILL, obj_target: alvo_de_mob(11), obj_count: 10,
        reward_cobre: 1_100, reward_xp: 8_000, reward_item: item_id::QUINTESSENCE, reward_item_qty: 12,
        requires: 820, min_level: 19, ..posto(206) },
    QuestDef { id: 822, title: "The first T2 mould", desc: "Create a piece of gear in Craft to prepare the outpost's defence.",
        obj_kind: objective_kind::CRAFT, obj_target: 0, obj_count: 1,
        reward_cobre: 1_500, reward_xp: 10_000, reward_item: item_id::na_cor(item_id::CLAW, 2), reward_item_qty: 1,
        requires: 821, min_level: 20, ..posto(206) },
    QuestDef { id: 823, title: "Tempered steel", desc: "Try refining a piece three times at the Forge.",
        obj_kind: objective_kind::REFINE, obj_target: 0, obj_count: 3,
        reward_cobre: 1_800, reward_xp: 12_000, reward_item: item_id::GLITTERING_POWDER, reward_item_qty: 2,
        requires: 822, min_level: 22, ..posto(206) },
    QuestDef { id: 824, title: "Marks in the stone", desc: "Break twenty rocks to keep the metal supply going.",
        obj_kind: objective_kind::GATHER, obj_target: alvo_de_coleta::PEDRA, obj_count: 20,
        reward_cobre: 2_000, reward_xp: 14_000, reward_item: item_id::na_cor(item_id::HORN, 2), reward_item_qty: 1,
        requires: 823, min_level: 24, ..posto(206) },

    QuestDef { id: 825, title: "Voices on the climb", desc: "The Explorer heard the Mother of Blizzards on the mountain. Defeat ten enemies on the way.",
        obj_kind: objective_kind::KILL, obj_target: 0, obj_count: 10,
        reward_cobre: 1_100, reward_xp: 8_000, reward_item: item_id::GREATER_HEAL, reward_item_qty: 4,
        min_level: 20, ..posto(207) },
    QuestDef { id: 826, title: "The first boss", desc: "Defeat a field boss on the Glacier. Other adventurers nearby count too.",
        obj_kind: objective_kind::KILL, obj_target: ALVO_QUALQUER_CHEFE, obj_count: 1,
        reward_cobre: 1_500, reward_xp: 12_000, reward_item: item_id::GLITTERING_POWDER, reward_item_qty: 1,
        requires: 825, min_level: 22, ..posto(207) },
    QuestDef { id: 827, title: "Three signs in the snow", desc: "Defeat two more field bosses to map their routes.",
        obj_kind: objective_kind::KILL, obj_target: ALVO_QUALQUER_CHEFE, obj_count: 2,
        reward_cobre: 2_000, reward_xp: 16_000, reward_item: item_id::DARKSTEEL, reward_item_qty: 250,
        requires: 826, min_level: 24, ..posto(207) },
    QuestDef { id: 828, title: "The way to the summit", desc: "Drive thirty creatures off the frozen lookout trail.",
        obj_kind: objective_kind::KILL, obj_target: 0, obj_count: 30,
        reward_cobre: 2_300, reward_xp: 18_000, reward_item: item_id::XP_POTION, reward_item_qty: 1,
        requires: 827, min_level: 26, ..posto(207) },
    QuestDef { id: 829, title: "Guardian of the Glacier", desc: "Defeat one last field boss and return to the Explorer's cabin.",
        obj_kind: objective_kind::KILL, obj_target: ALVO_QUALQUER_CHEFE, obj_count: 1,
        reward_cobre: 3_000, reward_xp: 24_000, reward_item: item_id::GLITTERING_POWDER, reward_item_qty: 3,
        requires: 828, min_level: 28, ..posto(207) },

    QuestDef { id: 830, title: "The buried hull", desc: "Complete the Frozen Hull and tell the Watch what you found.",
        obj_kind: objective_kind::DUNGEON, obj_target: 3, obj_count: 1,
        reward_cobre: 1_500, reward_xp: 12_000, reward_item: item_id::GREATER_HEAL, reward_item_qty: 4,
        min_level: 22, ..posto(204) },
    QuestDef { id: 831, title: "Deep ice", desc: "Complete the Deep Ice Caverns with your party.",
        obj_kind: objective_kind::DUNGEON, obj_target: 11, obj_count: 1,
        reward_cobre: 2_000, reward_xp: 16_000, reward_item: item_id::GLITTERING_POWDER, reward_item_qty: 1,
        requires: 830, min_level: 24, ..posto(207) },
    QuestDef { id: 832, title: "Fragments of the hull", desc: "Break twenty rocks to work out why the hull froze.",
        obj_kind: objective_kind::GATHER, obj_target: alvo_de_coleta::PEDRA, obj_count: 20,
        reward_cobre: 2_300, reward_xp: 18_000, reward_item: item_id::DARKSTEEL, reward_item_qty: 250,
        requires: 831, min_level: 27, ..posto(206) },
    QuestDef { id: 833, title: "A secret under the snow", desc: "Complete one more dungeon and take the report to the Explorer.",
        obj_kind: objective_kind::DUNGEON, obj_target: 0, obj_count: 1,
        reward_cobre: 3_000, reward_xp: 25_000, reward_item: item_id::GLITTERING_POWDER, reward_item_qty: 3,
        requires: 832, min_level: 30, ..posto(207) },
    // --- Ermo: tres cabanas fora da cidade, cinco missoes por trilha. ---
    QuestDef { id: 840, title: "Water for the crossing", desc: "The Provisioner of the Waste needs stone to shore up the cistern. Break ten veins.",
        obj_kind: objective_kind::GATHER, obj_target: alvo_de_coleta::PEDRA, obj_count: 10,
        reward_cobre: 950, reward_xp: 7_000, reward_item: item_id::GREATER_HEAL, reward_item_qty: 4,
        min_level: 30, ..posto(208) },
    QuestDef { id: 841, title: "Shells on the trail", desc: "Scarabs surround the supply cabin. Defeat eight.",
        obj_kind: objective_kind::KILL, obj_target: alvo_de_mob(13), obj_count: 8,
        reward_cobre: 1_050, reward_xp: 8_500, reward_item: item_id::STEEL, reward_item_qty: 30,
        requires: 840, min_level: 30, ..posto(208) },
    QuestDef { id: 842, title: "Metal reserve", desc: "Break 15 rocks: the cistern needs steel for its repair.",
        obj_kind: objective_kind::GATHER, obj_target: alvo_de_coleta::PEDRA, obj_count: 15,
        reward_cobre: 1_250, reward_xp: 10_000, reward_item: item_id::DARKSTEEL, reward_item_qty: 120,
        requires: 841, min_level: 31, ..posto(208) },
    QuestDef { id: 843, title: "Water patrol", desc: "Drive twelve enemies off the paths between the cistern and the dunes.",
        obj_kind: objective_kind::KILL, obj_target: 0, obj_count: 12,
        reward_cobre: 1_400, reward_xp: 12_000, reward_item: item_id::GREATER_MANA, reward_item_qty: 5,
        requires: 842, min_level: 32, ..posto(208) },
    QuestDef { id: 844, title: "Supplies secured", desc: "Break fifteen veins to leave a reserve at the cabin.",
        obj_kind: objective_kind::GATHER, obj_target: alvo_de_coleta::PEDRA, obj_count: 15,
        reward_cobre: 1_600, reward_xp: 15_000, reward_item: item_id::na_cor(item_id::HIDE, 2), reward_item_qty: 1,
        requires: 843, min_level: 33, ..posto(208) },
    QuestDef { id: 845, title: "Marks in the sand", desc: "The Researcher of the Dunes studies the scarabs. Defeat ten of them.",
        obj_kind: objective_kind::KILL, obj_target: alvo_de_mob(13), obj_count: 10,
        reward_cobre: 1_200, reward_xp: 11_000, reward_item: item_id::GREATER_HEAL, reward_item_qty: 5,
        min_level: 34, ..posto(209) },
    QuestDef { id: 846, title: "Glass beneath the dunes", desc: "Break twenty rocks and look for the veins the storm turned to glass.",
        obj_kind: objective_kind::GATHER, obj_target: alvo_de_coleta::PEDRA, obj_count: 20,
        reward_cobre: 1_400, reward_xp: 13_000, reward_item: item_id::DARKSTEEL, reward_item_qty: 150,
        requires: 845, min_level: 35, ..posto(209) },
    QuestDef { id: 847, title: "A research tool", desc: "Create a piece of gear for the expedition into the dunes.",
        obj_kind: objective_kind::CRAFT, obj_target: 0, obj_count: 1,
        reward_cobre: 1_600, reward_xp: 15_000, reward_item: item_id::COPPER, reward_item_qty: 1_000,
        requires: 846, min_level: 35, ..posto(209) },
    QuestDef { id: 848, title: "The queen of the dunes", desc: "Drive a Scarab Queen away from the research sites.",
        obj_kind: objective_kind::KILL, obj_target: alvo_de_mob(14), obj_count: 1,
        reward_cobre: 2_000, reward_xp: 19_000, reward_item: item_id::GLITTERING_POWDER, reward_item_qty: 2,
        requires: 847, min_level: 36, ..posto(209) },
    QuestDef { id: 849, title: "Tomb of the Salt Sands", desc: "Complete the Waste dungeon once and tell the Researcher.",
        obj_kind: objective_kind::DUNGEON, obj_target: 12, obj_count: 1,
        reward_cobre: 2_800, reward_xp: 28_000, reward_item: item_id::na_cor(item_id::SCALE, 2), reward_item_qty: 1,
        requires: 848, min_level: 36, ..posto(209) },
    QuestDef { id: 850, title: "Shipwreck watch", desc: "Defeat ten enemies prowling the coast of the Waste.",
        obj_kind: objective_kind::KILL, obj_target: 0, obj_count: 10,
        reward_cobre: 1_500, reward_xp: 14_000, reward_item: item_id::GREATER_HEAL, reward_item_qty: 5,
        min_level: 37, ..posto(210) },
    QuestDef { id: 851, title: "Remains of the flagship", desc: "Break twenty coastal veins to reinforce the Watch's shelter.",
        obj_kind: objective_kind::GATHER, obj_target: alvo_de_coleta::PEDRA, obj_count: 20,
        reward_cobre: 1_700, reward_xp: 16_000, reward_item: item_id::DARKSTEEL, reward_item_qty: 180,
        requires: 850, min_level: 38, ..posto(210) },
    QuestDef { id: 852, title: "Line of defence", desc: "Defeat fifteen enemies before they reach the coastal cabin.",
        obj_kind: objective_kind::KILL, obj_target: 0, obj_count: 15,
        reward_cobre: 1_900, reward_xp: 19_000, reward_item: item_id::GREATER_MANA, reward_item_qty: 5,
        requires: 851, min_level: 39, ..posto(210) },
    QuestDef { id: 853, title: "Watch of the Waste", desc: "Defeat a field boss on the island and return to the Watch.",
        obj_kind: objective_kind::KILL, obj_target: ALVO_QUALQUER_CHEFE, obj_count: 1,
        reward_cobre: 2_600, reward_xp: 26_000, reward_item: item_id::GLITTERING_POWDER, reward_item_qty: 3,
        requires: 852, min_level: 40, ..posto(210) },
    QuestDef { id: 854, title: "Route to the Plateau", desc: "Make a piece of gear for the journey beyond the Waste.",
        obj_kind: objective_kind::CRAFT, obj_target: 0, obj_count: 1,
        reward_cobre: 3_000, reward_xp: 30_000, reward_item: item_id::na_cor(item_id::HIDE, 3), reward_item_qty: 1,
        requires: 853, min_level: 40, ..posto(210) },
    // --- Ermo (ilha_deserto, 28-42) ---
    diaria(621, "Quarry of the day", "Break 30 rocks at any vein on the island.", objective_kind::GATHER, alvo_de_coleta::PEDRA, 30, 750, 1_200, item_id::GREATER_HEAL, 4, 28, true, false),
    diaria(622, "Hunt of the day", "Defeat 50 beasts on the island.", objective_kind::KILL, 0, 50, 900, 1_600, item_id::GREATER_HEAL, 4, 28, true, false),
    diaria(623, "To work", "Create 1 piece of gear in Craft (the HUD button).", objective_kind::CRAFT, 0, 1, 600, 960, item_id::GREATER_MANA, 3, 28, false, false),
    diaria(624, "Hot forge", "Try refining a piece once at the Forge (the HUD button or the Blacksmith).", objective_kind::REFINE, 0, 1, 600, 960, item_id::GREATER_MANA, 3, 28, false, false),
    diaria(625, "Enchant of the day", "Enchant a piece.", objective_kind::ENCHANT, 0, 1, 750, 1_200, 0, 0, 28, false, true),
    diaria(626, "Cellar of the day", "Complete a dungeon (Cellar or Cavern).", objective_kind::DUNGEON, 0, 1, 1_250, 2_000, 0, 0, 28, false, false),
    diaria(627, "Chase of the day", "Defeat the boss of the Hunt.", objective_kind::RAID, 0, 1, 2_000, 3_200, 0, 0, 30, false, true),
    diaria(628, "Boss of the day", "Defeat a field boss on the island (the map shows where).", objective_kind::KILL, ALVO_QUALQUER_CHEFE, 1, 2_500, 20_000, item_id::GREATER_HEAL, 5, 33, true, false),
    // --- Planalto (ilha_planalto, 40-60) ---
    diaria(631, "Quarry of the day", "Break 35 rocks at any vein on the island.", objective_kind::GATHER, alvo_de_coleta::PEDRA, 35, 1_200, 2_400, item_id::GREATER_HEAL, 5, 40, true, false),
    diaria(632, "Hunt of the day", "Defeat 60 beasts on the island.", objective_kind::KILL, 0, 60, 1_440, 3_200, item_id::GREATER_HEAL, 5, 40, true, false),
    diaria(633, "To work", "Create 1 piece of gear in Craft (the HUD button).", objective_kind::CRAFT, 0, 1, 960, 1_920, item_id::GREATER_MANA, 4, 40, false, false),
    diaria(634, "Hot forge", "Try refining a piece once at the Forge (the HUD button or the Blacksmith).", objective_kind::REFINE, 0, 1, 960, 1_920, item_id::GREATER_MANA, 4, 40, false, false),
    diaria(635, "Enchant of the day", "Enchant a piece.", objective_kind::ENCHANT, 0, 1, 1_200, 2_400, 0, 0, 40, false, true),
    diaria(636, "Cellar of the day", "Complete a dungeon (Cellar or Cavern).", objective_kind::DUNGEON, 0, 1, 2_000, 4_000, 0, 0, 40, false, false),
    diaria(637, "Chase of the day", "Defeat the boss of the Hunt.", objective_kind::RAID, 0, 1, 3_200, 6_400, 0, 0, 40, false, true),
    diaria(638, "Boss of the day", "Defeat a field boss on the island (the map shows where).", objective_kind::KILL, ALVO_QUALQUER_CHEFE, 1, 4_000, 40_000, item_id::GREATER_HEAL, 6, 48, true, false),

    QuestDef { id: 860, title: "Contrato dos Sentinelas", desc: "Derrote 30 inimigos do Planalto e volte ao Mestre. Contrato disponível a cada dez minutos.", obj_kind: objective_kind::KILL, obj_target: 0, obj_count: 30, reward_cobre: 1_200, reward_xp: 60_000, reward_item: item_id::GREATER_HEAL, reward_item_qty: 4, min_level: 40, repeatable: true, cooldown_secs: 600, ..mestre() },
    QuestDef { id: 861, title: "Pedra para o farol", desc: "Quebre 20 pedras do Planalto. O campo de tempestade marcado no mapa melhora a coleta quando está ativo.", obj_kind: objective_kind::GATHER, obj_target: alvo_de_coleta::PEDRA, obj_count: 20, reward_cobre: 1_000, reward_xp: 50_000, reward_item: item_id::na_cor(item_id::STEEL, 3), reward_item_qty: 8, min_level: 40, repeatable: true, cooldown_secs: 600, ..mestre() },
    diaria(862, "O sino do Mosteiro", "Vença o Mosteiro dos Ventos.", objective_kind::DUNGEON, 13, 1, 3_000, 180_000, item_id::GREATER_HEAL, 8, 40, false, false),
    diaria(863, "O fogo da Forja", "Vença a Forja do Titã.", objective_kind::DUNGEON, 14, 1, 4_000, 260_000, item_id::GREATER_HEAL, 10, 50, false, false),
    diaria(864, "Lightning in a vault", "Clear the Thunder Vault.", objective_kind::DUNGEON, 5, 1, 3_500, 220_000, item_id::GREATER_HEAL, 8, 50, false, false),
    diaria(639, "Silence the Herald", "Clear the Tempest Spire.", objective_kind::DUNGEON, 18, 1, 4_500, 300_000, item_id::GREATER_HEAL, 10, 56, false, false),
    QuestDef { id: 630, title: "Riders of the Eye", desc: "Defeat 40 beasts on the high Plateau and return to the Master. Contract available every ten minutes.", obj_kind: objective_kind::KILL, obj_target: 0, obj_count: 40, reward_cobre: 1_500, reward_xp: 85_000, reward_item: item_id::GREATER_HEAL, reward_item_qty: 5, min_level: 55, repeatable: true, cooldown_secs: 600, ..mestre() },
    // --- Skyreach (ilha_celeste, 60-80) ---
    diaria(641, "Quarry of the day", "Break 40 rocks on the sky islands.", objective_kind::GATHER, alvo_de_coleta::PEDRA, 40, 1_900, 3_800, item_id::GREATER_HEAL, 6, 60, true, false),
    diaria(642, "Hunt of the day", "Defeat 70 winged beasts on the island.", objective_kind::KILL, 0, 70, 2_300, 5_100, item_id::GREATER_HEAL, 6, 60, true, false),
    diaria(643, "To work", "Create 1 piece of gear in Craft (the HUD button).", objective_kind::CRAFT, 0, 1, 1_540, 3_070, item_id::GREATER_MANA, 5, 60, false, false),
    diaria(644, "Hot forge", "Try refining a piece once at the Forge (the HUD button or the Blacksmith).", objective_kind::REFINE, 0, 1, 1_540, 3_070, item_id::GREATER_MANA, 5, 60, false, false),
    diaria(646, "Cellar of the day", "Complete a dungeon (Cellar or Cavern).", objective_kind::DUNGEON, 0, 1, 3_200, 6_400, 0, 0, 60, false, false),
    diaria(648, "Boss of the day", "Defeat a field boss on the island (the map shows where).", objective_kind::KILL, ALVO_QUALQUER_CHEFE, 1, 6_400, 64_000, item_id::GREATER_HEAL, 8, 70, true, false),
    QuestDef { id: 865, title: "Wardens of the Clouds", desc: "Defeat 40 winged ones on Skyreach and return to the Master. Contract available every ten minutes.", obj_kind: objective_kind::KILL, obj_target: 0, obj_count: 40, reward_cobre: 1_900, reward_xp: 110_000, reward_item: item_id::GREATER_HEAL, reward_item_qty: 5, min_level: 60, repeatable: true, cooldown_secs: 600, ..mestre() },
    QuestDef { id: 866, title: "Stone for the bells", desc: "Break 25 rocks on the sky islands. The Bellspire needs new stone.", obj_kind: objective_kind::GATHER, obj_target: alvo_de_coleta::PEDRA, obj_count: 25, reward_cobre: 1_600, reward_xp: 90_000, reward_item: item_id::na_cor(item_id::STEEL, 4), reward_item_qty: 8, min_level: 60, repeatable: true, cooldown_secs: 600, ..mestre() },
    diaria(867, "Relics of the Reliquary", "Clear the Seraph Reliquary.", objective_kind::DUNGEON, 6, 1, 5_000, 320_000, item_id::GREATER_HEAL, 10, 65, false, false),
    diaria(868, "The silent choir", "Clear the Cathedral of Clouds.", objective_kind::DUNGEON, 16, 1, 6_000, 420_000, item_id::GREATER_HEAL, 12, 70, false, false),
    diaria(869, "Above the storm", "Clear the Pegasus Aerie.", objective_kind::DUNGEON, 17, 1, 7_000, 540_000, item_id::GREATER_HEAL, 14, 75, false, false),
    // --- Contracts for the first three islands (04/10/2026): 10-40 had no
    // repeatable side work, so the side share there sat at 5-7%.
    QuestDef { id: 912, title: "Woodland patrol", desc: "Defeat 30 beasts in the Bosque and return to the Master. Contract available every ten minutes.", obj_kind: objective_kind::KILL, obj_target: 0, obj_count: 30, reward_cobre: 300, reward_xp: 12_000, reward_item: item_id::HEALTH_POTION, reward_item_qty: 5, min_level: 10, repeatable: true, cooldown_secs: 600, ..mestre() },
    QuestDef { id: 913, title: "Ice patrol", desc: "Defeat 30 beasts on the Glacier and return to the Master. Contract available every ten minutes.", obj_kind: objective_kind::KILL, obj_target: 0, obj_count: 30, reward_cobre: 500, reward_xp: 20_000, reward_item: item_id::GREATER_HEAL, reward_item_qty: 3, min_level: 15, repeatable: true, cooldown_secs: 600, ..mestre() },
    QuestDef { id: 914, title: "Dune patrol", desc: "Defeat 30 beasts in the Waste and return to the Master. Contract available every ten minutes.", obj_kind: objective_kind::KILL, obj_target: 0, obj_count: 30, reward_cobre: 800, reward_xp: 35_000, reward_item: item_id::GREATER_HEAL, reward_item_qty: 4, min_level: 28, repeatable: true, cooldown_secs: 600, ..mestre() },
    // --- Skyreach one-offs (04/10/2026): the island only had contracts and
    // dailies, so 60-80 sat at 8% side XP.
    QuestDef { id: 906, title: "Wolves of the meadow", desc: "Seraph Wolves harry the cloud meadows. Defeat 20.", obj_kind: objective_kind::KILL, obj_target: alvo_de_mob(0), obj_count: 20, reward_cobre: 2_400, reward_xp: 24_000, reward_item: item_id::GREATER_HEAL, reward_item_qty: 6, min_level: 60, ..mestre() },
    QuestDef { id: 907, title: "Lynx on the cliffs", desc: "Seraph Lynxes stalk the marble cliffs. Defeat 15.", obj_kind: objective_kind::KILL, obj_target: alvo_de_mob(3), obj_count: 15, reward_cobre: 2_700, reward_xp: 27_000, reward_item: item_id::GREATER_HEAL, reward_item_qty: 6, requires: 906, min_level: 63, ..mestre() },
    QuestDef { id: 908, title: "Arrows from above", desc: "Seraph Archers shoot at the cloud roads. Defeat 15.", obj_kind: objective_kind::KILL, obj_target: alvo_de_mob(6), obj_count: 15, reward_cobre: 3_000, reward_xp: 30_000, reward_item: item_id::GLITTERING_POWDER, reward_item_qty: 2, requires: 907, min_level: 67, ..mestre() },
    QuestDef { id: 909, title: "The bear of the bells", desc: "Seraph Bears block the way to the Bellspire. Defeat 12.", obj_kind: objective_kind::KILL, obj_target: alvo_de_mob(1), obj_count: 12, reward_cobre: 3_300, reward_xp: 33_000, reward_item: item_id::GREATER_HEAL, reward_item_qty: 8, requires: 908, min_level: 70, ..mestre() },
    QuestDef { id: 910, title: "Spells in the wind", desc: "Seraph Mages bend the storm. Defeat 12.", obj_kind: objective_kind::KILL, obj_target: alvo_de_mob(4), obj_count: 12, reward_cobre: 3_600, reward_xp: 36_000, reward_item: item_id::GLITTERING_POWDER, reward_item_qty: 2, requires: 909, min_level: 74, ..mestre() },
    QuestDef { id: 911, title: "The owlbear of the throne", desc: "Seraph Owlbears nest below the Throne of the Sky. Defeat 6.", obj_kind: objective_kind::KILL, obj_target: alvo_de_mob(5), obj_count: 6, reward_cobre: 4_000, reward_xp: 40_000, reward_item: item_id::GREATER_HEAL, reward_item_qty: 10, requires: 910, min_level: 77, ..mestre() },
    // --- Kōgen-tō (ilha_kogen, 80-100) --- side missions (04/10/2026): the
    // island had none, so 80-100 leveled on the story alone. The XP written
    // here is a WEIGHT; `progressao::xp_da_quest` pays it. Hunt targets are
    // SPECIES (`alvo_de_mob(species)`): the island's variant counts for its
    // species (`kill_conta`), as the story's hunts do.
    diaria(650, "Sentries of the day", "Defeat 25 Laser Sentries on the island.", objective_kind::KILL, alvo_de_mob(6), 25, 2_600, 4_400, item_id::GREATER_HEAL, 7, 80, true, false),
    diaria(651, "Hunt of the day", "Defeat 80 machines on the island.", objective_kind::KILL, 0, 80, 3_100, 5_900, item_id::GREATER_HEAL, 7, 80, true, false),
    diaria(652, "To work", "Create 1 piece of gear in Craft (the HUD button).", objective_kind::CRAFT, 0, 1, 2_100, 3_500, item_id::GREATER_MANA, 6, 80, false, false),
    diaria(653, "Hot forge", "Try refining a piece once at the Forge (the HUD button or the Blacksmith).", objective_kind::REFINE, 0, 1, 2_100, 3_500, item_id::GREATER_MANA, 6, 80, false, false),
    diaria(654, "Cellar of the day", "Complete a dungeon (Cellar or Cavern).", objective_kind::DUNGEON, 0, 1, 4_300, 7_400, 0, 0, 80, false, false),
    diaria(655, "Boss of the day", "Defeat a field boss on the island (the map shows where).", objective_kind::KILL, ALVO_QUALQUER_CHEFE, 1, 8_600, 74_000, item_id::GREATER_HEAL, 9, 85, true, false),
    diaria(656, "Gears of the Foundry", "Clear the Robot Foundry.", objective_kind::DUNGEON, 7, 1, 8_000, 600_000, item_id::GREATER_HEAL, 15, 85, false, false),
    diaria(657, "The last train", "Clear the Undercity Line.", objective_kind::DUNGEON, 8, 1, 9_000, 680_000, item_id::GREATER_HEAL, 16, 90, false, false),
    diaria(658, "Top of the Tower", "Clear the Kōgen Tower.", objective_kind::DUNGEON, 9, 1, 10_000, 760_000, item_id::GREATER_HEAL, 18, 95, false, false),
    QuestDef { id: 659, title: "Scrap patrol", desc: "Defeat 40 machines on Kōgen-tō and return to the Master. Contract available every ten minutes.", obj_kind: objective_kind::KILL, obj_target: 0, obj_count: 40, reward_cobre: 2_400, reward_xp: 130_000, reward_item: item_id::GREATER_HEAL, reward_item_qty: 6, min_level: 80, repeatable: true, cooldown_secs: 600, ..mestre() },
    QuestDef { id: 660, title: "Sentry sweep", desc: "Defeat 20 Laser Sentries on Kōgen-tō and return to the Master. Contract available every ten minutes.", obj_kind: objective_kind::KILL, obj_target: alvo_de_mob(6), obj_count: 20, reward_cobre: 2_000, reward_xp: 105_000, reward_item: item_id::na_cor(item_id::STEEL, 4), reward_item_qty: 10, min_level: 80, repeatable: true, cooldown_secs: 600, ..mestre() },
    QuestDef { id: 661, title: "Hounds on the docks", desc: "Mech Hounds are loose on the Harbor Docks. Put down 20 of them.", obj_kind: objective_kind::KILL, obj_target: alvo_de_mob(0), obj_count: 20, reward_cobre: 4_000, reward_xp: 40_000, reward_item: item_id::GREATER_HEAL, reward_item_qty: 8, min_level: 80, ..mestre() },
    QuestDef { id: 662, title: "Panther sightings", desc: "Volt Panthers stalk the crossing. Defeat 15.", obj_kind: objective_kind::KILL, obj_target: alvo_de_mob(3), obj_count: 15, reward_cobre: 4_400, reward_xp: 44_000, reward_item: item_id::GREATER_HEAL, reward_item_qty: 8, requires: 661, min_level: 84, ..mestre() },
    QuestDef { id: 663, title: "The sentry grid", desc: "Laser Sentries lock down the streets. Destroy 15.", obj_kind: objective_kind::KILL, obj_target: alvo_de_mob(6), obj_count: 15, reward_cobre: 4_800, reward_xp: 48_000, reward_item: item_id::GLITTERING_POWDER, reward_item_qty: 2, requires: 662, min_level: 88, ..mestre() },
    QuestDef { id: 664, title: "Coils in the neon", desc: "Tesla Units overload the Kabukicho lights. Destroy 12.", obj_kind: objective_kind::KILL, obj_target: alvo_de_mob(4), obj_count: 12, reward_cobre: 5_200, reward_xp: 52_000, reward_item: item_id::GREATER_HEAL, reward_item_qty: 10, requires: 663, min_level: 91, ..mestre() },
    QuestDef { id: 665, title: "Iron in the towers", desc: "Iron Bears guard the tower district. Defeat 12.", obj_kind: objective_kind::KILL, obj_target: alvo_de_mob(1), obj_count: 12, reward_cobre: 5_600, reward_xp: 56_000, reward_item: item_id::GLITTERING_POWDER, reward_item_qty: 3, requires: 664, min_level: 94, ..mestre() },
    QuestDef { id: 666, title: "The Dynamo beast", desc: "Dynamo Owlbears feed on the Tocho's power. Defeat 6.", obj_kind: objective_kind::KILL, obj_target: alvo_de_mob(5), obj_count: 6, reward_cobre: 6_000, reward_xp: 60_000, reward_item: item_id::GREATER_HEAL, reward_item_qty: 12, requires: 665, min_level: 97, ..mestre() },
    // --- Abyssia (ilha_abissal, 100-120) --- the same set, the deepest one.
    diaria(670, "Sharks of the day", "Defeat 30 Reef Sharks on the island.", objective_kind::KILL, alvo_de_mob(0), 30, 3_300, 5_200, item_id::GREATER_HEAL, 8, 100, true, false),
    diaria(671, "Hunt of the day", "Defeat 90 sea creatures on the island.", objective_kind::KILL, 0, 90, 3_900, 6_900, item_id::GREATER_HEAL, 8, 100, true, false),
    diaria(672, "To work", "Create 1 piece of gear in Craft (the HUD button).", objective_kind::CRAFT, 0, 1, 2_700, 4_100, item_id::GREATER_MANA, 7, 100, false, false),
    diaria(673, "Hot forge", "Try refining a piece once at the Forge (the HUD button or the Blacksmith).", objective_kind::REFINE, 0, 1, 2_700, 4_100, item_id::GREATER_MANA, 7, 100, false, false),
    diaria(674, "Cellar of the day", "Complete a dungeon (Cellar or Cavern).", objective_kind::DUNGEON, 0, 1, 5_400, 8_600, 0, 0, 100, false, false),
    diaria(675, "Boss of the day", "Defeat a field boss on the island (the map shows where).", objective_kind::KILL, ALVO_QUALQUER_CHEFE, 1, 10_800, 86_000, item_id::GREATER_HEAL, 10, 105, true, false),
    diaria(676, "The drowned hold", "Clear the Sunken Galleon.", objective_kind::DUNGEON, 19, 1, 11_000, 840_000, item_id::GREATER_HEAL, 18, 102, false, false),
    diaria(677, "Vaults of coral", "Clear the Coral Palace Vaults.", objective_kind::DUNGEON, 21, 1, 12_500, 920_000, item_id::GREATER_HEAL, 20, 110, false, false),
    diaria(678, "Into the dark", "Clear The Abyss.", objective_kind::DUNGEON, 22, 1, 14_000, 1_000_000, item_id::GREATER_HEAL, 22, 118, false, false),
    QuestDef { id: 679, title: "Reef watch", desc: "Defeat 40 sea creatures around Abyssia and return to the Master. Contract available every ten minutes.", obj_kind: objective_kind::KILL, obj_target: 0, obj_count: 40, reward_cobre: 3_000, reward_xp: 150_000, reward_item: item_id::GREATER_HEAL, reward_item_qty: 7, min_level: 100, repeatable: true, cooldown_secs: 600, ..mestre() },
    QuestDef { id: 680, title: "Triton sweep", desc: "Defeat 20 Triton Guards around Abyssia and return to the Master. Contract available every ten minutes.", obj_kind: objective_kind::KILL, obj_target: alvo_de_mob(2), obj_count: 20, reward_cobre: 2_500, reward_xp: 120_000, reward_item: item_id::na_cor(item_id::STEEL, 4), reward_item_qty: 12, min_level: 100, repeatable: true, cooldown_secs: 600, ..mestre() },
    QuestDef { id: 681, title: "Teeth in the kelp", desc: "Reef Sharks circle the Kelp Forest. Defeat 20.", obj_kind: objective_kind::KILL, obj_target: alvo_de_mob(0), obj_count: 20, reward_cobre: 5_500, reward_xp: 64_000, reward_item: item_id::GREATER_HEAL, reward_item_qty: 12, min_level: 100, ..mestre() },
    QuestDef { id: 682, title: "Eels in the wrecks", desc: "Tiger Eels nest in the Shipwreck Graveyard. Defeat 15.", obj_kind: objective_kind::KILL, obj_target: alvo_de_mob(3), obj_count: 15, reward_cobre: 6_000, reward_xp: 68_000, reward_item: item_id::GLITTERING_POWDER, reward_item_qty: 3, requires: 681, min_level: 104, ..mestre() },
    QuestDef { id: 683, title: "Claws on the sand", desc: "Giant Crabs block the seafloor path. Defeat 15.", obj_kind: objective_kind::KILL, obj_target: alvo_de_mob(9), obj_count: 15, reward_cobre: 6_500, reward_xp: 72_000, reward_item: item_id::GREATER_HEAL, reward_item_qty: 14, requires: 682, min_level: 108, ..mestre() },
    QuestDef { id: 684, title: "Songs in the trench", desc: "Siren Witches lure divers into the Lantern Trench. Silence 12.", obj_kind: objective_kind::KILL, obj_target: alvo_de_mob(4), obj_count: 12, reward_cobre: 7_000, reward_xp: 76_000, reward_item: item_id::GLITTERING_POWDER, reward_item_qty: 4, requires: 683, min_level: 111, ..mestre() },
    QuestDef { id: 685, title: "Ink in the water", desc: "Coral Octopuses foul the kingdom's water. Defeat 12.", obj_kind: objective_kind::KILL, obj_target: alvo_de_mob(5), obj_count: 12, reward_cobre: 7_500, reward_xp: 80_000, reward_item: item_id::GREATER_HEAL, reward_item_qty: 16, requires: 684, min_level: 114, ..mestre() },
    QuestDef { id: 686, title: "The whale's song", desc: "Abyssal Whales roam the rim of the Abyss. Defeat 3.", obj_kind: objective_kind::KILL, obj_target: alvo_de_mob(1), obj_count: 3, reward_cobre: 8_000, reward_xp: 84_000, reward_item: item_id::GLITTERING_POWDER, reward_item_qty: 5, requires: 685, min_level: 117, ..mestre() },

    // ===================== BOARD (quadro da cidade) — DIÁRIAS =====================
    QuestDef { id: 101, title: "Timberman", desc: "The board asks for wood for the city's works.",
        obj_kind: objective_kind::COLLECT, obj_target: item_id::WOOD_T1, obj_count: 20,
        reward_cobre: 150, reward_xp: 80, repeatable: true, daily: true, ..q() },
    QuestDef { id: 102, title: "Miner", desc: "Deliver ore to the city's foundry.",
        obj_kind: objective_kind::COLLECT, obj_target: item_id::STEEL, obj_count: 15,
        reward_cobre: 160, reward_xp: 90, repeatable: true, daily: true, ..q() },
    QuestDef { id: 103, title: "The Tannery", desc: "The city needs leather for gear.",
        obj_kind: objective_kind::COLLECT, obj_target: item_id::LEATHER_T1, obj_count: 12,
        reward_cobre: 140, reward_xp: 80, repeatable: true, daily: true, ..q() },
    QuestDef { id: 104, title: "Clearing the coast", desc: "Thin out the monsters prowling the city.",
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
        title: "Fine timber", desc: "The fortress needs reinforced tier 3 wood.",
        obj_kind: objective_kind::COLLECT, obj_target: item_id::WOOD_T3, obj_count: 10,
        reward_cobre: 600, reward_xp: 520, reward_item: item_id::GREATER_HEAL, reward_item_qty: 5,
        repeatable: true, daily: true, min_level: 45, ..q() },
    QuestDef { id: 202, source: quest_source::NPC, giver: 102,
        title: "Hunting the raiders", desc: "Veteran raiders surround the region. Wipe them out.",
        obj_kind: objective_kind::KILL, obj_target: 0, obj_count: 22,
        reward_cobre: 680, reward_xp: 600, repeatable: true, daily: true, min_level: 45, ..q() },

    // --- T5 · cidade#1 (lv71-80) — inclui TRANSPORTE ---
    QuestDef { id: 203, source: quest_source::NPC, giver: 103,
        title: "Supplies for the northern fortress",
        desc: "Take 10 tier 3 wood to the northern island (follow the compass).",
        obj_kind: objective_kind::TRANSPORT, obj_target: item_id::WOOD_T3, obj_count: 10,
        obj_x: 11100.0, obj_y: 639.0, obj_radius: 45.0,
        reward_cobre: 1000, reward_xp: 900, reward_item: item_id::GLITTERING_POWDER, reward_item_qty: 2,
        repeatable: true, daily: true, min_level: 65, ..q() },
    QuestDef { id: 204, source: quest_source::NPC, giver: 104,
        title: "A growing threat", desc: "Powerful beasts threaten the island. Thin them out.",
        obj_kind: objective_kind::KILL, obj_target: 0, obj_count: 28,
        reward_cobre: 1100, reward_xp: 980, repeatable: true, daily: true, min_level: 65, ..q() },

    // --- T3 · cidade#2 (lv31-40) ---
    QuestDef { id: 205, source: quest_source::NPC, giver: 105,
        title: "Ore for the forge", desc: "The blacksmith needs tier 2 ore.",
        obj_kind: objective_kind::COLLECT, obj_target: item_id::na_cor(item_id::STEEL, 2), obj_count: 12,
        reward_cobre: 350, reward_xp: 300, reward_item: item_id::DARKSTEEL, reward_item_qty: 2,
        repeatable: true, daily: true, min_level: 25, ..q() },
    QuestDef { id: 206, source: quest_source::NPC, giver: 106,
        title: "Clearing the forest", desc: "Hostile creatures have taken the woods. Clear them out.",
        obj_kind: objective_kind::KILL, obj_target: 0, obj_count: 18,
        reward_cobre: 400, reward_xp: 340, repeatable: true, daily: true, min_level: 25, ..q() },

    // --- T1 · cidade#3 (ilha inicial lv1-5) ---
    QuestDef { id: 207, source: quest_source::NPC, giver: 107,
        title: "Wood for the port", desc: "The village port needs wood for repairs.",
        obj_kind: objective_kind::COLLECT, obj_target: item_id::WOOD_T1, obj_count: 15,
        reward_cobre: 80, reward_xp: 60, repeatable: true, daily: true, min_level: 1, ..q() },
    QuestDef { id: 208, source: quest_source::NPC, giver: 108,
        title: "Beasts on the beach", desc: "Small creatures are bothering the fishermen.",
        obj_kind: objective_kind::KILL, obj_target: 0, obj_count: 8,
        reward_cobre: 100, reward_xp: 70, repeatable: true, daily: true, min_level: 1, ..q() },

    // --- T6 · cidade#4 (ilha endgame lv91-100) — inclui TESOURO ---
    QuestDef { id: 209, source: quest_source::NPC, giver: 109,
        title: "Legendary treasure",
        desc: "A map points to a legendary chest on a distant island. Open it (follow the compass).",
        obj_kind: objective_kind::TREASURE, obj_count: 1,
        obj_x: 6820.0, obj_y: 624.0, obj_radius: 60.0,
        reward_cobre: 1800, reward_xp: 1600, reward_item: item_id::na_cor(item_id::SCALE, 3), reward_item_qty: 1,
        repeatable: true, daily: true, min_level: 88, ..q() },
    QuestDef { id: 210, source: quest_source::NPC, giver: 110,
        title: "Warlords", desc: "The deadliest monsters in the archipelago prowl here.",
        obj_kind: objective_kind::KILL, obj_target: 0, obj_count: 35,
        reward_cobre: 2000, reward_xp: 1750, reward_item: item_id::GLITTERING_POWDER, reward_item_qty: 3,
        repeatable: true, daily: true, min_level: 88, ..q() },

    // --- T2 · cidade#5 (ilha inicial lv1-5) ---
    QuestDef { id: 211, source: quest_source::NPC, giver: 111,
        title: "Leather for the tannery", desc: "The tannery needs fresh leather to get started.",
        obj_kind: objective_kind::COLLECT, obj_target: item_id::LEATHER_T1, obj_count: 12,
        reward_cobre: 110, reward_xp: 90, repeatable: true, daily: true, min_level: 1, ..q() },
    QuestDef { id: 212, source: quest_source::NPC, giver: 112,
        title: "Village rounds", desc: "Keep the outskirts of the village safe.",
        obj_kind: objective_kind::KILL, obj_target: 0, obj_count: 12,
        reward_cobre: 140, reward_xp: 110, repeatable: true, daily: true, min_level: 1, ..q() },

    // ===================== FACÇÃO (PvP; XP + pontos de facção, SEM ouro) ====
    QuestDef { id: 301, source: quest_source::FACTION, faction: faction_id::MORGANEERS,
        giver: faction_id::MORGANEERS as u16,
        title: "Morganeer Dominion", desc: "Defeat Peacemain members in combat.",
        obj_kind: objective_kind::PVP_KILL, obj_count: 3,
        reward_xp: 500, reward_faction_points: 100, repeatable: true, cooldown_secs: 1800,
        min_level: 10, ..q() },
    QuestDef { id: 302, source: quest_source::FACTION, faction: faction_id::PEACEMAIN,
        giver: faction_id::PEACEMAIN as u16,
        title: "Peacemain Defence", desc: "Defeat Morganeers members in combat.",
        obj_kind: objective_kind::PVP_KILL, obj_count: 3,
        reward_xp: 500, reward_faction_points: 100, repeatable: true, cooldown_secs: 1800,
        min_level: 10, ..q() },
    // Reconhecimento (EXPLORE): chegue à sede INIMIGA. Coords = faction spawns do
    // mapa atual (morganeer (600,1350) / peacemain (850,620)). Sabor PvP: vai ao
    // território rival. Completa ao chegar (live-track), entrega na própria sede.
    QuestDef { id: 303, source: quest_source::FACTION, faction: faction_id::MORGANEERS,
        giver: faction_id::MORGANEERS as u16,
        title: "Enemy reconnaissance", desc: "Infiltrate the Peacemain headquarters and come back with information.",
        obj_kind: objective_kind::EXPLORE, obj_count: 1,
        obj_x: 850.0, obj_y: 620.0, obj_radius: 40.0,
        reward_xp: 400, reward_faction_points: 80, repeatable: true, cooldown_secs: 1800,
        min_level: 10, ..q() },
    QuestDef { id: 304, source: quest_source::FACTION, faction: faction_id::PEACEMAIN,
        giver: faction_id::PEACEMAIN as u16,
        title: "Enemy reconnaissance", desc: "Infiltrate the Morganeers headquarters and come back with information.",
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
    QuestDef { id: 900, title: "Jump the Rock",
        desc: "Walk into the rock holding the direction to JUMP over it, then talk to Matteo (get close and press E / tap him).",
        obj_kind: objective_kind::EXPLORE, obj_count: 1,
        obj_x: 195.5, obj_y: 1030.5, obj_radius: 4.0,
        reward_xp: 40, ..q() },
    QuestDef { id: 901, title: "Stay Near the Trees",
        desc: "Gathering is automatic: no tool, no clicking. Just stand near the trees until the first Wood drops.",
        obj_kind: objective_kind::COLLECT, obj_target: item_id::WOOD_T1, obj_count: 1,
        reward_xp: 40, ..q() },
    QuestDef { id: 902, title: "Harvest and Deliver Wood",
        desc: "The more trees around you, the faster it comes. Gather 5 Wood and DELIVER it to Matteo (talk to him).",
        obj_kind: objective_kind::DELIVER, obj_target: item_id::WOOD_T1, obj_count: 5,
        reward_xp: 60, ..q() },
    QuestDef { id: 903, title: "Forge your Weapon",
        desc: "Head to the craft station and forge the T1 weapon of your choice with Matteo's materials.",
        obj_kind: objective_kind::COLLECT, obj_target: 0, obj_count: 1,
        reward_xp: 70, ..q() },
    QuestDef { id: 904, title: "Prove your Worth",
        desc: "Head to the arena (to the east) and defeat 3 enemies with your new weapon. It'll give you the XP to level up!",
        obj_kind: objective_kind::KILL, obj_target: 0, obj_count: 3,
        reward_xp: 120, ..q() },
    QuestDef { id: 905, source: quest_source::HISTORIA, title: "Learn how quests work",
        desc: "Open Menu › Quests. Pick this quest from the list and tap Do. Then, out in the world, use Take to gather several quests before using Go or Do all.",
        obj_kind: objective_kind::TUTORIAL, obj_target: tutorial::MISSAO_MENU, obj_count: 1,
        reward_xp: 40, ..q() },
    // ===================== STORYLINE (4xx) — Lvl 1-10 (Chapter 1) =====================
    QuestDef { id: 401, source: quest_source::NPC, giver: 107,
        title: "The Salt of the Earth", desc: "The Royal Armada has blockaded the port. Talk to the fishermen and help by gathering 15 Wood to repair the barricades.",
        obj_kind: objective_kind::COLLECT, obj_target: item_id::WOOD_T1, obj_count: 15,
        reward_cobre: 200, reward_xp: 100, reward_item: item_id::HEALTH_POTION, reward_item_qty: 5,
        min_level: 1, ..q() },
    QuestDef { id: 402, source: quest_source::NPC, giver: 107,
        title: "The Strength of the Forge", desc: "The blacksmith needs ore to forge defensive weapons. Bring 10 T1 Iron Ore.",
        obj_kind: objective_kind::COLLECT, obj_target: item_id::STEEL, obj_count: 10,
        reward_cobre: 250, reward_xp: 120, reward_item: item_id::MANA_POTION, reward_item_qty: 5,
        min_level: 2, ..q() },
    QuestDef { id: 403, source: quest_source::NPC, giver: 108,
        title: "The Clash on the Quay", desc: "Defeat 10 thugs from the Armada's corrupt garrison who are terrorising the port beach.",
        obj_kind: objective_kind::KILL, obj_target: 0, obj_count: 10,
        reward_cobre: 400, reward_xp: 200, reward_item: item_id::BRACELETE, reward_item_qty: 1,
        min_level: 3, ..q() },
    // ===================== STORYLINE (4xx) — Lvl 10-30 (Chapter 2) =====================
    QuestDef { id: 405, source: quest_source::NPC, giver: 107,
        title: "Following the Wind", desc: "Take 10 T1 Wood to the outpost on the island to the east (follow the compass to the marked area).",
        obj_kind: objective_kind::TRANSPORT, obj_target: item_id::WOOD_T1, obj_count: 10,
        obj_x: 1200.0, obj_y: 800.0, obj_radius: 30.0,
        reward_cobre: 300, reward_xp: 200, min_level: 10, ..q() },
    QuestDef { id: 406, source: quest_source::NPC, giver: 107,
        title: "The Sunken Relic", desc: "Wipe out 12 hostile creatures on the beach to clear the smuggling caves and recover the first Ring of the Abyss.",
        obj_kind: objective_kind::KILL, obj_target: 0, obj_count: 12,
        reward_cobre: 400, reward_xp: 350, reward_item: item_id::BRACELETE, reward_item_qty: 1,
        min_level: 12, ..q() },
    QuestDef { id: 407, source: quest_source::NPC, giver: 108,
        title: "The Coastal Stronghold", desc: "Storm the fort's beach and defeat 15 soldiers of the Royal Armada of the Sun who hold the first map fragment.",
        obj_kind: objective_kind::KILL, obj_target: 0, obj_count: 15,
        reward_cobre: 600, reward_xp: 500, reward_item: item_id::GLITTERING_POWDER, reward_item_qty: 2,
        min_level: 15, ..q() },

    // ===================== STORYLINE (4xx) — Lvl 30-50 (Chapter 3) =====================
    QuestDef { id: 408, source: quest_source::NPC, giver: 105,
        title: "Heat and Ashes", desc: "Gather 15 T2 Iron Ore on the volcano's slopes to forge shields against the heat.",
        obj_kind: objective_kind::COLLECT, obj_target: item_id::na_cor(item_id::STEEL, 2), obj_count: 15,
        reward_cobre: 800, reward_xp: 1500, reward_item: item_id::DARKSTEEL, reward_item_qty: 5,
        min_level: 30, ..q() },
    QuestDef { id: 409, source: quest_source::NPC, giver: 105,
        title: "The Waking of the Earth", desc: "Sabotage the enemy's operations by wiping out 20 volcanic creatures deep in the mines.",
        obj_kind: objective_kind::KILL, obj_target: 0, obj_count: 20,
        reward_cobre: 1000, reward_xp: 2500, min_level: 40, ..q() },
    QuestDef { id: 410, source: quest_source::NPC, giver: 106,
        title: "The Heart of Stone", desc: "Defeat 15 magma guardians and recover the Ring of Black Ignition and the second map fragment.",
        obj_kind: objective_kind::KILL, obj_target: 0, obj_count: 15,
        reward_cobre: 1500, reward_xp: 4000, reward_item: item_id::BRINCO, reward_item_qty: 1,
        min_level: 45, ..q() },

    // ===================== STORYLINE (4xx) — Lvl 50-70 (Chapter 4) =====================
    QuestDef { id: 411, source: quest_source::NPC, giver: 101,
        title: "Crossfire", desc: "Destroy the outer garrison of the Iron Fortress by wiping out 25 Royal Armada soldiers.",
        obj_kind: objective_kind::KILL, obj_target: 0, obj_count: 25,
        reward_cobre: 2000, reward_xp: 8000, min_level: 50, ..q() },
    QuestDef { id: 412, source: quest_source::NPC, giver: 101,
        title: "A Bold Rescue", desc: "Infiltrate the dungeons and reach the cell where the royal historian is held (follow the compass).",
        obj_kind: objective_kind::EXPLORE, obj_count: 1,
        obj_x: 11100.0, obj_y: 639.0, obj_radius: 30.0,
        reward_cobre: 2500, reward_xp: 12000, reward_item: item_id::GLITTERING_POWDER, reward_item_qty: 5,
        min_level: 60, ..q() },
    QuestDef { id: 413, source: quest_source::NPC, giver: 102,
        title: "Escape from the Whirlpools", desc: "Sail through the blockading fleet and carry the wounded historian to the safe island to the north.",
        obj_kind: objective_kind::TRANSPORT, obj_target: item_id::WOOD_T3, obj_count: 5,
        obj_x: 6820.0, obj_y: 624.0, obj_radius: 50.0,
        reward_cobre: 3000, reward_xp: 18000, min_level: 65, ..q() },

    // ===================== STORYLINE (4xx) — Lvl 70-85 (Chapter 5) =====================
    QuestDef { id: 414, source: quest_source::NPC, giver: 103,
        title: "Through the Mists", desc: "Sail the misted sea and gather 20 T4 leathers to prepare sails that resist the damp of the ship graveyard.",
        obj_kind: objective_kind::COLLECT, obj_target: item_id::LEATHER_T4, obj_count: 20,
        reward_cobre: 4000, reward_xp: 30000, min_level: 70, ..q() },
    QuestDef { id: 415, source: quest_source::NPC, giver: 103,
        title: "Exorcism at Sea", desc: "Defeat 30 ghost sailors haunting the eastern mists to cleanse the path.",
        obj_kind: objective_kind::KILL, obj_target: 0, obj_count: 30,
        reward_cobre: 5000, reward_xp: 45000, reward_item: item_id::AMULETO, reward_item_qty: 1,
        min_level: 78, ..q() },
    QuestDef { id: 416, source: quest_source::NPC, giver: 104,
        title: "The Guardian of the Depths", desc: "Face the mutant sea monsters in the deep mists and defeat 15 elite creatures.",
        obj_kind: objective_kind::KILL, obj_target: 0, obj_count: 15,
        reward_cobre: 6000, reward_xp: 60000, reward_item: item_id::na_cor(item_id::SCALE, 3), reward_item_qty: 2,
        min_level: 82, ..q() },

    // ===================== STORYLINE (4xx) — Lvl 85-100 (Chapter 6) =====================
    QuestDef { id: 417, source: quest_source::NPC, giver: 109,
        title: "Breaking the Blockade", desc: "Take on the Royal Armada's elite fleet by defeating 35 of their best fighters in the Central Abyss.",
        obj_kind: objective_kind::KILL, obj_target: 0, obj_count: 35,
        reward_cobre: 8000, reward_xp: 100000, min_level: 85, ..q() },
    QuestDef { id: 418, source: quest_source::NPC, giver: 109,
        title: "Braving the Hurricane", desc: "Reach the summit of the Temple of Tides mountain at the heart of the Maelstrom to consecrate the quest.",
        obj_kind: objective_kind::EXPLORE, obj_count: 1,
        obj_x: 6820.0, obj_y: 624.0, obj_radius: 30.0,
        reward_cobre: 10000, reward_xp: 150000, min_level: 92, ..q() },
    QuestDef { id: 419, source: quest_source::NPC, giver: 110,
        title: "The Heart of the Storm", desc: "Face and wipe out the elite generals of Admiral Vane's final garrison and recover the Heart of the Storm.",
        obj_kind: objective_kind::KILL, obj_target: 0, obj_count: 20,
        reward_cobre: 15000, reward_xp: 250000, reward_item: item_id::CINTO, reward_item_qty: 1,
        min_level: 96, ..q() },
];

/// Cadeia ordenada das quests de tutorial. Concluir uma concede a próxima
/// (auto em TUTORIAL_MODE); concluir a última finaliza o tutorial.
/// Ordem: pular+falar → equipar → colher+entregar → forjar → combater →
/// convocar barco → ir à vela → ir ao leme → navegar.
pub const TUTORIAL_CHAIN: &[u16] = &[900, 901, 902, 903, 904, 905];
/// Primeira quest da cadeia (concedida no spawn do tutorial).
pub fn tutorial_first() -> u16 {
    TUTORIAL_CHAIN[0]
}
/// Próxima quest após `id` na cadeia (None se foi a última).
pub fn tutorial_next(id: u16) -> Option<u16> {
    let i = TUTORIAL_CHAIN.iter().position(|&q| q == id)?;
    TUTORIAL_CHAIN.get(i + 1).copied()
}
/// `id` faz parte da cadeia de tutorial?
pub fn is_tutorial_quest(id: u16) -> bool {
    TUTORIAL_CHAIN.contains(&id)
}

#[cfg(test)]
mod testes_dos_givers {
    use super::*;

    /// O estoque das secundarias novas cobre cada receita cinza uma vez,
    /// mesmo que o personagem ja tenha concluido as cadeias antigas.
    #[test]
    fn chaves_das_secundarias_cobrem_o_catalogo_cinza_e_duas_t2() {
        let receitas = crate::receitas::receitas_de_equipamento();
        for chave in item_id::CHAVES {
            let necessarias = receitas
                .iter()
                .filter(|r| r.tier == 1 && r.inputs[0][0] == chave as u32)
                .count();
            let entregues: usize = QUESTS
                .iter()
                .filter(|q| (544..=547).contains(&q.id) && q.reward_item == chave)
                .map(|q| q.reward_item_qty as usize)
                .sum();
            assert!(
                entregues >= necessarias,
                "chave {chave}: {entregues}/{necessarias}"
            );
        }
        for id in [544, 545, 546, 547] {
            let q = quest_by_id(id).unwrap();
            assert!(!q.repeatable && q.min_level <= 4);
            assert_eq!(zona_da_missao(id), Some("ilha_inicial"));
        }
        for (id, chave) in [
            (610, item_id::na_cor(item_id::SCALE, 2)),
            (619, item_id::na_cor(item_id::HIDE, 2)),
        ] {
            let q = quest_by_id(id).unwrap();
            assert_eq!((q.reward_item, q.reward_item_qty), (chave, 1));
            assert_eq!(q.min_level, 20);
            assert_eq!(zona_da_missao(id), Some("ilha_gelo"));
            assert!(receitas
                .iter()
                .any(|r| r.tier == 2 && r.inputs[0][0] == chave as u32));
        }
    }

    #[test]
    fn cada_npc_da_vila_e_um_giver_que_volta_pro_papel() {
        use crate::construcao::Papel;
        for p in PAPEIS_DE_CONVERSA.iter().chain([Papel::Armas].iter()) {
            assert_eq!(papel_do_giver(giver_do_papel(*p)), Some(*p), "{p:?}");
        }
        assert_eq!(giver_do_papel(Papel::Missoes), GIVER_MESTRE_DA_ILHA);
        assert_eq!(papel_do_giver(99), None, "morador antigo nao e' da vila");
        assert_eq!(
            giver_do_npc(Papel::Armas as u16),
            giver_do_npc(Papel::Armaduras as u16),
            "os dois sao o Armeiro"
        );
        assert_eq!(
            giver_do_npc(Papel::Missoes as u16),
            Some(GIVER_MESTRE_DA_ILHA)
        );
    }

    /// As cadeias 511-538 passam de NPC em NPC: dois passos seguidos quase
    /// nunca com o mesmo (a oficina do Ferreiro repete uma vez). As de cabana
    /// sao a excecao de proposito: cada posto fora da cidade tem a sua cadeia
    /// inteira, e os tres postos do Bosque dao missao.
    #[test]
    fn as_cadeias_do_bosque_trocam_de_npc() {
        let bosque = || QUESTS.iter().filter(|d| (511..=538).contains(&d.id));
        let mut mesmos = 0;
        for d in bosque() {
            assert!(quem_da(d).is_some(), "{} sem NPC", d.id);
            if nome_do_posto(d.giver).is_some() {
                continue;
            }
            if quest_by_id(d.requires).is_some_and(|a| a.giver == d.giver) {
                mesmos += 1;
            }
        }
        assert!(mesmos <= 1, "{mesmos} passos seguidos com o mesmo NPC");
        let postos: std::collections::HashSet<u16> = bosque()
            .map(|d| d.giver)
            .filter(|g| nome_do_posto(*g).is_some())
            .collect();
        assert_eq!(postos.len(), 3, "postos do Bosque com missao: {postos:?}");
        let npcs: std::collections::HashSet<u16> = bosque().map(|d| d.giver).collect();
        assert!(npcs.len() >= 8, "so' {} NPCs dando missao", npcs.len());
    }
}

#[cfg(test)]
mod testes_do_tutorial_travado {
    use super::tutorial as t;

    /// Os tutoriais de SALDO são exatamente os três, e nenhum a mais.
    ///
    /// A distinção não é cosmética: quem tem estado precisa ser fechado pelo
    /// estado, porque o saldo pode já estar gasto quando o passo abre. Quem
    /// não tem é um toque na interface, que se refaz sempre.
    ///
    /// O dono travou a história em 21/09/2026 chegando ao nível 4 com os
    /// pontos JÁ gastos: o passo "coloque um ponto" abriu sem haver ponto
    /// para gastar, e como a história é sequencial, **tudo parou** até o
    /// nível 5 dar um ponto novo.
    #[test]
    fn so_os_de_saldo_precisam_de_estado() {
        for a in [t::COLETA_ENERGIA, t::PONTO_ATRIBUTO, t::EVOLUIR_SKILL] {
            assert!(t::tem_estado(a), "{a} mexe em saldo e precisa de estado");
        }
        for a in [
            t::POCAO_LIMIAR,
            t::SKILL_AUTO,
            t::AUTO_COMBATE,
            t::AUTO_COLETA,
            t::MAPA_IR,
        ] {
            assert!(
                !t::tem_estado(a),
                "{a} é toque de interface: esperar o evento sempre funciona"
            );
        }
        // E todo tutorial que existe está classificado de um lado ou do
        // outro — um novo sem classificação cairia no evento por omissão, que
        // é justamente onde este defeito morava.
        for a in t::POCAO_LIMIAR..=t::EVOLUIR_SKILL {
            let _ = t::tem_estado(a);
            assert_ne!(
                t::instrucao(a),
                "Follow the hint",
                "o tutorial {a} não tem instrução — foi acrescentado sem passar por aqui?"
            );
        }
    }
}

#[cfg(test)]
mod testes_do_craft_com_alvo {
    use super::*;

    /// A craft step aimed at an item counts only that item; 0 keeps counting
    /// any piece, which is what every daily and the first armour use.
    #[test]
    fn so_o_item_pedido_conta() {
        let chave = crate::historia::CHAVE_DO_NAUFRAGIO;
        assert!(conta_craft(chave, chave));
        assert!(!conta_craft(chave, chave + 1), "another Cellar's key");
        assert!(!conta_craft(chave, 221), "a piece of armour");
        assert!(conta_craft(0, 221) && conta_craft(0, chave));
    }
}
