//! Effects in 3D space: slash, shot and impact follow the weapon and the target.
use macroquad::material::{
    gl_use_default_material, gl_use_material, load_material, Material, MaterialParams,
};
use macroquad::miniquad::{
    BlendFactor, BlendState, BlendValue, Comparison, Equation, PipelineParams,
};
use macroquad::prelude::*;
use std::f32::consts::{PI, TAU};

pub fn material() -> Material {
    material_com_depth(false)
}

/// Persistent auras need occlusion from the body and the equipment.
pub fn material_com_oclusao() -> Material {
    material_com_depth(true)
}

/// Without depth write the glow is not depth-tested either: it draws over
/// everything, as it always has. (The vendored miniquad now tests depth
/// whenever the pipeline asks for it, so the "no test" has to be explicit.)
fn material_com_depth(depth_write: bool) -> Material {
    load_material(ShaderSource::Glsl {
        vertex: r#"#version 100
            attribute vec3 position; attribute vec4 color0;
            uniform mat4 Model; uniform mat4 Projection;
            varying lowp vec4 cor;
            void main() { gl_Position = Projection * Model * vec4(position, 1.0); cor = color0 / 255.0; }"#,
        fragment: r#"#version 100
            varying lowp vec4 cor;
            void main() { if (cor.a <= 0.001) discard; gl_FragColor = cor; }"#,
    }, MaterialParams { pipeline_params: PipelineParams {
        depth_test: if depth_write { Comparison::LessOrEqual } else { Comparison::Always },
        depth_write,
        color_blend: Some(BlendState::new(Equation::Add, BlendFactor::Value(BlendValue::SourceAlpha), BlendFactor::One)),
        ..Default::default()
    }, ..Default::default() }).expect("emissive material for skills")
}

pub fn duracao(id: u32) -> f32 {
    match id {
        3 => 5.0,
        9 => 1.6,
        10..=12 => 1.4,
        7 | 8 => 0.85,
        _ => 1.05,
    }
}

pub struct Cena {
    pub id: u32,
    pub de: Vec3,
    pub alvo: Vec3,
    pub maos: [Vec3; 2],
    pub t: f32,
    pub impacto: bool,
    pub atraso: f32,
    pub raio: f32,
    pub frente: Vec3,
}

fn alfa(mut c: Color, a: f32) -> Color {
    c.a *= a.clamp(0.0, 1.0);
    c
}

/// Soft luminous volume: three crossed discs with radial opacity falloff.
fn halo(p: Vec3, raio: f32, cor: Color) {
    if raio < 0.005 || cor.a < 0.005 {
        return;
    }
    let mut vertices = Vec::with_capacity(3 * 49);
    let mut indices = Vec::with_capacity(3 * 24 * 9);
    for (a, b) in [(Vec3::X, Vec3::Y), (Vec3::Z, Vec3::Y), (Vec3::X, Vec3::Z)] {
        let base = vertices.len() as u16;
        vertices.push(Vertex::new2(p, Vec2::ZERO, alfa(cor, 0.8)));
        for r in [0.3, 1.0] {
            for i in 0..24 {
                let ang = i as f32 * TAU / 24.0;
                vertices.push(Vertex::new2(
                    p + (a * ang.cos() + b * ang.sin()) * raio * r,
                    Vec2::ZERO,
                    alfa(cor, if r < 0.5 { 0.3 } else { 0.0 }),
                ));
            }
        }
        for i in 0..24u16 {
            let j = (i + 1) % 24;
            indices.extend_from_slice(&[
                base,
                base + 1 + i,
                base + 1 + j,
                base + 1 + i,
                base + 25 + i,
                base + 1 + j,
                base + 1 + j,
                base + 25 + i,
                base + 25 + j,
            ]);
        }
    }
    draw_mesh(&Mesh {
        vertices,
        indices,
        texture: None,
    });
}

fn energia(pontos: &[Vec3], largura: f32, cor: Color) {
    fita(pontos, largura * 3.0, alfa(cor, 0.16));
    fita(pontos, largura, cor);
    fita(pontos, largura * 0.2, alfa(WHITE, cor.a * 0.9));
}

fn onda(p: Vec3, raio: f32, t: f32, cor: Color) {
    let fade = (1.0 - t / 0.85).clamp(0.0, 1.0);
    let r = raio * (1.0 - (-t * 8.0).exp());
    for (s, h) in [(1.0, 0.1), (0.75, 0.24)] {
        arco(
            p + Vec3::Y * h,
            Vec3::Z,
            r * s,
            0.0,
            TAU,
            0.0,
            0.18 * fade,
            alfa(cor, fade),
        );
    }
}

fn selo(p: Vec3, raio: f32, giro: f32, cor: Color) {
    for r in [raio, raio * 0.83] {
        arco(p, Vec3::Z, r, 0.0, TAU, 0.0, 0.045, cor);
    }
    for i in 0..8 {
        let a = i as f32 * TAU / 8.0 + giro;
        let d = vec3(a.cos(), 0.0, a.sin());
        let l = Vec3::Y.cross(d);
        energia(
            &[
                p + d * raio * 0.67 - l * 0.1,
                p + d * raio * 0.78,
                p + d * raio * 0.67 + l * 0.1,
            ],
            0.032,
            cor,
        );
    }
}

fn faiscas(p: Vec3, t: f32, cor: Color, alcance: f32, n: usize) {
    let fade = (1.0 - t / 1.05).clamp(0.0, 1.0);
    for i in 0..n {
        let a = i as f32 * 2.39996;
        let v = vec3(a.cos(), 0.2 + (i % 5) as f32 * 0.22, a.sin()) * alcance;
        let q = p + v * t * 2.3 - Vec3::Y * t * t * 1.8;
        energia(
            &[q - v * 0.075, q - v * 0.035, q],
            0.035 * fade,
            alfa(cor, fade),
        );
        halo(q, 0.13 * fade, alfa(cor, fade * 0.65));
    }
}

fn corte(p: Vec3, frente: Vec3, raio: f32, inclinacao: f32, t: f32, cor: Color) {
    let f = (1.0 - t / 0.7).clamp(0.0, 1.0);
    for (w, a) in [(0.75, 0.15), (0.3, 0.9), (0.045, 1.0)] {
        arco(
            p,
            frente,
            raio + t * 0.4,
            -1.5,
            1.5,
            inclinacao,
            w * f,
            alfa(if w < 0.1 { WHITE } else { cor }, a * f),
        );
    }
}

/// A tapered ribbon, with a bright core and transparent edges.
pub fn fita(pontos: &[Vec3], largura: f32, cor: Color) {
    if pontos.len() < 2 {
        return;
    }
    let mut vertices = Vec::with_capacity(pontos.len() * 3);
    let mut indices = Vec::with_capacity(pontos.len() * 24);
    for (i, &p) in pontos.iter().enumerate() {
        let u = i as f32 / (pontos.len() - 1) as f32;
        let d = pontos[(i + 1).min(pontos.len() - 1)] - pontos[i.saturating_sub(1)];
        let lado = d.cross(Vec3::Y).try_normalize().unwrap_or(Vec3::X);
        let w = largura * (PI * u).sin().max(0.025);
        for (q, c) in [
            (p - lado * w, alfa(cor, 0.05)),
            (p, cor),
            (p + lado * w, alfa(cor, 0.05)),
        ] {
            vertices.push(Vertex::new2(q, Vec2::ZERO, c));
        }
        if i > 0 {
            let b = (i * 3) as u16;
            for j in 0..2 {
                indices.extend_from_slice(&[
                    b - 3 + j,
                    b + j,
                    b - 2 + j,
                    b - 2 + j,
                    b + j,
                    b + 1 + j,
                    b - 3 + j,
                    b - 2 + j,
                    b + j,
                    b - 2 + j,
                    b + 1 + j,
                    b + j,
                ]);
            }
        }
    }
    draw_mesh(&Mesh {
        vertices,
        indices,
        texture: None,
    });
}

fn arco(
    centro: Vec3,
    frente: Vec3,
    raio: f32,
    inicio: f32,
    fim: f32,
    inclina: f32,
    largura: f32,
    cor: Color,
) {
    let lado = Vec3::Y.cross(frente).normalize_or_zero();
    let pontos: Vec<_> = (0..=28)
        .map(|i| {
            let ang = inicio + (fim - inicio) * i as f32 / 28.0;
            centro
                + frente * (ang.cos() * raio)
                + lado * (ang.sin() * raio)
                + Vec3::Y * (ang.sin() * inclina)
        })
        .collect();
    fita(&pontos, largura, cor);
}

fn brilho(p: Vec3, tamanho: f32, cor: Color) {
    for eixo in [Vec3::X, Vec3::Y, Vec3::Z] {
        fita(
            &[p - eixo * tamanho, p, p + eixo * tamanho],
            tamanho * 0.15,
            cor,
        );
    }
}

fn barril(p: Vec3, giro: f32) {
    // Faceted wood and two bands; real volume, with perspective and occlusion.
    for i in 0..12 {
        let a = i as f32 * TAU / 12.0 + giro;
        let b = (i + 1) as f32 * TAU / 12.0 + giro;
        let normal = |a: f32, y: f32| p + vec3(a.cos() * 0.19, y, a.sin() * 0.19);
        let cor = Color::new(0.36 + 0.09 * (i % 2) as f32, 0.20, 0.09, 1.0);
        let vertices = [
            normal(a, -0.24),
            normal(b, -0.24),
            normal(b, 0.24),
            normal(a, 0.24),
        ]
        .into_iter()
        .map(|p| Vertex::new2(p, Vec2::ZERO, cor))
        .collect();
        draw_mesh(&Mesh {
            vertices,
            indices: vec![0, 1, 2, 0, 2, 3, 2, 1, 0, 3, 2, 0],
            texture: None,
        });
    }
    for h in [-0.16, 0.16] {
        arco(p + Vec3::Y * h, Vec3::Z, 0.195, 0.0, TAU, 0.0, 0.035, GRAY);
    }
    draw_cylinder(p + Vec3::Y * 0.24, 0.18, 0.18, 0.025, None, BROWN);
    brilho(p + Vec3::Y * 0.3, 0.07, ORANGE);
}

pub fn desenha(c: &Cena, material: &Material) {
    let frente = vec3(c.alvo.x - c.de.x, 0.0, c.alvo.z - c.de.z)
        .try_normalize()
        .unwrap_or(c.frente);
    let lado = Vec3::Y.cross(frente);
    let u = (c.t / c.atraso.max(0.01)).clamp(0.0, 1.0);
    // Opaque wood first; only the flame and the trails use additive light.
    if c.id == 9 && !c.impacto {
        let v = ((u - 0.45) / 0.55).clamp(0.0, 1.0);
        let p = if u < 0.45 {
            c.maos[0]
        } else {
            (c.de + Vec3::Y * 1.65).lerp(c.alvo + Vec3::Y * 0.24, v)
                + Vec3::Y * (PI * v).sin() * 1.5
        };
        barril(p, u * 6.0);
    }
    gl_use_material(material);
    desenha_luz(c, frente, lado, u);
    gl_use_default_material();
}

fn desenha_luz(c: &Cena, frente: Vec3, lado: Vec3, u: f32) {
    let ouro = Color::new(1.0, 0.55, 0.08, 1.0);
    let azul = Color::new(0.08, 0.65, 1.0, 1.0);
    let verde = Color::new(0.1, 1.0, 0.4, 1.0);
    let violeta = Color::new(0.65, 0.12, 1.0, 1.0);
    let peito = c.alvo + Vec3::Y * 1.1;
    let cor = match c.id {
        1..=3 => ouro,
        4..=6 => azul,
        7..=9 => ORANGE,
        10 | 11 => verde,
        _ => violeta,
    };
    if !c.impacto {
        // Every skill has a visible wind-up from the start.
        for &mao in &c.maos {
            halo(mao, 0.32 + u * 0.55, alfa(cor, 0.3 + u * 0.5));
            brilho(mao, 0.18 + u * 0.22, alfa(WHITE, u * 0.75));
        }
        match c.id {
            1 => {
                for s in [-1.0, 1.0] {
                    energia(
                        &[
                            c.de - frente * 2.0 + lado * s * 0.45 + Vec3::Y * 0.3,
                            c.de - frente * 0.7 + lado * s * 0.55 + Vec3::Y * 0.7,
                            c.de + lado * s * 0.5 + Vec3::Y,
                        ],
                        0.22,
                        alfa(ouro, u),
                    );
                }
            }
            2 | 4 | 5 => {
                let p = c.maos[0];
                energia(
                    &[p, p + Vec3::Y * 0.6, p + Vec3::Y * 1.2],
                    0.12,
                    alfa(cor, u),
                );
                if c.id == 5 && u > 0.5 {
                    corte(
                        c.de + Vec3::Y,
                        frente,
                        1.55,
                        0.5,
                        (u - 0.5) * 0.5,
                        alfa(azul, 0.65),
                    );
                }
            }
            3 => selo(c.de + Vec3::Y * 0.08, 1.25, u, alfa(ouro, u)),
            6 => {
                if u > 0.72 {
                    let v = ((u - 0.72) / 0.28).clamp(0.0, 1.0);
                    let p = (c.de + Vec3::Y).lerp(peito, v);
                    corte(p - frente * 0.65, frente, 1.15, 1.3, 0.05, azul);
                    halo(p, 1.0, alfa(azul, 0.5));
                }
            }
            7 | 8 => {
                let p = c.maos[0];
                arco(p, frente, 0.4 + u * 0.2, -PI, PI, 0.3, 0.07, alfa(ouro, u));
            }
            9 => {
                let v = ((u - 0.45) / 0.55).clamp(0.0, 1.0);
                let p = if u < 0.45 {
                    c.maos[0]
                } else {
                    (c.de + Vec3::Y * 1.65).lerp(c.alvo + Vec3::Y * 0.24, v)
                        + Vec3::Y * (PI * v).sin() * 1.5
                };
                halo(p, 0.65, alfa(ORANGE, 0.75));
                faiscas(p, 0.08 + (u * 0.2) % 0.15, ORANGE, 0.7, 8);
                selo(
                    c.alvo + Vec3::Y * 0.09,
                    c.raio.max(1.0) * 0.65,
                    0.0,
                    alfa(ORANGE, u * 0.6),
                );
            }
            10..=12 => {
                let p = if c.id == 12 { c.alvo } else { c.de };
                selo(
                    p + Vec3::Y * 0.09,
                    if c.id == 11 { 2.3 } else { 1.2 },
                    u * 0.5,
                    alfa(cor, 0.25 + u * 0.65),
                );
                for i in 0..10 {
                    let a = i as f32 * TAU / 10.0 + u * 3.0;
                    let q = p + vec3(a.cos(), 0.25 + u * 1.8, a.sin()) * (1.0 - u * 0.5);
                    halo(q, 0.16, alfa(cor, 0.7));
                }
                if c.id == 12 {
                    halo(peito + Vec3::Y * 4.0, 0.35 + u, alfa(violeta, u));
                }
            }
            _ => {}
        }
        return;
    }
    let t = c.t;
    let fade = (1.0 - t / duracao(c.id)).clamp(0.0, 1.0);
    match c.id {
        1 => {
            let p = peito;
            halo(p, 1.8, alfa(ouro, (1.0 - t / 0.55).max(0.0)));
            onda(c.alvo, 2.2, t, ouro);
            faiscas(p, t, ouro, 1.5, 20);
            for s in [-1.0, 1.0] {
                energia(
                    &[
                        c.de + Vec3::Y * 0.5 + lado * s * 0.45,
                        c.de.lerp(c.alvo, 0.5) + Vec3::Y * 0.7 + lado * s * 0.5,
                        peito,
                    ],
                    0.2,
                    alfa(ouro, (1.0 - t / 0.5).max(0.0)),
                );
            }
        }
        2 => {
            corte(c.de + Vec3::Y, frente, 2.3, 0.25, t, ouro);
            corte(c.de + Vec3::Y * 0.8, frente, 1.9, -0.3, t + 0.08, ORANGE);
            halo(peito, 1.3, alfa(ouro, fade * 0.8));
            faiscas(peito, t, ouro, 1.4, 18);
        }
        3 => {
            let p = c.de + Vec3::Y * 1.0;
            let ativo = (t / 0.12).min(1.0) * ((5.0 - t) / 0.35).clamp(0.0, 1.0);
            selo(c.de + Vec3::Y * 0.07, 1.5, t * 0.3, alfa(ouro, ativo * 0.8));
            for h in [0.2, 0.85, 1.5, 2.1] {
                arco(
                    c.de + Vec3::Y * h,
                    frente,
                    if h > 2.0 { 0.7 } else { 1.25 },
                    0.0,
                    TAU,
                    0.0,
                    0.055,
                    alfa(ouro, ativo * 0.6),
                );
            }
            for i in 0..6 {
                let a = i as f32 * TAU / 6.0 + t * 0.15;
                let centro = p + vec3(a.cos(), 0.0, a.sin()) * 1.25;
                let lateral = vec3(-a.sin(), 0.0, a.cos());
                let pontos: Vec<_> = (0..=6)
                    .map(|j| {
                        let b = j as f32 * TAU / 6.0;
                        centro + lateral * b.cos() * 0.5 + Vec3::Y * b.sin() * 0.68
                    })
                    .collect();
                energia(&pontos, 0.055, alfa(ouro, ativo * 0.7));
            }
            halo(p, 1.8, alfa(ouro, ativo * 0.16));
            faiscas(p, t, ouro, 1.0, 18);
        }
        4 => {
            let pontos = [
                peito - lado * 2.1 - Vec3::Y * 0.7,
                peito,
                peito + lado * 2.1 + Vec3::Y * 0.7,
            ];
            energia(&pontos, 0.4 * fade, alfa(azul, fade));
            halo(peito, 1.3, alfa(azul, fade * 0.7));
            faiscas(peito, t, azul, 1.6, 22);
        }
        5 => {
            for i in 0..3 {
                let a = t * 3.0 + i as f32 * TAU / 3.0;
                let f = frente * a.cos() + lado * a.sin();
                corte(
                    peito - f * 0.35,
                    f,
                    1.65,
                    if i % 2 == 0 { 0.85 } else { -0.85 },
                    t * 0.65,
                    azul,
                );
            }
            onda(c.alvo, 2.5, t, alfa(azul, 0.6));
            faiscas(peito, t, azul, 1.7, 26);
        }
        6 => {
            corte(peito - frente * 0.6, frente, 1.6, 1.7, t, azul);
            halo(peito, 1.9, alfa(azul, fade * 0.7));
            onda(c.alvo, 2.4, t, azul);
            faiscas(peito, t, azul, 2.0, 26);
        }
        7 | 8 => {
            let n = if c.id == 7 { 1 } else { 5 };
            let disparo = (1.0 - t / 0.32).clamp(0.0, 1.0);
            for i in 0..n {
                let mao = c.maos[i % 2];
                let curva = lado * (i as f32 - (n - 1) as f32 * 0.5) * 0.22;
                energia(
                    &[mao, mao.lerp(peito, 0.5) + curva, peito],
                    if c.id == 7 { 0.14 } else { 0.07 },
                    alfa(ouro, disparo),
                );
                halo(mao, 0.8, alfa(ORANGE, disparo * 0.75));
            }
            halo(
                peito,
                if c.id == 7 { 1.0 } else { 1.6 },
                alfa(ORANGE, fade * 0.75),
            );
            brilho(peito, 0.65 * fade, alfa(WHITE, fade));
            faiscas(peito, t, ouro, 1.5, if c.id == 7 { 16 } else { 28 });
        }
        9 => {
            let explosao = (1.0 - t / 0.85).clamp(0.0, 1.0);
            let r = 0.7 + (t * 10.0).min(1.0) * 1.2;
            halo(c.alvo + Vec3::Y * 0.8, r * 2.2, alfa(RED, explosao * 0.7));
            for i in 0..7 {
                let a = i as f32 * 2.399;
                let p = c.alvo
                    + vec3(
                        a.cos() * r * 0.6,
                        0.6 + (i % 3) as f32 * 0.38 + t * 1.4,
                        a.sin() * r * 0.6,
                    );
                halo(p, r, alfa(ORANGE, explosao));
                halo(p, r * 0.42, alfa(YELLOW, explosao));
            }
            brilho(
                c.alvo + Vec3::Y * 0.8,
                1.7 * explosao,
                alfa(WHITE, explosao),
            );
            onda(c.alvo, c.raio.max(3.0), t, ORANGE);
            faiscas(peito, t, ORANGE, 2.8, 38);
        }
        10 | 11 => {
            let r = if c.id == 10 { 1.0 } else { c.raio.max(3.0) };
            selo(
                c.de + Vec3::Y * 0.09,
                r * (0.65 + 0.35 * (t * 6.0).min(1.0)),
                t * 0.35,
                alfa(verde, fade),
            );
            if c.id == 11 {
                onda(c.de, r, t, verde);
            }
            halo(c.de + Vec3::Y, 1.9, alfa(verde, fade * 0.3));
            for j in 0..3 {
                let pontos: Vec<_> = (0..=32)
                    .map(|i| {
                        let u = i as f32 / 32.0;
                        let a = u * TAU + t * 3.0 + j as f32 * TAU / 3.0;
                        c.de + vec3(a.cos() * r * 0.65, u * 2.7, a.sin() * r * 0.65)
                    })
                    .collect();
                energia(&pontos, 0.1, alfa(verde, fade * 0.8));
            }
            for i in 0..18 {
                let a = i as f32 * 2.399 + t;
                let p = c.de
                    + vec3(
                        a.cos() * r * 0.8,
                        (i % 5) as f32 * 0.4 + t * 1.1,
                        a.sin() * r * 0.8,
                    );
                brilho(p, 0.13 * fade, alfa(verde, fade));
                halo(p, 0.3, alfa(verde, fade * 0.7));
            }
        }
        12 => {
            let p = c.alvo;
            let coluna = (1.0 - t / 0.85).clamp(0.0, 1.0);
            energia(
                &[p + Vec3::Y * 8.0, p + Vec3::Y * 4.0, p + Vec3::Y * 0.25],
                0.7 * coluna,
                alfa(violeta, coluna),
            );
            for i in 0..5 {
                let a = i as f32 * TAU / 5.0 + t * 2.0;
                let pontos: Vec<_> = (0..=10)
                    .map(|j| {
                        let h = j as f32 / 10.0;
                        let desloc = (j as f32 * 4.7 + i as f32).sin() * 0.5;
                        p + vec3(a.cos(), 0.0, a.sin()) * (0.5 + desloc) + Vec3::Y * (h * 7.0)
                    })
                    .collect();
                energia(&pontos, 0.06, alfa(violeta, coluna));
            }
            selo(
                p + Vec3::Y * 0.09,
                c.raio.max(2.5),
                t * 0.15,
                alfa(violeta, fade),
            );
            halo(peito, 2.4, alfa(violeta, coluna * 0.8));
            onda(p, c.raio.max(3.0), t, violeta);
            faiscas(peito, t, violeta, 2.5, 32);
        }
        _ => {}
    }
}
