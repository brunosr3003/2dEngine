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
pub const PROTOCOL_VERSION: u16 = 131;

/// Pocao de Experiencia: +30% de XP de personagem por uma hora de tempo real.
/// Usar outra com o bonus ativo RENOVA a hora cheia — nao acumula porcentagem.
pub const BONUS_XP_PCT: u64 = 30;
pub const DURACAO_BONUS_XP_S: i64 = 3600;

/// XP com o bonus aplicado, se ele estiver ativo em `agora` (unix secs).
pub fn xp_com_bonus(amount: u64, agora: i64, bonus_ate: i64) -> u64 {
    if agora < bonus_ate {
        amount.saturating_add(amount * BONUS_XP_PCT / 100)
    } else {
        amount
    }
}

/// Ate' quando o bonus vale depois de beber uma pocao agora.
pub fn renovar_bonus_xp(agora: i64) -> i64 {
    agora + DURACAO_BONUS_XP_S
}

/// Pocao de Fortuna: +30% do ouro e do cobre que cai de BICHO, por uma hora.
/// Mesma regra da de XP: beber outra renova a hora cheia, nao acumula.
pub const BONUS_FORTUNA_PCT: u64 = 30;
/// Pocao de Sorte: +20% na chance de cada linha de drop que nao e' garantida
/// (bicho e coleta), por uma hora.
pub const BONUS_SORTE_PCT: u32 = 20;
/// Pocoes de buff de drop duram uma hora.
pub const DURACAO_BUFF_S: i64 = 3600;

/// Quantidade de ouro/cobre com a Fortuna aplicada, se ativa em `agora`.
pub fn qtd_com_fortuna(qtd: u32, agora: i64, ate: i64) -> u32 {
    if agora < ate {
        qtd.saturating_add(((qtd as u64 * BONUS_FORTUNA_PCT + 50) / 100) as u32)
    } else {
        qtd
    }
}

/// Multiplicador da chance de drop com a Sorte (1,0 sem ela).
pub fn mult_de_sorte(agora: i64, ate: i64) -> f32 {
    if agora < ate {
        1.0 + BONUS_SORTE_PCT as f32 / 100.0
    } else {
        1.0
    }
}

/// Ate' quando um buff de drop vale depois de beber agora (renova, nao soma).
pub fn renovar_buff(agora: i64) -> i64 {
    agora + DURACAO_BUFF_S
}

#[cfg(test)]
mod testes_de_buff {
    use super::*;

    #[test]
    fn fortuna_e_sorte_so_valem_com_o_buff_ativo() {
        assert_eq!(qtd_com_fortuna(100, 10, 20), 130);
        assert_eq!(qtd_com_fortuna(100, 20, 20), 100, "expirou");
        assert_eq!(qtd_com_fortuna(0, 10, 20), 0);
        assert!((mult_de_sorte(10, 20) - 1.2).abs() < 1e-6);
        assert_eq!(mult_de_sorte(30, 20), 1.0);
        assert_eq!(
            renovar_buff(100),
            100 + DURACAO_BUFF_S,
            "renova a hora cheia, sem acumular"
        );
    }
}

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

/// Cooldown entre golpes CORPO A CORPO em segundos.
///
/// Era 0,25 (quatro golpes por segundo): o dobro do tiro, e o que fazia todo
/// mob morrer em dois golpes. O corpo a corpo segue mais rapido que a
/// distancia (0,55 / 0,32), mas nao o bastante pra apagar a luta. Medido em
/// `server::balanceamento`.
pub const ATTACK_COOLDOWN: f32 = 0.40;

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
pub const ATTACK_STAMINA_COST: f32 = 0.0; // basic attack sem custo (hardcore: stamina so' pra dash + sprint)

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
        if lvl >= 100 {
            break;
        }
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
        if lvl >= CHAR_LEVEL_CAP {
            return CHAR_LEVEL_CAP;
        }
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
pub const fn level_of_xp(xp: u64) -> u32 {
    level_of_xp_with_mult(xp, DEFAULT_XP_MULTIPLIER)
}
pub const fn xp_for_level(level: u32) -> u64 {
    xp_for_level_with_mult(level, DEFAULT_XP_MULTIPLIER)
}

/// Quantidade de inimigos gerados no inicio.
pub const ENEMY_START_COUNT: usize = 24;

/// Numero de slots do inventario do jogador.
pub const INVENTORY_SLOTS: usize = 40;

/// Raio em tiles pra coletar um loot.
pub const PICKUP_RADIUS: f32 = 0.8;

/// Delay em segundos depois do spawn antes do auto-pickup ficar ativo.
/// Garante que o player VEJA o drop cair antes de ele "voar" pro inv.
pub const LOOT_PICKUP_DELAY_S: f32 = 0.6;

/// Quem matou tem a frente no saque por este tempo (docs/PETS.md). Da' pra
/// pegar andando ate' la', e o pet continua valendo a pena: ele busca o que
/// esta' longe demais pra alcancar a tempo, e depois da janela pega o que os
/// outros deixaram pra tras. Sem isto, pet de raio grande limpava o drop de
/// quem matou o bicho.
pub const LOOT_PRIORIDADE_S: f32 = 2.0;

/// Itens conhecidos. Numeric id vai pro DB e rede. Manter sincronizado com
/// o cliente para sprite/cor por item.
pub mod item_id {
    // ── O equipamento (docs/COMBATE.md) ──
    // A arma e' o CONJUNTO inteiro; a secundaria vem amarrada a ela — so'
    // entra a do conjunto da arma. O grau (cor) e o refino moram na
    // instancia, nao no id: um id por peca, nao um por cor.
    pub const ESPADA_E_ESCUDO: u16 = 400;
    pub const KATANA: u16 = 401;
    pub const PISTOLAS: u16 = 402;
    pub const ANEL_MAGICO: u16 = 403;
    pub const MANTO_DO_GUERREIRO: u16 = 404;
    pub const BAINHA: u16 = 405;
    pub const COLDRE: u16 = 406;
    pub const MANTO_DO_MAGO: u16 = 407;
    pub const ARMADURA_LEVE: u16 = 408;
    pub const ARMADURA_MEDIA: u16 = 409;
    pub const ARMADURA_PESADA: u16 = 410;
    pub const BRINCO: u16 = 411;
    pub const AMULETO: u16 = 412;
    pub const BRACELETE: u16 = 413;
    pub const CINTO: u16 = 414;
    // Moeda / consumíveis
    pub const GOLD: u16 = 1;
    pub const HEALTH_POTION: u16 = 2;
    pub const MANA_POTION: u16 = 8;
    pub const GREATER_HEAL: u16 = 9; // +150 HP
    pub const GREATER_MANA: u16 = 10; // +100 MP
    pub const STAMINA_POTION: u16 = 11; // restaura 100 stamina
                                        // Armas
                                        // Armaduras
                                        // Acessórios
                                        // Materiais / loot raro
                                        // === Fase D — novas armas + acessórios ===

    // === Fase E — slots novos (helm/legs/boots/gloves/belt/cape/necklace) ===

    // === Fase F — armas tier 2 (gate de level + proficiência) ===

    // Resources — material de crafting. Tier define poder do item resultante.
    // Drops de mob baseados em loot_item_level: 1-15→t1, 16-30→t2, 31-50→t3, 51+→t4.
    pub const WOOD_T1: u16 = 60;
    pub const WOOD_T2: u16 = 61;
    pub const WOOD_T3: u16 = 62;
    pub const WOOD_T4: u16 = 63;
    pub const LEATHER_T1: u16 = 64;
    pub const LEATHER_T2: u16 = 65;
    pub const LEATHER_T3: u16 = 66;
    pub const LEATHER_T4: u16 = 67;

    // === Peixes (drop da pesca). Stackáveis, sem slot. O species da
    // EntityKind::Fish(n) mapeia 1:1 pra estes IDs via fish_item_for_species. ===
    pub const FISH_ANCHOVY: u16 = 96; // T1 comum
    pub const FISH_CLOWNFISH: u16 = 97; // T2
    pub const FISH_SURGEONFISH: u16 = 98; // T3
    pub const FISH_PUFFERFISH: u16 = 99; // T4 raro

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
    pub const STEEL: u16 = 300;
    pub const DARK_HEART_STONE: u16 = 304;
    pub const MOON_SHADOW_STONE: u16 = 308;
    pub const QUINTESSENCE: u16 = 312;
    pub const EXORCISM_BAUBLE: u16 = 316;
    pub const PLATINUM: u16 = 320;
    pub const ILLUMINATING_FRAGMENT: u16 = 324;
    pub const ANIMA_STONE: u16 = 328;
    /// Chave da arma: 1 por craft.
    pub const SCALE: u16 = 332;
    /// Chave da sub-arma: 1 por craft.
    pub const CLAW: u16 = 336;
    /// Chave do acessorio: 1 por craft.
    pub const HORN: u16 = 340;
    /// Chave da armadura: 1 por craft. E' o `LEATHER_T*` que ja' existia.
    pub const HIDE: u16 = LEATHER_T1;

    /// Sem cor: quantidade varia com o NIVEL do item, nao com a cor dele.
    pub const COPPER: u16 = 344;
    pub const DARKSTEEL: u16 = 345;
    /// Sem cor porque ela E' a cor: e' o que sobe um material de uma cor pra
    /// proxima. Ver `docs/ECONOMIA_DE_CRAFT.md`.
    pub const GLITTERING_POWDER: u16 = 346;
    /// Pocao de Experiencia (+30% XP por 1 h). So' sai de recompensa de
    /// missao de area — nao ha' loja que venda. Fora das faixas que a M27
    /// apaga (3..51, 68..71, 80..95, 102..268).
    pub const XP_POTION: u16 = 350;
    /// Pocao de Fortuna (+30% de ouro e cobre de bicho por 1 h) e Pocao de
    /// Sorte (+20% na chance de drop por 1 h). Recompensa de diaria, sem loja.
    pub const FORTUNA_POTION: u16 = 351;
    pub const SORTE_POTION: u16 = 352;
    /// Marcas da Tempestade: toda conclusao de dungeon da' (vinculadas).
    /// Moeda do Selo e, depois, do Mestre das Mares (docs/DUNGEONS_E_RAIDS.md).
    pub const MARCAS_TEMPESTADE: u16 = 357;
    /// Selo da Tempestade: entrada do estagio 5 de conteudo 60+. So' craft.
    pub const SELO_TEMPESTADE: u16 = 358;
    /// Pergaminho de Teleporte: salta pro destino marcado (mapa, "Ir" de
    /// missao, NPC) dentro da ilha. Alquimista, em cobre (`shared::viagem`).
    pub const PERGAMINHO_TELEPORTE: u16 = 359;
    /// Consumíveis da loja: a compra entrega o pergaminho na bolsa; o prêmio
    /// só é sorteado pelo servidor quando o jogador o usa.
    pub const PERGAMINHO_INVOCA_CHAVE: u16 = 360;
    pub const PERGAMINHO_INVOCA_MONTARIA: u16 = 361;
    pub const PERGAMINHO_INVOCA_TOMO: u16 = 362;
    pub const PERGAMINHO_INVOCA_PET: u16 = 363;
    /// Ração de Pet: alimenta o pet equipado (docs/PETS.md). Com fome ele nao
    /// recebe XP nenhuma. Item de loja, negociavel.
    pub const RACAO_DE_PET: u16 = 364;
    /// Tira TODAS as skills do pet equipado e devolve os slots.
    pub const REMOVEDOR_DE_SKILL_PET: u16 = 365;
    /// Re-rola a AFINIDADE do pet ou da montaria EQUIPADA (docs/PETS.md):
    /// que atributos a criatura empresta. Item de cash, negociavel.
    pub const PEDRA_DE_AFINIDADE: u16 = 366;

    /// Os cinco pets coletores (docs/PETS.md), pelo id do CINZA. Cada especie
    /// ocupa cinco ids seguidos, um por grau — a mesma convencao de `na_cor`
    /// dos materiais. O grau mora no id porque o pet e' item de bolsa:
    /// negociavel no mercado e combinavel na aba Combinar do Craft.
    /// A COR E' A CRIATURA: cinco ids seguidos, um por grau — porquinho,
    /// coruja, filhote de lobo, corujurso, filhote de dragao
    /// (`shared::pets`). Nao ha' mais especie separada do grau, entao ha'
    /// uma base so'. Os ids 425..444 ficaram vagos de proposito: eram das
    /// especies antigas e reusar id de item e' como ressuscitar o item.
    pub const PET_BASE: u16 = 420;
    pub const PETS: [u16; 1] = [PET_BASE];
    /// Ultimo id de pet — tudo entre `PET_BASE` e este e' pet.
    pub const PET_ULTIMO: u16 = PET_BASE + 4;

    /// As cinco skills de pet (`shared::pets::SKILLS`), em ids seguidos.
    pub const SKILL_PET_FARO: u16 = 445;
    pub const SKILL_PET_PASSO: u16 = 446;
    pub const SKILL_PET_ESTOMAGO: u16 = 447;
    pub const SKILL_PET_APRENDIZ: u16 = 448;
    pub const SKILL_PET_VIGOR: u16 = 449;
    /// Regeneracao: vida e mana por segundo.
    pub const SKILL_PET_REGEN_VIDA: u16 = 450;
    pub const SKILL_PET_REGEN_MANA: u16 = 451;
    /// Uma por atributo, na ordem de `stat_idx`: FOR, DES, INT, VIT, SPD, RES.
    pub const SKILL_PET_ATRIBUTO: [u16; super::STAT_COUNT] = [452, 453, 454, 455, 456, 457];
    pub const SKILL_PET_PRIMEIRA: u16 = SKILL_PET_FARO;
    pub const SKILL_PET_ULTIMA: u16 = 457;

    /// As tres montarias (docs/MONTARIAS.md), pelo id da CINZA. Cinco ids
    /// seguidos por especie, um por cor — a mesma convencao do pet.
    /// A COR E' A CRIATURA: cinco ids seguidos, um por grau — cervo, lobo,
    /// tigre, hipogrifo, dragao (`shared::montarias`). Nao ha' mais especie
    /// separada do grau, entao ha' uma base so'.
    pub const MONTARIA_BASE: u16 = 460;
    pub const MONTARIAS: [u16; 1] = [MONTARIA_BASE];
    pub const MONTARIA_ULTIMA: u16 = MONTARIA_BASE + 4;

    /// O PASSE DA ILHA MÁGICA (`shared::magica`): gasta 1 pra meia hora lá
    /// dentro, até três de uma vez.
    ///
    /// É ITEM, e não coluna de banco, de propósito: assim ele cai de chefe,
    /// se compra com TP, se vende no mercado e se dá de presente sem uma
    /// linha de código nova — e o "quantos eu tenho" é a quantidade na bolsa,
    /// que o jogador já sabe ler.
    pub const PASSE_MAGICO: u16 = 466;

    pub const fn montaria_no_grau(base: u16, grau: u8) -> u16 {
        base + (if grau < 1 {
            0
        } else if grau > 5 {
            4
        } else {
            grau - 1
        }) as u16
    }

    pub const fn montaria_de_id(id: u16) -> Option<(u16, u8)> {
        if id < MONTARIA_BASE || id > MONTARIA_ULTIMA {
            return None;
        }
        Some((MONTARIA_BASE, (id - MONTARIA_BASE) as u8 + 1))
    }

    /// E' uma skill de pet?
    pub const fn e_skill_de_pet(id: u16) -> bool {
        id >= SKILL_PET_PRIMEIRA && id <= SKILL_PET_ULTIMA
    }

    /// id do pet da especie `base` no grau `grau` (1 cinza .. 5 laranja).
    pub const fn pet_no_grau(base: u16, grau: u8) -> u16 {
        base + (if grau < 1 {
            0
        } else if grau > 5 {
            4
        } else {
            grau - 1
        }) as u16
    }

    /// (especie cinza, grau) de um id de pet. `None` se nao for pet.
    pub const fn pet_de_id(id: u16) -> Option<(u16, u8)> {
        if id < PET_BASE || id > PET_ULTIMO {
            return None;
        }
        Some((PET_BASE, (id - PET_BASE) as u8 + 1))
    }

    /// As quatro CHAVES de craft (uma por receita), pelo id da cinza. So'
    /// caem de chefe e de dungeon/raid (`shared::chaves`).
    pub const CHAVES: [u16; 4] = [SCALE, CLAW, HORN, HIDE];
    /// A cor 5 (lendaria) das chaves. Ficou fora da faixa contigua de
    /// `na_cor`: o id seguinte ao roxo ja' e' o proximo material.
    pub const SCALE_LENDARIA: u16 = 353;
    pub const CLAW_LENDARIA: u16 = 354;
    pub const HORN_LENDARIA: u16 = 355;
    pub const HIDE_LENDARIA: u16 = 356;

    /// A chave na cor pedida, 1 cinza .. 4 roxa, 5 lendaria.
    pub const fn chave_na_cor(base: u16, cor: u8) -> u16 {
        if cor < 5 {
            return na_cor(base, cor);
        }
        match base {
            SCALE => SCALE_LENDARIA,
            CLAW => CLAW_LENDARIA,
            HORN => HORN_LENDARIA,
            _ => HIDE_LENDARIA,
        }
    }

    /// Toda chave, de toda cor.
    pub fn todas_as_chaves() -> Vec<u16> {
        CHAVES
            .iter()
            .flat_map(|&b| (1..=5).map(move |cor| chave_na_cor(b, cor)))
            .collect()
    }

    /// Todos os materiais que existem nas quatro cores, pelo id da cinza.
    pub const MATERIAIS_COLORIDOS: [u16; 12] = [
        STEEL,
        DARK_HEART_STONE,
        MOON_SHADOW_STONE,
        QUINTESSENCE,
        EXORCISM_BAUBLE,
        PLATINUM,
        ILLUMINATING_FRAGMENT,
        ANIMA_STONE,
        SCALE,
        CLAW,
        HORN,
        HIDE,
    ];

    /// A COR (1 cinza .. 5 laranja) que o proprio id carrega, quando ele
    /// carrega. Material colorido, chave e pet guardam a cor no id — so'
    /// equipamento rolado guarda numa `ItemInstance`. Sem isso a bolsa
    /// pintava de cinza um Aço Roxo e uma Escama Azul.
    pub fn cor_de_id(id: u16) -> Option<u8> {
        if let Some((_, grau)) = pet_de_id(id) {
            return Some(grau);
        }
        if let Some((_, grau)) = montaria_de_id(id) {
            return Some(grau);
        }
        // As lendarias ficaram fora da faixa contigua das chaves.
        if matches!(
            id,
            SCALE_LENDARIA | CLAW_LENDARIA | HORN_LENDARIA | HIDE_LENDARIA
        ) {
            return Some(5);
        }
        MATERIAIS_COLORIDOS
            .iter()
            .find(|&&b| id >= b && id < b + 4)
            .map(|&b| (id - b) as u8 + 1)
    }

    /// O mesmo material, na cor pedida (1 cinza .. 4 roxo).
    pub const fn na_cor(base: u16, cor: u8) -> u16 {
        base + (if cor < 1 {
            0
        } else if cor > 4 {
            3
        } else {
            cor - 1
        }) as u16
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

    // === Relíquias do Abismo — Anéis de Storyline ===
}

// ============================================================================
// Crafting recipes
// ============================================================================

/// Estações de craft da praça. O cliente recebe `CraftRecipeNet.station` e
/// filtra as receitas pela estação que o player abriu.
pub mod craft_station {
    pub const FORGE: u8 = 0; // armas pesadas/médias + armadura placa
    pub const ATELIER: u8 = 1; // armas leves/mágicas + armadura couro/pano
    pub const SMELTER: u8 = 2; // refino de mineral + couro(=heart)
}

// item_stack_max vive no DB (server crate::economy).

/// Codigo de animacao (`attack_anim::*`) que o cliente deve tocar quando o
/// player ataca com a arma indicada. Mantém em sync com
/// `ItemInfo.AttackAnimOf` no cliente C#.
pub fn weapon_attack_anim(weapon_id: u16) -> u8 {
    use crate::components::attack_anim::*;
    use crate::skills::Conjunto;
    match Conjunto::da_arma(weapon_id) {
        Conjunto::Pistolas => SHOOT,
        Conjunto::AnelMagico => THRUST,
        _ => SLASH,
    }
}

/// Retorna o slot de equipamento para um item_id, ou None se nao for
/// equipavel.
pub fn equip_slot_of(item_id: u16) -> Option<EquipSlot> {
    use item_id::*;
    match item_id {
        ESPADA_E_ESCUDO | KATANA | PISTOLAS | ANEL_MAGICO => Some(EquipSlot::Weapon),
        MANTO_DO_GUERREIRO | BAINHA | COLDRE | MANTO_DO_MAGO => Some(EquipSlot::Offhand),
        ARMADURA_LEVE | ARMADURA_MEDIA | ARMADURA_PESADA => Some(EquipSlot::Armor),
        BRINCO => Some(EquipSlot::Earring),
        AMULETO => Some(EquipSlot::Necklace),
        BRACELETE => Some(EquipSlot::Bracelet),
        CINTO => Some(EquipSlot::Belt),
        id if pet_de_id(id).is_some() => Some(EquipSlot::Pet),
        id if montaria_de_id(id).is_some() => Some(EquipSlot::Montaria),
        _ => None,
    }
}

/// Armas cujo ataque e habilidades escalam com Sabedoria/INT. Centralizar a
/// classificacao aqui faz os proximos cajados, grimorios etc. herdarem a
/// mesma regra sem espalhar `match` pelo servidor. Hoje so existe o anel.
pub const fn arma_magica(item_id: u16) -> bool {
    matches!(item_id, item_id::ANEL_MAGICO)
}

/// O peso da armadura (docs/COMBATE.md): leve da' dano e cobra resistencia,
/// pesada o contrario, media e' o meio. Devolve (multiplicador de dano,
/// reducao de dano somada). Sem armadura conta como media.
pub fn peso_da_armadura(item_id: u16) -> (f32, f32) {
    match item_id {
        item_id::ARMADURA_LEVE => (1.10, 0.0),
        item_id::ARMADURA_PESADA => (0.90, 0.10),
        _ => (1.0, 0.0),
    }
}

/// Quanto de FOR a armadura empresta, como ponto alocado.
///
/// A MEDIA ganha isto porque, sem, ela era a pior escolha das tres: da' menos
/// defesa que a pesada e nao tem o dano da leve. A FOR paga a diferenca em
/// ataque e vida, sem mexer no peso — ela continua sendo o meio.
///
/// **Cresce com o nivel** em vez de ser um numero fixo. Fixo em 5 a simulacao
/// de chefe reprovou na hora: o Lobo Alfa (nivel 8) caia em 55s contra a meta
/// de 60, e o jogador PARADO bebendo pocao vencia com 13% de vida. Cinco
/// pontos sao ruido no nivel 60 e sao a luta inteira no nivel 8. Um por
/// `FOR_DA_MEDIA_A_CADA` niveis poe o ganho na mesma escala em que o resto do
/// personagem cresce.
pub fn for_da_armadura(item_id: u16, nivel: u32) -> u32 {
    match item_id {
        item_id::ARMADURA_MEDIA => FOR_DA_ARMADURA_MEDIA + nivel / FOR_DA_MEDIA_A_CADA,
        _ => 0,
    }
}

/// Pontos de FOR da armadura media. Vale +1 ataque e +2 vida cada
/// (`STAT_POINT_BONUS`).
pub const FOR_DA_ARMADURA_MEDIA: u32 = 1;
/// A cada tantos niveis a media empresta mais um ponto de FOR.
pub const FOR_DA_MEDIA_A_CADA: u32 = 10;

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum EquipSlot {
    /// A arma: o conjunto (docs/COMBATE.md).
    Weapon,
    /// A secundaria amarrada ao conjunto (manto, bainha, coldre).
    Offhand,
    Armor,
    Earring,
    Necklace,
    Bracelet,
    Belt,
    /// O pet coletor (docs/PETS.md). Equipado, ele nasce no mundo e busca o
    /// saque do chao; os atributos dele entram como pontos alocados.
    Pet,
    /// A montaria (docs/MONTARIAS.md). E' nela que se monta, e a cor dela
    /// manda na velocidade.
    Montaria,
}

impl EquipSlot {
    pub const TODOS: [EquipSlot; 9] = [
        EquipSlot::Weapon,
        EquipSlot::Offhand,
        EquipSlot::Armor,
        EquipSlot::Earring,
        EquipSlot::Necklace,
        EquipSlot::Bracelet,
        EquipSlot::Belt,
        EquipSlot::Pet,
        EquipSlot::Montaria,
    ];

    /// String do slot pra ser persistido no DB (coluna `slot`).
    pub fn as_db_str(&self) -> &'static str {
        match self {
            EquipSlot::Weapon => "weapon",
            EquipSlot::Offhand => "offhand",
            EquipSlot::Armor => "armor",
            EquipSlot::Earring => "earring",
            EquipSlot::Necklace => "necklace",
            EquipSlot::Bracelet => "bracelet",
            EquipSlot::Belt => "belt",
            EquipSlot::Pet => "pet",
            EquipSlot::Montaria => "montaria",
        }
    }

    pub fn de_db_str(s: &str) -> Option<EquipSlot> {
        Self::TODOS.into_iter().find(|x| x.as_db_str() == s)
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

/// Energia cobrada pelo PRIMEIRO ponto de atributo de um personagem.
pub const ENERGIA_BASE_DO_PONTO: u64 = 10;
/// Quanto a Energia do proximo ponto sobe a cada ponto ja' alocado.
pub const ENERGIA_PASSO_DO_PONTO: u64 = 5;

/// Energia do PROXIMO ponto de atributo, dado quantos ja' estao alocados.
/// Sobe em passo fixo: o ponto 1 custa 10, o 11 custa 60, o 51 custa 260.
/// Quem redistribui de graca volta ao comeco da escada e paga a subida de
/// novo — o reset devolve os pontos, nunca a Energia.
pub fn custo_energia_do_ponto(ja_alocados: u32) -> u64 {
    ENERGIA_BASE_DO_PONTO + ENERGIA_PASSO_DO_PONTO * ja_alocados as u64
}

/// Ate' que nivel o tutorial da Energia tem que dar folga. O passo manda
/// juntar Energia bastante pra o jogador gastar TODOS os pontos que os
/// primeiros niveis renderam — nao um cristal de enfeite.
pub const NIVEL_DO_TUTORIAL_DE_ENERGIA: u32 = 4;

/// Energia pra gastar, do zero, todos os pontos que um personagem ganha ate'
/// `nivel`. E' a soma da escada `custo_energia_do_ponto`, em forma fechada
/// porque isto precisa ser `const` (a missao guarda o numero).
pub const fn energia_pros_pontos_ate_o_nivel(nivel: u32) -> u64 {
    let n = (POINTS_PER_LEVEL * nivel.saturating_sub(1)) as u64;
    if n == 0 {
        return 0;
    }
    ENERGIA_BASE_DO_PONTO * n + ENERGIA_PASSO_DO_PONTO * (n * (n - 1) / 2)
}

/// Quanta Energia o passo de tutorial pede.
pub const ENERGIA_DO_TUTORIAL: u32 =
    energia_pros_pontos_ate_o_nivel(NIVEL_DO_TUTORIAL_DE_ENERGIA) as u32;

/// Energia total pra sair de `ja_alocados` e alocar mais `quantos` pontos.
pub fn custo_energia_de_varios(ja_alocados: u32, quantos: u32) -> u64 {
    (0..quantos)
        .map(|k| custo_energia_do_ponto(ja_alocados.saturating_add(k)))
        .sum()
}

/// Rank máximo de uma skill (0 = não aprendida; 1..=10 = ranks).
pub const MAX_SKILL_RANK: u8 = 10;

/// Custo em SP de cada rank, indexado por rank (0 = unlock cost = 1, idx 1..10).
/// Total para maxar uma skill = 1+1+1+2+2+2+3+3+3+5 = **23 SP**.
pub const SP_COST_PER_RANK: [u32; 11] = [
    0, // rank 0 = não aprendida
    1, 1, 1, // rank 1-3: cheap unlock + early
    2, 2, 2, // rank 4-6: meio (rank 5 = milestone)
    3, 3, 3, // rank 7-9
    5, // rank 10 = capstone
];

/// Custo em SP pra subir de `current_rank` para `current_rank + 1`.
/// Retorna 0 se já está no max.
pub const fn sp_cost_for_next_rank(current_rank: u8) -> u32 {
    if current_rank >= MAX_SKILL_RANK {
        return 0;
    }
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
    (2, 3),   // T1 — Aprendiz (acessível quase imediato)
    (6, 8),   // T2 — Adepto
    (12, 15), // T3 — Mestre
    (20, 25), // T4 — Lendário (cap onde TUDO ta unlocked)
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
    unlock_char: u8,
    unlock_prof: u8,
    target_rank: u8,
) -> (u8, u8) {
    if target_rank <= 1 {
        return (unlock_char, unlock_prof);
    }
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

/// A sigla de cada atributo, na ordem de `stat_idx`. Existe no `shared`
/// porque o SERVIDOR precisa dela pra escrever no chat qual afinidade saiu na
/// Pedra (docs/PETS.md) — o cliente tem a tabela dele, com a descricao junto.
pub const SIGLA_DO_STAT: [&str; STAT_COUNT] = ["FOR", "DES", "INT", "VIT", "SPD", "RES"];

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
/// INT (Inteligencia): +1 wis/dano mágico, +2 mp_max. Com arma mágica, cada
/// ponto alocado entra no ataque básico e nas habilidades.
/// VIT (Vitalidade):   +5 hp_max, +HP_REGEN_PER_VIT hp regen
/// SPD (Velocidade):   +MOVE_SPEED_PCT_PER_SPD move speed, +STAMINA_MAX_PER_SPD stamina, +STAMINA_REGEN_PER_SPD st regen
/// RES (Resistencia):  +DEFENSE_PER_RES def, +BLOCK_REDUCTION_PER_RES dmg absorvido em block,
///                     -STAMINA_COST_REDUCTION_PER_RES no custo de block/parry
pub const STAT_POINT_BONUS: [StatAllocBonus; STAT_COUNT] = [
    /* FOR */
    StatAllocBonus {
        hp_max: 2,
        mp_max: 0,
        attack_damage: 1,
        dex: 0,
        wis: 0,
        defense: 0,
        speed_pct: 0.0,
        crit_chance: 0.0,
        hp_regen: 0.0,
        attack_speed_pct: 0.0,
        stamina_max: 0,
        stamina_regen: 0.0,
        block_reduction_bonus: 0.0,
        stamina_cost_reduction: 0.0,
        dash_cd_reduction_pct: 0.0,
    },
    /* DES */
    StatAllocBonus {
        hp_max: 0,
        mp_max: 0,
        attack_damage: 0,
        dex: 1,
        wis: 0,
        defense: 0,
        speed_pct: 0.0,
        crit_chance: CRIT_CHANCE_PER_DES,
        hp_regen: 0.0,
        attack_speed_pct: ATTACK_SPEED_PCT_PER_DES,
        stamina_max: 0,
        stamina_regen: 0.0,
        block_reduction_bonus: 0.0,
        stamina_cost_reduction: 0.0,
        dash_cd_reduction_pct: 0.0,
    },
    /* INT */
    StatAllocBonus {
        hp_max: 0,
        mp_max: 2,
        attack_damage: 0,
        dex: 0,
        wis: 1,
        defense: 0,
        speed_pct: 0.0,
        crit_chance: 0.0,
        hp_regen: 0.0,
        attack_speed_pct: 0.0,
        stamina_max: 0,
        stamina_regen: 0.0,
        block_reduction_bonus: 0.0,
        stamina_cost_reduction: 0.0,
        dash_cd_reduction_pct: 0.0,
    },
    /* VIT */
    StatAllocBonus {
        hp_max: 5,
        mp_max: 0,
        attack_damage: 0,
        dex: 0,
        wis: 0,
        defense: 0,
        speed_pct: 0.0,
        crit_chance: 0.0,
        hp_regen: HP_REGEN_PER_VIT,
        attack_speed_pct: 0.0,
        stamina_max: 0,
        stamina_regen: 0.0,
        block_reduction_bonus: 0.0,
        stamina_cost_reduction: 0.0,
        dash_cd_reduction_pct: 0.0,
    },
    /* SPD */
    StatAllocBonus {
        hp_max: 0,
        mp_max: 0,
        attack_damage: 0,
        dex: 0,
        wis: 0,
        defense: 0,
        speed_pct: 0.0,
        crit_chance: 0.0,
        hp_regen: 0.0,
        attack_speed_pct: 0.0,
        stamina_max: STAMINA_MAX_PER_SPD,
        stamina_regen: STAMINA_REGEN_PER_SPD,
        block_reduction_bonus: 0.0,
        stamina_cost_reduction: 0.0,
        dash_cd_reduction_pct: DASH_CD_REDUCTION_PER_SPD,
    },
    /* RES */
    StatAllocBonus {
        hp_max: 0,
        mp_max: 0,
        attack_damage: 0,
        dex: 0,
        wis: 0,
        defense: DEFENSE_PER_RES,
        speed_pct: 0.0,
        crit_chance: 0.0,
        hp_regen: 0.0,
        attack_speed_pct: 0.0,
        stamina_max: 0,
        stamina_regen: 0.0,
        block_reduction_bonus: BLOCK_REDUCTION_PER_RES,
        stamina_cost_reduction: STAMINA_COST_REDUCTION_PER_RES,
        dash_cd_reduction_pct: 0.0,
    },
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
    let z = WeaponScaling {
        hp_max: 0.0,
        mp_max: 0.0,
        attack_damage: 0.0,
        dex: 0.0,
        wis: 0.0,
        defense: 0.0,
    };
    match item_id {
        // espada e escudo: linha de frente — vida e resistencia
        item_id::ESPADA_E_ESCUDO => WeaponScaling {
            hp_max: 1.0,
            defense: 0.1,
            ..z
        },
        // katana: corte rapido — dano e destreza
        item_id::KATANA => WeaponScaling {
            attack_damage: 0.3,
            dex: 0.2,
            ..z
        },
        // pistolas: a' distancia — destreza
        item_id::PISTOLAS => WeaponScaling {
            attack_damage: 0.1,
            dex: 0.5,
            ..z
        },
        // anel: magia — mana e sabedoria
        item_id::ANEL_MAGICO => WeaponScaling {
            mp_max: 1.0,
            wis: 0.3,
            ..z
        },
        _ => z,
    }
}

/// Scaling quando sem arma (Unarmed). Aplicado com Proficiency::Unarmed level.
pub const fn unarmed_scaling() -> WeaponScaling {
    WeaponScaling {
        hp_max: 0.0,
        mp_max: 0.0,
        attack_damage: 0.2,
        dex: 0.0,
        wis: 0.0,
        defense: 0.0,
    }
}

/// True se a arma e de corpo-a-corpo (gera dano em cone na frente ao atacar,
/// nao projetil). Sem arma = melee (soco). BOW/WAND/STAFF disparam projetil.
pub fn weapon_is_melee(item_id: u16) -> bool {
    !crate::skills::Conjunto::da_arma(item_id).a_distancia()
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
    // O que a peca da' SEM instancia (sem rolagem) — a mesma ordem de grandeza
    // do meio do template (`items::item_template`).
    const fn b(
        hp_max: i32,
        mp_max: i32,
        attack_damage: i32,
        dex: i32,
        wis: i32,
        defense: i32,
    ) -> EquipBonus {
        EquipBonus {
            hp_max,
            mp_max,
            attack_damage,
            dex,
            wis,
            defense,
        }
    }
    match item_id {
        // A linha de frente: +60 de vida (era +20) — quem segura a mordida.
        item_id::ESPADA_E_ESCUDO => b(60, 0, 12, 0, 0, 0),
        item_id::KATANA => b(0, 0, 11, 8, 0, 0),
        item_id::PISTOLAS => b(0, 0, 10, 9, 0, 0),
        item_id::ANEL_MAGICO => b(0, 50, 9, 0, 7, 0),
        item_id::MANTO_DO_GUERREIRO => b(35, 0, 0, 0, 0, 5),
        item_id::BAINHA => b(0, 0, 3, 5, 0, 0),
        item_id::COLDRE => b(0, 0, 3, 5, 0, 0),
        item_id::MANTO_DO_MAGO => b(0, 45, 0, 0, 5, 0),
        item_id::ARMADURA_LEVE => b(25, 0, 0, 4, 0, 2),
        item_id::ARMADURA_MEDIA => b(45, 0, 0, 0, 0, 6),
        item_id::ARMADURA_PESADA => b(90, 0, 0, 0, 0, 12),
        item_id::BRINCO => b(0, 0, 3, 4, 0, 0),
        item_id::AMULETO => b(0, 35, 0, 0, 4, 0),
        item_id::BRACELETE => b(0, 0, 3, 0, 0, 2),
        item_id::CINTO => b(30, 0, 0, 0, 0, 2),
        _ => b(0, 0, 0, 0, 0, 0),
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
    pub const WALL: u16 = 2;
    pub const DIRT: u16 = 3;
    pub const WATER: u16 = 4;
    /// Piso de dungeon — visualmente distinto, colisoes iguais a FLOOR.
    pub const DUNGEON_FLOOR: u16 = 5;
    pub const WOOD: u16 = 5;
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

// ── Coleta por NO' ─────────────────────────────────────────────────────────
// A coleta deixou de ser passiva pelo lugar: o jogador escolhe a pedra (ou o
// tronco), vai ate' ela e coleta ELA, ciclo a ciclo, ate' a reserva
// (`COLETAS_POR_PEDRA` / `COLETAS_POR_ARVORE`) acabar. Reserva, respawn e
// rendimento seguem o planejamento (docs/COLETA.md); os tres numeros abaixo
// NAO estavam nele.

/// Segundos de um ciclo de coleta numa pedra, por tier.
/// DECISAO PROVISORIA (nao estava no planejamento): o doc so' definia o ritmo
/// por densidade do lugar, que saiu junto com a coleta passiva.
pub const COLETA_CICLO_PEDRA_S: [f32; 5] = [0.0, 2.5, 2.8, 3.1, 3.4];
/// Segundos de um ciclo num tronco. DECISAO PROVISORIA.
pub const COLETA_CICLO_ARVORE_S: f32 = 2.0;
/// Veio de Energia: reserva e respawn próprios, sem ocupar a bolsa.
///
/// Eram 20 ciclos — 240 de Energia por veio na primeira ilha, o que o dono
/// resumiu em 20/09/2026 como "acaba rápido, é muito pouco". Um veio agora
/// paga 600, e eles nascem em CAMPO (`terreno::no_campo_de_energia`), não
/// soltos pela ilha: o lugar é que é a fonte, e não o cristal.
pub const COLETAS_POR_ENERGIA: u32 = 50;
pub const RESPAWN_DA_ENERGIA: f32 = 360.0;
pub const COLETA_CICLO_ENERGIA_S: f32 = 2.5;
/// Distancia maxima da BORDA do corpo pra coletar. DECISAO PROVISORIA.
pub const COLETA_ALCANCE_UN: f32 = 1.4;
/// Raio de busca do AUTO COLETA a partir de onde foi ligado (config do
/// jogador). DECISAO PROVISORIA; o padrao e' o raio da busca de spot antiga.
pub const COLETA_RAIO_AUTO_MIN: f32 = 20.0;
pub const COLETA_RAIO_AUTO_MAX: f32 = 100.0;
pub const COLETA_RAIO_AUTO_PADRAO: f32 = 60.0;

/// Ciclo de coleta de um no' (0 = tronco, 1..4 = pedra pela cor).
pub fn ciclo_de_coleta_s(tier: u8) -> f32 {
    match tier {
        0 => COLETA_CICLO_ARVORE_S,
        5 => COLETA_CICLO_ENERGIA_S,
        _ => COLETA_CICLO_PEDRA_S[(tier as usize).min(4)],
    }
}

/// Nome do tipo de no' pra HUD: 0 madeira, 1..4 pedra pela cor.
pub fn nome_do_no(tier: u8) -> &'static str {
    match tier {
        0 => "Madeira",
        1 => "Pedra cinza",
        2 => "Pedra verde",
        3 => "Pedra azul",
        4 => "Pedra roxa",
        5 => "Energia",
        _ => "Recurso",
    }
}

/// O que uma pedra de cada tier ENTREGA, em peso por tier de material.
///
/// A escada e' a mesma em toda linha: o material cinza e' sempre a maioria, e
/// cada tier acrescenta um pouco do proprio e um pouco mais do anterior. A
/// pedra roxa NAO da' material roxo — ela da' mais azul que a azul, e paga a
/// diferenca em tempo de coleta (`COLETAS_POR_PEDRA`).
///
/// Nao ha' material laranja: sao quatro cores e o teto e' o roxo.
pub const RENDIMENTO_DA_PEDRA: [[u16; 4]; 5] = [
    [0, 0, 0, 0],
    [100, 0, 0, 0],  // cinza: so' cinza
    [80, 20, 0, 0],  // verde: verde, com muito mais cinza
    [65, 25, 10, 0], // azul: cinza ainda manda, mais verde que a anterior, um pouco de azul
    [55, 27, 18, 0], // roxo: mesma escada, sem roxo, um pouquinho mais de azul
];

/// Tier do MATERIAL que sai desta pedra neste sorteio. `f` e' 0..1.
pub fn tier_do_rendimento(tier_da_pedra: u8, f: f32) -> u8 {
    let pesos = RENDIMENTO_DA_PEDRA[(tier_da_pedra as usize).min(4)];
    let total: u16 = pesos.iter().sum();
    if total == 0 {
        return 1;
    }
    let mut alvo = (f.clamp(0.0, 0.999) * total as f32) as u16;
    for (i, p) in pesos.iter().enumerate() {
        if alvo < *p {
            return i as u8 + 1;
        }
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
                assert!(
                    (obtido - esperado).abs() < 0.01,
                    "pedra t{tier} material t{m}: {obtido:.3} != {esperado:.3}"
                );
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
/// Tempos das animacoes de ataque, compartilhados com o servidor. O dano
/// acontece ao terminar o corte, nunca no inicio da antecipacao.
pub const PLAYER_ATTACK_PREPARE_S: f32 = 0.08;
pub const PLAYER_ATTACK_CUT_S: f32 = 0.11;
pub const PLAYER_ATTACK_IMPACT_S: f32 = PLAYER_ATTACK_PREPARE_S + PLAYER_ATTACK_CUT_S;
pub const MOB_ATTACK_PREPARE_S: f32 = 0.24;
pub const MOB_ATTACK_CUT_S: f32 = 0.22;
pub const MOB_ATTACK_IMPACT_S: f32 = MOB_ATTACK_PREPARE_S + MOB_ATTACK_CUT_S;

#[cfg(test)]
mod testes_atributos {
    use super::*;

    /// A forma fechada tem que bater com a escada somada um a um — se ela
    /// divergir, a missao pede um numero que nao corresponde a nada.
    #[test]
    fn a_conta_fechada_do_tutorial_bate_com_a_escada() {
        for nivel in 1..=10u32 {
            let pontos = POINTS_PER_LEVEL * nivel.saturating_sub(1);
            assert_eq!(
                energia_pros_pontos_ate_o_nivel(nivel),
                custo_energia_de_varios(0, pontos),
                "nivel {nivel}"
            );
        }
        assert_eq!(energia_pros_pontos_ate_o_nivel(1), 0, "nivel 1 nao rendeu");
        // O numero da missao: nove pontos (tres niveis a tres cada).
        assert_eq!(
            ENERGIA_DO_TUTORIAL as u64,
            custo_energia_de_varios(0, POINTS_PER_LEVEL * 3)
        );
        assert!(ENERGIA_DO_TUTORIAL > 0);
        // E ele tem que caber em pouca coleta: mais que dois veios inteiros
        // seria farm, nao tutorial. Um veio rende COLETAS_POR_ENERGIA ciclos.
        let por_veio =
            crate::skills::energia_por_coleta(0) * COLETAS_POR_ENERGIA as u64;
        assert!(
            (ENERGIA_DO_TUTORIAL as u64) <= por_veio * 2,
            "{} de Energia sao mais de dois veios ({por_veio} cada)",
            ENERGIA_DO_TUTORIAL
        );
    }

    /// UMA coleta de Energia tem que pagar o PRIMEIRO ponto de atributo.
    ///
    /// O tutorial manda coletar Energia (776) e logo depois gastar um ponto
    /// (775). Se um ciclo de coleta rendesse menos que o ponto custa, o passo
    /// seguinte pediria uma coisa impossivel — e foi assim que o dono
    /// encontrou, jogando, em 20/09/2026: "o tutorial me pede pra distribuir
    /// atributos mas nao me deu energia pra fazer isso".
    #[test]
    fn uma_coleta_de_energia_paga_o_primeiro_ponto() {
        let rende = crate::skills::energia_por_coleta(0);
        let custo = custo_energia_do_ponto(0);
        assert!(
            rende >= custo,
            "uma coleta rende {rende} e o primeiro ponto custa {custo}"
        );
        // E vale em toda ilha: quem chega na Geleira com o passo em aberto
        // tambem tem que conseguir.
        for ilha in 0..4 {
            assert!(crate::skills::energia_por_coleta(ilha) >= custo);
        }
    }

    /// A media so' existe como escolha se compensar em algum lugar: ela da'
    /// menos defesa que a pesada e nao tem o dano da leve.
    #[test]
    fn so_a_armadura_media_empresta_forca() {
        assert_eq!(for_da_armadura(item_id::ARMADURA_MEDIA, 1), FOR_DA_ARMADURA_MEDIA);
        assert!(FOR_DA_ARMADURA_MEDIA > 0);
        for nivel in [1, 8, 30, 60] {
            assert_eq!(for_da_armadura(item_id::ARMADURA_LEVE, nivel), 0);
            assert_eq!(for_da_armadura(item_id::ARMADURA_PESADA, nivel), 0);
            assert_eq!(
                for_da_armadura(0, nivel),
                0,
                "sem armadura nao empresta nada"
            );
            assert_eq!(for_da_armadura(item_id::KATANA, nivel), 0);
        }
        // Cresce com o nivel, e devagar: no nivel do primeiro chefe ela ainda
        // vale UM ponto — foi isso que a simulacao de chefe cobrou.
        assert_eq!(for_da_armadura(item_id::ARMADURA_MEDIA, 8), 1);
        let baixo = for_da_armadura(item_id::ARMADURA_MEDIA, 10);
        let alto = for_da_armadura(item_id::ARMADURA_MEDIA, 60);
        assert!(alto > baixo, "no nivel 60 tem que valer mais que no 10");
        assert!(alto <= 10, "media nao pode virar a melhor de longe: {alto}");
        // O peso dela continua o do meio: a FOR nao a transformou em leve.
        assert_eq!(peso_da_armadura(item_id::ARMADURA_MEDIA), (1.0, 0.0));
        let (dano_leve, _) = peso_da_armadura(item_id::ARMADURA_LEVE);
        let (dano_pesada, red_pesada) = peso_da_armadura(item_id::ARMADURA_PESADA);
        assert!(dano_leve > 1.0 && dano_pesada < 1.0 && red_pesada > 0.0);
    }

    /// Cada ponto de atributo custa mais Energia que o anterior, e o total
    /// de uma tacada e' a soma dos degraus — nunca um multiplo do primeiro.
    #[test]
    fn energia_do_ponto_sobe_com_o_que_ja_foi_gasto() {
        assert_eq!(custo_energia_do_ponto(0), 10);
        assert_eq!(custo_energia_do_ponto(1), 15);
        assert_eq!(custo_energia_do_ponto(10), 60);
        assert_eq!(custo_energia_do_ponto(50), 260);
        for n in 0..200u32 {
            assert!(custo_energia_do_ponto(n + 1) > custo_energia_do_ponto(n));
        }
        assert_eq!(custo_energia_de_varios(0, 0), 0);
        assert_eq!(custo_energia_de_varios(0, 1), 10);
        assert_eq!(custo_energia_de_varios(0, 3), 10 + 15 + 20);
        assert_eq!(custo_energia_de_varios(10, 2), 60 + 65);
        // Um level-up inteiro (3 pontos) no comeco cabe em 3 coletas da ilha 1.
        assert!(custo_energia_de_varios(0, POINTS_PER_LEVEL) <= 3 * 12 + 9);
    }

}
