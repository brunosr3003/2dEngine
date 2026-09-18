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
const FECHA_DEPOIS_DE_VENCER_S: f32 = 75.0;
const FECHA_DEPOIS_DE_FALHAR_S: f32 = 8.0;
/// Distancia maxima pra abrir o bau com toque.
const ALCANCE_DO_BAU: f32 = 8.0;

pub struct MembroDg {
    pub sid: SessionId,
    pub nome: String,
    pub mortes: u32,
    pub ajudante: bool,
    pub saiu: bool,
    pub abriu_bau: bool,
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

fn tirar_item(inv: &mut [shared::InventorySlot], item: u16, mut qtd: u32) -> bool {
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

    fn dg_texto(&self, sid: SessionId, ok: bool, texto: impl Into<String>) {
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
            return Some("Personagem fora do jogo.".into());
        }
        if s.instancia != 0 {
            return Some("Você já está numa dungeon.".into());
        }
        if s.downed {
            return Some("Levante-se antes.".into());
        }
        let (nivel, poder) = self.dg_nivel_e_poder(sid);
        let tem_selo = tem_item(&s.inventory, shared::item_id::SELO_TEMPESTADE);
        dg::cadeado(c, estagio, nivel, poder, s.dungeon.liberado(c.id), tem_selo).map(|x| x.texto())
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
            cand.sort_by(|x, y| {
                y.distance_squared(porto)
                    .total_cmp(&x.distance_squared(porto))
                    .then(x.x.total_cmp(&y.x))
            });
            for p in cand {
                if sitios.iter().all(|s| s.distance(p) >= 70.0) {
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

    // ─────────────────────────────── pedidos ───────────────────────────────

    pub(super) fn handle_dungeon(&mut self, sid: SessionId, pedido: Pedido) {
        if !self.sessions.get(&sid).is_some_and(|s| s.logged_in) {
            return;
        }
        let agora = self.sim_time_s as f64;
        let k = chave(sid);
        match pedido {
            Pedido::Estado => {}
            Pedido::EntrarSolo { conteudo } => {
                let Some(c) = dg::conteudo(conteudo).filter(|c| c.tipo == Tipo::Porao) else {
                    return;
                };
                if let Some(motivo) = self.dg_recusa(sid, c, 1) {
                    self.dg_texto(sid, false, motivo);
                    return;
                }
                let ev = self.mesa.remover(k, agora);
                self.dg_eventos(ev);
                self.dg_comecar(conteudo, 1, vec![sid]);
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
                    self.dg_texto(sid, false, "A sala não existe mais.");
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
                self.dg_sair(sid);
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
                    texto: "Sem mais entradas à venda hoje.".into(),
                },
            });
            return;
        };
        if s.gold < preco {
            let _ = s.handle.to_client.send(ServerMessage::Dungeon {
                aviso: Aviso::Texto {
                    ok: false,
                    texto: format!("Faltam {} de ouro.", preco - s.gold),
                },
            });
            return;
        }
        s.gold -= preco;
        s.dungeon.gruta.comprar();
        crate::telemetria::conta("dungeon_entrada_comprada", "gruta", 1);
        crate::telemetria::conta("ouro_ralo", "dungeon_entrada", preco as i64);
        self.save_pending = true;
        self.dg_texto(sid, true, format!("Entrada comprada por {preco} de ouro."));
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
                                "Você saiu da partida.".to_string()
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
                            self.dg_texto(sid, false, "A sala ficou parada e fechou.");
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
        self.prox_instancia += 1;
        let id = self.prox_instancia;
        let uid = (now_ms() << 12) ^ id as u64;
        let hoje = dg::dia(unix_agora());
        let mut membros = Vec::new();
        for (i, sid) in validos.iter().enumerate() {
            let Some(s) = self.sessions.get_mut(sid) else {
                continue;
            };
            let entradas = s.dungeon.entradas(c.tipo);
            entradas.atualizar(c.tipo, hoje);
            let ajudante = !entradas.consumir();
            crate::telemetria::conta(
                "dungeon_entrada",
                format!(
                    "{conteudo}:{estagio}:{}",
                    if ajudante { "ajudante" } else { "normal" }
                ),
                1,
            );
            if dg::exige_selo(c, estagio) {
                crate::telemetria::conta("selo_usado", format!("{conteudo}:{estagio}"), 1);
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
            });
            let _ = self.ecs.insert_one(e, Instancia(id));
            let volta = Vec2::new((i as f32 * 1.3).cos(), (i as f32 * 1.3).sin()) * 3.0;
            self.dg_teleportar(*sid, arena[0] + volta);
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
        let centro = arena[(andar as usize).min(arena.len() - 1)];
        let nivel = dg::nivel_do_estagio(c, estagio);
        let vida = dg::vida_por_grupo(c, n) * mult_vida_teste();
        let dano = mult_dano_teste();
        let mut vivos = Vec::new();
        if andar >= c.andares {
            let (vida, dano) = {
                let (v, d) = dg::escala_do_chefe(c, n);
                (v * mult_vida_teste(), d * dano)
            };
            let pos = self.chao_livre(centro + Vec2::new(0.0, 14.0));
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
            for i in 0..dg::inimigos_do_andar(c, andar) {
                let ang = i as f32 * std::f32::consts::TAU / dg::inimigos_do_andar(c, andar) as f32;
                let pos = centro + Vec2::new(ang.cos(), ang.sin()) * (10.0 + (i % 3) as f32 * 4.0);
                let kind =
                    crate::economy::kind_para_nivel(nivel, uid ^ ((andar as u64) << 8) ^ i as u64);
                let semi = dg::tem_semi_chefe(c, andar) && i == 0;
                vivos.push(self.dg_nascer_mob(id, kind, pos, centro, nivel, vida, dano, semi));
            }
        }
        self.instancias[idx].vivos = vivos;
    }

    #[allow(clippy::too_many_arguments)]
    fn dg_nascer_mob(
        &mut self,
        inst: u32,
        kind: u16,
        pos: Vec2,
        ancora: Vec2,
        nivel: u32,
        vida: f32,
        dano: f32,
        semi: bool,
    ) -> Entity {
        let pos = self.chao_livre(pos);
        let (mut tag, _) = self.build_enemy_tag(kind, ancora, RAIO_DO_ANDAR * 0.7, pos);
        tag.nivel_da_faixa = nivel;
        tag.level = nivel;
        tag.detect_range = tag.detect_range.max(30.0);
        let (hp, d) = vida_e_dano_do_mob(tag.stats.hp_max, tag.stats.attack_damage, nivel);
        let (fv, fd) = if semi { (4.0, 1.5) } else { (1.0, 1.0) };
        let hp = ((hp as f32) * vida * fv).round().max(1.0) as i32;
        tag.stats.hp_max = hp;
        tag.stats.attack_damage = ((d as f32) * dano * fd).round().max(0.0) as i32;
        if semi {
            tag.boss_name = Some("Guardião da Gruta".into());
            tag.size_scale *= 1.4;
            tag.xp_reward *= 3;
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
            self.dg_texto(sid, true, "A dungeon foi reiniciada; você foi recuperado no porto.");
            self.save_pending = true;
            return;
        }
        if inst == 0 || !caido {
            return;
        }
        if falta > 0.0 {
            self.dg_texto(sid, false, format!("Reviver em {:.0} s.", falta.ceil()));
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
        let arena = self.dg_arena();
        self.dg_levantar(sid);
        self.dg_teleportar(sid, arena[(andar as usize).min(arena.len() - 1)]);
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
        let destino = retorno.unwrap_or_else(|| self.porto());
        self.dg_teleportar(sid, destino);
        self.dg_avisar(sid, Aviso::Saiu);
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
            self.dg_texto(sid, true, "A dungeon foi reiniciada; você saiu com segurança.");
            self.save_pending = true;
            return;
        }
        let Some(idx) = self.instancias.iter().position(|i| i.id == inst) else {
            self.dg_devolver(sid);
            return;
        };
        if matches!(self.instancias[idx].estado, EstadoDg::Concluida { .. }) {
            self.dg_dar_bau(idx, sid);
        }
        if let Some(m) = self.instancias[idx]
            .membros
            .iter_mut()
            .find(|m| m.sid == sid)
        {
            m.saiu = true;
        }
        self.dg_devolver(sid);
        self.dg_texto(sid, true, "Você saiu da dungeon.");
    }

    pub(super) fn tick_dungeons(&mut self) {
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
                // Longe demais do andar: volta pro centro dele.
                for sid in &presentes {
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
                        self.dg_texto(*sid, false, "O grupo caiu: o andar recomeçou.");
                    }
                    self.instancias[idx].aviso_em = 0.0;
                } else if !todos_caidos {
                    let limpo = self.instancias[idx].vivos.iter().all(|e| !self.dg_vivo(*e));
                    if limpo && !self.instancias[idx].vivos.is_empty() {
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
                nome: format!("Baú · {}", c.nome),
                rumo: shared::npc_kind(None, dg::PAPEL_BAU),
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
                }
                if de_todas && !ajudante {
                    let p = dg::peca_garantida(c, estagio, &mut rng);
                    let inst = GameWorld::dg_rolar_peca(&p);
                    s.dungeon.postar(p.item_id, 1, inst, 2, quando);
                    let nivel = dg::nivel_do_estagio(c, estagio);
                    let cor = shared::chaves::faixa(nivel).cor;
                    let base = shared::item_id::CHAVES[fastrand::usize(..4)];
                    s.dungeon
                        .postar(shared::item_id::chave_na_cor(base, cor), 1, None, 2, quando);
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
                },
            );
            if primeira {
                self.dg_texto(
                    sid,
                    true,
                    "Primeira vitória! A recompensa está nas Entregas.",
                );
            }
        }
        self.save_pending = true;
    }

    fn dg_rolar_peca(p: &dg::Premio) -> Option<shared::ItemInstance> {
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
            self.dg_texto(sid, false, "Chegue perto do baú.");
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
        let bau = dg::rolar_bau(c, estagio, ajudante, bonus, &mut rng);
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
        crate::telemetria::conta("dungeon_bau", format!("{conteudo}:{estagio}"), 1);
        for (item, qtd, inst) in premios {
            crate::telemetria::conta("dungeon_bau_item", item, qtd as i64);
            if shared::item_id::todas_as_chaves().contains(&item) {
                crate::telemetria::conta("chave_drop", format!("dungeon:{item}"), qtd as i64);
            }
            if add_to_inventory(&mut s.inventory, item, qtd, inst) {
                itens.push((item, qtd));
            } else {
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
        self.dg_avisar(
            sid,
            Aviso::Bau {
                itens,
                marcas: bau.marcas,
                no_correio,
            },
        );
    }

    fn dg_fechar(&mut self, idx: usize) {
        let inst = self.instancias.remove(idx);
        let mut sobra = inst.vivos;
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
                    texto: "Bolsa cheia.".into(),
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

#[cfg(test)]
mod testes {
    use super::*;

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
