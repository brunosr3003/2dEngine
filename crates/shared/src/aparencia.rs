//! A APARÊNCIA do personagem (docs/PERSONAGEM.md).
//!
//! # Um corpo só; o que muda é a cabeça e a roupa
//!
//! Decisão do dono, confirmada em 21/09/2026: **um rig, uma tabela de poses**.
//! Rosto, cabelo e tom de pele são a customização; a roupa é uma skin que
//! substitui as dez peças do corpo. Toda roupa é feita UMA vez e serve em
//! todo mundo — é isso que o corpo único compra.
//!
//! # No fio ela é um `u32`
//!
//! A aparência viaja na `EntityMeta`, que sai uma vez por entidade quando ela
//! entra na área de visão. Um struct de oito bytes por entidade seria 30% em
//! cima de uma meta de 26 — então ela vai empacotada:
//!
//! ```text
//!   bits 0-2    rosto        (8)
//!   bits 3-6    cabelo       (16)
//!   bits 7-9    cor_cabelo   (8)
//!   bits 10-12  pele         (8)
//!   bits 13-24  roupa        (4096 ids de skin)
//!   bits 25-31  livres
//! ```
//!
//! **Zero é o padrão** — o piratinha de sempre. Isso importa: toda entidade
//! que não é jogador manda zero, e o cliente desenha o que já desenhava.
//!
//! O chapéu NÃO entra no `u32`: ele ocupa o mesmo slot do cabelo (a junta da
//! cabeça), então quem tem chapéu gasta o campo `cabelo` com ele. Um campo só
//! para um slot só — dois campos disputando a mesma junta seria uma regra de
//! prioridade a mais para errar.

/// Quantos rostos, cabelos, cores e tons existem. Os campos do `u32` foram
/// dimensionados com folga por cima destes — crescer a tabela não mexe no fio.
pub const ROSTOS: u8 = 3;
pub const CABELOS: u8 = 3;
/// As cores de cabelo de `tools/voxrender/npcs.py` (`CABELOS`).
pub const CORES_DE_CABELO: [&str; 6] = [
    "castanho", "preto", "ruivo", "loiro", "grisalho", "branco",
];
/// Os tons de pele de `npcs.py` (`PELES`).
pub const TONS_DE_PELE: [&str; 4] = ["clara", "media", "morena", "escura"];

/// O primeiro id de skin de ROUPA. Depois de `MONTARIA_ULTIMA = 464`.
pub const ROUPA_BASE: u16 = 480;
/// O primeiro id de skin de CHAPEU.
pub const CHAPEU_BASE: u16 = 500;

/// As roupas, na ordem dos ids (`ROUPA_BASE + i`). O nome e' o arquivo em
/// `assets/vox/personagem/skins/`.
/// Append only: the position is the id, saved in every character's look.
pub const ROUPAS: [(&str, &str); 10] = [
    ("aventureiro", "Adventurer"),
    ("mercenario", "Mercenary"),
    ("andarilho", "Wanderer"),
    ("capitao", "Storm Captain"),
    ("marinheiro", "Deckhand"),
    ("explorador", "Explorer"),
    ("corsario", "Corsair"),
    ("arcanista", "Arcanist"),
    ("cavaleiro", "Knight"),
    ("nomade", "Dune Nomad"),
];

/// Price in TP of each outfit, same order as `ROUPAS`. 0 = free: unlocked
/// from the first login and offered on the creation screen.
pub const PRECO_DAS_ROUPAS: [u64; ROUPAS.len()] = [0, 0, 0, 300, 0, 0, 300, 300, 500, 300];

/// Os chapeus, na ordem dos ids (`CHAPEU_BASE + i`), em
/// `assets/vox/personagem/chapeus/`.
/// Append only, and at most 20: items 520+ are pet and mount accessories.
pub const CHAPEUS: [(&str, &str); 14] = [
    ("capuz", "Hood"),
    ("pontudo", "Pointed Hat"),
    ("tricornio", "Tricorne"),
    ("aba", "Brimmed Hat"),
    ("boina_pena", "Feathered Beret"),
    ("lenco", "Bandana"),
    ("faixa", "Headband"),
    ("touca", "Cap"),
    ("palha", "Straw Hat"),
    ("capitao", "Captain's Hat"),
    ("elmo", "Knight Helm"),
    ("chifres", "Horned Helm"),
    ("turbante", "Turban"),
    ("coroa", "Crown"),
];

/// Price in TP of each hat, same order as `CHAPEUS`. 0 = free.
pub const PRECO_DOS_CHAPEUS: [u64; CHAPEUS.len()] = [0, 0, 0, 0, 0, 0, 0, 0, 0, 200, 300, 300, 200, 500];

/// The first WEAPON skin id. Wardrobe id and bag item id are the same, as
/// with hats. 600-619.
pub const ARMA_SKIN_BASE: u16 = 600;
/// The first MOUNT skin id (wardrobe and item). 620-639.
pub const MONTARIA_SKIN_BASE: u16 = 620;

/// Weapon sets that draw a model and so can wear a skin: sword and shield,
/// katana, pistols (`skills::Conjunto` 0-2). The magic ring draws none.
pub const CONJUNTOS_COM_SKIN: usize = 3;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SkinDeArma {
    /// File suffix: `espada_<s>.vox` + `escudo_<s>.vox`, `katana_<s>` +
    /// `bainha_<s>`, `pistola_<s>` + `coldre_<s>` (`tools/voxrender/armas.py`).
    pub sufixo: &'static str,
    pub nome: &'static str,
    /// `skills::Conjunto` as u8 (0 sword and shield, 1 katana, 2 pistols).
    pub conjunto: u8,
    pub preco: u64,
}

/// Append only: the position is the id. All paid — the owner: "there will be
/// no free weapon or mount skin".
pub const SKINS_DE_ARMA: [SkinDeArma; 9] = [
    SkinDeArma { sufixo: "solar", nome: "Sunforged", conjunto: 0, preco: 400 },
    SkinDeArma { sufixo: "abissal", nome: "Abyssal", conjunto: 0, preco: 400 },
    SkinDeArma { sufixo: "real", nome: "Royal Guard", conjunto: 0, preco: 600 },
    SkinDeArma { sufixo: "sakura", nome: "Sakura", conjunto: 1, preco: 400 },
    SkinDeArma { sufixo: "tempestade", nome: "Stormcaller", conjunto: 1, preco: 400 },
    SkinDeArma { sufixo: "lua", nome: "Crimson Moon", conjunto: 1, preco: 600 },
    SkinDeArma { sufixo: "dourada", nome: "Gilded Pistols", conjunto: 2, preco: 400 },
    SkinDeArma { sufixo: "coral", nome: "Reef Pistols", conjunto: 2, preco: 400 },
    SkinDeArma { sufixo: "relampago", nome: "Thunder Pistols", conjunto: 2, preco: 600 },
];

/// Mount skins: a coat for WHATEVER mount is ridden, swapped by palette index
/// per species (`tools/voxrender/bichos.py: PELES_DE_MONTARIA`), file
/// `bichos/<species>_<sufixo>.vox`. (sufixo, name, price). Append only.
pub const SKINS_DE_MONTARIA: [(&str, &str, u64); 5] = [
    ("dourada", "Gilded Coat", 500),
    ("obsidiana", "Obsidian Coat", 500),
    ("gelida", "Frostborn Coat", 500),
    ("brasa", "Emberhide Coat", 500),
    ("espectral", "Spectral Coat", 700),
];

/// The weapon skin with this id.
pub fn skin_de_arma(id: u16) -> Option<&'static SkinDeArma> {
    SKINS_DE_ARMA.get(id.checked_sub(ARMA_SKIN_BASE)? as usize)
}

/// The mount skin with this id: (sufixo, name, price).
pub fn skin_de_montaria(id: u16) -> Option<&'static (&'static str, &'static str, u64)> {
    if id >= MONTARIA_SKIN_BASE + SKINS_DE_MONTARIA.len() as u16 {
        return None;
    }
    SKINS_DE_MONTARIA.get(id.checked_sub(MONTARIA_SKIN_BASE)? as usize)
}

/// The file suffix of the weapon skin worn on set `conjunto`, if any.
pub fn sufixo_da_arma(a: &Aparencia, conjunto: u8) -> Option<&'static str> {
    let i = *a.armas.get(conjunto as usize)?;
    let s = SKINS_DE_ARMA.get(i.checked_sub(1)? as usize)?;
    (s.conjunto == conjunto).then_some(s.sufixo)
}

/// The file suffix of the mount skin worn, if any.
pub fn sufixo_da_montaria(a: &Aparencia) -> Option<&'static str> {
    SKINS_DE_MONTARIA.get(a.montaria.checked_sub(1)? as usize).map(|m| m.0)
}

/// The BAG ITEM of an outfit skin starts here, apart from the wardrobe id.
///
/// Outfits were ids 480-483 both in the wardrobe and as bag items — and the
/// Porão keys took 480 + dungeon id on 29/09/2026 (`porao::CHAVE_BASE`).
/// Item 483 stopped being Storm Captain and became the Frozen Hull Key: a
/// Storm Captain bought on 27/09 sat in the bag as a key, and the owner
/// never got the outfit. The wardrobe keeps 480+ (characters' saved looks
/// stay valid); only what goes in the bag moves to 540+, which is free.
/// Hats (500-507) never collided and keep their ids as items.
pub const ITEM_ROUPA_BASE: u16 = 540;

/// The bag item that unlocks wardrobe skin `skin`.
pub fn item_da_skin(skin: u16) -> u16 {
    if (ROUPA_BASE..ROUPA_BASE + ROUPAS.len() as u16).contains(&skin) {
        ITEM_ROUPA_BASE + (skin - ROUPA_BASE)
    } else {
        skin
    }
}

/// The wardrobe skin a bag item unlocks. `None` = not a skin item — notably
/// the Porão keys 481-485, which share numbers with the wardrobe outfits.
pub fn skin_do_item(item: u16) -> Option<u16> {
    if (ITEM_ROUPA_BASE..ITEM_ROUPA_BASE + ROUPAS.len() as u16).contains(&item) {
        return Some(ROUPA_BASE + (item - ITEM_ROUPA_BASE));
    }
    let chapeu = (CHAPEU_BASE..CHAPEU_BASE + CHAPEUS.len() as u16).contains(&item);
    (chapeu || skin_de_arma(item).is_some() || skin_de_montaria(item).is_some()).then_some(item)
}

/// O arquivo da skin de roupa deste id. `None` = id que nao e' roupa.
pub fn arquivo_da_roupa(id: u16) -> Option<&'static str> {
    if id < ROUPA_BASE {
        return None;
    }
    ROUPAS.get((id - ROUPA_BASE) as usize).map(|r| r.0)
}

/// O nome bonito de uma skin, de roupa ou de chapeu.
pub fn nome_da_skin(id: u16) -> Option<&'static str> {
    if let Some(a) = skin_de_arma(id) {
        return Some(a.nome);
    }
    if let Some(m) = skin_de_montaria(id) {
        return Some(m.1);
    }
    if id >= CHAPEU_BASE {
        return CHAPEUS.get((id - CHAPEU_BASE) as usize).map(|c| c.1);
    }
    if id >= ROUPA_BASE {
        return ROUPAS.get((id - ROUPA_BASE) as usize).map(|r| r.1);
    }
    None
}

/// Uma skin de CHAPEU ocupa o slot do cabelo, entao ela entra no campo
/// `cabelo` da aparencia — depois dos cabelos de verdade.
pub fn cabelo_do_chapeu(id: u16) -> Option<u8> {
    if id < CHAPEU_BASE {
        return None;
    }
    let i = (id - CHAPEU_BASE) as usize;
    (i < CHAPEUS.len()).then(|| CABELOS + 1 + i as u8)
}

/// As skins que a LOJA vende: as que existem e nao sao gratis.
pub fn a_venda() -> Vec<u16> {
    todas().filter(|id| !gratuita(*id)).collect()
}

/// The table price of a skin, 0 included. `None` = not a skin.
fn preco_da_tabela(id: u16) -> Option<u64> {
    if let Some(a) = skin_de_arma(id) {
        return Some(a.preco);
    }
    if let Some(m) = skin_de_montaria(id) {
        return Some(m.2);
    }
    if id >= CHAPEU_BASE {
        return PRECO_DOS_CHAPEUS.get((id - CHAPEU_BASE) as usize).copied();
    }
    if id >= ROUPA_BASE {
        return PRECO_DAS_ROUPAS.get((id - ROUPA_BASE) as usize).copied();
    }
    None
}

/// O preco em TP de uma skin a venda. `None` = gratis ou nao e' skin.
pub fn preco_da_skin(id: u16) -> Option<u64> {
    preco_da_tabela(id).filter(|p| *p > 0)
}

/// A skin ja' nasce destravada? (price 0 in the table)
pub fn gratuita(id: u16) -> bool {
    preco_da_tabela(id) == Some(0)
}

/// As skins que todo personagem tem desde o primeiro login.
pub fn gratuitas() -> Vec<u16> {
    todas().filter(|id| gratuita(*id)).collect()
}

/// Every wardrobe skin id, outfits then hats.
fn todas() -> impl Iterator<Item = u16> {
    (0..ROUPAS.len() as u16)
        .map(|i| ROUPA_BASE + i)
        .chain((0..CHAPEUS.len() as u16).map(|i| CHAPEU_BASE + i))
        .chain((0..SKINS_DE_ARMA.len() as u16).map(|i| ARMA_SKIN_BASE + i))
        .chain((0..SKINS_DE_MONTARIA.len() as u16).map(|i| MONTARIA_SKIN_BASE + i))
}

/// The free outfits, in id order — what the creation screen offers.
pub fn roupas_gratis() -> Vec<u16> {
    (0..ROUPAS.len() as u16).map(|i| ROUPA_BASE + i).filter(|id| gratuita(*id)).collect()
}

/// The free hats, in id order.
pub fn chapeus_gratis() -> Vec<u16> {
    (0..CHAPEUS.len() as u16).map(|i| CHAPEU_BASE + i).filter(|id| gratuita(*id)).collect()
}

/// O caminho inverso: que chapeu este indice de `cabelo` representa.
pub fn chapeu_do_cabelo(cabelo: u8) -> Option<u16> {
    let primeiro = CABELOS + 1;
    if cabelo < primeiro {
        return None;
    }
    let i = (cabelo - primeiro) as usize;
    (i < CHAPEUS.len()).then(|| CHAPEU_BASE + i as u16)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct Aparencia {
    pub rosto: u8,
    /// O cabelo OU o chapéu: os dois ocupam a junta da cabeça.
    pub cabelo: u8,
    pub cor_cabelo: u8,
    pub pele: u8,
    /// Id da skin de roupa. 0 = o corpo padrão.
    pub roupa: u16,
    /// The weapon skin per weapon set (sword and shield, katana, pistols):
    /// index + 1 into `SKINS_DE_ARMA`, 0 = the default model. One per set,
    /// so switching weapons keeps each choice.
    pub armas: [u8; CONJUNTOS_COM_SKIN],
    /// The mount skin: index + 1 into `SKINS_DE_MONTARIA`, 0 = none.
    pub montaria: u8,
}

impl Default for Aparencia {
    fn default() -> Self {
        Self {
            rosto: 0,
            cabelo: 0,
            cor_cabelo: 0,
            pele: 0,
            roupa: 0,
            armas: [0; CONJUNTOS_COM_SKIN],
            montaria: 0,
        }
    }
}

impl Aparencia {
    /// Empacota pro fio. Valor fora da faixa é CORTADO, não recusado: o
    /// desenho tem que continuar acontecendo, e um rosto inválido virando o
    /// rosto 0 é melhor que um personagem invisível.
    /// `cabelo` had 4 bits (3-6), which held 3 hairs + "no hair" + 12 hats.
    /// Its high bits ride on the spare bits 25-26, so a value packed before
    /// the change (high bits zero) still reads the same.
    pub fn empacota(&self) -> u32 {
        let cabelo = self.cabelo.min(63) as u32;
        (self.rosto.min(7) as u32)
            | ((cabelo & 0b1111) << 3)
            | ((self.cor_cabelo.min(7) as u32) << 7)
            | ((self.pele.min(7) as u32) << 10)
            | ((self.roupa.min(4095) as u32) << 13)
            | ((cabelo >> 4) << 25)
    }

    pub fn desempacota(v: u32) -> Self {
        Self {
            rosto: (v & 0b111) as u8,
            cabelo: (((v >> 3) & 0b1111) | (((v >> 25) & 0b11) << 4)) as u8,
            cor_cabelo: ((v >> 7) & 0b111) as u8,
            pele: ((v >> 10) & 0b111) as u8,
            roupa: ((v >> 13) & 0xFFF) as u16,
            ..Default::default()
        }
    }

    /// Conserta o que estiver fora das tabelas. Vale no servidor (o cliente
    /// manda o que quiser) e na carga do banco (a tabela pode ter encolhido).
    pub fn saneada(mut self) -> Self {
        if self.rosto >= ROSTOS {
            self.rosto = 0;
        }
        // `CABELOS` é o índice do "no hair"; acima disso são os chapéus.
        // Quem valida se o chapéu foi DESTRAVADO é o guarda-roupa do
        // servidor — aqui só se corta o que não existe em tabela nenhuma.
        if self.cabelo > CABELOS + CHAPEUS.len() as u8 {
            self.cabelo = 0;
        }
        if self.cor_cabelo as usize >= CORES_DE_CABELO.len() {
            self.cor_cabelo = 0;
        }
        if self.pele as usize >= TONS_DE_PELE.len() {
            self.pele = 0;
        }
        // A weapon skin only on the set it was made for.
        for c in 0..CONJUNTOS_COM_SKIN {
            if self.armas[c] != 0 && sufixo_da_arma(&self, c as u8).is_none() {
                self.armas[c] = 0;
            }
        }
        if self.montaria as usize > SKINS_DE_MONTARIA.len() {
            self.montaria = 0;
        }
        self
    }

    /// The wardrobe ids of the weapon and mount skins worn — what the
    /// server checks against the unlocked ones.
    pub fn skins_extras(&self) -> Vec<u16> {
        let armas = self.armas.iter().filter(|i| **i != 0).map(|i| ARMA_SKIN_BASE + *i as u16 - 1);
        let montaria = (self.montaria != 0).then(|| MONTARIA_SKIN_BASE + self.montaria as u16 - 1);
        armas.chain(montaria).collect()
    }

    /// Weapon and mount skins for the wire (`EntityMeta::skins`): one byte per
    /// weapon set, then the mount. Apart from `empacota`, whose 32 bits are
    /// nearly full and which keys the body mesh cache.
    pub fn empacota_skins(&self) -> u32 {
        self.armas[0] as u32
            | (self.armas[1] as u32) << 8
            | (self.armas[2] as u32) << 16
            | (self.montaria as u32) << 24
    }

    /// `self` with the skins of `empacota_skins` put back.
    pub fn com_skins(mut self, v: u32) -> Self {
        self.armas = [v as u8, (v >> 8) as u8, (v >> 16) as u8];
        self.montaria = (v >> 24) as u8;
        self
    }

    /// `saneada`, and only FREE skins: a new character owns nothing else yet,
    /// so a paid outfit or hat sent at creation would be worn without buying.
    pub fn so_gratis(self) -> Self {
        let mut a = self.saneada();
        if a.roupa != 0 && !gratuita(a.roupa) {
            a.roupa = 0;
        }
        if chapeu_do_cabelo(a.cabelo).is_some_and(|c| !gratuita(c)) {
            a.cabelo = 0;
        }
        a.armas = [0; CONJUNTOS_COM_SKIN];
        a.montaria = 0;
        a
    }
}

/// O que o personagem guarda entre sessões: a escolha e o que ele PODE
/// escolher. Vai inteiro na coluna `visual_json`, que já existe.
#[derive(Debug, Clone, Default, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct GuardaRoupa {
    pub aparencia: Aparencia,
    /// Ids de skin já destravados (o item foi consumido). Vazio = só o padrão.
    pub desbloqueadas: Vec<u16>,
}

impl GuardaRoupa {
    pub fn tem(&self, id: u16) -> bool {
        id == 0 || self.desbloqueadas.contains(&id)
    }

    /// Destrava uma skin. `false` = já tinha (o item não deve ser consumido).
    pub fn destrava(&mut self, id: u16) -> bool {
        if id == 0 || self.desbloqueadas.contains(&id) {
            return false;
        }
        self.desbloqueadas.push(id);
        true
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    /// O ZERO é o padrão, e isso não é detalhe: toda entidade que não é
    /// jogador manda zero, e o cliente tem que desenhar o que já desenhava.
    #[test]
    fn zero_e_o_piratinha_de_sempre() {
        assert_eq!(Aparencia::desempacota(0), Aparencia::default());
        assert_eq!(Aparencia::default().empacota(), 0);
    }

    /// Ida e volta, em todo campo, até o máximo que o campo comporta.
    #[test]
    fn empacota_e_desempacota_sem_perder_nada() {
        for a in [
            Aparencia { rosto: 7, cabelo: 15, cor_cabelo: 7, pele: 7, roupa: 4095, ..Default::default() },
            Aparencia { rosto: 7, cabelo: 63, cor_cabelo: 7, pele: 7, roupa: 4095, ..Default::default() },
            Aparencia { rosto: 1, cabelo: CABELOS + CHAPEUS.len() as u8, cor_cabelo: 0, pele: 0, roupa: 0, ..Default::default() },
            Aparencia { rosto: 2, cabelo: 1, cor_cabelo: 5, pele: 3, roupa: 480, ..Default::default() },
            Aparencia { rosto: 0, cabelo: 0, cor_cabelo: 0, pele: 0, roupa: 1, ..Default::default() },
        ] {
            assert_eq!(Aparencia::desempacota(a.empacota()), a, "{a:?}");
        }
    }

    /// Os campos não se atropelam. Um bit vazando de um campo pro vizinho é o
    /// tipo de erro que aparece como "meu cabelo muda quando troco de roupa".
    #[test]
    fn um_campo_nunca_mexe_no_outro() {
        let base = Aparencia::default();
        let so_roupa = Aparencia { roupa: 4095, ..base };
        assert_eq!(Aparencia::desempacota(so_roupa.empacota()).rosto, 0);
        assert_eq!(Aparencia::desempacota(so_roupa.empacota()).cabelo, 0);
        let so_rosto = Aparencia { rosto: 7, ..base };
        assert_eq!(Aparencia::desempacota(so_rosto.empacota()).roupa, 0);
        assert_eq!(Aparencia::desempacota(so_rosto.empacota()).cor_cabelo, 0);
    }

    /// Valor fora da faixa é cortado, nunca recusado: melhor o rosto 0 que um
    /// personagem que não desenha.
    #[test]
    fn valor_absurdo_vira_o_padrao_em_vez_de_sumir() {
        let a = Aparencia { rosto: 200, cabelo: 200, cor_cabelo: 200, pele: 200, roupa: 9999, ..Default::default() };
        let s = a.saneada();
        assert!(s.rosto < ROSTOS && (s.cor_cabelo as usize) < CORES_DE_CABELO.len());
        assert!((s.pele as usize) < TONS_DE_PELE.len());
        // E o empacotamento nunca estoura pro campo vizinho.
        assert_eq!(Aparencia::desempacota(a.empacota()).rosto, 7);
    }

    /// Destravar é idempotente — senão comprar duas vezes consumiria dois
    /// itens pelo mesmo direito.
    #[test]
    fn destravar_duas_vezes_nao_consome_duas() {
        let mut g = GuardaRoupa::default();
        assert!(g.destrava(ROUPA_BASE));
        assert!(!g.destrava(ROUPA_BASE), "a segunda vez não consome");
        assert!(g.tem(ROUPA_BASE));
        assert!(g.tem(0), "o padrão está sempre destravado");
        assert!(!g.tem(ROUPA_BASE + 1));
    }
}

#[cfg(test)]
mod testes_das_skins {
    use super::*;

    /// Chapéu e cabelo dividem o slot, e a conversão vai e volta.
    ///
    /// É a parte mais fácil de errar do desenho todo: o chapéu não tem campo
    /// próprio no `u32` — ele ocupa o campo `cabelo`, depois dos cabelos de
    /// verdade. Um erro de um índice aqui põe chapéu onde devia ter cabelo.
    #[test]
    fn chapeu_e_cabelo_dividem_o_slot_sem_se_atropelar() {
        // Os cabelos de verdade, e o "no hair", não são chapéu.
        for c in 0..=CABELOS {
            assert_eq!(chapeu_do_cabelo(c), None, "cabelo {c} virou chapéu");
        }
        // Todo chapéu vai e volta.
        for i in 0..CHAPEUS.len() as u16 {
            let id = CHAPEU_BASE + i;
            let c = cabelo_do_chapeu(id).unwrap_or_else(|| panic!("{id} sem slot"));
            assert!(c > CABELOS, "o chapéu {id} caiu na faixa dos cabelos");
            assert_eq!(chapeu_do_cabelo(c), Some(id));
        }
        // And the packed field holds them all: 6 bits (4 + the spare 25-26).
        let ultimo = cabelo_do_chapeu(CHAPEU_BASE + CHAPEUS.len() as u16 - 1).unwrap();
        assert!(ultimo <= 63, "the last hat ({ultimo}) does not fit in 6 bits");
        let a = Aparencia { cabelo: ultimo, ..Default::default() };
        assert_eq!(Aparencia::desempacota(a.empacota()).cabelo, ultimo);
    }

    /// Prices follow the table: 0 is free (unlocked, not sold), the rest is
    /// sold at its own price. The creation screen offers only the free ones.
    #[test]
    fn preco_por_item_separa_gratis_de_pago() {
        assert!(gratuita(ROUPA_BASE));
        assert_eq!(preco_da_skin(ROUPA_BASE), None);
        assert_eq!(preco_da_skin(ROUPA_BASE + 3), Some(300), "Storm Captain");
        assert!(gratuita(ROUPA_BASE + 4), "a free outfit after a paid one");
        assert!(roupas_gratis().contains(&(ROUPA_BASE + 4)));
        assert!(!roupas_gratis().contains(&(ROUPA_BASE + 3)));
        assert_eq!(
            a_venda().len() + gratuitas().len(),
            ROUPAS.len() + CHAPEUS.len() + SKINS_DE_ARMA.len() + SKINS_DE_MONTARIA.len()
        );
        assert!(CHAPEU_BASE + CHAPEUS.len() as u16 <= 520, "520+ are accessories");
        assert!(ITEM_ROUPA_BASE + ROUPAS.len() as u16 <= 600);
        let pago = chapeu_do_cabelo(CABELOS + 1 + 13).unwrap();
        assert!(!gratuita(pago), "Crown");
        let criado = Aparencia { roupa: ROUPA_BASE + 8, cabelo: CABELOS + 1 + 13, ..Default::default() }
            .so_gratis();
        assert_eq!((criado.roupa, criado.cabelo), (0, 0), "no paid skin at creation");
        let livre = Aparencia { roupa: ROUPA_BASE + 4, cabelo: CABELOS + 1 + 8, ..Default::default() };
        assert_eq!(livre.so_gratis(), livre);
    }

    /// Weapon and mount skins: all paid, each an item of its own that never
    /// lands on a hat, an accessory (520+) or an outfit item (540+), and only
    /// worn on the set it was made for.
    #[test]
    fn skins_de_arma_e_montaria() {
        for i in 0..SKINS_DE_ARMA.len() as u16 {
            let id = ARMA_SKIN_BASE + i;
            assert!(!gratuita(id) && preco_da_skin(id).is_some(), "weapon skin {id} free");
            assert_eq!(skin_do_item(item_da_skin(id)), Some(id));
            assert!((SKINS_DE_ARMA[i as usize].conjunto as usize) < CONJUNTOS_COM_SKIN);
        }
        for i in 0..SKINS_DE_MONTARIA.len() as u16 {
            let id = MONTARIA_SKIN_BASE + i;
            assert!(!gratuita(id) && preco_da_skin(id).is_some(), "mount skin {id} free");
            assert_eq!(skin_do_item(id), Some(id));
        }
        assert!(ARMA_SKIN_BASE + SKINS_DE_ARMA.len() as u16 <= MONTARIA_SKIN_BASE);
        assert!(ITEM_ROUPA_BASE + ROUPAS.len() as u16 <= ARMA_SKIN_BASE);
        // Sakura (katana) on the sword slot is cut; on the katana slot it stays.
        let errada = Aparencia { armas: [4, 4, 0], montaria: 2, ..Default::default() }.saneada();
        assert_eq!(errada.armas, [0, 4, 0]);
        assert_eq!(sufixo_da_arma(&errada, 1), Some("sakura"));
        assert_eq!(sufixo_da_montaria(&errada), Some("obsidiana"));
        assert_eq!(errada.skins_extras(), vec![ARMA_SKIN_BASE + 3, MONTARIA_SKIN_BASE + 1]);
        let ida = Aparencia::default().com_skins(errada.empacota_skins());
        assert_eq!((ida.armas, ida.montaria), (errada.armas, errada.montaria));
        assert_eq!(Aparencia { montaria: 99, ..Default::default() }.saneada().montaria, 0);
    }

    /// Toda roupa da tabela tem arquivo, e nenhum id fora dela inventa um.
    #[test]
    fn toda_roupa_tem_arquivo_e_nome() {
        for i in 0..ROUPAS.len() as u16 {
            let id = ROUPA_BASE + i;
            assert_eq!(arquivo_da_roupa(id), Some(ROUPAS[i as usize].0));
            assert!(nome_da_skin(id).is_some());
        }
        assert_eq!(arquivo_da_roupa(0), None, "o padrão não tem arquivo de skin");
        assert_eq!(arquivo_da_roupa(ROUPA_BASE + 99), None);
        // Chapéu não é roupa, mesmo tendo nome.
        assert_eq!(arquivo_da_roupa(CHAPEU_BASE), None);
        assert!(nome_da_skin(CHAPEU_BASE).is_some());
    }

    /// As faixas de id não se encostam: se um chapéu caísse na faixa da
    /// roupa, vestir um chapéu trocaria a roupa.
    #[test]
    fn as_faixas_de_id_nao_se_encostam() {
        assert!(ROUPA_BASE + ROUPAS.len() as u16 <= CHAPEU_BASE);
        assert!(CHAPEU_BASE > ROUPA_BASE);
    }
}

#[cfg(test)]
mod testes_do_item_da_skin {
    use super::*;

    /// No skin item shares a number with a Porão key, and every skin item
    /// goes back to its own wardrobe skin.
    #[test]
    fn item_de_skin_nunca_e_chave_de_porao() {
        let chaves: Vec<u16> = crate::dungeon::CONTEUDOS
            .iter()
            .filter_map(crate::porao::chave_de)
            .collect();
        for skin in a_venda() {
            let item = item_da_skin(skin);
            assert!(!chaves.contains(&item), "skin {skin} sells as item {item}, a Porão key");
            assert_eq!(skin_do_item(item), Some(skin));
        }
        for chave in chaves {
            assert_eq!(skin_do_item(chave), None, "key {chave} would unlock a skin");
        }
        assert_eq!(item_da_skin(ROUPA_BASE + 3), ITEM_ROUPA_BASE + 3, "Storm Captain");
    }
}

