//! Tipos compartilhados entre cliente e servidor.
//!
//! Nada aqui depende de wgpu/winit/tokio — este crate compila em todos os
//! targets (native + wasm + headless) para manter o protocolo canonico.

pub mod components;
pub mod constants;
pub mod protocol;
pub mod world_gen;
pub mod physics;

pub use components::*;
pub use constants::*;
pub use physics::*;
