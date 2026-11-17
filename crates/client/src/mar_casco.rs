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
//! wake thins with distance. The switch never shows (sea fix, 17/11/2026):
//! the far sails ARE the near ones at rest, and over the last stretch
//! before `LONGE` the billow settles, the rigging and pennant fade and the
//! wake's side arms thin out, so at the switch the two draws are the same
//! pixels. A ship that comes into view (the server's area of interest)
//! fades in over `SURGE_S` instead of popping.

use std::cell::RefCell;
use std::collections::{HashMap, VecDeque};

use macroquad::models::{Mesh, Vertex};
use macroquad::prelude::*;
use crate::relogio::get_time;
use shared::navio::{self as nv, Visual};
use shared::terreno::Material;

use crate::vegetacao::{malha_com, Volume, VOX};

/// The hull GRID the sea's props are modelled on (wreckage, the drifting
/// chest: `vegetacao::DESTROCO_ESCALA`). Since the walkable deck each class
/// has its own voxel (`shared::conves::Medida::vox`, 0.15-0.25 u, `escala`);
/// the meshes come out at `VOX` (0.25) a voxel and the matrix scales them.
pub(crate) const VOX_CASCO: f32 = 0.125;

/// Past this (from the eye) a ship is hull + one sail mesh, no rigging.
const LONGE: f32 = 55.0;
/// Where the near detail starts settling towards the far draw.
const ASSENTA: f32 = 38.0;
/// A ship that comes into view fades in over this, seconds.
const SURGE_S: f64 = 0.6;

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

/// The measures of a class, in hull voxels. Length, beam, draught,
/// freeboard, masts and the raised decks come from `shared::conves::MEDIDAS`
/// (the walkable deck, 17/11/2026): the deck the server walks bodies on is
/// this mesh's deck. Each class has its own voxel size (`escala`), so the
/// big hulls stay a phone's mesh.
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
    let m = shared::conves::medida(classe as usize);
    let (redondo, canhoes, casco, faixa) = match classe {
        0 => (false, (0, 0), (120, 80, 46), (226, 208, 160)),
        1 => (false, (1, 3), (104, 66, 38), (236, 236, 228)),
        2 => (true, (1, 5), (92, 54, 32), (176, 52, 44)),
        _ => (true, (2, 6), (40, 36, 34), (222, 178, 64)),
    };
    Medidas {
        comp: m.comp,
        larg: m.larg,
        calado: m.calado,
        borda: m.borda,
        mastros: m.mastros,
        redondo,
        tombadilho: m.tombadilho,
        castelo: m.castelo,
        canhoes,
        casco,
        faixa,
    }
}

/// Mesh units → world units for a class: its voxel over the mesher's.
pub fn escala(classe: u8) -> f32 {
    shared::conves::medida(classe as usize).vox / VOX
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
    /// units), whether it billows along z (square sail) or x (gaff, jib), and
    /// the cloth's shader data (`Pano`).
    velas: Vec<(Mesh, Vec3, bool, Pano)>,
    pub classe: u8,
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

/// What the world shader needs to make a sail's CLOTH move (`VelaA`/`VelaB`,
/// `render3d::SOLIDO_VERTICE`): square sails hang from their yards (the
/// heights of the yards each sail hangs from, mesh units, bottom to top;
/// unused = far above); fore-and-aft sails and the jib are bent on a mast or
/// stay (boom/foot height, the mast's z, the sail's length).
#[derive(Clone, Copy)]
struct Pano {
    quadrada: bool,
    b: [f32; 4],
}

thread_local! {
    /// Whether a ship coming into view fades in (`sem_surgir`).
    static SURGE: std::cell::Cell<bool> = const { std::cell::Cell::new(true) };
    static MODELOS: RefCell<HashMap<Chave, &'static Navio3d>> = RefCell::new(HashMap::new());
    static MOVIMENTO: RefCell<HashMap<u64, Movimento>> = RefCell::new(HashMap::new());
}

/// Draws `f` with ships appearing at once, no fade-in: the scene builders
/// (previews, the benchmark, the ad capture), whose ships are all there
/// from the first frame.
pub fn sem_surgir<R>(f: impl FnOnce() -> R) -> R {
    let antes = SURGE.with(|s| s.replace(false));
    let r = f();
    SURGE.with(|s| s.set(antes));
    r
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
    // The SAME curves the server walks bodies on (`shared::conves`).
    let sm = shared::conves::medida(classe as usize);
    let meia_larg = |z: i32| -> f32 { shared::conves::meia_larg_vox(sm, z as f32) };
    let conves = |z: i32| -> i32 { shared::conves::conves_vox(sm, z) };
    let quilha = |z: i32| -> i32 {
        let t = (z as f32 / l2 as f32).abs();
        -m.calado + ((t - 0.55).max(0.0) / 0.45 * m.calado as f32 * 0.8).round() as i32
    };
    let tombadilho_z = shared::conves::tombadilho_z(sm);
    let castelo_z = shared::conves::castelo_z(sm);
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
    let mut velas: Vec<(Mesh, Vec3, bool, Pano)> = Vec::new();
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
            let mut vergas = [1.0e5f32; 4];
            for k in 0..nivel {
                let y0 = base + (h as f32 * alturas[k]) as i32 + 1;
                let y1 = base + (h as f32 * alturas[k + 1]) as i32;
                let meia0 = w2 as f32 * largs[k] * 0.95;
                let meia1 = w2 as f32 * largs[k + 1] * 0.95;
                vela_quadrada(&mut vol, y0, y1, meia0, meia1, zm + 2);
                vela_quadrada(&mut juntas, y0, y1, meia0, meia1, zm + 2);
                // Each sail hangs from the yard over it.
                vergas[k.min(3)] = y1 as f32 * VOX;
            }
            velas.push((
                para_mesh(malha_com(&vol, &cor)),
                em_malha(0.0, (base + h / 2) as f32, (zm + 1) as f32),
                true,
                Pano { quadrada: true, b: vergas },
            ));
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
            velas.push((
                para_mesh(malha_com(&vol, &cor)),
                em_malha(0.0, (base + h / 2) as f32, zm as f32),
                false,
                Pano { quadrada: false, b: [(y_retranca + 1) as f32 * VOX, zm as f32 * VOX, comp as f32 * VOX, 0.0] },
            ));
        }
    }
    // The jib: from the foremast's top to the bowsprit's tip.
    if let Some((topo_m, zm)) = primeiro_topo {
        let alto = (topo_m - 2).min(conves(zm) + (m.comp as f32 * 0.7) as i32);
        let pe = conves(l2) + 2;
        let mut vol = Volume::novo([-3, pe - 1, zm - 1], [7, alto - pe + 4, ponta_gurupes.2 - zm + 4]);
        bujarrona(&mut vol, (alto, zm + 1), (ponta_gurupes.1, ponta_gurupes.2), pe);
        bujarrona(&mut juntas, (alto, zm + 1), (ponta_gurupes.1, ponta_gurupes.2), pe);
        velas.push((
            para_mesh(malha_com(&vol, &cor)),
            em_malha(0.0, alto as f32, zm as f32),
            false,
            Pano { quadrada: false, b: [pe as f32 * VOX, (zm + 1) as f32 * VOX, (ponta_gurupes.2 - zm).max(1) as f32 * VOX, 1.0] },
        ));
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
    // THE SHIP'S WHEEL (the walkable deck's helm): a post and a spoked
    // wheel facing aft, where `shared::conves::roda` says.
    {
        let zr = (shared::conves::roda(classe as usize).y / sm.vox).round() as i32;
        let piso = shared::conves::piso_vox(sm, zr);
        caixa(&mut v, [0, piso, zr], [1, piso + 4, zr + 1], MADEIRA);
        let (cy, cz) = (piso + 5, zr - 1);
        for (dx, dy) in [(-2, 0), (2, 0), (0, -2), (0, 2), (-1, -1), (1, -1), (-1, 1), (1, 1), (-2, 1), (2, -1), (-2, -1), (2, 1), (-1, 2), (1, -2), (-1, -2), (1, 2)] {
            v.poe(dx, cy + dy, cz, MADEIRA);
        }
        v.poe(0, cy, cz, FRISO);
        for (dx, dy) in [(-3, 0), (3, 0), (0, 3), (0, -3)] {
            v.poe(dx, cy + dy, cz, FRISO);
        }
    }
    let casco = para_mesh(malha_com(&v, &cor));
    let velas_juntas = para_mesh(malha_com(&juntas, &cor));
    let vertices = casco.vertices.len() + velas.iter().map(|(m, _, _, _)| m.vertices.len()).sum::<usize>();
    Navio3d {
        classe,
        casco,
        velas,
        velas_juntas,
        cordas,
        topo: topo_principal,
        lanterna,
        meio_comp: l2 as f32 * shared::conves::medida(classe as usize).vox,
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
/// its speed (the sails), and the trail of its stern (the wake).
struct Movimento {
    yaw: f32,
    giro: f32,
    adernar: f32,
    t: f64,
    /// When it came into view (the fade-in, `SURGE_S`).
    nasceu: f64,
    rastro: VecDeque<(Vec2, f64)>,
    /// Where it was last frame and its smoothed speed (u/s): the sails read
    /// the ship's VELOCITY, so everyone sees the same (`vela`).
    q: Vec2,
    vel: f32,
    /// 0 = sails furled on the yards, 1 = set and drawing.
    vela: f32,
}

/// Under this speed (u/s) a ship's sails are furled; over `VELA_CHEIA` they
/// are set. In between, partly.
pub const VELA_RECOLHIDA: f32 = 0.6;
pub const VELA_CHEIA: f32 = 2.5;
/// Seconds for the sails to go from furled to set (and back).
pub const VELA_S: f32 = 1.4;

/// How set the sails should be at `vel` u/s (0 furled .. 1 set).
pub fn vela_alvo(vel: f32) -> f32 {
    let t = ((vel - VELA_RECOLHIDA) / (VELA_CHEIA - VELA_RECOLHIDA)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// One step of the sails toward `alvo`, eased over `VELA_S`.
pub fn passo_da_vela(atual: f32, alvo: f32, dt: f32) -> f32 {
    let passo = dt / VELA_S;
    atual + (alvo - atual).clamp(-passo, passo)
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

/// The hull's matrix for a heel `adernar`, at time `tempo`: on the swell,
/// pitched and rolled by it. The same for the draw and for the bodies on
/// its deck (`matriz_do_casco`), so they stand ON the planks.
fn compoe(p: Vec3, yaw: f32, adernar: f32, semente: u64, classe: u8, tempo: f32) -> Mat4 {
    let q = vec2(p.x, p.z);
    let (h, g) = crate::agua::onda_em(q.x, q.y, tempo);
    let fator = fator_de_onda(q);
    let frente = vec2(yaw.sin(), yaw.cos());
    let lado = vec2(frente.y, -frente.x);
    // A long hull rides the swell steadier.
    let calma = (shared::conves::meio_comp(0) / shared::conves::meio_comp(classe as usize)).sqrt();
    let arfagem = (g.dot(frente) * fator * 0.9 * calma).clamp(-0.12, 0.12);
    let balanco = (g.dot(lado) * fator * 0.9 * calma).clamp(-0.12, 0.12) + adernar + (tempo * 0.8 + semente as f32).sin() * 0.015;
    let y = crate::agua::ALTURA_DA_AGUA + h * fator + 0.02;
    Mat4::from_translation(vec3(p.x, y, p.z))
        * Mat4::from_rotation_y(yaw)
        * Mat4::from_rotation_x(-arfagem)
        * Mat4::from_rotation_z(balanco)
        * Mat4::from_scale(Vec3::splat(escala(classe)))
}

/// The matrix of the ship `semente` (class `classe`) at `p` heading `yaw`,
/// NOW, without touching its motion's bookkeeping: what places the bodies
/// walking on its deck (`world::World::tick`).
pub fn matriz_do_casco(p: Vec3, yaw: f32, semente: u64, classe: u8) -> Mat4 {
    let adernar = MOVIMENTO.with(|m| m.borrow().get(&semente).map_or(0.0, |e| e.adernar));
    compoe(p, yaw, adernar, semente, classe, get_time() as f32)
}

/// A deck place (`shared::conves`, units: `x` across, `y` along) on the
/// hull of class `classe` drawn with matrix `m`: the world point of the
/// planks there (the quarterdeck and forecastle included).
pub fn ponto_do_conves(m: Mat4, classe: u8, local: Vec2) -> Vec3 {
    let sm = shared::conves::medida(classe as usize);
    let alto = shared::conves::altura(classe as usize, ::glam::Vec2::new(local.x, local.y)) / sm.vox;
    // The mesher writes voxel `x` over [x, x+1] (`em_malha`): the keel line
    // is half a voxel off the mesh's origin.
    m.transform_point3(vec3((local.x / sm.vox + 0.5) * VOX, alto * VOX, (local.y / sm.vox + 0.5) * VOX))
}

/// How set the sails of ship `semente` are now (0 furled .. 1 set).
pub fn vela_de(semente: u64) -> f32 {
    MOVIMENTO.with(|m| m.borrow().get(&semente).map_or(0.0, |e| e.vela))
}

/// The ship's matrix on the water now, and the bookkeeping: the heel, the
/// speed and the sails, the wake. `vel_fio` = the entity's speed on the
/// wire (u/s), what the sails follow; `None` = a ship shown under sail (the
/// Shipyard's model, a preview's fleet).
fn movimento(p: Vec3, yaw: f32, semente: u64, meio_comp: f32, classe: u8, vel_fio: Option<f32>) -> (Mat4, Vec<(Vec2, f64)>, f32, f32) {
    let agora = get_time();
    let tempo = agora as f32;
    let q = vec2(p.x, p.z);
    let frente = vec2(yaw.sin(), yaw.cos());
    let (adernar, rastro, vel, vela) = MOVIMENTO.with(|m| {
        let mut m = m.borrow_mut();
        if m.len() > 64 {
            m.retain(|_, e| agora - e.t < 5.0);
        }
        let e = m.entry(semente).or_insert(Movimento {
            yaw,
            giro: 0.0,
            adernar: 0.0,
            t: agora,
            nasceu: agora,
            rastro: VecDeque::new(),
            q,
            vel: vel_fio.unwrap_or(VELA_CHEIA),
            vela: vela_alvo(vel_fio.unwrap_or(VELA_CHEIA)),
        });
        // Out of view for a while (left the area of interest and came
        // back): it fades in again, its sails as its speed says.
        if agora - e.t > 0.5 {
            e.nasceu = agora;
            e.q = q;
            e.vel = vel_fio.unwrap_or(VELA_CHEIA);
            e.vela = vela_alvo(e.vel);
        }
        let dt = ((agora - e.t) as f32).clamp(1e-3, 0.25);
        // The heel follows the TURN (turning inertia, 16/11/2026): the
        // server turns a hull at most `navio::giro_max` (1.5 rad/s for a
        // Sloop, 0.7 for a Man-o'-War), so a nimble Sloop leans ~9° in a
        // hard turn and a Man-o'-War ~4°, both easing in and out slowly.
        let giro = ang_dif(yaw, e.yaw) / dt;
        e.giro += (giro.clamp(-2.0, 2.0) - e.giro) * (dt * 3.0).min(1.0);
        let alvo = (-e.giro * 0.105).clamp(-0.2, 0.2);
        e.adernar += (alvo - e.adernar) * (dt * 2.0).min(1.0);
        e.yaw = yaw;
        // THE SAILS follow the speed (owner, 17/11/2026: "open and close the
        // sail when the ship walks / stops"): a jump (docking, a reconnect)
        // is not speed.
        let passo = q.distance(e.q);
        let v = match vel_fio {
            Some(v) => v,
            None => VELA_CHEIA.max(if passo > 8.0 { e.vel } else { passo / dt }),
        };
        e.vel += (v - e.vel) * (dt * 4.0).min(1.0);
        e.q = q;
        e.vela = passo_da_vela(e.vela, vela_alvo(e.vel), dt);
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
        (e.adernar, e.rastro.iter().copied().collect::<Vec<_>>(), e.vel, e.vela)
    });
    (compoe(p, yaw, adernar, semente, classe, tempo), rastro, vel, vela)
}

/// The FURL of one sail on the matrix: a square sail rolls up toward its
/// yards (the shader rolls each onto its own), a fore-and-aft sail and the
/// jib come down onto the boom / the foot. Zero vertex work on the CPU.
fn recolhe(pano: &Pano, vela: f32) -> f32 {
    let _ = pano;
    (1.0 - vela).clamp(0.0, 1.0)
}

/// THE SHIP at `p` heading `yaw` (`x.atan2(z)`): hull, sails, rigging, a
/// pennant, and its wake. `semente` = the entity's id (its own motion and
/// wake). The sails are furled lying still and set under way, and their
/// cloth ripples in the wind (`render3d::SOLIDO_VERTICE`, `VelaA`/`VelaB`).
pub fn desenha(p: Vec3, yaw: f32, semente: u64, v: Visual) {
    desenha_navio(p, yaw, semente, v, None);
}

/// The same with the ship's speed on the wire (u/s): the sails furl lying
/// still and unfurl under way (owner, 17/11/2026: "open and close the sail
/// when the ship walks / stops"), the same for everyone watching.
pub fn desenha_navio(p: Vec3, yaw: f32, semente: u64, v: Visual, vel_fio: Option<f32>) {
    let n = modelo(v);
    let (modelo_m, rastro, vel, vela) = movimento(p, yaw, semente, n.meio_comp, n.classe, vel_fio);
    let tempo = get_time() as f32 + (semente % 97) as f32 * 0.37;
    let olho = crate::mar_cena::olho();
    let dist = olho.map_or(0.0, |o| o.distance(p));
    // Just came into view: fade in (an ordered dither, `gpu_estatica`).
    let idade = MOVIMENTO.with(|m| m.borrow().get(&semente).map_or(SURGE_S, |e| get_time() - e.nasceu));
    let surge = if SURGE.with(|s| s.get()) { (idade / SURGE_S).clamp(0.0, 1.0) as f32 } else { 1.0 };
    let antes = crate::gpu_estatica::some_atual();
    crate::gpu_estatica::define_some(1.0 - (1.0 - antes) * surge);
    crate::gpu_estatica::desenha_voxel(&n.casco, modelo_m, [0.0; 4], Default::default());
    // 1 = the near ship's full detail, 0 = settled into the far draw.
    let perto = crate::mar_cena::presenca(dist, ASSENTA, LONGE);
    if perto <= 0.0 {
        // Far: all sails in one mesh, when set.
        if vela > 0.5 {
            let offset = Mat4::from_translation(vec3(0.0, 0.015, 0.0));
            crate::gpu_estatica::desenha_voxel(&n.velas_juntas, modelo_m * offset, [0.0; 4], Default::default());
        }
    } else {
        // Faster = more wind in the cloth.
        let vento = (vel / nv::VELOCIDADE_MAX).clamp(0.0, 1.0);
        for (k, (m, pivo, quadrada, pano)) in n.velas.iter().enumerate() {
            let recolhida = recolhe(pano, vela);
            if recolhida > 0.995 && !pano.quadrada {
                // A fore-and-aft sail all the way down is a roll on the boom:
                // the furl scale draws it, nothing to skip.
            }
            // THE BILLOW: the sail's belly breathes along the wind, about
            // its mast, only while it draws. One scale in the matrix.
            let s = 1.0 + 0.18 * perto * vela * (tempo * 1.1 + k as f32 * 0.9).sin();
            let esc = if *quadrada { vec3(1.0, 1.0, s) } else { vec3(s, 1.0, 1.0) };
            let infla = Mat4::from_translation(*pivo) * Mat4::from_scale(esc) * Mat4::from_translation(-*pivo);
            // A tiny offset to separate sails from the hull and avoid z-fighting.
            let offset = Mat4::from_translation(vec3(0.0, 0.015, 0.0));
            // THE CLOTH (owner: "the sail needs a movement animation, like
            // wind in it"): ripples running across it, luffing at the free
            // edges, stronger fast; and the furl, per sail on its yard.
            let amp = (0.05 + 0.10 * vento) * perto * vela;
            let a = [if pano.quadrada { 1.0 } else { 2.0 }, recolhida, amp, tempo + k as f32 * 1.7];
            crate::gpu_estatica::com_vela(a, pano.b, || {
                crate::gpu_estatica::desenha_voxel(m, modelo_m * offset * infla, [0.0; 4], Default::default());
            });
        }
        let corda = [52u8, 40, 30, (255.0 * perto * surge) as u8];
        cordame(&n.cordas, modelo_m, corda, olho.unwrap_or(p + vec3(0.0, 20.0, 0.0)));
        // The pennant at the main mast's top, streaming ahead with the wind;
        // it shortens away as the ship settles into the far draw.
        let topo = modelo_m.transform_point3(n.topo);
        let frente = vec3(yaw.sin(), 0.0, yaw.cos());
        let pano = nv::cor_da_vela(v.vela);
        flamula(topo, frente * (perto * surge), Color::from_rgba(pano[0], pano[1], pano[2], 255), tempo);
    }
    crate::gpu_estatica::define_some(antes);
    esteira(&rastro, dist, surge);
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
        * Mat4::from_scale(Vec3::splat(escala(0)));
    crate::gpu_estatica::desenha_voxel(&n.casco, modelo_m, [0.0; 4], Default::default());
}

/// The mast top of the ship at `p` now (the skull flag of a marked ship).
pub fn topo_do_mastro(p: Vec3, yaw: f32, semente: u64, v: Visual) -> Vec3 {
    let n = modelo(v);
    matriz_do_casco(p, yaw, semente, n.classe).transform_point3(n.topo)
}

/// THE RIGGING: each rope a thin ribbon turned to the eye, all in one
/// mesh. (They were `draw_line_3d`, which under the world's material drew
/// as TRIANGLES between consecutive ropes: the big dark sheets between the
/// masts and the bowsprit — visible since the hulls grew, 17/11/2026.)
fn cordame(cordas: &[(Vec3, Vec3)], m: Mat4, cor: [u8; 4], olho: Vec3) {
    if cordas.is_empty() || cor[3] == 0 {
        return;
    }
    let mut verts = Vec::with_capacity(cordas.len() * 4);
    let mut idx: Vec<u16> = Vec::with_capacity(cordas.len() * 12);
    for (a, b) in cordas {
        let (a, b) = (m.transform_point3(*a), m.transform_point3(*b));
        let meio = (a + b) * 0.5;
        let lado = (b - a).cross(olho - meio).normalize_or_zero() * 0.035;
        let k = verts.len() as u16;
        for p in [a - lado, a + lado, b + lado, b - lado] {
            verts.push(Vertex { position: p, uv: Vec2::ZERO, color: cor, normal: Vec4::ZERO });
        }
        idx.extend_from_slice(&[k, k + 1, k + 2, k, k + 2, k + 3, k, k + 2, k + 1, k, k + 3, k + 2]);
    }
    draw_mesh(&Mesh { vertices: verts, indices: idx, texture: None });
}

/// A pennant: a long thin strip waving from `topo` along `dir`. Two-sided.
pub fn flamula(topo: Vec3, dir: Vec3, cor: Color, t: f32) {
    if dir.length_squared() < 1e-4 {
        return;
    }
    let lado = vec3(-dir.z, 0.0, dir.x).normalize_or_zero();
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
fn esteira(rastro: &[(Vec2, f64)], dist: f32, surge: f32) {
    // The whole wake fades out from 120 u, its side arms as the ship
    // settles into the far draw.
    let toda = crate::mar_cena::presenca(dist, 120.0, 160.0) * surge;
    let bracos = crate::mar_cena::presenca(dist, ASSENTA, LONGE);
    if rastro.len() < 2 || toda <= 0.0 {
        return;
    }
    let agora = get_time();
    let tempo = agora as f32;
    let passo = if dist > 120.0 { 2 } else { 1 };
    let pontos: Vec<(Vec2, f64)> = rastro.iter().copied().step_by(passo).collect();
    if pontos.len() < 2 {
        return;
    }
    let mut verts: Vec<Vertex> = Vec::with_capacity(pontos.len() * 9);
    let mut idx: Vec<u16> = Vec::new();
    // (distance out from the trail at birth, spread per second of age,
    //  half width, strength)
    let mut faixas = vec![(0.0f32, 0.0f32, 0.45f32, 0.55f32 * toda)];
    if bracos > 0.0 {
        faixas.push((0.7, 1.5, 0.22, 0.5 * bracos * toda));
        faixas.push((-0.7, -1.5, 0.22, 0.5 * bracos * toda));
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
            // Bigger hulls use a coarser voxel (the walkable deck): it is the
            // SHIP that grows class by class, not the mesh.
            let comp = (n.meio_comp * 100.0) as usize;
            assert!(comp > anterior, "class {classe} is not bigger than the one before");
            anterior = comp;
            assert!(n.topo.y > 3.0, "class {classe}: mast top at {}", n.topo.y);
            println!("class {classe}: hull {} v, sails {} v, rigging {} lines", n.casco.vertices.len(), n.vertices - n.casco.vertices.len(), n.cordas.len());
        }
    }

    /// THE SAILS FOLLOW THE SPEED (17/11/2026): furled lying still, set
    /// under way, partly in between, and the change eases over `VELA_S`.
    #[test]
    fn as_velas_recolhem_parado_e_soltam_andando() {
        assert_eq!(vela_alvo(0.0), 0.0);
        assert_eq!(vela_alvo(VELA_RECOLHIDA), 0.0);
        assert_eq!(vela_alvo(nv::VELOCIDADE_MAX), 1.0);
        let meio = vela_alvo((VELA_RECOLHIDA + VELA_CHEIA) * 0.5);
        assert!(meio > 0.3 && meio < 0.7);
        // From furled to set takes VELA_S, smoothly.
        let mut v = 0.0;
        let mut t = 0.0;
        while v < 1.0 {
            let antes = v;
            v = passo_da_vela(v, 1.0, 1.0 / 60.0);
            assert!(v - antes <= 1.0 / 60.0 / VELA_S + 1e-6);
            t += 1.0 / 60.0;
        }
        assert!((t - VELA_S).abs() < 0.05, "unfurling took {t} s");
        // A ship shown with no speed (the Shipyard) sails set.
        assert_eq!(vela_alvo(VELA_CHEIA), 1.0);
    }

    /// The deck the bodies walk on is the hull's deck: a deck point lands
    /// on the planks' height of the class (the quarterdeck higher).
    #[test]
    fn o_conves_do_modelo_e_o_do_servidor() {
        for classe in 0..4u8 {
            let m = compoe(Vec3::ZERO, 0.0, 0.0, 0, classe, 0.0);
            let meio = ponto_do_conves(m, classe, vec2(0.0, 0.0));
            let popa = ponto_do_conves(m, classe, vec2(0.0, shared::conves::leme(classe as usize).y));
            let alto = shared::conves::altura(classe as usize, ::glam::Vec2::ZERO);
            assert!((meio.y - (crate::agua::ALTURA_DA_AGUA + alto)).abs() < 0.6, "class {classe}: deck at {}", meio.y);
            assert!(popa.y >= meio.y - 0.05, "class {classe}: the helm below the deck");
            // Forward on deck is +Z of the model (yaw 0).
            let proa = ponto_do_conves(m, classe, vec2(0.0, 2.0));
            assert!(proa.z > meio.z + 1.5);
        }
    }

    #[test]
    fn o_latao_cresce_com_as_estrelas() {
        assert_eq!(latao(0), 0);
        assert_eq!(latao(15), 3);
        assert!(latao(8) >= 1 && latao(8) <= 2);
    }
}
