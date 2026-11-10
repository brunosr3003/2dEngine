//! One official letter from a JSON file (staff tool):
//!   DATABASE_URL=… [CARTA_AUTOR=brunji CARTA_CONTA=1] carta_oficial carta.json
//! Same path as the Panopticon's official mail (`correio::enviar`): audited
//! campaign, the `envio` id makes a repeat send a no-op.
//! {"envio": "...", "para": "Name", "assunto": "...", "texto": "...", "anexos": [{"item_id": 605, "qtd": 1}]}
use anyhow::{Context, Result};
use shared::social::{Anexo, Pedido};

#[derive(serde::Deserialize)]
struct Item { item_id: u16, qtd: u32 }
#[derive(serde::Deserialize)]
struct Carta { envio: String, para: String, assunto: String, texto: String, anexos: Vec<Item> }

#[tokio::main]
async fn main() -> Result<()> {
    let arq = std::env::args().nth(1).context("uso: carta_oficial carta.json")?;
    let c: Carta = serde_json::from_str(&std::fs::read_to_string(arq)?)?;
    let pool = sqlx::PgPool::connect(&std::env::var("DATABASE_URL")?).await?;
    let pedido = Pedido::EnviarOficial {
        envio: c.envio,
        para: Some(c.para.clone()),
        assunto: c.assunto,
        texto: c.texto,
        anexos: c.anexos.into_iter().map(|a| Anexo { item_id: a.item_id, qtd: a.qtd, instance: None }).collect(),
    };
    let autor = std::env::var("CARTA_AUTOR").unwrap_or_else(|_| "brunji".into());
    let conta: i64 = std::env::var("CARTA_CONTA").ok().and_then(|v| v.parse().ok()).unwrap_or(1);
    let n = correio::enviar(&pool, &autor, conta, &pedido).await?;
    println!("sent to {} ({n})", c.para);
    Ok(())
}
