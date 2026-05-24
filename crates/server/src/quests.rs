//! Quests — persistência por personagem + helpers de elegibilidade.
//!
//! As DEFINIÇÕES de quest vivem em `shared::quests::QUESTS` (estáticas, igual
//! ao começo do sistema de recipes). Aqui só persistimos o ESTADO por
//! personagem (`character_quests`) + os pontos de facção (`characters.faction_points`).
//! Hot-reload de defs via DB pode vir depois (padrão recipes/economy).

use shared::quests::{self, QuestDef};
use sqlx::PgPool;

/// Estado de uma quest para um personagem (linha em `character_quests`).
#[derive(Debug, Clone)]
pub struct CharQuest {
    pub quest_id: u16,
    pub status: u8,        // shared::quests::quest_status
    pub progress: u32,
    pub cooldown_until: i64, // unix secs; 0 = sem cooldown
}

/// Cria as tabelas/colunas de quest se não existirem.
pub async fn init(pool: &PgPool) -> anyhow::Result<()> {
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS character_quests (
            char_name      TEXT NOT NULL,
            quest_id       INT  NOT NULL,
            status         SMALLINT NOT NULL DEFAULT 0,
            progress       INT  NOT NULL DEFAULT 0,
            cooldown_until BIGINT NOT NULL DEFAULT 0,
            PRIMARY KEY (char_name, quest_id)
        )"
    ).execute(pool).await?;
    sqlx::query("CREATE INDEX IF NOT EXISTS idx_cquests_char ON character_quests(char_name)")
        .execute(pool).await?;
    sqlx::query("ALTER TABLE characters ADD COLUMN IF NOT EXISTS faction_points INTEGER NOT NULL DEFAULT 0")
        .execute(pool).await?;
    tracing::info!("[quests] schema pronto ({} defs estáticas)", quests::QUESTS.len());
    Ok(())
}

/// Carrega o estado de quests + pontos de facção de um personagem.
pub async fn load_char(pool: &PgPool, char_name: &str) -> anyhow::Result<(Vec<CharQuest>, u32)> {
    let rows: Vec<(i32, i16, i32, i64)> = sqlx::query_as(
        "SELECT quest_id, status, progress, cooldown_until FROM character_quests WHERE char_name = $1"
    ).bind(char_name).fetch_all(pool).await?;
    let quests = rows.into_iter().map(|(qid, st, pr, cd)| CharQuest {
        quest_id: qid as u16,
        status: st as u8,
        progress: pr.max(0) as u32,
        cooldown_until: cd,
    }).collect();
    let fp: i32 = sqlx::query_scalar("SELECT faction_points FROM characters WHERE name = $1")
        .bind(char_name).fetch_optional(pool).await?.unwrap_or(0);
    Ok((quests, fp.max(0) as u32))
}

/// Persiste o estado completo de quests + pontos de facção de um personagem.
pub async fn save_char(pool: &PgPool, char_name: &str, quests: &[CharQuest], faction_points: u32) -> anyhow::Result<()> {
    // Reconcilia: apaga tudo do char e reinsere o estado atual. Assim quests
    // abandonadas (removidas da memória) somem do DB sem lista de deleção.
    let mut tx = pool.begin().await?;
    sqlx::query("DELETE FROM character_quests WHERE char_name = $1")
        .bind(char_name).execute(&mut *tx).await?;
    for q in quests {
        sqlx::query(
            "INSERT INTO character_quests (char_name, quest_id, status, progress, cooldown_until)
             VALUES ($1,$2,$3,$4,$5)"
        )
        .bind(char_name).bind(q.quest_id as i32).bind(q.status as i16)
        .bind(q.progress as i32).bind(q.cooldown_until)
        .execute(&mut *tx).await?;
    }
    sqlx::query("UPDATE characters SET faction_points = $2 WHERE name = $1")
        .bind(char_name).bind(faction_points as i32).execute(&mut *tx).await?;
    tx.commit().await?;
    Ok(())
}

/// Givers (ids) que têm ao menos uma quest aceitável agora pra este player.
/// Mesmos critérios do `offerable` (level, facção, cooldown/estado).
pub fn available_givers(level: u32, faction: u8, active: &[CharQuest], now: i64) -> Vec<u16> {
    let mut set: Vec<u16> = Vec::new();
    for d in quests::QUESTS.iter() {
        if level < d.min_level { continue; }
        if d.faction != quests::faction_id::NONE && d.faction != faction { continue; }
        let ok = match active.iter().find(|c| c.quest_id == d.id) {
            None => true,
            Some(c) => c.status == quests::quest_status::TURNED_IN && d.repeatable && now >= c.cooldown_until,
        };
        if ok && !set.contains(&d.giver) { set.push(d.giver); }
    }
    set
}

/// Quests que `giver` (source+id) oferece e o player PODE aceitar agora.
/// Filtra por level, facção, e cooldown/estado atual.
pub fn offerable<'a>(
    giver_source: u8,
    giver_id: u16,
    level: u32,
    faction: u8,
    active: &[CharQuest],
    now: i64,
) -> Vec<&'a QuestDef> {
    quests::QUESTS.iter().filter(|d| {
        if d.source != giver_source || d.giver != giver_id { return false; }
        if level < d.min_level { return false; }
        if d.faction != quests::faction_id::NONE && d.faction != faction { return false; }
        match active.iter().find(|c| c.quest_id == d.id) {
            None => true, // nunca pegou
            Some(c) => {
                // Ativa/pronta → não reoferece. Concluída → só se repetível e fora do cooldown.
                c.status == quests::quest_status::TURNED_IN
                    && d.repeatable
                    && now >= c.cooldown_until
            }
        }
    }).collect()
}
