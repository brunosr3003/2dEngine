//! What HUD layouts players actually use (`ClientMessage::LayoutDoHud`).
//!
//! One row per account and platform, overwritten on each report: the latest
//! layout of each player on each kind of device. Nothing in the game reads it;
//! it is there to tune the recommended layout (owner, 10/11/2026).

use std::collections::HashMap;

use shared::protocol::LayoutDoHud;

const CRIAR: &str = "CREATE TABLE IF NOT EXISTS hud_layouts (
    conta_id BIGINT NOT NULL,
    plataforma TEXT NOT NULL,
    personagem TEXT NOT NULL DEFAULT '',
    tela_w INT NOT NULL,
    tela_h INT NOT NULL,
    escala_ui REAL NOT NULL,
    botoes REAL NOT NULL,
    personalizado BOOLEAN NOT NULL,
    itens JSONB NOT NULL,
    atualizado TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (conta_id, plataforma)
)";

/// At most one write per account every few seconds (a buggy or hostile
/// client cannot flood the table).
const INTERVALO_S: f64 = 5.0;
static ULTIMO: parking_lot::Mutex<Option<HashMap<i64, f64>>> = parking_lot::Mutex::new(None);

fn agora_s() -> f64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0.0, |d| d.as_secs_f64())
}

pub fn registrar(conta: i64, personagem: String, layout: LayoutDoHud) {
    let Some(l) = layout.validado() else { return };
    {
        let mut g = ULTIMO.lock();
        let m = g.get_or_insert_with(HashMap::new);
        let agora = agora_s();
        if m.get(&conta).is_some_and(|&t| agora - t < INTERVALO_S) {
            return;
        }
        m.insert(conta, agora);
    }
    let Some(pool) = crate::chat_global::pool() else { return };
    let personalizado = (l.botoes - 1.0).abs() > 1e-3 || !l.itens.is_empty();
    let itens = serde_json::json!(l
        .itens
        .iter()
        .map(|i| serde_json::json!({ "id": i.id, "tamanho": i.tamanho, "centro": i.centro.map(|(x, y)| [x, y]) }))
        .collect::<Vec<_>>());
    tokio::spawn(async move {
        let r = async {
            sqlx::query(CRIAR).execute(&pool).await?;
            sqlx::query(
                "INSERT INTO hud_layouts (conta_id, plataforma, personagem, tela_w, tela_h, escala_ui, botoes, personalizado, itens) \
                 VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9::jsonb) \
                 ON CONFLICT (conta_id, plataforma) DO UPDATE SET personagem = $3, tela_w = $4, tela_h = $5, \
                   escala_ui = $6, botoes = $7, personalizado = $8, itens = $9::jsonb, atualizado = NOW()",
            )
            .bind(conta)
            .bind(&l.plataforma)
            .bind(&personagem)
            .bind(l.tela_w as i32)
            .bind(l.tela_h as i32)
            .bind(l.escala_ui)
            .bind(l.botoes)
            .bind(personalizado)
            .bind(itens.to_string())
            .execute(&pool)
            .await
        }
        .await;
        if let Err(e) = r {
            tracing::warn!("hud layout not saved: {e}");
        }
    });
}
