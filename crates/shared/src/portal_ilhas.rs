//! THE ISLAND PORTAL (17/11/2026): one in the town of every island of the
//! `ARQUIPELAGO`, the only way to travel between islands without sailing.
//!
//! The owner: "the dock npc and the teleport npc should not be the same. I
//! want you to create a portal, similar to the porão ones, but a few
//! different because it will be an island teleport portal. Every city will
//! have one and you can use it to teleport. The dock will only be to manage
//! and sail with ships."
//!
//! Like the Porão doors (`porao::porta_de`) the portal is not an entity: its
//! spot is a pure function of the island's `Gerador`, so the server (range
//! checks, arrival), the client (drawing, map icon, the walk-in trigger) and
//! the tests all find the same point without anything on the wire.
//!
//! ## Where it stands
//!
//! In the town, a few steps out of the square, on the side of the road to
//! the dock (or, on an island with no land port, the side of its dock NPC).
//! The search reads the finished village (`vila::montar`) and keeps away from
//! everything in it: no wall, prop, NPC, path or door is closer than the
//! portal's own footprint plus a margin, the ground is the town's level all
//! round it, and no Porão door is near. Deterministic: same island, same spot.
//!
//! The ARRIVAL point is on open ground in front of it, towards the square, and
//! outside the walk-in radius (otherwise arriving would open it again).

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

use glam::Vec2;

use crate::terreno::{Gerador, BLOCO, NIVEL_DO_MAR};
use crate::vila::{self, Vila};

/// Radius of the portal's stone platform (the drawn ring is a bit wider than
/// tall). Nothing solid comes inside `PEGADA + FOLGA`.
pub const PEGADA: f32 = 2.4;
/// Clear margin round the footprint.
pub const FOLGA: f32 = 1.2;
/// Walking this close to the centre opens the travel menu (the client's
/// trigger). Larger than the auto quest's arrival radius (3 u), so whoever the
/// auto quest walks here always steps into it.
pub const ALCANCE: f32 = 3.2;
/// The server accepts a portal request ("open", "board") this close: the
/// trigger radius plus slack for the step taken while the request travels.
pub const PERTO: f32 = 8.0;
/// The banner (the "Travel" button) shows this close.
pub const AVISO: f32 = 9.0;
/// The arrival point's distance from the portal's centre.
pub const CHEGADA: f32 = 5.5;
/// No Porão door closer than this.
const LONGE_DA_PORTA_DO_PORAO: f32 = 14.0;
/// The ring of candidate spots round the town square.
const RAIO_MIN: f32 = 9.0;
const RAIO_MAX: f32 = 40.0;

/// The portal of the island this generator makes, and the arrival point next
/// to it. `None`: no town (the colony, the arena, the sea).
pub fn sitio(ger: &Gerador) -> Option<(Vec2, Vec2)> {
    let def = crate::terreno::ARQUIPELAGO
        .iter()
        .find(|d| d.semente == ger.semente)?;
    let cidade = ger.cidade()?;
    let centro = cidade.centro();
    let chao = cidade.altura();
    let vila = ger.vila();
    let caixas = caixas_de_tudo(vila);
    let portas: Vec<Vec2> = crate::porao::portas_da_zona(def.zona, ger)
        .into_iter()
        .map(|(_, p)| p)
        .collect();
    let rumo = rumo_do_cais(ger, vila, centro);
    let a0 = rumo.y.atan2(rumo.x);
    // Candidates by preference: near the dock road's heading first, then
    // close to the square.
    let mut candidatos: Vec<(f32, Vec2)> = Vec::new();
    let mut r = RAIO_MIN;
    while r <= RAIO_MAX {
        for k in 0..48 {
            let desvio = (k as f32 - 23.5) / 24.0 * std::f32::consts::PI;
            let a = a0 + desvio;
            let q = centro + Vec2::new(a.cos(), a.sin()) * r;
            candidatos.push((desvio.abs() * 5.0 + r, q));
        }
        r += 1.0;
    }
    candidatos.sort_by(|a, b| a.0.total_cmp(&b.0));
    for (_, q) in candidatos {
        if !livre(ger, vila, &caixas, &portas, q, chao, PEGADA + FOLGA, true)
            || !sem_arvore(ger, def.bioma, q, PEGADA + FOLGA)
        {
            continue;
        }
        // The ring faces open ground: the arrival point and a clear view
        // beyond it (no wall right in front), towards the square if it can.
        let para_praca = (centro - q).normalize_or(Vec2::Y);
        for giro in [0.0f32, 0.785, -0.785, 1.571, -1.571, 2.356, -2.356, 3.142] {
            let frente = Vec2::from_angle(giro).rotate(para_praca);
            let chegada = q + frente * CHEGADA;
            let vista_livre = [8.5f32, 12.0].iter().all(|d| {
                let v = q + frente * *d;
                !caixas.iter().any(|c| dist_caixa(c, v) < 1.5)
            });
            if vista_livre
                && livre(ger, vila, &caixas, &portas, chegada, chao, 0.9, false)
                && sem_arvore(ger, def.bioma, chegada, 1.0)
            {
                return Some((q, chegada));
            }
        }
    }
    None
}

/// Only the portal's centre.
pub fn posicao(ger: &Gerador) -> Option<Vec2> {
    sitio(ger).map(|(p, _)| p)
}

/// The portal of `zona` (an island of the `ARQUIPELAGO`): (centre, arrival).
/// Computed once per zone — the search probes the relief and the village.
pub fn da_zona(zona: &str) -> Option<(Vec2, Vec2)> {
    let cache = CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    if let Some(v) = cache.lock().ok().and_then(|c| c.get(zona).copied()) {
        return v;
    }
    let def = crate::terreno::ARQUIPELAGO
        .iter()
        .find(|d| d.zona == zona)?;
    let v = sitio(&Gerador::da_ilha(def));
    if let Ok(mut c) = cache.lock() {
        c.insert(zona.to_string(), v);
    }
    v
}

/// The portal of `zona` IF it was already found (`da_zona` ran): never
/// computes. The client draws from this on the frame, and warms the cache on
/// the map's thread, where the search costs nothing visible.
pub fn ja_calculado(zona: &str) -> Option<(Vec2, Vec2)> {
    CACHE.get()?.lock().ok()?.get(zona).copied().flatten()
}

static CACHE: OnceLock<Mutex<HashMap<String, Option<(Vec2, Vec2)>>>> = OnceLock::new();

/// Where the travel menu's way out of town points: the road to the dock, or
/// the nearest dock NPC on an island with no land port.
fn rumo_do_cais(ger: &Gerador, vila: &Vila, centro: Vec2) -> Vec2 {
    if let Some(p) = ger.porto() {
        if let Some(d) = (p.centro - centro).try_normalize() {
            return d;
        }
    }
    use crate::construcao::Papel;
    vila.npcs
        .iter()
        .filter(|n| {
            matches!(
                n.papel,
                Papel::Estaleiro | Papel::Motorista | Papel::Submarino
            )
        })
        .min_by(|a, b| a.pos.distance(centro).total_cmp(&b.pos.distance(centro)))
        .and_then(|n| (n.pos - centro).try_normalize())
        .unwrap_or(Vec2::Y)
}

/// Every box of the village, the decorations too (a lamp or a bench does not
/// block a step, but a portal standing in it would look wrong).
fn caixas_de_tudo(vila: &Vila) -> Vec<vila::Caixa2> {
    let mut v = vila::caixas_solidas(vila);
    for pr in &vila.props {
        for (mn, mx) in pr.construcao().caixas_mundo(pr.pos, pr.yaw_q) {
            v.push(vila::Caixa2 {
                min: Vec2::new(mn.x, mn.z),
                max: Vec2::new(mx.x, mx.z),
            });
        }
    }
    for p in &vila.predios {
        for (mn, mx) in p.construcao().caixas_mundo(p.pos, p.yaw_q) {
            v.push(vila::Caixa2 {
                min: Vec2::new(mn.x, mn.z),
                max: Vec2::new(mx.x, mx.z),
            });
        }
    }
    v
}

fn dist_caixa(c: &vila::Caixa2, p: Vec2) -> f32 {
    let dx = (c.min.x - p.x).max(p.x - c.max.x).max(0.0);
    let dz = (c.min.y - p.y).max(p.y - c.max.y).max(0.0);
    Vec2::new(dx, dz).length()
}

fn dist_seg(p: Vec2, a: Vec2, b: Vec2) -> f32 {
    let ab = b - a;
    let t = if ab.length_squared() < 1e-6 {
        0.0
    } else {
        ((p - a).dot(ab) / ab.length_squared()).clamp(0.0, 1.0)
    };
    p.distance(a + ab * t)
}

/// Open ground of radius `raio` at `q`: town level, dry, clear of the
/// village. `portal` = also keep off paths, NPCs, doors and Porão portals
/// (the arrival point is ground to stand on, so only walls and props count).
#[allow(clippy::too_many_arguments)]
fn livre(
    ger: &Gerador,
    vila: &Vila,
    caixas: &[vila::Caixa2],
    portas: &[Vec2],
    q: Vec2,
    chao: f32,
    raio: f32,
    portal: bool,
) -> bool {
    // The ground: the town's level all round, well above the sea.
    let passos = (raio / BLOCO).ceil() as i32;
    for iz in -passos..=passos {
        for ix in -passos..=passos {
            let o = Vec2::new(ix as f32, iz as f32) * BLOCO;
            if o.length() > raio {
                continue;
            }
            let h = ger.altura(q.x + o.x, q.y + o.y);
            if (h - chao).abs() > 0.3 || h <= NIVEL_DO_MAR + 0.3 {
                return false;
            }
        }
    }
    if caixas.iter().any(|c| dist_caixa(c, q) < raio) {
        return false;
    }
    if !portal {
        return true;
    }
    if vila.npcs.iter().any(|n| n.pos.distance(q) < raio + 2.0) {
        return false;
    }
    if vila
        .caminhos
        .iter()
        .any(|(a, b)| dist_seg(q, *a, *b) < raio + vila::CAMINHO_MEIA)
    {
        return false;
    }
    if vila::atrapalha_porta(vila, q, raio + 2.0) {
        return false;
    }
    !portas
        .iter()
        .any(|p| p.distance(q) < LONGE_DA_PORTA_DO_PORAO)
}

/// No tree trunk, boulder or stump in the circle (`terreno::estorvos_da_coluna`,
/// the same planting the server's collision reads).
fn sem_arvore(ger: &Gerador, bioma: crate::terreno::Bioma, q: Vec2, raio: f32) -> bool {
    let n = ((raio + 1.5) / BLOCO).ceil() as i32;
    let (cx, cz) = ((q.x / BLOCO).round() as i32, (q.y / BLOCO).round() as i32);
    let mut achados = Vec::new();
    for bz in cz - n..=cz + n {
        for bx in cx - n..=cx + n {
            let topo = ger.bloco_em(bx, bz);
            let declive = [(1, 0), (-1, 0), (0, 1), (0, -1)]
                .iter()
                .map(|(dx, dz)| (topo - ger.bloco_em(bx + dx, bz + dz)).abs())
                .max()
                .unwrap_or(0);
            achados.clear();
            crate::terreno::estorvos_da_coluna(
                bioma,
                bx,
                bz,
                topo,
                declive,
                ger,
                false,
                &mut achados,
            );
            if achados.iter().any(|e| {
                !matches!(e.tipo, crate::terreno::TipoDeEstorvo::Forracao)
                    && e.centro.distance(q) < raio + e.raio
            }) {
                return false;
            }
        }
    }
    true
}

#[cfg(test)]
mod testes {
    use super::*;
    use crate::terreno::{Ilha, ARQUIPELAGO};

    /// EVERY TRAVEL ISLAND HAS EXACTLY ONE PORTAL, on open ground, and a
    /// player walks to it from the town square (the server's own route).
    #[test]
    fn toda_ilha_tem_um_portal_em_chao_livre_e_se_chega_andando() {
        for def in ARQUIPELAGO {
            let ger = Gerador::da_ilha(&def);
            let (p, chegada) =
                sitio(&ger).unwrap_or_else(|| panic!("{}: no Island Portal", def.zona));
            assert_eq!(
                da_zona(def.zona),
                Some((p, chegada)),
                "{}: the cache disagrees",
                def.zona
            );
            let cidade = ger.cidade().unwrap();
            let ilha = Ilha::da_ilha(&def);
            // Open ground: nothing the server collides with in the footprint
            // or on the arrival point.
            assert!(
                ilha.sem_estorvo(p, PEGADA),
                "{}: the portal stands in something",
                def.zona
            );
            assert!(
                ilha.sem_estorvo(chegada, 0.6),
                "{}: the arrival is blocked",
                def.zona
            );
            assert!(!ilha.agua(p.x, p.y) && !ilha.agua(chegada.x, chegada.y));
            assert!(
                chegada.distance(p) > ALCANCE + 1.0,
                "{}: arriving would open it again",
                def.zona
            );
            // Not on top of anyone, and not on a road.
            let vila = ger.vila();
            for n in &vila.npcs {
                assert!(
                    n.pos.distance(p) >= PEGADA + 2.0,
                    "{}: on top of {}",
                    def.zona,
                    n.nome
                );
            }
            for (a, b) in &vila.caminhos {
                assert!(dist_seg(p, *a, *b) >= PEGADA, "{}: blocks a path", def.zona);
            }
            for (_, d) in crate::porao::portas_da_zona(def.zona, &ger) {
                assert!(
                    d.distance(p) >= LONGE_DA_PORTA_DO_PORAO,
                    "{}: beside a Porão portal",
                    def.zona
                );
            }
            // In town: a few steps from the square.
            assert!(
                cidade.distancia(p) <= RAIO_MAX + 0.5,
                "{}: out of town",
                def.zona
            );
            // Walkable from the square, and back.
            let inicio = ilha.ponto_livre_perto(cidade.centro() + Vec2::new(0.0, 6.0), 0.6);
            let rota = ilha.caminho(inicio, chegada, 60_000);
            assert!(
                rota.as_ref()
                    .and_then(|r| r.last())
                    .is_some_and(|f| f.distance(chegada) < 2.0),
                "{}: no walk from the square to the portal at {p:?}",
                def.zona
            );
            let ate = ilha.caminho(chegada, p, 20_000);
            assert!(
                ate.as_ref()
                    .and_then(|r| r.last())
                    .is_some_and(|f| f.distance(p) < ALCANCE),
                "{}: the portal cannot be stepped into",
                def.zona
            );
            println!(
                "{}: portal at ({:.1}, {:.1}), {:.0} u from the square, arrival ({:.1}, {:.1})",
                def.zona,
                p.x,
                p.y,
                cidade.distancia(p),
                chegada.x,
                chegada.y
            );
        }
        assert!(
            da_zona("ilha_magica").is_none(),
            "the Magic Island is reached through the menu only"
        );
        assert!(da_zona(crate::mar::ZONA).is_none());
    }
}
