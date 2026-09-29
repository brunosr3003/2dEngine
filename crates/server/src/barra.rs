//! The character's item bar (MIR4): four slots, each with its consumable, its
//! AUTO and its threshold. The CLIENT is what uses it; here we only validate
//! what it sends and store it (JSON in `characters.barra_json`).
use shared::constants::item_id as it;
use shared::protocol::{EspacoDaBarra, ESPACOS_DA_BARRA};

/// What may go on the bar: a consumable that `UseItem` knows how to use.
pub fn cabe_na_barra(id: u16) -> bool {
    [
        it::HEALTH_POTION,
        it::GREATER_HEAL,
        it::MANA_POTION,
        it::GREATER_MANA,
        it::STAMINA_POTION,
        it::XP_POTION,
        it::FORTUNA_POTION,
        it::SORTE_POTION,
    ]
    .contains(&id)
}

/// Cleans what the client sent: at most `ESPACOS_DA_BARRA` slots, an item
/// outside the list becomes an empty slot, threshold between 5 and 95.
pub fn valida(espacos: &[EspacoDaBarra]) -> Vec<EspacoDaBarra> {
    espacos
        .iter()
        .take(ESPACOS_DA_BARRA)
        .map(|e| {
            if e.item_id == 0 || !cabe_na_barra(e.item_id) {
                EspacoDaBarra::default()
            } else {
                EspacoDaBarra {
                    item_id: e.item_id,
                    auto: e.auto,
                    limiar: e.limiar.clamp(5, 95),
                }
            }
        })
        .collect()
}

pub fn para_json(barra: &[EspacoDaBarra]) -> String {
    serde_json::to_string(barra).unwrap_or_default()
}

/// Empty or corrupt becomes an empty bar: the client falls back to the default.
pub fn de_json(s: &str) -> Vec<EspacoDaBarra> {
    if s.is_empty() {
        return Vec::new();
    }
    serde_json::from_str::<Vec<EspacoDaBarra>>(s)
        .map(|v| valida(&v))
        .unwrap_or_default()
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn valida_corta_espacos_itens_e_limiar() {
        let mandado = vec![
            EspacoDaBarra {
                item_id: it::HEALTH_POTION,
                auto: true,
                limiar: 99,
            },
            EspacoDaBarra {
                item_id: it::STEEL,
                auto: true,
                limiar: 40,
            },
            EspacoDaBarra {
                item_id: it::FORTUNA_POTION,
                auto: true,
                limiar: 0,
            },
            EspacoDaBarra {
                item_id: 0,
                auto: true,
                limiar: 50,
            },
            EspacoDaBarra {
                item_id: it::MANA_POTION,
                auto: false,
                limiar: 40,
            },
        ];
        let v = valida(&mandado);
        assert_eq!(v.len(), ESPACOS_DA_BARRA, "no maximo quatro");
        assert_eq!(
            v[0],
            EspacoDaBarra {
                item_id: it::HEALTH_POTION,
                auto: true,
                limiar: 95
            }
        );
        assert_eq!(v[1], EspacoDaBarra::default(), "material nao vai na barra");
        assert_eq!(v[2].item_id, it::FORTUNA_POTION);
        assert_eq!(v[3], EspacoDaBarra::default());
    }

    #[test]
    fn json_ida_e_volta_e_lixo_vira_vazio() {
        let barra = vec![
            EspacoDaBarra {
                item_id: it::SORTE_POTION,
                auto: true,
                limiar: 5,
            },
            EspacoDaBarra {
                item_id: it::GREATER_HEAL,
                auto: false,
                limiar: 35,
            },
        ];
        assert_eq!(de_json(&para_json(&barra)), barra);
        assert!(de_json("").is_empty());
        assert!(de_json("{nao e' json").is_empty());
    }
}
