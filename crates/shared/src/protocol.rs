//! Protocolo de rede. Serializado em binario com postcard (ver `encode`).
//!
//! Os enums sao TAGGED POR FORA (o default do serde): a variante vira um
//! indice de 1 byte. O `#[serde(tag = "type")]` que existia aqui era pro
//! Newtonsoft.Json do cliente Unity — enum tagueado por dentro escreve o NOME
//! da variante e exige `deserialize_any` na volta, coisa que o postcard nao
//! implementa. Com o Unity fora, o nome so' custava banda.
//!
//! Toda mudanca em `ClientMessage`/`ServerMessage` DEVE bumpar
//! `PROTOCOL_VERSION` em `constants.rs`.

use crate::{EntityId, EntityMeta, EntityState, PlayerId, PROTOCOL_VERSION};
use serde::{Deserialize, Serialize};

/// Cliente -> Servidor.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ClientMessage {
    /// Primeiro pacote; servidor responde com `HandshakeAck` ou `Kick`.
    Handshake {
        protocol_version: u16,
        client_version: String,
    },
    /// Define (ou limpa) o alvo do combate por target.
    ///
    /// A partir daqui o ataque basico e' AUTOMATICO: enquanto o alvo estiver
    /// vivo e no alcance da arma, o servidor bate sozinho no ritmo do
    /// cooldown. O cliente nao manda mais botao de ataque nem mira.
    SetTarget { target: Option<EntityId> },
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
    /// Toque no chao: "ande ate' aqui".
    ///
    /// O cliente manda um DESTINO, nunca um caminho — quem decide por onde
    /// da' pra passar e' quem tem o relevo. E' um destino que ele ja' poderia
    /// alcancar andando, entao nao concede nada que o joystick nao conceda.
    MoverPara { x: f32, z: f32 },
    UseItem { slot: u16 },
    /// Interagir. `target_eid` Some = entidade clicada específica (NPC/baú);
    /// None = pega o NPC mais próximo (tecla de interação / toggle).
    Interact {
        #[serde(default)]
        target_eid: Option<u64>,
    },
    ShopBuy { slot_idx: u8 },
    ShopSell { inv_slot: u16 },
    /// Trade atômico — todas as compras E vendas executadas juntas, ou
    /// nada. Cliente preview-only; servidor valida tudo de novo.
    ShopTrade { buying: Vec<TradeBuyEntry>, selling: Vec<TradeSellEntry> },
    VaultDeposit { inv_slot: u16 },
    VaultWithdraw { vault_slot: u16 },
    VaultClose,
    InventorySwap { a: InvSpot, b: InvSpot },
    /// Sort + merge stacks no inventario do player. Server agrupa stacks por
    /// item_id (respeitando stack_max), ordena ascendente, mantem itens com
    /// instance (rolls/refinamento) separados ao final.
    InventoryAutoArrange,
    /// Mesmo, pro vault aberto. Falha silenciosamente se vault nao aberto.
    VaultAutoArrange,
    /// Crafta uma receita (`id` na CRAFT_RECIPES table). Server valida inputs,
    /// consome, gera output (com ItemInstance se equipavel). Falha silenciosa
    /// se faltam materiais ou inv cheio.
    Craft { recipe_id: u16 },
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
    /// Sobe rank de uma skill já aprendida. Custo varia por rank (ver
    /// `SP_COST_PER_RANK`). Falha se rank == MAX_SKILL_RANK.
    /// Equipa skill ativa em slot 0..=5. `slot=None` ou `skill_id=0` desequipa.
    /// Passivas ignoram esse req (sempre ativas se aprendidas).
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
    /// Refina um item do inventário (+1 nível). Requer ItemInstance
    /// presente no slot. Custo: gold proporcional ao refinement atual.
    /// Falha (chance crescente com nível) reseta refinement pra 0.
    RefineItem { slot: u16 },
    /// Encrava uma gema (`gem_slot`) em um socket livre do item em
    /// `item_slot`. Requer ItemInstance com `sockets > 0`. Gema é
    /// consumida do inventário. Falha silenciosamente se sem socket
    /// livre ou item incompatível.
    SocketGem { item_slot: u16, gem_slot: u16 },
    /// Pede pra subir num barco. Server valida proximidade (player em
    /// tile adjacente ao barco) e parenta o player (Mounted) com
    /// local_pos no ponto de entrada do deck.
    BoardBoat { boat_eid: EntityId },
    /// Desce do barco atual. Server faz BFS pequeno procurando tile
    /// walkable adjacente ao barco — falha se barco em alto-mar.
    /// Mantém o nome `DismountBoat` por compat com clientes antigos —
    /// `LeaveBoat` é o alias preferido daqui pra frente.
    LeaveBoat,
    /// LEGADO — alias de `LeaveBoat`. Manter pra clientes velhos
    /// enquanto a transição rola.
    DismountBoat,
    /// Pega uma estação do barco onde o player está. Player precisa
    /// estar dentro da interaction zone da estação (helm/sail/anchor).
    /// Falha silenciosamente se ocupada ou fora de zona.
    GrabStation { station: u8 },
    /// Solta a estação atual (se tiver alguma). Idempotente.
    ReleaseStation,
    /// Ajusta a vela do barco onde o player tem a estação SAIL.
    /// `delta_position`: -1 baixa 1 nivel, +1 sobe 1 nivel
    /// (clamp em [0..2]).
    /// `delta_angle`: rad a adicionar ao angulo da vela (clamp -PI/2..PI/2).
    SailAdjust { delta_position: i8, delta_angle: f32 },
    /// Toggle da ancora. Player precisa ter station ANCHOR. Inicia
    /// animacao de drop (se up) ou raise (se down). Anim leva
    /// BOAT_ANCHOR_ANIM_TIME segundos.
    AnchorToggle,
    /// Ajusta angulo da roda do leme. Delta em rad — server soma e
    /// clampa em [-BOAT_MAX_RUDDER_ANGLE, +]. rudder_angle PERSISTE
    /// quando o player solta a estacao HELM (igual barco real).
    HelmAdjust { delta_angle: f32 },
    /// Ajusta o angulo de mira do canhao do slot `slot`. Player precisa
    /// ter station CANNON_BASE+slot. Angle: -CANNON_AIM_MAX_RAD a +.
    /// Server clampa e persiste.
    CannonAim { slot: u8, angle: f32 },
    /// Dispara o canhao. `power` 0..1 — controla range e altura do arco.
    /// Spawn de CannonBombTag; explosao AoE no impacto.
    CannonFire { slot: u8, power: f32 },
    /// Toggle do PK Mode (player vs player opt-in). Quando ON, o player
    /// pode dar/levar dano de outros players com pk_mode ON tambem.
    /// Futuro: zonas PvP forcam ON; faccoes diferentes ignoram flag.
    TogglePkMode { on: bool },
    /// Teletransporta o player para o spawn do mapa. Usar como escape em
    /// caso de bug de colisão (player preso em wall, fora do mapa, etc.).
    /// Permitido em qualquer estado — se montado em barco, desmonta antes.
    ResetPosition,
    /// Joga o item do slot do inventario no chao perto do player. Server
    /// valida slot ocupado, decrementa qty (ou zera) e spawna LootTag.
    DropItem { slot: u16 },
    RequestDisconnect,
    /// Cria novo personagem para a conta (multi-char). Aparece como nova
    /// entry na CharacterList apos sucesso. Server valida nome unico.
    CreateCharacter {
        name: String,
        visual: crate::VisualConfig,
        starting_weapon: u16,
        /// Facção escolhida na criação. Default Peacemain se cliente antigo
        /// não enviar (compat).
        #[serde(default)]
        faction: crate::Faction,
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
    /// Atualiza o visual do player mid-game (wardrobe). Server valida,
    /// salva no Session, persiste no DB, e o proximo snapshot replica
    /// pra todos os clientes — incluindo o autor (que ja aplicou local
    /// pra responsividade, mas confirma com server snapshot).
    UpdateVisual {
        visual: crate::VisualConfig,
    },
    /// Comando admin — server valida `secret` contra env var
    /// `MMORPG_ADMIN_SECRET`. Se `target_char` Some, aplica no char com
    /// esse nome (precisa estar online); senao aplica no player que enviou.
    /// Drop silencioso se secret invalido ou env nao configurada.
    AdminCommand {
        secret: String,
        #[serde(default)]
        target_char: Option<String>,
        action: AdminAction,
    },
    /// Aceita uma quest oferecida (board/npc/facção mais próximo já validou
    /// proximidade no Interact). Server valida level/facção/cooldown/slot.
    AcceptQuest { quest_id: u16 },
    /// Abandona uma quest ativa (libera o slot; repetível volta ao cooldown).
    AbandonQuest { quest_id: u16 },
    /// Entrega/conclui uma quest cujo objetivo está cumprido (status READY).
    /// Server reconfere, consome itens (Collect/Deliver) e concede recompensa.
    TurnInQuest { quest_id: u16 },
    /// Pede a lista de quests oferecidas por um giver. Usado pelo QUADRO da
    /// cidade (objeto de cena, não-entidade): source=BOARD(0), giver=0. NPCs e
    /// facção são entidades-servidor e ofertam via Interact (handle_interact).
    RequestQuestOffer { source: u8, giver: u16 },
    /// Compra um item na loja de facção (gasta pontos de facção). Server valida
    /// proximidade do NPC de facção da MESMA facção, pontos suficientes, e
    /// concede o item.
    FactionShopBuy { item_id: u16 },

    // ── Pesca ────────────────────────────────────────────────────────────
    /// Boia caiu na água em `pos`. Server valida vara equipada + que `pos` é
    /// água dentro do alcance, e passa a atrair peixes pra esse ponto. Quando
    /// um peixe encosta na boia, server responde `FishingBite`.
    FishingCast {
        #[serde(with = "crate::vec2_arr")]
        pos: glam::Vec2,
    },
    /// Resultado do minigame de reel-in do peixe fisgado. `success=true` →
    /// server remove o peixe do mundo e concede o item no inventário.
    /// `success=false` → peixe é solto (volta a nadar). Idempotente se não há
    /// peixe fisgado.
    FishingReel { success: bool },
    /// Cancela a pesca atual (player se moveu/desistiu antes da fisgada). Solta
    /// o peixe fisgado se houver e limpa a boia. Idempotente.
    FishingCancel,

    // ── Tutorial ─────────────────────────────────────────────────────────
    /// Player concluiu o tutorial (clicou "concluir" ou cumpriu objetivos).
    /// Só tem efeito no processo de tutorial (TUTORIAL_MODE); o server marca
    /// `last_tutorial_completed` no DB e responde `TutorialComplete`, que manda
    /// o client reconectar no mundo aberto. Ignorado no processo do mundo.
    FinishTutorial,

    // ── Dungeon ──────────────────────────────────────────────────────────
    /// Escolhe o modo da dungeon ANTES do SelectCharacter (processo :9002).
    /// raid=true → RAID BOSS: sem waves/mobs, spawn perto da arena, só o boss.
    /// Ignorado fora de DUNGEON_MODE.
    SelectDungeonMode { raid: bool },
}

/// Acoes administrativas aplicadas via `ClientMessage::AdminCommand`.
/// Sempre afetam o player que enviou (self). Pra mexer em outro player,
/// rode comando da conta desse player.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum AdminAction {
    /// Seta xp absoluto — recomputa stats (level escala HP/MP/poise unlock).
    SetXp { xp: u64 },
    /// Seta gold absoluto.
    SetGold { gold: i64 },
    /// Adiciona item ao inventario (qty stackavel).
    GiveItem { item_id: u16, qty: u16 },
    /// Esvazia inventario completamente.
    ClearInventory,
    /// HP/MP/Stamina/Poise full.
    HealFull,
    /// Concede skill points (incrementa sp_earned).
    GrantSp { amount: u32 },
    /// Concede stat points (incrementa unspent_points).
    GrantStatPoints { amount: u32 },
    /// Seta level: xp = sum(1..lvl-1) * mult, unspent = 3*(lvl-1), sp = lvl-1.
    /// Equivalente a fazer SetXp + ajuste de pontos atomicamente.
    SetLevel { level: u32 },
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
    /// Pulo. Vale por SUBIDA, nao por altura: o que ele muda e' o degrau
    /// maximo que o corpo aceita — de um bloco pra dois. O arco vertical e'
    /// so' o cliente contando o que o servidor ja' decidiu.
    pub const PULO:      u32 = 1 << 5;
}

/// Servidor -> Cliente.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ServerMessage {
    HandshakeAck {
        protocol_version: u16,
        server_time_ms: u64,
        /// Multiplier da curva de XP (configuravel via DB pra eventos). Cliente
        /// usa pra calcular display da barra de XP — DEVE bater com server.
        /// Default `DEFAULT_XP_MULTIPLIER` (500); evento 2x = 250 (mais facil).
        #[serde(default = "default_xp_mult")]
        xp_multiplier: u64,
    },
    LoginOk {
        player_id: PlayerId,
        entity_id: EntityId,
        spawn: [f32; 2],
    },
    LoginDenied {
        reason: String,
    },
    /// Onde o jogador esta, e quao cheio. Vai no login e a cada poucos
    /// segundos.
    ///
    /// O HUD precisa disso porque, num jogo com servidor/canal/zona, "onde eu
    /// estou" deixa de ser obvio: o mesmo personagem aparece em lugares
    /// diferentes conforme o canal, e lotacao muda o que da' pra fazer ali.
    InfoCanal {
        realm: String,
        canal: String,
        zona: String,
        jogadores: u32,
        capacidade: u32,
    },
    /// Portal pra outra ZONA: reconecte neste host.
    ///
    /// Zona (cidade, campo, dungeon) roda em processo proprio, entao mudar de
    /// zona nao e' teleporte — e' trocar de servidor. O personagem e' o mesmo
    /// (banco compartilhado no realm), so' a conexao muda.
    TrocarZona {
        zona: String,
        host: String,
    },
    /// Canal cheio: o jogador esta na FILA, nao recusado.
    ///
    /// Area de canal unico (cidade, arena, boss de mundo) nao pode simplesmente
    /// abrir outra instancia — a graca dela e' todo mundo estar no mesmo lugar.
    /// Quando lota, a saida e' esperar, e o jogador precisa ver a posicao pra
    /// decidir se espera ou faz outra coisa.
    FilaDeEntrada {
        posicao: u32,
        total: u32,
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
    /// Currency separado do inventário. Enviado no login e após cada
    /// transação que muda gold (loot, shop buy/sell, refining, trade).
    GoldUpdate { gold: u64 },

    // ── Farm Nodes ──────────────────────────────────────────────────────────
    /// Enviado após login com a lista completa de farm nodes do mapa. Cliente
    /// usa para associar IDs aos GameObjects locais por posição.
    FarmNodesConfig { nodes: Vec<FarmNodeInfo> },
    /// Node coletado — desaparece até respawn. Broadcast pra players na AOI.
    FarmNodeDepleted { node_id: u32 },
    /// Pedras de minerio ESGOTADAS no momento do login, por chave de coluna.
    ///
    /// A pedra em si nunca viaja: os dois lados a geram da mesma semente. O
    /// que viaja e' so' a excecao — quais sumiram —, e ela e' curta porque
    /// pedra esgotada e' minoria por construcao.
    PedrasEsgotadas { colunas: Vec<u32> },
    /// Uma pedra acabou de esgotar (ou de voltar). Broadcast na AOI.
    PedraEsgotada { coluna: u32 },
    PedraVoltou { coluna: u32 },
    /// Node respawnado — pode ser coletado novamente.
    FarmNodeRespawned { node_id: u32 },
    /// XP de proficiencia por CONJUNTO de arma. Coleta e artesanato nao tem.
    ProficienciesUpdate {
        #[serde(rename = "proficiency_xp")]
        xp: [u64; crate::PROF_COUNT],
    },
    /// Catálogo de skills carregado do DB. Enviado uma vez no login + após
    /// hot-reload (admin bumpou economy_version). Cliente cacheia em
    /// `SkillsConfigCache` pra UI consultar nome/icon/descrição.
    SkillsConfig { skills: Vec<crate::skills::Skill> },
    /// Catalogo de receitas de crafting carregado do DB. Enviado no login,
    /// substitui o hardcoded client-side. Admin pode mudar custos/inputs/
    /// outputs via DB — ideal pra eventos com receitas especiais.
    CraftRecipes { recipes: Vec<CraftRecipeNet> },
    /// Reverse-index de loot tables — pra cada item, quais mobs/farm nodes
    /// dropam ele. Usado pela UI de crafting pra mostrar "como conseguir"
    /// quando jogador clica num material que falta. Enviado no login + apos
    /// hot-reload da economy. Campo nomeado `resource_sources` (nao `items`)
    /// pra evitar colisao no deserializer compartilhado do cliente.
    ResourceSources {
        #[serde(rename = "resource_sources")]
        items: Vec<ItemResourceSources>,
    },
    /// Estado completo de skills do player. Enviado no login + após qualquer
    /// mutação (learn, rank-up, equip).
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
        target_eid: Option<crate::EntityId>,
        /// EntityId do caster — usado pelo cliente pra cancelar coroutines
        /// quando o cast é interrompido (SkillCastCancel mata visuals deste eid).
        caster_eid: Option<crate::EntityId>,
        /// Posicoes encadeadas dos bounces (Chain Lightning, Lightning Bolt
        /// rank 5+). Comeca no target_pos principal e segue por cada alvo
        /// adicional. None se a skill nao tem chain.
        #[serde(default)]
        chain_points: Option<Vec<[f32; 2]>>,
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
    /// Buff foi aplicado a uma entidade — visual feedback. Server emite
    /// quando: heal recebido (de aliado), buff applied (Bloodthirst, Group
    /// Heal aura, etc). Cliente spawna music_burst ou sparkle acima do alvo.
    /// `kind`:
    ///   0 = heal (aliado curou voce)
    ///   1 = damage_buff (Bloodthirst, Attack Up)
    ///   2 = defense_buff (Defense Up, Phalanx)
    ///   3 = haste (atk speed buff)
    BuffApplied {
        target_eid: crate::EntityId,
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

    /// Estado global do vento. Servidor envia no login (snapshot inicial)
    /// + sempre que muda significativamente (drift suave ou storm event).
    /// `direction` em rad world-space; `intensity` em [0,1] onde 1 = vendaval.
    WindUpdate {
        direction: f32,
        intensity: f32,
    },
    /// Resposta ao Interact com um quest giver (quadro/NPC/facção): lista de
    /// quests DISPONÍVEIS pra aceitar dali (já filtradas por level/facção/cooldown).
    QuestOffer {
        giver_source: u8,  // quests::quest_source
        giver_id: u16,     // shop_id do NPC, 0 (board) ou faction_id
        #[serde(default)]
        giver_name: String, // nome do NPC pro cabeçalho do diálogo ("" = board)
        quests: Vec<crate::quests::QuestNet>,
    },
    /// Log completo das quests ativas do player (login + ressincronização).
    /// Campo `quests` (não `active`) pra reusar o mesmo slot JSON do QuestOffer
    /// no cliente — o `type` desambigua.
    QuestLog {
        quests: Vec<crate::quests::QuestNet>,
    },
    /// Atualização incremental de uma quest (progresso/status mudou).
    QuestUpdate {
        quest_id: u16,
        progress: u32,
        status: u8,        // quests::quest_status
    },
    /// Pontos de facção atuais do player (atualiza HUD).
    FactionPoints {
        points: u32,
    },
    /// Givers (ids) que têm AO MENOS uma quest aceitável agora pra este player.
    /// Cliente usa pra mostrar o "!" só sobre quem realmente tem missão.
    QuestGivers {
        available: Vec<u16>,
    },
    /// Loja de facção (enviada ao interagir com o NPC de facção da própria
    /// facção). Itens custam PONTOS DE FACÇÃO, não ouro.
    FactionShopOpen {
        faction: u8,
        points: u32,
        // Nome distinto de ShopOpen.items (tipo diferente, mesma key colidiria no client).
        faction_items: Vec<FactionShopItemNet>,
    },

    /// Um peixe encostou na boia do player — fisgou. Cliente inicia o minigame
    /// de reel-in com o `species` informado (1-4, mapeia pro catálogo de
    /// peixes). Ao terminar, cliente manda `FishingReel{success}`.
    FishingBite {
        /// EntityId do peixe fisgado (cliente pode usar pra esconder/realçar).
        fish_eid: EntityId,
        /// Espécie do peixe (1=Anchova..4=Baiacu) — define dificuldade/sprite.
        species: u16,
    },

    // ── Tutorial ─────────────────────────────────────────────────────────
    /// Tutorial concluído (resposta ao `FinishTutorial`). O client deve
    /// desconectar e reconectar no MUNDO ABERTO. `world_host`/`world_path`
    /// vazios → client usa o default de produção (wss .../game).
    TutorialComplete {
        #[serde(default)]
        world_host: String,
        #[serde(default)]
        world_path: String,
    },
    /// Manda o client ir pro TUTORIAL (emitido pelo mundo aberto quando o
    /// player interage com o NPC de re-treino). `host`/`path` vazios → client
    /// usa o default de tutorial (wss .../tutorial).
    GoToTutorial {
        #[serde(default)]
        host: String,
        #[serde(default)]
        path: String,
    },
    /// Fala do NPC guia do tutorial (Matteo) — o client mostra um balão de
    /// diálogo. Emitido ao falar com ele (dá machado / recebe madeira / etc).
    TutorialSay {
        #[serde(default)]
        speaker: String,
        #[serde(default)]
        text: String,
    },

    // ── Dungeon ──────────────────────────────────────────────────────────
    /// Uma sala da dungeon foi limpa (todos os mobs mortos) → o gate pra
    /// próxima sala abriu (server flipou os tiles WALL→DUNGEON_FLOOR). O client
    /// atualiza o tracker e abre o gate visual. `room_idx` = índice da sala
    /// recém-limpa; o player avança pra `room_idx + 1`.
    DungeonRoomCleared {
        room_idx: u32,
        total_rooms: u32,
    },
    /// Dungeon concluída (boss morto). Igual ao `TutorialComplete`: o client
    /// reconecta no MUNDO ABERTO. host/path vazios → default de produção
    /// (/game). O loot/XP ganho na dungeon JÁ está persistido (diferente do
    /// tutorial, que descarta).
    DungeonComplete {
        #[serde(default)]
        world_host: String,
        #[serde(default)]
        world_path: String,
    },
}

/// Item da loja de facção (item_id + custo em pontos de facção).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FactionShopItemNet {
    pub item_id: u16,
    pub points: u32,
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

/// Receita de crafting enviada do server pro client. Espelho do
/// `crate::CraftRecipe` mas sem o `&'static str` (use `String` pra serializar).
/// Usado pra cliente renderizar a UI de crafting baseada no que esta no DB,
/// permitindo admin mudar receitas sem rebuild do client.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CraftRecipeNet {
    pub id:                u16,
    pub name:              String,
    /// 0=Other, 1=Weapon, 2=Armor, 3=Material/resource. UI filtra por tab.
    pub category:          u8,
    /// Estação de craft (shared::craft_station): 0=Forja 1=Ateliê 2=Smelter
    /// 3=Marcenaria. UI filtra pela estação que o player abriu. serde(default)
    /// pra compat com mensagens antigas sem o campo.
    #[serde(default)]
    pub station:           u8,
    /// 1-4. UI exibe badge colorido.
    pub tier:              u8,
    /// (item_id, qty) pares de inputs (max 4 entradas).
    pub inputs:            Vec<[u32; 2]>,
    pub output_item_id:    u16,
    pub output_qty:        u32,
    pub output_item_level: u16,
    pub roll_instance:     bool,
}

/// Origem de um recurso — mob drop ou farm node (gather). Usado pelo
/// painel de crafting pra mostrar "como obter" um material. `kind`:
///   0 = mob drop (caçar/matar)
///   1 = farm node / gather (Tree, Rock, Flower etc por tier)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceSource {
    pub kind:    u8,
    pub name:    String,
    pub qty_min: u32,
    pub qty_max: u32,
    /// Probabilidade [0.0..1.0] por kill/coleta.
    pub chance:  f32,
}

/// Conjunto de fontes que produzem um item específico. Reverse-index das
/// loot tables, computado server-side e enviado no `ResourceSources`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ItemResourceSources {
    pub item_id: u16,
    pub sources: Vec<ResourceSource>,
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
    /// Facção do char (cliente mostra cor/badge no card de seleção).
    #[serde(default)]
    pub faction: crate::Faction,
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

/// Descritor de um farm node carregado do mapa. Enviado no FarmNodesConfig.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FarmNodeInfo {
    pub id:   u32,
    pub x:    f32,
    pub y:    f32,
    pub kind: String,
    pub tier: u8,
}

/// Replicacao do mundo enviada a cada tick.
#[derive(Debug, Clone, Serialize, Deserialize)]
/// Um tick de mundo, em delta.
///
/// Tres listas em vez de uma: `entered` traz o dado estavel de quem acabou de
/// aparecer, `states` traz so' quem MUDOU de estado, `removed` quem saiu (por
/// morte ou por sair do AOI). Entidade que nao aparece em lista nenhuma esta
/// parada e nao custa byte.
pub struct WorldSnapshot {
    pub tick: u32,
    pub server_time_ms: u64,
    pub last_input_seq: u32,
    /// Entidades que entraram no campo de visao neste tick.
    pub entered: Vec<EntityMeta>,
    /// Estado de quem mudou.
    pub states: Vec<EntityState>,
    pub removed: Vec<EntityId>,
}

fn default_xp_mult() -> u64 { crate::constants::DEFAULT_XP_MULTIPLIER }

/// Codificacao do wire.
///
/// Era JSON. Com ~40 campos por entidade e nome de campo repetido em cada uma,
/// uma entidade custava 297 bytes: 100 mobs a 30Hz davam 0,85 MB/s POR JOGADOR,
/// ou 3 GB por hora de dado movel. Insustentavel no celular, que e' o alvo.
///
/// Postcard nao e' auto-descritivo: nao carrega nome de campo, inteiro vai em
/// varint e `Option::None` custa 1 byte. Em troca, os dois lados precisam
/// compilar exatamente a MESMA definicao — o que aqui e' de graca, porque
/// cliente e servidor usam este crate. Foi por nao ter isso que a producao caiu
/// com web em 63 e cliente em 66.
///
/// Por isso tambem NAO pode haver `skip_serializing_if` em tipo de wire: ele
/// muda a quantidade de campos escritos, e sem nome de campo o outro lado nao
/// tem como saber que faltou um.
pub fn encode<T: Serialize>(msg: &T) -> anyhow::Result<Vec<u8>> {
    Ok(postcard::to_allocvec(msg)?)
}

pub fn decode<T: for<'de> serde::Deserialize<'de>>(bytes: &[u8]) -> anyhow::Result<T> {
    Ok(postcard::from_bytes(bytes)?)
}

pub fn version() -> u16 {
    PROTOCOL_VERSION
}
