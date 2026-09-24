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
/// O PULSO DE INPUT: 30 Hz, o mesmo ritmo do mundo.
///
/// O cliente de verdade manda um `InputFrame` TODO QUADRO, mesmo parado. Não
/// é desperdício: o servidor **para o corpo** no tick em que não chega input
/// ("sem ordem, o corpo fica onde está"), e é dentro desse mesmo laço que ele
/// conduz a rota do auto-path.
///
/// O bot só mandava input em duas situações raras. Então o `MoverPara`
/// traçava a rota, o servidor a guardava, e ela nunca andava um passo: 28
/// "travou_no_caminho" numa corrida, todos dizendo "parado a 100 do destino",
/// sem nenhuma recusa do servidor — porque recusa não houve. Faltava o pulso.
const PULSA_A_CADA: Duration = Duration::from_millis(33);

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
    /// Inventário: item -> quantidade. Pro que só precisa saber "tenho?".
    bolsa: HashMap<u16, u32>,
    /// A bolsa por SLOT, que é o que o mercado pede pra anunciar. Guardar os
    /// dois não é redundância: `MercadoAnunciar` fala em posição, e quase
    /// todo o resto fala em item.
    slots: Vec<(u16, shared::InventorySlot)>,
    /// O nó de coleta que o servidor apontou: coluna E ONDE ELE ESTÁ.
    ///
    /// A posição era descartada, e isso era o defeito: o bot pede nós num
    /// raio de 40 unidades e mandava `ColetarNo` de onde estava. Coletar
    /// exige estar ao alcance do nó (`COLETA_ALCANCE_UN`), então o ciclo
    /// nunca começava — `progresso 0` em 40 coletas seguidas, com três bots
    /// parados no nível 1 a corrida inteira.
    no_de_coleta: Option<(u32, glam::Vec2)>,
    /// Está colhendo agora? Enquanto estiver, não se decide outra coisa.
    colhendo: bool,
    /// Desde quando ele está "colhendo", e o progresso de quando começou.
    ///
    /// `colhendo` vem de `ColetaEstado` e fica preso em `true` quando o nó
    /// rende nada: o bot voltava cedo da decisão PARA SEMPRE, sem registrar
    /// um evento sequer. Sete dos doze bots ficaram assim, plantados em
    /// -273,-274, e só o batimento denunciou — mais uma vez, "o auto está
    /// ativo" não prova que algo está acontecendo.
    colhendo_desde: Option<Instant>,
    /// Pra onde o auto-path já foi mandado, e onde o bot estava quando
    /// mandou. As duas coisas juntas são o que detecta "pedi e não saí do
    /// lugar" sem reenviar o pedido a cada decisão.
    andando_para: Option<(glam::Vec2, glam::Vec2, u32)>,
    /// O MUNDO QUE O BOT CONHECE: quem é quem, e onde.
    ///
    /// Um snapshot traz `entered` (quem ACABOU de aparecer), `states` (só
    /// quem MUDOU) e `removed`. O bot escolhia o alvo olhando só o `entered`
    /// do tick atual — ou seja, um inimigo só era alvo no instante em que
    /// entrava no campo de visão e nunca mais. Numa corrida inteira ele
    /// atacou UMA vez. Como um cliente de verdade, agora ele lembra.
    conhecidos: HashMap<shared::EntityId, shared::EntityTag>,
    posicoes: HashMap<shared::EntityId, glam::Vec2>,
    /// A DIREÇÃO QUE O PULSO DEVE MANDAR nos últimos metros.
    ///
    /// O direcional só saía na decisão, uma vez a cada 700 ms — e cada frame
    /// vale UM tick de 33 ms. O bot andava 33 ms a cada 700, ou seja, parava
    /// a 5 unidades do NPC e ficava lá: o batimento mostrou ele imóvel em
    /// -49,-494 por cinco minutos, sempre "a 5u" do destino.
    ///
    /// Agora quem anda é o pulso, 30 vezes por segundo, como no cliente de
    /// verdade. Some sozinho (`empurrao_ate`) pra não virar um bot que anda
    /// pra sempre na última direção que alguém pediu.
    empurrao: Option<glam::Vec2>,
    empurrao_ate: Option<Instant>,
    /// Coletas seguidas sem o objetivo da missão andar, e o progresso visto.
    ///
    /// Colher é lento e o progresso demora, então o limite é alto — mas 375
    /// coletas em dez minutos sem sair do nível 1 não é lentidão, é laço.
    coleta_sem_avanco: u32,
    progresso_visto: (u16, u32),
    /// Crafts recusados por missão. Ver `CraftResultado`.
    craft_a_toa: HashMap<u16, u32>,
    /// Passos que não deram, por (missão, ação). Ver `passo_nao_deu`.
    ///
    /// MAPA PRÓPRIO, e isto é um conserto: eu tinha reusado o `craft_a_toa`
    /// aqui, e o tratador de craft bem-sucedido faz `clear()` nele. Como os
    /// bots craftam o tempo todo, o contador da forja era zerado antes de
    /// chegar a três e a desistência NUNCA disparava — 1.081
    /// `sem_peca_pra_refinar` em três minutos.
    passos_a_toa: HashMap<(u16, &'static str), u32>,
    /// Missões cujo destino o bot não consegue alcançar, e larga.
    ///
    /// O Treinador da ilha inicial é o caso: o A* ACHA caminho até ele (4
    /// pontos), mas entrega o corpo a 8,6 unidades — a grade do A* é de 4
    /// unidades e o último trecho não cabe nela —, e daí em linha reta tem
    /// alguma coisa no meio. Um bot gastou 61 decisões seguidas ali.
    ///
    /// Um jogador, nessa situação, desiste e vai fazer outra coisa. O bot faz
    /// o mesmo: a missão fica de lado, e a trilha guarda o achado — que é um
    /// defeito do JOGO, não do bot.
    desistiu: HashMap<u16, Instant>,
    /// A missão em que o bot está trabalhando agora. Ver a escolha do foco.
    foco: Option<u16>,
    /// Missões que ele já sabe que NÃO consegue fazer, pra sempre.
    ///
    /// Diferente do `desistiu`, que tem prazo: aquele é pra "não dá agora"
    /// (faltou material, o caminho não abriu). Este é pra "não dá nunca" —
    /// uma receita cujo material não vem de lugar nenhum que o bot alcance. O
    /// prazo fazia ele voltar a cada dez minutos pra redescobrir a mesma
    /// coisa, e um jogador não faz isso.
    impossiveis: std::collections::HashSet<u16>,
    /// Alvo a que ele já mandou ir pela rota. Ver a etapa 10.
    ///
    /// Sem isto, o `MoverPara` seria reenviado a cada decisão e reiniciaria o
    /// trajeto a cada 700 ms — o defeito que já custou 511 comandos de
    /// movimento com o personagem parado no mesmo pixel.
    indo_ao_alvo: Option<shared::EntityId>,
    /// O rumo da caminhada atual e quantos saltos ainda faltam nele.
    ///
    /// Sem rumo, o vagar virava caminhada aleatória e o bot não saía de um
    /// quadrado de 20 unidades em volta do porto — justamente onde o mundo NÃO
    /// põe bicho. Ver o ramo de vagar.
    rumo_do_vagar: Option<glam::Vec2>,
    vagares_no_rumo: u32,
    /// Contador do empurrão até o NÓ. Ver `empurra_no`.
    no_parado: u32,
    dist_do_no: f32,
    /// Pedidos de nó de coleta seguidos que o servidor não respondeu.
    ///
    /// Este ramo era MUDO e sem teto, e foi o que plantou quatro bots no mesmo
    /// ponto por meia hora: sem nó do tipo pedido num raio de 40 u, o servidor
    /// não responde nada, o bot pede de novo, e a trilha não tinha uma linha
    /// pra contar. É o irmão do ramo da busca, que eu já tinha consertado —
    /// e deixei este de fora, que é exatamente o erro que esta rede evita.
    pedidos_de_no: u32,
    /// Pra onde ele está vagando agora, e desde quando.
    ///
    /// Sem este compromisso o bot trocava de destino a cada 5,6 s e cada troca
    /// reiniciava o trajeto: 275 "vagou" em meia hora sem sair do lugar.
    vagando_para: Option<(glam::Vec2, Instant)>,
    /// Quantas vezes ele saiu pra vagar. Serve de semente e de ritmo.
    vagou: u32,
    /// Decisões seguidas empurrando sem encurtar a distância, e qual era ela.
    ///
    /// Andar reto não vence quina de casa, cerca nem carroça. Um bot ficou a
    /// CINCO unidades do primeiro NPC da história por sete minutos, empurrando
    /// a cada 700 ms contra alguma coisa — e como empurrar não registra nada,
    /// só o batimento denunciou. Agora, quando o empurrão não anda, ele faz o
    /// que o jogador faria: chama o A* (`MoverPara`) e deixa o servidor achar
    /// a volta.
    empurrao_parado: u32,
    dist_do_empurrao: f32,
    /// Chamadas ao A* no mesmo destino sem chegar. Ver `desistiu`.
    tentativas_no_destino: u32,
    /// Decisões desde o último batimento. Ver `BATIMENTO_A_CADA`.
    desde_o_batimento: u32,
    /// A última recusa que o servidor explicou pelo chat de sistema. Entra no
    /// detalhe de quem travou, pra a trilha dizer o PORQUÊ e não só o quê.
    ultima_recusa: Option<String>,
    /// As habilidades que o servidor mandou no login (`SkillsConfig`). O
    /// passo de tutorial "evolua uma habilidade" precisa de um id, e inventar
    /// um levaria a uma recusa silenciosa.
    skills: Vec<u32>,
    /// CONVERSAS QUE NÃO DERAM EM NADA, por NPC.
    ///
    /// Existe porque a trilha mentia: `conversou` saía com `ok: true` porque
    /// a MENSAGEM foi enviada, não porque o diálogo andou. Numa corrida o bot
    /// registrou 640 sucessos seguidos parado no nível 1, e o resumo dizia
    /// "falhas = 0". Um contador que só sabe contar acerto é pior que nenhum:
    /// ele faz o defeito parecer saúde.
    ///
    /// Zera a cada sinal de que algo mudou (oferta, aceite, progresso).
    conversas_a_toa: HashMap<u64, u32>,
    /// Últimos anúncios que o mercado devolveu.
    anuncios: Vec<shared::mercado::AnuncioNet>,
    /// Quando o bot olhou o mercado pela última vez (em decisões).
    olhou_mercado: u32,
    /// Receitas que o servidor disse existir.
    /// As receitas INTEIRAS, com os ingredientes.
    ///
    /// Guardava só o id, e por isso o bot não sabia o que faltava: ele
    /// tentava, tomava "faltam: Madeira T1 36/100" e desistia. Com os inputs
    /// ele sabe o que buscar.
    receitas: Vec<shared::protocol::CraftRecipeNet>,
    /// item -> de que nós de coleta ele sai (`FonteDeItem::Coleta`). É o
    /// "onde obter" do próprio jogo: o bot não adivinha onde acha madeira.
    fontes: HashMap<u16, Vec<u8>>,
    /// O que ele foi BUSCAR pra poder fabricar: (missão, item, quanto falta).
    buscando: Option<(u16, u16, u32)>,
    /// Já pediu pra entrar no Porão nesta vida? Uma vez basta: insistir a
    /// cada decisão seria uma enxurrada de pedidos na fila.
    pediu_dungeon: bool,
    /// Está dentro de uma dungeon agora.
    na_dungeon: bool,
    /// (missão, slot) do refino pedido e ainda sem resposta.
    ///
    /// Guardado porque o `RefinoResultado` chega no laço de mensagens, longe
    /// da decisão que pediu — e sem a missão não dá pra contar a recusa a
    /// quem ela atrasa.
    refino_pedido: Option<(u16, u16)>,
    /// Quando o bot mandou o último `Reviver` de dungeon.
    ///
    /// O aviso de instância chega a ~1,6 por segundo, e `reviver_em_s` segue
    /// em `Some(0)` até o servidor processar o pedido. Sem este freio, um
    /// pedido viraria dezenas — o observador não pode virar a maior carga do
    /// que observa.
    reviveu_em: Option<Instant>,
    /// Já tentou entrar na dungeon DEPOIS de chegar na Arena.
    ///
    /// Sem este teto o bot entraria em laço: `NaArena` chega junto com todo
    /// `Estado`, e cada um dispararia um `EntrarSolo` novo. Foi assim que
    /// `dungeon_da_missao` rendeu 406 pedidos em dez minutos sem o bot pisar
    /// numa dungeon — tentar não é entrar.
    tentou_na_arena: bool,
    /// Viajou pra ARENA e ainda não voltou.
    ///
    /// A Arena é um saguão: não tem missão, nem bicho solto, nem recurso. Um
    /// bot que chega lá e não volta é um bot parado — e parado por um motivo
    /// que o analisador leria como "PARADO sem fazer nada", longe da causa.
    na_arena: bool,
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
    /// Vida atual e máxima. A primeira vem do snapshot, a segunda do
    /// `StatsUpdate` — e o bot ignorava as duas, então bebia poção nunca.
    hp: u16,
    hp_max: u16,
    /// Quando bebeu a última poção de vida.
    ///
    /// A recarga do grupo é de 8 s (`pocoes::CURAS`) e o servidor recusa
    /// dentro dela SEM gastar a poção — mas mandar assim mesmo, a 30 Hz,
    /// seria o bot virando a maior carga do que observa.
    bebeu_em: Option<Instant>,
    /// Host da zona pra onde o servidor acabou de mandar ele.
    ///
    /// O handoff de zona TROCA DE PROCESSO: o socket atual é de outra zona e
    /// não serve mais. Sem seguir, o bot pedia pra ir à Arena, o servidor o
    /// mandava, e ele continuava falando com o socket velho — 19 pedidos de
    /// viagem e nenhuma chegada, medido em prod.
    trocar_para: Option<String>,
    /// NPCs a quem ele já pediu balcão e não deu em poção nenhuma.
    ///
    /// Sem isto, um bot sem cobre ficaria cumprimentando o mesmo ferreiro a
    /// cada decisão — movimento puro, do tipo que a trilha conta como
    /// trabalho.
    balcao_seco: std::collections::HashSet<shared::EntityId>,
    /// Está saindo de um telegráfico até este instante.
    ///
    /// Enquanto durar, a decisão não manda em nada: sair do chão marcado é
    /// mais urgente que qualquer missão, e é por isso que este campo
    /// curto-circuita o `decide` inteiro.
    desviando_ate: Option<Instant>,
}

impl Eu {
    /// Pode mandar `Reviver` agora? Um por segundo, no máximo.
    ///
    /// O aviso de instância chega a ~1,6 por segundo e `reviver_em_s` continua
    /// em `Some(0)` até o servidor atender — sem freio, um pedido viraria
    /// dezenas por segundo. Marca na hora de perguntar porque quem pergunta
    /// vai mandar.
    fn pode_reviver(&mut self) -> bool {
        let agora = Instant::now();
        if self
            .reviveu_em
            .is_some_and(|t| agora.duration_since(t) < Duration::from_secs(1))
        {
            return false;
        }
        self.reviveu_em = Some(agora);
        true
    }
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

/// Cadastra e vive, seguindo o bot de zona em zona até o prazo acabar.
///
/// O laço existe por causa do handoff: a Arena, a Ilha Mágica e a geleira são
/// PROCESSOS diferentes, e trocar de zona é trocar de socket. Um jogador nem
/// percebe; o bot precisava aprender.
///
/// Teto de viagens pra um par de zonas que se empurram não virar um bot que só
/// viaja.
async fn vive(host: &str, api: &str, nome: &str, ate: Instant, t: &Trilha) -> Result<()> {
    const VIAGENS_MAX: u32 = 40;
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

    let mut onde = host.to_string();
    for viagem in 0..VIAGENS_MAX {
        eu.trocar_para = None;
        sessao(&onde, nome, ate, t, &mut eu).await?;
        let Some(proximo) = eu.trocar_para.take() else {
            return Ok(());
        };
        if Instant::now() >= ate {
            return Ok(());
        }
        onde = proximo;
        t.registra(ev(nome, &eu, "reconectou", true, format!("{onde} (viagem {})", viagem + 1)));
    }
    Ok(())
}

/// Uma conexão, do handshake até o fim ou até o servidor mandar trocar de zona.
async fn sessao(host: &str, nome: &str, ate: Instant, t: &Trilha, eu: &mut Eu) -> Result<()> {
    let url = format!("ws://{host}/ws");
    let (mut ws, _) = tokio_tungstenite::connect_async(&url).await?;
    ws.send(envia(&ClientMessage::Handshake {
        protocol_version: shared::PROTOCOL_VERSION,
        client_version: format!("jogadorbot/{}", env!("CARGO_PKG_VERSION")),
    })?)
    .await?;

    let mut fase = Fase::Logando;
    let mut pensa = tokio::time::interval(PENSA_A_CADA);
    let mut pulsa = tokio::time::interval(PULSA_A_CADA);
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
                    decide(&mut ws, eu, nome, t, &mut seq, tick).await?;
                }
            }
            // O PULSO. Direção ZERO: mexer no direcional MATA a rota
            // ("comando manual sempre ganha do automático"), então o pulso
            // tem que ser um frame parado. Ele não pede passo nenhum — só dá
            // ao servidor o tick em que conduzir o que já foi pedido.
            _ = pulsa.tick() => {
                if fase == Fase::NoMundo && !eu.morto {
                    // Expira o empurrão: ele vale pelo trecho curto que a
                    // decisão pediu, não pra sempre.
                    if eu.empurrao_ate.is_some_and(|t| Instant::now() >= t) {
                        eu.empurrao = None;
                        eu.empurrao_ate = None;
                    }
                    let dir = eu.empurrao.unwrap_or(glam::Vec2::ZERO);
                    seq = seq.wrapping_add(1);
                    ws.send(envia(&ClientMessage::Input {
                        input: InputFrame {
                            seq,
                            tick,
                            move_dir: dir,
                            aim: if dir == glam::Vec2::ZERO { glam::Vec2::X } else { dir },
                            buttons: 0,
                        },
                    })?)
                    .await?;
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
                if let Some(nova) = recebe(msg, &mut ws, eu, nome, t, &mut tick).await? {
                    fase = nova;
                }
                // TROCOU DE ZONA: este socket é de outro processo e não vale
                // mais. Sair daqui é o que faz `vive` reconectar no host novo.
                if eu.trocar_para.is_some() {
                    return Ok(());
                }
            }
        }
    }
}

/// EMPURRA NA DIREÇÃO DE `alvo`, e diz se ainda vale insistir.
///
/// O empurrão existe pros últimos metros, onde o A* não entrega. Mas ele
/// precisa de rede EM TODO RAMO que o usa, e eu só tinha posto no ramo de
/// chegar ao destino. Ao fazer o bot andar até o nó de coleta, armei o
/// empurrão e voltei sem contar nada: sete bots ficaram parados a seis
/// unidades do nó, empurrando alguma coisa, com o batimento repetindo
/// "#601 tipo 3 a 6u · empurrando" indefinidamente.
///
/// Devolve `false` quando a distância não encurta há cinco decisões — aí quem
/// chamou decide o que fazer (chamar o A*, ou largar a missão).
fn empurra(eu: &mut Eu, alvo: glam::Vec2) -> bool {
    let (parado, ultima) = (eu.empurrao_parado, eu.dist_do_empurrao);
    let (vale, parado, ultima) = insiste(eu.pos, alvo, parado, ultima);
    eu.empurrao_parado = parado;
    eu.dist_do_empurrao = ultima;
    if !vale {
        eu.empurrao = None;
        eu.empurrao_ate = None;
        return false;
    }
    eu.empurrao = Some((alvo - eu.pos).normalize_or_zero());
    eu.empurrao_ate = Some(Instant::now() + Duration::from_millis(800));
    true
}

/// EMPURRA ATÉ O NÓ DE COLETA, com contador PRÓPRIO.
///
/// Próprio porque compartilhar custou duas horas de bot parado. Quem empurra
/// até o DESTINO zera `dist_do_empurrao` para `f32::MAX` toda vez que o corpo
/// está "perto" — e perto de um destino de coleta é qualquer lugar dentro do
/// raio da missão. Como o nó fica dentro desse raio, a sequência era:
///
///   1. perto do destino -> zera o contador e o `MAX`
///   2. empurra até o nó -> `dist < MAX - 0.3` é sempre verdade, conta zera
///   3. volta ao passo 1
///
/// `empurrao_parado` nunca chegava a 5, a rede nunca disparava, e quatro bots
/// ficaram plantados a 6 u de um nó inalcançável sem um evento na trilha. É o
/// mesmo defeito do `passo_nao_deu` reusando `craft_a_toa`: um contador que
/// serve a dois donos não serve a nenhum.
fn empurra_no(eu: &mut Eu, alvo: glam::Vec2) -> bool {
    let (vale, parado, ultima) = insiste(eu.pos, alvo, eu.no_parado, eu.dist_do_no);
    eu.no_parado = parado;
    eu.dist_do_no = ultima;
    if !vale {
        eu.empurrao = None;
        eu.empurrao_ate = None;
        return false;
    }
    eu.empurrao = Some((alvo - eu.pos).normalize_or_zero());
    eu.empurrao_ate = Some(Instant::now() + Duration::from_millis(800));
    true
}

/// A conta do empurrão, sem estado: ainda vale insistir?
///
/// Fora do `Eu` pra ser testável, e porque é ela que se duplicou errado: a
/// regra é uma só, os contadores é que são dois.
fn insiste(de: glam::Vec2, para: glam::Vec2, parado: u32, ultima: f32) -> (bool, u32, f32) {
    let dist = de.distance(para);
    // PRIMEIRA CHAMADA não conta como "não encurtou". O campo nasce em 0,0
    // pelo `Default`, e sem isto o primeiro empurrão de cada nó já começaria
    // com uma marca contra ele — cinco nós seguidos e a rede dispararia com o
    // bot andando normalmente.
    if ultima <= 0.0 {
        return (true, 0, dist);
    }
    let parado = if dist < ultima - 0.3 { 0 } else { parado + 1 };
    if parado >= 5 {
        return (false, 0, f32::MAX);
    }
    (true, parado, dist)
}

/// Mais longe que isto não é alvo, é paisagem.
const LONGE_DEMAIS: f32 = 60.0;

/// O alvo em que VALE agir, que não é o mesmo que o alvo mais perto.
///
/// `eu.alvo` é o inimigo mais próximo DOS CONHECIDOS, e conhecido pode estar a
/// duzentas unidades. A etapa 10 já filtrava por distância, mas o ramo de
/// VAGAR ainda olhava o `eu.alvo` cru — então um bicho longe demais bloqueava
/// a caminhada sem render um golpe, e o bot caía em `sem_o_que_fazer`: 2.393
/// em dez minutos, todos com `destino=None`.
///
/// Duas leituras do mesmo campo, com regras diferentes, é sempre isto. A
/// pergunta é uma só e agora a resposta também.
fn alvo_util(eu: &Eu) -> Option<(shared::EntityId, glam::Vec2)> {
    eu.alvo.filter(|(_, p)| eu.pos.distance(*p) <= LONGE_DEMAIS)
}

/// UM PASSO QUE NÃO DEU: conta e, no limite, larga a missão.
///
/// Todo ramo de destino tem um caminho de "não dá agora" — sem receita, sem
/// peça pra refinar, sem ponto de atributo, sem lista de skills. Cada um
/// desses registrava o problema, soltava o destino e deixava o laço
/// recomeçar: `sem_peca_pra_refinar` apareceu **420 vezes em dez minutos**
/// nos doze bots.
///
/// Eu vinha consertando isso caso a caso (conversa, craft, coleta) e deixando
/// os outros. Esta função é a regra única: qualquer passo que não deu conta
/// pra mesma missão, e aos três a missão sai da frente pelo prazo da
/// desistência. Um jogador faria igual — o que não dá agora se faz depois.
fn passo_nao_deu(
    eu: &mut Eu,
    t: &Trilha,
    nome: &str,
    quest: u16,
    acao: &'static str,
    detalhe: String,
) {
    let n = eu.passos_a_toa.entry((quest, acao)).or_default();
    *n += 1;
    let vezes = *n;
    t.registra(ev(nome, eu, acao, false, format!("{detalhe} (tentativa {vezes})")));
    if vezes >= 3 {
        eu.passos_a_toa.remove(&(quest, acao));
        eu.desistiu.insert(quest, Instant::now());
        eu.destino = None;
        t.registra(ev(nome, eu, "passo_emperrado", false, format!("#{quest} em '{acao}'")));
    }
}

/// Quantas poções de vida o bot tenta manter na bolsa.
///
/// O dono: "eles têm que ficar mais fortes, comprar poções melhores". Doze dá
/// pra uma dungeon inteira com folga — a recarga é de 8 s e um andar dura
/// minutos, então o que falta não é quantidade, é lembrar de beber.
const POCOES_DESEJADAS: u32 = 12;

/// Abaixo de que fração da vida o bot bebe.
///
/// 95%, pedido do dono, e é mais agressivo do que parece: com a recarga de
/// 8 s, beber cedo significa estar sempre com a cura correndo durante a luta,
/// em vez de tomar o golpe que mata e só então reagir. Foi assim que dois
/// bots morreram no andar 0 e ficaram deitados dez minutos.
const BEBE_ABAIXO_DE: f32 = 0.95;

/// As poções de VIDA que ele tem na bolsa, como `pocoes::escolher` pede.
fn pocoes_de_vida(eu: &Eu) -> Vec<(u16, u32)> {
    eu.bolsa
        .iter()
        .filter(|(id, _)| {
            shared::pocoes::cura_de(**id)
                .is_some_and(|c| c.grupo == shared::pocoes::Grupo::Vida)
        })
        .map(|(id, q)| (*id, *q))
        .collect()
}

/// PRA ONDE FUGIR de uma forma telegrafada, se houver pra onde.
///
/// Procura o ponto mais PERTO que esteja fora da marcação: fugir longe demais
/// tira o bot da luta, e o golpe só pega quem está dentro no impacto — meio
/// ombro pra fora já escapou (`Forma::contem` mede o centro do corpo).
///
/// Devolve `None` quando já está fora, ou quando nada num raio razoável
/// escapa (um `Anel` enorme, por exemplo): aí não há esquiva, e insistir
/// seria correr pra dentro de outra borda.
fn saida_do_telegrafico(
    forma: &shared::bosses::Forma,
    centro: glam::Vec2,
    dir: glam::Vec2,
    eu_pos: glam::Vec2,
) -> Option<glam::Vec2> {
    if !forma.contem(centro, dir, eu_pos) {
        return None;
    }
    let mut melhor: Option<(f32, glam::Vec2)> = None;
    let mut r = 2.0f32;
    while r <= 18.0 {
        for i in 0..16 {
            let a = i as f32 * std::f32::consts::TAU / 16.0;
            let p = eu_pos + glam::Vec2::new(a.cos() * r, a.sin() * r);
            if forma.contem(centro, dir, p) {
                continue;
            }
            // Uma folga além da borda: parar colado nela é contar com o
            // servidor arredondar a favor.
            let p = p + (p - eu_pos).normalize_or_zero() * 1.5;
            let d = eu_pos.distance(p);
            if melhor.is_none_or(|(md, _)| d < md) {
                melhor = Some((d, p));
            }
        }
        if melhor.is_some() {
            break;
        }
        r += 4.0;
    }
    melhor.map(|(_, p)| p)
}

/// A aparência deste bot, deduzida do nome.
///
/// Sai do NOME e não de sorteio: o personagem é criado uma vez e reusado a
/// cada reinício do serviço, mas se um dia o banco for limpo o mesmo bot
/// renasce com a mesma cara. Aparência que muda sozinha entre sessões é pior
/// que aparência repetida — ninguém reconhece ninguém.
fn aparencia_de(nome: &str) -> shared::aparencia::Aparencia {
    let semente = semente_do_nome(nome);
    shared::aparencia::Aparencia {
        rosto: (semente % shared::aparencia::ROSTOS as u64) as u8,
        cabelo: ((semente / 3) % shared::aparencia::CABELOS as u64) as u8,
        cor_cabelo: ((semente / 7) % shared::aparencia::CORES_DE_CABELO.len() as u64) as u8,
        pele: ((semente / 13) % shared::aparencia::TONS_DE_PELE.len() as u64) as u8,
        // Roupa 0 = o corpo padrão: skin paga não se ganha de graça nem pra bot.
        roupa: 0,
    }
    .saneada()
}

/// Uma semente estável a partir do nome do bot (FNV-1a de 64 bits).
///
/// Estável é o ponto: `prodbot_3` tem que ter a mesma cara hoje e depois de
/// um `systemctl restart`. Um `fastrand` daria variedade e nenhuma memória.
fn semente_do_nome(nome: &str) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in nome.as_bytes() {
        h ^= *b as u64;
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    h
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
                None => {
                    // CADA BOT COM CARA E CLASSE PRÓPRIAS.
                    //
                    // O dono: "eles tão todos com a mesma skin?" e "varia as
                    // skins / e as classes". Estavam mesmo: `Default::default()`
                    // em tudo e sempre a PRIMEIRA arma da lista — doze
                    // gêmeos idênticos na praça, o que é péssimo justamente
                    // pro que os bots servem, que é fazer o mundo parecer
                    // habitado.
                    //
                    // A semente sai do NOME, e não de sorteio: o personagem é
                    // criado uma vez e reusado a cada reinício do serviço, mas
                    // se um dia o banco for limpo o mesmo bot renasce com a
                    // mesma cara. Aparência que muda sozinha entre sessões é
                    // pior que aparência repetida.
                    let semente = semente_do_nome(nome);
                    let ap = aparencia_de(nome);
                    // A CLASSE é a arma inicial, e o bot pegava sempre a
                    // primeira: doze do mesmo estilo de luta.
                    let arma = if available_weapons.is_empty() {
                        0
                    } else {
                        available_weapons[(semente / 17) as usize % available_weapons.len()]
                    };
                    t.registra(ev(nome, eu, "aparencia", true, format!(
                        "rosto {} cabelo {} cor {} pele {} arma {arma}",
                        ap.rosto, ap.cabelo, ap.cor_cabelo, ap.pele
                    )));
                    ClientMessage::CreateCharacter {
                        name: format!("{nome}c"),
                        aparencia: ap,
                        starting_weapon: arma,
                        faction: Default::default(),
                    }
                }
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
            // VESTE A PRÓPRIA CARA, mesmo em personagem que já existia.
            //
            // A aparência só é escolhida na CRIAÇÃO, e os bots de produção já
            // tinham personagem criado com o padrão — sem isto eles seguiriam
            // gêmeos para sempre, e a variedade só valeria pra banco limpo.
            // `UpdateVisual` é o guarda-roupa: o servidor valida e reenvia a
            // meta pra quem já enxergava o jogador.
            ws.send(envia(&ClientMessage::UpdateVisual {
                aparencia: aparencia_de(nome),
            })?)
            .await?;
            t.registra(ev(nome, eu, "entrou_no_mundo", true, String::new()));
            return Ok(Some(Fase::NoMundo));
        }
        ServerMessage::Snapshot { snapshot } => {
            *tick = snapshot.tick;
            // O MUNDO SE MANTÉM entre snapshots, como no cliente.
            for m in &snapshot.entered {
                eu.conhecidos.insert(m.id, m.tag);
            }
            for s in &snapshot.states {
                eu.posicoes.insert(s.id, s.pos_f32());
            }
            for id in &snapshot.removed {
                eu.conhecidos.remove(id);
                eu.posicoes.remove(id);
            }
            if let Some(meu) = eu.entidade {
                if let Some(s) = snapshot.states.iter().find(|s| s.id == meu) {
                    eu.pos = s.pos_f32();
                    // DE PÉ OUTRA VEZ. `morto` só era desfeito no login, e
                    // `RespawnAtCity` não faz login nenhum: depois da
                    // primeira morte o bot ficava deitado o resto da corrida
                    // — o batimento mostrou "MORTO" por quatro minutos.
                    if eu.morto && s.hp > 0 {
                        eu.morto = false;
                        eu.destino = None;
                        eu.andando_para = None;
                        t.registra(ev(nome, eu, "renasceu", true, String::new()));
                    }
                    eu.hp = s.hp;
                }
            }
            // BEBER É REFLEXO, NÃO DECISÃO.
            //
            // Fica aqui, no snapshot (30 Hz), e não no `decide` (a cada
            // 700 ms): a vida chega por aqui, e meio segundo de atraso na
            // poção é a diferença entre curar e morrer. É também o motivo de
            // o `decide` não ser o lugar — lá a poção competiria com missão,
            // craft e mercado, e perderia.
            if !eu.morto && eu.hp_max > 0 {
                let fracao = eu.hp as f32 / eu.hp_max as f32;
                let na_recarga = eu.bebeu_em.is_some_and(|t| {
                    Instant::now().duration_since(t) < Duration::from_secs(8)
                });
                if fracao < BEBE_ABAIXO_DE && !na_recarga {
                    let familia = pocoes_de_vida(eu);
                    // A MENOR que cobre o que falta: usar a melhor poção pra
                    // curar um arranhão é jogá-la fora, e `escolher` já sabe
                    // disso — é a mesma conta do auto da barra do jogador.
                    if let Some(item) = shared::pocoes::escolher(&familia, 1.0 - fracao) {
                        if let Some((slot, _)) =
                            eu.slots.iter().find(|(_, sl)| sl.item_id == item)
                        {
                            let slot = *slot;
                            eu.bebeu_em = Some(Instant::now());
                            ws.send(envia(&ClientMessage::UseItem { slot })?).await?;
                            t.registra(ev(nome, eu, "bebeu", true, format!(
                                "item {item} com {:.0}% de vida", fracao * 100.0
                            )));
                        }
                    }
                }
            }
            // O alvo é o inimigo vivo mais perto DOS QUE ELE CONHECE.
            eu.alvo = eu
                .conhecidos
                .iter()
                .filter(|(id, tag)| {
                    Some(**id) != eu.entidade && matches!(tag, shared::EntityTag::Enemy)
                })
                .filter_map(|(id, _)| eu.posicoes.get(id).map(|p| (*id, *p)))
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
        // BALCÃO ABERTO: ABASTECE.
        //
        // O dono: "comprar poções melhores". Comprar aqui, e não numa ida
        // dedicada à loja, é o que torna isso barato: o bot já vai a NPC o
        // tempo todo por causa das missões, e toda vez que um balcão abre ele
        // sai com a bolsa cheia. Nenhuma navegação nova.
        //
        // "Melhores" é literal: compra da poção mais forte pra mais fraca,
        // até o dinheiro ou a vontade acabarem.
        ServerMessage::ShopOpen { items, .. } => {
            let tenho: u32 = pocoes_de_vida(eu).iter().map(|(_, q)| q).sum();
            if tenho >= POCOES_DESEJADAS {
                return Ok(None);
            }
            // Poção de recurso se paga com COBRE, não com ouro
            // (`pocoes::compra_com_cobre`) — olhar o ouro aqui daria um bot
            // rico que não compra nada.
            let mut cobre = eu
                .bolsa
                .get(&shared::constants::item_id::COPPER)
                .copied()
                .unwrap_or(0);
            let mut faltam = POCOES_DESEJADAS - tenho;
            // Da mais forte pra mais fraca: `Cura::total` ordena.
            let mut balcao: Vec<(u8, u16, u32, f32)> = items
                .iter()
                .enumerate()
                .filter_map(|(i, it)| {
                    shared::pocoes::cura_de(it.item_id)
                        .filter(|c| c.grupo == shared::pocoes::Grupo::Vida)
                        .map(|c| (i as u8, it.item_id, it.price, c.total()))
                })
                .collect();
            balcao.sort_by(|a, b| b.3.total_cmp(&a.3));
            let mut comprou = 0u32;
            for (slot_idx, item, preco, _) in balcao {
                while faltam > 0 && cobre >= preco {
                    ws.send(envia(&ClientMessage::ShopBuy { slot_idx })?).await?;
                    cobre -= preco;
                    faltam -= 1;
                    comprou += 1;
                }
                let _ = item;
            }
            if comprou > 0 {
                // `ok` é ter MANDADO compra, e a trilha diz isso na cara: o
                // que prova a compra é a bolsa, que chega depois no
                // `InventoryUpdate`. Contar isto como progresso seria repetir
                // o erro do `refinou`.
                t.registra(ev(nome, eu, "comprou_pocao", true, format!(
                    "{comprou} pedida(s), tinha {tenho}, sobra {cobre} de cobre"
                )));
            }
        }
        // A VIDA MÁXIMA só vem por aqui, e o bot ignorava a mensagem inteira
        // — por isso nunca soube que estava ferido.
        ServerMessage::StatsUpdate { stats, .. } => {
            eu.hp_max = stats.hp_max.max(0) as u16;
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
            eu.slots.clear();
            for (i, s) in slots.into_iter().enumerate() {
                if s.item_id == 0 || s.qty == 0 {
                    continue;
                }
                *eu.bolsa.entry(s.item_id).or_default() += s.qty;
                eu.slots.push((i as u16, s));
            }
        }
        ServerMessage::QuestLog { quests } => {
            eu.quests = quests
                .iter()
                .map(|q| (q.id, (q.progress, q.status)))
                .collect();
        }
        ServerMessage::QuestUpdate { quest_id, progress, status } => {
            let antes = eu.quests.insert(quest_id, (progress, status));
            // SÓ A MISSÃO EM FOCO ZERA O CONTADOR, e só se ELA mudou.
            //
            // Zerar em qualquer QuestUpdate era o furo: o bot conversava pela
            // #503, o servidor oferecia e ele aceitava a #602 — outra missão
            // —, o contador zerava, a #503 seguia parada, e ele voltava a
            // conversar. 215 voltas em cinco minutos, em quatro bots.
            //
            // É a quarta vez nesta sessão que eu confundo "houve evento" com
            // "houve progresso". O que vale é a missão que ele está tentando.
            if eu.foco == Some(quest_id) && antes != Some((progress, status)) {
                eu.conversas_a_toa.clear();
            }
        }
        // O NPC QUE TEM MISSÃO **E** LOJA PERGUNTA ANTES.
        //
        // `interagir` só manda a oferta direto quando o NPC não tem outra
        // função. Tendo loja, forja, barco ou banco, ele manda esta escolha —
        // e o bot a ignorava. Ou seja: mesmo chegando perto, a missão do
        // primeiro NPC da história nunca era oferecida.
        //
        // O bot sempre escolhe a MISSÃO: é o caminho do jogador que está
        // seguindo a história, e é pra isso que ele foi até lá.
        ServerMessage::EscolhaNoNpc { npc_eid, nome: npc_nome, funcao } => {
            ws.send(envia(&ClientMessage::EscolherNoNpc { npc_eid, missao: true })?)
                .await?;
            eu.conversas_a_toa.clear();
            t.registra(ev(nome, eu, "escolheu_missao", true, format!(
                "npc {npc_eid} '{npc_nome}' (tinha {funcao})"
            )));
        }
        // O SERVIDOR EXPLICA AS RECUSAS PELO CHAT DE "SYS".
        //
        // "Não há caminho até ali", "Longe demais para ir a pé daqui",
        // "Precisa do nível X" — tudo isso chega como Chat do sistema, e o
        // bot jogava fora. Resultado: 28 `travou_no_caminho` dizendo "parado
        // a 100 do destino" quando o servidor já tinha dito, em português, o
        // motivo exato.
        //
        // Chat de JOGADOR continua ignorado: isso é conversa, não diagnóstico.
        ServerMessage::Chat { from, text } if from == "SYS" || from == "Sistema" => {
            eu.ultima_recusa = Some(text.clone());
            t.registra(ev(nome, eu, "aviso_do_servidor", false, text));
        }
        ServerMessage::SkillsConfig { skills } => {
            eu.skills = skills.iter().map(|s| s.id).collect();
        }
        ServerMessage::QuestGivers { available } => eu.ofertas = available,
        ServerMessage::QuestOffer { quests, .. } => {
            // Aceita a primeira que dê pra aceitar. Escolher a "melhor" seria
            // inventar uma estratégia que o jogador comum não tem.
            if let Some(q) = quests.first() {
                ws.send(envia(&ClientMessage::AcceptQuest { quest_id: q.id })?).await?;
                // ACEITAR OUTRA MISSÃO NÃO É PROGRESSO NA QUE ELE VEIO
                // FAZER. Só zera quando a aceita é a do foco — senão um NPC
                // que oferece uma missão paralela reseta a conta pra sempre.
                if eu.foco == Some(q.id) || eu.foco.is_none() {
                    eu.conversas_a_toa.clear();
                }
                t.registra(ev(nome, eu, "quest_aceita", true, format!("#{}", q.id)));
            }
        }
        ServerMessage::QuestDestino { quest_id, tipo, pos, raio, npc_eid } => {
            // TIPO ZERO É "NÃO HÁ PRA ONDE IR NESTA ILHA", e guardar isso como
            // destino era um beco sem saída: nenhum ramo trata o tipo 0, então
            // o bot caía no `sem_o_que_fazer` e repetia. Um bot registrou 144
            // seguidas, todas com `destino=Some((503, 0, 0.0, 0))`, sem nunca
            // largar a missão — porque ter destino é o que impede de vagar.
            //
            // A missão não é impossível pra sempre (o objetivo pode estar
            // noutra ilha), então ela vai pra desistência com prazo, como
            // qualquer passo que não deu.
            if tipo == shared::quests::destino_tipo::NENHUM {
                eu.destino = None;
                eu.esperando_destino = 0;
                passo_nao_deu(eu, t, nome, quest_id, "sem_destino_nesta_ilha",
                    format!("#{quest_id}"));
                return Ok(None);
            }
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
        ServerMessage::NoDeColeta { no } => {
            eu.no_de_coleta = no.map(|(coluna, c, _, _)| (coluna, glam::Vec2::new(c[0], c[1])));
            if eu.no_de_coleta.is_some() {
                eu.pedidos_de_no = 0;
            }
        }
        ServerMessage::ColetaEstado { pausado, centro, .. } => {
            // Colhendo enquanto houver centro e não estiver pausado. É o que
            // impede o bot de mandar `ColetarNo` por cima de uma coleta que
            // já está correndo.
            eu.colhendo = centro.is_some() && !pausado;
        }
        ServerMessage::MercadoLista { anuncios, .. } => {
            eu.anuncios = anuncios;
        }
        ServerMessage::CraftRecipes { recipes } => {
            eu.receitas = recipes;
        }
        ServerMessage::ResourceSources { items } => {
            // DE ONDE SAI CADA ITEM, pela boca do próprio jogo. Sem isto o
            // bot teria que adivinhar que "Madeira T1" vem de árvore — e
            // adivinhação envelhece mal quando o conteúdo muda.
            eu.fontes = items
                .into_iter()
                .map(|i| {
                    let tipos = i
                        .sources
                        .iter()
                        .filter_map(|f| match f {
                            shared::protocol::FonteDeItem::Coleta { tipo, .. } => Some(*tipo),
                            _ => None,
                        })
                        .collect();
                    (i.item_id, tipos)
                })
                .collect();
        }
        ServerMessage::Dungeon { aviso } => {
            // Só o que muda decisão: entrou, acabou. O resto do aviso é
            // detalhe de tela, e o bot não tem tela.
            // A IDA E A VOLTA DA ARENA, que o bot faz sozinho.
            //
            // A fila e as salas moram num processo só (`shared::arena`), então
            // pedir dungeon de qualquer outra zona é recusado com um convite.
            // O bot aceita o convite, entra, e — terminada a dungeon — volta
            // pra zona de onde veio. Sem a volta ele acamparia no saguão.
            match &aviso {
                shared::dungeon::Aviso::PrecisaDaArena => {
                    ws.send(envia(&ClientMessage::Dungeon {
                        pedido: shared::dungeon::Pedido::IrParaArena,
                    })?)
                    .await?;
                    // PEDIU não é CHEGOU: `na_arena` só vira verdade quando o
                    // servidor confirma (`NaArena`). Marcar aqui seria contar
                    // a mensagem enviada como progresso — o erro que custou
                    // nove consertos nestes bots.
                    t.registra(ev(nome, eu, "pediu_arena", true, String::new()));
                    return Ok(None);
                }
                shared::dungeon::Aviso::NaArena { dentro } => {
                    eu.na_arena = *dentro;
                    if *dentro && eu.pediu_dungeon && !eu.na_dungeon {
                        if eu.tentou_na_arena {
                            // Já tentou aqui dentro e não entrou (sem entrada
                            // do dia, nível baixo, o que for). Ficar seria
                            // acampar num saguão sem missão nem bicho — então
                            // volta pra onde há o que fazer.
                            ws.send(envia(&ClientMessage::Dungeon {
                                pedido: shared::dungeon::Pedido::SairDaArena,
                            })?)
                            .await?;
                        } else {
                            eu.tentou_na_arena = true;
                            ws.send(envia(&ClientMessage::Dungeon {
                                pedido: shared::dungeon::Pedido::EntrarSolo { conteudo: 1 },
                            })?)
                            .await?;
                        }
                    }
                    t.registra(ev(nome, eu, "arena", true, format!("dentro={dentro}")));
                    return Ok(None);
                }
                _ => {}
            }
            // DENTRO OU FORA, PELA VARIANTE — e não por palavra no texto.
            //
            // Isto estava sendo decidido por `txt.contains("Entrou")`, e NÃO
            // EXISTE aviso com essa palavra: quem diz que o jogador está numa
            // instância é `Instancia`. Resultado medido: `na_dungeon` nunca
            // virava verdade, o bot pedia entrada de novo lá de dentro e
            // levava "Você já está numa dungeon." 204 vezes.
            //
            // Pior, "Sala" casava com `Salas` — a LISTA de salas, que se
            // recebe fora de qualquer dungeon. A leitura estava errada nos
            // dois sentidos.
            match &aviso {
                shared::dungeon::Aviso::Instancia { reviver_em_s, .. } => {
                    eu.na_dungeon = true;
                    // CAIU LÁ DENTRO: LEVANTA.
                    //
                    // O bot morria no andar 0 e ficava deitado até o tempo
                    // acabar — 1.484 avisos seguidos com `reviver_em_s:
                    // Some(0)` num bot só, enquanto ele registrava 767
                    // "atacou" que não tiravam um ponto de vida dos cinco
                    // inimigos. O `atacou` conta o ENVIO, então a trilha
                    // mostrava um bot ocupadíssimo e morto.
                    //
                    // Zero quer dizer "pode levantar agora"; o que não é zero
                    // é contagem regressiva, e aí só se espera.
                    if *reviver_em_s == Some(0) && eu.pode_reviver() {
                        ws.send(envia(&ClientMessage::Dungeon {
                            pedido: shared::dungeon::Pedido::Reviver,
                        })?)
                        .await?;
                        t.registra(ev(nome, eu, "reviveu_na_dungeon", true, String::new()));
                        return Ok(None);
                    }
                    // O RESTO DOS AVISOS DE INSTÂNCIA NÃO VAI PRA TRILHA.
                    //
                    // Eles chegam a ~1,6 por segundo e rendiam 988 eventos
                    // "dungeon" em dez minutos: ruído que o analisador lia
                    // como LAÇO e que escondia os avisos que importam.
                    return Ok(None);
                }
                shared::dungeon::Aviso::Saiu | shared::dungeon::Aviso::Resultado { .. } => {
                    eu.na_dungeon = false;
                    eu.reviveu_em = None;
                    if eu.na_arena {
                        ws.send(envia(&ClientMessage::Dungeon {
                            pedido: shared::dungeon::Pedido::SairDaArena,
                        })?)
                        .await?;
                    }
                }
                _ => {}
            }
            t.registra(ev(nome, eu, "dungeon", true, format!("{aviso:?}")));
        }
        ServerMessage::RefinoResultado {
            resultado,
            nivel,
            motivo,
            ..
        } => {
            use shared::forja::resultado as r;
            let Some((quest, slot)) = eu.refino_pedido.take() else {
                return Ok(None);
            };
            // SUBIR, FALHAR E DESTRUIR são o jogo acontecendo: a forja tem
            // chance, e perder a peça faz parte. O que trava o bot é o que
            // nem chegou a ser tentado — sem material, no topo, alvo inválido
            // —, porque isso se repete igual pra sempre.
            match resultado {
                r::SUBIU | r::FALHOU | r::DESTRUIU => {
                    eu.passos_a_toa.remove(&(quest, "refino_recusado"));
                    t.registra(ev(nome, eu, "refinou", true, format!(
                        "#{quest} slot {slot} -> nível {nivel}"
                    )));
                }
                _ => passo_nao_deu(eu, t, nome, quest, "refino_recusado", format!(
                    "#{quest} slot {slot}: {motivo}"
                )),
            }
        }
        // O CHEFE CARREGANDO UM GOLPE: sai de baixo.
        //
        // O dono: "desviar dos ataques dos chefes". Isto é do BOT e só dele —
        // no jogo, desviar do telegráfico é na mão, e é a graça da luta. O bot
        // é que precisa fazer com as próprias mãos o que uma pessoa faria.
        ServerMessage::Telegrafico {
            forma,
            centro,
            dir,
            carga_s,
            ..
        } => {
            let centro = glam::Vec2::from(centro);
            let dir = glam::Vec2::from(dir);
            if let Some(saida) = saida_do_telegrafico(&forma, centro, dir, eu.pos) {
                // O prazo é a CARGA, encurtada: chegar na borda no instante
                // do impacto é contar com a sorte. Teto de 3 s pra uma carga
                // longa não deixar o bot parado de fuga o tempo todo.
                let prazo = (carga_s * 0.8).clamp(0.2, 3.0);
                eu.desviando_ate = Some(Instant::now() + Duration::from_secs_f32(prazo));
                eu.empurrao = Some((saida - eu.pos).normalize_or_zero());
                eu.empurrao_ate = eu.desviando_ate;
                t.registra(ev(nome, eu, "desviou", true, format!(
                    "{:.1}u em {carga_s:.1}s", eu.pos.distance(saida)
                )));
            }
        }
        ServerMessage::MercadoResultado { ok, texto } => {
            t.registra(ev(nome, eu, "mercado_resultado", ok, texto));
        }
        ServerMessage::CraftResultado { recipe_id, ok, motivo, .. } => {
            t.registra(ev(nome, eu, "craft", ok, format!("receita {recipe_id}: {motivo}")));
            // A RECUSA TEM QUE DERRUBAR A MISSÃO.
            //
            // O bot mandava `Craft`, o servidor respondia "faltam: Madeira
            // 0/40", ele largava o destino, pedia de novo e craftava de novo:
            // 1.692 tentativas em 40 minutos, com os doze parados no mesmo
            // nível. A recusa chegava aqui e morria aqui.
            //
            // Faltar material não se resolve insistindo — se resolve indo
            // buscar, que é o que o bot passa a fazer quando esta missão sai
            // da frente.
            if ok {
                eu.craft_a_toa.clear();
            } else if let Some(q) = eu.foco {
                let n = eu.craft_a_toa.entry(q).or_default();
                *n += 1;
                if *n >= 3 {
                    eu.craft_a_toa.remove(&q);
                    eu.desistiu.insert(q, Instant::now());
                    eu.destino = None;
                    t.registra(ev(nome, eu, "craft_emperrado", false, format!(
                        "#{q} receita {recipe_id}: {motivo}"
                    )));
                }
            }
        }
        ServerMessage::Kick { reason } => return Err(anyhow!("kick: {reason}")),
        ServerMessage::TrocarZona { zona, host } => {
            // Guarda pra onde ir; quem reconecta é o laço de `vive`, que é
            // dono do socket.
            eu.trocar_para = Some(host.clone());
            t.registra(ev(nome, eu, "trocou_de_zona", true, format!("{zona} em {host}")));
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
    // O BATIMENTO: silêncio também é mentira.
    //
    // A trilha tinha dois jeitos de enganar. O primeiro era dizer `ok: true`
    // pro que não deu em nada — consertado. O segundo é não dizer NADA: numa
    // corrida o bot ficou QUATRO MINUTOS sem um único evento, andando (ou não)
    // pra um destino a 100 unidades, e a trilha ficou muda. Um segundo bot
    // passou seis minutos inteiros sem registrar uma linha, e eu não tinha
    // como saber sequer se ele estava vivo.
    //
    // A cada ~21 s sai um batimento com onde ele está e o que está tentando.
    // Não é ruído: é a diferença entre "está indo" e "travou em silêncio",
    // que nenhum contador de ação revela.
    const BATIMENTO_A_CADA: u32 = 30;
    eu.desde_o_batimento += 1;
    if eu.desde_o_batimento >= BATIMENTO_A_CADA {
        eu.desde_o_batimento = 0;
        let alvo = match eu.destino {
            Some(d) => format!(
                "#{} tipo {} a {:.0}u",
                d.quest,
                d.tipo,
                eu.pos.distance(d.pos)
            ),
            None => "sem destino".into(),
        };
        let empurrando = if eu.empurrao.is_some() { " · empurrando" } else { "" };
        t.registra(ev(nome, eu, "batimento", true, format!(
            "em {:.0},{:.0} · {} quest(s) · {alvo}{}",
            eu.pos.x,
            eu.pos.y,
            eu.quests.len(),
            if eu.morto { " · MORTO" } else { "" }
        ) + empurrando));
    }
    if eu.morto {
        return Ok(());
    }
    // FUGINDO DE UM GOLPE: nada mais importa por um segundo.
    //
    // Sem isto a decisão seguinte trocaria o empurrão da fuga pelo da missão
    // — e o bot voltaria a andar pra dentro da marcação que acabou de sair.
    if eu.desviando_ate.is_some_and(|t| Instant::now() < t) {
        return Ok(());
    }
    eu.desviando_ate = None;
    // 0,5 BALCÃO À MÃO: se passou perto de um NPC e a bolsa está seca de
    // poção, fala com ele. Vendedor abre a loja e o `ShopOpen` abastece.
    //
    // De carona na proximidade, e não uma viagem à cidade: o bot já vai a NPC
    // o tempo todo por causa das missões, e uma ida dedicada custaria minutos
    // de caminhada por uma compra que talvez nem seja possível. Se o NPC não
    // vender nada, não acontece nada — e o teto abaixo impede insistir.
    {
        let tem: u32 = pocoes_de_vida(eu).iter().map(|(_, q)| q).sum();
        if tem < POCOES_DESEJADAS / 3 {
            let perto = eu
                .conhecidos
                .iter()
                .filter(|(_, tag)| matches!(tag, shared::EntityTag::Npc))
                .filter_map(|(id, _)| eu.posicoes.get(id).map(|p| (*id, *p)))
                .filter(|(id, p)| {
                    p.distance(eu.pos) <= shared::INTERACT_RADIUS
                        && !eu.balcao_seco.contains(id)
                })
                .min_by(|a, b| {
                    a.1.distance_squared(eu.pos)
                        .total_cmp(&b.1.distance_squared(eu.pos))
                });
            if let Some((id, _)) = perto {
                // UMA VEZ POR NPC. Sem isto o bot sem dinheiro ficaria
                // cumprimentando o mesmo ferreiro para sempre — e a trilha
                // chamaria aquilo de trabalho.
                eu.balcao_seco.insert(id);
                ws.send(envia(&ClientMessage::Interact {
                    target_eid: Some(id.0 as u64),
                })?)
                .await?;
                t.registra(ev(nome, eu, "procurou_balcao", true, format!(
                    "{tem} poção(ões) na bolsa"
                )));
                return Ok(());
            }
        }
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
        // ENTREGAR NÃO É ENTREGUE, e este contador mentia igual aos outros.
        //
        // 1.714 "quest_entregue" da MESMA missão (#603) em dez minutos, com o
        // servidor respondendo todas as vezes "entregue ao Mestre de Missões,
        // na praça da cidade": a missão exige estar DIANTE do NPC, e o bot
        // mandava o pedido de onde estivesse. Marcar sucesso no envio
        // escondeu isso — e ainda enganou o analisador, que via 857
        // "entregas" e dava o bot por produtivo.
        //
        // Agora conta como passo que não deu, e aos três a missão sai da
        // frente pelo prazo da desistência — o bot vai fazer outra coisa e
        // volta quando estiver perto do NPC de verdade.
        passo_nao_deu(eu, t, nome, id, "quest_entregue", format!("#{id}"));
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
    // UMA MISSÃO DE CADA VEZ, E ELA GIRA.
    //
    // Duas tentativas anteriores erraram de lados opostos. Deixar a "ativa"
    // sair do HashMap dava uma escolha que mudava a cada decisão: o bot pedia
    // o destino de uma, recebia, e na seguinte cobrava o de outra, sem nunca
    // agir. Trocar por `min()` deu o contrário — estabilidade demais: ele
    // fixava na missão mais antiga e falou com o mesmo NPC 324 vezes em dez
    // minutos, travado no nível 2.
    //
    // O certo é ter FOCO e saber largá-lo. O bot escolhe uma, insiste nela
    // enquanto ela andar, e quando ela para de andar (`desistiu`) passa pra
    // próxima. É o que um jogador faz com uma missão que emperrou.
    // A DESISTÊNCIA TEM PRAZO.
    //
    // Numa corrida de 25 minutos com quatro bots, todos largaram a missão do
    // Mestre de Missões e depois ficaram 2.016 decisões sem ter o que fazer.
    // O motivo de não chegar nele é quase certamente PASSAGEIRO — quatro
    // bots indo ao mesmo NPC se bloqueiam —, e desistir para sempre de algo
    // que era temporário é o pior dos dois erros.
    // DEZ MINUTOS, e não dois.
    //
    // Com 120 s a missão emperrada voltava logo e o bot repetia o mesmo
    // passo impossível: 577 "sem peça pra refinar" em três minutos, mesmo com
    // a desistência funcionando. Refinar exige uma peça que ele ainda não
    // tem, e isso não muda em dois minutos — muda quando ele caça e recebe
    // drop, que é o que ele faz enquanto a missão está de lado.
    const PRAZO_DA_DESISTENCIA: Duration = Duration::from_secs(600);
    eu.desistiu
        .retain(|_, quando| quando.elapsed() < PRAZO_DA_DESISTENCIA);
    let foco_vale = eu.foco.is_some_and(|f| {
        !eu.desistiu.contains_key(&f) && !eu.impossiveis.contains(&f)
            && eu
                .quests
                .get(&f)
                .is_some_and(|(_, st)| *st == shared::quests::quest_status::ACTIVE)
    });
    if !foco_vale {
        // A MAIS NOVA primeiro: a história é numerada em ordem, então a de
        // maior id é o passo mais recente — o que o jogador acabou de pegar.
        // A HISTÓRIA PRIMEIRO, e a mais ANTIGA dela.
        //
        // O dono: "tenta seguir ao máximo a história". A cadeia é numerada em
        // ordem, então a de menor id é o passo em que ele parou. Só quando
        // não há história disponível é que ele pega outra missão — e aí a
        // mais recente, que é a que acabou de chegar.
        let ativas: Vec<u16> = eu
            .quests
            .iter()
            .filter(|(id, (_, st))| {
                *st == shared::quests::quest_status::ACTIVE
                    && !eu.desistiu.contains_key(id)
                    && !eu.impossiveis.contains(id)
            })
            .map(|(id, _)| *id)
            .collect();
        let nova = ativas
            .iter()
            .filter(|id| shared::historia::def_da_historia(**id).is_some())
            .min()
            .copied()
            .or_else(|| ativas.iter().max().copied());
        if nova != eu.foco {
            eu.foco = nova;
            eu.destino = None;
            eu.esperando_destino = 0;
            eu.tentativas_no_destino = 0;
            if let Some(f) = nova {
                t.registra(ev(nome, eu, "foco", true, format!("#{f}")));
            }
        }
    }
    let ativa = eu.foco;
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
                // REGISTRA O PEDIDO. Este ramo era mudo, e é justamente onde
                // um bot com seis missões pode ficar pingando entre elas: a
                // "ativa" sai de um HashMap, cuja ordem muda, então ele podia
                // pedir o destino de uma, receber, e na decisão seguinte
                // cobrar o de outra — para sempre, sem um evento na trilha.
                t.registra(ev(nome, eu, "pediu_destino", true, format!(
                    "#{id} (tinha {:?}, tentativa {})",
                    eu.destino.map(|d| d.quest),
                    eu.esperando_destino
                )));
                return Ok(());
            }
        }
    }
    // 4.5 BUSCANDO MATERIAL: coleta até ter, e só então volta pra missão.
    //
    // O dono: "sempre tentando fazer o personagem e aprimorar ao máximo com
    // craft, aumento de nível etc; se ficar sem recursos aí tem que ir
    // coletar até ter recursos, fazer o upgrade e voltar a tentar história
    // novamente".
    //
    // Fica ANTES do passo do destino porque a busca é o que ele está fazendo
    // agora: deixar a missão mandar aqui o traria de volta ao craft que já
    // recusou, e o ciclo recomeçaria.
    if let Some((quest, item, quanto)) = eu.buscando {
        let tem = eu.bolsa.get(&item).copied().unwrap_or(0);
        if tem >= quanto {
            eu.buscando = None;
            eu.destino = None;
            // A missão volta a valer: ela foi largada só pra buscar.
            eu.desistiu.remove(&quest);
            eu.craft_a_toa.remove(&quest);
            t.registra(ev(nome, eu, "voltou_da_busca", true, format!(
                "#{quest} com {tem}/{quanto}x item {item}"
            )));
            return Ok(());
        }
        // Os tipos de nó que rendem este item, direto do "onde obter".
        let mut tipos = [false; 5];
        let mut energia = false;
        for tipo in eu.fontes.get(&item).cloned().unwrap_or_default() {
            match tipo {
                0..=4 => tipos[tipo as usize] = true,
                5 => energia = true,
                _ => {}
            }
        }
        if tipos.iter().all(|t| !t) && !energia {
            // Não sai de coleta: buscar aqui seria bater em árvore pra sempre.
            eu.buscando = None;
            t.registra(ev(nome, eu, "busca_impossivel", false, format!(
                "#{quest} item {item} não vem de coleta"
            )));
            return Ok(());
        }
        match eu.no_de_coleta {
            Some((coluna, onde)) => {
                let d_no = eu.pos.distance(onde);
                if d_no > shared::COLETA_ALCANCE_UN * 0.6 {
                    if !empurra_no(eu, onde) {
                        // ESTE RAMO ERA MUDO, E O PREÇO FOI ALTO.
                        //
                        // O empurrão avisa quando a distância para de
                        // encurtar, mas aqui ninguém escutava: o nó era
                        // largado em silêncio, a decisão seguinte pedia
                        // outro, o servidor devolvia O MESMO (é o mais perto)
                        // e o ciclo recomeçava. QUATRO bots passaram mais de
                        // dez minutos no mesmo ponto sem registrar um único
                        // evento fora do batimento — parados de um jeito que
                        // nem o alarme de laço pegava, porque não havia o que
                        // contar.
                        //
                        // O ramo gêmeo lá embaixo (o da missão de coleta) já
                        // tinha essa rede desde o conserto anterior. Eu pus a
                        // rede num e deixei o outro, que é exatamente o erro
                        // que o `parado.rs` do cliente veio corrigir.
                        eu.no_de_coleta = None;
                        passo_nao_deu(eu, t, nome, quest, "no_inalcancavel",
                            format!("#{quest} a {d_no:.1}u do nó, buscando item {item}"));
                        // Desistiu da missão? Então a BUSCA dela também acaba:
                        // `passo_nao_deu` solta o destino, mas `buscando` é
                        // outro estado e ficaria segurando o bot aqui pra
                        // sempre.
                        if eu.desistiu.contains_key(&quest) {
                            eu.buscando = None;
                            t.registra(ev(nome, eu, "busca_emperrada", false, format!(
                                "#{quest} item {item}: o nó não se alcança daqui"
                            )));
                        }
                    }
                    return Ok(());
                }
                eu.no_de_coleta = None;
                ws.send(envia(&ClientMessage::ColetarNo { coluna })?).await?;
                t.registra(ev(nome, eu, "buscou", tem > 0, format!(
                    "#{quest} item {item}: {tem}/{quanto}"
                )));
            }
            None => {
                ws.send(envia(&ClientMessage::PedirNoDeColeta {
                    tipos,
                    energia,
                    raio: 80.0,
                    centro: [eu.pos.x, eu.pos.y],
                })?)
                .await?;
                // Registrado porque um pedido que nunca vira nó é um bot
                // parado, e sem isto a trilha não teria como dizer isso. E com
                // TETO, pelo mesmo motivo: sem nó do tipo por perto o servidor
                // não responde, e pedir para sempre é o travamento silencioso
                // que este ramo já produziu uma vez.
                eu.pedidos_de_no += 1;
                t.registra(ev(nome, eu, "pediu_no", true, format!(
                    "#{quest} item {item} ({tem}/{quanto})"
                )));
                if eu.pedidos_de_no >= 8 {
                    eu.pedidos_de_no = 0;
                    eu.buscando = None;
                    t.registra(ev(nome, eu, "busca_emperrada", false, format!(
                        "#{quest} item {item}: nada do tipo pedido em 80u"
                    )));
                }
            }
        }
        return Ok(());
    }
    // 5. Com destino: anda até lá e faz o que o tipo pede.
    if let Some(d) = eu.destino {
        // CHEGOU O BASTANTE conta como chegou.
        //
        // A trilha mostrou o auto-path parando a 5 unidades do NPC e ficando
        // lá: 102 "travou_no_caminho", todos na mesma distância. O
        // pathfinder leva até a borda do que ele considera alcançável, e os
        // últimos metros não fecham — provavelmente o NPC está sobre algo que
        // a rota não pisa.
        //
        // Insistir seria trocar um laço infinito por outro. A distância de
        // conversa do jogo é maior que isso, então a resposta certa é agir
        // dali: é o que a pessoa faz quando o personagem para perto e ela
        // clica no NPC assim mesmo.
        /// Daqui pra dentro, o direcional termina o serviço.
        //
        // VINTE E OITO, e não dezesseis. A trilha mostrou o auto-path parando
        // a 24 u do NPC de entrega e desistindo três vezes seguidas — 24 caía
        // fora dos "últimos metros", então o empurrão nem era tentado e a
        // missão ia pro limbo. O empurrão tem rede própria (cinco decisões sem
        // encurtar e ele chama o A*), então alargar aqui não cria laço.
        const ULTIMOS_METROS: f32 = 28.0;
        let travado = eu.andando_para.is_none_or(|(_, _, paradas)| paradas >= 8);
        let dist = eu.pos.distance(d.pos);
        // O QUE É "PERTO" DEPENDE DO QUE SE VAI FAZER.
        //
        // Falar com NPC exige `INTERACT_RADIUS` — TRÊS unidades. O bot antes
        // se dava por perto a até 12 e mandava o `Interact` de lá: o servidor
        // não achava NPC nenhum no alcance, não respondia nada, o destino
        // voltava e tudo recomeçava. Era isso, e só isso, os 640 "conversou"
        // de uma corrida inteira sem uma única missão aceita.
        //
        // A margem de 0,6 é porque o bot decide com a posição do último
        // snapshot: decidir no limite exato é chegar a 3,1 e mandar mesmo
        // assim.
        let fala = matches!(
            d.tipo,
            shared::quests::destino_tipo::NPC | shared::quests::destino_tipo::ENTREGA
        );
        let alcance = if fala {
            shared::INTERACT_RADIUS - 0.6
        } else {
            d.raio.max(3.0)
        };
        // TRAVADO NÃO VIRA CHEGADA quando é pra falar: insistir de longe é o
        // laço de novo, só que com outro nome. Longe e travado desiste do
        // destino, que é o caminho que já existe logo abaixo.
        let perto = dist <= alcance || (travado && !fala && dist <= 12.0);
        // OS ÚLTIMOS METROS NO DIRECIONAL.
        //
        // Auto-path pro trecho longo, direcional pro fim — que é exatamente o
        // que a pessoa faz: toca a missão, o personagem vai, e nos últimos
        // passos ela ajeita na mão.
        //
        // Sem isto o bot conversava 512 vezes sem concluir nada: o auto-path
        // parava a 5 unidades do NPC, o `Interact` não alcançava de lá, o
        // destino voltava e tudo recomeçava. "Conversou 512" era o sintoma de
        // não ter conversado nenhuma vez.
        if !perto && dist <= ULTIMOS_METROS {
            // ENCURTOU? Então segue empurrando.
            if dist < eu.dist_do_empurrao - 0.3 {
                eu.empurrao_parado = 0;
            } else {
                eu.empurrao_parado += 1;
            }
            eu.dist_do_empurrao = dist;
            // NÃO ENCURTOU EM 5 DECISÕES (~3,5 s): tem coisa no caminho.
            // Chama o A*, que sabe contornar, e zera a conta pra dar tempo a
            // ele. Se nem assim, o `travou_no_caminho` de baixo aparece.
            if eu.empurrao_parado >= 5 {
                eu.empurrao_parado = 0;
                eu.empurrao = None;
                eu.empurrao_ate = None;
                eu.andando_para = None;
                eu.tentativas_no_destino += 1;
                // TRÊS VEZES E LARGA. A primeira chamada ao A* é conserto de
                // obstáculo; a terceira já é insistência, e insistir era
                // gastar a corrida inteira num NPC que o jogo não entrega.
                if eu.tentativas_no_destino >= 3 {
                    eu.tentativas_no_destino = 0;
                    eu.desistiu.insert(d.quest, Instant::now());
                    eu.destino = None;
                    t.registra(ev(nome, eu, "destino_inalcancavel", false, format!(
                        "#{} parou a {dist:.1}u de {:.0},{:.0} — A* entrega longe e reto não passa",
                        d.quest, d.pos.x, d.pos.y
                    )));
                    return Ok(());
                }
                ws.send(envia(&ClientMessage::MoverPara { x: d.pos.x, z: d.pos.y })?).await?;
                t.registra(ev(nome, eu, "empurrao_travado", false, format!(
                    "#{} a {dist:.1}u de {:.0},{:.0} — chamando o A*",
                    d.quest, d.pos.x, d.pos.y
                )));
                return Ok(());
            }
            // ARMA O EMPURRÃO e deixa o pulso andar. Mandar UM frame aqui
            // movia 33 ms a cada 700 — na prática, parado.
            eu.empurrao = Some((d.pos - eu.pos).normalize_or_zero());
            eu.empurrao_ate = Some(Instant::now() + Duration::from_millis(800));
            return Ok(());
        }
        eu.empurrao_parado = 0;
        eu.dist_do_empurrao = f32::MAX;
        // Longe: quem conduz é a rota, e empurrão manual a MATA.
        eu.empurrao = None;
        eu.empurrao_ate = None;
        if !perto {
            // AUTO-PATH, e UMA VEZ SÓ.
            //
            // `MoverPara` é o mesmo clique-para-andar que o jogador usa ao
            // tocar a missão ou o mapa: o servidor traça a rota e conduz. O
            // caminho do jogador é este, não segurar o direcional.
            //
            // O "uma vez só" é o conserto de verdade. Eu reenviava o comando a
            // cada 700 ms e cada envio REINICIAVA o trajeto — 511 comandos de
            // movimento em 3 minutos com o personagem parado no mesmo pixel,
            // medido no banco. Eu tinha lido isso como "MoverPara não
            // funciona" e trocado pelo direcional; o defeito era o reenvio.
            let ja_mandado = eu
                .andando_para
                .is_some_and(|(alvo, _, _)| alvo.distance(d.pos) < 1.0);
            if !ja_mandado {
                ws.send(envia(&ClientMessage::MoverPara { x: d.pos.x, z: d.pos.y })?).await?;
                eu.andando_para = Some((d.pos, eu.pos, 0));
                t.registra(ev(nome, eu, "andou", true, format!(
                    "#{} -> {:.0},{:.0} (faltam {:.0})", d.quest, d.pos.x, d.pos.y, eu.pos.distance(d.pos)
                )));
                return Ok(());
            }
            // TRAVOU? Só então se insiste.
            //
            // Sem esta conferência, um trajeto que o servidor não consegue
            // traçar (porta, parede, alvo do outro lado da água) deixaria o
            // bot esperando pra sempre — e "esperando" não aparece em
            // contador nenhum, que é o pior tipo de travamento.
            if let Some((alvo, onde, paradas)) = eu.andando_para {
                let andou = eu.pos.distance(onde) > 1.0;
                if andou {
                    eu.andando_para = Some((alvo, eu.pos, 0));
                } else if paradas >= 8 {
                    // Longe E travado: desiste do destino e deixa a decisão
                    // seguir pro resto (caçar, mercado). Perto e travado já
                    // foi tratado acima como chegada.
                    eu.andando_para = None;
                    eu.destino = None;
                    let porque = eu
                        .ultima_recusa
                        .clone()
                        .unwrap_or_else(|| "sem aviso do servidor".into());
                    // O ÚLTIMO LAÇO SEM REDE.
                    //
                    // Isto soltava o destino e deixava o ciclo recomeçar:
                    // pede destino, anda, trava, pede de novo — para sempre.
                    // Um bot ficou assim a 24 unidades do Mestre de Missões,
                    // com o servidor traçando rota e o corpo sem sair do
                    // lugar ("sem aviso do servidor" = o A* achou caminho).
                    //
                    // Três voltas e a missão sai da frente, como todo o resto.
                    passo_nao_deu(eu, t, nome, d.quest, "travou_no_caminho", format!(
                        "#{} parado a {:.0} de {:.0},{:.0} — {porque}",
                        d.quest,
                        eu.pos.distance(d.pos),
                        d.pos.x,
                        d.pos.y
                    ));
                } else {
                    eu.andando_para = Some((alvo, onde, paradas + 1));
                }
            }
            return Ok(());
        }
        eu.andando_para = None;
        use shared::quests::destino_tipo;
        match d.tipo {
            // FALAR: a primeira missão da história é esta, e era ela que
            // travava tudo — o bot chegava (ou nem isso) e não sabia
            // conversar.
            destino_tipo::NPC | destino_tipo::ENTREGA => {
                if let Some(npc) = d.npc {
                    ws.send(envia(&ClientMessage::Interact { target_eid: Some(npc) })?).await?;
                    ws.send(envia(&ClientMessage::ConcluirConversa { npc_eid: npc })?).await?;
                    // O SUCESSO NÃO É TER MANDADO A MENSAGEM.
                    //
                    // Enquanto nada voltar (oferta, escolha ou progresso), a
                    // conta deste NPC sobe e a trilha registra FALHA a partir
                    // da terceira. É o que teria mostrado, na primeira
                    // corrida, que 640 "conversou" eram 640 nadas.
                    let n = eu.conversas_a_toa.entry(npc).or_default();
                    *n += 1;
                    let vezes = *n;
                    t.registra(ev(
                        nome,
                        eu,
                        "conversou",
                        vezes < 3,
                        format!("#{} npc {npc} a {dist:.1}u (tentativa {vezes})", d.quest),
                    ));
                    // CINCO CONVERSAS SEM NADA: esta missão não anda por
                    // falar. Larga e vai pra próxima — insistir foi o que deu
                    // 324 conversas com o mesmo NPC numa corrida inteira.
                    if vezes >= 5 {
                        eu.conversas_a_toa.remove(&npc);
                        eu.desistiu.insert(d.quest, Instant::now());
                        t.registra(ev(nome, eu, "missao_emperrada", false, format!(
                            "#{} — {vezes} conversas com o npc {npc} e nada mudou", d.quest
                        )));
                    }
                    eu.destino = None;
                    eu.esperando_destino = 0;
                    return Ok(());
                }
            }
            destino_tipo::COLETA => {
                // Já colhendo: não se manda nada. `ColetarNo` por cima de uma
                // coleta em curso a reinicia — o mesmo erro do `MoverPara`,
                // que custou 511 comandos e nenhum passo.
                //
                // MAS COM TETO. Sem ele, `colhendo` preso em `true` fazia o
                // bot voltar cedo indefinidamente: sete bots plantados no
                // mesmo ponto, sem um evento na trilha.
                const COLHENDO_MAX: Duration = Duration::from_secs(25);
                if eu.colhendo {
                    let desde = *eu.colhendo_desde.get_or_insert(Instant::now());
                    if desde.elapsed() < COLHENDO_MAX {
                        return Ok(());
                    }
                    // Tempo demais "colhendo" sem o objetivo andar: o nó
                    // acabou, ou nunca rendeu. Larga e procura outro.
                    eu.colhendo = false;
                    eu.colhendo_desde = None;
                    eu.no_de_coleta = None;
                    passo_nao_deu(eu, t, nome, d.quest, "coleta_sem_render", format!(
                        "#{} {}s colhendo sem o objetivo andar",
                        d.quest,
                        COLHENDO_MAX.as_secs()
                    ));
                    return Ok(());
                }
                eu.colhendo_desde = None;
                match eu.no_de_coleta {
                    Some((coluna, onde)) => {
                        // ANDA ATÉ O NÓ ANTES DE COLHER.
                        //
                        // `COLETA_ALCANCE_UN` é curto e o nó pode estar a até
                        // 40 unidades (o raio que o bot pede). Mandar
                        // `ColetarNo` de longe é pedir o que o servidor
                        // recusa — e era exatamente isso que mantinha o
                        // progresso em zero.
                        let d_no = eu.pos.distance(onde);
                        if d_no > shared::COLETA_ALCANCE_UN * 0.6 {
                            if !empurra_no(eu, onde) {
                                // Não encurta: o nó pode estar do outro lado
                                // de uma pedra. Larga ESTE nó e peça outro; se
                                // for a missão inteira que não anda, o
                                // `coleta_emperrada` cuida.
                                eu.no_de_coleta = None;
                                passo_nao_deu(eu, t, nome, d.quest, "no_inalcancavel",
                                    format!("#{} a {d_no:.1}u do nó", d.quest));
                            }
                            return Ok(());
                        }
                        eu.empurrao = None;
                        eu.empurrao_ate = None;
                        eu.no_de_coleta = None;
                        ws.send(envia(&ClientMessage::ColetarNo { coluna })?).await?;
                        // O SUCESSO É O OBJETIVO ANDAR, não o envio.
                        //
                        // Marcar `true` por ter mandado `ColetarNo` escondeu
                        // um laço de 375 coletas em dez minutos com o bot
                        // parado no nível 1 — a mesma mentira dos 640
                        // "conversou", pela terceira vez neste arquivo.
                        let agora_p = eu.quests.get(&d.quest).map_or(0, |(p, _)| *p);
                        let andou = eu.progresso_visto != (d.quest, agora_p);
                        if andou {
                            eu.progresso_visto = (d.quest, agora_p);
                            eu.coleta_sem_avanco = 0;
                            eu.colhendo_desde = None;
                        } else {
                            eu.coleta_sem_avanco += 1;
                        }
                        t.registra(ev(nome, eu, "coletou", andou, format!(
                            "#{} coluna {coluna} (progresso {agora_p}, {} sem avanço)",
                            d.quest, eu.coleta_sem_avanco
                        )));
                        // O nó pode estar esgotado, ou a missão pode pedir
                        // outro recurso. Nos dois casos insistir não resolve:
                        // larga a missão e vai fazer outra coisa.
                        if eu.coleta_sem_avanco >= 40 {
                            eu.coleta_sem_avanco = 0;
                            eu.desistiu.insert(d.quest, Instant::now());
                            eu.destino = None;
                            t.registra(ev(nome, eu, "coleta_emperrada", false, format!(
                                "#{} 40 coletas sem o objetivo andar", d.quest
                            )));
                        }
                    }
                    None => {
                        // PEDE O RECURSO QUE A MISSÃO QUER, não qualquer um.
                        //
                        // Com `[true; 5]` o bot pegava o nó mais perto seja
                        // ele qual for: a 704 pede ÁRVORE, a 705 pede PEDRA, e
                        // três bots passaram a corrida inteira derrubando o
                        // recurso errado — 40 coletas seguidas sem o objetivo
                        // andar, parados no nível 1. A desistência salvava o
                        // bot do laço, mas não fazia a missão.
                        //
                        // `tipos[0]` é madeira e `tipos[1..4]` são as pedras
                        // (`server::coleta::aceita`); o alvo sai da própria
                        // definição da missão.
                        use shared::quests::alvo_de_coleta as alvo;
                        let quer = shared::historia::def_da_historia(d.quest)
                            .map_or(alvo::QUALQUER, |q| q.obj_target);
                        let tipos = match quer {
                            alvo::ARVORE => [true, false, false, false, false],
                            alvo::PEDRA => [false, true, true, true, true],
                            _ => [true; 5],
                        };
                        ws.send(envia(&ClientMessage::PedirNoDeColeta {
                            tipos,
                            energia: quer == alvo::QUALQUER,
                            raio: 40.0,
                            centro: [eu.pos.x, eu.pos.y],
                        })?)
                        .await?;
                        // PEDIR NÃO É RECEBER, e sem nó por perto o servidor
                        // simplesmente não responde. Com o ramo mudo e sem
                        // teto, quatro bots ficaram meia hora no mesmo ponto
                        // sem um evento fora do batimento — e sem um único XP.
                        eu.pedidos_de_no += 1;
                        if eu.pedidos_de_no >= 8 {
                            eu.pedidos_de_no = 0;
                            passo_nao_deu(eu, t, nome, d.quest, "sem_no_por_perto",
                                format!("#{} nada do tipo pedido em 40u", d.quest));
                        }
                    }
                }
                return Ok(());
            }
            // OS PASSOS QUE NÃO SE ANDA: painel de craft, forja e dungeon.
            //
            // Eles são `destino_tipo` como os outros, mas não têm pra onde ir
            // — o cliente ABRE uma janela. O bot caía no `_ => {}` e
            // registrava "sem o que fazer": 439 numa corrida de oito minutos,
            // todas com `tipo 6` (a forja) no detalhe.
            destino_tipo::PAINEL_CRAFT => {
                // A RECEITA QUE ELE CONSEGUE FAZER, e não a primeira da
                // lista. Tentar uma sem material é garantir a recusa.
                let escolhida = eu
                    .receitas
                    .iter()
                    .find(|r| {
                        r.nivel_min as u32 <= eu.nivel
                            && r.inputs.iter().all(|[item, q]| {
                                eu.bolsa.get(&(*item as u16)).copied().unwrap_or(0) >= *q
                            })
                    })
                    .or_else(|| {
                        eu.receitas
                            .iter()
                            .find(|r| r.nivel_min as u32 <= eu.nivel)
                    })
                    .map(|r| (r.id, r.inputs.clone()));
                match escolhida {
                    Some((r, inputs)) => {
                        // FALTA MATERIAL? VAI BUSCAR, não desiste.
                        //
                        // O dono: "se ficar sem recursos aí tem que ir
                        // coletar até ter recursos, fazer o upgrade e voltar
                        // a tentar história novamente". Antes ele largava a
                        // missão por 10 minutos e ia fazer outra coisa — o
                        // personagem nunca melhorava.
                        // GUARDA O ALVO, NÃO O QUE FALTA.
                        //
                        // Eu guardava o DÉFICIT e comparava contra o TOTAL da
                        // bolsa: com 90 de 100, o déficit era 10, e `90 >= 10`
                        // dava "já tenho" na mesma decisão. Resultado: 2.580
                        // "foi buscar" e 2.569 "voltou da busca" em meia hora,
                        // sem colher nada. O alvo é a quantidade CHEIA.
                        // TODOS OS QUE FALTAM, e não só o primeiro.
                        //
                        // A receita 200 pede "Madeira T2, Aço Cinza, Couro
                        // T2". O bot olhava só o primeiro que faltava, via
                        // que ele saía de coleta, e ia buscar — mas os T2 NÃO
                        // saem de coleta, são refinados. Ele enchia a bolsa
                        // do coletável, o craft recusava do mesmo jeito, a
                        // forja consumia o que ele tinha colhido, e a conta
                        // caía de novo. Ida e volta eterna, com o craft em
                        // ZERO sucessos em 58 tentativas.
                        let faltando: Vec<(u16, u32)> = inputs
                            .iter()
                            .filter_map(|[item, q]| {
                                let item = *item as u16;
                                let tem = eu.bolsa.get(&item).copied().unwrap_or(0);
                                (tem < *q).then_some((item, *q))
                            })
                            .collect();
                        // Algum que NÃO vem de coleta? Então buscar não
                        // resolve, por mais que se colha. Larga a missão —
                        // insistir era o laço.
                        let impossivel = faltando.iter().find(|(item, _)| {
                            eu.fontes.get(item).is_none_or(|t| t.is_empty())
                        });
                        if let Some((item, _)) = impossivel {
                            // DE VEZ, e não pelo prazo da desistência.
                            //
                            // "Não sai de coleta" é estrutural: a receita é a
                            // mesma daqui a dez minutos, e a desistência com
                            // prazo fazia o bot voltar pra descobrir de novo —
                            // 96 `craft_sem_caminho` em dez minutos, sempre as
                            // mesmas duas missões e o mesmo item 61.
                            //
                            // Um jogador que descobre que não tem como fazer a
                            // peça não volta a cada dez minutos pra conferir.
                            eu.impossiveis.insert(d.quest);
                            eu.destino = None;
                            t.registra(ev(nome, eu, "missao_impossivel", false, format!(
                                "#{} item {item} não sai de coleta — largada de vez", d.quest
                            )));
                            return Ok(());
                        }
                        let falta = faltando.first().copied();
                        if let Some((item, quanto)) = falta {
                            // Só vale buscar o que SAI DE COLETA. Material de
                            // mob ou de loja não se resolve batendo em árvore.
                            if eu.fontes.get(&item).is_some_and(|t| !t.is_empty()) {
                                // TETO NO VAIVÉM. Ir buscar e voltar sem que
                                // o craft ande é laço, por mais que cada
                                // metade pareça certa: 589 idas e 581 voltas
                                // em quatro minutos, com 16 coletas.
                                let n = eu.passos_a_toa.entry((d.quest, "foi_buscar")).or_default();
                                *n += 1;
                                if *n > 6 {
                                    eu.passos_a_toa.remove(&(d.quest, "foi_buscar"));
                                    eu.buscando = None;
                                    eu.desistiu.insert(d.quest, Instant::now());
                                    eu.destino = None;
                                    t.registra(ev(nome, eu, "busca_em_circulo", false, format!(
                                        "#{} item {item}: seis idas sem o craft andar", d.quest
                                    )));
                                    return Ok(());
                                }
                                eu.buscando = Some((d.quest, item, quanto));
                                eu.destino = None;
                                let tem = eu.bolsa.get(&item).copied().unwrap_or(0);
                                t.registra(ev(nome, eu, "foi_buscar", true, format!(
                                    "#{} precisa de {quanto}x item {item} (tem {tem})", d.quest
                                )));
                                return Ok(());
                            }
                        }
                        ws.send(envia(&ClientMessage::Craft { recipe_id: r })?).await?;
                        // `ok` SAI DA CONTA DE RECUSAS, e não do envio.
                        //
                        // Registrar `true` por ter mandado a mensagem é a
                        // mesma mentira que custou 640 "conversou" a nada no
                        // começo — e eu a reintroduzi aqui, o que rendeu
                        // 1.692 "craft_da_missao" marcados como sucesso
                        // enquanto os doze bots giravam em falso.
                        let recusas = eu.craft_a_toa.get(&d.quest).copied().unwrap_or(0);
                        t.registra(ev(nome, eu, "craft_da_missao", recusas == 0, format!(
                            "#{} receita {r} (recusas {recusas})", d.quest
                        )));
                    }
                    None => passo_nao_deu(eu, t, nome, d.quest, "craft_sem_receita", format!("#{}", d.quest)),
                }
                eu.destino = None;
                eu.esperando_destino = 0;
                return Ok(());
            }
            destino_tipo::PAINEL_FORJA => {
                // Refina a primeira peça com instância — peça única é o que a
                // forja aceita. Sem nenhuma, o passo espera o drop.
                match eu.slots.iter().find(|(_, sl)| sl.instance.is_some()) {
                    Some((slot, _)) => {
                        let slot = *slot;
                        ws.send(envia(&ClientMessage::Refinar {
                            alvo: shared::protocol::AlvoDaForja::Bolsa(slot),
                        })?)
                        .await?;
                        // QUEM DIZ SE REFINOU É A RESPOSTA, não o envio.
                        //
                        // Isto marcava `ok: true` por ter MANDADO a mensagem —
                        // a mesma mentira que já custou caro no craft e na
                        // conversa. Um bot registrou "refinou #604 slot 8"
                        // 1.514 vezes, sempre o mesmo slot, sem sair do nível.
                        // (Ele estava morto dentro de uma dungeon; o contador
                        // não tinha como saber, porque nunca olhou a resposta.)
                        //
                        // Repetir o slot NÃO serve de sinal: refinar a mesma
                        // peça de novo é o uso normal da forja. O que serve é
                        // `RefinoResultado`, que o servidor já manda e que o
                        // bot ignorava — ele é quem sabe a diferença entre
                        // "subiu" e "sem material".
                        eu.refino_pedido = Some((d.quest, slot));
                    }
                    None => passo_nao_deu(eu, t, nome, d.quest, "sem_peca_pra_refinar", format!("#{}", d.quest)),
                }
                eu.destino = None;
                eu.esperando_destino = 0;
                return Ok(());
            }
            destino_tipo::PAINEL_DUNGEON => {
                // O `raio` carrega o id do conteúdo (0 = qualquer).
                let conteudo = if d.raio as u16 == 0 { 1 } else { d.raio as u16 };
                if !eu.na_dungeon {
                    ws.send(envia(&ClientMessage::Dungeon {
                        pedido: shared::dungeon::Pedido::EntrarSolo { conteudo },
                    })?)
                    .await?;
                    // COM TETO, como todo passo que pode não dar. Pedir
                    // entrada e não entrar rendeu 406 pedidos em dez minutos
                    // (e 979 respostas), sem o bot pisar numa dungeon.
                    passo_nao_deu(eu, t, nome, d.quest, "dungeon_da_missao", format!(
                        "#{} conteúdo {conteudo}", d.quest
                    ));
                }
                eu.destino = None;
                eu.esperando_destino = 0;
                return Ok(());
            }
            // TRAVA DE NÍVEL: não há pra onde ir, e conversar não resolve. A
            // saída é subir — ou seja, caçar, que é o que o fim da decisão já
            // faz. Larga a missão pelo prazo da desistência e segue.
            destino_tipo::TRAVA => {
                eu.desistiu.insert(d.quest, Instant::now());
                eu.destino = None;
                t.registra(ev(nome, eu, "trava_de_nivel", false, format!(
                    "#{} — nível {} não basta", d.quest, eu.nivel
                )));
                return Ok(());
            }
            // LUGAR: chegar É o objetivo, e o servidor conclui sozinho. Aqui
            // já se chegou (estamos no ramo do `perto`): só soltar o destino.
            destino_tipo::LUGAR => {
                eu.destino = None;
                eu.esperando_destino = 0;
                t.registra(ev(nome, eu, "chegou_no_lugar", true, format!("#{}", d.quest)));
                return Ok(());
            }
            // ZONA DE BICHO: chegou, agora BATE.
            //
            // O bot chegava na zona e caía no `_ => {}`: nada acontecia, e
            // ele registrava "sem_o_que_fazer" 103 vezes em pé no meio dos
            // lobos. A missão de matar N bichos nunca andava, e como ela
            // trava a história, o bot parava no nível 2 para sempre.
            //
            // Aqui não se chama auto-combate: o alvo mais perto e o soco são
            // o que o jogador faz, e é o mesmo caminho que a seção 10 já usa
            // quando não há mais nada pendente.
            destino_tipo::COMBATE => {
                if let Some((alvo, p)) = eu.alvo {
                    ws.send(envia(&ClientMessage::SetTarget { target: Some(alvo) })?).await?;
                    let dir = (p - eu.pos).normalize_or_zero();
                    let longe = eu.pos.distance(p) > 2.0;
                    // Longe do bicho: o pulso leva até ele. Perto: para de
                    // empurrar e só bate, senão o corpo atravessa o alvo.
                    if longe {
                        empurra(eu, p);
                    } else {
                        eu.empurrao = None;
                        eu.empurrao_ate = None;
                        eu.empurrao_parado = 0;
                        eu.dist_do_empurrao = f32::MAX;
                    }
                    *seq = seq.wrapping_add(1);
                    ws.send(envia(&ClientMessage::Input {
                        input: InputFrame {
                            seq: *seq,
                            tick,
                            move_dir: glam::Vec2::ZERO,
                            aim: dir,
                            buttons: 1,
                        },
                    })?)
                    .await?;
                    return Ok(());
                }
                // Sem bicho à vista dentro da zona: anda pro meio dela, que é
                // onde eles nascem.
                eu.empurrao = Some((d.pos - eu.pos).normalize_or_zero());
                eu.empurrao_ate = Some(Instant::now() + Duration::from_millis(800));
                return Ok(());
            }
            // PASSO DE TUTORIAL. Não se anda: faz-se o gesto.
            //
            // Era aqui que o bot parava depois de destravar a conversa: 219
            // "sem_o_que_fazer" numa corrida de 4 minutos, todos com
            // `destino=(790, 10)`. Tipo 10 é TUTORIAL e o `raio` carrega a
            // ação (`shared::quests::tutorial`).
            //
            // Os passos se dividem em dois, e o bot precisa dos dois:
            //
            // * os de INTERFACE (ligar o auto, mexer na barra, tocar o mapa)
            //   só o cliente vê, e ele os reporta com `ClientMessage::Tutorial`
            //   — é literalmente a mesma mensagem que o jogo manda quando a
            //   pessoa faz o gesto;
            // * os de SALDO (ponto, tier, Energia) o servidor conta ONDE A
            //   AÇÃO ACONTECE, e mandar `Tutorial` não adiantaria nada: o bot
            //   tem que fazer a coisa.
            destino_tipo::TUTORIAL => {
                use shared::quests::tutorial as tut;
                let acao = d.raio as u16;
                match acao {
                    tut::PONTO_ATRIBUTO => {
                        if eu.pontos_livres > 0 {
                            ws.send(envia(&ClientMessage::AllocStatPoint { stat: 0 })?).await?;
                            t.registra(ev(nome, eu, "tutorial_ponto", true, String::new()));
                        } else {
                            // Sem ponto livre não há o que gastar: o passo
                            // espera o próximo nível, e dizer "feito" seria
                            // mentir pro servidor.
                            passo_nao_deu(eu, t, nome, d.quest, "tutorial_espera", "sem ponto livre".into());
                        }
                    }
                    tut::EVOLUIR_SKILL => {
                        match eu.skills.first().copied() {
                            Some(skill_id) => {
                                ws.send(envia(&ClientMessage::EvoluirSkill { skill_id })?).await?;
                                t.registra(ev(nome, eu, "tutorial_evoluiu", true, format!("skill {skill_id}")));
                            }
                            None => {
                                // Sem a lista não há id pra mandar, e um id
                                // inventado vira recusa silenciosa. Ela vem
                                // no login; se não veio, é isso que a trilha
                                // tem que dizer.
                                passo_nao_deu(eu, t, nome, d.quest, "tutorial_espera", "sem lista de skills".into());
                            }
                        }
                    }
                    // Os de interface: o gesto é a mensagem.
                    _ => {
                        ws.send(envia(&ClientMessage::Tutorial { acao })?).await?;
                        t.registra(ev(nome, eu, "tutorial_feito", true, format!("#{} ação {acao}", d.quest)));
                    }
                }
                eu.destino = None;
                eu.esperando_destino = 0;
                return Ok(());
            }
            _ => {}
        }
    }
    // 6. O MERCADO, de vez em quando.
    //
    // A cada ~40 decisões (uns 30 s), e não toda vez: o bot não é um robô de
    // arbitragem, é um jogador. Olhar o mercado a cada 700 ms seria uma carga
    // que nenhum jogador faz e que sujaria a telemetria que a gente quer ler.
    //
    // E SÓ A PARTIR DO NÍVEL 20, que é o que o mercado exige. Abaixo disso o
    // servidor respondia "Precisa do nível 20 para vender no mercado" a cada
    // tentativa — 280 recusas em trinta minutos, medidas em prod, todas
    // escondidas na lista de falha esperada do analisador. Pedir o que não se
    // pode não é um jogador insistente, é um contador girando.
    const NIVEL_DO_MERCADO: u32 = 20;
    eu.olhou_mercado = eu.olhou_mercado.saturating_add(1);
    if eu.nivel >= NIVEL_DO_MERCADO && eu.olhou_mercado >= 40 {
        eu.olhou_mercado = 0;
        // VENDE a maior pilha que não seja equipamento nem consumível de vida:
        // é o que um jogador larga no mercado. Sem instância, porque item com
        // instância é peça única e vender a peça equipada seria sabotagem.
        let vender = eu
            .slots
            .iter()
            .filter(|(_, s)| s.instance.is_none() && s.qty >= 5)
            .max_by_key(|(_, s)| s.qty)
            .map(|(i, s)| (*i, s.item_id, s.qty));
        if let Some((slot, item, qtd)) = vender {
            // Metade da pilha, nunca tudo: esvaziar a pilha que uma missão
            // pede seria o bot competindo com ele mesmo.
            let q = (qtd / 2).max(1);
            ws.send(envia(&ClientMessage::MercadoAnunciar {
                inv_slot: slot,
                qtd: q,
                preco_unit: 10,
            })?)
            .await?;
            t.registra(ev(nome, eu, "anunciou", true, format!("item {item} x{q}")));
            return Ok(());
        }
        ws.send(envia(&ClientMessage::MercadoBuscar {
            filtro: shared::mercado::FiltroNet {
                categoria: 0,
                texto: String::new(),
                pagina: 0,
            },
        })?)
        .await?;
        t.registra(ev(nome, eu, "olhou_mercado", true, String::new()));
        return Ok(());
    }
    // 7. COMPRA o mais barato que caiba no bolso, quando há o que comprar.
    if eu.ouro > 200 {
        if let Some(a) = eu
            .anuncios
            .iter()
            .filter(|a| a.preco_unit > 0 && (a.preco_unit as i64) < eu.ouro / 4)
            .min_by_key(|a| a.preco_unit)
        {
            let (id, preco, item) = (a.id.clone(), a.preco_unit, a.item_id);
            eu.anuncios.clear();
            ws.send(envia(&ClientMessage::MercadoComprar {
                anuncio: id,
                qtd: 1,
                preco_unit: preco,
            })?)
            .await?;
            t.registra(ev(nome, eu, "comprou", true, format!("item {item} por {preco}")));
            return Ok(());
        }
    }
    // 8. CRAFT: tenta uma receita de vez em quando.
    //
    // Sem conferir se os materiais dão: o servidor recusa com motivo, e a
    // recusa vai pra trilha. Conferir aqui seria reimplementar a receita no
    // bot — duas regras pro mesmo assunto, e a do bot ficaria velha.
    if !eu.receitas.is_empty() && eu.olhou_mercado == 20 {
        let r = eu.receitas[(eu.nivel as usize) % eu.receitas.len()].id;
        ws.send(envia(&ClientMessage::Craft { recipe_id: r })?).await?;
        t.registra(ev(nome, eu, "tentou_craft", true, format!("receita {r}")));
        return Ok(());
    }
    // 9. DUNGEON: o Porão, que entra sozinho, a partir do nível 5.
    //
    // Uma vez por vida do bot. Insistir a cada decisão seria uma enxurrada de
    // pedidos numa fila que é do servidor — o observador não pode virar a
    // maior carga do que observa.
    if eu.nivel >= 5 && !eu.pediu_dungeon && !eu.na_dungeon {
        eu.pediu_dungeon = true;
        ws.send(envia(&ClientMessage::Dungeon {
            pedido: shared::dungeon::Pedido::EntrarSolo { conteudo: 1 },
        })?)
        .await?;
        t.registra(ev(nome, eu, "pediu_dungeon", true, "Porão".into()));
        return Ok(());
    }
    // 10. Sem mais nada a fazer: bate no inimigo mais perto.
    //
    // MAS SÓ SE ELE ESTIVER AO ALCANCE, e essa linha faltava.
    //
    // `eu.alvo` é o inimigo mais perto DOS CONHECIDOS, e conhecido não quer
    // dizer alcançável: pode estar a duzentas unidades, do outro lado de uma
    // parede. O bot mandava o soco assim mesmo e, pior, mandava `move_dir` na
    // direção dele a cada decisão — e comando manual MATA a rota do servidor.
    // Daí os dois sintomas juntos na trilha: 767 "atacou" em dez minutos com
    // ZERO de XP, e "rota não saiu" no desencalhe logo em seguida. Ele
    // perseguia no direcional um alvo que nunca alcançava, e o direcional
    // cancelava a única coisa que o tiraria dali.
    //
    // O alcance do básico é 1,8 corpo a corpo e 9 à distância; 12 cobre os dois
    // com folga pro alvo estar andando.
    const ALCANCE_DO_SOCO: f32 = 12.0;
    if let Some((id, p)) = alvo_util(eu) {
        let dist = eu.pos.distance(p);
        if dist > ALCANCE_DO_SOCO {
            // Vai até ele PELA ROTA, uma vez só — o mesmo cuidado do destino
            // de missão. Reenviar a cada decisão reinicia o trajeto.
            if eu.indo_ao_alvo != Some(id) {
                eu.indo_ao_alvo = Some(id);
                ws.send(envia(&ClientMessage::MoverPara { x: p.x, z: p.y })?).await?;
                t.registra(ev(nome, eu, "foi_ao_alvo", true, format!("{dist:.0}u")));
            }
            return Ok(());
        }
        eu.indo_ao_alvo = None;
        ws.send(envia(&ClientMessage::SetTarget { target: Some(id) })?).await?;
        *seq = seq.wrapping_add(1);
        let dir = (p - eu.pos).normalize_or_zero();
        ws.send(envia(&ClientMessage::Input {
            input: InputFrame {
                seq: *seq,
                tick,
                move_dir: if dist > 2.0 { dir } else { glam::Vec2::ZERO },
                aim: dir,
                buttons: 1,
            },
        })?)
        .await?;
        t.registra(ev(nome, eu, "atacou", true, String::new()));
        return Ok(());
    }
    // O DETALHE TEM QUE BASTAR PRA CONSERTAR.
    //
    // A primeira versão dizia só `(quest, tipo)`, e o `raio` — que é o que
    // diz QUAL passo de tutorial — ficava de fora. Eu precisei ir ao código
    // pra descobrir o que "tipo 10" queria. Agora vem tudo.
    // SEM NADA A FAZER, UM JOGADOR ANDA.
    //
    // Ficar parado registrando "sem o que fazer" duas mil vezes não é o que
    // ninguém faz — e, pior, não produz nem o dado que o bot existe pra
    // gerar. Ele sai pra caçar: escolhe um ponto a algumas dezenas de
    // unidades e vai, que é como se acha bicho quando a missão emperrou.
    //
    // O `%` sobre o relógio é um sorteio bom o bastante: não precisa de
    // aleatoriedade boa, precisa de direções diferentes a cada vez.
    if eu.destino.is_none() && alvo_util(eu).is_none() {
        // VAGAR COM COMPROMISSO, e esta é a diferença entre andar e girar.
        //
        // Antes escolhia um ponto novo a cada ~5,6 s e mandava `MoverPara`
        // outra vez. Cada envio REINICIA o trajeto no servidor — o mesmo
        // defeito que já tinha custado 511 comandos de movimento com o
        // personagem parado no mesmo pixel. Aqui rendeu 275 "vagou" em trinta
        // minutos com o bot no MESMO ponto (-50,-464) o tempo todo, e zero XP:
        // ele nunca chegava a lugar nenhum, então nunca encontrava bicho.
        //
        // Agora ele escolhe um destino e VAI até lá. Só escolhe outro quando
        // chega, ou quando desiste — e as duas coisas vão pra trilha com `ok`
        // diferente, que é o que separa "andou" de "tentou andar".
        const PERTO_O_BASTANTE: f32 = 5.0;
        const PRAZO_DA_VOLTA: Duration = Duration::from_secs(25);
        /// A que distância do ponto de vagar se anda: ver `LONGE`.
        const LONGE: f32 = 25.0;
        if let Some((alvo, desde)) = eu.vagando_para {
            if eu.pos.distance(alvo) <= PERTO_O_BASTANTE {
                eu.vagando_para = None;
                t.registra(ev(nome, eu, "vagou", true, format!(
                    "chegou em {:.0},{:.0}", alvo.x, alvo.y
                )));
                return Ok(());
            }
            if Instant::now().duration_since(desde) < PRAZO_DA_VOLTA {
                // Andando. Não reenvia nada: o servidor está conduzindo.
                return Ok(());
            }
            // NÃO SAIU DO LUGAR: anda NA MÃO.
            //
            // Agora que o vagar conta a verdade, ela apareceu: "não chegou
            // (parou a 45u)" — o bot nem começou a andar. Quatro deles
            // passaram a corrida inteira em -51,-464 mandando `MoverPara` pra
            // todo lado e ficando onde estavam: o A* do servidor não traça
            // rota dali, seja porque o ponto sorteado caiu na água, seja
            // porque o corpo está encurralado.
            //
            // É o que uma pessoa faz quando clicar no mapa não resolve: larga
            // o clique e segura o direcional. O empurrão atravessa onde a rota
            // não passa, e três segundos bastam pra sair da quina.
            let sobrou = eu.pos.distance(alvo);
            eu.vagando_para = None;
            // "NÃO ANDOU NADA" é sobrar quase tudo, e isso tem que ser medido
            // CONTRA O RAIO. Eu escrevi 40 quando o raio era 45 e depois baixei
            // o raio pra 25 sem voltar aqui: a condição virou impossível e o
            // desencalhe nunca disparou — zero em dez minutos, com 83 vagares
            // falhando. Número solto que espelha outro número é dívida.
            // Rota não saiu: o rumo pode estar dando no mar ou numa parede.
            // Troca de rumo no próximo salto, além de ir na mão agora.
            eu.vagares_no_rumo = 0;
            if sobrou > LONGE * 0.8 {
                eu.empurrao = Some((alvo - eu.pos).normalize_or_zero());
                eu.empurrao_ate = Some(Instant::now() + Duration::from_secs(3));
                t.registra(ev(nome, eu, "desencalhou", true, format!(
                    "rota não saiu de {:.0},{:.0} — indo na mão", eu.pos.x, eu.pos.y
                )));
                return Ok(());
            }
            t.registra(ev(nome, eu, "vagou", false, format!(
                "não chegou em {:.0},{:.0} (parou a {sobrou:.0}u)",
                alvo.x, alvo.y
            )));
            return Ok(());
        }
        // VAGAR É CAÇAR, E CAÇAR É IR EMBORA DA CIDADE.
        //
        // Direções sorteadas a cada salto davam uma caminhada aleatória: em
        // quinze minutos os bots andaram 318 vezes e não saíram de um quadrado
        // de 20 unidades em volta do porto — onde NÃO NASCE BICHO, porque o
        // mundo afasta as hordas da cidade de propósito
        // (`MOB_LONGE_DA_CIDADE_UN`). Resultado medido: 343 ataques por minuto
        // enquanto havia missão, e ZERO nos quinze minutos seguintes ao
        // momento em que todas as missões da ilha se esgotaram.
        //
        // Agora ele escolhe um rumo e ANDA NELE por vários saltos. Uma
        // caminhada com rumo sai do lugar; uma caminhada aleatória, não — e é
        // sair do lugar que encontra bicho.
        const SALTOS_POR_RUMO: u32 = 8;
        if eu.vagares_no_rumo == 0 || eu.rumo_do_vagar.is_none() {
            eu.vagou = eu.vagou.wrapping_add(1);
            // O `%` sobre o contador é um sorteio bom o bastante: não precisa
            // de aleatoriedade boa, precisa de rumos diferentes a cada vez.
            let volta = (eu.vagou as f32) * 2.399_963_2;
            eu.rumo_do_vagar = Some(glam::Vec2::new(volta.cos(), volta.sin()));
            eu.vagares_no_rumo = SALTOS_POR_RUMO;
        }
        eu.vagares_no_rumo -= 1;
        let rumo = eu.rumo_do_vagar.unwrap_or(glam::Vec2::X);
        // VINTE E CINCO por salto, e não quarenta e cinco. Ponto longe demais
        // cai no mar ou do outro lado de uma parede, e aí o servidor não traça
        // rota nenhuma. Oito saltos de 25 no mesmo rumo dão 200 unidades de
        // caminhada, que é o que tira o bot da orla da cidade.
        let alvo = eu.pos + rumo * LONGE;
        eu.vagando_para = Some((alvo, Instant::now()));
        ws.send(envia(&ClientMessage::MoverPara { x: alvo.x, z: alvo.y })?).await?;
        return Ok(());
    }
    // Achou alvo ou missão: a volta pode esperar.
    eu.vagando_para = None;
    t.registra(ev(nome, eu, "sem_o_que_fazer", false, format!(
        "{} quest(s), destino={:?}",
        eu.quests.len(),
        eu.destino
            .map(|d| (d.quest, d.tipo, d.raio, d.npc.unwrap_or(0))),
    )));
    Ok(())
}


#[cfg(test)]
mod testes_da_variedade {
    use super::semente_do_nome;

    /// DOZE BOTS, DOZE CARAS — e a mesma cara a cada reinício.
    ///
    /// O dono: "eles tão todos com a mesma skin?" e "varia as skins / e as
    /// classes". Estavam: `Default::default()` e sempre a primeira arma.
    ///
    /// As duas propriedades importam e brigam entre si. VARIEDADE: doze
    /// gêmeos na praça derrubam justamente o que os bots existem pra fazer.
    /// ESTABILIDADE: o serviço reinicia a cada 12 h, e um bot que muda de
    /// cara a cada reinício é pior que um bot repetido — ninguém reconhece
    /// ninguém.
    #[test]
    fn cada_bot_tem_cara_propria_e_estavel() {
        let nomes: Vec<String> = (0..12).map(|i| format!("prodbot_{i}")).collect();
        let cara = |n: &str| {
            let s = semente_do_nome(n);
            (
                s % shared::aparencia::ROSTOS as u64,
                (s / 3) % shared::aparencia::CABELOS as u64,
                (s / 7) % shared::aparencia::CORES_DE_CABELO.len() as u64,
                (s / 13) % shared::aparencia::TONS_DE_PELE.len() as u64,
                (s / 17) % 4,
            )
        };
        // ESTÁVEL: duas leituras do mesmo nome dão o mesmo.
        for n in &nomes {
            assert_eq!(cara(n), cara(n), "{n} mudou de cara entre duas leituras");
        }
        // VARIADO: com 3×3×6×4×4 = 864 combinações, doze iguais seriam
        // suspeitos. Exijo pelo menos oito combinações distintas entre doze.
        let mut vistas: Vec<_> = nomes.iter().map(|n| cara(n)).collect();
        vistas.sort();
        vistas.dedup();
        assert!(
            vistas.len() >= 8,
            "só {} aparências distintas entre 12 bots: {vistas:?}",
            vistas.len()
        );
        // E a CLASSE (a arma) também tem que variar — era sempre a primeira.
        let mut armas: Vec<u64> = nomes.iter().map(|n| cara(n).4).collect();
        armas.sort();
        armas.dedup();
        assert!(
            armas.len() >= 3,
            "só {} classes entre 12 bots: {armas:?}",
            armas.len()
        );
    }
}

#[cfg(test)]
mod testes_do_empurrao {
    use super::*;

    /// A REDE DISPARA quando a distância para de encurtar, e só então.
    ///
    /// Os dois lados importam. Sem o primeiro, o bot empurra uma parede pra
    /// sempre — foi o que plantou quatro bots a 6 u de um nó de coleta por
    /// horas, sem um evento na trilha. Sem o segundo, ela dispararia no meio
    /// de uma caminhada normal e o bot largaria destinos que ia alcançar.
    #[test]
    fn so_desiste_de_quem_nao_encurta() {
        let alvo = glam::Vec2::new(10.0, 0.0);
        // PARADO a 6 u: a quinta chamada desiste.
        let eu = glam::Vec2::new(4.0, 0.0);
        let (mut parado, mut ultima) = (0u32, 0.0f32);
        let mut desistiu = None;
        for n in 1..=8 {
            let (vale, p, u) = insiste(eu, alvo, parado, ultima);
            parado = p;
            ultima = u;
            if !vale {
                desistiu = Some(n);
                break;
            }
        }
        assert_eq!(desistiu, Some(6), "parado e a rede não disparou na hora certa");

        // ANDANDO: nunca desiste, por mais chamadas que passem.
        let (mut parado, mut ultima) = (0u32, 0.0f32);
        // Passos de meia unidade, SEM passar do alvo: depois de ultrapassar,
        // a distância volta a crescer e desistir ali estaria certo.
        for n in 0..19 {
            let eu = glam::Vec2::new(n as f32 * 0.5, 0.0);
            let (vale, p, u) = insiste(eu, alvo, parado, ultima);
            assert!(vale, "desistiu de quem estava andando (passo {n})");
            parado = p;
            ultima = u;
        }
    }

    /// ZERAR O ESTADO DE UM NÃO PODE ZERAR O DO OUTRO.
    ///
    /// Este é o defeito que custou caro, e ele não aparece olhando uma função
    /// só. Quem empurra até o DESTINO põe `dist` em `f32::MAX` toda vez que o
    /// corpo está "perto" — e perto de um destino de coleta é qualquer lugar
    /// dentro do raio da missão, o nó inclusive. Com um contador compartilhado
    /// a sequência era: zera, empurra até o nó (`dist < MAX` sempre), zera de
    /// novo. A rede nunca disparava.
    ///
    /// O teste refaz isso: intercala uma "chegada ao destino" (que reseta)
    /// entre as tentativas do nó, com estados SEPARADOS, e exige que o nó
    /// ainda assim desista.
    #[test]
    fn o_contador_do_no_nao_e_zerado_pelo_destino() {
        let no = glam::Vec2::new(10.0, 0.0);
        let eu = glam::Vec2::new(4.0, 0.0);
        let (mut no_parado, mut no_dist) = (0u32, 0.0f32);
        // O estado do destino, que zera a cada volta como no jogo.
        let (mut d_parado, mut d_dist) = (0u32, 0.0f32);
        let mut desistiu = false;
        for _ in 0..8 {
            d_parado = 0;
            d_dist = f32::MAX;
            let (vale, p, u) = insiste(eu, no, no_parado, no_dist);
            no_parado = p;
            no_dist = u;
            if !vale {
                desistiu = true;
                break;
            }
        }
        let _ = (d_parado, d_dist);
        assert!(
            desistiu,
            "o nó nunca desistiu: o contador dele está preso no do destino"
        );
    }
}

#[cfg(test)]
mod testes_da_esquiva {
    use super::*;
    use shared::bosses::Forma;

    /// A SAÍDA ESTÁ MESMO FORA — em toda forma.
    ///
    /// É o erro que este tipo de código comete calado: devolver um ponto que
    /// ainda está dentro da marcação, só que num lugar diferente. O bot
    /// "desviaria", tomaria o golpe do mesmo jeito, e a trilha registraria
    /// uma esquiva bem-sucedida.
    #[test]
    fn a_saida_fica_fora_da_marcacao() {
        let centro = glam::Vec2::new(10.0, -4.0);
        let dir = glam::Vec2::new(1.0, 1.0).normalize();
        let formas = [
            Forma::Circulo { raio: 8.0 },
            Forma::Cone {
                raio: 14.0,
                abertura: 0.9,
            },
            Forma::Linha {
                comprimento: 20.0,
                largura: 6.0,
            },
            Forma::Anel {
                interno: 4.0,
                externo: 10.0,
            },
        ];
        for f in formas {
            // Um ponto claramente DENTRO de cada uma.
            let dentro = match f {
                Forma::Anel { interno, externo } => {
                    centro + dir * ((interno + externo) * 0.5)
                }
                Forma::Linha { comprimento, .. } => centro + dir * (comprimento * 0.4),
                Forma::Cone { raio, .. } => centro + dir * (raio * 0.5),
                Forma::Circulo { .. } => centro,
            };
            assert!(f.contem(centro, dir, dentro), "o ponto de teste {f:?} não está dentro");
            let saida = saida_do_telegrafico(&f, centro, dir, dentro)
                .unwrap_or_else(|| panic!("{f:?}: não achou saída nenhuma"));
            assert!(
                !f.contem(centro, dir, saida),
                "{f:?}: a 'saída' {saida:?} continua dentro da marcação"
            );
        }
    }

    /// QUEM JÁ ESTÁ FORA NÃO CORRE.
    ///
    /// Sem isto o bot largaria a luta a cada telegráfico do chefe, inclusive
    /// os que nunca iam pegá-lo — e um chefe que telegrafa sem parar viraria
    /// um bot que só foge.
    #[test]
    fn quem_esta_fora_nao_desvia() {
        let f = Forma::Circulo { raio: 6.0 };
        let centro = glam::Vec2::ZERO;
        let fora = glam::Vec2::new(30.0, 0.0);
        assert!(saida_do_telegrafico(&f, centro, glam::Vec2::X, fora).is_none());
    }

    /// A FUGA É CURTA.
    ///
    /// Sair correndo trinta unidades tira o bot da luta: ele perde o alvo,
    /// volta andando e o chefe recupera vida. Meio ombro pra fora já escapa
    /// (`Forma::contem` mede o centro do corpo), então a saída certa é a mais
    /// PERTO que esteja fora, não a mais segura.
    #[test]
    fn a_fuga_e_a_mais_curta_possivel() {
        let f = Forma::Circulo { raio: 6.0 };
        let centro = glam::Vec2::ZERO;
        // Colado na borda de dentro: dois passos resolvem.
        let quase_fora = glam::Vec2::new(5.5, 0.0);
        let saida = saida_do_telegrafico(&f, centro, glam::Vec2::X, quase_fora).expect("saída");
        assert!(
            quase_fora.distance(saida) <= 6.0,
            "fugiu {:.1}u de uma borda a meia unidade",
            quase_fora.distance(saida)
        );
    }
}
