//! REBIRTH in 3D (`shared::renascimento`, docs/RESET.md): the always-on body
//! aura by rebirth tier, and the celebration of the moment of a rebirth.
//!
//! **The aura** (`personagem` + `desenha`) is layered and restrained, so the
//! body stays readable in a fight:
//! - a soft SIGIL on the ground (two thin rings and the tier's glyph between
//!   them, turning slowly);
//! - a short light CURTAIN rising from the outer ring, fading by the knees;
//! - a few MOTES in the tier's own shape.
//!
//! Each tier has an identity (`tier_da_aura`): green 1-2 = leaves and wisps
//! over a petal ring; blue 3-4 = floating ice shards over a snowflake; purple
//! 5-9 = orbiting runes over a hexagram; gold 10-14 = a halo over the head
//! and a sunburst behind the body; from 15 the BLACK HOLE = a dark vortex disc
//! at the feet (normal blending, so it really darkens), a violet-white
//! accretion rim with a hot spot running round it, a lensed photon arc behind
//! the legs and dust spiralling in. From 20 it grows brighter and denser every
//! 5 (`intensidade_da_aura`, 1 → 3), moderately: +45% light, +60% motes, +10%
//! radius and a dashed outer power ring, never anything taller.
//!
//! **Budget**: queued per player and drawn in one batch (two draw calls for
//! everyone), culled past `ALCANCE` like the gear auras (`auras.rs`), at most
//! `ORCAMENTO` bodies a frame (nearest first), and only the nearest `DETALHE`
//! of those within `PERTO` get glyphs and motes; the rest draw the rings and
//! curtain only (~150 vertices). Rings are shared-vertex strips, not quads.
//!
//! **The celebration** (`Renasceu`): motes gather at the feet, a flash, a
//! column of light, the tier's sigil stamped big on the ground and shrinking
//! into the aura, shock rings, and the tier's motes rising (for the black hole
//! they fall in). Scaled by `grandeza`, with a big shock wave on a milestone
//! (`e_marco`) and a banner naming what it unlocked.

use macroquad::material::{gl_use_default_material, gl_use_material, load_material, Material, MaterialParams};
use macroquad::miniquad::{BlendFactor, BlendState, BlendValue, Comparison, Equation, PipelineParams};
use macroquad::prelude::*;
use crate::relogio::get_time;
use std::cell::RefCell;
use std::f32::consts::{PI, TAU};

use shared::renascimento as rn;

/// Beyond this distance from the camera's target nobody's aura is drawn.
const ALCANCE: f32 = 38.0;
/// Within this, the full aura (glyphs, motes).
const PERTO: f32 = 18.0;
/// The `altura` of a plain body (`render3d`: neck pivot + 0.35, about the
/// top of the head); sizes are relative to it.
const ALTURA: f32 = 1.7;
/// Bodies with an aura drawn per frame.
pub const ORCAMENTO: usize = 24;
/// Of those, how many (nearest first) get the full detail.
pub const DETALHE: usize = 8;

#[derive(Clone, Copy)]
struct Corpo {
    pe: Vec3,
    altura: f32,
    r: u8,
    fase: f32,
    distancia: f32,
}

thread_local! {
    static FILA: RefCell<Vec<Corpo>> = const { RefCell::new(Vec::new()) };
    static MATERIAIS: RefCell<Option<(Material, Material)>> = const { RefCell::new(None) };
}

/// The colour of an aura tier (1..=5): the glow, and for the black hole its
/// violet rim. RGB 0..1.
pub fn cor_do_tier(tier: u8) -> Color {
    match tier {
        1 => Color::new(0.29, 0.92, 0.48, 1.0),
        2 => Color::new(0.26, 0.62, 1.0, 1.0),
        3 => Color::new(0.73, 0.37, 1.0, 1.0),
        4 => Color::new(1.0, 0.80, 0.28, 1.0),
        _ => Color::new(0.66, 0.30, 1.0, 1.0),
    }
}

/// The highlight of a tier: what the cores of lines and motes burn at.
fn claro_do_tier(tier: u8) -> Color {
    match tier {
        1 => Color::new(0.82, 1.0, 0.66, 1.0),
        2 => Color::new(0.84, 0.95, 1.0, 1.0),
        3 => Color::new(1.0, 0.78, 1.0, 1.0),
        4 => Color::new(1.0, 0.97, 0.80, 1.0),
        _ => Color::new(0.96, 0.88, 1.0, 1.0),
    }
}

/// The colour of `r` rebirths' aura (white-gold before the first).
pub fn cor_de(r: u32) -> Color {
    match rn::tier_da_aura(r) {
        0 => Color::new(1.0, 0.90, 0.62, 1.0),
        t => cor_do_tier(t),
    }
}

/// Queues the aura of a player with `r` rebirths standing at `pe`, its head
/// at `altura` above. `distancia` = from the camera target (0 for oneself).
pub fn personagem(r: u8, pe: Vec3, altura: f32, distancia: f32, id: u32) {
    if r == 0 || distancia > ALCANCE {
        return;
    }
    FILA.with(|f| {
        f.borrow_mut().push(Corpo {
            pe,
            altura: altura.clamp(0.8, 4.0),
            r,
            fase: id as f32 * 0.618,
            distancia,
        })
    });
}

fn materiais<R>(f: impl FnOnce(&Material, &Material) -> R) -> R {
    MATERIAIS.with(|m| {
        let mut m = m.borrow_mut();
        let (brilho, escuro) = m.get_or_insert_with(|| (material(true), material(false)));
        f(brilho, escuro)
    })
}

/// Draws (and empties) the queue. `olho` = the camera position, for the
/// billboards to face it.
pub fn desenha(olho: Vec3) {
    FILA.with(|fila| {
        let mut fila = fila.borrow_mut();
        if fila.is_empty() {
            return;
        }
        fila.sort_by(|a, b| a.distancia.total_cmp(&b.distancia));
        fila.truncate(ORCAMENTO);
        let t = get_time() as f32;
        materiais(|brilho, escuro| {
            // Black holes first, with normal blending: their dark disc must
            // be under the additive rim.
            // (`Malha::vaga` may flush mid-way, so the material is set first.)
            let detalhe = |i: usize, c: &Corpo| c.distancia < PERTO && i < DETALHE;
            let mut malha = Malha::default();
            if fila.iter().any(|c| rn::tier_da_aura(c.r as u32) == 5) {
                gl_use_material(escuro);
                for (i, c) in fila.iter().enumerate().filter(|(_, c)| rn::tier_da_aura(c.r as u32) == 5) {
                    let k = forca(c.r as u32);
                    vortice_escuro(&mut malha, c.pe, raio_base(c.altura, k), k, t * 0.6 + c.fase, detalhe(i, c));
                }
                malha.descarrega();
            }
            gl_use_material(brilho);
            for (i, c) in fila.iter().enumerate() {
                aura(&mut malha, c, olho, t, detalhe(i, c));
            }
            malha.descarrega();
            gl_use_default_material();
        });
        fila.clear();
    });
}

/// `aditivo` = glow (adds light); otherwise normal alpha blending (darkens).
/// Depth-tested against the bodies, never writing depth (it is see-through).
fn material(aditivo: bool) -> Material {
    load_material(
        ShaderSource::Glsl {
            vertex: r#"#version 100
            attribute vec3 position; attribute vec4 color0;
            uniform mat4 Model; uniform mat4 Projection;
            varying lowp vec4 cor;
            void main() { gl_Position = Projection * Model * vec4(position, 1.0); cor = color0 / 255.0; }"#,
            fragment: r#"#version 100
            varying lowp vec4 cor;
            void main() { if (cor.a <= 0.001) discard; gl_FragColor = cor; }"#,
        },
        MaterialParams {
            pipeline_params: PipelineParams {
                depth_test: Comparison::LessOrEqual,
                depth_write: false,
                color_blend: Some(BlendState::new(
                    Equation::Add,
                    BlendFactor::Value(BlendValue::SourceAlpha),
                    if aditivo {
                        BlendFactor::One
                    } else {
                        BlendFactor::OneMinusValue(BlendValue::SourceAlpha)
                    },
                )),
                ..Default::default()
            },
            ..Default::default()
        },
    )
    .expect("rebirth aura material")
}

// ───────────────────────────── geometry ─────────────────────────────

#[derive(Default)]
struct Malha {
    v: Vec<Vertex>,
    i: Vec<u16>,
}

impl Malha {
    fn vert(&mut self, p: Vec3, c: Color) -> u16 {
        let n = self.v.len() as u16;
        self.v.push(Vertex { position: p, uv: Vec2::ZERO, color: c.into(), normal: Vec4::ZERO });
        n
    }

    /// Room for `n` more indices in this draw call (macroquad clamps a call
    /// past 5000), flushing first if not.
    fn vaga(&mut self, n: usize) {
        if self.i.len() + n > 4800 {
            self.descarrega();
        }
    }

    fn quad(&mut self, p: [Vec3; 4], c: [Color; 4]) {
        self.vaga(6);
        let b = self.v.len() as u16;
        for k in 0..4 {
            self.vert(p[k], c[k]);
        }
        self.i.extend_from_slice(&[b, b + 1, b + 2, b, b + 2, b + 3]);
    }

    fn descarrega(&mut self) {
        if self.v.is_empty() {
            return;
        }
        draw_mesh(&Mesh { vertices: std::mem::take(&mut self.v), indices: std::mem::take(&mut self.i), texture: None });
    }

    /// A strip of `colunas + 1` columns of `perfil.len()` vertices each,
    /// placed by `pos(column, row)`: rings, curtains, arcs, spiral arms.
    fn grade(&mut self, colunas: usize, linhas: usize, mut pos: impl FnMut(usize, usize) -> (Vec3, Color)) {
        self.vaga(colunas * (linhas - 1) * 6);
        let b = self.v.len() as u16;
        for s in 0..=colunas {
            for j in 0..linhas {
                let (p, c) = pos(s, j);
                self.vert(p, c);
            }
        }
        let n = linhas as u16;
        for s in 0..colunas as u16 {
            for j in 0..n - 1 {
                let i0 = b + s * n + j;
                self.i.extend_from_slice(&[i0, i0 + 1, i0 + n + 1, i0, i0 + n + 1, i0 + n]);
            }
        }
    }

    /// A flat ring on the ground: `perfil` = (radius, colour with alpha) from
    /// the inside out; `f(angle)` scales the alpha round it (dashes, a hot
    /// spot), `giro` turns it.
    fn anel(&mut self, centro: Vec3, perfil: &[(f32, Color)], lados: usize, giro: f32, f: impl Fn(f32) -> f32) {
        self.grade(lados, perfil.len(), |s, j| {
            let ang = s as f32 / lados as f32 * TAU;
            let d = vec3((ang + giro).cos(), 0.0, (ang + giro).sin());
            let (r, c) = perfil[j];
            (centro + d * r, com_alfa(c, c.a * f(ang)))
        });
    }

    /// A camera-facing soft dot: bright centre, nothing at the rim.
    fn brilho(&mut self, p: Vec3, raio: f32, cor: Color, (dir, cima): (Vec3, Vec3)) {
        self.vaga(18);
        let b = self.vert(p, cor);
        let borda = com_alfa(cor, 0.0);
        for k in 0..6 {
            let a = k as f32 / 6.0 * TAU;
            self.vert(p + (dir * a.cos() + cima * a.sin()) * raio, borda);
        }
        for k in 0..6u16 {
            self.i.extend_from_slice(&[b, b + 1 + k, b + 1 + (k + 1) % 6]);
        }
    }

    /// A diamond along `eixo` (unit) and `lado` (unit): `centro` colour in the
    /// middle, `ponta` at the four tips.
    fn losango(&mut self, p: Vec3, eixo: Vec3, lado: Vec3, comp: f32, larg: f32, centro: Color, ponta: Color) {
        self.vaga(12);
        let b = self.vert(p, centro);
        self.vert(p + eixo * comp, ponta);
        self.vert(p + lado * larg, ponta);
        self.vert(p - eixo * comp, ponta);
        self.vert(p - lado * larg, ponta);
        for k in 0..4u16 {
            self.i.extend_from_slice(&[b, b + 1 + k, b + 1 + (k + 1) % 4]);
        }
    }

    /// A soft line from `a` to `b`, `larg` wide across `normal` (unit, the
    /// face's normal): bright on the axis, nothing at the edges.
    fn traco(&mut self, a: Vec3, b: Vec3, larg: f32, cor: Color, normal: Vec3) {
        let n = (b - a).cross(normal).normalize_or_zero() * larg;
        let borda = com_alfa(cor, 0.0);
        self.quad([a - n, b - n, b, a], [borda, borda, cor, cor]);
        self.quad([a, b, b + n, a + n], [cor, cor, borda, borda]);
    }
}

fn com_alfa(c: Color, a: f32) -> Color {
    Color::new(c.r, c.g, c.b, a.clamp(0.0, 1.0))
}

/// Camera-facing basis at `p`: (right, up).
fn base(p: Vec3, olho: Vec3) -> (Vec3, Vec3) {
    let d = (olho - p).normalize_or_zero();
    let dir = d.cross(Vec3::Y).normalize_or_zero();
    let dir = if dir.length_squared() < 0.1 { Vec3::X } else { dir };
    (dir, dir.cross(d).normalize_or_zero())
}

/// The sideways unit vector of a billboard at `p` facing `olho` (flat).
fn lado(p: Vec3, olho: Vec3) -> Vec3 {
    let d = vec3(olho.x - p.x, 0.0, olho.z - p.z).normalize_or_zero();
    let l = d.cross(Vec3::Y).normalize_or_zero();
    if l.length_squared() < 0.1 {
        Vec3::X
    } else {
        l
    }
}

/// 0 at 19 rebirths and below, 1 at the cap (`intensidade_da_aura` = 3).
fn forca(r: u32) -> f32 {
    ((rn::intensidade_da_aura(r) - 1.0) * 0.5).clamp(0.0, 1.0)
}

/// The sigil's outer radius for a body `altura` tall.
fn raio_base(altura: f32, k: f32) -> f32 {
    0.70 * (altura / ALTURA).clamp(0.6, 1.6) * (1.0 + 0.10 * k)
}

// ─────────────────────────── the aura layers ───────────────────────────

/// The ground sigil of tiers 1-4: a faint pool of light, a thin outer ring
/// with a bright core, an inner ring and the tier's glyph between them.
/// `luz` scales every alpha; `detalhe` draws the glyph. Shared with the
/// celebration's stamp.
fn sigilo(m: &mut Malha, pe: Vec3, raio: f32, tier: u8, luz: f32, giro: f32, k: f32, detalhe: bool, lados: usize) {
    let (cor, claro) = (cor_do_tier(tier), claro_do_tier(tier));
    let c = pe + Vec3::Y * 0.035;
    let ca = |a: f32| com_alfa(cor, a * luz);
    let cl = |a: f32| com_alfa(claro, a * luz);
    // The pool: barely there, it lifts the ground under the body.
    m.anel(c, &[(0.0, ca(0.10)), (raio * 0.75, ca(0.07)), (raio * 1.15, ca(0.0))], lados / 2, giro, |_| 1.0);
    // Outer ring: a sharp bright core inside a soft bloom.
    let w = 0.014 * raio / 0.7;
    m.anel(
        c,
        &[(raio - 0.10, ca(0.0)), (raio - w, ca(0.30)), (raio, cl(0.85)), (raio + w, ca(0.30)), (raio + 0.12, ca(0.0))],
        lados,
        giro,
        |_| 1.0,
    );
    let ri = raio * 0.80;
    m.anel(c, &[(ri - 0.025, ca(0.0)), (ri, ca(0.55)), (ri + 0.025, ca(0.0))], lados, -giro, |_| 1.0);
    // Power ring past 20 rebirths: dashed, turning the other way.
    if k > 0.05 {
        let rp = raio * 1.22;
        m.anel(
            c,
            &[(rp - 0.03, ca(0.0)), (rp, cl(0.55 * k)), (rp + 0.03, ca(0.0))],
            lados,
            -giro * 1.6,
            |a| (0.5 + 0.5 * (a * 12.0).sin()).powi(3),
        );
    }
    if !detalhe {
        return;
    }
    let meio = (raio + ri) * 0.5;
    let banda = raio - ri;
    match tier {
        // Leaves: eight petals laid along the band, tips bright.
        1 => {
            for i in 0..8 {
                let a = i as f32 / 8.0 * TAU + giro;
                let d = vec3(a.cos(), 0.0, a.sin());
                let tg = vec3(-a.sin(), 0.0, a.cos());
                m.losango(c + d * meio, tg, d, banda * 1.3, banda * 0.32, cl(0.60), ca(0.08));
            }
            // A vine of dots inside.
            for i in 0..16 {
                let a = (i as f32 + 0.5) / 16.0 * TAU - giro * 0.7;
                let p = c + vec3(a.cos(), 0.0, a.sin()) * ri * 0.86;
                m.losango(p, Vec3::X, Vec3::Z, 0.022, 0.022, ca(0.45), ca(0.0));
            }
        }
        // Frost: a six-armed snowflake inside the inner ring, ticks in the band.
        2 => {
            for i in 0..6 {
                let a = i as f32 / 6.0 * TAU - giro * 0.6;
                let d = vec3(a.cos(), 0.0, a.sin());
                m.traco(c + d * ri * 0.30, c + d * ri * 0.96, 0.020, cl(0.50), Vec3::Y);
                for (r, l) in [(0.55, 0.16), (0.76, 0.11)] {
                    let o = c + d * ri * r;
                    for s in [-1.0f32, 1.0] {
                        let b = a + s * 0.75;
                        m.traco(o, o + vec3(b.cos(), 0.0, b.sin()) * ri * l, 0.016, cl(0.40), Vec3::Y);
                    }
                }
            }
            for i in 0..12 {
                let a = i as f32 / 12.0 * TAU + giro;
                let d = vec3(a.cos(), 0.0, a.sin());
                let tg = vec3(-a.sin(), 0.0, a.cos());
                m.losango(c + d * meio, d, tg, banda * 0.42, banda * 0.16, cl(0.75), ca(0.05));
            }
        }
        // Arcane: a hexagram turning against a band of runes.
        3 => {
            for tri in 0..2 {
                for i in 0..3 {
                    let a0 = (i as f32 / 3.0 + tri as f32 / 6.0) * TAU - giro * 0.8;
                    let a1 = a0 + TAU / 3.0;
                    let p0 = c + vec3(a0.cos(), 0.0, a0.sin()) * ri * 0.97;
                    let p1 = c + vec3(a1.cos(), 0.0, a1.sin()) * ri * 0.97;
                    m.traco(p0, p1, 0.018, cl(0.45), Vec3::Y);
                }
            }
            for i in 0..12 {
                let a = i as f32 / 12.0 * TAU + giro;
                let d = vec3(a.cos(), 0.0, a.sin());
                let tg = vec3(-a.sin(), 0.0, a.cos());
                let o = c + d * meio;
                for (x0, y0, x1, y1) in RUNAS[i % RUNAS.len()] {
                    let s = banda * 0.36;
                    m.traco(o + (tg * *x0 + d * *y0) * s, o + (tg * *x1 + d * *y1) * s, 0.012, cl(0.65), Vec3::Y);
                }
            }
        }
        // Radiant: sun rays out of the outer ring, long and short.
        _ => {
            for i in 0..16 {
                let a = i as f32 / 16.0 * TAU + giro * 0.5;
                let d = vec3(a.cos(), 0.0, a.sin());
                let tg = vec3(-a.sin(), 0.0, a.cos());
                let l = if i % 2 == 0 { 0.30 } else { 0.16 } * raio;
                m.losango(c + d * (raio + l * 0.5 + 0.02), d, tg, l * 0.5, 0.035, cl(0.55), ca(0.0));
            }
            for i in 0..8 {
                let a = (i as f32 + 0.5) / 8.0 * TAU - giro;
                let d = vec3(a.cos(), 0.0, a.sin());
                let tg = vec3(-a.sin(), 0.0, a.cos());
                m.losango(c + d * meio, d, tg, banda * 0.30, banda * 0.30, cl(0.70), ca(0.0));
            }
        }
    }
}

/// Six little runes, strokes in a -1..1 square.
const RUNAS: [&[(f32, f32, f32, f32)]; 6] = [
    &[(-0.5, -0.9, -0.5, 0.9), (-0.5, 0.2, 0.5, 0.9), (-0.5, -0.3, 0.5, 0.4)],
    &[(0.0, -0.9, 0.0, 0.9), (0.0, 0.3, 0.6, 0.8), (0.0, -0.1, -0.6, 0.4)],
    &[(-0.6, 0.8, 0.6, 0.8), (0.6, 0.8, -0.6, -0.8), (-0.6, -0.8, 0.6, -0.8)],
    &[(0.0, -0.9, 0.0, 0.9), (-0.6, 0.5, 0.6, -0.1), (0.6, 0.5, -0.6, -0.1)],
    &[(-0.5, -0.9, -0.5, 0.9), (-0.5, 0.9, 0.5, 0.3), (0.5, 0.3, -0.5, -0.2)],
    &[(-0.5, 0.2, 0.0, 0.9), (0.0, 0.9, 0.5, 0.2), (-0.5, -0.9, 0.5, 0.2)],
];

/// The backlight: a soft camera-facing column of light just BEHIND the body
/// (pushed away from the camera), so it only shows round the silhouette as a
/// rim and never washes over the character.
fn contraluz(m: &mut Malha, pe: Vec3, altura: f32, olho: Vec3, cor: Color, a: f32, s: f32) {
    let atras = vec3(pe.x - olho.x, 0.0, pe.z - olho.z).normalize_or_zero() * 0.30;
    let l = lado(pe, olho);
    let base = pe + atras;
    // Rows: ground, hips, shoulders, over the head.
    let linhas = [(0.0, 0.55, 0.8), (0.35, 0.62, 1.0), (0.75, 0.55, 0.75), (1.05, 0.30, 0.0)];
    m.grade(linhas.len() - 1, 3, |s_, j| {
        let (y, w, al) = linhas[s_];
        let x = [-w, 0.0, w][j] * s;
        (base + l * x + Vec3::Y * (y * altura), com_alfa(cor, if j == 1 { a * al } else { 0.0 }))
    });
}

/// The light curtain: a short wall of light rising from the ring, fading
/// out by the knees.
fn cortina(m: &mut Malha, pe: Vec3, raio: f32, alt: f32, cor: Color, a: f32, lados: usize, giro: f32) {
    let perfil = [(0.0, a), (0.30, a * 0.50), (1.0, 0.0)];
    m.grade(lados, perfil.len(), |s, j| {
        let ang = s as f32 / lados as f32 * TAU + giro;
        // Gently uneven, as if it breathed.
        let h = alt * (0.85 + 0.15 * (ang * 3.0 + giro * 4.0).sin());
        let (y, al) = perfil[j];
        (pe + vec3(ang.cos() * raio, 0.04 + y * h, ang.sin() * raio), com_alfa(cor, al))
    });
}

/// One mote in the tier's shape at `p`, `tam` across, `a` alpha; `giro`
/// spins/tilts it and picks the rune.
fn mota(m: &mut Malha, tier: u8, p: Vec3, tam: f32, a: f32, giro: f32, olho: Vec3) {
    let (cor, claro) = (cor_do_tier(tier), claro_do_tier(tier));
    let (dir, cima) = base(p, olho);
    let normal = dir.cross(cima);
    match tier {
        // A leaf, tumbling: its width breathes as it turns.
        1 => {
            m.brilho(p, tam * 1.6, com_alfa(cor, a * 0.22), (dir, cima));
            let eixo = dir * giro.cos() + cima * giro.sin();
            let l = eixo.cross(normal);
            let virar = (giro * 1.7).sin().abs().max(0.25);
            m.losango(p, eixo, l, tam, tam * 0.38 * virar, com_alfa(claro, a), com_alfa(cor, a * 0.35));
        }
        // An ice shard: tall, thin, white at the heart.
        2 => {
            m.brilho(p, tam * 1.8, com_alfa(cor, a * 0.18), (dir, cima));
            let incl = 0.25 * giro.sin();
            let eixo = cima * incl.cos() + dir * incl.sin();
            let l = eixo.cross(normal);
            m.losango(p, eixo, l, tam * 1.6, tam * 0.36, com_alfa(claro, a), com_alfa(cor, a * 0.30));
        }
        // A rune.
        3 => {
            m.brilho(p, tam * 1.7, com_alfa(cor, a * 0.30), (dir, cima));
            let runa = RUNAS[(giro.abs() as usize) % RUNAS.len()];
            for (x0, y0, x1, y1) in runa {
                let a0 = p + (dir * *x0 + cima * *y0) * tam;
                let a1 = p + (dir * *x1 + cima * *y1) * tam;
                m.traco(a0, a1, tam * 0.16, com_alfa(claro, a), normal);
            }
        }
        // Glitter: a four-pointed star.
        4 => {
            m.brilho(p, tam * 1.3, com_alfa(cor, a * 0.35), (dir, cima));
            let (e, l) = (dir * giro.cos() + cima * giro.sin(), cima * giro.cos() - dir * giro.sin());
            m.losango(p, e, l, tam * 1.2, tam * 0.16, com_alfa(claro, a), com_alfa(cor, 0.0));
            m.losango(p, l, e, tam * 0.8, tam * 0.16, com_alfa(claro, a), com_alfa(cor, 0.0));
        }
        // Dust falling into the hole.
        _ => {
            m.brilho(p, tam * 1.5, com_alfa(cor, a * 0.35), (dir, cima));
            m.brilho(p, tam * 0.6, com_alfa(claro, a), (dir, cima));
        }
    }
}

fn aura(m: &mut Malha, c: &Corpo, olho: Vec3, t: f32, detalhe: bool) {
    let r = c.r as u32;
    let tier = rn::tier_da_aura(r);
    let k = forca(r);
    let luz = (1.0 + 0.45 * k) * (0.90 + 0.10 * (t * 1.7 + c.fase).sin());
    let raio = raio_base(c.altura, k);
    let lados = if detalhe { 36 } else { 18 };
    let giro = t * 0.25 + c.fase;
    let (cor, claro) = (cor_do_tier(tier), claro_do_tier(tier));
    let s = (c.altura / ALTURA).clamp(0.6, 1.6);
    if tier == 5 {
        buraco_negro(m, c, olho, t, raio, k, luz, detalhe);
        return;
    }
    sigilo(m, c.pe, raio, tier, luz, giro, k, detalhe, lados);
    let alt = [0.0, 0.42, 0.50, 0.55, 0.62][tier as usize] * s * (1.0 + 0.2 * k);
    cortina(m, c.pe, raio * 0.97, alt, cor, 0.26 * luz, lados / 2, giro);
    contraluz(m, c.pe, c.altura, olho, cor, 0.15 * luz, s);
    if tier == 4 {
        // The halo: a flat ring floating over the head.
        let topo = c.pe + Vec3::Y * (c.altura + 0.14 * s);
        let rh = 0.21 * s;
        m.anel(
            topo,
            &[(rh - 0.06, com_alfa(cor, 0.0)), (rh - 0.012, com_alfa(cor, 0.45 * luz)), (rh, com_alfa(claro, 0.95 * luz)), (rh + 0.012, com_alfa(cor, 0.45 * luz)), (rh + 0.07, com_alfa(cor, 0.0))],
            if detalhe { 24 } else { 14 },
            0.0,
            |_| 1.0,
        );
        if detalhe {
            sol_atras(m, c.pe + Vec3::Y * (c.altura * 0.52), olho, s * (1.0 + 0.15 * k), luz, t * 0.12 + c.fase);
        }
    }
    if !detalhe {
        return;
    }
    let n = ([0, 6, 5, 4, 7][tier as usize] as f32 * (1.0 + 0.6 * k)) as usize;
    for i in 0..n {
        let fi = i as f32;
        let semente = fi * 0.618_034 + c.fase;
        match tier {
            // Leaves drift up in a lazy spiral and fade.
            1 => {
                let u = (t * 0.16 + semente).fract();
                let ang = fi * 2.4 + u * 2.6 + t * 0.2 + c.fase;
                let rr = raio * (0.95 - 0.35 * u);
                let p = c.pe + vec3(ang.cos() * rr, 0.1 + u * c.altura * 1.05, ang.sin() * rr);
                mota(m, 1, p, 0.10 * s, 0.75 * (u * PI).sin() * luz.min(1.3), t * 1.3 + fi * 1.9, olho);
            }
            // Ice shards hover round the body, bobbing.
            2 => {
                let ang = fi / n as f32 * TAU + t * 0.35 + c.fase;
                let h = c.altura * (0.22 + 0.45 * (fi * 0.37 + c.fase).fract()) + 0.06 * (t * 1.3 + fi).sin();
                let p = c.pe + vec3(ang.cos() * raio * 0.95, h, ang.sin() * raio * 0.95);
                let pisca = 0.75 + 0.25 * (t * 2.0 + fi * 1.3).sin();
                mota(m, 2, p, 0.085 * s, 0.70 * pisca * luz.min(1.3), fi * 1.7 + t * 0.4, olho);
            }
            // Runes circle the waist, flickering in and out.
            3 => {
                let ang = fi / n as f32 * TAU + t * 0.40 + c.fase;
                let h = c.altura * (0.40 + 0.10 * (t * 0.9 + fi * 2.0).sin());
                let p = c.pe + vec3(ang.cos() * raio * 1.02, h, ang.sin() * raio * 1.02);
                let vida = (t * 0.22 + semente).fract();
                let a = (vida * PI).sin().powf(0.6) * 0.80 * luz.min(1.3);
                mota(m, 3, p, 0.09 * s, a, (fi * 1.0 + (t * 0.22 + semente).floor() * 3.0) % 6.0, olho);
            }
            // Gold glitter rising slowly, twinkling.
            _ => {
                let u = (t * 0.20 + semente).fract();
                let ang = fi * 2.399 + t * 0.3 + c.fase;
                let rr = raio * (0.6 + 0.35 * (fi * 0.71).fract());
                let p = c.pe + vec3(ang.cos() * rr, 0.15 + u * c.altura * 1.0, ang.sin() * rr);
                let pisca = (0.5 + 0.5 * (t * 4.0 + fi * 2.3).sin()).powi(2);
                mota(m, 4, p, 0.08 * s, (u * PI).sin() * (0.35 + 0.65 * pisca) * luz.min(1.3), t * 0.8 + fi, olho);
            }
        }
    }
}

/// The gold sunburst: thin rays fanning out from behind the chest, facing
/// the camera and pushed back so the body stands in front of it.
fn sol_atras(m: &mut Malha, peito: Vec3, olho: Vec3, s: f32, luz: f32, giro: f32) {
    let (cor, claro) = (cor_do_tier(4), claro_do_tier(4));
    let atras = (peito - olho).normalize_or_zero() * 0.35;
    let p = peito + atras;
    let (dir, cima) = base(p, olho);
    m.brilho(p, 0.75 * s, com_alfa(cor, 0.12 * luz), (dir, cima));
    for i in 0..14 {
        let a = i as f32 / 14.0 * TAU + giro;
        let l = if i % 2 == 0 { 1.15 } else { 0.80 } * s;
        let (d, n) = (dir * a.cos() + cima * a.sin(), cima * a.cos() - dir * a.sin());
        let r0 = 0.22 * s;
        m.quad(
            [p + d * r0 - n * 0.045 * s, p + d * r0 + n * 0.045 * s, p + d * l, p + d * l],
            [com_alfa(claro, 0.22 * luz), com_alfa(claro, 0.22 * luz), com_alfa(cor, 0.0), com_alfa(cor, 0.0)],
        );
    }
}

/// The BLACK HOLE's dark part (normal blending): a disc that really darkens
/// the ground, and three dark arms sweeping into it.
fn vortice_escuro(m: &mut Malha, pe: Vec3, raio: f32, k: f32, giro: f32, detalhe: bool) {
    let escuro = Color::new(0.02, 0.0, 0.05, 1.0);
    let c = pe + Vec3::Y * 0.10;
    let rh = raio * 0.62;
    let rd = raio * 1.20;
    m.anel(
        c,
        &[(0.0, com_alfa(escuro, 0.90)), (rh, com_alfa(escuro, 0.88)), (rh * 1.35, com_alfa(escuro, 0.60)), (rd, com_alfa(escuro, 0.0))],
        if detalhe { 32 } else { 16 },
        giro,
        |_| 1.0,
    );
    if detalhe {
        for b in 0..3 {
            braco(m, c, rh, rd * (1.15 + 0.1 * k), giro + b as f32 / 3.0 * TAU, escuro, 0.45, 0.10 * raio / 0.7);
        }
    }
}

/// A spiral arm on the ground from `r1` (out) to `r0` (in), winding with the
/// vortex; `a` alpha at the inner end, nothing outside.
fn braco(m: &mut Malha, c: Vec3, r0: f32, r1: f32, giro: f32, cor: Color, a: f32, larg: f32) {
    const SEG: usize = 12;
    m.grade(SEG, 3, |s, j| {
        let u = s as f32 / SEG as f32;
        let r = r1 + (r0 - r1) * u;
        let ang = giro + u * 2.4;
        let d = vec3(ang.cos(), 0.0, ang.sin());
        let tg = vec3(-ang.sin(), 0.0, ang.cos());
        let w = larg * (1.0 - 0.5 * u);
        let al = a * (u * PI * 0.5).sin() * (1.0 - u * 0.3);
        let off = [-w, 0.0, w][j];
        (c + d * r + tg * off, com_alfa(cor, if j == 1 { al } else { 0.0 }))
    });
}

/// The BLACK HOLE's light: accretion rim with a running hot spot, faint
/// glowing arms, the lensed photon arc behind the legs and dust falling in.
#[allow(clippy::too_many_arguments)]
fn buraco_negro(m: &mut Malha, c: &Corpo, olho: Vec3, t: f32, raio: f32, k: f32, luz: f32, detalhe: bool) {
    let (cor, claro) = (cor_do_tier(5), claro_do_tier(5));
    let centro = c.pe + Vec3::Y * 0.10;
    let rh = raio * 0.62;
    let rd = raio * 1.20;
    let giro = t * 0.6 + c.fase;
    let lados = if detalhe { 40 } else { 20 };
    // The rim: white-hot inner edge, violet glow outward; the hot spot runs
    // round it (Doppler beaming).
    let quente = t * 1.6 + c.fase;
    m.anel(
        centro,
        &[
            (rh - 0.05, com_alfa(cor, 0.0)),
            (rh - 0.008, com_alfa(claro, 0.85 * luz)),
            (rh + 0.035, com_alfa(cor, 0.55 * luz)),
            (rh + 0.26 * raio / 0.7, com_alfa(cor, 0.0)),
        ],
        lados,
        0.0,
        |a| 0.40 + 0.60 * (0.5 + 0.5 * (a - quente).cos()).powi(2),
    );
    // A thin outer orbit, where the dust begins to fall.
    m.anel(
        centro,
        &[(rd - 0.03, com_alfa(cor, 0.0)), (rd, com_alfa(cor, 0.30 * luz)), (rd + 0.03, com_alfa(cor, 0.0))],
        lados,
        giro,
        |a| (0.5 + 0.5 * (a * 5.0).sin()).powf(1.5),
    );
    if k > 0.05 {
        let rp = raio * 1.45;
        m.anel(
            centro,
            &[(rp - 0.03, com_alfa(cor, 0.0)), (rp, com_alfa(claro, 0.40 * k * luz)), (rp + 0.03, com_alfa(cor, 0.0))],
            lados,
            -giro * 0.7,
            |a| (0.5 + 0.5 * (a * 12.0).sin()).powi(3),
        );
    }
    if !detalhe {
        return;
    }
    // Glowing arms between the dark ones.
    for b in 0..3 {
        braco(m, centro, rh * 1.05, rd * 1.1, giro + (b as f32 + 0.5) / 3.0 * TAU, cor, 0.30 * luz, 0.05 * raio / 0.7);
    }
    // The photon arc: the far side of the disc, lensed up over it, standing
    // behind the legs.
    let atras = vec3(c.pe.x - olho.x, 0.0, c.pe.z - olho.z).normalize_or_zero() * rh * 0.5;
    let l = lado(c.pe, olho);
    let ra = rh * 1.12;
    m.grade(20, 3, |s, j| {
        let a = s as f32 / 20.0 * PI;
        let d = l * a.cos() + Vec3::Y * a.sin() * 0.85;
        let w = [ra - 0.06, ra, ra + 0.06][j];
        // Brightest at the top of the arc, gone at the ground.
        let al = if j == 1 { 0.55 * luz * a.sin().powf(0.7) } else { 0.0 };
        (centro + atras + d * w, com_alfa(claro, al))
    });
    // Dust spiralling in, each with a short trail.
    let n = (12.0 * (1.0 + 0.7 * k)) as usize;
    for i in 0..n {
        let fi = i as f32;
        let u = (t * 0.30 + fi * 0.618_034 + c.fase).fract();
        let pos = |u: f32| {
            let r = rd * 1.35 + (rh - rd * 1.35) * u.powf(0.8);
            let ang = fi / n as f32 * TAU + u * 4.2 + t * 0.15;
            centro + vec3(ang.cos() * r, 0.06 + 0.25 * (1.0 - u) * (fi * 0.37).fract(), ang.sin() * r)
        };
        let p = pos(u);
        let a = (u * PI).sin().powf(0.7) * 0.85 * luz.min(1.4);
        m.traco(pos((u - 0.06).max(0.0)), p, 0.010, com_alfa(cor, a * 0.35), (olho - p).normalize_or_zero());
        mota(m, 5, p, 0.032, a, 0.0, olho);
    }
}

// ───────────────────────── the moment of rebirth ─────────────────────────

/// How long the celebration lasts: longer than the level-up's 3 s.
pub const DURACAO: f32 = 4.5;
/// Fraction of the effect the banner stays.
pub const FIM_DA_FAIXA: f32 = 0.8;

/// The celebration in progress. `n` = the rebirth just reached.
#[derive(Debug, Default)]
pub struct Renasceu {
    t: f32,
    ativo: bool,
    n: u32,
}

impl Renasceu {
    pub fn dispara(&mut self, n: u32) {
        self.t = 0.0;
        self.ativo = true;
        self.n = n;
    }

    pub fn passo(&mut self, dt: f32) {
        if self.ativo {
            self.t += dt;
            if self.t >= DURACAO {
                self.ativo = false;
            }
        }
    }

    pub fn ativo(&self) -> bool {
        self.ativo
    }

    pub fn fase(&self) -> Option<f32> {
        self.ativo.then(|| (self.t / DURACAO).clamp(0.0, 1.0))
    }

    /// How grand: 1 + the aura tier, +2 on a milestone. Drives the rings,
    /// sparks and column.
    pub fn grandeza(&self) -> u32 {
        1 + rn::tier_da_aura(self.n) as u32 + if rn::e_marco(self.n) { 2 } else { 0 }
    }

    /// The world burst at the feet: gather, flash, column, the tier's sigil
    /// stamped big and settling into the aura, shock rings and the tier's
    /// motes. The caller has the world camera set.
    pub fn desenha(&self, pe: Vec3, olho: Vec3) {
        let Some(f) = self.fase() else { return };
        let g = self.grandeza() as f32;
        let tier = rn::tier_da_aura(self.n).max(1);
        let (cor, claro) = (cor_do_tier(tier), claro_do_tier(tier));
        let k = forca(self.n);
        let t = self.t;
        // The sigil: stamped at 3x and shrinking to the aura's size.
        let assenta = suave((f - 0.08) / 0.65);
        let raio_s = raio_base(ALTURA, k) * (3.2 - 2.2 * assenta);
        let luz_s = if f < 0.08 { f / 0.08 } else { (1.0 - (f - 0.08) / 0.92).powf(1.3) } * 1.6;
        let mut malha = Malha::default();
        materiais(|brilho, escuro| {
            if tier == 5 {
                vortice_escuro(&mut malha, pe, raio_s, k, t * 1.5, true);
                // Fade the dark with the rest: scale the alpha down.
                let fade = luz_s.min(1.0);
                for v in &mut malha.v {
                    v.color[3] = (v.color[3] as f32 * fade) as u8;
                }
                gl_use_material(escuro);
                malha.descarrega();
            }
            gl_use_material(brilho);
        });
        // Gather: motes converging on the feet before the flash.
        if f < 0.12 {
            let u = f / 0.12;
            for i in 0..(14 + 2 * g as usize) {
                let fi = i as f32;
                let ang = fi * 2.399 + u * 2.0;
                let r = 2.6 * (1.0 - u * u) + 0.1;
                let p = pe + vec3(ang.cos() * r, 0.3 + 1.2 * (fi * 0.37).fract() * (1.0 - u), ang.sin() * r);
                mota(&mut malha, tier, p, 0.08, 0.9 * u, fi, olho);
            }
        }
        // Flash on the ground.
        let flash = brilho_de((f - 0.10) / 0.22);
        if flash > 0.0 {
            malha.anel(pe + Vec3::Y * 0.05, &[(0.0, com_alfa(claro, 0.8 * flash)), (1.0, com_alfa(cor, 0.35 * flash)), (2.4, com_alfa(cor, 0.0))], 32, 0.0, |_| 1.0);
        }
        if tier == 5 {
            buraco_negro(
                &mut malha,
                &Corpo { pe, altura: ALTURA, r: self.n.min(255) as u8, fase: 0.0, distancia: 0.0 },
                olho,
                t * 2.0,
                raio_s,
                k,
                luz_s,
                true,
            );
        } else {
            sigilo(&mut malha, pe, raio_s, tier, luz_s, t * 0.8, k, true, 48);
        }
        // Shock rings: soft, staggered, the last one golden.
        let raio_max = 4.5 + 0.4 * g;
        let aneis = 2 + usize::from(g >= 5.0);
        for i in 0..aneis {
            let u = (f - 0.10 - i as f32 * 0.13) / 0.50;
            if !(0.0..1.0).contains(&u) {
                continue;
            }
            let r = raio_max * suave(u).sqrt();
            let a = (1.0 - u).powi(3) * 0.9;
            let c = if i + 1 == aneis { Color::new(1.0, 0.86, 0.50, 1.0) } else { cor };
            let w = 0.10 + 0.35 * u;
            malha.anel(pe + Vec3::Y * 0.05, &[(r - w, com_alfa(c, 0.0)), (r, com_alfa(claro, a)), (r + w * 0.4, com_alfa(c, 0.0))], 48, 0.0, |_| 1.0);
        }
        // Milestone: one huge, slow shock wave.
        if rn::e_marco(self.n) {
            let u = ((f - 0.10) / 0.6).clamp(0.0, 1.0);
            if f > 0.10 && u < 1.0 {
                let r = raio_max * 2.0 * suave(u);
                let w = 0.9 * (1.0 - u) + 0.1;
                malha.anel(pe + Vec3::Y * 0.05, &[(r - w, com_alfa(cor, 0.0)), (r, com_alfa(cor, 0.7 * (1.0 - u))), (r + w * 0.3, com_alfa(cor, 0.0))], 56, 0.0, |_| 1.0);
            }
        }
        // The column: a white core inside a soft coloured shaft, rising fast
        // and thinning out.
        let a = if f < 0.10 { 0.0 } else { brilho_de((f - 0.10) / 0.55) };
        if a > 0.0 {
            let h = (7.0 + 0.8 * g) * (0.35 + 0.65 * suave((f - 0.10) * 6.0));
            let l = lado(pe, olho);
            let estreita = 1.0 - 0.6 * suave((f - 0.10) / 0.5);
            for (larg, c, al) in [(1.3 + 0.08 * g, cor, 0.45), (0.35 + 0.03 * g, claro, 0.85)] {
                let larg = larg * estreita;
                malha.grade(4, 3, |s, j| {
                    let v = s as f32 / 4.0;
                    let y = h * v;
                    let topo = (1.0 - v).powf(1.4);
                    let x = [-larg, 0.0, larg][j];
                    (pe + l * x * (1.0 - 0.3 * v) + Vec3::Y * y, com_alfa(c, if j == 1 { al * a * topo } else { 0.0 }))
                });
            }
        }
        // The tier's motes: rising in a spiral (falling in for the hole).
        let n = 20 + 6 * g as usize;
        for i in 0..n {
            let fi = i as f32;
            let atraso = 0.10 + (fi * 0.618_034).fract() * 0.30;
            let u = (f - atraso) / (1.0 - atraso);
            if !(0.0..1.0).contains(&u) {
                continue;
            }
            let a = (1.0 - u).powf(1.5) * (u * 12.0).min(1.0);
            let p = if tier == 5 {
                let r = 3.2 * (1.0 - suave(u)) + 0.3;
                let ang = fi * 2.399 + u * 5.0;
                pe + vec3(ang.cos() * r, 0.1 + 1.6 * (1.0 - u) * (fi * 0.37).fract(), ang.sin() * r)
            } else {
                let ang = fi * 2.399 + u * 3.0;
                let r = 0.5 + (1.0 + 0.12 * g) * suave(u) * (0.5 + (fi * 0.71).fract());
                pe + vec3(ang.cos() * r, (4.5 + 0.4 * g) * suave(u) + 0.2, ang.sin() * r)
            };
            mota(&mut malha, tier, p, 0.10, a, fi * 1.3 + t * 1.5, olho);
            if malha.v.len() > 3000 {
                malha.descarrega();
            }
        }
        malha.descarrega();
        gl_use_default_material();
    }
}

/// The banner: "REBIRTH N" over "REBORN!", and on a milestone what it
/// unlocked. Bigger than the level-up's, in the aura's colour.
pub fn desenha_faixa(e: &Renasceu) {
    let Some(f) = e.fase() else { return };
    let vis = 1.0 - (f / FIM_DA_FAIXA).clamp(0.0, 1.0);
    if vis <= 0.0 {
        return;
    }
    let est = crate::hud_estilo::u;
    let seguro = crate::hud_layout::tela_segura();
    let y = seguro.y + seguro.h * 0.28 - (1.0 - vis) * est(26.0);
    let a = vis.powf(0.7);
    let pop = if f < 0.1 { 1.0 + 0.6 * (1.0 - f / 0.1) } else { 1.0 };
    let tam = ((52.0 + 2.0 * e.grandeza() as f32) * pop) as u16;
    let texto = format!("REBIRTH {}", e.n);
    let cx = seguro.center().x;
    let cor = cor_de(e.n);
    let cima = y - est(40.0);
    let sombra = Color::new(0.0, 0.0, 0.0, 0.55 * a);
    crate::hud_estilo::texto_centro_forte(cx + 2.0, cima + 2.0, "REBORN!", 22, sombra);
    crate::hud_estilo::texto_centro_forte(cx, cima, "REBORN!", 22, Color::new(1.0, 0.95, 0.80, a));
    for (dx, dy) in [(-4.0, 0.0), (4.0, 0.0), (0.0, -4.0), (0.0, 4.0)] {
        crate::hud_estilo::texto_centro_forte(cx + dx, y + dy, &texto, tam, com_alfa(cor, 0.25 * a));
    }
    crate::hud_estilo::texto_centro_forte(cx + 2.0, y + 2.0, &texto, tam, sombra);
    crate::hud_estilo::texto_centro_forte(cx, y, &texto, tam, com_alfa(crate::hud_estilo::misturar(cor, WHITE, 0.35), a));
    let baixo = y + est(36.0);
    let linha = format!("+{}% to every attribute", rn::bonus_em_pct(e.n));
    crate::hud_estilo::texto_centro_forte(cx, baixo, &linha, 17, Color::new(1.0, 0.92, 0.74, a));
    if let Some(m) = rn::marco_de(e.n) {
        let m = format!("Milestone: {}", shared::idioma::tr(m));
        crate::hud_estilo::texto_centro_forte(cx + 1.0, baixo + est(28.0) + 1.0, &m, 19, sombra);
        crate::hud_estilo::texto_centro_forte(cx, baixo + est(28.0), &m, 19, com_alfa(cor, a));
    }
}

/// Bright at once, falling fast with a long faint tail (`nivel_vfx`).
fn brilho_de(f: f32) -> f32 {
    if !(0.0..1.0).contains(&f) {
        return 0.0;
    }
    (1.0 - f).powi(2)
}

fn suave(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn a_festa_dura_e_acaba() {
        let mut e = Renasceu::default();
        assert!(e.fase().is_none());
        e.dispara(3);
        e.passo(DURACAO * 0.5);
        assert!(e.ativo());
        e.passo(DURACAO);
        assert!(!e.ativo() && e.fase().is_none());
        assert!(DURACAO > crate::nivel_vfx::DURACAO, "bigger than a level-up");
    }

    /// It keeps growing over many rebirths, and a milestone is grander than
    /// the rebirth after it.
    #[test]
    fn a_festa_cresce_com_o_tier_e_no_marco() {
        let g = |n| {
            let mut e = Renasceu::default();
            e.dispara(n);
            e.grandeza()
        };
        assert!(g(10) > g(5) && g(5) > g(3) && g(3) > g(1));
        assert!(g(10) > g(11), "milestone 10 is grander than 11");
        assert!(g(15) > g(14));
    }

    #[test]
    fn cada_tier_tem_sua_cor() {
        let cores: Vec<Color> = (1..=5).map(cor_do_tier).collect();
        for i in 0..cores.len() {
            for j in i + 1..cores.len() {
                assert_ne!(cores[i], cores[j]);
            }
        }
    }

    /// The aura's light grows with rebirths past 20 but stays moderate.
    #[test]
    fn a_intensidade_e_contida() {
        assert_eq!(forca(19), 0.0);
        assert!(forca(30) > forca(20));
        assert_eq!(forca(55), 1.0);
        assert!(raio_base(2.2, 1.0) < raio_base(2.2, 0.0) * 1.15, "never swallows the body");
        assert!(DETALHE <= ORCAMENTO);
    }

    /// The vertex budget of one fully detailed aura, per tier: cheap enough
    /// for `ORCAMENTO` of them on a phone, and the mesh never overflows u16.
    #[test]
    fn cada_aura_cabe_no_orcamento() {
        let olho = vec3(0.0, 8.0, 8.0);
        for r in [1u8, 3, 5, 10, 15, 55] {
            let mut m = Malha::default();
            let c = Corpo { pe: Vec3::ZERO, altura: 2.2, r, fase: 0.3, distancia: 0.0 };
            aura(&mut m, &c, olho, 1.7, true);
            if rn::tier_da_aura(r as u32) == 5 {
                vortice_escuro(&mut m, c.pe, raio_base(2.2, forca(r as u32)), forca(r as u32), 0.0, true);
            }
            assert!(m.v.len() < 1500, "R{r}: {} vertices", m.v.len());
            assert!(m.i.iter().all(|&i| (i as usize) < m.v.len()));
            let mut longe = Malha::default();
            aura(&mut longe, &c, olho, 1.7, false);
            assert!(longe.v.len() < 400, "R{r} far: {} vertices", longe.v.len());
        }
    }
}
