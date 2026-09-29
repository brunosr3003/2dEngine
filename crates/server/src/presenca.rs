//! Calendario de presenca no banco do realm (docs/CALENDARIO.md).
//!
//! One row per claim in `presenca_resgates`. The database decides who gets
//! the day: the claim locks the ACCOUNT (`pg_advisory_xact_lock`), reads the
//! rows, plans with `shared::presenca::planejar` and inserts, all in one
//! transaction. Two characters of the account on different channels
//! (processes) asking at the same time: one inserts, the other reads the row
//! and is refused. The UNIQUE constraints (one per day, one per grid day)
//! hold even without the lock.
//!
//! Delivery is idempotent like the market's letter: the row is born
//! `aplicado = FALSE`; the channel puts the prize in the bag and the
//! character's save marks `aplicado` in the SAME transaction as the bag. A
//! crash in the middle leaves the row pending, and the account's next login
//! (after `PENDENTE_APOS_S`, so as not to catch a save that is still going to
//! happen) reserves it and delivers again.

use anyhow::Result;
use shared::presenca::{self as pr, Premio, Recusa, ResgateFeito};
use sqlx::{PgPool, Postgres, Row, Transaction};
use tokio::sync::mpsc;

use crate::world::{IncomingMessage, SessionId};

/// A row unapplied for longer than this belongs to a channel that fell before the save.
pub const PENDENTE_APOS_S: i64 = 120;
/// Rows read per account: two full months with room to spare for events.
const LINHAS_LIDAS: i64 = 200;

/// The calendar's clock. `MMO_PRESENCA_TESTE_OFFSET_S` (TEST server) advances
/// the day without touching the machine's clock.
pub fn agora() -> i64 {
    let offset: i64 = std::env::var("MMO_PRESENCA_TESTE_OFFSET_S")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(0);
    let agora = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as i64);
    agora + offset
}

fn pendente_apos_s() -> i64 {
    std::env::var("MMO_PRESENCA_TESTE_PENDENTE_S")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(PENDENTE_APOS_S)
}

pub async fn criar_tabelas(pool: &PgPool) -> Result<()> {
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS presenca_resgates (
            id           TEXT PRIMARY KEY,
            account_id   BIGINT   NOT NULL,
            calendario   INTEGER  NOT NULL,
            ciclo        BIGINT   NOT NULL,
            dia_grade    SMALLINT NOT NULL,
            dia_jogo     BIGINT   NOT NULL,
            personagem   TEXT     NOT NULL,
            premios_json TEXT     NOT NULL,
            aplicado     BOOLEAN  NOT NULL DEFAULT FALSE,
            reservado_em TIMESTAMPTZ NOT NULL DEFAULT NOW(),
            criado       TIMESTAMPTZ NOT NULL DEFAULT NOW(),
            UNIQUE (account_id, calendario, dia_jogo),
            UNIQUE (account_id, calendario, ciclo, dia_grade)
        )",
    )
    .execute(pool)
    .await?;
    sqlx::query("CREATE INDEX IF NOT EXISTS idx_presenca_pendentes ON presenca_resgates (account_id) WHERE NOT aplicado")
        .execute(pool)
        .await?;
    Ok(())
}

pub fn id_do_resgate(conta: i64, p: &pr::Plano) -> String {
    format!("{conta}:{}:{}:{}", p.calendario, p.ciclo, p.dia_grade)
}

fn feito_da_linha(r: &sqlx::postgres::PgRow) -> ResgateFeito {
    ResgateFeito {
        calendario: r.get::<i32, _>("calendario") as u32,
        ciclo: r.get("ciclo"),
        dia_grade: r.get::<i16, _>("dia_grade") as u8,
        dia_jogo: r.get("dia_jogo"),
    }
}

const SELECT_FEITOS: &str =
    "SELECT calendario, ciclo, dia_grade, dia_jogo FROM presenca_resgates WHERE account_id = $1 ORDER BY dia_jogo DESC LIMIT $2";

pub async fn feitos(pool: &PgPool, conta: i64) -> Result<Vec<ResgateFeito>> {
    let rs = sqlx::query(SELECT_FEITOS)
        .bind(conta)
        .bind(LINHAS_LIDAS)
        .fetch_all(pool)
        .await?;
    Ok(rs.iter().map(feito_da_linha).collect())
}

#[derive(Debug, Clone, PartialEq)]
pub enum Resultado {
    Resgatou {
        id: String,
        plano: pr::Plano,
        feitos: Vec<ResgateFeito>,
    },
    Recusado {
        recusa: Recusa,
        feitos: Vec<ResgateFeito>,
    },
}

/// Claims the next prize of `calendario` for `conta`, in the database. Only
/// one request a day wins, across any number of processes.
pub async fn reivindicar(
    pool: &PgPool,
    conta: i64,
    personagem: &str,
    calendario: u32,
    unix: i64,
) -> Result<Resultado> {
    let mut tx = pool.begin().await?;
    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended('presenca:' || $1::text, 0))")
        .bind(conta)
        .execute(&mut *tx)
        .await?;
    let mut feitos: Vec<ResgateFeito> = sqlx::query(SELECT_FEITOS)
        .bind(conta)
        .bind(LINHAS_LIDAS)
        .fetch_all(&mut *tx)
        .await?
        .iter()
        .map(feito_da_linha)
        .collect();
    let plano = match pr::planejar(calendario, unix, pr::EVENTOS, &feitos) {
        Ok(p) => p,
        Err(recusa) => {
            tx.rollback().await?;
            return Ok(Resultado::Recusado { recusa, feitos });
        }
    };
    let id = id_do_resgate(conta, &plano);
    let inseriu = sqlx::query(
        "INSERT INTO presenca_resgates (id, account_id, calendario, ciclo, dia_grade, dia_jogo, personagem, premios_json)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8) ON CONFLICT DO NOTHING",
    )
    .bind(&id)
    .bind(conta)
    .bind(calendario as i32)
    .bind(plano.ciclo)
    .bind(plano.dia_grade as i16)
    .bind(plano.dia_jogo)
    .bind(personagem)
    .bind(serde_json::to_string(&plano.premios)?)
    .execute(&mut *tx)
    .await?
    .rows_affected();
    if inseriu == 0 {
        // Does not happen with the lock; if it does, another row's claim won.
        tx.rollback().await?;
        return Ok(Resultado::Recusado {
            recusa: Recusa::JaResgatouHoje,
            feitos,
        });
    }
    tx.commit().await?;
    feitos.push(ResgateFeito {
        calendario,
        ciclo: plano.ciclo,
        dia_grade: plano.dia_grade,
        dia_jogo: plano.dia_jogo,
    });
    Ok(Resultado::Resgatou { id, plano, feitos })
}

/// Claims of the account that never reached a save (the channel fell):
/// reserves for `personagem` and returns it to be delivered. The reservation
/// is atomic (UPDATE ... RETURNING): two logins at once do not take the same row.
pub async fn reservar_pendentes(
    pool: &PgPool,
    conta: i64,
    personagem: &str,
    apos_s: i64,
) -> Result<Vec<(String, Vec<Premio>)>> {
    let rs = sqlx::query(
        "UPDATE presenca_resgates SET personagem = $2, reservado_em = NOW()
         WHERE account_id = $1 AND NOT aplicado AND reservado_em < NOW() - make_interval(secs => $3)
         RETURNING id, premios_json",
    )
    .bind(conta)
    .bind(personagem)
    .bind(apos_s as f64)
    .fetch_all(pool)
    .await?;
    Ok(rs
        .iter()
        .map(|r| {
            (
                r.get::<String, _>("id"),
                serde_json::from_str(&r.get::<String, _>("premios_json")).unwrap_or_default(),
            )
        })
        .collect())
}

/// Inside the character's save: the prize is already in the recorded bag.
pub async fn marcar_aplicados(tx: &mut Transaction<'_, Postgres>, ids: &[String]) -> Result<()> {
    if ids.is_empty() {
        return Ok(());
    }
    sqlx::query("UPDATE presenca_resgates SET aplicado = TRUE WHERE id = ANY($1) AND NOT aplicado")
        .bind(ids)
        .execute(&mut **tx)
        .await?;
    Ok(())
}

// ─────────────────────────────── bridge with the world ───────────────────────────────

/// What the async tasks return to the world loop.
#[derive(Debug)]
pub enum Evento {
    Estado {
        sid: SessionId,
        feitos: Vec<ResgateFeito>,
    },
    Resgatou {
        sid: SessionId,
        personagem: String,
        id: String,
        plano: pr::Plano,
        feitos: Vec<ResgateFeito>,
    },
    Recusado {
        sid: SessionId,
        texto: String,
        feitos: Option<Vec<ResgateFeito>>,
    },
    Pendentes {
        sid: SessionId,
        personagem: String,
        pendentes: Vec<(String, Vec<Premio>)>,
        feitos: Vec<ResgateFeito>,
    },
}

pub fn spawn_estado(
    pool: PgPool,
    tx: mpsc::UnboundedSender<IncomingMessage>,
    sid: SessionId,
    conta: i64,
) {
    tokio::spawn(async move {
        match feitos(&pool, conta).await {
            Ok(feitos) => {
                let _ = tx.send(IncomingMessage::Presenca(Evento::Estado { sid, feitos }));
            }
            Err(e) => tracing::warn!("attendance: state failed: {e:#}"),
        }
    });
}

pub fn spawn_resgate(
    pool: PgPool,
    tx: mpsc::UnboundedSender<IncomingMessage>,
    sid: SessionId,
    conta: i64,
    personagem: String,
    calendario: u32,
) {
    tokio::spawn(async move {
        let ev = match reivindicar(&pool, conta, &personagem, calendario, agora()).await {
            Ok(Resultado::Resgatou { id, plano, feitos }) => Evento::Resgatou {
                sid,
                personagem,
                id,
                plano,
                feitos,
            },
            Ok(Resultado::Recusado { recusa, feitos }) => Evento::Recusado {
                sid,
                texto: recusa.texto().into(),
                feitos: Some(feitos),
            },
            Err(e) => {
                tracing::warn!("attendance: claim for {personagem} failed: {e:#}");
                Evento::Recusado {
                    sid,
                    texto: "Calendário indisponível agora. Tente de novo.".into(),
                    feitos: None,
                }
            }
        };
        let _ = tx.send(IncomingMessage::Presenca(ev));
    });
}

pub fn spawn_ao_logar(
    pool: PgPool,
    tx: mpsc::UnboundedSender<IncomingMessage>,
    sid: SessionId,
    conta: i64,
    personagem: String,
) {
    tokio::spawn(async move {
        let pendentes = match reservar_pendentes(&pool, conta, &personagem, pendente_apos_s()).await
        {
            Ok(p) => p,
            Err(e) => {
                tracing::warn!("attendance: pending for {personagem} failed: {e:#}");
                Vec::new()
            }
        };
        match feitos(&pool, conta).await {
            Ok(feitos) => {
                let _ = tx.send(IncomingMessage::Presenca(Evento::Pendentes {
                    sid,
                    personagem,
                    pendentes,
                    feitos,
                }));
            }
            Err(e) => tracing::warn!("attendance: state for {personagem} failed: {e:#}"),
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A disposable Postgres: `DATABASE_URL_PRESENCA_TESTE` (or the market's).
    /// Without the variable the test does nothing (and says so).
    #[tokio::test]
    async fn um_resgate_por_dia_mesmo_com_dois_canais_ao_mesmo_tempo() {
        let Ok(url) = std::env::var("DATABASE_URL_PRESENCA_TESTE")
            .or_else(|_| std::env::var("DATABASE_URL_CENTRAL_TESTE"))
        else {
            eprintln!(
                "DATABASE_URL_PRESENCA_TESTE nao setada: teste de integracao da presenca pulado"
            );
            return;
        };
        let pool = sqlx::postgres::PgPoolOptions::new()
            .max_connections(8)
            .connect(&url)
            .await
            .expect("pool");
        criar_tabelas(&pool).await.expect("tabelas");
        let conta: i64 = 9_000_000_000 + fastrand::i64(0..1_000_000_000);
        let t = agora();

        // Two "channels" (tasks with their own connections), same account, same instant.
        let (a, b) = tokio::join!(
            reivindicar(&pool, conta, "ana", pr::MENSAL, t),
            reivindicar(&pool, conta, "bia", pr::MENSAL, t)
        );
        let (a, b) = (a.unwrap(), b.unwrap());
        let vencedores = [&a, &b]
            .iter()
            .filter(|r| matches!(r, Resultado::Resgatou { .. }))
            .count();
        assert_eq!(vencedores, 1, "so' um canal leva o dia: {a:?} / {b:?}");
        let perdedor = if matches!(a, Resultado::Resgatou { .. }) {
            &b
        } else {
            &a
        };
        assert!(matches!(
            perdedor,
            Resultado::Recusado {
                recusa: Recusa::JaResgatouHoje,
                ..
            }
        ));

        // Mesmo dia de novo: recusado. Dia seguinte: dia 2 da grade.
        assert!(matches!(
            reivindicar(&pool, conta, "ana", pr::MENSAL, t + 60)
                .await
                .unwrap(),
            Resultado::Recusado {
                recusa: Recusa::JaResgatouHoje,
                ..
            }
        ));
        match reivindicar(&pool, conta, "ana", pr::MENSAL, t + 86_400)
            .await
            .unwrap()
        {
            Resultado::Resgatou { plano, feitos, .. } => {
                assert_eq!(plano.dia_grade, 2);
                assert_eq!(feitos.len(), 2);
            }
            r => panic!("dia seguinte deveria resgatar: {r:?}"),
        }
        let n: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM presenca_resgates WHERE account_id = $1")
                .bind(conta)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(n, 2);

        // Nothing was applied: both rows are pending. A 1 h window does not catch
        // them (the channel that claimed is still going to save); a 0 window
        // reserves both.
        assert!(reservar_pendentes(&pool, conta, "caio", 3600)
            .await
            .unwrap()
            .is_empty());
        let pend = reservar_pendentes(&pool, conta, "caio", 0).await.unwrap();
        assert_eq!(pend.len(), 2);
        assert!(pend.iter().all(|(_, premios)| !premios.is_empty()));

        // The save marks applied (twice does no harm) and it leaves the pending set.
        let ids: Vec<String> = pend.iter().map(|p| p.0.clone()).collect();
        for _ in 0..2 {
            let mut tx = pool.begin().await.unwrap();
            marcar_aplicados(&mut tx, &ids).await.unwrap();
            tx.commit().await.unwrap();
        }
        assert!(reservar_pendentes(&pool, conta, "caio", 0)
            .await
            .unwrap()
            .is_empty());
        sqlx::query("DELETE FROM presenca_resgates WHERE account_id = $1")
            .bind(conta)
            .execute(&pool)
            .await
            .unwrap();
    }
}
