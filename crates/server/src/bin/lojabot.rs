//! Bot da loja de cash e das montarias: o teste de aceite de docs/LOJA.md e
//! docs/MONTARIAS.md, contra um servidor de TESTE.
//!
//! Cenarios (`--cenario`):
//! - `completo`: compra pacote de TP com o MESMO pedido duas vezes (conta um
//!   credito so'), compra o pergaminho de montaria duas vezes, mede a
//!   velocidade a pe' e montado, chama um chefe de teste perto e confere que
//!   desmonta ao lutar.
//! - `estado`: so' loga e imprime o saldo (reconexao mantem tudo).
//! - `pedido`: compra o pacote `--pacote` com o pedido fixo `--pedido` (dois
//!   processos da mesma conta ao mesmo tempo nao duplicam o credito).
//!
//! ```sh
//! cargo run --bin lojabot -- --host 127.0.0.1:9340 --user bot0 --cenario completo --secret X
//! cargo run --bin lojabot -- --host 127.0.0.1:9340 --user bot0 --cenario pedido --pedido fixo-abc-123 --pacote 1
//! ```

use std::collections::HashMap;
use std::time::{Duration, Instant};

use futures_util::{SinkExt, StreamExt};
use shared::loja::{AvisoLoja, PedidoLoja, Produto};
use shared::protocol::{AdminAction, ClientMessage, ServerMessage};
use shared::{ent_flags, EntityId, EntityTag};
use tokio_tungstenite::tungstenite::Message;

#[derive(Debug, Clone, Copy, PartialEq)]
enum Fase {
    Entrando,
    EstadoInicial,
    CompraTp,
    CompraMontaria,
    CompraRepetida,
    APe,
    Montando,
    Montado,
    Viagem,
    Luta,
    Fim,
}

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
    let host = arg("--host", "127.0.0.1:9340");
    let user = arg("--user", "bot0");
    let senha = arg("--pass", "bruno123");
    let segredo = arg("--secret", "");
    let cenario = arg("--cenario", "completo");
    let pedido_fixo = arg("--pedido", "");
    let pacote: u16 = arg("--pacote", "2").parse()?;
    let secs: u64 = arg("--secs", "60").parse()?;
    let tag = format!(
        "{:x}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_nanos()
    );

    let (mut ws, _) = tokio_tungstenite::connect_async(format!("ws://{host}")).await?;
    let envia = |m: ClientMessage| -> anyhow::Result<Message> {
        Ok(Message::Binary(shared::protocol::encode(&m)?))
    };
    ws.send(envia(ClientMessage::Handshake {
        protocol_version: shared::PROTOCOL_VERSION,
        client_version: "lojabot".into(),
    })?)
    .await?;

    let fim = Instant::now() + Duration::from_secs(secs);
    let mut linhas: Vec<String> = Vec::new();
    let mut fase = Fase::Entrando;
    let mut fase_em = Instant::now();
    let mut eu: Option<EntityId> = None;
    let mut ents: HashMap<EntityId, (EntityTag, glam::Vec2)> = HashMap::new();
    let (mut minha_pos, mut meus_flags) = (glam::Vec2::ZERO, 0u8);
    let (mut seq, mut tick_srv) = (0u32, 0u32);
    let mut andar = glam::Vec2::ZERO;
    let mut tp_inicial: Option<u64> = None;
    let mut tp_atual = 0u64;
    let mut estados = 0u32;
    let mut medida: Option<(Instant, glam::Vec2)> = None;
    let (mut vel_a_pe, mut vel_montado) = (0.0f32, 0.0f32);
    let mut desmontou_lutando = false;
    let mut origem = glam::Vec2::ZERO;
    let mut entrada = tokio::time::interval(Duration::from_millis(50));
    let mut tique = tokio::time::interval(Duration::from_millis(200));

    let muda = |f: &mut Fase, em: &mut Instant, nova: Fase, linhas: &mut Vec<String>| {
        linhas.push(format!("fase {nova:?}"));
        *f = nova;
        *em = Instant::now();
    };

    while Instant::now() < fim && fase != Fase::Fim {
        tokio::select! {
            _ = entrada.tick(), if eu.is_some() => {
                seq += 1;
                ws.send(envia(ClientMessage::Input { input: shared::protocol::InputFrame {
                    seq, tick: tick_srv, move_dir: andar, aim: glam::Vec2::X, buttons: 0,
                }})?).await?;
            }
            _ = tique.tick() => {
                if eu.is_none() { continue; }
                let t = fase_em.elapsed();
                match fase {
                    Fase::EstadoInicial if t > Duration::from_secs(2) && estados == 0 => {
                        ws.send(envia(ClientMessage::Loja { pedido: PedidoLoja::Estado })?).await?;
                        fase_em = Instant::now();
                    }
                    Fase::CompraTp if t > Duration::from_secs(3) => {
                        linhas.push(format!("TP depois de 2 pedidos iguais do pacote {pacote}: {tp_atual} (antes {})", tp_inicial.unwrap_or(0)));
                        if cenario == "pedido" {
                            muda(&mut fase, &mut fase_em, Fase::Fim, &mut linhas);
                        } else {
                            // A montaria virou ITEM: compra-se o pergaminho, e
                            // ele e' repetivel (docs/MONTARIAS.md).
                            muda(&mut fase, &mut fase_em, Fase::CompraMontaria, &mut linhas);
                            ws.send(envia(ClientMessage::Loja { pedido: PedidoLoja::ComprarItem { produto: Produto::PergaminhoMontaria(1), vezes: 1, pedido: format!("{tag}-m1") } })?).await?;
                        }
                    }
                    Fase::CompraMontaria if t > Duration::from_secs(2) => {
                        muda(&mut fase, &mut fase_em, Fase::CompraRepetida, &mut linhas);
                        ws.send(envia(ClientMessage::Loja { pedido: PedidoLoja::ComprarItem { produto: Produto::PergaminhoMontaria(1), vezes: 1, pedido: format!("{tag}-m1b") } })?).await?;
                    }
                    Fase::CompraRepetida if t > Duration::from_secs(2) => {
                        linhas.push(format!("TP {tp_atual}"));
                        muda(&mut fase, &mut fase_em, Fase::APe, &mut linhas);
                        andar = -glam::Vec2::X;
                        medida = None;
                    }
                    Fase::APe => {
                        if medida.is_none() && t > Duration::from_millis(600) { medida = Some((Instant::now(), minha_pos)); }
                        if t > Duration::from_millis(3600) {
                            if let Some((m0, p0)) = medida { vel_a_pe = minha_pos.distance(p0) / m0.elapsed().as_secs_f32(); }
                            andar = glam::Vec2::ZERO;
                            muda(&mut fase, &mut fase_em, Fase::Montando, &mut linhas);
                            ws.send(envia(ClientMessage::Loja { pedido: PedidoLoja::Montar })?).await?;
                        }
                    }
                    Fase::Montando => {
                        if meus_flags & ent_flags::MONTADO != 0 {
                            linhas.push(format!("montou em {:.2}s", t.as_secs_f32()));
                            muda(&mut fase, &mut fase_em, Fase::Montado, &mut linhas);
                            andar = glam::Vec2::X;
                            medida = None;
                            origem = minha_pos;
                        } else if t > Duration::from_secs(6) {
                            linhas.push("NAO montou em 6 s".into());
                            muda(&mut fase, &mut fase_em, Fase::Fim, &mut linhas);
                        }
                    }
                    Fase::Montado => {
                        if medida.is_none() && t > Duration::from_millis(600) { medida = Some((Instant::now(), minha_pos)); }
                        if t > Duration::from_millis(3600) {
                            if let Some((m0, p0)) = medida { vel_montado = minha_pos.distance(p0) / m0.elapsed().as_secs_f32(); }
                            andar = glam::Vec2::ZERO;
                            linhas.push(format!("velocidade a pe' {vel_a_pe:.2} u/s, montado {vel_montado:.2} u/s, razao {:.2}", vel_montado / vel_a_pe.max(0.01)));
                            // Sai da zona segura montado (la' dentro nao ha' combate).
                            muda(&mut fase, &mut fase_em, Fase::Viagem, &mut linhas);
                            origem = minha_pos;
                        }
                    }
                    Fase::Viagem => {
                        let longe = minha_pos.distance(origem);
                        let fora = shared::terreno::Cidade::RAIO + 25.0;
                        if longe > fora || t > Duration::from_secs(70) {
                            linhas.push(format!("viajou montado {longe:.0} u (ainda montado: {})", meus_flags & ent_flags::MONTADO != 0));
                            muda(&mut fase, &mut fase_em, Fase::Luta, &mut linhas);
                            let alvo = minha_pos + glam::Vec2::new(2.5, 0.0);
                            ws.send(envia(ClientMessage::AdminCommand { secret: segredo.clone(), target_char: None, action: AdminAction::SpawnTestBoss { x: alvo.x, z: alvo.y, hp: 1000 } })?).await?;
                        } else {
                            // Tenta uma direcao por vez; a que andar, segue.
                            let dir = [glam::Vec2::new(-1.0, 0.0), glam::Vec2::new(0.0, 1.0), glam::Vec2::new(0.0, -1.0), glam::Vec2::new(1.0, 0.0)][(t.as_secs() / 17) as usize % 4];
                            let destino = origem + dir * (fora + 20.0);
                            ws.send(envia(ClientMessage::MoverPara { x: destino.x, z: destino.y })?).await?;
                        }
                    }
                    Fase::Luta => {
                        // O chefe de teste mais perto vira alvo; o auto-ataque bate.
                        if let Some((id, p)) = ents.iter().filter(|(_, e)| e.0 == EntityTag::Enemy).min_by(|a, b| a.1 .1.distance(minha_pos).total_cmp(&b.1 .1.distance(minha_pos))).map(|(id, e)| (*id, e.1)) {
                            ws.send(envia(ClientMessage::SetTarget { target: Some(id) })?).await?;
                            if p.distance(minha_pos) > 1.5 {
                                ws.send(envia(ClientMessage::MoverPara { x: p.x, z: p.y })?).await?;
                            }
                        }
                        if meus_flags & ent_flags::MONTADO == 0 {
                            desmontou_lutando = true;
                            linhas.push(format!("desmontou lutando em {:.2}s", t.as_secs_f32()));
                            muda(&mut fase, &mut fase_em, Fase::Fim, &mut linhas);
                        } else if t > Duration::from_secs(15) {
                            linhas.push("NAO desmontou em 15 s de luta".into());
                            muda(&mut fase, &mut fase_em, Fase::Fim, &mut linhas);
                        }
                    }
                    _ => {}
                }
            }
            quadro = ws.next() => {
                let Some(quadro) = quadro else { break };
                let bytes = match quadro? { Message::Binary(b) => b.to_vec(), Message::Close(_) => break, _ => continue };
                let Ok(msg) = shared::protocol::decode::<ServerMessage>(&bytes) else {
                    if std::env::var("LJ_DEBUG").is_ok() { eprintln!("decode failed ({} bytes)", bytes.len()); }
                    continue;
                };
                if std::env::var("LJ_DEBUG").is_ok() && !matches!(msg, ServerMessage::Snapshot { .. }) {
                    let t = format!("{msg:?}");
                    eprintln!("<- {}", &t[..t.len().min(140)]);
                }
                match msg {
                    ServerMessage::HandshakeAck { .. } => {
                        ws.send(envia(ClientMessage::Login { username: user.clone(), password: senha.clone(), lembrar: false })?).await?;
                    }
                    ServerMessage::CharacterList { chars, available_weapons, .. } => {
                        let m = match chars.first() {
                            Some(c) => ClientMessage::SelectCharacter { name: c.name.clone() },
                            None => ClientMessage::CreateCharacter {
                                name: format!("{user}_lj"),
                                aparencia: Default::default(),
                                starting_weapon: available_weapons.first().copied().unwrap_or(0),
                                faction: Default::default(),
                            },
                        };
                        ws.send(envia(m)?).await?;
                    }
                    ServerMessage::LoginOk { .. } if fase == Fase::Entrando => {
                        muda(&mut fase, &mut fase_em, Fase::EstadoInicial, &mut linhas);
                    }
                    ServerMessage::Snapshot { snapshot } => {
                        tick_srv = snapshot.tick;
                        for m in snapshot.entered {
                            ents.entry(m.id).or_insert((m.tag, glam::Vec2::ZERO)).0 = m.tag;
                        }
                        for st in snapshot.states {
                            if st.flags & ent_flags::SELF != 0 {
                                eu = Some(st.id);
                                minha_pos = st.pos_f32();
                                meus_flags = st.flags;
                            }
                            if let Some(e) = ents.get_mut(&st.id) { e.1 = st.pos_f32(); }
                        }
                        for id in snapshot.removed { ents.remove(&id); }
                    }
                    ServerMessage::Loja { aviso } => match aviso {
                        AvisoLoja::Estado(e) => {
                            estados += 1;
                            tp_atual = e.tp;
                            linhas.push(format!("estado: ligada={} simulado={} TP={}", e.ligada, e.simulado, e.tp));
                            if tp_inicial.is_none() {
                                tp_inicial = Some(e.tp);
                                if cenario == "estado" {
                                    muda(&mut fase, &mut fase_em, Fase::Fim, &mut linhas);
                                } else if fase == Fase::EstadoInicial {
                                    let pedido = if pedido_fixo.is_empty() { format!("{tag}-tp") } else { pedido_fixo.clone() };
                                    muda(&mut fase, &mut fase_em, Fase::CompraTp, &mut linhas);
                                    // O mesmo pedido duas vezes seguidas: clique duplo.
                                    for _ in 0..if cenario == "pedido" { 1 } else { 2 } {
                                        ws.send(envia(ClientMessage::Loja { pedido: PedidoLoja::ComprarTp { pacote, pedido: pedido.clone() } })?).await?;
                                        tokio::time::sleep(Duration::from_millis(350)).await;
                                    }
                                }
                            }
                        }
                        AvisoLoja::Resultado { ok, texto } => linhas.push(format!("resultado ok={ok}: {texto}")),
                        AvisoLoja::Montando { segundos } => linhas.push(format!("montando {segundos:.1}s")),
                        AvisoLoja::Invocacao { premio } => {
                            linhas.push(format!("invocacao: {premio:?}"));
                        }
                        AvisoLoja::MontariaCombate { .. } => {}
                        AvisoLoja::Invocacoes { premios } => {
                            linhas.push(format!("invocacoes: {premios:?}"));
                        }
                    },
                    ServerMessage::Kick { reason } => anyhow::bail!("kick: {reason}"),
                    ServerMessage::LoginDenied { reason } => anyhow::bail!("login negado: {reason}"),
                    _ => {}
                }
            }
        }
    }
    println!("── lojabot {user} ({cenario}) ──");
    for l in &linhas {
        println!("  · {l}");
    }
    println!("  RESUMO tp_inicial={:?} tp_final={tp_atual} vel_a_pe={vel_a_pe:.2} vel_montado={vel_montado:.2} desmontou_lutando={desmontou_lutando}", tp_inicial);
    Ok(())
}
