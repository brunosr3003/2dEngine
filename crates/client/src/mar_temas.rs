//! THE NEW SEA DOCKS' THEMES (16/11/2026; docs/NAVEGACAO.md, "Toda ilha no
//! mar"). The owner: "I want to be able to sail to any island in the game".
//! Skyreach, Kōgen-tō, Abyssia and the Magic Island have no land port, so
//! each got a sea dock of its own look (`shared::mar::Feitio`):
//!
//! * **Skyreach** (`Celeste`): marble shores and cloud banks at the water,
//!   FLOATING ISLES overhead, a waterfall pouring off one of them into the
//!   sea, and a sky-lift at the quay rising into the floating pier above;
//! * **Kōgen-tō** (`Neon`): a concrete harbour behind a sea wall, a skyline
//!   of towers with neon bands and blinking roof lights, container cranes and
//!   stacks on the quay, a concrete pier with neon signs and a signal mast;
//! * **Abyssia** (`Abissal`): a coral atoll ringing a lagoon where a glowing
//!   WHIRLPOOL turns, coral spires with glowing tips, a diving bell on its
//!   crane at the quay, a pier of brass and grating;
//! * **the Magic Island** (`Magica`): a lush isle with a crown of giant
//!   glowing crystals, an enchanted pier under a rune arch, and a crystal
//!   spire for a lighthouse.
//!
//! # The phone budget
//!
//! The same pattern as the four port islands: the far island is the
//! low-poly height mesh (`mar_cena::constroi_lod`) PLUS one low-poly mesh of
//! the theme's landmarks (`malha_de_longe`: floating isles, towers, cranes,
//! crystals, coral spires — a few hundred flat-shaded quads, built once,
//! always drawn when in the fog's reach, since they stand where no chunk
//! covers them); the near detail is the dock's voxel models (`pecas`, built
//! once, drawn within `mar_cena::ALCANCE_DA_DOCA`). Per frame only the
//! lights: halos, the waterfall's ribbon and the whirlpool's spiral (a few
//! hundred vertices).

use macroquad::models::{Mesh, Vertex};
use macroquad::prelude::*;
use shared::mar::{self, Feitio, IlhaVista};
use shared::terreno::Material;

use crate::mar_cena::mq;
use crate::vegetacao::{malha, Modelo, Volume, VOX};

/// Is this one of the new themes (the islands without a land port)?
pub fn e_tema(f: Feitio) -> bool {
    matches!(f, Feitio::Celeste | Feitio::Neon | Feitio::Abissal | Feitio::Magica)
}

/// Height of the dock's BEACON (the lighthouse's equivalent) over the sea:
/// the lamp on top of the floating pier, the signal mast's light, the
/// diving-bell crane's lamp, the crystal spire's tip.
pub fn altura_da_lampada(f: Feitio) -> f32 {
    match f {
        Feitio::Celeste => 43.5,
        Feitio::Neon => 23.6,
        Feitio::Abissal => 16.4,
        Feitio::Magica => 22.0,
        _ => crate::mar_cena::LAMPADA_Y,
    }
}

/// The colour of the dock's beacon, beam and pier lamps.
pub fn cor_da_luz(f: Feitio) -> Vec3 {
    match f {
        Feitio::Celeste => vec3(1.0, 0.88, 0.45),
        Feitio::Neon => vec3(1.0, 0.32, 0.72),
        Feitio::Abissal => vec3(0.35, 1.0, 0.95),
        Feitio::Magica => vec3(0.86, 0.5, 1.0),
        _ => vec3(1.0, 0.86, 0.5),
    }
}

// ───────────────────────────── the theme's layout ─────────────────────────────

/// A floating isle (Skyreach): centre, radius, height of its top.
#[derive(Debug, Clone, Copy)]
pub struct Ilhota {
    pub c: Vec2,
    pub r: f32,
    pub topo: f32,
}

/// A tower of Kōgen-tō's skyline: centre, half footprint, height, neon.
#[derive(Debug, Clone, Copy)]
pub struct Torre {
    pub c: Vec2,
    pub meia: Vec2,
    pub h: f32,
    pub neon: usize,
}

/// A crystal (Magic Island) or a coral spire (Abyssia): centre, radius,
/// height over its ground.
#[derive(Debug, Clone, Copy)]
pub struct Pico {
    pub c: Vec2,
    pub r: f32,
    pub h: f32,
    pub chao: f32,
}

/// Where a theme's landmarks stand (built once per island, read by the far
/// mesh and the dock's lights).
#[derive(Debug, Clone, Default)]
pub struct Tema {
    pub ilhotas: Vec<Ilhota>,
    pub torres: Vec<Torre>,
    pub picos: Vec<Pico>,
    /// The waterfall: from the isle's rim down to the sea.
    pub cachoeira: Option<(Vec3, Vec3)>,
    /// The whirlpool's eye, on the water.
    pub redemoinho: Option<Vec3>,
    /// Extra lights: (where, colour, phase; a negative phase BLINKS).
    pub luzes: Vec<(Vec3, Vec3, f32)>,
}

fn h01(a: i32, b: i32, s: u32) -> f32 {
    let mut h = (a as u32).wrapping_mul(0x9E37_79B1) ^ (b as u32).wrapping_mul(0x85EB_CA77) ^ s.wrapping_mul(0x27D4_EB2F);
    h ^= h >> 15;
    h = h.wrapping_mul(0x2C1B_3C6D);
    h ^= h >> 12;
    (h & 0xFFFF) as f32 / 65535.0
}

fn sg(v: Vec2) -> ::glam::Vec2 {
    ::glam::Vec2::new(v.x, v.y)
}

/// Land at `p`, at least `margem` in from the coast, off the pier and quay.
fn em_terra(p: Vec2, margem: f32) -> bool {
    mar::terra_em(sg(p)).is_some_and(|t| t.dentro > margem && !t.cais && t.molhe < 0.01)
}

/// Clear of the dock: the pier, the quay and the way in stay open.
fn longe_do_cais(i: &IlhaVista, p: Vec2, folga: f32) -> bool {
    let (ao_longo, lado) = i.no_cais(sg(p));
    !(ao_longo > -46.0 && lado.abs() < folga)
}

fn chao(p: Vec2) -> f32 {
    mar::altura(p.x, p.y).max(0.0)
}

/// The theme of island `i` (empty for the four port islands).
pub fn tema(i: &IlhaVista) -> Tema {
    let (c, r) = (mq(i.centro), i.raio);
    let (e, l) = (mq(i.eixo()), mq(i.lado()));
    let s = i.ilha as u32 * 7919 + 17;
    let mut t = Tema::default();
    match i.feitio {
        Feitio::Celeste => {
            let f = mq(i.farol());
            t.ilhotas = vec![
                // The floating PIER the sky-lift climbs to, over the quay.
                Ilhota { c: f - e * 2.0, r: 15.0, topo: 40.0 },
                Ilhota { c: c - e * (r * 0.15), r: 46.0, topo: 72.0 },
                // The waterfall's isle, overhanging the sea.
                Ilhota { c: c - l * (r * 0.82), r: 28.0, topo: 56.0 },
                Ilhota { c: c + l * (r * 0.35) - e * (r * 0.45), r: 30.0, topo: 92.0 },
                Ilhota { c: c - e * (r * 0.62) - l * (r * 0.3), r: 22.0, topo: 48.0 },
            ];
            let q = t.ilhotas[2];
            let borda = q.c - l * (q.r * 0.95);
            t.cachoeira = Some((vec3(borda.x, q.topo - 1.5, borda.y), vec3(borda.x, 0.0, borda.y)));
            for k in [1usize, 3] {
                let q = t.ilhotas[k];
                t.luzes.push((vec3(q.c.x, q.topo + q.r * 0.1 + 1.5, q.c.y), vec3(1.0, 0.9, 0.55), k as f32));
            }
        }
        Feitio::Neon => {
            let (k0, k1) = (((c.x - r) / 30.0).floor() as i32, ((c.x + r) / 30.0).ceil() as i32);
            let (j0, j1) = (((c.y - r) / 30.0).floor() as i32, ((c.y + r) / 30.0).ceil() as i32);
            for kz in j0..=j1 {
                for kx in k0..=k1 {
                    let p = vec2(kx as f32 * 30.0 + 18.5, kz as f32 * 30.0 + 18.5);
                    if !em_terra(p, 14.0) || !longe_do_cais(i, p, 34.0) {
                        continue;
                    }
                    let a = h01(kx, kz, s);
                    let perto = 1.0 - (p.distance(c) / r).clamp(0.0, 1.0);
                    let h = 14.0 + 38.0 * a * a + 46.0 * perto * perto;
                    let meia = vec2(5.0 + 5.0 * h01(kx, kz, s ^ 3), 5.0 + 5.0 * h01(kx, kz, s ^ 5));
                    let neon = (h01(kx, kz, s ^ 9) * 3.0) as usize;
                    t.torres.push(Torre { c: p, meia, h, neon });
                }
            }
            // The roof lights blink (aircraft warning), and read the
            // skyline at night and through the haze.
            for (n, tr) in t.torres.iter().enumerate().filter(|(_, tr)| tr.h > 34.0) {
                let y = chao(tr.c) + tr.h + 1.2;
                t.luzes.push((vec3(tr.c.x, y, tr.c.y), vec3(1.0, 0.18, 0.12), -(n as f32 * 0.37 + 0.1)));
            }
        }
        Feitio::Abissal => {
            t.redemoinho = Some(vec3(c.x, crate::agua::ALTURA_DA_AGUA, c.y));
            for k in 0..10 {
                let a = k as f32 / 10.0 * std::f32::consts::TAU + i.fase;
                let costa = i.costa(a);
                let raio = costa * (mar::FRACAO_DA_LAGOA + (1.0 - mar::FRACAO_DA_LAGOA) * (0.45 + 0.2 * h01(k, 1, s)));
                let p = c + vec2(a.cos(), a.sin()) * raio;
                if !em_terra(p, 3.0) || !longe_do_cais(i, p, 30.0) {
                    continue;
                }
                let pico = Pico { c: p, r: 2.0 + 1.6 * h01(k, 2, s), h: 9.0 + 13.0 * h01(k, 3, s), chao: chao(p) };
                t.luzes.push((vec3(p.x, pico.chao + pico.h + 0.6, p.y), vec3(0.35, 1.0, 0.95), k as f32 * 0.9));
                t.picos.push(pico);
            }
        }
        Feitio::Magica => {
            let mut cands = vec![(c, 6.5, 46.0)];
            for k in 0..14 {
                let a = h01(k, 7, s) * std::f32::consts::TAU;
                let d = r * (0.16 + 0.42 * h01(k, 8, s));
                cands.push((c + vec2(a.cos(), a.sin()) * d, 2.6 + 3.2 * h01(k, 9, s), 16.0 + 24.0 * h01(k, 10, s)));
            }
            for (p, raio, h) in cands {
                if t.picos.len() >= 8 || !em_terra(p, 22.0) || !longe_do_cais(i, p, 34.0) {
                    continue;
                }
                if t.picos.iter().any(|q| q.c.distance(p) < q.r + raio + 6.0) {
                    continue;
                }
                let pico = Pico { c: p, r: raio, h, chao: chao(p) };
                t.luzes.push((vec3(p.x, pico.chao + h * 0.86, p.y), vec3(0.82, 0.45, 1.0), t.picos.len() as f32 * 1.1));
                t.picos.push(pico);
            }
        }
        _ => {}
    }
    t
}

// ───────────────────────────── the far landmarks ─────────────────────────────

/// A flat-shaded mesh in the making (u16 indices: split every 60k vertices).
struct Obra {
    verts: Vec<Vertex>,
    idx: Vec<u16>,
    prontas: Vec<Mesh>,
}

const LUZ: Vec3 = Vec3::new(0.36, 0.82, 0.44);

fn tinta(m: Material, luz: f32) -> [u8; 4] {
    let (r, g, b) = m.rgb();
    let k = luz.clamp(0.0, 1.2);
    [(r as f32 * k).min(255.0) as u8, (g as f32 * k).min(255.0) as u8, (b as f32 * k).min(255.0) as u8, 255]
}

/// Lit by the sea's sun the way the voxel faces are (top 1.0, sides less).
fn luz_de(fora: Vec3) -> f32 {
    let n = fora.normalize_or_zero();
    0.6 + 0.4 * n.dot(LUZ.normalize()).max(0.0) + 0.1 * n.y.max(0.0)
}

impl Obra {
    fn nova() -> Self {
        Obra { verts: Vec::new(), idx: Vec::new(), prontas: Vec::new() }
    }

    fn cabe(&mut self, n: usize) {
        if self.verts.len() + n > 60_000 {
            let v = std::mem::take(&mut self.verts);
            let i = std::mem::take(&mut self.idx);
            self.prontas.push(Mesh { vertices: v, indices: i, texture: None });
        }
    }

    fn v(&mut self, p: Vec3, c: [u8; 4]) -> u16 {
        self.verts.push(Vertex { position: p, uv: Vec2::ZERO, color: c, normal: Vec4::ZERO });
        (self.verts.len() - 1) as u16
    }

    /// A triangle facing `fora` (wound so the world shader keeps it).
    fn tri(&mut self, a: Vec3, b: Vec3, c: Vec3, fora: Vec3, m: Material) {
        self.cabe(3);
        let cor = tinta(m, luz_de(fora));
        let (b, c) = if (b - a).cross(c - a).dot(fora) >= 0.0 { (b, c) } else { (c, b) };
        let (ia, ib, ic) = (self.v(a, cor), self.v(b, cor), self.v(c, cor));
        self.idx.extend_from_slice(&[ia, ib, ic]);
    }

    fn quad(&mut self, q: [Vec3; 4], fora: Vec3, m: Material) {
        self.cabe(4);
        let cor = tinta(m, luz_de(fora));
        let q = if (q[1] - q[0]).cross(q[2] - q[0]).dot(fora) >= 0.0 { q } else { [q[0], q[3], q[2], q[1]] };
        let b = self.verts.len() as u16;
        for p in q {
            self.v(p, cor);
        }
        self.idx.extend_from_slice(&[b, b + 1, b + 2, b, b + 2, b + 3]);
    }

    /// A box from `a` to `b` (no floor).
    fn caixa(&mut self, a: Vec3, b: Vec3, m: Material) {
        let (x0, y0, z0, x1, y1, z1) = (a.x, a.y, a.z, b.x, b.y, b.z);
        self.quad([vec3(x0, y1, z0), vec3(x1, y1, z0), vec3(x1, y1, z1), vec3(x0, y1, z1)], Vec3::Y, m);
        self.quad([vec3(x0, y0, z0), vec3(x1, y0, z0), vec3(x1, y1, z0), vec3(x0, y1, z0)], -Vec3::Z, m);
        self.quad([vec3(x0, y0, z1), vec3(x1, y0, z1), vec3(x1, y1, z1), vec3(x0, y1, z1)], Vec3::Z, m);
        self.quad([vec3(x0, y0, z0), vec3(x0, y0, z1), vec3(x0, y1, z1), vec3(x0, y1, z0)], -Vec3::X, m);
        self.quad([vec3(x1, y0, z0), vec3(x1, y0, z1), vec3(x1, y1, z1), vec3(x1, y1, z0)], Vec3::X, m);
    }

    /// An upright prism of `lados` sides from `y0` to `y1`, capped with a
    /// point `ponta` above `y1` (0 = flat).
    #[allow(clippy::too_many_arguments)]
    fn prisma(&mut self, c: Vec2, y0: f32, y1: f32, r: f32, lados: usize, lado: Material, topo: Material, ponta: f32) {
        let pt = |k: usize, y: f32| {
            let a = k as f32 / lados as f32 * std::f32::consts::TAU;
            vec3(c.x + a.cos() * r, y, c.y + a.sin() * r)
        };
        for k in 0..lados {
            let (a, b) = (pt(k, y0), pt(k + 1, y0));
            let (a1, b1) = (pt(k, y1), pt(k + 1, y1));
            let meio = (a + b) * 0.5;
            let fora = vec3(meio.x - c.x, 0.0, meio.z - c.y);
            self.quad([a, b, b1, a1], fora, lado);
            let tip = vec3(c.x, y1 + ponta, c.y);
            let fora_topo = if ponta > 0.0 { fora.normalize_or_zero() * ponta + Vec3::Y * r } else { Vec3::Y };
            self.tri(a1, b1, tip, fora_topo, topo);
        }
    }

    /// A FLOATING ISLE: a grassy dome on a marble rim over a jagged rock
    /// cone hanging down to a point.
    fn ilhota(&mut self, q: &Ilhota, s: u32) {
        const N: usize = 14;
        let jit = |k: usize, sal: u32| 0.82 + 0.3 * h01(k as i32, sal as i32, s);
        let anel = |k: usize, r: f32, y: f32, sal: u32| {
            let a = k as f32 / N as f32 * std::f32::consts::TAU;
            let rr = r * jit(k % N, sal);
            vec3(q.c.x + a.cos() * rr, y, q.c.y + a.sin() * rr)
        };
        let topo_c = vec3(q.c.x, q.topo + q.r * 0.1, q.c.y);
        let tip = vec3(q.c.x, q.topo - q.r * 1.15, q.c.y);
        for k in 0..N {
            let k1 = k + 1;
            let (b0, b1) = (anel(k, q.r * 0.97, q.topo, 1), anel(k1 % N, q.r * 0.97, q.topo, 1));
            let (a0, a1) = (anel(k, q.r, q.topo - 1.6, 1), anel(k1 % N, q.r, q.topo - 1.6, 1));
            let (c0, c1) = (anel(k, q.r * 0.6, q.topo - q.r * 0.55, 2), anel(k1 % N, q.r * 0.6, q.topo - q.r * 0.55, 2));
            let out = {
                let m = (a0 + a1) * 0.5;
                vec3(m.x - q.c.x, 0.0, m.z - q.c.y)
            };
            self.tri(topo_c, b0, b1, Vec3::Y, Material::GramaCeleste);
            self.quad([a0, a1, b1, b0], out, Material::Marmore);
            self.quad([c0, c1, a1, a0], out.normalize_or_zero() - Vec3::Y * 0.5, Material::MarmoreSombra);
            self.tri(c0, c1, tip, out.normalize_or_zero() - Vec3::Y, Material::RochaEscura);
        }
    }

    fn fim(mut self) -> Vec<Mesh> {
        if !self.idx.is_empty() {
            self.prontas.push(Mesh { vertices: self.verts, indices: self.idx, texture: None });
        }
        self.prontas
    }
}

/// THE FAR LANDMARKS of a themed island, in world space: floating isles,
/// the skyline and the cranes, the coral spires, the crystals. Empty for
/// the four port islands.
pub fn malha_de_longe(i: &IlhaVista, t: &Tema) -> Vec<Mesh> {
    let mut o = Obra::nova();
    let s = i.ilha as u32 * 31 + 5;
    match i.feitio {
        Feitio::Celeste => {
            for q in &t.ilhotas {
                o.ilhota(q, s);
            }
            // A few golden trees on the big isles' tops: a trunk and a cone.
            for q in t.ilhotas.iter().filter(|q| q.r > 25.0) {
                for k in 0..4 {
                    let a = k as f32 * 1.7 + q.topo;
                    let p = q.c + vec2(a.cos(), a.sin()) * q.r * 0.45;
                    let y = q.topo + q.r * 0.06;
                    o.prisma(p, y, y + 2.5, 0.5, 4, Material::TroncoBranco, Material::TroncoBranco, 0.0);
                    o.prisma(p, y + 2.0, y + 4.0, 2.6, 6, Material::FolhaDourada, Material::FolhaDourada, 3.2);
                }
            }
        }
        Feitio::Neon => {
            let neons = [Material::NeonRosa, Material::NeonCiano, Material::NeonAmarelo];
            for tr in &t.torres {
                let y0 = chao(tr.c) - 0.5;
                let (a, b) = (tr.c - tr.meia, tr.c + tr.meia);
                let corpo = if tr.neon == 1 { Material::Vidro } else { Material::ConcretoEscuro };
                o.caixa(vec3(a.x, y0, a.y), vec3(b.x, y0 + tr.h, b.y), corpo);
                // Neon bands round the tower and a crown band at the top.
                let mut y = y0 + 7.0;
                while y < y0 + tr.h - 3.0 {
                    o.caixa(vec3(a.x - 0.15, y, a.y - 0.15), vec3(b.x + 0.15, y + 0.6, b.y + 0.15), neons[tr.neon]);
                    y += 9.0;
                }
                o.caixa(vec3(a.x - 0.2, y0 + tr.h - 1.4, a.y - 0.2), vec3(b.x + 0.2, y0 + tr.h - 0.6, b.y + 0.2), neons[(tr.neon + 1) % 3]);
                if tr.h > 34.0 {
                    o.prisma(tr.c, y0 + tr.h, y0 + tr.h + 1.0, 0.25, 4, Material::RochaEscura, Material::RochaEscura, 0.0);
                }
            }
            // The two container cranes on the quay, read from afar: legs, a
            // top girder and the boom over the water beside the pier.
            for (x, z) in [(7.0f32, -12.0f32), (-14.0, -14.0)] {
                let p = |lx: f32, ly: f32, lz: f32| crate::mar_cena::no_mundo(i, vec3(lx, ly, lz));
                for (dx, dz) in [(0.0, 0.0), (5.0, 0.0), (0.0, 5.0), (5.0, 5.0)] {
                    let (a, b) = (p(x + dx - 0.4, 1.0, z + dz - 0.4), p(x + dx + 0.4, 14.0, z + dz + 0.4));
                    o.caixa(a.min(b), a.max(b), Material::PetalaVermelha);
                }
                let (a, b) = (p(x - 0.5, 14.0, z - 0.5), p(x + 5.5, 15.4, z + 5.5));
                o.caixa(a.min(b), a.max(b), Material::NeonAmarelo);
                let (a, b) = (p(x + 2.0, 13.4, z - 4.0), p(x + 3.0, 14.4, z + 17.5));
                o.caixa(a.min(b), a.max(b), Material::PetalaVermelha);
            }
        }
        Feitio::Abissal => {
            for (k, pc) in t.picos.iter().enumerate() {
                o.prisma(pc.c, pc.chao - 0.5, pc.chao + pc.h * 0.7, pc.r, 6, Material::Coral, Material::Coral, 0.0);
                o.prisma(pc.c, pc.chao + pc.h * 0.7, pc.chao + pc.h, pc.r * 0.55, 6, Material::Coral, Material::CristalAzul, pc.r * 0.8);
                // A branch.
                let d = vec2((k as f32 * 2.1).cos(), (k as f32 * 2.1).sin()) * pc.r * 1.4;
                o.prisma(pc.c + d, pc.chao - 0.5, pc.chao + pc.h * 0.45, pc.r * 0.5, 5, Material::Coral, Material::CristalAzul, pc.r * 0.6);
            }
        }
        Feitio::Magica => {
            for (k, pc) in t.picos.iter().enumerate() {
                let (corpo, ponta) = if k % 3 == 1 { (Material::CristalAzul, Material::CristalCeu) } else { (Material::CristalRoxo, Material::PetalaRoxa) };
                o.prisma(pc.c, pc.chao - 1.0, pc.chao + pc.h * 0.78, pc.r, 6, corpo, ponta, pc.h * 0.22);
                // Two smaller shards leaning against it.
                for j in 0..2 {
                    let a = k as f32 * 1.3 + j as f32 * 2.6;
                    let d = vec2(a.cos(), a.sin()) * pc.r * 1.25;
                    o.prisma(pc.c + d, pc.chao - 1.0, pc.chao + pc.h * 0.35, pc.r * 0.45, 6, corpo, ponta, pc.h * 0.12);
                }
            }
        }
        _ => {}
    }
    o.fim()
}

// ───────────────────────────── the dock's voxels ─────────────────────────────

fn vx(u: f32) -> i32 {
    (u / VOX).round() as i32
}

fn bx(v: &mut Volume, a: [i32; 3], b: [i32; 3], m: Material) {
    for y in a[1]..b[1] {
        for z in a[2]..b[2] {
            for x in a[0]..b[0] {
                v.poe(x, y, z, m);
            }
        }
    }
}

/// A disc of radius `r` voxels at `(cx, cz)`, from `y0` up `h`.
fn disco(v: &mut Volume, cx: i32, cz: i32, r: f32, y0: i32, h: i32, m: Material) {
    let ri = r.ceil() as i32;
    for y in y0..y0 + h {
        for z in -ri..=ri {
            for x in -ri..=ri {
                if (x * x + z * z) as f32 <= r * r {
                    v.poe(cx + x, y, cz + z, m);
                }
            }
        }
    }
}

/// The pier's materials, per theme.
struct Paleta {
    tabuas: [Material; 3],
    borda: Material,
    estaca: Material,
    poste: Material,
    luz: Material,
    carga: [Material; 2],
    /// A sign panel on each lamp post (Kōgen-tō's neon).
    placa: Option<Material>,
}

fn paleta(f: Feitio) -> Paleta {
    match f {
        Feitio::Celeste => Paleta {
            tabuas: [Material::Marmore, Material::MarmoreSombra, Material::Marmore],
            borda: Material::FolhaDourada,
            estaca: Material::TroncoBranco,
            poste: Material::TroncoBranco,
            luz: Material::CristalCeu,
            carga: [Material::FolhaDourada, Material::Marmore],
            placa: None,
        },
        Feitio::Neon => Paleta {
            tabuas: [Material::Concreto, Material::ConcretoEscuro, Material::Concreto],
            borda: Material::NeonAmarelo,
            estaca: Material::Concreto,
            poste: Material::ConcretoEscuro,
            luz: Material::NeonRosa,
            carga: [Material::PetalaVermelha, Material::CristalAzul],
            placa: Some(Material::NeonCiano),
        },
        Feitio::Abissal => Paleta {
            tabuas: [Material::ConcretoEscuro, Material::RochaAbissal, Material::ConcretoEscuro],
            borda: Material::Arenito,
            estaca: Material::RochaAbissal,
            poste: Material::Arenito,
            luz: Material::NeonCiano,
            carga: [Material::Coral, Material::Arenito],
            placa: None,
        },
        _ => Paleta {
            tabuas: [Material::Marmore, Material::Marmore, Material::CristalAzul],
            borda: Material::CristalRoxo,
            estaca: Material::TroncoBranco,
            poste: Material::Marmore,
            luz: Material::CristalRoxo,
            carga: [Material::CristalCeu, Material::PetalaRoxa],
            placa: None,
        },
    }
}

/// The themed PIER, in the pier's frame (the same shape as the planks of
/// `mar_cena::pier`, so the terrain's pier and its T head fit it).
fn pier(p: &Paleta) -> Modelo {
    let mut v = Volume::novo([-26, -8, -16], [52, 34, 124]);
    let (z0, z1) = (vx(-3.0), vx(mar::CAIS_COMP));
    let meia = vx(mar::CAIS_LARG * 0.5);
    let cab_z = vx(mar::CAIS_COMP - mar::CABECA_COMP);
    let cab_meia = vx(mar::CABECA_LARG * 0.5);
    let deck = 3;
    for z in z0..z1 {
        let larg = if z >= cab_z { cab_meia } else { meia };
        let tabua = p.tabuas[z.div_euclid(2).rem_euclid(3) as usize];
        for x in -larg..larg {
            let m = if x == -larg || x == larg - 1 { p.borda } else { tabua };
            v.poe(x, deck, z, m);
        }
        for x in [-larg, larg - 1] {
            v.poe(x, deck - 1, z, p.estaca);
        }
    }
    let mut z = z0 + 4;
    while z < z1 {
        let larg = if z >= cab_z { cab_meia } else { meia };
        for x in [-larg - 1, larg - 1] {
            bx(&mut v, [x, -8, z], [x + 2, deck + 2, z + 2], p.estaca);
        }
        z += 12;
    }
    // Lamp posts down the pier and at the head, with their lights.
    for (k, zl) in [30, 62, z1 - 3].into_iter().enumerate() {
        for s in [-1, 1] {
            let larg = if zl >= cab_z { cab_meia } else { meia };
            let x = if s < 0 { -larg + 1 } else { larg - 2 };
            bx(&mut v, [x, deck + 1, zl], [x + 1, deck + 12, zl + 1], p.poste);
            bx(&mut v, [x - 1, deck + 12, zl - 1], [x + 2, deck + 14, zl + 2], p.luz);
            bx(&mut v, [x - 1, deck + 14, zl - 1], [x + 2, deck + 15, zl + 2], p.poste);
            if let Some(placa) = p.placa {
                if k < 2 {
                    let xs = if s < 0 { x - 1 } else { x + 1 };
                    bx(&mut v, [xs, deck + 5, zl - 3], [xs + 1, deck + 10, zl + 4], if s < 0 { placa } else { p.luz });
                }
            }
        }
    }
    // Cargo on the head.
    for (k, (x, zz, h)) in [(cab_meia - 8, cab_z + 3, 3), (cab_meia - 12, cab_z + 2, 3), (-cab_meia + 4, cab_z + 4, 4), (-cab_meia + 8, cab_z + 6, 4)].into_iter().enumerate() {
        bx(&mut v, [x, deck + 1, zz], [x + 3, deck + 1 + h, zz + 3], p.carga[k / 2]);
    }
    malha(&v)
}

/// The QUAY's two flag poles (the same spots as the port islands', where
/// `mar_cena::Doca` waves the island's flags).
fn mastros(v: &mut Volume, b: i32, haste: Material, pe: Material) {
    for x in [12, -12] {
        bx(v, [x - 1, b, -6], [x + 2, b + 1, -3], pe);
        bx(v, [x, b, -5], [x + 1, b + 28, -4], haste);
    }
}

/// SKYREACH's SKY-LIFT at the lighthouse's spot: a marble footing, a white
/// lattice tower rising into the floating pier, its cabin halfway up with
/// lit windows, the ropes and the gold wheel-house.
fn elevador() -> Modelo {
    let mut v = Volume::novo([-10, 0, -10], [21, 130, 21]);
    let b = vx(mar::ALTURA_DO_MOLHE);
    disco(&mut v, 0, 0, 8.4, b, 3, Material::Marmore);
    disco(&mut v, 0, 0, 8.4, b + 2, 1, Material::FolhaDourada);
    let topo = b + 118;
    for (x, z) in [(-6, -6), (4, -6), (-6, 4), (4, 4)] {
        bx(&mut v, [x, b + 3, z], [x + 2, topo, z + 2], Material::TroncoBranco);
    }
    let mut y = b + 14;
    while y < topo {
        for (a, c) in [([-6, y, -6], [6, y + 1, -5]), ([-6, y, 5], [6, y + 1, 6]), ([-6, y, -6], [-5, y + 1, 6]), ([5, y, -6], [6, y + 1, 6])] {
            bx(&mut v, a, c, Material::TroncoBranco);
        }
        y += 16;
    }
    // The cabin, halfway up.
    let cab = b + 44;
    for y in cab..cab + 12 {
        for z in -4i32..4 {
            for x in -4i32..4 {
                let borda = x == -4 || x == 3 || z == -4 || z == 3;
                if !borda && y != cab && y != cab + 11 {
                    continue;
                }
                let janela = borda && (4..9).contains(&(y - cab)) && (x.abs() < 2 || z.abs() < 2);
                v.poe(x, y, z, if janela { Material::JanelaAcesa } else if y == cab + 11 { Material::FolhaDourada } else { Material::Marmore });
            }
        }
    }
    for x in [-1, 1] {
        bx(&mut v, [x, cab + 12, 0], [x + 1, topo, 1], Material::RochaEscura);
        bx(&mut v, [x, b + 3, 0], [x + 1, cab, 1], Material::RochaEscura);
    }
    bx(&mut v, [-5, topo, -5], [5, topo + 6, 5], Material::FolhaDourada);
    malha(&v)
}

/// Skyreach's quay: a marble gazebo with a gold dome where the shed would
/// be, cloud puffs, gold planters, the flag poles.
fn cais_celeste() -> Modelo {
    let mut v = Volume::novo([-22, 0, -90], [70, 40, 92]);
    let b = vx(mar::ALTURA_DO_MOLHE);
    let (cx, cz) = (27, -68);
    disco(&mut v, cx, cz, 11.0, b, 1, Material::Marmore);
    for k in 0..6 {
        let a = k as f32 / 6.0 * std::f32::consts::TAU;
        let (x, z) = (cx + (a.cos() * 9.0) as i32, cz + (a.sin() * 9.0) as i32);
        bx(&mut v, [x, b + 1, z], [x + 2, b + 16, z + 2], Material::Marmore);
    }
    for (k, r) in [11.0f32, 10.0, 8.5, 6.5, 4.0, 1.5].into_iter().enumerate() {
        disco(&mut v, cx, cz, r, b + 16 + k as i32, 1, if k == 0 { Material::Marmore } else { Material::FolhaDourada });
    }
    for (x, z) in [(14, -16), (-16, -14), (-18, -30)] {
        bx(&mut v, [x, b, z], [x + 4, b + 3, z + 4], Material::Marmore);
        disco(&mut v, x + 2, z + 2, 2.5, b + 3, 4, Material::FolhaDourada);
    }
    for (x, z, r) in [(-14, -60, 5.0f32), (-10, -66, 4.0), (40, -30, 4.5)] {
        disco(&mut v, x, z, r, b, 3, Material::Nuvem);
        disco(&mut v, x + 1, z, r * 0.7, b + 3, 2, Material::Nuvem);
    }
    mastros(&mut v, b, Material::TroncoBranco, Material::Marmore);
    malha(&v)
}

/// KŌGEN-TŌ's SIGNAL MAST at the lighthouse's spot: a concrete footing, a
/// steel lattice with neon rings, a lit sign and the light at the top.
fn mastro_neon() -> Modelo {
    let mut v = Volume::novo([-10, 0, -10], [21, 100, 21]);
    let b = vx(mar::ALTURA_DO_MOLHE);
    disco(&mut v, 0, 0, 8.0, b, 4, Material::Concreto);
    disco(&mut v, 0, 0, 8.0, b + 3, 1, Material::NeonAmarelo);
    let topo = b + 86;
    for (x, z) in [(-3, -3), (2, -3), (-3, 2), (2, 2)] {
        bx(&mut v, [x, b + 4, z], [x + 1, topo, z + 1], Material::ConcretoEscuro);
    }
    let mut y = b + 10;
    let mut k = 0;
    while y < topo {
        let m = if k % 2 == 0 { Material::NeonRosa } else { Material::NeonCiano };
        for z in -4i32..=4 {
            for x in -4i32..=4 {
                if x.abs() == 4 || z.abs() == 4 {
                    v.poe(x, y, z, m);
                }
            }
        }
        y += 14;
        k += 1;
    }
    // The sign: a lit panel facing the pier.
    bx(&mut v, [-8, b + 36, 3], [8, b + 48, 4], Material::NeonCiano);
    bx(&mut v, [-7, b + 38, 4], [7, b + 46, 5], Material::NeonRosa);
    bx(&mut v, [-3, topo, -3], [3, topo + 6, 3], Material::JanelaAcesa);
    bx(&mut v, [-3, topo + 6, -3], [3, topo + 7, 3], Material::ConcretoEscuro);
    malha(&v)
}

/// Kōgen-tō's quay: a gantry crane (red, with a yellow girder) whose boom
/// reaches over the water beside the pier, stacks of containers, the flag
/// poles.
fn cais_neon() -> Modelo {
    let mut v = Volume::novo([-60, 0, -90], [112, 66, 172]);
    let b = vx(mar::ALTURA_DO_MOLHE);
    for (x0, z0) in [(28, -48), (-56, -56)] {
        for (dx, dz) in [(0, 0), (18, 0), (0, 18), (18, 18)] {
            bx(&mut v, [x0 + dx, b, z0 + dz], [x0 + dx + 3, b + 52, z0 + dz + 3], Material::PetalaVermelha);
        }
        bx(&mut v, [x0 - 1, b + 52, z0 - 1], [x0 + 22, b + 57, z0 + 22], Material::NeonAmarelo);
        bx(&mut v, [x0 + 8, b + 50, z0 - 16], [x0 + 12, b + 54, z0 + 70], Material::PetalaVermelha);
        bx(&mut v, [x0 + 7, b + 40, z0 + 40], [x0 + 13, b + 46, z0 + 46], Material::ConcretoEscuro);
        bx(&mut v, [x0 + 9, b + 46, z0 + 42], [x0 + 11, b + 50, z0 + 44], Material::RochaEscura);
    }
    let cores = [Material::PetalaVermelha, Material::CristalAzul, Material::NeonAmarelo, Material::GramaEscura, Material::Arenito, Material::Concreto];
    let mut n = 0usize;
    for (x0, z0) in [(-22, -88), (-22, -58), (14, -88), (36, -86)] {
        let altura = 1 + (n % 3);
        for k in 0..altura {
            let m = cores[(n * 7 + k * 3) % cores.len()];
            let y = b + k as i32 * 9;
            bx(&mut v, [x0, y, z0], [x0 + 10, y + 9, z0 + 24], m);
            for z in (z0..z0 + 24).step_by(3) {
                bx(&mut v, [x0 - 1, y + 1, z], [x0 + 11, y + 8, z + 1], Material::RochaEscura);
            }
        }
        n += 1;
    }
    mastros(&mut v, b, Material::ConcretoEscuro, Material::Concreto);
    malha(&v)
}

/// ABYSSIA's DIVING BELL at the lighthouse's spot: a coral-stone platform, a
/// brass crane and the bell hanging from its jib with glowing portholes, a
/// lamp on top.
fn sino() -> Modelo {
    let mut v = Volume::novo([-12, 0, -12], [40, 72, 25]);
    let b = vx(mar::ALTURA_DO_MOLHE);
    disco(&mut v, 0, 0, 9.0, b, 3, Material::RochaAbissal);
    disco(&mut v, 0, 0, 9.0, b + 2, 1, Material::Coral);
    let topo = b + 58;
    bx(&mut v, [-2, b + 3, -2], [2, topo, 2], Material::Arenito);
    bx(&mut v, [-2, topo, -2], [24, topo + 3, 2], Material::Arenito);
    bx(&mut v, [-3, topo - 12, -1], [12, topo - 11, 1], Material::Arenito);
    // The cable and the BELL.
    bx(&mut v, [19, topo - 12, 0], [20, topo, 1], Material::RochaEscura);
    let base_sino = topo - 30;
    for y in 0..18 {
        let r = 7.5 - (y as f32 / 18.0).powf(1.6) * 4.5;
        let m = if y % 6 == 0 { Material::RochaEscura } else { Material::Arenito };
        disco(&mut v, 19, 0, r, base_sino + y, 1, m);
    }
    for (x, z) in [(26, 0), (12, 0), (19, 7), (19, -7)] {
        bx(&mut v, [x - 1, base_sino + 5, z - 1], [x + 1, base_sino + 8, z + 1], Material::NeonCiano);
    }
    bx(&mut v, [-3, topo + 3, -3], [3, topo + 7, 3], Material::NeonCiano);
    malha(&v)
}

/// Abyssia's quay: coral pillars with glowing tips, shell lanterns, kelp,
/// the flag poles.
fn cais_abissal() -> Modelo {
    let mut v = Volume::novo([-30, 0, -90], [80, 44, 92]);
    let b = vx(mar::ALTURA_DO_MOLHE);
    for (k, (x, z)) in [(20, -60), (34, -40), (-24, -44), (-20, -76), (40, -76)].into_iter().enumerate() {
        let h = 18 + (k as i32 * 7) % 14;
        disco(&mut v, x, z, 3.0, b, h, Material::Coral);
        disco(&mut v, x + 3, z, 1.6, b + h / 2, h / 2, Material::Coral);
        disco(&mut v, x, z, 2.0, b + h, 3, Material::CristalAzul);
        disco(&mut v, x + 3, z, 1.2, b + h, 2, Material::CristalAzul);
    }
    for (x, z) in [(14, -14), (-16, -12), (-16, -28)] {
        bx(&mut v, [x, b, z], [x + 1, b + 12, z + 1], Material::Arenito);
        disco(&mut v, x, z, 2.0, b + 12, 3, Material::NeonCiano);
    }
    for (x, z) in [(6, -34), (-8, -56), (26, -24)] {
        for k in 0..14 {
            v.poe(x + (k / 5) % 2, b + k, z, Material::Alga);
        }
    }
    mastros(&mut v, b, Material::Arenito, Material::RochaAbissal);
    malha(&v)
}

/// The MAGIC ISLAND's lighthouse: a rune circle and a great violet crystal
/// with blue shards round it, glowing at the tip.
fn espigao_de_cristal() -> Modelo {
    let mut v = Volume::novo([-12, 0, -12], [25, 96, 25]);
    let b = vx(mar::ALTURA_DO_MOLHE);
    disco(&mut v, 0, 0, 9.5, b, 2, Material::Marmore);
    for k in 0..8 {
        let a = k as f32 / 8.0 * std::f32::consts::TAU;
        let (x, z) = ((a.cos() * 8.0) as i32, (a.sin() * 8.0) as i32);
        bx(&mut v, [x, b + 2, z], [x + 1, b + 3, z + 1], Material::CristalAzul);
    }
    let alto = 84;
    for y in 0..alto {
        let t = y as f32 / alto as f32;
        let r = if t < 0.7 { 5.2 - t * 1.5 } else { (1.0 - t) / 0.3 * 4.15 };
        let m = if y > alto - 10 { Material::JanelaAcesa } else if (y / 4) % 5 == 0 { Material::PetalaRoxa } else { Material::CristalRoxo };
        disco(&mut v, 0, 0, r.max(0.6), b + 2 + y, 1, m);
    }
    for (x, z, h) in [(7, 3, 30), (-6, 5, 22), (2, -7, 36), (-5, -5, 18)] {
        for y in 0..h {
            let r = 2.2 * (1.0 - y as f32 / h as f32) + 0.6;
            disco(&mut v, x, z, r, b + 2 + y, 1, if y > h - 4 { Material::CristalCeu } else { Material::CristalAzul });
        }
    }
    malha(&v)
}

/// The Magic Island's quay: a rune arch over the pier's root, rune stones,
/// crystal clusters, the flag poles.
fn cais_magico() -> Modelo {
    let mut v = Volume::novo([-30, 0, -90], [80, 48, 92]);
    let b = vx(mar::ALTURA_DO_MOLHE);
    let meia = vx(mar::CAIS_LARG * 0.5) + 3;
    for x in [-meia - 3, meia] {
        bx(&mut v, [x, b, -4], [x + 3, b + 36, -1], Material::Marmore);
        bx(&mut v, [x, b + 10, -4], [x + 3, b + 12, -1], Material::CristalAzul);
    }
    bx(&mut v, [-meia - 4, b + 36, -5], [meia + 4, b + 40, 0], Material::Marmore);
    bx(&mut v, [-2, b + 40, -4], [2, b + 44, -1], Material::CristalRoxo);
    for (x, z) in [(20, -24), (30, -50), (-20, -34), (-24, -64), (8, -74)] {
        bx(&mut v, [x, b, z], [x + 4, b + 8, z + 3], Material::Marmore);
        bx(&mut v, [x + 1, b + 8, z], [x + 3, b + 10, z + 3], Material::CristalAzul);
    }
    for (x, z) in [(40, -30), (-26, -50), (18, -84)] {
        for (dx, dz, h) in [(0, 0, 14), (2, 1, 9), (-1, 2, 7)] {
            for y in 0..h {
                let r = 1.6 * (1.0 - y as f32 / h as f32) + 0.4;
                disco(&mut v, x + dx, z + dz, r, b + y, 1, if h > 10 { Material::CristalRoxo } else { Material::CristalCeu });
            }
        }
    }
    mastros(&mut v, b, Material::Marmore, Material::Marmore);
    malha(&v)
}

/// The themed dock's pieces in the pier frame: (model, offset). `None` for
/// the four port islands (`mar_cena::Doca` keeps theirs).
pub fn pecas(f: Feitio) -> Option<Vec<(Modelo, Vec3)>> {
    if !e_tema(f) {
        return None;
    }
    let farol = vec3(-10.5, 0.0, -12.0);
    let p = paleta(f);
    let (marco, cais) = match f {
        Feitio::Celeste => (elevador(), cais_celeste()),
        Feitio::Neon => (mastro_neon(), cais_neon()),
        Feitio::Abissal => (sino(), cais_abissal()),
        _ => (espigao_de_cristal(), cais_magico()),
    };
    Some(vec![(pier(&p), Vec3::ZERO), (marco, farol), (cais, Vec3::ZERO)])
}

// ───────────────────────────── per-frame lights ─────────────────────────────

fn cor8(c: Vec3, a: f32) -> [u8; 4] {
    [
        (c.x.clamp(0.0, 1.0) * 255.0) as u8,
        (c.y.clamp(0.0, 1.0) * 255.0) as u8,
        (c.z.clamp(0.0, 1.0) * 255.0) as u8,
        (a.clamp(0.0, 1.0) * 255.0) as u8,
    ]
}

fn vert(p: Vec3, c: [u8; 4]) -> Vertex {
    Vertex { position: p, uv: Vec2::ZERO, color: c, normal: Vec4::ZERO }
}

/// THE WATERFALL off Skyreach's isle: a ribbon facing the eye from the rim
/// to the sea, in bands that run down, and a foam glow where it lands.
/// Additive.
pub fn cachoeira(verts: &mut Vec<Vertex>, idx: &mut Vec<u16>, olho: Vec3, topo: Vec3, fundo: Vec3, tempo: f32) {
    let f = vec3(olho.x - topo.x, 0.0, olho.z - topo.z).normalize_or_zero();
    let lado = vec3(-f.z, 0.0, f.x);
    const N: usize = 16;
    let cor = vec3(0.82, 0.92, 1.0);
    let base = verts.len() as u16;
    for k in 0..=N {
        let t = k as f32 / N as f32;
        let y = topo.y + (fundo.y - topo.y) * t;
        let larg = 2.6 + 2.4 * t;
        let faixa = 0.55 + 0.45 * ((t * 14.0 - tempo * 3.2).sin() * 0.5 + 0.5);
        let a = 0.42 * faixa * (1.0 - 0.3 * t);
        let c = vec3(topo.x, y, topo.z);
        verts.push(vert(c - lado * larg, cor8(cor, a * 0.4)));
        verts.push(vert(c, cor8(cor, a)));
        verts.push(vert(c + lado * larg, cor8(cor, a * 0.4)));
    }
    for k in 0..N as u16 {
        let (a, b) = (base + k * 3, base + (k + 1) * 3);
        idx.extend_from_slice(&[a, b, b + 1, a, b + 1, a + 1, a + 1, b + 1, b + 2, a + 1, b + 2, a + 2]);
    }
    // The foam where it lands: a flat glowing disc that breathes.
    let b0 = verts.len() as u16;
    let y = crate::agua::ALTURA_DA_AGUA + crate::agua::AMPLITUDE_MAX * 0.75 + 0.08;
    let r = 7.0 + 0.8 * (tempo * 2.0).sin();
    verts.push(vert(vec3(fundo.x, y, fundo.z), cor8(cor, 0.55)));
    for k in 0..16 {
        let a = k as f32 / 16.0 * std::f32::consts::TAU;
        verts.push(vert(vec3(fundo.x + a.cos() * r, y, fundo.z + a.sin() * r), cor8(cor, 0.0)));
    }
    for k in 0..16u16 {
        idx.extend_from_slice(&[b0, b0 + 1 + k, b0 + 1 + (k + 1) % 16]);
    }
}

/// THE WHIRLPOOL in Abyssia's lagoon: four glowing arms spiralling into the
/// eye, turning, and a light column over it — the dock's beacon from afar.
/// Additive.
pub fn redemoinho(verts: &mut Vec<Vertex>, idx: &mut Vec<u16>, olho: Vec3, c: Vec3, tempo: f32, raio: f32) {
    let y = crate::agua::ALTURA_DA_AGUA + crate::agua::AMPLITUDE_MAX * 0.75 + 0.1;
    let cor = vec3(0.3, 1.0, 0.92);
    for braco in 0..4 {
        let fase = braco as f32 * std::f32::consts::FRAC_PI_2 - tempo * 0.9;
        let base = verts.len() as u16;
        const N: usize = 24;
        for k in 0..=N {
            let t = k as f32 / N as f32;
            let a = fase + t * 5.5;
            let r = raio * (1.0 - t) + 1.0;
            let larg = 0.6 + 2.2 * (1.0 - t);
            let d = vec3(a.cos(), 0.0, a.sin());
            let p = vec3(c.x, y, c.z) + d * r;
            let al = 0.5 * t.min(1.0 - t * 0.6) + 0.1;
            verts.push(vert(p - d * larg, cor8(cor, al * 0.2)));
            verts.push(vert(p + d * larg, cor8(cor, al)));
        }
        for k in 0..N as u16 {
            let (a0, b0, a1, b1) = (base + k * 2, base + k * 2 + 1, base + k * 2 + 2, base + k * 2 + 3);
            idx.extend_from_slice(&[a0, b0, b1, a0, b1, a1]);
        }
    }
    // The eye's glow and the column of light over it.
    let b0 = verts.len() as u16;
    verts.push(vert(vec3(c.x, y, c.z), cor8(vec3(0.6, 1.0, 1.0), 0.8)));
    for k in 0..16 {
        let a = k as f32 / 16.0 * std::f32::consts::TAU;
        verts.push(vert(vec3(c.x + a.cos() * 6.0, y, c.z + a.sin() * 6.0), cor8(cor, 0.0)));
    }
    for k in 0..16u16 {
        idx.extend_from_slice(&[b0, b0 + 1 + k, b0 + 1 + (k + 1) % 16]);
    }
    let f = vec3(olho.x - c.x, 0.0, olho.z - c.z).normalize_or_zero();
    let lado = vec3(-f.z, 0.0, f.x);
    let alto = 60.0;
    let pulso = 0.8 + 0.2 * (tempo * 1.7).sin();
    let b1 = verts.len() as u16;
    verts.push(vert(vec3(c.x, y, c.z) - lado * 3.0, cor8(cor, 0.0)));
    verts.push(vert(vec3(c.x, y, c.z), cor8(cor, 0.35 * pulso)));
    verts.push(vert(vec3(c.x, y, c.z) + lado * 3.0, cor8(cor, 0.0)));
    verts.push(vert(vec3(c.x, y + alto, c.z) - lado * 1.2, cor8(cor, 0.0)));
    verts.push(vert(vec3(c.x, y + alto, c.z), cor8(cor, 0.0)));
    verts.push(vert(vec3(c.x, y + alto, c.z) + lado * 1.2, cor8(cor, 0.0)));
    idx.extend_from_slice(&[b1, b1 + 1, b1 + 4, b1, b1 + 4, b1 + 3, b1 + 1, b1 + 2, b1 + 5, b1 + 1, b1 + 5, b1 + 4]);
}

#[cfg(test)]
mod testes {
    use super::*;

    /// THE NEW DOCKS' BUDGET, for the phone: the far landmarks stay a few
    /// thousand vertices and the dock's voxels within the port docks' size.
    /// Prints the counts (the report and docs/NAVEGACAO.md).
    #[test]
    fn as_docas_novas_sao_baratas() {
        for i in mar::ilhas().iter().filter(|i| e_tema(i.feitio)) {
            let t = tema(i);
            let longe: usize = malha_de_longe(i, &t).iter().map(|m| m.vertices.len()).sum();
            let doca: usize = pecas(i.feitio).unwrap().iter().map(|(m, _)| m.verts.len()).sum();
            println!("{}: landmarks {longe} vertices, dock {doca} vertices, {} lights", i.nome, t.luzes.len());
            assert!(longe > 100, "{}: no landmarks", i.nome);
            assert!(longe < 8_000, "{}: {longe} landmark vertices", i.nome);
            assert!(doca < 40_000, "{}: {doca} dock vertices", i.nome);
            for (m, _) in pecas(i.feitio).unwrap() {
                assert!(m.verts.len() < 65_000, "a piece overflows u16");
            }
        }
    }

    /// Every theme has its landmarks where they belong: Skyreach's isles over
    /// the island (and the waterfall over the WATER), Kōgen-tō's skyline on
    /// the concrete, the atoll's whirlpool in its lagoon, the Magic Island's
    /// crystals on land — and none of them in the way in.
    #[test]
    fn cada_tema_tem_os_seus_marcos() {
        let v = |z: &str| mar::vista_da_zona(z).unwrap();
        let ceu = tema(v(shared::celeste::ZONA));
        assert!(ceu.ilhotas.len() >= 4);
        let (_, fundo) = ceu.cachoeira.expect("Skyreach's waterfall");
        assert!(mar::navegavel(fundo.x, fundo.z), "the waterfall falls on land at {fundo:?}");
        let kogen = tema(v(shared::kogen::ZONA));
        assert!(kogen.torres.len() >= 8, "{} towers", kogen.torres.len());
        assert!(kogen.torres.iter().any(|t| t.h > 60.0), "no tall tower in the skyline");
        let atol = v(shared::abissal::ZONA);
        let a = tema(atol);
        let olho = a.redemoinho.expect("the whirlpool");
        assert!(vec2(olho.x, olho.z).distance(mq(atol.centro)) < 0.01);
        assert!(a.picos.len() >= 5);
        let magica = tema(v(shared::magica::ZONA));
        assert!(magica.picos.len() >= 4, "{} crystals", magica.picos.len());
        for (z, t) in [(shared::kogen::ZONA, &kogen), (shared::magica::ZONA, &magica)] {
            let i = v(z);
            for p in t.torres.iter().map(|t| t.c).chain(t.picos.iter().map(|p| p.c)) {
                assert!(!mar::navegavel(p.x, p.y), "{z}: a landmark in the water at {p:?}");
                assert!(longe_do_cais(i, p, 20.0), "{z}: a landmark on the dock");
            }
        }
    }
}
