//! O mar: superficie propria por cima do leito, com cor pela profundidade,
//! espuma na costa, brilho andando e ondulacao — tudo no SHADER, com um
//! uniform de tempo so'.
//!
//! Antes o mar era o topo das colunas submersas pintado de um azul chapado,
//! e alem do ultimo pedaco gerado nao havia nada: o ceu aparecia no lugar do
//! oceano. Aqui:
//!
//!   * a profundidade (nivel do mar menos o fundo do `Gerador`) e' calculada
//!     UMA vez por vertice, na geracao do pedaco — raso turquesa, fundo azul
//!     escuro, e o raso deixa ver a areia do leito (alfa menor);
//!   * a onda e' soma de duas senoides no vertex shader, com amplitude zero
//!     na costa (senao a agua descolaria da areia) e cheia a partir de 3 u;
//!   * espuma e brilho saem do fragment shader, sem textura;
//!   * um anel de horizonte segue a camera e some na cor do ceu.
//!
//! Custo por quadro: um uniform e o desenho. Nada de vertice na CPU, nada de
//! segunda passada — pensado pra GPU de celular.

use std::cell::RefCell;

use macroquad::prelude::*;
use shared::terreno::{Gerador, BLOCO, NIVEL_DO_MAR};

use crate::terreno::{Terreno, CHUNK};

/// Altura da superficie parada. Acima do topo das colunas submersas (que
/// ficam no nivel do mar) e abaixo do primeiro bloco de terra.
pub const ALTURA_DA_AGUA: f32 = NIVEL_DO_MAR + 0.12;
/// Soma das amplitudes das ondas do shader, em unidades.
///
/// Tem que casar com as constantes do vertex shader: ha' um teste que confere
/// que a crista nao encosta no primeiro bloco de terra.
pub const AMPLITUDE_MAX: f32 = 0.65;

/// `false` desliga a ondulacao (o resto continua).
///
/// **Ficou desligada de 19/09 a 21/09/2026.** O que havia eram duas senoides
/// de comprimento curto somadas no vertice, e no playtest leu feio — parecia
/// plastico ondulando, nao agua. Agora o mar e' o modelo de GERSTNER do
/// zone14 (docs/MAR_ABERTO.md): ondas longas, de cristas afiadas, que e'
/// o que o olho reconhece como mar aberto.
pub const ONDAS: bool = true;

/// Profundidade (u) a partir da qual a agua e' "fundo" na cor.
const PROFUNDO: f32 = 6.0;
/// Ate' aqui a onda e' zero: a borda da agua fica colada na areia.
const COSTA_SEM_ONDA: f32 = 0.5;
/// Daqui pra fundo a onda e' cheia.
const ONDA_CHEIA: f32 = 3.0;
/// Espuma some ate' esta profundidade.
const ESPUMA_ATE: f32 = 0.9;
/// Mesmo teto de desenho do terreno (4.800 indices por malha).
const MAX_QUADS: usize = 800;
/// Passo da grade perto da costa, em blocos (1 u).
const PASSO_FINO: i32 = 2;
/// Passo em mar aberto, em blocos (4 u): o fundo nao tem o que mostrar.
const PASSO_GROSSO: i32 = 8;
/// Cor do ceu (a mesma do `render3d::clear`): o horizonte some nela.
const CEU: [f32; 3] = [150.0, 186.0, 214.0];

fn suave(x: f32) -> f32 {
    let x = x.clamp(0.0, 1.0);
    x * x * (3.0 - 2.0 * x)
}

/// Profundidade da agua sobre a coluna `(bx, bz)`, em unidades. Negativa em
/// terra.
pub fn profundidade(ger: &Gerador, bx: i32, bz: i32) -> f32 {
    NIVEL_DO_MAR - (ger.bloco_em(bx, bz) + 1) as f32 * BLOCO
}

/// Quanto da onda este vertice leva (0 na costa, 1 no fundo).
pub fn onda_de(prof: f32) -> f32 {
    suave((prof - COSTA_SEM_ONDA) / (ONDA_CHEIA - COSTA_SEM_ONDA))
}

/// Espuma do vertice (1 na linha da costa, 0 depois de `ESPUMA_ATE`).
pub fn espuma_de(prof: f32) -> f32 {
    if prof < -0.3 {
        return 0.0;
    }
    1.0 - suave(prof / ESPUMA_ATE)
}

/// Cor e alfa da agua pela profundidade: raso turquesa e translucido, meio
/// azul, fundo azul escuro quase opaco.
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

/// Cor do leito (o topo das colunas submersas): areia que escurece com a
/// profundidade. E' o que o raso translucido deixa ver.
pub fn cor_do_leito(bloco: i32, bx: i32, bz: i32) -> [u8; 4] {
    let prof = NIVEL_DO_MAR - (bloco + 1) as f32 * BLOCO;
    let k = 1.0 - 0.6 * (prof.max(0.0) / 4.0).clamp(0.0, 1.0);
    let grao = ((bx.wrapping_mul(7) + bz.wrapping_mul(13)) & 7) as f32 - 3.5;
    let canal = |v: f32| (v * k + grao * 1.6).clamp(0.0, 255.0) as u8;
    [canal(196.0), canal(182.0), canal(136.0), 255]
}

/// Um quad e' agua se algum canto esta' no mar (ou quase): quad inteiro em
/// terra ficaria enterrado e so' gastaria vertice.
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
            // x = 0: agua nao entra no recorte; y = onda; z = espuma.
            normal: Vec4::new(0.0, onda, espuma, 0.0),
        });
    }
    // Olha pra cima (mesma conta do terreno: a ordem sai da normal).
    let geom = (p[1] - p[0]).cross(p[2] - p[0]);
    if geom.y >= 0.0 {
        idx.extend_from_slice(&[b, b + 1, b + 2, b, b + 2, b + 3]);
    } else {
        idx.extend_from_slice(&[b, b + 2, b + 1, b, b + 3, b + 2]);
    }
}

/// A superficie do pedaco `(cx, cz)`. Vazia se o pedaco e' todo terra.
pub fn malhas_do_pedaco(ger: &Gerador, cx: i32, cz: i32) -> Vec<Mesh> {
    let (bx0, bz0) = (cx * CHUNK, cz * CHUNK);
    // Amostra grossa pra decidir: todo terra → nada; todo fundo → grade grossa.
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
    // A grade tem UMA BORDA de folga de cada lado. Ela existe porque a onda
    // de cada vertice e' limitada pela agua VIZINHA (ver `onda_alcancavel`),
    // e sem a borda os vertices da beira do pedaco olhariam pra uma vizinhanca
    // menor que a dos pedacos ao lado — dando alturas diferentes no mesmo
    // ponto, ou seja, uma costura visivel entre pedacos.
    let lado = n + 3;
    let mut grade = vec![0.0f32; (lado * lado) as usize];
    for iz in -1..=n + 1 {
        for ix in -1..=n + 1 {
            grade[((iz + 1) * lado + ix + 1) as usize] =
                profundidade(ger, bx0 + ix * passo, bz0 + iz * passo);
        }
    }
    let prof = |ix: i32, iz: i32| grade[((iz + 1) * lado + ix + 1) as usize];
    // A ONDA QUE CABE: o Gerstner nao so' sobe e desce, ele ANDA na
    // horizontal — ate' `AMPLITUDE_MAX` de deslocamento. Um vertice em agua
    // funda pode terminar o passeio em cima de uma coluna rasa, e ai o vale
    // dele fica ABAIXO do leito: o chao aparece por dentro da agua. Medido no
    // arquipelago, o pior caso era um vertice em 2 u de fundo caindo sobre
    // terra meio metro ACIMA do mar — 0,80 u de penetracao.
    //
    // A regra, entao, nao e' "a onda tem a altura do fundo daqui", e sim "a
    // onda tem a altura da agua mais rasa que ela ALCANCA". O passo da grade
    // (>= 1 u) ja' cobre o alcance (0,65 u), entao o minimo dos vizinhos basta
    // e nao custa uma consulta a mais ao gerador.
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
                (onda_alcancavel(x, z), espuma_de(profs[k]), cor_da_agua(profs[k]))
            });
            quad(&mut verts, &mut idx, pos, extra);
        }
    }
    empilha(&mut malhas, &mut verts, &mut idx);
    malhas
}

/// Anel de oceano do fim dos pedacos ate' o horizonte, sumindo no ceu.
fn horizonte(centro: Vec2) -> Vec<Mesh> {
    const RAIOS: [f32; 8] = [84.0, 110.0, 150.0, 210.0, 300.0, 430.0, 600.0, 800.0];
    const LADOS: usize = 48;
    let fundo = cor_da_agua(PROFUNDO * 2.0);
    let cor_em = |r: f32| {
        let t = suave((r - RAIOS[0]) / (RAIOS[RAIOS.len() - 1] - RAIOS[0]));
        let mut c = [0u8; 4];
        for i in 0..3 {
            c[i] = (fundo[i] as f32 + (CEU[i] - fundo[i] as f32) * t).round() as u8;
        }
        c[3] = 255;
        c
    };
    let (mut malhas, mut verts, mut idx) = (Vec::new(), Vec::new(), Vec::new());
    for w in RAIOS.windows(2) {
        let (r0, r1) = (w[0], w[1]);
        for k in 0..LADOS {
            if idx.len() + 6 > MAX_QUADS * 6 {
                empilha(&mut malhas, &mut verts, &mut idx);
            }
            let a0 = k as f32 / LADOS as f32 * std::f32::consts::TAU;
            let a1 = (k + 1) as f32 / LADOS as f32 * std::f32::consts::TAU;
            // Um tico abaixo da agua dos pedacos: onde os dois se cruzam, a de
            // perto ganha na profundidade.
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
attribute vec3 position;
attribute vec2 texcoord;
attribute vec4 color0;
attribute vec4 normal;

varying lowp vec4 cor;
varying highp vec3 mundo;
varying lowp float espuma;
varying lowp float onda;

uniform mat4 Model;
uniform mat4 Projection;
uniform highp float Tempo;
uniform highp float Ondas;

// ── O MAR DE GERSTNER ────────────────────────────────────────────────
//
// Portado do zone14 (`GameSim/Ocean.cs`), que usa CINCO ondas de
// comprimentos 52/31/18/9,5/4,8 m. Aqui sao TRES, e o corte tem motivo:
// a grade da agua em mar aberto tem passo de 4 u, e Nyquist diz que
// abaixo de 8 u de comprimento a onda nao e' representada — ela vira
// serrilha andando. As curtas do zone14 existem porque la' a malha e'
// muito mais fina; aqui elas so' custariam.
//
// Gerstner, e nao soma de senoides: alem de subir e descer, o vertice
// ANDA na horizontal contra a direcao da onda. E' isso que afia a crista
// e achata o vale — a diferenca entre "agua" e "lencol balancando", que
// foi exatamente a reclamacao que desligou a onda antiga.
//
// omega = sqrt(g*k) e' a relacao de dispersao de agua funda: onda longa
// corre mais rapido que onda curta, sozinha. Sem isso todas andam juntas
// e o padrao se repete de um jeito que o olho pega.
#define ONDA(dx, dz, k, w, amp, fase)                                    \
    theta = k * (dx * position.x + dz * position.z) - w * Tempo + fase;  \
    sn = sin(theta); cs = cos(theta);                                    \
    p.y += a * amp * sn;                                                 \
    p.x -= a * amp * dx * cs;                                            \
    p.z -= a * amp * dz * cs;

void main() {
    highp vec3 p = position;
    // `normal.y` e' o quanto ESTE vertice leva de onda: zero na costa (a
    // beira fica colada na areia) e cheio no fundo.
    highp float a = normal.y * Ondas;
    highp float theta, sn, cs;
    ONDA(0.5646, 0.8253, 0.1208, 1.089, 0.341, 0.0)
    ONDA(0.7470, 0.6648, 0.2027, 1.410, 0.198, 1.7)
    ONDA(0.1977, 0.9803, 0.3491, 1.851, 0.110, 4.1)
    gl_Position = Projection * Model * vec4(p, 1.0);
    cor = color0 / 255.0;
    mundo = p;
    espuma = normal.z;
    onda = normal.y;
}"#;

// Fragmento sem `highp` fixo: GPU de celular pode nao ter `highp` no
// fragmento (GLES2 deixa opcional) e recusaria compilar. Usa `highp` quando o
// driver oferece; senao cai pra `mediump` — as manchas de brilho ficam menos
// finas longe da origem, mas o shader compila.
pub(crate) const FRAGMENTO: &str = r#"#version 100
#ifdef GL_FRAGMENT_PRECISION_HIGH
precision highp float;
#else
precision mediump float;
#endif
varying lowp vec4 cor;
varying vec3 mundo;
varying lowp float espuma;
varying lowp float onda;

uniform sampler2D Texture;
uniform float Tempo;

void main() {
    vec3 c = cor.rgb;
    // Espuma pulsando e deslizando na linha da costa.
    float n = sin(mundo.x * 1.7 + Tempo * 1.3) * sin(mundo.z * 1.3 - Tempo * 1.1);
    float f = clamp(espuma * (0.62 + 0.38 * n), 0.0, 1.0);
    c = mix(c, vec3(0.93, 0.97, 1.0), f);
    // Sem brilho falso: as manchas claras andando liam como bolinhas brancas
    // no playtest. Cor por profundidade e espuma da costa bastam.
    gl_FragColor = vec4(c, max(cor.a, f * 0.9));
}"#;

thread_local! {
    static HORIZONTE: RefCell<Option<((i32, i32), Vec<crate::gpu_estatica::MalhaEstatica>)>> = const { RefCell::new(None) };
}

/// Estado de pipeline do mar (sem descarte de face: a onda vira o quad).
/// Compartilhado com o desenho direto na GPU (`gpu_estatica`).
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

/// Desenha o mar (horizonte e superficie dos pedacos visiveis). Troca o
/// material: quem chama volta o dele depois.
pub fn desenha(t: &Terreno, cam: &Camera3D, tempo: f32) {
    use crate::gpu_estatica::{desenha as desenha_na_gpu, MalhaEstatica, Programa};
    let ondas = if ONDAS { 1.0f32 } else { 0.0 };
    // O anel acompanha o alvo da camera, refeito so' quando ele anda 32 u.
    let chave = (
        (cam.target.x / 32.0).round() as i32,
        (cam.target.z / 32.0).round() as i32,
    );
    HORIZONTE.with(|h| {
        let mut h = h.borrow_mut();
        if h.as_ref().is_none_or(|(k, _)| *k != chave) {
            let centro = vec2(chave.0 as f32 * 32.0, chave.1 as f32 * 32.0);
            *h = Some((
                chave,
                horizonte(centro)
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
        // A onda so' chega na altura de um bloco de terra em agua FUNDA.
        //
        // Este teste ja' exigiu que a crista CHEIA coubesse abaixo do
        // primeiro bloco. Era simples e errado: cobrava o mar aberto por uma
        // regra que so' vale na praia. Com o mar de Gerstner a amplitude
        // cresceu, e a pergunta certa passou a ser outra — a onda cheia so'
        // acontece onde `onda_de` deixa, e la' a terra mais perto esta'
        // longe.
        //
        // O que se guarda entao e': a partir de que profundidade a crista
        // alcancaria um bloco de terra? Tem que ser agua funda o bastante
        // pra nao ser beira de praia nenhuma.
        let primeira_terra = NIVEL_DO_MAR + BLOCO;
        let alcanca = (0..200)
            .map(|k| k as f32 * 0.05)
            .find(|p| ALTURA_DA_AGUA + AMPLITUDE_MAX * onda_de(*p) >= primeira_terra)
            .expect("em algum fundo a crista alcanca");
        assert!(
            alcanca >= 1.5,
            "a crista alcanca terra com so' {alcanca} u de fundo — isso e' praia"
        );
        // E na propria linha da costa ela e' zero.
        assert_eq!(onda_de(0.0), 0.0);
        // E acima do leito (topo das colunas submersas, no nivel do mar).
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
        // Em volta do porto (costa, agua rasa e fundo) com o raio de desenho do jogo.
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
        let anel = horizonte(vec2(0.0, 0.0));
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

    /// A onda que um ponto leva, limitada pela agua mais rasa ao ALCANCE
    /// dela. Espelha o `onda_alcancavel` da geracao da malha, consultando o
    /// gerador em vez da grade.
    fn onda_que_cabe(ger: &Gerador, bx: i32, bz: i32, passo: i32) -> f32 {
        let mut m = f32::MAX;
        for dz in -1..=1 {
            for dx in -1..=1 {
                m = m.min(profundidade(ger, bx + dx * passo, bz + dz * passo));
            }
        }
        onda_de(m)
    }

    /// O VALE da onda nao pode furar o leito — nem embaixo do vertice, nem
    /// onde o Gerstner LEVA o vertice.
    ///
    /// O teste irmao (`a_agua_nunca_sobe_na_terra`) guarda a CRISTA contra a
    /// terra, e nada mais. Por baixo nao havia guarda nenhum, e foi por
    /// baixo que o defeito entrou: em 21/09/2026 o dono viu a onda afundar
    /// no chao do mar e deixar o leito a mostra.
    ///
    /// A parte que pega e' a HORIZONTAL. Gerstner desloca o vertice de lado,
    /// entao a pergunta nao e' "cabe no fundo daqui?" e sim "cabe no fundo
    /// de tudo o que ela alcanca?". Medindo so' o vertical o teste passava,
    /// com o defeito na tela.
    #[test]
    fn o_vale_da_onda_nao_fura_o_leito() {
        let ger = Gerador::da_ilha(&ARQUIPELAGO[0]);
        let passo = PASSO_FINO;
        // Alcance horizontal do Gerstner, em blocos, arredondado pra cima.
        let alcance = (AMPLITUDE_MAX / BLOCO).ceil() as i32;
        let mut pior: Option<(f32, f32, f32)> = None;
        for bz in (-900..900).step_by(passo as usize) {
            for bx in (-900..900).step_by(passo as usize) {
                let prof = profundidade(&ger, bx, bz);
                if prof <= 0.0 {
                    continue;
                }
                let a = onda_que_cabe(&ger, bx, bz, passo);
                if a <= 0.0 {
                    continue;
                }
                let vale = ALTURA_DA_AGUA - AMPLITUDE_MAX * a;
                for dz in -alcance..=alcance {
                    for dx in -alcance..=alcance {
                        let viz = profundidade(&ger, bx + dx, bz + dz);
                        let folga = vale - (NIVEL_DO_MAR - viz);
                        if pior.is_none_or(|(f, _, _)| folga < f) {
                            pior = Some((folga, prof, viz));
                        }
                    }
                }
            }
        }
        let (folga, meu, viz) = pior.expect("ha' agua em volta da ilha");
        assert!(
            folga > 0.0,
            "o vale fura o leito por {:.2} u: vertice em fundo {meu:.2} passeia \
             sobre leito de {viz:.2} — o chao fica exposto por dentro da agua",
            -folga
        );
    }

    /// E a onda tem que CONTINUAR existindo: limitar demais daria um mar
    /// parado, que e' trocar um defeito por outro.
    #[test]
    fn o_mar_aberto_continua_ondulando() {
        let ger = Gerador::da_ilha(&ARQUIPELAGO[0]);
        let cheias = (-900..900)
            .step_by(7)
            .flat_map(|bz| (-900..900).step_by(7).map(move |bx| (bx, bz)))
            .filter(|(bx, bz)| profundidade(&ger, *bx, *bz) > 0.0)
            .filter(|(bx, bz)| onda_que_cabe(&ger, *bx, *bz, PASSO_FINO) > 0.9)
            .count();
        assert!(
            cheias > 200,
            "so' {cheias} pontos com onda cheia — o limite apagou o mar"
        );
    }
}
