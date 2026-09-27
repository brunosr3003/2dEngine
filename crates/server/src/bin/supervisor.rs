//! Supervisor de canais de UM servidor (realm).
//!
//! Abre e fecha canais sozinho conforme a populacao. A regra que o jogador
//! sente e' simples: nenhuma regiao fica apinhada. A regra que o operador
//! configura tambem:
//!
//!   * canal passou de `MMO_CANAL_ABRIR_EM` → abre mais um (ate `MMO_CANAIS_MAX`)
//!   * canal com o TICK apertado (p99 acima de `MMO_TICK_ALERTA`% do
//!     orcamento) conta como cheio mesmo com meia lotacao — cabeca contada nao
//!     distingue 60 pessoas espalhadas de 60 num boss
//!   * canal ficou vazio → fecha (ate sobrar `MMO_CANAIS_MIN`)
//!   * servidor bateu `MMO_REALM_CAPACIDADE` → para de abrir; quem chegar
//!     espera na fila do canal (ver `GameWorld::tick_fila`)
//!
//! **Area de canal unico** (`MMO_CANAL_UNICO=1`): cidade, arena, boss de
//! mundo. O supervisor mantem exatamente um processo e NUNCA abre um segundo —
//! abrir outra instancia da cidade destruiria o motivo de existir cidade.
//! Quando lota, a saida e' a fila.
//!
//! Um realm = um banco. Trocar de servidor nao e' reconectar, e' outro
//! personagem; so' um MERGE junta dois realms, e isso e' operacao de banco.
//!
//! ```sh
//! DATABASE_URL=... MAP_FILE=... MMO_REALM=SA01 \
//! MMO_CANAL_CAPACIDADE=60 MMO_CANAIS_MAX=8 ./supervisor
//! ```

use std::collections::HashMap;
use std::process::{Child, Command};
use std::time::Duration;

use sqlx::postgres::{PgPool, PgPoolOptions};

struct Canal {
    numero: u32,
    porta: u16,
    processo: Child,
    /// Quantas verificacoes seguidas ele apareceu vazio. Fechar no primeiro
    /// zero derrubaria canal que ficou um instante sem ninguem entre dois
    /// grupos entrando.
    vazio_ha: u32,
}

fn env_num<T: std::str::FromStr>(nome: &str, padrao: T) -> T {
    std::env::var(nome)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(padrao)
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
        )
        .init();

    let realm = std::env::var("MMO_REALM").unwrap_or_else(|_| "SA01".into());
    let db = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://solar:solar_dev_123@localhost:5432/mmo_dev".into());
    let bin = std::env::var("MMO_SERVER_BIN").unwrap_or_else(|_| "./server".into());

    let unico: bool = env_num::<u32>("MMO_CANAL_UNICO", 0) == 1;
    let min: u32 = if unico {
        1
    } else {
        env_num("MMO_CANAIS_MIN", 1)
    };
    let max: u32 = if unico {
        1
    } else {
        env_num("MMO_CANAIS_MAX", 8)
    };
    let cap_canal: u32 = env_num("MMO_CANAL_CAPACIDADE", 60);
    // Abre o proximo antes de encher de verdade: quem chega no canal cheio
    // esperaria na fila sem necessidade.
    let abrir_em: u32 = env_num("MMO_CANAL_ABRIR_EM", (cap_canal * 8 / 10).max(1));
    // 1500 nao e' numero redondo escolhido no olho: sai da medicao da VPS
    // (151.242.25.10, 16 nucleos, uplink de ~707 Mbit/s). Os tres limites
    // convergem em ~3.000-3.500 jogadores, mas a maquina hospeda outros
    // projetos — 1500 cabe com folga sem ameacar o resto. Ver
    // docs/SERVIDORES_E_CANAIS.md, secao "De onde sai a lotacao".
    let cap_realm: u32 = env_num("MMO_REALM_CAPACIDADE", 1500);
    let porta_base: u16 = env_num("MMO_PORTA_BASE", 9000);
    let host_base = std::env::var("MMO_HOST_PUBLICO_BASE").unwrap_or_else(|_| "127.0.0.1".into());
    // Endereco publico por CAMINHO, quando ha' um proxy na frente:
    // `MMO_CAMINHO_PUBLICO=/z/ilha_inicial` faz o canal 2 se anunciar como
    // `host/z/ilha_inicial/2` em vez de `host:9001`. Uma porta publica pro
    // arquipelago inteiro, em vez de uma por ilha — e TLS num lugar so'.
    // Vazio (o padrao) mantem o endereco por porta.
    let caminho = std::env::var("MMO_CAMINHO_PUBLICO").unwrap_or_default();
    let fechar_apos: u32 = env_num("MMO_CANAL_FECHAR_APOS", 6);
    // Fracao do orcamento de tick (33ms) acima da qual o canal conta como
    // cheio, independente de quanta gente tem dentro. 0,70 deixa margem pro
    // canal novo subir antes do atual comecar a atrasar de verdade — o
    // servidor pausa a propria admissao em 0,75.
    let tick_alerta: f32 = std::env::var("MMO_TICK_ALERTA")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(0.70);

    tracing::info!(
        "server {realm}: channels {min}..{max}{}, {cap_canal} per channel (opens at {abrir_em}), server cap {cap_realm}",
        if unico { " (SINGLE CHANNEL)" } else { "" }
    );

    let pool: PgPool = PgPoolOptions::new().max_connections(2).connect(&db).await?;

    let mut canais: Vec<Canal> = Vec::new();
    let mut tick = tokio::time::interval(Duration::from_secs(5));

    loop {
        tick.tick().await;

        // Processo que morreu sozinho sai da lista — o proximo ciclo reabre se
        // ainda for necessario. E' isso que faz o supervisor tambem servir de
        // reinicio automatico.
        canais.retain_mut(|c| match c.processo.try_wait() {
            Ok(Some(status)) => {
                tracing::warn!(
                    "channel {} died ({status}); it will be reopened if needed",
                    c.numero
                );
                false
            }
            _ => true,
        });

        let estado = estado_dos_canais(&pool, &realm).await;
        let pop: HashMap<u32, u32> = estado.iter().map(|(n, (p, _))| (*n, *p)).collect();
        let total: u32 = pop.values().sum();
        let vivos = canais.len() as u32;

        // ── abrir ────────────────────────────────────────────────────────
        // "Cheio" tem duas definicoes e as duas contam: gente dentro (o que o
        // jogador ve) e tick apertado (o que de fato quebra).
        let apertado = |n: &u32| -> bool {
            let (p, tick_ms) = estado.get(n).copied().unwrap_or((0, 0.0));
            p >= abrir_em || tick_ms / 33.33 >= tick_alerta
        };
        let todos_cheios = vivos == 0 || canais.iter().all(|c| apertado(&c.numero));
        // O piso (`min`) sobe de uma vez; a demanda sobe um por ciclo.
        //
        // Abrir um por ciclo tambem no piso custava 5s por canal: depois de um
        // restart com min=11 a zona levava ~55s pra ter todos os canais de
        // volta, e quem tentasse entrar num canal que ainda nao existia
        // simplesmente nao conectava. Ja' a abertura POR DEMANDA continua uma
        // por ciclo de proposito — um pico de entradas nao deve abrir seis
        // processos de uma vez.
        let alvo = if vivos < min {
            min
        } else if todos_cheios && vivos < max && total < cap_realm {
            vivos + 1
        } else {
            vivos
        };
        while (canais.len() as u32) < alvo {
            let numero = (1..)
                .find(|n| !canais.iter().any(|c| c.numero == *n))
                .unwrap();
            let porta = porta_base + numero as u16 - 1;
            match abrir(&bin, &realm, numero, porta, &host_base, &caminho, cap_canal, unico) {
                Ok(processo) => {
                    tracing::info!(
                        "channel {numero} opened on port {porta} ({total} players in total)"
                    );
                    canais.push(Canal {
                        numero,
                        porta,
                        processo,
                        vazio_ha: 0,
                    });
                }
                Err(e) => {
                    tracing::error!("failed to open channel {numero}: {e}");
                    break;
                }
            }
        }

        // ── fechar ───────────────────────────────────────────────────────
        if canais.len() as u32 > min {
            for c in canais.iter_mut() {
                if pop.get(&c.numero).copied().unwrap_or(0) == 0 {
                    c.vazio_ha += 1;
                } else {
                    c.vazio_ha = 0;
                }
            }
            // Fecha um por ciclo, o mais antigo que passou do limite — mas so'
            // se, DEPOIS de fechar, ainda sobrar canal com vaga.
            //
            // Sem essa condicao as duas regras se contradizem: com todos os
            // outros acima de `abrir_em`, fechar o canal vazio faz a regra de
            // abertura reabrir ele no ciclo seguinte. Medido em producao com
            // 1001 jogadores: abre, 30s vazio, fecha, reabre 5s depois, pra
            // sempre. O canal vazio ali NAO e' desperdicio — e' exatamente a
            // vaga que `abrir_em` existe pra garantir.
            let candidato = canais.iter().position(|c| c.vazio_ha >= fechar_apos);
            if let Some(i) = candidato {
                let sobra_vaga = canais
                    .iter()
                    .enumerate()
                    .any(|(j, c)| j != i && !apertado(&c.numero));
                if sobra_vaga {
                    let mut c = canais.remove(i);
                    tracing::info!(
                        "channel {} empty for {} cycles; closing",
                        c.numero,
                        c.vazio_ha
                    );
                    // SIGTERM: o servidor salva os personagens antes de sair.
                    let _ = c.processo.kill();
                    let _ = c.processo.wait();
                } else if canais[i].vazio_ha == fechar_apos {
                    tracing::info!(
                        "channel {} is empty, but it is the only slot on the server; keeping it open",
                        canais[i].numero
                    );
                }
            }
        }

        if total >= cap_realm {
            tracing::warn!(
                "server {realm} at capacity ({total}/{cap_realm}) — anyone arriving joins the queue"
            );
        }
    }
}

/// Populacao e saude por canal, direto da tabela que os proprios canais
/// alimentam.
///
/// O supervisor NAO fala com os canais: le' o mesmo heartbeat que o cliente
/// enxerga. Um jeito a menos de as duas visoes discordarem.
async fn estado_dos_canais(pool: &PgPool, realm: &str) -> HashMap<u32, (u32, f32)> {
    sqlx::query_as::<_, (String, i32, f32)>(
        "SELECT id, players, tick_p99_ms FROM channels
          WHERE realm = $1 AND updated > NOW() - INTERVAL '15 seconds'",
    )
    .bind(realm)
    .fetch_all(pool)
    .await
    .unwrap_or_default()
    .into_iter()
    .filter_map(|(id, n, tick)| {
        id.rsplit('/')
            .next()?
            .parse::<u32>()
            .ok()
            .map(|num| (num, (n.max(0) as u32, tick)))
    })
    .collect()
}

#[allow(clippy::too_many_arguments)]
fn abrir(
    bin: &str,
    realm: &str,
    numero: u32,
    porta: u16,
    host_base: &str,
    caminho: &str,
    cap_canal: u32,
    unico: bool,
) -> std::io::Result<Child> {
    let mut cmd = Command::new(bin);
    cmd.env("MMO_REALM", realm)
        // O filho anuncia isso no heartbeat; e' assim que a lista de canais
        // sabe distinguir "so' tem um canal agora" de "so' vai ter um".
        .env("MMO_CANAL_UNICO", if unico { "1" } else { "0" })
        .env("MMO_CANAL", numero.to_string())
        .env("BIND_ADDR", format!("0.0.0.0:{porta}"))
        .env("MMO_HOST_PUBLICO", endereco_publico(host_base, caminho, numero, porta))
        .env("MMO_CANAL_CAPACIDADE", cap_canal.to_string());
    // Retrato ao vivo pro panoptico: cada canal na porta do jogo + offset, so'
    // em loopback. Um PANOPTICO_BIND herdado igual pra todos faria os canais
    // brigarem pela mesma porta.
    if let Some(bind) = painel_do_canal(
        std::env::var("PANOPTICO_NOS_CANAIS").ok().as_deref(),
        std::env::var("PANOPTICO_OFFSET").ok().as_deref(),
        porta,
    ) {
        cmd.env("PANOPTICO_BIND", bind);
    }
    cmd.spawn()
}

/// Como o canal se anuncia pro CLIENTE — e' isto que vai no heartbeat e que o
/// `TrocarZona` devolve.
///
/// Sem `MMO_CAMINHO_PUBLICO`, e' `host:porta`: o cliente fala direto com o
/// processo. Com caminho, e' `host/caminho/numero`, e quem mapeia caminho ->
/// porta e' o proxy — a porta do jogo deixa de ser endereco publico e passa a
/// ser detalhe da maquina. O cliente nao sabe a diferenca: ele monta
/// `ws://{host}` com o que vier.
fn endereco_publico(host_base: &str, caminho: &str, numero: u32, porta: u16) -> String {
    if caminho.is_empty() {
        return format!("{host_base}:{porta}");
    }
    let caminho = caminho.trim_matches('/');
    format!("{host_base}/{caminho}/{numero}")
}

/// `PANOPTICO_BIND` de um canal: `127.0.0.1:(porta + offset)` quando
/// `PANOPTICO_NOS_CANAIS=1`. Offset padrao 1000, o mesmo que o painel assume.
fn painel_do_canal(ligado: Option<&str>, offset: Option<&str>, porta: u16) -> Option<String> {
    if ligado != Some("1") {
        return None;
    }
    let offset: u16 = offset.and_then(|v| v.parse().ok()).unwrap_or(1000);
    Some(format!("127.0.0.1:{}", porta.checked_add(offset)?))
}

#[cfg(test)]
mod testes_endereco {
    use super::endereco_publico;

    #[test]
    fn sem_caminho_e_porta() {
        assert_eq!(
            endereco_publico("mmo.brunji.com.br", "", 2, 9001),
            "mmo.brunji.com.br:9001"
        );
    }

    #[test]
    fn com_caminho_a_porta_some() {
        assert_eq!(
            endereco_publico("mmo.brunji.com.br", "/z/ilha_inicial", 1, 9000),
            "mmo.brunji.com.br/z/ilha_inicial/1"
        );
        assert_eq!(
            endereco_publico("mmo.brunji.com.br", "/z/ilha_inicial", 3, 9002),
            "mmo.brunji.com.br/z/ilha_inicial/3"
        );
    }

    /// Barra sobrando nao vira `//`: o cliente monta `ws://{host}` com isto
    /// cru, e `ws://host//z/...` nao bate com a location do nginx.
    #[test]
    fn barra_sobrando_nao_dobra() {
        assert_eq!(
            endereco_publico("mmo.brunji.com.br", "z/ilha_inicial/", 1, 9000),
            "mmo.brunji.com.br/z/ilha_inicial/1"
        );
    }
}

#[cfg(test)]
mod testes_painel {
    use super::painel_do_canal;

    #[test]
    fn porta_do_painel_por_canal() {
        assert_eq!(
            painel_do_canal(Some("1"), None, 9000).as_deref(),
            Some("127.0.0.1:10000")
        );
        assert_eq!(
            painel_do_canal(Some("1"), Some("500"), 9002).as_deref(),
            Some("127.0.0.1:9502")
        );
        assert_eq!(painel_do_canal(None, None, 9000), None);
        assert_eq!(painel_do_canal(Some("0"), None, 9000), None);
        assert_eq!(
            painel_do_canal(Some("1"), None, 65000),
            None,
            "estouro nao abre porta errada"
        );
    }
}
