//! THE PATH TREE screen ("árvore de caminhos", `shared::arvore`): the
//! Path-of-Exile-style web drawn as a constellation on a night sky.
//!
//! Owner (15/11/2026): "a hud dos paths tá um lixo, tem que ser uma vibe
//! Path of Exile bem bonita com ícones". The whole panel is a pannable,
//! zoomable star map:
//!
//! - the four arms glow in their path's colour over faint nebulae, with the
//!   ring guides (1/2/3 points) as dashed circles around the start;
//! - minor nodes are small circles, notables bigger with a gold ring,
//!   keystones large with an ornate double ring and spikes, actives
//!   hexagons with the skill's icon; every stat has a vector glyph
//!   (`arvore_icones`);
//! - links glow when both ends are owned, owned nodes glow, the ones that
//!   can be learned now pulse;
//! - drag (finger or mouse) pans, pinch / wheel / the ± buttons zoom
//!   (clamped), "Center" goes back to the start; the camera eases;
//! - a tap (or click) selects a node and opens its card (name, effects,
//!   cost, path, "Learn (N pts)" or why not, the active's slots 4/5/6); a
//!   second tap on the same node learns it; hovering with a mouse shows a
//!   small tooltip;
//! - the header shows available / spent / total points and the gold reset;
//! - a learned node bursts (`receber` sees the new id) with a sound.
//!
//! The screen only shows what the server sent (`ServerMessage::Arvore`) and
//! sends requests (`PedidoArvore::Aprender`, `Equipar`, `Redistribuir`); the
//! server checks the adjacency, the budget and the gold.

#[allow(unused_imports)]
use crate::ui::Toque as _;
use macroquad::prelude::*;
use shared::arvore::{self as av, Caminho, EstadoArvore, No, PedidoArvore, TipoNo};
use shared::protocol::ClientMessage;
use std::f32::consts::{PI, TAU};

use crate::arvore_icones::{self as icones, Glifo};
use crate::hud_estilo as estilo;

/// Base size of the panel at 100% (it fills the safe area anyway).
const BASE_W: f32 = 1000.0;
const BASE_H: f32 = 620.0;
/// Radius of a node in tree units, by kind (also the overlap test's).
pub fn raio_u(t: TipoNo) -> f32 {
    match t {
        TipoNo::Inicio => 0.55,
        TipoNo::Menor => 0.26,
        TipoNo::Notavel => 0.36,
        TipoNo::Ativa => 0.40,
        TipoNo::Chave => 0.48,
    }
}
/// How far the web reaches from the start, in tree units (zoom to fit).
const ALCANCE_U: f32 = 13.9;
/// The ring guides (dashed circles): where ring 1 ends, ring 2 ends, the rim.
const GUIAS_U: [f32; 3] = [5.1, 8.97, 13.95];
const ZOOM_MAX: f32 = 2.4;

/// Deep night palette.
const CEU_TOPO: Color = Color::new(0.035, 0.032, 0.075, 1.0);
const CEU_BASE: Color = Color::new(0.012, 0.010, 0.030, 1.0);
const APAGADO: Color = Color::new(0.42, 0.40, 0.48, 1.0);
const FUNDO_NO: Color = Color::new(0.055, 0.048, 0.085, 1.0);

#[derive(Clone, Copy)]
struct Arrasto {
    inicio: Vec2,
    ultimo: Vec2,
    moveu: bool,
}

pub struct ArvoreUi {
    pub aberta: bool,
    /// The server's last state; `None` until it arrives.
    pub estado: Option<EstadoArvore>,
    /// The tapped node (its card is open).
    selecionado: Option<u16>,
    /// The node under the mouse (desktop tooltip).
    sobre: Option<u16>,
    confirmar_redistribuir: bool,
    /// Camera: the tree point at the canvas centre, and the zoom; the
    /// `_alvo` pair is where they ease to.
    cam: Vec2,
    cam_alvo: Vec2,
    zoom: f32,
    zoom_alvo: f32,
    /// The zoom is set on the first frame (it depends on the screen).
    zoom_pronto: bool,
    arrasto: Option<Arrasto>,
    pinca: Option<f32>,
    /// A pinch happened: no tap until every finger is up.
    pincou: bool,
    /// Bursts of just-learned nodes: (id, start time).
    explosoes: Vec<(u16, f64)>,
    /// Preview only: a fixed clock, so captures are stable.
    relogio: Option<f64>,
}

impl Default for ArvoreUi {
    fn default() -> Self {
        ArvoreUi {
            aberta: false,
            estado: None,
            selecionado: None,
            sobre: None,
            confirmar_redistribuir: false,
            cam: Vec2::ZERO,
            cam_alvo: Vec2::ZERO,
            zoom: 1.0,
            zoom_alvo: 1.0,
            zoom_pronto: false,
            arrasto: None,
            pinca: None,
            pincou: false,
            explosoes: Vec::new(),
            relogio: None,
        }
    }
}

/// The screen split: the panel, its header strip and the star canvas, the
/// docked card's place, and the corner controls.
#[derive(Clone, Copy, Debug)]
struct Layout {
    painel: Rect,
    cabecalho: Rect,
    canvas: Rect,
    /// Two header rows (narrow / portrait).
    duas_linhas: bool,
    /// Where the selected node's card docks.
    doca: Rect,
    /// Center, zoom in, zoom out.
    centro: Rect,
    mais: Rect,
    menos: Rect,
}

fn layout(seguro: Rect, f: f32) -> Layout {
    let m = 6.0;
    let painel = Rect::new(seguro.x + m, seguro.y + m, seguro.w - 2.0 * m, seguro.h - 2.0 * m);
    let duas_linhas = painel.w < 760.0 * f;
    let ch = if duas_linhas { 104.0 * f } else { 60.0 * f };
    let cabecalho = Rect::new(painel.x, painel.y, painel.w, ch);
    let canvas = Rect::new(painel.x, painel.y + ch, painel.w, painel.h - ch);
    let retrato = canvas.h > canvas.w;
    let pad = 12.0 * f;
    let doca = if retrato {
        let h = (canvas.h * 0.46).min(330.0 * f);
        Rect::new(canvas.x + pad, canvas.y + canvas.h - h - pad, canvas.w - 2.0 * pad, h)
    } else {
        let w = (canvas.w * 0.42).min(370.0 * f);
        Rect::new(canvas.x + canvas.w - w - pad, canvas.y + pad, w, canvas.h - 2.0 * pad)
    };
    let b = 46.0 * f;
    let x = canvas.x + pad;
    let y = canvas.y + pad;
    Layout {
        painel,
        cabecalho,
        canvas,
        duas_linhas,
        doca,
        centro: Rect::new(x, y, b * 2.2, b),
        mais: Rect::new(x, y + b + 8.0 * f, b, b),
        menos: Rect::new(x, y + 2.0 * (b + 8.0 * f), b, b),
    }
}

/// Pixels per tree unit at zoom 1. Phones: a minor node's DIAMETER is at
/// least 36 px and close to a finger (`ui::alvo_minimo`); desktop: 44 × the
/// interface scale.
fn px_base(f: f32, celular: bool) -> f32 {
    if celular {
        let raio_px = (crate::ui::alvo_minimo() * 0.36).max(18.0).max(18.0 * f.min(1.6));
        raio_px / raio_u(TipoNo::Menor)
    } else {
        44.0 * f
    }
}

/// The zoom that fits the whole web in `canvas`.
fn zoom_que_cabe(canvas: Rect, base: f32) -> f32 {
    canvas.w.min(canvas.h) / (2.0 * ALCANCE_U * base)
}

fn zoom_min(canvas: Rect, base: f32) -> f32 {
    zoom_que_cabe(canvas, base).min(1.0) * 0.95
}

/// A path's colour.
pub fn cor_do_caminho(c: Caminho) -> Color {
    crate::ativas::cor_do_caminho(c)
}

/// A node's colour: its path's, a bridge's two blended, the start's gold.
fn cor_do_no(n: &No) -> Color {
    match (n.caminho, n.ponte) {
        (Some(c), _) => cor_do_caminho(c),
        (None, Some((a, b))) => estilo::misturar(cor_do_caminho(a), cor_do_caminho(b), 0.5),
        _ => estilo::OURO,
    }
}

/// The node's glyph.
fn glifo_do_no(n: &No) -> Glifo {
    match n.tipo {
        TipoNo::Inicio => Glifo::Bussola,
        TipoNo::Chave => icones::da_chave(n.efeitos()),
        _ => n.efeitos().first().map_or(Glifo::Estrela, |&(e, _)| icones::do_efeito(e)),
    }
}

/// A tiny repeatable hash in 0..1.
fn sorte(i: u32) -> f32 {
    let mut x = i.wrapping_mul(0x9E37_79B9) ^ 0x85EB_CA6B;
    x ^= x >> 15;
    x = x.wrapping_mul(0x2C1B_3C6D);
    x ^= x >> 12;
    (x & 0xFFFF) as f32 / 65_535.0
}

fn ease(atual: f32, alvo: f32, dt: f32, k: f32) -> f32 {
    atual + (alvo - atual) * (1.0 - (-k * dt).exp())
}

/// "Assassin · Notable": translated pieces joined (a template with no
/// letters would match anything, so the join is not translated).
fn subtitulo(n: &No) -> String {
    let tr = |s: &str| shared::idioma::tr(s).into_owned();
    let tipo = match n.tipo {
        TipoNo::Inicio => "Origin",
        TipoNo::Menor if n.ponte.is_some() => "Bridge",
        TipoNo::Menor => "Minor node",
        TipoNo::Notavel if n.ponte.is_some() => "Notable bridge",
        TipoNo::Notavel => "Notable",
        TipoNo::Chave => "Keystone",
        TipoNo::Ativa => "Active skill",
    };
    let onde = match (n.caminho, n.ponte) {
        (Some(c), _) => tr(c.nome()),
        (None, Some((a, b))) => format!("{} / {}", tr(a.nome()), tr(b.nome())),
        _ => tr("Path tree"),
    };
    format!("{onde} · {}", tr(tipo))
}

/// A node's title: its name, or a minor's first line.
fn titulo(n: &No) -> String {
    if !n.nome.is_empty() {
        return shared::idioma::tr(n.nome).into_owned();
    }
    n.efeitos().first().map_or_else(String::new, |&(e, v)| linha_traduzida(e, v))
}

/// "Crit chance +1%" in the player's language: the template translated,
/// then filled.
fn linha_traduzida(e: av::Efeito, v: f32) -> String {
    shared::idioma::tr(e.modelo_de(v)).replace("{}", &e.numero(v))
}

impl ArvoreUi {
    /// Opens on the start, at the default zoom (set on the first frame).
    pub fn abrir(&mut self) {
        self.aberta = true;
        self.confirmar_redistribuir = false;
        self.selecionado = None;
        self.sobre = None;
        self.cam = Vec2::ZERO;
        self.cam_alvo = Vec2::ZERO;
        self.zoom_pronto = false;
        self.arrasto = None;
        self.pinca = None;
    }

    pub fn fechar(&mut self) {
        self.aberta = false;
        self.confirmar_redistribuir = false;
        self.arrasto = None;
    }

    pub fn receber(&mut self, mut estado: EstadoArvore) {
        // The slots travel beside the allocation (`Arvore::slots` is not on
        // the wire): keep both views the same.
        estado.arvore.slots = estado.slots;
        if let Some(antes) = self.estado {
            let novos: Vec<u16> =
                estado.arvore.ids().into_iter().filter(|&id| !antes.arvore.tem(id)).collect();
            let agora = self.agora();
            let mut grande = false;
            for id in &novos {
                self.explosoes.push((*id, agora));
                grande |= av::no(*id).is_some_and(|n| n.tipo != TipoNo::Menor);
            }
            if !novos.is_empty() && self.relogio.is_none() {
                crate::sons::tocar(if grande { crate::sons::Som::Recompensa } else { crate::sons::Som::Progresso });
            }
        }
        self.estado = Some(estado);
    }

    /// Free points with something to spend them on: the red dot on the
    /// Sheet (and the Menu).
    pub fn tem_ponto_livre(&self) -> bool {
        self.estado.is_some_and(|e| {
            let livres = e.livres();
            livres > 0 && av::nos().iter().any(|n| e.arvore.alcancavel(n.id) && n.custo as u32 <= livres)
        })
    }

    /// Can node `id` be learned right now? The same rule as the server's.
    fn pode_aprender(&self, id: u16) -> bool {
        self.estado.is_some_and(|e| e.arvore.pode_aprender(id, e.ganhos).is_ok())
    }

    fn agora(&self) -> f64 {
        self.relogio.unwrap_or_else(get_time)
    }

    pub fn desenha(&mut self, ouro: u64) -> Option<ClientMessage> {
        if !self.aberta {
            return None;
        }
        // Upright phones measure against a tall, narrow base, so the text and
        // the card grow to the width the screen really has.
        let seguro = crate::hud_layout::tela_segura();
        let f = if seguro.h > seguro.w {
            estilo::escala_do_painel(BASE_H * 0.9, BASE_W)
        } else {
            estilo::escala_do_painel(BASE_W, BASE_H)
        };
        estilo::no_painel(f, || {
            let modal = self.confirmar_redistribuir;
            let pedido = self.desenha_painel(ouro, !modal);
            if modal {
                return self.desenha_confirmacao(ouro);
            }
            pedido
        })
    }

    fn desenha_painel(&mut self, ouro: u64, ativo: bool) -> Option<ClientMessage> {
        let f = estilo::fator_texto();
        let celular = crate::nativo::modo_celular();
        let l = layout(crate::hud_layout::tela_segura(), f);
        let base = px_base(f, celular);
        let zmin = zoom_min(l.canvas, base);
        if !self.zoom_pronto {
            self.zoom_pronto = true;
            let z = if celular { 1.0 } else { zoom_que_cabe(l.canvas, base).max(0.78).min(1.0) };
            self.zoom = z.clamp(zmin, ZOOM_MAX);
            self.zoom_alvo = self.zoom;
        }
        let agora = self.agora();
        let dt = get_frame_time().min(0.1);
        let m = Vec2::from(mouse_position());
        let mut pedido = None;

        // ── input on the canvas: pan, pinch, wheel, tap ──
        // (The preview's fixed clock also freezes the input.)
        if ativo && self.relogio.is_none() {
            pedido = self.entrada(&l, base, zmin, celular, m);
        }
        // Ease the camera.
        if self.arrasto.map_or(true, |a| !a.moveu) {
            self.cam.x = ease(self.cam.x, self.cam_alvo.x, dt, 10.0);
            self.cam.y = ease(self.cam.y, self.cam_alvo.y, dt, 10.0);
        }
        self.zoom = ease(self.zoom, self.zoom_alvo, dt, 12.0);
        let escala = base * self.zoom;
        let centro = l.canvas.center();
        let tela = |x: f32, y: f32, cam: Vec2| centro + (vec2(x, y) - cam) * escala;

        // ── the sky ──
        crate::hud_layout::escurece(0.6);
        estilo::sombra(l.painel, 10.0, 1.4);
        crate::rolagem::recortar(Some(l.painel));
        ceu(l.canvas.combine_with(l.cabecalho), centro, self.cam, escala, base, agora);
        crate::rolagem::recortar(Some(l.canvas));
        let cam = self.cam;
        // Ring guides: dashed circles around the start.
        let c0 = tela(0.0, 0.0, cam);
        for (i, r) in GUIAS_U.iter().enumerate() {
            circulo_tracejado(c0, r * escala, (1.0 + escala * 0.02).min(2.0), estilo::alfa(estilo::OURO, 0.10 - i as f32 * 0.02));
        }
        let Some(estado) = self.estado else {
            crate::rolagem::recortar(None);
            self.desenha_cabecalho(&l, None, ouro, ativo, m);
            estilo::texto_centro(centro.x, centro.y, "Waiting for the server…", 16, estilo::SUAVE);
            return None;
        };
        let arv = estado.arvore;
        let livres = estado.livres();

        // ── links ──
        let largura = (0.075 * escala).clamp(1.5, 7.0);
        for &(a, b) in av::ligacoes() {
            let (Some(na), Some(nb)) = (av::no(a), av::no(b)) else { continue };
            let (pa, pb) = (tela(na.x, na.y, cam), tela(nb.x, nb.y, cam));
            if !l.canvas.overlaps(&Rect::new(pa.x.min(pb.x) - 4.0, pa.y.min(pb.y) - 4.0, (pa.x - pb.x).abs() + 8.0, (pa.y - pb.y).abs() + 8.0)) {
                continue;
            }
            let cor = if na.tipo == TipoNo::Inicio { cor_do_no(nb) } else if nb.tipo == TipoNo::Inicio { cor_do_no(na) } else {
                estilo::misturar(cor_do_no(na), cor_do_no(nb), 0.5)
            };
            let (ta, tb) = (arv.tem(a), arv.tem(b));
            if ta && tb {
                estilo::traco(pa, pb, largura * 3.2, estilo::alfa(cor, 0.10));
                estilo::traco(pa, pb, largura * 1.8, estilo::alfa(cor, 0.28));
                estilo::traco(pa, pb, largura, estilo::clarear(cor, 0.25));
                // A spark travelling along the lit link.
                let t = ((agora * 0.35 + sorte(a as u32 * 31 + b as u32) as f64) % 1.0) as f32;
                let q = pa.lerp(pb, t);
                draw_circle(q.x, q.y, largura * 0.9, estilo::alfa(WHITE, 0.55));
            } else if ta || tb {
                let pulso = 0.5 + 0.5 * (agora as f32 * 2.6).sin();
                estilo::traco(pa, pb, largura * 0.8, estilo::alfa(cor, 0.30 + 0.25 * pulso));
            } else {
                estilo::traco(pa, pb, largura * 0.6, estilo::alfa(APAGADO, 0.22));
            }
        }

        // ── nodes ──
        let raio_de = |n: &No| raio_u(n.tipo) * escala;
        for n in av::nos() {
            let p = tela(n.x, n.y, cam);
            let r = raio_de(n);
            if !l.canvas.overlaps(&Rect::new(p.x - r * 3.0, p.y - r * 3.0, r * 6.0, r * 6.0)) {
                continue;
            }
            let tem = arv.tem(n.id);
            let pode = !tem && arv.alcancavel(n.id);
            let barato = pode && n.custo as u32 <= livres;
            let realce = if self.sobre == Some(n.id) { 1.08 } else { 1.0 };
            let suprema = n.ativa.is_some_and(|a| av::e_suprema(a, estado.renascimentos));
            let slot = n.ativa.and_then(|a| estado.slots.iter().position(|s| *s == Some(a)));
            desenha_no(n, p, r * realce, tem, pode, barato, suprema, slot, agora);
            if self.selecionado == Some(n.id) {
                anel_de_selecao(p, r * realce + 7.0 * f.min(1.6), agora);
            }
        }
        // ── bursts ──
        self.explosoes.retain(|(_, t0)| agora - t0 < 1.1);
        for &(id, t0) in &self.explosoes {
            if let Some(n) = av::no(id) {
                explosao(tela(n.x, n.y, cam), raio_de(n), cor_do_no(n), (agora - t0) as f32, id);
            }
        }
        crate::rolagem::recortar(None);
        // The frame of the sky.
        estilo::borda_arredondada(l.painel, 10.0, 2.0, estilo::alfa(estilo::BORDA_FORTE, 0.75));
        estilo::borda_arredondada(
            Rect::new(l.painel.x + 4.0, l.painel.y + 4.0, l.painel.w - 8.0, l.painel.h - 8.0),
            8.0,
            1.0,
            estilo::alfa(estilo::OURO, 0.18),
        );

        // ── corner controls ──
        let clique = crate::foco::clique() && ativo;
        controle(l.centro, "Center", clique, m);
        if clique && l.centro.toca(m) {
            self.cam_alvo = Vec2::ZERO;
            self.zoom_alvo = if celular { 1.0 } else { zoom_que_cabe(l.canvas, base).clamp(zmin, 1.0) };
        }
        controle(l.mais, "+", clique, m);
        controle(l.menos, "−", clique, m);
        if clique && l.mais.toca(m) {
            self.zoom_alvo = (self.zoom_alvo * 1.3).clamp(zmin, ZOOM_MAX);
        }
        if clique && l.menos.toca(m) {
            self.zoom_alvo = (self.zoom_alvo / 1.3).clamp(zmin, ZOOM_MAX);
        }
        let dica = if celular {
            "Drag to move · pinch to zoom · tap a node twice to learn it"
        } else {
            "Drag to move · scroll to zoom · click a node twice to learn it"
        };
        let livre_w = if self.selecionado.is_some() && l.doca.x > l.canvas.x + l.canvas.w * 0.4 { l.doca.x - l.canvas.x - 24.0 * f } else { l.canvas.w - 24.0 * f };
        if self.selecionado.is_none() || l.doca.x > l.canvas.x + l.canvas.w * 0.4 {
            let tw = (estilo::medir(dica, 12) + 20.0 * f).min(livre_w + 8.0 * f);
            let fundo = Rect::new(l.canvas.x + 8.0 * f, l.canvas.y + l.canvas.h - 32.0 * f, tw, 24.0 * f);
            estilo::ret_arredondado(fundo, 12.0 * f, Color::new(0.0, 0.0, 0.0, 0.55));
            estilo::texto_ajustado(dica, l.canvas.x + 18.0 * f, l.canvas.y + l.canvas.h - 15.0 * f, livre_w - 10.0 * f, 12, estilo::alfa(estilo::SUAVE, 0.9));
        }

        // ── header ──
        if let Some(p) = self.desenha_cabecalho(&l, Some(&estado), ouro, ativo, m) {
            pedido = Some(p);
        }

        // ── the card of the selected node, the tooltip of the hovered ──
        if let Some(id) = self.selecionado {
            if let Some(p) = self.desenha_cartao(&l, &estado, id, ativo, m, agora) {
                pedido = Some(p);
            }
        } else if let Some(id) = self.sobre {
            if let Some(n) = av::no(id) {
                dica_flutuante(n, &estado, tela(n.x, n.y, cam), raio_de(n), l.canvas);
            }
        }
        pedido
    }

    /// Pan, pinch, wheel and taps on the canvas. A tap on a node selects
    /// it; a second tap on the selected node learns it.
    fn entrada(&mut self, l: &Layout, base: f32, zmin: f32, celular: bool, m: Vec2) -> Option<ClientMessage> {
        let f = estilo::fator_texto();
        let escala = base * self.zoom;
        let centro = l.canvas.center();
        // What is not canvas: the header, the open card, the corner buttons.
        let com_cartao = self.selecionado.is_some();
        let fora = |p: Vec2| {
            !l.canvas.contains(p)
                || (com_cartao && l.doca.contains(p))
                || l.centro.contains(p)
                || l.mais.contains(p)
                || l.menos.contains(p)
        };
        let dedos: Vec<Vec2> =
            touches().iter().filter(|t| t.phase != TouchPhase::Ended && t.phase != TouchPhase::Cancelled).map(|t| t.position).collect();
        if dedos.len() >= 2 {
            self.arrasto = None;
            self.pincou = true;
            let d = dedos[0].distance(dedos[1]);
            let meio = (dedos[0] + dedos[1]) * 0.5;
            if let Some(antes) = self.pinca.replace(d) {
                if antes > 1.0 {
                    let z = (self.zoom * d / antes).clamp(zmin, ZOOM_MAX);
                    self.zoom_em(meio, centro, base, z);
                }
            }
            return None;
        }
        self.pinca = None;
        let segura = is_mouse_button_down(MouseButton::Left) || !dedos.is_empty();
        if !segura {
            let soltou = self.arrasto.take();
            let pincou = std::mem::take(&mut self.pincou);
            if let Some(a) = soltou {
                if !a.moveu && !pincou {
                    return self.toque(a.inicio, l, base);
                }
            }
        }
        if let Some(p) = apertou_em() {
            if !fora(p) && crate::foco::passa_ponto(p) && !self.pincou {
                self.arrasto = Some(Arrasto { inicio: p, ultimo: p, moveu: false });
            }
        }
        if let Some(mut a) = self.arrasto {
            if segura {
                let q = dedos.first().copied().unwrap_or(m);
                if q.distance(a.inicio) > 9.0 * f {
                    a.moveu = true;
                }
                if a.moveu {
                    self.cam_alvo -= (q - a.ultimo) / escala;
                    self.cam = self.cam_alvo;
                }
                a.ultimo = q;
                self.arrasto = Some(a);
            }
        }
        // Wheel: zoom around the pointer.
        let roda = mouse_wheel().1;
        if roda != 0.0 && !fora(m) {
            let z = (self.zoom_alvo * if roda > 0.0 { 1.15 } else { 1.0 / 1.15 }).clamp(zmin, ZOOM_MAX);
            self.zoom_em(m, centro, base, z);
        }
        // Hover (mouse only).
        self.sobre = if celular || fora(m) || self.arrasto.is_some_and(|a| a.moveu) {
            None
        } else {
            self.no_em(m, l, base)
        };
        // Keep the web on screen: the camera never leaves the rim.
        let lim = ALCANCE_U;
        self.cam_alvo = self.cam_alvo.clamp(Vec2::splat(-lim), Vec2::splat(lim));
        None
    }

    /// Zoom to `z` keeping the tree point under `p` where it is.
    fn zoom_em(&mut self, p: Vec2, centro: Vec2, base: f32, z: f32) {
        let antes = self.cam + (p - centro) / (base * self.zoom);
        self.zoom = z;
        self.zoom_alvo = z;
        self.cam = antes - (p - centro) / (base * z);
        self.cam_alvo = self.cam;
    }

    /// The node under screen point `p` (the nearest within a finger).
    fn no_em(&self, p: Vec2, l: &Layout, base: f32) -> Option<u16> {
        let escala = base * self.zoom;
        let centro = l.canvas.center();
        let alvo = crate::ui::alvo_minimo() * 0.5;
        av::nos()
            .iter()
            .map(|n| {
                let q = centro + (vec2(n.x, n.y) - self.cam) * escala;
                (n.id, q.distance(p), (raio_u(n.tipo) * escala * 1.15).max(alvo))
            })
            .filter(|(_, d, r)| d <= r)
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .map(|(id, _, _)| id)
    }

    fn toque(&mut self, p: Vec2, l: &Layout, base: f32) -> Option<ClientMessage> {
        let Some(id) = self.no_em(p, l, base) else {
            self.selecionado = None;
            return None;
        };
        if self.selecionado == Some(id) {
            if self.pode_aprender(id) {
                return Some(ClientMessage::Arvore { pedido: PedidoArvore::Aprender { no: id } });
            }
            return None;
        }
        self.selecionado = Some(id);
        // Bring the node out from under the card.
        let escala = base * self.zoom;
        if let Some(n) = av::no(id) {
            let q = l.canvas.center() + (vec2(n.x, n.y) - self.cam) * escala;
            let folga = Rect::new(l.doca.x - 30.0, l.doca.y - 30.0, l.doca.w + 60.0, l.doca.h + 60.0);
            if folga.contains(q) {
                let livre = if l.doca.y > l.canvas.y + l.canvas.h * 0.3 {
                    vec2(l.canvas.center().x, (l.canvas.y + l.doca.y) * 0.5)
                } else {
                    vec2((l.canvas.x + l.doca.x) * 0.5, l.canvas.center().y)
                };
                self.cam_alvo = vec2(n.x, n.y) - (livre - l.canvas.center()) / escala;
            }
        }
        None
    }

    /// Title, the three point counters, the gold reset, Close.
    fn desenha_cabecalho(&mut self, l: &Layout, estado: Option<&EstadoArvore>, ouro: u64, ativo: bool, m: Vec2) -> Option<ClientMessage> {
        let f = estilo::fator_texto();
        let h = l.cabecalho;
        estilo::ret_gradiente(h, 10.0, Color::new(0.10, 0.075, 0.12, 0.96), Color::new(0.05, 0.04, 0.07, 0.94));
        draw_line(h.x + 8.0, h.y + h.h, h.x + h.w - 8.0, h.y + h.h, 1.5, estilo::alfa(estilo::OURO, 0.55));
        let linha1 = Rect::new(h.x, h.y, h.w, 60.0 * f);
        let clique = crate::foco::clique() && ativo;
        // Title with the compass.
        let ic = vec2(h.x + 30.0 * f, linha1.y + linha1.h * 0.5);
        crate::loja_tp::brilho_radial(ic, 24.0 * f, estilo::OURO, 0.35);
        icones::desenha(Glifo::Bussola, ic, 16.0 * f, estilo::OURO);
        estilo::texto_forte(h.x + 54.0 * f, linha1.y + 39.0 * f, "PATHS", 24, estilo::OURO);
        let fechar = Rect::new(h.x + h.w - 50.0 * f, linha1.y + 9.0 * f, 42.0 * f, 42.0 * f);
        estilo::botao(fechar, "X", estilo::estado_de(fechar, false, false), false);
        if clique && fechar.toca(m) {
            self.fechar();
            return None;
        }
        let estado = estado?;
        let gastos = estado.arvore.gastos();
        // The counters: on the title row, or under it when narrow.
        let (mut x, y) = if l.duas_linhas {
            (h.x + 14.0 * f, h.y + 60.0 * f)
        } else {
            (h.x + 54.0 * f + estilo::medir_forte("PATHS", 24) + 44.0 * f, h.y + 10.0 * f)
        };
        let livres = estado.livres();
        for (rotulo, valor, cor) in [
            ("Available", livres, if livres > 0 { estilo::VERDE } else { estilo::SUAVE }),
            ("Spent", gastos, estilo::OURO),
            ("Total", estado.ganhos, estilo::TEXTO),
        ] {
            x += chip(vec2(x, y), rotulo, valor, cor, f) + 8.0 * f;
        }
        // The reset, at the right.
        let rotulo = format!("Reset paths · {} gold", crate::bolsa::milhar(estado.custo_redistribuir));
        let rw = (estilo::medir_forte(&shared::idioma::tr(&rotulo), 15) + 30.0 * f).max(150.0 * f);
        let reset = if l.duas_linhas {
            Rect::new(h.x + h.w - rw - 10.0 * f, h.y + 60.0 * f, rw, 38.0 * f)
        } else {
            Rect::new(fechar.x - rw - 10.0 * f, linha1.y + 10.0 * f, rw, 40.0 * f)
        };
        // Narrow and the counters reach the button: drop the "Total" chip's row
        // overlap by moving the button (it never covers the counters).
        let pode_reset = gastos > 0;
        estilo::botao(reset, &rotulo, estilo::estado_de(reset, !pode_reset, false), false);
        let _ = ouro;
        if clique && reset.toca(m) && pode_reset {
            self.confirmar_redistribuir = true;
        }
        None
    }

    /// The selected node's card: what it gives, what it costs, Learn.
    fn desenha_cartao(&mut self, l: &Layout, estado: &EstadoArvore, id: u16, ativo: bool, m: Vec2, agora: f64) -> Option<ClientMessage> {
        let n = av::no(id)?;
        let f = estilo::fator_texto();
        let arv = estado.arvore;
        let cor = cor_do_no(n);
        let d = l.doca;
        let pad = 16.0 * f;
        let larg = d.w - 2.0 * pad;
        // ── measure the content first, to size the card ──
        let desc: Vec<String> = n
            .ativa
            .and_then(av::def)
            .map_or_else(Vec::new, |dd| crate::missoes::quebra(dd.descricao, larg, 13, 5));
        let efeitos = n.efeitos().len();
        let tem_slots = n.ativa.is_some() && arv.tem(id);
        let mut h = pad + 54.0 * f; // title block
        h += 10.0 * f + efeitos as f32 * 24.0 * f;
        if !desc.is_empty() {
            h += 22.0 * f + desc.len() as f32 * 18.0 * f;
        }
        h += 50.0 * f; // cost, status
        h += if tem_slots { 30.0 * f + 50.0 * f } else { 56.0 * f };
        h += pad;
        let retrato = d.y > l.canvas.y + l.canvas.h * 0.3;
        let r = if retrato {
            let hh = h.min(l.canvas.h - 24.0 * f);
            Rect::new(d.x, d.y + d.h - hh, d.w, hh)
        } else {
            Rect::new(d.x, d.y, d.w, h.min(d.h))
        };
        // ── the card ──
        estilo::sombra(r, 10.0, 1.3);
        estilo::ret_gradiente(r, 10.0, Color::new(0.09, 0.07, 0.12, 0.97), Color::new(0.04, 0.03, 0.06, 0.97));
        crate::loja_tp::brilho_radial(vec2(r.x + 40.0 * f, r.y + 40.0 * f), 90.0 * f, cor, 0.16);
        let borda = if matches!(n.tipo, TipoNo::Chave | TipoNo::Notavel) { estilo::OURO } else { cor };
        estilo::borda_arredondada(r, 10.0, 2.0, estilo::alfa(borda, 0.85));
        estilo::borda_arredondada(Rect::new(r.x + 4.0, r.y + 4.0, r.w - 8.0, r.h - 8.0), 8.0, 1.0, estilo::alfa(borda, 0.22));
        let x = r.x + pad;
        let mut y = r.y + pad;
        // Medallion + title.
        let med = vec2(x + 24.0 * f, y + 24.0 * f);
        let tem = arv.tem(id);
        desenha_no(n, med, 21.0 * f, tem, !tem, true, false, None, agora);
        let tx = x + 58.0 * f;
        let tw = r.x + r.w - pad - tx - 34.0 * f;
        let cor_titulo = match n.tipo {
            TipoNo::Chave | TipoNo::Notavel => estilo::ACENTO,
            TipoNo::Ativa => estilo::clarear(cor, 0.15),
            _ => estilo::TEXTO,
        };
        estilo::texto_ajustado(&titulo(n), tx, y + 20.0 * f, tw, 18, cor_titulo);
        estilo::texto_ajustado(&subtitulo(n), tx, y + 42.0 * f, tw, 13, estilo::alfa(cor, 0.95));
        // Close (deselect).
        let x_r = Rect::new(r.x + r.w - pad - 30.0 * f, r.y + 10.0 * f, 30.0 * f, 30.0 * f);
        estilo::texto_centro_forte(x_r.center().x, x_r.center().y + 6.0 * f, "×", 20, estilo::SUAVE);
        let clique = crate::foco::clique() && ativo;
        if clique && x_r.toca(m) {
            self.selecionado = None;
            return None;
        }
        y += 54.0 * f;
        draw_line(x, y, r.x + r.w - pad, y, 1.0, estilo::alfa(estilo::OURO, 0.25));
        y += 10.0 * f;
        // Effects, each with its glyph; costs in red.
        for &(e, v) in n.efeitos() {
            let bom = e.bom(v);
            let c = if bom { Color::new(0.62, 0.92, 0.66, 1.0) } else { estilo::VERMELHO };
            icones::desenha(icones::do_efeito(e), vec2(x + 9.0 * f, y + 11.0 * f), 9.0 * f, c);
            estilo::texto_ajustado(&linha_traduzida(e, v), x + 26.0 * f, y + 17.0 * f, larg - 26.0 * f, 15, c);
            y += 24.0 * f;
        }
        if let Some(dd) = n.ativa.and_then(av::def) {
            let a = n.ativa.unwrap_or(0);
            let sup = if av::e_suprema(a, estado.renascimentos) {
                " · ".to_string() + &shared::idioma::tr("Supreme")
            } else {
                String::new()
            };
            let info = format!(
                "{}{sup}",
                shared::idioma::tr(&format!("{} MP · {}s cooldown", dd.custo_mp, dd.espera_s as u32))
            );
            estilo::texto(x, y + 16.0 * f, &info, 13, estilo::OURO);
            y += 22.0 * f;
            for linha in &desc {
                estilo::texto(x, y + 14.0 * f, linha, 13, estilo::SUAVE);
                y += 18.0 * f;
            }
        }
        // Cost and status.
        y += 6.0 * f;
        let custo = format!("Cost: {} points", n.custo);
        let livres = estado.livres();
        let (situacao, cor_sit) = if tem {
            ("Learned".to_string(), estilo::VERDE)
        } else if !arv.alcancavel(id) {
            let rota = arv.rota_ate(id);
            let pts: u32 = rota.iter().filter_map(|&i| av::no(i)).map(|n| n.custo as u32).sum();
            (
                shared::idioma::tr(&format!("Not connected yet: {} nodes, {} points away", rota.len(), pts)).into_owned(),
                estilo::SUAVE,
            )
        } else if (n.custo as u32) > livres {
            (shared::idioma::tr(&format!("Needs {} free points", n.custo)).into_owned(), estilo::VERMELHO)
        } else {
            ("Ready to learn".to_string(), estilo::ACENTO)
        };
        if n.tipo != TipoNo::Inicio {
            estilo::texto(x, y + 14.0 * f, &custo, 13, estilo::TEXTO);
            y += 20.0 * f;
        }
        estilo::texto_ajustado(&situacao, x, y + 14.0 * f, larg, 13, cor_sit);
        y += 28.0 * f;
        // Buttons: the slots of an owned active, else Learn.
        let mut pedido = None;
        if let (Some(a), true) = (n.ativa, tem_slots) {
            let onde = estado.slots.iter().position(|s| *s == Some(a));
            let dica = match onde {
                Some(i) => shared::idioma::tr(&format!("In skill slot {}", i + 4)).into_owned(),
                None if estado.slots_abertos == 0 => shared::idioma::tr("Skill slot 4 opens at the first rebirth").into_owned(),
                None => shared::idioma::tr("Tap a slot to equip it").into_owned(),
            };
            estilo::texto_ajustado(&dica, x, y + 14.0 * f, larg, 13, if onde.is_some() { estilo::VERDE } else { estilo::SUAVE });
            y += 24.0 * f;
            let gap = 8.0 * f;
            let bw = (larg - 2.0 * gap) / 3.0;
            for i in 0..av::SLOTS_EXTRAS {
                let b = Rect::new(x + i as f32 * (bw + gap), y, bw, 44.0 * f);
                let aberto = i < estado.slots_abertos as usize;
                let aqui = estado.slots[i] == Some(a);
                let rotulo = if aberto { (i + 4).to_string() } else { format!("R{}", av::ABRE_SLOT_EM[i]) };
                estilo::botao(b, &rotulo, estilo::estado_de(b, !aberto, aqui), aqui);
                if clique && aberto && b.toca(m) {
                    let ativa = if aqui { None } else { Some(a) };
                    pedido = Some(ClientMessage::Arvore { pedido: PedidoArvore::Equipar { slot: i as u8, ativa } });
                }
            }
        } else if n.tipo != TipoNo::Inicio && !tem {
            let b = Rect::new(x, y, larg, 46.0 * f);
            let pode = self.pode_aprender(id);
            let rotulo = format!("Learn ({} pts)", n.custo);
            let sobre = b.toca(m);
            crate::loja_tp::botao_ouro(b, &shared::idioma::tr(&rotulo), pode, sobre && pode, f, agora);
            if clique && pode && b.toca(m) {
                pedido = Some(ClientMessage::Arvore { pedido: PedidoArvore::Aprender { no: id } });
            }
        }
        pedido
    }

    /// "Reset all paths?": the points come back, the gold goes.
    fn desenha_confirmacao(&mut self, ouro: u64) -> Option<ClientMessage> {
        let estado = self.estado?;
        let f = estilo::fator_texto();
        let seguro = crate::hud_layout::tela_segura();
        let (w, h) = ((560.0 * f).min(seguro.w - 24.0), (250.0 * f).min(seguro.h - 24.0));
        let r = Rect::new(seguro.center().x - w * 0.5, seguro.center().y - h * 0.5, w, h);
        crate::hud_layout::escurece(0.5);
        estilo::sombra(r, 10.0, 1.4);
        estilo::ret_gradiente(r, 10.0, Color::new(0.10, 0.08, 0.13, 0.98), Color::new(0.05, 0.04, 0.07, 0.98));
        estilo::borda_arredondada(r, 10.0, 2.0, estilo::alfa(estilo::OURO, 0.8));
        let x = r.x + 24.0 * f;
        let larg = w - 48.0 * f;
        let custo = estado.custo_redistribuir;
        estilo::texto_forte(x, r.y + 42.0 * f, "Reset all paths?", 22, estilo::OURO);
        estilo::texto_ajustado(
            &shared::idioma::tr(&format!(
                "All {} points come back to spend again, for {} gold.",
                estado.arvore.gastos(),
                crate::bolsa::milhar(custo)
            )),
            x,
            r.y + 84.0 * f,
            larg,
            15,
            estilo::TEXTO,
        );
        let paga = ouro >= custo;
        estilo::ouro_texto(
            x,
            r.y + 118.0 * f,
            &shared::idioma::tr(&format!("You have {} gold.", crate::bolsa::milhar(ouro))),
            14,
            if paga { estilo::SUAVE } else { estilo::VERMELHO },
            false,
        );
        let bw = (w - 60.0 * f) * 0.5;
        let bh = 46.0 * f;
        let by = r.y + r.h - 22.0 * f - bh;
        let cancelar = Rect::new(x, by, bw, bh);
        let ok = Rect::new(cancelar.x + bw + 12.0 * f, by, bw, bh);
        estilo::botao(cancelar, "Cancel", estilo::estado_de(cancelar, false, false), false);
        estilo::botao(ok, "Reset", estilo::estado_de(ok, !paga, false), paga);
        if !crate::foco::clique() {
            return None;
        }
        let m = Vec2::from(mouse_position());
        if ok.toca(m) && paga {
            self.confirmar_redistribuir = false;
            return Some(ClientMessage::Arvore { pedido: PedidoArvore::Redistribuir });
        }
        if cancelar.toca(m) || !r.toca(m) {
            self.confirmar_redistribuir = false;
        }
        None
    }
}

/// The point where the finger (or the mouse) went down this frame.
fn apertou_em() -> Option<Vec2> {
    if let Some(t) = touches().into_iter().find(|t| t.phase == TouchPhase::Started) {
        return Some(t.position);
    }
    is_mouse_button_pressed(MouseButton::Left).then(|| Vec2::from(mouse_position()))
}

/// The night sky: gradient, the four nebulae, stars (with a slow parallax
/// and twinkle), the start's glow and a vignette.
fn ceu(r: Rect, centro: Vec2, cam: Vec2, escala: f32, base: f32, agora: f64) {
    estilo::ret_gradiente(r, 10.0, CEU_TOPO, CEU_BASE);
    // Nebulae behind each arm (half parallax: they sit farther away).
    for c in Caminho::TODOS {
        let a = av::ANGULO_DO_CAMINHO[c as usize].to_radians();
        for (k, (dist, raio, forca)) in [(6.5, 7.5, 0.10), (10.5, 5.0, 0.06)].iter().enumerate() {
            let p = vec2(a.cos(), a.sin()) * *dist;
            let q = centro + (p - cam * 0.6) * escala * 0.8;
            let tom = if k == 1 { estilo::misturar(cor_do_caminho(c), Color::new(0.5, 0.3, 0.9, 1.0), 0.3) } else { cor_do_caminho(c) };
            let raio_px = (raio * escala).min(r.w.max(r.h) * 0.55);
            crate::loja_tp::brilho_radial(q, raio_px, tom, *forca);
        }
    }
    // Stars: a field that wraps, drifting slower than the web.
    let deriva = -cam * base * 0.25;
    for i in 0..260u32 {
        let sx = sorte(i * 3 + 1) * r.w;
        let sy = sorte(i * 3 + 2) * r.h;
        let x = r.x + (sx + deriva.x).rem_euclid(r.w);
        let y = r.y + (sy + deriva.y).rem_euclid(r.h);
        let brilho = 0.25 + 0.65 * sorte(i * 7 + 5);
        let pisca = 0.65 + 0.35 * ((agora as f32) * (0.8 + 2.2 * sorte(i * 11)) + i as f32).sin();
        let tam = 0.6 + 1.5 * sorte(i * 13 + 3).powi(3);
        let tom = if i % 7 == 0 {
            Color::new(0.75, 0.82, 1.0, 1.0)
        } else if i % 11 == 0 {
            Color::new(1.0, 0.86, 0.66, 1.0)
        } else {
            WHITE
        };
        if tam > 1.6 {
            draw_circle(x, y, tam * 3.0, estilo::alfa(tom, 0.05 * pisca));
        }
        draw_circle(x, y, tam, estilo::alfa(tom, brilho * pisca));
    }
    // The start's warm light.
    let c0 = centro - cam * escala;
    crate::loja_tp::brilho_radial(c0, 3.0 * escala, estilo::OURO, 0.10);
    // Vignette: the edges fall into darkness.
    let passos = 14;
    for i in 0..passos {
        let k = i as f32 / passos as f32;
        let d = k * r.w.min(r.h) * 0.22;
        let a = 0.055 * (1.0 - k);
        let cor = Color::new(0.0, 0.0, 0.0, a);
        draw_rectangle(r.x, r.y + d, r.w, (r.h * 0.03).max(4.0), cor);
        draw_rectangle(r.x, r.y + r.h - d - (r.h * 0.03).max(4.0), r.w, (r.h * 0.03).max(4.0), cor);
        draw_rectangle(r.x + d, r.y, (r.w * 0.02).max(4.0), r.h, cor);
        draw_rectangle(r.x + r.w - d - (r.w * 0.02).max(4.0), r.y, (r.w * 0.02).max(4.0), r.h, cor);
    }
}

fn circulo_tracejado(c: Vec2, raio: f32, w: f32, cor: Color) {
    let n = ((raio * TAU / 14.0) as usize).clamp(24, 360);
    for i in (0..n).step_by(2) {
        let a0 = TAU * i as f32 / n as f32;
        let a1 = TAU * (i + 1) as f32 / n as f32;
        let p = c + Vec2::from_angle(a0) * raio;
        let q = c + Vec2::from_angle(a1) * raio;
        draw_line(p.x, p.y, q.x, q.y, w, cor);
    }
}

/// One node: its halo, body, ring(s) and glyph, by kind and state.
#[allow(clippy::too_many_arguments)]
fn desenha_no(n: &No, p: Vec2, r: f32, tem: bool, pode: bool, barato: bool, suprema: bool, slot: Option<usize>, agora: f64) {
    let cor = cor_do_no(n);
    let t = agora as f32;
    let pulso = 0.5 + 0.5 * (t * 3.0 + n.id as f32 * 0.7).sin();
    // Halo: owned glow steady and strong, learnable pulses.
    if tem {
        crate::loja_tp::brilho_radial(p, r * 2.4, cor, 0.42);
    } else if pode && barato {
        crate::loja_tp::brilho_radial(p, r * (1.8 + 0.4 * pulso), cor, 0.16 + 0.16 * pulso);
    }
    let corpo = if tem { estilo::misturar(FUNDO_NO, cor, 0.28) } else { FUNDO_NO };
    let anel = if tem {
        estilo::clarear(cor, 0.2)
    } else if pode {
        estilo::alfa(cor, 0.55 + 0.35 * pulso * barato as u8 as f32)
    } else {
        estilo::alfa(APAGADO, 0.75)
    };
    let tinta = if tem {
        estilo::misturar(WHITE, cor, 0.25)
    } else if pode {
        estilo::alfa(estilo::misturar(WHITE, cor, 0.5), 0.85)
    } else {
        estilo::alfa(APAGADO, 0.9)
    };
    let ouro = if tem { estilo::OURO } else if pode { estilo::alfa(estilo::OURO, 0.75) } else { estilo::alfa(Color::new(0.55, 0.47, 0.32, 1.0), 0.8) };
    let esp = (r * 0.13).max(1.4);
    match n.tipo {
        TipoNo::Inicio => {
            // A compass rose in the four paths' colours.
            crate::loja_tp::brilho_radial(p, r * 2.6, estilo::OURO, 0.30);
            draw_circle(p.x, p.y, r, Color::new(0.10, 0.08, 0.06, 1.0));
            for c in Caminho::TODOS {
                let a = av::ANGULO_DO_CAMINHO[c as usize].to_radians();
                let ponta = p + Vec2::from_angle(a) * r * 0.92;
                let e1 = p + Vec2::from_angle(a - 0.5) * r * 0.30;
                let e2 = p + Vec2::from_angle(a + 0.5) * r * 0.30;
                draw_triangle(ponta, e1, e2, cor_do_caminho(c));
            }
            draw_circle_lines(p.x, p.y, r, esp, estilo::OURO);
            draw_circle_lines(p.x, p.y, r * 1.18, esp * 0.5, estilo::alfa(estilo::OURO, 0.5));
            draw_circle(p.x, p.y, r * 0.18, estilo::OURO);
        }
        TipoNo::Menor => {
            draw_circle(p.x, p.y, r, corpo);
            draw_circle_lines(p.x, p.y, r, esp, anel);
            icones::desenha(glifo_do_no(n), p, r * 0.58, tinta);
        }
        TipoNo::Notavel => {
            draw_circle(p.x, p.y, r, corpo);
            draw_circle_lines(p.x, p.y, r, esp * 1.2, ouro);
            draw_circle_lines(p.x, p.y, r * 0.80, esp * 0.6, anel);
            icones::desenha(glifo_do_no(n), p, r * 0.52, tinta);
        }
        TipoNo::Chave => {
            // Ornate: eight spikes, a double gold ring, the path's inner ring.
            for i in 0..8 {
                let a = i as f32 * TAU / 8.0 + PI / 8.0 + if tem { t * 0.15 } else { 0.0 };
                let ponta = p + Vec2::from_angle(a) * r * 1.32;
                let e1 = p + Vec2::from_angle(a - 0.16) * r * 1.02;
                let e2 = p + Vec2::from_angle(a + 0.16) * r * 1.02;
                draw_triangle(ponta, e1, e2, ouro);
            }
            draw_circle(p.x, p.y, r * 1.06, Color::new(0.03, 0.02, 0.04, 1.0));
            draw_circle(p.x, p.y, r, corpo);
            draw_circle_lines(p.x, p.y, r * 1.06, esp * 1.1, ouro);
            draw_circle_lines(p.x, p.y, r * 0.88, esp * 0.7, ouro);
            draw_circle_lines(p.x, p.y, r * 0.74, esp * 0.5, anel);
            icones::desenha(glifo_do_no(n), p, r * 0.48, tinta);
        }
        TipoNo::Ativa => {
            // A hexagon in the path colour, the skill's own icon inside.
            draw_poly(p.x, p.y, 6, r * 1.08, 30.0, if tem { estilo::alfa(cor, 0.95) } else { estilo::alfa(cor, 0.35) });
            draw_poly(p.x, p.y, 6, r * 0.92, 30.0, corpo);
            draw_poly_lines(p.x, p.y, 6, r * 1.08, 30.0, esp, if tem { estilo::clarear(cor, 0.3) } else { anel });
            let a = n.ativa.unwrap_or(0);
            let icone = crate::ativas::icone_da_ativa(a);
            let alfa = if tem { 1.0 } else if pode { 0.75 } else { 0.4 };
            if !crate::icones_ui::skill(icone, p, r * 1.25, alfa) {
                estilo::icone(icone, p, r * 0.55, estilo::alfa(tinta, alfa));
            }
            if suprema {
                // The supreme crown.
                let c = p + vec2(0.0, -r * 1.28);
                let s = r * 0.30;
                draw_triangle(c + vec2(-s, s * 0.5), c + vec2(s, s * 0.5), c + vec2(0.0, -s * 0.6), estilo::OURO);
                draw_triangle(c + vec2(-s, s * 0.5), c + vec2(-s * 0.4, s * 0.5), c + vec2(-s * 0.9, -s * 0.5), estilo::OURO);
                draw_triangle(c + vec2(s, s * 0.5), c + vec2(s * 0.4, s * 0.5), c + vec2(s * 0.9, -s * 0.5), estilo::OURO);
            }
            if let Some(i) = slot {
                let b = p + vec2(r * 0.9, r * 0.8);
                draw_circle(b.x, b.y, r * 0.36, estilo::OURO);
                estilo::texto_centro_forte(b.x, b.y + r * 0.16, &(i + 4).to_string(), ((r * 0.5) / estilo::fator_texto()).max(8.0) as u16, Color::new(0.1, 0.07, 0.03, 1.0));
            }
        }
    }
}

fn anel_de_selecao(p: Vec2, r: f32, agora: f64) {
    let giro = agora as f32 * 1.2;
    for k in 0..12 {
        let a = giro + k as f32 * TAU / 12.0;
        estilo::arco(p, r, a, 0.045, 2.5, estilo::alfa(WHITE, 0.85));
    }
    draw_circle_lines(p.x, p.y, r + 4.0, 1.0, estilo::alfa(estilo::OURO, 0.35));
}

/// The learn burst: an expanding ring and sparks flying out.
fn explosao(p: Vec2, r: f32, cor: Color, t: f32, semente: u16) {
    let k = (t / 1.1).clamp(0.0, 1.0);
    let some = 1.0 - k;
    draw_circle_lines(p.x, p.y, r * (1.0 + 3.0 * k), (4.0 * some).max(0.5), estilo::alfa(estilo::clarear(cor, 0.3), some));
    draw_circle_lines(p.x, p.y, r * (1.0 + 1.8 * k), (2.0 * some).max(0.5), estilo::alfa(WHITE, some * 0.8));
    crate::loja_tp::brilho_radial(p, r * (2.0 + 2.0 * k), cor, 0.5 * some);
    for i in 0..14u32 {
        let a = TAU * i as f32 / 14.0 + sorte(semente as u32 * 17 + i);
        let v = 1.6 + 2.6 * sorte(semente as u32 * 5 + i * 3);
        let q = p + Vec2::from_angle(a) * r * (1.0 + v * k);
        draw_circle(q.x, q.y, (r * 0.12 * some).max(0.6), estilo::alfa(if i % 2 == 0 { WHITE } else { cor }, some));
    }
}

/// One header counter: "Available 12".
fn chip(p: Vec2, rotulo: &str, valor: u32, cor: Color, f: f32) -> f32 {
    let r_txt = shared::idioma::tr(rotulo).into_owned();
    let v = valor.to_string();
    let w = estilo::medir(&r_txt, 12) + estilo::medir_forte(&v, 18) + 26.0 * f;
    let r = Rect::new(p.x, p.y, w, 38.0 * f);
    estilo::ret_arredondado(r, r.h * 0.5, Color::new(0.0, 0.0, 0.0, 0.35));
    estilo::borda_arredondada(r, r.h * 0.5, 1.0, estilo::alfa(cor, 0.55));
    estilo::texto(r.x + 12.0 * f, r.y + 24.0 * f, &r_txt, 12, estilo::SUAVE);
    estilo::texto_forte(r.x + 18.0 * f + estilo::medir(&r_txt, 12), r.y + 26.0 * f, &v, 18, cor);
    w
}

/// A round-cornered control on the sky (Center, +, −).
fn controle(r: Rect, rotulo: &str, _clique: bool, m: Vec2) {
    let sobre = r.toca(m);
    estilo::ret_arredondado(r, 8.0, Color::new(0.06, 0.05, 0.09, if sobre { 0.95 } else { 0.8 }));
    estilo::borda_arredondada(r, 8.0, 1.0, estilo::alfa(estilo::OURO, if sobre { 0.8 } else { 0.4 }));
    let f = estilo::fator_texto();
    if rotulo == "Center" {
        let c = vec2(r.x + 20.0 * f, r.center().y);
        icones::desenha(Glifo::Mira, c, 10.0 * f, estilo::ACENTO);
        estilo::texto(r.x + 36.0 * f, r.center().y + 5.0 * f, rotulo, 14, estilo::TEXTO);
    } else {
        estilo::texto_centro_forte(r.center().x, r.center().y + 8.0 * f, rotulo, 22, estilo::TEXTO);
    }
}

/// Desktop: a small tooltip next to the hovered node.
fn dica_flutuante(n: &No, estado: &EstadoArvore, p: Vec2, r: f32, canvas: Rect) {
    let f = estilo::fator_texto();
    let titulo = titulo(n);
    let sub = subtitulo(n);
    let linhas: Vec<(String, bool)> = if n.nome.is_empty() {
        n.efeitos().iter().skip(1).map(|&(e, v)| (linha_traduzida(e, v), e.bom(v))).collect()
    } else {
        n.efeitos().iter().map(|&(e, v)| (linha_traduzida(e, v), e.bom(v))).collect()
    };
    let custo = if n.tipo == TipoNo::Inicio { String::new() } else { shared::idioma::tr(&format!("Cost: {} points", n.custo)).into_owned() };
    let estado_txt = if estado.arvore.tem(n.id) {
        shared::idioma::tr("Learned").into_owned()
    } else {
        shared::idioma::tr("Click to select").into_owned()
    };
    let mut w = estilo::medir_forte(&titulo, 15).max(estilo::medir(&sub, 12));
    for (l, _) in &linhas {
        w = w.max(estilo::medir(l, 13));
    }
    w = w.max(estilo::medir(&format!("{custo}   {estado_txt}"), 12)) + 24.0 * f;
    let h = (52.0 + 19.0 * linhas.len() as f32 + 22.0) * f;
    let mut x = p.x + r + 14.0 * f;
    if x + w > canvas.x + canvas.w - 8.0 {
        x = p.x - r - 14.0 * f - w;
    }
    let y = (p.y - h * 0.5).clamp(canvas.y + 6.0, canvas.y + canvas.h - h - 6.0);
    let b = Rect::new(x, y, w, h);
    estilo::sombra(b, 8.0, 1.0);
    estilo::ret_arredondado(b, 8.0, Color::new(0.05, 0.04, 0.08, 0.95));
    estilo::borda_arredondada(b, 8.0, 1.5, estilo::alfa(cor_do_no(n), 0.8));
    estilo::texto_forte(b.x + 12.0 * f, b.y + 22.0 * f, &titulo, 15, if n.nome.is_empty() { estilo::TEXTO } else { estilo::ACENTO });
    estilo::texto(b.x + 12.0 * f, b.y + 40.0 * f, &sub, 12, estilo::alfa(cor_do_no(n), 0.95));
    let mut yy = b.y + 60.0 * f;
    for (l, bom) in &linhas {
        estilo::texto(b.x + 12.0 * f, yy, l, 13, if *bom { Color::new(0.62, 0.92, 0.66, 1.0) } else { estilo::VERMELHO });
        yy += 19.0 * f;
    }
    estilo::texto(b.x + 12.0 * f, yy + 4.0 * f, &format!("{custo}   {estado_txt}"), 12, estilo::SUAVE);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn com(nos: [u8; 4], ganhos: u32) -> ArvoreUi {
        let mut ui = ArvoreUi::default();
        ui.receber(EstadoArvore::de(av::Arvore::com_nos(nos), ganhos));
        ui
    }

    #[test]
    fn so_aprende_vizinho_com_ponto_livre() {
        let ui = ArvoreUi::default();
        assert!(!ui.pode_aprender(1), "no state, no promise");
        let ui = com([2, 0, 0, 0], 5);
        let s2 = av::id_no_braco(Caminho::Assassino, 2);
        assert!(ui.pode_aprender(s2));
        assert!(!ui.pode_aprender(av::id_no_braco(Caminho::Assassino, 1)), "already learned");
        assert!(!ui.pode_aprender(av::id_no_braco(Caminho::Assassino, 3)), "not connected");
        assert!(ui.pode_aprender(av::id_no_braco(Caminho::Corsario, 0)));
        assert!(!ui.pode_aprender(9_999));
        let ui = com([2, 0, 0, 0], 2);
        assert!(!ui.pode_aprender(s2), "no free point");
        assert!(!ui.tem_ponto_livre());
        assert!(com([2, 0, 0, 0], 3).tem_ponto_livre());
        let tudo = av::Arvore::com_ids(&(1..av::NOS_TOTAL as u16).collect::<Vec<_>>());
        let mut cheio = ArvoreUi::default();
        cheio.receber(EstadoArvore::de(tudo, 600));
        assert!(!cheio.tem_ponto_livre(), "a full web has nothing to spend on");
    }

    #[test]
    fn ganhar_um_no_dispara_a_explosao() {
        let mut ui = com([2, 0, 0, 0], 10);
        ui.relogio = Some(5.0);
        assert!(ui.explosoes.is_empty(), "the first state is not a purchase");
        ui.receber(EstadoArvore::de(av::Arvore::com_nos([3, 0, 0, 0]), 10));
        assert_eq!(ui.explosoes.len(), 1);
        assert_eq!(ui.explosoes[0].0, av::id_no_braco(Caminho::Assassino, 2));
    }

    #[test]
    fn linhas_traduzidas_e_titulos() {
        let n = av::no(av::id_no_braco(Caminho::Assassino, 0)).unwrap();
        assert_eq!(titulo(n), "Crit chance +0.5%");
        let k = av::no(av::id_no_braco(Caminho::Assassino, 28)).unwrap();
        assert_eq!(titulo(k), "Death's Edge");
        assert_eq!(linha_traduzida(av::Efeito::VidaPct, -0.15), "Max health -15%");
        assert!(subtitulo(k).contains("Keystone"));
    }

    /// Phones: at the default zoom a minor node is at least 36 px across,
    /// and no two nodes overlap on screen at the smallest zoom either.
    #[test]
    fn layout_cabe_e_os_nos_nao_se_sobrepoem() {
        for (w, h) in [(2400.0, 1080.0), (1080.0, 2160.0), (1280.0, 720.0), (1920.0, 1080.0), (812.0, 375.0)] {
            let seguro = Rect::new(0.0, 0.0, w, h);
            for f in [1.0, 1.3, 2.0] {
                let l = layout(seguro, f);
                assert!(l.canvas.h > 150.0 && l.canvas.w > 150.0, "{w}x{h} f{f}");
                assert!(l.painel.contains(vec2(l.doca.x + 1.0, l.doca.y + 1.0)));
                assert!(l.doca.x + l.doca.w <= l.painel.x + l.painel.w + 0.5);
                assert!(l.doca.y + l.doca.h <= l.painel.y + l.painel.h + 0.5);
                assert!(l.mais.y > l.centro.y && l.menos.y > l.mais.y);
                let base = px_base(f, true);
                assert!(2.0 * raio_u(TipoNo::Menor) * base >= 36.0, "a minor is a finger target");
                let zmin = zoom_min(l.canvas, base);
                assert!(zmin <= 1.0);
                // Whole web inside the canvas at the smallest zoom.
                assert!(2.0 * ALCANCE_U * base * zmin <= l.canvas.w.min(l.canvas.h) + 0.5);
            }
        }
        // On screen, at any zoom, node circles keep apart (the geometry is
        // in tree units, so one scale is enough).
        let nos = av::nos();
        for a in nos {
            for b in nos.iter().filter(|b| b.id > a.id) {
                let d = vec2(a.x - b.x, a.y - b.y).length();
                assert!(d > raio_u(a.tipo) + raio_u(b.tipo), "{} {}", a.id, b.id);
            }
        }
        // The ring guides sit between the rings' nodes.
        for n in nos.iter().filter(|n| n.anel > 0 && n.ponte.is_none()) {
            let d = vec2(n.x, n.y).length();
            assert!(d < GUIAS_U[n.anel as usize - 1], "{} at {d}", n.id);
            if n.anel > 1 {
                assert!(d > GUIAS_U[n.anel as usize - 2], "{} at {d}", n.id);
            }
            assert!(d <= ALCANCE_U);
        }
    }

    #[test]
    fn zoom_mantem_o_ponto_sob_o_dedo() {
        let mut ui = ArvoreUi::default();
        let centro = vec2(500.0, 300.0);
        let p = vec2(700.0, 250.0);
        let base = 40.0;
        let antes = ui.cam + (p - centro) / (base * ui.zoom);
        ui.zoom_em(p, centro, base, 2.0);
        let depois = ui.cam + (p - centro) / (base * ui.zoom);
        assert!(antes.distance(depois) < 1e-4);
    }
}

/// Local captures of the screen, with no connection
/// (`MMO_PREVIA_ARVORE=1`, `MMO_PREVIA_SAIDA` for the folder).
#[cfg(debug_assertions)]
pub async fn previa() {
    let saida = std::env::var("MMO_PREVIA_SAIDA").unwrap_or_else(|_| "target/arvore_shots".into());
    std::fs::create_dir_all(&saida).unwrap();
    next_frame().await;
    let rt = render_target(screen_width() as u32, screen_height() as u32);
    crate::render3d::define_alvo(Some(rt.clone()));
    let foto = |ui: &mut ArvoreUi, nome: &str| {
        crate::render3d::camera_padrao();
        clear_background(Color::new(0.08, 0.12, 0.16, 1.0));
        ui.desenha(312_500);
        unsafe { get_internal_gl().flush() };
        rt.texture.get_texture_data().export_png(&format!("{saida}/{nome}.png"));
    };
    // A rebirth-12 build: Assassin deep with a keystone, a bridge into Mage,
    // some Guardian.
    let mut a = av::Arvore::com_nos([13, 4, 6, 0]);
    for l in [4, 5, 6, 14, 15, 16, 7, 8, 9] {
        a.aprender(av::id_no_braco(Caminho::Assassino, l), 999).unwrap();
    }
    a.aprender(av::id_no_braco(Caminho::Mago, 4), 999).ok();
    a.slots = [Some(0), Some(2), Some(1)];
    let ganhos = a.gastos() + 9;
    let mut ui = ArvoreUi::default();
    ui.relogio = Some(12.3);
    ui.receber(EstadoArvore::de(a, ganhos).com_renascimentos(12));
    ui.abrir();
    for _ in 0..3 {
        foto(&mut ui, "arvore");
        next_frame().await;
    }
    // Zoomed out: the whole web.
    ui.zoom_alvo = 0.3;
    ui.zoom = 0.3;
    foto(&mut ui, "arvore-inteira");
    // A keystone selected, the camera on it.
    let k = av::id_no_braco(Caminho::Guardiao, 33);
    let n = av::no(k).unwrap();
    ui.selecionado = Some(k);
    ui.cam = vec2(n.x, n.y) * 0.7;
    ui.cam_alvo = ui.cam;
    ui.zoom = 1.1;
    ui.zoom_alvo = 1.1;
    foto(&mut ui, "arvore-chave");
    // A learnable notable selected, near the owned web.
    let nt = av::id_no_braco(Caminho::Assassino, 31);
    let n = av::no(nt).unwrap();
    ui.selecionado = Some(nt);
    ui.cam = vec2(n.x, n.y) * 0.8;
    ui.cam_alvo = ui.cam;
    foto(&mut ui, "arvore-notavel");
    // An owned active: its slots.
    let at = av::no_da_ativa(2).unwrap().1;
    let n = av::no(at).unwrap();
    ui.selecionado = Some(at);
    ui.cam = vec2(n.x, n.y) * 0.8;
    ui.cam_alvo = ui.cam;
    foto(&mut ui, "arvore-ativa");
    // A learn burst in the middle of its animation.
    ui.selecionado = None;
    ui.cam = Vec2::ZERO;
    ui.cam_alvo = Vec2::ZERO;
    ui.zoom = 1.0;
    ui.zoom_alvo = 1.0;
    let mut b = a;
    b.aprender(av::id_no_braco(Caminho::Guardiao, 4), 999).unwrap();
    ui.relogio = Some(12.0);
    ui.receber(EstadoArvore::de(b, ganhos + 5).com_renascimentos(12));
    ui.relogio = Some(12.35);
    foto(&mut ui, "arvore-explosao");
    // The mouse tooltip on a notable bridge.
    let ponte = av::nos().iter().find(|n| n.ponte.is_some() && n.tipo == TipoNo::Notavel).unwrap();
    ui.sobre = Some(ponte.id);
    ui.cam = vec2(ponte.x, ponte.y) * 0.6;
    ui.cam_alvo = ui.cam;
    foto(&mut ui, "arvore-dica");
    ui.sobre = None;
    ui.cam = Vec2::ZERO;
    ui.cam_alvo = Vec2::ZERO;
    // The reset confirmation.
    ui.confirmar_redistribuir = true;
    foto(&mut ui, "arvore-reset");
    ui.confirmar_redistribuir = false;
    // The glyph sheet.
    crate::render3d::camera_padrao();
    clear_background(CEU_BASE);
    let lado = 64.0;
    for (i, g) in icones::TODOS.iter().enumerate() {
        let c = vec2(60.0 + (i % 9) as f32 * 90.0, 70.0 + (i / 9) as f32 * 100.0);
        draw_circle(c.x, c.y, lado * 0.5, FUNDO_NO);
        draw_circle_lines(c.x, c.y, lado * 0.5, 2.0, estilo::OURO);
        icones::desenha(*g, c, lado * 0.32, Color::new(0.95, 0.9, 0.8, 1.0));
        estilo::texto_centro(c.x, c.y + lado * 0.5 + 16.0, &format!("{g:?}"), 11, estilo::SUAVE);
    }
    unsafe { get_internal_gl().flush() };
    rt.texture.get_texture_data().export_png(&format!("{saida}/arvore-icones.png"));
    // Portrait (a phone held upright), when the window can be resized.
    let (pw, ph) = (540.0, 1080.0);
    request_new_screen_size(pw, ph);
    for _ in 0..4 {
        next_frame().await;
    }
    if (screen_width() - pw).abs() < 2.0 {
        let rt2 = render_target(pw as u32, ph as u32);
        crate::render3d::define_alvo(Some(rt2.clone()));
        let foto2 = |ui: &mut ArvoreUi, nome: &str| {
            crate::render3d::camera_padrao();
            clear_background(Color::new(0.08, 0.12, 0.16, 1.0));
            ui.desenha(312_500);
            unsafe { get_internal_gl().flush() };
            rt2.texture.get_texture_data().export_png(&format!("{saida}/{nome}.png"));
        };
        ui.zoom_pronto = false;
        foto2(&mut ui, "arvore-retrato");
        ui.selecionado = Some(nt);
        foto2(&mut ui, "arvore-retrato-cartao");
    }
}
