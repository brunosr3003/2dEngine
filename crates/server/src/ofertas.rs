//! FLASH OFFERS: a premium bundle that pops up when a character reaches a
//! level, open for a short time (owner, 08/11/2026: "reached level 5, a flash
//! offer shows on screen, available for 1 hour, R$ 0,99 or US$ 0.99 … as you
//! level up they grow … configurable in the DB so I can release them without
//! touching the code").
//!
//! Everything lives in the CENTRAL database (same as the cash shop):
//!
//! - `ofertas_relampago` — the catalogue, edited by hand (see
//!   docs/OFERTAS_RELAMPAGO.md). One row per offer: trigger level, duration,
//!   Google Play product, price shown, TP, energy, items, on/off.
//! - `ofertas_relampago_contas` — which offer each ACCOUNT got, until when,
//!   and whether it bought it. Per account, not per character: an alt
//!   reaching level 5 doesn't reopen the R$ 0,99 deal.
//!
//! Money: only Google Play today. The purchase is checked on the Android
//! Publisher API, credited once (unique Google order id), and CONSUMED on
//! Google afterwards so the same price product can be bought again by the
//! next offer.

use anyhow::Result;
use serde::Deserialize;
use sqlx::{PgPool, Row};

use crate::mercado_razao as razao;
use shared::loja::OfertaRelampago;

pub async fn criar_tabelas(central: &PgPool) -> Result<()> {
    for sql in [
        "CREATE TABLE IF NOT EXISTS ofertas_relampago (
            id                TEXT PRIMARY KEY,
            titulo            TEXT    NOT NULL,
            titulo_pt         TEXT,
            nivel             INTEGER NOT NULL,
            duracao_min       INTEGER NOT NULL DEFAULT 60,
            produto_play      TEXT    NOT NULL,
            preco_centavos    INTEGER NOT NULL,
            valor_de_centavos INTEGER,
            tp                INTEGER NOT NULL DEFAULT 0,
            energia           BIGINT  NOT NULL DEFAULT 0,
            itens             TEXT    NOT NULL DEFAULT '[]',
            ativa             BOOLEAN NOT NULL DEFAULT TRUE,
            nota              TEXT
        )",
        // What a paid purchase still owes the player. Written in the SAME
        // transaction that records the purchase, so a payment can never end
        // up without its delivery: the world claims the rows and posts them
        // to the character's mail (no expiry), never straight into a bag that
        // may be full (owner, 09/11/2026: "if the player buys anything he can
        // never be without receiving … in the mail … and no expire date").
        "CREATE TABLE IF NOT EXISTS loja_entregas (
            id          BIGSERIAL PRIMARY KEY,
            conta       TEXT        NOT NULL,
            pedido      TEXT        NOT NULL,
            item_id     INTEGER     NOT NULL DEFAULT 0,
            qtd         BIGINT      NOT NULL DEFAULT 0,
            energia     BIGINT      NOT NULL DEFAULT 0,
            criado_em   TIMESTAMPTZ NOT NULL DEFAULT NOW(),
            entregue_em TIMESTAMPTZ,
            personagem  TEXT
        )",
        "CREATE INDEX IF NOT EXISTS loja_entregas_pendentes ON loja_entregas (conta) WHERE entregue_em IS NULL",
        "CREATE TABLE IF NOT EXISTS ofertas_relampago_contas (
            conta       TEXT        NOT NULL,
            oferta      TEXT        NOT NULL REFERENCES ofertas_relampago(id),
            aberta_em   TIMESTAMPTZ NOT NULL DEFAULT NOW(),
            expira_em   TIMESTAMPTZ NOT NULL,
            comprada_em TIMESTAMPTZ,
            pedido      TEXT,
            PRIMARY KEY (conta, oferta)
        )",
    ] {
        sqlx::query(sql).execute(central).await?;
    }
    // CAMPAIGN offers (Panopticon › ofertas): open to EVERY account until
    // `campanha_ate`, whatever the level, alongside the level offer (owner,
    // 10/11/2026: "create a flash offer whenever I want and send it now to
    // everyone"). NULL = an ordinary level offer.
    sqlx::query("ALTER TABLE ofertas_relampago ADD COLUMN IF NOT EXISTS campanha_ate TIMESTAMPTZ")
        .execute(central)
        .await?;
    // BONUS PACKS (`shared::pacotes_bonus`, 15/11/2026): a TRIGGER instead
    // of a level (`gatilho`: porao_inicial / morte; NULL = level/campaign
    // offer as before), gold, and gear BUILT for the player when it opens
    // (`pecas`: arma / conjunto). The built pieces and the power shown are
    // kept on the account's row, so the purchase delivers what was shown.
    for sql in [
        "ALTER TABLE ofertas_relampago ADD COLUMN IF NOT EXISTS gatilho TEXT",
        "ALTER TABLE ofertas_relampago ADD COLUMN IF NOT EXISTS ouro BIGINT NOT NULL DEFAULT 0",
        "ALTER TABLE ofertas_relampago ADD COLUMN IF NOT EXISTS pecas TEXT NOT NULL DEFAULT ''",
        "ALTER TABLE ofertas_relampago_contas ADD COLUMN IF NOT EXISTS pecas TEXT NOT NULL DEFAULT '[]'",
        "ALTER TABLE ofertas_relampago_contas ADD COLUMN IF NOT EXISTS poder_antes INTEGER",
        "ALTER TABLE ofertas_relampago_contas ADD COLUMN IF NOT EXISTS poder_depois INTEGER",
        "ALTER TABLE ofertas_relampago_contas ADD COLUMN IF NOT EXISTS personagem TEXT",
        "ALTER TABLE loja_entregas ADD COLUMN IF NOT EXISTS ouro BIGINT NOT NULL DEFAULT 0",
        // > 0: `item_id` is a piece of gear built at this item level.
        "ALTER TABLE loja_entregas ADD COLUMN IF NOT EXISTS ilvl INTEGER NOT NULL DEFAULT 0",
    ] {
        sqlx::query(sql).execute(central).await?;
    }
    for p in shared::pacotes_bonus::PACOTES_BONUS {
        sqlx::query(
            "INSERT INTO ofertas_relampago
               (id, titulo, titulo_pt, nivel, duracao_min, produto_play, preco_centavos, valor_de_centavos,
                tp, energia, itens, gatilho, ouro, pecas, nota)
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15)
             ON CONFLICT (id) DO NOTHING",
        )
        .bind(p.id)
        .bind(p.titulo)
        .bind(p.titulo_pt)
        .bind(p.nivel() as i32)
        .bind(if p.gatilho.is_some() { shared::pacotes_bonus::DURACAO_MIN } else { 60 })
        .bind(p.produto_play)
        .bind(p.preco_centavos as i32)
        .bind(p.valor_de_centavos as i32)
        .bind(p.tp() as i32)
        .bind(p.energia as i64)
        .bind(p.itens_json())
        .bind(p.gatilho.map(|g| g.chave()))
        .bind(p.ouro as i64)
        .bind(p.pecas.chave())
        .bind(format!("pacote bonus: {} TP do preco (min 400) + {} TP de bonus", shared::pacotes_bonus::tp_base_da_oferta(p.preco_centavos), p.tp_bonus))
        .execute(central)
        .await?;
    }
    // Starting catalogue. ON CONFLICT DO NOTHING: once the row exists the
    // database is the truth — a deploy never overwrites what the owner edited.
    for &(id, titulo, titulo_pt, nivel, produto, preco, valor_de, energia, itens) in
        OFERTAS_DE_NIVEL
    {
        let tp = shared::pacotes_bonus::tp_base_da_oferta(preco as u32) as i32;
        sqlx::query(
            "INSERT INTO ofertas_relampago
               (id, titulo, titulo_pt, nivel, produto_play, preco_centavos, valor_de_centavos, tp, energia, itens)
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10)
             ON CONFLICT (id) DO NOTHING",
        )
        .bind(id)
        .bind(titulo)
        .bind(titulo_pt)
        .bind(nivel)
        .bind(produto)
        .bind(preco)
        .bind(valor_de)
        .bind(tp)
        .bind(energia)
        .bind(itens)
        .execute(central)
        .await?;
    }
    // EVERY offer gives at least the TP its price buys, never under 400
    // (`pacotes_bonus::tp_base_da_oferta`, owner 15/11/2026). Rows seeded
    // before the rule (or edited by hand) are lifted to it; a row above it
    // keeps its own number. Listing and crediting apply the same floor.
    let linhas: Vec<(String, i32, i32)> =
        sqlx::query_as("SELECT id, preco_centavos, tp FROM ofertas_relampago")
            .fetch_all(central)
            .await?;
    for (id, preco, tp) in linhas {
        let piso = shared::pacotes_bonus::tp_base_da_oferta(preco.max(0) as u32)
            .min(i32::MAX as u64) as i32;
        if tp < piso {
            sqlx::query("UPDATE ofertas_relampago SET tp = $2 WHERE id = $1 AND tp < $2")
                .bind(&id)
                .bind(piso)
                .execute(central)
                .await?;
            tracing::info!(
                "ofertas: {id} sobe de {tp} pra {piso} TP (regra: TP do preco, minimo 400)"
            );
        }
    }
    Ok(())
}

/// The starting LEVEL offers: (id, title, title pt, level, Play product,
/// price, crossed-out price, Energy, items). Their TP is THE RULE
/// (`pacotes_bonus::tp_base_da_oferta`), not a column here. Item ids: 363 pet
/// summon scroll, 361 mount summon scroll, 360 craft key summon scroll, 345
/// darksteel.
#[allow(clippy::type_complexity)]
const OFERTAS_DE_NIVEL: &[(&str, &str, &str, i32, &str, i32, Option<i32>, i64, &str)] = &[
    (
        "nivel_5",
        "Beginner's Flash Offer",
        "Oferta Relâmpago de Iniciante",
        5,
        "oferta_099",
        99,
        Some(990),
        0,
        r#"[{"item_id":363,"qtd":1},{"item_id":361,"qtd":1}]"#,
    ),
    (
        "nivel_15",
        "Adventurer's Flash Offer",
        "Oferta Relâmpago de Aventureiro",
        15,
        "oferta_499",
        499,
        Some(2990),
        6_000,
        r#"[{"item_id":363,"qtd":1},{"item_id":361,"qtd":1},{"item_id":360,"qtd":2},{"item_id":345,"qtd":2000}]"#,
    ),
    (
        "nivel_25",
        "Champion's Flash Offer",
        "Oferta Relâmpago de Campeão",
        25,
        "oferta_1500",
        1500,
        Some(8990),
        24_000,
        r#"[{"item_id":363,"qtd":2},{"item_id":361,"qtd":2},{"item_id":360,"qtd":5},{"item_id":345,"qtd":6000}]"#,
    ),
];

/// The TP a row gives, as shown and as credited: the rule's floor applies
/// even to a row edited below it after startup.
fn tp_da_linha(preco: i32, tp: i32) -> u64 {
    shared::pacotes_bonus::tp_da_oferta(preco.max(0) as u32, tp.max(0) as u64)
}

#[derive(Deserialize)]
struct ItemJson {
    item_id: u16,
    qtd: u32,
}

/// `[{"item_id":345,"qtd":2000}, …]` — a typo in the DB drops the bad entry
/// (and logs) instead of breaking the whole offer list.
fn itens_de(json: &str, oferta: &str) -> Vec<(u16, u32)> {
    match serde_json::from_str::<Vec<ItemJson>>(json) {
        Ok(v) => v
            .into_iter()
            .filter(|i| i.qtd > 0)
            .map(|i| (i.item_id, i.qtd))
            .collect(),
        Err(e) => {
            tracing::warn!("ofertas: itens invalidos na oferta {oferta}: {e} ({json})");
            Vec::new()
        }
    }
}

/// Opens the next offer this level unlocks (at most ONE per call, the lowest
/// level first, and only when none is open right now) and returns every offer
/// still open for the account.
///
/// `abrir` false (staff, test bots): only lists, never opens anything new.
/// A triggered bonus pack open right now does not hold the level offer back
/// (only another level offer does).
pub async fn abrir_e_listar(
    central: &PgPool,
    conta: &str,
    nivel: u32,
    abrir: bool,
) -> Result<Vec<OfertaRelampago>> {
    let mut nova: Option<String> = None;
    let abertas: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM ofertas_relampago_contas c
           JOIN ofertas_relampago o ON o.id = c.oferta
          WHERE c.conta = $1 AND c.comprada_em IS NULL AND c.expira_em > NOW()
            AND o.gatilho IS NULL AND o.campanha_ate IS NULL",
    )
    .bind(conta)
    .fetch_one(central)
    .await?;
    if !abrir {
        return listar(central, conta, None).await;
    }
    if abertas == 0 {
        nova = sqlx::query_scalar(
            "INSERT INTO ofertas_relampago_contas (conta, oferta, expira_em)
             SELECT $1, o.id, NOW() + make_interval(mins => GREATEST(o.duracao_min, 1))
               FROM ofertas_relampago o
              WHERE o.ativa AND o.nivel <= $2 AND o.campanha_ate IS NULL AND o.gatilho IS NULL
                AND NOT EXISTS (SELECT 1 FROM ofertas_relampago_contas c
                                 WHERE c.conta = $1 AND c.oferta = o.id)
              ORDER BY o.nivel, o.id
              LIMIT 1
             ON CONFLICT DO NOTHING
             RETURNING oferta",
        )
        .bind(conta)
        .bind(nivel as i32)
        .fetch_optional(central)
        .await?;
    }
    // Every running campaign the account has not seen yet: open now, until
    // the campaign ends (not one at a time — they are the staff's call).
    let campanhas: Vec<String> = sqlx::query_scalar(
        "INSERT INTO ofertas_relampago_contas (conta, oferta, expira_em)
         SELECT $1, o.id, o.campanha_ate
           FROM ofertas_relampago o
          WHERE o.ativa AND o.campanha_ate > NOW()
            AND NOT EXISTS (SELECT 1 FROM ofertas_relampago_contas c
                             WHERE c.conta = $1 AND c.oferta = o.id)
         ON CONFLICT DO NOTHING
         RETURNING oferta",
    )
    .bind(conta)
    .fetch_all(central)
    .await?;
    if nova.is_none() {
        nova = campanhas.into_iter().next();
    }
    listar(central, conta, nova.as_deref()).await
}

/// Every offer still open for the account; `nova` is flagged to pop.
pub async fn listar(
    central: &PgPool,
    conta: &str,
    nova: Option<&str>,
) -> Result<Vec<OfertaRelampago>> {
    let linhas = sqlx::query(
        "SELECT o.id, o.titulo, o.titulo_pt, o.produto_play, o.preco_centavos,
                o.valor_de_centavos, o.tp, o.energia, o.itens,
                GREATEST(EXTRACT(EPOCH FROM (c.expira_em - NOW())), 0)::BIGINT,
                o.ouro, c.pecas, c.poder_antes, c.poder_depois, COALESCE(o.gatilho, '')
           FROM ofertas_relampago_contas c
           JOIN ofertas_relampago o ON o.id = c.oferta
          WHERE c.conta = $1 AND c.comprada_em IS NULL AND c.expira_em > NOW()
          ORDER BY c.expira_em",
    )
    .bind(conta)
    .fetch_all(central)
    .await?;
    Ok(linhas
        .into_iter()
        .map(|r| {
            let id: String = r.get(0);
            let itens: String = r.get(8);
            let pecas: String = r.get(11);
            let poder = match (r.get::<Option<i32>, _>(12), r.get::<Option<i32>, _>(13)) {
                (Some(a), Some(d)) => Some((a, d)),
                _ => None,
            };
            OfertaRelampago {
                nova: nova == Some(id.as_str()),
                itens: itens_de(&itens, &id),
                titulo: r.get(1),
                titulo_pt: r.get(2),
                produto_play: r.get(3),
                preco_centavos: r.get::<i32, _>(4).max(0) as u32,
                valor_de_centavos: r.get::<Option<i32>, _>(5).map(|v| v.max(0) as u32),
                tp: tp_da_linha(r.get::<i32, _>(4), r.get::<i32, _>(6)).min(u32::MAX as u64) as u32,
                energia: r.get::<i64, _>(7).max(0) as u64,
                segundos: r.get::<i64, _>(9).clamp(0, u32::MAX as i64) as u32,
                ouro: r.get::<i64, _>(10).max(0) as u64,
                pecas: pecas_de(&pecas, &id)
                    .into_iter()
                    .filter_map(|(item, ilvl)| instancia_da_peca(item, ilvl).map(|i| (item, i)))
                    .collect(),
                poder,
                gatilho: r.get(14),
                id,
            }
        })
        .collect())
}

/// `[[item_id, item_level], …]`: the gear a bonus pack built when it opened.
fn pecas_de(json: &str, oferta: &str) -> Vec<(u16, u16)> {
    if json.trim().is_empty() {
        return Vec::new();
    }
    match serde_json::from_str::<Vec<(u16, u16)>>(json) {
        Ok(v) => v.into_iter().filter(|(id, l)| *id != 0 && *l > 0).collect(),
        Err(e) => {
            tracing::warn!("ofertas: pecas invalidas na oferta {oferta}: {e} ({json})");
            Vec::new()
        }
    }
}

/// The piece itself. Gear stats are FIXED by template, grade and item level
/// (`ItemInstance::fixar`), so the piece rebuilt at delivery is the one shown.
pub fn instancia_da_peca(item_id: u16, ilvl: u16) -> Option<shared::ItemInstance> {
    let tpl = crate::economy::item_template_of(item_id);
    shared::ItemInstance::roll_with_template(tpl, ilvl, || 0.5)
}

/// One gear option the world built for a trigger: which kind, the pieces
/// `(item_id, item level)`, and the power now / with them on.
#[derive(Debug, Clone)]
pub struct Montagem {
    pub pecas: shared::pacotes_bonus::Pecas,
    pub itens: Vec<(u16, u16)>,
    pub poder_antes: i32,
    pub poder_depois: i32,
}

/// TRIGGERED BONUS PACKS: opens the first pack of `gatilhos` (in that order,
/// then by level) the account never got, that its level reaches and — for a
/// gear pack — that really makes the character stronger. At most one
/// triggered pack open at a time, and never two opened closer than
/// `INTERVALO_ENTRE_GATILHOS_MIN`. Returns the id opened, if any.
pub async fn disparar(
    central: &PgPool,
    conta: &str,
    personagem: &str,
    gatilhos: &[shared::pacotes_bonus::Gatilho],
    nivel: u32,
    montagens: &[Montagem],
) -> Result<Option<String>> {
    use shared::pacotes_bonus as pb;
    let mut tx = central.begin().await?;
    // One trigger at a time per account (a death and a leave in the same
    // second must not open two).
    sqlx::query("SELECT pg_advisory_xact_lock(hashtext('ofertas_gatilho:' || $1))")
        .bind(conta)
        .execute(&mut *tx)
        .await?;
    let ocupado: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM ofertas_relampago_contas c
                          JOIN ofertas_relampago o ON o.id = c.oferta
                         WHERE c.conta = $1 AND o.gatilho IS NOT NULL
                           AND ((c.comprada_em IS NULL AND c.expira_em > NOW())
                                OR c.aberta_em > NOW() - make_interval(mins => $2)))",
    )
    .bind(conta)
    .bind(pb::INTERVALO_ENTRE_GATILHOS_MIN)
    .fetch_one(&mut *tx)
    .await?;
    if ocupado {
        return Ok(None);
    }
    let chaves: Vec<String> = gatilhos.iter().map(|g| g.chave().to_string()).collect();
    let candidatas: Vec<(String, String)> = sqlx::query_as(
        "SELECT o.id, o.pecas FROM ofertas_relampago o
          WHERE o.ativa AND o.gatilho = ANY($2) AND o.nivel <= $3
            AND NOT EXISTS (SELECT 1 FROM ofertas_relampago_contas c
                             WHERE c.conta = $1 AND c.oferta = o.id)
          ORDER BY array_position($2, o.gatilho), o.nivel, o.id",
    )
    .bind(conta)
    .bind(&chaves)
    .bind(nivel as i32)
    .fetch_all(&mut *tx)
    .await?;
    for (id, pecas) in candidatas {
        let tipo = pb::Pecas::de_chave(&pecas);
        let (json, antes, depois) = if tipo == pb::Pecas::Nenhuma {
            ("[]".to_string(), None, None)
        } else {
            // A gear pack that would not raise the power is not an offer.
            let Some(m) = montagens
                .iter()
                .find(|m| m.pecas == tipo && m.poder_depois > m.poder_antes)
            else {
                continue;
            };
            (
                serde_json::to_string(&m.itens)?,
                Some(m.poder_antes),
                Some(m.poder_depois),
            )
        };
        let aberta: Option<String> = sqlx::query_scalar(
            "INSERT INTO ofertas_relampago_contas (conta, oferta, expira_em, pecas, poder_antes, poder_depois, personagem)
             SELECT $1, o.id, NOW() + make_interval(mins => GREATEST(o.duracao_min, 1)), $3, $4, $5, $6
               FROM ofertas_relampago o WHERE o.id = $2
             ON CONFLICT DO NOTHING
             RETURNING oferta",
        )
        .bind(conta)
        .bind(&id)
        .bind(&json)
        .bind(antes)
        .bind(depois)
        .bind(personagem)
        .fetch_optional(&mut *tx)
        .await?;
        if aberta.is_some() {
            tx.commit().await?;
            return Ok(aberta);
        }
    }
    Ok(None)
}

/// What a paid offer hands over in the world (the TP is already in the
/// ledger when this comes back).
#[derive(Debug, Default)]
pub struct Entrega {
    /// Price charged (cents, same number in BRL/USD) and whether Google said
    /// it was a TEST purchase (license tester) — for the revenue telemetry.
    pub preco_centavos: u64,
    pub teste: bool,
    pub tp: u64,
    pub energia: u64,
    pub itens: Vec<(u16, u32)>,
    pub titulo: String,
    pub ouro: u64,
    /// Pieces of gear (bonus packs).
    pub pecas: u32,
}

pub enum Compra {
    Feita(Entrega, u64),
    JaFeita(u64),
    Recusada(String),
}

/// Buys `oferta` for `conta` with a Google Play `purchase_token`.
///
/// A PAID purchase is always honoured: no expiry check (the payment may be
/// approved hours later — PIX/boleto — or recovered at the next login), and
/// when the client no longer knows which offer it was (`oferta` empty) the
/// account's unbought offer on that product is used.
pub async fn comprar(
    central: &PgPool,
    conta: &str,
    oferta: &str,
    produto: &str,
    pedido: &str,
    purchase_token: &str,
) -> Result<Compra> {
    if !shared::loja::pedido_valido(pedido)
        || purchase_token.is_empty()
        || purchase_token.len() > 4096
    {
        return Ok(Compra::Recusada("Invalid order.".into()));
    }
    let achada: Option<String> = if oferta.is_empty() {
        sqlx::query_scalar(
            "SELECT c.oferta FROM ofertas_relampago_contas c
               JOIN ofertas_relampago o ON o.id = c.oferta
              WHERE c.conta = $1 AND o.produto_play = $2
                AND (c.comprada_em IS NULL OR c.pedido = $3)
              ORDER BY (c.pedido = $3) DESC NULLS LAST, (c.expira_em > NOW()) DESC, c.aberta_em DESC
              LIMIT 1",
        )
        .bind(conta)
        .bind(produto)
        .bind(pedido)
        .fetch_optional(central)
        .await?
    } else {
        Some(oferta.to_string())
    };
    let Some(oferta) = achada else {
        tracing::error!("ofertas: compra paga sem oferta da conta ({conta} {produto} {pedido}) — conferir na Play Console");
        return Ok(Compra::Recusada("This offer is not available.".into()));
    };
    let oferta = oferta.as_str();
    let Some(o) = sqlx::query(
        "SELECT o.produto_play, o.preco_centavos, o.tp, o.energia, o.itens, o.titulo,
                c.comprada_em IS NOT NULL, c.pedido
           FROM ofertas_relampago_contas c
           JOIN ofertas_relampago o ON o.id = c.oferta
          WHERE c.conta = $1 AND c.oferta = $2",
    )
    .bind(conta)
    .bind(oferta)
    .fetch_optional(central)
    .await?
    else {
        return Ok(Compra::Recusada("This offer is not available.".into()));
    };
    let produto: String = o.get(0);
    let comprada: bool = o.get(6);
    let pedido_feito: Option<String> = o.get(7);
    if comprada {
        // Re-sent by the client after a lost reply: same order → already done
        // (and it still needs consuming on Google).
        if pedido_feito.as_deref() == Some(pedido) {
            crate::loja_google_play::consumir_best_effort(&produto, purchase_token)
                .await
                .ok();
            return Ok(Compra::JaFeita(razao::saldo(central, conta).await?));
        }
        return Ok(Compra::Recusada("You already bought this offer.".into()));
    }
    let externo = match crate::loja_google_play::verificar_produto(&produto, purchase_token).await {
        Ok(x) => x,
        Err(e) => {
            tracing::warn!("ofertas: compra recusada ({conta} {oferta}): {e}");
            return Ok(Compra::Recusada(e.to_string()));
        }
    };
    let r = creditar(central, conta, oferta, pedido, &externo, "google_play").await?;
    if let Compra::Feita(..) = &r {
        if let Err(e) =
            crate::loja_google_play::consumir_best_effort(&produto, purchase_token).await
        {
            tracing::warn!("ofertas: consume no Google falhou (ja' entregue): {e}");
        }
    }
    Ok(r)
}

/// Buys an OPEN offer on a server with simulated payments (test server):
/// approved at once, nobody charged. The caller checks the provider.
pub async fn comprar_simulado(
    central: &PgPool,
    conta: &str,
    oferta: &str,
    pedido: &str,
) -> Result<Compra> {
    if !shared::loja::pedido_valido(pedido) {
        return Ok(Compra::Recusada("Invalid order.".into()));
    }
    let Some((aberta, comprada, pedido_feito)) = sqlx::query_as::<_, (bool, bool, Option<String>)>(
        "SELECT c.expira_em > NOW(), c.comprada_em IS NOT NULL, c.pedido
           FROM ofertas_relampago_contas c WHERE c.conta = $1 AND c.oferta = $2",
    )
    .bind(conta)
    .bind(oferta)
    .fetch_optional(central)
    .await?
    else {
        return Ok(Compra::Recusada("This offer is not available.".into()));
    };
    if comprada {
        if pedido_feito.as_deref() == Some(pedido) {
            return Ok(Compra::JaFeita(razao::saldo(central, conta).await?));
        }
        return Ok(Compra::Recusada("You already bought this offer.".into()));
    }
    if !aberta {
        return Ok(Compra::Recusada("This offer has expired.".into()));
    }
    creditar(
        central,
        conta,
        oferta,
        pedido,
        &format!("sim-{pedido}"),
        "simulado",
    )
    .await
}

/// Records a paid offer and everything it owes, in ONE transaction: the
/// order, the account row marked bought, the deliveries (items, Energy, gold,
/// gear) and the TP. `externo` is the provider's unique order id.
async fn creditar(
    central: &PgPool,
    conta: &str,
    oferta: &str,
    pedido: &str,
    externo: &str,
    provedor: &str,
) -> Result<Compra> {
    let Some(o) = sqlx::query(
        "SELECT o.preco_centavos, o.tp, o.energia, o.itens, o.titulo, o.ouro, c.pecas
           FROM ofertas_relampago_contas c
           JOIN ofertas_relampago o ON o.id = c.oferta
          WHERE c.conta = $1 AND c.oferta = $2",
    )
    .bind(conta)
    .bind(oferta)
    .fetch_optional(central)
    .await?
    else {
        return Ok(Compra::Recusada("This offer is not available.".into()));
    };
    let preco: i32 = o.get(0);
    // The same TP the card showed: the row's, never under the rule.
    let tp = tp_da_linha(preco, o.get::<i32, _>(1));
    let energia = o.get::<i64, _>(2).max(0) as u64;
    let itens = itens_de(&o.get::<String, _>(3), oferta);
    let titulo: String = o.get(4);
    let ouro = o.get::<i64, _>(5).max(0) as u64;
    let pecas = pecas_de(&o.get::<String, _>(6), oferta);
    let mut tx = central.begin().await?;
    let repetido: Option<String> = sqlx::query_scalar(
        "SELECT id FROM loja_pedidos WHERE provedor = $2 AND externo = $1 AND status = 'creditado'",
    )
    .bind(externo)
    .bind(provedor)
    .fetch_optional(&mut *tx)
    .await?;
    if repetido.is_some() {
        return Ok(Compra::Recusada(
            "This purchase has already been used.".into(),
        ));
    }
    let marcou = sqlx::query(
        "UPDATE ofertas_relampago_contas SET comprada_em = NOW(), pedido = $3
          WHERE conta = $1 AND oferta = $2 AND comprada_em IS NULL",
    )
    .bind(conta)
    .bind(oferta)
    .bind(pedido)
    .execute(&mut *tx)
    .await?
    .rows_affected();
    if marcou != 1 {
        return Ok(Compra::Recusada("You already bought this offer.".into()));
    }
    sqlx::query(
        "INSERT INTO loja_pedidos (id, conta, produto, tipo, valor, moeda, status, provedor, externo)
         VALUES ($1, $2, $3, 'oferta', $4, 'BRL_CENTAVOS', 'creditado', $6, $5)",
    )
    .bind(pedido)
    .bind(conta)
    .bind(format!("oferta:{oferta}"))
    .bind(preco as i64)
    .bind(externo)
    .bind(provedor)
    .execute(&mut *tx)
    .await?;
    // What it owes, in the same transaction: delivered to the mail by the
    // world (`world/loja_mundo.rs`, `Evento::Entregas`).
    for (item_id, qtd) in &itens {
        sqlx::query(
            "INSERT INTO loja_entregas (conta, pedido, item_id, qtd) VALUES ($1, $2, $3, $4)",
        )
        .bind(conta)
        .bind(pedido)
        .bind(*item_id as i32)
        .bind(*qtd as i64)
        .execute(&mut *tx)
        .await?;
    }
    if energia > 0 {
        sqlx::query("INSERT INTO loja_entregas (conta, pedido, energia) VALUES ($1, $2, $3)")
            .bind(conta)
            .bind(pedido)
            .bind(energia as i64)
            .execute(&mut *tx)
            .await?;
    }
    if ouro > 0 {
        sqlx::query("INSERT INTO loja_entregas (conta, pedido, ouro) VALUES ($1, $2, $3)")
            .bind(conta)
            .bind(pedido)
            .bind(ouro as i64)
            .execute(&mut *tx)
            .await?;
    }
    for (item_id, ilvl) in &pecas {
        sqlx::query("INSERT INTO loja_entregas (conta, pedido, item_id, qtd, ilvl) VALUES ($1, $2, $3, 1, $4)")
            .bind(conta)
            .bind(pedido)
            .bind(*item_id as i32)
            .bind(*ilvl as i32)
            .execute(&mut *tx)
            .await?;
    }
    // The TP goes in exactly like a TP-pack purchase (`loja::confirmar_pagamento`):
    // `razao::mover` with the order's key `loja:<pedido>`, so a re-sent
    // order can never credit twice. Same path for Google Play and simulated.
    let saldo = if tp > 0 {
        match razao::mover(
            &mut tx,
            conta,
            tp as i64,
            "loja:oferta",
            Some(&format!("loja:{pedido}")),
        )
        .await?
        {
            razao::Movimento::Feito { saldo }
            | razao::Movimento::JaFeito { saldo }
            | razao::Movimento::SemSaldo { saldo } => saldo,
        }
    } else {
        razao::saldo(&mut *tx, conta).await?
    };
    tx.commit().await?;
    let teste = provedor != "google_play" || crate::loja_google_play::e_teste(externo);
    Ok(Compra::Feita(
        Entrega {
            preco_centavos: preco.max(0) as u64,
            teste,
            tp,
            energia,
            itens,
            titulo,
            ouro,
            pecas: pecas.len() as u32,
        },
        saldo,
    ))
}

/// One thing a purchase still owes: an item (`item_id`, `qtd`) or energy.
#[derive(Debug, Clone)]
pub struct EntregaPendente {
    pub id: i64,
    pub item_id: u16,
    pub qtd: u32,
    pub energia: u64,
    /// Gold for the character (bonus packs).
    pub ouro: u64,
    /// > 0: `item_id` is a piece of gear rebuilt at this item level.
    pub ilvl: u16,
}

/// CLAIMS everything the account is still owed for `personagem` (marks it
/// delivered) and returns it. The world posts it to the mail right away; if
/// the character is gone by then, `devolver` un-claims it for the next login.
pub async fn reivindicar(
    central: &PgPool,
    conta: &str,
    personagem: &str,
) -> Result<Vec<EntregaPendente>> {
    let linhas = sqlx::query(
        "UPDATE loja_entregas SET entregue_em = NOW(), personagem = $2
          WHERE conta = $1 AND entregue_em IS NULL
          RETURNING id, item_id, qtd, energia, ouro, ilvl",
    )
    .bind(conta)
    .bind(personagem)
    .fetch_all(central)
    .await?;
    Ok(linhas
        .into_iter()
        .map(|r| EntregaPendente {
            id: r.get(0),
            item_id: r.get::<i32, _>(1).clamp(0, u16::MAX as i32) as u16,
            qtd: r.get::<i64, _>(2).clamp(0, u32::MAX as i64) as u32,
            energia: r.get::<i64, _>(3).max(0) as u64,
            ouro: r.get::<i64, _>(4).max(0) as u64,
            ilvl: r.get::<i32, _>(5).clamp(0, u16::MAX as i32) as u16,
        })
        .collect())
}

/// Un-claims deliveries that could not be posted (the character left).
pub async fn devolver(central: &PgPool, ids: &[i64]) -> Result<()> {
    if ids.is_empty() {
        return Ok(());
    }
    sqlx::query(
        "UPDATE loja_entregas SET entregue_em = NULL, personagem = NULL WHERE id = ANY($1)",
    )
    .bind(ids)
    .execute(central)
    .await?;
    Ok(())
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn itens_do_banco_aceitam_lista_e_ignoram_erro() {
        assert_eq!(
            itens_de(
                r#"[{"item_id":345,"qtd":2000},{"item_id":363,"qtd":0}]"#,
                "x"
            ),
            vec![(345, 2000)]
        );
        assert!(itens_de("not json", "x").is_empty());
    }

    /// Every offer the server seeds — level offers and bonus packs — gives
    /// at least 400 TP and at least what its price buys in the TP shop, and
    /// a row written under the rule is lifted when listed or credited.
    #[test]
    fn toda_oferta_semeada_da_o_tp_da_regra() {
        use shared::pacotes_bonus as pb;
        for &(id, _, _, _, produto, preco, _, _, itens) in OFERTAS_DE_NIVEL {
            let tp = pb::tp_base_da_oferta(preco as u32);
            assert!(tp >= pb::TP_MINIMO_OFERTA, "{id}");
            assert!(tp >= pb::tp_do_preco(preco as u32), "{id}");
            assert_eq!(produto, format!("oferta_{preco:03}"), "{id}");
            assert!(
                !itens_de(itens, id).is_empty(),
                "{id}: bonus on top of the TP"
            );
        }
        for p in pb::PACOTES_BONUS {
            assert!(
                p.tp() >= pb::TP_MINIMO_OFERTA && p.tp() >= pb::tp_do_preco(p.preco_centavos),
                "{}",
                p.id
            );
        }
        assert_eq!(tp_da_linha(499, 0), 400, "a 0 TP row is lifted");
        assert_eq!(tp_da_linha(99, 101), 400);
        assert_eq!(
            tp_da_linha(99, 900),
            900,
            "a row above the rule keeps its TP"
        );
        assert_eq!(tp_da_linha(-5, -5), 400);
    }

    #[test]
    fn pecas_do_pacote_ida_e_volta_e_azuis() {
        let v: Vec<(u16, u16)> = vec![
            (shared::item_id::KATANA, 26),
            (0, 30),
            (shared::item_id::CINTO, 0),
        ];
        let json = serde_json::to_string(&v).unwrap();
        assert_eq!(pecas_de(&json, "x"), vec![(shared::item_id::KATANA, 26)]);
        assert!(pecas_de("", "x").is_empty());
        assert!(pecas_de("[[1,", "x").is_empty());
        let p = shared::ItemInstance::roll_for(shared::item_id::KATANA, 26, || 0.5).unwrap();
        assert_eq!(p.rarity, 3, "blue");
    }
}
