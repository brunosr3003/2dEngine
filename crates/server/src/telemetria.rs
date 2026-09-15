//! Telemetria barata do jogo, lida pelo panoptico (docs/PANOPTICO.md).
//!
//! Tres coisas, e nenhuma encosta no tick alem de uma soma:
//!
//!   1. **Contadores** `(tipo, chave) -> soma`: o mundo chama `conta` no ponto
//!      em que a coisa acontece (bicho morto, craft, ouro que entrou). Um
//!      cadeado curto num mapa em memoria. Uma task drena a cada
//!      `TELEMETRIA_FLUSH_S` (60 s) e grava agregado POR MINUTO na tabela
//!      `telemetria` do banco do realm. Gravacao que falha devolve tudo pro
//!      mapa: um minuto de banco fora nao apaga o minuto.
//!   2. **Medidas** (o valor de agora: online, mobs vivos, p99 do tick),
//!      gravadas por minuto em `telemetria_medidas` — ultimo valor vence.
//!   3. **Erros**: toda linha WARN/ERROR do log passa por `CamadaDeErros`, fica
//!      num anel pro retrato ao vivo e vai pra `telemetria_erros`.
//!
//! E banda por sessao: contadores atomicos por conexao, sem cadeado no envio.
//!
//! Retencao: contadores e medidas 30 dias, erros 7 dias.

use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use parking_lot::Mutex;
use serde::Serialize;
use sqlx::PgPool;

type Chave = (&'static str, String);

static CONTADORES: Mutex<Option<HashMap<Chave, i64>>> = Mutex::new(None);
static MEDIDAS: Mutex<Option<HashMap<&'static str, f64>>> = Mutex::new(None);
/// Os ultimos erros, pro retrato ao vivo.
static ERROS: Mutex<VecDeque<Erro>> = Mutex::new(VecDeque::new());
/// Erros ainda nao gravados no banco.
static ERROS_A_GRAVAR: Mutex<VecDeque<Erro>> = Mutex::new(VecDeque::new());
static BANDA: Mutex<Option<HashMap<String, Arc<Banda>>>> = Mutex::new(None);
static INICIO: std::sync::OnceLock<Instant> = std::sync::OnceLock::new();

/// Quantos erros o anel ao vivo guarda.
const ERROS_AO_VIVO: usize = 60;
/// Teto dos que esperam gravacao: log em tempestade nao pode comer a memoria.
const ERROS_PENDENTES_MAX: usize = 500;
/// Mensagem de log cortada aqui: o painel mostra, nao arquiva stack trace.
const MSG_MAX: usize = 400;

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Erro {
    /// Unix, segundos.
    pub quando: i64,
    pub nivel: &'static str,
    pub alvo: String,
    pub msg: String,
}

/// Soma `n` em `(tipo, chave)`. Chave vazia quando o tipo e' um numero so'.
pub fn conta(tipo: &'static str, chave: impl ToString, n: i64) {
    if n == 0 {
        return;
    }
    let chave = chave.to_string();
    let mut g = CONTADORES.lock();
    *g.get_or_insert_with(HashMap::new).entry((tipo, chave)).or_insert(0) += n;
}

/// O valor de agora de uma medida (o ultimo vence).
pub fn medir(nome: &'static str, valor: f64) {
    MEDIDAS.lock().get_or_insert_with(HashMap::new).insert(nome, valor);
}

/// Tira tudo que foi somado ate' aqui.
pub fn drenar() -> HashMap<Chave, i64> {
    CONTADORES.lock().take().unwrap_or_default()
}

/// Devolve o que nao deu pra gravar, somando com o que chegou nesse meio tempo.
pub fn devolver(m: HashMap<Chave, i64>) {
    let mut g = CONTADORES.lock();
    let atual = g.get_or_insert_with(HashMap::new);
    for (k, v) in m {
        *atual.entry(k).or_insert(0) += v;
    }
}

fn drenar_medidas() -> HashMap<&'static str, f64> {
    MEDIDAS.lock().take().unwrap_or_default()
}

/// Uma linha de log WARN/ERROR.
pub fn registrar_erro(nivel: &'static str, alvo: &str, msg: String) {
    let mut msg = msg;
    if msg.len() > MSG_MAX {
        let mut corte = MSG_MAX;
        while !msg.is_char_boundary(corte) {
            corte -= 1;
        }
        msg.truncate(corte);
        msg.push('…');
    }
    let e = Erro { quando: agora_unix(), nivel, alvo: alvo.to_string(), msg };
    {
        let mut anel = ERROS.lock();
        if anel.len() >= ERROS_AO_VIVO {
            anel.pop_front();
        }
        anel.push_back(e.clone());
    }
    {
        let mut fila = ERROS_A_GRAVAR.lock();
        if fila.len() >= ERROS_PENDENTES_MAX {
            fila.pop_front();
        }
        fila.push_back(e);
    }
    conta("log", nivel, 1);
}

/// Os erros recentes, do mais velho pro mais novo.
pub fn erros_recentes() -> Vec<Erro> {
    ERROS.lock().iter().cloned().collect()
}

fn drenar_erros() -> Vec<Erro> {
    ERROS_A_GRAVAR.lock().drain(..).collect()
}

fn devolver_erros(v: Vec<Erro>) {
    let mut fila = ERROS_A_GRAVAR.lock();
    for e in v.into_iter().rev() {
        if fila.len() >= ERROS_PENDENTES_MAX {
            break;
        }
        fila.push_front(e);
    }
}

fn agora_unix() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs() as i64)
}

/// Segundos desde que o processo marcou o inicio (`marcar_inicio`).
pub fn uptime_s() -> u64 {
    INICIO.get().map_or(0, |i| i.elapsed().as_secs())
}

pub fn marcar_inicio() {
    let _ = INICIO.set(Instant::now());
}

// ─────────────────────────────── banda ───────────────────────────────

/// Bytes de UMA conexao. Atomicos: a task de envio soma sem cadeado.
#[derive(Debug)]
pub struct Banda {
    enviados: AtomicU64,
    recebidos: AtomicU64,
    desde: Instant,
}

impl Banda {
    pub fn enviou(&self, n: usize) {
        self.enviados.fetch_add(n as u64, Ordering::Relaxed);
    }
    pub fn recebeu(&self, n: usize) {
        self.recebidos.fetch_add(n as u64, Ordering::Relaxed);
    }
}

/// Registra a conexao `peer` e devolve o contador dela.
pub fn abrir_banda(peer: &str) -> Arc<Banda> {
    let b = Arc::new(Banda { enviados: AtomicU64::new(0), recebidos: AtomicU64::new(0), desde: Instant::now() });
    BANDA.lock().get_or_insert_with(HashMap::new).insert(peer.to_string(), b.clone());
    conta("conexao", "aberta", 1);
    b
}

/// A conexao fechou: o que ela trafegou vai pros contadores.
pub fn fechar_banda(peer: &str) {
    let b = BANDA.lock().as_mut().and_then(|m| m.remove(peer));
    if let Some(b) = b {
        conta("banda", "enviados_bytes", b.enviados.load(Ordering::Relaxed) as i64);
        conta("banda", "recebidos_bytes", b.recebidos.load(Ordering::Relaxed) as i64);
    }
    conta("conexao", "fechada", 1);
}

/// (enviados, recebidos, segundos conectado) de uma conexao viva.
pub fn banda_de(peer: &str) -> Option<(u64, u64, f64)> {
    let g = BANDA.lock();
    let b = g.as_ref()?.get(peer)?;
    Some((b.enviados.load(Ordering::Relaxed), b.recebidos.load(Ordering::Relaxed), b.desde.elapsed().as_secs_f64()))
}

// ─────────────────────────────── log ───────────────────────────────

/// Camada do `tracing` que copia WARN e ERROR pro anel e pro banco.
pub struct CamadaDeErros;

struct Mensagem(String);

impl tracing::field::Visit for Mensagem {
    fn record_debug(&mut self, campo: &tracing::field::Field, valor: &dyn std::fmt::Debug) {
        use std::fmt::Write;
        if campo.name() == "message" {
            let _ = write!(self.0, "{valor:?}");
        } else {
            let _ = write!(self.0, " {}={valor:?}", campo.name());
        }
    }
    fn record_str(&mut self, campo: &tracing::field::Field, valor: &str) {
        use std::fmt::Write;
        if campo.name() == "message" {
            self.0.push_str(valor);
        } else {
            let _ = write!(self.0, " {}={valor}", campo.name());
        }
    }
}

impl<S: tracing::Subscriber> tracing_subscriber::Layer<S> for CamadaDeErros {
    fn on_event(&self, ev: &tracing::Event<'_>, _ctx: tracing_subscriber::layer::Context<'_, S>) {
        let nivel = *ev.metadata().level();
        // Em `tracing`, mais verboso e' MAIOR: INFO > WARN > ERROR.
        if nivel > tracing::Level::WARN {
            return;
        }
        let mut m = Mensagem(String::new());
        ev.record(&mut m);
        registrar_erro(if nivel == tracing::Level::ERROR { "error" } else { "warn" }, ev.metadata().target(), m.0);
    }
}

// ─────────────────────────────── banco ───────────────────────────────

pub async fn init(pool: &PgPool) -> anyhow::Result<()> {
    marcar_inicio();
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS telemetria (
            minuto TIMESTAMPTZ NOT NULL,
            canal  TEXT NOT NULL,
            tipo   TEXT NOT NULL,
            chave  TEXT NOT NULL DEFAULT '',
            valor  BIGINT NOT NULL DEFAULT 0,
            PRIMARY KEY (minuto, canal, tipo, chave)
        )",
    )
    .execute(pool)
    .await?;
    sqlx::query("CREATE INDEX IF NOT EXISTS telemetria_tipo_minuto ON telemetria (tipo, minuto)").execute(pool).await?;
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS telemetria_medidas (
            minuto TIMESTAMPTZ NOT NULL,
            canal  TEXT NOT NULL,
            nome   TEXT NOT NULL,
            valor  DOUBLE PRECISION NOT NULL,
            PRIMARY KEY (minuto, canal, nome)
        )",
    )
    .execute(pool)
    .await?;
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS telemetria_erros (
            id     BIGSERIAL PRIMARY KEY,
            quando TIMESTAMPTZ NOT NULL,
            canal  TEXT NOT NULL,
            nivel  TEXT NOT NULL,
            alvo   TEXT NOT NULL,
            msg    TEXT NOT NULL
        )",
    )
    .execute(pool)
    .await?;
    sqlx::query("CREATE INDEX IF NOT EXISTS telemetria_erros_quando ON telemetria_erros (quando)").execute(pool).await?;
    Ok(())
}

/// Identidade deste processo nas tabelas: `REALM/CANAL`, igual `channels.id`.
pub fn canal() -> String {
    format!("{}/{}", crate::canais::realm(), std::env::var("MMO_CANAL").unwrap_or_else(|_| "1".into()))
}

/// Task que grava a cada `TELEMETRIA_FLUSH_S` segundos (60 por padrao).
pub fn spawn(pool: PgPool) {
    tokio::spawn(async move {
        let passo = std::env::var("TELEMETRIA_FLUSH_S").ok().and_then(|v| v.parse::<u64>().ok()).unwrap_or(60).max(1);
        let canal = canal();
        let mut limpou_em: Option<Instant> = None;
        loop {
            tokio::time::sleep(Duration::from_secs(passo)).await;
            let contadores = drenar();
            let medidas = drenar_medidas();
            let erros = drenar_erros();
            if let Err(e) = gravar(&pool, &canal, &contadores, &medidas, &erros).await {
                devolver(contadores);
                devolver_erros(erros);
                // Depois de devolver: o warn passa pela camada, que soma no mapa.
                tracing::warn!("telemetria: nao gravou ({e}) — tenta de novo no proximo minuto");
                continue;
            }
            if limpou_em.is_none_or(|t| t.elapsed() > Duration::from_secs(3600)) {
                limpou_em = Some(Instant::now());
                let _ = sqlx::query("DELETE FROM telemetria WHERE minuto < NOW() - INTERVAL '30 days'").execute(&pool).await;
                let _ = sqlx::query("DELETE FROM telemetria_medidas WHERE minuto < NOW() - INTERVAL '30 days'").execute(&pool).await;
                let _ = sqlx::query("DELETE FROM telemetria_erros WHERE quando < NOW() - INTERVAL '7 days'").execute(&pool).await;
            }
        }
    });
}

async fn gravar(
    pool: &PgPool,
    canal: &str,
    contadores: &HashMap<Chave, i64>,
    medidas: &HashMap<&'static str, f64>,
    erros: &[Erro],
) -> anyhow::Result<()> {
    if contadores.is_empty() && medidas.is_empty() && erros.is_empty() {
        return Ok(());
    }
    let mut tx = pool.begin().await?;
    if !contadores.is_empty() {
        let (mut tipos, mut chaves, mut valores) = (Vec::new(), Vec::new(), Vec::new());
        for ((t, c), v) in contadores {
            tipos.push(t.to_string());
            chaves.push(c.clone());
            valores.push(*v);
        }
        sqlx::query(
            "INSERT INTO telemetria (minuto, canal, tipo, chave, valor)
             SELECT date_trunc('minute', NOW()), $1, t, c, v
               FROM UNNEST($2::text[], $3::text[], $4::bigint[]) AS x(t, c, v)
             ON CONFLICT (minuto, canal, tipo, chave) DO UPDATE SET valor = telemetria.valor + EXCLUDED.valor",
        )
        .bind(canal)
        .bind(&tipos)
        .bind(&chaves)
        .bind(&valores)
        .execute(&mut *tx)
        .await?;
    }
    if !medidas.is_empty() {
        let (nomes, valores): (Vec<String>, Vec<f64>) = medidas.iter().map(|(n, v)| (n.to_string(), *v)).unzip();
        sqlx::query(
            "INSERT INTO telemetria_medidas (minuto, canal, nome, valor)
             SELECT date_trunc('minute', NOW()), $1, n, v
               FROM UNNEST($2::text[], $3::float8[]) AS x(n, v)
             ON CONFLICT (minuto, canal, nome) DO UPDATE SET valor = EXCLUDED.valor",
        )
        .bind(canal)
        .bind(&nomes)
        .bind(&valores)
        .execute(&mut *tx)
        .await?;
    }
    if !erros.is_empty() {
        let quando: Vec<i64> = erros.iter().map(|e| e.quando).collect();
        let nivel: Vec<String> = erros.iter().map(|e| e.nivel.to_string()).collect();
        let alvo: Vec<String> = erros.iter().map(|e| e.alvo.clone()).collect();
        let msg: Vec<String> = erros.iter().map(|e| e.msg.clone()).collect();
        sqlx::query(
            "INSERT INTO telemetria_erros (quando, canal, nivel, alvo, msg)
             SELECT to_timestamp(q), $1, n, a, m
               FROM UNNEST($2::bigint[], $3::text[], $4::text[], $5::text[]) AS x(q, n, a, m)",
        )
        .bind(canal)
        .bind(&quando)
        .bind(&nivel)
        .bind(&alvo)
        .bind(&msg)
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;
    Ok(())
}

#[cfg(test)]
mod testes {
    use super::*;

    // Os estaticos sao do processo inteiro e os testes rodam em paralelo:
    // cada teste usa um `tipo` so' dele e so' olha o que e' dele.

    #[test]
    fn conta_soma_por_chave_e_drenar_esvazia() {
        conta("t_soma", "a", 2);
        conta("t_soma", "a", 3);
        conta("t_soma", "b", 1);
        conta("t_soma", "zero", 0);
        let m = drenar();
        assert_eq!(m.get(&("t_soma", "a".into())), Some(&5));
        assert_eq!(m.get(&("t_soma", "b".into())), Some(&1));
        assert!(!m.contains_key(&("t_soma", "zero".into())), "zero nao cria linha");
        let outra = drenar();
        assert!(outra.keys().all(|k| k.0 != "t_soma"), "drenar leva tudo");
        // Devolve o que nao era deste teste, pra nao atrapalhar os outros.
        devolver(m.into_iter().filter(|(k, _)| k.0 != "t_soma").collect());
        devolver(outra);
    }

    #[test]
    fn devolver_soma_com_o_que_chegou_depois() {
        conta("t_devolve", "x", 4);
        let tirado: HashMap<Chave, i64> = drenar().into_iter().filter(|(k, _)| k.0 == "t_devolve").collect();
        conta("t_devolve", "x", 1);
        devolver(tirado);
        let m = drenar();
        assert_eq!(m.get(&("t_devolve", "x".into())), Some(&5));
        devolver(m.into_iter().filter(|(k, _)| k.0 != "t_devolve").collect());
    }

    #[test]
    fn erro_longo_e_cortado_e_o_anel_tem_teto() {
        for i in 0..(ERROS_AO_VIVO + 10) {
            registrar_erro("warn", "teste_anel", format!("linha {i}"));
        }
        let anel = erros_recentes();
        assert!(anel.len() <= ERROS_AO_VIVO);
        registrar_erro("error", "teste_corte", "ã".repeat(MSG_MAX));
        let ultimo = erros_recentes().into_iter().rev().find(|e| e.alvo == "teste_corte").unwrap();
        assert!(ultimo.msg.len() <= MSG_MAX + '…'.len_utf8());
        assert!(ultimo.msg.ends_with('…'));
    }

    #[test]
    fn banda_conta_por_conexao_e_some_ao_fechar() {
        let b = abrir_banda("teste:1");
        b.enviou(100);
        b.enviou(20);
        b.recebeu(7);
        let (env, rec, _) = banda_de("teste:1").unwrap();
        assert_eq!((env, rec), (120, 7));
        fechar_banda("teste:1");
        assert!(banda_de("teste:1").is_none());
    }
}
