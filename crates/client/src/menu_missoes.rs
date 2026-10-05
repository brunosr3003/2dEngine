//! Menu de TODAS as missoes (rodape do rastreador ou Menu): cada uma com o estado pro jogador —
//! disponivel, em andamento, pronta, concluida ou bloqueada — e o "Ir".
//!
//! O estado e' calculado AQUI com o `QuestLog`, o `QuestEstado` (entregues e
//! faccao) e o nivel; quem valida aceitar e entregar continua sendo o servidor.
//! Bloqueada mostra o cadeado e, no hover, os pre-requisitos exatos.
use macroquad::prelude::*;
use shared::historia;
use shared::quests::{faction_id, zona_da_missao, QuestDef, QuestNet, QUESTS};
use std::collections::HashMap;

use crate::hud_estilo as estilo;
use crate::missoes::{progresso, pronta, Tem};

const LARGURA: f32 = 850.0;

#[derive(Debug, Clone, PartialEq)]
pub enum Estado {
    Disponivel,
    EmAndamento {
        feito: u32,
        total: u32,
    },
    Pronta,
    Concluida,
    /// Os motivos, ja' em texto ("Requer nível 3").
    Bloqueada(Vec<String>),
}

/// O que o clique numa missao pede ao `main`.
#[derive(Debug, Clone, PartialEq)]
pub enum Clique {
    /// Disponivel aqui: ACEITAR na hora, sem andar ate' o NPC.
    ///
    /// Era `IrAoGiver`, que mandava o jogador caminhar ate' o balcao e abrir
    /// a oferta. O dono: "e de[ixa] pra pegar direto no menu de missoes igual
    /// no MIR4". A caminhada nao guardava regra nenhuma — quem vai fazer a
    /// missao anda ate' o OBJETIVO de qualquer jeito, e o trecho ate' o
    /// balcao era pedagio. O servidor continua exigindo que o NPC exista
    /// nesta ilha.
    Aceitar(u16),
    /// Ultimo passo do tutorial inicial: executar pelo menu.
    FazerTutorial(u16),
    /// Em andamento ou pronta: a auto missao que ja' existe.
    AutoMissao(u16),
    /// Abre a tela da tarefa manual, sem entrar na fila.
    AbrirManual(u16),
    /// Nao anda: so' avisa.
    Aviso(String),
    /// A FILA: fazer estas, nesta ordem.
    ///
    /// Pedido pelo dono: "ter como colocar para fazer as missoes em
    /// sequencia, no maximo 10, escolher quais". A ordem e' a de marcacao —
    /// quem marcou por ultimo vai por ultimo —, e missao que nao da' pra
    /// fazer na hora e' PULADA, nao trava a fila.
    Fila(Vec<u16>),
    /// Trava de nível: apresenta as rotas de XP disponíveis nesta ilha.
    OpcoesDeNivel(u32),
}

/// O que o calculo precisa saber do jogador.
pub struct Contexto<'a> {
    pub log: &'a [QuestNet],
    /// Entregues: id -> fim do cooldown (unix, 0 = sem).
    pub entregues: &'a HashMap<u16, i64>,
    pub nivel: u32,
    pub faccao: u8,
    /// Zona (ilha) em que o jogador esta'.
    pub zona: Option<&'a str>,
    pub agora_unix: i64,
    pub tem: Tem<'a>,
    /// Nomes de item, pra escrever a RECOMPENSA por extenso.
    pub nomes: &'a HashMap<u16, String>,
}

/// A recompensa de uma missão, por extenso.
///
/// O menu mostrava o que a missão PEDE e nunca o que ela PAGA — o dono:
/// "quero que no menu de missões esteja mais relatado o que a missão dá de
/// recompensa". Decidir qual fazer primeiro sem saber o que cada uma paga é
/// escolher no escuro.
///
/// Irmã de `missoes::recompensa`, que faz o mesmo a partir do `QuestNet` (a
/// missão ATIVA). Aqui a fonte é o `QuestDef`, que é o que o menu tem em mão
/// pra missão que ainda não foi aceita — e é justamente nessa que saber o
/// prêmio muda a decisão.
pub fn recompensa_de(d: &QuestDef, nomes: &HashMap<u16, String>) -> String {
    let nome = |id: u16| {
        nomes
            .get(&id)
            .cloned()
            .unwrap_or_else(|| format!("item {id}"))
    };
    let mut partes = Vec::new();
    if d.reward_item != 0 && d.reward_item_qty > 0 {
        partes.push(format!("{}x {}", d.reward_item_qty, nome(d.reward_item)));
    }
    if d.reward_item2 != 0 && d.reward_item2_qty > 0 {
        partes.push(format!("{}x {}", d.reward_item2_qty, nome(d.reward_item2)));
    }
    // O cartão tem uma só linha e corta o final. Os itens precisam aparecer
    // antes de XP/cobre para não esconder recompensas como o pergaminho de pet.
    if shared::progressao::xp_da_quest(&d) > 0 {
        partes.push(format!("{} XP", shared::progressao::xp_da_quest(&d)));
    }
    if d.reward_cobre > 0 {
        partes.push(format!("{} {}", d.reward_cobre, shared::idioma::cobre()));
    }
    if d.reward_faction_points > 0 {
        partes.push(format!("{} faction pts", d.reward_faction_points));
    }
    partes.join("  ·  ")
}

/// The island name when `d` lives on another one than `zona` (the card
/// starts with "On <island> · ").
fn em_outra_ilha(d: &QuestDef, zona: Option<&str>) -> Option<String> {
    zona_da_missao(d.id).filter(|z| Some(*z) != zona).map(nome_da_zona)
}

fn nome_da_zona(z: &str) -> String {
    shared::terreno::def_da_zona(z).map_or_else(|| z.to_string(), |d| d.nome.to_string())
}

fn nome_da_faccao(f: u8) -> &'static str {
    match f {
        faction_id::MORGANEERS => "Morganeers",
        faction_id::PEACEMAIN => "Peacemain",
        _ => "?",
    }
}

/// Estado de `d` pro jogador.
pub fn estado(d: &QuestDef, c: &Contexto) -> Estado {
    if let Some(q) = c.log.iter().find(|q| q.id == d.id) {
        if pronta(q, c.tem) {
            return Estado::Pronta;
        }
        let (feito, total) = progresso(q, c.tem);
        return Estado::EmAndamento { feito, total };
    }
    let mut motivos = Vec::new();
    if let Some(&cd) = c.entregues.get(&d.id) {
        if !d.repeatable {
            return Estado::Concluida;
        }
        if cd > c.agora_unix {
            let min = ((cd - c.agora_unix) as f32 / 60.0).ceil() as i64;
            motivos.push(format!("Available again in {min} min"));
        }
    }
    if d.em_breve {
        motivos.push("Coming soon".to_string());
    }
    // ANOTHER ISLAND IS NOT A LOCK. The server takes the quest from any
    // island and the auto quest sails there; the card says where
    // (`em_outra_ilha`). The owner saw every other island's quest under
    // "Locked" and read it as unreachable.
    if zona_da_missao(d.id).is_none() {
        motivos.push("Unavailable in this version".to_string());
    }
    if d.min_level > c.nivel {
        motivos.push(format!("Requires level {}", d.min_level));
    }
    if d.requires != 0 && !c.entregues.contains_key(&d.requires) {
        let t = shared::quests::quest_by_id(d.requires).map_or("?", |r| r.title);
        motivos.push(format!("Complete: {t}"));
    }
    if d.faction != faction_id::NONE && d.faction != c.faccao {
        motivos.push(format!("Only for {}", nome_da_faccao(d.faction)));
    }
    if motivos.is_empty() {
        Estado::Disponivel
    } else {
        Estado::Bloqueada(motivos)
    }
}

/// O que clicar numa missao nesse estado faz.
pub fn clique_de(d: &QuestDef, e: &Estado) -> Clique {
    match e {
        Estado::Disponivel => Clique::Aceitar(d.id),
        Estado::EmAndamento { .. } if d.id == 905 => Clique::FazerTutorial(d.id),
        // Trava de nível: o jogador escolhe uma rota de XP.
        Estado::EmAndamento { .. } if d.obj_kind == shared::quests::objective_kind::NIVEL => {
            Clique::OpcoesDeNivel(d.obj_count)
        }
        Estado::EmAndamento { .. } if tem_atalho_manual(d) => Clique::AbrirManual(d.id),
        Estado::EmAndamento { .. } if !automatizavel(d) => {
            Clique::Aviso(format!("\"{}\" needs you to act.", d.title))
        }
        Estado::EmAndamento { .. } | Estado::Pronta => Clique::AutoMissao(d.id),
        Estado::Concluida => Clique::Aviso(format!("\"{}\" has already been completed.", d.title)),
        Estado::Bloqueada(m) => nivel_bloqueado(m).map_or_else(
            || Clique::Aviso(format!("\"{}\" blocked: {}.", d.title, m.join(" · "))),
            Clique::OpcoesDeNivel,
        ),
    }
}

fn nivel_bloqueado(motivos: &[String]) -> Option<u32> {
    motivos.iter().find_map(|s| s.strip_prefix("Requires level ")?.parse().ok())
}

/// Só objetivos que a máquina e o servidor conseguem executar sem gesto do jogador.
/// A mesma regra governa botão, fila e avanço automático da história.
pub fn automatizavel(d: &QuestDef) -> bool {
    use shared::quests::objective_kind as obj;
    matches!(d.obj_kind, obj::COLLECT | obj::KILL | obj::EXPLORE | obj::DELIVER
        | obj::TRANSPORT | obj::TALK | obj::GATHER | obj::LUGAR | obj::VIAGEM)
        // Este tutorial é coleta no mundo: o servidor indica o cristal.
        || (d.obj_kind == obj::TUTORIAL
            && d.obj_target == shared::quests::tutorial::COLETA_ENERGIA)
}

pub fn tem_atalho_manual(d: &QuestDef) -> bool {
    use shared::quests::objective_kind as o;
    !automatizavel(d)
        && matches!(d.obj_kind, o::CRAFT | o::REFINE | o::DUNGEON | o::TREASURE | o::TUTORIAL)
}

pub fn pode_ir(d: &QuestDef, e: &Estado) -> bool {
    automatizavel(d) || matches!(e, Estado::Pronta)
}

pub fn pode_iniciar_auto(d: &QuestDef, status: u8) -> bool {
    automatizavel(d) || status == shared::quests::quest_status::READY
}

/// Quanto falta pro reset das diarias (04:00 de Brasilia), "5h 07min".
pub fn reset_em(agora_unix: i64) -> String {
    let s = shared::quests::proxima_meia_noite(agora_unix) - agora_unix;
    format!("{}h {:02}min", s / 3600, (s % 3600) / 60)
}

/// Posicao na cadeia: quantas missoes vem antes pelo `requires`.
fn profundidade(d: &QuestDef) -> u32 {
    let mut n = 0;
    let mut atual = d.requires;
    while atual != 0 && n < 64 {
        n += 1;
        atual = shared::quests::quest_by_id(atual).map_or(0, |r| r.requires);
    }
    n
}

/// A primeira missao da cadeia de `d` (ela mesma, se nao pede nenhuma).
fn raiz(d: &QuestDef) -> u16 {
    let mut id = d.id;
    let mut atual = d.requires;
    for _ in 0..64 {
        if atual == 0 {
            break;
        }
        id = atual;
        atual = shared::quests::quest_by_id(atual).map_or(0, |r| r.requires);
    }
    id
}

/// As missoes do menu, por ilha e na ordem da cadeia. As do mapa de tiles
/// antigo (sem giver nas ilhas) ficam FORA: nao ha' como fazer nenhuma. As
/// DIARIAS tambem: tem painel proprio (`diarias`).
pub fn todas() -> Vec<&'static QuestDef> {
    let mut v: Vec<&'static QuestDef> = QUESTS
        .iter()
        .filter(|d| !d.daily && zona_da_missao(d.id).is_some())
        .collect();
    v.sort_by(|a, b| {
        zona_da_missao(a.id)
            .cmp(&zona_da_missao(b.id))
            // Cadeia por cadeia: com varias correndo em paralelo, ordenar so'
            // pela profundidade intercalava todas.
            .then(raiz(a).cmp(&raiz(b)))
            .then(profundidade(a).cmp(&profundidade(b)))
            .then(a.id.cmp(&b.id))
    });
    v
}

/// Indice do passo atual da historia, pelo passo que esta' no log.
pub fn atual_da_historia(log: &[QuestNet]) -> Option<u32> {
    log.iter().find_map(|q| historia::indice(q.id))
}

/// Estado de um passo da historia: concluido (antes do atual), o atual, ou
/// futuro com o motivo — a trava de nivel no caminho, o passo anterior, a ilha.
pub fn estado_da_historia(d: &QuestDef, c: &Contexto) -> Estado {
    if let Some(q) = c.log.iter().find(|q| q.id == d.id) {
        if pronta(q, c.tem) {
            return Estado::Pronta;
        }
        let (feito, total) = progresso(q, c.tem);
        return Estado::EmAndamento { feito, total };
    }
    let i = historia::indice(d.id).unwrap_or(0);
    let atual = atual_da_historia(c.log);
    if atual.is_some_and(|a| i < a) {
        return Estado::Concluida;
    }
    let mut motivos = Vec::new();
    let de = atual.unwrap_or(0);
    for k in de..i {
        let Some(p) = historia::id_do_passo(k).and_then(historia::def_da_historia) else {
            break;
        };
        if p.obj_kind == shared::quests::objective_kind::NIVEL && p.obj_count > c.nivel {
            motivos.push(format!("Requires level {}", p.obj_count));
            break;
        }
    }
    if let Some(p) = i
        .checked_sub(1)
        .and_then(historia::id_do_passo)
        .and_then(historia::def_da_historia)
    {
        motivos.push(format!("Complete: {}", p.title));
    }
    if let Some(z) = zona_da_missao(d.id).filter(|z| Some(*z) != c.zona) {
        motivos.push(format!("On {}", nome_da_zona(z)));
    }
    Estado::Bloqueada(motivos)
}

/// Quantas cronicas do epilogo o menu mostra a partir da atual.
const CRONICAS_NO_MENU: u32 = 2;

/// A lista do menu: a HISTORIA primeiro (os capitulos escritos e as cronicas
/// da atual e da seguinte — o resto nao tem fim), depois as missoes das ilhas.
pub fn lista_do_menu(log: &[QuestNet]) -> Vec<&'static QuestDef> {
    let escritos = historia::total_escritos();
    let atual = atual_da_historia(log).unwrap_or(0);
    let cronica = atual.saturating_sub(escritos) / historia::PASSOS_POR_CRONICA;
    let fim = escritos + (cronica + CRONICAS_NO_MENU) * historia::PASSOS_POR_CRONICA;
    let mut v: Vec<&'static QuestDef> = (0..fim)
        .filter(|i| *i < escritos || *i >= escritos + cronica * historia::PASSOS_POR_CRONICA)
        .filter_map(|i| historia::id_do_passo(i).and_then(historia::def_da_historia))
        .collect();
    v.extend(todas());
    v
}

// ─────────────────────────── linhas (quest lines) ───────────────────────────
//
// O menu mostra LINHAS, nao passos: a historia e cada cadeia de subquests
// viram UMA entrada, com o passo em que o jogador esta' e "passo 2 de 5".
// Listar passo por passo (a historia inteira, as 38 do Bosque) parecia um
// monte de missoes soltas e nao dizia o que dava pra fazer agora (pedido do
// dono em 19/09/2026). E as linhas vao em ABAS pelo estado do passo atual.

/// Uma linha de missoes: a historia, ou uma cadeia (pelo `requires`).
pub struct Linha {
    pub nome: String,
    pub passos: Vec<&'static QuestDef>,
    pub historia: bool,
}

/// Nome de cada cadeia, pela primeira missao dela.
pub fn nome_da_cadeia(raiz: u16) -> Option<&'static str> {
    Some(match raiz {
        501 => "The first days",
        511 => "The bosses of the Grove",
        516 => "Bestiary of the Grove",
        523 => "Workshop",
        528 => "Hands in the soil",
        531 => "Cellars and vaults",
        534 => "Village folk",
        810 => "Walrus Coast",
        815 => "Forest Shelter",
        820 => "Bear Pass",
        825 => "The Ice Climb",
        830 => "Secrets of the Glacier",
        _ => return None,
    })
}

/// As linhas do menu: a historia primeiro, depois as cadeias, cada uma com
/// os passos na ordem.
pub fn linhas(log: &[QuestNet]) -> Vec<Linha> {
    let lista = lista_do_menu(log);
    let mut v = Vec::new();
    if log.iter().any(|q| q.id == 905 && q.status == shared::quests::quest_status::ACTIVE) {
        if let Some(d) = shared::quests::quest_by_id(905) {
            v.push(Linha { nome: "Learn how quests work".into(), passos: vec![d], historia: true });
        }
    }
    let hist: Vec<&'static QuestDef> = lista
        .iter()
        .copied()
        .filter(|d| historia::e_da_historia(d.id))
        .collect();
    if !hist.is_empty() {
        v.push(Linha {
            nome: "Story".into(),
            passos: hist,
            historia: true,
        });
    }
    // `todas` ja' vem por ilha, cadeia e profundidade: cadeia e' contigua.
    let mut raiz_atual = None;
    for d in todas() {
        let r = raiz(d);
        if raiz_atual != Some(r) {
            raiz_atual = Some(r);
            let nome = nome_da_cadeia(r)
                .map(str::to_string)
                .or_else(|| shared::quests::quest_by_id(r).map(|q| q.title.to_string()))
                .unwrap_or_default();
            v.push(Linha {
                nome,
                passos: Vec::new(),
                historia: false,
            });
        }
        if let Some(l) = v.last_mut() {
            l.passos.push(d);
        }
    }
    v
}

/// Onde a linha esta' pro jogador: o passo atual, o estado dele e quantos
/// ja' foram.
pub struct Resumo {
    /// `None` = linha inteira concluida.
    pub atual: Option<&'static QuestDef>,
    pub estado: Estado,
    pub feitos: usize,
    pub total: usize,
    /// Estado de cada passo, na ordem (pra lista aberta).
    pub estados: Vec<Estado>,
}

pub fn resumo(l: &Linha, c: &Contexto) -> Resumo {
    let estados: Vec<Estado> = l
        .passos
        .iter()
        .map(|d| {
            if l.historia {
                estado_da_historia(d, c)
            } else {
                estado(d, c)
            }
        })
        .collect();
    let feitos = estados.iter().filter(|e| **e == Estado::Concluida).count();
    // O passo atual: o que esta' em andamento (ou pronto); senao o que da'
    // pra pegar; senao o primeiro travado. Ramos paralelos (o bestiario abre
    // dois no fim) caem no primeiro.
    let achar = |f: &dyn Fn(&Estado) -> bool| estados.iter().position(f);
    let i = achar(&|e| matches!(e, Estado::EmAndamento { .. } | Estado::Pronta))
        .or_else(|| achar(&|e| *e == Estado::Disponivel))
        .or_else(|| achar(&|e| matches!(e, Estado::Bloqueada(_))));
    Resumo {
        atual: i.map(|i| l.passos[i]),
        estado: i.map_or(Estado::Concluida, |i| estados[i].clone()),
        feitos,
        total: l.passos.len(),
        estados,
    }
}

/// As abas do menu.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Aba {
    EmAndamento,
    Disponiveis,
    Bloqueadas,
    Concluidas,
}

impl Aba {
    pub const TODAS: [Aba; 4] = [
        Aba::EmAndamento,
        Aba::Disponiveis,
        Aba::Bloqueadas,
        Aba::Concluidas,
    ];

    pub fn nome(self) -> &'static str {
        match self {
            Aba::EmAndamento => "In progress",
            Aba::Disponiveis => "Available",
            Aba::Bloqueadas => "Locked",
            Aba::Concluidas => "Completed",
        }
    }
}

/// O SEGUNDO eixo do menu: de que tipo é a linha.
///
/// As abas de cima dizem em que PÉ a missão está (em andamento, disponível,
/// travada, feita). Este diz o que ela É. São perguntas independentes — "o
/// que da história está disponível" precisa das duas —, e por isso são duas
/// faixas e não uma lista de oito botões.
///
/// O dono: "no menu de todas as missões tem que ter subabas de missão de
/// história, missão secundária etc".
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tipo {
    Todos,
    Historia,
    Secundarias,
    Moradores,
    Faccao,
}

impl Default for Tipo {
    fn default() -> Self {
        Tipo::Todos
    }
}

impl Tipo {
    pub const TODOS: [Tipo; 5] = [
        Tipo::Todos,
        Tipo::Historia,
        Tipo::Secundarias,
        Tipo::Moradores,
        Tipo::Faccao,
    ];

    pub fn nome(self) -> &'static str {
        match self {
            Tipo::Todos => "All",
            Tipo::Historia => "Story",
            Tipo::Secundarias => "Side quests",
            Tipo::Moradores => "Residents",
            Tipo::Faccao => "Faction",
        }
    }
}

/// De que tipo é uma linha.
///
/// Pelo PRIMEIRO passo, e não por votação entre os passos: uma cadeia nasce
/// de uma fonte e é ela que dá o nome. Uma cadeia que começa no quadro e
/// segue com um NPC continua sendo secundária.
pub fn tipo_de(l: &Linha) -> Tipo {
    use shared::quests::quest_source as src;
    if l.historia {
        return Tipo::Historia;
    }
    match l.passos.first().map(|d| d.source) {
        Some(src::NPC) => Tipo::Moradores,
        Some(src::FACTION) => Tipo::Faccao,
        _ => Tipo::Secundarias,
    }
}

/// A linha passa pelo filtro de tipo?
pub fn cabe_no_tipo(l: &Linha, t: Tipo) -> bool {
    t == Tipo::Todos
        || tipo_de(l) == t
        || (t == Tipo::Secundarias && tipo_de(l) == Tipo::Moradores)
}

const MAPAS: [Option<&str>; 8] = [
    None,
    Some("ilha_inicial"),
    Some("ilha_gelo"),
    Some("ilha_deserto"),
    Some("ilha_planalto"),
    Some("ilha_celeste"),
    Some("ilha_kogen"),
    Some("ilha_abissal"),
];

fn cabe_no_mapa(l: &Linha, r: &Resumo, mapa: Option<&str>) -> bool {
    let Some(mapa) = mapa else { return true };
    // A história atravessa ilhas: seu mapa é o do passo que aparece no cartão.
    let passo = r.atual.or_else(|| l.passos.last().copied());
    passo.is_some_and(|d| zona_da_missao(d.id) == Some(mapa))
}

/// Em que aba a linha vai, pelo estado do passo atual.
pub fn aba_de(r: &Resumo) -> Aba {
    match r.estado {
        Estado::EmAndamento { .. } | Estado::Pronta => Aba::EmAndamento,
        Estado::Disponivel => Aba::Disponiveis,
        Estado::Bloqueada(_) => Aba::Bloqueadas,
        Estado::Concluida => Aba::Concluidas,
    }
}

/// A frase do estado do passo atual, curta: o que fazer AGORA.
pub fn frase(d: &QuestDef, e: &Estado) -> String {
    match e {
        Estado::Disponivel => match shared::quests::quem_da(d) {
            Some(q) => format!("Available · take it from: {q}"),
            None => "Available".into(),
        },
        Estado::EmAndamento { feito, total }
            if d.obj_kind == shared::quests::objective_kind::TUTORIAL =>
        {
            let o = shared::quests::tutorial::instrucao(d.obj_target);
            if *total > 1 {
                format!("Tutorial · {o}  {}/{total}", feito.min(total))
            } else {
                format!("Tutorial · {o}")
            }
        }
        // A TRAVA DE NIVEL diz o que fazer, e nao so' o quanto falta.
        // "Em andamento · 17/20" e' verdadeiro e nao ajuda ninguem.
        Estado::EmAndamento { feito, total }
            if d.obj_kind == shared::quests::objective_kind::NIVEL =>
        {
            format!(
                "Level {feito}/{total} · pick: Magic Island, side quests or hunting in dense areas"
            )
        }
        Estado::EmAndamento { feito, total } if !automatizavel(d) => {
            format!("Manual · {feito}/{total} · {}", caminho_manual(d))
        }
        Estado::EmAndamento { feito, total } => format!("In progress · {feito}/{total}"),
        Estado::Pronta => match shared::quests::quem_da(d) {
            Some(q) => format!("Ready · turn in to: {q}"),
            None => "Ready to turn in".into(),
        },
        Estado::Concluida => "Completed".into(),
        Estado::Bloqueada(m) => m.join(" · "),
    }
}

fn caminho_manual(d: &QuestDef) -> String {
    use shared::quests::objective_kind as obj;
    match d.obj_kind {
        obj::CRAFT => match shared::porao::nome_da_chave(d.obj_target) {
            Some(chave) => format!("Menu › Craft: create the {chave}"),
            None => format!("Menu › Craft: create {} piece(s) of gear", d.obj_count),
        },
        obj::REFINE => format!("Menu › Forge: try refining {} time(s)", d.obj_count),
        obj::DUNGEON => {
            let onde = if d.obj_target == 0 { "any dungeon".to_string() }
                else { shared::dungeon::conteudo(d.obj_target)
                    .map_or_else(|| "the dungeon named".to_string(), |c| c.nome.to_string()) };
            format!("Menu › Dungeons: conclua {} vez(es) {onde}", d.obj_count)
        }
        obj::TREASURE => "Map: find and open the chest named".into(),
        obj::PVP_KILL => format!("PvP combat: defeat {} rival(s)", d.obj_count),
        obj::TUTORIAL => shared::quests::tutorial::instrucao(d.obj_target).to_string(),
        obj::NIVEL => format!("Reach level {}", d.obj_count),
        obj::RAID => "Complete the raid named".into(),
        obj::ENCHANT => "Enchant the gear named".into(),
        _ => "Follow the quest description".into(),
    }
}

fn quebrar_linhas(texto: &str, largura: f32) -> Vec<String> {
    // Traduz ANTES de quebrar: a quebra entrega PEDACOS ao desenho, e pedaco de
    // frase nao casa com verbete. Traduzindo aqui, a quebra ja' mede e parte o
    // ingles — que e' mais comprido que o portugues e quebra em outro lugar.
    let texto = &shared::idioma::tr(texto);
    let mut linhas = Vec::new();
    let mut linha = String::new();
    for palavra in texto.split_whitespace() {
        let tentativa = if linha.is_empty() { palavra.to_string() } else { format!("{linha} {palavra}") };
        if !linha.is_empty() && estilo::medir(&tentativa, 13) > largura {
            linhas.push(std::mem::take(&mut linha));
            linha.push_str(palavra);
        } else {
            linha = tentativa;
        }
    }
    if !linha.is_empty() { linhas.push(linha); }
    linhas
}

fn detalhes_manuais(d: &QuestDef, largura: f32) -> Vec<String> {
    let mut linhas = quebrar_linhas(&format!("What to do: {}.", caminho_manual(d)), largura);
    linhas.extend(quebrar_linhas(d.desc, largura));
    linhas
}

fn cor_do_estado(e: &Estado) -> Color {
    match e {
        Estado::Disponivel => Color::new(1.0, 0.84, 0.2, 1.0),
        Estado::EmAndamento { .. } => estilo::TEXTO,
        Estado::Pronta => estilo::AUTO,
        Estado::Concluida => estilo::SUAVE,
        Estado::Bloqueada(_) => Color::new(0.85, 0.45, 0.40, 1.0),
    }
}

/// O icone do estado, centrado em `c`.
fn icone(c: Vec2, e: &Estado, s: f32) {
    let cor = cor_do_estado(e);
    match e {
        Estado::Bloqueada(_) => cadeado(c, 9.0 * s, cor),
        Estado::Concluida => {
            draw_line(c.x - 7.0 * s, c.y, c.x - 2.0 * s, c.y + 6.0 * s, 3.0, cor);
            draw_line(
                c.x - 2.0 * s,
                c.y + 6.0 * s,
                c.x + 8.0 * s,
                c.y - 7.0 * s,
                3.0,
                cor,
            );
        }
        Estado::Pronta => estilo::texto_centro(c.x, c.y + 10.0 * s, "?", 26, cor),
        Estado::Disponivel => estilo::texto_centro(c.x, c.y + 10.0 * s, "!", 26, cor),
        Estado::EmAndamento { .. } => draw_circle_lines(c.x, c.y, 8.0 * s, 2.0, cor),
    }
}

// O cartao cresceu pra caber a linha da RECOMPENSA.
const CARTAO: f32 = 96.0;

/// O RODAPÉ da fila, ou `None` quando não há nada marcado.
///
/// Fora do desenho pra ser medido contra a lista: foi desenhando um por cima
/// do outro que o rodapé cobriu o último cartão e comeu o toque dele.
pub(crate) fn rodape_da_fila(p: Rect, f: f32, _tem_marca: bool) -> Option<Rect> {
    // SEMPRE presente, e isso mudou com o "fazer todas".
    //
    // Antes ele só aparecia com alguma marca, porque só servia pra "Fazer as
    // n" e "Clear". Agora ele carrega o "Select all", que é justamente o
    // botão de quem NÃO marcou nada — se o rodapé só nascesse com marca, o
    // atalho pra marcar estaria escondido atrás do trabalho que ele evita.
    Some(Rect::new(
        p.x + 18.0,
        p.y + p.h - 52.0 * f,
        p.w - 36.0,
        40.0 * f,
    ))
}

/// Os três botões do rodapé: (marcar todas, fazer, limpar).
///
/// Fora do desenho pra poder ser medido. Eles dividem a MESMA faixa, e num
/// painel estreito medidas fixas se sobrepõem — que foi o defeito de um
/// cartão deste mesmo menu, achado por teste e não pelo dono.
pub(crate) fn botoes_do_rodape(rod: Rect, f: f32) -> [Rect; 3] {
    // Cada um pede um tamanho; se não couberem, todos encolhem junto.
    let (a, b, c) = (128.0 * f, 112.0 * f, 68.0 * f);
    let folga = 8.0 * f;
    let pedido = a + b + c + folga * 2.0;
    let k = (rod.w / pedido).min(1.0);
    let (a, b, c, folga) = (a * k, b * k, c * k, folga * k);
    let x0 = rod.x + rod.w - (a + b + c + folga * 2.0);
    [
        Rect::new(x0, rod.y, a, rod.h),
        Rect::new(x0 + a + folga, rod.y, b, rod.h),
        Rect::new(x0 + a + folga + b + folga, rod.y, c, rod.h),
    ]
}

/// As caixas de um cartão de missão: (Ir, caixa da fila, largura do texto).
///
/// Fora do desenho pra poder ser MEDIDA. Foi exatamente este tipo de conta —
/// escrita à mão e conferida no olho — que pôs o botão de entrar da Ilha
/// Mágica fora da janela e prendeu o jogador lá.
/// `esquerda` é onde o texto começa; `texto` é quanto ele tem de largura.
///
/// A coluna dos botões ENCOLHE num cartão estreito. Em medidas fixas ela
/// comia o cartão inteiro: com painel de 280 px e escala 2,2 sobravam **−24
/// px** pro nome da missão. Achado por este teste, não pelo dono — que foi o
/// ponto de escrevê-lo.
pub(crate) fn caixas_do_cartao(card: Rect, f: f32) -> (Rect, Rect, f32, f32) {
    // Os botões ficam com um terço do cartão, no máximo o tamanho de sempre.
    let k = (card.w * 0.34 / (82.0 * f)).clamp(0.35, 1.0);
    let ir = Rect::new(
        card.x + card.w - 82.0 * f * k,
        card.y + 12.0 * f,
        72.0 * f * k,
        30.0 * f,
    );
    let fila = Rect::new(
        card.x + card.w - 46.0 * f * k,
        card.y + 48.0 * f,
        34.0 * f * k,
        30.0 * f,
    );
    // A margem do ícone também cede, e o texto nunca fica sem nada.
    let esquerda = (48.0 * f).min(card.w * 0.18);
    let texto = (card.w - esquerda - 90.0 * f * k).max(24.0);
    (ir, fila, esquerda, texto)
}
const PASSO: f32 = 26.0;

#[derive(Default)]
pub struct MenuMissoes {
    pub aberto: bool,
    rolagem: crate::rolagem::Rolagem,
    /// `None` = escolher na abertura (em andamento, ou disponiveis).
    aba: Option<Aba>,
    /// O filtro de tipo. Nasce em `Todos`: quem abre o menu quer ver o que
    /// tem, nao escolher uma gaveta antes de saber o que ha' dentro.
    tipo: Tipo,
    mapa: Option<&'static str>,
    /// Linha aberta (mostra os passos), pelo nome.
    expandida: Option<String>,
    /// As missoes MARCADAS pra fila, na ordem em que foram marcadas.
    ///
    /// Um `Vec` e nao um conjunto porque a ORDEM e' a escolha: marcar e'
    /// dizer "esta depois daquela". Teto de `FILA_MAX`.
    marcadas: Vec<u16>,
    /// A ultima marca que mudou, pra quem quiser reagir: `(id, marcada)`.
    ///
    /// Existe porque marcar pra fila tambem FIXA a missao no rastreador — o
    /// dono: "as quests marcadas tem q ficar fixadas na esquerda, no atalho".
    /// Um evento e nao um espelho do conjunto: espelhar apagaria as fixadas
    /// na mao toda vez que alguem marcasse qualquer coisa.
    marca_mudou: Option<(u16, bool)>,
    /// Marcas soltas de uma vez pelo "Clear". Uma so' variavel de evento nao
    /// daria conta de dez baixas no mesmo quadro.
    desfixar: Vec<u16>,
}

/// Quantas missoes cabem numa fila. O numero e' do dono.
pub const FILA_MAX: usize = 10;

impl MenuMissoes {
    /// Abre pelo rodape do rastreador ou pelo Menu — nunca por tecla.
    pub fn abrir(&mut self) {
        self.aberto = true;
        self.aba = None;
        self.tipo = Tipo::Todos;
        self.mapa = None;
        self.expandida = None;
        self.rolagem.zera();
        // As marcas NAO sobrevivem ao fechar: uma fila montada ontem e
        // iniciada sem querer hoje e' pior que remontar.
        self.marcadas.clear();
    }

    pub fn abrir_secundarias(&mut self) {
        self.abrir();
        self.tipo = Tipo::Secundarias;
    }

    /// Marca ou desmarca uma missao pra fila. Devolve `false` quando a fila
    /// esta' cheia e a marca foi recusada.
    fn alterna_marca(&mut self, id: u16) -> bool {
        if let Some(i) = self.marcadas.iter().position(|x| *x == id) {
            self.marcadas.remove(i);
            self.marca_mudou = Some((id, false));
            return true;
        }
        if self.marcadas.len() >= FILA_MAX {
            return false;
        }
        self.marcadas.push(id);
        self.marca_mudou = Some((id, true));
        true
    }

    /// As marcas soltas em bloco pelo "Clear". Consome.
    pub fn desfixar(&mut self) -> Vec<u16> {
        std::mem::take(&mut self.desfixar)
    }

    /// A marca que mudou desde a ultima pergunta. Consome.
    pub fn marca_mudou(&mut self) -> Option<(u16, bool)> {
        self.marca_mudou.take()
    }

    /// A posicao dela na fila (1-based), se estiver marcada.
    fn posicao_na_fila(&self, id: u16) -> Option<usize> {
        self.marcadas.iter().position(|x| *x == id).map(|i| i + 1)
    }

    pub fn alterna(&mut self) {
        let abrir = !self.aberto;
        if abrir {
            self.abrir();
        } else {
            self.aberto = false;
        }
    }

    fn escala() -> f32 {
        estilo::escala_do_painel(LARGURA, 540.0)
    }

    fn painel() -> Rect {
        estilo::no_painel(Self::escala(), Self::painel_na_escala)
    }

    fn painel_na_escala() -> Rect {
        let f = estilo::fator_texto();
        let w = (LARGURA * f).min(screen_width() - 24.0);
        let h = (screen_height() - 100.0).clamp(240.0, 760.0 * f);
        Rect::new(
            (screen_width() - w) * 0.5,
            (screen_height() - h) * 0.5,
            w,
            h,
        )
    }

    pub fn pega_mouse(&self) -> bool {
        self.aberto && Self::painel().contains(Vec2::from(mouse_position()))
    }

    /// Desenha o menu e devolve o clique do quadro.
    pub fn desenha(&mut self, c: &Contexto) -> Option<Clique> {
        estilo::no_painel(Self::escala(), || self.desenha_na_escala(c))
    }

    fn desenha_na_escala(&mut self, c: &Contexto) -> Option<Clique> {
        if !self.aberto {
            return None;
        }
        // The quest book is READ: a page (`estilo::pergaminho`).
        estilo::pergaminho(Self::painel());
        estilo::no_pergaminho(|| self.desenha_pagina(c))
    }

    fn desenha_pagina(&mut self, c: &Contexto) -> Option<Clique> {
        let f = estilo::fator_texto();
        let p = Self::painel();
        let lateral = (150.0 * f).min(p.w * 0.24);
        let conteudo_x = p.x + lateral + 16.0;
        let conteudo_w = p.x + p.w - conteudo_x - 12.0;
        estilo::ret_arredondado(
            Rect::new(p.x + 6.0, p.y + 42.0 * f, lateral, p.h - 50.0 * f),
            6.0,
            Color::new(0.055, 0.10, 0.16, 0.92),
        );
        estilo::texto(p.x + 18.0, p.y + 32.0 * f, "QUESTS", 22, estilo::OURO);
        if crate::ui::botao(
            Rect::new(p.x + p.w - 44.0 * f, p.y + 10.0, 32.0 * f, 28.0 * f),
            "x",
            true,
        ) {
            self.aberto = false;
            return None;
        }

        // ── as linhas e as abas ──
        let todas: Vec<(Linha, Resumo)> = linhas(c.log)
            .into_iter()
            .map(|l| {
                let r = resumo(&l, c);
                (l, r)
            })
            .collect();
        // A CONTAGEM DAS ABAS RESPEITA O TIPO.
        //
        // Senão "Disponíveis (7)" com o filtro em História abriria uma lista
        // vazia — o número prometeria sete e a tela mostraria zero, que é o
        // jeito mais rápido de o jogador achar que a tela quebrou.
        let tipo = self.tipo;
        let conta = |a: Aba| {
            todas
                .iter()
                .filter(|(l, r)| aba_de(r) == a && cabe_no_tipo(l, tipo) && cabe_no_mapa(l, r, self.mapa))
                .count()
        };
        let aba = *self.aba.get_or_insert(if conta(Aba::EmAndamento) > 0 {
            Aba::EmAndamento
        } else {
            Aba::Disponiveis
        });
        let ya = p.y + 48.0 * f;
        let wa = (conteudo_w - 3.0 * 6.0) / 4.0;
        for (k, a) in Aba::TODAS.iter().enumerate() {
            let r = Rect::new(conteudo_x + k as f32 * (wa + 6.0), ya, wa, 34.0 * f);
            if *a == aba {
                estilo::ret_arredondado(r, 6.0, estilo::alfa(estilo::OURO, 0.22));
            }
            let rot = format!("{} ({})", a.nome(), conta(*a));
            if crate::ui::botao(r, &rot, true) && *a != aba {
                self.aba = Some(*a);
                self.expandida = None;
                self.rolagem.zera();
            }
        }

        // ── a faixa de TIPO, o segundo eixo ──
        let yt = ya + 38.0 * f;
        let wt = (conteudo_w - 4.0 * 5.0) / 5.0;
        for (k, tp) in Tipo::TODOS.iter().enumerate() {
            let r = Rect::new(conteudo_x + k as f32 * (wt + 5.0), yt, wt, 30.0 * f);
            if *tp == self.tipo {
                estilo::ret_arredondado(r, 6.0, estilo::alfa(estilo::ACENTO, 0.28));
            }
            let n = todas.iter().filter(|(l, r)| cabe_no_tipo(l, *tp) && cabe_no_mapa(l, r, self.mapa)).count();
            // Tipo sem nenhuma linha fica APAGADO em vez de sumir: a faixa
            // mudar de tamanho conforme o progresso faria o botão trocar de
            // lugar debaixo do dedo.
            if crate::ui::botao(r, &format!("{} ({n})", tp.nome()), n > 0) && *tp != self.tipo {
                self.tipo = *tp;
                self.expandida = None;
                self.rolagem.zera();
            }
        }

        // Mapa: mantém os filtros de estado e tipo independentes.
        estilo::texto(p.x + 18.0, ya + 4.0 * f, "MAPS", 14, estilo::SUAVE);
        let passo_mapa = ((p.h - 90.0 * f) / MAPAS.len() as f32).clamp(27.0 * f, 43.0 * f);
        for (k, mapa) in MAPAS.iter().enumerate() {
            let r = Rect::new(p.x + 12.0, ya + 22.0 * f + k as f32 * passo_mapa,
                lateral - 12.0, passo_mapa - 4.0 * f);
            if *mapa == self.mapa {
                estilo::ret_arredondado(r, 6.0, estilo::alfa(estilo::OURO, 0.22));
            }
            if crate::ui::botao(r, "", true) && *mapa != self.mapa {
                self.mapa = *mapa;
                self.aba = None;
                self.expandida = None;
                self.rolagem.zera();
            }
            let nome = mapa.map_or("All".to_string(), nome_da_zona);
            // On a wooden button: light, not ink (`sem_pergaminho`).
            estilo::sem_pergaminho(|| {
                estilo::texto_ajustado(&nome, r.x + 8.0, r.y + r.h * 0.68, r.w - 16.0, 15, estilo::TEXTO)
            });
        }

        // ── a lista da aba ──
        //
        // A ALTURA DELA CEDE pro rodapé da fila.
        //
        // Antes a lista ia até o fim do painel e o rodapé era desenhado por
        // cima dela: ele cobria o último cartão, e o toque acertava a missão
        // em vez do botão. O dono: "fazer as 1 e limpar está em cima da
        // última missão, aí não consigo clicar".
        //
        // Reservar o espaço ANTES de medir a lista é o que faz os dois
        // ocuparem lugares diferentes — desenhar um por cima do outro é
        // combinar no desenho o que não foi combinado na conta.
        let rodape = rodape_da_fila(p, f, !self.marcadas.is_empty());
        let topo_lista = yt + 38.0 * f;
        let fim_lista = rodape.map_or(p.y + p.h - 10.0, |r| r.y - 8.0 * f);
        let area = Rect::new(
            conteudo_x,
            topo_lista,
            conteudo_w,
            (fim_lista - topo_lista).max(40.0),
        );
        let mut da_aba: Vec<&(Linha, Resumo)> = todas
            .iter()
            .filter(|(l, r)| aba_de(r) == aba && cabe_no_tipo(l, self.tipo) && cabe_no_mapa(l, r, self.mapa))
            .collect();
        // This island's first: the others are a voyage away.
        da_aba.sort_by_key(|(_, r)| r.atual.is_some_and(|d| em_outra_ilha(d, c.zona).is_some()));
        // O QUE O "MARCAR TODAS" ALCANÇA: o que está na tela e dá pra marcar.
        //
        // Mesma condição da caixinha de cada cartão (`clicavel` + ter passo
        // atual). Sai daqui, e não de dentro do laço de desenho, porque o
        // rodapé é desenhado DEPOIS da lista — e porque uma segunda regra de
        // "pode marcar" escrita noutro lugar sairia do lugar da primeira na
        // primeira mudança.
        let marcaveis: Vec<u16> = da_aba
            .iter()
            .filter(|(_, r)| {
                matches!(
                    r.estado,
                    Estado::Disponivel | Estado::EmAndamento { .. } | Estado::Pronta
                ) && r.atual.is_some_and(automatizavel)
            })
            .filter_map(|(_, r)| r.atual.map(|d| d.id))
            .collect();
        let altura = |l: &Linha| {
            CARTAO * f
                + if self.expandida.as_deref() == Some(l.nome.as_str()) {
                    let detalhe = todas.iter().find(|(outra, _)| outra.nome == l.nome)
                        .and_then(|(_, r)| r.atual)
                        .filter(|d| !automatizavel(d))
                        .map_or(0.0, |d| {
                            let w = (area.w - 62.0 * f).max(80.0);
                            detalhes_manuais(d, w).len() as f32 * 20.0 * f + 12.0 * f
                        });
                    l.passos.len() as f32 * PASSO * f + detalhe + 8.0
                } else {
                    0.0
                }
        };
        let total: f32 = da_aba.iter().map(|(l, _)| altura(l) + 6.0).sum();
        let clique = self.rolagem.quadro(area, total, CARTAO * f);
        let arrastando = self.rolagem.arrastando();
        let tocou = |r: Rect| clique.is_some_and(|q| r.contains(q) && area.contains(q));
        let mouse = Vec2::from(mouse_position());
        let mut saida = None;
        let mut alternar: Option<String> = None;
        // A marca da fila sai do laço junto com a expansão: dentro dele o
        // `self` está emprestado pela closure que mede a altura das linhas.
        let mut marca_pedida: Option<u16> = None;
        if da_aba.is_empty() {
            // Com filtro de tipo ligado, o vazio tem DUAS causas possíveis, e
            // dizer só "nada em andamento" mandaria o jogador procurar o que
            // ele mesmo escondeu um botão acima.
            let vazio: String = if self.tipo != Tipo::Todos {
                format!("No {} in this tab.", self.tipo.nome().to_lowercase())
            } else {
                match aba {
                    Aba::EmAndamento => "Nothing in progress. Check Available.",
                    Aba::Disponiveis => "Nothing to take right now.",
                    Aba::Bloqueadas => "No locked line.",
                    Aba::Concluidas => "No line completed yet.",
                }
                .to_string()
            };
            estilo::texto(area.x + 8.0, area.y + 26.0 * f, &vazio, 15, estilo::SUAVE);
        }
        crate::rolagem::recortar(Some(area));
        let mut y = area.y - self.rolagem.pos;
        for (l, r) in da_aba {
            let h = altura(l);
            let card = Rect::new(area.x, y, area.w - 12.0, h);
            y += h + 6.0;
            if card.y + card.h < area.y || card.y > area.y + area.h {
                continue;
            }
            let topo = Rect::new(card.x, card.y, card.w, CARTAO * f);
            let sobre = !arrastando && topo.contains(mouse) && area.contains(mouse);
            estilo::ret_arredondado(
                card,
                8.0,
                Color::new(0.13, 0.21, 0.30, if sobre { 0.96 } else { 0.82 }),
            );
            icone(vec2(card.x + 24.0 * f, card.y + 28.0 * f), &r.estado, f);
            // As MESMAS medidas que o teste confere. Duas contas pro mesmo
            // lugar é como o desenho e a conta se separam sem ninguém ver.
            let (ir, cx, esquerda, largura) = caixas_do_cartao(card, f);
            let tx = card.x + esquerda;
            estilo::texto_ajustado(&l.nome, tx, card.y + 22.0 * f, largura, 17, estilo::TEXTO);
            let passo = match r.atual {
                Some(d) if l.historia => format!(
                    "{} · {}",
                    historia::indice(d.id)
                        .map_or(String::new(), |i| historia::nome_do_capitulo(i).to_string()),
                    d.title
                ),
                Some(d) => format!("Step {} of {} · {}", r.feitos + 1, r.total, d.title),
                None => format!("{} of {} steps", r.total, r.total),
            };
            estilo::texto_ajustado(&passo, tx, card.y + 43.0 * f, largura, 13, estilo::SUAVE);
            if let Some(d) = r.atual {
                let mut linha = frase(d, &r.estado);
                if !matches!(r.estado, Estado::Concluida | Estado::Bloqueada(_)) {
                    if let Some(ilha) = em_outra_ilha(d, c.zona) {
                        // Each half translated on its own: the dictionary
                        // matches whole phrases, and the joined one has none.
                        // The island FIRST: the card cuts the end of the line.
                        linha = format!(
                            "{} · {}",
                            shared::idioma::tr(&format!("On {ilha}")),
                            shared::idioma::tr(&linha)
                        );
                    }
                }
                estilo::texto_ajustado(
                    &linha,
                    tx,
                    card.y + 63.0 * f,
                    largura,
                    13,
                    cor_do_estado(&r.estado),
                );
                // O QUE ELA PAGA. Em verde e por último: é o motivo de fazer,
                // e vem depois do que ela pede, que é o custo.
                let premio = recompensa_de(d, c.nomes);
                if !premio.is_empty() {
                    estilo::texto_ajustado(
                        &format!("Gives: {premio}"),
                        tx,
                        card.y + 83.0 * f,
                        largura,
                        13,
                        estilo::AUTO,
                    );
                }
            }
            // "Ir" no passo atual, quando da' pra fazer algo com ele.

            let clicavel = matches!(
                r.estado,
                Estado::Disponivel | Estado::EmAndamento { .. } | Estado::Pronta
            ) || matches!(&r.estado, Estado::Bloqueada(m) if nivel_bloqueado(m).is_some());
            let auto = r.atual.is_some_and(|d| pode_ir(d, &r.estado));
            // A CAIXA DA FILA, logo abaixo do "Ir".
            //
            // Marcar é dizer "esta, e nesta ordem" — por isso ela mostra o
            // NÚMERO da posição e não um tique: numa fila de dez, saber que
            // algo está marcado sem saber onde não ajuda a montar nada.

            let mut marcou = None;
            if let (true, Some(d)) = (clicavel && r.atual.is_some_and(automatizavel), r.atual) {
                let pos = self.posicao_na_fila(d.id);
                estilo::cartao(cx, cx.contains(mouse), pos.is_some());
                match pos {
                    Some(n) => estilo::texto_centro_forte(
                        cx.center().x,
                        cx.center().y + 5.0 * f,
                        &n.to_string(),
                        15,
                        estilo::OURO,
                    ),
                    None => estilo::texto_centro(
                        cx.center().x,
                        cx.center().y + 5.0 * f,
                        "+",
                        16,
                        estilo::SUAVE,
                    ),
                }
                if tocou(cx) {
                    marcou = Some(d.id);
                }
            }
            if let Some(id) = marcou {
                marca_pedida = Some(id);
            }
            if let (true, Some(d), None) = (clicavel, r.atual, marcou) {
                // "Take" quando ela ainda não foi aceita: o botão diz o que
                // vai acontecer, e o que acontece agora é aceitar na hora.
                let rotulo = if d.id == 905 {
                    "Do"
                } else if matches!(r.estado, Estado::Disponivel) {
                    "Take"
                } else if matches!(&r.estado, Estado::Bloqueada(m) if nivel_bloqueado(m).is_some())
                    || d.obj_kind == shared::quests::objective_kind::NIVEL && !auto {
                    "How to level"
                } else if !auto && tem_atalho_manual(d) {
                    "Open"
                } else if !auto {
                    "Details"
                } else {
                    "Go"
                };
                let _ = crate::ui::botao(ir, rotulo, true);
                if tocou(ir) {
                    if auto || tem_atalho_manual(d) || matches!(r.estado, Estado::Disponivel)
                        || d.obj_kind == shared::quests::objective_kind::NIVEL
                        || matches!(&r.estado, Estado::Bloqueada(m) if nivel_bloqueado(m).is_some()) {
                        saida = Some(clique_de(d, &r.estado));
                    } else {
                        alternar = Some(l.nome.clone());
                    }
                }
            }
            if tocou(topo) && !(clicavel && (tocou(ir) || tocou(cx))) {
                alternar = Some(l.nome.clone());
            }
            // Aberta: os passos, com o que ja' foi, o atual e o que falta.
            if self.expandida.as_deref() == Some(l.nome.as_str()) {
                let mut py = card.y + CARTAO * f;
                if let Some(d) = r.atual.filter(|d| !automatizavel(d)) {
                    let linhas = detalhes_manuais(d, (card.w - 62.0 * f).max(80.0));
                    let h = linhas.len() as f32 * 20.0 * f + 12.0 * f;
                    estilo::ret_arredondado(
                        Rect::new(card.x + 12.0 * f, py, card.w - 24.0 * f, h),
                        5.0, Color::new(0.08, 0.15, 0.22, 0.95));
                    for (i, linha) in linhas.iter().enumerate() {
                        estilo::texto(card.x + 20.0 * f, py + (20.0 + i as f32 * 20.0) * f,
                            linha, 13, if i == 0 { estilo::OURO } else { estilo::TEXTO });
                    }
                    py += h;
                }
                for (d, e) in l.passos.iter().zip(&r.estados) {
                    let atual = r.atual.is_some_and(|a| a.id == d.id);
                    let cor = if atual {
                        estilo::OURO
                    } else {
                        cor_do_estado(e)
                    };
                    icone(vec2(tx + 6.0 * f, py + PASSO * f * 0.5), e, f * 0.7);
                    let t = if atual {
                        format!("› {}", d.title)
                    } else {
                        d.title.to_string()
                    };
                    estilo::texto_ajustado(
                        &t,
                        tx + 22.0 * f,
                        py + PASSO * f * 0.68,
                        (largura - 22.0 * f).max(24.0),
                        13,
                        cor,
                    );
                    py += PASSO * f;
                }
            }
        }
        crate::rolagem::recortar(None);
        self.rolagem.desenha(area, total);
        if let Some(id) = marca_pedida {
            if !self.alterna_marca(id) {
                saida = Some(Clique::Aviso(format!("The queue already has {FILA_MAX} quests.")));
            }
        }
        if let Some(nome) = alternar {
            self.expandida = if self.expandida.as_deref() == Some(nome.as_str()) {
                None
            } else {
                Some(nome)
            };
        }
        // O RODAPÉ DA FILA, só quando há algo marcado.
        //
        // Ele só aparece com marca porque um botão morto no rodapé de toda
        // abertura seria mais uma coisa a ignorar — e porque a fila é um modo
        // em que se entra de propósito, não o jeito normal de usar o menu.
        if let Some(rod) = rodape {
            let n = self.marcadas.len();
            let [todas_b, b, limpar] = botoes_do_rodape(rod, f);
            estilo::texto(
                rod.x,
                rod.y + 26.0 * f,
                &format!("Queue: {n} of {FILA_MAX} · locked ones are skipped"),
                14,
                estilo::SUAVE,
            );
            // MARCAR TODAS: o que está na tela AGORA, na ordem em que está.
            //
            // O que está na tela, e não "todas as do jogo": os dois filtros
            // acima são a escolha do jogador, e um botão que os ignorasse
            // encheria a fila com o que ele acabou de filtrar fora.
            //
            // Para no teto da fila em vez de recusar tudo: encher dez de doze
            // é o que a pessoa quis, e recusar por causa das duas que sobram
            // seria obedecer ao número em vez de à intenção.
            if crate::ui::botao(todas_b, "Select all", !marcaveis.is_empty()) {
                let cabem = FILA_MAX.saturating_sub(self.marcadas.len());
                let novas: Vec<u16> = marcaveis
                    .iter()
                    .copied()
                    .filter(|id| !self.marcadas.contains(id))
                    .take(cabem)
                    .collect();
                for id in novas {
                    self.marcadas.push(id);
                    self.marca_mudou = Some((id, true));
                }
            }
            if crate::ui::botao(b, &format!("Do all {n}"), n > 0) {
                saida = Some(Clique::Fila(std::mem::take(&mut self.marcadas)));
            }
            if crate::ui::botao(limpar, "Clear", n > 0) {
                // Desfixa uma por uma, pra quem escuta receber cada baixa: o
                // `Limpar` tem que soltar as do rastreador junto, senão a
                // lista da esquerda fica com missões que ninguém mais vai
                // fazer.
                for id in std::mem::take(&mut self.marcadas) {
                    self.marca_mudou = Some((id, false));
                    self.desfixar.push(id);
                }
            }
        }
        saida
    }
}

/// Cadeado: arco em cima e corpo.
pub(crate) fn cadeado(c: Vec2, s: f32, cor: Color) {
    if crate::icones_ui::ui("cadeado", c, s * 2.6, cor) {
        return;
    }
    draw_circle_lines(c.x, c.y - s * 0.35, s * 0.55, s * 0.22, cor);
    draw_rectangle(c.x - s * 0.8, c.y - s * 0.2, s * 1.6, s * 1.2, cor);
    draw_circle(c.x, c.y + s * 0.3, s * 0.18, Color::new(0.0, 0.0, 0.0, 0.7));
}

/// Um mapa de nomes vazio pros testes: eles medem estado e clique, não texto
/// de recompensa.
#[cfg(test)]
pub(crate) static NOMES_DE_TESTE: std::sync::LazyLock<HashMap<u16, String>> =
    std::sync::LazyLock::new(HashMap::new);

/// Preview of the quest menu (`MMO_PREVIA_MENU_MISSOES=1`; PNGs in
/// `MMO_PREVIA_SAIDA`): a level 30 player on the Glacier, so other islands'
/// quests show as available "on <island>" and the map list ends in Skyreach.
#[cfg(debug_assertions)]
pub async fn previa_menu() {
    use macroquad::prelude::*;
    let saida = std::env::var("MMO_PREVIA_SAIDA").unwrap_or_else(|_| "/tmp/tempest-menu-missoes".into());
    std::fs::create_dir_all(&saida).unwrap();
    let rt = render_target(1920, 1080);
    crate::render3d::define_alvo(Some(rt.clone()));
    crate::hud_layout::define_escala_ui(1.6);
    let nomes: HashMap<u16, String> = HashMap::new();
    let entregues: HashMap<u16, i64> = HashMap::new();
    let nada = |_: u16| 0u32;
    let c = Contexto {
        log: &[],
        entregues: &entregues,
        nivel: 30,
        faccao: faction_id::PEACEMAIN,
        zona: Some("ilha_gelo"),
        agora_unix: 1_000,
        tem: &nada,
        nomes: &nomes,
    };
    for (nome, aba, mapa) in [("disponiveis", Aba::Disponiveis, None), ("travadas", Aba::Bloqueadas, None), ("ermo", Aba::Disponiveis, Some("ilha_deserto"))] {
        let mut m = MenuMissoes::default();
        m.abrir();
        m.aba = Some(aba);
        m.mapa = mapa;
        for _ in 0..3 {
            crate::render3d::camera_padrao();
            clear_background(Color::new(0.08, 0.12, 0.16, 1.));
            m.desenha(&c);
            unsafe { get_internal_gl().flush() };
            rt.texture.get_texture_data().export_png(&format!("{saida}/{nome}.png"));
            next_frame().await;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use shared::quests::{quest_by_id, quest_status};

    fn nada(_: u16) -> u32 {
        0
    }

    fn ctx<'a>(
        log: &'a [QuestNet],
        entregues: &'a HashMap<u16, i64>,
        nivel: u32,
        zona: Option<&'a str>,
    ) -> Contexto<'a> {
        Contexto {
            nomes: &NOMES_DE_TESTE,
            log,
            entregues,
            nivel,
            faccao: faction_id::PEACEMAIN,
            zona,
            agora_unix: 1_000,
            tem: &nada,
        }
    }

    #[test]
    fn a_cadeia_do_mestre_bloqueia_com_o_motivo_certo() {
        let (d501, d502, d504) = (
            quest_by_id(501).unwrap(),
            quest_by_id(502).unwrap(),
            quest_by_id(504).unwrap(),
        );
        let vazio = HashMap::new();
        let c = ctx(&[], &vazio, 1, Some("ilha_inicial"));
        assert_eq!(estado(d501, &c), Estado::Disponivel);
        assert_eq!(
            estado(d502, &c),
            Estado::Bloqueada(vec!["Complete: Meet the Alchemist".into()])
        );
        // 504: nivel 3 e a 503 antes.
        match estado(d504, &c) {
            Estado::Bloqueada(m) => {
                assert!(m.contains(&"Requires level 3".to_string()), "{m:?}");
                assert!(m.iter().any(|s| s.starts_with("Complete: ")), "{m:?}");
            }
            outro => panic!("{outro:?}"),
        }
        // Another island is NOT a lock (the server takes it from anywhere and
        // the auto quest sails there); the card says where.
        let fora = ctx(&[], &vazio, 1, Some("ilha_gelo"));
        assert_eq!(estado(d501, &fora), Estado::Disponivel);
        assert_eq!(em_outra_ilha(d501, Some("ilha_gelo")).as_deref(), Some("Bosque"));
        assert_eq!(em_outra_ilha(d501, Some("ilha_inicial")), None);
    }

    #[test]
    fn log_e_entregues_viram_andamento_pronta_e_concluida() {
        let d501 = quest_by_id(501).unwrap();
        let d502 = quest_by_id(502).unwrap();
        let mut entregues = HashMap::new();
        entregues.insert(501, 0);
        let mut q = QuestNet::from_def(d502, quest_status::ACTIVE, 2);
        let log = vec![q.clone()];
        let c = ctx(&log, &entregues, 1, Some("ilha_inicial"));
        assert_eq!(estado(d501, &c), Estado::Concluida);
        assert_eq!(estado(d502, &c), Estado::EmAndamento { feito: 2, total: 6 });
        q.status = quest_status::READY;
        let log = vec![q];
        let c = ctx(&log, &entregues, 1, Some("ilha_inicial"));
        assert_eq!(estado(d502, &c), Estado::Pronta);
    }

    #[test]
    fn liberada_anda_e_bloqueada_so_avisa() {
        let d501 = quest_by_id(501).unwrap();
        assert_eq!(clique_de(d501, &Estado::Disponivel), Clique::Aceitar(501));
        assert_eq!(clique_de(d501, &Estado::Pronta), Clique::AutoMissao(501));
        assert_eq!(
            clique_de(d501, &Estado::EmAndamento { feito: 0, total: 1 }),
            Clique::AutoMissao(501)
        );
        match clique_de(d501, &Estado::Bloqueada(vec!["Requires level 3".into()])) {
            Clique::OpcoesDeNivel(3) => {},
            outro => panic!("bloqueada andou: {outro:?}"),
        }
        assert!(matches!(
            clique_de(d501, &Estado::Concluida),
            Clique::Aviso(_)
        ));
    }

    #[test]
    fn todos_os_passos_da_historia_tem_acao_compativel() {
        use shared::quests::{objective_kind as o, tutorial as t};
        for d in shared::historia::PASSOS {
            let mundo = matches!(d.obj_kind, o::TALK | o::KILL | o::GATHER | o::LUGAR | o::VIAGEM)
                || (d.obj_kind == o::TUTORIAL && d.obj_target == t::COLETA_ENERGIA);
            assert_eq!(automatizavel(d), mundo, "{}: {}", d.id, d.title);
            let clique = clique_de(d, &Estado::EmAndamento { feito: 0, total: d.obj_count });
            if mundo {
                assert_eq!(clique, Clique::AutoMissao(d.id));
            } else if d.obj_kind == o::NIVEL {
                assert!(matches!(clique, Clique::OpcoesDeNivel(_)));
            } else {
                assert_eq!(clique, Clique::AbrirManual(d.id));
            }
        }
    }

    #[test]
    fn tutorial_de_energia_viaja_e_coleta_sem_abrir_painel_manual() {
        let d = quest_by_id(796).unwrap();
        assert!(automatizavel(d));
        assert!(pode_iniciar_auto(d, quest_status::ACTIVE));
        assert!(!tem_atalho_manual(d));
        assert_eq!(clique_de(d, &Estado::EmAndamento { feito: 0, total: d.obj_count }), Clique::AutoMissao(d.id));
        for acao in [shared::quests::tutorial::PONTO_ATRIBUTO, shared::quests::tutorial::EVOLUIR_SKILL] {
            assert!(!automatizavel(&QuestDef { obj_target: acao, ..*d }));
        }
    }

    #[test]
    fn tarefas_manuais_nao_entram_no_auto() {
        use shared::quests::objective_kind as obj;
        let base = *quest_by_id(502).unwrap();
        assert!(automatizavel(&base));
        for tipo in [obj::CRAFT, obj::REFINE, obj::DUNGEON, obj::RAID,
            obj::PVP_KILL, obj::ENCHANT, obj::TREASURE, obj::TUTORIAL, obj::NIVEL] {
            let d = QuestDef { obj_kind: tipo, ..base };
            assert!(!automatizavel(&d), "tipo {tipo} entrou no auto");
            let acao = clique_de(&d, &Estado::EmAndamento { feito: 0, total: 1 });
            assert!(matches!(acao, Clique::Aviso(_) | Clique::OpcoesDeNivel(_) | Clique::AbrirManual(_)));
            assert_eq!(clique_de(&d, &Estado::Pronta), Clique::AutoMissao(d.id));
            assert!(!pode_iniciar_auto(&d, quest_status::ACTIVE));
            assert!(pode_iniciar_auto(&d, quest_status::READY));
        }
    }

    #[test]
    fn atalho_individual_nao_libera_fila_manual() {
        for id in [524,523] {
            let d=quest_by_id(id).unwrap();
            assert_eq!(clique_de(d,&Estado::EmAndamento {feito:0,total:1}),Clique::AbrirManual(id));
            assert!(!automatizavel(d));
            assert!(!pode_iniciar_auto(d,quest_status::ACTIVE));
            assert_eq!(clique_de(d,&Estado::Pronta),Clique::AutoMissao(id));
        }
    }

    #[test]
    fn missao_manual_explica_menu_quantidade_e_dungeon() {
        let craft = quest_by_id(524).unwrap();
        let forja = quest_by_id(523).unwrap();
        let dungeon = quest_by_id(531).unwrap();
        assert!(caminho_manual(craft).contains("Menu › Craft: create 2"));
        assert!(caminho_manual(forja).contains("Menu › Forge: try refining 3"));
        assert!(caminho_manual(dungeon).contains("Shipwreck Cellar"));
        let linha = frase(craft, &Estado::EmAndamento { feito: 1, total: 2 });
        assert!(linha.contains("1/2") && linha.contains("Craft"), "{linha}");
    }

    #[test]
    fn o_menu_lista_a_cadeia_em_ordem_e_esconde_as_legadas() {
        let bosque: Vec<u16> = todas()
            .iter()
            .filter(|d| zona_da_missao(d.id) == Some("ilha_inicial"))
            .map(|d| d.id)
            .collect();
        // 501-505 e' a cadeia original; 506-509 e' a oficina do Mestre, que
        // apresenta madeira, darksteel, quintessencia e berloque — as fontes
        // que a receita cinza pede e que o inicio nunca mostrava; 510 leva a
        // primeira dungeon, que era o unico lugar do inicio que ninguem
        // apresentava. A lista fica literal de proposito: e' ela que pega um id
        // legado caindo por engano na faixa da ilha. 511-538 sao as seis
        // cadeias paralelas do resto da ilha (chefes, bestiario, oficina,
        // coleta, dungeons, vila). 539-543 e' a CACADA: a cadeia de volume
        // (30/60/100) e as duas tematicas, pedidas em 22/09/2026 — "missoes
        // para matar mais inimigos que leva pras zonas de maior densidade".
        // 544-547 entregam as chaves de craft do catalogo cinza (24/09/2026).
        // The chains in order, then the Bosque's kill contract (912).
        let esperado: Vec<u16> = (501..=547).chain([912]).collect();
        assert_eq!(&bosque[..], &esperado[..], "so' as cadeias, em ordem");
        assert!(
            todas().iter().all(|d| !d.daily),
            "diaria no menu de todas: tem painel proprio"
        );
        assert!(
            todas().iter().all(|d| zona_da_missao(d.id).is_some()),
            "legada no menu"
        );
    }

    /// Historia: antes do atual concluido, o atual em andamento, o futuro com o
    /// passo anterior como motivo (e sem trava de nivel no capitulo I); o menu mostra os
    /// capitulos escritos e so' as cronicas perto do atual.
    #[test]
    fn historia_no_menu_concluida_atual_e_futura() {
        let vazio = HashMap::new();
        let atual = historia::def_da_historia(705).unwrap();
        let log = vec![QuestNet::from_def(atual, quest_status::ACTIVE, 3)];
        let c = ctx(&log, &vazio, 4, Some("ilha_inicial"));
        assert_eq!(
            estado_da_historia(historia::def_da_historia(702).unwrap(), &c),
            Estado::Concluida
        );
        assert_eq!(
            estado_da_historia(atual, &c),
            Estado::EmAndamento {
                feito: 3,
                total: 10
            }
        );
        match estado_da_historia(historia::def_da_historia(712).unwrap(), &c) {
            Estado::Bloqueada(m) => {
                // Capitulo I sem trava de nivel: a historia carrega o nivel.
                assert!(!m.iter().any(|s| s.starts_with("Requires level")), "{m:?}");
                assert!(m.iter().any(|s| s.starts_with("Complete: ")), "{m:?}");
            }
            outro => panic!("{outro:?}"),
        }
        assert!(matches!(
            clique_de(
                atual,
                &Estado::EmAndamento {
                    feito: 3,
                    total: 10
                }
            ),
            Clique::AutoMissao(705)
        ));
        // Outra ilha no futuro: diz onde. (721 desde que o barco entrou na
        // historia e empurrou o capitulo II em dois — docs/MAR_ABERTO.md.)
        match estado_da_historia(historia::def_da_historia(721).unwrap(), &c) {
            Estado::Bloqueada(m) => assert!(m.iter().any(|s| s == "On Geleira"), "{m:?}"),
            outro => panic!("{outro:?}"),
        }
        let lista = lista_do_menu(&log);
        assert_eq!(lista[0].id, historia::PRIMEIRO_ID);
        let n_hist = lista
            .iter()
            .filter(|d| historia::e_da_historia(d.id))
            .count() as u32;
        assert_eq!(
            n_hist,
            historia::total_escritos() + CRONICAS_NO_MENU * historia::PASSOS_POR_CRONICA
        );
        // Na cronica 3, o menu mostra a 3 e a 4 (nao todas desde a 1).
        let longe = historia::id_do_passo(
            historia::total_escritos() + 2 * historia::PASSOS_POR_CRONICA + 1,
        )
        .unwrap();
        let log = vec![QuestNet::from_def(
            historia::def_da_historia(longe).unwrap(),
            quest_status::ACTIVE,
            0,
        )];
        let ids: Vec<u16> = lista_do_menu(&log)
            .iter()
            .map(|d| d.id)
            .filter(|id| *id >= historia::PRIMEIRO_ID_DO_EPILOGO)
            .collect();
        assert_eq!(
            ids.first().copied(),
            historia::id_do_passo(historia::total_escritos() + 2 * historia::PASSOS_POR_CRONICA)
        );
    }

    /// O menu mostra LINHAS: a historia e uma entrada por cadeia, com o passo
    /// atual. Nada de 38 passos soltos.
    #[test]
    fn linhas_juntam_a_cadeia_e_acham_o_passo_atual() {
        let ls = linhas(&[]);
        assert!(ls[0].historia, "a historia vem primeiro");
        let nomes: Vec<&str> = ls
            .iter()
            .filter(|l| !l.historia)
            .map(|l| l.nome.as_str())
            .collect();
        for n in [
            "The first days",
            "The bosses of the Grove",
            "Bestiary of the Grove",
            "Workshop",
            "Hands in the soil",
            "Cellars and vaults",
            "Village folk",
        ] {
            assert!(nomes.contains(&n), "{n} fora do menu: {nomes:?}");
        }
        let chefes = ls.iter().find(|l| l.nome == "The bosses of the Grove").unwrap();
        assert_eq!(
            chefes.passos.iter().map(|d| d.id).collect::<Vec<_>>(),
            vec![511, 512, 513, 514, 515]
        );

        // Nivel 16, a 511 entregue: a linha dos chefes esta' no passo 2 e
        // continua na cabana do Guia do Mirante.
        let mut entregues = HashMap::new();
        for id in 501..=507 {
            entregues.insert(id, 0);
        }
        entregues.insert(511, 0);
        let c = ctx(&[], &entregues, 16, Some("ilha_inicial"));
        let r = resumo(chefes, &c);
        assert_eq!(r.atual.map(|d| d.id), Some(512));
        assert_eq!((r.feitos, r.total), (1, 5));
        assert_eq!(aba_de(&r), Aba::Disponiveis);
        assert!(frase(r.atual.unwrap(), &r.estado).contains("Lookout Guide"));

        // Em andamento vence disponivel; tudo feito vai pra Concluidas.
        let ativa = QuestNet::from_def(quest_by_id(512).unwrap(), quest_status::ACTIVE, 0);
        let log = [ativa];
        let c2 = ctx(&log, &entregues, 16, Some("ilha_inicial"));
        assert_eq!(aba_de(&resumo(chefes, &c2)), Aba::EmAndamento);
        for id in 512..=515 {
            entregues.insert(id, 0);
        }
        let c3 = ctx(&[], &entregues, 16, Some("ilha_inicial"));
        let fim = resumo(chefes, &c3);
        assert_eq!(
            (aba_de(&fim), fim.atual.map(|d| d.id)),
            (Aba::Concluidas, None)
        );
        // Nivel baixo: a primeira da linha trava e ela vai pra Bloqueadas.
        let nenhuma = HashMap::new();
        let c4 = ctx(&[], &nenhuma, 1, Some("ilha_inicial"));
        assert_eq!(aba_de(&resumo(chefes, &c4)), Aba::Bloqueadas);
    }

    /// Diaria de sistema que ainda nao existe: cadeado com "Coming soon"; reset
    /// conta ate' a meia-noite UTC.
    #[test]
    fn diaria_em_breve_bloqueia_e_o_reset_conta_ate_a_meia_noite() {
        let vazio = HashMap::new();
        let c = ctx(&[], &vazio, 50, Some("ilha_inicial"));
        // Locked, and since 04/10/2026 also out of every island's list
        // ("Unavailable in this version" joins the reason).
        assert!(matches!(
            estado(quest_by_id(607).unwrap(), &c),
            Estado::Bloqueada(ref m) if m.iter().any(|s| s == "Coming soon")
        ));
        assert_eq!(estado(quest_by_id(601).unwrap(), &c), Estado::Disponivel);
        // 19:53 UTC: the next reset is 07:00 UTC (04:00 Brasilia).
        assert_eq!(reset_em(86_400 * 10 + 3_600 * 19 + 60 * 53), "11h 07min");
    }
}
#[cfg(test)]
mod testes_da_trava {
    use super::*;

    /// O CARTÃO DE MISSÃO cabe em si mesmo, em toda tela.
    ///
    /// O cartão cresceu de 76 pra 96 px pra caber a linha da recompensa, e a
    /// caixa da fila entrou embaixo do "Ir". Os dois números foram escritos à
    /// mão — e foi exatamente esse tipo de conta que pôs o botão de entrar da
    /// Ilha Mágica fora da janela e prendeu o jogador lá dentro.
    ///
    /// Aqui a conta é medida: botões dentro do cartão, texto com largura
    /// positiva, e a última linha (a recompensa, em `+83f`) dentro da altura.
    /// Os três botões do rodapé não se encavalam, em tela nenhuma.
    ///
    /// Eles dividem a mesma faixa, e foi exatamente esse tipo de conta — feita
    /// à mão e conferida no olho — que pôs o botão de entrar da Ilha Mágica
    /// fora da janela e prendeu o jogador lá.
    #[test]
    fn os_botoes_do_rodape_nao_se_encavalam() {
        for (w, f) in [
            (1040.0, 1.0),
            (760.0, 1.4),
            (420.0, 1.0),
            (320.0, 2.2),
            (280.0, 2.2),
        ] {
            let rod = Rect::new(10.0, 500.0, w - 36.0, 40.0 * f);
            let b = botoes_do_rodape(rod, f);
            let nomes = ["marcar todas", "fazer", "limpar"];
            for i in 0..3 {
                assert!(
                    b[i].x >= rod.x - 0.01 && b[i].x + b[i].w <= rod.x + rod.w + 0.01,
                    "{w}x{f}: {} vaza o rodapé: {:?} em {rod:?}",
                    nomes[i],
                    b[i]
                );
                assert!(b[i].w > 8.0, "{w}x{f}: {} ficou sem largura", nomes[i]);
                for j in i + 1..3 {
                    assert!(
                        b[i].x + b[i].w <= b[j].x + 0.01,
                        "{w}x{f}: {} encosta em {}",
                        nomes[i],
                        nomes[j]
                    );
                }
            }
        }
    }

    /// O rodapé existe mesmo sem marca nenhuma.
    ///
    /// É onde mora o "Select all", que é justamente o botão de quem não
    /// marcou nada: escondê-lo atrás de uma marca seria trancar o atalho
    /// atrás do trabalho que ele evita.
    #[test]
    fn o_rodape_aparece_sem_marca() {
        let p = Rect::new(0.0, 0.0, 800.0, 600.0);
        assert!(rodape_da_fila(p, 1.0, false).is_some());
        assert!(rodape_da_fila(p, 1.0, true).is_some());
    }

    /// Cada linha cai num tipo só, e "All" aceita qualquer uma.
    #[test]
    fn o_tipo_separa_as_linhas() {
        let hist = Linha {
            nome: "Story".into(),
            // Qualquer passo serve: `tipo_de` olha a bandeira `historia`
            // antes da fonte, e é isso que o teste quer travar.
            passos: vec![todas().into_iter().next().unwrap()],
            historia: true,
        };
        assert_eq!(tipo_de(&hist), Tipo::Historia);
        assert!(cabe_no_tipo(&hist, Tipo::Todos));
        assert!(cabe_no_tipo(&hist, Tipo::Historia));
        assert!(!cabe_no_tipo(&hist, Tipo::Secundarias));

        // Uma cadeia de NPC é "Residents"; uma de quadro é "Side quests".
        let de_npc = todas().into_iter().find(|d| {
            d.source == shared::quests::quest_source::NPC && !historia::e_da_historia(d.id)
        });
        if let Some(d) = de_npc {
            let l = Linha {
                nome: d.title.into(),
                passos: vec![d],
                historia: false,
            };
            assert_eq!(tipo_de(&l), Tipo::Moradores, "quest {} é de NPC", d.id);
        }
        let de_quadro = todas().into_iter().find(|d| {
            d.source == shared::quests::quest_source::BOARD && !historia::e_da_historia(d.id)
        });
        if let Some(d) = de_quadro {
            let l = Linha {
                nome: d.title.into(),
                passos: vec![d],
                historia: false,
            };
            assert_eq!(tipo_de(&l), Tipo::Secundarias, "quest {} é do quadro", d.id);
        }
    }

    #[test]
    fn filtro_de_mapa_separa_ilhas_sem_esconder_historia_atual() {
        let ls = linhas(&[]);
        let bos = ls.iter().find(|l| l.passos.iter().any(|d| d.id == 501)).unwrap();
        let gelo = ls.iter().find(|l| l.passos.iter().any(|d| d.id == 810)).unwrap();
        let resumo = |l: &Linha| Resumo {
            atual: l.passos.first().copied(),
            estado: Estado::Disponivel,
            feitos: 0,
            total: l.passos.len(),
            estados: Vec::new(),
        };
        assert!(cabe_no_mapa(bos, &resumo(bos), Some("ilha_inicial")));
        assert!(!cabe_no_mapa(bos, &resumo(bos), Some("ilha_gelo")));
        assert!(cabe_no_mapa(gelo, &resumo(gelo), Some("ilha_gelo")));
        assert!(cabe_no_mapa(gelo, &resumo(gelo), None));
    }

    /// Todo tipo tem nome, e nenhum repete — eles são rótulo de botão.
    #[test]
    fn os_tipos_tem_nomes_distintos() {
        let mut vistos = std::collections::HashSet::new();
        for t in Tipo::TODOS {
            assert!(!t.nome().is_empty());
            assert!(vistos.insert(t.nome()), "nome repetido: {}", t.nome());
        }
        assert_eq!(Tipo::default(), Tipo::Todos, "o menu abre mostrando tudo");
    }

    #[test]
    fn o_cartao_de_missao_cabe_em_si_mesmo() {
        // Larguras de painel plausíveis, da mais apertada à mais folgada.
        for largura in [280.0f32, 420.0, 540.0, 720.0] {
            for f in [0.8f32, 1.0, 1.5, 2.2] {
                let card = Rect::new(0.0, 0.0, largura, CARTAO * f);
                let (ir, fila, esquerda, texto) = caixas_do_cartao(card, f);
                for (nome, b) in [("Ir", ir), ("fila", fila)] {
                    assert!(
                        b.x >= card.x && b.x + b.w <= card.x + card.w + 0.01,
                        "largura {largura} f={f}: o botão {nome} vaza de lado \
                         ({:.0}..{:.0} num cartão de {:.0})",
                        b.x,
                        b.x + b.w,
                        card.w
                    );
                    assert!(
                        b.y + b.h <= card.y + card.h + 0.01,
                        "largura {largura} f={f}: o botão {nome} passa da altura \
                         do cartão ({:.0} > {:.0})",
                        b.y + b.h,
                        card.h
                    );
                }
                assert!(
                    texto > 0.0,
                    "largura {largura} f={f}: sobra {texto:.0} px pro nome da missão"
                );
                assert!(
                    esquerda + texto <= ir.x - card.x + 0.01,
                    "largura {largura} f={f}: o nome da missão ({:.0}..{:.0}) passa \
                     por baixo do botão, que começa em {:.0}",
                    esquerda,
                    esquerda + texto,
                    ir.x - card.x
                );
                // A ÚLTIMA LINHA do cartão é a recompensa, na base +83f.
                assert!(
                    83.0 * f <= card.h,
                    "f={f}: a linha da recompensa (+{:.0}) cai fora do cartão ({:.0})",
                    83.0 * f,
                    card.h
                );
            }
        }
    }

    /// MARCAR PRA FILA AVISA quem fixa no rastreador — e desmarcar também.
    ///
    /// O dono: "as quests marcadas têm que ficar fixadas na esquerda, no
    /// atalho". O evento existe em vez de espelhar o conjunto porque
    /// espelhar apagaria as que ele fixou na mão toda vez que marcasse
    /// qualquer coisa aqui.
    #[test]
    fn marcar_avisa_pra_fixar_e_limpar_solta_todas() {
        let mut m = MenuMissoes::default();
        assert_eq!(m.marca_mudou(), None, "sem marca, sem aviso");

        assert!(m.alterna_marca(7));
        assert_eq!(m.marca_mudou(), Some((7, true)), "marcou: fixa");
        assert_eq!(m.marca_mudou(), None, "o aviso é consumido uma vez");

        assert!(m.alterna_marca(7));
        assert_eq!(m.marca_mudou(), Some((7, false)), "desmarcou: solta");

        // A fila cheia NÃO avisa: nada foi marcado.
        for id in 1..=FILA_MAX as u16 {
            assert!(m.alterna_marca(id));
        }
        let _ = m.marca_mudou();
        assert!(!m.alterna_marca(99), "passou do teto");
        assert_eq!(m.marca_mudou(), None, "recusada não fixa nada");

        // LIMPAR solta TODAS, uma por uma: uma variável de evento só não dá
        // conta de dez baixas no mesmo quadro, e o rastreador ficaria com
        // missões que ninguém mais vai fazer.
        let antes: Vec<u16> = m.marcadas.clone();
        m.marcadas.clear();
        for id in &antes {
            m.desfixar.push(*id);
        }
        let soltas = m.desfixar();
        assert_eq!(soltas, antes, "o Limpar tem que soltar todas");
        assert!(m.desfixar().is_empty(), "consome uma vez");
    }

    /// O RODAPÉ DA FILA NÃO COBRE A LISTA.
    ///
    /// Ele era desenhado por cima dela: cobria o último cartão e o toque
    /// acertava a missão em vez do botão — o dono não conseguia clicar em
    /// "Fazer as N". O teste que eu tinha media o CARTÃO, e o defeito estava
    /// entre o rodapé e a ÁREA, que ninguém media.
    #[test]
    fn o_rodape_da_fila_nao_cobre_a_lista() {
        for (w, h) in [(300.0f32, 260.0f32), (540.0, 480.0), (900.0, 1000.0)] {
            for f in [0.8f32, 1.0, 1.5, 2.2] {
                let p = Rect::new(0.0, 0.0, w, h);
                let topo = p.y + 90.0 * f; // onde a lista começa, com abas
                for tem_marca in [false, true] {
                    let rod = rodape_da_fila(p, f, tem_marca);
                    // O rodapé passou a existir SEMPRE (23/09/2026): ele
                    // carrega o "Select all", que é o botão de quem ainda
                    // não marcou nada. O que este teste guarda continua
                    // valendo, e é o que importa — ele não pode cobrir a
                    // lista.
                    assert!(rod.is_some(), "o rodapé é sempre desenhado");
                    let fim = rod.map_or(p.y + p.h - 10.0, |r| r.y - 8.0 * f);
                    let area = Rect::new(p.x + 10.0, topo, p.w - 20.0, (fim - topo).max(40.0));
                    if let Some(r) = rod {
                        assert!(
                            area.y + area.h <= r.y + 0.01 || area.h <= 40.0,
                            "{w}x{h} f={f}: a lista acaba em {:.0} e o rodapé começa em \
                             {:.0} — eles se cobrem",
                            area.y + area.h,
                            r.y
                        );
                        assert!(
                            r.y + r.h <= p.y + p.h + 0.01,
                            "{w}x{h} f={f}: o rodapé passa do fim do painel"
                        );
                    }
                    assert!(area.h > 0.0, "{w}x{h} f={f}: a lista ficou sem altura");
                }
            }
        }
    }

    /// A FILA tem teto, guarda a ORDEM e desmarca.
    ///
    /// A ordem é a escolha: marcar é dizer "esta depois daquela". Um conjunto
    /// perderia isso e a fila sairia em ordem qualquer, que é o contrário do
    /// que "fazer em sequência" quer dizer.
    #[test]
    fn a_fila_guarda_a_ordem_e_para_no_teto() {
        let mut m = MenuMissoes::default();
        for id in 1..=FILA_MAX as u16 {
            assert!(m.alterna_marca(id), "{id} cabia e foi recusada");
            assert_eq!(m.posicao_na_fila(id), Some(id as usize));
        }
        // A décima primeira é RECUSADA, e não troca nenhuma.
        assert!(!m.alterna_marca(99), "passou do teto");
        assert_eq!(m.posicao_na_fila(99), None);
        assert_eq!(m.marcadas.len(), FILA_MAX);

        // Desmarcar abre vaga e as de trás sobem.
        assert!(m.alterna_marca(1));
        assert_eq!(m.posicao_na_fila(1), None);
        assert_eq!(m.posicao_na_fila(2), Some(1), "as de trás sobem");
        assert!(m.alterna_marca(99), "com vaga, entra");
        assert_eq!(m.posicao_na_fila(99), Some(FILA_MAX), "e entra no FIM");

        // Abrir o menu limpa: uma fila montada ontem e iniciada sem querer
        // hoje é pior que remontar.
        m.abrir();
        assert!(m.marcadas.is_empty());
    }

    /// A TRAVA DE NÍVEL manda pra Ilha Mágica, e diz por quê.
    ///
    /// O dono: "na missão 'pegue nível 20' geralmente ela chega nível 17".
    /// O botão "Ir" dela não tinha pra onde ir — mandava `AutoMissao`, que
    /// respondia "História: chegue ao nível 20 para continuar" e parava.
    /// Uma missão cujo botão não faz nada é pior que uma missão sem botão.
    #[test]
    fn a_trava_de_nivel_mostra_rotas_de_xp() {
        let d = shared::historia::PASSOS
            .iter()
            .find(|d| d.obj_kind == shared::quests::objective_kind::NIVEL)
            .expect("a história tem trava de nível");
        let andando = Estado::EmAndamento {
            feito: 17,
            total: 20,
        };
        assert_eq!(clique_de(d, &andando), Clique::OpcoesDeNivel(d.obj_count));

        // E a frase diz o que fazer, não só o quanto falta.
        let f = frase(d, &andando);
        assert!(f.contains("17/20"), "{f}");
        assert!(f.contains("Magic Island"), "{f}");
        assert!(f.contains("side quests"), "{f}");

        // Uma missão comum em andamento continua indo pela auto missão.
        let comum = shared::quests::QUESTS
            .iter()
            .find(|q| q.obj_kind == shared::quests::objective_kind::KILL)
            .expect("há missão de caça");
        assert_eq!(
            clique_de(comum, &andando),
            Clique::AutoMissao(comum.id),
            "só a trava de nível desvia"
        );
    }
}
