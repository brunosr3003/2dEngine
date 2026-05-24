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
    // Slots novos (Fase E)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub helm: Option<u16>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub helm_inst: Option<crate::items::ItemInstance>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub legs: Option<u16>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub legs_inst: Option<crate::items::ItemInstance>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub boots: Option<u16>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub boots_inst: Option<crate::items::ItemInstance>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gloves: Option<u16>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gloves_inst: Option<crate::items::ItemInstance>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub belt: Option<u16>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub belt_inst: Option<crate::items::ItemInstance>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cape: Option<u16>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cape_inst: Option<crate::items::ItemInstance>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub necklace: Option<u16>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub necklace_inst: Option<crate::items::ItemInstance>,
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
            Necklace => self.necklace,
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
            Necklace => self.necklace_inst,
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
            Necklace => { self.necklace = id; self.necklace_inst = inst; }
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
            (self.cape,     self.cape_inst),
            (self.necklace, self.necklace_inst),
        ]
    }
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
    /// Item_id da arma do atacante no tick em que tomou dano. Cliente usa
    /// pra escolher VFX de impacto diferenciado por arma (sword=clean cut,
    /// axe=heavy slam, etc). Some apenas no tick do hit, None nos demais.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub attacker_weapon_id: Option<u16>,
    /// EntityId do dono pra projetil (player/enemy que disparou). Cliente
    /// usa pra desenhar tether visual (Spear Throw harpoon line) e
    /// outras integracoes player↔projectile. Some apenas em EntityKind::
    /// Projectile.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub owner_eid: Option<EntityId>,
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
    /// True enquanto o player está em cast (`session.casting_until > now`).
    /// Cliente segura a pose de ataque (Thrust pra Wand/Staff) frame parado
    /// até o cast terminar. Movimento/ataque/defesa estão bloqueados no
    /// servidor durante este período.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub casting: Option<bool>,
    /// True enquanto o inimigo está envenenado (`poisoned_until > now`).
    /// Cliente aplica tint verde no body sprite. Set por skills DOT (ex:
    /// Smoke Bomb 1038). Snapshot só envia quando ativo.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub poisoned: Option<bool>,
    /// True enquanto o inimigo está atordoado (`stunned_until > now`).
    /// Set por Shield Bash (1003). Cliente renderiza tint amarelo + parado.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stunned: Option<bool>,
    /// Offset Y visual pra arco de pulo (Leap Strike). Cliente soma esse
    /// valor à posição do paper-doll pra simular trajetória parabolica.
    /// Server calcula `4 * peak * t * (1-t)`. Snapshot envia somente
    /// durante o leap.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub leap_y: Option<f32>,
    /// True enquanto o player tem poise > 0 (barra de poise ativa). Cliente
    /// renderiza uma bolha visual em volta do char. Snapshot envia somente
    /// quando ativo (poise > 0).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub poise_active: Option<bool>,
    /// Preset visual pra NPCs (0..N). Cliente mapeia pra VisualConfig
    /// (race + outfit + hair). None pra Player/Enemy (esses usam outros
    /// caminhos de visual).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub skin_preset: Option<u8>,
    /// Direcao do barco (0=N, 1=NE, 2=E, 3=SE, 4=S, 5=SW, 6=W, 7=NW). Cliente
    /// escolhe a sheet correta dentre as 8 direcoes. Some apenas em
    /// EntityKind::Boat. None para outras entidades.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub boat_dir: Option<u8>,
    /// Animacao atual do barco (0=idle, 1=movement, 2=shoot). Cliente escolhe
    /// sheet baseado nisso. Some apenas em EntityKind::Boat.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub boat_anim: Option<u8>,
    /// Direcao do ULTIMO tiro (0..7). Independente da boat_dir (que e' a
    /// direcao do casco/movimento). Cliente usa pra rotacionar o flash do
    /// canhao pro mouse, nao pro casco. Some apenas em EntityKind::Boat
    /// quando shoot anim ativa.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub boat_shoot_dir: Option<u8>,
    /// EntityId do passageiro (player montado). Cliente verifica se o local
    /// player == passenger_eid pra decidir se a camera segue o barco. Some
    /// apenas em EntityKind::Boat com passageiro.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub passenger_eid: Option<EntityId>,
    /// True se o player esta montado em algum barco. Cliente esconde o
    /// paper-doll do player local (ele eh representado pelo barco). Some
    /// apenas em EntityKind::Player com Mounted ativo.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mounted: Option<bool>,
    /// True para inimigos boss. Cliente aplica scale maior + frame especial.
    /// Some(true) apenas em EntityKind::Enemy quando EnemyTag.is_boss=true.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_boss: Option<bool>,
    /// Bitmask de buffs ativos no player. Cliente renderiza aura por bit set.
    /// bit0=Bloodthirst (vermelho), bit1=Hunter's Mark (laranja). Some apenas
    /// quando ao menos um bit ativo. Outros bits reservados pra buffs futuros.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub buffs: Option<u8>,
    /// Tier do item dropado (1-4) — cliente usa pra colorir a aura/halo da loot.
    /// Resources (item_id 60-71) e equipaveis com `instance.item_level` >= 1.
    /// None quando sem tier definido (gold, pocoes, itens legacy).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub loot_tier: Option<u8>,

    // ── Boat 2.5D (Sea-of-Thieves) ─────────────────────────────────────────
    /// Heading do barco em rad (world-space). Usar pra renderer 3D / shader
    /// 2.5D. `boat_dir` (8-cardeais) é derivado disso server-side pra
    /// compatibilidade com o BoatRenderer 2D atual. Some apenas em Boat.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub boat_yaw: Option<f32>,
    /// Velocidade linear do barco no world-space (tiles/s). Cliente usa
    /// pra interpolacao + indicador de speed. Some apenas em Boat.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub boat_lin_vx: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub boat_lin_vy: Option<f32>,
    /// Posicao da vela: 0=raised (sem propulsao), 1=half (50%), 2=full (100%).
    /// Some apenas em Boat.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sail_position: Option<u8>,
    /// Angulo da vela em rad relativo ao casco. Range -PI/2..PI/2.
    /// Some apenas em Boat.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sail_angle: Option<f32>,
    /// True se a ancora esta dropada (barco freado). Some apenas em Boat.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub anchor_dropped: Option<bool>,
    /// Progresso da animacao de drop/raise da ancora em [0,1].
    /// 1 = totalmente dropada, 0 = totalmente recolhida. Some apenas em Boat.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub anchor_progress: Option<f32>,
    /// EntityId do player na estacao do leme. None se vazia. Some apenas em Boat.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub helm_eid: Option<EntityId>,
    /// EntityId do player na estacao da vela. Some apenas em Boat.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sail_eid: Option<EntityId>,
    /// EntityId do player na estacao da ancora. Some apenas em Boat.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub anchor_eid: Option<EntityId>,

    /// Player montado em qual barco (EntityId do barco). Some apenas em
    /// Player com Mounted ativo. Substitui o boolean `mounted` do legado
    /// (que vira `Some(true)` quando este campo é Some).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mounted_on: Option<EntityId>,
    /// Posicao do player no deck local (relativa ao centro do barco, sem
    /// rotacao). Cliente usa pra interpolar separado da posicao do barco
    /// quando montado. Some apenas em Player com Mounted ativo.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mounted_local_x: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mounted_local_y: Option<f32>,
    /// Estacao que o player esta operando (0=helm, 1=sail, 2=anchor). None
    /// = livre andando no deck. Some apenas em Player com Mounted.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub station: Option<u8>,
    /// Angulo acumulado da roda do leme em rad. Persiste quando ninguem
    /// esta no leme. Valores positivos viram pra direita; negativos esquerda.
    /// Some apenas em Boat.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rudder_angle: Option<f32>,
    /// True se ha terra walkable proxima — cliente usa pra mostrar/esconder
    /// botao "Sair do Barco". Calculado server-side via BFS, mesma logica
    /// que valida o handle_dismount_boat. Some apenas em Boat.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub can_dismount: Option<bool>,
    /// True quando o player tem PK Mode ativado (opt-in PvP). HUD do
    /// outro player mostra indicador (ex: nome vermelho) quando true.
    /// Some apenas em Player.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pk_mode_on: Option<bool>,
    /// Facção do player. Cliente usa pra colorir (vermelho=Morganeers,
    /// amarelo=Peacemain) e pra filtrar alvos (cross-facção sempre atacável).
    /// Some apenas em Player.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub faction: Option<Faction>,
    /// Giver de quest deste NPC (kind 3 arauto). Cliente usa pra mostrar o
    /// indicador !/? e casar com as quests ativas. Some quando não é arauto.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub quest_giver: Option<u16>,
}

/// Bits do `EntitySnapshot.buffs` — mantém em sync com o cliente C#.
pub mod buffs_mask {
    pub const BLOODTHIRST:  u8 = 1 << 0;
    pub const HUNTERS_MARK: u8 = 1 << 1;
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
    #[serde(skip_serializing_if = "Option::is_none")]
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
