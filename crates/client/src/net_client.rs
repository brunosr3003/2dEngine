//! Cliente WebSocket native. Roda em thread dedicada com um runtime tokio
//! para nao misturar com o event loop do winit.
//!
//! Interface publica:
//! - `connect(url)` -> `NetClient`
//! - `send(msg)`
//! - `incoming.try_recv()` para drenar mensagens no update do jogo.
//!
//! Para wasm, substituir esta impl por uma que use `web_sys::WebSocket`
//! direto (atras de um cfg).

use anyhow::Result;
use shared::protocol::{ClientMessage, ServerMessage};
use std::sync::{Arc, Mutex};
use tokio::sync::mpsc;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NetStatus {
    Connecting,
    Connected,
    Disconnected(String),
}

pub struct NetClient {
    pub incoming: mpsc::UnboundedReceiver<ServerMessage>,
    outgoing: mpsc::UnboundedSender<ClientMessage>,
    status: Arc<Mutex<NetStatus>>,
}

impl NetClient {
    pub fn status(&self) -> NetStatus {
        self.status.lock().map(|s| s.clone()).unwrap_or(NetStatus::Disconnected("poisoned".into()))
    }

    pub fn send(&self, msg: ClientMessage) {
        let _ = self.outgoing.send(msg);
    }
}

#[cfg(not(target_family = "wasm"))]
pub fn connect(url: String) -> Result<NetClient> {
    use futures_util::{SinkExt, StreamExt};
    use tokio_tungstenite::connect_async;
    use tokio_tungstenite::tungstenite::Message;

    let (tx_in, rx_in) = mpsc::unbounded_channel::<ServerMessage>();
    let (tx_out, mut rx_out) = mpsc::unbounded_channel::<ClientMessage>();
    let status = Arc::new(Mutex::new(NetStatus::Connecting));
    let status_t = status.clone();

    std::thread::Builder::new()
        .name("net-client".into())
        .spawn(move || {
            let rt = match tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
            {
                Ok(rt) => rt,
                Err(e) => {
                    *status_t.lock().unwrap() =
                        NetStatus::Disconnected(format!("rt build: {e}"));
                    return;
                }
            };
            rt.block_on(async move {
                let ws = match connect_async(&url).await {
                    Ok((ws, _)) => ws,
                    Err(e) => {
                        *status_t.lock().unwrap() =
                            NetStatus::Disconnected(format!("connect: {e}"));
                        return;
                    }
                };
                *status_t.lock().unwrap() = NetStatus::Connected;
                let (mut write, mut read) = ws.split();

                let status_sender = status_t.clone();
                let send_task = tokio::spawn(async move {
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
                    let _ = status_sender;
                });

                while let Some(msg) = read.next().await {
                    let Ok(m) = msg else { break };
                    if let Message::Binary(b) = m {
                        match shared::protocol::decode::<ServerMessage>(&b) {
                            Ok(sm) => {
                                if tx_in.send(sm).is_err() {
                                    break;
                                }
                            }
                            Err(e) => tracing::warn!("decode: {e}"),
                        }
                    }
                }

                send_task.abort();
                *status_t.lock().unwrap() = NetStatus::Disconnected("stream closed".into());
            });
        })?;

    Ok(NetClient { incoming: rx_in, outgoing: tx_out, status })
}
