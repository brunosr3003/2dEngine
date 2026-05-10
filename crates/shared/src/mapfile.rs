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
    /// Area dedicada a UM boss procedural (level-based). Spawna 1 boss
    /// dentro do polygon/rect, monitora morte, respawna apos `respawn_s`.
    /// Independente de EnemySpawner — separar boss spawn de mob spawn
    /// permite distribuir bosses em locais fixos sem misturar com hordas.
    /// `pos` (do placement) = canto inferior-esquerdo do AABB.
    BossSpawn {
        size: [f32; 2],
        /// Level do boss. Class e' sorteada do tier desse level.
        level: u32,
        /// Segundos apos morte ate o proximo spawn.
        respawn_s: f32,
        /// Vertices do poligono em coords LOCAIS (relativo a `pos`). None =
        /// area = rect AABB inteiro.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        polygon: Option<Vec<[f32; 2]>>,
    },
    /// Zona de spawn gerenciada: mantem quotas de inimigos vivos, respawna
    /// com delay configuravel. `pos` (do placement) e' o canto inferior-esquerdo
    /// do AABB; `size` e' a extensao do AABB em tiles. Se `polygon` Some,
    /// vertices (em coords locais) definem a area exata via rejection
    /// sampling + point-in-polygon; senao zona = rect AABB. `quotas` define
    /// quantos vivos manter por kind. `respawn_delay_s` apos morte.
    EnemySpawner {
        size: [f32; 2],
        quotas: Vec<SpawnQuota>,
        respawn_delay_s: f32,
        /// Vertices do poligono em coords LOCAIS (relativo a `pos` do
        /// placement). None = zona e' o rect AABB inteiro (legacy).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        polygon: Option<Vec<[f32; 2]>>,
        /// Modo level-range — quando Some(min) e Some(max), server ignora
        /// `quotas` e sortea inimigos aleatorios de level no range,
        /// mantendo `count` vivos.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        level_min: Option<u32>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        level_max: Option<u32>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        count: Option<u32>,
    },
    /// Zona segura: dentro dela combate é desabilitado (sem dano dado nem
    /// recebido), enemies dropam aggro de quem entra. `pos` (do placement) é
    /// o canto inferior-esquerdo; `size` é a extensão em tiles.
    SafeZone {
        size: [f32; 2],
    },
    /// Vendedor com loja própria. `shop_id` referencia uma row em
    /// `vendor_shops` no DB. `skin` é o índice do preset visual.
    Vendor {
        name:    String,
        shop_id: u32,
        skin:    u8,
    },
    /// NPC ambiental que anda por uma rota pré-definida. `route_id` referencia
    /// uma `NpcRoute` no MAPA (definida via NpcRoute entity). `skin` é o preset.
    WanderNpc {
        name:     String,
        route_id: u32,
        skin:     u8,
    },
    /// Ferreiro — refina itens e encrava gemas em sockets. `skin` é o preset.
    Blacksmith {
        name: String,
        skin: u8,
    },
    /// Definição de uma rota nomeada — lista ordenada de waypoints absolutos
    /// (em coords de tile). NPCs com WanderNpc { route_id } seguem em loop.
    NpcRoute {
        id:        u32,
        waypoints: Vec<[f32; 2]>,
    },
    /// Nó de recurso coletável (farming). Servidor gerencia HP, drops e respawn.
    /// `respawn_seconds`: override do default `FARM_NODE_RESPAWN_S` por tipo/tier.
    FarmNode {
        kind: String,
        tier: u32,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        respawn_seconds: Option<f32>,
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
