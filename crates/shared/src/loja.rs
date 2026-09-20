//! Loja de cash e montarias (docs/LOJA.md, docs/MONTARIAS.md).
//!
//! Fonte UNICA do catalogo: pacotes de TP (dinheiro de verdade), montarias,
//! skins e consumiveis (TP). O servidor valida tudo por aqui; o cliente so'
//! desenha.

use serde::{Deserialize, Serialize};

/// Velocidade de movimento montado, sobre a do personagem a pe'. A mesma pra
/// toda montaria: pagar mais caro compra aparencia, nao vantagem.
pub const VEL_MONTADO: f32 = 1.5;
/// Quanto demora pra montar (cancela com golpe, skill ou dano).
pub const MONTAR_S: f32 = 1.0;
/// Sem atacar nem apanhar por isto antes de poder montar.
pub const SEM_COMBATE_PRA_MONTAR_S: f32 = 3.0;
/// Velocidade de andar: montado ganha `VEL_MONTADO` e perde o sprint; a pe'
/// vale o sprint de quem esta' correndo.
pub fn velocidade_de_andar(base: f32, montado: bool, sprint_mult: f32) -> f32 {
    base * if montado { VEL_MONTADO } else { sprint_mult }
}

/// O instante da ultima luta (golpe, skill ou pancada) desmonta quem montou
/// antes dele. `desde` = quando montou (ou comecou a subir).
pub fn luta_desmonta(ultima_luta: f32, desde: f32) -> bool {
    ultima_luta > 0.0 && ultima_luta >= desde
}

/// Pode comecar a montar depois de `ultima_luta`, agora?
pub fn pode_montar_apos_luta(ultima_luta: f32, agora: f32) -> bool {
    ultima_luta <= 0.0 || agora - ultima_luta >= SEM_COMBATE_PRA_MONTAR_S
}

/// Pedido de loja com id maior que isto e' recusado (cabe no banco).
pub const PEDIDO_ID_MAX: usize = 64;

/// Pacote de TP vendido por dinheiro de verdade (centavos de real).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PacoteTp {
    pub id: u16,
    pub nome: &'static str,
    pub tp: u64,
    /// TP a mais, ja' somada em `total`.
    pub bonus: u64,
    pub centavos: u32,
}

impl PacoteTp {
    pub fn total(&self) -> u64 {
        self.tp + self.bonus
    }
}

/// Valores iniciais ⚠️ (docs/LOJA.md).
pub const PACOTES: [PacoteTp; 4] = [
    PacoteTp {
        id: 1,
        nome: "Punhado de TP",
        tp: 100,
        bonus: 0,
        centavos: 490,
    },
    PacoteTp {
        id: 2,
        nome: "Bolsa de TP",
        tp: 500,
        bonus: 50,
        centavos: 2490,
    },
    PacoteTp {
        id: 3,
        nome: "Baú de TP",
        tp: 1000,
        bonus: 200,
        centavos: 4990,
    },
    PacoteTp {
        id: 4,
        nome: "Tesouro de TP",
        tp: 2000,
        bonus: 600,
        centavos: 9990,
    },
];

/// Uma montaria: o bicho em pecas que ela usa e onde o cavaleiro senta.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Montaria {
    pub id: u16,
    pub nome: &'static str,
    /// Arquivo do bicho em pecas (`client::bicho::BICHOS`).
    pub bicho: &'static str,
    /// Escala sobre a altura em que o bicho e' carregado.
    pub escala: f32,
    /// Altura da sela, em unidades de mundo (ja' com a escala).
    pub sela: f32,
    /// Quanto a sela fica pra frente do centro do bicho.
    pub sela_frente: f32,
    pub preco_tp: u64,
    /// Skin que vem junto com a montaria.
    pub skin_padrao: u16,
    pub descricao: &'static str,
}

pub const MONTARIAS: [Montaria; 3] = [
    Montaria {
        id: 1,
        nome: "Lobo da Clareira",
        bicho: "bichos/lobo",
        escala: 0.56,
        sela: 1.26,
        sela_frente: -0.38,
        preco_tp: 500,
        skin_padrao: 101,
        descricao: "Leal e ligeiro, criado nas matas do Bosque.",
    },
    Montaria {
        id: 2,
        nome: "Tigre das Neves",
        bicho: "bichos/tigre",
        escala: 1.6,
        sela: 1.24,
        sela_frente: -0.38,
        preco_tp: 800,
        skin_padrao: 201,
        descricao: "Silencioso na neve, feroz na estrada.",
    },
    Montaria {
        id: 3,
        nome: "Urso de Carga",
        bicho: "bichos/urso",
        escala: 1.15,
        sela: 1.22,
        sela_frente: -0.40,
        preco_tp: 1200,
        skin_padrao: 301,
        descricao: "Largo e tranquilo: o passeio mais confortável da ilha.",
    },
];

/// Skin de montaria: a cor que puxa o corpo inteiro do bicho.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Skin {
    pub id: u16,
    pub montaria: u16,
    pub nome: &'static str,
    /// Cor alvo (0..255) e quanto o bicho puxa pra ela (0 = cor original).
    pub tinta: [u8; 3],
    pub forca: f32,
    /// 0 = vem com a montaria.
    pub preco_tp: u64,
}

pub const SKINS: [Skin; 9] = [
    Skin {
        id: 101,
        montaria: 1,
        nome: "Pelagem Cinza",
        tinta: [0, 0, 0],
        forca: 0.0,
        preco_tp: 0,
    },
    Skin {
        id: 102,
        montaria: 1,
        nome: "Lobo da Meia-Noite",
        tinta: [38, 42, 62],
        forca: 0.55,
        preco_tp: 300,
    },
    Skin {
        id: 103,
        montaria: 1,
        nome: "Lobo Dourado",
        tinta: [232, 178, 64],
        forca: 0.5,
        preco_tp: 450,
    },
    Skin {
        id: 201,
        montaria: 2,
        nome: "Listras Brancas",
        tinta: [0, 0, 0],
        forca: 0.0,
        preco_tp: 0,
    },
    Skin {
        id: 202,
        montaria: 2,
        nome: "Tigre de Brasa",
        tinta: [214, 84, 36],
        forca: 0.5,
        preco_tp: 400,
    },
    Skin {
        id: 203,
        montaria: 2,
        nome: "Tigre Espectral",
        tinta: [120, 196, 255],
        forca: 0.5,
        preco_tp: 600,
    },
    Skin {
        id: 301,
        montaria: 3,
        nome: "Pelo Castanho",
        tinta: [0, 0, 0],
        forca: 0.0,
        preco_tp: 0,
    },
    Skin {
        id: 302,
        montaria: 3,
        nome: "Urso Polar",
        tinta: [236, 240, 246],
        forca: 0.6,
        preco_tp: 350,
    },
    Skin {
        id: 303,
        montaria: 3,
        nome: "Urso de Obsidiana",
        tinta: [30, 24, 34],
        forca: 0.6,
        preco_tp: 550,
    },
];

/// Consumivel repetivel que entrega uma das quatro chaves de craft. A cor e'
/// rolada ao abrir; as probabilidades somam 100%.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BauCraft {
    pub id: u16,
    pub nome: &'static str,
    pub preco_tp: u64,
    pub descricao: &'static str,
    /// Cinza, verde, azul e roxa, em pontos percentuais.
    pub chances_cor: [u8; 4],
}

pub const BAUS_CRAFT: [BauCraft; 1] = [BauCraft {
    id: 1,
    nome: "Pergaminho de Invocação: Chaves",
    preco_tp: 120,
    descricao: "Abra na bolsa para invocar 1 chave aleatória de craft.",
    chances_cor: [55, 28, 12, 5],
}];

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PergaminhoMontaria {
    pub id: u16,
    pub nome: &'static str,
    pub preco_tp: u64,
    /// Lobo, Tigre e Urso, em pontos percentuais.
    pub chances: [u8; 3],
}

pub const PERGAMINHOS_MONTARIA: [PergaminhoMontaria; 1] = [PergaminhoMontaria {
    id: 1,
    nome: "Pergaminho de Invocação: Montaria",
    preco_tp: 500,
    chances: [55, 30, 15],
}];

pub fn pergaminho_montaria(id: u16) -> Option<&'static PergaminhoMontaria> {
    PERGAMINHOS_MONTARIA.iter().find(|p| p.id == id)
}

pub fn rolar_montaria(id: u16, sorte: f32) -> Option<u16> {
    let p = pergaminho_montaria(id)?;
    let alvo = (sorte.clamp(0.0, 0.999_999) * 100.0) as u16;
    let mut soma = 0u16;
    for (i, chance) in p.chances.iter().enumerate() {
        soma += *chance as u16;
        if alvo < soma {
            return Some(MONTARIAS[i].id);
        }
    }
    Some(MONTARIAS.last()?.id)
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PergaminhoTomo {
    pub id: u16,
    pub nome: &'static str,
    pub preco_tp: u64,
    /// Verde, Roxo e Lendário, em pontos percentuais.
    pub chances: [u8; 3],
}

pub const PERGAMINHOS_TOMO: [PergaminhoTomo; 1] = [PergaminhoTomo {
    id: 1,
    nome: "Pergaminho de Invocação: Tomos",
    preco_tp: 150,
    chances: [75, 20, 5],
}];

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PergaminhoPet {
    pub id: u16,
    pub nome: &'static str,
    pub preco_tp: u64,
}

/// O pergaminho de pet (docs/PETS.md). O grau e a especie sao sorteados ao
/// ABRIR, por `pets::rolar` — a compra so' entrega o pergaminho na bolsa.
pub const PERGAMINHOS_PET: [PergaminhoPet; 1] = [PergaminhoPet {
    id: 1,
    nome: "Pergaminho de Invocação: Pet",
    preco_tp: 250,
}];

pub fn pergaminho_pet(id: u16) -> Option<&'static PergaminhoPet> {
    PERGAMINHOS_PET.iter().find(|p| p.id == id)
}

pub fn pergaminho_tomo(id: u16) -> Option<&'static PergaminhoTomo> {
    PERGAMINHOS_TOMO.iter().find(|p| p.id == id)
}

/// Sorteia a habilidade (1..=12) e o grau do tomo. O estoque resultante
/// continua sendo do personagem e da habilidade, não um item da bolsa.
pub fn rolar_tomo(id: u16, r_skill: f32, r_grau: f32) -> Option<(u32, crate::skills::GrauTomo)> {
    let p = pergaminho_tomo(id)?;
    let skill_id = ((r_skill.clamp(0.0, 0.999_999) * crate::skills::SKILL_COUNT as f32) as u32) + 1;
    let alvo = (r_grau.clamp(0.0, 0.999_999) * 100.0) as u16;
    let mut soma = 0u16;
    for (i, chance) in p.chances.iter().enumerate() {
        soma += *chance as u16;
        if alvo < soma {
            return Some((skill_id, crate::skills::GrauTomo::TODOS[i]));
        }
    }
    Some((skill_id, crate::skills::GrauTomo::Lendario))
}

/// Quantos prêmios a abertura entrega. Dez pagos recebem um bônus; qualquer
/// outro lote é recusado para o cliente não inventar multiplicadores.
pub const fn premios_da_abertura(pagos: u8) -> Option<usize> {
    match pagos {
        1 => Some(1),
        10 => Some(11),
        _ => None,
    }
}

/// Moeda do jogo comprada com TP: entregue na hora no personagem (o ouro no
/// saldo; cobre e darksteel na carteira). Repetivel. Valores iniciais ⚠️
/// (docs/LOJA.md).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PacoteMoeda {
    pub id: u16,
    pub nome: &'static str,
    /// `item_id::GOLD`, `COPPER` ou `DARKSTEEL`.
    pub item_id: u16,
    pub qtd: u32,
    pub preco_tp: u64,
}

pub const MOEDAS: [PacoteMoeda; 3] = [
    PacoteMoeda {
        id: 1,
        nome: "Saco de Ouro",
        item_id: crate::item_id::GOLD,
        qtd: 10_000,
        preco_tp: 50,
    },
    PacoteMoeda {
        id: 2,
        nome: "Saco de Cobre",
        item_id: crate::item_id::COPPER,
        qtd: 20_000,
        preco_tp: 40,
    },
    PacoteMoeda {
        id: 3,
        nome: "Barras de Darksteel",
        item_id: crate::item_id::DARKSTEEL,
        qtd: 2_000,
        preco_tp: 60,
    },
];

pub fn moeda(id: u16) -> Option<&'static PacoteMoeda> {
    MOEDAS.iter().find(|m| m.id == id)
}

/// Energia comprada com TP. Nao e' item de bolsa: cai direto no saldo de
/// evolucao do personagem, o mesmo que paga tier de habilidade e ponto de
/// atributo. Repetivel. Valores iniciais ⚠️ (docs/LOJA.md).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PacoteEnergia {
    pub id: u16,
    pub nome: &'static str,
    pub qtd: u64,
    pub preco_tp: u64,
}

impl PacoteEnergia {
    /// TP por 1.000 de Energia — e' assim que o pacote maior se justifica.
    pub fn tp_por_mil(&self) -> f32 {
        if self.qtd == 0 {
            return 0.0;
        }
        self.preco_tp as f32 * 1_000.0 / self.qtd as f32
    }
}

pub const ENERGIAS: [PacoteEnergia; 3] = [
    PacoteEnergia {
        id: 1,
        nome: "Fagulha de Energia",
        qtd: 2_000,
        preco_tp: 40,
    },
    PacoteEnergia {
        id: 2,
        nome: "Cristal de Energia",
        qtd: 12_000,
        preco_tp: 200,
    },
    PacoteEnergia {
        id: 3,
        nome: "Núcleo de Energia",
        qtd: 70_000,
        preco_tp: 1_000,
    },
];

pub fn energia(id: u16) -> Option<&'static PacoteEnergia> {
    ENERGIAS.iter().find(|e| e.id == id)
}

pub fn pacote(id: u16) -> Option<&'static PacoteTp> {
    PACOTES.iter().find(|p| p.id == id)
}

pub fn montaria(id: u16) -> Option<&'static Montaria> {
    MONTARIAS.iter().find(|m| m.id == id)
}

pub fn skin(id: u16) -> Option<&'static Skin> {
    SKINS.iter().find(|s| s.id == id)
}

pub fn bau_craft(id: u16) -> Option<&'static BauCraft> {
    BAUS_CRAFT.iter().find(|b| b.id == id)
}

/// Rola o conteudo do bau com dois valores em [0, 1): um para a cor e outro
/// para a familia (Escama, Garra, Chifre ou Couro).
pub fn rolar_bau_craft(id: u16, r_cor: f32, r_tipo: f32) -> Option<(u16, u8)> {
    let b = bau_craft(id)?;
    let alvo = (r_cor.clamp(0.0, 0.999_999) * 100.0) as u16;
    let mut soma = 0u16;
    let mut cor = 4u8;
    for (i, chance) in b.chances_cor.iter().enumerate() {
        soma += *chance as u16;
        if alvo < soma {
            cor = i as u8 + 1;
            break;
        }
    }
    let tipo = ((r_tipo.clamp(0.0, 0.999_999) * 4.0) as usize).min(3);
    Some((
        crate::item_id::chave_na_cor(crate::item_id::CHAVES[tipo], cor),
        cor,
    ))
}

/// A montaria de uma skin.
pub fn montaria_da_skin(skin_id: u16) -> Option<&'static Montaria> {
    skin(skin_id).and_then(|s| montaria(s.montaria))
}

/// O que se compra na loja.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Produto {
    Tp(u16),
    Montaria(u16),
    Skin(u16),
    BauCraft(u16),
    /// Ouro, cobre ou darksteel (`MOEDAS`). Anexado no fim do enum.
    Moeda(u16),
    /// Pergaminho repetível; a montaria só é sorteada ao abrir na bolsa.
    PergaminhoMontaria(u16),
    /// Pergaminho repetível; sorteia um tomo para uma das doze habilidades.
    PergaminhoTomo(u16),
    /// Energia repetível; cai no saldo de evolução, não na bolsa.
    Energia(u16),
    /// Pergaminho repetível; sorteia espécie e grau de um pet coletor.
    PergaminhoPet(u16),
}

impl Produto {
    /// Codigo guardado no banco ("tp:2", "montaria:1", "skin:102").
    pub fn codigo(&self) -> String {
        match self {
            Produto::Tp(i) => format!("tp:{i}"),
            Produto::Montaria(i) => format!("montaria:{i}"),
            Produto::Skin(i) => format!("skin:{i}"),
            Produto::BauCraft(i) => format!("bau-craft:{i}"),
            Produto::Moeda(i) => format!("moeda:{i}"),
            Produto::PergaminhoMontaria(i) => format!("pergaminho-montaria:{i}"),
            Produto::PergaminhoTomo(i) => format!("pergaminho-tomo:{i}"),
            Produto::Energia(i) => format!("energia:{i}"),
            Produto::PergaminhoPet(i) => format!("pergaminho-pet:{i}"),
        }
    }

    pub fn de_codigo(c: &str) -> Option<Produto> {
        let (tipo, id) = c.split_once(':')?;
        let id: u16 = id.parse().ok()?;
        let p = match tipo {
            "tp" => Produto::Tp(id),
            "montaria" => Produto::Montaria(id),
            "skin" => Produto::Skin(id),
            "bau-craft" => Produto::BauCraft(id),
            "moeda" => Produto::Moeda(id),
            "pergaminho-montaria" => Produto::PergaminhoMontaria(id),
            "pergaminho-tomo" => Produto::PergaminhoTomo(id),
            "energia" => Produto::Energia(id),
            "pergaminho-pet" => Produto::PergaminhoPet(id),
            _ => return None,
        };
        p.existe().then_some(p)
    }

    pub fn existe(&self) -> bool {
        match *self {
            Produto::Tp(i) => pacote(i).is_some(),
            Produto::Montaria(i) => montaria(i).is_some(),
            Produto::Skin(i) => skin(i).is_some(),
            Produto::BauCraft(i) => bau_craft(i).is_some(),
            Produto::Moeda(i) => moeda(i).is_some(),
            Produto::PergaminhoMontaria(i) => pergaminho_montaria(i).is_some(),
            Produto::PergaminhoTomo(i) => pergaminho_tomo(i).is_some(),
            Produto::Energia(i) => energia(i).is_some(),
            Produto::PergaminhoPet(i) => pergaminho_pet(i).is_some(),
        }
    }

    pub fn nome(&self) -> String {
        match *self {
            Produto::Tp(i) => {
                pacote(i).map_or("?".into(), |p| format!("{} ({} TP)", p.nome, p.total()))
            }
            Produto::Montaria(i) => montaria(i).map_or("?".into(), |m| m.nome.to_string()),
            Produto::Skin(i) => skin(i).map_or("?".into(), |s| s.nome.to_string()),
            Produto::BauCraft(i) => bau_craft(i).map_or("?".into(), |b| b.nome.to_string()),
            Produto::Moeda(i) => moeda(i).map_or("?".into(), |m| m.nome.to_string()),
            Produto::PergaminhoMontaria(i) => {
                pergaminho_montaria(i).map_or("?".into(), |p| p.nome.to_string())
            }
            Produto::PergaminhoTomo(i) => {
                pergaminho_tomo(i).map_or("?".into(), |p| p.nome.to_string())
            }
            Produto::Energia(i) => energia(i).map_or("?".into(), |e| e.nome.to_string()),
            Produto::PergaminhoPet(i) => {
                pergaminho_pet(i).map_or("?".into(), |p| p.nome.to_string())
            }
        }
    }

    /// Preco em TP (montaria e skin). Pacote de TP nao tem preco em TP.
    pub fn preco_tp(&self) -> Option<u64> {
        match *self {
            Produto::Tp(_) => None,
            Produto::Montaria(i) => montaria(i).map(|m| m.preco_tp),
            Produto::Skin(i) => skin(i).filter(|s| s.preco_tp > 0).map(|s| s.preco_tp),
            Produto::BauCraft(i) => bau_craft(i).map(|b| b.preco_tp),
            Produto::Moeda(i) => moeda(i).map(|m| m.preco_tp),
            Produto::PergaminhoMontaria(i) => pergaminho_montaria(i).map(|p| p.preco_tp),
            Produto::PergaminhoTomo(i) => pergaminho_tomo(i).map(|p| p.preco_tp),
            Produto::Energia(i) => energia(i).map(|e| e.preco_tp),
            Produto::PergaminhoPet(i) => pergaminho_pet(i).map(|p| p.preco_tp),
        }
    }
}

/// "R$ 24,90".
pub fn preco_brl(centavos: u32) -> String {
    let reais = centavos / 100;
    let c = centavos % 100;
    let mut r = reais.to_string();
    let mut i = r.len() as i32 - 3;
    while i > 0 {
        r.insert(i as usize, '.');
        i -= 3;
    }
    format!("R$ {r},{c:02}")
}

/// O que a CONTA ja' possui.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Posses {
    pub montarias: Vec<u16>,
    pub skins: Vec<u16>,
    /// Cópias invocadas de cada montaria, guardadas para combinar/aprimorar.
    #[serde(default)]
    pub montarias_qtd: Vec<(u16, u32)>,
}

impl Posses {
    pub fn de_codigos<'a>(codigos: impl IntoIterator<Item = &'a str>) -> Posses {
        let mut p = Posses::default();
        for c in codigos {
            match Produto::de_codigo(c) {
                Some(Produto::Montaria(i)) => p.montarias.push(i),
                Some(Produto::Skin(i)) => p.skins.push(i),
                _ => {}
            }
        }
        p.montarias.sort_unstable();
        p.montarias.dedup();
        p.skins.sort_unstable();
        p.skins.dedup();
        p
    }

    pub fn tem(&self, produto: Produto) -> bool {
        match produto {
            Produto::Tp(_) => false,
            Produto::Montaria(i) => self.montarias.contains(&i),
            Produto::Skin(i) => self.skins.contains(&i),
            Produto::BauCraft(_)
            | Produto::Moeda(_)
            | Produto::PergaminhoMontaria(_)
            | Produto::PergaminhoTomo(_)
            | Produto::Energia(_)
            | Produto::PergaminhoPet(_) => false,
        }
    }

    pub fn quantidade_montaria(&self, id: u16) -> u32 {
        self.montarias_qtd
            .iter()
            .find(|(m, _)| *m == id)
            .map_or(u32::from(self.montarias.contains(&id)), |(_, q)| *q)
    }

    /// Reflete no estado em memoria uma invocacao ja' confirmada pelo servidor.
    pub fn registrar_montaria(&mut self, id: u16, quantidade: u32) {
        if !self.montarias.contains(&id) {
            self.montarias.push(id);
            self.montarias.sort_unstable();
        }
        if let Some(m) = montaria(id) {
            if !self.skins.contains(&m.skin_padrao) {
                self.skins.push(m.skin_padrao);
                self.skins.sort_unstable();
            }
        }
        if let Some(q) = self.montarias_qtd.iter_mut().find(|(m, _)| *m == id) {
            q.1 = quantidade;
        } else {
            self.montarias_qtd.push((id, quantidade));
            self.montarias_qtd.sort_unstable_by_key(|(m, _)| *m);
        }
    }

    /// A skin que vale pra montar: a escolhida, se a conta tem ela e a
    /// montaria dela; senao a padrao da primeira montaria que tiver.
    pub fn skin_para_montar(&self, escolhida: Option<u16>) -> Option<u16> {
        let valida = |id: u16| {
            skin(id).is_some_and(|s| {
                self.montarias.contains(&s.montaria)
                    && (s.preco_tp == 0 || self.skins.contains(&id))
            })
        };
        escolhida.filter(|id| valida(*id)).or_else(|| {
            self.montarias
                .iter()
                .filter_map(|m| montaria(*m))
                .map(|m| m.skin_padrao)
                .find(|id| valida(*id))
        })
    }
}

/// Por que uma compra de item nao pode acontecer (conferido antes de ir ao
/// banco; o banco confere de novo).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecusaCompra {
    ProdutoInvalido,
    JaPossui,
    PrecisaDaMontaria,
    SemSaldo,
}

impl RecusaCompra {
    pub fn texto(&self) -> &'static str {
        match self {
            RecusaCompra::ProdutoInvalido => "Produto indisponível.",
            RecusaCompra::JaPossui => "Você já possui este item.",
            RecusaCompra::PrecisaDaMontaria => "Compre a montaria antes da skin.",
            RecusaCompra::SemSaldo => "TP insuficiente.",
        }
    }
}

/// Pode comprar `produto` (item, nao pacote) com `saldo`?
pub fn pode_comprar(posses: &Posses, produto: Produto, saldo: u64) -> Result<u64, RecusaCompra> {
    // `Montaria` continua no enum porque representa posse e aparece no banco,
    // mas nao e' mais um produto direto. Ela so' nasce ao abrir o pergaminho.
    if matches!(produto, Produto::Montaria(_)) {
        return Err(RecusaCompra::ProdutoInvalido);
    }
    let preco = produto.preco_tp().ok_or(RecusaCompra::ProdutoInvalido)?;
    if posses.tem(produto) {
        return Err(RecusaCompra::JaPossui);
    }
    if let Produto::Skin(i) = produto {
        let s = skin(i).ok_or(RecusaCompra::ProdutoInvalido)?;
        if !posses.montarias.contains(&s.montaria) {
            return Err(RecusaCompra::PrecisaDaMontaria);
        }
    }
    if saldo < preco {
        return Err(RecusaCompra::SemSaldo);
    }
    Ok(preco)
}

/// Um id de pedido aceitavel: 8..=64 chars [A-Za-z0-9_-].
pub fn pedido_valido(id: &str) -> bool {
    (8..=PEDIDO_ID_MAX).contains(&id.len())
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

// ─────────────────────────────── rede ───────────────────────────────

/// Cliente -> servidor.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum PedidoLoja {
    /// Saldo, posses e historico.
    Estado,
    /// Pacote de TP por dinheiro. `pedido` e' gerado pelo cliente e faz o
    /// clique duplo / reenvio valer uma compra so'.
    ComprarTp {
        pacote: u16,
        pedido: String,
    },
    /// Montaria ou skin, com TP.
    ComprarItem {
        produto: Produto,
        pedido: String,
    },
    Montar,
    Desmontar,
}

/// Prêmio já decidido pelo servidor, usado na revelação animada do cliente.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum PremioInvocacao {
    Chave { item_id: u16, cor: u8 },
    Montaria { id: u16, quantidade: u32 },
    Tomo {
        skill_id: u32,
        grau: crate::skills::GrauTomo,
        quantidade: u16,
    },
    /// Pet coletor: o item_id ja' diz especie e grau.
    Pet { item_id: u16 },
}

/// Uma linha do historico de compras.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CompraNet {
    pub produto: String,
    /// "R$ 24,90" ou "500 TP".
    pub valor: String,
    /// pendente / creditado / entregue / recusado.
    pub status: String,
    pub quando_unix: i64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct EstadoLoja {
    /// Banco central ligado.
    pub ligada: bool,
    /// Pagamento simulado (auto-aprovado).
    pub simulado: bool,
    pub tp: u64,
    pub posses: Posses,
    pub historico: Vec<CompraNet>,
}

/// Servidor -> cliente.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum AvisoLoja {
    Estado(EstadoLoja),
    Resultado {
        ok: bool,
        texto: String,
    },
    /// Montando: o cavaleiro sobe em `segundos` (0 = cancelou).
    Montando {
        segundos: f32,
    },
    Invocacao {
        premio: PremioInvocacao,
    },
    /// Abertura em lote: dez pergaminhos pagos, onze prêmios entregues.
    Invocacoes {
        premios: Vec<PremioInvocacao>,
    },
}

#[cfg(test)]
mod tests {
    #[test]
    fn moeda_se_compra_com_tp_de_novo_e_de_novo() {
        for m in MOEDAS.iter() {
            let p = Produto::Moeda(m.id);
            assert_eq!(Produto::de_codigo(&p.codigo()), Some(p));
            assert_eq!(
                pode_comprar(&Posses::default(), p, m.preco_tp),
                Ok(m.preco_tp)
            );
            assert!(
                !Posses::de_codigos([p.codigo().as_str()]).tem(p),
                "nao vira posse"
            );
        }
        assert_eq!(
            pode_comprar(&Posses::default(), Produto::Moeda(1), 10),
            Err(RecusaCompra::SemSaldo)
        );
    }

    use super::*;
    use crate::item_id;

    #[test]
    fn catalogo_consistente() {
        let mut ids = std::collections::HashSet::new();
        for p in PACOTES {
            assert!(ids.insert(("tp", p.id)));
            assert!(p.centavos > 0 && p.tp > 0);
        }
        // Maior pacote rende mais TP por real.
        for par in PACOTES.windows(2) {
            let a = par[0].total() as f64 / par[0].centavos as f64;
            let b = par[1].total() as f64 / par[1].centavos as f64;
            assert!(
                b >= a,
                "pacote {} rende menos que o {}",
                par[1].id,
                par[0].id
            );
        }
        for m in MONTARIAS {
            assert!(ids.insert(("m", m.id)));
            let padrao = skin(m.skin_padrao).expect("skin padrao existe");
            assert_eq!(padrao.montaria, m.id);
            assert_eq!(padrao.preco_tp, 0);
            let da = SKINS.iter().filter(|s| s.montaria == m.id).count();
            assert!((3..=4).contains(&da), "{}: {da} skins", m.nome);
            assert!(m.preco_tp > 0 && m.escala > 0.0 && m.sela > 0.3);
        }
        for s in SKINS {
            assert!(ids.insert(("s", s.id)));
            assert!(montaria(s.montaria).is_some());
            assert!((0.0..=1.0).contains(&s.forca));
        }
        for b in BAUS_CRAFT {
            assert!(ids.insert(("b", b.id)));
            assert!(b.preco_tp > 0);
            assert_eq!(b.chances_cor.iter().map(|n| *n as u16).sum::<u16>(), 100);
        }
        for p in PERGAMINHOS_MONTARIA {
            assert!(ids.insert(("pm", p.id)));
            assert!(p.preco_tp > 0);
            assert_eq!(p.chances.iter().map(|n| *n as u16).sum::<u16>(), 100);
        }
        for p in PERGAMINHOS_TOMO {
            assert!(ids.insert(("pt", p.id)));
            assert!(p.preco_tp > 0);
            assert_eq!(p.chances.iter().map(|n| *n as u16).sum::<u16>(), 100);
        }
        for e in ENERGIAS {
            assert!(ids.insert(("e", e.id)));
            assert!(e.preco_tp > 0 && e.qtd > 0);
        }
        // Pacote maior tem que render mais Energia por TP, senao nao existe
        // motivo pra comprar o grande.
        for par in ENERGIAS.windows(2) {
            assert!(par[1].qtd > par[0].qtd, "{} nao cresce", par[1].nome);
            assert!(
                par[1].tp_por_mil() < par[0].tp_por_mil(),
                "{} nao rende mais por TP que {}",
                par[1].nome,
                par[0].nome
            );
        }
        assert!(VEL_MONTADO > 1.0 && VEL_MONTADO <= 1.6);
    }

    /// Energia comprada tem que pagar evolucao e atributo — e' o mesmo saldo.
    #[test]
    fn pacote_de_energia_paga_tier_e_ponto_de_atributo() {
        let menor = ENERGIAS[0];
        assert!(
            menor.qtd >= crate::skills::custo_de_evolucao(1).unwrap().energia,
            "o pacote mais barato nao cobre nem o primeiro tier"
        );
        // O maior banca uma rodada inteira de atributos de um personagem novo.
        let maior = ENERGIAS[ENERGIAS.len() - 1];
        assert!(maior.qtd >= crate::custo_energia_de_varios(0, 60));
        assert_eq!(pode_comprar(&Posses::default(), Produto::Energia(1), 39), Err(RecusaCompra::SemSaldo));
        assert_eq!(
            pode_comprar(&Posses::default(), Produto::Energia(1), 40),
            Ok(40)
        );
        assert_eq!(
            pode_comprar(&Posses::default(), Produto::Energia(99), 9_999),
            Err(RecusaCompra::ProdutoInvalido)
        );
    }

    #[test]
    fn codigo_ida_e_volta() {
        for p in [
            Produto::Tp(2),
            Produto::Montaria(3),
            Produto::Skin(102),
            Produto::BauCraft(1),
            Produto::PergaminhoMontaria(1),
            Produto::PergaminhoTomo(1),
            Produto::Energia(2),
        ] {
            assert_eq!(Produto::de_codigo(&p.codigo()), Some(p));
        }
        assert_eq!(Produto::de_codigo("energia:99"), None);
        assert_eq!(Produto::de_codigo("skin:999"), None);
        assert_eq!(Produto::de_codigo("lixo"), None);
    }

    #[test]
    fn regras_de_compra() {
        let nada = Posses::default();
        assert_eq!(
            pode_comprar(&nada, Produto::Montaria(1), 9999),
            Err(RecusaCompra::ProdutoInvalido),
            "montaria nao se compra mais diretamente"
        );
        assert_eq!(
            pode_comprar(&nada, Produto::PergaminhoMontaria(1), 499),
            Err(RecusaCompra::SemSaldo)
        );
        assert_eq!(
            pode_comprar(&nada, Produto::PergaminhoMontaria(1), 500),
            Ok(500)
        );
        assert_eq!(
            pode_comprar(&nada, Produto::Skin(102), 9999),
            Err(RecusaCompra::PrecisaDaMontaria)
        );
        assert_eq!(
            pode_comprar(&nada, Produto::Skin(101), 9999),
            Err(RecusaCompra::ProdutoInvalido),
            "skin padrao nao se vende"
        );
        assert_eq!(
            pode_comprar(&nada, Produto::Tp(1), 9999),
            Err(RecusaCompra::ProdutoInvalido)
        );
        let lobo = Posses {
            montarias: vec![1],
            skins: vec![101],
            ..Default::default()
        };
        assert_eq!(
            pode_comprar(&lobo, Produto::Montaria(1), 9999),
            Err(RecusaCompra::ProdutoInvalido)
        );
        assert_eq!(pode_comprar(&lobo, Produto::Skin(102), 300), Ok(300));
        assert_eq!(pode_comprar(&lobo, Produto::BauCraft(1), 120), Ok(120));
        assert_eq!(
            pode_comprar(&lobo, Produto::PergaminhoTomo(1), 150),
            Ok(150)
        );
        assert_eq!(
            pode_comprar(&lobo, Produto::PergaminhoMontaria(1), 500),
            Ok(500),
            "duplicata de montaria continua possivel"
        );
    }

    #[test]
    fn bau_de_craft_da_qualquer_familia_e_cor() {
        let esperadas = [
            (item_id::SCALE, 1),
            (item_id::CLAW + 1, 2),
            (item_id::HORN + 2, 3),
            (item_id::HIDE + 3, 4),
        ];
        for ((item, cor), (esperado, cor_esperada)) in [
            rolar_bau_craft(1, 0.00, 0.00).unwrap(),
            rolar_bau_craft(1, 0.56, 0.26).unwrap(),
            rolar_bau_craft(1, 0.84, 0.51).unwrap(),
            rolar_bau_craft(1, 0.99, 0.99).unwrap(),
        ]
        .into_iter()
        .zip(esperadas)
        {
            assert_eq!((item, cor), (esperado, cor_esperada));
        }
        assert_eq!(rolar_bau_craft(999, 0.0, 0.0), None);
    }

    #[test]
    fn pergaminho_de_montaria_respeita_as_faixas() {
        assert_eq!(rolar_montaria(1, 0.00), Some(1));
        assert_eq!(rolar_montaria(1, 0.549), Some(1));
        assert_eq!(rolar_montaria(1, 0.55), Some(2));
        assert_eq!(rolar_montaria(1, 0.849), Some(2));
        assert_eq!(rolar_montaria(1, 0.85), Some(3));
        assert_eq!(rolar_montaria(1, 1.0), Some(3));
        assert_eq!(rolar_montaria(999, 0.0), None);
    }

    #[test]
    fn pergaminho_de_tomo_sorteia_habilidade_e_grau() {
        use crate::skills::GrauTomo;
        assert_eq!(rolar_tomo(1, 0.00, 0.00), Some((1, GrauTomo::Verde)));
        assert_eq!(rolar_tomo(1, 0.99, 0.749), Some((12, GrauTomo::Verde)));
        assert_eq!(rolar_tomo(1, 0.40, 0.75), Some((5, GrauTomo::Roxo)));
        assert_eq!(rolar_tomo(1, 0.40, 0.95), Some((5, GrauTomo::Lendario)));
        assert_eq!(rolar_tomo(999, 0.0, 0.0), None);
        assert_eq!(premios_da_abertura(1), Some(1));
        assert_eq!(premios_da_abertura(10), Some(11));
        assert_eq!(premios_da_abertura(11), None);
    }

    #[test]
    fn skin_para_montar() {
        let nada = Posses::default();
        assert_eq!(nada.skin_para_montar(None), None);
        let p = Posses {
            montarias: vec![1, 2],
            skins: vec![101, 201, 202],
            montarias_qtd: vec![(1, 3), (2, 1)],
        };
        assert_eq!(p.quantidade_montaria(1), 3);
        assert_eq!(p.skin_para_montar(None), Some(101));
        assert_eq!(p.skin_para_montar(Some(202)), Some(202));
        let mut nova = Posses::default();
        nova.registrar_montaria(2, 2);
        assert_eq!(nova.montarias, vec![2]);
        assert!(nova.skins.contains(&201));
        assert_eq!(nova.quantidade_montaria(2), 2);
        assert_eq!(
            p.skin_para_montar(Some(203)),
            Some(101),
            "skin nao comprada cai na padrao"
        );
        assert_eq!(
            p.skin_para_montar(Some(302)),
            Some(101),
            "skin de montaria que nao tem"
        );
        // A padrao vale mesmo sem estar na lista de skins (vem com a montaria).
        let so_montaria = Posses {
            montarias: vec![3],
            skins: vec![],
            ..Default::default()
        };
        assert_eq!(so_montaria.skin_para_montar(Some(301)), Some(301));
    }

    #[test]
    fn montado_corre_mais_sem_sprint_e_luta_desmonta() {
        assert_eq!(velocidade_de_andar(3.0, false, 1.0), 3.0);
        assert_eq!(velocidade_de_andar(3.0, false, 1.65), 3.0 * 1.65);
        assert_eq!(
            velocidade_de_andar(3.0, true, 1.65),
            3.0 * VEL_MONTADO,
            "montado nao soma sprint"
        );
        // Montou no segundo 10: golpe antes nao desmonta, golpe depois sim.
        assert!(!luta_desmonta(9.0, 10.0));
        assert!(luta_desmonta(10.0, 10.0));
        assert!(luta_desmonta(12.5, 10.0));
        assert!(!luta_desmonta(0.0, 10.0), "quem nunca lutou nao desmonta");
        assert!(pode_montar_apos_luta(0.0, 1.0));
        assert!(!pode_montar_apos_luta(10.0, 12.0));
        assert!(pode_montar_apos_luta(10.0, 13.0));
    }

    #[test]
    fn textos_e_ids() {
        assert_eq!(preco_brl(490), "R$ 4,90");
        assert_eq!(preco_brl(9990), "R$ 99,90");
        assert_eq!(preco_brl(123456), "R$ 1.234,56");
        assert!(pedido_valido("a1b2c3d4-xyz"));
        assert!(!pedido_valido("curto"));
        assert!(!pedido_valido("com espaço aqui"));
    }
}
