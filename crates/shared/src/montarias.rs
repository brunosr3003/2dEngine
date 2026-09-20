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
//! O que a cor da': **velocidade**. O que a especie da': **atributo**. A cor
//! tambem e' a APARENCIA — a tinta do grau substituiu o sistema de skins, que
//! saiu inteiro.

use crate::constants::{item_id, stat_idx, STAT_COUNT};

/// Quantas especies de montaria existem.
pub const ESPECIE_COUNT: usize = 3;
/// Ultimo grau (1 cinza .. 5 laranja).
pub const GRAU_MAX: u8 = 5;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Especie {
    /// id da montaria CINZA desta especie (e a chave do catalogo).
    pub base: u16,
    pub nome: &'static str,
    /// Arquivo do bicho em pecas (`client::bicho::BICHOS`).
    pub bicho: &'static str,
    /// Escala sobre a altura em que o bicho e' carregado.
    pub escala: f32,
    /// Altura da sela, em unidades de mundo (ja' com a escala).
    pub sela: f32,
    /// Quanto a sela fica pra frente do centro do bicho.
    pub sela_frente: f32,
    /// Peso de cada atributo, somando `PESO_TOTAL`.
    pub afinidade: [u8; STAT_COUNT],
    pub descricao: &'static str,
}

/// Os pesos de `afinidade` somam isto.
pub const PESO_TOTAL: u8 = 10;

pub const ESPECIES: [Especie; ESPECIE_COUNT] = [
    Especie {
        base: item_id::MONTARIA_LOBO,
        nome: "Lobo da Clareira",
        bicho: "bichos/lobo",
        escala: 0.56,
        sela: 1.26,
        sela_frente: -0.38,
        afinidade: pesos(&[(stat_idx::SPD, 6), (stat_idx::DES, 4)]),
        descricao: "Leal e ligeiro, criado nas matas do Bosque.",
    },
    Especie {
        base: item_id::MONTARIA_TIGRE,
        nome: "Tigre das Neves",
        bicho: "bichos/tigre",
        escala: 1.6,
        sela: 1.24,
        sela_frente: -0.38,
        afinidade: pesos(&[(stat_idx::FOR, 6), (stat_idx::DES, 4)]),
        descricao: "Silencioso na neve, feroz na estrada.",
    },
    Especie {
        base: item_id::MONTARIA_URSO,
        nome: "Urso de Carga",
        bicho: "bichos/urso",
        escala: 1.3,
        sela: 1.32,
        sela_frente: -0.30,
        afinidade: pesos(&[(stat_idx::VIT, 6), (stat_idx::RES, 4)]),
        descricao: "Devagar no passo, difícil de derrubar.",
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

pub fn especie(base: u16) -> Option<&'static Especie> {
    ESPECIES.iter().find(|e| e.base == base)
}

/// (especie, grau) de um id de montaria.
pub fn de_item(item_id: u16) -> Option<(&'static Especie, u8)> {
    let (base, grau) = item_id::montaria_de_id(item_id)?;
    Some((especie(base)?, grau))
}

/// "Tigre das Neves Azul".
pub fn nome_do_item(id: u16) -> Option<String> {
    let (e, grau) = de_item(id)?;
    Some(format!("{} {}", e.nome, crate::pets::nome_do_grau(grau)))
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
pub fn pontos_por_stat(item_id: u16) -> [u32; STAT_COUNT] {
    let Some((e, grau)) = de_item(item_id) else {
        return [0; STAT_COUNT];
    };
    let total = pontos(grau);
    let mut v = [0u32; STAT_COUNT];
    for (i, &peso) in e.afinidade.iter().enumerate() {
        v[i] = total * peso as u32 / PESO_TOTAL as u32;
    }
    let dado: u32 = v.iter().sum();
    if dado < total {
        let maior = e
            .afinidade
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

/// Sorteia (especie, grau). `r_especie` e `r_grau` em 0..1.
pub fn rolar(r_especie: f32, r_grau: f32) -> (u16, u8) {
    let i = ((r_especie.clamp(0.0, 0.999_999) * ESPECIE_COUNT as f32) as usize)
        .min(ESPECIE_COUNT - 1);
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
    (ESPECIES[i].base, grau)
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

    #[test]
    fn o_id_carrega_a_especie_e_o_grau() {
        for e in ESPECIES {
            for grau in 1..=GRAU_MAX {
                let id = item_id::montaria_no_grau(e.base, grau);
                assert_eq!(item_id::montaria_de_id(id), Some((e.base, grau)));
                assert_eq!(de_item(id).map(|(x, g)| (x.base, g)), Some((e.base, grau)));
                assert_eq!(
                    crate::equip_slot_of(id),
                    Some(crate::EquipSlot::Montaria),
                    "montaria tem que equipar no slot dela"
                );
            }
        }
        assert_eq!(item_id::montaria_de_id(item_id::MONTARIA_LOBO - 1), None);
        assert_eq!(item_id::montaria_de_id(item_id::MONTARIA_ULTIMA + 1), None);
        // Nao se confunde com pet: as duas familias sao ids seguidos.
        assert_eq!(item_id::pet_de_id(item_id::MONTARIA_LOBO), None);
        assert_eq!(item_id::montaria_de_id(item_id::PET_LOBO), None);
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
            for grau in 1..=GRAU_MAX {
                let id = item_id::montaria_no_grau(e.base, grau);
                assert_eq!(pontos_por_stat(id).iter().sum::<u32>(), pontos(grau));
            }
        }
        assert_eq!(pontos_por_stat(item_id::GOLD), [0; STAT_COUNT]);
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
        assert_eq!(rolar(0.0, 0.0), (item_id::MONTARIA_LOBO, 1));
        assert_eq!(rolar(0.999, 0.999).1, GRAU_MAX);
        for k in 0..ESPECIE_COUNT {
            let r = (k as f32 + 0.5) / ESPECIE_COUNT as f32;
            assert_eq!(rolar(r, 0.0).0, ESPECIES[k].base);
        }
    }
}
