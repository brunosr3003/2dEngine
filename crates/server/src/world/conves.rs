//! THE WALKABLE DECK, server side (docs/NAVEGACAO.md, "Convés andável",
//! 17/11/2026).
//!
//! The ship is its own entity at sea (`NavioEnt`): position, heading,
//! velocity and the hull as its `Health`, owned by its captain. The players
//! aboard are ordinary player entities whose place is kept in DECK space
//! (`Session::conves`); their world `Position` is `ship × local`, rebuilt
//! every tick (`mover_navios_e_conves`), so everything land combat reads —
//! cones, projectiles, areas, targets, the area of interest — keeps working
//! on world positions without knowing about ships.
//!
//! - The HELM: at the wheel the captain's stick, route and AUTO pilot steer
//!   the ship through `Leme::governa`, as sailing always did; off it the
//!   stick walks the body on deck and the ship follows the live route and
//!   stops at its end (`world.rs`, the input loop).
//! - CREW: a party member close by boards ("Board <name>'s ship"); their
//!   own ship is stowed. Docking lands everyone aboard at that port; sinking
//!   sends everyone to the dock; a captain who leaves drops the crew at the
//!   nearest dock.

use super::*;
use shared::conves as cv;
use shared::navio as nv;

/// Aboard a ship: whose, where on deck (units, `shared::conves`), and at the
/// helm or not.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ABordo {
    pub navio: hecs::Entity,
    pub navio_eid: EntityId,
    pub capitao: SessionId,
    pub local: Vec2,
    pub leme: bool,
}

/// The ship entity's own data. Its hull is the entity's `Health`.
#[derive(Debug, Clone, Copy)]
pub struct NavioEnt {
    pub dono: SessionId,
    pub item: u16,
    pub classe: usize,
    /// The bow's unit direction (the captain's `Leme`).
    pub rumo: Vec2,
}

/// What a sea mob needs of the ship a player stands on.
#[derive(Debug, Clone, Copy)]
pub struct AlvoNoMar {
    pub centro: Vec2,
    pub rumo: Vec2,
    pub vel: Vec2,
    pub classe: usize,
}

/// The point of a ship's hull (its deck's outline, a box `meia_boca` ×
/// `meio_comp`) nearest `p`: what a sea mob measures its bite against, so a
/// shark at the side reaches the ship whoever it picked aboard.
pub fn casco_mais_perto(a: &AlvoNoMar, p: Vec2) -> Vec2 {
    let l = cv::para_local(a.centro, a.rumo, p);
    let q = Vec2::new(l.x.clamp(-cv::meia_boca(a.classe), cv::meia_boca(a.classe)), l.y.clamp(-cv::meio_comp(a.classe), cv::meio_comp(a.classe)));
    cv::do_mundo(a.centro, a.rumo, q)
}

impl GameWorld {
    /// The ship this session stands on, and its captain.
    pub(crate) fn navio_de(&self, sid: SessionId) -> Option<(hecs::Entity, SessionId)> {
        self.sessions.get(&sid)?.conves.map(|c| (c.navio, c.capitao))
    }

    /// Who commands the hull this session is on (itself when not aboard).
    pub(crate) fn capitao_de(&self, sid: SessionId) -> SessionId {
        self.navio_de(sid).map_or(sid, |(_, c)| c)
    }

    /// The HULL's position: the ship this session stands on, else its body.
    pub(crate) fn pos_do_casco(&self, sid: SessionId) -> Option<Vec2> {
        match self.navio_de(sid) {
            Some((e, _)) => self.ecs.get::<&Position>(e).ok().map(|p| p.0).or_else(|| self.pos_do_jogador(sid)),
            None => self.pos_do_jogador(sid),
        }
    }

    /// The hull's velocity (its entity's), or the body's.
    pub(crate) fn vel_do_casco(&self, sid: SessionId) -> Vec2 {
        let e = self.navio_de(sid).map(|(e, _)| e).or_else(|| self.sessions.get(&sid).and_then(|s| s.entity));
        e.and_then(|e| self.ecs.get::<&Velocity>(e).ok().map(|v| v.0)).unwrap_or(Vec2::ZERO)
    }

    /// The crew of `capitao`'s ship (not counting the captain).
    pub(crate) fn tripulacao_de(&self, capitao: SessionId) -> Vec<SessionId> {
        self.sessions
            .iter()
            .filter(|(id, s)| **id != capitao && s.conves.is_some_and(|c| c.capitao == capitao))
            .map(|(id, _)| *id)
            .collect()
    }

    /// Sea mobs' view of every player aboard: the ship under them.
    pub(crate) fn alvos_no_mar(&self) -> HashMap<EntityId, AlvoNoMar> {
        let mut m = HashMap::new();
        if !self.no_mar() {
            return m;
        }
        for s in self.sessions.values() {
            let Some(c) = s.conves else { continue };
            let Ok(n) = self.ecs.get::<&NavioEnt>(c.navio) else { continue };
            let Some(centro) = self.ecs.get::<&Position>(c.navio).ok().map(|p| p.0) else { continue };
            let vel = self.ecs.get::<&Velocity>(c.navio).map_or(Vec2::ZERO, |v| v.0);
            m.insert(s.entity_id, AlvoNoMar { centro, rumo: n.rumo, vel, classe: n.classe });
        }
        m
    }

    /// SAIL OUT on my own ship: it is spawned where the body floats, the
    /// body at the wheel (or at `local`, a reconnect). `false` without a
    /// seaworthy ship.
    pub(crate) fn lancar_navio(&mut self, sid: SessionId, local: Option<Vec2>) -> bool {
        let Some((item, d)) = self.navio_equipado(sid) else {
            return false;
        };
        if !nv::pode_zarpar(&d) {
            return false;
        }
        let Some(corpo) = self.pos_do_jogador(sid) else {
            return false;
        };
        let p = self.agua_no_mar(corpo);
        let classe = nv::classe(item);
        // Bow out to sea: away from the nearest island.
        let rumo = match self.sessions.get(&sid).and_then(|s| s.leme_do_mar.rumo()) {
            Some(r) => r,
            None => shared::mar::ilhas()
                .iter()
                .min_by(|a, b| a.centro.distance(p).total_cmp(&b.centro.distance(p)))
                .and_then(|i| (p - i.centro).try_normalize())
                .unwrap_or(Vec2::Y),
        };
        let eid = self.alloc_entity_id();
        let total = nv::casco_total(item, &d) as i32;
        let e = self.ecs.spawn((
            NetId(eid),
            Position(p),
            Velocity(Vec2::ZERO),
            EntityKind::Navio(item),
            Health { current: d.casco as i32, max: total.max(1) },
            NavioEnt { dono: sid, item, classe, rumo },
        ));
        let lugar = local.map(|l| cv::ponto_livre(classe, l)).unwrap_or_else(|| cv::leme(classe));
        if let Some(s) = self.sessions.get_mut(&sid) {
            if !s.leme_do_mar.pronto {
                s.leme_do_mar.proa = rumo.y.atan2(rumo.x);
                s.leme_do_mar.pronto = true;
            }
            s.leme_do_mar.vel = 0.0;
            s.conves = Some(ABordo { navio: e, navio_eid: eid, capitao: sid, local: lugar, leme: local.is_none() });
        }
        self.por_no_conves(sid);
        true
    }

    /// The body's world position from its deck place, now.
    pub(crate) fn por_no_conves(&mut self, sid: SessionId) {
        let Some(s) = self.sessions.get(&sid) else { return };
        let (Some(c), Some(corpo)) = (s.conves, s.entity) else { return };
        let Some(p) = self.ecs.get::<&Position>(c.navio).ok().map(|p| p.0) else { return };
        let rumo = self.ecs.get::<&NavioEnt>(c.navio).map_or(Vec2::Y, |n| n.rumo);
        if let Ok(mut q) = self.ecs.get::<&mut Position>(corpo) {
            q.0 = cv::do_mundo(p, rumo, c.local);
        }
    }

    /// Moves the ship under `sid` (and everyone on it) to `p` — tests, and
    /// the "no port is open" drift.
    pub(crate) fn por_navio_em(&mut self, sid: SessionId, p: Vec2) {
        match self.navio_de(sid) {
            Some((e, _)) => {
                if let Ok(mut q) = self.ecs.get::<&mut Position>(e) {
                    q.0 = p;
                }
                let a_bordo: Vec<SessionId> =
                    self.sessions.iter().filter(|(_, s)| s.conves.is_some_and(|c| c.navio == e)).map(|(id, _)| *id).collect();
                for s in a_bordo {
                    self.por_no_conves(s);
                }
            }
            None => {
                if let Some(e) = self.sessions.get(&sid).and_then(|s| s.entity) {
                    if let Ok(mut q) = self.ecs.get::<&mut Position>(e) {
                        q.0 = p;
                    }
                }
            }
        }
    }

    fn saindo_da_zona(&self, s: &Session) -> bool {
        self.zona_de_saida.contains_key(&s.name)
    }

    fn despachar_navio(&mut self, e: hecs::Entity) {
        if let Ok(n) = self.ecs.get::<&NetId>(e).map(|n| n.0) {
            self.removed_this_tick.push(n);
        }
        let _ = self.ecs.despawn(e);
    }

    /// THE DECK'S BOOKKEEPING, each tick at sea (start of `tick_mar`):
    /// ships whose captain left go; crew whose ship went are put ashore at
    /// the nearest dock; everyone at sea not aboard sails out on their own
    /// ship (login, arrival) or, without one, is washed ashore; the hull
    /// rides on the ship entity's `Health`.
    pub(crate) fn tick_conves(&mut self) {
        if !self.no_mar() {
            for s in self.sessions.values_mut() {
                s.conves = None;
                s.conves_enviado.clear();
            }
            return;
        }
        // 1. Ships without their captain aboard.
        let navios: Vec<(hecs::Entity, SessionId)> =
            self.ecs.query::<&NavioEnt>().iter().map(|(e, n)| (e, n.dono)).collect();
        for (e, dono) in navios {
            let fica = self.sessions.get(&dono).is_some_and(|s| {
                s.logged_in && s.entity.is_some() && !self.saindo_da_zona(s) && s.conves.is_some_and(|c| c.navio == e && c.capitao == dono)
            });
            if !fica {
                self.despachar_navio(e);
            }
        }
        // 2. Aboard a ship that is gone: ashore.
        let orfaos: Vec<SessionId> = self
            .sessions
            .iter()
            .filter(|(_, s)| s.conves.is_some_and(|c| !self.ecs.contains(c.navio)))
            .map(|(id, _)| *id)
            .collect();
        for sid in orfaos {
            if let Some(s) = self.sessions.get_mut(&sid) {
                s.conves = None;
            }
            let saindo = self.sessions.get(&sid).is_some_and(|s| self.saindo_da_zona(s));
            if !saindo {
                self.levar_ao_cais(sid, "Your captain left the sea. You are put ashore at the nearest dock.");
            }
        }
        // 3. At sea and not aboard: sail out (login, arrival), else ashore.
        let soltos: Vec<SessionId> = self
            .sessions
            .iter()
            .filter(|(_, s)| s.logged_in && s.entity.is_some() && s.conves.is_none() && !self.zona_de_saida.contains_key(&s.name))
            .map(|(id, _)| *id)
            .collect();
        let tenta_o_cais = self.tick % 30 == 0;
        for sid in soltos {
            if !self.lancar_navio(sid, None) && tenta_o_cais {
                // At sea without a ship that floats (unequipped out there, a
                // hull that sank): the sea washes them ashore.
                self.levar_ao_cais(sid, "Without a ship that floats, the sea washes you ashore at the nearest dock.");
            }
        }
        // 4. The hull on the entity (what the others see on the wire), and
        // its class (a new hull built at sea).
        let cascos: Vec<(hecs::Entity, i32, i32, u16)> = self
            .sessions
            .iter()
            .filter_map(|(id, s)| {
                let c = s.conves.filter(|c| c.capitao == *id)?;
                let item = s.equipment.navio.filter(|i| shared::item_id::e_navio(*i))?;
                let d = nv::dados(s.equipment.navio_inst.as_ref(), item);
                Some((c.navio, d.casco as i32, nv::casco_total(item, &d) as i32, item))
            })
            .collect();
        for (e, casco, total, item) in cascos {
            if let Ok(mut h) = self.ecs.get::<&mut Health>(e) {
                h.current = casco;
                h.max = total.max(1);
            }
            let mudou = self.ecs.get::<&NavioEnt>(e).is_ok_and(|n| n.item != item);
            if mudou {
                if let Ok(mut n) = self.ecs.get::<&mut NavioEnt>(e) {
                    n.item = item;
                    n.classe = nv::classe(item);
                }
                if let Ok(mut k) = self.ecs.get::<&mut EntityKind>(e) {
                    *k = EntityKind::Navio(item);
                }
            }
        }
    }

    /// THE SHIPS SAIL AND THE DECKS RIDE ALONG (right after the bodies'
    /// pass, which skips everyone aboard). Each ship moves on its velocity
    /// (set by its captain's helm, `world.rs`) against the coast
    /// (`mover_raspando`; a PK captain's peaceful waters are coast too);
    /// then each body aboard takes its deck step (`shared::conves::mover`:
    /// the rails are walls) and its world position is `ship × local`.
    pub(crate) fn mover_navios_e_conves(&mut self, dt: f32, barreira: &HashMap<hecs::Entity, (bool, Option<Vec2>)>) {
        if !self.no_mar() {
            return;
        }
        let mut bateu = Vec::new();
        let navios: Vec<(hecs::Entity, Vec2, Vec2, SessionId)> = self
            .ecs
            .query::<(&Position, &Velocity, &NavioEnt)>()
            .iter()
            .map(|(e, (p, v, n))| (e, p.0, v.0, n.dono))
            .collect();
        // Where each deck was before it sailed: a body something else moved
        // this tick (a leap, a blink, a knockback by position) is read back
        // into deck space against it.
        let antes: HashMap<hecs::Entity, (Vec2, Vec2)> = self
            .ecs
            .query::<(&Position, &NavioEnt)>()
            .iter()
            .map(|(e, (p, n))| (e, (p.0, n.rumo)))
            .collect();
        for (e, pos, vel, dono) in navios {
            let (barrado, empurra) = barreira.get(&e).copied().unwrap_or((false, None));
            let v = empurra.unwrap_or(vel);
            let livre = |x: f32, z: f32| shared::mar::navegavel(x, z);
            let fechada = |x: f32, z: f32| shared::mar::navegavel(x, z) && !shared::mar::aguas_pacificas(Vec2::new(x, z));
            let novo = if barrado {
                let p = nv::mover_raspando(&fechada, pos, v, dt, nv::RAIO_DO_CASCO);
                let sem = nv::mover_raspando(&livre, pos, v, dt, nv::RAIO_DO_CASCO);
                if sem.distance(p) > 1e-3 && shared::mar::aguas_pacificas(sem) {
                    bateu.push(e);
                }
                p
            } else {
                nv::mover_raspando(&livre, pos, v, dt, nv::RAIO_DO_CASCO)
            };
            let rumo = self.sessions.get(&dono).and_then(|s| s.leme_do_mar.rumo());
            if let Ok(mut q) = self.ecs.get::<&mut Position>(e) {
                q.0 = novo;
            }
            if let (Some(r), Ok(mut n)) = (rumo, self.ecs.get::<&mut NavioEnt>(e)) {
                n.rumo = r;
            }
        }
        for e in bateu {
            self.avisa_barreira(e);
        }
        // The bodies aboard.
        let a_bordo: Vec<(SessionId, ABordo, hecs::Entity)> = self
            .sessions
            .iter()
            .filter(|(_, s)| !self.zona_de_saida.contains_key(&s.name))
            .filter_map(|(id, s)| Some((*id, s.conves?, s.entity?)))
            .collect();
        for (sid, c, corpo) in a_bordo {
            let Some(centro) = self.ecs.get::<&Position>(c.navio).ok().map(|p| p.0) else { continue };
            let Some((rumo, classe)) = self.ecs.get::<&NavioEnt>(c.navio).ok().map(|n| (n.rumo, n.classe)) else { continue };
            let passo = self.ecs.get::<&Velocity>(corpo).map_or(Vec2::ZERO, |v| v.0);
            let mut base = c.local;
            if let (Some((c0, r0)), Some(agora)) = (antes.get(&c.navio), self.ecs.get::<&Position>(corpo).ok().map(|p| p.0)) {
                if agora.distance(cv::do_mundo(*c0, *r0, c.local)) > 0.05 {
                    // Moved off its deck place by something else: where it
                    // landed on the deck (off the rail = the nearest plank).
                    base = cv::ponto_livre(classe, cv::para_local(*c0, *r0, agora));
                }
            }
            let local = if c.leme {
                cv::leme(classe)
            } else {
                cv::mover(classe, base, cv::dir_local(rumo, passo) * dt)
            };
            if let Some(s) = self.sessions.get_mut(&sid) {
                if let Some(cc) = s.conves.as_mut() {
                    cc.local = local;
                }
            }
            if let Ok(mut q) = self.ecs.get::<&mut Position>(corpo) {
                q.0 = cv::do_mundo(centro, rumo, local);
            }
        }
    }

    /// The deck rows each client should know (`ServerMessage::Conves`):
    /// everyone aboard within its area of interest; only what changed.
    pub(crate) fn enviar_conves(&mut self) {
        if !self.no_mar() {
            return;
        }
        let linhas: Vec<(shared::conves::NoConves, Vec2)> = self
            .sessions
            .values()
            .filter_map(|s| {
                let c = s.conves?;
                let p = s.entity.and_then(|e| self.ecs.get::<&Position>(e).ok().map(|p| p.0))?;
                Some((cv::NoConves::novo(s.entity_id, c.navio_eid, c.local, c.leme), p))
            })
            .collect();
        let raio = AOI_RADIUS + 2.0;
        let centros: HashMap<SessionId, Vec2> =
            self.sessions.keys().filter_map(|id| Some((*id, self.pos_do_jogador(*id)?))).collect();
        for (sid, s) in self.sessions.iter_mut() {
            let Some(eu) = centros.get(sid) else { continue };
            let mut lista = Vec::new();
            let mut vistos = std::collections::HashSet::new();
            for (n, p) in &linhas {
                if p.distance(*eu) > raio {
                    continue;
                }
                vistos.insert(n.id);
                if s.conves_enviado.get(&n.id) != Some(n) {
                    s.conves_enviado.insert(n.id, *n);
                    lista.push(*n);
                }
            }
            let saiu: Vec<EntityId> = s.conves_enviado.keys().filter(|id| !vistos.contains(id)).copied().collect();
            for id in saiu {
                s.conves_enviado.remove(&id);
                lista.push(cv::NoConves { id, navio: EntityId(0), local: [0, 0], leme: false });
            }
            if !lista.is_empty() {
                let _ = s.handle.to_client.send(ServerMessage::Conves { lista });
            }
        }
    }

    /// TAKE or LEAVE the helm (`PedidoNavio::Leme`). Only the captain, and
    /// only near the wheel to take it.
    pub(crate) fn pegar_leme(&mut self, sid: SessionId, pegar: bool) {
        let Some(c) = self.sessions.get(&sid).and_then(|s| s.conves) else {
            return;
        };
        if c.capitao != sid {
            if pegar {
                self.avisa_missao(sid, "Only the captain takes the helm.".into());
            }
            return;
        }
        if pegar && !c.leme {
            let classe = self.ecs.get::<&NavioEnt>(c.navio).map_or(0, |n| n.classe);
            if c.local.distance(cv::roda(classe)) > cv::ALCANCE_DO_LEME {
                self.avisa_missao(sid, "Walk to the ship's wheel to take the helm.".into());
                return;
            }
        }
        if let Some(s) = self.sessions.get_mut(&sid) {
            if let Some(cc) = s.conves.as_mut() {
                if cc.leme && !pegar {
                    // Stepping back: just aft of the wheel, a free spot.
                    let classe = self.ecs.get::<&NavioEnt>(cc.navio).map_or(0, |n| n.classe);
                    cc.local = cv::ponto_livre(classe, cv::leme(classe) + Vec2::new(0.7, 0.0));
                }
                cc.leme = pegar;
            }
            // The pilot only flies the ship from the helm.
            if !pegar {
                s.piloto = Default::default();
            }
        }
        self.por_no_conves(sid);
    }

    /// BOARD a party member's ship (`PedidoNavio::Embarcar`): close by, my
    /// own ship stowed (it leaves the sea; nothing is lost). `capitao` = my
    /// OWN entity: back to my own ship, alongside.
    pub(crate) fn embarcar_no_navio(&mut self, sid: SessionId, capitao: EntityId) {
        if !self.no_mar() {
            return;
        }
        let Some(eu) = self.sessions.get(&sid) else { return };
        let (meu_eid, meu_grupo, minha) = (eu.entity_id, eu.party_id, eu.conves);
        if capitao == meu_eid {
            self.voltar_ao_meu_navio(sid);
            return;
        }
        // `capitao` = the captain's body, or the ship itself (the client
        // knows the ship).
        let Some((alvo_sid, alvo)) = self
            .sessions
            .iter()
            .find(|(id, s)| s.entity_id == capitao || s.conves.is_some_and(|c| c.navio_eid == capitao && c.capitao == **id))
            .map(|(id, s)| (*id, s.conves))
        else {
            return;
        };
        let Some(alvo) = alvo.filter(|c| c.capitao == alvo_sid) else {
            self.avisa_missao(sid, "That ship has no captain aboard.".into());
            return;
        };
        if meu_grupo.is_none() || self.sessions.get(&alvo_sid).and_then(|s| s.party_id) != meu_grupo {
            self.avisa_missao(sid, "Only your party can board each other's ships.".into());
            return;
        }
        if minha.is_some_and(|c| c.navio == alvo.navio) {
            return;
        }
        if minha.is_some_and(|c| c.capitao == sid) && !self.tripulacao_de(sid).is_empty() {
            self.avisa_missao(sid, "Your own crew is aboard: you can't leave them.".into());
            return;
        }
        let (Some(aqui), Some(la)) = (self.pos_do_casco(sid), self.ecs.get::<&Position>(alvo.navio).ok().map(|p| p.0)) else {
            return;
        };
        if aqui.distance(la) > cv::ALCANCE_DO_EMBARQUE {
            self.avisa_missao(sid, "Sail alongside your party's ship to board it.".into());
            return;
        }
        // My own ship is stowed.
        if let Some(c) = minha.filter(|c| c.capitao == sid) {
            self.despachar_navio(c.navio);
        }
        let classe = self.ecs.get::<&NavioEnt>(alvo.navio).map_or(0, |n| n.classe);
        let n = self.tripulacao_de(alvo_sid).len();
        if let Some(s) = self.sessions.get_mut(&sid) {
            s.conves = Some(ABordo { navio: alvo.navio, navio_eid: alvo.navio_eid, capitao: alvo_sid, local: cv::lugar_da_tripulacao(classe, n), leme: false });
            s.leme_do_mar = Default::default();
            s.piloto = Default::default();
            s.rota.limpa();
        }
        self.por_no_conves(sid);
        let (meu_nome, nome) = (self.nome_de(sid), self.nome_de(alvo_sid));
        self.avisa_missao(sid, format!("You board {nome}'s ship."));
        self.avisa_missao(alvo_sid, format!("{meu_nome} came aboard."));
    }

    /// Crew back to their own ship, launched alongside the captain's.
    fn voltar_ao_meu_navio(&mut self, sid: SessionId) {
        let Some(c) = self.sessions.get(&sid).and_then(|s| s.conves) else { return };
        if c.capitao == sid {
            return;
        }
        let Some(la) = self.ecs.get::<&Position>(c.navio).ok().map(|p| p.0) else { return };
        let (rumo, classe) = self.ecs.get::<&NavioEnt>(c.navio).map_or((Vec2::Y, 0), |n| (n.rumo, n.classe));
        let lado = cv::do_mundo(la, rumo, Vec2::new(cv::meia_boca(classe) * 2.0 + 4.0, 0.0));
        let antes = self.sessions.get(&sid).and_then(|s| s.entity).and_then(|e| self.ecs.get::<&Position>(e).ok().map(|p| p.0));
        if let Some(s) = self.sessions.get_mut(&sid) {
            s.conves = None;
        }
        self.por_navio_em(sid, lado);
        if !self.lancar_navio(sid, None) {
            // No seaworthy ship: stay aboard.
            if let (Some(p), Some(e)) = (antes, self.sessions.get(&sid).and_then(|s| s.entity)) {
                if let Ok(mut q) = self.ecs.get::<&mut Position>(e) {
                    q.0 = p;
                }
            }
            if let Some(s) = self.sessions.get_mut(&sid) {
                s.conves = Some(c);
            }
            self.avisa_missao(sid, "Your own ship can't sail: it needs a repair at a dock.".into());
        }
    }

    /// Ashore at the nearest dock this character may land at (the one it
    /// sailed from first), with `aviso`. If no island is up it stays at sea,
    /// drifting to an anchorage — a `TrocarZona` to a dead host is the one
    /// outcome that cannot happen.
    pub(crate) fn levar_ao_cais(&mut self, sid: SessionId, aviso: &str) -> bool {
        let Some(eu) = self.pos_do_casco(sid) else {
            return false;
        };
        let volta = self.sessions.get(&sid).map(|s| s.zona_volta.clone()).unwrap_or_default();
        let mut ordem: Vec<usize> = shared::mar::ilhas().iter().map(|v| v.ilha).collect();
        ordem.sort_by(|a, b| {
            let chave = |i: &usize| {
                let v = shared::mar::vista_da_ilha(*i).expect("island of the sea");
                (v.zona != volta, v.fundeadouro().distance(eu))
            };
            let (ka, kb) = (chave(a), chave(b));
            ka.0.cmp(&kb.0).then(ka.1.total_cmp(&kb.1))
        });
        let indice = self.sessions.get(&sid).and_then(|s| crate::quests::indice_da_historia(&s.quests));
        for i in ordem {
            if !shared::viagem::liberada(i, indice) {
                continue;
            }
            let Some((dest, chegada)) = Self::porto_da_ilha(i) else {
                continue;
            };
            if self.mandar_para_zona(sid, dest.zona, chegada, Some(aviso), Some(dest.nome)) {
                if let Some(s) = self.sessions.get_mut(&sid) {
                    s.zona_volta.clear();
                    s.conves = None;
                }
                return true;
            }
        }
        // Only a captain's ship drifts; a body without one stays put.
        if self.navio_de(sid).is_some_and(|(_, c)| c == sid) {
            let porto = self.agua_no_mar(super::navio::fundeadouro_padrao());
            self.por_navio_em(sid, porto);
            self.avisa_missao(sid, "No port is open: you drift to an anchorage.".into());
        }
        false
    }
}

#[cfg(test)]
mod testes {
    use super::*;
    use shared::navio::PedidoNavio;
    use shared::protocol::InputFrame;

    fn mundo() -> (GameWorld, SessionId, mpsc::UnboundedReceiver<ServerMessage>) {
        crate::economy::init_vazia_para_testes();
        let mut w = GameWorld::new(HashMap::new());
        w.zona = shared::mar::ZONA.into();
        w.ilha = Some(shared::terreno::Ilha::da_ilha(&shared::mar::DEF));
        w.safe_zones.clear();
        w.diretorio = Some(crate::canais::Diretorio::para_teste(&["ilha_inicial", "ilha_gelo", "ilha_deserto"]));
        let (sid, rx) = marujo(&mut w, 47_470, 4_700, "Marujo", shared::mar::ninhos()[0]);
        (w, sid, rx)
    }

    fn marujo(w: &mut GameWorld, porta: u16, eid: u32, nome: &str, p: Vec2) -> (SessionId, mpsc::UnboundedReceiver<ServerMessage>) {
        let sid = SessionId(([127, 0, 0, 1], porta).into());
        let (tx, rx) = mpsc::unbounded_channel();
        w.on_connect(SessionHandle { id: sid, to_client: tx });
        let e = w.ecs.spawn((
            NetId(EntityId(eid)),
            Position(p),
            Velocity(Vec2::ZERO),
            EntityKind::Player,
            Health { current: 100, max: 100 },
            PlayerTag { name: nome.into(), player_id: PlayerId(eid as u64), attack_anim_pending: None, combo_step_pending: None },
            shared::Solido,
        ));
        let s = w.sessions.get_mut(&sid).unwrap();
        s.logged_in = true;
        s.entity = Some(e);
        s.entity_id = EntityId(eid);
        s.name = nome.into();
        s.inventory = vec![shared::InventorySlot::default(); shared::INVENTORY_SLOTS];
        super::super::navio::dar_chalupa(s);
        (sid, rx)
    }

    fn com_casco(w: &mut GameWorld, sid: SessionId, item: u16) {
        w.sessions.get_mut(&sid).unwrap().equipment.set(shared::EquipSlot::Navio, Some(item), Some(nv::instancia_nova(item)));
    }

    fn anda(w: &mut GameWorld, sid: SessionId, dir: Vec2, segundos: f32) {
        let n = (segundos / shared::TICK_DT).round() as u32;
        for k in 0..n {
            for (id, s) in w.sessions.iter_mut() {
                let d = if *id == sid { dir } else { Vec2::ZERO };
                s.pending_input = Some(InputFrame { seq: k, tick: k, move_dir: d, aim: Vec2::ZERO, buttons: 0 });
            }
            w.step(shared::TICK_DT);
        }
    }

    fn corpo(w: &GameWorld, sid: SessionId) -> Vec2 {
        w.pos_do_jogador(sid).unwrap()
    }

    fn conves(w: &GameWorld, sid: SessionId) -> ABordo {
        w.sessions[&sid].conves.expect("aboard")
    }

    fn trocou(rx: &mut mpsc::UnboundedReceiver<ServerMessage>) -> Option<String> {
        let mut z = None;
        while let Ok(m) = rx.try_recv() {
            if let ServerMessage::TrocarZona { zona, .. } = m {
                z = Some(zona);
            }
        }
        z
    }

    /// Arriving at sea puts the captain on its own ship's deck, at the
    /// wheel: the ship is an entity with the hull as its health.
    #[test]
    fn ao_chegar_o_navio_nasce_com_o_capitao_no_leme() {
        let (mut w, sid, _rx) = mundo();
        w.step(shared::TICK_DT);
        let c = conves(&w, sid);
        assert!(c.leme && c.capitao == sid);
        let n = w.ecs.get::<&NavioEnt>(c.navio).map(|n| *n).unwrap();
        assert_eq!(n.dono, sid);
        let h = w.ecs.get::<&Health>(c.navio).map(|h| *h).unwrap();
        assert_eq!(h.current, nv::casco_max(shared::item_id::NAVIO_CHALUPA) as i32);
        let centro = w.pos_do_casco(sid).unwrap();
        assert!(corpo(&w, sid).distance(cv::do_mundo(centro, n.rumo, cv::leme(0))) < 1e-3);
    }

    /// A PLAYER ON DECK MOVES WITH THE SHIP: off the helm, standing still,
    /// the body keeps its deck place while the ship coasts.
    #[test]
    fn quem_esta_no_conves_anda_com_o_navio() {
        let (mut w, sid, _rx) = mundo();
        w.step(shared::TICK_DT);
        anda(&mut w, sid, Vec2::new(1.0, 0.3), 2.0);
        w.handle_navio(sid, PedidoNavio::Leme { pegar: false });
        assert!(!conves(&w, sid).leme);
        anda(&mut w, sid, Vec2::ZERO, 0.1);
        let local0 = conves(&w, sid).local;
        let navio0 = w.pos_do_casco(sid).unwrap();
        anda(&mut w, sid, Vec2::ZERO, 0.4);
        let navio1 = w.pos_do_casco(sid).unwrap();
        assert!(navio1.distance(navio0) > 0.5, "the ship should coast: {navio0} → {navio1}");
        let c = conves(&w, sid);
        assert!(c.local.distance(local0) < 1e-4, "the body slid on deck");
        let rumo = w.ecs.get::<&NavioEnt>(c.navio).unwrap().rumo;
        assert!(corpo(&w, sid).distance(cv::do_mundo(navio1, rumo, c.local)) < 1e-3, "the body is not where the deck is");
    }

    /// THE RAILS ARE WALLS AND THE MASTS BLOCK, on every class: walking any
    /// way for seconds never takes the body off the deck.
    #[test]
    fn ninguem_sai_do_conves_andando() {
        for item in [shared::item_id::NAVIO_CHALUPA, shared::item_id::NAVIO_ESCUNA, shared::item_id::NAVIO_GALEAO, shared::item_id::NAVIO_NAU] {
            let (mut w, sid, _rx) = mundo();
            com_casco(&mut w, sid, item);
            w.step(shared::TICK_DT);
            w.handle_navio(sid, PedidoNavio::Leme { pegar: false });
            let classe = nv::classe(item);
            for k in 0..8 {
                let a = k as f32 / 8.0 * std::f32::consts::TAU;
                anda(&mut w, sid, Vec2::new(a.cos(), a.sin()), 1.5);
                let c = conves(&w, sid);
                assert!(cv::cabe(classe, c.local), "class {classe}: off the deck at {}", c.local);
                let rumo = w.ecs.get::<&NavioEnt>(c.navio).unwrap().rumo;
                let l = cv::para_local(w.pos_do_casco(sid).unwrap(), rumo, corpo(&w, sid));
                assert!(l.x.abs() <= cv::meia_boca(classe) && l.y.abs() <= cv::meio_comp(classe), "class {classe}: body off the hull {l}");
            }
            let mastro = cv::mastros(classe).next().unwrap();
            let mut p = mastro - Vec2::new(0.0, 1.2);
            for _ in 0..60 {
                p = cv::mover(classe, p, Vec2::new(0.0, 0.05));
            }
            assert!(p.distance(mastro) >= cv::RAIO_DO_MASTRO + cv::RAIO_DO_CORPO - 1e-3);
        }
    }

    /// THE HELM: at it the stick steers the ship and the body stays at the
    /// wheel; off it the stick walks the body and the ship comes to a stop;
    /// it can only be taken near the wheel, and only by the captain.
    #[test]
    fn o_leme_pega_e_solta() {
        let (mut w, sid, _rx) = mundo();
        w.step(shared::TICK_DT);
        let antes = w.pos_do_casco(sid).unwrap();
        anda(&mut w, sid, Vec2::new(0.0, 1.0), 1.5);
        let c = conves(&w, sid);
        assert!(w.pos_do_casco(sid).unwrap().distance(antes) > 4.0, "the helm did not sail");
        assert_eq!(c.local, cv::leme(0), "the helmsman left the wheel");
        w.handle_navio(sid, PedidoNavio::Leme { pegar: false });
        let local = conves(&w, sid).local;
        anda(&mut w, sid, Vec2::new(0.0, 1.0), 3.0);
        let c = conves(&w, sid);
        assert!(c.local.distance(local) > 1.0, "off the helm the stick should walk the body");
        assert!(w.vel_do_casco(sid).length() < 0.05, "with no route the ship should lie still");
        w.handle_navio(sid, PedidoNavio::Leme { pegar: true });
        let longe = conves(&w, sid).local.distance(cv::roda(0)) > cv::ALCANCE_DO_LEME;
        assert_eq!(conves(&w, sid).leme, !longe);
        w.sessions.get_mut(&sid).unwrap().conves.as_mut().unwrap().local = cv::leme(0);
        w.handle_navio(sid, PedidoNavio::Leme { pegar: true });
        assert!(conves(&w, sid).leme);
    }

    /// OFF THE HELM THE SHIP FOLLOWS THE ROUTE AND STOPS AT ITS END.
    #[test]
    fn fora_do_leme_o_navio_segue_a_rota_e_para() {
        let (mut w, sid, _rx) = mundo();
        w.step(shared::TICK_DT);
        let de = w.pos_do_casco(sid).unwrap();
        let para = (0..32)
            .map(|k| {
                let a = k as f32 / 32.0 * std::f32::consts::TAU;
                de + Vec2::new(a.cos(), a.sin()) * 40.0
            })
            .find(|p| super::super::navio::caminho_no_mar(de, *p).is_some_and(|r| r.len() <= 2))
            .expect("open water around the nest");
        w.handle_mover_para(sid, para);
        w.handle_navio(sid, PedidoNavio::Leme { pegar: false });
        anda(&mut w, sid, Vec2::ZERO, 12.0);
        let fim = w.pos_do_casco(sid).unwrap();
        assert!(fim.distance(para) < 4.0, "the ship stopped at {fim}, {} from its destination", fim.distance(para));
        assert!(w.vel_do_casco(sid).length() < 0.2, "it did not stop: {}", w.vel_do_casco(sid));
        assert!(!conves(&w, sid).leme);
    }

    /// REGRESSION (owner, 17/11/2026: "leaving the helm stops the ship"):
    /// with a course set, letting go of the wheel and WALKING the deck with
    /// the stick (input through the network handler, which used to clear the
    /// route on any stick) — and a `PararRota` from the land's auto — leave
    /// the ship sailing its route; it stops only at the end.
    #[test]
    fn sair_do_leme_e_andar_nao_para_o_navio() {
        let (mut w, sid, _rx) = mundo();
        w.step(shared::TICK_DT);
        let de = w.pos_do_casco(sid).unwrap();
        let para = (0..32)
            .map(|k| {
                let a = k as f32 / 32.0 * std::f32::consts::TAU;
                de + Vec2::new(a.cos(), a.sin()) * 60.0
            })
            .find(|p| super::super::navio::caminho_no_mar(de, *p).is_some_and(|r| r.len() <= 2))
            .expect("open water around the nest");
        w.handle_mover_para(sid, para);
        // Under way at the helm first.
        for k in 0..30u32 {
            w.on_message(sid, ClientMessage::Input { input: InputFrame { seq: k, tick: k, move_dir: Vec2::ZERO, aim: Vec2::ZERO, buttons: 0 } });
            w.step(shared::TICK_DT);
        }
        w.handle_navio(sid, PedidoNavio::Leme { pegar: false });
        w.on_message(sid, ClientMessage::PararRota);
        let mut parado_no_meio = 0;
        let mut fim = de;
        for k in 30..(30 + 14 * 30u32) {
            // Walking about on deck the whole way.
            let a = k as f32 * 0.05;
            w.on_message(sid, ClientMessage::Input { input: InputFrame { seq: k, tick: k, move_dir: Vec2::new(a.cos(), a.sin()), aim: Vec2::ZERO, buttons: 0 } });
            w.step(shared::TICK_DT);
            fim = w.pos_do_casco(sid).unwrap();
            if fim.distance(para) > 6.0 && w.vel_do_casco(sid).length() < 0.5 && k > 60 {
                parado_no_meio += 1;
            }
        }
        assert!(!conves(&w, sid).leme);
        assert_eq!(parado_no_meio, 0, "the ship stopped before the end of its route");
        assert!(fim.distance(para) < 4.0, "the ship ended {} from its destination", fim.distance(para));
        assert!(w.vel_do_casco(sid).length() < 0.2, "it did not stop at the end");
    }

    /// PETS STAY HOME: at sea no pet body exists for a player with one.
    #[test]
    fn no_mar_o_pet_fica_guardado() {
        let (mut w, sid, _rx) = mundo();
        let id = shared::item_id::pet_no_grau(shared::item_id::PET_BASE, 2);
        w.sessions.get_mut(&sid).unwrap().equipment.pet = Some(id);
        for _ in 0..5 {
            w.step(shared::TICK_DT);
        }
        assert!(conves(&w, sid).capitao == sid);
        assert_eq!(w.ecs.query::<&super::super::pets::PetTag>().iter().count(), 0, "a pet walks on the water");
        // Ashore (another zone), it comes back.
        w.zona = "ilha_inicial".into();
        w.step(shared::TICK_DT);
        assert_eq!(w.ecs.query::<&super::super::pets::PetTag>().iter().count(), 1, "the pet did not come back on land");
    }

    /// MELEE FROM THE RAIL HITS A SURFACED MOB.
    #[test]
    fn o_golpe_da_amurada_acerta_o_bicho() {
        let (mut w, sid, _rx) = mundo();
        w.step(shared::TICK_DT);
        w.handle_navio(sid, PedidoNavio::Leme { pegar: false });
        let c = conves(&w, sid);
        let hw = cv::meia_largura_andavel(0, 0.0).unwrap();
        w.sessions.get_mut(&sid).unwrap().conves.as_mut().unwrap().local = Vec2::new(hw, 0.0);
        w.por_no_conves(sid);
        let rumo = w.ecs.get::<&NavioEnt>(c.navio).unwrap().rumo;
        let centro = w.pos_do_casco(sid).unwrap();
        let onde = cv::do_mundo(centro, rumo, Vec2::new(cv::meia_boca(0) + 0.6, 0.0));
        w.place_enemy(onde, super::super::navio::TUBARAO, 0.0);
        let mut bicho = None;
        for (_, (n, t, h)) in w.ecs.query_mut::<(&NetId, &mut EnemyTag, &mut Health)>() {
            *h = Health { current: 5_000, max: 5_000 };
            t.dead = false;
            t.spawn_grace_until = 0.0;
            t.stunned_until = 1e9;
            bicho = Some(n.0);
        }
        let bicho = bicho.unwrap();
        w.sessions.get_mut(&sid).unwrap().target = Some(bicho);
        anda(&mut w, sid, Vec2::ZERO, 2.0);
        let vida = w.ecs.query::<(&NetId, &Health)>().iter().find(|(_, (n, _))| n.0 == bicho).map(|(_, (_, h))| h.current).unwrap();
        assert!(vida < 5_000, "the sword from the rail never landed");
        assert_eq!(w.navio_equipado(sid).unwrap().1.casco, nv::casco_max(shared::item_id::NAVIO_CHALUPA));
    }

    fn em_grupo(w: &mut GameWorld, a: SessionId, b: SessionId) {
        w.sessions.get_mut(&a).unwrap().party_id = Some(9);
        w.sessions.get_mut(&b).unwrap().party_id = Some(9);
    }

    /// CREW BOARD, WALK, AND COME ASHORE WITH THE CAPTAIN WHEN IT DOCKS.
    #[test]
    fn a_tripulacao_embarca_e_desce_no_cais() {
        let (mut w, sid, mut rx) = mundo();
        let cais = shared::mar::vista_da_zona("ilha_inicial").unwrap().fundeadouro();
        w.por_navio_em(sid, cais);
        let (b, mut rx_b) = marujo(&mut w, 47_471, 4_701, "Grumete", cais + Vec2::new(6.0, 0.0));
        w.step(shared::TICK_DT);
        let navio_a = conves(&w, sid).navio;
        let navio_b = conves(&w, b).navio;
        assert_ne!(navio_a, navio_b, "each sails out on their own ship");
        let eid_a = conves(&w, sid).navio_eid;
        let capitao = w.sessions[&sid].entity_id;
        w.handle_navio(b, PedidoNavio::Embarcar { capitao });
        assert_eq!(conves(&w, b).navio, navio_b, "boarded without a party");
        em_grupo(&mut w, sid, b);
        w.handle_navio(b, PedidoNavio::Embarcar { capitao });
        let c = conves(&w, b);
        assert_eq!((c.navio, c.capitao, c.navio_eid), (navio_a, sid, eid_a));
        assert!(!w.ecs.contains(navio_b), "the crew's own ship is stowed");
        anda(&mut w, b, Vec2::new(1.0, 0.0), 0.5);
        assert!(cv::cabe(0, conves(&w, b).local));
        w.handle_navio(b, PedidoNavio::Leme { pegar: true });
        assert!(!conves(&w, b).leme);
        w.handle_navio(sid, PedidoNavio::Atracar { ilha: 0 });
        assert_eq!(trocou(&mut rx).as_deref(), Some("ilha_inicial"));
        assert_eq!(trocou(&mut rx_b).as_deref(), Some("ilha_inicial"), "the crew stayed at sea");
    }

    /// SINKING SENDS EVERYONE ABOARD TO THE DOCK.
    #[test]
    fn afundar_leva_todo_mundo_ao_cais() {
        let (mut w, sid, mut rx) = mundo();
        let (b, mut rx_b) = marujo(&mut w, 47_472, 4_702, "Grumete", shared::mar::ninhos()[0] + Vec2::new(5.0, 0.0));
        w.step(shared::TICK_DT);
        em_grupo(&mut w, sid, b);
        let capitao = w.sessions[&sid].entity_id;
        w.handle_navio(b, PedidoNavio::Embarcar { capitao });
        assert_eq!(conves(&w, b).capitao, sid);
        w.dano_no_casco(sid, 100_000, None);
        assert!(w.navio_equipado(sid).unwrap().1.avariado);
        assert!(trocou(&mut rx).is_some(), "the captain stayed at sea");
        assert!(trocou(&mut rx_b).is_some(), "the crew stayed at sea");
    }

    /// A CAPTAIN WHO LEAVES drops the crew at the nearest dock.
    #[test]
    fn sem_o_capitao_a_tripulacao_vai_ao_cais() {
        let (mut w, sid, _rx) = mundo();
        let (b, mut rx_b) = marujo(&mut w, 47_473, 4_703, "Grumete", shared::mar::ninhos()[0] + Vec2::new(5.0, 0.0));
        w.step(shared::TICK_DT);
        em_grupo(&mut w, sid, b);
        let capitao = w.sessions[&sid].entity_id;
        w.handle_navio(b, PedidoNavio::Embarcar { capitao });
        w.sessions.get_mut(&sid).unwrap().logged_in = false;
        w.step(shared::TICK_DT);
        assert!(trocou(&mut rx_b).is_some(), "the crew was left at sea without a ship");
    }

    /// RECONNECT: a character saved at sea comes back on its own deck.
    #[test]
    fn ao_reconectar_volta_ao_conves() {
        let (mut w, sid, _rx) = mundo();
        w.step(shared::TICK_DT);
        anda(&mut w, sid, Vec2::new(0.0, 1.0), 1.0);
        let salvo = corpo(&w, sid);
        let (mut w2, sid2, _rx2) = mundo();
        w2.por_navio_em(sid2, salvo);
        w2.step(shared::TICK_DT);
        let c = conves(&w2, sid2);
        assert!(c.leme && c.capitao == sid2);
        let p = w2.pos_do_casco(sid2).unwrap();
        assert!(p.distance(salvo) < 3.0);
        assert!(shared::mar::navegavel(p.x, p.y));
    }

    /// THE WIRE: a body on deck sends its STEP's velocity, never the ship's
    /// added (the i8 holds), and the deck rows go out only when they change.
    #[test]
    fn o_orcamento_do_fio_aguenta() {
        let (mut w, sid, mut rx) = mundo();
        com_casco(&mut w, sid, shared::item_id::NAVIO_NAU);
        w.step(shared::TICK_DT);
        anda(&mut w, sid, Vec2::new(0.0, 1.0), 4.0);
        w.handle_navio(sid, PedidoNavio::Leme { pegar: false });
        while rx.try_recv().is_ok() {}
        let e = w.sessions[&sid].entity.unwrap();
        for k in 0..30 {
            w.sessions.get_mut(&sid).unwrap().pending_input =
                Some(InputFrame { seq: k, tick: k, move_dir: Vec2::new(0.0, -1.0), aim: Vec2::ZERO, buttons: buttons::SPRINT });
            w.step(shared::TICK_DT);
            let v = w.ecs.get::<&Velocity>(e).unwrap().0;
            assert!(v.length() <= nv::TETO_DO_FIO, "the body's wire velocity {v} passes the i8");
        }
        let mut linhas = 0;
        while let Ok(m) = rx.try_recv() {
            if let ServerMessage::Conves { lista } = m {
                linhas += lista.len();
            }
        }
        assert!(linhas > 0 && linhas <= 30, "deck rows: {linhas} in 30 ticks for one walker");
        anda(&mut w, sid, Vec2::ZERO, 0.3);
        while rx.try_recv().is_ok() {}
        anda(&mut w, sid, Vec2::ZERO, 0.5);
        let mut parado = 0;
        while let Ok(m) = rx.try_recv() {
            if let ServerMessage::Conves { lista } = m {
                parado += lista.len();
            }
        }
        assert_eq!(parado, 0, "a still deck keeps sending rows");
    }
}
