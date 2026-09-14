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
    /// Area de instancia unica: ali nao abre outro canal, lota e vira fila.
    /// Vem do servidor — antes o cliente adivinhava pela contagem de canais e
    /// errava com a zona vazia.
    pub unica: bool,
}

impl Canal {
    /// Nome do servidor (realm) e do canal, separados: o id vem como
    /// `SA01/2`.
    pub fn realm(&self) -> &str {
        self.id.split('/').next().unwrap_or(&self.id)
    }

    pub fn numero(&self) -> &str {
        self.id.split('/').nth(1).unwrap_or("1")
    }

    pub fn fracao(&self) -> f32 {
        if self.capacidade == 0 {
            0.0
        } else {
            self.jogadores as f32 / self.capacidade as f32
        }
    }
}

/// Onde o `web` atende. No iPhone nao ha variavel de ambiente nem servidor
/// local: vai direto no web de producao (docs/SERVIDORES_E_CANAIS.md).
fn endereco() -> String {
    #[cfg(target_os = "ios")]
    const PADRAO: &str = "mmo.brunji.com.br:80";
    #[cfg(not(target_os = "ios"))]
    const PADRAO: &str = "127.0.0.1:8080";
    std::env::var("MMO_API").unwrap_or_else(|_| PADRAO.into())
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
    fluxo.write_all(pedido.as_bytes()).map_err(|e| e.to_string())?;
    let mut resposta = String::new();
    fluxo.read_to_string(&mut resposta).map_err(|e| e.to_string())?;

    let (cabecalho, corpo) = resposta.split_once("\r\n\r\n").ok_or("resposta sem corpo")?;
    let codigo = cabecalho
        .split_whitespace()
        .nth(1)
        .and_then(|c| c.parse().ok())
        .ok_or("resposta sem codigo HTTP")?;
    // `Connection: close` evita chunked encoding, entao o corpo vem inteiro.
    Ok((codigo, corpo.to_string()))
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
        Some("erro") => RespostaPoll::Erro(campo_json(corpo, "error").unwrap_or_else(|| "erro".into())),
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
            _ => Err(campo_json(&corpo, "error").unwrap_or_else(|| format!("servidor respondeu {codigo}"))),
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
    fn le_campos_e_respostas_do_login_google() {
        let ok = r#"{"status":"ok","username":"Joao_1","token":"abc\"def"}"#;
        assert_eq!(campo_json(ok, "username").as_deref(), Some("Joao_1"));
        assert_eq!(campo_json(ok, "token").as_deref(), Some("abc\"def"));
        assert_eq!(interpreta_poll(200, ok), RespostaPoll::Pronto { usuario: "Joao_1".into(), token: "abc\"def".into() });
        assert_eq!(interpreta_poll(200, r#"{"status": "pendente"}"#), RespostaPoll::Pendente);
        assert_eq!(interpreta_poll(404, r#"{"status":"expirado"}"#), RespostaPoll::Expirado);
        assert_eq!(interpreta_poll(200, r#"{"status":"erro","error":"negado"}"#), RespostaPoll::Erro("negado".into()));
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
            let fim = resto.find(|c: char| !c.is_ascii_digit()).unwrap_or(resto.len());
            resto[..fim].parse().ok()
        };
        let (Some(id), Some(host)) = (campo_txt("id"), campo_txt("host")) else { continue };
        saida.push(Canal {
            id,
            zona: campo_txt("zone").unwrap_or_else(|| "?".into()),
            host,
            jogadores: campo_num("players").unwrap_or(0),
            capacidade: campo_num("capacity").unwrap_or(0),
            cheio: bloco.contains("\"full\": true") || bloco.contains("\"full\":true"),
            unica: bloco.contains("\"single\": true") || bloco.contains("\"single\":true"),
        });
    }
    saida
}
