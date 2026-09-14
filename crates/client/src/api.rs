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

/// Dispara a busca. O resultado (ou o erro) chega pelo receiver.
pub fn buscar_canais() -> Receiver<Result<Vec<Canal>, String>> {
    let (tx, rx) = mpsc::channel();
    // No iPhone nao ha variavel de ambiente nem servidor local: vai direto no
    // web de producao (docs/SERVIDORES_E_CANAIS.md).
    #[cfg(target_os = "ios")]
    const PADRAO: &str = "mmo.brunji.com.br:80";
    #[cfg(not(target_os = "ios"))]
    const PADRAO: &str = "127.0.0.1:8080";
    let endereco = std::env::var("MMO_API").unwrap_or_else(|_| PADRAO.into());
    std::thread::spawn(move || {
        let _ = tx.send(buscar(&endereco));
    });
    rx
}

fn buscar(endereco: &str) -> Result<Vec<Canal>, String> {
    let mut fluxo = TcpStream::connect(endereco).map_err(|e| format!("{endereco}: {e}"))?;
    fluxo
        .set_read_timeout(Some(Duration::from_secs(4)))
        .map_err(|e| e.to_string())?;
    let pedido = format!(
        "GET /api/channels HTTP/1.1\r\nHost: {endereco}\r\nConnection: close\r\n\r\n"
    );
    fluxo.write_all(pedido.as_bytes()).map_err(|e| e.to_string())?;
    let mut resposta = String::new();
    fluxo.read_to_string(&mut resposta).map_err(|e| e.to_string())?;

    let corpo = resposta
        .split_once("\r\n\r\n")
        .map(|(_, b)| b)
        .ok_or("resposta sem corpo")?;
    // `Connection: close` evita chunked encoding, entao o corpo vem inteiro.
    Ok(parse(corpo))
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
