//! THE WALKABLE DECK (docs/NAVEGACAO.md, "Convés andável", 17/11/2026).
//!
//! The ship is its own entity at sea and the players aboard walk on it. A
//! body aboard keeps its position in DECK space — `x` across (to starboard),
//! `y` along (to the bow), units, origin at the ship's position — and its
//! world position is `ship × local`, every tick (`do_mundo`).
//!
//! The deck of each class is the SAME curve the client's voxel hull is built
//! from (`MEDIDAS`, `meia_larg_vox`), less the rail and the body's radius:
//! what you see is where you can walk. Masts and the wheel are obstacles.
//! Client and server read this table.

use glam::Vec2;
use serde::{Deserialize, Serialize};

use crate::components::EntityId;

/// The measures of a hull class, in hull voxels.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Medida {
    /// Length, voxels.
    pub comp: i32,
    /// Beam, voxels.
    pub larg: i32,
    /// Below the waterline, voxels.
    pub calado: i32,
    /// Freeboard (deck height amidships), voxels.
    pub borda: i32,
    /// The voxel's size, units: the bigger hulls use a coarser voxel so the
    /// mesh stays a phone's size (`mar_casco::os_cascos_saem_bonitos_e_baratos`).
    pub vox: f32,
    /// (position along the hull −1 stern..1 bow, height × length).
    pub mastros: &'static [(f32, f32)],
    /// Raised deck aft (the helm stands on it) and forward.
    pub tombadilho: bool,
    pub castelo: bool,
}

/// Sloop, Schooner, Galleon, Man-o'-War (`navio::CASCOS` order).
///
/// | Class | Hull (length × beam) | Walkable deck |
/// |---|---|---|
/// | Sloop | 7.2 × 3.0 u | ~6 × 2.5 u |
/// | Schooner | 10.8 × 4.0 u | ~9 × 3.5 u |
/// | Galleon | 15.5 × 5.5 u | ~13 × 5 u |
/// | Man-o'-War | 20.2 × 6.2 u | ~17 × 6 u |
pub const MEDIDAS: [Medida; 4] = [
    Medida {
        comp: 48,
        larg: 20,
        calado: 4,
        borda: 5,
        vox: 0.15,
        mastros: &[(0.12, 1.0)],
        tombadilho: false,
        castelo: false,
    },
    Medida {
        comp: 60,
        larg: 22,
        calado: 5,
        borda: 6,
        vox: 0.18,
        mastros: &[(0.32, 0.92), (-0.14, 1.02)],
        tombadilho: false,
        castelo: false,
    },
    Medida {
        comp: 74,
        larg: 26,
        calado: 6,
        borda: 7,
        vox: 0.21,
        mastros: &[(0.36, 0.86), (0.02, 1.0), (-0.34, 0.78)],
        tombadilho: true,
        castelo: true,
    },
    Medida {
        comp: 72,
        larg: 22,
        calado: 5,
        borda: 7,
        vox: 0.28,
        mastros: &[(0.38, 0.86), (0.02, 1.0), (-0.34, 0.8)],
        tombadilho: true,
        castelo: true,
    },
];

pub fn medida(classe: usize) -> &'static Medida {
    &MEDIDAS[classe.min(MEDIDAS.len() - 1)]
}

/// A body's radius on deck (the land one).
pub const RAIO_DO_CORPO: f32 = crate::constants::ENTITY_RADIUS;
/// A mast's radius, units.
pub const RAIO_DO_MASTRO: f32 = 0.28;
/// The wheel's radius as an obstacle, units.
pub const RAIO_DA_RODA: f32 = 0.22;
/// From how far the captain can take the helm, units.
pub const ALCANCE_DO_LEME: f32 = 1.6;
/// How close a party member's own ship must be to board, units.
pub const ALCANCE_DO_EMBARQUE: f32 = 14.0;
/// Deck positions on the wire: 1/32 unit.
pub const ESCALA_LOCAL: f32 = 32.0;

/// Half the beam (voxels) at `z` voxels along the hull — the hull mesh's own
/// curve: a fine bow, a slightly drawn-in stern.
pub fn meia_larg_vox(m: &Medida, z: f32) -> f32 {
    let l2 = (m.comp / 2) as f32;
    let w2 = (m.larg / 2) as f32;
    let t = z / l2;
    let proa = t.max(0.0);
    let popa = (-t).max(0.0);
    w2 * (1.0 - proa.powf(2.2) * 0.94) * (1.0 - popa.powi(4) * 0.32) + 0.5
}

/// The deck's height (voxels, the top of the deck voxel) at `z` voxels: the
/// sheer rising fore and aft, the quarterdeck and the forecastle.
pub fn conves_vox(m: &Medida, z: i32) -> i32 {
    let l2 = m.comp / 2;
    let t = z as f32 / l2 as f32;
    m.borda + (t * t * (m.borda as f32 * 0.35)).round() as i32 + if t > 0.75 { 1 } else { 0 }
}

/// Where the quarterdeck ends (voxels, `z <= this`) and the forecastle
/// starts (`z >= this`).
pub fn tombadilho_z(m: &Medida) -> i32 {
    -m.comp / 2 + (m.comp as f32 * 0.24) as i32
}
pub fn castelo_z(m: &Medida) -> i32 {
    m.comp / 2 - (m.comp as f32 * 0.18) as i32
}

/// The walking surface (voxels above the waterline) at `z` voxels.
pub fn piso_vox(m: &Medida, z: i32) -> i32 {
    let base = conves_vox(m, z) + 1;
    if m.tombadilho && z <= tombadilho_z(m) {
        base + 4
    } else if m.castelo && z >= castelo_z(m) {
        base + 2
    } else {
        base
    }
}

/// The deck's height under `local`, units above the waterline.
pub fn altura(classe: usize, local: Vec2) -> f32 {
    let m = medida(classe);
    piso_vox(m, (local.y / m.vox).floor() as i32) as f32 * m.vox
}

/// Half the beam, units (a sea mob holds off the side at this + a bit).
pub fn meia_boca(classe: usize) -> f32 {
    let m = medida(classe);
    (m.larg / 2) as f32 * m.vox
}

/// Half the length, units.
pub fn meio_comp(classe: usize) -> f32 {
    let m = medida(classe);
    (m.comp / 2) as f32 * m.vox
}

/// The masts' positions on deck.
pub fn mastros(classe: usize) -> impl Iterator<Item = Vec2> {
    let m = medida(classe);
    let l2 = (m.comp / 2) as f32;
    m.mastros
        .iter()
        .map(move |(pos, _)| Vec2::new(0.0, (pos * l2).trunc() * m.vox))
}

/// The ship's WHEEL, on deck: at the stern (on the quarterdeck of the big
/// two).
pub fn roda(classe: usize) -> Vec2 {
    let m = medida(classe);
    let l2 = m.comp / 2;
    let z = if m.tombadilho {
        -l2 + (m.comp as f32 * 0.12) as i32
    } else {
        -l2 + 6
    };
    Vec2::new(0.0, z as f32 * m.vox)
}

/// Where the helmsman stands: just aft of the wheel, facing the bow.
pub fn leme(classe: usize) -> Vec2 {
    roda(classe) - Vec2::new(0.0, 0.65)
}

/// Half the walkable width at `y` along the deck (units, body centre), or
/// `None` past the bow or the stern.
pub fn meia_largura_andavel(classe: usize, y: f32) -> Option<f32> {
    let m = medida(classe);
    let z = y / m.vox;
    let l2 = (m.comp / 2) as f32;
    if z.abs() > l2 - 1.5 {
        return None;
    }
    let hw = (meia_larg_vox(m, z) - 1.3) * m.vox - RAIO_DO_CORPO;
    (hw > 0.05).then_some(hw)
}

/// Does a body fit at `local`? Inside the rails, off the masts and the wheel.
pub fn cabe(classe: usize, local: Vec2) -> bool {
    let Some(hw) = meia_largura_andavel(classe, local.y) else {
        return false;
    };
    if local.x.abs() > hw {
        return false;
    }
    if mastros(classe).any(|m| m.distance(local) < RAIO_DO_MASTRO + RAIO_DO_CORPO) {
        return false;
    }
    roda(classe).distance(local) >= RAIO_DA_RODA + RAIO_DO_CORPO
}

/// One step on deck: the whole step if it fits, else slide along the axis
/// that does (the rail is a wall), else stay.
pub fn mover(classe: usize, local: Vec2, passo: Vec2) -> Vec2 {
    if !passo.is_finite() {
        return local;
    }
    for p in [
        local + passo,
        local + Vec2::new(passo.x, 0.0),
        local + Vec2::new(0.0, passo.y),
    ] {
        if cabe(classe, p) {
            return p;
        }
    }
    local
}

/// The nearest spot on deck to `local` that fits (boarding, a stale save).
pub fn ponto_livre(classe: usize, local: Vec2) -> Vec2 {
    if cabe(classe, local) {
        return local;
    }
    let mut melhor = leme(classe);
    let mut d_melhor = f32::MAX;
    let comp = meio_comp(classe);
    let mut y = -comp;
    while y <= comp {
        if let Some(hw) = meia_largura_andavel(classe, y) {
            for k in -4..=4 {
                let p = Vec2::new(hw * k as f32 / 4.0, y);
                let d = p.distance(local);
                if d < d_melhor && cabe(classe, p) {
                    melhor = p;
                    d_melhor = d;
                }
            }
        }
        y += 0.25;
    }
    melhor
}

/// A free spot for the `n`-th crew member to come aboard: amidships, spread
/// across and along.
pub fn lugar_da_tripulacao(classe: usize, n: usize) -> Vec2 {
    let lado = if n % 2 == 0 { 1.0 } else { -1.0 };
    let fila = (n / 2) as f32;
    ponto_livre(classe, Vec2::new(lado * 0.6, -0.4 - fila * 0.8))
}

/// The across axis (starboard) of a ship heading `rumo`.
pub fn travessa(rumo: Vec2) -> Vec2 {
    Vec2::new(rumo.y, -rumo.x)
}

/// Deck → world. `rumo` is the bow's unit direction; the client's yaw
/// convention (`x.atan2(z)`, models facing +Z) maps the mesh's +X to
/// `travessa(rumo)`.
pub fn do_mundo(navio: Vec2, rumo: Vec2, local: Vec2) -> Vec2 {
    navio + rumo * local.y + travessa(rumo) * local.x
}

/// World → deck.
pub fn para_local(navio: Vec2, rumo: Vec2, mundo: Vec2) -> Vec2 {
    let d = mundo - navio;
    Vec2::new(d.dot(travessa(rumo)), d.dot(rumo))
}

/// A world direction turned into deck space.
pub fn dir_local(rumo: Vec2, dir: Vec2) -> Vec2 {
    Vec2::new(dir.dot(travessa(rumo)), dir.dot(rumo))
}

/// A deck direction turned into world space.
pub fn dir_do_mundo(rumo: Vec2, dir: Vec2) -> Vec2 {
    rumo * dir.y + travessa(rumo) * dir.x
}

/// One body aboard, on the wire (`ServerMessage::Conves`): sent only when it
/// changes, per watching session. `navio` 0 = it left the deck.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct NoConves {
    pub id: EntityId,
    pub navio: EntityId,
    /// Deck position in 1/32 unit (`ESCALA_LOCAL`).
    pub local: [i16; 2],
    /// At the wheel (the steering pose).
    pub leme: bool,
}

impl NoConves {
    pub fn novo(id: EntityId, navio: EntityId, local: Vec2, leme: bool) -> Self {
        let q = |v: f32| {
            (v * ESCALA_LOCAL)
                .round()
                .clamp(i16::MIN as f32, i16::MAX as f32) as i16
        };
        NoConves {
            id,
            navio,
            local: [q(local.x), q(local.y)],
            leme,
        }
    }
    pub fn local_f32(&self) -> Vec2 {
        Vec2::new(self.local[0] as f32, self.local[1] as f32) / ESCALA_LOCAL
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    /// Each class's walkable deck is about the size the owner asked for, and
    /// grows class by class.
    #[test]
    fn os_conveses_tem_o_tamanho_pedido() {
        let pedido = [(6.0, 2.5), (9.0, 3.5), (13.0, 5.0), (17.0, 6.0)];
        for (classe, (comp_q, larg_q)) in pedido.iter().enumerate() {
            let mc = meio_comp(classe);
            let mut ys = Vec::new();
            let mut larg = 0.0f32;
            let mut y = -mc;
            while y <= mc {
                if let Some(hw) = meia_largura_andavel(classe, y) {
                    ys.push(y);
                    larg = larg.max(2.0 * (hw + RAIO_DO_CORPO));
                }
                y += 0.05;
            }
            let comp = ys.last().unwrap() - ys.first().unwrap();
            println!("class {classe}: deck {comp:.1} x {larg:.1} u");
            assert!(
                (comp - comp_q).abs() < comp_q * 0.15,
                "class {classe}: deck length {comp}"
            );
            assert!(
                (larg - larg_q).abs() < larg_q * 0.15,
                "class {classe}: deck width {larg}"
            );
        }
    }

    /// The rails are walls, and the masts and the wheel block.
    #[test]
    fn a_amurada_e_parede_e_o_mastro_barra() {
        for classe in 0..4 {
            // A stretch of deck clear of the masts.
            let y = (0..80)
                .map(|k| -(k as f32) * 0.1)
                .find(|y| {
                    cabe(classe, Vec2::new(0.0, *y))
                        && mastros(classe).all(|m| m.distance(Vec2::new(0.0, *y)) > 0.9)
                })
                .unwrap();
            let hw = meia_largura_andavel(classe, y).unwrap();
            // Walking to starboard forever stops at the rail.
            let mut p = Vec2::new(0.0, y);
            for _ in 0..200 {
                p = mover(classe, p, Vec2::new(0.1, 0.0));
            }
            assert!(p.x <= hw + 1e-4 && p.x > hw - 0.11, "class {classe}: {p}");
            // Forward forever stops before the bow.
            let mut q = Vec2::new(0.0, 0.0);
            for _ in 0..400 {
                q = mover(classe, q, Vec2::new(0.0, 0.1));
            }
            assert!(q.y < meio_comp(classe), "class {classe}: past the bow {q}");
            // The masts block.
            for m in mastros(classe) {
                assert!(!cabe(classe, m), "class {classe}: inside a mast");
            }
            assert!(!cabe(classe, roda(classe)));
            assert!(
                cabe(classe, leme(classe)),
                "class {classe}: the helmsman's spot is walkable"
            );
            for n in 0..6 {
                assert!(cabe(classe, lugar_da_tripulacao(classe, n)));
            }
        }
    }

    /// THE WIRE: a deck row is a dozen bytes; a full crew of eight moving
    /// is under 120 bytes a tick, and every class's deck fits the i16 rows.
    #[test]
    fn a_linha_do_conves_cabe_no_fio() {
        let lista: Vec<NoConves> = (0..8)
            .map(|k| {
                NoConves::novo(
                    EntityId(40_000 + k),
                    EntityId(41_000),
                    Vec2::new(-2.9 + k as f32 * 0.7, 9.8 - k as f32 * 2.4),
                    k == 0,
                )
            })
            .collect();
        let m = crate::protocol::ServerMessage::Conves { lista };
        let bytes = postcard::to_allocvec(&m).unwrap();
        println!("8 deck rows: {} bytes", bytes.len());
        assert!(bytes.len() <= 120, "{} bytes", bytes.len());
        for classe in 0..4 {
            assert!(meio_comp(classe) * ESCALA_LOCAL < i16::MAX as f32);
        }
    }

    #[test]
    fn local_e_mundo_vao_e_voltam() {
        let rumo = Vec2::new(0.6, 0.8);
        let navio = Vec2::new(100.0, -40.0);
        let local = Vec2::new(1.2, -3.4);
        let w = do_mundo(navio, rumo, local);
        assert!(para_local(navio, rumo, w).distance(local) < 1e-4);
        // The bow is ahead along `rumo`.
        assert!(do_mundo(navio, rumo, Vec2::new(0.0, 2.0)).distance(navio + rumo * 2.0) < 1e-5);
        let n = NoConves::novo(EntityId(3), EntityId(4), local, false);
        assert!(n.local_f32().distance(local) < 0.02);
    }
}
