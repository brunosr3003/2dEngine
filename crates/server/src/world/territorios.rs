//! TERRITORIES, phase 3 (docs/TERRITORIOS.md): who holds which territory
//! islet, the claim, and the list every client at sea keeps.
//!
//! The database (`territorios`, `social::reivindicar`) is the truth; the sea
//! process keeps a copy (`GameWorld::territorios`), reloaded every 30 s and
//! after each claim, so a clan dissolved elsewhere frees its islet here too.

use super::*;
use shared::territorio::{Dono, TomadaInfo, RAIO_DO_NUCLEO, TEMPO_DE_TOMADA_S};

/// A territory structure (`shared::territorio::KIND_*`): which islet, what,
/// which one (wall segment index, 0 for the gate, cannon plot).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EstruturaTag {
    pub ilhota: u8,
    pub tipo: u16,
    pub indice: u8,
}

/// An islet's DEFENDER (`shared::territorio`, phase 4.4).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DefensorTag {
    pub ilhota: u8,
    pub indice: u8,
}

/// An islet cannon's ball in flight at a ship (whose captain is `alvo`).
#[derive(Debug, Clone, Copy)]
pub struct BalaDaIlha {
    pub alvo: SessionId,
    pub mira: Vec2,
    pub dano: i32,
    pub chega_em: f32,
}

/// One defender slot of an islet: who stands there (and at what level), or
/// when the next one comes.
#[derive(Debug, Clone, Default)]
pub struct VagaDeDefensor {
    pub corpo: Option<(hecs::Entity, u32)>,
    pub volta_em: f32,
}

/// Seconds before a fallen defender is replaced.
const DEFENSOR_VOLTA_S: f32 = 60.0;

/// A held islet's structure entities.
#[derive(Debug, Default)]
pub struct Estruturas {
    pub muros: [Option<hecs::Entity>; shared::territorio::SEGMENTOS_DA_MURALHA],
    pub portao: Option<hecs::Entity>,
    pub canhoes: Vec<hecs::Entity>,
    /// The levels they were built at (to re-scale HP on an upgrade).
    pub niveis: (u8, u8, u8),
    /// The upkeep weeks behind they were built with (half HP when > 0).
    pub atrasos: u8,
}

/// A claim under way: who claims, for which clan, how far along.
#[derive(Debug, Clone)]
pub struct Tomada {
    pub ilhota: u8,
    pub cla_id: i64,
    pub tag: String,
    pub cla: String,
    /// Who started it (the database re-checks their rank at the end).
    pub quem: SessionId,
    pub nome: String,
    pub feito_s: f32,
    pub contestada: bool,
}

/// Ticks between reloads of the held islets.
const RECARREGA_A_CADA_S: u32 = 30;

impl GameWorld {
    /// Each tick at sea: reload now and then; send the list to whoever does
    /// not have it yet.
    pub(crate) fn tick_territorios(&mut self) {
        if self.tick % (shared::TICK_RATE_HZ as u32 * RECARREGA_A_CADA_S) == 1 {
            self.carregar_territorios();
        }
        let bandeiras = self.lista_de_bandeiras();
        for s in self.sessions.values_mut() {
            if s.logged_in && !s.bandeiras_enviadas && !bandeiras.is_empty() {
                s.bandeiras_enviadas = true;
                let _ = s.handle.to_client.send(ServerMessage::Bandeiras {
                    lista: bandeiras.clone(),
                });
            }
        }
        self.manter_npcs_das_ilhotas();
        // A closed siege forgets who plundered (the next one starts clean).
        let cercos = &self.cercos_abertos;
        self.saques_do_cerco.retain(|(k, _)| cercos.contains(k));
        self.manter_estruturas();
        self.manter_defensores();
        self.tick_canhoes_das_ilhas();
        self.tick_guerras();
        let lista = self.lista_de_territorios();
        for s in self.sessions.values_mut() {
            if s.logged_in && !s.territorios_enviados {
                s.territorios_enviados = true;
                let _ = s.handle.to_client.send(ServerMessage::Territorios {
                    lista: lista.clone(),
                });
            }
        }
    }

    fn lista_de_bandeiras(&self) -> Vec<shared::bandeira::BandeiraDoCla> {
        let mut v: Vec<_> = self.bandeiras.values().cloned().collect();
        v.sort_by_key(|b| b.cla_id);
        v
    }

    /// CLAN FLAGS: the database's list arrived; when it changed, everyone at
    /// sea gets it again.
    pub fn on_bandeiras(&mut self, v: Vec<shared::bandeira::BandeiraDoCla>) {
        let novo: HashMap<i64, _> = v.into_iter().map(|b| (b.cla_id, b)).collect();
        if novo != self.bandeiras {
            self.bandeiras = novo;
            let lista = self.lista_de_bandeiras();
            for s in self.sessions.values_mut().filter(|s| s.logged_in) {
                s.bandeiras_enviadas = true;
                let _ = s.handle.to_client.send(ServerMessage::Bandeiras {
                    lista: lista.clone(),
                });
            }
        }
    }

    fn lista_de_territorios(&self) -> Vec<Dono> {
        let mut v: Vec<Dono> = self.territorios.values().cloned().collect();
        v.sort_by_key(|d| d.ilhota);
        v
    }

    /// Reads the held islets from the database (no database in tests: the
    /// in-memory copy stays).
    pub(crate) fn carregar_territorios(&mut self) {
        let Some(ctx) = self.auth_ctx.clone() else {
            return;
        };
        let agora = agora_unix();
        tokio::spawn(async move {
            // The weekly upkeep falls due here (Monday 00:00, Brasília).
            match crate::social::cobrar_manutencao(&ctx.pool, agora).await {
                Ok(v) if !v.is_empty() => tracing::info!("manutencao: ilhotas liberadas por falta de pagamento: {v:?}"),
                Ok(_) => {}
                Err(e) => tracing::warn!("manutencao: falha ao cobrar: {e:#}"),
            }
            match crate::social::territorios(&ctx.pool).await {
                Ok(v) => {
                    let _ = ctx.tx.send(IncomingMessage::Territorios(v));
                }
                Err(e) => tracing::warn!("territorios: falha ao carregar: {e:#}"),
            }
            match crate::social::guerras(&ctx.pool).await {
                Ok(v) => {
                    let _ = ctx.tx.send(IncomingMessage::Guerras(v));
                }
                Err(e) => tracing::warn!("guerras: falha ao carregar: {e:#}"),
            }
            match crate::social::bandeiras(&ctx.pool).await {
                Ok(v) => {
                    let _ = ctx.tx.send(IncomingMessage::Bandeiras(v));
                }
                Err(e) => tracing::warn!("bandeiras: falha ao carregar: {e:#}"),
            }
        });
    }

    /// The database's list arrived: replace the copy and, when it changed,
    /// tell everyone at sea.
    pub fn on_territorios(&mut self, v: Vec<Dono>) {
        let novo: HashMap<u8, Dono> = v.into_iter().map(|d| (d.ilhota, d)).collect();
        if novo != self.territorios {
            self.territorios = novo;
            self.avisar_territorios();
        }
    }

    fn avisar_territorios(&mut self) {
        let lista = self.lista_de_territorios();
        for s in self.sessions.values_mut().filter(|s| s.logged_in) {
            s.territorios_enviados = true;
            let _ = s.handle.to_client.send(ServerMessage::Territorios {
                lista: lista.clone(),
            });
        }
    }

    /// Why `sid` cannot claim the islet they stand on, or the islet index.
    /// The rules the sea knows; the clan rank is the database's.
    pub(crate) fn pode_reivindicar(&self, sid: SessionId) -> Result<u8, &'static str> {
        let s = self.sessions.get(&sid).ok_or("")?;
        let t = s.em_terra.ok_or("Go ashore on a territory islet first.")?;
        let ilha = shared::mar::ilhotas()
            .get(t.ilhota as usize)
            .ok_or("Go ashore on a territory islet first.")?;
        let corpo = s
            .entity
            .and_then(|e| self.ecs.get::<&Position>(e).ok().map(|p| p.0))
            .ok_or("")?;
        if corpo.distance(ilha.centro) > RAIO_DO_NUCLEO {
            return Err("Walk to the heart of the islet to claim it.");
        }
        if s.cla_id == 0 {
            return Err("Only a clan can claim a territory islet.");
        }
        if let Some(dono) = self.territorios.get(&t.ilhota) {
            // Held: only the declared ATTACKER, in its war's open window.
            let em_guerra = self.guerra_aberta(t.ilhota).is_some_and(|g| g.atacante_id == s.cla_id);
            if dono.cla_id == s.cla_id || !em_guerra {
                return Err("This islet already belongs to a clan.");
            }
            return Ok(t.ilhota);
        }
        if self.territorios.values().any(|d| d.cla_id == s.cla_id) {
            return Err("Your clan already holds a territory islet.");
        }
        Ok(t.ilhota)
    }

    /// CLAIM the free islet I stand on, at its heart (`PedidoNavio::
    /// ReivindicarIlhota`): the sea's checks, then the database's (rank,
    /// one per clan, still free) in a task; on a yes the two minutes start
    /// (`on_tomada_autorizada`).
    pub(crate) fn reivindicar_ilhota(&mut self, sid: SessionId) {
        let ilhota = match self.pode_reivindicar(sid) {
            Ok(i) => i,
            Err(motivo) => {
                if !motivo.is_empty() {
                    self.avisa_missao(sid, motivo.into());
                }
                return;
            }
        };
        if self.tomadas.contains_key(&ilhota) {
            self.avisa_missao(sid, "This islet is already being claimed.".into());
            return;
        }
        // In a war the database checks the rank at the conquest itself.
        if let Some(g) = self.guerra_aberta(ilhota).cloned() {
            self.on_tomada_autorizada(sid, ilhota, Ok(g.atacante));
            return;
        }
        let nome = self.nome_de(sid);
        let Some(ctx) = self.auth_ctx.clone() else {
            // No database (tests): the sea's checks are the whole answer.
            let cla = self.sessions.get(&sid).map_or(String::new(), |s| s.cla_tag.clone());
            self.on_tomada_autorizada(sid, ilhota, Ok(cla));
            return;
        };
        tokio::spawn(async move {
            let resultado = crate::social::pode_reivindicar(&ctx.pool, &nome, ilhota)
                .await
                .map_err(|e| e.to_string());
            let _ = ctx.tx.send(IncomingMessage::TomadaAutorizada {
                sid,
                ilhota,
                resultado,
            });
        });
    }

    /// The database said whether the claim may start: the two minutes begin.
    pub fn on_tomada_autorizada(&mut self, sid: SessionId, ilhota: u8, resultado: Result<String, String>) {
        let cla_nome = match resultado {
            Ok(n) => n,
            Err(e) => {
                self.avisa_missao(sid, e);
                return;
            }
        };
        // Still true now (the answer took a moment)?
        if self.pode_reivindicar(sid) != Ok(ilhota) || self.tomadas.contains_key(&ilhota) {
            return;
        }
        let Some(s) = self.sessions.get(&sid) else {
            return;
        };
        let tomada = Tomada {
            ilhota,
            cla_id: s.cla_id,
            tag: s.cla_tag.clone(),
            cla: cla_nome,
            quem: sid,
            nome: s.name.clone(),
            feito_s: 0.0,
            contestada: false,
        };
        let ilha = shared::mar::ilhotas()[ilhota as usize].nome;
        tracing::info!("territorio: {} comecou a tomar {ilha}", tomada.nome);
        self.tomadas.insert(ilhota, tomada);
        self.send_chat_to(
            sid,
            &format!("Claiming {ilha}: hold the ring for 2 minutes with only your clan inside."),
        );
        self.avisar_tomadas();
    }

    /// Each tick: every claim's clock. Clan members inside and no one else
    /// = it runs; an outsider inside = paused; no clan member = cancelled.
    pub(crate) fn tick_tomadas(&mut self, dt: f32) {
        if self.tomadas.is_empty() {
            return;
        }
        let corpos: Vec<(SessionId, i64, Vec2)> = self
            .sessions
            .iter()
            .filter(|(_, s)| s.logged_in)
            .filter_map(|(id, s)| {
                let p = self.ecs.get::<&Position>(s.entity?).ok()?.0;
                Some((*id, s.cla_id, p))
            })
            .collect();
        let mut canceladas = Vec::new();
        let mut prontas = Vec::new();
        let mut mudou = false;
        for (ilhota, t) in self.tomadas.iter_mut() {
            let centro = shared::mar::ilhotas()[*ilhota as usize].centro;
            let dentro = corpos
                .iter()
                .filter(|(_, _, p)| p.distance(centro) <= RAIO_DO_NUCLEO);
            let (mut do_cla, mut de_fora) = (0, 0);
            for (_, cla, _) in dentro {
                if *cla == t.cla_id && *cla != 0 {
                    do_cla += 1;
                } else {
                    de_fora += 1;
                }
            }
            if do_cla == 0 {
                canceladas.push(*ilhota);
                continue;
            }
            let contestada = de_fora > 0;
            if contestada != t.contestada {
                t.contestada = contestada;
                mudou = true;
            }
            if !contestada {
                let antes = t.feito_s as u32;
                t.feito_s += dt;
                mudou |= t.feito_s as u32 != antes;
                if t.feito_s >= TEMPO_DE_TOMADA_S {
                    prontas.push(*ilhota);
                }
            }
        }
        for ilhota in canceladas {
            if let Some(t) = self.tomadas.remove(&ilhota) {
                let ilha = shared::mar::ilhotas()[ilhota as usize].nome;
                tracing::info!("territorio: tomada de {ilha} por {} cancelada", t.nome);
                self.avisar_cla(t.cla_id, &format!("The claim on {ilha} was lost: no clan member was left in the ring."));
                mudou = true;
            }
        }
        for ilhota in prontas {
            if let Some(t) = self.tomadas.remove(&ilhota) {
                self.concluir_tomada(t);
                mudou = true;
            }
        }
        if mudou {
            self.avisar_tomadas();
        }
    }

    /// Two minutes held: write it (the database re-checks rank and freedom).
    fn concluir_tomada(&mut self, t: Tomada) {
        // A held islet taken in its war: CONQUEST.
        if self.territorios.contains_key(&t.ilhota) {
            let Some(g) = self.guerra_aberta(t.ilhota).cloned() else {
                return;
            };
            let escudo = shared::territorio::escudo_da_conquista(agora_unix());
            let Some(ctx) = self.auth_ctx.clone() else {
                // No database (tests).
                let mut d = self.territorios[&t.ilhota].clone();
                d.cla_id = t.cla_id;
                d.tag = t.tag;
                d.cla = t.cla;
                d.escudo_ate = escudo;
                self.on_reivindicacao(t.quem, Ok(d));
                return;
            };
            let (sid, nome) = (t.quem, t.nome);
            tokio::spawn(async move {
                let resultado = crate::social::conquistar(&ctx.pool, &nome, g.id, escudo)
                    .await
                    .map_err(|e| e.to_string());
                let _ = ctx.tx.send(IncomingMessage::Reivindicacao { sid, resultado });
            });
            return;
        }
        let Some(ctx) = self.auth_ctx.clone() else {
            // No database (tests).
            self.on_reivindicacao(
                t.quem,
                Ok(Dono {
                    ilhota: t.ilhota,
                    cla_id: t.cla_id,
                    tag: t.tag,
                    cla: t.cla,
                    niveis: shared::territorio::NIVEIS_INICIAIS,
                    ..Default::default()
                }),
            );
            return;
        };
        let (sid, nome, ilhota) = (t.quem, t.nome, t.ilhota);
        tokio::spawn(async move {
            let resultado = crate::social::reivindicar(&ctx.pool, &nome, ilhota)
                .await
                .map_err(|e| e.to_string());
            let _ = ctx.tx.send(IncomingMessage::Reivindicacao { sid, resultado });
        });
    }

    /// A line to every clan member at sea.
    fn avisar_cla(&mut self, cla_id: i64, texto: &str) {
        let alvos: Vec<SessionId> = self
            .sessions
            .iter()
            .filter(|(_, s)| s.logged_in && s.cla_id == cla_id && cla_id != 0)
            .map(|(id, _)| *id)
            .collect();
        for sid in alvos {
            self.send_chat_to(sid, texto);
        }
    }

    /// The claims under way, to everyone at sea.
    pub(crate) fn avisar_tomadas(&mut self) {
        let mut lista: Vec<TomadaInfo> = self
            .tomadas
            .values()
            .map(|t| TomadaInfo {
                ilhota: t.ilhota,
                tag: t.tag.clone(),
                cla: t.cla.clone(),
                feito_s: t.feito_s,
                contestada: t.contestada,
            })
            .collect();
        lista.sort_by_key(|t| t.ilhota);
        for s in self.sessions.values().filter(|s| s.logged_in) {
            let _ = s.handle.to_client.send(ServerMessage::Tomadas {
                lista: lista.clone(),
            });
        }
    }

    /// A claim's outcome came back from the database.
    pub fn on_reivindicacao(&mut self, sid: SessionId, resultado: Result<Dono, String>) {
        match resultado {
            Ok(d) => {
                let ilha = shared::mar::ilhotas()
                    .get(d.ilhota as usize)
                    .map_or("the islet", |i| i.nome);
                tracing::info!("territorio: {} reivindicou {ilha} para {}", self.nome_de(sid), d.cla);
                let texto = format!("{ilha} now belongs to {}.", d.cla);
                self.send_chat_to(sid, &texto);
                let cla = d.cla_id;
                self.avisar_cla(cla, &texto);
                let k = d.ilhota;
                self.territorios.insert(k, d);
                // A conquest ends that islet's war.
                if let Some(g) = self.guerra_aberta(k).cloned() {
                    self.fechar_guerra(g.id, k, false);
                }
                self.avisar_territorios();
            }
            Err(e) => self.avisa_missao(sid, e),
        }
    }

    // ─────────────────── phase 4: Governor, Farmer, upgrades ───────────────────

    /// Every held islet has its Governor and Farmer at the heart; a freed
    /// islet loses them.
    fn manter_npcs_das_ilhotas(&mut self) {
        use shared::construcao::Papel;
        let fora: Vec<u8> = self
            .npcs_das_ilhotas
            .keys()
            .copied()
            .filter(|k| !self.territorios.contains_key(k))
            .collect();
        for k in fora {
            if let Some(es) = self.npcs_das_ilhotas.remove(&k) {
                for e in es {
                    if let Ok(n) = self.ecs.get::<&NetId>(e).map(|n| n.0) {
                        self.removed_this_tick.push(n);
                    }
                    let _ = self.ecs.despawn(e);
                }
            }
        }
        let novas: Vec<u8> = self
            .territorios
            .keys()
            .copied()
            .filter(|k| !self.npcs_das_ilhotas.contains_key(k))
            .collect();
        for k in novas {
            let Some(ilha) = shared::mar::ilhotas().get(k as usize) else {
                continue;
            };
            let mut es = Vec::new();
            for (papel, pos) in [
                (Papel::Governador, shared::territorio::pos_do_governador(ilha.centro)),
                (Papel::Fazendeiro, shared::territorio::pos_do_fazendeiro(ilha.centro)),
            ] {
                let eid = self.alloc_entity_id();
                // Facing out from the heart.
                let olha = (pos - ilha.centro).normalize_or_zero();
                let yaw = olha.x.atan2(olha.y);
                es.push(self.ecs.spawn((
                    NetId(eid),
                    Position(pos),
                    EntityKind::Npc(9),
                    NpcDaVilaTag {
                        nome: papel.nome().into(),
                        rumo: shared::npc_kind(Some(yaw), papel as u8),
                        giver: None,
                    },
                )));
            }
            self.npcs_das_ilhotas.insert(k, [es[0], es[1]]);
        }
    }

    /// RAISE a track of my clan's islet (`PedidoNavio::MelhorarIlha`):
    /// ashore, by the right NPC (Governor for the village and defences,
    /// Farmer for the resources), under the cap, paying materials and gold
    /// from my bag; the database writes it (and a refusal refunds).
    pub(crate) fn melhorar_ilha(&mut self, sid: SessionId, trilha: u8) {
        use shared::territorio as tr;
        let t = trilha as usize;
        if t >= tr::N_TRILHAS {
            return;
        }
        let Some(s) = self.sessions.get(&sid) else {
            return;
        };
        let Some(k) = s.em_terra.map(|e| e.ilhota) else {
            return;
        };
        let Some(dono) = self.territorios.get(&k).cloned() else {
            self.avisa_missao(sid, "This islet belongs to no clan.".into());
            return;
        };
        if dono.cla_id != s.cla_id || s.cla_id == 0 {
            self.avisa_missao(sid, "Only the clan that holds this islet can upgrade it.".into());
            return;
        }
        let Some(ilha) = shared::mar::ilhotas().get(k as usize) else {
            return;
        };
        let npc = if tr::do_governador(t) {
            tr::pos_do_governador(ilha.centro)
        } else {
            tr::pos_do_fazendeiro(ilha.centro)
        };
        let perto = s
            .entity
            .and_then(|e| self.ecs.get::<&Position>(e).ok().map(|p| p.0))
            .is_some_and(|p| p.distance(npc) <= tr::ALCANCE_DO_NPC + 1.0);
        if !perto {
            return;
        }
        let atual = dono.niveis[t];
        let para = atual + 1;
        if para > tr::teto(t, ilha.porte, dono.niveis[tr::trilha::VILA]) {
            self.avisa_missao(sid, "That is as high as it goes for now.".into());
            return;
        }
        let (mats, ouro) = tr::custo(t, para);
        let falta = s.gold < ouro
            || mats
                .iter()
                .any(|(item, q)| crate::craft::tem(&s.inventory, *item) < *q);
        if falta {
            self.avisa_missao(sid, "Not enough materials or gold for this upgrade.".into());
            return;
        }
        let s = self.sessions.get_mut(&sid).unwrap();
        s.gold -= ouro;
        for (item, q) in &mats {
            crate::craft::consumir(&mut s.inventory, *item, *q);
        }
        s.inventory_dirty = true;
        self.save_pending = true;
        let nome = self.nome_de(sid);
        let pago = (mats, ouro);
        let Some(ctx) = self.auth_ctx.clone() else {
            // No database (tests).
            self.on_ilha_melhorada(sid, k, t, para, pago, Ok(()));
            return;
        };
        tokio::spawn(async move {
            let resultado = crate::social::melhorar_ilha(&ctx.pool, &nome, k, t, para)
                .await
                .map_err(|e| e.to_string());
            let _ = ctx.tx.send(IncomingMessage::IlhaMelhorada {
                sid,
                ilhota: k,
                trilha: t,
                para,
                pago,
                resultado,
            });
        });
    }

    /// The database's answer to an upgrade: the new level for everyone at
    /// sea, or the payment back.
    pub fn on_ilha_melhorada(
        &mut self,
        sid: SessionId,
        ilhota: u8,
        trilha: usize,
        para: u8,
        pago: (Vec<(u16, u32)>, u64),
        resultado: Result<(), String>,
    ) {
        use shared::territorio as tr;
        match resultado {
            Ok(()) => {
                if let Some(d) = self.territorios.get_mut(&ilhota) {
                    d.niveis[trilha] = para;
                }
                let nome = tr::NOMES_DAS_TRILHAS[trilha];
                let texto = if trilha == tr::trilha::VILA {
                    format!(
                        "The islet grew into a {}.",
                        tr::NOMES_DA_VILA[(para as usize).clamp(1, 4) - 1]
                    )
                } else {
                    format!("{nome} upgraded to level {para}.")
                };
                self.send_chat_to(sid, &texto);
                self.avisar_territorios();
            }
            Err(e) => {
                if let Some(s) = self.sessions.get_mut(&sid) {
                    s.gold += pago.1;
                    for (item, q) in pago.0 {
                        crate::world::add_to_inventory(&mut s.inventory, item, q, None);
                    }
                    s.inventory_dirty = true;
                }
                self.save_pending = true;
                self.avisa_missao(sid, e);
            }
        }
    }

    // ─────────────────── phase 4.3: walls, gate, cannons ───────────────────

    fn por_estrutura(&mut self, pos: Vec2, tag: EstruturaTag, hp: i32) -> hecs::Entity {
        let eid = self.alloc_entity_id();
        self.ecs.spawn((
            NetId(eid),
            Position(pos),
            Velocity(Vec2::ZERO),
            EntityKind::Enemy(tag.tipo),
            Health { current: hp, max: hp.max(1) },
            tag,
        ))
    }

    fn tirar(&mut self, e: hecs::Entity) {
        if let Ok(n) = self.ecs.get::<&NetId>(e).map(|n| n.0) {
            self.removed_this_tick.push(n);
        }
        let _ = self.ecs.despawn(e);
    }

    /// Every held islet's walls, gate and cannons match its levels: built
    /// when a track reaches 1, their HP max following the level (an upgrade
    /// raises current HP by the same amount), gone when the islet is freed.
    pub(crate) fn manter_estruturas(&mut self) {
        use shared::territorio as tr;
        let livres: Vec<u8> = self
            .estruturas
            .keys()
            .copied()
            .filter(|k| !self.territorios.contains_key(k))
            .collect();
        for k in livres {
            if let Some(es) = self.estruturas.remove(&k) {
                for e in es.muros.into_iter().flatten().chain(es.portao).chain(es.canhoes) {
                    self.tirar(e);
                }
            }
        }
        let donos: Vec<(u8, [u8; tr::N_TRILHAS], u8)> =
            self.territorios.iter().map(|(k, d)| (*k, d.niveis, d.atrasos)).collect();
        for (k, niveis, atrasos) in donos {
            let Some(ilha) = shared::mar::ilhotas().get(k as usize) else {
                continue;
            };
            // Upkeep a week behind: the defences stand at half strength.
            let metade = |hp: i32| if atrasos > 0 { (hp / 2).max(1) } else { hp };
            let (nm, np, nc) = (
                niveis[tr::trilha::MUROS],
                niveis[tr::trilha::PORTOES],
                niveis[tr::trilha::CANHOES],
            );
            let atual = self.estruturas.get(&k).map(|e| (e.niveis, e.atrasos));
            if atual == Some(((nm, np, nc), atrasos)) {
                continue;
            }
            let mut es = self.estruturas.remove(&k).unwrap_or_default();
            let portao_seg = tr::segmento_do_portao(ilha.fase);
            // Walls: every segment but the gate's.
            let hp_m = metade(tr::hp_do_muro(nm));
            for i in 0..tr::SEGMENTOS_DA_MURALHA {
                if i == portao_seg {
                    continue;
                }
                match (es.muros[i], nm) {
                    (Some(e), 0) => {
                        self.tirar(e);
                        es.muros[i] = None;
                    }
                    (Some(e), _) => self.reescalar(e, hp_m),
                    (None, 0) => {}
                    (None, _) => {
                        let tag = EstruturaTag { ilhota: k, tipo: tr::KIND_MURO, indice: i as u8 };
                        es.muros[i] = Some(self.por_estrutura(tr::pos_do_segmento(ilha.centro, ilha.porte, i), tag, hp_m));
                    }
                }
            }
            // The gate (needs walls round it).
            let hp_p = metade(tr::hp_do_portao(np));
            match (es.portao, np > 0 && nm > 0) {
                (Some(e), false) => {
                    self.tirar(e);
                    es.portao = None;
                }
                (Some(e), true) => self.reescalar(e, hp_p),
                (None, true) => {
                    let tag = EstruturaTag { ilhota: k, tipo: tr::KIND_PORTAO, indice: portao_seg as u8 };
                    es.portao = Some(self.por_estrutura(tr::pos_do_segmento(ilha.centro, ilha.porte, portao_seg), tag, hp_p));
                }
                (None, false) => {}
            }
            // Cannons on the coast plots.
            let hp_c = metade(tr::hp_do_canhao(nc));
            if nc == 0 {
                for e in std::mem::take(&mut es.canhoes) {
                    self.tirar(e);
                }
            } else if es.canhoes.is_empty() {
                for (n, pos) in tr::pos_dos_canhoes(ilha.centro, ilha.raio, ilha.porte, ilha.fase)
                    .into_iter()
                    .enumerate()
                {
                    let tag = EstruturaTag { ilhota: k, tipo: tr::KIND_CANHAO, indice: n as u8 };
                    let e = self.por_estrutura(pos, tag, hp_c);
                    es.canhoes.push(e);
                }
            } else {
                for e in es.canhoes.clone() {
                    self.reescalar(e, hp_c);
                }
            }
            es.niveis = (nm, np, nc);
            es.atrasos = atrasos;
            self.estruturas.insert(k, es);
        }
    }

    /// A new max HP: current rises (or falls) by the same amount.
    fn reescalar(&mut self, e: hecs::Entity, max: i32) {
        if let Ok(mut h) = self.ecs.get::<&mut Health>(e) {
            let ganho = max - h.max;
            h.max = max.max(1);
            h.current = (h.current + ganho).clamp(0, h.max);
        }
    }

    /// May the body `atacante` damage structure `t`? Only in its islet's
    /// siege window, and only someone outside the clan that holds it.
    pub(crate) fn pode_ferir_estrutura(&self, atacante: EntityId, t: EstruturaTag) -> bool {
        if !self.cercos_abertos.contains(&t.ilhota) {
            return false;
        }
        let Some(dono) = self.territorios.get(&t.ilhota) else {
            return false;
        };
        self.sessions
            .values()
            .find(|s| s.entity_id == atacante)
            .is_some_and(|s| s.cla_id != dono.cla_id)
    }

    /// Does the wall stop a step from `de` to `para` on islet `ilhota` for a
    /// body of clan `cla`? A standing segment stops everyone; the gate lets
    /// the holding clan through; a breach (0 HP) or a broken gate lets anyone.
    pub(crate) fn muralha_barra(&self, ilhota: u8, cla: i64, de: Vec2, para: Vec2) -> bool {
        use shared::territorio as tr;
        let (Some(es), Some(ilha)) = (self.estruturas.get(&ilhota), shared::mar::ilhotas().get(ilhota as usize)) else {
            return false;
        };
        let Some(seg) = tr::cruza_a_muralha(ilha.centro, ilha.porte, de, para) else {
            return false;
        };
        let de_pe = |e: Option<hecs::Entity>| {
            e.and_then(|e| self.ecs.get::<&Health>(e).ok().map(|h| h.current > 0))
                .unwrap_or(false)
        };
        if seg == tr::segmento_do_portao(ilha.fase) {
            let do_dono = self.territorios.get(&ilhota).is_some_and(|d| d.cla_id == cla && cla != 0);
            return de_pe(es.portao) && !do_dono;
        }
        de_pe(es.muros[seg])
    }

    // ─────────────────── phase 4.4: the defenders ───────────────────

    /// One defender of `papel` at `pos`, level `nivel`: a land fighter
    /// tethered to its post (it walks back home), no XP.
    fn por_defensor(&mut self, k: u8, i: usize, pos: Vec2, papel: u8, nivel: u32) -> hecs::Entity {
        use shared::territorio::papel_defensor as pd;
        // Warriors wear the Triton Guard forced to melee; archers and mages
        // are the Archer and the Mage (`docs/TERRITORIOS.md`).
        let kind = match papel {
            pd::ARQUEIRO => 6,
            pd::MAGO => 4,
            _ => 71,
        };
        let (mut tag, mut health) = self.build_enemy_tag(kind, pos, 14.0, pos);
        tag.level = nivel;
        tag.nivel_da_faixa = nivel;
        tag.xp_reward = 0;
        tag.detect_range = deteccao_do_nivel(tag.detect_range, nivel);
        let (hp, dano) = vida_e_dano_do_mob(health.max, tag.stats.attack_damage, nivel);
        tag.stats.hp_max = hp;
        tag.stats.attack_damage = dano;
        tag.stats.defense = defesa_do_mob(tag.stats.defense, nivel);
        health = Health { current: hp, max: hp };
        if papel == pd::GUERREIRO {
            tag.kite_dist = None;
            tag.is_melee = true;
            tag.attack_range = 1.8;
        }
        let eid = self.alloc_entity_id();
        let corpo = self.spawn_entity_body(pos);
        self.ecs.spawn((
            NetId(eid),
            corpo,
            Position(pos),
            Velocity(Vec2::ZERO),
            health,
            EntityKind::Enemy(kind),
            tag,
            DefensorTag { ilhota: k, indice: i as u8 },
        ))
    }

    /// Every held islet keeps its defenders: as many as its defence level
    /// says, at its level; a fallen one comes back after a minute; a freed
    /// islet loses them. Old invader marks expire.
    pub(crate) fn manter_defensores(&mut self) {
        use shared::territorio as tr;
        let agora = self.sim_time_s;
        for m in self.invasores.values_mut() {
            m.retain(|_, ate| *ate > agora);
        }
        self.invasores.retain(|_, m| !m.is_empty());
        let livres: Vec<u8> = self
            .defensores
            .keys()
            .copied()
            .filter(|k| !self.territorios.contains_key(k))
            .collect();
        for k in livres {
            for v in self.defensores.remove(&k).unwrap_or_default() {
                if let Some((e, _)) = v.corpo {
                    self.tirar(e);
                }
            }
        }
        let donos: Vec<(u8, [u8; tr::N_TRILHAS], u8)> =
            self.territorios.iter().map(|(k, d)| (*k, d.niveis, d.atrasos)).collect();
        for (k, niveis, atrasos) in donos {
            let Some(ilha) = shared::mar::ilhotas().get(k as usize) else {
                continue;
            };
            let n = niveis[tr::trilha::DEFENSORES];
            // Upkeep a week behind: half the guards.
            let total = tr::quantos_defensores(ilha.porte, n) / if atrasos > 0 { 2 } else { 1 };
            let nivel = tr::nivel_dos_defensores(ilha.nivel.0, niveis[tr::trilha::VILA], n);
            let mut vagas = self.defensores.remove(&k).unwrap_or_default();
            while vagas.len() > total {
                if let Some((e, _)) = vagas.pop().and_then(|v| v.corpo) {
                    self.tirar(e);
                }
            }
            vagas.resize_with(total, VagaDeDefensor::default);
            for v in vagas.iter_mut() {
                // Gone (despawned) or dead: the slot waits, then refills.
                let vivo = v.corpo.is_some_and(|(e, _)| {
                    self.ecs.get::<&EnemyTag>(e).is_ok_and(|t| !t.dead)
                });
                if v.corpo.is_some() && !vivo {
                    v.corpo = None;
                    v.volta_em = agora + DEFENSOR_VOLTA_S;
                }
                // A new level: the guard is replaced at once.
                if let Some((e, nv)) = v.corpo {
                    if nv != nivel {
                        if let Ok(n) = self.ecs.get::<&NetId>(e).map(|n| n.0) {
                            self.removed_this_tick.push(n);
                        }
                        let _ = self.ecs.despawn(e);
                        v.corpo = None;
                        v.volta_em = 0.0;
                    }
                }
            }
            // Empty slots whose wait is over get a new guard.
            for (i, v) in vagas.iter_mut().enumerate() {
                if v.corpo.is_none() && agora >= v.volta_em {
                    let pos = tr::pos_do_defensor(ilha.centro, ilha.porte, i, total);
                    let e = self.por_defensor(k, i, pos, tr::papel_do_defensor(i), nivel);
                    v.corpo = Some((e, nivel));
                    v.volta_em = 0.0;
                }
            }
            self.defensores.insert(k, vagas);
        }
    }

    /// Is the body `atacante` of the clan that holds islet `k`?
    pub(crate) fn mesmo_cla_da_ilhota(&self, atacante: EntityId, k: u8) -> bool {
        let Some(dono) = self.territorios.get(&k) else {
            return false;
        };
        self.sessions
            .values()
            .find(|s| s.entity_id == atacante)
            .is_some_and(|s| s.cla_id == dono.cla_id && s.cla_id != 0)
    }

    /// A hostile act by `atacante` on `alvo`: a structure, a defender, or a
    /// member of the holding clan standing on its islet. It marks the
    /// attacker as that islet's INVADER for `INVASOR_S`.
    pub(crate) fn marcar_invasor(&mut self, atacante: EntityId, alvo: hecs::Entity, alvo_id: EntityId) {
        let ilhota = if let Ok(t) = self.ecs.get::<&EstruturaTag>(alvo) {
            Some(t.ilhota)
        } else if let Ok(d) = self.ecs.get::<&DefensorTag>(alvo) {
            Some(d.ilhota)
        } else {
            self.sessions
                .values()
                .find(|s| s.entity_id == alvo_id)
                .and_then(|s| {
                    let k = s.em_terra?.ilhota;
                    let dono = self.territorios.get(&k)?;
                    (dono.cla_id == s.cla_id && s.cla_id != 0).then_some(k)
                })
        };
        let Some(k) = ilhota else {
            return;
        };
        if self.mesmo_cla_da_ilhota(atacante, k) {
            return;
        }
        let ate = self.sim_time_s + shared::territorio::INVASOR_S;
        self.invasores.entry(k).or_default().insert(atacante, ate);
    }

    // ─────────────────── phase 4.5: the cannons fire ───────────────────

    /// Each standing islet cannon fires at the nearest enemy ship in reach:
    /// in the siege window any ship from outside the holding clan; outside
    /// it, only an invader's ship. Then the balls that landed this tick.
    pub(crate) fn tick_canhoes_das_ilhas(&mut self) {
        use shared::territorio as tr;
        let agora = self.sim_time_s;
        // Ships: (captain, position, captain's body, clan).
        let navios: Vec<(SessionId, Vec2, EntityId, i64)> = self
            .ecs
            .query::<(&Position, &super::conves::NavioEnt)>()
            .iter()
            .filter_map(|(_, (p, n))| {
                let s = self.sessions.get(&n.dono)?;
                Some((n.dono, p.0, s.entity_id, s.cla_id))
            })
            .collect();
        let mut tiros: Vec<(hecs::Entity, EntityId, Vec2, SessionId, Vec2, i32)> = Vec::new();
        for (k, es) in &self.estruturas {
            let Some(dono) = self.territorios.get(k) else {
                continue;
            };
            let cerco = self.cercos_abertos.contains(k);
            let invasores = self.invasores.get(k);
            let dano = tr::dano_do_canhao(dono.niveis[tr::trilha::CANHOES]);
            for c in &es.canhoes {
                if self.recarga_dos_canhoes.get(c).is_some_and(|t| *t > agora) {
                    continue;
                }
                let de_pe = self.ecs.get::<&Health>(*c).is_ok_and(|h| h.current > 0);
                let (Some(pos), Ok(eid)) = (
                    self.ecs.get::<&Position>(*c).ok().map(|p| p.0),
                    self.ecs.get::<&NetId>(*c).map(|n| n.0),
                ) else {
                    continue;
                };
                if !de_pe {
                    continue;
                }
                let alvo = navios
                    .iter()
                    .filter(|(_, p, corpo, cla)| {
                        p.distance(pos) <= tr::ALCANCE_DO_CANHAO
                            && *cla != dono.cla_id
                            && (cerco || invasores.is_some_and(|m| m.contains_key(corpo)))
                    })
                    .min_by(|a, b| a.1.distance(pos).total_cmp(&b.1.distance(pos)));
                if let Some((sid, p, _, _)) = alvo {
                    tiros.push((*c, eid, pos, *sid, *p, dano));
                }
            }
        }
        for (canhao, eid, de, alvo, mira, dano) in tiros {
            self.recarga_dos_canhoes.insert(canhao, agora + tr::RECARGA_DO_CANHAO_S);
            let voo = shared::navio::tempo_de_voo(de.distance(mira));
            self.ecs.spawn((BalaDaIlha { alvo, mira, dano, chega_em: agora + voo },));
            for s in self.sessions.values().filter(|s| s.logged_in) {
                let _ = s.handle.to_client.send(ServerMessage::SkillImpactFx {
                    skill_id: shared::navio::id_de_skill(1),
                    caster_eid: eid,
                    caster_pos: de,
                    target_pos: mira,
                    tier: 0,
                });
            }
        }
        // The balls that landed: a hit if the ship is still there.
        let prontas: Vec<(hecs::Entity, BalaDaIlha)> = self
            .ecs
            .query::<&BalaDaIlha>()
            .iter()
            .filter(|(_, b)| b.chega_em <= agora)
            .map(|(e, b)| (e, *b))
            .collect();
        for (e, b) in prontas {
            let _ = self.ecs.despawn(e);
            let onde = self
                .ecs
                .query::<(&Position, &super::conves::NavioEnt)>()
                .iter()
                .find(|(_, (_, n))| n.dono == b.alvo)
                .map(|(_, (p, _))| p.0);
            if onde.is_some_and(|p| p.distance(b.mira) <= shared::navio::RAIO_DO_CASCO + 3.0) {
                self.dano_no_casco(b.alvo, b.dano, None);
            }
        }
    }

    // ─────────────────── phase 4.6: the islet's resources ───────────────────

    /// A gather on islet `k` of a node of `tier` (0 tree, 1..4 rock): on a
    /// held islet the Farmer's level of that resource raises the yield
    /// (+25% a level), and a tenth of it goes to the holder's clan bank.
    pub(crate) fn recurso_da_ilhota(&mut self, k: u8, tier: u8, drops: &mut [(u16, u32)], quem: Option<SessionId>) {
        use shared::territorio as tr;
        let Some(dono) = self.territorios.get(&k) else {
            return;
        };
        let trilha = match tier {
            0 => tr::trilha::MADEIRA,
            5 => tr::trilha::ENERGIA,
            _ => tr::trilha::PEDRA,
        };
        let mult = 1.0 + 0.25 * dono.niveis[trilha] as f32;
        let cla = dono.cla_id;
        let mut taxa: Vec<(u16, u32)> = Vec::new();
        for (item, qtd) in drops.iter_mut() {
            let bruto = *qtd as f32 * mult;
            let total = bruto.floor() as u32 + u32::from(fastrand::f32() < bruto.fract());
            let t = total / 10 + u32::from(fastrand::u32(0..10) < total % 10);
            *qtd = total - t;
            if t > 0 {
                taxa.push((*item, t));
            }
        }
        if taxa.is_empty() {
            return;
        }
        let Some(ctx) = self.auth_ctx.clone() else {
            return;
        };
        // A member gathering on their own clan's islet earns clan points.
        let membro = quem
            .and_then(|sid| self.sessions.get(&sid))
            .filter(|s| s.cla_id == cla)
            .map(|s| s.name.clone());
        let pts: u32 = taxa.iter().map(|(_, q)| q).sum::<u32>() * shared::territorio::PONTOS_POR_ITEM_DE_TAXA;
        tokio::spawn(async move {
            if let Err(e) = crate::social::depositar_no_banco(&ctx.pool, cla, &taxa).await {
                tracing::warn!("banco do cla: falha ao depositar a taxa: {e:#}");
                return;
            }
            if let Some(nome) = membro {
                if let Err(e) = crate::social::somar_pontos(&ctx.pool, cla, &nome, pts).await {
                    tracing::warn!("loja do cla: falha ao somar pontos: {e:#}");
                }
            }
        });
    }

    /// PLUNDER (`PedidoNavio::SaquearBanco`): an outsider by an enemy islet's
    /// Governor, in its siege window, loots a tenth of the clan's biggest
    /// stacks — by mail, once per siege.
    pub(crate) fn saquear_banco_da_ilha(&mut self, sid: SessionId) {
        use shared::territorio as tr;
        let Some(s) = self.sessions.get(&sid) else {
            return;
        };
        let Some(k) = s.em_terra.map(|e| e.ilhota) else {
            return;
        };
        let Some(dono) = self.territorios.get(&k).cloned() else {
            return;
        };
        if s.cla_id == dono.cla_id && s.cla_id != 0 {
            return;
        }
        if !self.cercos_abertos.contains(&k) {
            self.avisa_missao(sid, "The bank can only be plundered during a siege.".into());
            return;
        }
        let Some(ilha) = shared::mar::ilhotas().get(k as usize) else {
            return;
        };
        let perto = s
            .entity
            .and_then(|e| self.ecs.get::<&Position>(e).ok().map(|p| p.0))
            .is_some_and(|p| p.distance(tr::pos_do_governador(ilha.centro)) <= tr::ALCANCE_DO_NPC + 1.0);
        if !perto {
            return;
        }
        let eu = s.entity_id;
        if !self.saques_do_cerco.insert((k, eu)) {
            self.avisa_missao(sid, "You already plundered this bank in this siege.".into());
            return;
        }
        let nome = self.nome_de(sid);
        let Some(ctx) = self.auth_ctx.clone() else {
            return;
        };
        tokio::spawn(async move {
            let resultado = crate::social::saquear_banco(&ctx.pool, dono.cla_id, &nome)
                .await
                .map_err(|e| e.to_string());
            let _ = ctx.tx.send(IncomingMessage::Saque { sid, resultado });
        });
    }

    pub fn on_saque(&mut self, sid: SessionId, resultado: Result<Vec<(u16, u32)>, String>) {
        match resultado {
            Ok(v) => {
                let n: u32 = v.iter().map(|(_, q)| q).sum();
                self.send_chat_to(sid, &format!("Plundered {n} items from the clan bank: they are in your Mail."));
            }
            Err(e) => self.avisa_missao(sid, e),
        }
    }

    // ─────────────────── phase 5: the wars ───────────────────

    /// The war on islet `k` whose window is open now, if any.
    pub(crate) fn guerra_aberta(&self, k: u8) -> Option<&shared::territorio::Guerra> {
        self.guerras.iter().find(|g| g.ilhota == k && self.guerras_abertas.contains(&g.id))
    }

    fn lista_de_guerras(&self) -> Vec<shared::territorio::Guerra> {
        self.guerras.clone()
    }

    pub fn on_guerras(&mut self, v: Vec<shared::territorio::Guerra>) {
        if v != self.guerras {
            self.guerras = v;
            self.avisar_guerras();
        }
    }

    fn avisar_guerras(&mut self) {
        let lista = self.lista_de_guerras();
        for s in self.sessions.values_mut().filter(|s| s.logged_in) {
            s.guerras_enviadas = true;
            let _ = s.handle.to_client.send(ServerMessage::Guerras { lista: lista.clone() });
        }
    }

    /// Each tick: a war's window opens at its start; past its end it runs
    /// on while an attacker stands in the ring (unlimited overtime); then it
    /// closes — the defender held — and the walls, gates and cannons are
    /// whole again.
    pub(crate) fn tick_guerras(&mut self) {
        let lista = self.lista_de_guerras();
        for s in self.sessions.values_mut() {
            if s.logged_in && !s.guerras_enviadas {
                s.guerras_enviadas = true;
                let _ = s.handle.to_client.send(ServerMessage::Guerras { lista: lista.clone() });
            }
        }
        let agora = agora_unix();
        let mut fechar = Vec::new();
        for g in &self.guerras {
            let atacante_no_anel = self.cla_no_anel(g.ilhota, g.atacante_id);
            let aberta = agora >= g.inicio && (agora < g.fim || atacante_no_anel);
            if aberta {
                if self.guerras_abertas.insert(g.id) {
                    self.cercos_abertos.insert(g.ilhota);
                    tracing::info!("guerra {}: abriu na ilhota {}", g.id, g.ilhota);
                }
            } else if agora >= g.fim {
                fechar.push((g.id, g.ilhota));
            }
        }
        for (id, k) in fechar {
            self.fechar_guerra(id, k, true);
        }
    }

    /// A war ends (`defensor_ficou`: the window ran out with no conquest).
    pub(crate) fn fechar_guerra(&mut self, id: i64, k: u8, defensor_ficou: bool) {
        self.guerras.retain(|g| g.id != id);
        self.guerras_abertas.remove(&id);
        self.cercos_abertos.remove(&k);
        self.tomadas.remove(&k);
        // Whole again for the next war.
        if let Some(es) = self.estruturas.get(&k) {
            let todas: Vec<hecs::Entity> = es.muros.iter().flatten().copied().chain(es.portao).chain(es.canhoes.iter().copied()).collect();
            for e in todas {
                if let Ok(mut h) = self.ecs.get::<&mut Health>(e) {
                    h.current = h.max;
                }
            }
        }
        if defensor_ficou {
            let ilha = shared::mar::ilhotas().get(k as usize).map_or("the islet", |i| i.nome);
            let texto = format!("The war for {ilha} is over: the defenders held.");
            let alvos: Vec<SessionId> = self.sessions.iter().filter(|(_, s)| s.logged_in).map(|(id, _)| *id).collect();
            for sid in alvos {
                self.send_chat_to(sid, &texto);
            }
            if let Some(ctx) = self.auth_ctx.clone() {
                tokio::spawn(async move {
                    if let Err(e) = crate::social::encerrar_guerra(&ctx.pool, id, None).await {
                        tracing::warn!("guerra: falha ao encerrar: {e:#}");
                    }
                });
            }
        }
        self.avisar_guerras();
    }

    /// Does clan `cla` have someone standing in islet `k`'s ring?
    fn cla_no_anel(&self, k: u8, cla: i64) -> bool {
        let Some(ilha) = shared::mar::ilhotas().get(k as usize) else {
            return false;
        };
        self.sessions.values().any(|s| {
            s.logged_in
                && s.cla_id == cla
                && cla != 0
                && s.entity
                    .and_then(|e| self.ecs.get::<&Position>(e).ok().map(|p| p.0))
                    .is_some_and(|p| p.distance(ilha.centro) <= shared::territorio::RAIO_DO_NUCLEO)
        })
    }

    /// The islet whose Governor `sid` stands by, if any (held by anyone).
    pub(crate) fn governador_ao_lado(&self, sid: SessionId) -> Option<u8> {
        let s = self.sessions.get(&sid)?;
        let k = s.em_terra?.ilhota;
        self.territorios.get(&k)?;
        let ilha = shared::mar::ilhotas().get(k as usize)?;
        let p = self.ecs.get::<&Position>(s.entity?).ok()?.0;
        (p.distance(shared::territorio::pos_do_governador(ilha.centro)) <= shared::territorio::ALCANCE_DO_NPC + 1.0)
            .then_some(k)
    }

    /// DECLARE WAR on the enemy islet whose Governor I stand by.
    pub(crate) fn declarar_guerra_aqui(&mut self, sid: SessionId) {
        let Some(k) = self.governador_ao_lado(sid) else {
            return;
        };
        let nome = self.nome_de(sid);
        let Some(ctx) = self.auth_ctx.clone() else {
            return;
        };
        let agora = agora_unix();
        tokio::spawn(async move {
            let resultado = crate::social::declarar_guerra(&ctx.pool, &nome, k, agora)
                .await
                .map(|sabado| {
                    let ilha = shared::mar::ilhotas().get(k as usize).map_or("the islet", |i| i.nome);
                    format!(
                        "War declared on {ilha} for the weekend of {} (the defender picks the hour).",
                        shared::territorio::data_curta(sabado)
                    )
                })
                .map_err(|e| e.to_string());
            let _ = ctx.tx.send(IncomingMessage::RespostaDeGuerra { sid, resultado });
        });
    }

    /// SET MY ISLET'S WAR TIME at its Governor.
    pub(crate) fn agendar_guerra_aqui(&mut self, sid: SessionId, dia: u8, hora: u8) {
        let Some(k) = self.governador_ao_lado(sid) else {
            return;
        };
        let nome = self.nome_de(sid);
        let Some(ctx) = self.auth_ctx.clone() else {
            return;
        };
        let agora = agora_unix();
        tokio::spawn(async move {
            let resultado = crate::social::agendar_guerra(&ctx.pool, &nome, k, dia, hora, agora)
                .await
                .map(|_| {
                    let d = if dia == 0 { "Sunday" } else { "Saturday" };
                    format!("War time set: {d} {:02}:00.", hora.clamp(shared::territorio::HORA_MIN, shared::territorio::HORA_MAX))
                })
                .map_err(|e| e.to_string());
            let _ = ctx.tx.send(IncomingMessage::RespostaDeGuerra { sid, resultado });
        });
    }

    pub fn on_resposta_de_guerra(&mut self, sid: SessionId, resultado: Result<String, String>) {
        match resultado {
            Ok(t) => {
                self.send_chat_to(sid, &t);
                // The new war or slot reaches everyone at once.
                self.carregar_territorios();
            }
            Err(e) => self.avisa_missao(sid, e),
        }
    }

    /// GOLD FOR THE UPKEEP (`PedidoNavio::DepositarOuro`): at my clan's
    /// islet's Governor, `qtd` of my gold into the clan bank (refunded if the
    /// database refuses).
    pub(crate) fn depositar_ouro_aqui(&mut self, sid: SessionId, qtd: u32) {
        let Some(k) = self.governador_ao_lado(sid) else {
            return;
        };
        let Some(dono) = self.territorios.get(&k).cloned() else {
            return;
        };
        let Some(s) = self.sessions.get_mut(&sid) else {
            return;
        };
        if s.cla_id != dono.cla_id || s.cla_id == 0 || qtd == 0 {
            return;
        }
        if s.gold < qtd as u64 {
            self.avisa_missao(sid, "You do not have that much gold.".into());
            return;
        }
        s.gold -= qtd as u64;
        self.save_pending = true;
        let Some(ctx) = self.auth_ctx.clone() else {
            return;
        };
        let cla = dono.cla_id;
        let nome = self.nome_de(sid);
        tokio::spawn(async move {
            let resultado = crate::social::depositar_no_banco(&ctx.pool, cla, &[(shared::item_id::GOLD, qtd)])
                .await
                .map_err(|e| e.to_string());
            if resultado.is_ok() {
                let pts = qtd / shared::territorio::OURO_POR_PONTO;
                if let Err(e) = crate::social::somar_pontos(&ctx.pool, cla, &nome, pts).await {
                    tracing::warn!("loja do cla: falha ao somar pontos: {e:#}");
                }
            }
            let _ = ctx.tx.send(IncomingMessage::Deposito { sid, qtd, resultado });
        });
    }

    pub fn on_deposito(&mut self, sid: SessionId, qtd: u32, resultado: Result<(), String>) {
        match resultado {
            Ok(()) => self.send_chat_to(sid, &format!("{qtd} gold put in the clan bank for the upkeep.")),
            Err(e) => {
                if let Some(s) = self.sessions.get_mut(&sid) {
                    s.gold += qtd as u64;
                }
                self.save_pending = true;
                self.avisa_missao(sid, e);
            }
        }
    }

    /// THE ISLET AS A BASE (`PedidoNavio::IrParaIlhaDoCla`): from anywhere to
    /// my clan's islet, ashore in front of its Governor. Free, with a
    /// cooldown; not from a dungeon, the tutorial or a PvP fight (the handoff
    /// refuses that), not from aboard a ship at sea (go ashore first).
    pub(crate) fn ir_para_ilha_do_cla(&mut self, sid: SessionId) {
        if self.tutorial_mode || self.dungeon_mode {
            self.avisa_missao(sid, "You can't travel to your islet from here.".into());
            return;
        }
        let agora = self.sim_time_s;
        let Some(s) = self.sessions.get(&sid).filter(|s| s.logged_in) else {
            return;
        };
        let Some(k) = s.ilhota_do_cla else {
            self.avisa_missao(sid, "Your clan holds no islet.".into());
            return;
        };
        if s.instancia != 0 || s.downed || s.carrying.is_some() || s.carried_by.is_some() {
            self.avisa_missao(sid, "You can't travel to your islet right now.".into());
            return;
        }
        if agora < s.viagem_a_ilha_em {
            let falta = (s.viagem_a_ilha_em - agora).ceil() as u32;
            self.avisa_missao(sid, format!("The trip to your islet is ready again in {}:{:02}.", falta / 60, falta % 60));
            return;
        }
        let Some(ilha) = shared::mar::ilhotas().get(k as usize) else {
            return;
        };
        let chegada = shared::territorio::chegada_na_ilhota(ilha.centro);
        if !self.no_mar() {
            let aviso = format!("You travel to your clan's islet, {}.", ilha.nome);
            if self.mandar_para_zona(sid, shared::mar::ZONA, chegada, Some(&aviso), Some(ilha.nome)) {
                if let Some(s) = self.sessions.get_mut(&sid) {
                    s.viagem_a_ilha_em = agora + shared::territorio::RECARGA_DA_VIAGEM_S;
                }
            }
            return;
        }
        // Already at sea: ashore only (a ship at anchor stays where it is).
        let Some(s) = self.sessions.get(&sid) else {
            return;
        };
        if s.conves.is_some() {
            self.avisa_missao(sid, "At sea, go ashore first: your ship cannot follow.".into());
            return;
        }
        let Some(corpo) = s.entity else {
            return;
        };
        let navio = s.em_terra.and_then(|t| t.navio);
        if let Ok(mut p) = self.ecs.get::<&mut Position>(corpo) {
            p.0 = chegada;
        }
        if let Some(s) = self.sessions.get_mut(&sid) {
            s.em_terra = Some(super::conves::EmTerra { ilhota: k, navio });
            s.rota.limpa();
            s.viagem_a_ilha_em = agora + shared::territorio::RECARGA_DA_VIAGEM_S;
            let _ = s.handle.to_client.send(ServerMessage::EmTerra { ilhota: Some(k) });
        }
        self.save_pending = true;
        self.send_chat_to(sid, &format!("You travel to your clan's islet, {}.", ilha.nome));
    }
}

/// Now, unix seconds.
pub(crate) fn agora_unix() -> i64 {
    (now_ms() / 1000) as i64
}
