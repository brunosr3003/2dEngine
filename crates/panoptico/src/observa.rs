//! O resto do jogo visto de cima: jogadores, mercado, dungeons, atividade
//! (telemetria), missoes, itens e infraestrutura.
//!
//! Tudo SO' leitura e tudo tolerante: consulta que falha vira lista vazia e um
//! aviso no log, nunca um painel fora do ar. Os numeros de desenho (catalogo de
//! dungeons, nomes de missao, chaves) saem de `shared` — o mesmo codigo do
//! jogo, sem copia aqui.

use std::collections::{BTreeMap, HashMap};

use serde_json::{json, Value};
use sqlx::postgres::PgRow;
use sqlx::PgPool;

use crate::economia::campo;

async fn linhas(pool: &PgPool, sql: &str) -> Vec<PgRow> {
    match sqlx::query(sql).fetch_all(pool).await {
        Ok(r) => r,
        Err(e) => {
            tracing::warn!("panoptico: consulta falhou: {e}");
            Vec::new()
        }
    }
}

async fn uma(pool: &PgPool, sql: &str) -> Option<PgRow> {
    match sqlx::query(sql).fetch_optional(pool).await {
        Ok(r) => r,
        Err(e) => {
            tracing::warn!("panoptico: consulta falhou: {e}");
            None
        }
    }
}

fn agora_unix() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as i64)
}

/// `characters.updated` guarda unix em segundos ou em ms, conforme a epoca do
/// banco. Normaliza pra segundos.
pub fn para_segundos(v: i64) -> i64 {
    if v > 100_000_000_000 {
        v / 1000
    } else {
        v
    }
}

async fn multiplicador_de_xp(pool: &PgPool) -> u64 {
    uma(
        pool,
        "SELECT value FROM server_config WHERE key = 'xp_multiplier'",
    )
    .await
    .and_then(|r| campo::<String>(&r, 0).parse().ok())
    .unwrap_or(shared::DEFAULT_XP_MULTIPLIER)
}

async fn nomes_de_itens(pool: &PgPool) -> HashMap<i64, String> {
    linhas(pool, "SELECT id::bigint, name FROM items")
        .await
        .iter()
        .map(|r| (campo::<i64>(r, 0), campo::<String>(r, 1)))
        .collect()
}

async fn tem_tabela(pool: &PgPool, nome: &str) -> bool {
    sqlx::query_scalar::<_, bool>("SELECT to_regclass($1) IS NOT NULL")
        .bind(nome)
        .fetch_one(pool)
        .await
        .unwrap_or(false)
}

fn e_historia(id: u16) -> bool {
    id == shared::historia::ID_MARCO || shared::historia::e_da_historia(id)
}

fn capitulo_de(id: u16) -> Option<&'static str> {
    shared::historia::CAPITULOS
        .iter()
        .find(|c| (c.primeiro..=c.ultimo).contains(&id))
        .map(|c| c.nome)
}

/// Categoria de uma missao, pro painel.
pub fn categoria_da_missao(id: u16) -> &'static str {
    if e_historia(id) {
        "história"
    } else if (600..700).contains(&id) {
        "diária"
    } else if (501..=505).contains(&id) {
        "mestre"
    } else if (900..1000).contains(&id) {
        "tutorial"
    } else {
        "área"
    }
}

fn titulo_da_missao(id: u16) -> String {
    shared::quests::quest_by_id(id).map_or_else(|| format!("missão {id}"), |q| q.title.to_string())
}

fn nome_do_conteudo(id: u16) -> String {
    shared::dungeon::conteudo(id).map_or_else(|| format!("conteúdo {id}"), |c| c.nome.to_string())
}

// ─────────────────────────────── jogadores ───────────────────────────────

/// `online`: nome do personagem -> canal onde esta' agora (dos retratos).
pub async fn jogadores(pool: &PgPool, online: &HashMap<String, String>) -> Value {
    let agora = agora_unix();
    let mult = multiplicador_de_xp(pool).await;
    let contas = uma(
        pool,
        "SELECT COUNT(*)::bigint, COUNT(google_sub)::bigint,
                COUNT(*) FILTER (WHERE created_at > NOW() - INTERVAL '7 days')::bigint
           FROM accounts",
    )
    .await;

    // Historia: onde cada um esta' na cadeia.
    let mut historia: HashMap<String, (u16, u8, u32)> = HashMap::new(); // (atual, status, concluidas)
    for r in linhas(
        pool,
        "SELECT char_name, quest_id, status::int FROM character_quests",
    )
    .await
    {
        let nome: String = campo(&r, 0);
        let id = campo::<i32>(&r, 1).clamp(0, u16::MAX as i32) as u16;
        let status = campo::<i32>(&r, 2) as u8;
        if !e_historia(id) {
            continue;
        }
        let e = historia.entry(nome).or_insert((0, 0, 0));
        if status == shared::quests::quest_status::TURNED_IN {
            e.2 += 1;
        } else if id >= e.0 {
            *e = (id, status, e.2);
        }
    }

    let rs = linhas(
        pool,
        "SELECT c.name, COALESCE(c.account_id, 0)::bigint, COALESCE(a.username, ''), COALESCE(c.xp, 0)::bigint,
                COALESCE(c.gold, 0)::bigint, COALESCE(c.zona, ''), COALESCE(c.updated, 0)::bigint, COALESCE(c.faction, ''),
                COALESCE(c.xp_bonus_ate, 0)::bigint, COALESCE(c.fortuna_ate, 0)::bigint, COALESCE(c.sorte_ate, 0)::bigint,
                COALESCE(c.mortes_json, ''), COALESCE(c.recuperacoes_usadas, 0)::int, COALESCE(c.preferencias_json, ''),
                COALESCE(c.dungeon_json, ''), COALESCE(c.fame, 0)::bigint
           FROM characters c LEFT JOIN accounts a ON a.id = c.account_id
          ORDER BY c.updated DESC NULLS LAST
          LIMIT 2000",
    )
    .await;

    let hoje = shared::dungeon::dia(agora);
    let (mut ativos_24h, mut ativos_7d, mut buffs, mut mortes_pend, mut xp_pend, mut eco_auto) =
        (0, 0, 0, 0, 0i64, 0);
    let mut por_zona: BTreeMap<String, (i64, i64)> = BTreeMap::new();
    let mut lista = Vec::with_capacity(rs.len());
    for r in &rs {
        let nome: String = campo(r, 0);
        let xp = campo::<i64>(r, 3).max(0) as u64;
        let visto = para_segundos(campo(r, 6));
        let idade = agora - visto;
        if visto > 0 && idade < 86_400 {
            ativos_24h += 1;
        }
        if visto > 0 && idade < 7 * 86_400 {
            ativos_7d += 1;
        }
        let zona: String = campo(r, 5);
        let z = por_zona
            .entry(if zona.is_empty() {
                "?".into()
            } else {
                zona.clone()
            })
            .or_default();
        z.0 += 1;
        let canal = online.get(&nome).cloned();
        if canal.is_some() {
            z.1 += 1;
        }
        let (bxp, bfor, bsor) = (
            campo::<i64>(r, 8) - agora,
            campo::<i64>(r, 9) - agora,
            campo::<i64>(r, 10) - agora,
        );
        if bxp > 0 || bfor > 0 || bsor > 0 {
            buffs += 1;
        }
        let mortes: Vec<Value> = serde_json::from_str(&campo::<String>(r, 11)).unwrap_or_default();
        let xp_mortes: i64 = mortes
            .iter()
            .filter_map(|m| m.get("xp").and_then(Value::as_i64))
            .sum();
        mortes_pend += mortes.len() as i64;
        xp_pend += xp_mortes;
        let prefs: shared::protocol::Preferencias =
            serde_json::from_str(&campo::<String>(r, 13)).unwrap_or_default();
        if prefs.economia_auto_min.is_some_and(|m| m > 0) {
            eco_auto += 1;
        }
        let mut dg: shared::dungeon::DadosDungeon =
            serde_json::from_str(&campo::<String>(r, 14)).unwrap_or_default();
        let compradas_hoje = if dg.gruta.dia == hoje {
            dg.gruta.compradas
        } else {
            0
        };
        let vitorias: u32 = dg.vitorias.iter().map(|v| v.2).sum();
        let maior = dg.liberado.iter().map(|l| l.1).max().unwrap_or(0);
        dg.correio.truncate(200);
        let h = historia.get(&nome).copied();
        lista.push(json!({
            "nome": nome,
            "conta": campo::<i64>(r, 1),
            "usuario": campo::<String>(r, 2),
            "nivel": shared::level_of_xp_with_mult(xp, mult),
            "xp": xp,
            "ouro": campo::<i64>(r, 4),
            "zona": zona,
            "visto": visto,
            "online": canal,
            "faccao": campo::<String>(r, 7),
            "fama": campo::<i64>(r, 15),
            "buff_xp_s": bxp.max(0),
            "buff_fortuna_s": bfor.max(0),
            "buff_sorte_s": bsor.max(0),
            "mortes": mortes.len(),
            "xp_a_recuperar": xp_mortes,
            "recuperacoes_usadas": campo::<i32>(r, 12),
            "skills_auto": prefs.skills_auto,
            "economia_auto_min": prefs.economia_auto_min,
            "escala_ui": prefs.escala_ui,
            "dungeon": {
                "vitorias": vitorias,
                "maior_estagio": maior,
                "correio": dg.correio.len(),
                "baus": dg.baus_abertos.len(),
                "gruta_saldo": dg.gruta.saldo,
                "gruta_compradas_hoje": compradas_hoje,
                "porao_saldo": dg.porao.saldo,
            },
            "historia": h.map(|(id, status, feitas)| json!({
                "id": id,
                "titulo": titulo_da_missao(id),
                "capitulo": capitulo_de(id),
                "pronta": status == shared::quests::quest_status::READY,
                "concluidas": feitas,
            })),
        }));
    }
    json!({
        "resumo": {
            "contas": contas.as_ref().map_or(0, |r| campo::<i64>(r, 0)),
            "google": contas.as_ref().map_or(0, |r| campo::<i64>(r, 1)),
            "contas_7d": contas.as_ref().map_or(0, |r| campo::<i64>(r, 2)),
            "personagens": rs.len(),
            "online": online.len(),
            "ativos_24h": ativos_24h,
            "ativos_7d": ativos_7d,
            "buffs_ativos": buffs,
            "mortes_pendentes": mortes_pend,
            "xp_a_recuperar": xp_pend,
            "economia_auto": eco_auto,
            "multiplicador_xp": mult,
        },
        "por_zona": por_zona.into_iter().map(|(z, (t, o))| json!([z, t, o])).collect::<Vec<_>>(),
        "lista": lista,
    })
}

// ─────────────────────────────── mercado ───────────────────────────────

pub async fn mercado(realm: &PgPool, central: Option<&PgPool>) -> Value {
    let saida = uma(
        realm,
        "SELECT COUNT(*) FILTER (WHERE enviada IS NULL)::bigint,
                COUNT(*) FILTER (WHERE enviada > NOW() - INTERVAL '24 hours')::bigint,
                COALESCE(EXTRACT(EPOCH FROM NOW() - MIN(criada) FILTER (WHERE enviada IS NULL)), 0)::bigint
           FROM mercado_saida",
    )
    .await;
    let aplicadas = uma(
        realm,
        "SELECT COUNT(*) FILTER (WHERE aplicada > NOW() - INTERVAL '24 hours')::bigint,
                COUNT(*) FILTER (WHERE avisada IS NULL)::bigint
           FROM mercado_cartas_aplicadas",
    )
    .await;
    let resultados: Vec<Value> = linhas(
        realm,
        "SELECT COALESCE(resultado, ''), COUNT(*)::bigint FROM mercado_saida
          WHERE resultado IS NOT NULL AND criada > NOW() - INTERVAL '7 days'
          GROUP BY 1 ORDER BY 2 DESC LIMIT 20",
    )
    .await
    .iter()
    .map(|r| json!([campo::<String>(r, 0), campo::<i64>(r, 1)]))
    .collect();
    let realm_json = json!({
        "saidas_pendentes": saida.as_ref().map_or(0, |r| campo::<i64>(r, 0)),
        "saidas_24h": saida.as_ref().map_or(0, |r| campo::<i64>(r, 1)),
        "saida_mais_velha_s": saida.as_ref().map_or(0, |r| campo::<i64>(r, 2)),
        "cartas_aplicadas_24h": aplicadas.as_ref().map_or(0, |r| campo::<i64>(r, 0)),
        "cartas_sem_aviso": aplicadas.as_ref().map_or(0, |r| campo::<i64>(r, 1)),
        "resultados_7d": resultados,
    });
    let Some(c) = central else {
        return json!({ "ligado": false, "realm": realm_json });
    };

    let ativos: Vec<Value> = linhas(
        c,
        "SELECT tipo::int, COUNT(*)::bigint, COALESCE(SUM(qtd_restante), 0)::bigint,
                COALESCE(SUM(qtd_restante * preco_unit), 0)::bigint
           FROM mercado_anuncios WHERE estado = 0 GROUP BY tipo",
    )
    .await
    .iter()
    .map(|r| json!({ "tipo": campo::<i32>(r, 0), "anuncios": campo::<i64>(r, 1), "qtd": campo::<i64>(r, 2), "valor": campo::<i64>(r, 3) }))
    .collect();
    let anunciados: Vec<Value> = linhas(
        c,
        "SELECT nome, COUNT(*)::bigint, COALESCE(SUM(qtd_restante), 0)::bigint, MIN(preco_unit)::bigint, AVG(preco_unit)::float8
           FROM mercado_anuncios WHERE estado = 0 GROUP BY nome ORDER BY 2 DESC LIMIT 60",
    )
    .await
    .iter()
    .map(|r| json!([campo::<String>(r, 0), campo::<i64>(r, 1), campo::<i64>(r, 2), campo::<i64>(r, 3), campo::<f64>(r, 4)]))
    .collect();
    let v24 = uma(
        c,
        "SELECT COUNT(*)::bigint, COALESCE(SUM(bruto), 0)::bigint, COALESCE(SUM(taxa), 0)::bigint, COALESCE(SUM(qtd), 0)::bigint
           FROM mercado_vendas WHERE quando > NOW() - INTERVAL '24 hours'",
    )
    .await;
    let vtot = uma(c, "SELECT COUNT(*)::bigint, COALESCE(SUM(bruto), 0)::bigint, COALESCE(SUM(taxa), 0)::bigint FROM mercado_vendas").await;
    let precos: Vec<Value> = linhas(
        c,
        "SELECT nome, tipo::int, COUNT(*)::bigint, COALESCE(SUM(qtd), 0)::bigint,
                (SUM(bruto)::float8 / NULLIF(SUM(qtd), 0))::float8,
                percentile_cont(0.5) WITHIN GROUP (ORDER BY preco_unit)::float8,
                MIN(preco_unit)::bigint, MAX(preco_unit)::bigint, COALESCE(SUM(taxa), 0)::bigint
           FROM mercado_vendas WHERE quando > NOW() - INTERVAL '7 days'
          GROUP BY nome, tipo ORDER BY 3 DESC LIMIT 80",
    )
    .await
    .iter()
    .map(|r| {
        json!({
            "nome": campo::<String>(r, 0), "tipo": campo::<i32>(r, 1), "vendas": campo::<i64>(r, 2), "qtd": campo::<i64>(r, 3),
            "media": campo::<f64>(r, 4), "mediana": campo::<f64>(r, 5), "min": campo::<i64>(r, 6), "max": campo::<i64>(r, 7),
            "taxa": campo::<i64>(r, 8),
        })
    })
    .collect();
    let por_hora: Vec<Value> = linhas(
        c,
        "SELECT EXTRACT(EPOCH FROM date_trunc('hour', quando))::bigint, COUNT(*)::bigint,
                COALESCE(SUM(bruto), 0)::bigint, COALESCE(SUM(taxa), 0)::bigint
           FROM mercado_vendas WHERE quando > NOW() - INTERVAL '48 hours' GROUP BY 1 ORDER BY 1",
    )
    .await
    .iter()
    .map(|r| {
        json!([
            campo::<i64>(r, 0),
            campo::<i64>(r, 1),
            campo::<i64>(r, 2),
            campo::<i64>(r, 3)
        ])
    })
    .collect();
    let ultimas: Vec<Value> = linhas(
        c,
        "SELECT nome, qtd, preco_unit, bruto, taxa, vendedor || '@' || vendedor_realm, comprador || '@' || comprador_realm,
                EXTRACT(EPOCH FROM quando)::bigint
           FROM mercado_vendas ORDER BY quando DESC LIMIT 40",
    )
    .await
    .iter()
    .map(|r| {
        json!([campo::<String>(r, 0), campo::<i64>(r, 1), campo::<i64>(r, 2), campo::<i64>(r, 3), campo::<i64>(r, 4),
               campo::<String>(r, 5), campo::<String>(r, 6), campo::<i64>(r, 7)])
    })
    .collect();
    let cartas = uma(
        c,
        "SELECT COUNT(*) FILTER (WHERE entregue IS NULL)::bigint,
                COUNT(*) FILTER (WHERE entregue > NOW() - INTERVAL '24 hours')::bigint,
                COALESCE(EXTRACT(EPOCH FROM NOW() - MIN(criada) FILTER (WHERE entregue IS NULL)), 0)::bigint,
                COALESCE(SUM(gold) FILTER (WHERE entregue IS NULL), 0)::bigint
           FROM mercado_cartas",
    )
    .await;
    let cartas_motivo: Vec<Value> = linhas(c, "SELECT motivo, COUNT(*)::bigint FROM mercado_cartas WHERE entregue IS NULL GROUP BY motivo ORDER BY 2 DESC")
        .await
        .iter()
        .map(|r| json!([campo::<String>(r, 0), campo::<i64>(r, 1)]))
        .collect();
    let ops: Vec<Value> = linhas(
        c,
        "SELECT tipo, ok, COUNT(*)::bigint FROM mercado_operacoes
          WHERE quando > NOW() - INTERVAL '24 hours' GROUP BY tipo, ok ORDER BY tipo, ok DESC",
    )
    .await
    .iter()
    .map(|r| {
        json!([
            campo::<String>(r, 0),
            campo::<bool>(r, 1),
            campo::<i64>(r, 2)
        ])
    })
    .collect();
    let recusadas: Vec<Value> = linhas(
        c,
        "SELECT tipo, realm, COALESCE(resultado, ''), EXTRACT(EPOCH FROM quando)::bigint
           FROM mercado_operacoes WHERE NOT ok ORDER BY quando DESC LIMIT 40",
    )
    .await
    .iter()
    .map(|r| {
        json!([
            campo::<String>(r, 0),
            campo::<String>(r, 1),
            campo::<String>(r, 2),
            campo::<i64>(r, 3)
        ])
    })
    .collect();
    let tp = uma(
        c,
        "WITH ult AS (SELECT DISTINCT ON (conta) conta, saldo_depois FROM tp_razao ORDER BY conta, id DESC)
         SELECT COUNT(*) FILTER (WHERE saldo_depois > 0)::bigint, COALESCE(SUM(saldo_depois), 0)::bigint FROM ult",
    )
    .await;
    let tp_maiores: Vec<Value> = linhas(
        c,
        "SELECT conta, saldo_depois FROM (SELECT DISTINCT ON (conta) conta, saldo_depois FROM tp_razao ORDER BY conta, id DESC) u
          ORDER BY saldo_depois DESC LIMIT 20",
    )
    .await
    .iter()
    .map(|r| json!([campo::<String>(r, 0), campo::<i64>(r, 1)]))
    .collect();
    let tp_motivos: Vec<Value> = linhas(
        c,
        "SELECT motivo, COUNT(*)::bigint, COALESCE(SUM(delta), 0)::bigint FROM tp_razao
          WHERE quando > NOW() - INTERVAL '7 days' GROUP BY motivo ORDER BY 2 DESC",
    )
    .await
    .iter()
    .map(|r| {
        json!([
            campo::<String>(r, 0),
            campo::<i64>(r, 1),
            campo::<i64>(r, 2)
        ])
    })
    .collect();
    let tp_movs: Vec<Value> = linhas(
        c,
        "SELECT conta, delta, saldo_depois, motivo, EXTRACT(EPOCH FROM quando)::bigint FROM tp_razao ORDER BY id DESC LIMIT 40",
    )
    .await
    .iter()
    .map(|r| json!([campo::<String>(r, 0), campo::<i64>(r, 1), campo::<i64>(r, 2), campo::<String>(r, 3), campo::<i64>(r, 4)]))
    .collect();
    let g = |r: &Option<PgRow>, i: usize| r.as_ref().map_or(0, |r| campo::<i64>(r, i));
    json!({
        "ligado": true,
        "realm": realm_json,
        "ativos": ativos,
        "anunciados": anunciados,
        "vendas_24h": { "vendas": g(&v24, 0), "bruto": g(&v24, 1), "taxa": g(&v24, 2), "qtd": g(&v24, 3) },
        "vendas_total": { "vendas": g(&vtot, 0), "bruto": g(&vtot, 1), "taxa": g(&vtot, 2) },
        "precos_7d": precos,
        "por_hora": por_hora,
        "ultimas": ultimas,
        "cartas": { "pendentes": g(&cartas, 0), "entregues_24h": g(&cartas, 1), "mais_velha_s": g(&cartas, 2), "gold_pendente": g(&cartas, 3) },
        "cartas_por_motivo": cartas_motivo,
        "operacoes_24h": ops,
        "recusadas": recusadas,
        "tp": { "contas_com_saldo": g(&tp, 0), "em_circulacao": g(&tp, 1) },
        "tp_maiores": tp_maiores,
        "tp_motivos_7d": tp_motivos,
        "tp_movimentos": tp_movs,
    })
}

// ─────────────────────────────── telemetria ───────────────────────────────

/// Soma por `(tipo, chave)` na janela, pra quem monta tabela.
async fn somas(pool: &PgPool, filtro: &str, horas: i32) -> Vec<(String, String, i64)> {
    if !tem_tabela(pool, "telemetria").await {
        return Vec::new();
    }
    let sql = format!(
        "SELECT tipo, chave, SUM(valor)::bigint FROM telemetria
          WHERE minuto > NOW() - make_interval(hours => $1) AND ({filtro})
          GROUP BY tipo, chave"
    );
    match sqlx::query(&sql).bind(horas).fetch_all(pool).await {
        Ok(rs) => rs
            .iter()
            .map(|r| (campo(r, 0), campo(r, 1), campo(r, 2)))
            .collect(),
        Err(e) => {
            tracing::warn!("panoptico: telemetria: {e}");
            Vec::new()
        }
    }
}

/// Rotulo legivel de uma chave de telemetria.
pub fn rotulo(
    tipo: &str,
    chave: &str,
    itens: &HashMap<i64, String>,
    receitas: &HashMap<i64, String>,
    skills: &HashMap<i64, String>,
) -> String {
    let item = |s: &str| {
        s.parse::<i64>()
            .ok()
            .and_then(|id| itens.get(&id).cloned())
            .unwrap_or_else(|| format!("item {s}"))
    };
    match tipo {
        "drop" | "coleta_item" | "item_usado" | "loja_compra" | "loja_venda"
        | "dungeon_bau_item" => item(chave),
        "chave_drop" => match chave.split_once(':') {
            Some((origem, id)) => format!("{} · {origem}", item(id)),
            None => chave.to_string(),
        },
        "craft" => chave
            .parse::<i64>()
            .ok()
            .and_then(|id| receitas.get(&id).cloned())
            .unwrap_or_else(|| format!("receita {chave}")),
        "skill_pedida" => chave
            .parse::<i64>()
            .ok()
            .and_then(|id| skills.get(&id).cloned())
            .unwrap_or_else(|| format!("skill {chave}")),
        "missao_entregue" | "missao_historia" | "missao_abandonada" => {
            chave.parse::<u16>().map_or_else(
                |_| chave.to_string(),
                |id| format!("{} ({})", titulo_da_missao(id), categoria_da_missao(id)),
            )
        }
        t if t.starts_with("dungeon") || t == "selo_usado" => {
            let mut p = chave.split(':');
            match (
                p.next().and_then(|c| c.parse::<u16>().ok()),
                p.next(),
                p.next(),
            ) {
                (Some(c), Some(e), resto) => {
                    let base = format!("{} · estágio {e}", nome_do_conteudo(c));
                    resto.map_or(base.clone(), |r| {
                        format!("{base} · {}", r.replace('_', " "))
                    })
                }
                _ => chave.to_string(),
            }
        }
        _ if chave.is_empty() => "total".to_string(),
        _ => chave.replace('_', " "),
    }
}

pub async fn atividade(pool: &PgPool, horas: i32) -> Value {
    let horas = horas.clamp(1, 24 * 30);
    let ligada = tem_tabela(pool, "telemetria").await;
    let itens = nomes_de_itens(pool).await;
    let receitas: HashMap<i64, String> = linhas(pool, "SELECT id::bigint, name FROM craft_recipes")
        .await
        .iter()
        .map(|r| (campo::<i64>(r, 0), campo::<String>(r, 1)))
        .collect();
    let skills: HashMap<i64, String> = linhas(pool, "SELECT id::bigint, nome FROM skills")
        .await
        .iter()
        .map(|r| (campo::<i64>(r, 0), campo::<String>(r, 1)))
        .collect();
    let totais: Vec<Value> = somas(pool, "TRUE", horas)
        .await
        .into_iter()
        .map(|(t, c, v)| {
            let rot = rotulo(&t, &c, &itens, &receitas, &skills);
            json!({ "tipo": t, "chave": c, "rotulo": rot, "valor": v })
        })
        .collect();
    let mut series: BTreeMap<String, Vec<[i64; 2]>> = BTreeMap::new();
    if ligada {
        if let Ok(rs) = sqlx::query(
            "SELECT tipo, EXTRACT(EPOCH FROM date_trunc('hour', minuto))::bigint, SUM(valor)::bigint FROM telemetria
              WHERE minuto > NOW() - make_interval(hours => $1) GROUP BY 1, 2 ORDER BY 2",
        )
        .bind(horas)
        .fetch_all(pool)
        .await
        {
            for r in rs {
                series.entry(campo(&r, 0)).or_default().push([campo(&r, 1), campo(&r, 2)]);
            }
        }
    }
    json!({ "ligada": ligada, "horas": horas, "totais": totais, "series": series })
}

// ─────────────────────────────── dungeons ───────────────────────────────

/// Parte "conteudo:estagio[:resto]".
pub fn partes_de_dungeon(chave: &str) -> Option<(u16, u8, Option<&str>)> {
    let mut p = chave.splitn(3, ':');
    let c = p.next()?.parse().ok()?;
    let e = p.next()?.parse().ok()?;
    Some((c, e, p.next()))
}

#[derive(Default, Clone, Copy)]
struct PorEstagio {
    entradas: i64,
    ajudantes: i64,
    vitorias: i64,
    tempo_esgotado: i64,
    wipes: i64,
    tempo_s: i64,
    bonus: i64,
    baus: i64,
    selos: i64,
}

pub async fn dungeons(pool: &PgPool, horas: i32) -> Value {
    let horas = horas.clamp(1, 24 * 30);
    let itens = nomes_de_itens(pool).await;
    let mut mapa: BTreeMap<(u16, u8), PorEstagio> = BTreeMap::new();
    let mut bau_itens: Vec<(i64, String, i64)> = Vec::new();
    let (mut compradas, mut chefes_mortos, mut mobs) = (0i64, 0i64, 0i64);
    for (tipo, chave, v) in somas(
        pool,
        "tipo LIKE 'dungeon%' OR tipo IN ('selo_usado', 'chefe_dungeon_morto', 'kill_dungeon')",
        horas,
    )
    .await
    {
        match tipo.as_str() {
            "dungeon_bau_item" => {
                let id = chave.parse::<i64>().unwrap_or(0);
                bau_itens.push((
                    id,
                    itens
                        .get(&id)
                        .cloned()
                        .unwrap_or_else(|| format!("item {id}")),
                    v,
                ));
                continue;
            }
            "dungeon_entrada_comprada" => {
                compradas += v;
                continue;
            }
            "chefe_dungeon_morto" => {
                chefes_mortos += v;
                continue;
            }
            "kill_dungeon" => {
                mobs += v;
                continue;
            }
            _ => {}
        }
        let Some((c, e, resto)) = partes_de_dungeon(&chave) else {
            continue;
        };
        let x = mapa.entry((c, e)).or_default();
        match (tipo.as_str(), resto) {
            ("dungeon_entrada", Some("ajudante")) => {
                x.entradas += v;
                x.ajudantes += v;
            }
            ("dungeon_entrada", _) => x.entradas += v,
            ("dungeon_resultado", Some("vitoria")) => x.vitorias += v,
            ("dungeon_resultado", Some(_)) => x.tempo_esgotado += v,
            ("dungeon_wipe", _) => x.wipes += v,
            ("dungeon_tempo_s", _) => x.tempo_s += v,
            ("dungeon_bonus_tempo", _) => x.bonus += v,
            ("dungeon_bau", _) => x.baus += v,
            ("selo_usado", _) => x.selos += v,
            _ => {}
        }
    }
    bau_itens.sort_by(|a, b| b.2.cmp(&a.2));

    // O que os personagens guardam: vitorias de sempre, correio, entradas.
    let hoje = shared::dungeon::dia(agora_unix());
    let mut vitorias_sempre: BTreeMap<(u16, u8), u32> = BTreeMap::new();
    let mut liberado: BTreeMap<(u16, u8), u32> = BTreeMap::new();
    let (mut correio, mut baus, mut gruta_compradas_hoje, mut com_dados) =
        (0usize, 0usize, 0u32, 0usize);
    let mut correio_motivo = [0usize; 3];
    for r in linhas(pool, "SELECT COALESCE(dungeon_json, '') FROM characters WHERE dungeon_json IS NOT NULL AND dungeon_json <> ''").await {
        let Ok(d) = serde_json::from_str::<shared::dungeon::DadosDungeon>(&campo::<String>(&r, 0)) else { continue };
        com_dados += 1;
        for (c, e, n) in &d.vitorias {
            *vitorias_sempre.entry((*c, *e)).or_default() += n;
        }
        for (c, e) in &d.liberado {
            *liberado.entry((*c, *e)).or_default() += 1;
        }
        correio += d.correio.len();
        for carta in &d.correio {
            correio_motivo[(carta.motivo as usize).min(2)] += 1;
        }
        baus += d.baus_abertos.len();
        if d.gruta.dia == hoje {
            gruta_compradas_hoje += d.gruta.compradas as u32;
        }
    }
    let semana = shared::dungeon::semana(agora_unix());
    let (mut selos_semana, mut primeiras_semana, mut contas) = (0u32, 0usize, 0usize);
    for r in linhas(pool, "SELECT COALESCE(dados_json, '') FROM dungeon_contas").await {
        let Ok(d) = serde_json::from_str::<shared::dungeon::DadosConta>(&campo::<String>(&r, 0))
        else {
            continue;
        };
        contas += 1;
        if d.semana == semana {
            selos_semana += d.selos as u32;
            primeiras_semana += d.primeiras.len();
        }
    }

    let catalogo: Vec<Value> = shared::dungeon::CONTEUDOS
        .iter()
        .map(|c| {
            let estagios: Vec<Value> = (1..=5u8)
                .map(|e| {
                    let x = mapa.get(&(c.id, e)).copied().unwrap_or_default();
                    let terminadas = x.vitorias + x.tempo_esgotado;
                    json!({
                        "estagio": e,
                        "entradas": x.entradas, "ajudantes": x.ajudantes, "vitorias": x.vitorias,
                        "tempo_esgotado": x.tempo_esgotado, "wipes": x.wipes, "bonus": x.bonus, "baus": x.baus, "selos": x.selos,
                        "taxa_vitoria": if terminadas > 0 { x.vitorias as f64 / terminadas as f64 } else { 0.0 },
                        "duracao_media_s": if terminadas > 0 { x.tempo_s as f64 / terminadas as f64 } else { 0.0 },
                        "vitorias_sempre": vitorias_sempre.get(&(c.id, e)).copied().unwrap_or(0),
                        "personagens_liberados": liberado.get(&(c.id, e)).copied().unwrap_or(0),
                    })
                })
                .collect();
            json!({
                "id": c.id, "nome": c.nome, "tipo": format!("{:?}", c.tipo), "zona": c.zona, "nivel_min": c.nivel_min,
                "grupo_max": c.grupo_max, "limite_s": c.limite_s, "andares": c.andares, "disponivel": c.disponivel,
                "estagios": estagios,
            })
        })
        .collect();
    json!({
        "horas": horas,
        "catalogo": catalogo,
        "bau_itens": bau_itens.into_iter().take(60).map(|(id, n, v)| json!([id, n, v])).collect::<Vec<_>>(),
        "entradas_compradas": compradas,
        "chefes_mortos": chefes_mortos,
        "mobs_mortos": mobs,
        "personagens": {
            "com_dados": com_dados, "correio_pendente": correio, "correio_bolsa_cheia": correio_motivo[0],
            "correio_primeira_semanal": correio_motivo[1], "correio_primeira_de_todas": correio_motivo[2],
            "baus_abertos_lembrados": baus, "gruta_compradas_hoje": gruta_compradas_hoje,
        },
        "contas": { "com_dados": contas, "selos_na_semana": selos_semana, "primeiras_vitorias_na_semana": primeiras_semana },
    })
}

// ─────────────────────────────── missoes ───────────────────────────────

pub async fn missoes(pool: &PgPool, horas: i32) -> Value {
    let mut por: BTreeMap<u16, [i64; 3]> = BTreeMap::new();
    for r in linhas(
        pool,
        "SELECT quest_id, status::int, COUNT(*)::bigint FROM character_quests GROUP BY 1, 2",
    )
    .await
    {
        let id = campo::<i32>(&r, 0).clamp(0, u16::MAX as i32) as u16;
        let st = (campo::<i32>(&r, 1) as usize).min(2);
        por.entry(id).or_default()[st] += campo::<i64>(&r, 2);
    }
    let mut tele: BTreeMap<u16, [i64; 3]> = BTreeMap::new();
    for (tipo, chave, v) in somas(
        pool,
        "tipo IN ('missao_entregue', 'missao_historia', 'missao_abandonada')",
        horas,
    )
    .await
    {
        let Ok(id) = chave.parse::<u16>() else {
            continue;
        };
        let i = match tipo.as_str() {
            "missao_entregue" => 0,
            "missao_historia" => 1,
            _ => 2,
        };
        tele.entry(id).or_default()[i] += v;
    }
    let ids: std::collections::BTreeSet<u16> = por.keys().chain(tele.keys()).copied().collect();
    let lista: Vec<Value> = ids
        .into_iter()
        .map(|id| {
            let p = por.get(&id).copied().unwrap_or_default();
            let t = tele.get(&id).copied().unwrap_or_default();
            json!({
                "id": id, "titulo": titulo_da_missao(id), "categoria": categoria_da_missao(id), "capitulo": capitulo_de(id),
                "ativas": p[0], "prontas": p[1], "entregues_sempre": p[2],
                "entregues_janela": t[0] + t[1], "abandonadas_janela": t[2],
            })
        })
        .collect();
    // Onde a historia de cada um parou, por capitulo.
    let mut capitulos: BTreeMap<String, i64> = BTreeMap::new();
    let mut atual: HashMap<String, u16> = HashMap::new();
    for r in linhas(
        pool,
        "SELECT char_name, quest_id, status::int FROM character_quests",
    )
    .await
    {
        let id = campo::<i32>(&r, 1).clamp(0, u16::MAX as i32) as u16;
        if !e_historia(id) || campo::<i32>(&r, 2) as u8 == shared::quests::quest_status::TURNED_IN {
            continue;
        }
        let e = atual.entry(campo(&r, 0)).or_insert(0);
        *e = (*e).max(id);
    }
    for id in atual.values() {
        *capitulos
            .entry(capitulo_de(*id).unwrap_or("sem capítulo").to_string())
            .or_default() += 1;
    }
    json!({ "horas": horas, "lista": lista, "historia_por_capitulo": capitulos.into_iter().collect::<Vec<_>>() })
}

// ─────────────────────────────── itens ───────────────────────────────

pub async fn itens(pool: &PgPool, horas: i32) -> Value {
    use shared::item_id as i;
    let nomes = nomes_de_itens(pool).await;
    let tudo = "WITH tudo AS (
            SELECT item_id, qty::bigint AS qty, character_name, COALESCE(instance_data, '') AS inst FROM inventory
            UNION ALL SELECT item_id, qty::bigint, character_name, COALESCE(instance_data, '') FROM vault
            UNION ALL SELECT item_id, 1::bigint, character_name, COALESCE(instance_data, '') FROM equipment WHERE item_id IS NOT NULL
        )";
    let estoque: HashMap<i64, (i64, i64)> = linhas(
        pool,
        &format!("{tudo} SELECT item_id::bigint, SUM(qty)::bigint, COUNT(DISTINCT character_name)::bigint FROM tudo GROUP BY item_id"),
    )
    .await
    .iter()
    .map(|r| (campo::<i64>(r, 0), (campo::<i64>(r, 1), campo::<i64>(r, 2))))
    .collect();
    let item = |id: u16| {
        let (q, d) = estoque.get(&(id as i64)).copied().unwrap_or_default();
        json!({ "id": id, "nome": nomes.get(&(id as i64)).cloned().unwrap_or_else(|| format!("item {id}")), "qtd": q, "donos": d })
    };
    let chaves: Vec<Value> = i::todas_as_chaves().into_iter().map(item).collect();
    let especiais: Vec<Value> = [
        i::COPPER,
        i::DARKSTEEL,
        i::GLITTERING_POWDER,
        i::MARCAS_TEMPESTADE,
        i::SELO_TEMPESTADE,
        i::XP_POTION,
        i::FORTUNA_POTION,
        i::SORTE_POTION,
    ]
    .into_iter()
    .map(item)
    .collect();
    // Materiais por cor: a mesma escada que o craft pede.
    let materiais: Vec<Value> = i::MATERIAIS_COLORIDOS
        .iter()
        .map(|&base| {
            let cores: Vec<i64> = (1..=4)
                .map(|c| estoque.get(&(i::na_cor(base, c) as i64)).map_or(0, |e| e.0))
                .collect();
            // O id base e' o da cinza ("Aço Cinza"): o nome da linha e' o material.
            let nome = nomes.get(&(base as i64)).cloned().unwrap_or_default();
            let nome = nome
                .strip_suffix(" Cinza")
                .map(str::to_string)
                .unwrap_or(nome);
            json!({ "nome": nome, "cores": cores })
        })
        .collect();
    let raridade: Vec<Value> = linhas(
        pool,
        &format!(
            "{tudo} SELECT COALESCE(substring(inst from '\"rarity\"\\s*:\\s*(\\d+)'), '?'),
                           COUNT(*)::bigint,
                           COUNT(*) FILTER (WHERE inst ~ '\"vinculado\"\\s*:\\s*true')::bigint
                      FROM tudo WHERE inst <> '' GROUP BY 1 ORDER BY 1"
        ),
    )
    .await
    .iter()
    .map(|r| {
        json!([
            campo::<String>(r, 0),
            campo::<i64>(r, 1),
            campo::<i64>(r, 2)
        ])
    })
    .collect();
    let chaves_drop: Vec<Value> = somas(pool, "tipo = 'chave_drop'", horas)
        .await
        .into_iter()
        .map(|(_, chave, v)| {
            let (origem, id) = chave.split_once(':').unwrap_or(("?", &chave));
            let id: i64 = id.parse().unwrap_or(0);
            json!([
                origem,
                nomes
                    .get(&id)
                    .cloned()
                    .unwrap_or_else(|| format!("item {id}")),
                v
            ])
        })
        .collect();
    json!({ "horas": horas, "chaves": chaves, "especiais": especiais, "materiais": materiais, "raridade": raridade, "chaves_drop": chaves_drop })
}

// ─────────────────────────────── infra ───────────────────────────────

pub async fn infra(pool: &PgPool, central: Option<&PgPool>) -> Value {
    let canais: Vec<Value> = linhas(
        pool,
        "SELECT id, realm, host, players, capacity, zone, tick_p99_ms::float8, single,
                EXTRACT(EPOCH FROM NOW() - updated)::bigint
           FROM channels ORDER BY id",
    )
    .await
    .iter()
    .map(|r| {
        let idade = campo::<i64>(r, 8);
        json!({
            "id": campo::<String>(r, 0), "realm": campo::<String>(r, 1), "host": campo::<String>(r, 2),
            "jogadores": campo::<i32>(r, 3), "capacidade": campo::<i32>(r, 4), "zona": campo::<String>(r, 5),
            "tick_p99_ms": campo::<f64>(r, 6), "unico": campo::<bool>(r, 7), "idade_s": idade, "vivo": idade < 15,
        })
    })
    .collect();
    let migracoes: Vec<Value> = linhas(
        pool,
        "SELECT name, EXTRACT(EPOCH FROM applied_at)::bigint FROM economy_migrations
         UNION ALL SELECT nome, EXTRACT(EPOCH FROM feita)::bigint FROM migracoes_de_dados
         ORDER BY 2 DESC",
    )
    .await
    .iter()
    .map(|r| json!([campo::<String>(r, 0), campo::<i64>(r, 1)]))
    .collect();
    let versoes = uma(
        pool,
        "SELECT (SELECT version FROM economy_version WHERE id = 1)::bigint, (SELECT version FROM recipes_version LIMIT 1)::bigint",
    )
    .await;
    let telemetria = tem_tabela(pool, "telemetria").await;
    let erros: Vec<Value> = if telemetria {
        linhas(pool, "SELECT EXTRACT(EPOCH FROM quando)::bigint, canal, nivel, alvo, msg FROM telemetria_erros ORDER BY quando DESC LIMIT 150")
            .await
            .iter()
            .map(|r| json!([campo::<i64>(r, 0), campo::<String>(r, 1), campo::<String>(r, 2), campo::<String>(r, 3), campo::<String>(r, 4)]))
            .collect()
    } else {
        Vec::new()
    };
    let erros_24h: Vec<Value> = if telemetria {
        linhas(pool, "SELECT canal, nivel, COUNT(*)::bigint FROM telemetria_erros WHERE quando > NOW() - INTERVAL '24 hours' GROUP BY 1, 2 ORDER BY 1, 2")
            .await
            .iter()
            .map(|r| json!([campo::<String>(r, 0), campo::<String>(r, 1), campo::<i64>(r, 2)]))
            .collect()
    } else {
        Vec::new()
    };
    let rede: BTreeMap<String, i64> = somas(pool, "tipo IN ('save', 'save_ms', 'save_linhas', 'save_mercado', 'save_atrasado_mercado', 'conexao', 'banda', 'log')", 24)
        .await
        .into_iter()
        .map(|(t, c, v)| (if c.is_empty() { t } else { format!("{t}:{c}") }, v))
        .collect();
    let mut medidas: BTreeMap<String, Vec<(i64, f64)>> = BTreeMap::new();
    if telemetria {
        for r in linhas(
            pool,
            "SELECT canal || ' · ' || nome,
                    (EXTRACT(EPOCH FROM minuto)::bigint / 300 * 300)::bigint AS t, AVG(valor)::float8
               FROM telemetria_medidas WHERE minuto > NOW() - INTERVAL '24 hours'
              GROUP BY 1, 2 ORDER BY 2",
        )
        .await
        {
            medidas.entry(campo(&r, 0)).or_default().push((campo(&r, 1), campo(&r, 2)));
        }
    }
    let tamanhos = |p: &PgPool| {
        let p = p.clone();
        async move {
            let db = uma(
                &p,
                "SELECT current_database(), pg_database_size(current_database())::bigint",
            )
            .await;
            let tabelas: Vec<Value> = linhas(
                &p,
                "SELECT c.relname, pg_total_relation_size(c.oid)::bigint FROM pg_class c
                   JOIN pg_namespace n ON n.oid = c.relnamespace
                  WHERE n.nspname = 'public' AND c.relkind = 'r' ORDER BY 2 DESC LIMIT 25",
            )
            .await
            .iter()
            .map(|r| json!([campo::<String>(r, 0), campo::<i64>(r, 1)]))
            .collect();
            json!({
                "banco": db.as_ref().map(|r| campo::<String>(r, 0)),
                "bytes": db.as_ref().map_or(0, |r| campo::<i64>(r, 1)),
                "tabelas": tabelas,
            })
        }
    };
    json!({
        "painel": { "protocolo": shared::PROTOCOL_VERSION, "versao": env!("CARGO_PKG_VERSION") },
        "canais": canais,
        "migracoes": migracoes,
        "economy_version": versoes.as_ref().map_or(0, |r| campo::<i64>(r, 0)),
        "recipes_version": versoes.as_ref().map_or(0, |r| campo::<i64>(r, 1)),
        "telemetria_ligada": telemetria,
        "erros": erros,
        "erros_24h": erros_24h,
        "rede_e_saves_24h": rede,
        "medidas": medidas,
        "realm_db": tamanhos(pool).await,
        "central_db": match central { Some(c) => tamanhos(c).await, None => Value::Null },
    })
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn segundos_ou_milissegundos() {
        assert_eq!(para_segundos(1_789_000_000), 1_789_000_000);
        assert_eq!(para_segundos(1_789_000_000_123), 1_789_000_000);
        assert_eq!(para_segundos(0), 0);
    }

    #[test]
    fn categorias_de_missao() {
        assert_eq!(categoria_da_missao(shared::historia::ID_MARCO), "história");
        assert_eq!(categoria_da_missao(601), "diária");
        assert_eq!(categoria_da_missao(503), "mestre");
    }

    #[test]
    fn chave_de_dungeon() {
        assert_eq!(partes_de_dungeon("2:3"), Some((2, 3, None)));
        assert_eq!(
            partes_de_dungeon("2:3:vitoria"),
            Some((2, 3, Some("vitoria")))
        );
        assert_eq!(partes_de_dungeon("x"), None);
    }

    #[test]
    fn rotulos_resolvem_nomes() {
        let itens: HashMap<i64, String> = [(332, "Escama Cinza".to_string())].into();
        let receitas: HashMap<i64, String> = [(1000, "Espada".to_string())].into();
        let skills: HashMap<i64, String> = [(4, "Corte".to_string())].into();
        assert_eq!(
            rotulo("drop", "332", &itens, &receitas, &skills),
            "Escama Cinza"
        );
        assert_eq!(rotulo("drop", "9", &itens, &receitas, &skills), "item 9");
        assert_eq!(
            rotulo("chave_drop", "chefe:332", &itens, &receitas, &skills),
            "Escama Cinza · chefe"
        );
        assert_eq!(
            rotulo("craft", "1000", &itens, &receitas, &skills),
            "Espada"
        );
        assert_eq!(
            rotulo("skill_pedida", "4", &itens, &receitas, &skills),
            "Corte"
        );
        assert_eq!(rotulo("save_ms", "", &itens, &receitas, &skills), "total");
        assert_eq!(
            rotulo("ouro_fonte", "venda_npc", &itens, &receitas, &skills),
            "venda npc"
        );
        let d = rotulo(
            "dungeon_resultado",
            "1:2:tempo_esgotado",
            &itens,
            &receitas,
            &skills,
        );
        assert!(
            d.contains("estágio 2") && d.ends_with("tempo esgotado"),
            "{d}"
        );
    }
}

/// Loja de cash (docs/LOJA.md): receita dos pacotes de TP (simulada enquanto
/// o pagamento for simulado), TP vendida e gasta, pedidos por status, itens
/// mais comprados, ultimos pedidos e jogadores montados agora.
pub async fn loja(realm: &PgPool, central: Option<&PgPool>) -> Value {
    let montados = uma(
        realm,
        "SELECT COALESCE(SUM(valor), 0)::float8 FROM telemetria_medidas
          WHERE nome = 'montados' AND minuto = (SELECT MAX(minuto) FROM telemetria_medidas WHERE nome = 'montados')",
    )
    .await
    .map_or(0.0, |r| campo::<f64>(&r, 0));
    let Some(c) = central else {
        return json!({ "ligado": false, "montados": montados });
    };
    let nome =
        |cod: &str| shared::loja::Produto::de_codigo(cod).map_or(cod.to_string(), |p| p.nome());
    let receita = uma(
        c,
        "SELECT COALESCE(SUM(valor) FILTER (WHERE atualizado_em > NOW() - INTERVAL '24 hours'), 0)::bigint,
                COALESCE(SUM(valor), 0)::bigint, COUNT(*)::bigint
           FROM loja_pedidos WHERE tipo = 'tp' AND status = 'creditado'",
    )
    .await;
    let tp = uma(
        c,
        "SELECT COALESCE(SUM(delta) FILTER (WHERE motivo = 'loja:tp'), 0)::bigint,
                COALESCE(-SUM(delta) FILTER (WHERE motivo LIKE 'loja:item:%'), 0)::bigint
           FROM tp_razao",
    )
    .await;
    let status: Vec<Value> = linhas(
        c,
        "SELECT tipo, status, COUNT(*)::bigint, COALESCE(SUM(valor), 0)::bigint FROM loja_pedidos
          WHERE criado_em > NOW() - INTERVAL '30 days' GROUP BY 1, 2 ORDER BY 1, 2",
    )
    .await
    .iter()
    .map(|r| {
        json!([
            campo::<String>(r, 0),
            campo::<String>(r, 1),
            campo::<i64>(r, 2),
            campo::<i64>(r, 3)
        ])
    })
    .collect();
    let itens: Vec<Value> = linhas(
        c,
        "SELECT produto, COUNT(*)::bigint FROM loja_posses WHERE pedido NOT LIKE '%#padrao'
          GROUP BY 1 ORDER BY 2 DESC LIMIT 20",
    )
    .await
    .iter()
    .map(|r| json!([nome(&campo::<String>(r, 0)), campo::<i64>(r, 1)]))
    .collect();
    let pacotes: Vec<Value> = linhas(
        c,
        "SELECT produto, COUNT(*)::bigint, COALESCE(SUM(valor), 0)::bigint FROM loja_pedidos
          WHERE tipo = 'tp' AND status = 'creditado' GROUP BY 1 ORDER BY 2 DESC",
    )
    .await
    .iter()
    .map(|r| {
        json!([
            nome(&campo::<String>(r, 0)),
            campo::<i64>(r, 1),
            campo::<i64>(r, 2)
        ])
    })
    .collect();
    let por_hora: Vec<Value> = linhas(
        c,
        "SELECT EXTRACT(EPOCH FROM date_trunc('hour', atualizado_em))::bigint,
                COALESCE(SUM(valor) FILTER (WHERE tipo = 'tp'), 0)::bigint,
                COUNT(*) FILTER (WHERE tipo = 'item')::bigint
           FROM loja_pedidos
          WHERE atualizado_em > NOW() - INTERVAL '48 hours' AND status IN ('creditado', 'entregue')
          GROUP BY 1 ORDER BY 1",
    )
    .await
    .iter()
    .map(|r| json!([campo::<i64>(r, 0), campo::<i64>(r, 1), campo::<i64>(r, 2)]))
    .collect();
    let ultimos: Vec<Value> = linhas(
        c,
        "SELECT conta, produto, valor, moeda, status, provedor, EXTRACT(EPOCH FROM criado_em)::bigint
           FROM loja_pedidos ORDER BY criado_em DESC LIMIT 30",
    )
    .await
    .iter()
    .map(|r| {
        json!([
            campo::<String>(r, 0),
            nome(&campo::<String>(r, 1)),
            campo::<i64>(r, 2),
            campo::<String>(r, 3),
            campo::<String>(r, 4),
            campo::<String>(r, 5),
            campo::<i64>(r, 6)
        ])
    })
    .collect();
    json!({
        "ligado": true,
        "montados": montados,
        "receita_24h_centavos": receita.as_ref().map_or(0, |r| campo::<i64>(r, 0)),
        "receita_total_centavos": receita.as_ref().map_or(0, |r| campo::<i64>(r, 1)),
        "pedidos_tp_creditados": receita.as_ref().map_or(0, |r| campo::<i64>(r, 2)),
        "tp_vendida": tp.as_ref().map_or(0, |r| campo::<i64>(r, 0)),
        "tp_gasta": tp.as_ref().map_or(0, |r| campo::<i64>(r, 1)),
        "status": status,
        "itens": itens,
        "pacotes": pacotes,
        "por_hora": por_hora,
        "ultimos": ultimos,
    })
}
