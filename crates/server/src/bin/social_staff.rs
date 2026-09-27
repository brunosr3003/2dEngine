//! Gestao local de permissoes. DATABASE_URL obrigatoria; nunca imprime segredos.
use anyhow::{bail, Result};
#[tokio::main]
async fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.len() != 2 || !["admin", "mod", "remover"].contains(&args[1].as_str()) {
        bail!("Uso: social_staff PERSONAGEM admin|mod|remover (DATABASE_URL no ambiente)");
    }
    let pool = sqlx::PgPool::connect(&std::env::var("DATABASE_URL")?).await?;
    let contas: Vec<i64> = sqlx::query_scalar(
        "SELECT account_id FROM characters WHERE lower(name)=lower($1) AND account_id IS NOT NULL",
    )
    .bind(&args[0])
    .fetch_all(&pool)
    .await?;
    let [conta] = contas.as_slice() else {
        bail!("Personagem não encontrado ou ambíguo.");
    };
    if args[1] == "remover" {
        sqlx::query("DELETE FROM social_staff WHERE account_id=$1")
            .bind(conta)
            .execute(&pool)
            .await?;
    } else {
        sqlx::query("INSERT INTO social_staff(account_id,cargo) VALUES($1,$2) ON CONFLICT(account_id) DO UPDATE SET cargo=excluded.cargo").bind(conta).bind(&args[1]).execute(&pool).await?;
    }
    println!("Account for character {}: {}", args[0], args[1]);
    Ok(())
}
