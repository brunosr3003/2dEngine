//! GLOBAL CHAT rules shared by the server and the Panopticon (owner,
//! 09/11/2026: "global chat with a swear filter, report, spam etc; even with
//! the filter, save the history of everything said, and in the Panopticon a
//! list of forbidden words where I choose to ban or give a warning").
//!
//! The filter works on a NORMALIZED copy of the text (lower case, no accents,
//! leetspeak undone, repeated letters squeezed, spaced-out letters joined), so
//! "F.u.c.k", "fuuuck" and "fvck"-style tricks hit the same entry. What the
//! players see keeps the original text with each hit replaced by asterisks.

use serde::{Deserialize, Serialize};

/// Chat channels: everyone online, this island only, the party.
pub const CANAL_GLOBAL: u8 = 0;
pub const CANAL_ILHA: u8 = 1;
pub const CANAL_GRUPO: u8 = 2;
/// One player to another (whisper / PM), on any island.
pub const CANAL_SUSSURRO: u8 = 3;

pub fn nome_do_canal(c: u8) -> &'static str {
    match c {
        CANAL_ILHA => "Island",
        CANAL_GRUPO => "Party",
        CANAL_SUSSURRO => "Whisper",
        _ => "Global",
    }
}

/// Who may whisper me (a setting of the receiver).
pub const SUSSURRO_TODOS: u8 = 0;
pub const SUSSURRO_AMIGOS: u8 = 1;
pub const SUSSURRO_NINGUEM: u8 = 2;

/// How long the sender waits for the receiver's process to answer before
/// calling them offline.
pub const SUSSURRO_ESPERA_S: f64 = 2.5;

/// "@Name message" → (Name, message). The name is the first word after "@".
pub fn sussurro_no_texto(texto: &str) -> Option<(String, String)> {
    let t = texto.trim_start().strip_prefix('@')?;
    let (nome, resto) = t.split_once(char::is_whitespace)?;
    let resto = resto.trim();
    (!nome.is_empty() && !resto.is_empty()).then(|| (nome.to_string(), resto.to_string()))
}

/// Longest message, in characters.
pub const MAX_CHARS: usize = 140;
/// Minimum gap between two messages of the same player.
pub const INTERVALO_MIN_S: f64 = 2.0;
/// At most this many messages in `JANELA_S`.
pub const RAJADA: usize = 5;
pub const JANELA_S: f64 = 30.0;
/// The same text again inside this window is refused (copy-paste spam).
pub const REPETIDA_S: f64 = 60.0;
/// Automatic mute for a word whose action is `Silenciar`.
pub const SILENCIO_AUTOMATICO_MIN: i64 = 30;

/// What a forbidden word does when it shows up. Ordered from mild to severe:
/// when a message hits several, the most severe wins.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, Hash)]
pub enum Acao {
    /// Replaced by *** and the message goes out.
    Censurar,
    /// Censored, goes out, and the player gets a warning.
    Alertar,
    /// Not delivered; the player is muted for `SILENCIO_AUTOMATICO_MIN`.
    Silenciar,
    /// Not delivered; the account is banned.
    Banir,
}

impl Acao {
    pub fn de_texto(s: &str) -> Option<Self> {
        match s {
            "censurar" => Some(Acao::Censurar),
            "alertar" => Some(Acao::Alertar),
            "silenciar" => Some(Acao::Silenciar),
            "banir" => Some(Acao::Banir),
            _ => None,
        }
    }

    pub fn texto(self) -> &'static str {
        match self {
            Acao::Censurar => "censurar",
            Acao::Alertar => "alertar",
            Acao::Silenciar => "silenciar",
            Acao::Banir => "banir",
        }
    }

    /// Does the message still reach the other players?
    pub fn entrega(self) -> bool {
        matches!(self, Acao::Censurar | Acao::Alertar)
    }
}

/// Seed list (censor only). The staff edits it in the Panopticon.
pub const PADRAO: &[&str] = &[
    // English
    "fuck", "fucker", "fucking", "motherfucker", "shit", "bullshit", "bitch", "bastard", "asshole",
    "dick", "cock", "pussy", "cunt", "whore", "slut", "fag", "faggot", "nigger", "nigga", "retard",
    "wanker", "twat",
    // Português
    "porra", "caralho", "merda", "puta", "puto", "putaria", "viado", "veado", "buceta", "boceta",
    "cacete", "foda", "foder", "fodase", "foda-se", "arrombado", "arrombada", "cuzao", "cuzão",
    "corno", "piranha", "vagabunda", "babaca", "otario", "otário", "desgraçado", "fdp", "vsf",
    "pqp", "krl", "crl", "macaco",
];

/// Lower case, accents off, leetspeak undone, letters only, repeats squeezed.
pub fn normalizar(palavra: &str) -> String {
    let mut s = String::new();
    for c in palavra.chars().flat_map(|c| c.to_lowercase()) {
        let c = match c {
            'á' | 'à' | 'â' | 'ã' | 'ä' | '@' | '4' => 'a',
            'é' | 'è' | 'ê' | 'ë' | '3' => 'e',
            'í' | 'ì' | 'î' | 'ï' | '1' | '!' | '|' => 'i',
            'ó' | 'ò' | 'ô' | 'õ' | 'ö' | '0' => 'o',
            'ú' | 'ù' | 'û' | 'ü' => 'u',
            'ç' => 'c',
            '$' | '5' => 's',
            '7' => 't',
            c if c.is_alphanumeric() => c,
            _ => continue,
        };
        if s.ends_with(c) {
            continue; // "fuuuck" -> "fuck"
        }
        s.push(c);
    }
    s
}

/// One word of the text with where it sits (byte range in the original).
struct Pedaco {
    ini: usize,
    fim: usize,
    norm: String,
}

/// Splits on whitespace; runs of single letters ("f u c k") also become one
/// joined piece, so spacing a word out does not dodge the list.
fn pedacos(texto: &str) -> Vec<Pedaco> {
    let mut v = Vec::new();
    let mut i = 0;
    let bytes: Vec<(usize, char)> = texto.char_indices().collect();
    let n = bytes.len();
    let mut toks: Vec<(usize, usize)> = Vec::new();
    while i < n {
        while i < n && bytes[i].1.is_whitespace() {
            i += 1;
        }
        let ini = i;
        while i < n && !bytes[i].1.is_whitespace() {
            i += 1;
        }
        if ini < i {
            let a = bytes[ini].0;
            let b = if i < n { bytes[i].0 } else { texto.len() };
            toks.push((a, b));
        }
    }
    for &(a, b) in &toks {
        v.push(Pedaco { ini: a, fim: b, norm: normalizar(&texto[a..b]) });
    }
    // Spaced-out letters.
    let mut k = 0;
    while k < toks.len() {
        let curto = |t: &(usize, usize)| normalizar(&texto[t.0..t.1]).chars().count() == 1;
        if curto(&toks[k]) {
            let mut j = k;
            while j < toks.len() && curto(&toks[j]) {
                j += 1;
            }
            if j - k >= 3 {
                let (a, b) = (toks[k].0, toks[j - 1].1);
                v.push(Pedaco { ini: a, fim: b, norm: normalizar(&texto[a..b]) });
            }
            k = j;
        } else {
            k += 1;
        }
    }
    v
}

#[derive(Debug, Clone, PartialEq)]
pub struct Resultado {
    /// What the other players see (hits as asterisks).
    pub mostrado: String,
    /// The list entries that matched, with their action.
    pub achadas: Vec<(String, Acao)>,
}

impl Resultado {
    /// The most severe action hit, if any.
    pub fn pior(&self) -> Option<Acao> {
        self.achadas.iter().map(|a| a.1).max()
    }
}

/// Runs the forbidden-word list over a message.
pub fn filtrar(texto: &str, lista: &[(String, Acao)]) -> Resultado {
    let normas: Vec<(String, &str, Acao)> = lista
        .iter()
        .map(|(p, a)| (normalizar(p), p.as_str(), *a))
        .filter(|(n, _, _)| !n.is_empty())
        .collect();
    let mut trechos: Vec<(usize, usize)> = Vec::new();
    let mut achadas: Vec<(String, Acao)> = Vec::new();
    for p in pedacos(texto) {
        if p.norm.is_empty() {
            continue;
        }
        for (n, original, acao) in &normas {
            // Whole word, or the word with a short tail ("fucking", "porras").
            let bate = p.norm == *n
                || (n.chars().count() >= 4 && p.norm.starts_with(n.as_str()) && p.norm.len() <= n.len() + 3);
            if bate {
                trechos.push((p.ini, p.fim));
                if !achadas.iter().any(|(w, _)| w == original) {
                    achadas.push((original.to_string(), *acao));
                }
            }
        }
    }
    trechos.sort();
    let mut mostrado = String::with_capacity(texto.len());
    let mut pos = 0;
    for (a, b) in trechos {
        if a < pos {
            continue;
        }
        mostrado.push_str(&texto[pos..a]);
        mostrado.push_str(&"*".repeat(texto[a..b].chars().count().clamp(3, 12)));
        pos = b;
    }
    mostrado.push_str(&texto[pos..]);
    Resultado { mostrado, achadas }
}

/// Trims, squeezes spaces, drops control characters and cuts at `MAX_CHARS`.
pub fn limpar(texto: &str) -> String {
    let mut s = String::new();
    let mut espaco = false;
    for c in texto.chars().filter(|c| !c.is_control()) {
        if c.is_whitespace() {
            espaco = !s.is_empty();
            continue;
        }
        if espaco {
            s.push(' ');
            espaco = false;
        }
        s.push(c);
        if s.chars().count() >= MAX_CHARS {
            break;
        }
    }
    s
}

/// The tables, created by both the game server and the Panopticon.
pub const TABELAS: &[&str] = &[
    "CREATE TABLE IF NOT EXISTS chat_historico (
        id BIGSERIAL PRIMARY KEY,
        conta BIGINT NOT NULL,
        nome TEXT NOT NULL,
        zona TEXT NOT NULL DEFAULT '',
        texto TEXT NOT NULL,
        mostrado TEXT NOT NULL,
        palavras TEXT NOT NULL DEFAULT '',
        acao TEXT NOT NULL DEFAULT '',
        entregue BOOLEAN NOT NULL DEFAULT TRUE,
        criado TIMESTAMPTZ NOT NULL DEFAULT NOW()
    )",
    "CREATE INDEX IF NOT EXISTS chat_historico_conta ON chat_historico (conta, id DESC)",
    "ALTER TABLE chat_historico ADD COLUMN IF NOT EXISTS canal TEXT NOT NULL DEFAULT 'global'",
    "ALTER TABLE chat_historico ADD COLUMN IF NOT EXISTS para TEXT NOT NULL DEFAULT ''",
    "CREATE TABLE IF NOT EXISTS chat_palavras (
        palavra TEXT PRIMARY KEY,
        acao TEXT NOT NULL DEFAULT 'censurar',
        criado TIMESTAMPTZ NOT NULL DEFAULT NOW()
    )",
    "CREATE TABLE IF NOT EXISTS chat_silencios (
        id BIGSERIAL PRIMARY KEY,
        conta BIGINT NOT NULL,
        nome TEXT NOT NULL DEFAULT '',
        ate TIMESTAMPTZ NOT NULL,
        motivo TEXT NOT NULL DEFAULT '',
        autor TEXT NOT NULL DEFAULT '',
        criado TIMESTAMPTZ NOT NULL DEFAULT NOW()
    )",
    "CREATE TABLE IF NOT EXISTS banimentos (
        id BIGSERIAL PRIMARY KEY,
        conta BIGINT NOT NULL,
        nome TEXT NOT NULL DEFAULT '',
        motivo TEXT NOT NULL DEFAULT '',
        autor TEXT NOT NULL DEFAULT '',
        ate TIMESTAMPTZ NULL,
        revogado BOOLEAN NOT NULL DEFAULT FALSE,
        criado TIMESTAMPTZ NOT NULL DEFAULT NOW()
    )",
    "CREATE TABLE IF NOT EXISTS chat_acoes (
        id BIGSERIAL PRIMARY KEY,
        tipo TEXT NOT NULL,
        conta BIGINT NOT NULL,
        nome TEXT NOT NULL DEFAULT '',
        detalhe TEXT NOT NULL DEFAULT '',
        autor TEXT NOT NULL DEFAULT '',
        criado TIMESTAMPTZ NOT NULL DEFAULT NOW()
    )",
    "CREATE TABLE IF NOT EXISTS chat_denuncias (
        id BIGSERIAL PRIMARY KEY,
        msg_id BIGINT NOT NULL,
        conta BIGINT NOT NULL,
        nome TEXT NOT NULL DEFAULT '',
        criado TIMESTAMPTZ NOT NULL DEFAULT NOW(),
        UNIQUE (msg_id, conta)
    )",
];

/// The Postgres NOTIFY channel every game process listens to.
pub const CANAL_NOTIFY: &str = "tempest_chat";

/// What travels on `CANAL_NOTIFY` (JSON).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "t")]
pub enum Notificacao {
    /// A message for everyone online.
    #[serde(rename = "msg")]
    Mensagem {
        id: i64,
        de: String,
        texto: String,
        staff: bool,
        #[serde(default)]
        canal: u8,
        /// For `CANAL_ILHA`: only processes serving this zone deliver it.
        #[serde(default)]
        zona: String,
    },
    /// A whisper looking for its receiver: the process that has them online
    /// delivers it and answers with `SussurroResposta`.
    #[serde(rename = "sussurro")]
    Sussurro {
        id: i64,
        de: String,
        para: String,
        texto: String,
        staff: bool,
        /// Sender and receiver are friends (checked in the DB before sending).
        amigos: bool,
        /// Which process sent it (`server::chat_global::PROCESSO`).
        origem: u64,
    },
    /// The receiver's process: 0 delivered, 1 refused (their setting).
    #[serde(rename = "sussurro_ok")]
    SussurroResposta { id: i64, origem: u64, resultado: u8, para: String },
    /// A staff warning for one account.
    #[serde(rename = "aviso")]
    Aviso { conta: i64, texto: String },
    /// Staff asked everyone online to rate the game (Panopticon).
    #[serde(rename = "avaliacao")]
    PedirAvaliacao,
    /// Banned: drop its sessions now.
    #[serde(rename = "expulsar")]
    Expulsar { conta: i64, motivo: String },
}

#[cfg(test)]
mod testes {
    use super::*;

    fn lista() -> Vec<(String, Acao)> {
        vec![
            ("fuck".into(), Acao::Censurar),
            ("merda".into(), Acao::Alertar),
            ("macaco".into(), Acao::Banir),
        ]
    }

    #[test]
    fn pega_os_truques_e_censura_so_a_palavra() {
        let r = filtrar("what the FUUUCK man", &lista());
        assert_eq!(r.mostrado, "what the ****** man");
        assert_eq!(r.pior(), Some(Acao::Censurar));
        for t in ["f.u.c.k", "f u c k you", "fucking noob", "M3rd@ de jogo", "Mérda"] {
            assert!(filtrar(t, &lista()).pior().is_some(), "{t} escaped");
        }
        assert_eq!(filtrar("merda", &lista()).pior(), Some(Acao::Alertar));
        assert_eq!(filtrar("seu macaco", &lista()).pior(), Some(Acao::Banir), "the most severe wins");
    }

    #[test]
    fn nao_pega_palavra_inocente() {
        for t in ["class assignment", "Scunthorpe", "good luck have fun", "merdinha não"] {
            let r = filtrar(t, &[("ass".into(), Acao::Censurar), ("cunt".into(), Acao::Censurar)]);
            assert_eq!(r.pior(), None, "{t} flagged");
        }
    }

    #[test]
    fn arroba_vira_sussurro() {
        assert_eq!(sussurro_no_texto("@Lyra oi tudo bem"), Some(("Lyra".into(), "oi tudo bem".into())));
        assert_eq!(sussurro_no_texto("@Lyra"), None);
        assert_eq!(sussurro_no_texto("oi @Lyra"), None);
    }

    #[test]
    fn limpar_corta_e_aperta() {
        assert_eq!(limpar("  oi \n\t  tudo   bem  "), "oi tudo bem");
        assert_eq!(limpar(&"a".repeat(500)).chars().count(), MAX_CHARS);
    }

    #[test]
    fn notificacao_vai_e_volta() {
        let n = Notificacao::Mensagem { id: 3, de: "Ana".into(), texto: "hi".into(), staff: false, canal: 1, zona: "ilha_gelo".into() };
        let j = serde_json::to_string(&n).unwrap();
        assert!(j.contains("\"t\":\"msg\""));
        assert_eq!(serde_json::from_str::<Notificacao>(&j).unwrap(), n);
    }
}
