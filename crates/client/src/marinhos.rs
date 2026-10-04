//! Marine bodies have fins and animated tails, with no walking legs.
use crate::gpu_estatica::{MalhaEstatica, Programa};
use macroquad::models::{Mesh, Vertex};
use macroquad::prelude::*;
use std::cell::RefCell;

fn triangle(m: &mut Mesh, a: Vec3, b: Vec3, c: Vec3, color: Color) {
    let i = m.vertices.len() as u16;
    for position in [a, b, c] {
        m.vertices.push(Vertex {
            position,
            uv: Vec2::ZERO,
            color: color.into(),
            normal: Vec4::ZERO,
        });
    }
    m.indices
        .extend_from_slice(&[i, i + 1, i + 2, i + 2, i + 1, i]);
}
fn ellipsoid(m: &mut Mesh, p: Vec3, size: Vec3, color: Color) {
    let point = |j: usize, k: usize| {
        let lat = -std::f32::consts::FRAC_PI_2 + j as f32 * std::f32::consts::PI / 6.;
        let lon = k as f32 * std::f32::consts::TAU / 10.;
        p + vec3(lat.cos() * lon.cos(), lat.sin(), lat.cos() * lon.sin()) * size
    };
    for j in 0..6 {
        for k in 0..10 {
            let shade = 0.65 + j as f32 * 0.055;
            let tint = Color::new(color.r * shade, color.g * shade, color.b * shade, 1.);
            triangle(m, point(j, k), point(j + 1, k), point(j + 1, k + 1), tint);
            triangle(m, point(j, k), point(j + 1, k + 1), point(j, k + 1), tint);
        }
    }
}
fn empty() -> Mesh {
    Mesh {
        vertices: vec![],
        indices: vec![],
        texture: None,
    }
}
struct Model {
    body: MalhaEstatica,
    tail: MalhaEstatica,
    horizontal: bool,
    pivot: Vec3,
}
fn model(kind: u16) -> Model {
    let mut body = empty();
    let mut tail = empty();
    let sea = Color::from_rgba(67, 137, 158, 255);
    let belly = Color::from_rgba(191, 216, 210, 255);
    let human = matches!(kind, 71 | 73 | 75 | 91 | 92);
    let whale = kind == 90;
    let eel = kind == 72;
    let octopus = kind == 76;
    let pivot;
    if octopus {
        ellipsoid(
            &mut body,
            vec3(0., 0.35, 0.),
            vec3(0.7, 0.85, 0.7),
            Color::from_rgba(147, 83, 151, 255),
        );
        for sign in [-1., 1.] {
            ellipsoid(
                &mut body,
                vec3(sign * 0.35, 0.35, 0.6),
                Vec3::splat(0.13),
                belly,
            );
        }
        for i in 0..8 {
            let a = i as f32 * std::f32::consts::TAU / 8.;
            let axis = vec3(a.cos(), 0., a.sin());
            for j in 1..=6 {
                let f = j as f32 / 6.;
                ellipsoid(
                    &mut tail,
                    axis * (0.3 + f * 1.7) + vec3(0., -0.3 + (f * 3.).sin() * 0.25, 0.),
                    Vec3::splat(0.25 * (1. - f * 0.7)),
                    sea,
                );
            }
        }
        pivot = Vec3::ZERO;
    } else if human {
        let siren = matches!(kind, 75 | 91);
        let skin = if siren {
            Color::from_rgba(161, 207, 195, 255)
        } else {
            sea
        };
        ellipsoid(&mut body, vec3(0., 0.4, 0.), vec3(0.27, 0.4, 0.19), skin);
        ellipsoid(&mut body, vec3(0., 1., 0.), vec3(0.23, 0.27, 0.23), skin);
        ellipsoid(&mut body, vec3(-0.37, 0.3, 0.), vec3(0.12, 0.4, 0.12), skin);
        ellipsoid(&mut body, vec3(0.37, 0.3, 0.), vec3(0.12, 0.4, 0.12), skin);
        if siren {
            ellipsoid(
                &mut body,
                vec3(0., 1.1, -0.14),
                vec3(0.28, 0.29, 0.19),
                Color::from_rgba(104, 54, 146, 255),
            );
        } else {
            let mut weapon = crate::dungeon_cenario::Obra::nova();
            weapon.caixa(vec3(0.55, 0.4, 0.12), vec3(0.05, 1.8, 0.05), belly, 0.);
            for x in [-0.15, 0., 0.15] {
                weapon.caixa(vec3(0.55 + x, 1.35, 0.12), vec3(0.05, 0.4, 0.05), belly, 0.);
            }
            let offset = body.vertices.len() as u16;
            body.vertices.extend(weapon.mesh.vertices);
            body.indices
                .extend(weapon.mesh.indices.into_iter().map(|i| i + offset));
        }
        pivot = vec3(0., 0., 0.);
        ellipsoid(
            &mut tail,
            vec3(0., -0.35, -0.3),
            vec3(0.25, 0.35, 0.55),
            sea,
        );
        triangle(
            &mut tail,
            vec3(0., -0.4, -0.75),
            vec3(-0.5, -0.5, -1.25),
            vec3(0., -0.5, -1.05),
            sea,
        );
        triangle(
            &mut tail,
            vec3(0., -0.4, -0.75),
            vec3(0.5, -0.5, -1.25),
            vec3(0., -0.5, -1.05),
            sea,
        );
    } else {
        let length = if whale {
            2.8
        } else if eel {
            1.55
        } else {
            1.25
        };
        let width = if whale {
            0.85
        } else if eel {
            0.18
        } else {
            0.38
        };
        let color = if whale {
            Color::from_rgba(57, 91, 119, 255)
        } else {
            sea
        };
        ellipsoid(
            &mut body,
            Vec3::ZERO,
            vec3(width, width * 0.85, length),
            color,
        );
        ellipsoid(
            &mut body,
            vec3(0., -width * 0.43, length * 0.2),
            vec3(width * 0.8, width * 0.44, length * 0.76),
            belly,
        );
        for sign in [-1., 1.] {
            triangle(
                &mut body,
                vec3(sign * width * 0.5, 0., 0.2),
                vec3(sign * width * 3., -0.12, -0.7),
                vec3(sign * width * 0.6, 0., -0.8),
                color,
            );
            ellipsoid(
                &mut body,
                vec3(sign * width * 0.8, width * 0.15, length * 0.7),
                Vec3::splat(if whale { 0.065 } else { 0.045 }),
                BLACK,
            );
        }
        if !eel {
            triangle(
                &mut body,
                vec3(0., width * 0.7, 0.1),
                vec3(0., width * 2.1, -0.35),
                vec3(0., width * 0.7, -0.8),
                color,
            );
        }
        if kind == 93 {
            for i in 0..5 {
                let x = (i as f32 - 2.) * 0.48;
                let height = 1.1 + (i % 2) as f32 * 0.5;
                ellipsoid(
                    &mut body,
                    vec3(x, height * 0.5, 0.7),
                    vec3(0.18, height * 0.65, 0.2),
                    sea,
                );
                ellipsoid(
                    &mut body,
                    vec3(x, height, 1.05),
                    vec3(0.24, 0.22, 0.55),
                    sea,
                );
                for sign in [-1., 1.] {
                    ellipsoid(
                        &mut body,
                        vec3(x + sign * 0.2, height + 0.08, 1.3),
                        Vec3::splat(0.06),
                        Color::from_rgba(241, 220, 77, 255),
                    );
                }
                triangle(
                    &mut body,
                    vec3(x - 0.2, height + 0.15, 0.7),
                    vec3(x, height + 0.65, 0.55),
                    vec3(x + 0.2, height + 0.15, 0.7),
                    belly,
                );
            }
        }
        pivot = vec3(0., 0., -length * 0.78);
        ellipsoid(
            &mut tail,
            vec3(0., 0., -length * 0.17),
            vec3(width * 0.32, width * 0.35, length * 0.38),
            color,
        );
        if whale {
            for sign in [-1., 1.] {
                triangle(
                    &mut tail,
                    vec3(0., 0., -length * 0.4),
                    vec3(sign * 1.1, 0., -length * 0.8),
                    vec3(sign * 0.4, 0., -length * 0.48),
                    color,
                );
            }
        } else {
            triangle(
                &mut tail,
                vec3(0., 0., -length * 0.35),
                vec3(0., width * 2., -length * 0.85),
                vec3(0., 0., -length * 0.65),
                color,
            );
            triangle(
                &mut tail,
                vec3(0., 0., -length * 0.35),
                vec3(0., -width * 1.3, -length * 0.8),
                vec3(0., 0., -length * 0.65),
                color,
            );
        }
    }
    Model {
        body: MalhaEstatica::nova(body),
        tail: MalhaEstatica::nova(tail),
        horizontal: whale || human,
        pivot,
    }
}
thread_local! {static MODELS:RefCell<std::collections::HashMap<u16,Model>>=RefCell::new(std::collections::HashMap::new());}
pub fn desenha(kind: u16, p: Vec3, yaw: f32, seed: u64, scale: f32) -> bool {
    if !matches!(kind, 70 | 71 | 72 | 73 | 75 | 76 | 90 | 91 | 92 | 93) {
        return false;
    }
    let t = get_time() as f32;
    let phase = t * 3.5 + (seed % 113) as f32;
    let position = p + vec3(0., 0.85 + phase.sin() * 0.09, 0.);
    let transform = Mat4::from_scale_rotation_translation(
        Vec3::splat(scale),
        Quat::from_rotation_y(yaw),
        position,
    );
    MODELS.with(|cache| {
        let mut cache = cache.borrow_mut();
        let m = cache.entry(kind).or_insert_with(|| model(kind));
        let program = Programa::Solido {
            recorte: Vec3::ZERO,
            recorte_z: 0.,
        };
        crate::gpu_estatica::desenha_com_modelo(program, [&m.body], transform);
        let angle = phase.sin() * 0.25;
        let rotation = if m.horizontal {
            Quat::from_rotation_x(angle)
        } else {
            Quat::from_rotation_y(angle)
        };
        crate::gpu_estatica::desenha_com_modelo(
            program,
            [&m.tail],
            transform * Mat4::from_translation(m.pivot) * Mat4::from_quat(rotation),
        );
    });
    true
}

thread_local! {
    static CORAIS:RefCell<Vec<MalhaEstatica>>=const {RefCell::new(Vec::new())};
    static SANTUARIO:RefCell<Option<MalhaEstatica>>=const {RefCell::new(None)};
}
pub fn cenario(cam: &Camera3D, chao: &dyn Fn(f32, f32) -> f32) {
    let sanctuary = shared::abissal::arenas()[3];
    if vec2(sanctuary.x, sanctuary.y).distance_squared(vec2(cam.target.x, cam.target.z))
        < 130. * 130.
    {
        SANTUARIO.with(|cache| {
            let mut cache = cache.borrow_mut();
            let mesh = cache.get_or_insert_with(|| {
                let mut o = crate::dungeon_cenario::Obra::nova();
                for i in 0..12 {
                    let a = i as f32 * std::f32::consts::TAU / 12.;
                    let p = vec3(
                        sanctuary.x + a.cos() * 24.,
                        chao(sanctuary.x, sanctuary.y),
                        sanctuary.y + a.sin() * 24.,
                    );
                    let color = Color::from_rgba(130, 167, 164, 255);
                    o.caixa(p + vec3(0., 0.4, 0.), vec3(3., 0.8, 3.), color, a);
                    o.caixa(p + vec3(0., 4., 0.), vec3(1.2, 8., 1.2), color, a);
                    o.caixa(p + vec3(0., 8., 0.), vec3(3., 0.7, 3.), color, a);
                    o.caixa(
                        p + vec3(0., 8.7, 0.),
                        vec3(0.7, 0.7, 0.7),
                        Color::from_rgba(91, 240, 183, 255),
                        a,
                    );
                }
                MalhaEstatica::nova(o.mesh)
            });
            crate::gpu_estatica::desenha(
                Programa::Solido {
                    recorte: Vec3::ZERO,
                    recorte_z: 0.,
                },
                [&*mesh],
            );
        });
    }
    CORAIS.with(|cache| {
        let mut cache = cache.borrow_mut();
        if cache.is_empty() {
            for color in [
                Color::from_rgba(230, 107, 182, 255),
                Color::from_rgba(83, 224, 202, 255),
                Color::from_rgba(137, 155, 247, 255),
            ] {
                let mut mesh = empty();
                for i in 0..6 {
                    let angle = i as f32 * std::f32::consts::TAU / 6.;
                    let axis = vec3(angle.cos(), 0., angle.sin());
                    ellipsoid(
                        &mut mesh,
                        axis * 0.4 + vec3(0., 0.5, 0.),
                        vec3(0.12, 0.6, 0.12),
                        color,
                    );
                    ellipsoid(
                        &mut mesh,
                        axis * 0.65 + vec3(0., 1., 0.),
                        vec3(0.1, 0.4, 0.1),
                        color,
                    );
                    ellipsoid(
                        &mut mesh,
                        axis * 0.9 + vec3(0., 1.25, 0.),
                        Vec3::splat(0.16),
                        color,
                    );
                }
                cache.push(MalhaEstatica::nova(mesh));
            }
        }
        let target = vec2(cam.target.x, cam.target.z);
        for (i, l) in shared::abissal::luzes().iter().enumerate() {
            if !l.coral || vec2(l.pos.x, l.pos.y).distance_squared(target) > 95. * 95. {
                continue;
            }
            let p = vec3(l.pos.x, chao(l.pos.x, l.pos.y), l.pos.y);
            crate::gpu_estatica::desenha_com_modelo(
                Programa::Solido {
                    recorte: Vec3::ZERO,
                    recorte_z: 0.,
                },
                [&cache[i % 3]],
                Mat4::from_translation(p),
            );
        }
    });
    // Large, peaceful silhouettes cruise above the hunting grounds.
    let t = get_time() as f32;
    for i in 0..8 {
        let a = i as f32 * 0.78 + t * 0.018;
        let radius = 180. + i as f32 * 43.;
        let p = vec2(a.cos(), a.sin()) * radius;
        if p.distance_squared(vec2(cam.target.x, cam.target.z)) > 120. * 120. {
            continue;
        }
        desenha(90, vec3(p.x, chao(p.x, p.y) + 12., p.y), -a, i, 1.8);
    }
}
