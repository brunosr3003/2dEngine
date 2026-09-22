//! Espia uma zona pelo caminho de um cliente de verdade — e vê se o corpo ANDA.
//!
//! Nasceu de "a minha ilha tá completamente bugada, nada carregou e tá tudo
//! azul vazio", e depois de "meu personagem não anda, nada acontece". Os dois
//! sintomas são de TELA, e a tela é o último lugar onde dá pra diagnosticar:
//! o relevo do cliente confere com o do servidor, a prévia desenha a ilha
//! inteira, e mesmo assim não aparecia nada.
//!
//! Aqui se vê o que sai do servidor, na ordem, sem GPU e sem janela:
//!
//! 1. as mensagens do login (é `AvisoColonia::Terreno` que diz QUAL ilha
//!    desenhar — sem ela `terreno` e `map` ficam `None` e a tela é azul);
//! 2. se o CORPO do jogador entra na AOI dele mesmo (instância da sessão sem
//!    o componente `Instancia` na entidade = snapshot com zero entidades);
//! 3. se ele SAI DO LUGAR com input — que é outra coisa: na colônia
//!    `self.ilha` é `None`, e quem manda na física deixa de ser o relevo.
//!
//!   cargo run --bin colonia_espia -- --host 127.0.0.1:9200 --user X --pass Y

use futures_util::{SinkExt, StreamExt};
use shared::protocol::{ClientMessage, InputFrame, ServerMessage};
use std::time::{Duration, Instant};
use tokio_tungstenite::tungstenite::Message;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let arg = |nome: &str, padrao: &str| -> String {
        args.iter()
            .position(|a| a == nome)
            .and_then(|i| args.get(i + 1))
            .cloned()
            .unwrap_or_else(|| padrao.to_string())
    };
    let host = arg("--host", "127.0.0.1:9200");
    let user = arg("--user", "diagcolonia");
    let pass = arg("--pass", "Diag!2026colonia");
    let segs: u64 = arg("--secs", "18").parse().unwrap_or(18);
    // `--rota N`: em vez de andar em círculo, pede ao A* uma rota de N
    // unidades e mede se o corpo CHEGA. É outro caminho no servidor — a
    // física anda sem A*, e o A* achava a ilha por outro lugar.
    let rota_un: f32 = arg("--rota", "0").parse().unwrap_or(0.0);
    let rota = rota_un > 0.0;
    // `--engasgo`: anda 2 s, depois PARA DE MANDAR INPUT por 1,5 s, como um
    // celular que perdeu pacote. Mede quanto o corpo andou no SILENCIO — o
    // servidor não deveria mover ninguém sem input, e cada unidade andada aí
    // é uma que o jogador não pediu e que vai ser corrigida na volta (o
    // "voltando ao ponto anterior").
    let engasgo = args.iter().any(|a| a == "--engasgo");
    // `--quests`: percorre a LINHA DE TUTORIAL DA ILHA inteira — mural,
    // assentamento, contratar, colher, retirar — e diz qual passo fechou e
    // qual não. É o caminho que o jogador faz, pelas mesmas mensagens.
    let quests = args.iter().any(|a| a == "--quests");

    let (mut ws, _) = tokio_tungstenite::connect_async(format!("ws://{host}")).await?;
    let envia = |m: &ClientMessage| -> anyhow::Result<Message> {
        Ok(Message::Binary(shared::protocol::encode(m)?))
    };
    ws.send(envia(&ClientMessage::Handshake {
        protocol_version: shared::PROTOCOL_VERSION,
        client_version: "colonia_espia".into(),
    })?)
    .await?;

    let fim = Instant::now() + Duration::from_secs(segs);
    let mut viu_terreno = false;
    let mut n = 0usize;
    let mut meu: Option<shared::EntityId> = None;
    let mut no_mundo = false;
    let mut partida: Option<glam::Vec2> = None;
    let mut agora_em: Option<glam::Vec2> = None;
    let mut andou = 0.0f32;
    let mut tick = 0u32;
    let mut seq = 0u32;
    let mut alvo: Option<glam::Vec2> = None;
    let mut ilha_da_colonia: Option<shared::terreno::Ilha> = None;
    let mut pediu = false;
    let mut passo_aberto: Option<u16> = None;
    let mut fechados: Vec<u16> = Vec::new();
    let mut proximo_passo = 0.0f32;
    let mut avisou_silencio = false;
    let mut antes_do_silencio: Option<glam::Vec2> = None;
    let mut depois_do_silencio: Option<glam::Vec2> = None;
    let mut relogio = tokio::time::interval(Duration::from_millis(33));

    while Instant::now() < fim {
        tokio::select! {
            _ = relogio.tick(), if no_mundo => {
                seq += 1;
                if quests {
                    // Um pedido por segundo, na ordem que a quest ensina.
                    // Cada um responde com `Estado`, e o QuestUpdate diz se o
                    // passo fechou — é isso que o teste está lendo.
                    let t_s = seq as f32 / 30.0;
                    if t_s >= proximo_passo {
                        proximo_passo = t_s + 1.2;
                        use shared::colonia::PedidoColonia as P;
                        let pedido = match passo_aberto {
                            Some(798) => Some(P::Painel),
                            Some(799) => Some(P::Melhorar {
                                eixo: shared::colonia::eixo::ASSENTAMENTO as u8,
                            }),
                            Some(800) => Some(P::Contratar { vaga: 0, oficio: 0 }),
                            Some(801) => Some(P::Colher),
                            Some(802) => Some(P::Retirar),
                            // Fora da linha da ilha: só abre o painel, pra ver
                            // o estado e não ficar mudo.
                            _ => Some(P::Painel),
                        };
                        if let Some(p) = pedido {
                            println!("    -> pedindo {p:?} (passo aberto: {passo_aberto:?})");
                            ws.send(envia(&ClientMessage::Colonia { pedido: p })?).await?;
                        }
                    }
                } else if rota {
                    // A rota pede DUAS coisas, e faltar uma não mede nada:
                    //
                    // 1. `MoverPara` uma vez — input com direção MATA a rota
                    //    (`handle_mover_para`), então não dá pra andar junto;
                    // 2. `Input` com direção ZERO a cada tick — é ele que o
                    //    servidor preenche com o passo da rota. O cliente de
                    //    verdade manda input parado o tempo todo; sem isso a
                    //    rota existe e ninguém a consome, e o teste acusa
                    //    "não sai do lugar" num A* que funciona.
                    if !pediu {
                        if let Some(d) = alvo {
                            pediu = true;
                            println!("    pedindo rota para ({:.2}, {:.2})", d.x, d.y);
                            ws.send(envia(&ClientMessage::MoverPara { x: d.x, z: d.y })?).await?;
                        }
                    }
                    ws.send(envia(&ClientMessage::Input { input: InputFrame {
                        seq, tick,
                        move_dir: glam::Vec2::ZERO,
                        aim: glam::Vec2::X,
                        buttons: 0,
                    }})?).await?;
                } else {
                    // A JANELA DE SILÊNCIO do `--engasgo`: de 2,0 s a 3,5 s
                    // depois de entrar no mundo, nenhum input sai.
                    let t_s = seq as f32 / 30.0;
                    let calado = engasgo && (2.0..3.5).contains(&t_s);
                    if calado {
                        if !avisou_silencio {
                            avisou_silencio = true;
                            antes_do_silencio = agora_em;
                            println!("    [engasgo] parei de mandar input em {:?}", agora_em);
                        }
                    } else {
                        if avisou_silencio && depois_do_silencio.is_none() {
                            depois_do_silencio = agora_em;
                            let d = match (antes_do_silencio, agora_em) {
                                (Some(a), Some(b)) => a.distance(b),
                                _ => 0.0,
                            };
                            println!(
                                "    [engasgo] voltei a mandar. O corpo andou {d:.2} u                                  SEM input, ate {:?}",
                                agora_em
                            );
                        }
                        // EM CÍRCULO, e não sempre pro mesmo lado: andando reto
                        // o corpo encosta na primeira árvore e o teste diz "não
                        // anda" numa ilha onde anda. Medido: no Bosque, 0,75 u.
                        let t = seq as f32 * 0.05;
                        ws.send(envia(&ClientMessage::Input { input: InputFrame {
                            seq, tick,
                            move_dir: glam::Vec2::new(t.cos(), t.sin()),
                            aim: glam::Vec2::X,
                            buttons: 0,
                        }})?).await?;
                    }
                }
            }
            frame = ws.next() => {
                let Some(frame) = frame else { break };
                let Message::Binary(bytes) = frame? else { continue };
                let Ok(msg) = shared::protocol::decode::<ServerMessage>(&bytes) else {
                    println!("  (nao decodificou, {} bytes)", bytes.len());
                    continue;
                };
                n += 1;
                match msg {
                    ServerMessage::HandshakeAck { .. } => {
                        println!("{n:3} HandshakeAck");
                        ws.send(envia(&ClientMessage::Login {
                            username: user.clone(), password: pass.clone(),
                        })?).await?;
                    }
                    ServerMessage::CharacterList { chars, available_weapons } => {
                        println!("{n:3} CharacterList: {} personagem(ns)", chars.len());
                        let m = match chars.first() {
                            Some(c) => ClientMessage::SelectCharacter { name: c.name.clone() },
                            None => ClientMessage::CreateCharacter {
                                name: format!("{user}c"),
                                aparencia: Default::default(),
                                starting_weapon: available_weapons.first().copied().unwrap_or(0),
                                faction: Default::default(),
                            },
                        };
                        ws.send(envia(&m)?).await?;
                    }
                    ServerMessage::MapChange { map_name, .. } => {
                        println!("{n:3} MapChange '{map_name}'   <-- o cliente ZERA o terreno aqui");
                    }
                    ServerMessage::Colonia { ref aviso } => match *aviso {
                        shared::colonia::AvisoColonia::Terreno {
                            plato,
                            assentamento,
                            ref trabalhadores,
                        } => {
                            viu_terreno = true;
                            println!(
                                "{n:3} Colonia::TERRENO plato={plato:.0} assentamento={} ({} morador(es))",
                                shared::colonia::Assentamento::do_nivel(assentamento).nome(),
                                trabalhadores.len()
                            );
                            if rota {
                                ilha_da_colonia =
                                    Some(shared::terreno::Ilha::da_colonia(plato));
                            }
                        }
                        shared::colonia::AvisoColonia::Estado { niveis, .. } =>
                            println!("{n:3} Colonia::Estado niveis={niveis:?}"),
                        shared::colonia::AvisoColonia::Recusa(ref t) =>
                            println!("{n:3} Colonia::Recusa {t}"),
                    },
                    ServerMessage::LoginOk { spawn, entity_id, .. } => {
                        meu = Some(entity_id);
                        no_mundo = true;
                        println!("{n:3} LoginOk spawn=({:.2}, {:.2})", spawn[0], spawn[1]);
                    }
                    ServerMessage::LoginDenied { reason } => {
                        println!("{n:3} LoginDenied: {reason}");
                        break;
                    }
                    ServerMessage::Kick { reason } => {
                        println!("{n:3} Kick: {reason}");
                        break;
                    }
                    ServerMessage::Chat { ref from, ref text } => {
                println!("{n:3} Chat [{from}] {text}")
            }
            ServerMessage::QuestUpdate {
                quest_id,
                status,
                ..
            } if quests => {
                let nome = shared::quests::quest_by_id(quest_id)
                    .map(|d| d.title)
                    .unwrap_or("?");
                let st = match status {
                    shared::quests::quest_status::ACTIVE => "ABERTO",
                    shared::quests::quest_status::READY => "pronto",
                    shared::quests::quest_status::TURNED_IN => "FECHADO",
                    _ => "?",
                };
                println!("    [quest] {quest_id} '{nome}' -> {st}");
                if status == shared::quests::quest_status::ACTIVE {
                    passo_aberto = Some(quest_id);
                }
                if status == shared::quests::quest_status::TURNED_IN {
                    fechados.push(quest_id);
                }
            }
            ServerMessage::QuestLog { quests: ref log } if quests => {
                for q in log.iter().filter(|q| {
                    q.status == shared::quests::quest_status::ACTIVE
                        && shared::historia::e_da_historia(q.id)
                }) {
                    println!("    [quest] no log, ABERTO: {} '{}'", q.id, q.title);
                    passo_aberto = Some(q.id);
                }
            }
                    ServerMessage::Snapshot { snapshot } => {
                        tick = snapshot.tick;
                        if let Some(id) = meu {
                            let entrou = partida.is_none()
                                && snapshot.entered.iter().any(|m| m.id == id);
                            for e in snapshot.states.iter().filter(|e| e.id == id) {
                                if partida.is_none() {
                                    let p = e.pos_f32();
                                    partida = Some(p);
                                    agora_em = Some(p);
                                    if rota {
                                        // O destino tem que ser CHÃO. Mandar
                                        // um ponto qualquer a N unidades cai
                                        // na água numa ilha que é 38% terra, e
                                        // o A* recusa — com razão. O teste
                                        // diria "não acha rota" onde acha.
                                        //
                                        // O espião gera a mesma ilha (a
                                        // semente é pública e única) e escolhe
                                        // um ponto de terra de verdade.
                                        alvo = ilha_da_colonia.as_ref().map(|i| {
                                            let mut melhor = p;
                                            for k in 0..64 {
                                                let a = k as f32 * std::f32::consts::TAU / 64.0;
                                                let c = p + glam::Vec2::new(
                                                    a.cos() * rota_un,
                                                    a.sin() * rota_un,
                                                );
                                                let d = i.terra_mais_proxima(c.x, c.y, 12.0);
                                                if !i.agua(d.x, d.y) && d.distance(p) > rota_un * 0.6
                                                {
                                                    melhor = d;
                                                    break;
                                                }
                                            }
                                            melhor
                                        });
                                    }
                                    println!(
                                        "    >>> MEU CORPO na AOI em ({:.2}, {:.2}){}",
                                        p.x,
                                        p.y,
                                        if entrou { " (entrou agora)" } else { "" }
                                    );
                                    continue;
                                }
                                let p = e.pos_f32();
                                if let Some(antes) = agora_em {
                                    andou += antes.distance(p);
                                }
                                agora_em = Some(p);
                                if seq % 30 == 0 {
                                    println!(
                                        "    ... em ({:.2}, {:.2}) · andou {andou:.2} u                                          · hp {} · flags {:#04x}",
                                        p.x, p.y, e.hp, e.flags
                                    );
                                }
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
    }

    if quests {
        println!("\n─────────────── a linha da ilha ───────────────");
        for (id, nome) in [
            (798u16, "O mural da praça"),
            (799, "Casa vira vila"),
            (800, "O primeiro morador"),
            (801, "O que a ilha rendeu"),
            (802, "Buscar no baú"),
        ] {
            let ok = fechados.contains(&id);
            println!("  {} {id} {nome}", if ok { "FECHOU " } else { "TRAVOU " });
        }
        println!("  passo ainda aberto: {passo_aberto:?}");
    }
    println!("\n─────────────── o que deu ───────────────");
    println!(
        "relevo mandado .... {}",
        if viu_terreno { "sim" } else { "NAO — a tela fica azul sem isto" }
    );
    match (partida, agora_em) {
        (Some(a), Some(b)) => {
            println!("corpo na AOI ...... sim, em ({:.2}, {:.2})", a.x, a.y);
            if let Some(d) = alvo {
                println!(
                    "rota pedida ....... para ({:.2}, {:.2}) · falta {:.2} u",
                    d.x,
                    d.y,
                    b.distance(d)
                );
            }
            println!(
                "andou ............. {andou:.2} u percorridos, {:.2} u de deslocamento\n\
                 {}",
                a.distance(b),
                if andou > 1.0 {
                    "                    O CORPO ANDA."
                } else {
                    "                    O CORPO NAO SAI DO LUGAR."
                }
            );
        }
        _ => println!(
            "corpo na AOI ...... NAO — o jogador nao ve nem a si mesmo, \
             e sem corpo nao ha' o que andar"
        ),
    }
    Ok(())
}
