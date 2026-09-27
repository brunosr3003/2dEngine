//! Manda um CONJUNTO de equipamento por correio oficial, pra testar.
//!
//! Existe porque testar equipamento de faixa alta pela mão custa horas de
//! farm, e porque o correio é o caminho certo: ele passa pelo mesmo funil do
//! jogo (`correio::enviar` → `add_to_inventory`), então o que chega é o que
//! um jogador receberia — e não uma linha escrita na marra no banco.
//!
//! Uso:
//!
//!   DATABASE_URL=… cargo run --bin enviar_kit -- <personagem> [cor] [tier] [refino]
//!
//! Padrão: cor 4 (roxo), tier IV, +8. A cor é 1 cinza .. 5 laranja; o tier é
//! I..IV DENTRO da cor (`forja::TIER_MAX`); o refino é o +N.
//!
//! O autor do envio precisa ser staff (`social_staff`). Se o personagem
//! pedido for o único da conta, ele mesmo assina — é ferramenta local.

#[path = "../economy.rs"]
mod economy;

use anyhow::{bail, Context, Result};
use shared::items::ItemInstance;
use shared::social::{Anexo, Pedido};

/// Uma peça por slot: a arma do conjunto espada-e-escudo, a secundária dela,
/// a armadura média e os quatro acessórios.
const KIT: [u16; 7] = [400, 404, 409, 411, 412, 413, 414];

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

    let conta: i64 = sqlx::query_scalar(
        "SELECT account_id FROM characters WHERE lower(name)=lower($1) AND account_id IS NOT NULL",
    )
    .bind(nome)
    .fetch_optional(&pool)
    .await?
    .context("personagem não encontrado")?;

    // O autor precisa ser staff. Ferramenta local: concede e segue.
    sqlx::query(
        "INSERT INTO social_staff (account_id, cargo) VALUES ($1,'admin') \
         ON CONFLICT (account_id) DO UPDATE SET cargo='admin'",
    )
    .bind(conta)
    .execute(&pool)
    .await?;

    let nivel = shared::forja::nivel_de_item_da_cor(cor);
    let anexos: Vec<Anexo> = KIT
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

    // O identificador torna o envio IDEMPOTENTE: rodar de novo com os mesmos
    // argumentos não manda duas vezes.
    let envio = format!("kit-{nome}-{cor}-{tier}-{refino}").to_lowercase();
    let pedido = Pedido::EnviarOficial {
        envio,
        para: Some(nome.clone()),
        assunto: format!("Equipamento de teste ({})", shared::items::tier_name(cor)),
        texto: format!(
            "Conjunto completo: cor {}, Tier {}, +{}. Abra a bolsa e equipe.",
            cor, tier, refino
        ),
        anexos,
    };
    let n = correio::enviar(&pool, nome, conta, &pedido).await?;
    println!("sent to {n} recipient(s): {} pieces", KIT.len());
    Ok(())
}
