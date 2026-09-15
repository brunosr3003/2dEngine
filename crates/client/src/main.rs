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
mod login_google;
mod nativo;
mod teclado_virtual;
mod bicho;
mod bolsa;
mod efeitos;
mod entrada;
mod hud;
mod hud_layout;
mod habilidades;
mod habilidades_input;
mod hud_estilo;
mod icones;
mod icones_ui;
mod lojas;
mod mercado_ui;
mod menu;
mod auto_combate;
mod loja;
mod craft_ui;
mod forja_ui;
mod ganhos;
mod missoes;
mod auto_missao;
mod auto_coleta;
mod dialogo;
mod construcoes;
mod mapa;
mod rastro;
mod telegrafico;
mod chefe_anim;
mod toque;
mod gesto_camera;
mod camera_suave;
mod joystick;
mod barra;
mod preferencias;
mod config_barra;
mod config_coleta;
mod config_interface;
mod economia;
mod onde_obter;
mod coleta_hud;
mod lascas;
mod morte;
mod corrida;
mod ir_para;
mod menu_missoes;
mod diarias;
mod personagens;
mod habilidades_vfx;
mod previa_skills;
mod map;
mod net;
mod render3d;
mod rig;
mod terreno;
mod agua;
mod vegetacao;
mod ui;
mod vox;
mod world;

use std::sync::mpsc::Receiver;

use macroquad::prelude::*;
use shared::protocol::{ClientMessage, InputFrame, ServerMessage};

use api::Canal;
use map::Map;
use net::{Net, NetEvent};
use vox::VoxCache;
use macroquad::material::{gl_use_default_material, gl_use_material, Material};
use world::World;

/// O servidor tica a 30Hz; mandar input mais rapido que isso so' gasta banda.
const INPUT_HZ: f64 = 30.0;

/// Raio em que F e Tab procuram inimigo.
const RAIO_DE_MIRA: f32 = 24.0;

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
    /// Algum campo de login foi tocado: o teclado da tela fica aberto.
    campo_login_ativo: bool,
    teclado_virtual: teclado_virtual::TecladoVirtual,
    /// "Entrar com Google" (docs/LOGIN_GOOGLE.md).
    google: login_google::LoginGoogle,
    /// Sessao do login com Google: vai no lugar da senha em toda conexao
    /// (inclusive na troca de zona), ate' sair.
    token_login: Option<String>,
    // ── personagens ──
    personagens: Vec<shared::protocol::CharacterListEntry>,
    armas: Vec<u16>,
    selecao_personagem: personagens::Personagens,
    selecionado: usize,
    /// Personagem em jogo, pra reentrar sozinho depois de trocar de zona.
    personagem_atual: Option<String>,
    // ── mundo ──
    net: Option<Net>,
    /// Terreno voxel gerado da semente. `None` nas zonas antigas de tile —
    /// enquanto as duas convivem, quem manda e' o nome da zona.
    terreno: Option<terreno::Terreno>,
    vox: VoxCache,
    /// Material com descarte de face de costas. Ver `render3d::material_solido`.
    solido: Material,
    map: Option<Map>,
    world: World,
    alvo: Option<shared::EntityId>,
    /// Inventario e equipamento (icone do HUD ou Menu). Ver `bolsa`.
    bolsa: bolsa::Bolsa,
    /// Vida, mana, vigor e experiencia do HUD (`hud::Ficha`).
    ficha: hud::Ficha,
    habilidades: habilidades::Habilidades,
    auto_combate: auto_combate::AutoCombate,
    /// Loja do NPC vendedor aberta, e o NPC clicado de longe (anda e fala).
    loja: loja::Loja,
    /// "+57 Cobre" subindo do personagem: o que entrou na bolsa.
    ganhos: ganhos::Ganhos,
    interacao: loja::Pendente,
    /// Craft e Forja: abrem pelo HUD (e a Forja tambem pelo Ferreiro), nunca
    /// por tecla. API em `craft_ui` / `forja_ui`.
    craft: craft_ui::Craft,
    forja: forja_ui::Forja,
    /// Pocao de Experiencia: bonus ativo ate' (unix secs; 0 = nenhum).
    bonus_xp_ate: i64,
    /// Pocoes de Fortuna e de Sorte ativas ate' (unix secs; 0 = nenhuma).
    bonus_fortuna_ate: i64,
    bonus_sorte_ate: i64,
    /// Barra de itens (C, 8, 9, 0) e o configurador dela.
    barra: barra::Barra,
    /// Quando mandar as preferencias de tela pro servidor.
    prefs: preferencias::Sincronia,
    config_barra: config_barra::ConfigBarra,
    /// Tipos e raio do AUTO COLETA (botao direito no AUTO COLETA).
    config_coleta: config_coleta::ConfigColeta,
    config_interface: config_interface::ConfigInterface,
    /// A barrinha "Coletando · tipo · N s".
    coleta_hud: coleta_hud::BarraDeColeta,
    /// Clique numa pedra/tronco: (coluna, centro, raio) — anda e coleta ao chegar.
    coleta_pendente: Option<(u32, Vec2, f32)>,
    /// O AUTO COLETA estava ligado no quadro anterior (desligou: para no servidor).
    coleta_auto_estava: bool,
    /// Janela do Mestre de Missoes, diario (J) e rastreador.
    missoes: missoes::Missoes,
    /// Clicou numa missao do rastreador: o personagem vai sozinho.
    auto_missao: auto_missao::AutoMissao,
    /// X: fica coletando no melhor spot perto.
    auto_coleta: auto_coleta::AutoColeta,
    /// Toque longo no AUTO COLETA e na barra de itens: o que no PC e' o botao
    /// direito (configuracao), no toque e' segurar.
    toque_coleta: toque::ToqueLongo,
    toque_barra: toque::ToqueLongo,
    /// Falas das missoes (Proximo / Receber / Aceitar).
    dialogo: dialogo::Dialogo,
    /// A rota do servidor, pro tracejado no chao.
    rastro: rastro::Rastro,
    /// Golpes de chefe carregando: a forma no chao.
    telegrafos: telegrafico::Telegrafos,
    /// Tremor de camera do impacto de chefe: (forca, ate quando).
    tremor: (f32, f64),
    /// Tela de morte e "Recuperar XP".
    morte: morte::Morte,
    /// Indo sozinho ha' 1,5 s: corre. `correndo_auto` e' o resultado do quadro.
    corrida: corrida::Corrida,
    correndo_auto: bool,
    /// Ultimo NPC com quem se falou: a oferta de missoes que chega depois e'
    /// dele (a mensagem nao traz a entidade).
    ultimo_npc: Option<shared::EntityId>,
    /// Mapa da ilha (M), minimapa e a viagem por clique neles.
    mapa: mapa::Mapa,
    /// Casas, props e cais da vila, assados numa thread quando a zona muda.
    construcoes: construcoes::Construcoes,
    /// "Ir para" do mapa (zona de bicho, regiao de recurso) e do menu de
    /// missoes (ir ao Mestre).
    ir_para: ir_para::IrPara,
    /// Menu de todas as missoes (rodape do rastreador ou Menu).
    menu_missoes: menu_missoes::MenuMissoes,
    /// Painel das diarias: icone no topo e Menu, separado das outras missoes.
    diarias: diarias::Diarias,
    /// O Menu Principal (botao ≡ do HUD). Nenhum painel abre por tecla.
    menu: menu::Menu,
    /// Menu → Comercio: os vendedores da ilha com "Ir".
    lojas: lojas::Lojas,
    /// Menu → Comércio → Mercado (docs/MERCADO.md).
    mercado: mercado_ui::Mercado,
    /// O painel aberto veio do Menu: Esc volta pra ele em vez de pro jogo.
    voltar_ao_menu: bool,
    /// Neste quadro o Esc fechou alguma coisa — nao cancela alvo nem AUTO.
    esc_consumido: bool,
    /// Alt+Enter.
    tela_cheia: bool,
    /// Missoes ja' entregues (id -> fim do cooldown) e a faccao, do
    /// `QuestEstado`: o menu de missoes calcula bloqueio com isso.
    quest_entregues: std::collections::HashMap<u16, i64>,
    faccao_qid: u8,
    /// Ultima vez que o "ir ate' o alvo" pediu rota.
    ultima_aproximacao: f64,
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
    /// Inclinacao da camera, em radianos acima do horizonte.
    cam_pitch: f32,
    /// Altura pra onde a camera OLHA, com atraso.
    ///
    /// Campo e nao variavel local porque desenho e mira leem a mesma: com uma
    /// conta em cada lugar, o clique mira num mundo e o olho ve' outro — e o
    /// erro so' aparece quando o jogador esta' subindo um barranco.
    /// `f32::MIN` = ainda nao assentou.
    cam_altura: f32,
    /// Velocidade vertical da camera. A mola precisa dela pra ter inercia —
    /// sem guardar a velocidade, ela vira interpolacao de novo.
    cam_altura_vel: f32,
    /// Fila de digitacao. Ver `entrada` — a repeticao de tecla passa por aqui.
    teclado: entrada::Teclado,
    /// Quanto o jogador inclinou A MAIS do que o zoom pediu, em radianos.
    ///
    /// Guardar o DESVIO e nao o angulo e' o que deixa o automatico e a mao
    /// conviverem: girar a roda muda o enquadramento sem apagar a correcao
    /// que a mao fez, e a mao continua mandando dentro da banda daquele zoom.
    cam_pitch_ajuste: f32,
    /// Onde o arrasto de camera comecou.
    arrasto_de: Vec2,
    /// Este aperto do botao direito ja' virou camera?
    ///
    /// Vale pro APERTO INTEIRO: passou do limiar uma vez, nao volta a ser
    /// defesa ate' o jogador soltar. Sem isso, parar a mao no meio do giro
    /// levantaria o escudo no meio da briga.
    arrasto_virou_camera: bool,
    /// Arrasto de rotacao em andamento: posicao do mouse no quadro anterior.
    arrasto: Option<Vec2>,
    /// Camera por toque (um dedo gira, pinca da' zoom). Ver `gesto_camera`.
    gesto_camera: gesto_camera::GestoCamera,
    /// O que o toque pediu NESTE quadro.
    toque_acao: gesto_camera::Acao,
    /// Havia dedo na tela neste quadro: o aperto simulado do mouse nao vale.
    toque_ativo: bool,
    /// `get_time` do ultimo quadro com dedo na tela.
    ultimo_toque: f64,
    /// Alvos da camera (a entrada mexe neles; a camera persegue).
    camera_suave: camera_suave::CameraSuave,
    /// Um dedo girou a camera e ainda nao soltou: ao soltar sobra inercia.
    girando_toque: bool,
    /// WASD virtual na metade esquerda (so' com toque).
    joystick: joystick::Joystick,
    rede: hud::Rede,
    ultimo_ping: f64,
    /// Marca da ultima janela de banda: instante e total de bytes.
    banda_marca: (f64, u64),
    info: hud::Info,
    chat: Vec<String>,
    /// Modo economia de energia (`economia.rs`).
    economia: economia::Economia,
    /// "Onde obter" (`onde_obter.rs`).
    onde_obter: onde_obter::OndeObter,
    /// Ultimo pedido de tela acesa mandado ao sistema.
    tela_acesa: bool,
}

#[macroquad::main(window_conf)]
async fn main() {
    // Os modelos entram uma vez, no boot. O desenho e' sincrono, entao nada
    // pode ficar carregando no meio do quadro.
    let mut vox = VoxCache::default();
    for nome in render3d::MODELOS_DE_GENTE {
        vox.load(nome, render3d::VOXEL).await;
    }
    for (nome, altura) in render3d::ALTURA_DO_BICHO {
        vox.load_na_altura(nome, altura).await;
    }
    // O personagem em PECAS (docs/character create.md). Sem o arquivo, o
    // desenho cai no modelo inteiro de antes.
    vox.load_rig(render3d::RIG_CORPO, render3d::VOXEL, rig::pivo).await;
    for nome in ["humanoides/pistoleiro", "humanoides/mago", "humanoides/arqueiro"] {
        vox.load_rig(nome, render3d::VOXEL, rig::pivo).await;
    }
    // Um corpo por oficio (tools/voxrender/npcs.py). Sem o arquivo, o NPC cai
    // no corpo do jogador.
    for nome in render3d::MODELOS_DE_NPC {
        vox.load_rig(nome, render3d::VOXEL, rig::pivo).await;
    }
    vox.load_rig(render3d::RIG_CHAPEU, render3d::VOXEL, rig::pivo).await;
    vox.load_variantes("saque", render3d::VOXEL, &render3d::variantes_do_saque()).await;
    // Os bichos em PECAS (tools/voxrender/bichos.py). Sem o arquivo, o mob
    // cai no modelo inteiro de antes.
    // As armas do primeiro conjunto (tools/voxrender/armas.py).
    // E as ferramentas de coleta (machado, picareta de cada cor).
    for nome in ["espada", "escudo", "katana", "bainha", "pistola", "coldre"].into_iter().chain(rig::FERRAMENTAS) {
        vox.load_arma(nome, render3d::VOXEL).await;
    }
    for (nome, altura) in bicho::BICHOS {
        vox.load_bicho(nome, altura).await;
    }

    if std::env::var("MMO_PREVIA_SKILLS").is_ok() {
        previa_skills::abrir(&vox).await;
        return;
    }
    if std::env::var("MMO_PREVIA_PERSONAGENS").is_ok() {
        personagens::previa(&vox).await;
        return;
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
        campo_login_ativo: false,
        teclado_virtual: teclado_virtual::TecladoVirtual::default(),
        google: login_google::LoginGoogle::consultando(),
        token_login: None,
        personagens: Vec::new(),
        armas: Vec::new(),
        selecao_personagem: personagens::Personagens::default(),
        selecionado: 0,
        personagem_atual: std::env::var("MMO_CHAR").ok().filter(|v| !v.is_empty()),
        net: None,
        terreno: None,
        vox,
        solido: render3d::material_solido(),
        map: None,
        world: World::default(),
        alvo: None,
        bolsa: bolsa::Bolsa::default(),
        ficha: hud::Ficha::default(),
        habilidades: habilidades::Habilidades::default(),
        auto_combate: auto_combate::AutoCombate::default(),
        loja: loja::Loja::default(),
        ganhos: ganhos::Ganhos::default(),
        interacao: loja::Pendente::default(),
        craft: craft_ui::Craft::default(),
        forja: forja_ui::Forja::default(),
        bonus_xp_ate: 0,
        bonus_fortuna_ate: 0,
        bonus_sorte_ate: 0,
        barra: Default::default(),
        prefs: Default::default(),
        config_barra: Default::default(),
        config_coleta: Default::default(),
        config_interface: Default::default(),
        coleta_hud: Default::default(),
        coleta_pendente: None,
        coleta_auto_estava: false,
        missoes: missoes::Missoes::default(),
        auto_missao: auto_missao::AutoMissao::default(),
        auto_coleta: auto_coleta::AutoColeta::default(),
        toque_coleta: toque::ToqueLongo::default(),
        toque_barra: toque::ToqueLongo::default(),
        dialogo: dialogo::Dialogo::default(),
        rastro: rastro::Rastro::default(),
        telegrafos: telegrafico::Telegrafos::default(),
        tremor: (0.0, 0.0),
        morte: morte::Morte::default(),
        corrida: corrida::Corrida::default(),
        correndo_auto: false,
        ultimo_npc: None,
        mapa: mapa::Mapa::default(),
        construcoes: construcoes::Construcoes::default(),
        ir_para: ir_para::IrPara::default(),
        menu_missoes: menu_missoes::MenuMissoes::default(),
        diarias: diarias::Diarias::default(),
        menu: menu::Menu::default(),
        lojas: lojas::Lojas::default(),
        mercado: mercado_ui::Mercado::default(),
        voltar_ao_menu: false,
        esc_consumido: false,
        tela_cheia: false,
        quest_entregues: std::collections::HashMap::new(),
        faccao_qid: 0,
        ultima_aproximacao: 0.0,
        input_seq: 0,
        ultimo_input: 0.0,
        tick: 0,
        pedacos_desenhados: 0,
        cam_yaw: 0.0,
        cam_zoom: 1.0,
        cam_pitch: render3d::pitch_do_zoom(1.0),
        cam_altura: f32::MIN,
        cam_altura_vel: 0.0,
        teclado: entrada::Teclado::novo(),
        cam_pitch_ajuste: 0.0,
        arrasto_de: Vec2::ZERO,
        arrasto_virou_camera: false,
        arrasto: None,
        gesto_camera: gesto_camera::GestoCamera::default(),
        toque_acao: gesto_camera::Acao::Nada,
        toque_ativo: false,
        ultimo_toque: f64::NEG_INFINITY,
        camera_suave: camera_suave::CameraSuave::default(),
        girando_toque: false,
        joystick: joystick::Joystick::default(),
        rede: hud::Rede::default(),
        ultimo_ping: 0.0,
        banda_marca: (0.0, 0),
        info: hud::Info::default(),
        chat: Vec::new(),
        economia: economia::Economia::default(),
        onde_obter: onde_obter::OndeObter::default(),
        tela_acesa: false,
    };
    // `MMO_HOST` explicito pula a escolha — e' o caminho do run-client.sh e dos
    // testes de carga.
    if jogo.host.is_some() {
        jogo.conectar();
    }

    loop {
        jogo.passo();
        jogo.desenhar();
        // Modo economia: o quadro cai pra `economia::FPS`.
        jogo.economia.segurar_quadro();
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
    // iPhone/iPad: tela cheia na resolucao nativa. A orientacao (paisagem)
    // vem do Info.plist do pacote (scripts/build-ios.sh).
    #[cfg(target_os = "ios")]
    {
        conf.fullscreen = true;
        conf.high_dpi = true;
    }
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
        // Antes de tudo: a digitacao deste quadro. Quem desenha campo de
        // texto le' dela, e nao da fila crua da macroquad.
        self.teclado.coleta(get_time());
        hud_layout::acompanhar();
        self.passo_google();
        self.passo_teclado_virtual();
        self.receber_lista();
        self.pump_rede();
        // Jogando, a tela nao apaga sozinha: o automatico segue sem toque.
        let quer_acesa = matches!(self.tela, Tela::Jogando);
        if quer_acesa != self.tela_acesa {
            nativo::manter_tela_acesa(quer_acesa);
            self.tela_acesa = quer_acesa;
        }
        if !quer_acesa && self.economia.ativa {
            self.economia.sair(get_time());
        }
        if matches!(self.tela, Tela::Jogando) {
            self.habilidades.acompanhar_alvos(&mut self.world);
            {
                let terreno = self.terreno.as_ref();
                self.world.tick(get_frame_time(), &|x, z| {
                    terreno.map_or(0.0, |t| t.altura_apoio(x, z, shared::ENTITY_RADIUS))
                });
            }
            self.seguir_altura();
            // Nenhum painel abre por tecla (docs/HUD.md 2.5): so' os icones do
            // HUD e o Menu. O que a interface pega e' medido ANTES da entrada:
            // o clique que fecha o mapa por fora nao pode virar mira no mesmo
            // quadro.
            self.mapa.acompanhar();
            self.construcoes.acompanhar();
            // Modo economia: parado tempo demais entra sozinho; ligado (ou
            // acabando de sair), nenhum toque chega ao mundo — so' o deslize.
            let agora = get_time();
            if self.economia.parado_demais(agora, economia::houve_entrada()) {
                self.entrar_economia();
            }
            let eco = self.economia.bloqueia_entrada(agora);
            let eco_ativa = self.economia.ativa;
            let ui_pega = self.ui_pega_mouse();
            // Mapa e minimapa so' recebem clique quando estao a' mostra.
            if !eco && (self.mapa.aberto || !self.painel_grande()) {
                match self.mapa.entrada(self.world.self_pos()) {
                    Some(mapa::Entrada::Viajar(destino)) => self.iniciar_viagem(destino),
                    Some(mapa::Entrada::Ir(alvo)) => self.iniciar_ir_para(alvo),
                    None => {}
                }
            }
            self.esc_consumido = !eco && is_key_pressed(KeyCode::Escape) && self.esc();
            // Toques antes de qualquer clique: com dedo, o clique no mundo sai
            // no SOLTAR (ver `gesto_camera`).
            if eco {
                self.toque_acao = gesto_camera::Acao::Nada;
            } else {
                self.ler_toques();
            }
            if !ui_pega {
                self.atualizar_alvo();
            }
            // Painel do Menu fechado pelo X: nao volta mais pro Menu.
            if self.voltar_ao_menu && !self.painel_grande() && !self.missoes.aberta {
                self.voltar_ao_menu = false;
            }
            if !eco {
                self.teclas_de_acao();
            }
            self.acompanhar_loja();
            self.atualizar_auto_combate();
            self.usar_habilidade();
            self.auto_da_barra();
            self.world.alvo = self.alvo;
            self.ir_ate_o_alvo();
            self.conduzir_viagem();
            self.conduzir_ir_para();
            if let Some(eu) = self.world.self_pos() {
                self.rastro.acompanha(eu);
                self.mapa.rota = self.rastro.caminho(eu);
            }
            // Indo sozinho e nada segurando: depois de 1,5 s andando, corre.
            let indo_sozinho = self.mapa.viagem.ativa()
                || self.ir_para.ativo()
                || self.rastro.ativo()
                || self.auto_missao.etapa() == Some(auto_missao::Etapa::Indo);
            let automatico = indo_sozinho
                && !self.dialogo.aberto
                && !self.auto_combate.ativo()
                // A coleta ANDA pro spot pela viagem: corre indo, nao parada nele.
                && !(self.auto_coleta.ativo() && !self.mapa.viagem.ativa());
            self.correndo_auto = self.corrida.atualiza(self.world.self_pos(), automatico, get_frame_time());
            self.atualizar_auto_coleta();
            self.conduzir_auto_missao();
            if !eco {
                self.camera_controles();
            }
            self.enviar_input();
            self.sincroniza_preferencias();
            self.medir_rede();
            // Com o modo economia ligado nada e' desenhado: malha nova espera.
            if let Some(t) = self.terreno.as_mut().filter(|_| !eco_ativa) {
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
        // Trocando de zona: o pendente vai pelo canal velho antes de soltar.
        self.guardar_preferencias_agora();
        self.prefs = preferencias::Sincronia::default();
        self.world = World::default();
        self.ganhos = ganhos::Ganhos::default();
        self.habilidades = habilidades::Habilidades::default();
        if self.economia.ativa {
            self.economia.sair(get_time());
        }
        self.auto_combate.parar();
        self.loja.fecha();
        self.interacao.cancela();
        self.missoes.limpa();
        self.auto_missao.parar();
        self.auto_coleta.parar();
        self.dialogo.fechar();
        self.ultimo_npc = None;
        let filtros = std::mem::take(&mut self.mapa.filtros);
        self.mapa = mapa::Mapa::default();
        self.mapa.filtros = filtros;
        self.ir_para.parar();
        self.menu_missoes.aberto = false;
        self.diarias.fechar();
        self.quest_entregues.clear();
        self.construcoes = construcoes::Construcoes::default();
        self.rastro.limpa();
        self.menu.fechar();
        self.lojas.fechar();
        self.voltar_ao_menu = false;
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
            ServerMessage::HandshakeAck { xp_multiplier, .. } => {
                self.ficha.mult_xp = xp_multiplier;
                match self.token_login.clone() {
                    Some(token) => self.envia(ClientMessage::LoginToken { token }),
                    None => self.envia(ClientMessage::Login {
                        username: self.usuario.clone(),
                        password: self.senha.clone(),
                    }),
                }
            }
            ServerMessage::CharacterList { chars, available_weapons } => {
                self.personagens = chars;
                self.personagens.sort_by(|a,b|a.name.to_lowercase().cmp(&b.name.to_lowercase()));
                self.armas = available_weapons;
                self.selecao_personagem.recebeu_lista(&self.personagens,&self.armas,&mut self.selecionado);
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
                self.selecao_personagem.falhou(reason);
                self.tela = Tela::Personagens;
            }
            ServerMessage::LoginDenied { reason } => {
                // Sessao do Google vencida: some, e o jogador entra de novo.
                if self.token_login.take().is_some() {
                    self.tela = Tela::Erro("sua sessão do Google expirou — entre de novo".into());
                } else {
                    self.tela = Tela::Erro(format!("login negado: {reason}"));
                }
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
            // Pedra de minerio: a pedra nasce da semente dos dois lados, entao
            // o que o servidor manda e' so' QUAL sumiu. Sem isso o veio
            // limpo continuaria brilhando e o jogador nao teria como saber
            // onde ja' passou.
            ServerMessage::PedrasEsgotadas { colunas } => {
                if let Some(t) = &mut self.terreno {
                    for c in colunas { t.marca_esgotada(c, true); }
                }
            }
            ServerMessage::PedraEsgotada { coluna } => {
                if let Some(t) = &mut self.terreno { t.marca_esgotada(coluna, true); }
            }
            ServerMessage::PedraVoltou { coluna } => {
                if let Some(t) = &mut self.terreno { t.marca_esgotada(coluna, false); }
            }
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
                self.auto_combate.parar();
                self.loja.fecha();
                self.interacao.cancela();
                self.missoes.fecha();
                self.auto_missao.parar();
                self.dialogo.fechar();
                self.rastro.limpa();
                // Ilha do arquipelago: o terreno nasce da SEMENTE, e nem o
                // arquivo de tiles nem um byte de rede entram nisso.
                self.terreno = shared::terreno::def_da_zona(&map_name).map(terreno::Terreno::novo);
                // Mapa novo pra ilha nova; a viagem da ilha anterior morre junto.
                // Os filtros do mapa valem a sessao: sobrevivem a ilha nova.
                let filtros = std::mem::take(&mut self.mapa.filtros);
                self.mapa = mapa::Mapa::para(shared::terreno::def_da_zona(&map_name));
                self.mapa.filtros = filtros;
                self.ir_para.parar();
                self.construcoes = construcoes::Construcoes::para(shared::terreno::def_da_zona(&map_name));
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
                self.world.acertos(&snapshot.acertos);
            }
            ServerMessage::Chat { from, text } => {
                self.chat.push(format!("{from}: {text}"));
                if self.chat.len() > 8 {
                    self.chat.remove(0);
                }
            }
            // O que a bolsa mostra. O servidor manda tudo no login e de novo a
            // cada mudanca; aqui so' se guarda.
            ServerMessage::InventoryUpdate { slots } => {
                // O que ENTROU sobe do personagem (coleta, loot, compra).
                let entrou = self.ganhos.bolsa_nova(&slots);
                self.economia.itens_novos(&entrou);
                self.bolsa.slots = slots;
            }
            ServerMessage::ResourceSources { items } => self.onde_obter.define(items),
            ServerMessage::ItemsConfig { items } => {
                self.mercado.vinculados = items.iter().filter(|i| i.vinculado).map(|i| i.id).collect();
                self.bolsa.nomes = items.into_iter().map(|i| (i.id, i.name)).collect();
            }
            ServerMessage::MercadoLista { anuncios, pagina, tem_mais } => self.mercado.lista(anuncios, pagina, tem_mais),
            ServerMessage::MercadoMeus { anuncios, historico, tp } => self.mercado.meus(anuncios, historico, tp),
            ServerMessage::MercadoEntregas { cartas, tp } => self.mercado.entregas(cartas, tp),
            ServerMessage::MercadoResultado { ok, texto } => {
                // Venda fechada chega com o painel fechado: o chat avisa.
                self.chat.push(format!("Mercado: {texto}"));
                if self.chat.len() > 8 {
                    self.chat.remove(0);
                }
                for pedido in self.mercado.resultado(ok, texto, get_time()) {
                    self.envia(pedido);
                }
            }
            ServerMessage::StatsUpdate { stats, equipment } => {
                self.bolsa.stats = Some(stats);
                self.bolsa.equip = equipment;
            }
            ServerMessage::GoldUpdate { gold } => self.bolsa.ouro = gold,
            ServerMessage::CraftRecipes { recipes } => self.craft.define_receitas(recipes),
            ServerMessage::CraftResultado { ok, motivo, item_id, .. } => {
                let txt = if ok { format!("Criado: {}", self.bolsa.nome(item_id)) } else { format!("Craft recusado: {motivo}") };
                self.craft.resultado(ok, txt.clone(), get_time());
                self.chat.push(txt);
            }
            ServerMessage::RefinoResultado { resultado, nivel, item_id, motivo } => {
                let nome = self.bolsa.nome(item_id);
                self.forja.resultado(resultado, nivel, &nome, &motivo, get_time());
                self.chat.push(forja_ui::texto_do_resultado(resultado, nivel, &nome, &motivo).0);
            }
            // O Ferreiro da vila abre a mesma Forja do HUD.
            ServerMessage::BlacksmithOpen => {
                self.interacao.cancela();
                self.craft.fechar();
                self.forja.abrir();
            }
            ServerMessage::BuffXp { ate } => self.bonus_xp_ate = ate,
            ServerMessage::BuffsDeDrop { fortuna_ate, sorte_ate } => {
                self.bonus_fortuna_ate = fortuna_ate;
                self.bonus_sorte_ate = sorte_ate;
            }
            ServerMessage::BarraDeItens { espacos } => self.barra.do_servidor(&espacos),
            ServerMessage::Preferencias { prefs } => self.aplica_preferencias(prefs),
            ServerMessage::Morte { xp_perdido } => {
                self.parar_tudo_ao_morrer();
                self.morte.morreu(xp_perdido);
                self.economia.morreu();
            }
            ServerMessage::DownedUpdate { active, .. } => {
                if active && !self.morte.morto {
                    self.parar_tudo_ao_morrer();
                }
                self.morte.caido(active);
            }
            ServerMessage::Recuperaveis { mortes, gratis_restantes } => {
                self.morte.recuperaveis(mortes, gratis_restantes);
            }
            ServerMessage::RecuperarXpResultado { ok, motivo, .. } => {
                self.chat.push(motivo.clone());
                self.morte.resultado(ok, motivo);
            }
            ServerMessage::ShopOpen { items, vendor_id, .. } => {
                self.interacao.cancela();
                self.missoes.fecha();
                self.auto_missao.parar();
                self.dialogo.fechar();
                self.mapa.viagem.cancelar();
                self.ir_para.parar();
                self.loja.abre(vendor_id, items);
            }
            ServerMessage::ShopClose => self.loja.fecha(),
            ServerMessage::ShopTradeResult { ok: false, reason } => {
                self.chat.push(format!("loja: {reason}"));
                self.loja.avisa(reason);
            }
            ServerMessage::ProgressUpdate { xp, level } => {
                self.bolsa.nivel = level;
                self.ficha.xp = xp;
                self.ficha.nivel = level;
            }
            ServerMessage::ManaUpdate { current } => self.ficha.mp = Some(current),
            ServerMessage::StaminaUpdate { current } => self.ficha.vigor = Some(current),
            ServerMessage::SkillsConfig { skills } => self.habilidades.catalogo = skills,
            ServerMessage::SkillsState { cooldowns, busy_s } => self.habilidades.estado(cooldowns, busy_s),
            ServerMessage::SkillRejected { skill_id, motivo } => self.habilidades.rejeitada(skill_id, motivo),
            ServerMessage::MobAttackFx { attacker, target, dir, impact_s } => {
                let alvo = target.and_then(|id| self.world.ents.get(&id)).map(|e| e.render_pos);
                if let Some(e) = self.world.ents.get_mut(&attacker) {
                    e.golpe = 0.0;
                    e.ataque_mob = Some((target, 0.0, impact_s));
                    e.mira = Some((alvo.unwrap_or(e.render_pos + vec2(dir[0], dir[1])), impact_s + 0.3));
                }
            }
            ServerMessage::SkillCastFx { skill_id, caster_eid: Some(id), caster_pos, target_pos, target_eid, .. } => {
                let de = vec2(caster_pos.x, caster_pos.y);
                let alvo = vec2(target_pos.x, target_pos.y);
                self.habilidades.efeito(skill_id, id, de, alvo, target_eid, false);
                if let (Some(e), Some(s)) = (self.world.ents.get_mut(&id), self.habilidades.catalogo.iter().find(|s| s.id == skill_id)) {
                    e.skill = Some((skill_id, 0.0, s.impacto_em()));
                    e.combo = None;
                    e.combo_ant = None;
                    e.sacada = 1.0;
                    if de.distance(alvo) > 0.01 { e.mira = Some((alvo, s.impacto_em() + 0.3)); }
                }
            }
            ServerMessage::SkillImpactFx { skill_id, caster_eid, caster_pos, target_pos } => {
                self.habilidades.efeito(skill_id, caster_eid, vec2(caster_pos.x, caster_pos.y), vec2(target_pos.x, target_pos.y), None, true);
            }
            ServerMessage::SkillCastCancel { caster_eid, skill_id } => {
                self.habilidades.cancelar(caster_eid, skill_id, self.world.self_id == Some(caster_eid));
                if let Some(e) = self.world.ents.get_mut(&caster_eid) { e.skill = None; }
            }
            // Missoes: a oferta abre a janela do NPC com quem se acabou de
            // falar; log, progresso e givers so' atualizam o estado.
            ServerMessage::QuestOffer { giver_name, quests, .. } => {
                self.interacao.cancela();
                self.loja.fecha();
                self.ao_receber_oferta(giver_name, quests);
            }
            ServerMessage::QuestLog { quests } => self.missoes.define_log(quests),
            ServerMessage::QuestUpdate { quest_id, progress, status } => {
                use shared::historia;
                if status == shared::quests::quest_status::TURNED_IN {
                    self.quest_entregues.entry(quest_id).or_insert(0);
                }
                // A historia passou pro proximo passo com a auto missao nela:
                // segue sozinha pro novo.
                let segue = status == shared::quests::quest_status::ACTIVE
                    && historia::e_da_historia(quest_id)
                    && self.auto_missao.quest.is_some_and(|q| q != quest_id && historia::e_da_historia(q));
                self.missoes.atualiza(quest_id, progress, status);
                if segue {
                    if let Some(d) = shared::quests::quest_by_id(quest_id) {
                        self.auto_missao.iniciar(quest_id, d.title.to_string(), get_time());
                    }
                }
            }
            ServerMessage::QuestEstado { entregues, faccao } => {
                self.quest_entregues = entregues.into_iter().collect();
                self.faccao_qid = faccao;
            }
            ServerMessage::MapaDaIlha { zonas, recursos, nomes, rendimentos, chefes } => {
                self.mapa.define_info(zonas, recursos, nomes, rendimentos);
                self.mapa.define_chefes(chefes);
            }
            ServerMessage::Telegrafico { id, chefe, forma, centro, dir, carga_s } => {
                let agora = get_time();
                self.telegrafos.comeca(id, chefe, forma, centro, dir, carga_s, agora);
                // O chefe arma o golpe no desenho (preparacao ate' o impacto).
                if let Some(e) = self.world.ents.get_mut(&chefe) {
                    let (c, d) = (vec2(centro[0], centro[1]), vec2(dir[0], dir[1]));
                    e.carga_chefe = Some(chefe_anim::Carga::nova(&forma, c, d, e.render_pos, carga_s, agora));
                }
            }
            ServerMessage::TelegraficoFim { id, impacto } => {
                let agora = get_time();
                if let Some((chefe, forma, centro, dir)) = self.telegrafos.termina(id, impacto, agora) {
                    // Cancelado (chefe morreu): o gesto para. Impacto: sai o golpe.
                    let kind = self.world.ents.get(&chefe).map_or(0, |e| e.meta.kind);
                    if let Some(e) = self.world.ents.get_mut(&chefe) {
                        match (&mut e.carga_chefe, impacto) {
                            (Some(c), true) => c.bateu(agora),
                            (c, false) => *c = None,
                            _ => {}
                        }
                    }
                    if impacto {
                        if let Some(t) = &self.terreno {
                            let cor = chefe_anim::cor_do_chefe(kind);
                            for (k, p) in chefe_anim::pontos_de_impacto(&forma, centro, dir).into_iter().enumerate() {
                                lascas::explosao(vec3(p.x, t.altura(p.x, p.y) + 0.1, p.y), cor, id.wrapping_mul(31) ^ k as u32);
                            }
                        }
                        // Perto do golpe: a camera sente (curto e com teto).
                        if self.world.self_pos().is_some_and(|eu| eu.distance(centro) <= forma.alcance() + 8.0) {
                            self.tremor = (chefe_anim::TREMOR_FORCA, agora + chefe_anim::TREMOR_S as f64);
                        }
                    }
                }
            }
            ServerMessage::QuestGivers { available } => self.missoes.define_givers(available),
            ServerMessage::Rota { pontos, destino } => {
                self.rastro.define(pontos.into_iter().map(|p| vec2(p[0], p[1])).collect(), vec2(destino[0], destino[1]));
            }
            ServerMessage::QuestDestino { quest_id, tipo, pos, raio, npc_eid } => {
                use shared::quests::destino_tipo;
                // Criar e refinar nao se faz andando: abre o painel e a auto
                // missao para (quem cria e' o jogador).
                if tipo == destino_tipo::PAINEL_CRAFT || tipo == destino_tipo::PAINEL_FORJA {
                    if self.auto_missao.quest == Some(quest_id) {
                        self.auto_missao.parar();
                        if tipo == destino_tipo::PAINEL_CRAFT {
                            self.forja.fechar();
                            self.craft.abrir();
                            self.chat.push("Missão: crie um equipamento no Craft.".into());
                        } else {
                            self.craft.fechar();
                            self.forja.abrir();
                            self.chat.push("Missão: tente refinar uma peça na Forja.".into());
                        }
                    }
                } else {
                    let npc = npc_eid.map(|e| shared::EntityId(e as u32));
                    if let Some(aviso) = self.auto_missao.destino_recebido(quest_id, tipo, vec2(pos[0], pos[1]), raio, npc, get_time()) {
                        self.chat.push(aviso);
                    }
                }
            }
            ServerMessage::NoDeColeta { no } => {
                let no = no.map(|(k, onde, c, t)| (k, vec2(onde[0], onde[1]), vec2(c[0], c[1]), t));
                if let auto_coleta::Acao::Ir(p) = self.auto_coleta.no_recebido(no, get_time()) {
                    self.mapa.viagem.iniciar(p, get_time());
                }
            }
            ServerMessage::ColetaEstado { tipo, intervalo_s, progresso, centro, pausado } => {
                self.coleta_hud.recebe(tipo, intervalo_s, progresso, centro, pausado, get_time());
                self.auto_coleta.estado_coleta(tipo, pausado, get_time());
            }
            ServerMessage::PocaoGrupo { grupo, recarga_s, cura_s } => {
                self.barra.pocao_grupo(grupo, recarga_s, cura_s, get_time());
            }
            // As demais ainda nao tem UI;
            // ignorar e' seguro porque nada aqui e' autoritativo.
            _ => {}
        }
    }

    // ─────────────────────────────── jogo ────────────────────────────────
    /// Clique esquerdo escolhe alvo; direito ou Esc limpa. So' avisa o
    /// servidor quando o alvo REALMENTE muda.
    fn atualizar_alvo(&mut self) {
        let antes = self.alvo;
        // Esc so' limpa o alvo quando nao fechou nada neste quadro.
        if is_key_pressed(KeyCode::Escape) && !self.esc_consumido {
            self.alvo = None;
        }
        if self.clique_no_mundo() {
            // Clique no mundo e' comando novo: a viagem do mapa acaba aqui, e a
            // auto missao junto.
            self.mapa.viagem.cancelar();
            self.auto_missao.parar();
            self.ir_para.parar();
            let centro = self.world.self_pos().unwrap_or(Vec2::ZERO);
            // Empresta so' o CAMPO `terreno`, nao `self` inteiro: o alvo e a
            // rota sao escritos logo abaixo.
            let (mx, my) = self.pos_do_clique();
            let escolhido = {
                let terreno = self.terreno.as_ref();
                let f = |x: f32, z: f32| terreno.map_or(0.0, |t| t.altura(x, z));
                let vista = render3d::Vista::nova(
                    centro, self.cam_yaw, self.cam_zoom, self.cam_pitch, self.cam_altura, &f,
                );
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
            // So' bicho e gente viram alvo. Clicar no saque e' ir BUSCAR (ele
            // e' pego por proximidade); clicar em NPC nao mira ninguem.
            let tag = escolhido.0.and_then(|id| self.world.ents.get(&id)).map(|e| (e.meta.tag, e.render_pos));
            // Qualquer clique novo desfaz o "ir falar com o NPC" anterior.
            self.interacao.cancela();
            match tag {
                Some((shared::EntityTag::Enemy | shared::EntityTag::Player, _)) => self.alvo = escolhido.0,
                Some((shared::EntityTag::Npc, p)) => {
                    if let Some(id) = escolhido.0 {
                        self.falar_com(id, p);
                    }
                }
                Some((shared::EntityTag::Loot, p)) => {
                    self.envia(ClientMessage::MoverPara { x: p.x, z: p.y });
                }
                Some(_) => {}
                None => self.alvo = None,
            }
            // O cliente nunca manda rota — so' um ponto que ele ja' poderia
            // alcancar andando. Clicou numa pedra/tronco: vai ate' o alcance
            // dela e coleta ao chegar (`acompanhar_coleta_manual`).
            self.coleta_pendente = None;
            if let Some(p) = escolhido.1 {
                let no = self.terreno.as_ref().and_then(|t| t.coletavel_perto(p, 0.8));
                match (no, self.world.self_pos()) {
                    (Some((coluna, c, _tipo, raio)), Some(eu)) => {
                        let dir = (eu - c).try_normalize().unwrap_or(Vec2::X);
                        let onde = c + dir * (raio + shared::ENTITY_RADIUS + shared::COLETA_ALCANCE_UN * 0.5);
                        self.auto_coleta.parar();
                        self.coleta_pendente = Some((coluna, c, raio));
                        self.envia(ClientMessage::MoverPara { x: onde.x, z: onde.y });
                    }
                    _ => self.envia(ClientMessage::MoverPara { x: p.x, z: p.y }),
                }
            }

        }
        if let Some(t) = self.alvo {
            // sumiu ou morreu: o alvo solta
            if self.world.ents.get(&t).map_or(true, |e| e.morte.is_some()) {
                self.alvo = None;
            }
        }
        if self.alvo != antes {
            self.envia(ClientMessage::SetTarget { target: self.alvo });
        }
    }

    /// Clicou num NPC: perto, interage; longe, anda ate' ele e interage ao
    /// chegar (ver `acompanhar_loja`).
    fn falar_com(&mut self, id: shared::EntityId, npc: Vec2) {
        let Some(eu) = self.world.self_pos() else { return };
        if eu.distance(npc) <= loja::PERTO {
            self.interagir_agora(id);
        } else {
            let destino = npc + (eu - npc).normalize_or_zero() * 2.0;
            self.envia(ClientMessage::MoverPara { x: destino.x, z: destino.y });
            self.interacao.ir(id);
        }
    }

    /// A cada quadro: interage com o NPC pendente ao chegar, e fecha a loja
    /// longe do vendedor. Andar no teclado desiste de ir ate' o NPC.
    fn acompanhar_loja(&mut self) {
        if self.andando_na_mao() {
            self.interacao.cancela();
        }
        let eu = self.world.self_pos();
        let npc = self.interacao.npc.and_then(|id| self.world.ents.get(&id)).map(|e| e.render_pos);
        if let Some(eu) = eu {
            if let Some(id) = self.interacao.acompanhar(eu, npc) {
                self.interagir_agora(id);
            }
        }
        let vendedor = self.loja.vendedor.and_then(|id| self.world.ents.get(&id)).map(|e| e.render_pos);
        self.loja.conferir_distancia(eu, vendedor);
        let mestre = self.missoes.npc.and_then(|id| self.world.ents.get(&id)).map(|e| e.render_pos);
        self.missoes.conferir_distancia(eu, mestre);
    }

    /// Chegou num NPC: se ele e' o alvo de uma missao "fale com", abre a
    /// conversa (a missao conta ao TERMINAR o dialogo); senao, `Interact` como
    /// sempre.
    fn interagir_agora(&mut self, id: shared::EntityId) {
        self.ultimo_npc = Some(id);
        if let Some((quest_id, titulo, quem)) = self.conversa_de_missao(id) {
            let falas = shared::quests::falas(quest_id, shared::quests::momento::CONVERSA);
            self.dialogo.abrir(&quem, &titulo, &falas, dialogo::Fim::Conversa { quest_id, npc: id }, String::new());
            return;
        }
        self.envia(ClientMessage::Interact { target_eid: Some(id.0 as u64) });
    }

    /// A missao "fale com" ativa cujo alvo e' este NPC: (id, titulo, nome dele).
    fn conversa_de_missao(&self, id: shared::EntityId) -> Option<(u16, String, String)> {
        use shared::quests::{objective_kind, quest_status};
        let e = self.world.ents.get(&id)?;
        if e.meta.tag != shared::EntityTag::Npc {
            return None;
        }
        let papel = shared::npc_papel_de_kind(e.meta.kind) as u16;
        // Viagem da historia tambem e' conversa: com o Capitao do Porto.
        let capitao = shared::construcao::Papel::Estaleiro as u16;
        let q = self.missoes.log.iter().find(|q| {
            q.status == quest_status::ACTIVE
                && ((q.obj_kind == objective_kind::TALK && q.obj_target == papel)
                    || (q.obj_kind == objective_kind::VIAGEM && papel == capitao))
        })?;
        Some((q.id, q.title.clone(), e.meta.name.clone().unwrap_or_default()))
    }

    /// Resposta do Mestre (`QuestOffer`): entrega pronta vira dialogo de
    /// "Receber"; missao nova vira dialogo de "Aceitar" — ou, em auto missao
    /// logo depois de receber, ja' e' aceita e o personagem segue; sem nada a
    /// fazer, a janela de sempre.
    fn ao_receber_oferta(&mut self, quem: String, quests: Vec<shared::quests::QuestNet>) {
        use shared::quests::{falas, momento, GIVER_MESTRE_DA_ILHA};
        let pronta = {
            let slots = &self.bolsa.slots;
            self.missoes.log.iter()
                .find(|q| q.giver == GIVER_MESTRE_DA_ILHA && missoes::pronta(q, &|id| missoes::na_bolsa(slots, id)))
                .cloned()
        };
        self.missoes.guarda_oferta(self.ultimo_npc, quem.clone(), quests);
        let proxima = self.missoes.oferta().first().cloned();
        if let Some(q) = pronta {
            let r = missoes::recompensa(&q, &self.bolsa.nomes);
            self.dialogo.abrir(&quem, &q.title, &falas(q.id, momento::ENTREGA), dialogo::Fim::Entrega { quest_id: q.id }, r);
        } else if let Some(q) = proxima {
            if self.auto_missao.etapa() == Some(auto_missao::Etapa::AguardandoProxima) {
                self.envia(ClientMessage::AcceptQuest { quest_id: q.id });
                self.chat.push(format!("Nova missão: {}", q.title));
                self.auto_missao.iniciar(q.id, q.title.clone(), get_time());
            } else {
                let r = missoes::recompensa(&q, &self.bolsa.nomes);
                self.dialogo.abrir(&quem, &q.title, &falas(q.id, momento::OFERTA), dialogo::Fim::Oferta { quest_id: q.id }, r);
            }
        } else if !self.auto_missao.ativo() {
            self.missoes.abre_janela();
        }
    }

    /// Clicou numa missao do rastreador (ou "Ir" no diario): auto missao.
    fn iniciar_auto_missao(&mut self, id: u16) {
        let Some(nome) = self.missoes.log.iter().find(|q| q.id == id).map(|q| q.title.clone()) else { return };
        self.mapa.viagem.cancelar();
        self.auto_combate.parar();
        self.auto_coleta.parar();
        self.ir_para.parar();
        self.menu_missoes.aberto = false;
        self.diarias.fechar();
        self.interacao.cancela();
        self.loja.fecha();
        self.missoes.aberta = false;
        if self.alvo.take().is_some() {
            self.envia(ClientMessage::SetTarget { target: None });
        }
        self.auto_missao.iniciar(id, nome, get_time());
    }

    // ─────────────────────────── HUD e Menu (MIR4) ───────────────────────────

    /// Algum painel GRANDE aberto (Menu, Bolsa, Mapa, Craft, Forja, Todas as
    /// missoes, Lojas)? Com ele o HUD some e todo clique e roda sao do painel.
    fn painel_grande(&self) -> bool {
        self.menu.aberto
            || self.bolsa.aberta
            || self.mapa.aberto
            || self.craft.aberto()
            || self.forja.aberto()
            || self.menu_missoes.aberto
            || self.diarias.aberto
            || self.lojas.aberto
            || self.mercado.aberto
            || self.morte.painel
            || self.config_barra.aberto
            || self.config_coleta.aberto
            || self.config_interface.aberto
            || self.onde_obter.aberto()
    }

    /// Morreu: nada automatico continua e os paineis fecham — a tela de morte
    /// fica sozinha na frente.
    fn parar_tudo_ao_morrer(&mut self) {
        self.auto_missao.parar();
        self.auto_coleta.parar();
        self.auto_combate.parar();
        self.fecha_paineis();
        self.alvo = None;
    }

    /// O mouse esta' sobre a interface (o mundo nao recebe o clique)? Uma
    /// pergunta so', no lugar da condicao de onze termos que crescia a cada
    /// painel novo.
    fn entrar_economia(&mut self) {
        let nivel = match self.world.self_id.and_then(|id| self.world.ents.get(&id)) {
            Some(e) if self.ficha.nivel == 0 => e.meta.nivel as u32,
            _ => self.ficha.nivel,
        };
        self.economia.entrar(get_time(), self.ficha.xp, nivel, self.bolsa.ouro);
    }

    /// O que o personagem esta' fazendo, pro resumo da tela preta.
    fn estado_da_economia(&self) -> (&'static str, Color) {
        let verde = Color::new(0.45, 0.85, 0.52, 1.0);
        if self.net.is_none() {
            ("SEM CONEXÃO", Color::new(0.92, 0.30, 0.30, 1.0))
        } else if self.morte.morto {
            ("MORTO", Color::new(0.92, 0.30, 0.30, 1.0))
        } else if self.dialogo.aberto {
            ("AGUARDANDO VOCÊ", hud_estilo::OURO)
        } else if self.auto_combate.ativo() {
            ("AUTO COMBATE", verde)
        } else if self.auto_coleta.ativo() {
            ("AUTO COLETA", verde)
        } else if self.auto_missao.etapa().is_some() {
            ("AUTO MISSÃO", verde)
        } else {
            ("PARADO", Color::new(0.55, 0.56, 0.60, 1.0))
        }
    }

    fn desenhar_economia(&mut self) {
        let agora = get_time();
        let eu = self.world.self_id.and_then(|id| self.world.ents.get(&id));
        let (hp, hp_max, nivel_ent, nome) = eu
            .map(|e| (e.state.hp as i32, e.meta.hp_max as i32, e.meta.nivel as u32, e.meta.name.clone().unwrap_or_default()))
            .unwrap_or_default();
        let nivel = if self.ficha.nivel == 0 { nivel_ent } else { self.ficha.nivel };
        let hp_max = self.bolsa.stats.as_ref().map_or(hp_max, |s| s.hp_max);
        let mult = if self.ficha.mult_xp > 0 { self.ficha.mult_xp } else { shared::DEFAULT_XP_MULTIPLIER };
        let (base, prox) = (shared::xp_for_level_with_mult(nivel, mult), shared::xp_for_level_with_mult(nivel + 1, mult));
        let exp = if prox > base { self.ficha.xp.saturating_sub(base) as f32 / (prox - base) as f32 } else { 0.0 };
        let estado = self.estado_da_economia();
        let bolsa = &self.bolsa;
        let nome_item = |id: u16| bolsa.nome(id);
        let resumo = economia::Resumo {
            nome: &nome,
            nivel,
            hp,
            hp_max,
            exp,
            xp: self.ficha.xp,
            ouro: bolsa.ouro,
            estado,
            ping_ms: self.rede.ms,
            nome_item: &nome_item,
        };
        if self.economia.desenha(&resumo, agora) {
            self.economia.sair(agora);
        }
    }

    fn ui_pega_mouse(&self) -> bool {
        if self.economia.bloqueia_entrada(get_time()) {
            return true;
        }
        if self.painel_grande() || self.morte.pega_mouse() {
            return true;
        }
        let m = Vec2::from(mouse_position());
        let z = hud_layout::atual();
        z.contem(m)
            || z.chat.contains(m)
            || self.mapa.pega_mouse()
            || self.loja.pega_mouse()
            || self.missoes.pega_mouse()
            || self.habilidades.pega_mouse()
            || self.dialogo.pega_mouse()
    }

    /// Fecha os paineis grandes: so' um por vez (docs/HUD.md 4.2).
    fn fecha_paineis(&mut self) {
        self.bolsa.fecha();
        self.mapa.aberto = false;
        self.craft.fechar();
        self.forja.fechar();
        self.menu_missoes.aberto = false;
        self.diarias.fechar();
        self.lojas.fechar();
        self.mercado.fechar();
        self.morte.painel = false;
        self.config_barra.fechar();
        self.config_coleta.fechar();
        self.config_interface.fechar();
        self.menu.fechar();
        self.onde_obter.fechar();
        self.voltar_ao_menu = false;
    }

    /// O "Ir" do Onde obter: fecha os paineis e faz o que a fonte pede.
    fn executar_onde_obter(&mut self, ir: onde_obter::Ir) {
        self.fecha_paineis();
        self.missoes.fecha();
        match ir {
            onde_obter::Ir::Alvo(alvo) => {
                self.chat.push(format!("Indo: {}", alvo.rotulo));
                self.iniciar_ir_para(alvo);
            }
            onde_obter::Ir::AbrirCraft(receita) => self.craft.abrir_receita(receita),
            onde_obter::Ir::AbrirMercado(item) => {
                let nome = self.bolsa.nome(item);
                for pedido in self.mercado.abrir_buscando(&nome) {
                    self.envia(pedido);
                }
            }
        }
    }

    /// Icone 📜 ou titulo do rastreador: abre (ou fecha) o diario.
    fn abrir_diario(&mut self) {
        let era_diario = self.missoes.aberta && self.missoes.npc.is_none();
        self.fecha_paineis();
        if era_diario {
            self.missoes.fecha();
        } else {
            self.missoes.fecha();
            self.missoes.alterna_diario();
        }
    }

    /// Um item do Menu Principal. Abrir pelo Menu empilha: Esc volta a ele.
    fn abrir_do_menu(&mut self, item: menu::Item) {
        use menu::Item;
        self.fecha_paineis();
        self.voltar_ao_menu = true;
        match item {
            Item::Bolsa => self.bolsa.abrir(),
            Item::Missoes => {
                self.missoes.fecha();
                self.missoes.alterna_diario();
            }
            Item::TodasMissoes => self.menu_missoes.abrir(),
            Item::Diarias => self.diarias.abrir(),
            Item::Craft => self.craft.abrir(),
            Item::Forja => self.forja.abrir(),
            Item::Mapa if self.mapa.tem_ilha() => self.mapa.abrir(),
            Item::Mapa => {
                self.voltar_ao_menu = false;
                self.chat.push("Mapa: só nas ilhas.".into());
            }
            Item::Lojas => self.lojas.abrir(),
            Item::Mercado => {
                for pedido in self.mercado.abrir() {
                    self.envia(pedido);
                }
            }
            Item::RecuperarXp => self.morte.painel = true,
            Item::BarraItens => self.config_barra.abrir(None),
            Item::Coleta => self.config_coleta.abrir(),
            Item::Configuracoes => self.config_interface.abrir(),
            Item::Sair => {
                self.voltar_ao_menu = false;
                self.sair();
            }
            _ => self.voltar_ao_menu = false,
        }
    }

    /// Esc so' FECHA (docs/HUD.md 2.5): a janela ou o painel de cima; o que
    /// veio do Menu volta pro Menu; o Menu volta pro jogo. Nunca abre nada.
    /// Devolve `true` se fechou algo (ai' nao cancela alvo nem AUTO).
    fn esc(&mut self) -> bool {
        if self.dialogo.aberto {
            self.dialogo.fechar();
            self.auto_missao.parar();
            return true;
        }
        let do_menu = std::mem::take(&mut self.voltar_ao_menu);
        let fechou_painel = if self.config_interface.aberto {
            self.config_interface.fechar();
            true
        } else if self.config_coleta.aberto {
            self.config_coleta.fechar();
            true
        } else if self.config_barra.aberto {
            self.config_barra.fechar();
            true
        } else if self.forja.aberto() {
            self.forja.fechar();
            true
        } else if self.craft.aberto() {
            self.craft.fechar();
            true
        } else if self.morte.painel {
            self.morte.painel = false;
            true
        } else if self.mercado.aberto {
            self.mercado.fechar();
            true
        } else if self.lojas.aberto {
            self.lojas.fechar();
            true
        } else if self.diarias.aberto {
            self.diarias.fechar();
            true
        } else if self.menu_missoes.aberto {
            self.menu_missoes.aberto = false;
            true
        } else if self.mapa.aberto {
            self.mapa.aberto = false;
            true
        } else if self.missoes.aberta {
            self.missoes.fecha();
            self.auto_missao.parar();
            true
        } else if self.bolsa.aberta {
            self.bolsa.fecha();
            true
        } else {
            false
        };
        if fechou_painel {
            if do_menu {
                self.menu.abrir();
            }
            return true;
        }
        if self.loja.aberta() {
            self.loja.fecha();
            return true;
        }
        if self.menu.aberto {
            self.menu.fechar();
            return true;
        }
        false
    }

    /// Missao pronta pra entregar ou nova no Mestre: ponto vermelho em Missoes
    /// e no MENU.
    fn selo_missoes(&self) -> bool {
        let slots = &self.bolsa.slots;
        self.missoes.marcador(&|id| missoes::na_bolsa(slots, id)).is_some()
    }

    /// Diaria pra aceitar ou entregar: ponto vermelho no icone e no MENU.
    fn selo_diarias(&self) -> bool {
        let agora_unix = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_secs() as i64);
        let slots = &self.bolsa.slots;
        let tem = |id: u16| missoes::na_bolsa(slots, id);
        let c = menu_missoes::Contexto {
            log: &self.missoes.log,
            entregues: &self.quest_entregues,
            nivel: self.ficha.nivel,
            faccao: self.faccao_qid,
            zona: self.mapa.zona(),
            agora_unix,
            tem: &tem,
        };
        diarias::tem_pendente(&c)
    }

    /// A faixa de estado UNICA: aviso de skill > INDO > AUTO MISSAO >
    /// Viajando > AUTO COLETA > AUTO COMBATE.
    fn texto_da_faixa(&self) -> Option<(String, Color)> {
        let eu = self.world.self_pos();
        if let Some(a) = self.habilidades.aviso_ativo() {
            return Some((a.to_string(), hud_estilo::OURO));
        }
        if let Some(t) = self.ir_para.faixa(eu) {
            return Some((t, hud_estilo::AUTO));
        }
        if let Some(t) = self.auto_missao.faixa() {
            return Some((t, hud_estilo::OURO));
        }
        if let Some(t) = self.mapa.faixa_viagem(eu) {
            return Some((t, hud_estilo::AUTO));
        }
        if let Some(t) = self.auto_coleta.faixa(eu) {
            return Some((t.to_string(), hud_estilo::AUTO));
        }
        if let Some(t) = self.auto_combate.faixa(self.alvo.is_some()) {
            return Some((t.to_string(), hud_estilo::AUTO));
        }
        // Andando por clique, sem modo nenhum: quanto falta pelo caminho.
        rastro::restante(&self.rastro, eu?, self.mapa.viagem.destino())
            .filter(|m| *m >= 1.0)
            .map(|m| (format!("Andando · {}", rastro::formata_distancia(m)), hud_estilo::AUTO))
    }

    /// Teclas de ACAO no molde do MIR4 de PC (docs/HUD.md 2.5). Nenhuma abre
    /// painel. Z, X, 1/2/3, WASD e Espaco continuam onde ja' eram tratados.
    fn teclas_de_acao(&mut self) {
        if hud_layout::alt() && is_key_pressed(KeyCode::Enter) {
            self.tela_cheia = !self.tela_cheia;
            macroquad::window::set_fullscreen(self.tela_cheia);
        }
        if self.painel_grande() || self.dialogo.aberto {
            return;
        }
        if is_key_pressed(KeyCode::F) {
            self.atacar();
        }
        if is_key_pressed(KeyCode::Tab) {
            self.proximo_alvo();
        }
        if is_key_pressed(KeyCode::C) {
            self.usar_rapido(0);
        }
        for (i, k) in [KeyCode::Key8, KeyCode::Key9, KeyCode::Key0].into_iter().enumerate() {
            if is_key_pressed(k) {
                self.usar_rapido(i + 1);
            }
        }
    }

    /// Inimigos vivos perto, do mais perto pro mais longe.
    fn inimigos_perto(&self) -> Vec<shared::EntityId> {
        let Some(eu) = self.world.self_pos() else { return Vec::new() };
        let mut v: Vec<(f32, shared::EntityId)> = self
            .world
            .ents
            .iter()
            .filter(|(_, e)| e.meta.tag == shared::EntityTag::Enemy && e.state.hp > 0 && e.morte.is_none())
            .map(|(id, e)| (e.render_pos.distance(eu), *id))
            .filter(|(d, _)| *d <= RAIO_DE_MIRA)
            .collect();
        v.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1 .0.cmp(&b.1 .0)));
        v.into_iter().map(|(_, id)| id).collect()
    }

    /// Mira neste inimigo: e' comando novo, entao viagem e auto missao param.
    fn mirar(&mut self, id: shared::EntityId) {
        self.mapa.viagem.cancelar();
        self.auto_missao.parar();
        self.ir_para.parar();
        self.interacao.cancela();
        self.alvo = Some(id);
        self.envia(ClientMessage::SetTarget { target: Some(id) });
        self.ultima_aproximacao = 0.0;
    }

    /// F ou o botao ATACAR: com alvo vivo, vai ate' ele (o golpe e' automatico
    /// no alcance); sem alvo, escolhe o inimigo mais perto.
    fn atacar(&mut self) {
        let valido = self.alvo.and_then(|id| self.world.ents.get(&id))
            .is_some_and(|e| e.meta.tag == shared::EntityTag::Enemy && e.state.hp > 0 && e.morte.is_none());
        if valido {
            self.ultima_aproximacao = 0.0;
            return;
        }
        match self.inimigos_perto().first() {
            Some(&id) => self.mirar(id),
            None => self.chat.push("Nenhum inimigo por perto.".into()),
        }
    }

    /// Tab: o proximo inimigo perto (MIR4).
    fn proximo_alvo(&mut self) {
        let lista = self.inimigos_perto();
        if lista.is_empty() {
            return;
        }
        let i = self.alvo.and_then(|a| lista.iter().position(|x| *x == a)).map_or(0, |p| (p + 1) % lista.len());
        self.mirar(lista[i]);
    }

    /// Quanto a bolsa tem do consumivel de cada espaco da barra.
    fn qtd_rapidos(&self) -> [u32; barra::ESPACOS] {
        self.barra.espacos.map(|e| barra::quantidade(&self.bolsa.slots, e.item_id))
    }

    /// C (0) ou 8/9/0 (1..3): usa o consumivel do espaco. Vazio abre o
    /// configurador ja' com aquele espaco escolhido.
    fn usar_rapido(&mut self, i: usize) {
        self.usar_espaco(i, false);
    }

    fn usar_espaco(&mut self, i: usize, forte: bool) {
        let Some(esp) = self.barra.espacos.get(i).copied() else { return };
        if esp.item_id == 0 {
            self.fecha_paineis();
            self.config_barra.abrir(Some(i));
            return;
        }
        match barra::slot_para_usar(&self.bolsa.slots, esp.item_id, forte) {
            Some(slot) => self.envia(ClientMessage::UseItem { slot: slot as u16 }),
            None => self.chat.push(format!("Sem {} na bolsa.", self.bolsa.nome(esp.item_id))),
        }
    }

    fn salvar_barra(&mut self) {
        let espacos = self.barra.para_servidor();
        self.envia(ClientMessage::SalvarBarra { espacos });
    }

    /// Preferencias guardadas no servidor: skills AUTO, filtros do mapa e
    /// zooms. Aplica e so' entao libera a sincronia a mandar mudancas.
    fn aplica_preferencias(&mut self, p: shared::protocol::Preferencias) {
        self.habilidades.automaticas = p.skills_auto.iter().copied().collect();
        self.mapa.filtros = mapa::Filtros::from(&p.filtros_mapa);
        if let Some(a) = p.alcance_minimapa {
            self.mapa.define_alcance_minimapa(a);
        }
        if let Some(z) = p.camera_zoom {
            self.cam_zoom = z.clamp(render3d::ZOOM_MIN, render3d::ZOOM_MAX);
        }
        if let Some(a) = p.camera_pitch_ajuste {
            self.cam_pitch_ajuste = a;
        }
        if let Some(t) = p.coleta_tipos {
            self.auto_coleta.tipos = t;
        }
        if let Some(r) = p.coleta_raio {
            self.auto_coleta.raio = r;
        }
        if let Some(e) = p.escala_ui {
            hud_layout::define_escala_ui(e);
        }
        if let Some(m) = p.economia_auto_min {
            self.economia.auto_min = Some(m);
        }
        let atual = self.preferencias_atuais();
        self.prefs.recebeu(&atual);
    }

    /// O estado de agora, no formato guardado. Numeros em centesimos: a
    /// camera recalcula o ajuste todo quadro e ruido de f32 nao e' mudanca.
    fn preferencias_atuais(&self) -> shared::protocol::Preferencias {
        let cent = |v: f32| (v * 100.0).round() / 100.0;
        let mut skills: Vec<u32> = self.habilidades.automaticas.iter().copied().collect();
        skills.sort_unstable();
        shared::protocol::Preferencias {
            versao: shared::protocol::Preferencias::VERSAO,
            skills_auto: skills,
            filtros_mapa: self.mapa.filtros.para_rede(),
            alcance_minimapa: Some(cent(self.mapa.alcance_minimapa())),
            camera_zoom: Some(cent(self.cam_zoom)),
            camera_pitch_ajuste: Some(cent(self.cam_pitch_ajuste)),
            coleta_tipos: Some(self.auto_coleta.tipos),
            coleta_raio: Some(cent(self.auto_coleta.raio)),
            escala_ui: Some(cent(hud_layout::escala_ui())),
            economia_auto_min: self.economia.auto_min,
        }
    }

    /// A cada quadro: manda as preferencias quando pararam de mudar.
    fn sincroniza_preferencias(&mut self) {
        let atual = self.preferencias_atuais();
        if let Some(prefs) = self.prefs.acompanhar(&atual, get_time()) {
            self.envia(ClientMessage::SalvarPreferencias { prefs });
        }
    }

    /// Saindo ou trocando de zona: o pendente vai agora.
    fn guardar_preferencias_agora(&mut self) {
        let atual = self.preferencias_atuais();
        if let Some(prefs) = self.prefs.forcar(&atual) {
            self.envia(ClientMessage::SalvarPreferencias { prefs });
        }
    }

    /// A cada quadro: o AUTO de cada espaco da barra. Roda com painel aberto
    /// tambem — pocao nao espera o jogador fechar a bolsa.
    fn auto_da_barra(&mut self) {
        let Some(eu) = self.world.self_id.and_then(|id| self.world.ents.get(&id)) else { return };
        let st = self.bolsa.stats.as_ref();
        let mp_max = st.map_or(50, |s| s.mp_max).max(1) as f32;
        let vigor_max = st.map_or(100, |s| s.stamina_max).max(1) as f32;
        let agora_unix = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_secs() as i64);
        let estado = barra::Estado {
            vivo: eu.state.hp > 0 && eu.morte.is_none() && eu.state.flags & shared::ent_flags::DOWNED == 0 && !self.morte.morto,
            hp: eu.state.hp as f32 / eu.meta.hp_max.max(1) as f32,
            mp: self.ficha.mp.map_or(1.0, |m| m as f32 / mp_max),
            vigor: self.ficha.vigor.map_or(1.0, |v| v as f32 / vigor_max),
            xp_ativo: self.bonus_xp_ate > agora_unix,
            fortuna_ativo: self.bonus_fortuna_ate > agora_unix,
            sorte_ativo: self.bonus_sorte_ate > agora_unix,
        };
        let qtd = self.qtd_rapidos();
        if let Some((i, forte)) = self.barra.decide(&estado, &qtd, &self.bolsa.slots, get_time()) {
            self.usar_espaco(i, forte);
        }
    }

    /// A cada quadro: conduz a auto missao. Teclado ou queda pausam.
    fn conduzir_auto_missao(&mut self) {
        if !self.auto_missao.ativo() {
            return;
        }
        let morto = self.world.self_id.and_then(|id| self.world.ents.get(&id))
            .is_some_and(|e| e.state.hp == 0 || e.morte.is_some());
        if self.andando_na_mao() || morto {
            self.auto_missao.parar();
            self.chat.push(if morto { "Auto missão pausada: você caiu." } else { "Auto missão pausada." }.into());
            return;
        }
        let Some(eu) = self.world.self_pos() else { return };
        let agora = get_time();
        let id = self.auto_missao.quest.unwrap_or(0);
        let ctx = {
            let slots = &self.bolsa.slots;
            let q = self.missoes.log.iter().find(|q| q.id == id);
            auto_missao::Ctx {
                eu,
                agora,
                viajando: self.mapa.viagem.ativa(),
                pronta: q.is_some_and(|q| missoes::pronta(q, &|i| missoes::na_bolsa(slots, i))),
                na_log: q.is_some(),
                dialogo_aberto: self.dialogo.aberto,
                combate_ativo: self.auto_combate.ativo(),
                coleta_ativa: self.auto_coleta.ativo(),
            }
        };
        for acao in self.auto_missao.passo(ctx) {
            match acao {
                auto_missao::Acao::PedirDestino(q) => self.envia(ClientMessage::QuestDestino { quest_id: q }),
                auto_missao::Acao::Viajar(p) => {
                    if self.alvo.take().is_some() {
                        self.envia(ClientMessage::SetTarget { target: None });
                    }
                    self.mapa.viagem.iniciar(p, agora);
                }
                auto_missao::Acao::Interagir(npc, pos) => {
                    match self.world.ents.get(&npc).map(|e| e.render_pos) {
                        Some(p) => self.falar_com(npc, p),
                        None => self.envia(ClientMessage::MoverPara { x: pos.x, z: pos.y }),
                    }
                }
                auto_missao::Acao::LigarCombate(p) => {
                    self.mapa.viagem.cancelar();
                    self.auto_coleta.parar();
                    self.auto_combate.ligar(p);
                }
                auto_missao::Acao::LigarColeta(p) => {
                    self.auto_combate.parar();
                    // Missao de coleta: os tipos DELA, nao os da configuracao.
                    let tipos = shared::quests::quest_by_id(id).map_or([true; 5], |d| auto_coleta::tipos_da_missao(d));
                    self.auto_coleta.ligar_missao(p, tipos, agora);
                }
                auto_missao::Acao::PararAutos => {
                    self.auto_combate.parar();
                    self.auto_coleta.parar();
                    if self.alvo.take().is_some() {
                        self.envia(ClientMessage::SetTarget { target: None });
                    }
                }
                auto_missao::Acao::Aviso(s) => self.chat.push(s),
            }
        }
    }

    /// X (ou o botao): auto coleta. Teclado, Esc ou Z desligam. Tambem conduz
    /// a coleta manual (clique numa pedra/tronco) e vira o boneco pro no'.
    fn atualizar_auto_coleta(&mut self) {
        let agora = get_time();
        // Desligou (aqui ou em qualquer outro lugar): a coleta em curso para.
        if self.coleta_auto_estava && !self.auto_coleta.ativo() && self.coleta_pendente.is_none() {
            self.envia(ClientMessage::PararColeta);
        }
        self.coleta_auto_estava = self.auto_coleta.ativo();
        // Coletando: olha pro no'.
        if let (Some(c), Some(id)) = (self.coleta_hud.centro, self.world.self_id) {
            if let Some(e) = self.world.ents.get_mut(&id) {
                e.mira = Some((c, 0.3));
            }
        }
        let movimento = self.andando_na_mao();
        self.acompanhar_coleta_manual(movimento);
        // O botao: toque CURTO liga/desliga; SEGURAR ou a engrenagem no canto
        // abrem a configuracao (sem botao direito no toque). Liga no SOLTAR, e
        // nao no apertar, senao todo toque longo ligaria o AUTO antes de abrir.
        let mouse = Vec2::from(mouse_position());
        let livre = !self.painel_grande();
        let na_engrenagem = livre && auto_coleta::engrenagem().contains(mouse);
        if na_engrenagem && is_mouse_button_pressed(MouseButton::Left) {
            self.fecha_paineis();
            self.config_coleta.abrir();
        }
        let sobre_botao = (livre && auto_coleta::pega_mouse() && !na_engrenagem).then_some(0);
        let toque = self.toque_coleta.quadro(
            is_mouse_button_pressed(MouseButton::Left),
            is_mouse_button_down(MouseButton::Left),
            is_mouse_button_released(MouseButton::Left),
            sobre_botao,
            mouse,
            get_time(),
        );
        if matches!(toque, toque::Toque::Longo(_)) {
            self.fecha_paineis();
            self.config_coleta.abrir();
        }
        let alterna = is_key_pressed(KeyCode::X) || matches!(toque, toque::Toque::Curto(_));
        let esc = is_key_pressed(KeyCode::Escape) && !self.esc_consumido;
        if self.auto_coleta.ativo() && (movimento || alterna || esc || is_key_pressed(KeyCode::Z)) {
            self.auto_coleta.parar();
            self.mapa.viagem.cancelar();
            if alterna {
                self.auto_missao.parar();
            }
            return;
        }
        if alterna {
            self.auto_combate.parar();
            self.auto_missao.parar();
            self.ir_para.parar();
            self.mapa.viagem.cancelar();
            if self.alvo.take().is_some() {
                self.envia(ClientMessage::SetTarget { target: None });
            }
            if let Some(p) = self.world.self_pos() {
                self.auto_coleta.ligar(p, agora);
            }
        }
        if !self.auto_coleta.ativo() {
            return;
        }
        let Some(eu) = self.world.self_pos() else { return };
        match self.auto_coleta.passo(eu, agora, self.mapa.viagem.ativa()) {
            auto_coleta::Acao::PedirNo { tipos, raio, centro } => {
                self.envia(ClientMessage::PedirNoDeColeta { tipos, raio, centro: [centro.x, centro.y] });
            }
            // Veio do mapa ("Ir" numa regiao): so' aquele tipo, em volta dela.
            auto_coleta::Acao::PedirNoDoTipo { tipo, perto } => {
                self.envia(ClientMessage::PedirSpotDeColetaDe { tipo, perto: [perto.x, perto.y] });
            }
            auto_coleta::Acao::Ir(p) => self.mapa.viagem.iniciar(p, agora),
            auto_coleta::Acao::Coletar(coluna) => self.envia(ClientMessage::ColetarNo { coluna }),
            auto_coleta::Acao::Nada => {}
        }
    }

    /// Clique numa pedra/tronco: anda ate' o alcance e manda coletar ao
    /// chegar. Teclado ou ligar o AUTO desistem.
    fn acompanhar_coleta_manual(&mut self, movimento: bool) {
        let Some((coluna, centro, raio)) = self.coleta_pendente else { return };
        if movimento || self.auto_coleta.ativo() {
            self.coleta_pendente = None;
            return;
        }
        let Some(eu) = self.world.self_pos() else { return };
        if eu.distance(centro) - raio - shared::ENTITY_RADIUS <= shared::COLETA_ALCANCE_UN {
            self.envia(ClientMessage::ColetarNo { coluna });
            self.coleta_pendente = None;
        }
    }

    /// Clicou no mapa ou no minimapa: viaja ate' la'. Desliga o auto combate e
    /// solta o alvo — senao "ir ate' o alvo" pede rota por cima da viagem.
    fn iniciar_viagem(&mut self, destino: Vec2) {
        if !self.mapa.terra(destino) {
            self.chat.push("mapa: lá é água".into());
            return;
        }
        self.auto_combate.parar();
        self.ir_para.parar();
        self.interacao.cancela();
        self.loja.fecha();
        self.missoes.fecha();
                self.auto_missao.parar();
                self.dialogo.fechar();
        if self.alvo.take().is_some() {
            self.envia(ClientMessage::SetTarget { target: None });
        }
        self.mapa.viagem.iniciar(destino, get_time());
    }

    /// "Ir" do mapa (zona de bicho, regiao de recurso) ou do menu de missoes
    /// (ir ao Mestre). Encerra o que brigaria pela rota.
    fn iniciar_ir_para(&mut self, alvo: ir_para::Alvo) {
        self.mapa.viagem.cancelar();
        self.auto_combate.parar();
        self.auto_coleta.parar();
        self.auto_missao.parar();
        self.interacao.cancela();
        self.loja.fecha();
        self.missoes.fecha();
        self.dialogo.fechar();
        self.menu_missoes.aberto = false;
        self.mapa.aberto = false;
        if self.alvo.take().is_some() {
            self.envia(ClientMessage::SetTarget { target: None });
        }
        self.ir_para.iniciar(alvo, get_time());
    }

    /// A cada quadro: conduz o "Ir". Teclado ou queda encerram.
    fn conduzir_ir_para(&mut self) {
        if !self.ir_para.ativo() {
            return;
        }
        let morto = self.world.self_id.and_then(|id| self.world.ents.get(&id))
            .is_some_and(|e| e.state.hp == 0 || e.morte.is_some());
        if morto || self.andando_na_mao() {
            self.ir_para.parar();
            self.mapa.viagem.cancelar();
            return;
        }
        let Some(eu) = self.world.self_pos() else { return };
        let agora = get_time();
        let Some(acao) = self.ir_para.passo(eu, agora, self.mapa.viagem.ativa()) else { return };
        match acao {
            ir_para::Acao::Viajar(p) => self.mapa.viagem.iniciar(p, agora),
            ir_para::Acao::LigarCombate(p) => {
                self.mapa.viagem.cancelar();
                self.auto_coleta.parar();
                self.auto_combate.ligar(p);
            }
            ir_para::Acao::LigarColeta(tipo, p) => {
                self.mapa.viagem.cancelar();
                self.auto_combate.parar();
                self.auto_coleta.ligar(p, agora);
                self.auto_coleta.filtro = Some((tipo, p));
            }
            ir_para::Acao::FalarPerto(p) => {
                let npc = self.world.ents.iter()
                    .filter(|(_, e)| e.meta.tag == shared::EntityTag::Npc && e.render_pos.distance(p) <= 8.0)
                    .min_by(|a, b| a.1.render_pos.distance_squared(p).total_cmp(&b.1.render_pos.distance_squared(p)))
                    .map(|(id, e)| (*id, e.render_pos));
                match npc {
                    Some((id, pos)) => self.falar_com(id, pos),
                    None => self.chat.push("Não achei o NPC aqui.".into()),
                }
            }
            ir_para::Acao::Aviso(s) => self.chat.push(s),
        }
    }

    /// Clique no menu de todas as missoes.
    fn clique_menu_missoes(&mut self, c: menu_missoes::Clique) {
        match c {
            menu_missoes::Clique::AutoMissao(id) => {
                self.menu_missoes.aberto = false;
                self.diarias.fechar();
                self.iniciar_auto_missao(id);
            }
            menu_missoes::Clique::IrAoGiver(_) => {
                self.diarias.fechar();
                let nome = shared::construcao::Papel::Missoes.nome();
                let mestre = self.mapa.mestre.or_else(|| {
                    self.world.ents.values()
                        .find(|e| e.meta.tag == shared::EntityTag::Npc && e.meta.name.as_deref() == Some(nome))
                        .map(|e| e.render_pos)
                });
                match mestre {
                    Some(p) => self.iniciar_ir_para(ir_para::Alvo {
                        objetivo: ir_para::Objetivo::Npc,
                        pos: p,
                        raio: 0.0,
                        rotulo: nome.to_string(),
                    }),
                    None => self.chat.push("Não sei onde fica o Mestre de Missões.".into()),
                }
            }
            menu_missoes::Clique::Aviso(s) => self.chat.push(s),
        }
    }

    /// A cada quadro: pede a proxima etapa. Teclado ou morte encerram.
    fn conduzir_viagem(&mut self) {
        if !self.mapa.viagem.ativa() {
            return;
        }
        let morto = self.world.self_id.and_then(|id| self.world.ents.get(&id))
            .is_some_and(|e| e.state.hp == 0 || e.morte.is_some());
        if morto || self.andando_na_mao() {
            self.mapa.viagem.cancelar();
            return;
        }
        let Some(eu) = self.world.self_pos() else { return };
        // A viagem sai do mapa pelo tempo da conta: ela quer `&mut`, e o teste
        // de terra quer o mapa inteiro.
        let mut viagem = std::mem::take(&mut self.mapa.viagem);
        let passo = viagem.passo(eu, get_time(), |p| self.mapa.terra(p));
        self.mapa.viagem = viagem;
        match passo {
            mapa::Passo::Enviar(p) => self.envia(ClientMessage::MoverPara { x: p.x, z: p.y }),
            mapa::Passo::Desistiu => self.chat.push("viagem: caminho bloqueado".into()),
            mapa::Passo::Chegou | mapa::Passo::Nada => {}
        }
    }

    /// Skills ofensivas usam a selecao enviada por SetTarget.
    fn usar_habilidade(&mut self) {
        if self.painel_grande() { self.habilidades.cancela_arrasto(); return; }
        let conjunto = shared::skills::Conjunto::da_arma(self.bolsa.equip.weapon.unwrap_or(0));
        let Some(eu)=self.world.self_id.and_then(|id|self.world.ents.get(&id)) else {return};
        let distancia_alvo=self.alvo.and_then(|id|self.world.ents.get(&id)).filter(|e|e.state.hp>0 && e.morte.is_none()).map(|e|eu.render_pos.distance(e.render_pos));
        let contexto=habilidades::Contexto{conjunto,nivel:self.ficha.nivel,mp:self.ficha.mp.unwrap_or(0),
            vivo:eu.state.hp>0 && eu.state.flags & shared::ent_flags::DOWNED == 0,
            vida_baixa:eu.state.hp as f32/(eu.meta.hp_max.max(1) as f32)<0.85,distancia_alvo};
        let Some(id) = self.habilidades.pedido(contexto) else { return };
        self.envia(ClientMessage::SkillCast { skill_id: id });
    }

    fn atualizar_auto_combate(&mut self) {
        let movimento=self.andando_na_mao();
        let clique_mundo=self.clique_no_mundo() && !self.ui_pega_mouse();
        // O botao so' existe com o HUD a' mostra; a tecla Z vale sempre. Painel
        // aberto NAO desliga o AUTO (MIR4: o menu aberto nao para o combate).
        let alterna=is_key_pressed(KeyCode::Z) || (!self.painel_grande() && auto_combate::pega_mouse() && is_mouse_button_pressed(MouseButton::Left));
        let esc=is_key_pressed(KeyCode::Escape) && !self.esc_consumido;
        if self.auto_combate.ativo() && (esc || alterna) {
            self.auto_combate.parar();
            // Desligar o combate a mao desliga a auto missao que dependia dele.
            if alterna || esc {
                self.auto_missao.parar();
            }
            self.alvo=None;self.envia(ClientMessage::SetTarget{target:None});
            if let Some(p)=self.world.self_pos() {self.envia(ClientMessage::MoverPara{x:p.x,z:p.y});}
            return;
        }
        if alterna {
            // Ligar o auto combate encerra a viagem (os dois pedem rota), a auto
            // coleta (exclusivas) e a auto missao.
            self.mapa.viagem.cancelar();
            self.auto_coleta.parar();
            self.auto_missao.parar();
            self.ir_para.parar();
            if let Some(p)=self.world.self_pos() {self.auto_combate.ligar(p);}
        }
        if !self.auto_combate.ativo() {return;}
        let agora=get_time();
        // Andar nao desliga: teclado, ou clique no CHAO (clique em bicho so'
        // troca o alvo, e o AUTO segue com ele).
        if movimento || (clique_mundo && self.alvo.is_none()) {
            self.auto_combate.andar_manual(agora);
        }
        let Some(eu)=self.world.self_pos() else {return};
        if self.auto_combate.segurando(eu,agora) {return;}
        let novo=self.auto_combate.escolher(&self.world,self.alvo,agora);
        if novo!=self.alvo {
            self.alvo=novo;self.envia(ClientMessage::SetTarget{target:novo});
            if novo.is_none() {
                if let Some(p)=self.world.self_pos() {self.envia(ClientMessage::MoverPara{x:p.x,z:p.y});}
            }
        }
    }

    /// Com um inimigo selecionado e fora do alcance, anda ate ele.
    fn ir_ate_o_alvo(&mut self) {
        if self.habilidades.ocupada() { return; }
        let Some(alvo) = self.alvo else { return };
        if self.andando_na_mao() {
            return;
        }
        let eu = self.world.self_id.and_then(|i| self.world.ents.get(&i));
        let (Some(eu), Some(ele)) = (eu, self.world.ents.get(&alvo)) else { return };
        if ele.state.hp == 0 {
            return;
        }
        let conjunto = shared::skills::Conjunto::de_u8(shared::components::acao::conjunto(eu.state.acao))
            .unwrap_or(shared::skills::Conjunto::EspadaEscudo);
        let alcance = if conjunto.a_distancia() { shared::RANGED_ATTACK_RANGE } else { shared::MELEE_RANGE };
        let (a, b) = (eu.render_pos, ele.render_pos);
        if a.distance(b) <= alcance * 0.9 {
            return;
        }
        let agora = get_time();
        if agora - self.ultima_aproximacao < 0.35 {
            return;
        }
        self.ultima_aproximacao = agora;
        let destino = b + (a - b).normalize_or_zero() * (alcance * 0.6);
        self.envia(ClientMessage::MoverPara { x: destino.x, z: destino.y });
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
        self.guardar_preferencias_agora();
        self.prefs = preferencias::Sincronia::default();
        self.auto_combate.parar();
        self.loja.fecha();
        self.interacao.cancela();
        self.missoes.limpa();
        self.auto_missao.parar();
        self.auto_coleta.parar();
        self.dialogo.fechar();
        self.ultimo_npc = None;
        let filtros = std::mem::take(&mut self.mapa.filtros);
        self.mapa = mapa::Mapa::default();
        self.mapa.filtros = filtros;
        self.ir_para.parar();
        self.menu_missoes.aberto = false;
        self.diarias.fechar();
        self.quest_entregues.clear();
        self.construcoes = construcoes::Construcoes::default();
        self.rastro.limpa();
        self.menu.fechar();
        self.lojas.fechar();
        self.voltar_ao_menu = false;
        self.personagem_atual=None;
        self.token_login = None;
        self.selecao_personagem=personagens::Personagens::default();
        self.net = None;
        self.world = World::default();
        self.ganhos = ganhos::Ganhos::default();
        self.map = None;
        self.terreno = None;
        self.alvo = None;
        self.info = hud::Info::default();
        self.rede = hud::Rede::default();
        self.chat.clear();
        self.bolsa = bolsa::Bolsa::default();
        self.ficha = hud::Ficha::default();
        self.teclado.limpa();
        // Volta pro login e nao pra escolha de servidor: o canal continua
        // sendo o mesmo, quem mudou de ideia foi a conta.
        self.tela = Tela::Login;
    }

    /// Giro, inclinacao e zoom.
    ///
    /// Arrastar com o DIREITO ou com o do meio move a camera: horizontal
    /// gira, vertical inclina. Q/E giram, R/F inclinam, a roda aproxima.
    ///
    /// O direito acumula dois sentidos porque e' onde a mao ja' esta': parado
    /// ele defende, arrastado ele e' camera. O esquerdo fica so' com a mira —
    /// esse nao da' pra dividir, porque clique de mira e arrasto de camera
    /// acontecem no mesmo instante e brigariam pelo alvo.
    /// A altura pra onde a camera olha persegue o CORPO, nao o chao.
    ///
    /// Olhar pro apoio deixava o pulo invisivel na camera — o boneco subia e
    /// o enquadramento ficava. E olhar pro corpo sem atraso poria a camera
    /// pra cima e pra baixo junto com cada degrau. Perseguir com atraso
    /// resolve os dois: o degrau some, o pulo levanta a camera um pouco.
    fn seguir_altura(&mut self) {
        let alvo = self
            .world
            .self_id
            .and_then(|id| self.world.ents.get(&id))
            .map(|e| e.render_y)
            .or_else(|| {
                // Sem corpo ainda (entrando no mundo): o chao serve.
                let centro = self.world.self_pos()?;
                let t = self.terreno.as_ref()?;
                Some(t.altura_apoio(centro.x, centro.y, shared::ENTITY_RADIUS))
            });
        let Some(alvo) = alvo else { return };
        (self.cam_altura, self.cam_altura_vel) = render3d::altura_da_camera(
            self.cam_altura,
            self.cam_altura_vel,
            alvo,
            get_frame_time(),
        );
    }

    /// Le os dedos deste quadro e decide o gesto (ver `gesto_camera`).
    fn ler_toques(&mut self) {
        let brutos = touches();
        // Um pouco depois do ultimo dedo o mouse simulado da macroquad ainda
        // conta como toque: aperto de mouse "sozinho" nesse meio tempo e' eco
        // do dedo, nao clique no mundo.
        if !brutos.is_empty() {
            self.ultimo_toque = get_time();
        }
        self.toque_ativo = !brutos.is_empty() || self.gesto_camera.ativo() || get_time() - self.ultimo_toque < 0.3;
        let toques: Vec<gesto_camera::ToqueNoQuadro> = brutos
            .iter()
            .map(|t| gesto_camera::ToqueNoQuadro {
                id: t.id,
                fase: match t.phase {
                    TouchPhase::Started => gesto_camera::Fase::Comecou,
                    TouchPhase::Ended | TouchPhase::Cancelled => gesto_camera::Fase::Acabou,
                    _ => gesto_camera::Fase::Segurando,
                },
                pos: t.position,
            })
            .collect();
        // Joystick primeiro: um dedo que COMECA na metade esquerda de baixo,
        // fora de botao/painel, e' dele — e some da lista da camera e do
        // clique. Painel grande aberto: sem joystick.
        let z = hud_layout::atual();
        let painel = self.painel_grande();
        if painel {
            self.joystick.soltar();
        }
        let so_um_dedo = toques.len() == 1;
        let sobre_ui = self.ui_pega_mouse();
        let pode_comecar = |p: Vec2| {
            !painel && z.joystick.contains(p) && !z.contem(p) && !(so_um_dedo && sobre_ui)
        };
        let dono = self.joystick.dedo();
        self.joystick.quadro(&toques, joystick::RAIO_BASE * z.s, &pode_comecar);
        // O de antes tambem sai: no quadro do soltar o joystick ja' largou o id.
        let resto = joystick::sem_dedos(&toques, &[dono, self.joystick.dedo()]);
        // O HUD e' conferido no ponto do dedo (a macroquad ja' levou o mouse
        // simulado pra la').
        let sobre_hud = !resto.is_empty() && sobre_ui;
        self.toque_acao = self.gesto_camera.quadro(&resto, sobre_hud);
    }

    /// Andar "na mao": WASD/setas ou o joystick virtual. E' o que pausa auto
    /// missao, viagem, ir-para e a ida ate' o NPC.
    fn andando_na_mao(&self) -> bool {
        let teclas = [KeyCode::W, KeyCode::A, KeyCode::S, KeyCode::D, KeyCode::Up, KeyCode::Down, KeyCode::Left, KeyCode::Right]
            .iter()
            .any(|k| is_key_down(*k));
        joystick::movimento_manual(teclas, &self.joystick)
    }

    /// Clique no MUNDO neste quadro: com dedo, so' o toque curto no soltar;
    /// sem dedo, o aperto do botao esquerdo como sempre.
    fn clique_no_mundo(&self) -> bool {
        match self.toque_acao {
            gesto_camera::Acao::Clique(_) => true,
            _ => !self.toque_ativo && is_mouse_button_pressed(MouseButton::Left),
        }
    }

    fn pos_do_clique(&self) -> (f32, f32) {
        match self.toque_acao {
            gesto_camera::Acao::Clique(p) => (p.x, p.y),
            _ => mouse_position(),
        }
    }

    fn camera_controles(&mut self) {
        let dt = get_frame_time().min(0.1);
        // Tudo abaixo mexe no ALVO; a camera persegue no fim (camera_suave).
        // Valor mudado por fora (preferencia carregada) vira o alvo.
        self.camera_suave.sincroniza(self.cam_yaw, self.cam_pitch_ajuste, self.cam_zoom);
        let mut dyaw = 0.0f32;
        let mut dzoom = 0.0f32;
        if is_key_down(KeyCode::Q) {
            dyaw -= 2.2 * dt;
        }
        if is_key_down(KeyCode::E) {
            dyaw += 2.2 * dt;
        }
        // R e F sairam da camera: F ataca e R fica reservado pro golpe letal
        // (MIR4). Inclinar e' o arrasto do botao do meio (ou do direito) —
        // horizontal gira, vertical inclina — e a roda aproxima.
        let mut mexeu = 0.0f32;
        let (mx, my) = mouse_position();
        // O botao do meio e' sempre camera. O DIREITO tem dois sentidos:
        // parado ele defende, arrastado ele move a camera — e quem separa e'
        // o movimento, nao um modo. E' assim que MMO faz, e evita gastar
        // outra tecla numa acao que o jogador ja' procura no direito.
        if is_mouse_button_pressed(MouseButton::Right) {
            self.arrasto_de = vec2(mx, my);
            self.arrasto_virou_camera = false;
        }
        if is_mouse_button_down(MouseButton::Right)
            && (vec2(mx, my) - self.arrasto_de).length() > render3d::ARRASTO_MINIMO
        {
            self.arrasto_virou_camera = true;
        }
        if !is_mouse_button_down(MouseButton::Right) {
            self.arrasto_virou_camera = false;
        }
        if is_mouse_button_down(MouseButton::Middle) || self.arrasto_virou_camera {
            let p = vec2(mx, my);
            if let Some(anterior) = self.arrasto {
                dyaw += (p.x - anterior.x) * 0.008;
                // Arrastar pra baixo LEVANTA a camera. E' a leitura de quem
                // esta' com o mundo na mao e nao com a cabeca: puxar o chao
                // pra baixo e' olhar de mais alto.
                mexeu += (p.y - anterior.y) * 0.004;
            }
            self.arrasto = Some(p);
        } else {
            self.arrasto = None;
        }
        // Toque: um dedo arrastando no mundo gira com a MESMA sensibilidade do
        // arrasto do mouse; a pinca aproxima/afasta na faixa da roda.
        // O delta do dedo passa por uma media curta: no iPhone os eventos
        // chegam em ritmo irregular, e somado cru a camera anda aos degraus.
        match self.toque_acao {
            gesto_camera::Acao::Gira(d) => {
                let f = self.camera_suave.filtro.filtra(d, dt);
                dyaw += f.x * 0.008;
                mexeu += f.y * 0.004;
                self.camera_suave.inercia.para();
                self.girando_toque = true;
            }
            gesto_camera::Acao::Zoom(d) if !self.painel_grande() => {
                // Abrir os dedos aproxima, como a roda pra cima.
                dzoom -= d * 0.004;
            }
            _ => {
                if self.girando_toque {
                    if self.gesto_camera.ativo() {
                        // Dedo parado na tela: a media esvazia sozinha.
                        let f = self.camera_suave.filtro.filtra(Vec2::ZERO, dt);
                        dyaw += f.x * 0.008;
                        mexeu += f.y * 0.004;
                    } else {
                        // Soltou: sobra a inercia do arrasto.
                        let v = self.camera_suave.filtro.velocidade();
                        self.camera_suave.inercia.solta(vec2(v.x * 0.008, v.y * 0.004));
                        self.camera_suave.filtro.zera();
                        self.girando_toque = false;
                    }
                }
            }
        }
        let embalo = self.camera_suave.inercia.passo(dt);
        dyaw += embalo.x;
        mexeu += embalo.y;
        let (_, roda) = mouse_wheel();
        // Roda em cima do mapa e' zoom do MAPA, nao da camera.
        if roda != 0.0 && !self.mapa.pega_mouse() && !self.painel_grande() {
            dzoom -= roda.signum() * 0.12;
        }

        // ── alvos ──
        let cs = &mut self.camera_suave;
        cs.yaw += dyaw;
        if cs.yaw > std::f32::consts::PI {
            cs.yaw -= std::f32::consts::TAU;
        } else if cs.yaw < -std::f32::consts::PI {
            cs.yaw += std::f32::consts::TAU;
        }
        cs.zoom = (cs.zoom + dzoom).clamp(render3d::ZOOM_MIN, render3d::ZOOM_MAX);
        // O ajuste do alvo ja' nasce dentro da banda do zoom do alvo, senao a
        // mao empurra um desvio que a camera nunca alcanca.
        let base_alvo = render3d::pitch_do_zoom(cs.zoom);
        let piso_alvo = render3d::pitch_min_para(cs.zoom);
        cs.ajuste = (base_alvo + cs.ajuste + mexeu).clamp(piso_alvo, render3d::PITCH_MAX) - base_alvo;

        // ── a camera persegue ──
        let (yaw, ajuste, zoom) = cs.persegue(self.cam_yaw, self.cam_pitch_ajuste, self.cam_zoom, dt);
        self.cam_yaw = yaw;
        self.cam_pitch_ajuste = ajuste;
        self.cam_zoom = zoom;
        // A roda escolhe o enquadramento; o ajuste da mao vai por cima. O
        // recorte e' no fim, com a banda daquele zoom: afastar EMPURRA a
        // camera pra cima em vez de recusar o zoom — recusar seria a roda
        // parar de responder sem explicacao nenhuma na tela.
        let base = render3d::pitch_do_zoom(self.cam_zoom);
        let piso = render3d::pitch_min_para(self.cam_zoom);
        self.cam_pitch = (base + self.cam_pitch_ajuste).clamp(piso, render3d::PITCH_MAX);
        // Devolve o ajuste recortado, senao a mao acumula um desvio invisivel
        // e a camera fica surda por meia volta de roda.
        self.cam_pitch_ajuste = self.cam_pitch - base;
        // Mantem o angulo em [-pi, pi]: sem isso ele cresce sem limite e a
        // precisao do f32 come a suavidade depois de uns minutos girando.
        if self.cam_yaw > std::f32::consts::PI {
            self.cam_yaw -= std::f32::consts::TAU;
        } else if self.cam_yaw < -std::f32::consts::PI {
            self.cam_yaw += std::f32::consts::TAU;
        }
        self.camera_suave.escreveu(self.cam_yaw, self.cam_pitch_ajuste, self.cam_zoom);
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
        // Joystick virtual: mesma direcao de TELA do WASD, com intensidade
        // (o servidor so' normaliza acima de 1 — empurrao leve anda devagar).
        let joy = self.joystick.direcao();
        if joy != Vec2::ZERO {
            dir = joy;
        }
        // "Pra frente" e' longe da camera, nao o norte do mundo. A conta e'
        // aqui; o que sai no fio continua sendo direcao em espaco de mundo.
        dir = render3d::input_para_mundo(dir, self.cam_yaw);
        let mut buttons = 0u32;
        // Direito parado defende; direito arrastando e' camera, e ai' ele
        // NAO defende — girar a vista nao pode levantar o escudo.
        if is_mouse_button_down(MouseButton::Right) && !self.arrasto_virou_camera {
            buttons |= shared::protocol::buttons::SECONDARY;
        }
        // Shift segurado corre: o servidor multiplica a velocidade
        // (`SPRINT_SPEED_MULT`) e gasta vigor. A animacao de correr nao olha
        // a tecla — sai da velocidade, igual pra quem esta' de fora.
        // Indo sozinho ha' um tempo (viagem, auto missao, rota por clique), o
        // bit vai ligado igual — ver `corrida`.
        if is_key_down(KeyCode::LeftShift) || is_key_down(KeyCode::RightShift) || self.correndo_auto {
            buttons |= shared::protocol::buttons::SPRINT;
        }
        // O servidor detecta a BORDA de subida; aqui basta mandar o estado.
        if is_key_down(KeyCode::Space) {
            buttons |= shared::protocol::buttons::PULO;
        }
        // O arco comeca no quadro da tecla, sem esperar a ida e volta. So' a
        // ANIMACAO: quem decide se o degrau de dois blocos foi vencido e' o
        // servidor, e ele responde antes de o arco chegar ao topo.
        if is_key_pressed(KeyCode::Space) {
            self.world.pular_local();
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
            Tela::Jogando if self.economia.ativa => self.desenhar_economia(),
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
        let centro = self.world.self_pos().unwrap_or(Vec2::ZERO)
            + chefe_anim::sacudida(self.tremor.0, self.tremor.1, get_time());
        // A camera sobe com o chao. Presa em zero, o jogador some dentro do
        // morro assim que o terreno passou a ter 34 unidades de altura.
        let terreno = self.terreno.as_ref();
        let f = |x: f32, z: f32| terreno.map_or(0.0, |t| t.altura(x, z));
        let vista = render3d::Vista::nova(
            centro, self.cam_yaw, self.cam_zoom, self.cam_pitch, self.cam_altura, &f,
        );
        set_camera(&vista.cam);
        // Tudo que e' mundo — chao, vegetacao, bichos — vai com descarte de
        // face de costas. O HUD volta pro material padrao no fim, porque ele
        // e' 2D e nao tem lado de tras.
        gl_use_material(&self.solido);
        // ── VER O PERSONAGEM ATRAVES DO QUE ESTA' NA FRENTE ──
        //
        // O furo acompanha o jogador e so' vale pro que estiver a' frente
        // dele — e so' pro que for alto o bastante pra esconder alguem.
        let (recorte, corte_z) = match self.world.self_id.and_then(|id| self.world.ents.get(&id)) {
            Some(e) => render3d::recorte_do_jogador(
                &vista.cam,
                vista.pos_de(e),
                // Fracao da ALTURA APARENTE do boneco, nao da tela: e' o que
                // mantem o furo do mesmo tamanho relativo em qualquer zoom.
                0.85,
                vec2(screen_width(), screen_height()),
            ),
            None => (Vec3::ZERO, 0.0),
        };
        self.solido.set_uniform("Recorte", recorte);
        self.solido.set_uniform("RecorteZ", corte_z);
        match &self.terreno {
            Some(t) => {
                self.pedacos_desenhados = t.desenha(&vista.cam);
                // Casas no mesmo passe: descarte de face e recorte da camera
                // valem pra elas como valem pra arvore.
                let jogador = self.world.self_pos().map(|p| vec3(p.x, t.altura(p.x, p.y), p.y));
                self.construcoes.desenha(&vista.cam, jogador);
                // Pra onde esta' indo: tracejado rente ao chao, no mesmo passe.
                if let Some(eu) = self.world.self_pos() {
                    let altura = |x: f32, z: f32| t.altura(x, z);
                    rastro::desenha(&self.rastro, eu, self.mapa.viagem.destino(), &altura, get_time() as f32);
                }
                // Golpe de chefe carregando: onde vai bater, crescendo ate' o impacto.
                {
                    let altura = |x: f32, z: f32| t.altura(x, z);
                    self.telegrafos.desenha(&altura, get_time());
                }
                // O mar por cima do leito, com material proprio (onda no
                // shader); depois o solido volta pro resto do mundo.
                agua::desenha(t, &vista.cam, get_time() as f32);
                gl_use_material(&self.solido);
            }
            None => {
                if let Some(m) = &self.map {
                    render3d::draw_ground(m, centro);
                }
            }
        }
        // A MESMA vista da mira: desenho e clique nao tem como divergir
        // porque nao existe a segunda conta.
        // O furo vale so' pro CENARIO. Bicho nao e' obstaculo: ele e' o que
        // se olha, e cortar um circulo no meio do lobo que esta' te atacando
        // esconde exatamente o que precisa ser lido — de que lado ele vem,
        // se ja' levantou o golpe, quanta vida sobrou.
        //
        // Desligar por UNIFORME e nao por material: o material carrega o
        // descarte de face de costas, que os bichos tambem precisam. Dois
        // materiais seriam dois lugares pra a configuracao divergir.
        self.solido.set_uniform("Recorte", Vec3::ZERO);
        render3d::draw_entities(&mut self.world, &self.vox, self.alvo, &vista);
        gl_use_default_material();
        self.habilidades.desenha_efeitos(&self.world, &vista);
        set_default_camera();
        // Numero de dano, faisca e a borda vermelha, por cima do mundo.
        efeitos::desenha(&self.world, &vista);
        // Quanto falta andar, sobre o destino.
        if let (Some(t), Some(eu)) = (&self.terreno, self.world.self_pos()) {
            let altura = |x: f32, z: f32| t.altura(x, z);
            rastro::desenha_distancia(&self.rastro, eu, self.mapa.viagem.destino(), &altura, &vista.cam);
        }
        if let Some(e) = self.world.self_id.and_then(|i| self.world.ents.get(&i)) {
            let bolsa = &self.bolsa;
            self.ganhos.desenha(&vista.cam, vista.pos_de(e), |id| bolsa.nome(id));
        }
        {
            let slots = &self.bolsa.slots;
            self.missoes.desenha_marcador(&self.world, &vista, &|id| missoes::na_bolsa(slots, id));
        }
        // ── HUD no molde do MIR4 (docs/HUD.md). Posicoes em `hud_layout`. ──
        let z = hud_layout::atual();
        let painel = self.painel_grande();
        let agora_unix = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_secs() as i64);
        let eu = self.world.self_id.and_then(|id| self.world.ents.get(&id))
            .map(|e| (e.state.hp as i32, e.meta.hp_max as i32, e.meta.nivel as u32, e.meta.name.clone().unwrap_or_default()));
        let nivel = match &eu {
            Some((_, _, n, _)) if self.ficha.nivel == 0 => *n,
            _ => self.ficha.nivel,
        };
        // Painel grande aberto: o HUD some, como no MIR4 — fica a EXP e a faixa.
        if !painel {
            if hud::draw_hud(
                &z,
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
                self.fecha_paineis();
                self.mapa.abrir();
            }
            self.mapa.desenha_mini(&self.world);
            if hud::draw_botao_economia(&z) {
                self.entrar_economia();
            }
            if let Some((hp, hp_max, _, nome)) = eu.clone() {
                let st = self.bolsa.stats.as_ref();
                let (hp_max, mp_max, vigor_max, poder) =
                    (st.map_or(hp_max, |s| s.hp_max), st.map_or(50, |s| s.mp_max), st.map_or(100, |s| s.stamina_max), st.map(crate::bolsa::poder));
                hud::draw_ficha(&z, &mut self.ficha, get_frame_time(), &nome, nivel, hp, hp_max, mp_max, vigor_max, poder);
                hud::draw_buffs(self.bonus_xp_ate, self.bonus_fortuna_ate, self.bonus_sorte_ate, agora_unix, Rect::new(z.buffs.x, z.buffs.y - 6.0, z.buffs.w, 0.0), self.barra.curas(get_time()));
                let conjunto = shared::skills::Conjunto::da_arma(self.bolsa.equip.weapon.unwrap_or(0));
                self.habilidades.barra(conjunto, nivel, self.ficha.mp.unwrap_or(0));
                self.auto_combate.desenha();
                self.auto_coleta.desenha();
                if hud::draw_atacar(&z, self.alvo.is_some()) {
                    self.atacar();
                }
                {
                    let bolsa = &self.bolsa;
                    let itens = self.barra.espacos.map(|e| e.item_id);
                    let auto = self.barra.espacos.map(|e| e.auto);
                    let agora_s = get_time();
                    let recarga = std::array::from_fn(|i| self.barra.recarga(i, agora_s));
                    let curando = std::array::from_fn(|i| self.barra.curando(i, agora_s));
                    hud::draw_rapidos(&z, itens, self.qtd_rapidos(), auto, recarga, curando, self.barra.arrastando(), &|id| bolsa.nome(id));
                }
                // Barra de itens: clique usa, arrastar ↑/↓ liga/desliga o AUTO
                // do espaco, botao direito abre o configurador.
                let rects = hud::rects_rapidos(&z);
                let mouse = Vec2::from(mouse_position());
                if is_mouse_button_pressed(MouseButton::Right) {
                    if let Some(i) = rects.iter().position(|r| r.contains(mouse)) {
                        self.fecha_paineis();
                        self.config_barra.abrir(Some(i));
                    }
                }
                // Botao direito no AUTO COLETA: o que coletar e o raio.
                if is_mouse_button_pressed(MouseButton::Right) && auto_coleta::retangulo().contains(mouse) {
                    self.fecha_paineis();
                    self.config_coleta.abrir();
                }
                // Segurar um espaco parado (toque longo) tambem abre o
                // configurador; arrastar continua sendo o AUTO.
                let sob_dedo = rects.iter().position(|r| r.contains(mouse)).map(|i| i as u32);
                if let toque::Toque::Longo(i) = self.toque_barra.quadro(
                    is_mouse_button_pressed(MouseButton::Left),
                    is_mouse_button_down(MouseButton::Left),
                    is_mouse_button_released(MouseButton::Left),
                    sob_dedo,
                    mouse,
                    get_time(),
                ) {
                    self.barra.cancela_gesto();
                    self.fecha_paineis();
                    self.config_barra.abrir(Some(i as usize));
                }
                let gesto = self.barra.entrada(
                    &rects,
                    mouse,
                    is_mouse_button_pressed(MouseButton::Left),
                    is_mouse_button_down(MouseButton::Left),
                    is_mouse_button_released(MouseButton::Left),
                );
                if let Some(g) = gesto {
                    let (usar, mudou) = self.barra.aplica(g);
                    if let Some(i) = usar {
                        self.usar_rapido(i);
                    }
                    if mudou {
                        self.salvar_barra();
                    }
                }
                // A janela do NPC (ou o diario) e a loja cobrem o mesmo canto.
                if !self.missoes.aberta && !self.loja.aberta() {
                    let clique = {
                        let slots = &self.bolsa.slots;
                        let nivel = self.ficha.nivel.max(1);
                        let base = shared::xp_for_level_with_mult(nivel, self.ficha.mult_xp);
                        let prox = shared::xp_for_level_with_mult(nivel + 1, self.ficha.mult_xp);
                        let fracao = if prox > base { (self.ficha.xp.saturating_sub(base)) as f32 / (prox - base) as f32 } else { 0.0 };
                        self.missoes.desenha_rastreador(&|id| missoes::na_bolsa(slots, id), self.auto_missao.quest, nivel, fracao)
                    };
                    match clique {
                        Some(missoes::NoRastreador::Missao(id)) => self.iniciar_auto_missao(id),
                        Some(missoes::NoRastreador::Diario) => self.abrir_diario(),
                        Some(missoes::NoRastreador::Todas) => {
                            self.fecha_paineis();
                            self.menu_missoes.abrir();
                        }
                        None => {}
                    }
                }
            }
            let alvo = self.alvo.and_then(|id| self.world.ents.get(&id)).filter(|a| a.morte.is_none()).map(|a| {
                (a.meta.name.clone().unwrap_or_else(|| "?".into()), a.meta.nivel, a.state.hp, a.meta.hp_max, a.state.flags & shared::ent_flags::BOSS != 0)
            });
            if let Some((nome, nv, hp, hp_max, chefe)) = alvo {
                if hud::draw_alvo(&z, &nome, nv, hp, hp_max, chefe) {
                    self.alvo = None;
                    self.envia(ClientMessage::SetTarget { target: None });
                }
                if chefe {
                    telegrafico::rotulo_de_fase(z.alvo, hp, hp_max);
                }
            } else if let Some((nome, nv, hp, hp_max)) =
                self.world.self_pos().and_then(|eu| telegrafico::chefe_perto(&self.world, eu))
            {
                // Chefe em luta por perto sem estar selecionado: a barra dele.
                telegrafico::desenha_barra_de_chefe(z.alvo, &nome, nv, hp, hp_max);
            }
            let selo = self.selo_missoes();
            let selo_diarias = self.selo_diarias();
            match hud::draw_topo(&z, selo, selo_diarias, selo || selo_diarias) {
                Some(hud::Topo::Bolsa) => {
                    self.fecha_paineis();
                    self.bolsa.abrir();
                }
                Some(hud::Topo::Missoes) => self.abrir_diario(),
                Some(hud::Topo::Diarias) => {
                    self.fecha_paineis();
                    self.diarias.abrir();
                }
                Some(hud::Topo::Grupo) => self.chat.push("Grupo: em breve.".into()),
                Some(hud::Topo::Avisos) => self.chat.push("Avisos: nada novo.".into()),
                Some(hud::Topo::Menu) => {
                    self.fecha_paineis();
                    self.menu.abrir();
                }
                None => {}
            }
        }
        if let Some((texto, cor)) = self.texto_da_faixa() {
            hud_layout::desenha_faixa(&z, &texto, cor);
        }
        // Joystick virtual: so' aparece com o dedo na tela.
        self.joystick.desenha();
        if self.coleta_hud.ativa() {
            self.coleta_hud.desenha(&z, get_time());
        }
        if eu.is_some() {
            hud::draw_exp(&z, &self.ficha, nivel);
        }
        if self.loja.aberta() {
            let nome = self.loja.vendedor
                .and_then(|id| self.world.ents.get(&id))
                .and_then(|e| e.meta.name.clone())
                .unwrap_or_else(|| "Loja".into());
            for pedido in self.loja.desenha(&self.bolsa.nomes, self.bolsa.ouro, &nome) {
                self.envia(pedido);
            }
        }
        if self.craft.aberto() || self.forja.aberto() || self.config_barra.aberto || self.config_coleta.aberto || self.config_interface.aberto {
            hud_layout::escurece(0.55);
        }
        if self.config_interface.aberto {
            // Vale no quadro seguinte e vai pro servidor pelas preferencias.
            match self.config_interface.desenha(hud_layout::escala_ui(), self.economia.auto_min()) {
                Some(config_interface::Mudanca::Escala(nova)) => hud_layout::define_escala_ui(nova),
                Some(config_interface::Mudanca::EconomiaAuto(min)) => self.economia.auto_min = Some(min),
                Some(config_interface::Mudanca::EconomiaAgora) => self.entrar_economia(),
                None => {}
            }
        }
        if self.config_coleta.aberto {
            let (mut tipos, mut raio) = (self.auto_coleta.tipos, self.auto_coleta.raio);
            if self.config_coleta.desenha(&mut tipos, &mut raio) {
                // Vai pro servidor pelas preferencias (sincronia a cada quadro).
                self.auto_coleta.tipos = tipos;
                self.auto_coleta.raio = raio;
            }
        }
        // Com o "Onde obter" aberto por cima, o configurador fica parado.
        if self.config_barra.aberto && !self.onde_obter.aberto() {
            let bolsa = &self.bolsa;
            if self.config_barra.desenha(&mut self.barra, &bolsa.slots, &|id| bolsa.nome(id)) {
                self.salvar_barra();
            }
        }
        // Com o "Onde obter" aberto os paineis de baixo nao desenham: o toque
        // no popup nao pode cair num botao deles.
        let onde = self.onde_obter.aberto();
        if self.craft.aberto() && !onde {
            let nivel = self.ficha.nivel.max(1);
            if let Some(m) = self.craft.desenha(&self.bolsa.slots, &self.bolsa.nomes, nivel, get_time()) {
                self.envia(m);
            }
        }
        if self.forja.aberto() && !onde {
            if let Some(m) = self.forja.desenha(&self.bolsa.slots, &self.bolsa.equip, &self.bolsa.nomes, get_time()) {
                self.envia(m);
            }
        }
        if self.missoes.aberta && !onde {
            let slots = &self.bolsa.slots;
            let pedidos = self.missoes.desenha(&self.bolsa.nomes, &|id| missoes::na_bolsa(slots, id));
            for pedido in pedidos {
                self.envia(pedido);
            }
        }
        if let Some(id) = self.missoes.ir.take() {
            self.iniciar_auto_missao(id);
        }
        if self.menu_missoes.aberto && !onde {
            hud_layout::escurece(0.55);
            let agora_unix = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |d| d.as_secs() as i64);
            let clique = {
                let slots = &self.bolsa.slots;
                let tem = |id: u16| missoes::na_bolsa(slots, id);
                let c = menu_missoes::Contexto {
                    log: &self.missoes.log,
                    entregues: &self.quest_entregues,
                    nivel: self.ficha.nivel,
                    faccao: self.faccao_qid,
                    zona: self.mapa.zona(),
                    agora_unix,
                    tem: &tem,
                };
                self.menu_missoes.desenha(&c)
            };
            if let Some(c) = clique {
                self.clique_menu_missoes(c);
            }
        }
        if self.diarias.aberto && !onde {
            hud_layout::escurece(0.55);
            let agora_unix = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |d| d.as_secs() as i64);
            let clique = {
                let slots = &self.bolsa.slots;
                let tem = |id: u16| missoes::na_bolsa(slots, id);
                let c = menu_missoes::Contexto {
                    log: &self.missoes.log,
                    entregues: &self.quest_entregues,
                    nivel: self.ficha.nivel,
                    faccao: self.faccao_qid,
                    zona: self.mapa.zona(),
                    agora_unix,
                    tem: &tem,
                };
                self.diarias.desenha(&c, &self.bolsa.nomes)
            };
            if let Some(c) = clique {
                self.clique_menu_missoes(c);
            }
        }
        let agora = get_time();
        match self.dialogo.desenha() {
            dialogo::Resultado::Conversou { npc, .. } => {
                self.envia(ClientMessage::ConcluirConversa { npc_eid: npc.0 as u64 });
                if self.auto_missao.ativo() {
                    self.auto_missao.conversou(agora);
                } else {
                    // A mao: depois da conversa o NPC atende como sempre (a
                    // loja do Alquimista abre).
                    self.envia(ClientMessage::Interact { target_eid: Some(npc.0 as u64) });
                }
            }
            dialogo::Resultado::Receber(q) => {
                self.envia(ClientMessage::TurnInQuest { quest_id: q });
                self.auto_missao.entregue(agora);
            }
            dialogo::Resultado::Aceitar(q) => {
                self.envia(ClientMessage::AcceptQuest { quest_id: q });
            }
            dialogo::Resultado::Recusou(_) => {
                self.auto_missao.parar();
                self.missoes.abre_janela();
            }
            dialogo::Resultado::Fechou => self.auto_missao.parar(),
            dialogo::Resultado::Nada => {}
        }
        let nivel = self.ficha.nivel.max(1);
        match self.mapa.desenha_grande(&self.world, nivel) {
            Some(mapa::Entrada::Ir(alvo)) => self.iniciar_ir_para(alvo),
            Some(mapa::Entrada::Viajar(p)) => self.iniciar_viagem(p),
            None => {}
        }
        // A bolsa por cima do mundo.
        let pedido_da_bolsa = if onde { None } else { self.bolsa.desenha(&self.vox, &self.solido) };
        if let Some(pedido) = pedido_da_bolsa {
            self.envia(pedido);
        }
        if self.mercado.aberto && !onde {
            let ctx = mercado_ui::Contexto {
                slots: &self.bolsa.slots,
                nomes: &self.bolsa.nomes,
                ouro: self.bolsa.ouro,
                nivel: self.ficha.nivel.max(self.bolsa.nivel),
                digitado: self.teclado.digitado(),
            };
            let pedidos = self.mercado.desenha(&ctx, get_time());
            for pedido in pedidos {
                self.envia(pedido);
            }
        }
        if self.lojas.aberto {
            let lista = self.mapa.lojas();
            if let Some((nome, pos)) = self.lojas.desenha(&lista, self.world.self_pos()) {
                self.voltar_ao_menu = false;
                self.iniciar_ir_para(ir_para::Alvo { objetivo: ir_para::Objetivo::Npc, pos, raio: 0.0, rotulo: nome });
            }
        }
        // "Onde obter": a lupa de algum painel pediu; o popup vai por cima.
        let pedido_onde = [
            self.craft.onde_obter.take(),
            self.forja.onde_obter.take(),
            self.bolsa.onde_obter.take(),
            self.missoes.onde_obter.take(),
            self.diarias.onde_obter.take(),
            self.mercado.onde_obter.take(),
            self.config_barra.onde_obter.take(),
        ];
        if let Some(id) = pedido_onde.into_iter().flatten().next() {
            self.onde_obter.abrir(id);
        }
        if let Some(item) = self.onde_obter.item {
            let lojas = self.mapa.lojas_com_id();
            let nivel = self.ficha.nivel.max(self.bolsa.nivel).max(1);
            let ops = {
                let c = onde_obter::Onde {
                    info: self.mapa.info.as_ref(),
                    lojas: &lojas,
                    eu: self.world.self_pos(),
                    nivel,
                    vinculado: self.mercado.vinculados.contains(&item),
                    ilha_atual: onde_obter::ilha_da_zona(&self.info.zona),
                };
                onde_obter::opcoes(item, self.onde_obter.fontes_de(item), &c)
            };
            let nome = self.bolsa.nome(item);
            if let Some(ir) = self.onde_obter.desenha(&nome, &ops) {
                self.executar_onde_obter(ir);
            }
        }
        // O Menu por cima de tudo.
        if self.menu.aberto {
            let (nome, arma) = (
                eu.as_ref().map_or(String::new(), |e| e.3.clone()),
                shared::skills::Conjunto::da_arma(self.bolsa.equip.weapon.unwrap_or(0)).nome().to_string(),
            );
            let tem = |id: u16| missoes::na_bolsa(&self.bolsa.slots, id) as u64;
            let saldos = [
                ("Ouro", self.bolsa.ouro),
                ("Cobre", tem(shared::constants::item_id::COPPER)),
                ("Darksteel", tem(shared::constants::item_id::DARKSTEEL)),
            ];
            let mut selos: Vec<menu::Item> = Vec::new();
            if self.selo_missoes() {
                selos.push(menu::Item::Missoes);
            }
            if self.selo_diarias() {
                selos.push(menu::Item::Diarias);
            }
            let ctx = menu::Contexto {
                nome: &nome,
                nivel,
                poder: self.bolsa.stats.as_ref().map(crate::bolsa::poder),
                arma: &arma,
                saldos: &saldos,
                selos: &selos,
            };
            match self.menu.desenha(&ctx) {
                Some(menu::Clique::Abrir(item)) => self.abrir_do_menu(item),
                Some(menu::Clique::Aviso(t)) => self.chat.push(t),
                None => {}
            }
        }
        // Morte e "Recuperar XP" por cima de tudo. So' clique: Esc nao revive.
        let agora_unix = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_secs() as i64);
        if let Some(pedido) = self.morte.desenha(self.bolsa.ouro, agora_unix) {
            self.envia(pedido);
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
                let d = crate::hud_estilo::medir_dim(&dir, 14);
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
        const ALTURA: f32 = 440.0;
        // Teclado da tela aberto: o painel sobe o bastante pro campo com foco
        // (a senha, no pior caso) ficar acima dele.
        let topo = (screen_height() - ALTURA) * 0.5;
        let fundo_campo = topo + 60.0 + if self.foco_senha { 164.0 } else { 88.0 };
        let aberto = nativo::TECLADO_NA_TELA && self.teclado_virtual.aberto();
        ui::subir_paineis(teclado_virtual::deslocamento(aberto, screen_height(), topo, fundo_campo));
        let r = ui::painel(460.0, ALTURA, "entrar");
        ui::subir_paineis(0.0);
        let cx = r.x + r.w * 0.5;
        if let Some(h) = &self.host {
            ui::texto_centro(cx, r.y + 6.0, h, 15, ui::OURO);
        }

        let cu = Rect::new(r.x, r.y + 46.0, r.w, 42.0);
        let cs = Rect::new(r.x, r.y + 122.0, r.w, 42.0);
        let mut usuario = std::mem::take(&mut self.usuario);
        let mut senha = std::mem::take(&mut self.senha);
        let digitado = self.teclado.digitado().to_vec();
        let clicou_u = ui::campo(cu, "usuário", &mut usuario, !self.foco_senha, false, &digitado);
        let clicou_s = ui::campo(cs, "senha", &mut senha, self.foco_senha, true, &digitado);
        self.usuario = usuario;
        self.senha = senha;
        if clicou_u { self.foco_senha = false; self.campo_login_ativo = true; }
        if clicou_s { self.foco_senha = true; self.campo_login_ativo = true; }
        // Toque fora dos campos fecha o teclado da tela.
        if is_mouse_button_pressed(MouseButton::Left) && !clicou_u && !clicou_s {
            self.campo_login_ativo = false;
        }
        // Tab troca de campo; Enter (ou o Return do teclado do iPhone) no
        // usuario passa pra senha — teclado antes de mouse.
        if is_key_pressed(KeyCode::Tab) {
            self.foco_senha = !self.foco_senha;
        }
        let enter = is_key_pressed(KeyCode::Enter) || is_key_pressed(KeyCode::KpEnter);
        if enter && !self.foco_senha {
            self.foco_senha = true;
        } else {
            let pode = !self.usuario.is_empty() && !self.senha.is_empty() && !self.google.aguardando();
            let entrar = ui::botao(Rect::new(r.x, r.y + 196.0, r.w, 44.0), "entrar", pode)
                || (pode && self.foco_senha && enter);
            if entrar {
                self.campo_login_ativo = false;
                self.token_login = None;
                self.conectar();
            }
        }

        // Entrar com Google: so' aparece com o servidor configurado.
        if self.google.disponivel() {
            let rg = Rect::new(r.x, r.y + 252.0, r.w, 44.0);
            if self.google.aguardando() {
                ui::texto_centro(cx, rg.y + 16.0, &self.google.texto().unwrap_or_default(), 16, ui::OURO_CLARO);
                if ui::botao(Rect::new(cx - 70.0, rg.y + 26.0, 140.0, 32.0), "cancelar", true) {
                    self.google.cancelar();
                }
            } else {
                ui::texto_centro(cx, r.y + 246.0, "ou", 13, ui::OURO);
                if ui::botao(rg, "Entrar com Google", true) {
                    self.campo_login_ativo = false;
                    self.google.iniciar();
                }
                if let Some(e) = self.google.texto() {
                    ui::erro(cx, rg.y + rg.h + 16.0, &e);
                }
            }
        }
        if ui::botao(Rect::new(r.x, r.y + r.h - 40.0, 140.0, 36.0), "< voltar", true) {
            self.campo_login_ativo = false;
            self.google.cancelar();
            self.tela = Tela::Servidores;
        }
    }

    /// Um quadro do login com Google: abre o navegador e, quando o navegador
    /// termina, entra com a sessao recebida.
    fn passo_google(&mut self) {
        match self.google.tick(get_time()) {
            Some(login_google::Saida::AbrirUrl(url)) => {
                if !nativo::abrir_url(&url) {
                    self.google.falhou("Não consegui abrir o navegador.");
                }
            }
            Some(login_google::Saida::Pronto { usuario, token }) => {
                self.usuario = usuario;
                self.senha.clear();
                self.token_login = Some(token);
                if matches!(self.tela, Tela::Login) {
                    self.conectar();
                }
            }
            _ => {}
        }
    }

    /// Teclado da tela: aberto enquanto um campo de texto tem foco.
    fn passo_teclado_virtual(&mut self) {
        let precisa = match self.tela {
            Tela::Login => self.campo_login_ativo,
            Tela::Personagens => self.selecao_personagem.foco_no_nome(),
            Tela::Jogando => self.mercado.foco_na_busca(),
            _ => false,
        };
        if let Some(mostrar) = self.teclado_virtual.quer(precisa) {
            nativo::teclado_virtual(mostrar);
        }
    }

    fn tela_personagens(&mut self) {
        let acao=self.selecao_personagem.desenha(&self.personagens,&self.armas,&mut self.selecionado,
            self.teclado.digitado(),&self.vox,&self.solido);
        match acao {
            Some(personagens::Acao::Enviar(m)) => {
                if matches!(m,ClientMessage::SelectCharacter{..}) {self.tela=Tela::Conectando;}
                self.envia(m);
            }
            Some(personagens::Acao::Voltar) => self.sair(),
            None => {},
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
