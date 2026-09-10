//! Carga das skills do banco.
//!
//! Doze linhas, todas ativas, tres por conjunto de arma. A tabela antiga tinha
//! 44 colunas — rank, afinidade, payload por rank, passiva — pra um sistema que
//! nao existe mais.

use shared::skills::{Conjunto, Forma, Skill};
use sqlx::{PgPool, Row};

/// Cria a tabela e semeia as doze do playtest, se estiver vazia.
pub async fn init(pool: &PgPool) -> anyhow::Result<()> {
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

    let (n,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM skills")
        .fetch_one(pool)
        .await
        .unwrap_or((0,));
    if n == 0 {
        semear(pool).await?;
    }
    Ok(())
}

/// As doze do playtest.
///
/// Numeros de partida, nao de equilibrio: eles existem pra o playtest ter o que
/// apertar, e o painel de economia e' quem vai dizer se estao no lugar.
async fn semear(pool: &PgPool) -> anyhow::Result<()> {
    // (id, nome, conjunto, ordem, forma, mp, espera, conjuracao, dano, cura, alcance, raio)
    let linhas: [(i32, &str, &str, i16, &str, i32, f32, f32, i32, i32, f32, f32); 12] = [
        // ── espada e escudo: segurar a linha ──
        (1, "Investida",   "espada_escudo", 1, "linha",   10, 8.0,  0.0, 25,  0, 6.0, 1.0),
        (2, "Golpe Largo", "espada_escudo", 2, "cone",    15, 6.0,  0.0, 35,  0, 3.5, 0.0),
        (3, "Muralha",     "espada_escudo", 3, "em_si",   25, 20.0, 0.0,  0,  0, 0.0, 0.0),
        // ── katana: corte rapido ──
        (4, "Saque",       "katana", 1, "linha",    8, 6.0,  0.0, 30,  0, 4.0, 0.8),
        (5, "Dança",       "katana", 2, "circulo", 18, 10.0, 0.0, 28,  0, 0.0, 2.5),
        (6, "Vento Cortante","katana",3,"projetil",22, 12.0, 0.3, 45,  0, 9.0, 0.0),
        // ── duas pistolas: distancia ──
        (7, "Tiro Certeiro","pistolas", 1, "projetil", 8, 4.0,  0.0, 28, 0, 11.0, 0.0),
        (8, "Rajada",       "pistolas", 2, "cone",    16, 9.0,  0.0, 20, 0,  6.0, 0.0),
        (9, "Barril",       "pistolas", 3, "circulo", 24, 16.0, 0.4, 50, 0,  8.0, 3.0),
        // ── anel magico: cura e magia ──
        (10, "Bênção",     "anel_magico", 1, "em_si",   14, 10.0, 0.0,  0, 40, 0.0, 0.0),
        (11, "Aura",       "anel_magico", 2, "circulo", 26, 18.0, 0.5,  0, 30, 7.0, 4.0),
        (12, "Julgamento", "anel_magico", 3, "circulo", 30, 14.0, 0.6, 55,  0, 9.0, 3.0),
    ];
    for l in linhas {
        sqlx::query(
            "INSERT INTO skills
             (id, nome, conjunto, ordem, forma, custo_mp, espera_s, conjuracao_s,
              dano, cura, alcance, raio)
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12)
             ON CONFLICT (id) DO NOTHING",
        )
        .bind(l.0).bind(l.1).bind(l.2).bind(l.3).bind(l.4).bind(l.5)
        .bind(l.6).bind(l.7).bind(l.8).bind(l.9).bind(l.10).bind(l.11)
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
