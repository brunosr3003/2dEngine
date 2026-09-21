//! Bot do calendario de presenca: o teste de aceite de docs/CALENDARIO.md.
//!
//! Faz o caminho de um jogador: login, entra num personagem (cria se nao
//! existir), opcionalmente enche a bolsa pelo comando de admin (servidor de
//! TESTE), pede o estado e resgata `--vezes` vezes. Imprime tudo que o
//! servidor respondeu, o ouro e a bolsa no fim.
//!
//! ```sh
//! cargo run --bin presencabot -- --host 127.0.0.1:9311 --user presenca1 --char pres_a --vezes 2 --secret X
//! cargo run --bin presencabot -- --host 127.0.0.1:9311 --user presenca1 --char pres_b --encher --secret X
//! ```

use std::time::{Duration, Instant};

use futures_util::{SinkExt, StreamExt};
use shared::presenca::{AvisoPresenca, PedidoPresenca};
use shared::protocol::{AdminAction, ClientMessage, ServerMessage};
use tokio_tungstenite::tungstenite::Message;

#[tokio::main(flavor = "multi_thread")]
async fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let arg = |nome: &str, padrao: &str| -> String {
        args.iter()
            .position(|a| a == nome)
            .and_then(|i| args.get(i + 1))
            .cloned()
            .unwrap_or_else(|| padrao.to_string())
    };
    let tem = |nome: &str| args.iter().any(|a| a == nome);
    let host = arg("--host", "127.0.0.1:9311");
    let user = arg("--user", "presenca1");
    let senha = arg("--pass", "bruno123");
    let personagem = arg("--char", "pres_a");
    let segredo = arg("--secret", "");
    let vezes: u32 = arg("--vezes", "1").parse()?;
    // Espera antes de resgatar: da' tempo de dois bots em canais diferentes
    // chegarem juntos ao mesmo instante (`--em-unix`).
    let em_unix: u64 = arg("--em-unix", "0").parse()?;
    let segs: u64 = arg("--secs", "12").parse()?;
    let encher = tem("--encher");
    let limpar = tem("--limpar");

    let (mut ws, _) = tokio_tungstenite::connect_async(format!("ws://{host}")).await?;
    let envia = |m: ClientMessage| -> anyhow::Result<Message> {
        Ok(Message::Binary(shared::protocol::encode(&m)?))
    };
    ws.send(envia(ClientMessage::Handshake {
        protocol_version: shared::PROTOCOL_VERSION,
        client_version: "presencabot".into(),
    })?)
    .await?;

    let fim = Instant::now() + Duration::from_secs(segs);
    let mut no_mundo: Option<Instant> = None;
    let mut resgatou_pedidos = 0u32;
    let mut ouro = 0u64;
    let mut bolsa: Vec<(u16, u32)> = Vec::new();
    let mut correio: Vec<(u16, u32, u8)> = Vec::new();
    let mut linhas: Vec<String> = Vec::new();
    let mut tique = tokio::time::interval(Duration::from_millis(100));

    while Instant::now() < fim {
        tokio::select! {
            _ = tique.tick() => {
                let Some(t) = no_mundo else { continue };
                let pronto = t.elapsed() > Duration::from_millis(2500);
                let na_hora = em_unix == 0 || std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH)?.as_millis() as u64 >= em_unix * 1000;
                if pronto && na_hora && resgatou_pedidos < vezes {
                    resgatou_pedidos += 1;
                    linhas.push(format!("-> Resgatar #{resgatou_pedidos}"));
                    ws.send(envia(ClientMessage::Presenca { pedido: PedidoPresenca::Resgatar { calendario: 0 } })?).await?;
                    // Os pedidos seguintes saem logo em seguida (duplo toque).
                }
            }
            quadro = ws.next() => {
                let Some(quadro) = quadro else { break };
                let bytes = match quadro? {
                    Message::Binary(b) => b.to_vec(),
                    Message::Close(_) => break,
                    _ => continue,
                };
                let Ok(msg) = shared::protocol::decode::<ServerMessage>(&bytes) else { continue };
                match msg {
                    ServerMessage::HandshakeAck { .. } => {
                        ws.send(envia(ClientMessage::Login { username: user.clone(), password: senha.clone() })?).await?;
                    }
                    ServerMessage::CharacterList { chars, available_weapons } => {
                        let m = if chars.iter().any(|c| c.name == personagem) {
                            ClientMessage::SelectCharacter { name: personagem.clone() }
                        } else {
                            ClientMessage::CreateCharacter {
                                name: personagem.clone(),
                                aparencia: Default::default(),
                                starting_weapon: available_weapons.first().copied().unwrap_or(0),
                                faction: Default::default(),
                            }
                        };
                        ws.send(envia(m)?).await?;
                    }
                    ServerMessage::LoginOk { .. } => {
                        if no_mundo.is_none() {
                            no_mundo = Some(Instant::now());
                            linhas.push(format!("login ok como {personagem}"));
                            if limpar || encher {
                                ws.send(envia(ClientMessage::AdminCommand { secret: segredo.clone(), target_char: None, action: AdminAction::ClearInventory })?).await?;
                            }
                            if encher {
                                // 40 materiais diferentes (1 de cada): nenhum premio empilha.
                                for base in shared::item_id::MATERIAIS_COLORIDOS {
                                    for cor in 1..=4u8 {
                                        let item_id = shared::item_id::na_cor(base, cor);
                                        ws.send(envia(ClientMessage::AdminCommand { secret: segredo.clone(), target_char: None, action: AdminAction::GiveItem { item_id, qty: 1 } })?).await?;
                                    }
                                }
                            }
                        }
                    }
                    ServerMessage::GoldUpdate { gold } => ouro = gold,
                    ServerMessage::InventoryUpdate { slots } => {
                        bolsa = slots.iter().filter(|s| s.qty > 0).map(|s| (s.item_id, s.qty)).collect();
                    }
                    ServerMessage::Dungeon { aviso: shared::dungeon::Aviso::Correio { cartas } } => {
                        correio = cartas.iter().map(|c| (c.item_id, c.qtd, c.motivo)).collect();
                    }
                    ServerMessage::Presenca { aviso } => match aviso {
                        AvisoPresenca::Estado(e) => {
                            let c = e.calendarios.first();
                            linhas.push(format!("<- Estado: resgatados={:?} pode_hoje={:?}", c.map(|c| c.resgatados), c.map(|c| c.pode_hoje)));
                        }
                        AvisoPresenca::Resgatou { dia, premios, no_correio, .. } => {
                            linhas.push(format!("<- RESGATOU dia={dia} premios={:?} no_correio={no_correio}", premios.iter().map(|p| (p.item_id, p.qtd)).collect::<Vec<_>>()));
                        }
                        AvisoPresenca::Recusado { texto } => linhas.push(format!("<- RECUSADO: {texto}")),
                    },
                    _ => {}
                }
            }
        }
    }
    let _ = ws.send(envia(ClientMessage::RequestDisconnect)?).await;
    tokio::time::sleep(Duration::from_millis(500)).await;
    println!("── {user}/{personagem} @ {host}");
    for l in &linhas {
        println!("   {l}");
    }
    println!(
        "   ouro={ouro} itens_na_bolsa={} correio={correio:?}",
        bolsa.len()
    );
    Ok(())
}
