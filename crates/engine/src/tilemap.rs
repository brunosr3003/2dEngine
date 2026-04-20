//! Tilemap 2D. Mundo dividido em grid de tiles, cada um com UVs no atlas
//! e flag de solido. O renderer alimenta o SpriteBatch usando camera culling —
//! so tiles visiveis sao processados por frame.

use crate::math::Rect;
use crate::render::{Camera2D, Sprite, SpriteBatch};
use glam::Vec2;

/// Definicao de um tipo de tile: como ele aparece no atlas e se bloqueia.
#[derive(Debug, Clone, Copy)]
pub struct TileDef {
    pub uv_min: Vec2,
    pub uv_max: Vec2,
    /// true = bloqueia movimento (parede, agua, etc.).
    pub solid: bool,
}

/// O mapa em si. Coordenadas: origem (0,0) no canto inferior-esquerdo,
/// X cresce pra direita, Y cresce pra cima (igual ao mundo do jogo).
pub struct Tilemap {
    pub width: u32,
    pub height: u32,
    /// Tamanho de cada tile em unidades de mundo (tiles).
    pub tile_size: f32,
    /// IDs dos tiles, row-major [y * width + x]. 0 = vazio (nao renderiza).
    tiles: Vec<u16>,
    /// Lookup de definicoes por ID. Index 0 e reservado (vazio).
    pub tile_defs: Vec<TileDef>,
}

impl Tilemap {
    pub fn new(width: u32, height: u32, tile_size: f32) -> Self {
        Self {
            width,
            height,
            tile_size,
            tiles: vec![0; (width * height) as usize],
            tile_defs: vec![TileDef {
                uv_min: Vec2::ZERO,
                uv_max: Vec2::ONE,
                solid: false,
            }],
        }
    }

    /// Registra um tipo de tile e retorna o ID atribuido.
    pub fn add_tile_def(&mut self, def: TileDef) -> u16 {
        let id = self.tile_defs.len() as u16;
        self.tile_defs.push(def);
        id
    }

    pub fn get(&self, x: i32, y: i32) -> u16 {
        if x < 0 || y < 0 || x >= self.width as i32 || y >= self.height as i32 {
            return 0;
        }
        self.tiles[(y as u32 * self.width + x as u32) as usize]
    }

    pub fn set(&mut self, x: i32, y: i32, id: u16) {
        if x < 0 || y < 0 || x >= self.width as i32 || y >= self.height as i32 {
            return;
        }
        self.tiles[(y as u32 * self.width + x as u32) as usize] = id;
    }

    pub fn is_solid(&self, x: i32, y: i32) -> bool {
        let id = self.get(x, y) as usize;
        self.tile_defs.get(id).map(|d| d.solid).unwrap_or(true)
    }

    /// Preenche o SpriteBatch com os tiles visiveis pela camera.
    /// Adiciona 1 tile de margem para evitar pop-in nas bordas.
    pub fn fill_batch(&self, camera: &Camera2D, batch: &mut SpriteBatch) {
        let ts = self.tile_size;
        let vis = camera.visible_rect();
        let margin = ts;

        let x0 = ((vis.min.x - margin) / ts).floor() as i32;
        let x1 = ((vis.max.x + margin) / ts).ceil() as i32;
        let y0 = ((vis.min.y - margin) / ts).floor() as i32;
        let y1 = ((vis.max.y + margin) / ts).ceil() as i32;

        for ty in y0..=y1 {
            for tx in x0..=x1 {
                let id = self.get(tx, ty) as usize;
                if id == 0 {
                    continue;
                }
                let def = match self.tile_defs.get(id) {
                    Some(d) => d,
                    None => continue,
                };
                let world_pos = Vec2::new(tx as f32 * ts + ts * 0.5, ty as f32 * ts + ts * 0.5);
                batch.push(&Sprite {
                    position: world_pos,
                    size: Vec2::splat(ts),
                    uv_min: def.uv_min,
                    uv_max: def.uv_max,
                    ..Default::default()
                });
            }
        }
    }

    /// Converte posicao de mundo para coordenadas de tile.
    pub fn world_to_tile(&self, pos: Vec2) -> (i32, i32) {
        (
            (pos.x / self.tile_size).floor() as i32,
            (pos.y / self.tile_size).floor() as i32,
        )
    }

    /// AABB de um tile em coordenadas de mundo.
    pub fn tile_aabb(&self, tx: i32, ty: i32) -> Rect {
        let ts = self.tile_size;
        Rect {
            min: Vec2::new(tx as f32 * ts, ty as f32 * ts),
            max: Vec2::new((tx + 1) as f32 * ts, (ty + 1) as f32 * ts),
        }
    }
}
