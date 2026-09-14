//! Chefes de campo: quem sao, onde moram e o que fazem.
//!
//! Um chefe e' um MOB PRESET no modelo de corpo inteiro (o arquivo detalhado
//! que o mob comum nao usa por orcamento), maior, com nome proprio e um kit de
//! golpes TELEGRAFADOS: o chefe para, a forma do golpe aparece no chao e
//! cresce ate' o impacto; quem ainda estiver dentro NAQUELE instante toma. O
//! servidor decide com a posicao dele — o desenho do cliente e' so' aviso.
//!
//! Tudo aqui e' dado e conta pura, pra servidor e cliente lerem o mesmo
//! catalogo (o cliente precisa do corpo e da escala; o servidor, do resto).

use serde::{Deserialize, Serialize};

/// A forma de um golpe no chao. Medidas em unidades de mundo; `abertura` do
/// cone e' o MEIO-angulo, em radianos.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum Forma {
    /// Disco centrado no `centro`.
    Circulo { raio: f32 },
    /// Setor a partir do `centro`, aberto pra `dir`.
    Cone { raio: f32, abertura: f32 },
    /// Retangulo que SAI do `centro` pra `dir`.
    Linha { comprimento: f32, largura: f32 },
    /// Coroa: seguro colado no centro e longe; perigoso no meio.
    Anel { interno: f32, externo: f32 },
}

impl Forma {
    /// `p` esta' dentro da forma posta em `centro`, virada pra `dir`?
    ///
    /// Mede o CENTRO do corpo — um jogador com meio ombro pra fora da borda
    /// escapou, que e' o que o olho espera de uma esquiva.
    pub fn contem(&self, centro: glam::Vec2, dir: glam::Vec2, p: glam::Vec2) -> bool {
        let v = p - centro;
        let d = v.length();
        let dir = dir.normalize_or(glam::Vec2::X);
        match *self {
            Forma::Circulo { raio } => d <= raio,
            Forma::Cone { raio, abertura } => {
                if d > raio {
                    return false;
                }
                if d < 1e-3 {
                    return true;
                }
                (v / d).dot(dir).clamp(-1.0, 1.0).acos() <= abertura
            }
            Forma::Linha { comprimento, largura } => {
                let ao_longo = v.dot(dir);
                ao_longo >= 0.0 && ao_longo <= comprimento && v.perp_dot(dir).abs() <= largura * 0.5
            }
            Forma::Anel { interno, externo } => d >= interno && d <= externo,
        }
    }

    /// Distancia maxima do centro que a forma alcanca (pro corte de desenho e
    /// de quem recebe o aviso).
    pub fn alcance(&self) -> f32 {
        match *self {
            Forma::Circulo { raio } | Forma::Cone { raio, .. } => raio,
            Forma::Linha { comprimento, largura } => (comprimento * comprimento + largura * largura * 0.25).sqrt(),
            Forma::Anel { externo, .. } => externo,
        }
    }
}

/// Onde a forma e' posta quando o golpe COMECA a carregar.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mira {
    /// No chao onde o alvo estava — ele tem a carga inteira pra sair.
    NoAlvo,
    /// Em volta do proprio chefe.
    EmSi,
    /// A partir do chefe, virada pro alvo (cone, investida).
    AFrente,
}

#[derive(Debug, Clone, Copy)]
pub struct Habilidade {
    pub nome: &'static str,
    pub forma: Forma,
    pub mira: Mira,
    /// Segundos entre o aviso e o impacto (fase 0).
    pub carga_s: f32,
    /// Multiplicador do dano base do chefe.
    pub dano_mult: f32,
    /// Recarga desta habilidade (fase 0). Sempre maior que a carga.
    pub recarga_s: f32,
    /// Distancia maxima do alvo pra comecar a carregar.
    pub alcance: f32,
    /// 0 = sempre; 1 = so' abaixo de metade da vida.
    pub fase_min: u8,
    /// Empurrao no impacto, em unidades.
    pub empurra: f32,
}

/// De que corpo o chefe e' feito: o KIND do mob preset.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Corpo {
    /// Bicho de quatro patas (`kind` do mob comum: 0 lobo, 1 urso, 3 tigre,
    /// 5 owlbear; 7 = o lobo grande de corpo inteiro).
    Bicho(u16),
    /// Gente (2 pistoleiro, 4 mago, 6 arqueiro).
    Gente(u16),
    /// O capitao pirata: o corpo do personagem com chapeu.
    Pirata,
}

#[derive(Debug)]
pub struct Chefe {
    /// Kind no fio (`EntityKind::Enemy`). Chefe comeca em `KIND_MIN`.
    pub kind: u16,
    pub nome: &'static str,
    pub corpo: Corpo,
    /// Escala sobre o tamanho do corpo preset.
    pub escala: f32,
    /// Ilha (`DefIlha::zona`).
    pub zona: &'static str,
    pub nivel: u32,
    pub habilidades: &'static [Habilidade],
}

/// Primeiro kind de chefe: 0..9 sao mob comum (7 = o lobo grande antigo).
pub const KIND_MIN: u16 = 10;
/// Pausa depois de um impacto antes do proximo golpe telegrafado: o jogador
/// respira e o chefe volta a bater normal.
pub const PAUSA_ENTRE_GOLPES: f32 = 2.2;
/// Cadencia MINIMA do golpe comum do chefe. O corpo preset traz a do bicho
/// (tigre 1,0 s, pistoleiro 1,5 s) e isso fazia o golpe comum — que nao se
/// esquiva — matar quem luta de pano antes do telegrafado importar. Medido na
/// simulacao de balanceamento: Tigre das Neves derrubava Pistolas e Anel em
/// ~12 s mesmo esquivando tudo.
pub const CADENCIA_COMUM_S: f32 = 2.4;
/// O golpe telegrafado tira `dano_mult × VIDA_POR_MULT` da VIDA MAXIMA de
/// quem ficou dentro (2,4 → 29%).
pub const VIDA_POR_MULT: f32 = 0.12;
/// Quanto da resistencia do alvo VALE contra o telegrafado: nada. So' desviar
/// salva. Com metade valendo, a espada e escudo (defesa + 40% do escudo + 10%
/// da pesada, no teto de 90%) ainda vencia parada no meio dos golpes — e a
/// resistencia continua valendo inteira contra o golpe comum, que e' onde o
/// tanque segura.
pub const RESISTENCIA_NO_TELEGRAFICO: f32 = 0.0;
/// Fase 2 bate mais forte (alem de carregar e recarregar mais rapido).
pub const BONUS_DA_FASE_2: f32 = 1.15;

const fn h(
    nome: &'static str,
    forma: Forma,
    mira: Mira,
    carga_s: f32,
    dano_mult: f32,
    recarga_s: f32,
    alcance: f32,
    fase_min: u8,
    empurra: f32,
) -> Habilidade {
    Habilidade { nome, forma, mira, carga_s, dano_mult, recarga_s, alcance, fase_min, empurra }
}

use Forma::{Anel, Circulo, Cone, Linha};
use Mira::{AFrente, EmSi, NoAlvo};

/// Os chefes de campo: um por mob preset de corpo inteiro. Os mais fracos no
/// Bosque (a ilha que roda localmente tem tres, pra testar), os fortes
/// subindo pelas ilhas.
pub const CHEFES: [Chefe; 9] = [
    Chefe {
        kind: 10, nome: "Lobo Alfa da Clareira", corpo: Corpo::Bicho(0), escala: 2.3,
        zona: "ilha_inicial", nivel: 8,
        habilidades: &[
            h("Mordida Dilacerante", Cone { raio: 3.6, abertura: 0.6 }, AFrente, 0.8, 2.4, 7.0, 3.6, 0, 0.6),
            h("Investida", Linha { comprimento: 12.0, largura: 2.2 }, AFrente, 1.2, 2.2, 9.0, 12.0, 0, 1.2),
            h("Uivo da Matilha", Anel { interno: 0.8, externo: 7.0 }, EmSi, 1.6, 1.8, 14.0, 7.0, 1, 0.8),
        ],
    },
    Chefe {
        kind: 11, nome: "Capitao Barba-Tormenta", corpo: Corpo::Pirata, escala: 1.8,
        zona: "ilha_inicial", nivel: 11,
        habilidades: &[
            h("Corte de Sabre", Cone { raio: 4.0, abertura: 0.75 }, AFrente, 0.9, 2.4, 6.0, 4.0, 0, 0.6),
            h("Tiro de Canhao", Circulo { raio: 3.0 }, NoAlvo, 1.6, 2.8, 8.0, 16.0, 0, 1.0),
            h("Barragem", Circulo { raio: 5.0 }, NoAlvo, 2.0, 3.2, 15.0, 14.0, 1, 1.4),
        ],
    },
    Chefe {
        kind: 12, nome: "Urso Anciao", corpo: Corpo::Bicho(1), escala: 2.0,
        zona: "ilha_inicial", nivel: 14,
        habilidades: &[
            h("Patada Larga", Cone { raio: 4.5, abertura: 0.9 }, AFrente, 1.0, 3.0, 8.0, 4.5, 0, 0.9),
            h("Pisao Sismico", Circulo { raio: 6.0 }, EmSi, 2.0, 3.0, 13.0, 6.0, 0, 1.2),
            h("Rugido Esmagador", Anel { interno: 1.0, externo: 9.0 }, EmSi, 1.5, 2.0, 16.0, 9.0, 1, 1.0),
        ],
    },
    Chefe {
        kind: 13, nome: "Tigre das Neves", corpo: Corpo::Bicho(3), escala: 2.2,
        zona: "ilha_gelo", nivel: 24,
        habilidades: &[
            h("Garras em Leque", Cone { raio: 4.0, abertura: 1.0 }, AFrente, 0.95, 2.4, 6.0, 4.0, 0, 0.7),
            h("Salto Predador", Circulo { raio: 3.5 }, NoAlvo, 1.35, 2.8, 8.0, 14.0, 0, 1.2),
            h("Rodopio", Circulo { raio: 5.0 }, EmSi, 1.7, 2.2, 12.0, 5.0, 1, 1.0),
        ],
    },
    Chefe {
        kind: 14, nome: "Lobo da Tempestade", corpo: Corpo::Bicho(7), escala: 1.0,
        zona: "ilha_gelo", nivel: 30,
        habilidades: &[
            h("Investida Trovejante", Linha { comprimento: 16.0, largura: 3.0 }, AFrente, 1.3, 3.0, 9.0, 16.0, 0, 1.5),
            h("Uivo da Tempestade", Anel { interno: 1.0, externo: 10.0 }, EmSi, 1.8, 2.6, 15.0, 10.0, 0, 1.0),
            h("Relampago Caido", Circulo { raio: 4.0 }, NoAlvo, 1.5, 3.0, 11.0, 18.0, 1, 0.8),
        ],
    },
    Chefe {
        kind: 15, nome: "Saqueador das Areias", corpo: Corpo::Gente(2), escala: 1.7,
        zona: "ilha_deserto", nivel: 36,
        habilidades: &[
            h("Linha de Tiro", Linha { comprimento: 18.0, largura: 1.6 }, AFrente, 1.0, 2.6, 8.0, 18.0, 0, 0.6),
            h("Rajada em Leque", Cone { raio: 10.0, abertura: 0.45 }, AFrente, 1.25, 2.2, 7.0, 10.0, 0, 0.5),
            h("Barril Explosivo", Circulo { raio: 4.0 }, NoAlvo, 1.6, 3.0, 10.0, 14.0, 1, 1.4),
        ],
    },
    Chefe {
        kind: 16, nome: "Arqueira do Ermo", corpo: Corpo::Gente(6), escala: 1.7,
        zona: "ilha_deserto", nivel: 41,
        habilidades: &[
            h("Flecha Perfurante", Linha { comprimento: 20.0, largura: 1.4 }, AFrente, 1.1, 2.8, 7.0, 20.0, 0, 0.6),
            h("Chuva de Flechas", Circulo { raio: 5.5 }, NoAlvo, 1.8, 2.6, 11.0, 18.0, 0, 0.3),
            h("Armadilha Espinhosa", Anel { interno: 2.0, externo: 6.0 }, EmSi, 1.4, 2.0, 13.0, 6.0, 1, 0.8),
        ],
    },
    Chefe {
        kind: 17, nome: "Owlbear Primevo", corpo: Corpo::Bicho(5), escala: 2.0,
        zona: "ilha_planalto", nivel: 52,
        habilidades: &[
            h("Patada Dupla", Cone { raio: 5.0, abertura: 0.8 }, AFrente, 1.0, 2.8, 6.0, 5.0, 0, 1.0),
            h("Giro Selvagem", Anel { interno: 2.0, externo: 6.5 }, EmSi, 1.3, 2.6, 9.0, 6.5, 0, 1.2),
            h("Queda Estrondosa", Circulo { raio: 6.2 }, NoAlvo, 2.0, 3.4, 14.0, 12.0, 1, 1.6),
        ],
    },
    Chefe {
        kind: 18, nome: "Arquimago da Tormenta", corpo: Corpo::Gente(4), escala: 1.7,
        zona: "ilha_planalto", nivel: 60,
        habilidades: &[
            h("Raio Arcano", Linha { comprimento: 22.0, largura: 2.0 }, AFrente, 1.2, 2.8, 8.0, 22.0, 0, 0.8),
            h("Meteoro", Circulo { raio: 4.5 }, NoAlvo, 1.8, 3.2, 9.0, 20.0, 0, 1.0),
            h("Nova de Gelo", Circulo { raio: 6.2 }, EmSi, 2.0, 2.6, 12.0, 6.2, 0, 1.2),
            h("Anel de Chamas", Anel { interno: 5.0, externo: 11.0 }, EmSi, 2.0, 2.4, 16.0, 11.0, 1, 0.8),
        ],
    },
];

/// Maximo de habilidades por chefe (o estado de recarga e' um array).
pub const MAX_HABILIDADES: usize = 4;

pub fn chefe(kind: u16) -> Option<&'static Chefe> {
    CHEFES.iter().find(|c| c.kind == kind)
}

pub fn e_chefe(kind: u16) -> bool {
    chefe(kind).is_some()
}

/// Os chefes de uma ilha, do mais fraco pro mais forte.
pub fn da_zona(zona: &str) -> Vec<&'static Chefe> {
    let mut v: Vec<&'static Chefe> = CHEFES.iter().filter(|c| c.zona == zona).collect();
    v.sort_by_key(|c| c.nivel);
    v
}

/// O kind do corpo preset (pro cliente escolher rig): chefe vira o mob de que
/// e' feito; o resto fica como veio.
pub fn kind_do_corpo(kind: u16) -> u16 {
    match chefe(kind).map(|c| c.corpo) {
        Some(Corpo::Bicho(k)) | Some(Corpo::Gente(k)) => k,
        _ => kind,
    }
}

/// Vida do chefe. Dimensionada na simulacao (server `balanceamento`): um
/// jogador do nivel, esquivando e com pocao, leva de 1 a 4 min conforme a
/// arma; grupo bem menos. O TETO existe porque o dano do jogador cresce com o
/// equipamento da faixa: sem ele o anel passava de 4 min no Planalto. Cabe no
/// `u16` do fio.
pub fn vida(nivel: u32) -> i32 {
    (8_000 + nivel as i32 * 240).min(18_500)
}

/// Dano do golpe COMUM (mitigado normalmente). O telegrafado nao usa isto:
/// ele tira fracao da vida (`dano_telegrafado`).
pub fn dano(nivel: u32) -> i32 {
    12 + nivel as i32 * 2
}

/// A resistencia total que `dano_mitigado` do servidor aplica: defesa a 1,5%
/// por ponto (teto 75%), reducao somada (teto 75%), total no maximo 90%.
pub fn resistencia(defesa: i32, reducao: f32) -> f32 {
    ((defesa as f32 * 0.015).clamp(0.0, 0.75) + reducao.clamp(0.0, 0.75)).min(0.90)
}

/// O que um golpe telegrafado TIRA de quem ficou dentro: fracao da vida
/// maxima, e so' `RESISTENCIA_NO_TELEGRAFICO` da resistencia vale. Quem desvia
/// nao toma nada; quem fica parado perde uma fatia fixa da barra a cada golpe,
/// qualquer que seja a build.
pub fn dano_telegrafado(h: &Habilidade, fase: u8, hp_max_alvo: i32, resistencia: f32) -> i32 {
    let bonus = if fase >= 1 { BONUS_DA_FASE_2 } else { 1.0 };
    let bruto = hp_max_alvo as f32 * h.dano_mult * VIDA_POR_MULT * bonus;
    ((bruto * (1.0 - resistencia.clamp(0.0, 0.90) * RESISTENCIA_NO_TELEGRAFICO)).round() as i32).max(1)
}

/// XP do chefe (o servidor ainda multiplica chefe por 5 na morte).
pub fn xp(nivel: u32) -> u64 {
    nivel as u64 * 40
}

/// Segundos ate' voltar depois de morto.
pub fn respawn_s(nivel: u32) -> f32 {
    600.0 + nivel as f32 * 10.0
}

/// 0 com mais de metade da vida; 1 abaixo — golpes novos, carga e recarga
/// mais curtas.
pub fn fase(hp: i32, max: i32) -> u8 {
    if hp * 2 <= max { 1 } else { 0 }
}

pub fn carga(h: &Habilidade, fase: u8) -> f32 {
    if fase >= 1 { h.carga_s * 0.85 } else { h.carga_s }
}

pub fn recarga(h: &Habilidade, fase: u8) -> f32 {
    if fase >= 1 { h.recarga_s * 0.75 } else { h.recarga_s }
}

/// Qual golpe comecar agora, se algum: pronto, liberado pela fase e com o
/// alvo no alcance. Prioridade: golpe de fase alta, depois o mais forte, e
/// no empate o primeiro da lista.
pub fn escolher(c: &Chefe, prontas_em: &[f32; MAX_HABILIDADES], agora: f32, fase: u8, dist: f32) -> Option<usize> {
    c.habilidades
        .iter()
        .enumerate()
        .filter(|(i, h)| h.fase_min <= fase && agora >= prontas_em[*i] && dist <= h.alcance)
        .max_by(|(ia, a), (ib, b)| {
            a.fase_min
                .cmp(&b.fase_min)
                .then(a.dano_mult.total_cmp(&b.dano_mult))
                .then(ib.cmp(ia))
        })
        .map(|(i, _)| i)
}

/// Centro e direcao da forma, fixados quando a carga COMECA (o alvo que se
/// mexer depois nao arrasta o golpe atras dele).
pub fn centro_e_dir(mira: Mira, chefe: glam::Vec2, alvo: glam::Vec2) -> (glam::Vec2, glam::Vec2) {
    let dir = (alvo - chefe).normalize_or(glam::Vec2::X);
    match mira {
        Mira::NoAlvo => (alvo, dir),
        Mira::EmSi | Mira::AFrente => (chefe, dir),
    }
}

/// Indices de `alvos` dentro da forma NO IMPACTO.
pub fn atingidos(forma: &Forma, centro: glam::Vec2, dir: glam::Vec2, alvos: &[glam::Vec2]) -> Vec<usize> {
    alvos.iter().enumerate().filter(|(_, p)| forma.contem(centro, dir, **p)).map(|(i, _)| i).collect()
}

/// Quanto do aviso ja' encheu (0 no inicio, 1 no impacto).
pub fn preenchimento(decorrido: f32, carga_s: f32) -> f32 {
    if carga_s <= 0.0 { 1.0 } else { (decorrido / carga_s).clamp(0.0, 1.0) }
}

/// Um chefe no mapa da ilha.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChefeNoMapa {
    pub kind: u16,
    pub nome: String,
    pub nivel: u16,
    pub centro: [f32; 2],
    pub vivo: bool,
}

#[cfg(test)]
mod testes {
    use super::*;
    use glam::Vec2;

    #[test]
    fn circulo_e_anel_contem_so_o_que_devem() {
        let c = Circulo { raio: 3.0 };
        assert!(c.contem(Vec2::ZERO, Vec2::X, Vec2::new(2.9, 0.0)));
        assert!(!c.contem(Vec2::ZERO, Vec2::X, Vec2::new(3.1, 0.0)));
        let a = Anel { interno: 2.0, externo: 5.0 };
        assert!(!a.contem(Vec2::ZERO, Vec2::X, Vec2::new(1.0, 0.0)), "colado no centro e' seguro");
        assert!(a.contem(Vec2::ZERO, Vec2::X, Vec2::new(0.0, 3.0)));
        assert!(!a.contem(Vec2::ZERO, Vec2::X, Vec2::new(6.0, 0.0)));
    }

    #[test]
    fn cone_so_pra_frente_e_linha_so_no_corredor() {
        let cone = Cone { raio: 4.0, abertura: 0.6 };
        assert!(cone.contem(Vec2::ZERO, Vec2::X, Vec2::new(3.0, 0.5)));
        assert!(!cone.contem(Vec2::ZERO, Vec2::X, Vec2::new(-3.0, 0.0)), "atras nao");
        assert!(!cone.contem(Vec2::ZERO, Vec2::X, Vec2::new(1.0, 2.0)), "de lado nao");
        let l = Linha { comprimento: 10.0, largura: 2.0 };
        assert!(l.contem(Vec2::ZERO, Vec2::Y, Vec2::new(0.9, 8.0)));
        assert!(!l.contem(Vec2::ZERO, Vec2::Y, Vec2::new(1.1, 8.0)), "fora da largura");
        assert!(!l.contem(Vec2::ZERO, Vec2::Y, Vec2::new(0.0, -0.5)), "atras da origem");
        assert!(!l.contem(Vec2::ZERO, Vec2::Y, Vec2::new(0.0, 10.5)), "alem do comprimento");
    }

    #[test]
    fn quem_sai_da_forma_antes_do_impacto_nao_toma() {
        let h = &chefe(12).unwrap().habilidades[1]; // Pisao Sismico, em volta de si
        let (centro, dir) = centro_e_dir(h.mira, Vec2::ZERO, Vec2::new(2.0, 0.0));
        // No inicio da carga os dois estavam dentro; no impacto um saiu.
        let no_impacto = [Vec2::new(2.0, 0.0), Vec2::new(8.0, 0.0)];
        assert_eq!(atingidos(&h.forma, centro, dir, &no_impacto), vec![0]);
        // Golpe no chao do alvo fica onde foi marcado.
        let tiro = &chefe(11).unwrap().habilidades[1];
        let (c2, _) = centro_e_dir(tiro.mira, Vec2::ZERO, Vec2::new(10.0, 0.0));
        assert_eq!(c2, Vec2::new(10.0, 0.0));
        assert!(atingidos(&tiro.forma, c2, dir, &[Vec2::new(14.0, 0.0)]).is_empty());
    }

    #[test]
    fn escolhe_pela_recarga_alcance_e_fase() {
        let c = chefe(10).unwrap();
        let prontas = [0.0; MAX_HABILIDADES];
        // Longe: so' a investida alcanca.
        assert_eq!(escolher(c, &prontas, 0.0, 0, 10.0), Some(1));
        // Perto na fase 0: o uivo (fase 1) nao sai; o mais forte perto e' a mordida.
        assert_eq!(escolher(c, &prontas, 0.0, 0, 3.0), Some(0));
        // Fase 1: o golpe de fase alta vem primeiro.
        assert_eq!(escolher(c, &prontas, 0.0, 1, 3.0), Some(2));
        // Em recarga nao sai.
        let mut r = prontas;
        r[1] = 5.0;
        assert_eq!(escolher(c, &r, 1.0, 0, 10.0), None);
        assert_eq!(fase(50, 100), 1);
        assert_eq!(fase(51, 100), 0);
        let h0 = &c.habilidades[0];
        assert!(recarga(h0, 1) < recarga(h0, 0) && carga(h0, 1) < carga(h0, 0));
    }

    #[test]
    fn catalogo_coerente() {
        use std::collections::HashSet;
        let mut kinds = HashSet::new();
        for c in &CHEFES {
            assert!(c.kind >= KIND_MIN && kinds.insert(c.kind), "{}: kind repetido ou baixo", c.nome);
            assert!((3..=MAX_HABILIDADES).contains(&c.habilidades.len()), "{}: 3–4 golpes", c.nome);
            assert!(c.habilidades.iter().any(|h| h.fase_min == 1), "{}: sem golpe de fase 2", c.nome);
            for h in c.habilidades {
                assert!(h.carga_s >= 0.8 && h.carga_s <= 2.0, "{}/{}: carga fora de 0,8–2 s", c.nome, h.nome);
                assert!(h.recarga_s > h.carga_s, "{}/{}", c.nome, h.nome);
            }
            let def = crate::terreno::ARQUIPELAGO.iter().find(|d| d.zona == c.zona).expect("ilha existe");
            assert!(c.nivel >= def.nivel.0 && c.nivel <= def.nivel.1 + 5, "{}: nivel fora da ilha", c.nome);
            assert!(vida(c.nivel) <= 60_000);
        }
        assert!(da_zona("ilha_inicial").len() >= 2);
        for z in ["ilha_gelo", "ilha_deserto", "ilha_planalto"] {
            assert!(!da_zona(z).is_empty(), "{z} sem chefe");
        }
    }
}
