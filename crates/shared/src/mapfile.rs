//! Formato de mapa serializavel (ferramentas externas <-> servidor).
//!
//! Um MapFile descreve um mapa estatico: grid de tiles + entidades
//! "pre-posicionadas" (spawns de inimigos, portais, NPCs). Salvo via
//! MessagePack (rmp-serde named) em disco.
//!
//! O servidor carrega um MapFile por "mapa" (nexus, dungeon X, etc).

use serde::{Deserialize, Serialize};

/// Tipo de entidade pre-posicionada num MapFile.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MapEntity {
    /// Spawn de inimigo com kind (ver `ENEMY_KINDS`).
    Enemy { kind: u16 },
    /// Portal para outro mapa. `target_map` e o nome do arquivo (sem extensao).
    Portal { target_map: String, target_spawn: [f32; 2] },
    /// NPC vendedor.
    Npc { name: String },
    /// Vault / bau persistente (NPC especial no nexus).
    Vault,
    /// Boss unico (so 1 por mapa, respawn via constante).
    Boss { kind: u16 },
    /// Zona de spawn gerenciada: mantem quotas de inimigos vivos, respawna
    /// com delay configuravel. `pos` (do placement) e' o canto inferior-esquerdo;
    /// `size` e' a extensao em tiles. `quotas` define quantos vivos manter por
    /// kind. `respawn_delay_s` e' o tempo apos morte antes de spawnar de novo.
    EnemySpawner {
        size: [f32; 2],
        quotas: Vec<SpawnQuota>,
        respawn_delay_s: f32,
    },
}

/// Quota de inimigos por kind dentro de uma EnemySpawner.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpawnQuota {
    pub kind: u16,
    pub count: u32,
}

/// Entidade posicionada num ponto do mapa.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MapEntityPlacement {
    pub pos: [f32; 2],
    pub entity: MapEntity,
}

/// Mapa completo. Unidades em tiles; posicoes de entidade em tile-coords.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MapFile {
    /// Nome do mapa (identificador para portais).
    pub name: String,
    pub width: u32,
    pub height: u32,
    pub tiles: Vec<u16>,
    /// Ponto de spawn default quando o jogador entra sem `target_spawn`.
    pub spawn: [f32; 2],
    /// Se true, combate e desabilitado (safe zone tipo nexus).
    pub safe_zone: bool,
    /// Entidades pre-posicionadas.
    pub entities: Vec<MapEntityPlacement>,
}

impl MapFile {
    pub fn new(name: impl Into<String>, width: u32, height: u32) -> Self {
        Self {
            name: name.into(),
            width,
            height,
            tiles: vec![crate::constants::tile_id::FLOOR; (width * height) as usize],
            spawn: [width as f32 * 0.5, height as f32 * 0.5],
            safe_zone: false,
            entities: Vec::new(),
        }
    }

    pub fn get(&self, x: i32, y: i32) -> u16 {
        if x < 0 || y < 0 || x >= self.width as i32 || y >= self.height as i32 {
            return crate::constants::tile_id::WALL;
        }
        self.tiles[(y as u32 * self.width + x as u32) as usize]
    }

    pub fn set(&mut self, x: i32, y: i32, id: u16) {
        if x < 0 || y < 0 || x >= self.width as i32 || y >= self.height as i32 { return; }
        self.tiles[(y as u32 * self.width + x as u32) as usize] = id;
    }

    pub fn to_bytes(&self) -> anyhow::Result<Vec<u8>> {
        Ok(serde_json::to_vec(self)?)
    }

    pub fn from_bytes(data: &[u8]) -> anyhow::Result<Self> {
        Ok(serde_json::from_slice(data)?)
    }

    /// Carrega de disco.
    pub fn load(path: impl AsRef<std::path::Path>) -> anyhow::Result<Self> {
        let data = std::fs::read(path)?;
        Self::from_bytes(&data)
    }

    /// Salva em disco.
    pub fn save(&self, path: impl AsRef<std::path::Path>) -> anyhow::Result<()> {
        let data = self.to_bytes()?;
        std::fs::write(path, data)?;
        Ok(())
    }

    /// Converte pra WorldMap (usado pelo cliente/servidor p/ colisao).
    pub fn to_world_map(&self) -> crate::world_gen::WorldMap {
        crate::world_gen::WorldMap {
            width: self.width,
            height: self.height,
            tiles: self.tiles.clone(),
            override_spawn: None,
        }
    }
}
