//! Bot de carga: conecta N jogadores falsos e mede o custo REAL por jogador.
//!
//! Tudo que foi medido ate aqui (CPU do servidor, bytes no wire) foi com UM
//! jogador. O numero que decide se um MMO escala e' outro: montar snapshot e'
//! O(jogadores x entidades visiveis), e e' nessa multiplicacao que servidor de
//! MMO morre. Este binario existe pra essa conta parar de ser teoria.
//!
//! Cada bot faz o caminho inteiro de um cliente de verdade — handshake, login,
//! selecao/criacao de personagem — e depois anda em circulos mandando input a
//! 30Hz, que e' o pior caso pro delta (jogador parado nao gera trafego).
//!
//! ```sh
//! cargo run --release --bin loadbot -- --n 50 --secs 30
//! # espalhado por varios canais, como o jogo faz de verdade:
//! cargo run --release --bin loadbot -- --n 1000 --secs 120 \
//!     --hosts 127.0.0.1:9000,127.0.0.1:9001,127.0.0.1:9002
//! ```
//!
//! As contas precisam existir antes (`scripts/loadbot-accounts.sh`).

use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use futures_util::{SinkExt, StreamExt};
use shared::protocol::{ClientMessage, InputFrame, ServerMessage};
use tokio_tungstenite::tungstenite::Message;

#[derive(Default)]
struct Stats {
    bytes_in: AtomicU64,
    msgs_in: AtomicU64,
    snapshots: AtomicU64,
    states: AtomicU64,
    entered: AtomicU64,
    conectados: AtomicUsize,
    no_mundo: AtomicUsize,
    falhas: AtomicUsize,
}

#[tokio::main(flavor = "multi_thread")]
async fn main() {
    let args: Vec<String> = std::env::args().collect();
    let arg = |nome: &str, padrao: &str| -> String {
        args.iter()
            .position(|a| a == nome)
            .and_then(|i| args.get(i + 1))
            .cloned()
            .unwrap_or_else(|| padrao.to_string())
    };
    let n: usize = arg("--n", "50").parse().expect("--n");
    // `--hosts` (plural) espalha os bots pelos canais em rodizio. Um canal so'
    // mede o teto de UM processo; o que interessa num teste de realm e' a
    // maquina inteira segurando a carga dividida, que e' como o jogo roda.
    let hosts: Vec<String> = arg("--hosts", &arg("--host", "127.0.0.1:9000"))
        .split(',')
        .map(|h| h.trim().to_string())
        .filter(|h| !h.is_empty())
        .collect();
    let secs: u64 = arg("--secs", "30").parse().expect("--secs");
    let senha = arg("--pass", "bruno123");
    // Primeiro numero de conta. Serve pra rodar dois loadbots ao mesmo tempo
    // sem que os dois tentem logar como `bot0`.
    let offset: usize = arg("--offset", "0").parse().expect("--offset");
    // Intervalo entre entradas. 1000 bots batendo no argon2 juntos mediriam a
    // fila de login, nao o custo de regime.
    let rampa: u64 = arg("--rampa-ms", "20").parse().expect("--rampa-ms");

    println!(
        "subindo {n} bots por {secs}s em {} canal(is): {}",
        hosts.len(),
        hosts.join(", ")
    );
    let stats = Arc::new(Stats::default());

    let mut tasks = Vec::with_capacity(n);
    for i in 0..n {
        let st = stats.clone();
        let url = format!("ws://{}", hosts[i % hosts.len()]);
        let user = format!("bot{}", i + offset);
        let senha = senha.clone();
        tasks.push(tokio::spawn(async move {
            if let Err(e) = bot(url, user, senha, st.clone(), secs).await {
                st.falhas.fetch_add(1, Ordering::Relaxed);
                if st.falhas.load(Ordering::Relaxed) <= 3 {
                    eprintln!("bot falhou: {e}");
                }
            }
        }));
        tokio::time::sleep(Duration::from_millis(rampa)).await;
    }

    // Amostra por segundo, pra ver o custo estabilizar em vez de so' a media.
    let inicio = Instant::now();
    let mut ultimo_bytes = 0u64;
    for _ in 0..secs {
        tokio::time::sleep(Duration::from_secs(1)).await;
        let b = stats.bytes_in.load(Ordering::Relaxed);
        let dentro = stats.no_mundo.load(Ordering::Relaxed);
        println!(
            "  t={:>3}s  no mundo {:>3}  {:>8.1} KB/s total  {:>6.1} KB/s por jogador",
            inicio.elapsed().as_secs(),
            dentro,
            (b - ultimo_bytes) as f64 / 1024.0,
            if dentro > 0 {
                (b - ultimo_bytes) as f64 / 1024.0 / dentro as f64
            } else {
                0.0
            }
        );
        ultimo_bytes = b;
    }

    for t in tasks {
        let _ = t.await;
    }

    let dentro = stats.no_mundo.load(Ordering::Relaxed).max(1);
    let b = stats.bytes_in.load(Ordering::Relaxed);
    let snaps = stats.snapshots.load(Ordering::Relaxed).max(1);
    println!("\n── resultado ──────────────────────────────────────────");
    println!(
        "conectados      {}",
        stats.conectados.load(Ordering::Relaxed)
    );
    println!("entraram        {dentro}");
    println!("falhas          {}", stats.falhas.load(Ordering::Relaxed));
    println!("bytes recebidos {:.1} MB", b as f64 / 1024.0 / 1024.0);
    println!(
        "por jogador     {:.1} KB/s",
        b as f64 / 1024.0 / secs as f64 / dentro as f64
    );
    println!(
        "snapshots       {snaps}  ({:.1}/s por jogador)",
        snaps as f64 / secs as f64 / dentro as f64
    );
    println!(
        "estados/snap    {:.1}",
        stats.states.load(Ordering::Relaxed) as f64 / snaps as f64
    );
    println!(
        "metas/snap      {:.2}",
        stats.entered.load(Ordering::Relaxed) as f64 / snaps as f64
    );
}

async fn bot(
    url: String,
    user: String,
    senha: String,
    st: Arc<Stats>,
    secs: u64,
) -> anyhow::Result<()> {
    let (mut ws, _) = tokio_tungstenite::connect_async(&url).await?;
    st.conectados.fetch_add(1, Ordering::Relaxed);

    let envia = |m: &ClientMessage| -> anyhow::Result<Message> {
        Ok(Message::Binary(shared::protocol::encode(m)?))
    };
    ws.send(envia(&ClientMessage::Handshake {
        protocol_version: shared::PROTOCOL_VERSION,
        client_version: "loadbot".into(),
    })?)
    .await?;

    let fim = Instant::now() + Duration::from_secs(secs);
    let mut no_mundo = false;
    let mut seq = 0u32;
    let mut tick = 0u32;
    let mut input = tokio::time::interval(Duration::from_millis(33));
    // Cada bot anda num circulo de fase propria: assim eles se espalham pelo
    // mapa em vez de empilhar no mesmo ponto, que daria um AOI irrealista.
    let fase = (user.len() as f32 * 1.7) + user.bytes().map(|b| b as f32).sum::<f32>() * 0.013;

    while Instant::now() < fim {
        tokio::select! {
            _ = input.tick(), if no_mundo => {
                seq += 1;
                let t = seq as f32 * 0.03 + fase;
                ws.send(envia(&ClientMessage::Input { input: InputFrame {
                    seq, tick,
                    move_dir: glam::Vec2::new(t.cos(), t.sin()),
                    aim: glam::Vec2::X,
                    buttons: 0,
                }})?).await?;
            }
            frame = ws.next() => {
                let Some(frame) = frame else { break };
                let frame = frame?;
                let bytes = match &frame {
                    Message::Binary(b) => b.clone(),
                    Message::Text(t) => t.as_bytes().to_vec(),
                    Message::Close(_) => break,
                    _ => continue,
                };
                st.bytes_in.fetch_add(bytes.len() as u64, Ordering::Relaxed);
                st.msgs_in.fetch_add(1, Ordering::Relaxed);
                let Ok(msg) = shared::protocol::decode::<ServerMessage>(&bytes) else { continue };
                match msg {
                    ServerMessage::HandshakeAck { .. } => {
                        ws.send(envia(&ClientMessage::Login {
                            username: user.clone(),
                            password: senha.clone(),
                            lembrar: false,
                        })?).await?;
                    }
                    ServerMessage::CharacterList { chars, available_weapons, .. } => {
                        let m = match chars.first() {
                            Some(c) => ClientMessage::SelectCharacter { name: c.name.clone() },
                            None => ClientMessage::CreateCharacter {
                                name: format!("{user}_c"),
                                aparencia: Default::default(),
                                starting_weapon: available_weapons.first().copied().unwrap_or(0),
                                faction: Default::default(),
                            },
                        };
                        ws.send(envia(&m)?).await?;
                    }
                    ServerMessage::LoginOk { .. } => {
                        if !no_mundo {
                            no_mundo = true;
                            st.no_mundo.fetch_add(1, Ordering::Relaxed);
                        }
                    }
                    ServerMessage::Snapshot { snapshot } => {
                        tick = snapshot.tick;
                        st.snapshots.fetch_add(1, Ordering::Relaxed);
                        st.states.fetch_add(snapshot.states.len() as u64, Ordering::Relaxed);
                        st.entered.fetch_add(snapshot.entered.len() as u64, Ordering::Relaxed);
                    }
                    ServerMessage::Kick { reason } => {
                        anyhow::bail!("kick: {reason}");
                    }
                    ServerMessage::LoginDenied { reason } => {
                        anyhow::bail!("login negado: {reason}");
                    }
                    _ => {}
                }
            }
        }
    }
    Ok(())
}
