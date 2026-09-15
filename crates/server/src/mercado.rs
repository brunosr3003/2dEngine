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
use shared::mercado::{self as regras, AnuncioNet, CartaNet, FiltroNet, Recusa, VendaNet};
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
        sqlx::query("SELECT pg_advisory_lock(728431)").execute(&mut *trava).await?;
        let r = criar_tabelas_do_realm(realm).await;
        let _ = sqlx::query("SELECT pg_advisory_unlock(728431)").execute(&mut *trava).await;
        r?;
    }
    let url = std::env::var("DATABASE_URL_CENTRAL").unwrap_or_default();
    if url.trim().is_empty() {
        tracing::warn!("mercado: DATABASE_URL_CENTRAL nao setada — mercado global DESLIGADO neste processo");
        return Ok(());
    }
    match abrir_central(&url).await {
        Ok(pool) => {
            let _ = CENTRAL.set(pool);
            tracing::info!("mercado: banco central conectado — mercado global ligado");
        }
        Err(e) => tracing::error!("mercado: banco central nao abriu ({e:#}) — mercado global DESLIGADO"),
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
    sqlx::query("SELECT pg_advisory_lock(728432)").execute(&mut *trava).await?;
    let r = criar_tabelas_centrais(&pool).await;
    let _ = sqlx::query("SELECT pg_advisory_unlock(728432)").execute(&mut *trava).await;
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
    ] {
        sqlx::query(sql).execute(pool).await?;
    }
    razao::criar_tabela(pool).await
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
        preco_unit: u64,
    },
    Comprar {
        id: String,
        realm: String,
        conta: String,
        personagem: String,
        anuncio: String,
        qtd: u64,
        preco_unit: u64,
        /// Gold que ja' saiu do comprador.
        pago: u64,
    },
}

impl OpCentral {
    pub fn id(&self) -> &str {
        match self {
            OpCentral::Anunciar { id, .. } | OpCentral::Comprar { id, .. } => id,
        }
    }

    pub fn personagem(&self) -> &str {
        match self {
            OpCentral::Anunciar { personagem, .. } | OpCentral::Comprar { personagem, .. } => personagem,
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
        let (ok, texto): (bool, String) = sqlx::query_as("SELECT ok, resultado FROM mercado_operacoes WHERE id = $1")
            .bind(op.id())
            .fetch_one(central)
            .await?;
        return Ok(Resultado { ok, texto, novo: false });
    }
    let (ok, texto) = match op {
        OpCentral::Anunciar { id, realm, conta, personagem, item_id, nome, categoria, instancia, qtd, preco_unit } => {
            anunciar_item(&mut tx, id, realm, conta, personagem, *item_id, nome, *categoria, instancia, *qtd, *preco_unit).await?
        }
        OpCentral::Comprar { id, realm, conta, personagem, anuncio, qtd, preco_unit, pago } => {
            comprar(&mut tx, id, realm, conta, personagem, anuncio, *qtd, *preco_unit, *pago).await?
        }
    };
    sqlx::query("UPDATE mercado_operacoes SET ok = $2, resultado = $3 WHERE id = $1")
        .bind(op.id())
        .bind(ok)
        .bind(&texto)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(Resultado { ok, texto, novo: true })
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

async fn ativos_de(tx: &mut Transaction<'_, Postgres>, realm: &str, personagem: &str) -> Result<usize> {
    let n: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM mercado_anuncios WHERE realm = $1 AND personagem = $2 AND estado = 0")
        .bind(realm)
        .bind(personagem)
        .fetch_one(&mut **tx)
        .await?;
    Ok(n.max(0) as usize)
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
    preco_unit: u64,
) -> Result<(bool, String)> {
    let inst = instancia.as_ref().and_then(|i| serde_json::to_string(i).ok());
    // Travado por personagem: dois anuncios simultaneos nao passam juntos do limite.
    sqlx::query("SELECT pg_advisory_xact_lock(hashtext($1))").bind(format!("anuncios:{realm}:{personagem}")).execute(&mut **tx).await?;
    let recusa = if qtd == 0 || item_id == 0 {
        Some(Recusa::Quantidade)
    } else if !regras::preco_valido(preco_unit) {
        Some(Recusa::Preco)
    } else if ativos_de(tx, realm, personagem).await? >= regras::MAX_ANUNCIOS {
        Some(Recusa::MuitosAnuncios)
    } else {
        None
    };
    if let Some(r) = recusa {
        // O item ja' saiu da bolsa: volta por carta.
        carta(tx, &format!("{id}:devolve"), realm, personagem, item_id, qtd, inst.as_deref(), 0, &format!("Anúncio recusado: {nome}")).await?;
        return Ok((false, format!("{} {nome} volta em Entregas.", r.texto())));
    }
    sqlx::query(
        "INSERT INTO mercado_anuncios (id, tipo, realm, conta, personagem, item_id, nome, categoria, instancia, qtd_total, qtd_restante, preco_unit)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $10, $11)",
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
    .bind(preco_unit as i64)
    .execute(&mut **tx)
    .await?;
    Ok((true, format!("Anunciado: {nome} ×{qtd} a {preco_unit} gold cada.")))
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
    preco_unit: u64,
    pago: u64,
) -> Result<(bool, String)> {
    let linha = sqlx::query(
        "SELECT tipo, realm, personagem, item_id, nome, instancia, qtd_restante, preco_unit, estado
           FROM mercado_anuncios WHERE id = $1 FOR UPDATE",
    )
    .bind(anuncio)
    .fetch_optional(&mut **tx)
    .await?;
    let fechamento = match &linha {
        None => Err(Recusa::Esgotado),
        Some(l) if l.get::<String, _>("realm") == realm && l.get::<String, _>("personagem") == personagem => Err(Recusa::ProprioAnuncio),
        Some(l) => regras::fechar_compra(
            l.get::<i64, _>("qtd_restante").max(0) as u64,
            l.get::<i16, _>("estado") as u8,
            l.get::<i64, _>("preco_unit").max(0) as u64,
            qtd,
            preco_unit,
        )
        .and_then(|f| if f.bruto == pago { Ok(f) } else { Err(Recusa::PrecoMudou) }),
    };
    let (l, f) = match (linha, fechamento) {
        (Some(l), Ok(f)) => (l, f),
        (_, Err(r)) => {
            if pago > 0 {
                carta(tx, &format!("{id}:reembolso"), realm, personagem, 0, 0, None, pago, "Compra não fechou: gold devolvido").await?;
            }
            return Ok((false, r.texto()));
        }
        (None, Ok(_)) => unreachable!("fechamento sem anuncio"),
    };
    let tipo = l.get::<i16, _>("tipo") as u8;
    let nome: String = l.get("nome");
    let vendedor_realm: String = l.get("realm");
    let vendedor: String = l.get("personagem");
    let estado = if f.sobra == 0 { regras::ESTADO_ESGOTADO } else { regras::ESTADO_ATIVO };
    sqlx::query("UPDATE mercado_anuncios SET qtd_restante = $2, estado = $3, atualizado = NOW() WHERE id = $1")
        .bind(anuncio)
        .bind(f.sobra as i64)
        .bind(estado as i16)
        .execute(&mut **tx)
        .await?;
    let texto = if tipo == regras::TIPO_TP {
        match razao::mover(tx, conta, f.vendida as i64, "comprou TP no mercado", Some(&format!("{id}:tp"))).await? {
            razao::Movimento::Feito { .. } | razao::Movimento::JaFeito { .. } => {}
            razao::Movimento::SemSaldo { .. } => unreachable!("credito nunca fica sem saldo"),
        }
        format!("Comprado: {} TP.", f.vendida)
    } else {
        let inst: Option<String> = l.get("instancia");
        carta(tx, &format!("{id}:item"), realm, personagem, l.get::<i32, _>("item_id") as u16, f.vendida, inst.as_deref(), 0, &format!("Comprado no mercado: {nome}")).await?;
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
            &format!("Venda: {nome} ×{} (taxa {})", f.vendida, f.taxa),
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
    .bind(preco_unit as i64)
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
pub async fn cancelar(central: &PgPool, realm: &str, personagem: &str, anuncio: &str) -> Result<(bool, String)> {
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
        return Ok((false, "Anúncio não encontrado.".into()));
    };
    if l.get::<i16, _>("estado") as u8 != regras::ESTADO_ATIVO {
        return Ok((false, "O anúncio já não está ativo.".into()));
    }
    let restante = l.get::<i64, _>("qtd_restante").max(0) as u64;
    let nome: String = l.get("nome");
    sqlx::query("UPDATE mercado_anuncios SET estado = $2, qtd_restante = 0, atualizado = NOW() WHERE id = $1")
        .bind(anuncio)
        .bind(regras::ESTADO_CANCELADO as i16)
        .execute(&mut *tx)
        .await?;
    let texto = if l.get::<i16, _>("tipo") as u8 == regras::TIPO_TP {
        let conta: String = l.get("conta");
        razao::mover(&mut tx, &conta, restante as i64, "cancelou anúncio de TP", Some(&format!("{anuncio}:cancelado"))).await?;
        format!("Anúncio cancelado: {restante} TP de volta.")
    } else if restante > 0 {
        let inst: Option<String> = l.get("instancia");
        carta(&mut tx, &format!("{anuncio}:cancelado"), realm, personagem, l.get::<i32, _>("item_id") as u16, restante, inst.as_deref(), 0, &format!("Anúncio cancelado: {nome}")).await?;
        format!("Anúncio cancelado: {nome} ×{restante} volta em Entregas.")
    } else {
        "Anúncio cancelado.".into()
    };
    tx.commit().await?;
    Ok((true, texto))
}

/// Anuncia TP da conta: a TP sai do livro na mesma transacao.
pub async fn anunciar_tp(central: &PgPool, realm: &str, conta: &str, personagem: &str, qtd: u64, preco_unit: u64) -> Result<(bool, String)> {
    if qtd == 0 || qtd > regras::TP_MAX_POR_ANUNCIO {
        return Ok((false, Recusa::Quantidade.texto()));
    }
    if !regras::preco_valido(preco_unit) {
        return Ok((false, Recusa::Preco.texto()));
    }
    let mut tx = central.begin().await?;
    sqlx::query("SELECT pg_advisory_xact_lock(hashtext($1))").bind(format!("anuncios:{realm}:{personagem}")).execute(&mut *tx).await?;
    if ativos_de(&mut tx, realm, personagem).await? >= regras::MAX_ANUNCIOS {
        return Ok((false, Recusa::MuitosAnuncios.texto()));
    }
    let id = novo_id();
    match razao::mover(&mut tx, conta, -(qtd as i64), "anunciou TP no mercado", Some(&format!("{id}:custodia"))).await? {
        razao::Movimento::SemSaldo { .. } => return Ok((false, Recusa::SemTp.texto())),
        razao::Movimento::Feito { .. } | razao::Movimento::JaFeito { .. } => {}
    }
    sqlx::query(
        "INSERT INTO mercado_anuncios (id, tipo, realm, conta, personagem, item_id, nome, categoria, instancia, qtd_total, qtd_restante, preco_unit)
         VALUES ($1, $2, $3, $4, $5, 0, 'Tempest Points', $6, NULL, $7, $7, $8)",
    )
    .bind(&id)
    .bind(regras::TIPO_TP as i16)
    .bind(realm)
    .bind(conta)
    .bind(personagem)
    .bind(regras::Categoria::Tp as i16)
    .bind(qtd as i64)
    .bind(preco_unit as i64)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok((true, format!("Anunciado: {qtd} TP a {preco_unit} gold cada.")))
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
        preco_unit: l.get::<i64, _>("preco_unit").max(0) as u64,
        meu: r == realm && p == personagem,
        realm: r,
        vendedor: p,
    }
}

/// Uma pagina da busca: mais barato primeiro.
pub async fn buscar(central: &PgPool, filtro: &FiltroNet, realm: &str, personagem: &str) -> Result<(Vec<AnuncioNet>, bool)> {
    let texto: String = filtro.texto.chars().filter(|c| !matches!(c, '%' | '_' | '\\')).take(regras::BUSCA_MAX_CHARS).collect();
    let pagina = filtro.pagina.min(500) as i64;
    let linhas = sqlx::query(
        "SELECT id, tipo, realm, personagem, item_id, nome, categoria, instancia, qtd_restante, preco_unit
           FROM mercado_anuncios
          WHERE estado = 0
            AND ($1 = 0 OR categoria = $1)
            AND ($1 = 4 OR categoria <> 4)
            AND ($2 = '' OR nome ILIKE '%' || $2 || '%')
          ORDER BY preco_unit ASC, criado ASC
          LIMIT $3 OFFSET $4",
    )
    .bind(filtro.categoria as i16)
    .bind(texto.trim())
    .bind(regras::POR_PAGINA as i64 + 1)
    .bind(pagina * regras::POR_PAGINA as i64)
    .fetch_all(central)
    .await?;
    let tem_mais = linhas.len() > regras::POR_PAGINA;
    Ok((linhas.iter().take(regras::POR_PAGINA).map(|l| anuncio_de(l, realm, personagem)).collect(), tem_mais))
}

/// Meus anuncios ativos, as ultimas vendas/compras e a TP da conta.
pub async fn meus(central: &PgPool, realm: &str, personagem: &str, conta: &str) -> Result<(Vec<AnuncioNet>, Vec<VendaNet>, u64)> {
    let ativos = sqlx::query(
        "SELECT id, tipo, realm, personagem, item_id, nome, categoria, instancia, qtd_restante, preco_unit
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
    Ok((ativos.iter().map(|l| anuncio_de(l, realm, personagem)).collect(), historico, tp))
}

/// Cartas ainda nao entregues, sem as que o realm ja' aplicou (e so' falta
/// avisar o central).
pub async fn cartas_pendentes(central: &PgPool, realm_pool: &PgPool, realm: &str, personagem: &str) -> Result<Vec<CartaNet>> {
    let linhas = sqlx::query(
        "SELECT id, item_id, qtd, instancia, gold, motivo FROM mercado_cartas
          WHERE realm = $1 AND personagem = $2 AND entregue IS NULL ORDER BY criada LIMIT 100",
    )
    .bind(realm)
    .bind(personagem)
    .fetch_all(central)
    .await?;
    let aplicadas: HashSet<String> = sqlx::query_scalar::<_, String>("SELECT id FROM mercado_cartas_aplicadas WHERE personagem = $1 AND avisada IS NULL")
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
    Ok(regras::cartas_a_aplicar(&cartas, &aplicadas).into_iter().cloned().collect())
}

pub async fn saldo_tp(central: &PgPool, conta: &str) -> Result<u64> {
    razao::saldo(central, conta).await
}

// ─────────────────────────────── relay ───────────────────────────────

/// O que o relay e as consultas devolvem ao loop do mundo.
#[derive(Debug)]
pub enum Evento {
    /// Operacao fechou no central: avisa o personagem, se estiver aqui.
    Aviso { personagem: String, ok: bool, texto: String },
    /// Cartas pra aplicar (pedido "Receber").
    Cartas { sid: SessionId, personagem: String, cartas: Vec<CartaNet> },
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
                tracing::warn!("mercado: envio ao central falhou (tenta de novo): {e:#}");
            }
            if let Err(e) = avisar_entregues(&realm, &central).await {
                tracing::warn!("mercado: aviso de entregas falhou (tenta de novo): {e:#}");
            }
        }
    });
}

pub async fn enviar_saida(realm: &PgPool, central: &PgPool, tx: &mpsc::UnboundedSender<IncomingMessage>) -> Result<usize> {
    let pendentes: Vec<(String, String)> =
        sqlx::query_as("SELECT id, payload FROM mercado_saida WHERE enviada IS NULL ORDER BY criada LIMIT 50")
            .fetch_all(realm)
            .await?;
    let mut feitas = 0;
    for (id, payload) in pendentes {
        let op: OpCentral = match serde_json::from_str(&payload) {
            Ok(op) => op,
            Err(e) => {
                // Nunca marca como enviada: e' item/gold de alguem. Fica pra
                // correcao a' mao.
                tracing::error!("mercado: payload ilegivel em mercado_saida {id}: {e}");
                continue;
            }
        };
        let r = processar(central, &op).await?;
        sqlx::query("UPDATE mercado_saida SET enviada = NOW(), resultado = $2 WHERE id = $1")
            .bind(&id)
            .bind(&r.texto)
            .execute(realm)
            .await?;
        if r.novo {
            let _ = tx.send(IncomingMessage::Mercado(Evento::Aviso { personagem: op.personagem().to_string(), ok: r.ok, texto: r.texto }));
        }
        feitas += 1;
    }
    Ok(feitas)
}

pub async fn avisar_entregues(realm: &PgPool, central: &PgPool) -> Result<usize> {
    let ids: Vec<String> = sqlx::query_scalar("SELECT id FROM mercado_cartas_aplicadas WHERE avisada IS NULL LIMIT 500")
        .fetch_all(realm)
        .await?;
    if ids.is_empty() {
        return Ok(0);
    }
    sqlx::query("UPDATE mercado_cartas SET entregue = NOW() WHERE id = ANY($1) AND entregue IS NULL")
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

    /// Fluxo inteiro num Postgres descartavel: `DATABASE_URL_CENTRAL_TESTE`.
    /// Sem a variavel o teste nao faz nada (e diz). As tabelas do realm
    /// entram no mesmo banco.
    #[tokio::test]
    async fn custodia_cartas_e_tp_no_postgres() {
        let Ok(url) = std::env::var("DATABASE_URL_CENTRAL_TESTE") else {
            eprintln!("DATABASE_URL_CENTRAL_TESTE nao setada: teste de integracao do mercado pulado");
            return;
        };
        let central = abrir_central(&url).await.expect("central");
        criar_tabelas_do_realm(&central).await.expect("tabelas do realm");
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
            preco_unit: 7,
        };
        let mut t = realm_pool.begin().await.unwrap();
        gravar_registros(&mut t, &[Registro::Saida(op.clone())]).await.unwrap();
        gravar_registros(&mut t, &[Registro::Saida(op.clone())]).await.unwrap();
        t.commit().await.unwrap();
        assert!(enviar_saida(&realm_pool, &central, &tx).await.unwrap() >= 1);
        assert!(matches!(rx.try_recv(), Ok(IncomingMessage::Mercado(Evento::Aviso { ok: true, .. }))));
        let de_novo = processar(&central, &op).await.unwrap();
        assert!(de_novo.ok && !de_novo.novo, "a mesma operacao nao anuncia duas vezes");

        let filtro = FiltroNet { categoria: 0, texto: tag.to_string(), pagina: 0 };
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
        assert!(!processar(&central, &compra).await.unwrap().novo, "compra repetida nao compra de novo");
        let c = cartas_pendentes(&central, &realm_pool, "T", &comp).await.unwrap();
        assert_eq!(c.len(), 1);
        assert_eq!((c[0].item_id, c[0].qtd, c[0].gold), (60, 4, 0));
        let v = cartas_pendentes(&central, &realm_pool, "T", &vend).await.unwrap();
        assert_eq!(v.iter().map(|c| c.gold).sum::<u64>(), 26);

        // Carta aplicada no realm some das pendentes; o aviso marca entregue.
        let mut t = realm_pool.begin().await.unwrap();
        let aplicada = Registro::CartaAplicada { id: c[0].id.clone(), personagem: comp.clone() };
        gravar_registros(&mut t, &[aplicada.clone(), aplicada]).await.unwrap();
        t.commit().await.unwrap();
        assert!(cartas_pendentes(&central, &realm_pool, "T", &comp).await.unwrap().is_empty());
        avisar_entregues(&realm_pool, &central).await.unwrap();
        assert!(cartas_pendentes(&central, &realm_pool, "T", &comp).await.unwrap().is_empty());

        // Comprar mais do que sobrou: nada fecha, gold volta.
        let demais = OpCentral::Comprar { id: novo_id(), realm: "T".into(), conta: conta_c.clone(), personagem: comp.clone(), anuncio: anuncio.clone(), qtd: 7, preco_unit: 7, pago: 49 };
        assert!(!processar(&central, &demais).await.unwrap().ok);
        let reemb = cartas_pendentes(&central, &realm_pool, "T", &comp).await.unwrap();
        assert_eq!(reemb.iter().map(|c| c.gold).sum::<u64>(), 49);

        // Cancela: as 6 que sobraram voltam; cancelar de novo nao devolve mais.
        assert!(cancelar(&central, "T", &vend, &anuncio).await.unwrap().0);
        assert!(!cancelar(&central, "T", &vend, &anuncio).await.unwrap().0);
        let volta = cartas_pendentes(&central, &realm_pool, "T", &vend).await.unwrap();
        assert_eq!(volta.iter().filter(|c| c.item_id == 60).map(|c| c.qtd).sum::<u64>(), 6);

        // TP: credita 100, anuncia 30 (sai do livro), compra 10, cancela o resto.
        assert_eq!(razao::creditar(&central, &conta_v, 100, "teste", Some(&format!("{tag}:credito"))).await.unwrap(), razao::Movimento::Feito { saldo: 100 });
        assert!(matches!(razao::creditar(&central, &conta_v, 100, "teste", Some(&format!("{tag}:credito"))).await.unwrap(), razao::Movimento::JaFeito { saldo: 100 }));
        assert!(!anunciar_tp(&central, "T", &conta_v, &vend, 101, 5).await.unwrap().0, "sem TP bastante");
        assert!(anunciar_tp(&central, "T", &conta_v, &vend, 30, 5).await.unwrap().0);
        assert_eq!(saldo_tp(&central, &conta_v).await.unwrap(), 70);
        let tp = FiltroNet { categoria: regras::Categoria::Tp as u8, texto: String::new(), pagina: 0 };
        let (anuncios_tp, _) = buscar(&central, &tp, "T", &comp).await.unwrap();
        let meu_tp = anuncios_tp.iter().find(|a| a.vendedor == vend).expect("anuncio de TP");
        let compra_tp = OpCentral::Comprar { id: novo_id(), realm: "T".into(), conta: conta_c.clone(), personagem: comp.clone(), anuncio: meu_tp.id.clone(), qtd: 10, preco_unit: 5, pago: 50 };
        assert!(processar(&central, &compra_tp).await.unwrap().ok);
        assert_eq!(saldo_tp(&central, &conta_c).await.unwrap(), 10);
        assert!(cancelar(&central, "T", &vend, &meu_tp.id).await.unwrap().0);
        assert_eq!(saldo_tp(&central, &conta_v).await.unwrap(), 90, "as 20 que sobraram voltam");
        let (_, historico, _) = meus(&central, "T", &vend, &conta_v).await.unwrap();
        assert_eq!(historico.iter().filter(|h| h.vendi).map(|h| h.liquido).sum::<u64>(), 26 + 47);
    }
}
