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
/// gerar a malha em volta dela. Peça fora da lista gira em volta da raiz.
pub fn pivo(nome: &str) -> [f32; 3] {
    PECAS.iter().find(|p| p.0 == nome).map(|p| p.2).unwrap_or(RAIZ)
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
}

impl Pose {
    fn de(rot: [Quat; N], subida: f32) -> Pose {
        Pose { rot, subida, punho: [Quat::IDENTITY; 2], na_mao: false, armado: false }
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
    m
}

/// A pose do quadro: a locomocao e, por cima dela, o combate.
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
    /// O golpe que este interrompeu, CONGELADO no instante da troca: o novo
    /// parte de onde o anterior estava, entao a troca nao estala.
    pub golpe_ant: Option<(u8, f32)>,
    /// Segundos desde o ultimo dano.
    pub ferido: Option<f32>,
}

/// Quanto tempo sacar (ou guardar) leva.
pub const TEMPO_DE_SACAR: f32 = 0.4;

const PREPARA: f32 = 0.08;
const CORTA: f32 = 0.09;
const SEGURA: f32 = 0.04;
const VOLTA: f32 = 0.24;
/// Um golpe inteiro. A cadencia do ataque e' 0,25 s: o seguinte chega no meio
/// da VOLTA deste e parte dali (`Combate::golpe_ant`).
pub const DURACAO_DO_GOLPE: f32 = PREPARA + CORTA + SEGURA + VOLTA;

/// So' espada e escudo tem modelo por enquanto (docs/ARTE_DO_PERSONAGEM.md,
/// entrega 3). Os outros conjuntos entram com a arte deles.
const ESPADA_ESCUDO: u8 = 0;

/// Onde as armas se prendem, em voxel da tela comum.
const MAO_D: [f32; 3] = [23.0, 12.0, 17.5];
const MAO_E: [f32; 3] = [9.0, 12.0, 17.5];
/// Guardadas: a espada pendurada no quadril ESQUERDO — a mao direita saca
/// cruzando o corpo — e o escudo nas costas.
const QUADRIL_E: [f32; 3] = [10.2, 13.5, 21.5];
const COSTAS: [f32; 3] = [16.0, 8.4, 26.0];

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
    /// lamina deitada (corte horizontal).
    giro: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct Chave {
    torce: f32,
    inclina: f32,
    d: Braco,
    e: Braco,
    lamina: Lamina,
    /// Pra onde a face do escudo olha: guinada, 0 = frente.
    escudo: f32,
    /// 0..1: o passo a' frente do golpe de cima.
    avanco: f32,
}

const fn br(guinada: f32, elevacao: f32, cotovelo: f32) -> Braco {
    Braco { guinada, elevacao, cotovelo }
}
const fn la(guinada: f32, elevacao: f32, giro: f32) -> Lamina {
    Lamina { guinada, elevacao, giro }
}
const MEIA_VOLTA: f32 = std::f32::consts::FRAC_PI_2;
const PI: f32 = std::f32::consts::PI;

/// Em guarda: a espada erguida a' frente com o fio pra baixo, o escudo
/// cobrindo o peito.
const GUARDA: Chave = Chave {
    torce: -0.15,
    inclina: 0.05,
    d: br(-0.35, 0.45, 1.25),
    e: br(-0.15, 0.55, 1.35),
    lamina: la(-0.2, 1.9, PI),
    escudo: 0.2,
    avanco: 0.0,
};

/// Sacando: a mao direita no quadril esquerdo, a esquerda por cima do ombro
/// buscando o escudo.
const SACANDO: Chave = Chave {
    torce: 0.3,
    inclina: 0.05,
    d: br(0.9, 0.35, 1.0),
    e: br(0.3, 2.6, 1.6),
    lamina: la(0.0, -0.5, 0.0),
    escudo: PI,
    avanco: 0.0,
};

/// Os tres golpes, (preparo, impacto). Cada um comeca onde o anterior
/// termina: o corte da direita pra esquerda deixa a lamina a' esquerda, e e'
/// dali que sai o de volta; o de volta termina no alto a' direita, de onde
/// sobe o de cima.
const GOLPES: [[Chave; 2]; 3] = [
    // 1 — corte horizontal, da direita pra esquerda
    [
        Chave {
            torce: -0.55,
            inclina: 0.0,
            d: br(-1.4, 1.35, 0.5),
            e: br(-0.1, 0.5, 1.4),
            lamina: la(-1.6, 1.5, MEIA_VOLTA),
            escudo: 0.3,
            avanco: 0.0,
        },
        Chave {
            torce: 0.55,
            inclina: 0.05,
            d: br(0.9, 1.45, 0.25),
            e: br(0.5, 0.4, 1.1),
            lamina: la(1.1, 1.55, MEIA_VOLTA),
            escudo: 0.7,
            avanco: 0.0,
        },
    ],
    // 2 — de volta, subindo da esquerda pra direita
    [
        Chave {
            torce: 0.6,
            inclina: 0.05,
            d: br(1.1, 1.1, 0.7),
            e: br(0.5, 0.4, 1.1),
            lamina: la(1.3, 1.2, -MEIA_VOLTA),
            escudo: 0.7,
            avanco: 0.0,
        },
        Chave {
            torce: -0.45,
            inclina: -0.05,
            d: br(-1.1, 1.8, 0.3),
            e: br(-0.1, 0.5, 1.4),
            lamina: la(-1.2, 1.9, -MEIA_VOLTA),
            escudo: 0.2,
            avanco: 0.0,
        },
    ],
    // 3 — de cima pra baixo, com o passo a' frente
    [
        Chave {
            torce: -0.1,
            inclina: -0.25,
            d: br(-0.2, 2.8, 0.6),
            e: br(-0.1, 0.6, 1.3),
            lamina: la(-0.1, 3.3, PI),
            escudo: 0.2,
            avanco: 0.3,
        },
        Chave {
            torce: 0.05,
            inclina: 0.35,
            d: br(-0.1, 1.2, 0.15),
            e: br(0.2, 0.35, 1.0),
            lamina: la(-0.05, 1.15, PI),
            escudo: 0.4,
            avanco: 1.0,
        },
    ],
];

fn mistura(a: &Chave, b: &Chave, k: f32) -> Chave {
    let l = |x: f32, y: f32| x + (y - x) * k;
    let bra = |x: &Braco, y: &Braco| br(l(x.guinada, y.guinada), l(x.elevacao, y.elevacao), l(x.cotovelo, y.cotovelo));
    Chave {
        torce: l(a.torce, b.torce),
        inclina: l(a.inclina, b.inclina),
        d: bra(&a.d, &b.d),
        e: bra(&a.e, &b.e),
        lamina: la(
            l(a.lamina.guinada, b.lamina.guinada),
            l(a.lamina.elevacao, b.lamina.elevacao),
            l(a.lamina.giro, b.lamina.giro),
        ),
        escudo: l(a.escudo, b.escudo),
        avanco: l(a.avanco, b.avanco),
    }
}

fn suave(u: f32) -> f32 {
    let u = u.clamp(0.0, 1.0);
    u * u * (3.0 - 2.0 * u)
}

/// A chave do golpe `passo` no instante `t`, saindo de `desde`.
///
/// Preparo sai rapido e chega devagar (a antecipacao); o corte acelera e
/// freia (o meio e' o tapa); segura um instante no impacto, que e' o quadro
/// que o olho guarda; e volta pra guarda.
fn chave_do_golpe(passo: u8, t: f32, desde: &Chave) -> Chave {
    let [prep, imp] = &GOLPES[passo.min(2) as usize];
    if t < PREPARA {
        let u = t / PREPARA;
        return mistura(desde, prep, 1.0 - (1.0 - u) * (1.0 - u));
    }
    if t < PREPARA + CORTA {
        return mistura(prep, imp, suave((t - PREPARA) / CORTA));
    }
    if t < PREPARA + CORTA + SEGURA {
        return *imp;
    }
    mistura(imp, &GUARDA, suave((t - PREPARA - CORTA - SEGURA) / VOLTA))
}

fn ombro(b: &Braco) -> Quat {
    Quat::from_rotation_y(b.guinada) * Quat::from_rotation_x(-b.elevacao)
}

/// A lamina no espaco do tronco. A malha da espada tem a lamina no +Z, o fio
/// no +Y e a face chata no X; `x(pi/2)` a pendura apontando pra baixo com o
/// fio pra frente, e dai' guinada, elevacao e giro fazem o resto.
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
fn tranco(t: f32) -> f32 {
    if t < 0.05 {
        t / 0.05
    } else {
        (-(t - 0.05) / 0.10).exp()
    }
}

fn aplica_combate(p: &mut Pose, e: &Entrada) {
    let c = &e.combate;
    if let Some(t) = c.ferido {
        let k = tranco(t);
        if k > 0.001 {
            p.rot[TORSO] = p.rot[TORSO] * Quat::from_rotation_x(-0.35 * k);
            p.rot[1] = p.rot[1] * Quat::from_rotation_x(-0.2 * k);
            p.rot[BRACO_D] = abre(1.0, 0.3 * k) * p.rot[BRACO_D];
            p.rot[BRACO_E] = abre(-1.0, 0.3 * k) * p.rot[BRACO_E];
        }
    }
    if c.conjunto != ESPADA_ESCUDO {
        return;
    }
    p.armado = true;
    let sacada = c.sacada.clamp(0.0, 1.0);
    p.na_mao = c.golpe.is_some() || sacada >= 0.5;

    // A chave do quadro e o quanto ela manda sobre a locomocao.
    let (chave, peso_bracos, peso_tronco) = match c.golpe {
        Some((passo, t)) => {
            let desde = match c.golpe_ant {
                Some((pa, ta)) => chave_do_golpe(pa, ta, &GUARDA),
                None => GUARDA,
            };
            (chave_do_golpe(passo, t, &desde), 1.0, 1.0)
        }
        None => {
            // Sacar e guardar sao o MESMO gesto de ida e volta: a mao vai ao
            // quadril na metade do caminho, e e' ai' que a arma troca de lugar.
            let bump = (sacada * PI).sin().max(0.0);
            let g = suave(sacada);
            (mistura(&GUARDA, &SACANDO, bump), (0.85 * g).max(bump), 0.6 * g)
        }
    };
    if peso_bracos <= 0.0 && !p.na_mao {
        p.punho = [orienta_lamina(&SACANDO.lamina), orienta_escudo(PI)];
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

    // o passo a' frente: perna direita adiante, a esquerda firma atras, e o
    // corpo desce um pouco
    if chave.avanco > 0.0 {
        let a = chave.avanco;
        p.rot[COXA_D] = p.rot[COXA_D] * frente(0.5 * a);
        p.rot[CANELA_D] = p.rot[CANELA_D] * dobra_pra_tras(0.35 * a);
        p.rot[COXA_E] = p.rot[COXA_E] * frente(-0.35 * a);
        p.rot[CANELA_E] = p.rot[CANELA_E] * dobra_pra_tras(0.3 * a);
        p.subida -= 1.0 * a;
    }

    // a arma na mao aponta pra onde a chave manda, qualquer que seja o braco
    let cadeia_d = p.rot[BRACO_D] * p.rot[ANTEBRACO_D];
    let cadeia_e = p.rot[BRACO_E] * p.rot[ANTEBRACO_E];
    p.punho = [
        cadeia_d.inverse() * orienta_lamina(&chave.lamina),
        cadeia_e.inverse() * orienta_escudo(chave.escudo),
    ];
}

/// A matriz de mundo da espada e do escudo, na mao ou guardados. `None` se o
/// conjunto ainda nao tem arma pra desenhar.
pub fn armas(p: &Pose, m: &[Mat4; N], voxel: f32) -> Option<[Mat4; 2]> {
    if !p.armado {
        return None;
    }
    let encaixe = |pt: [f32; 3], pai: usize| {
        Mat4::from_translation(mapa(pt, voxel) - mapa(PECAS[pai].2, voxel))
    };
    Some(if p.na_mao {
        [
            m[ANTEBRACO_D] * encaixe(MAO_D, ANTEBRACO_D) * Mat4::from_quat(p.punho[0]),
            m[ANTEBRACO_E] * encaixe(MAO_E, ANTEBRACO_E) * Mat4::from_quat(p.punho[1]),
        ]
    } else {
        [
            m[TORSO] * encaixe(QUADRIL_E, TORSO) * Mat4::from_quat(orienta_lamina(&SACANDO.lamina)),
            m[TORSO] * encaixe(COSTAS, TORSO) * Mat4::from_quat(orienta_escudo(PI)),
        ]
    })
}


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
            assert!((s.y - degrau).abs() < 0.3, "degrau {degrau}: sola em y {:.2}", s.y);
            assert!(s.z.abs() < 0.5, "degrau {degrau}: sola saiu {:.2} pra frente/trás", s.z);
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
                assert!(s.y < 4.5, "tempo {:.2}: perna encolhida, sola em y {:.2}", e.tempo, s.y);
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
            assert!(d.z > 1.5, "tempo {:.2}: a perna da frente saiu da frente ({:.2})", e.tempo, d.z);
            assert!(es.z < -1.5, "tempo {:.2}: a perna de trás saiu de trás ({:.2})", e.tempo, es.z);
            assert!(d.z - es.z > 4.0, "tempo {:.2}: as pernas se juntaram ({:.2} x {:.2})", e.tempo, d.z, es.z);
            let ombro = mapa(PECAS[BRACO_D].2, 1.0);
            let (md, me) = (mao(&p, ANTEBRACO_D), mao(&p, ANTEBRACO_E));
            assert!(md.x.abs() > ombro.x.abs() + 3.0 && me.x.abs() > ombro.x.abs() + 3.0,
                "tempo {:.2}: braço não está aberto", e.tempo);
            lo = lo.min(d.z);
            hi = hi.max(d.z);
        }
        assert!(hi - lo > 0.8, "a perna não balança no ar ({lo:.2}..{hi:.2})");
    }

    /// No meio do passo a perna direita está NA FRENTE (o +Z do rig) e a
    /// esquerda atrás — e o braço direito vai pra trás, oposto.
    #[test]
    fn andando_perna_e_braco_se_opoem() {
        let mut e = parado();
        e.andar = 1.0;
        e.fase = std::f32::consts::FRAC_PI_2;
        let p = pose(&e);
        assert!(sola(&p, COXA_D, CANELA_D).z > 2.0, "perna direita não foi pra frente");
        assert!(sola(&p, COXA_E, CANELA_E).z < -2.0, "perna esquerda não foi pra trás");
        let m = matrizes(&p, Mat4::IDENTITY, 1.0);
        let mao = m[ANTEBRACO_D].transform_point3(mapa([23.0, 12.0, 16.0], 1.0) - mapa(PECAS[ANTEBRACO_D].2, 1.0));
        assert!(mao.z < 0.0, "braço direito deveria ir pra trás, mão em {mao:?}");
    }
    // ── combate ──

    fn em_combate(golpe: Option<(u8, f32)>, golpe_ant: Option<(u8, f32)>, sacada: f32) -> Entrada {
        let mut e = parado();
        e.combate = Combate { conjunto: 0, sacada, golpe, golpe_ant, ferido: None };
        e
    }

    /// Ponta da lamina e (posicao, normal da face) do escudo, em voxels.
    fn armas_em(e: &Entrada) -> (Vec3, Vec3, Vec3) {
        let p = pose(e);
        let m = matrizes(&p, Mat4::IDENTITY, 1.0);
        let [espada, escudo] = armas(&p, &m, 1.0).expect("espada e escudo");
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
        let [espada, escudo] = armas(&p, &m, 1.0).unwrap();
        let (e, s) = (espada.transform_point3(Vec3::ZERO), escudo.transform_point3(Vec3::ZERO));
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
            assert!((18.0..40.0).contains(&p.y), "fora da altura do peito: {p:?}");
        }
        assert!((prep.y - imp.y).abs() < 10.0, "corte nao ficou horizontal");
    }

    #[test]
    fn o_terceiro_golpe_desce_de_cima() {
        let (prep, _, _) = armas_em(&em_combate(Some((2, PREPARA)), None, 1.0));
        let (imp, _, _) = armas_em(&em_combate(Some((2, PREPARA + CORTA)), None, 1.0));
        assert!(prep.y > 45.0, "preparo tem que ir la' em cima: {prep:?}");
        assert!(imp.y < 30.0 && imp.z > 10.0, "impacto a' frente e embaixo: {imp:?}");
    }

    #[test]
    fn o_golpe_nao_estala_nem_na_troca() {
        let (a, _, _) = armas_em(&em_combate(Some((0, 0.25)), None, 1.0));
        let (b, _, _) = armas_em(&em_combate(Some((1, 0.0)), Some((0, 0.25)), 1.0));
        assert!(a.distance(b) < 0.5, "troca de golpe saltou {:.1} voxels", a.distance(b));
        for passo in 0..3u8 {
            let mut antes = armas_em(&em_combate(Some((passo, 0.0)), None, 1.0));
            let mut t = 0.0;
            while t < DURACAO_DO_GOLPE {
                t += 0.001;
                let agora = armas_em(&em_combate(Some((passo, t)), None, 1.0));
                assert!(agora.0.distance(antes.0) < 3.0, "golpe {passo}: ponta saltou em t={t:.3}");
                assert!(agora.1.distance(antes.1) < 1.0, "golpe {passo}: escudo saltou em t={t:.3}");
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
            assert!(agora.0.distance(antes.0) < limite, "sacando em {s:.3}: {:.1}", agora.0.distance(antes.0));
            antes = agora;
        }
    }

    #[test]
    fn apanhar_joga_a_cabeca_pra_tras() {
        let cabeca = |e: &Entrada| matrizes(&pose(e), Mat4::IDENTITY, 1.0)[1].transform_point3(vec3(0.0, 6.0, 0.0));
        let mut e = parado();
        let parada = cabeca(&e);
        e.combate.ferido = Some(0.05);
        assert!(cabeca(&e).z < parada.z - 1.0);
    }
}
