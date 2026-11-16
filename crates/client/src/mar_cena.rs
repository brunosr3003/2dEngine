//! THE OPEN SEA'S SCENERY (visual remake, 16/11/2026; docs/NAVEGACAO.md,
//! "Remake visual").
//!
//! The owner, on the first sea: "the miniature islands are ugly, it doesn't
//! show the dock, you can't even understand it, the sea itself is very
//! ugly". What the sea now draws besides the terrain and the water:
//!
//! * **the islands from afar** (`Lod`): one low-poly height mesh per island,
//!   built once from the same `shared::mar` relief at 3-5 u a cell, ALWAYS
//!   below the voxel terrain (each vertex takes the lowest ground around
//!   it), so near the ship the real chunks cover it and past the loaded
//!   chunks it is the island's silhouette in the haze;
//! * **the islands' trees and rocks** (`props`, `prop_da_coluna`): small
//!   low-poly voxel models per palette, baked into the terrain chunks (zero
//!   cost per frame), thinned by the Graphics ground-cover setting;
//! * **the dock** (`Doca`): a pier of planks on pilings with a T head,
//!   lamps, crates and barrels, a stone quay with a shed, a red-and-white
//!   lighthouse whose lamp and beam are seen across the sea, flags in the
//!   island's colour, two boats moored, and a glowing ring on the water
//!   where you dock; plus a floating "Bosque · Dock" label and edge markers
//!   (`rotulos`);
//! * **the sky** (`desenha_ceu`): a gradient dome with the sun, the colour
//!   the water reflects (`ceu_e_sol`) and the fog fades into;
//! * **the floating wreckage and chests**, riding the swell (`agua::onda_em`).
//!
//! # The phone budget
//!
//! Static things are meshes built ONCE (islands, docks) or baked into the
//! chunks (trees), drawn through `gpu_estatica` in a handful of calls;
//! nothing static is rebuilt per frame. What moves per frame is tiny (a ring,
//! a few flags, beams: a few hundred vertices). Everything far is culled by
//! distance, and `qualidade` (from Graphics → Ground cover) thins it further.

use std::cell::RefCell;

use macroquad::models::{Mesh, Vertex};
use macroquad::prelude::*;
use crate::relogio::get_time;
use shared::mar::{self, Feitio, IlhaVista};
use shared::terreno::{Material, BLOCO};

use crate::gpu_estatica::{MalhaEstatica, Programa};
use crate::vegetacao::{malha, Modelo, Volume, VOX};

// ───────────────────────────── quality ─────────────────────────────

/// 0 low, 1 medium, 2 high — from Graphics → Ground cover (Sparse, Medium,
/// Full), the setting a phone player already turns down for frame rate.
pub fn qualidade() -> u8 {
    match crate::config_graficos::forracao() {
        0..=40 => 0,
        41..=80 => 1,
        _ => 2,
    }
}

/// Fog at sea: (start, solid sky), from the player. Much farther than on an
/// island (`config_graficos::neblina`): the open sea is cheap to draw past
/// the chunks (one ring of water and four island meshes), and seeing the
/// next island is what makes it a sea and not a fog bank.
pub fn neblina() -> (f32, f32) {
    let (inicio, _) = crate::config_graficos::neblina();
    let fim = match qualidade() {
        0 => 620.0,
        1 => 850.0,
        _ => 1100.0,
    };
    let previa = NEBLINA_DA_PREVIA.with(|n| n.get());
    if previa > 0.0 {
        return (inicio.max(110.0), previa);
    }
    (inicio.max(110.0), fim)
}

thread_local! {
    /// The preview's whole-sea shot pushes the fog out (0 = the game's).
    static NEBLINA_DA_PREVIA: std::cell::Cell<f32> = const { std::cell::Cell::new(0.0) };
}

// ───────────────────────────── sky and sun ─────────────────────────────

thread_local! {
    static ENTARDECER: std::cell::Cell<f32> = const { std::cell::Cell::new(0.0) };
    static OLHO: std::cell::Cell<Option<Vec3>> = const { std::cell::Cell::new(None) };
}

/// Where the eye is this frame (set by `agua::desenha`, which every 3D view
/// of the sea calls before its entities): the ships' level of detail.
pub fn define_olho(o: Vec3) {
    OLHO.with(|c| c.set(Some(o)));
}

pub fn olho() -> Option<Vec3> {
    OLHO.with(|c| c.get())
}

/// 0 = noon, 1 = dusk (the preview's "dusk" shot; the sea has no day cycle).
pub fn define_entardecer(f: f32) {
    ENTARDECER.with(|e| e.set(f.clamp(0.0, 1.0)));
}

pub fn entardecer() -> f32 {
    ENTARDECER.with(|e| e.get())
}

fn mistura3(a: Vec3, b: Vec3, t: f32) -> Vec3 {
    a + (b - a) * t
}

/// The horizon's colour (the same the world's fog fades to at noon:
/// `render3d::SOLIDO_FRAGMENTO`).
pub fn cor_do_horizonte() -> Vec3 {
    mistura3(vec3(0.588, 0.729, 0.839), vec3(0.97, 0.63, 0.43), entardecer())
}

fn cor_do_zenite() -> Vec3 {
    mistura3(vec3(0.27, 0.50, 0.80), vec3(0.20, 0.20, 0.42), entardecer())
}

/// Towards the sun. Ahead of the default camera (it looks down -z), so the
/// glitter path lies in front of the ship.
pub fn dir_do_sol() -> Vec3 {
    mistura3(vec3(0.22, 0.70, -0.68), vec3(0.30, 0.13, -0.95), entardecer()).normalize()
}

/// What the water shader reflects (`agua::FRAGMENTO`): (horizon rgb, dusk),
/// (sun direction, strength).
pub fn ceu_e_sol() -> ([f32; 4], [f32; 4]) {
    let c = cor_do_horizonte();
    let s = dir_do_sol();
    ([c.x, c.y, c.z, entardecer()], [s.x, s.y, s.z, 1.0])
}

/// The clear colour at sea: the horizon.
pub fn cor_de_fundo() -> Color {
    let c = cor_do_horizonte();
    Color::new(c.x, c.y, c.z, 1.0)
}

fn material(depth_write: bool, aditivo: bool) -> Material2 {
    use macroquad::miniquad::graphics::{BlendFactor, BlendState, BlendValue, Comparison, CullFace, Equation, PipelineParams};
    let destino = if aditivo {
        BlendFactor::One
    } else {
        BlendFactor::OneMinusValue(BlendValue::SourceAlpha)
    };
    load_material(
        ShaderSource::Glsl { vertex: BRILHO_VERTICE, fragment: BRILHO_FRAGMENTO },
        MaterialParams {
            pipeline_params: PipelineParams {
                cull_face: CullFace::Nothing,
                depth_test: Comparison::LessOrEqual,
                depth_write,
                color_blend: Some(BlendState::new(Equation::Add, BlendFactor::Value(BlendValue::SourceAlpha), destino)),
                ..Default::default()
            },
            ..Default::default()
        },
    )
    .expect("sea glow shader")
}

type Material2 = macroquad::material::Material;

const BRILHO_VERTICE: &str = r#"#version 100
attribute vec3 position;
attribute vec2 texcoord;
attribute vec4 color0;
varying lowp vec4 cor;
uniform mat4 Model;
uniform mat4 Projection;
void main() {
    gl_Position = Projection * Model * vec4(position, 1.0);
    cor = color0 / 255.0;
}"#;

const BRILHO_FRAGMENTO: &str = r#"#version 100
varying lowp vec4 cor;
void main() {
    gl_FragColor = cor;
}"#;

thread_local! {
    /// (additive glow, plain translucent): no depth write, no culling.
    static MATERIAIS: RefCell<Option<(Material2, Material2)>> = const { RefCell::new(None) };
}

fn com_material(aditivo: bool, f: impl FnOnce()) {
    MATERIAIS.with(|m| {
        let mut m = m.borrow_mut();
        let (a, b) = m.get_or_insert_with(|| (material(false, true), material(false, false)));
        gl_use_material(if aditivo { a } else { b });
    });
    f();
}

/// `shared`'s glam → macroquad's (two glam versions in the tree).
pub fn mq(v: ::glam::Vec2) -> Vec2 {
    vec2(v.x, v.y)
}

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

/// THE SKY: a dome round the eye, horizon colour at the rim, zenith blue on
/// top, a warm glow and a disc where the sun is. ~200 vertices, drawn after
/// the world with depth test so it only fills what nothing else covered.
pub fn desenha_ceu(cam: &Camera3D) {
    const ANEIS: usize = 7;
    const LADOS: usize = 28;
    // Beyond the sea's farthest ring (1,600 u), or the ring pokes through it.
    const R: f32 = 3000.0;
    let o = cam.position;
    let sol = dir_do_sol();
    let (h, z) = (cor_do_horizonte(), cor_do_zenite());
    let quente = mistura3(vec3(1.0, 0.95, 0.85), vec3(1.0, 0.55, 0.28), entardecer());
    let mut verts = Vec::with_capacity((ANEIS + 1) * LADOS + 40);
    let mut idx: Vec<u16> = Vec::new();
    for a in 0..=ANEIS {
        // From a little under the horizon (the sea's edge never shows a gap)
        // to straight up.
        let el = -0.08 + (a as f32 / ANEIS as f32).powf(1.6) * (std::f32::consts::FRAC_PI_2 + 0.08);
        for l in 0..LADOS {
            let az = l as f32 / LADOS as f32 * std::f32::consts::TAU;
            let d = vec3(el.cos() * az.cos(), el.sin(), el.cos() * az.sin());
            let t = (el.max(0.0) / 0.9).min(1.0).powf(0.7);
            let mut c = mistura3(h, z, t);
            let perto = d.dot(sol).max(0.0);
            c = mistura3(c, quente, perto.powf(6.0) * (0.35 + 0.45 * entardecer()));
            verts.push(vert(o + d * R, cor8(c, 1.0)));
        }
    }
    for a in 0..ANEIS {
        for l in 0..LADOS {
            let (l1, a1) = ((l + 1) % LADOS, a + 1);
            let i = |a: usize, l: usize| (a * LADOS + l) as u16;
            idx.extend_from_slice(&[i(a, l), i(a1, l), i(a1, l1), i(a, l), i(a1, l1), i(a, l1)]);
        }
    }
    // The sun's disc and its halo: a fan facing the eye.
    let base = verts.len() as u16;
    let c0 = o + sol * (R * 0.95);
    let (dx, dy) = {
        let dx = sol.cross(Vec3::Y).normalize_or_zero();
        (dx, dx.cross(sol).normalize_or_zero())
    };
    let raio = R * 0.05;
    verts.push(vert(c0, cor8(mistura3(vec3(1.0, 1.0, 0.95), quente, 0.4), 1.0)));
    for k in 0..16 {
        let a = k as f32 / 16.0 * std::f32::consts::TAU;
        verts.push(vert(c0 + (dx * a.cos() + dy * a.sin()) * raio, cor8(mistura3(vec3(1.0, 0.98, 0.9), quente, 0.6), 1.0)));
    }
    for k in 0..16u16 {
        idx.extend_from_slice(&[base, base + 1 + k, base + 1 + (k + 1) % 16]);
    }
    let m = Mesh { vertices: verts, indices: idx, texture: None };
    com_material(false, || draw_mesh(&m));
}

// ───────────────────────────── island props ─────────────────────────────

const P_COPA: usize = 0; // 3 round trees
const P_PINHO: usize = 3; // 2 green pines
const P_PINHO_NEVE: usize = 5; // 2 snowy pines
const P_PINHO_ESCURO: usize = 7; // 2 dark pines
const P_PEDRA: usize = 9; // 2 grey rocks
const P_ARENITO: usize = 11; // 2 sandstone rocks
const P_PEDRA_ESCURA: usize = 13; // 2 dark rocks
const P_CACTO: usize = 15; // 2 cacti
const P_MONTE_DE_NEVE: usize = 17;
const P_SECA: usize = 18;
// The new sea docks (16/11/2026, `mar_temas`).
const P_ARVORE_CELESTE: usize = 19; // 2 white-and-gold sky trees
const P_CRISTAL: usize = 21; // 3 crystal clusters: violet, blue, sky
const P_CORAL: usize = 24; // 2 coral fans
const P_ALGA: usize = 26; // kelp
const N_PROPS: usize = 27;

fn caixa_em(v: &mut Volume, a: [i32; 3], b: [i32; 3], m: Material) {
    for y in a[1]..b[1] {
        for z in a[2]..b[2] {
            for x in a[0]..b[0] {
                v.poe(x, y, z, m);
            }
        }
    }
}

/// A box centred on x/z: half sizes `hx`, `hz`, from `y0` up `h`.
fn bloco_c(v: &mut Volume, cx: i32, cz: i32, hx: i32, hz: i32, y0: i32, h: i32, m: Material) {
    caixa_em(v, [cx - hx, y0, cz - hz], [cx + hx + 1, y0 + h, cz + hz + 1], m);
}

/// The sea's island props: low-poly voxel trees and rocks, 30-120 quads
/// each (the land's trees run to hundreds: an island seen from a deck is
/// thousands of them).
pub fn props() -> Vec<Modelo> {
    let mut out = Vec::new();
    let novo = || Volume::novo([-5, 0, -5], [11, 18, 11]);
    // Round broadleaf: trunk and two or three stacked crowns.
    for k in 0..3 {
        let mut v = novo();
        bloco_c(&mut v, 0, 0, 0, 0, 0, 4 + k % 2, Material::Tronco);
        let (a, b) = [(Material::FolhaEscura, Material::Folha), (Material::Folha, Material::GramaClara), (Material::FolhaEscura, Material::GramaEscura)][k as usize];
        bloco_c(&mut v, 0, 0, 3, 3, 3 + k % 2, 3, a);
        bloco_c(&mut v, [0, 1, -1][k as usize], [0, -1, 1][k as usize], 2, 2, 6 + k % 2, 2, b);
        bloco_c(&mut v, [2, -2, 1][k as usize], [-1, 2, 2][k as usize], 1, 1, 4 + k % 2, 2, b);
        out.push(malha(&v));
    }
    // Pines: trunk and stepped tiers. Snowy: white caps; dark: the Plateau's.
    for (verde, neve) in [(Material::FolhaEscura, false), (Material::FolhaEscura, true), (Material::Alga, false)] {
        for k in 0..2 {
            let mut v = novo();
            let alto = 2 + k;
            bloco_c(&mut v, 0, 0, 0, 0, 0, alto + 1, Material::Tronco);
            for (n, (r, h)) in [(3, 2), (2, 2), (1, 2), (0, 2)].into_iter().enumerate() {
                let y = alto + n as i32 * 2;
                bloco_c(&mut v, 0, 0, r, r, y, h, verde);
                if neve {
                    bloco_c(&mut v, 0, 0, r, r, y + h - 1, 1, Material::Neve);
                }
            }
            out.push(malha(&v));
        }
    }
    // Rocks: a lump and a smaller one leaning on it.
    for m in [Material::Rocha, Material::Arenito, Material::RochaAbissal] {
        for k in 0..2 {
            let mut v = novo();
            bloco_c(&mut v, 0, 0, 2, 1 + k, 0, 2 + k, m);
            bloco_c(&mut v, 1, -1, 1, 1, 0, 3 + k, if m == Material::Rocha { Material::RochaEscura } else { m });
            bloco_c(&mut v, -2, 1, 1, 1, 0, 1, m);
            out.push(malha(&v));
        }
    }
    // Cacti.
    for k in 0..2 {
        let mut v = novo();
        bloco_c(&mut v, 0, 0, 0, 0, 0, 7 + k, Material::GramaEscura);
        bloco_c(&mut v, 1, 0, 0, 0, 3, 1, Material::GramaEscura);
        bloco_c(&mut v, 2, 0, 0, 0, 3, 3 + k, Material::GramaEscura);
        bloco_c(&mut v, -1, 0, 0, 0, 2 + k, 1, Material::GramaEscura);
        bloco_c(&mut v, -2, 0, 0, 0, 2 + k, 3, Material::GramaEscura);
        out.push(malha(&v));
    }
    // A snow mound.
    let mut v = novo();
    bloco_c(&mut v, 0, 0, 2, 2, 0, 1, Material::Neve);
    bloco_c(&mut v, 0, 0, 1, 1, 1, 1, Material::Neve);
    bloco_c(&mut v, 1, 0, 0, 0, 0, 1, Material::Gelo);
    out.push(malha(&v));
    // A dead tree.
    let mut v = novo();
    bloco_c(&mut v, 0, 0, 0, 0, 0, 6, Material::Tronco);
    bloco_c(&mut v, 1, 0, 0, 0, 4, 1, Material::Tronco);
    bloco_c(&mut v, 2, 0, 0, 0, 5, 2, Material::Tronco);
    bloco_c(&mut v, -1, 0, 0, 0, 3, 1, Material::Tronco);
    bloco_c(&mut v, -2, 0, 0, 0, 4, 2, Material::Tronco);
    out.push(malha(&v));
    // Skyreach's sky trees: a white trunk and gold or sky-blue crowns.
    for (k, (a, b)) in [(Material::FolhaDourada, Material::PetalaAmarela), (Material::FolhaCeleste, Material::GramaCeleste)].into_iter().enumerate() {
        let mut v = novo();
        bloco_c(&mut v, 0, 0, 0, 0, 0, 5 + k as i32, Material::TroncoBranco);
        bloco_c(&mut v, 0, 0, 2, 2, 4 + k as i32, 2, a);
        bloco_c(&mut v, 0, 0, 1, 1, 6 + k as i32, 2, b);
        bloco_c(&mut v, 2, 1, 1, 1, 3 + k as i32, 1, a);
        out.push(malha(&v));
    }
    // Crystal clusters: a tall shard and two short ones.
    for (m, ponta) in [(Material::CristalRoxo, Material::PetalaRoxa), (Material::CristalAzul, Material::CristalCeu), (Material::CristalCeu, Material::Nuvem)] {
        let mut v = novo();
        bloco_c(&mut v, 0, 0, 1, 1, 0, 6, m);
        bloco_c(&mut v, 0, 0, 0, 0, 6, 2, ponta);
        bloco_c(&mut v, 2, 1, 0, 0, 0, 3, m);
        bloco_c(&mut v, 2, 1, 0, 0, 3, 1, ponta);
        bloco_c(&mut v, -2, -1, 0, 0, 0, 4, m);
        out.push(malha(&v));
    }
    // Coral fans: a stem and branches, with glowing tips on the second.
    for (k, ponta) in [Material::Coral, Material::CristalAzul].into_iter().enumerate() {
        let mut v = novo();
        bloco_c(&mut v, 0, 0, 0, 0, 0, 4 + k as i32, Material::Coral);
        bloco_c(&mut v, 1, 0, 0, 0, 2, 3, Material::Coral);
        bloco_c(&mut v, -1, 0, 0, 0, 3, 3, Material::Coral);
        bloco_c(&mut v, 0, 1, 0, 0, 1, 2, Material::Coral);
        for (x, z, y) in [(0, 0, 4 + k as i32), (1, 0, 5), (-1, 0, 6), (0, 1, 3)] {
            bloco_c(&mut v, x, z, 0, 0, y, 1, ponta);
        }
        out.push(malha(&v));
    }
    // Kelp.
    let mut v = novo();
    for k in 0..7 {
        bloco_c(&mut v, (k / 3) % 2, 0, 0, 0, k, 1, Material::Alga);
        bloco_c(&mut v, 2 - (k / 2) % 2, 1, 0, 0, k, 1, Material::Alga);
    }
    out.push(malha(&v));
    debug_assert_eq!(out.len(), N_PROPS);
    out
}

fn mistura(a: i32, b: i32, s: u32) -> u32 {
    let mut h = (a as u32).wrapping_mul(0x9E37_79B1) ^ (b as u32).wrapping_mul(0x85EB_CA77) ^ s.wrapping_mul(0x27D4_EB2F);
    h ^= h >> 15;
    h = h.wrapping_mul(0x2C1B_3C6D);
    h ^= h >> 12;
    h
}

/// Column lattice of the props: at most one per 7×7 columns (3.5 u).
const CELULA_PROP: i32 = 7;

/// The prop on column `(bx, bz)` of the sea, if any: (model index into
/// `props`, scale, quarter turns). `forracao` = the Graphics ground cover
/// (percent): Sparse keeps a third of the forest.
pub fn prop_da_coluna(bx: i32, bz: i32, forracao: u8) -> Option<(usize, f32, u8)> {
    let (cx, cz) = (bx.div_euclid(CELULA_PROP), bz.div_euclid(CELULA_PROP));
    let h = mistura(cx, cz, 0xB05C);
    let ox = (h % CELULA_PROP as u32) as i32;
    let oz = ((h >> 5) % CELULA_PROP as u32) as i32;
    if bx != cx * CELULA_PROP + ox || bz != cz * CELULA_PROP + oz {
        return None;
    }
    let p = ::glam::Vec2::new(bx as f32 * BLOCO, bz as f32 * BLOCO);
    let t = mar::terra_em(p)?;
    if t.cais || t.molhe > 0.02 || t.dentro < 0.0 {
        return None;
    }
    let sorte = ((h >> 10) % 1000) as f32 / 1000.0;
    let sorte = sorte / (forracao.max(1) as f32 / 100.0);
    let escolha = (h >> 20) as usize;
    let (k, chance) = match t.ilha.feitio {
        Feitio::Bosque => {
            if t.dentro < 6.0 {
                (P_PEDRA + escolha % 2, 0.04)
            } else if escolha % 10 < 7 {
                (P_COPA + escolha % 3, if t.dentro < 12.0 { 0.2 } else { 0.62 })
            } else {
                (P_PINHO + escolha % 2, if t.dentro < 12.0 { 0.2 } else { 0.62 })
            }
        }
        Feitio::Geleira => {
            if t.h < 12.0 {
                return None;
            }
            if escolha % 4 == 0 {
                (if escolha % 8 == 0 { P_MONTE_DE_NEVE } else { P_PEDRA + escolha % 2 }, 0.2)
            } else {
                (P_PINHO_NEVE + escolha % 2, 0.18)
            }
        }
        Feitio::Ermo => {
            if t.h > 12.0 || t.dentro < 8.0 {
                return None;
            }
            match escolha % 7 {
                0..=2 => (P_CACTO + escolha % 2, 0.08),
                3..=5 => (P_ARENITO + escolha % 2, 0.08),
                _ => (P_SECA, 0.08),
            }
        }
        Feitio::Planalto => {
            if t.h < 18.0 {
                (P_PEDRA_ESCURA + escolha % 2, 0.03)
            } else if escolha % 3 == 0 {
                (P_PEDRA_ESCURA + escolha % 2, 0.3)
            } else {
                (P_PINHO_ESCURO + escolha % 2, 0.3)
            }
        }
        Feitio::Celeste => {
            if t.dentro < 5.0 {
                return None;
            }
            if escolha % 5 < 4 {
                (P_ARVORE_CELESTE + escolha % 2, 0.22)
            } else {
                (P_CRISTAL + 2, 0.08)
            }
        }
        // Kōgen-tō is concrete: its skyline is `mar_temas`'.
        Feitio::Neon => return None,
        Feitio::Abissal => {
            if t.dentro < 2.0 {
                return None;
            }
            if escolha % 3 == 0 {
                (P_ALGA, 0.16)
            } else {
                (P_CORAL + escolha % 2, 0.22)
            }
        }
        Feitio::Magica => {
            if t.dentro < 6.0 {
                return None;
            }
            if escolha % 3 == 0 {
                (P_CRISTAL + escolha % 2, 0.22)
            } else {
                (P_COPA + escolha % 3, 0.36)
            }
        }
    };
    if sorte >= chance {
        return None;
    }
    let escala = 1.5 + ((h >> 26) % 8) as f32 * 0.12;
    Some((k, escala, ((h >> 3) % 4) as u8))
}

/// `vegetacao::instancia`, turned by quarter turns (a turn keeps the faces'
/// winding, so back-face culling stays right) and never cut by the camera
/// (nothing on an island hides a ship).
#[allow(clippy::too_many_arguments)]
pub fn instancia_girada(
    modelo: &Modelo,
    em: Vec3,
    escala: f32,
    giro: u8,
    verts: &mut Vec<Vertex>,
    idx: &mut Vec<u16>,
    malhas: &mut Vec<Mesh>,
    max_quads: usize,
) {
    let gira = |p: Vec3| match giro % 4 {
        0 => p,
        1 => vec3(p.z, p.y, -p.x),
        2 => vec3(-p.x, p.y, -p.z),
        _ => vec3(-p.z, p.y, p.x),
    };
    let total = modelo.idx.len() / 6;
    let mut q = 0usize;
    while q < total {
        if idx.len() / 6 >= max_quads {
            malhas.push(Mesh { vertices: std::mem::take(verts), indices: std::mem::take(idx), texture: None });
        }
        let cabe = (max_quads - idx.len() / 6).min(total - q);
        let base = verts.len() as u16;
        for k in 0..cabe * 4 {
            let v = modelo.verts[q * 4 + k];
            verts.push(Vertex { position: gira(v.position) * escala + em, normal: Vec4::ZERO, ..v });
        }
        for k in 0..cabe {
            let b = base + (k * 4) as u16;
            idx.extend_from_slice(&[b, b + 1, b + 2, b, b + 2, b + 3]);
        }
        q += cabe;
    }
}

// ───────────────────────────── per-island colour ─────────────────────────────

/// The island's colour: its flags, its docking ring, its label.
pub fn cor_da_ilha(f: Feitio) -> Color {
    match f {
        Feitio::Bosque => Color::from_rgba(110, 214, 96, 255),
        Feitio::Geleira => Color::from_rgba(150, 214, 255, 255),
        Feitio::Ermo => Color::from_rgba(246, 176, 72, 255),
        Feitio::Planalto => Color::from_rgba(186, 116, 246, 255),
        Feitio::Celeste => Color::from_rgba(255, 214, 92, 255),
        Feitio::Neon => Color::from_rgba(255, 64, 150, 255),
        Feitio::Abissal => Color::from_rgba(64, 236, 220, 255),
        Feitio::Magica => Color::from_rgba(226, 120, 255, 255),
    }
}

// ───────────────────────────── far islands (LOD) ─────────────────────────────

struct Lod {
    /// The whole island: when none of it is under the loaded chunks.
    malhas: Vec<MalhaEstatica>,
    /// The same in 48 u tiles (with their box): when the island is partly
    /// under the loaded chunks, the tiles the chunks fully cover are skipped
    /// — a phone's 16-bit depth cannot keep a mesh a unit under the voxels
    /// hidden at 40 u, and drawing it there would z-fight through cliffs.
    ladrilhos: Vec<(MalhaEstatica, Vec2, Vec2)>,
    caixa: (Vec2, Vec2),
    vertices: usize,
    /// The new docks' LANDMARKS (`mar_temas::malha_de_longe`: floating
    /// isles, skyline, cranes, coral spires, crystals): they stand where no
    /// chunk covers them, so they are drawn whole whenever in reach.
    marcos: Vec<MalhaEstatica>,
    vertices_marcos: usize,
}

/// Side of a far-island tile, units.
const LADRILHO: f32 = 48.0;

/// Cuts a grid mesh of quads (6 indices each) into `LADRILHO` tiles by the
/// quad's centre: (mesh, box min, box max).
fn fatia(m: &Mesh) -> Vec<(Mesh, Vec2, Vec2)> {
    let mut baldes: std::collections::HashMap<(i32, i32), (Vec<Vertex>, Vec<u16>, std::collections::HashMap<u16, u16>, Vec2, Vec2)> =
        std::collections::HashMap::new();
    for q in m.indices.chunks(6) {
        let c = q.iter().map(|k| m.vertices[*k as usize].position).fold(Vec3::ZERO, |a, b| a + b) / 6.0;
        let chave = ((c.x / LADRILHO).floor() as i32, (c.z / LADRILHO).floor() as i32);
        let b = baldes.entry(chave).or_insert_with(|| (Vec::new(), Vec::new(), Default::default(), Vec2::splat(f32::MAX), Vec2::splat(f32::MIN)));
        for k in q {
            let novo = *b.2.entry(*k).or_insert_with(|| {
                b.0.push(m.vertices[*k as usize]);
                (b.0.len() - 1) as u16
            });
            b.1.push(novo);
            let p = m.vertices[*k as usize].position;
            b.3 = b.3.min(vec2(p.x, p.z));
            b.4 = b.4.max(vec2(p.x, p.z));
        }
    }
    baldes.into_values().map(|(v, i, _, a, b)| (Mesh { vertices: v, indices: i, texture: None }, a, b)).collect()
}

thread_local! {
    static LODS: RefCell<Vec<Option<Lod>>> = const { RefCell::new(Vec::new()) };
    static DOCAS: RefCell<Vec<Option<Doca>>> = const { RefCell::new(Vec::new()) };
}

/// The low-poly height mesh of island `i`, `passo` u a cell.
///
/// Each vertex takes the LOWEST ground within a cell round it, measured on
/// a 1 u grid: every point of a cell is then at least as high as its
/// corners' values, so the mesh never rises above the voxel terrain — where
/// the real chunks are loaded they cover it (no z-fight, no slope poking out
/// of a cliff or a mesa) and past them it is the island.
fn constroi_lod(i: &IlhaVista, passo: f32) -> (Vec<Mesh>, usize) {
    let meia = i.raio + 18.0;
    let p = passo.round().max(1.0) as i32;
    let n = (2.0 * meia / passo).ceil() as i32 + 1;
    let x0 = i.centro.x - meia;
    let z0 = i.centro.y - meia;
    let idx_de = |ix: i32, iz: i32| (iz * n + ix) as usize;
    // The ground on a 1 u grid, and its minimum over ±(passo + 1) u
    // (separable: rows, then columns).
    let nf = (n - 1) * p + 1;
    let fi = |x: i32, z: i32| (z * nf + x) as usize;
    let mut fino = vec![0.0f32; (nf * nf) as usize];
    for z in 0..nf {
        for x in 0..nf {
            fino[fi(x, z)] = mar::altura(x0 + x as f32, z0 + z as f32);
        }
    }
    let r = p + 1;
    let mut linha = vec![0.0f32; (nf * nf) as usize];
    for z in 0..nf {
        for x in 0..nf {
            let mut m = f32::MAX;
            for k in (x - r).max(0)..=(x + r).min(nf - 1) {
                m = m.min(fino[fi(k, z)]);
            }
            linha[fi(x, z)] = m;
        }
    }
    let mut minimo = vec![0.0f32; (n * n) as usize];
    for iz in 0..n {
        for ix in 0..n {
            let (x, z) = (ix * p, iz * p);
            let mut m = f32::MAX;
            for k in (z - r).max(0)..=(z + r).min(nf - 1) {
                m = m.min(linha[fi(x, k)]);
            }
            minimo[idx_de(ix, iz)] = m;
        }
    }
    let h_de = |ix: i32, iz: i32| minimo[idx_de(ix.clamp(0, n - 1), iz.clamp(0, n - 1))];
    let mut verts = Vec::with_capacity((n * n) as usize);
    for iz in 0..n {
        for ix in 0..n {
            let (x, z) = (x0 + ix as f32 * passo, z0 + iz as f32 * passo);
            let h = h_de(ix, iz);
            let y = if h <= 0.05 { -0.3 } else { h - 0.45 };
            // The voxel palette at the sample, shaded by slope the way the
            // voxel faces are (top 1.0, sides ~0.6).
            let (bx, bz) = ((x / BLOCO).round() as i32, (z / BLOCO).round() as i32);
            let mat = mar::material_do_topo(bx, bz).unwrap_or(Material::AreiaMolhada);
            let (r, g, b) = mat.rgb();
            let gx = h_de(ix + 1, iz) - h_de(ix - 1, iz);
            let gz = h_de(ix, iz + 1) - h_de(ix, iz - 1);
            let nrm = vec3(-gx, 2.0 * passo, -gz).normalize();
            let luz = 0.58 + 0.42 * nrm.y * nrm.y;
            verts.push(vert(vec3(x, y, z), [(r as f32 * luz) as u8, (g as f32 * luz) as u8, (b as f32 * luz) as u8, 255]));
        }
    }
    let mut idx: Vec<u16> = Vec::new();
    for iz in 0..n - 1 {
        for ix in 0..n - 1 {
            let c = [idx_de(ix, iz), idx_de(ix + 1, iz), idx_de(ix + 1, iz + 1), idx_de(ix, iz + 1)];
            if c.iter().all(|k| verts[*k].position.y < -0.1) {
                continue;
            }
            let [a, b, cc, d] = c.map(|k| k as u16);
            // Faces up (counter-clockwise seen from above in this frame).
            idx.extend_from_slice(&[a, cc, b, a, d, cc]);
        }
    }
    let usados = idx.len();
    (vec![Mesh { vertices: verts, indices: idx, texture: None }], usados)
}

/// Builds at most ONE missing island mesh or dock per frame (each costs a
/// few to a few tens of milliseconds): entering the sea never hitches.
fn prepara(perto: Vec2) {
    let ilhas = mar::ilhas();
    let passo = match qualidade() {
        0 => 5.0,
        1 => 4.0,
        _ => 3.0,
    };
    let feito = LODS.with(|l| {
        let mut l = l.borrow_mut();
        if l.len() != ilhas.len() {
            l.clear();
            l.resize_with(ilhas.len(), || None);
        }
        if let Some(k) = (0..ilhas.len()).find(|k| l[*k].is_none()) {
            let (ms, n) = constroi_lod(&ilhas[k], passo);
            let ladrilhos = ms.iter().flat_map(fatia).map(|(m, a, b)| (MalhaEstatica::nova(m), a, b)).collect();
            let i = &ilhas[k];
            let r = Vec2::splat(i.raio + 18.0);
            let c = mq(i.centro);
            let marcos = crate::mar_temas::malha_de_longe(i, &crate::mar_temas::tema(i));
            let vertices_marcos = marcos.iter().map(|m| m.vertices.len()).sum();
            l[k] = Some(Lod {
                malhas: ms.into_iter().map(MalhaEstatica::nova).collect(),
                ladrilhos,
                caixa: (c - r, c + r),
                vertices: n,
                marcos: marcos.into_iter().map(MalhaEstatica::nova).collect(),
                vertices_marcos,
            });
            return true;
        }
        false
    });
    if feito {
        return;
    }
    DOCAS.with(|d| {
        let mut d = d.borrow_mut();
        if d.len() != ilhas.len() {
            d.clear();
            d.resize_with(ilhas.len(), || None);
        }
        // The nearest dock first.
        let mut faltam: Vec<usize> = (0..ilhas.len()).filter(|k| d[*k].is_none()).collect();
        faltam.sort_by(|a, b| mq(ilhas[*a].raiz).distance(perto).total_cmp(&mq(ilhas[*b].raiz).distance(perto)));
        if let Some(&k) = faltam.first() {
            d[k] = Some(Doca::nova(&ilhas[k]));
        }
    });
}

/// How far a dock's geometry is drawn (its light and label go farther).
const ALCANCE_DA_DOCA: f32 = 360.0;

fn visivel(cam: &Camera3D, c: Vec2, r: f32, alcance: f32) -> bool {
    let olho = vec2(cam.position.x, cam.position.z);
    let frente = vec2(cam.target.x - cam.position.x, cam.target.z - cam.position.z).normalize_or_zero();
    let d = c - olho;
    let dist = d.length();
    dist - r < alcance && (dist < r * 1.2 || d.dot(frente) / dist > -0.35)
}

/// The far islands, with the world shader (fog included). Call AFTER the
/// water: far away the depth buffer cannot tell the island's shore from the
/// sea's ring (both near zero, hundreds of units off), and drawn later the
/// island wins those ties; up close it sits under the water and the chunks,
/// which already wrote their depth, so it never shows there.
pub fn desenha_ilhas_de_longe(t: &crate::terreno::Terreno, cam: &Camera3D) {
    #[cfg(debug_assertions)]
    if std::env::var("MMO_SEM_LOD").is_ok() {
        return;
    }
    let (_, fim) = neblina();
    // Is this box wholly under loaded chunks (sampled every chunk)?
    let coberta = |a: Vec2, b: Vec2| {
        let passo = crate::terreno::CHUNK as f32 * BLOCO;
        let (nx, nz) = (((b.x - a.x) / passo).ceil() as i32, ((b.y - a.y) / passo).ceil() as i32);
        (0..=nz).all(|j| {
            (0..=nx).all(|i| {
                let p = vec2((a.x + i as f32 * passo).min(b.x), (a.y + j as f32 * passo).min(b.y));
                t.pedaco_pronto_em(p.x, p.y)
            })
        })
    };
    let solido = Programa::Solido { recorte: Vec3::ZERO, recorte_z: 0.0 };
    LODS.with(|l| {
        for (k, lod) in l.borrow().iter().enumerate() {
            let (Some(lod), Some(i)) = (lod, mar::ilhas().get(k)) else {
                continue;
            };
            if !visivel(cam, mq(i.centro), i.raio + 18.0, fim) {
                continue;
            }
            if !lod.marcos.is_empty() {
                crate::gpu_estatica::desenha(solido, lod.marcos.iter());
            }
            let (a, b) = lod.caixa;
            let toca = t.pedaco_pronto_em(a.x, a.y) || t.pedaco_pronto_em(b.x, b.y) || t.pedaco_pronto_em(a.x, b.y) || t.pedaco_pronto_em(b.x, a.y) || {
                let alvo = vec2(cam.target.x, cam.target.z);
                let perto = alvo.clamp(a, b);
                perto.distance(alvo) < (crate::config_graficos::raio_terreno() as f32 + 1.5) * crate::terreno::CHUNK as f32 * BLOCO
            };
            if !toca {
                crate::gpu_estatica::desenha(solido, lod.malhas.iter());
            } else {
                crate::gpu_estatica::desenha(solido, lod.ladrilhos.iter().filter(|(_, a, b)| !coberta(*a, *b)).map(|(m, _, _)| m));
            }
        }
    });
}

/// The docks (pier, lighthouse, quay), with the world shader. Call after the
/// terrain, before the water (the pilings show through the shallows).
pub fn desenha_ilhas(cam: &Camera3D) {
    let alvo = vec2(cam.target.x, cam.target.z);
    prepara(alvo);
    let (_, fim) = neblina();
    DOCAS.with(|d| {
        for (k, doca) in d.borrow().iter().enumerate() {
            let (Some(doca), Some(i)) = (doca, mar::ilhas().get(k)) else {
                continue;
            };
            if visivel(cam, mq(i.raiz), 40.0, ALCANCE_DA_DOCA.min(fim)) {
                crate::gpu_estatica::desenha(Programa::Solido { recorte: Vec3::ZERO, recorte_z: 0.0 }, doca.malhas.iter());
            }
        }
    });
}

/// How much static sea geometry is built, for the report and the HUD:
/// (island-mesh vertices, dock vertices).
pub fn orcamento() -> (usize, usize) {
    let a = LODS.with(|l| l.borrow().iter().flatten().map(|l| l.vertices + l.vertices_marcos).sum());
    let b = DOCAS.with(|d| d.borrow().iter().flatten().map(|d| d.vertices).sum());
    (a, b)
}

// ───────────────────────────── the dock ─────────────────────────────

/// One island's dock, built once (`Doca::nova`), in world space.
struct Doca {
    malhas: Vec<MalhaEstatica>,
    vertices: usize,
    /// The lighthouse lamp.
    lampada: Vec3,
    /// The lamps at the pier's head.
    lanternas: [Vec3; 2],
    /// Flag poles' tops (the cloth is drawn per frame) and which way is out.
    bandeiras: Vec<Vec3>,
    /// The moored boats: where and their heading.
    barcos: [(Vec3, f32); 2],
    /// The new docks' lights and moving water (`mar_temas::Tema`): extra
    /// halos (a negative phase blinks), the waterfall, the whirlpool.
    luzes: Vec<(Vec3, Vec3, f32)>,
    cachoeira: Option<(Vec3, Vec3)>,
    redemoinho: Option<(Vec3, f32)>,
}

/// The dock's pieces in the pier frame: the port islands' planks,
/// lighthouse and quay, or a new dock's theme (`mar_temas::pecas`).
fn pecas_da_doca(i: &IlhaVista) -> Vec<(Modelo, Vec3)> {
    crate::mar_temas::pecas(i.feitio).unwrap_or_else(|| {
        vec![(pier(), Vec3::ZERO), (farol(), vec3(-10.5, 0.0, -12.0)), (cais_de_terra(), Vec3::ZERO)]
    })
}

/// Pier frame → world. Local x is across the pier (+x = −`lado`, so the
/// frame is a rotation, not a mirror: the faces keep their winding), z is
/// along it from the root, y is up from the sea.
pub(crate) fn no_mundo(i: &IlhaVista, p: Vec3) -> Vec3 {
    let e = i.eixo();
    let r = i.raiz + e * p.z + ::glam::Vec2::new(e.y, -e.x) * p.x;
    vec3(r.x, p.y, r.y)
}

/// Voxel counts of the dock, at `VOX` (0.25 u).
const V: f32 = VOX;

impl Doca {
    fn nova(i: &IlhaVista) -> Doca {
        // The lighthouse (or the theme's beacon) at `IlhaVista::farol`
        // (local x −10.5, z −12).
        let farol_local = vec3(-10.5, 0.0, -12.0);
        let pecas = pecas_da_doca(i);
        let tema = crate::mar_temas::e_tema(i.feitio);
        let mut verts: Vec<Vertex> = Vec::new();
        let mut idx: Vec<u16> = Vec::new();
        let mut malhas = Vec::new();
        let mut total = 0;
        for (m, desloca) in &pecas {
            let quads = m.idx.len() / 6;
            for q in 0..quads {
                if verts.len() + 4 > 60_000 {
                    total += verts.len();
                    malhas.push(Mesh { vertices: std::mem::take(&mut verts), indices: std::mem::take(&mut idx), texture: None });
                }
                let b = verts.len() as u16;
                for k in 0..4 {
                    let v = m.verts[q * 4 + k];
                    verts.push(Vertex { position: no_mundo(i, v.position + *desloca), normal: Vec4::ZERO, ..v });
                }
                idx.extend_from_slice(&[b, b + 1, b + 2, b, b + 2, b + 3]);
            }
        }
        total += verts.len();
        malhas.push(Mesh { vertices: verts, indices: idx, texture: None });
        let lampada = no_mundo(i, farol_local + vec3(0.0, crate::mar_temas::altura_da_lampada(i.feitio), 0.0));
        let lanternas = [-1.0f32, 1.0].map(|s| no_mundo(i, vec3(s * 5.25, if tema { 4.0 } else { 3.6 }, 25.3)));
        let mut bandeiras = vec![no_mundo(i, vec3(3.1, 8.0, -1.1)), no_mundo(i, vec3(-2.9, 8.0, -1.1))];
        if !tema {
            bandeiras.push(no_mundo(i, farol_local + vec3(0.0, 19.9, 0.0)));
        }
        let t = crate::mar_temas::tema(i);
        let redemoinho = t.redemoinho.map(|c| (c, mar::raio_da_lagoa(i, i.centro) * 0.85));
        let rumo = i.eixo();
        let yaw = rumo.x.atan2(rumo.y);
        let barcos = [
            (no_mundo(i, vec3(4.6, 0.0, 9.0)), yaw),
            (no_mundo(i, vec3(-4.4, 0.0, 16.0)), yaw + std::f32::consts::PI),
        ];
        Doca {
            malhas: malhas.into_iter().map(MalhaEstatica::nova).collect(),
            vertices: total,
            lampada,
            lanternas,
            bandeiras,
            barcos,
            luzes: t.luzes,
            cachoeira: t.cachoeira,
            redemoinho,
        }
    }
}

/// Height of the lighthouse lamp over the sea, units (the port islands';
/// the new docks' beacons: `mar_temas::altura_da_lampada`).
pub(crate) const LAMPADA_Y: f32 = 17.1;

fn vx(u: f32) -> i32 {
    (u / V).round() as i32
}

/// THE PIER in its own frame: planks across, two stringers, pilings into
/// the water every 3 u, a T head with lamps, crates and barrels, bollards.
fn pier() -> Modelo {
    let mut v = Volume::novo([-26, -8, -16], [52, 28, 124]);
    let (z0, z1) = (vx(-3.0), vx(mar::CAIS_COMP));
    let meia = vx(mar::CAIS_LARG * 0.5);
    let cab_z = vx(mar::CAIS_COMP - mar::CABECA_COMP);
    let cab_meia = vx(mar::CABECA_LARG * 0.5);
    let deck = 3; // voxel row 0.75..1.0 u: level with the quay
    for z in z0..z1 {
        let larg = if z >= cab_z { cab_meia } else { meia };
        // Planks across the pier, two voxels each, a dark seam every fifth.
        let tabua = match z.div_euclid(2).rem_euclid(5) {
            0 => Material::Tronco,
            1 | 3 => Material::Caminho,
            _ => Material::Terra,
        };
        for x in -larg..larg {
            let m = if x == -larg || x == larg - 1 { Material::Terra } else { tabua };
            v.poe(x, deck, z, m);
        }
        // Stringers under the deck edges.
        for x in [-larg, larg - 1] {
            v.poe(x, deck - 1, z, Material::Tronco);
        }
    }
    // Pilings: 2×2, from under the water to a bollard over the deck.
    let mut z = z0 + 4;
    while z < z1 {
        let larg = if z >= cab_z { cab_meia } else { meia };
        for x in [-larg - 1, larg - 1] {
            caixa_em(&mut v, [x, -8, z], [x + 2, deck + 2, z + 2], Material::Tronco);
        }
        z += 12;
    }
    for x in [-cab_meia - 1, -2, 0, cab_meia - 1] {
        caixa_em(&mut v, [x, -8, z1 - 2], [x + 2, deck + 2, z1], Material::Tronco);
    }
    // The head's two lamp posts with lanterns.
    for s in [-1, 1] {
        let x = if s < 0 { -cab_meia + 1 } else { cab_meia - 2 };
        caixa_em(&mut v, [x, deck + 1, z1 - 3], [x + 1, deck + 11, z1 - 2], Material::Tronco);
        caixa_em(&mut v, [x - 1, deck + 11, z1 - 4], [x + 2, deck + 13, z1 - 1], Material::JanelaAcesa);
        caixa_em(&mut v, [x - 1, deck + 13, z1 - 4], [x + 2, deck + 14, z1 - 1], Material::Tronco);
    }
    // Cargo on the head: crates and barrels.
    caixote(&mut v, [cab_meia - 8, deck + 1, cab_z + 3]);
    caixote(&mut v, [cab_meia - 8, deck + 4, cab_z + 4]);
    caixote(&mut v, [cab_meia - 12, deck + 1, cab_z + 2]);
    barril(&mut v, [-cab_meia + 4, deck + 1, cab_z + 4]);
    barril(&mut v, [-cab_meia + 4, deck + 1, cab_z + 8]);
    barril(&mut v, [-cab_meia + 8, deck + 1, cab_z + 6]);
    // Coiled rope beside the bollards.
    caixa_em(&mut v, [meia - 4, deck + 1, 40], [meia - 2, deck + 2, 42], Material::FolhaSeca);
    malha(&v)
}

fn caixote(v: &mut Volume, a: [i32; 3]) {
    caixa_em(v, a, [a[0] + 3, a[1] + 3, a[2] + 3], Material::Caminho);
    for (dx, dz) in [(0, 0), (2, 0), (0, 2), (2, 2)] {
        caixa_em(v, [a[0] + dx, a[1], a[2] + dz], [a[0] + dx + 1, a[1] + 3, a[2] + dz + 1], Material::Tronco);
    }
}

fn barril(v: &mut Volume, a: [i32; 3]) {
    for y in 0..4 {
        let m = if y == 1 || y == 3 { Material::RochaEscura } else { Material::Terra };
        caixa_em(v, [a[0], a[1] + y, a[2]], [a[0] + 3, a[1] + y + 1, a[2] + 3], m);
    }
}

/// THE LIGHTHOUSE, its foot on the quay (y 1 u): a stone base, a tapering
/// tower in red and white bands, a gallery with a rail, the lamp room in
/// glass with the light inside, and a red cap with a vane.
fn farol() -> Modelo {
    let mut v = Volume::novo([-10, 0, -10], [21, 84, 21]);
    let base = vx(mar::ALTURA_DO_MOLHE);
    let dentro = |x: i32, z: i32, r: f32| ((x * x + z * z) as f32) <= r * r;
    for y in 0..60 {
        let r = if y < 6 { 8.4 } else { 6.6 - (y - 6) as f32 * 0.04 };
        let m = if y < 6 {
            if y % 3 == 2 { Material::CalcadaEscura } else { Material::Calcada }
        } else if ((y - 6) / 7) % 2 == 0 {
            Material::PetalaBranca
        } else {
            Material::PetalaVermelha
        };
        for z in -9..=9 {
            for x in -9..=9 {
                if dentro(x, z, r) {
                    v.poe(x, base + y, z, m);
                }
            }
        }
    }
    // The door, facing the pier (+z), and two windows up the tower.
    caixa_em(&mut v, [-1, base, 6], [2, base + 9, 9], Material::Tronco);
    for y in [24, 38] {
        caixa_em(&mut v, [-1, base + y, 5], [1, base + y + 3, 8], Material::Vidro);
    }
    // The gallery: a wide dark ring, and its rail.
    let g = base + 60;
    for z in -9..=9 {
        for x in -9..=9 {
            if dentro(x, z, 8.6) {
                v.poe(x, g, z, Material::RochaEscura);
                if !dentro(x, z, 7.6) && (x + z) % 2 == 0 {
                    v.poe(x, g + 1, z, Material::RochaEscura);
                    v.poe(x, g + 2, z, Material::RochaEscura);
                }
            }
        }
    }
    // The lamp room: glass, and the light inside showing through.
    for y in 1..8 {
        for z in -4..=4 {
            for x in -4..=4 {
                if dentro(x, z, 4.4) {
                    let borda = !dentro(x, z, 3.4);
                    let pilar = borda && (x == 0 || z == 0);
                    let m = if pilar { Material::RochaEscura } else if borda { Material::Vidro } else { Material::JanelaAcesa };
                    v.poe(x, g + y, z, m);
                }
            }
        }
    }
    // The cap and the vane.
    for (k, r) in [5.2f32, 4.2, 3.0, 1.6, 0.6].into_iter().enumerate() {
        for z in -6..=6 {
            for x in -6..=6 {
                if dentro(x, z, r) {
                    v.poe(x, g + 8 + k as i32, z, Material::PetalaVermelha);
                }
            }
        }
    }
    caixa_em(&mut v, [0, g + 13, 0], [1, g + 15, 1], Material::RochaEscura);
    malha(&v)
}

/// What stands on the quay: a shed with a red roof, crates and barrels at
/// the pier's root, two flag poles.
fn cais_de_terra() -> Modelo {
    let mut v = Volume::novo([-22, 0, -90], [70, 40, 92]);
    let b = vx(mar::ALTURA_DO_MOLHE);
    // The shed: local x 4..10 u, z −20..−14 u.
    let (x0, x1, z0, z1) = (16, 38, -80, -56);
    for y in 0..11 {
        for z in z0..z1 {
            for x in x0..x1 {
                let borda = x == x0 || x == x1 - 1 || z == z0 || z == z1 - 1;
                if !borda {
                    continue;
                }
                let canto = (x == x0 || x == x1 - 1) && (z == z0 || z == z1 - 1);
                let m = if canto || y == 0 { Material::Tronco } else if (x + z) % 4 == 0 { Material::Terra } else { Material::Caminho };
                v.poe(x, b + y, z, m);
            }
        }
    }
    // Its door and windows, towards the pier (+z).
    caixa_em(&mut v, [25, b, z1 - 1], [29, b + 8, z1], Material::Tronco);
    for x in [19, 33] {
        caixa_em(&mut v, [x, b + 4, z1 - 1], [x + 3, b + 7, z1], Material::JanelaAcesa);
    }
    // The roof: steps up to a ridge along z, in a dark red.
    for k in 0..7 {
        for z in z0 - 1..z1 + 1 {
            for x in (x0 - 1 + k)..(x1 + 1 - k) {
                if x == x0 - 1 + k || x == x1 - k || k == 6 {
                    v.poe(x, b + 11 + k, z, Material::Arenito);
                }
            }
        }
    }
    caixa_em(&mut v, [x0 + 2, b + 11, z0], [x1 - 2, b + 12, z1], Material::Arenito);
    // Cargo at the root of the pier.
    caixote(&mut v, [14, b, -16]);
    caixote(&mut v, [14, b + 3, -15]);
    caixote(&mut v, [18, b, -18]);
    barril(&mut v, [-16, b, -14]);
    barril(&mut v, [-19, b, -10]);
    // Flag poles either side of the pier's root.
    for x in [12, -12] {
        caixa_em(&mut v, [x - 1, b, -6], [x + 2, b + 1, -3], Material::CalcadaEscura);
        caixa_em(&mut v, [x, b, -5], [x + 1, b + 28, -4], Material::Tronco);
    }
    malha(&v)
}

/// The dock's moving parts and lights, in the world pass after the water:
/// moored boats bobbing, flags waving, the lighthouse beam and glow, the
/// lamps, and the DOCKING RING on the water at the anchorage. `eu` = where
/// my ship is (the nearest ring brightens as you come in).
pub fn desenha_docas(cam: &Camera3D, tempo: f32, eu: Option<Vec2>) {
    let olho = cam.position;
    let (_, fim) = neblina();
    DOCAS.with(|d| {
        let d = d.borrow();
        // Boats and flags: solid, the world shader (whatever is bound).
        for (k, doca) in d.iter().enumerate() {
            let (Some(doca), Some(i)) = (doca, mar::ilhas().get(k)) else {
                continue;
            };
            let dist = vec2(olho.x, olho.z).distance(mq(i.raiz));
            if dist > ALCANCE_DA_DOCA.min(fim) {
                continue;
            }
            for (n, (p, yaw)) in doca.barcos.iter().enumerate() {
                crate::mar_casco::desenha_atracado(*p, *yaw, tempo + n as f32 * 1.3);
            }
            if dist < 160.0 {
                for (n, topo) in doca.bandeiras.iter().enumerate() {
                    bandeira(*topo, cor_da_ilha(i.feitio), tempo + n as f32 * 0.7 + k as f32);
                }
            }
        }
    });
    // Lights: additive, no depth write.
    let mut verts: Vec<Vertex> = Vec::new();
    let mut idx: Vec<u16> = Vec::new();
    DOCAS.with(|d| {
        for (k, doca) in d.borrow().iter().enumerate() {
            let (Some(doca), Some(i)) = (doca, mar::ilhas().get(k)) else {
                continue;
            };
            let dist = olho.distance(doca.lampada);
            let cor = crate::mar_temas::cor_da_luz(i.feitio);
            // The lamp's halo: never smaller than a few pixels, so it is a
            // star on the horizon from across the sea.
            let tam = (dist * 0.03).max(2.2) * (0.9 + 0.1 * (tempo * 3.0).sin());
            halo(&mut verts, &mut idx, olho, doca.lampada, tam, cor, 0.85);
            halo(&mut verts, &mut idx, olho, doca.lampada, tam * 2.6, cor, 0.25);
            // The beam: two blades turning round the lamp, seen from AFAR (up
            // close they would only sweep a white band over the screen).
            if dist > 150.0 && dist < fim {
                let a = tempo * 0.9 + k as f32;
                for s in [0.0f32, std::f32::consts::PI] {
                    facho(&mut verts, &mut idx, olho, doca.lampada, a + s, 55.0, cor);
                }
            }
            if dist < 120.0 {
                let cor_l = if crate::mar_temas::e_tema(i.feitio) { cor } else { vec3(1.0, 0.8, 0.45) };
                for l in doca.lanternas {
                    halo(&mut verts, &mut idx, olho, l, 0.9 + 0.1 * (tempo * 5.0 + l.x).sin(), cor_l, 0.7);
                }
            }
            // THE NEW DOCKS' LIGHTS (`mar_temas`): roof lights that blink,
            // glowing crystal and coral tips that breathe — stars over the
            // island from across the sea — and the moving water.
            if dist < fim {
                for (p, c, fase) in &doca.luzes {
                    let d = olho.distance(*p);
                    let (a, tam) = if *fase < 0.0 {
                        let liga = ((tempo * 0.8 - fase).fract() < 0.45) as u8 as f32;
                        (0.9 * liga, (d * 0.012).max(0.9))
                    } else {
                        (0.6 + 0.25 * (tempo * 1.6 + fase).sin(), (d * 0.016).max(1.4))
                    };
                    if a > 0.01 {
                        halo(&mut verts, &mut idx, olho, *p, tam, *c, a);
                    }
                }
                if let Some((topo, fundo)) = doca.cachoeira {
                    crate::mar_temas::cachoeira(&mut verts, &mut idx, olho, topo, fundo, tempo);
                }
                if let Some((c, r)) = doca.redemoinho {
                    crate::mar_temas::redemoinho(&mut verts, &mut idx, olho, c, tempo, r);
                }
            }
            // THE DOCKING RING at the anchorage, bright and pulsing; and a
            // faint wide one where "Dock" is offered.
            let f = mq(i.fundeadouro());
            if vec2(olho.x, olho.z).distance(f) < 260.0 {
                let perto = eu.map_or(1.0, |e| 1.0 - ((e.distance(f) - mar::PERTO_DO_CAIS) / 60.0).clamp(0.0, 1.0) * 0.5);
                let y = crate::agua::ALTURA_DA_AGUA + crate::agua::AMPLITUDE_MAX * 0.75 + 0.06;
                let c = cor_da_ilha(i.feitio);
                let c = vec3(c.r, c.g, c.b);
                let pulso = (tempo * 2.4).sin();
                anel(&mut verts, &mut idx, vec3(f.x, y, f.y), 4.2 + 0.25 * pulso, 0.55, c, 0.9 * perto, 0.0);
                anel(&mut verts, &mut idx, vec3(f.x, y, f.y), 1.6, 1.6, c, 0.18 * perto, 0.0);
                anel(&mut verts, &mut idx, vec3(f.x, y, f.y), mar::PERTO_DO_CAIS, 0.35, c, 0.35 * perto, tempo * 0.3);
            }
        }
    });
    if !idx.is_empty() {
        let m = Mesh { vertices: verts, indices: idx, texture: None };
        com_material(true, || draw_mesh(&m));
    }
}

/// A flag in the island's colour on top of a pole: a strip of quads waving
/// (two-sided: the world shader culls back faces).
fn bandeira(topo: Vec3, cor: Color, t: f32) {
    let mut verts = Vec::new();
    let mut idx: Vec<u16> = Vec::new();
    let vento = vec3(0.8, 0.0, 0.6).normalize();
    let (comp, alto, n) = (2.4f32, 1.4f32, 6);
    for k in 0..=n {
        let f = k as f32 / n as f32;
        let onda = (t * 4.0 - f * 5.0).sin() * 0.25 * f;
        let lado = vec3(-vento.z, 0.0, vento.x);
        let p = topo + vento * (f * comp) + lado * onda - vec3(0.0, 0.1 * f, 0.0);
        let escuro = 0.78 + 0.22 * ((t * 4.0 - f * 5.0).cos() * 0.5 + 0.5);
        let c = [(cor.r * escuro * 255.0) as u8, (cor.g * escuro * 255.0) as u8, (cor.b * escuro * 255.0) as u8, 255];
        verts.push(vert(p, c));
        verts.push(vert(p - vec3(0.0, alto, 0.0), c));
    }
    for k in 0..n as u16 {
        let (a, b, c, d) = (k * 2, k * 2 + 1, k * 2 + 2, k * 2 + 3);
        idx.extend_from_slice(&[a, b, c, b, d, c, a, c, b, b, c, d]);
    }
    draw_mesh(&Mesh { vertices: verts, indices: idx, texture: None });
}

fn halo(verts: &mut Vec<Vertex>, idx: &mut Vec<u16>, olho: Vec3, c: Vec3, tam: f32, cor: Vec3, a: f32) {
    let f = (olho - c).normalize_or_zero();
    let dx = f.cross(Vec3::Y).normalize_or_zero();
    let dy = dx.cross(f).normalize_or_zero();
    let base = verts.len() as u16;
    verts.push(vert(c, cor8(cor, a)));
    const N: u16 = 12;
    for k in 0..N {
        let ang = k as f32 / N as f32 * std::f32::consts::TAU;
        verts.push(vert(c + (dx * ang.cos() + dy * ang.sin()) * tam, cor8(cor, 0.0)));
    }
    for k in 0..N {
        idx.extend_from_slice(&[base, base + 1 + k, base + 1 + (k + 1) % N]);
    }
}

/// One blade of the lighthouse beam, turning flat over the sea.
fn facho(verts: &mut Vec<Vertex>, idx: &mut Vec<u16>, _olho: Vec3, c: Vec3, ang: f32, comp: f32, cor: Vec3) {
    let d = vec3(ang.cos(), -0.06, ang.sin());
    let lado = vec3(-d.z, 0.0, d.x);
    let base = verts.len() as u16;
    let ponta = c + d * comp;
    verts.push(vert(c + lado * 0.4, cor8(cor, 0.16)));
    verts.push(vert(c - lado * 0.4, cor8(cor, 0.16)));
    verts.push(vert(ponta + lado * 7.0, cor8(cor, 0.0)));
    verts.push(vert(ponta - lado * 7.0, cor8(cor, 0.0)));
    // And a vertical sheet, so it reads from above as well as from the side.
    verts.push(vert(c + Vec3::Y * 0.4, cor8(cor, 0.07)));
    verts.push(vert(c - Vec3::Y * 0.4, cor8(cor, 0.07)));
    verts.push(vert(ponta + Vec3::Y * 5.0, cor8(cor, 0.0)));
    verts.push(vert(ponta - Vec3::Y * 5.0, cor8(cor, 0.0)));
    idx.extend_from_slice(&[base, base + 1, base + 3, base, base + 3, base + 2]);
    idx.extend_from_slice(&[base + 4, base + 5, base + 7, base + 4, base + 7, base + 6]);
}

/// A flat ring on the water: radius `r`, width `w`; `tracejado` > 0 dashes
/// it (and turns the dashes with time).
#[allow(clippy::too_many_arguments)]
fn anel(verts: &mut Vec<Vertex>, idx: &mut Vec<u16>, c: Vec3, r: f32, w: f32, cor: Vec3, a: f32, tracejado: f32) {
    const N: usize = 48;
    let base = verts.len() as u16;
    for k in 0..=N {
        let ang = k as f32 / N as f32 * std::f32::consts::TAU + tracejado;
        let d = vec3(ang.cos(), 0.0, ang.sin());
        let alfa = if tracejado != 0.0 && k % 4 >= 2 { 0.0 } else { a };
        verts.push(vert(c + d * (r - w * 0.5).max(0.0), cor8(cor, if w > r { alfa } else { alfa * 0.6 })));
        verts.push(vert(c + d * (r + w * 0.5), cor8(cor, if w > r { 0.0 } else { alfa })));
    }
    for k in 0..N as u16 {
        let (a0, b0, a1, b1) = (base + k * 2, base + k * 2 + 1, base + k * 2 + 2, base + k * 2 + 3);
        idx.extend_from_slice(&[a0, b0, b1, a0, b1, a1]);
    }
}

// ───────────────────────────── labels ─────────────────────────────

/// Where `p` lands on a `tela`-sized target, and whether it is in FRONT of
/// the eye. Behind it, the point is mirrored, so its direction from the
/// centre still says which way to turn.
fn na_tela(cam: &Camera3D, p: Vec3, tela: Vec2) -> (Vec2, bool) {
    let clip = cam.matrix() * p.extend(1.0);
    let w = if clip.w.abs() < 1e-4 { 1e-4 } else { clip.w };
    let mut ndc = vec2(clip.x, clip.y) / w.abs();
    if w < 0.0 {
        // x/|w| keeps left and right; pushed far so it reads as off screen
        // (and towards the bottom: what is behind is below the view).
        ndc = vec2(ndc.x, ndc.y.min(-0.2)) * 1000.0;
    }
    (vec2((ndc.x * 0.5 + 0.5) * tela.x, (0.5 - ndc.y * 0.5) * tela.y), w > 0.0)
}

/// THE DOCK LABELS, screen space: "Bosque · Dock" and the distance over each
/// lighthouse in view, and a marker on the screen's edge pointing to every
/// dock out of view — so the way to land is always on screen. `tela` = the
/// size of what is drawn on (the screen, or the preview's target).
pub fn rotulos(cam: &Camera3D, eu: Option<Vec2>, tela: Vec2) {
    use crate::hud_estilo as estilo;
    let s = (tela.y / 800.0).clamp(0.7, 1.6);
    let tamanho = (17.0 * s) as u16;
    let pequeno = (13.0 * s) as u16;
    let margem = 46.0 * s;
    let de = eu.unwrap_or(vec2(cam.target.x, cam.target.z));
    // Markers already placed on the edge: a new one that would overlap
    // slides along the edge instead.
    let mut postos: Vec<Rect> = Vec::new();
    for i in mar::ilhas() {
        let cor = cor_da_ilha(i.feitio);
        let f = mq(i.fundeadouro());
        let dist = de.distance(f);
        let topo = no_mundo(i, vec3(-10.5, crate::mar_temas::altura_da_lampada(i.feitio) + 4.5, -12.0));
        let nome = format!("{} · Dock", i.nome);
        let metros = format!("{:.0} m", dist);
        let alfa = ((dist - 10.0) / 22.0).clamp(0.3, 1.0);
        let (q, na_frente) = na_tela(cam, topo, tela);
        let dentro = na_frente && q.x > margem && q.x < tela.x - margem && q.y > margem && q.y < tela.y - margem;
        if dentro {
            let c = Color::new(cor.r, cor.g, cor.b, alfa);
            estilo::texto_centro_forte(q.x + 1.5, q.y + 1.5, &nome, tamanho, Color::new(0.0, 0.0, 0.0, 0.75 * alfa));
            estilo::texto_centro_forte(q.x, q.y, &nome, tamanho, c);
            estilo::texto_centro(q.x + 1.0, q.y + 17.0 * s + 1.0, &metros, pequeno, Color::new(0.0, 0.0, 0.0, 0.7 * alfa));
            estilo::texto_centro(q.x, q.y + 17.0 * s, &metros, pequeno, Color::new(1.0, 1.0, 1.0, 0.9 * alfa));
            // An anchor mark under the name.
            ancora(vec2(q.x, q.y - 22.0 * s), 9.0 * s, c);
            continue;
        }
        // Off screen: a chevron on the edge, towards the dock, with its name.
        let centro = tela * 0.5;
        // The dock's own spot on the water, projected (flipped when behind).
        let (q, _) = na_tela(cam, vec3(f.x, 0.0, f.y), tela);
        let dir = (q - centro).normalize_or_zero();
        if dir == Vec2::ZERO {
            continue;
        }
        let meia = centro - Vec2::splat(margem);
        let k = (meia.x / dir.x.abs().max(1e-3)).min(meia.y / dir.y.abs().max(1e-3));
        let mut p = centro + dir * k;
        let rotulo = format!("{} · {:.0} m", i.nome, dist);
        let larg = estilo::medir_forte(&rotulo, pequeno) + 40.0 * s;
        let alto = 44.0 * s;
        let na_lateral = (p.x - margem).abs() < 1.0 || (p.x - (tela.x - margem)).abs() < 1.0;
        for _ in 0..6 {
            let r = Rect::new(p.x - larg * 0.5, p.y - alto * 0.5, larg, alto);
            if !postos.iter().any(|o| o.overlaps(&r)) {
                break;
            }
            if na_lateral {
                p.y = (p.y + alto).min(tela.y - margem);
            } else {
                p.x = (p.x - larg).max(margem);
            }
        }
        postos.push(Rect::new(p.x - larg * 0.5, p.y - alto * 0.5, larg, alto));
        let c = Color::new(cor.r, cor.g, cor.b, 0.95);
        let lado = vec2(-dir.y, dir.x);
        let ponta = p + dir * 12.0 * s;
        draw_triangle(ponta, p - dir * 4.0 * s + lado * 9.0 * s, p - dir * 4.0 * s - lado * 9.0 * s, Color::new(0.0, 0.0, 0.0, 0.5));
        draw_triangle(ponta, p - dir * 2.0 * s + lado * 8.0 * s, p - dir * 2.0 * s - lado * 8.0 * s, c);
        let t = p - dir * 26.0 * s;
        estilo::texto_centro_forte(t.x + 1.0, t.y + 5.0 * s + 1.0, &rotulo, pequeno, Color::new(0.0, 0.0, 0.0, 0.8));
        estilo::texto_centro_forte(t.x, t.y + 5.0 * s, &rotulo, pequeno, c);
    }
}

/// A small anchor: the dock icon (labels, minimap, map).
pub fn ancora(c: Vec2, r: f32, cor: Color) {
    let w = (r * 0.22).max(1.5);
    draw_circle_lines(c.x, c.y - r * 0.75, r * 0.25, w * 0.8, cor);
    draw_line(c.x, c.y - r * 0.5, c.x, c.y + r * 0.8, w, cor);
    draw_line(c.x - r * 0.5, c.y - r * 0.2, c.x + r * 0.5, c.y - r * 0.2, w, cor);
    // The flukes: an arc of two strokes.
    draw_line(c.x - r * 0.75, c.y + r * 0.25, c.x, c.y + r * 0.85, w, cor);
    draw_line(c.x + r * 0.75, c.y + r * 0.25, c.x, c.y + r * 0.85, w, cor);
}

// ───────────────────────────── wreckage and chests ─────────────────────────────

thread_local! {
    /// The wreckage models as meshes (`desenha_voxel` keys its buffers by the
    /// vertices' address: these are never dropped).
    static MALHAS_DESTROCO: RefCell<Vec<Option<Mesh>>> = const { RefCell::new(Vec::new()) };
}

/// THE FLOATING WRECKAGE in view, each riding the swell: rise, tilt with the
/// slope, a slow turn. One draw each, culled at 110 u.
pub fn desenha_destrocos(t: &crate::terreno::Terreno, cam: &Camera3D, tempo: f32) {
    let alvo = vec2(cam.target.x, cam.target.z);
    let alcance = match qualidade() {
        0 => 70.0,
        _ => 110.0,
    };
    MALHAS_DESTROCO.with(|c| {
        let mut c = c.borrow_mut();
        for (base, k) in t.destrocos_visiveis(cam) {
            if vec2(base.x, base.z).distance(alvo) > alcance {
                continue;
            }
            if c.len() <= k {
                c.resize_with(k + 1, || None);
            }
            if c[k].is_none() {
                let Some(m) = t.modelo_de_destroco(k) else {
                    continue;
                };
                c[k] = Some(Mesh { vertices: m.verts.clone(), indices: m.idx.clone(), texture: None });
            }
            let Some(m) = c[k].as_ref() else {
                continue;
            };
            let modelo = boia(vec2(base.x, base.z), tempo, (k as f32 * 1.7 + base.x * 0.13).sin() * 3.0) * Mat4::from_scale(Vec3::splat(2.0));
            crate::gpu_estatica::desenha_voxel(m, modelo, [0.0; 4], Default::default());
        }
    });
}

/// The matrix of something FLOATING at `p`: on the swell, tilted with it,
/// turned `giro` radians.
pub fn boia(p: Vec2, tempo: f32, giro: f32) -> Mat4 {
    let (h, g) = crate::agua::onda_em(p.x, p.y, tempo);
    let y = crate::agua::ALTURA_DA_AGUA + h - 0.15;
    Mat4::from_translation(vec3(p.x, y, p.y))
        * Mat4::from_rotation_x((g.y * 0.8).clamp(-0.3, 0.3))
        * Mat4::from_rotation_z((-g.x * 0.8).clamp(-0.3, 0.3))
        * Mat4::from_rotation_y(giro + (tempo * 0.2 + giro).sin() * 0.15)
}

/// A drifting CHEST at sea: a little raft riding the swell, a buoy flag
/// over it; returns where the chest itself sits (the caller draws it).
pub fn jangada(p: Vec3, semente: u64, tempo: f32) -> Vec3 {
    let q = vec2(p.x, p.z);
    let (h, _) = crate::agua::onda_em(q.x, q.y, tempo);
    let y = crate::agua::ALTURA_DA_AGUA + h;
    let fase = (semente % 31) as f32;
    let madeira = Color::from_rgba(122, 86, 50, 255);
    let escura = Color::from_rgba(84, 58, 34, 255);
    for k in -2..=2 {
        draw_cube(vec3(q.x + k as f32 * 0.42, y + 0.02, q.y), vec3(0.38, 0.16, 2.0), None, if k % 2 == 0 { madeira } else { escura });
    }
    draw_cube(vec3(q.x, y + 0.14, q.y - 0.8), vec3(2.2, 0.1, 0.22), None, escura);
    draw_cube(vec3(q.x, y + 0.14, q.y + 0.8), vec3(2.2, 0.1, 0.22), None, escura);
    // The buoy pole and its flag, so the prize reads from afar.
    let topo = vec3(q.x + 0.9, y + 2.4, q.y + 0.8);
    draw_cube(vec3(q.x + 0.9, y + 1.2, q.y + 0.8), vec3(0.08, 2.4, 0.08), None, escura);
    let ondula = (tempo * 4.0 + fase).sin() * 0.06;
    draw_cube(topo + vec3(0.35, -0.2 + ondula, 0.0), vec3(0.7, 0.4, 0.04), None, Color::from_rgba(236, 190, 84, 255));
    vec3(q.x - 0.1, y + 0.1, q.y)
}

// ───────────────────────────── preview ─────────────────────────────

/// A ship in a preview scene: where, heading, look.
#[cfg(debug_assertions)]
type NavioNaCena = (Vec2, f32, shared::navio::Visual);

/// THE OPEN SEA'S 3D PASS, the way the game draws it (`main::desenhar_mundo`
/// order): terrain, far islands and docks, water, wreckage, dock lights and
/// sky, then `navios` (where, heading, look, id). For the preview and the
/// first-login benchmark (`benchmark::cena_mar`), so both measure and show
/// exactly what sailing draws. Leaves the default material bound.
pub fn desenha_mundo(
    terreno: &crate::terreno::Terreno,
    solido: &Material2,
    cam: &Camera3D,
    navios: &[(Vec2, f32, shared::navio::Visual, u64)],
    eu: Option<Vec2>,
) {
    let no_mar = crate::mar_ui::no_mar();
    crate::mar_ui::define_no_mar(true);
    let t = get_time() as f32;
    set_camera(cam);
    let alvo = vec2(cam.target.x, cam.target.z);
    let (inicio, fim) = neblina();
    crate::gpu_estatica::define_neblina(alvo, inicio, fim);
    solido.set_uniform("Neblina", vec4(alvo.x, alvo.y, inicio, fim));
    let luz = if entardecer() > 0.5 { 1.0f32 } else { 0.0 };
    solido.set_uniform("LuzDia", luz);
    crate::gpu_estatica::define_luz_dia(luz);
    solido.set_uniform("Crop", Vec3::ZERO);
    gl_use_material(solido);
    #[cfg(debug_assertions)]
    let so_lod = std::env::var("MMO_SO_LOD").is_ok();
    #[cfg(not(debug_assertions))]
    let so_lod = false;
    if !so_lod {
        terreno.desenha(cam, Vec3::ZERO, 0.0);
    }
    desenha_ilhas(cam);
    crate::agua::desenha(terreno, cam, t);
    desenha_ilhas_de_longe(terreno, cam);
    gl_use_material(solido);
    desenha_destrocos(terreno, cam, t);
    desenha_docas(cam, t, eu);
    desenha_ceu(cam);
    gl_use_material(solido);
    for (q, yaw, v, id) in navios {
        crate::mar_casco::desenha(vec3(q.x, 0.0, q.y), *yaw, *id, *v);
    }
    gl_use_default_material();
    crate::gpu_estatica::define_neblina(Vec2::ZERO, 0.0, 0.0);
    solido.set_uniform("Neblina", Vec4::ZERO);
    solido.set_uniform("LuzDia", 0.0f32);
    crate::gpu_estatica::define_luz_dia(0.0);
    crate::mar_ui::define_no_mar(no_mar);
}

/// One preview frame: the 3D pass and the dock labels, into the target.
#[cfg(debug_assertions)]
fn cena(
    terreno: &crate::terreno::Terreno,
    solido: &Material2,
    cam: &Camera3D,
    navios: &[(Vec2, f32, shared::navio::Visual, u64)],
    eu: Option<Vec2>,
    tela: Vec2,
) {
    crate::render3d::camera_padrao();
    clear_background(cor_de_fundo());
    desenha_mundo(terreno, solido, cam, navios, eu);
    crate::render3d::camera_padrao();
    rotulos(cam, eu, tela);
}

/// PREVIEW (`MMO_PREVIA_MAR=1`): the open sea's scenes to PNGs in
/// `MMO_PREVIA_SAIDA` — a wide shot with two islands and their docks, a
/// close shot of each dock, each hull class, a ship under way with its wake,
/// the game's own camera off the Bosque's pier, the water at noon and at
/// dusk, and the four islands from the air. `MMO_PREVIA_SO=<part>` renders
/// only the scenes whose name contains it.
#[cfg(debug_assertions)]
pub async fn previa() {
    use shared::navio::Visual;
    let saida = std::env::var("MMO_PREVIA_SAIDA").unwrap_or_else(|_| "/tmp/tempest-mar".into());
    std::fs::create_dir_all(&saida).unwrap();
    crate::mar_ui::define_no_mar(true);
    crate::config_graficos::define_para_previa(crate::config_graficos::RAIO_PADRAO, 100);
    let (w, h) = (1280u32, 800u32);
    let tela = vec2(w as f32, h as f32);
    let rt = render_target_ex(w, h, RenderTargetParams { depth: true, sample_count: 1 });
    crate::render3d::define_alvo(Some(rt.clone()));
    let solido = crate::render3d::material_solido();
    let mut terreno = crate::terreno::Terreno::novo(&mar::DEF);
    let v = |z: &str| mar::vista_da_zona(z).unwrap();
    let (bosque, gelo, ermo, planalto) = (v("ilha_inicial"), v("ilha_gelo"), v("ilha_deserto"), v("ilha_planalto"));
    let so = std::env::var("MMO_PREVIA_SO").ok();
    let camera = |de: Vec3, para: Vec3, fovy: f32| Camera3D {
        position: de,
        target: para,
        up: Vec3::Y,
        fovy,
        aspect: Some(w as f32 / h as f32),
        render_target: Some(rt.clone()),
        ..Default::default()
    };
    let v3 = |p: Vec2, y: f32| vec3(p.x, y, p.y);
    let yaw_de = |d: Vec2| d.x.atan2(d.y);
    // Every island mesh and dock first (they build one a frame).
    for _ in 0..2 * mar::ilhas().len() + 4 {
        prepara(mq(bosque.raiz));
    }
    let fb = mq(bosque.fundeadouro());
    let eb = mq(bosque.eixo());
    let lb = mq(bosque.lado());
    let raiz_b = mq(bosque.raiz);
    let frota: Vec<NavioNaCena> = vec![
        (fb + eb * 22.0 + lb * 6.0, yaw_de(eb), Visual { classe: 1, vela: 2, ..Default::default() }),
        (fb + eb * 48.0 - lb * 14.0, yaw_de(eb + lb * 0.4), Visual { classe: 3, vela: 5, figura: 2, estrelas: 15 }),
        (fb + eb * 9.0 - lb * 3.0, yaw_de(-eb), Visual::default()),
    ];
    let com_id = |f: &[NavioNaCena]| f.iter().enumerate().map(|(k, (p, y, v))| (*p, *y, *v, 900 + k as u64)).collect::<Vec<_>>();
    // (name, camera, ships, terrain centre, dusk)
    let gf = mq(gelo.fundeadouro());
    let mut cenas: Vec<(String, Camera3D, Vec<NavioNaCena>, Vec2, f32)> = Vec::new();
    // 1. Wide: from over the Glacier's harbour across the channel to the
    //    Waste and its dock, ships on the way.
    let (eg, lg) = (mq(gelo.eixo()), mq(gelo.lado()));
    let ef = mq(ermo.fundeadouro());
    let rota = |t: f32| gf.lerp(ef, t);
    cenas.push((
        "largo".into(),
        camera(v3(mq(gelo.raiz) - eg * 30.0 + lg * 60.0, 55.0), v3(rota(0.3), 0.0), 1.0),
        vec![
            (gf + eg * 6.0 + lg * 5.0, yaw_de(eg), Visual { classe: 1, vela: 2, ..Default::default() }),
            (rota(0.2) - lg * 12.0, yaw_de(eg) + 0.3, Visual { classe: 3, vela: 5, figura: 2, estrelas: 15 }),
            (rota(0.35) + lg * 20.0, yaw_de(-eg), Visual { classe: 2, ..Default::default() }),
        ],
        rota(0.3),
        0.0,
    ));
    // 1b. Wide: the Bosque's harbour and the open sea towards the Glacier.
    let meio = (fb + gf) * 0.5;
    cenas.push((
        "largo-bosque".into(),
        camera(v3(fb + (fb - gf).normalize() * 40.0 + vec2(60.0, 90.0), 50.0), v3(fb.lerp(meio, 0.3), 0.0), 0.95),
        frota.clone(),
        fb.lerp(meio, 0.3),
        0.0,
    ));
    // 2. The Bosque's dock, close.
    cenas.push((
        "cais".into(),
        camera(v3(fb + eb * 26.0 + lb * 22.0, 14.0), v3(raiz_b + eb * 8.0, 4.0), 0.9),
        frota.clone(),
        raiz_b,
        0.0,
    ));
    // 2b. The other three docks.
    for (nome, i) in [("cais-geleira", gelo), ("cais-ermo", ermo), ("cais-planalto", planalto)] {
        let (f, e, l, raiz) = (mq(i.fundeadouro()), mq(i.eixo()), mq(i.lado()), mq(i.raiz));
        cenas.push((
            nome.into(),
            camera(v3(f + e * 34.0 - l * 26.0, 20.0), v3(raiz - e * 6.0, 6.0), 0.9),
            vec![(f + e * 5.0 + l * 4.0, yaw_de(e), Visual { classe: 2, vela: 3, figura: 3, estrelas: 9 })],
            raiz,
            0.0,
        ));
    }
    // 2c. THE NEW DOCKS (16/11/2026, `mar_temas`): each one close, and the
    //     approach from the open sea with the island's landmarks.
    for (nome, zona) in [
        ("celeste", shared::celeste::ZONA),
        ("kogen", shared::kogen::ZONA),
        ("abissal", shared::abissal::ZONA),
        ("magica", shared::magica::ZONA),
    ] {
        let i = v(zona);
        let (f, e, l, raiz, c) = (mq(i.fundeadouro()), mq(i.eixo()), mq(i.lado()), mq(i.raiz), mq(i.centro));
        let alto = crate::mar_temas::altura_da_lampada(i.feitio);
        cenas.push((
            format!("cais-{nome}"),
            camera(v3(f + e * 36.0 - l * 30.0, 16.0 + alto * 0.35), v3(raiz - e * 8.0 + l * 4.0, 4.0 + alto * 0.3), 0.95),
            vec![(f + e * 5.0 + l * 4.0, yaw_de(e), Visual { classe: 2, vela: 3, figura: 3, estrelas: 9 })],
            raiz,
            0.0,
        ));
        cenas.push((
            format!("chegada-{nome}"),
            camera(v3(f + e * 150.0 + l * 70.0, 34.0), v3(c.lerp(raiz, 0.45), alto * 0.45), 1.0),
            vec![
                (f + e * 40.0 + l * 6.0, yaw_de(-e), Visual { classe: 1, vela: 2, ..Default::default() }),
                (f + e * 90.0 - l * 10.0, yaw_de(-e) + 0.2, Visual { classe: 3, vela: 4, figura: 2, estrelas: 12 }),
            ],
            f + e * 40.0,
            0.0,
        ));
    }
    // 2d. THE WHOLE SEA, every island and dock, from high above (the fog
    //     pushed out for this one shot).
    cenas.push(("todo".into(), camera(vec3(0.0, 2350.0, 520.0), vec3(0.0, 0.0, 10.0), 0.95), vec![], Vec2::ZERO, 0.0));
    // 3. Each hull class, three-quarter view, on open water.
    let aberto = fb + eb * 120.0;
    for c in 0..4u8 {
        let vis = Visual { classe: c, vela: [0, 1, 4, 5][c as usize], figura: [0, 1, 3, 2][c as usize], estrelas: [0, 5, 10, 15][c as usize] };
        let dist = 7.0 + c as f32 * 1.6;
        cenas.push((
            format!("classe-{c}"),
            camera(
                v3(aberto + vec2(0.6f32.cos(), -0.6f32.sin()) * dist + vec2(0.6f32.sin(), 0.6f32.cos()) * dist * 0.55, 3.0 + c as f32 * 0.7),
                v3(aberto + vec2(0.6f32.sin(), 0.6f32.cos()) * 0.8, 2.4 + c as f32 * 0.6),
                0.95,
            ),
            vec![(aberto, 0.6, vis)],
            aberto,
            0.0,
        ));
    }
    // 4. The game's camera, behind my ship off the Bosque's pier.
    let jogo_alvo = fb + eb * 8.0;
    let mut jogo = crate::render3d::camera(jogo_alvo, 0.0, yaw_de(-eb) + 0.5, 1.0, crate::render3d::pitch_padrao());
    jogo.render_target = Some(rt.clone());
    jogo.aspect = Some(w as f32 / h as f32);
    cenas.push(("jogo".into(), jogo, vec![(jogo_alvo, yaw_de(-eb) + 0.9, Visual::default())], jogo_alvo, 0.0));
    // 4b. Zoomed out and lying low (the most the player can see).
    let z = crate::render3d::ZOOM_MAX;
    let mut longe = crate::render3d::camera(jogo_alvo, 0.0, yaw_de(-eb) + 0.5, z, crate::render3d::pitch_min_para(z));
    longe.render_target = Some(rt.clone());
    longe.aspect = Some(w as f32 / h as f32);
    cenas.push(("jogo-longe".into(), longe, vec![(jogo_alvo, yaw_de(-eb) + 0.9, Visual { classe: 2, vela: 2, ..Default::default() })], jogo_alvo, 0.0));
    // 5. The water at noon and at dusk: low over the swell, towards the sun.
    for (nome, tarde) in [("meio-dia", 0.0f32), ("entardecer", 1.0)] {
        let de = fb + eb * 70.0 + lb * 30.0;
        cenas.push((
            nome.into(),
            camera(v3(de, 7.0), v3(de + vec2(0.0, -60.0), 1.0), 1.0),
            vec![(de + vec2(6.0, -26.0), 2.6, Visual { classe: 2, ..Default::default() })],
            de,
            tarde,
        ));
    }
    // 6. The whole sea from the air.
    cenas.push(("alto".into(), camera(vec3(0.0, 900.0, 520.0), vec3(0.0, 0.0, 20.0), 0.95), vec![], Vec2::ZERO, 0.0));
    for (nome, cam, navios, centro, tarde) in cenas {
        if so.as_deref().is_some_and(|s| !nome.contains(s)) {
            continue;
        }
        define_entardecer(tarde);
        NEBLINA_DA_PREVIA.with(|n| n.set(if nome == "todo" { 4200.0 } else { 0.0 }));
        for _ in 0..80 {
            terreno.atualiza(vec2(cam.target.x, cam.target.z), crate::config_graficos::RAIO_PADRAO + 1, 400);
        }
        let _ = centro;
        let navios = com_id(&navios);
        for _ in 0..4 {
            cena(&terreno, &solido, &cam, &navios, Some(vec2(cam.target.x, cam.target.z)), tela);
            unsafe {
                get_internal_gl().flush();
            }
            next_frame().await;
        }
        rt.texture.get_texture_data().export_png(&format!("{saida}/mar-{nome}.png"));
        println!("mar-{nome}.png");
    }
    NEBLINA_DA_PREVIA.with(|n| n.set(0.0));
    // 7. A ship UNDER WAY: under three seconds of sailing in an arc, so the
    //    wake is real, then the shot from astern and above.
    if so.as_deref().is_none_or(|s| "navegando".contains(s)) {
        define_entardecer(0.0);
        let inicio = fb + eb * 60.0;
        for _ in 0..80 {
            terreno.atualiza(inicio + eb * 20.0, crate::config_graficos::RAIO_PADRAO + 1, 400);
        }
        let t0 = get_time();
        let mut p = inicio;
        let mut yaw = yaw_de(eb);
        let vis = Visual { classe: 2, vela: 1, figura: 1, estrelas: 6 };
        let mut ultimo = t0;
        while get_time() - t0 < 2.8 {
            let agora = get_time();
            let dt = (agora - ultimo) as f32;
            ultimo = agora;
            yaw += 0.35 * dt;
            p += vec2(yaw.sin(), yaw.cos()) * 11.0 * dt;
            let atras = p - vec2(yaw.sin(), yaw.cos()) * 16.0 + vec2(yaw.cos(), -yaw.sin()) * 8.0;
            let cam = camera(v3(atras, 9.0), v3(p + vec2(yaw.sin(), yaw.cos()) * 2.0, 1.0), 0.95);
            cena(&terreno, &solido, &cam, &[(p, yaw, vis, 77)], None, tela);
            unsafe {
                get_internal_gl().flush();
            }
            next_frame().await;
        }
        rt.texture.get_texture_data().export_png(&format!("{saida}/mar-navegando.png"));
        println!("mar-navegando.png");
    }
    let (ilhas, docas) = orcamento();
    println!("orcamento: ilhas {ilhas} indices, docas {docas} vertices");
    for c in 0..4u8 {
        let n = crate::mar_casco::modelo(Visual { classe: c, ..Default::default() });
        println!("casco {c}: {} vertices", n.vertices);
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    /// The island props stay LOW-POLY: an island seen from a deck is
    /// thousands of them, baked into the chunks.
    #[test]
    fn os_props_do_mar_sao_baratos() {
        let p = props();
        assert_eq!(p.len(), N_PROPS);
        assert_eq!(P_ALGA + 1, N_PROPS);
        for (k, m) in p.iter().enumerate() {
            let quads = m.idx.len() / 6;
            assert!(quads > 0 && quads <= 120, "prop {k} has {quads} quads");
        }
    }

    /// The Bosque is a FOREST and the props never stand on the pier, the
    /// quay or in the water.
    #[test]
    fn o_bosque_tem_mata_e_o_cais_fica_livre() {
        let b = mar::vista_da_zona("ilha_inicial").unwrap();
        let r = (b.raio / BLOCO) as i32;
        let (cx, cz) = ((b.centro.x / BLOCO) as i32, (b.centro.y / BLOCO) as i32);
        let mut arvores = 0;
        for bz in (cz - r)..(cz + r) {
            for bx in (cx - r)..(cx + r) {
                if let Some((k, _, _)) = prop_da_coluna(bx, bz, 100) {
                    let t = mar::terra_em(::glam::Vec2::new(bx as f32 * BLOCO, bz as f32 * BLOCO)).unwrap();
                    assert!(!t.cais && t.molhe <= 0.02 && t.dentro >= 0.0);
                    if k < P_PEDRA {
                        arvores += 1;
                    }
                }
            }
        }
        // ~60% of the 3.5 u cells of the interior: well over a thousand.
        assert!(arvores > 1000, "{arvores} trees on the Bosque");
        // And the Sparse setting keeps about a third.
        let ralo = (cz - r..cz + r)
            .flat_map(|bz| (cx - r..cx + r).map(move |bx| (bx, bz)))
            .filter(|(bx, bz)| prop_da_coluna(*bx, *bz, 30).is_some_and(|(k, _, _)| k < P_PEDRA))
            .count();
        assert!(ralo * 2 < arvores && ralo * 5 > arvores, "Sparse kept {ralo} of {arvores}");
    }

    /// The far island mesh never stands ABOVE the voxel terrain (or the near
    /// chunks would not cover it) and stays within a phone's budget.
    #[test]
    fn a_ilha_de_longe_fica_por_baixo_e_e_barata() {
        for i in mar::ilhas() {
            let (ms, idx) = constroi_lod(i, 3.0);
            let verts: usize = ms.iter().map(|m| m.vertices.len()).sum();
            let alto = ms.iter().flat_map(|m| m.vertices.iter()).map(|v| v.position.y).fold(f32::MIN, f32::max);
            println!("{}: {verts} vertices, {} triangles, top {alto}", i.nome, idx / 3);
            // Kōgen-tō's harbour is flat concrete (its skyline is the
            // landmarks' mesh) and the atoll is low coral.
            let teto = if crate::mar_temas::e_tema(i.feitio) { 2.0 } else { 10.0 };
            assert!(idx / 3 > 1000 && alto > teto, "{}: the far mesh is empty", i.nome);
            assert!(verts < 20_000, "{}: {verts} vertices", i.nome);
            assert!(idx / 3 < 30_000, "{}: {} triangles", i.nome, idx / 3);
            // Inside every triangle, not only at the vertices: a slope between
            // a cliff top and the quay below must not stand over the quay.
            let mut pior = f32::MIN;
            for m in &ms {
                for t in m.indices.chunks(3) {
                    let [a, b, c] = [0, 1, 2].map(|k| m.vertices[t[k] as usize].position);
                    for (u, w) in [(0.33f32, 0.33f32), (0.1, 0.1), (0.8, 0.1), (0.1, 0.8), (0.45, 0.45)] {
                        let q = a + (b - a) * u + (c - a) * w;
                        let h = mar::altura(q.x, q.z).max(0.0);
                        pior = pior.max(q.y - h);
                    }
                }
            }
            assert!(pior <= 0.05, "{}: the far mesh stands {pior} u over the voxel ground", i.nome);
        }
    }

    /// The dock frame is a rotation: local +z runs out along the pier and the
    /// lighthouse lands on the `IlhaVista::farol` spot.
    #[test]
    fn o_farol_fica_onde_o_mar_diz() {
        for i in mar::ilhas() {
            let f = no_mundo(i, vec3(-10.5, 0.0, -12.0));
            assert!(vec2(f.x, f.z).distance(mq(i.farol())) < 0.01, "{}", i.nome);
            let ponta = no_mundo(i, vec3(0.0, 0.0, mar::CAIS_COMP));
            assert!(vec2(ponta.x, ponta.z).distance(mq(i.ponta)) < 0.01);
        }
    }
}
