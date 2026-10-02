//! Sends MOUNTS or PETS by official mail, for testing.
//!
//! Same funnel as `enviar_kit` and `enviar_chaves`: the game's own mail
//! (`correio::enviar`), claimed into the bag in game.
//!
//! Uso:
//!
//!   DATABASE_URL=… cargo run --bin enviar_bichos -- <personagem> <montaria|pet> <grau> [qtd] [tier] [refino]
//!
//! `grau` is the color, 1 grey .. 5 orange (the creature IS the color); `tier`
//! is I..IV within it (default I) and `refino` the +N (default 0). Each one is
//! born with its own rolled affinity, like any summoned creature.
//!
//! Signed by a character that already is staff (`social_staff`), like
//! `enviar_chaves`: it grants nothing.

use anyhow::{bail, Context, Result};
use shared::items::ItemInstance;
use shared::social::{Anexo, Pedido};

#[tokio::main]
async fn main() -> Result<()> {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let uso = "uso: enviar_bichos <personagem> <montaria|pet> <grau> [qtd] [tier] [refino]";
    let para = a.first().context(uso)?;
    let tipo = a.get(1).context(uso)?.as_str();
    let grau: u8 = a.get(2).context(uso)?.parse()?;
    let qtd: u32 = a.get(3).map_or(Ok(1), |x| x.parse())?;
    let tier: u8 = a.get(4).map_or(Ok(1), |x| x.parse())?;
    let refino: u8 = a.get(5).map_or(Ok(0), |x| x.parse())?;
    if !(1..=5).contains(&grau) || !(1..=shared::forja::TIER_MAX).contains(&tier) {
        bail!("grau tem que ser 1..5 e tier 1..{}", shared::forja::TIER_MAX);
    }
    if refino > shared::forja::REFINO_MAX || !(1..=10).contains(&qtd) {
        bail!("refino até +{} e qtd 1..10", shared::forja::REFINO_MAX);
    }
    use shared::item_id;
    let (id, nome_do) = match tipo {
        "montaria" | "mount" => {
            let id = item_id::montaria_no_grau(item_id::MONTARIA_BASE, grau);
            (id, shared::montarias::nome_do_item(id))
        }
        "pet" => {
            let id = item_id::pet_no_grau(item_id::PET_BASE, grau);
            (id, shared::pets::nome_do_item(id))
        }
        _ => bail!("{uso}"),
    };
    let nome_do = nome_do.context("criatura desconhecida")?;

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

    let anexos: Vec<Anexo> = (0..qtd)
        .map(|_| {
            let mut i = ItemInstance::vazia_de_grau(grau);
            i.tier = tier;
            i.refinement = refino;
            i.afinidade = Some(shared::pets::rolar_afinidade(fastrand::f32(), fastrand::f32()));
            Anexo { item_id: id, qtd: 1, instance: Some(i) }
        })
        .collect();

    // Idempotent per day and arguments, like `enviar_chaves`.
    let dia = shared::dungeon::dia((std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)?
        .as_secs()) as i64);
    let envio = format!("bichos-{nome}-{tipo}-{grau}-{qtd}-{tier}-{refino}-{dia}").to_lowercase();
    let pedido = Pedido::EnviarOficial {
        envio,
        para: Some(nome.clone()),
        assunto: format!("{nome_do} for testing"),
        texto: format!("{qtd}x {nome_do}, Tier {tier}, +{refino}, for testing. Claim it into your bag."),
        anexos,
    };
    let n = correio::enviar(&pool, &autor, conta, &pedido).await?;
    println!("sent to {n} recipient(s) ({nome}), signed by {autor}: {qtd}x {nome_do} T{tier} +{refino}");
    Ok(())
}
