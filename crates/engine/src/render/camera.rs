use crate::math::Rect;
use glam::{Mat4, Vec2};

/// Camera ortografica 2D. `zoom` e em pixels-por-tile — zoom=32 significa
/// cada tile ocupa 32px na tela.
pub struct Camera2D {
    pub position: Vec2,
    pub zoom: f32,
    pub viewport: Vec2,
}

impl Camera2D {
    pub fn new(viewport: Vec2) -> Self {
        Self { position: Vec2::ZERO, zoom: 32.0, viewport }
    }

    pub fn view_proj(&self) -> Mat4 {
        let half = self.viewport * 0.5 / self.zoom;
        let proj = Mat4::orthographic_rh(-half.x, half.x, -half.y, half.y, -1.0, 1.0);
        let view = Mat4::from_translation((-self.position).extend(0.0));
        proj * view
    }

    /// Retangulo visivel em coordenadas de mundo (com margem de 1 tile).
    pub fn visible_rect(&self) -> Rect {
        let half = self.viewport * 0.5 / self.zoom;
        Rect { min: self.position - half, max: self.position + half }
    }

    /// Converte coordenada de tela (pixels, origem top-left) para mundo.
    pub fn screen_to_world(&self, screen: Vec2) -> Vec2 {
        let ndc = Vec2::new(
            (screen.x / self.viewport.x) * 2.0 - 1.0,
            1.0 - (screen.y / self.viewport.y) * 2.0,
        );
        let half = self.viewport * 0.5 / self.zoom;
        Vec2::new(ndc.x * half.x, ndc.y * half.y) + self.position
    }
}
