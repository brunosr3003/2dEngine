//! A animacao do golpe de chefe: PREPARAR, BATER, RECUPERAR.
//!
//! O servidor manda o telegrafico (forma, centro, direcao, carga) e o fim; o
//! desenho do chefe le' daqui um AJUSTE por quadro — agachar, empinar, girar,
//! avancar, saltar — somado a' pose que o bicho ou o boneco ja' teria. Nada
//! aqui decide acerto: e' leitura, pra o jogador ver o golpe vindo e sair.
//!
//! Os tres tempos:
//!
//! 1. PREPARA — a carga inteira do telegrafico. Antecipacao exagerada e
//!    crescente (quem joga precisa ler de longe, com camera alta), com tremor
//!    no fim: e' o "vai sair agora".
//! 2. GOLPE — `GOLPE_S` a partir do impacto. Rapido e amplo, na direcao da
//!    forma: a investida anda de verdade, o salto sobe e cai no circulo, o giro
//!    da' a volta, a patada varre o cone, o pisao esmaga o chao.
//! 3. RECUPERA — `RECUPERA_S` de pausa pesada, voltando pro lugar.

use macroquad::prelude::*;
use shared::bosses::Forma;
use std::f32::consts::{PI, TAU};

/// Quanto o golpe dura depois do impacto (o movimento rapido).
pub const GOLPE_S: f32 = 0.30;
/// A pausa pesada depois do golpe.
pub const RECUPERA_S: f32 = 0.75;
/// Tremor de camera no impacto perto do jogador: forca maxima (u) e duracao.
pub const TREMOR_FORCA: f32 = 0.22;
pub const TREMOR_S: f32 = 0.35;

/// O gesto, pela forma do golpe e por onde ela cai.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Golpe {
    /// Linha a partir do chefe: agacha e dispara pra frente.
    Investida,
    /// Circulo em volta de si: empina e esmaga o chao.
    Pisao,
    /// Circulo no chao do alvo: agacha, salta e cai la'.
    Salto,
    /// Cone a' frente: torce pro lado e varre.
    Varrida,
    /// Anel em volta de si: arma o giro e roda.
    Giro,
}

pub fn golpe_de(forma: &Forma, centro: Vec2, chefe: Vec2) -> Golpe {
    match forma {
        Forma::Linha { .. } => Golpe::Investida,
        Forma::Cone { .. } => Golpe::Varrida,
        Forma::Anel { .. } => Golpe::Giro,
        Forma::Circulo { .. } => {
            if centro.distance(chefe) > 1.5 {
                Golpe::Salto
            } else {
                Golpe::Pisao
            }
        }
    }
}

/// O golpe carregando num chefe.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Carga {
    pub golpe: Golpe,
    /// Pra onde o golpe vai, no mundo (x, z).
    pub dir: Vec2,
    /// Quanto o corpo anda no golpe (u): investida e salto.
    pub alcance: f32,
    pub inicio: f64,
    pub carga_s: f32,
    /// Quando o impacto chegou do servidor, se chegou.
    pub impacto: Option<f64>,
}

impl Carga {
    pub fn nova(forma: &Forma, centro: Vec2, dir: Vec2, chefe: Vec2, carga_s: f32, agora: f64) -> Self {
        let golpe = golpe_de(forma, centro, chefe);
        let (dir, alcance) = match (golpe, forma) {
            (Golpe::Salto, _) => {
                let d = centro - chefe;
                (d.normalize_or(dir.normalize_or(Vec2::X)), d.length() * 0.85)
            }
            // Anda um bom pedaco da linha, sem sair do desenho da tela.
            (Golpe::Investida, Forma::Linha { comprimento, .. }) => (dir.normalize_or(Vec2::X), (comprimento * 0.55).min(7.0)),
            _ => (dir.normalize_or(Vec2::X), 0.0),
        };
        Self { golpe, dir, alcance, inicio: agora, carga_s: carga_s.max(0.05), impacto: None }
    }

    fn impacto_em(&self) -> f64 {
        self.impacto.unwrap_or(self.inicio + self.carga_s as f64)
    }

    /// O servidor confirmou o impacto. Fica no MAXIMO um pouco depois da hora
    /// prevista: se a mensagem atrasou, o golpe ja' saiu no desenho e nao volta
    /// pra preparacao.
    pub fn bateu(&mut self, agora: f64) {
        let previsto = self.inicio + self.carga_s as f64;
        self.impacto = Some(agora.min(previsto + 0.15).max(self.inicio));
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Tempo {
    Prepara(f32),
    Golpe(f32),
    Recupera(f32),
    Fim,
}

pub fn tempo(c: &Carga, agora: f64) -> Tempo {
    let imp = c.impacto_em();
    if agora < imp {
        let u = ((agora - c.inicio) / (imp - c.inicio).max(0.05)) as f32;
        return Tempo::Prepara(u.clamp(0.0, 1.0));
    }
    let d = (agora - imp) as f32;
    if d < GOLPE_S {
        return Tempo::Golpe(d / GOLPE_S);
    }
    let d = d - GOLPE_S;
    if d < RECUPERA_S {
        return Tempo::Recupera(d / RECUPERA_S);
    }
    Tempo::Fim
}

/// O que o golpe soma a' pose do quadro.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Ajuste {
    /// Mergulho do tronco (rad; negativo = empina).
    pub pitch: f32,
    /// Quanto o tronco sobe, em fracao da altura.
    pub sobe: f32,
    /// Salto: quanto o corpo INTEIRO sai do chao, em fracao da altura.
    pub voa: f32,
    /// Recuo (-) ou avanco (+) do tronco, em fracao da altura.
    pub avanca: f32,
    /// Quanto o corpo inteiro anda no mundo na direcao do golpe (u).
    pub desloca: f32,
    /// Giro extra em volta de Y (rad).
    pub giro: f32,
    /// Agachamento (0..~0.3): achata na vertical.
    pub agacha: f32,
    /// Tremor da antecipacao (0..1).
    pub tremor: f32,
}

fn mistura(a: Ajuste, b: Ajuste, k: f32) -> Ajuste {
    let l = |x: f32, y: f32| x + (y - x) * k;
    Ajuste {
        pitch: l(a.pitch, b.pitch),
        sobe: l(a.sobe, b.sobe),
        voa: l(a.voa, b.voa),
        avanca: l(a.avanca, b.avanca),
        desloca: l(a.desloca, b.desloca),
        giro: l(a.giro, b.giro),
        agacha: l(a.agacha, b.agacha),
        tremor: l(a.tremor, b.tremor),
    }
}

fn suave(u: f32) -> f32 {
    u * u * (3.0 - 2.0 * u)
}

fn sai_rapido(u: f32) -> f32 {
    1.0 - (1.0 - u).powi(3)
}

/// A pose no fim da preparacao: o ponto mais armado.
fn armado(g: Golpe) -> Ajuste {
    match g {
        Golpe::Investida => Ajuste { pitch: 0.20, agacha: 0.20, avanca: -0.16, ..Default::default() },
        Golpe::Pisao => Ajuste { pitch: -0.62, sobe: 0.30, avanca: -0.06, ..Default::default() },
        Golpe::Salto => Ajuste { pitch: 0.14, agacha: 0.28, avanca: -0.08, ..Default::default() },
        Golpe::Varrida => Ajuste { giro: -0.75, pitch: -0.18, sobe: 0.06, ..Default::default() },
        Golpe::Giro => Ajuste { giro: -0.95, agacha: 0.14, pitch: 0.06, ..Default::default() },
    }
}

/// A pose no fim do golpe: o golpe dado.
fn batido(g: Golpe, alcance: f32) -> Ajuste {
    match g {
        Golpe::Investida => Ajuste { pitch: 0.10, agacha: 0.06, avanca: 0.12, desloca: alcance, ..Default::default() },
        Golpe::Pisao => Ajuste { pitch: 0.28, sobe: -0.02, agacha: 0.32, avanca: 0.08, ..Default::default() },
        Golpe::Salto => Ajuste { agacha: 0.24, pitch: 0.10, desloca: alcance, ..Default::default() },
        Golpe::Varrida => Ajuste { giro: 0.85, pitch: 0.14, avanca: 0.12, ..Default::default() },
        Golpe::Giro => Ajuste { giro: -0.95 + TAU, agacha: 0.05, ..Default::default() },
    }
}

/// Onde o corpo termina a recuperacao: parado. O giro termina numa volta
/// inteira — voltar a zero desgiraria o boneco pra tras.
fn descanso(g: Golpe) -> Ajuste {
    Ajuste { giro: if g == Golpe::Giro { TAU } else { 0.0 }, ..Default::default() }
}

/// O ajuste deste quadro, ou `None` quando o golpe ja' terminou.
pub fn ajuste(c: &Carga, agora: f64) -> Option<Ajuste> {
    Some(match tempo(c, agora) {
        Tempo::Prepara(u) => {
            let mut a = mistura(Ajuste::default(), armado(c.golpe), suave(u));
            // Treme so' no fim, crescendo: o aviso de que vai sair.
            a.tremor = ((u - 0.55) / 0.45).clamp(0.0, 1.0).powi(2);
            a
        }
        Tempo::Golpe(u) => {
            let mut a = mistura(armado(c.golpe), batido(c.golpe, c.alcance), sai_rapido(u));
            if c.golpe == Golpe::Salto {
                a.voa = (u * PI).sin() * 0.9;
            }
            a
        }
        Tempo::Recupera(u) => mistura(batido(c.golpe, c.alcance), descanso(c.golpe), suave(u)),
        Tempo::Fim => return None,
    })
}

/// Relogio da PATADA do bicho (`bicho::golpe`) pra varrida: a pata arma na
/// preparacao, varre no golpe e volta na recuperacao. Outros golpes usam a
/// pata normal.
pub fn relogio_da_pata(c: &Carga, agora: f64) -> Option<f32> {
    if c.golpe != Golpe::Varrida {
        return None;
    }
    use crate::bicho::{T_LEVANTA, T_VARRE, T_VOLTA};
    Some(match tempo(c, agora) {
        Tempo::Prepara(u) => T_LEVANTA * u.min(0.999),
        Tempo::Golpe(u) => T_LEVANTA + T_VARRE * u,
        Tempo::Recupera(u) => T_LEVANTA + T_VARRE + T_VOLTA * u.min(0.999),
        Tempo::Fim => return None,
    })
}

/// Relogio do golpe do BRACO (rig de gente, `impacto` = `rig::IMPACTO`): o
/// braco arma ate' quase o impacto na carga inteira e so' completa no golpe.
pub fn relogio_do_braco(c: &Carga, agora: f64, impacto: f32) -> f32 {
    match tempo(c, agora) {
        Tempo::Prepara(u) => impacto * 0.9 * u,
        Tempo::Golpe(u) => impacto * 0.9 + impacto * 0.1 * u + 0.15 * u,
        Tempo::Recupera(u) => impacto + 0.15 + 0.35 * u,
        Tempo::Fim => 99.0,
    }
}

/// O tremor da antecipacao, no mundo (x, z), pra um corpo de `tamanho` u.
pub fn tremor_xz(a: &Ajuste, relogio: f32, tamanho: f32) -> Vec2 {
    vec2((relogio * 53.0).sin(), (relogio * 47.0).cos()) * (0.035 * tamanho * a.tremor)
}

/// Deslocamento de camera do tremor de impacto: some sozinho e nunca passa de
/// `TREMOR_FORCA`.
pub fn sacudida(forca: f32, ate: f64, agora: f64) -> Vec2 {
    let r = (ate - agora) as f32;
    if r <= 0.0 || forca <= 0.0 {
        return Vec2::ZERO;
    }
    let k = forca.min(TREMOR_FORCA) * (r / TREMOR_S).min(1.0);
    vec2((agora * 61.0).sin() as f32, (agora * 47.0).cos() as f32) * k
}

/// A cor do golpe: o elemento do chefe (fogo do pirata e do saqueador, gelo
/// do tigre, raio da tempestade, arcano do arquimago, espinho da arqueira,
/// terra dos bichos pesados).
pub fn cor_do_chefe(kind: u16) -> [u8; 3] {
    match kind {
        11 | 15 => [255, 140, 40],
        13 => [175, 225, 255],
        14 => [170, 160, 255],
        16 => [140, 200, 90],
        18 => [205, 130, 255],
        12 | 17 => [150, 115, 80],
        _ => [165, 135, 100],
    }
}

/// Onde as rajadas do impacto saem: pontos DENTRO da forma, espalhados.
pub fn pontos_de_impacto(forma: &Forma, centro: Vec2, dir: Vec2) -> Vec<Vec2> {
    let dir = dir.normalize_or(Vec2::X);
    let ang = dir.y.atan2(dir.x);
    match *forma {
        Forma::Circulo { raio } => {
            let mut v = vec![centro];
            v.extend((0..5).map(|k| centro + Vec2::from_angle(k as f32 * TAU / 5.0 + 0.3) * raio * 0.6));
            v
        }
        Forma::Anel { interno, externo } => {
            let r = (interno + externo) * 0.5;
            (0..6).map(|k| centro + Vec2::from_angle(k as f32 * TAU / 6.0) * r).collect()
        }
        Forma::Cone { raio, abertura } => [(-0.6f32, 0.45f32), (0.0, 0.3), (0.0, 0.75), (0.6, 0.6)]
            .iter()
            .map(|&(l, r)| centro + Vec2::from_angle(ang + l * abertura) * raio * r)
            .collect(),
        Forma::Linha { comprimento, .. } => (1..=5).map(|k| centro + dir * comprimento * (k as f32 / 5.5)).collect(),
    }
}

/// Uma emissao em taxa fixa por segundo, sem estado: o quadro cruzou um
/// multiplo de `1/taxa` (defasado por `semente`)?
pub fn deve_emitir(agora: f32, dt: f32, taxa: f32, semente: f32) -> bool {
    ((agora - dt) * taxa + semente).floor() != (agora * taxa + semente).floor()
}

/// Presenca de chefe vivo: sombra larga no chao e uma aura de faiscas lentas
/// na cor do elemento dele.
pub fn presenca(id: u64, kind: u16, p: Vec3, altura: f32, agora: f32, dt: f32) {
    // Sombra: disco escuro rente ao chao, com as duas faces.
    let raio = altura * 0.55;
    const LADOS: usize = 20;
    let mut vertices = Vec::with_capacity(LADOS + 1);
    let mut indices: Vec<u16> = Vec::with_capacity(LADOS * 6);
    let base = |q: Vec3, a: u8| Vertex { position: q, uv: Vec2::ZERO, color: [0, 0, 0, a], normal: Vec4::ZERO };
    vertices.push(base(p + vec3(0.0, 0.04, 0.0), 90));
    for k in 0..LADOS {
        let a = k as f32 / LADOS as f32 * TAU;
        vertices.push(base(p + vec3(a.cos() * raio, 0.04, a.sin() * raio), 0));
    }
    for k in 0..LADOS as u16 {
        let (b, c) = (1 + k, 1 + (k + 1) % LADOS as u16);
        indices.extend_from_slice(&[0, b, c, 0, c, b]);
    }
    draw_mesh(&Mesh { vertices, indices, texture: None });
    // Aura: poucas faiscas por segundo, subindo devagar em volta do corpo.
    let semente = (id % 997) as f32 * 0.013;
    if deve_emitir(agora, dt, 7.0, semente) {
        let s = (agora * 1000.0) as u32 ^ (id as u32).wrapping_mul(2_654_435_761);
        let ang = (s % 628) as f32 * 0.01;
        let alt = ((s >> 10) % 100) as f32 * 0.01 * altura * 0.8;
        let q = p + vec3(ang.cos() * altura * 0.4, alt, ang.sin() * altura * 0.4);
        crate::lascas::aura(q, cor_do_chefe(kind), s);
    }
}

/// O tombo do chefe bate no chao (`render3d::queda_de_chefe` cai em 1 s): uma
/// nuvem de poeira grande, uma vez, no quadro em que cruza.
pub fn poeira_da_queda(morte: Option<f32>, dt: f32, p: Vec3, altura: f32, kind: u16) {
    let Some(t) = morte else { return };
    if t - dt < 1.0 && t >= 1.0 {
        let cor = [120, 100, 80];
        for k in 0..6u32 {
            let a = k as f32 * TAU / 6.0;
            let q = p + vec3(a.cos() * altura * 0.35, 0.1, a.sin() * altura * 0.35);
            crate::lascas::explosao(q, cor, kind as u32 * 97 + k);
        }
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    fn carga(forma: Forma, centro: Vec2) -> Carga {
        Carga::nova(&forma, centro, Vec2::X, Vec2::ZERO, 1.0, 10.0)
    }

    const FORMAS: [(Forma, [f32; 2], Golpe); 5] = [
        (Forma::Linha { comprimento: 12.0, largura: 2.0 }, [0.0, 0.0], Golpe::Investida),
        (Forma::Circulo { raio: 5.0 }, [0.0, 0.0], Golpe::Pisao),
        (Forma::Circulo { raio: 3.0 }, [8.0, 0.0], Golpe::Salto),
        (Forma::Cone { raio: 4.0, abertura: 0.7 }, [0.0, 0.0], Golpe::Varrida),
        (Forma::Anel { interno: 2.0, externo: 6.0 }, [0.0, 0.0], Golpe::Giro),
    ];

    #[test]
    fn o_gesto_sai_da_forma_e_de_onde_ela_cai() {
        for (f, c, g) in FORMAS {
            assert_eq!(golpe_de(&f, vec2(c[0], c[1]), Vec2::ZERO), g, "{f:?}");
        }
    }

    #[test]
    fn prepara_golpe_e_recupera_no_tempo_certo() {
        let c = carga(Forma::Circulo { raio: 5.0 }, Vec2::ZERO);
        assert_eq!(tempo(&c, 10.0), Tempo::Prepara(0.0));
        assert!(matches!(tempo(&c, 10.5), Tempo::Prepara(u) if (u - 0.5).abs() < 1e-4));
        assert!(matches!(tempo(&c, 11.0 + 0.15), Tempo::Golpe(u) if (u - 0.5).abs() < 1e-3));
        assert!(matches!(tempo(&c, 11.0 + GOLPE_S as f64 + 0.1), Tempo::Recupera(_)));
        assert_eq!(tempo(&c, 11.0 + (GOLPE_S + RECUPERA_S) as f64 + 0.01), Tempo::Fim);
        // Impacto do servidor adiantado: o golpe sai na hora dele.
        let mut cedo = c;
        cedo.bateu(10.8);
        assert!(matches!(tempo(&cedo, 10.85), Tempo::Golpe(_)));
        // Mensagem muito atrasada nao volta o desenho pra preparacao.
        let mut tarde = c;
        tarde.bateu(12.5);
        assert!(!matches!(tempo(&tarde, 12.5), Tempo::Prepara(_)));
    }

    #[test]
    fn cada_gesto_arma_bate_e_volta_pro_lugar() {
        for (f, cen, g) in FORMAS {
            let c = carga(f, vec2(cen[0], cen[1]));
            let armado = ajuste(&c, 10.999).unwrap();
            let fim_do_golpe = ajuste(&c, 11.0 + GOLPE_S as f64 - 1e-3).unwrap();
            let quase_parado = ajuste(&c, 11.0 + (GOLPE_S + RECUPERA_S) as f64 - 1e-3).unwrap();
            assert!(ajuste(&c, 13.0).is_none(), "{g:?} nao terminou");
            assert!(armado.tremor > 0.5, "{g:?}: sem tremor no fim da carga");
            assert!(ajuste(&c, 10.2).unwrap().tremor == 0.0, "{g:?}: tremor cedo demais");
            let repouso = descanso(g);
            for (nome, v, alvo) in [
                ("pitch", quase_parado.pitch, 0.0),
                ("desloca", quase_parado.desloca, 0.0),
                ("agacha", quase_parado.agacha, 0.0),
                ("giro", quase_parado.giro, repouso.giro),
            ] {
                assert!((v - alvo).abs() < 0.02, "{g:?}: {nome} nao voltou ({v})");
            }
            match g {
                Golpe::Investida => {
                    assert!(armado.avanca < 0.0, "investida recua antes");
                    assert!(fim_do_golpe.desloca > 5.0, "investida anda: {}", fim_do_golpe.desloca);
                }
                Golpe::Pisao => {
                    assert!(armado.sobe > 0.2 && armado.pitch < -0.4, "pisao empina");
                    assert!(fim_do_golpe.agacha > 0.25, "pisao esmaga");
                }
                Golpe::Salto => {
                    assert!(armado.agacha > 0.2, "salto agacha");
                    let meio = ajuste(&c, 11.0 + GOLPE_S as f64 * 0.5).unwrap();
                    assert!(meio.voa > 0.8, "salto sobe: {}", meio.voa);
                    assert!(fim_do_golpe.desloca > 6.0, "salto cai no circulo");
                }
                Golpe::Varrida => {
                    assert!(armado.giro < -0.5 && fim_do_golpe.giro > 0.5, "varrida vai de um lado ao outro");
                    assert!(relogio_da_pata(&c, 11.1).is_some(), "a pata varre junto");
                }
                Golpe::Giro => {
                    assert!(fim_do_golpe.giro - armado.giro > TAU - 0.1, "giro da' a volta inteira");
                }
            }
        }
    }

    #[test]
    fn relogios_de_pata_e_braco_andam_pra_frente() {
        let c = carga(Forma::Cone { raio: 4.0, abertura: 0.7 }, Vec2::ZERO);
        let mut ant = -1.0;
        let mut ant_b = -1.0;
        for k in 0..=200 {
            let t = 10.0 + k as f64 * 0.01;
            if let Some(r) = relogio_da_pata(&c, t) {
                assert!(r >= ant, "pata voltou no relogio em {t}");
                ant = r;
            }
            let b = relogio_do_braco(&c, t, 0.3);
            if b < 99.0 {
                assert!(b >= ant_b, "braco voltou no relogio em {t}");
                ant_b = b;
            }
        }
        assert!(relogio_da_pata(&carga(Forma::Circulo { raio: 5.0 }, Vec2::ZERO), 10.5).is_none());
    }

    #[test]
    fn tremor_de_camera_some_e_tem_teto() {
        assert_eq!(sacudida(0.2, 5.0, 5.1), Vec2::ZERO);
        let v = sacudida(9.0, 5.0 + TREMOR_S as f64, 5.0);
        assert!(v.length() <= TREMOR_FORCA * 1.5, "forca fora do teto: {v:?}");
    }

    #[test]
    fn rajadas_do_impacto_caem_dentro_da_forma() {
        for (f, cen, _) in FORMAS {
            let c = vec2(cen[0], cen[1]);
            let pontos = pontos_de_impacto(&f, c, Vec2::X);
            assert!(!pontos.is_empty() && pontos.len() <= 7, "{f:?}");
            let g = |v: Vec2| ::glam::Vec2::new(v.x, v.y);
            for p in pontos {
                assert!(f.contem(g(c), ::glam::Vec2::X, g(p)), "{f:?}: {p:?} fora");
            }
        }
    }

    #[test]
    fn emissao_em_taxa_fixa() {
        let (mut n, mut t) = (0, 0.0f32);
        while t < 2.0 {
            t += 1.0 / 60.0;
            if deve_emitir(t, 1.0 / 60.0, 7.0, 0.3) {
                n += 1;
            }
        }
        assert!((13..=15).contains(&n), "7/s em 2 s deu {n}");
    }
}
