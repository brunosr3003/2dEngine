//! Official mail: the part that only talks to the DATABASE.
//!
//! It lives in its own crate because two processes use it: the game server
//! (Social panel, for whoever holds a role) and the **panoptico** (the sending
//! page). Duplicating was the alternative — and duplicating an auditable path,
//! one that moves an item into a player's bag, is like having two safes with
//! the same key and separate books.
//!
//! What stayed in the server (`correio_admin.rs`) is what needs the world
//! loop: listing the inbox of someone online and claiming an attachment into the bag.

use anyhow::{bail, Result};
use shared::social::{anexos_validos, texto_valido, Pedido};
use sqlx::{PgConnection, PgPool, Row};

pub async fn init(pool: &PgPool) -> Result<()> {
    for sql in [
        "CREATE TABLE IF NOT EXISTS social_staff (account_id BIGINT PRIMARY KEY REFERENCES accounts(id), cargo TEXT NOT NULL CHECK(cargo IN ('admin','mod')))",
        "CREATE TABLE IF NOT EXISTS social_campanhas (id BIGSERIAL PRIMARY KEY, envio TEXT NOT NULL, autor TEXT NOT NULL, account_id BIGINT NOT NULL, cargo TEXT NOT NULL, destino TEXT, assunto TEXT NOT NULL, texto TEXT NOT NULL, anexos TEXT NOT NULL, quando BIGINT NOT NULL, destinatarios INTEGER NOT NULL DEFAULT 0, UNIQUE(account_id,envio))",
        "CREATE TABLE IF NOT EXISTS social_oficiais (id BIGSERIAL PRIMARY KEY, campanha BIGINT NOT NULL REFERENCES social_campanhas(id), para TEXT NOT NULL REFERENCES characters(name), lida BOOLEAN NOT NULL DEFAULT FALSE, apagada BOOLEAN NOT NULL DEFAULT FALSE, resgatada BOOLEAN NOT NULL DEFAULT FALSE, token TEXT, UNIQUE(campanha,para))",
        "CREATE INDEX IF NOT EXISTS social_oficiais_caixa ON social_oficiais(para,id DESC)",
    ] { sqlx::query(sql).execute(pool).await?; }
    Ok(())
}

pub async fn cargo(db: &mut PgConnection, nome: &str) -> Result<Option<String>> {
    Ok(sqlx::query_scalar("SELECT s.cargo FROM social_staff s JOIN characters c ON c.account_id=s.account_id WHERE c.name=$1")
        .bind(nome).fetch_optional(db).await?)
}

pub async fn enviar(pool: &PgPool, autor: &str, conta: i64, pedido: &Pedido) -> Result<u32> {
    let Pedido::EnviarOficial {
        envio,
        para,
        assunto,
        texto,
        anexos,
    } = pedido
    else {
        bail!("Invalid order.");
    };
    if envio.len() < 8
        || envio.len() > 100
        || !envio.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
    {
        bail!("Identificador de envio inválido.");
    }
    if !texto_valido(assunto, 1, 60) || !texto_valido(texto, 1, 1000) || !anexos_validos(anexos) {
        bail!(
            "Revise assunto, texto e anexos (até 8 itens distintos, de 1 a 100.000 unidades cada)."
        );
    }
    let mut tx = pool.begin().await?;
    // Serialises sends per account (idempotency across channels too).
    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended('correio-admin:' || $1::text,0))")
        .bind(conta)
        .execute(&mut *tx)
        .await?;
    let cargo: Option<String>=sqlx::query_scalar("SELECT s.cargo FROM social_staff s JOIN characters c ON c.account_id=s.account_id WHERE c.name=$1 AND c.account_id=$2 FOR SHARE OF s")
        .bind(autor).bind(conta).fetch_optional(&mut *tx).await?;
    let Some(cargo) = cargo else {
        bail!("Somente administradores e moderadores podem enviar correio oficial.");
    };
    let payload = serde_json::to_string(anexos)?;
    if let Some(r)=sqlx::query("SELECT destino,assunto,texto,anexos,destinatarios FROM social_campanhas WHERE account_id=$1 AND envio=$2")
        .bind(conta).bind(envio).fetch_optional(&mut *tx).await? {
        if r.get::<Option<String>,_>("destino")!=*para || r.get::<String,_>("assunto")!=*assunto || r.get::<String,_>("texto")!=*texto || r.get::<String,_>("anexos")!=payload {bail!("Identificador já usado em outro envio. Crie uma nova carta.");}
        return Ok(r.get::<i32,_>("destinatarios") as u32);
    }
    let mut slots_necessarios = 0u64;
    for a in anexos {
        let stack: Option<i32> =
            sqlx::query_scalar("SELECT stack_max FROM items WHERE id=$1 AND active")
                .bind(a.item_id as i32)
                .fetch_optional(&mut *tx)
                .await?;
        let Some(stack) = stack else {
            bail!("Item {} não existe ou está desativado.", a.item_id);
        };
        slots_necessarios += (a.qtd as u64).div_ceil(stack.max(1) as u64);
    }
    if slots_necessarios > shared::INVENTORY_SLOTS as u64 {
        bail!("Os anexos não cabem nem numa bolsa vazia. Reduza as quantidades ou separe em mais cartas.");
    }
    let alvo = if let Some(nome) = para {
        if !texto_valido(nome, 1, 32) {
            bail!("Nome inválido.");
        }
        let nomes: Vec<String> =
            sqlx::query_scalar("SELECT name FROM characters WHERE lower(name)=lower($1) LIMIT 2")
                .bind(nome.trim())
                .fetch_all(&mut *tx)
                .await?;
        match nomes.as_slice() {
            [n] => Some(n.clone()),
            _ => bail!("Character not found, or the name is ambiguous."),
        }
    } else {
        None
    };
    let id:i64=sqlx::query_scalar("INSERT INTO social_campanhas(envio,autor,account_id,cargo,destino,assunto,texto,anexos,quando) VALUES($1,$2,$3,$4,$5,$6,$7,$8,extract(epoch FROM now())::bigint) RETURNING id")
        .bind(envio).bind(autor).bind(conta).bind(cargo).bind(para).bind(assunto).bind(texto).bind(payload).fetch_one(&mut *tx).await?;
    let n=sqlx::query("INSERT INTO social_oficiais(campanha,para) SELECT $1,name FROM characters WHERE ($2::text IS NULL OR name=$2)")
        .bind(id).bind(alvo).execute(&mut *tx).await?.rows_affected();
    sqlx::query("UPDATE social_campanhas SET destinatarios=$1 WHERE id=$2")
        .bind(n as i32)
        .bind(id)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    tracing::info!("correio oficial campanha={id} autor={autor} destinatarios={n}");
    Ok(n as u32)
}

