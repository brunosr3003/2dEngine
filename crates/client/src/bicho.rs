//! As CRIATURAS em pecas: a marcha e a patada.
//!
//! Porta do zone14 (`ModeloVoxel.PassoDaPata`, `GiroDaJunta`, `DoGolpe`). As
//! patas destes bichos sao TOCOS SOLTOS — o estilo, ver
//! docs/PIPELINE_ARTE.md —, e perna solta se anima por TRANSLACAO: girar o
//! toco no proprio topo move o pe' uns 4% da altura enquanto o corpo cruza
//! metros de chao, e a caminhada le' como deslizar no gelo. O pe' faz um D:
//! reto pra tras enquanto esta' plantado, na velocidade em que o chao passa,
//! e volta pela frente levantando.
//!
//! Nao deslizar e' identidade, nao sorte:
//!
//! ```text
//! viagem do pe' no apoio == distancia que o corpo andou no apoio
//! 2 * MEIA_VIAGEM_DO_PE * altura * alonga == FRACAO_DE_APOIO * ciclo
//! ```
//!
//! e o `ciclo` sai dai. `o_pe_plantado_nao_desliza` cobra.

use macroquad::models::Mesh;
use macroquad::prelude::{vec3, Vec3};
use std::f32::consts::{PI, TAU};

/// Meia-viagem do pe', em fracao da altura do bicho: passo de bicho andando.
pub const MEIA_VIAGEM_DO_PE: f32 = 0.18;
/// Quanto do ciclo o pe' passa PLANTADO. Quadrupede andando tem sempre mais
/// pe' no chao que no ar.
pub const FRACAO_DE_APOIO: f32 = 0.60;
/// Quanto o pe' levanta na volta, em fracao da meia-viagem. Sem levantar, o D
/// vira um vai-e-vem arrastado.
pub const ALTURA_DO_PASSO: f32 = 0.42;
/// Correndo, o passo alonga em vez de so' acelerar — senao a perna vira
/// desenho animado.
pub const ALONGA_NA_CORRIDA: f32 = 1.6;
/// Velocidade (u/s) em que a marcha ja' e' trote de todo.
const VEL_DE_TROTE: f32 = 5.0;

/// A patada: arma (recua a pata), SAI (o tapa) — e so'. O telegrafo e' curto
/// porque o dano sai no servidor no mesmo tick em que o golpe comeca.
pub const GOLPE_AVISO: f32 = 0.12;
pub const GOLPE_ATIVO: f32 = 0.22;

/// Cada bicho que anda em pecas: o arquivo (`tools/voxrender/bichos.py`) e a
/// altura na tela, em unidades de mundo.
pub const BICHOS: [(&str, f32); 5] = [
    ("bichos/lobo_pequeno", 0.9),
    ("bichos/urso", 1.3),
    ("bichos/tigre", 0.95),
    ("bichos/owlbear", 1.5),
    ("bichos/lobo", 2.8),
];

/// O bicho deste mob, se ele for bicho. Gente (pistoleiro, mago, arqueiro)
/// fica de fora: ela anda no rig do personagem, nao neste.
pub fn do_mob(tag: shared::EntityTag, kind: u16, boss: bool) -> Option<(&'static str, f32)> {
    if tag != shared::EntityTag::Enemy {
        return None;
    }
    if boss {
        return Some(BICHOS[4]);
    }
    let i = match kind {
        0 | 7 => 0,
        1 => 1,
        3 => 2,
        5 => 3,
        _ => return None,
    };
    Some(BICHOS[i])
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Junta {
    Tronco,
    Cabeca,
    Pescoco,
    Cauda,
    Pata { frente: bool, esq: bool },
}

/// Nome do objeto no `.vox` -> junta.
pub fn junta_de(nome: &str) -> Option<Junta> {
    Some(match nome {
        "tronco" => Junta::Tronco,
        "cabeca" => Junta::Cabeca,
        "pescoco" => Junta::Pescoco,
        "cauda" => Junta::Cauda,
        "pata_fd" => Junta::Pata { frente: true, esq: false },
        "pata_fe" => Junta::Pata { frente: true, esq: true },
        "pata_td" => Junta::Pata { frente: false, esq: false },
        "pata_te" => Junta::Pata { frente: false, esq: true },
        _ => return None,
    })
}

/// Onde a peca gira, na tela de voxels (em cantos de voxel; `hi` inclusivo).
///
/// O bicho olha pro +Y do voxel. A pata pendura do TOPO; a cabeca e o
/// pescoco giram na borda de TRAS, que e' onde encostam no corpo; a cauda na
/// da FRENTE, pelo mesmo motivo.
pub fn pivo_vox(j: Junta, lo: [usize; 3], hi: [usize; 3]) -> [f32; 3] {
    let meio = |i: usize| (lo[i] + hi[i] + 1) as f32 * 0.5;
    match j {
        Junta::Pata { .. } => [meio(0), meio(1), (hi[2] + 1) as f32],
        Junta::Cabeca | Junta::Pescoco => [meio(0), lo[1] as f32, meio(2)],
        Junta::Cauda => [meio(0), (hi[1] + 1) as f32, meio(2)],
        Junta::Tronco => [meio(0), meio(1), meio(2)],
    }
}

pub struct PecaDeBicho {
    pub junta: Junta,
    /// Pivo no espaco do bicho, em unidades de mundo.
    pub pivo: Vec3,
    pub malhas: Vec<Mesh>,
}

pub struct Bicho {
    pub altura: f32,
    pub pecas: Vec<PecaDeBicho>,
}

/// O que a pose precisa saber do quadro.
pub struct Entrada {
    /// Fase da passada, em radianos: anda com a DISTANCIA (ver `ciclo`).
    pub passada: f32,
    /// Velocidade desenhada, u/s.
    pub vel: f32,
    /// Relogio, pra o que mexe mesmo parado (cauda, cabeca, respiro).
    pub tempo: f32,
    /// Segundos desde o comeco do ultimo golpe. Grande = sem golpe.
    pub golpe: f32,
    /// Desencontra bichos iguais lado a lado.
    pub semente: f32,
}

struct Marcha {
    amp: f32,
    /// A perna "acorda" com o movimento: parado, o passo some sem estalo.
    acorda: f32,
    /// 0 = passeio (quatro tempos) .. 1 = trote (diagonais juntas).
    corre: f32,
    alonga: f32,
}

fn marcha(vel: f32) -> Marcha {
    let amp = (vel / 4.0).clamp(0.0, 1.0);
    let corre = (vel / VEL_DE_TROTE).clamp(0.0, 1.0);
    Marcha { amp, acorda: (amp * 6.0).min(1.0), corre, alonga: 1.0 + ALONGA_NA_CORRIDA * corre }
}

/// Distancia (u) de um ciclo inteiro de passada nesta velocidade. Sai da
/// identidade do topo do modulo: o pe' plantado varre exatamente o chao que
/// passou.
pub fn ciclo(altura: f32, vel: f32) -> f32 {
    2.0 * MEIA_VIAGEM_DO_PE * altura * marcha(vel).alonga / FRACAO_DE_APOIO
}

/// Em que ponto do ciclo cada pata esta', em fracao de volta.
///
/// Passeio e' de QUATRO TEMPOS, uma pata de cada vez: traseira-esquerda,
/// dianteira-esquerda, traseira-direita, dianteira-direita. Trote em passo
/// de passeio le' como mesa andando; ele so' entra quando o bicho apressa.
pub fn atraso_da_pata(frente: bool, esq: bool, corre: f32) -> f32 {
    let passeio = match (frente, esq) {
        (false, true) => 0.0,
        (true, true) => 0.25,
        (false, false) => 0.50,
        (true, false) => 0.75,
    };
    let trote = if frente == esq { 0.0 } else { 0.50 };
    passeio + (trote - passeio) * corre
}

/// O D do pe': deslocamento (u) no espaco do bicho, +Z pra frente.
pub fn passo_da_pata(frente: bool, esq: bool, passada: f32, corre: f32, meia: f32) -> Vec3 {
    let mut u = passada / TAU + atraso_da_pata(frente, esq, corre);
    u -= u.floor();
    if u < FRACAO_DE_APOIO {
        // PLANTADO: reto pra tras, velocidade constante. Qualquer curva aqui
        // e' deslize, porque o chao passa em velocidade constante.
        vec3(0.0, 0.0, meia - 2.0 * meia * (u / FRACAO_DE_APOIO))
    } else {
        // NA VOLTA: pela frente, levantando; o seno pousa o pe' macio.
        let v = (u - FRACAO_DE_APOIO) / (1.0 - FRACAO_DE_APOIO);
        vec3(0.0, (v * PI).sin() * meia * ALTURA_DO_PASSO, -meia + 2.0 * meia * v)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Golpe {
    /// Giro somado na pata da frente (negativo = pra frente).
    pub frente: f32,
    /// Mergulho do corpo inteiro (negativo = empina).
    pub pitch: f32,
    /// 0..1: o quanto a pata esta' esticada no tapa.
    pub alcance: f32,
}

/// A patada no instante `t` desde o comeco.
///
/// No zone14 a pata saltava de volta pra posicao armada quando o tapa
/// acabava. Aqui a pata sai da posicao armada e desce ate' zero DENTRO do
/// tapa, entao a curva e' continua de ponta a ponta.
pub fn golpe(t: f32) -> Golpe {
    if !(0.0..GOLPE_AVISO + GOLPE_ATIVO).contains(&t) {
        return Golpe::default();
    }
    if t < GOLPE_AVISO {
        let a = t / GOLPE_AVISO;
        let armar = a * a * (3.0 - 2.0 * a);
        return Golpe { frente: 0.45 * armar, pitch: 0.10 * armar, alcance: 0.0 };
    }
    let sair = (t - GOLPE_AVISO) / GOLPE_ATIVO;
    let arco = (sair * PI).sin();
    Golpe {
        frente: 0.45 * (1.0 - sair) - 1.30 * arco,
        pitch: 0.10 * (1.0 - sair) - 0.16 * arco,
        alcance: arco,
    }
}

/// O corpo inteiro: quanto ele sobe (u) e quanto mergulha (rad).
///
/// O corpo sobe quando a perna abre: no extremo do passo ela esta' inclinada
/// e mais curta na vertical, entao o quadril sobe pra o pe' nao afundar.
pub fn corpo(e: &Entrada, altura: f32) -> (f32, f32) {
    let m = marcha(e.vel);
    let respiro = (e.tempo * 1.8 + e.semente * 0.37).sin() * 0.006 * altura;
    let sobe = e.passada.sin().abs() * 0.045 * altura * m.acorda + respiro;
    (sobe, golpe(e.golpe).pitch)
}

/// Giro em X (rad) e deslocamento (u, espaco do bicho) de uma peca.
pub fn peca(j: Junta, e: &Entrada, altura: f32) -> (f32, Vec3) {
    let m = marcha(e.vel);
    match j {
        Junta::Tronco => (0.0, Vec3::ZERO),
        Junta::Cauda => ((e.tempo * 2.2 + e.semente * 0.01).sin() * 0.18, Vec3::ZERO),
        Junta::Cabeca | Junta::Pescoco => {
            ((e.tempo * 1.6).sin() * 0.12 + e.passada.sin() * 0.05 * m.amp, Vec3::ZERO)
        }
        Junta::Pata { frente, esq } => {
            // o toco inclina POUCO, acompanhando o passo: quem move o pe' e'
            // a translacao. Inclinar muito volta a girar o toco no topo.
            let fase = e.passada + atraso_da_pata(frente, esq, m.corre) * TAU;
            let mut giro = fase.sin() * 0.20 * m.acorda;
            let meia = MEIA_VIAGEM_DO_PE * altura * m.acorda * m.alonga;
            let mut d = passo_da_pata(frente, esq, e.passada, m.corre, meia);
            if frente {
                // a patada e' da DIREITA; a esquerda so' firma o corpo
                let g = golpe(e.golpe);
                let peso = if esq { 0.25 } else { 1.0 };
                giro += g.frente * peso;
                d += vec3(0.0, 0.10, 0.12) * (altura * g.alcance * peso);
            }
            (giro, d)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const PATAS: [(bool, bool); 4] = [(true, true), (true, false), (false, true), (false, false)];

    #[test]
    fn o_pe_plantado_nao_desliza() {
        // o pe' no MUNDO = corpo andado + deslocamento da pata. Plantado, ele
        // nao pode sair do lugar, em nenhuma velocidade.
        for vel in [1.0f32, 3.0, 5.0] {
            let altura = 1.3;
            let c = ciclo(altura, vel);
            let m = marcha(vel);
            let meia = MEIA_VIAGEM_DO_PE * altura * m.acorda * m.alonga;
            for (frente, esq) in PATAS {
                let mut antes: Option<f32> = None;
                for k in 0..2000 {
                    let passada = k as f32 * TAU / 1000.0;
                    let d = passo_da_pata(frente, esq, passada, m.corre, meia);
                    let no_mundo = passada / TAU * c + d.z;
                    if d.y == 0.0 {
                        if let Some(a) = antes {
                            assert!((no_mundo - a).abs() < 1e-3, "vel {vel}: pe' deslizou {}", no_mundo - a);
                        }
                        antes = Some(no_mundo);
                    } else {
                        antes = None;
                    }
                }
            }
        }
    }

    #[test]
    fn passeio_e_de_quatro_tempos_e_trote_junta_as_diagonais() {
        let mut passeio: Vec<f32> = PATAS.iter().map(|&(f, e)| atraso_da_pata(f, e, 0.0)).collect();
        passeio.sort_by(|a, b| a.partial_cmp(b).unwrap());
        assert_eq!(passeio, vec![0.0, 0.25, 0.5, 0.75]);
        // diagonal = dianteira de um lado com a traseira do outro
        assert_eq!(atraso_da_pata(true, false, 1.0), atraso_da_pata(false, true, 1.0));
        assert_eq!(atraso_da_pata(true, true, 1.0), atraso_da_pata(false, false, 1.0));
        assert_ne!(atraso_da_pata(true, true, 1.0), atraso_da_pata(true, false, 1.0));
    }

    #[test]
    fn parado_a_pata_nao_se_mexe() {
        let e = Entrada { passada: 1.3, vel: 0.0, tempo: 4.0, golpe: 99.0, semente: 7.0 };
        for (frente, esq) in PATAS {
            let (giro, d) = peca(Junta::Pata { frente, esq }, &e, 1.0);
            assert_eq!(giro, 0.0);
            assert_eq!(d, Vec3::ZERO);
        }
    }

    #[test]
    fn a_patada_arma_estica_e_volta_sem_estalo() {
        assert!(golpe(GOLPE_AVISO * 0.9).frente > 0.3, "tem que recuar a pata antes");
        assert!(golpe(GOLPE_AVISO + GOLPE_ATIVO * 0.5).frente < -0.9, "o tapa vai pra frente");
        assert_eq!(golpe(GOLPE_AVISO + GOLPE_ATIVO + 0.01), Golpe::default());
        assert_eq!(golpe(99.0), Golpe::default());
        let mut antes = golpe(0.0);
        let mut t = 0.0;
        while t < 0.6 {
            t += 0.002;
            let g = golpe(t);
            assert!((g.frente - antes.frente).abs() < 0.05, "estalo em t={t}");
            assert!((g.pitch - antes.pitch).abs() < 0.01, "estalo no corpo em t={t}");
            antes = g;
        }
    }

    #[test]
    fn a_patada_e_da_direita() {
        let e = Entrada { passada: 0.0, vel: 0.0, tempo: 0.0, golpe: GOLPE_AVISO + GOLPE_ATIVO * 0.5, semente: 0.0 };
        let (direita, _) = peca(Junta::Pata { frente: true, esq: false }, &e, 1.0);
        let (esquerda, _) = peca(Junta::Pata { frente: true, esq: true }, &e, 1.0);
        let (tras, _) = peca(Junta::Pata { frente: false, esq: false }, &e, 1.0);
        assert!(direita < -0.9 && esquerda > direita * 0.5 && tras == 0.0);
    }

    #[test]
    fn so_bicho_de_quatro_patas_anda_em_pecas() {
        use shared::EntityTag as T;
        assert_eq!(do_mob(T::Enemy, 0, false).unwrap().0, "bichos/lobo_pequeno");
        assert_eq!(do_mob(T::Enemy, 1, false).unwrap().0, "bichos/urso");
        assert_eq!(do_mob(T::Enemy, 0, true).unwrap().0, "bichos/lobo");
        for gente in [2u16, 4, 6] {
            assert!(do_mob(T::Enemy, gente, false).is_none());
        }
        assert!(do_mob(T::Player, 0, false).is_none());
    }
}
