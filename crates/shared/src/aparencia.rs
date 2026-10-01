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
pub const ROUPAS: [(&str, &str); 4] = [
    ("aventureiro", "Adventurer"),
    ("mercenario", "Mercenary"),
    ("andarilho", "Wanderer"),
    // A primeira PAGA. As três de cima são as "variações mais simples de
    // graça" que o dono pediu; esta existe pra a corrente da loja ser
    // demonstrável ponta a ponta — comprar, usar, destravar, vestir.
    ("capitao", "Storm Captain"),
];

/// Quantas roupas nascem destravadas. As de índice maior são da loja.
pub const ROUPAS_GRATIS: usize = 3;

/// Os chapeus, na ordem dos ids (`CHAPEU_BASE + i`), em
/// `assets/vox/personagem/chapeus/`.
pub const CHAPEUS: [(&str, &str); 8] = [
    ("capuz", "Hood"),
    ("pontudo", "Pointed Hat"),
    ("tricornio", "Tricorne"),
    ("aba", "Brimmed Hat"),
    ("boina_pena", "Feathered Beret"),
    ("lenco", "Bandana"),
    ("faixa", "Headband"),
    ("touca", "Cap"),
];

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
    (CHAPEU_BASE..CHAPEU_BASE + CHAPEUS.len() as u16)
        .contains(&item)
        .then_some(item)
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
    (0..ROUPAS.len() as u16)
        .map(|i| ROUPA_BASE + i)
        .chain((0..CHAPEUS.len() as u16).map(|i| CHAPEU_BASE + i))
        .filter(|id| !gratuita(*id))
        .collect()
}

/// O preco em TP de uma skin. `None` = nao e' skin.
///
/// As TRES primeiras roupas e os chapeus sao GRATIS — o dono pediu "tres
/// variacoes mais simples de graca, depois criamos coisas mais legais pagas".
/// Elas nascem destravadas e nao aparecem na loja; a tabela existe pra quando
/// as pagas chegarem, sem obra nova.
pub fn preco_da_skin(id: u16) -> Option<u64> {
    if !gratuita(id) && nome_da_skin(id).is_some() {
        return Some(PRECO_PADRAO_TP);
    }
    None
}

/// Quanto custa uma skin paga, em TP.
pub const PRECO_PADRAO_TP: u64 = 300;

/// A skin ja' nasce destravada? As tres roupas e os oito chapeus de hoje sim.
pub fn gratuita(id: u16) -> bool {
    (ROUPA_BASE..ROUPA_BASE + ROUPAS_GRATIS as u16).contains(&id)
        || (CHAPEU_BASE..CHAPEU_BASE + CHAPEUS.len() as u16).contains(&id)
}

/// As skins que todo personagem tem desde o primeiro login.
pub fn gratuitas() -> Vec<u16> {
    (0..ROUPAS_GRATIS as u16)
        .map(|i| ROUPA_BASE + i)
        .chain((0..CHAPEUS.len() as u16).map(|i| CHAPEU_BASE + i))
        .collect()
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
}

impl Default for Aparencia {
    fn default() -> Self {
        Self {
            rosto: 0,
            cabelo: 0,
            cor_cabelo: 0,
            pele: 0,
            roupa: 0,
        }
    }
}

impl Aparencia {
    /// Empacota pro fio. Valor fora da faixa é CORTADO, não recusado: o
    /// desenho tem que continuar acontecendo, e um rosto inválido virando o
    /// rosto 0 é melhor que um personagem invisível.
    pub fn empacota(&self) -> u32 {
        (self.rosto.min(7) as u32)
            | ((self.cabelo.min(15) as u32) << 3)
            | ((self.cor_cabelo.min(7) as u32) << 7)
            | ((self.pele.min(7) as u32) << 10)
            | ((self.roupa.min(4095) as u32) << 13)
    }

    pub fn desempacota(v: u32) -> Self {
        Self {
            rosto: (v & 0b111) as u8,
            cabelo: ((v >> 3) & 0b1111) as u8,
            cor_cabelo: ((v >> 7) & 0b111) as u8,
            pele: ((v >> 10) & 0b111) as u8,
            roupa: ((v >> 13) & 0xFFF) as u16,
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
        self
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
            Aparencia { rosto: 7, cabelo: 15, cor_cabelo: 7, pele: 7, roupa: 4095 },
            Aparencia { rosto: 2, cabelo: 1, cor_cabelo: 5, pele: 3, roupa: 480 },
            Aparencia { rosto: 0, cabelo: 0, cor_cabelo: 0, pele: 0, roupa: 1 },
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
        let a = Aparencia { rosto: 200, cabelo: 200, cor_cabelo: 200, pele: 200, roupa: 9999 };
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
        // E o campo comporta todos: 4 bits = 16, contra 3 cabelos + 1 + 8.
        let ultimo = cabelo_do_chapeu(CHAPEU_BASE + CHAPEUS.len() as u16 - 1).unwrap();
        assert!(ultimo <= 15, "o último chapéu ({ultimo}) não cabe em 4 bits");
        let a = Aparencia { cabelo: ultimo, ..Default::default() };
        assert_eq!(Aparencia::desempacota(a.empacota()).cabelo, ultimo);
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

