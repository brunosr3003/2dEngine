//! THE SHIP, remade (visual remake, 16/11/2026; docs/NAVEGACAO.md).
//!
//! The owner: "the boat is horrible". It was rows of loose `draw_cube`s —
//! three boxes narrowing to a bow, sticks and flat slabs for sails — a few
//! hundred cube draws a ship, every frame. Now each look (class, sail,
//! figurehead, brass) is a VOXEL MODEL built once (`vegetacao::malha_com`:
//! hidden faces culled, corner AO, greedy merge) and drawn as a handful of
//! static meshes with a matrix:
//!
//! * the **hull**: a shaped bow, a sheer rising fore and aft, a keel, a
//!   painted bottom, a wale stripe per class, deck planks, rails (brass with
//!   the stars), a quarterdeck with lit stern windows and a forecastle from
//!   the Galleon, gun ports with barrels (one row on the Schooner, the
//!   Galleon's four a side, the Man-o'-War's two decks), a bowsprit, masts,
//!   yards and a crow's nest, a stern lantern and the figurehead;
//! * the **sails**, one mesh per mast so each billows on its own (a scale
//!   along the wind on the matrix: zero vertex work on the CPU): gaff sails
//!   on the Sloop and Schooner, square courses and topsails on the big two,
//!   and a jib;
//! * **rigging**: shrouds and stays as lines, near ships only;
//! * **motion**: it rides the swell (`agua::onda_em`: rise, pitch, roll),
//!   heels into its turns, and leaves a WAKE of foam that widens and fades.
//!
//! LOD: past `LONGE` a ship is its hull and one sail mesh, no rigging; the
//! wake thins with distance.

use std::cell::RefCell;
use std::collections::{HashMap, VecDeque};

use macroquad::models::{Mesh, Vertex};
use macroquad::prelude::*;
use shared::navio::{self as nv, Visual};
use shared::terreno::Material;

use crate::vegetacao::{malha_com, Volume, VOX};

/// One hull voxel, units. The meshes come out at `VOX` (0.25) a voxel and
/// the matrix halves them.
const VOX_CASCO: f32 = 0.125;
const ESCALA: f32 = VOX_CASCO / VOX;

/// Past this (from the eye) a ship is hull + one sail mesh, no rigging.
const LONGE: f32 = 55.0;

// Palette SLOTS: materials the hull never uses for themselves, painted per
// look by `cor_do_slot`.
const VELA: Material = Material::TroncoBranco;
const VELA_BORDA: Material = Material::MarmoreSombra;
const FRISO: Material = Material::FolhaDourada;
const FAIXA: Material = Material::GramaCeleste;
const CASCO: Material = Material::Marmore;
const CASCO_ESCURO: Material = Material::NuvemSombra;
const FIGURA_A: Material = Material::PetalaRosa;
const FIGURA_B: Material = Material::FolhaCeleste;
const FIGURA_C: Material = Material::PetalaRoxa;
const FUNDO: Material = Material::Fruto;
const CONVES: Material = Material::Caminho;
const MADEIRA: Material = Material::Tronco;
const FERRO: Material = Material::Asfalto;
const LUZ: Material = Material::JanelaAcesa;

/// The measures of a class, in hull voxels (0.125 u).
struct Medidas {
    comp: i32,
    larg: i32,
    calado: i32,
    borda: i32,
    /// (position along the hull −1 stern..1 bow, height × length).
    mastros: &'static [(f32, f32)],
    redondo: bool,
    tombadilho: bool,
    castelo: bool,
    /// (rows, per side).
    canhoes: (i32, i32),
    casco: (u8, u8, u8),
    faixa: (u8, u8, u8),
}

fn medidas(classe: u8) -> Medidas {
    match classe {
        0 => Medidas {
            comp: 40,
            larg: 13,
            calado: 4,
            borda: 5,
            mastros: &[(0.12, 1.0)],
            redondo: false,
            tombadilho: false,
            castelo: false,
            canhoes: (0, 0),
            casco: (120, 80, 46),
            faixa: (226, 208, 160),
        },
        1 => Medidas {
            comp: 50,
            larg: 15,
            calado: 5,
            borda: 6,
            mastros: &[(0.32, 0.92), (-0.14, 1.02)],
            redondo: false,
            tombadilho: false,
            castelo: false,
            canhoes: (1, 2),
            casco: (104, 66, 38),
            faixa: (236, 236, 228),
        },
        2 => Medidas {
            comp: 60,
            larg: 19,
            calado: 6,
            borda: 7,
            mastros: &[(0.36, 0.86), (0.02, 1.0), (-0.34, 0.78)],
            redondo: true,
            tombadilho: true,
            castelo: true,
            canhoes: (1, 4),
            casco: (92, 54, 32),
            faixa: (176, 52, 44),
        },
        _ => Medidas {
            comp: 68,
            larg: 21,
            calado: 6,
            borda: 9,
            mastros: &[(0.38, 0.86), (0.02, 1.0), (-0.34, 0.8)],
            redondo: true,
            tombadilho: true,
            castelo: true,
            canhoes: (2, 5),
            casco: (40, 36, 34),
            faixa: (222, 178, 64),
        },
    }
}

/// Brass level of the trim, 0..3, from the stars (0..15).
fn latao(estrelas: u8) -> u8 {
    ((estrelas as u32 * 3 + 7) / 15).min(3) as u8
}

fn mistura(a: (u8, u8, u8), b: (u8, u8, u8), t: f32) -> (u8, u8, u8) {
    let f = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round() as u8;
    (f(a.0, b.0), f(a.1, b.1), f(a.2, b.2))
}

/// One look of a ship, built once and kept for the whole game (the GPU
/// buffers are keyed by the vertices' address: these never drop).
pub struct Navio3d {
    casco: Mesh,
    /// One per mast (and the jib): the mesh, the pivot it billows about (mesh
    /// units) and whether it billows along z (square sail) or x (gaff, jib).
    velas: Vec<(Mesh, Vec3, bool)>,
    /// All sails in one mesh: the far LOD.
    velas_juntas: Mesh,
    /// Rigging lines, mesh units.
    cordas: Vec<(Vec3, Vec3)>,
    /// The main mast's top (the pennant and the skull flag), mesh units.
    pub topo: Vec3,
    /// The stern lantern, mesh units.
    pub lanterna: Vec3,
    /// Half the length in world units (the wake starts at the stern).
    pub meio_comp: f32,
    pub vertices: usize,
}

type Chave = (u8, u8, u8, u8);

thread_local! {
    static MODELOS: RefCell<HashMap<Chave, &'static Navio3d>> = RefCell::new(HashMap::new());
    static MOVIMENTO: RefCell<HashMap<u64, Movimento>> = RefCell::new(HashMap::new());
}

/// The model of a look, building it the first time.
pub fn modelo(v: Visual) -> &'static Navio3d {
    let classe = v.classe.min(nv::CASCOS.len() as u8 - 1);
    let chave = (classe, v.vela, v.figura, latao(v.estrelas));
    MODELOS.with(|m| *m.borrow_mut().entry(chave).or_insert_with(|| Box::leak(Box::new(constroi(chave)))))
}

fn caixa(v: &mut Volume, a: [i32; 3], b: [i32; 3], m: Material) {
    for y in a[1]..b[1] {
        for z in a[2]..b[2] {
            for x in a[0]..b[0] {
                v.poe(x, y, z, m);
            }
        }
    }
}

fn para_mesh(m: crate::vegetacao::Modelo) -> Mesh {
    Mesh { vertices: m.verts, indices: m.idx, texture: None }
}

/// Voxel → mesh units (what the mesher writes: `VOX` a voxel, centred).
fn em_malha(x: f32, y: f32, z: f32) -> Vec3 {
    vec3(x + 0.5, y + 0.5, z + 0.5) * VOX
}

fn constroi((classe, vela, figura, nivel_latao): Chave) -> Navio3d {
    let m = medidas(classe);
    let pano = nv::cor_da_vela(vela);
    let pano = (pano[0], pano[1], pano[2]);
    let friso = mistura((70, 44, 24), (214, 172, 80), nivel_latao as f32 / 3.0);
    let casco_cor = m.casco;
    let faixa_cor = m.faixa;
    let fig = match figura {
        1 => ((228, 196, 170), (40, 150, 140), (196, 92, 44)),
        2 => ((52, 128, 62), (40, 96, 48), (230, 40, 30)),
        3 => ((214, 170, 58), (240, 232, 214), (120, 84, 40)),
        _ => ((120, 80, 46), (96, 64, 36), (90, 60, 30)),
    };
    let cor = move |mat: Material| -> (u8, u8, u8) {
        match mat {
            VELA => pano,
            VELA_BORDA => mistura(pano, (40, 36, 34), 0.25),
            FRISO => friso,
            FAIXA => faixa_cor,
            CASCO => casco_cor,
            CASCO_ESCURO => mistura(casco_cor, (0, 0, 0), 0.22),
            FIGURA_A => fig.0,
            FIGURA_B => fig.1,
            FIGURA_C => fig.2,
            FUNDO => (150, 64, 44),
            FERRO => if nivel_latao >= 3 { (196, 150, 64) } else { (34, 32, 34) },
            outro => outro.rgb(),
        }
    };
    let l2 = m.comp / 2;
    let w2 = m.larg / 2;
    let mastro_max = m.mastros.iter().map(|(_, h)| (m.comp as f32 * 0.95 * h) as i32).max().unwrap_or(30);
    let gurupes = m.comp / 4;
    let topo_y = m.borda + 6 + mastro_max + 4;
    let mut v = Volume::novo([-w2 - 16, -m.calado - 2, -l2 - 3], [2 * w2 + 33, topo_y + m.calado + 4, m.comp + gurupes + 10]);

    // ── the hull ──
    // Half-width and deck height along the hull (t = -1 stern .. 1 bow).
    let meia_larg = |z: i32| -> f32 {
        let t = z as f32 / l2 as f32;
        let proa = t.max(0.0);
        let popa = (-t).max(0.0);
        w2 as f32 * (1.0 - proa.powf(2.2) * 0.94) * (1.0 - popa.powi(4) * 0.32) + 0.5
    };
    let conves = |z: i32| -> i32 {
        let t = z as f32 / l2 as f32;
        m.borda + (t * t * (m.borda as f32 * 0.35)).round() as i32 + if t > 0.75 { 1 } else { 0 }
    };
    let quilha = |z: i32| -> i32 {
        let t = (z as f32 / l2 as f32).abs();
        -m.calado + ((t - 0.55).max(0.0) / 0.45 * m.calado as f32 * 0.8).round() as i32
    };
    let tombadilho_z = -l2 + (m.comp as f32 * 0.24) as i32;
    let castelo_z = l2 - (m.comp as f32 * 0.18) as i32;
    for z in -l2..=l2 {
        let hw = meia_larg(z);
        let topo = conves(z);
        let fundo = quilha(z);
        for y in fundo..=topo {
            // Narrower below the waterline, down to the keel.
            let s = if y < 0 { (y - fundo) as f32 / (-fundo).max(1) as f32 } else { 1.0 };
            let hy = if y < 0 { hw * (0.3 + 0.7 * s.sqrt()) } else { hw - if y == topo && y > 2 { 0.3 } else { 0.0 } };
            let r = hy.floor() as i32;
            for x in -r..=r {
                let borda = x.abs() == r;
                let mat = if y < 0 {
                    FUNDO
                } else if y == topo && !borda {
                    if x.rem_euclid(3) == 0 { MADEIRA } else { CONVES }
                } else if y == topo - 2 || (m.canhoes.0 > 1 && y == topo - 5) {
                    FAIXA
                } else if y == 1 {
                    CASCO_ESCURO
                } else {
                    CASCO
                };
                v.poe(x, y, z, mat);
            }
        }
        // The rail: a trim course over the deck's rim, posts under it.
        let r = (hw - 0.3).floor() as i32;
        let alto_da_amurada = if m.redondo { 2 } else { 1 };
        for x in [-r, r] {
            for k in 1..=alto_da_amurada {
                let mat = if k == alto_da_amurada { FRISO } else if z % 3 == 0 { MADEIRA } else { CASCO };
                v.poe(x, topo + k, z, mat);
            }
        }
        // The quarterdeck: a raised deck aft with its own rail.
        if m.tombadilho && z <= tombadilho_z {
            for y in topo + 1..=topo + 4 {
                for x in -r..=r {
                    let borda = x.abs() == r;
                    let mat = if y == topo + 4 && !borda { CONVES } else if y == topo + 2 { FAIXA } else { CASCO };
                    v.poe(x, y, z, mat);
                }
            }
            for x in [-r, r] {
                v.poe(x, topo + 5, z, FRISO);
            }
        }
        if m.castelo && z >= castelo_z {
            for y in topo + 1..=topo + 2 {
                for x in -r..=r {
                    v.poe(x, y, z, if y == topo + 2 && x.abs() < r { CONVES } else { CASCO });
                }
            }
            for x in [-r, r] {
                v.poe(x, topo + 3, z, FRISO);
            }
        }
    }
    // The transom: stern windows lit from inside, a lantern on its post.
    let z_popa = -l2;
    let topo_popa = conves(z_popa) + if m.tombadilho { 4 } else { 0 };
    let r_popa = (meia_larg(z_popa) - 0.3).floor() as i32;
    if m.tombadilho {
        for x in (-r_popa + 1..r_popa).step_by(2) {
            v.poe(x, topo_popa - 2, z_popa - 1, LUZ);
            v.poe(x, topo_popa - 1, z_popa - 1, FRISO);
        }
    }
    caixa(&mut v, [0, topo_popa + 1, z_popa + 1], [1, topo_popa + 5, z_popa + 2], MADEIRA);
    caixa(&mut v, [-1, topo_popa + 5, z_popa], [2, topo_popa + 7, z_popa + 3], LUZ);
    caixa(&mut v, [-1, topo_popa + 7, z_popa], [2, topo_popa + 8, z_popa + 3], FRISO);
    let lanterna = em_malha(0.0, (topo_popa + 6) as f32, (z_popa + 1) as f32);

    // Gun ports: dark squares on the sides with a barrel out, a red lid over.
    let (fileiras, por_lado) = m.canhoes;
    for f in 0..fileiras {
        for k in 0..por_lado {
            let t = if por_lado == 1 { 0.0 } else { -0.5 + k as f32 / (por_lado - 1) as f32 * 0.9 };
            let z = (t * l2 as f32) as i32;
            let y = conves(z) - 3 - f * 3;
            let r = meia_larg(z).floor() as i32;
            for s in [-1, 1] {
                v.poe(s * r, y, z, FERRO);
                v.poe(s * r, y, z + 1, FERRO);
                v.poe(s * r, y + 1, z, FUNDO);
                v.poe(s * r, y + 1, z + 1, FUNDO);
                v.poe(s * (r + 1), y, z, FERRO);
            }
        }
    }

    // ── the bowsprit and the figurehead ──
    let z_proa = l2;
    let topo_proa = conves(z_proa) + if m.castelo { 2 } else { 0 };
    let mut ponta_gurupes = (0, topo_proa, z_proa);
    for k in 0..gurupes {
        let (y, z) = (topo_proa + 1 + k / 3, z_proa - 2 + k);
        v.poe(0, y, z, MADEIRA);
        if k < gurupes / 2 {
            v.poe(0, y - 1, z, MADEIRA);
        }
        ponta_gurupes = (0, y, z);
    }
    figura_de_proa(&mut v, figura, z_proa, conves(z_proa));

    // ── masts, yards, a crow's nest ──
    let mut velas: Vec<(Mesh, Vec3, bool)> = Vec::new();
    let mut juntas = Volume::novo([-w2 - 16, 0, -l2 - 3], [2 * w2 + 33, topo_y + 4, m.comp + gurupes + 10]);
    let mut cordas: Vec<(Vec3, Vec3)> = Vec::new();
    let mut topo_principal = Vec3::ZERO;
    let principal = m.mastros.iter().enumerate().max_by(|a, b| a.1 .1.total_cmp(&b.1 .1)).map_or(0, |(k, _)| k);
    let mut primeiro_topo: Option<(i32, i32)> = None;
    for (n, (pos, alto)) in m.mastros.iter().enumerate() {
        let zm = (pos * l2 as f32) as i32;
        let base = conves(zm);
        let h = (m.comp as f32 * 0.95 * alto) as i32;
        if m.redondo && n == principal {
            caixa(&mut v, [-1, base, zm], [1, base + h, zm + 2], MADEIRA);
        } else {
            caixa(&mut v, [0, base, zm], [1, base + h, zm + 1], MADEIRA);
        }
        let topo_m = base + h;
        if n == principal {
            topo_principal = em_malha(0.0, topo_m as f32, zm as f32);
        }
        if primeiro_topo.is_none() {
            primeiro_topo = Some((topo_m, zm));
        }
        let r = meia_larg(zm).floor() as i32;
        // Shrouds: from high on the mast to the rail, both sides.
        for s in [-1.0f32, 1.0] {
            cordas.push((em_malha(0.0, (base + h * 3 / 4) as f32, zm as f32), em_malha(s * r as f32, (base + 1) as f32, (zm - 3) as f32)));
            cordas.push((em_malha(0.0, (base + h * 3 / 4) as f32, zm as f32), em_malha(s * r as f32, (base + 1) as f32, (zm + 2) as f32)));
        }
        if m.redondo {
            let meia_max = (w2 as f32 * 1.5) as i32 + 2;
            let mut vol = Volume::novo([-meia_max, base, zm], [2 * meia_max + 1, h + 2, 8]);
            // Yards and two square sails (course and topsail), and a third
            // (topgallant) on the main mast.
            let nivel = if n == principal { 3 } else { 2 };
            let alturas = [0.30f32, 0.62, 0.88, 0.99];
            let largs = [1.45f32, 1.15, 0.8, 0.5];
            for k in 0..=nivel {
                let y = base + (h as f32 * alturas[k]) as i32;
                let meia = (w2 as f32 * largs[k]) as i32;
                caixa(&mut v, [-meia, y, zm + 1], [meia + 1, y + 1, zm + 2], MADEIRA);
            }
            if n == principal && m.comp >= 60 {
                let y = base + (h as f32 * 0.55) as i32;
                caixa(&mut v, [-3, y, zm - 2], [4, y + 1, zm + 4], MADEIRA);
                for x in [-3, 3] {
                    caixa(&mut v, [x, y + 1, zm - 2], [x + 1, y + 2, zm + 4], FRISO);
                }
            }
            for k in 0..nivel {
                let y0 = base + (h as f32 * alturas[k]) as i32 + 1;
                let y1 = base + (h as f32 * alturas[k + 1]) as i32;
                let meia0 = w2 as f32 * largs[k] * 0.95;
                let meia1 = w2 as f32 * largs[k + 1] * 0.95;
                vela_quadrada(&mut vol, y0, y1, meia0, meia1, zm + 2);
                vela_quadrada(&mut juntas, y0, y1, meia0, meia1, zm + 2);
            }
            velas.push((para_mesh(malha_com(&vol, &cor)), em_malha(0.0, (base + h / 2) as f32, (zm + 1) as f32), true));
        } else {
            let comp_v = (m.comp as f32 * if m.mastros.len() == 1 { 0.5 } else { 0.32 }) as i32;
            let mut vol = Volume::novo([-3, base, zm - comp_v - 2], [7, h + comp_v / 4 + 4, comp_v + 4]);
            // A boom and a gaff aft of the mast, and the gaff sail between.
            let comp = (m.comp as f32 * if m.mastros.len() == 1 { 0.5 } else { 0.32 }) as i32;
            let y_retranca = base + 4;
            let y_carangueja = base + (h as f32 * 0.82) as i32;
            caixa(&mut v, [0, y_retranca, zm - comp], [1, y_retranca + 1, zm], MADEIRA);
            for k in 0..comp * 3 / 4 {
                v.poe(0, y_carangueja + k / 4, zm - k, MADEIRA);
            }
            vela_latina(&mut vol, zm - 1, comp, y_retranca + 1, y_carangueja, h);
            vela_latina(&mut juntas, zm - 1, comp, y_retranca + 1, y_carangueja, h);
            velas.push((para_mesh(malha_com(&vol, &cor)), em_malha(0.0, (base + h / 2) as f32, zm as f32), false));
        }
    }
    // The jib: from the foremast's top to the bowsprit's tip.
    if let Some((topo_m, zm)) = primeiro_topo {
        let alto = (topo_m - 2).min(conves(zm) + (m.comp as f32 * 0.7) as i32);
        let pe = conves(l2) + 2;
        let mut vol = Volume::novo([-3, pe - 1, zm - 1], [7, alto - pe + 4, ponta_gurupes.2 - zm + 4]);
        bujarrona(&mut vol, (alto, zm + 1), (ponta_gurupes.1, ponta_gurupes.2), pe);
        bujarrona(&mut juntas, (alto, zm + 1), (ponta_gurupes.1, ponta_gurupes.2), pe);
        velas.push((para_mesh(malha_com(&vol, &cor)), em_malha(0.0, alto as f32, zm as f32), false));
        cordas.push((em_malha(0.0, topo_m as f32, zm as f32), em_malha(0.0, ponta_gurupes.1 as f32, ponta_gurupes.2 as f32)));
    }
    // The backstay: the aftmost mast's top to the stern.
    if let Some((pos, alto)) = m.mastros.last() {
        let zm = (pos * l2 as f32) as i32;
        let topo_m = conves(zm) + (m.comp as f32 * 0.95 * alto) as i32;
        cordas.push((em_malha(0.0, topo_m as f32, zm as f32), em_malha(0.0, (topo_popa + 1) as f32, (z_popa + 1) as f32)));
    }
    // Stays between the masts.
    for par in m.mastros.windows(2) {
        let (a, b) = (par[0], par[1]);
        let (za, zb) = ((a.0 * l2 as f32) as i32, (b.0 * l2 as f32) as i32);
        let ya = conves(za) + (m.comp as f32 * 0.95 * a.1) as i32;
        let yb = conves(zb) + (m.comp as f32 * 0.95 * b.1 * 0.7) as i32;
        cordas.push((em_malha(0.0, ya as f32, za as f32), em_malha(0.0, yb as f32, zb as f32)));
    }
    let casco = para_mesh(malha_com(&v, &cor));
    let velas_juntas = para_mesh(malha_com(&juntas, &cor));
    let vertices = casco.vertices.len() + velas.iter().map(|(m, _, _)| m.vertices.len()).sum::<usize>();
    Navio3d {
        casco,
        velas,
        velas_juntas,
        cordas,
        topo: topo_principal,
        lanterna,
        meio_comp: l2 as f32 * VOX_CASCO,
        vertices,
    }
}

/// A square sail between two yards, bellied forward (+z) in the middle.
fn vela_quadrada(v: &mut Volume, y0: i32, y1: i32, meia0: f32, meia1: f32, z: i32) {
    let alto = (y1 - y0).max(1);
    for y in y0..y1 {
        let f = (y - y0) as f32 / alto as f32;
        let meia = meia0 + (meia1 - meia0) * f;
        let r = meia.floor() as i32;
        for x in -r..=r {
            let u = x as f32 / meia.max(1.0);
            // The belly in two steps (each step of a curve is a seam of
            // quads); the matrix's billow does the rest.
            let barriga = ((1.0 - u * u) * (f * std::f32::consts::PI).sin() * 2.0 - 0.3).round().clamp(0.0, 1.0) as i32;
            let borda = x.abs() == r || y == y0 || y == y1 - 1;
            v.poe(x, y, z + barriga, if borda { VELA_BORDA } else { VELA });
        }
    }
}

/// A gaff sail: aft of the mast at `zm`, `comp` long, from the boom up to
/// the gaff (rising aft), bellied to one side.
fn vela_latina(v: &mut Volume, zm: i32, comp: i32, y0: i32, y_carangueja: i32, _h: i32) {
    for k in 0..comp {
        let u = k as f32 / comp as f32;
        // The gaff's line falls aft; the sail's top follows it.
        let topo = y_carangueja + (k as f32 * 0.25) as i32 - (u * u * 6.0) as i32;
        let topo = topo.max(y0 + 2);
        for y in y0..topo {
            let f = (y - y0) as f32 / (topo - y0).max(1) as f32;
            let barriga = ((u * std::f32::consts::PI).sin() * (f * std::f32::consts::PI).sin() * 2.0 - 0.3).round().clamp(0.0, 1.0) as i32;
            let borda = k == comp - 1 || y == y0 || y == topo - 1 || k == 0;
            v.poe(barriga, y, zm - k, if borda { VELA_BORDA } else { VELA });
        }
    }
}

/// The jib: a triangle from high on the foremast (`topo` = (y, z)) to the
/// bowsprit's tip, its foot along the bow.
fn bujarrona(v: &mut Volume, topo: (i32, i32), ponta: (i32, i32), pe: i32) {
    let (y_t, z_t) = topo;
    let (y_p, z_p) = ponta;
    for z in z_t..=z_p {
        let u = (z - z_t) as f32 / (z_p - z_t).max(1) as f32;
        // The stay: from the mast top down to the tip.
        let y_cima = y_t as f32 + (y_p as f32 - y_t as f32) * u;
        let y_baixo = (pe as f32 + (y_p as f32 - pe as f32) * u.powf(3.0)).max(pe as f32);
        let (a, b) = (y_baixo.round() as i32, y_cima.round() as i32);
        for y in a..b {
            let f = (y - a) as f32 / (b - a).max(1) as f32;
            let barriga = ((u * std::f32::consts::PI).sin() * (f * std::f32::consts::PI).sin() * 2.0 - 0.6).round().clamp(0.0, 1.0) as i32;
            let borda = y == a || y == b - 1 || z == z_p;
            v.poe(barriga, y, z, if borda { VELA_BORDA } else { VELA });
        }
    }
}

/// The figurehead under the bowsprit.
fn figura_de_proa(v: &mut Volume, figura: u8, z: i32, topo: i32) {
    match figura {
        // Mermaid: a body leaning out, a tail curling under, copper hair.
        1 => {
            caixa(v, [0, topo - 4, z], [1, topo - 2, z + 2], FIGURA_B);
            caixa(v, [0, topo - 2, z + 1], [1, topo, z + 3], FIGURA_A);
            caixa(v, [0, topo, z + 2], [1, topo + 2, z + 3], FIGURA_A);
            caixa(v, [0, topo + 1, z + 1], [1, topo + 3, z + 2], FIGURA_C);
        }
        // Dragon: a green neck reaching forward, a snout, red eyes, horns.
        2 => {
            caixa(v, [0, topo - 3, z], [1, topo + 1, z + 2], FIGURA_A);
            caixa(v, [-1, topo, z + 2], [2, topo + 2, z + 5], FIGURA_A);
            caixa(v, [0, topo, z + 5], [1, topo + 1, z + 6], FIGURA_B);
            v.poe(-1, topo + 1, z + 4, FIGURA_C);
            v.poe(1, topo + 1, z + 4, FIGURA_C);
            v.poe(-1, topo + 2, z + 2, Material::PetalaBranca);
            v.poe(1, topo + 2, z + 2, Material::PetalaBranca);
        }
        // Eagle: a golden body, a white head, wings spread back along the bow.
        3 => {
            caixa(v, [0, topo - 3, z], [1, topo, z + 2], FIGURA_A);
            caixa(v, [0, topo, z + 1], [1, topo + 2, z + 3], FIGURA_B);
            v.poe(0, topo + 1, z + 3, FIGURA_C);
            for s in [-1, 1] {
                caixa(v, [s, topo - 2, z - 2], [s + 1, topo, z + 1], FIGURA_A);
                caixa(v, [s * 2, topo - 1, z - 4], [s * 2 + 1, topo + 1, z - 1], FIGURA_A);
            }
        }
        _ => {}
    }
}

// ───────────────────────────── motion and drawing ─────────────────────────────

/// What one ship has been doing: its heading a moment ago (for the heel),
/// and the trail of its stern (the wake).
struct Movimento {
    yaw: f32,
    giro: f32,
    adernar: f32,
    t: f64,
    rastro: VecDeque<(Vec2, f64)>,
}

fn ang_dif(a: f32, b: f32) -> f32 {
    let mut d = a - b;
    while d > std::f32::consts::PI {
        d -= std::f32::consts::TAU;
    }
    while d < -std::f32::consts::PI {
        d += std::f32::consts::TAU;
    }
    d
}

/// How much of the swell the water at `p` takes (shallow water near land
/// moves less: `agua::onda_de`).
fn fator_de_onda(p: Vec2) -> f32 {
    let mut raso = f32::MAX;
    for (dx, dz) in [(0.0, 0.0), (1.5, 0.0), (-1.5, 0.0), (0.0, 1.5), (0.0, -1.5)] {
        let prof = shared::terreno::NIVEL_DO_MAR - shared::mar::altura(p.x + dx, p.y + dz);
        raso = raso.min(prof);
    }
    crate::agua::onda_de(raso)
}

/// The ship's matrix on the water now: on the swell, pitched and rolled by
/// it, heeled by the turn; and the wake's bookkeeping.
fn movimento(p: Vec3, yaw: f32, semente: u64, meio_comp: f32) -> (Mat4, Vec<(Vec2, f64)>, f32) {
    let agora = get_time();
    let tempo = agora as f32;
    let q = vec2(p.x, p.z);
    let (h, g) = crate::agua::onda_em(q.x, q.y, tempo);
    let fator = fator_de_onda(q);
    let frente = vec2(yaw.sin(), yaw.cos());
    let lado = vec2(frente.y, -frente.x);
    let (adernar, rastro) = MOVIMENTO.with(|m| {
        let mut m = m.borrow_mut();
        if m.len() > 64 {
            m.retain(|_, e| agora - e.t < 5.0);
        }
        let e = m.entry(semente).or_insert(Movimento { yaw, giro: 0.0, adernar: 0.0, t: agora, rastro: VecDeque::new() });
        let dt = ((agora - e.t) as f32).clamp(1e-3, 0.25);
        let giro = ang_dif(yaw, e.yaw) / dt;
        e.giro += (giro.clamp(-3.0, 3.0) - e.giro) * (dt * 4.0).min(1.0);
        let alvo = (-e.giro * 0.09).clamp(-0.2, 0.2);
        e.adernar += (alvo - e.adernar) * (dt * 2.5).min(1.0);
        e.yaw = yaw;
        e.t = agora;
        let popa = q - frente * meio_comp * 0.9;
        // A jump (docking, sailing out, a reconnect) is not sailing: the old
        // trail would draw a line of foam across the sea.
        if e.rastro.back().is_some_and(|(u, _)| u.distance(popa) > 8.0) {
            e.rastro.clear();
        }
        if e.rastro.back().is_none_or(|(u, _)| u.distance(popa) > 0.6) {
            e.rastro.push_back((popa, agora));
        }
        while e.rastro.front().is_some_and(|(_, t)| agora - t > 3.2) {
            e.rastro.pop_front();
        }
        (e.adernar, e.rastro.iter().copied().collect::<Vec<_>>())
    });
    let arfagem = (g.dot(frente) * fator * 0.9).clamp(-0.12, 0.12);
    let balanco = (g.dot(lado) * fator * 0.9).clamp(-0.12, 0.12) + adernar + (tempo * 0.8 + semente as f32).sin() * 0.015;
    let y = crate::agua::ALTURA_DA_AGUA + h * fator + 0.02;
    let modelo = Mat4::from_translation(vec3(p.x, y, p.z))
        * Mat4::from_rotation_y(yaw)
        * Mat4::from_rotation_x(-arfagem)
        * Mat4::from_rotation_z(balanco)
        * Mat4::from_scale(Vec3::splat(ESCALA));
    (modelo, rastro, fator)
}

/// THE SHIP at `p` heading `yaw` (`x.atan2(z)`): hull, sails billowing,
/// rigging, a pennant, and its wake. `semente` = the entity's id (its own
/// motion and wake).
pub fn desenha(p: Vec3, yaw: f32, semente: u64, v: Visual) {
    let n = modelo(v);
    let (modelo_m, rastro, _) = movimento(p, yaw, semente, n.meio_comp);
    let tempo = get_time() as f32 + (semente % 97) as f32 * 0.37;
    let olho = crate::mar_cena::olho();
    let dist = olho.map_or(0.0, |o| o.distance(p));
    crate::gpu_estatica::desenha_voxel(&n.casco, modelo_m, [0.0; 4], Default::default());
    if dist > LONGE {
        crate::gpu_estatica::desenha_voxel(&n.velas_juntas, modelo_m, [0.0; 4], Default::default());
    } else {
        for (k, (m, pivo, quadrada)) in n.velas.iter().enumerate() {
            // THE BILLOW: the sail's belly breathes along the wind, about its
            // mast. One scale in the matrix, no vertex touched.
            let s = 1.0 + 0.18 * (tempo * 1.1 + k as f32 * 0.9).sin();
            let esc = if *quadrada { vec3(1.0, 1.0, s) } else { vec3(s, 1.0, 1.0) };
            let infla = Mat4::from_translation(*pivo) * Mat4::from_scale(esc) * Mat4::from_translation(-*pivo);
            crate::gpu_estatica::desenha_voxel(m, modelo_m * infla, [0.0; 4], Default::default());
        }
        let corda = Color::from_rgba(52, 40, 30, 255);
        for (a, b) in &n.cordas {
            draw_line_3d(modelo_m.transform_point3(*a), modelo_m.transform_point3(*b), corda);
        }
        // The pennant at the main mast's top, streaming ahead with the wind.
        let topo = modelo_m.transform_point3(n.topo);
        let frente = vec3(yaw.sin(), 0.0, yaw.cos());
        let pano = nv::cor_da_vela(v.vela);
        flamula(topo, frente, Color::from_rgba(pano[0], pano[1], pano[2], 255), tempo);
    }
    esteira(&rastro, dist);
}

/// A boat MOORED at a dock: a small Sloop with its sails furled, bobbing
/// gently (the harbour is sheltered).
pub fn desenha_atracado(p: Vec3, yaw: f32, tempo: f32) {
    let n = modelo(Visual::default());
    let q = vec2(p.x, p.z);
    let (h, g) = crate::agua::onda_em(q.x, q.y, tempo);
    let fator = fator_de_onda(q) * 0.5;
    let modelo_m = Mat4::from_translation(vec3(p.x, crate::agua::ALTURA_DA_AGUA + h * fator, p.z))
        * Mat4::from_rotation_y(yaw + (tempo * 0.3).sin() * 0.05)
        * Mat4::from_rotation_x(g.y * fator)
        * Mat4::from_rotation_z((tempo * 0.9).sin() * 0.03 + g.x * fator)
        * Mat4::from_scale(Vec3::splat(ESCALA));
    crate::gpu_estatica::desenha_voxel(&n.casco, modelo_m, [0.0; 4], Default::default());
}

/// The mast top of the ship at `p` now (the skull flag of a marked ship).
pub fn topo_do_mastro(p: Vec3, yaw: f32, semente: u64, v: Visual) -> Vec3 {
    let n = modelo(v);
    let q = vec2(p.x, p.z);
    let (h, _) = crate::agua::onda_em(q.x, q.y, get_time() as f32);
    let adernar = MOVIMENTO.with(|m| m.borrow().get(&semente).map_or(0.0, |e| e.adernar));
    let m = Mat4::from_translation(vec3(p.x, crate::agua::ALTURA_DA_AGUA + h * fator_de_onda(q), p.z))
        * Mat4::from_rotation_y(yaw)
        * Mat4::from_rotation_z(adernar)
        * Mat4::from_scale(Vec3::splat(ESCALA));
    m.transform_point3(n.topo)
}

/// A pennant: a long thin strip waving from `topo` along `dir`. Two-sided.
pub fn flamula(topo: Vec3, dir: Vec3, cor: Color, t: f32) {
    let lado = vec3(-dir.z, 0.0, dir.x);
    let (comp, n) = (1.6f32, 6);
    let mut verts = Vec::with_capacity(2 * (n + 1));
    let mut idx: Vec<u16> = Vec::new();
    for k in 0..=n {
        let f = k as f32 / n as f32;
        let onda = (t * 6.0 - f * 6.0).sin() * 0.18 * f;
        let p = topo + dir * (f * comp) + lado * onda;
        let alto = 0.32 * (1.0 - f * 0.8);
        let c = [(cor.r * 255.0) as u8, (cor.g * 255.0) as u8, (cor.b * 255.0) as u8, 255];
        verts.push(Vertex { position: p, uv: Vec2::ZERO, color: c, normal: Vec4::ZERO });
        verts.push(Vertex { position: p - vec3(0.0, alto, 0.0), uv: Vec2::ZERO, color: c, normal: Vec4::ZERO });
    }
    for k in 0..n as u16 {
        let (a, b, c, d) = (k * 2, k * 2 + 1, k * 2 + 2, k * 2 + 3);
        idx.extend_from_slice(&[a, b, c, b, d, c, a, c, b, b, c, d]);
    }
    draw_mesh(&Mesh { vertices: verts, indices: idx, texture: None });
}

/// THE WAKE: a broken trail of foam behind the stern and the two arms of a
/// V spreading out from it, widening and fading as they age, riding the
/// swell. A few dozen vertices; the arms drop far away.
fn esteira(rastro: &[(Vec2, f64)], dist: f32) {
    if rastro.len() < 2 || dist > 160.0 {
        return;
    }
    let agora = get_time();
    let tempo = agora as f32;
    let passo = if dist > LONGE { 2 } else { 1 };
    let pontos: Vec<(Vec2, f64)> = rastro.iter().copied().step_by(passo).collect();
    if pontos.len() < 2 {
        return;
    }
    let mut verts: Vec<Vertex> = Vec::with_capacity(pontos.len() * 9);
    let mut idx: Vec<u16> = Vec::new();
    // (distance out from the trail at birth, spread per second of age,
    //  half width, strength)
    let mut faixas = vec![(0.0f32, 0.0f32, 0.45f32, 0.55f32)];
    if dist <= LONGE {
        faixas.push((0.7, 1.5, 0.22, 0.5));
        faixas.push((-0.7, -1.5, 0.22, 0.5));
    }
    for (desvio, abre, meia, forca) in faixas {
        let base = verts.len() as u16;
        for (k, (q, t)) in pontos.iter().enumerate() {
            let idade_s = (agora - t) as f32;
            let idade = (idade_s / 3.2).clamp(0.0, 1.0);
            let prox = pontos[(k + 1).min(pontos.len() - 1)].0;
            let ant = pontos[k.saturating_sub(1)].0;
            let d = (prox - ant).normalize_or_zero();
            let lado = vec2(-d.y, d.x);
            let centro = *q + lado * (desvio + abre * idade_s);
            let larg = meia * (1.0 + idade * 1.6);
            let (h, _) = crate::agua::onda_em(centro.x, centro.y, tempo);
            let y = crate::agua::ALTURA_DA_AGUA + h + 0.07;
            let novo = 1.0 - idade;
            // Broken, not a painted stripe: the foam comes and goes along it.
            let quebra = 0.6 + 0.4 * (k as f32 * 1.9 + tempo * 2.3 + desvio).sin();
            let a = (novo * novo * forca * quebra * 255.0) as u8;
            for (s, alfa) in [(-1.0f32, 0u8), (0.0, a), (1.0, 0u8)] {
                let p = centro + lado * (s * larg);
                verts.push(Vertex { position: vec3(p.x, y, p.y), uv: Vec2::ZERO, color: [238, 247, 252, alfa], normal: Vec4::ZERO });
            }
        }
        for k in 0..(pontos.len() - 1) as u16 {
            let (a, b) = (base + k * 3, base + k * 3 + 3);
            for j in 0..2u16 {
                let (p0, p1, q0, q1) = (a + j, a + j + 1, b + j, b + j + 1);
                // Both windings: the ribbon turns with the trail.
                idx.extend_from_slice(&[p0, p1, q1, p0, q1, q0, p0, q1, p1, p0, q0, q1]);
            }
        }
    }
    draw_mesh(&Mesh { vertices: verts, indices: idx, texture: None });
}

#[cfg(test)]
mod testes {
    use super::*;

    /// Every class builds, has sails per mast and a jib, and stays inside a
    /// phone budget: the biggest look under 24k vertices (hull + sails; ten ships in view stay under ~150k).
    #[test]
    fn os_cascos_saem_bonitos_e_baratos() {
        let mut anterior = 0;
        for classe in 0..4u8 {
            let n = constroi((classe, 0, 2, 3));
            let m = medidas(classe);
            assert_eq!(n.velas.len(), m.mastros.len() + 1, "class {classe}: a sail mesh per mast and a jib");
            assert!(n.vertices < 24_000, "class {classe}: {} vertices", n.vertices);
            assert!(n.casco.vertices.len() > anterior, "class {classe} is not bigger than the one before");
            anterior = n.casco.vertices.len();
            assert!(n.topo.y > 3.0, "class {classe}: mast top at {}", n.topo.y);
            println!("class {classe}: hull {} v, sails {} v, rigging {} lines", n.casco.vertices.len(), n.vertices - n.casco.vertices.len(), n.cordas.len());
        }
    }

    #[test]
    fn o_latao_cresce_com_as_estrelas() {
        assert_eq!(latao(0), 0);
        assert_eq!(latao(15), 3);
        assert!(latao(8) >= 1 && latao(8) <= 2);
    }
}
