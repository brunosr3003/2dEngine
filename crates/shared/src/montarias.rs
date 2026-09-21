//! Montarias (docs/MONTARIAS.md).
//!
//! Mesma forma do pet (docs/PETS.md): a montaria e' **item de bolsa**, o grau
//! (a cor) mora no `item_id`, e por isso ela e' negociavel no mercado e
//! combinavel na aba Combinar do Craft. Equipada no slot `EquipSlot::Montaria`,
//! e' nela que o jogador monta.
//!
//! **Nao ha' nivel** — de proposito. O pet tem, porque ele trabalha; a
//! montaria so' leva voce de um lado pro outro.
//!
//! **A COR E' A CRIATURA.** Nao ha' especie separada do grau: cinza e' um
//! cervo, verde um lobo, azul um tigre, roxo um hipogrifo e laranja um
//! dragao. Antes eram tres especies tingidas de cinco cores, e o dono resumiu
//! o problema em 20/09/2026: "sao os mesmos, so' muda a cor; eu quero que
//! tenham realmente mounts diferentes para cada cor". Subir de grau agora e'
//! trocar de bicho, e e' isso que faz combinar valer a pena.
//!
//! Cada criatura tem a sua afinidade de atributo, entao a cor decide as duas
//! coisas: a velocidade E o que ela empresta.

use crate::constants::{item_id, stat_idx, STAT_COUNT};

/// Quantas criaturas — uma por grau.
pub const ESPECIE_COUNT: usize = 5;
/// Ultimo grau (1 cinza .. 5 laranja).
pub const GRAU_MAX: u8 = 5;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Especie {
    /// O grau desta criatura (1 cinza .. 5 laranja). E' tambem a posicao
    /// dela em `ESPECIES`.
    pub grau: u8,
    pub nome: &'static str,
    /// Arquivo do bicho em pecas (`client::bicho::BICHOS`).
    pub bicho: &'static str,
    /// Escala sobre a altura em que o bicho e' carregado.
    pub escala: f32,
    /// Quanto a sela fica pra frente do centro do bicho.
    pub sela_frente: f32,
    /// Peso de cada atributo, somando `PESO_TOTAL`.
    pub afinidade: [u8; STAT_COUNT],
    pub descricao: &'static str,
}

/// Os pesos de `afinidade` somam isto.
pub const PESO_TOTAL: u8 = 10;

/// A escada: do bicho de carga ao dragao. A ordem E' o grau.
///
/// **O porte tem um piso: o jogador.** A `escala` multiplica a altura do
/// bicho ADULTO (`client::bicho::BICHOS`).
///
/// O cervo saiu com 1,28 na primeira versao, depois 1,46, e o dono continuou
/// vendo o que o numero dizia: *"o veado ainda ta pequeno em relacao ao
/// player"*. Estava — 1,46 contra os 1,8 do boneco. O teste passava porque o
/// piso dele era 75% da altura do jogador, e 75% e' exatamente a licenca pra
/// montaria ser mais baixa que quem monta.
///
/// Agora o piso e' o jogador INTEIRO, e a escada vai de 1,95 (cervo) a 2,80
/// (dragao), sempre subindo. Onde o cavaleiro SENTA nao esta' aqui: sai do
/// tronco do proprio modelo (`client::vox::lombo_medido`), porque um numero
/// por especie escrito a mao nao acompanha a escala quando ela muda — e foi
/// exatamente isso que deixou o cavaleiro do cervo boiando.
pub const ESPECIES: [Especie; ESPECIE_COUNT] = [
    Especie {
        grau: 1,
        nome: "Cervo do Bosque",
        bicho: "bichos/cervo",
        escala: 1.22,
        sela_frente: -0.34,
        afinidade: pesos(&[(stat_idx::SPD, 6), (stat_idx::DES, 4)]),
        descricao: "Manso e ligeiro. A primeira montaria de qualquer um.",
    },
    Especie {
        grau: 2,
        nome: "Lobo da Clareira",
        bicho: "bichos/lobo",
        escala: 0.75,
        sela_frente: -0.38,
        afinidade: pesos(&[(stat_idx::DES, 6), (stat_idx::SPD, 4)]),
        descricao: "Leal e ligeiro, criado nas matas do Bosque.",
    },
    Especie {
        grau: 3,
        nome: "Tigre das Neves",
        bicho: "bichos/tigre",
        escala: 2.4,
        sela_frente: -0.38,
        afinidade: pesos(&[(stat_idx::FOR, 6), (stat_idx::DES, 4)]),
        descricao: "Silencioso na neve, feroz na estrada.",
    },
    Especie {
        grau: 4,
        nome: "Hipogrifo",
        bicho: "bichos/hipogrifo",
        escala: 1.32,
        sela_frente: -0.30,
        afinidade: pesos(&[(stat_idx::INT, 5), (stat_idx::SPD, 5)]),
        descricao: "Meio águia, meio cavalo. Não anda: quase voa.",
    },
    Especie {
        grau: 5,
        nome: "Dragão",
        bicho: "bichos/dragao",
        escala: 1.17,
        sela_frente: -0.26,
        afinidade: pesos(&[(stat_idx::FOR, 4), (stat_idx::VIT, 3), (stat_idx::INT, 3)]),
        descricao: "O topo. Quem monta um, todo mundo vê de longe.",
    },
];

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

/// (criatura, grau) de um id de montaria.
pub fn de_item(item_id: u16) -> Option<(&'static Especie, u8)> {
    let (_, grau) = item_id::montaria_de_id(item_id)?;
    Some((especie(grau)?, grau))
}

/// "Tigre das Neves". O nome JA' diz o grau — a criatura e' o grau —, entao
/// nao se cola a cor atras dele como se fazia quando eram tres especies
/// tingidas de cinco jeitos.
pub fn nome_do_item(id: u16) -> Option<String> {
    Some(de_item(id)?.0.nome.to_string())
}

// ─────────────────────────── o que a cor muda ───────────────────────────

/// Velocidade montado, em multiplos da velocidade a pe'. O CINZA e' o que a
/// montaria unica valia antes desta mudanca (`VEL_MONTADO`), entao ninguem
/// ficou mais lento do que ja' estava; a cor so' sobe daí.
pub fn velocidade(grau: u8) -> f32 {
    match grau {
        1 => 1.50,
        2 => 1.60,
        3 => 1.70,
        4 => 1.80,
        _ => 1.90,
    }
}

/// Pontos de atributo que a montaria entrega. Menos que o pet no mesmo grau:
/// o pet trabalha, a montaria so' anda.
pub fn pontos(grau: u8) -> u32 {
    (crate::items::tier_stat_mult(grau) * 6.0).round() as u32
}

/// Os pontos ja' repartidos pelos seis atributos, pela afinidade da especie.
/// A sobra da divisao vai pro atributo de maior peso.
/// `afinidade` e' a ROLADA da instancia (mesma regra do pet, docs/PETS.md).
/// `None` = montaria de antes do sorteio, que usa a fixa da criatura.
pub fn pontos_por_stat(item_id: u16, afinidade: Option<[u8; 2]>) -> [u32; STAT_COUNT] {
    let Some((e, grau)) = de_item(item_id) else {
        return [0; STAT_COUNT];
    };
    let afin = afinidade.map_or(e.afinidade, crate::pets::pesos_de);
    let total = pontos(grau);
    let mut v = [0u32; STAT_COUNT];
    for (i, &peso) in afin.iter().enumerate() {
        v[i] = total * peso as u32 / PESO_TOTAL as u32;
    }
    let dado: u32 = v.iter().sum();
    if dado < total {
        let maior = afin
            .iter()
            .enumerate()
            .max_by_key(|(_, p)| **p)
            .map_or(0, |(i, _)| i);
        v[maior] += total - dado;
    }
    v
}

// ─────────────────────────── pergaminho e combinacao ───────────────────────

/// Chance de cada grau no Pergaminho de Invocação: Montaria. Mesma escada do
/// pergaminho de pet.
pub const CHANCES_DO_PERGAMINHO: [u8; GRAU_MAX as usize] = [55, 28, 12, 4, 1];

/// Sorteia (base, grau). `_r_especie` sobrou do tempo em que especie e grau
/// eram sorteios separados: agora a criatura E' o grau, entao so' o segundo
/// dado decide. Mantido na assinatura pra nao mexer em quem chama.
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
    (item_id::MONTARIA_BASE, grau)
}

/// Montarias consumidas por tentativa de combinar.
pub const MONTARIAS_POR_TENTATIVA: u32 = 3;

/// Chance de SUBIR pro grau `destino` (2 verde .. 5 laranja), em %. Aposta,
/// como a chave e como o pet: falhar consome as tres.
pub const fn chance_de_combinar(destino: u8) -> u8 {
    match destino {
        2 => 60,
        3 => 40,
        4 => 25,
        _ => 10,
    }
}

/// Cobre por tentativa, pelo grau de ENTRADA.
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

    /// A montaria segue a MESMA afinidade rolada do pet: o que a criatura
    /// empresta e' sorte, e a Pedra de Afinidade re-rola (docs/PETS.md).
    #[test]
    fn a_montaria_tambem_usa_a_afinidade_rolada() {
        let id = item_id::montaria_no_grau(item_id::MONTARIA_BASE, 5);
        let fixa = pontos_por_stat(id, None);
        let rolada = pontos_por_stat(id, Some([stat_idx::INT as u8, stat_idx::RES as u8]));
        assert_eq!(
            rolada.iter().sum::<u32>(),
            fixa.iter().sum::<u32>(),
            "o TOTAL nao muda: a afinidade so' diz ONDE cai"
        );
        assert!(rolada[stat_idx::INT] > rolada[stat_idx::RES]);
        for (i, v) in rolada.iter().enumerate() {
            if i != stat_idx::INT && i != stat_idx::RES {
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
            let id = item_id::montaria_no_grau(item_id::MONTARIA_BASE, grau);
            assert_eq!(
                item_id::montaria_de_id(id),
                Some((item_id::MONTARIA_BASE, grau))
            );
            let (c, g) = de_item(id).expect("id de montaria");
            assert_eq!(g, grau);
            assert_eq!(c.grau, grau, "a posicao no catalogo E' o grau");
            assert!(
                bichos.insert(c.bicho),
                "grau {grau} repete o bicho {}: a cor tem que ser outra CRIATURA",
                c.bicho
            );
            assert_eq!(
                crate::equip_slot_of(id),
                Some(crate::EquipSlot::Montaria),
                "montaria tem que equipar no slot dela"
            );
        }
        assert_eq!(bichos.len(), GRAU_MAX as usize);
        assert_eq!(item_id::montaria_de_id(item_id::MONTARIA_BASE - 1), None);
        assert_eq!(item_id::montaria_de_id(item_id::MONTARIA_ULTIMA + 1), None);
        // Nao se confunde com pet: as duas familias sao ids seguidos.
        assert_eq!(item_id::pet_de_id(item_id::MONTARIA_BASE), None);
        assert_eq!(item_id::montaria_de_id(item_id::PET_BASE), None);
    }

    /// O cinza tem que valer o que a montaria unica valia antes, senao a
    /// mudanca deixaria todo mundo mais lento do que ja' estava.
    #[test]
    fn a_cor_so_acelera_a_partir_do_que_existia() {
        assert_eq!(velocidade(1), crate::loja::VEL_MONTADO);
        for grau in 1..GRAU_MAX {
            assert!(velocidade(grau + 1) > velocidade(grau));
            assert!(pontos(grau + 1) > pontos(grau));
        }
        // E montado continua sendo mais rapido que a pe', em todo grau.
        assert!(velocidade(1) > 1.0);
    }

    #[test]
    fn cada_especie_reparte_todos_os_pontos() {
        for e in ESPECIES {
            assert_eq!(
                e.afinidade.iter().map(|p| *p as u16).sum::<u16>(),
                PESO_TOTAL as u16,
                "{}: os pesos tem que somar {PESO_TOTAL}",
                e.nome
            );
            let id = item_id::montaria_no_grau(item_id::MONTARIA_BASE, e.grau);
            assert_eq!(pontos_por_stat(id, None).iter().sum::<u32>(), pontos(e.grau));
        }
        assert_eq!(pontos_por_stat(item_id::GOLD, None), [0; STAT_COUNT]);
        // A montaria da' MENOS que o pet no mesmo grau: ela so' anda.
        for grau in 1..=GRAU_MAX {
            assert!(pontos(grau) < crate::pets::pontos(grau, 1));
        }
    }

    #[test]
    fn o_pergaminho_sorteia_cem_por_cento() {
        assert_eq!(
            CHANCES_DO_PERGAMINHO.iter().map(|c| *c as u16).sum::<u16>(),
            100
        );
        assert_eq!(rolar(0.0, 0.0), (item_id::MONTARIA_BASE, 1));
        assert_eq!(rolar(0.999, 0.999).1, GRAU_MAX);
        // A criatura E' o grau: o primeiro dado nao muda mais nada.
        for k in 0..ESPECIE_COUNT {
            let r = (k as f32 + 0.5) / ESPECIE_COUNT as f32;
            assert_eq!(rolar(r, 0.0), (item_id::MONTARIA_BASE, 1));
        }
    }
}
