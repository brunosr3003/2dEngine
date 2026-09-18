//! Correio oficial: permissao por conta, campanha auditavel, destinatarios
//! fotografados na transacao e anexos entregues com recibo no save da bolsa.
use crate::world::{IncomingMessage, SessionId};
use anyhow::{bail, Result};
use shared::social::{anexos_validos, texto_valido, Anexo, Carta, Pedido};
use sqlx::{Connection, PgConnection, PgPool, Postgres, Row, Transaction};
use tokio::sync::{mpsc, oneshot};

pub async fn init(pool: &PgPool) -> Result<()> {
    for sql in [
        "CREATE TABLE IF NOT EXISTS social_staff (account_id BIGINT PRIMARY KEY REFERENCES accounts(id), cargo TEXT NOT NULL CHECK(cargo IN ('admin','mod')))",
        "CREATE TABLE IF NOT EXISTS social_campanhas (id BIGSERIAL PRIMARY KEY, envio TEXT NOT NULL, autor TEXT NOT NULL, account_id BIGINT NOT NULL, cargo TEXT NOT NULL, destino TEXT, assunto TEXT NOT NULL, texto TEXT NOT NULL, anexos TEXT NOT NULL, quando BIGINT NOT NULL, destinatarios INTEGER NOT NULL DEFAULT 0, UNIQUE(account_id,envio))",
        "CREATE TABLE IF NOT EXISTS social_oficiais (id BIGSERIAL PRIMARY KEY, campanha BIGINT NOT NULL REFERENCES social_campanhas(id), para TEXT NOT NULL REFERENCES characters(name), lida BOOLEAN NOT NULL DEFAULT FALSE, apagada BOOLEAN NOT NULL DEFAULT FALSE, resgatada BOOLEAN NOT NULL DEFAULT FALSE, token TEXT, UNIQUE(campanha,para))",
        "CREATE INDEX IF NOT EXISTS social_oficiais_caixa ON social_oficiais(para,id DESC)",
    ] { sqlx::query(sql).execute(pool).await?; }
    Ok(())
}

pub async fn cargo(db: &mut PgConnection, nome: &str) -> Result<Option<String>> {
    Ok(sqlx::query_scalar("SELECT s.cargo FROM social_staff s JOIN characters c ON c.account_id=s.account_id WHERE c.name=$1")
        .bind(nome).fetch_optional(db).await?)
}

pub async fn enviar(pool: &PgPool, autor: &str, conta: i64, pedido: &Pedido) -> Result<u32> {
    let Pedido::EnviarOficial {
        envio,
        para,
        assunto,
        texto,
        anexos,
    } = pedido
    else {
        bail!("Pedido inválido.");
    };
    if envio.len() < 8
        || envio.len() > 100
        || !envio.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
    {
        bail!("Identificador de envio inválido.");
    }
    if !texto_valido(assunto, 1, 60) || !texto_valido(texto, 1, 1000) || !anexos_validos(anexos) {
        bail!(
            "Revise assunto, texto e anexos (até 8 itens distintos, de 1 a 100.000 unidades cada)."
        );
    }
    let mut tx = pool.begin().await?;
    // Serializa envios por conta (idempotencia inclusive entre canais).
    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended('correio-admin:' || $1::text,0))")
        .bind(conta)
        .execute(&mut *tx)
        .await?;
    let cargo: Option<String>=sqlx::query_scalar("SELECT s.cargo FROM social_staff s JOIN characters c ON c.account_id=s.account_id WHERE c.name=$1 AND c.account_id=$2 FOR SHARE OF s")
        .bind(autor).bind(conta).fetch_optional(&mut *tx).await?;
    let Some(cargo) = cargo else {
        bail!("Somente administradores e moderadores podem enviar correio oficial.");
    };
    let payload = serde_json::to_string(anexos)?;
    if let Some(r)=sqlx::query("SELECT destino,assunto,texto,anexos,destinatarios FROM social_campanhas WHERE account_id=$1 AND envio=$2")
        .bind(conta).bind(envio).fetch_optional(&mut *tx).await? {
        if r.get::<Option<String>,_>("destino")!=*para || r.get::<String,_>("assunto")!=*assunto || r.get::<String,_>("texto")!=*texto || r.get::<String,_>("anexos")!=payload {bail!("Identificador já usado em outro envio. Crie uma nova carta.");}
        return Ok(r.get::<i32,_>("destinatarios") as u32);
    }
    let mut slots_necessarios = 0u64;
    for a in anexos {
        let stack: Option<i32> =
            sqlx::query_scalar("SELECT stack_max FROM items WHERE id=$1 AND active")
                .bind(a.item_id as i32)
                .fetch_optional(&mut *tx)
                .await?;
        let Some(stack) = stack else {
            bail!("Item {} não existe ou está desativado.", a.item_id);
        };
        slots_necessarios += (a.qtd as u64).div_ceil(stack.max(1) as u64);
    }
    if slots_necessarios > shared::INVENTORY_SLOTS as u64 {
        bail!("Os anexos não cabem nem numa bolsa vazia. Reduza as quantidades ou separe em mais cartas.");
    }
    let alvo = if let Some(nome) = para {
        if !texto_valido(nome, 1, 32) {
            bail!("Nome inválido.");
        }
        let nomes: Vec<String> =
            sqlx::query_scalar("SELECT name FROM characters WHERE lower(name)=lower($1) LIMIT 2")
                .bind(nome.trim())
                .fetch_all(&mut *tx)
                .await?;
        match nomes.as_slice() {
            [n] => Some(n.clone()),
            _ => bail!("Personagem não encontrado ou nome ambíguo."),
        }
    } else {
        None
    };
    let id:i64=sqlx::query_scalar("INSERT INTO social_campanhas(envio,autor,account_id,cargo,destino,assunto,texto,anexos,quando) VALUES($1,$2,$3,$4,$5,$6,$7,$8,extract(epoch FROM now())::bigint) RETURNING id")
        .bind(envio).bind(autor).bind(conta).bind(cargo).bind(para).bind(assunto).bind(texto).bind(payload).fetch_one(&mut *tx).await?;
    let n=sqlx::query("INSERT INTO social_oficiais(campanha,para) SELECT $1,name FROM characters WHERE ($2::text IS NULL OR name=$2)")
        .bind(id).bind(alvo).execute(&mut *tx).await?.rows_affected();
    sqlx::query("UPDATE social_campanhas SET destinatarios=$1 WHERE id=$2")
        .bind(n as i32)
        .bind(id)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    tracing::info!("correio oficial campanha={id} autor={autor} destinatarios={n}");
    Ok(n as u32)
}

pub async fn listar(db: &mut PgConnection, eu: &str) -> Result<Vec<Carta>> {
    let rs=sqlx::query("SELECT o.id,o.lida,o.resgatada,c.autor,c.cargo,c.assunto,c.texto,c.anexos,c.quando FROM social_oficiais o JOIN social_campanhas c ON c.id=o.campanha WHERE o.para=$1 AND NOT o.apagada ORDER BY o.resgatada,o.id DESC LIMIT 500")
        .bind(eu).fetch_all(db).await?;
    rs.into_iter()
        .map(|r| {
            Ok(Carta {
                id: -r.get::<i64, _>("id"),
                de: format!(
                    "Equipe · {} ({})",
                    r.get::<String, _>("autor"),
                    r.get::<String, _>("cargo")
                ),
                assunto: r.get("assunto"),
                texto: r.get("texto"),
                quando: r.get("quando"),
                lida: r.get("lida"),
                oficial: true,
                anexos: serde_json::from_str(&r.get::<String, _>("anexos"))?,
                resgatada: r.get("resgatada"),
            })
        })
        .collect()
}

pub async fn ler_apagar(pool: &PgPool, eu: &str, id: i64, apagar: bool) -> Result<()> {
    let Some(id) = id.checked_neg().filter(|n| *n > 0) else {
        bail!("Carta inválida.");
    };
    let sql = if apagar {
        "UPDATE social_oficiais o SET apagada=TRUE FROM social_campanhas c WHERE o.id=$1 AND o.para=$2 AND c.id=o.campanha AND (o.resgatada OR c.anexos='[]')"
    } else {
        "UPDATE social_oficiais SET lida=TRUE WHERE id=$1 AND para=$2 AND NOT apagada"
    };
    if sqlx::query(sql)
        .bind(id)
        .bind(eu)
        .execute(pool)
        .await?
        .rows_affected()
        == 0
    {
        bail!("Carta indisponível. Receba os anexos antes de apagar.");
    }
    Ok(())
}

/// Recibo e bolsa sao confirmados juntos. Token impede um save antigo de
/// confirmar uma reserva de outro processo apos perda da conexao.
#[derive(Debug, Clone)]
pub struct Recibo {
    pub id: i64,
    pub token: String,
}
pub async fn marcar(
    tx: &mut Transaction<'_, Postgres>,
    nome: &str,
    recibos: &[Recibo],
) -> Result<()> {
    for r in recibos {
        let n=sqlx::query("UPDATE social_oficiais SET resgatada=TRUE,lida=TRUE WHERE id=$1 AND para=$2 AND token=$3")
            .bind(r.id).bind(nome).bind(&r.token).execute(&mut **tx).await?.rows_affected();
        if n != 1 {
            bail!(
                "Reserva de anexo mudou; save recusado para evitar duplicação (carta {}).",
                r.id
            );
        }
    }
    Ok(())
}

pub struct Entrega {
    pub sid: SessionId,
    pub nome: String,
    pub recibo: Recibo,
    pub anexos: Vec<Anexo>,
    pub aceitou: oneshot::Sender<bool>,
}

/// Conexao dedicada segura advisory lock ate o save confirmar o recibo.
/// Sem prazo arbitrario: banco lento nao permite um segundo resgate.
pub fn resgatar(
    pool: PgPool,
    tx: mpsc::UnboundedSender<IncomingMessage>,
    sid: SessionId,
    nome: String,
    id: i64,
) {
    tokio::spawn(async move {
        let result = resgatar_inner(&pool, &tx, sid, &nome, id).await;
        let texto = match result {
            Ok(()) => "Anexos recebidos e salvos.".to_string(),
            Err(e) => {
                if e.downcast_ref::<sqlx::Error>().is_some() {
                    tracing::error!("correio resgate: {e:#}");
                    "Não foi possível concluir o resgate. Tente novamente.".into()
                } else {
                    e.to_string()
                }
            }
        };
        let _ = tx.send(IncomingMessage::CorreioFim { sid, nome, texto });
    });
}
async fn resgatar_inner(
    pool: &PgPool,
    tx: &mpsc::UnboundedSender<IncomingMessage>,
    sid: SessionId,
    nome: &str,
    id: i64,
) -> Result<()> {
    let Some(id) = id.checked_neg().filter(|n| *n > 0) else {
        bail!("Carta inválida.");
    };
    let mut conn = PgConnection::connect_with(&pool.connect_options()).await?;
    let locked: bool = sqlx::query_scalar(
        "SELECT pg_try_advisory_lock(hashtextextended('social-carta:' || $1::text,0))",
    )
    .bind(id)
    .fetch_one(&mut conn)
    .await?;
    if !locked {
        bail!("Este resgate já está sendo processado.");
    }
    let r=sqlx::query("SELECT c.anexos FROM social_oficiais o JOIN social_campanhas c ON c.id=o.campanha WHERE o.id=$1 AND o.para=$2 AND NOT o.resgatada AND NOT o.apagada")
        .bind(id).bind(nome).fetch_optional(&mut conn).await?;
    let Some(r) = r else {
        bail!("Carta já resgatada ou indisponível.");
    };
    let anexos: Vec<Anexo> = serde_json::from_str(&r.get::<String, _>("anexos"))?;
    if anexos.is_empty() {
        bail!("Esta carta não tem anexos.");
    }
    let token = format!(
        "{}-{}-{}",
        std::process::id(),
        crate::presenca::agora(),
        fastrand::u64(..)
    );
    // Reconfere sob o lock da linha: um save antigo pode concluir entre
    // a leitura acima e esta reserva, mesmo apos perder o advisory lock.
    let reservou = sqlx::query("UPDATE social_oficiais SET token=$1 WHERE id=$2 AND para=$3 AND NOT resgatada AND NOT apagada")
        .bind(&token)
        .bind(id)
        .bind(nome)
        .execute(&mut conn)
        .await?.rows_affected();
    if reservou != 1 {
        bail!("Carta já resgatada ou indisponível.");
    }
    let (ack, rx) = oneshot::channel();
    tx.send(IncomingMessage::CorreioEntrega(Entrega {
        sid,
        nome: nome.into(),
        recibo: Recibo { id, token },
        anexos,
        aceitou: ack,
    }))?;
    if !rx.await.unwrap_or(false) {
        bail!("Libere espaço na bolsa e tente novamente.");
    }
    loop {
        let salvo: bool = sqlx::query_scalar("SELECT resgatada FROM social_oficiais WHERE id=$1")
            .bind(id)
            .fetch_one(&mut conn)
            .await?;
        if salvo {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(250)).await;
    }
    conn.close().await?;
    Ok(())
}

#[cfg(test)]
pub mod tests {
    use super::*;
    pub async fn exercitar(p: &PgPool) -> Result<()> {
        let pedido = Pedido::EnviarOficial {
            envio: "teste-oficial-1".into(),
            para: None,
            assunto: "Evento".into(),
            texto: "Recompensa de teste".into(),
            anexos: vec![Anexo { item_id: 1, qtd: 3 }, Anexo { item_id: 2, qtd: 4 }],
        };
        assert!(enviar(p, "Ana", 1, &pedido).await.is_err());
        sqlx::query("INSERT INTO social_staff VALUES(1,'admin'),(2,'mod')")
            .execute(p)
            .await?;
        assert!(
            enviar(p, "Caio", 1, &pedido).await.is_err(),
            "conta precisa pertencer ao personagem autenticado"
        );
        let (a, b) = tokio::join!(enviar(p, "Ana", 1, &pedido), enviar(p, "Ana", 1, &pedido));
        assert_eq!(a?, 4);
        assert_eq!(b?, 4);
        let campanhas: i64 = sqlx::query_scalar("SELECT count(*) FROM social_campanhas")
            .fetch_one(p)
            .await?;
        assert_eq!(campanhas, 1, "reenvio nao duplica campanha");
        let mut conn = p.acquire().await?;
        let cartas = listar(&mut conn, "Caio").await?;
        assert_eq!(cartas.len(), 1);
        assert_eq!(cartas[0].anexos.len(), 2);
        assert!(
            ler_apagar(p, "Caio", cartas[0].id, true).await.is_err(),
            "nao apaga anexos pendentes"
        );
        assert!(ler_apagar(p, "Dani", cartas[0].id, false).await.is_err());
        let mut mod_p = pedido.clone();
        if let Pedido::EnviarOficial { envio, para, .. } = &mut mod_p {
            *envio = "teste-moderador-1".into();
            *para = Some("Caio".into());
        }
        assert_eq!(enviar(p, "Bia", 2, &mod_p).await?, 1);
        if let Pedido::EnviarOficial { texto, .. } = &mut mod_p {
            *texto = "Outro conteudo".into();
        }
        assert!(
            enviar(p, "Bia", 2, &mod_p).await.is_err(),
            "mesma chave com payload diferente rejeitada"
        );
        let mut invalido = pedido.clone();
        if let Pedido::EnviarOficial { envio, anexos, .. } = &mut invalido {
            *envio = "teste-invalido-1".into();
            anexos[0].item_id = 3;
        }
        assert!(
            enviar(p, "Ana", 1, &invalido).await.is_err(),
            "item desativado rejeitado"
        );
        sqlx::query("DELETE FROM social_staff WHERE account_id=2")
            .execute(p)
            .await?;
        assert!(
            enviar(p, "Bia", 2, &pedido).await.is_err(),
            "revogacao imediata"
        );
        // O recibo participa da transacao: rollback nao consome o anexo.
        let id = -cartas[0].id;
        sqlx::query("UPDATE social_oficiais SET token='teste-token' WHERE id=$1")
            .bind(id)
            .execute(p)
            .await?;
        let recibo = Recibo {
            id,
            token: "teste-token".into(),
        };
        let mut tx = p.begin().await?;
        marcar(&mut tx, "Caio", std::slice::from_ref(&recibo)).await?;
        tx.rollback().await?;
        assert!(!listar(&mut conn, "Caio").await?[1].resgatada);
        let mut tx = p.begin().await?;
        assert!(marcar(&mut tx, "Dani", std::slice::from_ref(&recibo))
            .await
            .is_err());
        tx.rollback().await?;
        let mut tx = p.begin().await?;
        assert!(marcar(
            &mut tx,
            "Caio",
            &[Recibo {
                id,
                token: "token-antigo".into()
            }]
        )
        .await
        .is_err());
        tx.rollback().await?;
        let mut tx = p.begin().await?;
        marcar(&mut tx, "Caio", std::slice::from_ref(&recibo)).await?;
        tx.commit().await?;
        assert!(
            listar(&mut conn, "Caio")
                .await?
                .iter()
                .find(|c| c.id == -id)
                .unwrap()
                .resgatada
        );
        ler_apagar(p, "Caio", -id, true).await?;
        // Exerce a reserva real, com duas conexoes e retorno pelo canal do mundo.
        let schema: String = sqlx::query_scalar("SELECT current_schema()")
            .fetch_one(p)
            .await?;
        let opts = (*p.connect_options())
            .clone()
            .options([("search_path", schema)]);
        let scoped = PgPool::connect_with(opts).await?;
        let id_bia = listar(&mut conn, "Bia").await?[0].id;
        let sid = SessionId("127.0.0.1:19876".parse()?);
        let (out, mut incoming) = mpsc::unbounded_channel();
        let p2 = scoped.clone();
        let tx2 = out.clone();
        let task = tokio::spawn(async move { resgatar_inner(&p2, &tx2, sid, "Bia", id_bia).await });
        let ev = tokio::time::timeout(std::time::Duration::from_secs(3), incoming.recv())
            .await?
            .unwrap();
        let IncomingMessage::CorreioEntrega(entrega) = ev else {
            panic!("entrega esperada");
        };
        assert_eq!(entrega.anexos.len(), 2);
        assert!(
            resgatar_inner(&scoped, &out, sid, "Bia", id_bia)
                .await
                .is_err(),
            "segunda conexao nao resgata durante reserva"
        );
        let _ = entrega.aceitou.send(false); // bolsa cheia ou jogador saiu
        assert!(task.await?.is_err());
        assert!(!listar(&mut conn, "Bia").await?[0].resgatada);
        let p2 = scoped.clone();
        let tx2 = out.clone();
        let task = tokio::spawn(async move { resgatar_inner(&p2, &tx2, sid, "Bia", id_bia).await });
        let ev = tokio::time::timeout(std::time::Duration::from_secs(3), incoming.recv())
            .await?
            .unwrap();
        let IncomingMessage::CorreioEntrega(entrega) = ev else {
            panic!("entrega esperada");
        };
        let _ = entrega.aceitou.send(true);
        let mut save = scoped.begin().await?;
        marcar(&mut save, "Bia", &[entrega.recibo]).await?;
        save.commit().await?;
        tokio::time::timeout(std::time::Duration::from_secs(3), task).await???;
        assert!(
            resgatar_inner(&scoped, &out, sid, "Bia", id_bia)
                .await
                .is_err(),
            "anexo salvo nao pode ser resgatado novamente"
        );
        scoped.close().await;
        // Esvazia apenas registros de teste para a suite social existente.
        sqlx::query("DELETE FROM social_oficiais")
            .execute(p)
            .await?;
        sqlx::query("DELETE FROM social_campanhas")
            .execute(p)
            .await?;
        Ok(())
    }
}
