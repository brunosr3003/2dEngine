//! Componentes do ECS que aparecem tanto no servidor quanto no cliente.
//! Tudo aqui e Serialize/Deserialize para ir nos pacotes de rede.

use glam::Vec2;
use serde::{Deserialize, Serialize};

/// Identificador estavel de uma entidade **no contexto de rede** (networked
/// ID). Nao confundir com `hecs::Entity`, que e local ao ECS. O servidor
/// atribui EntityId no spawn e o reusa em todos os snapshots.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct EntityId(pub u32);

/// Identificador persistente de jogador (vai para o banco).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PlayerId(pub u64);

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct Position(pub Vec2);

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct Velocity(pub Vec2);

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct Health {
    pub current: i32,
    pub max: i32,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum EntityKind {
    Player,
    Enemy(u16),
    Projectile,
    Loot,
}

/// Snapshot de uma entidade enviado pelo servidor no tick.
/// Campos `Option` permitem delta (omitir quando nao mudaram).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EntitySnapshot {
    pub id: EntityId,
    pub kind: EntityKind,
    pub pos: Vec2,
    pub vel: Vec2,
    pub hp: Option<Health>,
    pub name: Option<String>,
}
