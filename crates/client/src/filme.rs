//! THE AD CAPTURE (`MMO_FILME`, debug desktop only, docs/FILME.md).
//!
//! The first-login benchmark scenes (`benchmark`), grown into short films
//! for ads and store screenshots: the same local scenes with no server, the
//! same game camera, light, fog, bodies, auras, skill effects and damage
//! numbers, across the zones the client can build alone. No HUD, no panel.
//!
//! Every frame is drawn into a multisampled target of the requested size
//! with the drawing clock pinned to a fixed 1/60 s step (`relogio`), read
//! back and piped as raw RGBA into ffmpeg, so the video is perfectly smooth
//! however slow the capture runs. Stills are saved from the same frames.
//!
//! Never reachable by players: the module only exists in debug desktop
//! builds, and `main` enters it only with `MMO_FILME` set (`pedido`).

#![cfg(all(debug_assertions, not(any(target_os = "ios", target_os = "android"))))]

use std::io::Write;

use macroquad::material::{gl_use_default_material, gl_use_material, Material};
use macroquad::prelude::*;

use crate::config_graficos as cg;
use shared::{EntityId, EntityState, EntityTag};

/// Frames per second of the film, and the fixed step.
pub const QPS: u32 = 60;
const DT: f32 = 1.0 / QPS as f32;

/// The scenes, in the order `todas` renders them.
pub const CENAS: &[&str] = &[
    "bosque", "geleira", "ermo", "planalto", "celeste", "kogen", "abissal", "magica", "mar", "auras", "chefe",
];

/// `MMO_FILME`: which scenes to film. `None` (unset, empty, "0" or no known
/// name) = the capture stays off and the game starts as always.
pub fn pedido(v: Option<&str>) -> Option<Vec<&'static str>> {
    let v = v?.trim();
    if v.is_empty() || v == "0" {
        return None;
    }
    if v == "todas" || v == "all" {
        return Some(CENAS.to_vec());
    }
    let lista: Vec<&'static str> =
        v.split(',').filter_map(|n| CENAS.iter().copied().find(|c| *c == n.trim())).collect();
    (!lista.is_empty()).then_some(lista)
}

/// Named sizes (`MMO_FILME_TAM`), portrait unless the name says otherwise.
pub const TAMANHOS: &[(&str, u32, u32)] = &[
    ("16x9", 1920, 1080),
    ("4k", 3840, 2160),
    ("9x16", 1080, 1920),
    ("iphone67", 1290, 2796),
    ("iphone65", 1242, 2688),
    ("ipad13", 2064, 2752),
    ("ipad129", 2048, 2732),
    ("android", 1080, 1920),
    ("android-alto", 1080, 2400),
];

/// `16x9`, `iphone67`, `2048x2732`… plus `-paisagem` (or `-landscape`) for
/// the same size lying down. (width, height, name for the files).
pub fn tamanho(s: &str) -> Option<(u32, u32, String)> {
    let s = s.trim().to_lowercase();
    let (base, deitar) = match s.strip_suffix("-paisagem").or_else(|| s.strip_suffix("-landscape")) {
        Some(b) => (b.to_string(), true),
        None => (s.clone(), false),
    };
    let (w, h, nome) = match TAMANHOS.iter().find(|t| t.0 == base) {
        Some(&(n, w, h)) => (w, h, n.to_string()),
        None => {
            let (a, b) = base.split_once('x')?;
            let (w, h): (u32, u32) = (a.trim().parse().ok()?, b.trim().parse().ok()?);
            (w, h, format!("{w}x{h}"))
        }
    };
    if !(64..=8192).contains(&w) || !(64..=8192).contains(&h) {
        return None;
    }
    if deitar {
        Some((w.max(h), w.min(h), format!("{nome}-paisagem")))
    } else {
        Some((w, h, nome))
    }
}

/// The frames to save as stills: `n` evenly spaced over `duracao` (in the
/// middle of each slice, never the first or last frame) plus the explicit
/// times `em`, in seconds, as frame numbers, sorted, no repeats.
pub fn quadros_dos_prints(n: usize, em: &[f32], duracao: f32) -> Vec<u32> {
    let total = (duracao * QPS as f32).round().max(1.0) as u32;
    let mut v: Vec<u32> = (0..n).map(|i| (((i as f32 + 0.5) / n as f32) * total as f32) as u32).collect();
    v.extend(em.iter().filter(|t| **t >= 0.0).map(|t| (t * QPS as f32).round() as u32));
    v.retain(|q| *q < total);
    v.sort_unstable();
    v.dedup();
    v
}

/// How much farther the camera stands for this aspect, so a portrait frame
/// still holds the action (the vertical field of view is fixed: a narrow
/// frame sees less across). 1 for 16:9 and wider.
pub fn recuo_do_aspecto(aspecto: f32) -> f32 {
    ((16.0 / 9.0) / aspecto.max(0.1)).sqrt().max(1.0)
}

// ── camera paths ─────────────────────────────────────────────────────────

/// One key of a camera path: the game camera's yaw, zoom (distance factor
/// over the game's zoom 1; may go past the game's own limits) and pitch,
/// and where it looks, as an offset from the scene's focus.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Chave {
    yaw: f32,
    zoom: f32,
    pitch: f32,
    dx: f32,
    dz: f32,
}

const fn ch(yaw: f32, zoom: f32, pitch: f32, dx: f32, dz: f32) -> Chave {
    Chave { yaw, zoom, pitch, dx, dz }
}

/// The path at `u` (0..1): eased in and out over the whole shot, through
/// every key on a Catmull-Rom curve (no stop at the keys in between).
fn caminho(chaves: &[Chave], u: f32) -> Chave {
    if chaves.len() == 1 {
        return chaves[0];
    }
    let u = 0.5 - 0.5 * (u.clamp(0.0, 1.0) * std::f32::consts::PI).cos();
    let n = chaves.len() - 1;
    let s = u * n as f32;
    let i = (s.floor() as usize).min(n - 1);
    let f = s - i as f32;
    let k = |j: isize| chaves[j.clamp(0, n as isize) as usize];
    let (p0, p1, p2, p3) = (k(i as isize - 1), k(i as isize), k(i as isize + 1), k(i as isize + 2));
    let cr = |a: f32, b: f32, c: f32, d: f32| {
        0.5 * (2.0 * b + (c - a) * f + (2.0 * a - 5.0 * b + 4.0 * c - d) * f * f + (3.0 * b - a - 3.0 * c + d) * f * f * f)
    };
    Chave {
        yaw: cr(p0.yaw, p1.yaw, p2.yaw, p3.yaw),
        zoom: cr(p0.zoom, p1.zoom, p2.zoom, p3.zoom),
        pitch: cr(p0.pitch, p1.pitch, p2.pitch, p3.pitch),
        dx: cr(p0.dx, p1.dx, p2.dx, p3.dx),
        dz: cr(p0.dz, p1.dz, p2.dz, p3.dz),
    }
}

/// The game camera (`render3d::camera`) at `c`, pulled back by `recuo`,
/// into the capture target.
///
/// No ground collision: the paths are scripted, and pushing the eye up to
/// the block under it (a step function, half a unit per block) made it jump
/// whenever a low shot crossed a block edge (the owner saw the camera
/// flicker, 17/11/2026). `camera_mede` checks every path stays clear.
fn camera_de(alvo: Vec2, chao_alvo: f32, c: &Chave, recuo: f32, ctx: &Contexto) -> Camera3D {
    let mut cam = crate::render3d::camera(alvo, chao_alvo, c.yaw, 1.0, c.pitch);
    cam.position = cam.target + (cam.position - cam.target) * c.zoom * recuo;
    cam.aspect = Some(ctx.aspecto);
    cam.render_target = Some(ctx.rt.clone());
    let (perto, longe) = planos_da_captura(cam.position.distance(cam.target));
    cam.z_near = perto;
    cam.z_far = longe;
    cam
}

/// Near and far planes for an eye `d` from what it looks at.
///
/// The game's camera keeps macroquad's 0.01 near plane; with the capture's
/// eye 10-120 u out and the haze at ~300 u that wasted the depth buffer on
/// the first centimetres: depth error grows as distance² / near, so at
/// 0.01 even 24 bits leave ~0.5 u of error 300 u out (roofs, trees and the
/// ground behind them took turns, and distant geometry cut through other
/// geometry). Nothing is ever nearer than a fifth of the way to the target,
/// so the near plane goes there (1-3 u: ~100-300x finer depth). The far
/// plane matters little once the near one is sane; it stays well past the
/// haze so the sky and the clouds are never cut.
fn planos_da_captura(d: f32) -> (f32, f32) {
    ((d * 0.15).clamp(0.5, 3.0), 4000.0)
}

/// A critically damped spring (the `SmoothDamp` of `render3d::altura_da_camera`)
/// with a slower time: the capture's eye height follows the ground gently.
fn mola(atual: f32, vel: f32, alvo: f32, dt: f32, tempo: f32) -> (f32, f32) {
    if atual == f32::MIN {
        return (alvo, 0.0);
    }
    let w = 2.0 / tempo;
    let x = w * dt;
    let decai = 1.0 / (1.0 + x + 0.48 * x * x + 0.235 * x * x * x);
    let erro = atual - alvo;
    let temp = (vel + w * erro) * dt;
    (alvo + (erro + temp) * decai, (vel - w * temp) * decai)
}

/// The ground under the camera's target, smooth: the terrain is a step
/// function (blocks), so its height at one point jumps half a unit at every
/// block edge, and the camera rode those jumps (target AND eye). Averaged
/// over a disk 6 u wide, each block edge moves it ~1/17 of a step.
fn chao_suave(alvo: Vec2, chao: &dyn Fn(f32, f32) -> f32) -> f32 {
    let mut soma = chao(alvo.x, alvo.y);
    let mut n = 1.0;
    for r in [3.0f32, 6.0] {
        for k in 0..8 {
            let a = k as f32 * std::f32::consts::FRAC_PI_4 + r * 0.1;
            soma += chao(alvo.x + a.cos() * r, alvo.y + a.sin() * r);
            n += 1.0;
        }
    }
    soma / n
}

/// One frame's camera, for `MMO_FILME_CAMERA` (the frame-to-frame check):
/// the eye and target as drawn, the eye the old code drew (raw block height
/// under the target, pushed out of the ground), and the eye's clearance over
/// the ground under it.
#[derive(Clone, Copy, Debug, Default)]
pub struct Medida {
    pub olho: Vec3,
    pub alvo: Vec3,
    pub olho_antigo: Vec3,
    pub folga: f32,
    pub yaw: f32,
}

// ── the cast ─────────────────────────────────────────────────────────────

/// Me (id 1), the other players (10..), the boss (99), the mobs (100..).
const EU: u32 = 1;
const CHEFE: u32 = 99;

#[derive(Clone)]
struct Elenco {
    /// My rebirths (15 = the black hole).
    renascimentos: u8,
    /// The other players' rebirths; their gear auras cycle 4 tiers.
    jogadores: Vec<u8>,
    /// Mob kinds, cycled over `inimigos`.
    kinds: Vec<u16>,
    inimigos: u32,
    chefe: Option<u16>,
    /// The aura lineup: everyone in a row, facing the camera, no mobs.
    fila: bool,
    /// Ring radius of the players (only the boss's reach since the fights).
    roda: f32,
    /// The fights: how many clearings, how far from the focus, how far
    /// apart, players and mobs per clearing (`filme_luta`).
    plano: Plano,
}

#[derive(Clone)]
struct Plano {
    clareiras: usize,
    perto: f32,
    longe: f32,
    entre: f32,
    jogadores: Vec<usize>,
    mobs: Vec<usize>,
}

impl Default for Plano {
    fn default() -> Self {
        Self { clareiras: 4, perto: 4.0, longe: 30.0, entre: 13.0, jogadores: vec![3, 3, 2, 3], mobs: vec![4, 5, 3, 4] }
    }
}

// ── the frame context ────────────────────────────────────────────────────

struct Contexto {
    rt: RenderTarget,
    aspecto: f32,
    recuo: f32,
    escolha: cg::Escolha,
    /// No damage numbers or health bars (store screenshots).
    limpo: bool,
}

// ── an island scene ──────────────────────────────────────────────────────

struct Tomada {
    segundos: f32,
    chaves: Vec<Chave>,
}

struct Ilha {
    def: &'static shared::terreno::DefIlha,
    terreno: crate::terreno::Terreno,
    casas: crate::construcoes::Construcoes,
    mundo: crate::world::World,
    foco: Vec2,
    elenco: Elenco,
    tomadas: Vec<Tomada>,
    noite: bool,
    abismo: bool,
    relogio: f32,
    /// The fights (none in the aura lineup).
    luta: Option<crate::filme_luta::Luta>,
    /// The clearings the fights happen in.
    clareiras: Vec<Vec2>,
    /// The camera's ground height and its speed (`mola`), reset per shot.
    altura_cam: (f32, f32),
}

impl Ilha {
    #[allow(clippy::too_many_arguments)]
    fn nova(def: &'static shared::terreno::DefIlha, foco: Vec2, elenco: Elenco, tomadas: Vec<Tomada>, noite: bool, abismo: bool) -> Self {
        let mut s = Self {
            def,
            terreno: crate::terreno::Terreno::novo(def),
            casas: crate::construcoes::Construcoes::para(Some(def)),
            mundo: crate::world::World::default(),
            foco,
            elenco,
            tomadas,
            noite,
            abismo,
            relogio: 0.0,
            luta: None,
            clareiras: Vec::new(),
            altura_cam: (f32::MIN, 0.0),
        };
        s.povoa();
        s
    }

    /// The camera paths, once the clearings are known.
    fn com_tomadas(mut self, f: impl Fn(Vec2, &[Vec2]) -> Vec<Tomada>) -> Self {
        let t = f(self.foco, &self.clareiras);
        if !t.is_empty() {
            self.tomadas = t;
        }
        self
    }

    fn povoa(&mut self) {
        let el = self.elenco.clone();
        if el.fila {
            // The showcase: cash outfits, weapon skins, gear and rebirth
            // auras, weapons in hand, in a row.
            let herois = &crate::filme_luta::HEROIS;
            let n = herois.len() as f32;
            let x0 = -(n - 1.0) * 0.5 * 2.7;
            let g = |p: Vec2| ::glam::Vec2::new(p.x, p.y);
            let mut metas = Vec::new();
            let mut estados = Vec::new();
            for (k, h) in herois.iter().enumerate() {
                let id = if k == 0 { EU } else { 10 + k as u32 - 1 };
                metas.push(h.meta(id, k as u32));
                let mut st = EntityState::quantize(EntityId(id), g(self.foco + vec2(x0 + k as f32 * 2.7, 0.0)), g(Vec2::ZERO), 1000, 0);
                st.acao = shared::components::acao::monta(h.conjunto(), true, 0, 0);
                estados.push(st);
            }
            self.mundo.apply(metas, estados, &[]);
            return;
        }
        // The fights: clearings spread round the focus (`filme_luta`).
        let ilha = std::rc::Rc::new(shared::terreno::Ilha::da_ilha(self.def));
        let ger = std::rc::Rc::new(shared::terreno::Gerador::da_ilha(self.def));
        let livre = {
            let (ilha, ger) = (ilha.clone(), ger.clone());
            move |p: Vec2| {
                let q = ::glam::Vec2::new(p.x, p.y);
                if ilha.ocupado(q, 0.38) || ilha.agua(p.x, p.y) {
                    return false;
                }
                // No cliffs: one block of step at most round the body.
                let h = ger.altura(p.x, p.y);
                [(0.5f32, 0.0f32), (-0.5, 0.0), (0.0, 0.5), (0.0, -0.5)]
                    .iter()
                    .all(|(dx, dz)| (ger.altura(p.x + dx, p.y + dz) - h).abs() <= 0.55)
            }
        };
        let p = &el.plano;
        let clareiras = if el.chefe.is_some() {
            vec![self.foco]
        } else {
            let altura = |x: f32, z: f32| ger.altura(x, z);
            crate::filme_luta::clareiras(self.foco, p.clareiras, p.perto, p.longe, p.entre, &livre, &altura)
        };
        println!(
            "[filme] {} clearings: {:?}",
            clareiras.len(),
            clareiras.iter().map(|c| ((c.x - self.foco.x) as i32, (c.y - self.foco.y) as i32)).collect::<Vec<_>>()
        );
        let (jogadores, mobs) =
            if el.chefe.is_some() { (vec![el.jogadores.len() + 1], vec![0]) } else { (p.jogadores.clone(), p.mobs.clone()) };
        let semente = self.def.semente as u64 ^ 0x51_7cc1;
        let mut luta = crate::filme_luta::Luta::nova(&clareiras, &jogadores, &mobs, &el.kinds, el.chefe, semente, Box::new(livre));
        luta.passo(&mut self.mundo, 0.0);
        self.luta = Some(luta);
        self.clareiras = clareiras;
    }

    fn duracoes(&self) -> Vec<f32> {
        self.tomadas.iter().map(|t| t.segundos).collect()
    }

    fn alvo(&self, i: usize, u: f32) -> Vec2 {
        let c = caminho(&self.tomadas[i].chaves, u);
        self.foco + vec2(c.dx, c.dz)
    }

    /// Bakes toward the shot's start; `true` once all of it is in.
    fn prepara(&mut self, i: usize, ctx: &Contexto) -> bool {
        // Round the shot's start only: the loaded square is far wider than
        // the frame (`RAIO_DA_CAPTURA`), and the rest of the path streams
        // in under the haze as the camera moves. Baking three centres made
        // each evict the others' far chunks.
        let raio = ctx.escolha.raio;
        self.casas.acompanhar();
        self.altura_cam = (f32::MIN, 0.0);
        let a = self.alvo(i, 0.0);
        self.terreno.atualiza(a, raio, 96);
        let (feitos, total) = self.terreno.prontos_em(a, raio);
        self.casas.prontas() && feitos >= total
    }

    fn quadro(&mut self, i: usize, t: f32, vox: &crate::vox::VoxCache, solido: &Material, ctx: &Contexto) -> Medida {
        self.relogio += DT;
        let u = t / self.tomadas[i].segundos;
        let c = caminho(&self.tomadas[i].chaves, u);
        let alvo = self.foco + vec2(c.dx, c.dz);
        let e = &ctx.escolha;
        self.casas.acompanhar();
        self.terreno.atualiza(alvo, e.raio, 32);
        if let Some(l) = &mut self.luta {
            l.passo(&mut self.mundo, DT);
        }
        let t_ = &self.terreno;
        self.mundo.tick(DT, &|x, z, _| t_.altura(x, z));
        let fila = self.elenco.fila;
        if fila {
            for ent in self.mundo.ents.values_mut() {
                // Weapons in hand, facing the camera.
                ent.sacada = 1.0;
                ent.yaw = c.yaw;
            }
        }
        let t = &self.terreno;
        let chao = |x: f32, z: f32| t.altura(x, z);
        // The ground the camera rides: smooth, on the fixed clock.
        let (h, v) = self.altura_cam;
        self.altura_cam = mola(h, v, chao_suave(alvo, &chao), DT, 0.5);
        let apoio = self.altura_cam.0;
        let mut vista = crate::render3d::Vista::nova(alvo, c.yaw, 1.0, c.pitch, apoio, &chao);
        // The lineup in portrait: the full pull-back (1.78x at 9:16) left six
        // small bodies in a band across the middle, and at the end of the
        // dolly put the eye inside the house behind the camera. Pulled back
        // only a little, two or three heroes fill the tall frame as the
        // dolly passes them (17/11/2026).
        let recuo = if fila { ctx.recuo.powf(0.5) } else { ctx.recuo };
        vista.cam = camera_de(alvo, apoio, &c, recuo, ctx);
        let medida = {
            let mut antiga = camera_de(alvo, t.altura(alvo.x, alvo.y), &c, recuo, ctx).position;
            antiga.y = antiga.y.max(chao(antiga.x, antiga.z) + 1.2);
            let o = vista.cam.position;
            Medida { olho: o, alvo: vista.cam.target, olho_antigo: antiga, folga: o.y - chao(o.x, o.z), yaw: c.yaw }
        };
        set_camera(&vista.cam);
        crate::render3d::clear();
        let luz = crate::render3d::luz_dia(if e.sombras == cg::Sombras::Bonitas { 1.0 } else { 0.0 });
        solido.set_uniform("LuzDia", luz);
        crate::gpu_estatica::define_luz_dia(luz);
        // The haze is solid a chunk INSIDE the loaded square, never past
        // it: pushed out with a pulled-back camera (as it was) it showed the
        // square's edge, trees cut along a line (the owner, 17/11/2026).
        // The square is wide (`RAIO_DA_CAPTURA`), so the haze starts far.
        let (inicio, fim) = neblina_da_captura();
        let inicio = if self.noite { -inicio.max(0.01) } else { inicio };
        crate::gpu_estatica::define_neblina(alvo, inicio, fim);
        solido.set_uniform("Neblina", vec4(alvo.x, alvo.y, inicio, fim));
        crate::luzes::preparar(self.noite, alvo, &chao);
        gl_use_material(solido);
        solido.set_uniform("Crop", Vec3::ZERO);
        solido.set_uniform("RecorteZ", 0.0f32);
        t.desenha(&vista.cam, Vec3::ZERO, 0.0);
        self.casas.desenha(&vista.cam, None, Vec3::ZERO, 0.0);
        if e.sombras == cg::Sombras::Bonitas {
            t.desenha_sombras(&vista.cam);
        }
        crate::agua::desenha(t, &vista.cam, crate::relogio::get_time() as f32);
        gl_use_material(solido);
        crate::render3d::draw_entities_com_sombras(&mut self.mundo, vox, None, &vista, e.sombras);
        if self.noite {
            crate::luzes::desenha_halos(&vista.cam, &chao);
            if self.abismo {
                crate::abismo::desenha(&vista.cam, &chao);
            }
            gl_use_material(solido);
        }
        crate::luzes::preparar(false, Vec2::ZERO, &|_, _| 0.0);
        apaga_luz_e_neblina(solido);
        gl_use_default_material();
        if let Some(l) = &mut self.luta {
            // The boss's slams on the ground, then the skills as the game
            // draws them (`habilidades`, on the fixed clock).
            // The slam's layers (background, fill, front, rim, arrows) are
            // coplanar quads of different sizes, drawn one over the other.
            // The old 16-bit depth rounded them all to the same value and
            // each new layer passed; with exact depth they fight and the
            // red area came out in stepped pieces (17/11/2026). Each vertex
            // a hair above the one drawn before it: a later layer always
            // wins, by ~0.002 u per layer.
            let lift = std::cell::Cell::new(0.0f32);
            let chao_em_camadas = |x: f32, z: f32| {
                lift.set(lift.get() + 2e-5);
                chao(x, z) + lift.get()
            };
            l.telegrafos.desenha(&chao_em_camadas, crate::relogio::get_time());
            l.habilidades.desenha_efeitos(&self.mundo, &vista);
            gl_use_default_material();
        }
        crate::render3d::camera_padrao();
        if !fila && !ctx.limpo {
            crate::efeitos::numeros(&self.mundo, &vista);
        }
        medida
    }
}

/// The capture's view distance, in chunks of 16 u (`MMO_FILME_RAIO`): far
/// past Ultra's 12, since the capture is not bound by a phone's memory. At
/// 20 the haze starts ~150 u out and is solid at ~300 u, so trees and
/// houses never end in a line inside the frame.
pub const RAIO_DA_CAPTURA: i32 = 20;
/// At sea the game's own draw band (`mar_cena::faixa_de_desenho`) follows
/// the radius: pushed out, its dither fade sits far past the frame too.
pub const RAIO_DA_CAPTURA_NO_MAR: i32 = 18;

/// The haze for the capture's loaded square (`neblina` of the game, which
/// ends where the square ends, minus a chunk of margin: the square is
/// centred on the target's chunk, not on the target).
fn neblina_da_captura() -> (f32, f32) {
    let fim = (cg::raio_terreno() as f32 - 1.0) * crate::terreno::CHUNK as f32 * shared::terreno::BLOCO;
    (fim * 0.5, fim)
}

fn apaga_luz_e_neblina(solido: &Material) {
    solido.set_uniform("LuzDia", 0.0f32);
    crate::gpu_estatica::define_luz_dia(0.0);
    solido.set_uniform("Neblina", Vec4::ZERO);
    crate::gpu_estatica::define_neblina(Vec2::ZERO, 0.0, 0.0);
}

// ── the open sea ─────────────────────────────────────────────────────────

const TUBARAO: u16 = 70;
const ENGUIA: u16 = 72;

struct Mar {
    terreno: crate::terreno::Terreno,
    /// Open water off the Grove's dock, the way out, and across it.
    aberto: Vec2,
    rumo: Vec2,
    lado: Vec2,
    cais: Vec2,
    eixo: Vec2,
    /// A themed dock for the last shot: Kōgen-tō's neon harbour (or the
    /// Grove's again when the sea has no such dock).
    cais2: Vec2,
    eixo2: Vec2,
    mundo: crate::world::World,
    vfx: crate::mar_combate_ui::MarCombate,
    proximo_tiro: f32,
    proximo_passo: f32,
    tiros: u32,
}

const MAR_TOMADAS: [f32; 4] = [9.0, 11.0, 7.0, 7.0];

impl Mar {
    fn novo() -> Self {
        let ilhas = shared::mar::ilhas();
        let b = &ilhas[0];
        let mq = |v: ::glam::Vec2| vec2(v.x, v.y);
        let (f, eixo, lado) = (mq(b.fundeadouro()), mq(b.eixo()), mq(b.lado()));
        let aberto = f + eixo * 90.0;
        let (cais2, eixo2) = ilhas
            .iter()
            .find(|i| i.zona == "ilha_kogen")
            .or_else(|| ilhas.iter().find(|i| i.zona == "ilha_celeste"))
            .map_or((f, eixo), |i| (mq(i.fundeadouro()), mq(i.eixo())));
        let mut s = Self {
            terreno: crate::terreno::Terreno::novo(&shared::mar::DEF),
            aberto,
            rumo: lado,
            lado: vec2(lado.y, -lado.x),
            cais: f,
            eixo,
            cais2,
            eixo2,
            mundo: crate::world::World::default(),
            vfx: Default::default(),
            proximo_tiro: 0.0,
            proximo_passo: 0.0,
            tiros: 0,
        };
        s.povoa(0.0);
        s
    }

    /// The fight: my ship sailing slowly, sharks and an eel round it, an
    /// elite among them.
    fn corpos(&self, t: f32) -> Vec<(u32, EntityTag, u16, Vec2, Vec2, Option<&'static str>)> {
        let eu = self.aberto + self.rumo * 3.0 * t;
        let vel = self.rumo * 3.0;
        let mut v = vec![(EU, EntityTag::Player, 0, eu, vel, None)];
        for (k, (kind, raio, vel_a, nome)) in
            [(TUBARAO, 9.0f32, 0.35f32, None), (TUBARAO, 12.0, -0.28, Some("[ELITE] Dread Reef Shark")), (ENGUIA, 10.5, 0.3, None), (TUBARAO, 14.0, 0.22, None)]
                .into_iter()
                .enumerate()
        {
            let a = k as f32 * 1.7 + t * vel_a;
            let p = eu + vec2(a.cos(), a.sin()) * raio;
            let d = vec2(-a.sin(), a.cos()) * raio * vel_a + vel;
            v.push((2 + k as u32, EntityTag::Enemy, kind, p, d, nome));
        }
        v
    }

    fn povoa(&mut self, t: f32) {
        let g = |p: Vec2| ::glam::Vec2::new(p.x, p.y);
        let corpos = self.corpos(t);
        let metas = corpos
            .iter()
            .map(|(id, tag, kind, _, _, nome)| {
                let mut m = crate::filme_luta::meta(*id, *tag, *kind, 100);
                m.name = nome.map(String::from);
                m.nivel = 12;
                m
            })
            .collect();
        let estados = corpos
            .iter()
            .map(|(id, _, _, p, d, _)| {
                EntityState::quantize(EntityId(*id), g(*p), g(*d), 100, if *id == EU { shared::ent_flags::SELF } else { 0 })
            })
            .collect();
        self.mundo.apply(metas, estados, &[]);
    }

    /// Where the terrain loads for shot `i` at `u`.
    fn alvo(&self, i: usize, u: f32) -> Vec2 {
        match i {
            0 => self.aberto + self.rumo * 7.0 * u * MAR_TOMADAS[0] - self.lado * 20.0,
            1 => self.aberto + self.rumo * 3.0 * u * MAR_TOMADAS[1],
            2 => self.cais + self.eixo * (30.0 - 36.0 * u),
            _ => self.cais2 + self.eixo2 * (30.0 - 36.0 * u),
        }
    }

    fn prepara(&mut self, i: usize, _ctx: &Contexto) -> bool {
        let raio = crate::mar_cena::raio_no_mar().max(cg::RAIO_PADRAO + 1);
        let mut pronto = true;
        for u in [0.0, 0.5, 1.0] {
            let a = self.alvo(i, u);
            self.terreno.atualiza(a, raio, 64);
            let (feitos, total) = self.terreno.prontos_em(a, raio);
            pronto &= feitos >= total;
        }
        pronto
    }

    fn quadro(&mut self, i: usize, t: f32, solido: &Material, ctx: &Contexto) -> Medida {
        use shared::navio::Visual;
        let u = t / MAR_TOMADAS[i];
        let yaw_de = |d: Vec2| d.x.atan2(d.y);
        let raio = crate::mar_cena::raio_no_mar().max(cg::RAIO_PADRAO + 1);
        let alvo = self.alvo(i, u);
        self.terreno.atualiza(alvo, raio, 24);
        crate::mar_ui::define_no_mar(true);
        let (cam, navios, combate) = match i {
            // Ships sailing in formation, wakes behind them.
            0 => {
                let lider = self.aberto + self.rumo * 7.0 * t;
                let frota: Vec<(Vec2, f32, Visual, u64)> = [(0.0f32, 0.0f32, 2u8), (-9.0, 7.0, 1), (-9.0, -7.0, 1), (-18.0, 13.0, 0), (-18.0, -13.0, 3)]
                    .into_iter()
                    .enumerate()
                    .map(|(k, (atras, lado, classe))| {
                        let v = Visual { classe, vela: classe + 1, ..Default::default() };
                        let bal = (crate::relogio::get_time() as f32 * 0.7 + k as f32).sin() * 0.04;
                        (lider + self.rumo * atras + self.lado * lado, yaw_de(self.rumo) + bal, v, 10 + k as u64)
                    })
                    .collect();
                let c = caminho(&[ch(1.9, 2.6, 0.62, 0.0, 0.0), ch(1.2, 2.1, 0.5, 0.0, 0.0), ch(0.6, 1.9, 0.42, 0.0, 0.0)], u);
                let foco = lider - self.rumo * 8.0;
                let c = Chave { yaw: yaw_de(self.rumo) + c.yaw, ..c };
                (camera_de(foco, 0.0, &c, ctx.recuo, ctx), frota, false)
            }
            // The broadside fight.
            1 => {
                let tt = self.proximo_passo;
                if t >= tt {
                    self.proximo_passo = t + 0.25;
                    self.povoa(t);
                }
                self.mundo.tick(DT, &|_, _, _| 0.0);
                // The bodies exactly on their paths: the snapshots above are
                // re-sent every 0.25 s, and the world's catch-up between them
                // made my ship (and the camera following it) jerk 43 times in
                // the shot (`scripts/filme-camera.py`, 17/11/2026).
                for (id, _, _, p, _, _) in self.corpos(t) {
                    if let Some(e) = self.mundo.ents.get_mut(&EntityId(id)) {
                        e.render_pos = p;
                    }
                }
                let eu =self.mundo.ents.get(&EntityId(EU)).map_or(self.aberto, |e| e.render_pos);
                if let Some(e) = self.mundo.ents.get_mut(&EntityId(EU)) {
                    e.yaw = yaw_de(self.rumo);
                }
                for (id, e) in self.mundo.ents.iter_mut() {
                    if id.0 != EU {
                        let d = e.render_pos - eu;
                        e.yaw = yaw_de(vec2(-d.y, d.x));
                    }
                }
                if t >= self.proximo_tiro {
                    let alvo = 2 + self.tiros % 4;
                    let mira = self.mundo.ents.get(&EntityId(alvo)).map_or(eu + self.lado * 9.0, |e| e.render_pos);
                    let skill = if self.tiros % 3 == 2 { 1 } else { 0 };
                    self.vfx.tiro(&self.mundo, shared::navio::id_de_skill(skill), EntityId(EU), eu, mira, (self.tiros % 3) as u8);
                    if skill == 0 {
                        // The broadside fires both sides.
                        let outro = 2 + (self.tiros + 2) % 4;
                        if let Some(m) = self.mundo.ents.get(&EntityId(outro)).map(|e| e.render_pos) {
                            self.vfx.tiro(&self.mundo, shared::navio::id_de_skill(0), EntityId(EU), eu, m, 1);
                        }
                    }
                    self.tiros += 1;
                    self.proximo_tiro = t + 1.35;
                }
                let c = caminho(&[ch(0.5, 1.6, 0.6, 0.0, 0.0), ch(1.4, 1.35, 0.5, 0.0, 0.0), ch(2.3, 1.5, 0.55, 0.0, 0.0)], u);
                let c = Chave { yaw: yaw_de(self.rumo) + c.yaw, ..c };
                let navios = vec![(eu, yaw_de(self.rumo), Visual { classe: 2, vela: 3, ..Default::default() }, 1u64)];
                (camera_de(eu, 0.0, &c, ctx.recuo, ctx), navios, true)
            }
            // Into a dock: the Grove's with its lighthouse, then the themed one.
            _ => {
                let (cais, eixo, longe) = if i == 2 { (self.cais, self.eixo, 70.0) } else { (self.cais2, self.eixo2, 75.0) };
                let _ = longe;
                let foco = cais + eixo * (30.0 - 36.0 * u);
                let barco = cais + eixo * (longe - 8.0 - 40.0 * u) + vec2(eixo.y, -eixo.x) * 6.0;
                let navios = vec![
                    (barco, yaw_de(-eixo), Visual { classe: 1, vela: 2, ..Default::default() }, 21u64),
                    (barco + eixo * 16.0 - vec2(eixo.y, -eixo.x) * 12.0, yaw_de(-eixo) + 0.2, Visual { classe: 0, vela: 1, ..Default::default() }, 22),
                ];
                let c = caminho(&[ch(0.7, 2.6, 0.42, 0.0, 0.0), ch(0.3, 2.1, 0.3, 0.0, 0.0)], u);
                // The eye out at sea, looking in at the dock.
                let c = Chave { yaw: (-eixo.x).atan2(eixo.y) + c.yaw, ..c };
                (camera_de(foco, 0.0, &c, ctx.recuo, ctx), navios, false)
            }
        };
        set_camera(&cam);
        clear_background(crate::mar_cena::cor_de_fundo());
        crate::mar_cena::desenha_mundo(&self.terreno, solido, &cam, &navios, Some(alvo));
        if combate {
            set_camera(&cam);
            gl_use_material(solido);
            for (id, e) in &self.mundo.ents {
                if e.meta.tag == EntityTag::Enemy {
                    let escala = if crate::mar_combate_ui::e_elite(e) { 1.6 } else { 1.0 };
                    crate::marinhos::desenha(e.meta.kind, vec3(e.render_pos.x, 0.0, e.render_pos.y), e.yaw, id.0 as u64, escala);
                }
            }
            gl_use_default_material();
            self.vfx.atualiza(&self.mundo);
            self.vfx.desenha_3d(&self.mundo, &cam, None, Default::default());
        }
        apaga_luz_e_neblina(solido);
        crate::render3d::camera_padrao();
        crate::mar_ui::define_no_mar(false);
        // The sea is flat: the eye over the water at y 0.
        Medida { olho: cam.position, alvo: cam.target, olho_antigo: cam.position, folga: cam.position.y, yaw: 0.0 }
    }
}

// ── the scenes ───────────────────────────────────────────────────────────

enum Cena {
    Ilha(Box<Ilha>),
    Mar(Box<Mar>),
}

impl Cena {
    fn duracoes(&self) -> Vec<f32> {
        match self {
            Cena::Ilha(i) => i.duracoes(),
            Cena::Mar(_) => MAR_TOMADAS.to_vec(),
        }
    }

    fn prepara(&mut self, i: usize, ctx: &Contexto) -> bool {
        match self {
            Cena::Ilha(s) => s.prepara(i, ctx),
            Cena::Mar(s) => s.prepara(i, ctx),
        }
    }

    fn quadro(&mut self, i: usize, t: f32, vox: &crate::vox::VoxCache, solido: &Material, ctx: &Contexto) -> Medida {
        match self {
            Cena::Ilha(s) => s.quadro(i, t, vox, solido, ctx),
            Cena::Mar(s) => s.quadro(i, t, solido, ctx),
        }
    }

    fn sai(&self) {
        crate::render3d::define_noite(false);
        crate::render3d::define_abismo(false);
        crate::mar_ui::define_no_mar(false);
    }
}

fn v2(p: ::glam::Vec2) -> Vec2 {
    vec2(p.x, p.y)
}

fn cidade(def: &'static shared::terreno::DefIlha) -> Vec2 {
    shared::terreno::Gerador::da_ilha(def).cidade().map_or(Vec2::ZERO, |c| v2(c.centro()))
}

fn elenco(renascimentos: u8, jogadores: &[u8], kinds: &[u16], inimigos: u32) -> Elenco {
    Elenco {
        renascimentos,
        jogadores: jogadores.to_vec(),
        kinds: kinds.to_vec(),
        inimigos,
        chefe: None,
        fila: false,
        roda: 2.6,
        plano: Plano::default(),
    }
}

fn tomada(segundos: f32, chaves: &[Chave]) -> Tomada {
    Tomada { segundos, chaves: chaves.to_vec() }
}

/// The Plateau's camera over its clearings: a low glide past the three
/// nearest in a sweep round the focus, then the pull-up reveal over the
/// middle of all of them.
fn tomadas_da_batalha(foco: Vec2, clareiras: &[Vec2]) -> Vec<Tomada> {
    if clareiras.len() < 3 {
        return Vec::new();
    }
    let mut c: Vec<Vec2> = clareiras.iter().map(|p| *p - foco).collect();
    c.sort_by(|a, b| a.y.atan2(a.x).total_cmp(&b.y.atan2(b.x)));
    // The three in a row round the focus that are nearest one another.
    let n = c.len();
    let k = (0..n)
        .min_by(|&i, &j| {
            let l = |i: usize| c[i].distance(c[(i + 1) % n]) + c[(i + 1) % n].distance(c[(i + 2) % n]);
            l(i).total_cmp(&l(j))
        })
        .unwrap_or(0);
    let (a, b, d) = (c[k], c[(k + 1) % n], c[(k + 2) % n]);
    // The camera looks along the sweep: yaw from the travel's direction.
    // The eye BEHIND each clearing along the sweep, looking ahead along the
    // ring of fights and a little outward: looking in, the town walls filled
    // the frame; looking straight out, the rim's cliffs did.
    // (`render3d::camera` puts the eye at target + (-sin yaw, cos yaw).)
    let fora = |v: Vec2| v.y.atan2(v.x) + std::f32::consts::PI - 0.35;
    let (ya, yb, yd) = (fora(a), fora(b), fora(d));
    // Unwrapped so the path turns the short way.
    let perto = |de: f32, para: f32| de + (para - de + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU) - std::f32::consts::PI;
    let yb = perto(ya, yb);
    let yd = perto(yb, yd);
    let (y0, y1) = (ya, yd);
    let meio = c.iter().fold(Vec2::ZERO, |s, p| s + *p) / n as f32;
    let raio = c.iter().map(|p| p.distance(meio)).fold(0.0f32, f32::max);
    // Far enough out at the end to hold every clearing.
    let longe = (raio / 14.0).clamp(2.6, 4.2);
    vec![
        tomada(
            13.0,
            &[
                ch(y0, 1.15, 0.62, a.x, a.y),
                ch(yb, 1.05, 0.58, b.x, b.y),
                ch(y1, 1.15, 0.62, d.x, d.y),
            ],
        ),
        tomada(
            10.0,
            &[
                ch(y1, 1.15, 0.62, d.x, d.y),
                ch(y1 + 0.35, 1.8, 0.74, d.x * 0.6 + meio.x * 0.4, d.y * 0.6 + meio.y * 0.4),
                ch(y1 + 0.7, longe, 0.95, meio.x, meio.y),
            ],
        ),
    ]
}

/// Scene by name. `None`: this zone cannot be built here.
fn monta(nome: &str) -> Option<Cena> {
    use shared::terreno::ARQUIPELAGO as A;
    let ilha = |def, foco, el, tomadas, noite, abismo| Some(Cena::Ilha(Box::new(Ilha::nova(def, foco, el, tomadas, noite, abismo))));
    match nome {
        // The Grove: the village edge, a hunt among the trees.
        "bosque" => {
            let def = &A[0];
            ilha(
                def,
                cidade(def) + vec2(-28.0, 18.0),
                elenco(5, &[3, 0, 1, 0], &[0, 1, 3, 5, 6, 2], 18),
                vec![
                    tomada(9.0, &[ch(-0.4, 3.2, 0.95, 26.0, -22.0), ch(0.2, 2.4, 0.78, 10.0, -8.0), ch(0.6, 1.7, 0.62, 0.0, 0.0)]),
                    tomada(9.0, &[ch(0.7, 1.05, 0.5, 0.0, 0.0), ch(1.5, 0.85, 0.42, 0.0, 0.0), ch(2.2, 0.95, 0.46, 0.0, 0.0)]),
                ],
                false,
                false,
            )
        }
        "geleira" => {
            let def = &A[1];
            ilha(
                def,
                cidade(def) + vec2(24.0, -20.0),
                elenco(10, &[5, 3, 0, 1, 0], &[30, 31, 32, 0, 3], 22),
                vec![
                    tomada(9.0, &[ch(2.6, 3.0, 0.85, -20.0, 20.0), ch(3.1, 2.0, 0.66, -6.0, 6.0), ch(3.5, 1.4, 0.56, 0.0, 0.0)]),
                    tomada(9.0, &[ch(3.6, 1.0, 0.48, 0.0, 0.0), ch(4.6, 0.9, 0.44, 0.0, 0.0)]),
                ],
                false,
                false,
            )
        }
        "ermo" => {
            let def = &A[2];
            ilha(
                def,
                cidade(def) + vec2(-22.0, -26.0),
                elenco(3, &[10, 1, 0, 5, 0], &[33, 34, 35, 3], 22),
                vec![
                    tomada(9.0, &[ch(5.2, 3.6, 0.8, 30.0, 10.0), ch(5.8, 2.4, 0.62, 10.0, 4.0), ch(6.3, 1.5, 0.52, 0.0, 0.0)]),
                    tomada(9.0, &[ch(0.1, 1.0, 0.45, 0.0, 0.0), ch(1.2, 0.95, 0.42, 0.0, 0.0)]),
                ],
                false,
                false,
            )
        }
        // The Plateau: fights spread over the map, a low glide past three of
        // them, then the camera pulls up and back over all of them (the
        // owner liked that reveal, 17/11/2026).
        "planalto" => {
            let def = &A[3];
            let mut el = elenco(15, &[], &crate::benchmark::KINDS_DO_PLANALTO, 0);
            el.plano = Plano {
                clareiras: 6,
                perto: 14.0,
                longe: 58.0,
                entre: 17.0,
                jogadores: vec![3, 2, 4, 3, 2, 3],
                mobs: vec![4, 3, 5, 4, 3, 4],
            };
            let s = Ilha::nova(def, cidade(def), el, vec![tomada(1.0, &[ch(0.0, 1.0, 0.6, 0.0, 0.0)])], false, false);
            Some(Cena::Ilha(Box::new(s.com_tomadas(tomadas_da_batalha))))
        }
        // Skyreach: over the clouds, from island to island.
        "celeste" => {
            let p = &shared::celeste::PLATOS;
            let foco = v2(p[0].centro);
            let ate = v2(p[1].centro) - foco;
            ilha(
                &A[4],
                foco,
                elenco(10, &[5, 3, 1, 0], &[41, 42, 43, 44, 45, 46], 20),
                vec![
                    tomada(10.0, &[ch(0.5, 7.0, 0.7, ate.x * 0.8, ate.y * 0.8), ch(0.9, 5.0, 0.6, ate.x * 0.4, ate.y * 0.4), ch(1.3, 2.4, 0.55, 0.0, 0.0)]),
                    tomada(8.0, &[ch(1.3, 1.2, 0.7, 0.0, 0.0), ch(1.9, 1.0, 0.64, 0.0, 0.0)]),
                ],
                false,
                false,
            )
        }
        // Kōgen-tō: the neon city at night, at the crossing.
        "kogen" => {
            crate::render3d::define_noite(true);
            ilha(
                &A[5],
                v2(shared::kogen::cruzamento()),
                elenco(15, &[10, 5, 3, 0], &[56, 57, 58, 59, 60, 61, 62], 22),
                vec![
                    tomada(10.0, &[ch(0.3, 4.6, 0.62, 0.0, 40.0), ch(0.7, 2.8, 0.55, 0.0, 14.0), ch(1.0, 1.4, 0.5, 0.0, 0.0)]),
                    tomada(8.0, &[ch(1.1, 1.0, 0.46, 0.0, 0.0), ch(2.0, 0.9, 0.42, 0.0, 0.0)]),
                ],
                true,
                false,
            )
        }
        // Abyssia: the bubble kingdom on the sea floor.
        "abissal" => {
            crate::render3d::define_noite(true);
            crate::render3d::define_abismo(true);
            ilha(
                &A[6],
                v2(shared::abissal::centro_da_cidade()) + vec2(0.0, 30.0),
                elenco(15, &[10, 5, 3, 1], &[70, 71, 72, 73, 75, 91, 92], 20),
                vec![
                    tomada(10.0, &[ch(0.2, 5.0, 0.75, 0.0, -40.0), ch(0.6, 3.0, 0.65, 0.0, -15.0), ch(0.9, 1.6, 0.6, 0.0, 0.0)]),
                    tomada(8.0, &[ch(1.0, 1.15, 0.6, 0.0, 0.0), ch(1.9, 1.0, 0.56, 0.0, 0.0)]),
                ],
                true,
                true,
            )
        }
        // The Magic Island's first step.
        "magica" => {
            let def = shared::terreno::def_da_zona(shared::magica::NIVEIS[0].zona)?;
            ilha(
                def,
                v2(shared::magica::CHEGADA) + vec2(6.0, -6.0),
                elenco(10, &[5, 3, 1, 0], &[0, 1, 3, 5, 6, 4], 18),
                vec![
                    tomada(10.0, &[ch(0.8, 6.0, 0.8, 30.0, 0.0), ch(1.2, 3.5, 0.65, 12.0, 0.0), ch(1.5, 1.5, 0.55, 0.0, 0.0)]),
                    tomada(8.0, &[ch(1.6, 1.0, 0.46, 0.0, 0.0), ch(2.5, 0.9, 0.44, 0.0, 0.0)]),
                ],
                false,
                false,
            )
        }
        "mar" => Some(Cena::Mar(Box::new(Mar::novo()))),
        // The six rebirth tiers in a row, the camera dollying past.
        "auras" => {
            use std::f32::consts::PI;
            // The eye on the far side of the row (the house was the backdrop),
            // the target behind the row so the bodies sit low in the frame.
            let def = &A[0];
            let mut el = elenco(1, &[3, 5, 10, 15, 55], &[0], 0);
            el.fila = true;
            ilha(
                def,
                // A little off the old spot: a tree hid R15 at the end of the dolly.
                cidade(def) + vec2(-31.0, 18.0),
                el,
                vec![tomada(
                    14.0,
                    &[ch(PI - 0.3, 0.54, 0.3, -4.6, 2.1), ch(PI, 0.5, 0.27, 0.0, 2.0), ch(PI + 0.3, 0.54, 0.3, 4.6, 2.1)],
                )],
                false,
                false,
            )
        }
        // A field boss on the Plateau: the Thunderhide Behemoth.
        "chefe" => {
            let def = &A[3];
            let mut el = elenco(15, &[10, 5, 3, 1, 0, 0], &[0], 0);
            el.chefe = Some(22);
            // Seven players round it: the Luta's boss mode.
            el.roda = 5.0;
            let foco = shared::terreno::Gerador::da_ilha(def).planalto().map_or(cidade(def), |p| v2(p.regioes[0].centro));
            ilha(
                def,
                foco,
                el,
                vec![
                    tomada(10.0, &[ch(0.4, 0.6, 0.26, 0.0, 0.0), ch(1.3, 0.68, 0.3, 0.0, 0.0), ch(2.2, 0.8, 0.36, 0.0, 0.0)]),
                    tomada(8.0, &[ch(2.4, 0.9, 0.42, 0.0, 0.0), ch(3.2, 1.2, 0.55, 0.0, 0.0)]),
                ],
                false,
                false,
            )
        }
        _ => None,
    }
}

// ── the run ──────────────────────────────────────────────────────────────

struct Config {
    w: u32,
    h: u32,
    nome_tam: String,
    saida: String,
    prints: usize,
    prints_em: Vec<f32>,
    so_prints: bool,
    limpo: bool,
    amostras: i32,
    /// `MMO_FILME_ESCALA=W:H`: the films downscaled (lanczos) as they are
    /// encoded — render at 2x, keep the supersampled 1080p.
    escala: Option<String>,
    /// `MMO_FILME_CRF` / `MMO_FILME_PRESET` for the scene films (18, slow).
    crf: String,
    preset: String,
    /// `MMO_FILME_CAMERA=file.csv`: every frame's camera, for the
    /// frame-to-frame check (`scripts/filme-camera.py`).
    camera: Option<String>,
    /// `MMO_FILME_SO_CAMERA=1`: no video, only the camera file (every frame
    /// is still drawn, so run it small).
    so_camera: bool,
}

fn config() -> Config {
    let (w, h, nome_tam) = std::env::var("MMO_FILME_TAM")
        .ok()
        .and_then(|s| tamanho(&s))
        .unwrap_or_else(|| (1920, 1080, "16x9".into()));
    let lista = |v: &str| -> Vec<f32> {
        std::env::var(v).ok().map_or(Vec::new(), |s| s.split(',').filter_map(|x| x.trim().parse().ok()).collect())
    };
    Config {
        w,
        h,
        nome_tam,
        saida: std::env::var("MMO_FILME_SAIDA").unwrap_or_else(|_| "target/filme".into()),
        prints: std::env::var("MMO_FILME_PRINTS").ok().and_then(|s| s.parse().ok()).unwrap_or(0),
        prints_em: lista("MMO_FILME_PRINTS_EM"),
        so_prints: std::env::var("MMO_FILME_SO_PRINTS").is_ok_and(|s| !s.is_empty() && s != "0"),
        // Stills for the stores are clean unless asked otherwise.
        limpo: std::env::var("MMO_FILME_LIMPO").map_or(std::env::var("MMO_FILME_SO_PRINTS").is_ok_and(|s| !s.is_empty() && s != "0"), |s| !s.is_empty() && s != "0"),
        amostras: std::env::var("MMO_FILME_MSAA").ok().and_then(|s| s.parse().ok()).unwrap_or(8),
        escala: std::env::var("MMO_FILME_ESCALA").ok().filter(|s| !s.is_empty()),
        crf: std::env::var("MMO_FILME_CRF").unwrap_or_else(|_| "18".into()),
        preset: std::env::var("MMO_FILME_PRESET").unwrap_or_else(|_| "slow".into()),
        camera: std::env::var("MMO_FILME_CAMERA").ok().filter(|s| !s.is_empty()),
        so_camera: std::env::var("MMO_FILME_SO_CAMERA").is_ok_and(|s| !s.is_empty() && s != "0"),
    }
}

/// ffmpeg reading raw RGBA from stdin.
fn ffmpeg(c: &Config, caminho: &str) -> std::process::Child {
    // The GL readback is bottom-up: flip it (the PNG stills already were;
    // the films came out upside down, 17/11/2026).
    let filtro = match &c.escala {
        Some(e) => format!("vflip,scale={e}:flags=lanczos"),
        None => "vflip".into(),
    };
    std::process::Command::new("ffmpeg")
        .args(["-y", "-nostdin", "-loglevel", "error", "-f", "rawvideo", "-pix_fmt", "rgba", "-s"])
        .arg(format!("{}x{}", c.w, c.h))
        .args(["-r", &QPS.to_string(), "-i", "-", "-vf", &filtro, "-an", "-c:v", "libx264", "-preset", &c.preset, "-crf", &c.crf])
        .args(["-pix_fmt", "yuv420p", "-movflags", "+faststart"])
        .arg(caminho)
        .stdin(std::process::Stdio::piped())
        .spawn()
        .expect("ffmpeg")
}

pub async fn rodar(vox: &mut crate::vox::VoxCache, cenas: Vec<&'static str>) {
    let c = config();
    std::fs::create_dir_all(&c.saida).unwrap();
    for _ in 0..30 {
        while vox.atende_um_pendente(crate::render3d::VOXEL).await {}
        clear_background(BLACK);
        next_frame().await;
    }
    let solido = crate::render3d::material_solido();
    // Ultra, as the benchmark publishes a preset: only the renderer's statics.
    let mut escolha = cg::preset(cg::PRESETS.len() - 1);
    let raio_da_captura = std::env::var("MMO_FILME_RAIO").ok().and_then(|s| s.parse().ok()).unwrap_or(RAIO_DA_CAPTURA).clamp(3, 40);
    escolha.raio = raio_da_captura;
    cg::publica_para_teste(&escolha);
    let rt = render_target_ex(c.w, c.h, RenderTargetParams { depth: true, sample_count: c.amostras });
    rt.texture.set_filter(FilterMode::Linear);
    crate::render3d::define_alvo(Some(rt.clone()));
    // The 2D over the world (numbers, bars) at the size it has at 1080p.
    crate::render3d::define_escala_do_alvo(c.w.min(c.h) as f32 / 1080.0);
    let aspecto = c.w as f32 / c.h as f32;
    let mut ctx = Contexto { rt: rt.clone(), aspecto, recuo: recuo_do_aspecto(aspecto), escolha, limpo: c.limpo };
    let mut relogio = 100.0f64;
    let mut log = c.camera.as_ref().map(|p| {
        let mut f = std::io::BufWriter::new(std::fs::File::create(p).expect("MMO_FILME_CAMERA"));
        writeln!(f, "cena,tomada,quadro,ox,oy,oz,ax,ay,az,vx,vy,vz,folga").unwrap();
        f
    });
    for nome in cenas {
        // Beyond Ultra for the capture; the sea's own band follows its radius.
        ctx.escolha.raio = if nome == "mar" { RAIO_DA_CAPTURA_NO_MAR.min(raio_da_captura.max(12)) } else { raio_da_captura };
        cg::publica_para_teste(&ctx.escolha);
        let r = ctx.escolha.raio as f32 * crate::terreno::CHUNK as f32 * shared::terreno::BLOCO;
        crate::construcoes::define_alcance_da_captura(r - 16.0);
        let Some(mut cena) = monta(nome) else {
            println!("[filme] {nome}: this zone cannot be built locally, skipped");
            continue;
        };
        let duracoes = cena.duracoes();
        let total: f32 = duracoes.iter().sum();
        let prints = quadros_dos_prints(c.prints, &c.prints_em, total);
        let ultimo_print = prints.last().copied();
        let base = format!("{}/tempest-{nome}-{}", c.saida, c.nome_tam);
        let mut enc = (!c.so_prints && !c.so_camera).then(|| ffmpeg(&c, &format!("{base}.mp4")));
        let inicio = get_time();
        let mut quadro_global = 0u32;
        'tomadas: for (i, dur) in duracoes.iter().enumerate() {
            // Bake the shot's ground, then a second of warm-up (uploads,
            // shader variants, the crowd settling) that is not recorded.
            crate::relogio::fixa(Some((relogio, DT)));
            for k in 0..3000 {
                // The outfits, hats and weapon skins the bodies asked for
                // while drawing (`rig_ou_pede`): served only before the first
                // scene, every cash outfit stayed the default shirt.
                while vox.atende_um_pendente(crate::render3d::VOXEL).await {}
                if cena.prepara(i, &ctx) && k > 2 {
                    break;
                }
                set_camera(&Camera2D { render_target: Some(rt.clone()), ..Default::default() });
                clear_background(BLACK);
                next_frame().await;
            }
            for _ in 0..QPS {
                while vox.atende_um_pendente(crate::render3d::VOXEL).await {}
                relogio += DT as f64;
                crate::relogio::fixa(Some((relogio, DT)));
                let _ = cena.quadro(i, 0.0, vox, &solido, &ctx);
                next_frame().await;
            }
            let n = (dur * QPS as f32).round() as u32;
            for q in 0..n {
                while vox.atende_um_pendente(crate::render3d::VOXEL).await {}
                if enc.is_none() && !c.so_camera && ultimo_print.is_none_or(|u| quadro_global > u) {
                    break 'tomadas;
                }
                relogio += DT as f64;
                crate::relogio::fixa(Some((relogio, DT)));
                let m = cena.quadro(i, q as f32 * DT, vox, &solido, &ctx);
                if let Some(f) = &mut log {
                    let (o, a, v) = (m.olho, m.alvo, m.olho_antigo);
                    writeln!(f, "{nome},{i},{q},{},{},{},{},{},{},{},{},{},{}", o.x, o.y, o.z, a.x, a.y, a.z, v.x, v.y, v.z, m.folga).unwrap();
                }
                unsafe { get_internal_gl().flush() };
                let precisa_print = prints.binary_search(&quadro_global).is_ok();
                if enc.is_some() || precisa_print {
                    let mut img = rt.texture.get_texture_data();
                    for px in img.bytes.chunks_mut(4) {
                        px[3] = 255;
                    }
                    if let Some(p) = &mut enc {
                        p.stdin.as_mut().unwrap().write_all(&img.bytes).expect("ffmpeg stdin");
                    }
                    if precisa_print {
                        let t = quadro_global as f32 * DT;
                        img.export_png(&format!("{base}-{t:05.2}s.png"));
                    }
                }
                quadro_global += 1;
                next_frame().await;
            }
        }
        if let Some(mut p) = enc {
            drop(p.stdin.take());
            let ok = p.wait().is_ok_and(|s| s.success());
            println!("[filme] {base}.mp4: {quadro_global} frames, {:.0} s to render, ffmpeg {}", get_time() - inicio, if ok { "ok" } else { "FAILED" });
        } else {
            println!("[filme] {nome}: {} stills", prints.len());
        }
        cena.sai();
    }
    crate::relogio::fixa(None);
    crate::render3d::define_alvo(None);
    crate::render3d::define_escala_do_alvo(1.0);
}

#[cfg(test)]
mod testes {
    use super::*;

    /// The capture never starts by itself: off unless `MMO_FILME` names a
    /// scene, and the game binary only enters it then.
    #[test]
    fn o_filme_so_liga_com_a_variavel() {
        assert_eq!(pedido(None), None);
        assert_eq!(pedido(Some("")), None);
        assert_eq!(pedido(Some("0")), None);
        assert_eq!(pedido(Some("nada")), None);
        assert_eq!(pedido(Some("bosque")), Some(vec!["bosque"]));
        assert_eq!(pedido(Some("mar, auras")), Some(vec!["mar", "auras"]));
        assert_eq!(pedido(Some("todas")).unwrap().len(), CENAS.len());
        // And this process (the test run) has it off.
        assert_eq!(pedido(std::env::var("MMO_FILME").ok().as_deref()), None);
        assert!(!crate::relogio::fixo());
    }

    #[test]
    fn tamanhos_e_paisagem() {
        assert_eq!(tamanho("16x9"), Some((1920, 1080, "16x9".into())));
        assert_eq!(tamanho("9x16"), Some((1080, 1920, "9x16".into())));
        assert_eq!(tamanho("iphone67"), Some((1290, 2796, "iphone67".into())));
        assert_eq!(tamanho("ipad13-paisagem"), Some((2752, 2064, "ipad13-paisagem".into())));
        assert_eq!(tamanho("2048x2732"), Some((2048, 2732, "2048x2732".into())));
        assert_eq!(tamanho("16x9-paisagem").unwrap().0, 1920);
        assert_eq!(tamanho("10x10"), None);
        assert_eq!(tamanho("lixo"), None);
        assert!(recuo_do_aspecto(16.0 / 9.0) == 1.0 && recuo_do_aspecto(9.0 / 16.0) > 1.7);
    }

    #[test]
    fn prints_espalhados_e_nos_tempos_pedidos() {
        let v = quadros_dos_prints(5, &[3.5, 100.0], 20.0);
        assert_eq!(v, vec![120, 210, 360, 600, 840, 1080]);
        assert!(quadros_dos_prints(0, &[], 20.0).is_empty());
        assert_eq!(quadros_dos_prints(1, &[10.0], 20.0), vec![600]);
    }

    /// The camera's ground over block steps: the raw height jumps half a
    /// unit at each block edge; the camera's (`chao_suave` + `mola`) never
    /// moves more than a couple of centimetres a frame, and no faster in
    /// one frame than its neighbours by much (no kink).
    #[test]
    fn a_camera_nao_pula_nos_degraus() {
        let degrau = |x: f32, _z: f32| (x / 0.5).floor() * 0.25;
        let (mut h, mut v) = (f32::MIN, 0.0);
        let mut ys = Vec::new();
        for q in 0..600 {
            // 4 u/s across the steps, as the Plateau glide does.
            let alvo = vec2(q as f32 * DT * 4.0, 0.0);
            (h, v) = mola(h, v, chao_suave(alvo, &degrau), DT, 0.5);
            ys.push(h);
        }
        let d: Vec<f32> = ys.windows(2).map(|w| w[1] - w[0]).collect();
        assert!(d.iter().all(|x| x.abs() < 0.04), "{:?}", d.iter().fold(0.0f32, |a, b| a.max(b.abs())));
        let kink = d.windows(2).skip(60).map(|w| (w[1] - w[0]).abs()).fold(0.0f32, f32::max);
        assert!(kink < 0.002, "kink {kink}");
        // And the near plane is never the game's 0.01, nor in front of the target.
        let (perto, longe) = planos_da_captura(8.0);
        assert!(perto >= 0.5 && perto < 8.0 * 0.5 && longe > 1000.0);
    }

    #[test]
    fn o_caminho_passa_pelas_chaves() {
        let k = [ch(0.0, 1.0, 0.5, 0.0, 0.0), ch(1.0, 2.0, 0.6, 5.0, 0.0), ch(2.0, 1.0, 0.5, 10.0, 0.0)];
        assert!((caminho(&k, 0.0).yaw).abs() < 1e-5);
        assert!((caminho(&k, 1.0).dx - 10.0).abs() < 1e-4);
        assert!((caminho(&k, 0.5).zoom - 2.0).abs() < 1e-4);
        // Smooth: small steps between neighbouring frames.
        let mut ant = caminho(&k, 0.0);
        for i in 1..=600 {
            let c = caminho(&k, i as f32 / 600.0);
            assert!((c.yaw - ant.yaw).abs() < 0.02);
            ant = c;
        }
    }
}
