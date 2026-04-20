//! Colisao AABB discreta contra o tilemap.
//!
//! Algoritmo "move and slide" por eixo:
//! 1. Move so no eixo X. Se colidiu, empurra de volta e zera velocidade X.
//! 2. Move so no eixo Y. Se colidiu, empurra de volta e zera velocidade Y.
//!
//! Nao e swept (sem tunneling para objetos que se movem devagar). Para
//! projeteis rapidos, implementar CCD (fase 3).

use crate::tilemap::Tilemap;
use glam::Vec2;

/// Resultado de `move_and_slide`.
#[derive(Debug, Clone, Copy, Default)]
pub struct MoveResult {
    /// Posicao final apos resolucao.
    pub position: Vec2,
    /// true se colidiu horizontalmente.
    pub hit_x: bool,
    /// true se colidiu verticalmente.
    pub hit_y: bool,
}

/// Move `pos` por `delta` contra os tiles solidos do `map`.
///
/// * `pos`   — posicao do centro da entidade em unidades de mundo.
/// * `delta` — deslocamento desejado (vel * dt).
/// * `size`  — tamanho do AABB da entidade.
pub fn move_and_slide(pos: Vec2, delta: Vec2, size: Vec2, map: &Tilemap) -> MoveResult {
    let half = size * 0.5;
    let mut result = MoveResult { position: pos, ..Default::default() };

    // --- Eixo X ---
    let new_x = pos.x + delta.x;
    let test_x = Vec2::new(new_x, pos.y);
    if !overlaps_solid(test_x, half, map) {
        result.position.x = new_x;
    } else {
        result.hit_x = true;
        // Empurra para o lado livre mais proximo.
        result.position.x = push_out_x(pos.x, delta.x, half.x, map, pos.y);
    }

    // --- Eixo Y (usa X ja resolvido) ---
    let new_y = result.position.y + delta.y;
    let test_y = Vec2::new(result.position.x, new_y);
    if !overlaps_solid(test_y, half, map) {
        result.position.y = new_y;
    } else {
        result.hit_y = true;
        result.position.y = push_out_y(result.position.y, delta.y, half.y, map, result.position.x);
    }

    result
}

/// Verifica se o AABB em `center +/- half` sobrepoe qualquer tile solido.
pub fn overlaps_solid(center: Vec2, half: Vec2, map: &Tilemap) -> bool {
    let ts = map.tile_size;
    let min = center - half;
    let max = center + half - Vec2::splat(0.001); // epsilon para borda exata nao colidir

    let x0 = (min.x / ts).floor() as i32;
    let x1 = (max.x / ts).floor() as i32;
    let y0 = (min.y / ts).floor() as i32;
    let y1 = (max.y / ts).floor() as i32;

    for ty in y0..=y1 {
        for tx in x0..=x1 {
            if map.is_solid(tx, ty) {
                return true;
            }
        }
    }
    false
}

fn push_out_x(cx: f32, dx: f32, half_x: f32, map: &Tilemap, _cy: f32) -> f32 {
    let ts = map.tile_size;
    if dx > 0.0 {
        // Movendo para direita: encostar na face esquerda do tile.
        let tile_x = ((cx + half_x + dx) / ts).floor() as i32;
        (tile_x as f32 * ts) - half_x - 0.001
    } else if dx < 0.0 {
        // Movendo para esquerda: encostar na face direita.
        let tile_x = ((cx - half_x + dx) / ts).floor() as i32;
        ((tile_x + 1) as f32 * ts) + half_x + 0.001
    } else {
        cx
    }
    // Supress unused: cy nao e necessario no algoritmo simples por eixo.
    .max(half_x)
    .min(map.width as f32 * map.tile_size - half_x)
}

fn push_out_y(cy: f32, dy: f32, half_y: f32, map: &Tilemap, _cx: f32) -> f32 {
    let ts = map.tile_size;
    if dy > 0.0 {
        let tile_y = ((cy + half_y + dy) / ts).floor() as i32;
        (tile_y as f32 * ts) - half_y - 0.001
    } else if dy < 0.0 {
        let tile_y = ((cy - half_y + dy) / ts).floor() as i32;
        ((tile_y + 1) as f32 * ts) + half_y + 0.001
    } else {
        cy
    }
    .max(half_y)
    .min(map.height as f32 * map.tile_size - half_y)
}
