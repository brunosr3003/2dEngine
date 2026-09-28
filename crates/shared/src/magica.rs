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

/// Trocas do Mercador Magico: (item recebido, quantidade, moedas por pacote).
/// Uma tabela unica para a loja no cliente e a validacao no servidor.
pub const TROCAS: &[(u16, u32, u32)] = &[
    (crate::item_id::GLITTERING_POWDER, 1, 12),
    // Ritmo observado pelo dono: ~100 moedas/min. Itens da loja cash
    // seguem 100 moedas por TP para nao sair em poucos minutos de farm.
    (crate::item_id::COPPER, 1_000, 200),
    (crate::item_id::DARKSTEEL, 200, 600),
    (crate::item_id::SCALE, 1, 2_500),
    (crate::item_id::CLAW, 1, 2_500),
    (crate::item_id::HORN, 1, 2_500),
    (crate::item_id::HIDE, 1, 2_500),
    (crate::item_id::PERGAMINHO_INVOCA_CHAVE, 1, 12_000),
    (crate::item_id::PERGAMINHO_INVOCA_PET, 1, 25_000),
    (crate::item_id::PERGAMINHO_INVOCA_MONTARIA, 1, 50_000),
    (crate::item_id::PASSE_MAGICO, 1, 5_000),
];

pub const LOJA_DE_TROCAS: u32 = 90_475;
pub const GUIA_DOS_DEGRAUS: &str = "Guia dos Degraus";

pub fn posto_de_trocas() -> Vec2 {
    CHEGADA + Vec2::new(22.0, 3.0)
}

pub fn posto_dos_degraus() -> Vec2 {
    CHEGADA + Vec2::new(12.0, 12.0)
}

/// O nome da zona do PRIMEIRO nível. Os outros são `ilha_magica_2`, `_3`…
///
/// Ver `NIVEIS`: cada nível é uma ZONA própria, e não uma instância.
pub const ZONA: &str = "ilha_magica";

/// Esta zona é alguma Ilha Mágica?
pub fn e_magica(z: &str) -> bool {
    NIVEIS.iter().any(|n| n.zona == z)
}

// ─────────────────────────── os níveis ───────────────────────────

/// Um degrau da Ilha Mágica.
///
/// O dono: "a ilha mágica precisa ter níveis; exemplo: você é nv 15, aí pode
/// entrar na ilha que é desbloqueada no nv 15 com poder recomendado 1500, aí
/// os mobs de todas as ilhotas vão estar mais ou menos nesse ramo; aí a
/// próxima nv 30 com poder recomendado x, etc — dessa forma fica melhor,
/// todos os mobs padronizados".
///
/// O problema que isso resolve tem número: a `DefIlha` antiga dizia
/// `nivel: (1, 60)`, e `zonas_comuns_da_ilha` espalha esse intervalo pela
/// distância do desembarque. Ou seja, numa ilha de 280 u o jogador
/// atravessava uma ponte e saía do nível 3 pro 40. Não havia "poder
/// recomendado" possível.
///
/// ## Por que ZONA por nível, e não instância
///
/// `ServerSpawnZone` não conhece instância — o campo não existe, e mob por
/// instância seria mexer no coração do spawn. Zona, por outro lado, é como o
/// jogo já separa mundos: um processo por zona, cada um com seu relevo e sua
/// faixa de mob. Cada nível é uma zona, e o resto do jogo (diretório,
/// handoff, `def_da_zona`) já sabe lidar com isso sem uma linha nova.
///
/// O relevo é O MESMO nos três — muda a faixa de mob e o portão de entrada.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NivelMagico {
    /// 1, 2, 3… — é o que aparece pro jogador ("Ilha Mágica I").
    pub grau: u8,
    pub zona: &'static str,
    pub nome: &'static str,
    /// Nível do personagem pra liberar a entrada.
    ///
    /// PODE SER MENOR que o nível dos mobs, e no degrau I é: o dono quis
    /// "deixa entrar no nv 15 mesmo ele sendo lvl 20, só sobe o poder
    /// recomendado". A ilha não é obrigação — quem quer entrar abaixo do
    /// nível e apanhar tem o direito, desde que o número avise.
    pub exige_nivel: u32,
    /// A faixa de nível dos mobs — ESTREITA, que é o pedido.
    pub mob: (u32, u32),
    /// O nome do Colosso deste degrau (`bosses`).
    pub nome_do_chefe: &'static str,
}

/// Os degraus. Acrescentar um aqui é o bastante: `def_da_zona`, o painel e a
/// checagem de entrada saem todos desta tabela.
pub const NIVEIS: &[NivelMagico] = &[
    NivelMagico {
        grau: 1,
        zona: "ilha_magica",
        nome: "Ilha Mágica I",
        exige_nivel: 15,
        mob: (20, 23),
        nome_do_chefe: "Colosso da Ilha Mágica",
    },
    NivelMagico {
        grau: 2,
        zona: "ilha_magica_2",
        nome: "Ilha Mágica II",
        exige_nivel: 30,
        mob: (30, 33),
        nome_do_chefe: "Colosso Maior da Ilha Mágica",
    },
    NivelMagico {
        grau: 3,
        zona: "ilha_magica_3",
        nome: "Ilha Mágica III",
        exige_nivel: 45,
        mob: (45, 48),
        nome_do_chefe: "Colosso Ancião da Ilha Mágica",
    },
];

impl NivelMagico {
    /// O PODER RECOMENDADO, na escala do próprio jogo.
    ///
    /// Sai de `dungeon::poder_referencia` — a mesma conta que as dungeons
    /// usam e que o jogador já vê na ficha —, e não de um número escolhido a
    /// dedo. Um "1500" inventado não se compara com nada; este se compara com
    /// o poder dele.
    ///
    /// Usa o TOPO da faixa de mob, e não o nível de entrada: é o que o
    /// jogador vai enfrentar. No degrau I os dois são diferentes de propósito
    /// (entra no 15, mobs 20-23), e é justamente aí que o número serve de
    /// aviso.
    pub fn poder(&self) -> i32 {
        crate::dungeon::poder_referencia(self.mob.1)
    }

    /// O jogador entra abaixo do que a ilha pede?
    pub fn acima_do_nivel(&self, nivel: u32) -> bool {
        nivel < self.mob.0
    }
}

/// O degrau desta zona.
pub fn nivel_da_zona(z: &str) -> Option<&'static NivelMagico> {
    NIVEIS.iter().find(|n| n.zona == z)
}

/// O maior degrau que um personagem de `nivel` pode entrar — `None` se ele
/// ainda não alcançou o primeiro.
pub fn maior_liberado(nivel: u32) -> Option<&'static NivelMagico> {
    NIVEIS.iter().rev().find(|n| nivel >= n.exige_nivel)
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

    /// O mesmo nome sem o "Ilhota da": o que vai ESCRITO NO MAPA.
    ///
    /// No mapa da Ilha Mágica tudo é ilhota, então a palavra não distingue
    /// nada e só rouba a largura de que o nome precisa. Sete rótulos curtos
    /// cabem onde sete longos se atropelariam.
    pub fn nome_curto(self) -> &'static str {
        match self {
            Self::Xp => "Experiência",
            Self::DropDeMob => "Espólio",
            Self::Ouro => "Ouro",
            Self::DropDeChefe => "Colosso",
            Self::Coleta(0) => "Madeira",
            Self::Coleta(5) => "Energia",
            Self::Coleta(_) => "Pedra",
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
    ilhotas_fixas().to_vec()
}

fn monta_ilhotas() -> Vec<Ilhota> {
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
        // O COLOSSO SAI DO MEIO e vem pro anel, no lugar que era da madeira.
        //
        // O dono: "coloca a primeira ilha, a ilha central, para ser uma ilha
        // de recurso sem tantos mobs, porque aí você não nasce morrendo". E
        // era literalmente isso: `CHEGADA` e' o centro, o centro era a Ilhota
        // do Colosso, e desde que as ilhotas de combate ficaram lotadas o
        // jogador chegava em cima do chefe e da horda.
        //
        // O chefe nao precisou de mudanca: `chefes` ja' o poe na ilhota que
        // TEM o bonus dele, onde quer que ela esteja.
        Bonus::DropDeChefe,
        // TIER 4, e não 1: a pedra desta ilha vem da ilhota, não da altura
        // (`tier_da_pedra`), e a casa da pedra dá a MELHOR. Prometer 1 e
        // entregar 4 é o que `toda_ilhota_de_recurso_tem_o_recurso` reprova —
        // e é a promessa que estava errada, não a entrega.
        Bonus::Coleta(4),
        Bonus::DropDeMob,
    ];
    // O MEIO E' DE RECURSO, e e' onde o jogador chega (`CHEGADA`).
    //
    // Madeira porque e' o recurso mais inofensivo do jogo: ninguem nasce pra
    // defender tronco, e ilhota de coleta fica fora do adensamento de combate
    // (`e_de_combate`). Chegar e poder respirar e' o ponto.
    let mut v = vec![Ilhota {
        centro: Vec2::ZERO,
        raio: RAIO_CENTRO,
        bonus: Bonus::Coleta(0),
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
    pontes_fixas().to_vec()
}

fn monta_pontes() -> Vec<(Vec2, Vec2)> {
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
    ilhotas_fixas()
        .iter()
        .find(|i| {
            let d = p - i.centro;
            let dist2 = d.length_squared();
            // REJEIÇÃO BARATA PRIMEIRO. A ondulação da costa vale no máximo
            // ±20% do raio, então o que está além disso não precisa de
            // `atan2` nem de três senos — e quase toda coluna da ilha está.
            // Sem esta linha, abrir a ilha travava o cliente a 100% de CPU.
            if dist2 > (i.raio * 1.25) * (i.raio * 1.25) {
                return false;
            }
            if dist2 <= (i.raio * 0.75) * (i.raio * 0.75) {
                return true;
            }
            // A COSTA ONDULA, então a pergunta "estou na ilhota" usa o raio
            // daquela direção, e não o raio nominal. Se esta conta e a de
            // `bloco_da_coluna` discordarem, o jogador pisa em chão que o
            // servidor considera mar.
            dist2 <= raio_da_ilhota(i, d.y.atan2(d.x)).powi(2)
        })
        .copied()
}

/// O bônus que vale NESTE ponto. Na ponte não vale bônus nenhum — quem está
/// atravessando está entre dois lugares, e não nos dois.
pub fn bonus_em(p: Vec2) -> Option<Bonus> {
    ilhota_em(p).map(|i| i.bonus)
}

/// A distância do ponto ao segmento de ponte mais próximo.
fn dist_da_ponte(p: Vec2) -> f32 {
    pontes_fixas()
        .iter()
        .copied()
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
/// A `DefIlha` de um degrau.
///
/// Só três campos mudam entre eles: o nome da zona, o nome de tela e a FAIXA
/// DE NÍVEL dos mobs. O relevo é o mesmo — mesma semente, mesmo raio —, e é
/// isso que faz "os mobs de todas as ilhotas mais ou menos no mesmo ramo"
/// sair de graça: `zonas_comuns_da_ilha` espalha `def.nivel` pela distância
/// do desembarque, e com a faixa estreita não há o que espalhar.
pub fn def_do_nivel(n: &NivelMagico) -> crate::terreno::DefIlha {
    crate::terreno::DefIlha {
        zona: n.zona,
        nome: n.nome,
        nivel: (n.mob.0, n.mob.1),
        ..DEF
    }
}

pub const DEF: crate::terreno::DefIlha = crate::terreno::DefIlha {
    zona: ZONA,
    nome: "Ilha Mágica",
    semente: SEMENTE,
    raio_blocos: RAIO_BLOCOS,
    bioma: crate::terreno::Bioma::Floresta,
    // No mapa-mundi, ao norte do arquipelago: longe o bastante pra nao se
    // confundir com ilha de progressao.
    centro: [-1800.0, -2600.0],
    // A FAIXA DO PRIMEIRO DEGRAU. Era `(1, 60)`, e esse era o defeito que o
    // dono descreveu: `zonas_comuns_da_ilha` espalha o intervalo pela
    // distância do desembarque, então numa ilha de 280 u o jogador
    // atravessava uma ponte e saía do nível 3 pro 40. Não havia "poder
    // recomendado" possível.
    nivel: (20, 23),
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

/// Fração do raio que é PRAIA: faixa plana, no nível da orla, antes de o
/// terreno subir.
///
/// Sem ela a areia era um CONTORNO. `altura_do_domo` sobe em quadrática logo
/// da beira, então o chão passava do nível da orla em duas ou três colunas e
/// a praia virava uma linha de um pixel — visto no mapa de altura, não
/// deduzido. O dono pediu "uma ilha com praia e tudo mais".
///
/// Ela também ajuda a caminhada: é a parte mais rasa da ilhota, e agora é
/// plana de verdade em vez de ser a mais íngreme.
pub const PRAIA: f32 = 0.16;

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
    // A PRAIA, plana, na beira.
    let beira = 1.0 - PRAIA;
    if t >= beira {
        return ALTURA;
    }
    // A ladeira agora vai do platô até o começo da praia — trecho mais curto
    // que antes, então ela ficou um pouco mais íngreme. Continua abaixo do
    // 1,0 u/u que o passo vence, e `da_chegada_se_anda_ate_toda_ilhota` é
    // quem confere.
    let k = (t - TOPO_PLANO) / (beira - TOPO_PLANO);
    ALTURA_TOPO + (ALTURA - ALTURA_TOPO) * k * k
}

/// As ilhotas e as pontes, calculadas UMA vez.
///
/// `bloco_da_coluna` roda por COLUNA — 1,25 milhão delas numa ilha de 560
/// blocos de raio —, e `ilhota_em`/`dist_da_ponte` alocavam um `Vec` a cada
/// chamada. Com o relevo antigo (um `distance` por ilhota) isso já era caro e
/// passava; depois que a costa ganhou ondulação (três senos por ilhota, mais
/// a ondulação do chão) o custo por coluna multiplicou e o cliente TRAVOU a
/// 100% de CPU ao abrir a ilha — medido no emulador, não deduzido.
///
/// Elas são função pura de constantes: calcular de novo por coluna nunca fez
/// sentido, só não doía o bastante pra aparecer.
fn ilhotas_fixas() -> &'static [Ilhota] {
    static V: std::sync::OnceLock<Vec<Ilhota>> = std::sync::OnceLock::new();
    V.get_or_init(monta_ilhotas)
}

fn pontes_fixas() -> &'static [(Vec2, Vec2)] {
    static V: std::sync::OnceLock<Vec<(Vec2, Vec2)>> = std::sync::OnceLock::new();
    V.get_or_init(monta_pontes)
}

/// O raio da ilhota NAQUELA direção — a costa não é um compasso.
///
/// As ilhotas eram círculos perfeitos com um domo radial em cima, e o dono:
/// "as ilhas estão muito feias, estão só redondas bem padrão; tem que ser uma
/// ilhota mas com noise, relevo etc — só não pode ter montanha — e tem que
/// ser uma ilha com praia e tudo mais".
///
/// Dois senos, como na colônia: ruído de verdade seria pagar fbm por coluna
/// pra desenhar uma forma que cabe em duas linhas, e o relevo aqui precisa
/// ser função pura da coordenada (cliente e servidor calculam o mesmo chão
/// sem trocar um byte).
///
/// A FASE VEM DO CENTRO da ilhota: sem isso as seis sairiam com a mesma
/// silhueta girada, que é outro jeito de parecer padrão.
pub fn raio_da_ilhota(i: &Ilhota, ang: f32) -> f32 {
    let fase = i.centro.x * 0.031 + i.centro.y * 0.017;
    // TRÊS harmônicas, e não duas. Com duas a silhueta saía em trevo — três
    // lóbulos gordos e iguais, que de longe lê como folha e não como ilha.
    // A terceira, mais rápida e fraca, quebra a repetição sem virar serrilha.
    i.raio
        * (1.0
            + 0.10 * (ang * 3.0 + fase).sin()
            + 0.06 * (ang * 5.0 - fase * 1.7).sin()
            + 0.035 * (ang * 8.0 + fase * 0.6).sin())
}

/// A ondulação do chão da ilhota, em unidades.
///
/// PEQUENA de propósito, e presa à borda: o dono pediu relevo, "só não pode
/// ter montanha". Mais que isto e a ladeira passa de 1,0 u/u — o passo do
/// personagem vence um bloco por coluna, e acima disso a beira vira paredão e
/// a ilhota deixa de ser caminhável. `da_chegada_se_anda_ate_toda_ilhota` é
/// quem reprova.
///
/// Cresce PRA FORA (`0,25 + 0,75·t`) pra o platô do meio ficar calmo: é ele
/// que deixa a pedra e a Energia nascerem, porque as duas exigem o quadrado
/// em volta na mesma altura.
fn ondula(p: Vec2, t: f32) -> f32 {
    ((p.x * 0.045 + 1.3).sin() * (p.y * 0.039 - 0.7).cos() * 1.5
        + (p.x * 0.094 - p.y * 0.071).sin() * 0.6
        // A curta é ANTI-ANEL, não forma: sem ela a altura cruza a fronteira
        // de bloco ao longo de um círculo e a ilhota ganha curvas de nível
        // desenhadas.
        + (p.x * 0.42 + 0.4).sin() * (p.y * 0.39 - 1.1).sin() * 0.45)
        * (0.25 + 0.75 * t)
}

/// Esta ilhota é de COMBATE (XP, ouro, drop) e não de coleta?
///
/// É nelas que o bônus só vale se houver o que matar: uma Ilhota da
/// Experiência com XP em dobro e três lobos não é uma ilhota de XP. O dono:
/// "as ilhas de xp, cobre, drop têm que ter uma densidade de inimigos muito
/// grande, pra realmente fazer sentido".
pub fn e_de_combate(b: Bonus) -> bool {
    matches!(
        b,
        Bonus::Xp | Bonus::Ouro | Bonus::DropDeMob | Bonus::DropDeChefe
    )
}

/// O centro de cada ilhota de combate — onde as hordas devem nascer.
/// A ilhota da chegada é PORTO SEGURO: sem PvP, e com quem vender poção.
///
/// O dono: "coloca a ilha central para ser uma ilha de recurso sem tantos
/// mobs, porque aí você não nasce morrendo", depois "lá tem que ser PvP
/// desativado" e "tem que ter um NPC de venda de poções". As três coisas são
/// a mesma: a ilhota do meio é onde se chega, onde se volta ao morrer e onde
/// se repõe — matar alguém ali seria matar quem acabou de renascer, sem
/// chance nenhuma.
///
/// Regra ÚNICA, usada pelo servidor (que decide o dano) e pelo cliente (que
/// escreve na tela em qual das duas você está). Duas cópias divergiriam, e a
/// tela diria "seguro" enquanto o servidor deixava bater.
pub fn e_porto_seguro(p: Vec2) -> bool {
    ilhota_em(p).is_some_and(|i| i.centro == CHEGADA)
}

/// Onde fica o vendedor de poções da ilhota da chegada.
///
/// Fora do meio: o meio é o ponto de renascimento, e um NPC plantado ali
/// receberia todo mundo em cima dele a cada morte. A 22 u ele está longe do
/// tumulto e ainda dentro da ilhota (raio 58).
pub fn posto_de_pocoes() -> Vec2 {
    CHEGADA + Vec2::new(22.0, -10.0)
}

/// A ilhota do COLOSSO não tem recurso nenhum.
///
/// O dono: "na ilha do boss eu não quero que tenham recursos, eles só
/// atrapalham na caça pro A*". E atrapalham mesmo: árvore e pedra viram
/// estorvo, o A* contorna cada uma, e a ilhota onde o jogador mais precisa se
/// mover em linha reta — a do chefe telegráfico, em que desviar é a graça — é
/// justamente a mais entulhada.
///
/// Ela já era a única sem bônus de coleta; agora é também a única limpa.
pub fn sem_recursos(p: Vec2) -> bool {
    ilhota_em(p).is_some_and(|i| matches!(i.bonus, Bonus::DropDeChefe))
}

/// Ilhotas de combate ficam sem minério e sem pedras decorativas.
pub fn sem_pedras(p: Vec2) -> bool {
    ilhota_em(p).is_some_and(|i| e_de_combate(i.bonus))
}

pub fn centros_de_combate() -> Vec<Vec2> {
    ilhotas()
        .into_iter()
        .filter(|i| e_de_combate(i.bonus))
        .map(|i| i.centro)
        .collect()
}

/// O tier da pedra desta coluna na Ilha Mágica — `None` fora dela.
///
/// No mundo o tier sai da ALTURA (`terreno::tier_de_minerio`): pedra boa é de
/// mina, e mina fica no alto. Aqui isso não funciona — o arquipélago é baixo
/// por desenho, nenhuma ilhota passa de 20 u, e tudo caía na primeira faixa.
/// O dono: "só tem recurso cinza lá".
///
/// Então o tier vem da ILHOTA. A que promete pedra dá a MELHOR (4, roxa), e é
/// isso que faz o "recursos melhores" valer junto com o "menos recursos": a
/// ilhota da pedra é o lugar de ir buscar pedra boa, e não mais um chão
/// cinza. As outras dão 2 ou 3 conforme a posição, pra a ilha não ficar de
/// uma cor só.
pub fn tier_da_pedra(ger: &crate::terreno::Gerador, bx: i32, bz: i32) -> Option<u8> {
    if !ger.e_magica() {
        return None;
    }
    let p = Vec2::new(bx as f32, bz as f32) * crate::terreno::BLOCO;
    let i = ilhota_em(p)?;
    Some(match i.bonus {
        // A casa da pedra dá a melhor que existe.
        Bonus::Coleta(t) if (1..=4).contains(&t) => 4,
        // O centro é o mais disputado da ilha: a pedra dele acompanha.
        Bonus::DropDeChefe => 3,
        // O resto varia com a posição, pra não virar monocromia.
        _ => 2 + (i.centro.x.abs() as i32 / 37 % 2) as u8,
    })
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
        let d = p - i.centro;
        let raio = raio_da_ilhota(&i, d.y.atan2(d.x));
        let dist = d.length();
        let t = (dist / raio).clamp(0.0, 1.0);
        let h = altura_do_domo(dist, raio) + ondula(p, t);
        // Nunca abaixo da ORLA: a ondulação não pode cavar poça de mar no
        // meio da praia nem furar o chão da beirada.
        return ((h.max(ALTURA)) / crate::terreno::BLOCO).round() as i32 - 1;
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
    /// Gastar `entradas` passes e ir ao degrau `grau`. Zero retoma o tempo
    /// ativo sem cobrar nova entrada.
    ///
    /// `grau` é `NivelMagico::grau`; 0 significa "o maior que eu posso", que é
    /// o que o botão da tarja manda ao estender — lá não há onde escolher.
    Entrar { entradas: u8, grau: u8 },
    /// Trocar de degrau, dentro da ilha, sem alterar o relógio nem a volta.
    Trocar { grau: u8 },
    /// Sair antes da hora. O tempo CONTINUA correndo — senão o jogador sairia
    /// no primeiro susto e voltaria com o relógio intacto, e a ilha deixaria
    /// de ter hora.
    Sair,
}

/// O que o servidor conta sobre a Ilha Mágica.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum AvisoMagica {
    /// Interação com o guia dos degraus abre o painel dentro da ilha.
    AbrirPainel,
    Estado {
        /// O maior degrau liberado pro nível deste personagem — 0 se nenhum.
        ///
        /// Vem do servidor, e não do nível que o cliente conhece: a trava é do
        /// servidor, e a tela que decidisse sozinha mostraria botão que o
        /// servidor recusa.
        #[serde(default)]
        grau_maximo: u8,
        /// Em que degrau ele está agora (0 = fora da ilha).
        #[serde(default)]
        grau_atual: u8,
        /// O nível do personagem — o painel compara com a faixa de mob pra
        /// avisar "você está abaixo desta ilha".
        #[serde(default)]
        meu_nivel: u32,
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

    #[test]
    fn guia_dos_degraus_fica_na_chegada_segura() {
        let p = posto_dos_degraus();
        assert!(e_chao(p));
        assert!(e_porto_seguro(p));
        assert!(p.distance(posto_de_pocoes()) > 8.0);
        assert!(p.distance(posto_de_trocas()) > 8.0);
    }

    /// NADA nasce na ponte: nem árvore, nem pedra.
    ///
    /// A ponte é estreita de propósito — é o gargalo que dá sentido ao PvP —
    /// e um tronco no meio dela a fecha: o A* deixa de achar passagem e só dá
    /// pra atravessar andando na mão. O dono: "algumas pontes estão com
    /// árvores e pedras no meio, aí não dá pra passar usando A*".
    /// A Ilha Mágica tem MENOS recursos que o mundo, e cada um vale MAIS.
    ///
    /// O dono: "tá lotado de recursos, pedra, árvores etc, e fica muito feio;
    /// tem que ser menos recursos e recursos melhores — uma pedra normal dá
    /// 30 darksteel, na ilha mágica 50-60".
    ///
    /// O "melhores" já existia (×2 por ilhota, e 30 × 2 = 60); o que faltava
    /// era o "menos". Este teste trava os dois juntos, porque separados um
    /// deles some numa calibragem futura e ninguém nota.
    /// A Ilha Mágica não é toda de pedra cinza.
    ///
    /// O tier do minério sai da altura no mundo, e a Ilha Mágica é baixa por
    /// desenho: tudo caía na primeira faixa. O dono: "só tem recurso cinza
    /// lá".
    /// Os degraus são coerentes entre si e com o resto do jogo.
    /// O poder recomendado sai da escala do JOGO, e avisa quem está abaixo.
    ///
    /// O dono: "deixa entrar no nv 15 mesmo ele sendo lvl 20, só sobe o poder
    /// recomendado". Entrar abaixo do nível é um direito — apanhar por
    /// escolha é diferente de apanhar por surpresa —, e o que separa os dois
    /// é o número avisar.

    /// O relevo da ilha se monta num tempo aceitável.
    ///
    /// Este teste existe porque eu travei o cliente. A costa ondulada pôs
    /// três senos por ilhota dentro de `bloco_da_coluna`, que roda por COLUNA
    /// — 1,25 milhão delas —, e `ilhota_em`/`dist_da_ponte` ainda alocavam um
    /// `Vec` a cada chamada. Abrir a Ilha Mágica no emulador travou o jogo a
    /// 100% de CPU, e não havia teste nenhum olhando para isso.
    ///
    /// O limite é generoso de propósito: ele não mede o computador, mede
    /// ORDEM DE GRANDEZA. Se alguém puser um fbm aqui, ele estoura.
    /// Quem CHEGA não chega numa briga.
    ///
    /// O dono: "coloca a primeira ilha, a ilha central, para ser uma ilha de
    /// recurso sem tantos mobs, porque aí você não nasce morrendo". Era
    /// literal: `CHEGADA` é o centro, o centro era a Ilhota do Colosso, e
    /// desde que as ilhotas de combate ficaram lotadas o jogador aparecia em
    /// cima do chefe e de uma horda.
    #[test]
    fn a_chegada_e_um_lugar_calmo() {
        let onde = ilhota_em(CHEGADA).expect("a chegada é em terra");
        assert!(
            !e_de_combate(onde.bonus),
            "chega na {} — é de combate",
            onde.bonus.nome()
        );
        // E o Colosso continua existindo, só que longe da chegada.
        let chefe = ilhotas()
            .into_iter()
            .find(|i| i.bonus == Bonus::DropDeChefe)
            .expect("o Colosso tem casa");
        assert!(
            chefe.centro.distance(CHEGADA) > RAIO_CENTRO,
            "o Colosso continua em cima de quem chega"
        );
    }

    #[test]
    fn o_relevo_da_ilha_e_barato_por_coluna() {
        let lado = 2 * RAIO_BLOCOS;
        let amostras = 200_000usize;
        let ger = crate::terreno::Gerador::da_ilha_magica();
        let t0 = std::time::Instant::now();
        let mut soma = 0i64;
        for k in 0..amostras {
            // Varre em diagonal pra pegar mar, ponte e ilhota na mesma conta.
            let bx = (k as i32 * 7) % lado - RAIO_BLOCOS;
            let bz = (k as i32 * 13) % lado - RAIO_BLOCOS;
            let topo = bloco_da_coluna(bx, bz);
            soma += topo as i64;
            // A VEGETAÇÃO ENTRA NA CONTA, e ela é o pior caminho: ela chama
            // `na_ponte_magica` por coluna, que antes do cache alocava DOIS
            // `Vec` a cada chamada. Milhões de alocações num celular é o que
            // travou o jogo.
            if topo > 0 {
                let _ = crate::terreno::arvore_da_coluna(
                    crate::terreno::Bioma::Floresta,
                    bx,
                    bz,
                    topo,
                    0,
                    &ger,
                    false,
                );
            }
        }
        let por_coluna = t0.elapsed().as_secs_f64() / amostras as f64;
        // A ilha inteira são ~1,25 milhão de colunas. A 1 µs por coluna são
        // 1,25 s — o limite do aceitável pra montar um mundo.
        assert!(
            por_coluna < 1e-6,
            "{:.0} ns por coluna: a ilha inteira levaria {:.1} s",
            por_coluna * 1e9,
            por_coluna * (lado as f64).powi(2)
        );
        assert!(soma != 0, "a varredura não tocou chão nenhum");
    }

    #[test]
    fn o_poder_recomendado_e_da_escala_do_jogo() {
        for n in NIVEIS {
            // A MESMA conta das dungeons, e não um número escolhido a dedo:
            // "1500" não se compara com nada; este se compara com o poder que
            // o jogador já vê na ficha.
            assert_eq!(n.poder(), crate::dungeon::poder_referencia(n.mob.1));
            assert!(n.poder() > 0, "{}: poder zerado", n.nome);
        }
        // O degrau I: entra no 15, mas a ilha é de 20-23 — e ele avisa.
        let um = &NIVEIS[0];
        assert_eq!(um.exige_nivel, 15, "o portão é o que o dono pediu");
        assert!(
            um.acima_do_nivel(15),
            "quem entra no 15 tem que ser avisado de que a ilha é acima"
        );
        assert!(
            !um.acima_do_nivel(um.mob.0),
            "no nível dos mobs não há o que avisar"
        );
    }

    #[test]
    fn os_degraus_sobem_juntos() {
        let mut antes: Option<&NivelMagico> = None;
        for n in NIVEIS {
            assert!(n.mob.0 <= n.mob.1, "{}: faixa invertida", n.nome);
            // ESTREITA é o pedido: "todos os mobs padronizados". Larga demais
            // e o jogador atravessa uma ponte e muda de mundo, que é o que
            // acontecia com a faixa antiga de (1, 60).
            assert!(
                n.mob.1 - n.mob.0 <= 5,
                "{}: faixa de {} níveis é larga demais pra 'padronizado'",
                n.nome,
                n.mob.1 - n.mob.0
            );
            // O portão pode ser ABAIXO do mob (é o caso do degrau I: entra
            // no 15, mobs 20-23), mas nunca acima: entrar num degrau pra
            // achar bicho mais fraco que você é não ter degrau.
            assert!(
                n.exige_nivel <= n.mob.1,
                "{}: entra no {} e o mob mais forte é {}",
                n.nome,
                n.exige_nivel,
                n.mob.1
            );
            assert!(
                crate::terreno::def_da_zona(n.zona).is_some(),
                "{}: zona sem def",
                n.nome
            );
            if let Some(a) = antes {
                assert!(n.grau == a.grau + 1, "graus fora de ordem");
                assert!(
                    n.exige_nivel > a.exige_nivel,
                    "{}: não sobe o portão",
                    n.nome
                );
                assert!(n.poder() > a.poder(), "{}: não sobe o poder", n.nome);
                assert!(
                    n.mob.0 > a.mob.1,
                    "{}: faixa encavalada com a anterior",
                    n.nome
                );
            }
            antes = Some(n);
        }
    }

    /// O portão de nível libera na ordem certa.
    #[test]
    fn o_portao_libera_por_nivel() {
        let primeiro = NIVEIS[0].exige_nivel;
        assert_eq!(
            maior_liberado(primeiro - 1),
            None,
            "abaixo do primeiro, nada"
        );
        assert_eq!(maior_liberado(primeiro).map(|n| n.grau), Some(1));
        let ultimo = NIVEIS.last().unwrap();
        assert_eq!(
            maior_liberado(ultimo.exige_nivel + 50).map(|n| n.grau),
            Some(ultimo.grau),
            "acima de tudo, o último degrau"
        );
        // E cada degrau tem um chefe seu, na zona dele.
        for n in NIVEIS {
            let chefes = crate::bosses::da_zona(n.zona);
            assert!(!chefes.is_empty(), "{}: sem chefe", n.nome);
        }
    }

    #[test]
    fn a_pedra_da_ilha_magica_nao_e_so_cinza() {
        let ger = crate::terreno::Gerador::da_ilha_magica();
        let mut vistos = std::collections::BTreeSet::new();
        let mut da_pedra = None;
        for i in ilhotas() {
            let bx = (i.centro.x / crate::terreno::BLOCO) as i32;
            let bz = (i.centro.y / crate::terreno::BLOCO) as i32;
            let t = tier_da_pedra(&ger, bx, bz).expect("dentro da ilhota tem tier");
            vistos.insert(t);
            if matches!(i.bonus, Bonus::Coleta(x) if (1..=4).contains(&x)) {
                da_pedra = Some(t);
            }
        }
        assert!(
            vistos.len() >= 2,
            "a ilha inteira tem tier {vistos:?} — continua de uma cor só"
        );
        assert!(!vistos.contains(&1), "tier 1 é o cinza que o dono reclamou");
        assert_eq!(
            da_pedra,
            Some(4),
            "a ilhota que PROMETE pedra tem que dar a melhor"
        );
        // Fora da Ilha Mágica a regra não vale: o mundo continua com a dele.
        let mundo = crate::terreno::Gerador::novo(
            7,
            700,
            crate::terreno::Bioma::Floresta,
            crate::terreno::ESCALA_ALTURA,
        );
        assert_eq!(tier_da_pedra(&mundo, 0, 0), None);
    }

    #[test]
    fn a_ilha_magica_e_rala_e_rica() {
        // RICA: a pedra de 30 vira 60, que é o número do dono.
        assert_eq!(
            Bonus::Coleta(1).multiplicador(),
            2.0,
            "o nó tem que pagar o dobro"
        );

        // RALA: contando nós numa faixa igual das duas ilhas.
        let magica = crate::terreno::Gerador::da_ilha_magica();
        let mundo = crate::terreno::Gerador::novo(
            7,
            700,
            crate::terreno::Bioma::Floresta,
            crate::terreno::ESCALA_ALTURA,
        );
        let conta = |ger: &crate::terreno::Gerador, cx: i32, cz: i32| {
            let mut n = 0;
            for bz in cz - 90..cz + 90 {
                for bx in cx - 90..cx + 90 {
                    let topo = ger.bloco_em(bx, bz);
                    if topo <= 0 {
                        continue;
                    }
                    if crate::terreno::arvore_da_coluna(
                        crate::terreno::Bioma::Floresta,
                        bx,
                        bz,
                        topo,
                        0,
                        ger,
                        false,
                    )
                    .is_some()
                    {
                        n += 1;
                    }
                }
            }
            n
        };
        let c = ilhotas()[0].centro;
        let na_magica = conta(
            &magica,
            (c.x / crate::terreno::BLOCO) as i32,
            (c.y / crate::terreno::BLOCO) as i32,
        );
        let no_mundo = conta(&mundo, 0, 0);
        assert!(
            no_mundo > 0,
            "a ilha de comparação ficou sem árvore: o teste não mede nada"
        );
        assert!(
            (na_magica as f32) < no_mundo as f32 * 0.6,
            "a Ilha Mágica tem {na_magica} árvores contra {no_mundo} do mundo — \
             não ficou mais limpa"
        );
    }

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
                    // A COLUNA é quem manda, e não o ponto contínuo: o chão é
                    // feito bloco a bloco, e perto da costa (que agora ondula)
                    // o arredondamento troca a resposta. Perguntar no ponto e
                    // cobrar na coluna seria comparar duas coisas diferentes.
                    let bx = (p.x / crate::terreno::BLOCO).round() as i32;
                    let bz = (p.y / crate::terreno::BLOCO).round() as i32;
                    if !ger.na_ponte_magica(bx, bz) {
                        continue;
                    }
                    pontos_de_ponte += 1;
                    let topo = ger.bloco_em(bx, bz);
                    let arv = crate::terreno::arvore_da_coluna(
                        crate::terreno::Bioma::Floresta,
                        bx,
                        bz,
                        topo,
                        0,
                        &ger,
                        false,
                    );
                    let pl = crate::terreno::planta_da_coluna(
                        crate::terreno::Bioma::Floresta,
                        bx,
                        bz,
                        topo,
                        0,
                        &ger,
                        false,
                    );
                    if arv.is_some() || pl.is_some() {
                        com_estorvo += 1;
                    }
                }
            }
        }
        assert!(
            pontos_de_ponte > 50,
            "varreu pouca ponte: {pontos_de_ponte} pontos"
        );
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
                    crate::terreno::Bioma::Floresta,
                    bx,
                    bz,
                    topo,
                    0,
                    &ger,
                    false,
                )
                .is_some()
                {
                    achou += 1;
                }
            }
        }
        assert!(
            achou > 0,
            "a ilhota ficou pelada: o corte pegou mais que a ponte"
        );
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
            // A pedra é TIER 4: a melhor, porque nesta ilha o tier vem da
            // ilhota e não da altura.
            Bonus::Coleta(4),
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
        // A CHEGADA é a ilhota de MADEIRA, e não mais a do Colosso: o
        // jogador chega num lugar de recurso, sem horda nem chefe. Era o
        // contrário, e ele "nascia morrendo".
        assert_eq!(indice_do_bonus_em(CHEGADA), Bonus::Coleta(0).indice());
        assert!(
            !e_de_combate(bonus_em(CHEGADA).unwrap()),
            "quem chega não pode chegar numa ilhota de combate"
        );
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
        assert_eq!(
            gratis_restantes(g, hoje),
            0,
            "gastar a mais não vira dívida"
        );

        // AMANHÃ ela está cheia de novo, sem ninguém rodar nada.
        assert_eq!(gratis_restantes(g, amanha), GRATIS_POR_DIA);

        // E as três de uma vez valem o mesmo que três separadas.
        assert_eq!(gratis_restantes(apos_gastar_gratis(0, hoje, 3), hoje), 0);
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

#[cfg(test)]
mod testes_da_ilhota_limpa {
    use super::*;

    /// A ILHOTA DO COLOSSO NÃO TEM RECURSO, E AS OUTRAS TÊM.
    ///
    /// O dono: "na ilha do boss eu não quero que tenham recursos, eles só
    /// atrapalham na caça pro A*".
    ///
    /// Os dois lados importam: limpar a do chefe é o pedido, e NÃO limpar as
    /// outras é o que impede o conserto de virar um apagão de recursos.
    #[test]
    fn so_a_ilhota_do_colosso_fica_limpa() {
        let def = crate::terreno::def_da_zona(ZONA).expect("zona");
        let ilha = crate::terreno::Ilha::da_ilha(def);
        let mut limpas = 0;
        for i in ilhotas() {
            let tem = conta_estorvos(&ilha, i.centro);
            if matches!(i.bonus, Bonus::DropDeChefe) {
                limpas += 1;
                assert_eq!(tem, 0, "a ilhota do Colosso tem {tem} estorvos");
            } else {
                assert!(
                    tem > 0,
                    "{} ficou sem recurso: o corte vazou",
                    i.bonus.nome()
                );
            }
        }
        assert_eq!(limpas, 1, "esperava exatamente uma ilhota limpa");
    }

    /// E ILHA NORMAL NÃO PERDE NADA NAS MESMAS COORDENADAS.
    ///
    /// `ilhota_em` responde por COORDENADA, e `estorvos_da_coluna` roda em
    /// toda ilha. Sem a checagem de zona, a ilha comum perderia os recursos
    /// das colunas que caem onde ficam as ilhotas — um apagão silencioso.
    ///
    /// O teste compara o MESMO ponto com e sem a regra: contar "> 0" não
    /// bastava, porque sobra recurso em volta de qualquer jeito. Foi assim
    /// que a primeira versão passou com o vazamento.
    #[test]
    fn ilha_comum_nao_perde_recurso_nas_coordenadas_das_ilhotas() {
        let def = crate::terreno::def_da_zona("ilha_inicial").expect("zona");
        let ilha = crate::terreno::Ilha::da_ilha(def);
        let colosso = ilhotas()
            .into_iter()
            .find(|i| matches!(i.bonus, Bonus::DropDeChefe))
            .expect("há ilhota do Colosso");
        // Na ilha COMUM, o ponto que na mágica seria do Colosso tem que ter a
        // mesma cara dos vizinhos — a regra não pode alcançar aqui.
        let no_ponto = conta_estorvos(&ilha, colosso.centro);
        let vizinho = conta_estorvos(&ilha, colosso.centro + Vec2::new(90.0, 0.0));
        assert!(
            no_ponto > 0 || vizinho == 0,
            "ilha comum ficou limpa em {:.0},{:.0} enquanto o vizinho tem {vizinho}: a regra vazou",
            colosso.centro.x,
            colosso.centro.y
        );
    }

    /// Quantos estorvos há num quadrado de 40 unidades em volta de `centro`.
    fn conta_estorvos(ilha: &crate::terreno::Ilha, centro: Vec2) -> usize {
        use crate::terreno::BLOCO;
        let mut n = 0;
        let passo = 4;
        let raio = (20.0 / BLOCO) as i32;
        let (cx, cz) = ((centro.x / BLOCO) as i32, (centro.y / BLOCO) as i32);
        let mut bz = cz - raio;
        while bz < cz + raio {
            let mut bx = cx - raio;
            while bx < cx + raio {
                let p = Vec2::new(bx as f32 * BLOCO, bz as f32 * BLOCO);
                if ilha.estorvo_em(p, 0.1).is_some() {
                    n += 1;
                }
                bx += passo;
            }
            bz += passo;
        }
        n
    }
}
