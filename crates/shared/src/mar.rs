//! O MAR ABERTO: a zona entre as ilhas (docs/MAR_ABERTO.md).
//!
//! Desde 21/09/2026 trocar de ilha exige navegar. O teleporte do Capitao
//! acabou, e o mar deixou de ser cenario: e' por onde se vai.
//!
//! # Por que isto nao e' uma `Ilha`
//!
//! `Ilha` e' um quadrado de blocos centrado na origem — `coluna()` soma
//! `raio_blocos` e nao ha' campo de deslocamento. Cobrir as quatro ilhas do
//! `ARQUIPELAGO` pediria raio de ~5.200 blocos: 108 milhoes de colunas, uns
//! 216 MB de cache `.alt` (o teto documentado e' 20 MB) e um `plantar()` em
//! cima de tudo isso a cada boot.
//!
//! # O que e' entao
//!
//! Um COMPOSTO dos quatro `Gerador`es que ja' existem, cada um no seu
//! `DefIlha::centro`. Perguntar a altura de uma coluna aqui e' fazer quatro
//! testes de distancia e delegar pro gerador da ilha que contem o ponto;
//! fora de todas, fundo do mar.
//!
//! E' esse o pulo: **o terreno do mar SAO as ilhas de verdade, nas posicoes
//! de verdade.** Navegar a noroeste saindo do cais do Bosque e chegar no cais
//! real da Geleira nao e' aproximacao — e' o mesmo `Gerador` produzindo as
//! mesmas colunas. Sem cache, sem versao nova de relevo, e o custo de boot
//! sao quatro `Gerador::novo`, que o cliente ja' paga uma vez por login.
//!
//! O preco e' que aqui nao existe `Ilha`: nada de arvore plantada, nada de
//! vila indexada, nada de estorvo. No mar nao ha' o que plantar.

use glam::Vec2;

use crate::terreno::{
    mover_casco, Bioma, DefIlha, Gerador, ARQUIPELAGO, BLOCO, ESCALA_ALTURA, NIVEL_DO_MAR,
};

/// O nome da zona do mar. Nao e' ilha do arquipelago: os indices do
/// `ARQUIPELAGO` sao API (menu de viagem, passo de historia, tabela de mob),
/// e uma quinta entrada quebraria os tres de uma vez.
pub const ZONA: &str = "mar_aberto";

/// Fundo do mar longe de tudo, em blocos. Bem abaixo do nivel do mar: o que
/// importa e' que seja agua, nao a profundidade.
const FUNDO: i32 = -80;

/// Folga em volta do disco de cada ilha, em unidades. O relevo de uma ilha
/// nao para exatamente no raio — ele afunda pro mar perto da borda — entao a
/// consulta tem que continuar indo no gerador um pouco depois do fim.
const MARGEM: f32 = 64.0;

/// Perto disto de um cais o barco pode atracar.
pub const PERTO_DO_CAIS: f32 = 14.0;

/// O mar: os quatro geradores nas suas posicoes.
///
/// As coordenadas daqui sao as do MAR, nao as do `ARQUIPELAGO`: a origem fica
/// no centro da caixa que envolve as quatro ilhas. Isso existe por causa do
/// fio — `EntityState::pos` e' `i16` a `POS_SCALE`, e `quantize` CLAMPA em
/// silencio. Com as coordenadas cruas do arquipelago o Planalto ficaria em
/// x = -3600 e a costa oeste dele passaria do teto, empilhando entidades na
/// borda sem um erro em log nenhum. Centrado, o mar inteiro cabe com folga.
///
/// O `ARQUIPELAGO::centro` nao muda: ele continua sendo o mapa-mundi, e a
/// conversao mora aqui dentro e em nenhum outro lugar.
pub struct Mar {
    /// (centro em coordenada do MAR, raio em unidades, gerador).
    ilhas: Vec<(Vec2, f32, Gerador)>,
    /// Centro da caixa do arquipelago, em coordenada do mapa-mundi. E' o que
    /// se soma pra voltar pro referencial do `ARQUIPELAGO`.
    origem: Vec2,
}

impl Mar {
    /// Monta o mar. Quatro `Gerador::novo` — cada um faz a busca da cidade e
    /// do porto, que e' o unico custo real aqui.
    pub fn novo() -> Self {
        Self::com(&ARQUIPELAGO)
    }

    /// O mesmo, com um arquipelago dado. Existe pros testes poderem montar um
    /// mar pequeno sem esperar os quatro geradores de verdade.
    pub fn com(defs: &[DefIlha]) -> Self {
        let origem = Self::origem_de(defs);
        let ilhas = defs
            .iter()
            .map(|d| {
                (
                    Vec2::from(d.centro) - origem,
                    d.raio_m(),
                    Gerador::da_ilha(d),
                )
            })
            .collect();
        Self { ilhas, origem }
    }

    /// Centro da caixa que envolve as ilhas, contando o raio de cada uma.
    /// Calculado da tabela em vez de escrito a mao: mover uma ilha no
    /// `ARQUIPELAGO` continua dando um mar centrado.
    pub fn origem_de(defs: &[DefIlha]) -> Vec2 {
        let (mut x0, mut x1, mut z0, mut z1) = (f32::MAX, f32::MIN, f32::MAX, f32::MIN);
        for d in defs {
            let r = d.raio_m();
            x0 = x0.min(d.centro[0] - r);
            x1 = x1.max(d.centro[0] + r);
            z0 = z0.min(d.centro[1] - r);
            z1 = z1.max(d.centro[1] + r);
        }
        Vec2::new((x0 + x1) * 0.5, (z0 + z1) * 0.5)
    }

    /// Meia-extensao do mar: a maior distancia da origem ate' uma borda.
    /// E' o numero que tem que caber no orcamento do `POS_SCALE`.
    pub fn meia_extensao(&self) -> f32 {
        self.ilhas
            .iter()
            .map(|(c, r, _)| c.abs().max_element() + r)
            .fold(0.0, f32::max)
    }

    /// Coordenada do mar -> coordenada do mapa-mundi (`ARQUIPELAGO`).
    pub fn para_mapa_mundi(&self, p: Vec2) -> Vec2 {
        p + self.origem
    }

    /// Indice do bloco de topo na coluna, em coordenadas de MUNDO.
    ///
    /// Quatro testes de distancia e uma delegacao. 99% das amostras no mar
    /// aberto param no primeiro `if` e nunca tocam em ruido — e' por isso que
    /// um mapa quase todo de agua sai mais barato que uma ilha, nao mais caro.
    pub fn bloco_em(&self, bx: i32, bz: i32) -> i32 {
        let p = Vec2::new(bx as f32 * BLOCO, bz as f32 * BLOCO);
        for (centro, raio, ger) in &self.ilhas {
            if p.distance(*centro) <= raio + MARGEM {
                let c = *centro / BLOCO;
                return ger.bloco_em(bx - c.x.round() as i32, bz - c.y.round() as i32);
            }
        }
        FUNDO
    }

    /// Altura do chao em unidades de mundo, como `Ilha::altura`.
    pub fn altura(&self, x: f32, z: f32) -> f32 {
        let (bx, bz) = ((x / BLOCO).round() as i32, (z / BLOCO).round() as i32);
        (self.bloco_em(bx, bz) + 1) as f32 * BLOCO
    }

    /// Ha' agua aqui? E' a pergunta que o casco faz a cada passo.
    pub fn agua(&self, x: f32, z: f32) -> bool {
        self.altura(x, z) <= NIVEL_DO_MAR
    }

    /// Move um casco (ver `terreno::mover_casco`). Terra barra.
    pub fn mover_no_mar(&self, pos: Vec2, vel: Vec2, dt: f32, raio: f32) -> Vec2 {
        mover_casco(&|x, z| self.agua(x, z), pos, vel, dt, raio)
    }

    /// Dentro do disco de qual ilha esta' o ponto? `None` = mar aberto.
    pub fn ilha_em(&self, p: Vec2) -> Option<usize> {
        self.ilhas
            .iter()
            .position(|(c, r, _)| p.distance(*c) <= *r)
    }

    /// A ilha mais perto, sempre. E' quem decide pra onde a correnteza leva
    /// um naufrago.
    pub fn ilha_mais_perto(&self, p: Vec2) -> usize {
        let mut melhor = (0usize, f32::MAX);
        for (i, (c, _, _)) in self.ilhas.iter().enumerate() {
            let d = p.distance_squared(*c);
            if d < melhor.1 {
                melhor = (i, d);
            }
        }
        melhor.0
    }

    /// A PONTA DO CAIS da ilha `i`, em coordenadas de mundo.
    ///
    /// E' o ponto de partida e de chegada de toda travessia. `SitioPorto` ja'
    /// garante que a ponta e mais 6 u adiante sao oceano de verdade, e nao
    /// lago — entao todo cais e' alcancavel por casco por construcao.
    pub fn cais_de(&self, i: usize) -> Option<Vec2> {
        let (centro, _, ger) = self.ilhas.get(i)?;
        let p = ger.porto()?;
        Some(*centro + p.raiz + p.mar() * (p.comp + 4.0))
    }

    /// Coordenada do mar -> coordenada LOCAL da ilha `i`.
    ///
    /// Cada zona de ilha simula com a ilha dela centrada na origem, entao
    /// toda travessia passa por esta conversao ao atracar — e pela irma, ao
    /// zarpar.
    pub fn para_local(&self, i: usize, no_mar: Vec2) -> Vec2 {
        no_mar - self.ilhas.get(i).map_or(Vec2::ZERO, |(c, _, _)| *c)
    }

    /// Coordenada LOCAL da ilha `i` -> coordenada do mar.
    pub fn para_mar(&self, i: usize, local: Vec2) -> Vec2 {
        local + self.ilhas.get(i).map_or(Vec2::ZERO, |(c, _, _)| *c)
    }

    /// Quantas ilhas o mar tem.
    pub fn ilhas(&self) -> usize {
        self.ilhas.len()
    }
}

/// A zona `z` e' o mar aberto?
pub fn e_mar(z: &str) -> bool {
    z == ZONA
}

/// Um `DefIlha` de fachada pro mar, pro que le' zona por `def_da_zona`
/// (mapa, construcoes, terreno do cliente). Nao entra no `ARQUIPELAGO`.
pub const DEF: DefIlha = DefIlha {
    zona: ZONA,
    nome: "Mar Aberto",
    semente: 0,
    raio_blocos: 0,
    bioma: Bioma::Floresta,
    centro: [0.0, 0.0],
    nivel: (1, 60),
};

#[cfg(test)]
mod testes {
    use super::*;

    /// Um mar de uma ilha so', pra os testes nao esperarem os quatro
    /// geradores de verdade.
    fn mar_pequeno() -> Mar {
        Mar::com(&[DefIlha {
            zona: "t",
            nome: "T",
            semente: 5,
            raio_blocos: 64,
            bioma: Bioma::Floresta,
            centro: [200.0, -100.0],
            nivel: (1, 10),
        }])
    }

    /// O terreno do mar E' a ilha, deslocada pro `centro` dela. Se isto
    /// quebrar, navegar deixa de chegar no cais de verdade.
    #[test]
    fn a_ilha_aparece_no_centro_que_o_arquipelago_diz() {
        let m = mar_pequeno();
        // Numa ilha so', o centro dela E' a origem do mar — mas o mapa-mundi
        // continua sabendo onde ela fica de verdade.
        let centro = Vec2::ZERO;
        assert_eq!(m.para_mapa_mundi(centro), Vec2::new(200.0, -100.0));
        assert_eq!(m.ilha_em(centro), Some(0));
        assert!(!m.agua(centro.x, centro.y), "o centro da ilha e' terra");

        // Longe do disco e' mar aberto.
        let longe = Vec2::new(400.0, 0.0);
        assert!(m.agua(longe.x, longe.y), "fora do disco tinha que ser agua");
        assert_eq!(m.ilha_em(longe), None);

        // E o relevo bate com o da ilha sozinha, ponto a ponto.
        let sozinha = crate::terreno::Ilha::gerar(5, 64, Bioma::Floresta, ESCALA_ALTURA);
        for (dx, dz) in [(0.0, 0.0), (8.0, -6.0), (-12.0, 3.0)] {
            assert_eq!(
                m.altura(centro.x + dx, centro.y + dz),
                sozinha.altura(dx, dz),
                "relevo divergiu em ({dx}, {dz})"
            );
        }
    }

    /// Ida e volta entre coordenada de mundo e coordenada da ilha. Toda
    /// travessia passa por aqui duas vezes, e um erro de sinal aqui poe o
    /// jogador do outro lado da ilha.
    #[test]
    fn mar_e_local_sao_a_mesma_coisa_em_dois_referenciais() {
        let m = mar_pequeno();
        let local = Vec2::new(-15.0, 40.0);
        let mundo = m.para_mar(0, local);
        // Um mar de uma ilha so' tem a ilha na propria origem.
        assert_eq!(mundo, local);
        assert_eq!(m.para_local(0, mundo), local);
    }

    /// O MAR DE VERDADE tem que caber no `EntityState`.
    ///
    /// Este e' o teste mais importante do arquivo, porque a falha que ele
    /// pega nao aparece em log nenhum: `EntityState::quantize` CLAMPA. Um
    /// mar grande demais nao da' erro — ele empilha, em silencio, toda
    /// entidade alem do teto no mesmo ponto da borda.
    ///
    /// Se ele quebrar, ha' tres saidas, nesta ordem: aproximar as ilhas no
    /// `ARQUIPELAGO`, baixar o `POS_SCALE` de novo, ou aumentar o campo. Nao
    /// ha' saida que seja "ignorar".
    #[test]
    fn o_mar_inteiro_cabe_no_orcamento_do_fio() {
        let meia = Mar::meia_extensao(&Mar::novo());
        let teto = i16::MAX as f32 / crate::components::POS_SCALE;
        assert!(
            meia < teto,
            "o mar tem meia-extensao de {meia:.0} u e o fio so' carrega \
             +-{teto:.0} u — entidade alem disso empilha na borda, calada"
        );
    }

    /// O cais fica na agua, senao nao da' pra atracar nele.
    #[test]
    fn o_cais_da_pra_alcancar_de_barco() {
        let m = mar_pequeno();
        let Some(cais) = m.cais_de(0) else {
            return; // ilha de teste pode nao ter achado costa que sirva
        };
        assert!(
            m.agua(cais.x, cais.y),
            "a ponta do cais caiu em terra, em {cais:?}"
        );
        assert_eq!(m.ilha_mais_perto(cais), 0);
    }
}

// ─────────────────────────── protocolo do barco ───────────────────────────

/// O que o jogador pede ao barco.
///
/// Um pedido so', com variantes, em vez de uma mensagem por verbo — o molde
/// de `loja::PedidoLoja`. O barco apagado em 20/09/2026 tinha ONZE mensagens
/// (`BoardBoat`, `GrabStation`, `SailAdjust`, `AnchorToggle`, `HelmAdjust`,
/// `CannonAim`, ...) e o cliente macroquad nunca implementou uma.
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum PedidoBarco {
    /// No porto: sai pro Mar Aberto. Vira `TrocarZona`.
    Zarpar,
    /// No mar, perto do cais: entra na ilha `ilha` (indice do `ARQUIPELAGO`).
    Atracar { ilha: u8 },
    /// Desce do casco pra terra firme mais perto.
    Desembarcar,
}

/// O que o barco responde.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum AvisoBarco {
    /// O casco do jogador: entidade, vida e se esta' avariado.
    Meu {
        barco: Option<u64>,
        casco: u16,
        casco_max: u16,
        avariado: bool,
    },
    /// Naufragou: pra que porto foi e o que o porao levou.
    Naufragio { porto: String, perdeu: u16 },
    /// Nao deu, e por que. E' o que impede uma recusa silenciosa.
    Recusa(String),
}
