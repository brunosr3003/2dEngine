//! Types shared between client and server.
//!
//! Nothing here depends on wgpu/winit/tokio — this crate compiles on every
//! target (native + wasm + headless) to keep the protocol canonical.

pub mod armazem;
pub mod auras;
pub mod bestiary;
pub mod bosses;
pub mod chaves;
pub mod forte;
pub mod kogen;
pub mod abissal;
pub mod combat;
pub mod aparencia;
pub mod arena;
pub mod colonia;
pub mod celeste;
pub mod combinar;
pub mod components;
pub mod constants;
pub mod construcao;
pub mod desafio;
pub mod planta;
pub mod oasis;
pub mod dungeon;
pub mod porao;
pub mod ladder;
pub mod forja;
pub mod historia;
pub mod idioma;
pub mod acessorios;
pub mod items;
pub mod loja;
pub mod magica;
pub mod mapfile;
pub mod mercado;
pub mod physics;
pub mod montarias;
pub mod pets;
pub mod pocoes;
pub mod precos;
pub mod presenca;
pub mod protocol;
pub mod progressao;
pub mod quests;
pub mod receitas;
pub mod skills;
pub mod terreno;
pub mod planalto;
pub mod viagem;
pub mod vila;
pub mod world_gen;

pub use combat::*;
pub use components::*;
pub use constants::*;
pub use items::*;
pub use physics::*;
pub use skills::*;

/// Serde helper: serialises/deserialises `glam::Vec2` as a `[x, y]` array
/// for compatibility with the Unity client (float[]).
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
