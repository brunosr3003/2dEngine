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
    let mut ws = tokio_tungstenite::accept_async_with_config(stream, Some(cfg)).await?;
    tracing::info!("ws accepted from {peer}");

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
                        Message::Close(_) => break,
                        _ => {}
                    }
                }
            }
        }
    }

    crate::telemetria::fechar_banda(&peer.to_string());
    let _ = to_world.send(IncomingMessage::Disconnected(session_id));
    tracing::info!("session {peer} closed");
    Ok(())
}
