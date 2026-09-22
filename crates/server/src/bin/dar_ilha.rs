//! Põe um personagem no começo da LINHA DA ILHA, pra poder testá-la.
//!
//! A linha do tutorial da colônia (798-802) só abre depois da escritura, que
//! só vem depois de meia história. Testar o fim do capítulo I jogando até lá
//! toda vez não é teste, é maratona — e foi assim que a linha inteira foi pro
//! ar travada no primeiro degrau sem ninguém ver.
//!
//! Isto dá a escritura e adianta o marcador da história até o passo pedido.
//! Só pra personagem de diagnóstico; nunca correr num personagem de verdade,
//! que perderia o progresso dele.
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
    // A colônia volta ao ZERO, e não só ganha a escritura.
    //
    // Antes isto só mexia em `tem` e no relógio: os moradores e o nível da
    // rodada anterior ficavam. Testar "O primeiro morador" com um morador já
    // contratado faz o passo fechar na hora e o SEGUINTE travar — e o teste
    // acusa um defeito que é dele mesmo. Foi o que aconteceu aqui: instrumento
    // que não reseta mede a sujeira da corrida passada.
    let _ = atual;
    let mut dados = shared::colonia::DadosColonia::default();
    dados.tem = true;
    // A conta das horas começa AGORA: sem isso a primeira colheita entregaria
    // tudo o que "rendeu" desde 1970.
    dados.colhida_em = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)?
        .as_secs() as i64;
    sqlx::query("UPDATE characters SET colonia_json=$1 WHERE name=$2")
        .bind(serde_json::to_string(&dados)?)
        .bind(&nome)
        .execute(&pool)
        .await?;

    // A história: o MARCADOR guarda o índice, e o passo em andamento guarda o
    // id. Os dois têm que andar juntos, senão `garantir_historia` realinha o
    // marcador pelo passo velho e desfaz isto.
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
    println!("{nome}: escritura dada, história no passo {passo} '{titulo}' (índice {indice})");
    Ok(())
}
