//! 2dEngine — engine minima para MMO 2D pixel-art, escrita em Rust.
//!
//! # Arquitetura
//!
//! A engine e dividida em camadas finas, cada uma num modulo:
//!
//! - [`app`]  — event loop (winit), bootstrap do renderer, trait `Game`.
//! - [`render`] — renderer wgpu com batch de sprites instanciados.
//! - [`input`]  — estado unificado de teclado/mouse (touch no roadmap).
//! - [`time`]   — fixed timestep deterministico.
//! - [`ecs`]    — re-export do hecs + sistemas basicos.
//! - [`assets`] — carregamento de imagens/texturas.
//! - [`math`]   — re-export do glam + primitivas 2D.
//!
//! Rede nao esta na engine — fica no crate `client` (diferentes impls para
//! native/wasm) e no `server`. Isso mantem a engine portavel (wasm, iOS,
//! Android) sem puxar tokio.

pub mod app;
pub mod assets;
pub mod ecs;
pub mod input;
pub mod math;
pub mod render;
pub mod time;

// Re-exports para clientes do crate nao precisarem declarar glam/hecs/winit.
pub use anyhow;
pub use glam;
pub use hecs;
pub use tracing;
pub use winit;
