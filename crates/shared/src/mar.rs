//! THE OPEN SEA (`mar_aberto`), naval phase 1 (docs/NAVEGACAO.md).
//!
//! One zone, one process, centred on its own box. It came back on
//! 15/11/2026 after the September sea was removed for being MANDATORY: this
//! one is optional. The Harbour Captain still takes you from island to
//! island (for copper); the sea exists because it pays.
//!
//! # The island seen from the sea is a SILHOUETTE
//!
//! Analytic, like September's: a dome per island and a dock marking the way
//! in, no Perlin. Cheap on a phone and honest — whoever wants the island
//! docks. Only the four islands with a port (`terreno::tem_porto`) are here;
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

use crate::terreno::{suave, Bioma, DefIlha, ARQUIPELAGO, BLOCO, NIVEL_DO_MAR};

pub const ZONA: &str = "mar_aberto";
/// Bump when the relief changes: it names the height caches.
pub const REVISAO: u32 = 1;
pub const SEMENTE: i32 = 0x0005_EA01;
/// The square the height field covers: ±700 u.
pub const RAIO_BLOCOS: i32 = 1400;
/// The world-map positions are scaled by this into the sea.
pub const ESCALA: f32 = 0.25;
/// Ships stay inside ±this (units): the edge of the chart.
pub const BORDA: f32 = 680.0;
/// Open-sea floor, in blocks. Deep enough to read as deep water.
pub const FUNDO: i32 = -30;
/// Close to a dock's anchorage this much: "Dock" is offered.
pub const PERTO_DO_CAIS: f32 = 14.0;
/// THE HARBOUR CORRIDOR: within this of an anchorage nobody takes damage
/// (mobs or players) and no mob is placed. The chase is the game; the door
/// is not.
pub const RAIO_DO_CORREDOR: f32 = 40.0;

/// Dome height at the centre, units.
const ALTURA_DA_ILHA: f32 = 24.0;
/// What fraction of the radius the slope takes; the rest is plateau.
const ENCOSTA: f32 = 0.42;
/// The shallow shelf around each island, units.
const PLATAFORMA: f32 = 30.0;
const CAIS_LARG: f32 = 5.0;
const CAIS_COMP: f32 = 22.0;
const CAIS_ALTURA: f32 = 1.2;

/// An island as the sea sees it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct IlhaVista {
    /// Index in `ARQUIPELAGO`.
    pub ilha: usize,
    pub zona: &'static str,
    pub nome: &'static str,
    /// Centre, in sea coordinates.
    pub centro: Vec2,
    pub raio: f32,
    /// The dock's root on the coast and its tip, sea coordinates.
    pub raiz: Vec2,
    pub ponta: Vec2,
    /// Level band of the island (the sea around it follows it).
    pub nivel: (u32, u32),
}

impl IlhaVista {
    /// The ANCHORAGE: the water just past the dock's tip. Where the ship
    /// appears when sailing out and where it docks. Water, not planks: the
    /// dock is solid, it is the island's door.
    pub fn fundeadouro(&self) -> Vec2 {
        self.ponta + (self.ponta - self.raiz).normalize_or_zero() * 3.5
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
        let raio_de = |d: &DefIlha| (d.raio_m() * 0.15).clamp(50.0, 130.0);
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
                let raiz = centro + rumo * (raio - 2.0);
                IlhaVista {
                    ilha: i,
                    zona: d.zona,
                    nome: d.nome,
                    centro,
                    raio,
                    raiz,
                    ponta: raiz + rumo * CAIS_COMP,
                    nivel: d.nivel,
                }
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

/// Top block of the column, in sea coordinates (blocks from the centre).
///
/// Four distance tests. Almost every column of the open sea stops at the
/// first `continue` and never touches anything else.
pub fn bloco_da_coluna(bx: i32, bz: i32) -> i32 {
    let p = Vec2::new(bx as f32 * BLOCO, bz as f32 * BLOCO);
    let mut alto = f32::MIN;
    let mut fundo = FUNDO as f32 * BLOCO;
    for (k, i) in ilhas().iter().enumerate() {
        let d = p.distance(i.centro);
        if d > i.raio + PLATAFORMA {
            continue;
        }
        // THE SHELF: the floor rises towards the coast, so the shore reads
        // as shallow water and not as a wall standing in the deep.
        let t_raso = ((i.raio + PLATAFORMA - d) / PLATAFORMA).clamp(0.0, 1.0);
        fundo = fundo.max(FUNDO as f32 * BLOCO * (1.0 - suave(t_raso)) - BLOCO);
        if d <= i.raio {
            // THE DOME: plateau in the middle, slope at the edge, a little
            // swell by the angle so it is not a perfect bowl.
            let t = ((i.raio - d) / (i.raio * ENCOSTA)).clamp(0.0, 1.0);
            let r = p - i.centro;
            let ang = r.y.atan2(r.x);
            let ondula = 0.85 + 0.15 * (ang * 3.0 + k as f32 * 1.7).sin();
            alto = alto.max(BLOCO + suave(t) * ALTURA_DA_ILHA * ondula);
        }
        // THE DOCK: a narrow tongue out of the coast at sea level. It says
        // "the way in is here"; without it the island is a closed wall.
        let eixo = (i.ponta - i.raiz).normalize_or_zero();
        let rel = p - i.raiz;
        let ao_longo = rel.dot(eixo);
        let de_lado = rel.dot(Vec2::new(-eixo.y, eixo.x)).abs();
        if (-6.0..=CAIS_COMP).contains(&ao_longo) && de_lado <= CAIS_LARG * 0.5 {
            alto = alto.max(NIVEL_DO_MAR + CAIS_ALTURA);
        }
    }
    if alto == f32::MIN {
        return (fundo / BLOCO).round() as i32 - 1;
    }
    (alto / BLOCO).round() as i32 - 1
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
        assert!(p.distance(i.centro) > i.raio - 1.0);
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
}
