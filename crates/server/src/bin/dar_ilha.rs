//! Puts a character at the start of the ISLAND LINE, so it can be tested.
//!
//! The colony tutorial line (798-802) only opens after the deed, which only
//! comes after half the story. Testing the end of chapter I by playing all the
//! way there every time is not a test, it is a marathon — and that is how the
//! whole line went live stuck on the first step without anyone seeing.
//!
//! This gives the deed and advances the story marker to the requested step.
//! Only for a diagnostic character; never run it on a real one, which would
//! lose its progress.
//!
//!   DATABASE_URL=… cargo run --bin dar_ilha -- diagcoloniac [id_do_passo]

use anyhow::{bail, Context, Result};

#[tokio::main]
async fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some(nome) = args.first().cloned() else {
        bail!("uso: dar_ilha <personagem> [id_do_passo] (DATABASE_URL no ambiente)");
    };
    let passo: u16 = args
        .get(1)
        .map(|s| s.parse())
        .transpose()
        .context("id do passo inválido")?
        .unwrap_or(798);
    let indice = shared::historia::indice(passo)
        .with_context(|| format!("{passo} não é um passo da história"))?;

    let pool = sqlx::PgPool::connect(&std::env::var("DATABASE_URL")?).await?;
    let atual: Option<String> = sqlx::query_scalar("SELECT colonia_json FROM characters WHERE name=$1")
        .bind(&nome)
        .fetch_optional(&pool)
        .await?
        .context("personagem não existe")?;
    // The colony goes back to ZERO, and does not merely gain the deed.
    //
    // Before, this only touched `tem` and the clock: the residents and the level
    // from the previous round stayed. Testing "The first resident" with a
    // resident already hired closes the step instantly and jams the NEXT one —
    // and the test reports a defect that is its own. That is what happened here:
    // an instrument that does not reset measures the last run's dirt.
    let _ = atual;
    let mut dados = shared::colonia::DadosColonia::default();
    dados.tem = true;
    // The hour count starts NOW: without this the first harvest would deliver
    // everything it had "earned" since 1970.
    dados.colhida_em = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)?
        .as_secs() as i64;
    sqlx::query("UPDATE characters SET colonia_json=$1 WHERE name=$2")
        .bind(serde_json::to_string(&dados)?)
        .bind(&nome)
        .execute(&pool)
        .await?;

    // The story: the MARKER holds the index, and the step in progress holds the
    // id. The two have to move together, or `garantir_historia` realigns the
    // marker by the old step and undoes this.
    sqlx::query("DELETE FROM character_quests WHERE char_name=$1 AND (quest_id=$2 OR quest_id>=700)")
        .bind(&nome)
        .bind(shared::historia::ID_MARCO as i32)
        .execute(&pool)
        .await?;
    sqlx::query(
        "INSERT INTO character_quests(char_name, quest_id, status, progress, cooldown_until) \
         VALUES ($1,$2,$3,$4,0)",
    )
    .bind(&nome)
    .bind(shared::historia::ID_MARCO as i32)
    .bind(shared::historia::STATUS_MARCO as i32)
    .bind(indice as i32)
    .execute(&pool)
    .await?;
    sqlx::query(
        "INSERT INTO character_quests(char_name, quest_id, status, progress, cooldown_until) \
         VALUES ($1,$2,$3,0,0)",
    )
    .bind(&nome)
    .bind(passo as i32)
    .bind(shared::quests::quest_status::ACTIVE as i32)
    .execute(&pool)
    .await?;

    let titulo = shared::quests::quest_by_id(passo).map_or("?", |d| d.title);
    println!("{nome}: deed granted, story at step {passo} '{titulo}' (index {indice})");
    Ok(())
}
