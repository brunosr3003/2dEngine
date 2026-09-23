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

const LARGURA: f32 = 540.0;

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
    /// Em andamento ou pronta: a auto missao que ja' existe.
    AutoMissao(u16),
    /// Nao anda: so' avisa.
    Aviso(String),
    /// A FILA: fazer estas, nesta ordem.
    ///
    /// Pedido pelo dono: "ter como colocar para fazer as missoes em
    /// sequencia, no maximo 10, escolher quais". A ordem e' a de marcacao —
    /// quem marcou por ultimo vai por ultimo —, e missao que nao da' pra
    /// fazer na hora e' PULADA, nao trava a fila.
    Fila(Vec<u16>),
    /// Trava de NIVEL: abre a Ilha Magica, que e' onde se arruma XP.
    ///
    /// A historia para esperando nivel e o jogador chega uns tres abaixo —
    /// o dono viu isso na missao do nivel 20, chegando no 17. O botao "Ir"
    /// dela nao tinha pra onde ir: mandava `AutoMissao`, que respondia
    /// "Historia: chegue ao nivel 20 para continuar" e parava. Agora ele
    /// leva ao lugar que resolve.
    IlhaMagica,
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
    if d.reward_xp > 0 {
        partes.push(format!("{} XP", d.reward_xp));
    }
    if d.reward_cobre > 0 {
        partes.push(format!("{} cobre", d.reward_cobre));
    }
    if d.reward_item != 0 && d.reward_item_qty > 0 {
        partes.push(format!("{}x {}", d.reward_item_qty, nome(d.reward_item)));
    }
    if d.reward_item2 != 0 && d.reward_item2_qty > 0 {
        partes.push(format!("{}x {}", d.reward_item2_qty, nome(d.reward_item2)));
    }
    if d.reward_faction_points > 0 {
        partes.push(format!("{} pts de facção", d.reward_faction_points));
    }
    partes.join("  ·  ")
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
            motivos.push(format!("Disponível de novo em {min} min"));
        }
    }
    if d.em_breve {
        motivos.push("Em breve".to_string());
    }
    match zona_da_missao(d.id) {
        Some(z) if Some(z) != c.zona => motivos.push(format!("Na ilha {}", nome_da_zona(z))),
        None => motivos.push("Indisponível nesta versão".to_string()),
        _ => {}
    }
    if d.min_level > c.nivel {
        motivos.push(format!("Requer nível {}", d.min_level));
    }
    if d.requires != 0 && !c.entregues.contains_key(&d.requires) {
        let t = shared::quests::quest_by_id(d.requires).map_or("?", |r| r.title);
        motivos.push(format!("Conclua: {t}"));
    }
    if d.faction != faction_id::NONE && d.faction != c.faccao {
        motivos.push(format!("Só para {}", nome_da_faccao(d.faction)));
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
        // TRAVA DE NIVEL: o "Ir" leva pra Ilha Magica em vez de dizer que
        // nao ha' pra onde ir.
        Estado::EmAndamento { .. }
            if d.obj_kind == shared::quests::objective_kind::NIVEL =>
        {
            Clique::IlhaMagica
        }
        Estado::EmAndamento { .. } | Estado::Pronta => Clique::AutoMissao(d.id),
        Estado::Concluida => Clique::Aviso(format!("\"{}\" já foi concluída.", d.title)),
        Estado::Bloqueada(m) => {
            Clique::Aviso(format!("\"{}\" bloqueada: {}.", d.title, m.join(" · ")))
        }
    }
}

/// Quanto falta pro reset das diarias (meia-noite UTC), "5h 07min".
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
            motivos.push(format!("Requer nível {}", p.obj_count));
            break;
        }
    }
    if let Some(p) = i
        .checked_sub(1)
        .and_then(historia::id_do_passo)
        .and_then(historia::def_da_historia)
    {
        motivos.push(format!("Conclua: {}", p.title));
    }
    if let Some(z) = zona_da_missao(d.id).filter(|z| Some(*z) != c.zona) {
        motivos.push(format!("Na ilha {}", nome_da_zona(z)));
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
        501 => "Os primeiros dias",
        511 => "Os chefes do Bosque",
        516 => "Bestiário do Bosque",
        523 => "A oficina",
        528 => "Mãos na terra",
        531 => "Porões e adegas",
        534 => "Gente da vila",
        _ => return None,
    })
}

/// As linhas do menu: a historia primeiro, depois as cadeias, cada uma com
/// os passos na ordem.
pub fn linhas(log: &[QuestNet]) -> Vec<Linha> {
    let lista = lista_do_menu(log);
    let mut v = Vec::new();
    let hist: Vec<&'static QuestDef> = lista
        .iter()
        .copied()
        .filter(|d| historia::e_da_historia(d.id))
        .collect();
    if !hist.is_empty() {
        v.push(Linha {
            nome: "História".into(),
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
            Aba::EmAndamento => "Em andamento",
            Aba::Disponiveis => "Disponíveis",
            Aba::Bloqueadas => "Bloqueadas",
            Aba::Concluidas => "Concluídas",
        }
    }
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
            Some(q) => format!("Disponível · pegar com: {q}"),
            None => "Disponível".into(),
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
                "Nível {feito}/{total} · a Ilha Mágica dá XP em DOBRO ({} entradas grátis por dia)",
                shared::magica::GRATIS_POR_DIA
            )
        }
        Estado::EmAndamento { feito, total } => format!("Em andamento · {feito}/{total}"),
        Estado::Pronta => match shared::quests::quem_da(d) {
            Some(q) => format!("Pronta · entregar: {q}"),
            None => "Pronta pra entregar".into(),
        },
        Estado::Concluida => "Concluída".into(),
        Estado::Bloqueada(m) => m.join(" · "),
    }
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
            draw_line(c.x - 2.0 * s, c.y + 6.0 * s, c.x + 8.0 * s, c.y - 7.0 * s, 3.0, cor);
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
pub(crate) fn rodape_da_fila(p: Rect, f: f32, tem_marca: bool) -> Option<Rect> {
    tem_marca.then(|| Rect::new(p.x + 18.0, p.y + p.h - 52.0 * f, p.w - 36.0, 40.0 * f))
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
    /// Linha aberta (mostra os passos), pelo nome.
    expandida: Option<String>,
    /// As missoes MARCADAS pra fila, na ordem em que foram marcadas.
    ///
    /// Um `Vec` e nao um conjunto porque a ORDEM e' a escolha: marcar e'
    /// dizer "esta depois daquela". Teto de `FILA_MAX`.
    marcadas: Vec<u16>,
}

/// Quantas missoes cabem numa fila. O numero e' do dono.
pub const FILA_MAX: usize = 10;

impl MenuMissoes {
    /// Abre pelo rodape do rastreador ou pelo Menu — nunca por tecla.
    pub fn abrir(&mut self) {
        self.aberto = true;
        self.aba = None;
        self.expandida = None;
        self.rolagem.zera();
        // As marcas NAO sobrevivem ao fechar: uma fila montada ontem e
        // iniciada sem querer hoje e' pior que remontar.
        self.marcadas.clear();
    }

    /// Marca ou desmarca uma missao pra fila. Devolve `false` quando a fila
    /// esta' cheia e a marca foi recusada.
    fn alterna_marca(&mut self, id: u16) -> bool {
        if let Some(i) = self.marcadas.iter().position(|x| *x == id) {
            self.marcadas.remove(i);
            return true;
        }
        if self.marcadas.len() >= FILA_MAX {
            return false;
        }
        self.marcadas.push(id);
        true
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
        Rect::new((screen_width() - w) * 0.5, (screen_height() - h) * 0.5, w, h)
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
        let f = estilo::fator_texto();
        let p = Self::painel();
        estilo::painel(p);
        estilo::texto(p.x + 18.0, p.y + 32.0 * f, "Missões", 22, estilo::OURO);
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
        let conta = |a: Aba| todas.iter().filter(|(_, r)| aba_de(r) == a).count();
        let aba = *self.aba.get_or_insert(if conta(Aba::EmAndamento) > 0 {
            Aba::EmAndamento
        } else {
            Aba::Disponiveis
        });
        let ya = p.y + 44.0 * f;
        let wa = (p.w - 20.0 - 3.0 * 6.0) / 4.0;
        for (k, a) in Aba::TODAS.iter().enumerate() {
            let r = Rect::new(p.x + 10.0 + k as f32 * (wa + 6.0), ya, wa, 34.0 * f);
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
        let topo_lista = ya + 42.0 * f;
        let fim_lista = rodape.map_or(p.y + p.h - 10.0, |r| r.y - 8.0 * f);
        let area = Rect::new(p.x + 10.0, topo_lista, p.w - 20.0, (fim_lista - topo_lista).max(40.0));
        let da_aba: Vec<&(Linha, Resumo)> = todas.iter().filter(|(_, r)| aba_de(r) == aba).collect();
        let altura = |l: &Linha| {
            CARTAO * f
                + if self.expandida.as_deref() == Some(l.nome.as_str()) {
                    l.passos.len() as f32 * PASSO * f + 8.0
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
            let vazio = match aba {
                Aba::EmAndamento => "Nada em andamento. Veja as Disponíveis.",
                Aba::Disponiveis => "Nada pra pegar agora.",
                Aba::Bloqueadas => "Nenhuma linha travada.",
                Aba::Concluidas => "Nenhuma linha concluída ainda.",
            };
            estilo::texto(area.x + 8.0, area.y + 26.0 * f, vazio, 15, estilo::SUAVE);
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
            estilo::ret_arredondado(card, 8.0, Color::new(1.0, 1.0, 1.0, if sobre { 0.08 } else { 0.04 }));
            icone(vec2(card.x + 24.0 * f, card.y + 28.0 * f), &r.estado, f);
            // As MESMAS medidas que o teste confere. Duas contas pro mesmo
            // lugar é como o desenho e a conta se separam sem ninguém ver.
            let (ir, cx, esquerda, largura) = caixas_do_cartao(card, f);
            let tx = card.x + esquerda;
            estilo::texto_ajustado(&l.nome, tx, card.y + 22.0 * f, largura, 17, estilo::TEXTO);
            let passo = match r.atual {
                Some(d) if l.historia => format!(
                    "{} · {}",
                    historia::indice(d.id).map_or(String::new(), |i| historia::nome_do_capitulo(i).to_string()),
                    d.title
                ),
                Some(d) => format!("Passo {} de {} · {}", r.feitos + 1, r.total, d.title),
                None => format!("{} de {} passos", r.total, r.total),
            };
            estilo::texto_ajustado(&passo, tx, card.y + 43.0 * f, largura, 13, estilo::SUAVE);
            if let Some(d) = r.atual {
                estilo::texto_ajustado(&frase(d, &r.estado), tx, card.y + 63.0 * f, largura, 13, cor_do_estado(&r.estado));
                // O QUE ELA PAGA. Em verde e por último: é o motivo de fazer,
                // e vem depois do que ela pede, que é o custo.
                let premio = recompensa_de(d, c.nomes);
                if !premio.is_empty() {
                    estilo::texto_ajustado(
                        &format!("Dá: {premio}"),
                        tx,
                        card.y + 83.0 * f,
                        largura,
                        13,
                        estilo::AUTO,
                    );
                }
            }
            // "Ir" no passo atual, quando da' pra fazer algo com ele.

            let clicavel = matches!(r.estado, Estado::Disponivel | Estado::EmAndamento { .. } | Estado::Pronta);
            // A CAIXA DA FILA, logo abaixo do "Ir".
            //
            // Marcar é dizer "esta, e nesta ordem" — por isso ela mostra o
            // NÚMERO da posição e não um tique: numa fila de dez, saber que
            // algo está marcado sem saber onde não ajuda a montar nada.

            let mut marcou = None;
            if let (true, Some(d)) = (clicavel, r.atual) {
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
                // "Pegar" quando ela ainda não foi aceita: o botão diz o que
                // vai acontecer, e o que acontece agora é aceitar na hora.
                let rotulo = if matches!(r.estado, Estado::Disponivel) {
                    "Pegar"
                } else {
                    "Ir"
                };
                let _ = crate::ui::botao(ir, rotulo, true);
                if tocou(ir) {
                    saida = Some(clique_de(d, &r.estado));
                }
            }
            if tocou(topo) && !(clicavel && (tocou(ir) || tocou(cx))) {
                alternar = Some(l.nome.clone());
            }
            // Aberta: os passos, com o que ja' foi, o atual e o que falta.
            if self.expandida.as_deref() == Some(l.nome.as_str()) {
                let mut py = card.y + CARTAO * f;
                for (d, e) in l.passos.iter().zip(&r.estados) {
                    let atual = r.atual.is_some_and(|a| a.id == d.id);
                    let cor = if atual { estilo::OURO } else { cor_do_estado(e) };
                    icone(vec2(tx + 6.0 * f, py + PASSO * f * 0.5), e, f * 0.7);
                    let t = if atual { format!("› {}", d.title) } else { d.title.to_string() };
                    estilo::texto_ajustado(&t, tx + 22.0 * f, py + PASSO * f * 0.68, (largura - 22.0 * f).max(24.0), 13, cor);
                    py += PASSO * f;
                }
            }
        }
        crate::rolagem::recortar(None);
        self.rolagem.desenha(area, total);
        if let Some(id) = marca_pedida {
            if !self.alterna_marca(id) {
                saida = Some(Clique::Aviso(format!("A fila já tem {FILA_MAX} missões.")));
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
            let b = Rect::new(rod.x + rod.w - 190.0 * f, rod.y, 120.0 * f, rod.h);
            let limpar = Rect::new(rod.x + rod.w - 64.0 * f, rod.y, 64.0 * f, rod.h);
            estilo::texto(
                rod.x,
                rod.y + 26.0 * f,
                &format!("Fila: {n} de {FILA_MAX} · bloqueada é pulada"),
                14,
                estilo::SUAVE,
            );
            if crate::ui::botao(b, &format!("Fazer as {n}"), true) {
                saida = Some(Clique::Fila(std::mem::take(&mut self.marcadas)));
            }
            if crate::ui::botao(limpar, "Limpar", true) {
                self.marcadas.clear();
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
            Estado::Bloqueada(vec!["Conclua: Conheça o Alquimista".into()])
        );
        // 504: nivel 3 e a 503 antes.
        match estado(d504, &c) {
            Estado::Bloqueada(m) => {
                assert!(m.contains(&"Requer nível 3".to_string()), "{m:?}");
                assert!(m.iter().any(|s| s.starts_with("Conclua: ")), "{m:?}");
            }
            outro => panic!("{outro:?}"),
        }
        // Outra ilha: bloqueia dizendo onde.
        let fora = ctx(&[], &vazio, 1, Some("ilha_gelo"));
        assert_eq!(
            estado(d501, &fora),
            Estado::Bloqueada(vec!["Na ilha Bosque".into()])
        );
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
        match clique_de(d501, &Estado::Bloqueada(vec!["Requer nível 3".into()])) {
            Clique::Aviso(s) => assert!(s.contains("Requer nível 3"), "{s}"),
            outro => panic!("bloqueada andou: {outro:?}"),
        }
        assert!(matches!(
            clique_de(d501, &Estado::Concluida),
            Clique::Aviso(_)
        ));
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
        let esperado: Vec<u16> = (501..=543).collect();
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
                assert!(!m.iter().any(|s| s.starts_with("Requer nível")), "{m:?}");
                assert!(m.iter().any(|s| s.starts_with("Conclua: ")), "{m:?}");
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
            Estado::Bloqueada(m) => assert!(m.iter().any(|s| s == "Na ilha Geleira"), "{m:?}"),
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
        let nomes: Vec<&str> = ls.iter().filter(|l| !l.historia).map(|l| l.nome.as_str()).collect();
        for n in ["Os primeiros dias", "Os chefes do Bosque", "Bestiário do Bosque", "A oficina", "Mãos na terra", "Porões e adegas", "Gente da vila"] {
            assert!(nomes.contains(&n), "{n} fora do menu: {nomes:?}");
        }
        let chefes = ls.iter().find(|l| l.nome == "Os chefes do Bosque").unwrap();
        assert_eq!(chefes.passos.iter().map(|d| d.id).collect::<Vec<_>>(), vec![511, 512, 513, 514, 515]);

        // Nivel 16, a 511 entregue: a linha dos chefes esta' no passo 2 e e'
        // do Capitao do Porto.
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
        assert!(frase(r.atual.unwrap(), &r.estado).contains("Capitao do Porto"));

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
        assert_eq!((aba_de(&fim), fim.atual.map(|d| d.id)), (Aba::Concluidas, None));
        // Nivel baixo: a primeira da linha trava e ela vai pra Bloqueadas.
        let nenhuma = HashMap::new();
        let c4 = ctx(&[], &nenhuma, 1, Some("ilha_inicial"));
        assert_eq!(aba_de(&resumo(chefes, &c4)), Aba::Bloqueadas);
    }

    /// Diaria de sistema que ainda nao existe: cadeado com "Em breve"; reset
    /// conta ate' a meia-noite UTC.
    #[test]
    fn diaria_em_breve_bloqueia_e_o_reset_conta_ate_a_meia_noite() {
        let vazio = HashMap::new();
        let c = ctx(&[], &vazio, 50, Some("ilha_inicial"));
        assert_eq!(
            estado(quest_by_id(607).unwrap(), &c),
            Estado::Bloqueada(vec!["Em breve".into()])
        );
        assert_eq!(estado(quest_by_id(601).unwrap(), &c), Estado::Disponivel);
        assert_eq!(reset_em(86_400 * 10 + 3_600 * 19 + 60 * 53), "4h 07min");
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
                    assert_eq!(
                        rod.is_some(),
                        tem_marca,
                        "o rodapé só existe com missão marcada"
                    );
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
    fn a_trava_de_nivel_aponta_a_ilha_magica() {
        let d = shared::historia::PASSOS
            .iter()
            .find(|d| d.obj_kind == shared::quests::objective_kind::NIVEL)
            .expect("a história tem trava de nível");
        let andando = Estado::EmAndamento {
            feito: 17,
            total: 20,
        };
        assert_eq!(clique_de(d, &andando), Clique::IlhaMagica);

        // E a frase diz o que fazer, não só o quanto falta.
        let f = frase(d, &andando);
        assert!(f.contains("17/20"), "{f}");
        assert!(f.contains("Ilha Mágica"), "{f}");
        assert!(f.contains("DOBRO"), "{f}");

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
