//! Social persistence in the realm's database. No write runs on the tick.
//! Transactions serialise the social mutations so limits and roles are
//! validated even when requests arrive simultaneously on different channels.
use anyhow::{bail, Result};
use shared::social::*;
use sqlx::{PgConnection, PgPool, Row};

pub async fn init(pool: &PgPool) -> Result<()> {
    for sql in [
        "CREATE TABLE IF NOT EXISTS social_amigos (de TEXT NOT NULL REFERENCES characters(name), para TEXT NOT NULL REFERENCES characters(name), aceito BOOLEAN NOT NULL DEFAULT FALSE, PRIMARY KEY(de,para), CHECK(de <> para))",
        "CREATE TABLE IF NOT EXISTS social_cartas (id BIGSERIAL PRIMARY KEY, de TEXT NOT NULL REFERENCES characters(name), para TEXT NOT NULL REFERENCES characters(name), assunto TEXT NOT NULL, texto TEXT NOT NULL, quando BIGINT NOT NULL, lida BOOLEAN NOT NULL DEFAULT FALSE)",
        "CREATE INDEX IF NOT EXISTS social_caixa ON social_cartas(para,id DESC)",
        "CREATE TABLE IF NOT EXISTS social_clas (id BIGSERIAL PRIMARY KEY, nome TEXT NOT NULL, lider TEXT NOT NULL REFERENCES characters(name), aviso TEXT NOT NULL DEFAULT '')",
        "CREATE UNIQUE INDEX IF NOT EXISTS social_nome_cla ON social_clas(lower(nome))",
        "CREATE TABLE IF NOT EXISTS social_membros (nome TEXT PRIMARY KEY REFERENCES characters(name), cla BIGINT NOT NULL REFERENCES social_clas(id) ON DELETE CASCADE)",
        "CREATE TABLE IF NOT EXISTS social_convites (nome TEXT NOT NULL REFERENCES characters(name), cla BIGINT NOT NULL REFERENCES social_clas(id) ON DELETE CASCADE, de TEXT NOT NULL, PRIMARY KEY(nome,cla))",
        "CREATE TABLE IF NOT EXISTS social_envios (nome TEXT PRIMARY KEY REFERENCES characters(name), quando BIGINT NOT NULL)",
    ] {
        sqlx::query(sql).execute(pool).await?;
    }
    crate::correio_admin::init(pool).await?;
    Ok(())
}

async fn alvo(db: &mut PgConnection, eu: &str, nome: &str) -> Result<String> {
    if !texto_valido(nome, 1, 32) {
        bail!("Invalid character name.");
    }
    let nomes: Vec<String> = sqlx::query_scalar(
        "SELECT name FROM characters WHERE lower(name)=lower($1) ORDER BY name LIMIT 2",
    )
    .bind(nome.trim())
    .fetch_all(&mut *db)
    .await?;
    let nome = match nomes.as_slice() {
        [n] => n.clone(),
        _ => bail!("Character not found, or the name is ambiguous."),
    };
    if nome == eu {
        bail!("Choose another character.");
    }
    Ok(nome)
}

async fn meu_cla(db: &mut PgConnection, eu: &str) -> Result<Option<(i64, String)>> {
    Ok(sqlx::query_as("SELECT c.id,c.lider FROM social_clas c JOIN social_membros m ON m.cla=c.id WHERE m.nome=$1")
        .bind(eu).fetch_optional(db).await?)
}
async fn lider(db: &mut PgConnection, eu: &str) -> Result<i64> {
    match meu_cla(db, eu).await? {
        Some((id, nome)) if nome == eu => Ok(id),
        _ => bail!("Only the clan leader can do that."),
    }
}
async fn vagas_amigo(db: &mut PgConnection, nome: &str) -> Result<()> {
    let n: i64 = sqlx::query_scalar("SELECT count(*) FROM social_amigos WHERE de=$1 OR para=$1")
        .bind(nome)
        .fetch_one(db)
        .await?;
    if n >= MAX_AMIGOS as i64 {
        bail!("Friend or request list is full (100).");
    }
    Ok(())
}

pub async fn executar(pool: &PgPool, eu: &str, pedido: &Pedido) -> Result<()> {
    if matches!(pedido, Pedido::Estado) {
        return Ok(());
    }
    if let Pedido::LerCarta { id } | Pedido::ApagarCarta { id } = pedido {
        if *id < 0 {
            return crate::correio_admin::ler_apagar(
                pool,
                eu,
                *id,
                matches!(pedido, Pedido::ApagarCarta { .. }),
            )
            .await;
        }
    }
    let mut tx = pool.begin().await?;
    // Transactional lock for the subsystem, shared by every channel.
    sqlx::query("SELECT pg_advisory_xact_lock(83910422)")
        .execute(&mut *tx)
        .await?;
    match pedido {
        Pedido::Estado => {}
        Pedido::EnviarOficial { .. } => {
            bail!("Envio administrativo deve passar pelo canal autenticado.");
        }
        Pedido::ReceberAnexos { .. } => {
            bail!("The claim has to go through the world.");
        }
        Pedido::Amizade { nome } => {
            let outro = alvo(&mut tx, eu, nome).await?;
            let existe: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM social_amigos WHERE (de=$1 AND para=$2) OR (de=$2 AND para=$1))")
                .bind(eu).bind(&outro).fetch_one(&mut *tx).await?;
            if existe {
                bail!("There is already a friendship or request between you.");
            }
            vagas_amigo(&mut tx, eu).await?;
            vagas_amigo(&mut tx, &outro).await?;
            sqlx::query("INSERT INTO social_amigos(de,para) VALUES($1,$2)")
                .bind(eu)
                .bind(outro)
                .execute(&mut *tx)
                .await?;
        }
        Pedido::ResponderAmizade { nome, aceitar } => {
            let sql = if *aceitar {
                "UPDATE social_amigos SET aceito=TRUE WHERE de=$1 AND para=$2 AND NOT aceito"
            } else {
                "DELETE FROM social_amigos WHERE de=$1 AND para=$2 AND NOT aceito"
            };
            if sqlx::query(sql)
                .bind(nome)
                .bind(eu)
                .execute(&mut *tx)
                .await?
                .rows_affected()
                == 0
            {
                bail!("That friend request is no longer available.");
            }
        }
        Pedido::RemoverAmigo { nome } => {
            sqlx::query(
                "DELETE FROM social_amigos WHERE (de=$1 AND para=$2) OR (de=$2 AND para=$1)",
            )
            .bind(eu)
            .bind(nome)
            .execute(&mut *tx)
            .await?;
        }
        Pedido::EnviarCarta {
            para,
            assunto,
            texto,
        } => {
            if !texto_valido(assunto, 1, 60) || !texto_valido(texto, 1, 1000) {
                bail!("Preencha assunto (até 60) e mensagem (até 1000 caracteres).");
            }
            let para = alvo(&mut tx, eu, para).await?;
            let n: i64 = sqlx::query_scalar("SELECT count(*) FROM social_cartas WHERE para=$1")
                .bind(&para)
                .fetch_one(&mut *tx)
                .await?;
            if n >= MAX_CARTAS as i64 {
                bail!("The recipient's inbox is full (100 letters).");
            }
            let agora: i64 = sqlx::query_scalar("SELECT extract(epoch FROM now())::bigint")
                .fetch_one(&mut *tx)
                .await?;
            let ultimo: Option<i64> =
                sqlx::query_scalar("SELECT quando FROM social_envios WHERE nome=$1")
                    .bind(eu)
                    .fetch_optional(&mut *tx)
                    .await?;
            if ultimo.is_some_and(|t| agora - t < 10) {
                bail!("Wait 10 seconds between letters.");
            }
            sqlx::query(
                "INSERT INTO social_cartas(de,para,assunto,texto,quando) VALUES($1,$2,$3,$4,$5)",
            )
            .bind(eu)
            .bind(para)
            .bind(assunto.trim())
            .bind(texto.trim())
            .bind(agora)
            .execute(&mut *tx)
            .await?;
            sqlx::query("INSERT INTO social_envios(nome,quando) VALUES($1,$2) ON CONFLICT(nome) DO UPDATE SET quando=excluded.quando")
                .bind(eu).bind(agora).execute(&mut *tx).await?;
        }
        Pedido::LerCarta { id } | Pedido::ApagarCarta { id } => {
            let sql = if matches!(pedido, Pedido::LerCarta { .. }) {
                "UPDATE social_cartas SET lida=TRUE WHERE id=$1 AND para=$2"
            } else {
                "DELETE FROM social_cartas WHERE id=$1 AND para=$2"
            };
            if sqlx::query(sql)
                .bind(id)
                .bind(eu)
                .execute(&mut *tx)
                .await?
                .rows_affected()
                == 0
            {
                bail!("Letter not found.");
            }
        }
        Pedido::CriarCla { nome } => {
            if !texto_valido(nome, 3, 24) {
                bail!("Clan name: 3 to 24 characters.");
            }
            if meu_cla(&mut tx, eu).await?.is_some() {
                bail!("You already belong to a clan.");
            }
            let existe: bool = sqlx::query_scalar(
                "SELECT EXISTS(SELECT 1 FROM social_clas WHERE lower(nome)=lower($1))",
            )
            .bind(nome.trim())
            .fetch_one(&mut *tx)
            .await?;
            if existe {
                bail!("A clan with that name already exists.");
            }
            let id: i64 = sqlx::query_scalar(
                "INSERT INTO social_clas(nome,lider) VALUES($1,$2) RETURNING id",
            )
            .bind(nome.trim())
            .bind(eu)
            .fetch_one(&mut *tx)
            .await?;
            sqlx::query("INSERT INTO social_membros(nome,cla) VALUES($1,$2)")
                .bind(eu)
                .bind(id)
                .execute(&mut *tx)
                .await?;
            sqlx::query("DELETE FROM social_convites WHERE nome=$1")
                .bind(eu)
                .execute(&mut *tx)
                .await?;
        }
        Pedido::ConvidarCla { nome } => {
            let id = lider(&mut tx, eu).await?;
            let nome = alvo(&mut tx, eu, nome).await?;
            if meu_cla(&mut tx, &nome).await?.is_some() {
                bail!("That character already has a clan.");
            }
            let n: i64 =
                sqlx::query_scalar("SELECT count(*) FROM social_convites WHERE nome=$1 OR cla=$2")
                    .bind(&nome)
                    .bind(id)
                    .fetch_one(&mut *tx)
                    .await?;
            if n >= MAX_CLA as i64 {
                bail!("Pending invite limit reached.");
            }
            sqlx::query(
                "INSERT INTO social_convites(nome,cla,de) VALUES($1,$2,$3) ON CONFLICT DO NOTHING",
            )
            .bind(nome)
            .bind(id)
            .bind(eu)
            .execute(&mut *tx)
            .await?;
        }
        Pedido::ResponderCla { id, aceitar } => {
            if sqlx::query("DELETE FROM social_convites WHERE nome=$1 AND cla=$2")
                .bind(eu)
                .bind(id)
                .execute(&mut *tx)
                .await?
                .rows_affected()
                == 0
            {
                bail!("That clan invite is no longer available.");
            }
            if *aceitar {
                if meu_cla(&mut tx, eu).await?.is_some() {
                    bail!("Leave your clan before accepting another.");
                }
                let n: i64 = sqlx::query_scalar("SELECT count(*) FROM social_membros WHERE cla=$1")
                    .bind(id)
                    .fetch_one(&mut *tx)
                    .await?;
                if n >= MAX_CLA as i64 {
                    bail!("Clan full (50 members).");
                }
                sqlx::query("INSERT INTO social_membros(nome,cla) VALUES($1,$2)")
                    .bind(eu)
                    .bind(id)
                    .execute(&mut *tx)
                    .await?;
                sqlx::query("DELETE FROM social_convites WHERE nome=$1")
                    .bind(eu)
                    .execute(&mut *tx)
                    .await?;
            }
        }
        Pedido::SairCla => {
            let Some((_, chefe)) = meu_cla(&mut tx, eu).await? else {
                bail!("You have no clan.");
            };
            if chefe == eu {
                bail!("Transfira a liderança ou dissolva o clã antes de sair.");
            }
            sqlx::query("DELETE FROM social_membros WHERE nome=$1")
                .bind(eu)
                .execute(&mut *tx)
                .await?;
        }
        Pedido::ExpulsarCla { nome } | Pedido::LiderCla { nome } => {
            let id = lider(&mut tx, eu).await?;
            let nome = alvo(&mut tx, eu, nome).await?;
            if !meu_cla(&mut tx, &nome).await?.is_some_and(|(c, _)| c == id) {
                bail!("That character does not belong to your clan.");
            }
            if matches!(pedido, Pedido::LiderCla { .. }) {
                sqlx::query("UPDATE social_clas SET lider=$1 WHERE id=$2")
                    .bind(nome)
                    .bind(id)
                    .execute(&mut *tx)
                    .await?;
            } else {
                sqlx::query("DELETE FROM social_membros WHERE nome=$1 AND cla=$2")
                    .bind(nome)
                    .bind(id)
                    .execute(&mut *tx)
                    .await?;
            }
        }
        Pedido::AvisoCla { texto } => {
            let id = lider(&mut tx, eu).await?;
            if !texto_valido(texto, 0, 200) {
                bail!("Clan notice: up to 200 characters.");
            }
            sqlx::query("UPDATE social_clas SET aviso=$1 WHERE id=$2")
                .bind(texto.trim())
                .bind(id)
                .execute(&mut *tx)
                .await?;
        }
        Pedido::DissolverCla => {
            let id = lider(&mut tx, eu).await?;
            sqlx::query("DELETE FROM social_clas WHERE id=$1")
                .bind(id)
                .execute(&mut *tx)
                .await?;
        }
    }
    tx.commit().await?;
    Ok(())
}

pub async fn estado(pool: &PgPool, eu: &str) -> Result<Estado> {
    let mut tx = pool.begin().await?;
    sqlx::query("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ READ ONLY")
        .execute(&mut *tx)
        .await?;
    let mut e = Estado::default();
    e.cargo = crate::correio_admin::cargo(&mut tx, eu).await?;
    if e.cargo.is_some() {
        e.destinatarios = sqlx::query_scalar::<_, i64>("SELECT count(*) FROM characters")
            .fetch_one(&mut *tx)
            .await?
            .min(u32::MAX as i64) as u32;
    }
    e.oficiais = crate::correio_admin::listar(&mut tx, eu).await?;
    for r in sqlx::query(
        "SELECT de,para,aceito FROM social_amigos WHERE de=$1 OR para=$1 ORDER BY de,para",
    )
    .bind(eu)
    .fetch_all(&mut *tx)
    .await?
    {
        let de: String = r.get("de");
        let para: String = r.get("para");
        if r.get::<bool, _>("aceito") {
            e.amigos.push(if de == eu { para } else { de });
        } else if de == eu {
            e.enviados.push(para);
        } else {
            e.recebidos.push(de);
        }
    }
    e.amigos.sort();
    for r in sqlx::query("SELECT id,de,assunto,texto,quando,lida FROM social_cartas WHERE para=$1 ORDER BY id DESC LIMIT 100")
        .bind(eu).fetch_all(&mut *tx).await? {
        e.cartas.push(Carta { id:r.get("id"), de:r.get("de"), assunto:r.get("assunto"), texto:r.get("texto"), quando:r.get("quando"), lida:r.get("lida"), oficial:false, anexos:vec![], resgatada:false });
    }
    if let Some((id, chefe)) = meu_cla(&mut tx, eu).await? {
        let r = sqlx::query("SELECT nome,aviso FROM social_clas WHERE id=$1")
            .bind(id)
            .fetch_one(&mut *tx)
            .await?;
        let membros =
            sqlx::query_scalar("SELECT nome FROM social_membros WHERE cla=$1 ORDER BY nome")
                .bind(id)
                .fetch_all(&mut *tx)
                .await?;
        e.cla = Some(Cla {
            id,
            nome: r.get("nome"),
            aviso: r.get("aviso"),
            lider: chefe,
            membros,
        });
    }
    for r in sqlx::query("SELECT c.id,c.nome,i.de FROM social_convites i JOIN social_clas c ON c.id=i.cla WHERE i.nome=$1 ORDER BY c.nome")
        .bind(eu).fetch_all(&mut *tx).await? {
        e.convites_cla.push(ConviteCla { id:r.get("id"), nome:r.get("nome"), de:r.get("de") });
    }
    tx.commit().await?;
    Ok(e)
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::postgres::PgPoolOptions;

    /// Disposable schema: never uses the real characters.
    #[tokio::test]
    #[ignore = "requer TEST_DATABASE_URL; cria e remove schema isolado"]
    async fn social_postgres_permissoes_persistencia_e_concorrencia() -> Result<()> {
        let url = std::env::var("TEST_DATABASE_URL")?;
        let admin = PgPool::connect(&url).await?;
        let schema = format!(
            "social_test_{}_{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)?
                .as_millis()
        );
        sqlx::query(&format!("CREATE SCHEMA {schema}"))
            .execute(&admin)
            .await?;
        let search = format!("SET search_path TO {schema}");
        let pool = PgPoolOptions::new()
            .max_connections(4)
            .after_connect(move |c, _| {
                let sql = search.clone();
                Box::pin(async move {
                    sqlx::query(&sql).execute(c).await?;
                    Ok(())
                })
            })
            .connect(&url)
            .await?;
        let result = exercitar(&pool).await;
        pool.close().await;
        sqlx::query(&format!("DROP SCHEMA {schema} CASCADE"))
            .execute(&admin)
            .await?;
        result
    }

    async fn exercitar(p: &PgPool) -> Result<()> {
        sqlx::query("CREATE TABLE accounts(id BIGINT PRIMARY KEY)")
            .execute(p)
            .await?;
        sqlx::query("INSERT INTO accounts VALUES(1),(2),(3),(4)")
            .execute(p)
            .await?;
        sqlx::query("CREATE TABLE items(id INTEGER PRIMARY KEY,active BOOLEAN NOT NULL,stack_max INTEGER NOT NULL DEFAULT 99)").execute(p).await?;
        sqlx::query("INSERT INTO items(id,active) VALUES(1,TRUE),(2,TRUE),(3,FALSE)")
            .execute(p)
            .await?;
        sqlx::query("CREATE TABLE characters(name TEXT PRIMARY KEY,account_id BIGINT REFERENCES accounts(id))")
            .execute(p)
            .await?;
        sqlx::query("INSERT INTO characters VALUES('Ana',1),('Bia',2),('Caio',3),('Dani',4)")
            .execute(p)
            .await?;
        init(p).await?;
        init(p).await?; // idempotent migration
        crate::correio_admin::tests::exercitar(p).await?;
        executar(p, "Ana", &Pedido::Amizade { nome: "bia".into() }).await?;
        assert_eq!(estado(p, "Bia").await?.recebidos, vec!["Ana"]);
        assert!(executar(
            p,
            "Caio",
            &Pedido::ResponderAmizade {
                nome: "Ana".into(),
                aceitar: true
            }
        )
        .await
        .is_err());
        executar(
            p,
            "Bia",
            &Pedido::ResponderAmizade {
                nome: "Ana".into(),
                aceitar: true,
            },
        )
        .await?;
        assert_eq!(estado(p, "Ana").await?.amigos, vec!["Bia"]);
        assert_eq!(estado(p, "Bia").await?.amigos, vec!["Ana"]);
        assert!(executar(p, "Ana", &Pedido::Amizade { nome: "Ana".into() })
            .await
            .is_err());
        executar(p, "Ana", &Pedido::RemoverAmigo { nome: "Bia".into() }).await?;
        assert!(estado(p, "Bia").await?.amigos.is_empty());
        // Simultaneous reciprocal requests: one relationship only.
        let pa = Pedido::Amizade { nome: "Bia".into() };
        let pb = Pedido::Amizade { nome: "Ana".into() };
        let (a, b) = tokio::join!(executar(p, "Ana", &pa), executar(p, "Bia", &pb));
        assert_ne!(a.is_ok(), b.is_ok());
        executar(
            p,
            "Ana",
            &Pedido::EnviarCarta {
                para: "Bia".into(),
                assunto: "Olá".into(),
                texto: "Uma carta persistente.".into(),
            },
        )
        .await?;
        let carta = estado(p, "Bia").await?.cartas.remove(0);
        assert!(!carta.lida);
        assert!(estado(p, "Caio").await?.cartas.is_empty());
        assert!(executar(p, "Ana", &Pedido::LerCarta { id: carta.id })
            .await
            .is_err());
        assert!(executar(p, "Caio", &Pedido::ApagarCarta { id: carta.id })
            .await
            .is_err());
        executar(p, "Bia", &Pedido::LerCarta { id: carta.id }).await?;
        assert!(estado(p, "Bia").await?.cartas[0].lida);
        assert!(executar(
            p,
            "Ana",
            &Pedido::EnviarCarta {
                para: "Bia".into(),
                assunto: "Spam".into(),
                texto: "Outra.".into()
            }
        )
        .await
        .is_err());
        executar(p, "Bia", &Pedido::ApagarCarta { id: carta.id }).await?;
        executar(
            p,
            "Ana",
            &Pedido::CriarCla {
                nome: "Mares".into(),
            },
        )
        .await?;
        assert!(executar(
            p,
            "Bia",
            &Pedido::CriarCla {
                nome: "mArEs".into()
            }
        )
        .await
        .is_err());
        let id = estado(p, "Ana").await?.cla.unwrap().id;
        assert!(
            executar(p, "Caio", &Pedido::ResponderCla { id, aceitar: true })
                .await
                .is_err()
        );
        executar(p, "Ana", &Pedido::ConvidarCla { nome: "Bia".into() }).await?;
        executar(p, "Bia", &Pedido::ResponderCla { id, aceitar: true }).await?;
        assert!(
            executar(p, "Bia", &Pedido::ExpulsarCla { nome: "Ana".into() })
                .await
                .is_err()
        );
        assert!(executar(
            p,
            "Bia",
            &Pedido::AvisoCla {
                texto: "Forjado".into()
            }
        )
        .await
        .is_err());
        assert!(executar(p, "Ana", &Pedido::SairCla).await.is_err());
        executar(
            p,
            "Ana",
            &Pedido::AvisoCla {
                texto: "Bem-vindos!".into(),
            },
        )
        .await?;
        executar(p, "Ana", &Pedido::LiderCla { nome: "Bia".into() }).await?;
        assert_eq!(estado(p, "Ana").await?.cla.unwrap().lider, "Bia");
        assert!(executar(p, "Ana", &Pedido::DissolverCla).await.is_err());
        executar(p, "Bia", &Pedido::ExpulsarCla { nome: "Ana".into() }).await?;
        assert!(estado(p, "Ana").await?.cla.is_none());
        executar(
            p,
            "Bia",
            &Pedido::ConvidarCla {
                nome: "Caio".into(),
            },
        )
        .await?;
        executar(p, "Bia", &Pedido::DissolverCla).await?;
        assert!(estado(p, "Caio").await?.convites_cla.is_empty());
        assert!(estado(p, "Bia").await?.cla.is_none());
        // Limits checked inside the transaction; a refused letter is not inserted.
        sqlx::query("INSERT INTO social_cartas(de,para,assunto,texto,quando) SELECT 'Caio','Dani','x','y',0 FROM generate_series(1,100)").execute(p).await?;
        assert!(executar(
            p,
            "Caio",
            &Pedido::EnviarCarta {
                para: "Dani".into(),
                assunto: "Cheia".into(),
                texto: "Teste".into()
            }
        )
        .await
        .is_err());
        assert_eq!(estado(p, "Dani").await?.cartas.len(), 100);
        Ok(())
    }
}
