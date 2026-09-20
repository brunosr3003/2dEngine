//! Protocolo de rede. Serializado em binario com postcard (ver `encode`).
//!
//! Os enums sao TAGGED POR FORA (o default do serde): a variante vira um
//! indice de 1 byte. O `#[serde(tag = "type")]` que existia aqui era pro
//! Newtonsoft.Json do cliente Unity — enum tagueado por dentro escreve o NOME
//! da variante e exige `deserialize_any` na volta, coisa que o postcard nao
//! implementa. Com o Unity fora, o nome so' custava banda.
//!
//! Toda mudanca em `ClientMessage`/`ServerMessage` DEVE bumpar
//! `PROTOCOL_VERSION` em `constants.rs`.

use crate::{EntityId, EntityMeta, EntityState, PlayerId, PROTOCOL_VERSION};
use serde::{Deserialize, Serialize};

/// Cliente -> Servidor.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ClientMessage {
    /// Primeiro pacote; servidor responde com `HandshakeAck` ou `Kick`.
    Handshake {
        protocol_version: u16,
        client_version: String,
    },
    /// Define (ou limpa) o alvo do combate por target.
    ///
    /// A partir daqui o ataque basico e' AUTOMATICO: enquanto o alvo estiver
    /// vivo e no alcance da arma, o servidor bate sozinho no ritmo do
    /// cooldown. O cliente nao manda mais botao de ataque nem mira.
    SetTarget {
        target: Option<EntityId>,
    },
    /// Login apos handshake.
    Login {
        username: String,
        password: String,
    },
    /// Envio periodico de intent do jogador.
    Input {
        input: InputFrame,
    },
    Chat {
        text: String,
    },
    Ping {
        client_time_ms: u64,
    },
    /// Toque no chao: "ande ate' aqui".
    ///
    /// O cliente manda um DESTINO, nunca um caminho — quem decide por onde
    /// da' pra passar e' quem tem o relevo. E' um destino que ele ja' poderia
    /// alcancar andando, entao nao concede nada que o joystick nao conceda.
    MoverPara {
        x: f32,
        z: f32,
    },
    UseItem {
        slot: u16,
    },
    /// Interagir. `target_eid` Some = entidade clicada específica (NPC/baú);
    /// None = pega o NPC mais próximo (tecla de interação / toggle).
    Interact {
        #[serde(default)]
        target_eid: Option<u64>,
    },
    ShopBuy {
        slot_idx: u8,
    },
    ShopSell {
        inv_slot: u16,
    },
    /// Trade atômico — todas as compras E vendas executadas juntas, ou
    /// nada. Cliente preview-only; servidor valida tudo de novo.
    ShopTrade {
        buying: Vec<TradeBuyEntry>,
        selling: Vec<TradeSellEntry>,
    },
    VaultDeposit {
        inv_slot: u16,
    },
    VaultWithdraw {
        vault_slot: u16,
    },
    VaultClose,
    InventorySwap {
        a: InvSpot,
        b: InvSpot,
    },
    /// Sort + merge stacks no inventario do player. Server agrupa stacks por
    /// item_id (respeitando stack_max), ordena ascendente, mantem itens com
    /// instance (rolls/refinamento) separados ao final.
    InventoryAutoArrange,
    /// Mesmo, pro vault aberto. Falha silenciosamente se vault nao aberto.
    VaultAutoArrange,
    /// Crafta uma receita (`id` na CRAFT_RECIPES table). Server valida inputs,
    /// consome, gera output (com ItemInstance se equipavel). Falha silenciosa
    /// se faltam materiais ou inv cheio.
    Craft {
        recipe_id: u16,
    },
    StandUp,
    TeleportToVendor,
    PartyInvite {
        target_name: String,
    },
    PartyAccept,
    PartyDecline,
    PartyLeave,
    /// Aloca 1 ponto de atributo. `stat` indice em [0=FOR,1=DES,2=INT,3=VIT,4=SPD].
    AllocStatPoint {
        stat: u8,
    },
    /// Usa uma das tres skills da arma equipada. O servidor valida o nivel
    /// do personagem, mana, recarga, estado de combate e alcance. Ataques usam
    /// a entidade selecionada por SetTarget; suporte usa o proprio personagem.
    SkillCast {
        skill_id: u32,
    },
    /// Reseta TODOS os pontos alocados pra unspent_points. Util pra testes
    /// e respec — server zera o array, devolve os pontos e reenvia stats.
    ResetStats,
    /// Refina uma peca da BOLSA. Mesmo que `Refinar { alvo: Bolsa(slot) }` —
    /// regras de `forja` e resposta `RefinoResultado`.
    RefineItem {
        slot: u16,
    },
    /// Toggle do PK Mode (player vs player opt-in). Quando ON, o player
    /// pode dar/levar dano de outros players com pk_mode ON tambem.
    /// Futuro: zonas PvP forcam ON; faccoes diferentes ignoram flag.
    TogglePkMode {
        on: bool,
    },
    /// Teletransporta o player para o spawn do mapa. Usar como escape em
    /// caso de bug de colisão (player preso em wall, fora do mapa, etc.).
    /// Permitido em qualquer estado.
    ResetPosition,
    /// Joga o item do slot do inventario no chao perto do player. Server
    /// valida slot ocupado, decrementa qty (ou zera) e spawna LootTag.
    DropItem {
        slot: u16,
    },
    RequestDisconnect,
    /// Cria novo personagem para a conta (multi-char). Aparece como nova
    /// entry na CharacterList apos sucesso. Server valida nome unico.
    CreateCharacter {
        name: String,
        visual: crate::VisualConfig,
        starting_weapon: u16,
        /// Facção escolhida na criação. Default Peacemain se cliente antigo
        /// não enviar (compat).
        #[serde(default)]
        faction: crate::Faction,
    },
    /// Seleciona um char da lista pra entrar no jogo. Server valida que
    /// o char pertence a conta autenticada, carrega o estado e envia LoginOk.
    SelectCharacter {
        name: String,
    },
    /// Pula a janela de "stand up" e respawna direto na cidade. Disponivel
    /// quando session.downed=true, sem precisar esperar o timer chegar a 0.
    /// Server teleporta pro spawn_tile, restaura HP, limpa estado downed.
    RespawnAtCity,
    /// Atualiza o visual do player mid-game (wardrobe). Server valida,
    /// salva no Session, persiste no DB, e o proximo snapshot replica
    /// pra todos os clientes — incluindo o autor (que ja aplicou local
    /// pra responsividade, mas confirma com server snapshot).
    UpdateVisual {
        visual: crate::VisualConfig,
    },
    /// Comando admin — server valida `secret` contra env var
    /// `MMORPG_ADMIN_SECRET`. Se `target_char` Some, aplica no char com
    /// esse nome (precisa estar online); senao aplica no player que enviou.
    /// Drop silencioso se secret invalido ou env nao configurada.
    AdminCommand {
        secret: String,
        #[serde(default)]
        target_char: Option<String>,
        action: AdminAction,
    },
    /// Aceita uma quest oferecida (board/npc/facção mais próximo já validou
    /// proximidade no Interact). Server valida level/facção/cooldown/slot.
    AcceptQuest {
        quest_id: u16,
    },
    /// Abandona uma quest ativa (libera o slot; repetível volta ao cooldown).
    AbandonQuest {
        quest_id: u16,
    },
    /// Entrega/conclui uma quest cujo objetivo está cumprido (status READY).
    /// Server reconfere, consome itens (Collect/Deliver) e concede recompensa.
    TurnInQuest {
        quest_id: u16,
    },
    /// Pede a lista de quests oferecidas por um giver. Usado pelo QUADRO da
    /// cidade (objeto de cena, não-entidade): source=BOARD(0), giver=0. NPCs e
    /// facção são entidades-servidor e ofertam via Interact (handle_interact).
    RequestQuestOffer {
        source: u8,
        giver: u16,
    },
    /// Compra um item na loja de facção (gasta pontos de facção). Server valida
    /// proximidade do NPC de facção da MESMA facção, pontos suficientes, e
    /// concede o item.
    FactionShopBuy {
        item_id: u16,
    },

    // ── Pesca ────────────────────────────────────────────────────────────
    /// Boia caiu na água em `pos`. Server valida vara equipada + que `pos` é
    /// água dentro do alcance, e passa a atrair peixes pra esse ponto. Quando
    /// um peixe encosta na boia, server responde `FishingBite`.
    FishingCast {
        #[serde(with = "crate::vec2_arr")]
        pos: glam::Vec2,
    },
    /// Resultado do minigame de reel-in do peixe fisgado. `success=true` →
    /// server remove o peixe do mundo e concede o item no inventário.
    /// `success=false` → peixe é solto (volta a nadar). Idempotente se não há
    /// peixe fisgado.
    FishingReel {
        success: bool,
    },
    /// Cancela a pesca atual (player se moveu/desistiu antes da fisgada). Solta
    /// o peixe fisgado se houver e limpa a boia. Idempotente.
    FishingCancel,

    // ── Tutorial ─────────────────────────────────────────────────────────
    /// Player concluiu o tutorial (clicou "concluir" ou cumpriu objetivos).
    /// Só tem efeito no processo de tutorial (TUTORIAL_MODE); o server marca
    /// `last_tutorial_completed` no DB e responde `TutorialComplete`, que manda
    /// o client reconectar no mundo aberto. Ignorado no processo do mundo.
    FinishTutorial,

    // ── Dungeon ──────────────────────────────────────────────────────────
    /// Escolhe o modo da dungeon ANTES do SelectCharacter (processo :9002).
    /// raid=true → RAID BOSS: sem waves/mobs, spawn perto da arena, só o boss.
    /// Ignorado fora de DUNGEON_MODE.
    SelectDungeonMode {
        raid: bool,
    },

    // ── Auto missao / auto coleta ────────────────────────────────────────
    /// Onde fica o objetivo desta missao ativa? Resposta: `QuestDestino`.
    QuestDestino {
        quest_id: u16,
    },
    /// Terminou o dialogo de uma missao "fale com" com este NPC. O servidor
    /// confere a distancia e marca a conversa — nao e' mais no clique.
    ConcluirConversa {
        npc_eid: u64,
    },
    /// Auto coleta: qual o melhor spot de coleta perto de mim? Resposta:
    /// `SpotDeColeta`.
    /// AUTO COLETA: o no' vivo mais perto, dos `tipos` marcados (0 madeira,
    /// 1..4 pedra pela cor), a ate' `raio` de `centro` (onde foi ligado).
    /// Resposta: `NoDeColeta`.
    PedirNoDeColeta {
        tipos: [bool; 5],
        energia: bool,
        raio: f32,
        centro: [f32; 2],
    },
    /// Coletar o no' desta coluna. O servidor valida alcance, tipo e
    /// esgotamento e responde com `ColetaEstado`.
    ColetarNo {
        coluna: u32,
    },
    /// Para a coleta em curso (auto desligado, clique em outra coisa).
    PararColeta,
    /// Auto coleta de UM tipo (0 madeira, 1..4 pedra pela cor), procurando em
    /// volta de `perto` (a regiao escolhida no mapa). Resposta: `SpotDeColeta`.
    PedirSpotDeColetaDe {
        tipo: u8,
        perto: [f32; 2],
    },
    /// Recuperar o XP perdido na morte `quando` (unix secs). Gratis ate' 3 por
    /// dia; depois cobra ouro.
    RecuperarXp {
        quando: i64,
    },
    /// Salva a barra de itens configurada (MIR4). O servidor valida, guarda no
    /// personagem e devolve `BarraDeItens`.
    SalvarBarra {
        espacos: Vec<EspacoDaBarra>,
    },
    /// Salva as preferencias de tela do personagem (skills AUTO, filtros do
    /// mapa, zooms). O servidor valida e guarda; nao responde.
    SalvarPreferencias {
        prefs: Preferencias,
    },

    // ── Forja ────────────────────────────────────────────────────────────
    /// Refina uma peca da bolsa ou equipada pelas regras de `forja`: +1..+12,
    /// seguro ate' +5, do +6 em diante falhar DESTROI. Resposta:
    /// `RefinoResultado`. Vale de qualquer lugar (menu).
    Refinar {
        alvo: AlvoDaForja,
    },

    /// Login apos handshake com a sessao emitida pelo login com Google (o
    /// `web` entrega ao cliente no `/api/auth/google/poll`). Mesma resposta
    /// do `Login`. Ver docs/LOGIN_GOOGLE.md.
    LoginToken {
        token: String,
    },
    /// Mercado global (docs/MERCADO.md): busca anuncios ativos.
    MercadoBuscar {
        filtro: crate::mercado::FiltroNet,
    },
    /// Meus anuncios ativos, historico e saldo de TP.
    MercadoMeus,
    /// Anuncia `qtd` do slot da bolsa a `preco_unit` gold cada. O item sai da
    /// bolsa na hora e fica em custodia no mercado.
    MercadoAnunciar {
        inv_slot: u16,
        qtd: u32,
        preco_unit: u64,
    },
    /// Anuncia TP da conta a `preco_unit` gold cada (TP em custodia).
    MercadoAnunciarTp {
        qtd: u64,
        preco_unit: u64,
    },
    /// Compra `qtd` de um anuncio (item ou TP) ao preco que o cliente viu.
    MercadoComprar {
        anuncio: String,
        qtd: u64,
        preco_unit: u64,
    },
    /// Cancela um anuncio proprio: o que sobrou volta por entrega.
    MercadoCancelar {
        anuncio: String,
    },
    /// Entregas esperando e saldo de TP.
    MercadoEntregas,
    /// Recebe as entregas que couberem na bolsa.
    MercadoReceber,
    /// Dungeons (docs/DUNGEONS_E_RAIDS.md): fila, sala, instancia, bau, correio.
    Dungeon {
        pedido: crate::dungeon::Pedido,
    },
    /// Calendario de presenca (docs/CALENDARIO.md): estado e resgate do dia.
    Presenca {
        pedido: crate::presenca::PedidoPresenca,
    },
    /// Loja de cash e montarias (docs/LOJA.md).
    Loja {
        pedido: crate::loja::PedidoLoja,
    },
    /// Extensao opcional, anexada para preservar os indices postcard existentes.
    Social {
        pedido: crate::social::Pedido,
    },
    // As duas abas de oficina do Craft (protocolo 107, junto com o `tier` da
    // `ItemInstance`).
    /// Aba Aprimorar: funde duas pecas IGUAIS da bolsa no tier seguinte, ou
    /// duas Tier IV +8 na cor de cima (`forja::conferir_aprimorar`).
    /// Resposta: `AprimorarResultado`.
    Aprimorar {
        slot_a: u16,
        slot_b: u16,
    },
    /// Aba Combinar: `vezes` tentativas da receita cuja entrada e' `entrada`
    /// (`combinar::receita`). Resposta: `CombinarResultado`.
    Combinar {
        entrada: u16,
        vezes: u16,
    },
    /// Loja do NPC: `qtd` do item do `slot_idx` de uma vez, em cobre. Tudo ou
    /// nada (cobre e espaco conferidos antes). Resposta: `ShopTradeResult`.
    /// Anexada no fim: o app antigo segue com o `ShopBuy` de uma unidade.
    ShopComprar {
        slot_idx: u8,
        qtd: u16,
    },
    /// O jogador fez a acao de um passo TUTORIAL da historia
    /// (`quests::tutorial::*`). O cliente so' manda com o passo ativo.
    Tutorial {
        acao: u16,
    },
    /// Menu do Capitao do Porto: embarca pra ilha `ilha` (indice do
    /// `ARQUIPELAGO`), se a historia ja' liberou e o servidor dela esta' no
    /// ar (`shared::viagem`). Sucesso = `TrocarZona`.
    Viajar {
        ilha: u8,
    },
    /// Gasta 1 Pergaminho de Teleporte e salta pro chao firme mais perto de
    /// (x, z), na ilha atual. Recusa (motivo no chat) nao gasta.
    Teleportar {
        x: f32,
        z: f32,
    },
    /// Compra +10 espacos na bolsa (`banco = false`) ou no banco, em ouro
    /// (`shared::armazem::custo`). Resposta: `Armazem` e a bolsa/banco novos.
    ExpandirArmazem {
        banco: bool,
    },
    /// Resposta ao `EscolhaNoNpc`: `missao` = ver as missoes dele; senao, a
    /// funcao dele (loja, forja, viajar, banco).
    EscolherNoNpc {
        npc_eid: u64,
        missao: bool,
    },
    /// Condensa Energia num tomo da habilidade escolhida. Determinístico.
    FabricarTomoDeSkill {
        skill_id: u32,
        grau: u8,
    },
    /// Evolui uma habilidade em um tier. O servidor cobra Energia, cobre e,
    /// nos despertares, o tomo já condensado.
    EvoluirSkill {
        skill_id: u32,
    },
    /// Abre pergaminho de invocação. `quantidade` aceita 1 ou 10; ao gastar
    /// 10 o servidor sorteia e entrega 11 prêmios (bônus 10+1).
    AbrirPergaminhos {
        slot: u16,
        quantidade: u8,
    },
}

/// Onde esta' a peca que a forja vai refinar.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum AlvoDaForja {
    Bolsa(u16),
    Equipado(crate::EquipSlot),
}

/// Acoes administrativas aplicadas via `ClientMessage::AdminCommand`.
/// Sempre afetam o player que enviou (self). Pra mexer em outro player,
/// rode comando da conta desse player.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AdminAction {
    /// Seta xp absoluto — recomputa stats (level escala HP/MP/poise unlock).
    SetXp { xp: u64 },
    /// Seta gold absoluto.
    SetGold { gold: i64 },
    /// Adiciona item ao inventario (qty stackavel).
    GiveItem { item_id: u16, qty: u16 },
    /// Esvazia inventario completamente.
    ClearInventory,
    /// HP/MP/Stamina/Poise full.
    HealFull,
    /// Concede skill points (incrementa sp_earned).
    GrantSp { amount: u32 },
    /// Concede stat points (incrementa unspent_points).
    GrantStatPoints { amount: u32 },
    /// Seta level: xp = sum(1..lvl-1) * mult, unspent = 3*(lvl-1), sp = lvl-1.
    /// Equivalente a fazer SetXp + ajuste de pontos atomicamente.
    SetLevel { level: u32 },
    /// Boss temporario perto destas coordenadas, para testar combate.
    SpawnTestBoss { x: f32, z: f32, hp: i32 },
}

#[cfg(test)]
mod admin_tests {
    use super::*;

    #[test]
    fn admin_command_roundtrip_postcard() {
        for action in [
            AdminAction::HealFull,
            AdminAction::SetLevel { level: 10 },
            AdminAction::SpawnTestBoss {
                x: 74.5,
                z: -138.0,
                hp: 50_000,
            },
        ] {
            let msg = ClientMessage::AdminCommand {
                secret: "test-only".into(),
                target_char: None,
                action,
            };
            let bytes = encode(&msg).unwrap();
            let decoded: ClientMessage = decode(&bytes).unwrap();
            assert_eq!(encode(&decoded).unwrap(), bytes);
        }
    }
}

/// Localizacao logica de um slot no sistema de inventario do cliente.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum InvSpot {
    Inv(u16),
    Equip(crate::constants::EquipSlot),
}

/// Input de um tick do cliente.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct InputFrame {
    pub seq: u32,
    pub tick: u32,
    #[serde(with = "crate::vec2_arr")]
    pub move_dir: glam::Vec2,
    #[serde(with = "crate::vec2_arr")]
    pub aim: glam::Vec2,
    pub buttons: u32,
}

pub mod buttons {
    pub const PRIMARY: u32 = 1 << 0;
    pub const SECONDARY: u32 = 1 << 1;
    pub const INTERACT: u32 = 1 << 2;
    pub const DASH: u32 = 1 << 3;
    /// Shift held = sprint (multiplica speed por SPRINT_SPEED_MULT enquanto
    /// drena stamina). Ignorado se stamina<=0 ou defendendo.
    pub const SPRINT: u32 = 1 << 4;
    /// Pulo. Vale por SUBIDA, nao por altura: o que ele muda e' o degrau
    /// maximo que o corpo aceita — de um bloco pra dois. O arco vertical e'
    /// so' o cliente contando o que o servidor ja' decidiu.
    pub const PULO: u32 = 1 << 5;
}

/// Servidor -> Cliente.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ServerMessage {
    HandshakeAck {
        protocol_version: u16,
        server_time_ms: u64,
        /// Multiplier da curva de XP (configuravel via DB pra eventos). Cliente
        /// usa pra calcular display da barra de XP — DEVE bater com server.
        /// Default `DEFAULT_XP_MULTIPLIER` (500); evento 2x = 250 (mais facil).
        #[serde(default = "default_xp_mult")]
        xp_multiplier: u64,
    },
    LoginOk {
        player_id: PlayerId,
        entity_id: EntityId,
        spawn: [f32; 2],
    },
    LoginDenied {
        reason: String,
    },
    /// Onde o jogador esta, e quao cheio. Vai no login e a cada poucos
    /// segundos.
    ///
    /// O HUD precisa disso porque, num jogo com servidor/canal/zona, "onde eu
    /// estou" deixa de ser obvio: o mesmo personagem aparece em lugares
    /// diferentes conforme o canal, e lotacao muda o que da' pra fazer ali.
    InfoCanal {
        realm: String,
        canal: String,
        zona: String,
        jogadores: u32,
        capacidade: u32,
    },
    /// Portal pra outra ZONA: reconecte neste host.
    ///
    /// Zona (cidade, campo, dungeon) roda em processo proprio, entao mudar de
    /// zona nao e' teleporte — e' trocar de servidor. O personagem e' o mesmo
    /// (banco compartilhado no realm), so' a conexao muda.
    TrocarZona {
        zona: String,
        host: String,
    },
    /// Canal cheio: o jogador esta na FILA, nao recusado.
    ///
    /// Area de canal unico (cidade, arena, boss de mundo) nao pode simplesmente
    /// abrir outra instancia — a graca dela e' todo mundo estar no mesmo lugar.
    /// Quando lota, a saida e' esperar, e o jogador precisa ver a posicao pra
    /// decidir se espera ou faz outra coisa.
    FilaDeEntrada {
        posicao: u32,
        total: u32,
    },
    /// Lista de chars da conta autenticada — enviada apos Login bem-sucedido
    /// e apos cada CreateCharacter ou SelectCharacter. Cliente exibe a tela
    /// de selecao; pode estar vazia (conta nova) ou ter ate N chars.
    /// `available_weapons` = item_ids que o cliente deve mostrar como opcoes
    /// na criacao de char (filtrado por items.active=TRUE).
    CharacterList {
        chars: Vec<CharacterListEntry>,
        available_weapons: Vec<u16>,
    },
    /// Resposta a `CreateCharacter` quando criacao falha (nome duplicado,
    /// invalido, etc). Cliente mostra erro e reabre dialog.
    CharacterCreationFailed {
        reason: String,
    },
    #[serde(rename = "WorldSnapshot")]
    Snapshot {
        snapshot: WorldSnapshot,
    },
    Chat {
        from: String,
        text: String,
    },
    Pong {
        client_time_ms: u64,
        server_time_ms: u64,
    },
    MapChange {
        map_name: String,
        width: u32,
        height: u32,
        tiles: Vec<u16>,
        spawn: [f32; 2],
        safe_zone: bool,
        #[serde(default)]
        decorations: Vec<crate::world_gen::DecoPlacement>,
        /// Retângulos de zona segura do mapa (origem inferior-esquerda + tamanho).
        /// Cliente renderiza tint/borda; gameplay (sem dano etc) é decidido
        /// pelo servidor mas a UI ajuda o jogador a saber onde tá.
        #[serde(default)]
        safe_zones: Vec<SafeZoneRect>,
    },
    ProgressUpdate {
        xp: u64,
        level: u32,
    },
    InventoryUpdate {
        slots: Vec<crate::InventorySlot>,
    },
    /// Snapshot dos items configurados no servidor — enviado no login e
    /// re-enviado quando admin altera algo (hot-reload da economy).
    /// Cliente usa pra sobrescrever nomes/icones hardcoded em ItemInfo.
    ///
    /// Campo `item_configs` em vez de `items` pra não colidir com `items` do
    /// `ShopOpen` (mesma struct compartilhada no cliente C#).
    ItemsConfig {
        #[serde(rename = "item_configs")]
        items: Vec<ItemConfigEntry>,
    },
    StatsUpdate {
        stats: crate::PlayerStats,
        equipment: crate::Equipment,
    },
    ManaUpdate {
        current: i32,
    },
    StaminaUpdate {
        current: i32,
    },
    /// Poise atual do player (0..stats.poise_max). Server envia quando o
    /// inteiro muda (poise inteiro, nao fracionado). Cliente atualiza barra.
    PoiseUpdate {
        current: i32,
    },
    ShopOpen {
        items: Vec<ShopItem>,
        sell_prices: Vec<SellPrice>,
        /// Identificador opaco do vendor (NPC). Usado pelo cliente como
        /// chave pra cache visual (nome, retrato, reputação no futuro).
        vendor_id: u32,
        /// Multiplicador aplicado em compras (preço final = preço * mult).
        /// Default 1.0 — futuro: depende da relação com vendor.
        buy_mult: f32,
        /// Multiplicador aplicado em vendas. Default 1.0.
        sell_mult: f32,
    },
    ShopClose,
    ShopTradeResult {
        ok: bool,
        reason: String,
    },
    VaultOpen {
        slots: Vec<crate::InventorySlot>,
    },
    VaultUpdate {
        slots: Vec<crate::InventorySlot>,
    },
    VaultClose,
    /// Sinaliza ao cliente abrir o painel do ferreiro (refinar + socket gem).
    /// Sem payload — cliente apenas mostra a UI.
    BlacksmithOpen,
    BlacksmithClose,
    DownedUpdate {
        active: bool,
        dhp: i32,
        dhp_max: i32,
        timer_s: f32,
    },
    FameUpdate {
        fame: u64,
    },
    AuraUpdate {
        aura: u64,
    },
    /// Currency separado do inventário. Enviado no login e após cada
    /// transação que muda gold (loot, shop buy/sell, refining, trade).
    GoldUpdate {
        gold: u64,
    },

    // ── Farm Nodes ──────────────────────────────────────────────────────────
    /// Enviado após login com a lista completa de farm nodes do mapa. Cliente
    /// usa para associar IDs aos GameObjects locais por posição.
    FarmNodesConfig {
        nodes: Vec<FarmNodeInfo>,
    },
    /// Node coletado — desaparece até respawn. Broadcast pra players na AOI.
    FarmNodeDepleted {
        node_id: u32,
    },
    /// Pedras de minerio ESGOTADAS no momento do login, por chave de coluna.
    ///
    /// A pedra em si nunca viaja: os dois lados a geram da mesma semente. O
    /// que viaja e' so' a excecao — quais sumiram —, e ela e' curta porque
    /// pedra esgotada e' minoria por construcao.
    PedrasEsgotadas {
        colunas: Vec<u32>,
    },
    /// Uma pedra acabou de esgotar (ou de voltar). Broadcast na AOI.
    PedraEsgotada {
        coluna: u32,
    },
    PedraVoltou {
        coluna: u32,
    },
    /// Node respawnado — pode ser coletado novamente.
    FarmNodeRespawned {
        node_id: u32,
    },
    /// XP de proficiencia por CONJUNTO de arma. Coleta e artesanato nao tem.
    ProficienciesUpdate {
        #[serde(rename = "proficiency_xp")]
        xp: [u64; crate::PROF_COUNT],
    },
    /// Catálogo de skills carregado do DB. Enviado uma vez no login + após
    /// hot-reload (admin bumpou economy_version). Cliente cacheia em
    /// `SkillsConfigCache` pra UI consultar nome/icon/descrição.
    SkillsConfig {
        skills: Vec<crate::skills::Skill>,
    },
    /// Catalogo de receitas de crafting carregado do DB. Enviado no login,
    /// substitui o hardcoded client-side. Admin pode mudar custos/inputs/
    /// outputs via DB — ideal pra eventos com receitas especiais.
    CraftRecipes {
        recipes: Vec<CraftRecipeNet>,
    },
    /// "Onde obter": pra cada item, de onde ele sai (`FonteDeItem`). Enviado
    /// no login + apos hot-reload da economy. Campo nomeado `resource_sources` (nao `items`)
    /// pra evitar colisao no deserializer compartilhado do cliente.
    ResourceSources {
        #[serde(rename = "resource_sources")]
        items: Vec<ItemResourceSources>,
    },
    /// Inicio confirmado de uma skill: inicia o gesto e a antecipacao visual.
    SkillCastFx {
        skill_id: u32,
        #[serde(with = "crate::vec2_arr")]
        caster_pos: glam::Vec2,
        #[serde(with = "crate::vec2_arr")]
        target_pos: glam::Vec2,
        /// EntityId do alvo principal pra skills line/single (Lightning Bolt etc).
        /// None pra AoE/self/projectile (cliente desenha sem snap em alvo).
        target_eid: Option<crate::EntityId>,
        /// EntityId do caster — usado pelo cliente pra cancelar coroutines
        /// quando o cast é interrompido (SkillCastCancel mata visuals deste eid).
        caster_eid: Option<crate::EntityId>,
        /// Posicoes encadeadas dos bounces (Chain Lightning, Lightning Bolt
        /// rank 5+). Comeca no target_pos principal e segue por cada alvo
        /// adicional. None se a skill nao tem chain.
        #[serde(default)]
        chain_points: Option<Vec<[f32; 2]>>,
    },
    /// Cast foi cancelado (player se moveu durante o cast). Cliente despawna
    /// gizmos/VFX em andamento associados ao caster_eid. Tambem para qualquer
    /// pose de Thrust travada — snap.casting tambem fica false no proximo tick.
    /// Server tambem refunda mp/stamina e remove o cooldown — cliente reseta
    /// local cd timer pra refletir.
    SkillCastCancel {
        caster_eid: crate::EntityId,
        skill_id: u32,
    },
    /// Projetil atingiu um alvo. Cliente usa pra spawnar VFX de impacto
    /// "atachado" ao alvo (ex: flecha presa no inimigo + splatter de
    /// sangue rotacionado pela direção do projetil).
    ProjectileImpact {
        /// Entity_id do alvo atingido. Cliente lookup por NetId no
        /// dicionario de entidades pra parentar o visual.
        target_eid: crate::EntityId,
        /// Direção da flecha (unitario, vel.normalize()). Cliente rotaciona
        /// o stuck arrow + splatter pra alinhar.
        #[serde(with = "crate::vec2_arr")]
        dir: glam::Vec2,
        /// proj_kind (0=arrow, 1=fireball, etc). So 0 (arrow) atualmente
        /// dispara o visual stuck — outros kinds tem seu proprio impacto.
        kind: u8,
    },
    /// Buff foi aplicado a uma entidade — visual feedback. Server emite
    /// quando: heal recebido (de aliado), buff applied (Bloodthirst, Group
    /// Heal aura, etc). Cliente spawna music_burst ou sparkle acima do alvo.
    /// `kind`:
    ///   0 = heal (aliado curou voce)
    ///   1 = damage_buff (Bloodthirst, Attack Up)
    ///   2 = defense_buff (Defense Up, Phalanx)
    ///   3 = haste (atk speed buff)
    BuffApplied {
        target_eid: crate::EntityId,
        kind: u8,
    },
    PartyInviteReceived {
        from: String,
    },
    PartyUpdate {
        members: Vec<String>,
    },
    /// Pontos de atributo disponiveis + ja alocados em cada stat.
    /// `allocated[i]` = pontos no stat com indice `i` (0=FOR..4=SPD, ver `crate::stat_idx`).
    StatPointsUpdate {
        unspent: u32,
        allocated: [u32; crate::STAT_COUNT],
    },
    Kick {
        reason: String,
    },

    /// Resposta ao Interact com um quest giver (quadro/NPC/facção): lista de
    /// quests DISPONÍVEIS pra aceitar dali (já filtradas por level/facção/cooldown).
    QuestOffer {
        giver_source: u8, // quests::quest_source
        giver_id: u16,    // shop_id do NPC, 0 (board) ou faction_id
        #[serde(default)]
        giver_name: String, // nome do NPC pro cabeçalho do diálogo ("" = board)
        quests: Vec<crate::quests::QuestNet>,
    },
    /// Log completo das quests ativas do player (login + ressincronização).
    /// Campo `quests` (não `active`) pra reusar o mesmo slot JSON do QuestOffer
    /// no cliente — o `type` desambigua.
    QuestLog {
        quests: Vec<crate::quests::QuestNet>,
    },
    /// Atualização incremental de uma quest (progresso/status mudou).
    QuestUpdate {
        quest_id: u16,
        progress: u32,
        status: u8, // quests::quest_status
    },
    /// Pontos de facção atuais do player (atualiza HUD).
    FactionPoints {
        points: u32,
    },
    /// Givers (ids) que têm AO MENOS uma quest aceitável agora pra este player.
    /// Cliente usa pra mostrar o "!" só sobre quem realmente tem missão.
    QuestGivers {
        available: Vec<u16>,
    },
    /// Loja de facção (enviada ao interagir com o NPC de facção da própria
    /// facção). Itens custam PONTOS DE FACÇÃO, não ouro.
    FactionShopOpen {
        faction: u8,
        points: u32,
        // Nome distinto de ShopOpen.items (tipo diferente, mesma key colidiria no client).
        faction_items: Vec<FactionShopItemNet>,
    },

    /// Um peixe encostou na boia do player — fisgou. Cliente inicia o minigame
    /// de reel-in com o `species` informado (1-4, mapeia pro catálogo de
    /// peixes). Ao terminar, cliente manda `FishingReel{success}`.
    FishingBite {
        /// EntityId do peixe fisgado (cliente pode usar pra esconder/realçar).
        fish_eid: EntityId,
        /// Espécie do peixe (1=Anchova..4=Baiacu) — define dificuldade/sprite.
        species: u16,
    },

    // ── Tutorial ─────────────────────────────────────────────────────────
    /// Tutorial concluído (resposta ao `FinishTutorial`). O client deve
    /// desconectar e reconectar no MUNDO ABERTO. `world_host`/`world_path`
    /// vazios → client usa o default de produção (wss .../game).
    TutorialComplete {
        #[serde(default)]
        world_host: String,
        #[serde(default)]
        world_path: String,
    },
    /// Manda o client ir pro TUTORIAL (emitido pelo mundo aberto quando o
    /// player interage com o NPC de re-treino). `host`/`path` vazios → client
    /// usa o default de tutorial (wss .../tutorial).
    GoToTutorial {
        #[serde(default)]
        host: String,
        #[serde(default)]
        path: String,
    },
    /// Fala do NPC guia do tutorial (Matteo) — o client mostra um balão de
    /// diálogo. Emitido ao falar com ele (dá machado / recebe madeira / etc).
    TutorialSay {
        #[serde(default)]
        speaker: String,
        #[serde(default)]
        text: String,
    },

    // ── Dungeon ──────────────────────────────────────────────────────────
    /// Uma sala da dungeon foi limpa (todos os mobs mortos) → o gate pra
    /// próxima sala abriu (server flipou os tiles WALL→DUNGEON_FLOOR). O client
    /// atualiza o tracker e abre o gate visual. `room_idx` = índice da sala
    /// recém-limpa; o player avança pra `room_idx + 1`.
    DungeonRoomCleared {
        room_idx: u32,
        total_rooms: u32,
    },
    /// Dungeon concluída (boss morto). Igual ao `TutorialComplete`: o client
    /// reconecta no MUNDO ABERTO. host/path vazios → default de produção
    /// (/game). O loot/XP ganho na dungeon JÁ está persistido (diferente do
    /// tutorial, que descarta).
    DungeonComplete {
        #[serde(default)]
        world_host: String,
        #[serde(default)]
        world_path: String,
    },
    /// Recargas autoritativas em segundos restantes e trava da animacao.
    SkillsState {
        cooldowns: Vec<(u32, f32)>,
        busy_s: f32,
    },
    SkillRejected {
        skill_id: u32,
        motivo: String,
    },
    /// Momento em que a skill realmente produz seu efeito.
    SkillImpactFx {
        skill_id: u32,
        caster_eid: EntityId,
        #[serde(with = "crate::vec2_arr")]
        caster_pos: glam::Vec2,
        #[serde(with = "crate::vec2_arr")]
        target_pos: glam::Vec2,
    },
    /// Alvo e tempo do ataque de um mob; locomocao nao determina sua mira.
    MobAttackFx {
        attacker: EntityId,
        target: Option<EntityId>,
        dir: [f32; 2],
        impact_s: f32,
    },
    /// Onde fica o objetivo de uma missao (`quests::destino_tipo`). `npc_eid`
    /// quando e' pra falar ou entregar.
    QuestDestino {
        quest_id: u16,
        tipo: u8,
        pos: [f32; 2],
        raio: f32,
        npc_eid: Option<u64>,
    },
    /// O no' que o AUTO COLETA (ou o "Ir" do mapa) deve coletar: (coluna, onde
    /// ficar pra alcancar, centro do corpo, tipo). `None` = nada vivo no raio.
    NoDeColeta {
        no: Option<(u32, [f32; 2], [f32; 2], u8)>,
    },
    /// Coleta do proprio jogador: `tipo` 0 madeira / 1..4 pedra, ou
    /// `COLETA_PARADA`; intervalo do ciclo e progresso ja' andado (0..1) no
    /// instante do envio. Vai ao comecar, a cada ciclo e ao parar.
    /// `pausado`: a bolsa nao comporta o proximo ciclo — o no' continua
    /// escolhido, nada foi gasto, e a coleta volta sozinha quando abrir
    /// espaco.
    ColetaEstado {
        tipo: u8,
        intervalo_s: f32,
        progresso: f32,
        centro: Option<[f32; 2]>,
        pausado: bool,
    },
    /// Recarga e cura restantes (s) de um grupo de pocao
    /// (`pocoes::Grupo`). Vai ao beber e ao recusar.
    PocaoGrupo {
        grupo: u8,
        recarga_s: f32,
        cura_s: f32,
    },
    /// A rota que o servidor calculou pro proprio jogador, pro tracejado no
    /// chao. Vai quando a rota nasce ou e' refeita; `pontos` vazio = acabou
    /// (chegou, comando manual, limpa). O cliente descarta sozinho os pontos
    /// ja' alcancados.
    Rota {
        pontos: Vec<[f32; 2]>,
        destino: [f32; 2],
    },
    /// O que o mapa mostra da ilha: zonas de mob (com os bichos e a chance de
    /// cada um) e regioes de recurso. Vai uma vez, logo depois do `MapChange`.
    /// `nomes` = (kind, nome) dos bichos; `rendimentos` = (tipo, o que rende).
    MapaDaIlha {
        zonas: Vec<ZonaNoMapa>,
        recursos: Vec<RegiaoNoMapa>,
        nomes: Vec<(u16, String)>,
        rendimentos: Vec<(u8, String)>,
        /// Chefes de campo da ilha (onde moram, nome, nivel, se estao vivos).
        #[serde(default)]
        chefes: Vec<crate::bosses::ChefeNoMapa>,
    },
    /// Estado das missoes que o `QuestLog` nao carrega: as ja' entregues (com o
    /// fim do cooldown, 0 = sem) e a faccao do personagem (`quests::faction_id`).
    /// Pro menu de todas as missoes calcular bloqueio sem perguntar.
    QuestEstado {
        entregues: Vec<(u16, i64)>,
        faccao: u8,
    },
    /// Resultado de um `Craft`: criou `item_id`, ou o motivo da recusa.
    CraftResultado {
        recipe_id: u16,
        ok: bool,
        motivo: String,
        item_id: u16,
    },
    /// Resultado de `Refinar` (`forja::resultado`), com o nivel da peca agora
    /// (0 se destruida) e o motivo quando nem tentou.
    RefinoResultado {
        resultado: u8,
        nivel: u8,
        item_id: u16,
        motivo: String,
    },
    /// Bonus de XP da Pocao de Experiencia ativo ate' `ate` (unix secs; 0 =
    /// nenhum). Vai no login e ao beber.
    BuffXp {
        ate: i64,
    },
    /// Pocoes de Fortuna e de Sorte ativas ate' (unix secs; 0 = nenhuma). Vai
    /// no login e ao beber.
    BuffsDeDrop {
        fortuna_ate: i64,
        sorte_ate: i64,
    },
    /// A barra de itens do personagem (login e depois de salvar). Vazia = o
    /// cliente usa a padrao.
    BarraDeItens {
        espacos: Vec<EspacoDaBarra>,
    },
    /// Preferencias de tela salvas do personagem. Vai no login; o cliente so'
    /// comeca a salvar as dele depois de receber esta.
    Preferencias {
        prefs: Preferencias,
    },
    /// Voce morreu: XP perdido nesta morte (recuperavel por 24 h).
    Morte {
        xp_perdido: u64,
    },
    /// Mortes que ainda da' pra recuperar e as recuperacoes gratis de hoje.
    /// Vai no login, ao morrer e depois de recuperar.
    Recuperaveis {
        mortes: Vec<MorteRecuperavelNet>,
        gratis_restantes: u8,
    },
    /// Resultado de `RecuperarXp`: XP devolvido ou o motivo da recusa.
    RecuperarXpResultado {
        ok: bool,
        motivo: String,
        xp: u64,
    },
    /// Chefe carregando um golpe: a forma no chao (`centro`, virada pra `dir`)
    /// e quanto falta pro impacto, contado de quando esta mensagem chega.
    /// Quem estiver dentro no impacto toma — decidido pelo servidor.
    Telegrafico {
        id: u32,
        chefe: EntityId,
        forma: crate::bosses::Forma,
        centro: [f32; 2],
        dir: [f32; 2],
        carga_s: f32,
    },
    /// O golpe saiu (`impacto`) ou foi cancelado (chefe morreu).
    TelegraficoFim {
        id: u32,
        impacto: bool,
    },
    /// Mercado: uma pagina da busca.
    MercadoLista {
        anuncios: Vec<crate::mercado::AnuncioNet>,
        pagina: u16,
        tem_mais: bool,
    },
    /// Mercado: meus anuncios ativos, historico e TP da conta.
    MercadoMeus {
        anuncios: Vec<crate::mercado::AnuncioNet>,
        historico: Vec<crate::mercado::VendaNet>,
        tp: u64,
    },
    /// Mercado: entregas esperando e TP da conta.
    MercadoEntregas {
        cartas: Vec<crate::mercado::CartaNet>,
        tp: u64,
    },
    /// Mercado: resposta de um pedido (ou aviso de venda/compra fechada).
    MercadoResultado {
        ok: bool,
        texto: String,
    },
    /// Dungeons: estado da janela, pronto-check, instancia, bau e correio.
    Dungeon {
        aviso: crate::dungeon::Aviso,
    },
    /// Calendario de presenca: grade, progresso da conta e resultado do resgate.
    Presenca {
        aviso: crate::presenca::AvisoPresenca,
    },
    /// Loja de cash e montarias: estado, resultado de compra, montando.
    Loja {
        aviso: crate::loja::AvisoLoja,
    },
    /// O alvo esta' no alcance da arma a distancia, mas o relevo barra o
    /// tiro: o ataque nao sai. No maximo 1 por segundo por jogador.
    SemVisada {
        alvo: EntityId,
    },
    Social {
        aviso: crate::social::Aviso,
    },
    /// Resposta do `Aprimorar`. `ok` = a peca nova (`item_id`, `grau` = cor,
    /// `tier`) esta' na bolsa; senao `texto` diz o motivo.
    AprimorarResultado {
        ok: bool,
        texto: String,
        item_id: u16,
        grau: u8,
        tier: u8,
    },
    /// Resposta do `Combinar`: quantas tentativas foram pagas e quantas deram
    /// certo. `tentativas` 0 = recusado (motivo em `texto`).
    CombinarResultado {
        entrada: u16,
        tentativas: u16,
        sucessos: u16,
        texto: String,
    },
    /// Clique no Capitao do Porto: o menu "Viajar", uma linha por ilha.
    Viagem {
        destinos: Vec<crate::viagem::Destino>,
    },
    /// Expansoes compradas da bolsa e do banco (`shared::armazem`). Vai no
    /// login, ao abrir o banco e a cada expansao.
    Armazem {
        bolsa_extra: u8,
        banco_extra: u8,
    },
    /// Toque num NPC que tem missao E uma funcao: o jogador escolhe o que
    /// quer (antes a missao vinha na frente e a funcao abria atras dela).
    /// `funcao` e' o rotulo do botao: "Loja", "Forja", "Viajar", "Banco".
    EscolhaNoNpc {
        npc_eid: u64,
        nome: String,
        funcao: String,
    },
    /// Saldo, tiers e tomos das doze habilidades. Vai no login e após toda
    /// coleta/fabricação/evolução que alterar o estado.
    ProgressoDeSkills {
        progresso: crate::skills::ProgressoDeSkills,
    },
    ResultadoDeEvolucao {
        ok: bool,
        texto: String,
    },
    /// Enviado somente quando o servidor aceita o Dash; inclui redução por SPD.
    DashRecarga { segundos: f32 },
}

/// Quantos espacos a barra de itens tem: C, 8, 9 e 0.
pub const ESPACOS_DA_BARRA: usize = 4;

/// `ColetaEstado.tipo` quando nao ha' coleta.
pub const COLETA_PARADA: u8 = 255;

/// Teto do JSON de preferencias guardado no personagem.
pub const PREFERENCIAS_MAX_BYTES: usize = 8 * 1024;

/// Preferencias de tela do personagem, guardadas no servidor
/// (`characters.preferencias_json`): o que era so' memoria do cliente e sumia
/// no relog. Tudo com `serde(default)`: JSON antigo, vazio ou com campo a mais
/// continua valendo; campo `None` = "nunca escolheu", o cliente mantem o dele.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(default)]
pub struct Preferencias {
    pub versao: u8,
    /// Skills marcadas pra uso automatico (ids de `skills::playtest`).
    pub skills_auto: Vec<u32>,
    pub filtros_mapa: FiltrosDoMapa,
    /// Raio visivel do minimapa, em unidades.
    pub alcance_minimapa: Option<f32>,
    pub camera_zoom: Option<f32>,
    pub camera_pitch_ajuste: Option<f32>,
    /// AUTO COLETA: tipos marcados (0 madeira, 1..4 pedra pela cor).
    pub coleta_tipos: Option<[bool; 5]>,
    /// Cristais de Energia no AUTO COLETA.
    pub coleta_energia: Option<bool>,
    /// AUTO COLETA: raio de busca a partir de onde foi ligado.
    pub coleta_raio: Option<f32>,
    /// AUTO COLETA: apanhou de bicho, mata e volta a coletar.
    pub coleta_defender: Option<bool>,
    /// Escala da interface (HUD e textos), 0,8 a 1,6.
    pub escala_ui: Option<f32>,
    /// Modo economia de energia: entra sozinho depois de N minutos sem tocar
    /// na tela (0 = nunca).
    pub economia_auto_min: Option<u16>,
    /// Skin de montaria escolhida (`loja::SKINS`). O servidor so' aceita a
    /// que a conta possui (`Posses::skin_para_montar`).
    pub montaria_skin: Option<u16>,
    /// Minimapa grande: ve' mais mundo de uma vez. O tamanho de verdade quem
    /// decide e' o `hud_layout`, pelo espaco que sobra na tela.
    pub minimapa_expandido: Option<bool>,
    /// Minimapa fechado: so' o botao de reabrir fica na tela.
    pub minimapa_oculto: Option<bool>,
}

/// Filtros do mapa grande e do minimapa. O padrao e' tudo desligado.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(default)]
pub struct FiltrosDoMapa {
    pub mobs: bool,
    pub bichos_ocultos: Vec<u16>,
    /// 0 madeira, 1..4 pedra pela cor.
    pub recursos: [bool; 5],
    pub energia: bool,
    pub vila: bool,
}

impl Preferencias {
    pub const VERSAO: u8 = 2;
    /// Listas nao crescem sem fim: nem 12 skills nem 8 bichos chegam perto.
    const MAX_LISTA: usize = 64;

    /// Limpa o que veio de fora: skill que nao existe sai, listas sem
    /// repeticao e com teto, numero fora da faixa (ou NaN) recortado ou
    /// esquecido.
    pub fn validada(mut self, skill_existe: &dyn Fn(u32) -> bool) -> Self {
        fn faixa(v: Option<f32>, min: f32, max: f32) -> Option<f32> {
            v.filter(|x| x.is_finite()).map(|x| x.clamp(min, max))
        }
        self.versao = Self::VERSAO;
        self.skills_auto.retain(|id| skill_existe(*id));
        self.skills_auto.sort_unstable();
        self.skills_auto.dedup();
        self.skills_auto.truncate(Self::MAX_LISTA);
        self.filtros_mapa.bichos_ocultos.sort_unstable();
        self.filtros_mapa.bichos_ocultos.dedup();
        self.filtros_mapa.bichos_ocultos.truncate(Self::MAX_LISTA);
        self.alcance_minimapa = faixa(self.alcance_minimapa, 10.0, 1000.0);
        self.camera_zoom = faixa(self.camera_zoom, 0.1, 10.0);
        self.camera_pitch_ajuste = faixa(self.camera_pitch_ajuste, -3.0, 3.0);
        self.coleta_raio = faixa(
            self.coleta_raio,
            crate::COLETA_RAIO_AUTO_MIN,
            crate::COLETA_RAIO_AUTO_MAX,
        );
        self.escala_ui = faixa(self.escala_ui, 0.8, 1.6);
        self.economia_auto_min = self.economia_auto_min.map(|m| m.min(60));
        self
    }
}

#[cfg(test)]
mod testes_preferencias {
    use super::*;

    #[test]
    fn validada_tira_skill_inexistente_repeticao_e_numero_invalido() {
        let p = Preferencias {
            versao: 0,
            skills_auto: vec![5, 999, 1, 5],
            filtros_mapa: FiltrosDoMapa {
                mobs: true,
                bichos_ocultos: vec![3, 3, 1],
                recursos: [true; 5],
                energia: true,
                vila: true,
            },
            alcance_minimapa: Some(5000.0),
            camera_zoom: Some(f32::NAN),
            camera_pitch_ajuste: Some(-9.0),
            coleta_tipos: Some([true, false, true, false, true]),
            coleta_energia: Some(true),
            coleta_raio: Some(5000.0),
            escala_ui: Some(9.0),
            economia_auto_min: Some(500),
            montaria_skin: Some(102),
            minimapa_expandido: Some(true),
            minimapa_oculto: Some(true),
            coleta_defender: Some(false),
        }
        .validada(&|id| id <= 12);
        assert_eq!(p.escala_ui, Some(1.6));
        assert_eq!(p.economia_auto_min, Some(60));
        assert_eq!(p.coleta_raio, Some(crate::COLETA_RAIO_AUTO_MAX));
        assert_eq!(p.coleta_tipos, Some([true, false, true, false, true]));
        assert_eq!(p.coleta_energia, Some(true));
        assert_eq!(p.versao, Preferencias::VERSAO);
        assert_eq!(p.skills_auto, vec![1, 5]);
        assert_eq!(p.filtros_mapa.bichos_ocultos, vec![1, 3]);
        assert_eq!(p.alcance_minimapa, Some(1000.0));
        assert_eq!(p.camera_zoom, None, "NaN e' esquecido, nao vira zero");
        assert_eq!(p.camera_pitch_ajuste, Some(-3.0));
    }

    #[test]
    fn postcard_ida_e_volta() {
        let p = Preferencias {
            skills_auto: vec![2, 7],
            camera_zoom: Some(1.4),
            ..Default::default()
        };
        let bytes = postcard::to_allocvec(&p).unwrap();
        assert_eq!(postcard::from_bytes::<Preferencias>(&bytes).unwrap(), p);
    }
}

/// Um espaco da barra de itens: o consumivel (0 = vazio), se usa sozinho
/// (AUTO) e o limiar do AUTO em % (vida, mana e vigor; buff ignora).
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct EspacoDaBarra {
    pub item_id: u16,
    pub auto: bool,
    pub limiar: u8,
}

/// Uma morte recuperavel. `custo_gold` e' o preco se as gratis acabaram.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub struct MorteRecuperavelNet {
    pub quando: i64,
    pub xp: u64,
    pub expira: i64,
    pub custo_gold: u64,
}

/// Uma zona de spawn no mapa. `bichos` = (kind, chance em %), da maior chance
/// pra menor — a mesma conta de `quests::chance_do_kind` que a auto missao usa.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ZonaNoMapa {
    pub centro: [f32; 2],
    pub raio: f32,
    pub lv_min: u16,
    pub lv_max: u16,
    pub bichos: Vec<(u16, u8)>,
    /// FORTE: mesma escada de nivel, o dobro de inimigos num raio menor. O
    /// mapa marca com icone proprio — e' informacao de rota, nao enfeite.
    #[serde(default)]
    pub forte: bool,
}

/// Regiao de recurso: corpos coletaveis de UM tipo agrupados. `tipo` 0 =
/// madeira, 1..4 = pedra pela cor (cinza, verde, azul, roxa).
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub struct RegiaoNoMapa {
    pub centro: [f32; 2],
    pub raio: f32,
    pub tipo: u8,
    pub contagem: u16,
}

/// Item da loja de facção (item_id + custo em pontos de facção).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FactionShopItemNet {
    pub item_id: u16,
    pub points: u32,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct ShopItem {
    pub item_id: u16,
    pub price: u32,
}

/// Preço de venda de um item arbitrário (todos os itens vendáveis vêm na
/// abertura do shop pra UI mostrar custos antes do clique).
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct SellPrice {
    pub item_id: u16,
    pub price: u32,
}

/// Entrada do basket de compras dentro de um ShopTrade.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct TradeBuyEntry {
    pub shop_slot: u8,
    pub qty: u32,
}

/// Receita de crafting enviada do server pro client. Espelho do
/// `crate::CraftRecipe` mas sem o `&'static str` (use `String` pra serializar).
/// Usado pra cliente renderizar a UI de crafting baseada no que esta no DB,
/// permitindo admin mudar receitas sem rebuild do client.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CraftRecipeNet {
    pub id: u16,
    pub name: String,
    /// 0=Other, 1=Weapon, 2=Armor, 3=Material/resource. UI filtra por tab.
    pub category: u8,
    /// Estação de craft (shared::craft_station): 0=Forja 1=Ateliê 2=Smelter
    /// 3=Marcenaria. UI filtra pela estação que o player abriu. serde(default)
    /// pra compat com mensagens antigas sem o campo.
    #[serde(default)]
    pub station: u8,
    /// 1-4. UI exibe badge colorido.
    pub tier: u8,
    /// (item_id, qty) pares de inputs (max 4 entradas).
    pub inputs: Vec<[u32; 2]>,
    pub output_item_id: u16,
    pub output_qty: u32,
    pub output_item_level: u16,
    pub roll_instance: bool,
    /// Nivel de personagem pra criar. 1 = qualquer um.
    #[serde(default)]
    pub nivel_min: u16,
}

/// De onde sai um item: o "Onde obter" (docs/ONDE_OBTER.md). Montado no
/// servidor a partir das tabelas de loot, de coleta, das lojas da vila, das
/// receitas e das recompensas de missao — so' o que existe de verdade. O
/// Mercado nao vem aqui: o cliente mostra pra todo item nao vinculado.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum FonteDeItem {
    /// Coleta. `tipo` 0 = madeira, 1..4 = pedra pela cor. `chance` por coleta.
    Coleta {
        tipo: u8,
        chance: f32,
        qty_min: u32,
        qty_max: u32,
    },
    /// Bicho comum (zona de spawn). `chance` por morte. `ilhas` = indices de
    /// `terreno::ARQUIPELAGO` onde ele nasce com chance boa.
    Mob {
        kind: u16,
        nome: String,
        chance: f32,
        qty_min: u32,
        qty_max: u32,
        ilhas: Vec<u8>,
    },
    /// Chefe que nasce no mundo aberto. `ilha` = indice de `ARQUIPELAGO`.
    ChefeDoMundo {
        kind: u16,
        nome: String,
        nivel: u16,
        chance: f32,
        ilha: u8,
    },
    /// Vendedor da vila (`loja` = id da loja do NPC).
    Vendedor { loja: u32, nome: String, preco: u32 },
    /// Sai de uma receita de craft.
    Craft {
        receita: u16,
        nome: String,
        nivel_min: u16,
    },
    /// Recompensa de missao.
    Missao {
        quest: u16,
        titulo: String,
        diaria: bool,
    },
    /// Chefe de dungeon/raid: ainda nao existe ("em breve").
    DungeonRaid,
    /// Calendario de presenca (`shared::presenca`): dias da grade do mes.
    Calendario { dias: Vec<u8> },
}

/// Todas as fontes de um item. Vai no `ResourceSources`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ItemResourceSources {
    pub item_id: u16,
    pub sources: Vec<FonteDeItem>,
}

/// Entry da lista de personagens enviada apos login. Cliente renderiza
/// como card na tela de selecao (paper-doll thumbnail + nome + level).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CharacterListEntry {
    pub name: String,
    pub level: u32,
    pub visual: crate::VisualConfig,
    /// item_id da arma equipada (informativo — mostra ao lado do nome).
    pub weapon_id: Option<u16>,
    /// Facção do char (cliente mostra cor/badge no card de seleção).
    #[serde(default)]
    pub faction: crate::Faction,
}

/// Entrada do basket de vendas dentro de um ShopTrade.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct TradeSellEntry {
    pub inv_slot: u16,
    pub qty: u32,
}

/// Retângulo de zona segura enviado ao cliente pra renderização.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct SafeZoneRect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

/// Resultado de um ShopTrade — sucesso ou erro com motivo amigável.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShopTradeResult {
    pub ok: bool,
    pub reason: String,
}

/// Entrada da config de items enviada pro cliente. Cliente sobrescreve
/// `ItemInfo.NameOf` e o icone via runtime cache. icon_path tem precedência
/// sobre icon_col/icon_row.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ItemConfigEntry {
    pub id: u16,
    pub name: String,
    pub icon_path: Option<String>,
    pub icon_col: i32,
    pub icon_row: i32,
    pub equip_slot: Option<String>,
    /// Se false, server bloqueia equip/use. Client pode greyscale o ícone.
    #[serde(default = "default_true")]
    pub active: bool,
    /// Vinculado: nao entra no mercado (docs/MERCADO.md).
    #[serde(default)]
    pub vinculado: bool,
}

fn default_true() -> bool {
    true
}

/// Descritor de um farm node carregado do mapa. Enviado no FarmNodesConfig.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FarmNodeInfo {
    pub id: u32,
    pub x: f32,
    pub y: f32,
    pub kind: String,
    pub tier: u8,
}

/// Replicacao do mundo enviada a cada tick.
#[derive(Debug, Clone, Serialize, Deserialize)]
/// Um tick de mundo, em delta.
///
/// Tres listas em vez de uma: `entered` traz o dado estavel de quem acabou de
/// aparecer, `states` traz so' quem MUDOU de estado, `removed` quem saiu (por
/// morte ou por sair do AOI). Entidade que nao aparece em lista nenhuma esta
/// parada e nao custa byte.
pub struct WorldSnapshot {
    pub tick: u32,
    pub server_time_ms: u64,
    pub last_input_seq: u32,
    /// Entidades que entraram no campo de visao neste tick.
    pub entered: Vec<EntityMeta>,
    /// Estado de quem mudou.
    pub states: Vec<EntityState>,
    pub removed: Vec<EntityId>,
    /// Quem apanhou neste tick, entre os que este jogador enxerga. O cliente
    /// desenha o numero, a faisca e o tranco a partir daqui — e nao da queda
    /// de vida, que no modo imortal nao acontece e num golpe absorvido
    /// tambem nao.
    pub acertos: Vec<Acerto>,
}

/// Um golpe que acertou alguem.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Acerto {
    pub alvo: EntityId,
    /// Dano REAL do golpe, sem o corte pela vida que sobrava.
    pub dano: i32,
    pub critico: bool,
    /// De onde veio o golpe (aponta pro atacante), quantizado em -127..127.
    pub de: [i8; 2],
    /// Quem bateu — o cliente vira ele pro alvo.
    pub atacante: EntityId,
}

fn default_xp_mult() -> u64 {
    crate::constants::DEFAULT_XP_MULTIPLIER
}

/// Codificacao do wire.
///
/// Era JSON. Com ~40 campos por entidade e nome de campo repetido em cada uma,
/// uma entidade custava 297 bytes: 100 mobs a 30Hz davam 0,85 MB/s POR JOGADOR,
/// ou 3 GB por hora de dado movel. Insustentavel no celular, que e' o alvo.
///
/// Postcard nao e' auto-descritivo: nao carrega nome de campo, inteiro vai em
/// varint e `Option::None` custa 1 byte. Em troca, os dois lados precisam
/// compilar exatamente a MESMA definicao — o que aqui e' de graca, porque
/// cliente e servidor usam este crate. Foi por nao ter isso que a producao caiu
/// com web em 63 e cliente em 66.
///
/// Por isso tambem NAO pode haver `skip_serializing_if` em tipo de wire: ele
/// muda a quantidade de campos escritos, e sem nome de campo o outro lado nao
/// tem como saber que faltou um.
pub fn encode<T: Serialize>(msg: &T) -> anyhow::Result<Vec<u8>> {
    Ok(postcard::to_allocvec(msg)?)
}

pub fn decode<T: for<'de> serde::Deserialize<'de>>(bytes: &[u8]) -> anyhow::Result<T> {
    Ok(postcard::from_bytes(bytes)?)
}

pub fn version() -> u16 {
    PROTOCOL_VERSION
}
