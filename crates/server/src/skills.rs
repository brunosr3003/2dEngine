//! Carga das skills do banco.
//!
//! Doze linhas, todas ativas, tres por conjunto de arma. A tabela antiga tinha
//! 44 colunas — rank, afinidade, payload por rank, passiva — pra um sistema que
//! nao existe mais.

use shared::skills::{Conjunto, Forma, Skill};
use sqlx::{PgPool, Row};

/// Cria a tabela e semeia as doze do playtest, se estiver vazia.
pub async fn init(pool: &PgPool) -> anyhow::Result<()> {
    // Banco de antes do redesenho (44 colunas: name, prof, tier…): o `CREATE
    // TABLE IF NOT EXISTS` abaixo nao faz nada com a tabela velha no lugar, e o
    // INSERT das doze morre em "column nome does not exist" — o canal nao sobe.
    // Aconteceu na producao. A velha vira `skills_legado` (dados e a FK do
    // `player_skills` antigo vao junto) e a nova nasce limpa. A constraint e'
    // renomeada tambem: `skills_pkey` e' nome de indice, e a tabela nova
    // criaria outra com o mesmo nome.
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
        sqlx::query("ALTER TABLE skills RENAME TO skills_legado").execute(&mut *tx).await?;
        sqlx::query("ALTER TABLE skills_legado RENAME CONSTRAINT skills_pkey TO skills_legado_pkey")
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
/// Numeros de partida, nao de equilibrio: eles existem pra o playtest ter o que
/// apertar, e o painel de economia e' quem vai dizer se estao no lugar.
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
        .bind(l.id as i32).bind(l.nome).bind(l.conjunto.chave()).bind(l.ordem as i16).bind(l.forma.chave()).bind(l.custo_mp)
        .bind(l.espera_s).bind(l.conjuracao_s).bind(l.dano).bind(l.cura).bind(l.alcance).bind(l.raio)
        .execute(pool)
        .await?;
    }
    tracing::info!("skills: 12 do playtest semeadas");
    Ok(())
}

/// Catalogo em memoria. Doze linhas — cabe num `RwLock` sem cerimonia.
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
    let mut v: Vec<Skill> = CATALOGO.read().iter().filter(|s| s.conjunto == c).cloned().collect();
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
            // Linha com conjunto ou forma desconhecidos e' PULADA e avisada.
            // Cair no primeiro valor por padrao poria a skill na arma errada,
            // e ninguem descobriria olhando o banco.
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
