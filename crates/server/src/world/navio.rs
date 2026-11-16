//! THE SHIP AND THE OPEN SEA, server side (docs/NAVEGACAO.md, phase 1).
//!
//! Navigating is walking, only on water: there is no boat entity. At sea
//! every body moves with `terreno::mover_casco` (land is the wall), the
//! player's speed is the ship's, and the client draws a hull instead of the
//! body. What this file adds is the ship as an ITEM (its hull lives in the
//! instance), the Captain's "sail" and "repair", the "dock" at sea, the hull
//! taking the sea mobs' bites, and what there is to do out there.
//!
//! The September sea (removed in `95c5d3a`) failed because sailing was
//! MANDATORY. This one is optional: the Captain still takes you anywhere,
//! for copper, and the story's own trips stay free.

use super::*;
use shared::item_id::{self, NAVIO_CHALUPA};
use shared::navio::{self as nv, EstadoNavio, PedidoNavio};
use shared::EquipSlot;

/// A drifting chest of the open sea (`EntityKind::Npc(7)`, like the
/// treasure chests, so the client already draws it).
pub struct BauDoMarTag;

/// Seconds a looted drifting chest takes to come back.
pub const RESPAWN_DO_BAU_S: f32 = 180.0;

/// Zone ids of the sea's mob nests. At or above `ZONA_DE_PRAIA_ID`, so the
/// mapfile/forte paths leave them alone; the kind comes from `kind_do_mar`.
pub const ZONA_DO_MAR_ID: u32 = 30_000;

/// The sea's mobs reuse Abyssia's: the Reef Shark and the Tiger Eel.
pub const TUBARAO: u16 = 70;
pub const ENGUIA: u16 = 72;

/// Where someone at sea with nowhere better lands: the Bosque's anchorage.
pub fn fundeadouro_padrao() -> Vec2 {
    shared::mar::vista_da_ilha(0).map_or(Vec2::ZERO, |v| v.fundeadouro())
}

/// Which sea mob a nest slot gets: sharks near the start, eels mixed in
/// further out.
pub(crate) fn kind_do_mar(nivel: u32, semente: u64) -> u16 {
    let um_em = if nivel >= 30 { 2 } else { 4 };
    if semente % um_em == 0 {
        ENGUIA
    } else {
        TUBARAO
    }
}

/// Does this character have a ship ANYWHERE it keeps things: the ship slot,
/// the bag, the bank or the Mail?
pub(crate) fn tem_navio(s: &Session) -> bool {
    s.equipment.navio.is_some_and(item_id::e_navio)
        || s.inventory.iter().chain(s.vault.iter()).any(|x| x.qty > 0 && item_id::e_navio(x.item_id))
        || s.dungeon.correio.iter().any(|c| item_id::e_navio(c.item_id))
}

/// THE SLOOP from the story (`historia::PASSO_DA_CHALUPA`): into the ship
/// slot if it is free, else the bag, else the Mail. `false` = already had
/// a ship, nothing given.
pub(crate) fn dar_chalupa(s: &mut Session) -> bool {
    if tem_navio(s) {
        return false;
    }
    let inst = nv::instancia_nova(NAVIO_CHALUPA);
    if s.equipment.navio.is_none() {
        s.equipment.set(EquipSlot::Navio, Some(NAVIO_CHALUPA), Some(inst));
        s.stats_dirty = true;
    } else if add_to_inventory(&mut s.inventory, NAVIO_CHALUPA, 1, Some(inst)) {
        s.inventory_dirty = true;
    } else {
        let agora = (now_ms() / 1000) as i64;
        s.dungeon.postar(NAVIO_CHALUPA, 1, Some(inst), nv::MOTIVO_CORREIO_NAVIO, agora);
    }
    true
}

impl GameWorld {
    /// Is this process the open sea?
    pub(crate) fn no_mar(&self) -> bool {
        shared::mar::e_mar(&self.zona)
    }

    /// LOGIN FALLBACK: the character was saved at sea (`z`) and the sea is
    /// down. The island they sailed from, if it is not this one.
    pub(crate) fn volta_do_mar(&self, zona_volta: &str, z: &str) -> Option<String> {
        (shared::mar::e_mar(z) && !zona_volta.is_empty() && zona_volta != self.zona && zona_volta != z)
            .then(|| zona_volta.to_string())
    }

    /// The nearest navigable spot to `p`, or an anchorage.
    pub(crate) fn agua_no_mar(&self, p: Vec2) -> Vec2 {
        if shared::mar::navegavel(p.x, p.y) {
            return p;
        }
        // A small spiral first (a body nudged off the water), then the
        // nearest anchorage, which is always water.
        for anel in 1..=12 {
            let r = anel as f32 * 2.0;
            for k in 0..16 {
                let a = k as f32 / 16.0 * std::f32::consts::TAU;
                let q = p + Vec2::new(a.cos(), a.sin()) * r;
                if shared::mar::navegavel(q.x, q.y) {
                    return q;
                }
            }
        }
        shared::mar::ilhas()
            .iter()
            .map(|i| i.fundeadouro())
            .min_by(|a, b| a.distance(p).total_cmp(&b.distance(p)))
            .unwrap_or_else(fundeadouro_padrao)
    }

    /// The ship in the ship slot and its state.
    pub(crate) fn navio_equipado(&self, sid: SessionId) -> Option<(u16, shared::items::NavioData)> {
        let s = self.sessions.get(&sid)?;
        let id = s.equipment.navio.filter(|i| item_id::e_navio(*i))?;
        Some((id, nv::dados(s.equipment.navio_inst.as_ref(), id)))
    }

    /// Writes the ship's state back into its instance.
    pub(crate) fn guarda_navio(&mut self, sid: SessionId, d: shared::items::NavioData) {
        let Some(s) = self.sessions.get_mut(&sid) else {
            return;
        };
        let Some(id) = s.equipment.navio else {
            return;
        };
        let mut inst = s.equipment.navio_inst.unwrap_or_else(|| nv::instancia_nova(id));
        inst.navio = Some(d);
        s.equipment.set(EquipSlot::Navio, Some(id), Some(inst));
        s.stats_dirty = true;
        self.save_pending = true;
    }

    pub(crate) fn envia_estado_navio(&self, sid: SessionId) {
        let Some(s) = self.sessions.get(&sid) else {
            return;
        };
        let zarpar = shared::mar::vista_da_zona(&self.zona).is_some();
        let estado = EstadoNavio::de(s.equipment.navio, s.equipment.navio_inst.as_ref(), zarpar);
        let _ = s.handle.to_client.send(ServerMessage::Navio { estado });
    }

    /// Everything the ship is asked (`ClientMessage::Navio`).
    pub(crate) fn handle_navio(&mut self, sid: SessionId, pedido: PedidoNavio) {
        let Some(s) = self.sessions.get(&sid) else {
            return;
        };
        if !s.logged_in {
            return;
        }
        match pedido {
            PedidoNavio::Estado => self.envia_estado_navio(sid),
            PedidoNavio::Zarpar => self.zarpar(sid),
            PedidoNavio::Atracar { ilha } => self.atracar(sid, ilha as usize),
            PedidoNavio::Reparar => self.reparar_navio(sid),
        }
    }

    fn perto_do_capitao_do_porto(&self, sid: SessionId) -> bool {
        self.quem_viaja_perto(sid) == Some(shared::viagem::Meio::Capitao)
    }

    /// SAIL: from the Captain's dock to the anchorage off this island on
    /// the open sea.
    fn zarpar(&mut self, sid: SessionId) {
        if self.tutorial_mode || self.dungeon_mode || self.no_mar() {
            return;
        }
        let Some(s) = self.sessions.get(&sid) else {
            return;
        };
        if s.instancia != 0 || s.downed {
            return;
        }
        if !self.perto_do_capitao_do_porto(sid) {
            self.avisa_missao(sid, "Talk to the Harbour Captain at the dock to set sail.".into());
            return;
        }
        let Some(vista) = shared::mar::vista_da_zona(&self.zona) else {
            self.avisa_missao(sid, "This island has no dock on the open sea.".into());
            return;
        };
        let Some((_, d)) = self.navio_equipado(sid) else {
            self.avisa_missao(sid, "You have no ship. Put one in the Ship slot of your equipment.".into());
            return;
        };
        if !nv::pode_zarpar(&d) {
            self.avisa_missao(sid, "Your ship is damaged. The Harbour Captain can repair it.".into());
            return;
        }
        let volta = self.zona.clone();
        if self.mandar_para_zona(
            sid,
            shared::mar::ZONA,
            vista.fundeadouro(),
            Some("You cast off and take to the open sea."),
            Some("the Open Sea"),
        ) {
            // The way home if the sea's process falls (`volta_do_mar`).
            // Written in the same tick as the handoff's save.
            if let Some(s) = self.sessions.get_mut(&sid) {
                s.zona_volta = volta;
            }
        }
    }

    /// The port of island `ilha`, in that island's coordinates: where a
    /// ship that docks puts its captain ashore.
    fn porto_da_ilha(ilha: usize) -> Option<(&'static shared::terreno::DefIlha, Vec2)> {
        let dest = shared::terreno::ARQUIPELAGO.get(ilha)?;
        let ger = shared::terreno::Gerador::da_ilha(dest);
        let chegada = ger
            .porto()
            .map(|p| p.centro)
            .or_else(|| ger.cidade().map(|c| c.centro()))
            .unwrap_or(Vec2::ZERO);
        Some((dest, chegada))
    }

    /// DOCK: at sea, at island `ilha`'s anchorage, go ashore at its port.
    fn atracar(&mut self, sid: SessionId, ilha: usize) {
        if !self.no_mar() {
            return;
        }
        let Some(eu) = self.pos_do_jogador(sid) else {
            return;
        };
        if shared::mar::cais_perto(eu).map(|v| v.ilha) != Some(ilha) {
            self.avisa_missao(sid, "Sail closer to the dock to come ashore.".into());
            return;
        }
        let indice = self
            .sessions
            .get(&sid)
            .and_then(|s| crate::quests::indice_da_historia(&s.quests));
        let Some((dest, chegada)) = Self::porto_da_ilha(ilha) else {
            return;
        };
        // The story still opens the islands, by sea as by the Captain.
        if !shared::viagem::liberada(ilha, indice) {
            self.avisa_missao(sid, format!("The story has not taken you to {} yet.", dest.nome));
            return;
        }
        let aviso = format!("You dock at {}.", dest.nome);
        if self.mandar_para_zona(sid, dest.zona, chegada, Some(&aviso), Some(dest.nome)) {
            if let Some(s) = self.sessions.get_mut(&sid) {
                s.zona_volta.clear();
            }
        }
    }

    /// REPAIR at the Captain: the whole hull, for copper.
    fn reparar_navio(&mut self, sid: SessionId) {
        if !self.perto_do_capitao_do_porto(sid) {
            self.avisa_missao(sid, "The Harbour Captain repairs ships at the dock.".into());
            return;
        }
        let Some((id, mut d)) = self.navio_equipado(sid) else {
            self.avisa_missao(sid, "You have no ship. Put one in the Ship slot of your equipment.".into());
            return;
        };
        let custo = nv::custo_do_reparo(id, &d);
        if custo == 0 && !d.avariado {
            self.avisa_missao(sid, "Your hull is already sound.".into());
            return;
        }
        let tem = self
            .sessions
            .get(&sid)
            .map_or(0, |s| crate::craft::tem(&s.inventory, item_id::COPPER));
        if tem < custo {
            self.avisa_missao(sid, format!("The repair costs {custo} copper: {} copper short.", custo - tem));
            return;
        }
        if let Some(s) = self.sessions.get_mut(&sid) {
            crate::craft::consumir(&mut s.inventory, item_id::COPPER, custo);
            s.inventory_dirty = true;
            crate::telemetria::conta_de(&s.name, "cobre_reparo_navio", id, custo as i64);
        }
        d.casco = nv::casco_max(id);
        d.avariado = false;
        self.guarda_navio(sid, d);
        self.envia_estado_navio(sid);
        self.send_chat_to(sid, &format!("Hull repaired for {custo} copper."));
    }

    /// The hull takes a bite. At zero the ship is damaged and the player is
    /// put back at the dock they sailed from (`naufragio`).
    pub(crate) fn dano_no_casco(&mut self, sid: SessionId, dmg: i32) {
        let Some((id, mut d)) = self.navio_equipado(sid) else {
            // At sea without a ship (unequipped out there): nothing holds
            // them up, so the sea washes them ashore.
            self.naufragio(sid);
            return;
        };
        if d.avariado {
            self.naufragio(sid);
            return;
        }
        let dmg = dmg.max(0) as u16;
        let antes = d.casco;
        d.casco = d.casco.saturating_sub(dmg);
        let limiar = (nv::casco_max(id) as f32 * nv::CRITICO) as u16;
        if antes > limiar && d.casco <= limiar && d.casco > 0 {
            self.avisa_missao(sid, "Hull critical: head back to a dock.".into());
        }
        let afundou = d.casco == 0;
        if afundou {
            d.avariado = true;
        }
        self.guarda_navio(sid, d);
        if afundou {
            self.naufragio(sid);
        }
    }

    /// THE HULL GAVE WAY: back to the dock this character sailed from (or
    /// the nearest island that is up). If no island is up, they stay at
    /// sea, at the nearest anchorage — a `TrocarZona` to a dead host is the
    /// one outcome that cannot happen.
    pub(crate) fn naufragio(&mut self, sid: SessionId) {
        let Some(eu) = self.pos_do_jogador(sid) else {
            return;
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
        let indice = self
            .sessions
            .get(&sid)
            .and_then(|s| crate::quests::indice_da_historia(&s.quests));
        for i in ordem {
            if !shared::viagem::liberada(i, indice) {
                continue;
            }
            let Some((dest, chegada)) = Self::porto_da_ilha(i) else {
                continue;
            };
            if self.mandar_para_zona(
                sid,
                dest.zona,
                chegada,
                Some("Your hull gave way. You wash up at the dock: the Harbour Captain can repair the ship."),
                Some(dest.nome),
            ) {
                if let Some(s) = self.sessions.get_mut(&sid) {
                    s.zona_volta.clear();
                }
                return;
            }
        }
        let porto = self.agua_no_mar(fundeadouro_padrao());
        if let Some(e) = self.sessions.get(&sid).and_then(|s| s.entity) {
            if let Ok(mut p) = self.ecs.get::<&mut Position>(e) {
                p.0 = porto;
            }
        }
        self.avisa_missao(sid, "Your hull gave way, and no port is open: you drift to an anchorage.".into());
    }

    /// The sea's tick: the bites of this tick on the hulls, and the chests
    /// that come back.
    pub(crate) fn tick_mar(&mut self) {
        if !self.no_mar() {
            self.mordidas_no_casco.clear();
            return;
        }
        let mordidas = std::mem::take(&mut self.mordidas_no_casco);
        for (eid, dmg) in mordidas {
            let Some(sid) = self.sessions.values().find(|s| s.entity_id == eid).map(|s| s.handle.id) else {
                continue;
            };
            if self.pos_do_jogador(sid).is_some_and(shared::mar::no_corredor) {
                continue;
            }
            self.dano_no_casco(sid, dmg);
        }
        let agora = self.sim_time_s;
        let (prontos, esperando): (Vec<_>, Vec<_>) =
            std::mem::take(&mut self.baus_do_mar).into_iter().partition(|(_, t)| *t <= agora);
        self.baus_do_mar = esperando;
        for (p, _) in prontos {
            self.poe_bau_do_mar(p);
        }
    }

    fn poe_bau_do_mar(&mut self, p: Vec2) {
        let eid = self.alloc_entity_id();
        self.ecs.spawn((NetId(eid), Position(p), Velocity(Vec2::ZERO), EntityKind::Npc(7), BauDoMarTag));
    }

    /// Peoples THE OPEN SEA: mob nests along the routes and the drifting
    /// chests. The sibling of `povoar_ilha`; on the sea the places come from
    /// the routes, because the routes are the map.
    pub fn povoar_mar(&mut self) {
        const POR_NINHO: usize = 3;
        const RAIO: f32 = 9.0;
        let mut zonas = Vec::new();
        for (k, centro) in shared::mar::ninhos().into_iter().enumerate() {
            let mut slots: Vec<SpawnSlot> = Vec::new();
            for j in 0..POR_NINHO * 4 {
                if slots.len() >= POR_NINHO {
                    break;
                }
                let a = j as f32 / (POR_NINHO * 4) as f32 * std::f32::consts::TAU;
                let p = centro + Vec2::new(a.cos(), a.sin()) * RAIO;
                if !shared::mar::agua_livre(p) || slots.iter().any(|s| s.pos.distance(p) < 5.0) {
                    continue;
                }
                slots.push(SpawnSlot { pos: p, occupant: None, respawn_at: 0.0 });
            }
            if slots.is_empty() {
                continue;
            }
            let nivel = shared::mar::nivel_em(centro);
            let n = slots.len() as u32;
            zonas.push(ServerSpawnZone {
                id: ZONA_DO_MAR_ID + k as u32,
                origin: centro - Vec2::splat(RAIO * 2.0),
                size: Vec2::splat(RAIO * 4.0),
                respawn_delay_s: 30.0,
                quotas: Vec::new(),
                live: Vec::new(),
                respawn_queue: Vec::new(),
                polygon: None,
                level_range: Some((nivel, (nivel + 2).min(60), n)),
                level_range_live: 0,
                level_range_queue: (0..n).map(|_| 0.0_f32).collect(),
                slots,
                active: false,
                last_player_near_at: -1e9,
                forte: false,
            });
        }
        let baus = shared::mar::baus();
        tracing::info!("mar aberto: {} ninhos, {} baus a deriva", zonas.len(), baus.len());
        self.spawn_zones = zonas;
        self.boss_areas.clear();
        self.vagas_de_chefe.clear();
        for p in baus {
            self.poe_bau_do_mar(p);
        }
    }

    /// Opens a drifting chest: copper, Darksteel and a material of the
    /// stretch's level. Material, never gear: the forge and the dungeon own
    /// gear. Whoever gets there first takes it; it comes back later.
    pub(crate) fn saquear_bau_do_mar(&mut self, sid: SessionId, entidade: hecs::Entity) {
        let Some(pos) = self.ecs.get::<&Position>(entidade).ok().map(|p| p.0) else {
            return;
        };
        let Some(eu) = self.pos_do_jogador(sid) else {
            return;
        };
        if eu.distance(pos) > 6.0 {
            self.avisa_missao(sid, "Sail closer to the chest.".into());
            return;
        }
        let nivel = shared::mar::nivel_em(pos);
        let cor = (1 + nivel / 15).min(4) as u8;
        let cobre = 60 + 6 * nivel;
        let darksteel = 10 + 2 * nivel;
        let premios = [
            (item_id::DARKSTEEL, darksteel),
            (item_id::na_cor(item_id::STEEL, cor), 2 + nivel / 15),
            (madeira_da_cor(cor), 3 + nivel / 10),
        ];
        let agora = (now_ms() / 1000) as i64;
        let Some(s) = self.sessions.get_mut(&sid) else {
            return;
        };
        Self::pagar_em_cobre(s, cobre, "bau_do_mar");
        for (id, qtd) in premios {
            if !add_to_inventory(&mut s.inventory, id, qtd, None) {
                s.dungeon.postar(id, qtd, None, 0, agora);
            }
        }
        s.inventory_dirty = true;
        let _ = s.handle.to_client.send(ServerMessage::Chat {
            from: "SYS".into(),
            text: format!("Drifting chest: {cobre} copper, {darksteel} Darksteel and materials."),
        });
        let eid = self.ecs.get::<&NetId>(entidade).map(|n| n.0).ok();
        let _ = self.ecs.despawn(entidade);
        if let Some(eid) = eid {
            self.removed_this_tick.push(eid);
        }
        self.baus_do_mar.push((pos, self.sim_time_s + RESPAWN_DO_BAU_S));
        self.save_pending = true;
    }
}

/// The wood of a colour band (T1..T4).
fn madeira_da_cor(cor: u8) -> u16 {
    match cor {
        1 => item_id::WOOD_T1,
        2 => item_id::WOOD_T2,
        3 => item_id::WOOD_T3,
        _ => item_id::WOOD_T4,
    }
}

#[cfg(test)]
mod testes {
    use super::*;
    use shared::navio::PedidoNavio;

    const CAPITAO: EntityId = EntityId(7_001);

    fn mundo(zona: &str, no_ar: &[&str]) -> (GameWorld, SessionId, mpsc::UnboundedReceiver<ServerMessage>) {
        crate::economy::init_vazia_para_testes();
        let mut w = GameWorld::new(HashMap::new());
        w.zona = zona.into();
        w.diretorio = Some(crate::canais::Diretorio::para_teste(no_ar));
        let sid = SessionId(([127, 0, 0, 1], 47_470).into());
        let (tx, rx) = mpsc::unbounded_channel();
        w.on_connect(SessionHandle { id: sid, to_client: tx });
        let eid = EntityId(4_700);
        let e = w.ecs.spawn((
            NetId(eid),
            Position(Vec2::new(10.0, 10.0)),
            Velocity(Vec2::ZERO),
            EntityKind::Player,
            Health { current: 100, max: 100 },
            PlayerTag { name: "Marujo".into(), player_id: PlayerId(47), attack_anim_pending: None, combo_step_pending: None },
        ));
        let s = w.sessions.get_mut(&sid).unwrap();
        s.logged_in = true;
        s.entity = Some(e);
        s.entity_id = eid;
        s.name = "Marujo".into();
        s.inventory = vec![shared::InventorySlot::default(); shared::INVENTORY_SLOTS];
        (w, sid, rx)
    }

    /// The Harbour Captain right where the player stands.
    fn com_capitao(w: &mut GameWorld) {
        w.ecs.spawn((
            NetId(CAPITAO),
            Position(Vec2::new(10.0, 12.0)),
            EntityKind::Npc(9),
            NpcDaVilaTag {
                nome: "Harbour Captain".into(),
                rumo: shared::npc_kind(None, shared::construcao::Papel::Estaleiro as u8),
                giver: None,
            },
        ));
    }

    fn poe_em(w: &mut GameWorld, sid: SessionId, p: Vec2) {
        let e = w.sessions[&sid].entity.unwrap();
        w.ecs.get::<&mut Position>(e).unwrap().0 = p;
    }

    fn cobre(w: &mut GameWorld, sid: SessionId, n: u32) {
        let s = w.sessions.get_mut(&sid).unwrap();
        s.inventory[0] = shared::InventorySlot { item_id: item_id::COPPER, qty: n, instance: None };
    }

    fn tem_cobre(w: &GameWorld, sid: SessionId) -> u32 {
        crate::craft::tem(&w.sessions[&sid].inventory, item_id::COPPER)
    }

    fn trocou_para(rx: &mut mpsc::UnboundedReceiver<ServerMessage>) -> Option<String> {
        let mut z = None;
        while let Ok(m) = rx.try_recv() {
            if let ServerMessage::TrocarZona { zona, .. } = m {
                z = Some(zona);
            }
        }
        z
    }

    fn historia_em(w: &mut GameWorld, sid: SessionId, indice: u32) {
        let s = w.sessions.get_mut(&sid).unwrap();
        s.quests.retain(|c| c.quest_id != shared::historia::ID_MARCO);
        s.quests.push(crate::quests::CharQuest {
            quest_id: shared::historia::ID_MARCO,
            status: 0,
            progress: indice,
            cooldown_until: 0,
        });
    }

    /// SAIL AND DOCK, the round trip: sail from the Bosque's Captain to its
    /// anchorage at sea (with `zona_volta` written), then dock at the
    /// Glacier, which clears it.
    #[test]
    fn zarpar_e_atracar_ida_e_volta() {
        let (mut w, sid, mut rx) = mundo("ilha_inicial", &["ilha_inicial", shared::mar::ZONA, "ilha_gelo"]);
        com_capitao(&mut w);
        // No ship: refused.
        w.handle_navio(sid, PedidoNavio::Zarpar);
        assert_eq!(trocou_para(&mut rx), None);
        assert!(dar_chalupa(w.sessions.get_mut(&sid).unwrap()));
        w.handle_navio(sid, PedidoNavio::Zarpar);
        assert_eq!(trocou_para(&mut rx).as_deref(), Some(shared::mar::ZONA));
        assert_eq!(w.sessions[&sid].zona_volta, "ilha_inicial");
        let chegada = w.pos_do_jogador(sid).unwrap();
        let bosque = shared::mar::vista_da_zona("ilha_inicial").unwrap();
        assert!(chegada.distance(bosque.fundeadouro()) < 0.01, "spawns at the anchorage");

        // Now the sea process, same character.
        w.zona = shared::mar::ZONA.into();
        let gelo = shared::mar::vista_da_zona("ilha_gelo").unwrap();
        // Far from the Glacier's dock: refused.
        w.handle_navio(sid, PedidoNavio::Atracar { ilha: gelo.ilha as u8 });
        assert_eq!(trocou_para(&mut rx), None);
        poe_em(&mut w, sid, gelo.fundeadouro());
        // The story has not opened the Glacier: refused.
        historia_em(&mut w, sid, 0);
        w.handle_navio(sid, PedidoNavio::Atracar { ilha: gelo.ilha as u8 });
        assert_eq!(trocou_para(&mut rx), None);
        let (passo, _) = shared::viagem::passo_que_libera(gelo.ilha).unwrap();
        historia_em(&mut w, sid, passo + 1);
        w.handle_navio(sid, PedidoNavio::Atracar { ilha: gelo.ilha as u8 });
        assert_eq!(trocou_para(&mut rx).as_deref(), Some("ilha_gelo"));
        assert!(w.sessions[&sid].zona_volta.is_empty());
    }

    /// THE CAPTAIN CHARGES COPPER, and refuses without it (nothing taken).
    #[test]
    fn o_capitao_cobra_cobre_e_recusa_sem() {
        let (mut w, sid, mut rx) = mundo("ilha_gelo", &["ilha_gelo", "ilha_inicial"]);
        com_capitao(&mut w);
        let preco = shared::navio::preco_da_viagem(0);
        cobre(&mut w, sid, preco - 1);
        w.handle_viajar(sid, 0);
        assert_eq!(trocou_para(&mut rx), None);
        assert_eq!(tem_cobre(&w, sid), preco - 1, "refused trips take nothing");
        cobre(&mut w, sid, preco + 7);
        w.handle_viajar(sid, 0);
        assert_eq!(trocou_para(&mut rx).as_deref(), Some("ilha_inicial"));
        assert_eq!(tem_cobre(&w, sid), 7);
        // The story's own trip is free.
        let (mut w2, sid2, mut rx2) = mundo("ilha_inicial", &["ilha_inicial", "ilha_gelo"]);
        com_capitao(&mut w2);
        let i = shared::historia::indice(shared::historia::PASSO_DA_CHALUPA).unwrap();
        historia_em(&mut w2, sid2, i);
        w2.sessions.get_mut(&sid2).unwrap().quests.push(crate::quests::CharQuest {
            quest_id: shared::historia::PASSO_DA_CHALUPA,
            status: shared::quests::quest_status::ACTIVE,
            progress: 0,
            cooldown_until: 0,
        });
        w2.handle_viajar(sid2, 1);
        assert_eq!(trocou_para(&mut rx2).as_deref(), Some("ilha_gelo"), "the story trip charged copper");
        // A destination that is down charges nothing.
        let (mut w, sid, mut rx) = mundo("ilha_gelo", &["ilha_gelo"]);
        com_capitao(&mut w);
        cobre(&mut w, sid, 1_000);
        w.handle_viajar(sid, 0);
        assert_eq!(trocou_para(&mut rx), None);
        assert_eq!(tem_cobre(&w, sid), 1_000);
    }

    /// The story step and the gift give EXACTLY ONE Sloop, and none to
    /// someone who already has a ship anywhere.
    #[test]
    fn a_chalupa_e_uma_so() {
        let (mut w, sid, _rx) = mundo("ilha_gelo", &[]);
        let s = w.sessions.get_mut(&sid).unwrap();
        assert!(dar_chalupa(s));
        assert!(!dar_chalupa(s), "a second one");
        assert_eq!(s.equipment.navio, Some(NAVIO_CHALUPA));
        let d = nv::dados(s.equipment.navio_inst.as_ref(), NAVIO_CHALUPA);
        assert_eq!(d.casco, nv::casco_max(NAVIO_CHALUPA));
        // Ship in the bank: none.
        s.equipment.set(EquipSlot::Navio, None, None);
        s.vault = vec![shared::InventorySlot::default(); 4];
        s.vault[2] = shared::InventorySlot { item_id: NAVIO_CHALUPA, qty: 1, instance: None };
        assert!(!dar_chalupa(s));
        // Ship in the Mail: none.
        s.vault.clear();
        s.dungeon.postar(NAVIO_CHALUPA, 1, None, nv::MOTIVO_CORREIO_NAVIO, 0);
        assert!(!dar_chalupa(s));
    }

    /// The story step gives the Sloop on completion, through the real tick.
    #[test]
    fn o_passo_da_historia_entrega_a_chalupa() {
        let (mut w, sid, _rx) = mundo("ilha_gelo", &[]);
        // The story only walks on a zone with relief.
        w.ilha = Some(shared::terreno::Ilha::da_ilha(&shared::terreno::ARQUIPELAGO[1]));
        let i = shared::historia::indice(shared::historia::PASSO_DA_CHALUPA).unwrap();
        historia_em(&mut w, sid, i);
        let s = w.sessions.get_mut(&sid).unwrap();
        s.quests.push(crate::quests::CharQuest {
            quest_id: shared::historia::PASSO_DA_CHALUPA,
            status: shared::quests::quest_status::ACTIVE,
            progress: 0,
            cooldown_until: 0,
        });
        w.tick_historia();
        w.tick_historia();
        let s = &w.sessions[&sid];
        assert_eq!(s.equipment.navio, Some(NAVIO_CHALUPA), "arriving at the Glacier gives the Sloop");
        assert_eq!(crate::quests::indice_da_historia(&s.quests), Some(i + 1));
    }

    /// HULL AT ZERO: the bites go to the hull (not the HP), and at zero the
    /// ship is damaged and the player goes back to the dock they left.
    #[test]
    fn casco_a_zero_avaria_e_volta_ao_cais() {
        let (mut w, sid, mut rx) = mundo(shared::mar::ZONA, &[shared::mar::ZONA, "ilha_inicial", "ilha_gelo"]);
        dar_chalupa(w.sessions.get_mut(&sid).unwrap());
        w.sessions.get_mut(&sid).unwrap().zona_volta = "ilha_gelo".into();
        historia_em(&mut w, sid, 10_000);
        // Out in open water, away from the corridors.
        let longe = shared::mar::ninhos()[0];
        poe_em(&mut w, sid, longe);
        let eid = w.sessions[&sid].entity_id;
        w.mordidas_no_casco.push((eid, 1_000));
        w.tick_mar();
        let (_, d) = w.navio_equipado(sid).unwrap();
        assert_eq!(d.casco, nv::casco_max(NAVIO_CHALUPA) - 1_000);
        let e = w.sessions[&sid].entity.unwrap();
        assert_eq!(w.ecs.get::<&Health>(e).unwrap().current, 100, "the player is not hurt");
        assert_eq!(trocou_para(&mut rx), None);
        w.mordidas_no_casco.push((eid, 5_000));
        w.tick_mar();
        let (_, d) = w.navio_equipado(sid).unwrap();
        assert!(d.avariado && d.casco == 0);
        assert_eq!(trocou_para(&mut rx).as_deref(), Some("ilha_gelo"), "back to the dock sailed from");
        // In a harbour corridor nothing bites.
        let (mut w, sid, _rx) = mundo(shared::mar::ZONA, &[]);
        dar_chalupa(w.sessions.get_mut(&sid).unwrap());
        poe_em(&mut w, sid, fundeadouro_padrao());
        let eid = w.sessions[&sid].entity_id;
        w.mordidas_no_casco.push((eid, 1_000));
        w.tick_mar();
        assert_eq!(w.navio_equipado(sid).unwrap().1.casco, nv::casco_max(NAVIO_CHALUPA));
    }

    /// A damaged ship cannot sail; REPAIR costs copper and refuses without.
    #[test]
    fn reparo_custa_cobre_e_navio_avariado_nao_zarpa() {
        let (mut w, sid, mut rx) = mundo("ilha_inicial", &["ilha_inicial", shared::mar::ZONA]);
        com_capitao(&mut w);
        dar_chalupa(w.sessions.get_mut(&sid).unwrap());
        w.guarda_navio(sid, shared::items::NavioData { casco: 0, avariado: true });
        w.handle_navio(sid, PedidoNavio::Zarpar);
        assert_eq!(trocou_para(&mut rx), None, "a damaged ship sailed");
        cobre(&mut w, sid, 100);
        w.handle_navio(sid, PedidoNavio::Reparar);
        assert!(w.navio_equipado(sid).unwrap().1.avariado, "repaired without paying");
        assert_eq!(tem_cobre(&w, sid), 100);
        cobre(&mut w, sid, 300);
        w.handle_navio(sid, PedidoNavio::Reparar);
        let (_, d) = w.navio_equipado(sid).unwrap();
        assert!(!d.avariado && d.casco == nv::casco_max(NAVIO_CHALUPA));
        assert_eq!(tem_cobre(&w, sid), 60, "a full Sloop repair is 240");
        w.handle_navio(sid, PedidoNavio::Zarpar);
        assert_eq!(trocou_para(&mut rx).as_deref(), Some(shared::mar::ZONA));
    }

    /// THE SEA IS DOWN: login falls back to the island sailed from.
    #[test]
    fn com_o_mar_fora_do_ar_o_login_volta_pra_ilha() {
        let (w, _sid, _rx) = mundo("ilha_inicial", &["ilha_inicial", "ilha_gelo"]);
        assert_eq!(w.volta_do_mar("ilha_gelo", shared::mar::ZONA).as_deref(), Some("ilha_gelo"));
        // Sailed from THIS island: stay here (its port).
        assert_eq!(w.volta_do_mar("ilha_inicial", shared::mar::ZONA), None);
        // Never sailed: no rule.
        assert_eq!(w.volta_do_mar("", shared::mar::ZONA), None);
        // Saved on an island, not the sea: not this rule.
        assert_eq!(w.volta_do_mar("ilha_gelo", "ilha_deserto"), None);
    }

    /// PK modes apply at sea unchanged, except the harbour corridor.
    #[test]
    fn os_modos_de_pk_valem_no_mar() {
        let (mut w, a, _rx) = mundo(shared::mar::ZONA, &[]);
        // A second player.
        let b = SessionId(([127, 0, 0, 1], 47_471).into());
        let (tx, rx2) = mpsc::unbounded_channel();
        std::mem::forget(rx2);
        w.on_connect(SessionHandle { id: b, to_client: tx });
        let eb = EntityId(4_701);
        let e = w.ecs.spawn((NetId(eb), Position(Vec2::ZERO), Velocity(Vec2::ZERO), EntityKind::Player,
            Health { current: 100, max: 100 },
            PlayerTag { name: "Pirata".into(), player_id: PlayerId(48), attack_anim_pending: None, combo_step_pending: None }));
        {
            let s = w.sessions.get_mut(&b).unwrap();
            s.logged_in = true;
            s.entity = Some(e);
            s.entity_id = eb;
            s.name = "Pirata".into();
        }
        let ea = w.sessions[&a].entity_id;
        let longe = shared::mar::ninhos()[0];
        poe_em(&mut w, a, longe);
        poe_em(&mut w, b, longe + Vec2::new(2.0, 0.0));
        let modo = |w: &mut GameWorld, sid: SessionId, m: u8| {
            let s = w.sessions.get_mut(&sid).unwrap();
            s.pk_modo = m;
            s.pk_escolhido = m;
        };
        modo(&mut w, a, shared::pk::PACIFICO);
        modo(&mut w, b, shared::pk::PACIFICO);
        let esperado = |w: &GameWorld| {
            shared::pk::pode_atacar(&w.lado_pk(&w.sessions[&a]), &w.lado_pk(&w.sessions[&b]), false)
        };
        assert_eq!(w.can_damage_player(ea, eb), esperado(&w));
        assert!(!w.can_damage_player(ea, eb), "peaceful players fight at sea");
        modo(&mut w, a, shared::pk::LIVRE);
        assert_eq!(w.can_damage_player(ea, eb), esperado(&w));
        assert!(w.can_damage_player(ea, eb), "free PK does not reach at sea");
        // The harbour corridor protects everyone.
        poe_em(&mut w, b, fundeadouro_padrao());
        assert!(!w.can_damage_player(ea, eb));
    }

    /// The sea is peopled: nests of sea mobs and chests, none in a corridor.
    #[test]
    fn o_mar_tem_ninhos_e_baus() {
        let (mut w, sid, _rx) = mundo(shared::mar::ZONA, &[]);
        w.povoar_mar();
        assert!(w.spawn_zones.len() >= 8);
        let baus: Vec<Vec2> = w.ecs.query::<(&Position, &BauDoMarTag)>().iter().map(|(_, (p, _))| p.0).collect();
        assert!(baus.len() >= 5);
        // Loot one: it pays, disappears and is queued to come back.
        let e = w.ecs.query::<&BauDoMarTag>().iter().next().map(|(e, _)| e).unwrap();
        let p = w.ecs.get::<&Position>(e).unwrap().0;
        poe_em(&mut w, sid, p);
        w.saquear_bau_do_mar(sid, e);
        assert!(tem_cobre(&w, sid) > 0);
        assert!(w.ecs.get::<&BauDoMarTag>(e).is_err());
        assert_eq!(w.baus_do_mar.len(), 1);
    }
}
