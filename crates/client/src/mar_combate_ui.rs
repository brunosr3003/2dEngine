//! COMBAT AT SEA YOU CAN READ (docs/NAVEGACAO.md, "gameplay remake",
//! 16/11/2026). The owner: "o combate tá péssimo".
//!
//! Everything the sea fight shows, beside the hull itself (that is
//! `mar_ui`):
//!
//! - **the target**: a big ring on the water and its life bar;
//! - **the Broadside's arcs**: the two side cones on the water while a
//!   target is near — green when it would hit, grey when not;
//! - **the Bow Cannon**: an aim line to the target and the range ring;
//! - **the shot**: muzzle flashes along the side, smoke, the cannonball on
//!   its arc for its REAL flight time (`navio::tempo_de_voo`, the same the
//!   server lands it with), a splash on a miss and a burst of splinters on
//!   a hit, and a shake for the shooter;
//! - **the sea mobs**: fins and eel humps breaking the surface (the bodies
//!   swim under the waves), the RED RING of a bite winding up
//!   (`navio::MORDIDA_PREPARO_S`), the bite's splinters on your hull;
//!   elites bigger and with a name plate;
//! - **the ship panel**: hull, speed, the zone (safe, open, lawless in
//!   red), the treasure, and the nearest dock (the docks' own labels and
//!   off-screen chevrons are the scene's, `mar_cena::rotulos`).
//!
//! MOBILE FIRST (the owner: "o mar precisa ser o mais otimizado possível").
//! Every effect lives in a FIXED POOL (`MAX_BALAS`, `MAX_PARTICULAS`): a
//! new one overwrites the oldest, nothing is allocated per frame. All the
//! water decals of a frame go into ONE reused mesh and one draw call; the
//! particles and balls are camera-facing quads in a second one — no
//! `draw_sphere`, which builds a sphere per call. Low graphics settings
//! (`config_graficos`) halve the particles. The HUD's strings are cached
//! and rebuilt only when what they show changes.

use macroquad::models::{Mesh, Vertex};
use macroquad::prelude::*;

use crate::hud_estilo as estilo;
use crate::world::World;
use shared::navio::{self as nv, Efeito};
use shared::{EntityId, EntityTag};

/// The point on screen (the capture target's size when there is one).
fn world_to_screen(cam: &Camera3D, p: Vec3) -> Option<Vec2> {
    let clip = cam.matrix() * p.extend(1.0);
    if clip.w <= 0.0 {
        return None;
    }
    let ndc = clip.truncate() / clip.w;
    let (w, h) = crate::render3d::tela();
    Some(vec2((ndc.x * 0.5 + 0.5) * w, (1.0 - (ndc.y * 0.5 + 0.5)) * h))
}

/// Cannonballs in the air at once.
pub const MAX_BALAS: usize = 24;
/// Smoke, splash, splinters and sparkles at once.
pub const MAX_PARTICULAS: usize = 192;
/// A vertex budget per batch, under u16 indices.
const MAX_VERTICES: usize = 60_000;
/// Reef Shark and Tiger Eel (`server/world/navio.rs`).
const TUBARAO: u16 = 70;
const ENGUIA: u16 = 71 + 1;

fn g(v: Vec2) -> ::glam::Vec2 {
    ::glam::Vec2::new(v.x, v.y)
}

/// Where the water decals lie: just over the highest crest, so a swell never
/// swallows a ring (seen from above the gap does not show).
fn y_agua() -> f32 {
    crate::agua::ALTURA_DA_AGUA + crate::agua::AMPLITUDE_MAX + 0.06
}

/// Where the auto path's dashed line is drawn at sea (`rastro` adds its own
/// lift over this).
pub fn altura_do_tracejado() -> f32 {
    crate::agua::ALTURA_DA_AGUA + crate::agua::AMPLITUDE_MAX
}

/// Fewer particles on the light graphics settings.
pub fn efeitos_leves() -> bool {
    crate::config_graficos::forracao() < 65 || crate::config_graficos::raio_terreno() <= 3
}

/// Is this enemy a sea elite (`[ELITE] Dread …`, `tornar_elite_do_mar`)?
pub fn e_elite(e: &crate::world::Ent) -> bool {
    e.meta.name.as_deref().is_some_and(|n| n.starts_with("[ELITE]"))
}

#[derive(Clone, Copy)]
struct Bala {
    ativa: bool,
    de: Vec3,
    para: Vec2,
    inicio: f64,
    voo: f32,
    /// The shooter is the local player (sound and shake).
    meu: bool,
    /// Who shot (a ball never "hits" its own ship).
    dono: EntityId,
}

impl Default for Bala {
    fn default() -> Self {
        Self { ativa: false, de: Vec3::ZERO, para: Vec2::ZERO, inicio: 0.0, voo: 0.0, meu: false, dono: EntityId(0) }
    }
}

#[derive(Clone, Copy, Default)]
struct Particula {
    vida: f32,
    idade: f32,
    pos: Vec3,
    vel: Vec3,
    tam: f32,
    cresce: f32,
    cor: [f32; 4],
    peso: f32,
}

/// One cached HUD line: rebuilt only when its key changes.
#[derive(Default)]
struct Linha {
    chave: u64,
    texto: String,
}

impl Linha {
    fn com(&mut self, chave: u64, f: impl FnOnce() -> String) -> &str {
        if self.chave != chave || self.texto.is_empty() {
            self.chave = chave;
            self.texto = f();
        }
        &self.texto
    }
}

/// The sea fight's on-screen state. Lives in `main` as one field.
pub struct MarCombate {
    balas: [Bala; MAX_BALAS],
    prox_bala: usize,
    parts: [Particula; MAX_PARTICULAS],
    prox_part: usize,
    decais: Mesh,
    quads: Mesh,
    /// The hull was hit: flash until.
    clarao_ate: f64,
    /// A shake for the local shooter, taken by `main` (`tremor`).
    tremor: Option<f32>,
    linha_casco: Linha,
    linha_info: Linha,
    linha_cais: Linha,
    linha_alvo: Linha,
}

impl Default for MarCombate {
    fn default() -> Self {
        let malha = || Mesh { vertices: Vec::with_capacity(4096), indices: Vec::with_capacity(6144), texture: None };
        Self {
            balas: [Bala::default(); MAX_BALAS],
            prox_bala: 0,
            parts: [Particula::default(); MAX_PARTICULAS],
            prox_part: 0,
            decais: malha(),
            quads: malha(),
            clarao_ate: 0.0,
            tremor: None,
            linha_casco: Linha::default(),
            linha_info: Linha::default(),
            linha_cais: Linha::default(),
            linha_alvo: Linha::default(),
        }
    }
}

// ─────────────────────────── mesh helpers ───────────────────────────

fn cor8(c: [f32; 4]) -> [u8; 4] {
    [
        (c[0].clamp(0.0, 1.0) * 255.0) as u8,
        (c[1].clamp(0.0, 1.0) * 255.0) as u8,
        (c[2].clamp(0.0, 1.0) * 255.0) as u8,
        (c[3].clamp(0.0, 1.0) * 255.0) as u8,
    ]
}

fn vtx(p: Vec3, cor: [u8; 4]) -> Vertex {
    Vertex { position: p, uv: Vec2::ZERO, color: cor, normal: Vec4::ZERO }
}

fn tri(m: &mut Mesh, a: Vec3, b: Vec3, c: Vec3, cor: [f32; 4]) {
    if m.vertices.len() + 3 > MAX_VERTICES {
        return;
    }
    let k = cor8(cor);
    let i = m.vertices.len() as u16;
    m.vertices.extend([vtx(a, k), vtx(b, k), vtx(c, k)]);
    m.indices.extend([i, i + 1, i + 2]);
}

fn quad(m: &mut Mesh, a: Vec3, b: Vec3, c: Vec3, d: Vec3, cor: [f32; 4]) {
    if m.vertices.len() + 4 > MAX_VERTICES {
        return;
    }
    let k = cor8(cor);
    let i = m.vertices.len() as u16;
    m.vertices.extend([vtx(a, k), vtx(b, k), vtx(c, k), vtx(d, k)]);
    m.indices.extend([i, i + 1, i + 2, i, i + 2, i + 3]);
}

/// A flat ring on the water (`n` segments).
fn anel(m: &mut Mesh, c: Vec2, r: f32, largura: f32, cor: [f32; 4], n: usize) {
    let y = y_agua();
    let (r0, r1) = ((r - largura * 0.5).max(0.0), r + largura * 0.5);
    for k in 0..n {
        let a0 = k as f32 / n as f32 * std::f32::consts::TAU;
        let a1 = (k + 1) as f32 / n as f32 * std::f32::consts::TAU;
        let p = |a: f32, r: f32| vec3(c.x + a.cos() * r, y, c.y + a.sin() * r);
        quad(m, p(a0, r0), p(a0, r1), p(a1, r1), p(a1, r0), cor);
    }
}

/// A flat sector of the disc between angles `a0`..`a1` (radians, world
/// x/z), from radius `r0` to `r1`.
fn setor(m: &mut Mesh, c: Vec2, r0: f32, r1: f32, a0: f32, a1: f32, cor: [f32; 4], n: usize) {
    let y = y_agua();
    for k in 0..n {
        let t0 = a0 + (a1 - a0) * k as f32 / n as f32;
        let t1 = a0 + (a1 - a0) * (k + 1) as f32 / n as f32;
        let p = |a: f32, r: f32| vec3(c.x + a.cos() * r, y, c.y + a.sin() * r);
        quad(m, p(t0, r0), p(t0, r1), p(t1, r1), p(t1, r0), cor);
    }
}

/// A flat strip on the water from `a` to `b`.
fn faixa(m: &mut Mesh, a: Vec2, b: Vec2, largura: f32, cor: [f32; 4]) {
    let y = y_agua() + 0.01;
    let d = (b - a).normalize_or_zero();
    let l = vec2(-d.y, d.x) * largura * 0.5;
    quad(m, vec3(a.x - l.x, y, a.y - l.y), vec3(a.x + l.x, y, a.y + l.y), vec3(b.x + l.x, y, b.y + l.y), vec3(b.x - l.x, y, b.y - l.y), cor);
}

/// A camera-facing hexagon (particles, balls).
fn disco(m: &mut Mesh, c: Vec3, r: f32, dir: Vec3, cima: Vec3, cor: [f32; 4]) {
    let p = |k: usize| {
        let a = k as f32 / 6.0 * std::f32::consts::TAU;
        c + dir * a.cos() * r + cima * a.sin() * r
    };
    for k in 0..6 {
        tri(m, c, p(k), p(k + 1), cor);
    }
}

// ─────────────────────────── events ───────────────────────────

impl MarCombate {
    fn particula(&mut self, p: Particula) {
        self.parts[self.prox_part] = p;
        self.prox_part = (self.prox_part + 1) % MAX_PARTICULAS;
    }

    fn bala(&mut self, b: Bala) {
        self.balas[self.prox_bala] = b;
        self.prox_bala = (self.prox_bala + 1) % MAX_BALAS;
    }

    /// The shake for the local shooter, once.
    pub fn tremor(&mut self) -> Option<f32> {
        self.tremor.take()
    }

    fn fumaca(&mut self, p: Vec3, dir: Vec3, n: usize) {
        let n = if efeitos_leves() { n.div_ceil(2) } else { n };
        for k in 0..n {
            let s = k as f32 * 1.7 + p.x * 0.31;
            self.particula(Particula {
                vida: 1.1 + (s.sin() * 0.5 + 0.5) * 0.6,
                idade: 0.0,
                pos: p,
                vel: dir * (1.6 + (s * 1.3).cos().abs() * 1.2) + vec3(s.cos() * 0.4, 0.7, s.sin() * 0.4),
                tam: 0.35,
                cresce: 0.9,
                cor: [0.86, 0.86, 0.84, 0.55],
                peso: -0.15,
            });
        }
    }

    fn clarao(&mut self, p: Vec3) {
        self.particula(Particula { vida: 0.12, idade: 0.0, pos: p, vel: Vec3::ZERO, tam: 0.55, cresce: 2.0, cor: [1.0, 0.78, 0.32, 0.95], peso: 0.0 });
    }

    fn respingo(&mut self, p: Vec2) {
        let y = y_agua();
        let n = if efeitos_leves() { 6 } else { 12 };
        for k in 0..n {
            let a = k as f32 / n as f32 * std::f32::consts::TAU;
            self.particula(Particula {
                vida: 0.75,
                idade: 0.0,
                pos: vec3(p.x, y, p.y),
                vel: vec3(a.cos() * 1.3, 4.2 + (k % 3) as f32 * 0.9, a.sin() * 1.3),
                tam: 0.2,
                cresce: -0.1,
                cor: [0.88, 0.95, 1.0, 0.85],
                peso: 9.0,
            });
        }
        // The foam ring on the water.
        self.particula(Particula { vida: 0.6, idade: 0.0, pos: vec3(p.x, y + 0.05, p.y), vel: Vec3::ZERO, tam: 0.6, cresce: 3.0, cor: [0.95, 0.98, 1.0, 0.45], peso: 0.0 });
    }

    fn lascas(&mut self, p: Vec3) {
        let n = if efeitos_leves() { 6 } else { 14 };
        for k in 0..n {
            let a = k as f32 * 2.399;
            let madeira = if k % 3 == 0 { [0.36, 0.22, 0.12, 1.0] } else { [0.58, 0.38, 0.2, 1.0] };
            self.particula(Particula {
                vida: 0.7,
                idade: 0.0,
                pos: p,
                vel: vec3(a.cos() * 3.2, 3.5 + (k % 4) as f32 * 0.8, a.sin() * 3.2),
                tam: 0.12,
                cresce: 0.0,
                cor: madeira,
                peso: 12.0,
            });
        }
        self.clarao(p);
    }

    /// A SHIP SKILL's FX arrived (`SkillImpactFx` with a ship id). `tier` is
    /// the ball's index in a volley (0 = the first: the flashes and the
    /// sound go once). `false` = not a ship skill.
    pub fn tiro(&mut self, world: &World, skill_id: u32, dono: EntityId, de: Vec2, mira: Vec2, tier: u8) -> bool {
        let Some(h) = nv::habilidade_do_id(skill_id) else {
            return false;
        };
        let agora = get_time();
        let meu = world.self_id == Some(dono);
        let yaw = world.ents.get(&dono).map_or(0.0, |e| e.yaw);
        let frente = vec2(yaw.sin(), yaw.cos());
        let lado = vec2(frente.y, -frente.x);
        let y = y_agua() + 0.9;
        let p3 = |p: Vec2| vec3(p.x, y, p.y);
        let ouvinte = world.self_pos();
        match h.efeito {
            Efeito::Bordada => {
                let sem_alvo = mira.distance(de) < 0.01;
                if tier == 0 {
                    // Both sides fire: three ports a side, flash and smoke.
                    for s in [-1.0f32, 1.0] {
                        for k in [-1.0f32, 0.0, 1.0] {
                            let porta = de + frente * (k * 0.9) + lado * s * 1.1;
                            self.clarao(p3(porta));
                            self.fumaca(p3(porta), vec3(lado.x * s, 0.0, lado.y * s), 2);
                            if sem_alvo {
                                // Into open water: the balls splash at range.
                                let longe = porta + lado * s * (h.alcance - 1.5 + k.abs());
                                self.bala(Bala { ativa: true, de: p3(porta), para: longe, inicio: agora, voo: nv::tempo_de_voo(porta.distance(longe)), meu, dono });
                            }
                        }
                    }
                    crate::sons::mundo(crate::sons::Som::CanhaoDisparo, de, ouvinte, meu);
                    if meu {
                        self.tremor = Some(0.16);
                    }
                }
                if !sem_alvo {
                    let s = if (mira - de).perp_dot(frente) >= 0.0 { -1.0 } else { 1.0 };
                    let porta = de + lado * s * 1.1;
                    self.bala(Bala { ativa: true, de: p3(porta), para: mira, inicio: agora, voo: nv::tempo_de_voo(de.distance(mira)), meu, dono });
                }
            }
            Efeito::Canhonaco => {
                let proa = de + frente * 2.2;
                self.clarao(p3(proa));
                self.fumaca(p3(proa), vec3(frente.x, 0.0, frente.y), 4);
                if mira.distance(de) > 0.01 {
                    self.bala(Bala { ativa: true, de: p3(proa), para: mira, inicio: agora, voo: nv::tempo_de_voo(de.distance(mira)), meu, dono });
                }
                crate::sons::mundo(crate::sons::Som::CanhaoDisparo, de, ouvinte, meu);
                if meu {
                    self.tremor = Some(0.1);
                }
            }
            Efeito::Reparo => {
                for k in 0..10 {
                    let a = k as f32 * 0.63;
                    self.particula(Particula { vida: 1.1, idade: 0.0, pos: p3(de) + vec3(a.cos() * 1.3, 0.0, a.sin() * 1.3), vel: vec3(0.0, 1.6, 0.0), tam: 0.16, cresce: 0.0, cor: [0.5, 0.96, 0.55, 0.95], peso: 0.0 });
                }
            }
            Efeito::Fumaca => {
                for k in 0..(if efeitos_leves() { 6 } else { 12 }) {
                    let a = k as f32 * 0.52;
                    self.particula(Particula { vida: nv::FUMACA_S, idade: 0.0, pos: p3(de) + vec3(a.cos() * 1.5, 0.2, a.sin() * 1.5), vel: vec3(a.cos() * 0.8, 0.25, a.sin() * 0.8), tam: 1.0, cresce: 0.35, cor: [0.78, 0.78, 0.8, 0.55], peso: -0.05 });
                }
            }
        }
        true
    }

    /// The hits of a snapshot, at sea: my hull hit flashes, and a sea mob's
    /// bite throws splinters off it.
    pub fn acertos(&mut self, world: &World, lista: &[shared::protocol::Acerto]) {
        let Some(eu) = world.self_id else {
            return;
        };
        for a in lista.iter().filter(|a| a.alvo == eu && a.dano > 0) {
            self.clarao_ate = get_time() + 0.25;
            let mordida = world.ents.get(&a.atacante).is_some_and(|e| e.meta.tag == EntityTag::Enemy);
            if let (true, Some(p)) = (mordida, world.ents.get(&eu).map(|e| e.render_pos)) {
                let de = vec2(a.de[0] as f32, a.de[1] as f32) / 127.0;
                let onde = p + de * 1.2;
                self.lascas(vec3(onde.x, y_agua() + 0.6, onde.y));
                self.respingo(onde);
                crate::sons::tocar(crate::sons::Som::CanhaoImpacto);
            }
        }
    }

    /// Particles and balls move; a ball that lands splashes or bursts.
    pub fn atualiza(&mut self, world: &World) {
        let dt = get_frame_time().min(0.1);
        let agora = get_time();
        for p in self.parts.iter_mut().filter(|p| p.vida > 0.0) {
            p.idade += dt;
            if p.idade >= p.vida {
                p.vida = 0.0;
                continue;
            }
            p.vel.y -= p.peso * dt;
            p.pos += p.vel * dt;
            p.tam = (p.tam + p.cresce * dt).max(0.02);
        }
        let ouvinte = world.self_pos();
        for k in 0..MAX_BALAS {
            let b = self.balas[k];
            if !b.ativa || ((agora - b.inicio) as f32) < b.voo {
                continue;
            }
            self.balas[k].ativa = false;
            // HIT OR SPLASH, the server's own rule (`navio::acertou`) against
            // what is drawn there now.
            let acertou = world.ents.iter().any(|(id, e)| {
                *id != b.dono
                    && e.state.hp > 0
                    && e.morte.is_none()
                    && matches!(e.meta.tag, EntityTag::Enemy | EntityTag::Player)
                    && nv::acertou(g(b.para), g(e.render_pos), if e.meta.tag == EntityTag::Player { nv::RAIO_DO_CASCO } else { 0.6 })
            });
            if acertou {
                self.lascas(vec3(b.para.x, y_agua() + 0.6, b.para.y));
                crate::sons::mundo(crate::sons::Som::CanhaoImpacto, b.para, ouvinte, b.meu);
            } else {
                self.respingo(b.para);
                crate::sons::mundo(crate::sons::Som::CanhaoAgua, b.para, ouvinte, b.meu);
            }
        }
    }

    // ─────────────────────────── the 3D pass ───────────────────────────

    /// The water decals (target ring, cannon arcs, aim line, bite
    /// telegraphs, fins) and the particles and balls, in two draw calls.
    pub fn desenha_3d(&mut self, world: &World, cam: &Camera3D, alvo: Option<EntityId>) {
        let agora = get_time();
        let t = agora as f32;
        self.decais.vertices.clear();
        self.decais.indices.clear();
        self.quads.vertices.clear();
        self.quads.indices.clear();
        let eu = world.self_id.and_then(|i| world.ents.get(&i));
        let alvo_e = alvo.and_then(|i| world.ents.get(&i)).filter(|e| e.state.hp > 0 && e.morte.is_none());
        // ── the sea mobs: fins, humps, the bite telegraph ──
        for (id, e) in &world.ents {
            if e.meta.tag != EntityTag::Enemy || e.morte.is_some() || e.state.hp == 0 {
                continue;
            }
            if let Some(eu) = eu {
                if eu.render_pos.distance(e.render_pos) > 70.0 {
                    continue;
                }
            }
            let elite = e_elite(e);
            let k = if elite { 1.7 } else { 1.0 };
            let frente = vec2(e.yaw.sin(), e.yaw.cos());
            let lado = vec2(-frente.y, frente.x);
            let p = e.render_pos;
            let y = y_agua();
            let fase = t * 2.4 + (id.0 % 17) as f32;
            match e.meta.kind {
                ENGUIA => {
                    // Three humps breaking the surface, rolling along the body.
                    for s in 0..3 {
                        let c = p - frente * (s as f32 * 0.9 * k) + lado * (fase + s as f32 * 1.4).sin() * 0.35 * k;
                        let alto = (0.32 + 0.12 * (fase + s as f32).sin()) * k;
                        let a = vec3(c.x - frente.x * 0.45 * k, y, c.y - frente.y * 0.45 * k);
                        let b = vec3(c.x + frente.x * 0.45 * k, y, c.y + frente.y * 0.45 * k);
                        tri(&mut self.decais, a, vec3(c.x, y + alto, c.y), b, [0.86, 0.55, 0.16, 1.0]);
                        tri(&mut self.decais, b, vec3(c.x, y + alto, c.y), a, [0.30, 0.20, 0.10, 1.0]);
                    }
                }
                _ => {
                    // THE FIN: a dark blade cutting the water, a little wake.
                    let base = p + frente * 0.25 * k;
                    let a = vec3(base.x + frente.x * 0.5 * k, y, base.y + frente.y * 0.5 * k);
                    let b = vec3(base.x - frente.x * 0.55 * k, y, base.y - frente.y * 0.55 * k);
                    let topo = vec3(base.x - frente.x * 0.35 * k, y + (0.95 + 0.06 * fase.sin()) * k, base.y - frente.y * 0.35 * k);
                    let cor = if elite { [0.42, 0.12, 0.12, 1.0] } else { [0.2, 0.24, 0.28, 1.0] };
                    tri(&mut self.decais, a, topo, b, cor);
                    tri(&mut self.decais, b, topo, a, [cor[0] * 0.7, cor[1] * 0.7, cor[2] * 0.7, 1.0]);
                    let rabo = p - frente * 1.6 * k;
                    faixa(&mut self.decais, p + lado * 0.25 * k, rabo + lado * 0.7 * k, 0.16, [0.95, 0.98, 1.0, 0.45]);
                    faixa(&mut self.decais, p - lado * 0.25 * k, rabo - lado * 0.7 * k, 0.16, [0.95, 0.98, 1.0, 0.45]);
                }
            }
            if elite {
                anel(&mut self.decais, p, 1.8 * k, 0.12, [0.9, 0.25, 0.2, 0.35], 24);
            }
            // THE BITE WINDING UP: a red ring that closes on the mob.
            if let Some((quem, dt, impacto)) = e.ataque_mob {
                if dt < impacto && impacto > 0.0 {
                    let f = (dt / impacto).clamp(0.0, 1.0);
                    let em_mim = quem.is_some() && quem == world.self_id;
                    let r = nv::RAIO_DA_MORDIDA * k;
                    let alfa = if em_mim { 0.85 } else { 0.5 };
                    anel(&mut self.decais, p, r, 0.2, [1.0, 0.18, 0.12, alfa], 32);
                    setor(&mut self.decais, p, 0.0, r * f, 0.0, std::f32::consts::TAU, [1.0, 0.12, 0.08, 0.35 * alfa + 0.2 * f], 20);
                    // A wedge from the mob towards its prey: who it bites.
                    if let Some(presa) = quem.and_then(|q| world.ents.get(&q)) {
                        let d = (presa.render_pos - p).normalize_or_zero();
                        faixa(&mut self.decais, p + d * r, p + d * (r + 1.2 + 1.5 * f), 0.35, [1.0, 0.2, 0.1, alfa]);
                    }
                }
            }
        }
        // ── the target, the Broadside's arcs and the Bow Cannon's line ──
        if let Some(a) = alvo_e {
            let r = if a.meta.tag == EntityTag::Player { 3.4 } else if e_elite(a) { 3.2 } else { 2.2 };
            let pulsa = 0.75 + 0.25 * (t * 4.0).sin();
            let hostil = a.meta.tag == EntityTag::Enemy || a.meta.pk.hostil;
            let cor = if hostil { [1.0, 0.42, 0.25, 0.9 * pulsa] } else { [0.95, 0.8, 0.45, 0.9 * pulsa] };
            anel(&mut self.decais, a.render_pos, r, 0.22, cor, 40);
            // Corner ticks: the "locked" read.
            for q in 0..4 {
                let ang = q as f32 * std::f32::consts::FRAC_PI_2 + t * 0.8;
                let d = vec2(ang.cos(), ang.sin());
                faixa(&mut self.decais, a.render_pos + d * (r + 0.25), a.render_pos + d * (r + 0.95), 0.2, cor);
            }
            if let Some(eu) = eu {
                let d = eu.render_pos.distance(a.render_pos);
                let bordada = &nv::HABILIDADES[0];
                let proa = &nv::HABILIDADES[1];
                // The Bow Cannon: a line to the target, the range ring.
                let no_alcance = d <= proa.alcance;
                let cor_linha = if no_alcance { [1.0, 0.62, 0.22, 0.55] } else { [0.6, 0.6, 0.62, 0.35] };
                faixa(&mut self.decais, eu.render_pos, a.render_pos, 0.12, cor_linha);
                anel(&mut self.decais, eu.render_pos, proa.alcance, 0.1, [1.0, 0.62, 0.22, 0.16], 64);
                // The Broadside's two cones, while the target is near.
                if d <= bordada.alcance + 8.0 {
                    let rumo = eu.state.vel_f32();
                    let parado = rumo.length_squared() < 0.25;
                    let costado = nv::no_costado(g(eu.render_pos), if parado { ::glam::Vec2::ZERO } else { rumo }, g(a.render_pos));
                    let acerta = d <= bordada.alcance && costado;
                    let verde = [0.35, 0.95, 0.45, 0.22];
                    let cinza = [0.75, 0.75, 0.78, 0.1];
                    if parado {
                        // Standing still it fires all round: the reach as a
                        // ring with a whisper of fill (a full disc buried the
                        // fight under it).
                        let (borda, miolo) = if acerta {
                            ([0.35, 0.95, 0.45, 0.55], [0.35, 0.95, 0.45, 0.07])
                        } else {
                            ([0.75, 0.75, 0.78, 0.35], [0.75, 0.75, 0.78, 0.03])
                        };
                        anel(&mut self.decais, eu.render_pos, bordada.alcance, 0.14, borda, 48);
                        setor(&mut self.decais, eu.render_pos, 0.0, bordada.alcance, 0.0, std::f32::consts::TAU, miolo, 24);
                    } else {
                        let ang = rumo.y.atan2(rumo.x);
                        let meia = nv::BORDADA_COS_MAX.acos();
                        let lado_alvo = (a.render_pos - eu.render_pos).perp_dot(vec2(rumo.x, rumo.y)) < 0.0;
                        for (s, este) in [(1.0f32, lado_alvo), (-1.0, !lado_alvo)] {
                            let centro = ang + s * std::f32::consts::FRAC_PI_2;
                            let cor = if acerta && este { verde } else { cinza };
                            setor(&mut self.decais, eu.render_pos, 2.0, bordada.alcance, centro - (std::f32::consts::FRAC_PI_2 - meia), centro + (std::f32::consts::FRAC_PI_2 - meia), cor, 12);
                        }
                    }
                }
            }
        }
        // ── my hull was hit: a red flash on the water around it ──
        if let Some(eu) = eu {
            if agora < self.clarao_ate {
                let f = ((self.clarao_ate - agora) / 0.25) as f32;
                setor(&mut self.decais, eu.render_pos, 0.0, 3.2, 0.0, std::f32::consts::TAU, [1.0, 0.15, 0.1, 0.4 * f], 16);
            }
        }
        // ── particles and balls: camera-facing quads ──
        let frente = (cam.target - cam.position).normalize_or_zero();
        let dir = frente.cross(Vec3::Y).normalize_or_zero();
        let cima = dir.cross(frente).normalize_or_zero();
        for p in self.parts.iter().filter(|p| p.vida > 0.0) {
            let f = 1.0 - p.idade / p.vida;
            let cor = [p.cor[0], p.cor[1], p.cor[2], p.cor[3] * f];
            disco(&mut self.quads, p.pos, p.tam, dir, cima, cor);
        }
        for b in self.balas.iter().filter(|b| b.ativa) {
            let f = (((agora - b.inicio) as f32) / b.voo.max(0.01)).clamp(0.0, 1.0);
            let fim = vec3(b.para.x, y_agua() + 0.3, b.para.y);
            let arco = (f * std::f32::consts::PI).sin() * (1.0 + b.voo * 2.2);
            let q = b.de.lerp(fim, f) + vec3(0.0, arco, 0.0);
            disco(&mut self.quads, q, 0.2, dir, cima, [0.08, 0.07, 0.07, 1.0]);
            // A short smoke trail behind it.
            let atras = b.de.lerp(fim, (f - 0.06).max(0.0)) + vec3(0.0, ((f - 0.06).max(0.0) * std::f32::consts::PI).sin() * (1.0 + b.voo * 2.2), 0.0);
            disco(&mut self.quads, atras, 0.16, dir, cima, [0.85, 0.85, 0.85, 0.35]);
        }
        if !self.decais.vertices.is_empty() {
            draw_mesh(&self.decais);
        }
        if !self.quads.vertices.is_empty() {
            draw_mesh(&self.quads);
        }
    }

    // ─────────────────────────── the 2D overlay ───────────────────────────

    /// Over the world: the target's life bar, the elites' plates and the
    /// nearest dock (on screen, or an arrow at the edge).
    pub fn desenha_2d(&mut self, world: &World, cam: &Camera3D, alvo: Option<EntityId>) {
        let eu = world.self_id.and_then(|i| world.ents.get(&i)).map(|e| e.render_pos);
        // Elite plates (only the near ones).
        for e in world.ents.values() {
            if e.meta.tag != EntityTag::Enemy || e.state.hp == 0 || e.morte.is_some() || !e_elite(e) {
                continue;
            }
            if eu.is_some_and(|p| p.distance(e.render_pos) > 45.0) {
                continue;
            }
            let Some(s) = world_to_screen(cam, vec3(e.render_pos.x, y_agua() + 3.2, e.render_pos.y)) else {
                continue;
            };
            let w = 74.0;
            let r = Rect::new(s.x - w * 0.5, s.y - 30.0, w, 18.0);
            estilo::ret_arredondado(r, 4.0, Color::new(0.45, 0.06, 0.05, 0.9));
            estilo::borda_arredondada(r, 4.0, 1.0, Color::new(1.0, 0.55, 0.35, 0.9));
            estilo::texto_centro_forte(s.x, r.y + 13.5, "ELITE", 12, Color::new(1.0, 0.86, 0.62, 1.0));
        }
        // The target's bar.
        if let Some(a) = alvo.and_then(|i| world.ents.get(&i)).filter(|e| e.state.hp > 0 && e.morte.is_none()) {
            if let Some(s) = world_to_screen(cam, vec3(a.render_pos.x, y_agua() + 2.6, a.render_pos.y)) {
                let max = a.meta.hp_max.max(1);
                let f = (a.state.hp as f32 / max as f32).clamp(0.0, 1.0);
                let (w, h) = (120.0, 10.0);
                let r = Rect::new(s.x - w * 0.5, s.y, w, h);
                draw_rectangle(r.x - 1.0, r.y - 1.0, r.w + 2.0, r.h + 2.0, Color::new(0.0, 0.0, 0.0, 0.75));
                let cor = if a.meta.tag == EntityTag::Player { Color::new(0.82, 0.62, 0.3, 1.0) } else { Color::new(0.9, 0.24, 0.2, 1.0) };
                draw_rectangle(r.x, r.y, r.w * f, r.h, cor);
                let d = eu.map_or(0.0, |p| p.distance(a.render_pos));
                let txt = self.linha_alvo.com(((a.state.hp as u64) << 20) | (d.round() as u64 & 0xFFFFF), || format!("{} · {:.0} m", a.state.hp, d));
                estilo::texto_centro_forte(s.x, r.y + h + 13.0, txt, 12, estilo::TEXTO);
            }
        }
        // The docks' names, distances and off-screen chevrons are the sea
        // scene's (`mar_cena::rotulos`); the panel names the nearest.
    }

    /// THE SHIP PANEL, top centre, at sea: the hull bar with its numbers,
    /// then speed · zone (safe / open / LAWLESS in red) · treasure, and a
    /// pulsing "low hull" warning. Returns its rectangle (the "Sail to"
    /// button hangs under it).
    pub fn painel_do_navio(&mut self, z: &crate::hud_layout::Zonas, eq: &shared::Equipment, eu: Option<Vec2>, velocidade: f32, marcado: bool) -> Option<Rect> {
        let (casco, max, avariado) = crate::mar_ui::casco_de(eq)?;
        let s = z.s;
        let w = 280.0 * s;
        let x = crate::render3d::tela().0 * 0.5 - w * 0.5;
        let y = z.alvo.y + z.alvo.h + 8.0 * s;
        let painel = Rect::new(x, y, w, 68.0 * s);
        estilo::ret_arredondado(painel, 6.0, Color::new(0.04, 0.07, 0.1, 0.82));
        estilo::borda_arredondada(painel, 6.0, 1.0, Color::new(0.9, 0.78, 0.5, 0.6));
        let frac = if max == 0 { 0.0 } else { casco as f32 / max as f32 };
        let barra = Rect::new(x + 8.0 * s, y + 6.0 * s, w - 16.0 * s, 16.0 * s);
        draw_rectangle(barra.x, barra.y, barra.w, barra.h, Color::new(0.0, 0.0, 0.0, 0.6));
        let baixo = frac <= nv::CRITICO;
        let pisca = 0.7 + 0.3 * (get_time() as f32 * 6.0).sin();
        let cor = if baixo { Color::new(0.92, 0.25, 0.2, pisca) } else { Color::new(0.78, 0.56, 0.28, 1.0) };
        draw_rectangle(barra.x, barra.y, barra.w * frac, barra.h, cor);
        let t = self.linha_casco.com(((casco as u64) << 16) | max as u64, || format!("Hull {casco} / {max}"));
        estilo::texto_centro_forte(barra.center().x, barra.y + barra.h - 3.0 * s, t, 13, estilo::TEXTO);
        // Speed · zone · treasure.
        let q = eu.map(g);
        let zona = match q {
            Some(p) if shared::mar::no_corredor(p) => 0u8,
            Some(p) if shared::mar::sem_lei(p) => 2,
            _ => 1,
        };
        let tesouro = crate::mar_ui::tesouro_no_conves(eq);
        let (zona_txt, zona_cor) = match zona {
            0 => ("Safe harbour", estilo::VERDE),
            2 => ("Lawless waters", estilo::VERMELHO),
            _ => ("Open waters", estilo::SUAVE),
        };
        let vel = velocidade.round() as u64;
        let info = self.linha_info.com((vel << 8) | (tesouro.is_some() as u64) << 4 | (avariado as u64), || format!("{vel} m/s"));
        let ly = y + 40.0 * s;
        estilo::texto(x + 10.0 * s, ly, info, 13, estilo::TEXTO);
        estilo::texto_forte(x + 10.0 * s + estilo::medir(info, 13) + 14.0, ly, zona_txt, 13, zona_cor);
        if tesouro.is_some() {
            estilo::texto_forte(x + w - 10.0 * s - estilo::medir_forte("Treasure", 13), ly, "Treasure", 13, estilo::OURO);
        }
        // The nearest dock: name and distance (its chevron is the scene's).
        if let Some((ilha, nome, _, d)) = eu.and_then(crate::mar_rota::porto_mais_perto) {
            let txt = self.linha_cais.com(((ilha as u64) << 32) | (d / 5.0).round() as u64, || format!("Nearest dock: {} · {:.0} m", nome, (d / 5.0).round() * 5.0));
            ancora_icone(vec2(x + 16.0 * s, y + 57.0 * s), 6.0 * s, estilo::OURO);
            estilo::texto(x + 28.0 * s, y + 61.0 * s, txt, 13, estilo::OURO);
        }
        // Why you are a target, or the low-hull warning, under the panel.
        let sem_lei = zona == 2;
        let aviso = if baixo { Some("Low hull — head to a dock!") } else { crate::mar_ui::linha_de_perigo(tesouro.is_some(), marcado, sem_lei) };
        if let Some(texto) = aviso {
            let ty = painel.y + painel.h + 16.0 * s;
            estilo::texto_centro_forte(x + w * 0.5 + 1.0, ty + 1.0, texto, 14, Color::new(0.0, 0.0, 0.0, 0.85));
            estilo::texto_centro_forte(x + w * 0.5, ty, texto, 14, Color::new(1.0, 0.32, 0.28, pisca));
        }
        Some(Rect::new(painel.x, painel.y, painel.w, painel.h + if aviso.is_some() { 22.0 * s } else { 0.0 }))
    }
}

/// A little anchor in vector strokes (no icon art for it).
pub fn ancora_icone(c: Vec2, r: f32, cor: Color) {
    let esp = (r * 0.18).max(1.5);
    draw_circle_lines(c.x, c.y - r * 0.75, r * 0.22, esp, cor);
    draw_line(c.x, c.y - r * 0.55, c.x, c.y + r * 0.8, esp, cor);
    draw_line(c.x - r * 0.45, c.y - r * 0.25, c.x + r * 0.45, c.y - r * 0.25, esp, cor);
    let n = 8;
    for k in 0..n {
        let a0 = std::f32::consts::PI * (0.15 + 0.7 * k as f32 / n as f32);
        let a1 = std::f32::consts::PI * (0.15 + 0.7 * (k + 1) as f32 / n as f32);
        draw_line(c.x + a0.cos() * r * 0.75, c.y + a0.sin() * r * 0.75 - r * 0.05, c.x + a1.cos() * r * 0.75, c.y + a1.sin() * r * 0.75 - r * 0.05, esp, cor);
    }
}

// ─────────────────────────── preview ───────────────────────────

/// One body of a preview scene: (id, tag, kind, position, name).
#[cfg(debug_assertions)]
type CorpoDaPrevia = (u32, EntityTag, u16, Vec2, Option<&'static str>);

/// A world for a preview scene: me (id 1, a Sloop) and the others.
#[cfg(debug_assertions)]
fn mundo_da_previa(eu: Vec2, yaw: f32, vel: Vec2, outros: &[CorpoDaPrevia]) -> World {
    let mut w = World::default();
    let mut metas = Vec::new();
    let mut estados = Vec::new();
    let todos = std::iter::once((1u32, EntityTag::Player, 0u16, eu, None)).chain(outros.iter().copied());
    for (id, tag, kind, p, nome) in todos {
        metas.push(shared::EntityMeta {
            skins: 0,
            cla: String::new(),
            pk: Default::default(),
            auras: 0,
            id: EntityId(id),
            tag,
            name: nome.map(String::from),
            hp_max: 100,
            faction: None,
            kind,
            nivel: 12,
            desafio: None,
            aparencia: 0,
            poder: 0,
            renascimentos: 0,
        });
        let v = if id == 1 { g(vel) } else { ::glam::Vec2::ZERO };
        estados.push(shared::EntityState::quantize(EntityId(id), g(p), v, if id == 2 { 62 } else { 100 }, if id == 1 { shared::ent_flags::SELF } else { 0 }));
    }
    w.apply(metas, estados, &[]);
    for (id, e) in w.ents.iter_mut() {
        e.yaw = if id.0 == 1 { yaw } else { yaw + 2.2 + id.0 as f32 };
    }
    w
}

/// PREVIEW of the sea fight (`MMO_PREVIA_MAR=1`, after the scene's own):
/// a Broadside in flight, a Bow Cannon shot, a shark winding up its bite
/// with an elite beside it, and the auto path's route on the water with
/// the ship panel and the "Sail to" ports — to `MMO_PREVIA_SAIDA`
/// (`mar-combate-*.png`).
#[cfg(debug_assertions)]
pub async fn previa() {
    use shared::navio::Visual;
    let saida = std::env::var("MMO_PREVIA_SAIDA").unwrap_or_else(|_| "/tmp/tempest-mar".into());
    std::fs::create_dir_all(&saida).unwrap();
    crate::mar_ui::define_no_mar(true);
    let (w, h) = (1280u32, 800u32);
    let tela = vec2(w as f32, h as f32);
    let rt = render_target_ex(w, h, RenderTargetParams { depth: true, sample_count: 1 });
    crate::render3d::define_alvo(Some(rt.clone()));
    let solido = crate::render3d::material_solido();
    let mut terreno = crate::terreno::Terreno::novo(&shared::mar::DEF);
    let ilhas = shared::mar::ilhas();
    let bosque = &ilhas[0];
    let mq = |v: ::glam::Vec2| vec2(v.x, v.y);
    let (fb, eb, lb) = (mq(bosque.fundeadouro()), mq(bosque.eixo()), mq(bosque.lado()));
    let aberto = fb + eb * 90.0;
    let yaw_de = |d: Vec2| d.x.atan2(d.y);
    let rumo = lb;
    let yaw = yaw_de(rumo);
    let lado = vec2(rumo.y, -rumo.x);
    let so = std::env::var("MMO_PREVIA_SO").ok();
    // (name, my position, my velocity, the others, target, shots, seconds after the shots)
    type Tiro = (u32, Vec2, Vec2, u8);
    let cenas: Vec<(&str, Vec2, Vec2, Vec<CorpoDaPrevia>, Option<u32>, Vec<Tiro>, f64)> = vec![
        (
            "bordada",
            aberto,
            rumo * 6.0,
            vec![(2, EntityTag::Enemy, TUBARAO, aberto + lado * 8.0, None), (3, EntityTag::Enemy, ENGUIA, aberto - lado * 9.0 + rumo * 2.0, None)],
            Some(2),
            vec![
                (nv::id_de_skill(0), aberto, aberto + lado * 8.0, 0),
                (nv::id_de_skill(0), aberto, aberto - lado * 9.0 + rumo * 2.0, 1),
            ],
            0.28,
        ),
        (
            "proa",
            aberto,
            Vec2::ZERO,
            vec![(2, EntityTag::Enemy, TUBARAO, aberto + rumo * 17.0 + lado * 3.0, None)],
            Some(2),
            vec![(nv::id_de_skill(1), aberto, aberto + rumo * 17.0 + lado * 3.0, 0)],
            0.35,
        ),
        (
            "tubarao",
            aberto,
            Vec2::ZERO,
            vec![
                (2, EntityTag::Enemy, TUBARAO, aberto + lado * 2.4 + rumo * 1.0, None),
                (3, EntityTag::Enemy, TUBARAO, aberto - lado * 7.0 + rumo * 4.0, Some("[ELITE] Dread Reef Shark")),
                (4, EntityTag::Enemy, ENGUIA, aberto + rumo * 9.0 - lado * 2.0, None),
            ],
            Some(2),
            vec![],
            0.0,
        ),
        ("rota", fb + eb * 30.0, eb * 10.0, vec![], None, vec![], 0.0),
    ];
    let gf = mq(ilhas[1].fundeadouro());
    for (nome, eu, vel, outros, alvo, tiros, depois) in cenas {
        if so.as_deref().is_some_and(|s| !nome.contains(s) && !"combate".contains(s)) {
            continue;
        }
        let yaw_eu = if vel.length_squared() > 0.01 { yaw_de(vel) } else { yaw };
        let mut world = mundo_da_previa(eu, yaw_eu, vel, &outros);
        if nome == "tubarao" {
            if let Some(e) = world.ents.get_mut(&EntityId(2)) {
                e.ataque_mob = Some((Some(EntityId(1)), nv::MORDIDA_PREPARO_S * 0.55, nv::MORDIDA_PREPARO_S));
                e.yaw = yaw_de(eu - e.render_pos);
            }
        }
        let mut vfx = MarCombate::default();
        let mut rastro = crate::rastro::Rastro::default();
        let mut rota = crate::mar_rota::RotaDoMar::default();
        if nome == "rota" {
            let meio = eu + eb * 120.0 + lb * 60.0;
            rastro.define(vec![eu + eb * 40.0 + lb * 10.0, meio, gf + mq(ilhas[1].eixo()) * 30.0, gf], gf);
            rota.para_ilha(ilhas[1].ilha as u8, get_time());
            rota.portos_abertos = true;
        }
        for _ in 0..80 {
            terreno.atualiza(eu, crate::config_graficos::RAIO_PADRAO + 1, 400);
        }
        // From astern for the bow shot (the target ahead in view).
        let cam_yaw = yaw_eu
            + match nome {
                "rota" => 0.3,
                "proa" => std::f32::consts::PI + 0.35,
                _ => 0.5,
            };
        let z = crate::render3d::ZOOM_MAX;
        let mut cam = crate::render3d::camera(eu, 0.0, cam_yaw, z, crate::render3d::pitch_padrao());
        cam.render_target = Some(rt.clone());
        cam.aspect = Some(w as f32 / h as f32);
        let disparo = get_time();
        for (id, de, mira, tier) in &tiros {
            vfx.tiro(&world, *id, EntityId(1), *de, *mira, *tier);
        }
        let mut eq = shared::Equipment::default();
        let navio = shared::item_id::NAVIO_CHALUPA;
        let mut inst = nv::instancia_nova(navio);
        inst.navio.as_mut().unwrap().casco = if nome == "tubarao" { 620 } else { 1_840 };
        eq.set(shared::EquipSlot::Navio, Some(navio), Some(inst));
        let mut quadros = 0;
        while quadros < 4 || get_time() - disparo < depois {
            quadros += 1;
            crate::render3d::camera_padrao();
            clear_background(crate::mar_cena::cor_de_fundo());
            let navios = [(eu, yaw_eu, Visual::default(), 1u64)];
            crate::mar_cena::desenha_mundo(&terreno, &solido, &cam, &navios, Some(eu));
            set_camera(&cam);
            macroquad::material::gl_use_material(&solido);
            for (id, tag, kind, p, _) in &outros {
                if *tag == EntityTag::Enemy {
                    let e = &world.ents[&EntityId(*id)];
                    let escala = if e_elite(e) { 1.6 } else { 1.0 };
                    crate::marinhos::desenha(*kind, vec3(p.x, 0.0, p.y), e.yaw, *id as u64, escala);
                }
            }
            macroquad::material::gl_use_default_material();
            let y = altura_do_tracejado();
            crate::rastro::desenha(&rastro, eu, rota.destino(), &|_, _| y, get_time() as f32);
            vfx.atualiza(&world);
            vfx.desenha_3d(&world, &cam, alvo.map(EntityId));
            crate::render3d::camera_padrao();
            crate::mar_cena::rotulos(&cam, Some(eu), tela);
            vfx.desenha_2d(&world, &cam, alvo.map(EntityId));
            // The HUD's sea half: the ship panel, the ports, the bar.
            let z = crate::hud_layout::atual();
            if let Some(r) = vfx.painel_do_navio(&z, &eq, Some(eu), vel.length(), false) {
                rota.desenha_portos(r, z.s, Some(eu), nv::CASCOS[0].velocidade);
            }
            if let Some(t) = rota.faixa(Some(eu), Some(rastro.caminho(eu).windows(2).map(|p| p[0].distance(p[1])).sum::<f32>() + 30.0), nv::CASCOS[0].velocidade) {
                crate::hud_layout::desenha_faixa(&z, &t, estilo::AUTO);
            }
            for (i, hab) in nv::HABILIDADES.iter().enumerate() {
                let r = crate::mar_ui::retangulo(i);
                let c = r.center();
                estilo::botao_redondo(c, r.w * 0.5, estilo::OURO, estilo::estado(false, false, false, false), false);
                crate::mar_ui::icone_da_habilidade(hab.efeito, c, r.w * 0.5 * 0.55, 1.0);
                if i == 0 && !tiros.is_empty() {
                    estilo::setor(c, r.w * 0.5 - 3.0, 0.8, Color::new(0.0, 0.0, 0.0, 0.62));
                }
            }
            unsafe {
                get_internal_gl().flush();
            }
            next_frame().await;
        }
        rt.texture.get_texture_data().export_png(&format!("{saida}/mar-combate-{nome}.png"));
        println!("mar-combate-{nome}.png");
    }
    println!("mar combate: {MAX_BALAS} balas, {MAX_PARTICULAS} particulas (piscinas fixas)");
}

#[cfg(test)]
mod testes {
    use super::*;

    /// THE POOLS ARE FIXED: a thousand effects never grow them, the oldest
    /// is overwritten.
    #[test]
    fn os_efeitos_vivem_em_piscinas_fixas() {
        let mut m = MarCombate::default();
        for k in 0..1000 {
            m.particula(Particula { vida: 1.0, tam: k as f32, ..Default::default() });
            m.bala(Bala { ativa: true, voo: 1.0, ..Default::default() });
        }
        assert_eq!(m.parts.len(), MAX_PARTICULAS);
        assert_eq!(m.balas.len(), MAX_BALAS);
        assert!(m.parts.iter().all(|p| p.vida > 0.0));
        // The newest is in, the very first was overwritten.
        assert!(m.parts.iter().any(|p| p.tam == 999.0));
        assert!(!m.parts.iter().any(|p| p.tam == 0.0));
    }

    /// The cached HUD line is rebuilt only when its key changes.
    #[test]
    fn a_linha_do_hud_so_refaz_quando_muda() {
        let mut l = Linha::default();
        let mut feitas = 0;
        for _ in 0..10 {
            l.com(7, || {
                feitas += 1;
                "x".into()
            });
        }
        assert_eq!(feitas, 1);
        l.com(8, || "y".into());
        assert_eq!(l.texto, "y");
    }
}
