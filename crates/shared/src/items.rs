//! Sistema de itemização.
//!
//! Modelo:
//!  - `ItemTemplate`: definição estática por item_id (faixas históricas).
//!  - `ItemInstance`: instância única gerada no drop, com atributos fixos
//!    pela combinação item + cor + tier + item level, além do refino.
//!  - `InventorySlot.instance`: Option<ItemInstance> — quando None, item
//!    usa stats base (compat com itens antigos pre-Fase A).
//!
//! Stats finais = valor central do template × cor × tier × item level ×
//! refino, mais a parte fixa do refino por nivel (`refino_fixo`). Não há
//! sorteio nem afixo aleatório.
//!
//! Sincronizar: ItemInstance é serializada como JSONB na coluna
//! `inventory.instance_data` e replicada via wire pro cliente em
//! `InventorySlot.instance`.

use serde::{Deserialize, Serialize};

// ── Grau/cor do item (1–5) ───────────────────────────────────────────────
// O item level determina a cor inicial: cinza, verde, azul, roxo ou laranja.
// Dentro de cada cor, `ItemInstance::tier` guarda o Tier I–IV. Os dois são
// determinísticos e multiplicam os atributos fixos do template.
//
// O nome `tier_from_ilvl` e o campo `ItemInstance.rarity` foram mantidos por
// compatibilidade com o wire/DB, mas ambos representam o GRAU (cor) 1–5.

/// Grau/cor (1–5) a partir do item level. Nome legado mantido por compatibilidade.
pub fn tier_from_ilvl(item_level: u16) -> u8 {
    match item_level {
        0..=10 => 1,
        11..=25 => 2,
        26..=45 => 3,
        46..=70 => 4,
        _ => 5,
    }
}

/// Multiplicador dos atributos fixos por grau/cor (cinza → lendário).
pub fn tier_stat_mult(tier: u8) -> f32 {
    match tier {
        1 => 0.6,
        2 => 0.9,
        3 => 1.2,
        4 => 1.5,
        _ => 1.9,
    }
}

/// Cor RGB (#RRGGBB) por grau. Fallback — o cliente tem a própria tabela.
pub fn tier_color_hex(tier: u8) -> &'static str {
    match tier {
        1 => "#bfbfbf", // cinza
        2 => "#5fd35f", // verde
        3 => "#5577ff", // azul
        4 => "#aa55ff", // roxo
        _ => "#ff7733", // laranja
    }
}

/// Nome curto legado do grau.
pub fn tier_name(tier: u8) -> &'static str {
    match tier {
        1 => "T1",
        2 => "T2",
        3 => "T3",
        4 => "T4",
        _ => "T5",
    }
}

/// Range de cada stat no template (min..=max inteiros, antes de
/// aplicar o multiplicador. O jogo usa o ponto central; min/max continuam
/// no modelo para manter as tabelas existentes legíveis.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
pub struct StatRange {
    pub min: i32,
    pub max: i32,
}

impl StatRange {
    pub const fn new(min: i32, max: i32) -> Self {
        Self { min, max }
    }
    pub const fn zero() -> Self {
        Self { min: 0, max: 0 }
    }
    pub fn is_zero(&self) -> bool {
        self.min == 0 && self.max == 0
    }

    /// Roll dentro do range. Retorna inteiro arredondado.
    pub fn roll(&self, r: f32, rarity_mult: f32) -> i32 {
        if self.is_zero() {
            return 0;
        }
        let lo = self.min as f32;
        let hi = self.max as f32;
        let raw = lo + r * (hi - lo); // [min, max)
        (raw * rarity_mult).round() as i32
    }
}

/// Template — ranges de stat por item_id. Cada item equipável tem o seu.
/// Não-equipáveis (gold, potions, materiais) não têm template e não geram
/// instances no drop.
#[derive(Debug, Clone, Copy, Default)]
pub struct ItemTemplate {
    pub hp_max: StatRange,
    pub mp_max: StatRange,
    pub attack_damage: StatRange,
    pub dex: StatRange,
    pub wis: StatRange,
    pub defense: StatRange,
}

impl ItemTemplate {
    pub fn has_any_range(&self) -> bool {
        !(self.hp_max.is_zero()
            && self.mp_max.is_zero()
            && self.attack_damage.is_zero()
            && self.dex.is_zero()
            && self.wis.is_zero()
            && self.defense.is_zero())
    }
}

/// Lookup do template por item_id. Item não-equipável retorna template
/// vazio (sem ranges → drop não gera instance).
pub fn item_template(item_id: u16) -> ItemTemplate {
    use crate::constants::item_id::*;
    let r = StatRange::new;
    match item_id {
        // === a arma: o conjunto ===
        ESPADA_E_ESCUDO => ItemTemplate {
            attack_damage: r(8, 16),
            hp_max: r(10, 30),
            ..Default::default()
        },
        KATANA => ItemTemplate {
            attack_damage: r(8, 15),
            dex: r(5, 12),
            ..Default::default()
        },
        PISTOLAS => ItemTemplate {
            attack_damage: r(7, 14),
            dex: r(6, 13),
            ..Default::default()
        },
        ANEL_MAGICO => ItemTemplate {
            attack_damage: r(6, 13),
            mp_max: r(30, 70),
            wis: r(4, 10),
            ..Default::default()
        },
        // === a secundaria de cada conjunto ===
        MANTO_DO_GUERREIRO => ItemTemplate {
            hp_max: r(20, 50),
            defense: r(3, 8),
            ..Default::default()
        },
        BAINHA => ItemTemplate {
            attack_damage: r(1, 4),
            dex: r(3, 8),
            ..Default::default()
        },
        COLDRE => ItemTemplate {
            attack_damage: r(2, 5),
            dex: r(3, 7),
            ..Default::default()
        },
        MANTO_DO_MAGO => ItemTemplate {
            mp_max: r(25, 60),
            wis: r(3, 8),
            ..Default::default()
        },
        // === armadura: o peso e' a escolha (o dano/resistencia do peso sai de
        // `peso_da_armadura`, aqui e' so' o que ela rola) ===
        ARMADURA_LEVE => ItemTemplate {
            hp_max: r(15, 35),
            defense: r(1, 4),
            dex: r(2, 6),
            ..Default::default()
        },
        ARMADURA_MEDIA => ItemTemplate {
            hp_max: r(30, 60),
            defense: r(4, 9),
            ..Default::default()
        },
        ARMADURA_PESADA => ItemTemplate {
            hp_max: r(60, 120),
            defense: r(8, 16),
            ..Default::default()
        },
        // === acessorios: iguais pra todo mundo ===
        BRINCO => ItemTemplate {
            attack_damage: r(1, 4),
            dex: r(2, 6),
            ..Default::default()
        },
        AMULETO => ItemTemplate {
            mp_max: r(20, 50),
            wis: r(2, 6),
            ..Default::default()
        },
        BRACELETE => ItemTemplate {
            attack_damage: r(2, 5),
            defense: r(1, 3),
            ..Default::default()
        },
        CINTO => ItemTemplate {
            hp_max: r(20, 45),
            defense: r(1, 3),
            ..Default::default()
        },
        _ => ItemTemplate::default(),
    }
}

/// Instância única dropada. Substitui o uso de `item_bonus(id)` (estático)
/// pra itens que tenham instance — ItemBonus base ainda é fallback pra
/// itens sem instance (legacy).
///
/// Refinement: +0 a `MAX_REFINE`. Cada nível adiciona
/// `REFINE_BOOST_PER_LEVEL` × stats. NPC vendor (futuro) gasta ouro pra
/// upgrade; chance de falha cresce com o nível.
/// Máximo de slots legados de afixo. O campo continua no wire/DB por
/// compatibilidade, mas peças atuais sempre o deixam vazio.
pub const MAX_AFFIXES: usize = 4;

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct ItemInstance {
    /// GRAU (a cor) do item, 1 cinza .. 5 lendario. Nome `rarity` mantido por
    /// compat de wire/DB. Sai do nivel do item no drop/craft
    /// (`tier_from_ilvl`, nome antigo) e sobe no Aprimorar.
    pub rarity: u8,
    pub refinement: u8,
    /// Item Level — vem do enemy que dropou. Escala stats no roll
    /// (Fase C). Default 1 pra drops antigos.
    #[serde(default = "default_ilvl")]
    pub item_level: u16,
    /// Level mínimo do player pra equipar. None = sem requirement.
    #[serde(default)]
    pub level_req: Option<u16>,
    pub hp_max: i32,
    pub mp_max: i32,
    pub attack_damage: i32,
    pub dex: i32,
    pub wis: i32,
    pub defense: i32,
    /// Slots legados de afixo. Posições vazias têm name_id=0; o sistema
    /// atual sempre os zera para que os atributos sejam determinísticos.
    #[serde(default)]
    pub affixes: [AffixSlot; MAX_AFFIXES],
    /// Sockets disponíveis (vem da rarity). 0..3.
    #[serde(default)]
    pub sockets: u8,
    /// Gemas inseridas nos sockets (item_id da gema, 0 = vazio).
    /// Aplicado em ordem: socketed_gems[0] vai pro 1° socket, etc.
    #[serde(default)]
    pub socketed_gems: [u16; 3],
    /// Vinculada ao personagem: nao entra no mercado (peca de bau de dungeon).
    #[serde(default)]
    pub vinculado: bool,
    /// TIER dentro da cor, I..IV (`forja::TIER_MAX`). Dois iguais viram o de
    /// cima no Aprimorar; duas Tier IV +8 sobem de cor e voltam ao I. Peca de
    /// antes deste campo (banco em JSON) le' como Tier I.
    #[serde(default = "tier_um")]
    pub tier: u8,
    /// Estado do PET (docs/PETS.md): nivel, fome e skills. Viaja com o item,
    /// entao pet vendido no mercado leva o que voce criou junto. `None` = pet
    /// de antes deste campo, lido como nivel 1 com fome e sem skill.
    #[serde(default)]
    pub pet: Option<PetData>,
    /// AFINIDADE rolada do pet ou da montaria: `[principal, secundario]`,
    /// indices de `stat_idx`. Decide em que atributos os pontos da criatura
    /// caem — e e' o que a Pedra de Afinidade re-rola (docs/PETS.md).
    ///
    /// `None` = bicho de antes deste campo, que usa a afinidade fixa da
    /// criatura. Ninguem perde o que tinha: so' nao da' pra re-rolar ate'
    /// rolar a primeira vez.
    #[serde(default)]
    pub afinidade: Option<[u8; 2]>,
    /// Estado do BARCO (docs/MAR_ABERTO.md): casco, melhorias e
    /// quilometragem. Viaja com o item, entao barco vendido no mercado leva
    /// junto tudo o que o dono investiu nele. `None` = casco de antes deste
    /// campo, lido como cheio e sem melhoria.
    #[serde(default)]
    pub barco: Option<BarcoData>,
}

/// O que o BARCO acumula.
///
/// Mora na instancia porque o barco e' item: assim casco, melhorias e
/// quilometragem sobrevivem ao mercado e ao banco sem tabela propria — e' o
/// mesmo arranjo do `PetData`.
///
/// **12 bytes e `Copy`**, e essa restricao e' carga: `ItemInstance` e' `Copy`
/// e existe uma vez por slot de bolsa, de banco, de anuncio e de envio no
/// fio. E' por isso que o PORAO (corte 4) NAO vai morar aqui — um porao
/// dentro da instancia somaria centenas de bytes a todo item do jogo, e
/// instancia nao pode conter instancia.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct BarcoData {
    /// Pontos de casco AGORA. 0 = naufragado: nao zarpa ate' reparar.
    pub casco: u16,
    /// Nivel de cada melhoria: [casco, vela, porao, canhao]. O canhao e'
    /// fase 2 e fica em 0 ate' la'.
    pub melhorias: [u8; 4],
    /// Travessias concluidas. Nao faz nada mecanicamente: e' o hodometro que
    /// o mercado le' ("Nau, 340 travessias, 2 naufragios").
    pub travessias: u32,
    /// Quantas vezes afundou. Honestidade no anuncio.
    pub afundou: u32,
    /// O BAU no CONVES: o tesouro de chefe global sendo carregado. 0 = nada.
    ///
    /// Um por vez, e no barco e nao no jogador. Nao ha' porao — decisao do
    /// dono: *"nao precisa de porao, e o tesouro vai ficar no conves mesmo"*.
    /// Um so' mantem a marca de PK BINARIA (ou voce carrega, ou nao) e impede
    /// uma guilda de juntar seis baus num galeao defendido.
    pub carga: u16,
    /// De que ilha o bau veio (indice do `ARQUIPELAGO` + 1; 0 = nenhuma). E'
    /// o que faz a distancia valer dinheiro na entrega.
    pub carga_de: u8,
}

/// O que o pet acumula. Fica na instancia porque o pet e' item: assim nivel e
/// skills sobrevivem ao mercado, ao banco e ao correio sem tabela propria.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct PetData {
    pub xp: u64,
    /// Unix em segundos ate' quando esta' alimentado. Com fome, o pet nao
    /// recebe XP nenhuma (docs/PETS.md).
    #[serde(default)]
    pub alimentado_ate: i64,
    /// Skills instaladas (item_id, 0 = slot vazio). O nivel libera os slots.
    #[serde(default)]
    pub skills: [u16; 3],
}

impl ItemInstance {
    /// Uma instancia sem atributo nenhum, so' com a cor. E' o que um PET usa:
    /// ele nao tem roll, mas precisa de instancia pra carregar nivel e skills
    /// (docs/PETS.md).
    pub fn vazia_de_grau(grau: u8) -> ItemInstance {
        ItemInstance {
            rarity: grau.clamp(1, 5),
            refinement: 0,
            item_level: 1,
            level_req: None,
            hp_max: 0,
            mp_max: 0,
            attack_damage: 0,
            dex: 0,
            wis: 0,
            defense: 0,
            affixes: [AffixSlot::default(); MAX_AFFIXES],
            pet: None,
            afinidade: None,
            barco: None,
            sockets: 0,
            socketed_gems: [0; 3],
            vinculado: false,
            tier: 1,
        }
    }
}

fn default_ilvl() -> u16 {
    1
}

fn tier_um() -> u8 {
    1
}

/// Quanto cada tier soma dentro da cor: +15% por degrau (docs/ITENS.md). O
/// Tier IV (1,52x) fica perto do I da cor de cima.
pub fn bonus_do_tier(tier: u8) -> f32 {
    1.15f32.powi(tier.clamp(1, 4) as i32 - 1)
}

/// Slot de affix — name_id=0 = vazio.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
pub struct AffixSlot {
    pub name_id: u16, // 0 = vazio
    pub stat: u8,     // AffixStat as u8
    pub tier: u8,
    pub value: i32,
    pub value_pct: f32,
    pub is_prefix: bool,
}

impl AffixSlot {
    pub fn from_affix(a: Affix) -> Self {
        AffixSlot {
            name_id: a.name_id,
            stat: a.stat as u8,
            tier: a.tier,
            value: a.value,
            value_pct: a.value_pct,
            is_prefix: a.is_prefix,
        }
    }
    pub fn is_empty(&self) -> bool {
        self.name_id == 0
    }

    pub fn stat(&self) -> AffixStat {
        match self.stat {
            1 => AffixStat::Mp,
            2 => AffixStat::Attack,
            3 => AffixStat::Defense,
            4 => AffixStat::Dex,
            5 => AffixStat::Wis,
            6 => AffixStat::CritChance,
            7 => AffixStat::AttackSpeed,
            8 => AffixStat::MoveSpeed,
            9 => AffixStat::HpRegen,
            _ => AffixStat::Hp,
        }
    }
}

pub const MAX_REFINE: u8 = 15;
/// Parte percentual do refino: +8% dos atributos da peca por nivel.
///
/// Era +5% e SO' isso: numa peca cinza (vida 13, defesa 1) o +1 arredondava
/// pra nada e o +4 dava +3 de poder em 767 — o dono refinou ate' +4 e nao viu
/// mudar. Agora ha' tambem a parte FIXA (`refino_fixo`), que e' o que pesa no
/// comeco; a percentual e' o que pesa em peca boa.
pub const REFINE_BOOST_PER_LEVEL: f32 = 0.08;

/// Parte fixa do refino, por nivel, na escala do nivel do item: 1 no item
/// nivel 5, 4 no 18, 7 no 35, 12 no 60. Vida e mana ganham o dobro disso;
/// ataque e defesa, isso; destreza e sabedoria so' a parte percentual. So'
/// entra em atributo que a peca TEM — refinar nao cria atributo.
pub fn refino_fixo(item_level: u16) -> i32 {
    ((item_level as i32 + 4) / 5).max(1)
}

// ── Compatibilidade do antigo sistema de afixos ─────────────────────────
// Tipos e tabela permanecem porque fazem parte do formato salvo e do wire.
// `ItemInstance::fixar` remove qualquer afixo ao carregar a peça.

/// Tipo do stat afetado pelo affix.
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AffixStat {
    Hp = 0,
    Mp = 1,
    Attack = 2,
    Defense = 3,
    Dex = 4,
    Wis = 5,
    /// % crit_chance flat add (ex: +0.02 = +2%).
    CritChance = 6,
    /// % attack_speed_mult flat add (ex: +0.10 = +10% atk speed).
    AttackSpeed = 7,
    /// % move_speed_mult flat add.
    MoveSpeed = 8,
    /// HP regen flat add.
    HpRegen = 9,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct Affix {
    pub stat: AffixStat,
    pub value: i32,     // pra stats inteiros (Hp/Mp/Atk/Def/Dex/Wis)
    pub value_pct: f32, // pra % stats (Crit/AtkSpd/MoveSpd/HpRegen)
    pub tier: u8,       // 1..3 (T1=fraco, T3=forte)
    pub is_prefix: bool,
    /// Index na affix table — usado pra recuperar o nome em runtime.
    pub name_id: u16,
}

/// Tabela de affix. Cada entrada define um possível roll. ranges são
/// (min, max) por tier (T1/T2/T3). Affixes ID < 1000 são prefixes.
pub struct AffixDef {
    pub name_id: u16,
    pub display: &'static str,
    pub stat: AffixStat,
    pub is_prefix: bool,
    /// Range (min, max) por tier (T1, T2, T3).
    pub tiers: [(i32, i32); 3],
    /// Se for % stat, multiplicador pra dividir os tiers (caso seja x0.001 etc).
    pub pct_div: f32,
}

pub const AFFIX_TABLE: &[AffixDef] = &[
    // ── Prefixes (name_id 1..999) ──
    AffixDef {
        name_id: 1,
        display: "Forte",
        stat: AffixStat::Attack,
        is_prefix: true,
        tiers: [(2, 5), (5, 10), (10, 18)],
        pct_div: 0.0,
    },
    AffixDef {
        name_id: 2,
        display: "Brutal",
        stat: AffixStat::Attack,
        is_prefix: true,
        tiers: [(8, 15), (15, 25), (25, 40)],
        pct_div: 0.0,
    },
    AffixDef {
        name_id: 3,
        display: "Resistente",
        stat: AffixStat::Hp,
        is_prefix: true,
        tiers: [(10, 25), (25, 50), (50, 90)],
        pct_div: 0.0,
    },
    AffixDef {
        name_id: 4,
        display: "Fortificado",
        stat: AffixStat::Defense,
        is_prefix: true,
        tiers: [(2, 5), (5, 10), (10, 18)],
        pct_div: 0.0,
    },
    AffixDef {
        name_id: 5,
        display: "Mágico",
        stat: AffixStat::Mp,
        is_prefix: true,
        tiers: [(15, 30), (30, 60), (60, 110)],
        pct_div: 0.0,
    },
    AffixDef {
        name_id: 6,
        display: "Ágil",
        stat: AffixStat::Dex,
        is_prefix: true,
        tiers: [(2, 5), (5, 9), (9, 15)],
        pct_div: 0.0,
    },
    AffixDef {
        name_id: 7,
        display: "Sábio",
        stat: AffixStat::Wis,
        is_prefix: true,
        tiers: [(2, 5), (5, 9), (9, 15)],
        pct_div: 0.0,
    },
    AffixDef {
        name_id: 8,
        display: "Implacável",
        stat: AffixStat::AttackSpeed,
        is_prefix: true,
        tiers: [(3, 7), (7, 12), (12, 20)],
        pct_div: 100.0,
    }, // %
    AffixDef {
        name_id: 9,
        display: "Crítico",
        stat: AffixStat::CritChance,
        is_prefix: true,
        tiers: [(2, 5), (5, 8), (8, 15)],
        pct_div: 100.0,
    }, // %
    AffixDef {
        name_id: 10,
        display: "Veloz",
        stat: AffixStat::MoveSpeed,
        is_prefix: true,
        tiers: [(2, 5), (5, 8), (8, 12)],
        pct_div: 100.0,
    }, // %
    // ── Suffixes (name_id 1000..1999) ──
    AffixDef {
        name_id: 1001,
        display: "do Urso",
        stat: AffixStat::Hp,
        is_prefix: false,
        tiers: [(15, 30), (30, 60), (60, 100)],
        pct_div: 0.0,
    },
    AffixDef {
        name_id: 1002,
        display: "do Touro",
        stat: AffixStat::Attack,
        is_prefix: false,
        tiers: [(3, 7), (7, 13), (13, 22)],
        pct_div: 0.0,
    },
    AffixDef {
        name_id: 1003,
        display: "da Tartaruga",
        stat: AffixStat::Defense,
        is_prefix: false,
        tiers: [(3, 6), (6, 11), (11, 18)],
        pct_div: 0.0,
    },
    AffixDef {
        name_id: 1004,
        display: "do Lince",
        stat: AffixStat::Dex,
        is_prefix: false,
        tiers: [(3, 7), (7, 11), (11, 17)],
        pct_div: 0.0,
    },
    AffixDef {
        name_id: 1005,
        display: "do Mago",
        stat: AffixStat::Wis,
        is_prefix: false,
        tiers: [(3, 7), (7, 11), (11, 17)],
        pct_div: 0.0,
    },
    AffixDef {
        name_id: 1006,
        display: "do Vento",
        stat: AffixStat::MoveSpeed,
        is_prefix: false,
        tiers: [(2, 5), (5, 8), (8, 12)],
        pct_div: 100.0,
    },
    AffixDef {
        name_id: 1007,
        display: "da Fúria",
        stat: AffixStat::AttackSpeed,
        is_prefix: false,
        tiers: [(3, 7), (7, 12), (12, 20)],
        pct_div: 100.0,
    },
    AffixDef {
        name_id: 1008,
        display: "do Assassino",
        stat: AffixStat::CritChance,
        is_prefix: false,
        tiers: [(2, 5), (5, 8), (8, 15)],
        pct_div: 100.0,
    },
    AffixDef {
        name_id: 1009,
        display: "do Manancial",
        stat: AffixStat::Mp,
        is_prefix: false,
        tiers: [(15, 40), (40, 80), (80, 140)],
        pct_div: 0.0,
    },
    AffixDef {
        name_id: 1010,
        display: "da Regeneração",
        stat: AffixStat::HpRegen,
        is_prefix: false,
        tiers: [(1, 3), (3, 5), (5, 9)],
        pct_div: 10.0,
    }, // 0.1/0.3/0.5/0.9 hp/s
];

pub fn affix_def(name_id: u16) -> Option<&'static AffixDef> {
    AFFIX_TABLE.iter().find(|a| a.name_id == name_id)
}

impl Affix {
    pub fn roll<F: FnMut() -> f32>(rng: &mut F, is_prefix: bool, item_tier: u8) -> Option<Self> {
        let pool: Vec<&AffixDef> = AFFIX_TABLE
            .iter()
            .filter(|a| a.is_prefix == is_prefix)
            .collect();
        if pool.is_empty() {
            return None;
        }
        let pick = &pool[(rng() * pool.len() as f32) as usize];
        // Força do affix (T1 fraco → T3 forte) ponderada pelo tier do item.
        let tier_idx = match item_tier {
            1 | 2 => 0,
            3 => {
                if rng() < 0.5 {
                    0
                } else {
                    1
                }
            }
            4 => {
                if rng() < 0.6 {
                    1
                } else {
                    2
                }
            }
            _ => {
                if rng() < 0.3 {
                    1
                } else {
                    2
                }
            }
        };
        let (lo, hi) = pick.tiers[tier_idx];
        let raw = lo as f32 + rng() * (hi - lo) as f32;
        if pick.pct_div > 0.0 {
            Some(Affix {
                stat: pick.stat,
                value: 0,
                value_pct: raw / pick.pct_div,
                tier: (tier_idx + 1) as u8,
                is_prefix,
                name_id: pick.name_id,
            })
        } else {
            Some(Affix {
                stat: pick.stat,
                value: raw.round() as i32,
                value_pct: 0.0,
                tier: (tier_idx + 1) as u8,
                is_prefix,
                name_id: pick.name_id,
            })
        }
    }
}

impl ItemInstance {
    /// Roll uma instance fresh pra um item_id usando lookup hardcoded
    /// (legacy). Prefira `roll_with_template` em código novo — esse aqui
    /// só sobrevive pra testes/tools que não tem acesso ao economy cache.
    pub fn roll_for<F: FnMut() -> f32>(item_id: u16, item_level: u16, rng: F) -> Option<Self> {
        Self::roll_with_template(item_template(item_id), item_level, rng)
    }

    /// Roll uma instance usando um template já obtido (do DB cache no server).
    /// `item_level` define o nível (boss=alto, mob comum=baixo). None se
    /// o template não tem nenhum range (item não-equipável).
    pub fn roll_with_template<F: FnMut() -> f32>(
        tpl: ItemTemplate,
        item_level: u16,
        rng: F,
    ) -> Option<Self> {
        Self::roll_em(tpl, item_level, tier_from_ilvl(item_level), 1, rng)
    }

    /// A mesma criação, com GRAU (cor) e TIER dados em vez de tirados do
    /// `item_level`. É o que o Aprimorar usa. O parâmetro de RNG continua na
    /// assinatura por compatibilidade com os chamadores, mas não é usado.
    pub fn roll_em<F: FnMut() -> f32>(
        tpl: ItemTemplate,
        item_level: u16,
        grau: u8,
        tier_na_cor: u8,
        _rng: F,
    ) -> Option<Self> {
        if !tpl.has_any_range() {
            return None;
        }
        let tier = grau.clamp(1, 5);
        let mut inst = ItemInstance {
            rarity: tier, // campo `rarity` guarda a COR (grau 1–5)
            refinement: 0,
            item_level,
            level_req: if item_level > 5 {
                Some(item_level / 2)
            } else {
                None
            },
            hp_max: 0,
            mp_max: 0,
            attack_damage: 0,
            dex: 0,
            wis: 0,
            defense: 0,
            affixes: [AffixSlot::default(); MAX_AFFIXES],
            pet: None,
            afinidade: None,
            barco: None,
            sockets: sockets_for_tier(tier),
            socketed_gems: [0; 3],
            vinculado: false,
            tier: tier_na_cor.clamp(1, 4),
        };
        inst.fixar(tpl);
        Some(inst)
    }

    /// Atributos FIXOS (decisao de 19/09/2026: nada de roll aleatorio). Cada
    /// atributo e' o MEIO da faixa do template na escala da peca (cor, nivel
    /// e tier) — o mesmo ponto que o balanceamento sempre usou — e nao ha'
    /// afixo. Mesma peca, mesma cor, mesmo tier: mesmos numeros. Refino,
    /// gemas e vinculo ficam como estao. Devolve se mudou algo (pra acertar
    /// as pecas que ja' existiam, roladas no sistema antigo).
    pub fn fixar(&mut self, tpl: ItemTemplate) -> bool {
        let antes = (
            self.hp_max,
            self.mp_max,
            self.attack_damage,
            self.dex,
            self.wis,
            self.defense,
            self.affixes.iter().any(|a| !a.is_empty()),
        );
        let m = mult_do_roll(self.rarity, self.item_level, self.tier);
        self.hp_max = tpl.hp_max.roll(0.5, m);
        self.mp_max = tpl.mp_max.roll(0.5, m);
        self.attack_damage = tpl.attack_damage.roll(0.5, m);
        self.dex = tpl.dex.roll(0.5, m);
        self.wis = tpl.wis.roll(0.5, m);
        self.defense = tpl.defense.roll(0.5, m);
        self.affixes = [AffixSlot::default(); MAX_AFFIXES];
        let depois = (
            self.hp_max,
            self.mp_max,
            self.attack_damage,
            self.dex,
            self.wis,
            self.defense,
            false,
        );
        antes != depois
    }

    /// Multiplier de refinamento (1.0 + refinement × `REFINE_BOOST_PER_LEVEL`).
    pub fn refine_mult(&self) -> f32 {
        1.0 + (self.refinement as f32) * REFINE_BOOST_PER_LEVEL
    }

    /// Bônus completo dos atributos fixos × refino. Afixos legados não contam.
    pub fn effective_bonus(&self) -> crate::constants::EquipBonus {
        let m = self.refine_mult();
        let (r, u) = (self.refinement as i32, refino_fixo(self.item_level));
        // Percentual sobre o valor base + fixo por nível, só se a peça tem o atributo.
        let com = |v: i32, fixo: i32| {
            if v == 0 {
                0
            } else {
                (v as f32 * m).round() as i32 + fixo * r
            }
        };
        crate::constants::EquipBonus {
            hp_max: com(self.hp_max, 2 * u),
            mp_max: com(self.mp_max, 2 * u),
            attack_damage: com(self.attack_damage, u),
            dex: com(self.dex, 0),
            wis: com(self.wis, 0),
            defense: com(self.defense, u),
        }
    }

    /// Compatibilidade com os consumidores antigos. Afixos foram desativados,
    /// então não existe bônus percentual vindo do item.
    pub fn effective_pct_bonus(&self) -> (f32, f32, f32, f32) {
        (0.0, 0.0, 0.0, 0.0)
    }

    /// Grau (cor) do item, 1–5. Lê o campo `rarity` e clampa pra cobrir
    /// itens legados com valor fora de [1,5].
    pub fn grau(&self) -> u8 {
        self.rarity.clamp(1, 5)
    }

    /// Tier dentro da cor, 1–4 (I..IV).
    pub fn tier(&self) -> u8 {
        self.tier.clamp(1, 4)
    }
}

/// Multiplier de stats baseado em item_level. iLvl 1 = 1.0× (baseline),
/// cresce ~1% por level. Fórmula: 1.0 + (ilvl - 1) × 0.015.
/// O multiplicador que o roll aplica numa peca dessa cor, nivel e tier (a
/// mesma conta do `roll_em`).
pub fn mult_do_roll(grau: u8, item_level: u16, tier: u8) -> f32 {
    tier_stat_mult(grau.clamp(1, 5)) * ilvl_scale(item_level) * bonus_do_tier(tier)
}

/// O que uma peca de `item_id` criada no `item_level` da': (atributo,
/// valor, valor) — fixo desde 19/09/2026, entao minimo = maximo. Vazio pro
/// que nao e' equipamento. E' o "o que da'" do Craft antes de criar.
pub fn faixas_do_roll(item_id: u16, item_level: u16) -> Vec<(&'static str, i32, i32)> {
    let tpl = item_template(item_id);
    let mult = tier_stat_mult(tier_from_ilvl(item_level)) * ilvl_scale(item_level) * bonus_do_tier(1);
    [
        ("Ataque", tpl.attack_damage),
        ("Defesa", tpl.defense),
        ("Vida", tpl.hp_max),
        ("Mana", tpl.mp_max),
        ("Destreza", tpl.dex),
        ("Sabedoria", tpl.wis),
    ]
    .into_iter()
    .filter(|(_, r)| !r.is_zero())
    .map(|(n, r)| (n, r.roll(0.5, mult), r.roll(0.5, mult)))
    .collect()
}

pub fn ilvl_scale(item_level: u16) -> f32 {
    1.0 + (item_level.saturating_sub(1) as f32) * 0.015
}

// ── Fase D: Item Sets ───────────────────────────────────────────────────
// Items pertencem a um set_id. Equipar N peças do mesmo set ativa
// `set_bonus(set_id, n)`. Não-stackable: cada peça única (anel + amuleto
// contam como 2 peças se ambos do set).

// ── Fase D: Sockets/Gems ────────────────────────────────────────────────
// Gems têm um stat fixo. Inserir gema num socket adiciona o stat.
// Socket count vem do template do item (tier rarity define max sockets).

/// Quantos sockets um item tem baseado no tier: T1/T2=0, T3=1, T4=2, T5=3.
pub fn sockets_for_tier(tier: u8) -> u8 {
    match tier {
        1 | 2 => 0,
        3 => 1,
        4 => 2,
        _ => 3,
    }
}

#[cfg(test)]
mod testes_do_refino {
    use super::*;

    #[test]
    fn aprimorar_sobe_exatamente_pela_tabela_fixa() {
        use crate::constants::item_id::ARMADURA_PESADA;
        let t1 = ItemInstance::roll_em(item_template(ARMADURA_PESADA), 5, 1, 1, || 0.0).unwrap();
        let t2a = ItemInstance::roll_em(item_template(ARMADURA_PESADA), 5, 1, 2, || 0.0).unwrap();
        let t2b = ItemInstance::roll_em(item_template(ARMADURA_PESADA), 5, 1, 2, || 0.99).unwrap();
        assert_eq!((t2a.hp_max, t2a.defense), (t2b.hp_max, t2b.defense));
        assert!(t2a.defense >= t1.defense, "{} vs {}", t2a.defense, t1.defense);
        assert!(t2a.hp_max >= t1.hp_max);
        assert_eq!(t2a.attack_damage, 0, "atributo que a peca nao tem continua zero");
    }

    #[test]
    fn atributos_sao_fixos_e_pecas_antigas_se_acertam() {
        use crate::constants::item_id::KATANA;
        let a = ItemInstance::roll_for(KATANA, 40, || 0.0).unwrap();
        let b = ItemInstance::roll_for(KATANA, 40, || 0.99).unwrap();
        assert_eq!((a.attack_damage, a.dex), (b.attack_damage, b.dex), "sem sorte");
        assert!(a.affixes.iter().all(|x| x.is_empty()), "sem afixo");
        // Peca antiga (roll alto + afixo) volta pro fixo, mantendo o refino.
        let mut velha = a;
        velha.attack_damage += 5;
        velha.refinement = 6;
        velha.vinculado = true;
        velha.socketed_gems[0] = 321;
        velha.affixes[0] = AffixSlot { name_id: 1, stat: 2, tier: 1, value: 4, value_pct: 0.0, is_prefix: true };
        assert!(velha.fixar(item_template(KATANA)));
        assert_eq!((velha.attack_damage, velha.refinement), (a.attack_damage, 6));
        assert!(velha.vinculado);
        assert_eq!(velha.socketed_gems[0], 321);
        assert!(velha.affixes.iter().all(|x| x.is_empty()));
        assert!(!velha.fixar(item_template(KATANA)), "ja' fixa: nada muda");
    }

    #[test]
    fn faixas_do_roll_cercam_o_que_o_craft_rola() {
        use crate::constants::item_id::KATANA;
        let f = faixas_do_roll(KATANA, 5);
        assert_eq!(f.iter().map(|x| x.0).collect::<Vec<_>>(), vec!["Ataque", "Destreza"]);
        for r in [0.0f32, 0.3, 0.7, 0.999] {
            let i = ItemInstance::roll_for(KATANA, 5, || r).unwrap();
            assert!((f[0].1..=f[0].2).contains(&i.attack_damage), "{r}: {}", i.attack_damage);
            assert!((f[1].1..=f[1].2).contains(&i.dex));
        }
        assert!(faixas_do_roll(crate::constants::item_id::COPPER, 5).is_empty());
    }

    /// Cada nivel de refino MUDA a peca, ate' a cinza do comeco: o +1 nao
    /// pode arredondar pra nada (era o caso com so' +5%).
    #[test]
    fn cada_nivel_de_refino_aumenta_a_peca() {
        let mut armadura =
            ItemInstance::roll_for(crate::constants::item_id::ARMADURA_LEVE, 5, || 0.5).unwrap();
        armadura.hp_max = 13;
        armadura.mp_max = 0;
        armadura.attack_damage = 0;
        armadura.dex = 2;
        armadura.wis = 0;
        armadura.defense = 1;
        armadura.item_level = 5;
        for a in armadura.affixes.iter_mut() {
            *a = AffixSlot::default();
        }
        let soma = |b: &crate::constants::EquipBonus| {
            b.hp_max + b.mp_max + b.attack_damage * 10 + b.defense * 8 + (b.dex + b.wis) * 5
        };
        armadura.refinement = 0;
        let mut antes = soma(&armadura.effective_bonus());
        for nivel in 1..=crate::forja::REFINO_MAX {
            armadura.refinement = nivel;
            let b = armadura.effective_bonus();
            assert!(soma(&b) > antes, "+{nivel} nao mudou a peca");
            assert_eq!(
                (b.attack_damage, b.mp_max, b.wis),
                (0, 0, 0),
                "refino criou atributo que a peca nao tem"
            );
            antes = soma(&b);
        }
        armadura.refinement = 4;
        let b = armadura.effective_bonus();
        assert_eq!(
            (b.hp_max, b.defense, b.dex),
            (25, 5, 3),
            "a armadura cinza +4 do dono"
        );
    }
}
