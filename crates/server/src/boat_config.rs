//! Configuracao por boat_kind carregada de `data/boats/boat_kind_N.json`.
//! Origem: Unity prefab exportado via Tools / Boat / Export Prefab Data.
//!
//! Fallback: se o arquivo nao existir, usa default hardcoded (compat com
//! fluxo antigo).

use glam::Vec2;
use serde::Deserialize;
use std::collections::HashMap;
use std::sync::OnceLock;

#[derive(Debug, Clone, Deserialize)]
pub struct BoatStations {
    pub helm:   [f32; 2],
    pub sail:   [f32; 2],
    pub anchor: [f32; 2],
}

#[derive(Debug, Clone, Deserialize)]
pub struct BoatKindConfig {
    pub kind: u16,
    pub deck_half_w: f32,
    pub deck_half_h: f32,
    pub stations: BoatStations,
    #[serde(default)]
    pub deck_polygon: Vec<[f32; 2]>,
    /// Hull collider — usado pra colisao do casco contra terra/margem.
    /// Geralmente um pouco maior que o deck (engloba proa/popa fora da
    /// area andavel). Se vazio, fall-back deck_polygon.
    #[serde(default)]
    pub hull_polygon: Vec<[f32; 2]>,
    /// Obstaculos internos (mastro, leme, bancos). Cada um eh uma lista
    /// de pontos formando um poligono. Player nao consegue entrar.
    /// Exportado de PolygonCollider2D em BoatVisualData.obstacleColliders.
    #[serde(default)]
    pub obstacles: Vec<Vec<[f32; 2]>>,
}

impl BoatKindConfig {
    pub fn deck_half(&self) -> Vec2 {
        Vec2::new(self.deck_half_w, self.deck_half_h)
    }
    pub fn helm_local(&self)   -> Vec2 { Vec2::new(self.stations.helm[0],   self.stations.helm[1]) }
    pub fn sail_local(&self)   -> Vec2 { Vec2::new(self.stations.sail[0],   self.stations.sail[1]) }
    pub fn anchor_local(&self) -> Vec2 { Vec2::new(self.stations.anchor[0], self.stations.anchor[1]) }

    /// Clampa `p` (local-space do barco) pra dentro do deck E fora dos
    /// obstaculos. Etapas:
    ///   1. Se fora do deck_polygon → push pra borda mais proxima do deck.
    ///   2. Pra cada obstacle, se dentro → push pra borda mais proxima do obstacle.
    /// Sem deck_polygon, fall-back bbox; sem obstacles, so' deck.
    pub fn clamp_to_deck(&self, p: Vec2) -> Vec2 {
        let mut q = if self.deck_polygon.len() >= 3 {
            if point_in_polygon(p, &self.deck_polygon) {
                p
            } else {
                nearest_point_on_polygon(p, &self.deck_polygon)
            }
        } else {
            let h = self.deck_half();
            Vec2::new(p.x.clamp(-h.x, h.x), p.y.clamp(-h.y, h.y))
        };
        // Empurra pra fora de cada obstacle (mastro/leme/bancos). Iterativo
        // ja que push de um obstacle pode jogar dentro de outro.
        for _ in 0..3 {
            let mut moved = false;
            for obs in &self.obstacles {
                if obs.len() < 3 { continue; }
                if point_in_polygon(q, obs) {
                    let edge = nearest_point_on_polygon(q, obs);
                    // Pequeno epsilon pra ficar realmente FORA do obstacle.
                    let dir = (edge - q).normalize_or_zero();
                    q = edge + dir * 0.05;
                    moved = true;
                }
            }
            if !moved { break; }
        }
        q
    }

    /// True se TODOS os vertices do hull_polygon (ou deck_polygon como
    /// fall-back), transformados pra world (rotacionados por yaw +
    /// transladados por pos), caem em tile navegavel. Usado pra colisao
    /// do casco contra ilhas — sem isso o barco entrava na terra ate o
    /// tile central virar nao-navegavel.
    pub fn is_hull_navigable(
        &self,
        pos: Vec2,
        yaw: f32,
        map: &shared::world_gen::WorldMap,
    ) -> bool {
        let cos_y = yaw.cos();
        let sin_y = yaw.sin();
        let check = |local: Vec2| -> bool {
            // Y-forward: world = (local.x * cos - local.y * sin, local.x * sin + local.y * cos)
            let wx = pos.x + local.x * cos_y - local.y * sin_y;
            let wy = pos.y + local.x * sin_y + local.y * cos_y;
            map.is_navigable(wx.floor() as i32, wy.floor() as i32)
        };
        let poly = if self.hull_polygon.len() >= 3 {
            &self.hull_polygon
        } else {
            &self.deck_polygon
        };
        if poly.len() >= 3 {
            for v in poly {
                if !check(Vec2::new(v[0], v[1])) {
                    return false;
                }
            }
            true
        } else {
            // Fall-back final: 4 cantos da bbox.
            let h = self.deck_half();
            check(Vec2::new(-h.x, -h.y))
                && check(Vec2::new( h.x, -h.y))
                && check(Vec2::new(-h.x,  h.y))
                && check(Vec2::new( h.x,  h.y))
        }
    }

    /// Empurra `pos` pra longe de terra ate o casco caber. Itera N steps;
    /// pra cada vertice do hull que cai em terra, acumula push = (boat_center
    /// - vertex) normalizado, somando todos. Aplica fracao por step ate
    /// chegar em pos navegavel ou esgotar tentativas. Sem mudar yaw — eh um
    /// translation-only repel.
    pub fn repel_from_land(
        &self,
        pos: Vec2,
        yaw: f32,
        map: &shared::world_gen::WorldMap,
    ) -> Vec2 {
        let poly = if self.hull_polygon.len() >= 3 {
            &self.hull_polygon
        } else {
            &self.deck_polygon
        };
        if poly.len() < 3 { return pos; }
        let cos_y = yaw.cos();
        let sin_y = yaw.sin();
        let mut p = pos;
        for _ in 0..6 {
            let mut push = Vec2::ZERO;
            let mut count = 0usize;
            for v in poly {
                let lx = v[0]; let ly = v[1];
                let wx = p.x + lx * cos_y - ly * sin_y;
                let wy = p.y + lx * sin_y + ly * cos_y;
                if !map.is_navigable(wx.floor() as i32, wy.floor() as i32) {
                    let dx = p.x - wx;
                    let dy = p.y - wy;
                    let len = (dx * dx + dy * dy).sqrt();
                    if len > 0.001 {
                        push.x += dx / len;
                        push.y += dy / len;
                        count += 1;
                    }
                }
            }
            if count == 0 { break; }
            push /= count as f32;
            p += push * 0.15; // step pequeno pra nao teleportar
        }
        p
    }

    /// Default hardcoded — usado quando o JSON nao existe (Lylian).
    fn default_lylian() -> Self {
        Self {
            kind: 0,
            deck_half_w: shared::constants::BOAT_LYLIAN_DECK_HALF_W,
            deck_half_h: shared::constants::BOAT_LYLIAN_DECK_HALF_H,
            stations: BoatStations {
                helm:   [0.0, -shared::constants::BOAT_LYLIAN_DECK_HALF_H * 0.85],
                sail:   [0.0, 0.0],
                anchor: [0.0,  shared::constants::BOAT_LYLIAN_DECK_HALF_H * 0.85],
            },
            deck_polygon: Vec::new(),
            hull_polygon: Vec::new(),
            obstacles: Vec::new(),
        }
    }
}

static REGISTRY: OnceLock<HashMap<u16, BoatKindConfig>> = OnceLock::new();

/// Carrega todos os boat_kind_*.json de `data/boats/`. Idempotente — primeira
/// chamada faz scan; chamadas subsequentes reusam o cache.
pub fn registry() -> &'static HashMap<u16, BoatKindConfig> {
    REGISTRY.get_or_init(|| {
        let mut map: HashMap<u16, BoatKindConfig> = HashMap::new();
        let dir = std::path::Path::new("data/boats");
        if let Ok(rd) = std::fs::read_dir(dir) {
            for entry in rd.flatten() {
                let path = entry.path();
                if path.extension().and_then(|e| e.to_str()) != Some("json") { continue; }
                match std::fs::read_to_string(&path) {
                    Ok(text) => match serde_json::from_str::<BoatKindConfig>(&text) {
                        Ok(cfg) => {
                            tracing::info!(
                                "[boat_config] loaded {}: kind={} deck={:.1}x{:.1} stations(helm={:?}, sail={:?}, anchor={:?}) obstacles={}",
                                path.display(), cfg.kind,
                                cfg.deck_half_w * 2.0, cfg.deck_half_h * 2.0,
                                cfg.stations.helm, cfg.stations.sail, cfg.stations.anchor,
                                cfg.obstacles.len()
                            );
                            map.insert(cfg.kind, cfg);
                        }
                        Err(e) => tracing::warn!("[boat_config] parse {} fail: {}", path.display(), e),
                    },
                    Err(e) => tracing::warn!("[boat_config] read {} fail: {}", path.display(), e),
                }
            }
        }
        // Garante que Lylian (kind=0) sempre exista — fallback hardcoded.
        map.entry(0).or_insert_with(BoatKindConfig::default_lylian);
        tracing::info!("[boat_config] registry pronto: {} kinds", map.len());
        map
    })
}

pub fn get(kind: u16) -> &'static BoatKindConfig {
    let r = registry();
    r.get(&kind).unwrap_or_else(|| r.get(&0).expect("boat_kind 0 default deve existir"))
}

// ── Geometry helpers ────────────────────────────────────────────────────────

/// Point-in-polygon via ray casting (par/impar). Polygon = lista de
/// [x, y] em ordem (CW ou CCW, qualquer).
pub fn point_in_polygon(p: Vec2, poly: &[[f32; 2]]) -> bool {
    let n = poly.len();
    if n < 3 { return false; }
    let mut inside = false;
    let mut j = n - 1;
    for i in 0..n {
        let (xi, yi) = (poly[i][0], poly[i][1]);
        let (xj, yj) = (poly[j][0], poly[j][1]);
        let intersect = (yi > p.y) != (yj > p.y)
            && p.x < (xj - xi) * (p.y - yi) / (yj - yi + 1e-9) + xi;
        if intersect { inside = !inside; }
        j = i;
    }
    inside
}

/// Acha o ponto mais proximo de `p` na perimetro do polygon (clamp pra dentro).
pub fn nearest_point_on_polygon(p: Vec2, poly: &[[f32; 2]]) -> Vec2 {
    let n = poly.len();
    let mut best = Vec2::new(poly[0][0], poly[0][1]);
    let mut best_d2 = (best - p).length_squared();
    for i in 0..n {
        let a = Vec2::new(poly[i][0], poly[i][1]);
        let b = Vec2::new(poly[(i + 1) % n][0], poly[(i + 1) % n][1]);
        let q = closest_on_segment(p, a, b);
        let d2 = (q - p).length_squared();
        if d2 < best_d2 { best_d2 = d2; best = q; }
    }
    best
}

fn closest_on_segment(p: Vec2, a: Vec2, b: Vec2) -> Vec2 {
    let ab = b - a;
    let len2 = ab.length_squared();
    if len2 < 1e-6 { return a; }
    let t = ((p - a).dot(ab) / len2).clamp(0.0, 1.0);
    a + ab * t
}
