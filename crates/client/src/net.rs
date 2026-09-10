//! Conexao WebSocket com o servidor autoritativo.
//!
//! A macroquad roda o loop de render na thread principal e nao tem runtime
//! async proprio, entao o socket vive numa thread dedicada que fala com o
//! jogo por canais. A thread e' dona do socket: poe o TcpStream em
//! non-blocking e alterna entre drenar a fila de saida e ler frames.
//!
//! Codificacao: `shared::protocol::{encode, decode}` — os MESMOS helpers que
//! o servidor usa. Nao existe serializacao duplicada neste cliente.

use std::io::ErrorKind;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, Sender, TryRecvError};
use std::sync::Arc;
use std::time::Duration;

use shared::protocol::{ClientMessage, ServerMessage};
use tungstenite::stream::MaybeTlsStream;
use tungstenite::{Message, WebSocket};

pub enum NetEvent {
    Connected,
    Message(Box<ServerMessage>),
    Disconnected(String),
}

pub struct Net {
    tx_out: Sender<ClientMessage>,
    rx_in: Receiver<NetEvent>,
    /// Bytes de payload recebidos desde a conexao. E' o numero que decide se o
    /// jogo cabe num plano de dados, entao ele e' medido aqui, na thread que
    /// realmente le' o socket, e nao estimado a partir das mensagens.
    bytes_in: Arc<AtomicU64>,
}

impl Net {
    /// Abre a conexao numa thread de fundo. Nao bloqueia o chamador — os
    /// erros de conexao chegam como `NetEvent::Disconnected`.
    pub fn connect(url: String) -> Self {
        let (tx_out, rx_out) = mpsc::channel::<ClientMessage>();
        let (tx_in, rx_in) = mpsc::channel::<NetEvent>();
        let bytes_in = Arc::new(AtomicU64::new(0));
        let contador = bytes_in.clone();

        std::thread::spawn(move || {
            let mut ws = match tungstenite::connect(&url) {
                Ok((ws, _resp)) => ws,
                Err(e) => {
                    let _ = tx_in.send(NetEvent::Disconnected(format!("connect {url}: {e}")));
                    return;
                }
            };
            if let Err(e) = set_nonblocking(&mut ws) {
                let _ = tx_in.send(NetEvent::Disconnected(e));
                return;
            }
            let _ = tx_in.send(NetEvent::Connected);
            pump(&mut ws, &rx_out, &tx_in, &contador);
            // Close limpo: o servidor encerra a sessao e salva o personagem em
            // vez de esperar o socket morrer por timeout.
            let _ = ws.close(None);
            let _ = ws.flush();
        });

        Self { tx_out, rx_in, bytes_in }
    }

    /// Total de bytes recebidos ate agora. Quem chama tira a diferenca entre
    /// duas leituras pra ter a taxa.
    pub fn bytes_in(&self) -> u64 {
        self.bytes_in.load(Ordering::Relaxed)
    }

    pub fn send(&self, msg: ClientMessage) {
        let _ = self.tx_out.send(msg);
    }

    /// Eventos acumulados desde o ultimo frame. Nunca bloqueia.
    pub fn poll(&self) -> Vec<NetEvent> {
        self.rx_in.try_iter().collect()
    }
}

fn set_nonblocking(ws: &mut WebSocket<MaybeTlsStream<std::net::TcpStream>>) -> Result<(), String> {
    let stream = match ws.get_mut() {
        MaybeTlsStream::Plain(s) => s,
        // TLS ainda nao e' necessario: em dev o server e' ws:// local.
        _ => return Err("conexao TLS nao suportada ainda".into()),
    };
    stream
        .set_nonblocking(true)
        .map_err(|e| format!("set_nonblocking: {e}"))
}

fn pump(
    ws: &mut WebSocket<MaybeTlsStream<std::net::TcpStream>>,
    rx_out: &Receiver<ClientMessage>,
    tx_in: &Sender<NetEvent>,
    bytes_in: &AtomicU64,
) {
    loop {
        // Saida — drena tudo que o jogo enfileirou neste frame.
        loop {
            match rx_out.try_recv() {
                Ok(msg) => {
                    let bytes = match shared::protocol::encode(&msg) {
                        Ok(b) => b,
                        Err(e) => {
                            let _ = tx_in.send(NetEvent::Disconnected(format!("encode: {e}")));
                            return;
                        }
                    };
                    if let Err(e) = ws.send(Message::Binary(bytes)) {
                        if !is_would_block(&e) {
                            let _ = tx_in.send(NetEvent::Disconnected(format!("send: {e}")));
                            return;
                        }
                    }
                }
                Err(TryRecvError::Empty) => break,
                // O jogo fechou — encerra a thread.
                Err(TryRecvError::Disconnected) => return,
            }
        }
        let _ = ws.flush();

        // Entrada — le o que chegou sem bloquear.
        loop {
            match ws.read() {
                Ok(Message::Binary(b)) => {
                    bytes_in.fetch_add(b.len() as u64, Ordering::Relaxed);
                    forward(tx_in, &b)
                }
                Ok(Message::Text(t)) => {
                    bytes_in.fetch_add(t.len() as u64, Ordering::Relaxed);
                    forward(tx_in, t.as_bytes())
                }
                Ok(Message::Close(_)) => {
                    let _ = tx_in.send(NetEvent::Disconnected("server fechou".into()));
                    return;
                }
                Ok(_) => {}
                Err(e) if is_would_block(&e) => break,
                Err(e) => {
                    let _ = tx_in.send(NetEvent::Disconnected(format!("read: {e}")));
                    return;
                }
            }
        }

        // ~1ms de folga: o servidor tica a 30Hz, nao ha ganho em girar quente.
        std::thread::sleep(Duration::from_millis(1));
    }
}

fn forward(tx_in: &Sender<NetEvent>, bytes: &[u8]) {
    match shared::protocol::decode::<ServerMessage>(bytes) {
        Ok(sm) => {
            let _ = tx_in.send(NetEvent::Message(Box::new(sm)));
        }
        // Mensagem desconhecida nao derruba a sessao: o servidor pode ter
        // variantes que este cliente ainda nao trata.
        Err(e) => eprintln!("[net] decode falhou: {e}"),
    }
}

fn is_would_block(e: &tungstenite::Error) -> bool {
    matches!(e, tungstenite::Error::Io(io) if io.kind() == ErrorKind::WouldBlock)
}
