//! As CRIATURAS em pecas: a marcha e a patada.
//!
//! A port from zone14 (`ModeloVoxel.PassoDaPata`, `GiroDaJunta`, `DoGolpe`).
//! These creatures' legs are LOOSE STUMPS — the style, see
//! docs/PIPELINE_ARTE.md — and a loose leg is animated by TRANSLATION:
//! rotating the stump at its own top moves the foot about 4% of the height
//! while the body crosses metres of ground, and the walk reads as sliding on
//! ice. The foot makes a D: straight back while planted, at the speed the
//! ground passes, and returns at the front, lifting.
//!
//! Not sliding is identity, not luck:
//!
//! ```text
//! the foot's travel in stance == the distance the body walked in stance
//! 2 * MEIA_VIAGEM_DO_PE * height * stretch == FRACAO_DE_APOIO * cycle
//! ```
//!
//! e o `ciclo` sai dai. `o_pe_plantado_nao_desliza` cobra.

use macroquad::models::Mesh;
use macroquad::prelude::{vec3, Quat, Vec3};
use std::f32::consts::{PI, TAU};

/// Meia-viagem do pe', em fracao da altura do bicho: passo de bicho andando.
pub const MEIA_VIAGEM_DO_PE: f32 = 0.18;
/// How much of the cycle the foot spends PLANTED. A walking quadruped always
/// has more feet on the ground than in the air.
pub const FRACAO_DE_APOIO: f32 = 0.60;
/// How much the foot lifts on the return, as a fraction of the half-travel.
/// Without lifting, the D becomes a dragged back-and-forth.
pub const ALTURA_DO_PASSO: f32 = 0.42;
/// Running, the stride stretches instead of only speeding up — otherwise the
/// leg becomes a cartoon.
pub const ALONGA_NA_CORRIDA: f32 = 1.6;
/// Speed (u/s) at which the gait is fully a trot.
const VEL_DE_TROTE: f32 = 5.0;

/// Each creature that walks in pieces: the file
/// (`tools/voxrender/bichos.py`) and the on-screen height, in world units.
pub const BICHOS: [(&str, f32); 31] = [
    ("bichos/lobo_pequeno", 0.9),
    ("bichos/urso", 1.3),
    ("bichos/tigre", 0.95),
    ("bichos/owlbear", 1.5),
    ("bichos/lobo", 2.8),
    // `tools/voxrender/caranguejos.py`: andam de lado (`Anatomia::lateral`)
    ("bichos/caranguejo", 0.5),
    ("bichos/caranguejo_rei", 0.85),
    // The pet and mount ladder: one CREATURE per color (docs/PETS.md,
    // docs/MONTARIAS.md). The height of the ADULT creature — the pet uses the
    // same mesh at a smaller scale, which is what was already done with the
    // wolf and the tiger.
    ("bichos/cervo", 1.6),
    ("bichos/hipogrifo", 1.9),
    ("bichos/dragao", 2.4),
    ("bichos/porco", 0.8),
    // ── The bestiary of the other islands (`economy::kinds_do_bioma`) ──
    // Heights of the real creature: the walrus is low and long, the beetle
    // ground-hugging, the queen bigger than it, and the rocky one squat.
    ("bichos/morsa", 1.1),
    ("bichos/escaravelho", 0.8),
    ("bichos/escaravelho_rainha", 1.3),
    ("bichos/rochoso", 1.2),
    // The WHITE ones of the Glacier. The same height as their relatives: it is
    // the same creature in another coat, and not another species.
    ("bichos/urso_polar", 1.3),
    ("bichos/tigre_branco", 0.95),
    // Island variants (`shared::bestiary`): the species' mesh in the island's
    // coat (`bichos.py: PELAGENS`), at the species' height.
    ("bichos/snow_owlbear", 1.5),
    ("bichos/storm_owlbear", 1.5),
    ("bichos/crag_lynx", 0.95),
    ("bichos/cave_bear", 1.3),
    // Skyreach's winged creatures (`bichos.py: BICHOS_ALADOS`): the
    // species with the hippogriff's wings. The height is the whole model's,
    // wings raised, so it is above the species' own.
    ("bichos/seraph_wolf", 1.7),
    ("bichos/seraph_lynx", 1.5),
    ("bichos/seraph_bear", 2.0),
    ("bichos/seraph_owlbear", 2.3),
    ("bichos/pegasus_stag", 2.3),
    // Kōgen-tō's robots (`bichos.py: BICHOS_ROBO`): the species' mesh in
    // gunmetal, at the species' height.
    ("bichos/mech_hound", 0.95),
    ("bichos/volt_panther", 1.0),
    ("bichos/iron_bear", 1.4),
    ("bichos/dynamo_owlbear", 1.6),
    // The Neon Spire's boss: the dragon in plate, at the dragon's height.
    ("bichos/mech_dragon", 2.4),
];

/// This mob's creature, if it is a creature. People (gunner, mage, archer)
/// are left out: they walk on the character rig, not this one.
/// The MOUNT's creature for this player, with the height already at its scale.
///
/// It exists so a mount walks the SAME path as a mob: the stride's phase
/// accumulates with the creature's cycle (`anda_a_fase`) and the gait comes
/// from the real speed, with no conversion in between. The `kind` of whoever
/// is mounted carries the skin (`render3d`), and the skin says which mount it is.
pub fn da_montaria(
    tag: shared::EntityTag,
    kind: u16,
    montado: bool,
) -> Option<(&'static str, f32)> {
    if tag != shared::EntityTag::Player || !montado {
        return None;
    }
    // The `kind` of a mounted player is the mount's item_id: species and color
    // come from it (docs/MONTARIAS.md).
    let (e, _) = shared::montarias::de_item(kind)?;
    let (nome, altura) = BICHOS.iter().copied().find(|(n, _)| *n == e.bicho)?;
    Some((nome, altura * e.escala))
}

/// A PET's creature (docs/PETS.md), with the height already at its scale.
/// The meta's `kind` is the item_id, which carries species and grade.
pub fn do_pet(tag: shared::EntityTag, kind: u16) -> Option<(&'static str, f32)> {
    if tag != shared::EntityTag::Pet {
        return None;
    }
    let (especie, _) = shared::pets::de_item(kind)?;
    let (nome, altura) = BICHOS.iter().copied().find(|(n, _)| *n == especie.bicho)?;
    Some((nome, altura * especie.escala))
}

pub fn do_mob(tag: shared::EntityTag, kind: u16, boss: bool) -> Option<(&'static str, f32)> {
    if tag != shared::EntityTag::Enemy {
        return None;
    }
    if boss {
        // A field boss: the creature of its preset body (the scale is render3d's).
        // People and pirates do not walk on this rig.
        use shared::bosses::Corpo;
        return match shared::bosses::chefe(kind).map(|c| c.corpo) {
            // Wolf: the boss uses the FULL-BODY wolf (the detailed one), and not the
            // small one scaled up — up close the scaled one's voxels are coarse.
            // The scale is corrected in `fator_do_modelo_de_chefe`.
            Some(Corpo::Bicho(7)) | Some(Corpo::Bicho(0)) | None => Some(BICHOS[4]),
            // The Storm Pegasus moves like the lynx and looks like a pegasus.
            Some(_) if kind == shared::bosses::PEGASUS => {
                BICHOS.iter().copied().find(|(n, _)| *n == "bichos/pegasus_stag")
            }
            // The Sky Dragon Mech fights as the Iron Bear and looks like a dragon.
            Some(_) if kind == shared::bosses::DRAGAO_DA_ESPIRAL => {
                BICHOS.iter().copied().find(|(n, _)| *n == "bichos/mech_dragon")
            }
            Some(Corpo::Bicho(k)) => do_mob(tag, k, false).or(Some(BICHOS[4])),
            Some(_) => None,
        };
    }
    // By NAME, and not by index. Indexing `BICHOS` by position is the kind of
    // thing that breaks silently when the list grows — and it grew on
    // 21/09/2026, with the per-island bestiary.
    let nome = modelo_de_kind(kind)?;
    BICHOS.iter().copied().find(|(n, _)| *n == nome)
}

/// Each mob's model. People (Gunner 2, Mage 4, Archer 6) are left out: they
/// walk on the character rig.
pub fn modelo_de_kind(kind: u16) -> Option<&'static str> {
    Some(match kind {
        0 | 7 => "bichos/lobo_pequeno",
        1 => "bichos/urso",
        3 => "bichos/tigre",
        5 => "bichos/owlbear",
        8 => "bichos/caranguejo",
        9 => "bichos/caranguejo_rei",
        // As outras ilhas.
        10 => "bichos/morsa",
        // The WHITE bear and tiger have their OWN MODEL, with the coat's palette
        // swapped in the generator (`bichos.py: PELAGENS`). Until 21/09/2026 they
        // were the bear and the tiger with a white per-vertex multiplier, and the
        // owner looked and killed the idea: "a normal bear painted white, that
        // doesn't exist, it looks terrible". Paint on top lightens EVERYTHING — the
        // tiger's stripe along with it, and a white tiger with no black stripe is a cat.
        11 => "bichos/urso_polar",
        12 => "bichos/tigre_branco",
        13 => "bichos/escaravelho",
        14 => "bichos/escaravelho_rainha",
        15 => "bichos/rochoso",
        // Island variants (`shared::bestiary`).
        32 => "bichos/snow_owlbear",
        36 => "bichos/crag_lynx",
        38 => "bichos/cave_bear",
        40 => "bichos/storm_owlbear",
        // Skyreach's winged ones (`bichos.py: BICHOS_ALADOS`).
        41 => "bichos/seraph_lynx",
        43 => "bichos/seraph_bear",
        45 => "bichos/seraph_owlbear",
        46 => "bichos/seraph_wolf",
        // Kōgen-tō's robots.
        56 => "bichos/mech_hound",
        58 => "bichos/volt_panther",
        60 => "bichos/iron_bear",
        62 => "bichos/dynamo_owlbear",
        _ => return None,
    })
}

/// How much the boss catalogue's scale (`bosses::Chefe::escala`, designed
/// around the common mob's body) changes when the boss switches to the
/// full-body model: the on-screen size stays the same.
pub fn fator_do_modelo_de_chefe(kind: u16) -> f32 {
    match shared::bosses::chefe(kind).map(|c| c.corpo) {
        Some(shared::bosses::Corpo::Bicho(0)) => BICHOS[0].1 / BICHOS[4].1,
        _ => 1.0,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Junta {
    Tronco,
    Cabeca,
    Pescoco,
    Cauda,
    Pata {
        frente: bool,
        esq: bool,
    },
    /// Wing: dragon, hippogriff and owl. It beats along with the stride, but
    /// with its own amplitude — a wing is not a leg, it does not touch the ground.
    Asa {
        esq: bool,
    },
}

/// Nome do objeto no `.vox` -> junta.
pub fn junta_de(nome: &str) -> Option<Junta> {
    Some(match nome {
        "tronco" => Junta::Tronco,
        "cabeca" => Junta::Cabeca,
        "pescoco" => Junta::Pescoco,
        "cauda" => Junta::Cauda,
        "pata_fd" => Junta::Pata {
            frente: true,
            esq: false,
        },
        "pata_fe" => Junta::Pata {
            frente: true,
            esq: true,
        },
        "pata_td" => Junta::Pata {
            frente: false,
            esq: false,
        },
        "pata_te" => Junta::Pata {
            frente: false,
            esq: true,
        },
        "asa_d" => Junta::Asa { esq: false },
        "asa_e" => Junta::Asa { esq: true },
        _ => return None,
    })
}

/// Where the piece rotates, in the voxel grid (in voxel corners; `hi` inclusive).
///
/// The creature looks towards voxel +Y. The leg hangs from the TOP; the head
/// and neck rotate at the BACK edge, which is where they meet the body; the
/// tail at the FRONT one, for the same reason.
pub fn pivo_vox(j: Junta, lo: [usize; 3], hi: [usize; 3]) -> [f32; 3] {
    let meio = |i: usize| (lo[i] + hi[i] + 1) as f32 * 0.5;
    match j {
        Junta::Pata { .. } => [meio(0), meio(1), (hi[2] + 1) as f32],
        // The wing rotates where it meets the trunk: the INNER edge, on the X axis.
        // Rotating at its middle would tear the wing off the body at every beat.
        Junta::Asa { esq } => [
            if esq {
                (hi[0] + 1) as f32
            } else {
                lo[0] as f32
            },
            meio(1),
            meio(2),
        ],
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

/// What the paw strike needs to know about the body, measured at load time.
#[derive(Clone, Copy, Debug)]
pub struct Anatomia {
    /// Altura na tela, em unidades.
    pub altura: f32,
    /// Z of the creature's frontmost point (the muzzle), in its own space.
    pub frente: f32,
    /// Height of the BACK — the top of the trunk — in world units at the height
    /// the creature was loaded at. It is where the rider sits.
    ///
    /// MEASURED ON THE MODEL, and not written by hand per species. A hand-written
    /// saddle was one number per creature that nobody revised when the scale
    /// changed: on 21/09/2026 the deer grew and the saddle stayed at 81% of its
    /// height — that is, at HEAD height — and the player floated. The trunk is
    /// the only thing that knows where the back is.
    pub lombo: f32,
    /// Pivot of the striking paw (the front right) at rest.
    pub ombro: Vec3,
    /// Which side (sign of X) that paw is on. The strike opens outwards from it
    /// and crosses to the other side.
    pub lado: f32,
    /// Crab: it walks SIDEWAYS. The "front legs" are the pincers and the back
    /// ones the two groups of legs (`tools/voxrender/caranguejos.py`).
    pub lateral: bool,
}

pub struct Bicho {
    pub anat: Anatomia,
    pub pecas: Vec<PecaDeBicho>,
}

/// What the pose needs to know about the frame.
#[derive(Default)]
pub struct Entrada {
    /// Stride phase, in radians: it advances with DISTANCE (see `ciclo`).
    pub passada: f32,
    /// Velocidade desenhada, u/s.
    pub vel: f32,
    /// A clock, for what moves even at rest (tail, head, breathing).
    pub tempo: f32,
    /// Seconds since the start of the last strike. Large = no strike.
    pub golpe: f32,
    /// Desencontra bichos iguais lado a lado.
    pub semente: f32,
    /// Segundos desde o ultimo golpe RECEBIDO.
    pub ferido: Option<f32>,
    /// Where the blow pushes (away from the attacker), in the creature's space.
    pub recuo: Vec3,
}

struct Marcha {
    amp: f32,
    /// The leg "wakes up" with movement: at rest, the stride fades with no snap.
    acorda: f32,
    /// 0 = passeio (quatro tempos) .. 1 = trote (diagonais juntas).
    corre: f32,
    alonga: f32,
}

fn marcha(vel: f32) -> Marcha {
    let amp = (vel / 4.0).clamp(0.0, 1.0);
    let corre = (vel / VEL_DE_TROTE).clamp(0.0, 1.0);
    Marcha {
        amp,
        acorda: (amp * 6.0).min(1.0),
        corre,
        alonga: 1.0 + ALONGA_NA_CORRIDA * corre,
    }
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
        vec3(
            0.0,
            (v * PI).sin() * meia * ALTURA_DO_PASSO,
            -meia + 2.0 * meia * v,
        )
    }
}

// ═══════════════════════════════════════════════════════════════════════
//  A PATADA
// ═══════════════════════════════════════════════════════════════════════
//
// Tres tempos, cada um com o seu trabalho:
//
// 1. LEVANTA — a pata sobe ate' a altura da cabeca e vai pra FORA, pro lado
//    dela; o corpo empina e torce junto, armando. Sai rapido e chega devagar
//    (ease-out): e' a antecipacao, o que avisa o jogador.
// 2. VARRE — a pata corta na HORIZONTAL, na altura da cabeca, de um lado ao
//    outro da cara: 120 graus que o rastro marca, mais o que ela arma antes e
//    o que passa depois. Devagar-rapido-devagar (smootherstep): o meio e' o
//    tapa. O corpo vira junto e mergulha pra frente.
// 3. VOLTA — a pata cai de volta pro chao assentando um pouco pra tras do
//    ponto onde parou (o repique), e o corpo desfaz a torcao. O rastro some
//    com a cauda correndo atras da cabeca.
//
// Cada grandeza termina um tempo exatamente onde o seguinte comeca, entao a
// curva nao tem estalo — `a_patada_nao_tem_estalo` cobra.

pub const T_LEVANTA: f32 = shared::MOB_ATTACK_PREPARE_S;
pub const T_VARRE: f32 = shared::MOB_ATTACK_CUT_S;
pub const T_VOLTA: f32 = 0.32;
pub const DURACAO_DO_GOLPE: f32 = T_LEVANTA + T_VARRE + T_VOLTA;

/// Angulos do arco, a partir da frente do bicho; positivo = lado da pata.
const ARMA: f32 = 80.0 * PI / 180.0;
/// O rastro marca de ABRE a FECHA: os 120 graus do tapa.
pub const ABRE: f32 = 60.0 * PI / 180.0;
pub const FECHA: f32 = -60.0 * PI / 180.0;
/// Onde a pata para de verdade: ela passa do fim do rastro e repica.
const PASSA: f32 = -74.0 * PI / 180.0;

/// O estado da patada num instante.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Golpe {
    /// 0 = pata no lugar dela, 1 = pata no arco.
    pub ergue: f32,
    /// Onde no arco (rad).
    pub angulo: f32,
    /// Quanto acima da cabeca, em fracao da altura (a antecipacao).
    pub acima: f32,
    /// Corpo: mergulho (negativo = empina), torcao (positivo = pro lado da
    /// pata) e avanco (fracao da altura).
    pub pitch: f32,
    pub torce: f32,
    pub avanca: f32,
    /// O quanto o corpo sobe (fracao da altura): empinar levanta o peito.
    pub sobe: f32,
    /// O rastro: de onde a onde no arco, e com que forca.
    pub rastro: Option<(f32, f32, f32)>,
}

fn smooth(u: f32) -> f32 {
    u * u * (3.0 - 2.0 * u)
}

fn smoother(u: f32) -> f32 {
    u * u * u * (u * (u * 6.0 - 15.0) + 10.0)
}

/// A patada no instante `t` desde o comeco. Fora da janela, nada.
pub fn golpe(t: f32) -> Golpe {
    if !(0.0..DURACAO_DO_GOLPE).contains(&t) {
        return Golpe::default();
    }
    if t < T_LEVANTA {
        let u = t / T_LEVANTA;
        let k = 1.0 - (1.0 - u).powi(3);
        return Golpe {
            ergue: k,
            angulo: ARMA,
            acima: 0.12 * k,
            pitch: -0.20 * k,
            torce: 0.30 * k,
            avanca: -0.04 * k,
            sobe: 0.06 * k,
            rastro: None,
        };
    }
    if t < T_LEVANTA + T_VARRE {
        let s = smoother((t - T_LEVANTA) / T_VARRE);
        let angulo = ARMA + (PASSA - ARMA) * s;
        return Golpe {
            ergue: 1.0,
            angulo,
            acima: 0.12 * (1.0 - s),
            pitch: -0.20 + 0.34 * s,
            torce: 0.30 - 0.70 * s,
            avanca: -0.04 + 0.14 * s,
            sobe: 0.06 * (1.0 - s),
            rastro: (angulo < ABRE).then_some((ABRE, angulo, 1.0)),
        };
    }
    let s = smooth((t - T_LEVANTA - T_VARRE) / T_VOLTA);
    Golpe {
        ergue: 1.0 - s,
        angulo: PASSA + (FECHA - PASSA) * s,
        acima: 0.0,
        pitch: 0.14 * (1.0 - s),
        torce: -0.40 * (1.0 - s),
        avanca: 0.10 * (1.0 - s),
        sobe: 0.0,
        rastro: Some((ABRE + (PASSA - ABRE) * s, PASSA, 1.0 - s)),
    }
}

/// Altura do pivo da pata no arco: um pouco acima do meio da cabeca.
fn altura_do_arco(a: &Anatomia) -> f32 {
    0.9 * a.altura
}

/// Raio do arco, medido do ombro: a pata passa na frente do focinho.
fn raio_do_arco(a: &Anatomia) -> f32 {
    (a.frente - a.ombro.z).max(0.1) + 0.10 * a.altura
}

/// Ponto do arco no angulo `phi`, no espaco do bicho, a `dr` do raio.
fn no_arco(a: &Anatomia, phi: f32, y: f32, dr: f32) -> Vec3 {
    let r = raio_do_arco(a) + dr;
    vec3(a.lado * r * phi.sin(), y, a.ombro.z + r * phi.cos())
}

/// O corpo inteiro neste quadro.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Corpo {
    /// Quanto tudo sobe (u): o balanco do passo.
    pub sobe: f32,
    /// So' o tronco, a cabeca e a cauda: a patada empina, torce e avanca o
    /// corpo por cima das patas plantadas.
    pub pitch: f32,
    pub torce: f32,
    pub avanca: f32,
    pub sobe_tronco: f32,
    /// Achatamento elastico do golpe recebido (fracao; `rig::esmagamento`).
    pub esmaga: f32,
    /// Deslize de lado do recuo, em unidades.
    pub lado: f32,
}

/// O corpo sobe quando a perna abre: no extremo do passo ela esta' inclinada
/// e mais curta na vertical, entao o quadril sobe pra o pe' nao afundar.
pub fn corpo(e: &Entrada, a: &Anatomia) -> Corpo {
    let m = marcha(e.vel);
    let respiro = (e.tempo * 1.8 + e.semente * 0.37).sin() * 0.006 * a.altura;
    let g = golpe(e.golpe);
    // o golpe recebido: empina, escorrega pra longe de quem bateu e amassa
    let (k, esmaga) = match e.ferido {
        Some(t) => (crate::rig::tranco(t), crate::rig::esmagamento(t)),
        None => (0.0, 0.0),
    };
    Corpo {
        sobe: e.passada.sin().abs() * 0.045 * a.altura * m.acorda + respiro,
        pitch: g.pitch - 0.30 * k,
        torce: g.torce * a.lado,
        avanca: g.avanca * a.altura + e.recuo.z * 0.12 * a.altura * k,
        sobe_tronco: g.sobe * a.altura,
        esmaga,
        lado: e.recuo.x * 0.12 * a.altura * k,
    }
}

/// Giro (em volta do pivo) e deslocamento (u, espaco do bicho) de uma peca
/// cujo pivo parado e' `pivo`.
pub fn peca(j: Junta, e: &Entrada, a: &Anatomia, pivo: Vec3) -> (Quat, Vec3) {
    let m = marcha(e.vel);
    let x = |ang: f32| Quat::from_rotation_x(ang);
    let dor = e.ferido.map_or(0.0, crate::rig::tranco);
    match j {
        Junta::Tronco => (Quat::IDENTITY, Vec3::ZERO),
        Junta::Cauda => (
            x((e.tempo * 2.2 + e.semente * 0.01).sin() * 0.18 + 0.4 * dor),
            Vec3::ZERO,
        ),
        Junta::Cabeca | Junta::Pescoco => {
            // no golpe a cabeca da' o tranco pra tras
            (
                x((e.tempo * 1.6).sin() * 0.12 + e.passada.sin() * 0.05 * m.amp - 0.35 * dor),
                Vec3::ZERO,
            )
        }
        // A asa bate em volta do proprio eixo (Z, o do corpo): pra cima e pra
        // baixo. Parada ela respira devagar; andando, bate no ritmo da
        // passada — e' o que faz o dragao parecer que se sustenta, e nao que
        // desliza com duas placas presas nas costas.
        Junta::Asa { esq } => {
            let lento = (e.tempo * 1.5 + e.semente as f32 * 0.01).sin() * 0.10;
            let batida = e.passada.sin() * 0.42 * m.acorda;
            let ang = (lento + batida) * if esq { -1.0 } else { 1.0 };
            (Quat::from_rotation_z(ang), Vec3::ZERO)
        }
        Junta::Pata { frente, esq } if a.lateral => pata_de_caranguejo(frente, esq, e, a, pivo),
        Junta::Pata { frente, esq } => {
            // o toco inclina POUCO, acompanhando o passo: quem move o pe' e'
            // a translacao. Inclinar muito volta a girar o toco no topo.
            let fase = e.passada + atraso_da_pata(frente, esq, m.corre) * TAU;
            let giro = x(fase.sin() * 0.20 * m.acorda);
            let meia = MEIA_VIAGEM_DO_PE * a.altura * m.acorda * m.alonga;
            let passo = passo_da_pata(frente, esq, e.passada, m.corre, meia);
            // a patada e' da pata do lado `a.lado`, a da frente
            let golpeia = frente && (pivo.x >= 0.0) == (a.lado >= 0.0);
            let g = golpe(e.golpe);
            if !golpeia || g.ergue <= 0.0 {
                return (giro, passo);
            }
            let alvo = no_arco(a, g.angulo, altura_do_arco(a) + g.acima * a.altura, 0.0) - pivo;
            // no arco a sola vira pra frente e aponta pra onde o tapa vai
            let no_tapa = Quat::from_rotation_y(a.lado * g.angulo) * x(-1.2);
            (giro.slerp(no_tapa, g.ergue), passo.lerp(alvo, g.ergue))
        }
    }
}

/// O rastro das garras, pronto pra desenhar no espaco do bicho (sem a torcao
/// do corpo: o rastro fica onde a pata passou, nao vira junto).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rastro {
    pub centro: Vec3,
    pub raio: f32,
    pub lado: f32,
    /// Do angulo `de` (a cauda) ao `ate` (a cabeca).
    pub de: f32,
    pub ate: f32,
    pub forca: f32,
    /// Largura de cada risco e o vao entre os tres.
    pub largura: f32,
    pub vao: f32,
}

pub fn rastro(e: &Entrada, a: &Anatomia) -> Option<Rastro> {
    let (de, ate, forca) = golpe(e.golpe).rastro?;
    if forca <= 0.01 || (de - ate).abs() < 1e-3 {
        return None;
    }
    Some(Rastro {
        // na ponta das garras, um pouco abaixo do pivo da pata
        centro: vec3(0.0, altura_do_arco(a) - 0.08 * a.altura, a.ombro.z),
        raio: raio_do_arco(a),
        lado: a.lado,
        de,
        ate,
        forca,
        largura: 0.05 * a.altura + 0.02,
        vao: 0.09 * a.altura,
    })
}

// ═══════════════════════════════════════════════════════════════════════
//  O CARANGUEJO
// ═══════════════════════════════════════════════════════════════════════

/// Pernas e pincas do caranguejo.
///
/// Ele anda de LADO: o desenho gira o corpo 90 graus (`yaw_lateral`), e o
/// movimento cai no -X do corpo. Por isso o pe' plantado corre no X, com a
/// mesma identidade de nao-deslize da marcha dos quadrupedes. Os dois grupos
/// de pernas vao em contratempo. A pinca abre e fecha devagar parada; no
/// golpe, a do lado `a.lado` belisca pelo mesmo arco da patada, mais baixo.
fn pata_de_caranguejo(
    frente: bool,
    esq: bool,
    e: &Entrada,
    a: &Anatomia,
    pivo: Vec3,
) -> (Quat, Vec3) {
    let m = marcha(e.vel);
    if !frente {
        let meia = MEIA_VIAGEM_DO_PE * a.altura * m.acorda * m.alonga;
        let passo = passo_da_pata(false, esq, e.passada, m.corre, meia);
        return (Quat::IDENTITY, vec3(-passo.z, passo.y, 0.0));
    }
    let fase = e.tempo * 3.1 + e.semente * 0.3 + if esq { 1.3 } else { 0.0 };
    let abre = Quat::from_rotation_x(fase.sin() * 0.12);
    let golpeia = (pivo.x >= 0.0) == (a.lado >= 0.0);
    let g = golpe(e.golpe);
    if !golpeia || g.ergue <= 0.0 {
        return (abre, Vec3::ZERO);
    }
    let alvo = no_arco(
        a,
        g.angulo,
        altura_do_arco(a) * 0.55 + g.acima * a.altura,
        0.0,
    ) - pivo;
    let belisca = Quat::from_rotation_y(a.lado * g.angulo) * Quat::from_rotation_x(-0.6);
    (
        abre.slerp(belisca, g.ergue),
        Vec3::ZERO.lerp(alvo, g.ergue * 0.6),
    )
}

/// Quanto o desenho gira o caranguejo pra ele andar de lado: 90 graus
/// andando, nada parado, e volta de frente no golpe (a pinca aponta pro alvo).
/// Suave nas duas pontas: a velocidade desenhada e o `ergue` do golpe ja' sao.
pub fn yaw_lateral(a: &Anatomia, e: &Entrada) -> f32 {
    if !a.lateral {
        return 0.0;
    }
    let andando = (e.vel / 1.2).clamp(0.0, 1.0);
    std::f32::consts::FRAC_PI_2 * andando * (1.0 - golpe(e.golpe).ergue)
}

/// Altura do boneco, em unidades de mundo. Nao ha' constante pra isso: o
/// personagem e' um rig de voxels e a altura sai da malha. 1,8 e' a medida
/// que o resto do jogo assume (`PULO_ALTURA` e' exatamente um corpo).
#[cfg(test)]
const ALTURA_DO_JOGADOR: f32 = 1.8;

/// Altura em unidades de mundo de um pet, ja' com a escala da criatura.
/// Existe pra os testes de porte falarem do numero que o jogador ve'.
#[cfg(test)]
fn altura_do_pet(e: &shared::pets::Especie) -> f32 {
    BICHOS
        .iter()
        .find(|(n, _)| *n == e.bicho)
        .map_or(0.0, |(_, a)| a * e.escala)
}

#[cfg(test)]
fn altura_da_montaria(e: &shared::montarias::Especie) -> f32 {
    BICHOS
        .iter()
        .find(|(n, _)| *n == e.bicho)
        .map_or(0.0, |(_, a)| a * e.escala)
}

#[cfg(test)]
mod tests {
    /// PET e' bichinho: nenhum chega perto do porte de uma montaria, e a
    /// ladder cresce de leve em vez de dar um salto no topo.
    ///
    /// O filhote de dragao saiu com 0,91 na primeira versao — quase o dobro
    /// dos outros pets e metade de um jogador — porque a altura do catalogo
    /// (`BICHOS`) e' a do bicho ADULTO, e o dragao vem com 2,4.
    #[test]
    fn nenhum_pet_chega_ao_tamanho_de_montaria() {
        let menor_montaria = shared::montarias::ESPECIES
            .iter()
            .map(altura_da_montaria)
            .fold(f32::MAX, f32::min);
        let mut anterior = 0.0f32;
        for e in shared::pets::ESPECIES.iter() {
            let h = altura_do_pet(e);
            assert!(h > 0.0, "{}: sem modelo em BICHOS", e.nome);
            assert!(
                h < menor_montaria * 0.7,
                "{} tem {h:.2} — perto da montaria mais baixa ({menor_montaria:.2})",
                e.nome
            );
            assert!(
                h >= anterior - 0.01,
                "{} ({h:.2}) encolheu em relacao ao grau anterior ({anterior:.2})",
                e.nome
            );
            anterior = h;
        }
        // E o topo nao dispara: o dragao e' filhote, nao um dragao pequeno.
        let topo = altura_do_pet(shared::pets::ESPECIES.last().unwrap());
        let base = altura_do_pet(&shared::pets::ESPECIES[0]);
        assert!(
            topo < base * 1.6,
            "o topo ({topo:.2}) dobrou a base ({base:.2})"
        );
        // E a escala tem que estar CHEGANDO no desenho: `render3d` multiplica
        // por ela porque a altura e' assada por ARQUIVO, e o mesmo arquivo
        // serve o pet e a montaria. Se o dragao-pet e o dragao-montaria
        // dividissem o porte, era este o teste que faltava.
        let mesmo_bicho = shared::montarias::ESPECIES
            .iter()
            .find(|m| m.bicho == shared::pets::ESPECIES[4].bicho)
            .expect("o dragao serve os dois");
        assert!(
            altura_da_montaria(mesmo_bicho) > topo * 2.0,
            "o dragao-montaria ({:.2}) tem que ser MUITO maior que o filhote ({topo:.2})",
            altura_da_montaria(mesmo_bicho)
        );
    }

    /// MONTARIA nao pode ser mais baixa que quem monta, e a ladder sobe: o
    /// dragao tem que ser visivelmente maior que o cervo.
    #[test]
    fn a_montaria_e_maior_que_o_jogador_e_a_escada_sobe() {
        let mut anterior = 0.0f32;
        for e in shared::montarias::ESPECIES.iter() {
            let h = altura_da_montaria(e);
            assert!(h > 0.0, "{}: sem modelo em BICHOS", e.nome);
            // O PISO E' O JOGADOR INTEIRO, e nao 75% dele.
            //
            // Com 0,75 este teste dava licenca pra exatamente o defeito que
            // ele existia pra pegar: o cervo passava com 1,46 contra os 1,8
            // do boneco, e o dono viu duas vezes antes de eu olhar o numero.
            // Um guarda que aceita o que ele proibe nao e' guarda.
            assert!(
                h > ALTURA_DO_JOGADOR,
                "{} tem {h:.2} contra {ALTURA_DO_JOGADOR:.2} do jogador: \
                 montaria mais baixa que quem monta nao le' como montaria",
                e.nome
            );
            assert!(
                h > anterior,
                "{} ({h:.2}) nao passou do grau anterior ({anterior:.2})",
                e.nome
            );
            anterior = h;
        }
    }

    /// Onde o cavaleiro SENTA, medido no proprio .vox.
    ///
    /// Este teste existe porque o anterior nao existia de verdade: ele
    /// comparava a altura da sela com um numero ESCRITO NA MESMA TABELA, e
    /// aceitava qualquer coisa abaixo do topo da cabeca (`sela < h`). O cervo
    /// passou com a sela em 81% da altura — na cabeca — e o dono viu o
    /// personagem boiando. Guarda que le' o mesmo numero que guarda nao
    /// guarda nada; este abre o modelo.
    #[test]
    fn o_cavaleiro_senta_no_lombo_e_nao_na_cabeca() {
        for e in shared::montarias::ESPECIES.iter() {
            let caminho = format!("../../assets/vox/{}.vox", e.bicho);
            let bytes = std::fs::read(&caminho).unwrap_or_else(|_| panic!("sem {caminho}"));
            let pecas = crate::vox::parse_nomeado(&bytes).expect("vox valido");
            let caixas: Vec<_> = pecas.iter().map(|(_, m)| m.bounds()).collect();
            let (mut lo, mut hi) = ([usize::MAX; 3], [0usize; 3]);
            for ((_, m), (l, h)) in pecas.iter().zip(&caixas) {
                if m.cells.iter().all(|c| *c == 0) {
                    continue;
                }
                for i in 0..3 {
                    lo[i] = lo[i].min(l[i]);
                    hi[i] = hi[i].max(h[i]);
                }
            }
            let base = BICHOS
                .iter()
                .find(|(n, _)| *n == e.bicho)
                .map(|(_, a)| *a)
                .unwrap_or_else(|| panic!("{}: sem altura em BICHOS", e.nome));
            let escala = base / (hi[2] - lo[2] + 1) as f32;
            let lombo = crate::vox::lombo_medido(&pecas, &caixas, lo[2], escala, base);
            let frac = lombo / base;
            // O lombo de um quadrupede fica entre a metade e tres quartos da
            // altura: abaixo disso e' a barriga, acima e' pescoco e cabeca.
            // 1. O modelo TEM tronco. Sem esta, um .vox sem a peca cai no
            //    palpite de 66% em silencio, e o teste aprovaria o palpite.
            assert!(
                pecas.iter().any(|(n, _)| n == "tronco"),
                "{}: {} sem peca 'tronco' — o lombo viraria chute",
                e.nome,
                e.bicho
            );
            // 2. O cavaleiro senta ABAIXO do topo do bicho. Era isto que
            //    estava quebrado: o cervo sentava a 81% de um bicho cuja
            //    cabeca e chifres ocupam o terco de cima.
            assert!(
                lombo < base,
                "{}: lombo {lombo:.2} no topo de {base:.2} — o cavaleiro boia",
                e.nome
            );
            // 3. E ACIMA do pivo da pata dianteira, senao ele afunda no
            //    bicho em vez de montar nele.
            let ombro_y = pecas
                .iter()
                .zip(&caixas)
                .find(|((n, _), _)| n == "pata_fd")
                .map(|(_, (l, h))| (l[2] + h[2]) as f32 * 0.5 - lo[2] as f32)
                .map_or(0.0, |z| z * escala);
            assert!(
                lombo > ombro_y,
                "{}: lombo {lombo:.2} abaixo do ombro ({ombro_y:.2}) — afunda",
                e.nome
            );
            let _ = frac;
        }
    }

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
                            assert!(
                                (no_mundo - a).abs() < 1e-3,
                                "vel {vel}: pe' deslizou {}",
                                no_mundo - a
                            );
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
        let mut passeio: Vec<f32> = PATAS
            .iter()
            .map(|&(f, e)| atraso_da_pata(f, e, 0.0))
            .collect();
        passeio.sort_by(|a, b| a.partial_cmp(b).unwrap());
        assert_eq!(passeio, vec![0.0, 0.25, 0.5, 0.75]);
        // diagonal = dianteira de um lado com a traseira do outro
        assert_eq!(
            atraso_da_pata(true, false, 1.0),
            atraso_da_pata(false, true, 1.0)
        );
        assert_eq!(
            atraso_da_pata(true, true, 1.0),
            atraso_da_pata(false, false, 1.0)
        );
        assert_ne!(
            atraso_da_pata(true, true, 1.0),
            atraso_da_pata(true, false, 1.0)
        );
    }

    fn lobo() -> Anatomia {
        Anatomia {
            altura: 1.0,
            frente: 0.55,
            lombo: 0.62,
            ombro: vec3(0.12, 0.3, 0.2),
            lado: 1.0,
            lateral: false,
        }
    }

    fn caranguejo() -> Anatomia {
        Anatomia {
            altura: 0.5,
            frente: 0.4,
            lombo: 0.33,
            ombro: vec3(0.15, 0.2, 0.2),
            lado: 1.0,
            lateral: true,
        }
    }

    #[test]
    fn caranguejo_anda_de_lado_sem_deslizar_o_pe() {
        let a = caranguejo();
        for vel in [1.0f32, 2.0] {
            let c = ciclo(a.altura, vel);
            for esq in [true, false] {
                let mut antes: Option<f32> = None;
                for k in 0..2000 {
                    let passada = k as f32 * TAU / 1000.0;
                    let e = Entrada {
                        passada,
                        vel,
                        ..Default::default()
                    };
                    let (_, d) = peca(
                        Junta::Pata { frente: false, esq },
                        &e,
                        &a,
                        vec3(0.0, 0.1, 0.0),
                    );
                    assert_eq!(d.z, 0.0, "perna de caranguejo nao anda pra frente");
                    // o corpo anda no -X; o pe' plantado fica no lugar do mundo
                    let no_mundo = -(passada / TAU * c) + d.x;
                    if d.y == 0.0 {
                        if let Some(p) = antes {
                            assert!(
                                (no_mundo - p).abs() < 1e-3,
                                "vel {vel}: pe' deslizou {}",
                                no_mundo - p
                            );
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
    fn caranguejo_gira_de_lado_andando_e_volta_de_frente_no_golpe() {
        let a = caranguejo();
        let parado = Entrada {
            vel: 0.0,
            golpe: 99.0,
            ..Default::default()
        };
        let andando = Entrada {
            vel: 2.0,
            golpe: 99.0,
            ..Default::default()
        };
        let golpeando = Entrada {
            vel: 2.0,
            golpe: T_LEVANTA + T_VARRE * 0.5,
            ..Default::default()
        };
        assert_eq!(yaw_lateral(&a, &parado), 0.0);
        assert!((yaw_lateral(&a, &andando) - std::f32::consts::FRAC_PI_2).abs() < 1e-4);
        assert!(
            yaw_lateral(&a, &golpeando) < 1e-4,
            "no golpe a pinca aponta pro alvo"
        );
        assert_eq!(yaw_lateral(&lobo(), &andando), 0.0, "lobo anda de frente");
    }

    #[test]
    fn a_pinca_belisca_so_no_golpe() {
        let a = caranguejo();
        let pinca = |t: f32| {
            peca(
                Junta::Pata {
                    frente: true,
                    esq: false,
                },
                &Entrada {
                    golpe: t,
                    ..Default::default()
                },
                &a,
                a.ombro,
            )
            .1
        };
        assert_eq!(pinca(99.0), Vec3::ZERO, "parada a pinca so' abre e fecha");
        assert!(
            pinca(T_LEVANTA + T_VARRE * 0.5).length() > 0.05,
            "no golpe ela sai do lugar"
        );
        let outra = peca(
            Junta::Pata {
                frente: true,
                esq: true,
            },
            &Entrada {
                golpe: T_LEVANTA,
                ..Default::default()
            },
            &a,
            vec3(-0.15, 0.2, 0.2),
        )
        .1;
        assert_eq!(outra, Vec3::ZERO, "so' uma pinca golpeia");
    }

    fn entrada(golpe: f32) -> Entrada {
        Entrada {
            passada: 1.3,
            vel: 0.0,
            tempo: 4.0,
            golpe,
            semente: 7.0,
            ..Default::default()
        }
    }

    fn pata_que_golpeia(t: f32) -> Vec3 {
        let a = lobo();
        let (_, d) = peca(
            Junta::Pata {
                frente: true,
                esq: false,
            },
            &entrada(t),
            &a,
            a.ombro,
        );
        a.ombro + d
    }

    /// Todo modelo da tabela tem arquivo, e todo mob aponta pra um da tabela.
    ///
    /// Ela cresceu duas vezes em um dia — o bestiario por ilha e, depois, os
    /// brancos da Geleira — e um nome errado aqui nao da' erro nenhum: o
    /// bicho some da tela e vira o fallback, calado.
    #[test]
    fn todo_modelo_de_bicho_tem_arquivo() {
        let raiz = concat!(env!("CARGO_MANIFEST_DIR"), "/../../assets/vox/");
        for (nome, altura) in BICHOS {
            let caminho = format!("{raiz}{nome}.vox");
            assert!(
                std::path::Path::new(&caminho).exists(),
                "{nome}: sem .vox (rode tools/voxrender/bichos.py)"
            );
            assert!(altura > 0.0, "{nome}: altura {altura}");
        }
        for kind in 0..64u16 {
            let Some(nome) = modelo_de_kind(kind) else {
                continue;
            };
            assert!(
                BICHOS.iter().any(|(n, _)| *n == nome),
                "kind {kind} aponta pra {nome}, que nao esta' na tabela"
            );
        }
    }

    /// O urso e o tigre BRANCOS sao malha PROPRIA, e nao os normais tingidos.
    /// Foi o dono quem reprovou a tinta: ela clareia a listra do tigre junto
    /// com o pelo, e tigre branco sem listra preta e' um gato.
    #[test]
    fn os_brancos_tem_modelo_proprio() {
        for (branco, normal) in [(11u16, 1u16), (12, 3), (32, 5), (36, 3), (38, 1), (40, 5)] {
            let b = modelo_de_kind(branco).unwrap();
            let n = modelo_de_kind(normal).unwrap();
            assert_ne!(b, n, "o kind {branco} ainda usa a malha do {normal}");
        }
    }

    #[test]
    fn parado_a_pata_nao_se_mexe() {
        let a = lobo();
        for (frente, esq) in PATAS {
            let pivo = vec3(
                if esq { -0.12 } else { 0.12 },
                0.3,
                if frente { 0.2 } else { -0.2 },
            );
            let (giro, d) = peca(Junta::Pata { frente, esq }, &entrada(99.0), &a, pivo);
            assert_eq!(giro, Quat::IDENTITY);
            assert_eq!(d, Vec3::ZERO);
        }
    }

    #[test]
    fn a_pata_sobe_ate_a_cabeca() {
        let no_alto = pata_que_golpeia(T_LEVANTA);
        assert!(no_alto.y >= 0.85, "pata so' chegou a {}", no_alto.y);
        // e vai pra FORA, pro lado dela, antes de cruzar
        assert!(no_alto.x > 0.3, "{no_alto:?}");
    }

    #[test]
    fn a_patada_varre_120_graus_na_horizontal() {
        let (de, ate, _) = golpe(T_LEVANTA + T_VARRE - 1e-4).rastro.unwrap();
        assert!(
            de - ate >= 120f32.to_radians() - 1e-3,
            "{} graus",
            (de - ate).to_degrees()
        );
        // na varrida a pata fica na altura da cabeca: sem arco pra cima
        let alturas: Vec<f32> = (0..=40)
            .map(|k| pata_que_golpeia(T_LEVANTA + T_VARRE * k as f32 / 40.0).y)
            .collect();
        let (lo, hi) = alturas
            .iter()
            .fold((f32::MAX, f32::MIN), |(l, h), y| (l.min(*y), h.max(*y)));
        assert!(lo >= 0.85 && hi - lo <= 0.13, "{lo}..{hi}");
        // cruza de um lado ao outro da cara
        assert!(pata_que_golpeia(T_LEVANTA + T_VARRE - 1e-4).x < -0.3);
    }

    #[test]
    fn a_patada_nao_tem_estalo() {
        let a = lobo();
        let mut antes = (pata_que_golpeia(0.0), corpo(&entrada(0.0), &a));
        let mut t = 0.0;
        while t < DURACAO_DO_GOLPE + 0.05 {
            t += 0.002;
            let agora = (pata_que_golpeia(t), corpo(&entrada(t), &a));
            assert!(agora.0.distance(antes.0) < 0.05, "pata saltou em t={t}");
            assert!(
                (agora.1.torce - antes.1.torce).abs() < 0.02,
                "torcao saltou em t={t}"
            );
            assert!(
                (agora.1.pitch - antes.1.pitch).abs() < 0.02,
                "mergulho saltou em t={t}"
            );
            antes = agora;
        }
        // e termina no lugar
        assert_eq!(pata_que_golpeia(DURACAO_DO_GOLPE + 0.01), a.ombro);
        assert_eq!(golpe(DURACAO_DO_GOLPE + 0.01), Golpe::default());
    }

    #[test]
    fn so_uma_pata_golpeia() {
        let a = lobo();
        let t = T_LEVANTA + T_VARRE * 0.5;
        let outra = vec3(-0.12, 0.3, 0.2);
        let (_, d) = peca(
            Junta::Pata {
                frente: true,
                esq: true,
            },
            &entrada(t),
            &a,
            outra,
        );
        assert_eq!(d, Vec3::ZERO);
        assert!(pata_que_golpeia(t).distance(a.ombro) > 0.4);
    }

    #[test]
    fn o_rastro_nasce_na_varrida_e_some_na_volta() {
        let a = lobo();
        assert!(rastro(&entrada(T_LEVANTA * 0.5), &a).is_none());
        assert!(rastro(&entrada(T_LEVANTA + T_VARRE * 0.6), &a).is_some());
        let fim = rastro(&entrada(DURACAO_DO_GOLPE - 0.03), &a).unwrap();
        assert!(fim.forca < 0.1 && (fim.de - fim.ate).abs() < 0.2);
        assert!(rastro(&entrada(DURACAO_DO_GOLPE + 0.01), &a).is_none());
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
        assert_eq!(do_mob(T::Enemy, 8, false).unwrap().0, "bichos/caranguejo");
        assert_eq!(
            do_mob(T::Enemy, 9, false).unwrap().0,
            "bichos/caranguejo_rei"
        );
    }
}
