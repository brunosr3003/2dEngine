//! **jogadorbot** — bots que jogam o caminho inteiro de um jogador.
//!
//! O pedido: "bots que vão seguir todo o caminho exato de um player, como se
//! fosse um player mesmo: criar conta, fazer quests, upar quando necessário,
//! fazer dungeons, vender e comprar no mercado — e tudo trackável".
//!
//! ── Por que isso vale ────────────────────────────────────────────────────
//!
//! Teste de unidade prova que uma peca funciona; bot que joga prova que o
//! JOGO funciona. Sao coisas diferentes: 919 testes verdes convivem bem com
//! uma missao que pede 6 de um item que cai a 0,3 por abate, e nenhum deles
//! reclama. O bot reclama, porque ele leva 20 minutos e o numero aparece na
//! trilha.
//!
//! ── Ele e' um CLIENTE, nao um atalho ─────────────────────────────────────
//!
//! Tudo passa pelo mesmo caminho do jogador de verdade: HTTP no `/api/register`,
//! WebSocket com `Handshake`, `Login`, `CreateCharacter`, e dai' em diante so'
//! `ClientMessage`. Nada de mexer no banco por baixo. Se der pra fazer por
//! SQL, nao prova nada — o que se quer medir e' justamente o caminho.
//!
//! ── Rastreio ─────────────────────────────────────────────────────────────
//!
//! Duas camadas, e as duas ja' existem:
//!
//! * a **trilha** deste processo (JSONL, uma linha por evento) responde "o que
//!   o bot 3 fez as 14h12";
//! * o **panoptico** ja' enxerga tudo, porque os bots sao sessoes de verdade:
//!   aparecem no mapa, na economia, no mercado e na telemetria por minuto sem
//!   uma linha de codigo nova.
//!
//! ```sh
//! cargo run --bin jogadorbot -- --host 127.0.0.1:9000 --api http://127.0.0.1:18090 \
//!     --n 4 --minutos 30 --trilha /tmp/bots.jsonl
//! ```

mod trilha;

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{anyhow, Result};
use futures_util::{SinkExt, StreamExt};
use shared::protocol::{ClientMessage, InputFrame, ServerMessage};
use tokio_tungstenite::tungstenite::Message;
use trilha::{Evento, Fase, Trilha};

/// Quanto o bot espera entre uma decisão e a próxima.
///
/// 700 ms e não 33: ele não está jogando reflexo, está percorrendo o jogo. Um
/// laço de tick cheio gastaria CPU do servidor que a gente quer medir — o
/// observador não pode ser a maior carga do que observa.
const PENSA_A_CADA: Duration = Duration::from_millis(700);

struct Cfg {
    host: String,
    api: String,
    n: usize,
    minutos: u64,
    trilha: String,
    prefixo: String,
}

fn cfg() -> Cfg {
    let a: Vec<String> = std::env::args().collect();
    let pega = |nome: &str, padrao: &str| -> String {
        a.iter()
            .position(|x| x == nome)
            .and_then(|i| a.get(i + 1))
            .cloned()
            .unwrap_or_else(|| padrao.to_string())
    };
    Cfg {
        host: pega("--host", "127.0.0.1:9000"),
        api: pega("--api", "http://127.0.0.1:18090"),
        n: pega("--n", "3").parse().unwrap_or(3),
        minutos: pega("--minutos", "20").parse().unwrap_or(20),
        trilha: pega("--trilha", "/tmp/jogadorbot.jsonl"),
        // O prefixo separa uma corrida da outra no banco. Sem ele, rodar duas
        // vezes bate em "username já existe" e o bot morre no cadastro.
        prefixo: pega(
            "--prefixo",
            &format!("bot{}", std::process::id() % 10_000),
        ),
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let c = cfg();
    let t = Arc::new(Trilha::nova(&c.trilha)?);
    println!(
        "jogadorbot: {} bot(s) em {} por {} min\n  api:    {}\n  trilha: {}\n  prefixo:{}\n",
        c.n, c.host, c.minutos, c.api, c.trilha, c.prefixo
    );

    let ate = Instant::now() + Duration::from_secs(c.minutos * 60);
    let mut tarefas = Vec::new();
    for i in 0..c.n {
        let (host, api, t) = (c.host.clone(), c.api.clone(), t.clone());
        let nome = format!("{}_{i}", c.prefixo);
        tarefas.push(tokio::spawn(async move {
            // Entrada escalonada: `n` bots cadastrando no mesmo instante é uma
            // onda de argon2, e a fila de login existe justamente pra isso não
            // roubar o tick de quem já está dentro.
            tokio::time::sleep(Duration::from_millis(400 * i as u64)).await;
            if let Err(e) = vive(&host, &api, &nome, ate, &t).await {
                t.registra(Evento {
                    bot: &nome,
                    fase: Fase::Desconectado,
                    acao: "bot_morreu",
                    ok: false,
                    detalhe: format!("{e:#}"),
                    nivel: 0,
                    xp: 0,
                    ouro: 0,
                });
                eprintln!("[{nome}] parou: {e:#}");
            }
        }));
    }
    for x in tarefas {
        let _ = x.await;
    }
    println!("{}", t.resumo());
    println!("trilha completa em {}", c.trilha);
    Ok(())
}

/// O estado que o bot conhece do próprio personagem.
#[derive(Default)]
struct Eu {
    nivel: u32,
    xp: i64,
    ouro: i64,
    pontos_livres: u32,
    pos: glam::Vec2,
    entidade: Option<shared::EntityId>,
    /// Missões ativas: id -> (progresso, status).
    quests: HashMap<u16, (u32, u8)>,
    /// Quem tem missão pra oferecer agora.
    ofertas: Vec<u16>,
    /// Inventário: item -> quantidade.
    bolsa: HashMap<u16, u32>,
    /// Destino do passo atual: (missão, tipo, ponto, raio, npc).
    destino: Option<Destino>,
    /// Há quantas decisões o bot está pedindo destino sem receber. Sem este
    /// contador ele reenvia `QuestDestino` pra sempre e nunca faz mais nada —
    /// foi o que aconteceu na primeira corrida: 3 bots parados no nascedouro,
    /// com missão ativa e zero de XP.
    esperando_destino: u32,
    /// Alvo de combate mais próximo visto no último snapshot.
    alvo: Option<(shared::EntityId, glam::Vec2)>,
    morto: bool,
    /// Já tentou gastar o saldo atual de pontos.
    tentou_gastar: bool,
}

#[derive(Clone, Copy)]
struct Destino {
    quest: u16,
    tipo: u8,
    pos: glam::Vec2,
    raio: f32,
    npc: Option<u64>,
}

fn envia(m: &ClientMessage) -> Result<Message> {
    Ok(Message::Binary(shared::protocol::encode(m)?.into()))
}

/// Cria a conta pelo `/api/register`, igual ao jogo faz.
async fn cadastra(api: &str, nome: &str) -> Result<()> {
    let corpo = format!(
        r#"{{"username":"{nome}","email":"{nome}@bot.local","password":"senhadebot123"}}"#
    );
    let r = reqwest::Client::new()
        .post(format!("{api}/api/register"))
        .header("content-type", "application/json")
        .body(corpo)
        .timeout(Duration::from_secs(15))
        .send()
        .await?;
    // 409 = já existe, e isso é SUCESSO pra um bot: rodar de novo com o mesmo
    // prefixo tem que reusar a conta, não parar.
    if r.status().is_success() || r.status().as_u16() == 409 {
        return Ok(());
    }
    Err(anyhow!("register {}: {}", r.status(), r.text().await.unwrap_or_default()))
}

async fn vive(host: &str, api: &str, nome: &str, ate: Instant, t: &Trilha) -> Result<()> {
    let mut eu = Eu::default();
    let reg = cadastra(api, nome).await;
    t.registra(Evento {
        bot: nome,
        fase: Fase::Cadastrando,
        acao: "conta_criada",
        ok: reg.is_ok(),
        detalhe: reg.as_ref().err().map(|e| format!("{e:#}")).unwrap_or_default(),
        nivel: 0,
        xp: 0,
        ouro: 0,
    });
    reg?;

    let url = format!("ws://{host}/ws");
    let (mut ws, _) = tokio_tungstenite::connect_async(&url).await?;
    ws.send(envia(&ClientMessage::Handshake {
        protocol_version: shared::PROTOCOL_VERSION,
        client_version: format!("jogadorbot/{}", env!("CARGO_PKG_VERSION")),
    })?)
    .await?;

    let mut fase = Fase::Logando;
    let mut pensa = tokio::time::interval(PENSA_A_CADA);
    let mut seq: u32 = 0;
    let mut tick: u32 = 0;

    loop {
        if Instant::now() >= ate {
            let _ = ws.send(envia(&ClientMessage::RequestDisconnect)?).await;
            return Ok(());
        }
        tokio::select! {
            _ = pensa.tick() => {
                if fase == Fase::NoMundo {
                    decide(&mut ws, &mut eu, nome, t, &mut seq, tick).await?;
                }
            }
            frame = ws.next() => {
                let Some(frame) = frame else { return Ok(()) };
                let bytes = match frame? {
                    Message::Binary(b) => b.to_vec(),
                    Message::Text(s) => s.as_bytes().to_vec(),
                    Message::Close(_) => return Ok(()),
                    _ => continue,
                };
                let Ok(msg) = shared::protocol::decode::<ServerMessage>(&bytes) else { continue };
                if let Some(nova) = recebe(msg, &mut ws, &mut eu, nome, t, &mut tick).await? {
                    fase = nova;
                }
            }
        }
    }
}

/// Um evento com o estado atual preenchido — é o que torna a trilha uma curva
/// e não uma lista de verbos soltos.
fn ev<'a>(bot: &'a str, eu: &Eu, acao: &'a str, ok: bool, detalhe: String) -> Evento<'a> {
    Evento {
        bot,
        fase: if eu.morto { Fase::Morto } else { Fase::NoMundo },
        acao,
        ok,
        detalhe,
        nivel: eu.nivel,
        xp: eu.xp,
        ouro: eu.ouro,
    }
}

type Ws = tokio_tungstenite::WebSocketStream<
    tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
>;

async fn recebe(
    msg: ServerMessage,
    ws: &mut Ws,
    eu: &mut Eu,
    nome: &str,
    t: &Trilha,
    tick: &mut u32,
) -> Result<Option<Fase>> {
    match msg {
        ServerMessage::HandshakeAck { .. } => {
            ws.send(envia(&ClientMessage::Login {
                username: nome.into(),
                password: "senhadebot123".into(),
                lembrar: false,
            })?)
            .await?;
        }
        ServerMessage::LoginDenied { reason } => {
            return Err(anyhow!("login negado: {reason}"));
        }
        ServerMessage::CharacterList {
            chars,
            available_weapons,
            ..
        } => {
            let m = match chars.first() {
                Some(c) => ClientMessage::SelectCharacter { name: c.name.clone() },
                None => ClientMessage::CreateCharacter {
                    name: format!("{nome}c"),
                    aparencia: Default::default(),
                    starting_weapon: available_weapons.first().copied().unwrap_or(0),
                    faction: Default::default(),
                },
            };
            let criando = matches!(m, ClientMessage::CreateCharacter { .. });
            ws.send(envia(&m)?).await?;
            t.registra(ev(
                nome,
                eu,
                if criando { "personagem_criado" } else { "personagem_escolhido" },
                true,
                String::new(),
            ));
            return Ok(Some(if criando { Fase::CriandoPersonagem } else { Fase::Logando }));
        }
        ServerMessage::CharacterCreationFailed { reason } => {
            return Err(anyhow!("criação do personagem falhou: {reason}"));
        }
        ServerMessage::LoginOk { entity_id, spawn, .. } => {
            eu.entidade = Some(entity_id);
            eu.pos = glam::Vec2::new(spawn[0], spawn[1]);
            eu.morto = false;
            t.registra(ev(nome, eu, "entrou_no_mundo", true, String::new()));
            return Ok(Some(Fase::NoMundo));
        }
        ServerMessage::Snapshot { snapshot } => {
            *tick = snapshot.tick;
            if let Some(meu) = eu.entidade {
                if let Some(s) = snapshot.states.iter().find(|s| s.id == meu) {
                    eu.pos = s.pos_f32();
                }
            }
            // O alvo é o inimigo vivo mais perto. Escolher aqui, e não na
            // decisão, porque é aqui que o dado chega fresco.
            eu.alvo = snapshot
                .states
                .iter()
                .filter(|s| Some(s.id) != eu.entidade)
                .filter(|s| snapshot.entered.iter().any(|m| m.id == s.id
                    && matches!(m.tag, shared::EntityTag::Enemy)))
                .map(|s| (s.id, s.pos_f32()))
                .min_by(|a, b| {
                    a.1.distance_squared(eu.pos)
                        .total_cmp(&b.1.distance_squared(eu.pos))
                });
        }
        ServerMessage::ProgressUpdate { level, xp, .. } => {
            let subiu = level > eu.nivel && eu.nivel > 0;
            eu.nivel = level;
            eu.xp = xp as i64;
            if subiu {
                t.registra(ev(nome, eu, "subiu_de_nivel", true, String::new()));
            }
        }
        ServerMessage::GoldUpdate { gold } => eu.ouro = gold as i64,
        ServerMessage::StatPointsUpdate { unspent, .. } => {
            eu.pontos_livres = unspent;
            // Saldo novo, tentativa nova. Sem isto o bot que teve UM pedido
            // recusado fica pedindo pra sempre e não faz mais nada: 139
            // tentativas em 2 minutos, medido na trilha.
            eu.tentou_gastar = false;
        }
        ServerMessage::InventoryUpdate { slots } => {
            eu.bolsa.clear();
            for s in slots {
                *eu.bolsa.entry(s.item_id).or_default() += s.qty;
            }
        }
        ServerMessage::QuestLog { quests } => {
            eu.quests = quests
                .iter()
                .map(|q| (q.id, (q.progress, q.status)))
                .collect();
        }
        ServerMessage::QuestUpdate { quest_id, progress, status } => {
            eu.quests.insert(quest_id, (progress, status));
        }
        ServerMessage::QuestGivers { available } => eu.ofertas = available,
        ServerMessage::QuestOffer { quests, .. } => {
            // Aceita a primeira que dê pra aceitar. Escolher a "melhor" seria
            // inventar uma estratégia que o jogador comum não tem.
            if let Some(q) = quests.first() {
                ws.send(envia(&ClientMessage::AcceptQuest { quest_id: q.id })?).await?;
                t.registra(ev(nome, eu, "quest_aceita", true, format!("#{}", q.id)));
            }
        }
        ServerMessage::QuestDestino { quest_id, tipo, pos, raio, npc_eid } => {
            eu.destino = Some(Destino {
                quest: quest_id,
                tipo,
                pos: glam::Vec2::new(pos[0], pos[1]),
                raio,
                npc: npc_eid,
            });
            eu.esperando_destino = 0;
        }
        ServerMessage::Morte { xp_perdido } => {
            eu.morto = true;
            t.registra(ev(nome, eu, "morreu", true, format!("-{xp_perdido} xp")));
            ws.send(envia(&ClientMessage::RespawnAtCity)?).await?;
        }
        ServerMessage::Kick { reason } => return Err(anyhow!("kick: {reason}")),
        ServerMessage::TrocarZona { .. } => {
            t.registra(ev(nome, eu, "trocou_de_zona", true, String::new()));
        }
        _ => {}
    }
    Ok(None)
}

/// A DECISÃO: uma lista de prioridades, não uma árvore.
///
/// Lista porque é o que um jogador faz — entrega o que está pronto, persegue o
/// que está ativo, pega mais quando acaba. Uma árvore de comportamento daria
/// mais nuance e esconderia a ordem, que aqui é justamente o que se quer poder
/// ler e mudar.
async fn decide(
    ws: &mut Ws,
    eu: &mut Eu,
    nome: &str,
    t: &Trilha,
    seq: &mut u32,
    tick: u32,
) -> Result<()> {
    if eu.morto {
        return Ok(());
    }
    // 1. Ponto de atributo parado é dano que não se causa.
    if eu.pontos_livres > 0 && !eu.tentou_gastar {
        eu.tentou_gastar = true;
        ws.send(envia(&ClientMessage::AllocStatPoint { stat: 0 })?).await?;
        t.registra(ev(nome, eu, "ponto_gasto", true, format!("{} livres", eu.pontos_livres)));
        return Ok(());
    }
    // 2. Missão pronta: entrega.
    let pronta = eu
        .quests
        .iter()
        .find(|(_, (_, st))| *st == shared::quests::quest_status::READY)
        .map(|(id, _)| *id);
    if let Some(id) = pronta {
        ws.send(envia(&ClientMessage::TurnInQuest { quest_id: id })?).await?;
        t.registra(ev(nome, eu, "quest_entregue", true, format!("#{id}")));
        return Ok(());
    }
    // 3. Sem missão nenhuma: pede oferta a quem tiver.
    let ativas = eu
        .quests
        .values()
        .filter(|(_, st)| *st != shared::quests::quest_status::TURNED_IN)
        .count();
    if ativas == 0 {
        let giver = eu.ofertas.first().copied().unwrap_or(0);
        ws.send(envia(&ClientMessage::RequestQuestOffer {
            source: shared::quests::quest_source::BOARD,
            giver,
        })?)
        .await?;
        t.registra(ev(nome, eu, "pediu_missao", true, format!("giver {giver}")));
        return Ok(());
    }
    // 4. Tem missão ativa: pergunta onde é o próximo passo.
    let ativa = eu
        .quests
        .iter()
        .find(|(_, (_, st))| *st == shared::quests::quest_status::ACTIVE)
        .map(|(id, _)| *id);
    if let Some(id) = ativa {
        if eu.destino.map(|d| d.quest) != Some(id) {
            // ANTI-TRAVAMENTO. O servidor pode simplesmente não ter destino
            // pra dar (`destino_tipo::NENHUM`, missão de craft, passo que
            // depende de item). Sem teto, o bot reenvia o pedido pra sempre e
            // fica parado no nascedouro — exatamente o que ele fez na
            // primeira corrida. Depois de três tentativas ele desiste e vai
            // caçar o que estiver por perto, que é o que um jogador faria.
            if eu.esperando_destino < 3 {
                eu.esperando_destino += 1;
                ws.send(envia(&ClientMessage::QuestDestino { quest_id: id })?).await?;
                return Ok(());
            }
        }
    }
    // 5. Com destino: anda até lá e faz o que o tipo pede.
    if let Some(d) = eu.destino {
        let perto = eu.pos.distance(d.pos) <= d.raio.max(3.0);
        if !perto {
            // ANDA PELO DIRECIONAL, e não pelo `MoverPara`.
            //
            // `MoverPara` é clique-para-andar: o servidor traça o caminho. Eu
            // o reenviava a cada 700 ms e cada envio REINICIAVA o trajeto —
            // 511 comandos de movimento em 3 minutos e o personagem sem sair
            // do nascedouro, medido no banco.
            //
            // O direcional é o que a mão do jogador faz, não depende do
            // servidor lembrar de um destino, e não tem nada pra reiniciar.
            *seq = seq.wrapping_add(1);
            let dir = (d.pos - eu.pos).normalize_or_zero();
            ws.send(envia(&ClientMessage::Input {
                input: InputFrame { seq: *seq, tick, move_dir: dir, aim: dir, buttons: 0 },
            })?)
            .await?;
            t.registra(ev(nome, eu, "andou", true, format!("#{} -> {:.0},{:.0} (faltam {:.0})", d.quest, d.pos.x, d.pos.y, eu.pos.distance(d.pos))));
            return Ok(());
        }
        use shared::quests::destino_tipo;
        match d.tipo {
            // FALAR: a primeira missão da história é esta, e era ela que
            // travava tudo — o bot chegava (ou nem isso) e não sabia
            // conversar.
            destino_tipo::NPC | destino_tipo::ENTREGA => {
                if let Some(npc) = d.npc {
                    ws.send(envia(&ClientMessage::Interact { target_eid: Some(npc) })?).await?;
                    ws.send(envia(&ClientMessage::ConcluirConversa { npc_eid: npc })?).await?;
                    t.registra(ev(nome, eu, "conversou", true, format!("#{} npc {npc}", d.quest)));
                    eu.destino = None;
                    eu.esperando_destino = 0;
                    return Ok(());
                }
            }
            destino_tipo::COLETA => {
                ws.send(envia(&ClientMessage::PedirNoDeColeta {
                    tipos: [true; 5],
                    energia: true,
                    raio: 40.0,
                    centro: [eu.pos.x, eu.pos.y],
                })?)
                .await?;
                t.registra(ev(nome, eu, "pediu_no_de_coleta", true, format!("#{}", d.quest)));
                return Ok(());
            }
            _ => {}
        }
    }
    // 6. Sem mais nada a fazer: bate no inimigo mais perto.
    if let Some((id, p)) = eu.alvo {
        ws.send(envia(&ClientMessage::SetTarget { target: Some(id) })?).await?;
        *seq = seq.wrapping_add(1);
        let dir = (p - eu.pos).normalize_or_zero();
        ws.send(envia(&ClientMessage::Input {
            input: InputFrame {
                seq: *seq,
                tick,
                move_dir: if eu.pos.distance(p) > 2.0 { dir } else { glam::Vec2::ZERO },
                aim: dir,
                buttons: 1,
            },
        })?)
        .await?;
        t.registra(ev(nome, eu, "atacou", true, String::new()));
        return Ok(());
    }
    t.registra(ev(nome, eu, "sem_o_que_fazer", false, format!(
        "{} quest(s), destino={:?}",
        eu.quests.len(),
        eu.destino.map(|d| (d.quest, d.tipo)),
    )));
    Ok(())
}
