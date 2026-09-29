//! Sends a SET of equipment by official mail, for testing.
//!
//! It exists because testing high-tier equipment by hand costs hours of
//! farming, and because the mail is the right path: it goes through the
//! game's own funnel (`correio::enviar` -> `add_to_inventory`), so what
//! arrives is what a player would receive — and not a row written by force
//! into the database.
//!
//! Uso:
//!
//!   DATABASE_URL=… cargo run --bin enviar_kit -- <personagem> [cor] [tier] [refino]
//!
//! Default: color 4 (purple), tier IV, +8. The color is 1 grey .. 5 orange;
//! the tier is I..IV WITHIN the color (`forja::TIER_MAX`); the refinement is the +N.
//!
//! The sender has to be staff (`social_staff`). If the requested character is
//! the account's only one, they sign it themselves — it is a local tool.

#[path = "../economy.rs"]
mod economy;

use anyhow::{bail, Context, Result};
use shared::items::ItemInstance;
use shared::social::{Anexo, Pedido};

/// One piece per slot, built FROM THE CHARACTER'S WEAPON.
///
/// It was fixed to sword-and-shield (`[400, 404, …]`), and on 28/09/2026 the
/// owner asked for a set for `kuni`, who is KATANA: two of the seven pieces
/// would have arrived useless in their bag — the weapon and the offhand,
/// which is the half that matters.
///
/// The tail (medium armor and the four accessories) comes from
/// `ladder::REFERENCE_SET`, the set the damage ladder was measured on. Armor
/// weight is a CHOICE, not a class: medium is the middle of the corridor, and
/// it is deliberate that the test kit does not take sides.
fn kit_da_arma(arma: u16) -> [u16; 7] {
    let conj = shared::skills::Conjunto::da_arma(arma);
    let cauda = &shared::ladder::REFERENCE_SET[2..];
    let mut kit = [arma, conj.secundaria(), 0, 0, 0, 0, 0];
    kit[2..].copy_from_slice(cauda);
    kit
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt().with_env_filter("info").init();
    let a: Vec<String> = std::env::args().skip(1).collect();
    let nome = a.first().context("uso: enviar_kit <personagem> [cor] [tier] [refino]")?;
    let cor: u8 = a.get(1).map_or(Ok(4), |x| x.parse())?;
    let tier: u8 = a.get(2).map_or(Ok(4), |x| x.parse())?;
    let refino: u8 = a.get(3).map_or(Ok(8), |x| x.parse())?;
    if !(1..=5).contains(&cor) || !(1..=shared::forja::TIER_MAX).contains(&tier) {
        bail!("cor tem que ser 1..5 e tier 1..{}", shared::forja::TIER_MAX);
    }

    let pool = sqlx::PgPool::connect(&std::env::var("DATABASE_URL")?).await?;
    economy::init(&pool).await?;
    correio::init(&pool).await?;

    let (conta, arma): (i64, i16) = sqlx::query_as(
        "SELECT account_id, starting_weapon FROM characters \
         WHERE lower(name)=lower($1) AND account_id IS NOT NULL",
    )
    .bind(nome)
    .fetch_optional(&pool)
    .await?
    .context("personagem não encontrado")?;
    let kit = kit_da_arma(arma as u16);

    // The sender must be staff. A local tool: it grants and moves on.
    sqlx::query(
        "INSERT INTO social_staff (account_id, cargo) VALUES ($1,'admin') \
         ON CONFLICT (account_id) DO UPDATE SET cargo='admin'",
    )
    .bind(conta)
    .execute(&pool)
    .await?;

    let nivel = shared::forja::nivel_de_item_da_cor(cor);
    let anexos: Vec<Anexo> = kit
        .iter()
        .map(|id| {
            let mut i = ItemInstance::roll_em(
                economy::item_template_of(*id),
                nivel,
                cor,
                tier,
                fastrand::f32,
            )
            .with_context(|| format!("o item {id} não é equipamento"))?;
            i.refinement = refino;
            Ok(Anexo {
                item_id: *id,
                qtd: 1,
                instance: Some(i),
            })
        })
        .collect::<Result<_>>()?;

    // The identifier makes the send IDEMPOTENT: running it again with the same
    // arguments does not send twice.
    let envio = format!("kit-{nome}-{cor}-{tier}-{refino}").to_lowercase();
    let pedido = Pedido::EnviarOficial {
        envio,
        para: Some(nome.clone()),
        assunto: format!("Equipamento de teste ({})", shared::items::tier_name(cor)),
        texto: format!(
            "Conjunto completo do seu conjunto de arma: cor {}, Tier {}, +{}. \
             Abra a bolsa e equipe.",
            cor, tier, refino
        ),
        anexos,
    };
    let n = correio::enviar(&pool, nome, conta, &pedido).await?;
    println!("sent to {n} recipient(s): {} pieces {kit:?}", kit.len());
    Ok(())
}
