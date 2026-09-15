//! O painel da economia: o que o desenho PREVE e o que o banco MEDE.
//!
//! Os dois lado a lado de proposito. A economia foi desenhada em cima de
//! numeros fechados — "Raro +7 custa 106 horas de mineracao" — e o unico jeito
//! de saber se ela esta' fazendo o que devia e' comparar a previsao com o que
//! esta' acontecendo. Um painel so' com a previsao e' a planilha de novo; um
//! painel so' com a medicao nao diz se o numero e' alto ou baixo.
//!
//! O que e' PREVISAO sai de `shared::forja`, a mesma funcao que o jogo usa
//! pra refinar. O que e' MEDIDA sai do Postgres. Nada aqui e' recalculado com
//! formula propria — painel com a sua versao da regra vira uma segunda regra.

use serde::Serialize;
use shared::forja::{self, Degrau, Grau, REFINO_MAX, REFINO_SEGURO, TIER_MAX};
use sqlx::{PgPool, Row};

/// Le' uma coluna, ou o padrao.
///
/// `Row::get` entra em PANICO quando o tipo nao bate, e no axum isso derruba a
/// requisicao inteira: uma coluna que virou `NUMERIC` numa migracao apagaria o
/// painel todo em vez de uma linha dele. Painel de observabilidade que cai
/// junto com o que observa nao serve.
pub(crate) fn campo<'r, T>(r: &'r sqlx::postgres::PgRow, i: usize) -> T
where
    T: sqlx::Decode<'r, sqlx::Postgres> + sqlx::Type<sqlx::Postgres> + Default,
{
    r.try_get(i).unwrap_or_default()
}

#[derive(Serialize)]
pub struct Economia {
    /// A escada de refino, por grau. Previsao.
    escadas: Vec<EscadaDoGrau>,
    /// A escada de DEGRAUS (grau x tier) com o refino em cima. E' aqui que as
    /// duas se multiplicam.
    degraus: Vec<DegrauNoMundo>,
    /// Os niveis de refino que a tabela de degraus mostra.
    alvos: Vec<u8>,
    /// A curva de demanda: o que cada passo da progressao cobra A MAIS.
    demanda: Vec<PassoDaDemanda>,
    tier_max: u8,
    /// Chance de chegar a cada nivel, e onde comeca a destruicao.
    chances: Vec<u8>,
    refino_seguro: u8,
    darksteel_por_hora: f64,
    /// O que o banco mede.
    jogadores: Jogadores,
    itens: Vec<ItemNoMundo>,
    drops: Vec<DropMedido>,
    tabela_de_loot: Vec<LootDoBicho>,
    /// Ouro carregado por quem esta' ONLINE agora, vindo dos canais.
    ouro_online: u64,
}

#[derive(Serialize)]
struct EscadaDoGrau {
    grau: String,
    /// Custo de UMA tentativa: (darksteel, cobre).
    tentativa: [u32; 2],
    degraus: Vec<Nivel>,
}

/// Um passo da progressao, com o custo MARGINAL.
///
/// O acumulado sozinho engana: ele cresce e a pessoa se acostuma com a curva.
/// O que decide se o jogador continua ou para e' o proximo passo — quanto ele
/// cobra A MAIS do que o que acabou de ser pago.
#[derive(Serialize)]
struct PassoDaDemanda {
    de: String,
    para: String,
    /// Acumulado: pecas de base pra ter esta peca, do zero.
    total: f64,
    /// So' este passo.
    passo: f64,
    /// Quantas vezes este passo custa em relacao ao anterior.
    vezes: f64,
    /// Horas de mineracao acumuladas (o darksteel do refino, quando ha').
    horas: f64,
}

#[derive(Serialize)]
struct DegrauNoMundo {
    nome: String,
    grau: String,
    tier: u8,
    /// Pecas de Comum I que UMA peca deste degrau custa. Dobra a cada passo.
    base: f64,
    /// Custo fechado por nivel de refino, na ordem de `alvos`.
    /// (pecas de base, darksteel, horas)
    por_alvo: Vec<(f64, f64, f64)>,
}

/// Um nivel de refino dentro de um grau.
#[derive(Serialize)]
struct Nivel {
    alvo: u8,
    pecas: f64,
    tentativas: f64,
    darksteel: f64,
    cobre: f64,
    horas: f64,
}

#[derive(Serialize, Default)]
struct Jogadores {
    total: i64,
    ouro_total: i64,
    ouro_mediano: i64,
    ouro_p90: i64,
    ouro_maior: i64,
    /// Quantos personagens em cada faixa de nivel.
    por_nivel: Vec<(u32, i64)>,
    /// Os que mais tem ouro. E' onde a inflacao aparece primeiro.
    maiores: Vec<(String, i64, u32)>,
}

#[derive(Serialize)]
struct ItemNoMundo {
    id: i32,
    nome: String,
    /// Somando mochila e baú.
    quantidade: i64,
    donos: i64,
    preco_venda: i32,
}

#[derive(Serialize)]
struct DropMedido {
    item_id: i32,
    nome: String,
    vezes: i64,
    quantidade: i64,
    /// Por hora, na janela medida.
    por_hora: f64,
}

#[derive(Serialize)]
struct LootDoBicho {
    kind: i32,
    nome: String,
    xp: i64,
    /// Valor esperado do loot em ouro de venda, por morte.
    ouro_por_morte: f64,
    itens: usize,
}

/// Junta previsao e medicao. Uma consulta por bloco, nada em laco.
pub async fn levantar(pool: &PgPool, ouro_online: u64) -> Economia {
    Economia {
        escadas: escadas(),
        degraus: degraus(),
        alvos: ALVOS.to_vec(),
        demanda: demanda(),
        tier_max: TIER_MAX,
        chances: (0..=REFINO_MAX).map(forja::chance_de_refino).collect(),
        refino_seguro: REFINO_SEGURO,
        darksteel_por_hora: forja::DARKSTEEL_POR_HORA,
        jogadores: jogadores(pool).await,
        itens: itens(pool).await,
        drops: drops(pool).await,
        tabela_de_loot: loot(pool).await,
        ouro_online,
    }
}

/// Os niveis que a tabela de degraus mostra.
///
/// Nao os doze: a tabela e' 20 degraus x N colunas, e doze colunas viram uma
/// planilha que ninguem le'. Estes quatro contam a historia — sem refino, no
/// fim da faixa segura, o primeiro degrau de aposta que vale a pena, e onde a
/// conta ja' saiu do mundo real.
const ALVOS: [u8; 4] = [0, 5, 7, 9];

fn degraus() -> Vec<DegrauNoMundo> {
    let base = Degrau::novo(Grau::Comum, 1);
    let mut fora = Vec::new();
    for grau in Grau::TODOS {
        for tier in 1..=TIER_MAX {
            let d = Degrau::novo(grau, tier);
            fora.push(DegrauNoMundo {
                nome: d.to_string(),
                grau: grau.nome().to_string(),
                tier,
                base: d.custo_em(base) as f64,
                por_alvo: ALVOS
                    .iter()
                    .map(|&a| {
                        let c = forja::custo_total(d, a, base);
                        (c.pecas_base, c.darksteel, c.horas)
                    })
                    .collect(),
            });
        }
    }
    fora
}

/// A progressao inteira, degrau a degrau, com o refino de referencia em cima.
///
/// O caminho e' o que o jogador realmente anda: sobe os vinte degraus, e em
/// cada um o custo considera levar a peca ao `REFINO_DE_REFERENCIA`. Sem o
/// refino a curva e' so' potencia de dois, que e' verdade e nao e' a conta —
/// ninguem para no +0.
fn demanda() -> Vec<PassoDaDemanda> {
    /// O refino que o painel usa pra falar de "uma peca pronta".
    ///
    /// +7 e nao +12: e' o primeiro degrau de aposta que ainda cabe numa vida
    /// (106 horas de mineracao num Raro), e foi o numero que decidiu o
    /// desenho da colonia offline.
    const REFINO_DE_REFERENCIA: u8 = 7;

    let base = Degrau::novo(Grau::Comum, 1);
    let mut fora: Vec<PassoDaDemanda> = Vec::new();
    let mut anterior_total = 0.0f64;
    let mut anterior_passo = 0.0f64;
    let mut nome_anterior = String::from("nada");
    for grau in Grau::TODOS {
        for tier in 1..=TIER_MAX {
            let d = Degrau::novo(grau, tier);
            let c = forja::custo_total(d, REFINO_DE_REFERENCIA, base);
            let passo = c.pecas_base - anterior_total;
            fora.push(PassoDaDemanda {
                de: nome_anterior.clone(),
                para: d.to_string(),
                total: c.pecas_base,
                passo,
                vezes: if anterior_passo > 0.0 { passo / anterior_passo } else { 0.0 },
                horas: c.horas,
            });
            anterior_total = c.pecas_base;
            anterior_passo = passo;
            nome_anterior = d.to_string();
        }
    }
    fora
}

fn escadas() -> Vec<EscadaDoGrau> {
    Grau::TODOS
        .iter()
        .map(|&g| {
            let (ds, cu) = forja::custo_de_refino(g);
            EscadaDoGrau {
                grau: g.nome().to_string(),
                tentativa: [ds, cu],
                degraus: forja::escada(g)
                    .into_iter()
                    .map(|e| Nivel {
                        alvo: e.alvo,
                        pecas: e.pecas,
                        tentativas: e.tentativas,
                        darksteel: e.darksteel,
                        cobre: e.cobre,
                        horas: e.horas,
                    })
                    .collect(),
            }
        })
        .collect()
}

async fn jogadores(pool: &PgPool) -> Jogadores {
    let mut j = Jogadores::default();
    // Percentil no banco: trazer o ouro de todo mundo pra ordenar aqui seria
    // carregar a base inteira pra responder tres numeros.
    if let Ok(r) = sqlx::query(
        "SELECT COUNT(*)::bigint,
                COALESCE(SUM(gold),0)::bigint,
                COALESCE(percentile_disc(0.5) WITHIN GROUP (ORDER BY gold),0)::bigint,
                COALESCE(percentile_disc(0.9) WITHIN GROUP (ORDER BY gold),0)::bigint,
                COALESCE(MAX(gold),0)::bigint
           FROM characters",
    )
    .fetch_one(pool)
    .await
    {
        j.total = campo(&r, 0);
        j.ouro_total = campo(&r, 1);
        j.ouro_mediano = campo(&r, 2);
        j.ouro_p90 = campo(&r, 3);
        j.ouro_maior = campo(&r, 4);
    }
    if let Ok(rs) = sqlx::query("SELECT name, gold::bigint, xp::bigint FROM characters ORDER BY gold DESC LIMIT 12")
        .fetch_all(pool)
        .await
    {
        j.maiores = rs
            .iter()
            .map(|r| {
                let xp: i64 = campo(r, 2);
                (campo::<String>(r, 0), campo::<i64>(r, 1), shared::level_of_xp(xp as u64))
            })
            .collect();
    }
    if let Ok(rs) = sqlx::query("SELECT xp::bigint FROM characters").fetch_all(pool).await {
        let mut por: std::collections::BTreeMap<u32, i64> = Default::default();
        for r in rs {
            let xp: i64 = campo(&r, 0);
            *por.entry(shared::level_of_xp(xp as u64)).or_default() += 1;
        }
        j.por_nivel = por.into_iter().collect();
    }
    j
}

async fn itens(pool: &PgPool) -> Vec<ItemNoMundo> {
    // Mochila e bau na mesma conta: pro estoque do mundo, onde a coisa esta'
    // guardada nao muda nada.
    let sql = "
        WITH tudo AS (
            SELECT item_id, qty, character_name FROM inventory
            UNION ALL
            SELECT item_id, qty, character_name FROM vault
        )
        SELECT t.item_id,
               COALESCE(i.name, '#' || t.item_id) AS nome,
               SUM(t.qty)::bigint                 AS quantidade,
               COUNT(DISTINCT t.character_name)::bigint AS donos,
               COALESCE(i.sell_price, 0)          AS preco
          FROM tudo t LEFT JOIN items i ON i.id = t.item_id
         GROUP BY t.item_id, i.name, i.sell_price
         ORDER BY quantidade DESC
         LIMIT 40";
    sqlx::query(sql)
        .fetch_all(pool)
        .await
        .map(|rs| {
            rs.iter()
                .map(|r| ItemNoMundo {
                    id: campo(r, 0),
                    nome: campo(r, 1),
                    quantidade: campo(r, 2),
                    donos: campo(r, 3),
                    preco_venda: campo(r, 4),
                })
                .collect()
        })
        .unwrap_or_default()
}

async fn drops(pool: &PgPool) -> Vec<DropMedido> {
    // Janela de 7 dias: o suficiente pra a cauda longa aparecer sem carregar
    // o log inteiro. `por_hora` sai da janela REAL do dado e nao de 168 fixo —
    // com o log de dois dias, dividir por 168 esconderia a taxa por 4x.
    let sql = "
        WITH janela AS (
            SELECT * FROM item_drops_log WHERE ts > NOW() - INTERVAL '7 days'
        ), horas AS (
            SELECT GREATEST(EXTRACT(EPOCH FROM (MAX(ts) - MIN(ts))) / 3600.0, 0.02) AS h
              FROM janela
        )
        SELECT j.item_id,
               COALESCE(i.name, '#' || j.item_id) AS nome,
               COUNT(*)::bigint                   AS vezes,
               SUM(j.qty)::bigint                 AS quantidade,
               (SUM(j.qty) / (SELECT h FROM horas))::float8 AS por_hora
          FROM janela j LEFT JOIN items i ON i.id = j.item_id
         GROUP BY j.item_id, i.name
         ORDER BY quantidade DESC
         LIMIT 40";
    sqlx::query(sql)
        .fetch_all(pool)
        .await
        .map(|rs| {
            rs.iter()
                .map(|r| DropMedido {
                    item_id: campo(r, 0),
                    nome: campo(r, 1),
                    vezes: campo(r, 2),
                    quantidade: campo(r, 3),
                    por_hora: campo(r, 4),
                })
                .collect()
        })
        .unwrap_or_default()
}

async fn loot(pool: &PgPool) -> Vec<LootDoBicho> {
    // Valor ESPERADO por morte: chance x quantidade media x preco de venda.
    // E' a curva de loot como ela foi CONFIGURADA — a medida vem do log.
    let sql = "
        SELECT e.kind, e.name, e.xp_reward::bigint,
               COALESCE(SUM(d.chance * (d.qty_min + d.qty_max) / 2.0
                            * COALESCE(i.sell_price, 0)), 0)::float8 AS ouro,
               COUNT(d.id)::bigint AS itens
          FROM enemy_kinds e
          LEFT JOIN loot_drops d ON d.enemy_kind = e.kind
          LEFT JOIN items i      ON i.id = d.item_id
         GROUP BY e.kind, e.name, e.xp_reward
         ORDER BY e.xp_reward ASC
         LIMIT 60";
    sqlx::query(sql)
        .fetch_all(pool)
        .await
        .map(|rs| {
            rs.iter()
                .map(|r| LootDoBicho {
                    kind: campo(r, 0),
                    nome: campo(r, 1),
                    xp: campo(r, 2),
                    ouro_por_morte: campo(r, 3),
                    itens: campo::<i64>(r, 4) as usize,
                })
                .collect()
        })
        .unwrap_or_default()
}
