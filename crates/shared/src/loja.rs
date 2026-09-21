//! Loja de cash e montarias (docs/LOJA.md, docs/MONTARIAS.md).
//!
//! Fonte UNICA do catalogo: pacotes de TP (dinheiro de verdade), montarias,
//! skins e consumiveis (TP). O servidor valida tudo por aqui; o cliente so'
//! desenha.

use serde::{Deserialize, Serialize};

/// Velocidade de movimento montado, sobre a do personagem a pe'. E' o piso: a
/// montaria CINZA vale isto, e a cor sobe daí (`montarias::velocidade`).
/// Antes era a mesma pra toda montaria.
pub const VEL_MONTADO: f32 = 1.5;
/// Quanto demora pra montar (cancela com golpe, skill ou dano).
pub const MONTAR_S: f32 = 1.0;
/// Sem atacar nem apanhar por isto antes de poder montar.
pub const SEM_COMBATE_PRA_MONTAR_S: f32 = 3.0;
/// Velocidade de andar. Montado vale o multiplicador da MONTARIA — que sai da
/// cor dela (`montarias::velocidade`) — e perde o sprint; a pe' vale o sprint
/// de quem esta' correndo. `montado` = `None` quando esta' a pe'.
pub fn velocidade_de_andar(base: f32, montado: Option<f32>, sprint_mult: f32) -> f32 {
    base * montado.unwrap_or(sprint_mult)
}

/// O multiplicador da montaria equipada, pra quem esta' montado. `None` = a
/// pe'. Montado sem montaria equipada nao existe, mas se acontecer o piso
/// vale, em vez de virar velocidade de tartaruga.
pub fn mult_de_montaria(montado: bool, equipada: Option<u16>) -> Option<f32> {
    if !montado {
        return None;
    }
    Some(
        equipada
            .and_then(crate::montarias::de_item)
            .map_or(VEL_MONTADO, |(_, grau)| crate::montarias::velocidade(grau)),
    )
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
}

pub const PERGAMINHOS_MONTARIA: [PergaminhoMontaria; 1] = [PergaminhoMontaria {
    id: 1,
    nome: "Pergaminho de Invocação: Montaria",
    preco_tp: 500,
}];

pub fn pergaminho_montaria(id: u16) -> Option<&'static PergaminhoMontaria> {
    PERGAMINHOS_MONTARIA.iter().find(|p| p.id == id)
}

/// Sorteia o ITEM de montaria (especie + cor). A montaria virou item de
/// bolsa como o pet, entao o pergaminho entrega um id, nao uma posse.
pub fn rolar_montaria(id: u16, r_especie: f32, r_grau: f32) -> Option<u16> {
    pergaminho_montaria(id)?;
    let (base, grau) = crate::montarias::rolar(r_especie, r_grau);
    Some(crate::item_id::montaria_no_grau(base, grau))
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

/// Consumivel de PET vendido por TP (docs/PETS.md): Ração, Removedor e as
/// cinco skills. Todos entregues na bolsa e NEGOCIAVEIS — quem farma compra
/// no mercado por gold.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ItemDePet {
    pub id: u16,
    pub item_id: u16,
    pub nome: &'static str,
    pub descricao: &'static str,
    pub preco_tp: u64,
}

/// Racao, removedor e pedra. As skills entram depois, do catalogo de `pets`.
const FIXOS: [ItemDePet; 3] = [
    ItemDePet {
        id: 1,
        item_id: crate::item_id::RACAO_DE_PET,
        nome: "Ração de Pet",
        descricao: "Alimenta o pet por 2 h. Com fome ele não ganha experiência.",
        preco_tp: 30,
    },
    ItemDePet {
        id: 2,
        item_id: crate::item_id::REMOVEDOR_DE_SKILL_PET,
        nome: "Removedor de Skill",
        descricao: "Tira todas as skills do pet e devolve os slots.",
        preco_tp: 200,
    },
    ItemDePet {
        id: 3,
        item_id: crate::item_id::PEDRA_DE_AFINIDADE,
        nome: "Pedra de Afinidade",
        descricao: "Sorteia de novo os atributos do pet ou da montaria equipada.",
        preco_tp: 250,
    },
];

/// A lista inteira: os dois fixos e uma linha por skill, com id seguindo.
pub fn itens_de_pet() -> Vec<ItemDePet> {
    let mut v = FIXOS.to_vec();
    for (i, sk) in crate::pets::todas_as_skills().iter().enumerate() {
        v.push(ItemDePet {
            id: FIXOS.len() as u16 + 1 + i as u16,
            item_id: sk.item_id,
            nome: sk.nome,
            descricao: sk.descricao,
            preco_tp: sk.preco_tp,
        });
    }
    v
}

pub fn item_de_pet(id: u16) -> Option<ItemDePet> {
    itens_de_pet().into_iter().find(|x| x.id == id)
}

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

/// O que se compra na loja.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Produto {
    Tp(u16),
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
    /// Ração, removedor ou skill de pet (`itens_de_pet`).
    ItemDePet(u16),
}

impl Produto {
    /// Codigo guardado no banco ("tp:2", "montaria:1", "skin:102").
    pub fn codigo(&self) -> String {
        match self {
            Produto::Tp(i) => format!("tp:{i}"),
            Produto::BauCraft(i) => format!("bau-craft:{i}"),
            Produto::Moeda(i) => format!("moeda:{i}"),
            Produto::PergaminhoMontaria(i) => format!("pergaminho-montaria:{i}"),
            Produto::PergaminhoTomo(i) => format!("pergaminho-tomo:{i}"),
            Produto::Energia(i) => format!("energia:{i}"),
            Produto::PergaminhoPet(i) => format!("pergaminho-pet:{i}"),
            Produto::ItemDePet(i) => format!("item-pet:{i}"),
        }
    }

    pub fn de_codigo(c: &str) -> Option<Produto> {
        let (tipo, id) = c.split_once(':')?;
        let id: u16 = id.parse().ok()?;
        let p = match tipo {
            "tp" => Produto::Tp(id),
            "bau-craft" => Produto::BauCraft(id),
            "moeda" => Produto::Moeda(id),
            "pergaminho-montaria" => Produto::PergaminhoMontaria(id),
            "pergaminho-tomo" => Produto::PergaminhoTomo(id),
            "energia" => Produto::Energia(id),
            "pergaminho-pet" => Produto::PergaminhoPet(id),
            "item-pet" => Produto::ItemDePet(id),
            _ => return None,
        };
        p.existe().then_some(p)
    }

    pub fn existe(&self) -> bool {
        match *self {
            Produto::Tp(i) => pacote(i).is_some(),
            Produto::BauCraft(i) => bau_craft(i).is_some(),
            Produto::Moeda(i) => moeda(i).is_some(),
            Produto::PergaminhoMontaria(i) => pergaminho_montaria(i).is_some(),
            Produto::PergaminhoTomo(i) => pergaminho_tomo(i).is_some(),
            Produto::Energia(i) => energia(i).is_some(),
            Produto::PergaminhoPet(i) => pergaminho_pet(i).is_some(),
            Produto::ItemDePet(i) => item_de_pet(i).is_some(),
        }
    }

    pub fn nome(&self) -> String {
        match *self {
            Produto::Tp(i) => {
                pacote(i).map_or("?".into(), |p| format!("{} ({} TP)", p.nome, p.total()))
            }
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
            Produto::ItemDePet(i) => item_de_pet(i).map_or("?".into(), |p| p.nome.to_string()),
        }
    }

    /// Preco em TP (montaria e skin). Pacote de TP nao tem preco em TP.
    pub fn preco_tp(&self) -> Option<u64> {
        match *self {
            Produto::Tp(_) => None,
            Produto::BauCraft(i) => bau_craft(i).map(|b| b.preco_tp),
            Produto::Moeda(i) => moeda(i).map(|m| m.preco_tp),
            Produto::PergaminhoMontaria(i) => pergaminho_montaria(i).map(|p| p.preco_tp),
            Produto::PergaminhoTomo(i) => pergaminho_tomo(i).map(|p| p.preco_tp),
            Produto::Energia(i) => energia(i).map(|e| e.preco_tp),
            Produto::PergaminhoPet(i) => pergaminho_pet(i).map(|p| p.preco_tp),
            Produto::ItemDePet(i) => item_de_pet(i).map(|p| p.preco_tp),
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

/// banco; o banco confere de novo).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecusaCompra {
    ProdutoInvalido,
    SemSaldo,
}

impl RecusaCompra {
    pub fn texto(&self) -> &'static str {
        match self {
            RecusaCompra::ProdutoInvalido => "Produto indisponível.",
            RecusaCompra::SemSaldo => "TP insuficiente.",
        }
    }
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
    /// Item de TP, `vezes` de uma vez. Tudo na loja e' repetivel, e comprar
    /// dez pergaminhos era tocar dez vezes no mesmo botao e confirmar dez
    /// vezes — pedido do dono em 20/09/2026.
    ///
    /// O LOTE e' um pedido so': um `pedido` (id de idempotencia), uma
    /// transacao, um debito. Dez pedidos separados podiam falhar no meio e
    /// deixar o jogador sem saber quantos entraram.
    ComprarItem {
        produto: Produto,
        /// 1..=`LOTE_MAX`. O servidor clampa — cliente velho nao manda e o
        /// serde cai em 1.
        #[serde(default = "um")]
        vezes: u16,
        pedido: String,
    },
    Montar,
    Desmontar,
}

/// Prêmio já decidido pelo servidor, usado na revelação animada do cliente.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum PremioInvocacao {
    Chave { item_id: u16, cor: u8 },
    /// Montaria: o item_id ja' diz especie e cor.
    Montaria { item_id: u16 },
    Tomo {
        skill_id: u32,
        grau: crate::skills::GrauTomo,
        quantidade: u16,
    },
    /// Pet coletor: o item_id ja' diz especie e grau.
    Pet { item_id: u16 },
}


/// Maximo de unidades num lote. Noventa e nove porque o preco total tem que
/// caber na cabeca do jogador antes de ele confirmar — lote de mil e' pedido
/// que se faz sem querer.
pub const LOTE_MAX: u16 = 99;

fn um() -> u16 {
    1
}

/// `vezes` valido pra um lote: pelo menos 1, no maximo `LOTE_MAX`.
pub fn lote(vezes: u16) -> u16 {
    vezes.clamp(1, LOTE_MAX)
}

/// Desconto do lote: (a partir de quantas unidades, quantos % de abatimento).
/// Da maior faixa pra menor — `desconto_pct` pega a primeira que couber.
///
/// As faixas casam com os atalhos da janela (1x / 10x / 50x): quem toca em
/// "10x" ve' o desconto mudar, que e' o que ensina a regra sem texto.
pub const DESCONTO_DO_LOTE: [(u16, u64); 4] = [(50, 20), (25, 15), (10, 10), (5, 5)];

/// Quantos % de desconto `vezes` unidades rendem.
pub fn desconto_pct(vezes: u16) -> u64 {
    let n = lote(vezes);
    DESCONTO_DO_LOTE
        .iter()
        .find(|(minimo, _)| n >= *minimo)
        .map_or(0, |(_, pct)| *pct)
}

/// Preco TOTAL de `vezes` unidades de `unitario`, com o desconto do lote.
///
/// A divisao inteira TRUNCA, e isso e' de proposito: a sobra fica com o
/// jogador. Arredondar pra cima seria cobrar por um TP que o desconto disse
/// que ele nao ia pagar.
pub fn preco_do_lote(unitario: u64, vezes: u16) -> u64 {
    let cheio = unitario.saturating_mul(lote(vezes) as u64);
    cheio * (100 - desconto_pct(vezes)) / 100
}

/// Pode comprar `vezes` unidades? Tudo na loja hoje e' repetivel (pergaminho,
/// moeda, Energia, item de pet): so' o saldo decide. Montaria e skin sairam
/// daqui — a montaria virou item de bolsa e vem do pergaminho
/// (docs/MONTARIAS.md), e a skin deixou de existir: a cor da montaria E' a
/// variacao dela.
///
/// Devolve o preco TOTAL do lote, ja' com desconto.
pub fn pode_comprar(produto: Produto, vezes: u16, saldo: u64) -> Result<u64, RecusaCompra> {
    let preco = produto.preco_tp().ok_or(RecusaCompra::ProdutoInvalido)?;
    let total = preco_do_lote(preco, vezes);
    if saldo < total {
        return Err(RecusaCompra::SemSaldo);
    }
    Ok(total)
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
            assert_eq!(pode_comprar(p, 1, m.preco_tp), Ok(m.preco_tp));
        }
        assert_eq!(
            pode_comprar(Produto::Moeda(1), 1, 10),
            Err(RecusaCompra::SemSaldo)
        );
    }

    /// O desconto do lote: nunca cobra mais, nunca zera, e quanto maior o
    /// lote menor o preco POR UNIDADE. Sem a ultima, o desconto podia
    /// inverter numa faixa e comprar mais sairia mais caro por peca.
    #[test]
    fn o_desconto_do_lote_so_barateia() {
        // As faixas estao em ordem decrescente — `desconto_pct` pega a
        // primeira que couber, e fora de ordem ela pegaria a errada.
        for par in DESCONTO_DO_LOTE.windows(2) {
            assert!(par[0].0 > par[1].0, "faixas fora de ordem: {DESCONTO_DO_LOTE:?}");
            assert!(par[0].1 > par[1].1, "% fora de ordem: {DESCONTO_DO_LOTE:?}");
        }
        assert!(
            DESCONTO_DO_LOTE.iter().all(|(_, p)| *p < 100),
            "100% seria de graca"
        );

        let unit = 500u64;
        let mut anterior = f64::MAX;
        for vezes in 1..=LOTE_MAX {
            let total = preco_do_lote(unit, vezes);
            let cheio = unit * vezes as u64;
            assert!(total <= cheio, "{vezes}x cobrou MAIS que o cheio");
            assert!(total > 0, "{vezes}x saiu de graca");
            // Por unidade, nunca sobe.
            let por_peca = total as f64 / vezes as f64;
            assert!(
                por_peca <= anterior + 1e-9,
                "{vezes}x ficou mais caro por peca ({por_peca:.2} depois de {anterior:.2})"
            );
            anterior = por_peca;
            // E o abatimento e' exatamente o da faixa (truncado pra baixo).
            assert_eq!(total, cheio * (100 - desconto_pct(vezes)) / 100);
        }

        // As faixas onde o desconto muda, uma a uma.
        assert_eq!(desconto_pct(1), 0);
        assert_eq!(desconto_pct(4), 0);
        assert_eq!(desconto_pct(5), 5);
        assert_eq!(desconto_pct(9), 5);
        assert_eq!(desconto_pct(10), 10);
        assert_eq!(desconto_pct(25), 15);
        assert_eq!(desconto_pct(50), 20);
        assert_eq!(desconto_pct(LOTE_MAX), 20);
        // O clamp vale aqui tambem: cliente pode mandar o que quiser.
        assert_eq!(desconto_pct(0), 0);
        assert_eq!(desconto_pct(u16::MAX), 20);

        // A sobra da divisao fica com o JOGADOR, nunca contra ele.
        assert_eq!(preco_do_lote(3, 10), 27, "30 −10% = 27");
        assert_eq!(preco_do_lote(1, 5), 4, "5 −5% = 4,75 → 4");
    }

    /// O LOTE cobra o TOTAL (ja' com desconto), e o saldo tem que cobrir esse
    /// total — nao o unitario. Cobrar unitario e entregar dez era o jeito
    /// obvio de a loja virar fabrica de TP.
    #[test]
    fn o_lote_cobra_o_total_e_nao_o_unitario() {
        let p = Produto::Moeda(MOEDAS[0].id);
        let unit = MOEDAS[0].preco_tp;
        for vezes in [1u16, 2, 10, LOTE_MAX] {
            let total = preco_do_lote(unit, vezes);
            assert!(total >= unit, "{vezes}x saiu mais barato que UMA unidade");
            assert_eq!(pode_comprar(p, vezes, total), Ok(total), "{vezes}x");
            // Um TP a menos que o total ja' recusa.
            assert_eq!(
                pode_comprar(p, vezes, total - 1),
                Err(RecusaCompra::SemSaldo),
                "{vezes}x com um a menos"
            );
        }
        // O clamp: 0 vale 1, e acima do teto para no teto. O cliente pode
        // mandar qualquer coisa — quem decide e' o servidor.
        assert_eq!(lote(0), 1);
        assert_eq!(lote(1), 1);
        assert_eq!(lote(LOTE_MAX), LOTE_MAX);
        assert_eq!(lote(u16::MAX), LOTE_MAX);
        assert_eq!(pode_comprar(p, 0, unit), Ok(unit), "0 cobra como 1");
        let teto = preco_do_lote(unit, LOTE_MAX);
        assert_eq!(
            pode_comprar(p, u16::MAX, teto),
            Ok(teto),
            "acima do teto cobra o teto"
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
        for b in BAUS_CRAFT {
            assert!(ids.insert(("b", b.id)));
            assert!(b.preco_tp > 0);
            assert_eq!(b.chances_cor.iter().map(|n| *n as u16).sum::<u16>(), 100);
        }
        for p in PERGAMINHOS_MONTARIA {
            assert!(ids.insert(("pm", p.id)));
            assert!(p.preco_tp > 0);
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
        assert_eq!(pode_comprar(Produto::Energia(1), 1, 39), Err(RecusaCompra::SemSaldo));
        assert_eq!(
            pode_comprar(Produto::Energia(1), 1, 40),
            Ok(40)
        );
        assert_eq!(
            pode_comprar(Produto::Energia(99), 1, 9_999),
            Err(RecusaCompra::ProdutoInvalido)
        );
    }

    #[test]
    fn codigo_ida_e_volta() {
        for p in [
            Produto::Tp(2),
            Produto::BauCraft(1),
            Produto::PergaminhoMontaria(1),
            Produto::PergaminhoTomo(1),
            Produto::Energia(2),
        ] {
            assert_eq!(Produto::de_codigo(&p.codigo()), Some(p));
        }
        assert_eq!(Produto::de_codigo("energia:99"), None);
        assert_eq!(Produto::de_codigo("skin:101"), None, "skin nao existe mais");
        assert_eq!(Produto::de_codigo("montaria:1"), None, "montaria virou item");
        assert_eq!(Produto::de_codigo("lixo"), None);
    }

    /// So' o saldo decide. Montaria e skin sairam da loja: a montaria vem do
    /// pergaminho, como item de bolsa (docs/MONTARIAS.md).
    #[test]
    fn regras_de_compra() {
        assert_eq!(
            pode_comprar(Produto::Tp(1), 1, 9999),
            Err(RecusaCompra::ProdutoInvalido),
            "pacote de TP nao se compra COM TP"
        );
        assert_eq!(
            pode_comprar(Produto::PergaminhoMontaria(1), 1, 499),
            Err(RecusaCompra::SemSaldo)
        );
        assert_eq!(pode_comprar(Produto::PergaminhoMontaria(1), 1, 500), Ok(500));
        assert_eq!(
            pode_comprar(Produto::PergaminhoPet(99), 1, 9999),
            Err(RecusaCompra::ProdutoInvalido)
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
    fn montado_corre_mais_sem_sprint_e_luta_desmonta() {
        assert_eq!(velocidade_de_andar(3.0, None, 1.0), 3.0);
        assert_eq!(velocidade_de_andar(3.0, None, 1.65), 3.0 * 1.65);
        assert_eq!(
            velocidade_de_andar(3.0, Some(VEL_MONTADO), 1.65),
            3.0 * VEL_MONTADO,
            "montado nao soma sprint"
        );
        // A COR da montaria equipada e' que manda no multiplicador.
        let cinza = crate::item_id::montaria_no_grau(crate::item_id::MONTARIA_BASE, 1);
        let laranja = crate::item_id::montaria_no_grau(crate::item_id::MONTARIA_BASE, 5);
        assert_eq!(mult_de_montaria(false, Some(laranja)), None, "a pe' e' a pe'");
        assert_eq!(mult_de_montaria(true, Some(cinza)), Some(VEL_MONTADO));
        assert_eq!(
            mult_de_montaria(true, Some(laranja)),
            Some(crate::montarias::velocidade(5))
        );
        assert!(
            mult_de_montaria(true, Some(laranja)) > mult_de_montaria(true, Some(cinza)),
            "a cor melhor tem que correr mais"
        );
        // Montado sem montaria equipada (nao deveria acontecer) cai no piso.
        assert_eq!(mult_de_montaria(true, None), Some(VEL_MONTADO));
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
