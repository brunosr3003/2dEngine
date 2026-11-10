//! Scheduled global announcements (owner, 10/11/2026: "send this message at
//! 12:00, 13:00 etc and see everything that is programmed").
//!
//! The panel keeps the schedule in `avisos_agendados` and a loop here, every
//! 20 s, drops the ones that are due into `avisos_globais` — the same table the
//! "aviso global" tab writes and every game process already reads. So nothing
//! changes in the game, and the old crontab line on the VPS can go.
//!
//! Two kinds: fixed times of day (São Paulo time, e.g. 12:00, 18:30) or every
//! N minutes counted from the start. Optional start and end. A message that
//! was due more than 5 min ago (panel down, paused) is skipped, not sent late.

use axum::extract::State;
use axum::response::IntoResponse;

use crate::Estado;

const FUSO: &str = "America/Sao_Paulo";

pub const CRIAR: &str = "CREATE TABLE IF NOT EXISTS avisos_agendados (
    id BIGSERIAL PRIMARY KEY,
    texto TEXT NOT NULL,
    autor TEXT NOT NULL DEFAULT '',
    horarios TEXT[] NOT NULL DEFAULT '{}',
    a_cada_min INT,
    inicio TIMESTAMPTZ,
    fim TIMESTAMPTZ,
    ativo BOOLEAN NOT NULL DEFAULT TRUE,
    ultimo_envio TIMESTAMPTZ,
    envios INT NOT NULL DEFAULT 0,
    criado TIMESTAMPTZ NOT NULL DEFAULT NOW()
)";

/// Last moment the row was due (NULL = never yet).
fn ultimo_devido() -> String {
    format!(
        "(CASE WHEN a.a_cada_min IS NOT NULL THEN \
            CASE WHEN COALESCE(a.inicio, a.criado) > NOW() THEN NULL ELSE \
              COALESCE(a.inicio, a.criado) + floor(extract(epoch FROM NOW() - COALESCE(a.inicio, a.criado)) / (a.a_cada_min * 60)) * make_interval(mins => a.a_cada_min) END \
          ELSE (SELECT max(x) FROM (SELECT (((NOW() AT TIME ZONE '{FUSO}')::date - dd) + h::time) AT TIME ZONE '{FUSO}' AS x \
                FROM unnest(a.horarios) h, generate_series(0, 1) dd) s WHERE x <= NOW()) END)"
    )
}

/// Next moment it will go out (NULL = paused, over, or nothing ahead).
fn proximo() -> String {
    format!(
        "(SELECT CASE WHEN p IS NULL OR (a.fim IS NOT NULL AND p >= a.fim) THEN NULL ELSE p END FROM (SELECT \
          CASE WHEN NOT a.ativo OR (a.fim IS NOT NULL AND a.fim <= NOW()) THEN NULL \
          WHEN a.a_cada_min IS NOT NULL THEN \
            CASE WHEN COALESCE(a.inicio, a.criado) > NOW() THEN COALESCE(a.inicio, a.criado) ELSE \
              COALESCE(a.inicio, a.criado) + (floor(extract(epoch FROM NOW() - COALESCE(a.inicio, a.criado)) / (a.a_cada_min * 60)) + 1) * make_interval(mins => a.a_cada_min) END \
          ELSE (SELECT min(x) FROM (SELECT (((GREATEST(NOW(), COALESCE(a.inicio, NOW())) AT TIME ZONE '{FUSO}')::date + dd) + h::time) AT TIME ZONE '{FUSO}' AS x \
                FROM unnest(a.horarios) h, generate_series(0, 1) dd) s WHERE x > NOW() AND (a.inicio IS NULL OR x >= a.inicio)) END AS p) q)"
    )
}

/// One pass: claims every due row and writes its announcement.
pub async fn passo(pool: &sqlx::PgPool) -> Result<u64, sqlx::Error> {
    let devido = ultimo_devido();
    let sql = format!(
        "WITH d AS (UPDATE avisos_agendados a SET ultimo_envio = NOW(), envios = a.envios + 1 \
           WHERE a.ativo AND (a.inicio IS NULL OR a.inicio <= NOW()) AND (a.fim IS NULL OR a.fim > NOW()) \
             AND {devido} IS NOT NULL \
             AND {devido} > COALESCE(a.ultimo_envio, a.criado - interval '1 second') \
             AND {devido} > NOW() - interval '5 minutes' \
           RETURNING a.texto, a.autor) \
         INSERT INTO avisos_globais (texto, autor) SELECT texto, autor FROM d"
    );
    Ok(sqlx::query(&sql).execute(pool).await?.rows_affected())
}

pub fn iniciar(pool: std::sync::Arc<sqlx::PgPool>) {
    tokio::spawn(async move {
        let _ = sqlx::query(CRIAR).execute(&*pool).await;
        let mut t = tokio::time::interval(std::time::Duration::from_secs(20));
        loop {
            t.tick().await;
            let _ = sqlx::query(crate::CRIAR_AVISOS).execute(&*pool).await;
            match passo(&pool).await {
                Ok(0) => {}
                Ok(n) => tracing::info!("avisos agendados: {n} enviado(s)"),
                Err(e) => tracing::warn!("avisos agendados: {e}"),
            }
        }
    });
}

fn resposta(ok: bool, extra: serde_json::Value) -> axum::response::Response {
    let status = if ok { axum::http::StatusCode::OK } else { axum::http::StatusCode::BAD_REQUEST };
    let mut v = extra;
    v["ok"] = ok.into();
    (status, axum::Json(v)).into_response()
}

fn erro(msg: impl Into<String>) -> axum::response::Response {
    resposta(false, serde_json::json!({ "erro": msg.into() }))
}

/// "9:5" → "09:05"; sorted, no repeats.
pub fn normaliza_horarios(lista: &[String]) -> Result<Vec<String>, String> {
    let mut v = Vec::new();
    for h in lista.iter().map(|h| h.trim()).filter(|h| !h.is_empty()) {
        let (a, b) = h.split_once(':').ok_or_else(|| format!("horário inválido: {h} (use HH:MM)"))?;
        let (a, b): (u32, u32) = match (a.parse(), b.parse()) {
            (Ok(a), Ok(b)) if a < 24 && b < 60 => (a, b),
            _ => return Err(format!("horário inválido: {h} (use HH:MM)")),
        };
        v.push(format!("{a:02}:{b:02}"));
    }
    v.sort();
    v.dedup();
    if v.len() > 96 {
        return Err("no máximo 96 horários".into());
    }
    Ok(v)
}

#[derive(serde::Deserialize)]
pub struct Pedido {
    #[serde(default)]
    id: Option<i64>,
    texto: String,
    #[serde(default)]
    autor: String,
    #[serde(default)]
    horarios: Vec<String>,
    #[serde(default)]
    a_cada_min: Option<i32>,
    /// "YYYY-MM-DDTHH:MM" in São Paulo time, or empty.
    #[serde(default)]
    inicio: String,
    #[serde(default)]
    fim: String,
}

fn data_local(s: &str) -> Option<String> {
    let s = s.trim();
    (!s.is_empty()).then(|| s.to_string())
}

pub async fn salvar(State(st): State<Estado>, axum::Json(p): axum::Json<Pedido>) -> impl IntoResponse {
    let texto = p.texto.trim().replace(['\n', '\r'], " ");
    if texto.is_empty() || texto.chars().count() > 200 {
        return erro("texto vazio ou com mais de 200 caracteres");
    }
    let horarios = match normaliza_horarios(&p.horarios) {
        Ok(h) => h,
        Err(e) => return erro(e),
    };
    let a_cada = p.a_cada_min.filter(|&m| m > 0);
    match (horarios.is_empty(), a_cada) {
        (true, None) => return erro("escolha horários fixos ou um intervalo"),
        (false, Some(_)) => return erro("use horários fixos OU intervalo, não os dois"),
        (_, Some(m)) if !(5..=10080).contains(&m) => return erro("intervalo entre 5 min e 7 dias"),
        _ => {}
    }
    let (inicio, fim) = (data_local(&p.inicio), data_local(&p.fim));
    let _ = sqlx::query(CRIAR).execute(&*st.pool).await;
    let sql_editar = format!(
        "UPDATE avisos_agendados SET texto = $2, autor = $3, horarios = $4, a_cada_min = $5, \
           inicio = $6::timestamp AT TIME ZONE '{FUSO}', fim = $7::timestamp AT TIME ZONE '{FUSO}' \
         WHERE id = $1 RETURNING id"
    );
    let sql_criar = format!(
        "INSERT INTO avisos_agendados (texto, autor, horarios, a_cada_min, inicio, fim) \
         VALUES ($1, $2, $3, $4, $5::timestamp AT TIME ZONE '{FUSO}', $6::timestamp AT TIME ZONE '{FUSO}') RETURNING id"
    );
    let r = match p.id {
        Some(id) => sqlx::query_scalar::<_, i64>(&sql_editar).bind(id),
        None => sqlx::query_scalar::<_, i64>(&sql_criar),
    };
    let r = r
        .bind(&texto)
        .bind(p.autor.trim())
        .bind(&horarios)
        .bind(a_cada)
        .bind(inicio)
        .bind(fim)
        .fetch_optional(&*st.pool)
        .await;
    match r {
        Ok(Some(id)) => resposta(true, serde_json::json!({ "id": id })),
        Ok(None) => erro("agendamento não existe mais"),
        Err(e) => erro(e.to_string()),
    }
}

#[derive(serde::Deserialize)]
pub struct Acao {
    id: i64,
    acao: String,
}

pub async fn acao(State(st): State<Estado>, axum::Json(a): axum::Json<Acao>) -> impl IntoResponse {
    let sql = match a.acao.as_str() {
        "pausar" => "UPDATE avisos_agendados SET ativo = FALSE WHERE id = $1",
        // Back on: what was due while paused is not sent late.
        "ativar" => "UPDATE avisos_agendados SET ativo = TRUE, ultimo_envio = NOW() WHERE id = $1",
        "apagar" => "DELETE FROM avisos_agendados WHERE id = $1",
        "enviar" => {
            "WITH d AS (UPDATE avisos_agendados SET envios = envios + 1 WHERE id = $1 RETURNING texto, autor) \
             INSERT INTO avisos_globais (texto, autor) SELECT texto, autor FROM d"
        }
        _ => return erro("ação desconhecida"),
    };
    let _ = sqlx::query(crate::CRIAR_AVISOS).execute(&*st.pool).await;
    match sqlx::query(sql).bind(a.id).execute(&*st.pool).await {
        Ok(r) if r.rows_affected() > 0 => resposta(true, serde_json::json!({})),
        Ok(_) => erro("agendamento não existe mais"),
        Err(e) => erro(e.to_string()),
    }
}

pub fn sql_lista() -> String {
    let fmt = |c: &str| format!("to_char(({c}) AT TIME ZONE '{FUSO}', 'DD/MM HH24:MI')");
    let fmt_campo = |c: &str| format!("to_char({c} AT TIME ZONE '{FUSO}', 'YYYY-MM-DD\"T\"HH24:MI')");
    format!(
        "SELECT a.id, a.texto, a.autor, a.horarios, a.a_cada_min, a.ativo, a.envios, \
           {ultimo}, {prox}, {ini}, {fim}, ({fim_passou}) \
         FROM avisos_agendados a ORDER BY a.ativo DESC, a.id",
        ultimo = fmt("a.ultimo_envio"),
        prox = fmt(&proximo()),
        ini = fmt_campo("a.inicio"),
        fim = fmt_campo("a.fim"),
        fim_passou = "a.fim IS NOT NULL AND a.fim <= NOW()",
    )
}

pub async fn lista(State(st): State<Estado>) -> impl IntoResponse {
    let _ = sqlx::query(CRIAR).execute(&*st.pool).await;
    let sql = sql_lista();
    type Linha = (
        i64, String, String, Vec<String>, Option<i32>, bool, i32,
        Option<String>, Option<String>, Option<String>, Option<String>, bool,
    );
    match sqlx::query_as::<_, Linha>(&sql).fetch_all(&*st.pool).await {
        Ok(v) => resposta(
            true,
            serde_json::json!({ "agendados": v.into_iter().map(
                |(id, texto, autor, horarios, a_cada_min, ativo, envios, ultimo, proximo, inicio, fim, acabou)| serde_json::json!({
                    "id": id, "texto": texto, "autor": autor, "horarios": horarios, "a_cada_min": a_cada_min,
                    "ativo": ativo, "envios": envios, "ultimo": ultimo, "proximo": proximo,
                    "inicio": inicio, "fim": fim, "acabou": acabou,
                })).collect::<Vec<_>>() }),
        ),
        Err(e) => resposta(false, serde_json::json!({ "erro": e.to_string() })),
    }
}

#[cfg(test)]
mod testes {
    use super::*;
    #[test]
    #[ignore]
    fn imprime_sql() {
        println!("{};\n{};", sql_lista(), proximo());
    }
    #[test]
    fn horarios_normalizados() {
        let v = |s: &[&str]| s.iter().map(|x| x.to_string()).collect::<Vec<_>>();
        assert_eq!(normaliza_horarios(&v(&["13:00", "9:5", " ", "13:00"])).unwrap(), v(&["09:05", "13:00"]));
        assert!(normaliza_horarios(&v(&["24:00"])).is_err());
        assert!(normaliza_horarios(&v(&["12h"])).is_err());
    }
}
