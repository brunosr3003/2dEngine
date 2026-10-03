//! Kōgen-tō's TRAFFIC: decorative cars on the real avenues and on the
//! elevated expressway. Client only — the server never hears of them, they
//! block nobody, and each client sees its own.
//!
//! The paths are the centre lines of the avenues and of the Shuto deck,
//! baked from OpenStreetMap with the map (`tools/kogen-osm/rasterizar.py`,
//! `assets/kogen_ruas.bin`), joined at their ends. A fixed fleet drives the
//! whole network all the time — on through junctions, mostly straight on,
//! U-turning at dead ends — and only the cars near the camera are drawn.
//!
//! At night (`render3d::noite`) the bright, saturated headlights and
//! taillights glow, like the neon.

use crate::gpu_estatica::{MalhaEstatica, Programa};
use macroquad::models::{Mesh, Vertex};
use macroquad::prelude::*;
use shared::terreno::BLOCO;

/// The FLEET: this many cars drive the whole city, all the time. They are
/// born once, spread over every avenue, and never vanish — the owner asked
/// for "fewer and constant driving in the city instead of disappearing".
const CARROS: usize = 48;
/// Only the cars this close to the camera are drawn (the rest keep driving).
const RAIO_VISTO: f32 = 150.0;
/// How fast a car turns its body, in radians per second: it eases round a
/// bend instead of snapping at every vertex of the OSM line.
const GIRO: f32 = 3.0;
/// Two path ends this close are the same junction (OSM cuts every avenue
/// into short ways: a car that died at each way's end vanished mid-street).
const EMENDA: f32 = 3.0;
/// Speed, in units per second (a city's pace next to a player's 5).
const VELOCIDADE: (f32, f32) = (5.0, 8.0);
/// The lane's offset from the centre line, in units (drive on the left, as
/// in Tokyo).
const FAIXA: f32 = 1.6;

struct Caminho {
    /// On the expressway deck (height from the deck) or on the street.
    elevado: bool,
    pontos: Vec<Vec2>,
    /// Running length at each point.
    acumulado: Vec<f32>,
    /// The paths that continue from each end: (path, joined at ITS start).
    seguintes: [Vec<(usize, bool)>; 2],
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
    /// The body's heading, eased towards the road's (`GIRO`).
    yaw: f32,
    /// Where it was drawn last frame (the lane offset eases too).
    pos: Option<Vec2>,
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
        if pontos.len() >= 2 && *acumulado.last().unwrap() > 4.0 {
            caminhos.push(Caminho { elevado: tipo == 2, pontos, acumulado, seguintes: [Vec::new(), Vec::new()] });
        }
    }
    // Join the ends: each end lists the paths of the same level that start
    // or end within `EMENDA` of it.
    let pontas: Vec<[Vec2; 2]> = caminhos.iter().map(|c| [c.pontos[0], *c.pontos.last().unwrap()]).collect();
    for i in 0..caminhos.len() {
        for lado in 0..2 {
            let p = pontas[i][lado];
            let mut v = Vec::new();
            for (j, pj) in pontas.iter().enumerate() {
                if j == i || caminhos[j].elevado != caminhos[i].elevado {
                    continue;
                }
                if pj[0].distance(p) < EMENDA {
                    v.push((j, true));
                } else if pj[1].distance(p) < EMENDA {
                    v.push((j, false));
                }
            }
            caminhos[i].seguintes[lado] = v;
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
    // A car at the CHARACTER's scale (the owner: wrong size): about as long
    // as two and a half characters are tall, a cabin at shoulder height.
    let (c, l, a) = (2.1f32, 0.9f32, 0.75f32); // half length, half width, body height
    caixa(vec3(-c, 0.22, -l), vec3(c, a, l), cor);
    caixa(vec3(-c * 0.45, a, -l * 0.85), vec3(c * 0.5, a + 0.55, l * 0.85), [40, 52, 76, 255]);
    // Headlights (pale yellow, saturated enough to glow) and taillights (red).
    for z in [-l * 0.75, l * 0.45] {
        caixa(vec3(c, 0.4, z), vec3(c + 0.08, 0.58, z + 0.28), [255, 232, 120, 255]);
        caixa(vec3(-c - 0.08, 0.4, z), vec3(-c, 0.58, z + 0.28), [255, 40, 40, 255]);
    }
    // Wheels.
    for x in [-c * 0.62, c * 0.62] {
        for z in [-l - 0.03, l - 0.17] {
            caixa(vec3(x - 0.32, 0.0, z), vec3(x + 0.32, 0.42, z + 0.2), [20, 20, 24, 255]);
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
        let mut t = Self { caminhos: le_caminhos(), carros: Vec::new(), modelos, semente: 0x5EED_CA25 };
        // The fleet, spread over the whole network, weighted by length.
        if !t.caminhos.is_empty() {
            for _ in 0..CARROS {
                let k = t.caminho_ao_acaso();
                let s = t.sorteio() * t.caminhos[k].comprimento();
                let sentido = if t.sorteio() < 0.5 { 1.0 } else { -1.0 };
                let velocidade = VELOCIDADE.0 + (VELOCIDADE.1 - VELOCIDADE.0) * t.sorteio();
                let modelo = (t.sorteio() * t.modelos.len() as f32) as usize % t.modelos.len();
                t.carros.push(Carro { caminho: k, s, sentido, velocidade, modelo, yaw: 0.0, pos: None });
            }
        }
        t
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

    /// A path at random, long ones more likely (the cars spread by road).
    fn caminho_ao_acaso(&mut self) -> usize {
        let total: f32 = self.caminhos.iter().map(|c| c.comprimento()).sum();
        let mut alvo = self.sorteio() * total;
        for (i, c) in self.caminhos.iter().enumerate() {
            alvo -= c.comprimento();
            if alvo <= 0.0 {
                return i;
            }
        }
        self.caminhos.len() - 1
    }

    /// Moves every car and draws the ones near `perto`. `chao(x, z)` is the
    /// street's height.
    pub fn desenha(&mut self, perto: Vec2, dt: f32, chao: &dyn Fn(f32, f32) -> f32) {
        if self.caminhos.is_empty() {
            return;
        }
        for i in 0..self.carros.len() {
            let sorte = self.sorteio();
            let c = &mut self.carros[i];
            c.s += c.sentido * c.velocidade * dt;
            // At a path's end: on into the path that goes on the STRAIGHTEST
            // (mostly — now and then a turn), or a U-turn at a dead end.
            for _ in 0..4 {
                let cam = &self.caminhos[c.caminho];
                let fim = cam.comprimento();
                let lado = if c.s > fim { 1 } else if c.s < 0.0 { 0 } else { break };
                let sobra = if lado == 1 { c.s - fim } else { -c.s };
                let rumo = cam.em(if lado == 1 { fim } else { 0.0 }).1 * c.sentido;
                let opcoes = &cam.seguintes[lado];
                if opcoes.is_empty() {
                    c.sentido = -c.sentido;
                    c.s = if lado == 1 { fim - sobra } else { sobra };
                    continue;
                }
                let saida = |&(j, no_inicio): &(usize, bool)| {
                    let o = &self.caminhos[j];
                    let d = if no_inicio { o.em(0.0).1 } else { -o.em(o.comprimento()).1 };
                    d.dot(rumo)
                };
                let reta = opcoes.iter().copied().max_by(|a, b| saida(a).total_cmp(&saida(b))).unwrap();
                let (j, no_inicio) = if sorte < 0.75 { reta } else { opcoes[(sorte * 97.0) as usize % opcoes.len()] };
                c.caminho = j;
                if no_inicio {
                    c.sentido = 1.0;
                    c.s = sobra;
                } else {
                    c.sentido = -1.0;
                    c.s = self.caminhos[j].comprimento() - sobra;
                }
            }
            let cam = &self.caminhos[c.caminho];
            c.s = c.s.clamp(0.0, cam.comprimento());
            let (p, dir) = cam.em(c.s);
            let dir = dir * c.sentido;
            // Drive on the left: the lane is to the left of the direction.
            let alvo = p + vec2(dir.y, -dir.x) * FAIXA;
            // Ease the body round bends, and the lane across junctions.
            let yaw_alvo = (-dir.y).atan2(dir.x);
            let mut dy = (yaw_alvo - c.yaw + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU) - std::f32::consts::PI;
            if c.pos.is_none() {
                dy = yaw_alvo - c.yaw;
            }
            c.yaw += dy.clamp(-GIRO * dt, GIRO * dt);
            if c.pos.is_none() || dy.abs() > 2.5 {
                c.yaw = yaw_alvo;
            }
            let q = match c.pos {
                Some(antes) if antes.distance(alvo) < 6.0 => antes.lerp(alvo, (dt * 8.0).min(1.0)),
                _ => alvo,
            };
            c.pos = Some(q);
        }
        for c in &self.carros {
            let Some(q) = c.pos else { continue };
            if q.distance(perto) > RAIO_VISTO {
                continue;
            }
            let cam = &self.caminhos[c.caminho];
            let (p, _) = cam.em(c.s);
            // On the deck the height comes from the CENTRE line: the lane
            // offset can fall past a narrow deck's edge.
            let y = if cam.elevado {
                crate::terreno::deck_altura(p.x, p.y).or_else(|| crate::terreno::deck_altura(q.x, q.y)).unwrap_or_else(|| chao(p.x, p.y))
            } else {
                chao(p.x, p.y)
            };
            let modelo = Mat4::from_translation(vec3(q.x, y, q.y)) * Mat4::from_rotation_y(c.yaw);
            crate::gpu_estatica::desenha_com_modelo(
                Programa::Solido { recorte: Vec3::ZERO, recorte_z: 0.0 },
                self.modelos[c.modelo].iter(),
                modelo,
            );
        }
    }
}
