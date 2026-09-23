//! A ILHA MÁGICA: o evento de bônus, com passe e relógio.
//!
//! Pedida pelo dono em 22/09/2026:
//!
//! > "criar a ilha mágica, evento com passe para entrar, permite 3 entradas de
//! > meia hora cada podendo ser usadas de uma vez para somar 1 hora e meia, e
//! > lá terão várias ilhotas conectadas por pontes onde cada um vai dar um
//! > bônus: bônus de xp, bônus de drop de item de mob, bônus de gold, e também
//! > uma com bônus de drop de item de Boss, e também uma com cada tipo de
//! > recurso coletável com bônus de coleta dele. Lá será open pvp mas morrer
//! > volta pra entrada da própria ilha."
//!
//! ## Por que ilhotas, e não zonas de bônus num chão só
//!
//! O bônus tem que ser uma ESCOLHA que custa alguma coisa. Numa ilha inteira
//! com manchas, o jogador anda de mancha em mancha sem pensar; com ilhotas
//! separadas por ponte, ir do ouro pro XP é uma travessia — e é a travessia
//! que faz ele decidir onde vale ficar com o relógio correndo.
//!
//! É também o que dá lugar ao PvP: ponte é gargalo, e gargalo é onde duas
//! pessoas que querem a mesma ilhota se encontram.
//!
//! ## O relógio
//!
//! Três entradas de meia hora, ACUMULÁVEIS: usar as três de uma vez dá 1h30
//! corridos. O passe some ao entrar; o tempo corre mesmo deslogado — senão o
//! jogador desloga no lugar bom e volta amanhã, e o evento deixa de ter hora.
//!
//! ## O relevo
//!
//! As ilhotas e as pontes entram no `Gerador` (`bloco_em`), pelo mesmo gancho
//! que a cidade usa pra aplainar a praça. É isso que faz o cliente desenhar o
//! chão em que o servidor anda: os dois chamam a mesma função. Uma ilha
//! "pós-processada" no servidor daria dois mundos diferentes.

use glam::Vec2;

/// O nome da zona.
pub const ZONA: &str = "ilha_magica";

/// Esta zona é a Ilha Mágica?
pub fn e_magica(z: &str) -> bool {
    z == ZONA
}

// ─────────────────────────── o passe e o relógio ───────────────────────────

/// Quanto dura UMA entrada, em segundos.
pub const DURACAO_S: i64 = 30 * 60;
/// Quantas entradas cabem numa sessão — as três de uma vez dão 1h30.
pub const ENTRADAS_MAX: u8 = 3;

// ─────────────────────────── as entradas de graça ───────────────────────────

/// Entradas DE GRAÇA por dia, decididas pelo dono em 22/09/2026: "a Ilha
/// Mágica terá 3 passes por dia de 30 min grátis".
///
/// São separadas do ITEM `PASSE_MAGICO` de propósito. O item cai de chefe, se
/// compra e se vende; a cota diária não — ela não entra na bolsa, não vai pro
/// mercado e não acumula de um dia pro outro. Fossem itens entregues todo
/// dia, três por dia por personagem virariam moeda, e a economia do passe
/// deixaria de existir.
pub const GRATIS_POR_DIA: u8 = 3;

/// A cota diária cabe num inteiro só: `dia * 16 + usadas`.
///
/// Uma coluna em vez de duas. `usadas` nunca passa de `GRATIS_POR_DIA`, então
/// quatro bits sobram com folga, e o dia é o mesmo `dungeon::dia` do resto do
/// jogo (vira às 04:00 de Brasília). Empacotar é o tipo de esperteza que
/// morde, então ela mora aqui, nestas duas funções, com teste.
pub fn empacota_gratis(dia: i64, usadas: u8) -> i64 {
    dia * 16 + usadas.min(15) as i64
}

/// Quantas entradas de graça ainda há HOJE, a partir do valor guardado.
///
/// Dia diferente do salvo = cota cheia: o reset não precisa de tarefa
/// nenhuma, ele acontece ao perguntar.
pub fn gratis_restantes(guardado: i64, agora_unix: i64) -> u8 {
    let hoje = crate::dungeon::dia(agora_unix);
    if guardado.div_euclid(16) != hoje {
        return GRATIS_POR_DIA;
    }
    GRATIS_POR_DIA.saturating_sub(guardado.rem_euclid(16) as u8)
}

/// O novo valor guardado depois de gastar `n` entradas de graça.
pub fn apos_gastar_gratis(guardado: i64, agora_unix: i64, n: u8) -> i64 {
    let hoje = crate::dungeon::dia(agora_unix);
    let usadas = if guardado.div_euclid(16) == hoje {
        guardado.rem_euclid(16) as u8
    } else {
        0
    };
    empacota_gratis(hoje, usadas.saturating_add(n))
}
/// O teto do relógio, em segundos.
pub const TETO_S: i64 = DURACAO_S * ENTRADAS_MAX as i64;

/// Quanto tempo resta (segundos) de uma sessão que acaba em `fim_unix`.
pub fn resta(fim_unix: i64, agora_unix: i64) -> i64 {
    (fim_unix - agora_unix).max(0)
}

/// O novo fim de sessão ao gastar `n` passes agora.
///
/// Soma ao que já resta (as entradas acumulam) e corta no teto: gastar o
/// quarto passe com 1h30 no relógio seria jogar um passe fora sem avisar, e é
/// por isso que `pode_entrar` recusa antes.
pub fn fim_apos_entrar(fim_atual: i64, agora_unix: i64, n: u8) -> i64 {
    let base = fim_atual.max(agora_unix);
    let novo = base + DURACAO_S * n as i64;
    novo.min(agora_unix + TETO_S)
}

/// Dá pra gastar `n` passes agora? `Err` diz por quê — o jogador precisa saber
/// se é falta de passe ou excesso de tempo.
pub fn pode_entrar(fim_atual: i64, agora_unix: i64, passes: u32, n: u8) -> Result<(), String> {
    if n == 0 || n > ENTRADAS_MAX {
        return Err("entre 1 e 3 entradas".into());
    }
    if (passes as u8) < n {
        return Err(format!("faltam passes: você tem {passes}"));
    }
    let resta_agora = resta(fim_atual, agora_unix);
    if resta_agora + DURACAO_S * n as i64 > TETO_S + DURACAO_S / 2 {
        return Err("seu tempo já está perto do teto de 1h30".into());
    }
    Ok(())
}

// ─────────────────────────── as ilhotas ───────────────────────────

/// O que uma ilhota dá.
///
/// Uma por bônus, e nenhuma com dois: a ilhota É o bônus, e é isso que faz a
/// escolha ser legível de longe — o jogador olha o mapa e sabe onde ficar.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Bonus {
    /// XP de abate.
    Xp,
    /// Chance de drop de item de MOB comum.
    DropDeMob,
    /// Ouro (cobre) de abate.
    Ouro,
    /// Chance de drop de item de CHEFE. A mais disputada, e de propósito no
    /// meio: quem quer o melhor bônus paga passando por todo mundo.
    DropDeChefe,
    /// Coleta de um tipo de recurso, na MESMA codificação que o resto do
    /// jogo usa em `Coletavel::tier`: 0 tronco, 1..4 pedra pela cor, 5
    /// Energia. Inventar uma segunda numeração aqui daria uma ilhota que
    /// promete pedra e conta árvore — que foi o primeiro erro deste arquivo.
    Coleta(u8),
}

impl Bonus {
    pub fn nome(self) -> &'static str {
        match self {
            Self::Xp => "Ilhota da Experiência",
            Self::DropDeMob => "Ilhota do Espólio",
            Self::Ouro => "Ilhota do Ouro",
            Self::DropDeChefe => "Ilhota do Colosso",
            Self::Coleta(0) => "Ilhota da Madeira",
            Self::Coleta(5) => "Ilhota da Energia",
            Self::Coleta(_) => "Ilhota da Pedra",
        }
    }

    /// O multiplicador que ela aplica. Não é o mesmo pra todos: o drop de
    /// chefe é o mais raro do jogo e dobrar já é muito, enquanto XP em dobro
    /// numa hora e meia não quebra nada.
    pub fn multiplicador(self) -> f32 {
        match self {
            Self::Xp => 2.0,
            Self::DropDeMob => 1.75,
            Self::Ouro => 2.0,
            Self::DropDeChefe => 2.0,
            Self::Coleta(_) => 2.0,
        }
    }
}

/// Uma ilhota: onde fica, que tamanho tem, o que dá.
#[derive(Debug, Clone, Copy)]
pub struct Ilhota {
    pub centro: Vec2,
    pub raio: f32,
    pub bonus: Bonus,
}

/// Raio da ilhota comum, em unidades de mundo.
pub const RAIO_ILHOTA: f32 = 46.0;
/// A do meio é maior: ela recebe a chegada e o trânsito de todo mundo.
pub const RAIO_CENTRO: f32 = 58.0;
/// Quão longe do centro ficam as ilhotas de fora.
pub const ANEL: f32 = 150.0;
/// Altura da ORLA da ilhota — e das pontes —, em unidades de mundo. Acima do
/// mar com folga: ilhota rasa vira banco de areia e a ponte perde o sentido.
pub const ALTURA: f32 = 8.0;
/// Altura do MEIO da ilhota.
///
/// Ela é um domo, e não um disco, por um motivo que não é estético: o jogo
/// decide onde nasce cada recurso pela ALTURA (`recurso_montanha_da_coluna`,
/// contra `Gerador::pico`, 37,8 u nesta ilha). Num disco a 6 u nasciam só
/// árvores — medido: 634 coletáveis nas sete ilhotas, **todos** tronco, e as
/// ilhotas da Pedra e da Energia prometiam o que não tinham.
///
/// Com o domo o corte cai onde se espera: praia com mato na beira, Energia da
/// meia encosta pra cima (limiar 6,05 u) e pedra no alto (15,9 u). É o mesmo
/// zoneamento do mundo normal, que é o ponto — a ilhota não inventa regra de
/// recurso, ela dobra o rendimento da que já existe.
pub const ALTURA_TOPO: f32 = 20.0;
/// Meia largura da ponte. Estreita de propósito: ponte é gargalo, e gargalo é
/// onde o PvP acontece.
pub const MEIA_PONTE: f32 = 3.0;

/// Onde o jogador chega — o centro da ilhota do meio.
pub const CHEGADA: Vec2 = Vec2::ZERO;

/// As ilhotas, em ordem: a do meio e depois o anel, no sentido do relógio.
///
/// A do CHEFE fica no meio de propósito: é o bônus mais cobiçado, e pôr o
/// melhor prêmio no cruzamento de todas as pontes é o que faz a ilha ter um
/// lugar pelo qual brigar.
pub fn ilhotas() -> Vec<Ilhota> {
    // A ORDEM NÃO É ARBITRÁRIA na casa da Energia.
    //
    // Energia não nasce em qualquer lugar alto: ela exige cair dentro de um
    // CAMPO (`no_campo_de_energia`), e a grade de campos é função pura da
    // coordenada de bloco — não olha a semente, então não há como "sortear"
    // um campo para a ilhota. Medido: das seis posições do anel, só a de
    // 60° (75, 130) e a de 180° (-150, 0) pegam campo; nas outras quatro a
    // cobertura é ZERO, e a Ilhota da Energia prometeria o que não tem.
    //
    // Por isso a Energia mora em 60°. Quem mexer no `ANEL`, no número de
    // ilhotas ou nesta ordem vai ser reprovado por
    // `toda_ilhota_de_recurso_tem_o_recurso`, que é exatamente o ponto: o
    // acoplamento é real, e um teste é melhor lugar para ele do que a
    // memória de quem escreveu.
    let volta = [
        Bonus::Xp,
        Bonus::Coleta(5),
        Bonus::Ouro,
        Bonus::Coleta(0),
        Bonus::Coleta(1),
        Bonus::DropDeMob,
    ];
    let mut v = vec![Ilhota {
        centro: Vec2::ZERO,
        raio: RAIO_CENTRO,
        bonus: Bonus::DropDeChefe,
    }];
    for (i, b) in volta.iter().enumerate() {
        let a = i as f32 / volta.len() as f32 * std::f32::consts::TAU;
        v.push(Ilhota {
            centro: Vec2::new(a.cos() * ANEL, a.sin() * ANEL),
            raio: RAIO_ILHOTA,
            bonus: *b,
        });
    }
    v
}

/// As pontes: cada ilhota do anel liga ao CENTRO, e o anel se fecha entre
/// vizinhas.
///
/// As duas coisas juntas importam: só os raios fariam todo caminho passar pelo
/// meio (e o meio é a ilhota mais disputada — ninguém atravessaria); só o anel
/// deixaria o centro ilhado.
pub fn pontes() -> Vec<(Vec2, Vec2)> {
    let v = ilhotas();
    let mut p = Vec::new();
    for i in 1..v.len() {
        p.push((v[0].centro, v[i].centro));
        let prox = if i + 1 < v.len() { i + 1 } else { 1 };
        p.push((v[i].centro, v[prox].centro));
    }
    p
}

/// Em que ilhota está o ponto, se estiver em alguma.
pub fn ilhota_em(p: Vec2) -> Option<Ilhota> {
    ilhotas()
        .into_iter()
        .find(|i| i.centro.distance(p) <= i.raio)
}

/// O bônus que vale NESTE ponto. Na ponte não vale bônus nenhum — quem está
/// atravessando está entre dois lugares, e não nos dois.
pub fn bonus_em(p: Vec2) -> Option<Bonus> {
    ilhota_em(p).map(|i| i.bonus)
}

/// A distância do ponto ao segmento de ponte mais próximo.
fn dist_da_ponte(p: Vec2) -> f32 {
    pontes()
        .into_iter()
        .map(|(a, b)| {
            let ab = b - a;
            let t = ((p - a).dot(ab) / ab.length_squared()).clamp(0.0, 1.0);
            (a + ab * t).distance(p)
        })
        .fold(f32::INFINITY, f32::min)
}

/// Este ponto está NA PONTE (e não numa ilhota)?
///
/// Existe pra a vegetação saber onde não nascer. A ponte é estreita de
/// propósito — ela é o gargalo que dá sentido ao PvP —, e uma árvore ou uma
/// pedra no meio dela a fecha: o A* não acha passagem e só dá pra atravessar
/// andando na mão. O dono: "algumas pontes estão com árvores e pedras no
/// meio, aí não dá pra passar usando A*, só andando".
pub fn na_ponte(p: Vec2) -> bool {
    ilhota_em(p).is_none() && dist_da_ponte(p) <= MEIA_PONTE
}

/// Este ponto é CHÃO (ilhota ou ponte)?
///
/// É a mesma pergunta que o gerador responde em `bloco_em` — e a única, pra
/// cliente e servidor não discordarem sobre onde dá pra pisar.
pub fn e_chao(p: Vec2) -> bool {
    ilhota_em(p).is_some() || dist_da_ponte(p) <= MEIA_PONTE
}

/// O raio que a zona precisa ter, em unidades: o anel mais uma ilhota mais
/// folga de mar em volta.
pub fn raio_do_mundo() -> f32 {
    ANEL + RAIO_ILHOTA + 60.0
}

/// A zona, na forma que o resto do jogo ja' sabe ler.
///
/// Ela NAO esta' no `ARQUIPELAGO` — a historia, os niveis das ilhas e o
/// mapa-mundi sao daquela tabela, e a Ilha Magica nao e' um degrau de
/// progressao. Mas `def_da_zona` a devolve, e e' isso que faz o cliente
/// montar o terreno, o mapa e as construcoes dela sem uma linha de codigo
/// nova: todos esses caminhos ja' perguntam `def_da_zona`.
pub const DEF: crate::terreno::DefIlha = crate::terreno::DefIlha {
    zona: ZONA,
    nome: "Ilha Mágica",
    semente: SEMENTE,
    raio_blocos: RAIO_BLOCOS,
    bioma: crate::terreno::Bioma::Floresta,
    // No mapa-mundi, ao norte do arquipelago: longe o bastante pra nao se
    // confundir com ilha de progressao.
    centro: [-1800.0, -2600.0],
    // Aberta a todo nivel: o passe e' o requisito, nao o nivel.
    nivel: (1, 60),
};

// ─────────────────────────── o relevo ───────────────────────────

/// Raio da zona, em BLOCOS. Cobre o anel inteiro com mar de sobra em volta.
pub const RAIO_BLOCOS: i32 = 560;

/// Semente do plantio. O relevo NÃO depende dela (é desenhado, não sorteado);
/// quem depende é a vegetação, e é por isso que ela foi escolhida por teste:
/// `toda_ilhota_de_recurso_tem_o_recurso` reprova uma semente que deixe a
/// Ilhota da Pedra sem pedra.
pub const SEMENTE: i32 = 0x3A61_C0DE;

/// Bloco de topo da ORLA e das pontes.
pub const NIVEL_CHAO: i32 = (ALTURA / crate::terreno::BLOCO) as i32 - 1;
/// O fundo do mar em volta. Fundo raso viraria banco de areia caminhável e o
/// arquipélago deixaria de ser arquipélago.
pub const NIVEL_FUNDO: i32 = -8;

/// Fração do raio que é TOPO PLANO.
///
/// O platô não é enfeite: é onde a pedra e a Energia conseguem nascer. Os
/// dois exigem o quadrado em volta na MESMA altura (`recurso_montanha_da_
/// coluna`, "chão limpo"), e numa ladeira contínua quase nenhuma coluna
/// passa — com o domo puro a Ilhota da Energia tinha 5 veios.
pub const TOPO_PLANO: f32 = 0.35;

/// A altura da ilhota a `d` unidades do centro, para um raio `raio`.
///
/// Platô no meio, ladeira quadrática até a orla — o perfil de uma mesa
/// cercada de praia.
///
/// **A inclinação da orla é o que limita os números.** O passo vence um
/// bloco (0,5 u) por coluna, ou seja 1,0 u/u; acima disso a beira vira
/// paredão e a ilhota deixa de ser caminhável. Com estes valores o pior
/// ponto dá 2·(20−8)/(0,65·46) = **0,80 u/u**, com folga. Mexer em
/// `ALTURA_TOPO`, `ALTURA`, `TOPO_PLANO` ou `RAIO_ILHOTA` mexe nessa conta,
/// e `da_chegada_se_anda_ate_toda_ilhota` é quem reprova.
fn altura_do_domo(d: f32, raio: f32) -> f32 {
    let t = (d / raio).clamp(0.0, 1.0);
    if t <= TOPO_PLANO {
        return ALTURA_TOPO;
    }
    let k = (t - TOPO_PLANO) / (1.0 - TOPO_PLANO);
    ALTURA_TOPO + (ALTURA - ALTURA_TOPO) * k * k
}

/// O CAMPO de Energia desta ilha: a Ilhota da Energia, e só ela.
///
/// A grade global (`terreno::no_campo_de_energia`) é função pura da
/// coordenada de bloco, com células de 300 blocos e campos de 80 — feita para
/// ilhas de 1,6 km. Nesta, de 280 u, ela cai onde cai: medido, o único campo
/// que toca uma ilhota pega a BEIRA dela, onde o domo é baixo e inclinado, e
/// ali a Energia não nasce nem por altura nem por chão limpo. O resultado era
/// uma Ilhota da Energia com zero Energia.
///
/// A Ilha Mágica já declara o próprio relevo em vez de sorteá-lo; declarar o
/// próprio campo é a mesma frase. A margem tira a orla: veio na beira nasce
/// na ladeira e metade dele fica no ar.
pub fn no_campo_de_energia(bx: i32, bz: i32) -> bool {
    let p = Vec2::new(bx as f32, bz as f32) * crate::terreno::BLOCO;
    ilhotas()
        .into_iter()
        .any(|i| i.bonus == Bonus::Coleta(5) && i.centro.distance(p) <= i.raio - 10.0)
}

/// O bloco de topo da coluna `(bx, bz)`, em índice de bloco.
///
/// É a ÚNICA fonte do relevo da Ilha Mágica: nenhum Perlin, nenhuma forma
/// sorteada — o chão É o desenho de `ilhotas()` e `pontes()`. Cliente e
/// servidor chamam esta função, e por isso não têm como discordar de onde dá
/// pra pisar.
pub fn bloco_da_coluna(bx: i32, bz: i32) -> i32 {
    let p = Vec2::new(bx as f32, bz as f32) * crate::terreno::BLOCO;
    // A ILHOTA PRIMEIRO. As pontes ligam CENTRO a CENTRO, então cada uma
    // atravessa as duas ilhotas que liga de ponta a ponta: testando a ponte
    // antes, ela abriria uma vala plana de 6 u cortando os dois domos ao
    // meio. Fora das ilhotas o segmento é o que sobra, e é lá que ele vira
    // ponte.
    if let Some(i) = ilhota_em(p) {
        let h = altura_do_domo(i.centro.distance(p), i.raio);
        return (h / crate::terreno::BLOCO).round() as i32 - 1;
    }
    if dist_da_ponte(p) <= MEIA_PONTE {
        return NIVEL_CHAO;
    }
    NIVEL_FUNDO
}

// ─────────────────────────── o que vai no fio ───────────────────────────

/// O que o cliente pede sobre a Ilha Mágica.
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum PedidoMagica {
    /// Abrir o painel: quantos passes tenho, quanto tempo resta.
    Painel,
    /// Gastar `entradas` passes e ir. Recusa não gasta nada.
    Entrar { entradas: u8 },
    /// Sair antes da hora. O tempo CONTINUA correndo — senão o jogador sairia
    /// no primeiro susto e voltaria com o relógio intacto, e a ilha deixaria
    /// de ter hora.
    Sair,
}

/// O que o servidor conta sobre a Ilha Mágica.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum AvisoMagica {
    Estado {
        /// Passes na bolsa.
        passes: u32,
        /// Entradas de graça que ainda há hoje (`GRATIS_POR_DIA`).
        gratis: u8,
        /// Quando a sessão acaba (unix, segundos). 0 = não está valendo.
        fim_unix: i64,
        /// Está dentro da ilha agora?
        dentro: bool,
        /// O bônus do chão onde ele está (`Bonus::indice`), se estiver em
        /// alguma ilhota. 255 = ponte ou fora.
        bonus: u8,
    },
    Recusa(String),
}

impl Bonus {
    /// O índice no fio. Um `u8` em vez do enum inteiro porque ele viaja em
    /// `Estado`, que vai a cada mudança de ilhota.
    pub fn indice(self) -> u8 {
        match self {
            Self::Xp => 0,
            Self::DropDeMob => 1,
            Self::Ouro => 2,
            Self::DropDeChefe => 3,
            Self::Coleta(t) => 10 + t,
        }
    }

    pub fn do_indice(i: u8) -> Option<Self> {
        Some(match i {
            0 => Self::Xp,
            1 => Self::DropDeMob,
            2 => Self::Ouro,
            3 => Self::DropDeChefe,
            10..=15 => Self::Coleta(i - 10),
            _ => return None,
        })
    }
}

/// O bônus que vale neste ponto, no formato do fio. 255 = nenhum.
pub fn indice_do_bonus_em(p: Vec2) -> u8 {
    bonus_em(p).map_or(255, |b| b.indice())
}

#[cfg(test)]
mod testes {
    use super::*;

    /// NADA nasce na ponte: nem árvore, nem pedra.
    ///
    /// A ponte é estreita de propósito — é o gargalo que dá sentido ao PvP —
    /// e um tronco no meio dela a fecha: o A* deixa de achar passagem e só dá
    /// pra atravessar andando na mão. O dono: "algumas pontes estão com
    /// árvores e pedras no meio, aí não dá pra passar usando A*".
    #[test]
    fn nada_nasce_na_ponte() {
        let ger = crate::terreno::Gerador::da_ilha_magica();
        let mut pontos_de_ponte = 0;
        let mut com_estorvo = 0;
        // Varre cada ponte ponto a ponto, no MEIO dela.
        for (a, b) in pontes() {
            let n = 60;
            let eixo = (b - a).normalize_or_zero();
            let lado = Vec2::new(-eixo.y, eixo.x);
            for k in 0..=n {
                let t = k as f32 / n as f32;
                // A LARGURA INTEIRA, e não só o eixo: um tronco na beira
                // fecha a ponte igual — ela tem MEIA_PONTE de cada lado, e o
                // corpo do jogador ocupa quase isso.
                for w in [-0.8f32, -0.4, 0.0, 0.4, 0.8] {
                    let p = a + (b - a) * t + lado * (w * MEIA_PONTE);
                    if !na_ponte(p) {
                        continue;
                    }
                pontos_de_ponte += 1;
                let bx = (p.x / crate::terreno::BLOCO).round() as i32;
                let bz = (p.y / crate::terreno::BLOCO).round() as i32;
                assert!(
                    ger.na_ponte_magica(bx, bz),
                    "a coluna {bx},{bz} está na ponte mas o gerador não sabe"
                );
                let topo = ger.bloco_em(bx, bz);
                let arv = crate::terreno::arvore_da_coluna(
                    crate::terreno::Bioma::Floresta, bx, bz, topo, 0, &ger, false,
                );
                let pl = crate::terreno::planta_da_coluna(
                    crate::terreno::Bioma::Floresta, bx, bz, topo, 0, &ger, false,
                );
                if arv.is_some() || pl.is_some() {
                    com_estorvo += 1;
                }
                }
            }
        }
        assert!(pontos_de_ponte > 50, "varreu pouca ponte: {pontos_de_ponte} pontos");
        assert_eq!(com_estorvo, 0, "{com_estorvo} pontos de ponte com estorvo");
    }

    /// E a ilhota continua tendo vegetação — o corte é só na ponte.
    #[test]
    fn a_ilhota_continua_com_mato() {
        let ger = crate::terreno::Gerador::da_ilha_magica();
        let mut achou = 0;
        for i in ilhotas() {
            for k in 0..400 {
                let a = k as f32 * 0.7;
                let p = i.centro + Vec2::new(a.cos(), a.sin()) * (RAIO_ILHOTA * 0.5);
                let bx = (p.x / crate::terreno::BLOCO).round() as i32;
                let bz = (p.y / crate::terreno::BLOCO).round() as i32;
                let topo = ger.bloco_em(bx, bz);
                if crate::terreno::arvore_da_coluna(
                    crate::terreno::Bioma::Floresta, bx, bz, topo, 0, &ger, false,
                )
                .is_some()
                {
                    achou += 1;
                }
            }
        }
        assert!(achou > 0, "a ilhota ficou pelada: o corte pegou mais que a ponte");
    }

    #[test]
    fn o_relogio_acumula_ate_uma_hora_e_meia() {
        let agora = 1_000_000i64;
        // Uma entrada: meia hora.
        let fim = fim_apos_entrar(0, agora, 1);
        assert_eq!(resta(fim, agora), DURACAO_S);
        // Mais duas, somadas: uma hora e meia.
        let fim = fim_apos_entrar(fim, agora, 2);
        assert_eq!(resta(fim, agora), TETO_S, "as tres somam 1h30");
        // As tres de uma vez dao o mesmo.
        assert_eq!(resta(fim_apos_entrar(0, agora, 3), agora), TETO_S);
        // O tempo CORRE: meia hora depois, resta uma hora.
        assert_eq!(resta(fim, agora + DURACAO_S), TETO_S - DURACAO_S);
        // E nunca fica negativo.
        assert_eq!(resta(fim, agora + TETO_S * 9), 0);
    }

    #[test]
    fn nao_da_pra_gastar_passe_a_toa() {
        let agora = 1_000_000i64;
        assert!(pode_entrar(0, agora, 3, 1).is_ok());
        assert!(pode_entrar(0, agora, 0, 1).is_err(), "sem passe, nao entra");
        assert!(pode_entrar(0, agora, 9, 4).is_err(), "quatro nao existe");
        assert!(pode_entrar(0, agora, 9, 0).is_err());
        // Com 1h30 no relogio, gastar mais um seria jogar o passe fora.
        let cheio = fim_apos_entrar(0, agora, 3);
        assert!(pode_entrar(cheio, agora, 9, 1).is_err());
    }

    /// TODA ilhota tem que ser alcançável A PÉ, pelas pontes.
    ///
    /// Uma ilhota sem ponte é um bônus que ninguém pega, e o jogador só
    /// descobre depois de gastar meia hora de passe tentando chegar nela.
    #[test]
    fn toda_ilhota_se_alcanca_a_pe() {
        let v = ilhotas();
        // Caminha em linha reta de uma ilhota a outra e exige chão o tempo
        // todo: é isso que a ponte tem que garantir.
        for (a, b) in pontes() {
            let n = 200;
            for k in 0..=n {
                let p = a.lerp(b, k as f32 / n as f32);
                assert!(
                    e_chao(p),
                    "buraco na ponte de ({:.0},{:.0}) pra ({:.0},{:.0}) em {p:?}",
                    a.x,
                    a.y,
                    b.x,
                    b.y
                );
            }
        }
        // E o grafo liga TUDO: uma ponte por ilhota não basta se ela ligar
        // duas que já se falavam.
        let mut visto = vec![false; v.len()];
        let mut fila = vec![0usize];
        visto[0] = true;
        while let Some(i) = fila.pop() {
            for (a, b) in pontes() {
                let (ia, ib) = (
                    v.iter().position(|x| x.centro == a).unwrap(),
                    v.iter().position(|x| x.centro == b).unwrap(),
                );
                for (de, para) in [(ia, ib), (ib, ia)] {
                    if de == i && !visto[para] {
                        visto[para] = true;
                        fila.push(para);
                    }
                }
            }
        }
        assert!(visto.iter().all(|x| *x), "ilhota ilhada: {visto:?}");
    }

    /// Cada bônus aparece UMA vez, e todos os pedidos estão lá.
    #[test]
    fn cada_bonus_tem_a_sua_ilhota() {
        let v = ilhotas();
        for b in [
            Bonus::Xp,
            Bonus::DropDeMob,
            Bonus::Ouro,
            Bonus::DropDeChefe,
            Bonus::Coleta(0),
            Bonus::Coleta(1),
            Bonus::Coleta(5),
        ] {
            assert_eq!(
                v.iter().filter(|i| i.bonus == b).count(),
                1,
                "{} aparece fora de uma vez",
                b.nome()
            );
        }
        // Todo bônus melhora alguma coisa: multiplicador 1 seria uma ilhota
        // que promete e não paga.
        assert!(v.iter().all(|i| i.bonus.multiplicador() > 1.0));
    }

    /// As ilhotas não se tocam: separadas, com ponte entre elas.
    #[test]
    fn as_ilhotas_sao_separadas() {
        let v = ilhotas();
        for (i, a) in v.iter().enumerate() {
            for b in &v[i + 1..] {
                let d = a.centro.distance(b.centro);
                assert!(
                    d > a.raio + b.raio + 8.0,
                    "{} e {} se encostam ({d:.0}u)",
                    a.bonus.nome(),
                    b.bonus.nome()
                );
            }
        }
        // A chegada é chão, senão o jogador nasce na água.
        assert!(e_chao(CHEGADA));
        // E fora de tudo é mar.
        assert!(!e_chao(Vec2::new(raio_do_mundo(), 0.0)));
    }

    /// O índice do bônus vai e volta pelo fio sem se perder.
    ///
    /// Ele é um `u8` porque viaja em `Estado`; um mapeamento torto aqui daria
    /// a ilhota certa com o bônus de outra, sem erro nenhum.
    #[test]
    fn o_indice_do_bonus_vai_e_volta() {
        for i in ilhotas() {
            assert_eq!(
                Bonus::do_indice(i.bonus.indice()),
                Some(i.bonus),
                "{} não volta do índice",
                i.bonus.nome()
            );
        }
        // Todos DISTINTOS: dois bônus no mesmo índice dariam o multiplicador
        // errado calado.
        let mut v: Vec<u8> = ilhotas().iter().map(|i| i.bonus.indice()).collect();
        v.sort_unstable();
        let n = v.len();
        v.dedup();
        assert_eq!(v.len(), n, "dois bônus no mesmo índice");
        assert_eq!(Bonus::do_indice(255), None, "255 é 'nenhum'");
        assert_eq!(indice_do_bonus_em(CHEGADA), Bonus::DropDeChefe.indice());
    }

    /// A COTA DIÁRIA vira sozinha, e o empacotamento vai e volta.
    ///
    /// Ela mora num inteiro só (`dia * 16 + usadas`). Empacotar é o tipo de
    /// esperteza que morde calada: uma conta trocada dá cota infinita num dia
    /// e zero no outro, sem erro nenhum.
    #[test]
    fn a_cota_diaria_reseta_sozinha_e_cabe_num_inteiro() {
        let hoje = 1_800_000_000i64; // um instante qualquer
        let amanha = hoje + 86_400;

        // Dia virgem: cota cheia.
        assert_eq!(gratis_restantes(0, hoje), GRATIS_POR_DIA);

        // Gastando uma por vez, ela desce até zero e para lá.
        let mut g = 0i64;
        for k in 1..=GRATIS_POR_DIA {
            g = apos_gastar_gratis(g, hoje, 1);
            assert_eq!(gratis_restantes(g, hoje), GRATIS_POR_DIA - k);
        }
        assert_eq!(gratis_restantes(g, hoje), 0, "a cota acabou");
        g = apos_gastar_gratis(g, hoje, 1);
        assert_eq!(gratis_restantes(g, hoje), 0, "gastar a mais não vira dívida");

        // AMANHÃ ela está cheia de novo, sem ninguém rodar nada.
        assert_eq!(gratis_restantes(g, amanha), GRATIS_POR_DIA);

        // E as três de uma vez valem o mesmo que três separadas.
        assert_eq!(
            gratis_restantes(apos_gastar_gratis(0, hoje, 3), hoje),
            0
        );
        // O dia guardado é o dia do jogo (vira às 04:00 de Brasília).
        assert_eq!(
            apos_gastar_gratis(0, hoje, 1).div_euclid(16),
            crate::dungeon::dia(hoje)
        );
    }

    /// Na PONTE não vale bônus: quem atravessa está entre dois lugares.
    #[test]
    fn a_ponte_nao_da_bonus() {
        let v = ilhotas();
        let meio = v[0].centro.lerp(v[1].centro, 0.5);
        assert!(e_chao(meio), "a ponte tem que ser chão");
        assert_eq!(bonus_em(meio), None);
        assert_eq!(bonus_em(v[1].centro), Some(v[1].bonus));
    }
}
