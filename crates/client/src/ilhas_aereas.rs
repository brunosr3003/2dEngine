//! Cenário das Ilhas Mágicas: bases suspensas e nuvens abaixo do chão real.
//! Não altera o relevo caminhável nem acrescenta obstáculos.
use crate::gpu_estatica::{MalhaEstatica, Programa};
use macroquad::prelude::*;

pub struct IlhasAereas {
    malhas: Vec<MalhaEstatica>,
}

fn triangulo(m: &mut Mesh, a: Vec3, b: Vec3, c: Vec3, cor: [u8; 4], fora: Vec3) {
    let i = m.vertices.len() as u16;
    for p in [a, b, c] {
        m.vertices.push(Vertex {
            position: p,
            uv: Vec2::ZERO,
            color: cor,
            normal: Vec4::ZERO,
        });
    }
    if (b - a).cross(c - a).dot(fora) >= 0.0 {
        m.indices.extend([i, i + 1, i + 2]);
    } else {
        m.indices.extend([i, i + 2, i + 1]);
    }
}
fn malha() -> Mesh {
    Mesh {
        vertices: Vec::new(),
        indices: Vec::new(),
        texture: None,
    }
}
fn tom(c: Vec3) -> [u8; 4] {
    [c.x as u8, c.y as u8, c.z as u8, 255]
}

impl IlhasAereas {
    pub fn nova() -> Self {
        let mut malhas = Vec::new();
        for ilha in shared::magica::ilhotas() {
            let mut m = malha();
            let centro = vec3(ilha.centro.x, 0.0, ilha.centro.y);
            const N: usize = 96;
            let aneis = [
                (1.0, 4.0),
                (0.92, -1.5),
                (0.73, -11.0),
                (0.40, -24.0),
                (0.07, -34.0),
            ];
            let ponto = |k: usize, j: usize| {
                let a = j as f32 / N as f32 * std::f32::consts::TAU;
                let r = shared::magica::raio_da_ilhota(&ilha, a) + 0.35;
                let (escala, y) = aneis[k];
                let variacao = if k == 0 {
                    0.0
                } else {
                    (a * 5.0 + ilha.centro.x).sin() * 2.0
                };
                centro + vec3(a.cos() * r * escala, y + variacao, a.sin() * r * escala)
            };
            for k in 0..aneis.len() - 1 {
                for j in 0..N {
                    let a = ponto(k, j);
                    let b = ponto(k, j + 1);
                    let c = ponto(k + 1, j + 1);
                    let d = ponto(k + 1, j);
                    let fora = vec3((a.x - centro.x) * 0.2, -1.0, (a.z - centro.z) * 0.2);
                    let face = ((j * 17 + k * 7) % 9) as f32;
                    let cor = tom(vec3(132.0, 139.0, 163.0) - Vec3::splat(k as f32 * 9.0)
                        + Vec3::splat(face * 1.4));
                    triangulo(&mut m, a, b, c, cor, fora);
                    triangulo(&mut m, a, c, d, cor, fora);
                }
            }
            for j in 0..N {
                triangulo(
                    &mut m,
                    ponto(4, j),
                    ponto(4, j + 1),
                    centro + vec3(0.0, -38.0, 0.0),
                    [94, 105, 140, 255],
                    -Vec3::Y,
                );
            }
            malhas.push(MalhaEstatica::nova(m));
            // Fragmentos separados abaixo da base tornam o vazio legível.
            for k in 0..3 {
                let a = k as f32 * 2.1 + ilha.centro.y * 0.04;
                let p = centro
                    + vec3(
                        a.cos() * ilha.raio * 0.65,
                        -27.0 - k as f32 * 6.0,
                        a.sin() * ilha.raio * 0.65,
                    );
                let mut m = malha();
                for j in 0..5 {
                    let a = j as f32 * std::f32::consts::TAU / 5.0;
                    let b = (j + 1) as f32 * std::f32::consts::TAU / 5.0;
                    let v = p + vec3(a.cos() * 2.2, 0.0, a.sin() * 2.2);
                    let w = p + vec3(b.cos() * 2.2, 0.0, b.sin() * 2.2);
                    triangulo(
                        &mut m,
                        v,
                        w,
                        p + vec3(0.0, 3.0, 0.0),
                        [156, 169, 197, 255],
                        Vec3::Y,
                    );
                    triangulo(
                        &mut m,
                        v,
                        w,
                        p - vec3(0.0, 7.0, 0.0),
                        [117, 133, 168, 255],
                        -Vec3::Y,
                    );
                }
                malhas.push(MalhaEstatica::nova(m));
            }
        }
        // Massas de nuvens espalhadas em duas alturas, com vãos de céu.
        for k in 0..42 {
            let a = k as f32 * 2.39996;
            let r = 90.0 + (k as f32 / 42.0) * 640.0;
            let centro = vec3(a.cos() * r, -65.0 - (k % 3) as f32 * 14.0, a.sin() * r);
            let mut m = malha();
            for l in 0..4 {
                let p = centro
                    + vec3(
                        l as f32 * 15.0 - 22.0,
                        (l % 2) as f32 * 3.0,
                        (l as f32 * 2.3).sin() * 12.0,
                    );
                let escala = vec3(
                    24.0 + (l % 2) as f32 * 8.0,
                    8.0 + (l % 3) as f32 * 3.0,
                    19.0,
                );
                let ponto = |u: usize, v: usize| {
                    let a = u as f32 * std::f32::consts::TAU / 16.0;
                    let b = v as f32 * std::f32::consts::PI / 8.0;
                    p + vec3(a.cos() * b.sin(), b.cos(), a.sin() * b.sin()) * escala
                };
                for v in 0..8 {
                    for u in 0..16 {
                        let a = ponto(u, v);
                        let b = ponto(u + 1, v);
                        let c = ponto(u + 1, v + 1);
                        let d = ponto(u, v + 1);
                        let cor = tom(vec3(246.0, 245.0, 251.0) - vec3(4.5, 4.0, 2.5) * v as f32);
                        let fora = (a + b + c + d) * 0.25 - p;
                        triangulo(&mut m, a, b, c, cor, fora);
                        triangulo(&mut m, a, c, d, cor, fora);
                    }
                }
            }
            malhas.push(MalhaEstatica::nova(m));
        }
        Self { malhas }
    }
    pub fn desenha(&self) {
        crate::gpu_estatica::desenha(
            Programa::Solido {
                recorte: Vec3::ZERO,
                recorte_z: 0.0,
            },
            &self.malhas,
        );
    }
}
