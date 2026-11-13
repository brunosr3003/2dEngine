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
        /// The clan tag (`tag_valida`), shown as "[TAG] Name".
        tag: String,
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
    /// Leader: set or change the clan tag (`tag_valida`).
    TagCla {
        tag: String,
    },
    /// Leader: promote a member to officer (`oficial = true`) or back.
    CargoCla {
        nome: String,
        oficial: bool,
    },
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Estado {
    pub amigos: Vec<String>,
    pub recebidos: Vec<String>,
    pub enviados: Vec<String>,
    /// Friends and clan members online in ANY channel of the realm (the
    /// realm-wide presence table, plus whoever is on this very channel).
    pub neste_canal: Vec<String>,
    pub cartas: Vec<Carta>,
    pub cla: Option<Cla>,
    pub convites_cla: Vec<ConviteCla>,
    pub cargo: Option<String>,
    pub destinatarios: u32,
    pub oficiais: Vec<Carta>,
    /// Presence, level, class and tag of every friend and clan member.
    pub contatos: Vec<Contato>,
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
    /// "" = no tag yet (clans created before tags existed).
    pub tag: String,
    /// Officers (subset of `membros`).
    pub oficiais: Vec<String>,
}

/// A friend or clan member, as the social panel shows them.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct Contato {
    pub nome: String,
    /// Clan tag, "" = none.
    pub tag: String,
    /// `cargo::*` inside MY clan (meaningless for a friend outside it).
    pub cargo: u8,
    pub nivel: u16,
    /// `skills::Conjunto` of the weapon; `SEM_CONJUNTO` = unknown.
    pub conjunto: u8,
    /// Zone id (`terreno::def_da_zona`) and channel where they are / were last.
    pub zona: String,
    pub canal: String,
    pub online: bool,
    /// Unix seconds of the last heartbeat (or logout); 0 = never seen.
    pub visto: i64,
}

pub const SEM_CONJUNTO: u8 = u8::MAX;

/// Ranks inside a clan.
pub mod cargo {
    pub const MEMBRO: u8 = 0;
    pub const OFICIAL: u8 = 1;
    pub const LIDER: u8 = 2;

    pub fn nome(c: u8) -> &'static str {
        match c {
            LIDER => "Leader",
            OFICIAL => "Officer",
            _ => "Member",
        }
    }
}

/// What a clan rank may do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AcaoCla {
    Convidar,
    Expulsar,
    /// Promote / demote officers.
    Cargo,
    TransferirLideranca,
    Aviso,
    Tag,
    Dissolver,
    Sair,
}

/// May `eu` (rank) do `acao` to `alvo` (rank; ignored for actions with no target)?
/// Officers invite, edit the notice and kick plain members; everything that
/// changes ranks, the tag or the clan's existence is the leader's.
pub fn pode_no_cla(eu: u8, acao: AcaoCla, alvo: u8) -> bool {
    use cargo::*;
    match acao {
        AcaoCla::Convidar | AcaoCla::Aviso => eu >= OFICIAL,
        AcaoCla::Expulsar => eu == LIDER && alvo != LIDER || eu == OFICIAL && alvo == MEMBRO,
        AcaoCla::Cargo | AcaoCla::TransferirLideranca => eu == LIDER && alvo != LIDER,
        AcaoCla::Tag | AcaoCla::Dissolver => eu == LIDER,
        AcaoCla::Sair => eu != LIDER,
    }
}

pub const TAG_MIN: usize = 2;
pub const TAG_MAX: usize = 5;

/// A clan tag: 2–5 characters, A–Z / 0–9. Lower case is accepted and
/// upper-cased; the returned string is what gets stored. Uniqueness is
/// case-insensitive (the database index) and the word filter runs on the
/// server (`tag_limpa`).
pub fn tag_valida(tag: &str) -> Result<String, &'static str> {
    let t = tag.trim().to_ascii_uppercase();
    let n = t.chars().count();
    if !(TAG_MIN..=TAG_MAX).contains(&n) {
        return Err("Clan tag: 2 to 5 characters.");
    }
    if !t.chars().all(|c| c.is_ascii_uppercase() || c.is_ascii_digit()) {
        return Err("Clan tag: letters A-Z and digits 0-9 only.");
    }
    Ok(t)
}

/// The tag against the chat's word list. A tag has no spaces to split words
/// on, so any listed word of 3+ letters INSIDE it counts too ("XFUCK").
pub fn tag_limpa(tag: &str, palavras: &[(String, crate::chat::Acao)]) -> bool {
    if !crate::chat::filtrar(tag, palavras).achadas.is_empty() {
        return false;
    }
    let t = crate::chat::normalizar(tag);
    !palavras.iter().any(|(p, _)| {
        let p = crate::chat::normalizar(p);
        p.chars().count() >= 3 && t.contains(&p)
    })
}

/// "[TAG] Name", or just the name with no tag.
pub fn com_tag(tag: &str, nome: &str) -> String {
    if tag.is_empty() {
        nome.to_string()
    } else {
        format!("[{tag}] {nome}")
    }
}

/// Seconds without a heartbeat after which a presence row counts as offline
/// (the channel process died without saying goodbye). The heartbeat is every
/// `PRESENCA_BATIDA_S`.
pub const PRESENCA_TTL_S: i64 = 60;
pub const PRESENCA_BATIDA_S: i64 = 20;

/// Online = the row says so AND it was refreshed recently.
pub fn esta_online(online: bool, visto: i64, agora: i64) -> bool {
    online && agora.saturating_sub(visto) <= PRESENCA_TTL_S
}

/// Clan list order: online first, then rank (leader, officers), then name.
pub fn ordenar_membros(v: &mut [Contato]) {
    v.sort_by(|a, b| {
        b.online
            .cmp(&a.online)
            .then(b.cargo.cmp(&a.cargo))
            .then_with(|| a.nome.to_lowercase().cmp(&b.nome.to_lowercase()))
    });
}

/// "5m ago", "3h ago", "2d ago" for the last-seen column.
pub fn visto_ha(visto: i64, agora: i64) -> String {
    if visto <= 0 {
        return "Never seen".into();
    }
    let s = agora.saturating_sub(visto).max(0);
    match s {
        0..=59 => "Just now".into(),
        60..=3599 => format!("{}m ago", s / 60),
        3600..=86_399 => format!("{}h ago", s / 3600),
        _ => format!("{}d ago", s / 86_400),
    }
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
/// The BODY of a letter: like `texto_valido`, but line breaks are allowed
/// (paragraphs). Names and subjects keep `texto_valido`, where a line break is
/// a way to fake another line.
pub fn corpo_de_carta_valido(texto: &str, min: usize, max: usize) -> bool {
    texto.len() <= max * 4
        && (min..=max).contains(&texto.trim().chars().count())
        && !texto.chars().any(|c| c.is_control() && c != '\n')
}

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
        assert!(corpo_de_carta_valido("Hi!\n\nSee you.", 1, 1000), "a letter has paragraphs");
        assert!(!corpo_de_carta_valido("bell\u{7}", 1, 1000));
        assert!(!texto_valido(&"á".repeat(25), 1, 24));
    }
    #[test]
    fn tag_do_cla() {
        assert_eq!(tag_valida("ab"), Ok("AB".into()));
        assert_eq!(tag_valida(" Mar1 "), Ok("MAR1".into()));
        assert_eq!(tag_valida("ABCDE"), Ok("ABCDE".into()));
        assert!(tag_valida("A").is_err());
        assert!(tag_valida("ABCDEF").is_err());
        assert!(tag_valida("A B").is_err());
        assert!(tag_valida("ÁB").is_err());
        assert!(tag_valida("AB-1").is_err());
        assert!(tag_valida("").is_err());
        let lista = vec![("fuck".to_string(), crate::chat::Acao::Censurar), ("ass".to_string(), crate::chat::Acao::Censurar)];
        assert!(tag_limpa("MARES", &lista));
        assert!(!tag_limpa("FUCK", &lista));
        assert!(!tag_limpa("XFUCK", &lista), "a listed word inside the tag");
        assert!(!tag_limpa("ASS", &lista));
        assert!(tag_limpa("BASS1", &lista), "a 2-3 letter word only matches the whole tag");
        assert_eq!(com_tag("MAR", "Ana"), "[MAR] Ana");
        assert_eq!(com_tag("", "Ana"), "Ana");
    }

    #[test]
    fn permissoes_do_cla() {
        use cargo::*;
        assert!(pode_no_cla(LIDER, AcaoCla::Convidar, MEMBRO));
        assert!(pode_no_cla(OFICIAL, AcaoCla::Convidar, MEMBRO));
        assert!(!pode_no_cla(MEMBRO, AcaoCla::Convidar, MEMBRO));
        assert!(pode_no_cla(OFICIAL, AcaoCla::Expulsar, MEMBRO));
        assert!(!pode_no_cla(OFICIAL, AcaoCla::Expulsar, OFICIAL));
        assert!(!pode_no_cla(OFICIAL, AcaoCla::Expulsar, LIDER));
        assert!(pode_no_cla(LIDER, AcaoCla::Expulsar, OFICIAL));
        assert!(!pode_no_cla(OFICIAL, AcaoCla::Cargo, MEMBRO));
        assert!(pode_no_cla(LIDER, AcaoCla::Cargo, OFICIAL));
        assert!(!pode_no_cla(OFICIAL, AcaoCla::Tag, 0));
        assert!(pode_no_cla(OFICIAL, AcaoCla::Aviso, 0));
        assert!(!pode_no_cla(LIDER, AcaoCla::Sair, 0));
        assert!(pode_no_cla(MEMBRO, AcaoCla::Sair, 0));
        assert!(!pode_no_cla(OFICIAL, AcaoCla::Dissolver, 0));
    }

    #[test]
    fn presenca_entre_canais() {
        assert!(esta_online(true, 1000, 1000));
        assert!(esta_online(true, 1000, 1000 + PRESENCA_TTL_S));
        assert!(!esta_online(true, 1000, 1001 + PRESENCA_TTL_S), "the channel died: stale row");
        assert!(!esta_online(false, 1000, 1000), "logged out");
        assert!(PRESENCA_BATIDA_S * 2 < PRESENCA_TTL_S, "one missed beat is not offline");
        let c = |n: &str, online: bool, cargo: u8| Contato { nome: n.into(), online, cargo, ..Default::default() };
        let mut v = vec![c("zed", false, 2), c("bia", true, 0), c("Ana", true, 1), c("caio", false, 0), c("dan", true, 2)];
        ordenar_membros(&mut v);
        let nomes: Vec<_> = v.iter().map(|c| c.nome.as_str()).collect();
        assert_eq!(nomes, ["dan", "Ana", "bia", "zed", "caio"]);
        assert_eq!(visto_ha(0, 10), "Never seen");
        assert_eq!(visto_ha(100, 130), "Just now");
        assert_eq!(visto_ha(0 + 1, 1 + 600), "10m ago");
        assert_eq!(visto_ha(1, 1 + 7200), "2h ago");
        assert_eq!(visto_ha(1, 1 + 3 * 86_400), "3d ago");
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
