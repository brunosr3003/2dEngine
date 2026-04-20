//! Protocolo de rede. Serializado com bincode (pequeno e rapido).
//! Toda mudanca em `ClientMessage`/`ServerMessage` DEVE bumpar
//! `PROTOCOL_VERSION` em `constants.rs`.

use crate::{EntityId, EntitySnapshot, PlayerId, PROTOCOL_VERSION};
use glam::Vec2;
use serde::{Deserialize, Serialize};

/// Cliente -> Servidor.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ClientMessage {
    /// Primeiro pacote; servidor responde com `HandshakeAck` ou `Kick`.
    Handshake {
        protocol_version: u16,
        client_version: String,
    },
    /// Login apos handshake. No scaffold usamos token stub; trocar por
    /// JWT/OAuth2/etc. em producao.
    Login {
        username: String,
        token: String,
    },
    /// Envio periodico de intent do jogador. O servidor e a autoridade —
    /// aqui so dizemos "quero mover para X, mirando em Y".
    Input(InputFrame),
    Chat(String),
    RequestDisconnect,
}

/// Input de um tick do cliente.
///
/// `seq` e o numero monotonico do input (para reconciliacao).
/// `tick` e o tick que o cliente *acredita* estar; serve como debug.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct InputFrame {
    pub seq: u32,
    pub tick: u32,
    pub move_dir: Vec2,
    pub aim: Vec2,
    pub buttons: u32,
}

pub mod buttons {
    pub const PRIMARY:   u32 = 1 << 0; // ataque basico
    pub const SECONDARY: u32 = 1 << 1; // habilidade
    pub const INTERACT:  u32 = 1 << 2; // E / tap
    pub const DASH:      u32 = 1 << 3;
}

/// Servidor -> Cliente.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ServerMessage {
    HandshakeAck {
        protocol_version: u16,
        server_time_ms: u64,
    },
    LoginOk {
        player_id: PlayerId,
        entity_id: EntityId,
        spawn: Vec2,
    },
    LoginDenied {
        reason: String,
    },
    Snapshot(WorldSnapshot),
    Chat {
        from: String,
        text: String,
    },
    Kick {
        reason: String,
    },
}

/// Replicacao do mundo. Enviado a cada tick (ou a cada N ticks para reduzir
/// banda — ver NETWORKING.md). `last_input_seq` confirma o ultimo input do
/// cliente que foi aplicado no servidor; cliente usa isso para descartar
/// inputs preditos que ja foram confirmados.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorldSnapshot {
    pub tick: u32,
    pub server_time_ms: u64,
    pub last_input_seq: u32,
    pub entities: Vec<EntitySnapshot>,
    pub removed: Vec<EntityId>,
}

pub fn encode<T: Serialize>(msg: &T) -> anyhow::Result<Vec<u8>> {
    Ok(bincode::serialize(msg)?)
}

pub fn decode<'a, T: Deserialize<'a>>(bytes: &'a [u8]) -> anyhow::Result<T> {
    Ok(bincode::deserialize(bytes)?)
}

pub fn version() -> u16 {
    PROTOCOL_VERSION
}
