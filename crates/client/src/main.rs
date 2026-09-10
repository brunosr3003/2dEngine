//! Cliente 3D do Tempest.
//!
//! O servidor continua sendo a autoridade unica: aqui nao ha nenhuma regra de
//! jogo, so' apresentacao e captura de input.
//!
//! O protocolo vem do crate `shared`, entao `PROTOCOL_VERSION` e' literalmente
//! a mesma constante que o servidor compila — a versao nao tem como divergir.
//! Era esse o bug que tirou a producao do ar: web em 63, cliente em 66,
//! `Protocol.cs` com 1175 linhas espelhadas a mao.

mod api;
mod hud;
mod map;
mod net;
mod render3d;
mod terreno;
mod vegetacao;
mod ui;
mod vox;
mod world;

use std::sync::mpsc::Receiver;

use macroquad::prelude::*;
use shared::protocol::{ClientMessage, InputFrame, ServerMessage};
use shared::VisualConfig;

use api::Canal;
use map::Map;
use net::{Net, NetEvent};
use vox::VoxCache;
use world::World;

/// O servidor tica a 30Hz; mandar input mais rapido que isso so' gasta banda.
const INPUT_HZ: f64 = 30.0;

/// Relogio local em milissegundos. O servidor devolve este mesmo numero no
/// `Pong`, entao a subtracao da' o RTT sem depender de relogios sincronizados
/// entre as duas maquinas — que nunca estao.
fn agora_ms() -> u64 {
    (get_time() * 1000.0) as u64
}

enum Tela {
    /// Escolha de SERVIDOR e depois de canal. Sao coisas diferentes: servidor
    /// tem banco proprio (personagem nao atravessa), canal e' so' instancia do
    /// mapa dentro dele. Ver docs/SERVIDORES_E_CANAIS.md.
    Servidores,
    Login,
    Conectando,
    Personagens,
    /// Canal de instancia unica lotado. Nao e' recusa: e' vez na fila.
    Fila { posicao: u32, total: u32 },
    Jogando,
    Erro(String),
}

struct Jogo {
    tela: Tela,
    // ── selecao de servidor ──
    canais: Vec<Canal>,
    busca: Option<Receiver<Result<Vec<Canal>, String>>>,
    /// Rolagem da lista de canais. Um realm cheio tem dezenas de canais e o
    /// painel tem altura fixa — sem isto a lista vaza pra fora e os ultimos
    /// canais ficam inclicaveis atras dos botoes.
    rolagem: f32,
    realm: Option<String>,
    host: Option<String>,
    // ── login ──
    usuario: String,
    senha: String,
    foco_senha: bool,
    // ── personagens ──
    personagens: Vec<shared::protocol::CharacterListEntry>,
    armas: Vec<u16>,
    nome_novo: String,
    selecionado: usize,
    /// Personagem em jogo, pra reentrar sozinho depois de trocar de zona.
    personagem_atual: Option<String>,
    // ── mundo ──
    net: Option<Net>,
    /// Terreno voxel gerado da semente. `None` nas zonas antigas de tile —
    /// enquanto as duas convivem, quem manda e' o nome da zona.
    terreno: Option<terreno::Terreno>,
    vox: VoxCache,
    map: Option<Map>,
    world: World,
    alvo: Option<shared::EntityId>,
    input_seq: u32,
    ultimo_input: f64,
    tick: u32,
    /// Medicao de rede mostrada no HUD. Latencia e banda sao as duas contas
    /// que dizem se o jogo esta jogavel no celular do jogador, e nenhuma das
    /// duas da' pra estimar de dentro do loop de render.
    /// Angulo da camera em torno do alvo. Vive so' no cliente: o servidor
    /// recebe direcao em espaco de MUNDO, entao girar a camera nao concede
    /// confianca nenhuma nova.
    /// Pedacos efetivamente desenhados no ultimo quadro (depois do corte de
    /// cone). No HUD ao lado dos vivos: a diferenca entre os dois e' o que o
    /// corte esta' economizando.
    pedacos_desenhados: usize,
    cam_yaw: f32,
    /// Fator de distancia da camera.
    cam_zoom: f32,
    /// Arrasto de rotacao em andamento: posicao do mouse no quadro anterior.
    arrasto: Option<Vec2>,
    rede: hud::Rede,
    ultimo_ping: f64,
    /// Marca da ultima janela de banda: instante e total de bytes.
    banda_marca: (f64, u64),
    info: hud::Info,
    chat: Vec<String>,
}

#[macroquad::main(window_conf)]
async fn main() {
    // Os modelos entram uma vez, no boot. O desenho e' sincrono, entao nada
    // pode ficar carregando no meio do quadro.
    let mut vox = VoxCache::default();
    for nome in ["player", "lobo", "lobo_pequeno"] {
        vox.load(nome, render3d::VOXEL).await;
    }

    let mut jogo = Jogo {
        tela: Tela::Servidores,
        canais: Vec::new(),
        busca: Some(api::buscar_canais()),
        rolagem: 0.0,
        realm: None,
        host: std::env::var("MMO_HOST").ok(),
        usuario: std::env::var("MMO_USER").unwrap_or_default(),
        senha: std::env::var("MMO_PASS").unwrap_or_default(),
        foco_senha: false,
        personagens: Vec::new(),
        armas: Vec::new(),
        nome_novo: String::new(),
        selecionado: 0,
        personagem_atual: std::env::var("MMO_CHAR").ok().filter(|v| !v.is_empty()),
        net: None,
        terreno: None,
        vox,
        map: None,
        world: World::default(),
        alvo: None,
        input_seq: 0,
        ultimo_input: 0.0,
        tick: 0,
        pedacos_desenhados: 0,
        cam_yaw: 0.0,
        cam_zoom: 1.0,
        arrasto: None,
        rede: hud::Rede::default(),
        ultimo_ping: 0.0,
        banda_marca: (0.0, 0),
        info: hud::Info::default(),
        chat: Vec::new(),
    };
    // `MMO_HOST` explicito pula a escolha — e' o caminho do run-client.sh e dos
    // testes de carga.
    if jogo.host.is_some() {
        jogo.conectar();
    }

    loop {
        jogo.passo();
        jogo.desenhar();
        next_frame().await;
    }
}

fn window_conf() -> Conf {
    let (w, h) = tamanho_janela();
    // Wayland NATIVO quando existir. Sob Xwayland o Hyprland nao reaplica o
    // layout depois do split. Ver docs/DESENVOLVIMENTO_LADO_A_LADO.md.
    let mut conf = Conf {
        window_title: "Tempest".to_owned(),
        window_width: w,
        window_height: h,
        ..Default::default()
    };
    conf.platform.linux_backend = miniquad::conf::LinuxBackend::WaylandWithX11Fallback;
    // O default do miniquad desenha decoracao do lado do cliente via libdecor;
    // o Hyprland ja decora pelo hyprbars.
    conf.platform.wayland_decorations = miniquad::conf::WaylandDecorations::ServerOnly;
    // Sem isto a janela se anuncia como "miniquad-application", o nome
    // generico do framework.
    conf.platform.linux_wm_class = "tempest";
    conf
}

fn tamanho_janela() -> (i32, i32) {
    std::env::var("MMO_WIN")
        .ok()
        .and_then(|s| {
            let (a, b) = s.split_once('x')?;
            Some((a.trim().parse().ok()?, b.trim().parse().ok()?))
        })
        .unwrap_or((940, 980))
}

impl Jogo {
    // ─────────────────────────────── passo ───────────────────────────────
    fn passo(&mut self) {
        self.receber_lista();
        self.pump_rede();
        if matches!(self.tela, Tela::Jogando) {
            {
                let terreno = self.terreno.as_ref();
                self.world.tick(get_frame_time(), &|x, z| {
                    terreno.map_or(0.0, |t| t.altura_apoio(x, z, shared::ENTITY_RADIUS))
                });
            }
            self.atualizar_alvo();
            self.camera_controles();
            self.enviar_input();
            self.medir_rede();
            if let Some(t) = &mut self.terreno {
                let centro = self.world.self_pos().unwrap_or(Vec2::ZERO);
                // Raio 4 cobre 128 unidades — mais que a camera alcanca. O
                // orcamento de 3 por quadro existe pra o mundo aparecer em
                // duas piscadas em vez de travar meio segundo.
                t.atualiza(centro, 5, 4);
            }
        }
    }

    fn receber_lista(&mut self) {
        let Some(rx) = &self.busca else { return };
        match rx.try_recv() {
            Ok(Ok(canais)) => {
                self.canais = canais;
                self.busca = None;
            }
            Ok(Err(e)) => {
                self.canais.clear();
                self.chat.push(format!("lista de servidores: {e}"));
                self.busca = None;
            }
            Err(std::sync::mpsc::TryRecvError::Empty) => {}
            Err(_) => self.busca = None,
        }
    }

    fn conectar(&mut self) {
        let Some(host) = self.host.clone() else { return };
        self.world = World::default();
        self.map = None;
        self.terreno = None;
        self.alvo = None;
        self.rede = hud::Rede::default();
        self.banda_marca = (get_time(), 0);
        self.ultimo_ping = 0.0;
        self.net = Some(Net::connect(format!("ws://{host}")));
        self.tela = Tela::Conectando;
    }

    fn envia(&self, msg: ClientMessage) {
        if let Some(n) = &self.net {
            n.send(msg);
        }
    }

    fn pump_rede(&mut self) {
        let eventos: Vec<NetEvent> = match &self.net {
            Some(n) => n.poll(),
            None => return,
        };
        for ev in eventos {
            match ev {
                NetEvent::Connected => self.envia(ClientMessage::Handshake {
                    protocol_version: shared::PROTOCOL_VERSION,
                    client_version: env!("CARGO_PKG_VERSION").to_string(),
                }),
                NetEvent::Disconnected(por_que) => self.tela = Tela::Erro(por_que),
                NetEvent::Message(msg) => self.on_message(*msg),
            }
        }
    }

    fn on_message(&mut self, msg: ServerMessage) {
        match msg {
            ServerMessage::HandshakeAck { .. } => {
                self.envia(ClientMessage::Login {
                    username: self.usuario.clone(),
                    password: self.senha.clone(),
                });
            }
            ServerMessage::CharacterList { chars, available_weapons } => {
                self.personagens = chars;
                self.armas = available_weapons;
                self.selecionado = 0;
                // `MMO_CHAR` pula a tela: e' o caminho do teste automatizado,
                // que nao tem quem clique.
                // Voltando de uma troca de zona: entra direto no mesmo
                // personagem, sem passar pela tela de novo.
                if let Some(nome) = self.personagem_atual.clone() {
                    if self.personagens.iter().any(|p| p.name == nome) {
                        self.envia(ClientMessage::SelectCharacter { name: nome });
                        self.tela = Tela::Conectando;
                        return;
                    }
                }
                match std::env::var("MMO_CHAR").ok().filter(|v| !v.is_empty()) {
                    Some(nome) if self.personagens.iter().any(|p| p.name == nome) => {
                        self.envia(ClientMessage::SelectCharacter { name: nome });
                        self.tela = Tela::Conectando;
                    }
                    _ => self.tela = Tela::Personagens,
                }
            }
            ServerMessage::CharacterCreationFailed { reason } => {
                self.chat.push(format!("criacao falhou: {reason}"));
                self.tela = Tela::Personagens;
            }
            ServerMessage::LoginDenied { reason } => {
                self.tela = Tela::Erro(format!("login negado: {reason}"));
            }
            ServerMessage::Pong { client_time_ms, .. } => {
                let rtt = agora_ms().saturating_sub(client_time_ms) as f32;
                // Media movel: o numero cru pula demais pra ser lido de
                // relance, e o que interessa e' a tendencia.
                self.rede.ms = if self.rede.ms <= 0.0 {
                    rtt
                } else {
                    self.rede.ms * 0.7 + rtt * 0.3
                };
            }
            ServerMessage::Kick { reason } => self.tela = Tela::Erro(format!("kick: {reason}")),
            ServerMessage::FilaDeEntrada { posicao, total } => {
                self.tela = Tela::Fila { posicao, total };
            }
            ServerMessage::LoginOk { .. } => self.tela = Tela::Jogando,
            ServerMessage::InfoCanal { realm, canal, zona, jogadores, capacidade } => {
                self.info = hud::Info { realm, canal, zona, jogadores, capacidade };
            }
            ServerMessage::TrocarZona { zona, host } => {
                // Zona e' outro PROCESSO: reconecta. O personagem e' o mesmo
                // (banco compartilhado no realm), entao a sessao refaz o
                // caminho de login sozinha e o jogador so' ve uma tela de
                // carregamento.
                self.chat.push(format!("indo pra {zona}"));
                self.personagem_atual = self
                    .personagens
                    .get(self.selecionado)
                    .map(|p| p.name.clone())
                    .or_else(|| self.personagem_atual.clone());
                self.host = Some(host);
                self.conectar();
            }
            ServerMessage::MapChange { map_name, width, height, tiles, .. } => {
                // Ilha do arquipelago: o terreno nasce da SEMENTE, e nem o
                // arquivo de tiles nem um byte de rede entram nisso.
                self.terreno = shared::terreno::def_da_zona(&map_name).map(terreno::Terreno::novo);
                if self.terreno.is_some() {
                    self.map = None;
                    println!("[terreno] {map_name}: gerado da semente, sem arquivo");
                } else {
                    let mut m = Map::new(map_name, width, height, tiles);
                    if m.is_empty() {
                        m.fill_from_disk();
                    }
                    self.map = Some(m);
                }
            }
            ServerMessage::Snapshot { snapshot } => {
                self.tick = snapshot.tick;
                self.world
                    .apply(snapshot.entered, snapshot.states, &snapshot.removed);
            }
            ServerMessage::Chat { from, text } => {
                self.chat.push(format!("{from}: {text}"));
                if self.chat.len() > 8 {
                    self.chat.remove(0);
                }
            }
            // As demais (inventario, skills, loja, quests) ainda nao tem UI;
            // ignorar e' seguro porque nada aqui e' autoritativo.
            _ => {}
        }
    }

    // ─────────────────────────────── jogo ────────────────────────────────
    /// Clique esquerdo escolhe alvo; direito ou Esc limpa. So' avisa o
    /// servidor quando o alvo REALMENTE muda.
    fn atualizar_alvo(&mut self) {
        let antes = self.alvo;
        if is_key_pressed(KeyCode::Escape) {
            self.alvo = None;
        }
        if is_mouse_button_pressed(MouseButton::Left) {
            let centro = self.world.self_pos().unwrap_or(Vec2::ZERO);
            // Empresta so' o CAMPO `terreno`, nao `self` inteiro: o alvo e a
            // rota sao escritos logo abaixo.
            let (mx, my) = mouse_position();
            let escolhido = {
                let terreno = self.terreno.as_ref();
                let f = |x: f32, z: f32| terreno.map_or(0.0, |t| t.altura(x, z));
                let apoio = terreno.map_or(0.0, |t| {
                    t.altura_apoio(centro.x, centro.y, shared::ENTITY_RADIUS)
                });
                let vista =
                    render3d::Vista::nova(centro, self.cam_yaw, self.cam_zoom, apoio, &f);
                let alvo = render3d::pick(&self.world, &vista, vec2(mx, my), 48.0);
                // Clicou no vazio? Entao foi no CHAO.
                let destino = if alvo.is_none() {
                    terreno.and_then(|t| {
                        let (o, d) = vista.raio(vec2(mx, my));
                        t.onde_o_raio_bate(o, d, 260.0)
                    })
                } else {
                    None
                };
                (alvo, destino)
            };
            self.alvo = escolhido.0;
            // O cliente nunca manda rota — so' um ponto que ele ja' poderia
            // alcancar andando.
            if let Some(p) = escolhido.1 {
                self.envia(ClientMessage::MoverPara { x: p.x, z: p.y });
            }

        }
        if let Some(t) = self.alvo {
            if !self.world.ents.contains_key(&t) {
                self.alvo = None;
            }
        }
        if self.alvo != antes {
            self.envia(ClientMessage::SetTarget { target: self.alvo });
        }
    }

    /// Ping de 1 em 1 segundo e janela de banda de 1 segundo.
    ///
    /// A latencia vem do par `Ping`/`Pong` que o servidor ja' respondia — e'
    /// ida e volta de verdade, e nao a diferenca entre relogios, que nunca
    /// batem entre maquinas.
    fn medir_rede(&mut self) {
        let agora = get_time();
        if agora - self.ultimo_ping >= 1.0 {
            self.ultimo_ping = agora;
            self.envia(ClientMessage::Ping { client_time_ms: agora_ms() });
        }

        let Some(net) = &self.net else { return };
        let total = net.bytes_in();
        let (t0, b0) = self.banda_marca;
        let dt = agora - t0;
        if dt >= 1.0 {
            self.rede.kbs = (total.saturating_sub(b0)) as f32 / 1024.0 / dt as f32;
            self.banda_marca = (agora, total);
        }
        self.rede.total_bytes = total;
    }

    /// Sai do mundo e volta pro login, sem fechar o jogo.
    ///
    /// Soltar o `Net` derruba o canal de saida, a thread de rede encerra e
    /// manda o close pro servidor — que aproveita pra salvar o personagem em
    /// vez de esperar o socket morrer de timeout.
    fn sair(&mut self) {
        self.net = None;
        self.world = World::default();
        self.map = None;
        self.terreno = None;
        self.alvo = None;
        self.info = hud::Info::default();
        self.rede = hud::Rede::default();
        self.chat.clear();
        // Esvazia a fila de caracteres do macroquad. Ninguem a consome
        // durante o jogo (WASD e' lido por `is_key_down`), entao tudo que foi
        // teclado no mundo estava esperando — e caia de uma vez no campo de
        // usuario assim que a tela de login aparecia.
        while get_char_pressed().is_some() {}
        // Volta pro login e nao pra escolha de servidor: o canal continua
        // sendo o mesmo, quem mudou de ideia foi a conta.
        self.tela = Tela::Login;
    }

    /// Rotacao e zoom. Botao do MEIO arrasta e Q/E giram; a roda aproxima.
    ///
    /// O botao direito continua sendo do jogo (`SECONDARY`) e o esquerdo mira
    /// — camera em botao de combate e' briga garantida com o alvo.
    fn camera_controles(&mut self) {
        let dt = get_frame_time();
        if is_key_down(KeyCode::Q) {
            self.cam_yaw -= 2.2 * dt;
        }
        if is_key_down(KeyCode::E) {
            self.cam_yaw += 2.2 * dt;
        }
        let (mx, my) = mouse_position();
        if is_mouse_button_down(MouseButton::Middle) {
            let p = vec2(mx, my);
            if let Some(anterior) = self.arrasto {
                self.cam_yaw += (p.x - anterior.x) * 0.008;
            }
            self.arrasto = Some(p);
        } else {
            self.arrasto = None;
        }
        let (_, roda) = mouse_wheel();
        if roda != 0.0 {
            self.cam_zoom = (self.cam_zoom - roda.signum() * 0.12)
                .clamp(render3d::ZOOM_MIN, render3d::ZOOM_MAX);
        }
        // Mantem o angulo em [-pi, pi]: sem isso ele cresce sem limite e a
        // precisao do f32 come a suavidade depois de uns minutos girando.
        if self.cam_yaw > std::f32::consts::PI {
            self.cam_yaw -= std::f32::consts::TAU;
        } else if self.cam_yaw < -std::f32::consts::PI {
            self.cam_yaw += std::f32::consts::TAU;
        }
    }



    fn enviar_input(&mut self) {
        let agora = get_time();
        if agora - self.ultimo_input < 1.0 / INPUT_HZ {
            return;
        }
        self.ultimo_input = agora;

        let mut dir = Vec2::ZERO;
        if is_key_down(KeyCode::W) || is_key_down(KeyCode::Up) { dir.y -= 1.0; }
        if is_key_down(KeyCode::S) || is_key_down(KeyCode::Down) { dir.y += 1.0; }
        if is_key_down(KeyCode::A) || is_key_down(KeyCode::Left) { dir.x -= 1.0; }
        if is_key_down(KeyCode::D) || is_key_down(KeyCode::Right) { dir.x += 1.0; }
        if dir != Vec2::ZERO {
            dir = dir.normalize();
        }
        // "Pra frente" e' longe da camera, nao o norte do mundo. A conta e'
        // aqui; o que sai no fio continua sendo direcao em espaco de mundo.
        dir = render3d::input_para_mundo(dir, self.cam_yaw);
        let mut buttons = 0u32;
        if is_mouse_button_down(MouseButton::Right) {
            buttons |= shared::protocol::buttons::SECONDARY;
        }
        self.input_seq += 1;
        self.envia(ClientMessage::Input {
            input: InputFrame {
                seq: self.input_seq,
                tick: self.tick,
                move_dir: ::glam::Vec2::new(dir.x, dir.y),
                aim: ::glam::Vec2::X,
                buttons,
            },
        });
    }

    // ────────────────────────────── desenho ──────────────────────────────
    fn desenhar(&mut self) {
        match &self.tela {
            Tela::Jogando => self.desenhar_mundo(),
            Tela::Servidores => self.tela_servidores(),
            Tela::Login => self.tela_login(),
            Tela::Personagens => self.tela_personagens(),
            Tela::Conectando => {
                ui::fundo();
                let r = ui::painel(420.0, 160.0, "");
                ui::texto_centro(r.x + r.w * 0.5, r.y + 50.0, "conectando...", 22, ui::OURO);
            }
            Tela::Fila { posicao, total } => {
                let (p, t) = (*posicao, *total);
                self.tela_fila(p, t);
            }
            Tela::Erro(por_que) => {
                let msg = por_que.clone();
                ui::fundo();
                let r = ui::painel(560.0, 220.0, "não deu");
                ui::erro(r.x + r.w * 0.5, r.y + 50.0, &msg);
                if ui::botao(Rect::new(r.x + r.w * 0.5 - 80.0, r.y + 110.0, 160.0, 40.0), "voltar", true) {
                    self.net = None;
                    self.busca = Some(api::buscar_canais());
                    self.tela = Tela::Servidores;
                }
            }
        }
    }

    fn desenhar_mundo(&mut self) {
        render3d::clear();
        let centro = self.world.self_pos().unwrap_or(Vec2::ZERO);
        // A camera sobe com o chao. Presa em zero, o jogador some dentro do
        // morro assim que o terreno passou a ter 34 unidades de altura.
        let terreno = self.terreno.as_ref();
        let f = |x: f32, z: f32| terreno.map_or(0.0, |t| t.altura(x, z));
        let apoio =
            terreno.map_or(0.0, |t| t.altura_apoio(centro.x, centro.y, shared::ENTITY_RADIUS));
        let vista = render3d::Vista::nova(centro, self.cam_yaw, self.cam_zoom, apoio, &f);
        set_camera(&vista.cam);
        match &self.terreno {
            Some(t) => {
                self.pedacos_desenhados = t.desenha(&vista.cam);
            }
            None => {
                if let Some(m) = &self.map {
                    render3d::draw_ground(m, centro);
                }
            }
        }
        // A MESMA vista da mira: desenho e clique nao tem como divergir
        // porque nao existe a segunda conta.
        render3d::draw_entities(&mut self.world, &self.vox, self.alvo, &vista);
        set_default_camera();
        if hud::draw_hud(
            &self.info,
            &self.rede,
            self.map.as_ref(),
            self.tick,
            self.world.ents.len(),
            self.pedacos_desenhados,
            self.terreno.as_ref().map_or(0, |t| t.pedacos_vivos()),
            centro,
            &self.chat,
        ) {
            self.sair();
        }
    }

    fn tela_servidores(&mut self) {
        ui::fundo();
        let r = ui::painel(560.0, 460.0, "escolha o servidor");
        let cx = r.x + r.w * 0.5;

        if self.busca.is_some() {
            ui::texto_centro(cx, r.y + 60.0, "procurando servidores...", 20, ui::OURO);
            return;
        }
        if self.canais.is_empty() {
            ui::erro(cx, r.y + 50.0, "nenhum servidor no ar");
            ui::texto_centro(cx, r.y + 78.0, "confira o web (/api/channels)", 16, ui::OURO);
            if ui::botao(Rect::new(cx - 80.0, r.y + 110.0, 160.0, 40.0), "procurar de novo", true) {
                self.busca = Some(api::buscar_canais());
            }
            return;
        }

        // Sem realm escolhido: lista os SERVIDORES, com a soma dos canais.
        // Servidor e canal sao decisoes diferentes e nao cabem na mesma lista —
        // trocar de servidor e' trocar de personagem.
        let Some(realm) = self.realm.clone() else {
            let mut realms: Vec<(String, u32, u32)> = Vec::new();
            for c in &self.canais {
                match realms.iter_mut().find(|(n, _, _)| n == c.realm()) {
                    Some(e) => { e.1 += c.jogadores; e.2 += c.capacidade; }
                    None => realms.push((c.realm().to_string(), c.jogadores, c.capacidade)),
                }
            }
            let mut y = r.y + 10.0;
            let mut escolhido = None;
            for (nome, jog, cap) in &realms {
                let linha = Rect::new(r.x, y, r.w, 46.0);
                if ui::linha(linha, nome, &format!("{jog} online · {} canais",
                    self.canais.iter().filter(|c| c.realm() == nome).count()), false)
                {
                    escolhido = Some(nome.clone());
                }
                ui::barra(Rect::new(r.x + 12.0, y + 34.0, r.w - 24.0, 5.0),
                    if *cap > 0 { *jog as f32 / *cap as f32 } else { 0.0 });
                y += 54.0;
            }
            if let Some(n) = escolhido {
                self.realm = Some(n);
                self.rolagem = 0.0;
            }
            return;
        };

        // Realm escolhido: os canais dele, agrupados por ZONA.
        //
        // Zona e canal sao coisas diferentes e o jogador precisa ver as duas:
        // a zona diz ONDE ele vai cair, o canal diz COM QUEM. Misturar numa
        // lista so' esconde que "cidade" tem uma instancia unica e "campo"
        // tem varias.
        ui::texto(r.x, r.y - 4.0, &format!("servidor {realm}"), 18, ui::OURO_CLARO);
        let canais: Vec<Canal> = self
            .canais
            .iter()
            .filter(|c| c.realm() == realm)
            .cloned()
            .collect();
        let mut zonas: Vec<String> = Vec::new();
        for c in &canais {
            if !zonas.contains(&c.zona) {
                zonas.push(c.zona.clone());
            }
        }

        // Area util da lista: acima dos botoes de baixo. Tudo que cai fora
        // dela nao e' desenhado NEM clicavel — desenhar fora do painel deixava
        // canal visivel atras do botao "atualizar", que roubava o clique.
        let topo = r.y + 18.0;
        let base = r.y + r.h - 52.0;
        let altura_total: f32 = zonas
            .iter()
            .map(|z| 30.0 + canais.iter().filter(|c| &c.zona == z).count() as f32 * 46.0)
            .sum();
        let rolagem_max = (altura_total - (base - topo)).max(0.0);
        let (_, roda) = mouse_wheel();
        if roda != 0.0 {
            self.rolagem = (self.rolagem - roda * 40.0).clamp(0.0, rolagem_max);
        }
        self.rolagem = self.rolagem.min(rolagem_max);

        let mut y = topo + 4.0 - self.rolagem;
        let mut escolhido: Option<String> = None;
        for zona in &zonas {
            let da_zona: Vec<&Canal> = canais.iter().filter(|c| &c.zona == zona).collect();
            let online: u32 = da_zona.iter().map(|c| c.jogadores).sum();
            if y >= topo && y + 20.0 <= base {
                ui::texto(r.x + 2.0, y + 14.0, zona, 17, ui::OURO);
                let dir = format!("{online} online");
                let d = measure_text(&dir, None, 14, 1.0);
                ui::texto(r.x + r.w - d.width - 2.0, y + 14.0, &dir, 14,
                    Color::new(0.55, 0.55, 0.58, 1.0));
            }
            y += 24.0;

            for c in da_zona {
                if y >= topo && y + 40.0 <= base {
                    let linha = Rect::new(r.x + 10.0, y, r.w - 10.0, 40.0);
                    // A zona diz se e' instancia unica; contar canais visiveis
                    // errava sempre que a zona estava vazia — campo com um
                    // canal aberto aparecia como "instância única".
                    let rotulo = if c.unica {
                        "instância única".to_string()
                    } else {
                        format!("canal {}", c.numero())
                    };
                    let direita = if c.cheio {
                        format!("{}/{} · fila", c.jogadores, c.capacidade)
                    } else {
                        format!("{}/{}", c.jogadores, c.capacidade)
                    };
                    if ui::linha(linha, &rotulo, &direita, false) {
                        escolhido = Some(c.host.clone());
                    }
                    ui::barra(Rect::new(linha.x + 12.0, y + 29.0, linha.w - 24.0, 5.0), c.fracao());
                }
                y += 46.0;
            }
            y += 6.0;
        }
        // Diz que ha' mais. Lista cortada sem aviso parece lista completa.
        if rolagem_max > 0.0 {
            let quanto = format!(
                "{} canais · roda do mouse pra rolar",
                canais.len()
            );
            ui::texto_centro(cx, base + 16.0, &quanto, 13, Color::new(0.45, 0.45, 0.48, 1.0));
        }
        if let Some(h) = escolhido {
            self.host = Some(h);
            self.tela = Tela::Login;
        }

        if ui::botao(Rect::new(r.x, r.y + r.h - 44.0, 140.0, 40.0), "< servidores", true) {
            self.realm = None;
            self.rolagem = 0.0;
        }
        if ui::botao(Rect::new(r.x + r.w - 140.0, r.y + r.h - 44.0, 140.0, 40.0), "atualizar", true) {
            self.busca = Some(api::buscar_canais());
        }
    }

    fn tela_login(&mut self) {
        ui::fundo();
        let r = ui::painel(460.0, 320.0, "entrar");
        let cx = r.x + r.w * 0.5;
        if let Some(h) = &self.host {
            ui::texto_centro(cx, r.y + 6.0, h, 15, ui::OURO);
        }

        let cu = Rect::new(r.x, r.y + 46.0, r.w, 42.0);
        let cs = Rect::new(r.x, r.y + 122.0, r.w, 42.0);
        let mut usuario = std::mem::take(&mut self.usuario);
        let mut senha = std::mem::take(&mut self.senha);
        let clicou_u = ui::campo(cu, "usuário", &mut usuario, !self.foco_senha, false);
        let clicou_s = ui::campo(cs, "senha", &mut senha, self.foco_senha, true);
        self.usuario = usuario;
        self.senha = senha;
        if clicou_u { self.foco_senha = false; }
        if clicou_s { self.foco_senha = true; }
        // Tab e Enter no usuario passam pra senha — teclado antes de mouse.
        if is_key_pressed(KeyCode::Tab) {
            self.foco_senha = !self.foco_senha;
        }

        let pode = !self.usuario.is_empty() && !self.senha.is_empty();
        let entrar = ui::botao(Rect::new(r.x, r.y + 196.0, r.w, 44.0), "entrar", pode)
            || (pode && self.foco_senha && is_key_pressed(KeyCode::Enter));
        if entrar {
            self.conectar();
        }
        if ui::botao(Rect::new(r.x, r.y + r.h - 40.0, 140.0, 36.0), "< voltar", true) {
            self.tela = Tela::Servidores;
        }
    }

    fn tela_personagens(&mut self) {
        ui::fundo();
        let r = ui::painel(520.0, 420.0, "personagens");
        let mut acao: Option<ClientMessage> = None;

        let mut y = r.y + 10.0;
        for (i, p) in self.personagens.iter().enumerate() {
            let linha = Rect::new(r.x, y, r.w, 44.0);
            if ui::linha(linha, &p.name, &format!("nível {}", p.level), i == self.selecionado) {
                self.selecionado = i;
            }
            y += 50.0;
        }
        if self.personagens.is_empty() {
            ui::texto(r.x, r.y + 30.0, "nenhum personagem ainda", 18, ui::OURO);
            let campo = Rect::new(r.x, r.y + 70.0, r.w, 40.0);
            let mut nome = std::mem::take(&mut self.nome_novo);
            ui::campo(campo, "nome do personagem", &mut nome, true, false);
            self.nome_novo = nome;
            let pode = self.nome_novo.chars().count() >= 3;
            if ui::botao(Rect::new(r.x, r.y + 130.0, r.w, 44.0), "criar", pode) {
                acao = Some(ClientMessage::CreateCharacter {
                    name: self.nome_novo.clone(),
                    visual: VisualConfig::default(),
                    starting_weapon: self.armas.first().copied().unwrap_or(0),
                    faction: Default::default(),
                });
            }
        } else if ui::botao(Rect::new(r.x, r.y + r.h - 50.0, r.w, 44.0), "jogar", true) {
            acao = Some(ClientMessage::SelectCharacter {
                name: self.personagens[self.selecionado].name.clone(),
            });
        }
        if let Some(m) = acao {
            self.envia(m);
        }
    }

    fn tela_fila(&mut self, posicao: u32, total: u32) {
        ui::fundo();
        let r = ui::painel(460.0, 240.0, "fila de entrada");
        let cx = r.x + r.w * 0.5;
        ui::texto_centro(cx, r.y + 60.0, &format!("{posicao}º de {total}"), 40, ui::OURO_CLARO);
        ui::texto_centro(cx, r.y + 96.0, "este canal está cheio", 17, ui::OURO);
        // A barra anda pra tras conforme a fila anda: sem numero mexendo, a
        // espera e' indistinguivel de travamento.
        let frac = if total > 0 { 1.0 - (posicao as f32 / total as f32) } else { 0.0 };
        ui::barra(Rect::new(r.x + 40.0, r.y + 120.0, r.w - 80.0, 10.0), frac);
        if ui::botao(Rect::new(cx - 90.0, r.y + 155.0, 180.0, 40.0), "sair da fila", true) {
            self.net = None;
            self.busca = Some(api::buscar_canais());
            self.tela = Tela::Servidores;
        }
    }
}
