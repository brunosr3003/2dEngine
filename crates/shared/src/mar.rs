//! O MAR ABERTO: a zona entre as ilhas (docs/MAR_ABERTO.md).
//!
//! Desde 21/09/2026 trocar de ilha exige navegar. O teleporte do Capitao
//! acabou, e o mar deixou de ser cenario: e' por onde se vai.
//!
//! # A ilha vista do mar e' uma SILHUETA
//!
//! A primeira versao compunha os quatro `Gerador`es de verdade: o relevo do
//! mar ERA o das ilhas, nas posicoes delas. Funcionava e era bonito no papel,
//! mas o dono viu o que importava: *"nao quero que as ilhas se materializem
//! inteiras no open world; quero uma mega simplificacao da ilha e um porto
//! mostrando onde entra"*.
//!
//! Ele esta' certo por tres motivos de uma vez:
//!
//! - **Leitura.** De longe, o que o navegante precisa e' "ha' terra ali, e se
//!   entra por ali". Arvore, casa e trilha a 800 u de distancia sao ruido.
//! - **Custo.** Ruido de Perlin por coluna, em quatro ilhas, num celular,
//!   pra desenhar o que vira um borrao.
//! - **Honestidade.** O mar nao e' a ilha. Quem quer a ilha, atraca.
//!
//! Entao o terreno daqui e' ANALITICO: um domo por ilha, e um cais marcando a
//! entrada. Sem Perlin, sem cache, sem `plantar`, sem vila. Uma conta de
//! distancia por coluna.
//!
//! Os `Gerador`es ainda nascem uma vez no boot — mas so' pra PERGUNTAR onde
//! fica o porto de cada ilha, que e' o unico dado do mundo real que o mar
//! precisa. Depois disso eles sao descartados.

use glam::Vec2;

use crate::terreno::{
    mover_casco, suave, Bioma, DefIlha, Gerador, ARQUIPELAGO, BLOCO, NIVEL_DO_MAR,
};

/// O nome da zona do mar. Nao e' ilha do arquipelago: os indices do
/// `ARQUIPELAGO` sao API (menu de viagem, passo de historia, tabela de mob),
/// e uma quinta entrada quebraria os tres de uma vez.
pub const ZONA: &str = "mar_aberto";

/// Fundo do mar longe de tudo, em blocos. Bem abaixo do nivel do mar: o que
/// importa e' que seja agua, nao a profundidade.
///
/// **Publico de proposito:** o cliente precisa do MESMO numero. Ele tem o
/// proprio roteador de colunas (`client::terreno::ger_em`), e enquanto este
/// valor morava so' aqui os dois discordavam — o servidor via agua funda e o
/// cliente desenhava um plano marrom de areia por cima do oceano inteiro.
pub const FUNDO: i32 = -80;

/// Folga em volta do disco de cada ilha, em unidades. Tambem publica pelo
/// mesmo motivo: e' onde os dois lados decidem "aqui ainda e' a ilha".
pub const MARGEM: f32 = 64.0;

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
    ilhas: Vec<IlhaVista>,
    /// Centro da caixa do arquipelago, em coordenada do mapa-mundi. E' o que
    /// se soma pra voltar pro referencial do `ARQUIPELAGO`.
    origem: Vec2,
}

/// Uma ilha COMO SE VE DO MAR: um domo e um cais. Nada mais.
pub struct IlhaVista {
    /// Centro, em coordenada do mar.
    pub centro: Vec2,
    /// Raio da ilha, em unidades.
    pub raio: f32,
    /// Raiz do cais na costa e a ponta dele, em coordenada do mar. `None`
    /// quando a ilha nao tem porto.
    pub cais: Option<(Vec2, Vec2)>,
}

/// Altura do domo no centro da ilha, em unidades. Alto o bastante pra virar
/// silhueta no horizonte, baixo o bastante pra nao virar muralha.
const ALTURA_DA_ILHA: f32 = 26.0;
/// Que fracao do raio a encosta ocupa. O resto e' planalto.
const ENCOSTA: f32 = 0.42;
/// Largura do cais, em unidades.
const CAIS_LARG: f32 = 5.0;
/// Quanto o cais avanca mar adentro, a partir da borda do domo.
const CAIS_COMP: f32 = 22.0;
/// Altura do tabuado do cais acima do nivel do mar.
const CAIS_ALTURA: f32 = 1.2;

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
                let centro = Vec2::from(d.centro) - origem;
                // O gerador nasce, responde ONDE FICA O PORTO, e morre. E' o
                // unico dado do mundo real que o mar precisa: e' por ali que
                // se entra na ilha, e o cais daqui tem que casar com o de la'.
                // O cais sai do domo na MESMA DIRECAO do porto de verdade —
                // e' por ali que se entra na ilha, e a silhueta tem que
                // concordar com o que o jogador acha quando atraca.
                //
                // Mas a RAIZ e' na borda do domo, nao na costa de verdade: a
                // costa real e' irregular e fica bem dentro do circulo, entao
                // ancorar nela enterrava o cais inteiro no morro — e ai' nao
                // havia agua no alcance pra encostar.
                let cais = Gerador::da_ilha(d).porto().map(|p| {
                    let rumo = (p.raiz + p.mar() * p.comp).try_normalize().unwrap_or(Vec2::X);
                    let raiz = centro + rumo * (d.raio_m() - 2.0);
                    (raiz, raiz + rumo * CAIS_COMP)
                });
                IlhaVista {
                    centro,
                    raio: d.raio_m(),
                    cais,
                }
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
            .map(|i| i.centro.abs().max_element() + i.raio)
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
        let mut alto = f32::MIN;
        for i in &self.ilhas {
            let d = p.distance(i.centro);
            if d > i.raio + MARGEM {
                continue;
            }
            // O DOMO: planalto no meio, encosta na borda, agua fora.
            let t = ((i.raio - d) / (i.raio * ENCOSTA)).clamp(0.0, 1.0);
            alto = alto.max(suave(t) * ALTURA_DA_ILHA);
            // O CAIS: uma lingua estreita saindo da costa, no nivel do mar.
            // E' ele que diz "entra por aqui" — sem ele a ilha e' uma parede
            // fechada e o jogador contorna procurando porta.
            if let Some((raiz, ponta)) = i.cais {
                let eixo = (ponta - raiz).normalize_or_zero();
                let rel = p - raiz;
                let ao_longo = rel.dot(eixo);
                let de_lado = rel.dot(Vec2::new(-eixo.y, eixo.x)).abs();
                if (-6.0..=(ponta - raiz).length()).contains(&ao_longo)
                    && de_lado <= CAIS_LARG * 0.5
                {
                    alto = alto.max(NIVEL_DO_MAR + CAIS_ALTURA);
                }
            }
        }
        if alto == f32::MIN {
            return FUNDO;
        }
        (alto / BLOCO).round() as i32 - 1
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
            .position(|i| p.distance(i.centro) <= i.raio)
    }

    /// As ilhas como o mar as enxerga. E' o que o cliente desenha.
    pub fn vistas(&self) -> &[IlhaVista] {
        &self.ilhas
    }

    /// A ilha mais perto, sempre. E' quem decide pra onde a correnteza leva
    /// um naufrago.
    pub fn ilha_mais_perto(&self, p: Vec2) -> usize {
        let mut melhor = (0usize, f32::MAX);
        for (i, il) in self.ilhas.iter().enumerate() {
            let d = p.distance_squared(il.centro);
            if d < melhor.1 {
                melhor = (i, d);
            }
        }
        melhor.0
    }

    /// O ANCORADOURO da ilha `i`: a agua logo depois da ponta do cais.
    ///
    /// E' onde o casco aparece ao zarpar e de onde se atraca — entao tem que
    /// ser AGUA, e nao o tabuado. O cais em si e' solido de proposito: ele e'
    /// a porta da ilha, e bater nele e' o que faz o barco parar ali.
    pub fn cais_de(&self, i: usize) -> Option<Vec2> {
        let il = self.ilhas.get(i)?;
        let (raiz, ponta) = il.cais?;
        Some(ponta + (ponta - raiz).normalize_or_zero() * 3.5)
    }

    /// Coordenada do mar -> coordenada LOCAL da ilha `i`.
    ///
    /// Cada zona de ilha simula com a ilha dela centrada na origem, entao
    /// toda travessia passa por esta conversao ao atracar — e pela irma, ao
    /// zarpar.
    pub fn para_local(&self, i: usize, no_mar: Vec2) -> Vec2 {
        no_mar - self.ilhas.get(i).map_or(Vec2::ZERO, |i| i.centro)
    }

    /// Coordenada LOCAL da ilha `i` -> coordenada do mar.
    pub fn para_mar(&self, i: usize, local: Vec2) -> Vec2 {
        local + self.ilhas.get(i).map_or(Vec2::ZERO, |i| i.centro)
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

    /// A ilha vista do mar e' um DOMO no lugar certo — nao o relevo dela.
    ///
    /// Este teste ja' afirmou o contrario: que o relevo do mar batia, coluna
    /// a coluna, com o da ilha sozinha. Era verdade e foi recusado — o dono
    /// nao quer a ilha inteira materializada no mar, quer saber que ha' terra
    /// ali e por onde se entra.
    #[test]
    fn a_ilha_vista_do_mar_e_um_domo_no_lugar_certo() {
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

        // E a silhueta SOBE indo pro centro: e' isso que a faz ler como ilha
        // no horizonte em vez de mancha.
        let raio = m.vistas()[0].raio;
        let borda = m.altura(raio * 0.92, 0.0);
        let meio = m.altura(raio * 0.4, 0.0);
        assert!(
            meio > borda && borda > crate::terreno::NIVEL_DO_MAR,
            "a encosta nao sobe: borda {borda}, meio {meio}"
        );
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
    /// No porto: conserta o casco. `pagando = false` pede o piso de graca.
    Reparar { pagando: bool },
    /// No porto: sobe um eixo de melhoria (`barcos::eixo`).
    Melhorar { eixo: u8 },
    /// Na Capitania: entrega o bau que esta' no conves.
    EntregarBau,
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
    /// O painel do CARPINTEIRO: o casco, o que custa consertar e o que custa
    /// subir cada eixo. Conta pura dos dois lados (`shared::barcos`), entao o
    /// painel desenha exatamente o que o servidor vai cobrar.
    Estaleiro {
        item: u16,
        casco: u16,
        casco_max: u16,
        melhorias: [u8; 4],
        travessias: u32,
        afundou: u32,
        /// (cobre, madeira) do reparo completo.
        reparo: (u32, u32),
        /// Por eixo: (item, qtd) x3 do proximo nivel. Vazio = no maximo.
        custos: Vec<Vec<(u16, u32)>>,
    },
}

// ─────────────────────────────── as rotas ───────────────────────────────

/// Uma travessia: as duas ilhas que ela liga e a faixa de nivel do MAR entre
/// elas (docs/MAR_ABERTO.md).
///
/// A faixa e' do mar, nao das ilhas: a Rota do Bosque liga uma ilha de 1-15 a
/// uma de 15-30, e o mar entre elas e' 12-20. Quem atravessa cedo demais nao
/// e' barrado — a historia ja' barra —, mas encontra bicho acima do nivel.
pub struct Rota {
    pub de: u8,
    pub para: u8,
    pub nome: &'static str,
    pub nivel: (u32, u32),
    /// O casco que se espera ter aqui: (item do casco, melhoria).
    ///
    /// Nao e' TRAVA — a trava de progressao e' a historia, decisao do dono.
    /// E' a referencia do balanceamento: `metas_do_mar` pergunta se ESTE
    /// casco aguenta a travessia, e o painel do porto avisa quem esta' abaixo
    /// dela.
    pub casco: (u16, u8),
}

pub const ROTAS: [Rota; 4] = [
    Rota {
        de: 0,
        para: 1,
        nome: "Rota do Bosque",
        nivel: (12, 20),
        casco: (crate::item_id::BARCO_BASE, 0),
    },
    Rota {
        de: 0,
        para: 2,
        nome: "Rota das Areias",
        nivel: (24, 32),
        casco: (crate::item_id::BARCO_BASE, 3),
    },
    Rota {
        de: 1,
        para: 3,
        nome: "Mar Fundo",
        nivel: (34, 44),
        casco: (crate::item_id::BARCO_ESCUNA, 0),
    },
    Rota {
        de: 2,
        para: 3,
        nome: "Olho da Tempestade",
        nivel: (46, 58),
        casco: (crate::item_id::BARCO_ESCUNA, 4),
    },
];

impl Mar {
    /// Pontos espacados ao longo de uma rota, em agua. E' onde nascem os
    /// bichos e os naufragios.
    ///
    /// `n` pontos entre os dois ancoradouros, pulando o que cair em terra —
    /// uma rota pode raspar a borda de uma ilha, e bicho de mar em cima do
    /// morro seria o mesmo defeito que desligou a pesca por um mes.
    pub fn pontos_da_rota(&self, r: &Rota, n: usize) -> Vec<Vec2> {
        let (Some(a), Some(b)) = (self.cais_de(r.de as usize), self.cais_de(r.para as usize))
        else {
            return Vec::new();
        };
        (1..=n)
            .map(|k| a.lerp(b, k as f32 / (n + 1) as f32))
            .filter(|p| self.agua(p.x, p.y))
            .collect()
    }
}

// ─────────────────────── o tesouro e a marca de PK ───────────────────────

/// Quanto tempo a marca de PK SOBREVIVE a entrega do bau, em segundos.
///
/// Sem esse rastro, descarregar um segundo antes do golpe seria um drible: o
/// carregador chega na Capitania, entrega, e fica intocavel no mesmo quadro
/// em que o perseguidor ia acertar. Com ele, quem correu a travessia inteira
/// atras de alguem ainda tem uma janela.
pub const RASTRO_DA_MARCA_S: i64 = 60;

/// O multiplicador do bau por quantas travessias ele ja' fez.
///
/// Abrir onde caiu paga `1,0` — quem nao quer PvP nenhum abre ali e leva um
/// premio justo de chefe. O resto e' escolha: cada travessia multiplica, e
/// cada travessia e' uma chance de perder tudo.
pub fn multiplicador_da_carga(travessias: u32) -> f32 {
    match travessias {
        0 => 1.0,
        1 => 1.8,
        2 => 2.6,
        _ => 3.4,
    }
}

/// Quantas travessias separam duas ilhas, pelo grafo das rotas.
pub fn saltos_entre(de: u8, para: u8) -> u32 {
    if de == para {
        return 0;
    }
    // Quatro ilhas e quatro rotas: uma busca em largura de tres linhas custa
    // menos que uma tabela escrita a mao, e nao envelhece se uma rota mudar.
    let mut dist = [u32::MAX; 4];
    let mut fila = std::collections::VecDeque::new();
    if (de as usize) < 4 {
        dist[de as usize] = 0;
        fila.push_back(de);
    }
    while let Some(i) = fila.pop_front() {
        for r in ROTAS.iter() {
            for (a, b) in [(r.de, r.para), (r.para, r.de)] {
                if a == i && (b as usize) < 4 && dist[b as usize] == u32::MAX {
                    dist[b as usize] = dist[i as usize] + 1;
                    fila.push_back(b);
                }
            }
        }
    }
    dist.get(para as usize).copied().unwrap_or(0).min(3)
}

#[cfg(test)]
mod testes_do_tesouro {
    use super::*;

    /// A distancia paga, e o grafo sai das ROTAS e nao de uma tabela.
    #[test]
    fn a_travessia_e_o_que_multiplica() {
        // Vizinhas: um salto.
        assert_eq!(saltos_entre(0, 1), 1);
        assert_eq!(saltos_entre(1, 0), 1);
        // Bosque e Planalto sao os extremos: dois saltos por qualquer lado.
        assert_eq!(saltos_entre(0, 3), 2);
        // Abrir onde caiu nao multiplica.
        assert_eq!(saltos_entre(2, 2), 0);
        assert_eq!(multiplicador_da_carga(0), 1.0);
        assert!(multiplicador_da_carga(1) > multiplicador_da_carga(0));
        assert!(multiplicador_da_carga(3) > multiplicador_da_carga(2));
    }
}

// ─────────────────────── as zonas de open PvP ───────────────────────

/// Raio de uma zona sem lei, em unidades.
pub const ZONA_SEM_LEI_RAIO: f32 = 70.0;

impl Mar {
    /// Este ponto esta' numa ZONA SEM LEI, onde qualquer um ataca qualquer
    /// um sem karma?
    ///
    /// Sao POUCAS e com motivo pra parar nelas (docs/MAR_ABERTO.md). A regra
    /// do dono e' que o mar em geral NAO e' open PvP — o que marca alguem e'
    /// carregar tesouro. Zona sem lei e' a excecao, e excecao que cobre tudo
    /// deixa de ser excecao.
    ///
    /// Hoje sao duas famílias:
    ///
    /// - **os naufragios de cada rota**, que e' onde carregador e cacador se
    ///   encontram de qualquer jeito, porque os dois querem a mesma coisa;
    /// - **a rota de endgame inteira** (a ultima de `ROTAS`), que e' o "mar
    ///   amaldicoado" que `docs/GAMEPLAY.md` sempre imaginou — a travessia
    ///   mais rica e' tambem a unica onde nao ha' regra.
    ///
    /// As outras tres travessias seguem governadas por karma. Elas sao
    /// OBRIGATORIAS pra progredir, e transformar o caminho obrigatorio em
    /// terra de ninguem e' cobrar imposto por existir.
    pub fn sem_lei(&self, p: Vec2) -> bool {
        if let Some(r) = ROTAS.last() {
            if let (Some(a), Some(b)) = (self.cais_de(r.de as usize), self.cais_de(r.para as usize))
            {
                // Perto do segmento da rota de endgame.
                let ab = b - a;
                let t = if ab.length_squared() > 1e-3 {
                    ((p - a).dot(ab) / ab.length_squared()).clamp(0.0, 1.0)
                } else {
                    0.0
                };
                if (a + ab * t).distance(p) <= ZONA_SEM_LEI_RAIO {
                    return true;
                }
            }
        }
        // Os naufragios das outras rotas.
        for r in ROTAS.iter() {
            for (k, c) in self.pontos_da_rota(r, 7).into_iter().enumerate() {
                if k % 3 == 1 && c.distance(p) <= ZONA_SEM_LEI_RAIO {
                    return true;
                }
            }
        }
        false
    }
}
