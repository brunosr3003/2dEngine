//! Tipos compartilhados entre cliente e servidor.
//!
//! Nada aqui depende de wgpu/winit/tokio — este crate compila em todos os
//! targets (native + wasm + headless) para manter o protocolo canonico.

pub mod components;
pub mod constants;
pub mod mapfile;
pub mod protocol;
pub mod world_gen;
pub mod physics;

pub use components::*;
pub use constants::*;
pub use physics::*;

/// Serde helper: serializa/deserializa `glam::Vec2` como array `[x, y]`
/// para compatibilidade com o cliente Unity (float[]).
pub mod vec2_arr {
    use glam::Vec2;
    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    pub fn serialize<S: Serializer>(v: &Vec2, s: S) -> Result<S::Ok, S::Error> {
        [v.x, v.y].serialize(s)
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Vec2, D::Error> {
        let [x, y] = <[f32; 2]>::deserialize(d)?;
        Ok(Vec2::new(x, y))
    }
}
