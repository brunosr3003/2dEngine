//! WebSocket connection to the authoritative server.
//!
//! macroquad runs the render loop on the main thread and has no async runtime
//! of its own, so the socket lives on a dedicated thread that talks to the
//! game over channels. The thread owns the socket: it puts the TcpStream in
//! non-blocking mode and alternates between draining the outgoing queue and
//! reading frames.
//!
//! Encoding: `shared::protocol::{encode, decode}` — the SAME helpers the
//! server uses. There is no duplicate serialisation in this client.

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
    /// Payload bytes received since connecting. It is the number that decides
    /// whether the game fits in a data plan, so it is measured here, on the
    /// thread that actually reads the socket, and not estimated from the messages.
    bytes_in: Arc<AtomicU64>,
}

impl Net {
    /// Opens the connection on a background thread. Does not block the caller —
    /// connection errors arrive as `NetEvent::Disconnected`.
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
            // A clean close: the server ends the session and saves the character
            // instead of waiting for the socket to die by timeout.
            let _ = ws.close(None);
            let _ = ws.flush();
        });

        Self {
            tx_out,
            rx_in,
            bytes_in,
        }
    }

    /// Total bytes received so far. The caller takes the difference between two
    /// reads to get the rate.
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
        // TLS is not needed yet: in dev the server is a local ws://.
        _ => return Err("TLS connection not supported yet".into()),
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
        // Outgoing — drains everything the game queued this frame.
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

        // Incoming — reads what arrived without blocking.
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
                    let _ = tx_in.send(NetEvent::Disconnected("the server closed the connection".into()));
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

        // ~1ms of slack: the server ticks at 30Hz, there is no gain in spinning hot.
        std::thread::sleep(Duration::from_millis(1));
    }
}

/// A clock both threads read (ms since the first call). The `Ping` carries
/// it and the `Pong` is timed HERE, on arrival — not on the next frame: the
/// on-screen ping is the network's, not the network's plus a frame or two
/// (owner, 11/11/2026: "I'm in Minas and my ping sits at 70-90 ms").
pub fn relogio_ms() -> u64 {
    static INICIO: std::sync::OnceLock<std::time::Instant> = std::sync::OnceLock::new();
    INICIO.get_or_init(std::time::Instant::now).elapsed().as_millis() as u64
}

/// The last round trip, timed when the `Pong` arrived (0 = none yet).
pub static ULTIMO_RTT_MS: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

fn forward(tx_in: &Sender<NetEvent>, bytes: &[u8]) {
    match shared::protocol::decode::<ServerMessage>(bytes) {
        Ok(sm) => {
            if let ServerMessage::Pong { client_time_ms, .. } = &sm {
                let rtt = relogio_ms().saturating_sub(*client_time_ms).min(u32::MAX as u64) as u32;
                ULTIMO_RTT_MS.store(rtt.max(1), Ordering::Relaxed);
            }
            let _ = tx_in.send(NetEvent::Message(Box::new(sm)));
        }
        // An unknown message does not drop the session: the server may have
        // variants this client does not handle yet.
        Err(e) => eprintln!("[net] decode failed: {e}"),
    }
}

fn is_would_block(e: &tungstenite::Error) -> bool {
    matches!(e, tungstenite::Error::Io(io) if io.kind() == ErrorKind::WouldBlock)
}
