//! GLOBAL CHAT across every channel and island (`shared::chat`).
//!
//! Sending: the world checks mute, ban and pace, runs the word filter, and
//! hands the line here. A task saves it in `chat_historico` (ALWAYS — the
//! original text too, delivered or not) and, when it goes out, NOTIFYs
//! `shared::chat::CANAL_NOTIFY`. Every game process LISTENs on that channel
//! and the tick delivers what arrives to its own players, so a line typed in
//! the Glade reaches the Frozen Isle in a few milliseconds.
//!
//! The Panopticon uses the same channel for staff warnings and bans, and
//! writes the word list, mutes and bans that a task here re-reads every 5 s.

use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::OnceLock;
use std::time::Duration;

use shared::chat::{self, Acao, Notificacao, CANAL_NOTIFY};
use sqlx::PgPool;

static POOL: OnceLock<PgPool> = OnceLock::new();
/// This process on the NOTIFY channel (whisper answers come back to it).
static PROCESSO: OnceLock<u64> = OnceLock::new();
/// Whispers waiting for the receiver's process: id -> (sender, receiver, text, since).
type Pendente = (crate::world::SessionId, String, String, f64);
static PENDENTES: parking_lot::Mutex<Option<HashMap<i64, Pendente>>> = parking_lot::Mutex::new(None);
/// Who may whisper whom, as set by each player online here (name lower case).
static MODOS: parking_lot::Mutex<Option<HashMap<String, u8>>> = parking_lot::Mutex::new(None);
static PALAVRAS: parking_lot::RwLock<Vec<(String, Acao)>> = parking_lot::RwLock::new(Vec::new());
static SILENCIOS: parking_lot::RwLock<Option<HashMap<i64, i64>>> = parking_lot::RwLock::new(None);
static BANIDOS: parking_lot::RwLock<Option<HashSet<i64>>> = parking_lot::RwLock::new(None);
static STAFF: parking_lot::RwLock<Option<HashSet<i64>>> = parking_lot::RwLock::new(None);
static FILA: parking_lot::Mutex<Vec<Notificacao>> = parking_lot::Mutex::new(Vec::new());
/// Pace per account: recent send times, last text and when.
static RITMO: parking_lot::Mutex<Option<HashMap<i64, (VecDeque<f64>, String, f64)>>> = parking_lot::Mutex::new(None);

fn agora_unix() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as i64)
}

fn agora_s() -> f64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0.0, |d| d.as_secs_f64())
}

pub fn palavras() -> Vec<(String, Acao)> {
    PALAVRAS.read().clone()
}

pub fn banido(conta: i64) -> bool {
    BANIDOS.read().as_ref().is_some_and(|b| b.contains(&conta))
}

pub fn staff(conta: i64) -> bool {
    STAFF.read().as_ref().is_some_and(|b| b.contains(&conta))
}

/// Minutes of mute left, if muted.
pub fn silenciado(conta: i64) -> Option<i64> {
    let ate = SILENCIOS.read().as_ref().and_then(|m| m.get(&conta).copied())?;
    let resta = ate - agora_unix();
    (resta > 0).then(|| (resta + 59) / 60)
}

fn silenciar_local(conta: i64, minutos: i64) {
    SILENCIOS.write().get_or_insert_with(HashMap::new).insert(conta, agora_unix() + minutos * 60);
}

fn banir_local(conta: i64) {
    BANIDOS.write().get_or_insert_with(HashSet::new).insert(conta);
}

#[derive(Debug, PartialEq)]
pub enum Recusa {
    Devagar,
    Repetida,
}

/// Anti-spam: minimum gap, burst cap and copy-paste repeats.
pub fn ritmo(conta: i64, texto: &str) -> Result<(), Recusa> {
    ritmo_em(conta, texto, agora_s())
}

fn ritmo_em(conta: i64, texto: &str, t: f64) -> Result<(), Recusa> {
    let mut g = RITMO.lock();
    let e = g.get_or_insert_with(HashMap::new).entry(conta).or_insert_with(|| (VecDeque::new(), String::new(), f64::MIN));
    while e.0.front().is_some_and(|&x| t - x > chat::JANELA_S) {
        e.0.pop_front();
    }
    if e.0.back().is_some_and(|&x| t - x < chat::INTERVALO_MIN_S) || e.0.len() >= chat::RAJADA {
        return Err(Recusa::Devagar);
    }
    if t - e.2 < chat::REPETIDA_S && e.1.eq_ignore_ascii_case(texto) {
        return Err(Recusa::Repetida);
    }
    e.0.push_back(t);
    e.1 = texto.to_string();
    e.2 = t;
    Ok(())
}

pub fn processo() -> u64 {
    *PROCESSO.get_or_init(|| fastrand::u64(1..u64::MAX))
}

pub fn define_modo(nome: &str, modo: u8) {
    MODOS.lock().get_or_insert_with(HashMap::new).insert(nome.to_lowercase(), modo.min(chat::SUSSURRO_NINGUEM));
}

pub fn modo(nome: &str) -> u8 {
    MODOS.lock().as_ref().and_then(|m| m.get(&nome.to_lowercase()).copied()).unwrap_or(chat::SUSSURRO_TODOS)
}

/// The answer for a whisper this process sent: who sent it, to whom, the text.
pub fn resolver(id: i64) -> Option<Pendente> {
    PENDENTES.lock().as_mut()?.remove(&id)
}

/// Whispers nobody answered: the receiver is offline (or does not exist).
pub fn vencidos() -> Vec<Pendente> {
    let t = agora_s();
    let mut g = PENDENTES.lock();
    let Some(m) = g.as_mut() else { return Vec::new() };
    let ids: Vec<i64> = m.iter().filter(|(_, p)| t - p.3 > chat::SUSSURRO_ESPERA_S).map(|(id, _)| *id).collect();
    ids.into_iter().filter_map(|id| m.remove(&id)).collect()
}

/// The receiver's process answers the sender's.
pub fn responder(id: i64, origem: u64, resultado: u8, para: String) {
    let Some(pool) = POOL.get().cloned() else { return };
    tokio::spawn(async move {
        let _ = notificar(&pool, &Notificacao::SussurroResposta { id, origem, resultado, para }).await;
    });
}

/// Saves a whisper and sends it looking for its receiver.
#[allow(clippy::too_many_arguments)]
pub fn publicar_sussurro(
    sid: crate::world::SessionId,
    conta: i64,
    nome: String,
    para: String,
    zona: String,
    texto: String,
    r: chat::Resultado,
    e_staff: bool,
) {
    let pior = r.pior();
    match pior {
        Some(Acao::Silenciar) => silenciar_local(conta, chat::SILENCIO_AUTOMATICO_MIN),
        Some(Acao::Banir) => banir_local(conta),
        _ => {}
    }
    let Some(pool) = POOL.get().cloned() else { return };
    tokio::spawn(async move {
        let entregue = pior.is_none_or(|a| a.entrega());
        let palavras = r.achadas.iter().map(|a| a.0.as_str()).collect::<Vec<_>>().join(", ");
        let res: anyhow::Result<()> = async {
            let amigos: bool = sqlx::query_scalar(
                "SELECT EXISTS(SELECT 1 FROM social_amigos WHERE aceito AND \
                 ((lower(de) = lower($1) AND lower(para) = lower($2)) OR (lower(de) = lower($2) AND lower(para) = lower($1))))",
            )
            .bind(&nome)
            .bind(&para)
            .fetch_one(&pool)
            .await
            .unwrap_or(false);
            let id: i64 = sqlx::query_scalar(
                "INSERT INTO chat_historico (conta, nome, zona, texto, mostrado, palavras, acao, entregue, canal, para) \
                 VALUES ($1, $2, $3, $4, $5, $6, $7, $8, 'whisper', $9) RETURNING id",
            )
            .bind(conta)
            .bind(&nome)
            .bind(&zona)
            .bind(&texto)
            .bind(&r.mostrado)
            .bind(&palavras)
            .bind(pior.map_or("", |a| a.texto()))
            .bind(entregue)
            .bind(&para)
            .fetch_one(&pool)
            .await?;
            if entregue {
                PENDENTES
                    .lock()
                    .get_or_insert_with(HashMap::new)
                    .insert(id, (sid, para.clone(), r.mostrado.clone(), agora_s()));
                notificar(
                    &pool,
                    &Notificacao::Sussurro {
                        id,
                        de: nome.clone(),
                        para: para.clone(),
                        texto: r.mostrado.clone(),
                        staff: e_staff,
                        amigos,
                        origem: processo(),
                    },
                )
                .await?;
            }
            if pior == Some(Acao::Banir) {
                sqlx::query("INSERT INTO banimentos (conta, nome, motivo, autor) VALUES ($1, $2, $3, 'auto')")
                    .bind(conta)
                    .bind(&nome)
                    .bind(format!("auto: \"{palavras}\" (whisper #{id})"))
                    .execute(&pool)
                    .await?;
                notificar(&pool, &Notificacao::Expulsar { conta, motivo: "language".into() }).await?;
            }
            Ok(())
        }
        .await;
        if let Err(e) = res {
            tracing::warn!("chat sussurro: {e}");
        }
    });
}

pub fn tomar() -> Vec<Notificacao> {
    std::mem::take(&mut *FILA.lock())
}

async fn recarregar(pool: &PgPool) {
    if let Ok(v) = sqlx::query_as::<_, (String, String)>("SELECT palavra, acao FROM chat_palavras").fetch_all(pool).await {
        *PALAVRAS.write() = v.into_iter().filter_map(|(p, a)| Some((p, Acao::de_texto(&a)?))).collect();
    }
    if let Ok(v) = sqlx::query_as::<_, (i64, i64)>(
        "SELECT conta, EXTRACT(EPOCH FROM MAX(ate))::BIGINT FROM chat_silencios WHERE ate > NOW() GROUP BY conta",
    )
    .fetch_all(pool)
    .await
    {
        *SILENCIOS.write() = Some(v.into_iter().collect());
    }
    if let Ok(v) = sqlx::query_scalar::<_, i64>(
        "SELECT DISTINCT conta FROM banimentos WHERE NOT revogado AND (ate IS NULL OR ate > NOW())",
    )
    .fetch_all(pool)
    .await
    {
        *BANIDOS.write() = Some(v.into_iter().collect());
    }
    if let Ok(v) = sqlx::query_scalar::<_, i64>("SELECT account_id FROM social_staff").fetch_all(pool).await {
        *STAFF.write() = Some(v.into_iter().collect());
    }
}

pub async fn iniciar(pool: PgPool) -> anyhow::Result<()> {
    for t in chat::TABELAS {
        sqlx::query(t).execute(&pool).await?;
    }
    // First run: seed the swear list (censor only); after that the staff owns it.
    let n: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM chat_palavras").fetch_one(&pool).await?;
    if n == 0 {
        for p in chat::PADRAO {
            sqlx::query("INSERT INTO chat_palavras (palavra, acao) VALUES ($1, 'censurar') ON CONFLICT DO NOTHING")
                .bind(p)
                .execute(&pool)
                .await?;
        }
    }
    recarregar(&pool).await;
    let _ = POOL.set(pool.clone());
    let p2 = pool.clone();
    tokio::spawn(async move {
        let mut iv = tokio::time::interval(Duration::from_secs(5));
        loop {
            iv.tick().await;
            recarregar(&p2).await;
        }
    });
    tokio::spawn(async move {
        loop {
            match sqlx::postgres::PgListener::connect_with(&pool).await {
                Ok(mut l) => {
                    if let Err(e) = l.listen(CANAL_NOTIFY).await {
                        tracing::warn!("chat listen: {e}");
                    } else {
                        loop {
                            match l.recv().await {
                                Ok(n) => match serde_json::from_str::<Notificacao>(n.payload()) {
                                    Ok(v) => FILA.lock().push(v),
                                    Err(e) => tracing::warn!("chat notify ruim: {e}"),
                                },
                                Err(e) => {
                                    tracing::warn!("chat listener caiu: {e}");
                                    break;
                                }
                            }
                        }
                    }
                }
                Err(e) => tracing::warn!("chat listener: {e}"),
            }
            tokio::time::sleep(Duration::from_secs(3)).await;
        }
    });
    Ok(())
}

async fn notificar(pool: &PgPool, n: &Notificacao) -> anyhow::Result<()> {
    sqlx::query("SELECT pg_notify($1, $2)")
        .bind(CANAL_NOTIFY)
        .bind(serde_json::to_string(n)?)
        .execute(pool)
        .await?;
    Ok(())
}

/// Saves the line (always) and sends it out (when its words allow), then
/// applies an automatic mute or ban. The world already told the player.
/// `canal` = `shared::chat::CANAL_*`. The party channel is delivered by the
/// world itself (`notificar = false`); this only logs it.
pub fn publicar(conta: i64, nome: String, tag: String, zona: String, texto: String, r: chat::Resultado, e_staff: bool, canal: u8, notificar_: bool) {
    let pior = r.pior();
    match pior {
        Some(Acao::Silenciar) => silenciar_local(conta, chat::SILENCIO_AUTOMATICO_MIN),
        Some(Acao::Banir) => banir_local(conta),
        _ => {}
    }
    let Some(pool) = POOL.get().cloned() else { return };
    tokio::spawn(async move {
        let entregue = pior.is_none_or(|a| a.entrega());
        let palavras = r.achadas.iter().map(|a| a.0.as_str()).collect::<Vec<_>>().join(", ");
        let res: anyhow::Result<()> = async {
            let id: i64 = sqlx::query_scalar(
                "INSERT INTO chat_historico (conta, nome, zona, texto, mostrado, palavras, acao, entregue, canal) \
                 VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9) RETURNING id",
            )
            .bind(conta)
            .bind(&nome)
            .bind(&zona)
            .bind(&texto)
            .bind(&r.mostrado)
            .bind(&palavras)
            .bind(pior.map_or("", |a| a.texto()))
            .bind(entregue)
            .bind(match canal {
                chat::CANAL_ILHA => "ilha",
                chat::CANAL_GRUPO => "grupo",
                _ => "global",
            })
            .fetch_one(&pool)
            .await?;
            if entregue && notificar_ {
                let zona_msg = if canal == chat::CANAL_ILHA { zona.clone() } else { String::new() };
                notificar(
                    &pool,
                    &Notificacao::Mensagem { id, de: nome.clone(), texto: r.mostrado.clone(), staff: e_staff, canal, zona: zona_msg, tag: tag.clone() },
                )
                .await?;
            }
            let motivo = format!("auto: \"{palavras}\" (msg #{id})");
            match pior {
                Some(Acao::Silenciar) => {
                    sqlx::query(
                        "INSERT INTO chat_silencios (conta, nome, ate, motivo, autor) \
                         VALUES ($1, $2, NOW() + make_interval(mins => $3::int), $4, 'auto')",
                    )
                    .bind(conta)
                    .bind(&nome)
                    .bind(chat::SILENCIO_AUTOMATICO_MIN as i32)
                    .bind(&motivo)
                    .execute(&pool)
                    .await?;
                }
                Some(Acao::Banir) => {
                    sqlx::query("INSERT INTO banimentos (conta, nome, motivo, autor) VALUES ($1, $2, $3, 'auto')")
                        .bind(conta)
                        .bind(&nome)
                        .bind(&motivo)
                        .execute(&pool)
                        .await?;
                    notificar(&pool, &Notificacao::Expulsar { conta, motivo: "language".into() }).await?;
                }
                _ => {}
            }
            Ok(())
        }
        .await;
        if let Err(e) = res {
            tracing::warn!("chat publicar: {e}");
        }
    });
}

/// The market changed (`Notificacao::MercadoMudou`): tell every process.
/// The realm pool, once the chat has connected (for small writes elsewhere).
pub fn pool() -> Option<PgPool> {
    POOL.get().cloned()
}

pub fn mercado_mudou(personagens: Vec<String>) {
    let Some(pool) = POOL.get().cloned() else {
        tracing::warn!("market push: chat pool not ready");
        return;
    };
    tokio::spawn(async move {
        let _ = notificar(&pool, &Notificacao::MercadoMudou { personagens }).await;
    });
}

/// A market operation's answer to its character, on whatever channel they
/// are (`Notificacao::MercadoAviso`). `false` = the channel is not ready:
/// the caller answers locally, as before.
pub fn mercado_aviso(personagem: String, ok: bool, texto: String) -> bool {
    let Some(pool) = POOL.get().cloned() else { return false };
    tokio::spawn(async move {
        let _ = notificar(&pool, &Notificacao::MercadoAviso { personagem, ok, texto }).await;
    });
    true
}

/// A player reports a line (once per line per account).
pub fn denunciar(msg_id: i64, conta: i64, nome: String) {
    let Some(pool) = POOL.get().cloned() else { return };
    tokio::spawn(async move {
        let _ = sqlx::query("INSERT INTO chat_denuncias (msg_id, conta, nome) VALUES ($1, $2, $3) ON CONFLICT DO NOTHING")
            .bind(msg_id)
            .bind(conta)
            .bind(&nome)
            .execute(&pool)
            .await;
    });
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn ritmo_segura_rajada_e_repeticao() {
        let c = 987_654;
        assert_eq!(ritmo_em(c, "oi", 100.0), Ok(()));
        assert_eq!(ritmo_em(c, "tudo bem", 100.5), Err(Recusa::Devagar), "too fast");
        assert_eq!(ritmo_em(c, "oi", 103.0), Err(Recusa::Repetida));
        for (i, t) in [103.0, 106.0, 109.0, 112.0].iter().enumerate() {
            assert_eq!(ritmo_em(c, &format!("msg {i}"), *t), Ok(()));
        }
        assert_eq!(ritmo_em(c, "mais uma", 115.0), Err(Recusa::Devagar), "burst cap");
        assert_eq!(ritmo_em(c, "depois", 140.0), Ok(()), "window passed");
    }
}
