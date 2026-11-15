//! **Healing you can see**: the out-of-combat regen glow and the potion
//! effects (owner, 15/11/2026: "coloca efeitos visuais tbm pra isso e pra
//! poção").
//!
//! * **Regen** (out of combat, HP below max and rising): a few soft green
//!   motes drifting up around the body and a faint pulse on the ground. Calm
//!   on purpose — it is on screen for long stretches.
//! * **Potion** (Life red, Mana blue): a burst when it is drunk (a ring on the
//!   ground and a puff of sparks) and, while the heal runs, motes and little
//!   "+" crosses spiralling up.
//!
//! Local player only, vertex colour on the world material like `nivel_vfx`:
//! no shader, no texture, a few dozen quads.

use macroquad::prelude::*;

/// How long the regen glow lingers after the last HP tick, in seconds.
const REGEN_SEGURA_S: f64 = 1.5;
/// How long the drink burst lasts.
const GOLE_S: f32 = 0.8;

const VERDE: [f32; 3] = [120.0, 235.0, 140.0];
const VERMELHO: [f32; 3] = [255.0, 92.0, 110.0];
const AZUL: [f32; 3] = [96.0, 168.0, 255.0];
const BRANCO: [f32; 3] = [255.0, 250.0, 235.0];

#[derive(Debug, Default)]
pub struct Cura {
    ultimo_hp: u32,
    regen_ate: f64,
    /// (heal ends at, burst started at) per group: 0 Life, 1 Mana.
    pocao: [(f64, f64); 2],
}

impl Cura {
    /// A `PocaoGrupo` from the server. A heal that starts (or lengthens past
    /// what was already running) is a fresh drink: it fires the burst.
    pub fn pocao(&mut self, grupo: u8, cura_s: f32, agora: f64) {
        let Some(p) = self.pocao.get_mut(grupo as usize) else { return };
        if cura_s <= 0.0 {
            p.0 = p.0.min(agora);
            return;
        }
        let fim = agora + cura_s as f64;
        if fim > p.0 + 0.25 {
            p.1 = agora;
        }
        p.0 = fim;
    }

    /// Every frame, with the player's own HP and combat bit.
    pub fn observa(&mut self, hp: u32, hp_max: u32, em_combate: bool, agora: f64) {
        if hp > self.ultimo_hp && self.ultimo_hp > 0 && !em_combate && hp <= hp_max {
            self.regen_ate = agora + REGEN_SEGURA_S;
        }
        if em_combate || hp == 0 {
            self.regen_ate = 0.0;
        }
        self.ultimo_hp = hp;
    }

    pub fn ativo(&self, agora: f64) -> bool {
        agora < self.regen_ate
            || self.pocao.iter().any(|&(fim, gole)| agora < fim || agora - gole < GOLE_S as f64)
    }

    /// Draws at the player's feet. The caller has set the world material.
    pub fn desenha(&self, pe: Vec3, olho: Vec3, agora: f64) {
        let t = agora as f32;
        let lado = {
            let para = vec3(olho.x - pe.x, 0.0, olho.z - pe.z).normalize_or_zero();
            para.cross(Vec3::Y).normalize_or_zero()
        };
        if lado.length_squared() < 0.1 {
            return;
        }
        let pocao_ativa = self.pocao.iter().any(|&(fim, _)| agora < fim);
        if agora < self.regen_ate && !pocao_ativa {
            let a = ((self.regen_ate - agora) / REGEN_SEGURA_S).clamp(0.0, 1.0) as f32;
            regen(pe, lado, t, a);
        }
        for (g, &(fim, gole)) in self.pocao.iter().enumerate() {
            let cor = if g == 0 { VERMELHO } else { AZUL };
            let desde = (agora - gole) as f32;
            if (0.0..GOLE_S).contains(&desde) {
                gole_burst(pe, lado, desde / GOLE_S, cor);
            }
            if agora < fim {
                let a = ((fim - agora) as f32).min(1.0);
                curando(pe, lado, t, a, cor);
            }
        }
    }
}

/// Six slow green motes rising on a loop, and a soft ground pulse.
fn regen(pe: Vec3, lado: Vec3, t: f32, a: f32) {
    for i in 0..6 {
        let u = (t * 0.45 + i as f32 / 6.0).fract();
        let ang = i as f32 * 1.047 + t * 0.6;
        let p = pe + vec3(ang.cos() * 0.6, 0.3 + u * 2.0, ang.sin() * 0.6);
        let alfa = (a * (1.0 - u) * (u * 4.0).min(1.0) * 200.0) as u8;
        quad(p, lado, 0.09, cor(VERDE, alfa));
    }
    let pulso = (t * 1.6).sin() * 0.5 + 0.5;
    anel(pe, 0.7 + 0.15 * pulso, 0.12, cor(VERDE, (a * 70.0 * (0.5 + pulso * 0.5)) as u8));
}

/// The drink: a ring opening on the ground and a puff of sparks.
fn gole_burst(pe: Vec3, lado: Vec3, f: f32, c: [f32; 3]) {
    let a = (1.0 - f).powi(2);
    anel(pe, 0.4 + 1.6 * f, 0.25 * (1.0 - f).max(0.2), cor(c, (220.0 * a) as u8));
    for i in 0..14 {
        let ang = i as f32 * 2.399963;
        let r = 0.3 + 1.1 * f;
        let p = pe + vec3(ang.cos() * r, 0.5 + 1.6 * f + (i % 3) as f32 * 0.25, ang.sin() * r);
        quad(p, lado, 0.11 * (1.0 - f).max(0.3), cor(BRANCO, (240.0 * a) as u8));
    }
}

/// While the heal runs: motes and "+" crosses spiralling up.
fn curando(pe: Vec3, lado: Vec3, t: f32, a: f32, c: [f32; 3]) {
    for i in 0..8 {
        let u = (t * 0.7 + i as f32 / 8.0).fract();
        let ang = i as f32 * 0.785 + t * 2.2 + u * 3.0;
        let r = 0.75 - 0.25 * u;
        let p = pe + vec3(ang.cos() * r, 0.2 + u * 2.4, ang.sin() * r);
        let alfa = (a * (1.0 - u) * (u * 5.0).min(1.0) * 230.0) as u8;
        if i % 2 == 0 {
            cruz(p, lado, 0.14, cor(c, alfa));
        } else {
            quad(p, lado, 0.08, cor(BRANCO, alfa));
        }
    }
}

fn cor(base: [f32; 3], alfa: u8) -> [u8; 4] {
    [base[0] as u8, base[1] as u8, base[2] as u8, alfa]
}

fn vtx(p: Vec3, c: [u8; 4]) -> Vertex {
    Vertex { position: p, uv: vec2(0.0, 0.0), color: c, normal: Vec4::ZERO }
}

/// A camera-facing square.
fn quad(p: Vec3, lado: Vec3, s: f32, c: [u8; 4]) {
    if c[3] == 0 {
        return;
    }
    let (l, u) = (lado * s, Vec3::Y * s);
    dupla(
        vec![vtx(p - l - u, c), vtx(p + l - u, c), vtx(p + l + u, c), vtx(p - l + u, c)],
        vec![[0, 1, 2], [0, 2, 3]],
    );
}

/// A camera-facing "+".
fn cruz(p: Vec3, lado: Vec3, s: f32, c: [u8; 4]) {
    if c[3] == 0 {
        return;
    }
    let e = s * 0.33;
    let (l, u) = (lado, Vec3::Y);
    dupla(
        vec![
            vtx(p - l * s - u * e, c),
            vtx(p + l * s - u * e, c),
            vtx(p + l * s + u * e, c),
            vtx(p - l * s + u * e, c),
            vtx(p - l * e - u * s, c),
            vtx(p + l * e - u * s, c),
            vtx(p + l * e + u * s, c),
            vtx(p - l * e + u * s, c),
        ],
        vec![[0, 1, 2], [0, 2, 3], [4, 5, 6], [4, 6, 7]],
    );
}

/// A flat ring on the ground, soft on both edges.
fn anel(pe: Vec3, r: f32, esp: f32, c: [u8; 4]) {
    if c[3] == 0 {
        return;
    }
    const LADOS: usize = 24;
    let centro = vec3(pe.x, pe.y + 0.05, pe.z);
    let fora = [c[0], c[1], c[2], 0];
    let (mut v, mut tris) = (Vec::new(), Vec::new());
    for k in 0..=LADOS {
        let ang = k as f32 / LADOS as f32 * std::f32::consts::TAU;
        let d = vec3(ang.cos(), 0.0, ang.sin());
        v.push(vtx(centro + d * (r - esp).max(0.0), fora));
        v.push(vtx(centro + d * r, c));
        v.push(vtx(centro + d * (r + esp), fora));
        if k > 0 {
            let (p, q) = (((k - 1) * 3) as u16, (k * 3) as u16);
            tris.push([p, q, p + 1]);
            tris.push([q, q + 1, p + 1]);
            tris.push([p + 1, q + 1, p + 2]);
            tris.push([q + 1, q + 2, p + 2]);
        }
    }
    dupla(v, tris);
}

fn dupla(vertices: Vec<Vertex>, tris: Vec<[u16; 3]>) {
    let mut indices = Vec::with_capacity(tris.len() * 6);
    for [a, b, c] in tris {
        indices.extend_from_slice(&[a, b, c, a, c, b]);
    }
    draw_mesh(&Mesh { vertices, indices, texture: None });
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn regen_so_fora_de_combate_e_apaga() {
        let mut c = Cura::default();
        c.observa(50, 100, false, 0.0);
        c.observa(51, 100, false, 1.0);
        assert!(c.ativo(1.1), "rising HP out of combat glows");
        assert!(!c.ativo(1.0 + REGEN_SEGURA_S + 0.1), "and fades");
        c.observa(52, 100, true, 3.0);
        assert!(!c.ativo(3.1), "in combat, no regen glow");
    }

    #[test]
    fn pocao_dispara_gole_e_dura_a_cura() {
        let mut c = Cura::default();
        c.pocao(0, 5.0, 10.0);
        assert!(c.ativo(10.1) && c.ativo(14.9));
        assert!(!c.ativo(15.1));
        // A refusal re-sends the same remaining heal: no new burst.
        c.pocao(1, 4.0, 20.0);
        let gole = c.pocao[1].1;
        c.pocao(1, 3.0, 21.0);
        assert_eq!(c.pocao[1].1, gole);
        c.pocao(9, 5.0, 0.0); // unknown group: ignored
    }
}
