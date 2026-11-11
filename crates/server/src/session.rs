//! Handles one WebSocket connection: accepts, decodes, forwards messages to
//! `world` and sends replies. Never touches the ECS directly.

use crate::world::{IncomingMessage, SessionHandle, SessionId};
use anyhow::Result;
use futures_util::{SinkExt, StreamExt};
use shared::protocol::{ClientMessage, ServerMessage};
use std::net::SocketAddr;
use tokio::net::TcpStream;
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::Message;

pub async fn handle_connection(
    stream: TcpStream,
    peer: SocketAddr,
    to_world: mpsc::UnboundedSender<IncomingMessage>,
) -> Result<()> {
    stream.set_nodelay(true)?;
    let mut cfg = tokio_tungstenite::tungstenite::protocol::WebSocketConfig::default();
    cfg.accept_unmasked_frames = true;
    // The player's REAL address: nginx sends it as `X-Real-IP` (every
    // connection here comes from 127.0.0.1). For the latency log only.
    let ip_real = std::sync::Arc::new(parking_lot::Mutex::new(None::<String>));
    let ip_cb = ip_real.clone();
    let cb = move |req: &tokio_tungstenite::tungstenite::handshake::server::Request,
                   resp: tokio_tungstenite::tungstenite::handshake::server::Response| {
        if let Some(v) = req.headers().get("x-real-ip").and_then(|v| v.to_str().ok()) {
            *ip_cb.lock() = Some(v.chars().take(64).collect());
        }
        Ok(resp)
    };
    let mut ws = tokio_tungstenite::accept_hdr_async_with_config(stream, cb, Some(cfg)).await?;
    tracing::info!("ws accepted from {peer}");
    let ip = ip_real.lock().clone().unwrap_or_else(|| peer.ip().to_string());
    crate::latencia::abriu(peer, ip);
    // A WebSocket ping every 15 s, answered by the client's network thread:
    // the pure network round trip, without the client's frames.
    let relogio_inicio = std::time::Instant::now();
    let mut relogio = tokio::time::interval(std::time::Duration::from_secs(15));

    let (tx_out, mut rx_out) = mpsc::unbounded_channel::<ServerMessage>();
    let session_id = SessionId(peer);
    // This connection's bandwidth, for the panoptico: atomic, no lock on send.
    let banda = crate::telemetria::abrir_banda(&peer.to_string());
    to_world.send(IncomingMessage::Connected(SessionHandle {
        id: session_id,
        to_client: tx_out.clone(),
    }))?;

    loop {
        tokio::select! {
            _ = relogio.tick() => {
                let t = relogio_inicio.elapsed().as_millis() as u64;
                if ws.send(Message::Ping(t.to_le_bytes().to_vec())).await.is_err() {
                    break;
                }
            }
            // Message arriving from the world to send to the client
            Some(server_msg) = rx_out.recv() => {
                let bytes = match shared::protocol::encode(&server_msg) {
                    Ok(b) => b,
                    Err(e) => { tracing::warn!("encode: {e}"); continue; }
                };
                // BINARY, not text. With JSON it could be sent as text; the wire is postcard
                // now and `from_utf8_lossy` would destroy the bytes that are not valid UTF-8
                // — the client froze on the handshake with no error appearing at all.
                banda.enviou(bytes.len());
                if ws.send(Message::Binary(bytes)).await.is_err() {
                    break;
                }
            }

            // Frame arriving from the client
            frame = ws.next() => {
                match frame {
                    None => break,
                    Some(Err(e)) => {
                        tracing::debug!("session {peer} ended: {e}");
                        break;
                    }
                    Some(Ok(msg)) => match msg {
                        Message::Binary(b) => match { banda.recebeu(b.len()); shared::protocol::decode::<ClientMessage>(&b) } {
                            // PING answered right here, not on the next world
                            // tick (up to 33 ms at 30 Hz): the on-screen ping
                            // is the network's.
                            Ok(ClientMessage::Ping { client_time_ms }) => {
                                let pong = ServerMessage::Pong {
                                    client_time_ms,
                                    server_time_ms: std::time::SystemTime::now()
                                        .duration_since(std::time::UNIX_EPOCH)
                                        .map_or(0, |d| d.as_millis() as u64),
                                };
                                if let Ok(bytes) = shared::protocol::encode(&pong) {
                                    banda.enviou(bytes.len());
                                    if ws.send(Message::Binary(bytes)).await.is_err() {
                                        break;
                                    }
                                }
                            }
                            Ok(cm) => { if to_world.send(IncomingMessage::Message(session_id, cm)).is_err() { break; } }
                            Err(e) => tracing::warn!("decode binary from {peer}: {e}"),
                        },
                        Message::Text(t) => match shared::protocol::decode::<ClientMessage>(t.as_bytes()) {
                            Ok(cm) => { if to_world.send(IncomingMessage::Message(session_id, cm)).is_err() { break; } }
                            Err(e) => tracing::warn!("decode text from {peer}: {e}"),
                        },
                        Message::Ping(data) => {
                            let _ = ws.send(Message::Pong(data)).await;
                        }
                        Message::Pong(data) => {
                            if let Ok(b) = <[u8; 8]>::try_from(&data[..]) {
                                let enviado = u64::from_le_bytes(b);
                                let agora = relogio_inicio.elapsed().as_millis() as u64;
                                crate::latencia::rede(peer, agora.saturating_sub(enviado) as u32);
                            }
                        }
                        Message::Close(_) => break,
                        _ => {}
                    }
                }
            }
        }
    }

    crate::telemetria::fechar_banda(&peer.to_string());
    crate::latencia::fechou(peer);
    let _ = to_world.send(IncomingMessage::Disconnected(session_id));
    tracing::info!("session {peer} closed");
    Ok(())
}
