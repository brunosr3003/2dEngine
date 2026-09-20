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

/// Quantas especies existem.
pub const ESPECIE_COUNT: usize = 5;
/// Ultimo grau (1 cinza, 2 verde, 3 azul, 4 roxo, 5 laranja).
pub const GRAU_MAX: u8 = 5;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Especie {
    /// id do pet CINZA desta especie (e a chave do catalogo).
    pub base: u16,
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

pub const ESPECIES: [Especie; ESPECIE_COUNT] = [
    Especie {
        base: item_id::PET_LOBO,
        nome: "Lobinho",
        bicho: "bichos/lobo_pequeno",
        escala: 0.55,
        // DES 6, SPD 4 — o pet do ataque rapido.
        afinidade: pesos(&[(stat_idx::DES, 6), (stat_idx::SPD, 4)]),
        descricao: "Rápido e curioso. Destreza e velocidade.",
    },
    Especie {
        base: item_id::PET_URSO,
        nome: "Ursinho",
        bicho: "bichos/urso",
        escala: 0.42,
        afinidade: pesos(&[(stat_idx::VIT, 6), (stat_idx::RES, 4)]),
        descricao: "Pesado e teimoso. Vitalidade e resistência.",
    },
    Especie {
        base: item_id::PET_TIGRE,
        nome: "Filhote de Tigre",
        bicho: "bichos/tigre",
        escala: 0.5,
        afinidade: pesos(&[(stat_idx::FOR, 6), (stat_idx::DES, 4)]),
        descricao: "Caçador nato. Força e destreza.",
    },
    Especie {
        base: item_id::PET_OWLBEAR,
        nome: "Corujinha-urso",
        bicho: "bichos/owlbear",
        escala: 0.36,
        afinidade: pesos(&[(stat_idx::INT, 7), (stat_idx::VIT, 3)]),
        descricao: "Estranho e sábio. Inteligência — o pet da arma mágica.",
    },
    Especie {
        base: item_id::PET_CARANGUEJO,
        nome: "Caranguejinho",
        bicho: "bichos/caranguejo",
        escala: 0.7,
        afinidade: pesos(&[(stat_idx::RES, 7), (stat_idx::VIT, 3)]),
        descricao: "Anda de lado e não larga. Resistência.",
    },
];

/// Monta o vetor de pesos em tempo de compilacao.
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

/// (especie, grau) de um id de pet.
pub fn de_item(item_id: u16) -> Option<(&'static Especie, u8)> {
    let (base, grau) = item_id::pet_de_id(item_id)?;
    Some((especie(base)?, grau))
}

/// Nome completo com o grau: "Lobinho Azul".
pub fn nome_do_item(id: u16) -> Option<String> {
    let (e, grau) = de_item(id)?;
    Some(format!("{} {}", e.nome, nome_do_grau(grau)))
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

/// Velocidade do pet, em multiplos de `PLAYER_SPEED`. O laranja empata com a
/// montaria (`loja::VEL_MONTADO`): so' o topo acompanha quem esta' montado.
pub fn velocidade(grau: u8) -> f32 {
    match grau {
        1 => 0.90,
        2 => 1.05,
        3 => 1.20,
        4 => 1.35,
        _ => 1.50,
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

/// Mais longe que isto do dono, o pet larga o alvo e volta.
pub const COLEIRA: f32 = 20.0;
/// Encostou a esta distancia do saque, coletou.
pub const ALCANCE_DA_COLETA: f32 = 0.6;
/// Distancia em que o pet orbita o dono quando nao tem o que fazer.
pub const DISTANCIA_DE_SEGUIR: f32 = 2.0;
/// Desistiu de um saque (bolsa cheia): nao tenta de novo por isto.
pub const DESISTENCIA_S: f32 = 5.0;

/// Quantos pontos de atributo o pet do grau `grau` entrega. E' a curva de cor
/// que os itens ja' usam (`items::tier_stat_mult`), em 9 pontos de base:
/// 5, 8, 11, 14 e 17.
pub fn pontos(grau: u8) -> u32 {
    (crate::items::tier_stat_mult(grau) * 9.0).round() as u32
}

/// Os pontos do pet, ja' repartidos pelos seis atributos. A sobra da divisao
/// vai pro atributo de maior peso — o total bate com `pontos` sempre.
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
        // O maior peso fica com a sobra.
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

/// Chance de cada grau no Pergaminho de Invocação: Pet, em pontos
/// percentuais. Mesma forma da tabela de chaves (`loja::BauCraft`), esticada
/// pro quinto grau.
pub const CHANCES_DO_PERGAMINHO: [u8; GRAU_MAX as usize] = [55, 28, 12, 4, 1];

/// Sorteia (especie, grau) do pergaminho. `r_especie` e `r_grau` em 0..1.
pub fn rolar(r_especie: f32, r_grau: f32) -> (u16, u8) {
    let i = ((r_especie.clamp(0.0, 0.999_999) * ESPECIE_COUNT as f32) as usize).min(ESPECIE_COUNT - 1);
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
            for grau in 1..=GRAU_MAX {
                let id = item_id::pet_no_grau(e.base, grau);
                let v = pontos_por_stat(id);
                assert_eq!(
                    v.iter().sum::<u32>(),
                    pontos(grau),
                    "{} grau {grau}: a soma tem que bater com o total",
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
        }
        assert_eq!(pontos(1), 5);
        assert_eq!(pontos(2), 8);
        assert_eq!(pontos(3), 11);
        assert_eq!(pontos(4), 14);
        assert_eq!(pontos(5), 17);
        assert_eq!(pontos_por_stat(item_id::GOLD), [0; STAT_COUNT]);
    }

    #[test]
    fn o_id_carrega_a_especie_e_o_grau() {
        for e in ESPECIES {
            for grau in 1..=GRAU_MAX {
                let id = item_id::pet_no_grau(e.base, grau);
                assert_eq!(item_id::pet_de_id(id), Some((e.base, grau)));
                assert_eq!(de_item(id).map(|(x, g)| (x.base, g)), Some((e.base, grau)));
                assert_eq!(
                    crate::equip_slot_of(id),
                    Some(crate::EquipSlot::Pet),
                    "pet tem que equipar no slot do pet"
                );
            }
        }
        assert_eq!(item_id::pet_de_id(item_id::PET_LOBO - 1), None);
        assert_eq!(item_id::pet_de_id(item_id::PET_ULTIMO + 1), None);
    }

    #[test]
    fn o_grau_manda_na_velocidade_e_no_raio() {
        for grau in 1..GRAU_MAX {
            assert!(velocidade(grau + 1) > velocidade(grau));
            assert!(raio_de_busca(grau + 1) > raio_de_busca(grau));
            assert!(pontos(grau + 1) > pontos(grau));
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
        assert_eq!(rolar(0.0, 0.0), (item_id::PET_LOBO, 1));
        assert_eq!(rolar(0.999, 0.999).1, GRAU_MAX);
        // Toda especie e' alcancavel e nada estoura o catalogo.
        for k in 0..ESPECIE_COUNT {
            let r = (k as f32 + 0.5) / ESPECIE_COUNT as f32;
            assert_eq!(rolar(r, 0.0).0, ESPECIES[k].base);
        }
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
