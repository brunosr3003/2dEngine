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
    /// Loot drop. Carrega o item_id (ver constants::item_id) para o cliente
    /// diferenciar cor/sprite sem precisar de uma tabela separada.
    Loot(u16),
    /// NPC estatico interagivel (vendedor). u16 = npc_id (1=vendor, 2=vault).
    Npc(u16),
    /// Portal para outro mapa. Ao pisar, servidor teleporta o jogador.
    Portal,
}

/// Slot de inventario. None = vazio. Quando `qty == 0`, o slot esta vazio.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default)]
pub struct InventorySlot {
    pub item_id: u16,
    pub qty: u32,
}

/// Stats iniciais. Todos os jogadores comecam iguais (classless por
/// proficiencia; a diferenciacao vem do equipamento + prof XP).
pub const fn base_player_stats() -> PlayerStats {
    PlayerStats {
        hp_max: 100,
        mp_max: 50,
        dex: 10,
        wis: 10,
        attack_damage: 20,
        defense: 0,
    }
}

/// Bloco de stats numericos do jogador. Sobrescrito por equipamentos —
/// enviado pro cliente pra HUD/painel de status.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct PlayerStats {
    pub hp_max: i32,
    pub mp_max: i32,
    pub dex: i32,
    pub wis: i32,
    pub attack_damage: i32,
    /// Resistencia. Reduz dano recebido: `dano_real = max(1, dmg - defense)`.
    pub defense: i32,
}

/// Slots de equipamento. None = vazio; Some(item_id) = item equipado.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default)]
pub struct Equipment {
    pub weapon: Option<u16>,
    pub armor: Option<u16>,
    pub ring: Option<u16>,
}

/// Snapshot de uma entidade enviado pelo servidor no tick.
/// Compativel com Protocol.cs do Unity (campos snake_case, Vec2 como [x,y]).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EntitySnapshot {
    pub id: EntityId,
    /// Nome da variante de EntityKind como string ("Player", "Enemy", etc.)
    pub kind: String,
    #[serde(with = "crate::vec2_arr")]
    pub pos: Vec2,
    #[serde(with = "crate::vec2_arr")]
    pub vel: Vec2,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hp: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hp_max: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sprite_id: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_self: Option<bool>,
    /// Inimigo iniciou um swing de melee neste tick. Cliente toca anim de
    /// ataque ao receber. None nas outras snapshots.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub attacking: Option<bool>,
}

#[derive(Clone, Copy, Debug)]
pub struct PhysicsHandle(pub rapier2d::prelude::RigidBodyHandle);

/// Identifica em qual "mapa logico" uma entidade esta.
///
/// HOJE (v1): tudo fica em "overworld"; portais teleportam dentro desse
/// mesmo espaco. A tag existe como preparacao pro refactor futuro onde
/// cada mapa tera physics/ECS isolados (ver TODO em server::world).
#[derive(Clone, Debug)]
pub struct MapId(pub String);

impl MapId {
    pub fn overworld() -> Self { Self("overworld".into()) }
}
