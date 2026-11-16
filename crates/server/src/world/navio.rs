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
            PedidoNavio::Construir => self.construir_casco(sid),
            PedidoNavio::Melhorar { eixo } => self.melhorar_navio(sid, eixo),
            PedidoNavio::Habilidade { id } => self.habilidade_do_navio(sid, id),
            PedidoNavio::AbrirTesouro => self.abrir_tesouro(sid),
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
        // A Colossus Chest on deck counts this island (phase 4).
        self.contar_atracacao(sid, ilha);
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
        d.casco = nv::casco_total(id, &d);
        d.avariado = false;
        self.guarda_navio(sid, d);
        self.envia_estado_navio(sid);
        self.send_chat_to(sid, &format!("Hull repaired for {custo} copper."));
    }

    /// The hull takes a bite. At zero the ship is damaged and the player is
    /// put back at the dock they sailed from (`naufragio`).
    ///
    /// Phase 4: `atacante` = the PLAYER whose cannon sank it (a PvP kill:
    /// PK points as on land, none for a marked carrier or in lawless
    /// waters, and the treasure goes to the killer); `None` = the sea (a
    /// mob): the treasure floats at the wreck. The Corsair's hull passive
    /// takes its share off the damage.
    pub(crate) fn dano_no_casco(&mut self, sid: SessionId, dmg: i32, atacante: Option<EntityId>) {
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
        let (marcado_antes, lado, vitima, casco_pct) = match self.sessions.get(&sid) {
            Some(s) => (self.marcado_de(s), self.lado_pk(s), s.entity_id, s.arvore.passivas().casco_pct),
            None => return,
        };
        let dmg = ((dmg.max(0) as f32) / (1.0 + casco_pct.max(0.0))).round().min(u16::MAX as f32) as u16;
        let antes = d.casco;
        d.casco = d.casco.saturating_sub(dmg);
        let limiar = (nv::casco_total(id, &d) as f32 * nv::CRITICO) as u16;
        if antes > limiar && d.casco <= limiar && d.casco > 0 {
            self.avisa_missao(sid, "Hull critical: head back to a dock.".into());
        }
        let afundou = d.casco == 0;
        let mut tesouro = None;
        if afundou {
            d.avariado = true;
            tesouro = d.conves.take();
        }
        self.guarda_navio(sid, d);
        if afundou {
            if let Some(a) = atacante {
                self.registrar_morte_pk(a, vitima, lado, marcado_antes);
            }
            if let Some(t) = tesouro {
                let onde = self.pos_do_jogador(sid).unwrap_or_else(fundeadouro_padrao);
                self.tesouro_perdido(sid, t, atacante, onde);
            }
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
            self.dano_no_casco(sid, dmg, None);
        }
        self.tick_tesouros();
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
        let pranchas = nv::pranchas_do_bau(nivel);
        let premios = [
            (item_id::DARKSTEEL, darksteel),
            (item_id::na_cor(item_id::STEEL, cor), 2 + nivel / 15),
            (madeira_da_cor(cor), 3 + nivel / 10),
            // The sea's own material (phase 2): the Shipwright's.
            (item_id::PRANCHA_DE_NAUFRAGIO, pranchas),
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
            text: format!("Drifting chest: {cobre} copper, {darksteel} Darksteel, {pranchas} Wreck Planks and materials."),
        });
        let eid = self.ecs.get::<&NetId>(entidade).map(|n| n.0).ok();
        let _ = self.ecs.despawn(entidade);
        if let Some(eid) = eid {
            self.removed_this_tick.push(eid);
        }
        self.baus_do_mar.push((pos, self.sim_time_s + RESPAWN_DO_BAU_S));
        self.save_pending = true;
    }

    // ───────────────────── phase 2: the Shipwright ─────────────────────

    /// The level of the character behind `sid`.
    fn nivel_de_sid(&self, sid: SessionId) -> u32 {
        self.sessions
            .get(&sid)
            .map_or(1, |s| shared::level_of_xp_with_mult(s.xp, crate::economy::xp_multiplier()))
    }

    /// BUILD THE NEXT HULL at the Shipwright (the Harbour Captain): the
    /// ship in the Ship slot is consumed with the recipe's materials, and
    /// the new class takes its place — hull full, stars and cosmetics
    /// carried over.
    fn construir_casco(&mut self, sid: SessionId) {
        if !self.perto_do_capitao_do_porto(sid) {
            self.avisa_missao(sid, "The Shipwright works at the dock: talk to the Harbour Captain.".into());
            return;
        }
        let Some((id, d)) = self.navio_equipado(sid) else {
            self.avisa_missao(sid, "You have no ship. Put one in the Ship slot of your equipment.".into());
            return;
        };
        let Some(r) = nv::receita_depois_de(id) else {
            self.avisa_missao(sid, "Your ship is already the finest hull the Shipwright builds.".into());
            return;
        };
        let Some(classe) = nv::casco(r.para) else {
            return;
        };
        let nivel = self.nivel_de_sid(sid);
        if nivel < classe.nivel {
            self.avisa_missao(
                sid,
                format!("The next hull needs level {}.", shared::progressao::texto_de_nivel(classe.nivel)),
            );
            return;
        }
        let Some(s) = self.sessions.get(&sid) else {
            return;
        };
        if r.materiais.iter().any(|(item, q)| crate::craft::tem(&s.inventory, *item) < *q) {
            self.avisa_missao(sid, "Not enough materials for the next hull.".into());
            return;
        }
        let novo = nv::reconstruido(r.para, &d);
        let s = self.sessions.get_mut(&sid).unwrap();
        for (item, q) in r.materiais {
            crate::craft::consumir(&mut s.inventory, item, q);
        }
        s.inventory_dirty = true;
        let mut inst = nv::instancia_nova(r.para);
        inst.navio = Some(novo);
        s.equipment.set(EquipSlot::Navio, Some(r.para), Some(inst));
        s.stats_dirty = true;
        crate::telemetria::conta_de(&s.name, "navio_construido", r.para, 1);
        self.save_pending = true;
        self.envia_estado_navio(sid);
        self.send_chat_to(sid, "The Shipwright launches your new hull.");
    }

    /// ONE STAR on an axis at the Shipwright: copper, Darksteel and Wreck
    /// Planks, always succeeds. The hull star raises the hull you have by
    /// what it adds (a damaged hull stays as damaged as it was).
    fn melhorar_navio(&mut self, sid: SessionId, eixo: u8) {
        if !self.perto_do_capitao_do_porto(sid) {
            self.avisa_missao(sid, "The Shipwright works at the dock: talk to the Harbour Captain.".into());
            return;
        }
        let Some(eixo) = nv::Eixo::de_u8(eixo) else {
            return;
        };
        let Some((id, mut d)) = self.navio_equipado(sid) else {
            self.avisa_missao(sid, "You have no ship. Put one in the Ship slot of your equipment.".into());
            return;
        };
        let atual = nv::estrelas(&d, eixo);
        let Some(custo) = nv::custo_da_estrela(atual) else {
            self.avisa_missao(sid, "That axis already has five stars.".into());
            return;
        };
        let Some(s) = self.sessions.get(&sid) else {
            return;
        };
        let falta = crate::craft::tem(&s.inventory, item_id::COPPER) < custo.cobre
            || crate::craft::tem(&s.inventory, item_id::DARKSTEEL) < custo.darksteel
            || crate::craft::tem(&s.inventory, item_id::PRANCHA_DE_NAUFRAGIO) < custo.pranchas;
        if falta {
            self.avisa_missao(
                sid,
                format!(
                    "The star costs {} copper, {} Darksteel and {} Wreck Planks.",
                    custo.cobre, custo.darksteel, custo.pranchas
                ),
            );
            return;
        }
        let s = self.sessions.get_mut(&sid).unwrap();
        crate::craft::consumir(&mut s.inventory, item_id::COPPER, custo.cobre);
        crate::craft::consumir(&mut s.inventory, item_id::DARKSTEEL, custo.darksteel);
        crate::craft::consumir(&mut s.inventory, item_id::PRANCHA_DE_NAUFRAGIO, custo.pranchas);
        s.inventory_dirty = true;
        crate::telemetria::conta_de(&s.name, "estrela_do_navio", eixo.nome(), 1);
        let antes = nv::casco_total(id, &d);
        d.estrelas[eixo as usize] = atual + 1;
        if eixo == nv::Eixo::Casco && !d.avariado {
            d.casco = d.casco.saturating_add(nv::casco_total(id, &d).saturating_sub(antes));
        }
        self.guarda_navio(sid, d);
        self.envia_estado_navio(sid);
        let texto = match eixo {
            nv::Eixo::Casco => format!("Hull star {} of {}.", atual + 1, nv::ESTRELAS_MAX),
            nv::Eixo::Vela => format!("Sail star {} of {}.", atual + 1, nv::ESTRELAS_MAX),
            nv::Eixo::Canhao => format!("Cannon star {} of {}.", atual + 1, nv::ESTRELAS_MAX),
        };
        self.send_chat_to(sid, &texto);
    }

    /// A SHIP COSMETIC used from the bag: the sail colour or figurehead
    /// goes on the ship in the Ship slot, and the item is spent.
    pub(crate) fn aplicar_cosmetico(&mut self, sid: SessionId, slot: usize, c: nv::Cosmetico) {
        let Some((_, mut d)) = self.navio_equipado(sid) else {
            self.avisa_missao(sid, "Put a ship in the Ship slot to dress it.".into());
            return;
        };
        let Some(s) = self.sessions.get_mut(&sid) else {
            return;
        };
        let Some(sl) = s.inventory.get_mut(slot).filter(|sl| sl.qty > 0) else {
            return;
        };
        sl.qty -= 1;
        if sl.qty == 0 {
            *sl = shared::InventorySlot::default();
        }
        s.inventory_dirty = true;
        nv::aplicar(&mut d, c);
        self.guarda_navio(sid, d);
        self.send_chat_to(sid, "Your ship has a new look.");
    }

    /// The look of every player's ship, for `EntityMeta::skins` at sea.
    pub(crate) fn visuais_dos_navios(&self) -> HashMap<EntityId, u64> {
        if !self.no_mar() {
            return HashMap::new();
        }
        self.sessions
            .values()
            .filter(|s| s.logged_in)
            .filter_map(|s| {
                let id = s.equipment.navio.filter(|i| item_id::e_navio(*i))?;
                let d = nv::dados(s.equipment.navio_inst.as_ref(), id);
                Some((s.entity_id, nv::Visual::de(id, &d).no_fio()))
            })
            .collect()
    }

    /// The ship's speed at sea for this session: class × sail stars, and the
    /// Smoke's burst on top, clamped under the wire.
    pub(crate) fn velocidade_no_mar(s: &Session, agora: f32) -> f32 {
        let id = s.equipment.navio.filter(|i| item_id::e_navio(*i)).unwrap_or(NAVIO_CHALUPA);
        let d = nv::dados(s.equipment.navio_inst.as_ref(), id);
        let v = nv::velocidade_do_navio(id, &d);
        if agora < s.fumaca_ate {
            (v * nv::FUMACA_MULT).min(nv::VELOCIDADE_MAX)
        } else {
            v
        }
    }

    // ───────────────────── phase 2: ship skills ─────────────────────

    /// A SHIP SKILL (`navio::HABILIDADES`), at sea only: instant, mana-free,
    /// on its own cooldown (`skill_cds` under `navio::id_de_skill`, so the
    /// client reads it from the usual `SkillsState`). The cannons go
    /// through the same target rules as the weapon skills (PK modes, the
    /// harbour corridor, line of sight); a mob takes the hit through
    /// `pending_skill_hits`, another player's HULL takes it directly.
    pub(crate) fn habilidade_do_navio(&mut self, sid: SessionId, h: u8) {
        let Some(hab) = nv::habilidade(h) else {
            return;
        };
        let id = nv::id_de_skill(h);
        if !self.no_mar() {
            self.rejeita_skill(sid, id, "Ship skills work only at sea.");
            return;
        }
        let agora = self.sim_time_s;
        let Some(s) = self.sessions.get(&sid).filter(|s| s.logged_in) else {
            return;
        };
        if s.downed || s.entity.is_none() {
            self.rejeita_skill(sid, id, "Wait for the current action to finish.");
            return;
        }
        if s.skill_cds.get(&id).is_some_and(|&fim| fim > agora) {
            self.rejeita_skill(sid, id, "Skill on cooldown.");
            return;
        }
        let (dono, entity, selecionado) = (s.entity_id, s.entity.unwrap(), s.target);
        let Some((navio, mut d)) = self.navio_equipado(sid) else {
            self.rejeita_skill(sid, id, "You have no ship. Put one in the Ship slot of your equipment.");
            return;
        };
        if !nv::pode_zarpar(&d) {
            self.rejeita_skill(sid, id, "Your ship is damaged. The Harbour Captain can repair it.");
            return;
        }
        let Some(de) = self.ecs.get::<&Position>(entity).ok().map(|p| p.0) else {
            return;
        };
        let rumo = self.ecs.get::<&Velocity>(entity).map(|v| v.0).unwrap_or(Vec2::ZERO);
        let nivel = self.nivel_de_sid(sid);
        let mut alvo_fx = de;
        match hab.efeito {
            nv::Efeito::Bordada | nv::Efeito::Canhonaco => {
                if self.in_safe_zone(de) {
                    self.rejeita_skill(sid, id, "You cannot attack in the safe zone.");
                    return;
                }
                let forma = forma_do_canhao(hab.alcance);
                let alvos: Vec<(EntityId, Vec2)> = if hab.efeito == nv::Efeito::Bordada {
                    self.alvos_da_habilidade(dono, de, Vec2::Y, de, &forma)
                        .into_iter()
                        .filter(|(_, p)| nv::no_costado(de, rumo, *p))
                        .collect()
                } else {
                    let Some(alvo) = selecionado else {
                        self.rejeita_skill(sid, id, "Select an enemy to use this skill.");
                        return;
                    };
                    match self.posicao_alvo_skill(dono, alvo, de, &forma) {
                        Ok(p) => vec![(alvo, p)],
                        Err(m) => {
                            self.rejeita_skill(sid, id, m);
                            return;
                        }
                    }
                };
                self.quebrar_invisibilidade(sid);
                // The Corsair's cannon passive (phase 4).
                let canhao = self.sessions.get(&sid).map_or(0.0, |s| s.arvore.passivas().canhao_pct);
                let dano = (nv::dano_do_canhao(hab, nivel, navio, &d) as f32 * (1.0 + canhao.max(0.0))).round() as i32;
                if let Some((_, p)) = alvos.first() {
                    alvo_fx = *p;
                }
                for (alvo, p) in alvos {
                    self.tiro_de_canhao(dono, alvo, de, p, dano);
                }
                if let Some(s) = self.sessions.get_mut(&sid) {
                    s.last_combat_at_s = agora;
                }
            }
            nv::Efeito::Reparo => {
                let cheio = nv::casco_total(navio, &d);
                if d.casco >= cheio {
                    self.rejeita_skill(sid, id, "Your hull is already sound.");
                    return;
                }
                d.casco = d.casco.saturating_add(nv::cura_do_reparo(navio, &d)).min(cheio);
                self.guarda_navio(sid, d);
            }
            nv::Efeito::Fumaca => {
                self.comecar_invisibilidade(sid, nv::FUMACA_S);
                if let Some(s) = self.sessions.get_mut(&sid) {
                    s.fumaca_ate = agora + nv::FUMACA_VELOCIDADE_S;
                }
            }
        }
        if let Some(s) = self.sessions.get_mut(&sid) {
            s.skill_cds.insert(id, agora + hab.espera_s);
        }
        crate::telemetria::conta("skill_do_navio", hab.nome, 1);
        self.estado_skills(sid);
        for s in self.sessions.values().filter(|s| s.logged_in) {
            let _ = s.handle.to_client.send(ServerMessage::SkillImpactFx {
                skill_id: id,
                caster_eid: dono,
                caster_pos: de,
                target_pos: alvo_fx,
                tier: 0,
            });
        }
    }

    /// One cannonball from `dono` at `alvo`. A mob takes it as a skill hit
    /// (defence, the level gap and the kill credit as usual); a player's
    /// HULL takes it directly — hull against hull — when the PK modes allow.
    fn tiro_de_canhao(&mut self, dono: EntityId, alvo: EntityId, de: Vec2, p: Vec2, dano: i32) {
        let vitima = self.sessions.values().find(|s| s.entity_id == alvo).map(|s| s.handle.id);
        match vitima {
            Some(vsid) => {
                if !self.can_damage_player(dono, alvo) || self.in_safe_zone(p) {
                    return;
                }
                self.registrar_agressao(dono, alvo);
                // The Pirate's Code: more against a marked ship.
                let contra_marcado = if self.marcado_eid(alvo) {
                    self.sessions.values().find(|s| s.entity_id == dono).map_or(0.0, |s| s.arvore.passivas().canhao_vs_marcado)
                } else {
                    0.0
                };
                let dano = dano as f32 * nv::PVP_CANHAO * (1.0 + contra_marcado.max(0.0));
                self.dano_no_casco(vsid, dano.round() as i32, Some(dono));
            }
            None => self.pending_skill_hits.push(PendingSkillHit {
                target_net: alvo,
                damage: dano,
                attacker_net: dono,
                hurt_dir: (de - p).normalize_or_zero(),
                is_crit: false,
                from_player: true,
                knockback: 0.0,
                drain_tier: 0,
            }),
        }
    }

    // ───────────────────── phase 2: sea elites and planks ─────────────────────

    /// Turns the freshly spawned `eid` into its route's SEA ELITE: five times
    /// the life, a harder bite, more XP, and Wreck Planks for whoever sinks
    /// it.
    pub(crate) fn tornar_elite_do_mar(&mut self, eid: EntityId) {
        let mut alvo = None;
        for (e, (n, tag, hp, kind)) in self.ecs.query_mut::<(&NetId, &mut EnemyTag, &mut Health, &EntityKind)>() {
            if n.0 != eid {
                continue;
            }
            let vida = ((hp.max as f32) * ELITE_DO_MAR_VIDA).round() as i32;
            hp.max = vida;
            hp.current = vida;
            tag.stats.hp_max = vida;
            tag.stats.attack_damage = ((tag.stats.attack_damage as f32) * ELITE_DO_MAR_DANO).round().max(1.0) as i32;
            tag.xp_reward = (tag.xp_reward as f32 * ELITE_DO_MAR_XP).round() as u64;
            let k = if let EntityKind::Enemy(k) = kind { *k } else { 0 };
            tag.boss_name = Some(format!("[ELITE] Dread {}", crate::economy::enemy_def(k).name));
            alvo = Some(e);
            break;
        }
        if let Some(e) = alvo {
            let _ = self.ecs.insert_one(e, EliteDoMarTag);
        }
    }

    /// A sea mob fell to `matador`: one Wreck Plank at
    /// `CHANCE_PRANCHA_DO_BICHO`, the elite's `PRANCHAS_DO_ELITE` always.
    pub(crate) fn prancha_do_bicho(&mut self, matador: EntityId, elite: bool, sorte: f32) {
        let qtd = if elite {
            nv::PRANCHAS_DO_ELITE
        } else if sorte < nv::CHANCE_PRANCHA_DO_BICHO {
            1
        } else {
            return;
        };
        let agora = (now_ms() / 1000) as i64;
        let Some(s) = self.sessions.values_mut().find(|s| s.entity_id == matador) else {
            return;
        };
        if !add_to_inventory(&mut s.inventory, item_id::PRANCHA_DE_NAUFRAGIO, qtd, None) {
            s.dungeon.postar(item_id::PRANCHA_DE_NAUFRAGIO, qtd, None, 0, agora);
        }
        s.inventory_dirty = true;
        self.save_pending = true;
    }

    // ───────────────────── phase 4: the treasure and the mark ─────────────────────

    /// Is this session MARKED (`navio::marcado`)? Treasure on deck, the
    /// trail after losing it, or the Pirate's Code — at sea only.
    pub(crate) fn marcado_de(&self, s: &Session) -> bool {
        let conves = s
            .equipment
            .navio
            .filter(|i| item_id::e_navio(*i))
            .is_some_and(|id| nv::dados(s.equipment.navio_inst.as_ref(), id).conves.is_some());
        nv::marcado(self.no_mar(), conves, self.sim_time_s, s.marca_ate, s.arvore.passivas().codigo_pirata())
    }

    /// `marcado_de` by entity.
    pub(crate) fn marcado_eid(&self, eid: EntityId) -> bool {
        self.sessions.values().find(|s| s.entity_id == eid).is_some_and(|s| self.marcado_de(s))
    }

    /// Every marked player at sea (for the nameplates and the skull flag).
    pub(crate) fn marcados(&self) -> std::collections::HashSet<EntityId> {
        if !self.no_mar() {
            return Default::default();
        }
        self.sessions.values().filter(|s| s.logged_in && self.marcado_de(s)).map(|s| s.entity_id).collect()
    }

    /// The `ARQUIPELAGO` index of this process's island, if it is one.
    fn ilha_daqui(&self) -> Option<usize> {
        shared::terreno::ARQUIPELAGO.iter().position(|d| d.zona == self.zona)
    }

    /// A WORLD BOSS FELL (`bosses::CHEFES`, field boss): one Colossus Chest,
    /// guaranteed, for THE KILLER — the kill credit, the same player who
    /// gets the boss's rewards. It goes on the deck of the ship in their
    /// Ship slot. No ship, or a deck already loaded: it opens on the spot
    /// at ×1.0 (a fair boss prize for whoever wants no PvP; nothing lost).
    pub(crate) fn dar_bau_do_colosso(&mut self, matador: EntityId, chefe: u16) {
        let Some(c) = shared::bosses::chefe(chefe) else {
            return;
        };
        let Some(sid) = self.sessions.values().find(|s| s.entity_id == matador && s.logged_in).map(|s| s.handle.id) else {
            return;
        };
        let t = nv::tesouro_novo(chefe, c.nivel, self.ilha_daqui());
        crate::telemetria::conta("bau_do_colosso", chefe, 1);
        match self.navio_equipado(sid) {
            Some((_, mut d)) if d.conves.is_none() => {
                d.conves = Some(t);
                self.guarda_navio(sid, d);
                self.envia_estado_navio(sid);
                self.avisa_missao(
                    sid,
                    "A Colossus Chest is on your deck. Open it here for ×1.0, or sail it to other islands for up to ×3.0 — while it is aboard, anyone can attack you at sea.".into(),
                );
            }
            _ => {
                self.avisa_missao(sid, "No free ship deck: the Colossus Chest opens on the spot (×1.0).".into());
                self.pagar_colosso(sid, t, nv::MULT_NA_ORIGEM);
            }
        }
    }

    /// Rolls and hands over a Colossus Chest at `mult`. What does not fit
    /// the bag goes to the Mail.
    fn pagar_colosso(&mut self, sid: SessionId, t: shared::items::Tesouro, mult: f32) {
        let mut rng = || fastrand::f32();
        let premios = nv::conteudo_do_colosso(&t, mult, &mut rng);
        let quando = (now_ms() / 1000) as i64;
        let Some(s) = self.sessions.get_mut(&sid) else {
            return;
        };
        let soma = |id: u16| premios.iter().filter(|p| p.item_id == id).map(|p| p.qtd).sum::<u32>();
        let (ouro, ds, pranchas) = (soma(item_id::GOLD), soma(item_id::DARKSTEEL), soma(item_id::PRANCHA_DE_NAUFRAGIO));
        for p in &premios {
            let inst = GameWorld::dg_rolar_peca(p);
            crate::telemetria::conta_de(&s.name, "colosso_item", p.item_id, p.qtd as i64);
            if !super::dungeon::entregar_premio(s, p.item_id, p.qtd, inst) {
                s.dungeon.postar(p.item_id, p.qtd, inst, 0, quando);
            }
        }
        s.inventory_dirty = true;
        self.save_pending = true;
        self.send_chat_to(
            sid,
            &format!("Colossus Chest opened at ×{mult:.1}: {ouro} gold, {ds} Darksteel, {pranchas} Wreck Planks and more."),
        );
    }

    /// OPEN THE CHEST ON DECK, on land: its multiplier (the islands crossed)
    /// plus the Corsair's treasure passives.
    fn abrir_tesouro(&mut self, sid: SessionId) {
        if self.no_mar() {
            self.avisa_missao(sid, "Bring the Colossus Chest ashore to open it: dock at an island.".into());
            return;
        }
        if self.sessions.get(&sid).is_some_and(|s| s.instancia != 0 || s.downed) {
            return;
        }
        let Some((_, mut d)) = self.navio_equipado(sid) else {
            self.avisa_missao(sid, "No treasure on your deck.".into());
            return;
        };
        let Some(t) = d.conves.take() else {
            self.avisa_missao(sid, "No treasure on your deck.".into());
            return;
        };
        let bonus = self.sessions.get(&sid).map_or(0.0, |s| s.arvore.passivas().mult_tesouro);
        let mult = nv::mult_do_tesouro(&t, bonus);
        self.guarda_navio(sid, d);
        if let Some(s) = self.sessions.get_mut(&sid) {
            crate::telemetria::conta_de(&s.name, "colosso_aberto", nv::ilhas_cruzadas(&t), 1);
        }
        self.pagar_colosso(sid, t, mult);
        self.envia_estado_navio(sid);
    }

    /// Docking with the chest on deck at island `ilha`: the island counts
    /// if the ship crossed from another one (`navio::conta_atracacao`).
    fn contar_atracacao(&mut self, sid: SessionId, ilha: usize) {
        let Some((_, mut d)) = self.navio_equipado(sid) else {
            return;
        };
        let Some(mut t) = d.conves else {
            return;
        };
        let de = self.sessions.get(&sid).map(|s| s.zona_volta.clone()).unwrap_or_default();
        if !nv::conta_atracacao(&mut t, ilha, &de) {
            return;
        }
        d.conves = Some(t);
        self.guarda_navio(sid, d);
        let bonus = self.sessions.get(&sid).map_or(0.0, |s| s.arvore.passivas().mult_tesouro);
        let mult = nv::mult_do_tesouro(&t, bonus);
        self.send_chat_to(sid, &format!("The Colossus Chest made the crossing: it now pays ×{mult:.1}."));
    }

    /// THE TREASURE IS LOST by `vitima` (its deck already emptied): the
    /// trail of the mark starts; killed by a PLAYER (`atacante`), it goes
    /// straight to the killer's deck if free, else it FLOATS at `onde`.
    /// Never destroyed.
    pub(crate) fn tesouro_perdido(&mut self, vitima: SessionId, t: shared::items::Tesouro, atacante: Option<EntityId>, onde: Vec2) {
        let agora = self.sim_time_s;
        if let Some(s) = self.sessions.get_mut(&vitima) {
            s.marca_ate = agora + nv::TRILHA_DA_MARCA_S;
        }
        let matador = atacante.and_then(|a| self.sessions.values().find(|s| s.entity_id == a && s.logged_in).map(|s| s.handle.id));
        if let Some(ksid) = matador.filter(|k| *k != vitima) {
            if let Some((_, mut kd)) = self.navio_equipado(ksid).filter(|(_, d)| d.conves.is_none()) {
                kd.conves = Some(t);
                self.guarda_navio(ksid, kd);
                self.envia_estado_navio(ksid);
                self.avisa_missao(ksid, "You took their Colossus Chest: it is on your deck now.".into());
                self.avisa_missao(vitima, "Your Colossus Chest was taken.".into());
                return;
            }
        }
        self.boiar_tesouro(onde, t);
        self.avisa_missao(vitima, "Your Colossus Chest floats at the wreck: anyone can haul it aboard.".into());
    }

    /// A Colossus Chest FLOATING at `p` (`EntityKind::Npc(7)`, drawn as a
    /// chest), first come first served.
    fn boiar_tesouro(&mut self, p: Vec2, t: shared::items::Tesouro) {
        let p = self.agua_no_mar(p);
        let eid = self.alloc_entity_id();
        self.ecs.spawn((
            NetId(eid),
            Position(p),
            Velocity(Vec2::ZERO),
            EntityKind::Npc(7),
            TesouroBoiandoTag { tesouro: t, some_em: self.sim_time_s + nv::BOIA_S },
        ));
        crate::telemetria::conta("colosso_boiando", "", 1);
    }

    /// A touch on a floating Colossus Chest: start hauling it aboard.
    pub(crate) fn tocar_tesouro(&mut self, sid: SessionId, entidade: hecs::Entity) {
        let Some(p) = self.ecs.get::<&Position>(entidade).ok().map(|p| p.0) else {
            return;
        };
        let Some(bau) = self.ecs.get::<&NetId>(entidade).ok().map(|n| n.0) else {
            return;
        };
        let Some(eu) = self.pos_do_jogador(sid) else {
            return;
        };
        let Some((_, d)) = self.navio_equipado(sid) else {
            self.avisa_missao(sid, "You have no ship. Put one in the Ship slot of your equipment.".into());
            return;
        };
        if d.conves.is_some() {
            self.avisa_missao(sid, "Your deck already carries a Colossus Chest.".into());
            return;
        }
        if eu.distance(p) > nv::ALCANCE_DO_TESOURO {
            self.avisa_missao(sid, "Sail closer to the chest.".into());
            return;
        }
        let agora = self.sim_time_s;
        let Some(s) = self.sessions.get_mut(&sid) else {
            return;
        };
        let tempo = nv::tempo_de_pegar(s.arvore.passivas().pegar_tesouro);
        s.pegando_tesouro = Some(forte::AberturaDeBau { bau, desde: agora, de: eu });
        let _ = s.handle.to_client.send(ServerMessage::ColetaEstado {
            tipo: nv::TIPO_COLETA_TESOURO,
            intervalo_s: tempo,
            progresso: 0.0,
            centro: Some([p.x, p.y]),
            pausado: false,
        });
    }

    fn parar_de_pegar(&mut self, sid: SessionId) {
        if let Some(s) = self.sessions.get_mut(&sid) {
            if s.pegando_tesouro.take().is_some() {
                let _ = s.handle.to_client.send(ServerMessage::ColetaEstado {
                    tipo: shared::protocol::COLETA_PARADA,
                    intervalo_s: 0.0,
                    progresso: 0.0,
                    centro: None,
                    pausado: false,
                });
            }
        }
    }

    /// The floating chests: hauls that finish or break, and chests whose
    /// time at the wreck ran out drift to the nearest drifting-chest spot.
    pub(crate) fn tick_tesouros(&mut self) {
        let agora = self.sim_time_s;
        let vencidos: Vec<(hecs::Entity, Vec2)> = self
            .ecs
            .query::<(&Position, &TesouroBoiandoTag)>()
            .iter()
            .filter(|(_, (_, t))| agora >= t.some_em)
            .map(|(e, (p, _))| (e, p.0))
            .collect();
        for (e, p) in vencidos {
            let destino = shared::mar::baus()
                .into_iter()
                .min_by(|a, b| a.distance(p).total_cmp(&b.distance(p)))
                .unwrap_or(p);
            if let Ok(mut pos) = self.ecs.get::<&mut Position>(e) {
                pos.0 = destino;
            }
            if let Ok(mut t) = self.ecs.get::<&mut TesouroBoiandoTag>(e) {
                t.some_em = f32::INFINITY;
            }
        }
        let pegando: Vec<(SessionId, forte::AberturaDeBau, f32)> = self
            .sessions
            .iter()
            .filter_map(|(sid, s)| s.pegando_tesouro.map(|a| (*sid, a, nv::tempo_de_pegar(s.arvore.passivas().pegar_tesouro))))
            .collect();
        for (sid, a, tempo) in pegando {
            let bau = self
                .ecs
                .query::<(&NetId, &Position, &TesouroBoiandoTag)>()
                .iter()
                .find(|(_, (n, _, _))| n.0 == a.bau)
                .map(|(e, (_, p, t))| (e, p.0, t.tesouro));
            let Some((e, p, t)) = bau else {
                self.parar_de_pegar(sid);
                continue;
            };
            let eu = self.pos_do_jogador(sid);
            if eu.is_none_or(|eu| eu.distance(a.de) > 1.0 || eu.distance(p) > nv::ALCANCE_DO_TESOURO) {
                self.parar_de_pegar(sid);
                continue;
            }
            if agora - a.desde < tempo {
                continue;
            }
            self.parar_de_pegar(sid);
            let Some((_, mut d)) = self.navio_equipado(sid).filter(|(_, d)| d.conves.is_none()) else {
                continue;
            };
            d.conves = Some(t);
            self.guarda_navio(sid, d);
            self.removed_this_tick.push(a.bau);
            let _ = self.ecs.despawn(e);
            self.envia_estado_navio(sid);
            self.avisa_missao(sid, "The Colossus Chest is on your deck. Anyone can attack you at sea now.".into());
        }
    }
}

/// The sea elite (`tornar_elite_do_mar`).
pub struct EliteDoMarTag;

/// A COLOSSUS CHEST FLOATING at a wreck (phase 4): a sunk carrier's.
/// `some_em` = sim time it drifts off to a drifting-chest spot (then
/// `INFINITY`: it waits there for anyone, never destroyed).
pub struct TesouroBoiandoTag {
    pub tesouro: shared::items::Tesouro,
    pub some_em: f32,
}

/// The sea elite: life, bite and XP multipliers, and how long it takes to
/// come back once sunk.
pub const ELITE_DO_MAR_VIDA: f32 = 5.0;
pub const ELITE_DO_MAR_DANO: f32 = 1.4;
pub const ELITE_DO_MAR_XP: f32 = 4.0;
pub const ELITE_DO_MAR_VOLTA_S: f32 = 180.0;

/// Is nest zone `zone_id` an ELITE nest? Every route has two nests (a
/// third and two thirds of the way, `mar::ninhos`); the second holds the
/// route's elite in its first slot.
pub(crate) fn e_ninho_de_elite(zone_id: u32) -> bool {
    zone_id >= ZONA_DO_MAR_ID && (zone_id - ZONA_DO_MAR_ID) % 2 == 1
}

/// A cannon's reach dressed as a `Skill`, so the targets come from the same
/// functions the weapon skills use.
fn forma_do_canhao(alcance: f32) -> shared::skills::Skill {
    shared::skills::Skill {
        id: 0,
        nome: String::new(),
        conjunto: shared::skills::Conjunto::EspadaEscudo,
        ordem: 0,
        forma: shared::skills::Forma::Circulo,
        custo_mp: 0,
        espera_s: 0.0,
        conjuracao_s: 0.0,
        dano: 1,
        cura: 0,
        alcance,
        raio: alcance,
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
        w.guarda_navio(sid, shared::items::NavioData { casco: 0, avariado: true, ..Default::default() });
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
        // The chest pays Wreck Planks (phase 2).
        assert_eq!(
            crate::craft::tem(&w.sessions[&sid].inventory, item_id::PRANCHA_DE_NAUFRAGIO),
            nv::pranchas_do_bau(shared::mar::nivel_em(p))
        );
        // One elite nest per route.
        let elites = w.spawn_zones.iter().filter(|z| e_ninho_de_elite(z.id)).count();
        assert!(elites >= 5 && elites <= shared::mar::rotas().len(), "{elites} elite nests");
    }

    // ───────────────────── phase 2 ─────────────────────

    fn da(w: &mut GameWorld, sid: SessionId, item: u16, qtd: u32) {
        // Straight into a slot of its own: the test economy has no stack
        // sizes, and what is counted is the total (`craft::tem`).
        let s = w.sessions.get_mut(&sid).unwrap();
        s.inventory.push(shared::InventorySlot { item_id: item, qty: qtd, instance: None });
    }

    fn tem(w: &GameWorld, sid: SessionId, item: u16) -> u32 {
        crate::craft::tem(&w.sessions[&sid].inventory, item)
    }

    fn no_nivel(w: &mut GameWorld, sid: SessionId, nivel: u32) {
        w.sessions.get_mut(&sid).unwrap().xp = shared::xp_for_level_with_mult(nivel, crate::economy::xp_multiplier());
    }

    fn tem_navio_fora_do_slot(w: &GameWorld, sid: SessionId) -> bool {
        let s = &w.sessions[&sid];
        s.inventory.iter().chain(s.vault.iter()).any(|x| x.qty > 0 && item_id::e_navio(x.item_id))
            || s.dungeon.correio.iter().any(|c| item_id::e_navio(c.item_id))
    }

    /// THE SHIPWRIGHT BUILDS THE NEXT HULL: it refuses without the level or
    /// the materials (taking nothing), then consumes the Sloop and exactly
    /// the recipe, and the Schooner keeps the stars and the sail.
    #[test]
    fn o_estaleiro_consome_o_casco_anterior_e_os_materiais() {
        let (mut w, sid, _rx) = mundo("ilha_inicial", &[]);
        com_capitao(&mut w);
        dar_chalupa(w.sessions.get_mut(&sid).unwrap());
        let d = shared::items::NavioData { estrelas: [1, 2, 3], vela: 2, ..nv::novo(NAVIO_CHALUPA) };
        w.guarda_navio(sid, d);
        let r = *nv::receita_depois_de(NAVIO_CHALUPA).unwrap();
        // No materials: refused.
        no_nivel(&mut w, sid, 40);
        w.handle_navio(sid, PedidoNavio::Construir);
        assert_eq!(w.sessions[&sid].equipment.navio, Some(NAVIO_CHALUPA));
        for (item, q) in r.materiais {
            da(&mut w, sid, item, q + 3);
        }
        // Below the Schooner's level: refused, nothing taken.
        no_nivel(&mut w, sid, nv::casco(r.para).unwrap().nivel - 1);
        w.handle_navio(sid, PedidoNavio::Construir);
        assert_eq!(w.sessions[&sid].equipment.navio, Some(NAVIO_CHALUPA));
        assert!(r.materiais.iter().all(|(item, q)| tem(&w, sid, *item) == q + 3));
        no_nivel(&mut w, sid, 40);
        w.handle_navio(sid, PedidoNavio::Construir);
        let (id, d) = w.navio_equipado(sid).unwrap();
        assert_eq!(id, item_id::NAVIO_ESCUNA, "the Sloop became a Schooner");
        assert_eq!(d.estrelas, [1, 2, 3]);
        assert_eq!(d.vela, 2);
        assert_eq!(d.casco, nv::casco_total(id, &d));
        assert!(r.materiais.iter().all(|(item, _)| tem(&w, sid, *item) == 3), "the recipe was not consumed exactly");
        assert!(!tem_navio_fora_do_slot(&w, sid), "the Sloop is gone");
        // Far from the Captain: refused.
        poe_em(&mut w, sid, Vec2::new(500.0, 500.0));
        for (item, q) in nv::receita_depois_de(item_id::NAVIO_ESCUNA).unwrap().materiais {
            da(&mut w, sid, item, q);
        }
        w.handle_navio(sid, PedidoNavio::Construir);
        assert_eq!(w.navio_equipado(sid).unwrap().0, item_id::NAVIO_ESCUNA);
    }

    /// SHIP STARS: copper, Darksteel and Wreck Planks by `custo_da_estrela`,
    /// five at most, never Rebirth Essence; the hull star raises the hull.
    #[test]
    fn estrelas_do_navio_custam_e_param_em_cinco_sem_essencia() {
        let (mut w, sid, _rx) = mundo("ilha_inicial", &[]);
        com_capitao(&mut w);
        dar_chalupa(w.sessions.get_mut(&sid).unwrap());
        da(&mut w, sid, item_id::COPPER, 100_000);
        da(&mut w, sid, item_id::DARKSTEEL, 50_000);
        da(&mut w, sid, item_id::PRANCHA_DE_NAUFRAGIO, 500);
        da(&mut w, sid, item_id::ESSENCIA_RENASCIMENTO, 77);
        let mut gasto = (0u32, 0u32, 0u32);
        for e in 0..nv::ESTRELAS_MAX {
            let c = nv::custo_da_estrela(e).unwrap();
            gasto = (gasto.0 + c.cobre, gasto.1 + c.darksteel, gasto.2 + c.pranchas);
            w.handle_navio(sid, PedidoNavio::Melhorar { eixo: nv::Eixo::Canhao as u8 });
        }
        let (_, d) = w.navio_equipado(sid).unwrap();
        assert_eq!(d.estrelas, [0, 0, 5]);
        assert_eq!(tem(&w, sid, item_id::COPPER), 100_000 - gasto.0);
        assert_eq!(tem(&w, sid, item_id::DARKSTEEL), 50_000 - gasto.1);
        assert_eq!(tem(&w, sid, item_id::PRANCHA_DE_NAUFRAGIO), 500 - gasto.2);
        // A sixth: refused, nothing taken.
        w.handle_navio(sid, PedidoNavio::Melhorar { eixo: nv::Eixo::Canhao as u8 });
        assert_eq!(w.navio_equipado(sid).unwrap().1.estrelas, [0, 0, 5]);
        assert_eq!(tem(&w, sid, item_id::COPPER), 100_000 - gasto.0);
        assert_eq!(tem(&w, sid, item_id::ESSENCIA_RENASCIMENTO), 77, "ship stars never cost Essence");
        // The hull star raises the hull by what it adds.
        w.handle_navio(sid, PedidoNavio::Melhorar { eixo: nv::Eixo::Casco as u8 });
        let (id, d) = w.navio_equipado(sid).unwrap();
        assert_eq!(d.casco, nv::casco_total(id, &d));
        assert!(d.casco > nv::casco_max(id));
        // Without copper: refused.
        let s = w.sessions.get_mut(&sid).unwrap();
        let todo = crate::craft::tem(&s.inventory, item_id::COPPER);
        crate::craft::consumir(&mut s.inventory, item_id::COPPER, todo);
        w.handle_navio(sid, PedidoNavio::Melhorar { eixo: nv::Eixo::Vela as u8 });
        assert_eq!(w.navio_equipado(sid).unwrap().1.estrelas[1], 0);
    }

    /// A world at sea with the sea's relief, the player on open water with a
    /// Sloop at level 30, and one sea mob at `alvo` from the player.
    fn no_mar_com_bicho(alvo: Vec2) -> (GameWorld, SessionId, mpsc::UnboundedReceiver<ServerMessage>, Vec2) {
        let (mut w, sid, rx) = mundo(shared::mar::ZONA, &[]);
        w.ilha = Some(shared::terreno::Ilha::da_ilha(&shared::mar::DEF));
        dar_chalupa(w.sessions.get_mut(&sid).unwrap());
        no_nivel(&mut w, sid, 30);
        let aqui = shared::mar::ninhos()[0];
        poe_em(&mut w, sid, aqui);
        w.ecs.spawn((
            NetId(EntityId(9_900)),
            Position(aqui + alvo),
            Velocity(Vec2::ZERO),
            EntityKind::Enemy(TUBARAO),
            Health { current: 800, max: 800 },
        ));
        (w, sid, rx, aqui)
    }

    fn golpes_em(w: &GameWorld, alvo: EntityId) -> Vec<i32> {
        w.pending_skill_hits.iter().filter(|h| h.target_net == alvo).map(|h| h.damage).collect()
    }

    /// BROADSIDE: the cannon damage of the class and stars to a mob off the
    /// beam, nothing to one dead ahead, and a cooldown. Mana-free.
    #[test]
    fn bordada_acerta_o_costado_e_tem_recarga() {
        let bicho = EntityId(9_900);
        let (mut w, sid, _rx, _) = no_mar_com_bicho(Vec2::new(6.0, 0.0));
        w.sessions.get_mut(&sid).unwrap().mp_current = 0.0;
        w.handle_navio(sid, PedidoNavio::Habilidade { id: 0 });
        let (id, d) = w.navio_equipado(sid).unwrap();
        let esperado = nv::dano_do_canhao(&nv::HABILIDADES[0], 30, id, &d);
        assert_eq!(golpes_em(&w, bicho), vec![esperado], "no mana needed");
        let cd = w.sessions[&sid].skill_cds[&nv::id_de_skill(0)];
        assert!((cd - w.sim_time_s - nv::HABILIDADES[0].espera_s).abs() < 1e-3);
        // On cooldown: nothing.
        w.handle_navio(sid, PedidoNavio::Habilidade { id: 0 });
        assert_eq!(golpes_em(&w, bicho).len(), 1);
        // Sailing north with the mob dead ahead: the bow is blind.
        let (mut w, sid, _rx, _) = no_mar_com_bicho(Vec2::new(0.0, 6.0));
        let e = w.sessions[&sid].entity.unwrap();
        w.ecs.get::<&mut Velocity>(e).unwrap().0 = Vec2::new(0.0, 8.0);
        w.handle_navio(sid, PedidoNavio::Habilidade { id: 0 });
        assert!(golpes_em(&w, bicho).is_empty(), "a broadside hit the bow");
        // More stars on the cannon, more damage.
        let (mut w, sid, _rx, _) = no_mar_com_bicho(Vec2::new(6.0, 0.0));
        let d = shared::items::NavioData { estrelas: [0, 0, 5], ..nv::novo(NAVIO_CHALUPA) };
        w.guarda_navio(sid, d);
        w.handle_navio(sid, PedidoNavio::Habilidade { id: 0 });
        assert!(golpes_em(&w, bicho)[0] > esperado);
        // Ashore the ship's bar does nothing.
        let (mut w, sid, _rx) = mundo("ilha_inicial", &[]);
        dar_chalupa(w.sessions.get_mut(&sid).unwrap());
        w.handle_navio(sid, PedidoNavio::Habilidade { id: 0 });
        assert!(w.sessions[&sid].skill_cds.is_empty());
    }

    /// BOW CANNON: the selected target in range, or nothing.
    #[test]
    fn canhonaco_precisa_de_alvo_no_alcance() {
        let bicho = EntityId(9_900);
        let (mut w, sid, _rx, _) = no_mar_com_bicho(Vec2::new(0.0, 15.0));
        w.handle_navio(sid, PedidoNavio::Habilidade { id: 1 });
        assert!(golpes_em(&w, bicho).is_empty(), "fired with no target");
        assert!(w.sessions[&sid].skill_cds.get(&nv::id_de_skill(1)).is_none(), "a refusal costs no cooldown");
        w.sessions.get_mut(&sid).unwrap().target = Some(bicho);
        w.handle_navio(sid, PedidoNavio::Habilidade { id: 1 });
        let (id, d) = w.navio_equipado(sid).unwrap();
        assert_eq!(golpes_em(&w, bicho), vec![nv::dano_do_canhao(&nv::HABILIDADES[1], 30, id, &d)]);
        // Out of range.
        let (mut w, sid, _rx, _) = no_mar_com_bicho(Vec2::new(0.0, 30.0));
        w.sessions.get_mut(&sid).unwrap().target = Some(bicho);
        w.handle_navio(sid, PedidoNavio::Habilidade { id: 1 });
        assert!(golpes_em(&w, bicho).is_empty());
    }

    /// EMERGENCY REPAIR heals 30% of the full hull, has a long cooldown,
    /// and is refused on a sound hull.
    #[test]
    fn reparo_de_emergencia_cura_o_casco_com_recarga_longa() {
        let (mut w, sid, _rx, _) = no_mar_com_bicho(Vec2::new(30.0, 30.0));
        w.handle_navio(sid, PedidoNavio::Habilidade { id: 2 });
        assert!(w.sessions[&sid].skill_cds.get(&nv::id_de_skill(2)).is_none(), "repaired a sound hull");
        w.guarda_navio(sid, shared::items::NavioData { casco: 1_000, ..nv::novo(NAVIO_CHALUPA) });
        w.handle_navio(sid, PedidoNavio::Habilidade { id: 2 });
        assert_eq!(w.navio_equipado(sid).unwrap().1.casco, 1_000 + 720);
        w.handle_navio(sid, PedidoNavio::Habilidade { id: 2 });
        assert_eq!(w.navio_equipado(sid).unwrap().1.casco, 1_720, "the cooldown let it heal twice");
        w.sim_time_s += nv::HABILIDADES[2].espera_s + 0.1;
        w.handle_navio(sid, PedidoNavio::Habilidade { id: 2 });
        assert_eq!(w.navio_equipado(sid).unwrap().1.casco, 2_400, "capped at the full hull");
    }

    /// SMOKE: untargetable at once, faster for a moment, never past the
    /// wire.
    #[test]
    fn fumaca_some_e_acelera_sem_passar_do_fio() {
        let (mut w, sid, _rx, _) = no_mar_com_bicho(Vec2::new(30.0, 30.0));
        let base = GameWorld::velocidade_no_mar(&w.sessions[&sid], w.sim_time_s);
        w.handle_navio(sid, PedidoNavio::Habilidade { id: 3 });
        let s = &w.sessions[&sid];
        assert!(s.invisivel_ate > w.sim_time_s);
        assert!(w.ecs.get::<&Untargetable>(s.entity.unwrap()).is_ok());
        let rapida = GameWorld::velocidade_no_mar(s, w.sim_time_s);
        assert!(rapida > base && rapida <= nv::VELOCIDADE_MAX);
        let depois = GameWorld::velocidade_no_mar(s, w.sim_time_s + nv::FUMACA_VELOCIDADE_S + 0.1);
        assert_eq!(depois, base);
        // A five-star sail Man-o'-War in smoke: clamped under the wire.
        let s = w.sessions.get_mut(&sid).unwrap();
        let mut inst = nv::instancia_nova(item_id::NAVIO_NAU);
        inst.navio.as_mut().unwrap().estrelas = [0, 5, 0];
        s.equipment.set(EquipSlot::Navio, Some(item_id::NAVIO_NAU), Some(inst));
        let v = GameWorld::velocidade_no_mar(s, s.fumaca_ate - 0.1);
        assert!(v <= nv::VELOCIDADE_MAX && v < nv::TETO_DO_FIO);
    }

    /// CANNONS AND PK: hull against hull. A peaceful target is never hit; in
    /// free PK the cannon takes the target's HULL, not its HP; the harbour
    /// corridor protects.
    #[test]
    fn canhao_contra_jogador_segue_o_pk_e_bate_no_casco() {
        let (mut w, a, _rx, aqui) = no_mar_com_bicho(Vec2::new(300.0, 300.0));
        let b = SessionId(([127, 0, 0, 1], 47_472).into());
        let (tx, rx2) = mpsc::unbounded_channel();
        std::mem::forget(rx2);
        w.on_connect(SessionHandle { id: b, to_client: tx });
        let eb = EntityId(4_702);
        let e = w.ecs.spawn((
            NetId(eb),
            Position(aqui + Vec2::new(6.0, 0.0)),
            Velocity(Vec2::ZERO),
            EntityKind::Player,
            Health { current: 100, max: 100 },
            PlayerTag { name: "Pirata".into(), player_id: PlayerId(49), attack_anim_pending: None, combo_step_pending: None },
        ));
        {
            let s = w.sessions.get_mut(&b).unwrap();
            s.logged_in = true;
            s.entity = Some(e);
            s.entity_id = eb;
            s.name = "Pirata".into();
            s.inventory = vec![shared::InventorySlot::default(); shared::INVENTORY_SLOTS];
            dar_chalupa(s);
        }
        let modo = |w: &mut GameWorld, sid: SessionId, m: u8| {
            let s = w.sessions.get_mut(&sid).unwrap();
            s.pk_modo = m;
            s.pk_escolhido = m;
        };
        modo(&mut w, a, shared::pk::PACIFICO);
        modo(&mut w, b, shared::pk::PACIFICO);
        w.handle_navio(a, PedidoNavio::Habilidade { id: 0 });
        assert_eq!(w.navio_equipado(b).unwrap().1.casco, 2_400, "a peaceful ship was shot");
        modo(&mut w, a, shared::pk::LIVRE);
        w.sim_time_s += 10.0;
        w.handle_navio(a, PedidoNavio::Habilidade { id: 0 });
        let casco = w.navio_equipado(b).unwrap().1.casco;
        assert!(casco < 2_400, "free PK does not reach the hull");
        assert_eq!(w.ecs.get::<&Health>(e).unwrap().current, 100, "the cannon hit the body, not the hull");
        // In the harbour corridor: nothing.
        poe_em(&mut w, a, fundeadouro_padrao());
        poe_em(&mut w, b, fundeadouro_padrao() + Vec2::new(5.0, 0.0));
        w.sim_time_s += 10.0;
        w.handle_navio(a, PedidoNavio::Habilidade { id: 0 });
        assert_eq!(w.navio_equipado(b).unwrap().1.casco, casco);
    }

    /// WRECK PLANKS at sea: the elite always pays them, a common mob now
    /// and then (the chest's are in `o_mar_tem_ninhos_e_baus`).
    #[test]
    fn pranchas_de_naufragio_caem_no_mar() {
        let (mut w, sid, _rx) = mundo(shared::mar::ZONA, &[]);
        let eu = w.sessions[&sid].entity_id;
        w.prancha_do_bicho(eu, true, 0.99);
        assert_eq!(tem(&w, sid, item_id::PRANCHA_DE_NAUFRAGIO), nv::PRANCHAS_DO_ELITE);
        w.prancha_do_bicho(eu, false, 0.99);
        assert_eq!(tem(&w, sid, item_id::PRANCHA_DE_NAUFRAGIO), nv::PRANCHAS_DO_ELITE);
        w.prancha_do_bicho(eu, false, nv::CHANCE_PRANCHA_DO_BICHO * 0.5);
        assert_eq!(tem(&w, sid, item_id::PRANCHA_DE_NAUFRAGIO), nv::PRANCHAS_DO_ELITE + 1);
        assert!(e_ninho_de_elite(ZONA_DO_MAR_ID + 1) && !e_ninho_de_elite(ZONA_DO_MAR_ID));
    }

    /// A SHIP COSMETIC from the bag dresses the ship and is spent; with no
    /// ship it stays in the bag. At sea the look rides on the meta.
    #[test]
    fn cosmetico_veste_o_navio() {
        let (mut w, sid, _rx) = mundo("ilha_inicial", &[]);
        let vela = shared::navio::VELAS[1].item;
        w.sessions.get_mut(&sid).unwrap().inventory[3] = shared::InventorySlot { item_id: vela, qty: 2, instance: None };
        w.handle_use_item(sid, 3);
        assert_eq!(w.sessions[&sid].inventory[3].qty, 2, "spent with no ship");
        dar_chalupa(w.sessions.get_mut(&sid).unwrap());
        w.handle_use_item(sid, 3);
        assert_eq!(w.navio_equipado(sid).unwrap().1.vela, 2);
        assert_eq!(w.sessions[&sid].inventory[3].qty, 1);
        let figura = shared::navio::FIGURAS[2].0;
        w.sessions.get_mut(&sid).unwrap().inventory[4] = shared::InventorySlot { item_id: figura, qty: 1, instance: None };
        w.handle_use_item(sid, 4);
        let d = w.navio_equipado(sid).unwrap().1;
        assert_eq!((d.vela, d.figura), (2, 3));
        assert_eq!(w.sessions[&sid].inventory[4].qty, 0);
        w.zona = shared::mar::ZONA.into();
        let eid = w.sessions[&sid].entity_id;
        let v = shared::navio::Visual::do_fio(w.visuais_dos_navios()[&eid]);
        assert_eq!((v.classe, v.vela, v.figura), (0, 2, 3));
    }

    // ───────────────────── phase 4 ─────────────────────

    /// A second player at `p` with a Sloop, in `modo`.
    fn outro(w: &mut GameWorld, porta: u16, eid: u32, nome: &str, p: Vec2) -> SessionId {
        let b = SessionId(([127, 0, 0, 1], porta).into());
        let (tx, rx2) = mpsc::unbounded_channel();
        std::mem::forget(rx2);
        w.on_connect(SessionHandle { id: b, to_client: tx });
        let eb = EntityId(eid);
        let e = w.ecs.spawn((
            NetId(eb),
            Position(p),
            Velocity(Vec2::ZERO),
            EntityKind::Player,
            Health { current: 100, max: 100 },
            PlayerTag { name: nome.into(), player_id: PlayerId(eid as u64), attack_anim_pending: None, combo_step_pending: None },
        ));
        let s = w.sessions.get_mut(&b).unwrap();
        s.logged_in = true;
        s.entity = Some(e);
        s.entity_id = eb;
        s.name = nome.into();
        s.inventory = vec![shared::InventorySlot::default(); shared::INVENTORY_SLOTS];
        dar_chalupa(s);
        b
    }

    fn modo_pk(w: &mut GameWorld, sid: SessionId, m: u8) {
        let s = w.sessions.get_mut(&sid).unwrap();
        s.pk_modo = m;
        s.pk_escolhido = m;
    }

    fn com_tesouro(w: &mut GameWorld, sid: SessionId) -> shared::items::Tesouro {
        let c = &shared::bosses::CHEFES[0];
        let t = nv::tesouro_novo(c.kind, c.nivel, Some(1));
        let (_, mut d) = w.navio_equipado(sid).unwrap();
        d.conves = Some(t);
        w.guarda_navio(sid, d);
        t
    }

    /// `item` in the bag and the Mail (a big chest overflows to the Mail).
    fn tem_tudo(w: &GameWorld, sid: SessionId, item: u16) -> u32 {
        let s = &w.sessions[&sid];
        tem(w, sid, item) + s.dungeon.correio.iter().filter(|c| c.item_id == item).map(|c| c.qtd).sum::<u32>()
    }

    fn conves(w: &GameWorld, sid: SessionId) -> Option<shared::items::Tesouro> {
        w.navio_equipado(sid).and_then(|(_, d)| d.conves)
    }

    /// THE COLOSSUS CHEST drops from a world boss onto the killer's deck,
    /// and the bag, the bank, a trade and the Mail all refuse the loaded
    /// ship; it cannot even leave the Ship slot. With no ship it opens on
    /// the spot.
    #[test]
    fn o_bau_do_colosso_cai_do_chefe_e_so_cabe_no_conves() {
        let (mut w, sid, _rx) = mundo("ilha_gelo", &[]);
        dar_chalupa(w.sessions.get_mut(&sid).unwrap());
        let eid = w.sessions[&sid].entity_id;
        let chefe = shared::bosses::CHEFES[0].kind;
        w.dar_bau_do_colosso(eid, chefe);
        let t = conves(&w, sid).expect("the chest is on deck");
        assert_eq!((t.chefe, t.origem, t.ilhas), (chefe, 1, 0), "dropped on the Glacier");
        assert_eq!(nv::mult_do_tesouro(&t, 0.0), 1.0);
        let s = w.sessions.get_mut(&sid).unwrap();
        let (id, inst) = (s.equipment.navio.unwrap(), s.equipment.navio_inst);
        // The bag.
        let mut bolsa = vec![shared::InventorySlot::default(); 4];
        assert!(!add_to_inventory(&mut bolsa, id, 1, inst), "the bag took the treasure");
        // The Mail.
        let cartas = s.dungeon.correio.len();
        s.dungeon.postar(id, 1, inst, 0, 0);
        assert_eq!(s.dungeon.correio.len(), cartas, "the Mail took the treasure");
        // A trade.
        let slot = shared::InventorySlot { item_id: id, qty: 1, instance: inst };
        assert!(!shared::troca::negociavel(&slot, false), "a trade took the treasure");
        // It cannot leave the Ship slot.
        w.handle_inventory_swap(sid, InvSpot::Equip(EquipSlot::Navio), InvSpot::Inv(5));
        assert_eq!(w.sessions[&sid].equipment.navio, Some(id), "the loaded ship left its slot");
        assert!(conves(&w, sid).is_some());
        // The bank, at a banker, even if the ship were in the bag.
        w.ecs.spawn((NetId(EntityId(7_002)), Position(Vec2::new(10.0, 11.0)), EntityKind::Npc(2)));
        let s = w.sessions.get_mut(&sid).unwrap();
        s.vault = vec![shared::InventorySlot::default(); 4];
        s.inventory[6] = slot;
        w.handle_vault_deposit(sid, 6);
        assert!(w.sessions[&sid].vault.iter().all(|v| v.qty == 0), "the bank took the treasure");
        // A deck already loaded: the second chest opens on the spot.
        w.sessions.get_mut(&sid).unwrap().inventory[6] = shared::InventorySlot::default();
        w.dar_bau_do_colosso(eid, chefe);
        assert_eq!(conves(&w, sid), Some(t), "the deck holds one chest");
        assert!(tem_tudo(&w, sid, item_id::PRANCHA_DE_NAUFRAGIO) >= nv::PRANCHAS_DO_COLOSSO, "opened on the spot");
        // An unloaded ship comes off as before (into an empty bag: the
        // prize above filled it).
        w.sessions.get_mut(&sid).unwrap().inventory = vec![shared::InventorySlot::default(); shared::INVENTORY_SLOTS];
        let (_, mut d) = w.navio_equipado(sid).unwrap();
        d.conves = None;
        w.guarda_navio(sid, d);
        w.handle_inventory_swap(sid, InvSpot::Equip(EquipSlot::Navio), InvSpot::Inv(5));
        assert_eq!(w.sessions[&sid].equipment.navio, None);
    }

    /// DOCKING ELSEWHERE raises the payout; opening on land pays it and
    /// empties the deck; at sea it cannot be opened.
    #[test]
    fn atracar_em_outra_ilha_sobe_o_premio_e_abrir_paga() {
        let (mut w, sid, _rx) = mundo(shared::mar::ZONA, &[shared::mar::ZONA, "ilha_inicial", "ilha_deserto"]);
        dar_chalupa(w.sessions.get_mut(&sid).unwrap());
        historia_em(&mut w, sid, 10_000);
        com_tesouro(&mut w, sid);
        w.handle_navio(sid, PedidoNavio::AbrirTesouro);
        assert!(conves(&w, sid).is_some(), "opened at sea");
        // Sailed from the Glacier, docks at the Bosque: one island.
        w.sessions.get_mut(&sid).unwrap().zona_volta = "ilha_gelo".into();
        let bosque = shared::mar::vista_da_zona("ilha_inicial").unwrap();
        poe_em(&mut w, sid, bosque.fundeadouro());
        w.handle_navio(sid, PedidoNavio::Atracar { ilha: bosque.ilha as u8 });
        let t = conves(&w, sid).unwrap();
        assert!((nv::mult_do_tesouro(&t, 0.0) - 1.6).abs() < 1e-5);
        // On land it opens and pays planks at ×1.6.
        w.zona = "ilha_inicial".into();
        w.handle_navio(sid, PedidoNavio::AbrirTesouro);
        assert!(conves(&w, sid).is_none());
        assert_eq!(tem_tudo(&w, sid, item_id::PRANCHA_DE_NAUFRAGIO), 16);
        // Nothing left to open.
        w.handle_navio(sid, PedidoNavio::AbrirTesouro);
        assert_eq!(tem_tudo(&w, sid, item_id::PRANCHA_DE_NAUFRAGIO), 16);
    }

    /// THE MARK: derived from the deck, a 60 s trail after losing it; a
    /// marked ship is fair game for a Peaceful player at sea, never in the
    /// harbour corridor.
    #[test]
    fn a_marca_abre_o_navio_para_o_pacifico_mas_nao_no_corredor() {
        let (mut w, a, _rx) = mundo(shared::mar::ZONA, &[]);
        dar_chalupa(w.sessions.get_mut(&a).unwrap());
        let longe = shared::mar::ninhos()[0];
        poe_em(&mut w, a, longe);
        let b = outro(&mut w, 47_480, 4_780, "Corsario", longe + Vec2::new(4.0, 0.0));
        modo_pk(&mut w, a, shared::pk::PACIFICO);
        modo_pk(&mut w, b, shared::pk::PACIFICO);
        let (ea, eb) = (w.sessions[&a].entity_id, w.sessions[&b].entity_id);
        assert!(!w.marcado_eid(ea));
        assert!(!w.can_damage_player(eb, ea), "peaceful vs peaceful");
        com_tesouro(&mut w, a);
        assert!(w.marcado_eid(ea), "treasure on deck marks");
        assert!(w.can_damage_player(eb, ea), "a Peaceful player may attack the carrier");
        assert!(!w.can_damage_player(ea, eb), "the mark opens the carrier, not the hunter");
        assert!(w.marcados().contains(&ea));
        // Not on land.
        w.zona = "ilha_gelo".into();
        assert!(!w.marcado_eid(ea));
        w.zona = shared::mar::ZONA.into();
        // The corridor protects even the marked.
        poe_em(&mut w, a, fundeadouro_padrao());
        poe_em(&mut w, b, fundeadouro_padrao() + Vec2::new(3.0, 0.0));
        assert!(!w.can_damage_player(eb, ea), "the corridor let a marked ship be hit");
        // The trail: the deck empties, the mark lasts 60 s.
        let (_, mut d) = w.navio_equipado(a).unwrap();
        let t = d.conves.take().unwrap();
        w.guarda_navio(a, d);
        w.tesouro_perdido(a, t, None, longe);
        assert!(w.marcado_eid(ea), "the trail");
        w.sim_time_s += nv::TRILHA_DA_MARCA_S - 1.0;
        assert!(w.marcado_eid(ea));
        w.sim_time_s += 2.0;
        assert!(!w.marcado_eid(ea), "the trail outlived 60 s");
    }

    /// THE TWO DEATHS: sunk by the sea, the treasure FLOATS (and can be
    /// hauled aboard by anyone); sunk by a player's cannon, it goes to the
    /// killer's deck. The ship is only damaged; PK points as specified.
    #[test]
    fn afundar_faz_boiar_e_matar_em_pvp_entrega_ao_matador() {
        let (mut w, a, _rx) = mundo(shared::mar::ZONA, &[]);
        dar_chalupa(w.sessions.get_mut(&a).unwrap());
        let longe = shared::mar::ninhos()[0];
        poe_em(&mut w, a, longe);
        com_tesouro(&mut w, a);
        let ea = w.sessions[&a].entity_id;
        // Sunk by the sea (a bite): floats.
        w.mordidas_no_casco.push((ea, 50_000));
        w.tick_mar();
        assert!(conves(&w, a).is_none());
        assert!(w.navio_equipado(a).unwrap().1.avariado, "the ship is damaged, not lost");
        let boias: Vec<(hecs::Entity, Vec2)> =
            w.ecs.query::<(&Position, &TesouroBoiandoTag)>().iter().map(|(e, (p, _))| (e, p.0)).collect();
        assert_eq!(boias.len(), 1, "the treasure did not float");
        let (boia, onde) = boias[0];
        // Someone else hauls it aboard: it takes time, then it is theirs.
        let b = outro(&mut w, 47_481, 4_781, "Catador", onde + Vec2::new(2.0, 0.0));
        w.tocar_tesouro(b, boia);
        assert!(w.sessions[&b].pegando_tesouro.is_some());
        w.tick_mar();
        assert!(conves(&w, b).is_none(), "hauled in no time");
        w.sim_time_s += nv::PEGAR_S + 0.1;
        w.tick_mar();
        assert!(conves(&w, b).is_some(), "the haul did not finish");
        assert!(w.ecs.get::<&TesouroBoiandoTag>(boia).is_err());

        // Killed in PvP: B (now marked) is sunk by C's cannon; C gets it and
        // no PK points (B was marked).
        let c = outro(&mut w, 47_482, 4_782, "Pirata", onde + Vec2::new(6.0, 0.0));
        poe_em(&mut w, b, onde + Vec2::new(2.0, 0.0));
        modo_pk(&mut w, b, shared::pk::PACIFICO);
        modo_pk(&mut w, c, shared::pk::PACIFICO);
        let (eb, ec) = (w.sessions[&b].entity_id, w.sessions[&c].entity_id);
        assert!(w.can_damage_player(ec, eb));
        w.guarda_navio(b, shared::items::NavioData { casco: 1, conves: conves(&w, b), ..nv::novo(NAVIO_CHALUPA) });
        w.tiro_de_canhao(ec, eb, onde, onde, 500);
        assert!(conves(&w, b).is_none());
        assert!(conves(&w, c).is_some(), "the killer did not get the treasure");
        assert_eq!(w.sessions[&c].pk_points, 0, "PK points for sinking a marked carrier");
    }

    /// PK POINTS at sea: sinking an UNMARKED Peaceful player gives them as on
    /// land; in lawless waters none.
    #[test]
    fn pontos_de_pk_no_mar() {
        let (mut w, a, _rx) = mundo(shared::mar::ZONA, &[]);
        dar_chalupa(w.sessions.get_mut(&a).unwrap());
        let longe = shared::mar::ninhos()[0];
        assert!(!shared::mar::sem_lei(longe), "the test spot must not be lawless");
        poe_em(&mut w, a, longe);
        let b = outro(&mut w, 47_483, 4_783, "Vilao", longe + Vec2::new(4.0, 0.0));
        modo_pk(&mut w, a, shared::pk::PACIFICO);
        modo_pk(&mut w, b, shared::pk::LIVRE);
        let (ea, eb) = (w.sessions[&a].entity_id, w.sessions[&b].entity_id);
        assert!(w.can_damage_player(eb, ea));
        w.guarda_navio(a, shared::items::NavioData { casco: 1, ..nv::novo(NAVIO_CHALUPA) });
        w.tiro_de_canhao(eb, ea, longe, longe, 500);
        assert!(w.navio_equipado(a).unwrap().1.avariado);
        assert_eq!(w.sessions[&b].pk_points, 1, "sinking an innocent costs a PK point");
        // Lawless waters: both inside, the mode is forced, no points.
        let z = shared::mar::zonas_sem_lei()[0];
        let (mut w, a, _rx) = mundo(shared::mar::ZONA, &[]);
        dar_chalupa(w.sessions.get_mut(&a).unwrap());
        poe_em(&mut w, a, z);
        let b = outro(&mut w, 47_484, 4_784, "Vilao", z + Vec2::new(4.0, 0.0));
        modo_pk(&mut w, a, shared::pk::PACIFICO);
        modo_pk(&mut w, b, shared::pk::PACIFICO);
        let (ea, eb) = (w.sessions[&a].entity_id, w.sessions[&b].entity_id);
        assert!(!w.can_damage_player(eb, ea));
        w.tick_pk(1);
        assert!(w.sessions[&a].pk_travado && w.sessions[&b].pk_travado, "lawless waters lock the mode");
        assert!(w.can_damage_player(eb, ea), "lawless waters are open PvP");
        w.guarda_navio(a, shared::items::NavioData { casco: 1, ..nv::novo(NAVIO_CHALUPA) });
        w.tiro_de_canhao(eb, ea, z, z, 500);
        assert!(w.navio_equipado(a).unwrap().1.avariado);
        assert_eq!(w.sessions[&b].pk_points, 0, "PK points in lawless waters");
    }

    /// THE CORSAIR AT SEA: the cannon node hits harder, the hull node takes
    /// less, the hauling node is faster, and the Pirate's Code marks you and
    /// hits marked ships harder.
    #[test]
    fn os_nos_do_mar_do_corsario_valem() {
        let bicho = EntityId(9_900);
        let (mut w, sid, _rx, _) = no_mar_com_bicho(Vec2::new(6.0, 0.0));
        let (id, d) = w.navio_equipado(sid).unwrap();
        let cru = nv::dano_do_canhao(&nv::HABILIDADES[0], 30, id, &d);
        let mut arv = shared::arvore::Arvore::default();
        for n in arv.rota_ate(shared::arvore::CODIGO_PIRATA) {
            arv.aprender(n, 999).unwrap();
        }
        let p = arv.passivas();
        assert!(p.codigo_pirata() && p.canhao_pct > 0.0 && p.casco_pct > 0.0);
        w.sessions.get_mut(&sid).unwrap().arvore = arv;
        w.handle_navio(sid, PedidoNavio::Habilidade { id: 0 });
        let golpe = golpes_em(&w, bicho)[0];
        assert_eq!(golpe, (cru as f32 * (1.0 + p.canhao_pct)).round() as i32);
        // Always marked at sea.
        let eu = w.sessions[&sid].entity_id;
        assert!(w.marcado_eid(eu), "the Pirate's Code");
        // Hull: a bite of 1,000 takes less.
        w.dano_no_casco(sid, 1_000, None);
        let tirou = nv::casco_max(NAVIO_CHALUPA) - w.navio_equipado(sid).unwrap().1.casco;
        assert_eq!(tirou, (1_000.0 / (1.0 + p.casco_pct)).round() as u16);
        // Faster hauling.
        assert!(nv::tempo_de_pegar(p.pegar_tesouro) < nv::PEGAR_S);
    }
}
