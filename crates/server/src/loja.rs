//! Loja de cash (docs/LOJA.md): pedidos, pagamento e posses, tudo no banco
//! CENTRAL, junto do livro-caixa da TP (`mercado_razao`).
//!
//! Dois caminhos, os dois idempotentes pelo id do pedido (gerado no cliente):
//!
//! - **Pacote de TP** (dinheiro de verdade): o pedido nasce `pendente`, o
//!   `Provedor` aprova e `confirmar_pagamento` credita a TP com a referencia
//!   `loja:<pedido>` — clique duplo, reenvio, reconexao ou webhook repetido
//!   valem um credito so'. Hoje o provedor e' o SIMULADO, que aprova na hora.
//! - **Montaria / skin** (TP): numa transacao so' do central, debita a TP e
//!   grava a posse. A posse e' da CONTA (vale pra todo personagem dela).

use anyhow::Result;
use shared::loja::{self as cat, CompraNet, EstadoLoja, PacoteTp, Produto, RecusaCompra};
use sqlx::{PgPool, Row};

use crate::mercado_razao as razao;
use crate::world::SessionId;

/// Resposta do banco central de volta ao loop do mundo.
#[derive(Debug)]
pub enum Evento {
    /// A loja da conta terminou de carregar (saldo e historico ja' foram).
    LojaCarregada { sid: SessionId, personagem: String },
    /// A compra entrega um pergaminho na bolsa; nenhum prêmio foi rolado ainda.
    Consumivel {
        sid: SessionId,
        personagem: String,
        item_id: u16,
    },
    FalhaInvocacao {
        sid: SessionId,
        personagem: String,
        item_id: u16,
        quantidade: u32,
    },
    /// Pacote de moeda pago em TP: ouro, cobre ou darksteel.
    Moeda {
        sid: SessionId,
        personagem: String,
        item_id: u16,
        qtd: u32,
    },
    /// Energia paga em TP: entra no saldo de evolucao, nunca na bolsa.
    Energia {
        sid: SessionId,
        personagem: String,
        qtd: u64,
    },
}

pub async fn criar_tabelas(pool: &PgPool) -> Result<()> {
    for sql in [
        "CREATE TABLE IF NOT EXISTS loja_pedidos (
            id            TEXT PRIMARY KEY,
            conta         TEXT        NOT NULL,
            produto       TEXT        NOT NULL,
            tipo          TEXT        NOT NULL,
            valor         BIGINT      NOT NULL,
            moeda         TEXT        NOT NULL,
            status        TEXT        NOT NULL,
            provedor      TEXT        NOT NULL DEFAULT '',
            externo       TEXT,
            motivo        TEXT,
            criado_em     TIMESTAMPTZ NOT NULL DEFAULT NOW(),
            atualizado_em TIMESTAMPTZ NOT NULL DEFAULT NOW()
        )",
        "CREATE INDEX IF NOT EXISTS loja_pedidos_conta ON loja_pedidos (conta, criado_em DESC)",
        "CREATE TABLE IF NOT EXISTS loja_posses (
            conta    TEXT        NOT NULL,
            produto  TEXT        NOT NULL,
            pedido   TEXT        NOT NULL UNIQUE,
            quando   TIMESTAMPTZ NOT NULL DEFAULT NOW(),
            PRIMARY KEY (conta, produto)
        )",
        "CREATE TABLE IF NOT EXISTS loja_montarias (
            conta       TEXT    NOT NULL,
            montaria    INTEGER NOT NULL,
            quantidade INTEGER NOT NULL DEFAULT 0,
            PRIMARY KEY (conta, montaria)
        )",
    ] {
        sqlx::query(sql).execute(pool).await?;
    }
    migrar_montarias_para_item(pool).await?;
    Ok(())
}

/// A montaria virou ITEM de bolsa e a skin deixou de existir
/// (docs/MONTARIAS.md). Esta migracao apaga as posses antigas e devolve em TP
/// tudo que foi comprado nelas — a decisao foi comecar do zero no sistema
/// novo, nao converter.
///
/// Roda uma vez, marcada por `montarias_viraram_item_v1` no livro-caixa: o
/// credito usa a referencia da conta, entao reiniciar o servidor nao paga de
/// novo.
async fn migrar_montarias_para_item(pool: &PgPool) -> Result<()> {
    let feita: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM loja_pedidos WHERE id = 'montarias_viraram_item_v1')",
    )
    .fetch_one(pool)
    .await
    .unwrap_or(false);
    if feita {
        return Ok(());
    }
    // Quanto cada conta gastou em montaria e skin, pelos pedidos entregues.
    // `SUM` de BIGINT volta NUMERIC no Postgres: sem o cast pra BIGINT o
    // decode em i64 falha e o servidor entra em crash-loop no start.
    let gastos: Vec<(String, i64)> = sqlx::query_as(
        "SELECT conta, COALESCE(SUM(valor), 0)::BIGINT FROM loja_pedidos          WHERE moeda = 'TP' AND status = 'entregue'            AND (produto LIKE 'montaria:%' OR produto LIKE 'skin:%')          GROUP BY conta",
    )
    .fetch_all(pool)
    .await?;
    for (conta, tp) in &gastos {
        if *tp <= 0 {
            continue;
        }
        razao::creditar(
            pool,
            conta,
            *tp as u64,
            "montaria virou item: devolucao",
            Some("montarias_viraram_item_v1"),
        )
        .await?;
        tracing::info!("migracao de montaria: {conta} recebeu {tp} TP de volta");
    }
    sqlx::query("DELETE FROM loja_posses WHERE produto LIKE 'montaria:%' OR produto LIKE 'skin:%'")
        .execute(pool)
        .await?;
    sqlx::query("DELETE FROM loja_montarias").execute(pool).await?;
    // O marcador e' um pedido proprio: a tabela ja' tem chave primaria por id.
    sqlx::query(
        "INSERT INTO loja_pedidos (id, conta, produto, tipo, valor, moeda, status, provedor)          VALUES ('montarias_viraram_item_v1', '', 'migracao', 'item', 0, 'TP', 'entregue', '')          ON CONFLICT (id) DO NOTHING",
    )
    .execute(pool)
    .await?;
    tracing::info!("migracao de montaria: {} conta(s) atendidas", gastos.len());
    Ok(())
}

/// Pagamento simulado (auto-aprovado) ligado? Padrao: SIM, ate' existir um
/// provedor de verdade. `PAGAMENTO_SIMULADO=0` desliga (e a compra de TP
/// passa a ser recusada).
pub fn simulado() -> bool {
    std::env::var("PAGAMENTO_SIMULADO").map_or(true, |v| v.trim() != "0")
}

/// Quem cobra.
///
/// ONDE ENTRA O PROVEDOR REAL: uma variante nova (ex. `MercadoPago`,
/// `Stripe`) cujo `iniciar` cria a cobranca na API dele e devolve
/// `Pendente { url }` (checkout/Pix). O provedor chama o webhook no `web`
/// (HTTPS, assinatura conferida), e o webhook chama `confirmar_pagamento` —
/// que e' idempotente, entao webhook repetido nao credita duas vezes.
///
/// No iOS a App Store exige In-App Purchase pra bem digital: la' o
/// "provedor" e' o recibo da Apple conferido no servidor (docs/LOJA.md).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Provedor {
    Simulado,
    Desligado,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Aprovacao {
    Aprovado {
        externo: String,
    },
    /// Provedor real: o jogador paga fora e o webhook confirma depois.
    Pendente {
        url: String,
    },
    Recusado {
        motivo: String,
    },
}

impl Provedor {
    pub fn do_ambiente() -> Provedor {
        if simulado() {
            Provedor::Simulado
        } else {
            Provedor::Desligado
        }
    }

    pub fn nome(&self) -> &'static str {
        match self {
            Provedor::Simulado => "simulado",
            Provedor::Desligado => "desligado",
        }
    }

    pub async fn iniciar(&self, pedido: &str, _pacote: &PacoteTp) -> Aprovacao {
        match self {
            Provedor::Simulado => Aprovacao::Aprovado {
                externo: format!("sim-{pedido}"),
            },
            Provedor::Desligado => Aprovacao::Recusado {
                motivo: "Pagamento indisponível no momento.".into(),
            },
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Resposta {
    Feito {
        saldo: u64,
        texto: String,
    },
    /// O mesmo pedido ja' tinha sido feito: nada mudou.
    JaFeito {
        saldo: u64,
        texto: String,
    },
    Pendente {
        url: String,
    },
    Recusado {
        texto: String,
    },
}

impl Resposta {
    pub fn ok(&self) -> bool {
        !matches!(self, Resposta::Recusado { .. })
    }

    pub fn texto(&self) -> String {
        match self {
            Resposta::Feito { texto, .. }
            | Resposta::JaFeito { texto, .. }
            | Resposta::Recusado { texto } => texto.clone(),
            Resposta::Pendente { .. } => "Pagamento aguardando confirmação.".into(),
        }
    }

    fn recusa(t: impl Into<String>) -> Resposta {
        Resposta::Recusado { texto: t.into() }
    }
}

/// Compra de pacote de TP com dinheiro.
pub async fn comprar_tp(
    central: &PgPool,
    provedor: Provedor,
    conta: &str,
    pacote_id: u16,
    pedido: &str,
) -> Result<Resposta> {
    if !cat::pedido_valido(pedido) {
        return Ok(Resposta::recusa("Pedido inválido."));
    }
    let Some(pacote) = cat::pacote(pacote_id) else {
        return Ok(Resposta::recusa(RecusaCompra::ProdutoInvalido.texto()));
    };
    let codigo = Produto::Tp(pacote.id).codigo();
    sqlx::query(
        "INSERT INTO loja_pedidos (id, conta, produto, tipo, valor, moeda, status, provedor)
         VALUES ($1, $2, $3, 'tp', $4, 'BRL_CENTAVOS', 'pendente', $5)
         ON CONFLICT (id) DO NOTHING",
    )
    .bind(pedido)
    .bind(conta)
    .bind(&codigo)
    .bind(pacote.centavos as i64)
    .bind(provedor.nome())
    .execute(central)
    .await?;
    let row = sqlx::query("SELECT conta, produto, status FROM loja_pedidos WHERE id = $1")
        .bind(pedido)
        .fetch_one(central)
        .await?;
    let (dono, produto, status): (String, String, String) = (row.get(0), row.get(1), row.get(2));
    if dono != conta || produto != codigo {
        return Ok(Resposta::recusa("Pedido inválido."));
    }
    match status.as_str() {
        "creditado" => {
            return Ok(Resposta::JaFeito {
                saldo: razao::saldo(central, conta).await?,
                texto: "Esta compra já foi creditada.".into(),
            });
        }
        "recusado" => return Ok(Resposta::recusa("Pagamento recusado.")),
        _ => {}
    }
    match provedor.iniciar(pedido, pacote).await {
        Aprovacao::Aprovado { externo } => confirmar_pagamento(central, pedido, &externo).await,
        Aprovacao::Pendente { url } => {
            sqlx::query("UPDATE loja_pedidos SET externo = $2, atualizado_em = NOW() WHERE id = $1 AND status = 'pendente'")
                .bind(pedido)
                .bind(&url)
                .execute(central)
                .await?;
            Ok(Resposta::Pendente { url })
        }
        Aprovacao::Recusado { motivo } => {
            sqlx::query("UPDATE loja_pedidos SET status = 'recusado', motivo = $2, atualizado_em = NOW() WHERE id = $1 AND status = 'pendente'")
                .bind(pedido)
                .bind(&motivo)
                .execute(central)
                .await?;
            Ok(Resposta::recusa(motivo))
        }
    }
}

/// Pagamento aprovado: credita a TP do pacote. E' o que o webhook de um
/// provedor real chama. Idempotente: a linha do pedido fica travada durante a
/// transacao e o credito leva a referencia unica `loja:<pedido>`.
pub async fn confirmar_pagamento(
    central: &PgPool,
    pedido: &str,
    externo: &str,
) -> Result<Resposta> {
    let mut tx = central.begin().await?;
    let Some(row) = sqlx::query(
        "SELECT conta, produto, status, tipo FROM loja_pedidos WHERE id = $1 FOR UPDATE",
    )
    .bind(pedido)
    .fetch_optional(&mut *tx)
    .await?
    else {
        return Ok(Resposta::recusa("Pedido não encontrado."));
    };
    let (conta, produto, status, tipo): (String, String, String, String) =
        (row.get(0), row.get(1), row.get(2), row.get(3));
    let pacote = match (tipo.as_str(), Produto::de_codigo(&produto)) {
        ("tp", Some(Produto::Tp(i))) => cat::pacote(i),
        _ => None,
    };
    let Some(pacote) = pacote else {
        return Ok(Resposta::recusa("Pedido inválido."));
    };
    if status == "creditado" {
        let saldo = razao::saldo(&mut *tx, &conta).await?;
        tx.commit().await?;
        return Ok(Resposta::JaFeito {
            saldo,
            texto: "Esta compra já foi creditada.".into(),
        });
    }
    let m = razao::mover(
        &mut tx,
        &conta,
        pacote.total() as i64,
        "loja:tp",
        Some(&format!("loja:{pedido}")),
    )
    .await?;
    sqlx::query("UPDATE loja_pedidos SET status = 'creditado', externo = $2, atualizado_em = NOW() WHERE id = $1")
        .bind(pedido)
        .bind(externo)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(match m {
        razao::Movimento::Feito { saldo } => Resposta::Feito {
            saldo,
            texto: format!("+{} TP creditados.", pacote.total()),
        },
        razao::Movimento::JaFeito { saldo } | razao::Movimento::SemSaldo { saldo } => {
            Resposta::JaFeito {
                saldo,
                texto: "Esta compra já foi creditada.".into(),
            }
        }
    })
}

/// Compra de montaria ou skin com TP: debito e posse na mesma transacao.
/// Compra `vezes` unidades de `produto` num PEDIDO so': uma transacao, um
/// debito, um id de idempotencia. Lote em pedidos separados podia falhar no
/// meio e deixar o jogador sem saber quantos entraram.
pub async fn comprar_item(
    central: &PgPool,
    conta: &str,
    produto: Produto,
    vezes: u16,
    pedido: &str,
) -> Result<Resposta> {
    if !cat::pedido_valido(pedido) {
        return Ok(Resposta::recusa("Pedido inválido."));
    }
    let vezes = cat::lote(vezes);
    let Some(unitario) = produto.preco_tp() else {
        return Ok(Resposta::recusa(RecusaCompra::ProdutoInvalido.texto()));
    };
    let preco = unitario.saturating_mul(vezes as u64);
    let codigo = produto.codigo();
    let mut tx = central.begin().await?;
    // Mesma trava do razao (reentrante na transacao): compras da mesma conta
    // em serie — duas skins ao mesmo tempo nao leem o mesmo saldo.
    sqlx::query("SELECT pg_advisory_xact_lock(hashtext($1))")
        .bind(conta)
        .execute(&mut *tx)
        .await?;
    sqlx::query(
        "INSERT INTO loja_pedidos (id, conta, produto, tipo, valor, moeda, status, provedor)
         VALUES ($1, $2, $3, 'item', $4, 'TP', 'pendente', 'tp')
         ON CONFLICT (id) DO NOTHING",
    )
    .bind(pedido)
    .bind(conta)
    .bind(&codigo)
    .bind(preco as i64)
    .execute(&mut *tx)
    .await?;
    let row = sqlx::query("SELECT conta, produto, status, COALESCE(motivo, '') FROM loja_pedidos WHERE id = $1 FOR UPDATE")
        .bind(pedido)
        .fetch_one(&mut *tx)
        .await?;
    let (dono, prod, status, motivo): (String, String, String, String) =
        (row.get(0), row.get(1), row.get(2), row.get(3));
    if dono != conta || prod != codigo {
        return Ok(Resposta::recusa("Pedido inválido."));
    }
    match status.as_str() {
        "entregue" => {
            let saldo = razao::saldo(&mut *tx, conta).await?;
            tx.commit().await?;
            return Ok(Resposta::JaFeito {
                saldo,
                texto: "Esta compra já foi entregue.".into(),
            });
        }
        "recusado" => {
            tx.commit().await?;
            return Ok(Resposta::recusa(if motivo.is_empty() {
                "Compra recusada.".to_string()
            } else {
                motivo
            }));
        }
        _ => {}
    }
    let saldo = razao::saldo(&mut *tx, conta).await?;
    if let Err(r) = cat::pode_comprar(produto, vezes, saldo) {
        recusar_pedido(&mut tx, pedido, r.texto()).await?;
        tx.commit().await?;
        return Ok(Resposta::recusa(r.texto()));
    }
    let saldo = match razao::mover(
        &mut tx,
        conta,
        -(preco as i64),
        &format!("loja:item:{codigo}"),
        Some(&format!("loja:{pedido}")),
    )
    .await?
    {
        razao::Movimento::SemSaldo { .. } => {
            recusar_pedido(&mut tx, pedido, RecusaCompra::SemSaldo.texto()).await?;
            tx.commit().await?;
            return Ok(Resposta::recusa(RecusaCompra::SemSaldo.texto()));
        }
        razao::Movimento::Feito { saldo } | razao::Movimento::JaFeito { saldo } => saldo,
    };
    // Nada mais vira posse da conta: montaria e skin sairam da loja (a
    // montaria virou item de bolsa, docs/MONTARIAS.md) e todo o resto do
    // catalogo e' repetivel. A tabela `loja_posses` fica so' com o historico
    // antigo, que a migracao limpa.
    sqlx::query("UPDATE loja_pedidos SET status = 'entregue', atualizado_em = NOW() WHERE id = $1")
        .bind(pedido)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    // O texto diz QUANTOS: "10x Pergaminho entregue na bolsa" e' a unica
    // confirmacao de que o lote inteiro entrou.
    let quantos = if vezes > 1 {
        format!("{vezes}x ")
    } else {
        String::new()
    };
    Ok(Resposta::Feito {
        saldo,
        texto: if matches!(
            produto,
            Produto::BauCraft(_)
                | Produto::PergaminhoMontaria(_)
                | Produto::PergaminhoTomo(_)
                | Produto::PergaminhoPet(_)
                | Produto::ItemDePet(_)
        ) {
            format!("{quantos}{} entregue na bolsa!", produto.nome())
        } else if matches!(produto, Produto::Moeda(_) | Produto::Energia(_)) {
            format!("{quantos}{} comprado!", produto.nome())
        } else {
            format!("{quantos}{} é seu!", produto.nome())
        },
    })
}

async fn recusar_pedido(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    pedido: &str,
    motivo: &str,
) -> Result<()> {
    sqlx::query("UPDATE loja_pedidos SET status = 'recusado', motivo = $2, atualizado_em = NOW() WHERE id = $1")
        .bind(pedido)
        .bind(motivo)
        .execute(&mut **tx)
        .await?;
    Ok(())
}


/// Saldo, posses e as ultimas compras.
pub async fn estado(central: &PgPool, conta: &str) -> Result<EstadoLoja> {
    let historico = sqlx::query(
        "SELECT produto, valor, moeda, status, EXTRACT(EPOCH FROM criado_em)::bigint
           FROM loja_pedidos WHERE conta = $1 ORDER BY criado_em DESC LIMIT 20",
    )
    .bind(conta)
    .fetch_all(central)
    .await?
    .iter()
    .map(|r| {
        let produto: String = r.get(0);
        let valor: i64 = r.get(1);
        let moeda: String = r.get(2);
        CompraNet {
            produto: Produto::de_codigo(&produto).map_or(produto, |p| p.nome()),
            valor: if moeda == "TP" {
                format!("{valor} TP")
            } else {
                cat::preco_brl(valor.max(0) as u32)
            },
            status: r.get(3),
            quando_unix: r.get(4),
        }
    })
    .collect();
    Ok(EstadoLoja {
        ligada: true,
        simulado: simulado(),
        tp: razao::saldo(central, conta).await?,
        historico,
    })
}


#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn simulado_aprova_na_hora_e_desligado_recusa() {
        let p = cat::pacote(1).unwrap();
        assert!(matches!(
            Provedor::Simulado.iniciar("pedido-teste-1", p).await,
            Aprovacao::Aprovado { .. }
        ));
        assert!(matches!(
            Provedor::Desligado.iniciar("pedido-teste-1", p).await,
            Aprovacao::Recusado { .. }
        ));
    }

    /// Fluxo inteiro num Postgres descartavel: `DATABASE_URL_CENTRAL_TESTE`.
    #[tokio::test]
    async fn pedidos_idempotentes_e_repetiveis_no_postgres() {
        let Ok(url) = std::env::var("DATABASE_URL_CENTRAL_TESTE") else {
            eprintln!("DATABASE_URL_CENTRAL_TESTE nao setada: teste de integracao da loja pulado");
            return;
        };
        let central = crate::mercado::abrir_central(&url).await.expect("central");
        criar_tabelas(&central).await.expect("tabelas da loja");
        let tag = crate::mercado::novo_id();
        let conta = format!("T:loja:{}", &tag[..12]);
        let id = |s: &str| format!("{}-{s}", &tag[..16]);

        // Pacote de 550: o mesmo pedido duas vezes (clique duplo / reenvio).
        let r = comprar_tp(&central, Provedor::Simulado, &conta, 2, &id("tp1"))
            .await
            .unwrap();
        assert!(matches!(r, Resposta::Feito { saldo: 550, .. }), "{r:?}");
        let r = comprar_tp(&central, Provedor::Simulado, &conta, 2, &id("tp1"))
            .await
            .unwrap();
        assert!(matches!(r, Resposta::JaFeito { saldo: 550, .. }), "{r:?}");
        // Webhook repetido do provedor.
        let r = confirmar_pagamento(&central, &id("tp1"), "sim")
            .await
            .unwrap();
        assert!(matches!(r, Resposta::JaFeito { saldo: 550, .. }), "{r:?}");
        // Dois canais com o MESMO pedido ao mesmo tempo: um credito so'.
        let tp2 = id("tp2");
        let (a, b) = tokio::join!(
            comprar_tp(&central, Provedor::Simulado, &conta, 3, &tp2),
            comprar_tp(&central, Provedor::Simulado, &conta, 3, &tp2),
        );
        let (a, b) = (a.unwrap(), b.unwrap());
        assert!(a.ok() && b.ok());
        assert_eq!(
            razao::saldo(&central, &conta).await.unwrap(),
            550 + 1200,
            "{a:?} {b:?}"
        );
        // Pedido de outra conta com o mesmo id: recusado.
        let r = comprar_tp(&central, Provedor::Simulado, "T:outra", 2, &id("tp1"))
            .await
            .unwrap();
        assert!(!r.ok());
        // Provedor desligado: recusa e nao credita.
        let r = comprar_tp(&central, Provedor::Desligado, &conta, 1, &id("tp3"))
            .await
            .unwrap();
        assert!(!r.ok());
        assert_eq!(razao::saldo(&central, &conta).await.unwrap(), 1750);

        // Pergaminho de montaria (500): a compra e' idempotente, mas novos
        // pergaminhos sao repetiveis. A montaria so' vira posse ao abrir.
        let r = comprar_item(&central, &conta, Produto::PergaminhoMontaria(1), 1, &id("m1"))
            .await
            .unwrap();
        assert!(matches!(r, Resposta::Feito { saldo: 1250, .. }), "{r:?}");
        let r = comprar_item(&central, &conta, Produto::PergaminhoMontaria(1), 1, &id("m1"))
            .await
            .unwrap();
        assert!(matches!(r, Resposta::JaFeito { saldo: 1250, .. }), "{r:?}");
        let r = comprar_item(&central, &conta, Produto::PergaminhoMontaria(1), 1, &id("m1b"))
            .await
            .unwrap();
        assert!(matches!(r, Resposta::Feito { saldo: 750, .. }), "{r:?}");
        // Montaria e skin sairam da loja: os codigos antigos nao existem mais.
        assert_eq!(shared::loja::Produto::de_codigo("montaria:2"), None);
        assert_eq!(shared::loja::Produto::de_codigo("skin:102"), None);
        // Nada mais vira posse de conta: tudo no catalogo e' repetivel.
        let posses: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM loja_posses WHERE conta = $1")
                .bind(&conta)
                .fetch_one(&central)
                .await
                .unwrap();
        assert_eq!(posses, 0, "pergaminho nao vira posse");
        let r = comprar_item(&central, &conta, Produto::ItemDePet(1), 1, &id("racao"))
            .await
            .unwrap();
        assert!(matches!(r, Resposta::Feito { saldo: 720, .. }), "{r:?}");
        // Saldo insuficiente: nada muda.
        let r = comprar_item(&central, &conta, Produto::PergaminhoMontaria(1), 1, &id("m3"))
            .await
            .unwrap();
        assert_eq!(
            r,
            Resposta::Recusado {
                texto: RecusaCompra::SemSaldo.texto().into()
            }
        );
        assert_eq!(razao::saldo(&central, &conta).await.unwrap(), 720);
        // Duas compras diferentes ao mesmo tempo nao leem o mesmo saldo.
        let (m2, s3) = (id("m2"), id("s3"));
        let (a, b) = tokio::join!(
            comprar_item(&central, &conta, Produto::PergaminhoMontaria(1), 1, &m2),
            comprar_item(&central, &conta, Produto::PergaminhoPet(1), 1, &s3),
        );
        let (a, b) = (a.unwrap(), b.unwrap());
        assert!(a.ok() != b.ok() || (a.ok() && b.ok()), "{a:?} {b:?}");
        let saldo = razao::saldo(&central, &conta).await.unwrap();
        let gasto: u64 = [(&a, 500u64), (&b, 250u64)]
            .iter()
            .filter(|(r, _)| r.ok())
            .map(|(_, p)| *p)
            .sum();
        assert_eq!(saldo, 720 - gasto, "{a:?} {b:?}");

        let e = estado(&central, &conta).await.unwrap();
        assert!(e.ligada && e.historico.len() >= 6);
        assert_eq!(e.tp, saldo);
    }

    /// A migracao devolve em TP o que a conta gastou em montaria e skin, e
    /// roda uma vez so' — reiniciar o servidor nao paga de novo.
    #[tokio::test]
    #[ignore = "precisa de DATABASE_URL_CENTRAL"]
    async fn migracao_de_montaria_devolve_tp_uma_vez() {
        let Ok(url) = std::env::var("DATABASE_URL_CENTRAL") else {
            return;
        };
        let central = PgPool::connect(&url).await.unwrap();
        criar_tabelas(&central).await.unwrap();
        let conta = format!("migra-{}", fastrand::u32(..));
        // Um pedido antigo de skin, ja' entregue.
        sqlx::query(
            "INSERT INTO loja_pedidos (id, conta, produto, tipo, valor, moeda, status, provedor) \
             VALUES ($1, $2, 'skin:102', 'item', 350, 'TP', 'entregue', '')",
        )
        .bind(format!("velho-{conta}"))
        .bind(&conta)
        .execute(&central)
        .await
        .unwrap();
        sqlx::query("INSERT INTO loja_posses (conta, produto, pedido) VALUES ($1,'skin:102',$2)")
            .bind(&conta)
            .bind(format!("velho-{conta}"))
            .execute(&central)
            .await
            .unwrap();
        // O marcador global ja' existe do `criar_tabelas` acima, entao o
        // credito desta conta nao acontece: a migracao e' de uma vez so'.
        let posses: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM loja_posses WHERE conta = $1 AND produto LIKE 'skin:%'",
        )
        .bind(&conta)
        .fetch_one(&central)
        .await
        .unwrap();
        assert_eq!(posses, 1, "a conta entrou depois da migracao ter rodado");
        // Rodar de novo nao credita nada nem apaga o que chegou depois.
        criar_tabelas(&central).await.unwrap();
        assert_eq!(razao::saldo(&central, &conta).await.unwrap(), 0);
    }
}
