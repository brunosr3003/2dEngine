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
    /// Login apos handshake. O servidor valida `password` contra a tabela
    /// `accounts` (argon2id). Ha uma pagina web para cadastro.
    Login {
        username: String,
        password: String,
    },
    /// Envio periodico de intent do jogador. O servidor e a autoridade —
    /// aqui so dizemos "quero mover para X, mirando em Y".
    Input(InputFrame),
    Chat(String),
    /// Ping com timestamp do cliente (ms). Servidor responde com `Pong` carregando
    /// o mesmo valor para o cliente medir RTT.
    Ping { client_time_ms: u64 },
    /// Usa o item no slot `slot` do inventario. Servidor aplica o efeito e
    /// decrementa a quantidade.
    UseItem { slot: u16 },
    /// Interacao com o NPC mais proximo (dentro de INTERACT_RADIUS). Pro
    /// vendedor, abre a loja.
    Interact,
    /// Compra o item em `slot_idx` da tabela SHOP_ITEMS.
    ShopBuy { slot_idx: u8 },
    /// Deposita o item do `inv_slot` no proximo slot livre do vault.
    /// So funciona se o vault esta aberto (interagiu com NPC(2)).
    VaultDeposit { inv_slot: u16 },
    /// Retira o item do `vault_slot` para o proximo slot livre do inv.
    VaultWithdraw { vault_slot: u16 },
    /// Fecha o vault (cliente saindo do vault UI).
    VaultClose,
    /// Swap generico entre dois 'spots' do inventario + equipamento. Ver
    /// `InvSpot` pra semantica. Server valida (tipo de equip, etc) e aplica.
    InventorySwap { a: InvSpot, b: InvSpot },
    RequestDisconnect,
}

/// Localizacao logica de um slot no sistema de inventario do cliente.
/// Inv(idx) = slot do grid (0..INVENTORY_SLOTS). Os primeiros 6 slots sao
/// a hotbar; os demais sao a mochila. Equip(slot) = slot de equipamento.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum InvSpot {
    Inv(u16),
    Equip(crate::constants::EquipSlot),
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
    /// Resposta a `ClientMessage::Ping`. Carrega o mesmo `client_time_ms` para
    /// o cliente calcular RTT sem manter estado.
    Pong {
        client_time_ms: u64,
        server_time_ms: u64,
    },
    /// Jogador foi movido para outro mapa (via portal ou first spawn).
    /// Cliente deve: limpar entidades interpoladas, recarregar tilemap, reset prediction.
    MapChange {
        map_name: String,
        width: u32,
        height: u32,
        tiles: Vec<u16>,
        spawn: Vec2,
        safe_zone: bool,
    },
    /// Atualizacao de progresso do proprio jogador (XP, level). Enviada
    /// no login e sempre que XP mudar.
    ProgressUpdate {
        xp: u64,
        level: u32,
    },
    /// Estado completo do inventario. Enviado no login e a cada mudanca.
    /// `slots[i].qty == 0` = slot vazio.
    InventoryUpdate {
        slots: Vec<crate::InventorySlot>,
    },
    /// Stats do jogador (classe + bonus de equipamento aplicados) +
    /// slots de equipamento. Enviado no login e a cada mudanca.
    StatsUpdate {
        class: crate::PlayerClass,
        stats: crate::PlayerStats,
        equipment: crate::Equipment,
    },
    /// MP atual do jogador. Enviado ao login, no gasto e periodicamente
    /// conforme a regeneracao muda em +/-1.
    ManaUpdate { current: i32 },
    /// Stamina atual do jogador. Mesma logica do ManaUpdate.
    StaminaUpdate { current: i32 },
    /// Abre a loja do vendedor — lista de (item_id, preco).
    ShopOpen { items: Vec<(u16, u32)> },
    /// Fecha a loja (jogador saiu do alcance ou cancelou).
    ShopClose,
    /// Abre o vault — envia os slots atuais. Cliente passa a mostrar UI.
    VaultOpen { slots: Vec<crate::InventorySlot> },
    /// Update apos deposit/withdraw — cliente atualiza UI.
    VaultUpdate { slots: Vec<crate::InventorySlot> },
    /// Fecha o vault (jogador afastou-se ou cancelou).
    VaultClose,
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
