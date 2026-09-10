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
}

#[derive(Serialize)]
struct Coleta {
    /// Pedras VIVAS no raio, por tier: [_, cinza, verde, azul, roxo].
    pedras: [u32; 5],
    /// Troncos vivos no raio.
    troncos: u32,
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
    let chao = |p: glam::Vec2| {
        w.ilha.as_ref().map_or(0.0, |i| i.altura(p.x, p.y))
    };

    let mut jogadores = Vec::with_capacity(w.sessions.len());
    for (sid, s) in &w.sessions {
        let Some(e) = s.entity else { continue };
        let (pos, vel) = corpo(w, e);
        let (hp, hp_max) = vida(w, e);
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
                densidade: r.densidade,
                intervalo_s: r.intervalo_s,
                coletas_restantes: r.coletas_restantes,
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

    let def = shared::terreno::def_da_zona(&w.zona);
    let p = Panorama {
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
            tracing::warn!("panoptico: nao consegui serializar: {e}");
            return;
        }
    };
    *PANORAMA.write() = Some(texto);
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
        "em combate"
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
