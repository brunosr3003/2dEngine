//! Debug renderer: retangulos, circulos e pontos via SpriteBatch.
//!
//! Usa o pixel branco do atlas (UV (0,0)-(1,1) em atlas 1x1 gerado no init).
//! Para usar: coloque um pixel branco solid na posicao (0,0) do seu atlas,
//! ou use o atlas placeholder (sempre branco).
//!
//! Todas as formas sao so visiveis em builds debug (use #[cfg(debug_assertions)]).

use super::{Sprite, SpriteBatch};
use glam::{Vec2, Vec4};

/// UV do pixel branco do atlas. Ajustar se o atlas mudar de layout.
/// Por padrao aponta para (0,0)-(e,e) onde e e um epsilon pequeno.
const WHITE_UV_MIN: Vec2 = Vec2::ZERO;
const WHITE_UV_MAX: Vec2 = Vec2::new(0.0625, 0.0625); // 1/16 do atlas

pub struct DebugDraw;

impl DebugDraw {
    /// Retangulo solido.
    pub fn rect(batch: &mut SpriteBatch, min: Vec2, max: Vec2, color: Vec4) {
        let size = max - min;
        let center = (min + max) * 0.5;
        batch.push(&Sprite {
            position: center,
            size,
            uv_min: WHITE_UV_MIN,
            uv_max: WHITE_UV_MAX,
            tint: color,
            ..Default::default()
        });
    }

    /// Borda de retangulo (4 segmentos finos).
    pub fn rect_outline(batch: &mut SpriteBatch, min: Vec2, max: Vec2, thickness: f32, color: Vec4) {
        let t = thickness;
        // Bottom
        Self::rect(batch, Vec2::new(min.x, min.y), Vec2::new(max.x, min.y + t), color);
        // Top
        Self::rect(batch, Vec2::new(min.x, max.y - t), Vec2::new(max.x, max.y), color);
        // Left
        Self::rect(batch, Vec2::new(min.x, min.y), Vec2::new(min.x + t, max.y), color);
        // Right
        Self::rect(batch, Vec2::new(max.x - t, min.y), Vec2::new(max.x, max.y), color);
    }

    /// Circulo aproximado por N segmentos de linha radial.
    pub fn circle_outline(batch: &mut SpriteBatch, center: Vec2, radius: f32, thickness: f32, color: Vec4, segments: u32) {
        let n = segments.max(8) as usize;
        for i in 0..n {
            let a0 = (i as f32 / n as f32) * std::f32::consts::TAU;
            let a1 = ((i + 1) as f32 / n as f32) * std::f32::consts::TAU;
            let p0 = center + Vec2::new(a0.cos(), a0.sin()) * radius;
            let p1 = center + Vec2::new(a1.cos(), a1.sin()) * radius;
            Self::line(batch, p0, p1, thickness, color);
        }
    }

    /// Linha entre dois pontos (retangulo rotacionado).
    pub fn line(batch: &mut SpriteBatch, a: Vec2, b: Vec2, thickness: f32, color: Vec4) {
        let diff = b - a;
        let len = diff.length();
        if len < 0.0001 { return; }
        let angle = diff.y.atan2(diff.x);
        let center = (a + b) * 0.5;
        batch.push(&Sprite {
            position: center,
            size: Vec2::new(len, thickness),
            uv_min: WHITE_UV_MIN,
            uv_max: WHITE_UV_MAX,
            tint: color,
            rotation: angle,
            ..Default::default()
        });
    }

    /// Ponto (quadradinho).
    pub fn point(batch: &mut SpriteBatch, pos: Vec2, size: f32, color: Vec4) {
        batch.push(&Sprite {
            position: pos,
            size: Vec2::splat(size),
            uv_min: WHITE_UV_MIN,
            uv_max: WHITE_UV_MAX,
            tint: color,
            ..Default::default()
        });
    }

    /// Cruz em `pos` (util para debug de posicao de entidades).
    pub fn cross(batch: &mut SpriteBatch, pos: Vec2, size: f32, thickness: f32, color: Vec4) {
        let h = size * 0.5;
        Self::line(batch, pos - Vec2::X * h, pos + Vec2::X * h, thickness, color);
        Self::line(batch, pos - Vec2::Y * h, pos + Vec2::Y * h, thickness, color);
    }
}
