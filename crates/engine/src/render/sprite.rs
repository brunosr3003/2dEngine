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
    /// Profundidade de desenho. Menor valor desenha primeiro (atras). Use para
    /// z-ordering top-down: entidades com `y` menor (mais baixas na tela)
    /// devem ficar na frente ⇒ depth = LAYER + (-pos.y).
    pub depth: f32,
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
            depth: 0.0,
        }
    }
}

pub mod layer {
    /// Camadas base — combine com `-pos.y` para sort top-down dentro da camada.
    pub const TILEMAP:  f32 = 0.0;
    pub const SHADOW:   f32 = 1000.0;
    pub const ENTITY:   f32 = 2000.0;
    pub const HP_BAR:   f32 = 3000.0;
    pub const NAMEPLATE: f32 = 4000.0;
    pub const HUD:      f32 = 10_000.0;
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
    pub(crate) depths: Vec<f32>,
}

impl SpriteBatch {
    pub fn new() -> Self {
        Self {
            instances: Vec::with_capacity(4096),
            depths: Vec::with_capacity(4096),
        }
    }

    pub fn clear(&mut self) {
        self.instances.clear();
        self.depths.clear();
    }

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
        self.depths.push(s.depth);
    }

    /// Ordena as instancias por `depth` ascendente, mantendo estavel a ordem
    /// de insercao quando `depth` e igual. Chamar antes do flush.
    pub fn sort_by_depth(&mut self) {
        // Sort indirect para preservar par (instancia, depth).
        let n = self.instances.len();
        if n <= 1 { return; }
        let mut idx: Vec<usize> = (0..n).collect();
        idx.sort_by(|&a, &b| {
            self.depths[a]
                .partial_cmp(&self.depths[b])
                .unwrap_or(std::cmp::Ordering::Equal)
                .then(a.cmp(&b))
        });
        let new_inst: Vec<SpriteInstance> = idx.iter().map(|&i| self.instances[i]).collect();
        let new_dep: Vec<f32> = idx.iter().map(|&i| self.depths[i]).collect();
        self.instances = new_inst;
        self.depths = new_dep;
    }

    pub fn len(&self) -> usize { self.instances.len() }
    pub fn is_empty(&self) -> bool { self.instances.is_empty() }
}

impl Default for SpriteBatch {
    fn default() -> Self { Self::new() }
}
