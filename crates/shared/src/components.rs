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
    /// NPC estatico interagivel (vendedor). u16 = npc_id (1=vendor).
    Npc(u16),
}

/// Slot de inventario. None = vazio. Quando `qty == 0`, o slot esta vazio.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default)]
pub struct InventorySlot {
    pub item_id: u16,
    pub qty: u32,
}

/// Classes disponiveis. A classe define stats base e dano dos projeteis.
/// Persistida em `accounts.class` e imutavel pos-cadastro (por ora).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PlayerClass {
    Warrior,
    Archer,
    Wizard,
}

impl PlayerClass {
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_ascii_lowercase().as_str() {
            "warrior" => Some(Self::Warrior),
            "archer"  => Some(Self::Archer),
            "wizard"  => Some(Self::Wizard),
            _         => None,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Warrior => "warrior",
            Self::Archer  => "archer",
            Self::Wizard  => "wizard",
        }
    }

    /// Stats iniciais derivados da classe.
    pub fn base_stats(&self) -> PlayerStats {
        match self {
            Self::Warrior => PlayerStats { hp_max: 140, mp_max:  20, dex:  8, wis:  6, attack_damage: 30 },
            Self::Archer  => PlayerStats { hp_max:  90, mp_max:  40, dex: 14, wis:  8, attack_damage: 22 },
            Self::Wizard  => PlayerStats { hp_max:  70, mp_max: 100, dex:  6, wis: 14, attack_damage: 45 },
        }
    }
}

/// Bloco de stats numericos do jogador. Pode ser sobrescrito por equipamentos
/// futuros — enviado pro cliente pra HUD.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct PlayerStats {
    pub hp_max: i32,
    pub mp_max: i32,
    pub dex: i32,
    pub wis: i32,
    pub attack_damage: i32,
}

/// Slots de equipamento. None = vazio; Some(item_id) = item equipado.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default)]
pub struct Equipment {
    pub weapon: Option<u16>,
    pub armor: Option<u16>,
    pub ring: Option<u16>,
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

#[derive(Clone, Copy, Debug)]
pub struct PhysicsHandle(pub rapier2d::prelude::RigidBodyHandle);
