//! Tipos compartilhados entre cliente e servidor.
//!
//! Nada aqui depende de wgpu/winit/tokio — este crate compila em todos os
//! targets (native + wasm + headless) para manter o protocolo canonico.

pub mod armazem;
pub mod bosses;
pub mod chaves;
pub mod combat;
pub mod aparencia;
pub mod colonia;
pub mod combinar;
pub mod components;
pub mod constants;
pub mod construcao;
pub mod dungeon;
pub mod forja;
pub mod historia;
pub mod items;
pub mod loja;
pub mod mapfile;
pub mod mercado;
pub mod physics;
pub mod montarias;
pub mod pets;
pub mod pocoes;
pub mod presenca;
pub mod protocol;
pub mod quests;
pub mod receitas;
pub mod skills;
pub mod terreno;
pub mod viagem;
pub mod vila;
pub mod world_gen;

pub use combat::*;
pub use components::*;
pub use constants::*;
pub use items::*;
pub use physics::*;
pub use skills::*;

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

pub mod social;
