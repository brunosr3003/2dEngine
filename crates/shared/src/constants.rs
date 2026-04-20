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

/// Tamanho de uma celula do grid espacial (em tiles). Deve ser >= AOI/2
/// para que a busca de vizinhos acesse no maximo 4 celulas.
pub const SPATIAL_CELL_SIZE: f32 = 16.0;

/// Limite de jogadores por shard/mapa. Acima disso, spawn novo shard.
pub const MAX_PLAYERS_PER_SHARD: usize = 256;

/// Versao do protocolo. INCREMENTAR sempre que mensagens/layouts mudarem
/// em shared::protocol — clientes com versao errada sao rejeitados.
pub const PROTOCOL_VERSION: u16 = 4;

/// Velocidade base do jogador em tiles/segundo.
pub const PLAYER_SPEED: f32 = 5.0;

/// Velocidade dos projeteis em tiles/segundo.
pub const PROJ_SPEED: f32 = 15.0;

/// Tempo de vida de um projetil em segundos.
pub const PROJ_TTL: f32 = 1.5;

/// Cooldown entre ataques em segundos.
pub const ATTACK_COOLDOWN: f32 = 0.25;

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

/// Raio de colisao de projeteis.
pub const PROJ_RADIUS: f32 = 0.15;

/// Tempo de respawn do jogador em segundos.
pub const RESPAWN_DELAY: f32 = 3.0;

/// XP ganho por matar um inimigo (base — pode variar por tipo no futuro).
pub const XP_PER_KILL: u64 = 30;

/// Retorna o level derivado a partir da XP acumulada.
/// Curva simples quadratica: L = 1 + floor(sqrt(xp / 100)).
///     L1: 0 xp  L2: 100  L3: 400  L4: 900  L5: 1600  L10: 8100
pub const fn level_of_xp(xp: u64) -> u32 {
    let mut lvl = 1u32;
    let mut need = 100u64;
    let mut remaining = xp;
    while remaining >= need {
        remaining -= need;
        lvl += 1;
        need = (lvl as u64) * (lvl as u64) * 100;
    }
    lvl
}

/// XP total necessaria para atingir `level` (acumulada desde L1).
pub const fn xp_for_level(level: u32) -> u64 {
    let mut sum = 0u64;
    let mut l = 1u32;
    while l < level {
        sum += (l as u64) * (l as u64) * 100;
        l += 1;
    }
    sum
}

/// Quantidade de inimigos gerados no inicio.
pub const ENEMY_START_COUNT: usize = 24;

/// IDs logicos de tile — usados no WorldMap e no TileDef lookup.
pub mod tile_id {
    pub const FLOOR: u16 = 1;
    pub const WALL:  u16 = 2;
    pub const DIRT:  u16 = 3;
    pub const WATER: u16 = 4;
    pub const WOOD:  u16 = 5;
}
