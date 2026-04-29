//! Protocolo de rede. Serializado com JSON (serde_json).
//! Compativel com Newtonsoft.Json no Unity (campo `type` como discriminante).
//! Toda mudanca em `ClientMessage`/`ServerMessage` DEVE bumpar
//! `PROTOCOL_VERSION` em `constants.rs`.

use crate::{EntityId, EntitySnapshot, PlayerId, PROTOCOL_VERSION};
use serde::{Deserialize, Serialize};

/// Cliente -> Servidor.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ClientMessage {
    /// Primeiro pacote; servidor responde com `HandshakeAck` ou `Kick`.
    Handshake {
        protocol_version: u16,
        client_version: String,
    },
    /// Login apos handshake.
    Login {
        username: String,
        password: String,
    },
    /// Envio periodico de intent do jogador.
    Input {
        input: InputFrame,
    },
    Chat {
        text: String,
    },
    Ping { client_time_ms: u64 },
    UseItem { slot: u16 },
    Interact,
    ShopBuy { slot_idx: u8 },
    ShopSell { inv_slot: u16 },
    /// Trade atômico — todas as compras E vendas executadas juntas, ou
    /// nada. Cliente preview-only; servidor valida tudo de novo.
    ShopTrade { buying: Vec<TradeBuyEntry>, selling: Vec<TradeSellEntry> },
    VaultDeposit { inv_slot: u16 },
    VaultWithdraw { vault_slot: u16 },
    VaultClose,
    InventorySwap { a: InvSpot, b: InvSpot },
    StandUp,
    TeleportToVendor,
    PartyInvite { target_name: String },
    PartyAccept,
    PartyDecline,
    PartyLeave,
    /// Aloca 1 ponto de atributo. `stat` indice em [0=FOR,1=DES,2=INT,3=VIT,4=SPD].
    AllocStatPoint { stat: u8 },
    /// Reseta TODOS os pontos alocados pra unspent_points. Util pra testes
    /// e respec — server zera o array, devolve os pontos e reenvia stats.
    ResetStats,
    /// Refina um item do inventário (+1 nível). Requer ItemInstance
    /// presente no slot. Custo: gold proporcional ao refinement atual.
    /// Falha (chance crescente com nível) reseta refinement pra 0.
    RefineItem { slot: u16 },
    /// Encrava uma gema (`gem_slot`) em um socket livre do item em
    /// `item_slot`. Requer ItemInstance com `sockets > 0`. Gema é
    /// consumida do inventário. Falha silenciosamente se sem socket
    /// livre ou item incompatível.
    SocketGem { item_slot: u16, gem_slot: u16 },
    RequestDisconnect,
}

/// Localizacao logica de um slot no sistema de inventario do cliente.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum InvSpot {
    Inv(u16),
    Equip(crate::constants::EquipSlot),
}

/// Input de um tick do cliente.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct InputFrame {
    pub seq: u32,
    pub tick: u32,
    #[serde(with = "crate::vec2_arr")]
    pub move_dir: glam::Vec2,
    #[serde(with = "crate::vec2_arr")]
    pub aim: glam::Vec2,
    pub buttons: u32,
}

pub mod buttons {
    pub const PRIMARY:   u32 = 1 << 0;
    pub const SECONDARY: u32 = 1 << 1;
    pub const INTERACT:  u32 = 1 << 2;
    pub const DASH:      u32 = 1 << 3;
    /// Shift held = sprint (multiplica speed por SPRINT_SPEED_MULT enquanto
    /// drena stamina). Ignorado se stamina<=0 ou defendendo.
    pub const SPRINT:    u32 = 1 << 4;
}

/// Servidor -> Cliente.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ServerMessage {
    HandshakeAck {
        protocol_version: u16,
        server_time_ms: u64,
    },
    LoginOk {
        player_id: PlayerId,
        entity_id: EntityId,
        spawn: [f32; 2],
    },
    LoginDenied {
        reason: String,
    },
    #[serde(rename = "WorldSnapshot")]
    Snapshot {
        snapshot: WorldSnapshot,
    },
    Chat {
        from: String,
        text: String,
    },
    Pong {
        client_time_ms: u64,
        server_time_ms: u64,
    },
    MapChange {
        map_name: String,
        width: u32,
        height: u32,
        tiles: Vec<u16>,
        spawn: [f32; 2],
        safe_zone: bool,
        #[serde(default)]
        decorations: Vec<crate::world_gen::DecoPlacement>,
        /// Retângulos de zona segura do mapa (origem inferior-esquerda + tamanho).
        /// Cliente renderiza tint/borda; gameplay (sem dano etc) é decidido
        /// pelo servidor mas a UI ajuda o jogador a saber onde tá.
        #[serde(default)]
        safe_zones: Vec<SafeZoneRect>,
    },
    ProgressUpdate {
        xp: u64,
        level: u32,
    },
    InventoryUpdate {
        slots: Vec<crate::InventorySlot>,
    },
    StatsUpdate {
        stats: crate::PlayerStats,
        equipment: crate::Equipment,
    },
    ManaUpdate { current: i32 },
    StaminaUpdate { current: i32 },
    ShopOpen {
        items:        Vec<ShopItem>,
        sell_prices:  Vec<SellPrice>,
        /// Identificador opaco do vendor (NPC). Usado pelo cliente como
        /// chave pra cache visual (nome, retrato, reputação no futuro).
        vendor_id:    u32,
        /// Multiplicador aplicado em compras (preço final = preço * mult).
        /// Default 1.0 — futuro: depende da relação com vendor.
        buy_mult:     f32,
        /// Multiplicador aplicado em vendas. Default 1.0.
        sell_mult:    f32,
    },
    ShopClose,
    ShopTradeResult { ok: bool, reason: String },
    VaultOpen { slots: Vec<crate::InventorySlot> },
    VaultUpdate { slots: Vec<crate::InventorySlot> },
    VaultClose,
    /// Sinaliza ao cliente abrir o painel do ferreiro (refinar + socket gem).
    /// Sem payload — cliente apenas mostra a UI.
    BlacksmithOpen,
    BlacksmithClose,
    DownedUpdate { active: bool, dhp: i32, dhp_max: i32, timer_s: f32 },
    FameUpdate { fame: u64 },
    AuraUpdate { aura: u64 },
    ProficienciesUpdate {
        #[serde(rename = "proficiency_xp")]
        xp: [u64; 6],
    },
    PartyInviteReceived { from: String },
    PartyUpdate { members: Vec<String> },
    /// Pontos de atributo disponiveis + ja alocados em cada stat.
    /// `allocated[i]` = pontos no stat com indice `i` (0=FOR..4=SPD, ver `crate::stat_idx`).
    StatPointsUpdate {
        unspent: u32,
        allocated: [u32; crate::STAT_COUNT],
    },
    Kick {
        reason: String,
    },
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct ShopItem {
    pub item_id: u16,
    pub price: u32,
}

/// Preço de venda de um item arbitrário (todos os itens vendáveis vêm na
/// abertura do shop pra UI mostrar custos antes do clique).
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct SellPrice {
    pub item_id: u16,
    pub price:   u32,
}

/// Entrada do basket de compras dentro de um ShopTrade.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct TradeBuyEntry {
    pub shop_slot: u8,
    pub qty:       u32,
}

/// Entrada do basket de vendas dentro de um ShopTrade.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct TradeSellEntry {
    pub inv_slot: u16,
    pub qty:      u32,
}

/// Retângulo de zona segura enviado ao cliente pra renderização.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct SafeZoneRect {
    pub x:      f32,
    pub y:      f32,
    pub width:  f32,
    pub height: f32,
}

/// Resultado de um ShopTrade — sucesso ou erro com motivo amigável.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShopTradeResult {
    pub ok:     bool,
    pub reason: String,
}

/// Replicacao do mundo enviada a cada tick.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorldSnapshot {
    pub tick: u32,
    pub server_time_ms: u64,
    pub last_input_seq: u32,
    pub entities: Vec<EntitySnapshot>,
    pub removed: Vec<EntityId>,
}

pub fn encode<T: Serialize>(msg: &T) -> anyhow::Result<Vec<u8>> {
    Ok(serde_json::to_vec(msg)?)
}

pub fn decode<T: for<'de> serde::Deserialize<'de>>(bytes: &[u8]) -> anyhow::Result<T> {
    Ok(serde_json::from_slice(bytes)?)
}

pub fn version() -> u16 {
    PROTOCOL_VERSION
}
