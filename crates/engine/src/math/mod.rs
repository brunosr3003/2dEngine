//! Utilitarios de matematica 2D. Re-exporta tipos do glam para conveniencia.

pub use glam::{IVec2, Mat4, UVec2, Vec2, Vec3, Vec4};

/// Retangulo eixo-alinhado em coordenadas de mundo.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rect {
    pub min: Vec2,
    pub max: Vec2,
}

impl Rect {
    pub fn from_center_size(center: Vec2, size: Vec2) -> Self {
        let h = size * 0.5;
        Self { min: center - h, max: center + h }
    }

    pub fn from_min_size(min: Vec2, size: Vec2) -> Self {
        Self { min, max: min + size }
    }

    pub fn size(&self) -> Vec2 { self.max - self.min }
    pub fn center(&self) -> Vec2 { (self.min + self.max) * 0.5 }

    pub fn contains(&self, p: Vec2) -> bool {
        p.x >= self.min.x && p.x <= self.max.x
            && p.y >= self.min.y && p.y <= self.max.y
    }

    pub fn intersects(&self, other: &Rect) -> bool {
        self.min.x <= other.max.x && self.max.x >= other.min.x
            && self.min.y <= other.max.y && self.max.y >= other.min.y
    }
}
