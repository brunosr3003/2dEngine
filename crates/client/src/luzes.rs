//! THE NEON CITY'S LIGHTS (`shared::kogen::luzes`): street lamps, shop signs
//! and lanterns that really light what is round them.
//!
//! Two halves:
//!
//! * **Light**: every frame the nearest `MAX_LUZES` lights to the camera go
//!   to the world shader (`gpu_estatica::define_luzes`), which adds each
//!   one's colour to the ground, walls and bodies inside its reach, falling
//!   off with distance. Night only.
//! * **Glow**: a soft additive halo round each light in view, tested
//!   against depth so a wall in front hides it.

use macroquad::prelude::*;
use macroquad::miniquad::{BlendFactor, BlendState, BlendValue, Comparison, Equation, PipelineParams};
use shared::kogen::{LuzDaCidade, TipoDeLuz};

/// How far from the camera's target a light still counts (units).
const ALCANCE: f32 = 70.0;
/// How far its halo is drawn.
const ALCANCE_DO_HALO: f32 = 90.0;

/// A light's strength right now: signs hum, a few of them flicker.
fn forca(l: &LuzDaCidade, t: f32) -> f32 {
    match l.tipo {
        TipoDeLuz::Poste => 1.6,
        TipoDeLuz::Lanterna => 1.3,
        TipoDeLuz::Letreiro => {
            let fase = l.seed as f32 * 1.7;
            let hum = 1.5 + 0.12 * (t * 3.0 + fase).sin();
            // One sign in twelve is on the blink.
            if l.seed.rem_euclid(12) == 0 && (t * 1.3 + fase).sin() > 0.82 {
                0.15
            } else {
                hum
            }
        }
    }
}

/// Where the light is in the world: `chao` gives the ground under it.
fn centro(l: &LuzDaCidade, chao: &dyn Fn(f32, f32) -> f32) -> Vec3 {
    let (p, alto) = l.centro();
    vec3(p.x, chao(l.pos.x, l.pos.y) + alto, p.y)
}

/// Sends the nearest lights to the shader. `ligado` false (day, another
/// island) turns them off.
pub fn preparar(ligado: bool, alvo: Vec2, chao: &dyn Fn(f32, f32) -> f32) {
    if !ligado {
        crate::gpu_estatica::define_luzes(&[]);
        return;
    }
    let t = get_time() as f32;
    let mut perto: Vec<(f32, &LuzDaCidade)> = shared::kogen::luzes()
        .iter()
        .map(|l| (dist2(l, alvo), l))
        .filter(|(d, _)| *d < ALCANCE * ALCANCE)
        .collect();
    perto.sort_by(|a, b| a.0.total_cmp(&b.0));
    let luzes: Vec<(Vec3, f32, [f32; 3], f32)> = perto
        .iter()
        .take(crate::gpu_estatica::MAX_LUZES)
        .map(|(_, l)| (centro(l, chao), l.raio, l.cor, forca(l, t)))
        .collect();
    crate::gpu_estatica::define_luzes(&luzes);
}

/// Squared distance from a light's foot to `p` (shared's glam is another
/// version than macroquad's: plain numbers between them).
fn dist2(l: &LuzDaCidade, p: Vec2) -> f32 {
    (l.pos.x - p.x).powi(2) + (l.pos.y - p.y).powi(2)
}

thread_local! {
    static MATERIAL: std::cell::OnceCell<Material> = const { std::cell::OnceCell::new() };
}

fn material() -> Material {
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
            .expect("light halo material")
        })
        .clone()
    })
}

/// A soft disc facing the camera: bright core, fading edge.
fn halo(vertices: &mut Vec<Vertex>, indices: &mut Vec<u16>, p: Vec3, raio: f32, cor: Color, dir: Vec3, cima: Vec3) {
    const LADOS: u16 = 16;
    let base = vertices.len() as u16;
    vertices.push(Vertex::new2(p, Vec2::ZERO, cor));
    for i in 0..LADOS {
        let a = i as f32 / LADOS as f32 * std::f32::consts::TAU;
        let q = p + (dir * a.cos() + cima * a.sin()) * raio;
        vertices.push(Vertex::new2(q, Vec2::ZERO, Color::new(cor.r, cor.g, cor.b, 0.0)));
    }
    for i in 0..LADOS {
        indices.extend_from_slice(&[base, base + 1 + i, base + 1 + (i + 1) % LADOS]);
    }
}

/// The halos of the lights in view. Draw after the world, with its camera.
pub fn desenha_halos(cam: &Camera3D, chao: &dyn Fn(f32, f32) -> f32) {
    let frente = (cam.target - cam.position).normalize_or_zero();
    let dir = frente.cross(Vec3::Y).normalize_or_zero();
    let cima = dir.cross(frente).normalize_or_zero();
    let alvo = vec2(cam.target.x, cam.target.z);
    let t = get_time() as f32;
    let mut vertices = Vec::new();
    let mut indices = Vec::new();
    for l in shared::kogen::luzes() {
        if dist2(l, alvo) > ALCANCE_DO_HALO * ALCANCE_DO_HALO {
            continue;
        }
        let p = centro(l, chao);
        let f = forca(l, t);
        let c = |a: f32| Color::new(l.cor[0], l.cor[1], l.cor[2], a * f.min(1.0));
        let (perto, largo) = match l.tipo {
            TipoDeLuz::Poste => (0.6, 2.4),
            TipoDeLuz::Letreiro => (1.6, 3.6),
            TipoDeLuz::Lanterna => (0.8, 2.2),
        };
        // A wide faint glow and a small hot core.
        halo(&mut vertices, &mut indices, p, largo, c(0.22), dir, cima);
        halo(&mut vertices, &mut indices, p, perto, c(0.55), dir, cima);
        if vertices.len() > 60_000 {
            break;
        }
    }
    if vertices.is_empty() {
        return;
    }
    gl_use_material(&material());
    draw_mesh(&Mesh { vertices, indices, texture: None });
    gl_use_default_material();
}
