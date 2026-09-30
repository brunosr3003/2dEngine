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
            .unwrap_or_else(|| format!("Beast {kind}"))
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
        draw_rectangle(
            x - 1.0,
            corpo.y - dente - 1.0,
            dente * 0.66 + 2.0,
            dente + 2.0,
            preto,
        );
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
        0 => "Wood",
        1 => "Grey stone",
        2 => "Green stone",
        3 => "Blue stone",
        4 => "Purple stone",
        5 => "Energy",
        _ => "Node",
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
    /// O mapa nasce com a VILA e mais nada marcado.
    ///
    /// Tres versoes ate' aqui, e vale guardar a sequencia porque ela e' a
    /// razao do padrao de hoje:
    ///
    /// 1. tudo DESLIGADO — o jogador abria e via so' o contorno da ilha
    ///    ("mapa ta mt simples, n mostra recursos, n mostra casa porto");
    /// 2. recurso, energia e vila LIGADOS — virou o oposto, e o dono: "o
    ///    filtro padrao do mapa ta vindo com tudo clicado como ativo";
    /// 3. so' a VILA.
    ///
    /// A vila fica porque e' REFERENCIA: e' por ela que o jogador se situa, e
    /// mapa sem referencia e' desenho. Recurso e energia saem porque sao
    /// BUSCA — quem quer madeira liga madeira, e ate' entao aquilo e' ruido
    /// por cima do relevo.
    ///
    /// `mobs` desligado e `bichos_ocultos` vazio nao e' contradicao: o filtro
    /// agregado controla o que APARECE, e a lista controla o que fica
    /// ESCONDIDO quando ele e' ligado. Vazia, ligar "mobs" mostra todos de
    /// uma vez, que e' o que o dono pediu ("apenas os mobs, todos eles no
    /// caso, mas com o filtro mobs desativado"). Desligado, so' a zona de
    /// ALTA DENSIDADE aparece (`zona_visivel`) — foi o mob solto que poluia.
    fn default() -> Self {
        Self {
            mobs: false,
            bichos_ocultos: HashSet::new(),
            recursos: [false; 5],
            energia: false,
            vila: true,
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

#[cfg(test)]
mod testes_dos_filtros {
    use super::*;

    /// O padrão do SERVIDOR tem que bater com o do cliente.
    ///
    /// `From<&FiltrosDoMapa>` sobrescreve o `Filtros::default` do cliente com
    /// o que vem pelo fio, então quem manda de verdade é o `Default` do
    /// `FiltrosDoMapa` — e aquele era tudo `false`. O dono reclamou DUAS
    /// vezes que "o mapa não mostra recursos, não mostra casa e porto, apenas
    /// a ilha em si", e das duas vezes o conserto foi no lado que não manda.
    ///
    /// Este teste é o que impede a terceira.
    #[test]
    fn o_padrao_do_fio_e_o_padrao_do_cliente() {
        let do_fio = Filtros::from(&shared::protocol::FiltrosDoMapa::default());
        let do_cliente = Filtros::default();
        assert_eq!(
            do_fio.recursos, do_cliente.recursos,
            "recurso desligado no fio: o mapa volta a ser só o contorno da ilha"
        );
        assert_eq!(do_fio.energia, do_cliente.energia);
        assert_eq!(
            do_fio.vila, do_cliente.vila,
            "vila desligada no fio: porto e NPCs somem do mapa"
        );
        assert_eq!(do_fio.mobs, do_cliente.mobs);
        // E o que o padrão PROMETE hoje: SÓ a vila marcada.
        //
        // Recurso e energia já nasceram ligados e o dono reclamou ("o filtro
        // padrão do mapa tá vindo com tudo clicado como ativo"); antes disso
        // nasciam desligados junto com a vila, e ele reclamou do contrário.
        // O meio-termo é: referência sim, busca não.
        assert!(do_fio.vila, "vila é a referência: sem ela o mapa é desenho");
        assert!(!do_fio.energia, "energia é busca, não referência");
        assert!(
            do_fio.recursos.iter().all(|r| !*r),
            "recurso é busca: nasce desmarcado"
        );
        assert!(!do_fio.mobs, "mob solto é o que poluía a tela");
        // A lista de ocultos VAZIA é o par do `mobs: false`: o agregado
        // controla o que aparece, a lista o que fica escondido quando ele é
        // ligado. Vazia, ligar "mobs" mostra todos de uma vez — "apenas os
        // mobs, todos eles no caso, mas com o filtro mobs desativado".
        assert!(
            do_fio.bichos_ocultos.is_empty(),
            "nenhum bicho nasce escondido"
        );
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
    /// escondido: esconder "Wolf" tira as zonas de lobo.
    /// A zona de mob aparece no mapa?
    ///
    /// Com o filtro DESLIGADO (o padrao), so' as de ALTA DENSIDADE — as
    /// `forte`. O dono abriu o mapa e disse que "tá mt poluído os mons, é
    /// marcar somente área de alta densidade e boss": desenhar toda mancha de
    /// bicho enche a ilha de circulos, e no meio deles o CHEFE — que e' o que
    /// se procura num mapa — vira mais um ponto colorido.
    ///
    /// Ligando o filtro, tudo aparece: quem quer caçar bicho comum procura
    /// zona comum, e essa escolha continua sendo dele.
    pub fn zona_visivel(&self, z: &ZonaNoMapa) -> bool {
        (self.mobs || z.forte)
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
    Dungeon(u16),
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

/// Largura do painel de filtros e "Go to" ao lado do mapa grande.
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
    /// As portas de Porão da ilha (nome, posição). Separadas dos NPCs de
    /// propósito: ver `portas` em `Mapa`.
    portas: Vec<(&'static str, Vec2)>,
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
    let ger = Gerador::da_ilha(def);
    let rgba = gerar_imagem(&ger, def.raio_blocos, def.bioma);
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
    let mut npcs: Vec<(String, Vec2)> = vila
        .npcs
        .iter()
        .map(|n| (n.nome.to_string(), vec2(n.pos.x, n.pos.y)))
        .collect();
    if shared::magica::e_magica(def.zona) {
        let p = shared::magica::posto_dos_degraus();
        npcs.push((shared::magica::GUIA_DOS_DEGRAUS.to_string(), vec2(p.x, p.y)));
    }
    // AS PORTAS DE PORÃO ENTRAM NO MAPA.
    //
    // Sem isto elas eram invisíveis pra quem não tropeçasse nelas: a tarja só
    // acende a 26 unidades, e uma ilha tem centenas. O dono, ao procurar a
    // primeira: "it is in the world? where is it i cant find it in the map
    // menu". Uma dungeon que só se acha por acaso não está no mundo.
    // Roda na thread do mapa, uma vez por ilha: é o único lugar que pode
    // pagar o `Gerador` (24,6 ms) e a busca de chão das portas sem engasgar.
    let portas: Vec<(&'static str, Vec2)> = crate::porao_ui::portas_da_zona(def.zona)
        .into_iter()
        .filter_map(|(id, p)| shared::dungeon::conteudo(id).map(|c| (c.nome, p)))
        .collect();
    Dados {
        rgba,
        pegadas,
        porto,
        mestre,
        npcs,
        portas,
    }
}

/// A COLONIA (docs/COLONIA.md) no minimapa: o relevo E o que ha' nele.
///
/// Mandava `porto: None, mestre: None, npcs: vazio` — a ilha era desenhada e
/// NADA em cima dela. O dono abriu o mapa e disse que "nao esta' mostrando
/// nada": estava mostrando o chao, que e' o que menos ajuda. Os dois lugares
/// que importam na colonia sao o MURAL (onde se administra) e o CAIS (por
/// onde se sai), e eram justamente os que faltavam — inclusive o "Ir" deles.
///
/// As casas entram como pegadas, como na vila: o assentamento cresce, e o
/// mapa tem que mostrar que cresceu.
fn gerar_dados_da_colonia(plato: f32) -> Dados {
    let ger = Gerador::da_colonia(plato);
    let rgba = gerar_imagem(
        &ger,
        shared::colonia::RAIO_BLOCOS,
        shared::terreno::Bioma::Floresta,
    );
    let vila = ger.vila();
    let pegadas = vila
        .predios
        .iter()
        .map(|p| {
            let m = p.construcao().meia(p.yaw_q);
            (vec2(p.pos.x, p.pos.z), vec2(m.x, m.y))
        })
        .collect();
    // O cais vem da VILA, e nao do `SitioPorto` cru: e' ela que sabe onde a
    // ponta do molhe ficou (o mesmo caminho da ilha normal, logo acima).
    let porto = vila.porto.map(|p| PortoNoMapa {
        centro: vec2(p.centro.x, p.centro.y),
        raio: p.raio,
        raiz: vec2(p.raiz.x, p.raiz.y),
        ponta: vec2(p.ponta.x, p.ponta.y),
    });
    // O MURAL e' o ponto de referencia da ilha: entra como "mestre" porque e'
    // esse o marcador que o mapa ja' sabe desenhar e levar o jogador ate'.
    let mural = ger.cidade().map(|c| c.centro()).map(|c| vec2(c.x, c.y));
    let mut npcs = Vec::new();
    if let Some(m) = mural {
        npcs.push(("Island Board".to_string(), m));
    }
    if let Some(p) = &porto {
        npcs.push(("Boatman".to_string(), p.centro));
    }
    Dados {
        rgba,
        pegadas,
        porto,
        mestre: mural,
        npcs,
        portas: Vec::new(),
    }
}

/// RGBA da ilha inteira: o terreno E as casas. Roda FORA do quadro.
fn gerar_imagem(ger: &Gerador, raio_blocos: i32, bioma: shared::terreno::Bioma) -> Vec<u8> {
    let mut rgba = gerar_terreno(ger, raio_blocos, bioma);
    pintar_casas(&mut rgba, ger, raio_blocos as f32 * BLOCO);
    rgba
}

/// So' o relevo, sem nada construido em cima.
fn gerar_terreno(ger: &Gerador, raio_blocos: i32, bioma: shared::terreno::Bioma) -> Vec<u8> {
    let raio = raio_blocos as f32 * BLOCO;
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
                let cor = cor_de_terra(bioma, h, pico).map(|v| v * luz);
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

/// As CASAS, assadas na imagem do mapa.
///
/// Elas ja' eram desenhadas por cima, mas atras do filtro Vila — e o filtro
/// vem GUARDADO do servidor, entao ligar o padrao nao alcanca quem ja' tinha
/// personagem. O dono abriu o mapa e disse que ele "so' mostra o chao".
///
/// Aqui elas viram parte do desenho do terreno, como numa carta de verdade:
/// nao dependem de filtro, nao dependem de zoom e nao dependem de o jogador
/// descobrir uma caixinha. O filtro continua valendo pro RESTO da vila (o
/// contorno vivo, o porto, os NPCs).
/// OS NOMES DAS ILHOTAS, ESCRITOS NO MAPA DA ILHA MÁGICA.
///
/// O dono: "seria legal se no mapa da ilha mágica tivesse escrito qual é cada
/// ilha". Sem isto o mapa da ilha é sete manchas iguais: a tarja do HUD diz
/// onde você ESTÁ, mas nada dizia pra onde IR — e é o mapa que se abre quando
/// se quer trocar de ilhota antes de o relógio acabar.
///
/// O nome vai na BORDA DE CIMA da ilhota, e não no centro, porque o centro já
/// é de quem mora lá: o marcador do bicho dominante, o rótulo de nível da
/// zona e o ícone de chefe. Escrever por cima deles trocaria um mapa mudo por
/// um mapa ilegível.
///
/// A cor é a mesma da tarja (`magica_ui::cor_do_bonus`): o jogador aprende o
/// par cor↔ilhota uma vez e ele vale nos dois lugares.
/// Quais ilhotas recebem nome nesta escala, e com que nome.
///
/// Fora do desenho pra poder ser medido: desenhar exige tela, e o que decide
/// se o mapa fica legível ou ilegível é esta conta — quantos rótulos entram e
/// se eles são distintos entre si.
fn ilhotas_rotuladas(
    zona: Option<&str>,
    escala: f32,
) -> Vec<(shared::magica::Ilhota, &'static str)> {
    if !zona.is_some_and(shared::magica::e_magica) {
        return Vec::new();
    }
    shared::magica::ilhotas()
        .into_iter()
        // Ilhota pequena demais na tela não recebe nome: sete rótulos em cima
        // uns dos outros são pior que rótulo nenhum. É o mesmo critério que as
        // zonas de bicho já usam pro rótulo delas.
        .filter(|i| i.raio * escala >= u(16.0))
        .map(|i| (i, i.bonus.nome_curto()))
        .collect()
}

fn nomes_das_ilhotas(zona: Option<&str>, ponto: impl Fn(Vec2) -> Vec2, escala: f32, tamanho: u16) {
    for (i, nome) in ilhotas_rotuladas(zona, escala) {
        let rp = i.raio * escala;
        // O Vec2 do shared é de outra versão do glam.
        let q = ponto(vec2(i.centro.x, i.centro.y));
        let y = q.y - rp + u(12.0);
        // Sombra por baixo: o mapa é claro em praia e escuro em mata, e um
        // texto de cor só some numa das duas.
        estilo::texto_centro(
            q.x + 1.0,
            y + 1.0,
            nome,
            tamanho,
            Color::new(0.0, 0.0, 0.0, 0.85),
        );
        estilo::texto_centro(
            q.x,
            y,
            nome,
            tamanho,
            crate::magica_ui::cor_do_bonus(i.bonus),
        );
    }
}

fn pintar_casas(rgba: &mut [u8], ger: &Gerador, raio: f32) {
    // Telhado e parede: duas cores, senao um quarteirao inteiro vira uma
    // mancha so' e o mapa fica pior do que sem casa nenhuma.
    const TELHADO: [f32; 3] = [0.68, 0.34, 0.24];
    const PAREDE: [f32; 3] = [0.88, 0.80, 0.66];
    let px = |v: f32| ((v + raio) / (2.0 * raio) * LADO as f32).round() as i32;
    for pr in &ger.vila().predios {
        let m = pr.construcao().meia(pr.yaw_q);
        let (x0, x1) = (px(pr.pos.x - m.x), px(pr.pos.x + m.x));
        let (z0, z1) = (px(pr.pos.z - m.y), px(pr.pos.z + m.y));
        // Casa menor que um pixel some na conversao: uma ilha de 1,6 km cabe
        // em 384 pixels, e ai' 2,1 u por pixel apagariam o povoado inteiro.
        // Um pixel e' o minimo — melhor um ponto no lugar certo que nada.
        for j in z0.min(z1 - 1)..=z1.max(z0 + 1) {
            for i in x0.min(x1 - 1)..=x1.max(x0 + 1) {
                if !(0..LADO as i32).contains(&i) || !(0..LADO as i32).contains(&j) {
                    continue;
                }
                let borda = i <= x0 || i >= x1 || j <= z0 || j >= z1;
                let c = if borda { TELHADO } else { PAREDE };
                let k = (j as usize * LADO + i as usize) * 4;
                for (n, v) in c.iter().enumerate() {
                    rgba[k + n] = (v * 255.0) as u8;
                }
            }
        }
    }
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
    /// Raio e nome quando NAO ha' `DefIlha`: a colonia e' uma ilha de
    /// verdade sem entrada no `ARQUIPELAGO`.
    raio_sem_def: Option<f32>,
    nome_sem_def: String,
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
    /// NPCs da vila (da thread do mapa): o marcador e o "Go to".
    npcs: Vec<(String, Vec2)>,
    /// AS PORTAS DE PORÃO — sempre visíveis, e não dentro da lista de NPCs.
    ///
    /// A primeira versão empurrou as portas pra `npcs`, e `npcs` só aparece
    /// quando o jogador abre o toggle "NPCs" na lateral — fechado por padrão.
    /// Resultado: as portas ESTAVAM no mapa e ninguém as via. O dono: "i did
    /// not find anything in the map". Porta é conteúdo, como o porto e o
    /// chefe; entra sempre, com nome e com "Ir".
    portas: Vec<(&'static str, Vec2)>,
    /// A secao NPCs do "Go to" aberta (fechada por padrao: e' lista longa).
    npcs_abertos: bool,
    /// Zonas de mob e regioes de recurso (`MapaDaIlha`).
    pub info: Option<InfoDaIlha>,
    pub filtros: Filtros,
    /// A aba aberta: a ilha onde se esta', ou o arquipelago inteiro.
    pub no_mundo: bool,
    rolagem_lateral: crate::rolagem::Rolagem,
    /// Zoom do mapa GRANDE. 1 = a ilha inteira; acima disso, aproxima em
    /// volta do jogador.
    ///
    /// O minimapa ja' tinha zoom (`zoom_minimapa`); o mapa grande, nao — e e'
    /// nele que se procura coisa. Numa ilha de 1,6 km a tela inteira cabia
    /// num quadrado, e cada arvore era um pixel.
    ///
    /// Aproxima em volta do JOGADOR enquanto ninguem arrasta; a partir do
    /// primeiro arrasto, em volta de `centro`.
    zoom_grande: f32,
    /// O ponto que o mapa grande mostra no meio. `None` = segue o jogador.
    ///
    /// Era so' o jogador, e o comentario dizia que arrastar "briga com o toque
    /// que manda andar". Brigava porque o clique saia no APERTO: o dedo
    /// encostava e ja' viajava, entao nao havia arrasto possivel. Com o clique
    /// no SOLTAR (`GestoDoMapa`) os dois convivem, e o dono pediu o arrasto.
    centro: Option<Vec2>,
    gesto: GestoDoMapa,
}

/// O gesto do mapa grande: arrastar move a vista, dois dedos dao zoom, e
/// toque so' vira clique se o dedo NAO andou.
///
/// Vale pro mouse tambem — a macroquad entrega o dedo como botao esquerdo, e
/// separar os dois caminhos so' daria duas regras pra mesma coisa.
#[derive(Debug, Default)]
struct GestoDoMapa {
    /// Onde o dedo encostou, SE ele encostou com o mapa ja' aberto.
    ///
    /// Essa condicao e' o que impede o mapa de fechar sozinho: o toque que
    /// ABRE o mapa e' apertado com ele fechado, e o soltar chega um quadro
    /// depois, ja' aberto e fora do desenho — que e' a regra de fechar.
    de: Option<Vec2>,
    /// O aperto caiu dentro do desenho? So' ali o arrasto move a vista.
    no_desenho: bool,
    /// Onde o dedo estava no quadro passado.
    ultimo: Vec2,
    arrastou: bool,
    /// Distancia entre os dois dedos no quadro passado.
    pinca: Option<f32>,
    /// Houve dois dedos: nada de clique ate' soltar tudo.
    cancelado: bool,
}

impl Default for Mapa {
    fn default() -> Self {
        Self {
            def: None,
            ger: None,
            cidade: None,
            portas: Vec::new(),
            raio_sem_def: None,
            nome_sem_def: String::new(),
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
            no_mundo: false,
            rolagem_lateral: Default::default(),
            zoom_grande: 1.0,
            centro: None,
            gesto: GestoDoMapa::default(),
        }
    }
}

/// Prévia do MAPA GRANDE (`MMO_PREVIA_MAPA=1`; PNGs em `MMO_PREVIA_SAIDA`).
///
/// Existe porque as portas de Porão "estavam no mapa" e ninguém as via: foram
/// parar na lista de NPCs, que nasce fechada. Uma prévia do mapa teria
/// mostrado isso antes de o dono procurar no jogo. Renderiza a ilha inicial
/// com a lateral aberta, que é o que o jogador vê ao apertar o mapa.
#[cfg(debug_assertions)]
pub async fn previa() {
    let saida =
        std::env::var("MMO_PREVIA_SAIDA").unwrap_or_else(|_| "/tmp/tempest-mapa-preview".into());
    std::fs::create_dir_all(&saida).unwrap();
    let rt = render_target(1920, 1080);
    crate::render3d::define_alvo(Some(rt.clone()));
    crate::hud_layout::define_escala_ui(1.6);
    let world = crate::world::World::default();
    let mundo = crate::mundo_ui::Mundo::default();
    for zona in ["ilha_inicial", "ilha_deserto"] {
        let def = shared::terreno::def_da_zona(zona).expect("zona");
        let mut m = Mapa::para(Some(def));
        // A textura nasce numa thread; espera ela chegar.
        for _ in 0..600 {
            m.acompanhar();
            if m.tex.is_some() {
                break;
            }
            next_frame().await;
        }
        assert!(m.tex.is_some(), "{zona}: o mapa não montou");
        m.aberto = true;
        // A lateral só desenha com o `info` do servidor (zonas de bicho); sem
        // ele fica em "loading zones…" e a lista de "Ir" — que é onde as
        // portas aparecem pra quem procura — nem existe. Um `info` vazio
        // basta: as vilas e as portas são locais.
        m.info = Some(Default::default());
        for _ in 0..3 {
            crate::render3d::camera_padrao();
            clear_background(Color::new(0.08, 0.12, 0.16, 1.0));
            m.desenha_grande(&world, 20, &mundo, 1_789_000_000);
            unsafe { get_internal_gl().flush() };
            rt.texture.get_texture_data().export_png(&format!("{saida}/mapa-{zona}.png"));
            next_frame().await;
        }
    }
}

/// O marcador de uma porta de Porão no mapa.
///
/// Três candidatos, escolhidos olhando (`MMO_PREVIA_ICONE_PORTA=1` desenha os
/// três lado a lado, no tamanho real do mapa e do minimapa):
///
/// * `Arco`     — um arco de pedra com o vão escuro: a porta vista de frente.
/// * `Fechadura`— um buraco de fechadura: "isto se abre com chave".
/// * `Cadeado`  — um cadeado fechado: o mesmo recado, mais gordo e mais
///                legível a 6 px.
///
/// `ICONE_PORTA` diz qual está em uso. O nome vai ao lado e alterna de lado
/// por porta, porque duas portas a 56 u da cidade viram um borrão se o nome
/// ficar em cima (visto na prévia do mapa em 29/09/2026).
#[derive(Clone, Copy, PartialEq)]
pub enum IconePorta {
    Arco,
    Fechadura,
    Cadeado,
    /// Escada descendo num alçapão: "porão" é pra baixo.
    Escada,
    /// Caveira: perigo, dungeon.
    Caveira,
    /// Espadas cruzadas sobre escudo: combate.
    Espadas,
    /// Boca de caverna escura com brilho: entrada.
    Caverna,
}

pub const ICONE_PORTA: IconePorta = IconePorta::Caverna;

fn porta_no_mapa(q: Vec2, lado: f32, nome: &str, fonte: u16, direita: bool) {
    icone_da_porta(ICONE_PORTA, q, lado);
    let cor = estilo::OURO;
    let largura = estilo::medir(nome, fonte);
    let x = if direita { q.x + lado + 6.0 } else { q.x - lado - 6.0 - largura };
    estilo::texto(x, q.y + fonte as f32 * 0.35, nome, fonte, cor);
}

/// Só o desenho, sem nome: é o que a prévia dos ícones compara.
pub fn icone_da_porta(qual: IconePorta, q: Vec2, lado: f32) {
    let ouro = estilo::OURO;
    let escuro = Color::new(0.05, 0.05, 0.08, 1.0);
    let contorno = Color::new(0.0, 0.0, 0.0, 0.75);
    match qual {
        IconePorta::Arco => {
            // Contorno escuro pra destacar do verde/areia, depois o arco.
            draw_rectangle(q.x - lado - 1.5, q.y - lado * 0.4 - 1.5, lado * 2.0 + 3.0, lado * 1.6 + 3.0, contorno);
            draw_circle(q.x, q.y - lado * 0.4, lado + 1.5, contorno);
            draw_rectangle(q.x - lado, q.y - lado * 0.4, lado * 2.0, lado * 1.6, ouro);
            draw_circle(q.x, q.y - lado * 0.4, lado, ouro);
            // O vão.
            draw_rectangle(q.x - lado * 0.45, q.y - lado * 0.3, lado * 0.9, lado * 1.5, escuro);
            draw_circle(q.x, q.y - lado * 0.3, lado * 0.45, escuro);
        }
        IconePorta::Fechadura => {
            draw_circle(q.x, q.y, lado * 1.35, contorno);
            draw_circle(q.x, q.y, lado * 1.2, ouro);
            // O buraco: círculo em cima, cunha embaixo.
            draw_circle(q.x, q.y - lado * 0.25, lado * 0.42, escuro);
            draw_triangle(
                vec2(q.x - lado * 0.22, q.y - lado * 0.05),
                vec2(q.x + lado * 0.22, q.y - lado * 0.05),
                vec2(q.x, q.y + lado * 0.75),
                escuro,
            );
            draw_rectangle(q.x - lado * 0.22, q.y - lado * 0.1, lado * 0.44, lado * 0.55, escuro);
        }
        IconePorta::Escada => {
            let r = lado * 1.25;
            draw_circle(q.x, q.y, r + 1.5, contorno);
            draw_circle(q.x, q.y, r, Color::new(0.16, 0.12, 0.09, 1.0));
            // Degraus descendo, cada um mais escuro.
            for i in 0..4 {
                let t = i as f32 / 4.0;
                let w = r * (1.5 - t * 0.9);
                let y = q.y - r * 0.55 + t * r * 1.05;
                let c = Color::new(ouro.r * (1.0 - t * 0.55), ouro.g * (1.0 - t * 0.55), ouro.b * (1.0 - t * 0.55), 1.0);
                draw_rectangle(q.x - w * 0.5, y, w, r * 0.2, c);
            }
            draw_circle_lines(q.x, q.y, r, 1.5f32.max(lado * 0.18), ouro);
        }
        IconePorta::Caveira => {
            let r = lado * 1.05;
            draw_circle(q.x, q.y - r * 0.1, r + 1.5, contorno);
            draw_rectangle(q.x - r * 0.55 - 1.5, q.y + r * 0.35, r * 1.1 + 3.0, r * 0.6 + 1.5, contorno);
            draw_circle(q.x, q.y - r * 0.1, r, ouro);
            draw_rectangle(q.x - r * 0.55, q.y + r * 0.35, r * 1.1, r * 0.6, ouro);
            draw_circle(q.x - r * 0.4, q.y - r * 0.05, r * 0.28, escuro);
            draw_circle(q.x + r * 0.4, q.y - r * 0.05, r * 0.28, escuro);
            draw_triangle(vec2(q.x, q.y + r * 0.2), vec2(q.x - r * 0.12, q.y + r * 0.42), vec2(q.x + r * 0.12, q.y + r * 0.42), escuro);
            for dx in [-0.3f32, 0.0, 0.3] {
                draw_line(q.x + r * dx, q.y + r * 0.55, q.x + r * dx, q.y + r * 0.95, 1.0f32.max(lado * 0.1), escuro);
            }
        }
        IconePorta::Espadas => {
            let r = lado * 1.3;
            // Escudo.
            let escudo = |c: Color, k: f32| {
                draw_rectangle(q.x - r * 0.65 * k, q.y - r * 0.7 * k, r * 1.3 * k, r * 0.8 * k, c);
                draw_triangle(vec2(q.x - r * 0.65 * k, q.y + r * 0.1 * k), vec2(q.x + r * 0.65 * k, q.y + r * 0.1 * k), vec2(q.x, q.y + r * 0.85 * k), c);
            };
            escudo(contorno, 1.15);
            escudo(Color::new(0.45, 0.12, 0.10, 1.0), 1.0);
            let g = 2.0f32.max(lado * 0.28);
            draw_line(q.x - r * 0.75, q.y - r * 0.75, q.x + r * 0.75, q.y + r * 0.6, g + 2.0, contorno);
            draw_line(q.x + r * 0.75, q.y - r * 0.75, q.x - r * 0.75, q.y + r * 0.6, g + 2.0, contorno);
            draw_line(q.x - r * 0.75, q.y - r * 0.75, q.x + r * 0.75, q.y + r * 0.6, g, ouro);
            draw_line(q.x + r * 0.75, q.y - r * 0.75, q.x - r * 0.75, q.y + r * 0.6, g, ouro);
        }
        IconePorta::Caverna => {
            let r = lado * 1.3;
            // Rocha em volta, boca escura, brilho roxo no fundo.
            draw_circle(q.x, q.y, r + 1.5, contorno);
            draw_circle(q.x, q.y, r, Color::new(0.42, 0.40, 0.44, 1.0));
            draw_circle(q.x, q.y + r * 0.2, r * 0.65, escuro);
            draw_rectangle(q.x - r * 0.65, q.y + r * 0.2, r * 1.3, r * 0.75, escuro);
            draw_circle(q.x, q.y + r * 0.35, r * 0.3, Color::new(0.75, 0.45, 1.0, 0.9));
            draw_circle_lines(q.x, q.y, r, 1.5f32.max(lado * 0.15), ouro);
        }
        IconePorta::Cadeado => {
            let w = lado * 1.8;
            let h = lado * 1.4;
            let topo = q.y - lado * 0.2;
            // Contorno.
            draw_rectangle(q.x - w * 0.5 - 1.5, topo - 1.5, w + 3.0, h + 3.0, contorno);
            draw_circle_lines(q.x, topo - lado * 0.15, lado * 0.62, lado * 0.42 + 3.0, contorno);
            // A alça.
            draw_circle_lines(q.x, topo - lado * 0.15, lado * 0.62, lado * 0.42, ouro);
            draw_rectangle(q.x - w * 0.5, topo - lado * 0.15, w, lado * 0.3, ouro);
            // O corpo.
            draw_rectangle(q.x - w * 0.5, topo, w, h, ouro);
            // O buraco.
            draw_circle(q.x, topo + h * 0.42, lado * 0.28, escuro);
            draw_rectangle(q.x - lado * 0.12, topo + h * 0.42, lado * 0.24, h * 0.38, escuro);
        }
    }
}

/// Prévia dos ícones de porta (`MMO_PREVIA_ICONE_PORTA=1`): os três, no
/// tamanho do mapa grande (6 px) e do minimapa (~11 px), sobre floresta e
/// sobre areia — os dois fundos em que eles têm que ser lidos.
#[cfg(debug_assertions)]
pub async fn previa_icones() {
    let saida = std::env::var("MMO_PREVIA_SAIDA")
        .unwrap_or_else(|_| "/tmp/tempest-icone-porta".into());
    std::fs::create_dir_all(&saida).unwrap();
    let (w, h) = (1400.0f32, 560.0f32);
    let rt = render_target(w as u32, h as u32);
    crate::render3d::define_alvo(Some(rt.clone()));
    crate::hud_layout::define_escala_ui(1.6);
    let floresta = Color::from_rgba(78, 128, 58, 255);
    let areia = Color::from_rgba(214, 190, 140, 255);
    let icones = [
        (IconePorta::Escada, "1 Escada"),
        (IconePorta::Caveira, "2 Caveira"),
        (IconePorta::Espadas, "3 Espadas"),
        (IconePorta::Caverna, "4 Caverna"),
        (IconePorta::Arco, "5 Arco"),
        (IconePorta::Fechadura, "6 Fechadura"),
        (IconePorta::Cadeado, "7 Cadeado (atual)"),
    ];
    for _ in 0..3 {
        crate::render3d::camera_padrao();
        clear_background(Color::new(0.08, 0.12, 0.16, 1.0));
        draw_rectangle(0.0, 0.0, w, h * 0.5, floresta);
        draw_rectangle(0.0, h * 0.5, w, h * 0.5, areia);
        for (i, (ic, nome)) in icones.iter().enumerate() {
            let cx = 100.0 + i as f32 * 200.0;
            estilo::texto_centro(cx, 34.0, nome, 18, WHITE);
            for fy in [0.0f32, h * 0.5] {
                icone_da_porta(*ic, vec2(cx, fy + 110.0), 40.0);
                icone_da_porta(*ic, vec2(cx - 40.0, fy + 220.0), 6.0);
                icone_da_porta(*ic, vec2(cx + 30.0, fy + 220.0), 11.2);
            }
        }
        unsafe { get_internal_gl().flush() };
        rt.texture.get_texture_data().export_png(&format!("{saida}/icones-porta.png"));
        next_frame().await;
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

    /// O minimapa da COLONIA: mesmo desenho, gerador do personagem.
    pub fn da_colonia(plato: f32) -> Self {
        let raio_blocos = shared::colonia::RAIO_BLOCOS;
        let mut m = Self::default();
        let (tx, rx) = channel();
        std::thread::spawn(move || {
            let _ = tx.send(gerar_dados_da_colonia(plato));
        });
        let ger = Gerador::da_colonia(plato);
        // A PRACA. `Mapa::para` define isto e o `da_colonia` esquecia: sem ela
        // o minimapa da colonia nao sabe onde e' o centro do assentamento.
        m.cidade = ger.cidade();
        m.ger = Some(ger);
        m.raio_sem_def = Some(raio_blocos as f32 * BLOCO);
        m.nome_sem_def = "My Island".into();
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
            self.portas = dados.portas;
            let t = Texture2D::from_rgba8(LADO as u16, LADO as u16, &dados.rgba);
            t.set_filter(FilterMode::Linear);
            self.tex = Some(t);
            self.rx = None;
        }
    }

    pub fn tem_ilha(&self) -> bool {
        self.def.is_some() || self.raio_sem_def.is_some()
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
            .find(|n| {
                n.giver
                    .or_else(|| shared::quests::giver_do_npc(n.papel as u16))
                    == Some(giver)
            })
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
    /// `foco` e' o mesmo centro que o DESENHO usou (`foco_do_zoom`).
    ///
    /// Vem por parametro, e nao calculado aqui: projetar duas vezes o mesmo
    /// ponto e' como o marcador acaba desenhado num lugar e clicado noutro.
    fn marcador_sob(&self, m: Vec2, r: Rect, foco: Vec2) -> Option<Marcador> {
        let raio = self.raio() / self.zoom_grande;
        let escala = r.w / (2.0 * raio);
        let k = Self::escala();
        // NPC primeiro: e' o menor, dentro da cidade, por cima de tudo.
        if self.filtros.vila {
            let npc = self
                .npcs_da_vila()
                .iter()
                .enumerate()
                .map(|(i, (_, p))| (i, para_tela(*p - foco, r, raio).distance(m)))
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
                    para_tela(vec2(g.centro[0], g.centro[1]) - foco, r, raio).distance(m),
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
                    para_tela(centro_da_zona(z) - foco, r, raio).distance(m),
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
                linhas.push(("click: go talk (or teleport)".into(), estilo::AUTO));
            }
            Marcador::Zona(i) => {
                let Some(z) = info.zonas.get(i) else { return };
                linhas.push((
                    format!(
                        "{} · Nv {}–{}",
                        if z.forte { "STRONGHOLD" } else { "Hunting zone" },
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
                        "many more enemies in the same space".into(),
                        estilo::VERMELHO,
                    ));
                }
                for (k, c) in &z.bichos {
                    linhas.push((format!("{}  {c}%", info.nome(*k)), cor_do_bicho(*k)));
                }
                linhas.push(("click: go hunt (auto combat)".into(), estilo::AUTO));
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
                linhas.push(("click: go gather (auto gather)".into(), estilo::AUTO));
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

    /// Filtros e "Go to", ao lado do mapa grande. Devolve o "Ir" clicado.
    fn desenha_lateral(&mut self, r: Rect, eu: Option<Vec2>, nivel: u32) -> Option<Entrada> {
        let lat = Self::lateral_rect(r);
        estilo::painel(lat);
        let Some(info) = self.info.as_ref() else {
            estilo::texto(
                lat.x + u(14.0),
                lat.y + u(28.0),
                "loading zones…",
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
            "Filters",
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
        chips.push(("Village".into(), self.filtros.vila, estilo::OURO, Chip::Vila));
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
        estilo::texto(lat.x + u(14.0), y, "Go to", 16, estilo::OURO);
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
        // VILAS: a praça e o cais. Vão na lista como tudo o mais, com o
        // mesmo botão "Ir" — antes elas só existiam como desenho no mapa, e
        // chegar nelas exigia adivinhar onde tocar.
        let vilas = self.vilas_da_ilha();
        let total =
            (4 + vilas.len() + bichos.len() + tipos.len() + npcs.len()) as f32 * u(LINHA_IR);
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
        // VILAS primeiro: é para onde se volta, e é o que o jogador procura
        // quando está perdido no meio da ilha.
        if visivel(ly) {
            estilo::texto(area.x + u(6.0), ly + u(18.0), "Villages", 13, estilo::SUAVE);
        }
        ly += u(LINHA_IR);
        for (nome, p, raio) in &vilas {
            let alvo = Alvo {
                objetivo: Objetivo::Lugar,
                pos: *p,
                raio: *raio,
                rotulo: nome.to_string(),
            };
            linha(
                nome,
                format!("{:.0} m", p.distance(eu)),
                estilo::OURO,
                Some(alvo),
                ly,
                &mut saida,
            );
            ly += u(LINHA_IR);
        }
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
            estilo::texto(area.x + u(6.0), ly + u(18.0), "Beasts", 13, estilo::SUAVE);
        }
        ly += u(LINHA_IR);
        for k in &bichos {
            let nome = info.nome(*k);
            let z = zona_mais_perto(&info.zonas, *k, eu, nivel);
            let faixa = info
                .faixa(*k)
                .map_or(String::new(), |(a, b)| format!("Lv {a}–{b} · "));
            let detalhe = z.map_or("no zone".to_string(), |z| {
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
            estilo::texto(area.x + u(6.0), ly + u(18.0), "Nodes", 13, estilo::SUAVE);
        }
        ly += u(LINHA_IR);
        for t in &tipos {
            let g = regiao_mais_perto(&info.recursos, *t, eu);
            let n = info.recursos.iter().filter(|r| r.tipo == *t).count();
            let detalhe = g.map_or("—".to_string(), |g| {
                format!(
                    "{n} regions · {:.0} m",
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

    /// Onde o zoom do mapa grande fica centrado: o JOGADOR, preso dentro da
    /// ilha pra a janela nunca sair do desenho.
    ///
    /// Sem jogador (mapa aberto antes de entrar no mundo), o centro da ilha.
    /// A pinca deste quadro, como FATOR de zoom. `None` = nao houve.
    fn pinca_do_quadro(&mut self) -> Option<f32> {
        let dedos: Vec<Vec2> = touches()
            .iter()
            .filter(|t| t.phase != TouchPhase::Ended)
            .map(|t| t.position)
            .collect();
        if dedos.len() < 2 {
            self.gesto.pinca = None;
            // Soltar TUDO destrava o clique. Enquanto sobrar dedo na tela
            // depois da pinca, o toque nao vale — senao tirar um dedo de cada
            // vez terminaria mandando o jogador andar pro meio do mapa.
            if touches().is_empty() {
                self.gesto.cancelado = false;
            }
            return None;
        }
        self.gesto.de = None;
        self.gesto.cancelado = true;
        let d = dedos[0].distance(dedos[1]);
        let antes = self.gesto.pinca.replace(d)?;
        (antes > 1.0 && (d - antes).abs() > 0.5).then(|| d / antes)
    }

    /// Quanto o dedo arrastou o DESENHO neste quadro, em pixels.
    fn arrasto_do_quadro(&mut self, r: Rect) -> Option<Vec2> {
        let m = Vec2::from(mouse_position());
        if is_mouse_button_pressed(MouseButton::Left) {
            self.gesto.de = Some(m);
            self.gesto.no_desenho = r.contains(m);
            self.gesto.ultimo = m;
            self.gesto.arrastou = false;
            return None;
        }
        if self.gesto.cancelado || !self.gesto.no_desenho {
            return None;
        }
        let de = self.gesto.de?;
        if !is_mouse_button_down(MouseButton::Left) {
            return None;
        }
        let d = m - self.gesto.ultimo;
        self.gesto.ultimo = m;
        // Ate' o limiar nada acontece: um toque tremido ainda e' um toque, e
        // e' esse mesmo limiar que o resto do jogo usa (`toque::TOLERANCIA_PX`).
        if !self.gesto.arrastou && m.distance(de) <= crate::toque::TOLERANCIA_PX {
            return None;
        }
        self.gesto.arrastou = true;
        (d != Vec2::ZERO).then_some(d)
    }

    /// Soltou sem ter arrastado — o clique do mapa grande.
    fn soltou_clicando(&mut self, _r: Rect) -> bool {
        if !is_mouse_button_released(MouseButton::Left) {
            return false;
        }
        let vale = self.gesto.de.is_some() && !self.gesto.arrastou && !self.gesto.cancelado;
        self.gesto.de = None;
        self.gesto.arrastou = false;
        self.gesto.no_desenho = false;
        // O mesmo portao do `foco::clique`: com o tutorial apontando pra um
        // lugar, toque fora dele nao conta.
        vale && crate::foco::passa_ponto(Vec2::from(mouse_position()))
    }

    /// A moldura do mapa grande: a faixa acima do desenho, onde ficam o
    /// titulo, o X e as abas Ilha | Mundo.
    fn moldura_rect(r: Rect) -> Rect {
        Rect::new(r.x - u(8.0), r.y - u(38.0), r.w + u(16.0), u(38.0))
    }

    fn foco_do_zoom(&self, eu: Option<Vec2>) -> Vec2 {
        if self.zoom_grande <= 1.001 {
            return Vec2::ZERO;
        }
        // Preso dentro da ilha: sem isto, aproximar perto da costa mostraria
        // metade de oceano fora do desenho.
        let meia = self.raio() * (1.0 - 1.0 / self.zoom_grande);
        self.centro
            .or(eu)
            .unwrap_or(Vec2::ZERO)
            .clamp(Vec2::splat(-meia), Vec2::splat(meia))
    }

    /// Quantas unidades de mundo cabem num pixel do mapa grande.
    fn unidade_por_pixel(&self, r: Rect) -> f32 {
        2.0 * self.raio() / self.zoom_grande / r.w.max(1.0)
    }

    /// Arrasta a vista. O conteudo segue o dedo, entao o foco anda ao
    /// CONTRARIO do arrasto.
    fn arrastar(&mut self, d: Vec2, r: Rect, eu: Option<Vec2>) {
        // O primeiro arrasto congela o foco onde ele esta': sem isto a vista
        // voltaria pro jogador no quadro seguinte e o mapa pareceria elastico.
        let base = self.centro.unwrap_or_else(|| self.foco_do_zoom(eu));
        self.centro = Some(base - d * self.unidade_por_pixel(r));
    }

    /// Zoom continuo da pinca. Multiplicativo, pra o gesto responder igual
    /// perto e longe.
    fn zoom_por_fator(&mut self, f: f32, eu: Option<Vec2>) {
        let antes = self.zoom_grande;
        self.zoom_grande = (self.zoom_grande * f).clamp(1.0, 8.0);
        // Abrir a pinca a partir da ilha inteira tem que aproximar de ALGUM
        // lugar: sem fixar o centro aqui, o primeiro zoom pularia pro jogador.
        if antes <= 1.001 && self.zoom_grande > 1.001 && self.centro.is_none() {
            // `eu` = None (os botoes − / +) deixa o centro solto de proposito:
            // ai' vale a regra antiga, que segue o jogador.
            self.centro = eu;
        }
        if self.zoom_grande <= 1.001 {
            self.centro = None;
        }
    }

    /// Aproxima ou afasta o mapa grande. Um toque = um degrau.
    pub fn zoom_do_mapa(&mut self, perto: bool) {
        self.zoom_por_fator(if perto { 1.5 } else { 1.0 / 1.5 }, None);
    }

    fn raio(&self) -> f32 {
        match self.def {
            Some(d) => d.raio_blocos as f32 * BLOCO,
            None => self.raio_sem_def.unwrap_or(1.0),
        }
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

    /// O mapa quadrado, com o painel lateral de filtros e "Go to" a' direita.
    /// As duas abas em cima do mapa: Ilha | Mundo.
    fn abas_rect(r: Rect) -> (Rect, Rect) {
        let k = Self::escala();
        let (w, h) = (92.0 * k, 26.0 * k);
        let y = r.y - 34.0 * k;
        (
            Rect::new(r.x, y, w, h),
            Rect::new(r.x + w + 6.0 * k, y, w, h),
        )
    }

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
    /// Os povoados desta ilha: a praça e o cais.
    ///
    /// São `(nome, centro, raio)`. O raio existe pra o "Ir" parar na BORDA do
    /// povoado em vez de mirar o ponto exato — chegar na praça é chegar na
    /// praça, não pisar no meio dela.
    pub fn vilas_da_ilha(&self) -> Vec<(&'static str, Vec2, f32)> {
        let mut v = Vec::new();
        // O `Cidade` vem do `shared` (glam próprio); o resto do mapa usa o
        // Vec2 da macroquad. São dois `glam` no grafo de dependências.
        if let Some(c) = self.cidade {
            let p = c.centro();
            v.push(("Village square", vec2(p.x, p.y), 10.0));
        }
        if let Some(p) = self.porto {
            v.push(("Port", vec2(p.centro.x, p.centro.y), p.raio.max(8.0)));
        }
        // "Go" to the oasis lands on its dry shore, not in the pond.
        if let Some(o) = self.ger.as_ref().and_then(|g| g.oasis()) {
            let margem = shared::oasis::RAIO_MARGEM - 1.5;
            v.push(("Oasis", vec2(o.centro.x + margem, o.centro.y), 4.0));
        }
        // As portas de Porão, com o mesmo "Ir" da praça e do cais. O raio é o
        // de abrir a porta: o "Ir" para onde o botão de abrir já acende.
        for (nome, p) in &self.portas {
            v.push((nome, *p, shared::porao::ALCANCE_DA_PORTA));
        }
        v
    }

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

    /// Os dois botoes de zoom do mapa GRANDE, no canto de baixo do desenho:
    /// (− , +). Longe do X pra nao fechar o mapa querendo afastar.
    fn zoom_grande_rects(r: Rect) -> (Rect, Rect) {
        let k = Self::escala();
        let l = 34.0 * k;
        let y = r.y + r.h - l - 8.0 * k;
        (
            Rect::new(r.x + 8.0 * k, y, l, l),
            Rect::new(r.x + 8.0 * k + l + 6.0 * k, y, l, l),
        )
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
            // PINCA e ARRASTO antes do clique: o clique do mapa grande passou
            // a sair no SOLTAR, e quem decide se houve clique e' o gesto.
            if let Some(f) = self.pinca_do_quadro() {
                self.zoom_por_fator(f, eu);
                return None;
            }
            if let Some(d) = self.arrasto_do_quadro(r) {
                self.arrastar(d, r, eu);
                return None;
            }
            if self.soltou_clicando(r) {
                // O painel lateral trata os proprios botoes no desenho.
                if Self::lateral_rect(r).contains(m) {
                    return None;
                }
                // Fora do mapa ou no X: fecha — MENOS na moldura de cima, que
                // tem as abas Ilha|Mundo. Elas ficam ACIMA do `grande_rect`, e
                // sem esta excecao tocar em "World" trocava a aba e fechava o
                // mapa no mesmo quadro: o dono via o mapa sumir.
                if Self::fechar_rect(r).contains(m)
                    || (!r.contains(m) && !Self::moldura_rect(r).contains(m))
                {
                    self.aberto = false;
                    return None;
                }
                if Self::moldura_rect(r).contains(m) {
                    return None;
                }
                // ZOOM antes de tudo: os botoes ficam DENTRO do desenho, e
                // sem isto o toque neles viraria "andar pra la'".
                let (menos, mais) = Self::zoom_grande_rects(r);
                if menos.contains(m) {
                    self.zoom_do_mapa(false);
                    return None;
                }
                if mais.contains(m) {
                    self.zoom_do_mapa(true);
                    return None;
                }
                let foco = self.foco_do_zoom(eu);
                if let Some(a) = self
                    .marcador_sob(m, r, foco)
                    .and_then(|mk| self.alvo_do_marcador(mk))
                {
                    return Some(Entrada::Ir(a));
                }
                // O destino sai da MESMA janela que o desenho usa: sem somar
                // o foco, aproximar mandaria o jogador pro lugar errado.
                let raio_v = self.raio() / self.zoom_grande;
                let destino = de_tela(m, r, raio_v) + foco;
                return Some(Entrada::Viajar(destino));
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
            None => estilo::texto_centro(c.x, c.y + 30.0, "loading map…", 12, estilo::SUAVE),
        }
        let ponto = |p: Vec2| c + (p - eu) * escala;
        // O DISCO é que manda, não o retângulo.
        //
        // O minimapa é redondo, mas `visivel` testava a caixa em volta dele:
        // marcador nos cantos aparecia FORA do disco, boiando no HUD. O dono:
        // "no mapa e no minimapa tá dando pra ver os ícones mesmo fora do
        // mapa / minimapa".
        let visivel = |q: Vec2| q.distance(c) <= rad;
        // E o RECORTE por baixo, pra quem desenha forma com tamanho: um
        // círculo de zona ou uma linha de rota não cabem num teste de ponto,
        // e vazavam pela borda mesmo com o centro dentro. O `scissor` é
        // retangular (é o que o GL dá), então ele corta o excesso grosseiro e
        // o teste do disco cuida do resto.
        crate::rolagem::recortar(Some(dentro));
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
        // As portas de Porão: SEM filtro. Porta é conteúdo, não decoração da
        // vila; some do mapa só quando a ilha não tem nenhuma.
        for (i, (nome, p)) in self.portas.iter().enumerate() {
            let q = ponto(*p);
            if visivel(q) {
                porta_no_mapa(q, (6.0f32).max(escala * 3.0), nome, 13, i % 2 == 0);
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
        // No minimapa também: é o mapa que fica na tela o tempo todo, e
        // trocar de ilhota é decisão que se toma andando. Fonte menor, e o
        // corte por tamanho de `nomes_das_ilhotas` cuida de calar quando o
        // disco está afastado demais pra caber sete rótulos.
        nomes_das_ilhotas(self.zona(), ponto, escala, 12);
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
        crate::rolagem::recortar(None);
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
        estilo::arco(c, rad, 0.0, 1.0, 1.0, estilo::alfa(estilo::OURO, 0.50));
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

    pub fn desenha_grande(
        &mut self,
        world: &World,
        nivel: u32,
        mundo: &crate::mundo_ui::Mundo,
        agora_unix: i64,
    ) -> Option<Entrada> {
        if !self.aberto || !self.tem_ilha() {
            return None;
        }
        estilo::no_painel(Self::escala(), || {
            let m = Vec2::from(mouse_position());
            let r = Self::grande_rect();
            if self.no_mundo {
                self.desenha_grande_mundo(mundo, agora_unix);
                self.desenha_abas(r);
                return self.lateral_do_mundo(r, mundo, agora_unix);
            }
            self.desenha_grande_mapa(world, agora_unix);
            self.desenha_abas(r);
            // Tutorial "abra o mapa e toque num lugar": o alvo e' o mapa todo.
            crate::foco::marca(crate::foco::chave::MAPA_IR, r);
            if r.contains(m) {
                if let Some(mk) = self.marcador_sob(m, r, self.foco_do_zoom(world.self_pos())) {
                    self.dica(mk, m);
                }
            }
            self.desenha_lateral(r, world.self_pos(), nivel)
        })
    }

    /// As abas Ilha | Mundo. Clicar troca a vista.
    fn desenha_abas(&mut self, r: Rect) {
        let (ilha, mundo) = Self::abas_rect(r);
        let m = Vec2::from(mouse_position());
        let clicou = crate::foco::clique();
        for (caixa, rotulo, ativa) in [
            (ilha, "Island", !self.no_mundo),
            (mundo, "World", self.no_mundo),
        ] {
            estilo::botao(caixa, rotulo, estilo::estado_de(caixa, false, ativa), ativa);
            if clicou && caixa.contains(m) {
                self.no_mundo = rotulo == "World";
            }
        }
    }

    /// O MAPA-MUNDI no lugar do mapa da ilha (`mundo_ui`).
    fn desenha_grande_mundo(&self, mundo: &crate::mundo_ui::Mundo, agora_unix: i64) {
        let (sw, sh) = (screen_width(), screen_height());
        draw_rectangle(0.0, 0.0, sw, sh, Color::new(0.0, 0.0, 0.0, 0.45));
        let r = Self::grande_rect();
        estilo::painel(Rect::new(
            r.x - u(8.0),
            r.y - u(38.0),
            r.w + u(16.0),
            r.h + u(46.0),
        ));
        estilo::texto_forte(r.x, r.y - u(14.0), "World", 17, estilo::OURO);
        let dica = "bosses from every island · Esc closes";
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
        crate::mundo_ui::desenha(mundo, r, self.zona(), agora_unix, u);
    }

    /// O painel lateral do mapa-mundi: os chefes do realm, ordenados por
    /// quem da' pra caçar primeiro.
    fn lateral_do_mundo(
        &mut self,
        r: Rect,
        mundo: &crate::mundo_ui::Mundo,
        agora_unix: i64,
    ) -> Option<Entrada> {
        let lat = Self::lateral_rect(r);
        estilo::painel(lat);
        estilo::texto_forte(lat.x + u(14.0), lat.y + u(26.0), "Bosses", 16, estilo::OURO);
        if mundo.vazio() {
            estilo::texto(
                lat.x + u(14.0),
                lat.y + u(52.0),
                "asking the islands…",
                14,
                estilo::SUAVE,
            );
            return None;
        }
        let linhas = crate::mundo_ui::linhas_de_chefe(mundo, agora_unix);
        if linhas.is_empty() {
            estilo::texto(
                lat.x + u(14.0),
                lat.y + u(52.0),
                "no island replied.",
                14,
                estilo::SUAVE,
            );
            return None;
        }
        let mut y = lat.y + u(52.0);
        for (nome, onde, vivo) in linhas {
            if y > lat.y + lat.h - u(24.0) {
                break;
            }
            // O ponto verde e' o que se le' antes do texto: ele responde
            // "da' pra ir agora?" sem ninguem precisar ler a linha.
            let cor = if vivo { estilo::VERDE } else { estilo::SUAVE };
            draw_circle(lat.x + u(20.0), y - u(4.0), u(4.0), cor);
            estilo::texto(lat.x + u(32.0), y, &nome, 14, estilo::TEXTO);
            estilo::texto(lat.x + u(32.0), y + u(16.0), &onde, 12, cor);
            y += u(40.0);
        }
        None
    }

    fn desenha_grande_mapa(&self, world: &World, agora_unix: i64) {
        let (sw, sh) = (screen_width(), screen_height());
        draw_rectangle(0.0, 0.0, sw, sh, Color::new(0.0, 0.0, 0.0, 0.45));
        let r = Self::grande_rect();
        estilo::painel(Rect::new(
            r.x - u(8.0),
            r.y - u(38.0),
            r.w + u(16.0),
            r.h + u(46.0),
        ));
        let nome = self.def.map_or(self.nome_sem_def.as_str(), |d| d.nome);
        // DEPOIS das abas, e nao em `r.x`: o titulo nascia debaixo de "Island" e
        // "World", desenhadas no mesmo canto. Com nome curto dava pra nao
        // reparar; "Planalto da Tormenta" botou o defeito na tela.
        let (_, mundo) = Self::abas_rect(r);
        estilo::texto_forte(
            mundo.x + mundo.w + u(12.0),
            r.y - u(14.0),
            nome,
            17,
            estilo::OURO,
        );
        // A DICA CEDE O LUGAR AO NOME. Ela e' alinhada a' direita e o titulo
        // cresce pra direita; com nome longo os dois se escreviam por cima. O
        // nome da ilha diz onde voce esta' e a dica repete um atalho — quando
        // so' um cabe, fica o nome.
        let dica = "clique: viajar · zona/recurso: ir · Esc fecha";
        let x_dica = r.x + r.w - u(36.0) - estilo::medir(dica, 13);
        if x_dica > mundo.x + mundo.w + u(12.0) + estilo::medir(nome, 17) + u(10.0) {
            estilo::texto(x_dica, r.y - u(14.0), dica, 13, estilo::SUAVE);
        }
        let f = Self::fechar_rect(r);
        if !crate::icones_ui::ui("fechar", f.center(), f.w.min(f.h) * 0.55, estilo::TEXTO) {
            estilo::texto_centro(f.x + f.w * 0.5, f.y + u(19.0), "x", 18, estilo::TEXTO);
        }

        // Os botoes de ZOOM vao por cima do desenho, no canto de baixo.
        let (menos, mais) = Self::zoom_grande_rects(r);
        draw_rectangle(r.x, r.y, r.w, r.h, COR_AGUA);
        match &self.tex {
            Some(tex) => {
                // Com zoom, desenha so' o PEDACO da imagem que esta' a' vista
                // — a mesma janela que `ponto` usa, senao o desenho e os
                // marcadores discordariam.
                let z = self.zoom_grande;
                let foco = self.foco_do_zoom(world.self_pos());
                let raio_ilha = self.raio();
                let lado = LADO as f32 / z;
                let meio = |v: f32| (v / raio_ilha * 0.5 + 0.5) * LADO as f32;
                draw_texture_ex(
                    tex,
                    r.x,
                    r.y,
                    WHITE,
                    DrawTextureParams {
                        dest_size: Some(vec2(r.w, r.h)),
                        source: (z > 1.001).then(|| {
                            Rect::new(
                                meio(foco.x) - lado * 0.5,
                                meio(foco.y) - lado * 0.5,
                                lado,
                                lado,
                            )
                        }),
                        ..Default::default()
                    },
                )
            }
            None => estilo::texto_centro(
                r.x + r.w * 0.5,
                r.y + r.h * 0.5,
                "loading map…",
                18,
                estilo::SUAVE,
            ),
        }
        // O ZOOM encolhe o raio VISTO e desloca o mundo pro foco. Uma conta
        // so' — `ponto` e `escala` saem dela, e tudo o que o mapa desenha
        // passa por `ponto`.
        let raio = self.raio() / self.zoom_grande;
        let foco = self.foco_do_zoom(world.self_pos());
        let escala = r.w / (2.0 * raio);
        let ponto = |p: Vec2| para_tela(p - foco, r, raio);
        // TUDO O QUE É MARCADOR FICA DENTRO DO DESENHO.
        //
        // Com zoom, `ponto` manda pra fora do retângulo o que está fora da
        // janela vista — e chefe, zona, recurso, NPC e rota eram desenhados
        // assim mesmo, por cima do painel lateral e do resto da tela. O dono:
        // "no mapa e no minimapa tá dando pra ver os ícones mesmo fora do
        // mapa / minimapa".
        //
        // O recorte fecha ANTES dos botões de zoom, que ficam por cima do
        // desenho de propósito e têm que continuar aparecendo.
        crate::rolagem::recortar(Some(r));
        if let Some(pl) = self.ger.as_ref().and_then(|g| g.planalto()) {
            for e in &pl.estradas {
                let a = ponto(vec2(e.a.x,e.a.y)); let b = ponto(vec2(e.b.x,e.b.y));
                draw_line(a.x,a.y,b.x,b.y,u(3.0),Color::new(0.78,0.68,0.46,0.85));
            }
            // EM ESCADA, e nao todos na linha do proprio ponto: as cinco
            // regioes ficam a ~100 u uma da outra, e no zoom de ilha inteira os
            // cinco nomes caiam uns por cima dos outros — e por cima do Abrigo e
            // do Porto. O degrau por indice da' linha propria a cada um mesmo
            // quando dois pontos coincidem, e a guia liga o nome ao ponto dele.
            for (i, regiao) in pl.regioes.iter().enumerate() {
                let q = ponto(vec2(regiao.centro.x,regiao.centro.y));
                let (a,b) = shared::planalto::NIVEIS[i];
                let nome = format!("{} · {}–{}",shared::planalto::NOMES[i],a,b);
                let dy = (i as f32 - 2.0) * u(12.0) - u(10.0);
                draw_circle(q.x,q.y,u(4.0),estilo::OURO);
                draw_line(q.x,q.y,q.x,q.y+dy+u(3.0),1.0,Color::new(1.0,0.78,0.35,0.45));
                estilo::texto_centro(q.x+1.0,q.y+dy+1.0,&nome,12,BLACK);
                estilo::texto_centro(q.x,q.y+dy,&nome,12,estilo::OURO);
            }
            if let Some(g) = &self.ger { if let Some(c) = g.cidade() {
                let q = ponto(vec2(c.centro().x,c.centro().y));
                estilo::texto_centro(q.x,q.y-u(22.0),"Último Abrigo",13,estilo::OURO);
            } }
            if let Some((_,c)) = pl.campo(agora_unix) {
                let q = ponto(vec2(c.x,c.y));
                draw_circle_lines(q.x,q.y,32.0*escala,u(2.0),SKYBLUE);
                estilo::texto_centro(q.x,q.y+u(22.0),"Tempestade · coleta +50%",12,SKYBLUE);
            }
        }
        if let Some(info) = &self.info {
            // Chefes sempre visiveis: sao o que se procura no mapa.
            for ch in &info.chefes {
                let q = ponto(vec2(ch.centro[0], ch.centro[1]));
                let ouro = Color::new(1.0, 0.72, 0.25, if ch.vivo { 1.0 } else { 0.5 });
                if !crate::icones_ui::mapa("chefe", q, u(26.0), ouro, 0.0) {
                    crate::telegrafico::desenha_coroa(q, u(8.0));
                }
                let t = format!("{} · Nv {}{}", ch.nome, ch.nivel, volta_em(ch));
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
                            format!("Stronghold · {} · Lv {}–{}", info.nome(b.0), z.lv_min, z.lv_max)
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
            estilo::texto_centro(q.x, q.y - u(12.0), "City", 14, estilo::OURO);
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
            estilo::texto_centro(q.x, q.y - u(14.0), "Port", 14, COR_PORTO);
        }
        // The Ermo's oasis: the one thick wood of the desert, worth marking.
        if let Some(o) = self.ger.as_ref().and_then(|g| g.oasis()) {
            let verde = Color::from_rgba(120, 214, 110, 255);
            let q = ponto(vec2(o.centro.x, o.centro.y));
            draw_circle_lines(
                q.x,
                q.y,
                (shared::oasis::RAIO_VERDE * escala).max(u(6.0)),
                u(2.0),
                verde,
            );
            estilo::texto_centro(q.x, q.y - u(14.0), "Oasis", 14, verde);
        }
        for (i, (nome, p)) in self.portas.iter().enumerate() {
            porta_no_mapa(ponto(*p), u(7.0), nome, 14, i % 2 == 0);
        }
        // NPCs da vila, com o filtro Vila (o `world` so' tem os de perto).
        // Tocar leva ate' ele; a lista do "Go to" tem todos, sempre.
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
        // Os nomes das ilhotas por ÚLTIMO entre os rótulos: texto por baixo de
        // mancha some, e este é o rótulo que o jogador veio buscar.
        nomes_das_ilhotas(self.zona(), ponto, escala, 14);
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
        crate::rolagem::recortar(None);
        // Os BOTOES DE ZOOM, por cima de tudo.
        for (b, rotulo, ativo) in [
            (menos, "−", self.zoom_grande > 1.001),
            (mais, "+", self.zoom_grande < 5.99),
        ] {
            estilo::botao(b, rotulo, estilo::estado_de(b, false, false), ativo);
        }
        if self.zoom_grande > 1.001 {
            estilo::texto(
                mais.x + mais.w + u(8.0),
                mais.y + u(22.0),
                &format!("{:.0}x", self.zoom_grande),
                13,
                estilo::SUAVE,
            );
        }
        // Onde o clique cairia: agua avisa antes de clicar.
        let m = Vec2::from(mouse_position());
        if r.contains(m) && self.tex.is_some() && !self.terra(de_tela(m, r, raio) + foco) {
            estilo::texto_centro(m.x, m.y - u(12.0), "water", 13, estilo::SUAVE);
        }
    }

    /// "Viajando · N m" enquanto houver viagem: o texto da faixa de estado.
    pub fn faixa_viagem(&self, eu: Option<Vec2>) -> Option<String> {
        let (Some(d), Some(eu)) = (self.viagem.destino(), eu) else {
            return None;
        };
        Some(format!("Travelling · {:.0} m", eu.distance(d)))
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
        // O mapa abre com a VILA, e so' ela (23/09/2026).
        //
        // Ja' abriu com tudo desligado (o jogador via a ilha e nada mais) e
        // depois com recurso, energia e vila ligados — e ai' o dono: "o
        // filtro padrao ta vindo com tudo clicado como ativo". A vila fica
        // porque e' REFERENCIA; recurso e energia saem porque sao BUSCA.
        let padrao = Filtros::default();
        assert!(!padrao.zona_visivel(&lobos), "mob comum nasce desligado");
        assert!(
            (0..=5).all(|t| !padrao.regiao_visivel(&regiao(0.0, t))),
            "recurso nasce DESLIGADO: quem procura madeira liga madeira"
        );
        assert!(padrao.vila, "cidade e porto sao referencia, nao enfeite");
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

/// O sufixo do rotulo de um chefe no mapa da ilha: nada se esta' vivo, e a
/// contagem se esta' voltando.
///
/// DURACAO, e nao hora de relogio. E' a convencao da base inteira — ate' o
/// reset diario aparece como "3h 12min" —, e ela existe por um motivo: o
/// cliente roda em celular, nao ha' crate de fuso no projeto, e uma hora
/// errada na tela e' pior que nenhuma. "Volta em 7m12s" responde o que o
/// jogador precisa decidir sem depender de fuso nenhum.
fn volta_em(ch: &shared::bosses::ChefeNoMapa) -> String {
    let agora = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    match shared::bosses::falta_pra_voltar(ch, agora) {
        None if ch.vivo => String::new(),
        None => " (renascendo)".into(),
        Some(s) => format!(" · back in {}", shared::bosses::conta_regressiva(s)),
    }
}

#[cfg(test)]
mod testes_das_vilas {
    use super::*;

    /// A ilha que tem povoado mostra o povoado na lista.
    ///
    /// Antes disto a praça e o cais só existiam como DESENHO no mapa: para
    /// chegar neles era preciso adivinhar onde tocar, enquanto bicho, recurso
    /// e NPC já tinham botão "Ir". O dono resumiu jogando — "vilas tem q
    /// mostrar no mapa igual npcs para ir sem precisar clicar, se não fica
    /// confuso".
    #[test]
    fn toda_ilha_com_povoado_lista_a_praca_e_o_porto() {
        use shared::terreno::{Gerador, ARQUIPELAGO};
        for d in ARQUIPELAGO.iter() {
            let ger = Gerador::da_ilha(d);
            let tem_cidade = ger.cidade().is_some();
            let tem_porto = ger.porto().is_some();
            // O mapa monta a lista dos mesmos dois campos; aqui se confere
            // que a ILHA os tem, que é o que alimenta a lista.
            assert!(
                tem_cidade || tem_porto,
                "{}: sem praça e sem porto — nada pra listar",
                d.nome
            );
            if let Some(c) = ger.cidade() {
                let p = c.centro();
                assert!(
                    p.x.is_finite() && p.y.is_finite(),
                    "{}: praça em posição inválida",
                    d.nome
                );
            }
        }
    }

    /// Sem ilha, a lista é vazia — e não um "Ir" que não leva a lugar nenhum.
    #[test]
    fn mapa_sem_ilha_nao_inventa_vila() {
        let m = Mapa::default();
        assert!(m.vilas_da_ilha().is_empty());
    }
}

#[cfg(test)]
mod testes_do_mapa_da_colonia {
    use super::*;

    /// O minimapa da COLÔNIA mostra a ilha — e não um retângulo de mar.
    ///
    /// O dono disse "o mapa tá errado, não tá mostrando nada". A imagem é
    /// gerada varrendo `ger.altura` num quadrado de `±raio`, e o raio da
    /// colônia vem de `raio_sem_def` (não há `DefIlha`): se ele não casar com
    /// o raio DE VERDADE da ilha, a varredura cai toda fora dela e o mapa sai
    /// todo azul — tecnicamente certo, e inútil.
    #[test]
    fn o_mapa_da_colonia_mostra_terra() {
        for nivel in [1u8, 5] {
            let plato = shared::colonia::plato_do_assentamento(nivel);
            let dados = gerar_dados_da_colonia(plato);
            assert_eq!(
                dados.rgba.len(),
                LADO * LADO * 4,
                "imagem do tamanho errado"
            );
            // Água é azulada (`cor_da_agua`): o canal B domina o R. Terra não.
            let terra = dados
                .rgba
                .chunks_exact(4)
                .filter(|p| p[0] as u16 + 12 > p[2] as u16)
                .count();
            let pct = 100.0 * terra as f32 / (LADO * LADO) as f32;
            println!("nivel {nivel}: {pct:.1}% da imagem é terra");
            assert!(
                pct > 15.0,
                "nível {nivel}: só {pct:.1}% da imagem é terra — o mapa está \
                 mostrando mar, e a ilha ficou fora do quadro"
            );
            // O CAIS SAIU, e com ele o barqueiro: desde 22/09/2026 a colônia
            // é uma ilhota desenhada que não se anda nem se navega, e um cais
            // seria um atracadouro para viagem nenhuma. O que importa agora é
            // a praça, no centro, e as casas.
            assert!(dados.porto.is_none(), "nível {nivel}: a ilhota ganhou cais");
            assert!(
                dados.mestre.is_some(),
                "nível {nivel}: o mapa não mostra a praça"
            );
            assert!(
                !dados.pegadas.is_empty(),
                "nível {nivel}: nenhuma casa no mapa"
            );
        }
    }

    /// A Ilha Mágica não tem cidade nem porto — e o minimapa monta mesmo
    /// assim.
    ///
    /// `gerar_dados` pede `ger.vila()`, e a vila sai da CIDADE. Numa zona sem
    /// nenhuma, o caminho nunca tinha sido exercitado: o jogador descobriria
    /// entrando, com meia hora de passe correndo.
    #[test]
    /// CADA ILHOTA TEM UM NOME, E SÃO NOMES DIFERENTES.
    ///
    /// O dono: "seria legal se no mapa da ilha mágica tivesse escrito qual é
    /// cada ilha". Dois rótulos iguais não resolveriam nada — o jogador
    /// abriria o mapa, veria "Stone" em dois lugares e continuaria sem saber
    /// pra onde ir.
    #[test]
    fn cada_ilhota_do_mapa_tem_um_nome_proprio() {
        // Escala generosa: o mapa grande com a ilha inteira à vista.
        let rotulos = ilhotas_rotuladas(Some(shared::magica::ZONA), 1.0);
        assert_eq!(
            rotulos.len(),
            shared::magica::ilhotas().len(),
            "nem toda ilhota recebeu nome com a ilha inteira à vista"
        );
        let nomes: std::collections::BTreeSet<&str> = rotulos.iter().map(|(_, n)| *n).collect();
        assert_eq!(
            nomes.len(),
            rotulos.len(),
            "duas ilhotas com o mesmo nome no mapa: {nomes:?}"
        );
        for (_, n) in &rotulos {
            assert!(!n.is_empty(), "ilhota sem nome");
            assert!(
                !n.contains("Ilhota"),
                "o nome do mapa repete a palavra Ilhota: {n}"
            );
        }
    }

    /// FORA DA ILHA MÁGICA, NENHUM NOME.
    ///
    /// As ilhotas são posições fixas de um mapa só; escrevê-las numa ilha
    /// comum poria sete rótulos aleatórios no meio do mar.
    #[test]
    fn so_a_ilha_magica_ganha_nome_de_ilhota() {
        assert!(ilhotas_rotuladas(Some("ilha_inicial"), 1.0).is_empty());
        assert!(ilhotas_rotuladas(Some("ilha_gelo"), 1.0).is_empty());
        assert!(ilhotas_rotuladas(None, 1.0).is_empty());
        // E todo degrau da ilha mágica ganha, não só o primeiro.
        for n in shared::magica::NIVEIS {
            assert!(
                !ilhotas_rotuladas(Some(n.zona), 1.0).is_empty(),
                "o degrau {} ficou sem nome de ilhota",
                n.grau
            );
        }
    }

    /// COM O MAPA AFASTADO, CALA A BOCA.
    ///
    /// Sete rótulos num disco de minimapa afastado viram uma mancha de texto
    /// em cima das próprias ilhotas — pior que rótulo nenhum.
    #[test]
    fn de_longe_os_nomes_das_ilhotas_somem() {
        assert!(
            ilhotas_rotuladas(Some(shared::magica::ZONA), 0.02).is_empty(),
            "os nomes continuaram com o mapa afastado demais"
        );
    }

    #[test]
    fn o_mapa_da_ilha_magica_monta_sem_cidade_nem_porto() {
        let def = shared::terreno::def_da_zona(shared::magica::ZONA).expect("a zona existe");
        let ger = Gerador::da_ilha(def);
        assert!(ger.cidade().is_none(), "a Ilha Mágica não tem praça");
        assert!(ger.porto().is_none(), "nem cais");
        assert!(ger.vila().predios.is_empty(), "nem casa");
        let dados = gerar_dados(def);
        assert_eq!(dados.rgba.len(), LADO * LADO * 4);
        assert!(dados.pegadas.is_empty());
        assert!(dados.porto.is_none());
        // E o desenho mostra TERRA: as sete ilhotas têm que aparecer.
        let terra = dados
            .rgba
            .chunks_exact(4)
            .filter(|p| p[0] as u16 + 12 > p[2] as u16)
            .count();
        let pct = 100.0 * terra as f32 / (LADO * LADO) as f32;
        println!("Ilha Mágica: {pct:.1}% da imagem é terra");
        assert!(pct > 3.0, "só {pct:.1}% de terra — o mapa saiu todo mar");
    }

    /// As casas estão na IMAGEM, e não só na camada de filtro.
    ///
    /// O dono: "o mapa só mostra visualmente, sem ser através de filtros, o
    /// chão; não mostra as casas". Estavam desenhadas por cima, atrás do
    /// filtro Vila — que vem guardado do servidor, então ligar o padrão não
    /// alcançava quem já tinha personagem. Este teste olha os PIXELS, que é
    /// onde o jogador olha.
    #[test]
    fn a_casa_aparece_no_desenho_do_mapa() {
        for def in shared::terreno::ARQUIPELAGO.iter() {
            let ger = Gerador::da_ilha(def);
            let raio = def.raio_blocos as f32 * BLOCO;
            let sem = gerar_terreno(&ger, def.raio_blocos, def.bioma);
            let mut com = sem.clone();
            pintar_casas(&mut com, &ger, raio);
            let mudou = sem
                .chunks_exact(4)
                .zip(com.chunks_exact(4))
                .filter(|(a, b)| a != b)
                .count();
            println!("{}: {mudou} pixels de casa", def.nome);
            assert!(
                mudou > 0,
                "{}: a vila tem {} prédios e nenhum pintou pixel — numa ilha \
                 de {:.0} m a casa é menor que o pixel, e some na conversão",
                def.nome,
                ger.vila().predios.len(),
                raio,
            );
        }
    }

    /// O raio que o mapa usa pra desenhar é o raio da ILHA.
    ///
    /// `raio()` sai de `raio_sem_def` na colônia. Se ele divergir de
    /// `colonia::RAIO_BLOCOS`, o ponto do jogador cai no lugar errado do mapa
    /// — e "terra" responde errado pro auto-caminho.
    #[test]
    fn o_raio_do_mapa_e_o_raio_da_ilha() {
        let m = Mapa::da_colonia(shared::colonia::plato_do_assentamento(1));
        let esperado = shared::colonia::RAIO_BLOCOS as f32 * BLOCO;
        assert!(m.tem_ilha(), "o mapa da colônia não se considera uma ilha");
        assert_eq!(m.raio(), esperado, "o raio do mapa não é o da ilha");
        // E o centro da ilha tem que ser terra pelo teste que o mapa usa.
        assert!(m.terra(Vec2::ZERO), "o centro da ilha deu água no mapa");
    }
}
