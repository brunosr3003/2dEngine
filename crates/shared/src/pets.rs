//! Pets coletores (docs/PETS.md).
//!
//! O pet e' ITEM DE BOLSA, nao posse de conta: por isso e' negociavel no
//! mercado e combinavel na aba Combinar do Craft, igual a chave e ao material.
//! O grau mora no `item_id` (cinco ids seguidos por especie, `pet_no_grau`),
//! a mesma convencao dos materiais coloridos.
//!
//! Equipado no slot `EquipSlot::Pet`, ele nasce no mundo, anda ate' o saque
//! caido no chao e credita no dono ao encostar. Os atributos dele entram como
//! PONTOS ALOCADOS (`STAT_POINT_BONUS`), nao como bonus proprio — e' por isso
//! que ele "comba" com a classe sem regra nova: owlbear dá INT, e INT so' vira
//! ataque com arma magica.

use crate::constants::{item_id, stat_idx, STAT_COUNT};

/// Quantas criaturas — uma por grau.
pub const ESPECIE_COUNT: usize = 5;
/// Ultimo grau (1 cinza, 2 verde, 3 azul, 4 roxo, 5 laranja).
pub const GRAU_MAX: u8 = 5;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Especie {
    /// O grau desta criatura (1 cinza .. 5 laranja). E' tambem a posicao
    /// dela em `ESPECIES`.
    pub grau: u8,
    pub nome: &'static str,
    /// Modelo em `client::bicho::BICHOS`.
    pub bicho: &'static str,
    /// Escala sobre a altura em que o bicho e' carregado.
    pub escala: f32,
    /// Peso de cada atributo, somando `PESO_TOTAL`. E' o que faz a especie
    /// servir a uma classe e nao a outra.
    pub afinidade: [u8; STAT_COUNT],
    pub descricao: &'static str,
}

/// Os pesos de `afinidade` somam isto.
pub const PESO_TOTAL: u8 = 10;

/// A ladder do pet: do bichinho de quintal ao filhote de dragao. A ordem E'
/// o grau — subir de cor e' trocar de bicho, nao repintar o mesmo.
///
/// **A `escala` e' o que faz o FILHOTE ser filhote.** Ela multiplica a altura
/// em que o bicho e' carregado (`client::bicho::BICHOS`), e essas alturas sao
/// do bicho ADULTO: o dragao vem com 2,4 — mais alto que o jogador, e do
/// tamanho da montaria. A ladder sai em 0,65 / 0,70 / 0,75 / 0,85 / 0,95:
/// cresce de leve, todos claramente menores que a montaria mais baixa
/// (1,46), e `nenhum_pet_chega_ao_tamanho_de_montaria` cobra o teto.
pub const ESPECIES: [Especie; ESPECIE_COUNT] = [
    Especie {
        grau: 1,
        nome: "Porquinho",
        bicho: "bichos/porco",
        escala: 0.812,
        // VIT 6, RES 4 — o gordinho aguenta.
        afinidade: pesos(&[(stat_idx::VIT, 6), (stat_idx::RES, 4)]),
        descricao: "Come tudo e não reclama. Vitalidade.",
    },
    Especie {
        grau: 2,
        nome: "Filhote de Lobo",
        bicho: "bichos/lobo_pequeno",
        escala: 0.778,
        afinidade: pesos(&[(stat_idx::DES, 6), (stat_idx::SPD, 4)]),
        descricao: "Rápido e curioso. Destreza e velocidade.",
    },
    Especie {
        grau: 3,
        nome: "Filhote de Tigre",
        bicho: "bichos/tigre",
        escala: 0.789,
        afinidade: pesos(&[(stat_idx::FOR, 5), (stat_idx::DES, 5)]),
        descricao: "Caçador desde pequeno. Força e destreza.",
    },
    Especie {
        grau: 4,
        nome: "Corujurso",
        bicho: "bichos/owlbear",
        escala: 0.567,
        afinidade: pesos(&[(stat_idx::INT, 5), (stat_idx::FOR, 5)]),
        descricao: "Estranho e sábio. O pet da arma mágica.",
    },
    Especie {
        grau: 5,
        nome: "Filhote de Dragão",
        bicho: "bichos/dragao",
        escala: 0.396,
        afinidade: pesos(&[(stat_idx::FOR, 4), (stat_idx::INT, 3), (stat_idx::VIT, 3)]),
        descricao: "Pequeno, e já sabe disso. O topo da ladder.",
    },
];

/// Peso do atributo PRINCIPAL e do secundario, somando `PESO_TOTAL`.
pub const PESO_PRINCIPAL: u8 = 6;
pub const PESO_SECUNDARIO: u8 = 4;

/// Sorteia a afinidade de um bicho: `[principal, secundario]`, indices de
/// `stat_idx`, sempre DIFERENTES. `r1` e `r2` em 0..1.
///
/// A afinidade deixou de sair da criatura em 20/09/2026 (pedido do dono: "os
/// atributos que o pet da' podem ser aleatorios, e ter como roletar eles com
/// um item de cash"). A criatura decide o porte e a cor; o que ela EMPRESTA
/// e' sorte — e sorte que se pode comprar de novo, com a Pedra de Afinidade.
pub fn rolar_afinidade(r1: f32, r2: f32) -> [u8; 2] {
    let n = STAT_COUNT as f32;
    let a = ((r1.clamp(0.0, 0.999_999) * n) as usize).min(STAT_COUNT - 1);
    // O segundo sorteia entre os N-1 restantes e pula o primeiro: sortear em
    // N e repetir ate' diferir podia nao terminar.
    let b = ((r2.clamp(0.0, 0.999_999) * (n - 1.0)) as usize).min(STAT_COUNT - 2);
    let b = if b >= a { b + 1 } else { b };
    [a as u8, b as u8]
}

/// Os pesos de uma afinidade rolada.
pub fn pesos_de(afinidade: [u8; 2]) -> [u8; STAT_COUNT] {
    let mut v = [0u8; STAT_COUNT];
    let (a, b) = (afinidade[0] as usize, afinidade[1] as usize);
    if a < STAT_COUNT {
        v[a] = PESO_PRINCIPAL;
    }
    if b < STAT_COUNT && b != a {
        v[b] = PESO_SECUNDARIO;
    }
    v
}

const fn pesos(pares: &[(usize, u8)]) -> [u8; STAT_COUNT] {
    let mut v = [0u8; STAT_COUNT];
    let mut i = 0;
    while i < pares.len() {
        v[pares[i].0] = pares[i].1;
        i += 1;
    }
    v
}

/// A criatura de um grau (1..=5).
pub fn especie(grau: u8) -> Option<&'static Especie> {
    ESPECIES.get(grau.clamp(1, GRAU_MAX) as usize - 1)
}

/// (criatura, grau) de um id de pet.
pub fn de_item(item_id: u16) -> Option<(&'static Especie, u8)> {
    let (_, grau) = item_id::pet_de_id(item_id)?;
    Some((especie(grau)?, grau))
}

/// O nome JA' diz o grau — a criatura E' o grau —, entao nao se cola a cor
/// atras dele como quando eram cinco especies tingidas de cinco jeitos.
pub fn nome_do_item(id: u16) -> Option<String> {
    Some(de_item(id)?.0.nome.to_string())
}

pub fn nome_do_grau(grau: u8) -> &'static str {
    match grau {
        1 => "Cinza",
        2 => "Verde",
        3 => "Azul",
        4 => "Roxo",
        _ => "Laranja",
    }
}

// ─────────────────────────── o que o grau muda ───────────────────────────

/// Velocidade do pet com as skills somadas.
pub fn velocidade_com(grau: u8, d: &crate::items::PetData) -> f32 {
    let extra: f32 = soma_efeito(d, |ef| match ef {
        Efeito::VelocidadeExtra(v) => Some(v),
        _ => None,
    })
    .iter()
    .sum();
    velocidade(grau) + extra
}

/// Raio de busca com as skills somadas. O teto continua sendo a AOI: fora
/// dela o pet sumiria da tela de quem olha.
pub fn raio_com(grau: u8, d: &crate::items::PetData) -> f32 {
    let extra: f32 = soma_efeito(d, |ef| match ef {
        Efeito::RaioExtra(v) => Some(v),
        _ => None,
    })
    .iter()
    .sum();
    (raio_de_busca(grau) + extra).min(crate::AOI_RADIUS - 2.0)
}

/// Quanto tempo a Ração alimenta este pet, com as skills.
pub fn duracao_da_racao(d: &crate::items::PetData) -> i64 {
    if skills_ativas(d)
        .iter()
        .any(|s| s.efeito == Efeito::RacaoDobrada)
    {
        RACAO_SEGUNDOS * 2
    } else {
        RACAO_SEGUNDOS
    }
}

/// A fatia da XP do jogador que chega no pet, com as skills.
pub fn fatia_da_xp(d: &crate::items::PetData) -> f32 {
    let extra: f32 = soma_efeito(d, |ef| match ef {
        Efeito::XpExtra(v) => Some(v),
        _ => None,
    })
    .iter()
    .sum();
    FATIA_DA_XP * (1.0 + extra)
}

/// Esta' alimentado em `agora_unix`?
pub fn alimentado(d: &crate::items::PetData, agora_unix: i64) -> bool {
    d.alimentado_ate > agora_unix
}

/// Velocidade do pet, em multiplos de `PLAYER_SPEED`. O laranja empata com a
/// montaria (`loja::VEL_MONTADO`): so' o topo acompanha quem esta' montado.
///
/// A ladder SUBIU junto com a das montarias em 22/09/2026. Ela nao e'
/// independente: o teto do pet e' `VEL_MONTADO` por contrato, e deixar o pet
/// para tras quando a montaria acelerou seria trocar um defeito por outro —
/// o jogador montado veria o pet laranja, o mais caro do jogo, ficando
/// comendo poeira.
pub fn velocidade(grau: u8) -> f32 {
    match grau {
        1 => 1.05,
        2 => 1.25,
        3 => 1.45,
        4 => 1.62,
        _ => 1.80,
    }
}

/// Quao longe do DONO o pet aceita ir buscar, em tiles. Fica abaixo do
/// `AOI_RADIUS` de propósito: fora da AOI ele sumiria da tela de quem olha.
pub fn raio_de_busca(grau: u8) -> f32 {
    match grau {
        1 => 8.0,
        2 => 10.0,
        3 => 12.0,
        4 => 14.0,
        _ => 16.0,
    }
}

/// Mais longe que isto do dono, o pet larga o alvo e volta andando.
pub const COLEIRA: f32 = 20.0;
/// Mais longe que ISTO, ninguem andou: o dono teleportou (portal, viagem,
/// pergaminho, dungeon). O pet reaparece do lado dele em vez de atravessar o
/// mapa a pe'.
pub const TELEPORTE: f32 = 40.0;
/// Encostou a esta distancia do saque, coletou.
pub const ALCANCE_DA_COLETA: f32 = 0.6;
/// Distancia em que o pet orbita o dono quando nao tem o que fazer.
pub const DISTANCIA_DE_SEGUIR: f32 = 2.0;
/// Desistiu de um saque (bolsa cheia): nao tenta de novo por isto.
pub const DESISTENCIA_S: f32 = 5.0;

// ─────────────────────────── nivel, fome e skills ──────────────────────────

/// Teto do nivel do pet. Nele o terceiro (e ultimo) slot de skill abre.
pub const NIVEL_MAX: u8 = 30;

/// Fatia da XP do jogador que vai pro pet equipado — e SO' se ele estiver
/// alimentado (docs/PETS.md). Com fome, nao entra nada.
pub const FATIA_DA_XP: f32 = 0.20;

/// XP acumulada pra ESTAR no nivel `n`. Quadratica: subir cedo e' rapido e o
/// fim custa, que e' o que faz a Racao valer a pena o tempo todo.
pub fn xp_para_nivel(n: u8) -> u64 {
    let n = n.clamp(1, NIVEL_MAX) as u64 - 1;
    50 * n * n
}

/// O nivel de quem tem `xp`.
pub fn nivel_de_xp(xp: u64) -> u8 {
    let mut n = 1u8;
    while n < NIVEL_MAX && xp >= xp_para_nivel(n + 1) {
        n += 1;
    }
    n
}

/// Quantos slots de skill o nivel libera: o primeiro no 10, o segundo no 20,
/// o terceiro no 30.
pub fn slots_de_skill(nivel: u8) -> usize {
    match nivel {
        0..=9 => 0,
        10..=19 => 1,
        20..=29 => 2,
        _ => 3,
    }
}

/// Em que nivel o slot `i` (0..3) abre.
pub fn nivel_do_slot(i: usize) -> u8 {
    10 * (i as u8 + 1)
}

/// Quanto tempo uma Ração alimenta, em segundos.
pub const RACAO_SEGUNDOS: i64 = 2 * 60 * 60;

/// O que uma skill de pet faz. Nada de combate: o pet nao luta
/// (docs/PETS.md), entao toda skill e' passiva e mexe no que ele JA' faz.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Efeito {
    /// Soma tiles no raio de busca.
    RaioExtra(f32),
    /// Soma na velocidade, em multiplos de `PLAYER_SPEED`.
    VelocidadeExtra(f32),
    /// A Ração dura o dobro.
    RacaoDobrada,
    /// Soma na fatia da XP que o pet recebe.
    XpExtra(f32),
    /// Soma pontos de atributo, repartidos pela afinidade da especie.
    PontosExtra(u32),
    /// Soma pontos num atributo ESCOLHIDO (indice de `stat_idx`). E' como se
    /// consertar o que a especie nao da': corujinha com VIT, ursinho com DES.
    PontoEm(usize, u32),
    /// Soma vida por segundo no regen base do dono.
    RegenDeVida(f32),
    /// Soma mana por segundo no regen base do dono.
    RegenDeMana(f32),
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SkillDePet {
    pub item_id: u16,
    pub nome: &'static str,
    pub descricao: &'static str,
    pub efeito: Efeito,
    pub preco_tp: u64,
}

pub const SKILLS: [SkillDePet; 7] = [
    SkillDePet {
        item_id: item_id::SKILL_PET_FARO,
        nome: "Faro Apurado",
        descricao: "+3 tiles no raio de busca.",
        efeito: Efeito::RaioExtra(3.0),
        preco_tp: 300,
    },
    SkillDePet {
        item_id: item_id::SKILL_PET_PASSO,
        nome: "Passo Leve",
        descricao: "+20% de velocidade.",
        efeito: Efeito::VelocidadeExtra(0.2),
        preco_tp: 300,
    },
    SkillDePet {
        item_id: item_id::SKILL_PET_ESTOMAGO,
        nome: "Estômago Fundo",
        descricao: "A Ração dura o dobro.",
        efeito: Efeito::RacaoDobrada,
        preco_tp: 250,
    },
    SkillDePet {
        item_id: item_id::SKILL_PET_APRENDIZ,
        nome: "Aprendiz",
        descricao: "+50% da experiência que o pet recebe.",
        efeito: Efeito::XpExtra(0.5),
        preco_tp: 400,
    },
    SkillDePet {
        item_id: item_id::SKILL_PET_VIGOR,
        nome: "Vigor Emprestado",
        descricao: "+3 pontos de atributo, na afinidade da espécie.",
        efeito: Efeito::PontosExtra(3),
        preco_tp: 500,
    },
    SkillDePet {
        item_id: item_id::SKILL_PET_REGEN_VIDA,
        nome: "Sopro Curativo",
        descricao: "+1,5 de vida por segundo.",
        efeito: Efeito::RegenDeVida(1.5),
        preco_tp: 450,
    },
    SkillDePet {
        item_id: item_id::SKILL_PET_REGEN_MANA,
        nome: "Fonte Interior",
        descricao: "+2 de mana por segundo.",
        efeito: Efeito::RegenDeMana(2.0),
        preco_tp: 450,
    },
];

/// Quantos pontos uma skill de atributo da'.
pub const PONTOS_DA_SKILL_DE_ATRIBUTO: u32 = 4;

/// Nome e sigla de cada atributo, na ordem de `stat_idx`.
const ATRIBUTOS: [(&str, &str); STAT_COUNT] = [
    ("Força Emprestada", "FOR"),
    ("Destreza Emprestada", "DES"),
    ("Sabedoria Emprestada", "INT"),
    ("Vitalidade Emprestada", "VIT"),
    ("Ligeireza Emprestada", "SPD"),
    ("Resistência Emprestada", "RES"),
];

/// O catalogo inteiro: as fixas acima e uma por atributo.
pub fn todas_as_skills() -> Vec<SkillDePet> {
    let mut v = SKILLS.to_vec();
    for (i, (nome, sigla)) in ATRIBUTOS.iter().enumerate() {
        v.push(SkillDePet {
            item_id: item_id::SKILL_PET_ATRIBUTO[i],
            nome,
            // O `descricao` e' `&'static str`: a sigla ja' esta' no nome, e o
            // numero e' o mesmo pra todas.
            descricao: match sigla {
                &"FOR" => "+4 de FOR.",
                &"DES" => "+4 de DES.",
                &"INT" => "+4 de INT.",
                &"VIT" => "+4 de VIT.",
                &"SPD" => "+4 de SPD.",
                _ => "+4 de RES.",
            },
            efeito: Efeito::PontoEm(i, PONTOS_DA_SKILL_DE_ATRIBUTO),
            preco_tp: 450,
        });
    }
    v
}

pub fn skill(item_id: u16) -> Option<SkillDePet> {
    todas_as_skills().into_iter().find(|s| s.item_id == item_id)
}

/// O estado do pet, com o default de quem ainda nao tem instancia.
pub fn dados(inst: Option<&crate::items::ItemInstance>) -> crate::items::PetData {
    inst.and_then(|i| i.pet).unwrap_or_default()
}

/// As skills instaladas que o NIVEL de fato libera. Slot travado nao vale:
/// assim, um pet que perdeu nivel (nao acontece hoje) ou uma instancia
/// adulterada nao rendem skill de graca.
pub fn skills_ativas(d: &crate::items::PetData) -> Vec<SkillDePet> {
    let n = slots_de_skill(nivel_de_xp(d.xp));
    (0..n)
        .filter_map(|i| d.skills.get(i).copied().and_then(skill))
        .collect()
}

/// Vida por segundo que as skills do pet somam no dono.
pub fn regen_de_vida(d: &crate::items::PetData) -> f32 {
    soma_efeito(d, |ef| match ef {
        Efeito::RegenDeVida(v) => Some(v),
        _ => None,
    })
    .iter()
    .sum()
}

/// Mana por segundo que as skills do pet somam no dono.
pub fn regen_de_mana(d: &crate::items::PetData) -> f32 {
    soma_efeito(d, |ef| match ef {
        Efeito::RegenDeMana(v) => Some(v),
        _ => None,
    })
    .iter()
    .sum()
}

fn soma_efeito<T: Copy>(d: &crate::items::PetData, f: impl Fn(Efeito) -> Option<T>) -> Vec<T> {
    skills_ativas(d).iter().filter_map(|s| f(s.efeito)).collect()
}

/// Quantos pontos de atributo o pet do grau `grau` entrega no nivel `nivel`.
/// A base e' a curva de cor que os itens ja' usam (`items::tier_stat_mult`),
/// em 18 pontos; o nivel dobra isso do 1 ao 30.
pub fn pontos(grau: u8, nivel: u8) -> u32 {
    let base = crate::items::tier_stat_mult(grau) * 18.0;
    let n = nivel.clamp(1, NIVEL_MAX) as f32;
    (base * (1.0 + (n - 1.0) / (NIVEL_MAX as f32 - 1.0))).round() as u32
}

/// Tier e refino amplificam os pontos naturais e as skills do pet.
pub fn pontos_por_stat_da_instancia(item_id: u16, inst: Option<&crate::items::ItemInstance>) -> [u32; STAT_COUNT] {
    let d = dados(inst);
    let af = inst.and_then(|i| i.afinidade);
    let mult = inst.map_or(1.0, |i| 1.0 + 0.18 * (i.tier().saturating_sub(1)) as f32 + 0.05 * i.refinement as f32);
    pontos_por_stat(item_id, &d, af).map(|p| (p as f32 * mult).round() as u32)
}

/// Multiplicador da velocidade de coleta produzido pela evolução do pet.
pub fn mult_coleta(inst: Option<&crate::items::ItemInstance>) -> f32 {
    inst.map_or(1.0, |i| 1.0 + 0.10 * (i.tier().saturating_sub(1)) as f32 + 0.025 * i.refinement as f32)
}

/// Os pontos do pet, ja' repartidos pelos seis atributos. A sobra da divisao
/// vai pro atributo de maior peso — o total bate com `pontos` sempre.
/// `afinidade` e' a ROLADA da instancia. `None` = bicho de antes do sorteio,
/// que usa a fixa da criatura — ninguem perde o que tinha.
pub fn pontos_por_stat(
    item_id: u16,
    d: &crate::items::PetData,
    afinidade: Option<[u8; 2]>,
) -> [u32; STAT_COUNT] {
    let Some((e, grau)) = de_item(item_id) else {
        return [0; STAT_COUNT];
    };
    let afin = afinidade.map_or(e.afinidade, pesos_de);
    let extra: u32 = soma_efeito(d, |ef| match ef {
        Efeito::PontosExtra(n) => Some(n),
        _ => None,
    })
    .iter()
    .sum();
    let total = pontos(grau, nivel_de_xp(d.xp)) + extra;
    // Os pontos de atributo ESCOLHIDO nao passam pela afinidade: eles vao
    // direto no stat da skill, somados depois do reparto.
    let escolhidos = soma_efeito(d, |ef| match ef {
        Efeito::PontoEm(i, n) => Some((i, n)),
        _ => None,
    });
    let mut v = [0u32; STAT_COUNT];
    for (i, &peso) in afin.iter().enumerate() {
        v[i] = total * peso as u32 / PESO_TOTAL as u32;
    }
    let dado: u32 = v.iter().sum();
    if dado < total {
        // O maior peso fica com a sobra.
        let maior = afin
            .iter()
            .enumerate()
            .max_by_key(|(_, p)| **p)
            .map_or(0, |(i, _)| i);
        v[maior] += total - dado;
    }
    for (i, n) in escolhidos {
        if let Some(x) = v.get_mut(i) {
            *x += n;
        }
    }
    v
}

// ─────────────────────────── pergaminho e combinacao ───────────────────────

/// Chance de cada grau no Pergaminho de Invocação: Pet, em pontos
/// percentuais. Mesma forma da tabela de chaves (`loja::BauCraft`), esticada
/// pro quinto grau.
pub const CHANCES_DO_PERGAMINHO: [u8; GRAU_MAX as usize] = [55, 28, 12, 4, 1];

/// Sorteia (base, grau) do pergaminho. `_r_especie` sobrou do tempo em que
/// especie e grau eram sorteios separados: agora a criatura E' o grau, entao
/// so' o segundo dado decide. Mantido na assinatura pra nao mexer em quem
/// chama.
pub fn rolar(_r_especie: f32, r_grau: f32) -> (u16, u8) {
    let alvo = (r_grau.clamp(0.0, 0.999_999) * 100.0) as u16;
    let mut soma = 0u16;
    let mut grau = GRAU_MAX;
    for (k, chance) in CHANCES_DO_PERGAMINHO.iter().enumerate() {
        soma += *chance as u16;
        if alvo < soma {
            grau = k as u8 + 1;
            break;
        }
    }
    (item_id::PET_BASE, grau)
}

/// Pets consumidos por tentativa de combinar.
pub const PETS_POR_TENTATIVA: u32 = 3;

/// Chance de o pet SUBIR pro grau `destino` (2 verde .. 5 laranja), em %.
/// E' aposta, como a chave: falhar consome os tres.
pub const fn chance_de_combinar(destino: u8) -> u8 {
    match destino {
        2 => 60,
        3 => 40,
        4 => 25,
        _ => 10,
    }
}

/// Cobre cobrado por tentativa, pelo grau de ENTRADA.
pub const fn cobre_de_combinar(entrada: u8) -> u32 {
    match entrada {
        1 => 2_000,
        2 => 8_000,
        3 => 30_000,
        _ => 120_000,
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn cada_especie_reparte_todos_os_pontos_do_grau() {
        for e in ESPECIES {
            assert_eq!(
                e.afinidade.iter().map(|p| *p as u16).sum::<u16>(),
                PESO_TOTAL as u16,
                "{}: os pesos tem que somar {PESO_TOTAL}",
                e.nome
            );
            let novo = crate::items::PetData::default();
            let id = item_id::pet_no_grau(item_id::PET_BASE, e.grau);
            let v = pontos_por_stat(id, &novo, None);
            assert_eq!(
                v.iter().sum::<u32>(),
                pontos(e.grau, 1),
                "{}: a soma tem que bater com o total",
                e.nome
            );
            // O atributo de maior peso nunca fica sem nada.
            let maior = e
                .afinidade
                .iter()
                .enumerate()
                .max_by_key(|(_, p)| **p)
                .map(|(i, _)| i)
                .unwrap();
            assert!(v[maior] > 0);
        }
        assert_eq!(pontos(1, 1), 11);
        assert_eq!(pontos(2, 1), 16);
        assert_eq!(pontos(3, 1), 22);
        assert_eq!(pontos(4, 1), 27);
        assert_eq!(pontos(5, 1), 34);
        // O nivel maximo dobra o que o grau da' (a menos do arredondamento).
        for grau in 1..=GRAU_MAX {
            let dobro = pontos(grau, 1) * 2;
            let topo = pontos(grau, NIVEL_MAX);
            assert!(
                topo.abs_diff(dobro) <= 1,
                "grau {grau}: {topo} nao e' o dobro de {}",
                pontos(grau, 1)
            );
            // E o nivel nunca tira nada de ninguem.
            for n in 1..NIVEL_MAX {
                assert!(pontos(grau, n + 1) >= pontos(grau, n));
            }
        }
        assert_eq!(
            pontos_por_stat(item_id::GOLD, &crate::items::PetData::default(), None),
            [0; STAT_COUNT]
        );
    }

    /// A afinidade ROLADA manda no reparto, e os dois atributos sao sempre
    /// DIFERENTES — senao o "principal e secundario" viravam um so'.
    #[test]
    fn a_afinidade_rolada_manda_no_reparto() {
        // Cobre os dois dados de ponta a ponta: todo par sai valido.
        let mut vistos = std::collections::HashSet::new();
        for a in 0..40 {
            for b in 0..40 {
                let r = rolar_afinidade(a as f32 / 40.0, b as f32 / 40.0);
                assert!(
                    (r[0] as usize) < STAT_COUNT && (r[1] as usize) < STAT_COUNT,
                    "{r:?} fora da tabela"
                );
                assert_ne!(r[0], r[1], "principal e secundario iguais: {r:?}");
                let p = pesos_de(r);
                assert_eq!(
                    p.iter().map(|x| *x as u16).sum::<u16>(),
                    PESO_TOTAL as u16,
                    "os pesos de {r:?} nao somam {PESO_TOTAL}"
                );
                assert_eq!(p[r[0] as usize], PESO_PRINCIPAL);
                assert_eq!(p[r[1] as usize], PESO_SECUNDARIO);
                vistos.insert(r);
            }
        }
        // Todos os pares ordenados possiveis saem — o sorteio nao esquece
        // nenhum atributo.
        assert_eq!(vistos.len(), STAT_COUNT * (STAT_COUNT - 1));

        // E o reparto segue a ROLADA, nao a da criatura.
        let id = item_id::pet_no_grau(item_id::PET_BASE, 5);
        let d = crate::items::PetData::default();
        let fixa = pontos_por_stat(id, &d, None);
        let rolada = pontos_por_stat(id, &d, Some([stat_idx::RES as u8, stat_idx::SPD as u8]));
        assert_eq!(
            rolada.iter().sum::<u32>(),
            fixa.iter().sum::<u32>(),
            "o TOTAL nao muda: a afinidade so' diz ONDE cai"
        );
        assert!(rolada[stat_idx::RES] > rolada[stat_idx::SPD]);
        assert!(
            rolada[stat_idx::RES] > 0 && rolada[stat_idx::SPD] > 0,
            "os dois atributos rolados recebem algo"
        );
        for (i, v) in rolada.iter().enumerate() {
            if i != stat_idx::RES && i != stat_idx::SPD {
                assert_eq!(*v, 0, "caiu ponto fora da afinidade rolada");
            }
        }
    }

    /// O id carrega o GRAU, e o grau E' a criatura: cinco ids, cinco bichos
    /// diferentes. Subir de cor e' trocar de bicho.
    #[test]
    fn o_id_carrega_a_criatura_e_o_grau() {
        let mut bichos = std::collections::HashSet::new();
        for grau in 1..=GRAU_MAX {
            let id = item_id::pet_no_grau(item_id::PET_BASE, grau);
            assert_eq!(item_id::pet_de_id(id), Some((item_id::PET_BASE, grau)));
            let (c, g) = de_item(id).expect("id de pet");
            assert_eq!(g, grau);
            assert_eq!(c.grau, grau, "a posicao no catalogo E' o grau");
            assert!(
                bichos.insert(c.bicho),
                "grau {grau} repete o bicho {}: a cor tem que ser outra CRIATURA",
                c.bicho
            );
            assert_eq!(
                crate::equip_slot_of(id),
                Some(crate::EquipSlot::Pet),
                "pet tem que equipar no slot do pet"
            );
        }
        assert_eq!(bichos.len(), GRAU_MAX as usize);
        assert_eq!(item_id::pet_de_id(item_id::PET_BASE - 1), None);
        assert_eq!(item_id::pet_de_id(item_id::PET_ULTIMO + 1), None);
    }

    #[test]
    fn o_grau_manda_na_velocidade_e_no_raio() {
        for grau in 1..GRAU_MAX {
            assert!(velocidade(grau + 1) > velocidade(grau));
            assert!(raio_de_busca(grau + 1) > raio_de_busca(grau));
            assert!(pontos(grau + 1, 1) > pontos(grau, 1));
        }
        // Nenhum grau pode buscar fora da AOI: fora dela o pet some da tela.
        assert!(raio_de_busca(GRAU_MAX) < crate::AOI_RADIUS);
        assert!(COLEIRA < crate::AOI_RADIUS);
        // O laranja acompanha quem esta' montado; o resto nao.
        assert_eq!(velocidade(GRAU_MAX), crate::loja::VEL_MONTADO);
        assert!(velocidade(GRAU_MAX - 1) < crate::loja::VEL_MONTADO);
    }

    #[test]
    fn o_pergaminho_sorteia_cem_por_cento_e_o_cinza_e_o_mais_comum() {
        assert_eq!(
            CHANCES_DO_PERGAMINHO.iter().map(|c| *c as u16).sum::<u16>(),
            100
        );
        for k in 1..CHANCES_DO_PERGAMINHO.len() {
            assert!(
                CHANCES_DO_PERGAMINHO[k] < CHANCES_DO_PERGAMINHO[k - 1],
                "grau melhor nao pode ser mais comum"
            );
        }
        // As bordas do sorteio: 0 cai no primeiro grau, quase 1 no ultimo.
        assert_eq!(rolar(0.0, 0.0), (item_id::PET_BASE, 1));
        assert_eq!(rolar(0.999, 0.999).1, GRAU_MAX);
        // Toda especie e' alcancavel e nada estoura o catalogo.
        for k in 0..ESPECIE_COUNT {
            let r = (k as f32 + 0.5) / ESPECIE_COUNT as f32;
            // A criatura E' o grau: o primeiro dado nao muda mais nada.
            assert_eq!(rolar(r, 0.0), (item_id::PET_BASE, 1));
        }
    }

    #[test]
    fn o_nivel_sobe_e_abre_um_slot_a_cada_dez() {
        assert_eq!(nivel_de_xp(0), 1);
        assert_eq!(xp_para_nivel(1), 0);
        for n in 1..NIVEL_MAX {
            assert!(xp_para_nivel(n + 1) > xp_para_nivel(n), "nivel {n}");
            assert_eq!(nivel_de_xp(xp_para_nivel(n + 1)), n + 1);
            assert_eq!(nivel_de_xp(xp_para_nivel(n + 1) - 1), n);
        }
        // XP alem do teto nao passa do nivel maximo.
        assert_eq!(nivel_de_xp(u64::MAX), NIVEL_MAX);

        assert_eq!(slots_de_skill(1), 0);
        assert_eq!(slots_de_skill(9), 0);
        assert_eq!(slots_de_skill(10), 1);
        assert_eq!(slots_de_skill(19), 1);
        assert_eq!(slots_de_skill(20), 2);
        assert_eq!(slots_de_skill(NIVEL_MAX), 3);
        for i in 0..3 {
            assert_eq!(slots_de_skill(nivel_do_slot(i)), i + 1);
            assert_eq!(slots_de_skill(nivel_do_slot(i) - 1), i);
        }
    }

    #[test]
    fn skill_so_vale_no_slot_que_o_nivel_abriu() {
        use crate::items::PetData;
        let tres = [
            item_id::SKILL_PET_FARO,
            item_id::SKILL_PET_PASSO,
            item_id::SKILL_PET_VIGOR,
        ];
        // Nivel 1: os tres estao guardados, nenhum vale.
        let cru = PetData {
            xp: 0,
            alimentado_ate: 0,
            skills: tres,
        };
        assert_eq!(skills_ativas(&cru).len(), 0);
        assert_eq!(raio_com(3, &cru), raio_de_busca(3));
        assert_eq!(velocidade_com(3, &cru), velocidade(3));

        // Nivel 10: so' o primeiro.
        let dez = PetData {
            xp: xp_para_nivel(10),
            ..cru
        };
        assert_eq!(skills_ativas(&dez).len(), 1);
        assert_eq!(raio_com(3, &dez), raio_de_busca(3) + 3.0);
        assert_eq!(velocidade_com(3, &dez), velocidade(3));

        // Nivel 30: os tres.
        let trinta = PetData {
            xp: xp_para_nivel(NIVEL_MAX),
            ..cru
        };
        assert_eq!(skills_ativas(&trinta).len(), 3);
        assert!((velocidade_com(3, &trinta) - (velocidade(3) + 0.2)).abs() < 1e-6);
        assert_eq!(
            pontos_por_stat(item_id::pet_no_grau(item_id::PET_BASE, 3), &trinta, None)
                .iter()
                .sum::<u32>(),
            pontos(3, NIVEL_MAX) + 3,
            "Vigor Emprestado soma 3 pontos em cima do nivel"
        );
        // O raio nunca sai da AOI, por mais skill que tenha.
        assert!(raio_com(GRAU_MAX, &trinta) < crate::AOI_RADIUS);
    }

    #[test]
    fn com_fome_o_pet_nao_recebe_nada() {
        use crate::items::PetData;
        let d = PetData::default();
        assert!(!alimentado(&d, 1_000), "sem racao, com fome");
        let cheio = PetData {
            alimentado_ate: 5_000,
            ..d
        };
        assert!(alimentado(&cheio, 4_999));
        assert!(!alimentado(&cheio, 5_000), "na virada ja' esta' com fome");
        assert_eq!(duracao_da_racao(&d), RACAO_SEGUNDOS);
        // Estomago Fundo dobra — mas so' com o slot aberto.
        let com_skill = PetData {
            xp: xp_para_nivel(10),
            skills: [item_id::SKILL_PET_ESTOMAGO, 0, 0],
            ..d
        };
        assert_eq!(duracao_da_racao(&com_skill), RACAO_SEGUNDOS * 2);
        assert!(fatia_da_xp(&d) > 0.0);
        let aprendiz = PetData {
            xp: xp_para_nivel(10),
            skills: [item_id::SKILL_PET_APRENDIZ, 0, 0],
            ..d
        };
        assert!(fatia_da_xp(&aprendiz) > fatia_da_xp(&d));
    }

    /// Regen e atributo escolhido: os dois somam DIRETO, sem passar pela
    /// afinidade da especie — e' assim que se conserta o que o bicho nao da'.
    #[test]
    fn regen_e_atributo_escolhido_somam_direto() {
        use crate::items::PetData;
        let vazio = PetData::default();
        assert_eq!(regen_de_vida(&vazio), 0.0);
        assert_eq!(regen_de_mana(&vazio), 0.0);

        let d = PetData {
            xp: xp_para_nivel(NIVEL_MAX),
            alimentado_ate: 0,
            skills: [
                item_id::SKILL_PET_REGEN_VIDA,
                item_id::SKILL_PET_REGEN_MANA,
                // Corujinha e' INT/VIT; esta da' DES, que ela nao tem.
                item_id::SKILL_PET_ATRIBUTO[stat_idx::DES],
            ],
        };
        assert!(regen_de_vida(&d) > 0.0);
        assert!(regen_de_mana(&d) > 0.0);

        let coruja = item_id::pet_no_grau(item_id::PET_BASE, 1);
        let sem = pontos_por_stat(coruja, &vazio, None);
        let com = pontos_por_stat(coruja, &d, None);
        assert_eq!(sem[stat_idx::DES], 0, "corujinha nao da' DES sozinha");
        assert_eq!(com[stat_idx::DES], PONTOS_DA_SKILL_DE_ATRIBUTO);
        // O resto do reparto so' cresce pelo NIVEL, nao pela skill escolhida.
        assert_eq!(
            com[stat_idx::INT],
            pontos_por_stat(coruja, &PetData { skills: [0; 3], ..d }, None)[stat_idx::INT]
        );
    }

    #[test]
    fn toda_skill_tem_item_proprio_e_preco() {
        let mut ids = std::collections::HashSet::new();
        for s in todas_as_skills() {
            assert!(ids.insert(s.item_id), "id repetido em {}", s.nome);
            assert!(item_id::e_skill_de_pet(s.item_id), "{} fora da faixa", s.nome);
            assert!(s.preco_tp > 0);
            assert_eq!(skill(s.item_id).map(|x| x.nome), Some(s.nome));
        }
        // Uma por atributo, e nenhuma fora da faixa.
        assert_eq!(
            todas_as_skills().len(),
            SKILLS.len() + STAT_COUNT,
            "cada atributo tem a skill dele"
        );
        assert!(!item_id::e_skill_de_pet(item_id::PET_ULTIMO));
        assert!(!item_id::e_skill_de_pet(item_id::RACAO_DE_PET));
        assert_eq!(skill(item_id::GOLD), None);
    }

    #[test]
    fn combinar_e_aposta_e_fica_pior_a_cada_degrau() {
        for destino in 3..=GRAU_MAX {
            assert!(chance_de_combinar(destino) < chance_de_combinar(destino - 1));
            assert!(cobre_de_combinar(destino - 1) > cobre_de_combinar(destino - 2));
        }
        assert!(chance_de_combinar(GRAU_MAX) > 0, "o topo tem que ser possivel");
        assert!(chance_de_combinar(2) < 100, "nenhum degrau e' garantido");
    }
}
