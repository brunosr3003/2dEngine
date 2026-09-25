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
    /// Peixe nadando no oceano. u16 = species (1=Anchova, 2=Peixe-palhaço,
    /// 3=Peixe-cirurgião, 4=Baiacu). Spawnado pelo servidor perto dos players
    /// em tiles de água; vagueia com wander AI. Ao pescar, é atraído pela boia
    /// e fisgado quando encosta. Cliente usa o species pra escolher o sprite.
    Fish(u16),
    /// Pet coletor de um jogador (docs/PETS.md). u16 = item_id do pet, que ja'
    /// carrega especie e grau — o cliente tira dele o modelo e a cor.
    Pet(u16),
}

/// Slot de inventario. None = vazio. Quando `qty == 0`, o slot esta vazio.
///
/// `instance`: Some(...) para itens equipáveis dropados (atributos fixos
/// + cor + tier + refino). None pra itens stackáveis (gold, poções) ou
/// itens legacy pre-Fase A — esses usam stats base via `item_bonus`.
// `PartialEq` (sem `Eq`): a instancia tem float nos afixos. Existe porque o
// BAU DA ILHA (`colonia::DadosColonia`) e' comparado inteiro pro painel saber
// se mudou.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default, PartialEq)]
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
        mp_regen: crate::constants::MP_REGEN_PER_SEC,
        attack_speed_mult: 1.0,
        stamina_max: 100,
        stamina_regen: 15.0,
        block_dmg_reduction: 0.6,       // 60% absorvido por block (base)
        defense_stamina_cost_mult: 1.0, // 100% do custo base (RES reduz)
        damage_reduction_pct: 0.0,      // breakpoints de VIT/RES somam aqui
        bow_range_bonus_pct: 0.0,       // Eagle Eye passive (Bow T1)
        dash_cd_mult: 1.0,              // SPD soma reduction por ponto
        poise_max: 0,                   // hardcore: zero poise base — gateado em skill T4 (lvl 60+)
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
    /// MP regenerado por segundo. Era a constante `MP_REGEN_PER_SEC` fixa;
    /// virou stat pra skill de pet poder somar (docs/PETS.md). Ficha antiga
    /// no banco le' o valor base.
    #[serde(default = "default_mp_regen")]
    pub mp_regen: f32,
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

fn default_poise_max() -> i32 {
    50
}

fn default_block_reduction() -> f32 {
    0.6
}
fn default_one() -> f32 {
    1.0
}

fn default_stamina_max() -> i32 {
    100
}
fn default_mp_regen() -> f32 {
    crate::constants::MP_REGEN_PER_SEC
}

fn default_stamina_regen() -> f32 {
    15.0
}

fn default_speed_mult() -> f32 {
    1.0
}

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
    #[serde(default)]
    pub offhand: Option<u16>,
    #[serde(default)]
    pub offhand_inst: Option<crate::items::ItemInstance>,
    #[serde(default)]
    pub armor: Option<u16>,
    #[serde(default)]
    pub armor_inst: Option<crate::items::ItemInstance>,
    #[serde(default)]
    pub earring: Option<u16>,
    #[serde(default)]
    pub earring_inst: Option<crate::items::ItemInstance>,
    #[serde(default)]
    pub necklace: Option<u16>,
    #[serde(default)]
    pub necklace_inst: Option<crate::items::ItemInstance>,
    #[serde(default)]
    pub bracelet: Option<u16>,
    #[serde(default)]
    pub bracelet_inst: Option<crate::items::ItemInstance>,
    #[serde(default)]
    pub belt: Option<u16>,
    #[serde(default)]
    pub belt_inst: Option<crate::items::ItemInstance>,
    /// O pet coletor equipado (docs/PETS.md).
    #[serde(default)]
    pub pet: Option<u16>,
    #[serde(default)]
    pub pet_inst: Option<crate::items::ItemInstance>,
    /// A montaria equipada (docs/MONTARIAS.md).
    #[serde(default)]
    pub montaria: Option<u16>,
    #[serde(default)]
    pub montaria_inst: Option<crate::items::ItemInstance>,
    #[serde(default)]
    pub pet2: Option<u16>,
    #[serde(default)]
    pub pet2_inst: Option<crate::items::ItemInstance>,
    #[serde(default)]
    pub pet3: Option<u16>,
    #[serde(default)]
    pub pet3_inst: Option<crate::items::ItemInstance>,
}

impl Equipment {
    fn campo(
        &mut self,
        slot: crate::constants::EquipSlot,
    ) -> (&mut Option<u16>, &mut Option<crate::items::ItemInstance>) {
        use crate::constants::EquipSlot::*;
        match slot {
            Weapon => (&mut self.weapon, &mut self.weapon_inst),
            Offhand => (&mut self.offhand, &mut self.offhand_inst),
            Armor => (&mut self.armor, &mut self.armor_inst),
            Earring => (&mut self.earring, &mut self.earring_inst),
            Necklace => (&mut self.necklace, &mut self.necklace_inst),
            Bracelet => (&mut self.bracelet, &mut self.bracelet_inst),
            Belt => (&mut self.belt, &mut self.belt_inst),
            Pet => (&mut self.pet, &mut self.pet_inst),
            Pet2 => (&mut self.pet2, &mut self.pet2_inst),
            Pet3 => (&mut self.pet3, &mut self.pet3_inst),
            Montaria => (&mut self.montaria, &mut self.montaria_inst),
        }
    }

    /// Lê o item_id atualmente equipado em `slot` (None = vazio).
    pub fn get(&self, slot: crate::constants::EquipSlot) -> Option<u16> {
        let mut c = *self;
        *c.campo(slot).0
    }

    /// Lê a ItemInstance do slot (None = item sem rolagem).
    pub fn get_inst(
        &self,
        slot: crate::constants::EquipSlot,
    ) -> Option<crate::items::ItemInstance> {
        let mut c = *self;
        *c.campo(slot).1
    }

    /// Sobrescreve item_id e instance do slot.
    pub fn set(
        &mut self,
        slot: crate::constants::EquipSlot,
        id: Option<u16>,
        inst: Option<crate::items::ItemInstance>,
    ) {
        let (i, n) = self.campo(slot);
        *i = id;
        *n = inst;
    }

    /// Todos os slots, na ordem de `EquipSlot::TODOS` — usado por effective_stats.
    pub fn iter_equipped(&self) -> Vec<(Option<u16>, Option<crate::items::ItemInstance>)> {
        crate::constants::EquipSlot::TODOS
            .iter()
            .map(|s| (self.get(*s), self.get_inst(*s)))
            .collect()
    }

    pub fn pets(&self) -> [(crate::constants::EquipSlot, Option<u16>, Option<crate::items::ItemInstance>); 3] {
        use crate::constants::EquipSlot;
        [EquipSlot::Pet, EquipSlot::Pet2, EquipSlot::Pet3].map(|slot| (slot, self.get(slot), self.get_inst(slot)))
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
    Other,
    /// Pet coletor. O `EntityMeta::kind` carrega o item_id do pet, de onde o
    /// cliente tira o modelo e a cor do grau.
    Pet,
}

/// O que o corpo esta' fazendo, num byte do `EntityState`.
///
/// ```text
/// bit   7 6    | 5 4 3 | 2          | 1 0
///       variante | gesto | em combate | conjunto
/// ```
///
/// O conjunto viaja no TICK, e nao na meta, porque trocar de arma no meio do
/// jogo nao reenvia a meta — e um byte que quase nunca muda nao pesa no delta.
/// A variante e' o passo do combo (0-2) ou a ordem da skill (0-2).
pub mod acao {
    pub const NADA: u8 = 0;
    pub const GOLPE: u8 = 1;
    pub const SKILL: u8 = 2;
    /// Coletando PEDRA com a picareta. Variante = cor da pedra - 1 (0 cinza ..
    /// 3 roxa): e' ela que pinta a cabeca da picareta de quem esta' de fora.
    pub const COLETA: u8 = 3;
    /// Coletando MADEIRA com o machado (golpe de lado). Variante 0.
    pub const COLETA_MADEIRA: u8 = 4;
    /// Quanto o gesto fica aceso depois de comecar, em segundos. Um quadro so'
    /// se perderia num snapshot pulado; o cliente toca na borda de subida ou
    /// quando a variante muda.
    pub const SEGURA_S: f32 = 0.2;
    /// Sem atacar, conjurar nem apanhar por isto, a arma volta pra bainha
    /// (docs/PERSONAGEM.md).
    pub const EM_COMBATE_S: f32 = 8.0;

    pub fn monta(conjunto: u8, em_combate: bool, gesto: u8, variante: u8) -> u8 {
        (conjunto & 0b11)
            | ((em_combate as u8) << 2)
            | ((gesto & 0b111) << 3)
            | ((variante & 0b11) << 6)
    }
    pub fn conjunto(a: u8) -> u8 {
        a & 0b11
    }
    pub fn em_combate(a: u8) -> bool {
        a & 0b100 != 0
    }
    pub fn gesto(a: u8) -> u8 {
        (a >> 3) & 0b111
    }
    pub fn variante(a: u8) -> u8 {
        a >> 6
    }

    /// O byte de quem esta' coletando o tipo `tipo` (0 madeira, 1..4 pedra
    /// pela cor).
    pub fn monta_coleta(conjunto: u8, em_combate: bool, tipo: u8) -> u8 {
        if tipo == 0 {
            monta(conjunto, em_combate, COLETA_MADEIRA, 0)
        } else {
            monta(conjunto, em_combate, COLETA, tipo.clamp(1, 4) - 1)
        }
    }

    /// O que o byte diz que esta' sendo coletado: 0 madeira, 1..4 pedra.
    pub fn tipo_da_coleta(a: u8) -> Option<u8> {
        match gesto(a) {
            COLETA_MADEIRA => Some(0),
            COLETA => Some(variante(a) + 1),
            _ => None,
        }
    }

    #[cfg(test)]
    mod testes {
        use super::*;

        #[test]
        fn a_coleta_leva_o_tipo_no_byte() {
            for tipo in 0..=4u8 {
                for c in 0..4 {
                    let a = monta_coleta(c, true, tipo);
                    assert_eq!(tipo_da_coleta(a), Some(tipo));
                    assert_eq!(conjunto(a), c);
                }
            }
            assert_eq!(tipo_da_coleta(monta(1, false, GOLPE, 2)), None);
        }

        #[test]
        fn o_byte_da_a_volta() {
            for c in 0..4 {
                for combate in [false, true] {
                    for g in [NADA, GOLPE, SKILL, COLETA, COLETA_MADEIRA] {
                        for v in 0..3 {
                            let a = monta(c, combate, g, v);
                            assert_eq!(
                                (conjunto(a), em_combate(a), gesto(a), variante(a)),
                                (c, combate, g, v)
                            );
                        }
                    }
                }
            }
        }
    }
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
    /// O mob comecou um golpe ha' pouco. O servidor segura o bit uns quadros
    /// (um quadro so' se perderia num snapshot pulado); o cliente toca a
    /// animacao na borda de subida.
    pub const ATACANDO: u8 = 1 << 5;
    /// Montado (docs/MONTARIAS.md). A skin vai no `EntityMeta::kind` do
    /// jogador.
    pub const MONTADO: u8 = 1 << 6;
    pub const DASHING: u8 = 1 << 7;
}

/// Precisao da posicao no wire: 1/8 de tile.
///
/// Com `i16` isso cobre +-4095 tiles e corta a posicao de 8 bytes (2x f32)
/// pra 4. Um decimo de pixel de erro num jogo de vista de cima ninguem
/// enxerga — e o cliente interpola por cima disso.
///
/// **Era 1/16 ate' 21/09/2026, e apertava em dois lugares ao mesmo tempo.**
/// `quantize` CLAMPA, em silencio:
///
/// - **posicao**, com `i16`, cobria +-2047,9 u. O Mar Aberto poe as quatro
///   ilhas num espaco de coordenadas so' (`terreno::ARQUIPELAGO`), e o
///   Planalto mora em x = -3600: toda entidade a oeste de -2048 empilharia no
///   mesmo ponto, sem um erro em log nenhum.
/// - **velocidade**, com `i8`, cobria +-7,94 u/s. Um casco a 11 u/s ja'
///   saturava — e `vel` e' o que o cliente usa pra girar o modelo e decidir
///   se anda.
///
/// Meia escala conserta os dois, e o fio nao muda: mesmo `i16`, mesmo `i8`,
/// mesmos bytes. So' a precisao cai de 6,25 cm pra 12,5 cm, que continua
/// abaixo do que se enxerga.
pub const POS_SCALE: f32 = 8.0;

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
    /// Qual mob da tabela (`enemy_kinds.kind`) — e' por ele que o cliente
    /// escolhe o MODELO. No SAQUE, e' o tier do item (1-4; 0 = ouro/pocao,
    /// sem tier), que pinta a faixa do saquinho. Zero pro resto. Vai so' na meta, que sai
    /// uma vez quando a entidade entra na visao: nao pesa por tick.
    pub kind: u16,
    /// Nivel: o do mob, ou o do personagem. Vai na placa em cima da cabeca.
    pub nivel: u16,
    /// APARENCIA empacotada (`aparencia::Aparencia::empacota`). Zero = o
    /// corpo padrao — e' o que mob, saque e projetil mandam.
    ///
    /// Aqui e nao no `kind`: o `kind` do Player ja' carrega o item_id da
    /// montaria. E aqui e nao numa mensagem propria: a meta ja' e' o lugar do
    /// dado estavel por entidade, e ja' tem reenvio forcado
    /// (`loja_mundo::atualizar_montaria_vista`).
    #[serde(default)]
    pub aparencia: u32,
}

/// Rumo de NPC parado, guardado no `EntityMeta::kind` (so' pra
/// `EntityTag::Npc`, onde o campo nao tinha uso).
///
/// O estado por tick nao leva angulo: o cliente tira o rumo da velocidade, e
/// NPC parado nascia olhando pro +Z — o vendedor de costas pra propria porta.
/// Vai na meta, que sai uma vez, e nao pesa por tick.
///
/// `0` = sem rumo. `1..=256` = a volta inteira em 256 passos. Convencao do
/// cliente: `yaw = atan2(dir.x, dir.z)`.
pub fn kind_de_npc_yaw(yaw: f32) -> u16 {
    let t = yaw.rem_euclid(std::f32::consts::TAU) / std::f32::consts::TAU;
    1 + ((t * 256.0).round() as u16 % 256)
}

/// Bits de baixo do `kind` de NPC que guardam o rumo (0..=256 cabe em 9).
const NPC_BITS_DO_RUMO: u16 = 9;
const NPC_MASCARA_DO_RUMO: u16 = (1 << NPC_BITS_DO_RUMO) - 1;

/// O `EntityMeta.kind` de um NPC da vila: rumo E oficio no mesmo `u16`.
///
/// Os 9 bits de baixo sao o rumo (`kind_de_npc_yaw`, 0 = sem rumo) e os de
/// cima o PAPEL (`shared::construcao::Papel as u8`). O papel e' o que o
/// cliente usa pra escolher o modelo: sem ele todo NPC saia com o corpo do
/// jogador.
pub fn npc_kind(yaw: Option<f32>, papel: u8) -> u16 {
    ((papel as u16) << NPC_BITS_DO_RUMO) | yaw.map_or(0, kind_de_npc_yaw)
}

/// O rumo que `npc_kind`/`kind_de_npc_yaw` guardou, se houver. Ignora o
/// papel: um `kind` so' de rumo (sem papel) continua valendo.
pub fn npc_yaw_de_kind(kind: u16) -> Option<f32> {
    let rumo = kind & NPC_MASCARA_DO_RUMO;
    if rumo == 0 || rumo > 256 {
        return None;
    }
    Some((rumo - 1) as f32 / 256.0 * std::f32::consts::TAU)
}

/// O papel que `npc_kind` guardou. `0` (`Papel::Casa`) quando nao veio papel.
pub fn npc_papel_de_kind(kind: u16) -> u8 {
    (kind >> NPC_BITS_DO_RUMO) as u8
}

#[cfg(test)]
mod testes_npc_kind {
    use super::*;

    #[test]
    fn rumo_e_papel_vao_e_voltam_juntos() {
        for papel in [0u8, 1, 14, 15, 40, 127] {
            for passo in 0..256 {
                let yaw = passo as f32 / 256.0 * std::f32::consts::TAU;
                let k = npc_kind(Some(yaw), papel);
                assert_eq!(npc_papel_de_kind(k), papel);
                let volta = npc_yaw_de_kind(k).unwrap();
                assert!(
                    (volta - yaw).abs() < 1e-3,
                    "papel {papel}, passo {passo}: {volta} != {yaw}"
                );
            }
            let sem = npc_kind(None, papel);
            assert_eq!(npc_yaw_de_kind(sem), None);
            assert_eq!(npc_papel_de_kind(sem), papel);
        }
    }

    /// O `kind` antigo, so' com rumo, continua lido igual.
    #[test]
    fn kind_so_de_rumo_continua_valendo() {
        let k = kind_de_npc_yaw(2.0);
        assert_eq!(npc_papel_de_kind(k), 0);
        assert!(
            (npc_yaw_de_kind(k).unwrap() - npc_yaw_de_kind(npc_kind(Some(2.0), 0)).unwrap()).abs()
                < 1e-6
        );
    }
}

/// Estado de uma entidade num tick. E' o unico dado que se repete.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct EntityState {
    pub id: EntityId,
    /// Posicao em 1/8 de tile (ver `POS_SCALE`).
    pub pos: [i16; 2],
    /// Velocidade em 1/8 de tile/s, saturada. O cliente usa pra girar o
    /// modelo e decidir se anda — nao precisa de precisao.
    pub vel: [i8; 2],
    pub hp: u16,
    /// Ver `ent_flags`.
    pub flags: u8,
    /// O que o corpo esta' fazendo — ver `acao`.
    pub acao: u8,
    /// Pra onde o corpo OLHA, quando o servidor sabe (`rumo_de_dir`): 0 = sem
    /// rumo (o cliente segue a velocidade), 1..=255 = a volta em 255 passos.
    ///
    /// Sem ele quem estava de fora so' via o rumo pela velocidade: parado
    /// coletando, atacando ou mirando, o boneco ficava olhando pro ultimo
    /// passo. Um byte, e so' muda quando o corpo vira — nao pesa no delta.
    pub rumo: u8,
}

/// Passos da volta no `EntityState::rumo` (0 fica pra "sem rumo").
const RUMO_PASSOS: f32 = 255.0;

/// O `rumo` de quem olha na direcao `dir` (x, z do mundo). Direcao nula = 0.
/// Convencao do cliente: `yaw = atan2(dir.x, dir.z)`.
pub fn rumo_de_dir(dir: Vec2) -> u8 {
    if dir.length_squared() < 1e-6 || !dir.is_finite() {
        return 0;
    }
    let yaw = dir.x.atan2(dir.y);
    rumo_de_yaw(yaw)
}

pub fn rumo_de_yaw(yaw: f32) -> u8 {
    let t = yaw.rem_euclid(std::f32::consts::TAU) / std::f32::consts::TAU;
    1 + ((t * RUMO_PASSOS).round() as u32 % RUMO_PASSOS as u32) as u8
}

/// O yaw que o `rumo` guarda, se houver.
pub fn yaw_de_rumo(rumo: u8) -> Option<f32> {
    (rumo != 0).then(|| (rumo - 1) as f32 / RUMO_PASSOS * std::f32::consts::TAU)
}

#[cfg(test)]
mod testes_rumo {
    use super::*;

    #[test]
    fn o_rumo_da_a_volta_com_erro_de_menos_de_um_passo() {
        for i in 0..720 {
            let yaw = i as f32 / 720.0 * std::f32::consts::TAU;
            let r = rumo_de_yaw(yaw);
            assert_ne!(r, 0);
            let volta = yaw_de_rumo(r).unwrap();
            let mut d = (volta - yaw).rem_euclid(std::f32::consts::TAU);
            if d > std::f32::consts::PI {
                d = std::f32::consts::TAU - d;
            }
            assert!(
                d <= std::f32::consts::TAU / RUMO_PASSOS * 0.51,
                "yaw {yaw}: voltou {volta}"
            );
        }
        assert_eq!(rumo_de_dir(Vec2::ZERO), 0);
        assert_eq!(yaw_de_rumo(0), None);
        // +Z e' yaw 0; +X e' um quarto de volta.
        assert!(yaw_de_rumo(rumo_de_dir(Vec2::new(0.0, 1.0))).unwrap().abs() < 0.03);
        let x = yaw_de_rumo(rumo_de_dir(Vec2::new(1.0, 0.0))).unwrap();
        assert!((x - std::f32::consts::FRAC_PI_2).abs() < 0.03);
    }
}

impl EntityState {
    pub fn pos_f32(&self) -> Vec2 {
        Vec2::new(
            self.pos[0] as f32 / POS_SCALE,
            self.pos[1] as f32 / POS_SCALE,
        )
    }

    pub fn vel_f32(&self) -> Vec2 {
        Vec2::new(
            self.vel[0] as f32 / POS_SCALE,
            self.vel[1] as f32 / POS_SCALE,
        )
    }

    pub fn quantize(id: EntityId, pos: Vec2, vel: Vec2, hp: i32, flags: u8) -> Self {
        let q = |v: f32| {
            (v * POS_SCALE)
                .round()
                .clamp(i16::MIN as f32, i16::MAX as f32) as i16
        };
        let qv = |v: f32| {
            (v * POS_SCALE)
                .round()
                .clamp(i8::MIN as f32, i8::MAX as f32) as i8
        };
        Self {
            id,
            pos: [q(pos.x), q(pos.y)],
            vel: [qv(vel.x), qv(vel.y)],
            hp: hp.max(0) as u16,
            flags,
            acao: 0,
            rumo: 0,
        }
    }
}

/// Bits do `EntitySnapshot.buffs` — mantém em sync com o cliente C#.
pub mod buffs_mask {
    pub const BLOODTHIRST: u8 = 1 << 0;
    pub const HUNTERS_MARK: u8 = 1 << 1;
    pub const AURA_TIDE: u8 = 1 << 2;
    pub const AURA_IGNITION: u8 = 1 << 3;
    pub const AURA_MIST: u8 = 1 << 4;
    pub const AURA_TEMPEST: u8 = 1 << 5;
}

/// Codigo enviado em `EntitySnapshot.attack_anim` pra discriminar qual
/// animacao o cliente deve tocar no atacante. O server escolhe baseado na
/// arma equipada.
pub mod attack_anim {
    pub const SLASH: u8 = 0; // Sword/Dagger/GreatSword/Unarmed (pONE3)
    pub const SHOOT: u8 = 1; // Bow (pBOW3)
    pub const THRUST: u8 = 2; // Staff/Wand (pONE3 Thrust)
    pub const ENEMY_SWING: u8 = 3; // Inimigo melee (compat com `attacking`)
    pub const ENEMY_SHOOT: u8 = 4; // Inimigo ranged (Goblin Archer / Mago)
    pub const DASH: u8 = 5; // Dash do player (anim de jump, p1 cols 4-7)
    pub const PARRY_FLASH: u8 = 6; // Parry sucesso — full ShieldBash swing + flash
    pub const SHIELD_BASH: u8 = 7; // Shield Bash skill (1003) — pONE3 ShieldBash
    pub const TOOL_SWING: u8 = 8; // Rock (mine) e Tree (wood) — p2 rows 0-3
    pub const TOOL_GATHER: u8 = 9; // Flower — p2 rows 4-7
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
    fn default() -> Self {
        Faction::Peacemain
    }
}

impl Faction {
    /// Parse tolerante (case-insensitive) — usado ao ler do DB (TEXT).
    pub fn from_str_lenient(s: &str) -> Option<Faction> {
        match s.trim().to_ascii_lowercase().as_str() {
            "morganeers" => Some(Faction::Morganeers),
            "peacemain" => Some(Faction::Peacemain),
            _ => None,
        }
    }
    /// String estável pra persistir no DB.
    pub fn as_db_str(self) -> &'static str {
        match self {
            Faction::Morganeers => "morganeers",
            Faction::Peacemain => "peacemain",
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
    pub fn overworld() -> Self {
        Self("overworld".into())
    }
}
