//! RANKINGS (`shared::ranking`): power and level, overall or per class.
//!
//! Built from the realm's database, not from this process's memory: each
//! channel/island runs in its own process and its in-memory copy of the
//! characters goes stale as soon as someone plays elsewhere. Power is not
//! stored, so it is recomputed with the same `effective_stats` the game uses
//! and the sheet's formula (`dungeon::poder_de_stats`). The whole table is
//! cached for `VIDA_DO_CACHE` — one rebuild a minute at most, whoever asks.

use super::*;
use shared::ranking::{self as rk, LinhaRanking, Ranking, TipoRanking};
use shared::skills::Conjunto;
use std::sync::Mutex;
use std::time::{Duration, Instant};

const VIDA_DO_CACHE: Duration = Duration::from_secs(60);
/// One page per session per this long (tab spam doesn't hit the DB anyway,
/// but it would flood the socket).
const INTERVALO_MIN: Duration = Duration::from_millis(400);

struct Cache {
    feito: Instant,
    linhas: Vec<LinhaRanking>,
}

static CACHE: Mutex<Option<Cache>> = Mutex::new(None);
static PEDIDOS: Mutex<Option<HashMap<SessionId, Instant>>> = Mutex::new(None);

/// Every rankable character of the realm (unordered), with level, power and
/// class. Staff accounts are left out.
async fn monta(pool: &sqlx::PgPool) -> anyhow::Result<Vec<LinhaRanking>> {
    let todos = crate::persistence::load_all(pool).await?;
    let staff: Vec<i64> = sqlx::query_scalar("SELECT account_id FROM social_staff")
        .fetch_all(pool)
        .await
        .unwrap_or_default();
    let mult = crate::economy::xp_multiplier();
    Ok(todos
        .into_values()
        .filter(|c| !c.account_id.is_some_and(|a| staff.contains(&a)))
        .map(|c| {
            let stats = effective_stats(&c.equipment, &c.allocated_points, &c.proficiencies, c.xp);
            LinhaRanking {
                posicao: 0,
                nivel: shared::level_of_xp_with_mult(c.xp, mult),
                poder: shared::dungeon::poder_de_stats(&stats),
                classe: Conjunto::da_arma(c.equipment.weapon.unwrap_or(0)) as u8,
                nome: c.name,
            }
        })
        .collect())
}

/// The cached table, rebuilt when older than a minute.
async fn tabela(pool: &sqlx::PgPool) -> anyhow::Result<Vec<LinhaRanking>> {
    if let Some(c) = CACHE.lock().unwrap().as_ref() {
        if c.feito.elapsed() < VIDA_DO_CACHE {
            return Ok(c.linhas.clone());
        }
    }
    let linhas = monta(pool).await?;
    *CACHE.lock().unwrap() = Some(Cache { feito: Instant::now(), linhas: linhas.clone() });
    Ok(linhas)
}

/// One page: filtered by class, ordered, the top `rk::TOPO` plus `eu`.
pub(crate) fn pagina(mut todas: Vec<LinhaRanking>, tipo: TipoRanking, classe: Option<u8>, eu: &str) -> Ranking {
    if let Some(c) = classe {
        todas.retain(|l| l.classe == c);
    }
    rk::ordena(&mut todas, tipo);
    let minha = todas.iter().find(|l| l.nome == eu).cloned();
    let total = todas.len() as u32;
    todas.truncate(rk::TOPO);
    Ranking { tipo, classe, linhas: todas, eu: minha, total }
}

impl GameWorld {
    pub(super) fn handle_ranking(&mut self, sid: SessionId, tipo: TipoRanking, classe: Option<u8>) {
        let Some(s) = self.sessions.get(&sid).filter(|s| s.logged_in) else {
            return;
        };
        {
            let mut p = PEDIDOS.lock().unwrap();
            let mapa = p.get_or_insert_with(HashMap::new);
            if mapa.get(&sid).is_some_and(|t| t.elapsed() < INTERVALO_MIN) {
                return;
            }
            if mapa.len() > 10_000 {
                mapa.retain(|_, t| t.elapsed() < Duration::from_secs(60));
            }
            mapa.insert(sid, Instant::now());
        }
        let classe = classe.filter(|c| Conjunto::de_u8(*c).is_some());
        let Some(ctx) = &self.auth_ctx else {
            return;
        };
        let pool = ctx.pool.clone();
        let to = s.handle.to_client.clone();
        let nome = s.name.clone();
        tokio::spawn(async move {
            match tabela(&pool).await {
                Ok(t) => {
                    let ranking = pagina(t, tipo, classe, &nome);
                    let _ = to.send(ServerMessage::Ranking { ranking });
                }
                Err(e) => tracing::warn!("ranking: falhou: {e:#}"),
            }
        });
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    fn l(nome: &str, nivel: u32, poder: i32, classe: u8) -> LinhaRanking {
        LinhaRanking { posicao: 0, nome: nome.into(), nivel, poder, classe }
    }

    #[test]
    fn pagina_filtra_por_classe_e_acha_o_jogador_fora_do_topo() {
        let mut v: Vec<LinhaRanking> = (0..80).map(|i| l(&format!("p{i:02}"), i, i as i32 * 10, (i % 2) as u8)).collect();
        v.push(l("eu", 0, -1, 1));
        let r = pagina(v.clone(), TipoRanking::Poder, None, "eu");
        assert_eq!(r.linhas.len(), rk::TOPO);
        assert_eq!(r.total, 81);
        assert_eq!(r.linhas[0].nome, "p79");
        assert_eq!(r.eu.as_ref().map(|x| x.posicao), Some(81));
        let k = pagina(v, TipoRanking::Nivel, Some(1), "eu");
        assert!(k.linhas.iter().all(|x| x.classe == 1));
        assert_eq!(k.total, 41);
    }
}
