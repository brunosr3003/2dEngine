//! REBIRTH in 3D (`shared::renascimento`, docs/RESET.md): the always-on body
//! aura by rebirth tier, and the celebration of the moment of a rebirth.
//!
//! **The aura** (`personagem` + `desenha`): a light full-body glow, DBZ
//! style — flame tongues licking up round the body, a ring on the ground and,
//! close by, a few motes rising. Tier by rebirths (`tier_da_aura`): green
//! 1-2, blue 3-4, purple 5-9, gold 10-14, and from 15 the BLACK HOLE: dark
//! tongues drawn with normal blending (they darken what is behind) inside a
//! violet additive rim. From 20 it shines brighter every 5
//! (`intensidade_da_aura`). Cheap on purpose: queued per player, drawn in one
//! batch, culled past 38 u like the gear auras (`auras.rs`), fewer tongues
//! past 18 u, and at most `ORCAMENTO` bodies a frame (nearest first).
//!
//! **The celebration** (`Renasceu`): bigger than the level-up burst
//! (`nivel_vfx`) and scaled by tier, so the tenth rebirth still feels like
//! an event: more rings, sparks and a taller column at higher tiers, the
//! aura's colour, and on a milestone (`e_marco`) a shock wave, double the
//! sparks and a banner naming what it unlocked.

use macroquad::material::{gl_use_default_material, gl_use_material, load_material, Material, MaterialParams};
use macroquad::miniquad::{BlendFactor, BlendState, BlendValue, Comparison, Equation, PipelineParams};
use macroquad::prelude::*;
use std::cell::RefCell;
use std::f32::consts::TAU;

use shared::renascimento as rn;

/// Beyond this distance from the camera's target nobody's aura is drawn.
const ALCANCE: f32 = 38.0;
/// Within this, the full aura (more tongues, motes).
const PERTO: f32 = 18.0;
/// Bodies with an aura drawn per frame.
const ORCAMENTO: usize = 24;

#[derive(Clone, Copy)]
struct Corpo {
    pe: Vec3,
    altura: f32,
    r: u8,
    fase: f32,
    distancia: f32,
}

thread_local! {
    static FILA: RefCell<Vec<Corpo>> = const { RefCell::new(Vec::new()) };
    static MATERIAIS: RefCell<Option<(Material, Material)>> = const { RefCell::new(None) };
}

/// The colour of an aura tier (1..=5): the glow, and for the black hole its
/// violet rim. RGB 0..1.
pub fn cor_do_tier(tier: u8) -> Color {
    match tier {
        1 => Color::new(0.29, 0.92, 0.48, 1.0),
        2 => Color::new(0.26, 0.62, 1.0, 1.0),
        3 => Color::new(0.73, 0.37, 1.0, 1.0),
        4 => Color::new(1.0, 0.80, 0.28, 1.0),
        _ => Color::new(0.66, 0.30, 1.0, 1.0),
    }
}

/// The colour of `r` rebirths' aura (white-gold before the first).
pub fn cor_de(r: u32) -> Color {
    match rn::tier_da_aura(r) {
        0 => Color::new(1.0, 0.90, 0.62, 1.0),
        t => cor_do_tier(t),
    }
}

/// Queues the aura of a player with `r` rebirths standing at `pe`, its head
/// at `altura` above. `distancia` = from the camera target (0 for oneself).
pub fn personagem(r: u8, pe: Vec3, altura: f32, distancia: f32, id: u32) {
    if r == 0 || distancia > ALCANCE {
        return;
    }
    FILA.with(|f| {
        f.borrow_mut().push(Corpo {
            pe,
            altura: altura.clamp(0.8, 4.0),
            r,
            fase: id as f32 * 0.618,
            distancia,
        })
    });
}

/// Draws (and empties) the queue. `olho` = the camera position, for the
/// tongues to face it.
pub fn desenha(olho: Vec3) {
    FILA.with(|fila| {
        let mut fila = fila.borrow_mut();
        if fila.is_empty() {
            return;
        }
        fila.sort_by(|a, b| a.distancia.total_cmp(&b.distancia));
        fila.truncate(ORCAMENTO);
        MATERIAIS.with(|m| {
            let mut m = m.borrow_mut();
            let (brilho, escuro) = m.get_or_insert_with(|| (material(true), material(false)));
            let t = get_time() as f32;
            // Black holes first, with normal blending: their dark core must
            // be under the additive rim.
            let mut malha = Malha::default();
            for c in fila.iter().filter(|c| rn::tier_da_aura(c.r as u32) == 5) {
                nucleo_escuro(&mut malha, c, olho, t);
            }
            if !malha.v.is_empty() {
                gl_use_material(escuro);
                malha.descarrega();
            }
            gl_use_material(brilho);
            for c in fila.iter() {
                aura(&mut malha, c, olho, t);
                if malha.v.len() > 3000 {
                    malha.descarrega();
                }
            }
            malha.descarrega();
            gl_use_default_material();
        });
        fila.clear();
    });
}

/// `aditivo` = glow (adds light); otherwise normal alpha blending (darkens).
/// Depth-tested against the bodies, never writing depth (it is see-through).
fn material(aditivo: bool) -> Material {
    load_material(
        ShaderSource::Glsl {
            vertex: r#"#version 100
            attribute vec3 position; attribute vec4 color0;
            uniform mat4 Model; uniform mat4 Projection;
            varying lowp vec4 cor;
            void main() { gl_Position = Projection * Model * vec4(position, 1.0); cor = color0 / 255.0; }"#,
            fragment: r#"#version 100
            varying lowp vec4 cor;
            void main() { if (cor.a <= 0.001) discard; gl_FragColor = cor; }"#,
        },
        MaterialParams {
            pipeline_params: PipelineParams {
                depth_test: Comparison::LessOrEqual,
                depth_write: false,
                color_blend: Some(BlendState::new(
                    Equation::Add,
                    BlendFactor::Value(BlendValue::SourceAlpha),
                    if aditivo {
                        BlendFactor::One
                    } else {
                        BlendFactor::OneMinusValue(BlendValue::SourceAlpha)
                    },
                )),
                ..Default::default()
            },
            ..Default::default()
        },
    )
    .expect("rebirth aura material")
}

#[derive(Default)]
struct Malha {
    v: Vec<Vertex>,
    i: Vec<u16>,
}

impl Malha {
    fn quad(&mut self, p: [Vec3; 4], c: [Color; 4]) {
        let b = self.v.len() as u16;
        for k in 0..4 {
            self.v.push(Vertex { position: p[k], uv: Vec2::ZERO, color: c[k].into(), normal: Vec4::ZERO });
        }
        self.i.extend_from_slice(&[b, b + 1, b + 2, b, b + 2, b + 3]);
    }

    fn descarrega(&mut self) {
        if self.v.is_empty() {
            return;
        }
        draw_mesh(&Mesh { vertices: std::mem::take(&mut self.v), indices: std::mem::take(&mut self.i), texture: None });
    }
}

fn com_alfa(c: Color, a: f32) -> Color {
    Color::new(c.r, c.g, c.b, a.clamp(0.0, 1.0))
}

/// The sideways unit vector of a billboard at `p` facing `olho` (flat).
fn lado(p: Vec3, olho: Vec3) -> Vec3 {
    let d = vec3(olho.x - p.x, 0.0, olho.z - p.z).normalize_or_zero();
    let l = d.cross(Vec3::Y).normalize_or_zero();
    if l.length_squared() < 0.1 {
        Vec3::X
    } else {
        l
    }
}

/// One flame tongue: a camera-facing strip from `base` up `h`, wide at the
/// bottom, a point at the top, bright at the root and fading out.
fn lingua(m: &mut Malha, base: Vec3, h: f32, larg: f32, olho: Vec3, cor: Color, a: f32, curva: f32) {
    let l = lado(base, olho);
    let meio = base + Vec3::Y * h * 0.5 + l * curva * 0.5;
    let topo = base + Vec3::Y * h + l * curva;
    let (c0, c1) = (com_alfa(cor, a), com_alfa(cor, a * 0.45));
    let c2 = com_alfa(cor, 0.0);
    m.quad([base - l * larg, base + l * larg, meio + l * larg * 0.6, meio - l * larg * 0.6], [c0, c0, c1, c1]);
    m.quad([meio - l * larg * 0.6, meio + l * larg * 0.6, topo + l * 0.01, topo - l * 0.01], [c1, c1, c2, c2]);
}

/// The ring of light on the ground.
fn anel_no_chao(m: &mut Malha, centro: Vec3, raio: f32, esp: f32, cor: Color, a: f32, lados: usize) {
    let c = centro + Vec3::Y * 0.04;
    for k in 0..lados {
        let (a0, a1) = (k as f32 / lados as f32 * TAU, (k + 1) as f32 / lados as f32 * TAU);
        let d0 = vec3(a0.cos(), 0.0, a0.sin());
        let d1 = vec3(a1.cos(), 0.0, a1.sin());
        let (fora, dentro) = (com_alfa(cor, 0.0), com_alfa(cor, a));
        m.quad([c + d0 * (raio - esp), c + d1 * (raio - esp), c + d1 * raio, c + d0 * raio], [fora, fora, dentro, dentro]);
        m.quad([c + d0 * raio, c + d1 * raio, c + d1 * (raio + esp), c + d0 * (raio + esp)], [dentro, dentro, fora, fora]);
    }
}

fn aura(m: &mut Malha, c: &Corpo, olho: Vec3, t: f32) {
    let r = c.r as u32;
    let tier = rn::tier_da_aura(r);
    let forca = rn::intensidade_da_aura(r);
    let cor = cor_do_tier(tier);
    let perto = c.distancia < PERTO;
    // Higher tiers: more and taller tongues.
    let n = if perto { 8 + 2 * tier as usize } else { 5 + tier as usize };
    let raio = 0.36 + 0.03 * tier as f32;
    let pulso = 0.85 + 0.15 * (t * 2.3 + c.fase).sin();
    let a = (0.16 + 0.03 * tier as f32) * forca.min(2.0) * pulso;
    for k in 0..n {
        let u = k as f32 / n as f32;
        let ang = u * TAU + t * 0.55 + c.fase;
        let base = c.pe + vec3(ang.cos() * raio, 0.02, ang.sin() * raio);
        let tremor = (t * 7.0 + k as f32 * 1.7 + c.fase).sin() * 0.5 + 0.5;
        let h = c.altura * (0.70 + 0.25 * tremor) * (0.9 + 0.1 * tier as f32).min(1.4) * forca.sqrt();
        let curva = (t * 3.1 + k as f32).sin() * 0.10;
        lingua(m, base, h, 0.13 + 0.02 * tier as f32, olho, cor, a, curva);
    }
    // A white-hot inner layer from gold on (it reads as "stronger").
    if tier >= 4 && perto {
        let branco = if tier == 5 { Color::new(0.85, 0.65, 1.0, 1.0) } else { Color::new(1.0, 0.97, 0.80, 1.0) };
        for k in 0..4 {
            let ang = k as f32 / 4.0 * TAU - t * 0.8 + c.fase;
            let base = c.pe + vec3(ang.cos() * raio * 0.55, 0.02, ang.sin() * raio * 0.55);
            lingua(m, base, c.altura * 0.75 * forca.sqrt(), 0.09, olho, branco, a * 0.6, 0.0);
        }
    }
    anel_no_chao(m, c.pe, raio + 0.22, 0.10 + 0.02 * tier as f32, cor, a * 1.4, if perto { 24 } else { 12 });
    if perto {
        let motas = 3 + tier as usize * 2;
        for k in 0..motas {
            let s = (t * 0.5 + k as f32 * 0.618 + c.fase).fract();
            let ang = k as f32 * 2.399 + t * 0.9 + c.fase;
            let p = c.pe + vec3(ang.cos() * raio * 1.2, s * c.altura * 1.2, ang.sin() * raio * 1.2);
            let tam = 0.035 * (s * std::f32::consts::PI).sin() * forca.min(2.0);
            let l = lado(p, olho);
            let ca = com_alfa(cor, 0.85 * pulso);
            m.quad([p - l * tam, p + Vec3::Y * tam * 2.0, p + l * tam, p - Vec3::Y * tam * 2.0], [ca, ca, ca, ca]);
        }
    }
}

/// The BLACK HOLE's dark core: tongues of near-black, normal blending.
fn nucleo_escuro(m: &mut Malha, c: &Corpo, olho: Vec3, t: f32) {
    let forca = rn::intensidade_da_aura(c.r as u32);
    let escuro = Color::new(0.05, 0.0, 0.10, 1.0);
    let n = if c.distancia < PERTO { 10 } else { 6 };
    for k in 0..n {
        let ang = k as f32 / n as f32 * TAU - t * 0.4 + c.fase;
        let base = c.pe + vec3(ang.cos() * 0.30, 0.02, ang.sin() * 0.30);
        let tremor = (t * 5.0 + k as f32 * 2.1).sin() * 0.5 + 0.5;
        lingua(m, base, c.altura * (0.65 + 0.25 * tremor), 0.17, olho, escuro, 0.45 * forca.min(1.5), 0.0);
    }
    // The dark disc it stands on.
    let centro = c.pe + Vec3::Y * 0.035;
    let lados = 20;
    for k in 0..lados {
        let (a0, a1) = (k as f32 / lados as f32 * TAU, (k + 1) as f32 / lados as f32 * TAU);
        let (d0, d1) = (vec3(a0.cos(), 0.0, a0.sin()) * 0.85, vec3(a1.cos(), 0.0, a1.sin()) * 0.85);
        let (dentro, fora) = (com_alfa(escuro, 0.6), com_alfa(escuro, 0.0));
        m.quad([centro, centro, centro + d1, centro + d0], [dentro, dentro, fora, fora]);
    }
}

// ───────────────────────── the moment of rebirth ─────────────────────────

/// How long the celebration lasts: longer than the level-up's 3 s.
pub const DURACAO: f32 = 4.5;
/// Fraction of the effect the banner stays.
pub const FIM_DA_FAIXA: f32 = 0.8;

/// The celebration in progress. `n` = the rebirth just reached.
#[derive(Debug, Default)]
pub struct Renasceu {
    t: f32,
    ativo: bool,
    n: u32,
}

impl Renasceu {
    pub fn dispara(&mut self, n: u32) {
        self.t = 0.0;
        self.ativo = true;
        self.n = n;
    }

    pub fn passo(&mut self, dt: f32) {
        if self.ativo {
            self.t += dt;
            if self.t >= DURACAO {
                self.ativo = false;
            }
        }
    }

    pub fn ativo(&self) -> bool {
        self.ativo
    }

    pub fn fase(&self) -> Option<f32> {
        self.ativo.then(|| (self.t / DURACAO).clamp(0.0, 1.0))
    }

    /// How grand: 1 + the aura tier, +2 on a milestone. Drives the rings,
    /// sparks and column.
    pub fn grandeza(&self) -> u32 {
        1 + rn::tier_da_aura(self.n) as u32 + if rn::e_marco(self.n) { 2 } else { 0 }
    }

    /// The world burst at the feet. Additive glow: the caller has the world
    /// camera set.
    pub fn desenha(&self, pe: Vec3, olho: Vec3) {
        let Some(f) = self.fase() else { return };
        MATERIAIS.with(|m| {
            let mut m = m.borrow_mut();
            let (brilho, _) = m.get_or_insert_with(|| (material(true), material(false)));
            gl_use_material(brilho);
        });
        let g = self.grandeza();
        let cor = cor_de(self.n);
        let ouro = Color::new(1.0, 0.86, 0.50, 1.0);
        let mut malha = Malha::default();
        let raio_max = 5.0 + 0.6 * g as f32;
        // Rings: 3 + grandeur, staggered, alternating the aura colour and gold.
        let aneis = 3 + g as usize;
        for k in 0..aneis {
            let atraso = k as f32 * 0.55 / aneis as f32;
            let u = (f - atraso) / (1.0 - atraso);
            if !(0.0..1.0).contains(&u) {
                continue;
            }
            let c = if k % 2 == 0 { cor } else { ouro };
            anel_no_chao(&mut malha, pe, raio_max * suave(u), 0.45 * (1.0 - u).max(0.12), c, brilho_de(u), 32);
        }
        // Milestone: one huge, slow shock wave.
        if rn::e_marco(self.n) {
            let u = (f / 0.7).min(1.0);
            if u < 1.0 {
                anel_no_chao(&mut malha, pe, raio_max * 2.2 * suave(u), 0.9 * (1.0 - u), cor, 0.8 * (1.0 - u), 40);
            }
        }
        // The column: taller and wider with grandeur, in the aura's colour.
        let a = brilho_de(f * 0.8);
        if a > 0.0 {
            let h = (8.0 + 1.2 * g as f32) * (0.45 + 0.55 * suave(f * 1.6));
            let l = lado(pe, olho) * (1.0 + 0.12 * g as f32);
            let (c0, c1) = (com_alfa(Color::new(1.0, 0.98, 0.9, 1.0), 0.8 * a), com_alfa(cor, 0.0));
            malha.quad([pe - l, pe + l, pe + l * 0.25 + Vec3::Y * h, pe - l * 0.25 + Vec3::Y * h], [c0, c0, c1, c1]);
            let (c0, c1) = (com_alfa(cor, 0.7 * a), com_alfa(cor, 0.0));
            malha.quad([pe - l * 1.8, pe + l * 1.8, pe + l * 0.5 + Vec3::Y * h * 0.8, pe - l * 0.5 + Vec3::Y * h * 0.8], [c0, c0, c1, c1]);
        }
        // Sparks: 40 + 15 per grandeur, spiralling up.
        let fagulhas = 40 + 15 * g as usize;
        for i in 0..fagulhas {
            let atraso = (i as f32 * 0.618_034).fract() * 0.4;
            let u = (f - atraso) / (1.0 - atraso);
            if !(0.0..1.0).contains(&u) {
                continue;
            }
            let giro = i as f32 * 2.399_963 + u * 4.0;
            let r = 0.6 + (1.4 + 0.15 * g as f32) * u;
            let p = pe + vec3(giro.cos() * r, (6.0 + 0.5 * g as f32) * suave(u) + 0.2, giro.sin() * r);
            let s = 0.15 * (1.0 - u).max(0.25);
            let c = com_alfa(if i % 3 == 0 { ouro } else { cor }, brilho_de(u));
            let l = lado(p, olho);
            malha.quad([p - l * s, p + Vec3::Y * s, p + l * s, p - Vec3::Y * s], [c, c, c, c]);
            if malha.v.len() > 3000 {
                malha.descarrega();
            }
        }
        malha.descarrega();
        gl_use_default_material();
    }
}

/// The banner: "REBIRTH N" over "REBORN!", and on a milestone what it
/// unlocked. Bigger than the level-up's, in the aura's colour.
pub fn desenha_faixa(e: &Renasceu) {
    let Some(f) = e.fase() else { return };
    let vis = 1.0 - (f / FIM_DA_FAIXA).clamp(0.0, 1.0);
    if vis <= 0.0 {
        return;
    }
    let est = crate::hud_estilo::u;
    let seguro = crate::hud_layout::tela_segura();
    let y = seguro.y + seguro.h * 0.28 - (1.0 - vis) * est(26.0);
    let a = vis.powf(0.7);
    let pop = if f < 0.1 { 1.0 + 0.6 * (1.0 - f / 0.1) } else { 1.0 };
    let tam = ((52.0 + 2.0 * e.grandeza() as f32) * pop) as u16;
    let texto = format!("REBIRTH {}", e.n);
    let cx = seguro.center().x;
    let cor = cor_de(e.n);
    let cima = y - est(40.0);
    let sombra = Color::new(0.0, 0.0, 0.0, 0.55 * a);
    crate::hud_estilo::texto_centro_forte(cx + 2.0, cima + 2.0, "REBORN!", 22, sombra);
    crate::hud_estilo::texto_centro_forte(cx, cima, "REBORN!", 22, Color::new(1.0, 0.95, 0.80, a));
    for (dx, dy) in [(-4.0, 0.0), (4.0, 0.0), (0.0, -4.0), (0.0, 4.0)] {
        crate::hud_estilo::texto_centro_forte(cx + dx, y + dy, &texto, tam, com_alfa(cor, 0.25 * a));
    }
    crate::hud_estilo::texto_centro_forte(cx + 2.0, y + 2.0, &texto, tam, sombra);
    crate::hud_estilo::texto_centro_forte(cx, y, &texto, tam, com_alfa(crate::hud_estilo::misturar(cor, WHITE, 0.35), a));
    let baixo = y + est(36.0);
    let linha = format!("+{}% to every attribute", rn::bonus_em_pct(e.n));
    crate::hud_estilo::texto_centro_forte(cx, baixo, &linha, 17, Color::new(1.0, 0.92, 0.74, a));
    if let Some(m) = rn::marco_de(e.n) {
        let m = format!("Milestone: {}", shared::idioma::tr(m));
        crate::hud_estilo::texto_centro_forte(cx + 1.0, baixo + est(28.0) + 1.0, &m, 19, sombra);
        crate::hud_estilo::texto_centro_forte(cx, baixo + est(28.0), &m, 19, com_alfa(cor, a));
    }
}

/// Bright at once, falling fast with a long faint tail (`nivel_vfx`).
fn brilho_de(f: f32) -> f32 {
    if !(0.0..1.0).contains(&f) {
        return 0.0;
    }
    (1.0 - f).powi(2)
}

fn suave(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn a_festa_dura_e_acaba() {
        let mut e = Renasceu::default();
        assert!(e.fase().is_none());
        e.dispara(3);
        e.passo(DURACAO * 0.5);
        assert!(e.ativo());
        e.passo(DURACAO);
        assert!(!e.ativo() && e.fase().is_none());
        assert!(DURACAO > crate::nivel_vfx::DURACAO, "bigger than a level-up");
    }

    /// It keeps growing over many rebirths, and a milestone is grander than
    /// the rebirth after it.
    #[test]
    fn a_festa_cresce_com_o_tier_e_no_marco() {
        let g = |n| {
            let mut e = Renasceu::default();
            e.dispara(n);
            e.grandeza()
        };
        assert!(g(10) > g(5) && g(5) > g(3) && g(3) > g(1));
        assert!(g(10) > g(11), "milestone 10 is grander than 11");
        assert!(g(15) > g(14));
    }

    #[test]
    fn cada_tier_tem_sua_cor() {
        let cores: Vec<Color> = (1..=5).map(cor_do_tier).collect();
        for i in 0..cores.len() {
            for j in i + 1..cores.len() {
                assert_ne!(cores[i], cores[j]);
            }
        }
    }
}
