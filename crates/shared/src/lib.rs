//! Types shared between client and server.
//!
//! Nothing here depends on wgpu/winit/tokio — this crate compiles on every
//! target (native + wasm + headless) to keep the protocol canonical.

pub mod abissal;
pub mod acessorios;
pub mod ajuda;
pub mod aparencia;
pub mod arena;
pub mod armazem;
pub mod arvore;
pub mod auras;
pub mod bestiary;
pub mod bosses;
pub mod celeste;
pub mod chat;
pub mod chaves;
pub mod colonia;
pub mod combat;
pub mod combinar;
pub mod components;
pub mod constants;
pub mod construcao;
pub mod conves;
pub mod desafio;
pub mod dungeon;
pub mod escassez;
pub mod essencia;
pub mod eventos;
pub mod forja;
pub mod forte;
pub mod historia;
pub mod idioma;
pub mod items;
pub mod kogen;
pub mod ladder;
pub mod loja;
pub mod magica;
pub mod mapfile;
pub mod mar;
pub mod mercado;
pub mod montarias;
pub mod navio;
pub mod oasis;
pub mod pacotes_bonus;
pub mod pets;
pub mod physics;
pub mod pk;
pub mod planalto;
pub mod planta;
pub mod pocoes;
pub mod porao;
pub mod portal_ilhas;
pub mod precos;
pub mod presenca;
pub mod prime;
pub mod progressao;
pub mod protocol;
pub mod quests;
pub mod ranking;
pub mod receitas;
pub mod renascimento;
pub mod skills;
pub mod terreno;
pub mod troca;
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
