//! The sea: its own surface above the bed, colored by depth, foam at the
//! shore, moving glints and swell — all in the SHADER, with a single time uniform.
//!
//! Before, the sea was the top of the submerged columns painted a flat blue,
//! and beyond the last generated chunk there was nothing: the sky appeared
//! where the ocean should be. Here:
//!
//! * depth (sea level minus the `Gerador`'s floor) is computed ONCE per
//! vertex, when the chunk is generated — shallow turquoise, deep dark blue,
//! and the shallows let the bed's sand show through (lower alpha);
//! * the wave is a sum of two sinusoids in the vertex shader, with zero
//! amplitude at the shore (otherwise the water would peel off the sand)
//! and full from 3 u out;
//! * foam and glint come out of the fragment shader, with no texture;
//! * a horizon ring follows the camera and fades into the sky's color.
//!
//! Cost per frame: one uniform and the draw. No vertex work on the CPU, no
//! second pass — designed for a phone GPU.

use std::cell::RefCell;

use macroquad::prelude::*;
use shared::terreno::{Gerador, BLOCO, NIVEL_DO_MAR};

use crate::terreno::{Terreno, CHUNK};

/// Height of the surface at rest. Above the top of the submerged columns
/// (which sit at sea level) and below the first block of land.
pub const ALTURA_DA_AGUA: f32 = NIVEL_DO_MAR + 0.12;
/// Sum of the shader's wave amplitudes, in units.
///
/// It has to match the vertex shader's constants: there is a test that checks
/// the crest does not touch the first block of land.
pub const AMPLITUDE_MAX: f32 = 0.65;

/// `false` desliga a ondulacao (o resto continua).
///
/// **It was switched off from 19/09 to 21/09/2026.** What was there were two
/// short-wavelength sinusoids summed at the vertex, and in playtest it read
/// badly — it looked like rippling plastic, not water. Now the sea is
/// zone14's GERSTNER model (docs/MAR_ABERTO.md): long waves with sharp
/// crests, which is what the eye recognises as open sea.
pub const ONDAS: bool = true;

/// Profundidade (u) a partir da qual a agua e' "fundo" na cor.
const PROFUNDO: f32 = 6.0;
/// Up to here the wave is zero: the water's edge stays glued to the sand.
const COSTA_SEM_ONDA: f32 = 0.5;
/// From here outwards the wave is full.
const ONDA_CHEIA: f32 = 3.0;
/// Foam fades out by this depth.
const ESPUMA_ATE: f32 = 0.9;
/// The same draw cap as the terrain (4,800 indices per mesh).
const MAX_QUADS: usize = 800;
/// Passo da grade perto da costa, em blocos (1 u).
const PASSO_FINO: i32 = 2;
/// Step in open sea, in blocks (4 u): the bottom has nothing to show.
const PASSO_GROSSO: i32 = 8;
/// Cor do ceu (a mesma do `render3d::clear`): o horizonte some nela.
const CEU: [f32; 3] = [150.0, 186.0, 214.0];

fn suave(x: f32) -> f32 {
    let x = x.clamp(0.0, 1.0);
    x * x * (3.0 - 2.0 * x)
}

/// Water depth over column `(bx, bz)`, in units. Negative on land.
pub fn profundidade(ger: &Gerador, bx: i32, bz: i32) -> f32 {
    NIVEL_DO_MAR - (ger.bloco_em(bx, bz) + 1) as f32 * BLOCO
}

/// How much of the wave this vertex takes (0 at the shore, 1 in deep water).
pub fn onda_de(prof: f32) -> f32 {
    suave((prof - COSTA_SEM_ONDA) / (ONDA_CHEIA - COSTA_SEM_ONDA))
}

/// The vertex's foam (1 at the shoreline, 0 after `ESPUMA_ATE`).
pub fn espuma_de(prof: f32) -> f32 {
    if prof < -0.3 {
        return 0.0;
    }
    1.0 - suave(prof / ESPUMA_ATE)
}

/// Water color and alpha by depth: shallow turquoise and translucent, mid
/// blue, deep almost opaque dark blue.
pub fn cor_da_agua(prof: f32) -> [u8; 4] {
    const RASO: [f32; 4] = [72.0, 206.0, 200.0, 140.0];
    const MEIO: [f32; 4] = [34.0, 140.0, 180.0, 212.0];
    const FUNDO: [f32; 4] = [14.0, 54.0, 102.0, 244.0];
    let t = (prof.max(0.0) / PROFUNDO).clamp(0.0, 1.0);
    let (a, b, k) = if t < 0.35 {
        (RASO, MEIO, suave(t / 0.35))
    } else {
        (MEIO, FUNDO, suave((t - 0.35) / 0.65))
    };
    let mut c = [0u8; 4];
    for i in 0..4 {
        c[i] = (a[i] + (b[i] - a[i]) * k).round() as u8;
    }
    c
}

/// Bed color (the top of the submerged columns): sand darkening with depth.
/// It is what the translucent shallows let you see.
pub fn cor_do_leito(bloco: i32, bx: i32, bz: i32) -> [u8; 4] {
    let prof = NIVEL_DO_MAR - (bloco + 1) as f32 * BLOCO;
    let k = 1.0 - 0.6 * (prof.max(0.0) / 4.0).clamp(0.0, 1.0);
    let grao = ((bx.wrapping_mul(7) + bz.wrapping_mul(13)) & 7) as f32 - 3.5;
    let canal = |v: f32| (v * k + grao * 1.6).clamp(0.0, 255.0) as u8;
    [canal(196.0), canal(182.0), canal(136.0), 255]
}

/// A quad is water if any corner is in the sea (or nearly): a quad entirely
/// on land would be buried and would only cost vertices.
pub fn quad_de_agua(profs: [f32; 4]) -> bool {
    profs.iter().any(|p| *p > -0.25)
}

fn empilha(malhas: &mut Vec<Mesh>, verts: &mut Vec<Vertex>, idx: &mut Vec<u16>) {
    if !idx.is_empty() {
        malhas.push(Mesh {
            vertices: std::mem::take(verts),
            indices: std::mem::take(idx),
            texture: None,
        });
    }
}

fn quad(
    verts: &mut Vec<Vertex>,
    idx: &mut Vec<u16>,
    p: [Vec3; 4],
    extra: [(f32, f32, [u8; 4]); 4],
) {
    let b = verts.len() as u16;
    for (v, (onda, espuma, cor)) in p.into_iter().zip(extra) {
        verts.push(Vertex {
            position: v,
            uv: vec2(0.0, 0.0),
            color: cor,
            // x = 0: water does not enter the clip; y = wave; z = foam.
            normal: Vec4::new(0.0, onda, espuma, 0.0),
        });
    }
    // Faces up (the same sum as the terrain: the order comes from the normal).
    let geom = (p[1] - p[0]).cross(p[2] - p[0]);
    if geom.y >= 0.0 {
        idx.extend_from_slice(&[b, b + 1, b + 2, b, b + 2, b + 3]);
    } else {
        idx.extend_from_slice(&[b, b + 2, b + 1, b, b + 3, b + 2]);
    }
}

/// The surface of chunk `(cx, cz)`. Empty if the chunk is all land.
pub fn malhas_do_pedaco(ger: &Gerador, cx: i32, cz: i32) -> Vec<Mesh> {
    let (bx0, bz0) = (cx * CHUNK, cz * CHUNK);
    // A coarse sample to decide: all land -> nothing; all deep -> coarse grid.
    let mut min_prof = f32::MAX;
    let mut max_prof = f32::MIN;
    for iz in 0..=4 {
        for ix in 0..=4 {
            let p = profundidade(ger, bx0 + ix * CHUNK / 4, bz0 + iz * CHUNK / 4);
            min_prof = min_prof.min(p);
            max_prof = max_prof.max(p);
        }
    }
    if max_prof < -1.0 {
        return Vec::new();
    }
    let passo = if min_prof > ONDA_CHEIA + 0.5 {
        PASSO_GROSSO
    } else {
        PASSO_FINO
    };
    let n = CHUNK / passo;
    // The grid has ONE BORDER of slack on each side. It exists because each
    // vertex's wave is limited by the NEIGHBOURING water (see `onda_alcancavel`),
    // and without the border the vertices at a chunk's edge would look at a
    // smaller neighbourhood than the chunks beside them — giving different
    // heights at the same point, that is, a visible seam between chunks.
    let lado = n + 3;
    let mut grade = vec![0.0f32; (lado * lado) as usize];
    for iz in -1..=n + 1 {
        for ix in -1..=n + 1 {
            grade[((iz + 1) * lado + ix + 1) as usize] =
                profundidade(ger, bx0 + ix * passo, bz0 + iz * passo);
        }
    }
    let prof = |ix: i32, iz: i32| grade[((iz + 1) * lado + ix + 1) as usize];
    // THE WAVE THAT FITS: Gerstner does not only rise and fall, it MOVES
    // horizontally — up to `AMPLITUDE_MAX` of displacement. A vertex in deep
    // water can end its trip on top of a shallow column, and then its trough
    // ends up BELOW the bed: the ground shows through the water. Measured on the
    // archipelago, the worst case was a vertex in 2 u of depth landing on land
    // half a metre ABOVE the sea — 0.80 u of penetration.
    //
    // The rule, then, is not "the wave has the depth of the water here" but "the
    // wave has the depth of the shallowest water it REACHES". The grid step
    // (>= 1 u) already covers the reach (0.65 u), so the minimum of the
    // neighbours is enough and costs no extra query to the generator.
    let onda_alcancavel = |ix: i32, iz: i32| {
        let mut m = f32::MAX;
        for dz in -1..=1 {
            for dx in -1..=1 {
                m = m.min(prof(ix + dx, iz + dz));
            }
        }
        onda_de(m)
    };
    let (mut malhas, mut verts, mut idx) = (Vec::new(), Vec::new(), Vec::new());
    for iz in 0..n {
        for ix in 0..n {
            let cantos = [(ix, iz), (ix + 1, iz), (ix + 1, iz + 1), (ix, iz + 1)];
            let profs = cantos.map(|(x, z)| prof(x, z));
            if !quad_de_agua(profs) {
                continue;
            }
            if idx.len() + 6 > MAX_QUADS * 6 {
                empilha(&mut malhas, &mut verts, &mut idx);
            }
            // Mesma fase do terreno: coluna centrada em `i * BLOCO`.
            let pos = cantos.map(|(x, z)| {
                vec3(
                    (bx0 + x * passo) as f32 * BLOCO - BLOCO * 0.5,
                    ALTURA_DA_AGUA,
                    (bz0 + z * passo) as f32 * BLOCO - BLOCO * 0.5,
                )
            });
            let extra = [0, 1, 2, 3].map(|k| {
                let (x, z) = cantos[k];
                (
                    onda_alcancavel(x, z),
                    espuma_de(profs[k]),
                    cor_da_agua(profs[k]),
                )
            });
            quad(&mut verts, &mut idx, pos, extra);
        }
    }
    empilha(&mut malhas, &mut verts, &mut idx);
    malhas
}

/// Where the horizon ring starts for a terrain radius of `raio` chunks: 84 u
/// at the default 5, one chunk (16 u) in or out per step. Started any farther
/// than the chunks end, the Near view distance would show a gap of sky.
fn inicio_do_horizonte(raio: i32) -> f32 {
    84.0 + (raio - crate::config_graficos::RAIO_PADRAO) as f32 * CHUNK_U
}

const CHUNK_U: f32 = CHUNK as f32 * BLOCO;

/// A ring of ocean from the end of the chunks to the horizon, fading into the sky.
fn horizonte(centro: Vec2, raio: i32) -> Vec<Mesh> {
    const FORA: [f32; 7] = [110.0, 150.0, 210.0, 300.0, 430.0, 600.0, 800.0];
    const LADOS: usize = 48;
    let inicio = inicio_do_horizonte(raio);
    let raios: Vec<f32> = std::iter::once(inicio)
        .chain(FORA.into_iter().filter(|&r| r > inicio + 16.0))
        .collect();
    let fundo = cor_da_agua(PROFUNDO * 2.0);
    let cor_em = |r: f32| {
        let t = suave((r - raios[0]) / (raios[raios.len() - 1] - raios[0]));
        let mut c = [0u8; 4];
        for i in 0..3 {
            c[i] = (fundo[i] as f32 + (CEU[i] - fundo[i] as f32) * t).round() as u8;
        }
        c[3] = 255;
        c
    };
    let (mut malhas, mut verts, mut idx) = (Vec::new(), Vec::new(), Vec::new());
    for w in raios.windows(2) {
        let (r0, r1) = (w[0], w[1]);
        for k in 0..LADOS {
            if idx.len() + 6 > MAX_QUADS * 6 {
                empilha(&mut malhas, &mut verts, &mut idx);
            }
            let a0 = k as f32 / LADOS as f32 * std::f32::consts::TAU;
            let a1 = (k + 1) as f32 / LADOS as f32 * std::f32::consts::TAU;
            // A touch below the chunks' water: where the two cross, the near one wins on depth.
            let y = ALTURA_DA_AGUA - 0.05;
            let pt = |r: f32, a: f32| vec3(centro.x + r * a.cos(), y, centro.y + r * a.sin());
            let p = [pt(r0, a0), pt(r1, a0), pt(r1, a1), pt(r0, a1)];
            let e = [
                (0.5, 0.0, cor_em(r0)),
                (0.5, 0.0, cor_em(r1)),
                (0.5, 0.0, cor_em(r1)),
                (0.5, 0.0, cor_em(r0)),
            ];
            quad(&mut verts, &mut idx, p, e);
        }
    }
    empilha(&mut malhas, &mut verts, &mut idx);
    malhas
}

pub(crate) const VERTICE: &str = r#"#version 100
#if defined(GL_FRAGMENT_PRECISION_HIGH) || !defined(GL_ES)
#define AGUA_PRECISAO highp
#else
#define AGUA_PRECISAO mediump
#endif
attribute vec3 position;
attribute vec2 texcoord;
attribute vec4 color0;
attribute vec4 normal;

varying lowp vec4 cor;
varying AGUA_PRECISAO vec3 mundo;
varying lowp float espuma;
varying lowp float onda;

uniform mat4 Model;
uniform mat4 Projection;
uniform AGUA_PRECISAO float Tempo;
uniform highp float Ondas;

// ── O MAR DE GERSTNER ────────────────────────────────────────────────
//
// Ported from zone14 (`GameSim/Ocean.cs`), which uses FIVE waves of
// wavelength 52/31/18/9.5/4.8 m. Here there are THREE, and the cut has a
// reason: the water grid in open sea has a 4 u step, and Nyquist says that
// below 8 u of wavelength the wave is not represented — it becomes moving
// aliasing. zone14's short ones exist because its mesh is far finer; here
// they would only cost.
//
// Gerstner, and not a sum of sinusoids: besides rising and falling, the
// vertex MOVES horizontally against the wave's direction. That is what
// sharpens the crest and flattens the trough — the difference between
// "water" and "a sheet flapping", which was exactly the complaint that
// switched the old wave off.
//
// omega = sqrt(g*k) is the deep-water dispersion relation: a long wave runs
// faster than a short one, on its own. Without it they all move together and
// the pattern repeats in a way the eye catches.
// THE WAVE ONLY RISES. `(sn+1)/2` puts each wave's contribution in [0, amp]
// instead of [-amp, amp]: the surface oscillates UPWARD from rest and never
// drops below it.
//
// Without this the trough pierced the ground. And the sea floor is not where
// the generator says: `terreno.rs` flattens EVERY submerged column to
// `BLOCO_DO_MAR` (`h.max(BLOCO_DO_MAR)`, so the greedy merge joins the bed
// into one plane), so what is DRAWN is a plane at NIVEL_DO_MAR. Between it
// and the water there is 0.12 of slack, and the wave swung 0.65: the trough
// went half a metre BELOW the bed, and the player saw the ground through the water.
//
// Gerstner already makes a sharp crest and a flat trough, so an upward-only
// wave reads as sea anyway — what is lost is a trough that did not fit in
// any case.
// GLSL ES 1.00 does not accept line continuation in macros on all drivers.
// A function avoids the compile error when opening the characters.
highp vec3 deslocamento_onda(highp vec2 direcao, highp float k,
    highp float velocidade, highp float amplitude, highp float fase) {
    highp float theta = k * dot(direcao, position.xz) - velocidade * Tempo + fase;
    highp float sn = sin(theta);
    highp float cs = cos(theta);
    return amplitude * vec3(-direcao.x * cs, sn * 0.5 + 0.5, -direcao.y * cs);
}

void main() {
    highp vec3 p = position;
    // `normal.y` is how much of the wave THIS vertex takes: zero at the shore
    // (the edge stays glued to the sand) and full in deep water.
    highp float a = normal.y * Ondas;
    p += a * deslocamento_onda(vec2(0.5646, 0.8253), 0.1208, 1.089, 0.341, 0.0);
    p += a * deslocamento_onda(vec2(0.7470, 0.6648), 0.2027, 1.410, 0.198, 1.7);
    p += a * deslocamento_onda(vec2(0.1977, 0.9803), 0.3491, 1.851, 0.110, 4.1);
    gl_Position = Projection * Model * vec4(p, 1.0);
    cor = color0 / 255.0;
    mundo = p;
    espuma = normal.z;
    onda = normal.y;
}"#;

// The precision of Tempo and world must match in both stages. Desktop OpenGL
// (macOS) does not define GL_FRAGMENT_PRECISION_HIGH, but supports highp.
// A fragment with no fixed `highp`: a phone GPU may have no `highp` in the
// fragment (GLES2 makes it optional) and would refuse to compile. Uses
// `highp` when the driver offers it; otherwise falls back to `mediump` — the
// glint patches are less fine far from the origin, but the shader compiles.
pub(crate) const FRAGMENTO: &str = r#"#version 100
#if defined(GL_FRAGMENT_PRECISION_HIGH) || !defined(GL_ES)
#define AGUA_PRECISAO highp
#else
#define AGUA_PRECISAO mediump
#endif
precision AGUA_PRECISAO float;
varying lowp vec4 cor;
varying AGUA_PRECISAO vec3 mundo;
varying lowp float espuma;
varying lowp float onda;

uniform sampler2D Texture;
uniform AGUA_PRECISAO float Tempo;
// The world's distance fog (see `render3d::SOLIDO_FRAGMENTO`).
uniform highp vec4 Neblina;

void main() {
    vec3 c = cor.rgb;
    // Espuma pulsando e deslizando na linha da costa.
    float n = sin(mundo.x * 1.7 + Tempo * 1.3) * sin(mundo.z * 1.3 - Tempo * 1.1);
    float f = clamp(espuma * (0.62 + 0.38 * n), 0.0, 1.0);
    c = mix(c, vec3(0.93, 0.97, 1.0), f);
    // No fake glint: the moving bright patches read as white blobs in playtest.
    // Color by depth and foam at the shore are enough.
    float a = max(cor.a, f * 0.9);
    if (Neblina.w > 0.0) {
        // Fogged water turns opaque sky: past the fog the horizon ring is
        // just sky, and the edge of the loaded sea never shows.
        float k = smoothstep(Neblina.z, Neblina.w, distance(mundo.xz, Neblina.xy));
        c = mix(c, vec3(0.588, 0.729, 0.839), k);
        a = mix(a, 1.0, k);
    }
    gl_FragColor = vec4(c, a);
}"#;

thread_local! {
    static HORIZONTE: RefCell<Option<((i32, i32, i32), Vec<crate::gpu_estatica::MalhaEstatica>)>> = const { RefCell::new(None) };
}

/// The sea's pipeline state (no face culling: the wave turns the quad).
/// Shared with the direct GPU draw (`gpu_estatica`).
pub(crate) fn params_agua() -> PipelineParams {
    use macroquad::miniquad::graphics::{
        BlendFactor, BlendState, BlendValue, Comparison, CullFace, Equation,
    };
    PipelineParams {
        cull_face: CullFace::Nothing,
        depth_test: Comparison::LessOrEqual,
        depth_write: true,
        color_blend: Some(BlendState::new(
            Equation::Add,
            BlendFactor::Value(BlendValue::SourceAlpha),
            BlendFactor::OneMinusValue(BlendValue::SourceAlpha),
        )),
        ..Default::default()
    }
}

/// Draws the sea (the horizon and the surface of visible chunks). It swaps
/// the material: the caller restores its own afterwards.
pub fn desenha(t: &Terreno, cam: &Camera3D, tempo: f32) {
    if t.sem_oceano() { return; }
    use crate::gpu_estatica::{desenha as desenha_na_gpu, MalhaEstatica, Programa};
    let ondas = if ONDAS && crate::config_graficos::ondas() { 1.0f32 } else { 0.0 };
    // The ring follows the camera's target, rebuilt only when it moves 32 u
    // or the view distance changes.
    let raio = crate::config_graficos::raio_terreno();
    let chave = (
        (cam.target.x / 32.0).round() as i32,
        (cam.target.z / 32.0).round() as i32,
        raio,
    );
    HORIZONTE.with(|h| {
        let mut h = h.borrow_mut();
        if h.as_ref().is_none_or(|(k, _)| *k != chave) {
            let centro = vec2(chave.0 as f32 * 32.0, chave.1 as f32 * 32.0);
            *h = Some((
                chave,
                horizonte(centro, raio)
                    .into_iter()
                    .map(MalhaEstatica::nova)
                    .collect(),
            ));
        }
        if let Some((_, malhas)) = h.as_ref() {
            desenha_na_gpu(Programa::Agua { tempo, ondas }, malhas);
        }
    });
    t.desenha_agua(cam, tempo, ondas);
}

#[cfg(test)]
mod testes {
    use super::*;
    use shared::terreno::{ARQUIPELAGO, ESCALA_ALTURA};

    fn luminancia(c: [u8; 4]) -> f32 {
        0.2126 * c[0] as f32 + 0.7152 * c[1] as f32 + 0.0722 * c[2] as f32
    }

    #[test]
    fn cor_escurece_com_a_profundidade() {
        let mut anterior = f32::MAX;
        let mut alfa = 0u8;
        for k in 0..=48 {
            let c = cor_da_agua(k as f32 * 0.25);
            let l = luminancia(c);
            assert!(l <= anterior + 1e-3, "clareou em {} u", k as f32 * 0.25);
            assert!(
                c[3] >= alfa,
                "raso tem que ser mais translucido que o fundo"
            );
            anterior = l;
            alfa = c[3];
        }
        assert!(luminancia(cor_da_agua(0.0)) > luminancia(cor_da_agua(PROFUNDO)) + 60.0);
    }

    #[test]
    fn onda_e_zero_na_costa_e_cheia_no_fundo() {
        for p in [-2.0, -0.1, 0.0, 0.3, COSTA_SEM_ONDA] {
            assert_eq!(onda_de(p), 0.0, "onda na costa em {p}");
        }
        assert_eq!(onda_de(ONDA_CHEIA), 1.0);
        assert!(espuma_de(0.0) > 0.99 && espuma_de(ESPUMA_ATE) < 0.01 && espuma_de(-1.0) == 0.0);
    }

    #[test]
    fn a_agua_nunca_sobe_na_terra() {
        // The wave only reaches the height of a block of land in DEEP water.
        //
        // This test used to demand that the FULL crest fit below the first block.
        // It was simple and wrong: it charged the open sea by a rule that only
        // holds at the beach. With the Gerstner sea the amplitude grew, and the
        // right question became another — the full wave only happens where
        // `onda_de` allows, and there the nearest land is far away.
        //
        // What is kept, then, is: from what depth would the crest reach a block of
        // land? It has to be deep enough not to be any kind of beach edge.
        let primeira_terra = NIVEL_DO_MAR + BLOCO;
        let alcanca = (0..200)
            .map(|k| k as f32 * 0.05)
            .find(|p| ALTURA_DA_AGUA + AMPLITUDE_MAX * onda_de(*p) >= primeira_terra)
            .expect("em algum fundo a crista alcanca");
        assert!(
            alcanca >= 1.5,
            "a crista alcanca terra com so' {alcanca} u de fundo — isso e' praia"
        );
        // And on the shoreline itself it is zero.
        assert_eq!(onda_de(0.0), 0.0);
        // And above the bed (the top of the submerged columns, at sea level).
        assert!(ALTURA_DA_AGUA > NIVEL_DO_MAR);
        assert!(
            !quad_de_agua([-0.5, -1.0, -3.0, -0.3]),
            "quad todo em terra nao vira agua"
        );
        assert!(quad_de_agua([-0.5, 0.2, -3.0, -0.3]));
    }

    #[test]
    fn malha_da_agua_cabe_no_desenho() {
        let d = &ARQUIPELAGO[0];
        let ger = Gerador::da_ilha(d);
        // Around the harbour (shore, shallow water and deep) with the game's draw radius.
        let porto = ger
            .vila()
            .porto
            .as_ref()
            .map(|p| p.centro)
            .unwrap_or(::glam::Vec2::ZERO);
        let (pcx, pcz) = (
            ((porto.x / BLOCO) as i32).div_euclid(CHUNK),
            ((porto.y / BLOCO) as i32).div_euclid(CHUNK),
        );
        let t0 = std::time::Instant::now();
        let (mut malhas, mut verts, mut indices, mut pedacos) = (0, 0, 0, 0);
        for dz in -5..=5 {
            for dx in -5..=5 {
                let ms = malhas_do_pedaco(&ger, pcx + dx, pcz + dz);
                if !ms.is_empty() {
                    pedacos += 1
                }
                for m in &ms {
                    assert!(
                        m.indices.len() <= MAX_QUADS * 6,
                        "malha com {} indices",
                        m.indices.len()
                    );
                    assert!(m
                        .vertices
                        .iter()
                        .all(|v| (v.position.y - ALTURA_DA_AGUA).abs() < 1e-4));
                    malhas += 1;
                    verts += m.vertices.len();
                    indices += m.indices.len();
                }
            }
        }
        let tempo = t0.elapsed();
        let anel = horizonte(vec2(0.0, 0.0), crate::config_graficos::RAIO_PADRAO);
        let anel_idx: usize = anel.iter().map(|m| m.indices.len()).sum();
        assert!(anel.iter().all(|m| m.indices.len() <= MAX_QUADS * 6));
        println!(
            "agua em volta do porto: {pedacos} pedacos com agua, {malhas} malhas, {verts} vertices, \
             {indices} indices, gerada em {tempo:?}; horizonte {} malhas, {anel_idx} indices",
            anel.len()
        );
        assert!(pedacos > 0, "sem agua em volta do porto");
    }
}

#[cfg(test)]
mod testes_do_vale {
    use super::*;
    use shared::terreno::ARQUIPELAGO;

    /// THE BED THAT IS DRAWN, and not the one the generator says.
    ///
    /// `terreno.rs` flattens every submerged column to `BLOCO_DO_MAR`
    /// (`alt[i] = h.max(BLOCO_DO_MAR)`, so the greedy merge joins the bottom into
    /// one plane). So the sea floor is **not** at the generator's depth: it is a
    /// plane at `NIVEL_DO_MAR`, 0.12 from the water.
    ///
    /// The first version of this test used the GENERATOR's depth. It passed —
    /// measuring a bottom nobody draws — while the owner watched the ground
    /// appear through the water. A test that models the wrong thing is worse
    /// than no test: it grants permission.
    const LEITO_DESENHADO: f32 = NIVEL_DO_MAR;

    /// The wave does NOT go below rest, and so it does not pierce the bed.
    #[test]
    fn a_onda_nunca_desce_abaixo_do_leito_desenhado() {
        // The shader sums `amp * (sin + 1)/2` per wave: each term's minimum is
        // ZERO, so the minimum surface is the one at rest.
        let minimo = ALTURA_DA_AGUA;
        assert!(
            minimo > LEITO_DESENHADO,
            "a água em repouso ({minimo}) não está acima do leito desenhado \
             ({LEITO_DESENHADO})"
        );
        // And the slack really is small — that is why the wave has to be
        // upward-only instead of symmetric.
        let folga = ALTURA_DA_AGUA - LEITO_DESENHADO;
        assert!(
            folga < AMPLITUDE_MAX,
            "há {folga} de folga e a onda balança {AMPLITUDE_MAX}: se um dia \
             couber, a onda pode voltar a ser simétrica"
        );
    }

    /// And it DOES still swell: an upward-only wave zeroed everywhere would be
    /// trading one defect for a sea of ice.
    #[test]
    fn o_mar_aberto_continua_ondulando() {
        let ger = Gerador::da_ilha(&ARQUIPELAGO[0]);
        let cheias = (-900..900)
            .step_by(7)
            .flat_map(|bz| (-900..900).step_by(7).map(move |bx| (bx, bz)))
            .filter(|(bx, bz)| profundidade(&ger, *bx, *bz) > 0.0)
            .filter(|(bx, bz)| onda_de(profundidade(&ger, *bx, *bz)) > 0.9)
            .count();
        assert!(
            cheias > 200,
            "só {cheias} pontos com onda cheia — o limite apagou o mar"
        );
    }

    /// A CRISTA continua cabendo abaixo do primeiro bloco de terra.
    ///
    /// With the upward-only wave the crest stays where it was (the sine's peak
    /// did not change) — but that has to be stated, not assumed.
    #[test]
    fn a_crista_nao_subiu_com_a_onda_so_pra_cima() {
        let crista = ALTURA_DA_AGUA + AMPLITUDE_MAX;
        let simetrica_antes = ALTURA_DA_AGUA + AMPLITUDE_MAX;
        assert_eq!(crista, simetrica_antes);
        // And the land test (`a_agua_nunca_sobe_na_terra`) still holds: it measures
        // exactly this sum.
    }
}
