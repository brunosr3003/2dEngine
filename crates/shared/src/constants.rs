//! Constantes do mundo. Centralizadas aqui para garantir que cliente e
//! servidor concordem sobre a simulacao (tickrate, AOI, etc.).

/// Taxa de tick do servidor em Hz. O cliente tambem simula a este rate
/// para predicao client-side; divergir disso quebra a reconciliacao.
pub const TICK_RATE_HZ: u32 = 30;

/// Delta de tempo de um tick em segundos.
pub const TICK_DT: f32 = 1.0 / TICK_RATE_HZ as f32;

/// Raio de interesse (Area Of Interest) em tiles. Apenas entidades dentro
/// deste raio do jogador sao replicadas para cada cliente. Manter pequeno
/// reduz banda mas aumenta pop-in.
pub const AOI_RADIUS: f32 = 24.0;

/// Teto de entidades num snapshot, por jogador.
///
/// Sem teto, jogador no meio de uma horda recebe tudo que mexe: medido em 214
/// estados por snapshot com 200 jogadores e 1000 mobs, 66 KB/s cada. As mais
/// distantes ficam de fora — sao as que ele menos enxerga.
pub const AOI_MAX_ENTIDADES: usize = 60;

/// Folga pra entidade JA conhecida continuar dentro do teto. Sem histerese ela
/// entra e sai a cada passo do jogador, e reenviar o meta (24 bytes) come o
/// que o teto economizou.
pub const AOI_HISTERESE: usize = 12;

/// Ate esta distancia (tiles) a entidade atualiza todo tick.
pub const AOI_PERTO: f32 = 10.0;
/// Ate aqui, a cada 3 ticks. Depois, a cada 6.
pub const AOI_MEIO: f32 = 17.0;

/// Tamanho de uma celula do grid espacial (em tiles). Deve ser >= AOI/2
/// para que a busca de vizinhos acesse no maximo 4 celulas.
pub const SPATIAL_CELL_SIZE: f32 = 16.0;

/// Limite de jogadores por shard/mapa. Acima disso, spawn novo shard.
pub const MAX_PLAYERS_PER_SHARD: usize = 256;

/// Versao do protocolo. INCREMENTAR sempre que mensagens/layouts mudarem
/// em shared::protocol — clientes com versao errada sao rejeitados.
pub const PROTOCOL_VERSION: u16 = 76;

// ── Boat (Sea-of-Thieves style: vela/leme/ancora separados) ─────────────────
/// Velocidade maxima de qualquer barco (tiles/s). Atingida com vela full,
/// vento maximo, alinhamento perfeito.
pub const BOAT_MAX_SPEED: f32 = 32.0;

/// Velocidade base do barco com vela up — fracao de BOAT_MAX_SPEED garantida
/// mesmo SEM vento ou contra vento (vento vira boost, nao condicao). Modelo:
///   target = sail_factor × (BASE + BOOST × wind.intensity × alignment) × MAX
/// Com BASE=0.55 BOOST=0.45 → contra-vento ~55%, a-favor ~100%.
pub const BOAT_SAIL_BASE: f32  = 0.55;
pub const BOAT_WIND_BOOST: f32 = 0.45;
/// Drag da agua aplicado por segundo (sem vento, barco para em ~10s).
/// Drag baixo + lerp_k baixo = barco com inercia (accel/decel lentos).
pub const BOAT_WATER_DRAG: f32 = 0.25;
/// Multiplicador de drag quando ancora dropada (decel rapido).
pub const BOAT_ANCHOR_DRAG_MULT: f32 = 8.0;
/// Yaw rate maximo (rad/s) com leme totalmente virado e barco em velocidade
/// total. Escala com `lin_vel.length() / BOAT_MAX_SPEED` — leme so funciona
/// com movimento, igual barco real.
pub const BOAT_MAX_YAW_RATE: f32 = 0.45;
/// Tempo (s) para a animacao de drop/raise da ancora. Durante a anim a
/// `anchor_progress` cresce de 0 ate 1 (drop) ou volta a 0 (raise).
pub const BOAT_ANCHOR_ANIM_TIME: f32 = 2.5;
/// Half-extents do deck do Lylian em tiles (local space). Player montado
/// fica clamp ao bbox `[-x..x, -y..y]`. Tamanho visual do sprite eh
/// 4×8 (PPU=16, sprite 64×128) — deck bate com o visual.
pub const BOAT_LYLIAN_DECK_HALF_W: f32 = 2.0;
pub const BOAT_LYLIAN_DECK_HALF_H: f32 = 4.0;

/// Codigos de estacao usados em `GrabStation { station }` e
/// `EntitySnapshot.station`. Manter em sync com o cliente C#.
/// Canhoes: codigos >= CANNON_BASE. slot = code - CANNON_BASE.
/// Permite barcos com N canhoes sem mexer no enum.
pub mod station {
    pub const HELM:        u8 = 0;
    pub const SAIL:        u8 = 1;
    pub const ANCHOR:      u8 = 2;
    pub const CANNON_BASE: u8 = 3;
    pub const MAX_CANNONS: u8 = 16;
}

// ── Canhao ──────────────────────────────────────────────────────────────
/// Range maximo do tiro (tiles) em power=100%. Pode ser override por
/// upgrade no futuro (cannon_config tier).
pub const CANNON_MAX_RANGE: f32 = 28.0;
/// Range minimo do tiro (tiles) em power=0%. Garante que charge=0 nao
/// derruba a bola no proprio barco.
pub const CANNON_MIN_RANGE: f32 = 6.0;
/// Range maximo dos canhoes em angulacao lateral (rad). Aim varia de
/// -CANNON_AIM_MAX_RAD a +CANNON_AIM_MAX_RAD relativo ao "forward" do canhao
/// (que e' o lado do barco onde ele esta instalado).
pub const CANNON_AIM_MAX_RAD: f32 = std::f32::consts::FRAC_PI_4; // 45 graus
/// Tempo de voo da bola — t_max do arco parabolico, em segundos.
/// Range curto = vela rapida; range max = ~1.6s.
pub const CANNON_FLIGHT_TIME_MIN: f32 = 0.55;
pub const CANNON_FLIGHT_TIME_MAX: f32 = 1.60;
/// Altura maxima do arco da bola (unidades world, render offset Y).
/// Cresce com range pra dar a sensacao de tiro mais longo = arco mais alto.
pub const CANNON_PEAK_HEIGHT_MIN: f32 = 2.0;
pub const CANNON_PEAK_HEIGHT_MAX: f32 = 6.0;
/// Tempo (s) pra carga ir de 0 -> 100% (depois para no topo).
pub const CANNON_CHARGE_TIME_S: f32 = 2.0;
/// Cooldown (s) entre tiros pra um canhao.
pub const CANNON_COOLDOWN_S: f32 = 2.5;
/// Dano base da bola no impacto (no centro do blast). Cai linear com distancia.
pub const CANNON_DAMAGE_BASE: i32 = 80;
/// Raio do AoE (tiles).
pub const CANNON_BLAST_RADIUS: f32 = 2.5;

/// Angulo maximo absoluto da roda do leme em radianos. 4π ≈ 2 voltas
/// pra cada lado (lock-to-lock = 4 voltas total). Acima disso, server
/// clamp. yaw_rate eh proporcional a rudder_angle / MAX_RUDDER_ANGLE.
pub const BOAT_MAX_RUDDER_ANGLE: f32 = 4.0 * std::f32::consts::PI;

/// Poise base concedido a todo player a partir do nivel `POISE_BASE_UNLOCK_LEVEL`.
/// Skills T4 (Iron Will, Unstoppable, etc) somam +50/rank em cima disso.
pub const POISE_BASE_VALUE: i32 = 30;
pub const POISE_BASE_UNLOCK_LEVEL: u8 = 60;

/// Velocidade base do jogador em tiles/segundo.
pub const PLAYER_SPEED: f32 = 5.0;

/// Multiplicador de velocidade durante sprint (Shift + stamina > 0).
pub const SPRINT_SPEED_MULT: f32 = 1.65;

// ── Dash ───────────────────────────────────────────────────────────────
/// Duracao do impulso de dash em segundos.
pub const DASH_DURATION: f32 = 0.20;
/// Velocidade durante o dash (substitui PLAYER_SPEED * speed_mult).
pub const DASH_SPEED: f32 = 14.0;
/// Cooldown entre dashes em segundos. SPD reduz via divisao por
/// `speed_mult` — clampado em `DASH_COOLDOWN_MIN`.
pub const DASH_COOLDOWN: f32 = 1.5;
/// Cooldown minimo de dash apos reducao por SPD (clamp). Baixo pra permitir
/// dash quase instantaneo em SPD alto (~200+).
pub const DASH_COOLDOWN_MIN: f32 = 0.08;
/// Custo de stamina pra dash.
pub const DASH_STAMINA_COST: i32 = 30;

/// Capacidade maxima de stamina (pontos). Fixa para todas as classes por ora.
pub const STAMINA_MAX: i32 = 100;

/// Drena stamina por segundo enquanto sprintando.
pub const STAMINA_DRAIN_PER_SEC: f32 = 40.0;

/// Regenera stamina por segundo quando NAO sprintando.
pub const STAMINA_REGEN_PER_SEC: f32 = 25.0;

/// Velocidade dos projeteis em tiles/segundo.
pub const PROJ_SPEED: f32 = 15.0;

/// Tempo de vida de um projetil em segundos.
pub const PROJ_TTL: f32 = 1.5;

/// Cooldown entre ataques em segundos.
pub const ATTACK_COOLDOWN: f32 = 0.25;

/// Tempo (s) sem atacar antes do combo melee resetar pra step 0 (Slash1).
/// Mantém em sync com client `_comboResetTime` em CharacterAnimator.
pub const COMBO_RESET_TIME: f32 = 1.0;
/// Quantidade de steps no combo melee. 0=Slash1, 1=Slash2, 2=Finisher.
pub const COMBO_STEPS: u8 = 3;

/// Cooldown estendido para Bow. Sincroniza com a anim de saque do arco
/// (`ShootStraight`: 8 frames × 70ms = 560ms).
pub const BOW_ATTACK_COOLDOWN: f32 = 0.55;
/// Delay entre input e spawn real da flecha — frame de release do saque.
pub const BOW_FIRE_DELAY: f32 = 0.40;

/// Cooldown para Wand/Staff. Anim usada é `Thrust` (4 frames × 80ms = 320ms).
pub const MAGIC_ATTACK_COOLDOWN: f32 = 0.32;
/// Delay entre input e spawn da bola de fogo — durante a extensão do thrust
/// (frame ~3 de 4, ~70% da anim). Match com o feel do bow (0.40/0.56 = 71%).
/// O gate de PRIMARY no cliente (InputHandler) impede shots fantasmas pós-
/// depleção, então não precisa empilhar o delay no fim da anim.
pub const MAGIC_FIRE_DELAY: f32 = 0.22;

/// Offset vertical (Y mundo) do spawn de projétil em relação à pos da entidade.
/// Pos fica nos pés (PaperDoll pivot 0.40); arco/cajado é segurado próximo ao
/// peito, então projétil sai ~0.5 unidade acima do pé.
pub const PROJ_SPAWN_OFFSET_Y: f32 = 0.5;

/// Offset adicional (na direção do tiro) para fireball — faz a bola sair da
/// ponta da varinha em vez do peito do char. Aplicado só pra projéteis de
/// magia; flechas continuam saindo do peito (saem do arco visualmente).
pub const FIREBALL_FORWARD_OFFSET: f32 = 0.6;

/// Stamina consumida por cada ataque primario (LMB).
pub const ATTACK_STAMINA_COST: f32 = 0.0;  // basic attack sem custo (hardcore: stamina so' pra dash + sprint)

/// Velocidade dos inimigos em tiles/segundo.
pub const ENEMY_SPEED: f32 = 2.0;

/// Raio em que o inimigo detecta e persegue jogadores.
pub const ENEMY_DETECT_RANGE: f32 = 9.0;

/// Raio de ataque do inimigo.
pub const ENEMY_ATTACK_RANGE: f32 = 7.0;

/// Cooldown de ataque dos inimigos.
pub const ENEMY_ATTACK_COOLDOWN: f32 = 2.0;

/// Raio de colisao de jogadores/inimigos (para hit detection).
pub const ENTITY_RADIUS: f32 = 0.35;

/// Quanto tempo o corpo fica no ar num pulo.
///
/// Curto de proposito: pulo em MMO de vista de cima e' pra vencer degrau, nao
/// pra plataforma. Longo demais e o jogador perde o controle do boneco no meio
/// do combate por alvo.
pub const PULO_DURACAO: f32 = 0.60;

/// Espera entre um pulo e o proximo.
pub const PULO_ESPERA: f32 = 0.15;

/// Altura do PICO do pulo, em unidades.
///
/// Tem que passar do degrau maximo (`PULO_BLOCOS` x `BLOCO` = 1,5), senao o
/// boneco subiria um barranco que o pulo nunca alcanca.
pub const PULO_ALTURA: f32 = 1.8;

/// Altura do corpo acima de onde ele saiu, `t` segundos depois do pulo.
///
/// Balistica de verdade: sobe, desacelera, para no pico, cai. `v0` e a
/// gravidade saem de `PULO_ALTURA` e `PULO_DURACAO` — pico na metade do
/// tempo, chao no fim — entao mexer nos dois numeros de cima continua dando
/// um arco coerente.
///
/// **Esta funcao e' a regra, nao o desenho.** O cliente usa pra saber onde
/// desenhar o corpo e o servidor usa pra saber que degrau aceitar naquele
/// instante. Antes o arco era enfeite somado por cima do chao enquanto o
/// servidor liberava o degrau inteiro durante todo o pulo: o corpo colava no
/// piso de cima assim que saia do lugar, sem subida e sem queda. Duas
/// verdades sobre a mesma altura, e o olho via a discordancia.
pub fn altura_do_pulo(t: f32) -> f32 {
    if t <= 0.0 || t >= PULO_DURACAO {
        return 0.0;
    }
    let v0 = 4.0 * PULO_ALTURA / PULO_DURACAO;
    let g = 8.0 * PULO_ALTURA / (PULO_DURACAO * PULO_DURACAO);
    v0 * t - g * t * t * 0.5
}

/// Gravidade da queda, em unidades por segundo ao quadrado.
///
/// A MESMA do pulo: dois valores diferentes dariam ao mundo duas fisicas, e
/// cair de um barranco pareceria outro jogo que pular nele.
pub fn gravidade() -> f32 {
    8.0 * PULO_ALTURA / (PULO_DURACAO * PULO_DURACAO)
}

/// Raio de colisao de projeteis.
pub const PROJ_RADIUS: f32 = 0.15;

/// Duração do stagger ao tomar dano (s). Durante esse período a entidade
/// não pode andar nem atacar — sincroniza com a anim de Hurt no cliente.
pub const HURT_STAGGER_DURATION: f32 = 0.25;

/// Tempo (s) que o cadáver de um inimigo fica no mapa antes de despawnar.
/// Cliente exibe pose deitada + tint escuro durante esse período.
pub const ENEMY_CORPSE_LINGER: f32 = 2.0;

/// Duração do "spawn grace" — período após criar o enemy em que ele fica
/// invisível no cliente (rodando VFX de invocação) e estático no servidor
/// (sem mover, atacar ou tomar dano). Casa com a duração da spawn-VFX no
/// cliente (Magic Bursts/round_sparkle_burst_001 = 14 frames × 60ms).
pub const ENEMY_SPAWN_GRACE: f32 = 0.84;

/// Raio do "hitbox" da entidade (envolve TORSO/CABEÇA, não só os pés).
/// Usado por melee e projétil pra decidir se atinge — separado do
/// ENTITY_RADIUS (que continua pequeno pra física/collisão).
pub const HIT_TARGET_RADIUS: f32 = 0.6;
/// Offset vertical do centro do hitbox em relação ao Y da entidade (que fica
/// nos pés). 0.6 ≈ altura do tronco/peito do paper-doll Mana Seed.
pub const HIT_TARGET_Y_OFFSET: f32 = 0.6;

/// Tempo de respawn do jogador em segundos (nao usado em PvE puro — player
/// entra em Downed State; respawn so ocorre em PvP apos execucao).
pub const RESPAWN_DELAY: f32 = 3.0;

/// Tempo em segundos pra um jogador derrubado se levantar sozinho (PvE).
/// Durante esse periodo, dano de monstros NAO mata mas reseta o timer.
pub const DOWNED_HEAL_TIME: f32 = 10.0;

/// Velocidade do jogador enquanto derrubado (fracao de PLAYER_SPEED).
/// Rasteja lento, pode se esconder.
pub const DOWNED_SPEED_MULT: f32 = 0.2;

/// HP que o jogador recebe ao se levantar do Downed State (fracao do max).
pub const DOWNED_REVIVE_HP_PCT: f32 = 0.05;

/// HP maximo da barra do Downed State. So jogadores (nao monstros)
/// conseguem reduzir — quando zera, morte real com respawn.
pub const DOWNED_HP_MAX: i32 = 100;

/// Tipos de proficiencia (classless). XP se acumula ao usar arma do tipo.
///
/// Variantes adicionadas no Skills Phase 1 (M11): `Axe` e `Spear`. Antes
/// HAMMER (item 25) caía em Unarmed; agora vai pra Axe. SPEAR (item 26) idem.
/// O array de proficiencies em CharacterRow cresceu de 6 pra `PROF_COUNT`.
/// Quantas proficiencias um personagem tem: uma por CONJUNTO de arma.
///
/// Eram quinze — oito de arma, tres de coleta, quatro de artesanato. Coleta e
/// artesanato nao tem proficiencia NENHUMA: coletar e' automatico e nao tem
/// portao. As oito de arma viraram os quatro conjuntos, e o indice e'
/// `shared::skills::Conjunto as usize`.
pub const PROF_COUNT: usize = 4;


/// Nivel de proficiencia dado XP acumulado (curva quadratica similar ao XP do player).
pub const fn proficiency_level(prof_xp: u64) -> u32 {
    let mut lvl = 1u32;
    let mut need = 50u64;
    let mut rem = prof_xp;
    while rem >= need {
        rem -= need;
        lvl += 1;
        need = (lvl as u64) * 50;
        if lvl >= 100 { break; }
    }
    lvl
}

/// XP ganho por matar um inimigo (fallback — helpers por kind mais abaixo).
pub const XP_PER_KILL: u64 = 30;

// Stats/loot/shop de inimigos e itens vivem no DB (server crate::economy).
// Constantes de gameplay puras (cones, delays sem balancing) ficam aqui.

/// Abertura angular do cone de ataque do boss (radianos entre 1o e ultimo proj).
pub const BOSS_SPREAD_RAD: f32 = 1.0; // ~57 graus

/// Tempo de respawn do boss em segundos.
pub const BOSS_RESPAWN_DELAY: f32 = 120.0;

/// Cap máximo de level do personagem. Acima disso, XP continua acumulando
/// mas `level_of_xp` clamp no valor; nenhum SP/stat point novo é gerado.
pub const CHAR_LEVEL_CAP: u32 = 100;

/// Multiplier default da curva de XP. Configuravel em runtime via `server_config`
/// table — server le no startup e envia pro client no `HandshakeAck`. Mudar
/// pra evento de XP duplicado: UPDATE server_config + restart server.
pub const DEFAULT_XP_MULTIPLIER: u64 = 500;

/// Retorna o level derivado a partir da XP acumulada com multiplier custom.
/// Use `level_of_xp` pra default. Curva: cada subida custa `lvl² × mult` xp.
pub const fn level_of_xp_with_mult(xp: u64, mult: u64) -> u32 {
    let mut lvl = 1u32;
    let mut need = mult;
    let mut remaining = xp;
    while remaining >= need {
        remaining -= need;
        lvl += 1;
        if lvl >= CHAR_LEVEL_CAP { return CHAR_LEVEL_CAP; }
        need = (lvl as u64) * (lvl as u64) * mult;
    }
    lvl
}

/// XP cumulativa necessaria pra atingir `level` com multiplier custom.
/// Sum-of-squares × mult.
pub const fn xp_for_level_with_mult(level: u32, mult: u64) -> u64 {
    let mut sum = 0u64;
    let mut l = 1u32;
    while l < level {
        sum += (l as u64) * (l as u64) * mult;
        l += 1;
    }
    sum
}

/// Default — server overwrites via DB-loaded mult em runtime, client recebe via
/// HandshakeAck. Estes wrappers ficam pra call sites legacy/test.
pub const fn level_of_xp(xp: u64) -> u32 { level_of_xp_with_mult(xp, DEFAULT_XP_MULTIPLIER) }
pub const fn xp_for_level(level: u32) -> u64 { xp_for_level_with_mult(level, DEFAULT_XP_MULTIPLIER) }

/// Quantidade de inimigos gerados no inicio.
pub const ENEMY_START_COUNT: usize = 24;

/// Numero de slots do inventario do jogador.
pub const INVENTORY_SLOTS: usize = 40;

/// Raio em tiles pra coletar um loot.
pub const PICKUP_RADIUS: f32 = 0.8;

/// Delay em segundos depois do spawn antes do auto-pickup ficar ativo.
/// Garante que o player VEJA o drop cair antes de ele "voar" pro inv.
pub const LOOT_PICKUP_DELAY_S: f32 = 0.6;

/// Itens conhecidos. Numeric id vai pro DB e rede. Manter sincronizado com
/// o cliente para sprite/cor por item.
pub mod item_id {
    // Moeda / consumíveis
    pub const GOLD:            u16 = 1;
    pub const HEALTH_POTION:   u16 = 2;
    pub const MANA_POTION:     u16 = 8;
    pub const GREATER_HEAL:    u16 = 9;   // +150 HP
    pub const GREATER_MANA:    u16 = 10;  // +100 MP
    pub const STAMINA_POTION:  u16 = 11;  // restaura 100 stamina
    // Armas
    pub const SWORD:           u16 = 3;
    pub const STAFF:           u16 = 6;
    pub const DAGGER:          u16 = 12;  // rapido, menos dano, +dex
    pub const GREAT_SWORD:     u16 = 13;  // muito dano, -mp
    pub const BOW:             u16 = 14;  // ranged, +dex
    pub const WAND:            u16 = 15;  // fraca mas muito mp
    // Armaduras
    pub const ARMOR:           u16 = 4;
    pub const SHIELD:          u16 = 7;
    pub const LEATHER_ARMOR:   u16 = 16;  // leve, +dex
    pub const PLATE_ARMOR:     u16 = 17;  // pesada, muito HP, -dex
    pub const ROBE:            u16 = 18;  // mago, +mp
    // Acessórios
    pub const RING:            u16 = 5;
    pub const AMULET:          u16 = 19;  // +wis +mp
    pub const LUCKY_RING:      u16 = 20;  // +dex +mp
    // Materiais / loot raro
    pub const GEM:             u16 = 21;  // valioso, vendavel
    pub const IRON_INGOT:      u16 = 22;
    pub const DRAGON_SCALE:    u16 = 23;  // raro de boss
    // === Fase D — novas armas + acessórios ===
    pub const SCIMITAR:        u16 = 24;  // espada curva, atk+atk_spd
    pub const AXE:             u16 = 25;  // machado, atk alto + def (era HAMMER pré-M11)
    pub const SPEAR:           u16 = 26;  // lança, atk+dex
    pub const CROSSBOW:        u16 = 27;  // besta, ranged + crit
    pub const HEAVY_SHIELD:    u16 = 28;  // escudo pesado, def alta
    pub const PENDANT:         u16 = 29;  // pingente, hp+mp
    pub const CHARM:           u16 = 30;  // amuleto crit

    // === Fase E — slots novos (helm/legs/boots/gloves/belt/cape/necklace) ===
    pub const HELM_LEATHER:    u16 = 31;  // capacete leve, def+dex
    pub const HELM_PLATE:      u16 = 32;  // elmo pesado, hp+def
    pub const LEGS_LEATHER:    u16 = 33;  // calça leve, dex+def
    pub const LEGS_PLATE:      u16 = 34;  // calça pesada, hp+def
    pub const BOOTS_LEATHER:   u16 = 35;  // botas leves, mov+dex
    pub const BOOTS_PLATE:     u16 = 36;  // botas pesadas, def+hp
    pub const GLOVES_LEATHER:  u16 = 37;  // luvas leves, atk_spd+dex
    pub const GLOVES_PLATE:    u16 = 38;  // manoplas, atk+def
    pub const BELT_BASIC:      u16 = 39;  // cinto, hp+def
    pub const BELT_MAGIC:      u16 = 40;  // faixa magica, mp+wis
    pub const CAPE_BASIC:      u16 = 41;  // capa, def+hp
    pub const CAPE_MAGIC:      u16 = 42;  // manto magico, mp+wis
    pub const NECKLACE_BASIC:  u16 = 43;  // colar, hp+wis
    pub const NECKLACE_MAGIC:  u16 = 44;  // colar magico, mp+wis

    // === Fase F — armas tier 2 (gate de level + proficiência) ===
    pub const ENHANCED_SWORD:  u16 = 45;  // espada lvl 10, sword prof 5 — atk dobrado, hp+10
    pub const VETERAN_SWORD:   u16 = 46;  // espada lvl 20, sword prof 10 — atk +28, hp+20, def+2
    pub const ENHANCED_BOW:    u16 = 47;  // arco lvl 10, bow prof 5 — atk dobrado, dex bump
    pub const ENHANCED_STAFF:  u16 = 48;  // cajado lvl 10, staff prof 5 — mp+atk dobrados
    pub const ENHANCED_WAND:   u16 = 49;  // varinha lvl 10, wand prof 5 — mp+wis dobrados
    pub const ENHANCED_AXE:    u16 = 50;  // machado lvl 10, axe prof 5 — atk +hp dobrado
    pub const ENHANCED_SPEAR:  u16 = 51;  // lança lvl 10, spear prof 5 — atk+dex dobrados

    // Resources — material de crafting. Tier define poder do item resultante.
    // Drops de mob baseados em loot_item_level: 1-15→t1, 16-30→t2, 31-50→t3, 51+→t4.
    pub const WOOD_T1:         u16 = 60;
    pub const WOOD_T2:         u16 = 61;
    pub const WOOD_T3:         u16 = 62;
    pub const WOOD_T4:         u16 = 63;
    pub const LEATHER_T1:      u16 = 64;
    pub const LEATHER_T2:      u16 = 65;
    pub const LEATHER_T3:      u16 = 66;
    pub const LEATHER_T4:      u16 = 67;
    pub const MINERAL_T1:      u16 = 68;
    pub const MINERAL_T2:      u16 = 69;
    pub const MINERAL_T3:      u16 = 70;
    pub const MINERAL_T4:      u16 = 71;

    // === Peixes (drop da pesca). Stackáveis, sem slot. O species da
    // EntityKind::Fish(n) mapeia 1:1 pra estes IDs via fish_item_for_species. ===
    pub const FISH_ANCHOVY:       u16 = 96;  // T1 comum
    pub const FISH_CLOWNFISH:     u16 = 97;  // T2
    pub const FISH_SURGEONFISH:   u16 = 98;  // T3
    pub const FISH_PUFFERFISH:    u16 = 99;  // T4 raro

    // === Materiais de coleta ==========================================
    //
    // A economia inteira cabe em poucos nomes de proposito: arma e sub-arma
    // gastam o MESMO tipo de recurso, armaduras diferentes gastam o mesmo
    // entre si, acessorios idem. Sao poucos recursos girando muito, em vez de
    // uma lista longa que ninguem consegue precificar.
    //
    // Cada material existe nas QUATRO cores. O id base e' a cinza e a cor
    // soma um — ver `na_cor`.
    //
    //   arma / sub-arma : Scale ou Claw (1) + Steel 300 + Dark Heart Stone 100
    //                     + Moon Shadow Stone 100
    //   armadura        : Couro (1) + Steel 300 + Quintessence 100
    //                     + Exorcism Bauble 100
    //   acessorio       : Horn (1) + Platinum 300 + Illuminating Fragment 100
    //                     + Anima Stone 100
    //
    // Darksteel e Copper variam com o NIVEL do item, nao com a cor. Glittering
    // Powder nao entra em craft nenhum: ela sobe a cor do material.
    pub const STEEL: u16                 = 300;
    pub const DARK_HEART_STONE: u16      = 304;
    pub const MOON_SHADOW_STONE: u16     = 308;
    pub const QUINTESSENCE: u16          = 312;
    pub const EXORCISM_BAUBLE: u16       = 316;
    pub const PLATINUM: u16              = 320;
    pub const ILLUMINATING_FRAGMENT: u16 = 324;
    pub const ANIMA_STONE: u16           = 328;
    /// Chave da arma: 1 por craft.
    pub const SCALE: u16                 = 332;
    /// Chave da sub-arma: 1 por craft.
    pub const CLAW: u16                  = 336;
    /// Chave do acessorio: 1 por craft.
    pub const HORN: u16                  = 340;
    /// Chave da armadura: 1 por craft. E' o `LEATHER_T*` que ja' existia.
    pub const HIDE: u16                  = LEATHER_T1;

    /// Sem cor: quantidade varia com o NIVEL do item, nao com a cor dele.
    pub const COPPER: u16                = 344;
    pub const DARKSTEEL: u16             = 345;
    /// Sem cor porque ela E' a cor: e' o que sobe um material de uma cor pra
    /// proxima. Ver `docs/ECONOMIA_DE_CRAFT.md`.
    pub const GLITTERING_POWDER: u16     = 346;

    /// Todos os materiais que existem nas quatro cores, pelo id da cinza.
    pub const MATERIAIS_COLORIDOS: [u16; 12] = [
        STEEL, DARK_HEART_STONE, MOON_SHADOW_STONE,
        QUINTESSENCE, EXORCISM_BAUBLE,
        PLATINUM, ILLUMINATING_FRAGMENT, ANIMA_STONE,
        SCALE, CLAW, HORN, HIDE,
    ];

    /// O mesmo material, na cor pedida (1 cinza .. 4 roxo).
    pub const fn na_cor(base: u16, cor: u8) -> u16 {
        base + (if cor < 1 { 0 } else if cor > 4 { 3 } else { cor - 1 }) as u16
    }

    /// item_id do peixe pra um species da EntityKind::Fish (1-4). Espécie
    /// fora do range cai no peixe T1. Server usa ao conceder o drop da pesca.
    pub fn fish_item_for_species(species: u16) -> u16 {
        match species {
            1 => FISH_ANCHOVY,
            2 => FISH_CLOWNFISH,
            3 => FISH_SURGEONFISH,
            4 => FISH_PUFFERFISH,
            _ => FISH_ANCHOVY,
        }
    }

    // === Fase Naval — barcos (consumiveis usados na margem) ===
    /// Progressao de barcos por TIER: Esquife (T1, 1 lugar) → Lylian (T2) →
    /// futuros T3/T4. O "tier" e' gameplay; o boat_kind (0/1) e' so id de
    /// renderer/config. Esquife = kind 1, Lylian = kind 0 (legado).
    /// Esquife — barquinho de 1 passageiro, primeiro barco craftavel (barato).
    pub const BOAT_ESQUIFE: u16 = 101;
    /// Lylian Leutard — barco T2 de exploracao costeira. Spawn na agua
    /// adjacente quando usado a partir de uma margem walkable.
    pub const BOAT_LYLIAN_LEUTARD: u16 = 100;

    // === Relíquias do Abismo — Anéis de Storyline ===
    pub const RING_TIDE:      u16 = 161; // Anel da Maré Alta (Capítulo 2 - Lvl 20)
    pub const RING_IGNITION:  u16 = 162; // Anel da Ignição Negra (Capítulo 3 - Lvl 50)
    pub const RING_MIST:      u16 = 163; // Anel da Névoa Fantasma (Capítulo 5 - Lvl 85)
    pub const RING_TEMPEST:   u16 = 164; // Coração da Tempestade (Capítulo 6 - Lvl 100)
}

/// True se o item_id e' um barco (consumido ao usar; spawna entidade Boat).
pub fn is_boat_item(id: u16) -> bool {
    id == item_id::BOAT_LYLIAN_LEUTARD || id == item_id::BOAT_ESQUIFE
}

// ============================================================================
// Crafting recipes
// ============================================================================

/// Receita de crafting — N entradas (item_id, qty) consumidas, 1 saida.
/// `output_item_level` define ilvl da instance rolada (so' aplica se
/// `roll_instance=true`); senao output e' stackavel puro.
#[derive(Debug, Clone, Copy)]
pub struct CraftRecipe {
    pub id:                u16,
    pub name:              &'static str,
    pub inputs:            [(u16, u32); 4],   // (item_id, qty); item_id=0 = vazio
    pub output_item_id:    u16,
    pub output_qty:        u32,
    pub output_item_level: u16,
    pub roll_instance:     bool,
}

/// Tabela hardcoded. Hot-reload via DB em fase futura.
pub const CRAFT_RECIPES: &[CraftRecipe] = &[
    // Weapons T1 — ilvl 5 (Common/Magic baixos).
    CraftRecipe { id:1, name:"Espada T1",  inputs:[(item_id::WOOD_T1,5),(item_id::LEATHER_T1,3),(item_id::MINERAL_T1,4),(0,0)], output_item_id:item_id::SWORD,  output_qty:1, output_item_level:5, roll_instance:true },
    CraftRecipe { id:2, name:"Arco T1",    inputs:[(item_id::WOOD_T1,6),(item_id::LEATHER_T1,4),(item_id::MINERAL_T1,2),(0,0)], output_item_id:item_id::BOW,    output_qty:1, output_item_level:5, roll_instance:true },
    CraftRecipe { id:3, name:"Machado T1", inputs:[(item_id::WOOD_T1,4),(item_id::LEATHER_T1,2),(item_id::MINERAL_T1,6),(0,0)], output_item_id:item_id::AXE,    output_qty:1, output_item_level:5, roll_instance:true },
    CraftRecipe { id:4, name:"Lança T1",   inputs:[(item_id::WOOD_T1,6),(item_id::LEATHER_T1,2),(item_id::MINERAL_T1,4),(0,0)], output_item_id:item_id::SPEAR,  output_qty:1, output_item_level:5, roll_instance:true },
    CraftRecipe { id:5, name:"Adaga T1",   inputs:[(item_id::WOOD_T1,2),(item_id::LEATHER_T1,3),(item_id::MINERAL_T1,4),(0,0)], output_item_id:item_id::DAGGER, output_qty:1, output_item_level:5, roll_instance:true },
    CraftRecipe { id:6, name:"Cajado T1",  inputs:[(item_id::WOOD_T1,7),(item_id::LEATHER_T1,1),(item_id::MINERAL_T1,3),(0,0)], output_item_id:item_id::STAFF,  output_qty:1, output_item_level:5, roll_instance:true },
    CraftRecipe { id:7, name:"Varinha T1", inputs:[(item_id::WOOD_T1,3),(item_id::LEATHER_T1,1),(item_id::MINERAL_T1,2),(0,0)], output_item_id:item_id::WAND,   output_qty:1, output_item_level:5, roll_instance:true },
    // Tier upgrades — 5 unidades T(n) → 1 unidade T(n+1).
    CraftRecipe { id:10, name:"Madeira T2", inputs:[(item_id::WOOD_T1,5),(0,0),(0,0),(0,0)],    output_item_id:item_id::WOOD_T2,    output_qty:1, output_item_level:0, roll_instance:false },
    CraftRecipe { id:11, name:"Couro T2",   inputs:[(item_id::LEATHER_T1,5),(0,0),(0,0),(0,0)], output_item_id:item_id::LEATHER_T2, output_qty:1, output_item_level:0, roll_instance:false },
    CraftRecipe { id:12, name:"Mineral T2", inputs:[(item_id::MINERAL_T1,5),(0,0),(0,0),(0,0)], output_item_id:item_id::MINERAL_T2, output_qty:1, output_item_level:0, roll_instance:false },
    CraftRecipe { id:13, name:"Madeira T3", inputs:[(item_id::WOOD_T2,5),(0,0),(0,0),(0,0)],    output_item_id:item_id::WOOD_T3,    output_qty:1, output_item_level:0, roll_instance:false },
    CraftRecipe { id:14, name:"Couro T3",   inputs:[(item_id::LEATHER_T2,5),(0,0),(0,0),(0,0)], output_item_id:item_id::LEATHER_T3, output_qty:1, output_item_level:0, roll_instance:false },
    CraftRecipe { id:15, name:"Mineral T3", inputs:[(item_id::MINERAL_T2,5),(0,0),(0,0),(0,0)], output_item_id:item_id::MINERAL_T3, output_qty:1, output_item_level:0, roll_instance:false },
    CraftRecipe { id:16, name:"Madeira T4", inputs:[(item_id::WOOD_T3,5),(0,0),(0,0),(0,0)],    output_item_id:item_id::WOOD_T4,    output_qty:1, output_item_level:0, roll_instance:false },
    CraftRecipe { id:17, name:"Couro T4",   inputs:[(item_id::LEATHER_T3,5),(0,0),(0,0),(0,0)], output_item_id:item_id::LEATHER_T4, output_qty:1, output_item_level:0, roll_instance:false },
    CraftRecipe { id:18, name:"Mineral T4", inputs:[(item_id::MINERAL_T3,5),(0,0),(0,0),(0,0)], output_item_id:item_id::MINERAL_T4, output_qty:1, output_item_level:0, roll_instance:false },

    // Weapons T2 — ilvl 20 (Magic/Rare comum, stats melhores que T1).
    CraftRecipe { id:100, name:"Espada T2",  inputs:[(item_id::WOOD_T2,5),(item_id::LEATHER_T2,3),(item_id::MINERAL_T2,4),(0,0)], output_item_id:item_id::SWORD,  output_qty:1, output_item_level:20, roll_instance:true },
    CraftRecipe { id:101, name:"Arco T2",    inputs:[(item_id::WOOD_T2,6),(item_id::LEATHER_T2,4),(item_id::MINERAL_T2,2),(0,0)], output_item_id:item_id::BOW,    output_qty:1, output_item_level:20, roll_instance:true },
    CraftRecipe { id:102, name:"Machado T2", inputs:[(item_id::WOOD_T2,4),(item_id::LEATHER_T2,2),(item_id::MINERAL_T2,6),(0,0)], output_item_id:item_id::AXE,    output_qty:1, output_item_level:20, roll_instance:true },
    CraftRecipe { id:103, name:"Lança T2",   inputs:[(item_id::WOOD_T2,6),(item_id::LEATHER_T2,2),(item_id::MINERAL_T2,4),(0,0)], output_item_id:item_id::SPEAR,  output_qty:1, output_item_level:20, roll_instance:true },
    CraftRecipe { id:104, name:"Adaga T2",   inputs:[(item_id::WOOD_T2,2),(item_id::LEATHER_T2,3),(item_id::MINERAL_T2,4),(0,0)], output_item_id:item_id::DAGGER, output_qty:1, output_item_level:20, roll_instance:true },
    CraftRecipe { id:105, name:"Cajado T2",  inputs:[(item_id::WOOD_T2,7),(item_id::LEATHER_T2,1),(item_id::MINERAL_T2,3),(0,0)], output_item_id:item_id::STAFF,  output_qty:1, output_item_level:20, roll_instance:true },
    CraftRecipe { id:106, name:"Varinha T2", inputs:[(item_id::WOOD_T2,3),(item_id::LEATHER_T2,1),(item_id::MINERAL_T2,2),(0,0)], output_item_id:item_id::WAND,   output_qty:1, output_item_level:20, roll_instance:true },

    // Weapons T3 — ilvl 40.
    CraftRecipe { id:107, name:"Espada T3",  inputs:[(item_id::WOOD_T3,5),(item_id::LEATHER_T3,3),(item_id::MINERAL_T3,4),(0,0)], output_item_id:item_id::SWORD,  output_qty:1, output_item_level:40, roll_instance:true },
    CraftRecipe { id:108, name:"Arco T3",    inputs:[(item_id::WOOD_T3,6),(item_id::LEATHER_T3,4),(item_id::MINERAL_T3,2),(0,0)], output_item_id:item_id::BOW,    output_qty:1, output_item_level:40, roll_instance:true },
    CraftRecipe { id:109, name:"Machado T3", inputs:[(item_id::WOOD_T3,4),(item_id::LEATHER_T3,2),(item_id::MINERAL_T3,6),(0,0)], output_item_id:item_id::AXE,    output_qty:1, output_item_level:40, roll_instance:true },
    CraftRecipe { id:110, name:"Lança T3",   inputs:[(item_id::WOOD_T3,6),(item_id::LEATHER_T3,2),(item_id::MINERAL_T3,4),(0,0)], output_item_id:item_id::SPEAR,  output_qty:1, output_item_level:40, roll_instance:true },
    CraftRecipe { id:111, name:"Adaga T3",   inputs:[(item_id::WOOD_T3,2),(item_id::LEATHER_T3,3),(item_id::MINERAL_T3,4),(0,0)], output_item_id:item_id::DAGGER, output_qty:1, output_item_level:40, roll_instance:true },
    CraftRecipe { id:112, name:"Cajado T3",  inputs:[(item_id::WOOD_T3,7),(item_id::LEATHER_T3,1),(item_id::MINERAL_T3,3),(0,0)], output_item_id:item_id::STAFF,  output_qty:1, output_item_level:40, roll_instance:true },
    CraftRecipe { id:113, name:"Varinha T3", inputs:[(item_id::WOOD_T3,3),(item_id::LEATHER_T3,1),(item_id::MINERAL_T3,2),(0,0)], output_item_id:item_id::WAND,   output_qty:1, output_item_level:40, roll_instance:true },

    // Weapons T4 — ilvl 70 (Epic/Legendary chance crescente).
    CraftRecipe { id:114, name:"Espada T4",  inputs:[(item_id::WOOD_T4,5),(item_id::LEATHER_T4,3),(item_id::MINERAL_T4,4),(0,0)], output_item_id:item_id::SWORD,  output_qty:1, output_item_level:70, roll_instance:true },
    CraftRecipe { id:115, name:"Arco T4",    inputs:[(item_id::WOOD_T4,6),(item_id::LEATHER_T4,4),(item_id::MINERAL_T4,2),(0,0)], output_item_id:item_id::BOW,    output_qty:1, output_item_level:70, roll_instance:true },
    CraftRecipe { id:116, name:"Machado T4", inputs:[(item_id::WOOD_T4,4),(item_id::LEATHER_T4,2),(item_id::MINERAL_T4,6),(0,0)], output_item_id:item_id::AXE,    output_qty:1, output_item_level:70, roll_instance:true },
    CraftRecipe { id:117, name:"Lança T4",   inputs:[(item_id::WOOD_T4,6),(item_id::LEATHER_T4,2),(item_id::MINERAL_T4,4),(0,0)], output_item_id:item_id::SPEAR,  output_qty:1, output_item_level:70, roll_instance:true },
    CraftRecipe { id:118, name:"Adaga T4",   inputs:[(item_id::WOOD_T4,2),(item_id::LEATHER_T4,3),(item_id::MINERAL_T4,4),(0,0)], output_item_id:item_id::DAGGER, output_qty:1, output_item_level:70, roll_instance:true },
    CraftRecipe { id:119, name:"Cajado T4",  inputs:[(item_id::WOOD_T4,7),(item_id::LEATHER_T4,1),(item_id::MINERAL_T4,3),(0,0)], output_item_id:item_id::STAFF,  output_qty:1, output_item_level:70, roll_instance:true },
    CraftRecipe { id:120, name:"Varinha T4", inputs:[(item_id::WOOD_T4,3),(item_id::LEATHER_T4,1),(item_id::MINERAL_T4,2),(0,0)], output_item_id:item_id::WAND,   output_qty:1, output_item_level:70, roll_instance:true },

    // Armor T1 — base pieces, ilvl 5.
    CraftRecipe { id:130, name:"Armadura Couro T1",  inputs:[(item_id::LEATHER_T1,8),(item_id::MINERAL_T1,2),(0,0),(0,0)], output_item_id:item_id::LEATHER_ARMOR, output_qty:1, output_item_level:5, roll_instance:true },
    CraftRecipe { id:131, name:"Armadura Placa T1",  inputs:[(item_id::MINERAL_T1,8),(item_id::LEATHER_T1,2),(0,0),(0,0)], output_item_id:item_id::PLATE_ARMOR,   output_qty:1, output_item_level:5, roll_instance:true },
    CraftRecipe { id:132, name:"Manto T1",           inputs:[(item_id::LEATHER_T1,5),(item_id::WOOD_T1,3),(0,0),(0,0)],    output_item_id:item_id::ROBE,          output_qty:1, output_item_level:5, roll_instance:true },
    CraftRecipe { id:133, name:"Capacete Couro T1", inputs:[(item_id::LEATHER_T1,4),(item_id::MINERAL_T1,1),(0,0),(0,0)], output_item_id:item_id::HELM_LEATHER,  output_qty:1, output_item_level:5, roll_instance:true },
    CraftRecipe { id:134, name:"Elmo Placa T1",      inputs:[(item_id::MINERAL_T1,4),(item_id::LEATHER_T1,1),(0,0),(0,0)], output_item_id:item_id::HELM_PLATE,    output_qty:1, output_item_level:5, roll_instance:true },
    CraftRecipe { id:135, name:"Botas Couro T1",     inputs:[(item_id::LEATHER_T1,3),(item_id::MINERAL_T1,1),(0,0),(0,0)], output_item_id:item_id::BOOTS_LEATHER, output_qty:1, output_item_level:5, roll_instance:true },
    CraftRecipe { id:136, name:"Botas Placa T1",     inputs:[(item_id::MINERAL_T1,4),(0,0),(0,0),(0,0)],                  output_item_id:item_id::BOOTS_PLATE,   output_qty:1, output_item_level:5, roll_instance:true },
    CraftRecipe { id:137, name:"Calças Couro T1",   inputs:[(item_id::LEATHER_T1,5),(item_id::MINERAL_T1,1),(0,0),(0,0)], output_item_id:item_id::LEGS_LEATHER,  output_qty:1, output_item_level:5, roll_instance:true },
    CraftRecipe { id:138, name:"Calças Placa T1",   inputs:[(item_id::MINERAL_T1,5),(item_id::LEATHER_T1,1),(0,0),(0,0)], output_item_id:item_id::LEGS_PLATE,    output_qty:1, output_item_level:5, roll_instance:true },

    // Armor T2 — ilvl 20.
    CraftRecipe { id:140, name:"Armadura Couro T2", inputs:[(item_id::LEATHER_T2,8),(item_id::MINERAL_T2,2),(0,0),(0,0)], output_item_id:item_id::LEATHER_ARMOR, output_qty:1, output_item_level:20, roll_instance:true },
    CraftRecipe { id:141, name:"Armadura Placa T2", inputs:[(item_id::MINERAL_T2,8),(item_id::LEATHER_T2,2),(0,0),(0,0)], output_item_id:item_id::PLATE_ARMOR,   output_qty:1, output_item_level:20, roll_instance:true },
    CraftRecipe { id:142, name:"Manto T2",          inputs:[(item_id::LEATHER_T2,5),(item_id::WOOD_T2,3),(0,0),(0,0)],    output_item_id:item_id::ROBE,          output_qty:1, output_item_level:20, roll_instance:true },
    CraftRecipe { id:143, name:"Capacete Couro T2", inputs:[(item_id::LEATHER_T2,4),(item_id::MINERAL_T2,1),(0,0),(0,0)], output_item_id:item_id::HELM_LEATHER,  output_qty:1, output_item_level:20, roll_instance:true },
    CraftRecipe { id:144, name:"Elmo Placa T2",     inputs:[(item_id::MINERAL_T2,4),(item_id::LEATHER_T2,1),(0,0),(0,0)], output_item_id:item_id::HELM_PLATE,    output_qty:1, output_item_level:20, roll_instance:true },
    CraftRecipe { id:145, name:"Botas Couro T2",    inputs:[(item_id::LEATHER_T2,3),(item_id::MINERAL_T2,1),(0,0),(0,0)], output_item_id:item_id::BOOTS_LEATHER, output_qty:1, output_item_level:20, roll_instance:true },
    CraftRecipe { id:146, name:"Botas Placa T2",    inputs:[(item_id::MINERAL_T2,4),(0,0),(0,0),(0,0)],                  output_item_id:item_id::BOOTS_PLATE,   output_qty:1, output_item_level:20, roll_instance:true },
    CraftRecipe { id:147, name:"Calças Couro T2",  inputs:[(item_id::LEATHER_T2,5),(item_id::MINERAL_T2,1),(0,0),(0,0)], output_item_id:item_id::LEGS_LEATHER,  output_qty:1, output_item_level:20, roll_instance:true },
    CraftRecipe { id:148, name:"Calças Placa T2",  inputs:[(item_id::MINERAL_T2,5),(item_id::LEATHER_T2,1),(0,0),(0,0)], output_item_id:item_id::LEGS_PLATE,    output_qty:1, output_item_level:20, roll_instance:true },

    // Armor T3 — ilvl 40.
    CraftRecipe { id:150, name:"Armadura Couro T3", inputs:[(item_id::LEATHER_T3,8),(item_id::MINERAL_T3,2),(0,0),(0,0)], output_item_id:item_id::LEATHER_ARMOR, output_qty:1, output_item_level:40, roll_instance:true },
    CraftRecipe { id:151, name:"Armadura Placa T3", inputs:[(item_id::MINERAL_T3,8),(item_id::LEATHER_T3,2),(0,0),(0,0)], output_item_id:item_id::PLATE_ARMOR,   output_qty:1, output_item_level:40, roll_instance:true },
    CraftRecipe { id:152, name:"Manto T3",          inputs:[(item_id::LEATHER_T3,5),(item_id::WOOD_T3,3),(0,0),(0,0)],    output_item_id:item_id::ROBE,          output_qty:1, output_item_level:40, roll_instance:true },
    CraftRecipe { id:153, name:"Capacete Couro T3", inputs:[(item_id::LEATHER_T3,4),(item_id::MINERAL_T3,1),(0,0),(0,0)], output_item_id:item_id::HELM_LEATHER,  output_qty:1, output_item_level:40, roll_instance:true },
    CraftRecipe { id:154, name:"Elmo Placa T3",     inputs:[(item_id::MINERAL_T3,4),(item_id::LEATHER_T3,1),(0,0),(0,0)], output_item_id:item_id::HELM_PLATE,    output_qty:1, output_item_level:40, roll_instance:true },
    CraftRecipe { id:155, name:"Botas Couro T3",    inputs:[(item_id::LEATHER_T3,3),(item_id::MINERAL_T3,1),(0,0),(0,0)], output_item_id:item_id::BOOTS_LEATHER, output_qty:1, output_item_level:40, roll_instance:true },
    CraftRecipe { id:156, name:"Botas Placa T3",    inputs:[(item_id::MINERAL_T3,4),(0,0),(0,0),(0,0)],                  output_item_id:item_id::BOOTS_PLATE,   output_qty:1, output_item_level:40, roll_instance:true },
    CraftRecipe { id:157, name:"Calças Couro T3",  inputs:[(item_id::LEATHER_T3,5),(item_id::MINERAL_T3,1),(0,0),(0,0)], output_item_id:item_id::LEGS_LEATHER,  output_qty:1, output_item_level:40, roll_instance:true },
    CraftRecipe { id:158, name:"Calças Placa T3",  inputs:[(item_id::MINERAL_T3,5),(item_id::LEATHER_T3,1),(0,0),(0,0)], output_item_id:item_id::LEGS_PLATE,    output_qty:1, output_item_level:40, roll_instance:true },

    // Armor T4 — ilvl 70.
    CraftRecipe { id:160, name:"Armadura Couro T4", inputs:[(item_id::LEATHER_T4,8),(item_id::MINERAL_T4,2),(0,0),(0,0)], output_item_id:item_id::LEATHER_ARMOR, output_qty:1, output_item_level:70, roll_instance:true },
    CraftRecipe { id:161, name:"Armadura Placa T4", inputs:[(item_id::MINERAL_T4,8),(item_id::LEATHER_T4,2),(0,0),(0,0)], output_item_id:item_id::PLATE_ARMOR,   output_qty:1, output_item_level:70, roll_instance:true },
    CraftRecipe { id:162, name:"Manto T4",          inputs:[(item_id::LEATHER_T4,5),(item_id::WOOD_T4,3),(0,0),(0,0)],    output_item_id:item_id::ROBE,          output_qty:1, output_item_level:70, roll_instance:true },
    CraftRecipe { id:163, name:"Capacete Couro T4", inputs:[(item_id::LEATHER_T4,4),(item_id::MINERAL_T4,1),(0,0),(0,0)], output_item_id:item_id::HELM_LEATHER,  output_qty:1, output_item_level:70, roll_instance:true },
    CraftRecipe { id:164, name:"Elmo Placa T4",     inputs:[(item_id::MINERAL_T4,4),(item_id::LEATHER_T4,1),(0,0),(0,0)], output_item_id:item_id::HELM_PLATE,    output_qty:1, output_item_level:70, roll_instance:true },
    CraftRecipe { id:165, name:"Botas Couro T4",    inputs:[(item_id::LEATHER_T4,3),(item_id::MINERAL_T4,1),(0,0),(0,0)], output_item_id:item_id::BOOTS_LEATHER, output_qty:1, output_item_level:70, roll_instance:true },
    CraftRecipe { id:166, name:"Botas Placa T4",    inputs:[(item_id::MINERAL_T4,4),(0,0),(0,0),(0,0)],                  output_item_id:item_id::BOOTS_PLATE,   output_qty:1, output_item_level:70, roll_instance:true },
    CraftRecipe { id:167, name:"Calças Couro T4",  inputs:[(item_id::LEATHER_T4,5),(item_id::MINERAL_T4,1),(0,0),(0,0)], output_item_id:item_id::LEGS_LEATHER,  output_qty:1, output_item_level:70, roll_instance:true },
    CraftRecipe { id:168, name:"Calças Placa T4",  inputs:[(item_id::MINERAL_T4,5),(item_id::LEATHER_T4,1),(0,0),(0,0)], output_item_id:item_id::LEGS_PLATE,    output_qty:1, output_item_level:70, roll_instance:true },

    // Boats (Naval) — embarcacoes craftaveis. Ids 200+ reservados pra naval.
    // Esquife (T1) — primeiro barco, barato: 40 madeira T1 + 20 couro T1.
    CraftRecipe { id:201, name:"Esquife",         inputs:[(item_id::WOOD_T1,40),(item_id::LEATHER_T1,20),(0,0),(0,0)], output_item_id:item_id::BOAT_ESQUIFE, output_qty:1, output_item_level:0, roll_instance:false },
    // Lylian Leutard (T2) usa: 100 madeira T1 + 100 madeira T2 + 100 mineral T2 + 100 monster heart T2.
    CraftRecipe { id:200, name:"Lylian Leutard",  inputs:[(item_id::WOOD_T1,100),(item_id::WOOD_T2,100),(item_id::MINERAL_T2,100),(item_id::LEATHER_T2,100)], output_item_id:item_id::BOAT_LYLIAN_LEUTARD, output_qty:1, output_item_level:0, roll_instance:false },
];

pub fn craft_recipe(id: u16) -> Option<&'static CraftRecipe> {
    CRAFT_RECIPES.iter().find(|r| r.id == id)
}

/// Estações de craft da praça. O cliente recebe `CraftRecipeNet.station` e
/// filtra as receitas pela estação que o player abriu.
pub mod craft_station {
    pub const FORGE:     u8 = 0; // armas pesadas/médias + armadura placa
    pub const ATELIER:   u8 = 1; // armas leves/mágicas + armadura couro/pano
    pub const SMELTER:   u8 = 2; // refino de mineral + couro(=heart)
    pub const CARPENTRY: u8 = 3; // refino de madeira + barcos
}

/// Deriva a estação de craft a partir do item de saída da receita
/// (sem coluna nova no DB — mesma ideia da categoria). Mantenha em sync
/// com o cliente se ele precisar derivar; hoje o cliente só LÊ o campo.
pub fn craft_station_of(output_item_id: u16) -> u8 {
    use item_id::*;
    match output_item_id {
        // Marcenaria: refino de madeira + barcos
        WOOD_T2 | WOOD_T3 | WOOD_T4 => craft_station::CARPENTRY,
        100..=109                   => craft_station::CARPENTRY,
        // Smelter: refino de mineral + couro(heart)
        MINERAL_T2 | MINERAL_T3 | MINERAL_T4
        | LEATHER_T2 | LEATHER_T3 | LEATHER_T4 => craft_station::SMELTER,
        // Ateliê: armas leves/mágicas + armadura couro/pano + acessórios de pano
        DAGGER | STAFF | WAND
        | LEATHER_ARMOR | ROBE
        | HELM_LEATHER | LEGS_LEATHER | BOOTS_LEATHER | GLOVES_LEATHER
        | BELT_BASIC | BELT_MAGIC | CAPE_BASIC | CAPE_MAGIC
        | NECKLACE_BASIC | NECKLACE_MAGIC => craft_station::ATELIER,
        // Forja: resto (armas marciais + armadura placa + escudos)
        _ => craft_station::FORGE,
    }
}

/// Mapeia item_id de barco pra boat_kind do EntityKind::Boat. Mantenha em
/// sync com o cliente (BoatRenderer escolhe sheets pelo kind).
pub fn boat_kind_of(id: u16) -> Option<u16> {
    match id {
        item_id::BOAT_LYLIAN_LEUTARD => Some(0), // 0 = Lylian Leutard (T2)
        item_id::BOAT_ESQUIFE        => Some(1), // 1 = Esquife (T1, 1 lugar)
        _ => None,
    }
}

// item_stack_max vive no DB (server crate::economy).

/// Codigo de animacao (`attack_anim::*`) que o cliente deve tocar quando o
/// player ataca com a arma indicada. Mantém em sync com
/// `ItemInfo.AttackAnimOf` no cliente C#.
pub fn weapon_attack_anim(weapon_id: u16) -> u8 {
    use crate::components::attack_anim::*;
    match weapon_id {
        id if id == item_id::BOW           => SHOOT,
        id if id == item_id::CROSSBOW      => SHOOT,
        id if id == item_id::ENHANCED_BOW  => SHOOT,
        id if id == item_id::STAFF         => THRUST,
        id if id == item_id::WAND          => THRUST,
        id if id == item_id::SPEAR         => THRUST,
        id if id == item_id::ENHANCED_STAFF => THRUST,
        id if id == item_id::ENHANCED_WAND  => THRUST,
        id if id == item_id::ENHANCED_SPEAR => THRUST,
        _ => SLASH,
    }
}

/// Retorna o slot de equipamento para um item_id, ou None se nao for
/// equipavel.
pub fn equip_slot_of(item_id: u16) -> Option<EquipSlot> {
    match item_id {
        id if id == item_id::SWORD
            || id == item_id::STAFF
            || id == item_id::DAGGER
            || id == item_id::GREAT_SWORD
            || id == item_id::BOW
            || id == item_id::WAND
            || id == item_id::SCIMITAR
            || id == item_id::AXE
            || id == item_id::SPEAR
            || id == item_id::CROSSBOW
            || id == item_id::ENHANCED_SWORD
            || id == item_id::VETERAN_SWORD
            || id == item_id::ENHANCED_BOW
            || id == item_id::ENHANCED_STAFF
            || id == item_id::ENHANCED_WAND
            || id == item_id::ENHANCED_AXE
            || id == item_id::ENHANCED_SPEAR => Some(EquipSlot::Weapon),
        id if id == item_id::SHIELD
            || id == item_id::HEAVY_SHIELD   => Some(EquipSlot::Offhand),
        id if id == item_id::ARMOR
            || id == item_id::LEATHER_ARMOR
            || id == item_id::PLATE_ARMOR
            || id == item_id::ROBE           => Some(EquipSlot::Armor),
        id if id == item_id::RING
            || id == item_id::LUCKY_RING     => Some(EquipSlot::Ring),
        // Fase E — Amulet/Pendant/Charm migram pra Necklace (slot dedicado)
        id if id == item_id::AMULET
            || id == item_id::PENDANT
            || id == item_id::CHARM
            || id == item_id::NECKLACE_BASIC
            || id == item_id::NECKLACE_MAGIC => Some(EquipSlot::Necklace),
        id if id == item_id::HELM_LEATHER
            || id == item_id::HELM_PLATE     => Some(EquipSlot::Helm),
        id if id == item_id::LEGS_LEATHER
            || id == item_id::LEGS_PLATE     => Some(EquipSlot::Legs),
        id if id == item_id::BOOTS_LEATHER
            || id == item_id::BOOTS_PLATE    => Some(EquipSlot::Boots),
        id if id == item_id::GLOVES_LEATHER
            || id == item_id::GLOVES_PLATE   => Some(EquipSlot::Gloves),
        id if id == item_id::BELT_BASIC
            || id == item_id::BELT_MAGIC     => Some(EquipSlot::Belt),
        id if id == item_id::CAPE_BASIC
            || id == item_id::CAPE_MAGIC     => Some(EquipSlot::Cape),
        // Tier T1-T4 (Phase G): faixas atribuídas em ordem por slot.
        101..=112 => Some(EquipSlot::Gloves),
        113..=124 => Some(EquipSlot::Legs),
        125..=136 => Some(EquipSlot::Boots),
        137..=148 => Some(EquipSlot::Helm),
        149..=160 => Some(EquipSlot::Armor),
        161..=172 => Some(EquipSlot::Ring),
        173..=184 => Some(EquipSlot::Necklace),
        185..=196 => Some(EquipSlot::Belt),
        197..=208 => Some(EquipSlot::Offhand),
        209..=220 => Some(EquipSlot::Cape),
        221..=268 => Some(EquipSlot::Weapon),
        _                                    => None,
    }
}

/// True se este item_id eh um escudo (vai no slot Offhand).
pub fn is_shield(item_id: u16) -> bool {
    item_id == item_id::SHIELD || item_id == item_id::HEAVY_SHIELD
        || (197..=208).contains(&item_id)
}

/// Quais armas permitem equipar um item no Offhand (escudo). One-handed melee
/// (sword, dagger) + unarmed (item_id=0). Two-handed (great_sword) e ranged
/// (bow/staff/wand) NAO permitem — o offhand fica trancado pelo server quando
/// uma dessas armas esta equipada.
pub fn weapon_allows_offhand(weapon_id: u16) -> bool {
    weapon_id == 0
        || weapon_id == item_id::SWORD
        || weapon_id == item_id::DAGGER
        || weapon_id == item_id::SCIMITAR
        || weapon_id == item_id::AXE
        || weapon_id == item_id::ENHANCED_SWORD
        || weapon_id == item_id::VETERAN_SWORD
        || weapon_id == item_id::ENHANCED_AXE
        // Phase G — armas T1-T4: Espada (221-224), Machado (237-240), Varinha (261-264) são 1H
        || (221..=224).contains(&weapon_id)
        || (237..=240).contains(&weapon_id)
        || (261..=264).contains(&weapon_id)
}

/// Nivel de proficiencia do CONJUNTO pra equipar este item.
///
/// `None` = sem requisito. O conjunto sai da propria arma, entao o requisito e'
/// so' o numero: quem pede espada aprimorada pede proficiencia 5 no conjunto
/// que aquela espada e'.
pub const fn item_prof_req(item_id: u16) -> Option<u16> {
    match item_id {
        id if id == item_id::ENHANCED_SWORD => Some(5),
        id if id == item_id::VETERAN_SWORD  => Some(10),
        id if id == item_id::ENHANCED_BOW   => Some(5),
        id if id == item_id::ENHANCED_STAFF => Some(5),
        id if id == item_id::ENHANCED_WAND  => Some(5),
        id if id == item_id::ENHANCED_AXE   => Some(5),
        id if id == item_id::ENHANCED_SPEAR => Some(5),
        _ => None,
    }
}

/// Char level mínimo pra equipar este item. Hardcore design removeu o gate de
/// char level — agora so' Proficiency level (`item_prof_req`) gateia equip.
/// Mantido como function (sempre None) pra preservar call sites no equip path.
pub const fn item_char_level_req(_item_id: u16) -> Option<u16> {
    None
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum EquipSlot {
    Weapon,
    Armor,
    Offhand,
    Ring,
    // Fase E
    Helm,
    Legs,
    Boots,
    Gloves,
    Belt,
    Cape,
    Necklace,
}

impl EquipSlot {
    /// String do slot pra ser persistido no DB (coluna `slot`).
    pub fn as_db_str(&self) -> &'static str {
        match self {
            EquipSlot::Weapon       => "weapon",
            EquipSlot::Armor        => "armor",
            EquipSlot::Offhand      => "offhand",
            EquipSlot::Ring         => "ring",
            EquipSlot::Helm         => "helm",
            EquipSlot::Legs         => "legs",
            EquipSlot::Boots        => "boots",
            EquipSlot::Gloves       => "gloves",
            EquipSlot::Belt         => "belt",
            EquipSlot::Cape         => "cape",
            EquipSlot::Necklace     => "necklace",
        }
    }
}

/// Bonus aplicado por um equipamento. Somado aos stats base.
#[derive(Debug, Clone, Copy, Default)]
pub struct EquipBonus {
    pub hp_max: i32,
    pub mp_max: i32,
    pub attack_damage: i32,
    pub dex: i32,
    pub wis: i32,
    pub defense: i32,
}

/// Pontos de atributo ganhos por level-up.
pub const POINTS_PER_LEVEL: u32 = 3;

/// Skill points ganhos por level-up. Pool global compartilhado entre profs.
pub const SP_PER_LEVEL: u32 = 1;

/// Rank máximo de uma skill (0 = não aprendida; 1..=10 = ranks).
pub const MAX_SKILL_RANK: u8 = 10;

/// Custo em SP de cada rank, indexado por rank (0 = unlock cost = 1, idx 1..10).
/// Total para maxar uma skill = 1+1+1+2+2+2+3+3+3+5 = **23 SP**.
pub const SP_COST_PER_RANK: [u32; 11] = [
    0, // rank 0 = não aprendida
    1, 1, 1,  // rank 1-3: cheap unlock + early
    2, 2, 2,  // rank 4-6: meio (rank 5 = milestone)
    3, 3, 3,  // rank 7-9
    5,        // rank 10 = capstone
];

/// Custo em SP pra subir de `current_rank` para `current_rank + 1`.
/// Retorna 0 se já está no max.
pub const fn sp_cost_for_next_rank(current_rank: u8) -> u32 {
    if current_rank >= MAX_SKILL_RANK { return 0; }
    SP_COST_PER_RANK[(current_rank + 1) as usize]
}

/// Custo total acumulado pra alcançar `rank` (somando rank 1..=rank).
pub const fn sp_cost_to_reach_rank(rank: u8) -> u32 {
    let mut sum = 0u32;
    let mut i = 1u8;
    while i <= rank && i <= MAX_SKILL_RANK {
        sum += SP_COST_PER_RANK[i as usize];
        i += 1;
    }
    sum
}

/// Slots de skill ativa equipados na barra do HUD (teclas 1..=N).
pub const SKILL_BAR_SLOTS: usize = 6;

/// Tiers de skill — define unlock_char_lvl e unlock_prof_lvl recomendados.
/// Skills no DB usam (char_lvl, prof_lvl) explícitos; estes são guidelines pro seed.
///
/// Filosofia: TODAS as skills devem estar acessíveis até char lvl ~20/prof ~25.
/// Após isso, progressão vira ranking de skills individuais (max rank desbloqueia
/// passives que aumentam o efeito daquela skill especifica).
pub const SKILL_TIER_UNLOCKS: [(u8, u8); 4] = [
    (2,  3),   // T1 — Aprendiz (acessível quase imediato)
    (6,  8),   // T2 — Adepto
    (12, 15),  // T3 — Mestre
    (20, 25),  // T4 — Lendário (cap onde TUDO ta unlocked)
];

/// Delta de char_lvl e prof_lvl exigido por rank acima do baseline da skill.
/// Rank N requer `unlock_X + (N-1) * delta_X`. Permite progressão escalonada:
/// rank 1 = unlock baseline; rank 10 = baseline + 9*delta.
///
/// Ex: T1 unlock 2/3 → rank 10 precisa de char 20 / prof 39.
/// Ex: T4 unlock 20/25 → rank 10 precisa de char 38 / prof 61.
pub const SKILL_RANK_CHAR_DELTA: u8 = 2;
pub const SKILL_RANK_PROF_DELTA: u8 = 4;

/// Calcula char_lvl/prof_lvl necessário pra atingir um rank específico de uma
/// skill. `target_rank` é 1-indexed (1 = aprender, 10 = max). Rank 1 retorna
/// o baseline; ranks maiores aplicam o delta por rank.
pub const fn skill_rank_requirements(
    unlock_char: u8, unlock_prof: u8, target_rank: u8,
) -> (u8, u8) {
    if target_rank <= 1 { return (unlock_char, unlock_prof); }
    let extra = target_rank - 1;
    let char_need = unlock_char.saturating_add(extra.saturating_mul(SKILL_RANK_CHAR_DELTA));
    let prof_need = unlock_prof.saturating_add(extra.saturating_mul(SKILL_RANK_PROF_DELTA));
    (char_need, prof_need)
}

/// Quantidade de stats alocaveis. Indices: 0=FOR, 1=DES, 2=INT, 3=VIT, 4=SPD, 5=RES.
pub const STAT_COUNT: usize = 6;

/// Aliases de indices pra deixar o codigo legivel.
pub mod stat_idx {
    pub const FOR: usize = 0;
    pub const DES: usize = 1;
    pub const INT: usize = 2;
    pub const VIT: usize = 3;
    pub const SPD: usize = 4;
    pub const RES: usize = 5;
}

/// Multiplicador de velocidade adicional por ponto em SPD (somado a 1.0).
/// Zerado: SPD nao escala mais movement speed (movement vira default
/// uniforme). SPD continua dando stamina/regen + dash CD reduction.
pub const MOVE_SPEED_PCT_PER_SPD: f32 = 0.0;
/// % redução de cooldown do dash por ponto em SPD. Final dash CD =
/// DASH_COOLDOWN / (1 + DASH_CD_REDUCTION_PER_SPD * spd_points).
pub const DASH_CD_REDUCTION_PER_SPD: f32 = 0.065; // +6.5%/ponto (100 SPD→0.2s, 200→~0.1s)

/// Chance de crit adicionada por ponto em DES (somada a 0.0).
pub const CRIT_CHANCE_PER_DES: f32 = 0.005; // +0.5% por ponto

/// Velocidade de ataque adicional por ponto em DES (somada a 1.0).
/// Aplicada como divisor no cooldown — 1.5 = ataques 50% mais rapidos.
pub const ATTACK_SPEED_PCT_PER_DES: f32 = 0.015; // +1.5% por ponto

/// Stamina maxima adicional por ponto em SPD (somada ao base 100).
pub const STAMINA_MAX_PER_SPD: i32 = 2;

/// Regen de stamina/seg adicional por ponto em SPD (somado ao base 25).
pub const STAMINA_REGEN_PER_SPD: f32 = 1.25; // regen escala forte c/ SPD → dash não fica gateado por stamina em builds de SPD

/// HP regenerado/seg adicionado por ponto em VIT.
pub const HP_REGEN_PER_VIT: f32 = 0.2;

/// Multiplicador de dano em hit critico.
pub const CRIT_DAMAGE_MULT: f32 = 1.5;

/// Defesa adicional por ponto em RES (somada ao base 0).
pub const DEFENSE_PER_RES: i32 = 1;

/// Aumento da fração de dano absorvido por block, por ponto em RES (somado
/// ao base `BLOCK_DAMAGE_REDUCTION_BASE = 0.6`). Ex.: +0.003 × 100 pontos =
/// +30% absorvido → 90% total. Capado em `BLOCK_REDUCTION_MAX`.
pub const BLOCK_REDUCTION_PER_RES: f32 = 0.003;

/// Reducao do multiplicador de custo de stamina por ponto em RES (subtraido
/// do base 1.0). Aplica em block E parry. Ex.: -0.005 × 100 = -50% → custos
/// caem pra 50%. Capado em `STAMINA_COST_MULT_MIN`.
pub const STAMINA_COST_REDUCTION_PER_RES: f32 = 0.005;

/// Custo BASE de stamina ao bloquear um ataque com sucesso. RES reduz via
/// `defense_stamina_cost_mult`.
pub const BLOCK_STAMINA_COST: f32 = 25.0;

/// Custo BASE de stamina ao executar um parry com sucesso. RES reduz via
/// `defense_stamina_cost_mult`.
pub const PARRY_STAMINA_COST: f32 = 15.0;

/// Fração base de dano absorvido por block (0.0 = sem efeito, 1.0 = anula tudo).
/// RES soma `BLOCK_REDUCTION_PER_RES` por ponto.
pub const BLOCK_DAMAGE_REDUCTION_BASE: f32 = 0.6;

/// Cap maximo de absorcao de dano por block (player nunca toma menos que
/// 5% do dano original quando bloqueia).
pub const BLOCK_REDUCTION_MAX: f32 = 0.95;

/// Cap minimo do multiplicador de custo de stamina (player nunca paga
/// menos que 50% do custo base de block/parry).
pub const STAMINA_COST_MULT_MIN: f32 = 0.5;

/// Janela em segundos durante a qual o atacante fica em stagger apos um parry.
pub const PARRY_STAGGER_S: f32 = 0.6;

/// Multiplicador de velocidade enquanto o player segura RMB (defesa ativa).
pub const MOVE_SPEED_DEFENDING_MULT: f32 = 0.4;

/// Janela em segundos apos um press de PRIMARY/SECONDARY pra contar como
/// tentativa de parry contra um hit incoming. ~250ms em 60fps = 15 frames.
pub const PARRY_WINDOW_S: f32 = 0.25;

/// Bonus aplicado por 1 ponto em cada stat (6 stats — design FOR/DES/INT/VIT/SPD/RES).
/// Indices alinhados com `stat_idx::*`.
///
/// FOR (Forca):        +1 atk, +2 hp_max
/// DES (Destreza):     +1 dex, +CRIT_CHANCE_PER_DES crit, +ATTACK_SPEED_PCT_PER_DES atk speed
/// INT (Inteligencia): +1 wis, +2 mp_max
/// VIT (Vitalidade):   +5 hp_max, +HP_REGEN_PER_VIT hp regen
/// SPD (Velocidade):   +MOVE_SPEED_PCT_PER_SPD move speed, +STAMINA_MAX_PER_SPD stamina, +STAMINA_REGEN_PER_SPD st regen
/// RES (Resistencia):  +DEFENSE_PER_RES def, +BLOCK_REDUCTION_PER_RES dmg absorvido em block,
///                     -STAMINA_COST_REDUCTION_PER_RES no custo de block/parry
pub const STAT_POINT_BONUS: [StatAllocBonus; STAT_COUNT] = [
    /* FOR */ StatAllocBonus { hp_max: 2, mp_max: 0, attack_damage: 1, dex: 0, wis: 0, defense: 0,                speed_pct: 0.0,                      crit_chance: 0.0,                  hp_regen: 0.0,               attack_speed_pct: 0.0,                       stamina_max: 0,                  stamina_regen: 0.0,                  block_reduction_bonus: 0.0,             stamina_cost_reduction: 0.0, dash_cd_reduction_pct: 0.0 },
    /* DES */ StatAllocBonus { hp_max: 0, mp_max: 0, attack_damage: 0, dex: 1, wis: 0, defense: 0,                speed_pct: 0.0,                      crit_chance: CRIT_CHANCE_PER_DES,  hp_regen: 0.0,               attack_speed_pct: ATTACK_SPEED_PCT_PER_DES,  stamina_max: 0,                  stamina_regen: 0.0,                  block_reduction_bonus: 0.0,             stamina_cost_reduction: 0.0, dash_cd_reduction_pct: 0.0 },
    /* INT */ StatAllocBonus { hp_max: 0, mp_max: 2, attack_damage: 0, dex: 0, wis: 1, defense: 0,                speed_pct: 0.0,                      crit_chance: 0.0,                  hp_regen: 0.0,               attack_speed_pct: 0.0,                       stamina_max: 0,                  stamina_regen: 0.0,                  block_reduction_bonus: 0.0,             stamina_cost_reduction: 0.0, dash_cd_reduction_pct: 0.0 },
    /* VIT */ StatAllocBonus { hp_max: 5, mp_max: 0, attack_damage: 0, dex: 0, wis: 0, defense: 0,                speed_pct: 0.0,                      crit_chance: 0.0,                  hp_regen: HP_REGEN_PER_VIT,  attack_speed_pct: 0.0,                       stamina_max: 0,                  stamina_regen: 0.0,                  block_reduction_bonus: 0.0,             stamina_cost_reduction: 0.0, dash_cd_reduction_pct: 0.0 },
    /* SPD */ StatAllocBonus { hp_max: 0, mp_max: 0, attack_damage: 0, dex: 0, wis: 0, defense: 0,                speed_pct: 0.0,                      crit_chance: 0.0,                  hp_regen: 0.0,               attack_speed_pct: 0.0,                       stamina_max: STAMINA_MAX_PER_SPD,stamina_regen: STAMINA_REGEN_PER_SPD,    block_reduction_bonus: 0.0,             stamina_cost_reduction: 0.0, dash_cd_reduction_pct: DASH_CD_REDUCTION_PER_SPD },
    /* RES */ StatAllocBonus { hp_max: 0, mp_max: 0, attack_damage: 0, dex: 0, wis: 0, defense: DEFENSE_PER_RES,  speed_pct: 0.0,                      crit_chance: 0.0,                  hp_regen: 0.0,               attack_speed_pct: 0.0,                       stamina_max: 0,                  stamina_regen: 0.0,                  block_reduction_bonus: BLOCK_REDUCTION_PER_RES, stamina_cost_reduction: STAMINA_COST_REDUCTION_PER_RES, dash_cd_reduction_pct: 0.0 },
];

/// Bonus de alocacao de pontos. Difere de `EquipBonus` por incluir
/// modificadores de velocidade / crit / regen / atk speed — campos que ainda
/// nao sao expostos em equipamento (pode-se unificar futuramente).
#[derive(Debug, Clone, Copy, Default)]
pub struct StatAllocBonus {
    pub hp_max: i32,
    pub mp_max: i32,
    pub attack_damage: i32,
    pub dex: i32,
    pub wis: i32,
    pub defense: i32,
    pub speed_pct: f32,
    pub crit_chance: f32,
    pub hp_regen: f32,
    pub attack_speed_pct: f32,
    pub stamina_max: i32,
    pub stamina_regen: f32,
    /// Adiciona ao base BLOCK_DAMAGE_REDUCTION_BASE — fração extra absorvida.
    pub block_reduction_bonus: f32,
    /// Subtrai do multiplicador de custo de stamina (1.0 = base).
    pub stamina_cost_reduction: f32,
    /// Adiciona ao bonus % de redução de cooldown do dash. Final mult =
    /// 1.0 + dash_cd_reduction_pct (clampado em [1, X]). Cooldown final =
    /// DASH_COOLDOWN / mult.
    pub dash_cd_reduction_pct: f32,
}

/// Escalamento por level de proficiencia, aplicado quando a arma correspondente
/// esta equipada. Tudo em f32 e truncado depois de multiplicar pelo level.
#[derive(Debug, Clone, Copy, Default)]
pub struct WeaponScaling {
    pub hp_max: f32,
    pub mp_max: f32,
    pub attack_damage: f32,
    pub dex: f32,
    pub wis: f32,
    pub defense: f32,
}

/// Scaling por arma equipada. level vem da Proficiency::from_item(weapon_id).
/// Unarmed (sem arma): usa `unarmed_scaling()`.
pub const fn weapon_scaling(item_id: u16) -> WeaponScaling {
    match item_id {
        // Espada (sword & board): tanque — +HP, +Res
        id if id == item_id::SWORD
            || id == item_id::ENHANCED_SWORD
            || id == item_id::VETERAN_SWORD => WeaponScaling {
            hp_max: 1.0, mp_max: 0.0, attack_damage: 0.0, dex: 0.0, wis: 0.0, defense: 0.1,
        },
        // Espadao: DPS puro
        id if id == item_id::GREAT_SWORD => WeaponScaling {
            hp_max: 0.0, mp_max: 0.0, attack_damage: 0.5, dex: 0.0, wis: 0.0, defense: 0.0,
        },
        // Adaga: duelista
        id if id == item_id::DAGGER => WeaponScaling {
            hp_max: 0.0, mp_max: 0.0, attack_damage: 0.3, dex: 0.2, wis: 0.0, defense: 0.0,
        },
        // Cajado: caster hibrido
        id if id == item_id::STAFF
            || id == item_id::ENHANCED_STAFF => WeaponScaling {
            hp_max: 0.0, mp_max: 1.0, attack_damage: 0.0, dex: 0.0, wis: 0.2, defense: 0.0,
        },
        // Arco: ranger
        id if id == item_id::BOW
            || id == item_id::ENHANCED_BOW => WeaponScaling {
            hp_max: 0.0, mp_max: 0.0, attack_damage: 0.1, dex: 0.5, wis: 0.0, defense: 0.0,
        },
        // Varinha: caster puro
        id if id == item_id::WAND
            || id == item_id::ENHANCED_WAND => WeaponScaling {
            hp_max: 0.0, mp_max: 1.0, attack_damage: 0.0, dex: 0.0, wis: 0.3, defense: 0.0,
        },
        // Machado: heavy hitter — atk alto + um pouco de hp
        id if id == item_id::AXE
            || id == item_id::ENHANCED_AXE => WeaponScaling {
            hp_max: 0.5, mp_max: 0.0, attack_damage: 0.4, dex: 0.0, wis: 0.0, defense: 0.0,
        },
        // Lança: zoning — atk médio + dex (alcance)
        id if id == item_id::SPEAR
            || id == item_id::ENHANCED_SPEAR => WeaponScaling {
            hp_max: 0.0, mp_max: 0.0, attack_damage: 0.25, dex: 0.15, wis: 0.0, defense: 0.0,
        },
        // Phase G — T1-T4 (mesmo scaling do equivalente base)
        id if id >= 221 && id <= 224 => WeaponScaling { // Espada T1-T4
            hp_max: 1.0, mp_max: 0.0, attack_damage: 0.0, dex: 0.0, wis: 0.0, defense: 0.1,
        },
        id if id >= 225 && id <= 228 => WeaponScaling { // Espadão T1-T4
            hp_max: 0.0, mp_max: 0.0, attack_damage: 0.5, dex: 0.0, wis: 0.0, defense: 0.0,
        },
        id if id >= 237 && id <= 240 => WeaponScaling { // Machado T1-T4
            hp_max: 0.5, mp_max: 0.0, attack_damage: 0.4, dex: 0.0, wis: 0.0, defense: 0.0,
        },
        id if id >= 241 && id <= 244 => WeaponScaling { // Lança T1-T4
            hp_max: 0.0, mp_max: 0.0, attack_damage: 0.25, dex: 0.15, wis: 0.0, defense: 0.0,
        },
        id if id >= 257 && id <= 260 => WeaponScaling { // Cajado T1-T4
            hp_max: 0.0, mp_max: 1.0, attack_damage: 0.0, dex: 0.0, wis: 0.2, defense: 0.0,
        },
        id if id >= 261 && id <= 264 => WeaponScaling { // Varinha T1-T4
            hp_max: 0.0, mp_max: 1.0, attack_damage: 0.0, dex: 0.0, wis: 0.3, defense: 0.0,
        },
        id if id >= 265 && id <= 268 => WeaponScaling { // Arco T1-T4
            hp_max: 0.0, mp_max: 0.0, attack_damage: 0.1, dex: 0.5, wis: 0.0, defense: 0.0,
        },
        _ => WeaponScaling {
            hp_max: 0.0, mp_max: 0.0, attack_damage: 0.0, dex: 0.0, wis: 0.0, defense: 0.0,
        },
    }
}

/// Scaling quando sem arma (Unarmed). Aplicado com Proficiency::Unarmed level.
pub const fn unarmed_scaling() -> WeaponScaling {
    WeaponScaling {
        hp_max: 0.0, mp_max: 0.0, attack_damage: 0.2, dex: 0.0, wis: 0.0, defense: 0.0,
    }
}

/// True se a arma e de corpo-a-corpo (gera dano em cone na frente ao atacar,
/// nao projetil). Sem arma = melee (soco). BOW/WAND/STAFF disparam projetil.
pub const fn weapon_is_melee(item_id: u16) -> bool {
    item_id == 0
        || item_id == item_id::SWORD
        || item_id == item_id::GREAT_SWORD
        || item_id == item_id::DAGGER
        || item_id == item_id::SCIMITAR
        || item_id == item_id::AXE
        || item_id == item_id::ENHANCED_SWORD
        || item_id == item_id::VETERAN_SWORD
        || item_id == item_id::ENHANCED_AXE
        // SPEAR e melee mas usa anim Thrust — atualmente damage gen e
        // controlado pela melee path baseado em is_melee, então mantém
        // como melee aqui (cone na frente).
        || item_id == item_id::SPEAR
        || item_id == item_id::ENHANCED_SPEAR
        // Phase G T1-T4 melee: Espada (221-224), Espadão (225-228), Machado (237-240), Lança (241-244)
        || matches!(item_id, 221..=228 | 237..=244)
}

/// True se o inimigo desse kind ataca em melee (cone de dano direto na frente)
/// ao inves de spawnar projetil. Kinds sem kite_dist são melee:
/// 0=Grunt, 1=Tank, 3=Ninja, 5=Berserker. Ranger/Mago/Arqueiro/Boss = ranged.
pub const fn enemy_is_melee(kind: u16) -> bool {
    matches!(kind, 0 | 1 | 3 | 5)
}

/// Raio do golpe melee em tiles.
pub const MELEE_RANGE: f32 = 1.8;

/// Alcance do auto-ataque com arma a distancia, em tiles.
///
/// No combate por target o servidor decide sozinho quando bater, entao ele
/// precisa de um alcance explicito — antes quem decidia era o jogador,
/// mirando. 9 tiles e' o mesmo alcance que o Ranger inimigo ja usa, pra
/// player e mob brigarem em pe de igualdade.
pub const RANGED_ATTACK_RANGE: f32 = 9.0;
/// Meio-angulo do cone em radianos (cone total = 2x).
pub const MELEE_CONE_HALF_ANGLE: f32 = std::f32::consts::FRAC_PI_3; // 60 graus -> 120 total

pub const fn item_bonus(item_id: u16) -> EquipBonus {
    match item_id {
        // Armas
        id if id == item_id::SWORD        => EquipBonus { hp_max:  0,  mp_max:   0, attack_damage: 10, dex: 0,  wis: 0, defense: 0 },
        id if id == item_id::STAFF        => EquipBonus { hp_max:  0,  mp_max:  40, attack_damage: 20, dex: 0,  wis: 5, defense: 0 },
        id if id == item_id::DAGGER       => EquipBonus { hp_max:  0,  mp_max:   0, attack_damage:  7, dex: 10, wis: 0, defense: 0 },
        id if id == item_id::GREAT_SWORD  => EquipBonus { hp_max:  0,  mp_max: -20, attack_damage: 28, dex: -3, wis: 0, defense: 0 },
        id if id == item_id::BOW          => EquipBonus { hp_max:  0,  mp_max:   0, attack_damage: 14, dex: 12, wis: 0, defense: 0 },
        id if id == item_id::WAND         => EquipBonus { hp_max:  0,  mp_max:  80, attack_damage:  6, dex: 0,  wis: 8, defense: 0 },
        // Armaduras
        id if id == item_id::ARMOR        => EquipBonus { hp_max: 40,  mp_max:   0, attack_damage:  0, dex: 0,  wis: 0, defense:  5 },
        id if id == item_id::SHIELD       => EquipBonus { hp_max: 75,  mp_max:   0, attack_damage: -7, dex: 0,  wis: 0, defense:  6 },
        id if id == item_id::LEATHER_ARMOR=> EquipBonus { hp_max: 25,  mp_max:   0, attack_damage:  0, dex: 6,  wis: 0, defense:  3 },
        id if id == item_id::PLATE_ARMOR  => EquipBonus { hp_max:120,  mp_max: -10, attack_damage: -8, dex: -5, wis: 0, defense:  9 },
        id if id == item_id::ROBE         => EquipBonus { hp_max: 10,  mp_max:  60, attack_damage:  0, dex: 0,  wis: 8, defense:  2 },
        // Acessorios
        id if id == item_id::RING         => EquipBonus { hp_max:  0,  mp_max:   0, attack_damage:  0, dex: 5,  wis: 3, defense: 0 },
        id if id == item_id::AMULET       => EquipBonus { hp_max: 15,  mp_max:  30, attack_damage:  0, dex: 0,  wis: 8, defense: 1 },
        id if id == item_id::LUCKY_RING   => EquipBonus { hp_max: 10,  mp_max:  20, attack_damage:  2, dex: 6,  wis: 2, defense: 0 },
        id if id == item_id::RING_TIDE    => EquipBonus { hp_max:  0,  mp_max:  15, attack_damage:  0, dex: 3,  wis: 0, defense: 0 },
        id if id == item_id::RING_IGNITION => EquipBonus { hp_max:  0,  mp_max:   0, attack_damage:  8, dex: 0,  wis: 5, defense: 0 },
        id if id == item_id::RING_MIST    => EquipBonus { hp_max: 80,  mp_max:   0, attack_damage:  0, dex: 8,  wis: 8, defense: 0 },
        id if id == item_id::RING_TEMPEST => EquipBonus { hp_max:150,  mp_max:  80, attack_damage: 10, dex:10,  wis:10, defense: 5 },
        // === Fase D — novas armas / armaduras / acessórios ===
        id if id == item_id::SCIMITAR     => EquipBonus { hp_max:  0,  mp_max:   0, attack_damage:  8, dex: 6,  wis: 0, defense: 0 },
        id if id == item_id::AXE       => EquipBonus { hp_max: 20,  mp_max:   0, attack_damage: 22, dex: -2, wis: 0, defense: 3 },
        id if id == item_id::SPEAR        => EquipBonus { hp_max:  0,  mp_max:   0, attack_damage: 16, dex: 5,  wis: 0, defense: 0 },
        id if id == item_id::CROSSBOW     => EquipBonus { hp_max:  0,  mp_max:   0, attack_damage: 18, dex: 8,  wis: 0, defense: 0 },
        id if id == item_id::HEAVY_SHIELD => EquipBonus { hp_max: 110, mp_max:   0, attack_damage:-12, dex: -5, wis: 0, defense: 10 },
        id if id == item_id::PENDANT      => EquipBonus { hp_max: 25,  mp_max:  35, attack_damage:  0, dex: 0,  wis: 4, defense: 1 },
        id if id == item_id::CHARM        => EquipBonus { hp_max:  0,  mp_max:  10, attack_damage:  3, dex: 4,  wis: 4, defense: 0 },
        // === Fase E — slots novos ===
        id if id == item_id::HELM_LEATHER => EquipBonus { hp_max: 15,  mp_max:   0, attack_damage:  0, dex: 4,  wis: 0, defense:  3 },
        id if id == item_id::HELM_PLATE   => EquipBonus { hp_max: 45,  mp_max:   0, attack_damage: -2, dex: -3, wis: 0, defense:  5 },
        id if id == item_id::LEGS_LEATHER => EquipBonus { hp_max: 20,  mp_max:   0, attack_damage:  0, dex: 5,  wis: 0, defense:  3 },
        id if id == item_id::LEGS_PLATE   => EquipBonus { hp_max: 60,  mp_max:   0, attack_damage: -3, dex: -4, wis: 0, defense:  6 },
        id if id == item_id::BOOTS_LEATHER=> EquipBonus { hp_max: 10,  mp_max:   0, attack_damage:  0, dex: 6,  wis: 0, defense:  2 },
        id if id == item_id::BOOTS_PLATE  => EquipBonus { hp_max: 30,  mp_max:   0, attack_damage: -1, dex: -3, wis: 0, defense:  3 },
        id if id == item_id::GLOVES_LEATHER=>EquipBonus { hp_max:  5,  mp_max:   0, attack_damage:  3, dex: 5,  wis: 0, defense:  1 },
        id if id == item_id::GLOVES_PLATE => EquipBonus { hp_max: 20,  mp_max:   0, attack_damage:  4, dex: -2, wis: 0, defense:  3 },
        id if id == item_id::BELT_BASIC   => EquipBonus { hp_max: 25,  mp_max:   0, attack_damage:  0, dex: 0,  wis: 0, defense:  2 },
        id if id == item_id::BELT_MAGIC   => EquipBonus { hp_max:  5,  mp_max:  35, attack_damage:  0, dex: 0,  wis: 4, defense:  1 },
        id if id == item_id::CAPE_BASIC   => EquipBonus { hp_max: 20,  mp_max:   0, attack_damage:  0, dex: 0,  wis: 0, defense:  4 },
        id if id == item_id::CAPE_MAGIC   => EquipBonus { hp_max:  0,  mp_max:  45, attack_damage:  0, dex: 0,  wis: 6, defense:  2 },
        id if id == item_id::NECKLACE_BASIC=>EquipBonus { hp_max: 20,  mp_max:  10, attack_damage:  0, dex: 0,  wis: 3, defense:  0 },
        id if id == item_id::NECKLACE_MAGIC=>EquipBonus { hp_max:  0,  mp_max:  40, attack_damage:  0, dex: 0,  wis: 7, defense:  0 },
        // === Fase F — armas tier 2/3 ===
        id if id == item_id::ENHANCED_SWORD=>EquipBonus { hp_max: 10,  mp_max:   0, attack_damage: 18, dex: 2,  wis: 0, defense:  1 },
        id if id == item_id::VETERAN_SWORD =>EquipBonus { hp_max: 20,  mp_max:   0, attack_damage: 28, dex: 4,  wis: 0, defense:  2 },
        id if id == item_id::ENHANCED_BOW  =>EquipBonus { hp_max:  0,  mp_max:   0, attack_damage: 26, dex:24,  wis: 0, defense:  0 },
        id if id == item_id::ENHANCED_STAFF=>EquipBonus { hp_max:  0,  mp_max:  80, attack_damage: 36, dex: 0,  wis:10, defense:  0 },
        id if id == item_id::ENHANCED_WAND =>EquipBonus { hp_max:  0,  mp_max: 150, attack_damage: 12, dex: 0,  wis:16, defense:  0 },
        id if id == item_id::ENHANCED_AXE  =>EquipBonus { hp_max: 35,  mp_max:   0, attack_damage: 40, dex:-2,  wis: 0, defense:  6 },
        id if id == item_id::ENHANCED_SPEAR=>EquipBonus { hp_max:  0,  mp_max:   0, attack_damage: 30, dex:10,  wis: 0, defense:  0 },
        _                                 => EquipBonus { hp_max:  0,  mp_max:   0, attack_damage:  0, dex: 0,  wis: 0, defense: 0 },
    }
}

/// Quanto HP uma pocao de vida restaura.
pub const HEALTH_POTION_HEAL: i32 = 50;

/// Regen de MP por segundo (inteiro).
pub const MP_REGEN_PER_SEC: f32 = 4.0;

/// Custo em MP da habilidade secundaria (triple-shot).
pub const SECONDARY_MP_COST: i32 = 25;

/// Cooldown entre ataques secundarios em segundos.
pub const SECONDARY_COOLDOWN: f32 = 0.5;

/// Quantidade de projeteis disparados pela habilidade secundaria e abertura
/// angular entre o primeiro e o ultimo (radianos).
pub const SECONDARY_PROJ_COUNT: i32 = 3;
pub const SECONDARY_SPREAD_RAD: f32 = 0.35; // ~20 graus

/// Raio em tiles pra interagir com NPC vendedor.
pub const INTERACT_RADIUS: f32 = 3.0;

// Loja e preços de venda vivem no DB (server crate::economy).

/// IDs logicos de tile — usados no WorldMap e no TileDef lookup.
pub mod tile_id {
    pub const FLOOR: u16 = 1;
    pub const WALL:  u16 = 2;
    pub const DIRT:  u16 = 3;
    pub const WATER: u16 = 4;
    /// Piso de dungeon — visualmente distinto, colisoes iguais a FLOOR.
    pub const DUNGEON_FLOOR: u16 = 5;
    pub const WOOD:  u16 = 5;
}

// ── Coleta ────────────────────────────────────────────────────────────────
//
// Nao ha' ferramenta, nivel nem proficiencia: qualquer um coleta qualquer
// coisa. O que decide o ganho e' O LUGAR — quantas pedras vivas ha' em volta
// de quem esta' parado ali. Como cada pedra tem um numero FINITO de coletas,
// o veio esvazia enquanto e' explorado e volta quando descansa; dois
// jogadores no mesmo veio esvaziam as mesmas pedras e cada um leva metade,
// sem nenhuma regra escrita a mao pra dividir. A disputa e' pelo spot.

/// Raio (em unidades de mundo) do "spot": o que conta como estar na mina.
/// 6 unidades = 12 blocos, mais que a maior clareira e menos que a ilha.
pub const COLETA_RAIO_SPOT: f32 = 6.0;

/// Numerador do intervalo entre coletas: `intervalo = BASE / vivos`.
///
/// Dividir pela densidade e' o modelo inteiro em uma linha. Um veio de 8
/// pedras rende uma coleta a cada 1,5 s enquanto esta' cheio; sobrando duas,
/// cai pra 6 s. Nao existe bonus por ficar parado: existe bonus por estar num
/// lugar melhor.
pub const COLETA_INTERVALO_BASE_S: f32 = 12.0;

/// Piso do intervalo. Impede que um veio grande vire torneira.
pub const COLETA_INTERVALO_MIN_S: f32 = 1.0;

/// Quantas coletas uma pedra rende antes de acabar, por tier
/// (1 cinza, 2 verde, 3 azul, 4 roxo).
///
/// E' a segunda metade do valor de um tier: a pedra roxa nao da' material
/// roxo, ela da' MUITO mais material — 128 coletas contra 14 da cinza. Subir
/// a montanha compra tempo de coleta, nao um item novo.
pub const COLETAS_POR_PEDRA: [u32; 5] = [0, 14, 24, 64, 128];

/// Quanto uma pedra esgotada demora pra voltar, por tier.
///
/// Sobe com o tier pelo mesmo motivo que a contagem sobe: a pedra roxa e'
/// destino, e destino que se repoe em dois minutos deixa de ser destino.
///
/// E' este numero que faz o veio ESVAZIAR de verdade. Com 300 s, um veio de
/// 24 pedras cinza se acomoda em ~9 vivas: some dois tercos do que estava la'
/// quando o jogador chegou, e ele ve' isso acontecer.
pub const RESPAWN_DA_PEDRA: [f32; 5] = [0.0, 300.0, 420.0, 600.0, 900.0];

/// Quantas coletas um tronco rende, e em quanto tempo volta.
///
/// PROVISORIO: a arvore ainda nao tem o desenho que a pedra tem (tier por
/// regiao da mata, contagem propria). Ela roda na mesma maquina com numeros
/// de marcador pra madeira nao sumir do jogo enquanto isso.
pub const COLETAS_POR_ARVORE: u32 = 8;
pub const RESPAWN_DA_ARVORE: f32 = 90.0;

/// O que uma pedra de cada tier ENTREGA, em peso por tier de material.
///
/// A escada e' a mesma em toda linha: o material cinza e' sempre a maioria, e
/// cada tier acrescenta um pouco do proprio e um pouco mais do anterior. A
/// pedra roxa NAO da' material roxo — ela da' mais azul que a azul, e paga a
/// diferenca em tempo de coleta (`COLETAS_POR_PEDRA`).
///
/// Nao ha' material laranja: sao quatro cores e o teto e' o roxo.
pub const RENDIMENTO_DA_PEDRA: [[u16; 4]; 5] = [
    [  0,  0,  0, 0],
    [100,  0,  0, 0], // cinza: so' cinza
    [ 80, 20,  0, 0], // verde: verde, com muito mais cinza
    [ 65, 25, 10, 0], // azul: cinza ainda manda, mais verde que a anterior, um pouco de azul
    [ 55, 27, 18, 0], // roxo: mesma escada, sem roxo, um pouquinho mais de azul
];

/// Tier do MATERIAL que sai desta pedra neste sorteio. `f` e' 0..1.
pub fn tier_do_rendimento(tier_da_pedra: u8, f: f32) -> u8 {
    let pesos = RENDIMENTO_DA_PEDRA[(tier_da_pedra as usize).min(4)];
    let total: u16 = pesos.iter().sum();
    if total == 0 { return 1 }
    let mut alvo = (f.clamp(0.0, 0.999) * total as f32) as u16;
    for (i, p) in pesos.iter().enumerate() {
        if alvo < *p { return i as u8 + 1 }
        alvo -= *p;
    }
    1
}

/// Tempo de respawn de um no' de coleta posto a mao num mapa de arquivo.
pub const FARM_NODE_RESPAWN_S: f32 = 30.0;

// ── Pesca (peixes do oceano) ──────────────────────────────────────────────
/// População alvo de peixes nadando perto de CADA player logado. O servidor
/// mantém ~este número dentro do raio de spawn.
pub const FISH_PER_PLAYER: usize = 8;
/// Raio (tiles) ao redor do player onde peixes são mantidos vivos. Peixes
/// que saem além de FISH_CULL_RADIUS de todos os players são despawnados.
pub const FISH_VIEW_RADIUS: f32 = 20.0;
pub const FISH_CULL_RADIUS: f32 = 30.0;
/// Velocidade de natação normal do peixe (tiles/s) — mais lento que player.
pub const FISH_SWIM_SPEED: f32 = 1.3;
/// Distância máxima player → ponto da boia pra validar um FishingCast (tiles).
pub const FISH_CAST_MAX_RANGE: f32 = 14.0;
/// Raio (tiles) ao redor da boia em que peixes sentem a atração leve.
pub const FISH_ATTRACT_RADIUS: f32 = 6.0;
/// Força da atração da boia (fração da velocidade direcionada à boia). Baixo
/// = "atração leve" — o peixe tende à boia mas mantém seu wander/sorte.
pub const FISH_ATTRACT_STRENGTH: f32 = 0.45;
/// Distância (tiles) peixe ↔ boia pra considerar fisgado (encostou na boia).
pub const FISH_HOOK_RADIUS: f32 = 0.7;


#[cfg(test)]
mod testes_coleta {
    use super::*;

    /// A escada de rendimento tem que obedecer, linha por linha, o que foi
    /// pedido: cinza sempre a maioria, cada tier acrescenta um pouco do
    /// proprio e um pouco mais do anterior, e a pedra roxa NAO da' roxo.
    #[test]
    fn a_escada_de_rendimento_obedece_o_desenho() {
        let p = RENDIMENTO_DA_PEDRA;
        for t in 1..=4usize {
            assert_eq!(p[t].iter().sum::<u16>(), 100, "tier {t} nao soma 100");
            let maior = *p[t].iter().max().unwrap();
            assert_eq!(p[t][0], maior, "tier {t}: cinza tem que ser a maioria");
        }
        assert_eq!(p[1], [100, 0, 0, 0], "pedra cinza so' da' cinza");
        assert_eq!(p[4][3], 0, "pedra roxa NAO da' material roxo");
        for t in 2..=4usize {
            assert!(p[t][1] > p[t - 1][1], "tier {t}: verde tem que subir");
        }
        assert!(p[4][2] > p[3][2], "roxo tem que dar mais azul que o azul");
        assert!(p[4][0] < p[3][0], "roxo tem que dar menos cinza que o azul");
    }

    /// O sorteio tem que devolver a mesma proporcao que a tabela declara.
    #[test]
    fn o_sorteio_bate_com_a_tabela() {
        for tier in 1..=4u8 {
            let mut conta = [0u32; 5];
            const N: u32 = 100_000;
            for k in 0..N {
                conta[tier_do_rendimento(tier, k as f32 / N as f32) as usize] += 1;
            }
            for m in 1..=4usize {
                let esperado = RENDIMENTO_DA_PEDRA[tier as usize][m - 1] as f32 / 100.0;
                let obtido = conta[m] as f32 / N as f32;
                assert!((obtido - esperado).abs() < 0.01,
                    "pedra t{tier} material t{m}: {obtido:.3} != {esperado:.3}");
            }
        }
    }

    /// A pedra melhor tem que render MAIS, e o que ela rende a mais e' TEMPO.
    #[test]
    fn a_pedra_melhor_rende_mais_tempo() {
        for t in 2..=4usize {
            assert!(COLETAS_POR_PEDRA[t] > COLETAS_POR_PEDRA[t - 1]);
            assert!(RESPAWN_DA_PEDRA[t] > RESPAWN_DA_PEDRA[t - 1]);
        }
        assert_eq!(COLETAS_POR_PEDRA[1..], [14, 24, 64, 128]);
    }
}
