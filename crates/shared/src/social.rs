//! Social por personagem, compartilhado por todos os canais do realm.
use serde::{Deserialize, Serialize};

pub const MAX_AMIGOS: usize = 100;
pub const MAX_CARTAS: usize = 100;
pub const MAX_CLA: usize = 50;
pub const MAX_GRUPO: usize = 5;

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
    /// Online neste canal; ausencia nao implica offline em outro canal.
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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Anexo {
    pub item_id: u16,
    pub qtd: u32,
}

pub const MAX_ANEXOS: usize = 8;
pub const MAX_QTD_ANEXO: u32 = 100_000;

pub fn anexos_validos(anexos: &[Anexo]) -> bool {
    // O BAU DO COLOSSO nao viaja por correio (docs/MAR_ABERTO.md): a unica
    // forma de leva-lo pra outra ilha e' o conves de um barco.
    if anexos
        .iter()
        .any(|a| crate::item_id::e_bau_de_colosso(a.item_id))
    {
        return false;
    }

    anexos.len() <= MAX_ANEXOS
        && anexos
            .iter()
            .all(|a| a.item_id > 0 && a.qtd > 0 && a.qtd <= MAX_QTD_ANEXO)
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

/// Valida antes de consultar o banco; limites em caracteres E bytes.
pub fn texto_valido(texto: &str, min: usize, max: usize) -> bool {
    texto.len() <= max * 4
        && (min..=max).contains(&texto.trim().chars().count())
        && !texto.chars().any(char::is_control)
}

#[cfg(test)]
mod tests {
    use super::*;
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
        assert!(anexos_validos(&[Anexo { item_id: 1, qtd: 1 }]));
        assert!(!anexos_validos(&[Anexo { item_id: 0, qtd: 1 }]));
        assert!(!anexos_validos(&[Anexo { item_id: 1, qtd: 0 }]));
        assert!(!anexos_validos(&[Anexo {
            item_id: 1,
            qtd: 100_001
        }]));
        assert!(!anexos_validos(&[
            Anexo { item_id: 1, qtd: 1 },
            Anexo { item_id: 1, qtd: 2 }
        ]));
        assert!(!anexos_validos(
            &(1..=9)
                .map(|i| Anexo { item_id: i, qtd: 1 })
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
