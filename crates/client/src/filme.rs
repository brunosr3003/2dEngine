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
use shared::{EntityId, EntityMeta, EntityState, EntityTag};

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
/// never inside the ground, into the capture target.
fn camera_de(alvo: Vec2, chao_alvo: f32, c: &Chave, recuo: f32, chao: &dyn Fn(f32, f32) -> f32, ctx: &Contexto) -> Camera3D {
    let mut cam = crate::render3d::camera(alvo, chao_alvo, c.yaw, 1.0, c.pitch);
    cam.position = cam.target + (cam.position - cam.target) * c.zoom * recuo;
    let minimo = chao(cam.position.x, cam.position.z) + 1.2;
    if cam.position.y < minimo {
        cam.position.y = minimo;
    }
    cam.aspect = Some(ctx.aspecto);
    cam.render_target = Some(ctx.rt.clone());
    cam.z_far = 4000.0;
    cam
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
    /// Ring radius of the players.
    roda: f32,
}

fn meta(id: u32, tag: EntityTag, kind: u16, auras: u64, renascimentos: u8, aparencia: u32, hp: u32) -> EntityMeta {
    EntityMeta {
        skins: 0,
        cla: String::new(),
        pk: Default::default(),
        auras,
        id: EntityId(id),
        tag,
        name: None,
        hp_max: hp,
        faction: None,
        kind,
        nivel: 80,
        desafio: None,
        aparencia,
        poder: 0,
        renascimentos,
    }
}

fn aparencia_de(k: u32) -> u32 {
    shared::aparencia::Aparencia {
        rosto: (k % 6) as u8 + 1,
        cabelo: (k * 3 % 9) as u8 + 1,
        cor_cabelo: (k * 5 % 7) as u8,
        pele: (k % 4) as u8 + 1,
        ..Default::default()
    }
    .empacota()
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
    luz: Option<Material>,
    foco: Vec2,
    elenco: Elenco,
    tomadas: Vec<Tomada>,
    noite: bool,
    abismo: bool,
    relogio: f32,
    proximo_passo: f32,
    proxima_dano: f32,
}

impl Ilha {
    #[allow(clippy::too_many_arguments)]
    fn nova(def: &'static shared::terreno::DefIlha, foco: Vec2, elenco: Elenco, tomadas: Vec<Tomada>, noite: bool, abismo: bool) -> Self {
        let mut s = Self {
            def,
            terreno: crate::terreno::Terreno::novo(def),
            casas: crate::construcoes::Construcoes::para(Some(def)),
            mundo: crate::world::World::default(),
            luz: None,
            foco,
            elenco,
            tomadas,
            noite,
            abismo,
            relogio: 0.0,
            proximo_passo: 0.0,
            proxima_dano: 0.0,
        };
        s.povoa();
        s
    }

    fn centro_da_roda(&self) -> Vec2 {
        self.foco
    }

    fn povoa(&mut self) {
        let el = self.elenco.clone();
        let combate = shared::components::acao::monta(0, true, 0, 0);
        let mut metas = vec![meta(EU, EntityTag::Player, 0, 0, el.renascimentos, aparencia_de(0), 1000)];
        for (k, r) in el.jogadores.iter().enumerate() {
            let tier = (k % 4) as u64;
            let bits = if el.fila { 0 } else { (0..shared::auras::SLOTS).fold(0u64, |b, s| b | ((3 | (tier << 3)) << (s * 8))) };
            metas.push(meta(10 + k as u32, EntityTag::Player, 0, bits, *r, aparencia_de(k as u32 + 1), 1000));
        }
        if let Some(kind) = el.chefe {
            metas.push(meta(CHEFE, EntityTag::Enemy, kind, 0, 0, 0, 100_000));
        }
        for k in 0..el.inimigos {
            metas.push(meta(100 + k, EntityTag::Enemy, el.kinds[k as usize % el.kinds.len()], 0, 0, 0, 1000));
        }
        let mut estados = self.posicoes(0.0);
        for s in &mut estados {
            s.acao = combate;
        }
        self.mundo.apply(metas, estados, &[]);
    }

    /// Everyone on slowly turning rings (the benchmark's crowd), or in a row.
    fn posicoes(&self, t: f32) -> Vec<EntityState> {
        let el = &self.elenco;
        let combate = shared::components::acao::monta(0, !el.fila, 0, 0);
        let c = self.centro_da_roda();
        let g = |p: Vec2| ::glam::Vec2::new(p.x, p.y);
        let mut v = Vec::new();
        let est = |id: u32, p: Vec2, d: Vec2, hp: i32, flags: u8| {
            let mut s = EntityState::quantize(EntityId(id), g(p), g(d), hp, flags);
            s.acao = combate;
            s
        };
        if el.fila {
            let n = el.jogadores.len() as f32 + 1.0;
            let x0 = -(n - 1.0) * 0.5 * 2.6;
            v.push(est(EU, c + vec2(x0, 0.0), Vec2::ZERO, 1000, shared::ent_flags::SELF));
            for k in 0..el.jogadores.len() as u32 {
                v.push(est(10 + k, c + vec2(x0 + (k as f32 + 1.0) * 2.6, 0.0), Vec2::ZERO, 1000, 0));
            }
            return v;
        }
        let no_anel = |raio: f32, a: f32, vel: f32| (c + vec2(a.cos(), a.sin()) * raio, vec2(-a.sin(), a.cos()) * vel);
        // With a boss, it takes the middle and I join the ring.
        if el.chefe.is_some() {
            let (p, d) = no_anel(el.roda, 1.2 + t * 0.15, 0.4);
            v.push(est(EU, p, d, 1000, shared::ent_flags::SELF));
            let hp = (1000.0 - (t * 9.0) % 700.0) as i32;
            v.push(est(CHEFE, c, Vec2::ZERO, hp, shared::ent_flags::BOSS | shared::ent_flags::ATACANDO));
        } else {
            v.push(est(EU, c, Vec2::ZERO, 1000, shared::ent_flags::SELF));
        }
        let n = el.jogadores.len().max(1) as f32;
        for k in 0..el.jogadores.len() as u32 {
            let a = k as f32 / n * std::f32::consts::TAU + t * 0.15;
            let (p, d) = no_anel(el.roda, a, 0.4);
            v.push(est(10 + k, p, d, 1000, 0));
        }
        for k in 0..el.inimigos {
            let anel = el.roda + 1.9 + (k % 3) as f32 * 1.8;
            let a = k as f32 * 2.399 + t * 0.25;
            let (p, d) = no_anel(anel, a, 1.2);
            v.push(est(100 + k, p, d, 800, shared::ent_flags::ATACANDO));
        }
        v
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
        let raio = ctx.escolha.raio;
        self.casas.acompanhar();
        let mut pronto = self.casas.prontas();
        // The whole path: the start, the middle and the end.
        for u in [0.0, 0.5, 1.0] {
            let a = self.alvo(i, u);
            self.terreno.atualiza(a, raio, 64);
            let (feitos, total) = self.terreno.prontos_em(a, raio);
            pronto &= feitos >= total;
        }
        pronto
    }

    fn quadro(&mut self, i: usize, t: f32, vox: &crate::vox::VoxCache, solido: &Material, ctx: &Contexto) {
        self.relogio += DT;
        let u = t / self.tomadas[i].segundos;
        let c = caminho(&self.tomadas[i].chaves, u);
        let alvo = self.foco + vec2(c.dx, c.dz);
        let e = &ctx.escolha;
        self.casas.acompanhar();
        self.terreno.atualiza(alvo, e.raio, 24);
        if self.relogio >= self.proximo_passo {
            self.proximo_passo = self.relogio + 0.25;
            let estados = self.posicoes(self.relogio);
            self.mundo.apply(Vec::new(), estados, &[]);
        }
        let t_ = &self.terreno;
        self.mundo.tick(DT, &|x, z, _| t_.altura(x, z));
        let fila = self.elenco.fila;
        for (id, ent) in self.mundo.ents.iter_mut() {
            ent.sacada = 1.0;
            if ent.meta.tag == EntityTag::Enemy && ent.golpe > 0.9 + (id.0 % 5) as f32 * 0.2 {
                ent.golpe = 0.0;
            }
            if fila {
                // Facing the camera.
                ent.yaw = c.yaw;
            }
        }
        let alvos: Vec<u32> = if self.elenco.chefe.is_some() {
            vec![CHEFE]
        } else {
            (0..self.elenco.inimigos).map(|k| 100 + k).collect()
        };
        if !fila && !alvos.is_empty() && self.relogio >= self.proxima_dano {
            self.proxima_dano = self.relogio + if self.elenco.chefe.is_some() { 0.11 } else { 0.07 };
            let k = (self.relogio * 1000.0) as u32;
            self.mundo.efeitos.push(crate::world::Efeito {
                alvo: EntityId(alvos[k as usize % alvos.len()]),
                dano: if self.elenco.chefe.is_some() { 4000 + (k * 37 % 9000) as i32 } else { 900 + (k * 37 % 4000) as i32 },
                critico: k % 4 == 0,
                eu: false,
                t: 0.0,
                semente: (k % 1000) as f32 / 1000.0,
            });
        }
        let t = &self.terreno;
        let chao = |x: f32, z: f32| t.altura(x, z);
        let apoio = t.altura(alvo.x, alvo.y);
        let mut vista = crate::render3d::Vista::nova(alvo, c.yaw, 1.0, c.pitch, apoio, &chao);
        vista.cam = camera_de(alvo, apoio, &c, ctx.recuo, &chao, ctx);
        set_camera(&vista.cam);
        crate::render3d::clear();
        let luz = crate::render3d::luz_dia(if e.sombras == cg::Sombras::Bonitas { 1.0 } else { 0.0 });
        solido.set_uniform("LuzDia", luz);
        crate::gpu_estatica::define_luz_dia(luz);
        let (inicio, fim) = cg::neblina();
        // A pulled-back camera sees farther: the haze goes with it.
        let longe = (vista.cam.position - vista.cam.target).length();
        let (inicio, fim) = (inicio + longe * 0.5, fim + longe * 0.5);
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
        if !fila && !alvos.is_empty() {
            let luz = self.luz.get_or_insert_with(crate::habilidades_vfx::material);
            // On a boss, three casters in turn: seven at once bury it in light.
            let lancadores = if self.elenco.chefe.is_some() { 3 } else { 1 + self.elenco.jogadores.len() as u32 };
            for k in 0..lancadores {
                let id = if k == 0 { EU } else { 10 + k - 1 };
                let Some(de) = self.mundo.ents.get(&EntityId(id)).map(|x| x.render_pos) else { continue };
                let Some(a) = self.mundo.ents.get(&EntityId(alvos[(k as usize * 5) % alvos.len()])).map(|x| x.render_pos) else { continue };
                let de3 = vec3(de.x, t.altura(de.x, de.y), de.y);
                let alvo3 = vec3(a.x, t.altura(a.x, a.y), a.y);
                let skill = 1 + (k % 12);
                let ciclo = crate::habilidades_vfx::duracao(skill) + 0.4;
                let passo = if self.elenco.chefe.is_some() { ciclo / 3.0 } else { 0.37 };
                let fase = (self.relogio + k as f32 * passo) % ciclo;
                let frente = vec3(alvo3.x - de3.x, 0.0, alvo3.z - de3.z).normalize_or_zero();
                let cena = |impacto: bool, tt: f32| crate::habilidades_vfx::Cena {
                    id: skill,
                    de: de3,
                    alvo: alvo3,
                    maos: [de3 + Vec3::Y * 1.2; 2],
                    t: tt,
                    impacto,
                    atraso: 0.4,
                    raio: 3.0,
                    frente,
                    tier: 10,
                };
                crate::habilidades_vfx::desenha(&cena(false, fase.min(0.9)), luz);
                if fase > 0.4 {
                    crate::habilidades_vfx::desenha(&cena(true, fase - 0.4), luz);
                }
            }
            gl_use_default_material();
        }
        crate::render3d::camera_padrao();
        if !fila && !ctx.limpo {
            crate::efeitos::numeros(&self.mundo, &vista);
        }
    }
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
                let mut m = meta(*id, *tag, *kind, 0, 0, 0, 100);
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

    fn prepara(&mut self, i: usize, ctx: &Contexto) -> bool {
        let raio = ctx.escolha.raio.max(cg::RAIO_PADRAO + 1);
        let mut pronto = true;
        for u in [0.0, 0.5, 1.0] {
            let a = self.alvo(i, u);
            self.terreno.atualiza(a, raio, 64);
            let (feitos, total) = self.terreno.prontos_em(a, raio);
            pronto &= feitos >= total;
        }
        pronto
    }

    fn quadro(&mut self, i: usize, t: f32, solido: &Material, ctx: &Contexto) {
        use shared::navio::Visual;
        let u = t / MAR_TOMADAS[i];
        let yaw_de = |d: Vec2| d.x.atan2(d.y);
        let raio = ctx.escolha.raio.max(cg::RAIO_PADRAO + 1);
        let alvo = self.alvo(i, u);
        self.terreno.atualiza(alvo, raio, 24);
        crate::mar_ui::define_no_mar(true);
        let sem_chao = |_: f32, _: f32| 0.0f32;
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
                (camera_de(foco, 0.0, &c, ctx.recuo, &sem_chao, ctx), frota, false)
            }
            // The broadside fight.
            1 => {
                let tt = self.proximo_passo;
                if t >= tt {
                    self.proximo_passo = t + 0.25;
                    self.povoa(t);
                }
                self.mundo.tick(DT, &|_, _, _| 0.0);
                let eu = self.mundo.ents.get(&EntityId(EU)).map_or(self.aberto, |e| e.render_pos);
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
                (camera_de(eu, 0.0, &c, ctx.recuo, &sem_chao, ctx), navios, true)
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
                (camera_de(foco, 0.0, &c, ctx.recuo, &sem_chao, ctx), navios, false)
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
            self.vfx.desenha_3d(&self.mundo, &cam, None);
        }
        apaga_luz_e_neblina(solido);
        crate::render3d::camera_padrao();
        crate::mar_ui::define_no_mar(false);
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

    fn quadro(&mut self, i: usize, t: f32, vox: &crate::vox::VoxCache, solido: &Material, ctx: &Contexto) {
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
    Elenco { renascimentos, jogadores: jogadores.to_vec(), kinds: kinds.to_vec(), inimigos, chefe: None, fila: false, roda: 2.6 }
}

fn tomada(segundos: f32, chaves: &[Chave]) -> Tomada {
    Tomada { segundos, chaves: chaves.to_vec() }
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
        // The benchmark's big fight, as it was.
        "planalto" => {
            let def = &A[3];
            ilha(
                def,
                cidade(def),
                elenco(15, &[0, 0, 0, 0, 0, 0], &crate::benchmark::KINDS_DO_PLANALTO, 32),
                vec![
                    tomada(10.0, &[ch(0.6, 1.0, crate::render3d::pitch_do_zoom(1.0), 0.0, 0.0), ch(1.6, 0.85, crate::render3d::pitch_do_zoom(0.85), 0.0, 0.0)]),
                    tomada(10.0, &[ch(1.8, 1.0, 0.55, 0.0, 0.0), ch(2.4, 1.5, 0.72, 0.0, 0.0), ch(2.9, 2.6, 0.95, 0.0, 0.0)]),
                ],
                false,
                false,
            )
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
            let def = &A[0];
            let mut el = elenco(1, &[3, 5, 10, 15, 55], &[0], 0);
            el.fila = true;
            ilha(
                def,
                cidade(def) + vec2(-28.0, 18.0),
                el,
                vec![tomada(16.0, &[ch(-0.3, 0.5, 0.3, -7.8, 0.0), ch(0.0, 0.42, 0.26, 0.0, 0.0), ch(0.3, 0.5, 0.3, 7.8, 0.0)])],
                false,
                false,
            )
        }
        // A field boss on the Plateau: the Thunderhide Behemoth.
        "chefe" => {
            let def = &A[3];
            let mut el = elenco(15, &[10, 5, 3, 1, 0, 0], &[0], 0);
            el.chefe = Some(22);
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
    }
}

/// ffmpeg reading raw RGBA from stdin.
fn ffmpeg(c: &Config, caminho: &str) -> std::process::Child {
    std::process::Command::new("ffmpeg")
        .args(["-y", "-loglevel", "error", "-f", "rawvideo", "-pix_fmt", "rgba", "-s"])
        .arg(format!("{}x{}", c.w, c.h))
        .args(["-r", &QPS.to_string(), "-i", "-", "-an", "-c:v", "libx264", "-preset", "slow", "-crf", "18"])
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
    let escolha = cg::preset(cg::PRESETS.len() - 1);
    cg::publica_para_teste(&escolha);
    let rt = render_target_ex(c.w, c.h, RenderTargetParams { depth: true, sample_count: c.amostras });
    rt.texture.set_filter(FilterMode::Linear);
    crate::render3d::define_alvo(Some(rt.clone()));
    // The 2D over the world (numbers, bars) at the size it has at 1080p.
    crate::render3d::define_escala_do_alvo(c.w.min(c.h) as f32 / 1080.0);
    let aspecto = c.w as f32 / c.h as f32;
    let ctx = Contexto { rt: rt.clone(), aspecto, recuo: recuo_do_aspecto(aspecto), escolha, limpo: c.limpo };
    let mut relogio = 100.0f64;
    for nome in cenas {
        let Some(mut cena) = monta(nome) else {
            println!("[filme] {nome}: this zone cannot be built locally, skipped");
            continue;
        };
        let duracoes = cena.duracoes();
        let total: f32 = duracoes.iter().sum();
        let prints = quadros_dos_prints(c.prints, &c.prints_em, total);
        let ultimo_print = prints.last().copied();
        let base = format!("{}/tempest-{nome}-{}", c.saida, c.nome_tam);
        let mut enc = (!c.so_prints).then(|| ffmpeg(&c, &format!("{base}.mp4")));
        let inicio = get_time();
        let mut quadro_global = 0u32;
        'tomadas: for (i, dur) in duracoes.iter().enumerate() {
            // Bake the shot's ground, then a second of warm-up (uploads,
            // shader variants, the crowd settling) that is not recorded.
            crate::relogio::fixa(Some((relogio, DT)));
            for k in 0..3000 {
                if cena.prepara(i, &ctx) && k > 2 {
                    break;
                }
                set_camera(&Camera2D { render_target: Some(rt.clone()), ..Default::default() });
                clear_background(BLACK);
                next_frame().await;
            }
            for _ in 0..QPS {
                relogio += DT as f64;
                crate::relogio::fixa(Some((relogio, DT)));
                cena.quadro(i, 0.0, vox, &solido, &ctx);
                next_frame().await;
            }
            let n = (dur * QPS as f32).round() as u32;
            for q in 0..n {
                if enc.is_none() && ultimo_print.is_none_or(|u| quadro_global > u) {
                    break 'tomadas;
                }
                relogio += DT as f64;
                crate::relogio::fixa(Some((relogio, DT)));
                cena.quadro(i, q as f32 * DT, vox, &solido, &ctx);
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
