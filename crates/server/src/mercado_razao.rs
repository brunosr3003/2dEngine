//! Livro-caixa da TP, no banco CENTRAL (docs/MERCADO.md).
//!
//! TP e' dinheiro de verdade: nunca um saldo sobrescrito. Cada movimento e'
//! uma linha (quem, quanto, por que, saldo depois); o saldo e' a ultima linha
//! da conta. `referencia` unica faz o mesmo movimento pedido duas vezes valer
//! uma so'.
//!
//! Arquivo sem `crate::`: o binario `creditar_tp` inclui ele por `#[path]`.
use anyhow::Result;
use sqlx::{PgPool, Postgres, Transaction};

pub async fn criar_tabela(pool: &PgPool) -> Result<()> {
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS tp_razao (
            id            BIGSERIAL PRIMARY KEY,
            conta         TEXT        NOT NULL,
            delta         BIGINT      NOT NULL,
            saldo_depois  BIGINT      NOT NULL CHECK (saldo_depois >= 0),
            motivo        TEXT        NOT NULL,
            referencia    TEXT        UNIQUE,
            quando        TIMESTAMPTZ NOT NULL DEFAULT NOW()
        )",
    )
    .execute(pool)
    .await?;
    sqlx::query("CREATE INDEX IF NOT EXISTS tp_razao_conta ON tp_razao (conta, id DESC)")
        .execute(pool)
        .await?;
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Movimento {
    Feito { saldo: u64 },
    /// A `referencia` ja' tinha sido lancada: nada muda.
    JaFeito { saldo: u64 },
    /// Debito maior que o saldo: nada muda.
    SemSaldo { saldo: u64 },
}

/// Saldo atual de uma conta.
pub async fn saldo<'e, E: sqlx::PgExecutor<'e>>(exec: E, conta: &str) -> Result<u64> {
    let s: Option<i64> = sqlx::query_scalar("SELECT saldo_depois FROM tp_razao WHERE conta = $1 ORDER BY id DESC LIMIT 1")
        .bind(conta)
        .fetch_optional(exec)
        .await?;
    Ok(s.unwrap_or(0).max(0) as u64)
}

/// Lanca `delta` na conta, dentro da transacao de quem chama. A trava por
/// conta serializa dois movimentos simultaneos da mesma conta.
pub async fn mover(tx: &mut Transaction<'_, Postgres>, conta: &str, delta: i64, motivo: &str, referencia: Option<&str>) -> Result<Movimento> {
    sqlx::query("SELECT pg_advisory_xact_lock(hashtext($1))").bind(conta).execute(&mut **tx).await?;
    if let Some(r) = referencia {
        let feito: Option<i64> = sqlx::query_scalar("SELECT saldo_depois FROM tp_razao WHERE referencia = $1")
            .bind(r)
            .fetch_optional(&mut **tx)
            .await?;
        if feito.is_some() {
            return Ok(Movimento::JaFeito { saldo: saldo(&mut **tx, conta).await? });
        }
    }
    let atual = saldo(&mut **tx, conta).await? as i64;
    let novo = atual.saturating_add(delta);
    if novo < 0 {
        return Ok(Movimento::SemSaldo { saldo: atual as u64 });
    }
    sqlx::query("INSERT INTO tp_razao (conta, delta, saldo_depois, motivo, referencia) VALUES ($1, $2, $3, $4, $5)")
        .bind(conta)
        .bind(delta)
        .bind(novo)
        .bind(motivo)
        .bind(referencia)
        .execute(&mut **tx)
        .await?;
    Ok(Movimento::Feito { saldo: novo as u64 })
}

/// Credito avulso numa transacao propria (TP de teste, compra de TP).
pub async fn creditar(pool: &PgPool, conta: &str, qtd: u64, motivo: &str, referencia: Option<&str>) -> Result<Movimento> {
    let mut tx = pool.begin().await?;
    let m = mover(&mut tx, conta, qtd.min(i64::MAX as u64) as i64, motivo, referencia).await?;
    tx.commit().await?;
    Ok(m)
}
