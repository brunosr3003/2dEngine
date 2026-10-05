//! Olho de cima: o estado INTEIRO desta instancia, sem campo de visao.
//!
//! O cliente do jogo ve' 60 entidades por AOI porque banda e' cara e porque
//! saber onde esta' todo mundo E' vantagem. O panoptico e' o contrario disso
//! de proposito: ele existe pra quem opera o jogo enxergar tudo — cada
//! jogador, cada bicho, o que cada um esta' fazendo — e por isso mesmo NUNCA
//! pode ficar exposto junto com o jogo.
//!
//! Tres garantias, e as tres sao estruturais e nao disciplina:
//!
//!   1. **So' leitura.** Nao ha' rota que escreva. O modulo nao tem `&mut
//!      GameWorld` em lugar nenhum.
//!   2. **Fora do laco.** O tick nao serve HTTP: ele PUBLICA um retrato ja'
//!      serializado num `RwLock`, e quem atende a requisicao so' clona um
//!      `Arc<str>`. Requisicao lenta, cliente travado ou dez abas abertas nao
//!      encostam no laco do mundo.
//!   3. **Fechado por padrao.** Sem `PANOPTICO_BIND` no ambiente, o servidor
//!      nao abre porta nenhuma. Com ela, exige o token de `MMO_ADMIN_TOKEN`.

use std::sync::Arc;

use hecs::Entity;
use serde::Serialize;

use crate::world::{EnemyTag, GameWorld, NetId};
use shared::{EntityKind, Health, Position, Velocity};

/// O ultimo retrato publicado, ja' em JSON.
///
/// Guardar TEXTO e nao a estrutura e' o que mantem o `RwLock` sem disputa: o
/// tick serializa uma vez a cada 200 ms com o cadeado solto, e o cadeado so'
/// e' pego pra trocar um ponteiro.
static PANORAMA: parking_lot::RwLock<Option<Arc<str>>> = parking_lot::RwLock::new(None);

/// De quanto em quanto tempo o retrato e' refeito.
///
/// Cinco por segundo. O tick e' 30Hz; publicar em todos seria serializar mil
/// entidades trinta vezes por segundo pra alimentar uma tela que o olho le' a
/// cinco. O painel interpola o resto.
const INTERVALO_S: f32 = 0.2;

#[derive(Serialize)]
struct Panorama {
    canal: String,
    zona: String,
    /// Semente e raio da ilha: e' o que o painel precisa pra desenhar o mapa.
    semente: i32,
    raio_blocos: i32,
    bioma: String,
    tempo_s: f32,
    tick: u32,
    imortal: bool,
    jogadores: Vec<Jogador>,
    mobs: Vec<Mob>,
    estorvos: usize,
    /// Versao do protocolo e do crate: o painel mostra canal com build velho.
    build: Build,
    uptime_s: u64,
    /// Chefes de campo: vivo ou quando volta.
    chefes: Vec<ChefeDeCampo>,
    /// Dungeons abertas agora neste canal.
    instancias: Vec<InstanciaAoVivo>,
    /// Fila automatica, salas e pronto-checks.
    mesa: MesaAoVivo,
    /// Ultimas linhas WARN/ERROR deste processo.
    erros: Vec<crate::telemetria::Erro>,
}

#[derive(Serialize)]
struct Build {
    protocolo: u16,
    versao: &'static str,
}

#[derive(Serialize)]
struct ChefeDeCampo {
    kind: u16,
    nome: String,
    nivel: u32,
    x: f32,
    z: f32,
    vivo: bool,
    /// Segundos ate' renascer (0 = vivo).
    renasce_em_s: f32,
    hp: i32,
    hp_max: i32,
}

#[derive(Serialize)]
struct InstanciaAoVivo {
    id: u32,
    conteudo: u16,
    nome: String,
    estagio: u8,
    andar: u8,
    andares: u8,
    estado: &'static str,
    decorrido_s: f32,
    restante_s: f32,
    inimigos_vivos: usize,
    wipes: u32,
    membros: Vec<MembroAoVivo>,
}

#[derive(Serialize)]
struct MembroAoVivo {
    nome: String,
    mortes: u32,
    ajudante: bool,
    saiu: bool,
    abriu_bau: bool,
}

#[derive(Serialize)]
struct MesaAoVivo {
    /// (conteudo, estagio, segundos esperando).
    fila: Vec<(u16, u8, f32)>,
    /// (id, conteudo, estagio, membros, completar pela fila).
    salas: Vec<(u32, u16, u8, usize, bool)>,
    prontos: usize,
}

#[derive(Serialize)]
struct Jogador {
    id: u32,
    nome: String,
    conta: Option<i64>,
    sessao: String,
    nivel: u32,
    xp: u64,
    ouro: u64,
    x: f32,
    z: f32,
    /// Altura do chao sob ele. So' pra leitura humana — o mapa e' de cima.
    y: f32,
    vx: f32,
    vz: f32,
    hp: i32,
    hp_max: i32,
    mp: i32,
    mp_max: i32,
    stamina: i32,
    stamina_max: i32,
    /// Uma palavra pro estado dominante. A ordem da precedencia esta' no
    /// codigo, e ela e' a mesma que o jogador sente: caido ganha de tudo.
    estado: &'static str,
    alvo: Option<u32>,
    /// Quantos pontos de rota ainda faltam. Zero = andando no teclado.
    rota: usize,
    /// Pra onde a rota vai, quando ha' rota.
    destino: Option<[f32; 2]>,
    forca: i32,
    defesa: i32,
    dano: i32,
    arma: Option<u16>,
    /// O que o LUGAR esta' rendendo de coleta pra ele agora. `None` = lugar
    /// sem recurso nenhum. E' a unica janela pra um sistema que nao tem UI:
    /// a coleta e' automatica e silenciosa, e sem isto so' se enxerga o
    /// inventario crescendo.
    coleta: Option<Coleta>,
    /// Buffs ligados: segundos que ainda faltam (0 = desligado).
    buff_xp_s: i64,
    buff_fortuna_s: i64,
    buff_sorte_s: i64,
    /// Mortes com XP ainda recuperavel e revives gratis usados hoje.
    mortes_recuperaveis: usize,
    recuperacoes_usadas: u32,
    /// Preferencias que dizem como a pessoa joga.
    skills_auto: Vec<u32>,
    economia_auto_min: Option<u16>,
    /// Instancia de dungeon em que esta' (0 = mundo aberto).
    instancia: u32,
    /// Banda desta conexao desde que entrou.
    banda_enviados: u64,
    banda_recebidos: u64,
    banda_kbps: f64,
}

#[derive(Serialize)]
struct Coleta {
    /// Pedras VIVAS no raio, por tier: [_, cinza, verde, azul, roxo].
    pedras: [u32; 5],
    /// Troncos vivos no raio.
    troncos: u32,
    /// Cristais de Energia vivos no raio.
    energias: u32,
    /// Corpos vivos no raio — e' ela que divide `COLETA_INTERVALO_BASE_S`.
    densidade: f32,
    /// Segundos por coleta aqui, agora.
    intervalo_s: f32,
    /// Quanto ainda sai deste spot antes de ele acabar, em coletas.
    coletas_restantes: u32,
}

#[derive(Serialize)]
struct Mob {
    id: u32,
    nome: String,
    kind: u16,
    nivel: u32,
    x: f32,
    z: f32,
    y: f32,
    vx: f32,
    vz: f32,
    hp: i32,
    hp_max: i32,
    estado: &'static str,
    /// Jogador mais proximo em CACHE. Ter alvo nao quer dizer perseguir: a IA
    /// guarda o mais perto pra nao varrer todos os jogadores por tick, e so'
    /// persegue dentro do alcance de deteccao.
    alvo: Option<u32>,
    /// Esta' de fato indo atras. E' este que vira linha no mapa.
    ///
    /// A primeira versao do painel desenhava uma linha pra todo mob com
    /// `alvo`, e a tela mostrou 54 bichos "cacando" um jogador a 124 unidades
    /// com deteccao de 9. A IA estava certa; a leitura e' que estava errada —
    /// e um painel que mente e' pior que painel nenhum, porque da' confianca.
    perseguindo: bool,
    chefe: bool,
    /// Casa do bicho e raio da coleira: e' o que explica um mob "voltando".
    casa: [f32; 2],
    coleira: f32,
    dano: i32,
    defesa: i32,
    alcance: f32,
    deteccao: f32,
    xp: u64,
}

/// Refaz o retrato, se ja' passou o intervalo. Chamada do laco do mundo.
pub fn publicar(w: &GameWorld, ultima: &mut f32) {
    if !ativo() || w.sim_time_s - *ultima < INTERVALO_S {
        return;
    }
    *ultima = w.sim_time_s;
    let chao = |p: glam::Vec2| w.ilha.as_ref().map_or(0.0, |i| i.altura(p.x, p.y));

    let agora_unix = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as i64);
    let mut jogadores = Vec::with_capacity(w.sessions.len());
    for (sid, s) in &w.sessions {
        let Some(e) = s.entity else { continue };
        let (pos, vel) = corpo(w, e);
        let (hp, hp_max) = vida(w, e);
        let banda = crate::telemetria::banda_de(&sid.0.to_string());
        jogadores.push(Jogador {
            id: s.entity_id.0,
            nome: s.name.clone(),
            conta: s.account_id,
            sessao: sid.0.to_string(),
            nivel: shared::level_of_xp(s.xp),
            xp: s.xp,
            ouro: s.gold,
            x: pos.x,
            z: pos.y,
            y: chao(pos),
            vx: vel.x,
            vz: vel.y,
            hp,
            hp_max,
            mp: s.mp_current as i32,
            mp_max: s.stats.mp_max,
            stamina: s.stamina_current as i32,
            stamina_max: s.stats.stamina_max,
            estado: estado_do_jogador(w, s),
            alvo: s.target.map(|t| t.0),
            rota: s.rota.restantes(),
            destino: (!s.rota.vazia()).then(|| [s.rota.destino().x, s.rota.destino().y]),
            forca: s.stats.attack_damage,
            defesa: s.stats.defense,
            dano: s.stats.attack_damage,
            arma: s.equipment.weapon,
            coleta: w.retrato_da_coleta(pos).map(|r| Coleta {
                pedras: r.pedras,
                troncos: r.troncos,
                energias: r.energias,
                densidade: r.densidade,
                intervalo_s: r.intervalo_s,
                coletas_restantes: r.coletas_restantes,
            }),
            buff_xp_s: (s.xp_bonus_ate - agora_unix).max(0),
            buff_fortuna_s: (s.fortuna_ate - agora_unix).max(0),
            buff_sorte_s: (s.sorte_ate - agora_unix).max(0),
            mortes_recuperaveis: s.mortes.len(),
            recuperacoes_usadas: s.recuperacoes_usadas,
            skills_auto: s.preferencias.skills_auto.clone(),
            economia_auto_min: s.preferencias.economia_auto_min,
            instancia: s.instancia,
            banda_enviados: banda.map_or(0, |b| b.0),
            banda_recebidos: banda.map_or(0, |b| b.1),
            banda_kbps: banda.map_or(0.0, |(env, rec, seg)| {
                (env + rec) as f64 * 8.0 / 1000.0 / seg.max(1.0)
            }),
        });
    }

    // Posicao dos jogadores por id: e' o que permite dizer se um mob esta'
    // perseguindo de verdade ou so' tem o cache preenchido.
    let onde: std::collections::HashMap<u32, glam::Vec2> = jogadores
        .iter()
        .map(|j| (j.id, glam::Vec2::new(j.x, j.z)))
        .collect();
    let mut mobs = Vec::new();
    for (e, (net, pos, vel, tag)) in w
        .ecs
        .query::<(&NetId, &Position, &Velocity, &EnemyTag)>()
        .iter()
    {
        let (hp, hp_max) = vida(w, e);
        // Perseguir e' alvo DENTRO do alcance de deteccao, e nao voltando pra
        // casa. Falta a linha de visao — na ilha ela nunca barra, porque
        // relevo nao e' tile de parede.
        let perseguindo = !tag.dead
            && !tag.returning_home
            && tag
                .ai_target
                .and_then(|t| onde.get(&t.0))
                .is_some_and(|p| p.distance(pos.0) < tag.detect_range);
        let nome = w
            .ecs
            .get::<&EntityKind>(e)
            .ok()
            .and_then(|k| match *k {
                EntityKind::Enemy(kind) => Some(crate::economy::enemy_def(kind).name.clone()),
                _ => None,
            })
            .unwrap_or_else(|| "?".into());
        let kind = match w.ecs.get::<&EntityKind>(e).map(|k| *k) {
            Ok(EntityKind::Enemy(k)) => k,
            _ => 0,
        };
        mobs.push(Mob {
            id: net.0 .0,
            nome,
            kind,
            nivel: tag.level,
            x: pos.0.x,
            z: pos.0.y,
            y: chao(pos.0),
            vx: vel.0.x,
            vz: vel.0.y,
            hp,
            hp_max,
            estado: estado_do_mob(w, tag, perseguindo),
            alvo: tag.ai_target.map(|t| t.0),
            perseguindo,
            chefe: tag.is_boss,
            casa: [tag.spawn_anchor.x, tag.spawn_anchor.y],
            coleira: tag.leash_max,
            dano: tag.stats.attack_damage,
            defesa: tag.stats.defense,
            alcance: tag.attack_range,
            deteccao: tag.detect_range,
            xp: tag.xp_reward,
        });
    }

    let chefes = w
        .vagas_de_chefe
        .iter()
        .map(|v| {
            let c = shared::bosses::chefe(v.kind);
            let (hp, hp_max) = v.vivo.map_or((0, 0), |e| vida(w, e));
            let pos = v.vivo.map_or(v.pos, |e| corpo(w, e).0);
            ChefeDeCampo {
                kind: v.kind,
                nome: c.map_or_else(|| format!("kind {}", v.kind), |c| c.nome.to_string()),
                nivel: c.map_or(0, |c| c.nivel),
                x: pos.x,
                z: pos.y,
                vivo: v.vivo.is_some(),
                renasce_em_s: if v.vivo.is_some() {
                    0.0
                } else {
                    (v.volta_em - w.sim_time_s).max(0.0)
                },
                hp,
                hp_max,
            }
        })
        .collect();
    let instancias = w
        .instancias
        .iter()
        .map(|i| {
            let c = shared::dungeon::conteudo(i.conteudo);
            use crate::world::dungeon::EstadoDg;
            InstanciaAoVivo {
                id: i.id,
                conteudo: i.conteudo,
                nome: c.map_or_else(
                    || format!("conteudo {}", i.conteudo),
                    |c| c.nome.to_string(),
                ),
                estagio: i.estagio,
                andar: i.andar,
                andares: c.map_or(0, |c| c.andares),
                estado: match i.estado {
                    EstadoDg::Andando => "andando",
                    EstadoDg::Concluida { .. } => "vencida",
                    EstadoDg::Falhou { .. } => "falhou",
                },
                decorrido_s: (w.sim_time_s - i.inicio).max(0.0),
                restante_s: (i.limite - w.sim_time_s).max(0.0),
                inimigos_vivos: i.vivos.iter().filter(|e| vida(w, **e).0 > 0).count(),
                wipes: i.wipes,
                membros: i
                    .membros
                    .iter()
                    .map(|m| MembroAoVivo {
                        nome: m.nome.clone(),
                        mortes: m.mortes,
                        ajudante: m.ajudante,
                        saiu: m.saiu,
                        abriu_bau: m.abriu_bau,
                    })
                    .collect(),
            }
        })
        .collect();
    let agora_mesa = w.sim_time_s as f64;
    let mesa = MesaAoVivo {
        fila: w
            .mesa
            .fila
            .iter()
            .map(|f| {
                (
                    f.conteudo,
                    f.estagio,
                    (agora_mesa - f.desde).max(0.0) as f32,
                )
            })
            .collect(),
        salas: w
            .mesa
            .salas
            .iter()
            .map(|s| {
                (
                    s.id,
                    s.conteudo,
                    s.estagio,
                    s.membros.len(),
                    s.completar_pela_fila,
                )
            })
            .collect(),
        prontos: w.mesa.prontos.len(),
    };
    let def = shared::terreno::def_da_zona(&w.zona);
    let p = Panorama {
        build: Build {
            protocolo: shared::PROTOCOL_VERSION,
            versao: env!("CARGO_PKG_VERSION"),
        },
        uptime_s: crate::telemetria::uptime_s(),
        chefes,
        instancias,
        mesa,
        erros: crate::telemetria::erros_recentes(),
        canal: std::env::var("MMO_CANAL").unwrap_or_else(|_| "1".into()),
        zona: w.zona.clone(),
        semente: def.map_or(0, |d| d.semente),
        raio_blocos: def.map_or(0, |d| d.raio_blocos),
        bioma: def.map_or_else(|| "?".into(), |d| format!("{:?}", d.bioma)),
        tempo_s: w.sim_time_s,
        tick: w.tick,
        imortal: w.imortal,
        estorvos: w.ilha.as_ref().map_or(0, |i| i.total_de_estorvos()),
        jogadores,
        mobs,
    };
    // Serializa com o cadeado SOLTO. Pegar o cadeado pra montar o JSON poria
    // o laco do mundo esperando por leitor de painel.
    let texto: Arc<str> = match serde_json::to_string(&p) {
        Ok(t) => t.into(),
        Err(e) => {
            tracing::warn!("panoptico: could not serialise: {e}");
            return;
        }
    };
    *PANORAMA.write() = Some(texto);
}

impl GameWorld {
    /// O valor de agora das medidas do panoptico (`telemetria::medir`). Chamado
    /// a cada 30 s do laco — vale sempre, com ou sem `PANOPTICO_BIND`.
    pub fn medir_telemetria(&self) {
        use crate::telemetria::medir;
        let online = self.sessions.values().filter(|s| s.logged_in).count();
        let mobs = self
            .ecs
            .query::<&EnemyTag>()
            .iter()
            .filter(|(_, t)| !t.dead)
            .count();
        let chefes_vivos = self
            .vagas_de_chefe
            .iter()
            .filter(|v| v.vivo.is_some())
            .count();
        medir("online", online as f64);
        medir("mobs_vivos", mobs as f64);
        medir("chefes_vivos", chefes_vivos as f64);
        medir("instancias", self.instancias.len() as f64);
        medir("fila_dungeon", self.mesa.fila.len() as f64);
        medir("salas_dungeon", self.mesa.salas.len() as f64);
        medir(
            "ouro_online",
            self.sessions
                .values()
                .filter(|s| s.logged_in)
                .map(|s| s.gold as f64)
                .sum(),
        );
        let (nos, esgotados) = self.nos_de_coleta();
        medir("nos_de_coleta", nos as f64);
        medir("nos_de_coleta_esgotados", esgotados as f64);
        // PLAY TIME per character, keyed by level: the denominator of every
        // "per hour" (gold, items, xp) the market's prices are anchored on.
        // This runs every 30 s, so each call adds 30 seconds.
        let mult = crate::economy::xp_multiplier();
        for s in self.sessions.values().filter(|s| s.logged_in) {
            let nivel = shared::level_of_xp_with_mult(s.xp, mult);
            crate::telemetria::conta_de(&s.name, "online_s", nivel, 30);
        }
    }
}

fn corpo(w: &GameWorld, e: Entity) -> (glam::Vec2, glam::Vec2) {
    let pos = w.ecs.get::<&Position>(e).map(|p| p.0).unwrap_or_default();
    let vel = w.ecs.get::<&Velocity>(e).map(|v| v.0).unwrap_or_default();
    (pos, vel)
}

fn vida(w: &GameWorld, e: Entity) -> (i32, i32) {
    w.ecs
        .get::<&Health>(e)
        .map(|h| (h.current, h.max))
        .unwrap_or((0, 0))
}

/// A ordem importa e e' a que o jogador sente: caido ganha de tudo, e "parado"
/// so' vale quando nada mais esta' acontecendo.
fn estado_do_jogador(w: &GameWorld, s: &crate::world::Session) -> &'static str {
    let agora = w.sim_time_s;
    if s.downed {
        "caido"
    } else if s.respawn_timer.is_some() {
        "morto"
    } else if agora < s.pulo_ate {
        "pulando"
    } else if agora < s.dash_until {
        "dash"
    } else if agora < s.hurt_until {
        "levando"
    } else if s.casting_until > agora {
        "conjurando"
    } else if s.defending {
        "defendendo"
    } else if s.target.is_some() {
        "in combat"
    } else if !s.rota.vazia() {
        "indo"
    } else if s
        .pending_input
        .as_ref()
        .is_some_and(|f| f.move_dir.length_squared() > 0.01)
    {
        "andando"
    } else {
        "parado"
    }
}

fn estado_do_mob(w: &GameWorld, t: &EnemyTag, perseguindo: bool) -> &'static str {
    let agora = w.sim_time_s;
    if t.dead {
        "morto"
    } else if agora < t.spawn_grace_until {
        "nascendo"
    } else if agora < t.stunned_until {
        "atordoado"
    } else if agora < t.hurt_until {
        "levando"
    } else if t.returning_home {
        "voltando"
    } else if perseguindo {
        "cacando"
    } else if t.wander_phase == 0 {
        "vagando"
    } else {
        "parado"
    }
}

/// O painel esta' ligado nesta instancia?
pub fn ativo() -> bool {
    std::env::var("PANOPTICO_BIND").is_ok()
}

/// Sobe o servidor de leitura. So' e' chamado quando `PANOPTICO_BIND` existe.
pub async fn servir() -> anyhow::Result<()> {
    use axum::extract::Query;
    use axum::http::StatusCode;
    use axum::response::IntoResponse;
    use axum::routing::get;

    let Ok(bind) = std::env::var("PANOPTICO_BIND") else {
        return Ok(());
    };
    // Sem token nao sobe. O panoptico ve' conta, ouro e posicao de todo mundo:
    // deixar isso aberto "so' no dev" e' como esses vazamentos comecam.
    let token = std::env::var("MMO_ADMIN_TOKEN").unwrap_or_default();
    if token.len() < 16 {
        anyhow::bail!(
            "PANOPTICO_BIND pedido mas MMO_ADMIN_TOKEN tem menos de 16 chars — recusando subir"
        );
    }

    async fn estado(
        Query(q): Query<std::collections::HashMap<String, String>>,
    ) -> axum::response::Response {
        let esperado = std::env::var("MMO_ADMIN_TOKEN").unwrap_or_default();
        if q.get("token").map(String::as_str) != Some(esperado.as_str()) {
            return (StatusCode::FORBIDDEN, "token").into_response();
        }
        match PANORAMA.read().clone() {
            Some(j) => (
                [(axum::http::header::CONTENT_TYPE, "application/json")],
                j.to_string(),
            )
                .into_response(),
            None => (StatusCode::SERVICE_UNAVAILABLE, "ainda nao ha' retrato").into_response(),
        }
    }

    let app = axum::Router::new().route("/estado", get(estado));
    let addr: std::net::SocketAddr = bind.parse()?;
    tracing::info!("panoptico ouvindo em http://{addr}/estado");
    let escuta = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(escuta, app).await?;
    Ok(())
}
