//! A COLONIA: a ilha do jogador (docs/COLONIA.md).
//!
//! Recebida numa quest antes de sair da primeira ilha, alcançada por
//! teleporte no porto. Ela **rende sozinha enquanto voce nao esta' la'** — e
//! e' por isso que existe: um lugar que trabalha por voce e cresce com o que
//! voce investe nele.
//!
//! # Por que ela e' barata
//!
//! O terreno ja' faz o trabalho pesado: `Ilha::gerar` e' funcao pura da
//! semente. A colonia e' `hash(nome do personagem)` num raio pequeno —
//! deterministica, zero de armazenamento, materializada so' quando alguem
//! visita. O que o banco guarda sao QUATRO numeros: nivel de cada eixo e
//! quando foi a ultima colheita.
//!
//! E cada um esta' sozinho na sua: a colonia vive em INSTANCIA, como a
//! dungeon. Sem espaco de coordenadas compartilhado, sem silhueta de vizinho,
//! sem AOI nova. Foi exatamente o que encareceu o Mar Aberto.

use crate::constants::item_id;

/// O nome da zona da colonia.
pub const ZONA: &str = "colonia";

/// Esta zona e' a colonia?
pub fn e_colonia(z: &str) -> bool {
    z == ZONA
}

/// Onde o jogador aparece ao chegar na colonia.
///
/// O centro da propria ilha: ela e' pequena, entao nao ha' "longe" pra
/// nascer, e nascer no meio e' o que faz a primeira visita mostrar tudo de
/// uma vez.
pub const CHEGADA: glam::Vec2 = glam::Vec2::ZERO;

/// Quantos eixos a colonia tem.
pub const EIXOS: usize = 3;

pub mod eixo {
    /// O ASSENTAMENTO: casa → vila → castelo. Mais chao aplainado, mais
    /// construcao e mais gente morando.
    ///
    /// Era o TAMANHO da ILHA, e o nome mudou junto com a coisa: a ilha nao
    /// cresce mais. Ela e' uma so' e inteira desde o primeiro dia (ver
    /// `RAIO_BLOCOS`), e o que cresce e' o que ha' em cima dela.
    pub const ASSENTAMENTO: usize = 0;
    /// Nome velho, pros chamadores que ainda falam em tamanho.
    pub const TAMANHO: usize = ASSENTAMENTO;
    /// O OFICIO dos moradores: quanto cada trabalhador rende por hora.
    pub const RECURSOS: usize = 1;
    /// O BANCO LOCAL: espacos de deposito na propria ilha.
    pub const BANCO: usize = 2;
    pub const NOMES: [&str; super::EIXOS] = ["Settlement", "Trade", "Bank"];
}

/// Niveis por eixo, 0..=MAX.
pub const NIVEL_MAX: u8 = 5;

/// A ilha INTEIRA, sempre. Nao depende de nivel nenhum.
///
/// Ate' 21/09/2026 o raio crescia com o nivel, e isso CORTAVA a ilha: com a
/// mesma semente, `Forma::de(semente, raio)` escala o contorno, entao subir o
/// nivel nao acrescentava terra na beirada — desenhava outra ilha. Tudo
/// andava de lugar: a costa, a praca, a casa que o jogador via da janela.
/// O dono cortou a ideia — "a ideia é não cortar a ilha".
///
/// Agora o relevo e' FIXO: uma ilha, inteira, igual pra todo mundo e igual
/// pra sempre. O que cresce e' o ASSENTAMENTO em cima dela
/// (`plato_do_assentamento`), que so' aplaina mais chao em volta da mesma
/// praca — nenhuma coluna que ja' estava plana muda.
pub const RAIO_BLOCOS: i32 = 180;

// ──────────────────────── a ilhota, desenhada ────────────────────────
//
// Desde 22/09/2026 o relevo da colônia é DESENHADO, como o da Ilha Mágica, e
// não sorteado por Perlin. O dono, vendo a maquete:
//
// > "a ilha que está sendo mostrada em 3D hoje é a ilha que era antigamente.
// > Como não vai ter mais como andar na ilha, tem que ser algo muito mais
// > simples e visualmente agradável: tem que ser uma ilhota mesmo, uma praia,
// > quase que dá pra ver tudo em 360, com o centro sendo a cidade."
//
// **Encolher o raio não resolvia.** Medido: com a mesma semente,
// `Forma::de(semente, raio)` ESCALA o contorno, então a ilha ficava com ~37%
// de terra em qualquer raio — de 280 blocos a 120 — e o platô do assentamento
// (81 u com a rampa) só coube nos 140 u originais. Diminuir a ilha só afogava
// a cidade: o platô seco caía de 84% para 26%.
//
// Desenhada, a ilhota é o que ela precisa ser: redonda o bastante para caber
// na tela inteira, com a cidade no meio e praia em volta.

/// O raio médio da ilhota, em unidades de mundo.
pub const RAIO: f32 = RAIO_BLOCOS as f32 * crate::terreno::BLOCO * 0.94;
/// Altura do meio da ilhota, antes de a praça aplainar.
///
/// Eram 9,0 — oito unidades de desnível num raio de 85: uma moeda, não uma
/// ilha, e o dono disse "tá chapada plana e horrível".
///
/// Trinta foi longe DEMAIS, e a prévia mostrou por quê: a praça é aplainada
/// num disco (`Cidade::aplainar`) com rampa de 14 u, e com o meio a 30 u o
/// terreno em volta despencava — a cidade virava uma MESA DE PEDRA terraçada
/// no meio da ilha, que lê como pedreira.
///
/// Dezesseis dá silhueta sem fazer barranco: quem carrega o relevo é a
/// ondulação, que cresce PRA FORA (`0,30 + 0,70·t`) e deixa o meio calmo
/// justamente onde a cidade assenta.
pub const ALTURA_TOPO: f32 = 16.0;
/// Altura da ORLA: logo acima do mar, que é o que faz ela ler como praia.
pub const ALTURA_ORLA: f32 = 0.8;
/// O que há FORA da ilhota, em índice de bloco.
///
/// Bem fundo (−64 blocos = −32 u), e isso é decisão de MAQUETE, não de mundo:
/// a colônia não é mais uma zona que se anda, é uma peça que se olha. A −8
/// aquilo era um fundo de mar raso, e sem água por cima virava um prato bege
/// em volta da ilha — visto na prévia. Fundo assim, ele cai atrás da saia de
/// terra do diorama e some.
pub const NIVEL_FUNDO: i32 = -64;

/// O raio da ilhota naquela direção.
///
/// Dois senos e nada mais: a costa precisa ondular para não parecer uma
/// moeda, e ruído de verdade seria pagar fbm por coluna para desenhar uma
/// forma que cabe em duas linhas. Determinístico, então cliente e servidor
/// concordam sem combinar nada.
pub fn raio_na_direcao(ang: f32) -> f32 {
    RAIO * (1.0 + 0.10 * (ang * 3.0 + 0.7).sin() + 0.05 * (ang * 5.0 - 1.9).sin())
}

/// O bloco de topo da coluna, em índice de bloco. A ÚNICA fonte do relevo da
/// colônia — a praça (`Cidade::aplainar`) entra depois, por cima.
pub fn bloco_da_coluna(bx: i32, bz: i32) -> i32 {
    let (x, z) = (
        bx as f32 * crate::terreno::BLOCO,
        bz as f32 * crate::terreno::BLOCO,
    );
    let d = (x * x + z * z).sqrt();
    let r = raio_na_direcao(z.atan2(x));
    if d >= r {
        return NIVEL_FUNDO;
    }
    // Quadrática: quase plana no meio (onde a cidade vai) e caindo para a
    // praia. No pior ponto isso dá 0,12 bloco de degrau por coluna — não há
    // como criar paredão aqui.
    let t = d / r;
    let h = ALTURA_TOPO + (ALTURA_ORLA - ALTURA_TOPO) * t * t;
    // RELEVO POR CIMA, senão a ilha vira um alvo de tiro.
    //
    // Um domo liso quantizado em blocos de 0,5 u vira ANÉIS concêntricos, e o
    // olho lê aquilo como curva de nível de mapa topográfico.
    //
    // O RELEVO PESA. Ele era ±1,1 u num desnível de 12 — ruído, não
    // terreno. O dono: "a ilha tá muito feia, tá chapada plana e horrível".
    //
    // Agora ±4,5 u, com uma onda longa que faz morro e vale de verdade e duas
    // curtas que quebram o contorno. O degrau por coluna no pior ponto fica
    // em 0,30 u — abaixo do bloco (0,5) que o passo vence, então a ilhota
    // continua caminhável por construção.
    //
    // A amplitude ainda cai perto da orla (`t`), pra praia continuar praia e
    // a costa não virar penhasco.
    // A ÚLTIMA onda é CURTA de propósito.
    //
    // As três longas (períodos de 180, 77 e 43 u) fazem morro e vale, mas não
    // quebram os ANÉIS: eles nascem de a altura cruzar a fronteira de um
    // bloco ao longo de um círculo, e uma onda mais longa que o anel
    // atravessa todos os anéis junto. Só uma onda da ordem do espaçamento
    // deles (~14 u) faz a fronteira serpentear.
    //
    // A amplitude é limitada pelo passo: 0,7 u a cada 15 u de período dá 0,15
    // u de degrau por coluna, e somada à ladeira do domo (0,34) fica em 0,98
    // bloco — abaixo do 1 que o passo vence, por pouco.
    // `a_ladeira_da_ilhota_nunca_vira_paredao` é quem confere.
    // DUAS ondas de forma, e nao quatro.
    //
    // Havia mais duas no meio (periodos de ~77 e ~43 u) e o dono: "a ilha
    // poder ser mais simples ainda que isso". Elas nao faziam morro nenhum —
    // no porte de 1,15 e 0,45 u contra os 16 de altura da ilhota, davam
    // caroco, nao relevo. De longe (e a maquete SO' e' vista de longe) isso
    // le como grama suja.
    //
    // Fica a longa, que e' quem da a silhueta, e fica a CURTA — que nao e'
    // forma, e' anti-anel: sem ela a altura cruza a fronteira de bloco ao
    // longo de um circulo e a ilha ganha curvas de nivel desenhadas.
    let ondula = ((x * 0.035 + 1.7).sin() * (z * 0.031 - 0.9).cos() * 2.9
        + (x * 0.082 - z * 0.061).sin() * 0.55
        + (x * 0.42 + 0.4).sin() * (z * 0.39 - 1.1).sin() * 0.70)
        * (0.30 + 0.70 * t);
    ((h + ondula) / crate::terreno::BLOCO).round() as i32 - 1
}

/// O raio da ilha. O nivel sobrou da versao que crescia e e' ignorado.
pub fn raio_blocos(_nivel: u8) -> i32 {
    RAIO_BLOCOS
}

/// O PLATO do assentamento, em unidades de mundo, pelo nivel.
///
/// E' isto que cresce quando o jogador sobe o assentamento: mais chao
/// APLAINADO em volta da mesma praca, no mesmo nivel de sempre. Crescer o
/// plato nunca mexe no que ja' estava plano — a rampa e' que anda pra fora.
pub fn plato_do_assentamento(nivel: u8) -> f32 {
    // Encolheu junto com a ilhota. O piso é o platô de uma cidade normal do
    // mundo (`Cidade::RAIO_PLATO`, 28 u), que o comentário de lá descreve
    // como "grande o bastante pro anel de ofícios caber inteiro no plano" —
    // então o nível 2 já cabe tudo, e os de cima só abrem espaço.
    16.0 + 4.0 * nivel.clamp(1, NIVEL_MAX) as f32
}

/// O que o assentamento E', pelo nivel. E' o nome que o jogador ve' e o que
/// decide quanta construcao nasce na ilha.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Assentamento {
    Casa,
    Vila,
    Castelo,
}

impl Assentamento {
    pub fn do_nivel(nivel: u8) -> Self {
        match nivel.clamp(1, NIVEL_MAX) {
            1 | 2 => Self::Casa,
            3 | 4 => Self::Vila,
            _ => Self::Castelo,
        }
    }

    pub fn nome(self) -> &'static str {
        match self {
            Self::Casa => "House",
            Self::Vila => "Village",
            Self::Castelo => "Castle",
        }
    }

}

/// Quantos MORADORES o assentamento sustenta, por nivel. Um por construcao.
///
/// O nivel 2 tem UMA vaga de proposito: e' o degrau do tutorial. Pular da
/// casa vazia pra vila de tres obrigaria o jogador a contratar tres pessoas
/// pra aprender a contratar uma, e o tutorial ensinaria o passo errado —
/// "encha as vagas" em vez de "escolha um oficio".
const VAGAS: [usize; 5] = [0, 1, 3, 4, 6];

/// Quantos MORADORES o assentamento sustenta. Um por construcao de oficio.
pub fn vagas_de_trabalho(nivel: u8) -> usize {
    VAGAS[nivel.clamp(1, NIVEL_MAX) as usize - 1]
}

/// A semente da colonia. UMA SO', igual pra todo mundo.
///
/// Ate' 21/09/2026 ela saia do hash do NOME: cada personagem tinha uma ilha
/// diferente. O dono cortou a ideia — "não quero que seja única, vai pesar e
/// ter margem pra erro" — e ele esta' certo nas duas contas:
///
/// - **peso**: relevo por jogador e' um campo de altura por jogador VIVO no
///   processo, e nada o limita a nao ser quantas pessoas entraram. Com uma
///   semente so', o servidor guarda uma ilha por TAMANHO (seis, no maximo),
///   quantos jogadores existirem;
/// - **margem pra erro**: cada semente e' um mundo que ninguem olhou. O teste
///   `toda_colonia_tem_chao_onde_se_chega` provava seis nomes; os outros bilhoes
///   iam no escuro — um hash azarado entregava o jogador boiando, e so' ELE
///   veria. Uma ilha so' e' uma ilha que da' pra olhar.
///
/// A ilha continua sendo SUA: a instancia e' por personagem
/// (`world::colonia::instancia_do_nome`), entao ninguem entra na sua. O que
/// deixa de ser unico e' o RELEVO, nao a posse.
pub const SEMENTE: i32 = 1_973_725_076;

/// A semente da colonia. O argumento sobrou da versao por-nome e continua
/// aqui pros chamadores nao precisarem saber disso.
pub fn semente(_nome: &str) -> i32 {
    SEMENTE
}

// ─────────────────────────── a producao ───────────────────────────

/// Horas ate' a colheita "cheia".
pub const HORAS_CHEIAS: f32 = 12.0;
/// Horas ate' o deposito parar de acumular.
pub const HORAS_TETO: f32 = 24.0;
/// Que fracao do rendimento sobra entre `HORAS_CHEIAS` e `HORAS_TETO`.
///
/// Nao zero e nao um: quem volta em 12 h leva tudo, quem volta em 24 leva
/// 75% do que teria levado em duas visitas. O atraso custa, mas nao apaga —
/// castigo binario faria o jogador sentir que perdeu o dia inteiro por meia
/// hora de sono.
pub const FRACAO_ATRASADA: f32 = 0.5;

/// O OFICIO de um morador. Cada um faz uma coisa, e o que ele faz e' o que
/// entra no deposito — a ilha deixa de ser um botao de colher e passa a ser o
/// que o jogador montou nela.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Profissao {
    /// Derruba arvore: MADEIRA.
    Lenhador,
    /// Quebra pedra: ACO.
    Minerador,
    /// Caca e escolta: COBRE (o soldo que ele traz).
    Mercenario,
    /// Trabalha o couro e o tecido: COURO.
    Curtidor,
    /// Destila: QUINTESSENCIA.
    Alquimista,
}

impl Profissao {
    /// Do indice do fio (`PedidoColonia::Contratar`). `None` = indice que nao
    /// existe, que e' pedido malformado e nao um oficio novo.
    pub fn do_indice(i: u8) -> Option<Self> {
        Self::TODAS.get(i as usize).copied()
    }

    pub fn indice(self) -> u8 {
        Self::TODAS.iter().position(|p| *p == self).unwrap_or(0) as u8
    }

    pub const TODAS: [Profissao; 5] = [
        Profissao::Lenhador,
        Profissao::Minerador,
        Profissao::Mercenario,
        Profissao::Curtidor,
        Profissao::Alquimista,
    ];

    pub fn nome(self) -> &'static str {
        match self {
            Self::Lenhador => "Woodcutter",
            Self::Minerador => "Miner",
            Self::Mercenario => "Mercenary",
            Self::Curtidor => "Tanner",
            Self::Alquimista => "Alchemist",
        }
    }

    /// O que ele produz, e quanto por hora no nivel 1 de OFICIO.
    ///
    /// Os numeros sao baixos de proposito: renda passiva que compete com
    /// jogar mata o jogo. Seis moradores no oficio maximo rendem menos que
    /// uma hora de caca — a colonia paga quem VOLTA, nao quem some.
    pub fn por_hora(self) -> (u16, u32) {
        match self {
            Self::Lenhador => (item_id::WOOD_T1, 8),
            Self::Minerador => (item_id::na_cor(item_id::STEEL, 1), 4),
            Self::Mercenario => (item_id::COPPER, 45),
            Self::Curtidor => (item_id::HIDE, 2),
            Self::Alquimista => (item_id::QUINTESSENCE, 2),
        }
    }

    /// Que PAPEL da vila desenha a construcao dele. Reusa o gerador de vila
    /// do mundo normal — a casa com o toldo do oficio ja' existe, e fazer
    /// outra seria uma segunda fonte de verdade pro mesmo desenho.
    pub fn papel(self) -> crate::construcao::Papel {
        use crate::construcao::Papel;
        match self {
            Self::Lenhador => Papel::Deposito,
            Self::Minerador => Papel::Ferreiro,
            Self::Mercenario => Papel::Treinador,
            Self::Curtidor => Papel::Alfaiate,
            Self::Alquimista => Papel::Alquimista,
        }
    }
}

/// Quanto UM trabalhador rende por hora, no nivel de OFICIO dado.
pub fn por_hora_do_trabalhador(p: Profissao, nivel_oficio: u8) -> (u16, u32) {
    let (item, base) = p.por_hora();
    (item, base * (nivel_oficio.clamp(1, NIVEL_MAX) as u32))
}

/// Quanto a colonia INTEIRA rende por hora: a soma dos moradores.
///
/// Sem morador nenhum ela nao rende nada. E' a diferenca entre uma ilha que
/// pinga recurso sozinha e uma que o jogador POVOOU — e o dono pediu a
/// segunda.
pub fn por_hora_dos_trabalhadores(
    trabalhadores: &[Profissao],
    nivel_oficio: u8,
) -> Vec<(u16, u32)> {
    let mut soma: Vec<(u16, u32)> = Vec::new();
    for t in trabalhadores {
        let (item, q) = por_hora_do_trabalhador(*t, nivel_oficio);
        match soma.iter_mut().find(|(i, _)| *i == item) {
            Some((_, acc)) => *acc += q,
            None => soma.push((item, q)),
        }
    }
    soma
}

/// Quantas HORAS EFETIVAS valeram, dadas `horas` desde a ultima colheita.
///
/// A curva inteira num lugar so': cheio ate' 12 h, meio de 12 a 24, nada
/// depois. Enunciavel numa frase — "colha a cada 12, ela guarda ate' 24" — e
/// e' isso que faz o jogador conseguir planejar sem abrir planilha.
pub fn horas_efetivas(horas: f32) -> f32 {
    let h = horas.max(0.0);
    if h <= HORAS_CHEIAS {
        h
    } else {
        HORAS_CHEIAS + (h.min(HORAS_TETO) - HORAS_CHEIAS) * FRACAO_ATRASADA
    }
}

/// O que ha' pra colher agora.
pub fn colheita(trabalhadores: &[Profissao], nivel_oficio: u8, horas: f32) -> Vec<(u16, u32)> {
    let h = horas_efetivas(horas);
    por_hora_dos_trabalhadores(trabalhadores, nivel_oficio)
        .into_iter()
        .filter_map(|(id, q)| {
            let n = (q as f32 * h) as u32;
            (n > 0).then_some((id, n))
        })
        .collect()
}

/// Espacos do BAU DA ILHA, pelo nivel de BANCO.
///
/// Nunca zero: o bau e' onde a colheita CAI, e um bau de zero espacos faria a
/// ilha render pra lugar nenhum. O nivel 1 (que todo mundo comeca com) da' o
/// suficiente pros cinco oficios mais folga.
pub fn espacos_do_banco(nivel: u8) -> u8 {
    8 * nivel.clamp(1, NIVEL_MAX)
}

/// O mesmo numero, com o nome do que ele e' hoje.
pub fn espacos_do_bau(nivel: u8) -> usize {
    espacos_do_banco(nivel) as usize
}

/// Poe `qtd` de `item` no bau, respeitando o teto de espacos.
///
/// Devolve o que NAO caebe. Empilha por item (a colheita nao tem instancia),
/// entao o bau cheio e' cheio de ITENS DIFERENTES, e nao de quantidade.
pub fn guardar_no_bau(
    bau: &mut Vec<crate::InventorySlot>,
    espacos: usize,
    item: u16,
    qtd: u32,
) -> u32 {
    if let Some(slot) = bau.iter_mut().find(|s| s.item_id == item) {
        slot.qty = slot.qty.saturating_add(qtd);
        return 0;
    }
    if bau.len() >= espacos {
        return qtd;
    }
    bau.push(crate::InventorySlot {
        item_id: item,
        qty: qtd,
        instance: None,
    });
    0
}

/// (item, quantidade) pra subir `eixo` ate' `nivel_alvo`.
pub fn custo(eixo: usize, nivel_alvo: u8) -> [(u16, u32); 2] {
    let n = nivel_alvo.clamp(1, NIVEL_MAX) as u32;
    // O eixo pesa: banco e' o mais caro porque e' o que mais muda a vida.
    let peso = match eixo {
        self::eixo::TAMANHO => 1,
        self::eixo::RECURSOS => 2,
        _ => 3,
    };
    [
        (item_id::COPPER, 2_000 * n * n * peso),
        (item_id::WOOD_T1, 40 * n * peso),
    ]
}

#[cfg(test)]
mod testes {
    use super::*;

    /// A curva de acumulo, enunciada: cheio ate' 12, meio ate' 24, nada
    /// depois.
    #[test]
    fn colher_em_doze_leva_tudo_e_atrasar_custa_metade() {
        assert_eq!(horas_efetivas(0.0), 0.0);
        assert_eq!(horas_efetivas(6.0), 6.0);
        assert_eq!(horas_efetivas(HORAS_CHEIAS), HORAS_CHEIAS);
        // De 12 a 24 rende metade.
        assert_eq!(horas_efetivas(18.0), 12.0 + 6.0 * FRACAO_ATRASADA);
        assert_eq!(horas_efetivas(HORAS_TETO), 12.0 + 12.0 * FRACAO_ATRASADA);
        // Depois de 24 nao acumula mais nada: dormir dois dias nao dobra.
        assert_eq!(horas_efetivas(48.0), horas_efetivas(HORAS_TETO));
        assert_eq!(horas_efetivas(1_000.0), horas_efetivas(HORAS_TETO));
    }

    /// Duas colheitas de 12 h rendem MAIS que uma de 24.
    ///
    /// E' o que faz a regra ser um convite a voltar em vez de um castigo por
    /// nao voltar: quem joga todo dia ganha mais, e quem sumiu um dia ainda
    /// leva a maior parte.
    #[test]
    fn voltar_cedo_rende_mais_e_sumir_nao_zera() {
        let duas = horas_efetivas(12.0) * 2.0;
        let uma = horas_efetivas(24.0);
        assert!(duas > uma, "voltar cedo tinha que render mais");
        assert!(
            uma > duas * 0.7,
            "sumir um dia nao pode apagar o dia: {uma} contra {duas}"
        );
    }

    /// Todo eixo faz alguma coisa a cada nivel, e o banco so' existe depois
    /// de investido.
    ///
    /// A ILHA nao cresce mais — o que cresce e' o plato do assentamento
    /// (`RAIO_BLOCOS` explica por que). Este teste mudou junto: onde ele
    /// media o raio, mede o plato.
    #[test]
    fn todo_eixo_sobe_e_o_banco_comeca_zerado() {
        for n in 1..NIVEL_MAX {
            assert!(
                plato_do_assentamento(n + 1) > plato_do_assentamento(n),
                "o assentamento parou de crescer no {n}"
            );
            assert_eq!(
                raio_blocos(n + 1),
                raio_blocos(n),
                "a ilha voltou a mudar de tamanho — ela tem que ser uma so'"
            );
            let um = [Profissao::Lenhador];
            assert!(
                por_hora_dos_trabalhadores(&um, n + 1)[0].1
                    > por_hora_dos_trabalhadores(&um, n)[0].1,
                "o oficio parou de render mais no {n}"
            );
            assert!(custo(eixo::ASSENTAMENTO, n + 1)[0].1 > 0);
        }
        // O bau NUNCA tem zero espacos: e' onde a colheita cai, e um bau de
        // zero faria a ilha render pra lugar nenhum.
        assert!(espacos_do_bau(1) >= Profissao::TODAS.len());
        assert!(espacos_do_bau(NIVEL_MAX) > espacos_do_bau(1));
        // O banco e' o eixo mais caro: e' o que mais muda a vida.
        assert!(custo(eixo::BANCO, 1)[0].1 > custo(eixo::ASSENTAMENTO, 1)[0].1);
    }

    /// Sem morador a ilha NAO rende. E' a regra que faz o assentamento
    /// importar: uma ilha vazia e' um terreno, e nao uma fazenda.
    #[test]
    fn ilha_vazia_nao_rende_e_cada_morador_soma() {
        assert!(colheita(&[], NIVEL_MAX, 24.0).is_empty());
        let um = colheita(&[Profissao::Lenhador], 1, 12.0);
        let dois = colheita(&[Profissao::Lenhador, Profissao::Lenhador], 1, 12.0);
        assert_eq!(um.len(), 1, "um lenhador rende um recurso");
        assert_eq!(dois[0].1, um[0].1 * 2, "dois lenhadores rendem o dobro");
        // Oficios diferentes sao LINHAS diferentes, e nao um monte so'.
        let mistos = colheita(&[Profissao::Lenhador, Profissao::Minerador], 1, 12.0);
        assert_eq!(mistos.len(), 2);
    }

    /// As vagas sobem, nunca descem, e o nivel 2 tem UMA — o degrau do
    /// tutorial de contratar.
    #[test]
    fn o_assentamento_abre_as_vagas() {
        assert_eq!(Assentamento::do_nivel(1), Assentamento::Casa);
        assert_eq!(vagas_de_trabalho(1), 0, "a casa mora so' o dono");
        assert_eq!(
            vagas_de_trabalho(2),
            1,
            "o nivel 2 e' o degrau do tutorial: UMA vaga, pra ensinar a              escolher um oficio em vez de encher tres"
        );
        for n in 1..NIVEL_MAX {
            assert!(
                vagas_de_trabalho(n + 1) >= vagas_de_trabalho(n),
                "as vagas encolheram do {n} pro {}: alguem seria despejado",
                n + 1
            );
        }
        assert!(vagas_de_trabalho(NIVEL_MAX) > vagas_de_trabalho(2));
        // O plato acompanha: gente nova precisa de chao plano pra morar.
        assert!(plato_do_assentamento(NIVEL_MAX) > plato_do_assentamento(1));
    }

    /// A ilha e' a MESMA pra todo mundo — de proposito (ver `SEMENTE`).
    ///
    /// O que era um teste de que dois nomes dao ilhas DIFERENTES agora prova o
    /// contrario, e e' o mesmo teste: o que ele guarda e' a decisao.
    #[test]
    fn a_semente_e_uma_so_pra_todo_mundo() {
        assert_eq!(semente("curandinho"), semente("onurb"));
        assert_eq!(semente(""), SEMENTE);
        assert!(SEMENTE > 0);
    }
}

// ─────────────────────────── protocolo ───────────────────────────

/// O que o jogador pede a colonia.
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum PedidoColonia {
    /// O estado, pro painel.
    ///
    /// A ilha NAO E' MAIS UMA ZONA (decisao do dono, 22/09/2026): ela virou
    /// painel com maquete. Nao ha' viagem de ida nem de volta, nao ha' mural
    /// nem cais, e o painel abre de qualquer lugar do arquipelago.
    ///
    /// O que motivou: a ilha como LUGAR cobrava a caminhada e nao entregava
    /// nada nela — praca vazia, um boneco sem icone no cais, e a interface do
    /// que importa (o que se tem, o que rende) escondida atras de tudo isso.
    /// "Talvez seja melhor refatorar, virar apenas menu com mapa 3D."
    Painel,
    /// Colhe o que ela rendeu.
    Colher,
    /// Sobe um eixo (`colonia::eixo`).
    Melhorar { eixo: u8 },
    /// Poe (ou troca) o morador da vaga `vaga`. A construcao daquele lugar
    /// vira a do oficio novo.
    Contratar { vaga: u8, oficio: u8 },
    /// Esvazia a vaga: a casa some e o rendimento dela para.
    Demitir { vaga: u8 },
    /// Tira do bau da ilha o que couber na bolsa.
    Retirar,
}

/// O estado da colonia, pro painel.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum AvisoColonia {
    Estado {
        niveis: [u8; EIXOS],
        /// Horas desde a ultima colheita (ja' cruas, nao efetivas: o painel
        /// mostra as duas, e ver a diferenca e' o que ensina a regra).
        horas: f32,
        /// O que ha' pra colher agora.
        colheita: Vec<(u16, u32)>,
        /// Por eixo: o custo do proximo nivel. Vazio = no maximo.
        custos: Vec<Vec<(u16, u32)>>,
        /// Espacos do bau da ilha.
        banco: u8,
        /// O que esta' NO BAU agora. A colheita cai aqui, e nao na bolsa.
        #[serde(default)]
        bau: Vec<crate::InventorySlot>,
        /// Os moradores, na ordem das vagas. Menor que `vagas` = ha' vaga
        /// aberta, e o painel mostra o lugar vazio.
        #[serde(default)]
        trabalhadores: Vec<Profissao>,
        /// Quantas vagas o assentamento sustenta hoje.
        #[serde(default)]
        vagas: u8,
    },
    /// O relevo e o que ha' em cima dele. A semente e o raio sao constantes
    /// dos dois lados (`SEMENTE`, `RAIO_BLOCOS`); o que o cliente nao tem como
    /// adivinhar e' o PLATO do assentamento e QUEM mora na ilha.
    Terreno {
        /// Raio do chao aplainado (`plato_do_assentamento`).
        plato: f32,
        /// Nivel do assentamento: casa, vila ou castelo.
        assentamento: u8,
        /// Os moradores, na ordem das vagas. Sao eles que decidem QUAIS casas
        /// nascem, e por isso vem junto do relevo e nao so' no painel: o
        /// desenho da ilha precisa deles antes de o jogador abrir nada.
        trabalhadores: Vec<Profissao>,
    },
    Recusa(String),
}

// ─────────────────────────── persistencia ───────────────────────────

/// O que a colonia guarda entre sessoes. Uma coluna JSON so' — a colonia vai
/// ganhar campos (nome dado pelo jogador, decoracao), e cada um deles nao pode
/// custar uma coluna nova e as quatro edicoes de UPSERT que ela exige.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct DadosColonia {
    /// Nivel de cada eixo (`eixo`).
    pub niveis: [u8; EIXOS],
    /// Unix seconds da ultima colheita. 0 = nunca colheu (e a conta comeca
    /// na primeira visita, nao na criacao do personagem).
    pub colhida_em: i64,
    /// Se a quest ja' entregou a ilha.
    pub tem: bool,
    /// Sobra da epoca em que a ilha era zona e se viajava pra ela. Fica no
    /// JSON pra nao quebrar o save de quem tem a coluna preenchida; nada le'.
    #[serde(default)]
    pub volta: String,
    /// Os MORADORES, na ordem das vagas (`vagas_de_trabalho`). Vazio = a ilha
    /// nao rende nada, que e' como ela comeca: uma casa sozinha.
    ///
    /// Lista e nao contagem por oficio: a ordem e' a das construcoes na vila,
    /// e e' ela que decide qual casa nasce onde. Trocar de oficio tem que
    /// trocar a casa daquele lugar, e nao remontar o assentamento inteiro.
    #[serde(default)]
    pub trabalhadores: Vec<Profissao>,
    /// O BAU DA ILHA: onde a colheita cai.
    ///
    /// A colheita ia direto pra bolsa do jogador, e isso apagava a ilha como
    /// LUGAR — dava pra administrar a colonia inteira de outra ilha, sem
    /// nunca pisar nela. O dono pediu o contrario: colher enche o bau, e o
    /// bau esta' aqui. Buscar e' uma viagem.
    ///
    /// Vive no `colonia_json`, como o resto: o bau e' da ilha e nao da sessao,
    /// e tem que sobreviver ao logout (`espacos_do_bau` diz quantos caibem).
    #[serde(default)]
    pub bau: Vec<crate::InventorySlot>,
}

impl Default for DadosColonia {
    fn default() -> Self {
        Self {
            niveis: [1; EIXOS],
            colhida_em: 0,
            tem: false,
            volta: String::new(),
            trabalhadores: Vec::new(),
            bau: Vec::new(),
        }
    }
}

#[cfg(test)]
mod testes_persistencia {
    use super::*;

    #[test]
    fn coluna_vazia_vira_colonia_nivel_um_sem_quest() {
        // O default e' o que TODO personagem velho le' no primeiro login: a
        // ilha existe no papel, mas `tem` falso mantem o teleporte escondido
        // ate' a quest. Nivel 0 nao pode ser o default — `raio_blocos(0)` da'
        // uma ilha, e um eixo em 0 custaria um caso especial em cada conta.
        let d: DadosColonia = serde_json::from_str("{}").unwrap();
        assert_eq!(d.niveis, [1; EIXOS]);
        assert!(!d.tem);
        assert_eq!(d.colhida_em, 0);
    }
}

#[cfg(test)]
mod testes_do_chao {
    use super::*;

    /// "lugar onde anda". A colonia nasce de um hash do NOME, e nada garante
    /// que um hash qualquer de' uma ilha com terra no meio — um pico submerso
    /// entregaria o jogador boiando, sem chao e sem saida a nao ser o painel.
    ///
    /// Entao isto e' testado pra NOMES DE VERDADE, em todos os niveis de
    /// Tamanho: a chegada tem que cair em terra, e perto.
    #[test]
    fn toda_colonia_tem_chao_onde_se_chega() {
        // Uma semente so': o que varia e' o TAMANHO. E' por isso que este
        // teste virou exaustivo — seis ilhas sao todas as que existem, e
        // antes ele provava seis NOMES de bilhoes possiveis.
        for nome in ["qualquer um"] {
            for nivel in 1..=NIVEL_MAX {
                let ilha = crate::terreno::Ilha::gerar(
                    semente(nome),
                    raio_blocos(nivel),
                    crate::terreno::Bioma::Floresta,
                    crate::terreno::ESCALA_ALTURA,
                );
                let p = ilha.terra_mais_proxima(CHEGADA.x, CHEGADA.y, 400.0);
                assert!(
                    !ilha.agua(p.x, p.y),
                    "'{nome}' nivel {nivel}: chegada na agua em {p:?}"
                );
                assert!(
                    p.distance(CHEGADA) < 300.0,
                    "'{nome}' nivel {nivel}: chao a {:.0}u da chegada",
                    p.distance(CHEGADA)
                );
            }
        }
    }

    /// Subir o assentamento NAO MEXE NA ILHA. A costa e o relevo de fora do
    /// plato sao os mesmos em todos os niveis.
    ///
    /// Antes isto era "o raio nunca encolhe", porque a ilha crescia e encolher
    /// afogaria quem estivesse na beirada. Agora a garantia e' mais forte e o
    /// motivo e' o do dono: "a ideia é não cortar a ilha". Com a mesma semente
    /// e raios diferentes, `Forma::de` escalava o contorno — subir de nivel
    /// nao acrescentava terra, desenhava outra ilha, e a casa do jogador
    /// mudava de lugar embaixo dele.
    ///
    /// Roda sobre as ilhas de VERDADE (`Ilha::da_colonia`), e nao sobre os
    /// numeros: o que precisa ser igual e' o chao, nao a formula.
    /// A LADEIRA NUNCA VIRA PAREDÃO, em nenhuma coluna da ilhota.
    ///
    /// O relevo subiu de 8 para 29 u de desnível porque a ilha estava
    /// "chapada plana e horrível", e ondulação forte é exatamente o que cria
    /// degrau sem ninguém ver. Um bloco (0,5 u) é o que o passo vence; acima
    /// disso é penhasco, e num painel de maquete penhasco lê como buraco.
    ///
    /// Varre a ilhota inteira comparando cada coluna com a vizinha — é o
    /// chão de verdade, não a fórmula.
    #[test]
    fn a_ladeira_da_ilhota_nunca_vira_paredao() {
        let r = RAIO_BLOCOS;
        let mut pior = 0i32;
        let mut onde = (0, 0);
        for bz in -r..r {
            for bx in -r..r {
                let h = bloco_da_coluna(bx, bz);
                // Só compara TERRA com TERRA: a queda pra água é a costa, e
                // costa é para ser íngreme.
                if h <= NIVEL_FUNDO {
                    continue;
                }
                for (dx, dz) in [(1, 0), (0, 1)] {
                    let v = bloco_da_coluna(bx + dx, bz + dz);
                    if v <= NIVEL_FUNDO {
                        continue;
                    }
                    let d = (h - v).abs();
                    if d > pior {
                        pior = d;
                        onde = (bx, bz);
                    }
                }
            }
        }
        println!("pior degrau: {pior} bloco(s), em {onde:?}");
        assert!(
            pior <= 1,
            "degrau de {pior} blocos em {onde:?}: a ilhota ganhou penhasco"
        );
    }

    /// A ILHOTA CABE NUMA OLHADA, e a cidade fica no meio dela.
    ///
    /// Substitui `a_colonia_tem_cais_e_ele_fica_no_lugar`, que morreu com o
    /// que ele guardava: o cais existia porque se ANDAVA na colônia e se saía
    /// dela de barco. Desde que ela virou painel não há caminhada nem saída
    /// física — o dono: "como não vai ter mais como andar na ilha, tem que ser
    /// uma ilhota mesmo, uma praia, quase que dá pra ver tudo em 360, com o
    /// centro sendo a cidade".
    ///
    /// Então o que precisa ser garantido mudou, e é isto: o assentamento
    /// inteiro cabe dentro da ilhota com praia sobrando, em TODO nível.
    #[test]
    fn a_ilhota_cabe_numa_olhada_com_a_cidade_no_meio() {
        for n in 1..=NIVEL_MAX {
            let g = crate::terreno::Gerador::da_colonia(plato_do_assentamento(n));
            let c = g.cidade().expect("sem praça");
            assert_eq!(c.centro(), CHEGADA, "nível {n}: a praça saiu do centro");
            assert!(g.porto().is_none(), "nível {n}: a ilhota não tem cais");

            // O platô MAIS a rampa cabem, com praia sobrando. Sem a folga a
            // cidade encostaria na água e o anel de ofícios ficaria metade no
            // barranco — que foi o que o dono viu quando a ilha era a antiga.
            let fim_da_rampa = plato_do_assentamento(n) + crate::terreno::Cidade::RAMPA;
            let menor_raio = (0..64)
                .map(|k| raio_na_direcao(k as f32 / 64.0 * std::f32::consts::TAU))
                .fold(f32::INFINITY, f32::min);
            assert!(
                menor_raio > fim_da_rampa + 15.0,
                "nível {n}: a rampa acaba a {fim_da_rampa:.0} u e a costa mais \
                 perto está a {menor_raio:.0} u — não sobra praia"
            );
        }

        // E ela CABE NA TELA: a ilhota inteira num raio que a câmera da
        // maquete enquadra. A antiga tinha 140 u de raio e o dono via um
        // pedaço de ilha, não uma ilhota.
        assert!(RAIO < 90.0, "ilhota de {RAIO:.0} u não cabe numa olhada");

        // A COSTA ONDULA: uma moeda perfeita não lê como ilha.
        let raios: Vec<f32> = (0..64)
            .map(|k| raio_na_direcao(k as f32 / 64.0 * std::f32::consts::TAU))
            .collect();
        let (mn, mx) = raios.iter().fold((f32::MAX, 0.0f32), |a, r| (a.0.min(*r), a.1.max(*r)));
        assert!(
            mx - mn > RAIO * 0.12,
            "costa quase circular ({mn:.0}..{mx:.0} u): parece uma moeda"
        );
    }

    #[test]
    fn subir_o_assentamento_nao_mexe_na_ilha() {
        let base = crate::terreno::Ilha::da_colonia(plato_do_assentamento(1));
        for n in 2..=NIVEL_MAX {
            let maior = crate::terreno::Ilha::da_colonia(plato_do_assentamento(n));
            // Fora do plato do nivel MAIOR (mais a rampa), tudo igual.
            let limite = plato_do_assentamento(n) + crate::terreno::Cidade::RAMPA + 2.0;
            let mut conferidos = 0;
            let mut passo = -140.0f32;
            while passo < 140.0 {
                let mut z = -140.0f32;
                while z < 140.0 {
                    let d = glam::Vec2::new(passo, z).distance(glam::Vec2::ZERO);
                    if d > limite {
                        assert_eq!(
                            base.altura(passo, z),
                            maior.altura(passo, z),
                            "nivel {n}: o chao em ({passo}, {z}) mudou — a ilha foi cortada"
                        );
                        assert_eq!(base.agua(passo, z), maior.agua(passo, z), "a costa mudou");
                        conferidos += 1;
                    }
                    z += 7.0;
                }
                passo += 7.0;
            }
            assert!(conferidos > 200, "so' {conferidos} colunas conferidas");
        }
    }
}
