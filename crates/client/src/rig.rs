//! O personagem em peças: hierarquia, pivôs e a pose procedural.
//!
//! O corpo é feito de dez peças rígidas (mais o chapéu), cada uma girando em
//! volta de uma junta — ver `docs/character create.md` para as medidas. A
//! locomoção sai toda de código: fase do passo pela distância andada, sem
//! clipe gravado, então ela nunca dessincroniza da velocidade que o servidor
//! manda.
//!
//! O andar é o mesmo que foi aprovado na página de comparação: coxa balançando
//! ~26°, joelho dobrando quando a perna volta, braço oposto à perna,
//! antebraço levemente dobrado e o corpo subindo um pouco a cada passo.

use macroquad::prelude::*;

/// Quantas peças o rig desenha.
pub const N: usize = 11;

/// (nome, índice do pai, pivô em coordenada de voxel da tela comum).
///
/// As pernas penduram na RAIZ e não no tronco: inclinar o tronco pra correr
/// não pode levar as pernas junto. A ordem importa — pai sempre antes do filho.
pub const PECAS: [(&str, Option<usize>, [f32; 3]); N] = [
    ("torso", None, [16.0, 12.0, 20.0]),
    ("cabeca", Some(0), [16.0, 12.0, 33.0]),
    ("cabelo", Some(1), [16.0, 12.0, 33.0]),
    ("braco_d", Some(0), [23.0, 12.0, 31.0]),
    ("antebraco_d", Some(3), [23.0, 12.0, 25.0]),
    ("braco_e", Some(0), [9.0, 12.0, 31.0]),
    ("antebraco_e", Some(5), [9.0, 12.0, 25.0]),
    ("coxa_d", None, [18.0, 12.0, 19.0]),
    ("canela_d", Some(7), [18.0, 12.0, 10.0]),
    ("coxa_e", None, [14.0, 12.0, 19.0]),
    ("canela_e", Some(9), [14.0, 12.0, 10.0]),
];
const TORSO: usize = 0;
const BRACO_D: usize = 3;
const ANTEBRACO_D: usize = 4;
const BRACO_E: usize = 5;
const ANTEBRACO_E: usize = 6;
const COXA_D: usize = 7;
const CANELA_D: usize = 8;
const COXA_E: usize = 9;
const CANELA_E: usize = 10;

/// Onde fica a raiz do rig na tela comum: no chão, no eixo do corpo.
const RAIZ: [f32; 3] = [16.0, 12.0, 0.0];

/// Coxa: do quadril (z 19) ao joelho (z 10). Canela: do joelho à sola (z 0).
const COXA: f32 = 9.0;
const CANELA: f32 = 10.0;
const QUADRIL: f32 = 19.0;

/// Pivô de uma peça pelo nome — o que o carregamento do `.vox` precisa pra
/// gerar a malha em volta dela.
///
/// `None` pra nome fora do contrato, e isso importa: até 21/09/2026 esta
/// função devolvia a RAIZ (o chão, entre os pés) pra qualquer nome
/// desconhecido. Uma peça mal-nomeada virava malha em volta do chão e
/// **nunca era desenhada** — `desenha_rig` percorre `PECAS`, não o arquivo.
/// Duas falhas caladas empilhadas, bem no caminho de quem vai escrever
/// skins com `capuz` e `elmo` (docs/ARTE_DO_PERSONAGEM.md).
pub fn pivo(nome: &str) -> Option<[f32; 3]> {
    let slot = slot_de(nome)?;
    PECAS.iter().find(|p| p.0 == slot).map(|p| p.2)
}

/// A peça do rig que este objeto ocupa.
///
/// Quase sempre é o próprio nome. A exceção é o slot `cabelo`, que recebe
/// cabelo, chapéu, capuz e elmo — são a mesma junta na cabeça, e quem estiver
/// lá ganha (docs/ARTE_DO_PERSONAGEM.md, skins de armadura).
pub fn slot_de(nome: &str) -> Option<&'static str> {
    match nome {
        "capuz" | "elmo" | "chapeu" => Some("cabelo"),
        _ => PECAS.iter().find(|p| p.0 == nome).map(|p| p.0),
    }
}

/// Coordenada de voxel da tela comum → mundo, relativa à raiz. É o mesmo mapa
/// da malha (`-x, z, y`): rotação, não espelho.
pub fn mapa(v: [f32; 3], voxel: f32) -> Vec3 {
    vec3(-(v[0] - RAIZ[0]), v[2] - RAIZ[2], v[1] - RAIZ[1]) * voxel
}

/// O que o corpo está fazendo agora. Tudo aqui vem do que o cliente já sabe:
/// velocidade, altura, se está no ar.
pub struct Entrada {
    /// Fase do passo em radianos. Anda com a DISTÂNCIA percorrida, não com o
    /// tempo — parado, a perna para junto.
    pub fase: f32,
    /// 0 parado .. 1 andando, suavizado.
    pub andar: f32,
    /// 0 andando .. 1 correndo, suavizado.
    pub correr: f32,
    /// Relógio, só pra respiração de quem está parado.
    pub tempo: f32,
    /// 0 no chao .. 1 no ar, suavizado. Descer um degrau tambem poe o corpo
    /// no ar por um instante; sem suavizar, a pose trocaria num estalo.
    pub ar: f32,
    /// Quanto o chão sob cada pé (direito, esquerdo) está ACIMA da base do
    /// corpo, em voxels. Positivo = degrau.
    pub degrau: [f32; 2],
    /// Arma, golpe e dano (ver `Combate`).
    pub combate: Combate,
}

/// Rotação de cada peça em volta do próprio pivô, e quanto o corpo sobe.
pub struct Pose {
    pub rot: [Quat; N],
    /// Subida da raiz, em voxels.
    pub subida: f32,
    /// Giro de cada arma dentro da mao (direita, esquerda). A mao nao tem
    /// pulso: sai da conta, pra lamina apontar pra onde a chave manda.
    pub punho: [Quat; 2],
    /// A arma esta' na mao (senao, guardada no corpo).
    pub na_mao: bool,
    /// Tem arma pra desenhar (o conjunto ja' tem modelo).
    pub armado: bool,
    /// Qual conjunto (`shared::skills::Conjunto as u8`).
    pub conjunto: u8,
    /// Coletando: a ferramenta na mao direita (`FERRAMENTAS`) no lugar da
    /// arma.
    pub ferramenta: Option<&'static str>,
    /// O ARCO, na mao ESQUERDA. Nao e' um `conjunto`: o arco e' de MOB
    /// (`aplica_arqueiro`), e conjunto e' o que o jogador equipa.
    ///
    /// Existe porque o rig do humanoide sai "sem a arma" (as dez pecas
    /// nomeadas), e o arqueiro ficava fazendo o gesto de puxar no VAZIO.
    pub arco: bool,
}

impl Pose {
    fn de(rot: [Quat; N], subida: f32) -> Pose {
        Pose {
            rot,
            subida,
            punho: [Quat::IDENTITY; 2],
            na_mao: false,
            armado: false,
            conjunto: 0,
            ferramenta: None,
            arco: false,
        }
    }
}

/// Membro que aponta pra baixo vai pra FRENTE (o +Z do rig). Girar em +X
/// levaria pra trás, por isso o sinal.
fn frente(a: f32) -> Quat {
    Quat::from_rotation_x(-a)
}

/// Joelho dobra a canela pra TRÁS.
fn dobra_pra_tras(a: f32) -> Quat {
    Quat::from_rotation_x(a)
}

/// Braço abrindo pro lado. `lado` = +1 direito (que é o -X do mundo), -1 esquerdo.
fn abre(lado: f32, a: f32) -> Quat {
    Quat::from_rotation_z(-lado * a)
}

/// Joelho e quadril pra um pé tocar um chão `delta` voxels acima da base.
///
/// IK de dois ossos, conta pura: com o quadril na altura `QUADRIL` e o chão
/// subindo `delta`, a distância do quadril à sola vira `QUADRIL - delta`; a
/// lei dos cossenos dá quanto a coxa vai pra frente e quanto o joelho dobra
/// pra sola cair de novo embaixo do quadril. Devolve (coxa, joelho).
pub fn joelho_no_degrau(delta: f32) -> (f32, f32) {
    let d = (QUADRIL - delta.clamp(0.0, 14.0)).max(1.0);
    if d >= COXA + CANELA {
        return (0.0, 0.0);
    }
    let cos_coxa = (COXA * COXA + d * d - CANELA * CANELA) / (2.0 * COXA * d);
    let cos_joelho = (COXA * COXA + CANELA * CANELA - d * d) / (2.0 * COXA * CANELA);
    let coxa = cos_coxa.clamp(-1.0, 1.0).acos();
    let joelho = std::f32::consts::PI - cos_joelho.clamp(-1.0, 1.0).acos();
    (coxa, joelho)
}

fn pose_do_corpo(e: &Entrada) -> Pose {
    let chao = pose_no_chao(e);
    let ar = e.ar.clamp(0.0, 1.0);
    if ar <= 0.0 {
        return chao;
    }
    let voo = pose_no_ar(e.tempo);
    let mut rot = [Quat::IDENTITY; N];
    for i in 0..N {
        rot[i] = chao.rot[i].slerp(voo.rot[i], ar);
    }
    Pose::de(rot, chao.subida * (1.0 - ar))
}

/// O corpo no ar. NAO e' pulo: o jogo nao tem pulo atletico — o que tira o
/// pe' do chao e' vencer um degrau de dois ou tres blocos, ou cair de uma
/// borda.
///
/// Quatro tentativas ate' aqui, e o que cada uma ensinou:
///  1. encolher a perna subindo e jogar o braco caindo — pose de pulo, conta
///     uma historia que nao acontece;
///  2. corpo solto e PARADO — duro no ar;
///  3. perna e braco indo e voltando pra frente e pra tras — parece ANDAR no
///     ar: perna que TROCA de lado le' como passo;
///  4. tudo de lado (perna aberta, braco aberto pra baixo) — nao agradou.
///
/// O que ficou: o instante de uma corrida CONGELADO. Uma perna na frente, a
/// outra atras com o joelho dobrado, e elas NAO trocam de lado — so' a
/// abertura balanca um pouco. Bracos abertos, balancando leve como quem se
/// equilibra.
fn pose_no_ar(tempo: f32) -> Pose {
    let mut rot = [Quat::IDENTITY; N];
    let s = (tempo * std::f32::consts::TAU * AR_BALANCO_HZ).sin();
    // Abertura da passada congelada: abre e fecha um pouco, sem trocar de lado.
    let abertura = 0.33 + 0.07 * s;
    rot[COXA_D] = frente(abertura);
    rot[CANELA_D] = dobra_pra_tras(0.25);
    rot[COXA_E] = frente(-abertura);
    rot[CANELA_E] = dobra_pra_tras(0.45);
    // Bracos abertos, balancando leve.
    let braco = 0.75 + 0.10 * s;
    rot[BRACO_D] = abre(1.0, braco);
    rot[BRACO_E] = abre(-1.0, braco);
    rot[ANTEBRACO_D] = frente(0.2);
    rot[ANTEBRACO_E] = frente(0.2);
    Pose::de(rot, 0.0)
}

/// Balancos por segundo no ar. O ar dura ~0,6 s (o arco do degrau): da' pouco
/// menos de um balanco por subida — balancadinho, nao pedalada.
const AR_BALANCO_HZ: f32 = 1.6;

fn pose_no_chao(e: &Entrada) -> Pose {
    let mut rot = [Quat::IDENTITY; N];
    let andar = e.andar.clamp(0.0, 1.0);
    let correr = e.correr.clamp(0.0, 1.0);
    let s = e.fase.sin();
    let passo = (0.45 + 0.30 * correr) * andar * s;
    let dobra = (0.6 + 0.5 * correr) * andar;
    let mut coxa = [passo, -passo];
    let mut joelho = [
        dobra * (-(e.fase - 0.7).sin()).max(0.0),
        dobra * ((e.fase - 0.7).sin()).max(0.0),
    ];

    // Pé no degrau: o joelho dobra pra plantar a sola no bloco de cima.
    for (i, d) in e.degrau.iter().enumerate() {
        if *d > 0.5 {
            let (c, j) = joelho_no_degrau(*d);
            coxa[i] += c;
            joelho[i] += j;
        }
    }
    rot[COXA_D] = frente(coxa[0]);
    rot[COXA_E] = frente(coxa[1]);
    rot[CANELA_D] = dobra_pra_tras(joelho[0]);
    rot[CANELA_E] = dobra_pra_tras(joelho[1]);

    // Braço oposto à perna; correndo, balança mais e dobra o cotovelo.
    let respira = (e.tempo * std::f32::consts::TAU * 0.3).sin();
    let braco = -(0.8 + 0.2 * correr) * passo + 0.03 * respira * (1.0 - andar);
    rot[BRACO_D] = frente(braco);
    rot[BRACO_E] = frente(-braco);
    let cotovelo = 0.12 + (0.13 + 1.0 * correr) * andar;
    rot[ANTEBRACO_D] = frente(cotovelo);
    rot[ANTEBRACO_E] = frente(cotovelo);

    // Correndo, o tronco inclina pra frente (pra frente = topo indo pra +Z).
    rot[TORSO] = Quat::from_rotation_x(0.18 * correr * andar);

    let subida = (0.5 + 0.7 * correr) * andar * s.abs() + 0.25 * respira * (1.0 - andar);
    Pose::de(rot, subida)
}

/// Impulso baixo: tronco adiantado, perna dianteira dobrada e bracos para tras.
/// Sobrepoe a pose de combate para o Dash interromper visualmente o golpe.
pub fn aplica_dash(p: &mut Pose) {
    p.rot.fill(Quat::IDENTITY);
    p.rot[TORSO] = Quat::from_rotation_x(0.65);
    p.rot[BRACO_D] = Quat::from_rotation_x(-0.65);
    p.rot[BRACO_E] = Quat::from_rotation_x(-0.50);
    p.rot[ANTEBRACO_D] = Quat::from_rotation_x(0.45);
    p.rot[ANTEBRACO_E] = Quat::from_rotation_x(0.55);
    p.rot[COXA_D] = Quat::from_rotation_x(0.75);
    p.rot[CANELA_D] = Quat::from_rotation_x(-1.0);
    p.rot[COXA_E] = Quat::from_rotation_x(-0.55);
    p.rot[CANELA_E] = Quat::from_rotation_x(-0.25);
    p.subida = -2.0;
    p.ferramenta = None;
}

/// A matriz de mundo de cada peça, pai antes do filho.
///
/// `base` já traz posição e direção da entidade. Cada peça é
/// `pai · desloca(pivô − pivô do pai) · gira`, e a malha dela nasceu em volta
/// do próprio pivô (`vox::mesh_na_origem`) — por isso girar a peça gira em
/// volta da junta, e o filho vai junto.
pub fn matrizes(p: &Pose, base: Mat4, voxel: f32) -> [Mat4; N] {
    let raiz = base * Mat4::from_translation(vec3(0.0, p.subida * voxel, 0.0));
    let mut m = [Mat4::IDENTITY; N];
    for (i, (_, pai, piv)) in PECAS.iter().enumerate() {
        let (mp, pp) = match pai {
            Some(j) => (m[*j], PECAS[*j].2),
            None => (raiz, RAIZ),
        };
        let desloca = mapa(*piv, voxel) - mapa(pp, voxel);
        m[i] = mp * Mat4::from_translation(desloca) * Mat4::from_quat(p.rot[i]);
    }
    if p.armado && p.na_mao {
        if p.ferramenta.is_some() {
            segura_duas_maos(p, &mut m, voxel, MAOS_NA_FERRAMENTA);
        } else if p.conjunto == KATANA {
            segura_duas_maos(p, &mut m, voxel, MAOS_NA_KATANA);
        }
    }
    m
}

/// Voxels entre a mao direita e a esquerda no cabo da katana (6 e 1).
const MAOS_NA_KATANA: f32 = 5.0;
/// Voxels entre as maos no cabo do machado e da picareta: a esquerda no
/// marcador (voxel 3 do cabo, perto da ponta), a direita 6 acima, perto da
/// cabeca. Coleta e' com as DUAS maos — golpe de ferramenta pesada com uma so'
/// le' como brinquedo.
pub const MAOS_NA_FERRAMENTA: f32 = 6.0;

/// Resolve os dois bracos depois das molas: as maos continuam no mesmo cabo
/// mesmo quando o golpe, a locomocao e o impacto recebido se misturam. Vale
/// pra katana e pras ferramentas de coleta; `entre_maos` e' a distancia, em
/// voxels, da direita (perto da lamina/cabeca) ate' a esquerda (perto da
/// ponta do cabo), ao longo do eixo da arma.
///
/// E' IK de dois ossos: o cabo e' trazido pro alcance dos DOIS ombros, e cada
/// cotovelo sai da lei dos cossenos com o polo aberto pro lado e pra baixo.
fn segura_duas_maos(p: &Pose, m: &mut [Mat4; N], voxel: f32, entre_maos: f32) {
    let torso = m[TORSO];
    let local = torso.inverse();
    let ombro = |i: usize| mapa(PECAS[i].2, voxel) - mapa(PECAS[TORSO].2, voxel);
    let sd = ombro(BRACO_D);
    let se = ombro(BRACO_E);
    let direcao = (p.rot[BRACO_D] * p.rot[ANTEBRACO_D] * p.punho[0]) * Vec3::Z;
    let separacao = direcao * (entre_maos * voxel);
    let mut direita = local.transform_point3(
        m[ANTEBRACO_D].transform_point3(mapa(MAO_D, voxel) - mapa(PECAS[ANTEBRACO_D].2, voxel)),
    );
    // A direita fica perto da guarda e a esquerda junto ao pomo. Trazemos
    // o cabo para o alcance dos DOIS ombros, sem esticar os membros.
    let alcance = 13.4 * voxel;
    for _ in 0..64 {
        for centro in [sd, se + separacao] {
            let delta = direita - centro;
            if delta.length() > alcance {
                direita = centro + delta.normalize() * alcance;
            }
        }
        if direita.distance(sd) <= alcance + voxel * 0.0001 {
            break;
        }
    }
    for (braco, antebraco, origem, alvo, lado) in [
        (BRACO_D, ANTEBRACO_D, sd, direita, -1.0),
        (BRACO_E, ANTEBRACO_E, se, direita - separacao, 1.0),
    ] {
        let delta = alvo - origem;
        let distancia = delta.length();
        let eixo = delta / distancia;
        let superior = 6.0 * voxel;
        let inferior = 7.5 * voxel;
        let ao_longo =
            (superior * superior - inferior * inferior + distancia * distancia) / (2.0 * distancia);
        let altura = (superior * superior - ao_longo * ao_longo).max(0.0).sqrt();
        // Cotovelos abertos para os lados e baixos, fora do peito.
        let polo = vec3(lado, -0.5, -0.25);
        let dobra = (polo - eixo * polo.dot(eixo)).normalize();
        let cotovelo = origem + eixo * ao_longo + dobra * altura;
        let superior_rot = Quat::from_rotation_arc(-Vec3::Y, (cotovelo - origem).normalize());
        let inferior_rot = Quat::from_rotation_arc(-Vec3::Y, (alvo - cotovelo).normalize());
        m[braco] = torso * Mat4::from_translation(origem) * Mat4::from_quat(superior_rot);
        m[antebraco] = torso * Mat4::from_translation(cotovelo) * Mat4::from_quat(inferior_rot);
    }
}

/// A pose do quadro: a locomocao e, por cima dela, o combate.
/// Montado (docs/MONTARIAS.md): pernas abertas pros lados do bicho com o
/// joelho dobrado, maos juntas na frente (a redea), tronco um pouco pra
/// frente. A arma fica guardada e nada anda: quem anda e' a montaria.
pub fn aplica_montado(p: &mut Pose, tempo: f32, em_combate: bool) {
    let s = (tempo * std::f32::consts::TAU * 0.8).sin();
    // A coxa ABRE muito (≈49°) antes de ir pra frente: com pouca abertura a
    // perna sobe POR CIMA do lombo e entra no corpo do bicho. Menos `frente`
    // deixa a coxa deitada no flanco, e o joelho dobra menos junto.
    p.rot[COXA_D] = abre(1.0, 1.15) * frente(0.6);
    p.rot[COXA_E] = abre(-1.0, 1.15) * frente(0.6);
    p.rot[CANELA_D] = dobra_pra_tras(1.3);
    p.rot[CANELA_E] = dobra_pra_tras(1.3);
    p.subida = 0.0;
    p.ferramenta = None;
    if em_combate { return; }
    p.rot[BRACO_D] = frente(0.7 + 0.04 * s);
    p.rot[BRACO_E] = frente(0.7 + 0.04 * s);
    p.rot[ANTEBRACO_D] = frente(0.6);
    p.rot[ANTEBRACO_E] = frente(0.6);
    p.rot[TORSO] = Quat::from_rotation_x(0.12);
    p.subida = 0.0;
    p.na_mao = false;
    p.ferramenta = None;
}

/// Altura do quadril do rig (u): o que senta na sela.
pub fn altura_do_quadril(voxel: f32) -> f32 {
    QUADRIL * voxel
}

pub fn pose(e: &Entrada) -> Pose {
    let mut p = pose_do_corpo(e);
    aplica_combate(&mut p, e);
    p
}

// ═══════════════════════════════════════════════════════════════════════
//  COMBATE — espada e escudo
// ═══════════════════════════════════════════════════════════════════════
//
// Pose-chave por TABELA (docs/PERSONAGEM.md, "A animação"): cada chave diz
// pra onde os bracos apontam, quanto o tronco torce e pra onde a lamina
// aponta, e o cliente interpola. Nenhum quadro desenhado.
//
// Braco e lamina se descrevem por GUINADA e ELEVACAO — elevacao 0 = caido,
// pi/2 = na horizontal pra frente, pi = pra cima; guinada positiva = pra
// esquerda do boneco. Interpolar esses numeros, e nao o quaternion, e' o que
// faz o braco VARRER em arco num corte horizontal, em vez de cortar caminho
// por dentro do peito.
//
// A lamina tem direcao PROPRIA na chave, no espaco do tronco: a mao nao tem
// pulso, e o giro da arma dentro dela sai da conta (`Pose::punho`). A chave
// diz "a lamina aponta pra esquerda com o fio na frente", e nao "gire o
// antebraco 37 graus", que ninguem consegue ler nem acertar.

/// O que o combate pede do corpo neste quadro (ver `world::Ent`).
#[derive(Clone, Copy, Debug, Default)]
pub struct Combate {
    /// Conjunto na mao (`shared::skills::Conjunto as u8`).
    pub conjunto: u8,
    /// 0 = arma guardada .. 1 = na mao.
    pub sacada: f32,
    /// Golpe do combo em curso: (passo 0-2, segundos desde o comeco).
    pub golpe: Option<(u8, f32)>,
    /// Coletando: (tipo 0 madeira / 1..4 pedra, segundos desde o comeco).
    /// Guarda a arma e poe a ferramenta do tipo na mao.
    pub coleta: Option<(u8, f32)>,
    /// Skill confirmada: id, tempo e instante de impacto.
    pub skill: Option<(u32, f32, f32)>,
    /// O golpe que este interrompeu, CONGELADO no instante da troca: o novo
    /// parte de onde o anterior estava, entao a troca nao estala.
    pub golpe_ant: Option<(u8, f32)>,
    /// Segundos desde o ultimo golpe recebido.
    pub ferido: Option<f32>,
    /// Pra onde o golpe empurra (longe do atacante), no espaco do boneco.
    pub recuo: Vec3,
}

/// Quanto tempo sacar (ou guardar) leva.
pub const TEMPO_DE_SACAR: f32 = 0.4;

const PREPARA: f32 = shared::PLAYER_ATTACK_PREPARE_S;
const CORTA: f32 = shared::PLAYER_ATTACK_CUT_S;
const SEGURA: f32 = 0.04;
const VOLTA: f32 = 0.24;
/// Um golpe inteiro. A cadencia do ataque e' 0,25 s: o seguinte chega no meio
/// da VOLTA deste e parte dali (`Combate::golpe_ant`).
pub const DURACAO_DO_GOLPE: f32 = PREPARA + CORTA + SEGURA + VOLTA;

/// Os conjuntos (`shared::skills::Conjunto as u8`).
const ESPADA_ESCUDO: u8 = 0;
const KATANA: u8 = 1;
const PISTOLAS: u8 = 2;
const ANEL: u8 = 3;

/// Onde as armas se prendem, em voxel da tela comum.
const MAO_D: [f32; 3] = [23.0, 12.0, 17.5];
const MAO_E: [f32; 3] = [9.0, 12.0, 17.5];
/// O pulso: e' ali que o anel projeta o circulo.
const PULSO_D: [f32; 3] = [23.0, 12.0, 19.5];
const PULSO_E: [f32; 3] = [9.0, 12.0, 19.5];
/// Guardadas: a espada pendurada no quadril ESQUERDO — a mao direita saca
/// cruzando o corpo — e o escudo nas costas.
const QUADRIL_E: [f32; 3] = [10.2, 13.5, 21.5];
const COSTAS: [f32; 3] = [16.0, 8.4, 26.0];
/// A boca da bainha, no quadril esquerdo um pouco a' frente; os coldres, um
/// de cada lado, atras do braco (docs/ARTE_DO_PERSONAGEM.md).
const BAINHA: [f32; 3] = [9.6, 14.5, 21.5];
const COLDRE_D: [f32; 3] = [21.8, 11.0, 22.0];
const COLDRE_E: [f32; 3] = [10.2, 11.0, 22.0];

#[derive(Clone, Copy, Debug, PartialEq)]
struct Braco {
    guinada: f32,
    elevacao: f32,
    cotovelo: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct Lamina {
    guinada: f32,
    elevacao: f32,
    /// Giro em volta do proprio eixo: 0 = o fio vai pra onde a elevacao
    /// SOBE; pi = pra onde desce (golpe de cima); +-pi/2 = fio de lado, a
    /// lamina deitada (corte horizontal). Na pistola, 0 = em pe'.
    giro: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct Chave {
    torce: f32,
    inclina: f32,
    d: Braco,
    e: Braco,
    /// A arma da mao direita (lamina, cano).
    lamina: Lamina,
    /// A arma da mao esquerda quando ela nao e' escudo (a segunda pistola).
    arma_e: Lamina,
    /// Pra onde a face do escudo olha: guinada, 0 = frente.
    escudo: f32,
    /// 0..1: o passo a' frente.
    avanco: f32,
    /// Quanto o corpo desce (negativo) ou sobe, em voxels. O peso cai NO
    /// golpe: e' isso que faz o corte ter massa.
    agacha: f32,
}

const fn br(guinada: f32, elevacao: f32, cotovelo: f32) -> Braco {
    Braco {
        guinada,
        elevacao,
        cotovelo,
    }
}
const fn la(guinada: f32, elevacao: f32, giro: f32) -> Lamina {
    Lamina {
        guinada,
        elevacao,
        giro,
    }
}
/// Uma chave, na ordem: torcao, inclinacao, braco direito, braco esquerdo,
/// arma direita, arma esquerda, escudo, avanco, agacha.
#[allow(clippy::too_many_arguments)]
const fn ch(
    torce: f32,
    inclina: f32,
    d: Braco,
    e: Braco,
    lamina: Lamina,
    arma_e: Lamina,
    escudo: f32,
    avanco: f32,
    agacha: f32,
) -> Chave {
    Chave {
        torce,
        inclina,
        d,
        e,
        lamina,
        arma_e,
        escudo,
        avanco,
        agacha,
    }
}
const MEIA_VOLTA: f32 = std::f32::consts::FRAC_PI_2;
const PI: f32 = std::f32::consts::PI;
/// Chave sem arma na mao esquerda / sem lamina (o anel).
const SEM: Lamina = la(0.0, 1.5, 0.0);

/// Um conjunto inteiro: a guarda, o gesto de sacar e os tres golpes, cada um
/// (preparo, impacto). Cada golpe comeca onde o anterior termina.
struct Estilo {
    guarda: Chave,
    sacando: Chave,
    golpes: [[Chave; 2]; 3],
}

/// Espada e escudo. Guarda: espada erguida a' frente com o fio pra baixo,
/// escudo cobrindo o peito. Golpes: horizontal da direita pra esquerda, de
/// volta subindo, e de cima com o passo a' frente.
const ESTILO_ESPADA: Estilo = Estilo {
    guarda: ch(
        -0.15,
        0.05,
        br(-0.35, 0.45, 1.25),
        br(-0.15, 0.55, 1.35),
        la(-0.2, 1.9, PI),
        SEM,
        0.2,
        0.0,
        -0.5,
    ),
    // a mao direita no quadril esquerdo, a esquerda por cima do ombro
    sacando: ch(
        0.3,
        0.05,
        br(0.9, 0.35, 1.0),
        br(0.3, 2.6, 1.6),
        la(0.0, -0.5, 0.0),
        SEM,
        PI,
        0.0,
        0.0,
    ),
    golpes: [
        [
            ch(
                -0.75,
                0.0,
                br(-1.4, 1.35, 0.5),
                br(-0.1, 0.5, 1.4),
                la(-1.6, 1.5, MEIA_VOLTA),
                SEM,
                0.3,
                0.0,
                0.3,
            ),
            ch(
                0.75,
                0.05,
                br(0.9, 1.45, 0.25),
                br(0.5, 0.4, 1.1),
                la(1.1, 1.55, MEIA_VOLTA),
                SEM,
                0.7,
                0.0,
                -1.4,
            ),
        ],
        [
            ch(
                0.75,
                0.05,
                br(1.1, 1.1, 0.7),
                br(0.5, 0.4, 1.1),
                la(1.3, 1.2, -MEIA_VOLTA),
                SEM,
                0.7,
                0.0,
                0.2,
            ),
            ch(
                -0.65,
                -0.05,
                br(-1.1, 1.8, 0.3),
                br(-0.1, 0.5, 1.4),
                la(-1.2, 1.9, -MEIA_VOLTA),
                SEM,
                0.2,
                0.0,
                -1.2,
            ),
        ],
        [
            ch(
                -0.1,
                -0.25,
                br(-0.2, 2.8, 0.6),
                br(-0.1, 0.6, 1.3),
                la(-0.1, 3.3, PI),
                SEM,
                0.2,
                0.3,
                0.8,
            ),
            ch(
                0.05,
                0.35,
                br(-0.1, 1.2, 0.15),
                br(0.2, 0.35, 1.0),
                la(-0.05, 1.15, PI),
                SEM,
                0.4,
                1.0,
                -2.2,
            ),
        ],
    ],
};

/// Katana: guarda firme, a lamina erguida a' frente na diagonal. Golpes: diagonal
/// descendo do ombro direito (kesa-giri), diagonal subindo de volta
/// (gyaku-kesa) e a estocada com o passo longo (tsuki).
const ESTILO_KATANA: Estilo = Estilo {
    guarda: ch(
        -0.15,
        0.03,
        br(-0.1, 0.75, 1.2),
        br(0.35, 0.3, 0.7),
        la(-0.1, 2.15, PI),
        SEM,
        0.0,
        0.0,
        -0.6,
    ),
    // a mao direita no cabo, na boca da bainha; a esquerda segura a bainha
    sacando: ch(
        0.35,
        0.05,
        br(0.95, 0.45, 1.1),
        br(0.55, 0.3, 0.8),
        la(0.15, -1.12, 0.0),
        SEM,
        0.0,
        0.0,
        -0.4,
    ),
    golpes: [
        [
            ch(
                -0.55,
                -0.1,
                br(-0.8, 2.5, 0.9),
                br(0.3, 0.5, 1.0),
                la(-0.5, 2.9, PI),
                SEM,
                0.0,
                0.0,
                0.4,
            ),
            ch(
                0.6,
                0.25,
                br(0.7, 0.95, 0.2),
                br(0.5, 0.3, 0.8),
                la(0.9, 0.85, PI),
                SEM,
                0.0,
                0.0,
                -1.8,
            ),
        ],
        [
            ch(
                0.6,
                0.2,
                br(0.9, 0.8, 0.5),
                br(0.5, 0.3, 0.8),
                la(1.0, 0.7, 0.0),
                SEM,
                0.0,
                0.0,
                -0.6,
            ),
            ch(
                -0.6,
                -0.05,
                br(-1.0, 2.05, 0.25),
                br(0.2, 0.5, 1.0),
                la(-1.1, 2.35, 0.0),
                SEM,
                0.0,
                0.0,
                -0.8,
            ),
        ],
        [
            ch(
                -0.45,
                0.0,
                br(-0.25, 1.0, 1.6),
                br(0.3, 0.9, 1.4),
                la(-0.1, 1.55, -MEIA_VOLTA),
                SEM,
                0.0,
                0.2,
                0.2,
            ),
            ch(
                0.25,
                0.2,
                br(0.0, 1.5, 0.05),
                br(0.5, 0.4, 0.8),
                la(0.0, 1.57, -MEIA_VOLTA),
                SEM,
                0.0,
                1.0,
                -2.0,
            ),
        ],
    ],
};

/// Duas pistolas: os dois bracos a' frente, os canos baixos. Golpes: tiro da
/// direita, tiro da esquerda, e as duas juntas — cada tiro com o COICE, o
/// braco e o cano subindo e o corpo cedendo.
const ESTILO_PISTOLAS: Estilo = Estilo {
    guarda: ch(
        0.0,
        0.05,
        br(-0.25, 0.75, 0.9),
        br(0.25, 0.75, 0.9),
        la(-0.1, 1.45, 0.0),
        la(0.1, 1.45, 0.0),
        0.0,
        0.0,
        -0.4,
    ),
    // as duas maos nos coldres
    sacando: ch(
        0.0,
        0.05,
        br(-0.35, 0.05, 0.4),
        br(0.35, 0.05, 0.4),
        la(0.0, 0.0, 0.0),
        la(0.0, 0.0, 0.0),
        0.0,
        0.0,
        -0.3,
    ),
    golpes: [
        [
            ch(
                -0.25,
                0.0,
                br(-0.1, 1.35, 0.35),
                br(0.25, 0.75, 0.9),
                la(-0.05, 1.5, 0.0),
                la(0.1, 1.45, 0.0),
                0.0,
                0.0,
                0.0,
            ),
            ch(
                -0.15,
                -0.08,
                br(-0.05, 1.8, 0.25),
                br(0.25, 0.75, 0.9),
                la(0.0, 2.0, 0.0),
                la(0.1, 1.45, 0.0),
                0.0,
                0.0,
                -0.7,
            ),
        ],
        [
            ch(
                0.25,
                0.0,
                br(-0.25, 0.8, 0.9),
                br(0.1, 1.35, 0.35),
                la(-0.1, 1.45, 0.0),
                la(0.05, 1.5, 0.0),
                0.0,
                0.0,
                0.0,
            ),
            ch(
                0.15,
                -0.08,
                br(-0.25, 0.8, 0.9),
                br(0.05, 1.8, 0.25),
                la(-0.1, 1.45, 0.0),
                la(0.0, 2.0, 0.0),
                0.0,
                0.0,
                -0.7,
            ),
        ],
        [
            ch(
                0.0,
                0.05,
                br(-0.12, 1.4, 0.4),
                br(0.12, 1.4, 0.4),
                la(-0.05, 1.5, 0.0),
                la(0.05, 1.5, 0.0),
                0.0,
                0.0,
                0.3,
            ),
            ch(
                0.0,
                -0.15,
                br(-0.08, 1.95, 0.3),
                br(0.08, 1.95, 0.3),
                la(-0.05, 2.1, 0.0),
                la(0.05, 2.1, 0.0),
                0.0,
                0.0,
                -1.2,
            ),
        ],
    ],
};

/// Anel magico: sem arma — as maos abertas a' frente do peito. Golpes: a
/// palma direita empurrando, as duas juntas, e a mao erguida descendo de uma
/// vez. O anel projeta o circulo no pulso (efeito, nao modelo).
const ESTILO_ANEL: Estilo = Estilo {
    guarda: ch(
        -0.1,
        0.05,
        br(-0.25, 0.95, 1.6),
        br(0.25, 0.95, 1.6),
        SEM,
        SEM,
        0.0,
        0.0,
        -0.3,
    ),
    // "sacar" o anel e' erguer a mao
    sacando: ch(
        0.0,
        0.0,
        br(-0.3, 1.9, 1.3),
        br(0.3, 0.9, 1.6),
        SEM,
        SEM,
        0.0,
        0.0,
        0.0,
    ),
    golpes: [
        [
            ch(
                -0.45,
                0.0,
                br(-0.45, 1.25, 1.95),
                br(0.3, 0.9, 1.6),
                SEM,
                SEM,
                0.0,
                0.0,
                0.2,
            ),
            ch(
                0.3,
                0.1,
                br(-0.05, 1.55, 0.05),
                br(0.35, 0.8, 1.5),
                SEM,
                SEM,
                0.0,
                0.0,
                -0.9,
            ),
        ],
        [
            ch(
                0.0,
                -0.05,
                br(-0.35, 1.1, 1.9),
                br(0.35, 1.1, 1.9),
                SEM,
                SEM,
                0.0,
                0.0,
                0.3,
            ),
            ch(
                0.0,
                0.1,
                br(-0.12, 1.5, 0.1),
                br(0.12, 1.5, 0.1),
                SEM,
                SEM,
                0.0,
                0.0,
                -1.0,
            ),
        ],
        [
            ch(
                -0.1,
                -0.2,
                br(-0.2, 2.95, 0.4),
                br(0.35, 0.6, 1.2),
                SEM,
                SEM,
                0.0,
                0.0,
                0.6,
            ),
            ch(
                0.05,
                0.3,
                br(-0.1, 1.0, 0.2),
                br(0.3, 0.5, 1.0),
                SEM,
                SEM,
                0.0,
                0.6,
                -1.8,
            ),
        ],
    ],
};

fn estilo(conjunto: u8) -> Option<&'static Estilo> {
    match conjunto {
        ESPADA_ESCUDO => Some(&ESTILO_ESPADA),
        KATANA => Some(&ESTILO_KATANA),
        PISTOLAS => Some(&ESTILO_PISTOLAS),
        ANEL => Some(&ESTILO_ANEL),
        _ => None,
    }
}

fn mistura(a: &Chave, b: &Chave, k: f32) -> Chave {
    let l = |x: f32, y: f32| x + (y - x) * k;
    let bra = |x: &Braco, y: &Braco| {
        br(
            l(x.guinada, y.guinada),
            l(x.elevacao, y.elevacao),
            l(x.cotovelo, y.cotovelo),
        )
    };
    let lam = |x: &Lamina, y: &Lamina| {
        la(
            l(x.guinada, y.guinada),
            l(x.elevacao, y.elevacao),
            l(x.giro, y.giro),
        )
    };
    Chave {
        torce: l(a.torce, b.torce),
        inclina: l(a.inclina, b.inclina),
        d: bra(&a.d, &b.d),
        e: bra(&a.e, &b.e),
        lamina: lam(&a.lamina, &b.lamina),
        arma_e: lam(&a.arma_e, &b.arma_e),
        escudo: l(a.escudo, b.escudo),
        avanco: l(a.avanco, b.avanco),
        agacha: l(a.agacha, b.agacha),
    }
}

/// Acelera, e PASSA do ponto (~10%) antes de assentar: o corte tem inercia.
/// A curva de passar do ponto sozinha sai com 4x a velocidade media no
/// primeiro instante — um tranco, nao um golpe. Correndo por cima de uma
/// suave, ela sai do zero, e o pico cai pra ~2,4x.
fn passa(u: f32) -> f32 {
    let g = suave(u);
    let (c1, c3) = (1.3, 2.3);
    1.0 + c3 * (g - 1.0).powi(3) + c1 * (g - 1.0).powi(2)
}

fn suave(u: f32) -> f32 {
    let u = u.clamp(0.0, 1.0);
    u * u * (3.0 - 2.0 * u)
}

/// A chave do golpe `passo` no instante `t`, saindo de `desde`.
///
/// Preparo sai rapido e chega devagar (a antecipacao); o corte acelera e
/// passa do ponto; segura um instante no impacto, que e' o quadro que o olho
/// guarda; e volta pra guarda.
fn chave_do_golpe(est: &Estilo, passo: u8, t: f32, desde: &Chave) -> Chave {
    let [prep, imp] = &est.golpes[passo.min(2) as usize];
    if t < PREPARA {
        let u = t / PREPARA;
        return mistura(desde, prep, 1.0 - (1.0 - u) * (1.0 - u));
    }
    if t < PREPARA + CORTA {
        return mistura(prep, imp, passa((t - PREPARA) / CORTA));
    }
    if t < PREPARA + CORTA + SEGURA {
        return *imp;
    }
    mistura(
        imp,
        &est.guarda,
        suave((t - PREPARA - CORTA - SEGURA) / VOLTA),
    )
}

fn ombro(b: &Braco) -> Quat {
    Quat::from_rotation_y(b.guinada) * Quat::from_rotation_x(-b.elevacao)
}

/// A arma no espaco do tronco. A malha tem a lamina (ou o cano) no +Z, o fio
/// (ou a pega) no +Y e a face chata no X; `x(pi/2)` a pendura apontando pra
/// baixo com o fio pra frente, e dai' guinada, elevacao e giro fazem o resto.
fn orienta_lamina(l: &Lamina) -> Quat {
    Quat::from_rotation_y(l.guinada)
        * Quat::from_rotation_x(-l.elevacao)
        * Quat::from_rotation_y(l.giro)
        * Quat::from_rotation_x(MEIA_VOLTA)
}

/// O escudo no espaco do tronco: a face (o +X da malha) olhando na guinada
/// pedida, em pe'.
fn orienta_escudo(guinada: f32) -> Quat {
    Quat::from_rotation_y(guinada - MEIA_VOLTA)
}

/// O tranco de quem apanhou: sobe em 0,05 s e some em ~0,3 s.
pub fn tranco(t: f32) -> f32 {
    if t < 0.05 {
        t / 0.05
    } else {
        (-(t - 0.05) / 0.10).exp()
    }
}

fn aplica_combate(p: &mut Pose, e: &Entrada) {
    if let Some((tipo, t)) = e.combate.coleta {
        aplica_coleta(p, tipo, t);
    } else {
        arma_na_mao(p, e);
    }
    // o golpe recebido vai POR CIMA de tudo, inclusive do golpe dado
    if let Some(t) = e.combate.ferido {
        aplica_ferido(p, t, e.combate.recuo);
    }
}

// ═══════════════════════════════════════════════════════════════════════
//  COLETA — machado e picareta
// ═══════════════════════════════════════════════════════════════════════

/// A ferramenta de cada tipo de coleta (0 madeira, 1..4 pedra pela cor da
/// pedra: a cabeca da picareta sai na cor do veio). `tools/voxrender/armas.py`.
pub const FERRAMENTAS: [&str; 5] = [
    "machado",
    "picareta_1",
    "picareta_2",
    "picareta_3",
    "picareta_4",
];

/// Um golpe de coleta. O ciclo do servidor (2,0 s no tronco, 2,5–3,4 s na
/// pedra) cabe um golpe e meio a dois: o impacto le' como "trabalho", sem
/// sincronia exata com o rendimento — que e' do servidor.
pub const PERIODO_DA_COLETA: f32 = 1.3;

pub fn ferramenta_de(tipo: u8) -> &'static str {
    FERRAMENTAS[(tipo as usize).min(FERRAMENTAS.len() - 1)]
}

/// Fase (0..1 do golpe) do IMPACTO: o fim da varrida do machado e o fim da
/// descida da picareta (`chave_da_coleta`). E' onde saem as lascas.
pub fn fase_do_impacto(tipo: u8) -> f32 {
    if tipo == 0 {
        0.55
    } else {
        0.58
    }
}

/// Onde a cabeca bate, no espaco da ferramenta (voxels a partir do marcador,
/// que fica na mao esquerda): a malha tem o cabo no +Z e o fio no +Y. Machado:
/// o fio da cunha (voxels 13–18 do cabo, fio 3 a frente). Picareta: o olho da
/// cabeca, no alto do cabo (voxel 17).
pub fn cabeca_da_ferramenta(tipo: u8) -> Vec3 {
    if tipo == 0 {
        vec3(0.0, 3.5, 12.5)
    } else {
        vec3(0.0, 0.0, 14.0)
    }
}

/// O braco direito, a ferramenta (no espaco do tronco) e o corpo (torce,
/// inclina, agacha) na fase `u` (0..1) de um golpe.
///
/// Picareta: sobe as duas maos acima da cabeca, desce de uma vez com o tronco
/// inclinando e o peso caindo, quica um pouco no impacto e volta.
/// Machado: arma de lado (a direita do boneco), varre em arco horizontal
/// torcendo o tronco, recua no impacto e volta.
fn chave_da_coleta(tipo: u8, u: f32) -> (Braco, Lamina, f32, f32, f32) {
    let u = u.clamp(0.0, 1.0);
    if tipo == 0 {
        // machado: guinada negativa = lado DIREITO do boneco
        let (g, torce) = if u < 0.40 {
            let k = suave(u / 0.40);
            (-1.3 * k, -0.5 * k)
        } else if u < 0.55 {
            let k = passa((u - 0.40) / 0.15);
            (-1.3 + 2.0 * k, -0.5 + 0.9 * k)
        } else if u < 0.63 {
            let k = suave((u - 0.55) / 0.08);
            (0.7 - 0.2 * k, 0.4 - 0.1 * k)
        } else {
            let k = suave((u - 0.63) / 0.37);
            (0.5 * (1.0 - k), 0.3 * (1.0 - k))
        };
        let d = br(g, 1.35, 0.35);
        return (
            d,
            la(g, 1.45, MEIA_VOLTA),
            torce,
            0.12,
            if (0.5..0.63).contains(&u) { -0.8 } else { 0.0 },
        );
    }
    // picareta: elevacao pi = pra cima
    let (e, lamina_e, inclina, agacha) = if u < 0.45 {
        let k = suave(u / 0.45);
        (0.9 + 2.0 * k, 1.1 + 2.0 * k, -0.1 * k, 0.4 * k)
    } else if u < 0.58 {
        let k = passa((u - 0.45) / 0.13);
        (2.9 - 2.3 * k, 3.1 - 2.9 * k, -0.1 + 0.5 * k, 0.4 - 1.8 * k)
    } else if u < 0.66 {
        let k = suave((u - 0.58) / 0.08);
        (
            0.6 + 0.15 * k,
            0.2 + 0.15 * k,
            0.4 - 0.05 * k,
            -1.4 + 0.2 * k,
        )
    } else {
        let k = suave((u - 0.66) / 0.34);
        (
            0.75 + 0.15 * k,
            0.35 + 0.75 * k,
            0.35 * (1.0 - k),
            -1.2 * (1.0 - k),
        )
    };
    (
        br(0.15, e, 0.3),
        la(0.15, lamina_e, PI),
        0.0,
        inclina,
        agacha,
    )
}

fn aplica_coleta(p: &mut Pose, tipo: u8, t: f32) {
    let u = (t.max(0.0) / PERIODO_DA_COLETA).fract();
    let (d, lamina, torce, inclina, agacha_d) = chave_da_coleta(tipo, u);
    p.armado = true;
    p.na_mao = true;
    p.ferramenta = Some(ferramenta_de(tipo));
    p.rot[TORSO] = Quat::from_rotation_y(torce) * Quat::from_rotation_x(inclina);
    p.rot[1] = Quat::from_rotation_y(-torce * 0.6);
    p.rot[BRACO_D] = ombro(&d);
    p.rot[ANTEBRACO_D] = frente(d.cotovelo);
    // A esquerda aqui e' so' o ponto de partida das molas: quem poe as DUAS
    // maos no cabo e' a IK em `matrizes` (`segura_duas_maos`).
    let e = br(
        d.guinada + 0.3,
        (d.elevacao - 0.2).max(0.3),
        d.cotovelo + 0.3,
    );
    p.rot[BRACO_E] = ombro(&e);
    p.rot[ANTEBRACO_E] = frente(e.cotovelo);
    agacha(p, agacha_d);
    let cadeia = p.rot[BRACO_D] * p.rot[ANTEBRACO_D];
    p.punho = [cadeia.inverse() * orienta_lamina(&lamina), Quat::IDENTITY];
}

#[cfg(test)]
mod testes_de_coleta {
    use super::*;

    #[test]
    fn cada_tipo_segura_a_sua_ferramenta() {
        assert_eq!(ferramenta_de(0), "machado");
        for t in 1..=4 {
            assert_eq!(ferramenta_de(t), format!("picareta_{t}"));
        }
        assert_eq!(ferramenta_de(9), "picareta_4");
        let mut p = Pose::de([Quat::IDENTITY; N], 0.0);
        aplica_coleta(&mut p, 2, 0.1);
        assert_eq!(p.ferramenta, Some("picareta_2"));
        assert!(p.armado && p.na_mao);
    }

    /// As DUAS maos no cabo, em toda a volta do golpe (preparacao, impacto,
    /// recuo), pro machado e pra picareta: direita 6 voxels acima do marcador,
    /// esquerda no marcador, e os ossos do braco sem esticar.
    #[test]
    fn coleta_segura_o_cabo_com_as_duas_maos() {
        for voxel in [1.0, 0.04] {
            for tipo in [0u8, 1, 4] {
                for u in [0.0, 0.2, 0.44, fase_do_impacto(tipo), 0.62, 0.8, 0.99] {
                    let mut p = Pose::de([Quat::IDENTITY; N], 0.0);
                    aplica_coleta(&mut p, tipo, u * PERIODO_DA_COLETA);
                    let base =
                        Mat4::from_translation(vec3(2.0, 1.0, -3.0)) * Mat4::from_rotation_y(0.6);
                    let m = matrizes(&p, base, voxel);
                    let (nome, f) = armas(&p, &m, voxel)[0];
                    assert_eq!(nome, ferramenta_de(tipo));
                    for (braco, antebraco, mao, no_cabo) in [
                        (BRACO_D, ANTEBRACO_D, MAO_D, MAOS_NA_FERRAMENTA),
                        (BRACO_E, ANTEBRACO_E, MAO_E, 0.0),
                    ] {
                        let pos = m[antebraco]
                            .transform_point3(mapa(mao, voxel) - mapa(PECAS[antebraco].2, voxel));
                        let cabo = f.transform_point3(Vec3::Z * no_cabo * voxel);
                        assert!(
                            pos.distance(cabo) <= 0.06 * voxel / 0.04,
                            "tipo {tipo} u {u}: mao a {} do cabo",
                            pos.distance(cabo) / voxel
                        );
                        let ombro = m[braco].transform_point3(Vec3::ZERO);
                        let cotovelo = m[antebraco].transform_point3(Vec3::ZERO);
                        assert!(
                            (ombro.distance(cotovelo) / voxel - 6.0).abs() < 0.01,
                            "braco esticou"
                        );
                        assert!(
                            (cotovelo.distance(pos) / voxel - 7.5).abs() < 0.01,
                            "antebraco esticou"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn o_golpe_percorre_a_pose_e_volta() {
        // picareta: sobe ate' acima da cabeca, desce abaixo do comeco no
        // impacto, e termina o ciclo onde comecou.
        let (ini, _, _, _, _) = chave_da_coleta(1, 0.0);
        let (alto, _, _, _, _) = chave_da_coleta(1, 0.44);
        let (baixo, _, _, inclina, agacha) = chave_da_coleta(1, 0.58);
        let (fim, _, _, _, _) = chave_da_coleta(1, 1.0);
        assert!(
            alto.elevacao > 2.5,
            "levanta acima da cabeca: {}",
            alto.elevacao
        );
        assert!(baixo.elevacao < ini.elevacao, "desce no impacto");
        assert!(
            inclina > 0.3 && agacha < -1.0,
            "o corpo vai junto no impacto"
        );
        assert!(
            (fim.elevacao - ini.elevacao).abs() < 0.05,
            "volta ao comeco"
        );
        // machado: arma pra direita, varre pra esquerda, volta ao meio.
        let (a, _, _, _, _) = chave_da_coleta(0, 0.39);
        let (b, _, _, _, _) = chave_da_coleta(0, 0.55);
        let (c, _, _, _, _) = chave_da_coleta(0, 1.0);
        assert!(a.guinada < -1.0 && b.guinada > 0.5, "arco lateral");
        assert!(c.guinada.abs() < 0.05, "volta ao meio");
        // O relogio repete o golpe.
        let mut p1 = Pose::de([Quat::IDENTITY; N], 0.0);
        let mut p2 = Pose::de([Quat::IDENTITY; N], 0.0);
        aplica_coleta(&mut p1, 1, 0.3);
        aplica_coleta(&mut p2, 1, 0.3 + PERIODO_DA_COLETA);
        assert!(p1.rot[BRACO_D].abs_diff_eq(p2.rot[BRACO_D], 1e-4));
    }
}

/// O tranco de quem apanhou: o tronco e a cabeca vao pra LONGE do atacante,
/// os bracos abrem, os joelhos cedem e o corpo desce.
fn aplica_ferido(p: &mut Pose, t: f32, recuo: Vec3) {
    let k = tranco(t);
    if k <= 0.001 {
        return;
    }
    let recuo = if recuo.length_squared() > 0.01 {
        recuo.normalize()
    } else {
        vec3(0.0, 0.0, -1.0)
    };
    let eixo = Vec3::Y.cross(recuo).normalize_or_zero();
    p.rot[TORSO] = Quat::from_axis_angle(eixo, 0.5 * k) * p.rot[TORSO];
    p.rot[1] = Quat::from_axis_angle(eixo, 0.35 * k) * p.rot[1];
    p.rot[BRACO_D] = abre(1.0, 0.45 * k) * p.rot[BRACO_D];
    p.rot[BRACO_E] = abre(-1.0, 0.45 * k) * p.rot[BRACO_E];
    agacha(p, -1.6 * k);
}

/// Desce o corpo `-d` voxels dobrando os DOIS joelhos na conta do degrau (a
/// mesma lei dos cossenos): sem isso o pe' afundaria no chao. Subir, so' um
/// pouco — perna reta nao estica.
fn agacha(p: &mut Pose, d: f32) {
    if d < 0.0 {
        let (coxa, joelho) = joelho_no_degrau(-d);
        for (c, k) in [(COXA_D, CANELA_D), (COXA_E, CANELA_E)] {
            p.rot[c] = p.rot[c] * frente(coxa);
            p.rot[k] = p.rot[k] * dobra_pra_tras(joelho);
        }
        p.subida += d;
    } else {
        p.subida += d.min(0.5);
    }
}

/// A guarda VIVA: respira, a ponta da arma balanca, e andando os bracos
/// acompanham o passo e o tronco torce contra as pernas. Guarda parada lia
/// como boneco de vitrine segurando uma espada.
fn guarda_viva(est: &Estilo, e: &Entrada) -> Chave {
    let mut c = est.guarda;
    let (t, a) = (e.tempo, e.andar.clamp(0.0, 1.0));
    let passo2 = (e.fase * 2.0).sin();
    c.torce += 0.04 * (t * 1.1).sin() + 0.10 * a * e.fase.sin();
    c.d.elevacao += 0.05 * (t * 2.1).sin() + 0.10 * a * passo2;
    c.e.elevacao += 0.04 * (t * 2.1 + 0.4).sin() + 0.08 * a * (e.fase * 2.0 + 0.3).sin();
    c.lamina.elevacao += 0.07 * (t * 2.1 + 0.9).sin() + 0.12 * a * (e.fase * 2.0 + 0.6).sin();
    c.lamina.guinada += 0.04 * (t * 1.3).sin();
    c.arma_e.elevacao += 0.06 * (t * 2.1 + 1.3).sin() + 0.10 * a * (e.fase * 2.0 + 0.9).sin();
    c.escudo += 0.05 * (t * 1.7).sin();
    c
}

/// Altura do pulo do Salto (skill 1 da espada e escudo), em voxels.
const SALTO_PICO: f32 = 10.0;

/// Cada habilidade tem antecipacao, impacto e retorno proprios; o relogio
/// vem do mesmo catalogo que agenda o efeito no servidor.
fn chave_da_skill(est: &Estilo, id: u32, t: f32, impacto: f32) -> Chave {
    if t >= impacto + shared::skills::RECUPERACAO_S {
        return est.guarda;
    }
    let mut prep = est.guarda;
    let mut hit = est.guarda;
    match id {
        1 => {
            // SALTO: agacha com o escudo firme, a espada sobe no voo e desce
            // com o corpo na queda — o arco do pulo vem depois, em
            // `SALTO_PICO`, por cima da mistura.
            prep.e = br(-0.12, 1.05, 1.4);
            prep.d = br(-0.6, 0.65, 1.5);
            prep.inclina = 0.2;
            prep.agacha = -2.2;
            prep.escudo = 0.0;
            hit = prep;
            hit.d = br(0.0, 1.3, 0.12);
            hit.e = br(0.0, 1.45, 0.25);
            hit.inclina = 0.45;
            hit.avanco = 0.6;
            hit.agacha = -2.6;
        }
        2 => {
            prep = est.golpes[0][0];
            prep.torce = -0.85;
            prep.agacha = -0.8;
            hit = est.golpes[0][1];
            hit.torce = 0.85;
            hit.avanco = 0.8;
            hit.agacha = -1.8;
        }
        3 => {
            prep.e = br(-0.25, 0.7, 1.8);
            prep.agacha = -1.2;
            hit.e = br(0.0, 1.3, 0.8);
            hit.d = br(-0.25, 0.9, 1.4);
            hit.agacha = -2.0;
            hit.escudo = 0.0;
        }
        4 => {
            prep = est.golpes[1][0];
            prep.torce = 0.65;
            prep.agacha = -1.6;
            hit = est.golpes[1][1];
            hit.torce = -0.65;
            hit.avanco = 0.95;
            hit.lamina = la(-0.75, 1.6, -MEIA_VOLTA);
        }
        5 => {
            prep = est.golpes[0][0];
            prep.torce = -0.65;
            prep.agacha = -0.9;
            hit = est.golpes[1][1];
            hit.torce = -0.7;
            hit.avanco = 0.7;
            // Duas diagonais ligadas pelo quadril; nenhuma volta instantanea do tronco.
            let meio = est.golpes[0][1];
            if t < impacto * 0.34 {
                return mistura(&est.guarda, &prep, suave(t / (impacto * 0.34)));
            }
            if t < impacto * 0.65 {
                return mistura(&prep, &meio, suave((t / impacto - 0.34) / 0.31));
            }
            if t < impacto {
                return mistura(&meio, &hit, suave((t / impacto - 0.65) / 0.35));
            }
        }
        6 => {
            prep = est.golpes[0][0];
            prep.d = br(-0.15, 2.6, 0.9);
            prep.lamina = la(0.0, 2.9, PI);
            hit = est.golpes[0][1];
            hit.torce = 0.05;
            hit.d = br(0.0, 1.15, 0.2);
            hit.lamina = la(0.0, 1.05, PI);
            hit.avanco = 0.8;
        }
        7 | 8 => {
            prep = est.golpes[if id == 7 { 0 } else { 2 }][0];
            prep.agacha = -0.8;
            prep.torce = if id == 7 { -0.25 } else { 0.0 };
            hit = prep;
            // O cano aponta ao alvo no disparo; o coice acontece DEPOIS.
            if t >= impacto {
                let dt = t - impacto;
                let coice =
                    (dt / 0.045).min(1.0) * (1.0 - dt / shared::skills::RECUPERACAO_S).max(0.0);
                hit.d.elevacao += coice * 0.42;
                hit.lamina.elevacao += coice * 0.35;
                if id == 8 {
                    hit.e.elevacao += coice * 0.38;
                    hit.arma_e.elevacao += coice * 0.35;
                }
                hit.inclina -= coice * 0.12;
                hit.agacha -= coice * 0.7;
                return mistura(
                    &hit,
                    &est.guarda,
                    suave((dt / shared::skills::RECUPERACAO_S).clamp(0.0, 1.0)),
                );
            }
        }
        9 => {
            prep.d = br(-0.4, 2.45, 1.4);
            prep.e = br(0.2, 0.55, 1.3);
            prep.torce = -0.35;
            hit.d = br(0.05, 1.3, 0.12);
            hit.torce = 0.25;
            hit.inclina = 0.18;
            hit.avanco = 0.6;
            let solta = impacto * 0.45;
            if t < solta * 0.6 {
                return mistura(&est.guarda, &prep, suave(t / (solta * 0.6)));
            }
            if t < solta {
                return mistura(&prep, &hit, suave((t / solta - 0.6) / 0.4));
            }
            return mistura(
                &hit,
                &est.guarda,
                suave(
                    ((t - solta) / (impacto + shared::skills::RECUPERACAO_S - solta))
                        .clamp(0.0, 1.0),
                ),
            );
        }
        10 => {
            prep.d = br(0.35, 1.15, 1.7);
            prep.e = br(-0.35, 1.15, 1.7);
            prep.agacha = -0.7;
            hit.d = br(-0.35, 1.75, 0.8);
            hit.e = br(0.35, 1.75, 0.8);
            hit.inclina = -0.08;
        }
        11 => {
            prep.d = br(0.35, 1.05, 1.8);
            prep.e = br(-0.35, 1.05, 1.8);
            prep.agacha = -1.1;
            hit.d = br(-1.0, 1.5, 0.3);
            hit.e = br(1.0, 1.5, 0.3);
            hit.agacha = -0.4;
        }
        12 => {
            prep.d = br(-0.15, 2.75, 0.4);
            prep.e = br(0.25, 1.35, 1.5);
            prep.inclina = -0.12;
            hit.d = br(0.0, 1.3, 0.12);
            hit.e = br(0.2, 1.0, 0.7);
            hit.inclina = 0.25;
            hit.agacha = -1.8;
            hit.avanco = 0.7;
        }
        _ => return est.guarda,
    }
    let prepara = (impacto - 0.14).max(0.06);
    if id == 1 && t < impacto {
        // O arco do Salto: agacha no primeiro quinto, sobe e cai no impacto,
        // que e' quando o servidor aplica o golpe da queda.
        let decola = impacto * 0.2;
        if t < decola {
            return mistura(&est.guarda, &prep, suave(t / decola));
        }
        let u = ((t - decola) / (impacto - decola)).clamp(0.0, 1.0);
        let mut c = mistura(&prep, &hit, suave(u));
        c.agacha += SALTO_PICO * (std::f32::consts::PI * u).sin();
        return c;
    }
    if t < prepara {
        mistura(&est.guarda, &prep, suave((t / prepara).clamp(0.0, 1.0)))
    } else if t < impacto {
        mistura(
            &prep,
            &hit,
            suave(((t - prepara) / (impacto - prepara)).clamp(0.0, 1.0)),
        )
    } else {
        mistura(
            &hit,
            &est.guarda,
            suave(((t - impacto) / shared::skills::RECUPERACAO_S).clamp(0.0, 1.0)),
        )
    }
}

fn arma_na_mao(p: &mut Pose, e: &Entrada) {
    let c = &e.combate;
    let Some(est) = estilo(c.conjunto) else {
        return;
    };
    p.armado = true;
    p.conjunto = c.conjunto;
    let sacada = c.sacada.clamp(0.0, 1.0);
    p.na_mao = c.skill.is_some() || c.golpe.is_some() || sacada >= 0.5;

    // A chave do quadro e o quanto ela manda sobre a locomocao.
    let (chave, peso_bracos, peso_tronco) = if let Some((id, t, impacto)) = c.skill {
        (chave_da_skill(est, id, t, impacto), 1.0, 1.0)
    } else {
        match c.golpe {
            Some((passo, t)) => {
                let desde = match c.golpe_ant {
                    Some((pa, ta)) => chave_do_golpe(est, pa, ta, &est.guarda),
                    None => est.guarda,
                };
                (chave_do_golpe(est, passo, t, &desde), 1.0, 1.0)
            }
            None => {
                // Sacar e guardar sao o MESMO gesto de ida e volta: a mao vai a'
                // arma na metade do caminho, e e' ai' que ela troca de lugar.
                let bump = (sacada * PI).sin().max(0.0);
                let g = suave(sacada) * (1.0 - 0.3 * e.correr.clamp(0.0, 1.0));
                (
                    mistura(&guarda_viva(est, e), &est.sacando, bump),
                    (0.85 * g).max(bump),
                    0.6 * g,
                )
            }
        }
    };
    if peso_bracos <= 0.0 && !p.na_mao {
        return;
    }

    let torso = Quat::from_rotation_y(chave.torce) * Quat::from_rotation_x(chave.inclina);
    p.rot[TORSO] = p.rot[TORSO].slerp(torso, peso_tronco);
    // a cabeca desfaz parte da torcao: o olho fica no alvo
    p.rot[1] = p.rot[1].slerp(Quat::from_rotation_y(-chave.torce * 0.6), peso_tronco);
    p.rot[BRACO_D] = p.rot[BRACO_D].slerp(ombro(&chave.d), peso_bracos);
    p.rot[ANTEBRACO_D] = p.rot[ANTEBRACO_D].slerp(frente(chave.d.cotovelo), peso_bracos);
    p.rot[BRACO_E] = p.rot[BRACO_E].slerp(ombro(&chave.e), peso_bracos);
    p.rot[ANTEBRACO_E] = p.rot[ANTEBRACO_E].slerp(frente(chave.e.cotovelo), peso_bracos);

    // o peso: desce no golpe, sobe um pouco na antecipacao
    agacha(p, chave.agacha * peso_tronco);
    // o passo a' frente: perna direita adiante, a esquerda firma atras
    if chave.avanco > 0.0 {
        let a = chave.avanco;
        p.rot[COXA_D] = p.rot[COXA_D] * frente(0.5 * a);
        p.rot[CANELA_D] = p.rot[CANELA_D] * dobra_pra_tras(0.35 * a);
        p.rot[COXA_E] = p.rot[COXA_E] * frente(-0.35 * a);
        p.rot[CANELA_E] = p.rot[CANELA_E] * dobra_pra_tras(0.3 * a);
    }

    // a arma na mao aponta pra onde a chave manda, qualquer que seja o braco
    let cadeia_d = p.rot[BRACO_D] * p.rot[ANTEBRACO_D];
    let cadeia_e = p.rot[BRACO_E] * p.rot[ANTEBRACO_E];
    let esquerda = if c.conjunto == ESPADA_ESCUDO {
        orienta_escudo(chave.escudo)
    } else {
        orienta_lamina(&chave.arma_e)
    };
    p.punho = [
        cadeia_d.inverse() * orienta_lamina(&chave.lamina),
        cadeia_e.inverse() * esquerda,
    ];
}

/// O achatamento elastico de quem apanha: amassa na hora do golpe e volta
/// balancando. Fracao da altura (positivo = mais baixo e mais largo).
pub fn esmagamento(t: f32) -> f32 {
    if t > 0.6 {
        return 0.0;
    }
    0.13 * (-t / 0.11).exp() * (t * std::f32::consts::TAU * 6.0).cos()
}

// ═══════════════════════════════════════════════════════════════════════
//  MOLAS — o que tira o duro
// ═══════════════════════════════════════════════════════════════════════
//
// A pose do quadro vira ALVO, e cada peca chega nele com a propria inercia.
// Quanto mais longe do tronco, mais mole: o antebraco chega depois do braco e
// a lamina depois do antebraco — a quebra de juntas em sucessao da animacao
// classica. Amortecimento abaixo de 1 deixa PASSAR um pouco e voltar: e' o
// que faz a lamina chicotear no fim do corte e o chapeu balancar no tranco.

const NM: usize = N + 2;
/// As pernas nao ganham mola: o pe' plantado e o joelho do degrau tem que
/// cair exatamente onde a conta manda — mola ali e' pe' patinando.
const COM_MOLA: [bool; NM] = [
    true, true, true, true, true, true, true, false, false, false, false, true, true,
];
/// Rigidez (1/s^2) por junta: torso, cabeca, chapeu, braco d, antebraco d,
/// braco e, antebraco e, as quatro da perna, punho d (lamina), punho e.
const RIGIDEZ: [f32; NM] = [
    650.0, 260.0, 160.0, 520.0, 360.0, 520.0, 360.0, 0.0, 0.0, 0.0, 0.0, 240.0, 380.0,
];
/// Amortecimento relativo (1 = chega sem passar).
const AMORTECE: [f32; NM] = [
    0.72, 0.55, 0.42, 0.6, 0.52, 0.6, 0.52, 1.0, 1.0, 1.0, 1.0, 0.45, 0.6,
];

#[derive(Clone, Copy, Debug)]
pub struct Molas {
    rot: [Quat; NM],
    vel: [Vec3; NM],
    viva: bool,
}

impl Default for Molas {
    fn default() -> Self {
        Molas {
            rot: [Quat::IDENTITY; NM],
            vel: [Vec3::ZERO; NM],
            viva: false,
        }
    }
}

impl Molas {
    /// Leva a pose do quadro pela inercia de cada junta. Na primeira vez (ou
    /// depois de um soluco longo) so' copia: mola acordando longe do alvo
    /// daria um chicote do nada.
    pub fn segue(&mut self, p: &mut Pose, dt: f32) {
        let alvo = |p: &Pose, i: usize| if i < N { p.rot[i] } else { p.punho[i - N] };
        if !self.viva || dt > 0.25 {
            for i in 0..NM {
                self.rot[i] = alvo(p, i);
                self.vel[i] = Vec3::ZERO;
            }
            self.viva = true;
            return;
        }
        let passos = ((dt * 240.0).ceil() as usize).clamp(1, 16);
        let h = dt / passos as f32;
        for i in 0..NM {
            if !COM_MOLA[i] {
                continue;
            }
            let a = alvo(p, i);
            let k = RIGIDEZ[i];
            let c = 2.0 * AMORTECE[i] * k.sqrt();
            for _ in 0..passos {
                let mut erro = a * self.rot[i].conjugate();
                if erro.w < 0.0 {
                    erro = -erro;
                }
                let e = erro.to_scaled_axis();
                self.vel[i] += (e * k - self.vel[i] * c) * h;
                self.rot[i] = (Quat::from_scaled_axis(self.vel[i] * h) * self.rot[i]).normalize();
            }
            if i < N {
                p.rot[i] = self.rot[i];
            } else {
                p.punho[i - N] = self.rot[i];
            }
        }
    }
}

/// As pecas do conjunto, com a matriz de mundo de cada uma: a arma na mao
/// ou guardada, e o que se veste junto (bainha, coldres). Vazio pra quem
/// nao tem arma pra desenhar (o anel, que e' efeito).
pub fn armas(p: &Pose, m: &[Mat4; N], voxel: f32) -> Vec<(&'static str, Mat4)> {
    if !p.armado {
        return Vec::new();
    }
    let encaixe = |pt: [f32; 3], pai: usize| {
        Mat4::from_translation(mapa(pt, voxel) - mapa(PECAS[pai].2, voxel))
    };
    let mao_d = m[ANTEBRACO_D] * encaixe(MAO_D, ANTEBRACO_D) * Mat4::from_quat(p.punho[0]);
    let mao_e = m[ANTEBRACO_E] * encaixe(MAO_E, ANTEBRACO_E) * Mat4::from_quat(p.punho[1]);
    // Coletando: so' a ferramenta, nas DUAS maos. A arma some (guardada).
    // Presa como a katana: a pega sai da mao direita ja' resolvida pela IK, a
    // direcao sai da pose, e a malha anda `MAOS_NA_FERRAMENTA` voxels pra
    // tras — o marcador do modelo fica na mao esquerda e a direita fica 6
    // voxels acima dele, perto da cabeca.
    if let Some(f) = p.ferramenta {
        let pega = (m[ANTEBRACO_D] * encaixe(MAO_D, ANTEBRACO_D)).transform_point3(Vec3::ZERO);
        let orientacao = p.rot[BRACO_D] * p.rot[ANTEBRACO_D] * p.punho[0];
        let local = m[TORSO].inverse().transform_point3(pega);
        let presa = m[TORSO]
            * Mat4::from_translation(local)
            * Mat4::from_quat(orientacao)
            * Mat4::from_translation(vec3(0.0, 0.0, -MAOS_NA_FERRAMENTA * voxel));
        let _ = mao_d;
        return vec![(f, presa)];
    }
    // O ARCO vai na mao ESQUERDA, e antes do `match`: ele nao e' um conjunto
    // de jogador, e' a arma de um mob (`aplica_arqueiro`).
    if p.arco {
        return vec![("arco", mao_e)];
    }
    let no_torso = |pt: [f32; 3], q: Quat| m[TORSO] * encaixe(pt, TORSO) * Mat4::from_quat(q);
    // puxa a peca `v` voxels pra FORA pelo proprio eixo: e' assim que o cabo
    // da katana fica pra fora da bainha e a pega da pistola pra fora do coldre
    let pra_fora = |mat: Mat4, v: f32| mat * Mat4::from_translation(vec3(0.0, 0.0, -v * voxel));
    match p.conjunto {
        ESPADA_ESCUDO => {
            if p.na_mao {
                vec![("espada", mao_d), ("escudo", mao_e)]
            } else {
                vec![
                    (
                        "espada",
                        no_torso(QUADRIL_E, orienta_lamina(&ESTILO_ESPADA.sacando.lamina)),
                    ),
                    ("escudo", no_torso(COSTAS, orienta_escudo(PI))),
                ]
            }
        }
        KATANA => {
            let bainha = no_torso(BAINHA, orienta_lamina(&ESTILO_KATANA.sacando.lamina));
            let katana = if p.na_mao {
                let pega =
                    (m[ANTEBRACO_D] * encaixe(MAO_D, ANTEBRACO_D)).transform_point3(Vec3::ZERO);
                let orientacao = p.rot[BRACO_D] * p.rot[ANTEBRACO_D] * p.punho[0];
                let local = m[TORSO].inverse().transform_point3(pega);
                // O marcador do modelo fica no voxel 3 do cabo; a direita
                // segura no 6, a esquerda no 1 (cinco voxels entre as maos).
                pra_fora(
                    m[TORSO] * Mat4::from_translation(local) * Mat4::from_quat(orientacao),
                    3.0,
                )
            } else {
                pra_fora(bainha, 5.0)
            };
            vec![("bainha", bainha), ("katana", katana)]
        }
        PISTOLAS => {
            let q = orienta_lamina(&la(0.0, 0.0, 0.0));
            let (cd, ce) = (no_torso(COLDRE_D, q), no_torso(COLDRE_E, q));
            let (pd, pe) = if p.na_mao {
                (mao_d, mao_e)
            } else {
                (pra_fora(cd, 3.0), pra_fora(ce, 3.0))
            };
            vec![
                ("coldre", cd),
                ("coldre", ce),
                ("pistola", pd),
                ("pistola", pe),
            ]
        }
        _ => Vec::new(),
    }
}

/// Os dois pulsos (direito, esquerdo), no espaco de mundo: e' neles que o
/// anel projeta o circulo. O eixo -Y de cada um aponta pra mao.
pub fn pulsos(m: &[Mat4; N], voxel: f32) -> [Mat4; 2] {
    let em = |pt: [f32; 3], pai: usize| {
        m[pai] * Mat4::from_translation(mapa(pt, voxel) - mapa(PECAS[pai].2, voxel))
    };
    [em(PULSO_D, ANTEBRACO_D), em(PULSO_E, ANTEBRACO_E)]
}

/// Arco na esquerda, CORDA FIXA. A direita recua um pouco no disparo.
///
/// Era um gesto de puxar a corda ate' o rosto: o antebraco direito girava
/// 1,65 rad (94 graus) ao longo do golpe. Dois problemas, e o segundo
/// explicava o primeiro:
///
/// 1. **nao havia arco.** O rig do humanoide sai "sem a arma" (as dez pecas
///    nomeadas de `humanoides.py`) — so' o modelo PLANO tinha o arco. Entao o
///    braco fazia o movimento de puxar no vazio;
/// 2. **a corda nao pode ser puxada em voxel.** Ela e' uma linha de blocos de
///    meio bloco: curvada vira ladder, e curvada ANIMADA vira ladder que se
///    mexe. O dono viu e disse o certo — "nem precisa ser [dinamica] mais,
///    pode ser fixo".
///
/// Entao: arco de verdade na mao esquerda (`arco.vox`, corda reta), e o gesto
/// vira um RECUO curto do cotovelo direito. O disparo se le' pela flecha que
/// sai, que e' o que o jogador de fato olha.
pub fn aplica_arqueiro(p: &mut Pose, golpe: Option<f32>) {
    p.armado = false;
    p.na_mao = false;
    p.arco = true;
    // Recuo CURTO (0,35 rad, 20 graus) e so' no golpe: e' pontuacao do tiro,
    // e nao a animacao inteira de armar o arco.
    let recuo = golpe.map_or(0.0, |t| {
        if t < IMPACTO {
            suave((t / IMPACTO).clamp(0.0, 1.0))
        } else {
            1.0 - suave(((t - IMPACTO) / 0.16).clamp(0.0, 1.0))
        }
    });
    p.rot[TORSO] = Quat::from_rotation_y(-0.3);
    p.rot[BRACO_E] = ombro(&br(0.05, 1.45, 0.15));
    p.rot[ANTEBRACO_E] = frente(0.15);
    p.rot[BRACO_D] = ombro(&br(-0.5 - 0.1 * recuo, 1.45, 0.0));
    p.rot[ANTEBRACO_D] = frente(1.15 + 0.35 * recuo);
}

/// Onde fica a palma de cada mao (direita, esquerda), no mundo: e' ali que o
/// anel brilha.
pub fn palmas(m: &[Mat4; N], voxel: f32) -> [Vec3; 2] {
    let em = |pt: [f32; 3], pai: usize| {
        m[pai].transform_point3(mapa(pt, voxel) - mapa(PECAS[pai].2, voxel))
    };
    [em(MAO_D, ANTEBRACO_D), em(MAO_E, ANTEBRACO_E)]
}

/// O conjunto e' o anel? (o render desenha o circulo no pulso)
pub fn e_anel(p: &Pose) -> bool {
    p.armado && p.conjunto == ANEL
}

/// O conjunto e' de pistolas? (o render desenha o clarao do cano)
pub fn e_pistolas(p: &Pose) -> bool {
    p.armado && p.conjunto == PISTOLAS
}

/// O instante do tiro/impacto dentro do golpe, pro efeito casar com o gesto.
pub const IMPACTO: f32 = PREPARA + CORTA * 0.35;

#[cfg(test)]
mod testes {
    use super::*;

    fn parado() -> Entrada {
        Entrada {
            fase: 0.0,
            andar: 0.0,
            correr: 0.0,
            tempo: 0.0,
            ar: 0.0,
            degrau: [0.0, 0.0],
            combate: Combate::default(),
        }
    }

    /// Onde a sola de um pé fica, em voxels, com a raiz no chão.
    fn sola(p: &Pose, coxa: usize, canela: usize) -> Vec3 {
        let m = matrizes(p, Mat4::IDENTITY, 1.0);
        let local = mapa([PECAS[coxa].2[0], 12.0, 0.0], 1.0) - mapa(PECAS[canela].2, 1.0);
        m[canela].transform_point3(local)
    }

    /// Parado, as pernas ficam retas e a sola no chão — senão o boneco parado
    /// já nasce agachado ou flutuando.
    #[test]
    fn parado_as_solas_ficam_no_chao() {
        let p = pose(&parado());
        for (c, k) in [(COXA_D, CANELA_D), (COXA_E, CANELA_E)] {
            let s = sola(&p, c, k);
            assert!(s.y.abs() < 0.01, "sola em {s:?}");
        }
    }

    /// O joelho do degrau leva a sola EXATAMENTE pro bloco de cima, embaixo
    /// do quadril. Um bloco do terreno são 12,5 voxels do personagem.
    #[test]
    fn o_joelho_planta_o_pe_no_degrau() {
        for degrau in [2.0f32, 6.0, 12.5] {
            let mut e = parado();
            e.degrau = [degrau, 0.0];
            let p = pose(&e);
            let s = sola(&p, COXA_D, CANELA_D);
            assert!(
                (s.y - degrau).abs() < 0.3,
                "degrau {degrau}: sola em y {:.2}",
                s.y
            );
            assert!(
                s.z.abs() < 0.5,
                "degrau {degrau}: sola saiu {:.2} pra frente/trás",
                s.z
            );
            // O outro pé continua no chão.
            assert!(sola(&p, COXA_E, CANELA_E).y.abs() < 0.01);
        }
    }

    fn mao(p: &Pose, antebraco: usize) -> Vec3 {
        let x = PECAS[antebraco].2[0];
        matrizes(p, Mat4::IDENTITY, 1.0)[antebraco]
            .transform_point3(mapa([x, 12.0, 16.0], 1.0) - mapa(PECAS[antebraco].2, 1.0))
    }

    /// No ar o corpo fica SOLTO, não encolhido como num pulo: em nenhum ponto
    /// do balanço a sola passa de poucos voxels acima da altura de parado. A
    /// pose de pulo antiga levantava a sola uns 8.
    #[test]
    fn no_ar_o_corpo_fica_solto_e_nao_pula() {
        let mut e = parado();
        e.ar = 1.0;
        for k in 0..32 {
            e.tempo = k as f32 * 0.02;
            let p = pose(&e);
            for (c, kk) in [(COXA_D, CANELA_D), (COXA_E, CANELA_E)] {
                let s = sola(&p, c, kk);
                assert!(
                    s.y < 4.5,
                    "tempo {:.2}: perna encolhida, sola em y {:.2}",
                    e.tempo,
                    s.y
                );
            }
        }
    }

    /// No ar é o instante de uma corrida CONGELADO: a perna direita fica na
    /// frente e a esquerda atrás o tempo todo. Perna que TROCA de lado lê
    /// como andar no ar — foi o que a terceira tentativa fez. Braço aberto, e
    /// tudo balançando um pouco, não parado.
    #[test]
    fn no_ar_um_pe_na_frente_outro_atras_sem_trocar() {
        let mut e = parado();
        e.ar = 1.0;
        let (mut lo, mut hi) = (f32::MAX, f32::MIN);
        for k in 0..32 {
            e.tempo = k as f32 * 0.02;
            let p = pose(&e);
            let (d, es) = (sola(&p, COXA_D, CANELA_D), sola(&p, COXA_E, CANELA_E));
            // O que importa é NUNCA trocar de lado: a da frente sempre à
            // frente do corpo, a de trás sempre atrás, bem separadas. Quando
            // a abertura fecha, a da frente fica ~3 voxels à frente — ainda
            // na frente.
            assert!(
                d.z > 1.5,
                "tempo {:.2}: a perna da frente saiu da frente ({:.2})",
                e.tempo,
                d.z
            );
            assert!(
                es.z < -1.5,
                "tempo {:.2}: a perna de trás saiu de trás ({:.2})",
                e.tempo,
                es.z
            );
            assert!(
                d.z - es.z > 4.0,
                "tempo {:.2}: as pernas se juntaram ({:.2} x {:.2})",
                e.tempo,
                d.z,
                es.z
            );
            let ombro = mapa(PECAS[BRACO_D].2, 1.0);
            let (md, me) = (mao(&p, ANTEBRACO_D), mao(&p, ANTEBRACO_E));
            assert!(
                md.x.abs() > ombro.x.abs() + 3.0 && me.x.abs() > ombro.x.abs() + 3.0,
                "tempo {:.2}: braço não está aberto",
                e.tempo
            );
            lo = lo.min(d.z);
            hi = hi.max(d.z);
        }
        assert!(
            hi - lo > 0.8,
            "a perna não balança no ar ({lo:.2}..{hi:.2})"
        );
    }

    /// No meio do passo a perna direita está NA FRENTE (o +Z do rig) e a
    /// esquerda atrás — e o braço direito vai pra trás, oposto.
    #[test]
    fn andando_perna_e_braco_se_opoem() {
        let mut e = parado();
        e.andar = 1.0;
        e.fase = std::f32::consts::FRAC_PI_2;
        let p = pose(&e);
        assert!(
            sola(&p, COXA_D, CANELA_D).z > 2.0,
            "perna direita não foi pra frente"
        );
        assert!(
            sola(&p, COXA_E, CANELA_E).z < -2.0,
            "perna esquerda não foi pra trás"
        );
        let m = matrizes(&p, Mat4::IDENTITY, 1.0);
        let mao = m[ANTEBRACO_D]
            .transform_point3(mapa([23.0, 12.0, 16.0], 1.0) - mapa(PECAS[ANTEBRACO_D].2, 1.0));
        assert!(
            mao.z < 0.0,
            "braço direito deveria ir pra trás, mão em {mao:?}"
        );
    }
    // ── combate ──

    fn em_combate(golpe: Option<(u8, f32)>, golpe_ant: Option<(u8, f32)>, sacada: f32) -> Entrada {
        let mut e = parado();
        e.combate = Combate {
            conjunto: 0,
            sacada,
            golpe,
            golpe_ant,
            ..Default::default()
        };
        e
    }

    /// Ponta da lamina e (posicao, normal da face) do escudo, em voxels.
    fn armas_em(e: &Entrada) -> (Vec3, Vec3, Vec3) {
        let p = pose(e);
        let m = matrizes(&p, Mat4::IDENTITY, 1.0);
        let a = armas(&p, &m, 1.0);
        let pega = |n: &str| a.iter().find(|(x, _)| *x == n).map(|(_, m)| *m).expect(n);
        let (espada, escudo) = (pega("espada"), pega("escudo"));
        (
            espada.transform_point3(vec3(0.0, 0.0, 20.0)),
            escudo.transform_point3(Vec3::ZERO),
            escudo.transform_vector3(Vec3::X).normalize(),
        )
    }

    #[test]
    fn em_guarda_o_escudo_cobre_a_frente_e_a_espada_fica_erguida() {
        let (ponta, escudo, face) = armas_em(&em_combate(None, None, 1.0));
        assert!(face.z > 0.7, "o escudo olha pra {face:?}");
        assert!(escudo.z > 2.0, "escudo em {escudo:?}");
        assert!(ponta.y > 30.0 && ponta.z > 5.0, "ponta em {ponta:?}");
    }

    #[test]
    fn guardada_a_espada_fica_no_quadril_esquerdo_e_o_escudo_nas_costas() {
        let p = pose(&em_combate(None, None, 0.0));
        let m = matrizes(&p, Mat4::IDENTITY, 1.0);
        let a = armas(&p, &m, 1.0);
        let pega = |n: &str| a.iter().find(|(x, _)| *x == n).map(|(_, m)| *m).expect(n);
        let (espada, escudo) = (pega("espada"), pega("escudo"));
        let (e, s) = (
            espada.transform_point3(Vec3::ZERO),
            escudo.transform_point3(Vec3::ZERO),
        );
        // a esquerda do boneco e' o +X
        assert!(e.x > 4.0 && (15.0..26.0).contains(&e.y), "espada em {e:?}");
        assert!(s.z < -2.0, "escudo em {s:?}");
    }

    #[test]
    fn o_primeiro_golpe_corta_da_direita_pra_esquerda_na_altura_do_peito() {
        let (prep, _, _) = armas_em(&em_combate(Some((0, PREPARA)), None, 1.0));
        let (imp, _, _) = armas_em(&em_combate(Some((0, PREPARA + CORTA)), None, 1.0));
        assert!(prep.x < -8.0, "preparo a' direita: {prep:?}");
        assert!(imp.x > 8.0, "impacto a' esquerda: {imp:?}");
        for p in [prep, imp] {
            assert!(
                (18.0..40.0).contains(&p.y),
                "fora da altura do peito: {p:?}"
            );
        }
        assert!((prep.y - imp.y).abs() < 10.0, "corte nao ficou horizontal");
    }

    #[test]
    fn o_terceiro_golpe_desce_de_cima() {
        let (prep, _, _) = armas_em(&em_combate(Some((2, PREPARA)), None, 1.0));
        let (imp, _, _) = armas_em(&em_combate(Some((2, PREPARA + CORTA)), None, 1.0));
        assert!(prep.y > 45.0, "preparo tem que ir la' em cima: {prep:?}");
        assert!(
            imp.y < 30.0 && imp.z > 10.0,
            "impacto a' frente e embaixo: {imp:?}"
        );
    }

    #[test]
    fn o_golpe_nao_estala_nem_na_troca() {
        let (a, _, _) = armas_em(&em_combate(Some((0, 0.25)), None, 1.0));
        let (b, _, _) = armas_em(&em_combate(Some((1, 0.0)), Some((0, 0.25)), 1.0));
        assert!(
            a.distance(b) < 0.5,
            "troca de golpe saltou {:.1} voxels",
            a.distance(b)
        );
        for passo in 0..3u8 {
            let mut antes = armas_em(&em_combate(Some((passo, 0.0)), None, 1.0));
            let mut t = 0.0;
            while t < DURACAO_DO_GOLPE {
                t += 0.001;
                let agora = armas_em(&em_combate(Some((passo, t)), None, 1.0));
                // o corte varre ~190 graus somando tronco e braco: rapido de verdade, mas
                // continuo — estalo seria dezenas de voxels
                assert!(
                    agora.0.distance(antes.0) < 6.0,
                    "golpe {passo}: ponta saltou em t={t:.3}"
                );
                assert!(
                    agora.1.distance(antes.1) < 1.0,
                    "golpe {passo}: escudo saltou em t={t:.3}"
                );
                antes = agora;
            }
        }
    }

    #[test]
    fn sacar_nao_estala() {
        let mut antes = armas_em(&em_combate(None, None, 0.0));
        for k in 1..=400 {
            let s = k as f32 / 400.0;
            let agora = armas_em(&em_combate(None, None, s));
            // na metade a arma troca de lugar (quadril -> mao): so' la' pode andar mais
            let limite = if (s - 0.5).abs() < 0.003 { 14.0 } else { 1.5 };
            assert!(
                agora.0.distance(antes.0) < limite,
                "sacando em {s:.3}: {:.1}",
                agora.0.distance(antes.0)
            );
            antes = agora;
        }
    }

    #[test]
    fn apanhar_joga_a_cabeca_pra_tras() {
        let cabeca = |e: &Entrada| {
            matrizes(&pose(e), Mat4::IDENTITY, 1.0)[1].transform_point3(vec3(0.0, 6.0, 0.0))
        };
        let mut e = parado();
        let parada = cabeca(&e);
        e.combate.ferido = Some(0.05);
        assert!(cabeca(&e).z < parada.z - 1.0);
    }

    #[test]
    fn a_mola_chega_no_alvo_e_a_lamina_passa_um_pouco() {
        let mut m = Molas::default();
        let mut p = pose(&parado());
        m.segue(&mut p, 1.0 / 60.0); // acorda copiando
        let alvo = Quat::from_rotation_x(-1.0);
        let mut max_lamina: f32 = 0.0;
        for _ in 0..60 {
            let mut q = pose(&parado());
            q.rot[BRACO_D] = alvo;
            q.punho[0] = alvo;
            m.segue(&mut q, 1.0 / 60.0);
            max_lamina = max_lamina.max(q.punho[0].angle_between(Quat::IDENTITY));
            p = q;
        }
        assert!(
            p.rot[BRACO_D].angle_between(alvo) < 0.02,
            "braco nao chegou"
        );
        assert!(
            max_lamina > 1.02,
            "a lamina tinha que passar do alvo: {max_lamina:.3}"
        );
        // perna nao tem mola
        assert_eq!(p.rot[COXA_D], pose(&parado()).rot[COXA_D]);
    }

    #[test]
    fn no_golpe_o_peso_desce_sem_afundar_o_pe() {
        let e = em_combate(Some((0, PREPARA + CORTA + 0.01)), None, 1.0);
        let p = pose(&e);
        assert!(p.subida < -0.8, "o corpo tinha que descer: {}", p.subida);
        for (c, k) in [(COXA_D, CANELA_D), (COXA_E, CANELA_E)] {
            let s = sola(&p, c, k);
            assert!(s.y.abs() < 0.6, "pe' afundou ou saiu do chao: {s:?}");
        }
    }

    #[test]
    fn o_tranco_vai_pra_longe_do_atacante() {
        let cabeca = |e: &Entrada| {
            matrizes(&pose(e), Mat4::IDENTITY, 1.0)[1].transform_point3(vec3(0.0, 6.0, 0.0))
        };
        let mut e = parado();
        let parada = cabeca(&e);
        e.combate.ferido = Some(0.05);
        e.combate.recuo = vec3(1.0, 0.0, 0.0); // golpe vindo da direita empurra pra esquerda (+X)
        assert!(cabeca(&e).x > parada.x + 1.0);
    }

    fn com_conjunto(conj: u8, golpe: Option<(u8, f32)>, sacada: f32) -> Entrada {
        let mut e = em_combate(golpe, None, sacada);
        e.combate.conjunto = conj;
        e
    }

    fn arma_em(e: &Entrada, nome: &str, i: usize) -> Mat4 {
        let p = pose(e);
        let m = matrizes(&p, Mat4::IDENTITY, 1.0);
        armas(&p, &m, 1.0)
            .into_iter()
            .filter(|(n, _)| *n == nome)
            .nth(i)
            .expect(nome)
            .1
    }

    #[test]
    fn katana_fica_nas_duas_maos_durante_guarda_e_combo() {
        for voxel in [1.0, 0.04] {
            let mut molas = Molas::default();
            for quadro in 0..240 {
                let mut e = com_conjunto(
                    KATANA,
                    if quadro < 60 {
                        None
                    } else {
                        Some((
                            ((quadro - 60) / 60) as u8,
                            ((quadro - 60) % 60) as f32 / 60.0,
                        ))
                    },
                    1.0,
                );
                e.tempo = quadro as f32 / 60.0;
                e.fase = e.tempo * 8.0;
                e.andar = 1.0;
                e.correr = 1.0;
                let mut p = pose(&e);
                molas.segue(&mut p, 1.0 / 60.0);
                let base =
                    Mat4::from_translation(vec3(3.0, 2.0, -4.0)) * Mat4::from_rotation_y(0.8);
                let m = matrizes(&p, base, voxel);
                let katana = armas(&p, &m, voxel)
                    .into_iter()
                    .find(|(n, _)| *n == "katana")
                    .unwrap()
                    .1;
                for (braco, antebraco, mao, pega) in [
                    (BRACO_D, ANTEBRACO_D, MAO_D, 3.0),
                    (BRACO_E, ANTEBRACO_E, MAO_E, -2.0),
                ] {
                    let pos = m[antebraco]
                        .transform_point3(mapa(mao, voxel) - mapa(PECAS[antebraco].2, voxel));
                    let cabo = katana.transform_point3(Vec3::Z * pega * voxel);
                    assert!(
                        pos.distance(cabo) < 0.01 * voxel,
                        "mao solta no quadro {quadro}: {pos:?} / {cabo:?}"
                    );
                    let ombro = m[braco].transform_point3(Vec3::ZERO);
                    let cotovelo = m[antebraco].transform_point3(Vec3::ZERO);
                    assert!((ombro.distance(cotovelo) / voxel - 6.0).abs() < 0.01);
                    assert!((cotovelo.distance(pos) / voxel - 7.5).abs() < 0.01);
                }
            }
        }
    }

    #[test]
    fn skills_tem_pose_finita_e_katana_mantem_as_duas_maos_no_cabo() {
        for skill in shared::skills::playtest() {
            let mut molas = Molas::default();
            for quadro in 0..120 {
                let t =
                    quadro as f32 / 120.0 * (skill.impacto_em() + shared::skills::RECUPERACAO_S);
                let mut e = com_conjunto(skill.conjunto as u8, None, 1.0);
                e.combate.skill = Some((skill.id, t, skill.impacto_em()));
                let mut p = pose(&e);
                molas.segue(&mut p, 1.0 / 120.0);
                let m = matrizes(&p, Mat4::IDENTITY, 1.0);
                assert!(
                    m.iter().all(|mat| mat.is_finite()),
                    "{} quadro {quadro}",
                    skill.nome
                );
                if skill.conjunto == shared::skills::Conjunto::Katana {
                    let k = armas(&p, &m, 1.0)
                        .into_iter()
                        .find(|(n, _)| *n == "katana")
                        .unwrap()
                        .1;
                    for (i, mao, z) in [(ANTEBRACO_D, MAO_D, 3.0), (ANTEBRACO_E, MAO_E, -2.0)] {
                        let pos = m[i].transform_point3(mapa(mao, 1.0) - mapa(PECAS[i].2, 1.0));
                        assert!(
                            pos.distance(k.transform_point3(Vec3::Z * z)) < 0.01,
                            "{}: mao solta",
                            skill.nome
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn skills_voltam_a_guarda_sem_torcer_o_tronco_uma_volta_inteira() {
        for skill in shared::skills::playtest() {
            let est = estilo(skill.conjunto as u8).unwrap();
            for i in 0..=120 {
                let t = i as f32 / 120.0 * (skill.impacto_em() + shared::skills::RECUPERACAO_S);
                let c = chave_da_skill(est, skill.id, t, skill.impacto_em());
                assert!(c.torce.abs() < 1.2, "{} torce demais", skill.nome);
            }
            let c = chave_da_skill(
                est,
                skill.id,
                skill.impacto_em() + shared::skills::RECUPERACAO_S + 0.001,
                skill.impacto_em(),
            );
            assert_eq!(c, est.guarda, "{} nao voltou a guarda", skill.nome);
        }
    }

    #[test]
    fn a_katana_estoca_pra_frente_no_terceiro() {
        let e = com_conjunto(KATANA, Some((2, PREPARA + CORTA)), 1.0);
        let ponta = arma_em(&e, "katana", 0).transform_point3(vec3(0.0, 0.0, 22.0));
        assert!(
            ponta.z > 22.0 && (15.0..38.0).contains(&ponta.y),
            "{ponta:?}"
        );
    }

    #[test]
    fn guardada_a_katana_fica_dentro_da_bainha() {
        let e = com_conjunto(KATANA, None, 0.0);
        let (k, b) = (arma_em(&e, "katana", 0), arma_em(&e, "bainha", 0));
        let ponta = k.transform_point3(vec3(0.0, 0.0, 20.0));
        let meio = b.transform_point3(vec3(0.0, 0.0, 12.0));
        assert!(
            ponta.distance(meio) < 6.0,
            "a lamina tem que estar na bainha: {ponta:?} x {meio:?}"
        );
        assert!(
            b.transform_point3(Vec3::ZERO).x > 4.0,
            "bainha no quadril esquerdo"
        );
    }

    #[test]
    fn a_pistola_aponta_pra_frente_e_da_coice() {
        let cano = |t: f32| {
            arma_em(&com_conjunto(PISTOLAS, Some((0, t)), 1.0), "pistola", 0)
                .transform_vector3(Vec3::Z)
        };
        let (prep, imp) = (cano(PREPARA), cano(PREPARA + CORTA));
        assert!(prep.z > 0.8, "{prep:?}");
        assert!(
            imp.y > prep.y + 0.2,
            "o coice levanta o cano: {prep:?} -> {imp:?}"
        );
    }

    #[test]
    fn guardadas_as_pistolas_ficam_nos_coldres() {
        let e = com_conjunto(PISTOLAS, None, 0.0);
        for i in 0..2 {
            let (pi, co) = (arma_em(&e, "pistola", i), arma_em(&e, "coldre", i));
            let d = pi
                .transform_point3(vec3(0.0, 0.0, 6.0))
                .distance(co.transform_point3(vec3(0.0, 0.0, 4.0)));
            assert!(d < 4.0, "pistola {i} fora do coldre: {d:.1}");
        }
    }

    #[test]
    fn todo_conjunto_golpeia_sem_estalo() {
        for conj in [KATANA, PISTOLAS, ANEL] {
            for passo in 0..3u8 {
                let mao = |t: f32| {
                    let p = pose(&com_conjunto(conj, Some((passo, t)), 1.0));
                    matrizes(&p, Mat4::IDENTITY, 1.0)[ANTEBRACO_D]
                        .transform_point3(vec3(0.0, -8.0, 0.0))
                };
                let mut antes = mao(0.0);
                let mut t = 0.0;
                while t < DURACAO_DO_GOLPE {
                    t += 0.001;
                    let agora = mao(t);
                    assert!(
                        agora.distance(antes) < 3.0,
                        "conjunto {conj} golpe {passo} saltou em {t:.3}"
                    );
                    antes = agora;
                }
            }
        }
    }

    #[test]
    fn o_anel_nao_tem_arma_pra_desenhar() {
        let p = pose(&com_conjunto(ANEL, Some((0, 0.1)), 1.0));
        let m = matrizes(&p, Mat4::IDENTITY, 1.0);
        assert!(armas(&p, &m, 1.0).is_empty());
        assert!(e_anel(&p));
    }
}

#[cfg(test)]
mod testes_do_contrato {
    use super::*;

    /// TODA peça de TODO `.vox` de personagem tem pivô.
    ///
    /// `pivo` devolvia a RAIZ pra nome desconhecido, e `desenha_rig` percorre
    /// `PECAS` e não o arquivo: uma peça mal-nomeada virava malha em volta do
    /// chão que ninguém desenhava. Duas falhas caladas, uma escondendo a
    /// outra. Este teste abre os arquivos de verdade — é a conferência que
    /// hoje acontece no boot e que a carga preguiçosa vai tirar de lá.
    #[test]
    fn toda_peca_de_personagem_tem_pivo() {
        let mut vistos = 0;
        for dir in ["personagem", "humanoides", "npcs"] {
            let caminho = format!("../../assets/vox/{dir}");
            let Ok(entradas) = std::fs::read_dir(&caminho) else {
                continue;
            };
            for e in entradas.flatten() {
                let p = e.path();
                if p.extension().is_none_or(|x| x != "vox") {
                    continue;
                }
                let bytes = std::fs::read(&p).expect("le o vox");
                let Ok(pecas) = crate::vox::parse_nomeado(&bytes) else {
                    continue;
                };
                // Arquivo de UMA peça sem nome é modelo inteiro (arma, saque):
                // não é rig e não passa por `pivo`.
                if pecas.len() == 1 && pecas[0].0.is_empty() {
                    continue;
                }
                for (nome, _) in &pecas {
                    assert!(
                        !nome.is_empty(),
                        "{:?}: peça SEM NOME — ela some no carregamento",
                        p.file_name().unwrap()
                    );
                    assert!(
                        pivo(nome).is_some(),
                        "{:?}: a peça '{nome}' não está no contrato do rig \
                         ({:?}) — ela nunca seria desenhada",
                        p.file_name().unwrap(),
                        PECAS.iter().map(|x| x.0).collect::<Vec<_>>()
                    );
                    vistos += 1;
                }
            }
        }
        assert!(
            vistos > 100,
            "só {vistos} peças conferidas — achou os arquivos?"
        );
    }

    /// Os apelidos da cabeça caem no slot do cabelo, e nada mais inventa slot.
    #[test]
    fn capuz_e_elmo_ocupam_o_slot_do_cabelo() {
        for n in ["capuz", "elmo", "chapeu"] {
            assert_eq!(slot_de(n), Some("cabelo"), "{n}");
            assert_eq!(pivo(n), pivo("cabelo"), "{n} gira no pivô da cabeça");
        }
        assert_eq!(slot_de("torso"), Some("torso"));
        assert_eq!(
            slot_de("perna_d"),
            None,
            "nome que não existe não vira slot"
        );
        assert_eq!(pivo("perna_d"), None);
    }
}
