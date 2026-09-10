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
    /// Barco navegavel. u16 = boat_kind (0=Lylian Leutard). Cliente usa o
    /// kind pra escolher quais sprite-sheets carregar.
    Boat(u16),
    /// Bola de canhao em voo. Renderiza sprite + sombra; explode no impacto.
    /// Snapshot envia `pos` (XY do landing) e `height` (offset Y do arco).
    CannonBomb,
    /// Peixe nadando no oceano. u16 = species (1=Anchova, 2=Peixe-palhaço,
    /// 3=Peixe-cirurgião, 4=Baiacu). Spawnado pelo servidor perto dos players
    /// em tiles de água; vagueia com wander AI. Ao pescar, é atraído pela boia
    /// e fisgado quando encosta. Cliente usa o species pra escolher o sprite.
    Fish(u16),
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
    #[serde(default)]
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
        stamina_regen: 15.0,
        block_dmg_reduction: 0.6,         // 60% absorvido por block (base)
        defense_stamina_cost_mult: 1.0,   // 100% do custo base (RES reduz)
        damage_reduction_pct: 0.0,        // breakpoints de VIT/RES somam aqui
        bow_range_bonus_pct: 0.0,         // Eagle Eye passive (Bow T1)
        dash_cd_mult: 1.0,                // SPD soma reduction por ponto
        poise_max: 0,                     // hardcore: zero poise base — gateado em skill T4 (lvl 60+)
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
    /// Bonus % no alcance de projeteis de arco (Eagle Eye passive). Aplicado
    /// como multiplicador no PROJ_TTL ao spawnar arrow. 0 = sem bonus.
    #[serde(default)]
    pub bow_range_bonus_pct: f32,
    /// Multiplicador de redução de cooldown do dash. Final dash_cooldown =
    /// DASH_COOLDOWN / dash_cd_mult. SPD soma DASH_CD_REDUCTION_PER_SPD por
    /// ponto. Substituiu o uso de `speed_mult` pra dash CD (movement speed
    /// agora e independente de SPD).
    #[serde(default = "default_one")]
    pub dash_cd_mult: f32,
    /// Poise máximo. Barra que absorve dano antes do HP — enquanto poise > 0
    /// o player nao toma stagger nem hurt anim. Regen fora de combate.
    #[serde(default = "default_poise_max")]
    pub poise_max: i32,
    /// Fração de redução de dano percentual aplicada APOS defense flat. Vem de
    /// breakpoints de stat (ex: cada 25 VIT = +5%). 0..0.75. Cap de 75% pra
    /// evitar invulnerabilidade.
    #[serde(default)]
    pub damage_reduction_pct: f32,
}

fn default_poise_max() -> i32 { 50 }

fn default_block_reduction() -> f32 { 0.6 }
fn default_one() -> f32 { 1.0 }

fn default_stamina_max() -> i32 { 100 }
fn default_stamina_regen() -> f32 { 15.0 }

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
    #[serde(default)]
    pub weapon_inst: Option<crate::items::ItemInstance>,
    pub armor: Option<u16>,
    #[serde(default)]
    pub armor_inst: Option<crate::items::ItemInstance>,
    pub ring: Option<u16>,
    #[serde(default)]
    pub ring_inst: Option<crate::items::ItemInstance>,
    #[serde(default)]
    pub offhand: Option<u16>,
    #[serde(default)]
    pub offhand_inst: Option<crate::items::ItemInstance>,
    // Slots novos (Fase E)
    #[serde(default)]
    pub helm: Option<u16>,
    #[serde(default)]
    pub helm_inst: Option<crate::items::ItemInstance>,
    #[serde(default)]
    pub legs: Option<u16>,
    #[serde(default)]
    pub legs_inst: Option<crate::items::ItemInstance>,
    #[serde(default)]
    pub boots: Option<u16>,
    #[serde(default)]
    pub boots_inst: Option<crate::items::ItemInstance>,
    #[serde(default)]
    pub gloves: Option<u16>,
    #[serde(default)]
    pub gloves_inst: Option<crate::items::ItemInstance>,
    #[serde(default)]
    pub belt: Option<u16>,
    #[serde(default)]
    pub belt_inst: Option<crate::items::ItemInstance>,
    #[serde(default)]
    pub cape: Option<u16>,
    #[serde(default)]
    pub cape_inst: Option<crate::items::ItemInstance>,
    #[serde(default)]
    pub necklace: Option<u16>,
    #[serde(default)]
    pub necklace_inst: Option<crate::items::ItemInstance>,
    // 4 slots dedicados pras ferramentas — todas equipáveis ao mesmo tempo.
    #[serde(default)]
    pub tool_axe: Option<u16>,
    #[serde(default)]
    pub tool_axe_inst: Option<crate::items::ItemInstance>,
    #[serde(default)]
    pub tool_sickle: Option<u16>,
    #[serde(default)]
    pub tool_sickle_inst: Option<crate::items::ItemInstance>,
    #[serde(default)]
    pub tool_pickaxe: Option<u16>,
    #[serde(default)]
    pub tool_pickaxe_inst: Option<crate::items::ItemInstance>,
    #[serde(default)]
    pub tool_rod: Option<u16>,
    #[serde(default)]
    pub tool_rod_inst: Option<crate::items::ItemInstance>,
}

impl Equipment {
    /// Lê o item_id atualmente equipado em `slot` (None = vazio).
    pub fn get(&self, slot: crate::constants::EquipSlot) -> Option<u16> {
        use crate::constants::EquipSlot::*;
        match slot {
            Weapon   => self.weapon,
            Armor    => self.armor,
            Ring     => self.ring,
            Offhand  => self.offhand,
            Helm     => self.helm,
            Legs     => self.legs,
            Boots    => self.boots,
            Gloves   => self.gloves,
            Belt     => self.belt,
            Cape     => self.cape,
            Necklace     => self.necklace,
            ToolAxe      => self.tool_axe,
            ToolSickle   => self.tool_sickle,
            ToolPickaxe  => self.tool_pickaxe,
            ToolRod      => self.tool_rod,
        }
    }

    /// Lê a ItemInstance do slot (None = item sem rolls/legacy).
    pub fn get_inst(&self, slot: crate::constants::EquipSlot) -> Option<crate::items::ItemInstance> {
        use crate::constants::EquipSlot::*;
        match slot {
            Weapon   => self.weapon_inst,
            Armor    => self.armor_inst,
            Ring     => self.ring_inst,
            Offhand  => self.offhand_inst,
            Helm     => self.helm_inst,
            Legs     => self.legs_inst,
            Boots    => self.boots_inst,
            Gloves   => self.gloves_inst,
            Belt     => self.belt_inst,
            Cape     => self.cape_inst,
            Necklace     => self.necklace_inst,
            ToolAxe      => self.tool_axe_inst,
            ToolSickle   => self.tool_sickle_inst,
            ToolPickaxe  => self.tool_pickaxe_inst,
            ToolRod      => self.tool_rod_inst,
        }
    }

    /// Sobrescreve item_id e instance do slot.
    pub fn set(&mut self, slot: crate::constants::EquipSlot, id: Option<u16>, inst: Option<crate::items::ItemInstance>) {
        use crate::constants::EquipSlot::*;
        match slot {
            Weapon   => { self.weapon   = id; self.weapon_inst   = inst; }
            Armor    => { self.armor    = id; self.armor_inst    = inst; }
            Ring     => { self.ring     = id; self.ring_inst     = inst; }
            Offhand  => { self.offhand  = id; self.offhand_inst  = inst; }
            Helm     => { self.helm     = id; self.helm_inst     = inst; }
            Legs     => { self.legs     = id; self.legs_inst     = inst; }
            Boots    => { self.boots    = id; self.boots_inst    = inst; }
            Gloves   => { self.gloves   = id; self.gloves_inst   = inst; }
            Belt     => { self.belt     = id; self.belt_inst     = inst; }
            Cape     => { self.cape     = id; self.cape_inst     = inst; }
            Necklace     => { self.necklace     = id; self.necklace_inst     = inst; }
            ToolAxe      => { self.tool_axe     = id; self.tool_axe_inst     = inst; }
            ToolSickle   => { self.tool_sickle  = id; self.tool_sickle_inst  = inst; }
            ToolPickaxe  => { self.tool_pickaxe = id; self.tool_pickaxe_inst = inst; }
            ToolRod      => { self.tool_rod     = id; self.tool_rod_inst     = inst; }
        }
    }

    /// Itera todos os slots não-vazios — usado por effective_stats.
    pub fn iter_equipped(&self) -> Vec<(Option<u16>, Option<crate::items::ItemInstance>)> {
        vec![
            (self.weapon,   self.weapon_inst),
            (self.armor,    self.armor_inst),
            (self.ring,     self.ring_inst),
            (self.offhand,  self.offhand_inst),
            (self.helm,     self.helm_inst),
            (self.legs,     self.legs_inst),
            (self.boots,    self.boots_inst),
            (self.gloves,   self.gloves_inst),
            (self.belt,     self.belt_inst),
            (self.cape,         self.cape_inst),
            (self.necklace,     self.necklace_inst),
            (self.tool_axe,     self.tool_axe_inst),
            (self.tool_sickle,  self.tool_sickle_inst),
            (self.tool_pickaxe, self.tool_pickaxe_inst),
            (self.tool_rod,     self.tool_rod_inst),
        ]
    }
}

/// Tipo de entidade no wire. Era `String` ("Player", "Enemy"...) reenviada a
/// cada tick por entidade; virou um byte.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EntityTag {
    Player,
    Enemy,
    Projectile,
    Loot,
    Npc,
    Portal,
    Boat,
    Other,
}

pub mod ent_flags {
    /// E' o personagem do proprio jogador que recebe o pacote.
    pub const SELF: u8 = 1 << 0;
    pub const DOWNED: u8 = 1 << 1;
    pub const CASTING: u8 = 1 << 2;
    pub const BOSS: u8 = 1 << 3;
    /// No ar. O cliente desenha o arco; quem decide se o pulo aconteceu e'
    /// o servidor.
    pub const PULANDO: u8 = 1 << 4;
}

/// Precisao da posicao no wire: 1/16 de tile.
///
/// Com `i16` isso cobre +-2048 tiles, folga de sobra pro mundo, e corta a
/// posicao de 8 bytes (2x f32) pra 4. Um decimo de pixel de erro num jogo de
/// vista de cima ninguem enxerga — e o cliente interpola por cima disso.
pub const POS_SCALE: f32 = 16.0;

/// Dado ESTAVEL de uma entidade: vai uma vez, quando ela entra no campo de
/// visao do jogador.
///
/// Separar isto do estado por tick e' o que tira `name` e `kind` do caminho
/// quente. Antes, um mob andando reenviava "Green Goblin Lv3" 30 vezes por
/// segundo — 23 bytes por tick por mob, exatamente nas entidades que se movem.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EntityMeta {
    pub id: EntityId,
    pub tag: EntityTag,
    pub name: Option<String>,
    pub hp_max: u16,
    pub faction: Option<Faction>,
}

/// Estado de uma entidade num tick. E' o unico dado que se repete.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct EntityState {
    pub id: EntityId,
    /// Posicao em 1/16 de tile (ver `POS_SCALE`).
    pub pos: [i16; 2],
    /// Velocidade em 1/16 de tile/s, saturada. O cliente usa pra girar o
    /// modelo e decidir se anda — nao precisa de precisao.
    pub vel: [i8; 2],
    pub hp: u16,
    /// Ver `ent_flags`.
    pub flags: u8,
}

impl EntityState {
    pub fn pos_f32(&self) -> Vec2 {
        Vec2::new(self.pos[0] as f32 / POS_SCALE, self.pos[1] as f32 / POS_SCALE)
    }

    pub fn vel_f32(&self) -> Vec2 {
        Vec2::new(self.vel[0] as f32 / POS_SCALE, self.vel[1] as f32 / POS_SCALE)
    }

    pub fn quantize(id: EntityId, pos: Vec2, vel: Vec2, hp: i32, flags: u8) -> Self {
        let q = |v: f32| (v * POS_SCALE).round().clamp(i16::MIN as f32, i16::MAX as f32) as i16;
        let qv = |v: f32| (v * POS_SCALE).round().clamp(i8::MIN as f32, i8::MAX as f32) as i8;
        Self {
            id,
            pos: [q(pos.x), q(pos.y)],
            vel: [qv(vel.x), qv(vel.y)],
            hp: hp.max(0) as u16,
            flags,
        }
    }
}


/// Bits do `EntitySnapshot.buffs` — mantém em sync com o cliente C#.
pub mod buffs_mask {
    pub const BLOODTHIRST:  u8 = 1 << 0;
    pub const HUNTERS_MARK: u8 = 1 << 1;
    pub const AURA_TIDE:     u8 = 1 << 2;
    pub const AURA_IGNITION: u8 = 1 << 3;
    pub const AURA_MIST:     u8 = 1 << 4;
    pub const AURA_TEMPEST:  u8 = 1 << 5;
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
    pub const SHIELD_BASH: u8  = 7; // Shield Bash skill (1003) — pONE3 ShieldBash
    pub const TOOL_SWING:  u8  = 8; // Rock (mine) e Tree (wood) — p2 rows 0-3
    pub const TOOL_GATHER: u8  = 9; // Flower — p2 rows 4-7
}

/// Facção do personagem, escolhida na criação. Define ilha de spawn e
/// regras de PvP (facções diferentes = PvP sempre ON). Serializa como
/// string lowercase ("morganeers"/"peacemain") pra interop com o cliente C#.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Faction {
    /// Piratas — saque, roubo, caos e anarquia. Spawn na ilha Norte.
    Morganeers,
    /// Aventureiros e exploradores. Spawn na ilha Sul.
    Peacemain,
}

impl Default for Faction {
    fn default() -> Self { Faction::Peacemain }
}

impl Faction {
    /// Parse tolerante (case-insensitive) — usado ao ler do DB (TEXT).
    pub fn from_str_lenient(s: &str) -> Option<Faction> {
        match s.trim().to_ascii_lowercase().as_str() {
            "morganeers" => Some(Faction::Morganeers),
            "peacemain"  => Some(Faction::Peacemain),
            _ => None,
        }
    }
    /// String estável pra persistir no DB.
    pub fn as_db_str(self) -> &'static str {
        match self { Faction::Morganeers => "morganeers", Faction::Peacemain => "peacemain" }
    }
}

/// Configuracao visual de um personagem (skin/race/outfit/hair). Replicada
/// no `EntitySnapshot.visual` pra que clientes remotos renderizem o
/// paper-doll Mana Seed correto. Nomes de campo casam com `VisualConfig`
/// no cliente C#.
///
/// Default por classe via `VisualConfig::for_class("warrior"|"wizard"|"archer")`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct VisualConfig {
    pub skin: Option<u8>,
    pub skin_race: Option<String>,
    pub outfit: Option<String>,
    pub outfit_color: Option<u8>,
    pub hair: Option<String>,
    pub hair_color: Option<u8>,
    /// Hat layer (5hat sheet). None = sem chapeu. Cliente derive overlay
    /// (chifres/cauda) a partir de skin_race; hat e independente.
    /// NAO usa skip_serializing pra que client distinga "no hat" de
    /// "campo omitido" — wardrobe pode REMOVER chapeu mid-game.
    pub hat: Option<String>,
    pub hat_color: Option<u8>,
    /// Tint RGBA aplicado por cima do paper-doll inteiro. Usado pra
    /// diferenciar mobs do mesmo "class visual" mas tier diferente
    /// (goblin verde / amarelo / cinza / demonio roxo / vermelho / dourado).
    /// None = sem tint (Color.white). Cliente multiplica este valor em todos
    /// os SpriteRenderers do paper-doll.
    pub body_tint: Option<[f32; 4]>,
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
                hat: None,
                hat_color: None,
                body_tint: None,
            },
            "archer" => Self {
                skin: Some(2),
                skin_race: Some("humn".into()),
                outfit: Some("fstr".into()),
                outfit_color: Some(3),
                hair: Some("bob1".into()),
                hair_color: Some(2),
                hat: None,
                hat_color: None,
                body_tint: None,
            },
            // "warrior" (default)
            _ => Self {
                skin: Some(1),
                skin_race: Some("humn".into()),
                outfit: Some("fstr".into()),
                outfit_color: Some(1),
                hair: Some("dap1".into()),
                hair_color: Some(1),
                hat: None,
                hat_color: None,
                body_tint: None,
            },
        }
    }
}

/// Marca de corpo solido: empurra e e' empurrado no passe de separacao.
///
/// Era `PhysicsHandle`, que carregava o handle do rigid body no rapier. Com a
/// colisao propria nao ha corpo paralelo — so' a marca.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct Solido;

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
