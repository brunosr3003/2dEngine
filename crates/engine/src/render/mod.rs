//! Renderer 2D baseado em wgpu. Design: um unico draw-call instanciado por
//! frame, com um atlas de texturas e um uniform de camera.
//!
//! Suficiente para dezenas de milhares de sprites por frame em hardware
//! modesto. Quando crescer, adicionar:
//! - multiplos atlases (bind por material)
//! - text rendering (glyph_brush)
//! - post-process (fullscreen pass)
//! - tilemap (instancia geometria especial por chunk)

pub mod camera;
pub mod renderer;
pub mod sprite;

pub use camera::Camera2D;
pub use renderer::Renderer;
pub use sprite::{Sprite, SpriteBatch};
