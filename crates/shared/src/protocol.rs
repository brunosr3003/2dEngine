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
    /// Aprende uma skill (rank 0 → 1) gastando 1 SP. Server valida
    /// unlock_char_lvl + unlock_prof_lvl + SP suficiente.
    SkillLearn { skill_id: u32 },
    /// Sobe rank de uma skill já aprendida. Custo varia por rank (ver
    /// `SP_COST_PER_RANK`). Falha se rank == MAX_SKILL_RANK.
    SkillRankUp { skill_id: u32 },
    /// Equipa skill ativa em slot 0..=5. `slot=None` ou `skill_id=0` desequipa.
    /// Passivas ignoram esse req (sempre ativas se aprendidas).
    SkillEquip { skill_id: u32, slot: Option<u8> },
    /// Dispara cast de skill ativa. `target_pos` = world position do mouse
    /// (mira pra projectile/AoE). Server valida cd/cost/weapon e dispatch
    /// pelo target_type da SkillDef.
    SkillCast {
        skill_id: u32,
        #[serde(with = "crate::vec2_arr")]
        target_pos: glam::Vec2,
    },
    /// Reseta TODOS os pontos alocados pra unspent_points. Util pra testes
    /// e respec — server zera o array, devolve os pontos e reenvia stats.
    ResetStats,
    /// Reseta TODAS as skills aprendidas — refunda os SP gastos. Limpa
    /// learned_skills, equipped slots, cooldowns e estados de skills (riposte,
    /// hunter_marks, etc). Util pra respec do tree.
    ResetSkills,
    /// Refina um item do inventário (+1 nível). Requer ItemInstance
    /// presente no slot. Custo: gold proporcional ao refinement atual.
    /// Falha (chance crescente com nível) reseta refinement pra 0.
    RefineItem { slot: u16 },
    /// Encrava uma gema (`gem_slot`) em um socket livre do item em
    /// `item_slot`. Requer ItemInstance com `sockets > 0`. Gema é
    /// consumida do inventário. Falha silenciosamente se sem socket
    /// livre ou item incompatível.
    SocketGem { item_slot: u16, gem_slot: u16 },
    /// Desmonta do barco atual. Server faz BFS pequeno (≤3 tiles) procurando
    /// um tile walkable adjacente ao barco e teleporta o player. Falha se
    /// nao houver terra acessivel — player precisa mover o barco antes.
    DismountBoat,
    /// Teletransporta o player para o spawn do mapa. Usar como escape em
    /// caso de bug de colisão (player preso em wall, fora do mapa, etc.).
    /// Permitido em qualquer estado — se montado em barco, desmonta antes.
    ResetPosition,
    RequestDisconnect,
    /// Cria novo personagem para a conta (multi-char). Aparece como nova
    /// entry na CharacterList apos sucesso. Server valida nome unico.
    CreateCharacter {
        name: String,
        visual: crate::VisualConfig,
        starting_weapon: u16,
    },
    /// Seleciona um char da lista pra entrar no jogo. Server valida que
    /// o char pertence a conta autenticada, carrega o estado e envia LoginOk.
    SelectCharacter {
        name: String,
    },
    /// Pula a janela de "stand up" e respawna direto na cidade. Disponivel
    /// quando session.downed=true, sem precisar esperar o timer chegar a 0.
    /// Server teleporta pro spawn_tile, restaura HP, limpa estado downed.
    RespawnAtCity,
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
    /// Lista de chars da conta autenticada — enviada apos Login bem-sucedido
    /// e apos cada CreateCharacter ou SelectCharacter. Cliente exibe a tela
    /// de selecao; pode estar vazia (conta nova) ou ter ate N chars.
    /// `available_weapons` = item_ids que o cliente deve mostrar como opcoes
    /// na criacao de char (filtrado por items.active=TRUE).
    CharacterList {
        chars: Vec<CharacterListEntry>,
        available_weapons: Vec<u16>,
    },
    /// Resposta a `CreateCharacter` quando criacao falha (nome duplicado,
    /// invalido, etc). Cliente mostra erro e reabre dialog.
    CharacterCreationFailed {
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
    /// Snapshot dos items configurados no servidor — enviado no login e
    /// re-enviado quando admin altera algo (hot-reload da economy).
    /// Cliente usa pra sobrescrever nomes/icones hardcoded em ItemInfo.
    ///
    /// Campo `item_configs` em vez de `items` pra não colidir com `items` do
    /// `ShopOpen` (mesma struct compartilhada no cliente C#).
    ItemsConfig {
        #[serde(rename = "item_configs")]
        items: Vec<ItemConfigEntry>,
    },
    StatsUpdate {
        stats: crate::PlayerStats,
        equipment: crate::Equipment,
    },
    ManaUpdate { current: i32 },
    StaminaUpdate { current: i32 },
    /// Poise atual do player (0..stats.poise_max). Server envia quando o
    /// inteiro muda (poise inteiro, nao fracionado). Cliente atualiza barra.
    PoiseUpdate { current: i32 },
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
        xp: [u64; crate::PROF_COUNT],
    },
    /// Catálogo de skills carregado do DB. Enviado uma vez no login + após
    /// hot-reload (admin bumpou economy_version). Cliente cacheia em
    /// `SkillsConfigCache` pra UI consultar nome/icon/descrição.
    SkillsConfig { skills: Vec<crate::SkillDef> },
    /// Estado completo de skills do player. Enviado no login + após qualquer
    /// mutação (learn, rank-up, equip).
    PlayerSkillsUpdate { state: crate::PlayerSkillsState },
    /// Broadcast de cast pra renderização cliente (gizmos/VFX). Servidor
    /// envia pra todos clientes em AOI quando alguém casta uma skill.
    SkillCastFx {
        skill_id: u32,
        #[serde(with = "crate::vec2_arr")]
        caster_pos: glam::Vec2,
        #[serde(with = "crate::vec2_arr")]
        target_pos: glam::Vec2,
        /// EntityId do alvo principal pra skills line/single (Lightning Bolt etc).
        /// None pra AoE/self/projectile (cliente desenha sem snap em alvo).
        #[serde(skip_serializing_if = "Option::is_none")]
        target_eid: Option<crate::EntityId>,
        /// EntityId do caster — usado pelo cliente pra cancelar coroutines
        /// quando o cast é interrompido (SkillCastCancel mata visuals deste eid).
        #[serde(skip_serializing_if = "Option::is_none")]
        caster_eid: Option<crate::EntityId>,
    },
    /// Cast foi cancelado (player se moveu durante o cast). Cliente despawna
    /// gizmos/VFX em andamento associados ao caster_eid. Tambem para qualquer
    /// pose de Thrust travada — snap.casting tambem fica false no proximo tick.
    /// Server tambem refunda mp/stamina e remove o cooldown — cliente reseta
    /// local cd timer pra refletir.
    SkillCastCancel {
        caster_eid: crate::EntityId,
        skill_id: u32,
    },
    /// Projetil atingiu um alvo. Cliente usa pra spawnar VFX de impacto
    /// "atachado" ao alvo (ex: flecha presa no inimigo + splatter de
    /// sangue rotacionado pela direção do projetil).
    ProjectileImpact {
        /// Entity_id do alvo atingido. Cliente lookup por NetId no
        /// dicionario de entidades pra parentar o visual.
        target_eid: crate::EntityId,
        /// Direção da flecha (unitario, vel.normalize()). Cliente rotaciona
        /// o stuck arrow + splatter pra alinhar.
        #[serde(with = "crate::vec2_arr")]
        dir: glam::Vec2,
        /// proj_kind (0=arrow, 1=fireball, etc). So 0 (arrow) atualmente
        /// dispara o visual stuck — outros kinds tem seu proprio impacto.
        kind: u8,
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

/// Entry da lista de personagens enviada apos login. Cliente renderiza
/// como card na tela de selecao (paper-doll thumbnail + nome + level).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CharacterListEntry {
    pub name: String,
    pub level: u32,
    pub visual: crate::VisualConfig,
    /// item_id da arma equipada (informativo — mostra ao lado do nome).
    pub weapon_id: Option<u16>,
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

/// Entrada da config de items enviada pro cliente. Cliente sobrescreve
/// `ItemInfo.NameOf` e o icone via runtime cache. icon_path tem precedência
/// sobre icon_col/icon_row.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ItemConfigEntry {
    pub id:         u16,
    pub name:       String,
    pub icon_path:  Option<String>,
    pub icon_col:   i32,
    pub icon_row:   i32,
    pub equip_slot: Option<String>,
    /// Se false, server bloqueia equip/use. Client pode greyscale o ícone.
    #[serde(default = "default_true")]
    pub active:     bool,
}

fn default_true() -> bool { true }

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
