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
    /// Disponivel aqui: ir ate' quem da' e abrir a oferta.
    IrAoGiver(u16),
    /// Em andamento ou pronta: a auto missao que ja' existe.
    AutoMissao(u16),
    /// Nao anda: so' avisa.
    Aviso(String),
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
        Estado::Disponivel => Clique::IrAoGiver(d.id),
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
        Estado::EmAndamento { .. } if d.obj_kind == shared::quests::objective_kind::TUTORIAL => {
            format!("Tutorial · {}", shared::quests::tutorial::instrucao(d.obj_target))
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

const CARTAO: f32 = 76.0;
const PASSO: f32 = 26.0;

#[derive(Default)]
pub struct MenuMissoes {
    pub aberto: bool,
    rolagem: crate::rolagem::Rolagem,
    /// `None` = escolher na abertura (em andamento, ou disponiveis).
    aba: Option<Aba>,
    /// Linha aberta (mostra os passos), pelo nome.
    expandida: Option<String>,
}

impl MenuMissoes {
    /// Abre pelo rodape do rastreador ou pelo Menu — nunca por tecla.
    pub fn abrir(&mut self) {
        self.aberto = true;
        self.aba = None;
        self.expandida = None;
        self.rolagem.zera();
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
        let area = Rect::new(p.x + 10.0, ya + 42.0 * f, p.w - 20.0, p.y + p.h - (ya + 42.0 * f) - 10.0);
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
            let tx = card.x + 48.0 * f;
            let largura = card.w - 48.0 * f - 90.0 * f;
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
                estilo::texto_ajustado(&frase(d, &r.estado), tx, card.y + 63.0 * f, card.w - 60.0 * f, 13, cor_do_estado(&r.estado));
            }
            // "Ir" no passo atual, quando da' pra fazer algo com ele.
            let ir = Rect::new(card.x + card.w - 82.0 * f, card.y + 12.0 * f, 72.0 * f, 30.0 * f);
            let clicavel = matches!(r.estado, Estado::Disponivel | Estado::EmAndamento { .. } | Estado::Pronta);
            if let (true, Some(d)) = (clicavel, r.atual) {
                let _ = crate::ui::botao(ir, "Ir", true);
                if tocou(ir) {
                    saida = Some(clique_de(d, &r.estado));
                }
            }
            if tocou(topo) && !(clicavel && tocou(ir)) {
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
                    estilo::texto_ajustado(&t, tx + 22.0 * f, py + PASSO * f * 0.68, card.w - 90.0 * f, 13, cor);
                    py += PASSO * f;
                }
            }
        }
        crate::rolagem::recortar(None);
        self.rolagem.desenha(area, total);
        if let Some(nome) = alternar {
            self.expandida = if self.expandida.as_deref() == Some(nome.as_str()) {
                None
            } else {
                Some(nome)
            };
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
        assert_eq!(clique_de(d501, &Estado::Disponivel), Clique::IrAoGiver(501));
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
        // coleta, dungeons, vila).
        let esperado: Vec<u16> = (501..=538).collect();
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
        // Outra ilha no futuro: diz onde.
        match estado_da_historia(historia::def_da_historia(719).unwrap(), &c) {
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
