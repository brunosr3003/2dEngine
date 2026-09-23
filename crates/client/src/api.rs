//! Consulta a lista de canais no `web`.
//!
//! HTTP cru sobre `TcpStream`, sem dependencia nova. E' UMA requisicao GET num
//! endpoint conhecido; trazer um cliente HTTP completo pra isso engordaria o
//! binario do celular por nada.
//!
//! Roda numa thread propria — o loop de render nao pode parar esperando rede,
//! e a resposta chega por canal como qualquer outro evento.

use std::io::{Read, Write};
use std::net::TcpStream;
use std::sync::mpsc::{self, Receiver};
use std::time::Duration;

#[derive(Clone, Debug)]
pub struct Canal {
    pub id: String,
    pub zona: String,
    pub host: String,
    pub jogadores: u32,
    pub capacidade: u32,
    pub cheio: bool,
}

impl Canal {
    /// Nome do servidor (realm) e do canal, separados: o id vem como
    /// `SA01/2`.
    pub fn realm(&self) -> &str {
        self.id.split('/').next().unwrap_or(&self.id)
    }
}

/// Por onde se entra no servidor `realm`: o canal menos cheio da ilha
/// inicial (sem fila, se houver). O jogador so' escolhe o SERVIDOR — a ilha e'
/// a do personagem: ao escolher um salvo em outra ilha, o proprio servidor
/// manda reconectar la' (`TrocarZona`), e o personagem novo nasce no Bosque.
/// Sem canal da ilha inicial no ar, qualquer um do servidor serve de porta.
pub fn porta_de_entrada(canais: &[Canal], realm: &str) -> Option<String> {
    let inicial = shared::terreno::ARQUIPELAGO[0].zona;
    canais
        .iter()
        .filter(|c| c.realm() == realm)
        .min_by_key(|c| (c.zona != inicial, c.cheio, c.jogadores, c.id.clone()))
        .map(|c| c.host.clone())
}

/// Onde o `web` atende. No celular (iOS e Android) nao ha variavel de ambiente
/// nem servidor local: vai direto no web de producao
/// (docs/SERVIDORES_E_CANAIS.md).
fn endereco() -> String {
    #[cfg(any(target_os = "ios", target_os = "android"))]
    const PADRAO: &str = "mmo.brunji.com.br:80";
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    const PADRAO: &str = "127.0.0.1:8080";
    // `MMO_API_PADRAO` na COMPILACAO troca o padrao: e' como um APK de teste
    // aponta pro web local (10.0.2.2 no emulador), ja' que no celular nao ha'
    // variavel de ambiente.
    std::env::var("MMO_API")
        .unwrap_or_else(|_| option_env!("MMO_API_PADRAO").unwrap_or(PADRAO).into())
}

/// Dispara a busca. O resultado (ou o erro) chega pelo receiver.
pub fn buscar_canais() -> Receiver<Result<Vec<Canal>, String>> {
    em_thread(|| get("/api/channels").map(|(_, corpo)| parse(&corpo)))
}

/// Roda `f` numa thread e devolve o receiver do resultado.
fn em_thread<T: Send + 'static>(f: impl FnOnce() -> T + Send + 'static) -> Receiver<T> {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send(f());
    });
    rx
}

/// GET cru no `web`: (codigo HTTP, corpo).
fn get(caminho: &str) -> Result<(u16, String), String> {
    let endereco = endereco();
    let mut fluxo = TcpStream::connect(&endereco).map_err(|e| format!("{endereco}: {e}"))?;
    fluxo
        .set_read_timeout(Some(Duration::from_secs(6)))
        .map_err(|e| e.to_string())?;
    let pedido = format!("GET {caminho} HTTP/1.1\r\nHost: {endereco}\r\nConnection: close\r\n\r\n");
    fluxo
        .write_all(pedido.as_bytes())
        .map_err(|e| e.to_string())?;
    let mut resposta = String::new();
    fluxo
        .read_to_string(&mut resposta)
        .map_err(|e| e.to_string())?;

    let (cabecalho, corpo) = resposta
        .split_once("\r\n\r\n")
        .ok_or("resposta sem corpo")?;
    let codigo = cabecalho
        .split_whitespace()
        .nth(1)
        .and_then(|c| c.parse().ok())
        .ok_or("resposta sem codigo HTTP")?;
    // `Connection: close` evita chunked encoding, entao o corpo vem inteiro.
    Ok((codigo, corpo.to_string()))
}

/// POST cru no `web`: (codigo HTTP, corpo).
///
/// Gemeo do `get`, e nao uma generalizacao dele: o pedido com corpo precisa de
/// `Content-Length` e de `Content-Type`, e enfiar isso no `get` com `Option`
/// deixaria as duas chamadas piores pra nao repetir dez linhas.
fn post(caminho: &str, corpo_json: &str) -> Result<(u16, String), String> {
    let endereco = endereco();
    let mut fluxo = TcpStream::connect(&endereco).map_err(|e| format!("{endereco}: {e}"))?;
    fluxo
        .set_read_timeout(Some(Duration::from_secs(8)))
        .map_err(|e| e.to_string())?;
    let pedido = format!(
        "POST {caminho} HTTP/1.1\r\nHost: {endereco}\r\nContent-Type: application/json\r\n\
         Content-Length: {}\r\nConnection: close\r\n\r\n{corpo_json}",
        corpo_json.len()
    );
    fluxo
        .write_all(pedido.as_bytes())
        .map_err(|e| e.to_string())?;
    let mut resposta = String::new();
    fluxo
        .read_to_string(&mut resposta)
        .map_err(|e| e.to_string())?;
    let (cabecalho, corpo) = resposta
        .split_once("\r\n\r\n")
        .ok_or("resposta sem corpo")?;
    let codigo = cabecalho
        .split_whitespace()
        .nth(1)
        .and_then(|c| c.parse().ok())
        .ok_or("resposta sem codigo HTTP")?;
    Ok((codigo, corpo.to_string()))
}

/// Escapa o que vai DENTRO de uma string JSON.
///
/// Sem isto, uma senha com aspas ou barra invertida quebraria o JSON e o
/// servidor responderia 400 com uma mensagem que nao explica nada — e senha
/// e' justamente onde esses caracteres aparecem.
fn escapa(v: &str) -> String {
    let mut s = String::with_capacity(v.len() + 2);
    for c in v.chars() {
        match c {
            '"' => s.push_str("\\\""),
            '\\' => s.push_str("\\\\"),
            '\n' => s.push_str("\\n"),
            '\r' => s.push_str("\\r"),
            '\t' => s.push_str("\\t"),
            c if (c as u32) < 0x20 => s.push_str(&format!("\\u{:04x}", c as u32)),
            c => s.push(c),
        }
    }
    s
}

/// O que o cadastro respondeu.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RespostaCadastro {
    Criada,
    /// Usuario ou e-mail ja' existe (409), com o campo que bateu.
    JaExiste(String),
    /// O servidor recusou (400) — a mensagem dele ja' e' em portugues.
    Recusado(String),
    Erro(String),
}

/// Le' a resposta de `/api/register`.
pub fn interpreta_cadastro(codigo: u16, corpo: &str) -> RespostaCadastro {
    match codigo {
        201 => RespostaCadastro::Criada,
        409 => RespostaCadastro::JaExiste(
            campo_json(corpo, "error").unwrap_or_else(|| "já existe".into()),
        ),
        400 => RespostaCadastro::Recusado(
            campo_json(corpo, "error").unwrap_or_else(|| "dados inválidos".into()),
        ),
        c => RespostaCadastro::Erro(
            campo_json(corpo, "error").unwrap_or_else(|| format!("erro do servidor ({c})")),
        ),
    }
}

/// Cria a conta por usuario, e-mail e senha.
pub fn criar_conta(usuario: &str, email: &str, senha: &str) -> Receiver<RespostaCadastro> {
    let corpo = format!(
        r#"{{"username":"{}","email":"{}","password":"{}"}}"#,
        escapa(usuario),
        escapa(email),
        escapa(senha)
    );
    em_thread(move || match post("/api/register", &corpo) {
        Ok((c, corpo)) => interpreta_cadastro(c, &corpo),
        Err(e) => RespostaCadastro::Erro(e),
    })
}

/// Campo de texto de um JSON pequeno do nosso servidor (sem parser completo).
pub fn campo_json(corpo: &str, chave: &str) -> Option<String> {
    let i = corpo.find(&format!("\"{chave}\""))? + chave.len() + 2;
    let resto = corpo[i..].trim_start_matches([':', ' ']);
    if !resto.starts_with('"') {
        return None;
    }
    let mut saida = String::new();
    let mut escapado = false;
    for c in resto[1..].chars() {
        match (escapado, c) {
            (true, c) => {
                saida.push(c);
                escapado = false;
            }
            (false, '\\') => escapado = true,
            (false, '"') => return Some(saida),
            (false, c) => saida.push(c),
        }
    }
    None
}

/// Resposta do `/api/auth/google/poll`.
#[derive(Clone, Debug, PartialEq)]
pub enum RespostaPoll {
    Pendente,
    Pronto { usuario: String, token: String },
    Erro(String),
    Expirado,
}

pub fn interpreta_poll(codigo: u16, corpo: &str) -> RespostaPoll {
    match campo_json(corpo, "status").as_deref() {
        _ if codigo == 404 => RespostaPoll::Expirado,
        Some("pendente") => RespostaPoll::Pendente,
        Some("ok") => match (campo_json(corpo, "username"), campo_json(corpo, "token")) {
            (Some(usuario), Some(token)) => RespostaPoll::Pronto { usuario, token },
            _ => RespostaPoll::Erro("resposta incompleta do servidor".into()),
        },
        Some("erro") => {
            RespostaPoll::Erro(campo_json(corpo, "error").unwrap_or_else(|| "erro".into()))
        }
        _ => RespostaPoll::Erro(format!("resposta inesperada ({codigo})")),
    }
}

/// O servidor tem login com Google configurado?
pub fn google_config() -> Receiver<bool> {
    em_thread(|| {
        get("/api/auth/google/config")
            .map(|(c, corpo)| c == 200 && corpo.replace(' ', "").contains("\"enabled\":true"))
            .unwrap_or(false)
    })
}

/// Pede um login novo: (state, url de autorizacao do Google).
pub fn google_start() -> Receiver<Result<(String, String), String>> {
    em_thread(|| {
        let (codigo, corpo) = get("/api/auth/google/start")?;
        match (campo_json(&corpo, "state"), campo_json(&corpo, "url")) {
            (Some(state), Some(url)) if codigo == 200 => Ok((state, url)),
            _ => Err(campo_json(&corpo, "error")
                .unwrap_or_else(|| format!("servidor respondeu {codigo}"))),
        }
    })
}

/// Consulta se o login do `state` terminou no navegador.
pub fn google_poll(state: &str) -> Receiver<Result<RespostaPoll, String>> {
    // `state` e' base64url: nao precisa de escape na query.
    let caminho = format!("/api/auth/google/poll?state={state}");
    em_thread(move || get(&caminho).map(|(c, corpo)| interpreta_poll(c, &corpo)))
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn a_porta_de_entrada_e_a_ilha_inicial_menos_cheia() {
        let c = |id: &str, zona: &str, host: &str, jogadores: u32, cheio: bool| Canal {
            id: id.into(),
            zona: zona.into(),
            host: host.into(),
            jogadores,
            capacidade: 10,
            cheio,
        };
        let canais = vec![
            c("SA01/gelo1", "ilha_gelo", "h:9100", 0, false),
            c("SA01/1", "ilha_inicial", "h:9000", 7, false),
            c("SA01/2", "ilha_inicial", "h:9001", 3, false),
            c("BR1/1", "ilha_inicial", "b:9000", 0, false),
        ];
        assert_eq!(porta_de_entrada(&canais, "SA01").as_deref(), Some("h:9001"));
        assert_eq!(porta_de_entrada(&canais, "BR1").as_deref(), Some("b:9000"));
        assert_eq!(porta_de_entrada(&canais, "XX"), None);
        // Canal da ilha inicial lotado vem depois do que tem vaga.
        let lotado = vec![
            c("SA01/1", "ilha_inicial", "h:9000", 10, true),
            c("SA01/2", "ilha_inicial", "h:9001", 9, false),
        ];
        assert_eq!(porta_de_entrada(&lotado, "SA01").as_deref(), Some("h:9001"));
        // Sem ilha inicial no ar: entra pelo que houver.
        assert_eq!(porta_de_entrada(&canais[..1], "SA01").as_deref(), Some("h:9100"));
    }

    #[test]
    fn le_a_resposta_do_cadastro() {
        assert_eq!(interpreta_cadastro(201, ""), RespostaCadastro::Criada);
        assert_eq!(
            interpreta_cadastro(409, r#"{"error":"username ja existe"}"#),
            RespostaCadastro::JaExiste("username ja existe".into())
        );
        assert_eq!(
            interpreta_cadastro(400, r#"{"error":"senha deve ter 6-128 chars"}"#),
            RespostaCadastro::Recusado("senha deve ter 6-128 chars".into())
        );
        // Sem corpo util, ainda assim diz o que houve.
        assert!(matches!(interpreta_cadastro(502, ""), RespostaCadastro::Erro(_)));
    }

    /// Senha com aspas não quebra o JSON.
    ///
    /// É o caractere que mais aparece em senha gerada por gerenciador, e sem
    /// escape o servidor responderia 400 com uma mensagem que não explica
    /// nada — o jogador veria "dados inválidos" com dados válidos.
    #[test]
    fn senha_com_aspas_e_barra_sobrevive() {
        assert_eq!(escapa(r#"a"b\c"#), r#"a\"b\\c"#);
        assert_eq!(escapa("linha\nnova"), "linha\\nnova");
        // E o corpo montado continua sendo JSON legível pelo nosso próprio
        // leitor de campo.
        let corpo = format!(
            r#"{{"username":"{}","email":"{}","password":"{}"}}"#,
            escapa("zé"), escapa("a@b.c"), escapa(r#"se"nha"#)
        );
        assert_eq!(campo_json(&corpo, "password").as_deref(), Some(r#"se"nha"#));
        assert_eq!(campo_json(&corpo, "username").as_deref(), Some("zé"));
    }

    #[test]
    fn le_campos_e_respostas_do_login_google() {
        let ok = r#"{"status":"ok","username":"Joao_1","token":"abc\"def"}"#;
        assert_eq!(campo_json(ok, "username").as_deref(), Some("Joao_1"));
        assert_eq!(campo_json(ok, "token").as_deref(), Some("abc\"def"));
        assert_eq!(
            interpreta_poll(200, ok),
            RespostaPoll::Pronto {
                usuario: "Joao_1".into(),
                token: "abc\"def".into()
            }
        );
        assert_eq!(
            interpreta_poll(200, r#"{"status": "pendente"}"#),
            RespostaPoll::Pendente
        );
        assert_eq!(
            interpreta_poll(404, r#"{"status":"expirado"}"#),
            RespostaPoll::Expirado
        );
        assert_eq!(
            interpreta_poll(200, r#"{"status":"erro","error":"negado"}"#),
            RespostaPoll::Erro("negado".into())
        );
    }
}

/// Extrai os campos que interessam sem trazer um parser de JSON.
///
/// O formato e' do nosso proprio servidor e tem uma forma so'; um parser
/// completo aqui seria peso sem retorno.
fn parse(corpo: &str) -> Vec<Canal> {
    let mut saida = Vec::new();
    for bloco in corpo.split('{').skip(1) {
        let campo_txt = |chave: &str| -> Option<String> {
            let i = bloco.find(&format!("\"{chave}\""))? + chave.len() + 3;
            let resto = &bloco[i..];
            let ini = resto.find('"')? + 1;
            let fim = resto[ini..].find('"')? + ini;
            Some(resto[ini..fim].to_string())
        };
        let campo_num = |chave: &str| -> Option<u32> {
            let i = bloco.find(&format!("\"{chave}\""))? + chave.len() + 3;
            let resto = bloco[i..].trim_start_matches([':', ' ']);
            let fim = resto
                .find(|c: char| !c.is_ascii_digit())
                .unwrap_or(resto.len());
            resto[..fim].parse().ok()
        };
        let (Some(id), Some(host)) = (campo_txt("id"), campo_txt("host")) else {
            continue;
        };
        saida.push(Canal {
            id,
            zona: campo_txt("zone").unwrap_or_else(|| "?".into()),
            host,
            jogadores: campo_num("players").unwrap_or(0),
            capacidade: campo_num("capacity").unwrap_or(0),
            cheio: bloco.contains("\"full\": true") || bloco.contains("\"full\":true"),
        });
    }
    saida
}
