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
mod auto_combate;
mod auto_dungeon;
mod auto_missao;
mod auto_resumo;
mod avisos;
mod banco_ui;
mod bicho;
mod bolsa;
mod colecao;
mod craft_ui;
mod dungeon_ui;
mod dungeon_cenario;
mod dungeon_recompensas;
mod nivel_ui;
mod recompensas_ui;
mod reconexao;
mod efeitos;
mod energia_vfx;
mod entrada;
mod novidades;
mod atualizacao;
mod desktop;
mod evolucao_skills;
mod ficha_ui;
mod foco;
mod forja_ui;
mod ganhos;
mod gpu_estatica;
mod luzes;
mod abismo;
mod capacete_abissal;
mod marinhos;
mod hidra;
mod habilidades;
mod habilidades_input;
mod hud;
mod porao_ui;
// A prévia da porta é só do desktop em debug, como a do HUD: ela usa alvo com
// profundidade, que no iOS some.
#[cfg(all(debug_assertions, not(any(target_os = "ios", target_os = "android"))))]
mod previa_porta;
mod previa_bestiary;
mod porao_planta;
#[cfg(all(debug_assertions, not(any(target_os = "ios", target_os = "android"))))]
mod previa_hud;
mod hud_estilo;
mod hud_layout;
mod icone_npc;
mod icones;
mod icones_ui;
mod invocacao_ui;
mod login_google;
mod loja;
mod lojas;
mod menu;
mod mobs_ui;
mod mercado_ui;
mod missoes;
mod sons;
mod auras;
mod nativo;
mod oficina_ui;
mod pets_ui;
mod rolagem;
mod social_ui;
mod social_hud;
mod seguir;
mod teclado_virtual;

/// Pedacos de terreno em volta do jogador que precisam existir pra tela de
/// carregando sair (3 = 7x7, mais que a camera enxerga de perto).
const RAIO_DO_CARREGANDO: i32 = 3;
mod agua;
mod auto_coleta;
mod barra;
mod camera_suave;
mod chefe_anim;
mod coleta_hud;
mod colonia_ui;
mod config_barra;
mod config_coleta;
mod config_combate;
mod config_graficos;
mod config_interface;
mod confirmar;
mod construcoes;
mod corrida;
mod dialogo;
mod diarias;
mod economia;
mod escolha_npc;
mod gesto_camera;
mod guarda_roupa_ui;
mod habilidades_vfx;
mod ir_para;
mod joystick;
mod lascas;
mod lembranca;
mod loja_tp;
mod magica_ui;
mod map;
mod mapa;
mod menu_missoes;
mod montarias_ui;
mod morte;
mod mundo_ui;
mod net;
mod nivel_vfx;
mod onde_obter;
mod parado;
mod personagens;
mod preferencias;
mod presenca_ui;
mod previa_skills;
mod rastro;
mod render3d;
mod rig;
mod telegrafico;
mod terreno;
mod ilhas_aereas;
mod transito;
mod toque;
mod ui;
mod vegetacao;
mod viagem_ui;
mod vox;
mod world;

use std::sync::mpsc::Receiver;

use macroquad::prelude::*;
use shared::protocol::{ClientMessage, InputFrame, ServerMessage};

use api::Canal;
use macroquad::material::{gl_use_default_material, gl_use_material, Material};
use map::Map;
use net::{Net, NetEvent};
use vox::VoxCache;
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
    /// Criar conta por usuário e senha (o Google cria a dele sozinho).
    Cadastro,
    /// Pedir o e-mail de redefinição de senha.
    EsqueciSenha,
    Conectando,
    Personagens,
    /// Canal de instancia unica lotado. Nao e' recusa: e' vez na fila.
    Fila {
        posicao: u32,
        total: u32,
    },
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
    realm: Option<String>,
    host: Option<String>,
    // ── login ──
    usuario: String,
    senha: String,
    foco_senha: bool,
    /// A caixinha "remember me" da tela de login.
    lembrar: bool,
    /// Já houve um login bem-sucedido nesta execução. É o que separa o
    /// primeiro login da reconexão por troca de zona.
    ja_entrou: bool,
    /// Recado a mostrar na tela de login (sessão vencida, por exemplo).
    erro_login: Option<String>,
    /// Os três campos do cadastro e qual deles tem o teclado.
    cad_usuario: String,
    cad_email: String,
    cad_senha: String,
    /// A repetição da senha. Sem ela, um dedo errado no celular vira uma
    /// conta cuja senha ninguém sabe — e não há como recuperar.
    cad_senha2: String,
    cad_foco: usize,
    /// Pedido de cadastro em voo, e o que ele respondeu.
    cad_pedido: Option<std::sync::mpsc::Receiver<api::RespostaCadastro>>,
    cad_recado: Option<(String, bool)>,
    /// "Esqueci minha senha": o e-mail digitado, o pedido em voo e o recado.
    esq_email: String,
    esq_pedido: Option<std::sync::mpsc::Receiver<Result<(), String>>>,
    esq_recado: Option<(String, bool)>,
    /// Algum campo de login foi tocado: o teclado da tela fica aberto.
    campo_login_ativo: bool,
    teclado_virtual: teclado_virtual::TecladoVirtual,
    /// "Sign in with Google" (docs/LOGIN_GOOGLE.md).
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
    /// The connection dropped mid-game and the client is trying to come
    /// back by itself (`reconexao`). `None` = connected, or gave up.
    reconexao: Option<reconexao::Reconexao>,
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
    social_hud: social_hud::SocialHud,
    seguir: seguir::Seguir,
    /// Inventario e equipamento (icone do HUD ou Menu). Ver `bolsa`.
    bolsa: bolsa::Bolsa,
    /// Vida, mana, vigor e experiencia do HUD (`hud::Ficha`).
    ficha: hud::Ficha,
    /// Ficha completa e distribuição dos seis atributos.
    ficha_ui: ficha_ui::FichaUi,
    /// A aba Pets: o bichinho equipado, nivel, fome e skills (docs/PETS.md).
    pets_ui: pets_ui::PetsUi,
    habilidades: habilidades::Habilidades,
    evolucao_skills: evolucao_skills::EvolucaoSkills,
    invocacao: invocacao_ui::InvocacaoUi,
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
    config_combate: config_combate::ConfigCombate,
    /// Toque longo no AUTO COMBATE abre a configuração. Ver `toque`.
    toque_combate: toque::ToqueLongo,
    config_interface: config_interface::ConfigInterface,
    config_graficos: config_graficos::ConfigGraficos,
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
    auto_resumo: auto_resumo::AutoResumo,
    /// Missão aceita pelo menu que deve começar assim que o servidor confirmar.
    /// AUTO DUNGEON: ultimo pedido mandado e quando o bau abriu.
    auto_dungeon_envio: f64,
    /// Tela de carregando: desde quando (entrou no mundo ou foi teleportado).
    carregando_desde: Option<f64>,
    /// Ultimo bicho que me acertou, e quando.
    agressor: Option<(shared::EntityId, f64)>,
    /// A auto coleta largou o no' pra matar este bicho.
    defesa_da_coleta: Option<shared::EntityId>,
    auto_dungeon_bau_em: Option<f64>,
    /// X: fica coletando no melhor spot perto.
    auto_coleta: auto_coleta::AutoColeta,
    /// Field-boss chests opened in auto gather.
    bau_do_auto: auto_coleta::BauDoAuto,
    /// Toque longo no AUTO COLETA e na barra de itens: o que no PC e' o botao
    /// direito (configuracao), no toque e' segurar.
    toque_coleta: toque::ToqueLongo,
    toque_barra: toque::ToqueLongo,
    /// Falas das missoes (Proximo / Receber / Aceitar).
    dialogo: dialogo::Dialogo,
    /// A rota do servidor, pro tracejado no chao.
    rastro: rastro::Rastro,
    /// Missões que o auto pulou por exigirem o jogador. Ver `auto_missao_pula`.
    auto_pulados: std::collections::HashSet<u16>,
    /// Golpes de chefe carregando: a forma no chao.
    telegrafos: telegrafico::Telegrafos,
    /// Tremor de camera do impacto de chefe: (forca, ate quando).
    tremor: (f32, f64),
    /// Tela de morte e "Recover XP".
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
    /// "Go to" do mapa (zona de bicho, regiao de recurso) e do menu de
    /// missoes (ir ao Mestre).
    ir_para: ir_para::IrPara,
    /// Menu de todas as missoes (rodape do rastreador ou Menu).
    menu_missoes: menu_missoes::MenuMissoes,
    mobs_ui: mobs_ui::MobsUi,
    /// Painel das diarias: icone no topo e Menu, separado das outras missoes.
    diarias: diarias::Diarias,
    /// O Menu Principal (botao ≡ do HUD). Nenhum painel abre por tecla.
    menu: menu::Menu,
    /// Menu → Comercio: os vendedores da ilha com "Ir".
    lojas: lojas::Lojas,
    /// Menu → Comércio → Mercado (docs/MERCADO.md).
    mercado: mercado_ui::Mercado,
    social: social_ui::Social,
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
    /// Rota que o cliente pediu para alcancar o alvo em combate.
    aproximando_alvo: Option<shared::EntityId>,
    /// Ultimo `ServerMessage::SemVisada`: (alvo, quando).
    sem_visada: Option<(shared::EntityId, f64)>,
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
    novidades: novidades::Novidades,
    atualizacao: atualizacao::Atualizacao,
    /// Quanto o jogador inclinou A MAIS do que o zoom pediu, em radianos.
    ///
    /// Guardar o DESVIO e nao o angulo e' o que deixa o automatico e a mao
    /// conviverem: girar a roda muda o enquadramento sem apagar a correcao
    /// que a mao fez, e a mao continua mandando dentro da banda daquele zoom.
    cam_pitch_ajuste: f32,
    mouse_camera: desktop::ArrastoCamera,
    /// A zona em que o personagem está, pra saber que portas de Porão existem
    /// aqui. Chega no `Map` e vale até a próxima troca.
    zona_atual: String,
    porao: porao_ui::PoraoUi,
    /// Camera por toque (um dedo gira, pinca da' zoom). Ver `gesto_camera`.
    gesto_camera: gesto_camera::GestoCamera,
    /// O que o toque pediu NESTE quadro.
    toque_acao: gesto_camera::Acao,
    /// Havia dedo na tela neste quadro: o aperto simulado do mouse nao vale.
    toque_ativo: bool,
    /// Um painel tratado ANTES dos dedos (o mapa) consumiu o toque deste
    /// quadro: o dedo e' da interface ate' soltar.
    toque_consumido: bool,
    /// The left button went down while a modal popup (rewards, level up,
    /// confirm) owned the input: the press belongs to the popup until the
    /// button is released, even if the popup closed on that same press.
    clique_de_modal: bool,
    /// Ids dos dedos na tela no quadro anterior (ver `ler_toques`).
    dedos_anteriores: Vec<u64>,
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
    chat: avisos::Avisos,
    /// When the last AcceptQuest went out: the next SYS line is its answer.
    aceite_pendente: Option<f64>,
    /// Modo economia de energia (`economia.rs`).
    economia: economia::Economia,
    /// Calendario de presenca (`presenca_ui.rs`).
    presenca: presenca_ui::PresencaUi,
    /// Menu "Travel" do Capitao do Porto.
    viagem: viagem_ui::ViagemUi,
    colonia: colonia_ui::ColoniaUi,
    magica: magica_ui::MagicaUi,
    mundo: mundo_ui::Mundo,
    guarda_roupa: guarda_roupa_ui::GuardaRoupaUi,
    subiu_de_nivel: nivel_vfx::SubiuDeNivel,
    /// O banco (Banqueiro da vila).
    banco: banco_ui::Banco,
    /// Banco que chegou com um dialogo aberto: abre quando fechar.
    banco_pendente: Option<Vec<shared::InventorySlot>>,
    /// "Missões ou Loja?" do NPC que tem as duas coisas.
    escolha_npc: escolha_npc::EscolhaNpc,
    /// Menu do Capitao que chegou com um dialogo aberto: abre quando fechar.
    viagem_pendente: Option<Vec<shared::viagem::Destino>>,
    /// A missao que o AUTO conduzia quando embarcou pra outra ilha.
    ///
    /// A troca de zona passa por `conectar`, que para todos os autos e limpa o
    /// log — sem guardar isto, o auto atravessava o mar e desligava sozinho ao
    /// desembarcar, que e' pior do que nao ter atravessado: o jogador volta
    /// pro teclado numa ilha que nao escolheu. `conectar` NAO limpa este
    /// campo, de proposito.
    retomar_auto_missao: Option<u16>,
    /// A quest da ilha propria ja' passou (docs/COLONIA.md). Vem no menu do
    /// Capitao, que e' o unico lugar de onde se viaja pra la'.
    tem_colonia: bool,
    /// O guarda-roupa que o servidor confirmou (docs/PERSONAGEM.md).
    guarda_roupa_salvo: shared::aparencia::GuardaRoupa,
    /// O botao "Teleportar" do ultimo quadro (so' com viagem longa na tela).
    botao_teleporte: Option<Rect>,
    /// Loja de cash (`loja_tp.rs`).
    loja_tp: loja_tp::LojaTp,
    /// Janela de montarias (`montarias_ui.rs`).
    montarias: montarias_ui::MontariasUi,
    /// Skin de montaria escolhida (preferencias; o servidor valida a posse).
    montaria_skin: Option<u16>,
    /// Ultimo pedido de montar automatico (viagem): nao repete em rajada.
    montar_auto_em: f64,
    /// Dungeons: janela, fila, pronto-check, instancia e resultado.
    dungeon: dungeon_ui::DungeonUi,
    /// Porão floor plans, built once each (`porao_planta`).
    plantas: porao_planta::Cache,
    cenario_dungeon: dungeon_cenario::Cache,
    recompensas: recompensas_ui::Ui,
    /// "Onde obter" (`onde_obter.rs`).
    onde_obter: onde_obter::OndeObter,
    /// Ultimo pedido de tela acesa mandado ao sistema.
    tela_acesa: bool,
    /// Botao de PULO do HUD (o celular nao tem tecla de espaco): tocou neste
    /// quadro e esta' segurando.
    pulo_toque: bool,
    pulo_segurando: bool,
    /// Dash tocado no HUD. Fica memorizado ate o proximo pacote de input para
    /// um toque curto nao se perder entre dois envios de rede.
    dash_toque: bool,
    dash_recarga: (f64, f32),
    /// Uso de pocao de efeito esperando "tem certeza?" (`confirmar.rs`).
    confirmar: Option<confirmar::Pendente>,
    /// O passo TUTORIAL com o foco armado: (quest, quando armou). Armar e'
    /// tocar no passo no rastreador; o prazo existe pra que um alvo que
    /// sumiu da tela nunca deixe o jogador preso atras do escuro.
    foco_tutorial: Option<(u16, f64)>,
    /// A FILA de missões: o que ainda falta fazer, na ordem escolhida.
    ///
    /// Vazia = sem fila. Quem a esvazia é o laço `tocar_fila`, que puxa a
    /// próxima assim que a auto missão para — por ter entregado, ou por ter
    /// sido recusada.
    fila_de_missoes: Vec<u16>,
    /// Quantas a fila já entregou e quantas pulou, pra contar no fim.
    fila_feitas: usize,
    fila_puladas: usize,
    /// A DICA da trava de nível: acende o caminho até a Ilha Mágica.
    ///
    /// A história para esperando nível e o jogador chega uns três abaixo. A
    /// trava dizia "História: chegue ao nível 20 para continuar" e o
    /// deixava ali, sem dizer onde arrumar XP. A Ilha Mágica é a resposta —
    /// XP em dobro e três entradas de graça por dia —, e apontar o caminho é
    /// o mesmo gesto dos outros tutoriais: o buraco aceso anda MENU → Evento
    /// → Ilha Mágica.
    dica_da_trava: Option<f64>,
    nivel_ui: nivel_ui::NivelUi,
}

#[macroquad::main(window_conf)]
async fn main() {
    // Atalhos do Windows podem iniciar com outra pasta de trabalho.
    #[cfg(target_os = "windows")]
    if let Ok(exe) = std::env::current_exe() {
        if let Some(pasta) = exe.parent().filter(|p| p.join("assets").is_dir()) {
            std::env::set_current_dir(pasta).expect("app resources folder");
        }
    }
    // Finder não inicia o app na pasta dos assets.
    #[cfg(target_os = "macos")]
    if let Ok(exe) = std::env::current_exe() {
        if let Some(contents) = exe.parent().and_then(|p| p.parent()) {
            let resources = contents.join("Resources");
            if resources.join("assets").is_dir() {
                std::env::set_current_dir(&resources).expect("app resources folder");
            }
        }
    }
    sons::carregar().await;
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
    vox.load_rig(render3d::RIG_CORPO, render3d::VOXEL, rig::pivo)
        .await;
    for (_, nome) in render3d::RIGS_DE_GENTE {
        vox.load_rig(nome, render3d::VOXEL, rig::pivo).await;
    }
    // Um corpo por oficio (tools/voxrender/npcs.py). Sem o arquivo, o NPC cai
    // no corpo do jogador.
    for nome in render3d::MODELOS_DE_NPC {
        vox.load_rig(nome, render3d::VOXEL, rig::pivo).await;
    }
    vox.load_rig(render3d::RIG_CHAPEU, render3d::VOXEL, rig::pivo)
        .await;
    // Rostos, cabelos e chapeus (tools/voxrender/personagem.py). Sao 14
    // arquivos pequenos; o boot ja' carregava 19 rigs.
    //
    // A lista sai de `rigs_da_cabeca`, que e' a MESMA que o seletor oferece:
    // dois lacos a mao divergiram uma vez e os chapeus ficaram sem carregar.
    for nome in render3d::rigs_da_cabeca() {
        vox.load_rig(&nome, render3d::VOXEL, rig::pivo).await;
    }
    vox.load_variantes("saque", render3d::VOXEL, &render3d::variantes_do_saque())
        .await;
    // Os bichos em PECAS (tools/voxrender/bichos.py). Sem o arquivo, o mob
    // cai no modelo inteiro de antes.
    // As armas do primeiro conjunto (tools/voxrender/armas.py).
    // E as ferramentas de coleta (machado, picareta de cada cor).
    for nome in [
        "espada", "escudo", "katana", "bainha", "pistola", "coldre", "arco",
    ]
    .into_iter()
    .chain(rig::FERRAMENTAS)
    {
        vox.load_arma(nome, render3d::VOXEL).await;
    }
    for (nome, altura) in bicho::BICHOS {
        vox.load_bicho(nome, altura).await;
    }

    // O IDIOMA ENTRA ANTES DE QUALQUER TELA — inclusive antes das prévias.
    //
    // Estava depois, junto do resto da leitura de preferências, e as prévias
    // (que terminam em `return`) nunca chegavam nele: a captura em inglês saía
    // IDÊNTICA à em português, byte por byte. Prévia é o jeito de conferir
    // tradução sem abrir o jogo, então ela é justamente quem mais precisa.
    //
    // A escolha de servidor e a tela de entrar também já são texto: trocar o
    // idioma depois do login faria a primeira tela aparecer em português para
    // quem joga em inglês.
    //
    // `MMO_IDIOMA` ganha do arquivo — é como o teste e a captura de tela pedem
    // uma língua sem mexer nas preferências de ninguém.
    shared::idioma::definir(
        std::env::var("MMO_IDIOMA")
            .ok()
            .and_then(|v| shared::idioma::Idioma::do_codigo(&v))
            .unwrap_or_else(|| lembranca::carrega().idioma),
    );

    #[cfg(all(debug_assertions, not(any(target_os = "ios", target_os = "android"))))]
    if std::env::var("MMO_PREVIA_HUD").is_ok() {
        previa_hud::abrir(&mut vox).await;
        return;
    }
    if std::env::var("MMO_PREVIA_NOVIDADES").is_ok() {
        novidades::previa().await;
        return;
    }
    #[cfg(debug_assertions)]
    if std::env::var("MMO_PREVIA_AURAS_ANIMAIS").is_ok() {
        auras::previa_animais(&vox).await;
        return;
    }
    if std::env::var("MMO_PREVIA_AURAS").is_ok() {
        auras::previa(&vox).await;
        return;
    }
    if std::env::var("MMO_PREVIA_SKILLS").is_ok() {
        previa_skills::abrir(&vox).await;
        return;
    }
    #[cfg(debug_assertions)]
    if std::env::var("MMO_PREVIA_BESTIARY").is_ok() {
        previa_bestiary::previa(&mut vox, &render3d::material_solido()).await;
        return;
    }
    #[cfg(debug_assertions)]
    if std::env::var("MMO_PREVIA_PETS").is_ok() {
        pets_ui::previa(&vox, &render3d::material_solido()).await;
        return;
    }
    #[cfg(debug_assertions)]
    if std::env::var("MMO_PREVIA_ENERGIA").is_ok() {
        energia_vfx::previa(&render3d::material_solido()).await;
        return;
    }
    #[cfg(debug_assertions)]
    if std::env::var("MMO_PREVIA_COLONIA").is_ok() {
        terreno::previa_da_colonia(&render3d::material_solido(), &vox).await;
        return;
    }
    #[cfg(debug_assertions)]
    if std::env::var("MMO_PREVIA_FICHA").is_ok() {
        ficha_ui::previa().await;
        return;
    }
    #[cfg(debug_assertions)]
    if std::env::var("MMO_PREVIA_OASIS").is_ok() {
        terreno::previa_do_oasis().await;
        return;
    }
    #[cfg(debug_assertions)]
    if std::env::var("MMO_PREVIA_SOMBRAS").is_ok() {
        terreno::previa_das_sombras().await;
        return;
    }
    #[cfg(debug_assertions)]
    if std::env::var("MMO_PREVIA_GRAFICOS").is_ok() {
        terreno::previa_dos_graficos().await;
        return;
    }
    #[cfg(debug_assertions)]
    if std::env::var("MMO_PREVIA_PLANALTO").is_ok() {
        terreno::previa_do_planalto().await;
        return;
    }
    #[cfg(debug_assertions)]
    if std::env::var("MMO_PREVIA_ILHAS_MAGICAS").is_ok() {
        terreno::previa_das_ilhas_magicas().await;
        return;
    }
    #[cfg(debug_assertions)]
    if std::env::var("MMO_PREVIA_PORAO").is_ok() {
        porao_ui::previa().await;
        return;
    }
    #[cfg(debug_assertions)]
    if std::env::var("MMO_PREVIA_PLANTA").is_ok() {
        previa_porta::plantas(&render3d::material_solido()).await;
        return;
    }
    #[cfg(debug_assertions)]
    if std::env::var("MMO_PREVIA_PORTA").is_ok() {
        previa_porta::abrir(&render3d::material_solido()).await;
        return;
    }
    #[cfg(debug_assertions)]
    if std::env::var("MMO_PREVIA_PRESENCA").is_ok() {
        presenca_ui::previa().await;
        return;
    }
    #[cfg(debug_assertions)]
    if std::env::var("MMO_PREVIA_MAGICA").is_ok() {
        magica_ui::previa().await;
        return;
    }
    #[cfg(debug_assertions)]
    if std::env::var("MMO_PREVIA_INVOCACAO").is_ok() {
        invocacao_ui::previa(&vox).await;
        return;
    }
    #[cfg(debug_assertions)]
    if std::env::var("MMO_PREVIA_EVOLUCAO").is_ok() {
        evolucao_skills::previa().await;
        return;
    }
    #[cfg(debug_assertions)]
    if std::env::var("MMO_PREVIA_KOGEN").is_ok() {
        terreno::previa_kogen(&mut vox).await;
        return;
    }
    #[cfg(debug_assertions)]
    if std::env::var("MMO_PREVIA_CAPACETE").is_ok() {
        capacete_abissal::previa(&vox).await;
        return;
    }
    #[cfg(debug_assertions)]
    if std::env::var("MMO_PREVIA_ABISSAL").is_ok() {
        terreno::previa_abissal().await;
        return;
    }
    #[cfg(debug_assertions)]
    if std::env::var("MMO_PREVIA_MARINHOS").is_ok() {
        marinhos::previa().await;
        return;
    }
    #[cfg(debug_assertions)]
    if std::env::var("MMO_PREVIA_BAUS").is_ok() {
        terreno::previa_baus(&mut vox).await;
        return;
    }
    #[cfg(debug_assertions)]
    if std::env::var("MMO_PREVIA_CELESTE").is_ok() {
        terreno::previa_celeste(&mut vox).await;
        return;
    }
    #[cfg(all(debug_assertions, not(any(target_os = "ios", target_os = "android"))))]
    if std::env::var("MMO_PREVIA_FORJA").is_ok() {
        forja_ui::previa().await;
        return;
    }
    #[cfg(debug_assertions)]
    if std::env::var("MMO_PREVIA_OFICINA").is_ok() {
        craft_ui::previa().await;
        return;
    }
    #[cfg(all(debug_assertions, not(any(target_os = "ios", target_os = "android"))))]
    if std::env::var("MMO_PREVIA_APARENCIA").is_ok() {
        guarda_roupa_ui::previa(&mut vox).await;
        return;
    }
    #[cfg(debug_assertions)]
    if std::env::var("MMO_PREVIA_PROTOCOLO").is_ok() {
        atualizacao::previa_incompativel().await;
        return;
    }
    #[cfg(debug_assertions)]
    if std::env::var("MMO_PREVIA_ICONE_PORTA").is_ok() {
        mapa::previa_icones().await;
        return;
    }
    #[cfg(debug_assertions)]
    if std::env::var("MMO_PREVIA_MAPA").is_ok() {
        mapa::previa().await;
        return;
    }
    #[cfg(debug_assertions)]
    if std::env::var("MMO_PREVIA_MENU_MISSOES").is_ok() {
        menu_missoes::previa_menu().await;
        return;
    }
    #[cfg(debug_assertions)]
    if std::env::var("MMO_PREVIA_DUNGEON_CENARIO").is_ok() {
        dungeon_cenario::previa().await;
        return;
    }
    #[cfg(debug_assertions)]
    if std::env::var("MMO_PREVIA_DUNGEON_PAINEL").is_ok() {
        dungeon_ui::previa_painel().await;
        return;
    }
    #[cfg(debug_assertions)]
    if std::env::var("MMO_PREVIA_DUNGEON_RECOMPENSAS").is_ok() {
        dungeon_ui::previa_recompensas().await;
        return;
    }
    #[cfg(debug_assertions)]
    if std::env::var("MMO_PREVIA_SOCIAL_HUD").is_ok() {
        social_hud::previa().await;
        return;
    }
    #[cfg(debug_assertions)]
    if std::env::var("MMO_PREVIA_SOCIAL").is_ok() {
        social_ui::previa().await;
        return;
    }
    if std::env::var("MMO_PREVIA_LOJA").is_ok() {
        loja_tp::previa(&vox).await;
        return;
    }
    // `option_env!` tambem: no celular nao ha' variavel de ambiente, e o APK
    // de medir desempenho e' compilado com ela ligada.
    #[cfg(debug_assertions)]
    if std::env::var("MMO_PREVIA_MERCADO").is_ok() {
        mercado_ui::previa(&vox).await;
        return;
    }
    if std::env::var("MMO_PREVIA_PERSONAGENS").is_ok()
        || option_env!("MMO_PREVIA_PERSONAGENS").is_some()
    {
        personagens::previa(&vox).await;
        return;
    }
    // O QUE O APARELHO LEMBRA da última vez: usuário e, se o jogador pediu,
    // a sessão. Lido UMA vez, aqui, antes de a tela existir.
    let lembranca = lembranca::carrega();
    let mut jogo = Jogo {
        tela: Tela::Servidores,
        canais: Vec::new(),
        busca: Some(api::buscar_canais()),
        realm: None,
        host: std::env::var("MMO_HOST").ok(),
        // O ÚLTIMO USUÁRIO vem do disco, e `MMO_USER` ainda ganha dele: o
        // teste automatizado precisa mandar em quem entra.
        usuario: std::env::var("MMO_USER")
            .ok()
            .filter(|v| !v.is_empty())
            .unwrap_or_else(|| lembranca.usuario.clone()),
        senha: std::env::var("MMO_PASS").unwrap_or_default(),
        foco_senha: false,
        lembrar: lembranca.lembrar,
        ja_entrou: false,
        erro_login: None,
        cad_usuario: String::new(),
        cad_email: String::new(),
        cad_senha: String::new(),
        cad_senha2: String::new(),
        cad_foco: 0,
        cad_pedido: None,
        cad_recado: None,
        esq_email: String::new(),
        esq_pedido: None,
        esq_recado: None,
        campo_login_ativo: false,
        teclado_virtual: teclado_virtual::TecladoVirtual::default(),
        google: login_google::LoginGoogle::consultando(),
        // A SESSÃO GUARDADA entra como token de login: com ela a tela de
        // login nem chega a aparecer.
        token_login: lembranca.sessao.clone(),
        personagens: Vec::new(),
        armas: Vec::new(),
        selecao_personagem: personagens::Personagens::default(),
        selecionado: 0,
        personagem_atual: std::env::var("MMO_CHAR").ok().filter(|v| !v.is_empty()),
        reconexao: None,
        net: None,
        terreno: None,
        vox,
        solido: render3d::material_solido(),
        map: None,
        world: World::default(),
        alvo: None,
        social_hud: social_hud::SocialHud::default(),
        seguir: seguir::Seguir::default(),
        bolsa: bolsa::Bolsa::default(),
        ficha: hud::Ficha::default(),
        ficha_ui: ficha_ui::FichaUi::default(),
        pets_ui: pets_ui::PetsUi::default(),
        habilidades: habilidades::Habilidades::default(),
        evolucao_skills: evolucao_skills::EvolucaoSkills::default(),
        invocacao: invocacao_ui::InvocacaoUi::default(),
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
        config_combate: Default::default(),
        toque_combate: toque::ToqueLongo::default(),
        config_interface: Default::default(),
        config_graficos: Default::default(),
        coleta_hud: Default::default(),
        coleta_pendente: None,
        coleta_auto_estava: false,
        missoes: missoes::Missoes::default(),
        auto_missao: auto_missao::AutoMissao::default(),
        auto_resumo: auto_resumo::AutoResumo::default(),
        auto_dungeon_envio: 0.0,
        carregando_desde: None,
        agressor: None,
        defesa_da_coleta: None,
        auto_dungeon_bau_em: None,
        auto_coleta: auto_coleta::AutoColeta::default(),
        bau_do_auto: auto_coleta::BauDoAuto::default(),
        toque_coleta: toque::ToqueLongo::default(),
        toque_barra: toque::ToqueLongo::default(),
        dialogo: dialogo::Dialogo::default(),
        rastro: rastro::Rastro::default(),
        auto_pulados: Default::default(),
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
        mobs_ui: mobs_ui::MobsUi::default(),
        diarias: diarias::Diarias::default(),
        menu: menu::Menu::default(),
        lojas: lojas::Lojas::default(),
        mercado: mercado_ui::Mercado::default(),
        social: social_ui::Social::default(),
        voltar_ao_menu: false,
        esc_consumido: false,
        tela_cheia: false,
        quest_entregues: std::collections::HashMap::new(),
        faccao_qid: 0,
        ultima_aproximacao: 0.0,
        aproximando_alvo: None,
        sem_visada: None,
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
        novidades: novidades::Novidades::default(),
        atualizacao: atualizacao::Atualizacao::default(),
        cam_pitch_ajuste: 0.0,
        mouse_camera: desktop::ArrastoCamera::default(),
        zona_atual: String::new(),
        porao: porao_ui::PoraoUi::default(),
        gesto_camera: gesto_camera::GestoCamera::default(),
        toque_acao: gesto_camera::Acao::Nada,
        toque_ativo: false,
        toque_consumido: false,
        clique_de_modal: false,
        dedos_anteriores: Vec::new(),
        ultimo_toque: f64::NEG_INFINITY,
        camera_suave: camera_suave::CameraSuave::default(),
        girando_toque: false,
        joystick: joystick::Joystick::default(),
        rede: hud::Rede::default(),
        ultimo_ping: 0.0,
        banda_marca: (0.0, 0),
        info: hud::Info::default(),
        chat: avisos::Avisos::default(),
        aceite_pendente: None,
        economia: economia::Economia::default(),
        presenca: presenca_ui::PresencaUi::default(),
        viagem: viagem_ui::ViagemUi::default(),
        colonia: colonia_ui::ColoniaUi::default(),
        magica: magica_ui::MagicaUi::default(),
        mundo: mundo_ui::Mundo::default(),
        guarda_roupa: guarda_roupa_ui::GuardaRoupaUi::default(),
        subiu_de_nivel: nivel_vfx::SubiuDeNivel::default(),
        banco: banco_ui::Banco::default(),
        banco_pendente: None,
        viagem_pendente: None,
        retomar_auto_missao: None,
        tem_colonia: false,
        guarda_roupa_salvo: Default::default(),
        escolha_npc: escolha_npc::EscolhaNpc::default(),
        botao_teleporte: None,
        loja_tp: loja_tp::LojaTp::default(),
        montarias: montarias_ui::MontariasUi::default(),
        montaria_skin: None,
        montar_auto_em: -99.0,
        dungeon: dungeon_ui::DungeonUi::default(),
        plantas: porao_planta::Cache::default(),
        cenario_dungeon: dungeon_cenario::Cache::default(),
        recompensas: recompensas_ui::Ui::default(),
        onde_obter: onde_obter::OndeObter::default(),
        tela_acesa: false,
        pulo_toque: false,
        pulo_segurando: false,
        dash_toque: false,
        dash_recarga: (0.0, 0.0),
        confirmar: None,
        foco_tutorial: None,
        fila_de_missoes: Vec::new(),
        fila_feitas: 0,
        fila_puladas: 0,
        dica_da_trava: None,
        nivel_ui: nivel_ui::NivelUi::default(),
    };
    // `MMO_HOST` explicito pula a escolha — e' o caminho do run-client.sh e dos
    // testes de carga.
    if jogo.host.is_some() {
        jogo.conectar();
    }

    loop {
        jogo.passo();
        jogo.desenhar();
        // UM pendente por quadro (skins de roupa que alguem a' vista esta'
        // usando). Um rig custa 2-5 ms de malha; dois por quadro engasgam.
        // A 60 fps isto resolve uma praca cheia de skins distintas em pouco
        // mais de um segundo, mostrando o corpo padrao enquanto isso.
        jogo.vox.atende_um_pendente(render3d::VOXEL).await;
        // Modo economia: o quadro cai pra `economia::FPS`.
        jogo.economia.segurar_quadro();
        // The Graphics frame-rate cap. Battery saver's own, lower one wins
        // while it is on.
        if !jogo.economia.ativa {
            config_graficos::segurar_quadro();
        }
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
    // A captura precisa continuar quando a janela fica coberta: o Wayland
    // pode suspender os callbacks de quadro. Só a prévia usa X11.
    #[cfg(all(debug_assertions, target_os = "linux"))]
    if std::env::var("MMO_PREVIA_APARENCIA").is_ok() {
        conf.platform.linux_backend = miniquad::conf::LinuxBackend::X11Only;
    }
    // O default do miniquad desenha decoracao do lado do cliente via libdecor;
    // o Hyprland ja decora pelo hyprbars.
    conf.platform.wayland_decorations = miniquad::conf::WaylandDecorations::ServerOnly;
    // Sem isto a janela se anuncia como "miniquad-application", o nome
    // generico do framework.
    conf.platform.linux_wm_class = "tempest";
    // MSAA: the scene draws straight to the window framebuffer, and without
    // it the voxel edges and fences come out jagged. Menu → Interface.
    conf.sample_count = config_graficos::antialias_salvo();
    // Celular: tela cheia na resolucao nativa. A orientacao (paisagem) vem do
    // pacote: Info.plist (scripts/build-ios.sh) ou o manifest do APK
    // ([package.metadata.android] do crates/client/Cargo.toml).
    #[cfg(any(target_os = "ios", target_os = "android"))]
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

/// Quebra um texto em linhas de no máximo `n` caracteres, sem cortar palavra.
fn quebra_em_linhas(t: &str, n: usize) -> Vec<String> {
    let mut linhas = Vec::new();
    let mut atual = String::new();
    for p in t.split_whitespace() {
        if !atual.is_empty() && atual.chars().count() + 1 + p.chars().count() > n {
            linhas.push(std::mem::take(&mut atual));
        }
        if !atual.is_empty() {
            atual.push(' ');
        }
        atual.push_str(p);
    }
    if !atual.is_empty() {
        linhas.push(atual);
    }
    linhas
}

/// Onde o destaque da trava de nível aponta, pelo estado do painel.
///
/// O DESTAQUE SEGUE O PAINEL. Antes ele apontava o MENU pelo tempo todo,
/// aberto ou não: o jogador abria a Ilha Mágica e o jogo continuava pedindo
/// pra abrir o que já estava aberto, sem dizer o que fazer dentro. O dono:
/// "eu abro a ilha mágica e continua pedindo pra eu abrir o menu e não pra
/// entrar na ilha mágica".
///
/// Fora do método pra poder ser testado: `foco_da_trava` depende do `Jogo`
/// inteiro, e a escolha em si é uma linha.
fn alvo_da_trava(painel_aberto: bool) -> &'static [u16] {
    if painel_aberto {
        &[foco::chave::MAGICA_ENTRAR]
    } else {
        &[foco::chave::MENU_ILHA_MAGICA, foco::chave::MENU]
    }
}

/// Onde cada peça da tela de "I forgot my password" fica.
///
/// Ordem: e-mail, enviar, voltar.
fn layout_do_esqueci(r: Rect) -> [Rect; 3] {
    [
        Rect::new(r.x, r.y + 92.0, r.w, 42.0),
        Rect::new(r.x, r.y + 154.0, r.w, 44.0),
        Rect::new(r.x, r.y + 208.0, r.w, 44.0),
    ]
}

/// Onde cada peça da tela de CADASTRO fica.
///
/// Ordem: usuário, e-mail, senha, criar, voltar.
fn layout_do_cadastro(r: Rect) -> [Rect; 6] {
    [
        Rect::new(r.x, r.y + 52.0, r.w, 42.0),
        Rect::new(r.x, r.y + 128.0, r.w, 42.0),
        Rect::new(r.x, r.y + 204.0, r.w, 42.0),
        Rect::new(r.x, r.y + 280.0, r.w, 42.0),
        Rect::new(r.x, r.y + 342.0, r.w, 44.0),
        Rect::new(r.x, r.y + 396.0, r.w, 44.0),
    ]
}

/// Onde cada peça da tela de login fica, dado o retângulo do painel.
///
/// Ordem: usuário, senha, lembrar, entrar, "ou", google.
///
/// Extraída porque as posições eram números soltos no meio do desenho, e
/// acrescentar UMA linha ("remember me") empurrou três delas. Sem função
/// não há como conferir que nada encosta em nada a não ser abrindo o jogo — e
/// deixar de abrir é o que já me fez entregar tela quebrada mais de uma vez
/// neste projeto.
fn layout_do_login(r: Rect) -> [Rect; 8] {
    [
        Rect::new(r.x, r.y + 46.0, r.w, 42.0),
        Rect::new(r.x, r.y + 122.0, r.w, 42.0),
        Rect::new(r.x, r.y + 178.0, r.w, 38.0),
        Rect::new(r.x, r.y + 224.0, r.w, 44.0),
        Rect::new(r.x, r.y + 270.0, r.w, 14.0),
        Rect::new(r.x, r.y + 288.0, r.w, 44.0),
        Rect::new(r.x, r.y + 338.0, r.w, 44.0),
        Rect::new(r.x, r.y + 388.0, r.w, 36.0),
    ]
}

#[cfg(test)]
mod testes_do_login {
    use super::*;

    /// Nada encosta em nada, e tudo cabe no painel.
    ///
    /// A altura é a constante de `tela_login`; se ela mudar sem o layout
    /// mudar junto, este teste é quem avisa.
    #[test]
    fn a_tela_de_login_nao_se_sobrepoe() {
        const ALTURA: f32 = 560.0;
        let r = Rect::new(100.0, 60.0, 460.0, ALTURA);
        let pecas = layout_do_login(r);
        let nomes = [
            "username", "senha", "lembrar", "entrar", "ou", "google", "criar", "esqueci",
        ];
        for (i, a) in pecas.iter().enumerate() {
            assert!(
                a.y >= r.y && a.y + a.h <= r.y + ALTURA,
                "{} vaza o painel: {a:?}",
                nomes[i]
            );
            for (j, b) in pecas.iter().enumerate().skip(i + 1) {
                assert!(
                    a.y + a.h <= b.y || b.y + b.h <= a.y,
                    "{} encosta em {}: {a:?} e {b:?}",
                    nomes[i],
                    nomes[j]
                );
            }
        }
    }

    /// O destaque da trava sai do menu quando o painel abre.
    #[test]
    fn o_destaque_da_trava_segue_o_painel() {
        let fechado = alvo_da_trava(false);
        assert!(
            fechado.contains(&foco::chave::MENU_ILHA_MAGICA),
            "com o painel fechado, o caminho é pelo menu"
        );
        let aberto = alvo_da_trava(true);
        assert_eq!(
            aberto,
            &[foco::chave::MAGICA_ENTRAR],
            "com o painel aberto o alvo é o Entrar"
        );
        assert!(
            !aberto.contains(&foco::chave::MENU),
            "pedir pra abrir o menu que já está aberto é o defeito que se conserta aqui"
        );
    }

    /// A tela de "I forgot my password" também não se sobrepõe.
    #[test]
    fn a_tela_de_esqueci_nao_se_sobrepoe() {
        const ALTURA: f32 = 300.0;
        let r = Rect::new(100.0, 60.0, 460.0, ALTURA);
        let p = layout_do_esqueci(r);
        let nomes = ["e-mail", "enviar", "voltar"];
        for (i, a) in p.iter().enumerate() {
            assert!(a.y >= r.y && a.y + a.h <= r.y + ALTURA, "{} vaza", nomes[i]);
            for (j, b) in p.iter().enumerate().skip(i + 1) {
                assert!(
                    a.y + a.h <= b.y || b.y + b.h <= a.y,
                    "{} encosta em {}",
                    nomes[i],
                    nomes[j]
                );
            }
        }
        for (r, nome) in [(p[1], "enviar"), (p[2], "voltar")] {
            assert!(ui::area_de_toque(r).h >= 44.0, "{nome} é pequeno demais");
        }
    }

    /// A quebra de linha não corta palavra e respeita o limite.
    #[test]
    fn o_recado_quebra_sem_cortar_palavra() {
        let t = "Se houver uma conta com esse e-mail, o link já saiu. Confira a caixa de entrada e o spam.";
        let l = quebra_em_linhas(t, 52);
        assert!(l.len() >= 2, "texto longo tem que virar mais de uma linha");
        for linha in &l {
            assert!(linha.chars().count() <= 52, "linha longa demais: {linha:?}");
        }
        // Nada some na quebra.
        assert_eq!(l.join(" "), t);
    }

    /// A tela de cadastro também não se sobrepõe nem vaza.
    #[test]
    fn a_tela_de_cadastro_nao_se_sobrepoe() {
        const ALTURA: f32 = 496.0;
        let r = Rect::new(100.0, 60.0, 460.0, ALTURA);
        let pecas = layout_do_cadastro(r);
        let nomes = ["username", "e-mail", "senha", "repetir", "criar", "voltar"];
        for (i, a) in pecas.iter().enumerate() {
            assert!(
                a.y >= r.y && a.y + a.h <= r.y + ALTURA,
                "{} vaza o painel: {a:?}",
                nomes[i]
            );
            for (j, b) in pecas.iter().enumerate().skip(i + 1) {
                assert!(
                    a.y + a.h <= b.y || b.y + b.h <= a.y,
                    "{} encosta em {}",
                    nomes[i],
                    nomes[j]
                );
            }
        }
        // Os dois botões cabem num dedo.
        for (r, nome) in [(pecas[4], "criar"), (pecas[5], "voltar")] {
            assert!(ui::area_de_toque(r).h >= 44.0, "{nome} é pequeno demais");
        }
    }

    /// O que se toca tem o tamanho de um dedo (44 pt da Apple).
    ///
    /// O dono já reclamou disso uma vez — "os botões de ação estão muito
    /// pequenos, eu clico errado toda hora" — e aquilo rendeu 55 botões
    /// corrigidos. Uma caixinha de marcar de 20 px seria o 56º.
    #[test]
    fn o_que_se_toca_cabe_num_dedo() {
        let p = layout_do_login(Rect::new(0.0, 0.0, 460.0, 480.0));
        for (r, nome) in [
            (p[2], "lembrar"),
            (p[3], "entrar"),
            (p[5], "google"),
            (p[6], "criar"),
        ] {
            let toque = ui::area_de_toque(r);
            assert!(toque.h >= 44.0, "{nome} tem só {:.0} pt de altura", toque.h);
        }
    }
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
        self.acompanhar_reconexao();
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
                self.world.tick(get_frame_time(), &|x, z, camada| {
                    terreno.map_or(0.0, |t| t.altura_apoio_na(x, z, shared::ENTITY_RADIUS, camada))
                });
            }
            self.subiu_de_nivel.passo(get_frame_time());
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
            if let Some(pedido) = self.social.passo(agora) {
                self.envia(pedido);
            }
            if let Some(pedido) = self.social.verificar_correio_dungeon(agora) {
                self.envia(pedido);
            }
            if self
                .economia
                .parado_demais(agora, economia::houve_entrada())
            {
                self.entrar_economia();
            }
            let eco = self.economia.bloqueia_entrada(agora);
            let eco_ativa = self.economia.ativa;
            let ui_pega = self.ui_pega_mouse();
            // A modal popup owns the click, and so does the rest of that
            // press. The rewards X closes the popup on PRESS, but the big map
            // clicks on RELEASE: with the popup already gone, the release fell
            // through to the map behind it and the player travelled there.
            let modal = self.recompensas.captura_entrada()
                || self.nivel_ui.captura_entrada()
                || self.confirmar.is_some();
            self.clique_de_modal = toque::press_do_modal(
                self.clique_de_modal,
                modal,
                is_mouse_button_pressed(MouseButton::Left),
                is_mouse_button_down(MouseButton::Left),
                is_mouse_button_released(MouseButton::Left),
            );
            let mapa_livre = !modal && !self.clique_de_modal;
            // Mapa e minimapa so' recebem clique quando estao a' mostra.
            let mapa_aberto = self.mapa.aberto;
            if !eco && mapa_livre && (self.mapa.aberto || !self.painel_grande()) {
                match self.mapa.entrada(self.world.self_pos()) {
                    Some(mapa::Entrada::Viajar(destino)) => {
                        self.iniciar_viagem(destino);
                        self.tutorial(shared::quests::tutorial::MAPA_IR);
                    }
                    Some(mapa::Entrada::Dungeon(id)) => {
                        self.mapa.aberto = false;
                        for pedido in self.dungeon.abrir_em(id) { self.envia(pedido); }
                    }
                    Some(mapa::Entrada::Ir(alvo)) => {
                        self.iniciar_ir_para(alvo);
                        self.tutorial(shared::quests::tutorial::MAPA_IR);
                    }
                    None => {}
                }
            }
            self.esc_consumido = !eco && is_key_pressed(KeyCode::Escape) && self.esc();
            // Toques antes de qualquer clique: com dedo, o clique no mundo sai
            // no SOLTAR (ver `gesto_camera`).
            // O toque que FECHOU o mapa (no X ou fora dele) e' do mapa: ele e'
            // tratado antes dos dedos, e sem isto o dedo parecia estar no
            // mundo — ao soltar virava "toque no chao", o personagem andava
            // pra la' e a auto rota caia.
            self.toque_consumido = mapa_aberto && !self.mapa.aberto;
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
            // Chegou na ilha pra onde o auto missao embarcou? Volta a conduzir.
            self.retomar_auto_apos_viagem();
            if !eco {
                self.teclas_de_acao();
            }
            self.acompanhar_loja();
            self.conduzir_auto_dungeon();
            self.atualizar_auto_combate();
            self.usar_habilidade();
            if std::mem::take(&mut self.habilidades.ligou_auto) {
                self.tutorial(shared::quests::tutorial::SKILL_AUTO);
            }
            self.auto_da_barra();
            self.world.alvo = self.alvo;
            self.conduzir_seguir();
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
            self.correndo_auto =
                self.corrida
                    .atualiza(self.world.self_pos(), automatico, get_frame_time());
            // Indo sozinho, a montaria entra igual a corrida: quem viaja no
            // automatico nao tem mao no teclado pra montar. O servidor recusa
            // em combate, e `montar_pra_viajar` tem o proprio intervalo.
            if automatico {
                self.montar_pra_viajar();
            }
            self.atualizar_auto_coleta();
            self.conduzir_auto_missao();
            if !eco {
                self.camera_controles();
            }
            self.enviar_input();
            self.sincroniza_preferencias();
            self.medir_rede();
            // Com o modo economia ligado nada e' desenhado: malha nova espera.
            // SALTO DO EU: a tela de carregando cobre o chao que ainda nao
            // existe — entao ela so' entra se o chao REALMENTE nao existe.
            //
            // Antes bastava a posicao pular mais de `SALTO_DE_TELEPORTE`, e
            // isso e' comum num link ruim: engasgo de rede, os snapshots
            // chegam em rajada, o corpo aparece longe. A tela subia e
            // `ui_pega_em` BLOQUEIA TODO O TOQUE enquanto ela esta' de pe' —
            // ate' 10 s por vez, o teto. Camera travada, clique no mundo
            // ignorado, e nada explicando. Foi o que o dono viu no iPhone:
            // "n consigo clicar pra andar e n consigo mover a camera".
            //
            // Trocar de zona e morrer continuam cobertos: la' o chao de fato
            // nao existe, e o teste abaixo diz sim.
            if std::mem::take(&mut self.world.salto_do_eu) {
                let chao_pronto = match (self.world.self_pos(), self.terreno.as_ref()) {
                    (Some(p), Some(t)) => {
                        let (feitos, de) = t.prontos_em(p, 1);
                        feitos == de
                    }
                    // Sem terreno nenhum (ilha de tiles) nao ha' o que esperar.
                    (_, None) => true,
                    // Sem corpo, esperar e' o certo: e' entrada de mundo.
                    (None, _) => false,
                };
                if !chao_pronto {
                    self.comecar_carregando();
                }
            }
            let carregando = self.carregando_desde.is_some();
            if let Some(t) = self.terreno.as_mut().filter(|_| !eco_ativa) {
                let centro = self.world.self_pos().unwrap_or(Vec2::ZERO);
                // The radius is the Graphics view distance (5 by default,
                // 80 units — more than the camera reaches at its band). O
                // orcamento de 3 por quadro existe pra o mundo aparecer em
                // duas piscadas em vez de travar meio segundo. Atras da tela
                // de carregando ninguem ve' a trava: gera muito mais.
                t.atualiza(
                    centro,
                    config_graficos::raio_terreno(),
                    if carregando { 40 } else { 4 },
                );
            }
            self.acompanhar_carregando();
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
                self.chat.push(format!("server list: {e}"));
                self.busca = None;
            }
            Err(std::sync::mpsc::TryRecvError::Empty) => {}
            Err(_) => self.busca = None,
        }
    }

    fn conectar(&mut self) {
        let Some(host) = self.host.clone() else {
            return;
        };
        // Trocando de zona: o pendente vai pelo canal velho antes de soltar.
        self.guardar_preferencias_agora();
        self.prefs = preferencias::Sincronia::default();
        self.world = World::default();
        self.ganhos = ganhos::Ganhos::default();
        self.habilidades = habilidades::Habilidades::default();
        self.evolucao_skills.progresso = Default::default();
        self.ficha_ui.limpar_pontos();
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
        self.mobs_ui = mobs_ui::MobsUi::default();
        self.construcoes = construcoes::Construcoes::default();
        self.rastro.limpa();
        self.menu.fechar();
        self.lojas.fechar();
        self.social = social_ui::Social::default();
        self.social_hud = social_hud::SocialHud::default();
        self.seguir = seguir::Seguir::default();
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

    /// Drives the reconnect: the next attempt when its wait is over, the
    /// error screen once `reconexao::DESISTE_APOS_S` has passed.
    fn acompanhar_reconexao(&mut self) {
        let agora = get_time();
        let Some(r) = self.reconexao.as_mut() else {
            return;
        };
        if r.esgotou(agora) {
            let motivo = format!("connection lost: {}", r.motivo);
            self.reconexao = None;
            self.net = None;
            self.tela = Tela::Erro(motivo);
            return;
        }
        if self.net.is_none() && self.host.is_some() && r.hora_de_tentar(agora) {
            r.tentou(agora);
            println!("[reconexao] tentativa {}", r.tentativas);
            self.conectar();
        }
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
                NetEvent::Disconnected(por_que) => {
                    // DROPPED MID-GAME: try to come back instead of throwing
                    // the player at the error screen (`reconexao`). A kick or
                    // a refused login already put the error up, so it is not
                    // `Jogando` any more and goes straight through.
                    let jogando = matches!(self.tela, Tela::Jogando);
                    let reentrando = matches!(self.tela, Tela::Conectando)
                        && (self.reconexao.is_some()
                            || (self.ja_entrou && self.personagem_atual.is_some()));
                    if reconexao::deve_reconectar(jogando, reentrando, &por_que) {
                        if self.personagem_atual.is_none() {
                            self.personagem_atual = self
                                .personagens
                                .get(self.selecionado)
                                .map(|p| p.name.clone());
                        }
                        match self.reconexao.as_mut() {
                            Some(r) => r.motivo = por_que,
                            None => {
                                println!("[reconexao] conexao caiu: {por_que}");
                                self.reconexao = Some(reconexao::Reconexao::nova(get_time(), por_que));
                            }
                        }
                        self.net = None;
                        self.tela = Tela::Conectando;
                    } else {
                        self.tela = Tela::Erro(por_que);
                    }
                }
                NetEvent::Message(msg) => self.on_message(*msg),
            }
            // Preserva o motivo do Kick: o fechamento do socket logo depois
            // não pode substituir o link de atualização por "the server closed the connection".
            if matches!(&self.tela, Tela::Erro(m) if atualizacao::protocolo_incompativel(m)) {
                self.net = None;
                break;
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
                        // Só pede sessão no PRIMEIRO login. Na troca de zona o
                        // cliente reconecta e reenvia usuário e senha — pedir
                        // ali emitiria um token por zona visitada.
                        lembrar: self.lembrar && !self.ja_entrou,
                    }),
                }
            }
            ServerMessage::CharacterList {
                chars,
                available_weapons,
                sessao,
            } => {
                self.ja_entrou = true;
                // GUARDA O QUE LEMBRAR, e só o que o jogador permitiu: o
                // usuário sempre (é ele que preenche o campo, e não é
                // segredo), a sessão só com `lembrar`, a senha nunca.
                if let Some(t) = sessao {
                    self.token_login = Some(t);
                }
                crate::lembranca::salva(&crate::lembranca::Prefs {
                    usuario: self.usuario.clone(),
                    sessao: self.token_login.clone(),
                    lembrar: self.lembrar,
                    // O idioma corrente, e nao o do arquivo: se o jogador
                    // trocou antes de entrar, e' essa escolha que fica.
                    idioma: shared::idioma::atual(),
                });
                self.personagens = chars;
                self.personagens
                    .sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
                self.armas = available_weapons;
                self.selecao_personagem.recebeu_lista(
                    &self.personagens,
                    &self.armas,
                    &mut self.selecionado,
                );
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
                // Sessão vencida (Google ou "remember me"): some do disco e
                // do jogo, e o jogador entra de novo. Sem apagar do disco, a
                // próxima abertura tentaria o MESMO token morto e cairia no
                // mesmo erro pra sempre.
                if self.token_login.take().is_some() {
                    crate::lembranca::esquece_a_sessao();
                    self.lembrar = false;
                    self.erro_login = Some("your session expired — sign in again".into());
                    self.tela = Tela::Login;
                } else {
                    self.tela = Tela::Erro(format!("login denied: {reason}"));
                }
            }
            ServerMessage::SemVisada { alvo } => {
                let agora = get_time();
                self.sem_visada = Some((alvo, agora));
                self.auto_combate.sem_visada(alvo, agora);
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
            ServerMessage::Kick { reason } => self.tela = Tela::Erro(format!("kicked: {reason}")),
            ServerMessage::FilaDeEntrada { posicao, total } => {
                self.tela = Tela::Fila { posicao, total };
            }
            ServerMessage::LoginOk { .. } => {
                if let Some(r) = self.reconexao.take() {
                    println!("[reconexao] voltou na tentativa {}", r.tentativas);
                    self.chat.push("Reconnected.".into());
                }
                self.tela = Tela::Jogando;
                self.comecar_carregando();
                // Caminho do teste automatizado (como o `MMO_CHAR`): entra
                // sozinho na dungeon pedida e liga o AUTO DUNGEON la' dentro.
                if let Some(c) = std::env::var("MMO_TESTE_DUNGEON")
                    .ok()
                    .and_then(|v| v.parse::<u16>().ok())
                {
                    self.envia(ClientMessage::Dungeon {
                        pedido: shared::dungeon::Pedido::EntrarSolo { conteudo: c },
                    });
                    self.dungeon.auto = true;
                }
            }
            // Pedra de minerio: a pedra nasce da semente dos dois lados, entao
            // o que o servidor manda e' so' QUAL sumiu. Sem isso o veio
            // limpo continuaria brilhando e o jogador nao teria como saber
            // onde ja' passou.
            ServerMessage::PedrasEsgotadas { colunas } => {
                if let Some(t) = &mut self.terreno {
                    for c in colunas {
                        t.marca_esgotada(c, true);
                    }
                }
            }
            ServerMessage::PedraEsgotada { coluna } => {
                if let Some(t) = &mut self.terreno {
                    t.marca_esgotada(coluna, true);
                }
            }
            ServerMessage::PedraVoltou { coluna } => {
                if let Some(t) = &mut self.terreno {
                    t.marca_esgotada(coluna, false);
                }
            }
            ServerMessage::InfoCanal {
                realm,
                canal,
                zona,
                jogadores,
                capacidade,
            } => {
                self.info = hud::Info {
                    realm,
                    canal,
                    zona,
                    jogadores,
                    capacidade,
                };
            }
            ServerMessage::TrocarZona { zona, host } => {
                // Zona e' outro PROCESSO: reconecta. O personagem e' o mesmo
                // (banco compartilhado no realm), entao a sessao refaz o
                // caminho de login sozinha e o jogador so' ve uma tela de
                // carregamento.
                self.chat.push(format!("heading to {zona}"));
                self.personagem_atual = self
                    .personagens
                    .get(self.selecionado)
                    .map(|p| p.name.clone())
                    .or_else(|| self.personagem_atual.clone());
                self.host = Some(host);
                self.conectar();
            }
            ServerMessage::MapChange {
                map_name,
                width,
                height,
                tiles,
                ..
            } => {
                self.auto_combate.parar();
                self.loja.fecha();
                self.interacao.cancela();
                self.missoes.fecha();
                self.auto_missao.parar();
                self.dialogo.fechar();
                self.rastro.limpa();
                // Ilha do arquipelago: o terreno nasce da SEMENTE, e nem o
                // arquivo de tiles nem um byte de rede entram nisso.
                self.zona_atual = map_name.clone();
                self.terreno = shared::terreno::def_da_zona(&map_name).map(terreno::Terreno::novo);
                render3d::define_noite(shared::kogen::e_kogen(&map_name) || shared::abissal::e_abissal(&map_name));
                render3d::define_abismo(shared::abissal::e_abissal(&map_name));
                // Mapa novo pra ilha nova; a viagem da ilha anterior morre junto.
                // Os filtros do mapa valem a sessao: sobrevivem a ilha nova.
                let filtros = std::mem::take(&mut self.mapa.filtros);
                self.mapa = mapa::Mapa::para(shared::terreno::def_da_zona(&map_name));
                self.mapa.filtros = filtros;
                self.ir_para.parar();
                self.construcoes =
                    construcoes::Construcoes::para(shared::terreno::def_da_zona(&map_name));
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
            ServerMessage::GuardaRoupa { guarda_roupa } => {
                self.guarda_roupa.define(&guarda_roupa);
                self.guarda_roupa_salvo = guarda_roupa;
            }
            ServerMessage::Mundo { ilhas } => {
                // A ilha em que se esta' tambem se atualiza por aqui.
                if let Some(z) = self.mapa.zona() {
                    if let Some(i) = ilhas.iter().find(|i| i.zona == z) {
                        self.mapa.define_chefes(i.chefes.clone());
                    }
                }
                self.mundo.define(ilhas);
            }
            ServerMessage::Colonia { aviso } => {
                use shared::colonia::AvisoColonia as A;
                match aviso {
                    A::Estado {
                        niveis,
                        horas,
                        colheita,
                        custos,
                        banco,
                        bau,
                        trabalhadores,
                        vagas,
                    } => {
                        let e = colonia_ui::Estado {
                            niveis,
                            horas,
                            colheita,
                            custos,
                            banco,
                            bau,
                            trabalhadores,
                            vagas,
                        };
                        // Pedido do porto ABRE; colher e melhorar so' atualizam
                        // o que ja' esta' aberto.
                        if self.colonia.aberto() {
                            self.colonia.atualizar(e);
                        } else {
                            self.fecha_paineis();
                            self.colonia.abrir(e);
                        }
                    }
                    A::Terreno {
                        plato,
                        assentamento: _,
                        trabalhadores,
                    } => {
                        // A ilha nao e' mais o MUNDO: ela e' a maquete do
                        // painel. Antes isto trocava `self.terreno`,
                        // `self.mapa` e `self.construcoes` — o jogador era
                        // teleportado pra dentro dela.
                        self.colonia.define_maquete(plato, trabalhadores);
                    }
                    A::Recusa(t) => self.chat.push(t),
                }
            }
            ServerMessage::Magica { aviso } => {
                // Fechar os outros paineis so' quando ele ABRE: dentro da
                // ilha este aviso chega a cada troca de ilhota, e fechar a
                // bolsa de quem esta' lutando seria pior que nao avisar nada.
                let antes = self.magica.aberto();
                self.magica.recebe(aviso, get_time());
                if !antes && self.magica.aberto() {
                    self.fecha_paineis();
                    self.magica.abrir();
                }
            }
            ServerMessage::Snapshot { snapshot } => {
                let novo_tick = self.tick != snapshot.tick;
                self.tick = snapshot.tick;
                self.world
                    .apply(snapshot.entered, snapshot.states, &snapshot.removed);
                if novo_tick { self.world.acertos(&snapshot.acertos, get_time()); }
                // Quem me bateu (bicho): a defesa da auto coleta revida.
                let eu = self.world.self_id;
                if let Some(a) = snapshot.acertos.iter().rev().find(|a| {
                    Some(a.alvo) == eu
                        && self
                            .world
                            .ents
                            .get(&a.atacante)
                            .is_some_and(|e| e.meta.tag == shared::EntityTag::Enemy)
                }) {
                    self.agressor = Some((a.atacante, get_time()));
                }
            }
            ServerMessage::Chat { from, text } => {
                // System lines are translated BEFORE the "SYS: " prefix: with
                // it, the whole line matches no dictionary entry and reaches
                // the screen in whatever language the server wrote it.
                // Player chat goes as typed.
                let text = if from == "SYS" {
                    shared::idioma::tr(&text).into_owned()
                } else {
                    text
                };
                if from == "SYS" && self.aceite_pendente.is_some_and(|t| get_time() - t < 3.0) {
                    self.aceite_pendente = None;
                    self.menu_missoes.aviso = Some((text.clone(), get_time() + 6.0));
                    self.habilidades.aviso(text.clone());
                }
                self.chat.push(format!("{from}: {text}"));
            }
            // O que a bolsa mostra. O servidor manda tudo no login e de novo a
            // cada mudanca; aqui so' se guarda.
            ServerMessage::InventoryUpdate { slots } => {
                // O que ENTROU sobe do personagem (coleta, loot, compra).
                let entrou = self.ganhos.bolsa_nova(&slots);
                if !entrou.is_empty() { sons::tocar(sons::Som::Item); }
                self.economia.itens_novos(&entrou);
                if self.auto_coleta.coletando_confirmado() {
                    self.auto_resumo.coletar_itens(&entrou);
                }
                self.bolsa.slots = slots;
            }
            ServerMessage::ResourceSources { items } => self.onde_obter.define(items),
            ServerMessage::ItemsConfig { items } => {
                self.mercado.vinculados =
                    items.iter().filter(|i| i.vinculado).map(|i| i.id).collect();
                crate::icones::define_vinculados(self.mercado.vinculados.iter().copied());
                self.bolsa.nomes = items.into_iter().map(|i| (i.id, i.name)).collect();
                self.social.nomes = self.bolsa.nomes.clone();
            }
            ServerMessage::Social { aviso } => self.social.receber(aviso),
            ServerMessage::PartyInviteReceived { from } => {
                self.chat.push(format!(
                    "{from} convidou você para um grupo. Abra Menu › Grupo."
                ));
                self.social.convite = Some((from, get_time() + 60.0));
            }
            ServerMessage::PartyUpdate { members } => {
                self.social.grupo = members;
                self.missoes.membros_grupo = self.social.grupo.len();
                self.social.convite = None;
            }
            ServerMessage::MercadoLista {
                anuncios,
                pagina,
                tem_mais,
            } => self.mercado.lista(anuncios, pagina, tem_mais),
            ServerMessage::MercadoMeus {
                anuncios,
                historico,
                tp,
                compras_de_vagas,
            } => self.mercado.meus(anuncios, historico, tp, compras_de_vagas),
            ServerMessage::MercadoEntregas { cartas, tp } => self.mercado.entregas(cartas, tp),
            ServerMessage::PrecosDoMercado { tabela } => self.mercado.precos = tabela.mapa(),
            ServerMessage::Presenca { aviso } => {
                if let Some(t) = self.presenca.receber(aviso, &self.bolsa.nomes) {
                    self.chat.push(t);
                }
            }
            ServerMessage::Loja { aviso } => {
                self.montarias.receber(&aviso, get_time());
                if let shared::loja::AvisoLoja::Invocacao { premio } = &aviso {
                    self.fecha_paineis();
                    self.invocacao.abrir(premio.clone(), get_time());
                }
                if let shared::loja::AvisoLoja::Invocacoes { premios } = &aviso {
                    self.fecha_paineis();
                    self.invocacao.abrir_varias(premios.clone(), get_time());
                }
                if let Some(t) = self.loja_tp.receber(aviso) {
                    self.chat.push(t);
                }
            }
            ServerMessage::Dungeon { aviso } => {
                if self.mapa.zona() == Some(shared::arena::ZONA) {
                    if let Some(terreno) = self.terreno.as_mut() {
                        match &aviso {
                            shared::dungeon::Aviso::Instancia { conteudo, .. } => {
                                if let Some(bioma) = shared::dungeon::conteudo(*conteudo)
                                    .and_then(|c| shared::terreno::def_da_zona(c.zona))
                                    .map(|def| def.bioma)
                                {
                                    terreno.tema_da_dungeon(bioma, Some(*conteudo));
                                }
                            }
                            shared::dungeon::Aviso::Saiu => {
                                terreno.tema_da_dungeon(shared::terreno::Bioma::Floresta, None);
                            }
                            _ => {}
                        }
                    }
                }
                if let shared::dungeon::Aviso::Correio { cartas } = &aviso {
                    self.social.correio_dungeon = cartas.clone();
                }
                if let Some(t) = dungeon_ui::DungeonUi::texto_pro_chat(&aviso) {
                    self.chat.push(t);
                }
                for pedido in self.dungeon.aviso(aviso, get_time()) {
                    self.envia(pedido);
                }
            }
            ServerMessage::MercadoResultado { ok, texto } => {
                // Venda fechada chega com o painel fechado: o chat avisa.
                self.chat.push(format!("Market: {texto}"));
                for pedido in self.mercado.resultado(ok, texto, get_time()) {
                    self.envia(pedido);
                }
            }
            ServerMessage::StatsUpdate { stats, equipment } => {
                self.bolsa.stats = Some(stats);
                self.bolsa.equip = equipment;
            }
            ServerMessage::StatPointsUpdate {
                unspent,
                allocated,
                emprestados,
            } => {
                self.ficha_ui
                    .atualizar_pontos(unspent, allocated, emprestados);
            }
            ServerMessage::ProficienciesUpdate { xp } => {
                self.ficha_ui.atualizar_proficiencias(xp);
            }
            ServerMessage::GoldUpdate { gold } => self.bolsa.ouro = gold,
            ServerMessage::VaultOpen { slots } => {
                // Como o Capitao: com a oferta de missao do Banqueiro na
                // frente, o banco espera o dialogo fechar.
                if self.dialogo.aberto {
                    self.banco_pendente = Some(slots);
                } else {
                    self.fecha_paineis();
                    self.missoes.fecha();
                    self.banco.abrir(slots);
                }
            }
            ServerMessage::VaultUpdate { slots } => self.banco.cofre = slots,
            ServerMessage::VaultClose => self.banco.fechar(),
            ServerMessage::Armazem {
                bolsa_extra,
                banco_extra,
            } => {
                self.bolsa.extra = bolsa_extra;
                self.banco.bolsa_extra = bolsa_extra;
                self.banco.banco_extra = banco_extra;
            }
            ServerMessage::CraftRecipes { recipes } => self.craft.define_receitas(recipes),
            ServerMessage::CraftResultado {
                ok,
                motivo,
                item_id,
                ..
            } => {
                let txt = if ok {
                    format!("Created: {}", self.bolsa.nome(item_id))
                } else {
                    format!("Craft refused: {motivo}")
                };
                self.craft.resultado(ok, txt.clone(), get_time());
                self.chat.push(txt);
            }
            ServerMessage::AprimorarResultado {
                ok,
                texto,
                item_id,
                grau,
                tier,
            } => {
                let txt = if ok {
                    format!(
                        "Upgraded: {} is now {} {}!",
                        self.bolsa.nome(item_id),
                        oficina_ui::nome_da_cor(grau),
                        oficina_ui::romano(tier)
                    )
                } else {
                    format!("Not upgraded: {texto}")
                };
                self.craft.resultado(ok, txt.clone(), get_time());
                self.chat.push(txt);
            }
            ServerMessage::EscolhaNoNpc {
                npc_eid,
                nome,
                funcao,
            } => {
                self.dialogo.fechar();
                // COM O AUTO MISSÃO RODANDO, A ESCOLHA JÁ FOI FEITA.
                //
                // O NPC que tem missão E função (loja, forja, barco, banco)
                // pergunta antes. Mas o auto missão fica esperando o diálogo,
                // que nunca abre: ele estoura o prazo, interage de novo, o
                // menu reabre, e trava. O dono: "quando conversa com um NPC
                // que tem que escolher entre missão e viagem, ou missão e
                // loja, aí também trava".
                //
                // Quem tocou "fazer a missão" já disse o que queria. O menu
                // continua aparecendo pra quem clicou no NPC na mão.
                if self.auto_missao.ativo() {
                    self.envia(ClientMessage::EscolherNoNpc {
                        npc_eid,
                        missao: true,
                    });
                } else {
                    self.escolha_npc.abrir(npc_eid, nome, funcao);
                }
            }
            ServerMessage::Viagem { destinos, colonia } => {
                self.tem_colonia = colonia;
                // O Capitao pode abrir com uma oferta de missao na frente: o
                // menu espera o dialogo fechar.
                if self.dialogo.aberto {
                    self.viagem_pendente = Some(destinos);
                } else {
                    self.fecha_paineis();
                    self.abrir_viagem(destinos);
                }
            }
            ServerMessage::CombinarResultado {
                entrada,
                tentativas,
                sucessos,
                texto,
            } => {
                let saida = shared::combinar::receita(entrada)
                    .map(|r| self.bolsa.nome(r.saida))
                    .unwrap_or_default();
                let (txt, ok) =
                    if shared::combinar::receita(entrada).is_some_and(|r| r.chance == 100) {
                        oficina_ui::texto_da_sintese(tentativas, sucessos, &saida, &texto)
                    } else {
                        oficina_ui::texto_do_resultado(tentativas, sucessos, &saida, &texto)
                    };
                self.craft.resultado(ok, txt.clone(), get_time());
                self.chat.push(txt);
            }
            ServerMessage::RefinoResultado {
                resultado,
                nivel,
                item_id,
                motivo,
            } => {
                let nome = self.bolsa.nome(item_id);
                self.forja
                    .resultado(resultado, nivel, &nome, &motivo, get_time());
                self.chat
                    .push(forja_ui::texto_do_resultado(resultado, nivel, &nome, &motivo).0);
            }
            // O Ferreiro da vila abre a mesma Forja do HUD.
            ServerMessage::BlacksmithOpen => {
                self.interacao.cancela();
                self.craft.fechar();
                self.forja.abrir();
            }
            ServerMessage::BuffXp { ate } => self.bonus_xp_ate = ate,
            ServerMessage::BuffsDeDrop {
                fortuna_ate,
                sorte_ate,
            } => {
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
            ServerMessage::Recuperaveis {
                mortes,
                gratis_restantes,
            } => {
                self.morte.recuperaveis(mortes, gratis_restantes);
            }
            ServerMessage::RecuperarXpResultado { ok, motivo, .. } => {
                self.chat.push(motivo.clone());
                self.morte.resultado(ok, motivo);
            }
            ServerMessage::ShopOpen {
                items, vendor_id, ..
            } => {
                self.interacao.cancela();
                self.missoes.fecha();
                self.auto_missao.parar();
                self.dialogo.fechar();
                self.mapa.viagem.cancelar();
                self.ir_para.parar();
                self.loja.abre(vendor_id, items);
            }
            ServerMessage::MagicShopOpen { items, vendor_id } => {
                self.interacao.cancela();
                self.missoes.fecha();
                self.auto_missao.parar();
                self.dialogo.fechar();
                self.mapa.viagem.cancelar();
                self.ir_para.parar();
                self.loja.abre_magica(vendor_id, items);
            }
            ServerMessage::ShopClose => self.loja.fecha(),
            ServerMessage::ShopTradeResult { ok, reason } => {
                self.chat.push(format!("shop: {reason}"));
                if ok {
                    self.loja.sucesso(reason);
                } else {
                    self.loja.avisa(reason);
                }
            }
            ServerMessage::ProgressUpdate { xp, level } => {
                // SUBIU: o cliente ja' sabia o nivel de antes, entao a
                // comemoracao nao custa um byte de rede. `> 0` exclui o
                // primeiro `ProgressUpdate` do login, que nao e' subida.
                if level > self.ficha.nivel && self.ficha.nivel > 0 {
                    self.subiu_de_nivel.dispara();
                }
                self.bolsa.nivel = level;
                self.ficha.xp = xp;
                self.ficha.nivel = level;
            }
            ServerMessage::ManaUpdate { current } => self.ficha.mp = Some(current),
            ServerMessage::StaminaUpdate { current } => self.ficha.vigor = Some(current),
            ServerMessage::DashRecarga { segundos } => {
                self.dash_recarga = (get_time() + segundos as f64, segundos);
                if let Some(e) = self
                    .world
                    .self_id
                    .and_then(|id| self.world.ents.get_mut(&id))
                {
                    e.dash_visual_ate = get_time() + shared::DASH_DURATION as f64;
                    e.skill = None;
                    e.combo = None;
                    e.combo_ant = None;
                    e.ferido = None;
                    e.pulo_local = 0.0;
                }
            }
            ServerMessage::SkillsConfig { skills } => self.habilidades.catalogo = skills,
            ServerMessage::ProgressoDeSkills { progresso } => {
                let ganho = self.ganhos.energia_nova(progresso.energia);
                if self.auto_coleta.coletando_confirmado() {
                    if let Some(qtd) = ganho { self.auto_resumo.coletar_energia(qtd); }
                }
                self.bolsa.energia = progresso.energia;
                self.evolucao_skills.progresso = progresso;
            }
            ServerMessage::ResultadoDeEvolucao { ok: _, texto } => {
                self.evolucao_skills.resultado(texto.clone());
                self.chat.push(texto);
            }
            ServerMessage::SkillsState { cooldowns, busy_s } => {
                self.habilidades.estado(cooldowns, busy_s)
            }
            ServerMessage::SkillRejected { skill_id, motivo } => {
                sons::tocar(sons::Som::Recusa);
                self.habilidades.rejeitada(skill_id, motivo)
            }
            ServerMessage::MobAttackFx {
                attacker,
                target,
                dir,
                impact_s,
            } => {
                let alvo = target
                    .and_then(|id| self.world.ents.get(&id))
                    .map(|e| e.render_pos);
                if let Some(e) = self.world.ents.get(&attacker) {
                    sons::mob(sons::ataque_mob(e.meta.kind), e.render_pos,
                        self.world.self_pos(), target.is_some() && target == self.world.self_id);
                }
                if let Some(e) = self.world.ents.get_mut(&attacker) {
                    e.golpe = 0.0;
                    e.ataque_mob = Some((target, 0.0, impact_s));
                    e.mira = Some((
                        alvo.unwrap_or(e.render_pos + vec2(dir[0], dir[1])),
                        impact_s + 0.3,
                    ));
                }
            }
            ServerMessage::SkillCastFx {
                skill_id,
                caster_eid: Some(id),
                caster_pos,
                target_pos,
                target_eid,
                tier,
                ..
            } => {
                let de = vec2(caster_pos.x, caster_pos.y);
                sons::skill(skill_id, false, de, self.world.self_pos(), self.world.self_id == Some(id));
                let alvo = vec2(target_pos.x, target_pos.y);
                self.habilidades
                    .efeito(skill_id, id, de, alvo, target_eid, false, tier);
                if let (Some(e), Some(s)) = (
                    self.world.ents.get_mut(&id),
                    self.habilidades.catalogo.iter().find(|s| s.id == skill_id),
                ) {
                    e.skill = Some((skill_id, 0.0, s.impacto_em()));
                    // LEAP (1): a real jump — the whole body on the jump's
                    // gravity arc, launched so it lands at the impact, when
                    // the server moves it there and hits. The pose alone
                    // could only bend the legs, and it read as a dash.
                    if s.e_salto() {
                        e.vel_y = shared::gravidade() * s.impacto_em() * 0.5;
                        e.voando = true;
                    }
                    e.combo = None;
                    e.combo_ant = None;
                    e.sacada = 1.0;
                    if de.distance(alvo) > 0.01 {
                        e.mira = Some((alvo, s.impacto_em() + 0.3));
                    }
                }
            }
            ServerMessage::SkillImpactFx {
                skill_id,
                caster_eid,
                caster_pos,
                target_pos,
                tier,
            } => {
                sons::skill(skill_id, true, vec2(target_pos.x,target_pos.y), self.world.self_pos(), self.world.self_id == Some(caster_eid));
                // MY Danca landed: the same instant the server opens the
                // katana's THIRST lifesteal window.
                if skill_id == 5 && self.world.self_id == Some(caster_eid) {
                    self.habilidades.open_thirst();
                }
                self.habilidades.efeito(
                    skill_id,
                    caster_eid,
                    vec2(caster_pos.x, caster_pos.y),
                    vec2(target_pos.x, target_pos.y),
                    None,
                    true,
                    tier,
                );
            }
            ServerMessage::SkillCastCancel {
                caster_eid,
                skill_id,
            } => {
                self.habilidades.cancelar(
                    caster_eid,
                    skill_id,
                    self.world.self_id == Some(caster_eid),
                );
                if let Some(e) = self.world.ents.get_mut(&caster_eid) {
                    e.skill = None;
                    e.combo = None;
                    e.combo_ant = None;
                }
            }
            // Missoes: a oferta abre a janela do NPC com quem se acabou de
            // falar; log, progresso e givers so' atualizam o estado.
            ServerMessage::QuestOffer {
                giver_id,
                giver_name,
                quests,
                ..
            } => {
                self.interacao.cancela();
                self.loja.fecha();
                self.ao_receber_oferta(giver_id, giver_name, quests);
            }
            ServerMessage::QuestLog { quests } => self.missoes.define_log(quests),
            ServerMessage::QuestUpdate {
                quest_id,
                progress,
                status,
            } => {
                use shared::historia;
                let anterior = self.missoes.log.iter().find(|q| q.id == quest_id)
                    .map(|q| (q.progress, q.status));
                if status == shared::quests::quest_status::TURNED_IN {
                    if let Some(def) = shared::quests::quest_by_id(quest_id) {
                        if anterior.is_none_or(|(_, antigo)| antigo != status) {
                            self.auto_resumo.concluir(def, &self.bolsa.nomes);
                        }
                        let itens = [(def.reward_item, def.reward_item_qty), (def.reward_item2, def.reward_item2_qty)]
                            .into_iter().filter(|(id, qtd)| *id != 0 && *qtd > 0)
                            .map(|(id, qtd)| (id, qtd as u32)).collect();
                        self.recompensas.mostrar("QUEST COMPLETE".into(), def.title.into(), itens,
                            def.reward_cobre, shared::progressao::xp_da_quest(def), def.reward_faction_points);
                    }
                    self.quest_entregues.entry(quest_id).or_insert(0);
                    // O CONTADOR do topo: sobe a cada missao concluida, FIXADA
                    // OU NAO. Terminar uma missao so' aparecia no chat, que
                    // rola e some — o jogador fechava cinco e nao via nenhuma.
                    self.missoes.concluidas += 1;
                    self.missoes.concluida_em = get_time();
                    self.missoes.fixadas.remove(&quest_id);
                }
                // A historia passou pro proximo passo com a auto missao nela:
                // segue sozinha pro novo.
                let segue = status == shared::quests::quest_status::ACTIVE
                    && historia::e_da_historia(quest_id)
                    && self
                        .auto_missao
                        .quest
                        .is_some_and(|q| q != quest_id && historia::e_da_historia(q));
                self.missoes.atualiza(quest_id, progress, status);
                if status != 255 && anterior != Some((progress, status)) {
                    sons::tocar(if status == shared::quests::quest_status::TURNED_IN { sons::Som::Recompensa } else if status == shared::quests::quest_status::READY { sons::Som::Pronta } else { sons::Som::Progresso });
                    if let Some(def) = shared::quests::quest_by_id(quest_id) {
                        self.missoes.aviso_progresso = if status == shared::quests::quest_status::TURNED_IN {
                            format!("Quests completed: {} · {}", self.missoes.concluidas, def.title)
                        } else if status == shared::quests::quest_status::READY {
                            format!("{} · {}/{} · Ready!", def.title, def.obj_count, def.obj_count)
                        } else {
                            format!("{} · {}/{}", def.title, progress.min(def.obj_count), def.obj_count)
                        };
                        self.missoes.aviso_em = get_time();
                    }
                }
                if segue && shared::quests::quest_by_id(quest_id).is_some_and(menu_missoes::automatizavel) {
                    if let Some(d) = shared::quests::quest_by_id(quest_id) {
                        self.auto_missao
                            .iniciar(quest_id, d.title.to_string(), get_time());
                    }
                }
            }
            ServerMessage::BossRewards { items } => {
                self.recompensas.mostrar("BOSS DEFEATED".into(), "Loot on the ground · pick the items up".into(), items, 0, 0, 0);
            }
            ServerMessage::QuestEstado { entregues, faccao } => {
                self.quest_entregues = entregues.into_iter().collect();
                self.faccao_qid = faccao;
            }
            ServerMessage::MapaDaIlha {
                zonas,
                recursos,
                nomes,
                rendimentos,
                mobs,
                chefes,
            } => {
                self.mapa.define_info(zonas, recursos, nomes, rendimentos);
                self.mapa.define_chefes(chefes);
                self.mobs_ui.catalogo(mobs);
            }
            ServerMessage::Telegrafico {
                id,
                chefe,
                forma,
                centro,
                dir,
                carga_s,
            } => {
                let agora = get_time();
                self.telegrafos
                    .comeca(id, chefe, forma, centro, dir, carga_s, agora);
                // O chefe arma o golpe no desenho (preparacao ate' o impacto).
                if let Some(e) = self.world.ents.get_mut(&chefe) {
                    let (c, d) = (vec2(centro[0], centro[1]), vec2(dir[0], dir[1]));
                    e.carga_chefe = Some(chefe_anim::Carga::nova(
                        &forma,
                        c,
                        d,
                        e.render_pos,
                        carga_s,
                        agora,
                    ));
                }
            }
            ServerMessage::TelegraficoFim { id, impacto } => {
                let agora = get_time();
                if let Some((chefe, forma, centro, dir)) =
                    self.telegrafos.termina(id, impacto, agora)
                {
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
                            for (k, p) in chefe_anim::pontos_de_impacto(&forma, centro, dir)
                                .into_iter()
                                .enumerate()
                            {
                                lascas::explosao(
                                    vec3(p.x, t.altura(p.x, p.y) + 0.1, p.y),
                                    cor,
                                    id.wrapping_mul(31) ^ k as u32,
                                );
                            }
                        }
                        // Perto do golpe: a camera sente (curto e com teto).
                        if self
                            .world
                            .self_pos()
                            .is_some_and(|eu| eu.distance(centro) <= forma.alcance() + 8.0)
                        {
                            self.tremor = (
                                chefe_anim::TREMOR_FORCA,
                                agora + chefe_anim::TREMOR_S as f64,
                            );
                        }
                    }
                }
            }
            ServerMessage::QuestGivers { available } => self.missoes.define_givers(available),
            ServerMessage::Rota { pontos, destino } => {
                self.rastro.define(
                    pontos.into_iter().map(|p| vec2(p[0], p[1])).collect(),
                    vec2(destino[0], destino[1]),
                );
            }
            ServerMessage::QuestDestino {
                quest_id,
                tipo,
                pos,
                raio,
                npc_eid,
            } => {
                use shared::quests::destino_tipo;
                // Criar e refinar nao se faz andando: abre o painel e a auto
                // missao para (quem cria e' o jogador).
                if tipo == destino_tipo::PAINEL_DUNGEON {
                    // Vencer dungeon nao se faz andando: abre o painel na dungeon
                    // do passo (o id vem no `raio`).
                    if self.auto_missao.quest == Some(quest_id) {
                        self.fecha_paineis();
                        for pedido in self.dungeon.abrir_em(raio as u16) {
                            self.envia(pedido);
                        }
                        // DIZ POR QUE PAROU. O craft e a forja já diziam; a
                        // dungeon parava calada, e auto missão que para sem
                        // explicar é indistinguível de auto missão travada —
                        // que foi como o dono descreveu.
                        self.chat
                            .push("Missão: vença a dungeon (o auto não luta por você).".into());
                        self.auto_missao_pula(quest_id);
                    }
                } else if tipo == destino_tipo::TUTORIAL {
                    // Passo tutorial nao anda: abre onde se faz (a barra, o
                    // mapa) ou diz o gesto.
                    if self.auto_missao.quest == Some(quest_id) {
                        use shared::quests::tutorial as t;
                        self.auto_missao.parar();
                        let acao = raio as u16;
                        match acao {
                            t::POCAO_LIMIAR => {
                                self.fecha_paineis();
                                self.config_barra.abrir(Some(0));
                            }
                            t::MAPA_IR => {
                                self.fecha_paineis();
                                self.mapa.aberto = true;
                            }
                            t::PONTO_ATRIBUTO => {
                                self.fecha_paineis();
                                self.ficha_ui.abrir();
                            }
                            t::EVOLUIR_SKILL => {
                                self.fecha_paineis();
                                self.evolucao_skills.abrir(self.conjunto_equipado());
                            }
                            _ => {}
                        }
                        self.foco_tutorial = Some((quest_id, get_time()));
                        self.chat.push(format!("Tutorial: {}", t::instrucao(acao)));
                        if matches!(acao, t::PONTO_ATRIBUTO | t::EVOLUIR_SKILL) {
                            self.auto_coleta.parar();
                            self.auto_combate.parar();
                            self.mapa.viagem.cancelar();
                        } else {
                            self.auto_missao_pula(quest_id);
                        }
                    }
                } else if tipo == destino_tipo::PAINEL_CRAFT || tipo == destino_tipo::PAINEL_FORJA {
                    if self.auto_missao.quest == Some(quest_id) {
                        self.auto_missao.parar();
                        if tipo == destino_tipo::PAINEL_CRAFT {
                            self.forja.fechar();
                            // Na aba do que DA' pra fazer agora, e nao na
                            // primeira: a missao pede "crie um equipamento", e
                            // cair numa aba onde nada e' possivel e' mandar o
                            // jogador procurar o que a tela ja' sabe.
                            self.craft
                                .abrir_no_que_da(&self.bolsa.slots, self.bolsa.nivel);
                            self.chat
                                .push("Quest: craft a piece of gear in Craft.".into());
                        } else {
                            self.craft.fechar();
                            self.forja.abrir();
                            self.chat
                                .push("Quest: try refining a piece at the Forge.".into());
                        }
                        self.auto_missao_pula(quest_id);
                    }
                } else {
                    let npc = npc_eid.map(|e| shared::EntityId(e as u32));
                    if let Some(aviso) = self.auto_missao.destino_recebido(
                        quest_id,
                        tipo,
                        vec2(pos[0], pos[1]),
                        raio,
                        npc,
                        get_time(),
                    ) {
                        self.chat.push(aviso);
                    }
                    // A trava de nível oferece três rotas de XP. O jogador
                    // escolhe uma delas, ou fecha o guia e continua livre.
                    if tipo == shared::quests::destino_tipo::TRAVA {
                        self.dica_da_trava = None;
                        self.foco_tutorial = None;
                        foco::limpar();
                        let alvo = self
                            .missoes
                            .log
                            .iter()
                            .find(|q| q.id == quest_id)
                            .map_or(self.bolsa.nivel + 1, |q| q.obj_count);
                        self.nivel_ui.abrir(alvo);
                    }
                }
            }
            ServerMessage::NoDeColeta { no } => {
                let no = no.map(|(k, onde, c, t)| (k, vec2(onde[0], onde[1]), vec2(c[0], c[1]), t));
                if let auto_coleta::Acao::Ir(p) = self.auto_coleta.no_recebido(no, get_time()) {
                    self.mapa.viagem.iniciar(p, get_time());
                }
            }
            ServerMessage::ColetaEstado {
                tipo,
                intervalo_s,
                progresso,
                centro,
                pausado,
            } => {
                self.coleta_hud
                    .recebe(tipo, intervalo_s, progresso, centro, pausado, get_time());
                let coletava = self.auto_coleta.coletando_confirmado();
                self.auto_coleta.estado_coleta(tipo, pausado, get_time());
                if tipo == shared::protocol::COLETA_PARADA && coletava {
                    self.auto_missao.recurso_parou();
                }
            }
            ServerMessage::PocaoGrupo {
                grupo,
                recarga_s,
                cura_s,
            } => {
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
            self.parar_seguir();
            self.social_hud.pacifico = None;
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
                    centro,
                    self.cam_yaw,
                    self.cam_zoom,
                    self.cam_pitch,
                    self.cam_altura,
                    &f,
                );
                let alvo = render3d::pick(&self.world, &vista, vec2(mx, my), 48.0);
                // Clicou no vazio? Entao foi no CHAO.
                let destino = if alvo.is_none() {
                    terreno.and_then(|t| {
                        let (o, d) = vista.raio(vec2(mx, my));
                        t.onde_o_raio_bate_no_andar(o, d, 260.0)
                    })
                } else {
                    None
                };
                (alvo, destino)
            };
            // The floor the tap landed on (Kōgen-tō's expressway deck).
            let camada_do_toque = escolhido.1.map_or(0, |d| d.1);
            let escolhido = (escolhido.0, escolhido.1.map(|d| d.0));
            // So' bicho e gente viram alvo. Clicar no saque e' ir BUSCAR (ele
            // e' pego por proximidade); clicar em NPC nao mira ninguem.
            let tag = escolhido
                .0
                .and_then(|id| self.world.ents.get(&id))
                .map(|e| (e.meta.tag, e.render_pos));
            // Um toque em chao/saque/NPC ja' substitui a rota de combate.
            // Nao deixar o PararRota do alvo antigo apagar a rota nova.
            if !matches!(
                tag,
                Some((shared::EntityTag::Enemy | shared::EntityTag::Player, _))
            ) {
                self.aproximando_alvo = None;
            }
            // Qualquer clique novo desfaz o "ir falar com o NPC" anterior.
            self.interacao.cancela();
            match tag {
                Some((shared::EntityTag::Enemy | shared::EntityTag::Player, _)) => {
                    self.alvo = escolhido.0
                }
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
                let no = self
                    .terreno
                    .as_ref()
                    .and_then(|t| t.coletavel_perto(p, 0.8));
                match (no, self.world.self_pos()) {
                    (Some((coluna, c, _tipo, raio)), Some(eu)) => {
                        let dir = (eu - c).try_normalize().unwrap_or(Vec2::X);
                        let onde = c + dir
                            * (raio + shared::ENTITY_RADIUS + shared::COLETA_ALCANCE_UN * 0.5);
                        self.auto_coleta.parar();
                        self.coleta_pendente = Some((coluna, c, raio));
                        self.envia(ClientMessage::MoverPara {
                            x: onde.x,
                            z: onde.y,
                        });
                    }
                    _ if camada_do_toque != 0 => self.envia(ClientMessage::MoverParaCamada {
                        x: p.x,
                        z: p.y,
                        camada: camada_do_toque,
                    }),
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
        let Some(eu) = self.world.self_pos() else {
            return;
        };
        if eu.distance(npc) <= loja::PERTO {
            self.interagir_agora(id);
        } else {
            let destino = npc + (eu - npc).normalize_or_zero() * 2.0;
            self.envia(ClientMessage::MoverPara {
                x: destino.x,
                z: destino.y,
            });
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
        let npc = self
            .interacao
            .npc
            .and_then(|id| self.world.ents.get(&id))
            .map(|e| e.render_pos);
        if let Some(eu) = eu {
            if let Some(id) = self.interacao.acompanhar(eu, npc) {
                self.interagir_agora(id);
            }
        }
        let vendedor = self
            .loja
            .vendedor
            .and_then(|id| self.world.ents.get(&id))
            .map(|e| e.render_pos);
        self.loja.conferir_distancia(eu, vendedor);
        let mestre = self
            .missoes
            .npc
            .and_then(|id| self.world.ents.get(&id))
            .map(|e| e.render_pos);
        self.missoes.conferir_distancia(eu, mestre);
    }

    /// Chegou num NPC: se ele e' o alvo de uma missao "fale com", abre a
    /// conversa (a missao conta ao TERMINAR o dialogo); senao, `Interact` como
    /// sempre.
    fn interagir_agora(&mut self, id: shared::EntityId) {
        self.ultimo_npc = Some(id);
        if let Some((quest_id, titulo, quem)) = self.conversa_de_missao(id) {
            let falas = shared::quests::falas(quest_id, shared::quests::momento::CONVERSA);
            self.dialogo.abrir(
                &quem,
                &titulo,
                &falas,
                dialogo::Fim::Conversa { quest_id, npc: id },
                String::new(),
            );
            return;
        }
        self.envia(ClientMessage::Interact {
            target_eid: Some(id.0 as u64),
        });
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
        Some((
            q.id,
            q.title.clone(),
            e.meta.name.clone().unwrap_or_default(),
        ))
    }

    /// Resposta do Mestre (`QuestOffer`): entrega pronta vira dialogo de
    /// "Receive"; missao nova vira dialogo de "Accept" — ou, em auto missao
    /// logo depois de receber, ja' e' aceita e o personagem segue; sem nada a
    /// fazer, a janela de sempre.
    fn ao_receber_oferta(
        &mut self,
        giver: u16,
        quem: String,
        quests: Vec<shared::quests::QuestNet>,
    ) {
        use shared::quests::{falas, momento};
        // Entrega-se a quem deu: o Mestre OU o NPC da vila da cadeia.
        let pronta = {
            let slots = &self.bolsa.slots;
            self.missoes
                .log
                .iter()
                .find(|q| {
                    q.giver == giver && missoes::pronta(q, &|id| missoes::na_bolsa(slots, id))
                })
                .cloned()
        };
        self.missoes
            .guarda_oferta(self.ultimo_npc, giver, quem.clone(), quests);
        let proxima = self.missoes.oferta().first().cloned();
        if let Some(q) = pronta {
            let r = missoes::recompensa(&q, &self.bolsa.nomes);
            self.dialogo.abrir(
                &quem,
                &q.title,
                &falas(q.id, momento::ENTREGA),
                dialogo::Fim::Entrega { quest_id: q.id },
                r,
            );
        } else if let Some(q) = proxima {
            if self.auto_missao.etapa() == Some(auto_missao::Etapa::AguardandoProxima)
                && shared::quests::quest_by_id(q.id).is_some_and(menu_missoes::automatizavel) {
                self.envia(ClientMessage::AcceptQuest { quest_id: q.id });
                self.chat.push(format!("New quest: {}", q.title));
                self.auto_missao.iniciar(q.id, q.title.clone(), get_time());
            } else {
                let r = missoes::recompensa(&q, &self.bolsa.nomes);
                self.dialogo.abrir(
                    &quem,
                    &q.title,
                    &falas(q.id, momento::OFERTA),
                    dialogo::Fim::Oferta { quest_id: q.id },
                    r,
                );
            }
        } else if !self.auto_missao.ativo() {
            self.missoes.abre_janela();
        }
    }

    /// Quanto tempo o foco do tutorial fica de pe' depois de armado. E' a
    /// valvula de escape: se o alvo sumir da tela (painel que nao abriu, botao
    /// que so' aparece com item), o escuro se desfaz sozinho em vez de deixar
    /// o jogador preso.
    const FOCO_TUTORIAL_S: f64 = 90.0;

    /// O passo TUTORIAL armado pede o foco. A lista vai do botao mais FUNDO
    /// (dentro do painel) ao mais raso (o MENU): ganha o primeiro que estiver
    /// Esvazia a fila sem contar nada: o jogador mandou outra coisa.
    fn parar_fila(&mut self) {
        self.fila_de_missoes.clear();
        self.fila_feitas = 0;
        self.fila_puladas = 0;
    }

    /// Puxa a próxima da fila quando a auto missão para.
    ///
    /// Por SONDAGEM, e não por gancho nos lugares que param a auto missão:
    /// são mais de dez, entre entregar, recusar, trocar de zona, morrer e
    /// abrir outra coisa. Um gancho esquecido num deles deixaria a fila
    /// pendurada pra sempre, e é o tipo de esquecimento que ninguém vê.
    ///
    /// Missão que não está no log (não foi aceita, ou já foi entregue) é
    /// PULADA: a fila nunca trava, que foi a escolha do dono.
    fn tocar_fila(&mut self) {
        if self.fila_de_missoes.is_empty() || self.auto_missao.ativo() {
            return;
        }
        while let Some(id) = self.fila_de_missoes.first().copied() {
            self.fila_de_missoes.remove(0);
            let tem = self
                .missoes
                .log
                .iter()
                .any(|q| q.id == id && shared::quests::quest_by_id(id)
                    .is_some_and(|d| menu_missoes::automatizavel(d) && matches!(q.status, shared::quests::quest_status::ACTIVE | shared::quests::quest_status::READY)));
            if tem {
                self.fila_feitas += 1;
                self.iniciar_auto_missao(id);
                return;
            }
            self.fila_puladas += 1;
        }
        let (f, pl) = (self.fila_feitas, self.fila_puladas);
        if f + pl > 0 {
            self.chat.push(if pl > 0 {
                format!("Fila terminada: {f} feita(s), {pl} pulada(s).")
            } else {
                format!("Queue finished: {f} quest(s).")
            });
        }
        self.fila_feitas = 0;
        self.fila_puladas = 0;
    }

    /// O caminho até a Ilha Mágica, aceso enquanto a dica da trava vale.
    fn foco_da_trava(&mut self) {
        let Some(armado) = self.dica_da_trava else {
            return;
        };
        if get_time() - armado > Self::FOCO_TUTORIAL_S {
            self.dica_da_trava = None;
            return;
        }
        // JÁ ESTÁ DENTRO? Então a dica não tem o que dizer.
        //
        // O dono: "na missão da Ilha Mágica, eu entrei dentro da ilha e
        // continua o destaque do tutorial me pedindo pra entrar na Ilha
        // Mágica". A dica é armada pela trava de nível e apontava o caminho
        // de ENTRAR, sem nunca perguntar se o jogador já tinha entrado —
        // então ela ficava piscando por cima de quem já estava lá.
        if self.magica.na_ilha() {
            self.dica_da_trava = None;
            return;
        }
        foco::pede(alvo_da_trava(self.magica.aberto()));
    }

    /// O destaque de tutorial bloqueia os demais toques. Este botão fica
    /// acima do escurecimento e sempre permite sair da orientação.
    fn desenhar_fechar_tutorial(&mut self) {
        if self.nivel_ui.captura_entrada()
            || (self.foco_tutorial.is_none() && self.dica_da_trava.is_none())
        {
            return;
        }
        let f = hud_estilo::fator_texto();
        let seguro = hud_layout::tela_segura();
        let r = Rect::new(seguro.x + seguro.w - 142.0 * f, seguro.y + 8.0 * f, 132.0 * f, 34.0 * f);
        let sobre = r.contains(Vec2::from(mouse_position()));
        hud_estilo::botao(
            r,
            "Close the tutorial",
            hud_estilo::estado(sobre, sobre && is_mouse_button_down(MouseButton::Left), false, false),
            false,
        );
        // Usa o clique bruto porque o botão fica fora do buraco do foco.
        if sobre && is_mouse_button_pressed(MouseButton::Left) {
            self.foco_tutorial = None;
            self.dica_da_trava = None;
            foco::limpar();
        }
    }

    /// na tela, e e' assim que o buraco anda sozinho MENU -> Ficha -> "+".
    fn foco_do_tutorial(&mut self) {
        use shared::quests::{objective_kind, quest_status, tutorial as t};
        let Some((quest, armado)) = self.foco_tutorial else {
            return;
        };
        let ativo = self.missoes.log.iter().any(|q| {
            q.id == quest
                && q.obj_kind == objective_kind::TUTORIAL
                && q.status == quest_status::ACTIVE
        });
        if !ativo || get_time() - armado > Self::FOCO_TUTORIAL_S {
            self.foco_tutorial = None;
            return;
        }
        let Some(acao) = self
            .missoes
            .log
            .iter()
            .find(|q| q.id == quest)
            .map(|q| q.obj_target)
        else {
            return;
        };
        use foco::chave as c;
        let caminho: &[u16] = match acao {
            t::POCAO_LIMIAR => &[c::POCAO_LIMIAR, c::MENU],
            t::SKILL_AUTO => &[c::SKILL_AUTO],
            t::AUTO_COMBATE => &[c::AUTO_COMBATE],
            t::AUTO_COLETA => &[c::AUTO_COLETA],
            t::MAPA_IR => &[c::MAPA_IR, c::MINIMAPA],
            t::PONTO_ATRIBUTO => &[c::FICHA_MAIS, c::MENU_FICHA, c::MENU],
            t::EVOLUIR_SKILL => &[c::SKILL_EVOLUIR, c::MENU_SKILLS, c::MENU],
            // A ILHA: os quatro que acontecem DENTRO do painel do mural. O
            // caminho tem um degrau só porque o painel já está aberto quando
            // o passo abre — quem fechou o anterior fechou nele.
            // O passo que ENSINA A ACHAR o painel. Os quatro de dentro
            // tinham destaque e este não: o jogador via o buraco aceso a
            // partir do segundo passo, quando já tinha descoberto sozinho.
            t::COLONIA_PAINEL => &[c::MENU_MINHA_ILHA, c::MENU],
            t::COLONIA_ASSENTAMENTO => &[c::ILHA_ASSENTAMENTO],
            t::COLONIA_CONTRATAR => &[c::ILHA_CONTRATAR],
            t::COLONIA_COLHER => &[c::ILHA_COLHER],
            t::COLONIA_RETIRAR => &[c::ILHA_RETIRAR],
            // COLETA_ENERGIA e COLONIA_MURAL se fazem no MUNDO: escurecer a
            // tela esconderia exatamente a pedra (ou o mural) que ele tem que
            // achar.
            _ => return,
        };
        foco::pede(caminho);
    }

    /// O jogador fez a acao de um passo TUTORIAL: avisa o servidor — so' se
    /// o passo esta' ativo, pra nao mandar a cada toque no COMBATE.
    fn tutorial(&mut self, acao: u16) {
        use shared::quests::{objective_kind, quest_status};
        let ativo = self.missoes.log.iter().any(|q| {
            q.obj_kind == objective_kind::TUTORIAL
                && q.obj_target == acao
                && q.status == quest_status::ACTIVE
        });
        if ativo {
            self.envia(ClientMessage::Tutorial { acao });
        }
    }

    /// Clicou numa missao do rastreador (ou "Ir" no diario): auto missao.
    fn iniciar_auto_missao(&mut self, id: u16) {
        self.parar_seguir();
        let Some(def) = shared::quests::quest_by_id(id) else { return };
        if let Some(alvo) = self
            .missoes
            .log
            .iter()
            .find(|q| q.id == id && q.obj_kind == shared::quests::objective_kind::NIVEL)
            .map(|q| q.obj_count)
            .filter(|alvo| self.bolsa.nivel < *alvo)
        {
            self.fecha_paineis();
            self.foco_tutorial = None;
            self.dica_da_trava = None;
            foco::limpar();
            self.nivel_ui.abrir(alvo);
            return;
        }
        let pode = self.missoes.log.iter().find(|q| q.id == id)
            .is_some_and(|q| menu_missoes::pode_iniciar_auto(def, q.status));
        if !pode {
            if self.missoes.log.iter().any(|q|q.id==id && q.status==shared::quests::quest_status::ACTIVE) && menu_missoes::tem_atalho_manual(def) {
                self.abrir_missao_manual(id);
                return;
            }
            self.chat.push(format!("{} needs you to act.", def.title));
            return;
        }
        let Some(nome) = self
            .missoes
            .log
            .iter()
            .find(|q| q.id == id)
            .map(|q| q.title.clone())
        else {
            return;
        };
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
        if !self.auto_resumo.ativo { self.auto_resumo.iniciar(); }
    }

    // ─────────────────────────── HUD e Menu (MIR4) ───────────────────────────

    /// Algum painel GRANDE aberto (Menu, Bolsa, Mapa, Craft, Forja, Todas as
    /// missoes, Lojas)? Com ele o HUD some e todo clique e roda sao do painel.
    /// Digitar em painéis e diálogos não pode comandar o personagem.
    fn teclado_bloqueado(&self) -> bool {
        self.painel_grande()
            || self.dialogo.aberto
            || self.confirmar.is_some()
            || self.carregando_desde.is_some()
            || self.economia.bloqueia_entrada(get_time())
    }

    fn painel_grande(&self) -> bool {
        self.menu.aberto
            || self.bolsa.aberta
            || self.mapa.aberto
            || self.craft.aberto()
            || self.forja.aberto()
            || self.evolucao_skills.aberto
            || self.ficha_ui.aberta
            || self.pets_ui.aberta
            || self.invocacao.aberta()
            || self.menu_missoes.aberto
            || self.mobs_ui.aberto
            || self.diarias.aberto
            || self.lojas.aberto
            || self.mercado.aberto
            || self.social.aberto
            || self.morte.painel
            || self.config_barra.aberto
            || self.config_coleta.aberto
            || self.config_combate.aberto
            || self.config_interface.aberto
            || self.config_graficos.aberto
            || self.onde_obter.aberto()
            || self.dungeon.aberto
            || self.presenca.aberto
            || self.viagem.aberto()
            || self.colonia.aberto()
            || self.guarda_roupa.aberto
            || self.banco.aberto()
            || self.escolha_npc.aberta()
            || self.loja_tp.aberto
            || self.montarias.aberto
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
        self.economia
            .entrar(get_time(), self.ficha.xp, nivel, self.bolsa.ouro);
    }

    /// O que o personagem esta' fazendo, pro resumo da tela preta.
    fn estado_da_economia(&self) -> (&'static str, Color) {
        let verde = Color::new(0.45, 0.85, 0.52, 1.0);
        if self.net.is_none() {
            ("NO CONNECTION", Color::new(0.92, 0.30, 0.30, 1.0))
        } else if self.morte.morto {
            ("DEAD", Color::new(0.92, 0.30, 0.30, 1.0))
        } else if self.dialogo.aberto {
            ("WAITING FOR YOU", hud_estilo::OURO)
        } else if self.auto_combate.ativo() {
            ("AUTO COMBAT", verde)
        } else if self.auto_coleta.ativo() {
            ("AUTO GATHER", verde)
        } else if self.auto_missao.etapa().is_some() {
            ("AUTO QUEST", verde)
        } else {
            ("IDLE", Color::new(0.55, 0.56, 0.60, 1.0))
        }
    }

    fn desenhar_economia(&mut self) {
        let agora = get_time();
        let eu = self.world.self_id.and_then(|id| self.world.ents.get(&id));
        let (hp, hp_max, nivel_ent, nome) = eu
            .map(|e| {
                (
                    e.state.hp as i32,
                    e.meta.hp_max as i32,
                    e.meta.nivel as u32,
                    e.meta.name.clone().unwrap_or_default(),
                )
            })
            .unwrap_or_default();
        let nivel = if self.ficha.nivel == 0 {
            nivel_ent
        } else {
            self.ficha.nivel
        };
        let hp_max = self.bolsa.stats.as_ref().map_or(hp_max, |s| s.hp_max);
        let mult = if self.ficha.mult_xp > 0 {
            self.ficha.mult_xp
        } else {
            shared::DEFAULT_XP_MULTIPLIER
        };
        let (base, prox) = (
            shared::xp_for_level_with_mult(nivel, mult),
            shared::xp_for_level_with_mult(nivel + 1, mult),
        );
        let exp = if prox > base {
            self.ficha.xp.saturating_sub(base) as f32 / (prox - base) as f32
        } else {
            0.0
        };
        let estado = self.estado_da_economia();
        let bolsa = &self.bolsa;
        let nome_item = |id: u16| bolsa.nome(id);
        let missao_atual = self.auto_missao.quest.and_then(|id| self.missoes.log.iter()
            .find(|q| q.id == id).map(|q| (id, q.title.as_str(), q.progress, q.obj_count)));
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
            auto_resumo: (self.auto_resumo.ativo
                && (self.auto_missao.ativo() || !self.fila_de_missoes.is_empty()))
                .then_some(&self.auto_resumo),
            missao_atual,
            proximas_missoes: &self.fila_de_missoes,
            nomes_itens: &bolsa.nomes,
        };
        if self.economia.desenha(&resumo, agora) {
            // Deslizou: volta e mostra o que rendeu enquanto estava fora.
            self.economia
                .sair_com_resumo(agora, self.ficha.xp, nivel, self.bolsa.ouro);
        }
    }

    /// A interface pega o dedo que encostou em `p`? Como `ui_pega_em`, mas
    /// sem contar o arrasto de skill do OUTRO dedo: segurar uma skill com a
    /// direita nao pode travar o joystick da esquerda.
    fn ui_pega_dedo(&self, p: Vec2) -> bool {
        if self.carregando_desde.is_some()
            || self.recompensas.captura_entrada()
            || self.nivel_ui.captura_entrada()
            || self.foco_tutorial.is_some()
            || self.dica_da_trava.is_some()
            || self.confirmar.is_some()
            || self.economia.bloqueia_entrada(get_time())
            || self.economia.resumo.is_some()
            || self.painel_grande()
            || self.morte.pega_mouse()
        {
            return true;
        }
        self.social_hud.captura(&hud_layout::atual(), p, self.alvo_jogador().is_some(), self.social.grupo.len())
            || hud_layout::atual().contem(p)
            || self.botao_teleporte.is_some_and(|r| r.contains(p))
            || self.habilidades.botao_em(p)
            || self.mapa.pega_mouse()
            || self.loja.pega_mouse()
            || self.missoes.pega_mouse()
            || self.dialogo.pega_mouse()
            || self.dungeon.pega_mouse()
            || self.porao.pega_mouse(p)
    }

    fn ui_pega_mouse(&self) -> bool {
        self.ui_pega_em(Vec2::from(mouse_position()))
    }

    /// O HUD pega o toque NESTE ponto?
    ///
    /// Existe separado de `ui_pega_mouse` por causa do dedo: no iPhone o mouse
    /// simulado fica onde foi o toque ANTERIOR, entao um toque no mundo logo
    /// depois de encostar num botao nascia marcado como "no HUD" — e, como o
    /// `gesto_camera` le' isso uma vez so', no encosto, aquele arrasto nunca
    /// virava giro. Ficava so' a pinca, que nem consulta o HUD.
    /// Entrou no mundo ou foi teleportado: a tela de carregando cobre ate' o
    /// chao em volta existir. Sem ela o boneco aparecia no ar, caindo num
    /// buraco de ceu enquanto o terreno nascia aos poucos.
    fn comecar_carregando(&mut self) {
        if self.carregando_desde.is_none() {
            println!("[carregando] comecou em {:?}", self.world.self_pos());
        }
        self.carregando_desde = Some(get_time());
        self.cam_altura = f32::MIN;
    }

    /// Quanto do chao em volta ja' existe (0..1), pra barra.
    fn progresso_do_carregando(&self) -> f32 {
        let Some(eu) = self.world.self_pos() else {
            return 0.0;
        };
        let (feitos, total) = self
            .terreno
            .as_ref()
            .map_or((1, 1), |t| t.prontos_em(eu, RAIO_DO_CARREGANDO));
        let vila = if self.construcoes.prontas() { 1.0 } else { 0.0 };
        (feitos as f32 / total.max(1) as f32) * 0.85 + vila * 0.15
    }

    fn acompanhar_carregando(&mut self) {
        let Some(desde) = self.carregando_desde else {
            return;
        };
        let passou = get_time() - desde;
        let pronto = self.world.self_pos().is_some() && self.progresso_do_carregando() >= 1.0;
        // Minimo curto pra nao piscar; teto pra nunca prender o jogador.
        if (pronto && passou >= 0.35) || passou >= 10.0 {
            println!(
                "[loading] finished in {passou:.2} s ({})",
                if pronto {
                    "chao pronto"
                } else {
                    "10 s cap"
                }
            );
            self.carregando_desde = None;
            self.cam_altura = f32::MIN;
        }
    }

    fn desenhar_carregando(&self) {
        ui::fundo();
        let (w, h) = (screen_width(), screen_height());
        let c = vec2(w * 0.5, h * 0.5);
        ui::texto_centro(c.x, c.y - 18.0, "Loading…", 26, ui::OURO);
        let bw = (w * 0.4).clamp(220.0, 420.0);
        let barra = Rect::new(c.x - bw * 0.5, c.y + 6.0, bw, 8.0);
        ui::barra(barra, self.progresso_do_carregando());
    }

    fn ui_pega_em(&self, m: Vec2) -> bool {
        if self.carregando_desde.is_some() {
            return true;
        }
        if self.recompensas.captura_entrada() {
            return true;
        }
        if self.nivel_ui.captura_entrada()
            || self.foco_tutorial.is_some()
            || self.dica_da_trava.is_some()
        {
            return true;
        }
        if self.confirmar.is_some() {
            return true;
        }
        if self.economia.bloqueia_entrada(get_time()) || self.economia.resumo.is_some() {
            return true;
        }
        if self.painel_grande() || self.morte.pega_mouse() {
            return true;
        }
        let z = hud_layout::atual();
        self.social_hud.captura(&z, m, self.alvo_jogador().is_some(), self.social.grupo.len())
            || z.contem(m)
            || self.botao_teleporte.is_some_and(|r| r.contains(m))
            || self.mapa.pega_mouse()
            || self.loja.pega_mouse()
            || self.missoes.pega_mouse()
            || self.habilidades.pega_mouse()
            || self.dialogo.pega_mouse()
            || self.dungeon.pega_mouse()
            || self.porao.pega_mouse(m)
    }

    /// The player's packed look as the server last sent it: what the world
    /// draws for this character (0, the default look, before the first meta).
    fn minha_aparencia(&self) -> u32 {
        self.world
            .self_id
            .and_then(|id| self.world.ents.get(&id))
            .map_or(0, |e| e.meta.aparencia)
    }

    /// The skill set of the equipped weapon — the same rule the skill bar uses.
    fn conjunto_equipado(&self) -> shared::skills::Conjunto {
        shared::skills::Conjunto::da_arma(self.bolsa.equip.weapon.unwrap_or(0))
    }

    /// O proprio personagem esta' montado (flag do servidor)?
    /// Tem montaria EQUIPADA? E' o slot da bolsa que decide (docs/MONTARIAS.md).
    fn tem_montaria(&self) -> bool {
        self.bolsa
            .equip
            .montaria
            .is_some_and(|id| shared::montarias::de_item(id).is_some())
    }

    fn eu_montado(&self) -> bool {
        self.world
            .self_id
            .and_then(|id| self.world.ents.get(&id))
            .is_some_and(|e| e.state.flags & shared::ent_flags::MONTADO != 0)
    }

    /// Botao do HUD e "Ride" da janela: monta, desmonta ou cancela.
    fn alternar_montaria(&mut self) {
        use shared::loja::PedidoLoja;
        if self.eu_montado() || self.montarias.montando(get_time()) {
            self.envia(ClientMessage::Loja {
                pedido: PedidoLoja::Desmontar,
            });
            return;
        }
        if !self.tem_montaria() {
            self.chat.push(
                "Montaria: equipe uma no slot Montaria da bolsa. Onde obter: Menu › Comércio › Loja."
                    .into(),
            );
            return;
        }
        self.envia(ClientMessage::Loja {
            pedido: PedidoLoja::Montar,
        });
    }

    /// Viagem automatica (mapa, Ir para, auto missao): sobe na montaria se
    /// tiver e estiver a pe'. O intervalo evita
    /// pedir de novo a cada trecho da rota.
    fn montar_pra_viajar(&mut self) {
        let agora = get_time();
        if self.world.self_pos().is_some_and(|p| self.world.ents.values().any(|e|
            e.state.flags & shared::ent_flags::BOSS != 0 && e.state.hp > 0
                && p.distance_squared(e.render_pos) <= 144.0)) { return; }
        if !self.tem_montaria()
            || self.eu_montado()
            || self.montarias.montando(agora)
            || agora - self.montar_auto_em < 6.0
        {
            return;
        }
        self.montar_auto_em = agora;
        self.envia(ClientMessage::Loja {
            pedido: shared::loja::PedidoLoja::Montar,
        });
    }

    /// Fecha os paineis grandes: so' um por vez (docs/HUD.md 4.2).
    fn fecha_paineis(&mut self) {
        self.bolsa.fecha();
        self.mapa.aberto = false;
        self.craft.fechar();
        self.forja.fechar();
        self.evolucao_skills.fechar();
        self.ficha_ui.fechar();
        self.pets_ui.fechar();
        self.menu_missoes.aberto = false;
        self.mobs_ui.fechar();
        self.diarias.fechar();
        self.lojas.fechar();
        self.mercado.fechar();
        self.social.fechar();
        self.dungeon.fechar();
        self.presenca.fechar();
        self.viagem.fechar();
        self.colonia.fechar();
        self.magica.fechar();
        self.guarda_roupa.fechar();
        self.banco.fechar();
        self.escolha_npc.fechar();
        self.loja_tp.fechar();
        self.montarias.fechar();
        self.morte.painel = false;
        self.config_barra.fechar();
        self.config_coleta.fechar();
        self.config_combate.fechar();
        self.config_interface.fechar();
        self.config_graficos.fechar();
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
                self.chat.push(format!("Heading to: {}", alvo.rotulo));
                self.iniciar_ir_para(alvo);
            }
            onde_obter::Ir::AbrirCraft(receita) => self.craft.abrir_receita(receita),
            onde_obter::Ir::AbrirCraftMaterial(entrada) => self.craft.abrir_material(entrada),
            onde_obter::Ir::AbrirCalendario => {
                for pedido in self.presenca.abrir() {
                    self.envia(pedido);
                }
            }
            onde_obter::Ir::AbrirDungeons => {
                for pedido in self.dungeon.abrir() {
                    self.envia(pedido);
                }
            }
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
            Item::Ficha => self.ficha_ui.abrir(),
            Item::Pets => self.pets_ui.abrir(),
            Item::Missoes => {
                self.missoes.fecha();
                self.missoes.alterna_diario();
            }
            Item::TodasMissoes => self.menu_missoes.abrir(),
            Item::Diarias => self.diarias.abrir(),
            Item::Craft => self.craft.abrir(),
            Item::Combinar => self.craft.abrir_combinar(),
            Item::IlhaMagica => {
                self.magica.pedir_abertura();
                self.envia(ClientMessage::Magica {
                    pedido: shared::magica::PedidoMagica::Painel,
                });
            }
            Item::Forja => self.forja.abrir(),
            Item::Habilidades => self.evolucao_skills.abrir(self.conjunto_equipado()),
            Item::Mapa if self.mapa.tem_ilha() => self.mapa.abrir(),
            Item::Mobs => self.mobs_ui.abrir(),
            Item::Mapa => {
                self.voltar_ao_menu = false;
                self.chat.push("Map: only on the islands.".into());
            }
            Item::Lojas => self.lojas.abrir(),
            // O banco so' abre no Banqueiro: o Menu leva ate' ele.
            Item::Banco => {
                self.voltar_ao_menu = false;
                let eu = self.world.self_pos().unwrap_or(Vec2::ZERO);
                match self
                    .mapa
                    .npcs_da_vila()
                    .into_iter()
                    .filter(|(n, _)| n == "Banker" || n == shared::construcao::Papel::Deposito.nome())
                    .min_by(|a, b| a.1.distance_squared(eu).total_cmp(&b.1.distance_squared(eu)))
                {
                    Some((nome, pos)) => {
                        self.chat.push(format!("Heading to the bank: {nome}"));
                        self.iniciar_ir_para(ir_para::Alvo {
                            objetivo: ir_para::Objetivo::Npc,
                            pos,
                            raio: 0.0,
                            rotulo: format!("Bank · {nome}"),
                        });
                    }
                    None => self
                        .chat
                        .push("Banco: procure a Banqueira da cidade ou o Banqueiro do porto.".into()),
                }
            }
            Item::GuardaRoupa => {
                self.fecha_paineis();
                let g = self.guarda_roupa_salvo.clone();
                self.guarda_roupa.abrir(&g);
            }
            Item::MinhaIlha => {
                // A PORTA DE SAIR da ilha. O mural continua sendo o jeito de
                // administrar (e o unico, de fora daqui o servidor recusa),
                // mas ele exige ANDAR ate' ele — e a saida mora neste painel.
                // Quem chegou e nao conseguiu andar ficava preso na ilha, sem
                // porto e sem porta. O dono ficou.
                self.envia(ClientMessage::Colonia {
                    pedido: shared::colonia::PedidoColonia::Painel,
                });
            }
            Item::Presenca => {
                for pedido in self.presenca.abrir() {
                    self.envia(pedido);
                }
            }
            Item::LojaTp => {
                for pedido in self.loja_tp.abrir() {
                    self.envia(pedido);
                }
            }
            Item::Montaria => {
                for pedido in self.montarias.abrir() {
                    self.envia(pedido);
                }
            }
            Item::Aventuras => {
                for pedido in self.dungeon.abrir() {
                    self.envia(pedido);
                }
            }
            Item::Grupo | Item::Amigos | Item::Correio | Item::Clan => {
                let aba = match item {
                    Item::Grupo => social_ui::Aba::Grupo,
                    Item::Amigos => social_ui::Aba::Amigos,
                    Item::Correio => social_ui::Aba::Correio,
                    _ => social_ui::Aba::Cla,
                };
                for pedido in self.social.abrir(aba) {
                    self.envia(pedido);
                }
            }
            Item::Mercado => {
                for pedido in self.mercado.abrir() {
                    self.envia(pedido);
                }
            }
            Item::RecuperarXp => self.morte.painel = true,
            Item::BarraItens => self.config_barra.abrir(None),
            Item::Coleta => self.config_coleta.abrir(),
            Item::Configuracoes => self.config_interface.abrir(),
            Item::Graficos => self.config_graficos.abrir(),
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
        if self.social_hud.aberto.take().is_some() { return true; }
        if self.seguir.alvo.is_some() { self.parar_seguir(); return true; }
        if self.dialogo.aberto {
            self.dialogo.fechar();
            self.auto_missao.parar();
            return true;
        }
        let do_menu = std::mem::take(&mut self.voltar_ao_menu);
        let fechou_painel = if self.config_interface.aberto {
            self.config_interface.fechar();
            true
        } else if self.config_graficos.aberto {
            self.config_graficos.fechar();
            true
        } else if self.config_combate.aberto {
            self.config_combate.fechar();
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
        } else if self.evolucao_skills.aberto {
            self.evolucao_skills.fechar();
            true
        } else if self.ficha_ui.aberta {
            self.ficha_ui.fechar();
            true
        } else if self.pets_ui.aberta {
            self.pets_ui.fechar();
            true
        } else if self.invocacao.aberta() {
            self.invocacao.fechar();
            true
        } else if self.craft.aberto() {
            self.craft.fechar();
            true
        } else if self.morte.painel {
            self.morte.painel = false;
            true
        } else if self.social.aberto {
            self.social.fechar();
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
        } else if self.mobs_ui.aberto {
            self.mobs_ui.fechar();
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
        self.missoes
            .marcador(&|id| missoes::na_bolsa(slots, id))
            .is_some()
    }

    /// Ponto de atributo sobrando pra distribuir: ponto vermelho na Ficha e no
    /// MENU. Some sozinho quando o servidor confirma que gastou tudo.
    fn selo_ficha(&self) -> bool {
        self.ficha_ui.tem_ponto_sobrando()
    }

    /// Da' pra evoluir alguma habilidade AGORA (nivel, Energia, cobre e tomo
    /// na mao): ponto vermelho em Habilidades. Sem ele so' se descobria
    /// abrindo o painel e conferindo as doze uma a uma.
    fn selo_skills(&self) -> bool {
        let cobre: u32 = self
            .bolsa
            .slots
            .iter()
            .filter(|s| s.item_id == shared::item_id::COPPER && s.instance.is_none())
            .map(|s| s.qty)
            .sum();
        self.evolucao_skills.pode_evoluir_alguma(
            &self.habilidades.catalogo,
            self.ficha.nivel.max(1),
            cobre,
        )
    }

    /// Pet equipado com FOME: ponto vermelho em Pets. Com fome ele nao ganha
    /// experiencia nenhuma (docs/PETS.md) — e' o aviso que mais importa.
    fn selo_pets(&self) -> bool {
        let equip = &self.bolsa.equip;
        let Some(_) = equip.pet.filter(|id| shared::pets::de_item(*id).is_some()) else {
            return false;
        };
        let agora_unix = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_secs() as i64);
        !shared::pets::alimentado(&shared::pets::dados(equip.pet_inst.as_ref()), agora_unix)
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
            nomes: &self.bolsa.nomes,
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
        if self
            .sem_visada
            .is_some_and(|(id, t)| Some(id) == self.alvo && get_time() - t < 1.6)
        {
            let t = if self.auto_combate.ativo() {
                "No line of sight · switching target"
            } else {
                "No line of sight · closing in"
            };
            return Some((t.to_string(), hud_estilo::OURO));
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
            .map(|m| {
                (
                    format!("Walking · {}", rastro::formata_distancia(m)),
                    hud_estilo::AUTO,
                )
            })
    }

    /// Teclas de ACAO no molde do MIR4 de PC (docs/HUD.md 2.5). Nenhuma abre
    /// painel. Z, X, 1/2/3, WASD e Espaco continuam onde ja' eram tratados.
    fn teclas_de_acao(&mut self) {
        if hud_layout::alt() && is_key_pressed(KeyCode::Enter) {
            self.tela_cheia = !self.tela_cheia;
            macroquad::window::set_fullscreen(self.tela_cheia);
        }
        if self.teclado_bloqueado() {
            return;
        }
        if is_key_pressed(KeyCode::LeftShift) || is_key_pressed(KeyCode::RightShift) {
            self.corrida.alternar(self.correndo_auto);
        }
        if desktop::ataque_pressionado() {
            self.atacar();
        }
        if is_key_pressed(KeyCode::Tab) {
            self.proximo_alvo();
        }
        if is_key_pressed(KeyCode::C) {
            self.usar_rapido(0);
        }
        for (i, k) in [KeyCode::Key8, KeyCode::Key9, KeyCode::Key0]
            .into_iter()
            .enumerate()
        {
            if is_key_pressed(k) {
                self.usar_rapido(i + 1);
            }
        }
    }

    /// Inimigos vivos perto, do mais perto pro mais longe.
    fn inimigos_perto(&self) -> Vec<shared::EntityId> {
        let Some(eu) = self.world.self_pos() else {
            return Vec::new();
        };
        let mut v: Vec<(f32, shared::EntityId)> = self
            .world
            .ents
            .iter()
            .filter(|(_, e)| {
                e.meta.tag == shared::EntityTag::Enemy && e.state.hp > 0 && e.morte.is_none()
            })
            .map(|(id, e)| (e.render_pos.distance(eu), *id))
            .filter(|(d, _)| *d <= RAIO_DE_MIRA)
            .collect();
        v.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1 .0.cmp(&b.1 .0)));
        v.into_iter().map(|(_, id)| id).collect()
    }

    /// Mira neste inimigo: e' comando novo, entao viagem e auto missao param.
    fn mirar(&mut self, id: shared::EntityId) {
        self.parar_seguir();
        self.social_hud.pacifico = None;
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
        let valido = self
            .alvo
            .and_then(|id| self.world.ents.get(&id))
            .is_some_and(|e| {
                e.meta.tag == shared::EntityTag::Enemy && e.state.hp > 0 && e.morte.is_none()
            });
        if valido {
            self.ultima_aproximacao = 0.0;
            return;
        }
        match self.inimigos_perto().first() {
            Some(&id) => self.mirar(id),
            None => self.chat.push("No enemy nearby.".into()),
        }
    }

    /// Tab: o proximo inimigo perto (MIR4).
    fn proximo_alvo(&mut self) {
        let lista = self.inimigos_perto();
        if lista.is_empty() {
            return;
        }
        let i = self
            .alvo
            .and_then(|a| lista.iter().position(|x| *x == a))
            .map_or(0, |p| (p + 1) % lista.len());
        self.mirar(lista[i]);
    }

    /// Quanto a bolsa tem do consumivel de cada espaco da barra.
    fn qtd_rapidos(&self) -> [u32; barra::ESPACOS] {
        self.barra
            .espacos
            .map(|e| barra::quantidade(&self.bolsa.slots, e.item_id))
    }

    /// C (0) ou 8/9/0 (1..3): usa o consumivel do espaco. Vazio abre o
    /// configurador ja' com aquele espaco escolhido.
    fn usar_rapido(&mut self, i: usize) {
        self.usar_espaco(i, false);
    }

    /// Segundos que ainda restam do efeito DESTE item, se ele for pocao de
    /// buff e o efeito estiver no ar. `None` = nao e' buff, ou ja' acabou.
    fn buff_ativo_de(&self, item_id: u16) -> Option<i64> {
        let agora = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_secs() as i64);
        let ate = match barra::categoria(item_id)? {
            barra::Categoria::Experiencia => self.bonus_xp_ate,
            barra::Categoria::Fortuna => self.bonus_fortuna_ate,
            barra::Categoria::Sorte => self.bonus_sorte_ate,
            _ => return None,
        };
        (ate > agora).then_some(ate - agora)
    }

    /// Uso PELA MAO (slot da barra): buff ja' ativo pergunta antes, porque o
    /// servidor renova a hora cheia e o tempo que resta se perde.
    fn usar_espaco(&mut self, i: usize, forte: bool) {
        if let Some(esp) = self.barra.espacos.get(i).copied() {
            if esp.item_id != 0 && self.buff_ativo_de(esp.item_id).is_some() {
                self.confirmar = Some(confirmar::Pendente::Barra {
                    i,
                    forte,
                    item: esp.item_id,
                });
                return;
            }
        }
        self.usar_espaco_sem_perguntar(i, forte);
    }

    /// O uso de verdade. O AUTO da barra entra por aqui: ele ja' pula buff
    /// ativo sozinho, e janela nenhuma pode aparecer sem o jogador pedir.
    fn usar_espaco_sem_perguntar(&mut self, i: usize, forte: bool) {
        let Some(esp) = self.barra.espacos.get(i).copied() else {
            return;
        };
        if esp.item_id == 0 {
            self.fecha_paineis();
            self.config_barra.abrir(Some(i));
            return;
        }
        match barra::slot_para_usar(&self.bolsa.slots, esp.item_id, forte) {
            Some(slot) => self.envia(ClientMessage::UseItem { slot: slot as u16 }),
            None => self
                .chat
                .push(format!("No {} in your bag.", self.bolsa.nome(esp.item_id))),
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
        // AUTO COMBATE: o que o jogador escolheu volta no login. Sem isto a
        // tela salvaria e nada mudaria na sessão seguinte.
        if let Some(o) = p.auto_alvo_ordem.clone() {
            if !o.is_empty() {
                self.auto_combate.ordem = o;
            }
        }
        if let Some(v) = p.auto_pvp {
            self.auto_combate.pvp = v;
        }
        if let Some(t) = p.coleta_tipos {
            self.auto_coleta.tipos = t;
        }
        if let Some(e) = p.coleta_energia {
            self.auto_coleta.energia = e;
        }
        if let Some(r) = p.coleta_raio {
            self.auto_coleta.raio = r;
        }
        if let Some(d) = p.coleta_defender {
            self.auto_coleta.defender = d;
        }
        if let Some(e) = p.escala_ui {
            hud_layout::define_escala_ui(e);
        }
        if let Some(m) = p.economia_auto_min {
            self.economia.auto_min = Some(m);
        }
        if p.montaria_skin.is_some() {
            self.montaria_skin = p.montaria_skin;
        }
        if let Some(v) = p.minimapa_expandido {
            hud_layout::define_minimapa_expandido(v);
        }
        if let Some(v) = p.minimapa_oculto {
            hud_layout::define_minimapa_oculto(v);
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
            coleta_energia: Some(self.auto_coleta.energia),
            coleta_raio: Some(cent(self.auto_coleta.raio)),
            coleta_defender: Some(self.auto_coleta.defender),
            escala_ui: Some(cent(hud_layout::escala_ui())),
            economia_auto_min: self.economia.auto_min,
            montaria_skin: self.montaria_skin,
            minimapa_expandido: Some(hud_layout::minimapa_expandido()),
            minimapa_oculto: Some(hud_layout::minimapa_oculto()),
            auto_alvo_ordem: Some(self.auto_combate.ordem.clone()),
            auto_pvp: Some(self.auto_combate.pvp),
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
        let Some(eu) = self.world.self_id.and_then(|id| self.world.ents.get(&id)) else {
            return;
        };
        let st = self.bolsa.stats.as_ref();
        let mp_max = st.map_or(50, |s| s.mp_max).max(1) as f32;
        let vigor_max = st.map_or(100, |s| s.stamina_max).max(1) as f32;
        let agora_unix = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_secs() as i64);
        let estado = barra::Estado {
            vivo: eu.state.hp > 0
                && eu.morte.is_none()
                && eu.state.flags & shared::ent_flags::DOWNED == 0
                && !self.morte.morto,
            hp: eu.state.hp as f32 / eu.meta.hp_max.max(1) as f32,
            mp: self.ficha.mp.map_or(1.0, |m| m as f32 / mp_max),
            vigor: self.ficha.vigor.map_or(1.0, |v| v as f32 / vigor_max),
            xp_ativo: self.bonus_xp_ate > agora_unix,
            fortuna_ativo: self.bonus_fortuna_ate > agora_unix,
            sorte_ativo: self.bonus_sorte_ate > agora_unix,
        };
        let qtd = self.qtd_rapidos();
        if let Some((i, forte)) = self
            .barra
            .decide(&estado, &qtd, &self.bolsa.slots, get_time())
        {
            self.usar_espaco_sem_perguntar(i, forte);
        }
    }

    /// A cada quadro: conduz a auto missao. Teclado ou queda pausam.
    fn conduzir_auto_missao(&mut self) {
        if !self.auto_missao.ativo() {
            return;
        }
        let morto = self
            .world
            .self_id
            .and_then(|id| self.world.ents.get(&id))
            .is_some_and(|e| e.state.hp == 0 || e.morte.is_some());
        if self.andando_na_mao() || morto {
            self.auto_missao.parar();
            self.chat.push(
                if morto {
                    "Auto quest paused: you went down."
                } else {
                    "Auto quest paused."
                }
                .into(),
            );
            return;
        }
        let Some(eu) = self.world.self_pos() else {
            return;
        };
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
                combatendo_alvo: self.alvo.and_then(|id| self.world.ents.get(&id))
                    .is_some_and(|e| e.meta.tag == shared::EntityTag::Enemy && e.state.hp > 0
                        && e.morte.is_none() && q.is_some_and(|q|
                            if matches!(q.obj_kind, shared::quests::objective_kind::COLLECT | shared::quests::objective_kind::DELIVER) {
                                true
                            } else {
                                shared::quests::kill_conta(q.obj_target, e.meta.kind)
                            })),
                coleta_ativa: self.auto_coleta.ativo(),
                // O PROGRESSO É O SINAL DE VIDA do passo: é por ele que o
                // auto missão sabe a diferença entre colher e girar no vazio.
                progresso: q.map_or(0, |q| q.progress),
            }
        };
        for acao in self.auto_missao.passo(ctx) {
            match acao {
                auto_missao::Acao::PedirDestino(q) => {
                    self.envia(ClientMessage::QuestDestino { quest_id: q })
                }
                auto_missao::Acao::Viajar(p) => {
                    if self.alvo.take().is_some() {
                        self.envia(ClientMessage::SetTarget { target: None });
                    }
                    self.montar_pra_viajar();
                    self.mapa.viagem.iniciar(p, agora);
                }
                auto_missao::Acao::Interagir(npc, pos) => {
                    match self.world.ents.get(&npc).map(|e| e.render_pos) {
                        Some(p) => self.falar_com(npc, p),
                        None => self.envia(ClientMessage::MoverPara { x: pos.x, z: pos.y }),
                    }
                }
                auto_missao::Acao::LigarCombate(p) => {
                    self.envia(ClientMessage::PararRota);
                    self.aproximando_alvo = None;
                    self.mapa.viagem.cancelar();
                    self.auto_coleta.parar();
                    self.auto_combate.ligar(p);
                }
                auto_missao::Acao::LigarColeta(p) => {
                    self.envia(ClientMessage::PararRota);
                    self.aproximando_alvo = None;
                    self.mapa.viagem.cancelar();
                    self.auto_combate.parar();
                    // Missao de coleta: os tipos DELA, nao os da configuracao.
                    let (tipos, energia) = shared::quests::quest_by_id(id)
                        .map_or(([true; 5], true), auto_coleta::tipos_da_missao);
                    self.auto_coleta.ligar_missao(p, tipos, energia, agora);
                }
                auto_missao::Acao::PararAutos => {
                    self.envia(ClientMessage::PararRota);
                    self.aproximando_alvo = None;
                    self.mapa.viagem.cancelar();
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
    /// Apanhou de bicho coletando (e a opcao esta' ligada): larga o no', mata
    /// o bicho e volta a coletar. Devolve `true` enquanto esta' defendendo — a
    /// coleta espera.
    fn defender_a_coleta(&mut self, agora: f64) -> bool {
        const LEMBRA_S: f64 = 3.0;
        let vivo = |w: &world::World, id: shared::EntityId| {
            w.ents.get(&id).is_some_and(|e| {
                e.meta.tag == shared::EntityTag::Enemy && e.state.hp > 0 && e.morte.is_none()
            })
        };
        if !self.auto_coleta.defender {
            self.defesa_da_coleta = None;
            return false;
        }
        if self.defesa_da_coleta.is_none() {
            let Some((id, quando)) = self.agressor else {
                return false;
            };
            if agora - quando > LEMBRA_S || !vivo(&self.world, id) {
                return false;
            }
            self.defesa_da_coleta = Some(id);
            self.mapa.viagem.cancelar();
            self.envia(ClientMessage::PararColeta);
            self.chat.push("Auto gather: attacked, striking back.".into());
        }
        let Some(id) = self.defesa_da_coleta else {
            return false;
        };
        if vivo(&self.world, id) {
            if self.alvo != Some(id) {
                self.alvo = Some(id);
                self.envia(ClientMessage::SetTarget { target: Some(id) });
            }
            return true;
        }
        // Morreu (ou sumiu): outro bicho ainda batendo vira o proximo; senao,
        // volta pro no'.
        self.defesa_da_coleta = None;
        self.alvo = None;
        self.envia(ClientMessage::SetTarget { target: None });
        if self
            .agressor
            .is_some_and(|(a, q)| a != id && agora - q <= LEMBRA_S && vivo(&self.world, a))
        {
            return self.defender_a_coleta(agora);
        }
        self.agressor = None;
        self.auto_coleta.retomar();
        false
    }

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
        if na_engrenagem && crate::foco::clique() {
            self.fecha_paineis();
            self.config_coleta.abrir();
        }
        let sobre_botao = (livre && auto_coleta::pega_mouse() && !na_engrenagem).then_some(0);
        let toque = self.toque_coleta.quadro(
            crate::foco::clique(),
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
                self.tutorial(shared::quests::tutorial::AUTO_COLETA);
            }
        }
        if !self.auto_coleta.ativo() {
            self.defesa_da_coleta = None;
            if self.bau_do_auto.ocupado() {
                self.bau_do_auto.parar();
            }
            return;
        }
        let Some(eu) = self.world.self_pos() else {
            return;
        };
        if self.defender_a_coleta(agora) {
            return;
        }
        // A field boss's chest in the gathering radius comes first.
        let centro = self.auto_coleta.centro.unwrap_or(eu);
        let raio = self.auto_coleta.raio.max(20.0);
        let baus: Vec<(shared::EntityId, Vec2)> = self
            .world
            .ents
            .iter()
            .filter(|(_, e)| {
                e.meta.tag == shared::EntityTag::Npc
                    && (shared::forte::PAPEL_VERDE..=shared::forte::PAPEL_VERDE + 2)
                        .contains(&shared::npc_papel_de_kind(e.meta.kind))
                    && (e.render_pos.distance(centro) <= raio || e.render_pos.distance(eu) <= raio)
            })
            .map(|(id, e)| (*id, e.render_pos))
            .collect();
        match self.bau_do_auto.passo(eu, &baus, agora) {
            auto_coleta::AcaoBau::Nada => {}
            auto_coleta::AcaoBau::Esperar => return,
            auto_coleta::AcaoBau::Ir(p) => {
                self.mapa.viagem.iniciar(p, agora);
                return;
            }
            auto_coleta::AcaoBau::Abrir(id) => {
                self.mapa.viagem.cancelar();
                self.envia(ClientMessage::PararRota);
                self.envia(ClientMessage::Interact { target_eid: Some(id.0 as u64) });
                return;
            }
            auto_coleta::AcaoBau::Terminou => {
                self.auto_coleta.retomar();
                return;
            }
        }
        match self.auto_coleta.passo(eu, agora, self.mapa.viagem.ativa()) {
            auto_coleta::Acao::PedirNo {
                tipos,
                energia,
                raio,
                centro,
                evitar,
            } => {
                self.envia(ClientMessage::PedirNoDeColeta {
                    tipos,
                    energia,
                    raio,
                    centro: [centro.x, centro.y],
                    evitar,
                });
            }
            // Veio do mapa ("Ir" numa regiao): so' aquele tipo, em volta dela.
            auto_coleta::Acao::PedirNoDoTipo { tipo, perto } => {
                self.envia(ClientMessage::PedirSpotDeColetaDe {
                    tipo,
                    perto: [perto.x, perto.y],
                });
            }
            auto_coleta::Acao::Ir(p) => self.mapa.viagem.iniciar(p, agora),
            auto_coleta::Acao::Coletar(coluna) => {
                self.mapa.viagem.cancelar();
                self.envia(ClientMessage::PararRota);
                self.envia(ClientMessage::ColetarNo { coluna });
            }
            auto_coleta::Acao::Nada => {}
        }
    }

    /// Clique numa pedra/tronco: anda ate' o alcance e manda coletar ao
    /// chegar. Teclado ou ligar o AUTO desistem.
    fn acompanhar_coleta_manual(&mut self, movimento: bool) {
        let Some((coluna, centro, raio)) = self.coleta_pendente else {
            return;
        };
        if movimento || self.auto_coleta.ativo() {
            self.coleta_pendente = None;
            return;
        }
        let Some(eu) = self.world.self_pos() else {
            return;
        };
        if eu.distance(centro) - raio - shared::ENTITY_RADIUS <= shared::COLETA_ALCANCE_UN {
            self.envia(ClientMessage::ColetarNo { coluna });
            self.coleta_pendente = None;
        }
    }

    /// Clicou no mapa ou no minimapa: viaja ate' la'. Desliga o auto combate e
    /// solta o alvo — senao "ir ate' o alvo" pede rota por cima da viagem.
    /// A ilha (indice do `ARQUIPELAGO`) que a missao do AUTO exige, quando ela
    /// mora em OUTRA ilha. A conta e' de `auto_missao::ilha_da_missao`.
    ///
    /// O servidor ja' resolve a primeira metade disto: pra passo de historia de
    /// outra ilha, `handle_quest_destino` manda o destino pro Capitao do Porto
    /// em vez de mandar pra lugar nenhum. O auto entao caminha ate' o cais e
    /// abre o menu — e era ai' que ele parava, esperando um toque. Este e' o
    /// toque.
    fn ilha_que_o_auto_precisa(&self) -> Option<u8> {
        let quest = self.auto_missao.quest?;
        auto_missao::ilha_da_missao(quest, self.mapa.zona()?)
    }

    /// O menu do Capitao do Porto. Com o AUTO conduzindo missao de outra ilha,
    /// EMBARCA sozinho em vez de abrir o menu.
    ///
    /// Embarcar nao custa nada (`shared::viagem` nao cobra) e se desfaz voltando
    /// pelo mesmo Capitao, entao nao ha' o que confirmar: quem ligou o auto numa
    /// missao de outra ilha pediu a viagem junto.
    ///
    /// Ilha bloqueada ou fora do ar PARA o auto e diz por que — antes deste ramo
    /// ele ficava girando no cais sem nada a fazer, e "parado sem explicacao" e'
    /// indistinguivel de travado.
    fn abrir_viagem(&mut self, destinos: Vec<shared::viagem::Destino>) {
        match auto_missao::rumo(&destinos, self.ilha_que_o_auto_precisa()) {
            auto_missao::Rumo::Embarcar(ilha) => {
                let nome = destinos
                    .iter()
                    .find(|d| d.ilha == ilha)
                    .map(|d| d.nome.clone())
                    .unwrap_or_default();
                self.chat
                    .push(format!("Auto quest: setting sail for {nome}."));
                // Guardado ANTES de `conectar` limpar tudo: e' o que faz o auto
                // voltar a conduzir ao desembarcar.
                self.retomar_auto_missao = self.auto_missao.quest;
                self.envia(ClientMessage::Viajar { ilha });
            }
            auto_missao::Rumo::Parar(aviso) => {
                self.chat.push(aviso);
                self.auto_missao.parar();
                self.viagem.abrir(destinos);
            }
            auto_missao::Rumo::Menu => self.viagem.abrir(destinos),
        }
    }

    /// Desembarcou: o auto que embarcou volta a conduzir sozinho.
    ///
    /// Espera as tres coisas que `conectar` derrubou: estar na zona certa, ter
    /// a missao de novo no log (o log chega depois do mundo) e ja' ter posicao
    /// — `iniciar_auto_missao` precisa de onde o personagem esta'.
    fn retomar_auto_apos_viagem(&mut self) {
        let Some(quest) = self.retomar_auto_missao else {
            return;
        };
        let chegou = shared::quests::zona_da_missao(quest)
            .zip(self.mapa.zona())
            .is_some_and(|(alvo, aqui)| alvo == aqui);
        if !chegou || self.world.self_pos().is_none() {
            return;
        }
        if self.missoes.log.iter().any(|q| q.id == quest) {
            self.retomar_auto_missao = None;
            self.chat.push("Auto quest: landed, resuming.".into());
            self.iniciar_auto_missao(quest);
        }
    }

    fn iniciar_viagem(&mut self, destino: Vec2) {
        self.parar_seguir();
        if !self.mapa.terra(destino) {
            self.chat.push("map: that is water".into());
            return;
        }
        self.auto_combate.parar();
        self.aproximando_alvo = None;
        self.ir_para.parar();
        self.interacao.cancela();
        self.loja.fecha();
        self.missoes.fecha();
        self.auto_missao.parar();
        self.dialogo.fechar();
        if self.alvo.take().is_some() {
            self.envia(ClientMessage::SetTarget { target: None });
        }
        self.montar_pra_viajar();
        self.mapa.viagem.iniciar(destino, get_time());
    }

    /// "Ir" do mapa (zona de bicho, regiao de recurso) ou do menu de missoes
    /// (ir ao Mestre). Encerra o que brigaria pela rota.
    fn iniciar_ir_para(&mut self, alvo: ir_para::Alvo) {
        self.parar_seguir();
        self.mapa.viagem.cancelar();
        self.auto_combate.parar();
        self.aproximando_alvo = None;
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
        self.montar_pra_viajar();
        self.ir_para.iniciar(alvo, get_time());
    }

    /// A cada quadro: conduz o "Ir". Teclado ou queda encerram.
    /// PASSO MANUAL: abre o painel, avisa, e SEGUE com outra missão.
    ///
    /// O dono: "o auto missão mistura missão de craft com dungeon com coleta
    /// com caça, e algumas dessas têm que ser feitas manualmente, aí ele
    /// trava". Travava mesmo — ele parava seco no primeiro passo que exigia o
    /// jogador, e da tela isso é idêntico a estar quebrado.
    ///
    /// Escolha do dono (24/09/2026): pular pra próxima missão automatizável.
    /// O painel do passo manual fica aberto esperando, e o auto continua
    /// rendendo no que ele sabe fazer sozinho.
    ///
    /// As puladas ficam guardadas pra ele não voltar pra mesma no ciclo
    /// seguinte — e a lista zera quando o jogador escolhe uma missão na mão,
    /// que é ele dizendo "agora eu quero esta".
    fn auto_missao_pula(&mut self, quest_id: u16) {
        self.auto_pulados.insert(quest_id);
        let prox = self
            .missoes
            .log
            .iter()
            .find(|q| {
                (q.status == shared::quests::quest_status::ACTIVE
                    || q.status == shared::quests::quest_status::READY)
                    && q.id != quest_id
                    && !self.auto_pulados.contains(&q.id)
                    && shared::quests::quest_by_id(q.id)
                        .is_some_and(|d| menu_missoes::automatizavel(d) && matches!(q.status, shared::quests::quest_status::ACTIVE | shared::quests::quest_status::READY))
            })
            .map(|q| (q.id, q.title.clone()));
        match prox {
            Some((id, nome)) => {
                self.chat
                    .push(format!("Auto quest moved on to \"{nome}\"."));
                self.auto_missao.iniciar(id, nome, get_time());
            }
            None => {
                self.auto_missao.parar();
                self.chat
                    .push("Auto quest: only steps that are yours to do are left.".into());
            }
        }
    }

    fn conduzir_ir_para(&mut self) {
        if !self.ir_para.ativo() {
            return;
        }
        let morto = self
            .world
            .self_id
            .and_then(|id| self.world.ents.get(&id))
            .is_some_and(|e| e.state.hp == 0 || e.morte.is_some());
        if morto || self.andando_na_mao() {
            self.ir_para.parar();
            self.mapa.viagem.cancelar();
            return;
        }
        let Some(eu) = self.world.self_pos() else {
            return;
        };
        let agora = get_time();
        let Some(acao) = self.ir_para.passo(eu, agora, self.mapa.viagem.ativa()) else {
            return;
        };
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
                let npc = self
                    .world
                    .ents
                    .iter()
                    .filter(|(_, e)| {
                        e.meta.tag == shared::EntityTag::Npc && e.render_pos.distance(p) <= 8.0
                    })
                    .min_by(|a, b| {
                        a.1.render_pos
                            .distance_squared(p)
                            .total_cmp(&b.1.render_pos.distance_squared(p))
                    })
                    .map(|(id, e)| (*id, e.render_pos));
                match npc {
                    Some((id, pos)) => self.falar_com(id, pos),
                    None => self.chat.push("I couldn't find the NPC here.".into()),
                }
            }
            // The world map's click: at the captain (or the bus), off we go.
            // The server still checks the route and the distance.
            ir_para::Acao::Embarcar(ilha) => self.envia(ClientMessage::Viajar { ilha }),
            ir_para::Acao::Aviso(s) => self.chat.push(s),
        }
    }

    /// Quanto o auto missão espera entre uma fala e a próxima.
    ///
    /// Um segundo e meio: a conversa é onde a história acontece, e passar
    /// tudo num quadro faria o texto piscar e sumir sem ninguém ler. Longo o
    /// bastante pra ler uma linha, curto o bastante pra não virar espera.
    const AUTO_FALA_S: f64 = 1.5;

    /// Clique no menu de todas as missoes.
    fn abrir_missao_manual(&mut self, id: u16) {
        use shared::quests::{objective_kind as o, tutorial as t};
        let Some(d)=shared::quests::quest_by_id(id) else {return};
        if !menu_missoes::tem_atalho_manual(d) {return}
        self.parar_fila();
        self.auto_missao.parar();
        self.auto_combate.parar();
        self.auto_coleta.parar();
        self.ir_para.parar();
        self.mapa.viagem.cancelar();
        self.fecha_paineis();
        self.menu_missoes.aberto=false;
        self.diarias.fechar();
        self.foco_tutorial=None;
        self.dica_da_trava=None;
        foco::limpar();
        match d.obj_kind {
            o::CRAFT=>self.craft.abrir_no_que_da(&self.bolsa.slots,self.bolsa.nivel),
            o::REFINE=>self.forja.abrir(),
            o::DUNGEON=>{for pedido in self.dungeon.abrir_em(d.obj_target) {self.envia(pedido);}},
            o::TREASURE=>self.mapa.abrir(),
            o::TUTORIAL=>{
                match d.obj_target {
                    t::POCAO_LIMIAR=>self.config_barra.abrir(Some(0)),
                    t::MAPA_IR=>self.mapa.abrir(),
                    t::PONTO_ATRIBUTO | t::EVOLUIR_SKILL=>{
                        // O servidor decide: sem energia leva ao cristal;
                        // com saldo devolve TUTORIAL para abrir o painel.
                        // Só o clique individual chega aqui, nunca a fila.
                        self.auto_missao.iniciar(id, d.title.to_string(), get_time());
                        return;
                    },
                    t::COLONIA_PAINEL | t::COLONIA_ASSENTAMENTO | t::COLONIA_CONTRATAR | t::COLONIA_COLHER | t::COLONIA_RETIRAR=>self.envia(ClientMessage::Colonia {pedido:shared::colonia::PedidoColonia::Painel}),
                    _=>{},
                }
                self.foco_tutorial=Some((id,get_time()));
            },
            _=>{},
        }
        self.chat.push(format!("{}: {}",d.title,d.desc));
    }

    fn clique_menu_missoes(&mut self, c: menu_missoes::Clique) {
        match c {
            menu_missoes::Clique::FazerTutorial(id) => {
                if id == 905 {
                    self.tutorial(shared::quests::tutorial::MISSAO_MENU);
                    self.menu_missoes.aberto = false;
                }
            }
            menu_missoes::Clique::AbrirManual(id) => self.abrir_missao_manual(id),
            menu_missoes::Clique::AutoMissao(id) => {
                self.menu_missoes.aberto = false;
                self.diarias.fechar();
                // Escolher UMA cancela a fila: é a decisão mais recente, e
                // uma fila que sobrevivesse a ela voltaria a mandar sozinha
                // no quadro seguinte.
                self.parar_fila();
                self.auto_resumo.iniciar();
                self.iniciar_auto_missao(id);
            }
            // A FILA: até 10, na ordem marcada. Bloqueada é pulada.
            menu_missoes::Clique::Fila(ids) => {
                self.menu_missoes.aberto = false;
                self.diarias.fechar();
                self.fila_de_missoes = ids.into_iter().filter(|id| shared::quests::quest_by_id(*id).is_some_and(menu_missoes::automatizavel)).collect();
                self.fila_feitas = 0;
                self.fila_puladas = 0;
                self.auto_resumo.iniciar();
                self.chat.push(format!(
                    "Fila: {} missões. O que não der pra fazer agora é pulado.",
                    self.fila_de_missoes.len()
                ));
                self.tocar_fila();
            }
            // TRAVA DE NÍVEL: o "Ir" abre a Ilha Mágica, que é onde se
            // arruma XP, e ACENDE o caminho até ela no menu — o jogador vê
            // onde ela fica, e não só que ela existe. É o mesmo gesto dos
            // outros tutoriais.
            menu_missoes::Clique::OpcoesDeNivel(alvo) => {
                self.menu_missoes.aberto = false;
                self.diarias.fechar();
                self.foco_tutorial = None;
                self.dica_da_trava = None;
                foco::limpar();
                self.nivel_ui.abrir(alvo);
            }
            // Pegar so' aceita. O jogador pode juntar varias missoes e depois
            // escolher Ir ou Fazer tudo.
            // The server confirms ("Quest accepted: …") or says why not:
            // announcing it here claimed quests the server had refused.
            menu_missoes::Clique::Aceitar(id) => {
                self.envia(ClientMessage::AcceptQuest { quest_id: id });
                // The answer (accepted, or why not) comes as a SYS line.
                self.aceite_pendente = Some(get_time());
            }
            menu_missoes::Clique::Aviso(s) => self.chat.push(s),
        }
    }

    /// A cada quadro: pede a proxima etapa. Teclado ou morte encerram.
    fn conduzir_viagem(&mut self) {
        if !self.mapa.viagem.ativa() {
            return;
        }
        let morto = self
            .world
            .self_id
            .and_then(|id| self.world.ents.get(&id))
            .is_some_and(|e| e.state.hp == 0 || e.morte.is_some());
        if morto || self.andando_na_mao() {
            self.mapa.viagem.cancelar();
            return;
        }
        let Some(eu) = self.world.self_pos() else {
            return;
        };
        // A viagem sai do mapa pelo tempo da conta: ela quer `&mut`, e o teste
        // de terra quer o mapa inteiro.
        let mut viagem = std::mem::take(&mut self.mapa.viagem);
        let passo = viagem.passo(eu, get_time(), |p| self.mapa.terra(p));
        self.mapa.viagem = viagem;
        match passo {
            mapa::Passo::Enviar(p) => self.envia(ClientMessage::MoverPara { x: p.x, z: p.y }),
            mapa::Passo::Desistiu => self.chat.push("travel: path blocked".into()),
            // Viagem pelo mapa chegou (e nada automatico segue): desce.
            mapa::Passo::Chegou => {
                if self.eu_montado()
                    && self.auto_missao.etapa().is_none()
                    && !self.ir_para.ativo()
                    && !self.auto_coleta.ativo()
                {
                    self.envia(ClientMessage::Loja {
                        pedido: shared::loja::PedidoLoja::Desmontar,
                    });
                }
            }
            mapa::Passo::Nada => {}
        }
    }

    /// Skills ofensivas usam a selecao enviada por SetTarget.
    fn usar_habilidade(&mut self) {
        // Entrada bloqueada NAO desliga o AUTO: no modo economia e com painel
        // aberto o gesto e a tecla nao valem, mas a rotacao automatica segue
        // — igual ao auto combate, que continua escolhendo alvo. Isto era um
        // `return` seco, e no modo economia o personagem parava de lancar
        // tudo, inclusive a CURA (ver `habilidades::pedido_automatico`).
        let entrada_bloqueada = self.teclado_bloqueado();
        if entrada_bloqueada {
            self.habilidades.cancela_arrasto();
        }
        let conjunto = shared::skills::Conjunto::da_arma(self.bolsa.equip.weapon.unwrap_or(0));
        self.habilidades.wis = self.bolsa.stats.as_ref().map_or(0, |s| s.wis);
        let Some(eu) = self.world.self_id.and_then(|id| self.world.ents.get(&id)) else {
            return;
        };
        let distancia_alvo = self
            .alvo
            .filter(|id| self.seguir.alvo.is_none() && self.social_hud.pacifico != Some(*id))
            .and_then(|id| self.world.ents.get(&id))
            .filter(|e| e.state.hp > 0 && e.morte.is_none())
            .map(|e| eu.render_pos.distance(e.render_pos));
        let contexto = habilidades::Contexto {
            conjunto,
            nivel: self.ficha.nivel,
            mp: self.ficha.mp.unwrap_or(0),
            vivo: eu.state.hp > 0 && eu.state.flags & shared::ent_flags::DOWNED == 0,
            vida_baixa: eu.state.hp as f32 / (eu.meta.hp_max.max(1) as f32) < 0.85,
            distancia_alvo,
        };
        let pedido = if entrada_bloqueada {
            self.habilidades
                .pedido_automatico(contexto, macroquad::prelude::get_time())
        } else {
            self.habilidades.pedido(contexto)
        };
        let Some(id) = pedido else {
            return;
        };
        self.envia(ClientMessage::SkillCast { skill_id: id });
    }

    /// AUTO DUNGEON (`auto_dungeon`): revive, luta, abre o bau e sai.
    fn conduzir_auto_dungeon(&mut self) {
        use auto_dungeon::Passo;
        let agora = get_time();
        if !self.dungeon.auto {
            self.auto_dungeon_bau_em = None;
            return;
        }
        // Fora da instancia so' espera: sair dela (`Aviso::Saiu`) ja' desliga.
        let (Some(mut estado), Some(eu)) = (self.dungeon.estado_auto(agora), self.world.self_pos())
        else {
            return;
        };
        let bau = self
            .world
            .ents
            .iter()
            .find(|(_, e)| {
                e.meta.tag == shared::EntityTag::Npc
                    && shared::npc_papel_de_kind(e.meta.kind) == shared::dungeon::PAPEL_BAU
            })
            .map(|(id, e)| (*id, e.render_pos));
        estado.bau = bau.map(|b| b.1);
        if estado.bau_aberto && self.auto_dungeon_bau_em.is_none() {
            self.auto_dungeon_bau_em = Some(agora);
        }
        let passo = auto_dungeon::decide(estado, eu, self.auto_dungeon_bau_em.map(|t| agora - t));
        // Pedido pro servidor no maximo 1 por segundo; o combate e' por quadro.
        let pode_enviar = agora - self.auto_dungeon_envio >= 1.0;
        match passo {
            Passo::Esperar => {}
            Passo::Reviver if pode_enviar => {
                self.auto_dungeon_envio = agora;
                self.envia(ClientMessage::Dungeon {
                    pedido: shared::dungeon::Pedido::Reviver,
                });
            }
            Passo::Lutar => {
                // O auto combate de sempre, com a area indo junto: a arena e'
                // maior que o raio dele.
                if !self.auto_combate.ativo() {
                    self.mapa.viagem.cancelar();
                    self.auto_coleta.parar();
                    self.auto_missao.parar();
                    self.ir_para.parar();
                    self.auto_combate.ligar(eu);
                }
                self.auto_combate.centro = Some(eu);
                // Nenhum no alcance do auto combate: anda ate' o mais perto.
                if self.alvo.is_none() && pode_enviar {
                    let perto = self
                        .world
                        .ents
                        .values()
                        .filter(|e| {
                            e.meta.tag == shared::EntityTag::Enemy
                                && e.state.hp > 0
                                && e.morte.is_none()
                        })
                        .map(|e| e.render_pos)
                        .min_by(|a, b| a.distance_squared(eu).total_cmp(&b.distance_squared(eu)));
                    // Nothing in sight: the next horde may be past the view
                    // range — walk to the room of the step the run is on.
                    let rumo = perto.or_else(|| {
                        let (conteudo, andar) = self.dungeon.em_curso()?;
                        auto_dungeon::sala_da_vez(conteudo, andar)
                            .filter(|c| c.distance(eu) > 3.0)
                    });
                    if let Some(p) = rumo {
                        self.auto_dungeon_envio = agora;
                        self.envia(ClientMessage::MoverPara { x: p.x, z: p.y });
                    }
                }
            }
            Passo::IrAoBau { perto } if pode_enviar => {
                self.auto_combate.parar();
                self.auto_dungeon_envio = agora;
                if let Some((id, p)) = bau {
                    if perto {
                        self.envia(ClientMessage::Interact {
                            target_eid: Some(id.0 as u64),
                        });
                    } else {
                        self.envia(ClientMessage::MoverPara { x: p.x, z: p.y });
                    }
                }
            }
            Passo::Sair if pode_enviar => {
                self.auto_dungeon_envio = agora;
                self.dungeon.auto = false;
                self.envia(ClientMessage::Dungeon {
                    pedido: shared::dungeon::Pedido::Sair,
                });
            }
            _ => {}
        }
    }

    fn atualizar_auto_combate(&mut self) {
        let movimento = self.andando_na_mao();
        let clique_mundo = self.clique_no_mundo() && !self.ui_pega_mouse();
        // TOQUE LONGO (ou botão direito) ABRE A CONFIGURAÇÃO, como no auto
        // coleta. É por aqui que se chega na ordem de alvo e no PvP — sem
        // isto a lógica existiria e ninguém teria onde mexer nela.
        let mouse_combate = Vec2::from(mouse_position());
        let sobre_combate = (!self.painel_grande() && auto_combate::pega_mouse()).then_some(0);
        let toque_combate = self.toque_combate.quadro(
            crate::foco::clique(),
            is_mouse_button_down(MouseButton::Left),
            is_mouse_button_released(MouseButton::Left),
            sobre_combate,
            mouse_combate,
            get_time(),
        );
        if matches!(toque_combate, toque::Toque::Longo(_))
            || (sobre_combate.is_some() && is_mouse_button_pressed(MouseButton::Right))
        {
            self.fecha_paineis();
            self.config_combate.abrir();
        }
        // O botao so' existe com o HUD a' mostra; a tecla Z vale sempre. Painel
        // aberto NAO desliga o AUTO (MIR4: o menu aberto nao para o combate).
        let alterna = is_key_pressed(KeyCode::Z) || matches!(toque_combate, toque::Toque::Curto(_));
        let esc = is_key_pressed(KeyCode::Escape) && !self.esc_consumido;
        if self.auto_combate.ativo() && (esc || alterna) {
            self.auto_combate.parar();
            // Desligar o combate na mao desliga tambem o AUTO DUNGEON, que o religaria.
            self.dungeon.auto = false;
            // Desligar o combate a mao desliga a auto missao que dependia dele.
            if alterna || esc {
                self.auto_missao.parar();
            }
            self.alvo = None;
            self.envia(ClientMessage::SetTarget { target: None });
            if let Some(p) = self.world.self_pos() {
                self.envia(ClientMessage::MoverPara { x: p.x, z: p.y });
            }
            return;
        }
        if alterna {
            // Ligar o auto combate encerra a viagem (os dois pedem rota), a auto
            // coleta (exclusivas) e a auto missao.
            self.mapa.viagem.cancelar();
            self.auto_coleta.parar();
            self.auto_missao.parar();
            self.ir_para.parar();
            if let Some(p) = self.world.self_pos() {
                self.auto_combate.ligar(p);
                self.tutorial(shared::quests::tutorial::AUTO_COMBATE);
            }
        }
        if !self.auto_combate.ativo() {
            return;
        }
        let agora = get_time();
        // Andar nao desliga: teclado, ou clique no CHAO (clique em bicho so'
        // troca o alvo, e o AUTO segue com ele).
        if movimento || (clique_mundo && self.alvo.is_none()) {
            self.auto_combate.andar_manual(agora);
        }
        let Some(eu) = self.world.self_pos() else {
            return;
        };
        let dirigindo = self.auto_combate.segurando(eu, agora);
        // O BICHO DA MISSÃO, quando há missão de caça ativa. É o que faz o
        // auto priorizar o que o jogador está tentando terminar.
        let missao_kind = self
            .missoes
            .log
            .iter()
            .find(|q| {
                q.status == shared::quests::quest_status::ACTIVE
                    && q.obj_kind == shared::quests::objective_kind::KILL
                    && self.auto_missao.quest.is_none_or(|id| q.id == id)
            })
            // `obj_target` is `alvo_de_mob(kind)` = kind + 1; 0 is "any mob"
            // and ALVO_QUALQUER_CHEFE "any boss" — no species to prefer. It
            // was passed raw since 24/09/2026: a wolf quest made the AUTO
            // chase bears, and an "any mob" quest hunted only wolves.
            .and_then(|q| {
                (q.obj_target != 0 && q.obj_target != shared::quests::ALVO_QUALQUER_CHEFE)
                    .then(|| q.obj_target - 1)
            });
        let novo = self
            .auto_combate
            .escolher(&self.world, self.alvo, agora, missao_kind);
        if novo != self.alvo {
            self.alvo = novo;
            self.envia(ClientMessage::SetTarget { target: novo });
            if novo.is_none() {
                if let Some(p) = self.world.self_pos() {
                    self.envia(ClientMessage::MoverPara { x: p.x, z: p.y });
                }
            }
        }
        // Limpou a area: vai atras do proximo em vez de ficar parado.
        if novo.is_none() && !dirigindo {
            if let Some(p) = self.auto_combate.caca(&self.world, eu, agora, missao_kind) {
                self.envia(ClientMessage::MoverPara { x: p.x, z: p.y });
            }
        }
    }

    fn alvo_jogador(&self) -> Option<(shared::EntityId, String)> {
        let id = self.alvo?;
        if Some(id) == self.world.self_id { return None; }
        let e = self.world.ents.get(&id)?;
        (e.meta.tag == shared::EntityTag::Player && e.morte.is_none())
            .then(|| (id, e.meta.name.clone().unwrap_or_default()))
    }

    fn parar_seguir(&mut self) {
        if let Some(msg) = self.seguir.parar() { self.envia(msg); }
    }

    fn conduzir_seguir(&mut self) {
        let Some(id) = self.seguir.alvo else { return };
        if self.andando_na_mao() || self.alvo != Some(id) || self.auto_combate.ativo() {
            self.parar_seguir();
            return;
        }
        if self.habilidades.ocupada() { return; }
        let eu = self.world.self_pos();
        let vivo = self.world.self_id.and_then(|id| self.world.ents.get(&id))
            .is_some_and(|e| e.state.hp > 0 && e.morte.is_none() && e.state.flags & shared::ent_flags::DOWNED == 0);
        if !vivo || eu.is_none() { self.parar_seguir(); return; }
        let destino = self.world.ents.get(&id).filter(|e| e.state.hp > 0 && e.morte.is_none()).map(|e| e.render_pos);
        if let Some(msg) = self.seguir.passo(eu.unwrap(), destino, get_time()) { self.envia(msg); }
    }

    fn acao_social_alvo(&mut self, acao: social_hud::Acao) {
        use social_hud::Acao;
        match acao {
            Acao::AbrirGrupo => {
                let pedidos = self.social.abrir(social_ui::Aba::Grupo);
                for p in pedidos { self.envia(p); }
            }
            Acao::Selecionar(id) => {
                self.parar_seguir();
                self.auto_combate.parar();
                self.dungeon.auto = false;
                self.alvo = Some(id);
                self.social_hud.aberto = None;
                self.social_hud.pacifico = Some(id);
                self.aproximando_alvo = None;
                self.envia(ClientMessage::PararRota);
                self.envia(ClientMessage::SetTarget { target: None });
            }
            _ => {
                let Some((id, nome)) = self.alvo_jogador() else { return };
                match acao {
                    Acao::Inspecionar => {
                        self.dungeon.auto = false;
                        self.social_hud.pacifico = Some(id);
                        self.auto_combate.parar();
                        self.aproximando_alvo = None;
                        self.envia(ClientMessage::SetTarget { target: None });
                        self.envia(ClientMessage::PararRota);
                    }
                    Acao::Amigo => self.envia(ClientMessage::Social { pedido: shared::social::Pedido::Amizade { nome } }),
                    Acao::Grupo => self.envia(ClientMessage::PartyInvite { target_name: nome }),
                    Acao::Seguir => {
                        if self.seguir.alvo == Some(id) { self.parar_seguir(); return; }
                        self.dungeon.auto = false;
                        self.auto_combate.parar();
                        self.auto_coleta.parar();
                        self.auto_missao.parar();
                        self.mapa.viagem.cancelar();
                        self.ir_para.parar();
                        self.interacao.cancela();
                        self.aproximando_alvo = None;
                        self.envia(ClientMessage::SetTarget { target: None });
                        self.envia(ClientMessage::PararRota);
                        self.social_hud.pacifico = Some(id);
                        self.seguir.iniciar(id);
                        self.chat.push(format!("Seguindo {nome}. Mova-se ou use Esc para parar."));
                    }
                    _ => {}
                }
            }
        }
    }

    /// Com um inimigo selecionado e fora do alcance, anda ate ele.
    fn ir_ate_o_alvo(&mut self) {
        if self.seguir.alvo.is_some() { return; }
        if self.aproximando_alvo.is_some() && self.aproximando_alvo != self.alvo {
            self.envia(ClientMessage::PararRota);
            self.aproximando_alvo = None;
        }
        if self.habilidades.ocupada() {
            return;
        }
        let Some(alvo) = self.alvo else { return };
        if self.andando_na_mao() || self.auto_combate.dirigindo() {
            self.aproximando_alvo = None;
            return;
        }
        let eu = self.world.self_id.and_then(|i| self.world.ents.get(&i));
        let (Some(eu), Some(ele)) = (eu, self.world.ents.get(&alvo)) else {
            return;
        };
        if ele.meta.tag == shared::EntityTag::Player
            && (!eu.meta.pk.hostil || self.social_hud.pacifico == Some(alvo)) { return; }
        if ele.state.hp == 0 || ele.morte.is_some() {
            if self.aproximando_alvo == Some(alvo) {
                self.envia(ClientMessage::PararRota);
                self.aproximando_alvo = None;
            }
            return;
        }
        let conjunto =
            shared::skills::Conjunto::de_u8(shared::components::acao::conjunto(eu.state.acao))
                .unwrap_or(shared::skills::Conjunto::EspadaEscudo);
        let alcance = if conjunto.a_distancia() {
            shared::RANGED_ATTACK_RANGE
        } else {
            shared::MELEE_RANGE
        };
        let (a, b) = (eu.render_pos, ele.render_pos);
        let agora = get_time();
        // O servidor disse que o relevo barra o tiro: chega perto pelo
        // caminho (o servidor faz a rota) ate' a visada voltar.
        let sem_visada = self
            .sem_visada
            .is_some_and(|(id, t)| id == alvo && agora - t < 1.6);
        let distancia = a.distance(b);
        // A rota do ultimo pedido pode continuar andando mesmo depois de
        // entrar no alcance. Para-la impede o melee de atravessar o mob e
        // refazer A* a cada golpe; a mira e o ataque seguem ativos.
        let parar_em = if conjunto.a_distancia() {
            alcance * 0.9
        } else {
            alcance * 0.72
        };
        if distancia <= parar_em && (!sem_visada || distancia <= 2.5) {
            if self.aproximando_alvo == Some(alvo) {
                self.envia(ClientMessage::PararRota);
                self.aproximando_alvo = None;
            }
            return;
        }
        let intervalo = if conjunto.a_distancia() { 0.35 } else { 0.25 };
        if agora - self.ultima_aproximacao < intervalo {
            return;
        }
        self.ultima_aproximacao = agora;
        let perto = if sem_visada { 2.0 } else { alcance * 0.55 };
        let destino = b + (a - b).normalize_or_zero() * perto;
        self.envia(ClientMessage::MoverPara {
            x: destino.x,
            z: destino.y,
        });
        self.aproximando_alvo = Some(alvo);
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
            self.envia(ClientMessage::Ping {
                client_time_ms: agora_ms(),
            });
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
        self.social = social_ui::Social::default();
        self.social_hud = social_hud::SocialHud::default();
        self.seguir = seguir::Seguir::default();
        self.voltar_ao_menu = false;
        self.personagem_atual = None;
        self.token_login = None;
        self.selecao_personagem = personagens::Personagens::default();
        self.net = None;
        self.world = World::default();
        self.ganhos = ganhos::Ganhos::default();
        self.map = None;
        self.terreno = None;
        self.alvo = None;
        self.aproximando_alvo = None;
        self.info = hud::Info::default();
        self.rede = hud::Rede::default();
        self.chat.clear();
        self.bolsa = bolsa::Bolsa::default();
        self.ficha = hud::Ficha::default();
        self.ficha_ui = ficha_ui::FichaUi::default();
        self.pets_ui = pets_ui::PetsUi::default();
        self.invocacao = invocacao_ui::InvocacaoUi::default();
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
            })
            // Sem corpo E sem posicao: o chao da ORIGEM, que e' pra onde
            // `desenhar_mundo` aponta a camera nesse caso.
            //
            // Sem isto `cam_altura` ficava em `f32::MIN` e a camera olhava pro
            // infinito negativo: o mundo inteiro caia fora da tela e sobrava a
            // cor de fundo. Foi assim que a colonia apareceu "toda azul e
            // vazia" com o relevo CARREGADO — 121 pedacos prontos, nenhum na
            // tela. Um defeito de entidade virou um defeito de camera, e o de
            // camera escondeu o de entidade.
            .or_else(|| {
                let t = self.terreno.as_ref()?;
                Some(t.altura_apoio(0.0, 0.0, shared::ENTITY_RADIUS))
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
        self.toque_ativo =
            !brutos.is_empty() || self.gesto_camera.ativo() || get_time() - self.ultimo_toque < 0.3;
        // A fase vem da NOSSA memoria de dedos, nao so' da macroquad: ela guarda
        // um toque por id e o evento mais novo do quadro sobrescreve o outro —
        // o dedo que encosta e ja' ARRASTA no mesmo quadro chega como "Moveu",
        // nunca como "Comecou". O joystick so' pega dedo que comeca, entao
        // perdia esse, e a camera (que aceita qualquer fase) girava: o "as
        // vezes o joystick buga e mexe a camera" (19/09/2026). Dedo que nao
        // existia no quadro anterior E' um dedo que comecou.
        let anteriores = std::mem::take(&mut self.dedos_anteriores);
        let toques: Vec<gesto_camera::ToqueNoQuadro> = brutos
            .iter()
            .map(|t| gesto_camera::ToqueNoQuadro {
                id: t.id,
                fase: match t.phase {
                    TouchPhase::Ended | TouchPhase::Cancelled => gesto_camera::Fase::Acabou,
                    TouchPhase::Started => gesto_camera::Fase::Comecou,
                    _ if !anteriores.contains(&t.id) => gesto_camera::Fase::Comecou,
                    _ => gesto_camera::Fase::Segurando,
                },
                pos: t.position,
            })
            .collect();
        self.dedos_anteriores = toques
            .iter()
            .filter(|t| t.fase != gesto_camera::Fase::Acabou)
            .map(|t| t.id)
            .collect();
        // Joystick primeiro: um dedo que COMECA na metade esquerda de baixo,
        // fora de botao/painel, e' dele — e some da lista da camera e do
        // clique. Painel grande aberto: sem joystick.
        let z = hud_layout::atual();
        let painel = self.painel_grande();
        if painel {
            self.joystick.soltar();
        }
        // TODO o quadrado inferior esquerdo e' do joystick (pedido do dono em
        // 19/09/2026: "as vezes buga e mexe a camera"). A faixa `z.joystick`
        // parava acima da linha da bateria/montaria e antes do meio da tela —
        // o polegar que encostava ali embaixo virava camera. Fora continuam
        // so' os botoes do HUD e o que a interface pega, conferidos no ponto
        // do DEDO.
        let quadrante = hud_layout::quadrante_do_joystick();
        let na_ui: Vec<Vec2> = toques
            .iter()
            .filter(|t| t.fase == gesto_camera::Fase::Comecou && self.ui_pega_dedo(t.pos))
            .map(|t| t.pos)
            .collect();
        let consumido = self.toque_consumido;
        let pode_comecar = |p: Vec2| {
            !painel
                && !consumido
                // Tutorial com foco: o dedo fora do buraco nao anda.
                && foco::passa_ponto(p)
                && (quadrante.contains(p) || z.joystick.contains(p))
                && !z.contem(p)
                && !na_ui.contains(&p)
        };
        let dono = self.joystick.dedo();
        self.joystick
            .quadro(&toques, joystick::RAIO_BASE * z.s, &pode_comecar);
        // O de antes tambem sai: no quadro do soltar o joystick ja' largou o id.
        let resto = joystick::sem_dedos(&toques, &[dono, self.joystick.dedo()]);
        // O HUD e' conferido no ponto do DEDO, nao no do mouse simulado: ele
        // fica onde foi o toque anterior, e um giro que comecasse logo depois
        // de tocar num botao nascia morto (ver `ui_pega_em`).
        let sobre_hud =
            self.toque_consumido || resto.first().is_some_and(|t| self.ui_pega_em(t.pos));
        self.toque_acao = self.gesto_camera.quadro(&resto, sobre_hud);
    }

    /// Andar "na mao": WASD/setas ou o joystick virtual. E' o que pausa auto
    /// missao, viagem, ir-para e a ida ate' o NPC.
    fn andando_na_mao(&self) -> bool {
        let teclas = !self.teclado_bloqueado() && desktop::movimento() != Vec2::ZERO;
        joystick::movimento_manual(teclas, &self.joystick)
    }

    /// Clique no MUNDO neste quadro: com dedo, so' o toque curto no soltar;
    /// sem dedo, o aperto do botao esquerdo como sempre.
    fn clique_no_mundo(&self) -> bool {
        match self.toque_acao {
            gesto_camera::Acao::Clique(_) => true,
            _ => !self.toque_ativo && crate::foco::clique(),
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
        self.camera_suave
            .sincroniza(self.cam_yaw, self.cam_pitch_ajuste, self.cam_zoom);
        let mut dyaw = 0.0f32;
        let mut dzoom = 0.0f32;
        if !self.teclado_bloqueado() && is_key_down(KeyCode::Q) {
            dyaw -= 2.2 * dt;
        }
        if !self.teclado_bloqueado() && is_key_down(KeyCode::E) {
            dyaw += 2.2 * dt;
        }
        let mut mexeu = 0.0f32;
        let segurando = is_mouse_button_down(MouseButton::Right)
            || is_mouse_button_down(MouseButton::Middle);
        let bloqueado = self.teclado_bloqueado() || self.toque_ativo;
        let sobre_ui = self.ui_pega_mouse();
        let delta = self.mouse_camera.quadro(
            Vec2::from(mouse_position()), segurando, sobre_ui, bloqueado,
        );
        dyaw += delta.x * 0.008;
        mexeu += delta.y * 0.004;
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
                        self.camera_suave
                            .inercia
                            .solta(vec2(v.x * 0.008, v.y * 0.004));
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
        if roda != 0.0 && !self.ui_pega_mouse() && !self.teclado_bloqueado() {
            dzoom -= roda.clamp(-4.0, 4.0) * 0.12;
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
        cs.ajuste =
            (base_alvo + cs.ajuste + mexeu).clamp(piso_alvo, render3d::PITCH_MAX) - base_alvo;

        // ── a camera persegue ──
        let (yaw, ajuste, zoom) =
            cs.persegue(self.cam_yaw, self.cam_pitch_ajuste, self.cam_zoom, dt);
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
        self.camera_suave
            .escreveu(self.cam_yaw, self.cam_pitch_ajuste, self.cam_zoom);
    }

    fn enviar_input(&mut self) {
        let agora = get_time();
        if agora - self.ultimo_input < 1.0 / INPUT_HZ {
            return;
        }
        self.ultimo_input = agora;

        let mut dir = if self.teclado_bloqueado() {
            Vec2::ZERO
        } else {
            desktop::movimento()
        };
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
        // Mouse direito fica livre para a câmera; G segura a defesa.
        if !self.teclado_bloqueado() && is_key_down(KeyCode::G) {
            buttons |= shared::protocol::buttons::SECONDARY;
        }
        // Sprint ligado pelo botão ou Shift: o servidor multiplica a velocidade
        // (`SPRINT_SPEED_MULT`) e gasta vigor. A animacao de correr nao olha
        // a tecla — sai da velocidade, igual pra quem esta' de fora.
        // Indo sozinho ha' um tempo (viagem, auto missao, rota por clique), o
        // bit vai ligado igual — ver `corrida`.
        if self.corrida.ativa(self.correndo_auto) {
            buttons |= shared::protocol::buttons::SPRINT;
        }
        // O servidor detecta a BORDA de subida; aqui basta mandar o estado.
        if (!self.teclado_bloqueado() && is_key_down(desktop::PULO)) || self.pulo_segurando {
            buttons |= shared::protocol::buttons::PULO;
        }
        // No celular vem do quarto botao do arco; no PC, Ctrl. O toque e'
        // pulso unico para nao repetir o dash quando o dedo fica apoiado.
        if std::mem::take(&mut self.dash_toque)
            || (!self.teclado_bloqueado()
                && (is_key_down(KeyCode::LeftControl) || is_key_down(KeyCode::RightControl)))
        {
            buttons |= shared::protocol::buttons::DASH;
        }
        // O arco comeca no quadro da tecla, sem esperar a ida e volta. So' a
        // ANIMACAO: quem decide se o degrau de dois blocos foi vencido e' o
        // servidor, e ele responde antes de o arco chegar ao topo.
        let tocou_pulo = std::mem::take(&mut self.pulo_toque);
        if (!self.teclado_bloqueado() && is_key_pressed(desktop::PULO)) || tocou_pulo {
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
        if matches!(self.tela, Tela::Jogando) {
            self.mobs_ui.observar(&self.world);
        }
        self.foco_do_tutorial();
        self.foco_da_trava();
        self.tocar_fila();
        if !self.auto_missao.ativo() && self.fila_de_missoes.is_empty() {
            self.auto_resumo.parar();
        }
        match &self.tela {
            Tela::Jogando if self.economia.ativa => self.desenhar_economia(),
            Tela::Jogando => {
                self.desenhar_mundo();
                if self.carregando_desde.is_some() {
                    self.desenhar_carregando();
                }
            }
            Tela::Servidores => self.tela_servidores(),
            Tela::Login => self.tela_login(),
            Tela::Cadastro => self.tela_cadastro(),
            Tela::EsqueciSenha => self.tela_esqueci(),
            Tela::Personagens => self.tela_personagens(),
            Tela::Conectando => {
                ui::fundo();
                match self.reconexao.as_ref().map(|r| r.tentativas) {
                    None => {
                        let r = ui::painel(420.0, 160.0, "");
                        ui::texto_centro(r.x + r.w * 0.5, r.y + 50.0, "conectando...", 22, ui::OURO);
                    }
                    Some(tentativas) => {
                        let r = ui::painel(520.0, 220.0, "");
                        ui::texto_centro(r.x + r.w * 0.5, r.y + 50.0, "Connection lost — reconnecting…", 22, ui::OURO);
                        if tentativas > 0 {
                            ui::texto_centro(r.x + r.w * 0.5, r.y + 88.0, &format!("Attempt {tentativas}"), 16, crate::hud_estilo::SUAVE);
                        }
                        if ui::botao(Rect::new(r.x + r.w * 0.5 - 80.0, r.y + 130.0, 160.0, 44.0), "Give up", true) {
                            self.reconexao = None;
                            self.net = None;
                            self.busca = Some(api::buscar_canais());
                            self.tela = Tela::Servidores;
                        }
                    }
                }
            }
            Tela::Fila { posicao, total } => {
                let (p, t) = (*posicao, *total);
                self.tela_fila(p, t);
            }
            Tela::Erro(por_que) => {
                let msg = por_que.clone();
                ui::fundo();
                let voltar = if atualizacao::protocolo_incompativel(&msg) {
                    self.atualizacao.desenha_incompativel()
                } else {
                    let r = ui::painel(560.0, 220.0, "didn't work");
                    ui::erro(r.x + r.w * 0.5, r.y + 50.0, &msg);
                    ui::botao(
                        Rect::new(r.x + r.w * 0.5 - 80.0, r.y + 110.0, 160.0, 40.0),
                        "voltar", true,
                    )
                };
                if voltar {
                    self.net = None;
                    self.busca = Some(api::buscar_canais());
                    self.tela = Tela::Servidores;
                }
            }
        }
        // O foco do tutorial vai por CIMA de tudo — HUD, paineis, popup: o
        // que ele apaga tem mesmo que sumir. Depois fecha o quadro: o alvo
        // marcado agora e' o buraco do quadro seguinte.
        // O contador de missoes: por cima de tudo, menos do foco do tutorial
        // (que apaga o que nao interessa e tem que continuar apagando).
        missoes::desenha_contador(&self.missoes, get_time());
        self.recompensas.desenhar(&self.bolsa.nomes, Some((&self.vox, &self.solido)));
        foco::desenha(get_time());
        foco::novo_quadro();
        if let Some(escolha) = self.nivel_ui.desenhar() {
            self.fecha_paineis();
            match escolha {
                nivel_ui::Escolha::IlhaMagica => {
                    self.magica.pedir_abertura();
                    self.envia(ClientMessage::Magica {
                        pedido: shared::magica::PedidoMagica::Painel,
                    });
                    self.dica_da_trava = Some(get_time());
                }
                nivel_ui::Escolha::Missoes => {
                    self.menu_missoes.abrir_secundarias();
                    self.chat.push("Secundárias: toque Pegar, depois Ir. A auto missão segue o objetivo; craft e dungeon pedem sua ação.".into());
                }
                nivel_ui::Escolha::Caca => {
                    self.mapa.abrir();
                    self.mapa.no_mundo = false;
                    self.chat.push("Mapa: toque um círculo FORTE para ir à área com mais mobs. Ao chegar, o auto combate liga.".into());
                }
            }
        }
        self.desenhar_fechar_tutorial();
    }

    fn desenhar_mundo(&mut self) {
        // Equipamento e guarda-roupa chegam completos no login, antes da
        // primeira entidade. Reaplica quando ela existir e a cada quadro:
        // fechar a prévia também restaura o visual confirmado pelo servidor.
        self.world.sincroniza_visual_local(
            &self.bolsa.equip,
            self.guarda_roupa.provando().unwrap_or(self.guarda_roupa_salvo.aparencia),
        );
        render3d::clear();
        let centro = self.world.self_pos().unwrap_or(Vec2::ZERO)
            + chefe_anim::sacudida(self.tremor.0, self.tremor.1, get_time());
        // A camera sobe com o chao. Presa em zero, o jogador some dentro do
        // morro assim que o terreno passou a ter 34 unidades de altura.
        let terreno = self.terreno.as_ref();
        let f = |x: f32, z: f32| terreno.map_or(0.0, |t| t.altura(x, z));
        let vista = render3d::Vista::nova(
            centro,
            self.cam_yaw,
            self.cam_zoom,
            self.cam_pitch,
            self.cam_altura,
            &f,
        );
        set_camera(&vista.cam);
        let modo_sombras = self.config_graficos.sombras;
        // Kōgen-tō is always night: -1 in the late-sun slot tells the world
        // shader so (`render3d::SOLIDO_VERTICE`).
        let noite = render3d::noite();
        let luz_dia = render3d::luz_dia(if modo_sombras == config_graficos::Sombras::Bonitas { 1.0 } else { 0.0 });
        self.solido.set_uniform("LuzDia", luz_dia);
        gpu_estatica::define_luz_dia(luz_dia);
        // Distance fog: centred where the terrain is loaded around (the
        // player), solid sky before the edge of what is loaded. A negative
        // start is night's fog (the night sky).
        let (inicio, fim) = config_graficos::neblina();
        let inicio = if noite { -inicio.max(0.01) } else { inicio };
        let em_volta = self.world.self_pos().unwrap_or(Vec2::ZERO);
        gpu_estatica::define_neblina(em_volta, inicio, fim);
        self.solido
            .set_uniform("Neblina", vec4(em_volta.x, em_volta.y, inicio, fim));
        // The neon city's lamps and signs light what is round them (night =
        // Kōgen-tō).
        {
            let terreno = self.terreno.as_ref();
            let alvo = vec2(vista.cam.target.x, vista.cam.target.z);
            luzes::preparar(noite, alvo, &|x, z| terreno.map_or(0.0, |t| t.altura(x, z)));
        }
        if self.zona_atual == shared::arena::ZONA {
            if let (Some((id, andar, centro)), Some(t)) = (self.dungeon.cenario(), &self.terreno) {
                self.cenario_dungeon.preparar(id, andar, centro, &|x,z| t.altura(x,z));
                self.cenario_dungeon.luzes();
            } else { self.cenario_dungeon.limpar(); }
        } else { self.cenario_dungeon.limpar(); }
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
        self.solido.set_uniform("Crop", recorte);
        self.solido.set_uniform("RecorteZ", corte_z);
        match &self.terreno {
            Some(t) => {
                self.pedacos_desenhados = t.desenha(&vista.cam, recorte, corte_z);
                // Casas no mesmo passe: descarte de face e recorte da camera
                // valem pra elas como valem pra arvore.
                let jogador = self
                    .world
                    .self_pos()
                    .map(|p| vec3(p.x, t.altura(p.x, p.y), p.y));
                self.construcoes
                    .desenha(&vista.cam, jogador, recorte, corte_z);
                if modo_sombras == config_graficos::Sombras::Bonitas {
                    t.desenha_sombras(&vista.cam);
                }
                // Pra onde esta' indo: tracejado rente ao chao, no mesmo passe.
                if let Some(eu) = self.world.self_pos() {
                    let altura = |x: f32, z: f32| t.altura(x, z);
                    rastro::desenha(
                        &self.rastro,
                        eu,
                        self.mapa.viagem.destino(),
                        &altura,
                        get_time() as f32,
                    );
                }
                // Os veios de Energia: desenhados quadro a quadro, com a luz
                // subindo. Antes de os bichos entrarem, pra o facho ficar
                // atras de quem passa na frente dele.
                energia_vfx::desenha(
                    t.energias_visiveis(&vista.cam).into_iter(),
                    &vista.cam,
                    get_time() as f32,
                );
                // Subiu de nivel: o estouro de luz em volta de quem subiu.
                // No mesmo passe dos veios, antes da agua.
                if self.subiu_de_nivel.ativo() {
                    if let Some(eu) = self.world.self_pos() {
                        let y = t.altura(eu.x, eu.y);
                        self.subiu_de_nivel.desenha(vec3(eu.x, y, eu.y), &vista.cam);
                    }
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
        self.cenario_dungeon.desenha();
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
        self.solido.set_uniform("Crop", Vec3::ZERO);
        render3d::draw_entities_com_sombras(
            &mut self.world,
            &self.vox,
            self.alvo,
            &vista,
            modo_sombras,
        );
        // AS PORTAS DE PORÃO, antes de largar o material sólido.
        //
        // Ficam aqui e não entre as entidades porque não SÃO entidades: a
        // posição é calculada (`shared::porao::porta_de`), não vem do
        // servidor. Desenhar no chão de verdade é o que evita porta flutuando
        // — a altura sai do mesmo terreno que o jogador pisa.
        if self.terreno.is_some() {
            let eu = self.world.self_pos();
            let portas = self.porao.portas(&self.zona_atual).to_vec();
            if let Some(terreno) = &self.terreno {
                for (conteudo, pos) in portas {
                    let perto = eu
                        .is_some_and(|e| e.distance(pos) <= shared::porao::ALCANCE_DA_PORTA);
                    let y = terreno.altura(pos.x, pos.y);
                    render3d::desenha_porta_do_porao(vec3(pos.x, y, pos.y), conteudo, perto);
                }
                // Stepping in: the light wraps the character before the trip.
                if let (Some((conteudo, progresso)), Some(e)) = (self.porao.entrando(), eu) {
                    let y = terreno.altura(e.x, e.y);
                    render3d::desenha_entrada_no_portal(
                        vec3(e.x, y, e.y),
                        render3d::cor_do_portal(conteudo),
                        progresso,
                        get_time() as f32,
                    );
                }
            }
        }
        // THE PORÃO FLOOR PLAN: walls, floor, torches and shut gates — only
        // inside a planned run, and only on the Arena, where plans live.
        if self.zona_atual == shared::arena::ZONA {
            if let (Some((conteudo, andar)), Some(terreno)) =
                (self.dungeon.em_curso(), &self.terreno)
            {
                if let Some(p) = shared::planta::da(conteudo) {
                    // The floor's height: the entrance hall is floor.
                    let e = p.centro(p.entrada());
                    let y = terreno.altura(e.x, e.y);
                    self.plantas.de(p).desenha(p, y, andar);
                }
            }
        }
        if noite {
            let terreno = self.terreno.as_ref();
            luzes::desenha_halos(&vista.cam, &|x, z| terreno.map_or(0.0, |t| t.altura(x, z)));
            // Abyssia: the bubble, rays, motes, fish and everyone's helmet.
            if render3d::abismo() {
                abismo::desenha(&vista.cam, &|x, z| terreno.map_or(0.0, |t| t.altura(x, z)));
            }
            gl_use_material(&self.solido);
        }
        luzes::preparar(false, Vec2::ZERO, &|_, _| 0.0);
        self.solido.set_uniform("LuzDia", 0.0f32);
        gpu_estatica::define_luz_dia(0.0);
        // Off again before the panels: the colony model and the previews
        // draw with the same shader and must not fog.
        self.solido.set_uniform("Neblina", Vec4::ZERO);
        gpu_estatica::define_neblina(Vec2::ZERO, 0.0, 0.0);
        gl_use_default_material();
        self.habilidades.desenha_efeitos(&self.world, &vista);
        set_default_camera();
        // Numero de dano, faisca e a borda vermelha, por cima do mundo.
        efeitos::desenha(&self.world, &vista);
        // Quanto falta andar, sobre o destino.
        if let (Some(t), Some(eu)) = (&self.terreno, self.world.self_pos()) {
            let altura = |x: f32, z: f32| t.altura(x, z);
            rastro::desenha_distancia(
                &self.rastro,
                eu,
                self.mapa.viagem.destino(),
                &altura,
                &vista.cam,
            );
        }
        if let Some(e) = self.world.self_id.and_then(|i| self.world.ents.get(&i)) {
            let bolsa = &self.bolsa;
            self.ganhos
                .desenha(&vista.cam, vista.pos_de(e), |id| bolsa.nome(id));
        }
        // O oficio de cada NPC em cima da cabeca; o "!" de missao vai acima.
        icone_npc::desenha(&self.world, &vista, self.world.self_pos());
        {
            let slots = &self.bolsa.slots;
            self.missoes
                .desenha_marcador(&self.world, &vista, &|id| missoes::na_bolsa(slots, id));
        }
        // ── HUD no molde do MIR4 (docs/HUD.md). Posicoes em `hud_layout`. ──
        let z = hud_layout::atual();
        let painel = self.painel_grande();
        let agora_unix = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_secs() as i64);
        let eu = self
            .world
            .self_id
            .and_then(|id| self.world.ents.get(&id))
            .map(|e| {
                (
                    e.state.hp as i32,
                    e.meta.hp_max as i32,
                    e.meta.nivel as u32,
                    e.meta.name.clone().unwrap_or_default(),
                )
            });
        let nivel = match &eu {
            Some((_, _, n, _)) if self.ficha.nivel == 0 => *n,
            _ => self.ficha.nivel,
        };
        // A ILHA MÁGICA, ANTES DE TODO O RESTO DO HUD.
        //
        // O dono: "o hud onde fica o contador da ilha mágica e o adicionar
        // tempo ou sair da ilha fica na frente de todo outro hud do jogo, não
        // quero isso; tem que ficar atrás de tudo". Desenhando primeiro, tudo
        // que vem depois passa por cima dela — e o painel da ilha (janela, e
        // janela é pra ficar em cima) continua no fim do quadro.
        //
        // A ZONA MANDA na tarja: o `dentro` do estado vem do processo da ilha e
        // fica velho quando o jogador sai (a saída é handoff, e na zona de
        // origem ninguém manda estado novo). Sem isto a tarja sobrevivia à
        // saída, com o relógio correndo.
        self.magica.atualiza_zona(self.mapa.zona());
        // E o layout reserva a faixa dela, senão os avisos ficariam por baixo.
        hud_layout::define_tarja_magica(self.magica.dentro(agora_unix));
        // A faixa de PvP vale na ilha inteira, e não só com tempo valendo: quem
        // entrou precisa saber a regra antes de apanhar por ela.
        hud_layout::define_faixa_pvp(self.magica.na_ilha());
        // Com painel grande aberto ela nem desenha: ficar atrás de uma janela
        // que cobre a tela é ficar invisível, e os botões dela continuariam
        // pegando o clique por baixo dela.
        if !painel {
            self.magica.desenha_faixa_pvp(self.world.self_pos());
            // A PORTA DO PORÃO: só aparece se houver uma perto, e só abre na
            // porta com a chave. O servidor confere tudo de novo.
            {
                let slots = self.bolsa.slots.clone();
                let tem = move |chave: u16| {
                    slots.iter().any(|i| i.item_id == chave && i.qty > 0)
                };
                let eu = self.world.self_pos();
                let zona = self.zona_atual.clone();
                if let Some(msg) = self.porao.desenha(&zona, eu, &tem) {
                    self.envia(msg);
                }
            }
            if let Some(pedido) = self.magica.desenha_hud(agora_unix) {
                self.envia(ClientMessage::Magica { pedido });
            }
        }
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
                &self.chat.recentes(get_time()),
            ) {
                self.fecha_paineis();
                self.mapa.abrir();
            }
            self.mapa.desenha_mini(&self.world);
            if let Some((hp, hp_max, _, nome)) = eu.clone() {
                let st = self.bolsa.stats.as_ref();
                let (hp_max, mp_max, vigor_max, poder) = (
                    st.map_or(hp_max, |s| s.hp_max),
                    st.map_or(50, |s| s.mp_max),
                    st.map_or(100, |s| s.stamina_max),
                    st.map(crate::bolsa::poder),
                );
                hud::draw_ficha(
                    &z,
                    &mut self.ficha,
                    get_frame_time(),
                    &nome,
                    nivel,
                    hp,
                    hp_max,
                    mp_max,
                    vigor_max,
                    poder,
                );
                let pk = self.world.self_id.and_then(|id| self.world.ents.get(&id))
                    .map(|e| e.meta.pk).unwrap_or_default();
                if hud::draw_modo_pk(&z, pk) {
                    self.envia(ClientMessage::TogglePkMode { on: !pk.hostil });
                }
                hud::draw_buffs(
                    self.bonus_xp_ate,
                    self.bonus_fortuna_ate,
                    self.bonus_sorte_ate,
                    agora_unix,
                    z.buffs,
                    self.barra.curas(get_time()),
                );
                let conjunto =
                    shared::skills::Conjunto::da_arma(self.bolsa.equip.weapon.unwrap_or(0));
                self.habilidades.barra(
                    conjunto,
                    nivel,
                    self.ficha.mp.unwrap_or(0),
                    &self.evolucao_skills.progresso,
                );
                self.auto_combate.desenha();
                self.auto_coleta.desenha();
                // Alvos de tutorial que moram no HUD.
                foco::marca(foco::chave::AUTO_COMBATE, auto_combate::retangulo());
                foco::marca(foco::chave::AUTO_COLETA, auto_coleta::retangulo());
                foco::marca(foco::chave::SKILL_AUTO, z.skills[0]);
                foco::marca(foco::chave::MINIMAPA, z.minimapa);
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
                    hud::draw_rapidos(
                        &z,
                        itens,
                        self.qtd_rapidos(),
                        auto,
                        recarga,
                        curando,
                        self.barra.arrastando(),
                        &|id| bolsa.nome(id),
                    );
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
                if is_mouse_button_pressed(MouseButton::Right)
                    && auto_coleta::retangulo().contains(mouse)
                {
                    self.fecha_paineis();
                    self.config_coleta.abrir();
                }
                // Segurar um espaco parado (toque longo) tambem abre o
                // configurador; arrastar continua sendo o AUTO.
                let sob_dedo = rects
                    .iter()
                    .position(|r| r.contains(mouse))
                    .map(|i| i as u32);
                if let toque::Toque::Longo(i) = self.toque_barra.quadro(
                    crate::foco::clique(),
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
                    crate::foco::clique(),
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
                // Dentro da dungeon o rastreador e' da DUNGEON (a linha de
                // missao dela ocupa este mesmo lugar): as missoes normais nao
                // aparecem.
                if !self.missoes.aberta && !self.loja.aberta() && !self.dungeon.na_instancia() {
                    // Quem mede a caixa precisa saber o tamanho do grupo.
                    self.missoes.membros_grupo = self.social.grupo.len();
                    let clique = {
                        let slots = &self.bolsa.slots;
                        let nivel = self.ficha.nivel.max(1);
                        let base = shared::xp_for_level_with_mult(nivel, self.ficha.mult_xp);
                        let prox = shared::xp_for_level_with_mult(nivel + 1, self.ficha.mult_xp);
                        let fracao = if prox > base {
                            (self.ficha.xp.saturating_sub(base)) as f32 / (prox - base) as f32
                        } else {
                            0.0
                        };
                        self.missoes.desenha_rastreador(
                            &|id| missoes::na_bolsa(slots, id),
                            self.auto_missao.quest,
                            nivel,
                            fracao,
                        )
                    };
                    match clique {
                        Some(missoes::NoRastreador::Fixar(id)) => {
                            if !self.missoes.fixadas.remove(&id) {
                                self.missoes.fixadas.insert(id);
                            }
                        }
                        Some(missoes::NoRastreador::Missao(id)) => self.iniciar_auto_missao(id),
                        // A aba so' troca o conteudo da caixa. Abrir o diario
                        // daqui era o que o dono NAO queria.
                        Some(missoes::NoRastreador::Aba(grupo)) => self.missoes.aba_grupo = grupo,
                        // As setas andam a janela de três.
                        Some(missoes::NoRastreador::Rolar(pra_frente)) => {
                            let d = &mut self.missoes.desloca_rastreador;
                            *d = if pra_frente {
                                d.saturating_add(1)
                            } else {
                                d.saturating_sub(1)
                            };
                        }
                        None => {}
                    }
                    // A ABA DO GRUPO, dentro da mesma caixa: o rastreador
                    // desenhou a moldura e as abas, aqui vai o conteudo.
                    if self.missoes.aba_grupo {
                        let corpo = self.missoes.corpo_do_rastreador();
                        if let Some(acao) =
                            social_hud::grupo_no_corpo(corpo, &self.social.grupo, &self.world)
                        {
                            self.acao_social_alvo(acao);
                        }
                    }
                }
            }
            // Dentro da dungeon nao ha' rastreador (a linha da missao da
            // instancia ocupa o lugar dele), e sem rastreador nao ha' aba: ali
            // o grupo continua no painel solto, ao lado dela.
            self.social_hud.solto = self.dungeon.na_instancia();
            if self.social_hud.solto {
                if let Some(acao) = self.social_hud.grupo(&z, &self.social.grupo, &self.world) {
                    self.acao_social_alvo(acao);
                }
            }
            let alvo = self
                .alvo
                .and_then(|id| self.world.ents.get(&id))
                .filter(|a| a.morte.is_none())
                .map(|a| {
                    (
                        a.meta.name.clone().unwrap_or_else(|| "?".into()),
                        a.meta.nivel,
                        a.state.hp,
                        a.meta.hp_max,
                        a.state.flags & shared::ent_flags::BOSS != 0,
                    )
                });
            if let Some((nome, nv, hp, hp_max, chefe)) = alvo {
                if hud::draw_alvo(&z, &nome, nv, hp, hp_max, chefe) {
                    self.alvo = None;
                    self.envia(ClientMessage::SetTarget { target: None });
                }
                if chefe {
                    telegrafico::rotulo_de_fase(z.alvo, hp, hp_max);
                }
            } else if let Some((nome, nv, hp, hp_max)) = self
                .world
                .self_pos()
                .and_then(|eu| telegrafico::chefe_perto(&self.world, eu))
            {
                // Chefe em luta por perto sem estar selecionado: a barra dele.
                telegrafico::desenha_barra_de_chefe(z.alvo, &nome, nv, hp, hp_max);
            }
            let jogador = self.alvo_jogador().map(|(id, _)| id);
            if let Some(acao) = self.social_hud.alvo(&z, jogador, jogador.is_some() && self.seguir.alvo == jogador) {
                self.acao_social_alvo(acao);
            }
            foco::marca(foco::chave::MENU, z.menu);
            let selo = self.selo_missoes();
            let selo_diarias = self.selo_diarias();
            let selo_presenca = self.presenca.tem_resgate();
            let selo_ficha = self.selo_ficha();
            let selo_menu = selo
                || selo_diarias
                || selo_presenca
                || selo_ficha
                || self.selo_skills()
                || self.selo_pets();
            match hud::draw_topo(&z, selo, selo_diarias, selo_presenca, selo_menu) {
                Some(hud::Topo::Presenca) => {
                    self.fecha_paineis();
                    for pedido in self.presenca.abrir() {
                        self.envia(pedido);
                    }
                }
                Some(hud::Topo::Bolsa) => {
                    self.fecha_paineis();
                    self.bolsa.abrir();
                }
                Some(hud::Topo::Missoes) => self.abrir_diario(),
                Some(hud::Topo::Diarias) => {
                    self.fecha_paineis();
                    self.diarias.abrir();
                }
                Some(hud::Topo::Grupo) => self.chat.push("Party: coming soon.".into()),
                Some(hud::Topo::Avisos) => self.chat.push("Notices: nothing new.".into()),
                Some(hud::Topo::Menu) => {
                    self.fecha_paineis();
                    self.menu.abrir();
                }
                None => {}
            }
        }
        // Montaria: ao lado da bateria, sempre na tela.
        {
            let montado = self.eu_montado();
            if montado {
                self.montarias.montou();
            }
            let progresso = self.montarias.progresso(get_time());
            if hud::draw_botao_montaria(&z, montado, progresso, self.tem_montaria()) {
                self.alternar_montaria();
            }
        }
        if hud::draw_sprint(&z, self.corrida.ativa(self.correndo_auto)) {
            self.corrida.alternar(self.correndo_auto);
        }
        // Dash ocupa o antigo quarto slot do arco; Pulo subiu uma fileira.
        let dash_restante = (self.dash_recarga.0 - get_time()).max(0.0) as f32;
        if hud::draw_dash(&z, dash_restante, self.dash_recarga.1) {
            self.dash_toque = true;
        }
        // Pulo: sem tecla no celular, o botao e' o unico jeito de pular.
        {
            let no_ar = self
                .world
                .self_id
                .and_then(|id| self.world.ents.get(&id))
                .is_some_and(|e| e.pulo_local > 0.0 || e.voando);
            let (tocou, segurando) = hud::draw_pulo(&z, no_ar);
            self.pulo_toque |= tocou;
            self.pulo_segurando = segurando;
        }
        // A bateria do modo economia fica SEMPRE na tela, com ou sem painel.
        if hud::draw_botao_economia(&z) {
            self.entrar_economia();
        }
        if let Some((texto, cor)) = self.texto_da_faixa() {
            hud_layout::desenha_faixa(&z, &texto, cor);
        }

        // Joystick virtual: so' aparece com o dedo na tela.
        self.joystick.desenha(&hud_layout::atual());
        if self.coleta_hud.ativa() {
            self.coleta_hud.desenha(&z, get_time());
        }
        if eu.is_some() {
            hud::draw_exp(&z, &self.ficha, nivel);
        }
        if self.loja.aberta() {
            let nome = self
                .loja
                .vendedor
                .and_then(|id| self.world.ents.get(&id))
                .and_then(|e| e.meta.name.clone())
                .unwrap_or_else(|| "Shop".into());
            let cobre: u64 = self
                .bolsa
                .slots
                .iter()
                .filter(|s| s.item_id == shared::item_id::COPPER && s.instance.is_none())
                .map(|s| s.qty as u64)
                .sum();
            for pedido in self.loja.desenha(
                &self.bolsa.nomes,
                &self.bolsa.slots,
                self.bolsa.ouro,
                cobre,
                &nome,
                Some((&self.vox, &self.solido)),
            ) {
                self.envia(pedido);
            }
        }
        if self.craft.aberto()
            || self.forja.aberto()
            || self.config_barra.aberto
            || self.config_coleta.aberto
            || self.config_combate.aberto
            || self.config_interface.aberto
            || self.config_graficos.aberto
        {
            hud_layout::escurece(0.55);
        }
        if self.config_graficos.aberto {
            self.config_graficos.desenha();
        }
        if self.config_interface.aberto {
            // Vale no quadro seguinte e vai pro servidor pelas preferencias.
            match self
                .config_interface
                .desenha(hud_layout::escala_ui(), self.economia.auto_min())
            {
                Some(config_interface::Mudanca::Escala(nova)) => hud_layout::define_escala_ui(nova),
                Some(config_interface::Mudanca::EconomiaAuto(min)) => {
                    self.economia.auto_min = Some(min)
                }
                Some(config_interface::Mudanca::EconomiaAgora) => self.entrar_economia(),
                // TROCA NA HORA: o proximo quadro ja' desenha na lingua nova,
                // porque a traducao acontece ao desenhar. Nada recarrega, nada
                // reconecta — e o que estiver na tela muda junto.
                Some(config_interface::Mudanca::Idioma(lang)) => {
                    shared::idioma::definir(lang);
                    // Salva na hora: o jogador que troca o idioma e fecha o
                    // jogo sem entrar em nenhum personagem ainda quer a
                    // escolha de volta na proxima abertura.
                    let mut prefs = crate::lembranca::carrega();
                    prefs.idioma = lang;
                    crate::lembranca::salva(&prefs);
                }
                None => {}
            }
        }
        if self.config_combate.aberto {
            let (mut ordem, mut pvp) = (self.auto_combate.ordem.clone(), self.auto_combate.pvp);
            if self.config_combate.desenha(&mut ordem, &mut pvp) {
                // Vai pro servidor pelas preferências, como o auto coleta.
                self.auto_combate.ordem = ordem;
                self.auto_combate.pvp = pvp;
            }
        }
        if self.config_coleta.aberto {
            let (mut tipos, mut energia, mut raio, mut defender) = (
                self.auto_coleta.tipos,
                self.auto_coleta.energia,
                self.auto_coleta.raio,
                self.auto_coleta.defender,
            );
            if self
                .config_coleta
                .desenha(&mut tipos, &mut energia, &mut raio, &mut defender)
            {
                // Vai pro servidor pelas preferencias (sincronia a cada quadro).
                self.auto_coleta.tipos = tipos;
                self.auto_coleta.energia = energia;
                self.auto_coleta.raio = raio;
                self.auto_coleta.defender = defender;
            }
        }
        // Com o "Onde obter" aberto por cima, o configurador fica parado.
        if self.config_barra.aberto && !self.onde_obter.aberto() {
            let bolsa = &self.bolsa;
            if self
                .config_barra
                .desenha(&mut self.barra, &bolsa.slots, &|id| bolsa.nome(id))
            {
                self.salvar_barra();
            }
            if std::mem::take(&mut self.config_barra.mexeu_no_limiar) {
                self.tutorial(shared::quests::tutorial::POCAO_LIMIAR);
            }
        }
        // Com o "Onde obter" aberto os paineis de baixo nao desenham: o toque
        // no popup nao pode cair num botao deles.
        let onde = self.onde_obter.aberto();
        if self.craft.aberto() && !onde {
            let nivel = self.ficha.nivel.max(1);
            if let Some(m) = self.craft.desenha(
                &self.bolsa.slots,
                &self.bolsa.nomes,
                nivel,
                get_time(),
                Some((&self.vox, &self.solido)),
            ) {
                self.envia(m);
            }
        }
        if self.forja.aberto() && !onde {
            if let Some(m) = self.forja.desenha(
                &self.bolsa.slots,
                &self.bolsa.equip,
                &self.bolsa.nomes,
                get_time(),
                Some((&self.vox, &self.solido)),
            ) {
                self.envia(m);
            }
        }
        if self.evolucao_skills.aberto && !onde {
            let cobre: u32 = self
                .bolsa
                .slots
                .iter()
                .filter(|s| s.item_id == shared::item_id::COPPER && s.instance.is_none())
                .map(|s| s.qty)
                .sum();
            if let Some(pedido) = self.evolucao_skills.desenha(
                &self.habilidades.catalogo,
                self.ficha.nivel.max(1),
                cobre,
            ) {
                self.envia(pedido);
            }
        }
        if self.ficha_ui.aberta && !onde {
            let pedido = self.ficha_ui.desenha(
                self.personagem_atual.as_deref().unwrap_or("Character"),
                self.ficha.nivel.max(1),
                self.ficha.xp,
                self.ficha.mult_xp,
                self.bolsa.stats.as_ref(),
                self.bolsa.energia,
                self.bolsa.equip.weapon,
            );
            if let Some(pedido) = pedido {
                self.envia(pedido);
            }
        }
        if self.pets_ui.aberta && !onde {
            let agora_unix = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |d| d.as_secs() as i64);
            // `desenha` so' le' a bolsa; o emprestimo do palco sai antes do
            // `envia`, que precisa do self inteiro.
            let pedido = {
                let equip = self.bolsa.equip;
                let slots = std::mem::take(&mut self.bolsa.slots);
                let p = self
                    .pets_ui
                    .desenha(&self.vox, &self.solido, &equip, &slots, agora_unix);
                self.bolsa.slots = slots;
                p
            };
            if let Some(pedido) = pedido {
                self.envia(pedido);
            }
        }
        if self.missoes.aberta && !onde {
            let slots = &self.bolsa.slots;
            let pedidos = self
                .missoes
                .desenha(&self.bolsa.nomes, &|id| missoes::na_bolsa(slots, id));
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
                    nomes: &self.bolsa.nomes,
                };
                self.menu_missoes.desenha(&c)
            };
            // MARCAR PRA FILA TAMBÉM FIXA NO RASTREADOR.
            //
            // O dono: "as quests marcadas têm que ficar fixadas na esquerda,
            // no atalho lá". Faz sentido: marcar é dizer "vou fazer estas", e
            // o rastreador é onde se acompanha o que se está fazendo. As
            // fixadas sobem na lista (`ordem_com_fixadas`).
            //
            // Por EVENTO, e não espelhando o conjunto: espelhar apagaria as
            // que o jogador fixou na mão toda vez que ele marcasse qualquer
            // coisa no menu.
            if let Some((id, marcada)) = self.menu_missoes.marca_mudou() {
                if marcada {
                    self.missoes.fixadas.insert(id);
                } else {
                    self.missoes.fixadas.remove(&id);
                }
            }
            for id in self.menu_missoes.desfixar() {
                self.missoes.fixadas.remove(&id);
            }
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
                    nomes: &self.bolsa.nomes,
                };
                self.diarias.desenha(&c, &self.bolsa.nomes)
            };
            if let Some(c) = clique {
                self.clique_menu_missoes(c);
            }
        }
        let agora = get_time();
        // A AUTO MISSÃO CONDUZ A CONVERSA.
        //
        // O dono: "no auto missão tem que aceitar e entregar missões no NPC
        // de maneira automática também, clicar em próximo na conversa etc".
        // O auto já anda até o NPC e abre a fala; parar ali e pedir quatro
        // toques interrompe justamente o que ele automatizou.
        //
        // O desenho vem DEPOIS, e o resultado dos dois cai no mesmo `match`:
        // o que o dedo faz e o que o auto faz têm que ser a mesma coisa, ou
        // viram dois caminhos pro mesmo botão.
        let pelo_auto = if self.auto_missao.ativo() {
            self.dialogo.conduzir(agora, Self::AUTO_FALA_S)
        } else {
            dialogo::Resultado::Nada
        };
        let desenhado = self.dialogo.desenha();
        let resultado = if matches!(desenhado, dialogo::Resultado::Nada) {
            pelo_auto
        } else {
            desenhado
        };
        match resultado {
            dialogo::Resultado::Conversou { npc, .. } => {
                self.envia(ClientMessage::ConcluirConversa {
                    npc_eid: npc.0 as u64,
                });
                if self.auto_missao.ativo() {
                    self.auto_missao.conversou(agora);
                } else {
                    // A mao: depois da conversa o NPC atende como sempre (a
                    // loja do Alquimista abre).
                    self.envia(ClientMessage::Interact {
                        target_eid: Some(npc.0 as u64),
                    });
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
        // Com o mapa-mundi aberto, repede o estado dos chefes no ritmo do
        // heartbeat. Fechado, nao pede nada: quem nao esta' olhando nao
        // precisa saber que um chefe morreu em outra ilha.
        // Vale pras DUAS abas: o `MapaDaIlha` chega uma vez so', na entrada
        // da zona, entao o "vivo" dele envelhece em minutos. O mesmo pedido
        // que enche o mapa-mundi reabastece a ilha — uma fonte so' pros dois,
        // que e' o que impede as duas abas de discordarem.
        if self.mapa.aberto && self.mundo.precisa_pedir(get_time()) {
            self.envia(ClientMessage::Mundo);
        }
        if !self.mapa.aberto {
            self.mundo.fechou();
        }
        let agora_unix = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);
        // DUNGEONS: O PRIMEIRO DOS PAINEIS A DESENHAR, LOGO O DE BAIXO.
        //
        // Isto ficava depois da bolsa, do mercado, das lojas, do social e do
        // Menu, e em modo imediato quem desenha depois fica POR CIMA: a janela
        // da dungeon tapava todos eles. O dono, em 29/09/2026: "dungeon hud is
        // in front of every other hud, i dont want that, it should be behind".
        //
        // Aqui em cima ele e' o primeiro painel do quadro, entao todo o resto
        // do HUD passa a cobri-lo.
        {
            let eu = self.personagem_atual.clone().unwrap_or_default();
            let ctx = dungeon_ui::Contexto {
                nomes: &self.bolsa.nomes,
                palco: Some((&self.vox, &self.solido)),
                ouro: self.bolsa.ouro,
                eu: &eu,
                zona: &self.zona_atual,
            };
            for pedido in self.dungeon.desenha(&ctx, get_time()) {
                self.envia(pedido);
            }
            // "Go to the door": o painel não entra, ele encaminha.
            if let Some((pos, nome)) = self.dungeon.ir_para_a_porta.take() {
                self.dungeon.fechar();
                self.iniciar_ir_para(ir_para::Alvo {
                    objetivo: ir_para::Objetivo::Lugar,
                    pos,
                    raio: shared::porao::ALCANCE_DA_PORTA,
                    rotulo: nome,
                });
            }
        }
        match self
            .mapa
            .desenha_grande(&self.world, nivel, &self.mundo, agora_unix)
        {
            Some(mapa::Entrada::Dungeon(id)) => {
                self.mapa.aberto = false;
                for pedido in self.dungeon.abrir_em(id) { self.envia(pedido); }
            }
            Some(mapa::Entrada::Ir(alvo)) => {
                self.iniciar_ir_para(alvo);
                self.tutorial(shared::quests::tutorial::MAPA_IR);
            }
            Some(mapa::Entrada::Viajar(p)) => {
                self.iniciar_viagem(p);
                self.tutorial(shared::quests::tutorial::MAPA_IR);
            }
            None => {}
        }
        // A bolsa por cima do mundo.
        let pedido_da_bolsa = if onde {
            None
        } else {
            self.bolsa.aparencia = self.minha_aparencia();
            self.bolsa.skins = self.world.self_id
                .and_then(|id| self.world.ents.get(&id))
                .map_or(0, |e| e.meta.skins);
            self.bolsa
                .desenha(&self.vox, &self.solido, self.craft.receitas_atuais())
        };
        if let Some(alvo) = self.bolsa.refinar.take() {
            self.fecha_paineis();
            self.forja.abrir_em(alvo);
        }
        if let Some(item) = self.bolsa.combinar.take() {
            self.fecha_paineis();
            self.craft.abrir_combinacao_do_item(item);
        }
        if let Some(item) = self.bolsa.craftar.take() {
            self.fecha_paineis();
            self.craft.abrir_pelo_item(item);
        }
        if let Some((item, grau, tier)) = self.bolsa.aprimorar.take() {
            self.fecha_paineis();
            self.craft.abrir_aprimoramento(item, grau, tier);
        }
        if let Some(pedido) = pedido_da_bolsa {
            // Pocao de efeito ja' ativa: pergunta antes de jogar fora o tempo
            // que resta (o servidor renova a hora cheia, nao soma).
            let buff = match &pedido {
                ClientMessage::UseItem { slot } => self
                    .bolsa
                    .slots
                    .get(*slot as usize)
                    .map(|s| s.item_id)
                    .filter(|id| self.buff_ativo_de(*id).is_some())
                    .map(|item| (*slot, item)),
                _ => None,
            };
            match buff {
                Some((slot, item)) => {
                    self.confirmar = Some(confirmar::Pendente::Bolsa { slot, item })
                }
                None => self.envia(pedido),
            }
        }
        let eu_social = self.personagem_atual.clone().unwrap_or_default();
        for pedido in
            self.social
                .desenha(&eu_social, self.teclado.digitado(), &self.vox, &self.solido)
        {
            self.envia(pedido);
        }
        if self.mercado.aberto && !onde {
            let ctx = mercado_ui::Contexto {
                slots: &self.bolsa.slots,
                nomes: &self.bolsa.nomes,
                ouro: self.bolsa.ouro,
                nivel: self.ficha.nivel.max(self.bolsa.nivel),
                digitado: self.teclado.digitado(),
                vox: &self.vox,
                solido: &self.solido,
                energia: self.bolsa.energia,
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
                self.iniciar_ir_para(ir_para::Alvo {
                    objetivo: ir_para::Objetivo::Npc,
                    pos,
                    raio: 0.0,
                    rotulo: nome,
                });
            }
        }
        // "Onde obter": a lupa de algum painel pediu; o popup vai por cima.
        let pedido_onde = [
            self.craft.onde_obter(),
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
            if let Some(ir) = self
                .onde_obter
                .desenha(&nome, &ops, Some((&self.vox, &self.solido)))
            {
                self.executar_onde_obter(ir);
            }
        }
        if self.mobs_ui.aberto && !onde {
            if let Some(pedido) = self.mobs_ui.desenha(
                self.bolsa.stats.as_ref(),
                &self.bolsa.equip,
                self.ficha_ui.nivel_da_arma(self.bolsa.equip.weapon),
                &self.vox, &self.solido,
            ) {
                let eu = self.world.self_pos().unwrap_or(Vec2::ZERO);
                let destino = self.mapa.info.as_ref().and_then(|info| {
                    if pedido.chefe {
                        info.chefes.iter().find(|c| c.kind == pedido.kind && c.nivel == pedido.nivel)
                            .map(|c| (vec2(c.centro[0], c.centro[1]), 8.0))
                    } else {
                        mapa::zona_mais_perto(&info.zonas, pedido.kind, eu,
                            self.ficha.nivel.max(self.bolsa.nivel))
                            .map(|z| (vec2(z.centro[0], z.centro[1]), z.raio))
                    }
                });
                if let Some((pos, raio)) = destino {
                    self.mobs_ui.fechar();
                    self.iniciar_ir_para(ir_para::Alvo {
                        objetivo: ir_para::Objetivo::Lugar,
                        pos, raio, rotulo: pedido.nome,
                    });
                } else {
                    self.chat.push("Esse mob não aparece na ilha atual. Viaje para a ilha dele antes de usar Ir.".into());
                }
            }
        }
        // O Menu por cima de tudo.
        if self.menu.aberto {
            let (nome, arma) = (
                eu.as_ref().map_or(String::new(), |e| e.3.clone()),
                shared::skills::Conjunto::da_arma(self.bolsa.equip.weapon.unwrap_or(0))
                    .nome()
                    .to_string(),
            );
            let tem = |id: u16| missoes::na_bolsa(&self.bolsa.slots, id) as u64;
            let saldos = [
                ("Gold", self.bolsa.ouro),
                ("Copper", tem(shared::constants::item_id::COPPER)),
                ("Darksteel", tem(shared::constants::item_id::DARKSTEEL)),
            ];
            let mut selos: Vec<menu::Item> = Vec::new();
            if self.social.convite.is_some() {
                selos.push(menu::Item::Grupo);
            }
            if !self.social.estado.recebidos.is_empty() {
                selos.push(menu::Item::Amigos);
            }
            if self.social.correio_pendente() {
                selos.push(menu::Item::Correio);
            }
            if !self.social.estado.convites_cla.is_empty() {
                selos.push(menu::Item::Clan);
            }
            if self.selo_missoes() {
                selos.push(menu::Item::Missoes);
            }
            if self.selo_diarias() {
                selos.push(menu::Item::Diarias);
            }
            if self.selo_ficha() {
                selos.push(menu::Item::Ficha);
            }
            if self.selo_skills() {
                selos.push(menu::Item::Habilidades);
            }
            if self.selo_pets() {
                selos.push(menu::Item::Pets);
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
        // Morte e "Recover XP" por cima de tudo. So' clique: Esc nao revive.
        let agora_unix = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_secs() as i64);
        // Calendario de presenca (icone do topo, Menu, ou sozinho no login).
        {
            let agora_unix = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |d| d.as_secs() as i64);
            for pedido in
                self.presenca
                    .desenha(&self.bolsa.nomes, agora_unix, &self.vox, &self.solido)
            {
                self.envia(pedido);
            }
        }
        // Nao no quadro do toque que fechou o dialogo: o mesmo toque, fora do
        // menu recem-aberto, o fecharia na hora.
        if !self.dialogo.aberto && !crate::foco::clique() {
            if let Some(d) = self.viagem_pendente.take() {
                self.fecha_paineis();
                self.missoes.fecha();
                self.abrir_viagem(d);
            }
            if let Some(cofre) = self.banco_pendente.take() {
                self.fecha_paineis();
                self.missoes.fecha();
                self.banco.abrir(cofre);
            }
        }
        let auras = shared::auras::equipamento(&self.bolsa.equip);
        if let Some(msg) = self.guarda_roupa.desenha(&self.vox, &self.solido, self.bolsa.equip.weapon.unwrap_or(0), auras, self.bolsa.equip.montaria) {
            self.envia(msg);
        }
        nivel_vfx::desenha_faixa(&self.subiu_de_nivel, self.ficha.nivel);
        if let Some(pedido) = self.escolha_npc.desenha() {
            self.envia(pedido);
        }
        if let Some(pedido) = self.banco.desenha(
            &self.bolsa.slots,
            self.bolsa.ouro,
            Some((&self.vox, &self.solido)),
        ) {
            self.envia(pedido);
        }
        // O Capitao nao leva mais pra propria ilha: ela virou painel, e abre
        // pelo Menu de qualquer lugar. `false` tira a linha "My Island" da
        // lista de destinos.
        match self.viagem.desenha(false) {
            Some(viagem_ui::Escolha::Ilha(ilha)) => self.envia(ClientMessage::Viajar { ilha }),
            Some(viagem_ui::Escolha::MinhaIlha) => {}
            None => {}
        }
        {
            let nome_item = |id: u16| self.bolsa.nome(id);
            if let Some(pedido) = self.colonia.desenha(&nome_item, &self.solido, &self.vox) {
                self.envia(ClientMessage::Colonia { pedido });
            }
        }
        // A ILHA MAGICA: aqui so' o PAINEL (quando aberto), que e' janela e vem
        // por cima. A tarja do relogio desenha no comeco do quadro, atras de
        // todo o resto do HUD.
        if let Some(pedido) = self.magica.desenha(get_time(), agora_unix) {
            self.envia(ClientMessage::Magica { pedido });
        }
        // A TARJA e a faixa de PvP desenham no COMECO do quadro (mais acima):
        // aqui fica so' o painel, que e' janela e janela vem por cima.
        // Loja de cash e janela de montarias (Menu).
        self.loja_tp.define_personagem(self.guarda_roupa_salvo.aparencia, self.bolsa.equip.weapon.unwrap_or(0));
        for pedido in self.loja_tp.desenha(&self.vox, &self.solido) {
            self.envia(pedido);
        }
        let equip = self.bolsa.equip;
        let slots = std::mem::take(&mut self.bolsa.slots);
        let (acao, msg) = self
            .montarias
            .desenha(&self.vox, &self.solido, &equip, &slots);
        self.bolsa.slots = slots;
        if let Some(msg) = msg {
            self.envia(msg);
        }
        match acao {
            Some(montarias_ui::Acao::AbrirLoja) => {
                self.montarias.fechar();
                for pedido in self.loja_tp.abrir() {
                    self.envia(pedido);
                }
            }
            Some(montarias_ui::Acao::Montar) => {
                self.montarias.fechar();
                self.alternar_montaria();
            }
            None => {}
        }
        self.invocacao.desenha(
            &self.bolsa.nomes,
            &self.habilidades.catalogo,
            &self.vox,
            &self.solido,
            get_time(),
        );
        // "Tem certeza?" do buff ja' ativo, por cima de tudo.
        if let Some(p) = self.confirmar {
            let nome = self.bolsa.nome(p.item());
            let resta = self.buff_ativo_de(p.item()).unwrap_or(0);
            match confirmar::desenha(p, &nome, resta) {
                Some(true) => {
                    self.confirmar = None;
                    match p {
                        confirmar::Pendente::Bolsa { slot, .. } => {
                            self.envia(ClientMessage::UseItem { slot })
                        }
                        confirmar::Pendente::Barra { i, forte, .. } => {
                            self.usar_espaco_sem_perguntar(i, forte)
                        }
                    }
                }
                Some(false) => self.confirmar = None,
                None => {}
            }
        }
        // Voltou do modo economia: "While you were away", ate' fechar.
        {
            let bolsa = &self.bolsa;
            self.economia
                .desenha_resumo(&|id| bolsa.nome(id), &self.vox, &self.solido);
        }
        // Dentro da dungeon a derrota e' o "Reviver em N s" dela, sem cidade.
        if !self.dungeon.na_instancia() {
            if let Some(pedido) = self.morte.desenha(self.bolsa.ouro, agora_unix) {
                self.envia(pedido);
            }
        }
    }

    fn tela_servidores(&mut self) {
        ui::fundo();
        if self.atualizacao.desenha() { return; }
        if self.novidades.desenha() { return; }
        let r = ui::painel(560.0, 460.0, "choose the server");
        let cx = r.x + r.w * 0.5;

        if self.busca.is_some() {
            ui::texto_centro(cx, r.y + 60.0, "looking for servers...", 20, ui::OURO);
            return;
        }
        if self.canais.is_empty() {
            ui::erro(cx, r.y + 50.0, "no server online");
            ui::texto_centro(
                cx,
                r.y + 78.0,
                "check the web (/api/channels)",
                16,
                ui::OURO,
            );
            if ui::botao(
                Rect::new(cx - 80.0, r.y + 110.0, 160.0, 40.0),
                "search again",
                true,
            ) {
                self.busca = Some(api::buscar_canais());
            }
            return;
        }

        // So' os SERVIDORES (realms), com a soma de todas as ilhas. A ilha nao
        // se escolhe: e' onde o personagem esta'. Entra-se pela porta da ilha
        // inicial (`api::porta_de_entrada`) e, se o personagem escolhido mora
        // em outra, o servidor manda reconectar la' (`TrocarZona`).
        let mut realms: Vec<(String, u32, u32)> = Vec::new();
        for c in &self.canais {
            match realms.iter_mut().find(|(n, _, _)| n == c.realm()) {
                Some(e) => {
                    e.1 += c.jogadores;
                    e.2 += c.capacidade;
                }
                None => realms.push((c.realm().to_string(), c.jogadores, c.capacidade)),
            }
        }
        let mut y = r.y + 10.0;
        let mut escolhido = None;
        for (nome, jog, cap) in &realms {
            let linha = Rect::new(r.x, y, r.w, 46.0);
            if ui::linha(linha, nome, &format!("{jog} online"), false) {
                escolhido = Some(nome.clone());
            }
            ui::barra(
                Rect::new(r.x + 12.0, y + 34.0, r.w - 24.0, 5.0),
                if *cap > 0 {
                    *jog as f32 / *cap as f32
                } else {
                    0.0
                },
            );
            y += 54.0;
        }
        if let Some(n) = escolhido {
            if let Some(host) = api::porta_de_entrada(&self.canais, &n) {
                self.host = Some(host);
                self.realm = Some(n);
                self.tela = Tela::Login;
            }
        }
        if ui::botao(
            Rect::new(r.x + r.w - 140.0, r.y + r.h - 44.0, 140.0, 40.0),
            "atualizar",
            true,
        ) {
            self.busca = Some(api::buscar_canais());
        }
    }

    /// "Esqueci minha senha": pede o e-mail e manda o link.
    ///
    /// O recado é o MESMO exista ou não a conta ("se houver uma conta com
    /// esse e-mail, o link já saiu"). Dizer "e-mail não encontrado" seria
    /// transformar esta tela num verificador de quem joga aqui — e o
    /// servidor responde 200 nos dois casos justamente por isso.
    fn tela_esqueci(&mut self) {
        ui::fundo();
        const ALTURA: f32 = 300.0;
        let topo = (screen_height() - ALTURA) * 0.5;
        let aberto = nativo::TECLADO_NA_TELA && self.teclado_virtual.aberto();
        ui::subir_paineis(teclado_virtual::deslocamento(
            aberto,
            screen_height(),
            topo,
            topo + 134.0,
        ));
        let r = ui::painel(460.0, ALTURA, "I forgot my password");
        ui::subir_paineis(0.0);
        let cx = r.x + r.w * 0.5;
        let [ce, cenviar, cvoltar] = layout_do_esqueci(r);

        if let Some(rx) = &self.esq_pedido {
            if let Ok(resp) = rx.try_recv() {
                self.esq_pedido = None;
                self.esq_recado = Some(match resp {
                    Ok(()) => (
                        "Se houver uma conta com esse e-mail, o link já saiu.                          Confira a caixa de entrada e o spam."
                            .into(),
                        false,
                    ),
                    Err(e) => (format!("failed: {e}"), true),
                });
            }
        }
        let esperando = self.esq_pedido.is_some();

        ui::texto_centro(cx, r.y + 62.0, "Type the account's e-mail.", 14, ui::APOIO);
        let digitado = self.teclado.digitado().to_vec();
        let mut email = std::mem::take(&mut self.esq_email);
        if ui::campo(ce, "e-mail", &mut email, true, false, &digitado) {
            self.campo_login_ativo = true;
        }
        self.esq_email = email;

        let enter = is_key_pressed(KeyCode::Enter) || is_key_pressed(KeyCode::KpEnter);
        let pode = self.esq_email.contains('@') && !esperando;
        let rotulo = if esperando {
            "enviando…"
        } else {
            "send the link"
        };
        if (ui::botao(cenviar, rotulo, pode) || (pode && enter)) && pode {
            self.campo_login_ativo = false;
            self.esq_recado = None;
            self.esq_pedido = Some(api::esqueci_a_senha(self.esq_email.trim()));
        }
        if ui::botao(cvoltar, "voltar", !esperando) {
            self.campo_login_ativo = false;
            self.tela = Tela::Login;
        }
        if let Some((m, ruim)) = self.esq_recado.clone() {
            if ruim {
                ui::erro(cx, r.y + 30.0, &m);
            } else {
                // Recado longo e de duas linhas: centrado, na cor de apoio.
                for (i, linha) in quebra_em_linhas(&m, 52).iter().take(2).enumerate() {
                    ui::texto_centro(cx, r.y + 266.0 + i as f32 * 17.0, linha, 13, ui::OURO);
                }
            }
        }
    }

    /// Criar conta por usuário e senha.
    ///
    /// O `/api/register` do `web` sempre existiu e nunca teve tela: quem não
    /// tinha conta dependia de alguém criar uma no banco. Com o login do
    /// Google no ar ficou pior, porque passou a haver UM jeito de entrar
    /// sozinho e ele exigia conta Google.
    fn tela_cadastro(&mut self) {
        ui::fundo();
        const ALTURA: f32 = 496.0;
        let topo = (screen_height() - ALTURA) * 0.5;
        let p = layout_do_cadastro(Rect::new(0.0, 0.0, 1.0, 1.0));
        let _ = p;
        // O campo com foco pode ser o terceiro, e é ele que o teclado da tela
        // não pode cobrir.
        let fundo_campo = topo + 60.0 + 52.0 + self.cad_foco as f32 * 76.0 + 42.0;
        let aberto = nativo::TECLADO_NA_TELA && self.teclado_virtual.aberto();
        ui::subir_paineis(teclado_virtual::deslocamento(
            aberto,
            screen_height(),
            topo,
            fundo_campo,
        ));
        let r = ui::painel(460.0, ALTURA, "create account");
        ui::subir_paineis(0.0);
        let cx = r.x + r.w * 0.5;
        let [cu, ce, cs, cs2, ccriar, cvoltar] = layout_do_cadastro(r);

        // A RESPOSTA do pedido em voo. Lida antes de desenhar pra o recado já
        // aparecer no mesmo quadro em que chega.
        if let Some(rx) = &self.cad_pedido {
            if let Ok(resp) = rx.try_recv() {
                self.cad_pedido = None;
                use api::RespostaCadastro as R;
                match resp {
                    R::Criada => {
                        // Entra direto: o jogador acabou de digitar usuário e
                        // senha, pedir de novo na tela ao lado seria só
                        // desconfiança do nosso próprio cadastro.
                        self.usuario = self.cad_usuario.clone();
                        self.senha = self.cad_senha.clone();
                        self.cad_senha.clear();
                        self.cad_senha2.clear();
                        self.erro_login = None;
                        self.token_login = None;
                        self.tela = Tela::Login;
                        self.conectar();
                        return;
                    }
                    R::JaExiste(m) => self.cad_recado = Some((m, true)),
                    R::Recusado(m) => self.cad_recado = Some((m, true)),
                    R::Erro(m) => self.cad_recado = Some((format!("failed: {m}"), true)),
                }
            }
        }

        let esperando = self.cad_pedido.is_some();
        let digitado = self.teclado.digitado().to_vec();
        let mut campos = [
            (cu, "username", std::mem::take(&mut self.cad_usuario), false),
            (ce, "e-mail", std::mem::take(&mut self.cad_email), false),
            (cs, "senha", std::mem::take(&mut self.cad_senha), true),
            (
                cs2,
                "repeat the password",
                std::mem::take(&mut self.cad_senha2),
                true,
            ),
        ];
        for (i, (rect, rotulo, valor, senha)) in campos.iter_mut().enumerate() {
            if ui::campo(*rect, rotulo, valor, self.cad_foco == i, *senha, &digitado) {
                self.cad_foco = i;
                self.campo_login_ativo = true;
            }
        }
        self.cad_usuario = std::mem::take(&mut campos[0].2);
        self.cad_email = std::mem::take(&mut campos[1].2);
        self.cad_senha = std::mem::take(&mut campos[2].2);
        self.cad_senha2 = std::mem::take(&mut campos[3].2);

        // Tab e Enter andam pelos campos, como no login.
        if is_key_pressed(KeyCode::Tab) {
            self.cad_foco = (self.cad_foco + 1) % 4;
        }
        let enter = is_key_pressed(KeyCode::Enter) || is_key_pressed(KeyCode::KpEnter);
        if enter && self.cad_foco < 3 {
            self.cad_foco += 1;
        }

        // AS MESMAS REGRAS DO SERVIDOR, conferidas aqui.
        //
        // Não é desconfiança dele: é que uma viagem de rede para ouvir "senha
        // deve ter 6-128 chars" é uma espera que o jogador não precisa pagar.
        // O servidor continua sendo quem decide — isto aqui só adianta.
        let motivo = if self.cad_usuario.trim().is_empty() || self.cad_usuario.trim().len() > 32 {
            Some("username: 1 to 32 characters")
        } else if !self.cad_email.contains('@') || self.cad_email.trim().len() > 254 {
            Some("invalid e-mail")
        } else if self.cad_senha.len() < 6 {
            Some("password: at least 6 characters")
        } else if self.cad_senha.len() > 128 {
            Some("password: at most 128 characters")
        } else if self.cad_senha != self.cad_senha2 {
            Some("the two passwords don't match")
        } else {
            None
        };
        if let Some(m) = motivo {
            ui::texto_centro(cx, ccriar.y - 10.0, m, 13, ui::APOIO);
        }
        let pode = motivo.is_none() && !esperando;
        let rotulo = if esperando {
            "criando…"
        } else {
            "create account"
        };
        if (ui::botao(ccriar, rotulo, pode) || (pode && self.cad_foco == 3 && enter)) && pode {
            self.campo_login_ativo = false;
            self.cad_recado = None;
            self.cad_pedido = Some(api::criar_conta(
                self.cad_usuario.trim(),
                self.cad_email.trim(),
                &self.cad_senha,
            ));
        }
        if ui::botao(cvoltar, "voltar", !esperando) {
            self.campo_login_ativo = false;
            self.cad_recado = None;
            self.tela = Tela::Login;
        }
        if let Some((m, ruim)) = self.cad_recado.clone() {
            if ruim {
                ui::erro(cx, r.y + 30.0, &m);
            } else {
                ui::texto_centro(cx, r.y + 30.0, &m, 14, ui::OURO);
            }
        }
    }

    fn tela_login(&mut self) {
        ui::fundo();
        if self.atualizacao.desenha() { return; }
        if self.novidades.desenha() { return; }
        // SESSÃO GUARDADA: entra sozinho, sem mostrar a tela. É isto que faz
        // o "remember me" valer a pena — lembrar só o nome de usuário
        // ainda deixaria a senha pra digitar no celular.
        //
        // O `host.is_some()` NÃO é zelo: `conectar` sai calado sem host, sem
        // mexer na tela, e esta função roda POR QUADRO. Sem a guarda, um
        // token guardado com o host ainda indefinido vira laço infinito com a
        // tela de login nunca aparecendo.
        //
        // Token vencido: o `LoginDenied` o apaga e devolve pra cá sem token,
        // e a tela aparece normalmente com o recado.
        if self.token_login.is_some() && self.host.is_some() && !self.google.aguardando() {
            self.conectar();
            return;
        }
        // Cresceu duas vezes: 440 -> 480 pelo "remember me", 480 -> 540
        // pelo "create account". Cada linha nova empurra o resto, e é por isso
        // que as posições viraram `layout_do_login` com teste.
        const ALTURA: f32 = 540.0;
        // Teclado da tela aberto: o painel sobe o bastante pro campo com foco
        // (a senha, no pior caso) ficar acima dele.
        let topo = (screen_height() - ALTURA) * 0.5;
        let fundo_campo = topo + 60.0 + if self.foco_senha { 164.0 } else { 88.0 };
        let aberto = nativo::TECLADO_NA_TELA && self.teclado_virtual.aberto();
        ui::subir_paineis(teclado_virtual::deslocamento(
            aberto,
            screen_height(),
            topo,
            fundo_campo,
        ));
        let r = ui::painel(460.0, ALTURA, "entrar");
        ui::subir_paineis(0.0);
        let cx = r.x + r.w * 0.5;
        if let Some(realm) = &self.realm {
            ui::texto_centro(cx, r.y + 6.0, &format!("server {realm}"), 15, ui::OURO);
        }

        if let Some(e) = self.erro_login.clone() {
            ui::erro(cx, r.y + 30.0, &e);
        }
        let [cu, cs, clembrar, centrar, cou, cgoogle, ccriar, cesq] = layout_do_login(r);
        let mut usuario = std::mem::take(&mut self.usuario);
        let mut senha = std::mem::take(&mut self.senha);
        let digitado = self.teclado.digitado().to_vec();
        let clicou_u = ui::campo(
            cu,
            "username",
            &mut usuario,
            !self.foco_senha,
            false,
            &digitado,
        );
        let clicou_s = ui::campo(cs, "senha", &mut senha, self.foco_senha, true, &digitado);
        self.usuario = usuario;
        self.senha = senha;
        if clicou_u {
            self.foco_senha = false;
            self.campo_login_ativo = true;
        }
        if clicou_s {
            self.foco_senha = true;
            self.campo_login_ativo = true;
        }
        // Toque fora dos campos fecha o teclado da tela.
        if crate::foco::clique() && !clicou_u && !clicou_s {
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
            let pode =
                !self.usuario.is_empty() && !self.senha.is_empty() && !self.google.aguardando();
            let entrar = ui::botao(centrar, "entrar", pode) || (pode && self.foco_senha && enter);
            if entrar {
                self.campo_login_ativo = false;
                self.erro_login = None;
                self.token_login = None;
                self.conectar();
            }
        }
        // LEMBRAR DE MIM. Desmarcar apaga a sessão guardada NA HORA, e não só
        // no próximo login: quem desmarca está pedindo pra esquecer agora.
        if ui::caixa(clembrar, "remember me", &mut self.lembrar) && !self.lembrar {
            self.token_login = None;
            crate::lembranca::esquece_a_sessao();
        }
        // CRIAR CONTA: o caminho de quem não quer usar o Google.
        //
        // Fica ABAIXO do Google de propósito. O `/api/register` sempre
        // existiu no servidor e nunca teve tela — quem não tinha conta não
        // tinha como entrar no jogo por conta própria.
        if ui::botao(ccriar, "create account", true) {
            self.campo_login_ativo = false;
            self.erro_login = None;
            self.cad_recado = None;
            self.tela = Tela::Cadastro;
        }
        // ESQUECI MINHA SENHA. Discreto (texto, não botão cheio): é o caminho
        // raro, e competir com "entrar" pelo olho só atrapalharia quem lembra.
        if ui::botao(cesq, "I forgot my password", true) {
            self.campo_login_ativo = false;
            self.erro_login = None;
            self.esq_recado = None;
            self.esq_email = self.usuario.clone();
            self.esq_email.clear();
            self.tela = Tela::EsqueciSenha;
        }

        // Entrar com Google: so' aparece com o servidor configurado.
        if self.google.disponivel() {
            let rg = cgoogle;
            if self.google.aguardando() {
                ui::texto_centro(
                    cx,
                    rg.y + 16.0,
                    &self.google.texto().unwrap_or_default(),
                    16,
                    ui::OURO_CLARO,
                );
                if ui::botao(
                    Rect::new(cx - 70.0, rg.y + 26.0, 140.0, 32.0),
                    "cancelar",
                    true,
                ) {
                    self.google.cancelar();
                }
            } else {
                ui::texto_centro(cx, cou.y + 6.0, "ou", 13, ui::OURO);
                if ui::botao(rg, "Sign in with Google", true) {
                    self.campo_login_ativo = false;
                    self.google.iniciar();
                }
                if let Some(e) = self.google.texto() {
                    ui::erro(cx, rg.y + rg.h + 16.0, &e);
                }
            }
        }
        if ui::botao(
            Rect::new(r.x, r.y + r.h - 40.0, 140.0, 36.0),
            "< voltar",
            true,
        ) {
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
                    self.google.falhou("I couldn't open the browser.");
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
            Tela::Login | Tela::Cadastro | Tela::EsqueciSenha => self.campo_login_ativo,
            Tela::Personagens => self.selecao_personagem.foco_no_nome(),
            Tela::Jogando => self.mercado.foco_na_busca() || self.social.foco(),
            // SEM `_`, e isso é o conserto de verdade.
            //
            // A tela de cadastro nasceu com três campos de texto e caiu no
            // `_ => false` que estava aqui: no iPhone o teclado simplesmente
            // não abria, e não havia como digitar. O dono: "não estou
            // conseguindo digitar no menu de criar conta".
            //
            // Listando as telas uma a uma, quem criar a próxima com campo de
            // texto não consegue compilar sem decidir isto aqui — o
            // compilador passa a cobrar o que o `_` engolia em silêncio.
            Tela::Servidores | Tela::Conectando | Tela::Fila { .. } | Tela::Erro(_) => false,
        };
        if let Some(mostrar) = self.teclado_virtual.quer(precisa) {
            nativo::teclado_virtual(mostrar);
        }
    }

    fn tela_personagens(&mut self) {
        let acao = self.selecao_personagem.desenha(
            &self.personagens,
            &self.armas,
            &mut self.selecionado,
            self.teclado.digitado(),
            &self.vox,
            &self.solido,
        );
        match acao {
            Some(personagens::Acao::Enviar(m)) => {
                if matches!(m, ClientMessage::SelectCharacter { .. }) {
                    self.tela = Tela::Conectando;
                }
                self.envia(m);
            }
            Some(personagens::Acao::Voltar) => self.sair(),
            None => {}
        }
    }

    fn tela_fila(&mut self, posicao: u32, total: u32) {
        ui::fundo();
        let r = ui::painel(460.0, 240.0, "entry queue");
        let cx = r.x + r.w * 0.5;
        ui::texto_centro(
            cx,
            r.y + 60.0,
            &format!("{posicao} of {total}"),
            40,
            ui::OURO_CLARO,
        );
        ui::texto_centro(cx, r.y + 96.0, "this channel is full", 17, ui::OURO);
        // A barra anda pra tras conforme a fila anda: sem numero mexendo, a
        // espera e' indistinguivel de travamento.
        let frac = if total > 0 {
            1.0 - (posicao as f32 / total as f32)
        } else {
            0.0
        };
        ui::barra(Rect::new(r.x + 40.0, r.y + 120.0, r.w - 80.0, 10.0), frac);
        if ui::botao(
            Rect::new(cx - 90.0, r.y + 155.0, 180.0, 40.0),
            "leave queue",
            true,
        ) {
            self.net = None;
            self.busca = Some(api::buscar_canais());
            self.tela = Tela::Servidores;
        }
    }
}
