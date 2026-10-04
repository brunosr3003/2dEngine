//! THE DEEP's effects (Abyssia, `shared::abissal`): what makes the sea floor
//! read as under the sea, with light effects drawn after the world:
//!
//! * the BUBBLE over the kingdom: a huge glassy dome, bright at its rim and
//!   almost clear where you look through it;
//! * LIGHT RAYS falling into the bubble from above, slowly swaying;
//! * MOTES drifting upwards round the camera;
//! * SCHOOLS OF FISH circling over the floor;
//! * the BUBBLE HELMET every character wears here (the owner's one rule for
//!   this map), round each head.

use macroquad::prelude::*;
use std::f32::consts::TAU;


/// A soft-edged quad strip and disc builder over one additive mesh.
#[derive(Default)]
struct Lote {
    v: Vec<Vertex>,
    i: Vec<u16>,
}

impl Lote {
    fn cheio(&self) -> bool {
        self.v.len() > 60_000
    }

    fn tri(&mut self, a: (Vec3, Color), b: (Vec3, Color), c: (Vec3, Color)) {
        let n = self.v.len() as u16;
        for (p, cor) in [a, b, c] {
            self.v.push(Vertex::new2(p, Vec2::ZERO, cor));
        }
        self.i.extend_from_slice(&[n, n + 1, n + 2]);
    }

    fn quad(&mut self, a: (Vec3, Color), b: (Vec3, Color), c: (Vec3, Color), d: (Vec3, Color)) {
        let n = self.v.len() as u16;
        for (p, cor) in [a, b, c, d] {
            self.v.push(Vertex::new2(p, Vec2::ZERO, cor));
        }
        self.i.extend_from_slice(&[n, n + 1, n + 2, n, n + 2, n + 3]);
    }

    fn desenha(self) {
        if !self.v.is_empty() {
            draw_mesh(&Mesh { vertices: self.v, indices: self.i, texture: None });
        }
    }
}

fn alfa(c: Color, a: f32) -> Color {
    Color::new(c.r, c.g, c.b, a)
}

/// The kingdom's dome: a sphere cap over the bubble, each vertex brighter
/// the more it is seen edge-on (a fresnel by hand).
fn bolha(l: &mut Lote, cam: &Camera3D, chao_y: f32) {
    let r = shared::abissal::BOLHA_RAIO;
    let c = vec3(0.0, chao_y - r * 0.25, 0.0);
    let cor = Color::new(0.55, 0.9, 1.0, 1.0);
    const LAT: usize = 14;
    const LON: usize = 40;
    let ponto = |i: usize, j: usize| {
        let lat = (i as f32 / LAT as f32) * std::f32::consts::FRAC_PI_2 * 1.15 - 0.2;
        let lon = j as f32 / LON as f32 * TAU;
        let n = vec3(lat.cos() * lon.cos(), lat.sin(), lat.cos() * lon.sin());
        let p = c + n * r;
        let olho = (cam.position - p).normalize_or_zero();
        let borda = 1.0 - n.dot(olho).abs();
        (p, alfa(cor, 0.02 + 0.22 * borda.powf(3.0)))
    };
    for i in 0..LAT {
        for j in 0..LON {
            l.quad(ponto(i, j), ponto(i + 1, j), ponto(i + 1, j + 1), ponto(i, j + 1));
        }
    }
}

/// Light rays into the bubble: tall faint wedges from the dome's top down to
/// the floor, swaying with the time.
fn raios(l: &mut Lote, cam: &Camera3D, chao_y: f32, t: f32) {
    let frente = (cam.target - cam.position).normalize_or_zero();
    let lado = frente.cross(Vec3::Y).normalize_or_zero();
    let topo = chao_y + shared::abissal::BOLHA_RAIO * 0.7;
    for k in 0..18 {
        let a = k as f32 * 2.39996;
        let r = (k as f32 * 13.7) % (shared::abissal::BOLHA_RAIO * 0.85);
        let base = vec3(a.cos() * r, chao_y, a.sin() * r);
        let deriva = vec3((t * 0.13 + k as f32).sin() * 6.0, 0.0, (t * 0.11 + k as f32 * 0.7).cos() * 6.0);
        let cima = base + deriva + vec3(0.0, topo - chao_y, 0.0);
        let w = 2.5 + (k % 4) as f32;
        let pulso = 0.05 + 0.03 * (t * 0.6 + k as f32).sin();
        let cor = Color::new(0.7, 0.95, 1.0, 1.0);
        l.quad(
            (cima - lado * w * 0.4, alfa(cor, pulso)),
            (cima + lado * w * 0.4, alfa(cor, pulso)),
            (base + lado * w, alfa(cor, 0.0)),
            (base - lado * w, alfa(cor, 0.0)),
        );
    }
}

/// Motes drifting upwards in a box round the camera's target.
fn particulas(l: &mut Lote, cam: &Camera3D, chao_y: f32, t: f32) {
    let frente = (cam.target - cam.position).normalize_or_zero();
    let lado = frente.cross(Vec3::Y).normalize_or_zero() * 0.07;
    let cima = Vec3::Y * 0.07;
    let alvo = cam.target;
    const LADO: f32 = 40.0;
    for k in 0..260u32 {
        let h = |s: u32| ((k.wrapping_mul(2654435761).wrapping_add(s.wrapping_mul(40503))) % 10_000) as f32 / 10_000.0;
        // A fixed lattice in world space, wrapped round the target: motes stay
        // put when the camera moves, and new ones come in at the edges.
        let wrap = |base: f32, centro: f32| centro + ((base - centro).rem_euclid(LADO)) - LADO * 0.5;
        let x = wrap(h(1) * LADO * 8.0, alvo.x);
        let z = wrap(h(2) * LADO * 8.0, alvo.z);
        let y = chao_y + ((h(3) * 12.0 + t * (0.25 + h(4) * 0.4)) % 12.0);
        let p = vec3(x + (t * 0.5 + h(5) * 6.0).sin() * 0.3, y, z);
        let c = Color::new(0.75, 0.95, 1.0, 0.35);
        l.quad((p - lado - cima, c), (p + lado - cima, c), (p + lado + cima, c), (p - lado + cima, c));
    }
}

/// Schools of fish: a dozen slivers each, circling at their own height
/// round their own centre.
fn cardumes(l: &mut Lote, cam: &Camera3D, chao: &dyn Fn(f32, f32) -> f32, t: f32) {
    let alvo = vec2(cam.target.x, cam.target.z);
    for s in 0..40u32 {
        let h = |k: u32| ((s.wrapping_mul(2246822519).wrapping_add(k.wrapping_mul(3266489917))) % 10_000) as f32 / 10_000.0;
        let a0 = h(1) * TAU;
        let r0 = shared::abissal::BOLHA_RAIO + 20.0 + h(2) * 480.0;
        let centro = vec2(a0.cos(), a0.sin()) * r0;
        if centro.distance(alvo) > 90.0 {
            continue;
        }
        let raio = 8.0 + h(3) * 10.0;
        let vel = (0.18 + h(4) * 0.2) * if s % 2 == 0 { 1.0 } else { -1.0 };
        let altura = 3.0 + h(5) * 6.0;
        let cor = match s % 3 {
            0 => Color::new(0.75, 0.9, 1.0, 0.7),
            1 => Color::new(1.0, 0.7, 0.4, 0.6),
            _ => Color::new(0.6, 1.0, 0.85, 0.6),
        };
        for f in 0..12u32 {
            let fase = t * vel + f as f32 * 0.09 + h(10 + f) * 0.05;
            let off = vec2((h(20 + f) - 0.5) * 2.0, (h(30 + f) - 0.5) * 2.0);
            let p2 = centro + vec2(fase.cos(), fase.sin()) * raio + off;
            let y = chao(p2.x, p2.y) + altura + (h(40 + f) - 0.5) * 1.5 + (t * 2.0 + f as f32).sin() * 0.15;
            let p = vec3(p2.x, y, p2.y);
            let rumo = vec3(-fase.sin(), 0.0, fase.cos()) * vel.signum();
            let lado = rumo.cross(Vec3::Y) * 0.12;
            let cabeca = p + rumo * 0.35;
            let rabo = p - rumo * 0.35;
            let bate = (t * 9.0 + f as f32).sin() * 0.1;
            l.tri((cabeca, cor), (p + lado, cor), (p - lado, cor));
            l.tri((p + lado * 0.6, cor), (rabo + lado * (1.5 + bate), alfa(cor, cor.a * 0.6)), (rabo - lado * (1.5 - bate), alfa(cor, cor.a * 0.6)));
            if l.cheio() {
                return;
            }
        }
    }
}

thread_local! {
    static MATERIAL: std::cell::OnceCell<Material> = const { std::cell::OnceCell::new() };
}

fn material() -> Material {
    use macroquad::miniquad::{BlendFactor, BlendState, BlendValue, Comparison, Equation, PipelineParams};
    MATERIAL.with(|m| {
        m.get_or_init(|| {
            load_material(
                ShaderSource::Glsl {
                    vertex: r#"#version 100
                        attribute vec3 position; attribute vec4 color0;
                        uniform mat4 Model; uniform mat4 Projection;
                        varying lowp vec4 cor;
                        void main() { gl_Position = Projection * Model * vec4(position, 1.0); cor = color0 / 255.0; }"#,
                    fragment: r#"#version 100
                        varying lowp vec4 cor;
                        void main() { if (cor.a <= 0.002) discard; gl_FragColor = cor; }"#,
                },
                MaterialParams {
                    pipeline_params: PipelineParams {
                        depth_test: Comparison::LessOrEqual,
                        depth_write: false,
                        cull_face: macroquad::miniquad::CullFace::Nothing,
                        color_blend: Some(BlendState::new(
                            Equation::Add,
                            BlendFactor::Value(BlendValue::SourceAlpha),
                            BlendFactor::One,
                        )),
                        ..Default::default()
                    },
                    ..Default::default()
                },
            )
            .expect("deep effects material")
        })
        .clone()
    })
}

/// Everything above, for one frame. Draw after the world, with its camera.
pub fn desenha(cam: &Camera3D, chao: &dyn Fn(f32, f32) -> f32) {
    crate::marinhos::cenario(cam, chao);
    let t = get_time() as f32;
    let chao_y = shared::abissal::NIVEL_CHAO as f32 * shared::terreno::BLOCO;
    let mut l = Lote::default();
    bolha(&mut l, cam, chao_y);
    if vec2(cam.target.x, cam.target.z).length() < shared::abissal::BOLHA_RAIO + 40.0 {
        raios(&mut l, cam, chao_y, t);
    }
    particulas(&mut l, cam, chao(cam.target.x, cam.target.z), t);
    cardumes(&mut l, cam, chao, t);
    gl_use_material(&material());
    l.desenha();
    gl_use_default_material();
    crate::capacete_abissal::desenha(cam);
}
