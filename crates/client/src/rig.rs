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
}

/// Rotação de cada peça em volta do próprio pivô, e quanto o corpo sobe.
pub struct Pose {
    pub rot: [Quat; N],
    /// Subida da raiz, em voxels.
    pub subida: f32,
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

pub fn pose(e: &Entrada) -> Pose {
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
    Pose { rot, subida: chao.subida * (1.0 - ar) }
}

/// O corpo no ar. NAO e' pulo: o jogo nao tem pulo atletico — o que tira o
/// pe' do chao e' vencer um degrau de dois ou tres blocos, ou cair de uma
/// borda. A primeira versao encolhia as pernas subindo e jogava os bracos pra
/// fora caindo, e contava uma historia que nao acontece. Aqui e' so' o corpo
/// solto — igual subindo e descendo.
///
/// Solto nao e' parado: a versao parada ficava dura no ar. Pernas e bracos
/// vao e voltam num balanco leve, perna contra braco como no passo, com o
/// joelho dobrando na volta. Anda pelo TEMPO e nao pela distancia, porque o
/// corpo sobe um degrau quase sem andar — pela distancia, ficaria parado de
/// novo justamente ali.
fn pose_no_ar(tempo: f32) -> Pose {
    let mut rot = [Quat::IDENTITY; N];
    let s = (tempo * std::f32::consts::TAU * AR_BALANCO_HZ).sin();
    rot[COXA_D] = frente(0.10 + 0.35 * s);
    rot[COXA_E] = frente(0.10 - 0.35 * s);
    rot[CANELA_D] = dobra_pra_tras(0.25 + 0.30 * (-s).max(0.0));
    rot[CANELA_E] = dobra_pra_tras(0.25 + 0.30 * s.max(0.0));
    rot[BRACO_D] = abre(1.0, 0.25) * frente(-0.45 * s);
    rot[BRACO_E] = abre(-1.0, 0.25) * frente(0.45 * s);
    rot[ANTEBRACO_D] = frente(0.3);
    rot[ANTEBRACO_E] = frente(0.3);
    Pose { rot, subida: 0.0 }
}

/// Balancos por segundo de perna e braco no ar. O ar dura 0,6 s (o arco do
/// degrau), entao isto da' pouco mais de um vai-e-volta por subida.
const AR_BALANCO_HZ: f32 = 2.2;

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
    Pose { rot, subida }
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

#[cfg(test)]
mod testes {
    use super::*;

    fn parado() -> Entrada {
        Entrada { fase: 0.0, andar: 0.0, correr: 0.0, tempo: 0.0, ar: 0.0, degrau: [0.0, 0.0] }
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

    /// No ar o corpo fica SOLTO, não encolhido: sola perto da altura de
    /// parado e braço perto do corpo. Encolher a perna e jogar o braço pra
    /// fora é pose de pulo, e o jogo não tem pulo atlético.
    #[test]
    fn no_ar_o_corpo_fica_solto_e_nao_pula() {
        let mut e = parado();
        e.ar = 1.0;
        let p = pose(&e);
        for (c, k) in [(COXA_D, CANELA_D), (COXA_E, CANELA_E)] {
            let s = sola(&p, c, k);
            assert!(s.y < 1.5, "perna encolhida no ar: sola em y {:.2}", s.y);
        }
        let m = matrizes(&p, Mat4::IDENTITY, 1.0);
        let mao = m[ANTEBRACO_D].transform_point3(mapa([23.0, 12.0, 16.0], 1.0) - mapa(PECAS[ANTEBRACO_D].2, 1.0));
        assert!(mao.x.abs() < 11.0, "braço jogado pra fora no ar: mão em x {:.2}", mao.x);
        // Em nenhum ponto do balanço a perna encolhe como num pulo. Voltando,
        // o joelho dobra e o calcanhar sobe uns 3 voxels — é o chute natural
        // da perna. A pose de pulo antiga levantava a sola uns 8.
        for k in 0..20 {
            e.tempo = k as f32 * 0.025;
            let p = pose(&e);
            let s = sola(&p, COXA_D, CANELA_D);
            assert!(s.y < 4.5, "no tempo {:.3} a perna encolheu: sola em y {:.2}", e.tempo, s.y);
        }
    }

    /// Solto não é parado: no ar a perna e o braço VÃO E VOLTAM — a versão
    /// sem movimento ficava dura. Meio balanço depois, a perna direita que
    /// estava na frente está atrás, e o braço direito o contrário.
    #[test]
    fn no_ar_pernas_e_bracos_vao_e_voltam() {
        let mut e = parado();
        e.ar = 1.0;
        let quarto = 0.25 / AR_BALANCO_HZ;
        e.tempo = quarto;
        let ida = pose(&e);
        e.tempo = 3.0 * quarto;
        let volta = pose(&e);
        let (a, b) = (sola(&ida, COXA_D, CANELA_D).z, sola(&volta, COXA_D, CANELA_D).z);
        assert!(a - b > 4.0, "a perna direita não foi e voltou: {a:.2} -> {b:.2}");
        let mao = |p: &Pose| matrizes(p, Mat4::IDENTITY, 1.0)[ANTEBRACO_D]
            .transform_point3(mapa([23.0, 12.0, 16.0], 1.0) - mapa(PECAS[ANTEBRACO_D].2, 1.0)).z;
        assert!(mao(&volta) - mao(&ida) > 3.0, "o braço direito não se opôs à perna");
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
}
