//! Summoning scroll reveal. The prize already arrives decided by the server;
//! this is presentation only, so closing/skipping never changes the gain.

#[allow(unused_imports)]
use crate::ui::Toque as _;
use macroquad::material::Material;
use macroquad::prelude::*;
use shared::loja::PremioInvocacao;

use crate::hud_estilo as estilo;
use crate::vox::VoxCache;

/// The closed scroll used in the shop and the bag. It has rolls, ribbons and
/// a seal in layers so it stays legible even in a small slot.
pub fn icone_pergaminho(c: Vec2, lado: f32, selo: Color) {
    let w = lado * 0.58;
    let h = lado * 0.76;
    let sombra = Color::new(0.08, 0.03, 0.10, 0.52);
    let papel_escuro = Color::new(0.42, 0.23, 0.12, 1.0);
    let papel = Color::new(0.96, 0.79, 0.43, 1.0);
    let luz = Color::new(1.0, 0.92, 0.66, 1.0);
    draw_rectangle(
        c.x - w * 0.43 + lado * 0.05,
        c.y - h * 0.5 + lado * 0.06,
        w * 0.86,
        h,
        sombra,
    );
    draw_rectangle(c.x - w * 0.43, c.y - h * 0.5, w * 0.86, h, papel);
    draw_triangle(
        vec2(c.x - w * 0.43, c.y - h * 0.5),
        vec2(c.x - w * 0.15, c.y - h * 0.5),
        vec2(c.x - w * 0.43, c.y - h * 0.29),
        papel_escuro,
    );
    for y in [-h * 0.50, h * 0.50] {
        draw_rectangle(
            c.x - w * 0.58,
            c.y + y - lado * 0.055,
            w * 1.16,
            lado * 0.11,
            papel_escuro,
        );
        draw_circle(c.x - w * 0.58, c.y + y, lado * 0.07, luz);
        draw_circle(c.x + w * 0.58, c.y + y, lado * 0.07, luz);
    }
    for dy in [-0.18, -0.04, 0.10] {
        draw_line(
            c.x - w * 0.27,
            c.y + h * dy,
            c.x + w * 0.27,
            c.y + h * dy,
            lado * 0.018,
            papel_escuro,
        );
    }
    draw_circle(
        c.x,
        c.y + h * 0.29,
        lado * 0.16,
        Color::new(0.20, 0.04, 0.08, 1.0),
    );
    draw_circle(c.x, c.y + h * 0.29, lado * 0.125, selo);
    draw_poly(c.x, c.y + h * 0.29, 4, lado * 0.065, 45.0, luz);
}

/// Varies the seal by content: a key, a mount's head or an open book.
pub fn icone_pergaminho_de(item_id: u16, c: Vec2, lado: f32) {
    use shared::item_id as it;
    let cor = match item_id {
        it::PERGAMINHO_INVOCA_MONTARIA => estilo::OURO,
        it::PERGAMINHO_INVOCA_TOMO => estilo::VERDE,
        _ => Color::new(0.35, 0.78, 1.0, 1.0),
    };
    icone_pergaminho(c, lado, cor);
    let y = c.y + lado * 0.76 * 0.29;
    let tinta = Color::new(0.14, 0.07, 0.12, 0.95);
    match item_id {
        it::PERGAMINHO_INVOCA_CHAVE => {
            draw_circle_lines(c.x - lado * 0.025, y, lado * 0.045, lado * 0.018, tinta);
            draw_line(
                c.x + lado * 0.02,
                y,
                c.x + lado * 0.09,
                y,
                lado * 0.018,
                tinta,
            );
            draw_line(
                c.x + lado * 0.07,
                y,
                c.x + lado * 0.07,
                y + lado * 0.035,
                lado * 0.014,
                tinta,
            );
        }
        it::PERGAMINHO_INVOCA_MONTARIA => {
            draw_triangle(
                vec2(c.x, y - lado * 0.055),
                vec2(c.x - lado * 0.07, y + lado * 0.04),
                vec2(c.x, y + lado * 0.015),
                tinta,
            );
            draw_triangle(
                vec2(c.x, y - lado * 0.055),
                vec2(c.x + lado * 0.07, y + lado * 0.04),
                vec2(c.x, y + lado * 0.015),
                tinta,
            );
        }
        it::PERGAMINHO_INVOCA_TOMO => {
            draw_triangle(
                vec2(c.x, y + lado * 0.045),
                vec2(c.x - lado * 0.085, y - lado * 0.025),
                vec2(c.x, y - lado * 0.005),
                tinta,
            );
            draw_triangle(
                vec2(c.x, y + lado * 0.045),
                vec2(c.x + lado * 0.085, y - lado * 0.025),
                vec2(c.x, y - lado * 0.005),
                tinta,
            );
            draw_line(
                c.x,
                y - lado * 0.025,
                c.x,
                y + lado * 0.045,
                lado * 0.012,
                tinta,
            );
        }
        _ => {}
    }
}

#[derive(Default)]
pub struct InvocacaoUi {
    atual: Option<(Vec<PremioInvocacao>, f64)>,
}

impl InvocacaoUi {
    pub fn abrir(&mut self, premio: PremioInvocacao, agora: f64) {
        self.abrir_varias(vec![premio], agora);
    }

    pub fn abrir_varias(&mut self, premios: Vec<PremioInvocacao>, agora: f64) {
        if !premios.is_empty() {
            self.atual = Some((premios, agora));
        }
    }

    pub fn aberta(&self) -> bool {
        self.atual.is_some()
    }

    pub fn fechar(&mut self) {
        self.atual = None;
    }

    pub fn desenha(
        &mut self,
        nomes: &std::collections::HashMap<u16, String>,
        skills: &[shared::skills::Skill],
        vox: &VoxCache,
        solido: &Material,
        agora: f64,
    ) {
        let Some((premios, inicio)) = self.atual.clone() else {
            return;
        };
        let t = (agora - inicio).max(0.0) as f32;
        let seguro = crate::hud_layout::tela_segura();
        let k = (seguro.w / BASE_W).min(seguro.h / BASE_H).clamp(0.5, 3.0);
        let acao = estilo::no_painel(k, || cena(&premios, nomes, skills, vox, solido, t, k, seguro));
        match acao {
            Acao::Nada => {}
            Acao::Pular(para) => self.atual = Some((premios, agora - para as f64)),
            Acao::Fechar => self.fechar(),
        }
    }
}

// ───────────────────────────── the egg sequence ─────────────────────────────
//
// Owner, 09/11/2026: "the animation to open scrolls, pet, mount etc is very
// ugly … something like pokemon evolution, like cracking an egg". So: an egg
// drops in, shakes three times (each shake cracks it more and leaks light in
// the colour of the best prize), turns white-hot like an evolution, flashes,
// the two halves of the shell fly apart and the prize pops out over rays.
// Everything is drawn on a 900×420 canvas scaled to the safe area.

const BASE_W: f32 = 900.0;
const BASE_H: f32 = 420.0;
/// The egg has landed.
const T_QUEDA: f32 = 0.45;
/// The three shakes (start, length).
const SACUDIDAS: [(f32, f32); 3] = [(0.62, 0.30), (1.08, 0.30), (1.50, 0.34)];
/// The egg starts glowing white.
const T_BRILHO: f32 = 1.70;
/// Flash: the shell bursts.
const T_ESTOURO: f32 = 2.20;
/// After the burst: when the single prize's button shows up.
const PRONTO_UNICO: f32 = 0.65;
/// After the burst: the first batch card, and the gap between cards.
const LOTE_INICIO: f32 = 0.30;
const LOTE_PASSO: f32 = 0.09;

enum Acao {
    Nada,
    /// Jump to this moment of the sequence (tap to skip).
    Pular(f32),
    Fechar,
}

struct Info {
    titulo: String,
    sub: String,
    cor: Color,
    raridade: u8,
}

fn cor_do_tomo(grau: shared::skills::GrauTomo) -> Color {
    match grau {
        shared::skills::GrauTomo::Verde => estilo::VERDE,
        shared::skills::GrauTomo::Roxo => Color::new(0.72, 0.42, 1.0, 1.0),
        shared::skills::GrauTomo::Lendario => estilo::OURO,
    }
}

fn info(
    premio: &PremioInvocacao,
    nomes: &std::collections::HashMap<u16, String>,
    skills: &[shared::skills::Skill],
) -> Info {
    match premio {
        PremioInvocacao::Chave { item_id, cor } => Info {
            titulo: nomes.get(item_id).cloned().unwrap_or_else(|| "Crafting Key".into()),
            sub: format!("{} · Key", shared::forja::Grau::de_u8(*cor).map_or("Common", |g| g.nome())),
            // The colour IS the rarity (it used to be the same blue for every key).
            cor: cor_do_grau(*cor),
            raridade: *cor,
        },
        PremioInvocacao::Montaria { item_id } => {
            let d = shared::montarias::de_item(*item_id);
            let g = d.map_or(1, |(_, g)| g);
            Info {
                titulo: d.map_or("Mount".into(), |(e, _)| e.nome.to_string()),
                sub: format!("{} · Mount", shared::pets::nome_do_grau(g)),
                cor: cor_do_grau(g),
                raridade: g,
            }
        }
        PremioInvocacao::Pet { item_id } => {
            let d = shared::pets::de_item(*item_id);
            let g = d.map_or(1, |(_, g)| g);
            Info {
                titulo: d.map_or("Pet".into(), |(e, _)| e.nome.to_string()),
                sub: format!("{} · Pet", shared::pets::nome_do_grau(g)),
                cor: cor_do_grau(g),
                raridade: g,
            }
        }
        PremioInvocacao::Tomo { skill_id, grau, quantidade } => Info {
            titulo: skills
                .iter()
                .find(|s| s.id == *skill_id)
                .map_or("Tome", |s| s.nome.as_str())
                .to_string(),
            sub: format!("{} · now x{quantidade}", grau.nome()),
            cor: cor_do_tomo(*grau),
            raridade: match grau {
                shared::skills::GrauTomo::Verde => 2,
                shared::skills::GrauTomo::Roxo => 4,
                shared::skills::GrauTomo::Lendario => 5,
            },
        },
    }
}

/// Overshoots a little and settles: the "pop".
fn sai_volta(x: f32) -> f32 {
    let x = x.clamp(0.0, 1.0);
    let c1 = 1.70158;
    let c3 = c1 + 1.0;
    1.0 + c3 * (x - 1.0).powi(3) + c1 * (x - 1.0).powi(2)
}

fn sai(x: f32) -> f32 {
    1.0 - (1.0 - x.clamp(0.0, 1.0)).powi(3)
}

fn mistura(a: Color, b: Color, f: f32) -> Color {
    let f = f.clamp(0.0, 1.0);
    Color::new(a.r + (b.r - a.r) * f, a.g + (b.g - a.g) * f, a.b + (b.b - a.b) * f, a.a + (b.a - a.a) * f)
}

fn sorte(i: u32) -> f32 {
    ((i as f32 * 12.9898).sin() * 43758.547).fract().abs()
}

fn gira(v: Vec2, a: f32) -> Vec2 {
    let (s, c) = a.sin_cos();
    vec2(v.x * c - v.y * s, v.x * s + v.y * c)
}

/// A filled polygon (triangle fan from its centroid).
fn leque(pts: &[Vec2], cor: Color) {
    if pts.len() < 3 {
        return;
    }
    let cen = pts.iter().copied().sum::<Vec2>() / pts.len() as f32;
    for i in 0..pts.len() {
        draw_triangle(cen, pts[i], pts[(i + 1) % pts.len()], cor);
    }
}

/// A four-point twinkle.
fn brilhinho(c: Vec2, r: f32, cor: Color) {
    draw_triangle(c + vec2(0.0, -r), c + vec2(r * 0.22, 0.0), c + vec2(-r * 0.22, 0.0), cor);
    draw_triangle(c + vec2(0.0, r), c + vec2(r * 0.22, 0.0), c + vec2(-r * 0.22, 0.0), cor);
    draw_triangle(c + vec2(-r, 0.0), c + vec2(0.0, r * 0.22), c + vec2(0.0, -r * 0.22), cor);
    draw_triangle(c + vec2(r, 0.0), c + vec2(0.0, r * 0.22), c + vec2(0.0, -r * 0.22), cor);
}

/// The egg in its own coordinates (centre 0,0; `rx`/`ry` half sizes; the top
/// is narrower). Also the jagged seam where it breaks, and both halves.
struct Ovo {
    rx: f32,
    ry: f32,
}

impl Ovo {
    const LADOS: usize = 44;

    fn ponto(&self, th: f32) -> Vec2 {
        let cy = th.cos();
        vec2(th.sin() * self.rx * (1.0 - 0.13 * cy), -cy * self.ry)
    }

    fn contorno(&self) -> Vec<Vec2> {
        (0..Self::LADOS)
            .map(|i| self.ponto(i as f32 / Self::LADOS as f32 * std::f32::consts::TAU))
            .collect()
    }

    fn costura_y(&self) -> f32 {
        self.ry * 0.05
    }

    fn th_costura(&self) -> f32 {
        (-self.costura_y() / self.ry).acos()
    }

    /// Left to right, ends on the outline.
    fn costura(&self) -> Vec<Vec2> {
        let ts = self.th_costura();
        let xs = self.ponto(ts).x;
        let y = self.costura_y();
        let amp = self.ry * 0.09;
        (0..=8)
            .map(|i| {
                let dy = if i == 0 || i == 8 { 0.0 } else if i % 2 == 1 { amp } else { -amp };
                vec2(-xs + 2.0 * xs * i as f32 / 8.0, y + dy)
            })
            .collect()
    }

    fn arco(&self, de: f32, ate: f32) -> Vec<Vec2> {
        let n = 24;
        (0..=n).map(|i| self.ponto(de + (ate - de) * i as f32 / n as f32)).collect()
    }

    /// (outline arc, polygon) of the top half.
    fn de_cima(&self) -> (Vec<Vec2>, Vec<Vec2>) {
        let ts = self.th_costura();
        let arco = self.arco(-ts, ts);
        let mut pol = arco.clone();
        pol.extend(self.costura().into_iter().rev().skip(1).take(7));
        (arco, pol)
    }

    fn de_baixo(&self) -> (Vec<Vec2>, Vec<Vec2>) {
        let ts = self.th_costura();
        let arco = self.arco(ts, std::f32::consts::TAU - ts);
        let mut pol = arco.clone();
        pol.extend(self.costura().into_iter().skip(1).take(7));
        (arco, pol)
    }
}

const CASCA: Color = Color::new(0.97, 0.94, 0.87, 1.0);
const CASCA_SOMBRA: Color = Color::new(0.80, 0.75, 0.74, 1.0);
const CASCA_BORDA: Color = Color::new(0.16, 0.11, 0.20, 1.0);
const PINTA: Color = Color::new(0.42, 0.74, 0.80, 1.0);
/// Spots: (x, y) as fractions of rx/ry, radius as a fraction of rx.
const PINTAS: [(f32, f32, f32); 5] =
    [(-0.38, -0.28, 0.20), (0.42, 0.02, 0.15), (-0.12, 0.50, 0.13), (0.22, -0.60, 0.10), (0.50, 0.55, 0.09)];

/// The whole egg, still closed. `xf` maps egg coordinates to the screen.
#[allow(clippy::too_many_arguments)]
fn desenha_ovo(ovo: &Ovo, xf: &dyn Fn(Vec2) -> Vec2, k: f32, racha: f32, luz: Color, branco: f32) {
    let contorno = ovo.contorno();
    let borda: Vec<Vec2> = contorno.iter().map(|p| xf(*p * 1.06)).collect();
    leque(&borda, CASCA_BORDA);
    let corpo: Vec<Vec2> = contorno.iter().map(|p| xf(*p)).collect();
    leque(&corpo, CASCA_SOMBRA);
    // Lit part: a smaller egg nudged up-left, the rim left over is the shade.
    let des = vec2(-ovo.rx * 0.07, -ovo.ry * 0.06);
    let lit: Vec<Vec2> = contorno.iter().map(|p| xf(*p * 0.88 + des)).collect();
    leque(&lit, CASCA);
    for (x, y, r) in PINTAS {
        let c = xf(vec2(x * ovo.rx, y * ovo.ry));
        draw_circle(c.x, c.y, r * ovo.rx, PINTA);
        draw_circle(c.x - r * ovo.rx * 0.25, c.y - r * ovo.rx * 0.25, r * ovo.rx * 0.45, mistura(PINTA, WHITE, 0.35));
    }
    let h = xf(vec2(-0.40 * ovo.rx, -0.48 * ovo.ry));
    draw_circle(h.x, h.y, 0.15 * ovo.rx, Color::new(1.0, 1.0, 1.0, 0.75));
    let h2 = xf(vec2(-0.22 * ovo.rx, -0.66 * ovo.ry));
    draw_circle(h2.x, h2.y, 0.06 * ovo.rx, Color::new(1.0, 1.0, 1.0, 0.75));
    // The crack grows from the middle out, light leaking through it.
    if racha > 0.0 {
        let alcance = racha / 3.0;
        let costura = ovo.costura();
        let xs = costura[8].x.max(1.0);
        for w in costura.windows(2) {
            let meio = (w[0].x + w[1].x) * 0.5;
            if meio.abs() / xs > alcance + 0.01 {
                continue;
            }
            let (a, b) = (xf(w[0]), xf(w[1]));
            draw_line(a.x, a.y, b.x, b.y, 8.0 * k, estilo::alfa(luz, 0.35 * (racha / 3.0).min(1.0)));
            draw_line(a.x, a.y, b.x, b.y, 3.2 * k, CASCA_BORDA);
            draw_line(a.x, a.y, b.x, b.y, 1.6 * k, mistura(luz, WHITE, 0.5));
        }
        // Little branches off the seam.
        let ramos = [(3usize, -1.0, 2.0), (5, 1.0, 2.0), (2, 1.0, 3.0), (6, -1.0, 3.0)];
        for (i, lado, nivel) in ramos {
            if racha < nivel - 0.5 {
                continue;
            }
            let p0 = costura[i];
            let p1 = p0 + vec2(ovo.rx * 0.10 * lado, -ovo.ry * 0.16 * lado.signum() * if i % 2 == 0 { 1.0 } else { -1.0 });
            let p2 = p1 + vec2(ovo.rx * 0.10, -ovo.ry * 0.08 * if i % 2 == 0 { 1.0 } else { -1.0 });
            for (a, b) in [(p0, p1), (p1, p2)] {
                let (a, b) = (xf(a), xf(b));
                draw_line(a.x, a.y, b.x, b.y, 2.6 * k, CASCA_BORDA);
                draw_line(a.x, a.y, b.x, b.y, 1.2 * k, mistura(luz, WHITE, 0.5));
            }
        }
    }
    if branco > 0.0 {
        leque(&borda, Color::new(1.0, 1.0, 1.0, branco * 0.95));
    }
}

/// One flying half of the shell.
fn desenha_metade(arco: &[Vec2], pol: &[Vec2], xf: &dyn Fn(Vec2) -> Vec2, k: f32, luz: Color, a: f32) {
    let pts: Vec<Vec2> = pol.iter().map(|p| xf(*p)).collect();
    leque(&pts, estilo::alfa(CASCA, a));
    for w in arco.windows(2) {
        let (p, q) = (xf(w[0]), xf(w[1]));
        draw_line(p.x, p.y, q.x, q.y, 3.0 * k, estilo::alfa(CASCA_BORDA, a));
    }
    let n = pts.len();
    let costura = &pts[arco.len() - 1..];
    for i in 0..costura.len() {
        let (p, q) = (costura[i], if i + 1 < costura.len() { costura[i + 1] } else { pts[0] });
        draw_line(p.x, p.y, q.x, q.y, 3.0 * k, estilo::alfa(mistura(luz, WHITE, 0.4), a));
    }
    let _ = n;
}

/// Slowly turning rays and a soft halo behind the prize.
fn raios(c: Vec2, k: f32, cor: Color, t: f32, forca: f32) {
    let n = 14;
    for i in 0..n {
        let a = i as f32 / n as f32 * std::f32::consts::TAU + t * 0.22;
        let l = (190.0 + 30.0 * (t * 1.3 + i as f32).sin()) * k * forca;
        let d1 = vec2((a - 0.075).cos(), (a - 0.075).sin());
        let d2 = vec2((a + 0.075).cos(), (a + 0.075).sin());
        draw_triangle(c, c + d1 * l, c + d2 * l, estilo::alfa(cor, 0.13 * forca));
    }
    for (r, al) in [(150.0, 0.06), (110.0, 0.08), (75.0, 0.11), (45.0, 0.14)] {
        disco(c, r * k * forca, estilo::alfa(cor, al * forca));
    }
}

/// (No drop shadow: the HUD picks the title font by colour, so a black
/// shadow would come out in another font and look doubled.)
fn texto_com_sombra(x: f32, y: f32, s: &str, tam: u16, cor: Color, _k: f32) {
    estilo::texto_centro_forte(x, y, s, tam, cor);
}

/// A smooth disc (macroquad's `draw_circle` uses 20 sides: big ones show
/// their corners).
fn disco(c: Vec2, r: f32, cor: Color) {
    draw_poly(c.x, c.y, 72, r, 0.0, cor);
}

fn anel(c: Vec2, r: f32, grossura: f32, cor: Color) {
    draw_poly_lines(c.x, c.y, 72, r, 0.0, grossura, cor);
}

/// The tome that pops out: an open book in its grade colour, pages glowing,
/// a rune spinning above it.
fn livro_aberto(c: Vec2, lado: f32, cor: Color, t: f32) {
    let w = lado * 0.95;
    let h = lado * 0.62;
    let capa = mistura(cor, Color::new(0.10, 0.06, 0.14, 1.0), 0.55);
    let pagina = Color::new(0.98, 0.95, 0.86, 1.0);
    let pagina_sombra = Color::new(0.86, 0.80, 0.70, 1.0);
    let base = c + vec2(0.0, h * 0.55);
    // Cover, slightly bigger than the pages.
    for lado_x in [-1.0f32, 1.0] {
        let a = base;
        let b = base + vec2(lado_x * (w + lado * 0.06), -lado * 0.06);
        let d = base + vec2(0.0, -h - lado * 0.04);
        let e = base + vec2(lado_x * (w + lado * 0.06), -h - lado * 0.14);
        draw_triangle(a, b, e, capa);
        draw_triangle(a, e, d, capa);
    }
    // Pages: each side curves up to the spine.
    for lado_x in [-1.0f32, 1.0] {
        let n = 8;
        for i in 0..n {
            let f0 = i as f32 / n as f32;
            let f1 = (i + 1) as f32 / n as f32;
            let curva = |f: f32| -(f * std::f32::consts::PI).sin() * lado * 0.05;
            let x0 = lado_x * w * f0;
            let x1 = lado_x * w * f1;
            let p0 = base + vec2(x0, curva(f0));
            let p1 = base + vec2(x1, curva(f1));
            let q0 = base + vec2(x0, curva(f0) - h);
            let q1 = base + vec2(x1, curva(f1) - h);
            let cor_p = if lado_x < 0.0 { pagina } else { pagina_sombra };
            draw_triangle(p0, p1, q1, cor_p);
            draw_triangle(p0, q1, q0, cor_p);
        }
        // Text lines.
        for j in 0..4 {
            let y = base.y - h * (0.78 - j as f32 * 0.17);
            let x0 = base.x + lado_x * w * 0.15;
            let x1 = base.x + lado_x * w * (0.82 - if j == 3 { 0.25 } else { 0.0 });
            draw_line(x0, y, x1, y, lado * 0.025, estilo::alfa(capa, 0.45));
        }
    }
    draw_line(base.x, base.y, base.x, base.y - h, lado * 0.03, capa);
    // Light rising from the pages and the rune.
    let pulso = 0.5 + 0.5 * (t * 3.0).sin();
    let runa = c + vec2(0.0, -h * 0.85);
    disco(runa, lado * (0.42 + 0.04 * pulso), estilo::alfa(cor, 0.18));
    disco(runa, lado * 0.26, estilo::alfa(cor, 0.25));
    draw_poly_lines(runa.x, runa.y, 6, lado * 0.24, t * 40.0, lado * 0.03, cor);
    draw_poly(runa.x, runa.y, 6, lado * 0.13, -t * 60.0, mistura(cor, WHITE, 0.55));
    draw_triangle(base + vec2(-w * 0.5, -h * 0.6), base + vec2(w * 0.5, -h * 0.6), runa, estilo::alfa(cor, 0.12));
}

#[allow(clippy::too_many_arguments)]
fn cena(
    premios: &[PremioInvocacao],
    nomes: &std::collections::HashMap<u16, String>,
    skills: &[shared::skills::Skill],
    vox: &VoxCache,
    solido: &Material,
    t: f32,
    k: f32,
    seguro: Rect,
) -> Acao {
    let o = vec2(seguro.center().x - BASE_W * k * 0.5, seguro.center().y - BASE_H * k * 0.5);
    let p = |x: f32, y: f32| o + vec2(x, y) * k;
    let infos: Vec<Info> = premios.iter().map(|pr| info(pr, nomes, skills)).collect();
    let melhor = infos.iter().max_by_key(|i| i.raridade).map_or(estilo::OURO, |i| i.cor);
    let clicou = crate::foco::clique();
    let m = Vec2::from(mouse_position());

    crate::hud_layout::escurece(0.90 * (t / 0.3).min(1.0));
    // Racha: how cracked (0..3, grows mid-shake).
    let racha: f32 = SACUDIDAS
        .iter()
        .map(|(s, d)| ((t - s - d * 0.35) / (d * 0.4)).clamp(0.0, 1.0))
        .sum();
    let estourou = t >= T_ESTOURO;
    let b = (t - T_ESTOURO).max(0.0);
    // Spotlight: cold at first, the prize's colour as it cracks.
    let luz = mistura(Color::new(0.62, 0.74, 1.0, 1.0), melhor, if estourou { 1.0 } else { racha / 3.0 });
    let centro = if premios.len() > 1 { p(450.0, 200.0) } else { p(450.0, 150.0) };
    for i in 0..7 {
        let r = (330.0 - i as f32 * 42.0) * k;
        disco(centro, r, estilo::alfa(luz, 0.028 + if estourou { 0.012 } else { 0.0 }));
    }

    let ovo = Ovo { rx: 58.0 * k, ry: 76.0 * k };
    let c_ovo = p(450.0, 190.0);
    let mut acao = Acao::Nada;

    if !estourou {
        // Drop + landing squash.
        let u = (t / T_QUEDA).min(1.0);
        let cai = -(1.0 - u).powi(2) * 300.0 * k;
        let v = ((t - T_QUEDA) / 0.18).clamp(0.0, 1.0);
        let amasso = (v * std::f32::consts::PI).sin() * if t > T_QUEDA { 1.0 } else { 0.0 };
        let (mut sx, mut sy) = (1.0 + 0.10 * amasso, 1.0 - 0.13 * amasso);
        // Shakes: rocking on its base, harder each time.
        let mut rot = 0.0;
        for (i, (s, d)) in SACUDIDAS.iter().enumerate() {
            let x = (t - s) / d;
            if (0.0..1.0).contains(&x) {
                rot += (x * std::f32::consts::TAU * 2.0).sin() * (1.0 - x) * (0.16 + 0.07 * i as f32);
            }
        }
        // Breathing while it glows white, about to burst.
        let branco = ((t - T_BRILHO) / (T_ESTOURO - T_BRILHO)).clamp(0.0, 1.0);
        let pulso = 1.0 + branco * 0.06 * (t * 28.0).sin().abs() + branco * 0.08;
        sx *= pulso;
        sy *= pulso;
        // Ground shadow.
        let sombra = 1.0 - (-cai / (300.0 * k)).min(1.0) * 0.6;
        draw_ellipse(c_ovo.x, c_ovo.y + ovo.ry + 6.0 * k, ovo.rx * 1.05 * sombra, 11.0 * k * sombra, 0.0, Color::new(0.0, 0.0, 0.0, 0.40 * sombra));
        // Evolution rings pulling inwards while it glows.
        if branco > 0.0 {
            for j in 0..3 {
                let f = ((t * 1.8 + j as f32 / 3.0).fract()).max(0.0);
                let r = (220.0 * (1.0 - f) + 70.0) * k;
                anel(c_ovo, r, 3.0 * k, estilo::alfa(mistura(luz, WHITE, 0.5), branco * f * 0.8));
            }
        }
        let pivo = vec2(0.0, ovo.ry);
        let base = c_ovo + vec2(0.0, cai);
        let xf = move |l: Vec2| -> Vec2 {
            let l = vec2(l.x * sx, ovo.ry - (ovo.ry - l.y) * sy);
            base + pivo + gira(l - pivo, rot)
        };
        desenha_ovo(&ovo, &xf, k, racha, luz, branco);
        // A puff of sparks each time it cracks.
        for (i, (s, d)) in SACUDIDAS.iter().enumerate() {
            let q = (t - s - d * 0.5) / 0.5;
            if !(0.0..1.0).contains(&q) {
                continue;
            }
            for j in 0..8u32 {
                let a = sorte(j + 17 * i as u32) * std::f32::consts::TAU;
                let pos = c_ovo + vec2(a.cos() * ovo.rx, a.sin() * ovo.ry * 0.5 + ovo.costura_y()) * (0.9 + sai(q) * 0.7);
                brilhinho(pos, (7.0 * (1.0 - q)) * k, estilo::alfa(mistura(luz, WHITE, 0.5), 1.0 - q));
            }
        }
        estilo::texto_centro(p(450.0, 404.0).x, p(450.0, 404.0).y, "Tap to skip", 13, estilo::alfa(estilo::SUAVE, (t / 0.6).min(1.0)));
        if clicou {
            acao = Acao::Pular(T_ESTOURO);
        }
    } else {
        // Rays behind whatever came out.
        raios(centro, k, melhor, t, sai(b / 0.35));
        // Halves of the shell flying off.
        if b < 0.7 {
            let e = sai(b / 0.7);
            let a = 1.0 - (b / 0.7);
            let (arco_c, pol_c) = ovo.de_cima();
            let (arco_b, pol_b) = ovo.de_baixo();
            let cen_c = pol_c.iter().copied().sum::<Vec2>() / pol_c.len() as f32;
            let cen_b = pol_b.iter().copied().sum::<Vec2>() / pol_b.len() as f32;
            let off_c = vec2(-150.0, -150.0 + 240.0 * b * b) * k * e;
            let off_b = vec2(140.0, 60.0 + 200.0 * b * b) * k * e;
            let xf_c = move |l: Vec2| c_ovo + cen_c + off_c + gira(l - cen_c, -1.1 * e);
            let xf_b = move |l: Vec2| c_ovo + cen_b + off_b + gira(l - cen_b, 0.8 * e);
            desenha_metade(&arco_c, &pol_c, &xf_c, k, luz, a);
            desenha_metade(&arco_b, &pol_b, &xf_b, k, luz, a);
        }
        if premios.len() > 1 {
            acao = lote(premios, &infos, vox, solido, b, k, &p, clicou, m, t);
        } else {
            acao = unico(&premios[0], &infos[0], vox, solido, b, k, &p, clicou, m, t);
        }
        // Burst sparks and shockwave.
        if b < 0.9 {
            let q = b / 0.9;
            for j in 0..30u32 {
                let a = sorte(j) * std::f32::consts::TAU;
                let vel = (160.0 + 260.0 * sorte(j + 99)) * k;
                let pos = c_ovo + vec2(a.cos(), a.sin()) * vel * sai(q) + vec2(0.0, 60.0 * k * q * q);
                let cor = if j % 3 == 0 { WHITE } else { mistura(melhor, WHITE, 0.3) };
                brilhinho(pos, (9.0 + 6.0 * sorte(j + 7)) * (1.0 - q) * k, estilo::alfa(cor, 1.0 - q));
            }
        }
        if b < 0.5 {
            let q = b / 0.5;
            anel(c_ovo, (60.0 + 460.0 * sai(q)) * k, 8.0 * k * (1.0 - q), estilo::alfa(mistura(melhor, WHITE, 0.6), 1.0 - q));
        }
        // The white flash.
        let flash = (1.0 - b / 0.45).max(0.0).powf(1.6);
        if flash > 0.0 {
            draw_rectangle(0.0, 0.0, screen_width(), screen_height(), Color::new(1.0, 1.0, 1.0, flash * 0.92));
        }
    }
    acao
}

#[allow(clippy::too_many_arguments)]
fn unico(
    premio: &PremioInvocacao,
    inf: &Info,
    vox: &VoxCache,
    solido: &Material,
    b: f32,
    k: f32,
    p: &dyn Fn(f32, f32) -> Vec2,
    clicou: bool,
    m: Vec2,
    t: f32,
) -> Acao {
    let s = sai_volta((b - 0.05) / 0.5);
    let pc = p(450.0, 150.0);
    let caixa = |w: f32, h: f32| Rect::new(pc.x - w * 0.5 * k * s, pc.y - h * 0.5 * k * s, w * k * s, h * k * s);
    // A small pedestal glow under the prize.
    draw_ellipse(pc.x, pc.y + 96.0 * k, 120.0 * k * s, 16.0 * k * s, 0.0, estilo::alfa(inf.cor, 0.22));
    if s > 0.02 {
        match premio {
            PremioInvocacao::Montaria { item_id } => {
                crate::render3d::vitrine_montaria(vox, *item_id, caixa(330.0, 220.0), t * 0.45, solido);
            }
            PremioInvocacao::Pet { item_id } => {
                crate::render3d::vitrine_pet(vox, *item_id, caixa(330.0, 220.0), t * 0.45, solido);
            }
            PremioInvocacao::Chave { item_id, cor } => {
                // Bare drawing (no slot frame) floating over the rays.
                if !crate::icones::desenha(*item_id, caixa(170.0, 170.0), 1.0) {
                    crate::icones::icone(*item_id, caixa(150.0, 150.0), Some(*cor), None);
                }
            }
            PremioInvocacao::Tomo { grau, .. } => {
                livro_aberto(pc + vec2(0.0, 20.0 * k * s), 120.0 * k * s, cor_do_tomo(*grau), t);
            }
        }
    }
    // Twinkles around it.
    for j in 0..9u32 {
        let a = sorte(j + 300) * std::f32::consts::TAU + t * 0.3;
        let r = (120.0 + 50.0 * sorte(j + 400)) * k;
        let f = ((t * 1.4 + sorte(j + 500) * 6.0).sin() * 0.5 + 0.5) * sai(b / 0.6);
        brilhinho(pc + vec2(a.cos() * r * 1.3, a.sin() * r * 0.8), 8.0 * k * f, estilo::alfa(mistura(inf.cor, WHITE, 0.5), f));
    }
    let a = sai((b - 0.25) / 0.35);
    let cab = match premio {
        PremioInvocacao::Montaria { .. } => "NEW MOUNT!",
        PremioInvocacao::Pet { .. } => "NEW PET!",
        PremioInvocacao::Chave { .. } => "NEW KEY!",
        PremioInvocacao::Tomo { .. } => "NEW TOME!",
    };
    let x = p(450.0, 0.0).x;
    texto_com_sombra(x, p(0.0, 36.0).y, cab, 22, estilo::alfa(estilo::OURO, a), k);
    let (titulo, linha, extra) = match premio {
        PremioInvocacao::Montaria { item_id } => (
            inf.titulo.clone(),
            inf.sub.clone(),
            shared::montarias::de_item(*item_id).map(|(_, g)| {
                format!("Equip in the mount slot: {:.0}% mounted speed", shared::montarias::velocidade(g) * 100.0)
            }),
        ),
        PremioInvocacao::Pet { item_id } => (
            inf.titulo.clone(),
            inf.sub.clone(),
            shared::pets::de_item(*item_id).map(|(_, g)| {
                format!(
                    "Equip in the pet slot: picks loot up at {:.0} tiles and gives {} points",
                    shared::pets::raio_de_busca(g),
                    shared::pets::pontos(g, 1)
                )
            }),
        ),
        PremioInvocacao::Chave { .. } => (inf.titulo.clone(), inf.sub.clone(), None),
        PremioInvocacao::Tomo { grau, quantidade, .. } => {
            let nome = inf.titulo.clone();
            (format!("Tome {}", grau.nome()), format!("{nome} · you own x{quantidade}"), None)
        }
    };
    texto_com_sombra(x, p(0.0, 290.0).y, &titulo, 27, estilo::alfa(mistura(inf.cor, WHITE, 0.15), a), k);
    estilo::texto_centro_forte(x, p(0.0, 316.0).y, &linha, 15, estilo::alfa(inf.cor, a));
    if let Some(e) = extra {
        estilo::texto_centro(x, p(0.0, 340.0).y, &e, 14, estilo::alfa(estilo::TEXTO, a));
    }
    if b >= PRONTO_UNICO {
        botao_continuar(p, k, clicou, m)
    } else {
        Acao::Nada
    }
}

fn botao_continuar(p: &dyn Fn(f32, f32) -> Vec2, k: f32, clicou: bool, m: Vec2) -> Acao {
    let o = p(450.0 - 120.0, 360.0);
    let bt = Rect::new(o.x, o.y, 240.0 * k, 46.0 * k);
    estilo::botao(bt, "Continue", estilo::estado_de(bt, false, false), true);
    if clicou && bt.toca(m) {
        crate::sons::tocar(crate::sons::Som::Clique);
        Acao::Fechar
    } else {
        Acao::Nada
    }
}

#[allow(clippy::too_many_arguments)]
fn lote(
    premios: &[PremioInvocacao],
    infos: &[Info],
    vox: &VoxCache,
    solido: &Material,
    b: f32,
    k: f32,
    p: &dyn Fn(f32, f32) -> Vec2,
    clicou: bool,
    m: Vec2,
    t: f32,
) -> Acao {
    let n = premios.len();
    let cols = 4usize;
    let (cw, ch, gap) = (200.0, 76.0, 8.0);
    let y0 = 62.0;
    let x = p(450.0, 0.0).x;
    let a = sai((b - 0.2) / 0.3);
    let titulo = if n == 11 { "11 PRIZES · 10+1 BONUS" } else { "SUMMONING COMPLETE" };
    texto_com_sombra(x, p(0.0, 38.0).y, titulo, 22, estilo::alfa(estilo::OURO, a), k);
    for (i, (premio, inf)) in premios.iter().zip(infos).enumerate() {
        let linha = i / cols;
        let na_linha = (n - linha * cols).min(cols);
        let largura = na_linha as f32 * cw + (na_linha as f32 - 1.0) * gap;
        let cx = 450.0 - largura * 0.5 + (i % cols) as f32 * (cw + gap) + cw * 0.5;
        let cy = y0 + linha as f32 * (ch + gap) + ch * 0.5;
        let s = sai_volta((b - LOTE_INICIO - i as f32 * LOTE_PASSO) / 0.28);
        if s <= 0.01 {
            continue;
        }
        let c = p(cx, cy);
        let card = Rect::new(c.x - cw * 0.5 * k * s, c.y - ch * 0.5 * k * s, cw * k * s, ch * k * s);
        estilo::cartao(card, false, true);
        if inf.raridade >= 4 {
            // Rare ones breathe: a second, pulsing outline.
            let pulso = 0.5 + 0.5 * (t * 3.0 + i as f32).sin();
            let d = (3.0 + 2.0 * pulso) * k;
            draw_rectangle_lines(card.x - d, card.y - d, card.w + 2.0 * d, card.h + 2.0 * d, 2.0 * k, estilo::alfa(inf.cor, 0.35 + 0.4 * pulso));
        }
        draw_rectangle(card.x, card.y, 5.0 * k * s, card.h, inf.cor);
        draw_rectangle_lines(card.x, card.y, card.w, card.h, 2.0 * k, estilo::alfa(inf.cor, 0.85));
        if s < 0.85 {
            continue;
        }
        let ic = Rect::new(card.x + 10.0 * k, card.y + (card.h - 54.0 * k) * 0.5, 54.0 * k, 54.0 * k);
        match premio {
            PremioInvocacao::Pet { item_id } | PremioInvocacao::Montaria { item_id } => {
                crate::icones::icone_com_3d(*item_id, ic, None, None, Some((vox, solido)));
            }
            PremioInvocacao::Chave { item_id, cor } => {
                crate::icones::icone(*item_id, ic, Some(*cor), None);
            }
            PremioInvocacao::Tomo { .. } => {
                let cc = ic.center();
                draw_circle(cc.x, cc.y, 22.0 * k, estilo::alfa(inf.cor, 0.18));
                draw_poly(cc.x, cc.y, 6, 15.0 * k, t * 30.0, inf.cor);
                draw_poly(cc.x, cc.y, 6, 7.0 * k, -t * 30.0, mistura(inf.cor, WHITE, 0.6));
            }
        }
        let tx = card.x + 72.0 * k;
        estilo::texto_ajustado(&inf.titulo, tx, card.y + 32.0 * k, card.w - 80.0 * k, 14, estilo::TEXTO);
        estilo::texto(tx, card.y + 56.0 * k, &inf.sub, 12, inf.cor);
    }
    let fim = LOTE_INICIO + n as f32 * LOTE_PASSO + 0.28;
    if b >= fim {
        botao_continuar(p, k, clicou, m)
    } else {
        if clicou {
            return Acao::Pular(T_ESTOURO + fim);
        }
        Acao::Nada
    }
}

#[cfg(test)]
mod testes_ovo {
    use super::*;

    #[test]
    fn metades_fecham_o_ovo_e_a_costura_cai_no_contorno() {
        let ovo = Ovo { rx: 58.0, ry: 76.0 };
        let c = ovo.costura();
        let ts = ovo.th_costura();
        assert!((c[0] - ovo.ponto(-ts)).length() < 0.5, "left end of the seam on the outline");
        assert!((c[8] - ovo.ponto(ts)).length() < 0.5, "right end of the seam on the outline");
        let (_, cima) = ovo.de_cima();
        let (_, baixo) = ovo.de_baixo();
        assert!(cima.iter().all(|p| p.y <= ovo.costura_y() + ovo.ry * 0.1));
        assert!(baixo.iter().all(|p| p.y >= ovo.costura_y() - ovo.ry * 0.1));
    }

    #[test]
    fn a_sequencia_esta_em_ordem() {
        assert!(T_QUEDA < SACUDIDAS[0].0);
        for w in SACUDIDAS.windows(2) {
            assert!(w[0].0 + w[0].1 <= w[1].0, "shakes overlap");
        }
        let (s, d) = SACUDIDAS[2];
        assert!(s + d <= T_ESTOURO);
        assert!(T_BRILHO < T_ESTOURO);
        assert!((sai_volta(1.0) - 1.0).abs() < 1e-4 && sai_volta(0.0).abs() < 1e-4);
    }
}

/// `MMO_PREVIA_INVOCACAO=1`: captures the opening and both prize types,
/// with no server.
#[cfg(debug_assertions)]
pub async fn previa(vox: &VoxCache) {
    let saida = std::env::var("MMO_PREVIA_SAIDA")
        .unwrap_or_else(|_| "/tmp/tempest-invocacao-preview".into());
    std::fs::create_dir_all(&saida).unwrap();
    next_frame().await;
    let rt = macroquad::texture::render_target_ex(
        screen_width() as u32,
        screen_height() as u32,
        macroquad::texture::RenderTargetParams {
            depth: true,
            sample_count: 1,
        },
    );
    crate::render3d::define_alvo(Some(rt.clone()));
    let solido = crate::render3d::material_solido();
    let nomes =
        std::collections::HashMap::from([(shared::item_id::HORN + 2, "Blue Horn".to_string())]);
    let mut lote: Vec<_> = (0..11)
        .map(|i| PremioInvocacao::Tomo {
            skill_id: i % 12 + 1,
            grau: shared::skills::GrauTomo::TODOS[(i as usize / 4).min(2)],
            quantidade: i as u16 + 1,
        })
        .collect();
    lote[2] = PremioInvocacao::Pet { item_id: shared::item_id::PET_BASE + 1 };
    lote[5] = PremioInvocacao::Chave { item_id: shared::item_id::HORN + 2, cor: 3 };
    lote[9] = PremioInvocacao::Montaria {
        item_id: shared::item_id::montaria_no_grau(shared::item_id::MONTARIA_BASE, 4),
    };
    let montaria = || {
        vec![PremioInvocacao::Montaria {
            item_id: shared::item_id::montaria_no_grau(shared::item_id::MONTARIA_BASE, 3),
        }]
    };
    let cenas = [
        ("01-caindo.png", montaria(), 0.30),
        ("02-rachando.png", montaria(), 1.25),
        ("03-rachado.png", montaria(), 1.88),
        ("04-brilho.png", montaria(), 2.12),
        ("05-estouro.png", montaria(), 2.36),
        ("06-saindo.png", montaria(), 2.55),
        ("07-montaria.png", montaria(), 3.4),
        (
            "08-pet.png",
            vec![PremioInvocacao::Pet { item_id: shared::item_id::PET_BASE + 3 }],
            3.4,
        ),
        (
            "09-chave.png",
            vec![PremioInvocacao::Chave { item_id: shared::item_id::HORN + 2, cor: 3 }],
            3.4,
        ),
        (
            "10-tomo.png",
            vec![PremioInvocacao::Tomo {
                skill_id: 12,
                grau: shared::skills::GrauTomo::Lendario,
                quantidade: 2,
            }],
            3.4,
        ),
        ("11-lote-abrindo.png", lote.clone(), 3.0),
        ("12-lote-10-mais-1.png", lote, 5.0),
    ];
    for (arquivo, premios, tempo) in cenas {
        let agora = get_time();
        let mut ui = InvocacaoUi::default();
        ui.abrir_varias(premios, agora - tempo);
        for _ in 0..2 {
            crate::render3d::camera_padrao();
            clear_background(Color::new(0.08, 0.12, 0.16, 1.0));
            ui.desenha(&nomes, &shared::skills::playtest(), vox, &solido, agora);
            unsafe { get_internal_gl().flush() };
            rt.texture
                .get_texture_data()
                .export_png(&format!("{saida}/{arquivo}"));
            next_frame().await;
        }
    }
}

/// The pet's grade color: the same color table as the items.
fn cor_do_grau(grau: u8) -> Color {
    let h = shared::items::tier_color_hex(grau).trim_start_matches('#');
    let v = u32::from_str_radix(h, 16).unwrap_or(0xbf_bf_bf);
    Color::from_rgba((v >> 16) as u8, (v >> 8) as u8, v as u8, 255)
}
