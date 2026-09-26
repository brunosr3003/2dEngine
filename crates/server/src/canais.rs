//! Canais dentro de um SERVIDOR (realm).
//!
//! Sao dois niveis, e eles nao se misturam:
//!
//! **Servidor (realm)** — `SA01`, `BRASIL1`. Mundo proprio, BANCO proprio,
//! personagens proprios. Jogador de um servidor nao ve nem encontra jogador de
//! outro. So' se cruzam num MERGE, que e' operacao de banco, nao de rede. Tem
//! lotacao global: `MMO_REALM_CAPACIDADE`.
//!
//! **Canal** — dentro do servidor, uma instancia do mapa. Existe pra
//! distribuir gente e nao lotar regiao. Todos os canais de um servidor
//! compartilham o MESMO banco, entao trocar de canal e' reconectar, nao
//! recomecar. Abrem e fecham sozinhos conforme a populacao (ver o binario
//! `supervisor`).
//!
//! O que forca essa divisao: o world loop e' uma task so' e para num nucleo —
//! medido em ~400 jogadores nesta maquina. Canal resolve isso DENTRO do
//! servidor; servidor resolve capacidade total e latencia por regiao.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

use sqlx::postgres::PgPool;

/// Quantos jogadores estao dentro. Escrito pelo world loop, lido pelo
/// heartbeat — evita que a task de rede tenha que falar com o mundo.
#[derive(Clone, Default)]
pub struct Populacao(Arc<AtomicUsize>);

/// Saude do tick desta instancia: p99 do tempo de TRABALHO por tick, em
/// microssegundos.
///
/// Contar jogador e' um proxy, e um proxy ruim quando a carga nao e' uniforme:
/// 60 pessoas num boss com 300 mobs acordados nao e' a mesma coisa que 60
/// espalhadas, e o teto de populacao nao sabe distinguir. O que quebra de
/// verdade e' o tick nao fechar em 33ms — dai todo mundo lagga junto.
///
/// p99 e nao media: a media esconde exatamente o pico que estraga o combate.
#[derive(Clone, Default)]
pub struct Saude(Arc<std::sync::atomic::AtomicU32>);

impl Saude {
    pub fn set_p99_us(&self, us: u32) {
        self.0.store(us, Ordering::Relaxed);
    }

    pub fn p99_us(&self) -> u32 {
        self.0.load(Ordering::Relaxed)
    }

    /// Fracao do orcamento de tick consumida. `1.0` = o tick esta' estourando.
    pub fn carga(&self) -> f32 {
        self.p99_us() as f32 / (shared::TICK_DT * 1_000_000.0)
    }
}

/// Os chefes DESTE canal, pro heartbeat publicar. Mesma ideia da
/// `Populacao`: o world loop escreve, a task de rede le', e nenhuma das duas
/// precisa falar com a outra.
#[derive(Clone, Default)]
pub struct Chefes(Arc<std::sync::RwLock<Vec<shared::bosses::ChefeNoMapa>>>);

impl Chefes {
    pub fn set(&self, v: Vec<shared::bosses::ChefeNoMapa>) {
        if let Ok(mut g) = self.0.write() {
            *g = v;
        }
    }

    pub fn get(&self) -> Vec<shared::bosses::ChefeNoMapa> {
        self.0.read().map(|g| g.clone()).unwrap_or_default()
    }
}

/// Os chefes de TODAS as zonas do realm, pro mapa-mundi.
///
/// Chefe e' estado de PROCESSO: cada zona so' sabe dos seus. O mapa-mundi
/// mostra as quatro ilhas, entao o dado tem que cruzar processo — e o unico
/// caminho que ja' existe pra isso e' o banco, pela mesma ida que o heartbeat
/// ja' faz a cada 5s pro diretorio de canais. Uma consulta a mais na viagem
/// que ja' estava marcada.
#[derive(Clone, Default)]
pub struct MundoDeChefes(
    Arc<std::sync::RwLock<Vec<(String, String, Vec<shared::bosses::ChefeNoMapa>)>>>,
);

impl MundoDeChefes {
    fn set(&self, v: Vec<(String, String, Vec<shared::bosses::ChefeNoMapa>)>) {
        if let Ok(mut g) = self.0.write() {
            *g = v;
        }
    }

    /// Os chefes da zona `z`, do canal servido por `host`.
    ///
    /// O host importa: cada CANAL e' uma instancia do mapa com os chefes
    /// dela. Perguntar "os chefes da Geleira" sem dizer de qual canal daria
    /// a resposta de um canal qualquer — e o jogador iria pro outro.
    pub fn da_zona(&self, z: &str, host: Option<&str>) -> Option<Vec<shared::bosses::ChefeNoMapa>> {
        let g = self.0.read().ok()?;
        let achou = |h: Option<&str>| {
            g.iter()
                .find(|(zz, hh, _)| zz == z && h.is_none_or(|h| hh == h))
                .map(|(_, _, c)| c.clone())
        };
        // Sem canal no ar, a ultima noticia daquela zona ainda vale mais que
        // nada: o mapa mostra e diz que a ilha esta' fora do ar.
        achou(host).or_else(|| achou(None))
    }
}

impl Populacao {
    pub fn set(&self, n: usize) {
        self.0.store(n, Ordering::Relaxed);
    }

    pub fn get(&self) -> usize {
        self.0.load(Ordering::Relaxed)
    }
}

pub async fn init(pool: &PgPool) -> anyhow::Result<()> {
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS channels (
            id         TEXT PRIMARY KEY,
            realm      TEXT NOT NULL DEFAULT 'SA01',
            host       TEXT NOT NULL,
            players    INTEGER NOT NULL DEFAULT 0,
            capacity   INTEGER NOT NULL DEFAULT 0,
            map_name   TEXT NOT NULL DEFAULT '',
            zone       TEXT NOT NULL DEFAULT 'overworld',
            single     BOOLEAN NOT NULL DEFAULT FALSE,
            tick_p99_ms REAL NOT NULL DEFAULT 0,
            updated    TIMESTAMPTZ NOT NULL DEFAULT NOW()
        )",
    )
    .execute(pool)
    .await?;
    // Banco que ja tinha a tabela sem realm.
    sqlx::query("ALTER TABLE channels ADD COLUMN IF NOT EXISTS realm TEXT NOT NULL DEFAULT 'SA01'")
        .execute(pool)
        .await?;
    sqlx::query(
        "ALTER TABLE channels ADD COLUMN IF NOT EXISTS zone TEXT NOT NULL DEFAULT 'overworld'",
    )
    .execute(pool)
    .await?;
    // Area de instancia unica: quem le' a lista precisa saber que ali nao vai
    // abrir outro canal, vai dar fila. Antes o cliente adivinhava isso pelo
    // numero de canais visiveis — e errava sempre que a zona estava vazia.
    sqlx::query(
        "ALTER TABLE channels ADD COLUMN IF NOT EXISTS single BOOLEAN NOT NULL DEFAULT FALSE",
    )
    .execute(pool)
    .await?;
    // Os chefes por canal, pro mapa-mundi. Uma linha por CANAL, nao por
    // zona: dois canais da mesma ilha sao duas instancias, com chefes que
    // morrem e voltam em horas diferentes.
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS chefes_de_canal (
            canal      TEXT PRIMARY KEY,
            realm      TEXT NOT NULL DEFAULT 'SA01',
            zona       TEXT NOT NULL,
            host       TEXT NOT NULL,
            dados_json TEXT NOT NULL DEFAULT '[]',
            updated    TIMESTAMPTZ NOT NULL DEFAULT NOW()
        )",
    )
    .execute(pool)
    .await?;
    // Saude, nao so' lotacao: e' com isto que o supervisor decide abrir canal
    // por carga real em vez de so' por cabeca contada.
    sqlx::query(
        "ALTER TABLE channels ADD COLUMN IF NOT EXISTS tick_p99_ms REAL NOT NULL DEFAULT 0",
    )
    .execute(pool)
    .await?;
    // Onde ESTE canal atende o panoptico. Antes o painel deduzia isso do
    // `host` anunciado (porta do jogo + offset) — o que so' funcionava
    // enquanto o endereco publico terminava em `:porta`. Com a zona atras de
    // um proxy o endereco virou `host/z/ilha/1`, sem porta nenhuma, e o painel
    // perdeu o retrato de todos os canais de uma vez. O canal e' quem sabe
    // onde ele escuta; agora ele conta, em vez de deixar adivinharem.
    // Vazio = canal sem painel (nao subiu com `PANOPTICO_BIND`).
    sqlx::query("ALTER TABLE channels ADD COLUMN IF NOT EXISTS painel TEXT NOT NULL DEFAULT ''")
        .execute(pool)
        .await?;
    Ok(())
}

/// Diretorio de zonas do realm: qual host serve qual zona, e quao cheio esta.
///
/// Zona (cidade, campo, dungeon) roda em PROCESSO proprio. Quando o jogador
/// pisa num portal que aponta pra outra zona, o servidor precisa dizer PRA
/// ONDE ele reconecta — e essa resposta muda o tempo todo, porque canais abrem
/// e fecham. Por isso e' um diretorio vivo, alimentado pelo mesmo heartbeat
/// que o cliente ve, e nao configuracao estatica.
#[derive(Clone, Default)]
pub struct Diretorio(Arc<std::sync::RwLock<Vec<(String, String, i32)>>>);

impl Diretorio {
    #[cfg(test)]
    pub(crate) fn para_teste(zonas: &[&str]) -> Self {
        Self(Arc::new(std::sync::RwLock::new(
            zonas
                .iter()
                .map(|z| (z.to_string(), "127.0.0.1:9999".to_string(), 0))
                .collect(),
        )))
    }
    /// Host menos cheio que serve `zona`. `None` = zona fora do ar; quem
    /// chama tem que tratar (o jogador nao pode sumir num portal quebrado).
    pub fn melhor(&self, zona: &str) -> Option<String> {
        let g = self.0.read().ok()?;
        g.iter()
            .filter(|(z, _, _)| z == zona)
            .min_by_key(|(_, _, n)| *n)
            .map(|(_, h, _)| h.clone())
    }

    fn set(&self, novo: Vec<(String, String, i32)>) {
        if let Ok(mut g) = self.0.write() {
            *g = novo;
        }
    }
}

/// Zona servida por ESTE processo.
pub fn zona() -> String {
    std::env::var("MMO_ZONA").unwrap_or_else(|_| "overworld".into())
}

/// Nome deste servidor. Um realm = um banco; o nome e' so' a etiqueta que o
/// jogador ve na lista.
pub fn realm() -> String {
    std::env::var("MMO_REALM").unwrap_or_else(|_| "SA01".into())
}

/// Anuncia este processo e mantem o batimento.
///
/// A lista de canais e' derivada do `updated`: quem parou de bater some da
/// lista sozinho, sem ninguem precisar limpar. Servidor que caiu nao aparece
/// como opcao.
pub fn spawn_heartbeat(
    pool: PgPool,
    pop: Populacao,
    saude: Saude,
    dir: Diretorio,
    chefes: Chefes,
    mundo: MundoDeChefes,
) {
    let id_curto = std::env::var("MMO_CANAL").unwrap_or_else(|_| "1".into());
    let realm_id = realm();
    let realm = realm_id.clone();
    // A chave e' realm+canal: dois servidores podem ter "canal-1" sem colidir.
    let id = format!("{realm}/{id_curto}");
    // Endereco que o CLIENTE usa. Nao da pra deduzir do BIND_ADDR: o servidor
    // escuta em 0.0.0.0 e o cliente precisa de um host roteavel.
    let host = std::env::var("MMO_HOST_PUBLICO")
        .unwrap_or_else(|_| std::env::var("BIND_ADDR").unwrap_or_else(|_| "127.0.0.1:9000".into()));
    // Teto do CANAL — quantos cabem numa instancia do mapa antes da regiao
    // ficar apinhada. E' menor que o teto tecnico do processo de proposito:
    // canal cheio nao trava, so' fica ruim de jogar.
    let capacity: i32 = std::env::var("MMO_CANAL_CAPACIDADE")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(300);
    let map_name = std::env::var("MAP_FILE").unwrap_or_else(|_| "game".into());
    // Cidade, arena, boss de mundo: abrir uma segunda instancia destruiria o
    // motivo de existirem. O supervisor repassa a variavel pros filhos.
    let unico: bool = std::env::var("MMO_CANAL_UNICO").as_deref() == Ok("1");
    let zona_local = zona();
    // `0.0.0.0` e' onde se ESCUTA, nao um endereco pra onde falar — o painel
    // esta' sempre na mesma maquina que o canal.
    let painel = std::env::var("PANOPTICO_BIND")
        .unwrap_or_default()
        .replace("0.0.0.0:", "127.0.0.1:");

    tokio::spawn(async move {
        let mut tick = tokio::time::interval(Duration::from_secs(5));
        loop {
            tick.tick().await;
            let r = sqlx::query(
                "INSERT INTO channels (id, realm, host, players, capacity, map_name, zone, single, tick_p99_ms, painel, updated)
                 VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, NOW())
                 ON CONFLICT (id) DO UPDATE SET
                    realm = EXCLUDED.realm,
                    host = EXCLUDED.host,
                    players = EXCLUDED.players,
                    capacity = EXCLUDED.capacity,
                    map_name = EXCLUDED.map_name,
                    zone = EXCLUDED.zone,
                    single = EXCLUDED.single,
                    tick_p99_ms = EXCLUDED.tick_p99_ms,
                    painel = EXCLUDED.painel,
                    updated = NOW()",
            )
            .bind(&id)
            .bind(&realm)
            .bind(&host)
            .bind(pop.get() as i32)
            .bind(capacity)
            .bind(&map_name)
            .bind(&zona_local)
            .bind(unico)
            .bind(saude.p99_us() as f32 / 1000.0)
            .bind(&painel)
            .execute(&pool)
            .await;
            if let Err(e) = r {
                tracing::warn!("heartbeat do canal '{id}': {e}");
            }

            // Os chefes DESTE canal, pro mapa-mundi de quem estiver noutra
            // ilha. Zona sem chefe (a colonia, a dungeon) grava lista vazia,
            // e e' isso que distingue "sem chefe" de "fora do ar".
            let meus = chefes.get();
            let json = serde_json::to_string(&meus).unwrap_or_else(|_| "[]".into());
            let _ = sqlx::query(
                "INSERT INTO chefes_de_canal (canal, realm, zona, host, dados_json, updated)
                 VALUES ($1, $2, $3, $4, $5, NOW())
                 ON CONFLICT (canal) DO UPDATE SET
                    realm = EXCLUDED.realm,
                    zona = EXCLUDED.zona,
                    host = EXCLUDED.host,
                    dados_json = EXCLUDED.dados_json,
                    updated = NOW()",
            )
            .bind(&id)
            .bind(&realm)
            .bind(&zona_local)
            .bind(&host)
            .bind(&json)
            .execute(&pool)
            .await;
            if let Ok(linhas) = sqlx::query_as::<_, (String, String, String)>(
                "SELECT zona, host, dados_json FROM chefes_de_canal
                  WHERE realm = $1 AND updated > NOW() - INTERVAL '60 seconds'",
            )
            .bind(&realm)
            .fetch_all(&pool)
            .await
            {
                mundo.set(
                    linhas
                        .into_iter()
                        .map(|(z, h, j)| (z, h, serde_json::from_str(&j).unwrap_or_default()))
                        .collect(),
                );
            }

            // Mesma ida ao banco atualiza o diretorio de zonas do realm.
            if let Ok(linhas) = sqlx::query_as::<_, (String, String, i32)>(
                "SELECT zone, host, players FROM channels
                  WHERE realm = $1 AND updated > NOW() - INTERVAL '15 seconds'",
            )
            .bind(&realm)
            .fetch_all(&pool)
            .await
            {
                dir.set(linhas);
            }
        }
    });
    tracing::info!(
        "servidor {} / canal {} anunciado",
        realm_id,
        std::env::var("MMO_CANAL").unwrap_or_else(|_| "1".into())
    );
}
