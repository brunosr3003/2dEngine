//! Loja de cash e montarias (docs/LOJA.md, docs/MONTARIAS.md).
//!
//! Fonte UNICA do catalogo: pacotes de TP (dinheiro de verdade), montarias e
//! skins de montaria (TP). O servidor valida tudo por aqui; o cliente so'
//! desenha.
//!
//! Regra de TP (docs/ECONOMIA.md): o que a TP compra NAO da' poder de combate.
//! A montaria so' da' mobilidade — e todas correm igual (`VEL_MONTADO`); a
//! skin e' so' cor.

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
    PacoteTp { id: 1, nome: "Punhado de TP", tp: 100, bonus: 0, centavos: 490 },
    PacoteTp { id: 2, nome: "Bolsa de TP", tp: 500, bonus: 50, centavos: 2490 },
    PacoteTp { id: 3, nome: "Baú de TP", tp: 1000, bonus: 200, centavos: 4990 },
    PacoteTp { id: 4, nome: "Tesouro de TP", tp: 2000, bonus: 600, centavos: 9990 },
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
    Skin { id: 101, montaria: 1, nome: "Pelagem Cinza", tinta: [0, 0, 0], forca: 0.0, preco_tp: 0 },
    Skin { id: 102, montaria: 1, nome: "Lobo da Meia-Noite", tinta: [38, 42, 62], forca: 0.55, preco_tp: 300 },
    Skin { id: 103, montaria: 1, nome: "Lobo Dourado", tinta: [232, 178, 64], forca: 0.5, preco_tp: 450 },
    Skin { id: 201, montaria: 2, nome: "Listras Brancas", tinta: [0, 0, 0], forca: 0.0, preco_tp: 0 },
    Skin { id: 202, montaria: 2, nome: "Tigre de Brasa", tinta: [214, 84, 36], forca: 0.5, preco_tp: 400 },
    Skin { id: 203, montaria: 2, nome: "Tigre Espectral", tinta: [120, 196, 255], forca: 0.5, preco_tp: 600 },
    Skin { id: 301, montaria: 3, nome: "Pelo Castanho", tinta: [0, 0, 0], forca: 0.0, preco_tp: 0 },
    Skin { id: 302, montaria: 3, nome: "Urso Polar", tinta: [236, 240, 246], forca: 0.6, preco_tp: 350 },
    Skin { id: 303, montaria: 3, nome: "Urso de Obsidiana", tinta: [30, 24, 34], forca: 0.6, preco_tp: 550 },
];

pub fn pacote(id: u16) -> Option<&'static PacoteTp> {
    PACOTES.iter().find(|p| p.id == id)
}

pub fn montaria(id: u16) -> Option<&'static Montaria> {
    MONTARIAS.iter().find(|m| m.id == id)
}

pub fn skin(id: u16) -> Option<&'static Skin> {
    SKINS.iter().find(|s| s.id == id)
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
}

impl Produto {
    /// Codigo guardado no banco ("tp:2", "montaria:1", "skin:102").
    pub fn codigo(&self) -> String {
        match self {
            Produto::Tp(i) => format!("tp:{i}"),
            Produto::Montaria(i) => format!("montaria:{i}"),
            Produto::Skin(i) => format!("skin:{i}"),
        }
    }

    pub fn de_codigo(c: &str) -> Option<Produto> {
        let (tipo, id) = c.split_once(':')?;
        let id: u16 = id.parse().ok()?;
        let p = match tipo {
            "tp" => Produto::Tp(id),
            "montaria" => Produto::Montaria(id),
            "skin" => Produto::Skin(id),
            _ => return None,
        };
        p.existe().then_some(p)
    }

    pub fn existe(&self) -> bool {
        match *self {
            Produto::Tp(i) => pacote(i).is_some(),
            Produto::Montaria(i) => montaria(i).is_some(),
            Produto::Skin(i) => skin(i).is_some(),
        }
    }

    pub fn nome(&self) -> String {
        match *self {
            Produto::Tp(i) => pacote(i).map_or("?".into(), |p| format!("{} ({} TP)", p.nome, p.total())),
            Produto::Montaria(i) => montaria(i).map_or("?".into(), |m| m.nome.to_string()),
            Produto::Skin(i) => skin(i).map_or("?".into(), |s| s.nome.to_string()),
        }
    }

    /// Preco em TP (montaria e skin). Pacote de TP nao tem preco em TP.
    pub fn preco_tp(&self) -> Option<u64> {
        match *self {
            Produto::Tp(_) => None,
            Produto::Montaria(i) => montaria(i).map(|m| m.preco_tp),
            Produto::Skin(i) => skin(i).filter(|s| s.preco_tp > 0).map(|s| s.preco_tp),
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
        }
    }

    /// A skin que vale pra montar: a escolhida, se a conta tem ela e a
    /// montaria dela; senao a padrao da primeira montaria que tiver.
    pub fn skin_para_montar(&self, escolhida: Option<u16>) -> Option<u16> {
        let valida = |id: u16| skin(id).is_some_and(|s| self.montarias.contains(&s.montaria) && (s.preco_tp == 0 || self.skins.contains(&id)));
        escolhida.filter(|id| valida(*id)).or_else(|| self.montarias.iter().filter_map(|m| montaria(*m)).map(|m| m.skin_padrao).find(|id| valida(*id)))
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
    (8..=PEDIDO_ID_MAX).contains(&id.len()) && id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

// ─────────────────────────────── rede ───────────────────────────────

/// Cliente -> servidor.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum PedidoLoja {
    /// Saldo, posses e historico.
    Estado,
    /// Pacote de TP por dinheiro. `pedido` e' gerado pelo cliente e faz o
    /// clique duplo / reenvio valer uma compra so'.
    ComprarTp { pacote: u16, pedido: String },
    /// Montaria ou skin, com TP.
    ComprarItem { produto: Produto, pedido: String },
    Montar,
    Desmontar,
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
    Resultado { ok: bool, texto: String },
    /// Montando: o cavaleiro sobe em `segundos` (0 = cancelou).
    Montando { segundos: f32 },
}

#[cfg(test)]
mod tests {
    use super::*;

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
            assert!(b >= a, "pacote {} rende menos que o {}", par[1].id, par[0].id);
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
        assert!(VEL_MONTADO > 1.0 && VEL_MONTADO <= 1.6);
    }

    #[test]
    fn codigo_ida_e_volta() {
        for p in [Produto::Tp(2), Produto::Montaria(3), Produto::Skin(102)] {
            assert_eq!(Produto::de_codigo(&p.codigo()), Some(p));
        }
        assert_eq!(Produto::de_codigo("skin:999"), None);
        assert_eq!(Produto::de_codigo("lixo"), None);
    }

    #[test]
    fn regras_de_compra() {
        let nada = Posses::default();
        assert_eq!(pode_comprar(&nada, Produto::Montaria(1), 499), Err(RecusaCompra::SemSaldo));
        assert_eq!(pode_comprar(&nada, Produto::Montaria(1), 500), Ok(500));
        assert_eq!(pode_comprar(&nada, Produto::Skin(102), 9999), Err(RecusaCompra::PrecisaDaMontaria));
        assert_eq!(pode_comprar(&nada, Produto::Skin(101), 9999), Err(RecusaCompra::ProdutoInvalido), "skin padrao nao se vende");
        assert_eq!(pode_comprar(&nada, Produto::Tp(1), 9999), Err(RecusaCompra::ProdutoInvalido));
        let lobo = Posses { montarias: vec![1], skins: vec![101] };
        assert_eq!(pode_comprar(&lobo, Produto::Montaria(1), 9999), Err(RecusaCompra::JaPossui));
        assert_eq!(pode_comprar(&lobo, Produto::Skin(102), 300), Ok(300));
    }

    #[test]
    fn skin_para_montar() {
        let nada = Posses::default();
        assert_eq!(nada.skin_para_montar(None), None);
        let p = Posses { montarias: vec![1, 2], skins: vec![101, 201, 202] };
        assert_eq!(p.skin_para_montar(None), Some(101));
        assert_eq!(p.skin_para_montar(Some(202)), Some(202));
        assert_eq!(p.skin_para_montar(Some(203)), Some(101), "skin nao comprada cai na padrao");
        assert_eq!(p.skin_para_montar(Some(302)), Some(101), "skin de montaria que nao tem");
        // A padrao vale mesmo sem estar na lista de skins (vem com a montaria).
        let so_montaria = Posses { montarias: vec![3], skins: vec![] };
        assert_eq!(so_montaria.skin_para_montar(Some(301)), Some(301));
    }

    #[test]
    fn montado_corre_mais_sem_sprint_e_luta_desmonta() {
        assert_eq!(velocidade_de_andar(3.0, false, 1.0), 3.0);
        assert_eq!(velocidade_de_andar(3.0, false, 1.65), 3.0 * 1.65);
        assert_eq!(velocidade_de_andar(3.0, true, 1.65), 3.0 * VEL_MONTADO, "montado nao soma sprint");
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
