//! COMBAT AT SEA YOU CAN READ (docs/NAVEGACAO.md, "gameplay remake",
//! 16/11/2026). The owner: "o combate tá péssimo".
//!
//! Everything the sea fight shows, beside the hull itself (that is
//! `mar_ui`):
//!
//! - **the target**: the land's thin gold ring, on the water (its life is
//!   the top-centre target frame's, like on land);
//! - **the guns bearing** (sea HUD clean-up, 16/11/2026: "o modo de debug
//!   do tiro tá muito ruim"): no cones, no aim line. With the Broadside
//!   ready and the target in reach off a side, a soft glow runs along THAT
//!   gunwale and its ports glint; with the Long Gun ready and the target in
//!   its reach off a side, a small reticle sits on the target. Nothing
//!   otherwise;
//! - **the shot**: muzzle flashes along the side, smoke, the cannonball on
//!   its arc for its REAL flight time (`navio::tempo_de_voo`, the same the
//!   server lands it with), a splash on a miss and a burst of splinters on
//!   a hit, and a shake for the shooter;
//! - **the sea mobs**: fins and eel humps breaking the surface (the bodies
//!   swim under the waves), the RED RING of a bite winding up
//!   (`navio::MORDIDA_PREPARO_S`), the bite's splinters on your hull;
//!   elites bigger and with a name plate;
//! - **the sea HUD** (`hud_do_mar`): no floating box over the ship any more.
//!   The hull is the character frame's bar (`hud::draw_ficha`); the zone is
//!   a brief toast when it changes plus a red skull badge by the minimap
//!   while in lawless waters; the treasure and the mark are badges there
//!   only while they hold. "Sail to…" lives on the minimap
//!   (`mar_rota::desenha_portos`).
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
use crate::relogio::{get_frame_time, get_time};

use crate::hud_estilo as estilo;
#[allow(unused_imports)]
use crate::ui::Toque as _;
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

/// What the guns can do this frame (cooldowns, the hull's size): the
/// bearing hints only show for a gun that is READY.
#[derive(Debug, Clone, Copy, Default)]
pub struct Prontos {
    /// The Broadside is off cooldown.
    pub bordada: bool,
    /// The Long Gun is off cooldown.
    pub canhonaco: bool,
    /// The hull class's drawn scale (`navio::CASCOS[..].escala`).
    pub escala: f32,
}

/// The waters a hull is in: 0 = the safe corridor, 1 = open, 2 = lawless.
pub fn zona_do_mar(p: Vec2) -> u8 {
    if shared::mar::no_corredor(g(p)) {
        0
    } else if shared::mar::sem_lei(g(p)) {
        2
    } else {
        1
    }
}

/// THE ZONE TOAST: what to say when the waters change (`None` = nothing,
/// e.g. the very first frame in open water).
pub fn aviso_de_zona(antes: Option<u8>, agora: u8) -> Option<(&'static str, Color)> {
    if antes == Some(agora) {
        return None;
    }
    match (antes, agora) {
        (_, 2) => Some(("Entering lawless waters", estilo::VERMELHO)),
        (Some(_), 0) | (None, 0) => Some(("Safe harbour", estilo::VERDE)),
        (Some(2), 1) => Some(("Leaving lawless waters", estilo::TEXTO)),
        _ => None,
    }
}

/// How long a sea toast stays (it fades in and out inside this).
const AVISO_S: f64 = 2.6;

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
    /// The waters last frame (`zona_do_mar`), for the toast.
    zona: Option<u8>,
    /// The hull was low last frame (the warning toasts once per dip).
    baixo: bool,
    /// The toast on screen: text, colour, since.
    aviso: Option<(&'static str, Color, f64)>,
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
            zona: None,
            baixo: false,
            aviso: None,
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
                // The long gun fires from the SIDE facing the target (no bow
                // gun since 16/11/2026).
                let lado = Vec2::new(-frente.y, frente.x);
                let sinal = if lado.dot(mira - de) >= 0.0 { 1.0 } else { -1.0 };
                let proa = de + lado * sinal * 1.3;
                self.clarao(p3(proa));
                self.fumaca(p3(proa), vec3(lado.x * sinal, 0.0, lado.y * sinal), 4);
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

    /// The water decals (target ring, the bearing gunwale's glow, bite
    /// telegraphs, fins) and the particles, balls and port glints, in two
    /// draw calls.
    pub fn desenha_3d(&mut self, world: &World, cam: &Camera3D, alvo: Option<EntityId>, prontos: Prontos) {
        let agora = get_time();
        let t = agora as f32;
        // The Broadside's ports glinting, when it bears (drawn as quads).
        let mut portas: [Option<(Vec3, f32)>; 3] = [None; 3];
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
        // ── the target: the land's ring, on the water ──
        if let Some(a) = alvo_e {
            let r = if a.meta.tag == EntityTag::Player { 3.0 } else if e_elite(a) { 2.6 } else { 1.8 };
            anel(&mut self.decais, a.render_pos, r, 0.14, [0.945, 0.784, 0.44, 0.92], 40);
            // THE BROADSIDE BEARS: a soft glow along the gunwale facing the
            // target and a glint at its three ports — only with the gun
            // ready and the target in its reach off that side.
            if let Some(eu) = eu.filter(|_| prontos.bordada) {
                let bordada = &nv::HABILIDADES[0];
                let rumo = crate::mar_ui::rumo_do_casco(eu.state.vel_f32(), eu.yaw);
                let rumo = vec2(rumo.x, rumo.y);
                let d = eu.render_pos.distance(a.render_pos);
                if d <= bordada.alcance && nv::no_costado(g(eu.render_pos), g(rumo), g(a.render_pos)) {
                    let frente = rumo.normalize_or_zero();
                    let lado = vec2(frente.y, -frente.x);
                    let s = if (a.render_pos - eu.render_pos).dot(lado) >= 0.0 { 1.0 } else { -1.0 };
                    let k = prontos.escala.max(0.6);
                    let pulso = 0.75 + 0.25 * (t * 3.2).sin();
                    let borda = eu.render_pos + lado * s * 1.2 * k;
                    let (popa, proa) = (borda - frente * 1.5 * k, borda + frente * 1.5 * k);
                    faixa(&mut self.decais, popa, proa, 0.5 * k, [1.0, 0.72, 0.32, 0.14 * pulso]);
                    faixa(&mut self.decais, popa, proa, 0.12 * k, [1.0, 0.82, 0.45, 0.6 * pulso]);
                    for (i, q) in [-0.9f32, 0.0, 0.9].into_iter().enumerate() {
                        let porta = eu.render_pos + frente * q * k + lado * s * 1.2 * k;
                        portas[i] = Some((vec3(porta.x, y_agua() + 0.8, porta.y), pulso));
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
        for (p, pulso) in portas.into_iter().flatten() {
            disco(&mut self.quads, p, 0.32 * pulso, dir, cima, [1.0, 0.7, 0.28, 0.32]);
            disco(&mut self.quads, p, 0.13, dir, cima, [1.0, 0.93, 0.72, 0.95 * pulso]);
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

    /// Over the world: only THE LONG GUN'S RETICLE — a small ring of four
    /// arcs on the target, while the gun is ready and the target is in its
    /// reach off a side. The target's life is the top-centre frame's
    /// (`hud::draw_alvo`), like on land; no plates, no bars over the water.
    pub fn desenha_2d(&mut self, world: &World, cam: &Camera3D, alvo: Option<EntityId>, prontos: Prontos) {
        if !prontos.canhonaco {
            return;
        }
        let Some(eu) = world.self_id.and_then(|i| world.ents.get(&i)) else {
            return;
        };
        let Some(a) = alvo.and_then(|i| world.ents.get(&i)).filter(|e| e.state.hp > 0 && e.morte.is_none()) else {
            return;
        };
        let proa = &nv::HABILIDADES[1];
        let rumo = crate::mar_ui::rumo_do_casco(eu.state.vel_f32(), eu.yaw);
        if eu.render_pos.distance(a.render_pos) > proa.alcance || !nv::no_costado(g(eu.render_pos), rumo, g(a.render_pos)) {
            return;
        }
        let Some(c) = world_to_screen(cam, vec3(a.render_pos.x, y_agua() + 0.5, a.render_pos.y)) else {
            return;
        };
        let s = crate::hud_layout::atual().s;
        let t = get_time() as f32;
        let r = (17.0 + 1.5 * (t * 3.0).sin()) * s.max(0.7);
        let cor = Color::new(1.0, 0.66, 0.3, 0.95);
        let sombra = Color::new(0.0, 0.0, 0.0, 0.45);
        for q in 0..4 {
            let a0 = q as f32 * std::f32::consts::FRAC_PI_2 + 0.35;
            estilo::arco(c + vec2(1.0, 1.0), r, a0, 0.155, 3.0, sombra);
            estilo::arco(c, r, a0, 0.155, 2.0, cor);
        }
        draw_circle(c.x, c.y, 2.2 * s.max(0.7), cor);
    }

    /// THE SEA HUD, over the land's (sea HUD clean-up, 16/11/2026): the zone
    /// toast when the waters change, the low-hull toast once per dip, and
    /// the badges by the minimap — a red skull in lawless waters, the chest
    /// while carrying treasure, the black flag while marked. Nothing else:
    /// the hull is the character frame's (`hud::draw_ficha`), and "Sail to…"
    /// is the minimap's (`mar_rota::desenha_portos`).
    pub fn hud_do_mar(&mut self, z: &crate::hud_layout::Zonas, eq: &shared::Equipment, eu: Option<Vec2>, marcado: bool, mini: Rect) {
        let agora = get_time();
        let s = z.s;
        let Some(eu) = eu else {
            return;
        };
        let zona = zona_do_mar(eu);
        if let Some((texto, cor)) = aviso_de_zona(self.zona, zona) {
            self.aviso = Some((texto, cor, agora));
        }
        self.zona = Some(zona);
        let baixo = crate::mar_ui::casco_de(eq).is_some_and(|(c, m, _)| m > 0 && (c as f32 / m as f32) <= nv::CRITICO);
        if baixo && !self.baixo {
            self.aviso = Some(("Low hull — head to a dock!", estilo::VERMELHO, agora));
        }
        self.baixo = baixo;
        // The toast: under the target frame, fading in and out.
        if let Some((texto, cor, desde)) = self.aviso {
            let idade = agora - desde;
            if idade > AVISO_S {
                self.aviso = None;
            } else {
                let f = ((idade / 0.25).min(1.0) * ((AVISO_S - idade) / 0.5).min(1.0)) as f32;
                let tam = 19;
                let cx = z.alvo.center().x;
                let w = estilo::medir_forte(texto, tam);
                // Under the target frame; lower still if it would cross the
                // character frame (a phone's narrow top).
                let mut y = z.alvo.y + z.alvo.h + 34.0 * s;
                let caixa = Rect::new(cx - w * 0.5 - 50.0 * s, y - tam as f32 * 1.6, w + 100.0 * s, tam as f32 * 2.0);
                if caixa.overlaps(&z.ficha) {
                    y = z.ficha.y + z.ficha.h + 34.0 * s;
                }
                let fio = Color::new(cor.r, cor.g, cor.b, 0.55 * f);
                draw_line(cx - w * 0.5 - 46.0 * s, y - tam as f32 * 0.32, cx - w * 0.5 - 12.0 * s, y - tam as f32 * 0.32, 1.5, fio);
                draw_line(cx + w * 0.5 + 12.0 * s, y - tam as f32 * 0.32, cx + w * 0.5 + 46.0 * s, y - tam as f32 * 0.32, 1.5, fio);
                estilo::texto_centro_forte(cx + 1.5, y + 1.5, texto, tam, Color::new(0.0, 0.0, 0.0, 0.75 * f));
                estilo::texto_centro_forte(cx, y, texto, tam, Color::new(cor.r, cor.g, cor.b, f));
            }
        }
        // The badges: a column down the minimap disc's left, outside it.
        let tesouro = crate::mar_ui::tesouro_no_conves(eq).is_some();
        let lado = (estilo::medir_dim("M", 13).height * 2.6).max(22.0);
        let (c, raio) = (mini.center(), mini.w * 0.5 - 20.0);
        let _ = raio;
        let mut y = c.y - lado - 3.0 * s;
        let x = mini.x - lado - 4.0 * s;
        let m = Vec2::from(mouse_position());
        let mut dica = None;
        for (ativo, qual) in [(zona == 2, 0u8), (tesouro, 1), (marcado, 2)] {
            if !ativo {
                continue;
            }
            let r = Rect::new(x, y, lado, lado);
            let c = r.center();
            let (cor, texto) = match qual {
                // Why you are a target: the same line the land's warning used.
                0 => (estilo::VERMELHO, crate::mar_ui::linha_de_perigo(false, false, true).unwrap_or_default()),
                1 => (estilo::OURO, crate::mar_ui::linha_de_perigo(true, false, false).unwrap_or_default()),
                _ => (Color::new(0.9, 0.9, 0.92, 1.0), crate::mar_ui::linha_de_perigo(false, true, false).unwrap_or_default()),
            };
            draw_circle(c.x, c.y, lado * 0.5, Color::new(0.04, 0.05, 0.08, 0.82));
            draw_circle_lines(c.x, c.y, lado * 0.5, 1.5, estilo::alfa(cor, 0.85));
            match qual {
                0 => caveira(c, lado * 0.3, cor),
                1 => bau_icone(c, lado * 0.3, cor),
                _ => bandeira_icone(c, lado * 0.3, cor),
            }
            if r.toca(m) {
                dica = Some((r, texto));
            }
            y += lado + 6.0 * s;
        }
        if let Some((r, t)) = dica {
            estilo::tooltip(r, t, false);
        }
    }
}

/// A small skull: the lawless waters' badge (vector, no atlas art).
pub fn caveira(c: Vec2, r: f32, cor: Color) {
    let osso = cor;
    let fundo = Color::new(0.04, 0.05, 0.08, 1.0);
    draw_circle(c.x, c.y - r * 0.15, r * 0.82, osso);
    draw_rectangle(c.x - r * 0.48, c.y + r * 0.3, r * 0.96, r * 0.5, osso);
    draw_circle(c.x - r * 0.32, c.y - r * 0.12, r * 0.24, fundo);
    draw_circle(c.x + r * 0.32, c.y - r * 0.12, r * 0.24, fundo);
    draw_triangle(vec2(c.x, c.y + r * 0.12), vec2(c.x - r * 0.1, c.y + r * 0.3), vec2(c.x + r * 0.1, c.y + r * 0.3), fundo);
    for k in [-0.22f32, 0.0, 0.22] {
        draw_line(c.x + k * r, c.y + r * 0.5, c.x + k * r, c.y + r * 0.8, (r * 0.09).max(1.0), fundo);
    }
}

/// A small treasure chest (vector).
pub fn bau_icone(c: Vec2, r: f32, cor: Color) {
    let fundo = Color::new(0.04, 0.05, 0.08, 1.0);
    draw_rectangle(c.x - r * 0.9, c.y - r * 0.15, r * 1.8, r * 0.9, cor);
    draw_triangle(vec2(c.x - r * 0.9, c.y - r * 0.2), vec2(c.x + r * 0.9, c.y - r * 0.2), vec2(c.x, c.y - r * 0.8), cor);
    draw_rectangle(c.x - r * 0.9, c.y - r * 0.2, r * 1.8, r * 0.12, fundo);
    draw_rectangle(c.x - r * 0.14, c.y - r * 0.05, r * 0.28, r * 0.36, fundo);
}

/// A small black flag on a pole, the mark (vector).
pub fn bandeira_icone(c: Vec2, r: f32, cor: Color) {
    draw_line(c.x - r * 0.6, c.y - r * 0.9, c.x - r * 0.6, c.y + r * 0.9, (r * 0.14).max(1.2), cor);
    draw_triangle(vec2(c.x - r * 0.55, c.y - r * 0.85), vec2(c.x + r * 0.85, c.y - r * 0.45), vec2(c.x - r * 0.55, c.y - r * 0.05), cor);
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

/// PREVIEW of the sea with THE REAL HUD (`MMO_PREVIA_MAR=1`, after the
/// scene's own; `MMO_PREVIA_SO=combate` renders only these): idle sailing,
/// a fight with both guns bearing, the Broadside just fired, a crossing to
/// an island with "Sail to…" open, and lawless waters with treasure on
/// deck — each in landscape, a phone in landscape and a phone in portrait,
/// through the sailing camera (`mar_camera`). To `MMO_PREVIA_SAIDA`
/// (`mar-hud-<scene>-<screen>.png`).
#[cfg(debug_assertions)]
pub async fn previa() {
    use shared::navio::Visual;
    let saida = std::env::var("MMO_PREVIA_SAIDA").unwrap_or_else(|_| "/tmp/tempest-mar".into());
    std::fs::create_dir_all(&saida).unwrap();
    crate::mar_ui::define_no_mar(true);
    crate::hud_layout::define_escala_ui(crate::hud_layout::escala_ui_padrao());
    let solido = crate::render3d::material_solido();
    let mut terreno = crate::terreno::Terreno::novo(&shared::mar::DEF);
    let ilhas = shared::mar::ilhas();
    let bosque = &ilhas[0];
    let mq = |v: ::glam::Vec2| vec2(v.x, v.y);
    let (fb, eb, lb) = (mq(bosque.fundeadouro()), mq(bosque.eixo()), mq(bosque.lado()));
    let aberto = fb + eb * 90.0;
    let yaw_de = |d: Vec2| d.x.atan2(d.y);
    let rumo = lb;
    let lado = vec2(rumo.y, -rumo.x);
    let so = std::env::var("MMO_PREVIA_SO").ok();
    // The sea's minimap, as the game builds it.
    let mut mapa = crate::mapa::Mapa::para(Some(&shared::mar::DEF));
    for _ in 0..900 {
        mapa.acompanhar();
        if mapa.tem_textura() {
            break;
        }
        next_frame().await;
    }
    let sem_lei = mq(shared::mar::zonas_sem_lei()[0]);
    let gf = mq(ilhas[1].fundeadouro());
    // (name, me, my velocity, the others, target, shots, seconds after)
    type Tiro = (u32, Vec2, Vec2, u8);
    let alvo_perto = aberto + lado * 8.0 + rumo * 1.0;
    let cenas: Vec<(&str, Vec2, Vec2, Vec<CorpoDaPrevia>, Option<u32>, Vec<Tiro>, f64)> = vec![
        ("navegando", aberto, rumo * 6.0, vec![], None, vec![], 0.0),
        (
            "combate",
            aberto,
            rumo * 4.0,
            vec![(2, EntityTag::Enemy, TUBARAO, alvo_perto, Some("Reef Shark")), (3, EntityTag::Enemy, ENGUIA, aberto - lado * 9.0 + rumo * 4.0, None)],
            Some(2),
            vec![],
            0.0,
        ),
        (
            "combate-tiro",
            aberto,
            rumo * 4.0,
            vec![(2, EntityTag::Enemy, TUBARAO, alvo_perto, Some("Reef Shark")), (3, EntityTag::Enemy, TUBARAO, aberto - lado * 7.0 + rumo * 6.0, Some("[ELITE] Dread Reef Shark"))],
            Some(2),
            vec![(nv::id_de_skill(0), aberto, alvo_perto, 0)],
            0.3,
        ),
        ("rota", fb + eb * 30.0, eb * 10.0, vec![], None, vec![], 0.0),
        (
            "mapa",
            aberto,
            rumo * 6.0,
            vec![
                (2, EntityTag::Enemy, TUBARAO, aberto + lado * 14.0 + rumo * 8.0, None),
                (3, EntityTag::Enemy, TUBARAO, aberto - lado * 18.0 + rumo * 14.0, Some("[ELITE] Dread Reef Shark")),
                (4, EntityTag::Player, 0, aberto + rumo * 30.0 - lado * 6.0, Some("Corsair")),
                (5, EntityTag::Npc, shared::npc_kind(None, shared::dungeon::PAPEL_BAU), aberto - lado * 8.0 - rumo * 20.0, None),
            ],
            None,
            vec![],
            0.0,
        ),
        ("sem-lei", sem_lei, rumo * 6.0, vec![(2, EntityTag::Player, 0, sem_lei + lado * 14.0 + rumo * 10.0, Some("Corsair"))], None, vec![], 0.6),
    ];
    // (name, width, height, safe-area margins: top, left, bottom, right)
    let telas: [(&str, u32, u32, [f32; 4]); 3] = [
        ("paisagem", 1280, 800, [0.0; 4]),
        ("celular", 926, 428, [0.0, 47.0, 21.0, 47.0]),
        ("retrato", 428, 926, [47.0, 0.0, 34.0, 0.0]),
    ];
    for (tela_nome, w, h, margens) in telas {
        let rt = render_target_ex(w, h, RenderTargetParams { depth: true, sample_count: 1 });
        crate::render3d::define_alvo(Some(rt.clone()));
        crate::hud_layout::define_margens(margens);
        let tela = vec2(w as f32, h as f32);
        for (nome, eu, vel, outros, alvo, tiros, depois) in &cenas {
            let (nome, eu, vel, alvo, depois) = (*nome, *eu, *vel, *alvo, *depois);
            if so.as_deref().is_some_and(|s| !nome.contains(s) && !"combate".contains(s)) {
                continue;
            }
            let yaw_eu = if vel.length_squared() > 0.01 { yaw_de(vel) } else { yaw_de(rumo) };
            let mut world = mundo_da_previa(eu, yaw_eu, vel, outros);
            for (id, e) in world.ents.iter_mut() {
                if e.meta.tag == EntityTag::Player && id.0 != 1 {
                    e.yaw = yaw_eu + 0.6;
                }
            }
            let mut vfx = MarCombate::default();
            let mut rastro = crate::rastro::Rastro::default();
            let mut rota = crate::mar_rota::RotaDoMar::default();
            if nome == "rota" || nome == "mapa" {
                let eu = if nome == "mapa" { aberto } else { eu };
                let meio = eu + eb * 120.0 + lb * 60.0;
                rastro.define(vec![eu + eb * 40.0 + lb * 10.0, meio, gf + mq(ilhas[1].eixo()) * 30.0, gf], gf);
                rota.para_ilha(ilhas[1].ilha as u8, get_time());
                rota.portos_abertos = nome == "rota";
            }
            mapa.rota = rastro.caminho(eu);
            mapa.destino_mar = rota.destino();
            for _ in 0..80 {
                terreno.atualiza(eu, crate::config_graficos::RAIO_PADRAO + 1, 400);
            }
            // THE SAILING CAMERA, settled, a touch off the stern.
            let alvo_p = alvo.and_then(|a| world.ents.get(&EntityId(a))).map(|e| e.render_pos);
            let q = crate::mar_camera::enquadramento(eu, vec2(yaw_eu.sin(), yaw_eu.cos()), alvo_p, 0, h > w, 1.0, 0.0);
            let cam_yaw = crate::mar_camera::yaw_atras(vec2(yaw_eu.sin(), yaw_eu.cos())) + 0.35;
            let mut cam = crate::render3d::camera_no_mar(q.centro, 0.0, cam_yaw, q.fator, q.pitch);
            cam.render_target = Some(rt.clone());
            cam.aspect = Some(w as f32 / h as f32);
            let mut hab = crate::habilidades::Habilidades::default();
            // The Smoke on cooldown (the sweep); the Broadside too once fired.
            let mut recargas = vec![(nv::id_de_skill(3), 9.0)];
            if !tiros.is_empty() {
                recargas.push((nv::id_de_skill(0), 6.5));
            }
            hab.estado(recargas, 0.0);
            let disparo = get_time();
            for (id, de, mira, tier) in tiros {
                vfx.tiro(&world, *id, EntityId(1), *de, *mira, *tier);
            }
            let mut eq = shared::Equipment::default();
            let navio = shared::item_id::NAVIO_CHALUPA;
            let mut inst = nv::instancia_nova(navio);
            inst.navio.as_mut().unwrap().casco = 1_840;
            if nome == "sem-lei" {
                inst.navio.as_mut().unwrap().conves = Some(nv::tesouro_novo(10, 12, Some(0)));
            }
            eq.set(shared::EquipSlot::Navio, Some(navio), Some(inst));
            let prontos = |hab: &crate::habilidades::Habilidades| {
                let agora = get_time();
                Prontos {
                    bordada: hab.recarga_ate(nv::id_de_skill(0)) <= agora,
                    canhonaco: hab.recarga_ate(nv::id_de_skill(1)) <= agora,
                    escala: nv::CASCOS[0].escala,
                }
            };
            let barra = crate::mar_ui::BarraDoNavio::default();
            let mut ficha = crate::hud::Ficha::default();
            let mut quadros = 0;
            while quadros < 4 || get_time() - disparo < depois {
                quadros += 1;
                crate::render3d::camera_padrao();
                clear_background(crate::mar_cena::cor_de_fundo());
                let mut navios = vec![(eu, yaw_eu, Visual::default(), 1u64)];
                for (id, tag, _, p, _) in outros.iter() {
                    if *tag == EntityTag::Player {
                        navios.push((*p, yaw_eu + 0.6, Visual::default(), *id as u64));
                    }
                }
                crate::mar_cena::desenha_mundo(&terreno, &solido, &cam, &navios, Some(eu));
                set_camera(&cam);
                macroquad::material::gl_use_material(&solido);
                for (id, tag, kind, p, _) in outros.iter() {
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
                vfx.desenha_3d(&world, &cam, alvo.map(EntityId), prontos(&hab));
                crate::render3d::camera_padrao();
                let foco = rota.ilha_em_foco(eu);
                crate::mar_cena::rotulos(&cam, Some(eu), tela, foco);
                vfx.desenha_2d(&world, &cam, alvo.map(EntityId), prontos(&hab));
                // ── THE REAL HUD, as `main` draws it at sea ──
                let z = crate::hud_layout::atual();
                crate::chat_ui::ChatUi::default().caixa(z.chat, "");
                crate::hud::draw_hud(
                    &z,
                    &crate::hud::Info { realm: "Tempest".into(), canal: "Canal 1".into(), zona: shared::mar::DEF.zona.into(), jogadores: 42, capacidade: 200 },
                    &crate::hud::Rede { ms: 32.0, ..Default::default() },
                    None,
                    0,
                    0,
                    0,
                    0,
                    Vec2::ZERO,
                    &[],
                );
                mapa.desenha_mini(&world);
                let casco = crate::mar_ui::casco_de(&eq).map(|(c, m, _)| (c, m));
                crate::hud::draw_ficha(&z, &mut ficha, 0.016, "Navegante", 70, 1840, 2400, 1000, 240, Some(24850), casco);
                crate::hud::draw_modo_pk(&z, Default::default());
                barra.desenha(&hab);
                crate::auto_combate::AutoCombate::default().desenha();
                crate::hud::draw_atacar(&z, alvo.is_some());
                crate::hud::draw_rapidos(&z, [0; 4], [0; 4], [false; 4], [None; 4], [false; 4], None, &|_| String::new());
                if let Some(a) = alvo.and_then(|a| world.ents.get(&EntityId(a))) {
                    crate::hud::draw_alvo(&z, a.meta.name.as_deref().unwrap_or("?"), 12, a.state.hp, a.meta.hp_max, false);
                }
                crate::hud::draw_topo(&z, false, false, false, false, false);
                crate::hud::draw_botao_economia(&z);
                // The order `main` draws them in: the strip, then the sea HUD.
                if let Some(t) = rota.faixa(Some(eu), Some(rastro.caminho(eu).windows(2).map(|p| p[0].distance(p[1])).sum::<f32>()), nv::CASCOS[0].velocidade) {
                    crate::hud_layout::desenha_faixa(&z, &t, estilo::AUTO);
                }
                vfx.hud_do_mar(&z, &eq, Some(eu), false, crate::mapa::Mapa::mini_rect());
                rota.desenha_portos(crate::mapa::Mapa::disco_do_minimapa(), z.s, Some(eu), nv::CASCOS[0].velocidade);
                crate::hud::draw_exp(&z, &ficha);
                // The world map at sea, open over it all.
                if nome == "mapa" {
                    mapa.aberto = true;
                    mapa.desenha_grande(&world, 70, &crate::mundo_ui::Mundo::default(), 0);
                    mapa.aberto = false;
                }
                unsafe {
                    get_internal_gl().flush();
                }
                next_frame().await;
            }
            rt.texture.get_texture_data().export_png(&format!("{saida}/mar-hud-{nome}-{tela_nome}.png"));
            println!("mar-hud-{nome}-{tela_nome}.png");
        }
    }
    crate::hud_layout::define_margens([0.0; 4]);
    crate::render3d::define_alvo(None);
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

    /// THE ZONE TOAST speaks only when the waters CHANGE: into lawless
    /// waters (red), into the safe corridor, out of lawless waters — never
    /// twice for the same water, never for plain open water on arrival.
    #[test]
    fn o_aviso_de_zona_so_fala_quando_muda() {
        assert_eq!(aviso_de_zona(Some(1), 1), None);
        assert_eq!(aviso_de_zona(None, 1), None, "toasted plain open water on arrival");
        assert_eq!(aviso_de_zona(Some(1), 2).map(|a| a.0), Some("Entering lawless waters"));
        assert_eq!(aviso_de_zona(None, 2).map(|a| a.0), Some("Entering lawless waters"));
        assert_eq!(aviso_de_zona(Some(1), 0).map(|a| a.0), Some("Safe harbour"));
        assert_eq!(aviso_de_zona(Some(2), 1).map(|a| a.0), Some("Leaving lawless waters"));
        assert_eq!(aviso_de_zona(Some(0), 1), None);
        // The real chart: a lawless centre is lawless, a dock is safe.
        let c = shared::mar::zonas_sem_lei()[0];
        assert_eq!(zona_do_mar(vec2(c.x, c.y)), 2);
        let f = shared::mar::ilhas()[0].fundeadouro();
        assert_eq!(zona_do_mar(vec2(f.x, f.y)), 0);
    }
}
