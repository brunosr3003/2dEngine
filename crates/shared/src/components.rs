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
///
/// `instance`: Some(...) para itens equipáveis dropados (rolls aleatórios
/// + rarity + refinement). None pra itens stackáveis (gold, poções) ou
/// itens legacy pre-Fase A — esses usam stats base via `item_bonus`.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default)]
pub struct InventorySlot {
    pub item_id: u16,
    pub qty: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub instance: Option<crate::items::ItemInstance>,
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
        speed_mult: 1.0,
        crit_chance: 0.0,
        hp_regen: 0.5, // regen base de fora-de-combate
        attack_speed_mult: 1.0,
        stamina_max: 100,
        stamina_regen: 25.0,
        block_dmg_reduction: 0.6,         // 60% absorvido por block (base)
        defense_stamina_cost_mult: 1.0,   // 100% do custo base (RES reduz)
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
    /// Multiplicador de velocidade de movimento (1.0 = base PLAYER_SPEED).
    /// SPD soma `MOVE_SPEED_PCT_PER_SPD` por ponto alocado.
    #[serde(default = "default_speed_mult")]
    pub speed_mult: f32,
    /// Chance de crit (0.0..1.0). DES soma `CRIT_CHANCE_PER_DES` por ponto.
    /// Crit multiplica dano por `CRIT_DAMAGE_MULT`.
    #[serde(default)]
    pub crit_chance: f32,
    /// HP regenerado por segundo. VIT soma `HP_REGEN_PER_VIT` por ponto.
    #[serde(default)]
    pub hp_regen: f32,
    /// Multiplicador de velocidade de ataque (1.0 = base). DES soma
    /// `ATTACK_SPEED_PCT_PER_DES` por ponto. Aplicado dividindo o cooldown.
    #[serde(default = "default_speed_mult")]
    pub attack_speed_mult: f32,
    /// Stamina maxima total (base 100 + bonus de SPD).
    #[serde(default = "default_stamina_max")]
    pub stamina_max: i32,
    /// Regen de stamina por seg (base 25 + bonus de SPD).
    #[serde(default = "default_stamina_regen")]
    pub stamina_regen: f32,
    /// Fração de dano absorvido por block (0.0..BLOCK_REDUCTION_MAX). Base
    /// `BLOCK_DAMAGE_REDUCTION_BASE = 0.6`. RES soma `BLOCK_REDUCTION_PER_RES`.
    /// Aplicada apenas quando o player segura RMB com arma ranged.
    #[serde(default = "default_block_reduction")]
    pub block_dmg_reduction: f32,
    /// Multiplicador no custo de stamina de block/parry (1.0 = base, 0.5 cap).
    /// RES subtrai `STAMINA_COST_REDUCTION_PER_RES` por ponto.
    #[serde(default = "default_one")]
    pub defense_stamina_cost_mult: f32,
}

fn default_block_reduction() -> f32 { 0.6 }
fn default_one() -> f32 { 1.0 }

fn default_stamina_max() -> i32 { 100 }
fn default_stamina_regen() -> f32 { 25.0 }

fn default_speed_mult() -> f32 { 1.0 }

/// Slots de equipamento. None = vazio; Some(item_id) = item equipado.
/// `offhand` = escudo (apenas com armas que permitem — ver `weapon_allows_offhand`).
///
/// `*_inst`: instância única do item equipado (rolls + rarity + refinement).
/// None pra itens stackáveis ou legacy pre-Fase A — esses caem no
/// `item_bonus(id)` base.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default)]
pub struct Equipment {
    pub weapon: Option<u16>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub weapon_inst: Option<crate::items::ItemInstance>,
    pub armor: Option<u16>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub armor_inst: Option<crate::items::ItemInstance>,
    pub ring: Option<u16>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ring_inst: Option<crate::items::ItemInstance>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub offhand: Option<u16>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub offhand_inst: Option<crate::items::ItemInstance>,
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
    /// Player iniciou um ataque neste tick. Codifica qual animacao o cliente
    /// deve tocar (ver `AttackAnim` abaixo). None nas outras snapshots.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub attack_anim: Option<u8>,
    /// Item_id da arma equipada, replicado pra que outros clientes mostrem
    /// o sprite de arma correto no paper-doll. None se desarmado/desconhecido.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub weapon_id: Option<u16>,
    /// Item_id do offhand (escudo). Replicado pra renderizar shield sprite
    /// no `_weaponB` layer do paper-doll. None se sem offhand.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub offhand_id: Option<u16>,
    /// True se o player esta no estado Downed. Replicado pra que outros
    /// clientes mostrem a pose sentada + drip de sangue.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub downed: Option<bool>,
    /// Visual config (skin/race/outfit/hair) replicada pra todos os players
    /// visiveis. Permite que um wizard pareca diferente de um warrior.
    /// None pra Enemy/Npc/Projectile/Loot/Portal.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub visual: Option<VisualConfig>,
    /// Multiplicador de atk speed do player (1.0 = base). Cliente usa pra
    /// acelerar a animacao de ataque proporcionalmente ao cooldown reduzido.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub attack_speed_mult: Option<f32>,
    /// Vetor unitário do alvo TOWARD o atacante, no tick em que tomou dano.
    /// Cliente usa pra setar facing (e knockback futuro = -hurt_dir).
    /// None na maioria das snapshots; Some apenas no tick do hit.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hurt_dir: Option<[f32; 2]>,
    /// True se o hit deste tick foi um critico. Cliente usa pra mostrar
    /// floating damage number em estilo diferente (cor/tamanho).
    /// None nos demais ticks; Some(true/false) só quando hp_dropped.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_crit: Option<bool>,
    /// Step do combo melee (0=Slash1, 1=Slash2, 2=Finisher). Acompanha
    /// `attack_anim=SLASH` pra que outros clientes toquem a anim correta
    /// dentro do combo. None fora do tick de attack ou em SHOOT/THRUST.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub combo_step: Option<u8>,
    /// Dano REAL do hit (pre-clamp pelo HP atual). Cliente usa pra mostrar
    /// no floating damage text mesmo se overkill — não fica clampado em
    /// "5/50". None fora do tick de hit.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_damage: Option<i32>,
    /// True enquanto o player segura RMB (defesa ativa). Cliente renderiza
    /// pose de bloqueio + (pra arco/cajado/varinha) bolha de energia.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub defending: Option<bool>,
    /// Preset visual pra NPCs (0..N). Cliente mapeia pra VisualConfig
    /// (race + outfit + hair). None pra Player/Enemy (esses usam outros
    /// caminhos de visual).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub skin_preset: Option<u8>,
}

/// Codigo enviado em `EntitySnapshot.attack_anim` pra discriminar qual
/// animacao o cliente deve tocar no atacante. O server escolhe baseado na
/// arma equipada.
pub mod attack_anim {
    pub const SLASH: u8        = 0; // Sword/Dagger/GreatSword/Unarmed (pONE3)
    pub const SHOOT: u8        = 1; // Bow (pBOW3)
    pub const THRUST: u8       = 2; // Staff/Wand (pONE3 Thrust)
    pub const ENEMY_SWING: u8  = 3; // Inimigo melee (compat com `attacking`)
    pub const ENEMY_SHOOT: u8  = 4; // Inimigo ranged (Goblin Archer / Mago)
    pub const DASH: u8         = 5; // Dash do player (anim de jump, p1 cols 4-7)
    pub const PARRY_FLASH: u8  = 6; // Parry sucesso — full ShieldBash swing + flash
}

/// Configuracao visual de um personagem (skin/race/outfit/hair). Replicada
/// no `EntitySnapshot.visual` pra que clientes remotos renderizem o
/// paper-doll Mana Seed correto. Nomes de campo casam com `VisualConfig`
/// no cliente C#.
///
/// Default por classe via `VisualConfig::for_class("warrior"|"wizard"|"archer")`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct VisualConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub skin: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub skin_race: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub outfit: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub outfit_color: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hair: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hair_color: Option<u8>,
}

impl VisualConfig {
    /// Default por classe. Usado quando o player loga e nao tem visual
    /// customizado salvo. Outfits/hairs escolhidos pra ter sheets em
    /// TODOS os pages (p1/p2/p3/p4 + pONE/pBOW/pPOL) — senao sumiria
    /// durante combate.
    pub fn for_class(class: &str) -> Self {
        match class {
            "wizard" => Self {
                skin: Some(1),
                skin_race: Some("humn".into()),
                outfit: Some("pfpn".into()),
                outfit_color: Some(4),
                hair: Some("dap1".into()),
                hair_color: Some(7),
            },
            "archer" => Self {
                skin: Some(2),
                skin_race: Some("humn".into()),
                outfit: Some("fstr".into()),
                outfit_color: Some(3),
                hair: Some("bob1".into()),
                hair_color: Some(2),
            },
            // "warrior" (default)
            _ => Self {
                skin: Some(1),
                skin_race: Some("humn".into()),
                outfit: Some("fstr".into()),
                outfit_color: Some(1),
                hair: Some("dap1".into()),
                hair_color: Some(1),
            },
        }
    }
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
