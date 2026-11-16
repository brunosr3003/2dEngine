//! THE OPEN SEA (`mar_aberto`), naval phase 1 (docs/NAVEGACAO.md).
//!
//! One zone, one process, centred on its own box. It came back on
//! 15/11/2026 after the September sea was removed for being MANDATORY: this
//! one is optional. The Harbour Captain still takes you from island to
//! island (for copper); the sea exists because it pays.
//!
//! # The island seen from the sea is a SILHOUETTE
//!
//! Analytic, no Perlin: each island is a profile of its own (`Feitio`) —
//! the Bosque's beaches and wooded hills, the Glacier's white cliffs and
//! floes, the Waste's dunes and mesas, the Plateau's dark cliffs and sea
//! stacks — out of a few octaves of value noise, with a quay and a pier of
//! planks marking the way in (visual remake, 16/11/2026; it was a dome).
//! Cheap on a phone and honest — whoever wants the island docks. Only the four islands with a port (`terreno::tem_porto`) are here;
//! Skyreach, Kōgen-tō and Abyssia stay off the sea.
//!
//! # A CHART, not the world map
//!
//! The islands sit at `ESCALA` of their world-map positions. At 1:1 the sea
//! would be 5,200 u wide, a 216 MB height field on the server and minutes of
//! sailing per crossing (the toll that killed the old sea). At a quarter the
//! whole sea is a 1,400 u square that fits the wire with room to spare, a
//! crossing takes 30-60 s, and the islands keep their places relative to
//! each other.
//!
//! # The relief is a `RelevoDesenhado`
//!
//! `bloco_da_coluna` plugs into `Gerador::bloco_em` like Abyssia's and
//! Kōgen-tō's: the server's height field, the client's terrain and the
//! minimap all read the same function.

use glam::Vec2;

use crate::terreno::{suave, Bioma, DefIlha, Material, ARQUIPELAGO, BLOCO, NIVEL_DO_MAR};

pub const ZONA: &str = "mar_aberto";
/// Bump when the relief changes: it names the height caches.
pub const REVISAO: u32 = 2;
pub const SEMENTE: i32 = 0x0005_EA01;
/// The square the height field covers: ±900 u (visual remake, 16/11/2026:
/// was ±700 with domes of 60-120 u; the islands grew and so did the chart).
pub const RAIO_BLOCOS: i32 = 1800;
/// The world-map positions are scaled by this into the sea.
pub const ESCALA: f32 = 0.36;
/// Ships stay inside ±this (units): the edge of the chart.
pub const BORDA: f32 = 880.0;
/// Open-sea floor, in blocks. Deep enough to read as deep water.
pub const FUNDO: i32 = -30;
/// Close to a dock's anchorage this much: "Dock" is offered.
pub const PERTO_DO_CAIS: f32 = 14.0;
/// THE HARBOUR CORRIDOR: within this of an anchorage nobody takes damage
/// (mobs or players) and no mob is placed. The chase is the game; the door
/// is not.
pub const RAIO_DO_CORREDOR: f32 = 40.0;

/// The shallow shelf around each island, units.
pub const PLATAFORMA: f32 = 30.0;
/// The PIER: a long tongue of planks out of the quay, and a wider T at its
/// head so the berth reads as a berth.
pub const CAIS_LARG: f32 = 5.0;
pub const CAIS_COMP: f32 = 26.0;
pub const CABECA_LARG: f32 = 11.0;
pub const CABECA_COMP: f32 = 5.0;
/// The pier's TERRAIN top: one block over the water (it is a wall for the
/// hull). The planks the client draws sit on it.
pub const CAIS_ALTURA: f32 = 0.5;
/// The QUAY: flat stone ground at the pier's root, carved out of whatever
/// the island is (a cliff, a dune), where the lighthouse stands.
pub const ALTURA_DO_MOLHE: f32 = 1.0;
const MOLHE_FUNDO: f32 = 24.0;
const MOLHE_LARG: f32 = 15.0;

/// What an island looks like from the sea: its silhouette and palette.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Feitio {
    /// Green, forested, ringed with beaches.
    Bosque,
    /// White snow cliffs over a skirt of ice floes.
    Geleira,
    /// Sand dunes and sandstone mesas.
    Ermo,
    /// Tall dark cliffs, sea stacks, a stormy crown.
    Planalto,
}

/// An island as the sea sees it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct IlhaVista {
    /// Index in `ARQUIPELAGO`.
    pub ilha: usize,
    pub zona: &'static str,
    pub nome: &'static str,
    /// Centre, in sea coordinates.
    pub centro: Vec2,
    /// The FARTHEST the coast goes from the centre (the coast wanders
    /// between 84% and 100% of this, see `costa`).
    pub raio: f32,
    /// The dock's root on the coast and its tip, sea coordinates.
    pub raiz: Vec2,
    pub ponta: Vec2,
    /// Level band of the island (the sea around it follows it).
    pub nivel: (u32, u32),
    pub feitio: Feitio,
    /// Phase of the coast's wobble and the island's noise.
    pub fase: f32,
    /// The Waste's mesas: (centre, radius, height). Radius 0 = none.
    pub mesas: [(Vec2, f32, f32); 3],
}

impl IlhaVista {
    /// The ANCHORAGE: the water just past the dock's tip. Where the ship
    /// appears when sailing out and where it docks. Water, not planks: the
    /// dock is solid, it is the island's door.
    pub fn fundeadouro(&self) -> Vec2 {
        self.ponta + self.eixo() * 3.5
    }

    /// Along the pier, out to sea.
    pub fn eixo(&self) -> Vec2 {
        (self.ponta - self.raiz).normalize_or_zero()
    }

    /// Across the pier (the lighthouse's side).
    pub fn lado(&self) -> Vec2 {
        let e = self.eixo();
        Vec2::new(-e.y, e.x)
    }

    /// `p` in the pier's frame: (along it from the root, across it).
    pub fn no_cais(&self, p: Vec2) -> (f32, f32) {
        let r = p - self.raiz;
        (r.dot(self.eixo()), r.dot(self.lado()))
    }

    /// Distance from the centre to the coast at angle `ang` (radians, from
    /// +x towards +z): a few sines, so the island has capes and bays
    /// instead of a circle's edge.
    pub fn costa(&self, ang: f32) -> f32 {
        let f = self.fase;
        let s = 0.5
            + 0.22 * (ang * 3.0 + f).sin()
            + 0.16 * (ang * 5.0 + f * 2.3).sin()
            + 0.08 * (ang * 11.0 + f * 4.1).sin();
        self.raio * (0.84 + 0.16 * s.clamp(0.0, 1.0))
    }

    pub fn costa_em(&self, p: Vec2) -> f32 {
        let r = p - self.centro;
        self.costa(r.y.atan2(r.x))
    }

    /// Where the LIGHTHOUSE stands: on the quay, beside the pier's root.
    pub fn farol(&self) -> Vec2 {
        self.raiz - self.eixo() * 12.0 + self.lado() * 10.5
    }
}

fn feitio_de(b: Bioma) -> Feitio {
    match b {
        Bioma::Floresta => Feitio::Bosque,
        Bioma::Gelo => Feitio::Geleira,
        Bioma::Deserto => Feitio::Ermo,
        _ => Feitio::Planalto,
    }
}

/// The islands of the sea, computed once.
pub fn ilhas() -> &'static [IlhaVista] {
    static ILHAS: std::sync::OnceLock<Vec<IlhaVista>> = std::sync::OnceLock::new();
    ILHAS.get_or_init(|| {
        let defs: Vec<(usize, &DefIlha)> = ARQUIPELAGO
            .iter()
            .enumerate()
            .filter(|(_, d)| crate::terreno::tem_porto(d.zona))
            .collect();
        // Big enough to read as LAND from a deck (the remake's first
        // complaint: "miniature islands"): 160 u for the two big islands,
        // 105 for the two small ones.
        let raio_de = |d: &DefIlha| (d.raio_m() * 0.2).clamp(105.0, 160.0);
        // The box of the scaled islands, so the sea is centred on itself.
        let (mut x0, mut x1, mut z0, mut z1) = (f32::MAX, f32::MIN, f32::MAX, f32::MIN);
        for (_, d) in &defs {
            let c = Vec2::from(d.centro) * ESCALA;
            let r = raio_de(d);
            x0 = x0.min(c.x - r);
            x1 = x1.max(c.x + r);
            z0 = z0.min(c.y - r);
            z1 = z1.max(c.y + r);
        }
        let origem = Vec2::new((x0 + x1) * 0.5, (z0 + z1) * 0.5);
        defs.into_iter()
            .map(|(i, d)| {
                let centro = Vec2::from(d.centro) * ESCALA - origem;
                let raio = raio_de(d);
                // The same way the real port faces: towards the others.
                let rumo = d.rumo_do_porto().unwrap_or(Vec2::X);
                let feitio = feitio_de(d.bioma);
                let mut v = IlhaVista {
                    ilha: i,
                    zona: d.zona,
                    nome: d.nome,
                    centro,
                    raio,
                    raiz: centro,
                    ponta: centro,
                    nivel: d.nivel,
                    feitio,
                    fase: i as f32 * 2.399 + 0.7,
                    mesas: [(Vec2::ZERO, 0.0, 0.0); 3],
                };
                let ang = rumo.y.atan2(rumo.x);
                v.raiz = centro + rumo * (v.costa(ang) - 3.0);
                v.ponta = v.raiz + rumo * CAIS_COMP;
                if feitio == Feitio::Ermo {
                    // Three mesas, away from the quay (never behind the dock).
                    for k in 0..3 {
                        let a = ang + std::f32::consts::PI * (0.55 + 0.45 * k as f32) + 0.2 * (v.fase + k as f32).sin();
                        let dist = raio * [0.42, 0.22, 0.5][k];
                        v.mesas[k] = (
                            centro + Vec2::from_angle(a) * dist,
                            [22.0, 17.0, 14.0][k],
                            [24.0, 19.0, 15.0][k],
                        );
                    }
                }
                v
            })
            .collect()
    })
}

/// The island of `zona` on the sea, if it has a dock.
pub fn vista_da_zona(zona: &str) -> Option<&'static IlhaVista> {
    ilhas().iter().find(|v| v.zona == zona)
}

/// The island of `ARQUIPELAGO` index `ilha` on the sea.
pub fn vista_da_ilha(ilha: usize) -> Option<&'static IlhaVista> {
    ilhas().iter().find(|v| v.ilha == ilha)
}

pub fn e_mar(zona: &str) -> bool {
    zona == ZONA
}

/// Inside the chart (ships stop at the edge).
pub fn dentro(p: Vec2) -> bool {
    p.x.abs() < BORDA && p.y.abs() < BORDA
}

// ───────────────────────────── the relief ─────────────────────────────
//
// Hand-shaped, not the island's real generator downsampled: the real
// islands are 800 u of Perlin with villages and roads, and a quarter of
// that is mush. A few octaves of VALUE NOISE (a hash per lattice point,
// smoothstep between) and a profile per `Feitio` give each island the one
// thing a sailor reads — its silhouette — for a handful of multiplies per
// column, the same on the server and the client.

fn h01(ix: i32, iz: i32, s: u32) -> f32 {
    let mut h = (ix as u32).wrapping_mul(0x9E37_79B1)
        ^ (iz as u32).wrapping_mul(0x85EB_CA77)
        ^ s.wrapping_mul(0xC2B2_AE35);
    h ^= h >> 15;
    h = h.wrapping_mul(0x2C1B_3C6D);
    h ^= h >> 12;
    (h & 0xFFFF) as f32 / 65535.0
}

fn ruido(p: Vec2, s: u32) -> f32 {
    let (fx, fz) = (p.x.floor(), p.y.floor());
    let (ix, iz) = (fx as i32, fz as i32);
    let (tx, tz) = (suave(p.x - fx), suave(p.y - fz));
    let a = h01(ix, iz, s);
    let b = h01(ix + 1, iz, s);
    let c = h01(ix, iz + 1, s);
    let d = h01(ix + 1, iz + 1, s);
    let ab = a + (b - a) * tx;
    let cd = c + (d - c) * tx;
    ab + (cd - ab) * tz
}

/// Three octaves of value noise, in [0, 1].
fn fbm(p: Vec2, s: u32) -> f32 {
    ruido(p, s) * 0.57
        + ruido(p * 2.03 + Vec2::new(17.3, 5.1), s ^ 0x51) * 0.29
        + ruido(p * 4.1 + Vec2::new(-3.7, 11.9), s ^ 0xA3) * 0.14
}

/// One column of land of the sea (an island, the pier, a floe, a stack).
#[derive(Debug, Clone, Copy)]
pub struct Terra {
    pub ilha: &'static IlhaVista,
    /// Height of the ground, units over the sea.
    pub h: f32,
    /// How far inland from the coast (negative = out in the water: a floe
    /// or a sea stack).
    pub dentro: f32,
    /// On the pier's planks.
    pub cais: bool,
    /// How much of the quay this is (0..1).
    pub molhe: f32,
}

fn semente_da(i: &IlhaVista) -> u32 {
    SEMENTE as u32 ^ (i.ilha as u32).wrapping_mul(7919)
}

/// What stands in the water just off the coast: the Glacier's ice floes,
/// the Plateau's sea stacks, the Bosque's rocks. `fora` = distance past
/// the coast. Never in front of the pier (the way in stays open) and never
/// past `raio + 14` (`agua_livre` starts at `raio + 15`).
fn feicao_no_mar(i: &IlhaVista, p: Vec2, fora: f32, d: f32, s: u32) -> Option<f32> {
    if d > i.raio + 14.0 {
        return None;
    }
    let (ao_longo, de_lado) = i.no_cais(p);
    if ao_longo > -8.0 && de_lado.abs() < 34.0 {
        return None;
    }
    let (celula, chance, folga, r0, r1) = match i.feitio {
        Feitio::Geleira => (8.0, 3u32, 5.0, 1.5, 3.2),
        Feitio::Planalto => (15.0, 4, 4.0, 1.6, 3.4),
        Feitio::Bosque => (14.0, 7, 3.0, 0.9, 1.8),
        Feitio::Ermo => return None,
    };
    if fora < folga {
        return None;
    }
    let (cx, cz) = ((p.x / celula).floor() as i32, (p.y / celula).floor() as i32);
    let h = h01(cx, cz, s ^ 0xF10E);
    if (h * 1000.0) as u32 % chance != 0 {
        return None;
    }
    let r = r0 + (r1 - r0) * h01(cx, cz, s ^ 0x77);
    let folga_c = r + 0.5;
    let ox = folga_c + (celula - 2.0 * folga_c) * h01(cx, cz, s ^ 0x13);
    let oz = folga_c + (celula - 2.0 * folga_c) * h01(cx, cz, s ^ 0x29);
    let c = Vec2::new(cx as f32 * celula + ox, cz as f32 * celula + oz);
    // A lumpy disc, not a coin.
    let q = p - c;
    let borda = r * (0.8 + 0.35 * ruido(q * 0.9 + c, s ^ 0x5A));
    if q.length() > borda {
        return None;
    }
    Some(match i.feitio {
        Feitio::Geleira => {
            if h01(cx, cz, s ^ 0x3) < 0.3 {
                1.0
            } else {
                CAIS_ALTURA
            }
        }
        Feitio::Planalto => 6.0 + 12.0 * h01(cx, cz, s ^ 0x9) * (1.0 - q.length() / borda * 0.4),
        _ => 0.5 + 1.2 * h01(cx, cz, s ^ 0x9),
    })
}

/// The land at `p` (sea coordinates), if any.
pub fn terra_em(p: Vec2) -> Option<Terra> {
    for i in ilhas() {
        let r = p - i.centro;
        let d = r.length();
        if d > i.raio + PLATAFORMA {
            continue;
        }
        return terra_da(i, p, d, i.costa(r.y.atan2(r.x)));
    }
    None
}

fn terra_da(i: &'static IlhaVista, p: Vec2, d: f32, costa: f32) -> Option<Terra> {
    let s = semente_da(i);
    let e = costa - d;
    let (ao_longo, lado) = i.no_cais(p);
    let de_lado = lado.abs();
    // THE PIER: a tongue of planks, and the T of its head.
    let cais = ((-3.0..=CAIS_COMP).contains(&ao_longo) && de_lado <= CAIS_LARG * 0.5)
        || ((CAIS_COMP - CABECA_COMP..=CAIS_COMP).contains(&ao_longo) && de_lado <= CABECA_LARG * 0.5);
    if cais && e <= 0.0 {
        return Some(Terra { ilha: i, h: CAIS_ALTURA, dentro: e, cais: true, molhe: 0.0 });
    }
    if e < 0.0 {
        let h = feicao_no_mar(i, p, -e, d, s)?;
        return Some(Terra { ilha: i, h, dentro: e, cais: false, molhe: 0.0 });
    }
    let n = fbm(p * 0.035, s);
    let mut h = match i.feitio {
        Feitio::Bosque => {
            // Wide beaches, then rolling wooded hills and a crown.
            let praia = (0.5 + e * 0.07).min(1.2);
            let t = suave((e - 9.0) / 45.0);
            praia + t * (3.0 + 15.0 * n + 9.0 * suave((e - 45.0) / 60.0))
        }
        Feitio::Geleira => {
            // A thin strip of ice, then a sheer white wall and a snowfield.
            let praia = (0.5 + e * 0.1).min(1.0);
            let falesia = suave((e - 2.5) / 2.5) * (13.0 + 7.0 * n);
            let alto = suave((e - 8.0) / 45.0) * (8.0 + 14.0 * fbm(p * 0.06, s ^ 5));
            praia + falesia + alto
        }
        Feitio::Ermo => {
            // Beach, then dunes in long parallel crests, and the mesas.
            let praia = (0.5 + e * 0.06).min(1.3);
            let t = suave((e - 12.0) / 30.0);
            let dir = Vec2::from_angle(0.6 + i.fase);
            let crista = (p.dot(dir) * 0.09 + n * 4.0).sin().abs();
            let mut h = praia + t * (1.5 + 5.5 * crista * (0.4 + n));
            for (c, r, alto) in i.mesas {
                if r <= 0.0 {
                    continue;
                }
                let q = p - c;
                let borda = r * (0.85 + 0.3 * ruido(q * 0.12 + c * 0.01, s ^ 0x4E5A));
                let k = suave((borda + 2.5 - q.length()) / 2.5);
                h = h.max(h + (alto - h) * k);
            }
            h
        }
        Feitio::Planalto => {
            // No beach: dark cliffs straight out of the water, a jagged top.
            let falesia = suave((e - 1.0) / 2.5) * (22.0 + 9.0 * n);
            let pico = suave((e - 10.0) / 50.0) * (6.0 + 22.0 * fbm(p * 0.05, s ^ 9).powi(2));
            0.5 + e.min(1.5) * 0.3 + falesia + pico
        }
    };
    // THE QUAY: carved flat out of whatever the island is, at the pier's root.
    let mut molhe = 0.0;
    if ao_longo > -MOLHE_FUNDO - 4.0 && ao_longo < 6.0 && de_lado < MOLHE_LARG + 4.0 {
        molhe = (1.0 - suave((de_lado - MOLHE_LARG) / 4.0)) * (1.0 - suave((-ao_longo - MOLHE_FUNDO) / 4.0));
        h += (ALTURA_DO_MOLHE - h) * molhe;
    }
    Some(Terra { ilha: i, h, dentro: e, cais: false, molhe })
}

/// Top block of the column, in sea coordinates (blocks from the centre).
///
/// One distance test per island: almost every column of the open sea stops
/// at the first `continue` and never touches the noise.
pub fn bloco_da_coluna(bx: i32, bz: i32) -> i32 {
    let p = Vec2::new(bx as f32 * BLOCO, bz as f32 * BLOCO);
    let mut alto = f32::MIN;
    let mut fundo = FUNDO as f32 * BLOCO;
    for i in ilhas() {
        let r = p - i.centro;
        let d = r.length();
        if d > i.raio + PLATAFORMA {
            continue;
        }
        let costa = i.costa(r.y.atan2(r.x));
        // THE SHELF: the floor rises towards the coast, so the shore reads
        // as shallow water and not as a wall standing in the deep.
        let t_raso = ((costa + PLATAFORMA - d) / PLATAFORMA).clamp(0.0, 1.0);
        fundo = fundo.max(FUNDO as f32 * BLOCO * (1.0 - suave(t_raso)) - BLOCO);
        if let Some(t) = terra_da(i, p, d, costa) {
            alto = alto.max(t.h);
        }
    }
    if alto == f32::MIN {
        return (fundo / BLOCO).round() as i32 - 1;
    }
    // Land is land: never rounded down into the water.
    ((alto / BLOCO).round() as i32 - 1).max(0)
}

/// The material of the TOP of a land column of the sea (client terrain and
/// map). `None` = water.
pub fn material_do_topo(bx: i32, bz: i32) -> Option<Material> {
    let p = Vec2::new(bx as f32 * BLOCO, bz as f32 * BLOCO);
    let t = terra_em(p)?;
    if t.cais {
        return Some(Material::Tronco);
    }
    if t.molhe > 0.55 {
        return Some(if (bx.div_euclid(3) + bz.div_euclid(3)) % 2 == 0 {
            Material::Calcada
        } else {
            Material::CalcadaEscura
        });
    }
    let s = semente_da(t.ilha);
    let n = fbm(p * 0.09, s ^ 0xC0);
    Some(match t.ilha.feitio {
        _ if t.dentro < 0.0 => material_da_feicao(t.ilha.feitio),
        Feitio::Bosque => {
            if t.dentro < 7.0 {
                if t.h < 0.8 {
                    Material::AreiaMolhada
                } else {
                    Material::Areia
                }
            } else if t.dentro < 10.0 {
                Material::GramaClara
            } else if n < 0.38 {
                Material::GramaEscura
            } else if n > 0.66 {
                Material::GramaClara
            } else {
                Material::Grama
            }
        }
        Feitio::Geleira => {
            if t.dentro < 2.5 || n < 0.22 {
                Material::Gelo
            } else {
                Material::Neve
            }
        }
        Feitio::Ermo => {
            if t.h > 12.0 {
                Material::Arenito
            } else if t.dentro < 5.0 {
                Material::AreiaMolhada
            } else if n > 0.8 {
                Material::Arenito
            } else {
                Material::Areia
            }
        }
        Feitio::Planalto => {
            if t.dentro < 1.5 {
                Material::RochaAbissal
            } else if n < 0.4 && t.h > 20.0 {
                Material::GramaEscura
            } else if n > 0.72 {
                Material::Rocha
            } else {
                Material::RochaEscura
            }
        }
    })
}

fn material_da_feicao(f: Feitio) -> Material {
    match f {
        Feitio::Geleira => Material::Gelo,
        Feitio::Planalto => Material::RochaAbissal,
        _ => Material::Rocha,
    }
}

/// The material of a land column's SIDE, `prof` blocks under its top block
/// `topo`: strata by absolute height, so a cliff reads as rock in courses.
pub fn material_da_parede(bx: i32, bz: i32, topo: i32, prof: i32) -> Material {
    let p = Vec2::new(bx as f32 * BLOCO, bz as f32 * BLOCO);
    let Some(t) = terra_em(p) else {
        return Material::Rocha;
    };
    if t.cais {
        return Material::Tronco;
    }
    if t.molhe > 0.55 {
        return Material::CalcadaEscura;
    }
    if t.dentro < 0.0 {
        return material_da_feicao(t.ilha.feitio);
    }
    let y = topo - prof;
    let onda = (bx.wrapping_mul(7) ^ bz.wrapping_mul(13)) & 3;
    match t.ilha.feitio {
        Feitio::Bosque => match prof {
            0 => material_do_topo(bx, bz).unwrap_or(Material::Grama),
            1..=2 => Material::Terra,
            _ => {
                if (y + onda / 2).rem_euclid(5) == 0 {
                    Material::RochaEscura
                } else {
                    Material::Rocha
                }
            }
        },
        Feitio::Geleira => {
            if y <= 2 {
                Material::RochaEscura
            } else if prof == 0 || (y + onda / 2).rem_euclid(6) >= 2 {
                Material::Neve
            } else {
                Material::Gelo
            }
        }
        Feitio::Ermo => {
            if prof == 0 && t.h < 12.0 {
                Material::Areia
            } else {
                match (y + onda / 3).rem_euclid(7) {
                    0 => Material::Terra,
                    3 | 4 => Material::Areia,
                    _ => Material::Arenito,
                }
            }
        }
        Feitio::Planalto => {
            if prof == 0 && t.h > 20.0 {
                material_do_topo(bx, bz).unwrap_or(Material::RochaEscura)
            } else {
                match (y + onda / 2).rem_euclid(6) {
                    0 => Material::RochaEscura,
                    3 => Material::Rocha,
                    _ => Material::RochaAbissal,
                }
            }
        }
    }
}

/// Ground height in world units.
pub fn altura(x: f32, z: f32) -> f32 {
    let (bx, bz) = ((x / BLOCO).round() as i32, (z / BLOCO).round() as i32);
    (bloco_da_coluna(bx, bz) + 1) as f32 * BLOCO
}

/// Is there NAVIGABLE water here? Water, and inside the chart.
pub fn navegavel(x: f32, z: f32) -> bool {
    dentro(Vec2::new(x, z)) && altura(x, z) <= NIVEL_DO_MAR
}

/// The dock whose anchorage is near `p`.
pub fn cais_perto(p: Vec2) -> Option<&'static IlhaVista> {
    ilhas().iter().find(|i| i.fundeadouro().distance(p) <= PERTO_DO_CAIS)
}

/// In the harbour corridor (no damage, no mobs)?
pub fn no_corredor(p: Vec2) -> bool {
    ilhas().iter().any(|i| i.fundeadouro().distance(p) <= RAIO_DO_CORREDOR)
}

/// The nearest island (by its coast).
pub fn ilha_mais_perto(p: Vec2) -> &'static IlhaVista {
    ilhas()
        .iter()
        .min_by(|a, b| {
            (p.distance(a.centro) - a.raio).total_cmp(&(p.distance(b.centro) - b.raio))
        })
        .expect("the sea has islands")
}

/// The level of the sea at `p`: the nearest island's band, rising from its
/// low end at the coast to its high end 250 u out.
pub fn nivel_em(p: Vec2) -> u32 {
    let i = ilha_mais_perto(p);
    let t = ((p.distance(i.centro) - i.raio) / 250.0).clamp(0.0, 1.0);
    i.nivel.0 + (t * (i.nivel.1 - i.nivel.0) as f32) as u32
}

/// The routes: every pair of anchorages. Mobs and chests sit on them,
/// because the routes are the map — a mob in the middle of nowhere would
/// never be met.
pub fn rotas() -> Vec<(Vec2, Vec2)> {
    let v = ilhas();
    let mut r = Vec::new();
    for a in 0..v.len() {
        for b in a + 1..v.len() {
            r.push((v[a].fundeadouro(), v[b].fundeadouro()));
        }
    }
    r
}

/// Open water fit for something to float or swim in: navigable, off the
/// shelves, outside the harbour corridors.
pub fn agua_livre(p: Vec2) -> bool {
    navegavel(p.x, p.y)
        && !no_corredor(p)
        && ilhas().iter().all(|i| p.distance(i.centro) > i.raio + PLATAFORMA * 0.5)
}

/// Where the sea mobs live: along every route, at a third and two thirds.
pub fn ninhos() -> Vec<Vec2> {
    rotas()
        .into_iter()
        .flat_map(|(a, b)| [a.lerp(b, 0.33), a.lerp(b, 0.67)])
        .filter(|p| agua_livre(*p))
        .collect()
}

/// Where the drifting chests appear: the middle of every route, pushed a
/// little off it so they are a detour, not a toll.
pub fn baus() -> Vec<Vec2> {
    rotas()
        .into_iter()
        .enumerate()
        .map(|(k, (a, b))| {
            let meio = a.lerp(b, 0.5);
            let d = (b - a).normalize_or_zero();
            let lado = if k % 2 == 0 { 1.0 } else { -1.0 };
            meio + Vec2::new(-d.y, d.x) * 24.0 * lado
        })
        .filter(|p| agua_livre(*p))
        .collect()
}

/// LAWLESS WATERS (naval phase 4): radius of each zone, units.
pub const RAIO_SEM_LEI: f32 = 45.0;

/// THE LAWLESS WATERS: a few stretches in the middle of the routes, around
/// the route's ELITE NEST (two thirds of the way, where the wrecks and the
/// Dread elites are), where PK is open for everyone at sea — the sea's
/// `pk::zona_pvp_aberto`. Only on the routes that do NOT touch the Bosque:
/// the starter island's routes are where new captains learn to sail, and
/// turning them into no man's land would tax being new. With four ports
/// that is three zones: Glacier–Waste, Glacier–Plateau, Waste–Plateau.
/// Never in a harbour corridor (test).
pub fn zonas_sem_lei() -> &'static [Vec2] {
    static Z: std::sync::OnceLock<Vec<Vec2>> = std::sync::OnceLock::new();
    Z.get_or_init(|| {
        let v = ilhas();
        let bosque = v.iter().position(|i| i.zona == "ilha_inicial");
        let mut z = Vec::new();
        for a in 0..v.len() {
            for b in a + 1..v.len() {
                if Some(a) == bosque || Some(b) == bosque {
                    continue;
                }
                let (pa, pb) = (v[a].fundeadouro(), v[b].fundeadouro());
                let mut p = pa.lerp(pb, 0.67);
                if !agua_livre(p) {
                    p = pa.lerp(pb, 0.5);
                }
                z.push(p);
            }
        }
        z
    })
}

/// In lawless waters (and never inside a harbour corridor)?
pub fn sem_lei(p: Vec2) -> bool {
    !no_corredor(p) && zonas_sem_lei().iter().any(|c| c.distance(p) <= RAIO_SEM_LEI)
}

/// A piece of FLOATING WRECKAGE to gather. `tier`: 0 timber (wood), 1..4
/// a crate of Steel of that colour (the rock's yield: Steel and Darksteel),
/// 5 Energy.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Destroco {
    pub centro: Vec2,
    pub tier: u8,
    pub variante: u32,
}

/// Cell of the wreckage lattice, in blocks: at most one piece per cell.
const CELULA_DESTROCO: i32 = 40;

fn mistura(a: i32, b: i32) -> u32 {
    let mut h = (a as u32).wrapping_mul(0x9E37_79B1) ^ (b as u32).wrapping_mul(0x85EB_CA77) ^ SEMENTE as u32;
    h ^= h >> 15;
    h = h.wrapping_mul(0x2C1B_3C6D);
    h ^= h >> 12;
    h
}

/// The wreckage in column `(bx, bz)`, if any. Pure function of the column:
/// the server plants the same nodes the client draws.
pub fn destroco_da_coluna(bx: i32, bz: i32) -> Option<Destroco> {
    let (cx, cz) = (bx.div_euclid(CELULA_DESTROCO), bz.div_euclid(CELULA_DESTROCO));
    let h = mistura(cx, cz);
    // One cell in five has a piece.
    if h % 5 != 0 {
        return None;
    }
    let ox = ((h >> 4) % (CELULA_DESTROCO as u32 - 8)) as i32 + 4;
    let oz = ((h >> 10) % (CELULA_DESTROCO as u32 - 8)) as i32 + 4;
    if bx != cx * CELULA_DESTROCO + ox || bz != cz * CELULA_DESTROCO + oz {
        return None;
    }
    let centro = Vec2::new(bx as f32 * BLOCO, bz as f32 * BLOCO);
    if !agua_livre(centro) {
        return None;
    }
    let nivel = nivel_em(centro);
    let cor = (1 + nivel / 15).min(4) as u8;
    let tier = match (h >> 18) % 10 {
        0..=2 => 0,
        3..=7 => cor,
        _ => 5,
    };
    Some(Destroco { centro, tier, variante: h >> 22 })
}

/// The sea as the rest of the game reads a zone (`terreno::def_da_zona`).
/// Not in the `ARQUIPELAGO`: its indices are API (travel menu, story steps).
pub const DEF: DefIlha = DefIlha {
    zona: ZONA,
    nome: "Open Sea",
    semente: SEMENTE,
    raio_blocos: RAIO_BLOCOS,
    bioma: Bioma::Floresta,
    centro: [-1800.0, 0.0],
    nivel: (1, 60),
};

#[cfg(test)]
mod testes {
    use super::*;

    /// THE SEA FITS THE WIRE. `EntityState::pos` is i16 at `POS_SCALE` and
    /// `quantize` CLAMPS in silence: a sea too big gives no error, it piles
    /// every entity past the limit on the edge.
    #[test]
    fn o_mar_inteiro_cabe_no_orcamento_do_fio() {
        let teto = i16::MAX as f32 / crate::components::POS_SCALE;
        let meia = RAIO_BLOCOS as f32 * BLOCO;
        assert!(meia < teto, "the sea is ±{meia} u and the wire carries ±{teto:.0} u");
        assert!(BORDA < meia, "ships would leave the height field");
        for i in ilhas() {
            let longe = i.centro.abs().max_element() + i.raio + PLATAFORMA;
            assert!(longe < BORDA, "{} pokes out of the chart ({longe})", i.nome);
        }
        // Centred on its own box: the extremes are symmetric.
        let (mn, mx) = ilhas().iter().fold((f32::MAX, f32::MIN), |(a, b), i| {
            (a.min(i.centro.x - i.raio), b.max(i.centro.x + i.raio))
        });
        assert!((mn + mx).abs() < 1.0, "not centred: {mn}..{mx}");
    }

    #[test]
    fn as_quatro_ilhas_com_porto_estao_no_mar() {
        assert_eq!(ilhas().len(), 4);
        for z in ["ilha_inicial", "ilha_gelo", "ilha_deserto", "ilha_planalto"] {
            assert!(vista_da_zona(z).is_some(), "{z} missing");
        }
        assert!(vista_da_zona(crate::celeste::ZONA).is_none());
    }

    /// The island is a WALL and the dock is the DOOR: the centre is land,
    /// the dock is solid, the anchorage is water and offers docking.
    #[test]
    fn a_ilha_e_parede_e_o_cais_e_a_porta() {
        for i in ilhas() {
            assert!(!navegavel(i.centro.x, i.centro.y), "{}: centre is water", i.nome);
            let meio = (i.raiz + i.ponta) * 0.5;
            assert!(!navegavel(meio.x, meio.y), "{}: dock not solid", i.nome);
            let f = i.fundeadouro();
            assert!(navegavel(f.x, f.y), "{}: anchorage on land", i.nome);
            assert_eq!(cais_perto(f).map(|v| v.ilha), Some(i.ilha));
            assert!(no_corredor(f));
        }
        assert!(cais_perto(Vec2::new(0.0, 0.0)).is_none());
    }

    /// The hull moves on water and stops at the coast (`mover_casco`).
    #[test]
    fn o_casco_so_anda_na_agua() {
        use crate::terreno::mover_casco;
        let agua = |x: f32, z: f32| navegavel(x, z);
        let i = &ilhas()[0];
        let f = i.fundeadouro();
        let fora = (f - i.centro).normalize_or_zero();
        // Away from the island: moves.
        let andou = mover_casco(&agua, f, fora * 10.0, 1.0, crate::navio::RAIO_DO_CASCO);
        assert!(andou.distance(f) > 5.0, "the hull did not move: {andou:?}");
        // Towards the island's centre, from the open sea beside the dock:
        // it stops on the water and never climbs the beach.
        let lado = Vec2::new(-fora.y, fora.x);
        let ao_lado = i.centro + fora.rotate(Vec2::from_angle(0.6)) * (i.raio + 20.0);
        assert!(navegavel(ao_lado.x, ao_lado.y));
        let mut p = ao_lado;
        for _ in 0..200 {
            p = mover_casco(&agua, p, (i.centro - p).normalize_or_zero() * 10.0, 1.0 / 30.0, crate::navio::RAIO_DO_CASCO);
        }
        assert!(navegavel(p.x, p.y), "climbed onto {}: {p:?}", i.nome);
        assert!(p.distance(i.centro) > i.costa_em(p) - 1.0);
        let _ = lado;
        // The chart's edge is a wall too.
        let borda = Vec2::new(BORDA - 3.0, 0.0);
        let p = mover_casco(&agua, borda, Vec2::new(10.0, 0.0), 2.0, crate::navio::RAIO_DO_CASCO);
        assert!(dentro(p));
    }

    #[test]
    fn ha_destrocos_baus_e_ninhos_na_agua() {
        let mut n = [0u32; 6];
        for bz in -RAIO_BLOCOS..RAIO_BLOCOS {
            for bx in -RAIO_BLOCOS..RAIO_BLOCOS {
                if let Some(d) = destroco_da_coluna(bx, bz) {
                    assert!(agua_livre(d.centro));
                    n[d.tier as usize] += 1;
                }
            }
        }
        let total: u32 = n.iter().sum();
        assert!(total > 300, "too little wreckage: {n:?}");
        assert!(n[0] > 0 && n[5] > 0, "no wood or Energy: {n:?}");
        assert!(baus().len() >= 5, "chests: {}", baus().len());
        assert!(ninhos().len() >= 8, "nests: {}", ninhos().len());
        for p in baus().into_iter().chain(ninhos()) {
            assert!(!no_corredor(p), "something in a harbour corridor at {p:?}");
        }
    }

    /// LAWLESS WATERS: three, mid-route, on open water, never touching a
    /// harbour corridor, and none on a Bosque route.
    #[test]
    fn as_aguas_sem_lei_ficam_no_meio_das_rotas() {
        let z = zonas_sem_lei();
        assert_eq!(z.len(), 3, "{z:?}");
        for c in z {
            assert!(navegavel(c.x, c.y), "lawless zone on land at {c:?}");
            assert!(sem_lei(*c));
            for i in ilhas() {
                assert!(
                    i.fundeadouro().distance(*c) > RAIO_SEM_LEI + RAIO_DO_CORREDOR,
                    "lawless zone {c:?} touches the {} corridor",
                    i.nome
                );
            }
        }
        for i in ilhas() {
            assert!(!sem_lei(i.fundeadouro()), "{}: the anchorage is lawless", i.nome);
        }
        let bosque = vista_da_zona("ilha_inicial").unwrap().fundeadouro();
        for v in ilhas().iter().filter(|v| v.zona != "ilha_inicial") {
            let meio = bosque.lerp(v.fundeadouro(), 0.67);
            assert!(!sem_lei(meio), "the Bosque route to {} is lawless", v.nome);
        }
    }

    #[test]
    fn o_nivel_sobe_longe_do_bosque() {
        let bosque = vista_da_zona("ilha_inicial").unwrap();
        let planalto = vista_da_zona("ilha_planalto").unwrap();
        assert!(nivel_em(bosque.fundeadouro()) <= 15);
        assert!(nivel_em(planalto.fundeadouro()) >= 40);
    }

    /// VISUAL REMAKE: every island has its own silhouette. Measured on the
    /// height field the server and client share: the Plateau towers, the
    /// Glacier is a wall a few units in from its coast, the Waste has mesas
    /// standing over its dunes, the Bosque has beaches.
    #[test]
    fn cada_ilha_tem_a_sua_silhueta() {
        let pico = |i: &IlhaVista| {
            let mut m = 0.0f32;
            for k in 0..400 {
                let a = k as f32 * 0.157;
                let r = (k % 20) as f32 / 20.0 * i.raio;
                m = m.max(altura(i.centro.x + a.cos() * r, i.centro.y + a.sin() * r));
            }
            m
        };
        let v = |z: &str| vista_da_zona(z).unwrap();
        let (bosque, gelo, ermo, planalto) = (v("ilha_inicial"), v("ilha_gelo"), v("ilha_deserto"), v("ilha_planalto"));
        assert_eq!(bosque.feitio, Feitio::Bosque);
        assert_eq!(planalto.feitio, Feitio::Planalto);
        assert!(pico(planalto) > 35.0, "the Plateau peaks at {}", pico(planalto));
        assert!(pico(gelo) > 20.0, "the Glacier peaks at {}", pico(gelo));
        for (c, r, alto) in ermo.mesas {
            assert!(r > 0.0);
            let h = altura(c.x, c.y);
            assert!((h - alto).abs() < 1.0, "a mesa stands at {h}, not {alto}");
        }
        // Cliffs: 10 u in from the coast, away from the quay, the Glacier
        // and the Plateau are already tall; the Bosque is still near the beach.
        let costa_adentro = |i: &IlhaVista, dentro: f32| {
            let a = (i.raiz - i.centro).to_angle() + std::f32::consts::PI;
            let r = i.costa(a) - dentro;
            altura(i.centro.x + a.cos() * r, i.centro.y + a.sin() * r)
        };
        assert!(costa_adentro(gelo, 9.0) > 12.0, "no Glacier cliff: {}", costa_adentro(gelo, 9.0));
        assert!(costa_adentro(planalto, 6.0) > 15.0, "no Plateau cliff: {}", costa_adentro(planalto, 6.0));
        assert!(costa_adentro(bosque, 5.0) < 2.0, "no Bosque beach: {}", costa_adentro(bosque, 5.0));
        let b = |p: Vec2| ((p.x / BLOCO).round() as i32, (p.y / BLOCO).round() as i32);
        let atras = (bosque.raiz - bosque.centro).to_angle() + std::f32::consts::PI;
        let (bx, bz) = b(bosque.centro + Vec2::from_angle(atras) * (bosque.costa(atras) - 3.0));
        assert!(matches!(material_do_topo(bx, bz), Some(Material::Areia | Material::AreiaMolhada)));
        let (bx, bz) = b(gelo.centro + Vec2::new(0.0, 1.0).rotate(Vec2::from_angle(0.3)) * 30.0);
        assert!(matches!(material_do_topo(bx, bz), Some(Material::Neve | Material::Gelo)));
    }

    /// THE DOCK READS AS A DOCK: planks on the pier, a flat stone quay at its
    /// root where the lighthouse stands, and the way in clear of floes and
    /// stacks — straight out from the anchorage, 60 u of open water.
    #[test]
    fn o_cais_e_de_tabua_e_o_caminho_esta_livre() {
        let b = |p: Vec2| ((p.x / BLOCO).round() as i32, (p.y / BLOCO).round() as i32);
        for i in ilhas() {
            let meio = i.raiz + i.eixo() * (CAIS_COMP * 0.6);
            let (bx, bz) = b(meio);
            assert_eq!(material_do_topo(bx, bz), Some(Material::Tronco), "{}: the pier is not wood", i.nome);
            let cabeca = i.ponta - i.eixo() * 2.0 + i.lado() * (CABECA_LARG * 0.4);
            assert!(!navegavel(cabeca.x, cabeca.y), "{}: no T at the pier's head", i.nome);
            let f = i.farol();
            let h = altura(f.x, f.y);
            assert!((h - ALTURA_DO_MOLHE).abs() < 0.3, "{}: the lighthouse stands at {h}", i.nome);
            let a = i.fundeadouro();
            for k in 0..60 {
                for lado in [-4.0f32, 0.0, 4.0] {
                    let q = a + i.eixo() * k as f32 + i.lado() * lado;
                    assert!(navegavel(q.x, q.y), "{}: the way in is blocked at {q:?}", i.nome);
                }
            }
        }
    }

    /// THE CROSSINGS stay a crossing, not a voyage: anchorage to anchorage
    /// between 30 and 100 s for the Sloop (10 u/s), never more than ~95 s.
    #[test]
    fn as_travessias_levam_de_meio_a_um_minuto_e_meio() {
        let v = ilhas();
        let mut tempos = Vec::new();
        for a in 0..v.len() {
            for b in a + 1..v.len() {
                let d = v[a].fundeadouro().distance(v[b].fundeadouro());
                tempos.push((v[a].nome, v[b].nome, d / 10.0));
            }
        }
        for (a, b, t) in &tempos {
            println!("{a}–{b}: {:.0} u, {t:.0} s", t * 10.0);
            assert!((30.0..=100.0).contains(t), "{a}–{b}: {t:.0} s");
        }
    }
}
