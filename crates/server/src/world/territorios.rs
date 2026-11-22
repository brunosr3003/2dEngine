//! TERRITORIES, phase 3 (docs/TERRITORIOS.md): who holds which territory
//! islet, the claim, and the list every client at sea keeps.
//!
//! The database (`territorios`, `social::reivindicar`) is the truth; the sea
//! process keeps a copy (`GameWorld::territorios`), reloaded every 30 s and
//! after each claim, so a clan dissolved elsewhere frees its islet here too.

use super::*;
use shared::territorio::{Dono, TomadaInfo, RAIO_DO_NUCLEO, TEMPO_DE_TOMADA_S};

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
        tokio::spawn(async move {
            match crate::social::territorios(&ctx.pool).await {
                Ok(v) => {
                    let _ = ctx.tx.send(IncomingMessage::Territorios(v));
                }
                Err(e) => tracing::warn!("territorios: falha ao carregar: {e:#}"),
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
        if self.territorios.contains_key(&t.ilhota) {
            return Err("This islet already belongs to a clan.");
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
                self.territorios.insert(d.ilhota, d);
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
}
