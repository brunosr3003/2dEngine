//! Mapa da ilha (M), minimapa e viagem por clique.
//!
//! A imagem sai da SEMENTE, pelo mesmo `Gerador` que desenha o chao — nada
//! viaja pela rede. Ela e' amostrada numa grade grossa numa thread: a ilha
//! grande tem dez milhoes de colunas, e o quadro nao pode esperar por isso.
//!
//! Eixos: o mundo e' (x, z), e a camera em yaw 0 olha pra -z. O mapa usa norte
//! fixo com x pra direita e z pra BAIXO — a vista da camera sem giro —, entao
//! o que esta' a' esquerda no mundo esta' a' esquerda no mapa.
use crate::hud_estilo::{self as estilo, u};
use crate::world::World;
use macroquad::prelude::*;
use shared::terreno::{Bioma, Cidade, DefIlha, Gerador, BLOCO, ESCALA_ALTURA, NIVEL_DO_MAR};
use shared::EntityTag;
use std::sync::mpsc::{channel, Receiver};

/// Pixels da imagem por lado. 384 sobre 1,6 km da uns 4 m por pixel: le'
/// costa, rio de pedra e a praca da cidade, e gera em fracao de segundo.
use shared::protocol::{RegiaoNoMapa, ZonaNoMapa};
use std::collections::{HashMap, HashSet};

use crate::ir_para::{Alvo, Objetivo};

/// O que chega do servidor em `MapaDaIlha`, arrumado pro desenho.
#[derive(Debug, Default, Clone)]
pub struct InfoDaIlha {
    pub zonas: Vec<ZonaNoMapa>,
    pub recursos: Vec<RegiaoNoMapa>,
    pub nomes: HashMap<u16, String>,
    pub rendimentos: HashMap<u8, String>,
    /// Chefes de campo (coroa no mapa, sempre visivel).
    pub chefes: Vec<shared::bosses::ChefeNoMapa>,
}

/// Chance (em %) pra zona contar como "onde o bicho nasce": a mesma do
/// servidor (`quests::zona_do_bicho`).
const CHANCE_MINIMA_PCT: u8 = 15;

impl InfoDaIlha {
    pub fn nome(&self, kind: u16) -> String {
        self.nomes
            .get(&kind)
            .cloned()
            .unwrap_or_else(|| format!("Bicho {kind}"))
    }

    /// Todo bicho que nasce em alguma zona da ilha, do mais baixo pro mais alto.
    pub fn bichos(&self) -> Vec<u16> {
        let mut v: Vec<u16> = self
            .zonas
            .iter()
            .flat_map(|z| z.bichos.iter().map(|b| b.0))
            .collect();
        v.sort_unstable();
        v.dedup();
        v.sort_by_key(|k| self.faixa(*k).map_or(u16::MAX, |f| f.0));
        v
    }

    /// Faixa de nivel das zonas onde o bicho e' o dominante ou sai com chance
    /// razoavel; sem nenhuma assim, a da zona onde ele mais sai.
    pub fn faixa(&self, kind: u16) -> Option<(u16, u16)> {
        let boas: Vec<&ZonaNoMapa> = self
            .zonas
            .iter()
            .filter(|z| {
                chance_na_zona(z, kind) >= CHANCE_MINIMA_PCT
                    || z.bichos.first().is_some_and(|b| b.0 == kind)
            })
            .collect();
        if boas.is_empty() {
            let z = self
                .zonas
                .iter()
                .filter(|z| chance_na_zona(z, kind) > 0)
                .max_by_key(|z| chance_na_zona(z, kind))?;
            return Some((z.lv_min, z.lv_max));
        }
        Some((
            boas.iter().map(|z| z.lv_min).min()?,
            boas.iter().map(|z| z.lv_max).max()?,
        ))
    }

    /// Tipos de recurso que a ilha tem.
    pub fn tipos(&self) -> Vec<u8> {
        let mut v: Vec<u8> = self.recursos.iter().map(|r| r.tipo).collect();
        v.sort_unstable();
        v.dedup();
        v
    }
}

pub fn chance_na_zona(z: &ZonaNoMapa, kind: u16) -> u8 {
    z.bichos.iter().find(|b| b.0 == kind).map_or(0, |b| b.1)
}

/// A torre do FORTE: um bloco com ameias e uma haste de bandeira. Desenhada
/// a mao porque nenhum icone de `icones_ui` diz "posicao militar" — e o que
/// marca o forte tem que se separar do circulo do bicho a` primeira olhada.
fn torre(c: Vec2, lado: f32, cor: Color) {
    let preto = Color::new(0.0, 0.0, 0.0, 0.75);
    let corpo = Rect::new(c.x - lado * 0.5, c.y - lado * 0.35, lado, lado * 0.85);
    draw_rectangle(
        corpo.x - 1.0,
        corpo.y - 1.0,
        corpo.w + 2.0,
        corpo.h + 2.0,
        preto,
    );
    draw_rectangle(corpo.x, corpo.y, corpo.w, corpo.h, cor);
    // Ameias: tres dentes em cima, que e' o que le' como muralha.
    let dente = lado / 3.0;
    for k in 0..3 {
        let x = corpo.x + k as f32 * dente;
        draw_rectangle(x - 1.0, corpo.y - dente - 1.0, dente * 0.66 + 2.0, dente + 2.0, preto);
        draw_rectangle(x, corpo.y - dente, dente * 0.66, dente, cor);
    }
}

fn centro_da_zona(z: &ZonaNoMapa) -> Vec2 {
    vec2(z.centro[0], z.centro[1])
}

/// A zona pra ir caçar `kind` — a mesma escolha de `quests::zona_do_bicho` do
/// servidor: a mais perto em que ele sai com chance razoavel e cujo nivel o
/// jogador aguenta; senao a mais perto com chance; senao a de maior chance.
pub fn zona_mais_perto(
    zonas: &[ZonaNoMapa],
    kind: u16,
    eu: Vec2,
    nivel: u32,
) -> Option<&ZonaNoMapa> {
    fn perto<'a>(v: Vec<&'a ZonaNoMapa>, eu: Vec2) -> Option<&'a ZonaNoMapa> {
        v.into_iter().min_by(|a, b| {
            centro_da_zona(a)
                .distance_squared(eu)
                .total_cmp(&centro_da_zona(b).distance_squared(eu))
        })
    }
    let boas: Vec<&ZonaNoMapa> = zonas
        .iter()
        .filter(|z| chance_na_zona(z, kind) >= CHANCE_MINIMA_PCT && z.lv_min as u32 <= nivel + 3)
        .collect();
    if let Some(z) = perto(boas, eu) {
        return Some(z);
    }
    let com: Vec<&ZonaNoMapa> = zonas
        .iter()
        .filter(|z| chance_na_zona(z, kind) >= CHANCE_MINIMA_PCT)
        .collect();
    if let Some(z) = perto(com, eu) {
        return Some(z);
    }
    zonas
        .iter()
        .filter(|z| chance_na_zona(z, kind) > 0)
        .max_by(|a, b| {
            chance_na_zona(a, kind).cmp(&chance_na_zona(b, kind)).then(
                centro_da_zona(b)
                    .distance_squared(eu)
                    .total_cmp(&centro_da_zona(a).distance_squared(eu)),
            )
        })
}

/// A regiao do tipo mais perto do jogador.
pub fn regiao_mais_perto(recursos: &[RegiaoNoMapa], tipo: u8, eu: Vec2) -> Option<&RegiaoNoMapa> {
    recursos.iter().filter(|r| r.tipo == tipo).min_by(|a, b| {
        vec2(a.centro[0], a.centro[1])
            .distance_squared(eu)
            .total_cmp(&vec2(b.centro[0], b.centro[1]).distance_squared(eu))
    })
}

pub fn nome_do_tipo(t: u8) -> &'static str {
    match t {
        0 => "Madeira",
        1 => "Pedra cinza",
        2 => "Pedra verde",
        3 => "Pedra azul",
        4 => "Pedra roxa",
        5 => "Energia",
        _ => "Recurso",
    }
}

fn cor_do_tipo(t: u8) -> Color {
    match t {
        0 => Color::new(0.55, 0.78, 0.35, 1.0),
        1 => Color::new(0.78, 0.78, 0.78, 1.0),
        2 => Color::new(0.35, 0.90, 0.45, 1.0),
        3 => Color::new(0.35, 0.60, 1.0, 1.0),
        4 => Color::new(0.78, 0.45, 1.0, 1.0),
        5 => Color::new(0.25, 0.82, 1.0, 1.0),
        _ => WHITE,
    }
}

/// Uma cor por bicho, estavel pelo kind.
fn cor_do_bicho(kind: u16) -> Color {
    const CORES: [Color; 8] = [
        Color::new(0.95, 0.35, 0.28, 1.0),
        Color::new(0.95, 0.62, 0.22, 1.0),
        Color::new(0.93, 0.85, 0.30, 1.0),
        Color::new(0.85, 0.35, 0.70, 1.0),
        Color::new(0.55, 0.40, 0.95, 1.0),
        Color::new(0.35, 0.80, 0.85, 1.0),
        Color::new(0.75, 0.55, 0.40, 1.0),
        Color::new(1.0, 0.45, 0.55, 1.0),
    ];
    match kind {
        // os de praia: coral e verde-agua, como o casco de cada um
        8 => Color::new(1.0, 0.55, 0.35, 1.0),
        9 => Color::new(0.30, 0.78, 0.82, 1.0),
        _ => CORES[kind as usize % CORES.len()],
    }
}

/// Filtros do mapa grande e do minimapa. Tudo DESLIGADO de inicio — o mapa
/// abre limpo e o jogador liga o que quer achar; o `main`
/// guarda a escolha quando o mapa e' recriado (troca de zona), entao ela vale
/// a sessao inteira.
#[derive(Debug, Clone, PartialEq)]
pub struct Filtros {
    pub mobs: bool,
    pub bichos_ocultos: HashSet<u16>,
    /// Por tipo: 0 madeira, 1..4 pedra pela cor.
    pub recursos: [bool; 5],
    pub energia: bool,
    /// Cidade, porto, predios e NPCs.
    pub vila: bool,
}

impl Default for Filtros {
    fn default() -> Self {
        Self {
            mobs: false,
            bichos_ocultos: HashSet::new(),
            recursos: [false; 5],
            energia: false,
            vila: false,
        }
    }
}

impl From<&shared::protocol::FiltrosDoMapa> for Filtros {
    fn from(f: &shared::protocol::FiltrosDoMapa) -> Self {
        Self {
            mobs: f.mobs,
            bichos_ocultos: f.bichos_ocultos.iter().copied().collect(),
            recursos: f.recursos,
            energia: f.energia,
            vila: f.vila,
        }
    }
}

impl Filtros {
    /// Pra guardar no servidor. Lista ordenada: o mesmo conjunto sempre vira
    /// o mesmo valor, e a sincronia nao ve' "mudanca" que nao houve.
    pub fn para_rede(&self) -> shared::protocol::FiltrosDoMapa {
        let mut bichos: Vec<u16> = self.bichos_ocultos.iter().copied().collect();
        bichos.sort_unstable();
        shared::protocol::FiltrosDoMapa {
            mobs: self.mobs,
            bichos_ocultos: bichos,
            recursos: self.recursos,
            energia: self.energia,
            vila: self.vila,
        }
    }

    /// Zona aparece se mobs estao ligados e o bicho DOMINANTE dela nao foi
    /// escondido: esconder "Lobo" tira as zonas de lobo.
    pub fn zona_visivel(&self, z: &ZonaNoMapa) -> bool {
        self.mobs
            && z.bichos
                .first()
                .is_some_and(|b| !self.bichos_ocultos.contains(&b.0))
    }

    pub fn regiao_visivel(&self, r: &RegiaoNoMapa) -> bool {
        if r.tipo == 5 {
            self.energia
        } else {
            self.recursos.get(r.tipo as usize).copied().unwrap_or(false)
        }
    }
}

/// O que um clique no mapa pede ao `main`.
#[derive(Debug, Clone, PartialEq)]
pub enum Entrada {
    /// Ponto qualquer: viagem.
    Viajar(Vec2),
    /// Zona de bicho ou regiao de recurso (ou "Ir" do painel): ir e fazer.
    Ir(Alvo),
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Marcador {
    Zona(usize),
    Regiao(usize),
    /// NPC da vila (indice em `npcs_da_vila`).
    Npc(usize),
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Chip {
    Mobs,
    Bicho(u16),
    Recurso(u8),
    Vila,
}

/// Largura do painel de filtros e "Ir para" ao lado do mapa grande.
const LARGURA_LATERAL: f32 = 260.0;
const LINHA_IR: f32 = 26.0;

const LADO: usize = 384;

/// Teto de chamadas de desenho da rota 2D em um quadro. Uma rota recebida do
/// servidor pode ter 128 pontos; no celular, transformar cada trecho comprido
/// em dezenas de risquinhos custava mais que o resto do mapa.
const MAX_TRACOS_2D: usize = 256;

/// Recorta `a..b` no retangulo e devolve tambem a fracao do segmento que
/// sobrou. Liang-Barsky: alem de nao desenhar fora, evita ITERAR pelos tracos
/// invisiveis ate' chegar na tela.
fn recortar_segmento(a: Vec2, b: Vec2, r: Rect) -> Option<(Vec2, Vec2, f32, f32)> {
    let d = b - a;
    let mut entrada = 0.0f32;
    let mut saida = 1.0f32;
    for (p, q) in [
        (-d.x, a.x - r.x),
        (d.x, r.x + r.w - a.x),
        (-d.y, a.y - r.y),
        (d.y, r.y + r.h - a.y),
    ] {
        if p.abs() < 1e-6 {
            if q < 0.0 {
                return None;
            }
            continue;
        }
        let t = q / p;
        if p < 0.0 {
            entrada = entrada.max(t);
        } else {
            saida = saida.min(t);
        }
        if entrada > saida {
            return None;
        }
    }
    Some((a + d * entrada, a + d * saida, entrada, saida))
}

/// Visita so' os tracos que de fato podem aparecer. Separado do desenho para
/// testar o custo: um destino absurdamente longe continua gerando trabalho
/// proporcional ao tamanho da TELA, nao ao tamanho do caminho.
fn para_cada_traco(
    pontos: &[Vec2],
    traco: f32,
    vao: f32,
    recorte: Option<Rect>,
    limite: usize,
    mut visitar: impl FnMut(Vec2, Vec2),
) -> usize {
    let periodo = traco + vao;
    if periodo <= 1e-3 || traco <= 0.0 || limite == 0 {
        return 0;
    }
    let mut antes = 0.0f32;
    let mut feitos = 0usize;
    for par in pontos.windows(2) {
        let (a, b) = (par[0], par[1]);
        let comp = a.distance(b);
        if comp < 1e-3 || !comp.is_finite() {
            continue;
        }
        let dir = (b - a) / comp;
        let Some((vis_a, vis_b, t0, t1)) = recorte
            .map(|r| recortar_segmento(a, b, r))
            .unwrap_or(Some((a, b, 0.0, 1.0)))
        else {
            antes += comp;
            continue;
        };
        let inicio = t0 * comp;
        let fim = t1 * comp;
        let mut s = inicio;
        while s < fim - 1e-4 {
            let pos = (antes + s).rem_euclid(periodo);
            let ate_mudar = if pos < traco {
                traco - pos
            } else {
                periodo - pos
            };
            let passo = ate_mudar.max(1e-4).min(fim - s);
            if pos < traco {
                // Parte do ponto ja' recortado: evita perder precisao ao
                // somar um deslocamento enorme a uma origem muito distante.
                let mut p0 = vis_a + dir * (s - inicio);
                let mut p1 = if fim - (s + passo) < 1e-4 {
                    vis_b
                } else {
                    vis_a + dir * (s + passo - inicio)
                };
                // Com coordenadas muito distantes, f32 pode arredondar o
                // ponto recortado alguns centesimos para fora.
                if let Some(r) = recorte {
                    let prender =
                        |p: Vec2| vec2(p.x.clamp(r.x, r.x + r.w), p.y.clamp(r.y, r.y + r.h));
                    p0 = prender(p0);
                    p1 = prender(p1);
                }
                visitar(p0, p1);
                feitos += 1;
                if feitos >= limite {
                    return feitos;
                }
            }
            s += passo;
        }
        antes += comp;
    }
    feitos
}

/// Linha tracejada 2D pelos pontos (ja' em tela). Com `recorte`, so' os
/// tracos dentro dele. O recorte acontece ANTES de picotar a linha.
fn tracejado(
    pontos: &[Vec2],
    traco: f32,
    vao: f32,
    espessura: f32,
    cor: Color,
    recorte: Option<Rect>,
) {
    // Uma folga conserva a espessura na borda do recorte.
    let recorte = recorte.map(|r| {
        Rect::new(
            r.x - espessura,
            r.y - espessura,
            r.w + espessura * 2.0,
            r.h + espessura * 2.0,
        )
    });
    para_cada_traco(pontos, traco, vao, recorte, MAX_TRACOS_2D, |p0, p1| {
        draw_line(p0.x, p0.y, p1.x, p1.y, espessura, cor);
    });
}

// ─────────────────────────────── viagem ──────────────────────────────

/// Ate' aqui o destino vai direto. O servidor recusa rota acima de 220
/// (`handle_mover_para`); a folga cobre o A* dar a volta num morro.
pub const ALCANCE_DIRETO: f32 = 180.0;
/// Tamanho de cada etapa de uma viagem longa.
pub const ETAPA: f32 = 160.0;
/// Perto disto de uma etapa intermediaria, manda a proxima.
const CHEGOU_ETAPA: f32 = 6.0;
/// Perto disto do destino, a viagem acabou.
pub const CHEGOU: f32 = 2.0;
/// Parado por este tempo: a rota acabou antes (parcial, obstaculo). Pede de novo.
const PARADO_S: f64 = 1.5;
/// Nunca mais de um pedido nesse intervalo — o servidor ignora os de 0,2 s.
const INTERVALO_S: f64 = 0.35;
/// Pedidos seguidos parado sem chegar mais perto do destino: desiste.
const DESISTE_APOS: u32 = 6;

#[derive(Debug, PartialEq)]
pub enum Passo {
    Nada,
    Enviar(Vec2),
    Chegou,
    Desistiu,
}

/// Viagem por etapas. Sem macroquad: quem desenha e manda mensagem e' o
/// `main`, aqui so' se decide QUANDO pedir rota e PRA ONDE.
#[derive(Default)]
pub struct Viagem {
    destino: Option<Vec2>,
    etapa: Option<Vec2>,
    ultimo_envio: f64,
    ultima_pos: Option<Vec2>,
    parado_desde: f64,
    melhor: f32,
    sem_progresso: u32,
}

impl Viagem {
    pub fn iniciar(&mut self, destino: Vec2, agora: f64) {
        *self = Self {
            destino: Some(destino),
            ultimo_envio: f64::MIN,
            parado_desde: agora,
            melhor: f32::MAX,
            ..Default::default()
        };
    }

    pub fn cancelar(&mut self) {
        *self = Self::default();
    }

    pub fn ativa(&self) -> bool {
        self.destino.is_some()
    }

    pub fn destino(&self) -> Option<Vec2> {
        self.destino
    }

    /// O que fazer neste quadro. `terra` responde se um ponto e' chao firme.
    pub fn passo(&mut self, eu: Vec2, agora: f64, terra: impl Fn(Vec2) -> bool) -> Passo {
        let Some(destino) = self.destino else {
            return Passo::Nada;
        };
        if eu.distance(destino) <= CHEGOU {
            self.cancelar();
            return Passo::Chegou;
        }
        if self.ultima_pos.is_none_or(|u| u.distance(eu) > 0.3) {
            self.ultima_pos = Some(eu);
            self.parado_desde = agora;
        }
        // A ultima etapa E' o destino: chegar perto dela nao pede outra, quem
        // encerra e' o `CHEGOU` acima.
        let chegou_na_etapa = self
            .etapa
            .is_some_and(|e| e != destino && eu.distance(e) <= CHEGOU_ETAPA);
        let parado = self.etapa.is_some() && agora - self.parado_desde >= PARADO_S;
        if !(self.etapa.is_none() || chegou_na_etapa || parado) {
            return Passo::Nada;
        }
        if agora - self.ultimo_envio < INTERVALO_S {
            return Passo::Nada;
        }
        if parado {
            let d = eu.distance(destino);
            if d < self.melhor - 2.0 {
                self.melhor = d;
                self.sem_progresso = 0;
            } else {
                self.sem_progresso += 1;
                if self.sem_progresso >= DESISTE_APOS {
                    self.cancelar();
                    return Passo::Desistiu;
                }
            }
        }
        let p = proxima_etapa(eu, destino, &terra);
        self.etapa = Some(p);
        self.ultimo_envio = agora;
        self.parado_desde = agora;
        Passo::Enviar(p)
    }
}

/// O proximo ponto a pedir: o destino, se ele esta' ao alcance; senao um
/// ponto a `ETAPA` na direcao dele — em TERRA, recuando e abrindo pros lados
/// se a reta cair na agua. Etapa no mar e' rota que o servidor nao faz.
fn proxima_etapa(eu: Vec2, destino: Vec2, terra: &impl Fn(Vec2) -> bool) -> Vec2 {
    let d = destino - eu;
    let dist = d.length();
    if dist <= ALCANCE_DIRETO {
        return destino;
    }
    let dir = d / dist;
    let lado = vec2(-dir.y, dir.x);
    for k in 0..=10 {
        let recuo = ETAPA - k as f32 * 12.0;
        for desvio in [0.0, 1.0, -1.0, 2.0, -2.0] {
            let p = eu + dir * recuo + lado * desvio * 12.0;
            if terra(p) {
                return p;
            }
        }
    }
    eu + dir * ETAPA
}

// ─────────────────────────────── imagem ──────────────────────────────

fn misturar(a: [f32; 3], b: [f32; 3], t: f32) -> [f32; 3] {
    let t = t.clamp(0.0, 1.0);
    [
        a[0] + (b[0] - a[0]) * t,
        a[1] + (b[1] - a[1]) * t,
        a[2] + (b[2] - a[2]) * t,
    ]
}

fn cor_da_agua(h: f32) -> [f32; 3] {
    misturar([0.33, 0.62, 0.72], [0.07, 0.20, 0.36], -h / 6.0)
}

fn cor_de_terra(bioma: Bioma, h: f32, pico: f32) -> [f32; 3] {
    if h < 0.8 {
        return [0.80, 0.74, 0.52]; // praia
    }
    // Duas cores por bioma, do vale ao alto. Antes era UMA cor chapada pra tudo
    // abaixo de 45% do pico — ou seja, quase a ilha inteira saia do mesmo verde
    // e o minimapa virava uma mancha. Agora a altura se le' na cor, nao so' no
    // sombreado.
    let (vale, alto) = match bioma {
        Bioma::Floresta => ([0.22, 0.42, 0.20], [0.52, 0.64, 0.33]),
        Bioma::Gelo => ([0.62, 0.72, 0.80], [0.88, 0.92, 0.95]),
        Bioma::Deserto => ([0.72, 0.56, 0.32], [0.89, 0.78, 0.55]),
        Bioma::Montanha => ([0.33, 0.42, 0.28], [0.58, 0.62, 0.45]),
    };
    let (rocha, neve) = ([0.50, 0.48, 0.45], [0.93, 0.95, 0.97]);
    let t = (h / pico).clamp(0.0, 1.0);
    if t > 0.75 {
        misturar(rocha, neve, (t - 0.75) / 0.25)
    } else if t > 0.45 {
        misturar(misturar(vale, alto, 1.0), rocha, (t - 0.45) / 0.30)
    } else {
        misturar(vale, alto, t / 0.45)
    }
}

/// O que a thread do mapa entrega: a imagem e o desenho da vila por cima.
struct Dados {
    rgba: Vec<u8>,
    pegadas: Vec<(Vec2, Vec2)>,
    porto: Option<PortoNoMapa>,
    /// Onde fica o Mestre de Missoes (pro "Ir" do menu de missoes).
    mestre: Option<Vec2>,
    /// Os NPCs da vila (nome, posicao), montados junto — fora do quadro.
    npcs: Vec<(String, Vec2)>,
}

#[derive(Clone, Copy)]
struct PortoNoMapa {
    centro: Vec2,
    raio: f32,
    raiz: Vec2,
    ponta: Vec2,
}

const COR_PREDIO: Color = Color::new(0.62, 0.40, 0.26, 1.0);
const COR_PORTO: Color = Color::new(0.55, 0.82, 0.95, 1.0);

/// Imagem mais pegadas dos predios e o porto. Tudo FORA do quadro: a vila
/// gera o voxel de cada predio pra saber o tamanho dele.
fn gerar_dados(def: &'static DefIlha) -> Dados {
    let rgba = gerar_imagem(def);
    let ger = Gerador::da_ilha(def);
    let vila = ger.vila();
    let pegadas = vila
        .predios
        .iter()
        .filter(|p| p.tipo != shared::construcao::TipoCasa::Doca)
        .map(|p| {
            let m = p.construcao().meia(p.yaw_q);
            (vec2(p.pos.x, p.pos.z), vec2(m.x, m.y))
        })
        .collect();
    let porto = vila.porto.map(|p| PortoNoMapa {
        centro: vec2(p.centro.x, p.centro.y),
        raio: p.raio,
        raiz: vec2(p.raiz.x, p.raiz.y),
        ponta: vec2(p.ponta.x, p.ponta.y),
    });
    let mestre = vila
        .npcs
        .iter()
        .find(|n| matches!(n.papel, shared::construcao::Papel::Missoes))
        .map(|n| vec2(n.pos.x, n.pos.y));
    let npcs = vila
        .npcs
        .iter()
        .map(|n| (n.nome.to_string(), vec2(n.pos.x, n.pos.y)))
        .collect();
    Dados {
        rgba,
        pegadas,
        porto,
        mestre,
        npcs,
    }
}

/// RGBA da ilha inteira. Roda FORA do quadro.
fn gerar_imagem(def: &'static DefIlha) -> Vec<u8> {
    let ger = Gerador::da_ilha(def);
    let raio = def.raio_blocos as f32 * BLOCO;
    let pico = ger.pico().max(1.0);
    let mut hs = vec![0f32; LADO * LADO];
    for j in 0..LADO {
        let z = -raio + (j as f32 + 0.5) / LADO as f32 * 2.0 * raio;
        for i in 0..LADO {
            let x = -raio + (i as f32 + 0.5) / LADO as f32 * 2.0 * raio;
            hs[j * LADO + i] = ger.altura(x, z);
        }
    }
    let mut rgba = vec![255u8; LADO * LADO * 4];
    for j in 0..LADO {
        for i in 0..LADO {
            let h = hs[j * LADO + i];
            let c = if h <= NIVEL_DO_MAR {
                cor_da_agua(h)
            } else {
                // Luz de noroeste: o vizinho de cima-esquerda mais baixo
                // clareia, mais alto sombreia. E' o que faz o relevo ler — por
                // isso bate mais forte do que batia (era 0,18 e quase nao
                // aparecia).
                let viz = hs[j.saturating_sub(1) * LADO + i.saturating_sub(1)];
                let luz = (1.0 + (h - viz) * 0.42).clamp(0.55, 1.45);
                let cor = cor_de_terra(def.bioma, h, pico).map(|v| v * luz);
                // Contorno de costa: a primeira faixa acima do mar escurece, pra
                // a ilha ter silhueta em vez de desbotar na agua.
                let costa = ((h - NIVEL_DO_MAR) / 0.6).clamp(0.0, 1.0);
                misturar([0.16, 0.26, 0.30], cor, costa)
            };
            let k = (j * LADO + i) * 4;
            for (n, v) in c.iter().enumerate() {
                rgba[k + n] = (v.clamp(0.0, 1.0) * 255.0) as u8;
            }
        }
    }
    rgba
}

/// Ponto do mundo -> pixel do retangulo que mostra a ilha inteira.
fn para_tela(p: Vec2, r: Rect, raio: f32) -> Vec2 {
    vec2(
        r.x + (p.x + raio) / (2.0 * raio) * r.w,
        r.y + (p.y + raio) / (2.0 * raio) * r.h,
    )
}

/// Pixel do retangulo da ilha inteira -> ponto do mundo.
fn de_tela(t: Vec2, r: Rect, raio: f32) -> Vec2 {
    vec2(
        (t.x - r.x) / r.w * 2.0 * raio - raio,
        (t.y - r.y) / r.h * 2.0 * raio - raio,
    )
}

// ─────────────────────────────── mapa ────────────────────────────────

const COR_MOB: Color = Color::new(0.95, 0.30, 0.25, 1.0);
const COR_NPC: Color = Color::new(1.0, 0.85, 0.25, 1.0);
const COR_GENTE: Color = Color::new(0.45, 0.75, 1.0, 1.0);
const COR_AGUA: Color = Color::new(0.07, 0.20, 0.36, 1.0);

pub struct Mapa {
    def: Option<&'static DefIlha>,
    ger: Option<Gerador>,
    cidade: Option<Cidade>,
    rx: Option<Receiver<Dados>>,
    /// Pegada dos predios no chao: (centro, meias-dimensoes).
    pegadas: Vec<(Vec2, Vec2)>,
    porto: Option<PortoNoMapa>,
    tex: Option<Texture2D>,
    pub aberto: bool,
    /// Raio visivel do minimapa, em unidades. A roda sobre ele muda.
    alcance_mini: f32,
    pub viagem: Viagem,
    /// A rota em andamento (personagem + pontos que faltam), posta pelo
    /// `main` a cada quadro. Tracejada no minimapa e no mapa grande.
    pub rota: Vec<Vec2>,
    /// Onde fica o Mestre de Missoes nesta ilha.
    pub mestre: Option<Vec2>,
    /// NPCs da vila (da thread do mapa): o marcador e o "Ir para".
    npcs: Vec<(String, Vec2)>,
    /// A secao NPCs do "Ir para" aberta (fechada por padrao: e' lista longa).
    npcs_abertos: bool,
    /// Zonas de mob e regioes de recurso (`MapaDaIlha`).
    pub info: Option<InfoDaIlha>,
    pub filtros: Filtros,
    rolagem_lateral: crate::rolagem::Rolagem,
}

impl Default for Mapa {
    fn default() -> Self {
        Self {
            def: None,
            ger: None,
            cidade: None,
            rx: None,
            pegadas: Vec::new(),
            porto: None,
            tex: None,
            aberto: false,
            alcance_mini: 90.0,
            viagem: Viagem::default(),
            rota: Vec::new(),
            mestre: None,
            npcs: Vec::new(),
            npcs_abertos: false,
            info: None,
            filtros: Filtros::default(),
            rolagem_lateral: Default::default(),
        }
    }
}

impl Mapa {
    /// Raio visivel do minimapa (as preferencias guardam).
    pub fn alcance_minimapa(&self) -> f32 {
        self.alcance_mini
    }

    pub fn define_alcance_minimapa(&mut self, a: f32) {
        self.alcance_mini = a.clamp(ALCANCE_MIN, ALCANCE_MAX);
    }

    /// Mapa da ilha `def`. Zona sem ilha (mapa de tiles antigo) fica vazio e
    /// nao desenha nada.
    pub fn para(def: Option<&'static DefIlha>) -> Self {
        let mut m = Self::default();
        let Some(def) = def else { return m };
        let (tx, rx) = channel();
        std::thread::spawn(move || {
            let _ = tx.send(gerar_dados(def));
        });
        let ger = Gerador::da_ilha(def);
        m.cidade = ger.cidade();
        m.ger = Some(ger);
        m.def = Some(def);
        m.rx = Some(rx);
        m
    }

    /// Sobe a textura quando a thread terminar. GL so' no thread principal.
    pub fn acompanhar(&mut self) {
        let Some(rx) = &self.rx else { return };
        if let Ok(dados) = rx.try_recv() {
            self.pegadas = dados.pegadas;
            self.porto = dados.porto;
            self.mestre = dados.mestre;
            self.npcs = dados.npcs;
            let t = Texture2D::from_rgba8(LADO as u16, LADO as u16, &dados.rgba);
            t.set_filter(FilterMode::Linear);
            self.tex = Some(t);
            self.rx = None;
        }
    }

    pub fn tem_ilha(&self) -> bool {
        self.def.is_some()
    }

    /// Os vendedores desta ilha (NPC com loja), da vila gerada da semente:
    /// (nome, posicao). E' o painel "Lojas da ilha" do Menu.
    pub fn lojas(&self) -> Vec<(String, Vec2)> {
        let Some(g) = &self.ger else {
            return Vec::new();
        };
        g.vila()
            .npcs
            .iter()
            .filter(|n| n.loja.is_some())
            .map(|n| (n.nome.to_string(), vec2(n.pos.x, n.pos.y)))
            .collect()
    }

    /// Onde fica o NPC da vila que atende `giver` (o Mestre inclusive): e' o
    /// "Ir" de uma missao ainda nao aceita, que leva a quem da'.
    pub fn npc_do_giver(&self, giver: u16) -> Option<(String, Vec2)> {
        let g = self.ger.as_ref()?;
        g.vila()
            .npcs
            .iter()
            .find(|n| shared::quests::giver_do_npc(n.papel as u16) == Some(giver))
            .map(|n| (n.nome.to_string(), vec2(n.pos.x, n.pos.y)))
    }

    /// Os vendedores com o id da loja: (loja, nome, posicao). E' como o "Onde
    /// obter" acha o NPC de um `FonteDeItem::Vendedor`.
    pub fn lojas_com_id(&self) -> Vec<(u32, String, Vec2)> {
        let Some(g) = &self.ger else {
            return Vec::new();
        };
        g.vila()
            .npcs
            .iter()
            .filter_map(|n| {
                n.loja
                    .map(|l| (l, n.nome.to_string(), vec2(n.pos.x, n.pos.y)))
            })
            .collect()
    }

    /// Zona (ilha) deste mapa.
    pub fn zona(&self) -> Option<&'static str> {
        self.def.map(|d| d.zona)
    }

    /// `MapaDaIlha` chegou.
    pub fn define_info(
        &mut self,
        zonas: Vec<ZonaNoMapa>,
        recursos: Vec<RegiaoNoMapa>,
        nomes: Vec<(u16, String)>,
        rendimentos: Vec<(u8, String)>,
    ) {
        self.info = Some(InfoDaIlha {
            zonas,
            recursos,
            nomes: nomes.into_iter().collect(),
            rendimentos: rendimentos.into_iter().collect(),
            chefes: Vec::new(),
        });
    }

    /// Chefes de campo do `MapaDaIlha` (vem junto; separado pra nao mexer na
    /// assinatura de `define_info`).
    pub fn define_chefes(&mut self, chefes: Vec<shared::bosses::ChefeNoMapa>) {
        if let Some(info) = self.info.as_mut() {
            info.chefes = chefes;
        }
    }

    /// A zona ou regiao VISIVEL sob o mouse no mapa grande. Regiao primeiro:
    /// e' o marcador pequeno, que fica por cima da zona.
    fn marcador_sob(&self, m: Vec2, r: Rect) -> Option<Marcador> {
        let raio = self.raio();
        let escala = r.w / (2.0 * raio);
        let k = Self::escala();
        // NPC primeiro: e' o menor, dentro da cidade, por cima de tudo.
        if self.filtros.vila {
            let npc = self
                .npcs_da_vila()
                .iter()
                .enumerate()
                .map(|(i, (_, p))| (i, para_tela(*p, r, raio).distance(m)))
                .filter(|(_, d)| *d <= 12.0 * k)
                .min_by(|a, b| a.1.total_cmp(&b.1));
            if let Some((i, _)) = npc {
                return Some(Marcador::Npc(i));
            }
        }
        let info = self.info.as_ref()?;
        let regiao = info
            .recursos
            .iter()
            .enumerate()
            .filter(|(_, g)| self.filtros.regiao_visivel(g))
            .map(|(i, g)| {
                (
                    i,
                    para_tela(vec2(g.centro[0], g.centro[1]), r, raio).distance(m),
                )
            })
            .filter(|(_, d)| *d <= 9.0 * k)
            .min_by(|a, b| a.1.total_cmp(&b.1));
        if let Some((i, _)) = regiao {
            return Some(Marcador::Regiao(i));
        }
        info.zonas
            .iter()
            .enumerate()
            .filter(|(_, z)| self.filtros.zona_visivel(z))
            .map(|(i, z)| {
                (
                    i,
                    para_tela(centro_da_zona(z), r, raio).distance(m),
                    (z.raio * escala).max(9.0 * k),
                )
            })
            .filter(|(_, d, rp)| d <= rp)
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .map(|(i, _, _)| Marcador::Zona(i))
    }

    fn alvo_do_marcador(&self, mk: Marcador) -> Option<Alvo> {
        if let Marcador::Npc(i) = mk {
            let (nome, pos) = self.npcs_da_vila().into_iter().nth(i)?;
            return Some(Alvo {
                objetivo: Objetivo::Npc,
                pos,
                raio: 0.0,
                rotulo: nome,
            });
        }
        let info = self.info.as_ref()?;
        Some(match mk {
            Marcador::Npc(_) => return None,
            Marcador::Zona(i) => {
                let z = info.zonas.get(i)?;
                let dominante = z.bichos.first().map_or(0, |b| b.0);
                Alvo {
                    objetivo: Objetivo::Combate,
                    pos: centro_da_zona(z),
                    raio: z.raio,
                    rotulo: info.nome(dominante),
                }
            }
            Marcador::Regiao(i) => {
                let g = info.recursos.get(i)?;
                Alvo {
                    objetivo: Objetivo::Coleta(g.tipo),
                    pos: vec2(g.centro[0], g.centro[1]),
                    raio: g.raio,
                    rotulo: nome_do_tipo(g.tipo).to_string(),
                }
            }
        })
    }

    /// Tooltip do marcador sob o mouse.
    fn dica(&self, mk: Marcador, m: Vec2) {
        let Some(info) = self.info.as_ref() else {
            return;
        };
        let mut linhas: Vec<(String, Color)> = Vec::new();
        match mk {
            Marcador::Npc(i) => {
                let Some((nome, _)) = self.npcs_da_vila().into_iter().nth(i) else {
                    return;
                };
                linhas.push((nome, COR_NPC));
                linhas.push(("clique: ir falar (ou teleportar)".into(), estilo::AUTO));
            }
            Marcador::Zona(i) => {
                let Some(z) = info.zonas.get(i) else { return };
                linhas.push((
                    format!(
                        "{} · Nv {}–{}",
                        if z.forte { "FORTE" } else { "Zona de caça" },
                        z.lv_min,
                        z.lv_max
                    ),
                    estilo::OURO,
                ));
                if z.forte {
                    // O nivel e' o mesmo da vizinhanca; o que muda e' quantos
                    // vem de uma vez. Dizer isso e' o que faz o jogador
                    // escolher entrar em vez de tropecar.
                    linhas.push((
                        "muito mais inimigos no mesmo espaço".into(),
                        estilo::VERMELHO,
                    ));
                }
                for (k, c) in &z.bichos {
                    linhas.push((format!("{}  {c}%", info.nome(*k)), cor_do_bicho(*k)));
                }
                linhas.push(("clique: ir caçar (auto combate)".into(), estilo::AUTO));
            }
            Marcador::Regiao(i) => {
                let Some(g) = info.recursos.get(i) else {
                    return;
                };
                linhas.push((
                    format!("{} · {} corpos", nome_do_tipo(g.tipo), g.contagem),
                    cor_do_tipo(g.tipo),
                ));
                if let Some(rende) = info.rendimentos.get(&g.tipo) {
                    for parte in rende.split(", ").filter(|s| !s.is_empty()) {
                        linhas.push((parte.to_string(), estilo::TEXTO));
                    }
                }
                linhas.push(("clique: ir coletar (auto coleta)".into(), estilo::AUTO));
            }
        }
        let w = linhas
            .iter()
            .map(|(s, _)| estilo::medir(s, 13))
            .fold(120.0f32, f32::max)
            + u(24.0);
        let h = u(14.0) + linhas.len() as f32 * u(19.0);
        let x = (m.x + u(16.0)).min(screen_width() - w - u(8.0));
        let y = (m.y + u(12.0)).min(screen_height() - h - u(8.0));
        estilo::painel(Rect::new(x, y, w, h));
        for (i, (s, cor)) in linhas.iter().enumerate() {
            estilo::texto(x + u(12.0), y + u(22.0) + i as f32 * u(19.0), s, 13, *cor);
        }
    }

    fn lateral_rect(r: Rect) -> Rect {
        let k = Self::escala();
        Rect::new(
            r.x + r.w + 14.0 * k,
            r.y - 38.0 * k,
            LARGURA_LATERAL * k,
            r.h + 46.0 * k,
        )
    }

    /// Filtros e "Ir para", ao lado do mapa grande. Devolve o "Ir" clicado.
    fn desenha_lateral(&mut self, r: Rect, eu: Option<Vec2>, nivel: u32) -> Option<Entrada> {
        let lat = Self::lateral_rect(r);
        estilo::painel(lat);
        let Some(info) = self.info.as_ref() else {
            estilo::texto(
                lat.x + u(14.0),
                lat.y + u(28.0),
                "carregando zonas…",
                14,
                estilo::SUAVE,
            );
            return None;
        };
        let mouse = Vec2::from(mouse_position());
        let clique = crate::foco::clique();
        let mut toggle: Option<Chip> = None;
        let mut saida: Option<Entrada> = None;

        // ── filtros ──
        estilo::texto_forte(
            lat.x + u(14.0),
            lat.y + u(26.0),
            "Filtros",
            16,
            estilo::OURO,
        );
        let (mut x, mut y) = (lat.x + u(12.0), lat.y + u(38.0));
        let mut chips: Vec<(String, bool, Color, Chip)> =
            vec![("Mobs".into(), self.filtros.mobs, COR_MOB, Chip::Mobs)];
        for k in info.bichos() {
            chips.push((
                info.nome(k),
                !self.filtros.bichos_ocultos.contains(&k),
                cor_do_bicho(k),
                Chip::Bicho(k),
            ));
        }
        for t in 0..6u8 {
            chips.push((
                nome_do_tipo(t).into(),
                if t == 5 {
                    self.filtros.energia
                } else {
                    self.filtros.recursos[t as usize]
                },
                cor_do_tipo(t),
                Chip::Recurso(t),
            ));
        }
        chips.push(("Vila".into(), self.filtros.vila, estilo::OURO, Chip::Vila));
        for (rotulo, ligado, cor, id) in &chips {
            let w = estilo::medir(rotulo, 13) + u(24.0);
            if x + w > lat.x + lat.w - u(10.0) {
                x = lat.x + u(12.0);
                y += u(27.0);
            }
            let c = Rect::new(x, y, w, u(22.0));
            let alfa = if *ligado { 0.26 } else { 0.0 };
            estilo::ret_arredondado(c, c.h * 0.5, Color::new(cor.r, cor.g, cor.b, alfa));
            estilo::borda_arredondada(
                c,
                c.h * 0.5,
                1.0,
                Color::new(cor.r, cor.g, cor.b, if *ligado { 0.85 } else { 0.30 }),
            );
            draw_circle(
                c.x + u(9.0),
                c.y + u(11.0),
                u(3.5),
                if *ligado {
                    *cor
                } else {
                    Color::new(cor.r, cor.g, cor.b, 0.3)
                },
            );
            estilo::texto(
                c.x + u(16.0),
                c.y + u(16.0),
                rotulo,
                13,
                if *ligado {
                    estilo::TEXTO
                } else {
                    estilo::SUAVE
                },
            );
            if clique && c.contains(mouse) {
                toggle = Some(*id);
            }
            x += w + u(6.0);
        }

        // ── ir para ──
        y += u(44.0);
        estilo::texto(lat.x + u(14.0), y, "Ir para", 16, estilo::OURO);
        let area = Rect::new(
            lat.x + u(6.0),
            y + u(8.0),
            lat.w - u(12.0),
            lat.y + lat.h - (y + u(16.0)),
        );
        let eu = eu.unwrap_or(Vec2::ZERO);
        let bichos = info.bichos();
        let tipos = info.tipos();
        let npcs = if self.npcs_abertos {
            self.npcs.clone()
        } else {
            Vec::new()
        };
        let n_npcs = self.npcs.len();
        let total = (3 + bichos.len() + tipos.len() + npcs.len()) as f32 * u(LINHA_IR);
        // Rola arrastando, pela roda ou pela barra; o "Ir" vale no SOLTAR.
        let clique = self.rolagem_lateral.quadro(area, total, u(LINHA_IR));
        let mut ly = area.y - self.rolagem_lateral.pos;
        let visivel = |yy: f32| yy + u(LINHA_IR) > area.y && yy < area.y + area.h;
        crate::rolagem::recortar(Some(area));
        let mut linha = |rotulo: &str,
                         detalhe: String,
                         cor: Color,
                         alvo: Option<Alvo>,
                         ly: f32,
                         saida: &mut Option<Entrada>| {
            if !visivel(ly) {
                return;
            }
            draw_circle(area.x + u(10.0), ly + u(13.0), u(4.0), cor);
            estilo::texto_ajustado(
                rotulo,
                area.x + u(20.0),
                ly + u(12.0),
                area.w - u(80.0),
                13,
                estilo::TEXTO,
            );
            estilo::texto_ajustado(
                &detalhe,
                area.x + u(20.0),
                ly + u(24.0),
                area.w - u(80.0),
                11,
                estilo::SUAVE,
            );
            let b = Rect::new(area.x + area.w - u(64.0), ly + u(2.0), u(46.0), u(22.0));
            if let Some(a) = alvo {
                let _ = crate::ui::botao(b, "Ir", true);
                if clique.is_some_and(|c| b.contains(c) && area.contains(c)) {
                    *saida = Some(Entrada::Ir(a));
                }
            }
        };
        // NPCs: fechados por padrao (a lista e' longa); tocar no titulo abre.
        let titulo_npcs = Rect::new(area.x, ly, area.w - u(14.0), u(LINHA_IR));
        let mut alterna_npcs = false;
        if visivel(ly) {
            let acao = if self.npcs_abertos {
                "ocultar"
            } else {
                "ver ›"
            };
            estilo::texto(
                area.x + u(6.0),
                ly + u(18.0),
                &format!("NPCs ({n_npcs}) · {acao}"),
                13,
                estilo::OURO,
            );
            alterna_npcs = clique.is_some_and(|c| titulo_npcs.contains(c) && area.contains(c));
        }
        ly += u(LINHA_IR);
        for (nome, p) in &npcs {
            let alvo = Alvo {
                objetivo: Objetivo::Npc,
                pos: *p,
                raio: 0.0,
                rotulo: nome.clone(),
            };
            linha(
                nome,
                format!("{:.0} m", p.distance(eu)),
                COR_NPC,
                Some(alvo),
                ly,
                &mut saida,
            );
            ly += u(LINHA_IR);
        }
        if visivel(ly) {
            estilo::texto(area.x + u(6.0), ly + u(18.0), "Bichos", 13, estilo::SUAVE);
        }
        ly += u(LINHA_IR);
        for k in &bichos {
            let nome = info.nome(*k);
            let z = zona_mais_perto(&info.zonas, *k, eu, nivel);
            let faixa = info
                .faixa(*k)
                .map_or(String::new(), |(a, b)| format!("Nv {a}–{b} · "));
            let detalhe = z.map_or("sem zona".to_string(), |z| {
                format!("{faixa}{:.0} m", centro_da_zona(z).distance(eu))
            });
            let alvo = z.map(|z| Alvo {
                objetivo: Objetivo::Combate,
                pos: centro_da_zona(z),
                raio: z.raio,
                rotulo: nome.clone(),
            });
            linha(&nome, detalhe, cor_do_bicho(*k), alvo, ly, &mut saida);
            ly += u(LINHA_IR);
        }
        if visivel(ly) {
            estilo::texto(area.x + u(6.0), ly + u(18.0), "Recursos", 13, estilo::SUAVE);
        }
        ly += u(LINHA_IR);
        for t in &tipos {
            let g = regiao_mais_perto(&info.recursos, *t, eu);
            let n = info.recursos.iter().filter(|r| r.tipo == *t).count();
            let detalhe = g.map_or("—".to_string(), |g| {
                format!(
                    "{n} regiões · {:.0} m",
                    vec2(g.centro[0], g.centro[1]).distance(eu)
                )
            });
            let alvo = g.map(|g| Alvo {
                objetivo: Objetivo::Coleta(*t),
                pos: vec2(g.centro[0], g.centro[1]),
                raio: g.raio,
                rotulo: nome_do_tipo(*t).to_string(),
            });
            linha(
                nome_do_tipo(*t),
                detalhe,
                cor_do_tipo(*t),
                alvo,
                ly,
                &mut saida,
            );
            ly += u(LINHA_IR);
        }
        crate::rolagem::recortar(None);
        self.rolagem_lateral.desenha(area, total);
        if alterna_npcs {
            self.npcs_abertos = !self.npcs_abertos;
        }
        match toggle {
            Some(Chip::Mobs) => self.filtros.mobs = !self.filtros.mobs,
            Some(Chip::Bicho(k)) => {
                if !self.filtros.bichos_ocultos.remove(&k) {
                    self.filtros.bichos_ocultos.insert(k);
                }
            }
            Some(Chip::Recurso(t)) => {
                if t == 5 {
                    self.filtros.energia = !self.filtros.energia;
                } else {
                    self.filtros.recursos[t as usize] = !self.filtros.recursos[t as usize];
                }
            }
            Some(Chip::Vila) => self.filtros.vila = !self.filtros.vila,
            None => {}
        }
        saida
    }

    fn raio(&self) -> f32 {
        self.def.map_or(1.0, |d| d.raio_blocos as f32 * BLOCO)
    }

    /// Chao firme? Pela mesma funcao que desenha o chao.
    pub fn terra(&self, p: Vec2) -> bool {
        self.ger
            .as_ref()
            .is_some_and(|g| p.length() < self.raio() && g.altura(p.x, p.y) > NIVEL_DO_MAR + 0.1)
    }

    pub fn alterna(&mut self) {
        if self.tem_ilha() {
            self.aberto = !self.aberto;
        }
    }

    /// Abre pelo nome da zona, pelo ⤢ do minimapa ou pelo Menu — nunca tecla.
    pub fn abrir(&mut self) {
        if self.tem_ilha() {
            self.aberto = true;
        }
    }

    /// Canto de cima a' direita, abaixo da area e canal (ver `hud_layout`).
    pub fn mini_rect() -> Rect {
        crate::hud_layout::atual().minimapa
    }

    /// O mapa quadrado, com o painel lateral de filtros e "Ir para" a' direita.
    fn grande_rect() -> Rect {
        let k = Self::escala();
        let s = crate::hud_layout::tela_segura();
        let lat = LARGURA_LATERAL * k;
        let lado = (s.w - lat - 60.0 * k).min(s.h - 90.0 * k).max(200.0);
        let total = lado + 14.0 * k + lat;
        Rect::new(
            (s.x + (s.w - total) * 0.5).max(8.0),
            s.y + (s.h - lado) * 0.5 + 12.0 * k,
            lado,
            lado,
        )
    }

    /// Escala do mapa grande e do painel ao lado (no celular ele cresce ate'
    /// caber; ver `hud_estilo::escala_do_painel`).
    fn escala() -> f32 {
        estilo::escala_do_painel(LARGURA_LATERAL + 560.0, 600.0)
    }

    /// Os NPCs da vila desta ilha: (nome, posicao). Vem da thread do mapa
    /// (gerar a vila no quadro travava a abertura do mapa no celular).
    pub fn npcs_da_vila(&self) -> Vec<(String, Vec2)> {
        self.npcs.clone()
    }

    /// O botao de TAMANHO do minimapa, ao lado do ⤢ e dentro da moldura. Mora
    /// aqui e nao no `hud_layout` porque so' o minimapa o usa.
    fn expandir_rect() -> Rect {
        let ic = crate::hud_layout::atual().mapa_icone;
        Rect::new(ic.x - ic.w - 6.0, ic.y, ic.w, ic.h)
    }

    /// Os botoes dos CANTOS do minimapa, fora do disco: fechar em cima a'
    /// esquerda, − e + embaixo. Mesmo tamanho e recuo do ⤢.
    fn canto_do_minimapa(direita: bool, baixo: bool) -> Rect {
        let (r, ic) = (Self::mini_rect(), crate::hud_layout::atual().mapa_icone);
        let recuo = ic.y - r.y;
        let x = if direita {
            r.x + r.w - ic.w - recuo
        } else {
            r.x + recuo
        };
        let y = if baixo {
            r.y + r.h - ic.h - recuo
        } else {
            ic.y
        };
        Rect::new(x, y, ic.w, ic.h)
    }

    fn ocultar_rect() -> Rect {
        Self::canto_do_minimapa(false, false)
    }

    fn zoom_menos_rect() -> Rect {
        Self::canto_do_minimapa(false, true)
    }

    fn zoom_mais_rect() -> Rect {
        Self::canto_do_minimapa(true, true)
    }

    /// Minimapa fechado: o botao que sobra, no lugar do ⤢.
    fn reabrir_rect() -> Rect {
        crate::hud_layout::atual().minimapa_reabrir()
    }

    /// Aproxima (`perto`) ou afasta o minimapa um passo, o mesmo da roda.
    fn zoom_minimapa(&mut self, perto: bool) {
        let f = if perto { 0.8 } else { 1.25 };
        self.alcance_mini = (self.alcance_mini * f).clamp(ALCANCE_MIN, ALCANCE_MAX);
    }

    fn fechar_rect(r: Rect) -> Rect {
        let k = Self::escala();
        Rect::new(r.x + r.w - 32.0 * k, r.y - 36.0 * k, 32.0 * k, 32.0 * k)
    }

    /// O clique e a roda sao do mapa (e nao do mundo nem da camera)?
    pub fn pega_mouse(&self) -> bool {
        let m = Vec2::from(mouse_position());
        let mini = if crate::hud_layout::minimapa_oculto() {
            Self::reabrir_rect()
        } else {
            Self::mini_rect()
        };
        self.tem_ilha() && (self.aberto || mini.contains(m))
    }

    /// Roda e clique. Devolve o que o clique pede: viajar a um ponto, ou ir a
    /// uma zona/regiao (clicada no mapa grande).
    pub fn entrada(&mut self, eu: Option<Vec2>) -> Option<Entrada> {
        if !self.tem_ilha() {
            return None;
        }
        let m = Vec2::from(mouse_position());
        if self.aberto {
            let r = Self::grande_rect();
            if crate::foco::clique() {
                // O painel lateral trata os proprios botoes no desenho.
                if Self::lateral_rect(r).contains(m) {
                    return None;
                }
                // Fora do mapa ou no X: fecha. Numa zona/regiao: vai. No resto: viaja.
                if Self::fechar_rect(r).contains(m) || !r.contains(m) {
                    self.aberto = false;
                    return None;
                }
                if let Some(a) = self
                    .marcador_sob(m, r)
                    .and_then(|mk| self.alvo_do_marcador(mk))
                {
                    return Some(Entrada::Ir(a));
                }
                return Some(Entrada::Viajar(de_tela(m, r, self.raio())));
            }
            return None;
        }
        // Toque: o ponto do DEDO no quadro em que encosta. O mouse simulado
        // ainda aponta pro toque anterior (o mesmo defeito do botao de criar
        // personagem), e o botao errava.
        let aperto = apertou_em();
        if crate::hud_layout::minimapa_oculto() {
            if aperto.is_some_and(|p| Self::reabrir_rect().contains(p)) {
                crate::hud_layout::define_minimapa_oculto(false);
            }
            return None;
        }
        let r = Self::mini_rect();
        // Botoes: o ⤢ abre o Mapa, o de tamanho alterna compacto/expandido
        // (quem guarda e' o `main`, pelas preferencias), − e + dao zoom e o
        // do canto de cima fecha.
        if let Some(p) = aperto.filter(|p| r.contains(*p)) {
            if crate::hud_layout::atual().mapa_icone.contains(p) {
                self.aberto = true;
                return None;
            }
            if Self::expandir_rect().contains(p) {
                crate::hud_layout::define_minimapa_expandido(
                    !crate::hud_layout::minimapa_expandido(),
                );
                return None;
            }
            if Self::ocultar_rect().contains(p) {
                crate::hud_layout::define_minimapa_oculto(true);
                return None;
            }
            if Self::zoom_mais_rect().contains(p) {
                self.zoom_minimapa(true);
                return None;
            }
            if Self::zoom_menos_rect().contains(p) {
                self.zoom_minimapa(false);
                return None;
            }
        }
        if r.contains(m) {
            let (_, roda) = mouse_wheel();
            if roda != 0.0 {
                self.zoom_minimapa(roda > 0.0);
            }
        }
        if let Some(p) = aperto.filter(|p| r.contains(*p)) {
            let eu = eu?;
            // O mapa e' um disco: canto do painel e' moldura, nao viaja. E a
            // conta usa o raio DESENHADO, senao o clique cai a alguns metros
            // de onde se apontou.
            let (c, rad) = (r.center(), r.w * 0.5 - MARGEM_DO_DISCO);
            if p.distance(c) > rad {
                return None;
            }
            return Some(Entrada::Viajar(eu + (p - c) / rad * self.alcance_mini));
        }
        None
    }

    pub fn desenha_mini(&self, world: &World) {
        if !self.tem_ilha() {
            return;
        }
        if crate::hud_layout::minimapa_oculto() {
            let b = Self::reabrir_rect();
            let sobre = b.contains(Vec2::from(mouse_position()));
            let (c, raio) = (b.center(), b.w * 0.5);
            draw_circle(c.x, c.y, raio, estilo::FUNDO_ALTO);
            draw_circle_lines(
                c.x,
                c.y,
                raio,
                2.0,
                estilo::alfa(estilo::OURO, if sobre { 1.0 } else { 0.75 }),
            );
            let cor = if sobre { estilo::OURO } else { estilo::TEXTO };
            if !crate::icones_ui::mapa("mapa", c, b.w * 0.56, cor, 0.0) {
                glifo_minimapa(b, cor);
            }
            return;
        }
        let dentro = Self::mini_rect();
        let c = dentro.center();
        // SEM CAIXA: o disco flutua sobre o mundo, como no MIR4. A margem que
        // sobra dentro do retangulo do layout e' onde moram a rosa dos ventos e
        // as coordenadas — assim nada precisa vazar pra fora do que o
        // `hud_layout` reservou.
        let rad = dentro.w * 0.5 - MARGEM_DO_DISCO;
        draw_circle(c.x, c.y, rad, COR_AGUA);
        let Some(eu) = world.self_pos() else {
            self.moldura(dentro, None);
            return;
        };
        let raio = self.raio();
        let alcance = self.alcance_mini;
        let escala = dentro.w * 0.5 / alcance;
        match &self.tex {
            Some(tex) => disco_do_mapa(tex, c, rad, eu, alcance, raio),
            None => estilo::texto_centro(c.x, c.y + 30.0, "carregando mapa…", 12, estilo::SUAVE),
        }
        let ponto = |p: Vec2| c + (p - eu) * escala;
        let visivel = |q: Vec2| dentro.contains(q);
        // Zonas e regioes de leve: o minimapa e' pra se achar, nao pra ler.
        if let Some(info) = &self.info {
            for z in info.zonas.iter().filter(|z| self.filtros.zona_visivel(z)) {
                let q = ponto(centro_da_zona(z));
                let rp = (z.raio * escala).max(3.0);
                if q.distance(c) - rp < dentro.w * 0.75 {
                    let cor = z.bichos.first().map_or(COR_MOB, |b| cor_do_bicho(b.0));
                    let (esp, a) = if z.forte { (2.0, 0.8) } else { (1.0, 0.45) };
                    draw_circle_lines(q.x, q.y, rp, esp, Color::new(cor.r, cor.g, cor.b, a));
                    if z.forte && visivel(q) {
                        torre(q, 7.0, cor);
                    }
                }
            }
            for ch in &info.chefes {
                let q = ponto(vec2(ch.centro[0], ch.centro[1]));
                if visivel(q) {
                    let ouro = Color::new(1.0, 0.72, 0.25, if ch.vivo { 1.0 } else { 0.45 });
                    if !crate::icones_ui::mapa("chefe", q, 15.0, ouro, 0.0) {
                        crate::telegrafico::desenha_coroa(q, 4.0);
                    }
                }
            }
            for g in info
                .recursos
                .iter()
                .filter(|g| self.filtros.regiao_visivel(g))
            {
                let q = ponto(vec2(g.centro[0], g.centro[1]));
                if visivel(q) {
                    let cor = cor_do_tipo(g.tipo);
                    draw_circle(q.x, q.y, 2.0, Color::new(cor.r, cor.g, cor.b, 0.8));
                }
            }
        }
        if let Some(ci) = self.cidade.filter(|_| self.filtros.vila) {
            // O Vec2 do shared e' de outra versao do glam.
            let q = ponto(vec2(ci.centro().x, ci.centro().y));
            if visivel(q) {
                draw_circle_lines(
                    q.x,
                    q.y,
                    (Cidade::RAIO * escala).max(4.0),
                    1.5,
                    estilo::OURO,
                );
                casinha(q, 5.0, estilo::OURO);
            }
        }
        for (centro, meia) in self.pegadas.iter().filter(|_| self.filtros.vila) {
            let (a, b) = (ponto(*centro - *meia), ponto(*centro + *meia));
            if visivel(a) || visivel(b) {
                pegada(a, b, 1.5);
            }
        }
        if let Some(po) = self.porto.filter(|_| self.filtros.vila) {
            let (a, b) = (ponto(po.raiz), ponto(po.ponta));
            if visivel(a) || visivel(b) {
                draw_line(a.x, a.y, b.x, b.y, (3.0f32).max(escala * 2.5), COR_PREDIO);
            }
            let q = ponto(po.centro);
            if visivel(q) {
                ancora(q, 5.0, COR_PORTO);
            }
        }
        for (id, e) in &world.ents {
            if Some(*id) == world.self_id {
                continue;
            }
            let cor = match e.meta.tag {
                EntityTag::Enemy if e.morte.is_none() => COR_MOB,
                EntityTag::Npc => COR_NPC,
                EntityTag::Player => COR_GENTE,
                _ => continue,
            };
            let q = ponto(e.render_pos);
            if visivel(q) {
                draw_circle(q.x, q.y, 2.5, cor);
            }
        }
        let rota: Vec<Vec2> = self.rota.iter().map(|p| ponto(*p)).collect();
        tracejado(&rota, 4.0, 3.0, 2.0, estilo::AUTO, Some(dentro));
        if let Some(d) = self.viagem.destino() {
            let q = ponto(d);
            // Fora do disco, o destino fica preso na BORDA REDONDA, na direcao
            // dele. Preso no quadrado, ele parava nos cantos — que agora sao
            // moldura, e o jogador nao veria mais a marca.
            let fora = q - c;
            let preso = if fora.length() > rad - 5.0 {
                c + fora.normalize_or_zero() * (rad - 5.0)
            } else {
                q
            };
            marca_destino(preso, 5.0);
        }
        let yaw = world
            .self_id
            .and_then(|id| world.ents.get(&id))
            .map_or(0.0, |e| e.yaw);
        seta(c, yaw, 7.0, estilo::TEXTO);
        self.moldura(dentro, Some(eu));
    }

    /// Tudo que vai POR CIMA do disco: a mascara que come os cantos, o anel, a
    /// rosa dos ventos, as coordenadas e os dois botoes — ⤢ abre o Mapa, o
    /// outro muda o tamanho do minimapa.
    fn moldura(&self, dentro: Rect, eu: Option<Vec2>) {
        let (c, rad) = (dentro.center(), dentro.w * 0.5 - MARGEM_DO_DISCO);
        // Halo curto no lugar da caixa: descola o disco do cenario sem desenhar
        // painel nenhum.
        for i in 0..4 {
            let k = i as f32;
            draw_circle_lines(
                c.x,
                c.y,
                rad + 1.5 + k * 1.7,
                2.0,
                Color::new(0.0, 0.0, 0.0, 0.18 - k * 0.04),
            );
        }
        estilo::arco(c, rad, 0.0, 1.0, 2.5, estilo::alfa(estilo::OURO, 0.75));
        estilo::arco(
            c,
            rad - 3.0,
            0.0,
            1.0,
            1.0,
            estilo::alfa(estilo::BRILHO, 0.5),
        );
        rosa_dos_ventos(c, rad);
        if let Some(eu) = eu {
            let t = format!("{:.0}, {:.0}", eu.x, eu.y);
            let w = estilo::medir(&t, 12) + 18.0;
            // Encostada na base do anel, centrada. No canto ela ficava orfa,
            // solta num vazio sem relacao com o disco; o "S" da bussola saiu
            // justamente pra ela morar aqui sem cobrir letra nenhuma.
            let caixa = Rect::new(c.x - w * 0.5, c.y + rad + 1.0, w, 18.0);
            estilo::ret_arredondado(caixa, caixa.h * 0.5, estilo::FUNDO);
            estilo::borda_arredondada(caixa, caixa.h * 0.5, 1.0, estilo::alfa(estilo::OURO, 0.30));
            estilo::texto_centro(caixa.center().x, caixa.y + 13.0, &t, 12, estilo::TEXTO);
        }
        let m = Vec2::from(mouse_position());
        let ic = crate::hud_layout::atual().mapa_icone;
        let sobre = ic.contains(m);
        botao_da_moldura(ic, sobre);
        let cor = if sobre { estilo::OURO } else { estilo::TEXTO };
        let (a, b, d) = (
            vec2(ic.x + 5.0, ic.y + 5.0),
            vec2(ic.x + ic.w - 5.0, ic.y + ic.h - 5.0),
            ic.w * 0.28,
        );
        draw_line(a.x, a.y, b.x, b.y, 1.5, cor);
        draw_line(a.x, a.y, a.x + d, a.y, 1.5, cor);
        draw_line(a.x, a.y, a.x, a.y + d, 1.5, cor);
        draw_line(b.x, b.y, b.x - d, b.y, 1.5, cor);
        draw_line(b.x, b.y, b.x, b.y - d, 1.5, cor);
        let ex = Self::expandir_rect();
        let sobre_ex = ex.contains(m);
        botao_da_moldura(ex, sobre_ex);
        glifo_tamanho(
            ex,
            crate::hud_layout::minimapa_expandido(),
            if sobre_ex {
                estilo::OURO
            } else {
                estilo::TEXTO
            },
        );
        let apagado = estilo::alfa(estilo::TEXTO, 0.35);
        for (r, glifo, ativo) in [
            (Self::ocultar_rect(), '×', true),
            (
                Self::zoom_menos_rect(),
                '−',
                self.alcance_mini < ALCANCE_MAX,
            ),
            (Self::zoom_mais_rect(), '+', self.alcance_mini > ALCANCE_MIN),
        ] {
            let sobre = ativo && r.contains(m);
            botao_da_moldura(r, sobre);
            let cor = if !ativo {
                apagado
            } else if sobre {
                estilo::OURO
            } else {
                estilo::TEXTO
            };
            let (c, d) = (r.center(), r.w * 0.24);
            match glifo {
                '×' => {
                    draw_line(c.x - d, c.y - d, c.x + d, c.y + d, 1.8, cor);
                    draw_line(c.x - d, c.y + d, c.x + d, c.y - d, 1.8, cor);
                }
                _ => {
                    draw_line(c.x - d * 1.2, c.y, c.x + d * 1.2, c.y, 2.0, cor);
                    if glifo == '+' {
                        draw_line(c.x, c.y - d * 1.2, c.x, c.y + d * 1.2, 2.0, cor);
                    }
                }
            }
        }
    }

    /// O mapa grande (M) com o painel lateral. Devolve o "Ir" clicado no
    /// painel. `nivel` escolhe a zona certa pra cada bicho.
    pub fn desenha_grande(&mut self, world: &World, nivel: u32) -> Option<Entrada> {
        if !self.aberto || !self.tem_ilha() {
            return None;
        }
        estilo::no_painel(Self::escala(), || {
            self.desenha_grande_mapa(world);
            let m = Vec2::from(mouse_position());
            let r = Self::grande_rect();
            // Tutorial "abra o mapa e toque num lugar": o alvo e' o mapa todo.
            crate::foco::marca(crate::foco::chave::MAPA_IR, r);
            if r.contains(m) {
                if let Some(mk) = self.marcador_sob(m, r) {
                    self.dica(mk, m);
                }
            }
            self.desenha_lateral(r, world.self_pos(), nivel)
        })
    }

    fn desenha_grande_mapa(&self, world: &World) {
        let (sw, sh) = (screen_width(), screen_height());
        draw_rectangle(0.0, 0.0, sw, sh, Color::new(0.0, 0.0, 0.0, 0.45));
        let r = Self::grande_rect();
        estilo::painel(Rect::new(
            r.x - u(8.0),
            r.y - u(38.0),
            r.w + u(16.0),
            r.h + u(46.0),
        ));
        let nome = self.def.map_or("", |d| d.nome);
        estilo::texto_forte(
            r.x,
            r.y - u(14.0),
            &format!("Mapa · {nome}"),
            17,
            estilo::OURO,
        );
        let dica = "clique: viajar · zona/recurso: ir · Esc fecha";
        estilo::texto(
            r.x + r.w - u(36.0) - estilo::medir(dica, 13),
            r.y - u(14.0),
            dica,
            13,
            estilo::SUAVE,
        );
        let f = Self::fechar_rect(r);
        if !crate::icones_ui::ui("fechar", f.center(), f.w.min(f.h) * 0.55, estilo::TEXTO) {
            estilo::texto_centro(f.x + f.w * 0.5, f.y + u(19.0), "x", 18, estilo::TEXTO);
        }

        draw_rectangle(r.x, r.y, r.w, r.h, COR_AGUA);
        match &self.tex {
            Some(tex) => draw_texture_ex(
                tex,
                r.x,
                r.y,
                WHITE,
                DrawTextureParams {
                    dest_size: Some(vec2(r.w, r.h)),
                    ..Default::default()
                },
            ),
            None => estilo::texto_centro(
                r.x + r.w * 0.5,
                r.y + r.h * 0.5,
                "carregando mapa…",
                18,
                estilo::SUAVE,
            ),
        }
        let raio = self.raio();
        let escala = r.w / (2.0 * raio);
        let ponto = |p: Vec2| para_tela(p, r, raio);
        if let Some(info) = &self.info {
            // Chefes sempre visiveis: sao o que se procura no mapa.
            for ch in &info.chefes {
                let q = ponto(vec2(ch.centro[0], ch.centro[1]));
                let ouro = Color::new(1.0, 0.72, 0.25, if ch.vivo { 1.0 } else { 0.5 });
                if !crate::icones_ui::mapa("chefe", q, u(26.0), ouro, 0.0) {
                    crate::telegrafico::desenha_coroa(q, u(8.0));
                }
                let t = format!(
                    "{} · Nv {}{}",
                    ch.nome,
                    ch.nivel,
                    if ch.vivo { "" } else { " (renascendo)" }
                );
                estilo::texto_centro(
                    q.x + 1.0,
                    q.y + u(23.0),
                    &t,
                    12,
                    Color::new(0.0, 0.0, 0.0, 0.8),
                );
                estilo::texto_centro(q.x, q.y + u(22.0), &t, 12, Color::new(1.0, 0.64, 0.37, 1.0));
            }
            for z in info.zonas.iter().filter(|z| self.filtros.zona_visivel(z)) {
                let q = ponto(centro_da_zona(z));
                let rp = (z.raio * escala).max(u(5.0));
                let cor = z.bichos.first().map_or(COR_MOB, |b| cor_do_bicho(b.0));
                // O FORTE e' o mesmo circulo, mais forte e com anel duplo: a
                // mancha mais cheia ja' diz "aqui tem mais bicho" antes de o
                // olho chegar no icone.
                let (fundo, anel) = if z.forte { (0.30, 2.5) } else { (0.16, 1.5) };
                draw_circle(q.x, q.y, rp, Color::new(cor.r, cor.g, cor.b, fundo));
                draw_circle_lines(q.x, q.y, rp, anel, Color::new(cor.r, cor.g, cor.b, 0.85));
                if z.forte {
                    draw_circle_lines(
                        q.x,
                        q.y,
                        rp + u(3.0),
                        1.0,
                        Color::new(cor.r, cor.g, cor.b, 0.45),
                    );
                    torre(q - vec2(0.0, rp.max(u(14.0)) + u(4.0)), u(9.0), cor);
                }
                // Marcador do bicho dominante no centro da zona (acima do rotulo).
                if let Some(b) = z.bichos.first() {
                    let y = if rp >= u(14.0) { u(13.0) } else { 0.0 };
                    crate::icones_ui::mapa(
                        crate::icones_ui::nome_do_bicho(b.0),
                        q - vec2(0.0, y),
                        u(18.0),
                        cor,
                        0.0,
                    );
                }
                if rp >= u(14.0) {
                    if let Some(b) = z.bichos.first() {
                        let t = if z.forte {
                            format!("Forte · {} · Nv {}–{}", info.nome(b.0), z.lv_min, z.lv_max)
                        } else {
                            format!("{} · Nv {}–{}", info.nome(b.0), z.lv_min, z.lv_max)
                        };
                        estilo::texto_centro(
                            q.x + 1.0,
                            q.y + u(5.0),
                            &t,
                            12,
                            Color::new(0.0, 0.0, 0.0, 0.8),
                        );
                        estilo::texto_centro(q.x, q.y + u(4.0), &t, 12, estilo::TEXTO);
                    }
                }
            }
            for g in info
                .recursos
                .iter()
                .filter(|g| self.filtros.regiao_visivel(g))
            {
                let q = ponto(vec2(g.centro[0], g.centro[1]));
                let s = 3.0 + (g.contagem as f32).sqrt().min(3.5);
                let cor = cor_do_tipo(g.tipo);
                let nome = if g.tipo == 0 { "madeira" } else { "pedra" };
                if !crate::icones_ui::mapa(nome, q, (s + 1.2) * u(3.2), cor, 0.0) {
                    losango(q, u(s + 1.2), Color::new(0.0, 0.0, 0.0, 0.7));
                    losango(q, u(s), cor);
                }
            }
        }
        if let Some(ci) = self.cidade.filter(|_| self.filtros.vila) {
            // O Vec2 do shared e' de outra versao do glam.
            let q = ponto(vec2(ci.centro().x, ci.centro().y));
            draw_circle_lines(
                q.x,
                q.y,
                (Cidade::RAIO * escala).max(u(6.0)),
                u(2.0),
                estilo::OURO,
            );
            casinha(q, u(7.0), estilo::OURO);
            estilo::texto_centro(q.x, q.y - u(12.0), "Cidade", 14, estilo::OURO);
        }
        for (centro, meia) in self.pegadas.iter().filter(|_| self.filtros.vila) {
            pegada(ponto(*centro - *meia), ponto(*centro + *meia), 1.0);
        }
        if let Some(po) = self.porto.filter(|_| self.filtros.vila) {
            let q = ponto(po.centro);
            let (a, b) = (ponto(po.raiz), ponto(po.ponta));
            draw_circle_lines(q.x, q.y, (po.raio * escala).max(u(6.0)), u(2.0), COR_PORTO);
            draw_line(a.x, a.y, b.x, b.y, u(3.0), COR_PREDIO);
            ancora(q, u(7.0), COR_PORTO);
            estilo::texto_centro(q.x, q.y - u(14.0), "Porto", 14, COR_PORTO);
        }
        // NPCs da vila, com o filtro Vila (o `world` so' tem os de perto).
        // Tocar leva ate' ele; a lista do "Ir para" tem todos, sempre.
        if self.filtros.vila {
            for (_, p) in &self.npcs {
                let q = ponto(*p);
                if !crate::icones_ui::mapa("npc", q, u(14.0), COR_NPC, 0.0) {
                    draw_circle(q.x, q.y, u(3.5), COR_NPC);
                }
            }
        }
        for (id, e) in &world.ents {
            if Some(*id) == world.self_id || e.meta.tag != EntityTag::Player {
                continue;
            }
            let q = ponto(e.render_pos);
            draw_circle(q.x, q.y, u(3.0), COR_GENTE);
        }
        let rota: Vec<Vec2> = self.rota.iter().map(|p| ponto(*p)).collect();
        tracejado(&rota, u(5.0), u(3.0), u(2.5), estilo::AUTO, None);
        let eu = world.self_pos();
        if let Some(d) = self.viagem.destino() {
            let q = ponto(d);
            if let Some(eu) = eu {
                let p = ponto(eu);
                draw_line(
                    p.x,
                    p.y,
                    q.x,
                    q.y,
                    1.5,
                    Color::new(estilo::AUTO.r, estilo::AUTO.g, estilo::AUTO.b, 0.7),
                );
                estilo::texto_centro(
                    q.x,
                    q.y + u(20.0),
                    &format!("{:.0} m", eu.distance(d)),
                    13,
                    estilo::AUTO,
                );
            }
            marca_destino(q, u(7.0));
        }
        if let Some(eu) = eu {
            let yaw = world
                .self_id
                .and_then(|id| world.ents.get(&id))
                .map_or(0.0, |e| e.yaw);
            seta(ponto(eu), yaw, u(9.0), estilo::TEXTO);
        }
        // Onde o clique cairia: agua avisa antes de clicar.
        let m = Vec2::from(mouse_position());
        if r.contains(m) && self.tex.is_some() && !self.terra(de_tela(m, r, raio)) {
            estilo::texto_centro(m.x, m.y - u(12.0), "água", 13, estilo::SUAVE);
        }
    }

    /// "Viajando · N m" enquanto houver viagem: o texto da faixa de estado.
    pub fn faixa_viagem(&self, eu: Option<Vec2>) -> Option<String> {
        let (Some(d), Some(eu)) = (self.viagem.destino(), eu) else {
            return None;
        };
        Some(format!("Viajando · {:.0} m", eu.distance(d)))
    }
}

/// Seta do jogador. O modelo olha pra `(sin yaw, cos yaw)` em (x, z), e z
/// cresce pra baixo no mapa.
/// Folga entre a borda do retangulo do layout e o disco: e' nela que cabem a
/// rosa dos ventos e a pilula de coordenadas, sem vazar do que o `hud_layout`
/// reservou nem invadir o mapa.
/// 20 e' medido, nao escolhido: a pilula tem 18 de altura e nasce 1 abaixo do
/// disco, entao a borda de baixo dela cai em `rad + 19` — 1 px dentro. As
/// letras da bussola vao ate' `rad + 14`, tambem dentro. Com 15 a pilula
/// furava o retangulo do layout em 4 px.
const MARGEM_DO_DISCO: f32 = 20.0;

/// O mapa RECORTADO EM DISCO, por leque de triangulos com UV propria.
///
/// E' o que permite o minimapa nao ter caixa. Sem malha, o unico jeito de
/// fingir um circulo e' pintar os cantos por cima — e pintar por cima exige uma
/// moldura opaca, que era exatamente o que deixava o minimapa caixudo.
///
/// Fora do quadro da ilha a UV passa de [0,1] e a textura grampeia na borda,
/// que ali ja' e' agua: o resultado sai certo sem tratamento especial.
fn disco_do_mapa(tex: &Texture2D, c: Vec2, rad: f32, eu: Vec2, alcance: f32, raio: f32) {
    const N: usize = 96;
    let uv = |p: Vec2| vec2((p.x + raio) / (2.0 * raio), (p.y + raio) / (2.0 * raio));
    let branco = [255u8, 255, 255, 255];
    let mut vertices = Vec::with_capacity(N + 1);
    vertices.push(Vertex {
        position: vec3(c.x, c.y, 0.0),
        uv: uv(eu),
        color: branco,
        normal: Vec4::ZERO,
    });
    for i in 0..N {
        let a = i as f32 / N as f32 * std::f32::consts::TAU;
        let d = vec2(a.cos(), a.sin());
        vertices.push(Vertex {
            position: vec3(c.x + d.x * rad, c.y + d.y * rad, 0.0),
            uv: uv(eu + d * alcance),
            color: branco,
            normal: Vec4::ZERO,
        });
    }
    let mut indices = Vec::with_capacity(N * 3);
    for i in 0..N {
        indices.extend_from_slice(&[0, 1 + i as u16, 1 + ((i + 1) % N) as u16]);
    }
    draw_mesh(&Mesh {
        vertices,
        indices,
        texture: Some(tex.clone()),
    });
}

/// N/L/S/O em volta do disco. O norte e' o unico dourado: e' o que se procura.
fn rosa_dos_ventos(c: Vec2, rad: f32) {
    // Sem "S": o sul e' onde mora a pilula de coordenadas, encostada no anel.
    for (i, nome) in [(0usize, "N"), (1, "L"), (3, "O")] {
        let a = -std::f32::consts::FRAC_PI_2 + i as f32 * std::f32::consts::FRAC_PI_2;
        let d = vec2(a.cos(), a.sin());
        let cor = if i == 0 { estilo::OURO } else { estilo::SUAVE };
        // FORA do disco: dentro dele as letras ficavam por cima do terreno, que
        // e' o que a margem existe pra evitar.
        let (t0, t1) = (c + d * rad, c + d * (rad + 4.0));
        draw_line(t0.x, t0.y, t1.x, t1.y, 1.5, estilo::alfa(cor, 0.75));
        let p = c + d * (rad + 10.0);
        estilo::texto_centro(p.x, p.y + 4.0, nome, 11, cor);
    }
}

/// Faixa do zoom do minimapa (raio visivel, em unidades).
const ALCANCE_MIN: f32 = 30.0;
const ALCANCE_MAX: f32 = 400.0;

/// Onde o dedo (ou o mouse) apertou NESTE quadro. Igual ao da tela de
/// personagens: o mouse simulado da macroquad fica no toque ANTERIOR.
fn apertou_em() -> Option<Vec2> {
    if let Some(t) = touches()
        .into_iter()
        .find(|t| t.phase == TouchPhase::Started)
    {
        return Some(t.position);
    }
    crate::foco::clique().then(|| Vec2::from(mouse_position()))
}

/// Minimapa fechado: um disco com a seta do jogador, pra ler "minimapa".
fn glifo_minimapa(r: Rect, cor: Color) {
    let c = r.center();
    draw_circle_lines(c.x, c.y, r.w * 0.32, 1.6, cor);
    let s = r.w * 0.14;
    draw_triangle(
        c + vec2(0.0, -s),
        c + vec2(-s * 0.7, s * 0.7),
        c + vec2(s * 0.7, s * 0.7),
        cor,
    );
}

fn botao_da_moldura(r: Rect, sobre: bool) {
    estilo::ret_arredondado(r, estilo::RAIO_PEQUENO, estilo::FUNDO_ALTO);
    estilo::borda_arredondada(
        r,
        estilo::RAIO_PEQUENO,
        1.0,
        if sobre {
            estilo::alfa(estilo::ACENTO, 0.7)
        } else {
            estilo::BORDA_FORTE
        },
    );
}

/// Duas setas: afastando-se (crescer) ou aproximando-se (encolher). De
/// proposito NAO parece o ⤢ do lado, que faz outra coisa — abre o Mapa.
fn glifo_tamanho(r: Rect, grande: bool, cor: Color) {
    let c = r.center();
    let (w, h) = (r.w * 0.20, r.h * 0.17);
    let vao = if grande { r.h * 0.10 } else { r.h * 0.20 };
    for lado in [-1.0f32, 1.0] {
        let base = c + vec2(0.0, lado * vao);
        let ponta = base + vec2(0.0, if grande { -lado } else { lado } * h);
        draw_triangle(ponta, base + vec2(-w, 0.0), base + vec2(w, 0.0), cor);
    }
}

fn seta(c: Vec2, yaw: f32, s: f32, cor: Color) {
    // A seta do atlas aponta pra cima; girar `PI - yaw` leva o "cima" pra
    // `(sin yaw, cos yaw)` na tela (y pra baixo).
    if crate::icones_ui::mapa("jogador", c, s * 2.6, cor, std::f32::consts::PI - yaw) {
        return;
    }
    let dir = vec2(yaw.sin(), yaw.cos());
    let perp = vec2(-dir.y, dir.x);
    let (a, b, d) = (
        c + dir * s,
        c - dir * s * 0.6 + perp * s * 0.6,
        c - dir * s * 0.6 - perp * s * 0.6,
    );
    draw_triangle(a, b, d, Color::new(0.0, 0.0, 0.0, 0.6));
    draw_triangle(a, b, d, cor);
    draw_triangle_lines(a, b, d, 1.0, Color::new(0.0, 0.0, 0.0, 0.8));
}

fn marca_destino(q: Vec2, s: f32) {
    if crate::icones_ui::mapa("destino", q, s * 3.4, estilo::AUTO, 0.0) {
        return;
    }
    draw_line(q.x - s, q.y - s, q.x + s, q.y + s, 2.5, estilo::AUTO);
    draw_line(q.x - s, q.y + s, q.x + s, q.y - s, 2.5, estilo::AUTO);
}

/// Losango: o icone de regiao de recurso.
fn losango(q: Vec2, s: f32, cor: Color) {
    draw_triangle(
        vec2(q.x, q.y - s),
        vec2(q.x - s, q.y),
        vec2(q.x + s, q.y),
        cor,
    );
    draw_triangle(
        vec2(q.x, q.y + s),
        vec2(q.x - s, q.y),
        vec2(q.x + s, q.y),
        cor,
    );
}

/// Retangulo da pegada de um predio, com tamanho minimo pra nao sumir de longe.
fn pegada(a: Vec2, b: Vec2, minimo: f32) {
    let (mn, mx) = (a.min(b), a.max(b));
    let (w, h) = ((mx.x - mn.x).max(minimo), (mx.y - mn.y).max(minimo));
    draw_rectangle(mn.x, mn.y, w, h, COR_PREDIO);
}

/// Ancora: haste, argola e braco curvo.
fn ancora(q: Vec2, s: f32, cor: Color) {
    if crate::icones_ui::mapa("porto", q, s * 3.0, cor, 0.0) {
        return;
    }
    let sombra = Color::new(0.0, 0.0, 0.0, 0.6);
    for (c, g) in [(sombra, 3.0), (cor, 1.6)] {
        draw_circle_lines(q.x, q.y - s * 0.85, s * 0.25, g * 0.7, c);
        draw_line(q.x, q.y - s * 0.6, q.x, q.y + s * 0.8, g, c);
        draw_line(
            q.x - s * 0.45,
            q.y - s * 0.3,
            q.x + s * 0.45,
            q.y - s * 0.3,
            g,
            c,
        );
        draw_line(q.x - s * 0.75, q.y + s * 0.35, q.x, q.y + s * 0.8, g, c);
        draw_line(q.x + s * 0.75, q.y + s * 0.35, q.x, q.y + s * 0.8, g, c);
    }
}

fn casinha(q: Vec2, s: f32, cor: Color) {
    if crate::icones_ui::mapa("cidade", q, s * 3.0, cor, 0.0) {
        return;
    }
    draw_rectangle(q.x - s * 0.6, q.y - s * 0.1, s * 1.2, s * 0.8, cor);
    draw_triangle(
        vec2(q.x - s * 0.85, q.y - s * 0.05),
        vec2(q.x + s * 0.85, q.y - s * 0.05),
        vec2(q.x, q.y - s * 0.8),
        cor,
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tudo_terra(_: Vec2) -> bool {
        true
    }

    #[test]
    fn rota_longe_so_processa_o_pedaco_visivel() {
        let tela = Rect::new(100.0, 100.0, 200.0, 200.0);
        let rota = [vec2(-1_000_000.0, 180.0), vec2(1_000_000.0, 180.0)];
        let mut vistos = Vec::new();
        let n = para_cada_traco(&rota, 5.0, 3.0, Some(tela), 256, |a, b| {
            vistos.push((a, b));
        });
        assert!(n <= 27, "so' cabem poucos tracos em 200 px: {n}");
        assert!(n > 20, "a parte que cruza a tela continua desenhada: {n}");
        assert!(vistos.iter().all(|(a, b)| {
            a.x >= tela.x - 0.01
                && b.x <= tela.x + tela.w + 0.01
                && a.y >= tela.y
                && b.y <= tela.y + tela.h
        }));
    }

    #[test]
    fn rota_2d_tem_teto_de_trabalho_por_quadro() {
        let rota = [Vec2::ZERO, vec2(1_000_000.0, 0.0)];
        let mut chamados = 0;
        let n = para_cada_traco(&rota, 1.0, 1.0, None, 37, |_, _| chamados += 1);
        assert_eq!((n, chamados), (37, 37));
    }

    #[test]
    fn perto_vai_direto_ao_destino() {
        let mut v = Viagem::default();
        v.iniciar(vec2(100.0, 0.0), 0.0);
        assert_eq!(
            v.passo(Vec2::ZERO, 0.0, tudo_terra),
            Passo::Enviar(vec2(100.0, 0.0))
        );
    }

    #[test]
    fn longe_vira_etapa_dentro_do_alcance_do_servidor() {
        let mut v = Viagem::default();
        v.iniciar(vec2(500.0, 30.0), 0.0);
        let Passo::Enviar(p) = v.passo(Vec2::ZERO, 0.0, tudo_terra) else {
            panic!("devia enviar")
        };
        assert!(p.length() <= ALCANCE_DIRETO, "etapa a {:.0}", p.length());
        assert!(p.x > 100.0, "etapa na direcao do destino: {p:?}");
        // Chegou perto da etapa: manda a proxima, mas respeitando o intervalo.
        assert_eq!(v.passo(p, 0.1, tudo_terra), Passo::Nada);
        let Passo::Enviar(q) = v.passo(p, 0.5, tudo_terra) else {
            panic!("devia mandar a proxima")
        };
        assert!(q.distance(vec2(500.0, 30.0)) < p.distance(vec2(500.0, 30.0)));
    }

    #[test]
    fn etapa_na_agua_procura_terra() {
        // Faixa de agua em x 140..175, bem onde cairia a etapa reta.
        let terra = |p: Vec2| !(140.0..175.0).contains(&p.x);
        let mut v = Viagem::default();
        v.iniciar(vec2(600.0, 0.0), 0.0);
        let Passo::Enviar(p) = v.passo(Vec2::ZERO, 0.0, terra) else {
            panic!()
        };
        assert!(terra(p), "etapa caiu na agua: {p:?}");
    }

    #[test]
    fn parado_pede_de_novo_e_desiste_sem_progresso() {
        let mut v = Viagem::default();
        v.iniciar(vec2(100.0, 0.0), 0.0);
        let eu = Vec2::ZERO;
        assert!(matches!(v.passo(eu, 0.0, tudo_terra), Passo::Enviar(_)));
        assert_eq!(v.passo(eu, 1.0, tudo_terra), Passo::Nada);
        assert!(matches!(v.passo(eu, 1.6, tudo_terra), Passo::Enviar(_)));
        let mut t = 1.6;
        let mut desistiu = false;
        for _ in 0..20 {
            t += 1.6;
            match v.passo(eu, t, tudo_terra) {
                Passo::Desistiu => {
                    desistiu = true;
                    break;
                }
                Passo::Enviar(_) | Passo::Nada => {}
                Passo::Chegou => panic!(),
            }
        }
        assert!(desistiu, "parado no mesmo lugar devia desistir");
        assert!(!v.ativa());
    }

    #[test]
    fn chega_e_cancela() {
        let mut v = Viagem::default();
        v.iniciar(vec2(10.0, 0.0), 0.0);
        assert!(matches!(
            v.passo(Vec2::ZERO, 0.0, tudo_terra),
            Passo::Enviar(_)
        ));
        assert_eq!(v.passo(vec2(9.0, 0.0), 1.0, tudo_terra), Passo::Chegou);
        assert!(!v.ativa());
        v.iniciar(vec2(10.0, 0.0), 2.0);
        v.cancelar();
        assert_eq!(v.passo(Vec2::ZERO, 3.0, tudo_terra), Passo::Nada);
    }

    fn zona(x: f32, lv: (u16, u16), bichos: Vec<(u16, u8)>) -> ZonaNoMapa {
        ZonaNoMapa {
            centro: [x, 0.0],
            raio: 45.0,
            lv_min: lv.0,
            lv_max: lv.1,
            bichos,
            forte: false,
        }
    }

    fn regiao(x: f32, tipo: u8) -> RegiaoNoMapa {
        RegiaoNoMapa {
            centro: [x, 0.0],
            raio: 10.0,
            tipo,
            contagem: 5,
        }
    }

    #[test]
    fn filtros_escondem_zona_pelo_dominante_e_regiao_pelo_tipo() {
        let lobos = zona(0.0, (1, 3), vec![(0, 83), (1, 17)]);
        // O mapa abre limpo: nada aparece ate' o jogador ligar.
        let padrao = Filtros::default();
        assert!(!padrao.zona_visivel(&lobos));
        assert!((0..=5).all(|t| !padrao.regiao_visivel(&regiao(0.0, t))));
        assert!(!padrao.vila);
        let mut f = Filtros {
            mobs: true,
            bichos_ocultos: HashSet::new(),
            recursos: [true; 5],
            energia: true,
            vila: true,
        };
        assert!(f.zona_visivel(&lobos));
        assert!(f.regiao_visivel(&regiao(0.0, 5)));
        f.energia = false;
        assert!(!f.regiao_visivel(&regiao(0.0, 5)));
        f.bichos_ocultos.insert(1);
        assert!(
            f.zona_visivel(&lobos),
            "esconder o secundario nao tira a zona"
        );
        f.bichos_ocultos.insert(0);
        assert!(!f.zona_visivel(&lobos));
        f.bichos_ocultos.clear();
        f.mobs = false;
        assert!(!f.zona_visivel(&lobos));
        f.recursos[3] = false;
        assert!(!f.regiao_visivel(&regiao(0.0, 3)));
        assert!(f.regiao_visivel(&regiao(0.0, 0)));
    }

    #[test]
    fn a_zona_e_a_regiao_escolhidas_sao_as_certas() {
        let zonas = vec![
            zona(-300.0, (1, 3), vec![(0, 83), (1, 17)]),
            // Zona alta: empate vai pro kind maior (a ordem que o servidor manda).
            zona(100.0, (20, 25), vec![(1, 13), (0, 13), (7, 12)]),
            zona(400.0, (1, 3), vec![(0, 83), (1, 17)]),
        ];
        // Lobo (kind 0): a mais perto em que ele sai bem e que o nivel aguenta.
        let z = zona_mais_perto(&zonas, 0, vec2(200.0, 0.0), 1).unwrap();
        assert_eq!(
            z.centro[0], 400.0,
            "mandou pra zona alta so' por estar perto"
        );
        // Kind 7 so' sai na zona alta, abaixo da chance minima: a de maior chance.
        assert_eq!(
            zona_mais_perto(&zonas, 7, Vec2::ZERO, 1).unwrap().centro[0],
            100.0
        );
        assert!(zona_mais_perto(&zonas, 5, Vec2::ZERO, 1).is_none());
        let recursos = vec![regiao(-50.0, 3), regiao(30.0, 3), regiao(5.0, 0)];
        assert_eq!(
            regiao_mais_perto(&recursos, 3, vec2(10.0, 0.0))
                .unwrap()
                .centro[0],
            30.0
        );
        assert!(regiao_mais_perto(&recursos, 4, Vec2::ZERO).is_none());
        let info = InfoDaIlha {
            zonas,
            recursos,
            ..Default::default()
        };
        assert_eq!(info.faixa(0), Some((1, 3)));
        assert_eq!(info.tipos(), vec![0, 3]);
        assert_eq!(info.bichos()[0], 0);
    }

    #[test]
    fn tela_e_mundo_vao_e_voltam() {
        let r = Rect::new(100.0, 50.0, 400.0, 400.0);
        for p in [vec2(0.0, 0.0), vec2(-800.0, 800.0), vec2(123.0, -456.0)] {
            let t = para_tela(p, r, 800.0);
            assert!(de_tela(t, r, 800.0).distance(p) < 0.01);
        }
        // Norte fixo: z negativo (pra frente da camera em yaw 0) fica em cima.
        assert!(para_tela(vec2(0.0, -100.0), r, 800.0).y < para_tela(Vec2::ZERO, r, 800.0).y);
    }
}
