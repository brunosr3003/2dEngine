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
            Forma::Linha {
                comprimento,
                largura,
            } => {
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
            Forma::Linha {
                comprimento,
                largura,
            } => (comprimento * comprimento + largura * largura * 0.25).sqrt(),
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
    Habilidade {
        nome,
        forma,
        mira,
        carga_s,
        dano_mult,
        recarga_s,
        alcance,
        fase_min,
        empurra,
    }
}

use Forma::{Anel, Circulo, Cone, Linha};
use Mira::{AFrente, EmSi, NoAlvo};

/// Os chefes de campo: um por mob preset de corpo inteiro. Os mais fracos no
/// Bosque (a ilha que roda localmente tem tres, pra testar), os fortes
/// subindo pelas ilhas.
/// O Colosso de um degrau da Ilha Mágica.
const fn colosso(kind: u16, degrau: usize, nivel: u32) -> Chefe {
    Chefe {
        kind,
        nome: crate::magica::NIVEIS[degrau].nome_do_chefe,
        corpo: Corpo::Gente(4),
        escala: 1.9,
        zona: crate::magica::NIVEIS[degrau].zona,
        nivel,
        habilidades: HABILIDADES_DO_COLOSSO,
    }
}

/// As habilidades do Colosso: uma lista só para os três degraus da Ilha
/// Mágica. O que muda entre eles é o NÍVEL (e por ele vida e dano); três
/// listas escritas à mão seriam três lugares pra manter em sincronia, e a
/// segunda já nasceria diferente da primeira.
const HABILIDADES_DO_COLOSSO: &[Habilidade] = &[
            // As CARGAS saem da simulação, não do gosto.
            //
            // `todo_telegrafico_e_esquivavel` mede quanto tempo o jogador
            // precisa pra sair da forma e reprova carga menor que isso — na
            // fase 2 ela ainda encurta, então a fase 1 tem de ter folga. A
            // Pancada Sísmica começou com 1,6 s e foi reprovada por 1,36 s
            // efetivos contra 1,53 s necessários.
            h(
                "Seismic Blow",
                Circulo { raio: 5.5 },
                EmSi,
                1.9,
                3.0,
                8.0,
                5.5,
                0,
                1.0,
            ),
            h(
                "Charge of the Colossus",
                Linha {
                    comprimento: 16.0,
                    largura: 2.6,
                },
                AFrente,
                1.5,
                3.0,
                7.0,
                16.0,
                0,
                0.9,
            ),
            h(
                "Stone Wave",
                Anel {
                    interno: 4.0,
                    externo: 9.5,
                },
                EmSi,
                2.0,
                3.0,
                12.0,
                9.5,
                1,
                0.9,
            ),
];

pub const CHEFES: [Chefe; 43] = [
    Chefe {
        kind: 10,
        nome: "Alpha Wolf of the Glade",
        corpo: Corpo::Bicho(0),
        escala: 2.3,
        zona: "ilha_inicial",
        nivel: 8,
        habilidades: &[
            h(
                "Rending Bite",
                Cone {
                    raio: 3.6,
                    abertura: 0.6,
                },
                AFrente,
                0.8,
                2.4,
                7.0,
                3.6,
                0,
                0.6,
            ),
            h(
                "Charge",
                Linha {
                    comprimento: 12.0,
                    largura: 2.2,
                },
                AFrente,
                1.2,
                2.2,
                9.0,
                12.0,
                0,
                1.2,
            ),
            h(
                "Howl of the Pack",
                Anel {
                    interno: 0.8,
                    externo: 7.0,
                },
                EmSi,
                1.6,
                1.8,
                14.0,
                7.0,
                1,
                0.8,
            ),
        ],
    },
    Chefe {
        kind: 11,
        nome: "Captain Stormbeard",
        corpo: Corpo::Pirata,
        escala: 1.8,
        zona: "ilha_inicial",
        nivel: 11,
        habilidades: &[
            h(
                "Sabre Slash",
                Cone {
                    raio: 4.0,
                    abertura: 0.75,
                },
                AFrente,
                0.9,
                2.4,
                6.0,
                4.0,
                0,
                0.6,
            ),
            h(
                "Cannon Shot",
                Circulo { raio: 3.0 },
                NoAlvo,
                1.6,
                2.8,
                8.0,
                16.0,
                0,
                1.0,
            ),
            h(
                "Barrage",
                Circulo { raio: 5.0 },
                NoAlvo,
                2.0,
                3.2,
                15.0,
                14.0,
                1,
                1.4,
            ),
        ],
    },
    Chefe {
        kind: 12,
        nome: "Elder Bear",
        corpo: Corpo::Bicho(1),
        escala: 2.0,
        zona: "ilha_inicial",
        nivel: 14,
        habilidades: &[
            h(
                "Wide Swipe",
                Cone {
                    raio: 4.5,
                    abertura: 0.9,
                },
                AFrente,
                1.0,
                3.0,
                8.0,
                4.5,
                0,
                0.9,
            ),
            h(
                "Seismic Stomp",
                Circulo { raio: 6.0 },
                EmSi,
                2.0,
                3.0,
                13.0,
                6.0,
                0,
                1.2,
            ),
            h(
                "Crushing Roar",
                Anel {
                    interno: 1.0,
                    externo: 9.0,
                },
                EmSi,
                1.5,
                2.0,
                16.0,
                9.0,
                1,
                1.0,
            ),
        ],
    },
    Chefe {
        kind: 13,
        nome: "Snow Tiger",
        corpo: Corpo::Bicho(3),
        escala: 2.2,
        zona: "ilha_gelo",
        nivel: 24,
        habilidades: &[
            h(
                "Fanning Claws",
                Cone {
                    raio: 4.0,
                    abertura: 1.0,
                },
                AFrente,
                0.95,
                2.4,
                6.0,
                4.0,
                0,
                0.7,
            ),
            h(
                "Predator Leap",
                Circulo { raio: 3.5 },
                NoAlvo,
                1.35,
                2.8,
                8.0,
                14.0,
                0,
                1.2,
            ),
            h(
                "Whirl",
                Circulo { raio: 5.0 },
                EmSi,
                1.7,
                2.2,
                12.0,
                5.0,
                1,
                1.0,
            ),
        ],
    },
    Chefe {
        kind: 14,
        nome: "Storm Wolf",
        corpo: Corpo::Bicho(7),
        escala: 1.0,
        zona: "ilha_gelo",
        nivel: 30,
        habilidades: &[
            h(
                "Thundering Charge",
                Linha {
                    comprimento: 16.0,
                    largura: 3.0,
                },
                AFrente,
                1.3,
                3.0,
                9.0,
                16.0,
                0,
                1.5,
            ),
            h(
                "Howl of the Storm",
                Anel {
                    interno: 1.0,
                    externo: 10.0,
                },
                EmSi,
                1.8,
                2.6,
                15.0,
                10.0,
                0,
                1.0,
            ),
            h(
                "Fallen Lightning",
                Circulo { raio: 4.0 },
                NoAlvo,
                1.5,
                3.0,
                11.0,
                18.0,
                1,
                0.8,
            ),
        ],
    },
    Chefe {
        kind: 15,
        nome: "Sand Raider",
        corpo: Corpo::Gente(2),
        escala: 1.7,
        zona: "ilha_deserto",
        nivel: 36,
        habilidades: &[
            h(
                "Firing Line",
                Linha {
                    comprimento: 18.0,
                    largura: 1.6,
                },
                AFrente,
                1.0,
                2.6,
                8.0,
                18.0,
                0,
                0.6,
            ),
            h(
                "Fanning Volley",
                Cone {
                    raio: 10.0,
                    abertura: 0.45,
                },
                AFrente,
                1.25,
                2.2,
                7.0,
                10.0,
                0,
                0.5,
            ),
            h(
                "Explosive Barrel",
                Circulo { raio: 4.0 },
                NoAlvo,
                1.6,
                3.0,
                10.0,
                14.0,
                1,
                1.4,
            ),
        ],
    },
    Chefe {
        kind: 16,
        nome: "Archer of the Waste",
        corpo: Corpo::Gente(6),
        escala: 1.7,
        zona: "ilha_deserto",
        nivel: 41,
        habilidades: &[
            h(
                "Piercing Arrow",
                Linha {
                    comprimento: 20.0,
                    largura: 1.4,
                },
                AFrente,
                1.1,
                3.2,
                7.0,
                20.0,
                0,
                0.6,
            ),
            h(
                "Arrow Rain",
                Circulo { raio: 5.5 },
                NoAlvo,
                1.8,
                2.6,
                11.0,
                18.0,
                0,
                0.3,
            ),
            h(
                "Thorn Trap",
                Anel {
                    interno: 2.0,
                    externo: 6.0,
                },
                EmSi,
                1.4,
                2.0,
                13.0,
                6.0,
                1,
                0.8,
            ),
        ],
    },
    Chefe {
        kind: 17,
        nome: "Primeval Owlbear",
        corpo: Corpo::Bicho(5),
        escala: 2.0,
        zona: "ilha_planalto",
        nivel: 52,
        habilidades: &[
            h(
                "Double Swipe",
                Cone {
                    raio: 5.0,
                    abertura: 0.8,
                },
                AFrente,
                1.0,
                2.8,
                6.0,
                5.0,
                0,
                1.0,
            ),
            h(
                "Feral Spin",
                Anel {
                    interno: 2.0,
                    externo: 6.5,
                },
                EmSi,
                1.3,
                2.6,
                9.0,
                6.5,
                0,
                1.2,
            ),
            h(
                "Thunderous Fall",
                Circulo { raio: 6.2 },
                NoAlvo,
                2.0,
                3.4,
                14.0,
                12.0,
                1,
                1.6,
            ),
        ],
    },
    Chefe {
        kind: 18,
        nome: "Archmage of the Tempest",
        corpo: Corpo::Gente(4),
        escala: 1.7,
        zona: "ilha_planalto",
        nivel: 60,
        habilidades: &[
            h(
                "Arcane Bolt",
                Linha {
                    comprimento: 22.0,
                    largura: 2.0,
                },
                AFrente,
                1.2,
                2.8,
                8.0,
                22.0,
                0,
                0.8,
            ),
            h(
                "Meteor",
                Circulo { raio: 4.5 },
                NoAlvo,
                1.8,
                3.2,
                9.0,
                20.0,
                0,
                1.0,
            ),
            h(
                "Ice Nova",
                Circulo { raio: 6.2 },
                EmSi,
                2.0,
                2.6,
                12.0,
                6.2,
                0,
                1.2,
            ),
            h(
                "Ring of Flames",
                Anel {
                    interno: 5.0,
                    externo: 11.0,
                },
                EmSi,
                2.0,
                2.4,
                16.0,
                11.0,
                1,
                0.8,
            ),
        ],
    },
    // O COLOSSO DA ILHA MÁGICA (`shared::magica`).
    //
    // Ele existe porque a Ilhota do Colosso paga bônus de DROP DE CHEFE, e um
    // bônus sobre um chefe que não existe é uma promessa vazia — a ilhota
    // mais cobiçada do desenho ficaria sendo a mais inútil.
    //
    // Nível 45: a ilha é aberta a todo nível (o passe é o requisito), então
    // ele fica no meio da faixa alta em vez de no teto. Um chefe de 60 numa
    // ilha que qualquer um entra seria um muro; um de 10, um boneco.
    Chefe {
        kind: 19,
        nome: "Colossus of the Magic Island",
        corpo: Corpo::Gente(4),
        escala: 1.9,
        zona: crate::magica::ZONA,
        nivel: 28,
        habilidades: HABILIDADES_DO_COLOSSO,
    },
    // Os degraus II e III da Ilha Mágica (`magica::NIVEIS`).
    //
    // Antes havia UM Colosso, de nível 45, justificado por "a ilha é aberta a
    // todo nível". Ela deixou de ser: cada degrau tem faixa de mob estreita e
    // portão de entrada, e um chefe de 45 no degrau I (mobs 15-18) seria um
    // muro que ninguém passa.
    //
    // Cada um fica pouco acima do topo da faixa do seu degrau — chefe é
    // chefe — e `catalogo_coerente` confere que nenhum saiu da ilha dele.
    // O nível do chefe acompanha o DEGRAU, não a faixa de mob: quem entra no
    // degrau II tem nível 30, e um chefe de 21 é um boneco — a simulação
    // reprovou com "parado com poção venceu com HP mínimo 27%". Alguns
    // níveis ACIMA do topo da faixa, que é o que separa chefe de mob.
    colosso(20, 1, 36),
    colosso(21, 2, 51),
    // Tiers IV-VI (levels 60-80). Kinds past the mob kinds (30-46).
    colosso(47, 3, 66),
    colosso(48, 4, 76),
    colosso(49, 5, 80),
    // ── The Plateau's 50-60 half (02/10/2026) ──
    // Between the Owlbear (52) and the Archmage (60) there was no boss at
    // all: a field boss on the Vale of Thunder's island, and the Tempest
    // Spire's dungeon boss (`SO_DE_DUNGEON`).
    Chefe {
        kind: 22,
        nome: "Thunderhide Behemoth",
        corpo: Corpo::Bicho(38),
        escala: 2.3,
        zona: "ilha_planalto",
        nivel: 56,
        habilidades: &[
            h("Crushing Paw", Cone { raio: 6.5, abertura: 0.9 }, AFrente, 1.3, 2.6, 6.0, 6.5, 0, 1.2),
            h("Static Burst", Circulo { raio: 4.5 }, NoAlvo, 1.8, 3.0, 9.0, 18.0, 0, 1.2),
            h("Thunder Charge", Linha { comprimento: 20.0, largura: 3.0 }, AFrente, 1.3, 2.4, 7.0, 20.0, 0, 1.2),
            h("Skyfall", Circulo { raio: 5.6 }, NoAlvo, 2.0, 3.0, 12.0, 14.0, 1, 1.4),
        ],
    },
    Chefe {
        kind: 23,
        nome: "Herald of the Tempest",
        corpo: Corpo::Gente(39),
        escala: 1.8,
        zona: "ilha_planalto",
        nivel: 57,
        habilidades: &[
            h("Chain Lightning", Linha { comprimento: 22.0, largura: 2.2 }, AFrente, 1.2, 2.8, 7.0, 22.0, 0, 0.8),
            h("Storm Ring", Anel { interno: 2.5, externo: 7.5 }, EmSi, 1.4, 2.6, 9.5, 7.5, 0, 1.4),
            h("Thunderclap", Circulo { raio: 5.5 }, NoAlvo, 1.9, 3.4, 12.5, 18.0, 0, 1.2),
            h("Eye of the Storm", Anel { interno: 5.0, externo: 11.0 }, EmSi, 2.0, 2.4, 16.0, 11.0, 1, 0.8),
        ],
    },
    // ── Skyreach's dungeon bosses (`SO_DE_DUNGEON`) ──
    // The Seraph Reliquary: a winged owlbear of white and gold that guards
    // the relics with its whole weight. (On the bear's body the pistol fight
    // ended in 58 s: the bear preset is softer than the owlbear's.)
    Chefe {
        kind: 25,
        nome: "Reliquary Warden",
        corpo: Corpo::Bicho(45),
        escala: 2.0,
        zona: "ilha_celeste",
        nivel: 65,
        habilidades: &[
            h("Hallowed Slam", Cone { raio: 7.0, abertura: 0.9 }, AFrente, 1.3, 2.4, 5.0, 7.0, 0, 1.0),
            h("Wing Gust", Anel { interno: 2.0, externo: 8.5 }, EmSi, 1.5, 2.1, 7.5, 8.5, 0, 1.6),
            h("Radiant Wingbeat", Linha { comprimento: 16.0, largura: 3.0 }, AFrente, 1.3, 2.4, 8.0, 16.0, 0, 1.2),
            h("Falling Relic", Circulo { raio: 5.6 }, NoAlvo, 2.0, 3.0, 12.0, 14.0, 1, 1.4),
        ],
    },
    // The Cathedral of Clouds: a seraph who turned on the city.
    Chefe {
        kind: 26,
        nome: "Fallen Seraph",
        corpo: Corpo::Gente(44),
        escala: 1.7,
        zona: "ilha_celeste",
        nivel: 70,
        habilidades: &[
            h("Radiant Lance", Linha { comprimento: 22.0, largura: 2.2 }, AFrente, 1.2, 2.8, 7.0, 22.0, 0, 0.8),
            h("Halo Burst", Anel { interno: 2.5, externo: 7.5 }, EmSi, 1.4, 2.6, 9.5, 7.5, 0, 1.4),
            h("Judgement", Circulo { raio: 5.5 }, NoAlvo, 1.9, 3.4, 12.5, 18.0, 0, 1.2),
            h("Wings of Ash", Anel { interno: 5.0, externo: 11.0 }, EmSi, 2.0, 2.4, 16.0, 11.0, 1, 0.8),
        ],
    },
    // The Pegasus Aerie: the herd's storm-born leader. The server moves it
    // like the lynx (a quadruped that charges); the client draws the
    // pegasus stag (`client::bicho::do_mob`).
    Chefe {
        kind: 27,
        nome: "Storm Pegasus",
        corpo: Corpo::Bicho(41),
        escala: 1.6,
        zona: "ilha_celeste",
        nivel: 75,
        habilidades: &[
            h("Thunder Charge", Linha { comprimento: 18.0, largura: 3.0 }, AFrente, 1.2, 3.0, 7.0, 18.0, 0, 1.6),
            h("Hoof Storm", Circulo { raio: 4.5 }, EmSi, 1.8, 2.6, 7.0, 4.5, 0, 1.2),
            h("Sky Strike", Circulo { raio: 6.0 }, NoAlvo, 2.0, 3.4, 13.0, 16.0, 1, 1.4),
        ],
    },
    // ── Skyreach's field bosses ──
    // The Storm Gardens: a winged bear grown huge on lightning-fed flowers.
    Chefe {
        kind: 28,
        nome: "Gilded Colossus",
        corpo: Corpo::Bicho(43),
        escala: 2.4,
        zona: "ilha_celeste",
        nivel: 72,
        habilidades: &[
            h("Crushing Paw", Cone { raio: 6.5, abertura: 0.9 }, AFrente, 1.3, 2.6, 6.0, 6.5, 0, 1.2),
            h("Gilded Roar", Anel { interno: 2.0, externo: 8.0 }, EmSi, 1.5, 2.2, 8.0, 8.0, 0, 1.6),
            h("Wing Slam", Linha { comprimento: 16.0, largura: 3.0 }, AFrente, 1.3, 2.4, 8.0, 16.0, 0, 1.2),
            h("Thunderbloom", Circulo { raio: 5.6 }, NoAlvo, 2.0, 3.0, 12.0, 14.0, 1, 1.4),
        ],
    },
    // The Throne of the Sky: the seraph who still claims the city.
    Chefe {
        kind: 29,
        nome: "Seraph Archon",
        corpo: Corpo::Gente(44),
        escala: 1.9,
        zona: "ilha_celeste",
        nivel: 80,
        habilidades: &[
            h("Spear of Dawn", Linha { comprimento: 24.0, largura: 2.2 }, AFrente, 1.2, 2.8, 7.0, 24.0, 0, 0.8),
            h("Choir of Light", Anel { interno: 2.5, externo: 8.0 }, EmSi, 1.4, 2.6, 9.0, 8.0, 0, 1.4),
            h("Verdict", Circulo { raio: 5.5 }, NoAlvo, 1.9, 3.4, 12.0, 18.0, 0, 1.2),
            h("Wrath of the Sky", Anel { interno: 5.0, externo: 12.0 }, EmSi, 2.0, 2.4, 16.0, 12.0, 1, 0.8),
        ],
    },
    // ── Kōgen-tō's field bosses: the giant robots, one per arena plaza
    // (`kogen::ARENAS`), weakest first. ──
    // Shibuya: a construction mech gone feral, an Iron Bear three times over.
    Chefe {
        kind: 63,
        nome: "Titan Mk-I",
        corpo: Corpo::Bicho(60),
        escala: 2.8,
        zona: "ilha_kogen",
        nivel: 85,
        habilidades: &[
            h("Hydraulic Paw", Cone { raio: 6.5, abertura: 0.9 }, AFrente, 1.3, 2.6, 6.0, 6.5, 0, 1.2),
            h("Shockwave", Anel { interno: 2.0, externo: 8.0 }, EmSi, 1.5, 2.2, 8.0, 8.0, 0, 1.6),
            h("Rocket Charge", Linha { comprimento: 16.0, largura: 3.0 }, AFrente, 1.3, 2.4, 8.0, 16.0, 0, 1.2),
            h("Orbital Strike", Circulo { raio: 5.6 }, NoAlvo, 2.0, 3.0, 12.0, 14.0, 1, 1.4),
        ],
    },
    // Kabukicho: under the neon, the kaiju the signs were advertising.
    Chefe {
        kind: 64,
        nome: "Neon Kaiju",
        corpo: Corpo::Bicho(62),
        escala: 3.0,
        zona: "ilha_kogen",
        nivel: 92,
        habilidades: &[
            h("Tail Sweep", Cone { raio: 7.0, abertura: 1.1 }, AFrente, 1.4, 2.6, 8.5, 7.0, 0, 1.4),
            h("Neon Breath", Linha { comprimento: 22.0, largura: 2.6 }, AFrente, 1.3, 2.8, 10.0, 22.0, 0, 0.8),
            h("Ground Pound", Circulo { raio: 5.5 }, NoAlvo, 1.9, 3.2, 14.0, 16.0, 0, 1.2),
            h("Kaiju Roar", Anel { interno: 5.0, externo: 12.0 }, EmSi, 2.0, 2.4, 19.0, 12.0, 1, 0.8),
        ],
    },
    // The Tocho: the city's own overseer, a Tesla Unit grown to a tower's
    // guardian. The top of the island.
    Chefe {
        kind: 65,
        nome: "Overseer Unit",
        corpo: Corpo::Gente(61),
        escala: 2.2,
        zona: "ilha_kogen",
        nivel: 100,
        habilidades: &[
            h("Rail Beam", Linha { comprimento: 24.0, largura: 2.2 }, AFrente, 1.2, 2.8, 9.5, 24.0, 0, 0.8),
            h("Tesla Coil", Anel { interno: 2.5, externo: 8.0 }, EmSi, 1.4, 2.6, 12.0, 8.0, 0, 1.4),
            h("Drone Strike", Circulo { raio: 5.5 }, NoAlvo, 1.9, 3.4, 15.0, 18.0, 0, 1.2),
            h("System Purge", Anel { interno: 5.0, externo: 12.0 }, EmSi, 2.0, 2.4, 20.0, 12.0, 1, 0.8),
        ],
    },
    // ── Kōgen-tō's dungeon bosses (`SO_DE_DUNGEON`) ──
    Chefe {
        kind: 66,
        nome: "Foundry Mainframe",
        corpo: Corpo::Gente(57),
        escala: 2.0,
        zona: "ilha_kogen",
        nivel: 88,
        habilidades: &[
            h("Rivet Volley", Circulo { raio: 4.5 }, NoAlvo, 1.8, 2.8, 9.5, 18.0, 0, 1.0),
            h("Press Slam", Anel { interno: 2.5, externo: 8.0 }, EmSi, 1.4, 2.6, 12.0, 8.0, 0, 1.4),
            h("Conveyor Rush", Linha { comprimento: 20.0, largura: 3.0 }, AFrente, 1.3, 2.6, 10.0, 20.0, 0, 1.4),
            h("Meltdown", Anel { interno: 5.0, externo: 12.0 }, EmSi, 2.0, 2.4, 19.0, 12.0, 1, 0.8),
        ],
    },
    Chefe {
        kind: 67,
        nome: "Subway Leviathan",
        corpo: Corpo::Bicho(58),
        escala: 2.6,
        zona: "ilha_kogen",
        nivel: 93,
        habilidades: &[
            h("Third Rail", Linha { comprimento: 22.0, largura: 3.0 }, AFrente, 1.3, 2.8, 10.0, 22.0, 0, 1.4),
            h("Coil Snap", Cone { raio: 7.0, abertura: 1.0 }, AFrente, 1.4, 2.6, 8.5, 7.0, 0, 1.2),
            h("Tunnel Collapse", Circulo { raio: 5.5 }, NoAlvo, 1.9, 3.2, 14.0, 16.0, 0, 1.2),
            h("Blackout", Anel { interno: 5.0, externo: 12.0 }, EmSi, 2.0, 2.4, 19.0, 12.0, 1, 0.8),
        ],
    },
    Chefe {
        kind: 68,
        nome: "Tower Sovereign",
        corpo: Corpo::Gente(61),
        escala: 2.3,
        zona: "ilha_kogen",
        nivel: 99,
        habilidades: &[
            h("Skyline Beam", Linha { comprimento: 24.0, largura: 2.2 }, AFrente, 1.2, 2.8, 9.5, 24.0, 0, 0.8),
            h("Lightning Rod", Anel { interno: 2.5, externo: 8.0 }, EmSi, 1.4, 2.6, 12.0, 8.0, 0, 1.4),
            h("Helipad Strike", Circulo { raio: 5.5 }, NoAlvo, 1.9, 3.4, 15.0, 18.0, 0, 1.2),
            h("Rooftop Storm", Anel { interno: 5.0, externo: 12.0 }, EmSi, 2.0, 2.4, 20.0, 12.0, 1, 0.8),
        ],
    },
    // THE NEON SPIRE's boss, on its roof (`kogen::topo_da_espiral`): a war
    // machine built as a dragon. After the Overseer in the list: both are
    // 100, and `da_zona`'s sort keeps the order, so the arenas line up and
    // the dragon takes the roof.
    Chefe {
        kind: DRAGAO_DA_ESPIRAL,
        nome: "Sky Dragon Mech",
        corpo: Corpo::Bicho(60),
        escala: 2.6,
        zona: "ilha_kogen",
        nivel: 100,
        habilidades: &[
            h("Tail Sweep", Cone { raio: 7.5, abertura: 1.2 }, AFrente, 1.4, 2.6, 9.0, 7.5, 0, 1.4),
            h("Neon Fire Breath", Linha { comprimento: 22.0, largura: 3.0 }, AFrente, 1.3, 2.8, 10.0, 22.0, 0, 0.8),
            h("Wing Gust", Anel { interno: 3.0, externo: 10.0 }, EmSi, 1.5, 2.6, 12.0, 10.0, 0, 1.4),
            h("Dive Bomb", Circulo { raio: 6.0 }, NoAlvo, 2.0, 3.4, 15.0, 18.0, 1, 1.2),
        ],
    },
    // ── Abyssia's field bosses (`abissal::arenas`), weakest first ──
    // The Kelp Forest's edge: a sea predator as long as a street.
    Chefe {
        kind: REI_DO_MAR,
        nome: "Sea King",
        corpo: Corpo::Bicho(70),
        escala: 3.0,
        zona: "ilha_abissal",
        nivel: 105,
        habilidades: &[
            h("Tail Crash", Cone { raio: 7.5, abertura: 1.1 }, AFrente, 1.4, 2.6, 9.0, 7.5, 0, 1.4),
            h("Tidal Breath", Linha { comprimento: 22.0, largura: 3.0 }, AFrente, 1.3, 2.8, 10.0, 22.0, 0, 0.8),
            h("Whirlpool", Anel { interno: 3.0, externo: 10.0 }, EmSi, 1.5, 2.6, 12.0, 10.0, 0, 1.4),
            h("Breach", Circulo { raio: 6.0 }, NoAlvo, 2.0, 3.4, 15.0, 18.0, 1, 1.2),
        ],
    },
    // The Lantern Trench: the Kraken breaks the floor around its den.
    Chefe {
        kind: CARANGUEJO_TITA,
        nome: "Kraken",
        corpo: Corpo::Bicho(76),
        escala: 3.2,
        zona: "ilha_abissal",
        nivel: 112,
        habilidades: &[
            h("Pincer Sweep", Cone { raio: 7.0, abertura: 1.3 }, AFrente, 1.4, 2.6, 8.5, 7.0, 0, 1.4),
            h("Shell Slam", Anel { interno: 2.5, externo: 8.5 }, EmSi, 1.5, 2.6, 12.0, 8.5, 0, 1.4),
            h("Bubble Volley", Circulo { raio: 5.5 }, NoAlvo, 1.9, 3.2, 14.0, 18.0, 0, 1.2),
            h("Trench Quake", Anel { interno: 5.0, externo: 12.0 }, EmSi, 2.0, 2.4, 20.0, 12.0, 1, 0.8),
        ],
    },
    // The Abyss Rim: the captain of a ghost ship that never stopped sailing.
    Chefe {
        kind: 79,
        nome: "Ghost Pirate Captain",
        corpo: Corpo::Pirata,
        escala: 2.2,
        zona: "ilha_abissal",
        nivel: 116,
        habilidades: &[
            h("Cutlass Arc", Cone { raio: 6.5, abertura: 1.2 }, AFrente, 1.3, 2.4, 8.0, 6.5, 0, 1.2),
            h("Broadside", Linha { comprimento: 24.0, largura: 3.0 }, AFrente, 1.3, 2.8, 10.0, 24.0, 0, 0.8),
            h("Anchor Drop", Circulo { raio: 5.5 }, NoAlvo, 1.9, 3.4, 15.0, 18.0, 0, 1.2),
            h("Drowned Crew", Anel { interno: 5.0, externo: 12.0 }, EmSi, 2.0, 2.4, 20.0, 12.0, 1, 0.8),
        ],
    },
    // The top of the island: the leviathan that keeps the abyss.
    Chefe {
        kind: LEVIATA,
        nome: "Abyssal Leviathan",
        corpo: Corpo::Bicho(72),
        escala: 3.3,
        zona: "ilha_abissal",
        nivel: 120,
        habilidades: &[
            h("Maw", Cone { raio: 8.0, abertura: 1.0 }, AFrente, 1.4, 2.6, 9.5, 8.0, 0, 1.4),
            h("Abyssal Beam", Linha { comprimento: 26.0, largura: 3.0 }, AFrente, 1.3, 2.8, 10.0, 26.0, 0, 0.8),
            h("Pressure Wave", Anel { interno: 3.0, externo: 11.0 }, EmSi, 1.5, 2.6, 12.0, 11.0, 0, 1.4),
            h("Deep Strike", Circulo { raio: 6.5 }, NoAlvo, 2.0, 3.4, 15.0, 18.0, 1, 1.2),
        ],
    },
    // ── Abyssia's dungeon bosses (`SO_DE_DUNGEON`) ──
    Chefe {
        kind: 81,
        nome: "The Drowned Admiral",
        corpo: Corpo::Pirata,
        escala: 2.0,
        zona: "ilha_abissal",
        nivel: 102,
        habilidades: &[
            h("Grapeshot", Circulo { raio: 4.5 }, NoAlvo, 1.8, 2.8, 9.5, 18.0, 0, 1.0),
            h("Keelhaul", Linha { comprimento: 20.0, largura: 3.0 }, AFrente, 1.3, 2.6, 10.0, 20.0, 0, 1.4),
            h("Sabre Storm", Anel { interno: 2.5, externo: 8.0 }, EmSi, 1.4, 2.6, 12.0, 8.0, 0, 1.4),
            h("Sinking Ship", Anel { interno: 5.0, externo: 12.0 }, EmSi, 2.0, 2.4, 19.0, 12.0, 1, 0.8),
        ],
    },
    Chefe {
        kind: 82,
        nome: "Royal Guard Captain",
        corpo: Corpo::Gente(73),
        escala: 2.2,
        zona: "ilha_abissal",
        nivel: 110,
        habilidades: &[
            h("Trident Thrust", Linha { comprimento: 18.0, largura: 2.4 }, AFrente, 1.2, 2.6, 9.5, 18.0, 0, 0.8),
            h("Coral Wall", Anel { interno: 2.5, externo: 8.0 }, EmSi, 1.4, 2.6, 12.0, 8.0, 0, 1.4),
            h("Harpoon Rain", Circulo { raio: 5.5 }, NoAlvo, 1.9, 3.4, 15.0, 18.0, 0, 1.2),
            h("Royal Decree", Anel { interno: 5.0, externo: 12.0 }, EmSi, 2.0, 2.4, 20.0, 12.0, 1, 0.8),
        ],
    },
    Chefe {
        kind: SER_DAS_PROFUNDEZAS,
        nome: "The Deep One",
        corpo: Corpo::Bicho(76),
        escala: 3.0,
        zona: "ilha_abissal",
        nivel: 118,
        habilidades: &[
            h("Ink Cloud", Circulo { raio: 6.0 }, NoAlvo, 1.9, 3.2, 14.0, 18.0, 0, 1.2),
            h("Tentacle Lash", Cone { raio: 7.5, abertura: 1.2 }, AFrente, 1.4, 2.6, 9.0, 7.5, 0, 1.4),
            h("Abyss Gaze", Linha { comprimento: 24.0, largura: 2.6 }, AFrente, 1.3, 2.8, 10.0, 24.0, 0, 0.8),
            h("The Dark Below", Anel { interno: 5.0, externo: 12.0 }, EmSi, 2.0, 2.4, 20.0, 12.0, 1, 0.8),
        ],
    },
    // STORMKEEP's Warlord (`forte`), in the keep's arena. Last in the list:
    // `da_zona` sorts by level, and at 60 he must come after the Archmage
    // so the Plateau's boss spots line up ([2, 3, 4] then the keep).
    Chefe {
        kind: 24,
        nome: "Warlord of Stormkeep",
        corpo: Corpo::Gente(33),
        escala: 2.0,
        zona: "ilha_planalto",
        nivel: 60,
        habilidades: &[
            h("Thrown Axe", Circulo { raio: 4.2 }, NoAlvo, 1.8, 2.8, 8.0, 18.0, 0, 1.0),
            h("War Banner", Circulo { raio: 4.8 }, NoAlvo, 1.8, 3.0, 9.0, 18.0, 0, 1.2),
            h("Shield Charge", Linha { comprimento: 20.0, largura: 3.0 }, AFrente, 1.3, 2.6, 8.0, 20.0, 0, 1.4),
            h("Storm of Blades", Anel { interno: 5.0, externo: 11.0 }, EmSi, 2.0, 2.4, 16.0, 11.0, 1, 0.8),
        ],
    },
    // ── The Magic Island's dungeon bosses (`SO_DE_DUNGEON`) ──
    Chefe {
        kind: 50,
        nome: "Arcane Warden",
        corpo: Corpo::Gente(31),
        escala: 1.8,
        zona: "ilha_magica",
        nivel: 23,
        habilidades: &[
            h("Arcane Lance", Linha { comprimento: 22.0, largura: 2.2 }, AFrente, 1.2, 2.8, 7.0, 22.0, 0, 0.8),
            h("Rune Ring", Anel { interno: 2.5, externo: 7.5 }, EmSi, 1.4, 2.6, 9.5, 7.5, 0, 1.4),
            h("Glyph Burst", Circulo { raio: 5.5 }, NoAlvo, 1.9, 3.4, 12.5, 18.0, 0, 1.2),
            h("Unbound Sigil", Anel { interno: 5.0, externo: 11.0 }, EmSi, 2.0, 2.4, 16.0, 11.0, 1, 0.8),
        ],
    },
    Chefe {
        kind: 51,
        nome: "Keeper of Tomes",
        corpo: Corpo::Gente(35),
        escala: 1.8,
        zona: "ilha_magica_2",
        nivel: 33,
        habilidades: &[
            h("Ink Lash", Linha { comprimento: 22.0, largura: 2.2 }, AFrente, 1.2, 2.8, 7.0, 22.0, 0, 0.8),
            h("Turning Pages", Anel { interno: 2.5, externo: 7.5 }, EmSi, 1.4, 2.6, 9.5, 7.5, 0, 1.4),
            h("Falling Shelf", Circulo { raio: 5.5 }, NoAlvo, 1.9, 3.4, 12.5, 18.0, 0, 1.2),
            h("The Last Chapter", Anel { interno: 5.0, externo: 11.0 }, EmSi, 2.0, 2.4, 16.0, 11.0, 1, 0.8),
        ],
    },
    Chefe {
        kind: 52,
        nome: "The Mirrorbound",
        corpo: Corpo::Gente(39),
        escala: 1.8,
        zona: "ilha_magica_3",
        nivel: 48,
        habilidades: &[
            h("Glass Ray", Linha { comprimento: 22.0, largura: 2.2 }, AFrente, 1.2, 2.8, 7.0, 22.0, 0, 0.8),
            h("Shatter Ring", Anel { interno: 2.5, externo: 7.5 }, EmSi, 1.4, 2.6, 9.5, 7.5, 0, 1.4),
            h("Mirror Shard", Circulo { raio: 5.5 }, NoAlvo, 1.9, 3.4, 12.5, 18.0, 0, 1.2),
            h("Hall of Reflections", Anel { interno: 5.0, externo: 11.0 }, EmSi, 2.0, 2.4, 16.0, 11.0, 1, 0.8),
        ],
    },
    Chefe {
        kind: 53,
        nome: "Echo Sovereign",
        corpo: Corpo::Gente(44),
        escala: 1.9,
        zona: "ilha_magica_4",
        nivel: 63,
        habilidades: &[
            h("Resonant Spear", Linha { comprimento: 22.0, largura: 2.2 }, AFrente, 1.2, 2.8, 7.0, 22.0, 0, 0.8),
            h("Echo Ring", Anel { interno: 2.5, externo: 7.5 }, EmSi, 1.4, 2.6, 9.5, 7.5, 0, 1.4),
            h("Thunder Chime", Circulo { raio: 5.5 }, NoAlvo, 1.9, 3.4, 12.5, 18.0, 0, 1.2),
            h("Choir of Echoes", Anel { interno: 5.0, externo: 11.0 }, EmSi, 2.0, 2.4, 16.0, 11.0, 1, 0.8),
        ],
    },
    Chefe {
        kind: 54,
        nome: "Forgeheart Titan",
        corpo: Corpo::Bicho(43),
        escala: 2.4,
        zona: "ilha_magica_5",
        nivel: 73,
        habilidades: &[
            h("Anvil Swipe", Cone { raio: 6.5, abertura: 0.9 }, AFrente, 1.3, 2.6, 6.0, 6.5, 0, 1.2),
            h("Molten Drop", Circulo { raio: 4.5 }, NoAlvo, 1.8, 3.0, 9.0, 18.0, 0, 1.2),
            h("Furnace Charge", Linha { comprimento: 20.0, largura: 3.0 }, AFrente, 1.3, 2.4, 7.0, 20.0, 0, 1.2),
            h("Starfall Hammer", Circulo { raio: 5.6 }, NoAlvo, 2.0, 3.0, 12.0, 14.0, 1, 1.4),
        ],
    },
    Chefe {
        kind: 55,
        nome: "Eternal Sovereign",
        corpo: Corpo::Gente(4),
        escala: 2.0,
        zona: "ilha_magica_6",
        nivel: 80,
        habilidades: &[
            h("Lance of Ages", Linha { comprimento: 22.0, largura: 2.2 }, AFrente, 1.2, 2.8, 7.0, 22.0, 0, 0.8),
            h("Halo of Time", Anel { interno: 2.5, externo: 7.5 }, EmSi, 1.4, 2.6, 9.5, 7.5, 0, 1.4),
            h("Verdict of Eternity", Circulo { raio: 5.5 }, NoAlvo, 1.9, 3.4, 12.5, 18.0, 0, 1.2),
            h("The Endless Night", Anel { interno: 5.0, externo: 11.0 }, EmSi, 2.0, 2.4, 16.0, 11.0, 1, 0.8),
        ],
    },
];

// (Skyreach's field bosses are right below the dungeon ones.)

/// Bosses that only appear at the end of a dungeon, never in the field:
/// `da_zona` leaves them out, or their islands would grow more field bosses.
pub const SO_DE_DUNGEON: &[u16] = &[23, 25, 26, 27, 50, 51, 52, 53, 54, 55, 66, 67, 68, 81, 82, 83];

/// The Storm Pegasus: its body preset is the lynx, its model the pegasus stag.
pub const PEGASUS: u16 = 27;

/// The Sky Dragon Mech: its body preset is the Iron Bear (melee), its model
/// the mech dragon.
pub const DRAGAO_DA_ESPIRAL: u16 = 69;

/// Abyssia's beast bosses: they fight as the Iron Bear and look like
/// themselves (`client::bicho`).
pub const REI_DO_MAR: u16 = 77;
pub const CARANGUEJO_TITA: u16 = 78;
pub const LEVIATA: u16 = 80;
pub const SER_DAS_PROFUNDEZAS: u16 = 83;

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
    let mut v: Vec<&'static Chefe> = CHEFES
        .iter()
        .filter(|c| c.zona == zona && !SO_DE_DUNGEON.contains(&c.kind))
        .collect();
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

/// Vida do chefe: `ladder::BOSS_STRIKES` golpes do jogador esperado do
/// nivel (docs/ESCADA.md). Dimensionada na simulacao (server
/// `balanceamento`): um jogador do nivel, esquivando e com pocao, leva de 1
/// a 4 min conforme a arma; grupo bem menos. Era `8 960 + 269 x nivel` com
/// teto em 20 720 — o teto existia porque o dano do jogador crescia com a
/// faixa do equipamento em saltos; na ladder ele e' reta, e a luta nao
/// encurta com o nivel. Cabe no `u16` do fio.
pub fn vida(nivel: u32) -> i32 {
    crate::ladder::boss_health(nivel)
}

/// Dano do golpe COMUM (mitigado normalmente, `ladder::damage`). O telegrafado
/// nao usa isto: ele tira fracao da vida (`dano_telegrafado`).
///
/// Sai da ladder (docs/ESCADA.md): a defesa esperada do nivel mais tres
/// lobos de liquido. Era `15 + 2,4 x nivel`, que contra a defesa em
/// porcentagem virava 17 de dano no nivel 30 — o chefe so' doia no
/// telegrafico.
pub fn dano(nivel: u32) -> i32 {
    crate::ladder::boss_attack(nivel)
}

/// Defesa do chefe: uma fracao do ataque esperado do nivel maior que a de
/// qualquer mob comum (`ladder::BOSS_DEFENSE`). Quem esta' uma faixa
/// atras bate no piso.
pub fn defesa(nivel: u32) -> i32 {
    crate::ladder::boss_defense(nivel)
}

/// A parte da mitigacao do jogador que ainda e' FRACAO: so' a reducao de
/// identidade (escudo, armadura pesada), com o teto da ladder. A defesa em
/// pontos ja' nao e' porcentagem de nada — ela subtrai do golpe
/// (`ladder::damage`) — e o telegrafado, que tira fracao da VIDA, nem passa
/// por ela.
pub fn resistencia(_defesa: i32, reducao: f32) -> f32 {
    reducao.clamp(0.0, crate::ladder::MAX_REDUCTION)
}

/// O que um golpe telegrafado TIRA de quem ficou dentro: fracao da vida
/// maxima, e so' `RESISTENCIA_NO_TELEGRAFICO` da resistencia vale. Quem desvia
/// nao toma nada; quem fica parado perde uma fatia fixa da barra a cada golpe,
/// qualquer que seja a build.
pub fn dano_telegrafado(h: &Habilidade, fase: u8, hp_max_alvo: i32, resistencia: f32) -> i32 {
    let bonus = if fase >= 1 { BONUS_DA_FASE_2 } else { 1.0 };
    let bruto = hp_max_alvo as f32 * h.dano_mult * VIDA_POR_MULT * bonus;
    ((bruto * (1.0 - resistencia.clamp(0.0, 0.90) * RESISTENCIA_NO_TELEGRAFICO)).round() as i32)
        .max(1)
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
    if hp * 2 <= max {
        1
    } else {
        0
    }
}

pub fn carga(h: &Habilidade, fase: u8) -> f32 {
    if fase >= 1 {
        h.carga_s * 0.85
    } else {
        h.carga_s
    }
}

pub fn recarga(h: &Habilidade, fase: u8) -> f32 {
    if fase >= 1 {
        h.recarga_s * 0.75
    } else {
        h.recarga_s
    }
}

/// Qual golpe comecar agora, se algum: pronto, liberado pela fase e com o
/// alvo no alcance. Prioridade: golpe de fase alta, depois o mais forte, e
/// no empate o primeiro da lista.
pub fn escolher(
    c: &Chefe,
    prontas_em: &[f32; MAX_HABILIDADES],
    agora: f32,
    fase: u8,
    dist: f32,
) -> Option<usize> {
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
pub fn atingidos(
    forma: &Forma,
    centro: glam::Vec2,
    dir: glam::Vec2,
    alvos: &[glam::Vec2],
) -> Vec<usize> {
    alvos
        .iter()
        .enumerate()
        .filter(|(_, p)| forma.contem(centro, dir, **p))
        .map(|(i, _)| i)
        .collect()
}

/// Quanto do aviso ja' encheu (0 no inicio, 1 no impacto).
pub fn preenchimento(decorrido: f32, carga_s: f32) -> f32 {
    if carga_s <= 0.0 {
        1.0
    } else {
        (decorrido / carga_s).clamp(0.0, 1.0)
    }
}

/// Um chefe no mapa da ilha.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChefeNoMapa {
    pub kind: u16,
    pub nome: String,
    pub nivel: u16,
    pub centro: [f32; 2],
    pub vivo: bool,
    /// Unix seconds em que ele volta. 0 = vivo agora.
    ///
    /// ABSOLUTO, e nao "faltam N segundos", por dois motivos que se somam:
    /// quem le' esta' noutro PROCESSO (o mapa-mundi mostra chefe de outra
    /// ilha, e o `sim_time` de la' nao quer dizer nada aqui), e o cliente
    /// precisa descontar sozinho entre uma atualizacao e outra, senao a
    /// contagem congela na tela.
    #[serde(default)]
    pub volta_em_unix: i64,
}

/// Uma ilha no MAPA-MUNDI: o que ela tem de chefe, e se ela esta' no ar.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct IlhaNoMundo {
    /// A zona (`terreno::DefIlha::zona`).
    pub zona: String,
    /// Chefes de campo dela. Vazio = zona no ar mas sem chefe registrado.
    pub chefes: Vec<ChefeNoMapa>,
    /// O processo daquela ilha esta' respondendo. `false` = o que se mostra
    /// e' a ultima noticia, e ela pode estar velha.
    pub no_ar: bool,
}

/// Quanto falta, em segundos, pro chefe voltar. `None` = esta' vivo.
///
/// Uma funcao so' pros dois lados: o cliente conta o mesmo que o servidor
/// contaria, e "vivo" e "faltam 0s" nunca discordam.
pub fn falta_pra_voltar(c: &ChefeNoMapa, agora_unix: i64) -> Option<i64> {
    if c.vivo || c.volta_em_unix <= 0 {
        return None;
    }
    Some((c.volta_em_unix - agora_unix).max(0))
}

/// "7m12s", "48s", "agora". O formato que o jogador le' no mapa.
pub fn conta_regressiva(s: i64) -> String {
    if s <= 0 {
        return "agora".into();
    }
    let (m, r) = (s / 60, s % 60);
    if m == 0 {
        format!("{r}s")
    } else {
        format!("{m}m{r:02}s")
    }
}

#[cfg(test)]
mod testes_do_mapa {
    use super::*;

    fn ch(vivo: bool, volta: i64) -> ChefeNoMapa {
        ChefeNoMapa {
            kind: 0,
            nome: "X".into(),
            nivel: 10,
            centro: [0.0, 0.0],
            vivo,
            volta_em_unix: volta,
        }
    }

    /// Vivo nao tem contagem, e contagem vencida nao fica negativa — um
    /// "-3m" na tela e' pior que nenhum numero, porque parece defeito.
    #[test]
    fn so_o_morto_conta_e_a_conta_nunca_vira_negativa() {
        assert_eq!(falta_pra_voltar(&ch(true, 0), 100), None);
        // Vivo vence o relogio: se os dois discordarem, o vivo manda.
        assert_eq!(falta_pra_voltar(&ch(true, 999), 100), None);
        assert_eq!(falta_pra_voltar(&ch(false, 160), 100), Some(60));
        assert_eq!(falta_pra_voltar(&ch(false, 40), 100), Some(0));
        // Morto sem hora marcada (noticia velha) nao inventa contagem.
        assert_eq!(falta_pra_voltar(&ch(false, 0), 100), None);
    }

    #[test]
    fn a_conta_regressiva_le_como_tempo() {
        assert_eq!(conta_regressiva(0), "agora");
        assert_eq!(conta_regressiva(48), "48s");
        assert_eq!(conta_regressiva(432), "7m12s");
        assert_eq!(conta_regressiva(605), "10m05s");
    }
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
        let a = Anel {
            interno: 2.0,
            externo: 5.0,
        };
        assert!(
            !a.contem(Vec2::ZERO, Vec2::X, Vec2::new(1.0, 0.0)),
            "colado no centro e' seguro"
        );
        assert!(a.contem(Vec2::ZERO, Vec2::X, Vec2::new(0.0, 3.0)));
        assert!(!a.contem(Vec2::ZERO, Vec2::X, Vec2::new(6.0, 0.0)));
    }

    #[test]
    fn cone_so_pra_frente_e_linha_so_no_corredor() {
        let cone = Cone {
            raio: 4.0,
            abertura: 0.6,
        };
        assert!(cone.contem(Vec2::ZERO, Vec2::X, Vec2::new(3.0, 0.5)));
        assert!(
            !cone.contem(Vec2::ZERO, Vec2::X, Vec2::new(-3.0, 0.0)),
            "atras nao"
        );
        assert!(
            !cone.contem(Vec2::ZERO, Vec2::X, Vec2::new(1.0, 2.0)),
            "de lado nao"
        );
        let l = Linha {
            comprimento: 10.0,
            largura: 2.0,
        };
        assert!(l.contem(Vec2::ZERO, Vec2::Y, Vec2::new(0.9, 8.0)));
        assert!(
            !l.contem(Vec2::ZERO, Vec2::Y, Vec2::new(1.1, 8.0)),
            "fora da largura"
        );
        assert!(
            !l.contem(Vec2::ZERO, Vec2::Y, Vec2::new(0.0, -0.5)),
            "atras da origem"
        );
        assert!(
            !l.contem(Vec2::ZERO, Vec2::Y, Vec2::new(0.0, 10.5)),
            "alem do comprimento"
        );
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
            assert!(
                c.kind >= KIND_MIN && kinds.insert(c.kind),
                "{}: kind repetido ou baixo",
                c.nome
            );
            assert!(
                (3..=MAX_HABILIDADES).contains(&c.habilidades.len()),
                "{}: 3–4 golpes",
                c.nome
            );
            assert!(
                c.habilidades.iter().any(|h| h.fase_min == 1),
                "{}: sem golpe de fase 2",
                c.nome
            );
            for h in c.habilidades {
                assert!(
                    h.carga_s >= 0.8 && h.carga_s <= 2.0,
                    "{}/{}: carga fora de 0,8–2 s",
                    c.nome,
                    h.nome
                );
                assert!(h.recarga_s > h.carga_s, "{}/{}", c.nome, h.nome);
            }
            // A zona do chefe tem que EXISTIR. `def_da_zona` e nao o
            // `ARQUIPELAGO` direto: a Ilha Magica e' zona de verdade e nao
            // esta' naquela tabela (ela nao e' degrau de progressao).
            let def = crate::terreno::def_da_zona(c.zona).expect("ilha existe");
            assert!(
                c.nivel >= def.nivel.0 && c.nivel <= def.nivel.1 + 5,
                "{}: nivel fora da ilha",
                c.nome
            );
            // The cap was 60 000 while health went on the wire as u16; since
            // protocol 172 it is u32, and this only guards against a runaway.
            assert!(vida(c.nivel) <= 200_000, "{}: vida {}", c.nome, vida(c.nivel));
        }
        assert!(da_zona("ilha_inicial").len() >= 2);
        for z in ["ilha_gelo", "ilha_deserto", "ilha_planalto", crate::celeste::ZONA, crate::kogen::ZONA, crate::abissal::ZONA] {
            assert!(!da_zona(z).is_empty(), "{z} sem chefe");
        }
        // A ILHA MÁGICA precisa de chefe: a Ilhota do Colosso paga bônus de
        // DROP DE CHEFE, e sem chefe nenhum ela promete o que não existe.
        assert!(
            !da_zona(crate::magica::ZONA).is_empty(),
            "Ilha Mágica sem chefe: a Ilhota do Colosso fica sem sentido"
        );
    }
}
