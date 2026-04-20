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
pub const PROTOCOL_VERSION: u16 = 1;

/// Velocidade base do jogador em tiles/segundo.
pub const PLAYER_SPEED: f32 = 5.0;

/// Velocidade dos projeteis em tiles/segundo.
pub const PROJ_SPEED: f32 = 15.0;

/// Tempo de vida de um projetil em segundos.
pub const PROJ_TTL: f32 = 1.5;

/// Cooldown entre ataques em segundos.
pub const ATTACK_COOLDOWN: f32 = 0.25;
