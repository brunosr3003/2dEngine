//! Mercado global (docs/MERCADO.md): o banco CENTRAL, a custodia, as cartas e
//! o relay que liga o realm ao central.
//!
//! Dois bancos nao tem transacao comum, entao nada aqui "debita dos dois e
//! torce":
//!
//! - **Realm → central**: o que sai do personagem (item anunciado, gold de uma
//!   compra) e' gravado NA MESMA transacao do save do personagem, como uma
//!   linha em `mercado_saida`. O relay manda pro central; o central processa
//!   cada operacao UMA vez (`mercado_operacoes`, id unico). Caiu no meio? A
//!   linha continua la' e vai de novo — o central reconhece e nao repete.
//! - **Central → realm**: tudo que volta pro personagem (item comprado, gold
//!   da venda, item de anuncio cancelado, reembolso) e' uma CARTA com id unico.
//!   Aplicar a carta grava `mercado_cartas_aplicadas` na mesma transacao do
//!   save; so' depois o central marca `entregue`. Aplicar duas vezes vale uma.
//! - **TP**: mora so' no central (`mercado_razao`), movida dentro da mesma
//!   transacao do anuncio.
use std::collections::HashSet;
use std::time::Duration;

use anyhow::Result;
use once_cell::sync::{Lazy, OnceCell};
use serde::{Deserialize, Serialize};
use shared::items::ItemInstance;
use shared::mercado::{self as regras, AnuncioNet, CartaNet, FiltroNet, PedidoNet, Recusa, VendaNet};
use sqlx::postgres::PgPoolOptions;
use sqlx::{PgPool, Postgres, Row, Transaction};
use tokio::sync::{mpsc, Notify};

use crate::mercado_razao as razao;
use crate::world::{IncomingMessage, SessionId};

static CENTRAL: OnceCell<PgPool> = OnceCell::new();
static ACORDA: Lazy<Notify> = Lazy::new(Notify::new);

/// Pool do banco central, se o mercado estiver ligado neste processo.
pub fn central() -> Option<PgPool> {
    CENTRAL.get().cloned()
}

/// Um save com registro do mercado acabou de gravar: o relay manda ja'.
pub fn acordar_relay() {
    ACORDA.notify_one();
}

/// Id de operacao/anuncio: 128 bits aleatorios em hex.
pub fn novo_id() -> String {
    format!("{:016x}{:016x}", fastrand::u64(..), fastrand::u64(..))
}

/// A conta como o central conhece: o realm e o id da conta nele. Cadastro
/// unico de contas ainda nao existe; um merge de realm remapeia estas chaves.
pub fn conta_global(realm: &str, account_id: Option<i64>, personagem: &str) -> String {
    match account_id {
        Some(id) => format!("{realm}:{id}"),
        None => format!("{realm}:personagem:{personagem}"),
    }
}

// ─────────────────────────────── schema ───────────────────────────────

/// Tabelas do realm e, com `DATABASE_URL_CENTRAL`, o central. Sem ela o
/// mercado fica desligado com aviso — o jogo segue.
pub async fn init(realm: &PgPool) -> Result<()> {
    {
        // Mesma trava das migrations do realm (`persistence::open_pool`):
        // canais sobem juntos.
        let mut trava = realm.acquire().await?;
        sqlx::query("SELECT pg_advisory_lock(728431)")
            .execute(&mut *trava)
            .await?;
        let r = criar_tabelas_do_realm(realm).await;
        let _ = sqlx::query("SELECT pg_advisory_unlock(728431)")
            .execute(&mut *trava)
            .await;
        r?;
    }
    let url = std::env::var("DATABASE_URL_CENTRAL").unwrap_or_default();
    if url.trim().is_empty() {
        tracing::warn!(
            "market: DATABASE_URL_CENTRAL not set — global market OFF in this process"
        );
        return Ok(());
    }
    match abrir_central(&url).await {
        Ok(pool) => {
            let _ = CENTRAL.set(pool);
            tracing::info!("market: central database connected — global market on");
        }
        Err(e) => {
            tracing::error!("market: central database did not open ({e:#}) — global market OFF")
        }
    }
    Ok(())
}

pub async fn criar_tabelas_do_realm(pool: &PgPool) -> Result<()> {
    for sql in [
        "CREATE TABLE IF NOT EXISTS mercado_saida (
            id          TEXT PRIMARY KEY,
            personagem  TEXT        NOT NULL,
            payload     TEXT        NOT NULL,
            criada      TIMESTAMPTZ NOT NULL DEFAULT NOW(),
            enviada     TIMESTAMPTZ,
            resultado   TEXT
        )",
        "CREATE INDEX IF NOT EXISTS mercado_saida_pendente ON mercado_saida (criada) WHERE enviada IS NULL",
        "CREATE TABLE IF NOT EXISTS mercado_cartas_aplicadas (
            id          TEXT PRIMARY KEY,
            personagem  TEXT        NOT NULL,
            aplicada    TIMESTAMPTZ NOT NULL DEFAULT NOW(),
            avisada     TIMESTAMPTZ
        )",
        "CREATE INDEX IF NOT EXISTS mercado_cartas_aplicadas_pendente ON mercado_cartas_aplicadas (personagem) WHERE avisada IS NULL",
    ] {
        sqlx::query(sql).execute(pool).await?;
    }
    Ok(())
}

pub async fn abrir_central(url: &str) -> Result<PgPool> {
    let pool = PgPoolOptions::new()
        .max_connections(4)
        .acquire_timeout(Duration::from_secs(5))
        .connect(url)
        .await?;
    let mut trava = pool.acquire().await?;
    sqlx::query("SELECT pg_advisory_lock(728432)")
        .execute(&mut *trava)
        .await?;
    let r = criar_tabelas_centrais(&pool).await;
    let _ = sqlx::query("SELECT pg_advisory_unlock(728432)")
        .execute(&mut *trava)
        .await;
    drop(trava);
    r?;
    Ok(pool)
}

async fn criar_tabelas_centrais(pool: &PgPool) -> Result<()> {
    for sql in [
        "CREATE TABLE IF NOT EXISTS mercado_anuncios (
            id            TEXT PRIMARY KEY,
            tipo          SMALLINT    NOT NULL,
            realm         TEXT        NOT NULL,
            conta         TEXT        NOT NULL,
            personagem    TEXT        NOT NULL,
            item_id       INTEGER     NOT NULL,
            nome          TEXT        NOT NULL,
            categoria     SMALLINT    NOT NULL,
            instancia     TEXT,
            qtd_total     BIGINT      NOT NULL,
            qtd_restante  BIGINT      NOT NULL CHECK (qtd_restante >= 0),
            preco_unit    BIGINT      NOT NULL CHECK (preco_unit > 0),
            estado        SMALLINT    NOT NULL DEFAULT 0,
            criado        TIMESTAMPTZ NOT NULL DEFAULT NOW(),
            atualizado    TIMESTAMPTZ NOT NULL DEFAULT NOW()
        )",
        "CREATE INDEX IF NOT EXISTS mercado_anuncios_busca ON mercado_anuncios (categoria, preco_unit) WHERE estado = 0",
        "CREATE INDEX IF NOT EXISTS mercado_anuncios_dono ON mercado_anuncios (realm, personagem) WHERE estado = 0",
        "CREATE TABLE IF NOT EXISTS mercado_operacoes (
            id         TEXT PRIMARY KEY,
            realm      TEXT        NOT NULL,
            tipo       TEXT        NOT NULL,
            ok         BOOLEAN     NOT NULL DEFAULT TRUE,
            resultado  TEXT        NOT NULL DEFAULT '',
            quando     TIMESTAMPTZ NOT NULL DEFAULT NOW()
        )",
        "CREATE TABLE IF NOT EXISTS mercado_cartas (
            id          TEXT PRIMARY KEY,
            realm       TEXT        NOT NULL,
            personagem  TEXT        NOT NULL,
            item_id     INTEGER     NOT NULL DEFAULT 0,
            qtd         BIGINT      NOT NULL DEFAULT 0,
            instancia   TEXT,
            gold        BIGINT      NOT NULL DEFAULT 0,
            motivo      TEXT        NOT NULL,
            criada      TIMESTAMPTZ NOT NULL DEFAULT NOW(),
            entregue    TIMESTAMPTZ
        )",
        "CREATE INDEX IF NOT EXISTS mercado_cartas_pendentes ON mercado_cartas (realm, personagem) WHERE entregue IS NULL",
        "CREATE TABLE IF NOT EXISTS mercado_vendas (
            id               BIGSERIAL PRIMARY KEY,
            anuncio          TEXT        NOT NULL,
            tipo             SMALLINT    NOT NULL,
            nome             TEXT        NOT NULL,
            qtd              BIGINT      NOT NULL,
            preco_unit       BIGINT      NOT NULL,
            bruto            BIGINT      NOT NULL,
            taxa             BIGINT      NOT NULL,
            liquido          BIGINT      NOT NULL,
            vendedor_realm   TEXT        NOT NULL,
            vendedor         TEXT        NOT NULL,
            comprador_realm  TEXT        NOT NULL,
            comprador        TEXT        NOT NULL,
            quando           TIMESTAMPTZ NOT NULL DEFAULT NOW()
        )",
        "CREATE INDEX IF NOT EXISTS mercado_vendas_vendedor ON mercado_vendas (vendedor_realm, vendedor, id DESC)",
        "CREATE INDEX IF NOT EXISTS mercado_vendas_comprador ON mercado_vendas (comprador_realm, comprador, id DESC)",
        // Listing slots a character bought (`regras::max_anuncios`).
        "CREATE TABLE IF NOT EXISTS mercado_vagas (
            realm      TEXT     NOT NULL,
            personagem TEXT     NOT NULL,
            compras    SMALLINT NOT NULL DEFAULT 0,
            PRIMARY KEY (realm, personagem)
        )",
        // Buy orders: the gold for `qtd_total` was paid up front and waits
        // here; `gold_restante` of it is still in escrow.
        "CREATE TABLE IF NOT EXISTS mercado_pedidos (
            id            TEXT PRIMARY KEY,
            realm         TEXT        NOT NULL,
            conta         TEXT        NOT NULL,
            personagem    TEXT        NOT NULL,
            item_id       INTEGER     NOT NULL,
            nome          TEXT        NOT NULL,
            categoria     SMALLINT    NOT NULL,
            qtd_total     BIGINT      NOT NULL,
            qtd_restante  BIGINT      NOT NULL CHECK (qtd_restante >= 0),
            preco_unit    BIGINT      NOT NULL CHECK (preco_unit > 0),
            estado        SMALLINT    NOT NULL DEFAULT 0,
            criado        TIMESTAMPTZ NOT NULL DEFAULT NOW(),
            atualizado    TIMESTAMPTZ NOT NULL DEFAULT NOW()
        )",
        "CREATE INDEX IF NOT EXISTS mercado_pedidos_busca ON mercado_pedidos (item_id, preco_unit DESC) WHERE estado = 0",
        "CREATE INDEX IF NOT EXISTS mercado_pedidos_dono ON mercado_pedidos (realm, personagem) WHERE estado = 0",
        // Lot pricing (13/11/2026): the whole gold still asked (listing) or
        // held (buy order) for the units left; the unit price is
        // `gold_restante / qtd_restante`, fractions allowed. `preco_unit`
        // stays as that price rounded up, for the Panopticon and history.
        "ALTER TABLE mercado_anuncios ADD COLUMN IF NOT EXISTS gold_restante BIGINT",
        "UPDATE mercado_anuncios SET gold_restante = qtd_restante * preco_unit WHERE gold_restante IS NULL",
        "ALTER TABLE mercado_anuncios ALTER COLUMN gold_restante SET NOT NULL",
        "ALTER TABLE mercado_pedidos ADD COLUMN IF NOT EXISTS gold_restante BIGINT",
        "UPDATE mercado_pedidos SET gold_restante = qtd_restante * preco_unit WHERE gold_restante IS NULL",
        "ALTER TABLE mercado_pedidos ALTER COLUMN gold_restante SET NOT NULL",
    ] {
        sqlx::query(sql).execute(pool).await?;
    }
    razao::criar_tabela(pool).await?;
    let mut tx = pool.begin().await?;
    let antigos = sqlx::query("SELECT id, realm, personagem, qtd_restante FROM mercado_anuncios WHERE item_id = 359 AND estado = 0 FOR UPDATE")
        .fetch_all(&mut *tx).await?;
    for row in antigos {
        let id: String = row.get("id");
        let realm: String = row.get("realm");
        let personagem: String = row.get("personagem");
        let qtd = row.get::<i64, _>("qtd_restante").max(0) as u64;
        carta(&mut tx, &format!("{id}:fim-teleporte"), &realm, &personagem,
            shared::item_id::COPPER, qtd.saturating_mul(100), None, 0,
            "Pergaminho de Teleporte retirado: reembolso em cobre").await?;
        sqlx::query("UPDATE mercado_anuncios SET estado = $2, qtd_restante = 0 WHERE id = $1")
            .bind(&id).bind(regras::ESTADO_CANCELADO as i16).execute(&mut *tx).await?;
    }
    sqlx::query("UPDATE mercado_cartas SET item_id = $1, qtd = qtd * 100, instancia = NULL WHERE item_id = 359")
        .bind(shared::item_id::COPPER as i32).execute(&mut *tx).await?;
    tx.commit().await?;
    Ok(())
}

// ─────────────────────────── realm → central ───────────────────────────

/// O que sai do realm pro central. Vai como JSON em `mercado_saida.payload`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "op")]
pub enum OpCentral {
    Anunciar {
        id: String,
        realm: String,
        conta: String,
        personagem: String,
        item_id: u16,
        nome: String,
        categoria: u8,
        instancia: Option<ItemInstance>,
        qtd: u64,
        /// Gold asked for all `qtd` (lot pricing, 13/11/2026).
        #[serde(default)]
        preco_total: u64,
        /// Before lot pricing: gold per unit. Only read from an op queued by
        /// an older realm (`preco_total` = 0), as `qtd × preco_unit`.
        #[serde(default)]
        preco_unit: u64,
    },
    Comprar {
        id: String,
        realm: String,
        conta: String,
        personagem: String,
        anuncio: String,
        qtd: u64,
        /// Before lot pricing; unused (the cost is worked out from `pago`).
        #[serde(default)]
        preco_unit: u64,
        /// Gold que ja' saiu do comprador.
        pago: u64,
    },
    /// Slot purchase number `compra` (1 = the first), paid in gold that
    /// already left the character; a stale one gives the gold back by letter.
    ComprarVagas {
        id: String,
        realm: String,
        personagem: String,
        compra: u8,
        pago: u64,
    },
    /// A buy order: `pago` (the order's total) already left the buyer; a
    /// refused one gives it back by letter.
    PedirCompra {
        id: String,
        realm: String,
        conta: String,
        personagem: String,
        item_id: u16,
        nome: String,
        categoria: u8,
        qtd: u64,
        /// Before lot pricing; unused: `pago` IS the order's total.
        #[serde(default)]
        preco_unit: u64,
        pago: u64,
    },
    /// `qtd` of `item_id` that already left the seller, sold to `pedido`.
    /// What the order does not take goes back by letter.
    VenderAoPedido {
        id: String,
        realm: String,
        conta: String,
        personagem: String,
        pedido: String,
        item_id: u16,
        qtd: u64,
    },
}

impl OpCentral {
    pub fn id(&self) -> &str {
        match self {
            OpCentral::Anunciar { id, .. }
            | OpCentral::Comprar { id, .. }
            | OpCentral::ComprarVagas { id, .. }
            | OpCentral::PedirCompra { id, .. }
            | OpCentral::VenderAoPedido { id, .. } => id,
        }
    }

    pub fn personagem(&self) -> &str {
        match self {
            OpCentral::Anunciar { personagem, .. }
            | OpCentral::Comprar { personagem, .. }
            | OpCentral::ComprarVagas { personagem, .. }
            | OpCentral::PedirCompra { personagem, .. }
            | OpCentral::VenderAoPedido { personagem, .. } => personagem,
        }
    }
}

/// Registro que vai junto do save do personagem (`persistence::SaveBatch`).
#[derive(Debug, Clone)]
pub enum Registro {
    Saida(OpCentral),
    CartaAplicada { id: String, personagem: String },
}

impl Registro {
    pub fn personagem(&self) -> &str {
        match self {
            Registro::Saida(op) => op.personagem(),
            Registro::CartaAplicada { personagem, .. } => personagem,
        }
    }
}

/// Grava os registros dentro da transacao do save.
pub async fn gravar_registros(tx: &mut Transaction<'_, Postgres>, regs: &[Registro]) -> Result<()> {
    for r in regs {
        match r {
            Registro::Saida(op) => {
                sqlx::query("INSERT INTO mercado_saida (id, personagem, payload) VALUES ($1, $2, $3) ON CONFLICT (id) DO NOTHING")
                    .bind(op.id())
                    .bind(op.personagem())
                    .bind(serde_json::to_string(op)?)
                    .execute(&mut **tx)
                    .await?;
            }
            Registro::CartaAplicada { id, personagem } => {
                sqlx::query("INSERT INTO mercado_cartas_aplicadas (id, personagem) VALUES ($1, $2) ON CONFLICT (id) DO NOTHING")
                    .bind(id)
                    .bind(personagem)
                    .execute(&mut **tx)
                    .await?;
            }
        }
    }
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Resultado {
    pub ok: bool,
    pub texto: String,
    /// false = a operacao ja' tinha sido processada antes.
    pub novo: bool,
}

/// Processa uma operacao no central, uma vez so'.
pub async fn processar(central: &PgPool, op: &OpCentral) -> Result<Resultado> {
    let (realm, tipo) = match op {
        OpCentral::Anunciar { realm, .. } => (realm, "anunciar"),
        OpCentral::Comprar { realm, .. } => (realm, "comprar"),
        OpCentral::ComprarVagas { realm, .. } => (realm, "vagas"),
        OpCentral::PedirCompra { realm, .. } => (realm, "pedir"),
        OpCentral::VenderAoPedido { realm, .. } => (realm, "vender_pedido"),
    };
    let mut tx = central.begin().await?;
    let novo = sqlx::query("INSERT INTO mercado_operacoes (id, realm, tipo) VALUES ($1, $2, $3) ON CONFLICT (id) DO NOTHING")
        .bind(op.id())
        .bind(realm)
        .bind(tipo)
        .execute(&mut *tx)
        .await?
        .rows_affected()
        > 0;
    if !novo {
        tx.rollback().await?;
        let (ok, texto): (bool, String) =
            sqlx::query_as("SELECT ok, resultado FROM mercado_operacoes WHERE id = $1")
                .bind(op.id())
                .fetch_one(central)
                .await?;
        return Ok(Resultado {
            ok,
            texto,
            novo: false,
        });
    }
    let (ok, texto) = match op {
        OpCentral::Anunciar {
            id,
            realm,
            conta,
            personagem,
            item_id,
            nome,
            categoria,
            instancia,
            qtd,
            preco_total,
            preco_unit,
        } => {
            let total = if *preco_total > 0 { *preco_total } else { qtd.saturating_mul(*preco_unit) };
            anunciar_item(
                &mut tx,
                id,
                realm,
                conta,
                personagem,
                *item_id,
                nome,
                *categoria,
                instancia,
                *qtd,
                total,
            )
            .await?
        }
        OpCentral::Comprar {
            id,
            realm,
            conta,
            personagem,
            anuncio,
            qtd,
            preco_unit: _,
            pago,
        } => {
            comprar(
                &mut tx,
                id,
                realm,
                conta,
                personagem,
                anuncio,
                *qtd,
                *pago,
            )
            .await?
        }
        OpCentral::ComprarVagas {
            id,
            realm,
            personagem,
            compra,
            pago,
        } => vagas_com_ouro(&mut tx, id, realm, personagem, *compra, *pago).await?,
        OpCentral::PedirCompra {
            id,
            realm,
            conta,
            personagem,
            item_id,
            nome,
            categoria,
            qtd,
            preco_unit: _,
            pago,
        } => {
            pedir_compra(
                &mut tx, id, realm, conta, personagem, *item_id, nome, *categoria, *qtd, *pago,
            )
            .await?
        }
        OpCentral::VenderAoPedido {
            id,
            realm,
            conta: _,
            personagem,
            pedido,
            item_id,
            qtd,
        } => vender_ao_pedido(&mut tx, id, realm, personagem, pedido, *item_id, *qtd).await?,
    };
    sqlx::query("UPDATE mercado_operacoes SET ok = $2, resultado = $3 WHERE id = $1")
        .bind(op.id())
        .bind(ok)
        .bind(&texto)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(Resultado {
        ok,
        texto,
        novo: true,
    })
}

#[allow(clippy::too_many_arguments)]
async fn carta(
    tx: &mut Transaction<'_, Postgres>,
    id: &str,
    realm: &str,
    personagem: &str,
    item_id: u16,
    qtd: u64,
    instancia: Option<&str>,
    gold: u64,
    motivo: &str,
) -> Result<()> {
    let (item_id, qtd) = if item_id == shared::item_id::PERGAMINHO_TELEPORTE {
        (shared::item_id::COPPER, qtd.saturating_mul(100))
    } else { (item_id, qtd) };
    sqlx::query(
        "INSERT INTO mercado_cartas (id, realm, personagem, item_id, qtd, instancia, gold, motivo)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8) ON CONFLICT (id) DO NOTHING",
    )
    .bind(id)
    .bind(realm)
    .bind(personagem)
    .bind(item_id as i32)
    .bind(qtd as i64)
    .bind(instancia)
    .bind(gold as i64)
    .bind(motivo)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

async fn ativos_de(
    tx: &mut Transaction<'_, Postgres>,
    realm: &str,
    personagem: &str,
) -> Result<usize> {
    let n: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM mercado_anuncios WHERE realm = $1 AND personagem = $2 AND estado = 0",
    )
    .bind(realm)
    .bind(personagem)
    .fetch_one(&mut **tx)
    .await?;
    Ok(n.max(0) as usize)
}

/// Slot purchases `personagem` made (0 = none).
pub async fn compras_de_vagas<'e>(
    db: impl sqlx::PgExecutor<'e>,
    realm: &str,
    personagem: &str,
) -> Result<u8> {
    let n: Option<i16> = sqlx::query_scalar("SELECT compras FROM mercado_vagas WHERE realm = $1 AND personagem = $2")
        .bind(realm)
        .bind(personagem)
        .fetch_optional(db)
        .await?;
    Ok(n.unwrap_or(0).clamp(0, u8::MAX as i16) as u8)
}

/// Takes the per-character listing lock and checks that `compra` is the next
/// purchase. `Err` is the refusal for the player.
async fn proxima_compra(
    tx: &mut Transaction<'_, Postgres>,
    realm: &str,
    personagem: &str,
    compra: u8,
) -> Result<std::result::Result<(u64, u64), Recusa>> {
    sqlx::query("SELECT pg_advisory_xact_lock(hashtext($1))")
        .bind(format!("anuncios:{realm}:{personagem}"))
        .execute(&mut **tx)
        .await?;
    let feitas = compras_de_vagas(&mut **tx, realm, personagem).await?;
    Ok(match regras::proxima_vaga(feitas) {
        Some(custo) if feitas as u16 + 1 == compra as u16 => Ok(custo),
        _ => Err(Recusa::VagaMudou),
    })
}

async fn gravar_compra(tx: &mut Transaction<'_, Postgres>, realm: &str, personagem: &str, compra: u8) -> Result<String> {
    sqlx::query(
        "INSERT INTO mercado_vagas (realm, personagem, compras) VALUES ($1, $2, $3)
         ON CONFLICT (realm, personagem) DO UPDATE SET compras = EXCLUDED.compras",
    )
    .bind(realm)
    .bind(personagem)
    .bind(compra as i16)
    .execute(&mut **tx)
    .await?;
    Ok(format!(
        "+{} listing slots: you can now keep {} listings active.",
        regras::VAGAS_POR_COMPRA,
        regras::max_anuncios(compra)
    ))
}

/// Slot purchase paid in gold (through the outbox): a stale or overpaid
/// one sends the gold back by letter.
async fn vagas_com_ouro(
    tx: &mut Transaction<'_, Postgres>,
    id: &str,
    realm: &str,
    personagem: &str,
    compra: u8,
    pago: u64,
) -> Result<(bool, String)> {
    match proxima_compra(tx, realm, personagem, compra).await? {
        Ok((_, ouro)) if pago >= ouro => {
            if pago > ouro {
                carta(tx, &format!("{id}:troco"), realm, personagem, 0, 0, None, pago - ouro, "listing slots: change").await?;
            }
            Ok((true, gravar_compra(tx, realm, personagem, compra).await?))
        }
        Ok(_) | Err(_) => {
            carta(tx, &format!("{id}:devolve"), realm, personagem, 0, 0, None, pago, "listing slots: refund").await?;
            Ok((false, Recusa::VagaMudou.texto()))
        }
    }
}

/// Slot purchase paid in TP: the account's TP and the slots move in one
/// transaction.
pub async fn comprar_vagas_tp(
    central: &PgPool,
    realm: &str,
    conta: &str,
    personagem: &str,
    compra: u8,
) -> Result<(bool, String)> {
    let mut tx = central.begin().await?;
    let tp = match proxima_compra(&mut tx, realm, personagem, compra).await? {
        Ok((tp, _)) => tp,
        Err(r) => return Ok((false, r.texto())),
    };
    match razao::mover(
        &mut tx,
        conta,
        -(tp as i64),
        "market listing slots",
        Some(&format!("vagas:{realm}:{personagem}:{compra}")),
    )
    .await?
    {
        razao::Movimento::SemSaldo { .. } => return Ok((false, Recusa::SemTp.texto())),
        razao::Movimento::JaFeito { .. } => return Ok((false, Recusa::VagaMudou.texto())),
        razao::Movimento::Feito { .. } => {}
    }
    let texto = gravar_compra(&mut tx, realm, personagem, compra).await?;
    tx.commit().await?;
    Ok((true, texto))
}

#[allow(clippy::too_many_arguments)]
async fn anunciar_item(
    tx: &mut Transaction<'_, Postgres>,
    id: &str,
    realm: &str,
    conta: &str,
    personagem: &str,
    item_id: u16,
    nome: &str,
    categoria: u8,
    instancia: &Option<ItemInstance>,
    qtd: u64,
    preco_total: u64,
) -> Result<(bool, String)> {
    let inst = instancia
        .as_ref()
        .and_then(|i| serde_json::to_string(i).ok());
    // Travado por personagem: dois anuncios simultaneos nao passam juntos do limite.
    sqlx::query("SELECT pg_advisory_xact_lock(hashtext($1))")
        .bind(format!("anuncios:{realm}:{personagem}"))
        .execute(&mut **tx)
        .await?;
    let recusa = if qtd == 0 || item_id == 0 || item_id == shared::item_id::PERGAMINHO_TELEPORTE {
        Some(Recusa::Quantidade)
    } else if !regras::preco_total_valido(qtd, preco_total) {
        Some(Recusa::Preco)
    } else if ativos_de(tx, realm, personagem).await?
        >= regras::max_anuncios(compras_de_vagas(&mut **tx, realm, personagem).await?)
    {
        Some(Recusa::MuitosAnuncios)
    } else {
        None
    };
    if let Some(r) = recusa {
        // O item ja' saiu da bolsa: volta por carta.
        carta(
            tx,
            &format!("{id}:devolve"),
            realm,
            personagem,
            item_id,
            qtd,
            inst.as_deref(),
            0,
            &format!("Listing refused: {nome}"),
        )
        .await?;
        return Ok((false, format!("{} {nome} comes back in Deliveries.", r.texto())));
    }
    sqlx::query(
        "INSERT INTO mercado_anuncios (id, tipo, realm, conta, personagem, item_id, nome, categoria, instancia, qtd_total, qtd_restante, preco_unit, gold_restante)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $10, $11, $12)",
    )
    .bind(id)
    .bind(regras::TIPO_ITEM as i16)
    .bind(realm)
    .bind(conta)
    .bind(personagem)
    .bind(item_id as i32)
    .bind(nome)
    .bind(categoria as i16)
    .bind(inst)
    .bind(qtd as i64)
    .bind(unit_arredondado(qtd, preco_total))
    .bind(preco_total as i64)
    .execute(&mut **tx)
    .await?;
    Ok((
        true,
        format!(
            "Anunciado: {nome} ×{qtd} por {} gold ({} cada).",
            shared::idioma::milhar(preco_total),
            fmt_unit(qtd, preco_total)
        ),
    ))
}

/// The `preco_unit` column kept beside `gold_restante` since lot pricing:
/// the unit price rounded up (at least 1), for the Panopticon and history.
fn unit_arredondado(qtd: u64, gold: u64) -> i64 {
    if qtd == 0 {
        return 1;
    }
    gold.div_ceil(qtd).max(1).min(i64::MAX as u64) as i64
}

/// A unit price for a message: whole when it is, else up to 3 decimals.
fn fmt_unit(qtd: u64, gold: u64) -> String {
    regras::fmt_unitario(regras::preco_unitario(qtd, gold))
}

#[allow(clippy::too_many_arguments)]
async fn comprar(
    tx: &mut Transaction<'_, Postgres>,
    id: &str,
    realm: &str,
    conta: &str,
    personagem: &str,
    anuncio: &str,
    qtd: u64,
    pago: u64,
) -> Result<(bool, String)> {
    let linha = sqlx::query(
        "SELECT tipo, realm, personagem, item_id, nome, instancia, qtd_restante, gold_restante, estado
           FROM mercado_anuncios WHERE id = $1 FOR UPDATE",
    )
    .bind(anuncio)
    .fetch_optional(&mut **tx)
    .await?;
    let fechamento = match &linha {
        None => Err(Recusa::Esgotado),
        Some(l)
            if l.get::<String, _>("realm") == realm
                && l.get::<String, _>("personagem") == personagem =>
        {
            Err(Recusa::ProprioAnuncio)
        }
        Some(l) => regras::fechar_compra(
            l.get::<i64, _>("qtd_restante").max(0) as u64,
            l.get::<i16, _>("estado") as u8,
            l.get::<i64, _>("gold_restante").max(0) as u64,
            qtd,
            pago,
        ),
    };
    let (l, f) = match (linha, fechamento) {
        (Some(l), Ok(f)) => (l, f),
        (_, Err(r)) => {
            if pago > 0 {
                carta(
                    tx,
                    &format!("{id}:reembolso"),
                    realm,
                    personagem,
                    0,
                    0,
                    None,
                    pago,
                    "The purchase didn't go through: gold refunded",
                )
                .await?;
            }
            return Ok((false, r.texto()));
        }
        (None, Ok(_)) => unreachable!("fechamento sem anuncio"),
    };
    let tipo = l.get::<i16, _>("tipo") as u8;
    let nome: String = l.get("nome");
    let vendedor_realm: String = l.get("realm");
    let vendedor: String = l.get("personagem");
    let estado = if f.sobra == 0 {
        regras::ESTADO_ESGOTADO
    } else {
        regras::ESTADO_ATIVO
    };
    sqlx::query(
        "UPDATE mercado_anuncios SET qtd_restante = $2, estado = $3, gold_restante = $4, preco_unit = $5, atualizado = NOW() WHERE id = $1",
    )
    .bind(anuncio)
    .bind(f.sobra as i64)
    .bind(estado as i16)
    .bind(f.gold_sobra as i64)
    .bind(if f.sobra == 0 { unit_arredondado(f.vendida, f.bruto) } else { unit_arredondado(f.sobra, f.gold_sobra) })
    .execute(&mut **tx)
    .await?;
    let texto = if tipo == regras::TIPO_TP {
        match razao::mover(
            tx,
            conta,
            f.vendida as i64,
            "bought TP on the market",
            Some(&format!("{id}:tp")),
        )
        .await?
        {
            razao::Movimento::Feito { .. } | razao::Movimento::JaFeito { .. } => {}
            razao::Movimento::SemSaldo { .. } => unreachable!("credito nunca fica sem saldo"),
        }
        if f.troco > 0 {
            carta(
                tx,
                &format!("{id}:troco"),
                realm,
                personagem,
                0,
                0,
                None,
                f.troco,
                "Market: change from your purchase",
            )
            .await?;
        }
        format!("Comprado: {} TP.", f.vendida)
    } else {
        let inst: Option<String> = l.get("instancia");
        carta(
            tx,
            &format!("{id}:item"),
            realm,
            personagem,
            l.get::<i32, _>("item_id") as u16,
            f.vendida,
            inst.as_deref(),
            // The listing got cheaper since the buyer looked: the change
            // rides with the items.
            f.troco,
            &format!("Bought on the market: {nome}"),
        )
        .await?;
        format!("Comprado: {nome} ×{}. Receba em Entregas.", f.vendida)
    };
    if f.liquido > 0 {
        carta(
            tx,
            &format!("{id}:venda"),
            &vendedor_realm,
            &vendedor,
            0,
            0,
            None,
            f.liquido,
            &format!("Sale: {nome} ×{} (fee {})", f.vendida, f.taxa),
        )
        .await?;
    }
    sqlx::query(
        "INSERT INTO mercado_vendas (anuncio, tipo, nome, qtd, preco_unit, bruto, taxa, liquido, vendedor_realm, vendedor, comprador_realm, comprador)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)",
    )
    .bind(anuncio)
    .bind(tipo as i16)
    .bind(&nome)
    .bind(f.vendida as i64)
    .bind(unit_arredondado(f.vendida, f.bruto))
    .bind(f.bruto as i64)
    .bind(f.taxa as i64)
    .bind(f.liquido as i64)
    .bind(&vendedor_realm)
    .bind(&vendedor)
    .bind(realm)
    .bind(personagem)
    .execute(&mut **tx)
    .await?;
    Ok((true, texto))
}

// ─────────────────────── pedidos diretos ao central ───────────────────────

/// Cancela um anuncio proprio: item volta por carta, TP volta pro livro.
pub async fn cancelar(
    central: &PgPool,
    realm: &str,
    personagem: &str,
    anuncio: &str,
) -> Result<(bool, String)> {
    let mut tx = central.begin().await?;
    let Some(l) = sqlx::query(
        "SELECT tipo, conta, item_id, nome, instancia, qtd_restante, estado FROM mercado_anuncios
          WHERE id = $1 AND realm = $2 AND personagem = $3 FOR UPDATE",
    )
    .bind(anuncio)
    .bind(realm)
    .bind(personagem)
    .fetch_optional(&mut *tx)
    .await?
    else {
        return Ok((false, "Listing not found.".into()));
    };
    if l.get::<i16, _>("estado") as u8 != regras::ESTADO_ATIVO {
        return Ok((false, "The listing is no longer active.".into()));
    }
    let restante = l.get::<i64, _>("qtd_restante").max(0) as u64;
    let nome: String = l.get("nome");
    sqlx::query("UPDATE mercado_anuncios SET estado = $2, qtd_restante = 0, gold_restante = 0, atualizado = NOW() WHERE id = $1")
        .bind(anuncio)
        .bind(regras::ESTADO_CANCELADO as i16)
        .execute(&mut *tx)
        .await?;
    let texto = if l.get::<i16, _>("tipo") as u8 == regras::TIPO_TP {
        let conta: String = l.get("conta");
        razao::mover(
            &mut tx,
            &conta,
            restante as i64,
            "cancelled a TP listing",
            Some(&format!("{anuncio}:cancelado")),
        )
        .await?;
        format!("Anúncio cancelado: {restante} TP de volta.")
    } else if restante > 0 {
        let inst: Option<String> = l.get("instancia");
        carta(
            &mut tx,
            &format!("{anuncio}:cancelado"),
            realm,
            personagem,
            l.get::<i32, _>("item_id") as u16,
            restante,
            inst.as_deref(),
            0,
            &format!("Listing cancelled: {nome}"),
        )
        .await?;
        format!("Anúncio cancelado: {nome} ×{restante} volta em Entregas.")
    } else {
        "Listing cancelled.".into()
    };
    tx.commit().await?;
    Ok((true, texto))
}

/// Reprices an active listing of mine (`regras::validar_ajuste`). No fee and
/// no escrow change: the items or TP are already held by the listing. Setting
/// a price is idempotent, so a repeated request needs no operation id.
pub async fn ajustar_preco(
    central: &PgPool,
    realm: &str,
    personagem: &str,
    anuncio: &str,
    preco_total: u64,
) -> Result<(bool, String)> {
    let mut tx = central.begin().await?;
    let l = sqlx::query(
        "SELECT estado, qtd_restante, gold_restante, nome FROM mercado_anuncios
          WHERE id = $1 AND realm = $2 AND personagem = $3 FOR UPDATE",
    )
    .bind(anuncio)
    .bind(realm)
    .bind(personagem)
    .fetch_optional(&mut *tx)
    .await?;
    let (estado, restante, atual, nome) = match &l {
        Some(l) => (
            l.get::<i16, _>("estado") as u8,
            l.get::<i64, _>("qtd_restante").max(0) as u64,
            l.get::<i64, _>("gold_restante").max(0) as u64,
            l.get::<String, _>("nome"),
        ),
        None => (0, 0, 0, String::new()),
    };
    if let Err(e) = regras::validar_ajuste(l.is_some(), estado, restante, atual, preco_total) {
        return Ok((false, e.into()));
    }
    sqlx::query("UPDATE mercado_anuncios SET gold_restante = $2, preco_unit = $3, atualizado = NOW() WHERE id = $1")
        .bind(anuncio)
        .bind(preco_total as i64)
        .bind(unit_arredondado(restante, preco_total))
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok((
        true,
        format!(
            "New price for {nome}: {} gold for {restante} ({} each).",
            shared::idioma::milhar(preco_total),
            fmt_unit(restante, preco_total)
        ),
    ))
}

/// Anuncia TP da conta: a TP sai do livro na mesma transacao.
pub async fn anunciar_tp(
    central: &PgPool,
    realm: &str,
    conta: &str,
    personagem: &str,
    qtd: u64,
    preco_total: u64,
) -> Result<(bool, String)> {
    if qtd == 0 || qtd > regras::TP_MAX_POR_ANUNCIO {
        return Ok((false, Recusa::Quantidade.texto()));
    }
    if !regras::preco_total_valido(qtd, preco_total) {
        return Ok((false, Recusa::Preco.texto()));
    }
    let mut tx = central.begin().await?;
    sqlx::query("SELECT pg_advisory_xact_lock(hashtext($1))")
        .bind(format!("anuncios:{realm}:{personagem}"))
        .execute(&mut *tx)
        .await?;
    if ativos_de(&mut tx, realm, personagem).await?
        >= regras::max_anuncios(compras_de_vagas(&mut *tx, realm, personagem).await?)
    {
        return Ok((false, Recusa::MuitosAnuncios.texto()));
    }
    let id = novo_id();
    match razao::mover(
        &mut tx,
        conta,
        -(qtd as i64),
        "listed TP on the market",
        Some(&format!("{id}:custodia")),
    )
    .await?
    {
        razao::Movimento::SemSaldo { .. } => return Ok((false, Recusa::SemTp.texto())),
        razao::Movimento::Feito { .. } | razao::Movimento::JaFeito { .. } => {}
    }
    sqlx::query(
        "INSERT INTO mercado_anuncios (id, tipo, realm, conta, personagem, item_id, nome, categoria, instancia, qtd_total, qtd_restante, preco_unit, gold_restante)
         VALUES ($1, $2, $3, $4, $5, 0, 'Tempest Points', $6, NULL, $7, $7, $8, $9)",
    )
    .bind(&id)
    .bind(regras::TIPO_TP as i16)
    .bind(realm)
    .bind(conta)
    .bind(personagem)
    .bind(regras::Categoria::Tp as i16)
    .bind(qtd as i64)
    .bind(unit_arredondado(qtd, preco_total))
    .bind(preco_total as i64)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok((
        true,
        format!(
            "Anunciado: {qtd} TP por {} gold ({} cada).",
            shared::idioma::milhar(preco_total),
            fmt_unit(qtd, preco_total)
        ),
    ))
}

fn anuncio_de(l: &sqlx::postgres::PgRow, realm: &str, personagem: &str) -> AnuncioNet {
    let r: String = l.get("realm");
    let p: String = l.get("personagem");
    let inst: Option<String> = l.get("instancia");
    AnuncioNet {
        id: l.get("id"),
        tipo: l.get::<i16, _>("tipo") as u8,
        item_id: l.get::<i32, _>("item_id") as u16,
        nome: l.get("nome"),
        categoria: l.get::<i16, _>("categoria") as u8,
        instancia: inst.and_then(|s| serde_json::from_str(&s).ok()),
        qtd: l.get::<i64, _>("qtd_restante").max(0) as u64,
        preco_total: l.get::<i64, _>("gold_restante").max(0) as u64,
        meu: r == realm && p == personagem,
        realm: r,
        vendedor: p,
    }
}

/// Uma pagina da busca: mais barato primeiro.
pub async fn buscar(
    central: &PgPool,
    filtro: &FiltroNet,
    realm: &str,
    personagem: &str,
) -> Result<(Vec<AnuncioNet>, bool)> {
    let texto: String = filtro
        .texto
        .chars()
        .filter(|c| !matches!(c, '%' | '_' | '\\'))
        .take(regras::BUSCA_MAX_CHARS)
        .collect();
    let pagina = filtro.pagina.min(500) as i64;
    let linhas = sqlx::query(
        "SELECT id, tipo, realm, personagem, item_id, nome, categoria, instancia, qtd_restante, gold_restante
           FROM mercado_anuncios
          WHERE estado = 0
            AND ($1 = 0 OR categoria = $1)
            AND ($1 = 4 OR categoria <> 4)
            AND ($2 = '' OR nome ILIKE '%' || $2 || '%')
          ORDER BY gold_restante::float8 / GREATEST(qtd_restante, 1) ASC, criado ASC
          LIMIT $3 OFFSET $4",
    )
    .bind(filtro.categoria as i16)
    .bind(texto.trim())
    .bind(regras::POR_PAGINA as i64 + 1)
    .bind(pagina * regras::POR_PAGINA as i64)
    .fetch_all(central)
    .await?;
    let tem_mais = linhas.len() > regras::POR_PAGINA;
    Ok((
        linhas
            .iter()
            .take(regras::POR_PAGINA)
            .map(|l| anuncio_de(l, realm, personagem))
            .collect(),
        tem_mais,
    ))
}

/// Meus anuncios ativos, as ultimas vendas/compras e a TP da conta.
pub async fn meus(
    central: &PgPool,
    realm: &str,
    personagem: &str,
    conta: &str,
) -> Result<(Vec<AnuncioNet>, Vec<VendaNet>, u64, u8)> {
    let ativos = sqlx::query(
        "SELECT id, tipo, realm, personagem, item_id, nome, categoria, instancia, qtd_restante, gold_restante
           FROM mercado_anuncios WHERE realm = $1 AND personagem = $2 AND estado = 0 ORDER BY criado DESC",
    )
    .bind(realm)
    .bind(personagem)
    .fetch_all(central)
    .await?;
    let vendas = sqlx::query(
        "SELECT nome, qtd, preco_unit, taxa, liquido, EXTRACT(EPOCH FROM quando)::BIGINT AS quando,
                (vendedor_realm = $1 AND vendedor = $2) AS vendi
           FROM mercado_vendas
          WHERE (vendedor_realm = $1 AND vendedor = $2) OR (comprador_realm = $1 AND comprador = $2)
          ORDER BY id DESC LIMIT 30",
    )
    .bind(realm)
    .bind(personagem)
    .fetch_all(central)
    .await?;
    let historico = vendas
        .iter()
        .map(|v| VendaNet {
            nome: v.get("nome"),
            qtd: v.get::<i64, _>("qtd").max(0) as u64,
            preco_unit: v.get::<i64, _>("preco_unit").max(0) as u64,
            taxa: v.get::<i64, _>("taxa").max(0) as u64,
            liquido: v.get::<i64, _>("liquido").max(0) as u64,
            quando: v.get("quando"),
            vendi: v.get("vendi"),
        })
        .collect();
    let tp = razao::saldo(central, conta).await?;
    let compras = compras_de_vagas(central, realm, personagem).await?;
    Ok((
        ativos
            .iter()
            .map(|l| anuncio_de(l, realm, personagem))
            .collect(),
        historico,
        tp,
        compras,
    ))
}

/// Cartas ainda nao entregues, sem as que o realm ja' aplicou (e so' falta
/// avisar o central).
pub async fn cartas_pendentes(
    central: &PgPool,
    realm_pool: &PgPool,
    realm: &str,
    personagem: &str,
) -> Result<Vec<CartaNet>> {
    let linhas = sqlx::query(
        "SELECT id, item_id, qtd, instancia, gold, motivo FROM mercado_cartas
          WHERE realm = $1 AND personagem = $2 AND entregue IS NULL ORDER BY criada LIMIT 100",
    )
    .bind(realm)
    .bind(personagem)
    .fetch_all(central)
    .await?;
    let aplicadas: HashSet<String> = sqlx::query_scalar::<_, String>(
        "SELECT id FROM mercado_cartas_aplicadas WHERE personagem = $1 AND avisada IS NULL",
    )
    .bind(personagem)
    .fetch_all(realm_pool)
    .await?
    .into_iter()
    .collect();
    let cartas: Vec<CartaNet> = linhas
        .iter()
        .map(|l| {
            let inst: Option<String> = l.get("instancia");
            CartaNet {
                id: l.get("id"),
                item_id: l.get::<i32, _>("item_id") as u16,
                qtd: l.get::<i64, _>("qtd").max(0) as u64,
                instancia: inst.and_then(|s| serde_json::from_str(&s).ok()),
                gold: l.get::<i64, _>("gold").max(0) as u64,
                motivo: l.get("motivo"),
            }
        })
        .collect();
    Ok(regras::cartas_a_aplicar(&cartas, &aplicadas)
        .into_iter()
        .cloned()
        .collect())
}

pub async fn saldo_tp(central: &PgPool, conta: &str) -> Result<u64> {
    razao::saldo(central, conta).await
}

// ─────────────────────────────── buy orders ───────────────────────────────

/// Places a buy order whose gold (`pago`) already left the buyer. Anything
/// wrong (stale, over the limit, a total that does not match) gives the gold
/// back by letter.
#[allow(clippy::too_many_arguments)]
async fn pedir_compra(
    tx: &mut Transaction<'_, Postgres>,
    id: &str,
    realm: &str,
    conta: &str,
    personagem: &str,
    item_id: u16,
    nome: &str,
    categoria: u8,
    qtd: u64,
    pago: u64,
) -> Result<(bool, String)> {
    // Per character: two orders at once never pass the limit together.
    sqlx::query("SELECT pg_advisory_xact_lock(hashtext($1))")
        .bind(format!("pedidos:{realm}:{personagem}"))
        .execute(&mut **tx)
        .await?;
    let recusa = if qtd == 0
        || qtd > regras::QTD_MAX_POR_PEDIDO
        || item_id == 0
        || item_id == shared::item_id::PERGAMINHO_TELEPORTE
    {
        Some(Recusa::Quantidade)
    } else if !regras::preco_total_valido(qtd, pago) {
        Some(Recusa::Preco)
    } else if pedidos_abertos_de(tx, realm, personagem).await? >= regras::MAX_PEDIDOS {
        Some(Recusa::MuitosPedidos)
    } else {
        None
    };
    if let Some(r) = recusa {
        if pago > 0 {
            carta(
                tx,
                &format!("{id}:devolve"),
                realm,
                personagem,
                0,
                0,
                None,
                pago,
                &format!("Buy order refused: {nome} (gold refunded)"),
            )
            .await?;
        }
        return Ok((false, format!("{} The gold comes back in Deliveries.", r.texto())));
    }
    sqlx::query(
        "INSERT INTO mercado_pedidos (id, realm, conta, personagem, item_id, nome, categoria, qtd_total, qtd_restante, preco_unit, gold_restante)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $8, $9, $10)",
    )
    .bind(id)
    .bind(realm)
    .bind(conta)
    .bind(personagem)
    .bind(item_id as i32)
    .bind(nome)
    .bind(categoria as i16)
    .bind(qtd as i64)
    .bind(unit_arredondado(qtd, pago))
    .bind(pago as i64)
    .execute(&mut **tx)
    .await?;
    Ok((
        true,
        format!(
            "Buy order placed: {nome} ×{qtd} for {} gold ({} each, held now).",
            shared::idioma::milhar(pago),
            fmt_unit(qtd, pago)
        ),
    ))
}

async fn pedidos_abertos_de(
    tx: &mut Transaction<'_, Postgres>,
    realm: &str,
    personagem: &str,
) -> Result<usize> {
    let n: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM mercado_pedidos WHERE realm = $1 AND personagem = $2 AND estado = 0",
    )
    .bind(realm)
    .bind(personagem)
    .fetch_one(&mut **tx)
    .await?;
    Ok(n.max(0) as usize)
}

/// Fills an order with `qtd` that already left the seller. The order row is
/// locked: two sellers at once never fill more than it wants. The buyer gets
/// the items, the seller the gold (minus the fee) and what the order did not
/// take — all by letter.
async fn vender_ao_pedido(
    tx: &mut Transaction<'_, Postgres>,
    id: &str,
    realm: &str,
    personagem: &str,
    pedido: &str,
    item_id: u16,
    qtd: u64,
) -> Result<(bool, String)> {
    let linha = sqlx::query(
        "SELECT realm, personagem, item_id, nome, qtd_restante, gold_restante, estado
           FROM mercado_pedidos WHERE id = $1 FOR UPDATE",
    )
    .bind(pedido)
    .fetch_optional(&mut **tx)
    .await?;
    let preenchido = match &linha {
        None => Err(Recusa::PedidoFechado),
        Some(l) if l.get::<i32, _>("item_id") as u16 != item_id => Err(Recusa::PedidoFechado),
        Some(l)
            if l.get::<String, _>("realm") == realm
                && l.get::<String, _>("personagem") == personagem =>
        {
            Err(Recusa::ProprioPedido)
        }
        Some(l) => regras::preencher_pedido(
            l.get::<i64, _>("qtd_restante").max(0) as u64,
            l.get::<i16, _>("estado") as u8,
            l.get::<i64, _>("gold_restante").max(0) as u64,
            qtd,
        ),
    };
    let (l, p) = match (linha, preenchido) {
        (Some(l), Ok(p)) => (l, p),
        (_, Err(r)) => {
            // Everything goes back to the seller.
            if qtd > 0 && item_id != 0 {
                carta(
                    tx,
                    &format!("{id}:devolve"),
                    realm,
                    personagem,
                    item_id,
                    qtd,
                    None,
                    0,
                    "The sale to a buy order didn't go through: items returned",
                )
                .await?;
            }
            return Ok((false, r.texto()));
        }
        (None, Ok(_)) => unreachable!("preenchimento sem pedido"),
    };
    let nome: String = l.get("nome");
    let comprador_realm: String = l.get("realm");
    let comprador: String = l.get("personagem");
    let estado = if p.restante == 0 { regras::PEDIDO_CONCLUIDO } else { regras::PEDIDO_ABERTO };
    sqlx::query(
        "UPDATE mercado_pedidos SET qtd_restante = $2, estado = $3, gold_restante = $4, preco_unit = $5, atualizado = NOW() WHERE id = $1",
    )
    .bind(pedido)
    .bind(p.restante as i64)
    .bind(estado as i16)
    .bind(p.gold_restante as i64)
    .bind(if p.restante == 0 { unit_arredondado(p.vendida, p.bruto) } else { unit_arredondado(p.restante, p.gold_restante) })
    .execute(&mut **tx)
    .await?;
    carta(
        tx,
        &format!("{id}:item"),
        &comprador_realm,
        &comprador,
        item_id,
        p.vendida,
        None,
        0,
        &format!("Buy order filled: {nome}"),
    )
    .await?;
    if p.liquido > 0 {
        carta(
            tx,
            &format!("{id}:venda"),
            realm,
            personagem,
            0,
            0,
            None,
            p.liquido,
            &format!("Sold to a buy order: {nome} ×{} (fee {})", p.vendida, p.taxa),
        )
        .await?;
    }
    if p.devolve > 0 {
        carta(
            tx,
            &format!("{id}:sobra"),
            realm,
            personagem,
            item_id,
            p.devolve,
            None,
            0,
            &format!("The buy order wanted fewer: {nome} returned"),
        )
        .await?;
    }
    sqlx::query(
        "INSERT INTO mercado_vendas (anuncio, tipo, nome, qtd, preco_unit, bruto, taxa, liquido, vendedor_realm, vendedor, comprador_realm, comprador)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)",
    )
    .bind(pedido)
    .bind(regras::TIPO_ITEM as i16)
    .bind(&nome)
    .bind(p.vendida as i64)
    .bind(unit_arredondado(p.vendida, p.bruto))
    .bind(p.bruto as i64)
    .bind(p.taxa as i64)
    .bind(p.liquido as i64)
    .bind(realm)
    .bind(personagem)
    .bind(&comprador_realm)
    .bind(&comprador)
    .execute(&mut **tx)
    .await?;
    let mut texto = format!(
        "Sold {nome} ×{} to a buy order: {} gold in Deliveries.",
        p.vendida, p.liquido
    );
    if p.devolve > 0 {
        texto.push_str(&format!(" {} returned (the order wanted fewer).", p.devolve));
    }
    Ok((true, texto))
}

/// Cancels one of my buy orders: the gold for what is left comes back by
/// letter. Nothing left the character, so it is a direct call.
pub async fn cancelar_pedido(
    central: &PgPool,
    realm: &str,
    personagem: &str,
    pedido: &str,
) -> Result<(bool, String)> {
    let mut tx = central.begin().await?;
    let Some(l) = sqlx::query(
        "SELECT nome, gold_restante, estado FROM mercado_pedidos
          WHERE id = $1 AND realm = $2 AND personagem = $3 FOR UPDATE",
    )
    .bind(pedido)
    .bind(realm)
    .bind(personagem)
    .fetch_optional(&mut *tx)
    .await?
    else {
        return Ok((false, "Buy order not found.".into()));
    };
    if l.get::<i16, _>("estado") as u8 != regras::PEDIDO_ABERTO {
        return Ok((false, "The buy order is no longer open.".into()));
    }
    let gold_restante = l.get::<i64, _>("gold_restante").max(0) as u64;
    let nome: String = l.get("nome");
    sqlx::query("UPDATE mercado_pedidos SET estado = $2, qtd_restante = 0, gold_restante = 0, atualizado = NOW() WHERE id = $1")
        .bind(pedido)
        .bind(regras::PEDIDO_CANCELADO as i16)
        .execute(&mut *tx)
        .await?;
    let gold = regras::reembolso_do_pedido(gold_restante);
    if gold > 0 {
        carta(
            &mut tx,
            &format!("{pedido}:cancelado"),
            realm,
            personagem,
            0,
            0,
            None,
            gold,
            &format!("Buy order cancelled: {nome} (gold refunded)"),
        )
        .await?;
    }
    tx.commit().await?;
    Ok((true, format!("Buy order cancelled: {gold} gold comes back in Deliveries.")))
}

fn pedido_de(l: &sqlx::postgres::PgRow, realm: &str, personagem: &str) -> PedidoNet {
    let r: String = l.get("realm");
    let p: String = l.get("personagem");
    PedidoNet {
        id: l.get("id"),
        item_id: l.get::<i32, _>("item_id") as u16,
        nome: l.get("nome"),
        categoria: l.get::<i16, _>("categoria") as u8,
        qtd_total: l.get::<i64, _>("qtd_total").max(0) as u64,
        qtd: l.get::<i64, _>("qtd_restante").max(0) as u64,
        preco_total: l.get::<i64, _>("gold_restante").max(0) as u64,
        meu: r == realm && p == personagem,
        realm: r,
        comprador: p,
    }
}

/// Open buy orders, best price for the seller first.
pub async fn buscar_pedidos(
    central: &PgPool,
    item_id: Option<u16>,
    texto: &str,
    realm: &str,
    personagem: &str,
) -> Result<Vec<PedidoNet>> {
    let texto: String = texto
        .chars()
        .filter(|c| !matches!(c, '%' | '_' | '\\'))
        .take(regras::BUSCA_MAX_CHARS)
        .collect();
    let linhas = sqlx::query(
        "SELECT id, realm, personagem, item_id, nome, categoria, qtd_total, qtd_restante, gold_restante
           FROM mercado_pedidos
          WHERE estado = 0
            AND ($1 < 0 OR item_id = $1)
            AND ($2 = '' OR nome ILIKE '%' || $2 || '%')
          ORDER BY gold_restante::float8 / GREATEST(qtd_restante, 1) DESC, criado ASC
          LIMIT $3",
    )
    .bind(item_id.map_or(-1, |i| i as i32))
    .bind(texto.trim())
    .bind(regras::PEDIDOS_POR_BUSCA as i64)
    .fetch_all(central)
    .await?;
    Ok(linhas.iter().map(|l| pedido_de(l, realm, personagem)).collect())
}

/// My open buy orders.
pub async fn meus_pedidos(central: &PgPool, realm: &str, personagem: &str) -> Result<Vec<PedidoNet>> {
    let linhas = sqlx::query(
        "SELECT id, realm, personagem, item_id, nome, categoria, qtd_total, qtd_restante, gold_restante
           FROM mercado_pedidos WHERE realm = $1 AND personagem = $2 AND estado = 0 ORDER BY criado DESC",
    )
    .bind(realm)
    .bind(personagem)
    .fetch_all(central)
    .await?;
    Ok(linhas.iter().map(|l| pedido_de(l, realm, personagem)).collect())
}

// ─────────────────────────────── relay ───────────────────────────────

/// O que o relay e as consultas devolvem ao loop do mundo.
#[derive(Debug)]
pub enum Evento {
    /// Operacao fechou no central: avisa o personagem, se estiver aqui.
    Aviso {
        personagem: String,
        ok: bool,
        texto: String,
    },
    /// Cartas pra aplicar (pedido "Receive").
    Cartas {
        sid: SessionId,
        personagem: String,
        cartas: Vec<CartaNet>,
    },
}

/// Liga o realm ao central. Um por processo; varios processos do mesmo realm
/// podem mandar a mesma linha — o central processa uma vez.
pub fn spawn_relay(realm: PgPool, tx: mpsc::UnboundedSender<IncomingMessage>) {
    let Some(central) = central() else { return };
    tokio::spawn(async move {
        loop {
            tokio::select! {
                _ = ACORDA.notified() => {}
                _ = tokio::time::sleep(Duration::from_secs(3)) => {}
            }
            if let Err(e) = enviar_saida(&realm, &central, &tx).await {
                tracing::warn!("market: send to central failed (will retry): {e:#}");
            }
            if let Err(e) = avisar_entregues(&realm, &central).await {
                tracing::warn!("market: delivery notice failed (will retry): {e:#}");
            }
        }
    });
}

pub async fn enviar_saida(
    realm: &PgPool,
    central: &PgPool,
    tx: &mpsc::UnboundedSender<IncomingMessage>,
) -> Result<usize> {
    let pendentes: Vec<(String, String)> = sqlx::query_as(
        "SELECT id, payload FROM mercado_saida WHERE enviada IS NULL ORDER BY criada LIMIT 50",
    )
    .fetch_all(realm)
    .await?;
    let mut feitas = 0;
    for (id, payload) in pendentes {
        let op: OpCentral = match serde_json::from_str(&payload) {
            Ok(op) => op,
            Err(e) => {
                // Nunca marca como enviada: e' item/gold de alguem. Fica pra
                // correcao a' mao.
                tracing::error!("market: unreadable payload in mercado_saida {id}: {e}");
                continue;
            }
        };
        let r = processar(central, &op).await?;
        if r.novo && r.ok {
            // Instant: every process tells its players the market changed —
            // the one who did it, and on a purchase the seller too.
            let mut quem = vec![op.personagem().to_string()];
            if let OpCentral::Comprar { anuncio, .. } = &op {
                if let Ok(Some(v)) = sqlx::query_scalar::<_, String>("SELECT personagem FROM mercado_anuncios WHERE id = $1")
                    .bind(anuncio)
                    .fetch_optional(central)
                    .await
                {
                    quem.push(v);
                }
            }
            if let OpCentral::VenderAoPedido { pedido, .. } = &op {
                if let Ok(Some(v)) = sqlx::query_scalar::<_, String>("SELECT personagem FROM mercado_pedidos WHERE id = $1")
                    .bind(pedido)
                    .fetch_optional(central)
                    .await
                {
                    quem.push(v);
                }
            }
            crate::chat_global::mercado_mudou(quem);
        }
        sqlx::query("UPDATE mercado_saida SET enviada = NOW(), resultado = $2 WHERE id = $1")
            .bind(&id)
            .bind(&r.texto)
            .execute(realm)
            .await?;
        if r.novo {
            // The answer goes to the channel where the character IS: every
            // channel runs this relay, and the one that won the row may not be
            // theirs (then the buyer never heard "Bought" and kept a stale list
            // — owner, 10/11/2026, bought a sold-out listing and got a refund).
            let quem = op.personagem().to_string();
            if !crate::chat_global::mercado_aviso(quem.clone(), r.ok, r.texto.clone()) {
                let _ = tx.send(IncomingMessage::Mercado(Evento::Aviso {
                    personagem: quem,
                    ok: r.ok,
                    texto: r.texto,
                }));
            }
        }
        feitas += 1;
    }
    Ok(feitas)
}

pub async fn avisar_entregues(realm: &PgPool, central: &PgPool) -> Result<usize> {
    let ids: Vec<String> = sqlx::query_scalar(
        "SELECT id FROM mercado_cartas_aplicadas WHERE avisada IS NULL LIMIT 500",
    )
    .fetch_all(realm)
    .await?;
    if ids.is_empty() {
        return Ok(0);
    }
    sqlx::query(
        "UPDATE mercado_cartas SET entregue = NOW() WHERE id = ANY($1) AND entregue IS NULL",
    )
    .bind(&ids)
    .execute(central)
    .await?;
    sqlx::query("UPDATE mercado_cartas_aplicadas SET avisada = NOW() WHERE id = ANY($1)")
        .bind(&ids)
        .execute(realm)
        .await?;
    Ok(ids.len())
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn ids_nao_se_repetem() {
        let ids: HashSet<String> = (0..10_000).map(|_| novo_id()).collect();
        assert_eq!(ids.len(), 10_000);
        assert!(ids.iter().all(|i| i.len() == 32));
    }

    #[test]
    fn op_vai_e_volta_em_json() {
        let op = OpCentral::Comprar {
            id: "x".into(),
            realm: "SA01".into(),
            conta: "SA01:1".into(),
            personagem: "Ana".into(),
            anuncio: "a".into(),
            qtd: 3,
            preco_unit: 7,
            pago: 21,
        };
        let volta: OpCentral = serde_json::from_str(&serde_json::to_string(&op).unwrap()).unwrap();
        assert_eq!((volta.id(), volta.personagem()), ("x", "Ana"));
    }

    #[test]
    fn conta_sem_id_ainda_e_unica_por_personagem() {
        assert_eq!(conta_global("SA01", Some(7), "Ana"), "SA01:7");
        assert_eq!(conta_global("SA01", None, "Ana"), "SA01:personagem:Ana");
    }

    /// Slot purchases: gold through the outbox, then TP; a stale purchase
    /// pays nothing (gold back by letter, TP untouched).
    #[tokio::test]
    async fn vagas_com_ouro_e_tp_no_postgres() {
        let Ok(url) = std::env::var("DATABASE_URL_CENTRAL_TESTE") else {
            eprintln!("DATABASE_URL_CENTRAL_TESTE nao setada: teste de vagas pulado");
            return;
        };
        let central = abrir_central(&url).await.expect("central");
        let tag = &novo_id()[..10];
        let (quem, conta) = (format!("vagas_{tag}"), format!("T:{tag}:vagas"));
        let gold = |compra: u8| OpCentral::ComprarVagas {
            id: novo_id(),
            realm: "T".into(),
            personagem: quem.clone(),
            compra,
            pago: regras::CUSTO_VAGAS_OURO[compra as usize - 1],
        };
        let r = processar(&central, &gold(1)).await.unwrap();
        assert!(r.ok, "{}", r.texto);
        assert_eq!(compras_de_vagas(&central, "T", &quem).await.unwrap(), 1);
        // The same purchase again (a double click): refused, gold back by letter.
        let repetida = gold(1);
        let r = processar(&central, &repetida).await.unwrap();
        assert!(!r.ok);
        let devolvido: i64 = sqlx::query_scalar("SELECT gold FROM mercado_cartas WHERE id = $1")
            .bind(format!("{}:devolve", repetida.id()))
            .fetch_one(&central)
            .await
            .unwrap();
        assert_eq!(devolvido as u64, regras::CUSTO_VAGAS_OURO[0]);
        assert_eq!(compras_de_vagas(&central, "T", &quem).await.unwrap(), 1);
        // TP: none on the account, then enough for purchase 2.
        let (ok, _) = comprar_vagas_tp(&central, "T", &conta, &quem, 2).await.unwrap();
        assert!(!ok);
        let mut tx = central.begin().await.unwrap();
        razao::mover(&mut tx, &conta, 1_000, "test credit", None).await.unwrap();
        tx.commit().await.unwrap();
        let (ok, texto) = comprar_vagas_tp(&central, "T", &conta, &quem, 2).await.unwrap();
        assert!(ok, "{texto}");
        assert_eq!(razao::saldo(&central, &conta).await.unwrap(), 1_000 - regras::CUSTO_VAGAS_TP[1]);
        assert_eq!(regras::max_anuncios(compras_de_vagas(&central, "T", &quem).await.unwrap()), 30);
        let (ok, _) = comprar_vagas_tp(&central, "T", &conta, &quem, 2).await.unwrap();
        assert!(!ok, "purchase 2 twice must not charge again");
        assert_eq!(razao::saldo(&central, &conta).await.unwrap(), 1_000 - regras::CUSTO_VAGAS_TP[1]);
    }

    /// Buy orders end to end on a throwaway Postgres
    /// (`DATABASE_URL_CENTRAL_TESTE`; skipped without it): escrow, partial
    /// fill, overfill returned, own order refused, cancel refunds the rest.
    #[tokio::test]
    async fn pedidos_de_compra_no_postgres() {
        let Ok(url) = std::env::var("DATABASE_URL_CENTRAL_TESTE") else {
            eprintln!("DATABASE_URL_CENTRAL_TESTE nao setada: teste de pedidos pulado");
            return;
        };
        let central = abrir_central(&url).await.expect("central");
        criar_tabelas_do_realm(&central).await.expect("tabelas do realm");
        let tag = &novo_id()[..10];
        let (comp, vend) = (format!("pcomp_{tag}"), format!("pvend_{tag}"));
        let gold_de = |cartas: &[CartaNet]| cartas.iter().map(|c| c.gold).sum::<u64>();
        let itens_de = |cartas: &[CartaNet], it: u16| {
            cartas.iter().filter(|c| c.item_id == it).map(|c| c.qtd).sum::<u64>()
        };
        let pedido = novo_id();
        let pedir = OpCentral::PedirCompra {
            id: pedido.clone(),
            realm: "T".into(),
            conta: format!("T:{tag}:c"),
            personagem: comp.clone(),
            item_id: 345,
            nome: format!("Darksteel {tag}"),
            categoria: regras::Categoria::Material as u8,
            qtd: 10,
            preco_unit: 7,
            pago: 70,
        };
        let r = processar(&central, &pedir).await.unwrap();
        assert!(r.ok, "{}", r.texto);
        assert!(!processar(&central, &pedir).await.unwrap().novo, "placed once");
        // A bad order (no quantity): refused, gold back. (Since lot pricing
        // any whole total is a valid price: 69 for 10 is 6.9 each.)
        let errado = OpCentral::PedirCompra {
            id: novo_id(),
            realm: "T".into(),
            conta: format!("T:{tag}:c"),
            personagem: comp.clone(),
            item_id: 345,
            nome: format!("Darksteel {tag}"),
            categoria: 2,
            qtd: 0,
            preco_unit: 7,
            pago: 69,
        };
        assert!(!processar(&central, &errado).await.unwrap().ok);
        assert_eq!(gold_de(&cartas_pendentes(&central, &central, "T", &comp).await.unwrap()), 69);
        let lista = buscar_pedidos(&central, Some(345), tag, "T", &vend).await.unwrap();
        assert_eq!(lista.len(), 1);
        assert_eq!((lista[0].qtd, lista[0].preco_total, lista[0].meu), (10, 70, false));

        let vender = |qtd: u64, quem: &str, item_id: u16| OpCentral::VenderAoPedido {
            id: novo_id(),
            realm: "T".into(),
            conta: format!("T:{tag}:v"),
            personagem: quem.to_string(),
            pedido: pedido.clone(),
            item_id,
            qtd,
        };
        // The buyer selling to their own order: refused, items back.
        let proprio = vender(2, &comp, 345);
        assert!(!processar(&central, &proprio).await.unwrap().ok);
        assert_eq!(itens_de(&cartas_pendentes(&central, &central, "T", &comp).await.unwrap(), 345), 2);
        // Wrong item: refused, the same item goes back.
        assert!(!processar(&central, &vender(3, &vend, 60)).await.unwrap().ok);
        assert_eq!(itens_de(&cartas_pendentes(&central, &central, "T", &vend).await.unwrap(), 60), 3);
        // 4 sell: 28 gross, 26 to the seller, 4 to the buyer.
        let quatro = vender(4, &vend, 345);
        assert!(processar(&central, &quatro).await.unwrap().ok);
        assert!(!processar(&central, &quatro).await.unwrap().novo, "sold once");
        // 9 offered, 6 wanted: 6 sell (42 gross, 39 net), 3 come back.
        assert!(processar(&central, &vender(9, &vend, 345)).await.unwrap().ok);
        let v = cartas_pendentes(&central, &central, "T", &vend).await.unwrap();
        assert_eq!(gold_de(&v), 26 + 39);
        assert_eq!(itens_de(&v, 345), 3);
        let c = cartas_pendentes(&central, &central, "T", &comp).await.unwrap();
        assert_eq!(itens_de(&c, 345), 2 + 4 + 6);
        // Filled: gone from the search, nothing to cancel, one more sale bounces.
        assert!(buscar_pedidos(&central, Some(345), tag, "T", &vend).await.unwrap().is_empty());
        assert!(!cancelar_pedido(&central, "T", &comp, &pedido).await.unwrap().0);
        assert!(!processar(&central, &vender(1, &vend, 345)).await.unwrap().ok);
        let (_, historico, _, _) = meus(&central, "T", &vend, "x").await.unwrap();
        assert_eq!(historico.iter().filter(|h| h.vendi).map(|h| h.liquido).sum::<u64>(), 26 + 39);

        // Energy order, part filled then cancelled: the rest's gold comes back.
        let energia = novo_id();
        let r = processar(&central, &OpCentral::PedirCompra {
            id: energia.clone(),
            realm: "T".into(),
            conta: format!("T:{tag}:c"),
            personagem: comp.clone(),
            item_id: shared::item_id::ENERGIA_MIL,
            nome: "Energy ×1,000".into(),
            categoria: 3,
            qtd: 5,
            preco_unit: 40,
            pago: 200,
        })
        .await
        .unwrap();
        assert!(r.ok, "{}", r.texto);
        assert_eq!(meus_pedidos(&central, "T", &comp).await.unwrap().len(), 1);
        let lote = OpCentral::VenderAoPedido {
            id: novo_id(),
            realm: "T".into(),
            conta: format!("T:{tag}:v"),
            personagem: vend.clone(),
            pedido: energia.clone(),
            item_id: shared::item_id::ENERGIA_MIL,
            qtd: 2,
        };
        assert!(processar(&central, &lote).await.unwrap().ok);
        let antes = gold_de(&cartas_pendentes(&central, &central, "T", &comp).await.unwrap());
        assert!(!cancelar_pedido(&central, "T", &vend, &energia).await.unwrap().0, "not theirs");
        assert!(cancelar_pedido(&central, "T", &comp, &energia).await.unwrap().0);
        assert!(!cancelar_pedido(&central, "T", &comp, &energia).await.unwrap().0, "once");
        let c = cartas_pendentes(&central, &central, "T", &comp).await.unwrap();
        assert_eq!(gold_de(&c) - antes, 3 * 40);
        assert_eq!(itens_de(&c, shared::item_id::ENERGIA_MIL), 2);
        assert!(meus_pedidos(&central, "T", &comp).await.unwrap().is_empty());
    }

    /// Fluxo inteiro num Postgres descartavel: `DATABASE_URL_CENTRAL_TESTE`.
    /// Sem a variavel o teste nao faz nada (e diz). As tabelas do realm
    /// entram no mesmo banco.
    #[tokio::test]
    async fn custodia_cartas_e_tp_no_postgres() {
        let Ok(url) = std::env::var("DATABASE_URL_CENTRAL_TESTE") else {
            eprintln!(
                "DATABASE_URL_CENTRAL_TESTE nao setada: teste de integracao do mercado pulado"
            );
            return;
        };
        let central = abrir_central(&url).await.expect("central");
        criar_tabelas_do_realm(&central)
            .await
            .expect("tabelas do realm");
        let realm_pool = central.clone();
        let tag = &novo_id()[..10];
        let (vend, comp) = (format!("vend_{tag}"), format!("comp_{tag}"));
        let (conta_v, conta_c) = (format!("T:{tag}:v"), format!("T:{tag}:c"));
        let (tx, mut rx) = mpsc::unbounded_channel();

        // Anuncia 10 a 7: pela saida do realm, como o jogo faz.
        let anuncio = novo_id();
        let op = OpCentral::Anunciar {
            id: anuncio.clone(),
            realm: "T".into(),
            conta: conta_v.clone(),
            personagem: vend.clone(),
            item_id: 60,
            nome: format!("Madeira {tag}"),
            categoria: regras::Categoria::Material as u8,
            instancia: None,
            qtd: 10,
            // An op queued before lot pricing (no total): read as 10 × 7.
            preco_total: 0,
            preco_unit: 7,
        };
        let mut t = realm_pool.begin().await.unwrap();
        gravar_registros(&mut t, &[Registro::Saida(op.clone())])
            .await
            .unwrap();
        gravar_registros(&mut t, &[Registro::Saida(op.clone())])
            .await
            .unwrap();
        t.commit().await.unwrap();
        assert!(enviar_saida(&realm_pool, &central, &tx).await.unwrap() >= 1);
        assert!(matches!(
            rx.try_recv(),
            Ok(IncomingMessage::Mercado(Evento::Aviso { ok: true, .. }))
        ));
        let de_novo = processar(&central, &op).await.unwrap();
        assert!(
            de_novo.ok && !de_novo.novo,
            "a mesma operacao nao anuncia duas vezes"
        );

        let filtro = FiltroNet {
            categoria: 0,
            texto: tag.to_string(),
            pagina: 0,
        };
        let (lista, _) = buscar(&central, &filtro, "T", &comp).await.unwrap();
        assert_eq!(lista.len(), 1);
        assert_eq!((lista[0].qtd, lista[0].meu), (10, false));

        // Compra parcial de 4: item pro comprador, 26 de gold pro vendedor.
        let compra = OpCentral::Comprar {
            id: novo_id(),
            realm: "T".into(),
            conta: conta_c.clone(),
            personagem: comp.clone(),
            anuncio: anuncio.clone(),
            qtd: 4,
            preco_unit: 7,
            pago: 28,
        };
        assert!(processar(&central, &compra).await.unwrap().ok);
        assert!(
            !processar(&central, &compra).await.unwrap().novo,
            "compra repetida nao compra de novo"
        );
        let c = cartas_pendentes(&central, &realm_pool, "T", &comp)
            .await
            .unwrap();
        assert_eq!(c.len(), 1);
        assert_eq!((c[0].item_id, c[0].qtd, c[0].gold), (60, 4, 0));
        let v = cartas_pendentes(&central, &realm_pool, "T", &vend)
            .await
            .unwrap();
        assert_eq!(v.iter().map(|c| c.gold).sum::<u64>(), 26);

        // Carta aplicada no realm some das pendentes; o aviso marca entregue.
        let mut t = realm_pool.begin().await.unwrap();
        let aplicada = Registro::CartaAplicada {
            id: c[0].id.clone(),
            personagem: comp.clone(),
        };
        gravar_registros(&mut t, &[aplicada.clone(), aplicada])
            .await
            .unwrap();
        t.commit().await.unwrap();
        assert!(cartas_pendentes(&central, &realm_pool, "T", &comp)
            .await
            .unwrap()
            .is_empty());
        avisar_entregues(&realm_pool, &central).await.unwrap();
        assert!(cartas_pendentes(&central, &realm_pool, "T", &comp)
            .await
            .unwrap()
            .is_empty());

        // Comprar mais do que sobrou: nada fecha, gold volta.
        let demais = OpCentral::Comprar {
            id: novo_id(),
            realm: "T".into(),
            conta: conta_c.clone(),
            personagem: comp.clone(),
            anuncio: anuncio.clone(),
            qtd: 7,
            preco_unit: 7,
            pago: 49,
        };
        assert!(!processar(&central, &demais).await.unwrap().ok);
        let reemb = cartas_pendentes(&central, &realm_pool, "T", &comp)
            .await
            .unwrap();
        assert_eq!(reemb.iter().map(|c| c.gold).sum::<u64>(), 49);

        // Cancela: as 6 que sobraram voltam; cancelar de novo nao devolve mais.
        assert!(cancelar(&central, "T", &vend, &anuncio).await.unwrap().0);
        assert!(!cancelar(&central, "T", &vend, &anuncio).await.unwrap().0);
        let volta = cartas_pendentes(&central, &realm_pool, "T", &vend)
            .await
            .unwrap();
        assert_eq!(
            volta
                .iter()
                .filter(|c| c.item_id == 60)
                .map(|c| c.qtd)
                .sum::<u64>(),
            6
        );

        // TP: credita 100, anuncia 30 (sai do livro), compra 10, cancela o resto.
        assert_eq!(
            razao::creditar(
                &central,
                &conta_v,
                100,
                "teste",
                Some(&format!("{tag}:credito"))
            )
            .await
            .unwrap(),
            razao::Movimento::Feito { saldo: 100 }
        );
        assert!(matches!(
            razao::creditar(
                &central,
                &conta_v,
                100,
                "teste",
                Some(&format!("{tag}:credito"))
            )
            .await
            .unwrap(),
            razao::Movimento::JaFeito { saldo: 100 }
        ));
        assert!(
            !anunciar_tp(&central, "T", &conta_v, &vend, 101, 505)
                .await
                .unwrap()
                .0,
            "sem TP bastante"
        );
        assert!(
            anunciar_tp(&central, "T", &conta_v, &vend, 30, 150)
                .await
                .unwrap()
                .0
        );
        assert_eq!(saldo_tp(&central, &conta_v).await.unwrap(), 70);
        let tp = FiltroNet {
            categoria: regras::Categoria::Tp as u8,
            texto: String::new(),
            pagina: 0,
        };
        let (anuncios_tp, _) = buscar(&central, &tp, "T", &comp).await.unwrap();
        let meu_tp = anuncios_tp
            .iter()
            .find(|a| a.vendedor == vend)
            .expect("anuncio de TP");
        let compra_tp = OpCentral::Comprar {
            id: novo_id(),
            realm: "T".into(),
            conta: conta_c.clone(),
            personagem: comp.clone(),
            anuncio: meu_tp.id.clone(),
            qtd: 10,
            preco_unit: 5,
            pago: 50,
        };
        assert!(processar(&central, &compra_tp).await.unwrap().ok);
        assert_eq!(saldo_tp(&central, &conta_c).await.unwrap(), 10);
        assert!(cancelar(&central, "T", &vend, &meu_tp.id).await.unwrap().0);
        assert_eq!(
            saldo_tp(&central, &conta_v).await.unwrap(),
            90,
            "as 20 que sobraram voltam"
        );
        let (_, historico, _, _) = meus(&central, "T", &vend, &conta_v).await.unwrap();
        assert_eq!(
            historico
                .iter()
                .filter(|h| h.vendi)
                .map(|h| h.liquido)
                .sum::<u64>(),
            26 + 47
        );
    }
}
