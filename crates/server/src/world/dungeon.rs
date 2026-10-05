//! Dungeons dentro do canal (docs/DUNGEONS_E_RAIDS.md, F1 + F2).
//!
//! A instancia NAO e' outro processo: e' uma FASE do proprio canal. Todo
//! mundo que esta' nela (jogadores, mobs, chefe, bau, saque) leva o componente
//! `Instancia(id)`, e o canal filtra por ele no AOI, na IA dos mobs, nos
//! golpes, nos telegrafados e na coleta do saque. Duas instancias podem usar
//! a mesma arena ao mesmo tempo sem se ver; quem esta' no mundo aberto nao ve
//! nenhuma. A arena sao sitios planos da propria ilha, longe da cidade: o
//! terreno continua nascendo da semente e nada novo trafega.
//!
//! Isso junta so' quem esta' no MESMO canal. A `mesa` (fila e salas) ja' tem a
//! interface de um servico entre canais/realms — a F5 troca a implementacao,
//! nao este arquivo.

use super::*;
use crate::mesa::{self, Evento as EventoDaMesa};
use shared::dungeon::{self as dg, Aviso, Pedido, Tipo};

/// Entidade de uma instancia de dungeon (0 nao existe: mundo aberto nao leva
/// o componente).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Instancia(pub u32);

/// Quanto longe do centro do andar alguem pode ir antes de ser trazido de volta.
const RAIO_DO_ANDAR: f32 = 55.0;
/// Depois de vencer: o bau abre sozinho aqui, e a instancia fecha um pouco depois.
const BAU_ABRE_SOZINHO_S: f32 = 60.0;
/// Tempo pra juntar o que o chefe largou. O que sobrar no chao nao some: vai
/// pra quem sai (sozinho) ou e' repartido na hora de fechar (`dg_recolher`).
const FECHA_DEPOIS_DE_VENCER_S: f32 = dg::FECHA_DEPOIS_DE_VENCER_S;
const FECHA_DEPOIS_DE_FALHAR_S: f32 = 8.0;
/// Distancia maxima pra abrir o bau com toque.
const ALCANCE_DO_BAU: f32 = 8.0;
/// The Warden of a planned room: tougher than its pack, far from a semi-boss.
/// It is the one kill the gate asks for, so it has to be a fight — but every
/// room has one, so it can't be the Gruta's 4x.
const GUARDIAO_VIDA: f32 = 2.5;
const GUARDIAO_DANO: f32 = 1.2;

/// Which mob of a room is special.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Elite {
    Nao,
    /// The Gruta's middle floor.
    SemiChefe,
    /// Opens the gate of a planned room (`shared::planta`).
    Guardiao,
}

pub struct MembroDg {
    pub sid: SessionId,
    pub nome: String,
    pub mortes: u32,
    pub ajudante: bool,
    pub saiu: bool,
    pub abriu_bau: bool,
    /// Contadores da sessão NA ENTRADA; o baú compara com os de agora.
    pub pocoes_na_entrada: u32,
    pub dashes_na_entrada: u32,
    /// A menor fração de vida vista na instância (amostrada no tick).
    pub vida_min: f32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum EstadoDg {
    Andando,
    Concluida { em: f32 },
    Falhou { em: f32 },
}

pub struct InstanciaDg {
    pub id: u32,
    /// Id unico do bau (vai no personagem: abrir de novo nao da' nada).
    pub uid: u64,
    pub conteudo: u16,
    pub estagio: u8,
    pub membros: Vec<MembroDg>,
    pub andar: u8,
    pub vivos: Vec<Entity>,
    pub inicio: f32,
    pub limite: f32,
    pub estado: EstadoDg,
    pub bau: Option<(Entity, EntityId)>,
    pub aviso_em: f32,
    pub wipes: u32,
    /// O wipe atual ja' recomecou o andar: so' conta de novo depois que alguem
    /// levantar.
    pub wipe_tratado: bool,
    /// Os DESAFIOS desta corrida (`shared::desafio`), sorteados pela hora da
    /// entrada — os mesmos que a tarja da porta mostrava. Vazio fora do Porão.
    pub desafios: Vec<shared::desafio::Desafio>,
    /// The floor plan this run walks (`shared::planta`): walls, gates and one
    /// room per step. `None` = the old open floors (Gruta, or a Porão started
    /// off the Arena).
    pub planta: Option<&'static shared::planta::Planta>,
    /// Mobs of rooms already opened that nobody killed. Opening the gate only
    /// needs the Warden; the rest stay where they are, and go when the run
    /// closes.
    pub restos: Vec<Entity>,
}

fn env_f32(nome: &str, padrao: f32) -> f32 {
    std::env::var(nome)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(padrao)
}

/// Multiplicadores SO' DE TESTE (bots): vida e dano dos inimigos e limite.
/// Sem a variavel, 1,0 e o limite do catalogo.
fn mult_vida_teste() -> f32 {
    env_f32("MMO_DUNGEON_TESTE_VIDA", 1.0).max(0.01)
}
fn mult_dano_teste() -> f32 {
    env_f32("MMO_DUNGEON_TESTE_DANO", 1.0).max(0.0)
}
fn limite_teste(c: &dg::Conteudo) -> f32 {
    std::env::var("MMO_DUNGEON_TESTE_LIMITE_S")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(c.limite_s as f32)
}

fn unix_agora() -> i64 {
    (now_ms() / 1000) as i64
}

/// Chave da sessao na mesa.
fn chave(sid: SessionId) -> mesa::Chave {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    sid.hash(&mut h);
    h.finish()
}

pub(super) fn tirar_item(inv: &mut [shared::InventorySlot], item: u16, mut qtd: u32) -> bool {
    let tem: u32 = inv
        .iter()
        .filter(|s| s.qty > 0 && s.item_id == item)
        .map(|s| s.qty)
        .sum();
    if tem < qtd {
        return false;
    }
    for s in inv.iter_mut().filter(|s| s.qty > 0 && s.item_id == item) {
        let t = s.qty.min(qtd);
        s.qty -= t;
        qtd -= t;
        if s.qty == 0 {
            *s = shared::InventorySlot::default();
        }
        if qtd == 0 {
            break;
        }
    }
    true
}

fn tem_item(inv: &[shared::InventorySlot], item: u16) -> bool {
    inv.iter().any(|s| s.qty > 0 && s.item_id == item)
}

impl GameWorld {
    fn dg_avisar(&self, sid: SessionId, aviso: Aviso) {
        if let Some(s) = self.sessions.get(&sid) {
            let _ = s.handle.to_client.send(ServerMessage::Dungeon { aviso });
        }
    }

    pub(super) fn dg_texto(&self, sid: SessionId, ok: bool, texto: impl Into<String>) {
        self.dg_avisar(
            sid,
            Aviso::Texto {
                ok,
                texto: texto.into(),
            },
        );
    }

    fn dg_sid_da_chave(&self, k: mesa::Chave) -> Option<SessionId> {
        self.sessions.keys().copied().find(|sid| chave(*sid) == k)
    }

    fn dg_nome(&self, k: mesa::Chave) -> String {
        let nome = self
            .dg_sid_da_chave(k)
            .and_then(|sid| self.sessions.get(&sid))
            .map_or("?".to_string(), |s| s.name.clone());
        format!("{nome}@{}", crate::canais::realm())
    }

    fn dg_nivel_e_poder(&self, sid: SessionId) -> (u32, i32) {
        self.sessions.get(&sid).map_or((0, 0), |s| {
            (
                shared::level_of_xp_with_mult(s.xp, crate::economy::xp_multiplier()),
                dg::poder_de_stats(&s.stats),
            )
        })
    }

    /// Por que este personagem nao pode entrar agora (nivel, poder, estagio,
    /// selo, ja' dentro, caido).
    fn dg_recusa(&self, sid: SessionId, c: &dg::Conteudo, estagio: u8) -> Option<String> {
        let s = self.sessions.get(&sid)?;
        if !s.logged_in || s.entity.is_none() {
            return Some("Character out of the game.".into());
        }
        if s.instancia != 0 {
            return Some("You are already in a dungeon.".into());
        }
        if s.downed {
            return Some("Get up first.".into());
        }
        // A Magic Island tier's dungeon is ITS OWN: only entered from that
        // tier. In the Arena (entering solo or from the queue) it is the
        // island the player came from (`arena_volta`).
        let de_onde = if shared::arena::e_arena(&self.zona) { s.dungeon.arena_volta.as_str() } else { self.zona.as_str() };
        if shared::magica::e_magica(c.zona) && c.zona != de_onde {
            let ilha = shared::magica::nivel_da_zona(c.zona).map_or("its Magic Island", |n| n.nome);
            return Some(format!("{} is only entered from {ilha}.", c.nome));
        }
        let (nivel, poder) = self.dg_nivel_e_poder(sid);
        let tem_selo = tem_item(&s.inventory, shared::item_id::SELO_TEMPESTADE);
        dg::cadeado(c, estagio, nivel, poder, s.dungeon.liberado(c.id), tem_selo).map(|x| x.texto())
    }

    /// ABRE UM PORÃO PELA PORTA, no cenário.
    ///
    /// Três coisas são conferidas aqui, e as três no SERVIDOR: a zona é a do
    /// conteúdo, o jogador está a `ALCANCE_DA_PORTA` da porta, e a chave está
    /// na bolsa. A posição sai da entidade dele, nunca do pedido — um pedido
    /// traz o que o cliente quiser que ele traga, e aqui ela decide se a porta
    /// abre.
    ///
    /// Não há conta de entradas: o Porão passou a ser ilimitado em entrada E
    /// em recompensa, e o freio é a chave (`shared::porao`). Quem tem chave
    /// entra; quem não tem, fabrica.
    fn dg_abrir_porao(&mut self, sid: SessionId, conteudo: u16) {
        let Some(c) = dg::conteudo(conteudo).filter(|c| c.tipo == Tipo::Porao) else {
            return;
        };
        if c.zona != self.zona {
            self.dg_texto(
                sid,
                false,
                format!("{} has its portal on another island.", c.nome),
            );
            return;
        }
        if let Some(motivo) = self.dg_recusa(sid, c, 1) {
            self.dg_texto(sid, false, motivo);
            return;
        }
        let Some(porta) = self.porta_do_porao(c) else {
            return;
        };
        let Some(s) = self.sessions.get(&sid) else {
            return;
        };
        let Some(e) = s.entity else { return };
        let onde = self.ecs.get::<&Position>(e).map(|p| p.0).ok();
        let Some(onde) = onde else { return };
        if onde.distance(porta) > shared::porao::ALCANCE_DA_PORTA {
            self.dg_texto(sid, false, format!("Get closer to the {} portal.", c.nome));
            return;
        }
        let Some(chave) = shared::porao::chave_de(c) else {
            return;
        };
        let descer = shared::planta::da(conteudo).is_some() && !self.na_arena();
        let Some(s) = self.sessions.get_mut(&sid) else {
            return;
        };
        if !tem_item(&s.inventory, chave) {
            let nome = shared::porao::nome_da_chave(chave).unwrap_or_else(|| "key".into());
            self.dg_texto(sid, false, format!("You need a {nome} to open this."));
            return;
        }
        // DOWN THE STAIRS: the cellar is on the Arena islet, where the floor
        // plan has flat, empty ground to stand on (`shared::planta`). The run
        // starts when the character lands there (`porao_pendente`).
        //
        // The key is taken only once the trip is under way: with the Arena
        // down, the door says so and the key stays in the bag.
        if descer {
            s.dungeon.porao_pendente = conteudo;
            self.save_pending = true;
            let aviso = format!("You go down into the {}.", c.nome);
            if !self.entrar_na_arena_com(sid, &aviso) {
                if let Some(s) = self.sessions.get_mut(&sid) {
                    s.dungeon.porao_pendente = 0;
                }
                self.dg_texto(sid, false, "The cellar can't be reached right now. Try again in a moment.");
                return;
            }
        }
        let Some(s) = self.sessions.get_mut(&sid) else {
            return;
        };
        tirar_item(&mut s.inventory, chave, 1);
        s.inventory_dirty = true;
        crate::telemetria::conta_de(&s.name, "porao_chave_usada", format!("{conteudo}"), 1);
        if s.dungeon.porao_pendente == 0 {
            self.dg_comecar(conteudo, 1, vec![sid]);
        }
    }

    /// Starts the Porão runs of whoever just landed on the Arena through a
    /// door. On the tick after the login, not inside it: the login is still
    /// assembling the session when it notices.
    fn dg_poroes_que_chegaram(&mut self) {
        if self.poroes_a_comecar.is_empty() {
            return;
        }
        for (sid, conteudo) in std::mem::take(&mut self.poroes_a_comecar) {
            let Some(s) = self.sessions.get_mut(&sid) else {
                continue;
            };
            if !s.logged_in || s.entity.is_none() {
                self.poroes_a_comecar.push((sid, conteudo));
                continue;
            }
            s.dungeon.porao_pendente = 0;
            s.porao_de_volta = Some(conteudo);
            self.save_pending = true;
            let antes = self.instancias.len();
            self.dg_comecar(conteudo, 1, vec![sid]);
            if self.instancias.len() == antes {
                // Refused on arrival (it shouldn't: the door checked the same
                // things). The key comes back, and so does the character.
                if let (Some(s), Some(chave)) = (
                    self.sessions.get_mut(&sid),
                    dg::conteudo(conteudo).and_then(shared::porao::chave_de),
                ) {
                    if !super::add_to_inventory(&mut s.inventory, chave, 1, None) {
                        s.dungeon.postar(chave, 1, None, 0, unix_agora());
                    }
                    s.inventory_dirty = true;
                }
                self.dg_voltar_pra_porta(sid, "The cellar refused you; your key is back.");
            }
        }
    }

    /// Back up the stairs: to the island of the Porão, in front of its door.
    fn dg_voltar_pra_porta(&mut self, sid: SessionId, aviso: &str) {
        let Some(conteudo) = self.sessions.get_mut(&sid).and_then(|s| s.porao_de_volta.take()) else {
            self.sair_da_arena(sid, aviso);
            return;
        };
        let Some((c, def)) = dg::conteudo(conteudo)
            .and_then(|c| Some((c, shared::terreno::def_da_zona(c.zona)?)))
        else {
            self.sair_da_arena(sid, aviso);
            return;
        };
        let ger = shared::terreno::Gerador::da_ilha(def);
        let chegada = shared::porao::porta_de(c, &ger)
            .map(|p| p + Vec2::new(0.0, 2.0))
            .or_else(|| ger.cidade().map(|c| c.centro()))
            .unwrap_or(Vec2::ZERO);
        self.mandar_para_zona(sid, def.zona, chegada, Some(aviso), Some(def.nome));
    }

    /// Onde fica a porta deste Porão nesta zona.
    ///
    /// A cidade é a âncora (ver `shared::porao::porta_de`), então a porta anda
    /// junto com ela se a ilha for regerada — coordenada escrita à mão viraria
    /// porta no mar no dia em que a semente mudasse.
    pub(crate) fn porta_do_porao(&self, c: &dg::Conteudo) -> Option<Vec2> {
        // O MESMO `Gerador` que o cliente usa, pela `def` da zona — e não a
        // `Ilha` carregada. Os dois lados precisam chegar na MESMA porta, e a
        // busca por chão firme (`porao::porta_de`) só é determinística se a
        // fonte do relevo for a mesma dos dois lados.
        let def = shared::terreno::def_da_zona(&self.zona)?;
        let ger = shared::terreno::Gerador::da_ilha(def);
        shared::porao::porta_de(c, &ger)
    }

    // ─────────────────────────────── arena ───────────────────────────────

    /// Os sitios dos andares: planos, longe da cidade, do porto e dos chefes
    /// de campo. Calculado uma vez por processo (deterministico da ilha).
    fn dg_arena(&mut self) -> Vec<Vec2> {
        if !self.arena_dg.is_empty() {
            return self.arena_dg.clone();
        }
        use shared::terreno::BLOCO;
        let porto = self.porto();
        let mut sitios: Vec<Vec2> = Vec::new();
        if let (Some(ilha), Some(def)) =
            (self.ilha.as_ref(), shared::terreno::def_da_zona(&self.zona))
        {
            let passo = 48i32;
            let raio_sitio = (9.0 / BLOCO) as i32;
            let seguras: Vec<Vec2> = self.safe_zones.iter().map(|(o, s)| *o + *s * 0.5).collect();
            let chefes: Vec<Vec2> = self.vagas_de_chefe.iter().map(|v| v.pos).collect();
            let mut cand = Vec::new();
            let mut b = -def.raio_blocos;
            while b < def.raio_blocos {
                let mut a = -def.raio_blocos;
                while a < def.raio_blocos {
                    if ilha.sitio_plano(a + def.raio_blocos, b + def.raio_blocos, raio_sitio) {
                        let p = Vec2::new(a as f32 * BLOCO, b as f32 * BLOCO);
                        if !ilha.agua(p.x, p.y)
                            && ilha.sem_estorvo(p, ENTITY_RADIUS)
                            && seguras.iter().all(|s| s.distance(p) >= 160.0)
                            && chefes.iter().all(|s| s.distance(p) >= 90.0)
                        {
                            cand.push(p);
                        }
                    }
                    a += passo;
                }
                b += passo;
            }
            // O mais longe do porto primeiro: ninguem tropeca na arena.
            //
            // Menos NA ARENA, onde é o contrário: lá não há porto nem gente
            // passando, e o que interessa é o andar caber inteiro em chão
            // plano — ou seja, o mais PERTO do meio.
            let arena = self.na_arena();
            cand.sort_by(|x, y| {
                let (a, b) = (x.distance_squared(porto), y.distance_squared(porto));
                if arena {
                    a.total_cmp(&b)
                } else {
                    b.total_cmp(&a)
                }
                .then(x.x.total_cmp(&y.x))
            });
            // AS QUATRO INSTÂNCIAS PODEM DIVIDIR O MESMO CHÃO — na Arena.
            //
            // Os 70 u de distância entre sítios vêm do mundo aberto, onde duas
            // dungeons no mesmo lugar seriam duas hordas empilhadas. Na Arena
            // elas não se veem: cada uma vive dentro do seu `Instancia(id)`, e
            // o jogador de uma nunca enxerga a outra.
            //
            // Exigir o espaçamento ali era o que obrigava a ilhota a ser
            // grande — o dono, duas vezes: "desnecessariamente grande". Sem
            // ele, a ilhota só precisa caber UM andar.
            let entre = if self.na_arena() { 0.0 } else { 70.0 };
            for p in cand {
                if sitios.iter().all(|s| s.distance(p) >= entre) {
                    sitios.push(p);
                    if sitios.len() == 4 {
                        break;
                    }
                }
            }
        }
        if sitios.is_empty() {
            // Mapa sem ilha (teste/legado): perto do porto, espalhado.
            sitios = (0..4)
                .map(|i| porto + Vec2::new(80.0 + 70.0 * i as f32, 80.0))
                .collect();
        }
        while sitios.len() < 4 {
            let ultimo = *sitios.last().expect("nao vazio");
            sitios.push(ultimo);
        }
        tracing::info!("[dungeon] arena da ilha '{}': {:?}", self.zona, sitios);
        self.arena_dg = sitios.clone();
        sitios
    }

    fn dg_teleportar(&mut self, sid: SessionId, destino: Vec2) {
        let destino = self.chao_livre(destino);
        let Some(s) = self.sessions.get_mut(&sid) else {
            return;
        };
        s.rota = Default::default();
        s.target = None;
        let Some(e) = s.entity else { return };
        if let Ok(mut p) = self.ecs.get::<&mut Position>(e) {
            p.0 = destino;
        }
        if let Ok(mut v) = self.ecs.get::<&mut Velocity>(e) {
            v.0 = Vec2::ZERO;
        }
    }

    /// Esta zona é a ARENA?
    pub(crate) fn na_arena(&self) -> bool {
        shared::arena::e_arena(&self.zona)
    }

    /// Leva o jogador pra Arena, lembrando de onde ele veio.
    ///
    /// A Arena é destino sem saída própria: ninguém navega de volta dela, e
    /// por isso a zona de origem é guardada ANTES da viagem. Sem isso, sair
    /// seria um chute — o mesmo cuidado que a Ilha Mágica já tem.
    fn entrar_na_arena(&mut self, sid: SessionId) {
        // A INSTRUÇÃO DE VOLTA VAI JUNTO COM A IDA. Quem chega aqui não
        // tem barco nem portal, e descobrir sozinho onde fica a saída não
        // é parte do jogo.
        self.entrar_na_arena_com(
            sid,
            "Você entra na Arena. Para voltar, abra Dungeons e toque em Sair.",
        );
    }

    /// The trip to the Arena, with the line shown on the way. `false` = the
    /// Arena is down and nobody went anywhere.
    fn entrar_na_arena_com(&mut self, sid: SessionId, aviso: &str) -> bool {
        if self.na_arena() {
            return false;
        }
        let volta = self.zona.clone();
        // Where they stand right now: the door of the Porão they opened, or
        // wherever they asked for the Arena. That is where they come back.
        let aqui = self
            .sessions
            .get(&sid)
            .and_then(|s| s.entity)
            .and_then(|e| self.ecs.get::<&Position>(e).ok().map(|p| p.0));
        if let Some(s) = self.sessions.get_mut(&sid) {
            s.dungeon.arena_volta = volta;
            s.dungeon.arena_volta_pos = aqui.map(|p| [p.x, p.y]);
        }
        self.save_pending = true;
        let chegada = self.chegada_da_arena();
        self.mandar_para_zona(
            sid,
            shared::arena::ZONA,
            chegada,
            Some(aviso),
            Some(shared::arena::DEF.nome),
        )
    }

    /// Volta pra zona de onde entrou.
    fn sair_da_arena(&mut self, sid: SessionId, aviso: &str) {
        if !self.na_arena() {
            return;
        }
        let Some(s) = self.sessions.get(&sid) else {
            return;
        };
        // Vazio (entrou antes desta coluna existir, ou o processo caiu) volta
        // pro Bosque: ficar preso numa arena vazia é pior que chegar na ilha
        // errada.
        let volta = if s.dungeon.arena_volta.is_empty() {
            shared::terreno::ARQUIPELAGO[0].zona.to_string()
        } else {
            s.dungeon.arena_volta.clone()
        };
        let Some(def) = shared::terreno::def_da_zona(&volta) else {
            return;
        };
        // Back to the spot they left from (the Porão door), and only without
        // one — an old save — to the city.
        let chegada = s
            .dungeon
            .arena_volta_pos
            .map(|[x, z]| Vec2::new(x, z))
            .or_else(|| shared::terreno::Gerador::da_ilha(def).cidade().map(|c| c.centro()))
            .unwrap_or(Vec2::ZERO);
        if self.mandar_para_zona(sid, def.zona, chegada, Some(aviso), Some(def.nome)) {
            if let Some(s) = self.sessions.get_mut(&sid) {
                s.dungeon.arena_volta_pos = None;
            }
        }
    }

    /// This character is already on the way to another zone this tick
    /// (`mandar_para_zona` ran). A second trip would overwrite the first.
    fn ja_de_saida(&self, sid: SessionId) -> bool {
        self.sessions
            .get(&sid)
            .is_some_and(|s| self.zona_de_saida.contains_key(&s.name))
    }

    /// Onde se chega na Arena: o chão seco mais perto do meio.
    fn chegada_da_arena(&self) -> Vec2 {
        shared::arena::CHEGADA
    }

    // ─────────────────────────────── pedidos ───────────────────────────────

    pub(super) fn handle_dungeon(&mut self, sid: SessionId, pedido: Pedido) {
        if !self.sessions.get(&sid).is_some_and(|s| s.logged_in) {
            return;
        }
        let agora = self.sim_time_s as f64;
        let k = chave(sid);
        // A FILA E AS SALAS SÓ EXISTEM NA ARENA.
        //
        // A `mesa` vive num processo só — é isso que a torna a mesma pra todo
        // mundo, sem sincronizar nada entre zonas. O dono criou uma sala com
        // um personagem e não a viu com o outro justamente porque cada zona
        // tinha a própria mesa.
        //
        // Quem pede daqui de fora recebe um convite, e não um silêncio: botão
        // que não faz nada é pior que botão que explica.
        let precisa_da_arena = matches!(
            pedido,
            Pedido::EntrarSolo { .. }
                | Pedido::GrutaSolo { .. }
                | Pedido::FilaEntrar { .. }
                | Pedido::SalaCriar { .. }
                | Pedido::SalasBuscar { .. }
                | Pedido::SalaEntrar { .. }
        );
        if precisa_da_arena && !self.na_arena() {
            self.dg_avisar(sid, Aviso::PrecisaDaArena);
            return;
        }
        // O PORÃO NÃO PASSA MAIS PELA ARENA.
        //
        // Ele virou dungeon física em 29/09/2026: a entrada é uma porta na
        // ilha dele, e a instância nasce NESTE processo, sem viagem nenhuma.
        // `AbrirPorao` fica de fora de `precisa_da_arena` por isso — exigir a
        // Arena aqui seria mandar o jogador atravessar o mundo pra abrir uma
        // porta que está na frente dele.
        if let Pedido::AbrirPorao { conteudo } = pedido {
            self.dg_abrir_porao(sid, conteudo);
            return;
        }
        match pedido {
            Pedido::IrParaArena => {
                self.entrar_na_arena(sid);
                return;
            }
            Pedido::SairDaArena => {
                self.sair_da_arena(sid, "You leave the Arena.");
                return;
            }
            Pedido::Estado => {}
            // Cliente velho ainda pedindo o Porão pelo painel. Em vez de
            // silêncio, a explicação: a porta agora fica na ilha.
            Pedido::EntrarSolo { conteudo } => {
                let onde = dg::conteudo(conteudo)
                    .filter(|c| c.tipo == Tipo::Porao)
                    .map(|c| format!("{} is now entered through its portal, on the island.", c.nome))
                    .unwrap_or_else(|| "This cellar is now entered through its portal.".into());
                self.dg_texto(sid, false, onde);
                return;
            }
            Pedido::AbrirPorao { .. } => return,
            Pedido::GrutaSolo { conteudo, estagio } => {
                let Some(c) = dg::conteudo(conteudo).filter(|c| c.tipo == Tipo::Gruta) else {
                    return;
                };
                if let Some(motivo) = self.dg_recusa(sid, c, estagio) {
                    self.dg_texto(sid, false, motivo);
                    return;
                }
                let ev = self.mesa.remover(k, agora);
                self.dg_eventos(ev);
                self.dg_comecar(conteudo, estagio, vec![sid]);
                return;
            }
            Pedido::FilaEntrar { conteudo, estagio }
            | Pedido::SalaCriar {
                conteudo, estagio, ..
            } => {
                let Some(c) = dg::conteudo(conteudo).filter(|c| c.tipo == Tipo::Gruta) else {
                    return;
                };
                if let Some(motivo) = self.dg_recusa(sid, c, estagio) {
                    self.dg_texto(sid, false, motivo);
                    return;
                }
                let r = match pedido {
                    Pedido::SalaCriar {
                        completar_pela_fila,
                        ..
                    } => self
                        .mesa
                        .criar_sala(k, conteudo, estagio, completar_pela_fila, agora)
                        .map(|_| ()),
                    _ => self.mesa.entrar_fila(k, conteudo, estagio, agora),
                };
                if let Err(e) = r {
                    self.dg_texto(sid, false, e);
                }
            }
            Pedido::FilaSair => self.mesa.sair_fila(k),
            Pedido::SalasBuscar { conteudo, estagio } => {
                let lista = self
                    .mesa
                    .salas_de(conteudo, estagio)
                    .into_iter()
                    .map(|s| self.dg_sala_net(s))
                    .collect();
                self.dg_avisar(sid, Aviso::Salas { lista });
                return;
            }
            Pedido::SalaEntrar { sala } => {
                let Some((c, e)) = self.mesa.sala(sala).map(|s| (s.conteudo, s.estagio)) else {
                    self.dg_texto(sid, false, "The room no longer exists.");
                    return;
                };
                let Some(def) = dg::conteudo(c) else { return };
                if let Some(motivo) = self.dg_recusa(sid, def, e) {
                    self.dg_texto(sid, false, motivo);
                    return;
                }
                if let Err(e) = self.mesa.entrar_sala(k, sala, def.grupo_max, agora) {
                    self.dg_texto(sid, false, e);
                    return;
                }
                self.dg_estado_da_sala(sala);
                return;
            }
            Pedido::SalaSair => {
                let sala = match self.mesa.onde(k) {
                    mesa::Onde::Sala(id) => Some(id),
                    _ => None,
                };
                self.mesa.sair_sala(k, agora);
                if let Some(id) = sala {
                    self.dg_estado_da_sala(id);
                }
            }
            Pedido::SalaIniciar => match self.mesa.iniciar_sala(k, agora) {
                Ok(ev) => self.dg_eventos(vec![ev]),
                Err(e) => self.dg_texto(sid, false, e),
            },
            Pedido::Pronto { partida, aceito } => {
                let ev = self.mesa.responder(k, partida, aceito, agora);
                self.dg_eventos(ev);
                return;
            }
            Pedido::ComprarEntrada => self.dg_comprar_entrada(sid),
            Pedido::Sair => {
                // Um toque encerra a instância e retorna à ilha de origem.
                // dg_sair preserva a entrega do saque antes da troca de zona.
                self.dg_sair(sid);
                // `dg_sair` may already have sent them out of the Porão door.
                // Sending them again here put them in the city instead: the
                // second trip overwrote the first.
                if self.na_arena() && !self.ja_de_saida(sid) {
                    self.sair_da_arena(sid, "You leave the Arena.");
                }
                return;
            }
            Pedido::Reviver => {
                self.dg_reviver(sid);
                return;
            }
            Pedido::AbrirBau { eid } => {
                self.dg_abrir_bau(sid, EntityId(eid as u32));
                return;
            }
            Pedido::Correio => {
                self.dg_enviar_correio(sid);
                return;
            }
            Pedido::CorreioRetirar { id } => {
                self.dg_retirar_carta(sid, id);
                return;
            }
        }
        self.dg_enviar_estado(sid);
    }

    fn dg_sala_net(&self, s: &mesa::Sala) -> dg::SalaNet {
        let max = dg::conteudo(s.conteudo).map_or(5, |c| c.grupo_max);
        dg::SalaNet {
            id: s.id,
            conteudo: s.conteudo,
            estagio: s.estagio,
            membros: s
                .membros
                .iter()
                .map(|m| dg::MembroNet {
                    nome: self.dg_nome(*m),
                    lider: *m == s.lider,
                    aceitou: false,
                })
                .collect(),
            vagas: max.saturating_sub(s.membros.len() as u8),
            completar_pela_fila: s.completar_pela_fila,
        }
    }

    fn dg_estado_da_sala(&mut self, sala: u32) {
        let membros: Vec<SessionId> = self
            .mesa
            .sala(sala)
            .map(|s| {
                s.membros
                    .iter()
                    .filter_map(|m| self.dg_sid_da_chave(*m))
                    .collect()
            })
            .unwrap_or_default();
        for sid in membros {
            self.dg_enviar_estado(sid);
        }
    }

    /// A janela inteira: conteudos com cadeado por estagio, entradas do dia,
    /// fila e sala.
    pub(super) fn dg_enviar_estado(&mut self, sid: SessionId) {
        let hoje = dg::dia(unix_agora());
        let (nivel, poder) = self.dg_nivel_e_poder(sid);
        let k = chave(sid);
        let agora = self.sim_time_s as f64;
        let zona = self.zona.clone();
        let fila = self.mesa.na_fila(k).map(|f| dg::FilaNet {
            conteudo: f.conteudo,
            estagio: f.estagio,
            esperando_s: (agora - f.desde).max(0.0) as u32,
            na_fila: self.mesa.contar_fila(f.conteudo, f.estagio) as u8,
        });
        let sala = match self.mesa.onde(k) {
            mesa::Onde::Sala(id) => self.mesa.sala(id).map(|s| self.dg_sala_net(s)),
            _ => None,
        };
        // Antes do empréstimo da sessão: o catálogo acompanha o hot-reload.
        let drops: std::collections::HashMap<u16, Vec<u16>> = dg::CONTEUDOS.iter().map(|c| {
            let mut itens = crate::economy::com_config(|cfg| cfg.itens_possiveis_do_mob(c.chefe));
            itens.extend(super::chefes::itens_do_chefe(c.chefe).into_iter().filter(|(_, chance)| *chance > 0.0).map(|(id, _)| id));
            itens.sort_unstable();
            itens.dedup();
            (c.id, itens)
        }).collect();
        let Some(s) = self.sessions.get_mut(&sid) else {
            return;
        };
        s.dungeon.gruta.atualizar(Tipo::Gruta, hoje);
        s.dungeon.porao.atualizar(Tipo::Porao, hoje);
        let tem_selo = tem_item(&s.inventory, shared::item_id::SELO_TEMPESTADE);
        let conteudos = dg::CONTEUDOS
            .iter()
            .map(|c| {
                let liberado = s.dungeon.liberado(c.id);
                dg::ConteudoEstado {
                    id: c.id,
                    liberado,
                    cadeados: (1..=dg::estagios(c))
                        .map(|e| dg::cadeado(c, e, nivel, poder, liberado, tem_selo))
                        .collect(),
                    primeiras_concluidas: (1..=dg::estagios(c)).map(|e| s.dungeon.vitorias(c.id,e) > 0).collect(),
                    semanais_recebidas: (1..=dg::estagios(c)).map(|e|
                        s.conta_dungeon.semana == dg::semana(unix_agora()) && s.conta_dungeon.primeiras.contains(&(c.id,e))).collect(),
                    drops_chefe: drops.get(&c.id).cloned().unwrap_or_default(),
                    vitorias: (1..=dg::estagios(c))
                        .map(|e| s.dungeon.vitorias(c.id, e))
                        .sum(),
                }
            })
            .collect();
        let entradas = dg::EntradasNet {
            gruta: s.dungeon.gruta.saldo,
            gruta_preco: s.dungeon.gruta.preco_da_compra(Tipo::Gruta, nivel),
            porao: s.dungeon.porao.saldo,
        };
        let _ = s.handle.to_client.send(ServerMessage::Dungeon {
            aviso: Aviso::Estado {
                conteudos,
                entradas,
                fila,
                sala,
            },
        });
        // ONDE ELE ESTÁ vai JUNTO com o estado, e não só quando ele erra.
        //
        // A janela precisa saber disso pra oferecer a ida ANTES do clique —
        // um botão "Enter" que só depois avisa "aqui não" é o mesmo botão
        // morto que este trabalho veio consertar.
        let dentro = shared::arena::e_arena(&zona);
        let _ = s.handle.to_client.send(ServerMessage::Dungeon {
            aviso: Aviso::NaArena { dentro },
        });
    }

    fn dg_comprar_entrada(&mut self, sid: SessionId) {
        let (nivel, _) = self.dg_nivel_e_poder(sid);
        let hoje = dg::dia(unix_agora());
        let Some(s) = self.sessions.get_mut(&sid) else {
            return;
        };
        s.dungeon.gruta.atualizar(Tipo::Gruta, hoje);
        let Some(preco) = s.dungeon.gruta.preco_da_compra(Tipo::Gruta, nivel) else {
            let _ = s.handle.to_client.send(ServerMessage::Dungeon {
                aviso: Aviso::Texto {
                    ok: false,
                    texto: "No more entries for sale today.".into(),
                },
            });
            return;
        };
        if s.gold < preco {
            let _ = s.handle.to_client.send(ServerMessage::Dungeon {
                aviso: Aviso::Texto {
                    ok: false,
                    texto: format!("{} gold short.", preco - s.gold),
                },
            });
            return;
        }
        s.gold -= preco;
        s.dungeon.gruta.comprar();
        crate::telemetria::conta_de(&s.name, "dungeon_entrada_comprada", "gruta", 1);
        crate::telemetria::conta_de(&s.name, "ouro_ralo", "dungeon_entrada", preco as i64);
        self.save_pending = true;
        self.dg_texto(sid, true, format!("Entry bought for {preco} gold."));
    }

    // ─────────────────────────────── mesa ───────────────────────────────

    fn dg_eventos(&mut self, eventos: Vec<EventoDaMesa>) {
        for ev in eventos {
            match ev {
                EventoDaMesa::Pronto(p) => {
                    let membros: Vec<dg::MembroNet> = p
                        .membros
                        .iter()
                        .map(|m| dg::MembroNet {
                            nome: self.dg_nome(*m),
                            lider: false,
                            aceitou: p.aceitos.contains(m),
                        })
                        .collect();
                    for m in &p.membros {
                        if let Some(sid) = self.dg_sid_da_chave(*m) {
                            self.dg_avisar(
                                sid,
                                Aviso::Pronto {
                                    partida: p.id,
                                    conteudo: p.conteudo,
                                    estagio: p.estagio,
                                    membros: membros.clone(),
                                    expira_s: mesa::PRONTO_S as u8,
                                },
                            );
                        }
                    }
                }
                EventoDaMesa::Comecar {
                    conteudo,
                    estagio,
                    membros,
                } => {
                    let sids: Vec<SessionId> = membros
                        .iter()
                        .filter_map(|m| self.dg_sid_da_chave(*m))
                        .collect();
                    self.dg_comecar(conteudo, estagio, sids);
                }
                EventoDaMesa::Cancelado {
                    partida,
                    membros,
                    recusou,
                } => {
                    for m in &membros {
                        if let Some(sid) = self.dg_sid_da_chave(*m) {
                            let texto = if recusou.contains(m) {
                                "You left the run.".to_string()
                            } else {
                                "Alguém não aceitou: você voltou pra fila.".to_string()
                            };
                            self.dg_avisar(sid, Aviso::ProntoFechou { partida, texto });
                            self.dg_enviar_estado(sid);
                        }
                    }
                }
                EventoDaMesa::SalaFechou { membros, .. } => {
                    for m in membros {
                        if let Some(sid) = self.dg_sid_da_chave(m) {
                            self.dg_texto(sid, false, "The room sat idle and closed.");
                            self.dg_enviar_estado(sid);
                        }
                    }
                }
            }
        }
    }

    // ─────────────────────────────── instancia ───────────────────────────────

    /// Cria a instancia com quem ainda pode entrar. Gasta a entrada (ou vai
    /// como Ajudante), o Selo do topo, e leva todo mundo pro primeiro andar.
    fn dg_comecar(&mut self, conteudo: u16, estagio: u8, sids: Vec<SessionId>) {
        let Some(c) = dg::conteudo(conteudo) else {
            return;
        };
        let validos: Vec<SessionId> = sids
            .into_iter()
            .filter(|sid| match self.dg_recusa(*sid, c, estagio) {
                Some(motivo) => {
                    self.dg_texto(*sid, false, motivo);
                    false
                }
                None => true,
            })
            .collect();
        if validos.is_empty() {
            return;
        }
        let arena = self.dg_arena();
        // The floor plan needs the Arena's flat, empty top; anywhere else the
        // run falls back to the open floors.
        let planta = shared::planta::da(conteudo).filter(|_| self.na_arena());
        let inicio = planta.map_or(arena[0], |p| p.ponto_de_volta(0));
        self.prox_instancia += 1;
        let id = self.prox_instancia;
        let uid = (now_ms() << 12) ^ id as u64;
        let hoje = dg::dia(unix_agora());
        let mut membros = Vec::new();
        for (i, sid) in validos.iter().enumerate() {
            let Some(s) = self.sessions.get_mut(sid) else {
                continue;
            };
            // O PORÃO NÃO TEM MAIS COTA — nem de entrada, nem de recompensa.
            //
            // O dono, em 29/09/2026: "that way porao will be infity enters and
            // inifty rewards but needing key to get in". A chave já foi gasta
            // em `dg_abrir_porao`; cobrar também uma entrada do dia seria cobrar
            // duas vezes, e a partir da quarta o baú viria pela metade sem que
            // nada na tela explicasse por quê.
            //
            // A Gruta segue com a conta de sempre: lá a entrada é o freio.
            let ajudante = if c.tipo == Tipo::Porao {
                false
            } else {
                let entradas = s.dungeon.entradas(c.tipo);
                entradas.atualizar(c.tipo, hoje);
                !entradas.consumir()
            };
            crate::telemetria::conta_de(&s.name, 
                "dungeon_entrada",
                format!(
                    "{conteudo}:{estagio}:{}",
                    if ajudante { "ajudante" } else { "normal" }
                ),
                1,
            );
            if dg::exige_selo(c, estagio) {
                crate::telemetria::conta_de(&s.name, "selo_usado", format!("{conteudo}:{estagio}"), 1);
                tirar_item(&mut s.inventory, shared::item_id::SELO_TEMPESTADE, 1);
                s.inventory_dirty = true;
            }
            let Some(e) = s.entity else { continue };
            let pos = self
                .ecs
                .get::<&Position>(e)
                .map(|p| p.0)
                .unwrap_or(Vec2::ZERO);
            s.retorno_da_dungeon = Some(pos);
            s.instancia = id;
            membros.push(MembroDg {
                sid: *sid,
                nome: s.name.clone(),
                mortes: 0,
                ajudante,
                saiu: false,
                abriu_bau: false,
                pocoes_na_entrada: s.pocoes_bebidas,
                dashes_na_entrada: s.dashes_feitos,
                vida_min: 1.0,
            });
            let _ = self.ecs.insert_one(e, Instancia(id));
            let volta = Vec2::new((i as f32 * 1.3).cos(), (i as f32 * 1.3).sin()) * 3.0;
            self.dg_teleportar(*sid, inicio + volta);
        }
        let ev = validos
            .iter()
            .flat_map(|sid| self.mesa.remover(chave(*sid), self.sim_time_s as f64))
            .collect();
        self.dg_eventos(ev);
        let agora = self.sim_time_s;
        tracing::info!(
            "[dungeon] instancia {id}: {} estagio {estagio} com {}",
            c.nome,
            membros
                .iter()
                .map(|m| m.nome.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        );
        self.instancias.push(InstanciaDg {
            id,
            uid,
            conteudo,
            estagio,
            membros,
            andar: 0,
            vivos: Vec::new(),
            inicio: agora,
            limite: agora + limite_teste(c),
            estado: EstadoDg::Andando,
            bau: None,
            aviso_em: 0.0,
            wipes: 0,
            wipe_tratado: false,
            desafios: if shared::desafio::tem_desafio(c) {
                shared::desafio::da_hora(c, unix_agora()).to_vec()
            } else {
                Vec::new()
            },
            planta,
            restos: Vec::new(),
        });
        let idx = self.instancias.len() - 1;
        self.dg_povoar_andar(idx);
        self.save_pending = true;
    }

    fn dg_despawn(&mut self, entidades: Vec<Entity>) {
        for e in entidades {
            let eid = self.ecs.get::<&NetId>(e).map(|n| n.0).ok();
            if self.ecs.despawn(e).is_ok() {
                if let Some(eid) = eid {
                    self.removed_this_tick.push(eid);
                }
            }
        }
    }

    /// Os inimigos do andar atual, frescos (inicio do andar e wipe).
    fn dg_povoar_andar(&mut self, idx: usize) {
        let (id, uid, conteudo, estagio, andar, n) = {
            let i = &self.instancias[idx];
            (
                i.id,
                i.uid,
                i.conteudo,
                i.estagio,
                i.andar,
                i.membros.iter().filter(|m| !m.saiu).count(),
            )
        };
        let Some(c) = dg::conteudo(conteudo) else {
            return;
        };
        let velhos = std::mem::take(&mut self.instancias[idx].vivos);
        self.dg_despawn(velhos);
        let arena = self.dg_arena();
        // With a floor plan, the step's mobs live in the step's ROOM: they
        // spawn inside it and are leashed to it.
        let sala = self.instancias[idx]
            .planta
            .and_then(|p| Some((p, p.sala_da_etapa(andar)?)));
        let (centro, raio_da_sala) = match sala {
            Some((p, i)) => (p.centro(i), p.salas[i].raio),
            None => (arena[(andar as usize).min(arena.len() - 1)], RAIO_DO_ANDAR),
        };
        let com_planta = sala.is_some();
        let chao = sala.map(|(p, _)| (p, andar));
        let nivel = dg::nivel_do_estagio(c, estagio);
        let vida = dg::vida_por_grupo(c, n) * mult_vida_teste();
        let dano = mult_dano_teste();
        let mut vivos = Vec::new();
        if andar >= c.andares {
            let (vida, dano) = {
                let (v, d) = dg::escala_do_chefe(c, n);
                (v * mult_vida_teste(), d * dano)
            };
            let quer = if com_planta { centro } else { centro + Vec2::new(0.0, 14.0) };
            let corpo = ENTITY_RADIUS * shared::bosses::chefe(c.chefe).map_or(1.0, |b| b.escala);
            let pos = self.dg_chao_do_mob(quer, centro, corpo, chao);
            if let Some(e) = self.nascer_chefe_nivel(c.chefe, pos, nivel) {
                if let Ok(mut h) = self.ecs.get::<&mut Health>(e) {
                    h.max = ((h.max as f32) * vida).round().max(1.0) as i32;
                    h.current = h.max;
                }
                if let Ok(mut t) = self.ecs.get::<&mut EnemyTag>(e) {
                    t.stats.hp_max = ((t.stats.hp_max as f32) * vida).round().max(1.0) as i32;
                    t.stats.attack_damage =
                        ((t.stats.attack_damage as f32) * dano).round().max(0.0) as i32;
                    t.detect_range = t.detect_range.max(30.0);
                }
                let _ = self.ecs.insert_one(e, Instancia(id));
                vivos.push(e);
            }
        } else {
            // A dungeon herda o bestiario da ILHA em que ela fica: a masmorra
            // da Geleira tem bicho de gelo, e nao caranguejo.
            let bioma = shared::terreno::def_da_zona(c.zona)
                .map(|def| def.bioma)
                .unwrap_or_else(|| self.bioma_da_zona());
            let n_mobs = dg::inimigos_do_andar(c, andar);
            for i in 0..n_mobs {
                let (ang, longe) = if com_planta {
                    // A HORDE spread over the room: the Warden in the middle,
                    // the pack on a sunflower spiral out to 3/4 of the radius
                    // (clear of the wall and its rock bumps).
                    let t = (i as f32 / n_mobs.max(1) as f32).sqrt();
                    (
                        i as f32 * 2.399_963,
                        if i == 0 { 0.0 } else { raio_da_sala * (0.25 + 0.5 * t) },
                    )
                } else {
                    (
                        i as f32 * std::f32::consts::TAU / n_mobs as f32,
                        10.0 + (i % 3) as f32 * 4.0,
                    )
                };
                let pos = centro + Vec2::new(ang.cos(), ang.sin()) * longe;
                let kind = crate::economy::kind_para_nivel(
                    bioma,
                    nivel,
                    uid ^ ((andar as u64) << 8) ^ i as u64,
                );
                // The first mob of a planned room is its WARDEN: the one whose
                // death opens the gate. The Gruta's middle floor keeps its
                // semi-boss.
                let elite = if dg::tem_semi_chefe(c, andar) && i == 0 {
                    Elite::SemiChefe
                } else if com_planta && i == 0 {
                    Elite::Guardiao
                } else {
                    Elite::Nao
                };
                let coleira = if com_planta { raio_da_sala + 4.0 } else { RAIO_DO_ANDAR * 0.7 };
                let escala = match elite {
                    Elite::SemiChefe => 1.4,
                    Elite::Guardiao => 1.25,
                    Elite::Nao => 1.0,
                };
                let corpo = ENTITY_RADIUS * crate::economy::enemy_size_scale(kind) * escala;
                let pos = self.dg_chao_do_mob(pos, centro, corpo, chao);
                vivos.push(self.dg_nascer_mob(id, kind, pos, centro, coleira, nivel, vida, dano, elite));
            }
        }
        self.instancias[idx].vivos = vivos;
    }

    /// Where a dungeon mob (or boss) of body radius `corpo` may stand, as
    /// close to `quer` as the floor allows.
    ///
    /// Before 01/10/2026 every spawn went through `chao_livre`, which only
    /// moves a point off water and obstacles. In a Porão that ignored the
    /// floor plan — the spiral reaches 3/4 of the room's IDEAL radius, and the
    /// noisy edge and the cave's rock bumps come in further than that — so a
    /// mob could start inside the wall. In a Cavern the site is flat only 9 u
    /// around its centre (`dg_arena`) while the ring reaches 18 u, so mobs
    /// landed on ledges and cliff tops the party could not walk to.
    pub(super) fn dg_chao_do_mob(
        &self,
        quer: Vec2,
        centro: Vec2,
        corpo: f32,
        planta: Option<(&'static shared::planta::Planta, u8)>,
    ) -> Vec2 {
        if let Some((p, andar)) = planta {
            return if p.livre(quer, corpo, andar) { quer } else { p.mais_perto(quer, corpo, andar) };
        }
        let Some(ilha) = self.ilha.as_ref() else {
            return quer;
        };
        // Pulled in toward the centre — the flat site — until it is ground the
        // fight can walk to.
        for k in 0..=10 {
            let q = quer.lerp(centro, k as f32 / 10.0);
            if a_pe(ilha, centro, q, corpo) {
                return q;
            }
        }
        centro
    }

    #[allow(clippy::too_many_arguments)]
    fn dg_nascer_mob(
        &mut self,
        inst: u32,
        kind: u16,
        pos: Vec2,
        ancora: Vec2,
        coleira: f32,
        nivel: u32,
        vida: f32,
        dano: f32,
        elite: Elite,
    ) -> Entity {
        let semi = elite == Elite::SemiChefe;
        let (mut tag, _) = self.build_enemy_tag(kind, ancora, coleira, pos);
        tag.nivel_da_faixa = nivel;
        tag.level = nivel;
        tag.detect_range = tag.detect_range.max(30.0);
        let (hp, d) = vida_e_dano_do_mob(tag.stats.hp_max, tag.stats.attack_damage, nivel);
        let (fv, fd) = match elite {
            Elite::SemiChefe => (4.0, 1.5),
            Elite::Guardiao => (GUARDIAO_VIDA, GUARDIAO_DANO),
            Elite::Nao => (1.0, 1.0),
        };
        let hp = ((hp as f32) * vida * fv).round().max(1.0) as i32;
        tag.stats.hp_max = hp;
        tag.stats.attack_damage = ((d as f32) * dano * fd).round().max(0.0) as i32;
        tag.stats.defense = crate::world::defesa_do_mob(tag.stats.defense, nivel);
        if semi {
            tag.boss_name = Some("Guardian of the Cavern".into());
            tag.size_scale *= 1.4;
            tag.xp_reward *= 3;
        }
        if elite == Elite::Guardiao {
            tag.boss_name = Some("Warden".into());
            tag.size_scale *= 1.25;
            tag.xp_reward *= 2;
        }
        let net = self.alloc_entity_id();
        let body = self.spawn_entity_body(pos);
        self.ecs.spawn((
            NetId(net),
            Position(pos),
            Velocity(Vec2::ZERO),
            Health {
                current: hp,
                max: hp,
            },
            EntityKind::Enemy(kind),
            tag,
            body,
            Instancia(inst),
        ))
    }

    /// Morreu dentro: sem XP perdido; a espera de reviver cresce 10 s por morte.
    pub(super) fn dg_morreu(&mut self, inst: u32, nome: &str) {
        let Some(i) = self.instancias.iter_mut().find(|i| i.id == inst) else {
            return;
        };
        let Some(m) = i.membros.iter_mut().find(|m| m.nome == nome) else {
            return;
        };
        m.mortes += 1;
        let espera = dg::espera_reviver_s(m.mortes) as f32;
        let sid = m.sid;
        if let Some(s) = self.sessions.get_mut(&sid) {
            s.downed_heal_timer = espera;
        }
    }

    /// Reviver: so' depois da espera, com vida cheia, no inicio do andar.
    pub(super) fn dg_reviver(&mut self, sid: SessionId) {
        let Some(s) = self.sessions.get(&sid) else {
            return;
        };
        let (inst, caido, falta) = (s.instancia, s.downed, s.downed_heal_timer);
        // Após um restart a instância em memória deixa de existir, mas uma
        // sessão que estava caída pode ainda chegar com o pedido antigo. Não
        // deixe o jogador preso: reviver nesse caso recupera no porto.
        if inst == 0 && caido {
            self.dg_levantar(sid);
            let destino = self.porto();
            self.dg_teleportar(sid, destino);
            self.dg_texto(
                sid,
                true,
                "A dungeon foi reiniciada; você foi recuperado no porto.",
            );
            self.save_pending = true;
            return;
        }
        if inst == 0 || !caido {
            return;
        }
        if falta > 0.0 {
            self.dg_texto(sid, false, format!("Revive in {:.0} s.", falta.ceil()));
            return;
        }
        let Some(andar) = self
            .instancias
            .iter()
            .find(|i| i.id == inst)
            .map(|i| i.andar)
        else {
            return;
        };
        let planta = self
            .instancias
            .iter()
            .find(|i| i.id == inst)
            .and_then(|i| i.planta);
        let arena = self.dg_arena();
        self.dg_levantar(sid);
        let volta = match planta {
            Some(p) => p.ponto_de_volta(andar),
            None => arena[(andar as usize).min(arena.len() - 1)],
        };
        self.dg_teleportar(sid, volta);
    }

    fn dg_levantar(&mut self, sid: SessionId) {
        let Some(s) = self.sessions.get_mut(&sid) else {
            return;
        };
        let (e, hp_max) = (s.entity, s.stats.hp_max);
        s.downed = false;
        s.downed_heal_timer = 0.0;
        s.downed_hp = 0;
        s.mp_current = s.stats.mp_max as f32;
        if let Some(e) = e {
            if let Ok(mut h) = self.ecs.get::<&mut Health>(e) {
                h.current = hp_max;
            }
            let _ = self.ecs.remove_one::<Untargetable>(e);
        }
    }

    /// Volta pro mundo: onde estava antes de entrar, de pe'.
    fn dg_devolver(&mut self, sid: SessionId) {
        let Some(s) = self.sessions.get_mut(&sid) else {
            return;
        };
        if s.instancia == 0 {
            return;
        }
        let retorno = s.retorno_da_dungeon.take();
        s.instancia = 0;
        let caido = s.downed;
        let e = s.entity;
        if caido {
            self.dg_levantar(sid);
        }
        if let Some(e) = e {
            let _ = self.ecs.remove_one::<Instancia>(e);
        }
        // Came in through a Porão door: out the same door, on its island.
        if self.na_arena() && self.sessions.get(&sid).is_some_and(|s| s.porao_de_volta.is_some()) {
            self.dg_avisar(sid, Aviso::Saiu);
            self.dg_voltar_pra_porta(sid, "You climb back out of the cellar.");
            self.save_pending = true;
            return;
        }
        let destino = retorno.unwrap_or_else(|| self.porto());
        self.dg_teleportar(sid, destino);
        self.dg_avisar(sid, Aviso::Saiu);
        // ACABOU A DUNGEON, E VOCÊ ESTÁ NUM SAGUÃO.
        //
        // O dono: "quando eu junto o saque, sai mas eu fico preso lá no mapa".
        // Ele não estava travado — estava na ARENA, que não tem barco, nem
        // porto, nem missão. Sair dela dependia de saber abrir a janela de
        // Dungeons e achar um botão no cabeçalho.
        //
        // Então o próprio fim da dungeon diz onde está e como voltar, e o
        // cliente usa este aviso pra reabrir a janela com a saída à mão.
        if self.na_arena() {
            self.dg_avisar(sid, Aviso::NaArena { dentro: true });
            self.dg_texto(sid, true, "Você está na Arena. Toque em Sair para voltar.");
        }
        self.save_pending = true;
    }

    /// A porta: sai, a entrada fica gasta. Venceu e nao abriu o bau: abre agora.
    fn dg_sair(&mut self, sid: SessionId) {
        let Some(inst) = self.sessions.get(&sid).map(|s| s.instancia) else {
            return;
        };
        // A instância é somente memória. Se o processo reiniciar no meio da
        // run, o jogador volta com instancia=0 e o pedido de saída antigo não
        // pode virar um no-op (isso deixava o personagem preso/caído).
        if inst == 0 {
            let caido = self.sessions.get(&sid).is_some_and(|s| s.downed);
            if caido {
                self.dg_levantar(sid);
            }
            let destino = self.porto();
            self.dg_teleportar(sid, destino);
            self.dg_texto(
                sid,
                true,
                "A dungeon foi reiniciada; você saiu com segurança.",
            );
            self.save_pending = true;
            return;
        }
        let Some(idx) = self.instancias.iter().position(|i| i.id == inst) else {
            self.dg_devolver(sid);
            return;
        };
        if matches!(self.instancias[idx].estado, EstadoDg::Concluida { .. }) {
            self.dg_dar_bau(idx, sid);
            // Saindo por ultimo: o saque do chao vem junto (ninguem mais pega).
            let outros = self.instancias[idx]
                .membros
                .iter()
                .any(|m| m.sid != sid && !m.saiu);
            if !outros {
                let id = self.instancias[idx].id;
                self.dg_recolher(id, &[sid]);
            }
        }
        if let Some(m) = self.instancias[idx]
            .membros
            .iter_mut()
            .find(|m| m.sid == sid)
        {
            m.saiu = true;
        }
        self.dg_devolver(sid);
        self.dg_texto(sid, true, "You left the dungeon.");
    }

    /// Quanto tempo se pode ficar na Arena sem estar numa dungeon.
    ///
    /// A Arena é SAGUÃO: entra-se pra formar grupo e sair lutando. Quem está
    /// nela sem instância, sem fila e sem sala não está esperando nada.
    const SO_DE_PASSAGEM_S: f32 = 45.0;

    /// Devolve pra casa quem ficou parado no saguão.
    ///
    /// O dono: "eu e vários bots estamos presos lá dentro". E estavam mesmo —
    /// mas não por falta do botão: a zona SALVA deles virou `dungeon`, então a
    /// cada login eles nasciam na Arena de novo. Uma saída que depende de o
    /// jogador achar um botão não resolve quem chega lá dormindo.
    ///
    /// A regra certa é a da própria Arena: ela é de passagem. Se ninguém mora
    /// nela, ninguém fica preso nela.
    fn tick_saguao(&mut self) {
        if !self.na_arena() {
            return;
        }
        let agora = self.sim_time_s;
        let ociosos: Vec<SessionId> = self
            .sessions
            .iter()
            .filter(|(sid, s)| {
                s.logged_in
                    && s.instancia == 0
                    && matches!(self.mesa.onde(chave(**sid)), mesa::Onde::Livre)
            })
            .map(|(sid, _)| *sid)
            .collect();
        for sid in ociosos {
            let desde = *self.saguao_desde.entry(sid).or_insert(agora);
            if agora - desde >= Self::SO_DE_PASSAGEM_S {
                self.saguao_desde.remove(&sid);
                self.sair_da_arena(sid, "The Arena is a waypoint: you went back.");
            }
        }
        // Quem entrou numa dungeon ou na fila zera o relógio.
        let dentro: Vec<SessionId> = self
            .saguao_desde
            .keys()
            .copied()
            .filter(|sid| {
                self.sessions.get(sid).is_none_or(|s| {
                    s.instancia != 0 || !matches!(self.mesa.onde(chave(*sid)), mesa::Onde::Livre)
                })
            })
            .collect();
        for sid in dentro {
            self.saguao_desde.remove(&sid);
        }
    }

    pub(super) fn tick_dungeons(&mut self) {
        self.dg_poroes_que_chegaram();
        self.tick_saguao();
        let agora = self.sim_time_s;
        if self.tick % 15 == 0 {
            let r = mesa::Regras {
                grupo_max: &|c| dg::conteudo(c).map_or(1, |x| x.grupo_max),
                minimo: &|c, e| dg::conteudo(c).map_or(1, |x| dg::minimo_da_fila(x, e)),
            };
            let ev = self.mesa.tick(agora as f64, &r);
            if !ev.is_empty() {
                self.dg_eventos(ev);
            }
        }
        for idx in (0..self.instancias.len()).rev() {
            self.dg_tick_instancia(idx);
        }
    }

    fn dg_vivo(&self, e: Entity) -> bool {
        self.ecs
            .get::<&EnemyTag>(e)
            .map(|t| !t.dead)
            .unwrap_or(false)
    }

    fn dg_tick_instancia(&mut self, idx: usize) {
        let agora = self.sim_time_s;
        let id = self.instancias[idx].id;
        // Quem caiu da conexao ou ja' saiu nao volta.
        for m in self.instancias[idx].membros.iter_mut() {
            if !m.saiu
                && !self
                    .sessions
                    .get(&m.sid)
                    .is_some_and(|s| s.logged_in && s.instancia == id)
            {
                m.saiu = true;
            }
        }
        if self.instancias[idx].membros.iter().all(|m| m.saiu) {
            self.dg_fechar(idx);
            return;
        }
        // DESAFIO "vida alta": a menor vida de cada um, amostrada a cada tick
        // enquanto a corrida anda. Amostrar aqui, e não em cada ponto de dano,
        // é o que pega TODA fonte de dano sem caçar os lugares que ferem.
        if matches!(self.instancias[idx].estado, EstadoDg::Andando) {
            let vidas: Vec<(usize, f32)> = self.instancias[idx]
                .membros
                .iter()
                .enumerate()
                .filter(|(_, m)| !m.saiu)
                .filter_map(|(i, m)| {
                    let e = self.sessions.get(&m.sid)?.entity?;
                    let h = self.ecs.get::<&Health>(e).ok()?;
                    Some((i, h.current.max(0) as f32 / h.max.max(1) as f32))
                })
                .collect();
            for (i, v) in vidas {
                let m = &mut self.instancias[idx].membros[i];
                m.vida_min = m.vida_min.min(v);
            }
        }
        let Some(c) = dg::conteudo(self.instancias[idx].conteudo) else {
            return;
        };
        let arena = self.dg_arena();
        let andar = self.instancias[idx].andar;
        let centro = arena[(andar as usize).min(arena.len() - 1)];
        let presentes: Vec<SessionId> = self.instancias[idx]
            .membros
            .iter()
            .filter(|m| !m.saiu)
            .map(|m| m.sid)
            .collect();

        match self.instancias[idx].estado {
            EstadoDg::Andando => {
                if agora >= self.instancias[idx].limite {
                    self.dg_terminar(idx, false);
                    return;
                }
                // With a plan, the walls hold everyone in (the physics). A
                // teleport can still drop someone outside: back in.
                if let Some(p) = self.instancias[idx].planta {
                    for sid in &presentes {
                        let fora = self
                            .sessions
                            .get(sid)
                            .and_then(|s| s.entity)
                            .and_then(|e| self.ecs.get::<&Position>(e).ok().map(|q| q.0))
                            .filter(|q| !p.livre(*q, ENTITY_RADIUS, andar));
                        if let Some(q) = fora {
                            let dentro = p.mais_perto(q, ENTITY_RADIUS, andar);
                            self.dg_teleportar(*sid, dentro);
                        }
                    }
                }
                // Longe demais do andar: volta pro centro dele.
                let sem_planta = self.instancias[idx].planta.is_none();
                for sid in presentes.iter().filter(|_| sem_planta) {
                    let longe = self.sessions.get(sid).and_then(|s| s.entity).and_then(|e| {
                        self.ecs
                            .get::<&Position>(e)
                            .ok()
                            .map(|p| p.0.distance(centro) > RAIO_DO_ANDAR)
                    });
                    if longe == Some(true) {
                        self.dg_teleportar(*sid, centro);
                    }
                }
                // Wipe: todos caidos ao mesmo tempo. O andar recomeca inteiro;
                // o relogio nao para e nao ha' limite de wipes.
                let todos_caidos = presentes
                    .iter()
                    .all(|sid| self.sessions.get(sid).is_some_and(|s| s.downed));
                if !todos_caidos {
                    self.instancias[idx].wipe_tratado = false;
                }
                if todos_caidos && !self.instancias[idx].wipe_tratado {
                    self.instancias[idx].wipe_tratado = true;
                    self.instancias[idx].wipes += 1;
                    crate::telemetria::conta(
                        "dungeon_wipe",
                        format!(
                            "{}:{}",
                            self.instancias[idx].conteudo, self.instancias[idx].estagio
                        ),
                        1,
                    );
                    tracing::info!(
                        "[dungeon] instancia {id}: wipe no andar {} (#{})",
                        andar + 1,
                        self.instancias[idx].wipes
                    );
                    self.dg_povoar_andar(idx);
                    for sid in &presentes {
                        self.dg_texto(*sid, false, "The party went down: the floor restarted.");
                    }
                    self.instancias[idx].aviso_em = 0.0;
                } else if !todos_caidos {
                    let limpo = self.instancias[idx].vivos.iter().all(|e| !self.dg_vivo(*e));
                    // A planned room opens its gate once it is FULLY CLEARED —
                    // the Warden and its whole pack. The owner, 30/09/2026:
                    // "make each room need full clear" (the first version
                    // opened on the Warden alone and let the pack stay).
                    let sala_limpa = self.instancias[idx].planta.is_some()
                        && andar < c.andares
                        && limpo
                        && !self.instancias[idx].vivos.is_empty();
                    if sala_limpa {
                        let vivos = std::mem::take(&mut self.instancias[idx].vivos);
                        let (ficam, mortos): (Vec<Entity>, Vec<Entity>) =
                            vivos.into_iter().partition(|e| self.dg_vivo(*e));
                        self.instancias[idx].restos.extend(ficam);
                        self.dg_despawn(mortos);
                        self.instancias[idx].andar += 1;
                        self.dg_povoar_andar(idx);
                        let texto = if andar + 1 >= c.andares {
                            "Room cleared. The way to the boss is open."
                        } else {
                            "Room cleared. A gate opens."
                        };
                        for sid in &presentes {
                            self.dg_texto(*sid, true, texto);
                        }
                        tracing::info!("[dungeon] instancia {id}: portao {} aberto", andar + 1);
                    } else if limpo && !self.instancias[idx].vivos.is_empty() {
                        if andar < c.andares {
                            self.instancias[idx].andar += 1;
                            let proximo = arena[((andar + 1) as usize).min(arena.len() - 1)];
                            for (i, sid) in presentes.iter().enumerate() {
                                let volta =
                                    Vec2::new((i as f32 * 1.3).cos(), (i as f32 * 1.3).sin()) * 3.0;
                                self.dg_teleportar(*sid, proximo + volta);
                            }
                            self.dg_povoar_andar(idx);
                            tracing::info!(
                                "[dungeon] instancia {id}: andar {} liberado",
                                andar + 1
                            );
                        } else {
                            self.dg_terminar(idx, true);
                            return;
                        }
                    }
                }
            }
            EstadoDg::Concluida { em } => {
                if agora >= em + BAU_ABRE_SOZINHO_S {
                    for sid in presentes.clone() {
                        self.dg_dar_bau(idx, sid);
                    }
                }
                if agora >= em + FECHA_DEPOIS_DE_VENCER_S {
                    self.dg_fechar(idx);
                    return;
                }
            }
            EstadoDg::Falhou { em } => {
                if agora >= em + FECHA_DEPOIS_DE_FALHAR_S {
                    self.dg_fechar(idx);
                    return;
                }
            }
        }

        if agora >= self.instancias[idx].aviso_em {
            self.instancias[idx].aviso_em = agora + 1.0;
            let i = &self.instancias[idx];
            let inimigos = i.vivos.iter().filter(|e| self.dg_vivo(**e)).count() as u16;
            let restante = match i.estado {
                EstadoDg::Andando => (i.limite - agora).max(0.0) as u32,
                _ => 0,
            };
            let membros: Vec<dg::MembroDaInstancia> = i
                .membros
                .iter()
                .filter(|m| !m.saiu)
                .map(|m| dg::MembroDaInstancia {
                    nome: format!("{}@{}", m.nome, crate::canais::realm()),
                    vivo: !self.sessions.get(&m.sid).is_some_and(|s| s.downed),
                })
                .collect();
            let (conteudo, estagio, andar, andares, concluida) = (
                i.conteudo,
                i.estagio,
                i.andar,
                c.andares,
                !matches!(i.estado, EstadoDg::Andando),
            );
            let centro = match self.instancias[idx].planta {
                Some(p) => p.sala_da_etapa(andar).map_or(p.ponto_de_volta(andar), |s| p.centro(s)),
                None => self.dg_arena()[(andar as usize).min(3)],
            };
            for sid in presentes {
                let reviver_em_s = self
                    .sessions
                    .get(&sid)
                    .filter(|s| s.downed)
                    .map(|s| s.downed_heal_timer.max(0.0).ceil() as u16);
                self.dg_avisar(
                    sid,
                    Aviso::Instancia {
                        conteudo,
                        estagio,
                        andar,
                        andares,
                        restante_s: restante,
                        inimigos,
                        reviver_em_s,
                        membros: membros.clone(),
                        concluida,
                        centro: [centro.x, centro.y],
                    },
                );
            }
        }
    }

    /// Venceu (chefe morto) ou falhou (tempo). Venceu: bau no chao, vitoria
    /// registrada, 1ª vitoria no correio, diaria de dungeon.
    fn dg_terminar(&mut self, idx: usize, vitoria: bool) {
        let agora = self.sim_time_s;
        let (id, conteudo, estagio, inicio, limite) = {
            let i = &self.instancias[idx];
            (i.id, i.conteudo, i.estagio, i.inicio, i.limite)
        };
        let Some(c) = dg::conteudo(conteudo) else {
            return;
        };
        let tempo_s = (agora - inicio).max(0.0) as u32;
        let bonus = vitoria && dg::bonus_tempo(tempo_s, (limite - inicio).max(1.0) as u32);
        {
            let chave = format!("{conteudo}:{estagio}");
            crate::telemetria::conta(
                "dungeon_resultado",
                format!(
                    "{chave}:{}",
                    if vitoria { "vitoria" } else { "tempo_esgotado" }
                ),
                1,
            );
            crate::telemetria::conta("dungeon_tempo_s", &chave, tempo_s as i64);
            if bonus {
                crate::telemetria::conta("dungeon_bonus_tempo", &chave, 1);
            }
        }
        if !vitoria {
            self.instancias[idx].estado = EstadoDg::Falhou { em: agora };
            let velhos = std::mem::take(&mut self.instancias[idx].vivos);
            self.dg_despawn(velhos);
            tracing::info!("[dungeon] instancia {id}: tempo esgotado — estagio falho");
            let presentes: Vec<SessionId> = self.instancias[idx]
                .membros
                .iter()
                .filter(|m| !m.saiu)
                .map(|m| m.sid)
                .collect();
            for sid in presentes {
                self.dg_avisar(
                    sid,
                    Aviso::Resultado {
                        conteudo,
                        estagio,
                        vitoria: false,
                        tempo_s,
                        bonus_tempo: false,
                        primeira_vitoria: false,
                        recompensas_primeira: Vec::new(),
                    },
                );
            }
            return;
        }
        self.instancias[idx].estado = EstadoDg::Concluida { em: agora };
        // O bau aparece onde o chefe caiu.
        let onde = self.instancias[idx]
            .vivos
            .first()
            .and_then(|e| self.ecs.get::<&Position>(*e).ok().map(|p| p.0))
            .unwrap_or_else(|| self.dg_arena()[c.andares as usize % 4]);
        let eid = self.alloc_entity_id();
        let bau = self.ecs.spawn((
            NetId(eid),
            Position(self.chao_livre(onde)),
            Velocity(Vec2::ZERO),
            EntityKind::Npc(0),
            NpcDaVilaTag {
                nome: format!("Chest · {}", c.nome),
                rumo: shared::npc_kind(None, dg::PAPEL_BAU),
                giver: None,
            },
            Instancia(id),
        ));
        self.instancias[idx].bau = Some((bau, eid));
        tracing::info!("[dungeon] instancia {id}: {} estagio {estagio} vencida em {tempo_s}s (bonus de tempo: {bonus})", c.nome);
        self.instancias[idx].aviso_em = 0.0;
        let semana = dg::semana(unix_agora());
        let quando = unix_agora();
        let presentes: Vec<(SessionId, bool)> = self.instancias[idx]
            .membros
            .iter()
            .filter(|m| !m.saiu)
            .map(|m| (m.sid, m.ajudante))
            .collect();
        for (sid, ajudante) in presentes {
            let mut primeira = false;
            let mut recompensas_primeira = Vec::new();
            if let Some(s) = self.sessions.get_mut(&sid) {
                let de_todas = s.dungeon.registrar_vitoria(conteudo, estagio);
                s.conta_dungeon.virar(semana);
                let semanal = !ajudante && s.conta_dungeon.primeira_da_semana(conteudo, estagio);
                primeira = semanal || de_todas;
                let mut rng = || fastrand::f32();
                if semanal {
                    let p = dg::peca_garantida(c, estagio, &mut rng);
                    let inst = GameWorld::dg_rolar_peca(&p);
                    s.dungeon.postar(p.item_id, 1, inst, 1, quando);
                    recompensas_primeira.push((p.item_id, 1));
                }
                if de_todas && !ajudante {
                    let p = dg::peca_garantida(c, estagio, &mut rng);
                    let inst = GameWorld::dg_rolar_peca(&p);
                    s.dungeon.postar(p.item_id, 1, inst, 2, quando);
                    recompensas_primeira.push((p.item_id, 1));
                    let nivel = dg::nivel_do_estagio(c, estagio);
                    let cor = shared::chaves::faixa(nivel).cor;
                    let base = shared::item_id::CHAVES[fastrand::usize(..4)];
                    let chave = shared::item_id::chave_na_cor(base, cor);
                    s.dungeon.postar(chave, 1, None, 2, quando);
                    recompensas_primeira.push((chave, 1));
                }
            }
            // Alvo 0 = qualquer dungeon (diarias, 510); a historia nomeia a dela.
            self.quest_on_evento_se(sid, shared::quests::objective_kind::DUNGEON, 1, &|d| {
                d.obj_target == 0 || d.obj_target == conteudo
            });
            self.dg_avisar(
                sid,
                Aviso::Resultado {
                    conteudo,
                    estagio,
                    vitoria: true,
                    tempo_s,
                    bonus_tempo: bonus,
                    primeira_vitoria: primeira,
                    recompensas_primeira,
                },
            );
            if primeira {
                self.dg_enviar_correio(sid);
                self.dg_texto(sid, true, "Primeira vitória! A recompensa está no Correio.");
            }
        }
        self.save_pending = true;
    }

    pub(super) fn dg_rolar_peca(p: &dg::Premio) -> Option<shared::ItemInstance> {
        let (_, ilvl) = p.peca?;
        let tpl = crate::economy::item_template_of(p.item_id);
        shared::ItemInstance::roll_with_template(tpl, ilvl, || fastrand::f32()).map(|mut i| {
            i.vinculado = true;
            i
        })
    }

    /// Toque no bau (via `Interact`). `true` = era o bau (tratado aqui).
    pub(super) fn dg_abrir_bau(&mut self, sid: SessionId, eid: EntityId) -> bool {
        let Some(idx) = self
            .instancias
            .iter()
            .position(|i| i.bau.is_some_and(|b| b.1 == eid))
        else {
            return false;
        };
        let inst = self.instancias[idx].id;
        let Some(s) = self.sessions.get(&sid) else {
            return true;
        };
        if s.instancia != inst {
            return true;
        }
        let perto = match (s.entity, self.instancias[idx].bau) {
            (Some(pe), Some((be, _))) => {
                let a = self.ecs.get::<&Position>(pe).map(|p| p.0).ok();
                let b = self.ecs.get::<&Position>(be).map(|p| p.0).ok();
                matches!((a, b), (Some(a), Some(b)) if a.distance(b) <= ALCANCE_DO_BAU)
            }
            _ => false,
        };
        if !perto {
            self.dg_texto(sid, false, "Get close to the chest.");
            return true;
        }
        self.dg_dar_bau(idx, sid);
        true
    }

    /// Rola e entrega o bau de um membro, uma vez. O que nao cabe vai pro correio.
    fn dg_dar_bau(&mut self, idx: usize, sid: SessionId) {
        let (uid, conteudo, estagio, inicio, limite, em) = {
            let i = &self.instancias[idx];
            let EstadoDg::Concluida { em } = i.estado else {
                return;
            };
            (i.uid, i.conteudo, i.estagio, i.inicio, i.limite, em)
        };
        let Some(c) = dg::conteudo(conteudo) else {
            return;
        };
        let Some(m) = self.instancias[idx]
            .membros
            .iter_mut()
            .find(|m| m.sid == sid)
        else {
            return;
        };
        if m.abriu_bau {
            return;
        }
        m.abriu_bau = true;
        let ajudante = m.ajudante;
        let (pocoes0, dashes0, vida_min, mortes) =
            (m.pocoes_na_entrada, m.dashes_na_entrada, m.vida_min, m.mortes);
        let tempo_s = (em - inicio).max(0.0) as u32;
        let bonus = dg::bonus_tempo(tempo_s, (limite - inicio).max(1.0) as u32);
        let quando = unix_agora();
        let Some(s) = self.sessions.get_mut(&sid) else {
            return;
        };
        // Idempotente no banco: o id do bau vai no MESMO save da bolsa.
        if !s.dungeon.abrir_bau(uid) {
            return;
        }
        let mut rng = || fastrand::f32();
        let mut bau = dg::rolar_bau(c, estagio, ajudante, bonus, &mut rng);
        // OS DESAFIOS: cada um cumprido soma `BONUS_POR_DESAFIO` ao que é
        // CONTÁVEL no baú — ouro, cobre, marcas e material. Peça de
        // equipamento não multiplica: dobrar a quantidade de uma espada não
        // quer dizer nada, e a raridade dela é outra conversa.
        let placar = shared::desafio::Placar {
            segundos: tempo_s as f32,
            vida_min,
            pocoes: s.pocoes_bebidas.saturating_sub(pocoes0),
            mortes,
            dashes: s.dashes_feitos.saturating_sub(dashes0),
        };
        let desafios = self.instancias[idx].desafios.clone();
        let feitos: Vec<(shared::desafio::Desafio, bool)> =
            desafios.iter().map(|d| (*d, d.cumpriu(c, &placar))).collect();
        let n_ok = feitos.iter().filter(|(_, ok)| *ok).count();
        let mult = shared::desafio::multiplicador(n_ok);
        if n_ok > 0 {
            for p in bau.itens.iter_mut().filter(|p| p.peca.is_none()) {
                p.qtd = ((p.qtd as f32) * mult).round() as u32;
            }
            bau.marcas = ((bau.marcas as f32) * mult).round() as u32;
        }
        let Some(s) = self.sessions.get_mut(&sid) else {
            return;
        };
        let mut itens = Vec::new();
        let mut no_correio = 0u8;
        let mut premios: Vec<(u16, u32, Option<shared::ItemInstance>)> = bau
            .itens
            .iter()
            .map(|p| (p.item_id, p.qtd, GameWorld::dg_rolar_peca(p)))
            .collect();
        if bau.marcas > 0 {
            premios.push((shared::item_id::MARCAS_TEMPESTADE, bau.marcas, None));
        }
        crate::telemetria::conta_de(&s.name, "dungeon_bau", format!("{conteudo}:{estagio}"), 1);
        for (item, qtd, inst) in premios {
            crate::telemetria::conta_de(&s.name, "dungeon_bau_item", item, qtd as i64);
            if shared::item_id::todas_as_chaves().contains(&item) {
                crate::telemetria::conta_de(&s.name, "chave_drop", format!("dungeon:{item}"), qtd as i64);
            }
            itens.push((item, qtd));
            if !add_to_inventory(&mut s.inventory, item, qtd, inst) {
                s.dungeon.postar(item, qtd, inst, 0, quando);
                no_correio += 1;
            }
        }
        s.inventory_dirty = true;
        self.save_pending = true;
        tracing::info!(
            "[dungeon] bau de {} aberto: {:?} + {} marcas ({} no correio)",
            s.name,
            itens,
            bau.marcas,
            no_correio
        );
        if !feitos.is_empty() {
            let linhas: Vec<String> = feitos
                .iter()
                // Texto puro: a fonte do jogo não tem ✔/✘ (viraram caixinhas
                // vazias na prévia da porta, com a estrela).
                .map(|(d, ok)| format!("{} ({})", d.texto(c), if *ok { "done" } else { "missed" }))
                .collect();
            let bonus = if n_ok > 0 {
                format!(" — chest +{:.0}%", (mult - 1.0) * 100.0)
            } else {
                String::new()
            };
            self.dg_texto(sid, n_ok > 0, format!("Challenges: {}{bonus}", linhas.join(" · ")));
            crate::telemetria::conta_de(&self.nome_de(sid), "porao_desafios", format!("{conteudo}:{n_ok}"), 1);
        }
        self.dg_avisar(
            sid,
            Aviso::Bau {
                itens,
                marcas: bau.marcas,
                no_correio,
            },
        );
    }

    /// O saque que ficou no chao da instancia `id`, repartido entre `quem`
    /// (um item pra cada, em roda). Entra na bolsa; o que nao cabe vai pras
    /// Entregas da dungeon. Antes, fechar a instancia apagava tudo.
    fn dg_recolher(&mut self, id: u32, quem: &[SessionId]) {
        if quem.is_empty() {
            return;
        }
        let saque: Vec<(Entity, EntityId, LootTag)> = self
            .ecs
            .query::<(&NetId, &LootTag, &Instancia)>()
            .iter()
            .filter(|(_, (_, _, i))| i.0 == id)
            .map(|(e, (n, l, _))| (e, n.0, l.clone()))
            .collect();
        if saque.is_empty() {
            return;
        }
        let quando = unix_agora();
        let mut recolhidos = vec![0u32; quem.len()];
        for (k, (e, eid, l)) in saque.into_iter().enumerate() {
            let sid = quem[k % quem.len()];
            let Some(s) = self.sessions.get_mut(&sid) else {
                continue;
            };
            if l.item_id == shared::item_id::GOLD {
                s.gold = s.gold.saturating_add(l.qty as u64);
            } else if add_to_inventory(&mut s.inventory, l.item_id, l.qty, l.instance) {
                s.inventory_dirty = true;
            } else {
                s.dungeon.postar(l.item_id, l.qty, l.instance, 0, quando);
            }
            recolhidos[k % quem.len()] += 1;
            let _ = self.ecs.despawn(e);
            self.removed_this_tick.push(eid);
        }
        for (i, sid) in quem.iter().enumerate() {
            if recolhidos[i] > 0 {
                self.dg_texto(
                    *sid,
                    true,
                    &format!("{} item(s) picked up off the ground.", recolhidos[i]),
                );
            }
        }
        self.save_pending = true;
    }

    fn dg_fechar(&mut self, idx: usize) {
        {
            let i = &self.instancias[idx];
            let presentes: Vec<SessionId> = i
                .membros
                .iter()
                .filter(|m| !m.saiu)
                .map(|m| m.sid)
                .collect();
            let id = i.id;
            self.dg_recolher(id, &presentes);
        }
        let inst = self.instancias.remove(idx);
        let mut sobra = inst.vivos;
        sobra.extend(inst.restos);
        if let Some((b, _)) = inst.bau {
            sobra.push(b);
        }
        // Saque que ficou no chao da instancia some junto.
        let saque: Vec<Entity> = self
            .ecs
            .query::<(&LootTag, &Instancia)>()
            .iter()
            .filter(|(_, (_, i))| i.0 == inst.id)
            .map(|(e, _)| e)
            .collect();
        sobra.extend(saque);
        self.dg_despawn(sobra);
        for m in inst.membros.iter().filter(|m| !m.saiu) {
            self.dg_devolver(m.sid);
        }
        tracing::info!("[dungeon] instancia {} fechada", inst.id);
    }

    // ─────────────────────────────── correio ───────────────────────────────

    pub(super) fn dg_enviar_correio(&self, sid: SessionId) {
        let Some(s) = self.sessions.get(&sid) else {
            return;
        };
        let cartas = s
            .dungeon
            .correio
            .iter()
            .map(|c| dg::CartaNet {
                id: c.id,
                item_id: c.item_id,
                qtd: c.qtd,
                motivo: c.motivo,
            })
            .collect();
        let _ = s.handle.to_client.send(ServerMessage::Dungeon {
            aviso: Aviso::Correio { cartas },
        });
    }

    fn dg_retirar_carta(&mut self, sid: SessionId, id: u64) {
        let Some(s) = self.sessions.get_mut(&sid) else {
            return;
        };
        let Some(i) = s.dungeon.correio.iter().position(|c| c.id == id) else {
            return;
        };
        let carta = s.dungeon.correio[i];
        if !add_to_inventory(&mut s.inventory, carta.item_id, carta.qtd, carta.instance) {
            let _ = s.handle.to_client.send(ServerMessage::Dungeon {
                aviso: Aviso::Texto {
                    ok: false,
                    texto: "Bag full.".into(),
                },
            });
            return;
        }
        s.dungeon.correio.remove(i);
        s.inventory_dirty = true;
        self.save_pending = true;
        self.dg_enviar_correio(sid);
    }

    /// Saiu do jogo: sai da fila/sala/pronto-check (a instancia percebe no tick).
    pub(super) fn dg_desconectou(&mut self, sid: SessionId) {
        let ev = self.mesa.remover(chave(sid), self.sim_time_s as f64);
        self.dg_eventos(ev);
    }

    /// Instancia de uma entidade por NetId (0 = mundo aberto).
    pub(super) fn dg_instancias_por_eid(&self) -> HashMap<EntityId, u32> {
        self.ecs
            .query::<(&NetId, &Instancia)>()
            .iter()
            .map(|(_, (n, i))| (n.0, i.0))
            .collect()
    }
}

/// Can a body of radius `corpo` stand at `para`, and get there from `de` on
/// foot? Walking climbs one block at a time (`DEGRAU_BLOCOS`): a ledge two
/// blocks up is a jump the AI never makes, and from up there a mob neither
/// reaches the party nor gets reached. The footing must be level too — the
/// body stands on the highest block under it.
pub(super) fn a_pe(ilha: &shared::terreno::Ilha, de: Vec2, para: Vec2, corpo: f32) -> bool {
    use shared::terreno::{BLOCO, DEGRAU_BLOCOS};
    if ilha.agua(para.x, para.y) || !ilha.sem_estorvo(para, corpo) {
        return false;
    }
    let bloco = |q: Vec2| {
        let (x, z) = ilha.coluna(q.x, q.y);
        ilha.bloco(x, z)
    };
    let passos = (de.distance(para) / (BLOCO * 0.5)).ceil().max(1.0) as i32;
    let mut antes = bloco(de);
    for s in 1..=passos {
        let q = de.lerp(para, s as f32 / passos as f32);
        let b = bloco(q);
        if (b - antes).abs() > DEGRAU_BLOCOS || ilha.agua(q.x, q.y) {
            return false;
        }
        antes = b;
    }
    let (cx, cz) = ilha.coluna(para.x, para.y);
    ilha.sitio_plano(cx, cz, (corpo / BLOCO).ceil() as i32)
}

#[cfg(test)]
mod testes {
    use super::*;

    /// Every Cavern mob and boss stands on ground the party can walk to.
    ///
    /// Reported by the owner on 01/10/2026: "some mobs in the normal dungeon
    /// are spawning in non walkable areas". The floor's site is flat only
    /// around its centre, and the ring of mobs reaches past it. Runs on the
    /// real Arena relief, every floor site, every slot of a full floor.
    #[test]
    fn cavern_mobs_spawn_on_walkable_ground() {
        crate::economy::init_vazia_para_testes();
        let mut w = GameWorld::new(HashMap::new());
        w.zona = shared::arena::ZONA.to_string();
        w.ilha = Some(shared::terreno::Ilha::da_ilha(&shared::arena::DEF));
        let sitios = w.dg_arena();
        assert!(!sitios.is_empty(), "the Arena has floor sites");
        let n = 12;
        let (mut antes_ruins, mut total) = (0, 0);
        for centro in sitios {
            let mut quer: Vec<Vec2> = (0..n)
                .map(|i| {
                    let ang = i as f32 * std::f32::consts::TAU / n as f32;
                    centro + Vec2::new(ang.cos(), ang.sin()) * (10.0 + (i % 3) as f32 * 4.0)
                })
                .collect();
            quer.push(centro + Vec2::new(0.0, 14.0)); // the boss
            for q in quer {
                let ilha = w.ilha.as_ref().unwrap();
                total += 1;
                if !a_pe(ilha, centro, q, ENTITY_RADIUS) {
                    antes_ruins += 1;
                }
                let p = w.dg_chao_do_mob(q, centro, ENTITY_RADIUS * 1.4, None);
                assert!(
                    a_pe(ilha, centro, p, ENTITY_RADIUS * 1.4),
                    "{p:?} (asked {q:?}, floor {centro:?}) is not walkable"
                );
                assert!(p.distance(centro) <= RAIO_DO_ANDAR * 0.7, "outside the leash");
            }
        }
        println!("raw ring slots off walkable ground: {antes_ruins} of {total}");
    }

    /// In a Porão every mob starts inside the floor plan, clear of the walls
    /// and the cave's rock bumps.
    #[test]
    fn porao_mobs_spawn_inside_the_room() {
        crate::economy::init_vazia_para_testes();
        let mut w = GameWorld::new(HashMap::new());
        w.zona = shared::arena::ZONA.to_string();
        w.ilha = Some(shared::terreno::Ilha::da_ilha(&shared::arena::DEF));
        let (mut fora, mut antigo_fora, mut total) = (0, 0, 0);
        for c in dg::CONTEUDOS.iter().filter(|c| c.tipo == Tipo::Porao) {
            let Some(p) = shared::planta::da(c.id) else { continue };
            for andar in 0..=c.andares {
                let Some(sala) = p.sala_da_etapa(andar) else { continue };
                let (centro, raio) = (p.centro(sala), p.salas[sala].raio);
                let n = dg::inimigos_do_andar(c, andar).max(12);
                for i in 0..n {
                    let t = (i as f32 / n as f32).sqrt();
                    let longe = if i == 0 { 0.0 } else { raio * (0.25 + 0.5 * t) };
                    let ang = i as f32 * 2.399_963;
                    let q = centro + Vec2::new(ang.cos(), ang.sin()) * longe;
                    let corpo = ENTITY_RADIUS * 1.4;
                    total += 1;
                    if !p.livre(q, corpo, andar) {
                        fora += 1;
                    }
                    // What the spawn did before 01/10/2026.
                    if !p.livre(w.chao_livre(q), ENTITY_RADIUS, andar) {
                        antigo_fora += 1;
                    }
                    let pos = w.dg_chao_do_mob(q, centro, corpo, Some((p, andar)));
                    assert!(p.livre(pos, corpo, andar), "{}: floor {andar}, {pos:?} in the wall", c.nome);
                }
            }
        }
        println!("of {total} slots: {fora} raw in the wall, {antigo_fora} in the wall after chao_livre");
    }

    #[test]
    fn menu_recompensas_preserva_estagio_e_reseta_apenas_a_semana() {
        crate::economy::init_vazia_para_testes();
        let mut w = GameWorld::new(HashMap::new());
        let sid = SessionId(([127,0,0,1],19882).into());
        let (tx, mut rx) = mpsc::unbounded_channel();
        w.on_connect(SessionHandle { id: sid, to_client: tx });
        let s = w.sessions.get_mut(&sid).unwrap();
        s.logged_in = true;
        s.dungeon.registrar_vitoria(10,1);
        // O estado deve continuar correto depois de salvar e recarregar.
        s.dungeon = serde_json::from_str(&serde_json::to_string(&s.dungeon).unwrap()).unwrap();
        s.conta_dungeon.virar(dg::semana(unix_agora()));
        s.conta_dungeon.primeira_da_semana(10,2);
        for nova_semana in [false,true] {
            if nova_semana { w.sessions.get_mut(&sid).unwrap().conta_dungeon.semana -= 1; }
            w.dg_enviar_estado(sid);
            let mut conteudos = None;
            while let Ok(msg) = rx.try_recv() {
                if let ServerMessage::Dungeon { aviso: Aviso::Estado { conteudos: c, .. } } = msg { conteudos = Some(c); }
            }
            let c = conteudos.unwrap().into_iter().find(|c| c.id == 10).unwrap();
            assert_eq!(&c.primeiras_concluidas[..2], &[true,false]);
            assert_eq!(&c.semanais_recebidas[..2], &[false,!nova_semana]);
            assert!(c.drops_chefe.contains(&shared::item_id::GOLD));
        }
    }

    /// A PORTA CONFERE DISTÂNCIA E CHAVE, e as duas no servidor.
    ///
    /// São as duas mentiras que um cliente adulterado contaria pra entrar de
    /// graça: "estou na porta" e "tenho a chave". Nenhuma das duas vem no
    /// pedido — a posição sai da entidade e a chave sai da bolsa —, e este
    /// teste é o que prova que continua assim.
    ///
    /// Também prova que a chave é GASTA: sem isso ela abriria o Porão pra
    /// sempre, e a única torneira do conteúdo ficaria aberta.
    #[test]
    fn a_porta_do_porao_exige_estar_perto_e_ter_a_chave() {
        crate::economy::init_vazia_para_testes();
        let porao = dg::CONTEUDOS
            .iter()
            .find(|c| c.tipo == Tipo::Porao)
            .expect("um porão no catálogo");
        let mut w = GameWorld::new(HashMap::new());
        w.zona = porao.zona.to_string();
        // A ILHA DE VERDADE, senão não há porta e o teste passa sem medir
        // nada: `porta_do_porao` ancora na cidade, e `GameWorld::new` nasce
        // sem ilha. Foi exatamente assim que esta primeira versão passou
        // verde sem executar uma linha do que queria provar.
        let def = shared::terreno::def_da_zona(porao.zona).expect("def da ilha do porão");
        w.ilha = Some(shared::terreno::Ilha::da_ilha(def));
        let porta = w
            .porta_do_porao(porao)
            .expect("a ilha tem cidade, logo o porão tem porta");
        let sid = SessionId(([127, 0, 0, 1], 19883).into());
        let (tx, mut rx) = mpsc::unbounded_channel();
        w.on_connect(SessionHandle { id: sid, to_client: tx });
        let e = w.ecs.spawn((
            NetId(EntityId(950)),
            Position(porta + Vec2::new(400.0, 0.0)),
            Velocity(Vec2::ZERO),
            EntityKind::Player,
            Health { current: 100, max: 100 },
        ));
        {
            let s = w.sessions.get_mut(&sid).unwrap();
            s.logged_in = true;
            s.entity = Some(e);
            s.inventory = vec![shared::InventorySlot::default(); 8];
            // Nível e poder acima do cadeado do conteúdo: sem isto `dg_recusa`
            // barra antes por nível, e o teste mediria o cadeado em vez da
            // porta — que foi o que aconteceu na primeira tentativa.
            s.xp = shared::xp_for_level(porao.nivel_min + 10);
            s.stats.hp_max = 100_000;
            s.stats.attack_damage = 100_000;
            s.stats.defense = 100_000;
        }
        assert!(
            w.dg_recusa(sid, porao, 1).is_none(),
            "o personagem do teste ainda está barrado por cadeado: {:?}",
            w.dg_recusa(sid, porao, 1)
        );
        let chave = shared::porao::chave_de(porao).unwrap();
        let recado = |rx: &mut mpsc::UnboundedReceiver<ServerMessage>| -> String {
            let mut t = String::new();
            while let Ok(m) = rx.try_recv() {
                if let ServerMessage::Dungeon { aviso: Aviso::Texto { texto, .. } } = m {
                    t = texto;
                }
            }
            t
        };

        // Longe e sem chave: recusa por distância, e a chave nem é olhada.
        w.dg_abrir_porao(sid, porao.id);
        assert!(
            recado(&mut rx).contains("closer"),
            "longe da porta tinha que reclamar da distância"
        );
        assert_eq!(w.sessions[&sid].instancia, 0, "entrou de longe");

        // Na porta, mas sem chave.
        w.ecs.get::<&mut Position>(e).unwrap().0 = porta;
        w.dg_abrir_porao(sid, porao.id);
        assert!(
            recado(&mut rx).contains("need a"),
            "na porta e sem chave tinha que pedir a chave"
        );
        assert_eq!(w.sessions[&sid].instancia, 0, "entrou sem chave");

        // At the door with the key, but the Arena (where the cellar is) is
        // down — this test world has no directory: nobody goes anywhere, and
        // the key stays in the bag.
        w.sessions.get_mut(&sid).unwrap().inventory[0] = shared::InventorySlot {
            item_id: chave,
            qty: 1,
            instance: None,
        };
        w.dg_abrir_porao(sid, porao.id);
        assert!(
            recado(&mut rx).contains("can't be reached"),
            "with the Arena down the door has to say so"
        );
        let s = &w.sessions[&sid];
        assert_eq!(s.instancia, 0);
        assert_eq!(s.dungeon.porao_pendente, 0, "left a run pending that will never start");
        assert!(
            s.inventory.iter().any(|i| i.item_id == chave && i.qty == 1),
            "the key was spent on a trip that didn't happen"
        );
    }

    /// OUT WHERE YOU CAME IN. The owner: finishing a dungeon sent the player
    /// to the city of the map, and it "should return to the dungeon portal
    /// where he enters". The spot is remembered on the way in, in the saved
    /// dungeon data (it has to survive the zone handoff), and leaving the
    /// Arena lands there — not in the city.
    #[test]
    fn sair_da_arena_volta_pro_ponto_de_onde_entrou() {
        crate::economy::init_vazia_para_testes();
        let bosque = shared::terreno::ARQUIPELAGO[0].zona;
        let porta = Vec2::new(123.0, -45.0);
        let mut w = GameWorld::new(HashMap::new());
        w.zona = bosque.to_string();
        w.diretorio = Some(crate::canais::Diretorio::para_teste(&[shared::arena::ZONA, bosque]));
        let sid = SessionId(([127, 0, 0, 1], 19885).into());
        let (tx, _rx) = mpsc::unbounded_channel();
        w.on_connect(SessionHandle { id: sid, to_client: tx });
        let e = w.ecs.spawn((
            NetId(EntityId(952)),
            Position(porta),
            Velocity(Vec2::ZERO),
            EntityKind::Player,
            Health { current: 100, max: 100 },
        ));
        {
            let s = w.sessions.get_mut(&sid).unwrap();
            s.logged_in = true;
            s.entity = Some(e);
            s.name = "Volta".into();
        }
        assert!(w.entrar_na_arena_com(sid, "down"));
        let d = &w.sessions[&sid].dungeon;
        assert_eq!(d.arena_volta, bosque);
        assert_eq!(d.arena_volta_pos, Some([porta.x, porta.y]), "the door spot was not remembered");

        // The same character, now on the Arena process (what the handoff
        // carries is the saved dungeon data).
        w.zona = shared::arena::ZONA.to_string();
        w.zona_de_saida.clear();
        w.ecs.get::<&mut Position>(e).unwrap().0 = shared::arena::CHEGADA;
        w.sair_da_arena(sid, "up");
        let onde = w.ecs.get::<&Position>(e).unwrap().0;
        assert_eq!(onde, porta, "came out somewhere else than where they went in");
        let cidade = shared::terreno::Gerador::da_ilha(shared::terreno::def_da_zona(bosque).unwrap())
            .cidade()
            .map(|c| c.centro());
        assert_ne!(Some(onde), cidade, "landed in the city");
        assert!(w.sessions[&sid].dungeon.arena_volta_pos.is_none(), "the spot must be used once");
        // Already on the way out: a second trip is refused, so it can't
        // overwrite the first (the Leave button's double send).
        assert!(w.ja_de_saida(sid));
    }

    /// DOWN THE DOOR, INTO THE PLAN: a character landing on the Arena with a
    /// Porão pending starts the run in the plan's entrance room, and only a
    /// FULLY cleared room opens the next gate.
    #[test]
    fn quem_desce_a_porta_comeca_na_planta_e_o_guardiao_abre_o_portao() {
        crate::economy::init_vazia_para_testes();
        let porao = dg::CONTEUDOS
            .iter()
            .find(|c| c.tipo == Tipo::Porao)
            .expect("um porão no catálogo");
        let planta = shared::planta::da(porao.id).expect("the Porão has a plan");
        let mut w = GameWorld::new(HashMap::new());
        w.zona = shared::arena::ZONA.to_string();
        w.ilha = Some(shared::terreno::Ilha::da_ilha(&shared::arena::DEF));
        let sid = SessionId(([127, 0, 0, 1], 19884).into());
        let (tx, _rx) = mpsc::unbounded_channel();
        w.on_connect(SessionHandle { id: sid, to_client: tx });
        let e = w.ecs.spawn((
            NetId(EntityId(951)),
            Position(shared::arena::CHEGADA),
            Velocity(Vec2::ZERO),
            EntityKind::Player,
            Health { current: 100, max: 100 },
        ));
        {
            let s = w.sessions.get_mut(&sid).unwrap();
            s.logged_in = true;
            s.entity = Some(e);
            s.xp = shared::xp_for_level(porao.nivel_min + 10);
            s.stats.hp_max = 100_000;
            s.stats.attack_damage = 100_000;
            s.stats.defense = 100_000;
            s.dungeon.porao_pendente = porao.id;
        }
        // What the login does on the Arena, then the next tick.
        w.poroes_a_comecar.push((sid, porao.id));
        w.dg_poroes_que_chegaram();
        let s = &w.sessions[&sid];
        assert!(s.instancia != 0, "landed with a Porão pending and no run started");
        assert_eq!(s.dungeon.porao_pendente, 0, "the pending run would start again");
        assert_eq!(s.porao_de_volta, Some(porao.id), "wouldn't know which door to leave by");
        let i = &w.instancias[0];
        assert!(i.planta.is_some(), "a Porão on the Arena runs on its plan");
        let onde = w.ecs.get::<&Position>(e).unwrap().0;
        assert!(
            onde.distance(planta.centro(planta.entrada())) <= planta.salas[planta.entrada()].raio,
            "started outside the entrance room: {onde:?}"
        );
        // The first room's mobs are inside the first room.
        let sala = planta.sala_da_etapa(0).unwrap();
        for m in &i.vivos {
            let p = w.ecs.get::<&Position>(*m).unwrap().0;
            assert!(
                p.distance(planta.centro(sala)) <= planta.salas[sala].raio,
                "a mob of step 0 spawned outside its room: {p:?}"
            );
        }
        let n = i.vivos.len();
        assert!(n >= 2, "the room needs a pack, not just its Warden");
        // Kill ONLY the Warden: the gate stays shut — every room needs a
        // full clear.
        let pack: Vec<Entity> = i.vivos.clone();
        w.ecs.get::<&mut EnemyTag>(pack[0]).unwrap().dead = true;
        w.dg_tick_instancia(0);
        assert_eq!(w.instancias[0].andar, 0, "the gate opened with the pack still alive");
        // The whole pack down: now it opens.
        for m in &pack {
            w.ecs.get::<&mut EnemyTag>(*m).unwrap().dead = true;
        }
        w.dg_tick_instancia(0);
        let i = &w.instancias[0];
        assert_eq!(i.andar, 1, "the room was cleared and the gate didn't open");
        assert!(i.restos.is_empty(), "nothing should be left behind in a cleared room");
        let sala = planta.sala_da_etapa(1).unwrap();
        for m in &i.vivos {
            let p = w.ecs.get::<&Position>(*m).unwrap().0;
            assert!(
                p.distance(planta.centro(sala)) <= planta.salas[sala].raio,
                "a mob of step 1 spawned outside its room"
            );
        }

        // THE WALL HOLDS, through the real tick: back in the entrance room,
        // walking into its far wall for five seconds goes nowhere past it.
        let _ = w.ecs.insert_one(e, shared::Solido);
        let entrada = planta.centro(planta.entrada());
        w.ecs.get::<&mut Position>(e).unwrap().0 = entrada;
        // The entrance is the dead end of the plan: its only corridor leads
        // to the first room, so walking straight AWAY from that is a wall.
        let saida = planta.centro(sala_da_primeira(planta));
        let contra = (entrada - saida).normalize();
        for k in 0..150u32 {
            w.sessions.get_mut(&sid).unwrap().pending_input = Some(shared::protocol::InputFrame {
                seq: k + 1,
                tick: k,
                move_dir: contra,
                aim: contra,
                buttons: 0,
            });
            w.step(1.0 / 30.0);
        }
        let onde = w.ecs.get::<&Position>(e).unwrap().0;
        let andar = w.instancias[0].andar;
        assert!(
            planta.livre(onde, ENTITY_RADIUS, andar),
            "walked out of the cellar: {onde:?}"
        );
        assert!(
            onde.distance(entrada) <= planta.salas[planta.entrada()].raio,
            "went through the entrance room's wall: {onde:?}"
        );
        assert!(
            onde.distance(entrada) > planta.salas[planta.entrada()].raio - 1.5,
            "didn't even walk to the wall ({onde:?}): the test measured nothing"
        );
    }

    /// A WHOLE PORÃO RUN, PLAYED, for every cellar: a player who does what
    /// the dungeon auto does — walk (by the server's own route) to the nearest
    /// living enemy it can see, or to the room of the step when it sees none,
    /// and kill what it reaches — must clear every room and the boss.
    ///
    /// The owner, 30/09/2026, with the auto on: "the problem remain, the main
    /// issue is to A* go throug where is a gate". Unit tests of the plan
    /// passed; this plays the real tick (route follower, physics, gates,
    /// spawns) end to end.
    #[test]
    fn uma_corrida_inteira_em_cada_porao() {
        crate::economy::init_vazia_para_testes();
        for porao in dg::CONTEUDOS.iter().filter(|c| c.tipo == Tipo::Porao) {
            let planta = shared::planta::da(porao.id).unwrap();
            let mut w = GameWorld::new(HashMap::new());
            w.zona = shared::arena::ZONA.to_string();
            w.ilha = Some(shared::terreno::Ilha::da_ilha(&shared::arena::DEF));
            let sid = SessionId(([127, 0, 0, 1], 19890).into());
            let (tx, _rx) = mpsc::unbounded_channel();
            w.on_connect(SessionHandle { id: sid, to_client: tx });
            let e = w.ecs.spawn((
                NetId(EntityId(960)),
                Position(shared::arena::CHEGADA),
                Velocity(Vec2::ZERO),
                EntityKind::Player,
                Health { current: 100_000, max: 100_000 },
                shared::Solido,
            ));
            {
                let s = w.sessions.get_mut(&sid).unwrap();
                s.logged_in = true;
                s.entity = Some(e);
                s.entity_id = EntityId(960);
                s.xp = shared::xp_for_level(porao.nivel_min + 10);
                s.stats.hp_max = 100_000;
                s.stats.attack_damage = 100_000;
                s.stats.defense = 100_000;
                // A geared player doesn't stagger at every hit; the test's
                // bare stats did, and stood stun-locked by ranged mobs.
                s.stats.poise_max = 100_000;
                s.poise_current = 100_000.0;
            }
            w.poroes_a_comecar.push((sid, porao.id));
            w.dg_poroes_que_chegaram();
            assert!(!w.instancias.is_empty(), "{}: no run", porao.nome);
            let dt = 1.0 / 30.0;
            let mut t = 0.0f32;
            let mut ultimo_pedido = -1.0f32;
            let mut venceu = false;
            let mut rastro = Vec::new();
            while t < 900.0 {
                let Some(i) = w.instancias.first() else { break };
                if matches!(i.estado, EstadoDg::Concluida { .. }) {
                    venceu = true;
                    break;
                }
                let andar = i.andar;
                let eu = w.ecs.get::<&Position>(e).unwrap().0;
                // What the client sees: enemies of this run within 24 u.
                let inst = i.id;
                let mut vistos: Vec<(Entity, Vec2)> = w
                    .ecs
                    .query::<(&Position, &EnemyTag, &Instancia)>()
                    .iter()
                    .filter(|(_, (_, t, ii))| ii.0 == inst && !t.dead)
                    .map(|(en, (p, _, _))| (en, p.0))
                    .filter(|(_, p)| p.distance(eu) <= shared::AOI_RADIUS)
                    .collect();
                vistos.sort_by(|a, b| a.1.distance(eu).total_cmp(&b.1.distance(eu)));
                // Kill what is within reach (the fight itself isn't what's
                // being tested).
                for (en, p) in &vistos {
                    // About a melee reach: the real player is hitting back
                    // (a player who only takes hits is staggered in place).
                    if p.distance(eu) <= 3.5 {
                        if let Ok(mut tg) = w.ecs.get::<&mut EnemyTag>(*en) {
                            tg.dead = true;
                        }
                    }
                }
                if t - ultimo_pedido >= 1.0 {
                    ultimo_pedido = t;
                    let alvo = vistos
                        .first()
                        .map(|v| v.1)
                        .or_else(|| {
                            planta
                                .sala_da_etapa(andar)
                                .map(|s| planta.centro(s))
                                .filter(|c| c.distance(eu) > 3.0)
                        });
                    if let Some(a) = alvo {
                        w.sessions.get_mut(&sid).unwrap().rota_pedida_em = -100.0;
                        w.handle_mover_para(sid, a);
                    }
                    rastro.push((t, andar, eu, vistos.len()));
                }
                // The client sends a frame every tick; with no stick input,
                // the server walks the route (`rota`).
                let seq = (t / dt) as u32 + 1;
                w.sessions.get_mut(&sid).unwrap().pending_input = Some(shared::protocol::InputFrame {
                    seq,
                    tick: seq,
                    move_dir: Vec2::ZERO,
                    aim: Vec2::X,
                    buttons: 0,
                });
                w.step(dt);
                t += dt;
            }
            if !venceu {
                for r in rastro.iter().rev().take(12) {
                    eprintln!("{}: t={:.0} andar={} eu={:?} vistos={}", porao.nome, r.0, r.1, r.2, r.3);
                }
                {
                    let eu = w.ecs.get::<&Position>(e).unwrap().0;
                    let s = &w.sessions[&sid];
                    let pts: Vec<Vec2> = s.rota.pontos().collect();
                    eprintln!("rota {:?} destino {:?} travado={}", pts, s.rota.destino(), s.rota.travado());
                    let ilha = w.ilha.as_ref().unwrap();
                    for (dx, dz) in [(1.0, 0.0), (-1.0, 0.0), (0.0, 1.0), (0.0, -1.0)] {
                        let v = Vec2::new(dx, dz) * 5.0;
                        let q = ilha.mover_com_degrau(eu, v, 1.0 / 30.0, ENTITY_RADIUS, shared::terreno::DEGRAU_BLOCOS);
                        eprintln!("terreno {dx},{dz}: {:?} -> {:?} (altura aqui {:.2}, la' {:.2})", eu, q, ilha.altura(eu.x, eu.y), ilha.altura(eu.x + dx * 0.6, eu.y + dz * 0.6));
                    }
                }
                let i = &w.instancias[0];
                let vivos: Vec<Vec2> = i
                    .vivos
                    .iter()
                    .filter(|m| w.dg_vivo(**m))
                    .filter_map(|m| w.ecs.get::<&Position>(*m).ok().map(|p| p.0))
                    .collect();
                panic!(
                    "{}: stuck at step {} of {} — living mobs at {vivos:?}",
                    porao.nome, i.andar, porao.andares
                );
            }
        }
    }

    fn sala_da_primeira(p: &shared::planta::Planta) -> usize {
        p.sala_da_etapa(0).unwrap()
    }

    #[test]
    fn tirar_item_so_tira_se_tiver_tudo() {
        let mut inv = vec![shared::InventorySlot::default(); 4];
        inv[1] = shared::InventorySlot {
            item_id: 358,
            qty: 1,
            instance: None,
        };
        assert!(!tirar_item(&mut inv, 358, 2));
        assert_eq!(inv[1].qty, 1);
        assert!(tirar_item(&mut inv, 358, 1));
        assert!(inv.iter().all(|s| s.qty == 0));
    }
}
