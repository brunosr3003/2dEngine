//! FLASH-OFFER CAMPAIGNS (owner, 10/11/2026: "create a flash offer whenever I
//! want and send it now to everyone").
//!
//! The game side is `server::ofertas`: a row of `ofertas_relampago` (CENTRAL
//! database) with `campanha_ate` set is a campaign — every account that asks
//! for its offers while `campanha_ate > NOW()` gets it, open until
//! `campanha_ate`, whatever the level. The panel only writes that row and then
//! sends `Notificacao::OfertasMudaram` on the REALM database's NOTIFY channel,
//! so every client online asks again right now (that's when the server opens
//! the campaign for the account).
//!
//! Ending one sets `campanha_ate = NOW()` (never NULL: NULL would turn it into
//! a LEVEL offer at level 0) and closes the copies already open. A purchase
//! already paid is still honoured by the server (`comprar` has no expiry check).

use std::collections::HashMap;

use axum::extract::State;
use axum::response::IntoResponse;
use sqlx::Row;

use crate::Estado;

/// Google Play products the game knows (`server::ofertas` seed and
/// docs/OFERTAS_RELAMPAGO.md): id and the price it is set to in the Play
/// Console (same number in R$ and US$). A new price needs a new product in the
/// Play Console first, then a line here.
pub const PRODUTOS: &[(&str, u32)] = &[("oferta_099", 99), ("oferta_499", 499), ("oferta_1500", 1500)];

const FUSO: &str = "America/Sao_Paulo";
pub const HORAS_MAX: i64 = 168;
pub const ITENS_MAX: usize = 8;
const TITULO_MAX: usize = 60;
const TP_MAX: i64 = 100_000;
const ENERGIA_MAX: i64 = 10_000_000;
const QTD_MAX: i64 = 1_000_000;

fn resposta(ok: bool, extra: serde_json::Value) -> axum::response::Response {
    let status = if ok { axum::http::StatusCode::OK } else { axum::http::StatusCode::BAD_REQUEST };
    let mut v = extra;
    v["ok"] = ok.into();
    v["produtos"] = produtos_json();
    (status, axum::Json(v)).into_response()
}

fn erro(msg: impl Into<String>) -> axum::response::Response {
    resposta(false, serde_json::json!({ "erro": msg.into() }))
}

fn produtos_json() -> serde_json::Value {
    PRODUTOS
        .iter()
        .map(|(id, preco)| serde_json::json!({ "id": id, "preco_centavos": preco }))
        .collect()
}

#[derive(serde::Deserialize, serde::Serialize)]
struct ItemJson {
    item_id: u16,
    qtd: u32,
}

/// The game server's own reading (`server::ofertas::itens_de`): bad JSON = none.
fn itens_de(json: &str) -> Vec<(u16, u32)> {
    serde_json::from_str::<Vec<ItemJson>>(json)
        .map(|v| v.into_iter().filter(|i| i.qtd > 0).map(|i| (i.item_id, i.qtd)).collect())
        .unwrap_or_default()
}

/// Central pool and the `campanha_ate` column (added by the game server's
/// `criar_tabelas`; the panel does not own that schema).
async fn central_pronto(st: &Estado) -> Result<std::sync::Arc<sqlx::PgPool>, String> {
    let Some(c) = st.central.clone() else {
        return Err("sem banco central (DATABASE_URL_CENTRAL)".into());
    };
    let tem: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM information_schema.columns \
          WHERE table_name = 'ofertas_relampago' AND column_name = 'campanha_ate')",
    )
    .fetch_one(&*c)
    .await
    .map_err(|e| format!("banco central: {e}"))?;
    if !tem {
        return Err("ofertas_relampago.campanha_ate não existe no banco central: suba o servidor de jogo novo primeiro".into());
    }
    Ok(c)
}

/// Item names from the realm's `items` (EN as stored, PT from the game's dictionary).
/// Value: (EN name, PT name, is equipment, active).
async fn nomes_dos_itens(pool: &sqlx::PgPool, ids: &[i32]) -> HashMap<i32, (String, String, bool, bool)> {
    use shared::idioma::{tr_em, Idioma};
    if ids.is_empty() {
        return HashMap::new();
    }
    let linhas: Vec<(i32, String, Option<String>, bool)> =
        sqlx::query_as("SELECT id, name, equip_slot, active FROM items WHERE id = ANY($1)")
            .bind(ids)
            .fetch_all(pool)
            .await
            .unwrap_or_default();
    linhas
        .into_iter()
        .map(|(id, nome, slot, ativo)| {
            let pt = tr_em(Idioma::Pt, &nome).to_string();
            let equip = crate::e_peca(id as u16, slot.as_deref());
            (id, (nome, pt, equip, ativo))
        })
        .collect()
}

/// "Started at": campaigns made here carry it in the id
/// (`campanha_YYYYMMDD_HHMMSS`, UTC); a hand-made one falls back to
/// end − duration.
const SQL_INICIO: &str = "(CASE WHEN o.id ~ '^campanha_[0-9]{8}_[0-9]{6}$' \
    THEN to_timestamp(substr(o.id, 10), 'YYYYMMDD_HH24MISS')::timestamp AT TIME ZONE 'UTC' \
    ELSE o.campanha_ate - make_interval(mins => GREATEST(o.duracao_min, 0)) END)";

pub async fn lista(State(st): State<Estado>) -> impl IntoResponse {
    let central = match central_pronto(&st).await {
        Ok(c) => c,
        Err(e) => return erro(e),
    };
    let sql = format!(
        "SELECT o.id, o.titulo, o.titulo_pt, o.produto_play, o.preco_centavos, o.valor_de_centavos, \
                o.tp, o.energia, o.itens, (o.ativa AND o.campanha_ate > NOW()), \
                to_char({SQL_INICIO} AT TIME ZONE '{FUSO}', 'DD/MM HH24:MI'), \
                to_char(o.campanha_ate AT TIME ZONE '{FUSO}', 'DD/MM HH24:MI'), \
                GREATEST(EXTRACT(EPOCH FROM (o.campanha_ate - NOW())), 0)::BIGINT, \
                (SELECT COUNT(*) FROM ofertas_relampago_contas c WHERE c.oferta = o.id), \
                (SELECT COUNT(*) FROM ofertas_relampago_contas c WHERE c.oferta = o.id AND c.comprada_em IS NOT NULL), \
                (SELECT COUNT(*) FROM ofertas_relampago_contas c WHERE c.oferta = o.id AND c.comprada_em IS NULL AND c.expira_em > NOW()), \
                o.nota \
           FROM ofertas_relampago o \
          WHERE o.campanha_ate IS NOT NULL \
          ORDER BY {SQL_INICIO} DESC NULLS LAST, o.id DESC \
          LIMIT 50"
    );
    let linhas = match sqlx::query(&sql).fetch_all(&*central).await {
        Ok(v) => v,
        Err(e) => return erro(format!("banco central: {e}")),
    };
    let itens_por: Vec<Vec<(u16, u32)>> = linhas.iter().map(|r| itens_de(&r.get::<String, _>(8))).collect();
    let mut ids: Vec<i32> = itens_por.iter().flatten().map(|(i, _)| *i as i32).collect();
    ids.sort_unstable();
    ids.dedup();
    let nomes = nomes_dos_itens(&st.pool, &ids).await;
    let campanhas: Vec<serde_json::Value> = linhas
        .iter()
        .zip(itens_por)
        .map(|(r, itens)| {
            serde_json::json!({
                "id": r.get::<String, _>(0),
                "titulo": r.get::<String, _>(1),
                "titulo_pt": r.get::<Option<String>, _>(2),
                "produto": r.get::<String, _>(3),
                "preco_centavos": r.get::<i32, _>(4),
                "valor_de_centavos": r.get::<Option<i32>, _>(5),
                "tp": r.get::<i32, _>(6),
                "energia": r.get::<i64, _>(7),
                "itens": itens.iter().map(|(id, qtd)| {
                    let n = nomes.get(&(*id as i32));
                    serde_json::json!({
                        "item_id": id, "qtd": qtd,
                        "nome": n.map(|n| n.0.clone()),
                        "pt": n.map(|n| n.1.clone()),
                    })
                }).collect::<Vec<_>>(),
                "ativa": r.get::<bool, _>(9),
                "inicio": r.get::<Option<String>, _>(10),
                "fim": r.get::<Option<String>, _>(11),
                "segundos": r.get::<i64, _>(12),
                "contas": r.get::<i64, _>(13),
                "compradas": r.get::<i64, _>(14),
                "abertas": r.get::<i64, _>(15),
                "nota": r.get::<Option<String>, _>(16),
            })
        })
        .collect();
    resposta(true, serde_json::json!({ "campanhas": campanhas }))
}

#[derive(serde::Deserialize, Default)]
pub struct Pedido {
    titulo: String,
    #[serde(default)]
    titulo_pt: String,
    produto: String,
    preco_centavos: i64,
    #[serde(default)]
    valor_de_centavos: Option<i64>,
    #[serde(default)]
    tp: i64,
    #[serde(default)]
    energia: i64,
    #[serde(default)]
    itens: Vec<PedidoItem>,
    horas: i64,
}

#[derive(serde::Deserialize, Default, Clone)]
pub struct PedidoItem {
    item_id: i64,
    qtd: i64,
}

/// A request already checked, in the column types of `ofertas_relampago`.
#[derive(Debug, PartialEq)]
pub struct Campanha {
    titulo: String,
    titulo_pt: Option<String>,
    produto: String,
    preco_centavos: i32,
    valor_de_centavos: Option<i32>,
    tp: i32,
    energia: i64,
    itens: Vec<(u16, u32)>,
    horas: i32,
}

impl Campanha {
    /// `itens` exactly as `server::ofertas::itens_de` reads it.
    fn itens_json(&self) -> String {
        serde_json::to_string(
            &self.itens.iter().map(|&(item_id, qtd)| ItemJson { item_id, qtd }).collect::<Vec<_>>(),
        )
        .unwrap_or_else(|_| "[]".into())
    }
}

fn titulo_limpo(t: &str) -> String {
    t.replace(['\n', '\r', '\t'], " ").split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Everything that can be checked without a database.
pub fn validar(p: &Pedido) -> Result<Campanha, String> {
    let titulo = titulo_limpo(&p.titulo);
    if titulo.is_empty() || titulo.chars().count() > TITULO_MAX {
        return Err(format!("título (inglês) vazio ou com mais de {TITULO_MAX} caracteres"));
    }
    let titulo_pt = titulo_limpo(&p.titulo_pt);
    if titulo_pt.chars().count() > TITULO_MAX {
        return Err(format!("título (português) com mais de {TITULO_MAX} caracteres"));
    }
    let produto = p.produto.trim();
    let Some(&(_, preco)) = PRODUTOS.iter().find(|(id, _)| *id == produto) else {
        return Err(format!(
            "produto '{produto}' desconhecido; use um destes: {}",
            PRODUTOS.iter().map(|(id, _)| *id).collect::<Vec<_>>().join(", ")
        ));
    };
    if p.preco_centavos != preco as i64 {
        return Err(format!("o produto {produto} custa {preco} centavos na Play, não {}", p.preco_centavos));
    }
    let valor_de = match p.valor_de_centavos {
        None | Some(0) => None,
        Some(v) if v > preco as i64 && v <= 1_000_000 => Some(v as i32),
        Some(_) => return Err("preço \"de\" (riscado) precisa ser maior que o preço e até R$ 10.000".into()),
    };
    if !(0..=TP_MAX).contains(&p.tp) {
        return Err(format!("TP entre 0 e {TP_MAX}"));
    }
    if !(0..=ENERGIA_MAX).contains(&p.energia) {
        return Err(format!("energia entre 0 e {ENERGIA_MAX}"));
    }
    if p.itens.len() > ITENS_MAX {
        return Err(format!("no máximo {ITENS_MAX} itens"));
    }
    // Same item twice = one line with the sum (the delivery is per line anyway).
    let mut itens: Vec<(u16, u32)> = Vec::new();
    for i in &p.itens {
        if !(1..=u16::MAX as i64).contains(&i.item_id) {
            return Err(format!("item inválido: {}", i.item_id));
        }
        if !(1..=QTD_MAX).contains(&i.qtd) {
            return Err(format!("item {}: quantidade entre 1 e {QTD_MAX}", i.item_id));
        }
        let id = i.item_id as u16;
        match itens.iter_mut().find(|(x, _)| *x == id) {
            Some(e) => {
                let soma = e.1 as i64 + i.qtd;
                if soma > QTD_MAX {
                    return Err(format!("item {id}: quantidade entre 1 e {QTD_MAX}"));
                }
                e.1 = soma as u32;
            }
            None => itens.push((id, i.qtd as u32)),
        }
    }
    if p.tp == 0 && p.energia == 0 && itens.is_empty() {
        return Err("a oferta precisa dar alguma coisa: TP, energia ou item".into());
    }
    if !(1..=HORAS_MAX).contains(&p.horas) {
        return Err(format!("duração entre 1 e {HORAS_MAX} horas"));
    }
    Ok(Campanha {
        titulo,
        titulo_pt: (!titulo_pt.is_empty()).then_some(titulo_pt),
        produto: produto.to_string(),
        preco_centavos: preco as i32,
        valor_de_centavos: valor_de,
        tp: p.tp as i32,
        energia: p.energia,
        itens,
        horas: p.horas as i32,
    })
}

pub async fn criar(State(st): State<Estado>, axum::Json(p): axum::Json<Pedido>) -> impl IntoResponse {
    let c = match validar(&p) {
        Ok(c) => c,
        Err(e) => return erro(e),
    };
    // Items must exist in the game (realm `items`), and bare equipment goes one
    // piece per line (the mail makes it Common Tier I; it doesn't stack).
    let ids: Vec<i32> = c.itens.iter().map(|(i, _)| *i as i32).collect();
    let nomes = nomes_dos_itens(&st.pool, &ids).await;
    for (id, qtd) in &c.itens {
        match nomes.get(&(*id as i32)) {
            None | Some((_, _, _, false)) => return erro(format!("item {id} não existe (ou está inativo) no jogo")),
            Some((nome, _, true, _)) if *qtd != 1 => {
                return erro(format!("{nome} (#{id}) é equipamento: vai 1 por oferta (pediu {qtd})"))
            }
            _ => {}
        }
    }
    let central = match central_pronto(&st).await {
        Ok(c) => c,
        Err(e) => return erro(e),
    };
    let nota = format!("campanha pelo Panoptico, {} h", c.horas);
    // Same columns `server::ofertas` seeds; `nivel` 0 is never read for a
    // campaign (the level query skips `campanha_ate IS NOT NULL`).
    let r: Result<Option<(String, String)>, sqlx::Error> = sqlx::query_as(&format!(
        "INSERT INTO ofertas_relampago \
           (id, titulo, titulo_pt, nivel, duracao_min, produto_play, preco_centavos, valor_de_centavos, \
            tp, energia, itens, ativa, nota, campanha_ate) \
         VALUES ('campanha_' || to_char(NOW() AT TIME ZONE 'UTC', 'YYYYMMDD_HH24MISS'), \
                 $1, $2, 0, $3 * 60, $4, $5, $6, $7, $8, $9, TRUE, $10, NOW() + make_interval(hours => $3)) \
         ON CONFLICT (id) DO NOTHING \
         RETURNING id, to_char(campanha_ate AT TIME ZONE '{FUSO}', 'DD/MM HH24:MI')"
    ))
    .bind(&c.titulo)
    .bind(&c.titulo_pt)
    .bind(c.horas)
    .bind(&c.produto)
    .bind(c.preco_centavos)
    .bind(c.valor_de_centavos)
    .bind(c.tp)
    .bind(c.energia)
    .bind(c.itens_json())
    .bind(&nota)
    .fetch_optional(&*central)
    .await;
    let (id, ate) = match r {
        Ok(Some(v)) => v,
        Ok(None) => return erro("outra campanha foi criada neste mesmo segundo; tente de novo"),
        Err(e) => return erro(format!("banco central: {e}")),
    };
    tracing::info!("ofertas: campanha {id} criada ({} {}c, ate {ate})", c.produto, c.preco_centavos);
    let avisou = crate::chat_notificar(&st.pool, &shared::chat::Notificacao::OfertasMudaram).await;
    if let Err(e) = &avisou {
        tracing::warn!("ofertas: NOTIFY falhou ({e}); quem esta' online ve' no proximo pedido de ofertas");
    }
    resposta(true, serde_json::json!({ "id": id, "ate": ate, "avisou": avisou.is_ok() }))
}

#[derive(serde::Deserialize)]
pub struct Encerrar {
    id: String,
}

pub async fn encerrar(State(st): State<Estado>, axum::Json(e): axum::Json<Encerrar>) -> impl IntoResponse {
    let central = match central_pronto(&st).await {
        Ok(c) => c,
        Err(e) => return erro(e),
    };
    let r: Result<(u64, u64), sqlx::Error> = async {
        let mut tx = central.begin().await?;
        // Only a running CAMPAIGN (never touches a level offer).
        let n = sqlx::query(
            "UPDATE ofertas_relampago SET campanha_ate = NOW() WHERE id = $1 AND campanha_ate > NOW()",
        )
        .bind(&e.id)
        .execute(&mut *tx)
        .await?
        .rows_affected();
        let fechadas = if n > 0 {
            // Copies already open close too (the list shows `expira_em > NOW()`);
            // one paid meanwhile is still delivered (`comprar` ignores expiry).
            sqlx::query(
                "UPDATE ofertas_relampago_contas SET expira_em = NOW() \
                  WHERE oferta = $1 AND comprada_em IS NULL AND expira_em > NOW()",
            )
            .bind(&e.id)
            .execute(&mut *tx)
            .await?
            .rows_affected()
        } else {
            0
        };
        tx.commit().await?;
        Ok((n, fechadas))
    }
    .await;
    match r {
        Ok((0, _)) => erro("campanha não encontrada ou já encerrada"),
        Ok((_, fechadas)) => {
            tracing::info!("ofertas: campanha {} encerrada ({fechadas} abertas fechadas)", e.id);
            let avisou = crate::chat_notificar(&st.pool, &shared::chat::Notificacao::OfertasMudaram).await;
            resposta(true, serde_json::json!({ "fechadas": fechadas, "avisou": avisou.is_ok() }))
        }
        Err(e) => erro(format!("banco central: {e}")),
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    fn base() -> Pedido {
        Pedido {
            titulo: "  Weekend\nFlash Offer ".into(),
            titulo_pt: "Oferta de Fim de Semana".into(),
            produto: "oferta_499".into(),
            preco_centavos: 499,
            valor_de_centavos: Some(2990),
            tp: 0,
            energia: 6000,
            itens: vec![
                PedidoItem { item_id: 363, qtd: 1 },
                PedidoItem { item_id: 345, qtd: 2000 },
                PedidoItem { item_id: 363, qtd: 1 },
            ],
            horas: 24,
        }
    }

    #[test]
    fn campanha_valida_vira_as_colunas_da_tabela() {
        let c = validar(&base()).unwrap();
        assert_eq!(c.titulo, "Weekend Flash Offer");
        assert_eq!(c.titulo_pt.as_deref(), Some("Oferta de Fim de Semana"));
        assert_eq!(c.preco_centavos, 499);
        assert_eq!(c.valor_de_centavos, Some(2990));
        assert_eq!(c.itens, vec![(363, 2), (345, 2000)], "repetido soma numa linha");
        assert_eq!(c.horas, 24);
    }

    /// The JSON written is read back by the game server's own shape.
    #[test]
    fn itens_no_formato_do_servidor() {
        let c = validar(&base()).unwrap();
        let j = c.itens_json();
        assert_eq!(j, r#"[{"item_id":363,"qtd":2},{"item_id":345,"qtd":2000}]"#);
        assert_eq!(itens_de(&j), vec![(363, 2), (345, 2000)]);
    }

    #[test]
    fn recusa_o_que_nao_presta() {
        let com = |f: &dyn Fn(&mut Pedido)| {
            let mut p = base();
            f(&mut p);
            validar(&p)
        };
        assert!(com(&|p| p.titulo = "   ".into()).is_err());
        assert!(com(&|p| p.titulo = "x".repeat(61)).is_err());
        assert!(com(&|p| p.titulo_pt = "x".repeat(61)).is_err());
        assert!(com(&|p| p.produto = "oferta_999".into()).is_err(), "produto fora da lista");
        assert!(com(&|p| p.preco_centavos = 99).is_err(), "preço que não é o do produto");
        assert!(com(&|p| p.valor_de_centavos = Some(100)).is_err(), "riscado menor que o preço");
        assert!(com(&|p| p.tp = -1).is_err());
        assert!(com(&|p| p.energia = -5).is_err());
        assert!(com(&|p| p.horas = 0).is_err());
        assert!(com(&|p| p.horas = 169).is_err());
        assert!(com(&|p| p.itens = vec![PedidoItem { item_id: 345, qtd: 0 }]).is_err());
        assert!(com(&|p| p.itens = vec![PedidoItem { item_id: 0, qtd: 1 }]).is_err());
        assert!(com(&|p| p.itens = vec![PedidoItem { item_id: 70000, qtd: 1 }]).is_err());
        assert!(com(&|p| p.itens = vec![PedidoItem { item_id: 345, qtd: 1 }; 9]).is_err());
        assert!(com(&|p| { p.tp = 0; p.energia = 0; p.itens.clear(); }).is_err(), "sem recompensa");
        assert!(com(&|p| p.horas = 168).is_ok());
        assert!(com(&|p| { p.energia = 0; p.itens.clear(); p.tp = 400; }).is_ok(), "só TP vale");
        assert_eq!(com(&|p| p.valor_de_centavos = Some(0)).unwrap().valor_de_centavos, None);
        assert_eq!(com(&|p| p.titulo_pt = " ".into()).unwrap().titulo_pt, None);
    }
}
