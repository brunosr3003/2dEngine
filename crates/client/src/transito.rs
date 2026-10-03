//! Kōgen-tō's TRAFFIC: decorative cars on the real avenues and on the
//! elevated expressway. Client only — the server never hears of them, they
//! block nobody, and each client sees its own.
//!
//! The paths are the centre lines of the avenues and of the Shuto deck,
//! baked from OpenStreetMap with the map (`tools/kogen-osm/rasterizar.py`,
//! `assets/kogen_ruas.bin`). A car drives one path end to end on its lane,
//! then a new one starts near the player: the traffic always surrounds
//! whoever is watching, and costs a few dozen matrices a frame.
//!
//! At night (`render3d::noite`) the bright, saturated headlights and
//! taillights glow, like the neon.

use crate::gpu_estatica::{MalhaEstatica, Programa};
use macroquad::models::{Mesh, Vertex};
use macroquad::prelude::*;
use shared::terreno::BLOCO;

/// How many cars drive round the player at once.
const CARROS: usize = 44;
/// How far from the player a car may start, in units.
const RAIO: f32 = 80.0;
/// Speed, in units per second (a city's pace next to a player's 5).
const VELOCIDADE: (f32, f32) = (7.0, 12.0);
/// The lane's offset from the centre line, in units (drive on the left, as
/// in Tokyo).
const FAIXA: f32 = 1.4;

struct Caminho {
    /// On the expressway deck (height from the deck) or on the street.
    elevado: bool,
    pontos: Vec<Vec2>,
    /// Running length at each point.
    acumulado: Vec<f32>,
}

impl Caminho {
    fn comprimento(&self) -> f32 {
        *self.acumulado.last().unwrap_or(&0.0)
    }

    /// The point and direction at distance `s` along the path.
    fn em(&self, s: f32) -> (Vec2, Vec2) {
        let i = self.acumulado.partition_point(|&a| a <= s).clamp(1, self.pontos.len() - 1);
        let (a, b) = (self.pontos[i - 1], self.pontos[i]);
        let l = (self.acumulado[i] - self.acumulado[i - 1]).max(1e-4);
        let t = ((s - self.acumulado[i - 1]) / l).clamp(0.0, 1.0);
        (a.lerp(b, t), (b - a).normalize_or_zero())
    }
}

struct Carro {
    caminho: usize,
    s: f32,
    /// +1 along the path's direction, -1 against it.
    sentido: f32,
    velocidade: f32,
    modelo: usize,
}

pub struct Transito {
    caminhos: Vec<Caminho>,
    carros: Vec<Carro>,
    modelos: Vec<Vec<MalhaEstatica>>,
    semente: u32,
}

fn le_caminhos() -> Vec<Caminho> {
    let b: &[u8] = include_bytes!("../../../assets/kogen_ruas.bin");
    if b.len() < 8 || &b[0..4] != b"KRUA" {
        return Vec::new();
    }
    let n = u32::from_le_bytes([b[4], b[5], b[6], b[7]]) as usize;
    let mut i = 8;
    let mut caminhos = Vec::with_capacity(n);
    for _ in 0..n {
        if i + 3 > b.len() {
            break;
        }
        let tipo = b[i];
        let m = u16::from_le_bytes([b[i + 1], b[i + 2]]) as usize;
        i += 3;
        let mut pontos = Vec::with_capacity(m);
        for _ in 0..m {
            if i + 4 > b.len() {
                break;
            }
            let x = i16::from_le_bytes([b[i], b[i + 1]]) as f32 * BLOCO;
            let z = i16::from_le_bytes([b[i + 2], b[i + 3]]) as f32 * BLOCO;
            pontos.push(vec2(x, z));
            i += 4;
        }
        let mut acumulado = vec![0.0];
        for w in pontos.windows(2) {
            acumulado.push(acumulado.last().unwrap() + w[0].distance(w[1]));
        }
        if pontos.len() >= 2 && *acumulado.last().unwrap() > 12.0 {
            caminhos.push(Caminho { elevado: tipo == 2, pontos, acumulado });
        }
    }
    caminhos
}

/// One voxel-ish car: a body, a dark glass cabin, headlights in front and
/// taillights behind. Along +x, centred on the origin, wheels on y = 0.
fn malha_do_carro(cor: [u8; 4]) -> Mesh {
    let mut verts: Vec<Vertex> = Vec::new();
    let mut idx: Vec<u16> = Vec::new();
    let mut caixa = |min: Vec3, max: Vec3, c: [u8; 4]| {
        let tons = [1.0f32, 0.55, 0.8, 0.7, 0.9, 0.75];
        let faces: [[Vec3; 4]; 6] = [
            // top, bottom
            [vec3(min.x, max.y, min.z), vec3(min.x, max.y, max.z), vec3(max.x, max.y, max.z), vec3(max.x, max.y, min.z)],
            [vec3(min.x, min.y, min.z), vec3(max.x, min.y, min.z), vec3(max.x, min.y, max.z), vec3(min.x, min.y, max.z)],
            // +x, -x
            [vec3(max.x, min.y, min.z), vec3(max.x, max.y, min.z), vec3(max.x, max.y, max.z), vec3(max.x, min.y, max.z)],
            [vec3(min.x, min.y, min.z), vec3(min.x, min.y, max.z), vec3(min.x, max.y, max.z), vec3(min.x, max.y, min.z)],
            // +z, -z
            [vec3(min.x, min.y, max.z), vec3(max.x, min.y, max.z), vec3(max.x, max.y, max.z), vec3(min.x, max.y, max.z)],
            [vec3(min.x, min.y, min.z), vec3(min.x, max.y, min.z), vec3(max.x, max.y, min.z), vec3(max.x, min.y, min.z)],
        ];
        for (f, t) in faces.iter().zip(tons) {
            let b = verts.len() as u16;
            let cc = [(c[0] as f32 * t) as u8, (c[1] as f32 * t) as u8, (c[2] as f32 * t) as u8, 255];
            for p in f {
                verts.push(Vertex { position: *p, uv: vec2(0.0, 0.0), color: cc, normal: Vec4::ZERO });
            }
            idx.extend_from_slice(&[b, b + 1, b + 2, b, b + 2, b + 3]);
        }
    };
    let (c, l, a) = (1.2f32, 0.55f32, 0.42f32); // half length, half width, body height
    caixa(vec3(-c, 0.12, -l), vec3(c, a, l), cor);
    caixa(vec3(-c * 0.45, a, -l * 0.85), vec3(c * 0.5, a + 0.34, l * 0.85), [40, 52, 76, 255]);
    // Headlights (pale yellow, saturated enough to glow) and taillights (red).
    for z in [-l * 0.7, l * 0.55] {
        caixa(vec3(c, 0.22, z), vec3(c + 0.06, 0.34, z + 0.15), [255, 232, 120, 255]);
        caixa(vec3(-c - 0.06, 0.22, z), vec3(-c, 0.34, z + 0.15), [255, 40, 40, 255]);
    }
    // Wheels.
    for x in [-c * 0.6, c * 0.6] {
        for z in [-l - 0.02, l - 0.1] {
            caixa(vec3(x - 0.18, 0.0, z), vec3(x + 0.18, 0.24, z + 0.12), [20, 20, 24, 255]);
        }
    }
    Mesh { vertices: verts, indices: idx, texture: None }
}

impl Transito {
    pub fn novo() -> Self {
        let cores: [[u8; 4]; 7] = [
            [220, 222, 228, 255],
            [30, 32, 38, 255],
            [200, 40, 50, 255],
            [40, 90, 200, 255],
            [250, 200, 40, 255], // the taxi
            [120, 124, 132, 255],
            [60, 200, 220, 255],
        ];
        let modelos = cores.iter().map(|c| vec![MalhaEstatica::nova(malha_do_carro(*c))]).collect();
        Self { caminhos: le_caminhos(), carros: Vec::new(), modelos, semente: 0x5EED_CA25 }
    }

    /// The middle of the longest ground avenue (for the previews).
    pub fn meio_de_avenida(&self) -> Option<Vec2> {
        let c = self.caminhos.iter().filter(|c| !c.elevado).max_by(|a, b| a.comprimento().total_cmp(&b.comprimento()))?;
        Some(c.em(c.comprimento() * 0.5).0)
    }

    fn sorteio(&mut self) -> f32 {
        self.semente ^= self.semente << 13;
        self.semente ^= self.semente >> 17;
        self.semente ^= self.semente << 5;
        (self.semente >> 8) as f32 / (1u32 << 24) as f32
    }

    /// A new car on a path that passes near `perto`, or `None`.
    fn novo_carro(&mut self, perto: Vec2) -> Option<Carro> {
        for _ in 0..24 {
            let k = (self.sorteio() * self.caminhos.len() as f32) as usize % self.caminhos.len().max(1);
            let total = self.caminhos.get(k)?.comprimento();
            let s = self.sorteio() * total;
            let (p, _) = self.caminhos[k].em(s);
            if p.distance(perto) > RAIO {
                continue;
            }
            let sentido = if self.sorteio() < 0.5 { 1.0 } else { -1.0 };
            let velocidade = VELOCIDADE.0 + (VELOCIDADE.1 - VELOCIDADE.0) * self.sorteio();
            let modelo = (self.sorteio() * self.modelos.len() as f32) as usize % self.modelos.len();
            return Some(Carro { caminho: k, s, sentido, velocidade, modelo });
        }
        None
    }

    /// Moves the cars and draws them. `chao(x, z)` is the street's height.
    pub fn desenha(&mut self, perto: Vec2, dt: f32, chao: &dyn Fn(f32, f32) -> f32) {
        if self.caminhos.is_empty() {
            return;
        }
        // Advance; a car past its path's end or too far away gives its place.
        let mut i = 0;
        while i < self.carros.len() {
            let c = &mut self.carros[i];
            c.s += c.sentido * c.velocidade * dt;
            let cam = &self.caminhos[c.caminho];
            let fora = c.s < 0.0 || c.s > cam.comprimento() || cam.em(c.s).0.distance(perto) > RAIO * 1.4;
            if fora {
                self.carros.swap_remove(i);
            } else {
                i += 1;
            }
        }
        let mut tentativas = 0;
        while self.carros.len() < CARROS && tentativas < CARROS {
            tentativas += 1;
            if let Some(c) = self.novo_carro(perto) {
                self.carros.push(c);
            }
        }
        for c in &self.carros {
            let cam = &self.caminhos[c.caminho];
            let (p, dir) = cam.em(c.s);
            let dir = dir * c.sentido;
            // Drive on the left: the lane is to the left of the direction.
            let esquerda = vec2(dir.y, -dir.x);
            let q = p + esquerda * FAIXA;
            let y = if cam.elevado {
                crate::terreno::deck_altura(q.x, q.y).or_else(|| crate::terreno::deck_altura(p.x, p.y)).unwrap_or_else(|| chao(q.x, q.y))
            } else {
                chao(q.x, q.y)
            };
            let yaw = (-dir.y).atan2(dir.x);
            let modelo = Mat4::from_translation(vec3(q.x, y, q.y)) * Mat4::from_rotation_y(yaw);
            crate::gpu_estatica::desenha_com_modelo(
                Programa::Solido { recorte: Vec3::ZERO, recorte_z: 0.0 },
                self.modelos[c.modelo].iter(),
                modelo,
            );
        }
    }
}
