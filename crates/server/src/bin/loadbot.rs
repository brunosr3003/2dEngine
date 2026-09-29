//! Load bot: connects N fake players and measures the REAL cost per player.
//!
//! Everything measured so far (server CPU, bytes on the wire) was with ONE
//! player. The number that decides whether an MMO scales is another: building
//! a snapshot is O(players x visible entities), and that multiplication is
//! where an MMO server dies. This binary exists so that sum stops being theory.
//!
//! Each bot walks a real client's whole path — handshake, login, character
//! selection/creation — and then walks in circles sending input at 30Hz,
//! which is the worst case for the delta (an idle player generates no traffic).
//!
//! ```sh
//! cargo run --release --bin loadbot -- --n 50 --secs 30
//! # spread over several channels, the way the game really does it:
//! cargo run --release --bin loadbot -- --n 1000 --secs 120 \
//!   --hosts 127.0.0.1:9000,127.0.0.1:9001,127.0.0.1:9002
//! ```
//!
//! The accounts have to exist beforehand (`scripts/loadbot-accounts.sh`).

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
    // `--hosts` (plural) spreads the bots over the channels in rotation. One
    // channel only measures the ceiling of ONE process; what matters in a realm
    // test is the whole machine holding the divided load, which is how the game runs.
    let hosts: Vec<String> = arg("--hosts", &arg("--host", "127.0.0.1:9000"))
        .split(',')
        .map(|h| h.trim().to_string())
        .filter(|h| !h.is_empty())
        .collect();
    let secs: u64 = arg("--secs", "30").parse().expect("--secs");
    let senha = arg("--pass", "bruno123");
    // The first account number. Useful for running two loadbots at once without
    // both trying to log in as `bot0`.
    let offset: usize = arg("--offset", "0").parse().expect("--offset");
    // Interval between joins. 1000 bots hitting argon2 together would measure
    // the login queue, not the steady-state cost.
    let rampa: u64 = arg("--rampa-ms", "20").parse().expect("--rampa-ms");

    println!(
        "bringing up {n} bots for {secs}s on {} channel(s): {}",
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
                    eprintln!("bot failed: {e}");
                }
            }
        }));
        tokio::time::sleep(Duration::from_millis(rampa)).await;
    }

    // One sample per second, to watch the cost settle rather than only the average.
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
    println!("entered         {dentro}");
    println!("falhas          {}", stats.falhas.load(Ordering::Relaxed));
    println!("bytes received  {:.1} MB", b as f64 / 1024.0 / 1024.0);
    println!(
        "per player      {:.1} KB/s",
        b as f64 / 1024.0 / secs as f64 / dentro as f64
    );
    println!(
        "snapshots       {snaps}  ({:.1}/s por jogador)",
        snaps as f64 / secs as f64 / dentro as f64
    );
    println!(
        "states/snap     {:.1}",
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
    // Each bot walks a circle with its own phase: that way they spread over the
    // map instead of stacking on the same point, which would give an unrealistic AOI.
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
