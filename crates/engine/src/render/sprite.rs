use bytemuck::{Pod, Zeroable};
use glam::{Vec2, Vec4};

/// Sprite simples renderizavel. Coordenadas em mundo (tiles). O atlas
/// e referenciado por UVs pre-computadas — use `Atlas::get` para obte-las.
#[derive(Debug, Clone, Copy)]
pub struct Sprite {
    pub position: Vec2,
    pub size: Vec2,
    pub uv_min: Vec2,
    pub uv_max: Vec2,
    pub tint: Vec4,
    pub rotation: f32,
}

impl Default for Sprite {
    fn default() -> Self {
        Self {
            position: Vec2::ZERO,
            size: Vec2::ONE,
            uv_min: Vec2::ZERO,
            uv_max: Vec2::ONE,
            tint: Vec4::ONE,
            rotation: 0.0,
        }
    }
}

/// Layout do instance buffer. 64 bytes alinhados — importante para o GPU.
#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
pub(crate) struct SpriteInstance {
    pub position: [f32; 2],
    pub size: [f32; 2],
    pub uv_min: [f32; 2],
    pub uv_max: [f32; 2],
    pub tint: [f32; 4],
    pub rotation: f32,
    pub _pad: [f32; 3],
}

pub struct SpriteBatch {
    pub(crate) instances: Vec<SpriteInstance>,
}

impl SpriteBatch {
    pub fn new() -> Self { Self { instances: Vec::with_capacity(4096) } }

    pub fn clear(&mut self) { self.instances.clear(); }

    pub fn push(&mut self, s: &Sprite) {
        self.instances.push(SpriteInstance {
            position: s.position.to_array(),
            size: s.size.to_array(),
            uv_min: s.uv_min.to_array(),
            uv_max: s.uv_max.to_array(),
            tint: s.tint.to_array(),
            rotation: s.rotation,
            _pad: [0.0; 3],
        });
    }

    pub fn len(&self) -> usize { self.instances.len() }
    pub fn is_empty(&self) -> bool { self.instances.is_empty() }
}

impl Default for SpriteBatch {
    fn default() -> Self { Self::new() }
}
