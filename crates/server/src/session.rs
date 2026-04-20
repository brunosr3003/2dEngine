//! Lida com uma conexao WebSocket: aceita, decodifica, encaminha mensagens
//! para o `world` e envia respostas. Nunca toca no ECS diretamente.

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
    let ws = tokio_tungstenite::accept_async(stream).await?;
    tracing::info!("ws accepted from {peer}");
    let (mut write, mut read) = ws.split();

    let (tx_out, mut rx_out) = mpsc::unbounded_channel::<ServerMessage>();
    let session_id = SessionId(peer);
    to_world.send(IncomingMessage::Connected(SessionHandle {
        id: session_id,
        to_client: tx_out.clone(),
    }))?;

    // Task de envio: serializa mensagens do world e escreve no socket.
    let sender_task = tokio::spawn(async move {
        while let Some(msg) = rx_out.recv().await {
            let bytes = match shared::protocol::encode(&msg) {
                Ok(b) => b,
                Err(e) => {
                    tracing::warn!("encode: {e}");
                    continue;
                }
            };
            if write.send(Message::Binary(bytes)).await.is_err() {
                break;
            }
        }
    });

    // Loop de leitura.
    while let Some(msg) = read.next().await {
        let msg = msg?;
        match msg {
            Message::Binary(b) => match shared::protocol::decode::<ClientMessage>(&b) {
                Ok(cm) => {
                    if to_world
                        .send(IncomingMessage::Message(session_id, cm))
                        .is_err()
                    {
                        break;
                    }
                }
                Err(e) => tracing::warn!("decode from {peer}: {e}"),
            },
            Message::Close(_) => break,
            _ => {}
        }
    }

    let _ = to_world.send(IncomingMessage::Disconnected(session_id));
    sender_task.abort();
    tracing::info!("session {peer} closed");
    Ok(())
}
