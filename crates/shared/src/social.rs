//! Per-character social, shared by every channel of the realm.
use serde::{Deserialize, Serialize};

pub const MAX_AMIGOS: usize = 100;
pub const MAX_CARTAS: usize = 100;
pub const MAX_CLA: usize = 50;
pub const MAX_GRUPO: usize = 5;
/// Participants near the kill's author, in the same instance.
pub const RAIO_XP_GRUPO: f32 = 25.0;
pub fn bonus_xp_grupo(participantes: usize) -> u32 {
    participantes.saturating_sub(1).min(3) as u32 * 10
}
/// The final fraction is discarded; nobody gets more XP than another by order.
pub fn parcela_xp_grupo(xp: u64, participantes: usize) -> u64 {
    let n = participantes.max(1) as u128;
    (u128::from(xp) * u128::from(100 + bonus_xp_grupo(participantes)) / (100 * n)) as u64
}


#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Pedido {
    Estado,
    Amizade {
        nome: String,
    },
    ResponderAmizade {
        nome: String,
        aceitar: bool,
    },
    RemoverAmigo {
        nome: String,
    },
    EnviarCarta {
        para: String,
        assunto: String,
        texto: String,
    },
    LerCarta {
        id: i64,
    },
    ApagarCarta {
        id: i64,
    },
    CriarCla {
        nome: String,
    },
    ConvidarCla {
        nome: String,
    },
    ResponderCla {
        id: i64,
        aceitar: bool,
    },
    SairCla,
    ExpulsarCla {
        nome: String,
    },
    LiderCla {
        nome: String,
    },
    AvisoCla {
        texto: String,
    },
    DissolverCla,
    EnviarOficial {
        envio: String,
        para: Option<String>,
        assunto: String,
        texto: String,
        anexos: Vec<Anexo>,
    },
    ReceberAnexos {
        id: i64,
    },
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Estado {
    pub amigos: Vec<String>,
    pub recebidos: Vec<String>,
    pub enviados: Vec<String>,
    /// Online on this channel; absence does not imply offline on another channel.
    pub neste_canal: Vec<String>,
    pub cartas: Vec<Carta>,
    pub cla: Option<Cla>,
    pub convites_cla: Vec<ConviteCla>,
    pub cargo: Option<String>,
    pub destinatarios: u32,
    pub oficiais: Vec<Carta>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Carta {
    pub id: i64,
    pub de: String,
    pub assunto: String,
    pub texto: String,
    pub quando: i64,
    pub lida: bool,
    pub oficial: bool,
    pub anexos: Vec<Anexo>,
    pub resgatada: bool,
}

// No `Eq`: the instance has a float (the affixes' `value_pct`), and floats
// have no total equality. `PartialEq` is enough for everything the mail compares.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Anexo {
    pub item_id: u16,
    pub qtd: u32,
    /// A INSTANCIA do item: raridade, tier, refino, afixos, afinidade.
    ///
    /// Without it the mail delivered the item BARE. It is not an equipment
    /// detail: a pet sent by mail arrived at level 1, with no skills and with its
    /// affinity re-rolled, **silently** — the same with a mount. The player had
    /// no way to know the mail ate half the item.
    ///
    /// `None` = an item with no instance (material, potion, copper), which is most of them.
    #[serde(default)]
    pub instance: Option<crate::items::ItemInstance>,
}

pub const MAX_ANEXOS: usize = 8;
pub const MAX_QTD_ANEXO: u32 = 100_000;

pub fn anexos_validos(anexos: &[Anexo]) -> bool {
    anexos.len() <= MAX_ANEXOS
        && anexos
            .iter()
            .all(|a| a.item_id > 0 && a.qtd > 0 && a.qtd <= MAX_QTD_ANEXO)
        // An item WITH an instance does not stack (each is unique), so sending two
        // at once would deliver only one.
        && anexos
            .iter()
            .all(|a| a.instance.is_none() || a.qtd == 1)
        && anexos
            .iter()
            .enumerate()
            .all(|(i, a)| !anexos[..i].iter().any(|b| b.item_id == a.item_id))
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Cla {
    pub id: i64,
    pub nome: String,
    pub lider: String,
    pub aviso: String,
    pub membros: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConviteCla {
    pub id: i64,
    pub nome: String,
    pub de: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Aviso {
    Estado(Estado),
    Resultado { ok: bool, texto: String },
}

/// Validates before querying the database; limits in characters AND bytes.
pub fn texto_valido(texto: &str, min: usize, max: usize) -> bool {
    texto.len() <= max * 4
        && (min..=max).contains(&texto.trim().chars().count())
        && !texto.chars().any(char::is_control)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn xp_progressiva_tem_teto_e_divisao_inteira() {
        for (n, bonus, xp) in [(1,0,100),(2,10,55),(3,20,40),(4,30,32),(5,30,26)] {
            assert_eq!(bonus_xp_grupo(n), bonus);
            assert_eq!(parcela_xp_grupo(100,n), xp);
        }
        assert_eq!(parcela_xp_grupo(1,2),0);
        assert_eq!(parcela_xp_grupo(0,4),0);
        assert_eq!(parcela_xp_grupo(u64::MAX,1),u64::MAX);
        assert_eq!(parcela_xp_grupo(u64::MAX,2), (u128::from(u64::MAX)*110/200) as u64);
    }

    #[test]
    fn limites_unicode_e_controle() {
        assert!(texto_valido("Clã dos Mares", 3, 24));
        assert!(!texto_valido("   ", 1, 24));
        assert!(!texto_valido("a\nspoof", 1, 24));
        assert!(!texto_valido(&"á".repeat(25), 1, 24));
    }
    #[test]
    fn limites_de_anexos() {
        assert!(anexos_validos(&[]));
        assert!(anexos_validos(&[Anexo { item_id: 1, qtd: 1, instance: None }]));
        assert!(!anexos_validos(&[Anexo { item_id: 0, qtd: 1, instance: None }]));
        assert!(!anexos_validos(&[Anexo { item_id: 1, qtd: 0, instance: None }]));
        assert!(!anexos_validos(&[Anexo {
            item_id: 1,
            qtd: 100_001,
            instance: None
        }]));
        assert!(!anexos_validos(&[
            Anexo { item_id: 1, qtd: 1, instance: None },
            Anexo { item_id: 1, qtd: 2, instance: None }
        ]));
        assert!(!anexos_validos(
            &(1..=9)
                .map(|i| Anexo { item_id: i, qtd: 1, instance: None })
                .collect::<Vec<_>>()
        ));
    }
    #[test]
    fn wire_social() {
        let pedido = crate::protocol::ClientMessage::Social {
            pedido: Pedido::EnviarCarta {
                para: "brunji".into(),
                assunto: "Olá".into(),
                texto: "Vamos jogar?".into(),
            },
        };
        let bytes = crate::protocol::encode(&pedido).unwrap();
        let volta: crate::protocol::ClientMessage = crate::protocol::decode(&bytes).unwrap();
        assert_eq!(bytes, crate::protocol::encode(&volta).unwrap());
        let aviso = crate::protocol::ServerMessage::Social {
            aviso: Aviso::Estado(Estado::default()),
        };
        let bytes = crate::protocol::encode(&aviso).unwrap();
        let volta: crate::protocol::ServerMessage = crate::protocol::decode(&bytes).unwrap();
        assert_eq!(bytes, crate::protocol::encode(&volta).unwrap());
    }
}
