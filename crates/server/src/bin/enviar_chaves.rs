//! Sends Porão KEYS by official mail, for testing.
//!
//! Same funnel as `enviar_kit`: the game's own mail (`correio::enviar`), so the
//! keys arrive the way any attachment does, claimed into the bag in game.
//!
//! Uso:
//!
//!   DATABASE_URL=… cargo run --bin enviar_chaves -- <personagem> [quantidade] [porão]
//!
//! One key of every Porão, `quantidade` each (default 5), in a single letter;
//! with `porão` (the content id, e.g. 5 = Thunder Vault), only that one.
//!
//! Unlike `enviar_kit`, this does NOT make the recipient staff: the letter is
//! signed by a character that already is (`social_staff`). Granting admin to
//! whoever is testing keys would be a side effect nobody asked for.

use anyhow::{Context, Result};
use shared::social::{Anexo, Pedido};

#[tokio::main]
async fn main() -> Result<()> {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let para = a.first().context("uso: enviar_chaves <personagem> [quantidade]")?;
    let qtd: u32 = a.get(1).map_or(Ok(5), |x| x.parse())?;
    let so: Option<u16> = a.get(2).map(|x| x.parse()).transpose()?;

    let pool = sqlx::PgPool::connect(&std::env::var("DATABASE_URL")?).await?;
    correio::init(&pool).await?;

    let (nome,): (String,) = sqlx::query_as(
        "SELECT name FROM characters WHERE lower(name)=lower($1) AND account_id IS NOT NULL",
    )
    .bind(para)
    .fetch_optional(&pool)
    .await?
    .context("personagem não encontrado")?;

    let (autor, conta): (String, i64) = sqlx::query_as(
        "SELECT c.name, c.account_id FROM characters c \
         JOIN social_staff s ON s.account_id = c.account_id \
         ORDER BY c.account_id, c.name LIMIT 1",
    )
    .fetch_optional(&pool)
    .await?
    .context("no staff character to sign the letter (social_staff is empty)")?;

    let anexos: Vec<Anexo> = shared::dungeon::CONTEUDOS
        .iter()
        .filter(|c| so.is_none_or(|id| c.id == id))
        .filter_map(|c| shared::porao::chave_de(c))
        .map(|item_id| Anexo {
            item_id,
            qtd,
            instance: None,
        })
        .collect();
    anyhow::ensure!(!anexos.is_empty(), "no Porão with id {so:?}");
    let nomes: Vec<String> = anexos
        .iter()
        .filter_map(|x| shared::porao::nome_da_chave(x.item_id))
        .collect();

    // Idempotent: the same recipient and amount on the same day sends once.
    let dia = shared::dungeon::dia((std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)?
        .as_secs()) as i64);
    let envio = match so {
        Some(id) => format!("chaves-{nome}-{qtd}-{dia}-p{id}"),
        None => format!("chaves-{nome}-{qtd}-{dia}"),
    }
    .to_lowercase();
    let pedido = Pedido::EnviarOficial {
        envio,
        para: Some(nome.clone()),
        assunto: "Porão keys for testing".into(),
        texto: match so {
            Some(_) => format!("{qtd}x {}, for testing. Claim it into your bag.", nomes.join(", ")),
            None => format!(
                "{qtd} keys of every Porão, for testing the new cellars. Claim them into your bag."
            ),
        },
        anexos,
    };
    let n = correio::enviar(&pool, &autor, conta, &pedido).await?;
    println!("sent to {n} recipient(s) ({nome}), signed by {autor}: {qtd}x {nomes:?}");
    Ok(())
}
