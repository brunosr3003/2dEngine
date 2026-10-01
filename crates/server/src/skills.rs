//! Loading the skills from the database.
//!
//! Twelve rows, all active, three per weapon set. The old table had 44
//! columns — rank, affinity, payload per rank, passive — for a system that
//! no longer exists.

use shared::skills::{Conjunto, Forma, Skill};
use sqlx::{PgPool, Row};

/// Cria a tabela e semeia as doze do playtest, se estiver vazia.
pub async fn init(pool: &PgPool) -> anyhow::Result<()> {
    // A database from before the redesign (44 columns: name, prof, tier…): the
    // `CREATE TABLE IF NOT EXISTS` below does nothing with the old table in
    // place, and the INSERT of the twelve dies with "column nome does not
    // exist" — the channel does not come up. It happened in production. The old
    // one becomes `skills_legado` (its data and the old `player_skills` FK go
    // with it) and the new one is born clean. The constraint is renamed too:
    // `skills_pkey` is an index name, and the new table would create another
    // with the same name.
    let legado: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM information_schema.tables
                        WHERE table_schema = 'public' AND table_name = 'skills')
            AND NOT EXISTS (SELECT 1 FROM information_schema.columns
                            WHERE table_schema = 'public' AND table_name = 'skills'
                              AND column_name = 'nome')",
    )
    .fetch_one(pool)
    .await?;
    if legado {
        let mut tx = pool.begin().await?;
        sqlx::query("ALTER TABLE skills RENAME TO skills_legado")
            .execute(&mut *tx)
            .await?;
        sqlx::query(
            "ALTER TABLE skills_legado RENAME CONSTRAINT skills_pkey TO skills_legado_pkey",
        )
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        tracing::warn!("skills: tabela do sistema antigo renomeada pra skills_legado");
    }

    sqlx::query(
        "CREATE TABLE IF NOT EXISTS skills (
            id            INTEGER PRIMARY KEY,
            nome          TEXT    NOT NULL,
            conjunto      TEXT    NOT NULL,
            ordem         SMALLINT NOT NULL,
            forma         TEXT    NOT NULL,
            custo_mp      INTEGER NOT NULL DEFAULT 0,
            espera_s      REAL    NOT NULL DEFAULT 1,
            conjuracao_s  REAL    NOT NULL DEFAULT 0,
            dano          INTEGER NOT NULL DEFAULT 0,
            cura          INTEGER NOT NULL DEFAULT 0,
            alcance       REAL    NOT NULL DEFAULT 3,
            raio          REAL    NOT NULL DEFAULT 0
        )",
    )
    .execute(pool)
    .await?;

    semear(pool).await?;
    Ok(())
}

/// As doze do playtest.
///
/// Starting numbers, not balance ones: they exist so the playtest has
/// something to press, and the economy panel is what will say whether they
/// are in the right place.
async fn semear(pool: &PgPool) -> anyhow::Result<()> {
    // (id, nome, conjunto, ordem, forma, mp, espera, conjuracao, dano, cura, alcance, raio)
    for l in shared::skills::playtest() {
        sqlx::query(
            "INSERT INTO skills
             (id, nome, conjunto, ordem, forma, custo_mp, espera_s, conjuracao_s,
              dano, cura, alcance, raio)
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12)
             ON CONFLICT (id) DO NOTHING",
        )
        .bind(l.id as i32)
        .bind(l.nome)
        .bind(l.conjunto.chave())
        .bind(l.ordem as i16)
        .bind(l.forma.chave())
        .bind(l.custo_mp)
        .bind(l.espera_s)
        .bind(l.conjuracao_s)
        .bind(l.dano)
        .bind(l.cura)
        .bind(l.alcance)
        .bind(l.raio)
        .execute(pool)
        .await?;
    }
    tracing::info!("skills: 12 do playtest semeadas");
    // Blessing (10) became LIFE DRAIN on 01/10/2026. The seed above never
    // reaches a row that already exists, so the databases in production keep
    // the old self-heal until this runs. Only a row that still holds the old
    // Blessing numbers changes: an admin's tweak stays. NOT by name: the
    // production rows carry the Portuguese name ("Bênção").
    if let Some(dreno) = shared::skills::playtest().into_iter().find(|s| s.id == 10) {
        let mudou = sqlx::query(
            "UPDATE skills SET nome = $1, forma = $2, custo_mp = $3, espera_s = $4,
                    conjuracao_s = $5, dano = $6, cura = $7, alcance = $8, raio = $9
              WHERE id = 10 AND forma = 'em_si' AND dano = 0 AND cura = 40",
        )
        .bind(&dreno.nome)
        .bind(dreno.forma.chave())
        .bind(dreno.custo_mp)
        .bind(dreno.espera_s)
        .bind(dreno.conjuracao_s)
        .bind(dreno.dano)
        .bind(dreno.cura)
        .bind(dreno.alcance)
        .bind(dreno.raio)
        .execute(pool)
        .await?
        .rows_affected();
        if mudou > 0 {
            tracing::info!("skills: Blessing (10) is now Life Drain");
        }
    }
    Ok(())
}

/// In-memory catalogue. Twelve rows — it fits in an `RwLock` without ceremony.
static CATALOGO: parking_lot::RwLock<Vec<Skill>> = parking_lot::RwLock::new(Vec::new());

/// Todas as skills, pro cliente montar a barra.
pub fn all_skills() -> Vec<Skill> {
    CATALOGO.read().clone()
}

/// A skill de um id.
pub fn skill_of(id: u32) -> Option<Skill> {
    CATALOGO.read().iter().find(|s| s.id == id).cloned()
}

/// As tres de um conjunto, na ordem.
pub fn do_conjunto(c: Conjunto) -> Vec<Skill> {
    let mut v: Vec<Skill> = CATALOGO
        .read()
        .iter()
        .filter(|s| s.conjunto == c)
        .cloned()
        .collect();
    v.sort_by_key(|s| s.ordem);
    v
}

pub async fn recarregar(pool: &PgPool) {
    let v = carregar(pool).await;
    tracing::info!("skills: {} carregadas", v.len());
    *CATALOGO.write() = v;
}

pub async fn carregar(pool: &PgPool) -> Vec<Skill> {
    let rs = match sqlx::query(
        "SELECT id, nome, conjunto, ordem, forma, custo_mp, espera_s,
                conjuracao_s, dano, cura, alcance, raio
           FROM skills ORDER BY conjunto, ordem",
    )
    .fetch_all(pool)
    .await
    {
        Ok(r) => r,
        Err(e) => {
            tracing::error!("skills: nao carregou: {e}");
            return Vec::new();
        }
    };
    rs.iter()
        .filter_map(|r| {
            // A row with an unknown set or shape is SKIPPED and reported. Falling back
            // to the first value by default would put the skill on the wrong weapon,
            // and nobody would find out by looking at the database.
            let conjunto: String = r.get("conjunto");
            let forma: String = r.get("forma");
            let (Some(conjunto), Some(forma)) =
                (Conjunto::de_chave(&conjunto), Forma::de_chave(&forma))
            else {
                tracing::warn!(
                    "skill #{}: conjunto/forma desconhecidos ({conjunto}/{forma}) — pulada",
                    r.get::<i32, _>("id")
                );
                return None;
            };
            Some(Skill {
                id: r.get::<i32, _>("id") as u32,
                nome: r.get("nome"),
                conjunto,
                ordem: r.get::<i16, _>("ordem") as u8,
                forma,
                custo_mp: r.get("custo_mp"),
                espera_s: r.get("espera_s"),
                conjuracao_s: r.get("conjuracao_s"),
                dano: r.get("dano"),
                cura: r.get("cura"),
                alcance: r.get("alcance"),
                raio: r.get("raio"),
            })
        })
        .collect()
}
