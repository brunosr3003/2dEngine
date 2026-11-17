//! THE AD'S FIGHTS (`filme`, debug desktop only, docs/FILME.md).
//!
//! The first films had a crowd on slowly turning rings: positions with no
//! strikes behind them, so nobody swung, bit, flinched or fell (the owner,
//! 17/11/2026: "they just walk in circles"). This drives the bodies the way
//! the live game does, with no server: small groups of players spread over
//! the map, each against a few mobs walking in from the edges. Players run
//! in, strike combos (`acao::GOLPE`, the variant stepping 0-1-2), cast
//! skills (`Ent::skill`, the effects through the game's own `Habilidades`),
//! leap, kite back when a mob gets close; mobs lunge and bite
//! (`Ent::ataque_mob`), everyone flinches when hit (`Ent::ferido`), mobs die
//! (hp 0 topples them) and new ones arrive. A boss swings telegraphed slams
//! (`chefe_anim::Carga` + `telegrafico`) and the players dodge out of them.
//!
//! The looks are the shop's: cash outfits and hats, weapon skins, gear auras
//! (blue/purple T4 +12, green +5) and rebirth auras (`HEROIS`).

#![cfg(all(debug_assertions, not(any(target_os = "ios", target_os = "android"))))]

use macroquad::prelude::*;
use shared::bosses::Forma as FormaDoChefe;
use shared::components::acao;
use shared::skills::{Forma, Skill};
use shared::{ent_flags, EntityId, EntityMeta, EntityState, EntityTag};

use crate::relogio::get_time;
use crate::world::World;

// ── the looks ────────────────────────────────────────────────────────────

/// One showcase character: cash outfit, hat, weapon skin on its set, gear
/// aura (grade, tier, refine) and rebirths.
#[derive(Clone, Copy, Debug)]
pub struct Heroi {
    pub roupa: u16,
    pub chapeu: u16,
    /// Weapon skin id (`aparencia::ARMA_SKIN_BASE` + i); its set is the
    /// weapon the hero fights with.
    pub arma: u16,
    pub aura: (u8, u8, u8),
    pub renascimentos: u8,
    pub pele: u8,
    pub rosto: u8,
}

const fn h(roupa: u16, chapeu: u16, arma: u16, aura: (u8, u8, u8), renascimentos: u8, pele: u8, rosto: u8) -> Heroi {
    Heroi { roupa, chapeu, arma, aura, renascimentos, pele, rosto }
}

/// The lineup, left to right by rebirths: every paid outfit, paid hats,
/// a weapon skin of each set, the gear auras the owner named and R1..R15.
/// Outfits 483 Storm Captain, 486 Corsair, 487 Arcanist, 488 Knight, 489
/// Dune Nomad; hats 501 Pointed, 502 Tricorne, 509 Captain's, 510 Knight
/// Helm, 511 Horned Helm, 512 Turban, 513 Crown; weapons 602 Royal Guard,
/// 603 Sakura, 605 Crimson Moon, 606 Gilded, 608 Thunder, 610 Frost Ring,
/// 613 Void Ring.
pub const HEROIS: [Heroi; 6] = [
    h(486, 502, 606, (2, 3, 5), 1, 0, 0),
    h(489, 512, 603, (3, 4, 12), 2, 2, 1),
    h(487, 501, 610, (3, 4, 12), 3, 1, 2),
    h(483, 509, 608, (4, 4, 12), 5, 3, 0),
    h(488, 510, 602, (4, 4, 12), 10, 0, 1),
    h(483, 513, 605, (4, 4, 12), 15, 1, 2),
];

/// More of the same wardrobe for the crowds in the fights.
const OUTROS: [Heroi; 6] = [
    h(488, 511, 600, (4, 4, 12), 3, 2, 0),
    h(487, 513, 613, (3, 4, 12), 2, 0, 1),
    h(486, 509, 607, (2, 3, 5), 1, 3, 2),
    h(489, 502, 604, (3, 4, 12), 5, 1, 0),
    h(483, 510, 601, (4, 4, 12), 0, 2, 1),
    h(487, 512, 611, (2, 3, 5), 1, 0, 2),
];

/// Gear aura bits (`shared::auras`): the same piece in every equipment slot.
pub fn bits_de_aura(grau: u8, tier: u8, refino: u8) -> u64 {
    if grau < 2 {
        return 0;
    }
    let patamar: u64 = match refino {
        0..=4 => 0,
        5..=6 => 1,
        7..=9 => 2,
        _ => 3,
    };
    let byte = grau as u64 | ((tier.clamp(1, 4) as u64 - 1) << 3) | (patamar << 5);
    (0..shared::auras::MONTARIA).fold(0, |b, s| b | (byte << (s * 8)))
}

impl Heroi {
    /// The weapon set (`skills::Conjunto` as u8).
    pub fn conjunto(&self) -> u8 {
        shared::aparencia::skin_de_arma(self.arma).map_or(0, |s| s.conjunto)
    }

    pub fn aparencia(&self, cor_cabelo: u8) -> shared::aparencia::Aparencia {
        let mut a = shared::aparencia::Aparencia {
            rosto: self.rosto % shared::aparencia::ROSTOS,
            cabelo: shared::aparencia::cabelo_do_chapeu(self.chapeu).unwrap_or(1),
            cor_cabelo: cor_cabelo % shared::aparencia::CORES_DE_CABELO.len() as u8,
            pele: self.pele % shared::aparencia::TONS_DE_PELE.len() as u8,
            roupa: self.roupa,
            ..Default::default()
        };
        if let Some(s) = shared::aparencia::skin_de_arma(self.arma) {
            a.armas[s.conjunto as usize] = (self.arma - shared::aparencia::ARMA_SKIN_BASE + 1) as u8;
        }
        a
    }

    pub fn meta(&self, id: u32, k: u32) -> EntityMeta {
        let a = self.aparencia(k as u8 * 3 + 1);
        let mut m = meta(id, EntityTag::Player, 0, 1000);
        m.aparencia = a.empacota();
        m.skins = a.empacota_skins();
        m.auras = bits_de_aura(self.aura.0, self.aura.1, self.aura.2);
        m.renascimentos = self.renascimentos;
        m
    }
}

pub fn meta(id: u32, tag: EntityTag, kind: u16, hp: u32) -> EntityMeta {
    EntityMeta {
        skins: 0,
        cla: String::new(),
        pk: Default::default(),
        auras: 0,
        id: EntityId(id),
        tag,
        name: None,
        hp_max: hp,
        faction: None,
        kind,
        nivel: 80,
        desafio: None,
        aparencia: 0,
        poder: 0,
        renascimentos: 0,
    }
}

// ── a tiny deterministic random ──────────────────────────────────────────

struct Sorte(u64);

impl Sorte {
    fn u(&mut self) -> f32 {
        self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        ((self.0 >> 40) as f32) / (1u64 << 24) as f32
    }
    fn entre(&mut self, a: f32, b: f32) -> f32 {
        a + (b - a) * self.u()
    }
    fn dir(&mut self) -> Vec2 {
        let a = self.u() * std::f32::consts::TAU;
        vec2(a.cos(), a.sin())
    }
}

// ── the cast ─────────────────────────────────────────────────────────────

/// Where a fight happens: a clearing, the mob kinds that walk in, and how
/// many are kept alive.
#[derive(Clone, Debug)]
pub struct Grupo {
    pub centro: Vec2,
    pub raio: f32,
    kinds: Vec<u16>,
    vivos: usize,
    proxima_chegada: f32,
}

#[derive(Clone, Debug)]
struct Ator {
    id: u32,
    jogador: bool,
    chefe: bool,
    pos: Vec2,
    vel: Vec2,
    /// Where it looks (rumo), if not where it walks.
    olha: Option<Vec2>,
    hp: i32,
    hp_max: i32,
    grupo: usize,
    alvo: Option<u32>,
    conjunto: u8,
    /// Players: their angle round the target, so a crowd spreads round it.
    lado: f32,
    combo: u8,
    proximo_golpe: f32,
    /// The gesture byte held until `gesto_ate` (acao::SEGURA_S).
    gesto: (u8, u8),
    gesto_ate: f32,
    proxima_skill: f32,
    proxima_skill_idx: usize,
    preso_ate: f32,
    morto: Option<f32>,
    passeio: Vec2,
    proximo_passeio: f32,
    /// Leaping: (from, to, start, duration).
    salto: Option<(Vec2, Vec2, f32, f32)>,
    /// Mob lunge: (direction, start).
    bote: Option<(Vec2, f32)>,
    /// Ranged players: when the last kite step ends.
    recua_ate: f32,
    recua_dir: Vec2,
    tier: u8,
    /// Mobs: how fast it walks; how close it bites.
    passo: f32,
    alcance: f32,
}

enum Pendente {
    Golpe { de: u32, alvo: u32, dano: i32, critico: bool },
    Mordida { de: u32, alvo: u32 },
    Skill { dono: u32, skill: usize, alvo: u32, ponto: Vec2, de: Vec2 },
    ChefeBate { carga: u32 },
}

/// What a frame did to the bodies, applied after `World::apply`.
enum Ordem {
    Skill { id: u32, skill: usize, mira: Vec2, salto: bool },
    Mordida { id: u32, alvo: u32, impacto: f32, mira: Vec2 },
    Ferido { id: u32, de: Vec2 },
    Numero { alvo: u32, dano: i32, critico: bool, eu: bool },
    Carga { id: u32, forma: FormaDoChefe, centro: Vec2, dir: Vec2, carga_s: f32 },
    Bateu { id: u32 },
}

pub struct Luta {
    atores: Vec<Ator>,
    pub grupos: Vec<Grupo>,
    catalogo: Vec<Skill>,
    pub habilidades: crate::habilidades::Habilidades,
    pub telegrafos: crate::telegrafico::Telegrafos,
    pendentes: Vec<(f32, Pendente)>,
    ordens: Vec<Ordem>,
    novas: Vec<EntityMeta>,
    removidas: Vec<EntityId>,
    proximo_id: u32,
    sorte: Sorte,
    relogio: f32,
    /// The boss's next slam, and the one charging: (id, shape, centre, dir, hits at).
    proximo_tapa: f32,
    tapa: Option<(u32, FormaDoChefe, Vec2, Vec2, f32)>,
    tapas: u32,
    mortes: u32,
    /// Free ground (no tree, rock, wall or water) for a body at `p`.
    livre: Box<dyn Fn(Vec2) -> bool>,
}

pub const EU: u32 = 1;
pub const CHEFE: u32 = 99;

/// Kinds that are too big for a small fight (bosses' bodies).
fn passo_do_mob(kind: u16) -> f32 {
    2.6 + (kind % 5) as f32 * 0.25
}

impl Luta {
    /// `pontos`: the fight spots, `jogadores` per spot, `mobs` kept alive per
    /// spot, `kinds` cycled per spot. With `chefe`, one spot round the boss.
    #[allow(clippy::too_many_arguments)]
    pub fn nova(
        pontos: &[Vec2],
        jogadores: &[usize],
        mobs: &[usize],
        kinds: &[u16],
        chefe: Option<u16>,
        semente: u64,
        livre: Box<dyn Fn(Vec2) -> bool>,
    ) -> Self {
        let catalogo = shared::skills::playtest();
        let mut habilidades = crate::habilidades::Habilidades::default();
        habilidades.catalogo = catalogo.clone();
        let mut s = Self {
            atores: Vec::new(),
            grupos: Vec::new(),
            catalogo,
            habilidades,
            telegrafos: Default::default(),
            pendentes: Vec::new(),
            ordens: Vec::new(),
            novas: Vec::new(),
            removidas: Vec::new(),
            proximo_id: 100,
            sorte: Sorte(semente.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1),
            relogio: 0.0,
            proximo_tapa: 2.2,
            tapa: None,
            tapas: 0,
            mortes: 0,
            livre,
        };
        let mut k_jogador = 0u32;
        for (g, &c) in pontos.iter().enumerate() {
            let n_kinds = kinds.len().max(1);
            let ks: Vec<u16> = (0..3).map(|i| kinds[(g * 2 + i) % n_kinds]).collect();
            let vivos = mobs.get(g).copied().unwrap_or(4);
            s.grupos.push(Grupo { centro: c, raio: if chefe.is_some() { 9.0 } else { 7.0 }, kinds: ks, vivos, proxima_chegada: 0.0 });
            let nj = jogadores.get(g).copied().unwrap_or(3);
            for j in 0..nj {
                let id = if k_jogador == 0 { EU } else { 10 + k_jogador - 1 };
                let heroi = OUTROS[(k_jogador as usize + 2) % OUTROS.len()];
                let heroi = if k_jogador as usize % 2 == 0 { HEROIS[(k_jogador as usize / 2) % HEROIS.len()] } else { heroi };
                let mut m = heroi.meta(id, k_jogador);
                // One black hole on screen is a show; five are a mess.
                if m.renascimentos >= 15 && k_jogador != 0 {
                    m.renascimentos = 10;
                }
                let a = j as f32 / nj as f32 * std::f32::consts::TAU + g as f32;
                let pos = s.solta(c + vec2(a.cos(), a.sin()) * 2.5);
                let tier = [10u8, 8, 6, 9][k_jogador as usize % 4];
                s.novas.push(m);
                let lado = a + s.sorte.entre(-0.4, 0.4);
                let proxima_skill = s.sorte.entre(0.8, 3.5);
                s.atores.push(Ator {
                    id,
                    jogador: true,
                    chefe: false,
                    pos,
                    vel: Vec2::ZERO,
                    olha: None,
                    hp: 1000,
                    hp_max: 1000,
                    grupo: g,
                    alvo: None,
                    conjunto: heroi.conjunto(),
                    lado,
                    combo: 0,
                    proximo_golpe: 0.0,
                    gesto: (acao::NADA, 0),
                    gesto_ate: 0.0,
                    proxima_skill,
                    proxima_skill_idx: k_jogador as usize,
                    preso_ate: 0.0,
                    morto: None,
                    passeio: pos,
                    proximo_passeio: 0.0,
                    salto: None,
                    bote: None,
                    recua_ate: 0.0,
                    recua_dir: Vec2::ZERO,
                    tier,
                    passo: shared::PLAYER_SPEED,
                    alcance: 0.0,
                });
                k_jogador += 1;
            }
            // The first mobs are already there, at the clearing's edge, so
            // the shot opens on a fight and not on a walk.
            if chefe.is_none() {
                for k in 0..vivos {
                    let p = c + s.sorte.dir() * s.sorte.entre(4.5, 7.5);
                    let kind = s.grupos[g].kinds[k % 3];
                    s.chega_mob(g, kind, p);
                }
            }
        }
        if let Some(kind) = chefe {
            let c = pontos[0];
            let mut m = meta(CHEFE, EntityTag::Enemy, kind, 100_000);
            m.nivel = 90;
            s.novas.push(m);
            s.atores.push(Ator {
                id: CHEFE,
                jogador: false,
                chefe: true,
                pos: c,
                vel: Vec2::ZERO,
                olha: None,
                hp: 92_000,
                hp_max: 100_000,
                grupo: 0,
                alvo: None,
                conjunto: 0,
                lado: 0.0,
                combo: 0,
                proximo_golpe: 0.0,
                gesto: (acao::NADA, 0),
                gesto_ate: 0.0,
                proxima_skill: 0.0,
                proxima_skill_idx: 0,
                preso_ate: 0.0,
                morto: None,
                passeio: c,
                proximo_passeio: 0.0,
                salto: None,
                bote: None,
                recua_ate: 0.0,
                recua_dir: Vec2::ZERO,
                tier: 0,
                passo: 0.0,
                alcance: 4.2,
            });
            s.grupos[0].vivos = 0;
        }
        s
    }

    /// The nearest free ground to `p` (a short spiral).
    fn solta(&self, p: Vec2) -> Vec2 {
        if (self.livre)(p) {
            return p;
        }
        for anel in 1..8 {
            for k in 0..8 {
                let a = k as f32 / 8.0 * std::f32::consts::TAU;
                let q = p + vec2(a.cos(), a.sin()) * anel as f32 * 0.6;
                if (self.livre)(q) {
                    return q;
                }
            }
        }
        p
    }

    fn chega_mob(&mut self, g: usize, kind: u16, p: Vec2) {
        let id = self.proximo_id;
        self.proximo_id += 1;
        let p = self.solta(p);
        self.novas.push(meta(id, EntityTag::Enemy, kind, 30_000));
        let passo = passo_do_mob(kind) + self.sorte.entre(-0.2, 0.3);
        let proximo_golpe = self.relogio + self.sorte.entre(0.3, 1.2);
        self.atores.push(Ator {
            id,
            jogador: false,
            chefe: false,
            pos: p,
            vel: Vec2::ZERO,
            olha: None,
            hp: 30_000,
            hp_max: 30_000,
            grupo: g,
            alvo: None,
            conjunto: 0,
            lado: 0.0,
            combo: 0,
            proximo_golpe,
            gesto: (acao::NADA, 0),
            gesto_ate: 0.0,
            proxima_skill: 0.0,
            proxima_skill_idx: 0,
            preso_ate: 0.0,
            morto: None,
            passeio: p,
            proximo_passeio: 0.0,
            salto: None,
            bote: None,
            recua_ate: 0.0,
            recua_dir: Vec2::ZERO,
            tier: 0,
            passo,
            alcance: 1.35,
        });
    }

    fn idx(&self, id: u32) -> Option<usize> {
        self.atores.iter().position(|a| a.id == id)
    }

    fn vivo(&self, id: u32) -> Option<&Ator> {
        self.atores.iter().find(|a| a.id == id && a.morto.is_none())
    }

    /// Players' positions now (for the camera).
    pub fn centro_do_grupo(&self, g: usize) -> Vec2 {
        let (s, n) = self
            .atores
            .iter()
            .filter(|a| a.jogador && a.grupo == g)
            .fold((Vec2::ZERO, 0.0), |(s, n), a| (s + a.pos, n + 1.0));
        if n > 0.0 { s / n } else { self.grupos[g].centro }
    }

    /// Steps the fight by `dt` and hands the bodies to `mundo`.
    pub fn passo(&mut self, mundo: &mut World, dt: f32) {
        self.relogio += dt;
        let t = self.relogio;
        self.resolve(t);
        self.chegadas(t);
        let n = self.atores.len();
        for i in 0..n {
            if self.atores[i].morto.is_some() {
                self.atores[i].vel = Vec2::ZERO;
                continue;
            }
            if self.atores[i].chefe {
                self.chefe(i, t);
            } else if self.atores[i].jogador {
                self.jogador(i, t, dt);
            } else {
                self.mob(i, t);
            }
        }
        self.separa();
        // Move, around trees and walls.
        for i in 0..n {
            let a = &self.atores[i];
            if let Some((de, para, t0, dur)) = a.salto {
                let u = ((t - t0) / dur).clamp(0.0, 1.0);
                let p = de.lerp(para, u);
                let a = &mut self.atores[i];
                a.vel = (para - de) / dur;
                a.pos = p;
                if u >= 1.0 {
                    a.salto = None;
                    a.vel = Vec2::ZERO;
                }
                continue;
            }
            let v = a.vel;
            if v.length_squared() < 1e-4 {
                continue;
            }
            let p = a.pos;
            let mut feito = None;
            for giro in [0.0f32, 0.5, -0.5, 1.0, -1.0, 1.5, -1.5] {
                let vv = Vec2::from_angle(giro).rotate(v);
                let q = p + vv * dt;
                if (self.livre)(q) {
                    feito = Some((q, vv));
                    break;
                }
            }
            let a = &mut self.atores[i];
            match feito {
                Some((q, vv)) => {
                    a.pos = q;
                    a.vel = vv;
                }
                None => a.vel = Vec2::ZERO,
            }
        }
        // Old bodies go (out of the shot by then, mostly).
        let velhos: Vec<u32> = self
            .atores
            .iter()
            .filter(|a| !a.jogador && a.morto.is_some_and(|m| t - m > 7.0))
            .map(|a| a.id)
            .collect();
        for id in velhos {
            self.atores.retain(|a| a.id != id);
            self.removidas.push(EntityId(id));
        }
        self.entrega(mundo, t);
    }

    /// Mobs walking in from the edges to keep each clearing busy.
    fn chegadas(&mut self, t: f32) {
        for g in 0..self.grupos.len() {
            let vivos = self.atores.iter().filter(|a| !a.jogador && !a.chefe && a.grupo == g && a.morto.is_none()).count();
            if vivos < self.grupos[g].vivos && t >= self.grupos[g].proxima_chegada {
                let espera = self.sorte.entre(0.6, 1.8);
                self.grupos[g].proxima_chegada = t + espera;
                let c = self.grupos[g].centro;
                let p = c + self.sorte.dir() * (self.grupos[g].raio + self.sorte.entre(6.0, 10.0));
                let k = (t * 7.0) as usize % 3;
                let kind = self.grupos[g].kinds[k];
                self.chega_mob(g, kind, p);
            }
        }
    }

    fn mais_perto(&self, de: Vec2, g: usize, jogador: bool, raio: f32) -> Option<(u32, f32)> {
        self.atores
            .iter()
            .filter(|a| a.jogador == jogador && a.morto.is_none() && (a.grupo == g || a.chefe))
            .map(|a| (a.id, a.pos.distance(de)))
            .filter(|(_, d)| *d < raio)
            .min_by(|a, b| a.1.total_cmp(&b.1))
    }

    fn jogador(&mut self, i: usize, t: f32, dt: f32) {
        let a = self.atores[i].clone();
        if a.gesto_ate <= t {
            self.atores[i].gesto = (acao::NADA, 0);
        }
        if a.salto.is_some() {
            return;
        }
        // Keep a live target in its own clearing.
        let alvo = a.alvo.and_then(|id| self.vivo(id)).map(|x| (x.id, x.pos, x.chefe));
        let alvo = match alvo {
            Some(x) => Some(x),
            None => {
                let c = self.grupos[a.grupo].centro;
                let raio = self.grupos[a.grupo].raio + 7.0;
                let novo = self.mais_perto(a.pos, a.grupo, false, 26.0).filter(|(id, _)| self.vivo(*id).is_some_and(|m| m.chefe || m.pos.distance(c) < raio));
                self.atores[i].alvo = novo.map(|(id, _)| id);
                novo.and_then(|(id, _)| self.vivo(id)).map(|x| (x.id, x.pos, x.chefe))
            }
        };
        let distancia_ = matches!(a.conjunto, 2 | 3);
        if t < a.preso_ate {
            self.atores[i].vel = Vec2::ZERO;
            return;
        }
        let Some((alvo_id, alvo_pos, no_chefe)) = alvo else {
            // Nothing to hit: stroll round the clearing.
            if t >= a.proximo_passeio || a.pos.distance(a.passeio) < 0.6 {
                let c = self.grupos[a.grupo].centro;
                let p = c + self.sorte.dir() * self.sorte.entre(1.0, 4.0);
                self.atores[i].passeio = p;
                self.atores[i].proximo_passeio = t + self.sorte.entre(2.0, 4.0);
            }
            let d = self.atores[i].passeio - a.pos;
            self.atores[i].vel = if d.length() > 0.5 { d.normalize() * 2.6 } else { Vec2::ZERO };
            self.atores[i].olha = None;
            return;
        };
        // The boss's slam charging over me: run out of it.
        if let Some((_, forma, c, dir, _)) = self.tapa {
            let g = |v: Vec2| ::glam::Vec2::new(v.x, v.y);
            if forma.contem(g(c), g(dir), g(a.pos)) || forma.contem(g(c), g(dir), g(a.pos + a.vel * 0.3)) {
                let fora = (a.pos - c).normalize_or(vec2(1.0, 0.0));
                let fora = if let FormaDoChefe::Anel { interno, externo } = forma {
                    if a.pos.distance(c) < (interno + externo) * 0.5 { -fora } else { fora }
                } else {
                    fora
                };
                self.atores[i].vel = fora * shared::PLAYER_SPEED * 1.55;
                self.atores[i].olha = None;
                return;
            }
        }
        let raio_alvo = if no_chefe { 3.4 } else { 0.45 };
        let quero = if distancia_ { 6.0 + (a.id % 3) as f32 } else { 1.05 + raio_alvo };
        let d = alvo_pos.distance(a.pos);
        // Ranged: a mob in my face, step back and keep shooting after.
        if distancia_ && t >= a.recua_ate {
            let perto = self.mais_perto(a.pos, a.grupo, false, 2.6);
            if let Some((mid, _)) = perto.filter(|(id, _)| self.vivo(*id).is_some_and(|m| !m.chefe)) {
                let m = self.vivo(mid).map_or(a.pos, |m| m.pos);
                let fora = (a.pos - m).normalize_or(self.sorte.dir());
                let lado = vec2(-fora.y, fora.x) * self.sorte.entre(-0.6, 0.6);
                let casa = self.grupos[a.grupo].centro - a.pos;
                let volta = if casa.length() > self.grupos[a.grupo].raio { casa.normalize() * 1.3 } else { Vec2::ZERO };
                self.atores[i].recua_dir = (fora + lado + volta).normalize_or(fora);
                self.atores[i].recua_ate = t + self.sorte.entre(0.55, 0.8);
            }
        }
        if t < self.atores[i].recua_ate {
            self.atores[i].vel = self.atores[i].recua_dir * shared::PLAYER_SPEED * 1.3;
            self.atores[i].olha = None;
            return;
        }
        // Where to stand: on my side of the target, drifting a little.
        let lado = a.lado + (t * 0.21 + a.id as f32).sin() * 0.5;
        let vaga = alvo_pos + vec2(lado.cos(), lado.sin()) * quero;
        // Never far from the clearing: the fight stays where the camera is.
        let (c, r) = (self.grupos[a.grupo].centro, self.grupos[a.grupo].raio + 4.0);
        let vaga = if vaga.distance(c) > r { c + (vaga - c).normalize() * r } else { vaga };
        let ate_vaga = vaga - a.pos;
        let no_alcance = d <= quero + 0.6;
        // Skills: when ready and the target is in reach.
        let skills: Vec<usize> =
            (0..self.catalogo.len()).filter(|k| self.catalogo[*k].conjunto as u8 == a.conjunto).collect();
        if t >= a.proxima_skill && !skills.is_empty() {
            let k = skills[a.proxima_skill_idx % skills.len()];
            let s = &self.catalogo[k];
            let alcance = if s.forma == Forma::EmSi { 99.0 } else { s.alcance.max(2.5) + raio_alvo };
            let salto = s.e_salto();
            let vale = if salto { d > 3.0 && d < s.alcance.max(6.0) + raio_alvo } else { d <= alcance };
            if vale {
                self.lanca(i, k, alvo_id, alvo_pos, t);
                return;
            }
            if salto || s.id == 3 {
                // Not the moment for this one: try the next.
                self.atores[i].proxima_skill_idx += 1;
            }
        }
        if !no_alcance || ate_vaga.length() > 1.2 {
            let corre = ate_vaga.length() > 4.0;
            let v = if corre { shared::PLAYER_SPEED * 1.4 } else { shared::PLAYER_SPEED * 0.8 };
            self.atores[i].vel = ate_vaga.normalize_or_zero() * v.min(ate_vaga.length() / dt.max(1e-3));
            self.atores[i].olha = if no_alcance { Some(alvo_pos - a.pos) } else { None };
        } else {
            self.atores[i].vel = Vec2::ZERO;
            self.atores[i].olha = Some(alvo_pos - a.pos);
        }
        // Basic combo while in reach (pistols and ring shoot on the move).
        if no_alcance && t >= a.proximo_golpe && (distancia_ || self.atores[i].vel.length() < 2.0) {
            let passo = a.combo % 3;
            let ator = &mut self.atores[i];
            ator.gesto = (acao::GOLPE, passo);
            ator.gesto_ate = t + acao::SEGURA_S;
            ator.combo = passo + 1;
            ator.olha = Some(alvo_pos - a.pos);
            ator.proximo_golpe = t + if passo == 2 { 0.78 } else { 0.5 } + (a.id % 3) as f32 * 0.03;
            let critico = self.sorte.u() < 0.22;
            let base = if no_chefe { 4200.0 } else { 1300.0 };
            let dano = (base * self.sorte.entre(0.8, 1.35) * if critico { 2.1 } else { 1.0 }) as i32;
            let voo = if distancia_ { d / 28.0 } else { 0.0 };
            self.pendentes.push((t + shared::PLAYER_ATTACK_IMPACT_S + voo, Pendente::Golpe { de: a.id, alvo: alvo_id, dano, critico }));
        }
    }

    fn lanca(&mut self, i: usize, k: usize, alvo: u32, alvo_pos: Vec2, t: f32) {
        let s = self.catalogo[k].clone();
        let a = &mut self.atores[i];
        a.proxima_skill_idx += 1;
        a.proxima_skill = t + 3.2 + (a.id % 4) as f32 * 0.6;
        let impacto = s.impacto_em();
        a.preso_ate = t + impacto + shared::skills::RECUPERACAO_S;
        a.gesto = (acao::SKILL, s.ordem.saturating_sub(1));
        a.gesto_ate = t + acao::SEGURA_S;
        a.vel = Vec2::ZERO;
        a.olha = Some(alvo_pos - a.pos);
        a.combo = 0;
        let de = a.pos;
        let salto = s.e_salto();
        let ponto = if s.forma == Forma::EmSi { de } else { alvo_pos };
        if salto {
            let para = alvo_pos + (de - alvo_pos).normalize_or(vec2(1.0, 0.0)) * 1.3;
            a.salto = Some((de, para, t, impacto));
        }
        let (id, tier) = (a.id, a.tier);
        self.habilidades.efeito(s.id, EntityId(id), de, ponto, Some(EntityId(alvo)).filter(|_| s.forma != Forma::EmSi), false, tier);
        self.ordens.push(Ordem::Skill { id, skill: k, mira: alvo_pos, salto });
        self.pendentes.push((t + impacto, Pendente::Skill { dono: id, skill: k, alvo, ponto, de }));
    }

    fn mob(&mut self, i: usize, t: f32) {
        let a = self.atores[i].clone();
        let alvo = a.alvo.and_then(|id| self.vivo(id)).map(|x| (x.id, x.pos));
        let alvo = alvo.or_else(|| {
            let n = self.mais_perto(a.pos, a.grupo, true, 40.0);
            self.atores[i].alvo = n.map(|(id, _)| id);
            n.and_then(|(id, _)| self.vivo(id)).map(|x| (x.id, x.pos))
        });
        let Some((alvo_id, alvo_pos)) = alvo else {
            self.atores[i].vel = Vec2::ZERO;
            return;
        };
        // Retarget now and then: the nearest player, not the first one.
        if self.sorte.u() < 0.004 {
            self.atores[i].alvo = None;
        }
        let d = alvo_pos.distance(a.pos);
        let ator = &mut self.atores[i];
        if let Some((_, t0)) = ator.bote {
            if t - t0 > 0.75 {
                ator.bote = None;
            }
        }
        if d > a.alcance + 0.25 {
            ator.vel = (alvo_pos - a.pos).normalize_or_zero() * a.passo * if d > 6.0 { 1.35 } else { 1.0 };
            ator.olha = None;
        } else {
            ator.vel = Vec2::ZERO;
            ator.olha = Some(alvo_pos - a.pos);
            if t >= a.proximo_golpe {
                let espera = 1.3 + (a.id % 5) as f32 * 0.15;
                ator.proximo_golpe = t + espera;
                let dir = (alvo_pos - a.pos).normalize_or_zero();
                ator.bote = Some((dir, t));
                self.ordens.push(Ordem::Mordida { id: a.id, alvo: alvo_id, impacto: 0.42, mira: alvo_pos });
                self.pendentes.push((t + 0.42, Pendente::Mordida { de: a.id, alvo: alvo_id }));
            }
        }
    }

    fn chefe(&mut self, i: usize, t: f32) {
        let a = self.atores[i].clone();
        // Face whoever it is about to hit, turning slowly between slams.
        if a.alvo.and_then(|id| self.vivo(id)).is_none() || self.sorte.u() < 0.01 {
            let ids: Vec<u32> = self.atores.iter().filter(|x| x.jogador).map(|x| x.id).collect();
            if !ids.is_empty() {
                let k = (self.sorte.u() * ids.len() as f32) as usize % ids.len();
                self.atores[i].alvo = Some(ids[k]);
            }
        }
        let alvo = self.atores[i].alvo.and_then(|id| self.vivo(id)).map_or(a.pos + vec2(0.0, 1.0), |x| x.pos);
        self.atores[i].vel = Vec2::ZERO;
        if self.tapa.is_none() {
            self.atores[i].olha = Some(alvo - a.pos);
        }
        if let Some((_, _, _, _, bate)) = self.tapa {
            if t >= bate {
                self.tapa = None;
            }
        }
        if self.tapa.is_none() && t >= self.proximo_tapa {
            let dir = (alvo - a.pos).normalize_or(vec2(0.0, 1.0));
            let forma = match self.tapas % 3 {
                0 => FormaDoChefe::Circulo { raio: 6.5 },
                1 => FormaDoChefe::Cone { raio: 10.0, abertura: 0.75 },
                _ => FormaDoChefe::Anel { interno: 4.0, externo: 10.0 },
            };
            let carga_s = 1.25;
            self.tapas += 1;
            let id = self.tapas;
            self.tapa = Some((id, forma, a.pos, dir, t + carga_s));
            self.proximo_tapa = t + carga_s + 2.4;
            self.telegrafos.comeca(id, EntityId(a.id), forma, [a.pos.x, a.pos.y], [dir.x, dir.y], carga_s, get_time());
            self.ordens.push(Ordem::Carga { id: a.id, forma, centro: a.pos, dir, carga_s });
            self.pendentes.push((t + carga_s, Pendente::ChefeBate { carga: id }));
        }
    }

    /// Bodies too close push apart (no one stands inside another).
    fn separa(&mut self) {
        let n = self.atores.len();
        for i in 0..n {
            if self.atores[i].morto.is_some() || self.atores[i].salto.is_some() {
                continue;
            }
            let mut empurra = Vec2::ZERO;
            for j in 0..n {
                if i == j || self.atores[j].morto.is_some() {
                    continue;
                }
                let r = if self.atores[j].chefe { 3.2 } else { 1.05 };
                let d = self.atores[i].pos - self.atores[j].pos;
                let l = d.length();
                if l < r && l > 1e-4 {
                    empurra += d / l * (r - l);
                }
            }
            if empurra.length_squared() > 1e-6 && !self.atores[i].chefe {
                self.atores[i].vel += empurra * 2.5;
            }
        }
    }

    /// Hits land: numbers, flinches, deaths.
    fn resolve(&mut self, t: f32) {
        let (agora, depois): (Vec<_>, Vec<_>) = std::mem::take(&mut self.pendentes).into_iter().partition(|(q, _)| *q <= t);
        self.pendentes = depois;
        for (_, p) in agora {
            match p {
                Pendente::Golpe { de, alvo, dano, critico } => self.fere(de, alvo, dano, critico),
                Pendente::Mordida { de, alvo } => {
                    if self.vivo(de).is_none() {
                        continue;
                    }
                    let dano = (self.sorte.entre(180.0, 520.0)) as i32;
                    let pos_de = self.vivo(de).map_or(Vec2::ZERO, |a| a.pos);
                    if let Some(a) = self.vivo(alvo) {
                        if a.pos.distance(pos_de) < 2.6 {
                            self.ordens.push(Ordem::Ferido { id: alvo, de: pos_de });
                            self.ordens.push(Ordem::Numero { alvo, dano, critico: false, eu: true });
                        }
                    }
                }
                Pendente::Skill { dono, skill, alvo, ponto, de } => {
                    let s = self.catalogo[skill].clone();
                    let tier = self.vivo(dono).map_or(8, |a| a.tier);
                    let agora_de = self.vivo(dono).map_or(de, |a| a.pos);
                    let ponto = if s.forma == Forma::Projetil { self.vivo(alvo).map_or(ponto, |a| a.pos) } else { ponto };
                    self.habilidades.efeito(s.id, EntityId(dono), agora_de, ponto, None, true, tier);
                    if s.dano <= 0 {
                        continue;
                    }
                    let frente = (ponto - agora_de).normalize_or(vec2(0.0, 1.0));
                    let raio = shared::skills::ajustada_ao_tier(s.clone(), tier).raio.max(2.0);
                    let alvos: Vec<u32> = self
                        .atores
                        .iter()
                        .filter(|a| !a.jogador && a.morto.is_none())
                        .filter(|a| match s.forma {
                            Forma::Projetil => a.id == alvo,
                            Forma::Circulo => a.pos.distance(ponto) <= raio + if a.chefe { 3.0 } else { 0.0 },
                            Forma::Cone | Forma::Linha => {
                                let v = a.pos - agora_de;
                                v.length() <= s.alcance.max(4.0) + if a.chefe { 3.0 } else { 0.0 } && v.normalize_or_zero().dot(frente) > 0.45
                            }
                            Forma::EmSi => false,
                        })
                        .map(|a| a.id)
                        .take(5)
                        .collect();
                    for (n, id) in alvos.into_iter().enumerate() {
                        let critico = self.sorte.u() < 0.4;
                        let base = if id == CHEFE { 11_000.0 } else { 5200.0 };
                        let dano = (base * self.sorte.entre(0.85, 1.3) * if critico { 1.9 } else { 1.0 }) as i32;
                        // A little stagger so stacked numbers read.
                        self.pendentes.push((t + n as f32 * 0.05, Pendente::Golpe { de: dono, alvo: id, dano, critico }));
                    }
                }
                Pendente::ChefeBate { carga } => {
                    self.telegrafos.termina(carga, true, get_time());
                    self.ordens.push(Ordem::Bateu { id: CHEFE });
                    let Some((_, forma, c, dir)) = self.tapa.map(|(i, f, c, d, _)| (i, f, c, d)) else { continue };
                    let g = |v: Vec2| ::glam::Vec2::new(v.x, v.y);
                    let feridos: Vec<u32> =
                        self.atores.iter().filter(|a| a.jogador && forma.contem(g(c), g(dir), g(a.pos))).map(|a| a.id).collect();
                    for id in feridos {
                        let dano = self.sorte.entre(1800.0, 3200.0) as i32;
                        self.ordens.push(Ordem::Ferido { id, de: c });
                        self.ordens.push(Ordem::Numero { alvo: id, dano, critico: false, eu: true });
                    }
                    self.tapa = None;
                }
            }
        }
    }

    fn fere(&mut self, de: u32, alvo: u32, dano: i32, critico: bool) {
        let pos_de = self.atores.iter().find(|a| a.id == de).map_or(Vec2::ZERO, |a| a.pos);
        let Some(i) = self.idx(alvo).filter(|i| self.atores[*i].morto.is_none()) else { return };
        let relogio = self.relogio;
        let a = &mut self.atores[i];
        let mut ferido = true;
        if a.chefe {
            // Never dies on camera; a big flinch only on the heavy blows.
            a.hp = (a.hp - dano / 20).max(a.hp_max / 3);
            ferido = critico;
            // Seven players on one body pile their numbers into a smear:
            // only the crits and the skills get one.
            if !critico && dano < 8000 {
                if ferido {
                    self.ordens.push(Ordem::Ferido { id: alvo, de: pos_de });
                }
                return;
            }
        } else {
            a.hp -= dano;
            if a.hp <= 0 {
                a.hp = 0;
                a.morto = Some(relogio);
                a.vel = Vec2::ZERO;
                self.mortes += 1;
            }
        }
        if ferido {
            self.ordens.push(Ordem::Ferido { id: alvo, de: pos_de });
        }
        self.ordens.push(Ordem::Numero { alvo, dano, critico, eu: false });
    }

    /// The frame's states into the world, then what the server would have
    /// sent as events (skill casts, bites, hits, slams).
    fn entrega(&mut self, mundo: &mut World, t: f32) {
        let g = |p: Vec2| ::glam::Vec2::new(p.x, p.y);
        let estados: Vec<EntityState> = self
            .atores
            .iter()
            .map(|a| {
                let mut pos = a.pos;
                if let Some((dir, t0)) = a.bote {
                    // The lunge: in and back, under a second.
                    let u = ((t - t0) / 0.7).clamp(0.0, 1.0);
                    pos += dir * (u * std::f32::consts::PI).sin().max(0.0) * 0.55;
                }
                // Nobody is SELF: the local player's yaw ignores `rumo`.
                let mut flags = 0;
                if a.chefe {
                    flags |= ent_flags::BOSS;
                }
                let mut s = EntityState::quantize(EntityId(a.id), g(pos), g(a.vel), if a.morto.is_some() { 0 } else { a.hp.max(1) }, flags);
                s.acao = if a.jogador { acao::monta(a.conjunto, true, a.gesto.0, a.gesto.1) } else { 0 };
                let olha = a.olha.filter(|d| d.length_squared() > 1e-4).or((a.vel.length_squared() > 0.05).then_some(a.vel));
                s.rumo = olha.map_or(0, |d| shared::rumo_de_dir(g(d)));
                s
            })
            .collect();
        mundo.apply(std::mem::take(&mut self.novas), estados, &std::mem::take(&mut self.removidas));
        for o in std::mem::take(&mut self.ordens) {
            match o {
                Ordem::Skill { id, skill, mira, salto } => {
                    let s = &self.catalogo[skill];
                    if let Some(e) = mundo.ents.get_mut(&EntityId(id)) {
                        e.skill = Some((s.id, 0.0, s.impacto_em()));
                        e.combo = None;
                        e.combo_ant = None;
                        e.sacada = 1.0;
                        if salto {
                            e.vel_y = shared::gravidade() * s.impacto_em() * 0.5;
                            e.voando = true;
                        }
                        if s.forma != Forma::EmSi {
                            e.mira = Some((mira, s.impacto_em() + 0.3));
                        }
                    }
                }
                Ordem::Mordida { id, alvo, impacto, mira } => {
                    if let Some(e) = mundo.ents.get_mut(&EntityId(id)) {
                        e.golpe = 0.0;
                        e.ataque_mob = Some((Some(EntityId(alvo)), 0.0, impacto));
                        e.mira = Some((mira, impacto + 0.3));
                    }
                }
                Ordem::Ferido { id, de } => {
                    if let Some(e) = mundo.ents.get_mut(&EntityId(id)) {
                        e.ferido = Some(0.0);
                        e.golpe_de = (de - e.render_pos).normalize_or_zero();
                    }
                }
                Ordem::Numero { alvo, dano, critico, eu } => {
                    let k = mundo.efeitos.len() as u32 * 7 + alvo * 13 + (t * 100.0) as u32;
                    mundo.efeitos.push(crate::world::Efeito {
                        alvo: EntityId(alvo),
                        dano,
                        critico,
                        eu,
                        t: 0.0,
                        semente: (k.wrapping_mul(2_654_435_761) % 1000) as f32 / 1000.0,
                    });
                }
                Ordem::Carga { id, forma, centro, dir, carga_s } => {
                    if let Some(e) = mundo.ents.get_mut(&EntityId(id)) {
                        let p = e.render_pos;
                        e.carga_chefe = Some(crate::chefe_anim::Carga::nova(&forma, centro, dir, p, carga_s, get_time()));
                    }
                }
                Ordem::Bateu { id } => {
                    if let Some(c) = mundo.ents.get_mut(&EntityId(id)).and_then(|e| e.carga_chefe.as_mut()) {
                        c.bateu(get_time());
                    }
                }
            }
        }
        self.habilidades.acompanhar_alvos(mundo);
    }
}

// ── where to fight ───────────────────────────────────────────────────────

/// `n` clearings round `foco`, between `perto` and `longe` from it, at least
/// `entre` apart: open, dry, fairly flat ground with room to move.
pub fn clareiras(
    foco: Vec2,
    n: usize,
    perto: f32,
    longe: f32,
    entre: f32,
    livre: &dyn Fn(Vec2) -> bool,
    altura: &dyn Fn(f32, f32) -> f32,
) -> Vec<Vec2> {
    let mut boas: Vec<(f32, Vec2)> = Vec::new();
    let passo = 3.0;
    let k = (longe / passo).ceil() as i32;
    for iz in -k..=k {
        for ix in -k..=k {
            let p = foco + vec2(ix as f32, iz as f32) * passo;
            let d = p.distance(foco);
            if d < perto || d > longe {
                continue;
            }
            let h0 = altura(p.x, p.y);
            let (mut livres, mut total, mut desnivel) = (0, 0, 0.0f32);
            for anel in [2.0f32, 4.0, 6.5] {
                for j in 0..8 {
                    let a = j as f32 / 8.0 * std::f32::consts::TAU + anel;
                    let q = p + vec2(a.cos(), a.sin()) * anel;
                    total += 1;
                    if livre(q) {
                        livres += 1;
                    }
                    desnivel = desnivel.max((altura(q.x, q.y) - h0).abs());
                }
            }
            if livre(p) && livres as f32 >= total as f32 * 0.85 && desnivel < 1.6 {
                boas.push((livres as f32 / total as f32 - desnivel * 0.1, p));
            }
        }
    }
    // Farthest-point picking, from the best spot nearest the middle of the
    // band: spread over the map, never piled on one place.
    let mut escolhidas: Vec<Vec2> = Vec::new();
    let meio = (perto + longe) * 0.5;
    if let Some(&(_, p)) = boas.iter().min_by(|a, b| {
        let ca = (a.1.distance(foco) - meio).abs() - a.0 * 4.0;
        let cb = (b.1.distance(foco) - meio).abs() - b.0 * 4.0;
        ca.total_cmp(&cb)
    }) {
        escolhidas.push(p);
    }
    while escolhidas.len() < n {
        let prox = boas
            .iter()
            .map(|(q, p)| (escolhidas.iter().map(|e| e.distance(*p)).fold(f32::MAX, f32::min), *q, *p))
            .filter(|(d, _, _)| *d >= entre)
            .min_by(|a, b| {
                // The nearest spot that is still far enough: groups spread
                // but stay within one sweep of the camera.
                (a.0 - a.1 * 2.0).total_cmp(&(b.0 - b.1 * 2.0))
            });
        match prox {
            Some((_, _, p)) => escolhidas.push(p),
            None => break,
        }
    }
    escolhidas
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn a_vitrine_veste_skins_pagas_e_as_auras_pedidas() {
        let mut renasc: Vec<u8> = HEROIS.iter().map(|h| h.renascimentos).collect();
        renasc.sort_unstable();
        assert_eq!(renasc, vec![1, 2, 3, 5, 10, 15]);
        for h in HEROIS {
            assert!(shared::aparencia::preco_da_skin(h.roupa).is_some(), "{} is a cash outfit", h.roupa);
            assert!(shared::aparencia::skin_de_arma(h.arma).is_some());
            let a = h.aparencia(0);
            assert_eq!(a.saneada(), a, "the look survives the server's check");
            let bits = bits_de_aura(h.aura.0, h.aura.1, h.aura.2);
            assert_eq!(shared::auras::peca(bits, 2).map(|p| (p.0, p.1)), Some((h.aura.0, h.aura.1)));
        }
        assert_eq!(shared::auras::peca(bits_de_aura(3, 4, 12), 0), Some((3, 4, 10)));
        assert_eq!(shared::auras::peca(bits_de_aura(2, 3, 5), 0), Some((2, 3, 5)));
    }

    #[test]
    fn a_luta_golpeia_morde_mata_e_traz_mobs_novos() {
        let mut luta = Luta::nova(
            &[vec2(0.0, 0.0), vec2(40.0, 0.0)],
            &[3, 2],
            &[4, 3],
            &[0, 1, 3],
            None,
            7,
            Box::new(|_| true),
        );
        let mut mundo = World::default();
        let (mut golpes, mut mordidas, mut skills, mut feridos) = (0, 0, 0, 0);
        crate::relogio::fixa(Some((10.0, 1.0 / 60.0)));
        for q in 0..(60 * 20) {
            crate::relogio::fixa(Some((10.0 + q as f64 / 60.0, 1.0 / 60.0)));
            luta.passo(&mut mundo, 1.0 / 60.0);
            for e in mundo.ents.values() {
                golpes += e.combo.is_some_and(|(_, t)| t == 0.0) as u32;
                mordidas += e.ataque_mob.is_some_and(|(_, t, _)| t == 0.0) as u32;
                skills += e.skill.is_some_and(|(_, t, _)| t == 0.0) as u32;
                feridos += e.ferido.is_some_and(|t| t == 0.0) as u32;
            }
            mundo.tick(1.0 / 60.0, &|_, _, _| 0.0);
        }
        assert!(golpes > 40 && mordidas > 20 && skills > 5 && feridos > 40, "{golpes} {mordidas} {skills} {feridos}");
        assert!(luta.mortes >= 4, "mobs die: {}", luta.mortes);
        assert!(luta.proximo_id > 107, "and new ones walk in");
        // Players move with walking speed (the walk cycle has something to show).
        assert!(mundo.ents.values().filter(|e| e.meta.tag == EntityTag::Player).all(|e| e.fase > 1.0));
        // The two groups stay apart.
        // The two groups stay apart, each player near its own clearing.
        assert!(luta.centro_do_grupo(0).distance(luta.centro_do_grupo(1)) > 25.0);
        for a in luta.atores.iter().filter(|a| a.jogador) {
            let c = luta.grupos[a.grupo].centro;
            assert!(a.pos.distance(c) < luta.grupos[a.grupo].raio + 8.0, "player {} wandered to {:?}", a.id, a.pos - c);
        }
        crate::relogio::fixa(None);
    }

    #[test]
    fn clareiras_espalhadas() {
        let c = clareiras(Vec2::ZERO, 5, 10.0, 60.0, 18.0, &|_| true, &|_, _| 0.0);
        assert_eq!(c.len(), 5);
        for (i, a) in c.iter().enumerate() {
            for b in &c[i + 1..] {
                assert!(a.distance(*b) >= 18.0 - 1e-3);
            }
        }
    }
}
