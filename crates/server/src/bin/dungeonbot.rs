//! Bots de dungeon: o teste de aceite da F1/F2 (docs/DUNGEONS_E_RAIDS.md).
//!
//! Each bot walks a player's path: login, levels up through the admin command
//! (TEST server), asks for the Grotto queue (or enters the Cellar), accepts
//! the ready-check, fights floor by floor aiming at the nearest enemy,
//! revives when it can, opens the chest and leaves. At the end it prints what
//! each one saw.
//!
//! ```sh
//! # 5 bots in the Grotto (queue), 1 in the Cellar:
//! cargo run --bin dungeonbot -- --host 127.0.0.1:9300 --n 5 --cenario gruta --secret X
//! cargo run --bin dungeonbot -- --host 127.0.0.1:9300 --n 1 --offset 10 --cenario porao --secret X
//! # idle (server with a high MMO_DUNGEON_TESTE_DANO and a short LIMITE): wipe and time out
//! cargo run --bin dungeonbot -- --host 127.0.0.1:9301 --n 3 --offset 20 --cenario parado --secret X
//! ```

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use futures_util::{SinkExt, StreamExt};
use shared::dungeon::{self as dg, Aviso, Pedido};
use shared::protocol::{AdminAction, ClientMessage, ServerMessage};
use shared::{EntityId, EntityTag};
use tokio_tungstenite::tungstenite::Message;

#[derive(Default, Debug, Clone)]
struct Relato {
    bot: String,
    entrou: bool,
    andares: Vec<u8>,
    inimigos_no_inicio_do_andar: Vec<(u8, u16)>,
    mortes: u32,
    revividas: u32,
    wipes: u32,
    resultado: Option<(bool, u32)>,
    bau: Option<(Vec<(u16, u32)>, u32)>,
    saiu: bool,
    textos: Vec<String>,
    /// Position 4 s after entering (collision test between instances).
    pos_dentro: Option<(f32, f32)>,
    esperas_de_reviver: Vec<u16>,
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
    // `--hash SENHA`: prints the argon2 to create the test accounts in the database.
    if let Some(senha) = args
        .iter()
        .position(|a| a == "--hash")
        .and_then(|i| args.get(i + 1))
    {
        use argon2::password_hash::{PasswordHasher, SaltString};
        let sal = SaltString::from_b64("ZHVuZ2VvbmJvdHRlc3RlMTIz").expect("sal");
        println!(
            "{}",
            argon2::Argon2::default()
                .hash_password(senha.as_bytes(), &sal)
                .expect("hash")
        );
        return;
    }
    let host = arg("--host", "127.0.0.1:9300");
    let n: usize = arg("--n", "5").parse().expect("--n");
    let offset: usize = arg("--offset", "0").parse().expect("--offset");
    let cenario = arg("--cenario", "gruta");
    let senha = arg("--pass", "bruno123");
    let segredo = arg("--secret", "");
    let nivel: u32 = arg("--nivel", "16").parse().expect("--nivel");
    let secs: u64 = arg("--secs", "900").parse().expect("--secs");
    // Content: 10 = Sea Wolves' Den (Grotto, queue), 1 = Cellar (solo).
    let conteudo: u16 = arg("--conteudo", if cenario == "porao" { "1" } else { "10" })
        .parse()
        .expect("--conteudo");
    let relatos = Arc::new(Mutex::new(Vec::<Relato>::new()));
    let mut tarefas = Vec::new();
    for i in 0..n {
        let (host, senha, segredo, cenario, relatos) = (
            host.clone(),
            senha.clone(),
            segredo.clone(),
            cenario.clone(),
            relatos.clone(),
        );
        let conteudo = conteudo;
        let user = format!("bot{}", i + offset);
        tarefas.push(tokio::spawn(async move {
            let mut r = Relato {
                bot: user.clone(),
                ..Default::default()
            };
            if let Err(e) = bot(
                &host, &user, &senha, &segredo, &cenario, conteudo, nivel, secs, i, &mut r,
            )
            .await
            {
                r.textos.push(format!("ERRO: {e}"));
            }
            relatos.lock().unwrap().push(r);
        }));
        tokio::time::sleep(Duration::from_millis(250)).await;
    }
    for t in tarefas {
        let _ = t.await;
    }
    let mut rs = relatos.lock().unwrap().clone();
    rs.sort_by(|a, b| a.bot.cmp(&b.bot));
    println!("\n── relatorio ({cenario}) ──────────────────────────");
    for r in &rs {
        println!(
            "{}: entrou={} andares={:?} inimigos/andar={:?} mortes={} revividas={} wipes={} esperas={:?} resultado={:?} bau={:?} saiu={} pos_dentro={:?}",
            r.bot, r.entrou, r.andares, r.inimigos_no_inicio_do_andar, r.mortes, r.revividas, r.wipes, r.esperas_de_reviver, r.resultado, r.bau, r.saiu, r.pos_dentro
        );
        for t in &r.textos {
            println!("    · {t}");
        }
    }
}

struct Ent {
    tag: EntityTag,
    kind: u16,
    pos: glam::Vec2,
    hp: u16,
}

#[allow(clippy::too_many_arguments)]
async fn bot(
    host: &str,
    user: &str,
    senha: &str,
    segredo: &str,
    cenario: &str,
    conteudo: u16,
    nivel: u32,
    secs: u64,
    indice: usize,
    r: &mut Relato,
) -> anyhow::Result<()> {
    let (mut ws, _) = tokio_tungstenite::connect_async(format!("ws://{host}")).await?;
    let envia = |m: ClientMessage| -> anyhow::Result<Message> {
        Ok(Message::Binary(shared::protocol::encode(&m)?))
    };
    ws.send(envia(ClientMessage::Handshake {
        protocol_version: shared::PROTOCOL_VERSION,
        client_version: "dungeonbot".into(),
    })?)
    .await?;

    let fim = Instant::now() + Duration::from_secs(secs);
    let mut ents: HashMap<EntityId, Ent> = HashMap::new();
    let mut eu: Option<EntityId> = None;
    let mut no_mundo_em: Option<Instant> = None;
    let mut pediu = false;
    let mut alvo: Option<EntityId> = None;
    let mut na_instancia = false;
    let mut andar_visto: Option<u8> = None;
    let mut caido = false;
    let mut venceu_em: Option<Instant> = None;
    let mut acabou_em: Option<Instant> = None;
    let mut tique = tokio::time::interval(Duration::from_millis(400));
    // The server only processes attacks and routes from whoever sends input
    // (the client always sends, idle or not).
    let mut entrada = tokio::time::interval(Duration::from_millis(50));
    let (mut seq, mut tick_srv) = (0u32, 0u32);
    // parado: neither fights nor revives (times out). colisao: the same, only
    // measures the position. sala: 0 creates (completing via the queue), 1
    // searches and enters, 2+ queue; the leader starts at 75 s.
    // wipe: idle and not reviving until the whole group falls and the floor
    // restarts; after that it fights and revives as in the grotto.
    let parado_fixo = cenario == "parado" || cenario == "colisao";
    let mut entrou_em: Option<Instant> = None;
    let mut sala_pediu_inicio = false;

    while Instant::now() < fim {
        tokio::select! {
            _ = entrada.tick(), if no_mundo_em.is_some() => {
                seq += 1;
                ws.send(envia(ClientMessage::Input { input: shared::protocol::InputFrame {
                    seq, tick: tick_srv, move_dir: glam::Vec2::ZERO, aim: glam::Vec2::X, buttons: 0,
                }})?).await?;
            }
            _ = tique.tick() => {
                if let Some(t) = no_mundo_em {
                    if !pediu && t.elapsed() > Duration::from_secs(2) {
                        pediu = true;
                        let solo = dg::conteudo(conteudo).is_some_and(|c| c.tipo == dg::Tipo::Porao);
                        let p = match (cenario, indice) {
                            _ if solo => Pedido::EntrarSolo { conteudo },
                            ("sala", 0) => Pedido::SalaCriar { conteudo, estagio: 1, completar_pela_fila: true },
                            ("sala", 1) => Pedido::SalasBuscar { conteudo, estagio: 1 },
                            _ => Pedido::FilaEntrar { conteudo, estagio: 1 },
                        };
                        ws.send(envia(ClientMessage::Dungeon { pedido: p })?).await?;
                    }
                    if cenario == "sala" && indice == 0 && !sala_pediu_inicio && t.elapsed() > Duration::from_secs(75) {
                        sala_pediu_inicio = true;
                        r.textos.push("lider pediu SalaIniciar".into());
                        ws.send(envia(ClientMessage::Dungeon { pedido: Pedido::SalaIniciar })?).await?;
                    }
                }
                if r.saiu {
                    break;
                }
                if !na_instancia || caido {
                    continue;
                }
                let Some(minha) = eu.and_then(|id| ents.get(&id)).map(|e| e.pos) else { continue };
                if r.pos_dentro.is_none() && entrou_em.is_some_and(|t| t.elapsed() > Duration::from_secs(4)) {
                    r.pos_dentro = Some((minha.x, minha.y));
                }
                if let Some(v) = venceu_em {
                    // Won: walks to the chest and touches it; opened, leaves.
                    if r.bau.is_none() {
                        if let Some((id, p)) = ents.iter().find(|(_, e)| e.tag == EntityTag::Npc && shared::npc_papel_de_kind(e.kind) == dg::PAPEL_BAU).map(|(id, e)| (*id, e.pos)) {
                            if p.distance(minha) > 3.0 {
                                ws.send(envia(ClientMessage::MoverPara { x: p.x, z: p.y })?).await?;
                            }
                            ws.send(envia(ClientMessage::Interact { target_eid: Some(id.0 as u64) })?).await?;
                        }
                    } else if acabou_em.is_none() {
                        acabou_em = Some(Instant::now());
                    }
                    if acabou_em.is_some_and(|t| t.elapsed() > Duration::from_secs(2)) || v.elapsed() > Duration::from_secs(70) {
                        ws.send(envia(ClientMessage::Dungeon { pedido: Pedido::Sair })?).await?;
                        acabou_em = Some(Instant::now() + Duration::from_secs(3600));
                    }
                    continue;
                }
                let parado = parado_fixo || (cenario == "wipe" && r.wipes == 0);
                if parado {
                    continue;
                }
                if std::env::var("DG_DEBUG").is_ok() {
                    let inimigos: Vec<_> = ents.iter().filter(|(_, e)| e.tag == EntityTag::Enemy).map(|(id, e)| (id.0, e.pos, e.hp)).collect();
                    eprintln!("[{user}] eu={minha:?} alvo={alvo:?} inimigos={}", inimigos.iter().take(4).map(|(i, p, h)| format!("{i}@({:.0},{:.0})hp{h}", p.x, p.y)).collect::<Vec<_>>().join(" "));
                }
                let perto = ents
                    .iter()
                    .filter(|(_, e)| e.tag == EntityTag::Enemy && e.hp > 0)
                    .min_by(|a, b| a.1.pos.distance(minha).total_cmp(&b.1.pos.distance(minha)))
                    .map(|(id, e)| (*id, e.pos));
                if let Some((id, p)) = perto {
                    if alvo != Some(id) {
                        alvo = Some(id);
                        ws.send(envia(ClientMessage::SetTarget { target: Some(id) })?).await?;
                    }
                    if p.distance(minha) > 1.4 {
                        let d = (minha - p).normalize_or_zero() * 0.8;
                        ws.send(envia(ClientMessage::MoverPara { x: p.x + d.x, z: p.y + d.y })?).await?;
                    }
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
                        ws.send(envia(ClientMessage::Login { username: user.into(), password: senha.into(), lembrar: false })?).await?;
                    }
                    ServerMessage::CharacterList { chars, available_weapons, .. } => {
                        let m = match chars.first() {
                            Some(c) => ClientMessage::SelectCharacter { name: c.name.clone() },
                            None => ClientMessage::CreateCharacter {
                                name: format!("{user}_dg"),
                                aparencia: Default::default(),
                                starting_weapon: available_weapons.first().copied().unwrap_or(0),
                                faction: Default::default(),
                            },
                        };
                        ws.send(envia(m)?).await?;
                    }
                    ServerMessage::LoginOk { .. } => {
                        if no_mundo_em.is_none() {
                            no_mundo_em = Some(Instant::now());
                            ws.send(envia(ClientMessage::AdminCommand { secret: segredo.into(), target_char: None, action: AdminAction::SetLevel { level: nivel } })?).await?;
                            // The level's points, the way a player would spread them: half VIT, half STR.
                            let pontos = 3 * nivel.saturating_sub(1);
                            for i in 0..pontos {
                                let stat = if i % 2 == 0 { shared::stat_idx::VIT } else { 0 };
                                ws.send(envia(ClientMessage::AllocStatPoint { stat: stat as u8 })?).await?;
                            }
                            ws.send(envia(ClientMessage::AdminCommand { secret: segredo.into(), target_char: None, action: AdminAction::HealFull })?).await?;
                        }
                    }
                    ServerMessage::Snapshot { snapshot } => {
                        tick_srv = snapshot.tick;
                        for m in snapshot.entered {
                            let e = ents.entry(m.id).or_insert(Ent { tag: m.tag, kind: m.kind, pos: glam::Vec2::ZERO, hp: 1 });
                            e.tag = m.tag;
                            e.kind = m.kind;
                        }
                        for st in snapshot.states {
                            if st.flags & shared::ent_flags::SELF != 0 {
                                eu = Some(st.id);
                            }
                            if let Some(e) = ents.get_mut(&st.id) {
                                e.pos = st.pos_f32();
                                e.hp = st.hp;
                            }
                        }
                        for id in snapshot.removed {
                            ents.remove(&id);
                        }
                    }
                    ServerMessage::Dungeon { aviso } => match aviso {
                        Aviso::Pronto { partida, .. } => {
                            ws.send(envia(ClientMessage::Dungeon { pedido: Pedido::Pronto { partida, aceito: true } })?).await?;
                        }
                        Aviso::Instancia { andar, inimigos, reviver_em_s, concluida, .. } => {
                            na_instancia = true;
                            r.entrou = true;
                            if entrou_em.is_none() {
                                entrou_em = Some(Instant::now());
                            }
                            if andar_visto != Some(andar) {
                                andar_visto = Some(andar);
                                r.andares.push(andar);
                                r.inimigos_no_inicio_do_andar.push((andar, inimigos));
                                alvo = None;
                            }
                            match reviver_em_s {
                                Some(s) => {
                                    if !caido {
                                        caido = true;
                                        r.mortes += 1;
                                        r.esperas_de_reviver.push(s);
                                    }
                                    if s == 0 && !parado_fixo && !(cenario == "wipe" && r.wipes == 0) {
                                        ws.send(envia(ClientMessage::Dungeon { pedido: Pedido::Reviver })?).await?;
                                    }
                                }
                                None => {
                                    if caido {
                                        caido = false;
                                        r.revividas += 1;
                                    }
                                }
                            }
                            let _ = concluida;
                        }
                        Aviso::Resultado { vitoria, tempo_s, .. } => {
                            r.resultado = Some((vitoria, tempo_s));
                            if vitoria {
                                venceu_em = Some(Instant::now());
                            }
                        }
                        Aviso::Bau { itens, marcas, .. } => r.bau = Some((itens, marcas)),
                        Aviso::Salas { lista } => {
                            r.textos.push(format!("salas achadas: {}", lista.len()));
                            if let Some(s) = lista.first() {
                                ws.send(envia(ClientMessage::Dungeon { pedido: Pedido::SalaEntrar { sala: s.id } })?).await?;
                            }
                        }
                        Aviso::Estado { sala: Some(s), .. } => {
                            r.textos.push(format!("na sala {} com {} membro(s)", s.id, s.membros.len()));
                        }
                        Aviso::Saiu => {
                            r.saiu = true;
                        }
                        Aviso::Texto { texto, .. } | Aviso::ProntoFechou { texto, .. } => {
                            if texto.contains("O grupo caiu") {
                                r.wipes += 1;
                            }
                            r.textos.push(texto);
                        }
                        _ => {}
                    },
                    ServerMessage::Kick { reason } => anyhow::bail!("kick: {reason}"),
                    ServerMessage::LoginDenied { reason } => anyhow::bail!("login negado: {reason}"),
                    _ => {}
                }
            }
        }
    }
    Ok(())
}
