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

use super::conves::NavioEnt;
use super::*;
use shared::item_id::{self, NAVIO_CHALUPA};
use shared::navio::{self as nv, EstadoNavio, PedidoNavio};
use shared::EquipSlot;

/// A drifting chest of the open sea (`EntityKind::Npc(7)`, like the
/// treasure chests, so the client already draws it).
pub struct BauDoMarTag;

/// The generated loot of a wreck that sank before anyone salvaged its deck.
#[derive(Debug, Clone, Copy)]
pub struct BarrilDoMarTag {
    pub ouro: u64,
    pub tokens: u32,
    pub fragmentos: u32,
    pub some_em: f32,
}

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
        || s.inventory
            .iter()
            .chain(s.vault.iter())
            .any(|x| x.qty > 0 && item_id::e_navio(x.item_id))
        || s.dungeon
            .correio
            .iter()
            .any(|c| item_id::e_navio(c.item_id))
}

/// THE SLOOP from the story (`historia::PASSO_DA_CHALUPA`): into the ship
/// slot if it is free, else the bag, else the Mail. `false` = already had
/// a ship, nothing given.
pub(crate) fn dar_chalupa(s: &mut Session) -> bool {
    if tem_navio(s) {
        return false;
    }
    let inst = nv::instancia_nova_tipo(
        nv::TipoBarco {
            tamanho: nv::Tamanho::Pequeno,
            variante: nv::Variante::Veloz,
        },
        1,
    );
    if s.equipment.navio.is_none() {
        s.equipment
            .set(EquipSlot::Navio, Some(NAVIO_CHALUPA), Some(inst));
        s.stats_dirty = true;
    } else if add_to_inventory(&mut s.inventory, NAVIO_CHALUPA, 1, Some(inst)) {
        s.inventory_dirty = true;
    } else {
        let agora = (now_ms() / 1000) as i64;
        s.dungeon.postar(
            NAVIO_CHALUPA,
            1,
            Some(inst),
            nv::MOTIVO_CORREIO_NAVIO,
            agora,
        );
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
        (shared::mar::e_mar(z)
            && !zona_volta.is_empty()
            && zona_volta != self.zona
            && zona_volta != z)
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
        let mut inst = s
            .equipment
            .navio_inst
            .unwrap_or_else(|| nv::instancia_nova(id));
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
            PedidoNavio::Piloto { ligado } => self.ligar_piloto(sid, ligado),
            PedidoNavio::Leme { pegar } => self.pegar_leme(sid, pegar),
            PedidoNavio::Embarcar { capitao } => self.embarcar_no_navio(sid, capitao),
            PedidoNavio::ConstruirTipo { tipo } => self.construir_tipo(sid, tipo),
            PedidoNavio::Alocar { pontos } => self.alocar_navio(sid, pontos),
            PedidoNavio::Respec => self.respec_navio(sid),
            PedidoNavio::UpgradeTier => self.upgrade_tier_navio(sid),
            PedidoNavio::Afundar => self.afundar_derrotado(sid, true),
            PedidoNavio::RepararVela { iniciar } => self.reparar_vela(sid, iniciar),
            PedidoNavio::Saquear => self.saquear_navio_derrotado(sid),
            PedidoNavio::Desembarcar => self.desembarcar(sid),
            PedidoNavio::VoltarAoNavio => self.voltar_do_desembarque(sid),
            PedidoNavio::ReivindicarIlhota => self.reivindicar_ilhota(sid),
        }
    }

    /// Near ANY inter-island travel NPC — the Harbour Captain, the Sky Bus
    /// Professor or the Submarine Captain (owner, 16/11/2026: "any travel
    /// NPC between islands should have the Sail option"). Each of them
    /// offers the same "My ship" row: Sail, Repair and the Shipwright.
    fn perto_do_capitao_do_porto(&self, sid: SessionId) -> bool {
        self.quem_viaja_perto(sid).is_some()
    }

    /// SAIL: from a travel NPC to the anchorage off this island on the open
    /// sea — the land port's dock, or, on an island without one (Skyreach,
    /// Kōgen-tō, Abyssia), its own sea dock (`mar::NOVAS`).
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
            self.avisa_missao(sid, "Talk to the Harbour Captain, the Sky Bus Professor or the Submarine Captain to set sail.".into());
            return;
        }
        let Some(vista) = shared::mar::vista_da_zona(&self.zona) else {
            self.avisa_missao(sid, "This island has no dock on the open sea.".into());
            return;
        };
        let Some((_, d)) = self.navio_equipado(sid) else {
            self.avisa_missao(
                sid,
                "You have no ship. Put one in the Ship slot of your equipment.".into(),
            );
            return;
        };
        if !nv::pode_zarpar(&d) {
            self.avisa_missao(
                sid,
                "Your ship is damaged. The Harbour Captain can repair it.".into(),
            );
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

    /// The port of island `ilha` (`ARQUIPELAGO` index), in that island's
    /// coordinates: where a ship that docks puts its captain ashore. The
    /// SAME spot the Captain's, the bus's and the submarine's trips land on
    /// (`embarcar`): the port, or the town for the islands without one
    /// (Skyreach, Kōgen-tō, Abyssia), where their travel NPCs stand.
    pub(crate) fn porto_da_ilha(ilha: usize) -> Option<(&'static shared::terreno::DefIlha, Vec2)> {
        let dest = shared::terreno::ARQUIPELAGO.get(ilha)?;
        let ger = shared::terreno::Gerador::da_ilha(dest);
        let chegada = ger
            .porto()
            .map(|p| p.centro)
            .or_else(|| ger.cidade().map(|c| c.centro()))
            .unwrap_or(Vec2::ZERO);
        Some((dest, chegada))
    }

    /// Where docking at sea index `ilha` lands: (zone, name, arrival). (The
    /// Magic Island has no dock since 17/11/2026: menu only.)
    pub(crate) fn chegada_da_ilha(ilha: usize) -> Option<(&'static str, &'static str, Vec2)> {
        Self::porto_da_ilha(ilha).map(|(d, c)| (d.zona, d.nome, c))
    }

    /// DOCK: at sea, at island `ilha`'s anchorage, go ashore at its port.
    fn atracar(&mut self, sid: SessionId, ilha: usize) {
        if !self.no_mar() {
            return;
        }
        let Some(eu) = self.pos_do_casco(sid) else {
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
        // The story still opens the islands, by sea as by the Captain:
        // sailing there to look is free, going ashore is not.
        if !shared::viagem::liberada(ilha, indice) {
            let como = shared::viagem::passo_que_libera(ilha)
                .map(|(_, t)| format!("reach the story step \"{t}\""))
                .unwrap_or_else(|| "follow the story".into());
            self.avisa_missao(
                sid,
                format!("You haven't unlocked {} yet: {como}.", dest.nome),
            );
            return;
        }
        // A Colossus Chest on deck counts this island (phase 4).
        self.contar_atracacao(sid, ilha);
        let aviso = format!("You dock at {}.", dest.nome);
        // The crew comes ashore with the captain (the walkable deck).
        let tripulacao = if self.capitao_de(sid) == sid {
            self.tripulacao_de(sid)
        } else {
            Vec::new()
        };
        if self.mandar_para_zona(sid, dest.zona, chegada, Some(&aviso), Some(dest.nome)) {
            if let Some(s) = self.sessions.get_mut(&sid) {
                s.zona_volta.clear();
                s.conves = None;
            }
            for t in tripulacao {
                let pode = shared::viagem::liberada(
                    ilha,
                    self.sessions
                        .get(&t)
                        .and_then(|s| crate::quests::indice_da_historia(&s.quests)),
                );
                let aviso = format!("You come ashore at {} with your captain.", dest.nome);
                if pode
                    && self.mandar_para_zona(t, dest.zona, chegada, Some(&aviso), Some(dest.nome))
                {
                    if let Some(s) = self.sessions.get_mut(&t) {
                        s.zona_volta.clear();
                        s.conves = None;
                    }
                } else {
                    self.levar_ao_cais(t, "Your captain docked where you can't land: you are put ashore at the nearest dock.");
                }
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
            self.avisa_missao(
                sid,
                "You have no ship. Put one in the Ship slot of your equipment.".into(),
            );
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
            self.avisa_missao(
                sid,
                format!(
                    "The repair costs {custo} copper: {} copper short.",
                    custo - tem
                ),
            );
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

    /// The hull takes a bite. At zero it remains in the sea as a boardable
    /// wreck; sinking is a later explicit/timed transition.
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
            return;
        }
        let (marcado_antes, lado, vitima, casco_pct) = match self.sessions.get(&sid) {
            Some(s) => (
                self.marcado_de(s),
                self.lado_pk(s),
                s.entity_id,
                s.arvore.passivas().casco_pct,
            ),
            None => return,
        };
        let dmg = ((dmg.max(0) as f32) / (1.0 + casco_pct.max(0.0)))
            .round()
            .min(u16::MAX as f32) as u16;
        if dmg > 0 {
            // A hit on the hull interrupts any sail repair on it.
            self.para_reparo(sid, Some("Sail repair interrupted."));
        }
        let antes = d.casco;
        d.casco = d.casco.saturating_sub(dmg);
        let limiar = (nv::casco_total(id, &d) as f32 * nv::CRITICO) as u16;
        if antes > limiar && d.casco <= limiar && d.casco > 0 {
            self.avisa_missao(sid, "Hull critical: head back to a dock.".into());
        }
        let afundou = d.casco == 0;
        let tesouro_do_destroc = if afundou { d.conves.take() } else { None };
        if afundou {
            d.avariado = false;
        }
        self.guarda_navio(sid, d);
        if let Some(t) = tesouro_do_destroc {
            // The deck cargo leaves the defeated hull now: a PvP killer with
            // a free deck takes the Colossus Chest, otherwise it floats at
            // the wreck (`tesouro_perdido`). The wreck itself remains.
            let onde = self.pos_do_casco(sid).unwrap_or_else(fundeadouro_padrao);
            self.tesouro_perdido(sid, t, atacante, onde);
        }
        if afundou {
            if let Some(a) = atacante {
                self.registrar_morte_pk(a, vitima, lado, marcado_antes);
            }
            let navio = self
                .sessions
                .get(&sid)
                .and_then(|s| s.conves)
                .map(|c| c.navio);
            if let Some(e) = navio {
                let rumo = self.ecs.get::<&NavioEnt>(e).map_or(Vec2::Y, |n| n.rumo);
                if let Ok(mut n) = self.ecs.get::<&mut NavioEnt>(e) {
                    n.derrotado_ate = self.sim_time_s + nv::AFUNDAMENTO_S;
                    n.deriva = Vec2::new(-rumo.y, rumo.x) * nv::DERIVA_DERROTADO;
                    n.saqueado = false;
                }
                if let Ok(mut v) = self.ecs.get::<&mut Velocity>(e) {
                    v.0 = Vec2::ZERO;
                }
            }
            if let Some(s) = self.sessions.get_mut(&sid) {
                s.leme_do_mar.vel = 0.0;
                s.piloto = Default::default();
                s.rota.limpa();
                if let Some(c) = s.conves.as_mut() {
                    c.leme = false;
                }
            }
            self.avisa_missao(
                sid,
                "Hull defeated: repair the sail within 12 minutes or abandon ship.".into(),
            );
        }
    }

    fn navio_do_capitao(&self, sid: SessionId) -> Option<hecs::Entity> {
        self.ecs
            .query::<&NavioEnt>()
            .iter()
            .find(|(_, n)| n.dono == sid)
            .map(|(e, _)| e)
    }

    fn navio_derrotado(&self, e: hecs::Entity) -> bool {
        self.ecs
            .get::<&NavioEnt>(e)
            .is_ok_and(|n| n.derrotado(self.sim_time_s))
    }

    /// Give up or time out a wreck. Only here does the item become damaged,
    /// treasure float and the crew return to the dock.
    pub(crate) fn afundar_derrotado(&mut self, sid: SessionId, penalizar: bool) {
        let sid = self.capitao_de(sid);
        let Some(e) = self.navio_do_capitao(sid) else {
            return;
        };
        let ainda_destroc = self
            .ecs
            .get::<&NavioEnt>(e)
            .is_ok_and(|n| n.derrotado_ate > 0.0);
        if !ainda_destroc {
            return;
        }
        let onde = self
            .ecs
            .get::<&Position>(e)
            .map_or_else(|_| fundeadouro_padrao(), |p| p.0);
        let saqueado = self.ecs.get::<&NavioEnt>(e).map_or(true, |n| n.saqueado);
        if let Ok(mut n) = self.ecs.get::<&mut NavioEnt>(e) {
            n.derrotado_ate = 0.0;
            n.deriva = Vec2::ZERO;
        }
        let mut tesouro = None;
        if let Some((_, mut d)) = self.navio_equipado(sid) {
            d.avariado = true;
            d.casco = 0;
            tesouro = d.conves.take();
            self.guarda_navio(sid, d);
        }
        if let Some(t) = tesouro {
            self.tesouro_perdido(sid, t, None, onde);
        }
        if penalizar {
            self.aplicar_morte_naval(sid);
            for tripulante in self.tripulacao_de(sid) {
                self.aplicar_morte_naval(tripulante);
            }
        }
        if !saqueado {
            let nivel = self.nivel_de_sid(sid);
            let mut rng = fastrand::f32;
            let (ouro, tokens, fragmentos) = nv::saque_naval(nivel, &mut rng);
            self.poe_barril_do_mar(onde, ouro, tokens, fragmentos);
        }
        self.naufragio(sid);
    }

    pub(crate) fn aplicar_morte_naval(&mut self, sid: SessionId) {
        let Some(s) = self.sessions.get_mut(&sid) else {
            return;
        };
        let mult = crate::economy::xp_multiplier();
        let nivel = shared::level_of_xp_with_mult(s.xp, mult);
        let inicio = shared::xp_for_level_with_mult(nivel, mult);
        let no_nivel = s.xp.saturating_sub(inicio);
        let perda = ((no_nivel as f64) * 0.02).ceil() as u64;
        s.xp = s.xp.saturating_sub(perda).max(inicio);
        let _ = s.handle.to_client.send(ServerMessage::ProgressUpdate {
            xp: s.xp,
            level: shared::level_of_xp_with_mult(s.xp, mult),
        });
        s.inventory_dirty = true;
        crate::telemetria::conta_de(&s.name, "morte_naval", "", 1);
    }

    fn reparar_vela(&mut self, sid: SessionId, iniciar: bool) {
        if self.capitao_de(sid) != sid {
            self.avisa_missao(sid, "Only the captain repairs the sail.".into());
            return;
        }
        let Some(e) = self.navio_do_capitao(sid) else {
            return;
        };
        if !self.navio_derrotado(e) {
            self.avisa_missao(sid, "The sail is not torn.".into());
            return;
        }
        if !iniciar
            || self
                .sessions
                .get(&sid)
                .is_some_and(|s| s.reparando_navio.is_some())
        {
            self.para_reparo(sid, Some("Sail repair cancelled."));
            return;
        }
        let Some(c) = self.sessions.get(&sid).and_then(|s| s.conves) else {
            return;
        };
        let classe = self.ecs.get::<&NavioEnt>(e).map_or(0, |n| n.classe);
        let mastro = shared::conves::mastros(classe).next().unwrap_or(Vec2::ZERO);
        if c.local.distance(mastro) > 1.8 {
            self.avisa_missao(sid, "Stand by the broken mast to repair the sail.".into());
            return;
        }
        let agora = self.sim_time_s;
        let navio_eid = self.ecs.get::<&NetId>(e).ok().map(|n| n.0);
        if let Some(s) = self.sessions.get_mut(&sid) {
            s.reparando_vela_ate = agora + nv::REPARAR_VELA_S;
            s.reparando_navio = navio_eid;
            s.reparando_de = c.local;
            s.rota.limpa();
            s.piloto = Default::default();
            if let Some(cc) = s.conves.as_mut() {
                cc.leme = false;
            }
            s.leme_do_mar.vel = 0.0;
            let _ = s.handle.to_client.send(ServerMessage::ColetaEstado {
                tipo: nv::TIPO_COLETA_REPARO_VELA,
                intervalo_s: nv::REPARAR_VELA_S,
                progresso: 0.0,
                centro: None,
                pausado: false,
            });
        }
        self.avisa_missao(
            sid,
            "Repairing sail: stay by the mast and avoid damage for 45 seconds.".into(),
        );
    }

    /// Stops a sail repair and clears the client's channel bar.
    fn para_reparo(&mut self, sid: SessionId, aviso: Option<&str>) {
        let estava = self
            .sessions
            .get_mut(&sid)
            .map(|s| {
                s.reparando_vela_ate = 0.0;
                s.reparando_navio = None;
                s.reparando_de = Vec2::ZERO;
                s.handle.to_client.send(ServerMessage::ColetaEstado {
                    tipo: shared::protocol::COLETA_PARADA,
                    intervalo_s: 0.0,
                    progresso: 0.0,
                    centro: None,
                    pausado: false,
                })
            })
            .is_some();
        if estava {
            if let Some(m) = aviso {
                self.avisa_missao(sid, m.into());
            }
        }
    }

    /// LOOT A DEFEATED HULL: 10 s aboard the enemy wreck, interrupted by
    /// damage, death or leaving the deck. The reward is generated here (gold
    /// + Naval Tokens + fragments) and can be claimed once per wreck.
    fn saquear_navio_derrotado(&mut self, sid: SessionId) {
        let Some(c) = self.sessions.get(&sid).and_then(|s| s.conves) else {
            self.avisa_missao(sid, "Board the defeated ship to loot it.".into());
            return;
        };
        if self
            .sessions
            .get(&sid)
            .is_some_and(|s| s.saqueando_navio.is_some())
        {
            self.para_saque(sid, Some("Salvage cancelled."));
            return;
        }
        if !c.hostil && self.capitao_de(sid) == c.capitao {
            self.avisa_missao(sid, "This is your own ship.".into());
            return;
        }
        let Some(e) = self.navio_por_eid(c.navio_eid) else {
            return;
        };
        let Ok(n) = self.ecs.get::<&NavioEnt>(e) else {
            return;
        };
        if !n.derrotado(self.sim_time_s) {
            self.avisa_missao(sid, "That ship is not defeated.".into());
            return;
        }
        if n.saqueado {
            self.avisa_missao(sid, "This wreck has already been looted.".into());
            return;
        }
        let Some(p) = self.ecs.get::<&Position>(e).ok().map(|p| p.0) else {
            return;
        };
        if self.in_safe_zone(p) {
            self.avisa_missao(sid, "You cannot loot in the safe zone.".into());
            return;
        }
        let agora = self.sim_time_s;
        if let Some(s) = self.sessions.get_mut(&sid) {
            s.saqueando_navio_ate = agora + nv::SAQUE_NAVAL_S;
            s.saqueando_navio = Some(c.navio_eid);
            s.rota.limpa();
            let _ = s.handle.to_client.send(ServerMessage::ColetaEstado {
                tipo: nv::TIPO_COLETA_SAQUE,
                intervalo_s: nv::SAQUE_NAVAL_S,
                progresso: 0.0,
                centro: Some([p.x, p.y]),
                pausado: false,
            });
        }
        self.avisa_missao(
            sid,
            "Salvaging the wreck: stay aboard for 10 seconds.".into(),
        );
    }

    fn para_saque(&mut self, sid: SessionId, aviso: Option<&str>) {
        let estava = self
            .sessions
            .get_mut(&sid)
            .map(|s| {
                s.saqueando_navio_ate = 0.0;
                s.saqueando_navio = None;
                s.handle.to_client.send(ServerMessage::ColetaEstado {
                    tipo: shared::protocol::COLETA_PARADA,
                    intervalo_s: 0.0,
                    progresso: 0.0,
                    centro: None,
                    pausado: false,
                })
            })
            .is_some();
        if estava {
            if let Some(m) = aviso {
                self.avisa_missao(sid, m.into());
            }
        }
    }

    fn navio_por_eid(&self, eid: EntityId) -> Option<hecs::Entity> {
        self.ecs
            .query::<&NetId>()
            .iter()
            .find(|(_, n)| n.0 == eid)
            .map(|(e, _)| e)
    }

    /// NAVAL DEFEAT TICK: sail repairs, wreck salvage and the sinking timer.
    pub(crate) fn tick_naval(&mut self) {
        let agora = self.sim_time_s;
        let reparos: Vec<(SessionId, f32, EntityId, Vec2)> = self
            .sessions
            .iter()
            .filter_map(|(sid, s)| {
                (s.reparando_vela_ate > 0.0 && s.reparando_navio.is_some()).then(|| {
                    (
                        *sid,
                        s.reparando_vela_ate,
                        s.reparando_navio.unwrap(),
                        s.reparando_de,
                    )
                })
            })
            .collect();
        for (sid, ate, navio_eid, de) in reparos {
            let quebrou = {
                let Some(e) = self.navio_por_eid(navio_eid) else {
                    self.para_reparo(sid, Some("The wreck is gone."));
                    continue;
                };
                let c = self.sessions.get(&sid).and_then(|s| s.conves);
                let s = self.sessions.get(&sid);
                let navio_derrotado = self
                    .ecs
                    .get::<&NavioEnt>(e)
                    .is_ok_and(|n| n.derrotado(agora));
                s.is_none()
                    || s.is_some_and(|s| {
                        s.downed || s.hurt_until > agora - 0.25 || s.entity.is_none()
                    })
                    || !navio_derrotado
                    || c.is_none_or(|c| {
                        c.navio_eid != navio_eid || c.hostil || c.local.distance(de) > 0.35
                    })
            };
            if quebrou {
                self.para_reparo(sid, Some("Sail repair interrupted."));
                continue;
            }
            if agora < ate {
                continue;
            }
            self.para_reparo(sid, None);
            self.completa_reparo(sid, navio_eid);
        }

        let saques: Vec<(SessionId, f32, EntityId)> = self
            .sessions
            .iter()
            .filter_map(|(sid, s)| {
                (s.saqueando_navio_ate > 0.0 && s.saqueando_navio.is_some())
                    .then(|| (*sid, s.saqueando_navio_ate, s.saqueando_navio.unwrap()))
            })
            .collect();
        for (sid, ate, navio_eid) in saques {
            let quebrou = {
                let Some(e) = self.navio_por_eid(navio_eid) else {
                    self.para_saque(sid, Some("The wreck is gone."));
                    continue;
                };
                let c = self.sessions.get(&sid).and_then(|s| s.conves);
                let s = self.sessions.get(&sid);
                let navio = self.ecs.get::<&NavioEnt>(e).ok();
                s.is_none()
                    || s.is_some_and(|s| s.downed || s.hurt_until > agora - 0.25)
                    || c.is_none_or(|c| c.navio_eid != navio_eid)
                    || navio.is_none_or(|n| !n.derrotado(agora) || n.saqueado)
            };
            if quebrou {
                self.para_saque(sid, Some("Salvage interrupted."));
                continue;
            }
            if agora < ate {
                continue;
            }
            self.para_saque(sid, None);
            self.conclui_saque(sid, navio_eid);
        }

        self.tick_barris_do_mar();

        let vencidos: Vec<SessionId> = self
            .ecs
            .query::<&NavioEnt>()
            .iter()
            .filter(|(_, n)| n.derrotado_ate > 0.0 && agora >= n.derrotado_ate)
            .map(|(_, n)| n.dono)
            .collect();
        for dono in vencidos {
            self.afundar_derrotado(dono, true);
        }

        for s in self.sessions.values_mut() {
            if s.combate_naval_ate > 0.0
                && agora > s.combate_naval_ate
                && !s.downed
                && s.respawn_timer.is_none()
            {
                s.combate_naval_ate = 0.0;
                s.mortes_navais = 0;
            }
        }
    }

    fn completa_reparo(&mut self, sid: SessionId, navio_eid: EntityId) {
        let Some(e) = self.navio_por_eid(navio_eid) else {
            return;
        };
        let Some((id, mut d)) = self.navio_equipado(sid) else {
            return;
        };
        d.casco = ((nv::casco_total(id, &d) as f32 * nv::CASCO_APOS_REPARO).round() as u16).max(1);
        d.avariado = false;
        self.guarda_navio(sid, d);
        if let Ok(mut n) = self.ecs.get::<&mut NavioEnt>(e) {
            n.derrotado_ate = 0.0;
            n.deriva = Vec2::ZERO;
            n.saqueado = false;
        }
        if let Ok(mut v) = self.ecs.get::<&mut Velocity>(e) {
            v.0 = Vec2::ZERO;
        }
        if let Some(s) = self.sessions.get_mut(&sid) {
            s.leme_do_mar.vel = 0.0;
            s.piloto = Default::default();
        }
        self.envia_estado_navio(sid);
        self.send_chat_to(
            sid,
            &format!(
                "Sail patched: the hull holds at {}%. Sail to a dock for a full repair.",
                (nv::CASCO_APOS_REPARO * 100.0).round()
            ),
        );
        crate::telemetria::conta_de(&self.nome_de(sid), "reparo_vela_naval", id, 1);
        self.save_pending = true;
    }

    fn conclui_saque(&mut self, sid: SessionId, navio_eid: EntityId) {
        let Some(e) = self.navio_por_eid(navio_eid) else {
            return;
        };
        let pos = self.ecs.get::<&Position>(e).ok().map(|p| p.0);
        let dono = self.ecs.get::<&NavioEnt>(e).ok().map(|n| n.dono);
        if let Ok(mut n) = self.ecs.get::<&mut NavioEnt>(e) {
            n.saqueado = true;
        }
        let nivel = dono.map_or(1, |dono| self.nivel_de_sid(dono));
        let mut rng = fastrand::f32;
        let (ouro, tokens, fragmentos) = nv::saque_naval(nivel, &mut rng);
        let nome_dono = dono.map_or_else(|| "the crew".into(), |d| self.nome_de(d));
        let grupo = self.sessions.get(&sid).and_then(|s| s.party_id);
        let mut aliados: Vec<(SessionId, String)> = self
            .sessions
            .iter()
            .filter_map(|(aliado_sid, s)| {
                let no_grupo = *aliado_sid == sid || (grupo.is_some() && s.party_id == grupo);
                let perto = pos.is_some_and(|alvo| {
                    s.conves
                        .and_then(|c| self.ecs.get::<&Position>(c.navio).ok().map(|p| p.0))
                        .is_some_and(|p| p.distance(alvo) <= nv::ALCANCE_DIVISAO_SAQUE)
                });
                (s.logged_in && no_grupo && perto).then(|| (*aliado_sid, s.name.clone()))
            })
            .collect();
        aliados.sort_by(|a, b| a.1.cmp(&b.1));
        if !aliados.iter().any(|(aliado, _)| *aliado == sid) {
            aliados.push((sid, self.nome_de(sid)));
        }
        let total = aliados.len() as u64;
        for (i, (aliado, _)) in aliados.iter().enumerate() {
            let parte = |valor: u64| valor / total + u64::from((i as u64) < valor % total);
            let (ouro_parte, tokens_parte, fragmentos_parte) = (
                parte(ouro),
                parte(tokens as u64) as u32,
                parte(fragmentos as u64) as u32,
            );
            self.pagar_saque(*aliado, ouro_parte, tokens_parte, fragmentos_parte);
            self.send_chat_to(
                *aliado,
                &format!(
                    "Alliance share from {nome_dono}'s wreck: {ouro_parte} gold, {tokens_parte} Naval Token(s) and {fragmentos_parte} naval fragment(s)."
                ),
            );
        }
        crate::telemetria::conta_de(&self.nome_de(sid), "saque_naval", navio_eid.0 as u16, 1);
    }

    fn pagar_saque(&mut self, sid: SessionId, ouro: u64, tokens: u32, fragmentos: u32) -> bool {
        let quando = (now_ms() / 1000) as i64;
        let Some(s) = self.sessions.get_mut(&sid) else {
            return false;
        };
        s.gold = s.gold.saturating_add(ouro);
        if tokens > 0 && !add_to_inventory(&mut s.inventory, item_id::NAVAL_TOKEN, tokens, None) {
            s.dungeon
                .postar(item_id::NAVAL_TOKEN, tokens, None, 0, quando);
        }
        if fragmentos > 0
            && !add_to_inventory(&mut s.inventory, item_id::FRAGMENTO_NAVAL, fragmentos, None)
        {
            s.dungeon
                .postar(item_id::FRAGMENTO_NAVAL, fragmentos, None, 0, quando);
        }
        s.inventory_dirty = true;
        self.save_pending = true;
        true
    }

    fn poe_barril_do_mar(&mut self, p: Vec2, ouro: u64, tokens: u32, fragmentos: u32) {
        let eid = self.alloc_entity_id();
        self.ecs.spawn((
            NetId(eid),
            Position(self.agua_no_mar(p)),
            Velocity(Vec2::ZERO),
            EntityKind::Npc(7),
            NpcDaVilaTag {
                nome: "Salvage Barrel".into(),
                rumo: shared::npc_kind(None, nv::PAPEL_BARRIL_NAVAL),
                giver: None,
            },
            BarrilDoMarTag {
                ouro,
                tokens,
                fragmentos,
                some_em: self.sim_time_s + nv::BARRIL_DURA_S,
            },
        ));
    }

    pub(crate) fn tocar_barril_do_mar(&mut self, sid: SessionId, entidade: hecs::Entity) {
        if self
            .sessions
            .get(&sid)
            .is_some_and(|s| s.saqueando_barril.is_some())
        {
            self.parar_saque_barril(sid);
            return;
        }
        let Some(p) = self.ecs.get::<&Position>(entidade).ok().map(|p| p.0) else {
            return;
        };
        let Some(barril) = self.ecs.get::<&NetId>(entidade).ok().map(|n| n.0) else {
            return;
        };
        let Some(eu) = self.pos_do_casco(sid) else {
            return;
        };
        if eu.distance(p) > nv::ALCANCE_DO_TESOURO {
            self.avisa_missao(sid, "Sail closer to the salvage barrel.".into());
            return;
        }
        if let Some(s) = self.sessions.get_mut(&sid) {
            if s.downed || s.entity.is_none() {
                return;
            }
            s.saqueando_barril = Some(forte::AberturaDeBau {
                bau: barril,
                desde: self.sim_time_s,
                de: eu,
            });
            s.rota.limpa();
            let _ = s.handle.to_client.send(ServerMessage::ColetaEstado {
                tipo: nv::TIPO_COLETA_SAQUE,
                intervalo_s: nv::BARRIL_COLETA_S,
                progresso: 0.0,
                centro: Some([p.x, p.y]),
                pausado: false,
            });
        }
        self.avisa_missao(
            sid,
            "Salvaging the barrel: hold position and avoid damage for 10 seconds.".into(),
        );
    }

    fn parar_saque_barril(&mut self, sid: SessionId) {
        if let Some(s) = self.sessions.get_mut(&sid) {
            if s.saqueando_barril.take().is_some() {
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

    fn tick_barris_do_mar(&mut self) {
        let agora = self.sim_time_s;
        let vencidos: Vec<hecs::Entity> = self
            .ecs
            .query::<&BarrilDoMarTag>()
            .iter()
            .filter(|(_, b)| agora >= b.some_em)
            .map(|(e, _)| e)
            .collect();
        for e in vencidos {
            if let Ok(eid) = self.ecs.get::<&NetId>(e).map(|n| n.0) {
                self.removed_this_tick.push(eid);
            }
            let _ = self.ecs.despawn(e);
        }

        let saques: Vec<(SessionId, forte::AberturaDeBau)> = self
            .sessions
            .iter()
            .filter_map(|(sid, s)| s.saqueando_barril.map(|a| (*sid, a)))
            .collect();
        for (sid, abertura) in saques {
            let barril = self
                .ecs
                .query::<(&NetId, &Position, &BarrilDoMarTag)>()
                .iter()
                .find(|(_, (n, _, _))| n.0 == abertura.bau)
                .map(|(e, (_, p, b))| (e, p.0, *b));
            let Some((e, p, b)) = barril else {
                self.parar_saque_barril(sid);
                continue;
            };
            let eu = self.pos_do_casco(sid);
            let quebrou = self
                .sessions
                .get(&sid)
                .is_none_or(|s| s.downed || s.entity.is_none() || s.hurt_until > agora - 0.25)
                || eu.is_none_or(|eu| {
                    eu.distance(abertura.de) > 1.0 || eu.distance(p) > nv::ALCANCE_DO_TESOURO
                });
            if quebrou {
                self.parar_saque_barril(sid);
                continue;
            }
            if agora - abertura.desde < nv::BARRIL_COLETA_S {
                continue;
            }
            self.parar_saque_barril(sid);
            if !self.pagar_saque(sid, b.ouro, b.tokens, b.fragmentos) {
                continue;
            }
            self.removed_this_tick.push(abertura.bau);
            let _ = self.ecs.despawn(e);
            crate::telemetria::conta_de(&self.nome_de(sid), "barril_naval", "", 1);
            self.send_chat_to(
                sid,
                &format!(
                    "Salvage barrel: {} gold, {} Naval Token(s) and {} naval fragment(s).",
                    b.ouro, b.tokens, b.fragmentos
                ),
            );
        }
    }

    /// THE HULL GAVE WAY: back to the dock this character sailed from (or
    /// the nearest island that is up). If no island is up, they stay at
    /// sea, at the nearest anchorage — a `TrocarZona` to a dead host is the
    /// one outcome that cannot happen.
    pub(crate) fn naufragio(&mut self, sid: SessionId) {
        // Everyone aboard goes to the dock (the walkable deck).
        let tripulacao = if self.capitao_de(sid) == sid {
            self.tripulacao_de(sid)
        } else {
            Vec::new()
        };
        self.levar_ao_cais(
            sid,
            "Your hull gave way. You wash up at the dock: the Harbour Captain can repair the ship.",
        );
        for t in tripulacao {
            self.levar_ao_cais(t, "Your ship went down. You wash up at the dock.");
        }
    }

    /// The sea's tick: the bites of this tick on the hulls, and the chests
    /// that come back.
    pub(crate) fn tick_mar(&mut self) {
        // The walkable deck: ships, crews, launches (`world/conves.rs`).
        self.tick_conves();
        // Hulls whose captains boarded elsewhere or disconnected keep their
        // own route, defend against recent aggressors and return to port.
        self.tick_navios_autonomos();
        if !self.no_mar() {
            self.mordidas_no_casco.clear();
            return;
        }
        let mordidas = std::mem::take(&mut self.mordidas_no_casco);
        for (eid, dmg) in mordidas {
            let Some(sid) = self
                .sessions
                .values()
                .find(|s| s.entity_id == eid)
                .map(|s| s.handle.id)
            else {
                continue;
            };
            // A bite on anyone aboard is a bite on THEIR ship's hull.
            let sid = self.capitao_de(sid);
            if self.pos_do_casco(sid).is_some_and(shared::mar::no_corredor) {
                continue;
            }
            self.dano_no_casco(sid, dmg, None);
        }
        self.tick_tesouros();
        // Gameplay remake: the cannonballs in the air.
        self.pousar_balas();
        // NAVAL DEFEAT: sail repairs, wreck salvage and sinking timers.
        self.tick_naval();
        // AUTO COMBAT AT SEA: the server's pilot (`piloto_do_mar`).
        self.tick_piloto();
        let agora = self.sim_time_s;
        let (prontos, esperando): (Vec<_>, Vec<_>) = std::mem::take(&mut self.baus_do_mar)
            .into_iter()
            .partition(|(_, t)| *t <= agora);
        self.baus_do_mar = esperando;
        for (p, _) in prontos {
            self.poe_bau_do_mar(p);
        }
        self.enviar_conves();
    }

    fn poe_bau_do_mar(&mut self, p: Vec2) {
        let eid = self.alloc_entity_id();
        self.ecs.spawn((
            NetId(eid),
            Position(p),
            Velocity(Vec2::ZERO),
            EntityKind::Npc(7),
            BauDoMarTag,
        ));
    }

    /// Peoples THE OPEN SEA: mob nests along the routes and the drifting
    /// chests. The sibling of `povoar_ilha`; on the sea the places come from
    /// the routes, because the routes are the map.
    pub fn povoar_mar(&mut self) {
        // Scattered over the whole sea (20/11/2026): ONE mob per point of
        // `mar::pontos_dos_bichos`, each its own zone, so nothing clumps.
        const RAIO: f32 = 9.0;
        let mut zonas = Vec::new();
        for (k, centro) in shared::mar::pontos_dos_bichos().iter().copied().enumerate() {
            let slots = vec![SpawnSlot {
                pos: centro,
                occupant: None,
                respawn_at: 0.0,
            }];
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
                level_range: Some((nivel, (nivel + 2).min(120), n)),
                level_range_live: 0,
                level_range_queue: (0..n).map(|_| 0.0_f32).collect(),
                slots,
                active: false,
                last_player_near_at: -1e9,
                forte: false,
            });
        }
        let baus = shared::mar::baus();
        tracing::info!(
            "mar aberto: {} ninhos, {} baus a deriva",
            zonas.len(),
            baus.len()
        );
        // The water A*'s grid, built at boot rather than on the first tap.
        let t0 = std::time::Instant::now();
        let livres = grade_do_mar().livre.iter().filter(|l| **l).count();
        tracing::info!(
            "mar aberto: grade de rota {} celulas livres em {:?}",
            livres,
            t0.elapsed()
        );
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
        let Some(eu) = self.pos_do_casco(sid) else {
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
        self.baus_do_mar
            .push((pos, self.sim_time_s + RESPAWN_DO_BAU_S));
        self.save_pending = true;
    }

    // ───────────────────── phase 2: the Shipwright ─────────────────────

    /// The level of the character behind `sid`.
    pub(crate) fn nivel_de_sid(&self, sid: SessionId) -> u32 {
        self.sessions.get(&sid).map_or(1, |s| {
            shared::level_of_xp_with_mult(s.xp, crate::economy::xp_multiplier())
        })
    }

    /// BUILD THE NEXT HULL at the Shipwright (the Harbour Captain): the
    /// ship in the Ship slot is consumed with the recipe's materials, and
    /// the new class takes its place — hull full, stars and cosmetics
    /// carried over.
    fn construir_casco(&mut self, sid: SessionId) {
        if !self.perto_do_capitao_do_porto(sid) {
            self.avisa_missao(
                sid,
                "The Shipwright works at the dock: talk to the Harbour Captain.".into(),
            );
            return;
        }
        let Some((id, d)) = self.navio_equipado(sid) else {
            self.avisa_missao(
                sid,
                "You have no ship. Put one in the Ship slot of your equipment.".into(),
            );
            return;
        };
        let Some(r) = nv::receita_depois_de(id) else {
            self.avisa_missao(
                sid,
                "Your ship is already the finest hull the Shipwright builds.".into(),
            );
            return;
        };
        let Some(classe) = nv::casco(r.para) else {
            return;
        };
        // Reborn characters pass every level lock (owner, 17/11/2026: "kuna
        // is R55 so it needs to be counted"), like quests (`nivel_de_missao`).
        let renasc = self.sessions.get(&sid).map_or(0, |s| s.renascimentos);
        let nivel = shared::quests::nivel_de_missao(self.nivel_de_sid(sid), renasc);
        if nivel < classe.nivel {
            self.avisa_missao(
                sid,
                format!(
                    "The next hull needs level {}.",
                    shared::progressao::texto_de_nivel(classe.nivel)
                ),
            );
            return;
        }
        let Some(s) = self.sessions.get(&sid) else {
            return;
        };
        if r.materiais
            .iter()
            .any(|(item, q)| crate::craft::tem(&s.inventory, *item) < *q)
        {
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

    fn construir_tipo(&mut self, sid: SessionId, tipo: u8) {
        if !self.perto_do_capitao_do_porto(sid) {
            self.avisa_missao(sid, "The Shipwright works at the dock.".into());
            return;
        }
        let Some(tipo) = nv::TipoBarco::de_id(tipo) else {
            return;
        };
        let receita = nv::receita_tipo(tipo);
        let Some(s) = self.sessions.get(&sid) else {
            return;
        };
        if receita
            .iter()
            .any(|(item, q)| crate::craft::tem(&s.inventory, *item) < *q)
        {
            self.avisa_missao(sid, "Not enough materials for this ship.".into());
            return;
        }
        let no_slot = s.equipment.navio.is_none();
        if !no_slot && !s.inventory.iter().any(|slot| slot.qty == 0) {
            self.avisa_missao(sid, "Make room in your bag for the new ship.".into());
            return;
        }
        let inst = nv::instancia_nova_tipo(tipo, 1);
        let s = self.sessions.get_mut(&sid).unwrap();
        for (item, q) in receita {
            crate::craft::consumir(&mut s.inventory, item, q);
        }
        if no_slot {
            s.equipment
                .set(EquipSlot::Navio, Some(NAVIO_CHALUPA), Some(inst));
            s.stats_dirty = true;
        } else {
            let slot = s.inventory.iter_mut().find(|slot| slot.qty == 0).unwrap();
            *slot = shared::InventorySlot {
                item_id: NAVIO_CHALUPA,
                qty: 1,
                instance: Some(inst),
            };
        }
        s.inventory_dirty = true;
        crate::telemetria::conta_de(&s.name, "navio_tipo_construido", tipo.id() as u16, 1);
        self.save_pending = true;
        self.envia_estado_navio(sid);
        self.send_chat_to(sid, &format!("{} T1 launched.", tipo.nome()));
    }

    fn alocar_navio(&mut self, sid: SessionId, pontos: [u8; 4]) {
        if !self.perto_do_capitao_do_porto(sid) {
            self.avisa_missao(
                sid,
                "Visit the Shipwright to change your ship's build.".into(),
            );
            return;
        }
        let Some((id, mut d)) = self.navio_equipado(sid) else {
            return;
        };
        if !nv::alocacao_valida(d.pontos, pontos, d.tier) {
            self.avisa_missao(
                sid,
                "Invalid build: points only increase after allocation; use Respec to move them."
                    .into(),
            );
            return;
        }
        let antes = nv::casco_total(id, &d).max(1) as f32;
        let fracao = d.casco as f32 / antes;
        d.pontos = pontos;
        d.casco = if d.avariado {
            0
        } else {
            (nv::casco_total(id, &d) as f32 * fracao).round() as u16
        };
        self.guarda_navio(sid, d);
        self.envia_estado_navio(sid);
        self.send_chat_to(sid, "Ship build applied.");
    }

    fn respec_navio(&mut self, sid: SessionId) {
        if !self.perto_do_capitao_do_porto(sid) {
            self.avisa_missao(
                sid,
                "Visit the Shipwright to reset your ship's build.".into(),
            );
            return;
        }
        let Some((id, mut d)) = self.navio_equipado(sid) else {
            return;
        };
        let custo = nv::custo_respec(d.tier);
        let Some(s) = self.sessions.get(&sid) else {
            return;
        };
        if d.pontos == [0; 4] {
            self.avisa_missao(sid, "This ship has no allocated points.".into());
            return;
        }
        if s.gold < custo {
            self.avisa_missao(sid, format!("Respec costs {custo} gold."));
            return;
        }
        let antes = nv::casco_total(id, &d).max(1) as f32;
        let fracao = d.casco as f32 / antes;
        self.sessions.get_mut(&sid).unwrap().gold -= custo;
        d.pontos = [0; 4];
        d.casco = if d.avariado {
            0
        } else {
            (nv::casco_total(id, &d) as f32 * fracao).round() as u16
        };
        self.guarda_navio(sid, d);
        self.envia_estado_navio(sid);
        self.send_chat_to(sid, &format!("Ship points reset for {custo} gold."));
    }

    fn upgrade_tier_navio(&mut self, sid: SessionId) {
        if !self.perto_do_capitao_do_porto(sid) {
            self.avisa_missao(sid, "The Shipwright upgrades ships at the dock.".into());
            return;
        }
        let Some((_id, mut d)) = self.navio_equipado(sid) else {
            return;
        };
        let Some(tipo) = nv::tipo_novo(&d) else {
            return;
        };
        let Some(custo) = nv::custo_upgrade(d.tier) else {
            self.avisa_missao(sid, "This ship is already T4.".into());
            return;
        };
        let Some(s) = self.sessions.get(&sid) else {
            return;
        };
        // No duplicate ship: the upgrade takes the materials the duplicate
        // would have cost (`nv::materiais_upgrade`) plus the gold.
        let materiais = nv::materiais_upgrade(tipo, d.tier);
        if materiais
            .iter()
            .any(|(item, q)| crate::craft::tem(&s.inventory, *item) < *q)
        {
            self.avisa_missao(sid, "Not enough materials for this upgrade.".into());
            return;
        }
        if s.gold < custo {
            self.avisa_missao(sid, format!("This upgrade costs {custo} gold."));
            return;
        }
        let s = self.sessions.get_mut(&sid).unwrap();
        s.gold -= custo;
        for (item, q) in &materiais {
            crate::craft::consumir(&mut s.inventory, *item, *q);
        }
        s.inventory_dirty = true;
        d.tier += 1;
        d.casco = nv::stats_finais_novo(tipo, d.tier, d.pontos).hp.round() as u16;
        d.avariado = false;
        let mut inst = nv::instancia_nova_tipo(tipo, d.tier);
        inst.navio = Some(d);
        s.equipment
            .set(EquipSlot::Navio, Some(NAVIO_CHALUPA), Some(inst));
        s.stats_dirty = true;
        crate::telemetria::conta_de(&s.name, "navio_upgrade_tier", d.tier as u16, 1);
        self.save_pending = true;
        self.envia_estado_navio(sid);
        self.send_chat_to(sid, &format!("{} upgraded to T{}.", tipo.nome(), d.tier));
    }

    /// ONE STAR on an axis at the Shipwright: copper, Darksteel and Wreck
    /// Planks, always succeeds. The hull star raises the hull you have by
    /// what it adds (a damaged hull stays as damaged as it was).
    fn melhorar_navio(&mut self, sid: SessionId, eixo: u8) {
        if !self.perto_do_capitao_do_porto(sid) {
            self.avisa_missao(
                sid,
                "The Shipwright works at the dock: talk to the Harbour Captain.".into(),
            );
            return;
        }
        let Some(eixo) = nv::Eixo::de_u8(eixo) else {
            return;
        };
        let Some((id, mut d)) = self.navio_equipado(sid) else {
            self.avisa_missao(
                sid,
                "You have no ship. Put one in the Ship slot of your equipment.".into(),
            );
            return;
        };
        if nv::tipo_novo(&d).is_some() {
            self.avisa_missao(
                sid,
                "This ship uses stat points. Allocate them in the Upgrade panel.".into(),
            );
            return;
        }
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
        crate::craft::consumir(
            &mut s.inventory,
            item_id::PRANCHA_DE_NAUFRAGIO,
            custo.pranchas,
        );
        s.inventory_dirty = true;
        crate::telemetria::conta_de(&s.name, "estrela_do_navio", eixo.nome(), 1);
        let antes = nv::casco_total(id, &d);
        d.estrelas[eixo as usize] = atual + 1;
        if eixo == nv::Eixo::Casco && !d.avariado {
            d.casco = d
                .casco
                .saturating_add(nv::casco_total(id, &d).saturating_sub(antes));
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
        // Keyed by the SHIP's entity (the walkable deck): the bodies aboard
        // are drawn as themselves.
        self.sessions
            .values()
            .filter(|s| s.logged_in)
            .filter_map(|s| {
                let c = s.conves.filter(|c| c.capitao == s.handle.id)?;
                let id = s.equipment.navio.filter(|i| item_id::e_navio(*i))?;
                let d = nv::dados(s.equipment.navio_inst.as_ref(), id);
                Some((c.navio_eid, nv::Visual::de(id, &d).no_fio()))
            })
            .collect()
    }

    /// The ship's speed at sea for this session: class × sail stars, and the
    /// Smoke's burst and Chain Shot's slow, clamped under the wire.
    pub(crate) fn velocidade_no_mar(s: &Session, agora: f32) -> f32 {
        let id = s
            .equipment
            .navio
            .filter(|i| item_id::e_navio(*i))
            .unwrap_or(NAVIO_CHALUPA);
        let d = nv::dados(s.equipment.navio_inst.as_ref(), id);
        let v = nv::velocidade_do_navio(id, &d);
        let v = if agora < s.fumaca_ate {
            (v * nv::FUMACA_MULT).min(nv::VELOCIDADE_MAX)
        } else {
            v
        };
        if agora < s.corrente_ate {
            v * nv::CORRENTE_MULT
        } else {
            v
        }
    }

    /// The helm's numbers for this session's ship (turning inertia): top
    /// speed without the Smoke, the most it turns (rad/s) and the seconds
    /// to full speed.
    pub(crate) fn manobra_no_mar(s: &Session) -> (f32, f32, f32) {
        let id = s
            .equipment
            .navio
            .filter(|i| item_id::e_navio(*i))
            .unwrap_or(NAVIO_CHALUPA);
        let d = nv::dados(s.equipment.navio_inst.as_ref(), id);
        (
            nv::velocidade_do_navio(id, &d),
            nv::giro_max(id, &d),
            nv::arranque_do_navio(id, &d),
        )
    }

    /// Where this session's hull points: its helm's heading once it has
    /// sailed, else its velocity (zero = no heading, fires all round).
    pub(crate) fn rumo_do_casco(&self, sid: SessionId, entity: hecs::Entity) -> Vec2 {
        // Aboard: the heading of the ship under this session (its captain's
        // helm).
        let sid = self.capitao_de(sid);
        self.sessions
            .get(&sid)
            .and_then(|s| s.leme_do_mar.rumo())
            .unwrap_or_else(|| {
                self.ecs
                    .get::<&Velocity>(entity)
                    .map(|v| v.0)
                    .unwrap_or(Vec2::ZERO)
            })
    }

    // ───────────────────── gameplay remake: auto path and mobs ─────────────────────

    /// A TAP AT SEA (`handle_mover_para`): the water A* plans the whole
    /// crossing, the same `SeguidorDeRota` steers the hull and the same
    /// `ServerMessage::Rota` draws the dashed line — the land's auto path,
    /// on water. No distance cap: the chart is one crossing.
    pub(crate) fn rota_no_mar(&mut self, sid: SessionId, de: Vec2, para: Vec2) {
        // A PK captain: the peaceful waters are closed (`aguas_pacificas.rs`).
        let barrado = self.sessions.get(&sid).is_some_and(|s| self.pk_barrado(s));
        if barrado && shared::mar::aguas_pacificas(para) {
            self.avisa_missao(sid, super::aguas_pacificas::ROTA_RECUSADA.into());
            return;
        }
        let Some(rota) = caminho_no_mar_com(de, para, barrado) else {
            self.avisa_missao(sid, "There is no water route there.".into());
            return;
        };
        if let Some(s) = self.sessions.get_mut(&sid) {
            s.rota = shared::terreno::SeguidorDeRota::nova(rota, para).com_folga(nv::FOLGA_DA_ROTA);
            s.rota_geracao = s.rota_geracao.wrapping_add(1);
            s.travado_desde = None;
        }
    }

    /// A SEA MOB JUST SPAWNED: the sea's tougher hide on its life
    /// (`navio::VIDA_DO_BICHO_DO_MAR`), and the elite on top when it is the
    /// route's elite slot.
    pub(crate) fn preparar_bicho_do_mar(&mut self, eid: EntityId, elite: bool) {
        for (_, (n, tag, hp)) in self.ecs.query_mut::<(&NetId, &mut EnemyTag, &mut Health)>() {
            if n.0 != eid {
                continue;
            }
            let vida = ((hp.max as f32) * nv::VIDA_DO_BICHO_DO_MAR).round() as i32;
            hp.max = vida;
            hp.current = vida;
            tag.stats.hp_max = vida;
            break;
        }
        if elite {
            self.tornar_elite_do_mar(eid);
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
        let (dono, entity, selecionado, conves) =
            (s.entity_id, s.entity.unwrap(), s.target, s.conves);
        // The guns fire from the HELM (the walkable deck): the captain at
        // the wheel, or the AUTO pilot flying from it.
        match conves {
            Some(c) if c.capitao != sid => {
                self.rejeita_skill(sid, id, "Only the captain fires the ship's guns.");
                return;
            }
            Some(c) if !c.leme => {
                self.rejeita_skill(sid, id, "Take the helm to fire the ship's guns.");
                return;
            }
            _ => {}
        }
        let Some((navio, mut d)) = self.navio_equipado(sid) else {
            self.rejeita_skill(
                sid,
                id,
                "You have no ship. Put one in the Ship slot of your equipment.",
            );
            return;
        };
        if !nv::pode_zarpar(&d) {
            self.rejeita_skill(
                sid,
                id,
                "Your ship is damaged. The Harbour Captain can repair it.",
            );
            return;
        }
        let Some(de) = self.pos_do_casco(sid) else {
            return;
        };
        let rumo = self.rumo_do_casco(sid, entity);
        let nivel = self.nivel_de_sid(sid);
        let mut miras: Vec<Vec2> = Vec::new();
        match hab.efeito {
            nv::Efeito::Bordada | nv::Efeito::Canhonaco | nv::Efeito::Corrente => {
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
                    let alvo_navio = self.sessions.values().any(|s| s.entity_id == alvo);
                    if alvo_navio && self.pvp_em_aguas_pacificas(dono, alvo) {
                        self.rejeita_skill(sid, id, super::aguas_pacificas::SEM_PVP);
                        return;
                    }
                    match self.posicao_alvo_skill(dono, alvo, de, &forma) {
                        // Every ship gun is on the sides (16/11/2026).
                        Ok(p) if !nv::no_costado(de, rumo, p) => {
                            self.rejeita_skill(sid, id, "Turn your side to the target to fire.");
                            return;
                        }
                        Ok(p) => vec![(alvo, p)],
                        Err(m) => {
                            self.rejeita_skill(sid, id, m);
                            return;
                        }
                    }
                };
                self.quebrar_invisibilidade(sid);
                // The Corsair's cannon passive (phase 4).
                let canhao = self
                    .sessions
                    .get(&sid)
                    .map_or(0.0, |s| s.arvore.passivas().canhao_pct);
                let dano = (nv::dano_do_canhao(hab, nivel, navio, &d) as f32
                    * (1.0 + canhao.max(0.0)))
                .round() as i32;
                // THE BALLS FLY (gameplay remake): aimed where the target
                // will be when they land, resolved on landing
                // (`pousar_balas`). The FX goes out per ball, so the screen
                // shows every shot the server is going to resolve.
                //
                // PvP naval (18/11/2026): the Broadside is a FAN of five
                // balls per target (each a fifth of the damage); the Long
                // Gun flies fast; the Chain Shot's ball is marked so its
                // hull hit slows the enemy ship.
                let corrente = hab.efeito == nv::Efeito::Corrente;
                for (alvo, p) in alvos {
                    let mira =
                        nv::mira_com_avanco_da_skill(hab, de, p, self.velocidade_da_entidade(alvo));
                    if hab.efeito == nv::Efeito::Bordada {
                        let dano_bola = (dano / nv::LEQUE_DA_BORDADA as i32).max(1);
                        let base = (mira - de).y.atan2((mira - de).x);
                        let alcance = de.distance(mira);
                        for ab in nv::aberturas_do_leque() {
                            let (s, c) = (base + ab).sin_cos();
                            let m = de + Vec2::new(c, s) * alcance;
                            miras.push(m);
                            self.disparar_bala(dono, alvo, de, m, dano_bola, false, false);
                        }
                    } else {
                        miras.push(mira);
                        let rapida = hab.efeito == nv::Efeito::Canhonaco;
                        self.disparar_bala(dono, alvo, de, mira, dano, corrente, rapida);
                    }
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
                d.casco = d
                    .casco
                    .saturating_add(nv::cura_do_reparo(navio, &d))
                    .min(cheio);
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
        // No target (a Broadside into open water, a repair, the smoke): one
        // FX at the ship itself, and the client fires its volley at nothing.
        if miras.is_empty() {
            miras.push(de);
        }
        for s in self.sessions.values().filter(|s| s.logged_in) {
            for (k, mira) in miras.iter().enumerate() {
                let _ = s.handle.to_client.send(ServerMessage::SkillImpactFx {
                    skill_id: id,
                    caster_eid: dono,
                    caster_pos: de,
                    target_pos: *mira,
                    tier: k.min(255) as u8,
                });
            }
        }
    }

    /// The velocity of entity `eid` (zero when it is not found). Only for the
    /// few targets of a shot, so a scan is fine.
    fn velocidade_da_entidade(&self, eid: EntityId) -> Vec2 {
        // Someone aboard moves with their ship.
        if let Some(sid) = self
            .sessions
            .values()
            .find(|s| s.entity_id == eid && s.conves.is_some())
            .map(|s| s.handle.id)
        {
            return self.vel_do_casco(sid);
        }
        self.ecs
            .query::<(&NetId, &Velocity)>()
            .iter()
            .find(|(_, (n, _))| n.0 == eid)
            .map_or(Vec2::ZERO, |(_, (_, v))| v.0)
    }

    /// A cannonball leaves `de` for `mira`, meant for `alvo`. It is an ECS
    /// entity WITHOUT `NetId`, so no snapshot ever carries it: the clients
    /// draw it from the `SkillImpactFx`, with the same flight time. `rapida`
    /// is the Long Gun's flat shot; `corrente` (the Chain Shot) slows the
    /// ship whose hull it hits.
    pub(crate) fn disparar_bala(
        &mut self,
        dono: EntityId,
        alvo: EntityId,
        de: Vec2,
        mira: Vec2,
        dano: i32,
        corrente: bool,
        rapida: bool,
    ) {
        let dist = de.distance(mira);
        let voo = if rapida {
            nv::tempo_de_voo_rapido(dist)
        } else {
            nv::tempo_de_voo(dist)
        };
        let chega_em = self.sim_time_s + voo;
        self.ecs.spawn((BalaDeCanhao {
            dono,
            alvo,
            de,
            mira,
            dano,
            chega_em,
            corrente,
        },));
    }

    /// THE BALLS THAT LANDED THIS TICK. A ball hits its target when the
    /// target is still within `navio::RAIO_DO_ACERTO` of where it lands (a
    /// ship adds its hull radius); otherwise it is a splash and nothing
    /// happens — the target sailed out of it.
    pub(crate) fn pousar_balas(&mut self) {
        let agora = self.sim_time_s;
        let mut prontas: Vec<(hecs::Entity, BalaDeCanhao)> = Vec::new();
        for (e, b) in self.ecs.query::<&BalaDeCanhao>().iter() {
            if b.chega_em <= agora {
                prontas.push((e, *b));
            }
        }
        if prontas.is_empty() {
            return;
        }
        for (e, _) in &prontas {
            let _ = self.ecs.despawn(*e);
        }
        let a_bordo = self.alvos_no_mar();
        for (_, b) in prontas {
            // Someone aboard: the ball hits if it lands on (or by) their
            // ship's hull.
            if let Some(a) = a_bordo.get(&b.alvo) {
                let casco = super::conves::casco_mais_perto(a, b.mira);
                if nv::acertou(b.mira, casco, 0.0) {
                    let bateu = self.tiro_de_canhao(b.dono, b.alvo, b.de, casco, b.dano);
                    if b.corrente && bateu {
                        self.corrente_no_casco(b.alvo);
                    }
                }
                continue;
            }
            let corpo = self
                .ecs
                .query::<(&NetId, &Position, &EntityKind)>()
                .iter()
                .find(|(_, (n, _, _))| n.0 == b.alvo)
                .map(|(_, (_, p, k))| (p.0, matches!(k, EntityKind::Player)));
            let Some((onde, navio)) = corpo else {
                continue;
            };
            let raio = if navio { nv::RAIO_DO_CASCO } else { 0.6 };
            if nv::acertou(b.mira, onde, raio) {
                let bateu = self.tiro_de_canhao(b.dono, b.alvo, b.de, onde, b.dano);
                if b.corrente && navio && bateu {
                    self.corrente_no_casco(b.alvo);
                }
            }
        }
    }

    /// CHAIN SHOT ON A HULL (`nv::Efeito::Corrente`): the ship under `alvo`
    /// (whoever of the crew was aimed at) sails at `nv::CORRENTE_MULT` for
    /// `nv::CORRENTE_S`. Called only when the ball's damage already went in
    /// (`tiro_de_canhao` returned true), so PK modes and safe zones rule it.
    fn corrente_no_casco(&mut self, alvo: EntityId) {
        let Some(vsid) = self
            .sessions
            .values()
            .find(|s| s.entity_id == alvo)
            .map(|s| s.handle.id)
        else {
            return;
        };
        let vsid = self.capitao_de(vsid);
        let ate = self.sim_time_s + nv::CORRENTE_S;
        if let Some(s) = self.sessions.get_mut(&vsid) {
            s.corrente_ate = ate.max(s.corrente_ate);
        }
    }

    /// One cannonball from `dono` at `alvo`, LANDED. A mob takes it as a skill hit
    /// (defence, the level gap and the kill credit as usual); a player's
    /// HULL takes it directly — hull against hull — when the PK modes allow.
    /// Returns true when a player's hull took the hit (the Chain Shot's slow
    /// rides on exactly that).
    fn tiro_de_canhao(
        &mut self,
        dono: EntityId,
        alvo: EntityId,
        de: Vec2,
        p: Vec2,
        dano: i32,
    ) -> bool {
        let vitima = self
            .sessions
            .values()
            .find(|s| s.entity_id == alvo)
            .filter(|s| s.conves.is_none_or(|c| !c.hostil))
            .map(|s| s.handle.id);
        match vitima {
            Some(vsid) => {
                if !self.can_damage_player(dono, alvo) || self.in_safe_zone(p) {
                    return false;
                }
                // The number over the hull: the land damage-number system
                // reads `damage_this_tick` (a mob's hit goes through
                // `pending_skill_hits`, which already does this).
                *self.damage_this_tick.entry(alvo).or_insert(0) += dano;
                self.hit_this_tick
                    .insert(alvo, (de - p).normalize_or_zero());
                self.attacker_this_tick.insert(alvo, dono);
                self.registrar_agressao(dono, alvo);
                // The Pirate's Code: more against a marked ship.
                let contra_marcado = if self.marcado_eid(alvo) {
                    self.sessions
                        .values()
                        .find(|s| s.entity_id == dono)
                        .map_or(0.0, |s| s.arvore.passivas().canhao_vs_marcado)
                } else {
                    0.0
                };
                let dano = dano as f32 * nv::PVP_CANHAO * (1.0 + contra_marcado.max(0.0));
                // Whoever was aimed at, the HULL under them takes it.
                let vsid = self.capitao_de(vsid);
                self.dano_no_casco(vsid, dano.round() as i32, Some(dono));
                true
            }
            None => {
                self.pending_skill_hits.push(PendingSkillHit {
                    target_net: alvo,
                    damage: dano,
                    attacker_net: dono,
                    hurt_dir: (de - p).normalize_or_zero(),
                    is_crit: false,
                    from_player: true,
                    knockback: 0.0,
                    drain_tier: 0,
                });
                false
            }
        }
    }

    // ───────────────────── phase 2: sea elites and planks ─────────────────────

    /// Turns the freshly spawned `eid` into its route's SEA ELITE: five times
    /// the life, a harder bite, more XP, and Wreck Planks for whoever sinks
    /// it.
    pub(crate) fn tornar_elite_do_mar(&mut self, eid: EntityId) {
        let mut alvo = None;
        for (e, (n, tag, hp, kind)) in self
            .ecs
            .query_mut::<(&NetId, &mut EnemyTag, &mut Health, &EntityKind)>()
        {
            if n.0 != eid {
                continue;
            }
            let vida = ((hp.max as f32) * ELITE_DO_MAR_VIDA).round() as i32;
            hp.max = vida;
            hp.current = vida;
            tag.stats.hp_max = vida;
            tag.stats.attack_damage = ((tag.stats.attack_damage as f32) * ELITE_DO_MAR_DANO)
                .round()
                .max(1.0) as i32;
            tag.xp_reward = (tag.xp_reward as f32 * ELITE_DO_MAR_XP).round() as u64;
            let k = if let EntityKind::Enemy(k) = kind {
                *k
            } else {
                0
            };
            tag.boss_name = Some(format!(
                "[ELITE] Dread {}",
                crate::economy::enemy_def(k).name
            ));
            alvo = Some(e);
            break;
        }
        if let Some(e) = alvo {
            let _ = self.ecs.insert_one(e, EliteDoMarTag);
        }
    }

    /// A sea mob fell to `matador`: planks and Naval Tokens. The elite always
    /// pays both; a common mob has an independent chance for each reward.
    pub(crate) fn recompensa_do_bicho(
        &mut self,
        matador: EntityId,
        elite: bool,
        sorte_prancha: f32,
        sorte_token: f32,
    ) {
        let pranchas = if elite {
            nv::PRANCHAS_DO_ELITE
        } else if sorte_prancha < nv::CHANCE_PRANCHA_DO_BICHO {
            1
        } else {
            0
        };
        let tokens = if elite {
            nv::TOKENS_DO_ELITE
        } else if sorte_token < nv::CHANCE_TOKEN_DO_BICHO {
            1
        } else {
            0
        };
        if pranchas == 0 && tokens == 0 {
            return;
        }
        let agora = (now_ms() / 1000) as i64;
        let Some(s) = self.sessions.values_mut().find(|s| s.entity_id == matador) else {
            return;
        };
        for (item, qtd) in [
            (item_id::PRANCHA_DE_NAUFRAGIO, pranchas),
            (item_id::NAVAL_TOKEN, tokens),
        ] {
            if qtd > 0 && !add_to_inventory(&mut s.inventory, item, qtd, None) {
                s.dungeon.postar(item, qtd, None, 0, agora);
            }
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
            .is_some_and(|id| {
                nv::dados(s.equipment.navio_inst.as_ref(), id)
                    .conves
                    .is_some()
            });
        nv::marcado(
            self.no_mar(),
            conves,
            self.sim_time_s,
            s.marca_ate,
            s.arvore.passivas().codigo_pirata(),
        )
    }

    /// `marcado_de` by entity.
    pub(crate) fn marcado_eid(&self, eid: EntityId) -> bool {
        self.sessions
            .values()
            .find(|s| s.entity_id == eid)
            .is_some_and(|s| self.marcado_de(s))
    }

    /// Every marked player at sea (for the nameplates and the skull flag).
    pub(crate) fn marcados(&self) -> std::collections::HashSet<EntityId> {
        if !self.no_mar() {
            return Default::default();
        }
        self.sessions
            .values()
            .filter(|s| s.logged_in && self.marcado_de(s))
            .map(|s| s.entity_id)
            .collect()
    }

    /// The `ARQUIPELAGO` index of this process's island, if it is one.
    fn ilha_daqui(&self) -> Option<usize> {
        shared::terreno::ARQUIPELAGO
            .iter()
            .position(|d| d.zona == self.zona)
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
        let Some(sid) = self
            .sessions
            .values()
            .find(|s| s.entity_id == matador && s.logged_in)
            .map(|s| s.handle.id)
        else {
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
                self.avisa_missao(
                    sid,
                    "No free ship deck: the Colossus Chest opens on the spot (×1.0).".into(),
                );
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
        let soma = |id: u16| {
            premios
                .iter()
                .filter(|p| p.item_id == id)
                .map(|p| p.qtd)
                .sum::<u32>()
        };
        let (ouro, ds, pranchas) = (
            soma(item_id::GOLD),
            soma(item_id::DARKSTEEL),
            soma(item_id::PRANCHA_DE_NAUFRAGIO),
        );
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
            self.avisa_missao(
                sid,
                "Bring the Colossus Chest ashore to open it: dock at an island.".into(),
            );
            return;
        }
        if self
            .sessions
            .get(&sid)
            .is_some_and(|s| s.instancia != 0 || s.downed)
        {
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
        let bonus = self
            .sessions
            .get(&sid)
            .map_or(0.0, |s| s.arvore.passivas().mult_tesouro);
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
        let de = self
            .sessions
            .get(&sid)
            .map(|s| s.zona_volta.clone())
            .unwrap_or_default();
        if !nv::conta_atracacao(&mut t, ilha, &de) {
            return;
        }
        d.conves = Some(t);
        self.guarda_navio(sid, d);
        let bonus = self
            .sessions
            .get(&sid)
            .map_or(0.0, |s| s.arvore.passivas().mult_tesouro);
        let mult = nv::mult_do_tesouro(&t, bonus);
        self.send_chat_to(
            sid,
            &format!("The Colossus Chest made the crossing: it now pays ×{mult:.1}."),
        );
    }

    /// THE TREASURE IS LOST by `vitima` (its deck already emptied): the
    /// trail of the mark starts; killed by a PLAYER (`atacante`), it goes
    /// straight to the killer's deck if free, else it FLOATS at `onde`.
    /// Never destroyed.
    pub(crate) fn tesouro_perdido(
        &mut self,
        vitima: SessionId,
        t: shared::items::Tesouro,
        atacante: Option<EntityId>,
        onde: Vec2,
    ) {
        let agora = self.sim_time_s;
        if let Some(s) = self.sessions.get_mut(&vitima) {
            s.marca_ate = agora + nv::TRILHA_DA_MARCA_S;
        }
        let matador = atacante.and_then(|a| {
            self.sessions
                .values()
                .find(|s| s.entity_id == a && s.logged_in)
                .map(|s| s.handle.id)
        });
        if let Some(ksid) = matador.filter(|k| *k != vitima) {
            if let Some((_, mut kd)) = self
                .navio_equipado(ksid)
                .filter(|(_, d)| d.conves.is_none())
            {
                kd.conves = Some(t);
                self.guarda_navio(ksid, kd);
                self.envia_estado_navio(ksid);
                self.avisa_missao(
                    ksid,
                    "You took their Colossus Chest: it is on your deck now.".into(),
                );
                self.avisa_missao(vitima, "Your Colossus Chest was taken.".into());
                return;
            }
        }
        self.boiar_tesouro(onde, t);
        self.avisa_missao(
            vitima,
            "Your Colossus Chest floats at the wreck: anyone can haul it aboard.".into(),
        );
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
            TesouroBoiandoTag {
                tesouro: t,
                some_em: self.sim_time_s + nv::BOIA_S,
            },
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
        let Some(eu) = self.pos_do_casco(sid) else {
            return;
        };
        let Some((_, d)) = self.navio_equipado(sid) else {
            self.avisa_missao(
                sid,
                "You have no ship. Put one in the Ship slot of your equipment.".into(),
            );
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
        s.pegando_tesouro = Some(forte::AberturaDeBau {
            bau,
            desde: agora,
            de: eu,
        });
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
            .filter_map(|(sid, s)| {
                s.pegando_tesouro.map(|a| {
                    (
                        *sid,
                        a,
                        nv::tempo_de_pegar(s.arvore.passivas().pegar_tesouro),
                    )
                })
            })
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
            let eu = self.pos_do_casco(sid);
            if eu
                .is_none_or(|eu| eu.distance(a.de) > 1.0 || eu.distance(p) > nv::ALCANCE_DO_TESOURO)
            {
                self.parar_de_pegar(sid);
                continue;
            }
            if agora - a.desde < tempo {
                continue;
            }
            self.parar_de_pegar(sid);
            let Some((_, mut d)) = self.navio_equipado(sid).filter(|(_, d)| d.conves.is_none())
            else {
                continue;
            };
            d.conves = Some(t);
            self.guarda_navio(sid, d);
            self.removed_this_tick.push(a.bau);
            let _ = self.ecs.despawn(e);
            self.envia_estado_navio(sid);
            self.avisa_missao(
                sid,
                "The Colossus Chest is on your deck. Anyone can attack you at sea now.".into(),
            );
        }
    }
}

/// The sea elite (`tornar_elite_do_mar`).
pub struct EliteDoMarTag;

/// A CANNONBALL IN THE AIR (gameplay remake): an ECS entity with only this
/// component, so no snapshot carries it. `pousar_balas` resolves it at
/// `chega_em`.
#[derive(Debug, Clone, Copy)]
pub struct BalaDeCanhao {
    pub dono: EntityId,
    pub alvo: EntityId,
    pub de: Vec2,
    pub mira: Vec2,
    pub dano: i32,
    pub chega_em: f32,
    /// Chain Shot: a hull hit slows the enemy ship (`nv::CORRENTE_*`).
    pub corrente: bool,
}

/// How long after the swing starts a mob's melee hit lands: at sea the
/// bite is telegraphed (`navio::MORDIDA_PREPARO_S`), ashore it is the land
/// animation's.
pub(crate) fn impacto_da_mordida(no_mar: bool) -> f32 {
    if no_mar {
        nv::MORDIDA_PREPARO_S
    } else {
        shared::MOB_ATTACK_IMPACT_S
    }
}

/// Seconds a common mob chases without landing a swing before it gives up
/// and swims home AT FULL HEALTH. Ashore 5 s; at sea the hull sails at
/// twice a provoked shark's speed, so a ship that kites (sails out of the
/// bite, as the telegraph asks) reset every mob it was not hitting that
/// very second — measured with the sea's AUTO COMBAT (`piloto_do_mar`): a
/// nest of three healed back to full four times and took 120 s+. Being hit
/// still restarts the count, as ashore.
pub(crate) const DESISTE_NO_MAR_S: f32 = 15.0;

/// THE SEA MOBS HUNT FROM THE SIDES (owner, 17/11/2026: "os bichos ficam
/// quase sempre atrás do barco e fica meio difícil de acertar"). A chase
/// straight at the hull leaves the mob trailing in the wake, behind the
/// stern, where no gun bears. Instead it holds this far off the ship's
/// nearer BEAM, like a shark circling its prey, and closes to bite from the
/// side when its jaws are ready.
pub(crate) const COSTADO_DO_BICHO: f32 = 3.6;
/// ...with this much lead along the ship's heading per unit of its speed
/// (seconds), so a ship under way does not leave it behind...
pub(crate) const AVANCO_DO_BICHO_S: f32 = 1.2;
/// ...capped here (units).
pub(crate) const AVANCO_DO_BICHO_MAX: f32 = 3.0;
/// How hard it steers onto its point (per second of the gap).
const GANHO_DO_BICHO: f32 = 2.5;
/// Its jaws ready within this (seconds): it closes in to bite.
pub(crate) const MORDE_EM_S: f32 = 0.35;
/// Every this many seconds it may CROSS to the ship's other side
/// (`CHANCE_DE_CRUZAR`), committed for `CRUZANDO_S`.
pub(crate) const DECIDE_LADO_S: f32 = 1.0;
pub(crate) const CHANCE_DE_CRUZAR: f32 = 0.03;
pub(crate) const CRUZANDO_S: f32 = 3.0;

/// The point a sea mob at `bicho` swims for, off the beam of a ship at
/// `navio` heading `rumo` (unit) at velocity `vel_navio`, on side `lado`
/// (+1 starboard, -1 port), close in when it `morde` (to `alcance`, its
/// bite's reach). Never THROUGH the hull: a mob in the wake first swims out
/// abeam of where it is, and one crossing to the other side goes round the
/// nearer end (measured: straight at the beam point, the wake's mobs sat
/// against the stern, out of reach and out of the arc).
#[allow(dead_code)]
pub(crate) fn ponto_no_costado(
    navio: Vec2,
    rumo: Vec2,
    vel_navio: Vec2,
    bicho: Vec2,
    lado: f32,
    morde: bool,
    alcance: f32,
) -> Vec2 {
    ponto_no_costado_do_casco(
        navio,
        rumo,
        vel_navio,
        bicho,
        lado,
        morde,
        alcance,
        nv::RAIO_DO_CASCO,
        nv::RAIO_DO_CASCO,
    )
}

/// The same for a hull `meia` across (half beam) and `meio` long (half
/// length) — the walkable deck's bigger ships (17/11/2026). The distances
/// are kept off the hull's SIDE: holding `COSTADO_DO_BICHO − RAIO_DO_CASCO`
/// out, biting from `alcance − 0.25 − RAIO_DO_CASCO` (at least 0.3), so the
/// Sloop's numbers are the old ones and a Man-o'-War's shark still surfaces
/// within a sword's reach of the rail.
#[allow(clippy::too_many_arguments)]
pub(crate) fn ponto_no_costado_do_casco(
    navio: Vec2,
    rumo: Vec2,
    vel_navio: Vec2,
    bicho: Vec2,
    lado: f32,
    morde: bool,
    alcance: f32,
    meia: f32,
    meio: f32,
) -> Vec2 {
    let r = rumo.try_normalize().unwrap_or(Vec2::Y);
    let travessa = Vec2::new(r.y, -r.x);
    let fora_do_casco = meia - nv::RAIO_DO_CASCO;
    let afasta = if morde {
        meia + (alcance - 0.25 - nv::RAIO_DO_CASCO)
            .max(0.3)
            .max(0.8 - nv::RAIO_DO_CASCO)
    } else {
        COSTADO_DO_BICHO + fora_do_casco
    };
    // Biting, no lead: the jaws close on the side, not the bow.
    let avanco = if morde {
        0.0
    } else {
        (vel_navio.length() * AVANCO_DO_BICHO_S).min(AVANCO_DO_BICHO_MAX)
    };
    let alvo = navio + travessa * lado * afasta + r * avanco;
    let livre = meia + 0.1;
    if dist_ao_segmento(navio, bicho, alvo) >= livre {
        return alvo;
    }
    let q = bicho - navio;
    let ao_longo = q.dot(r).clamp(-meio, meio);
    let fora = navio + travessa * lado * (COSTADO_DO_BICHO + fora_do_casco) + r * ao_longo;
    if q.dot(travessa) * lado > 0.0 || dist_ao_segmento(navio, bicho, fora) >= livre {
        return fora;
    }
    navio
        + r * (meio.max(livre) + 1.5) * if q.dot(r) >= 0.0 { 1.0 } else { -1.0 }
        + travessa * lado * livre
}

/// Distance from `p` to the segment `a`–`b`.
fn dist_ao_segmento(p: Vec2, a: Vec2, b: Vec2) -> f32 {
    let ab = b - a;
    let t = ((p - a).dot(ab) / ab.length_squared().max(1e-6)).clamp(0.0, 1.0);
    p.distance(a + ab * t)
}

/// Which beam `bicho` is on (+1 starboard, -1 port; `antes` when it is
/// right on the keel line).
pub(crate) fn lado_do_costado(navio: Vec2, rumo: Vec2, bicho: Vec2, antes: f32) -> f32 {
    let r = rumo.try_normalize().unwrap_or(Vec2::Y);
    let d = (bicho - navio).dot(Vec2::new(r.y, -r.x));
    if d.abs() < 0.5 {
        if antes == 0.0 {
            1.0
        } else {
            antes.signum()
        }
    } else {
        d.signum()
    }
}

/// The velocity that brings the mob onto `ponto` while it moves with the
/// ship (`vel_navio`): the ship's own velocity plus a pull onto the point,
/// no faster than `max`.
pub(crate) fn velocidade_ao_costado(bicho: Vec2, ponto: Vec2, vel_navio: Vec2, max: f32) -> Vec2 {
    (vel_navio + (ponto - bicho) * GANHO_DO_BICHO).clamp_length_max(max)
}

pub(crate) fn desistencia_do_bicho(no_mar: bool) -> f32 {
    if no_mar {
        DESISTE_NO_MAR_S
    } else {
        5.0
    }
}

// ───────────────────── gameplay remake: the water A* ─────────────────────

/// Cell of the sea's route grid, units. Coarse on purpose: the hull's own
/// `mover_casco` does the fine movement; the route only says which way.
pub const CELULA_DA_ROTA: f32 = 4.0;
/// Clearance a planned route keeps from land: the hull radius and a margin,
/// so the bow never scrapes a coast it was routed past.
pub const FOLGA_DA_ROTA: f32 = nv::RAIO_DO_CASCO + 0.8;
/// Most cells one request expands. The whole chart is ~320k cells since
/// every island is on the sea (16/11/2026); a crossing around one island
/// expands a few thousand, and the cap covers the whole grid.
const ORCAMENTO_NO_MAR: usize = 400_000;

/// THE SEA'S ROUTE GRID: which `CELULA_DA_ROTA` cells a hull can stand in
/// with `FOLGA_DA_ROTA` of water all round. Built ONCE per process from
/// `mar::navegavel` (read at runtime, so a new chart scale or island
/// position needs no change here), ~190 KB.
pub struct GradeDoMar {
    lado: i32,
    origem: f32,
    livre: Vec<bool>,
    /// The cell is in PEACEFUL WATERS (`mar::aguas_pacificas`): a route for
    /// a PK captain goes round them (17/11/2026).
    pacifica: Vec<bool>,
}

impl GradeDoMar {
    fn nova() -> Self {
        let borda = shared::mar::BORDA - FOLGA_DA_ROTA;
        let lado = ((2.0 * borda) / CELULA_DA_ROTA).floor().max(1.0) as i32;
        let origem = -borda + CELULA_DA_ROTA * 0.5;
        let mut livre = vec![false; (lado * lado) as usize];
        let mut pacifica = vec![false; (lado * lado) as usize];
        for z in 0..lado {
            for x in 0..lado {
                let p = Vec2::new(
                    origem + x as f32 * CELULA_DA_ROTA,
                    origem + z as f32 * CELULA_DA_ROTA,
                );
                let k = (z * lado + x) as usize;
                livre[k] = agua_com_folga(p);
                pacifica[k] = livre[k] && shared::mar::aguas_pacificas(p);
            }
        }
        Self {
            lado,
            origem,
            livre,
            pacifica,
        }
    }

    fn celula(&self, p: Vec2) -> (i32, i32) {
        (
            ((p.x - self.origem) / CELULA_DA_ROTA).round() as i32,
            ((p.y - self.origem) / CELULA_DA_ROTA).round() as i32,
        )
    }

    fn centro(&self, c: (i32, i32)) -> Vec2 {
        Vec2::new(
            self.origem + c.0 as f32 * CELULA_DA_ROTA,
            self.origem + c.1 as f32 * CELULA_DA_ROTA,
        )
    }

    fn indice(&self, c: (i32, i32)) -> Option<usize> {
        (c.0 >= 0 && c.1 >= 0 && c.0 < self.lado && c.1 < self.lado)
            .then(|| (c.1 * self.lado + c.0) as usize)
    }

    /// Can a hull stand in cell `c` with clearance?
    pub fn livre(&self, c: (i32, i32)) -> bool {
        self.indice(c).is_some_and(|i| self.livre[i])
    }

    /// ...and may this route pass it (`evita` = keep out of peaceful waters)?
    fn passa(&self, c: (i32, i32), evita: bool) -> bool {
        self.indice(c)
            .is_some_and(|i| self.livre[i] && !(evita && self.pacifica[i]))
    }

    /// The free cell nearest `p`, searching rings out to `raio` cells.
    fn livre_perto(&self, p: Vec2, raio: i32, evita: bool) -> Option<(i32, i32)> {
        let c = self.celula(p);
        if self.passa(c, evita) {
            return Some(c);
        }
        for r in 1..=raio {
            let mut melhor: Option<((i32, i32), f32)> = None;
            for i in -r..=r {
                for q in [
                    (c.0 + i, c.1 - r),
                    (c.0 + i, c.1 + r),
                    (c.0 - r, c.1 + i),
                    (c.0 + r, c.1 + i),
                ] {
                    if self.passa(q, evita) {
                        let d = self.centro(q).distance_squared(p);
                        if melhor.is_none_or(|(_, m)| d < m) {
                            melhor = Some((q, d));
                        }
                    }
                }
            }
            if let Some((q, _)) = melhor {
                return Some(q);
            }
        }
        None
    }

    /// Straight from `a` to `b` through free cells only (sampled every half
    /// cell).
    fn reta_livre(&self, a: Vec2, b: Vec2, evita: bool) -> bool {
        let n = ((a.distance(b) / (CELULA_DA_ROTA * 0.5)).ceil() as i32).max(1);
        (0..=n).all(|k| self.passa(self.celula(a.lerp(b, k as f32 / n as f32)), evita))
    }
}

/// Water under the point and all round it at `FOLGA_DA_ROTA`.
fn agua_com_folga(p: Vec2) -> bool {
    if !shared::mar::navegavel(p.x, p.y) {
        return false;
    }
    (0..8).all(|k| {
        let a = k as f32 * std::f32::consts::FRAC_PI_4;
        let q = p + Vec2::new(a.cos(), a.sin()) * FOLGA_DA_ROTA;
        shared::mar::navegavel(q.x, q.y)
    })
}

/// The process's grid (built on first use; `povoar_mar` warms it at boot).
pub fn grade_do_mar() -> &'static GradeDoMar {
    static G: std::sync::OnceLock<GradeDoMar> = std::sync::OnceLock::new();
    G.get_or_init(GradeDoMar::nova)
}

/// The A*'s work buffers, reused between requests (a generation stamp
/// instead of clearing ~190k entries).
struct BuscaDoMar {
    custo: Vec<i64>,
    pai: Vec<u32>,
    geracao: Vec<u32>,
    atual: u32,
}

thread_local! {
    static BUSCA: std::cell::RefCell<BuscaDoMar> =
        const { std::cell::RefCell::new(BuscaDoMar { custo: Vec::new(), pai: Vec::new(), geracao: Vec::new(), atual: 0 }) };
}

/// THE WATER ROUTE from `de` to `para`: A* over `grade_do_mar` (8
/// neighbours, octile costs), then pulled taut — the waypoints are where it
/// turns, a handful for a crossing. A destination on land (a tap on an
/// island) ends at the nearest water to it. `None` = no water route.
///
/// Runs on the SERVER, once per request (taps are throttled to 0.2 s by
/// `handle_mover_para`), never per frame.
pub fn caminho_no_mar(de: Vec2, para: Vec2) -> Option<Vec<Vec2>> {
    caminho_no_mar_com(de, para, false)
}

/// The same for a PK captain (`evita_pacificas`): round the PEACEFUL
/// WATERS, which are closed to them (17/11/2026). A start inside them (the
/// grace to leave, `aguas_pacificas.rs`) may sail out through them.
pub fn caminho_no_mar_com(de: Vec2, para: Vec2, evita_pacificas: bool) -> Option<Vec<Vec2>> {
    use std::cmp::Reverse;
    use std::collections::BinaryHeap;
    let g = grade_do_mar();
    let evita = evita_pacificas && !shared::mar::aguas_pacificas(de);
    let inicio = g.livre_perto(de, 6, false)?;
    let fim = g.livre_perto(para, 60, evita)?;
    let alvo_exato = agua_com_folga(para) && g.celula(para) == fim;
    let ultimo = if alvo_exato { para } else { g.centro(fim) };
    // Most taps at sea are in plain sight: no search.
    if g.reta_livre(g.centro(inicio), ultimo, evita) {
        return Some(vec![ultimo]);
    }
    let n = g.livre.len();
    let h = |c: (i32, i32)| -> i64 {
        let (dx, dz) = ((c.0 - fim.0).abs() as i64, (c.1 - fim.1).abs() as i64);
        let (mi, ma) = (dx.min(dz), dx.max(dz));
        mi * 1414 + (ma - mi) * 1000
    };
    let celulas = BUSCA.with(|b| {
        let mut b = b.borrow_mut();
        if b.custo.len() != n {
            b.custo = vec![0; n];
            b.pai = vec![u32::MAX; n];
            b.geracao = vec![0; n];
            b.atual = 0;
        }
        b.atual = b.atual.wrapping_add(1);
        if b.atual == 0 {
            b.geracao.iter_mut().for_each(|x| *x = 0);
            b.atual = 1;
        }
        let ger = b.atual;
        let i0 = g.indice(inicio)?;
        let i1 = g.indice(fim)?;
        b.custo[i0] = 0;
        b.pai[i0] = u32::MAX;
        b.geracao[i0] = ger;
        let mut aberto = BinaryHeap::new();
        aberto.push(Reverse((h(inicio), i0 as u32)));
        let mut expandidos = 0usize;
        let mut achou = false;
        while let Some(Reverse((_, i))) = aberto.pop() {
            let i = i as usize;
            if i == i1 {
                achou = true;
                break;
            }
            expandidos += 1;
            if expandidos > ORCAMENTO_NO_MAR {
                break;
            }
            let c = ((i as i32) % g.lado, (i as i32) / g.lado);
            let gi = b.custo[i];
            for (dx, dz) in [
                (1, 0),
                (-1, 0),
                (0, 1),
                (0, -1),
                (1, 1),
                (1, -1),
                (-1, 1),
                (-1, -1),
            ] {
                let v = (c.0 + dx, c.1 + dz);
                let Some(j) = g.indice(v) else { continue };
                if !g.livre[j] || (evita && g.pacifica[j]) {
                    continue;
                }
                // No corner cutting: a diagonal needs both sides free.
                if dx != 0
                    && dz != 0
                    && !(g.passa((c.0 + dx, c.1), evita) && g.passa((c.0, c.1 + dz), evita))
                {
                    continue;
                }
                let novo = gi + if dx != 0 && dz != 0 { 1414 } else { 1000 };
                if b.geracao[j] == ger && b.custo[j] <= novo {
                    continue;
                }
                b.geracao[j] = ger;
                b.custo[j] = novo;
                b.pai[j] = i as u32;
                aberto.push(Reverse((novo + h(v), j as u32)));
            }
        }
        if !achou {
            return None;
        }
        let mut rota = vec![i1];
        let mut k = i1;
        while b.pai[k] != u32::MAX && rota.len() <= n {
            k = b.pai[k] as usize;
            rota.push(k);
        }
        rota.reverse();
        Some(
            rota.into_iter()
                .map(|i| ((i as i32) % g.lado, (i as i32) / g.lado))
                .collect::<Vec<_>>(),
        )
    })?;
    // PULLED TAUT: from each anchor, the farthest cell still in a straight
    // line through free water.
    let pontos: Vec<Vec2> = celulas.iter().map(|c| g.centro(*c)).collect();
    let mut saida = Vec::new();
    let mut ancora = pontos[0];
    let mut k = 0usize;
    while k + 1 < pontos.len() {
        let mut j = k + 1;
        while j + 1 < pontos.len() && g.reta_livre(ancora, pontos[j + 1], evita) {
            j += 1;
        }
        saida.push(pontos[j]);
        ancora = pontos[j];
        k = j;
    }
    if let Some(u) = saida.last_mut() {
        *u = ultimo;
    } else {
        saida.push(ultimo);
    }
    Some(saida)
}

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

/// One sea mob in this many is an elite (the old ratio: one per route of
/// two nests of three).
pub const UM_ELITE_A_CADA: u32 = 6;

/// Is sea zone `zone_id` an ELITE? The mobs are scattered one per zone
/// (`mar::pontos_dos_bichos`); every `UM_ELITE_A_CADA`-th one is an elite.
pub(crate) fn e_ninho_de_elite(zone_id: u32) -> bool {
    zone_id >= ZONA_DO_MAR_ID && (zone_id - ZONA_DO_MAR_ID) % UM_ELITE_A_CADA == UM_ELITE_A_CADA - 1
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

    fn mundo(
        zona: &str,
        no_ar: &[&str],
    ) -> (GameWorld, SessionId, mpsc::UnboundedReceiver<ServerMessage>) {
        crate::economy::init_vazia_para_testes();
        let mut w = GameWorld::new(HashMap::new());
        w.zona = zona.into();
        w.diretorio = Some(crate::canais::Diretorio::para_teste(no_ar));
        let sid = SessionId(([127, 0, 0, 1], 47_470).into());
        let (tx, rx) = mpsc::unbounded_channel();
        w.on_connect(SessionHandle {
            id: sid,
            to_client: tx,
        });
        let eid = EntityId(4_700);
        let e = w.ecs.spawn((
            NetId(eid),
            Position(Vec2::new(10.0, 10.0)),
            Velocity(Vec2::ZERO),
            EntityKind::Player,
            Health {
                current: 100,
                max: 100,
            },
            PlayerTag {
                name: "Marujo".into(),
                player_id: PlayerId(47),
                attack_anim_pending: None,
                combo_step_pending: None,
            },
        ));
        let s = w.sessions.get_mut(&sid).unwrap();
        s.logged_in = true;
        s.entity = Some(e);
        s.entity_id = eid;
        s.name = "Marujo".into();
        s.inventory = vec![shared::InventorySlot::default(); shared::INVENTORY_SLOTS];
        (w, sid, rx)
    }

    fn outro_marujo(w: &mut GameWorld, porta: u16, eid: u32, nome: &str) -> SessionId {
        let sid = SessionId(([127, 0, 0, 1], porta).into());
        let (tx, _rx) = mpsc::unbounded_channel();
        w.on_connect(SessionHandle { id: sid, to_client: tx });
        let e = w.ecs.spawn((
            NetId(EntityId(eid)),
            Position(Vec2::ZERO),
            Velocity(Vec2::ZERO),
            EntityKind::Player,
            Health { current: 100, max: 100 },
            PlayerTag {
                name: nome.into(),
                player_id: PlayerId(eid as u64),
                attack_anim_pending: None,
                combo_step_pending: None,
            },
        ));
        let s = w.sessions.get_mut(&sid).unwrap();
        s.logged_in = true;
        s.entity = Some(e);
        s.entity_id = EntityId(eid);
        s.name = nome.into();
        s.inventory = vec![shared::InventorySlot::default(); shared::INVENTORY_SLOTS];
        dar_chalupa(s);
        sid
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

    /// A nest out in PvP waters (the PvP tests: the first three islands'
    /// waters are peaceful since 17/11/2026).
    fn ninho_pvp() -> Vec2 {
        *shared::mar::ninhos()
            .iter()
            .find(|n| !shared::mar::aguas_pacificas(**n) && !shared::mar::sem_lei(**n))
            .expect("a nest in PvP waters")
    }

    fn poe_em(w: &mut GameWorld, sid: SessionId, p: Vec2) {
        // Aboard, the SHIP is put there (and the deck rides along).
        w.por_navio_em(sid, p);
    }

    fn cobre(w: &mut GameWorld, sid: SessionId, n: u32) {
        let s = w.sessions.get_mut(&sid).unwrap();
        s.inventory[0] = shared::InventorySlot {
            item_id: item_id::COPPER,
            qty: n,
            instance: None,
        };
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
        s.quests
            .retain(|c| c.quest_id != shared::historia::ID_MARCO);
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
        let (mut w, sid, mut rx) = mundo(
            "ilha_inicial",
            &["ilha_inicial", shared::mar::ZONA, "ilha_gelo"],
        );
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
        assert!(
            chegada.distance(bosque.fundeadouro()) < 0.01,
            "spawns at the anchorage"
        );

        // Now the sea process, same character.
        w.zona = shared::mar::ZONA.into();
        let gelo = shared::mar::vista_da_zona("ilha_gelo").unwrap();
        // Far from the Glacier's dock: refused.
        w.handle_navio(
            sid,
            PedidoNavio::Atracar {
                ilha: gelo.ilha as u8,
            },
        );
        assert_eq!(trocou_para(&mut rx), None);
        poe_em(&mut w, sid, gelo.fundeadouro());
        // The story has not opened the Glacier: refused.
        historia_em(&mut w, sid, 0);
        w.handle_navio(
            sid,
            PedidoNavio::Atracar {
                ilha: gelo.ilha as u8,
            },
        );
        assert_eq!(trocou_para(&mut rx), None);
        let (passo, _) = shared::viagem::passo_que_libera(gelo.ilha).unwrap();
        historia_em(&mut w, sid, passo + 1);
        w.handle_navio(
            sid,
            PedidoNavio::Atracar {
                ilha: gelo.ilha as u8,
            },
        );
        assert_eq!(trocou_para(&mut rx).as_deref(), Some("ilha_gelo"));
        assert!(w.sessions[&sid].zona_volta.is_empty());
    }

    // ───────────── every island on the sea (16/11/2026) ─────────────

    /// A travel NPC of role `papel` right where the player stands.
    fn com_npc(w: &mut GameWorld, papel: shared::construcao::Papel) {
        w.ecs.spawn((
            NetId(EntityId(7_100 + papel as u32)),
            Position(Vec2::new(10.0, 12.0)),
            EntityKind::Npc(9),
            NpcDaVilaTag {
                nome: papel.nome().into(),
                rumo: shared::npc_kind(None, papel as u8),
                giver: None,
            },
        ));
    }

    /// Drains the client's inbox: (the zone it was told to switch to, the
    /// chat lines).
    fn drena(rx: &mut mpsc::UnboundedReceiver<ServerMessage>) -> (Option<String>, Vec<String>) {
        let (mut z, mut chat) = (None, Vec::new());
        while let Ok(m) = rx.try_recv() {
            match m {
                ServerMessage::TrocarZona { zona, .. } => z = Some(zona),
                ServerMessage::Chat { text, .. } => chat.push(text),
                _ => {}
            }
        }
        (z, chat)
    }

    fn toda_a_historia(w: &mut GameWorld, sid: SessionId) {
        historia_em(w, sid, shared::historia::PASSOS.len() as u32);
    }

    /// SAIL AT EVERY TRAVEL NPC (owner, 16/11/2026): the Harbour Captain,
    /// the Sky Bus Professor and the Submarine Captain all bring the "My
    /// ship" row with their menu, and Sail puts the ship at THAT island's
    /// sea dock — the land port's, or the new sea dock of an island without
    /// one — with `zona_volta` written.
    #[test]
    fn todo_npc_de_viagem_oferece_zarpar_e_zarpa_da_doca_da_ilha() {
        use shared::construcao::Papel;
        use shared::viagem::Meio;
        for (zona, papel) in [
            ("ilha_gelo", Papel::Estaleiro),
            (shared::celeste::ZONA, Papel::Estaleiro),
            (shared::celeste::ZONA, Papel::Motorista),
            (shared::kogen::ZONA, Papel::Motorista),
            (shared::kogen::ZONA, Papel::Submarino),
            (shared::abissal::ZONA, Papel::Submarino),
        ] {
            let (mut w, sid, mut rx) = mundo(zona, &[zona, shared::mar::ZONA]);
            com_npc(&mut w, papel);
            let meio = Meio::do_papel(papel as u8).unwrap();
            assert_eq!(w.quem_viaja_perto(sid), Some(meio));
            assert!(dar_chalupa(w.sessions.get_mut(&sid).unwrap()));
            w.abrir_estaleiro(sid);
            let mut vela = None;
            while let Ok(m) = rx.try_recv() {
                if let ServerMessage::AbrirEstaleiro { estado } = m {
                    vela = Some(estado.tem && estado.zarpar && !estado.avariado);
                }
            }
            assert_eq!(
                vela,
                Some(true),
                "{zona}/{papel:?}: no Sail in the Shipyard"
            );
            w.handle_navio(sid, PedidoNavio::Zarpar);
            assert_eq!(
                trocou_para(&mut rx).as_deref(),
                Some(shared::mar::ZONA),
                "{zona}/{papel:?}"
            );
            let v = shared::mar::vista_da_zona(zona).unwrap();
            assert!(
                w.pos_do_jogador(sid).unwrap().distance(v.fundeadouro()) < 0.01,
                "{zona}: not at its sea dock"
            );
            assert_eq!(w.sessions[&sid].zona_volta, zona);
        }
        // Far from any of them: refused.
        let (mut w, sid, mut rx) = mundo(
            shared::kogen::ZONA,
            &[shared::kogen::ZONA, shared::mar::ZONA],
        );
        assert!(dar_chalupa(w.sessions.get_mut(&sid).unwrap()));
        w.handle_navio(sid, PedidoNavio::Zarpar);
        assert_eq!(trocou_para(&mut rx), None);
    }

    /// EVERY NEW DOCK HANDS OFF TO THE RIGHT ZONE, at the same arrival the
    /// Captain's, the bus's and the submarine's trips use.
    #[test]
    fn toda_doca_nova_atraca_na_ilha_certa() {
        for zona in [
            shared::celeste::ZONA,
            shared::kogen::ZONA,
            shared::abissal::ZONA,
        ] {
            let (mut w, sid, mut rx) = mundo(shared::mar::ZONA, &[shared::mar::ZONA, zona]);
            toda_a_historia(&mut w, sid);
            let v = shared::mar::vista_da_zona(zona).unwrap();
            poe_em(&mut w, sid, v.fundeadouro());
            w.handle_navio(sid, PedidoNavio::Atracar { ilha: v.ilha as u8 });
            assert_eq!(trocou_para(&mut rx).as_deref(), Some(zona));
            let (_, _, chegada) = GameWorld::chegada_da_ilha(v.ilha).unwrap();
            let (dest, porto) = GameWorld::porto_da_ilha(v.ilha).unwrap();
            assert_eq!(dest.zona, zona);
            assert!(
                w.pos_do_jogador(sid).unwrap().distance(porto) < 0.01 && chegada == porto,
                "{zona}: lands off its arrival"
            );
            assert!(w.sessions[&sid].zona_volta.is_empty());
        }
    }

    /// LOCKED DESTINATIONS refuse docking WITH THE REASON (sailing there to
    /// look is free): the story for Kōgen-tō. The Magic Island has no dock
    /// at all (menu only, 17/11/2026): leaving it never puts a ship at sea.
    #[test]
    fn destino_bloqueado_recusa_atracar_com_o_motivo() {
        let kogen = shared::mar::vista_da_zona(shared::kogen::ZONA).unwrap();
        let (mut w, sid, mut rx) =
            mundo(shared::mar::ZONA, &[shared::mar::ZONA, shared::kogen::ZONA]);
        historia_em(&mut w, sid, 0);
        poe_em(&mut w, sid, kogen.fundeadouro());
        w.handle_navio(
            sid,
            PedidoNavio::Atracar {
                ilha: kogen.ilha as u8,
            },
        );
        let (z, chat) = drena(&mut rx);
        assert_eq!(z, None);
        assert!(
            chat.iter()
                .any(|t| t.starts_with("You haven't unlocked Kōgen-tō yet: reach the story step")),
            "{chat:?}"
        );

        assert!(shared::magica::NIVEIS
            .iter()
            .all(|n| shared::mar::vista_da_zona(n.zona).is_none()));
        // Saved on the Magic Island with the sea as its way back (before the
        // dock was removed): leaving lands in the Bosque, not at sea.
        let mut no_ar = vec![shared::mar::ZONA, shared::terreno::ARQUIPELAGO[0].zona];
        no_ar.extend(shared::magica::NIVEIS.iter().map(|n| n.zona));
        let (mut w, sid, mut rx) = mundo(shared::magica::ZONA, &no_ar);
        assert!(dar_chalupa(w.sessions.get_mut(&sid).unwrap()));
        w.sessions.get_mut(&sid).unwrap().magica_volta = shared::mar::ZONA.into();
        w.handle_magica(sid, shared::magica::PedidoMagica::Sair);
        assert_eq!(
            trocou_para(&mut rx).as_deref(),
            Some(shared::terreno::ARQUIPELAGO[0].zona)
        );
    }

    /// THE CAPTAIN CHARGES COPPER, and refuses without it (nothing taken).
    #[test]
    fn o_pk_volta_pra_casa_pelo_capitao() {
        // A red-named captain on the Plateau (PvP waters) takes the Captain
        // home to the Bosque, whose waters are closed to its ship.
        let (mut w, sid, mut rx) = mundo("ilha_planalto", &["ilha_planalto", "ilha_inicial"]);
        no_portal(&mut w, sid);
        w.sessions.get_mut(&sid).unwrap().pk_points = 3;
        cobre(&mut w, sid, 10_000);
        w.handle_viajar(sid, 0);
        assert_eq!(
            trocou_para(&mut rx).as_deref(),
            Some("ilha_inicial"),
            "a PK captain could not take the Captain home"
        );
    }

    #[test]
    fn o_capitao_cobra_cobre_e_recusa_sem() {
        let (mut w, sid, mut rx) = mundo("ilha_gelo", &["ilha_gelo", "ilha_inicial"]);
        no_portal(&mut w, sid);
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
        no_portal(&mut w2, sid2);
        let i = shared::historia::indice(shared::historia::PASSO_DA_CHALUPA).unwrap();
        historia_em(&mut w2, sid2, i);
        w2.sessions
            .get_mut(&sid2)
            .unwrap()
            .quests
            .push(crate::quests::CharQuest {
                quest_id: shared::historia::PASSO_DA_CHALUPA,
                status: shared::quests::quest_status::ACTIVE,
                progress: 0,
                cooldown_until: 0,
            });
        w2.handle_viajar(sid2, 1);
        assert_eq!(
            trocou_para(&mut rx2).as_deref(),
            Some("ilha_gelo"),
            "the story trip charged copper"
        );
        // A destination that is down charges nothing.
        let (mut w, sid, mut rx) = mundo("ilha_gelo", &["ilha_gelo"]);
        no_portal(&mut w, sid);
        cobre(&mut w, sid, 1_000);
        w.handle_viajar(sid, 0);
        assert_eq!(trocou_para(&mut rx), None);
        assert_eq!(tem_cobre(&w, sid), 1_000);
    }

    /// Stands the player in this town's Island Portal.
    fn no_portal(w: &mut GameWorld, sid: SessionId) {
        let (p, _) = shared::portal_ilhas::da_zona(&w.zona.clone()).expect("a portal");
        poe_em(w, sid, p);
    }

    /// THE DOCK NPCS NO LONGER CARRY ANYONE (17/11/2026): clicking one opens
    /// the Shipyard, never the travel list, and "Board" next to them (away
    /// from the portal) is refused. At the portal the same trip leaves, and
    /// lands at the destination's portal.
    #[test]
    fn os_npcs_do_cais_nao_levam_mais_ninguem() {
        use shared::construcao::Papel;
        for (zona, papel, destino) in [
            ("ilha_gelo", Papel::Estaleiro, 0usize),
            (shared::celeste::ZONA, Papel::Motorista, 5),
            (shared::kogen::ZONA, Papel::Submarino, 6),
        ] {
            let alvo = shared::terreno::ARQUIPELAGO[destino].zona;
            let (mut w, sid, mut rx) = mundo(zona, &[zona, alvo]);
            toda_a_historia(&mut w, sid);
            cobre(&mut w, sid, 10_000);
            com_npc(&mut w, papel);
            let npc = w
                .ecs
                .query::<(&NetId, &NpcDaVilaTag)>()
                .iter()
                .map(|(e, _)| e)
                .next()
                .unwrap();
            assert!(w.e_capitao(npc));
            assert_eq!(w.funcao_do_npc(npc), Some("Shipyard"), "{zona}/{papel:?}");
            w.abrir_estaleiro(sid);
            let mut viagem = false;
            let mut estaleiro = false;
            while let Ok(m) = rx.try_recv() {
                viagem |= matches!(m, ServerMessage::Viagem { .. });
                estaleiro |= matches!(m, ServerMessage::AbrirEstaleiro { .. });
            }
            assert!(
                estaleiro && !viagem,
                "{zona}/{papel:?}: the dock NPC still offers trips"
            );
            w.handle_viajar(sid, destino as u8);
            let (z, chat) = drena(&mut rx);
            assert_eq!(z, None, "{zona}/{papel:?}: boarded at the dock");
            assert!(chat.iter().any(|t| t.contains("Island Portal")), "{chat:?}");
            no_portal(&mut w, sid);
            w.handle_abrir_portal(sid);
            let mut lista = None;
            while let Ok(m) = rx.try_recv() {
                if let ServerMessage::Viagem { destinos, .. } = m {
                    lista = Some(destinos);
                }
            }
            let lista = lista.expect("the portal opens the travel menu");
            assert_eq!(lista.len(), shared::terreno::ARQUIPELAGO.len());
            w.handle_viajar(sid, destino as u8);
            assert_eq!(
                trocou_para(&mut rx).as_deref(),
                Some(alvo),
                "{zona}: the portal did not carry"
            );
            let (_, chegada) = shared::portal_ilhas::da_zona(alvo).unwrap();
            assert!(
                w.pos_do_jogador(sid).unwrap().distance(chegada) < 0.01,
                "{zona}: lands off the destination portal"
            );
        }
        // Far from the portal the menu does not open either.
        let (mut w, sid, mut rx) = mundo("ilha_inicial", &["ilha_inicial"]);
        w.handle_abrir_portal(sid);
        assert!(!std::iter::from_fn(|| rx.try_recv().ok())
            .any(|m| matches!(m, ServerMessage::Viagem { .. })));
    }

    /// THE STORY'S TRIPS GO THROUGH THE PORTAL: every "Bound for…" step can
    /// be done from the portal of the island where it is given — free, even
    /// with no copper, and the auto quest is sent to the portal.
    #[test]
    fn os_passos_de_viagem_da_historia_saem_pelo_portal() {
        use shared::quests::objective_kind;
        for (i, d) in shared::historia::PASSOS
            .iter()
            .enumerate()
            .filter(|(_, d)| d.obj_kind == objective_kind::VIAGEM)
        {
            let zona = shared::historia::zona_do_passo(d.id).expect("a story step has an island");
            let alvo = shared::terreno::ARQUIPELAGO[d.obj_target as usize].zona;
            let (mut w, sid, mut rx) = mundo(zona, &[zona, alvo]);
            historia_em(&mut w, sid, i as u32);
            w.sessions
                .get_mut(&sid)
                .unwrap()
                .quests
                .push(crate::quests::CharQuest {
                    quest_id: d.id,
                    status: shared::quests::quest_status::ACTIVE,
                    progress: 0,
                    cooldown_until: 0,
                });
            assert_eq!(
                w.ilha_da_viagem_da_historia(sid),
                Some(d.obj_target as usize),
                "{}",
                d.id
            );
            // The auto quest walks to the portal.
            w.handle_quest_destino(sid, d.id);
            let (portal, _) = shared::portal_ilhas::da_zona(zona).unwrap();
            let mut destino = None;
            while let Ok(m) = rx.try_recv() {
                if let ServerMessage::QuestDestino { tipo, pos, .. } = m {
                    destino = Some((tipo, Vec2::from(pos)));
                }
            }
            let (tipo, pos) = destino.unwrap_or_else(|| panic!("{}: no destination", d.id));
            assert_eq!(tipo, shared::quests::destino_tipo::LUGAR, "{}", d.id);
            assert!(
                pos.distance(portal) < 0.01,
                "{}: the auto quest is not sent to the portal",
                d.id
            );
            // The portal lists the step's island, free and open.
            no_portal(&mut w, sid);
            w.handle_abrir_portal(sid);
            let mut lista = Vec::new();
            while let Ok(m) = rx.try_recv() {
                if let ServerMessage::Viagem { destinos, .. } = m {
                    lista = destinos;
                }
            }
            let linha = lista
                .iter()
                .find(|x| x.ilha == d.obj_target as u8)
                .unwrap_or_else(|| panic!("{}: not listed", d.id));
            assert_eq!(
                (linha.estado, linha.preco),
                (shared::viagem::estado::LIBERADA, 0),
                "{}",
                d.id
            );
            // No copper at all, and it still leaves.
            w.handle_viajar(sid, d.obj_target as u8);
            assert_eq!(
                trocou_para(&mut rx).as_deref(),
                Some(alvo),
                "{}: the story trip is blocked",
                d.id
            );
        }
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
        assert_eq!(d.casco, nv::casco_total(NAVIO_CHALUPA, &d));
        // Ship in the bank: none.
        s.equipment.set(EquipSlot::Navio, None, None);
        s.vault = vec![shared::InventorySlot::default(); 4];
        s.vault[2] = shared::InventorySlot {
            item_id: NAVIO_CHALUPA,
            qty: 1,
            instance: None,
        };
        assert!(!dar_chalupa(s));
        // Ship in the Mail: none.
        s.vault.clear();
        s.dungeon
            .postar(NAVIO_CHALUPA, 1, None, nv::MOTIVO_CORREIO_NAVIO, 0);
        assert!(!dar_chalupa(s));
    }

    /// The story step gives the Sloop on completion, through the real tick.
    #[test]
    fn o_passo_da_historia_entrega_a_chalupa() {
        let (mut w, sid, _rx) = mundo("ilha_gelo", &[]);
        // The story only walks on a zone with relief.
        w.ilha = Some(shared::terreno::Ilha::da_ilha(
            &shared::terreno::ARQUIPELAGO[1],
        ));
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
        assert_eq!(
            s.equipment.navio,
            Some(NAVIO_CHALUPA),
            "arriving at the Glacier gives the Sloop"
        );
        assert_eq!(crate::quests::indice_da_historia(&s.quests), Some(i + 1));
    }

    /// HULL AT ZERO (naval PvP phase 2): the bites still go to the hull, but
    /// the ship remains in the sea as a drifting wreck until the captain
    /// repairs it or gives it up; only then is it damaged and the crew goes
    /// back to the dock.
    #[test]
    fn casco_a_zero_deixa_destroc_reparavel_ou_afundado() {
        let (mut w, sid, mut rx) = mundo(
            shared::mar::ZONA,
            &[shared::mar::ZONA, "ilha_inicial", "ilha_gelo"],
        );
        dar_chalupa(w.sessions.get_mut(&sid).unwrap());
        w.sessions.get_mut(&sid).unwrap().zona_volta = "ilha_gelo".into();
        historia_em(&mut w, sid, 10_000);
        // Out in open water, away from the corridors.
        let longe = shared::mar::ninhos()[0];
        poe_em(&mut w, sid, longe);
        let eid = w.sessions[&sid].entity_id;
        let cheio = nv::casco_total(NAVIO_CHALUPA, &w.navio_equipado(sid).unwrap().1);
        w.mordidas_no_casco.push((eid, 1_000));
        w.tick_mar();
        let (_, d) = w.navio_equipado(sid).unwrap();
        assert_eq!(d.casco, cheio - 1_000);
        let e = w.sessions[&sid].entity.unwrap();
        assert_eq!(
            w.ecs.get::<&Health>(e).unwrap().current,
            100,
            "the player is not hurt"
        );
        assert_eq!(trocou_para(&mut rx), None);
        w.mordidas_no_casco.push((eid, 5_000));
        w.tick_mar();
        let (_, d) = w.navio_equipado(sid).unwrap();
        assert!(
            !d.avariado && d.casco == 0,
            "the wreck is not 'damaged' yet"
        );
        assert_eq!(
            trocou_para(&mut rx),
            None,
            "a wreck does not send anyone ashore"
        );
        let navio = w.sessions[&sid].conves.unwrap().navio;
        assert!(w
            .ecs
            .get::<&NavioEnt>(navio)
            .unwrap()
            .derrotado(w.sim_time_s));
        assert_eq!(
            w.ecs.get::<&Velocity>(navio).unwrap().0,
            Vec2::ZERO,
            "the defeated hull stops dead"
        );

        // Stand by the mast and finish the 45 s sail repair.
        let mastro = shared::conves::mastros(0).next().unwrap();
        w.sessions
            .get_mut(&sid)
            .unwrap()
            .conves
            .as_mut()
            .unwrap()
            .local = mastro;
        w.handle_navio(sid, PedidoNavio::RepararVela { iniciar: true });
        assert!(w.sessions[&sid].reparando_navio.is_some());
        w.sim_time_s += nv::REPARAR_VELA_S + 0.1;
        w.tick_mar();
        let (_, d) = w.navio_equipado(sid).unwrap();
        let esperado =
            (nv::casco_total(NAVIO_CHALUPA, &d) as f32 * nv::CASCO_APOS_REPARO).round() as u16;
        assert_eq!(d.casco, esperado.max(1));
        assert!(!d.avariado);
        assert!(!w
            .ecs
            .get::<&NavioEnt>(navio)
            .unwrap()
            .derrotado(w.sim_time_s));

        // Break it again: abandoning the wreck applies the item damage and
        // returns the crew to the last dock.
        w.mordidas_no_casco.push((eid, 10_000));
        w.tick_mar();
        w.handle_navio(sid, PedidoNavio::Afundar);
        let (_, d) = w.navio_equipado(sid).unwrap();
        assert!(d.avariado && d.casco == 0);
        assert_eq!(
            trocou_para(&mut rx).as_deref(),
            Some("ilha_gelo"),
            "back to the dock sailed from"
        );

        // In a harbour corridor nothing bites.
        let (mut w, sid, _rx) = mundo(shared::mar::ZONA, &[]);
        dar_chalupa(w.sessions.get_mut(&sid).unwrap());
        poe_em(&mut w, sid, fundeadouro_padrao());
        let eid = w.sessions[&sid].entity_id;
        w.mordidas_no_casco.push((eid, 1_000));
        w.tick_mar();
        let (_, d) = w.navio_equipado(sid).unwrap();
        assert_eq!(d.casco, nv::casco_total(NAVIO_CHALUPA, &d));
    }

    fn destroco(w: &mut GameWorld, sid: SessionId, saqueado: bool) -> hecs::Entity {
        dar_chalupa(w.sessions.get_mut(&sid).unwrap());
        w.sessions.get_mut(&sid).unwrap().zona_volta = "ilha_gelo".into();
        poe_em(w, sid, ninho_pvp());
        w.tick_mar();
        let navio = w.sessions[&sid].conves.unwrap().navio;
        w.guarda_navio(
            sid,
            shared::items::NavioData {
                casco: 0,
                ..nv::novo(NAVIO_CHALUPA)
            },
        );
        let mut n = w.ecs.get::<&mut NavioEnt>(navio).unwrap();
        n.derrotado_ate = w.sim_time_s + nv::AFUNDAMENTO_S;
        n.saqueado = saqueado;
        drop(n);
        navio
    }

    #[test]
    fn so_o_destroc_nao_saqueado_deixa_um_barril() {
        let (mut w, sid, _rx) = mundo(shared::mar::ZONA, &[shared::mar::ZONA, "ilha_gelo"]);
        destroco(&mut w, sid, false);
        w.afundar_derrotado(sid, false);
        assert_eq!(
            w.ecs.query::<&BarrilDoMarTag>().iter().count(),
            1,
            "the unclaimed loot did not float"
        );
        w.afundar_derrotado(sid, false);
        assert_eq!(w.ecs.query::<&BarrilDoMarTag>().iter().count(), 1);

        let (mut w, sid, _rx) = mundo(shared::mar::ZONA, &[shared::mar::ZONA, "ilha_gelo"]);
        destroco(&mut w, sid, true);
        w.afundar_derrotado(sid, false);
        assert_eq!(w.ecs.query::<&BarrilDoMarTag>().iter().count(), 0);
    }

    #[test]
    fn aliados_proximos_dividem_o_saque_do_destroco() {
        let (mut w, vitima, _rx) = mundo(shared::mar::ZONA, &[]);
        let destroco = destroco(&mut w, vitima, false);
        let p = w.ecs.get::<&Position>(destroco).unwrap().0;
        let a = outro_marujo(&mut w, 47_471, 4_701, "Aliado A");
        let b = outro_marujo(&mut w, 47_472, 4_702, "Aliado B");
        w.sessions.get_mut(&a).unwrap().party_id = Some(9);
        w.sessions.get_mut(&b).unwrap().party_id = Some(9);
        w.tick_mar();
        w.por_navio_em(a, p + Vec2::new(5.0, 0.0));
        w.por_navio_em(b, p + Vec2::new(8.0, 0.0));

        let destroco_eid = w.ecs.get::<&NetId>(destroco).unwrap().0;
        w.conclui_saque(a, destroco_eid);

        let (ga, gb) = (w.sessions[&a].gold, w.sessions[&b].gold);
        assert!(ga > 0 && gb > 0, "both nearby allies receive a share");
        assert!((ga as i64 - gb as i64).abs() <= 1, "gold is split evenly");
        assert!((nv::SAQUE_OURO_MIN..=nv::SAQUE_OURO_MAX).contains(&(ga + gb)));
        let tokens = tem(&w, a, item_id::NAVAL_TOKEN) + tem(&w, b, item_id::NAVAL_TOKEN);
        assert!((nv::SAQUE_TOKEN_MIN..=nv::SAQUE_TOKEN_MAX).contains(&tokens));
    }

    #[test]
    fn barril_canaliza_paga_uma_vez_e_some() {
        let (mut w, sid, _rx) = mundo(shared::mar::ZONA, &[]);
        dar_chalupa(w.sessions.get_mut(&sid).unwrap());
        let p = ninho_pvp();
        poe_em(&mut w, sid, p);
        w.tick_mar();
        w.poe_barril_do_mar(p, 1_234, 2, 2);
        let (e, eid) = w
            .ecs
            .query::<(&NetId, &BarrilDoMarTag)>()
            .iter()
            .map(|(e, (n, _))| (e, n.0))
            .next()
            .unwrap();
        assert_eq!(
            w.ecs.get::<&NpcDaVilaTag>(e).unwrap().rumo,
            shared::npc_kind(None, nv::PAPEL_BARRIL_NAVAL)
        );
        w.tocar_barril_do_mar(sid, e);
        assert!(w.sessions[&sid].saqueando_barril.is_some());
        w.sim_time_s += nv::BARRIL_COLETA_S + 0.1;
        w.tick_naval();
        assert_eq!(w.sessions[&sid].gold, 1_234);
        assert_eq!(tem(&w, sid, item_id::NAVAL_TOKEN), 2);
        assert_eq!(tem(&w, sid, item_id::FRAGMENTO_NAVAL), 2);
        assert!(!w.ecs.contains(e));
        assert!(w.removed_this_tick.contains(&eid));
        w.tocar_barril_do_mar(sid, e);
        w.sim_time_s += nv::BARRIL_COLETA_S + 0.1;
        w.tick_naval();
        assert_eq!(w.sessions[&sid].gold, 1_234);
    }

    #[test]
    fn barril_interrompe_com_dano_e_expira_sem_pagar() {
        let (mut w, sid, _rx) = mundo(shared::mar::ZONA, &[]);
        dar_chalupa(w.sessions.get_mut(&sid).unwrap());
        let p = ninho_pvp();
        poe_em(&mut w, sid, p);
        w.tick_mar();
        w.poe_barril_do_mar(p, 999, 1, 1);
        let e = w
            .ecs
            .query::<&BarrilDoMarTag>()
            .iter()
            .next()
            .map(|(e, _)| e)
            .unwrap();
        w.tocar_barril_do_mar(sid, e);
        w.sessions.get_mut(&sid).unwrap().hurt_until = w.sim_time_s + 1.0;
        w.tick_naval();
        assert!(w.sessions[&sid].saqueando_barril.is_none());
        assert_eq!(w.sessions[&sid].gold, 0);

        w.tocar_barril_do_mar(sid, e);
        assert!(w.sessions[&sid].saqueando_barril.is_some());
        w.sim_time_s += nv::BARRIL_DURA_S + 0.1;
        w.tick_naval();
        assert!(!w.ecs.contains(e));
        assert!(w.sessions[&sid].saqueando_barril.is_none());
        assert_eq!(w.sessions[&sid].gold, 0);
        assert_eq!(tem(&w, sid, item_id::NAVAL_TOKEN), 0);
    }

    /// A damaged ship cannot sail; REPAIR costs copper and refuses without.
    #[test]
    fn reparo_custa_cobre_e_navio_avariado_nao_zarpa() {
        let (mut w, sid, mut rx) = mundo("ilha_inicial", &["ilha_inicial", shared::mar::ZONA]);
        com_capitao(&mut w);
        dar_chalupa(w.sessions.get_mut(&sid).unwrap());
        w.guarda_navio(
            sid,
            shared::items::NavioData {
                casco: 0,
                avariado: true,
                ..Default::default()
            },
        );
        w.handle_navio(sid, PedidoNavio::Zarpar);
        assert_eq!(trocou_para(&mut rx), None, "a damaged ship sailed");
        cobre(&mut w, sid, 100);
        w.handle_navio(sid, PedidoNavio::Reparar);
        assert!(
            w.navio_equipado(sid).unwrap().1.avariado,
            "repaired without paying"
        );
        assert_eq!(tem_cobre(&w, sid), 100);
        cobre(&mut w, sid, 300);
        w.handle_navio(sid, PedidoNavio::Reparar);
        let (_, d) = w.navio_equipado(sid).unwrap();
        assert!(!d.avariado && d.casco == nv::casco_total(NAVIO_CHALUPA, &d));
        assert_eq!(
            tem_cobre(&w, sid),
            156,
            "repair follows the tier-one hull total"
        );
        w.handle_navio(sid, PedidoNavio::Zarpar);
        assert_eq!(trocou_para(&mut rx).as_deref(), Some(shared::mar::ZONA));
    }

    /// THE SEA IS DOWN: login falls back to the island sailed from.
    #[test]
    fn com_o_mar_fora_do_ar_o_login_volta_pra_ilha() {
        let (w, _sid, _rx) = mundo("ilha_inicial", &["ilha_inicial", "ilha_gelo"]);
        assert_eq!(
            w.volta_do_mar("ilha_gelo", shared::mar::ZONA).as_deref(),
            Some("ilha_gelo")
        );
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
        w.on_connect(SessionHandle {
            id: b,
            to_client: tx,
        });
        let eb = EntityId(4_701);
        let e = w.ecs.spawn((
            NetId(eb),
            Position(Vec2::ZERO),
            Velocity(Vec2::ZERO),
            EntityKind::Player,
            Health {
                current: 100,
                max: 100,
            },
            PlayerTag {
                name: "Pirata".into(),
                player_id: PlayerId(48),
                attack_anim_pending: None,
                combo_step_pending: None,
            },
        ));
        {
            let s = w.sessions.get_mut(&b).unwrap();
            s.logged_in = true;
            s.entity = Some(e);
            s.entity_id = eb;
            s.name = "Pirata".into();
        }
        let ea = w.sessions[&a].entity_id;
        let longe = ninho_pvp();
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
            shared::pk::pode_atacar(
                &w.lado_pk(&w.sessions[&a]),
                &w.lado_pk(&w.sessions[&b]),
                false,
            )
        };
        assert_eq!(w.can_damage_player(ea, eb), esperado(&w));
        assert!(
            !w.can_damage_player(ea, eb),
            "peaceful players fight at sea"
        );
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
        let baus: Vec<Vec2> = w
            .ecs
            .query::<(&Position, &BauDoMarTag)>()
            .iter()
            .map(|(_, (p, _))| p.0)
            .collect();
        assert!(baus.len() >= 5);
        // Loot one: it pays, disappears and is queued to come back.
        let e = w
            .ecs
            .query::<&BauDoMarTag>()
            .iter()
            .next()
            .map(|(e, _)| e)
            .unwrap();
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
        // Scattered: one mob per zone, one elite in UM_ELITE_A_CADA.
        let total = w.spawn_zones.len();
        let elites = w
            .spawn_zones
            .iter()
            .filter(|z| e_ninho_de_elite(z.id))
            .count();
        assert_eq!(total, shared::mar::pontos_dos_bichos().len());
        assert!(w.spawn_zones.iter().all(|z| z.slots.len() == 1));
        assert_eq!(elites, total / UM_ELITE_A_CADA as usize, "{elites} elites of {total}");
    }

    // ───────────────────── phase 2 ─────────────────────

    fn da(w: &mut GameWorld, sid: SessionId, item: u16, qtd: u32) {
        // Straight into a slot of its own: the test economy has no stack
        // sizes, and what is counted is the total (`craft::tem`).
        let s = w.sessions.get_mut(&sid).unwrap();
        s.inventory.push(shared::InventorySlot {
            item_id: item,
            qty: qtd,
            instance: None,
        });
    }

    fn tem(w: &GameWorld, sid: SessionId, item: u16) -> u32 {
        crate::craft::tem(&w.sessions[&sid].inventory, item)
    }

    fn no_nivel(w: &mut GameWorld, sid: SessionId, nivel: u32) {
        w.sessions.get_mut(&sid).unwrap().xp =
            shared::xp_for_level_with_mult(nivel, crate::economy::xp_multiplier());
    }

    fn tem_navio_fora_do_slot(w: &GameWorld, sid: SessionId) -> bool {
        let s = &w.sessions[&sid];
        s.inventory
            .iter()
            .chain(s.vault.iter())
            .any(|x| x.qty > 0 && item_id::e_navio(x.item_id))
            || s.dungeon
                .correio
                .iter()
                .any(|c| item_id::e_navio(c.item_id))
    }

    /// THE SHIPWRIGHT BUILDS THE NEXT HULL: it refuses without the level or
    /// the materials (taking nothing), then consumes the Sloop and exactly
    /// the recipe, and the Schooner keeps the stars and the sail.
    #[test]
    fn o_estaleiro_consome_o_casco_anterior_e_os_materiais() {
        let (mut w, sid, _rx) = mundo("ilha_inicial", &[]);
        com_capitao(&mut w);
        dar_chalupa(w.sessions.get_mut(&sid).unwrap());
        let d = shared::items::NavioData {
            estrelas: [1, 2, 3],
            vela: 2,
            ..nv::novo(NAVIO_CHALUPA)
        };
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
        assert!(r
            .materiais
            .iter()
            .all(|(item, q)| tem(&w, sid, *item) == q + 3));
        no_nivel(&mut w, sid, 40);
        w.handle_navio(sid, PedidoNavio::Construir);
        let (id, d) = w.navio_equipado(sid).unwrap();
        assert_eq!(id, item_id::NAVIO_ESCUNA, "the Sloop became a Schooner");
        assert_eq!(d.estrelas, [1, 2, 3]);
        assert_eq!(d.vela, 2);
        assert_eq!(d.casco, nv::casco_total(id, &d));
        assert!(
            r.materiais.iter().all(|(item, _)| tem(&w, sid, *item) == 3),
            "the recipe was not consumed exactly"
        );
        assert!(!tem_navio_fora_do_slot(&w, sid), "the Sloop is gone");
        // Far from the Captain: refused.
        poe_em(&mut w, sid, Vec2::new(500.0, 500.0));
        for (item, q) in nv::receita_depois_de(item_id::NAVIO_ESCUNA)
            .unwrap()
            .materiais
        {
            da(&mut w, sid, item, q);
        }
        w.handle_navio(sid, PedidoNavio::Construir);
        assert_eq!(w.navio_equipado(sid).unwrap().0, item_id::NAVIO_ESCUNA);
    }

    /// New ships use stat points, so the obsolete star request cannot consume
    /// materials or silently create a stat that no longer affects them.
    #[test]
    fn estrelas_do_navio_custam_e_param_em_cinco_sem_essencia() {
        let (mut w, sid, _rx) = mundo("ilha_inicial", &[]);
        com_capitao(&mut w);
        dar_chalupa(w.sessions.get_mut(&sid).unwrap());
        da(&mut w, sid, item_id::COPPER, 100_000);
        da(&mut w, sid, item_id::DARKSTEEL, 50_000);
        da(&mut w, sid, item_id::PRANCHA_DE_NAUFRAGIO, 500);
        da(&mut w, sid, item_id::ESSENCIA_RENASCIMENTO, 77);
        w.handle_navio(
            sid,
            PedidoNavio::Melhorar {
                eixo: nv::Eixo::Canhao as u8,
            },
        );
        let (_, d) = w.navio_equipado(sid).unwrap();
        assert_eq!(d.estrelas, [0; 3]);
        assert_eq!(tem(&w, sid, item_id::COPPER), 100_000);
        assert_eq!(tem(&w, sid, item_id::DARKSTEEL), 50_000);
        assert_eq!(tem(&w, sid, item_id::PRANCHA_DE_NAUFRAGIO), 500);
        assert_eq!(
            tem(&w, sid, item_id::ESSENCIA_RENASCIMENTO),
            77,
            "the obsolete request consumed Essence"
        );
    }

    /// A world at sea with the sea's relief, the player on open water with a
    /// Sloop at level 30, and one sea mob at `alvo` from the player.
    fn no_mar_com_bicho(
        alvo: Vec2,
    ) -> (
        GameWorld,
        SessionId,
        mpsc::UnboundedReceiver<ServerMessage>,
        Vec2,
    ) {
        let (mut w, sid, rx) = mundo(shared::mar::ZONA, &[]);
        w.ilha = Some(shared::terreno::Ilha::da_ilha(&shared::mar::DEF));
        dar_chalupa(w.sessions.get_mut(&sid).unwrap());
        no_nivel(&mut w, sid, 30);
        // PvP waters: some of these fights are ship against ship.
        let aqui = ninho_pvp();
        poe_em(&mut w, sid, aqui);
        w.ecs.spawn((
            NetId(EntityId(9_900)),
            Position(aqui + alvo),
            Velocity(Vec2::ZERO),
            EntityKind::Enemy(TUBARAO),
            Health {
                current: 800,
                max: 800,
            },
        ));
        (w, sid, rx, aqui)
    }

    /// Lands every cannonball in the air (the clock is put back: the tests
    /// read cooldowns against it).
    fn pousa(w: &mut GameWorld) {
        let antes = w.sim_time_s;
        w.sim_time_s += nv::VOO_MAX_S + 0.05;
        w.pousar_balas();
        w.sim_time_s = antes;
    }

    fn golpes_em(w: &mut GameWorld, alvo: EntityId) -> Vec<i32> {
        pousa(w);
        w.pending_skill_hits
            .iter()
            .filter(|h| h.target_net == alvo)
            .map(|h| h.damage)
            .collect()
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
        let golpes = golpes_em(&mut w, bicho);
        assert_eq!(golpes.len(), nv::LEQUE_DA_BORDADA, "the five-gun fan");
        assert_eq!(
            golpes.iter().sum::<i32>(),
            esperado / 5 * 5,
            "no mana needed"
        );
        let cd = w.sessions[&sid].skill_cds[&nv::id_de_skill(0)];
        assert!((cd - w.sim_time_s - nv::HABILIDADES[0].espera_s).abs() < 1e-3);
        // On cooldown: nothing.
        w.handle_navio(sid, PedidoNavio::Habilidade { id: 0 });
        assert_eq!(golpes_em(&mut w, bicho).len(), nv::LEQUE_DA_BORDADA);
        // Sailing north with the mob dead ahead: the bow is blind.
        let (mut w, sid, _rx, _) = no_mar_com_bicho(Vec2::new(0.0, 6.0));
        let e = w.sessions[&sid].entity.unwrap();
        w.ecs.get::<&mut Velocity>(e).unwrap().0 = Vec2::new(0.0, 8.0);
        w.handle_navio(sid, PedidoNavio::Habilidade { id: 0 });
        assert!(
            golpes_em(&mut w, bicho).is_empty(),
            "a broadside hit the bow"
        );
        // More stars on the cannon, more damage.
        let (mut w, sid, _rx, _) = no_mar_com_bicho(Vec2::new(6.0, 0.0));
        let d = shared::items::NavioData {
            estrelas: [0, 0, 5],
            ..nv::novo(NAVIO_CHALUPA)
        };
        w.guarda_navio(sid, d);
        w.handle_navio(sid, PedidoNavio::Habilidade { id: 0 });
        assert!(golpes_em(&mut w, bicho).iter().sum::<i32>() > esperado);
        // Ashore the ship's bar does nothing.
        let (mut w, sid, _rx) = mundo("ilha_inicial", &[]);
        dar_chalupa(w.sessions.get_mut(&sid).unwrap());
        w.handle_navio(sid, PedidoNavio::Habilidade { id: 0 });
        assert!(w.sessions[&sid].skill_cds.is_empty());
    }

    /// LONG GUN: the selected target in range and off a side, or nothing.
    #[test]
    fn canhonaco_precisa_de_alvo_no_alcance() {
        let bicho = EntityId(9_900);
        let (mut w, sid, _rx, _) = no_mar_com_bicho(Vec2::new(0.0, 15.0));
        w.handle_navio(sid, PedidoNavio::Habilidade { id: 1 });
        assert!(golpes_em(&mut w, bicho).is_empty(), "fired with no target");
        assert!(
            w.sessions[&sid]
                .skill_cds
                .get(&nv::id_de_skill(1))
                .is_none(),
            "a refusal costs no cooldown"
        );
        w.sessions.get_mut(&sid).unwrap().target = Some(bicho);
        w.handle_navio(sid, PedidoNavio::Habilidade { id: 1 });
        let (id, d) = w.navio_equipado(sid).unwrap();
        assert_eq!(
            golpes_em(&mut w, bicho),
            vec![nv::dano_do_canhao(&nv::HABILIDADES[1], 30, id, &d)]
        );
        // Out of range.
        let (mut w, sid, _rx, _) = no_mar_com_bicho(Vec2::new(0.0, 30.0));
        w.sessions.get_mut(&sid).unwrap().target = Some(bicho);
        w.handle_navio(sid, PedidoNavio::Habilidade { id: 1 });
        assert!(golpes_em(&mut w, bicho).is_empty());
        // No bow gun (16/11/2026): the target dead ahead is refused, even
        // with the ship lying still — its helm still points at it.
        let (mut w, sid, _rx, _) = no_mar_com_bicho(Vec2::new(0.0, 15.0));
        {
            let s = w.sessions.get_mut(&sid).unwrap();
            s.target = Some(bicho);
            s.leme_do_mar = nv::Leme {
                proa: std::f32::consts::FRAC_PI_2,
                vel: 0.0,
                pronto: true,
            };
        }
        w.handle_navio(sid, PedidoNavio::Habilidade { id: 1 });
        assert!(
            golpes_em(&mut w, bicho).is_empty(),
            "the long gun fired off the bow"
        );
        assert!(w.sessions[&sid]
            .skill_cds
            .get(&nv::id_de_skill(1))
            .is_none());
        // Turned beam-on (heading east), the same target is off the side.
        w.sessions.get_mut(&sid).unwrap().leme_do_mar.proa = 0.0;
        w.handle_navio(sid, PedidoNavio::Habilidade { id: 1 });
        assert_eq!(
            golpes_em(&mut w, bicho).len(),
            1,
            "the long gun did not fire off the side"
        );
    }

    /// EMERGENCY REPAIR heals 30% of the full hull, has a long cooldown,
    /// and is refused on a sound hull.
    #[test]
    fn reparo_de_emergencia_cura_o_casco_com_recarga_longa() {
        let (mut w, sid, _rx, _) = no_mar_com_bicho(Vec2::new(30.0, 30.0));
        w.handle_navio(sid, PedidoNavio::Habilidade { id: 2 });
        assert!(
            w.sessions[&sid]
                .skill_cds
                .get(&nv::id_de_skill(2))
                .is_none(),
            "repaired a sound hull"
        );
        let (_, mut d) = w.navio_equipado(sid).unwrap();
        d.casco = 1_000;
        w.guarda_navio(sid, d);
        w.handle_navio(sid, PedidoNavio::Habilidade { id: 2 });
        assert_eq!(w.navio_equipado(sid).unwrap().1.casco, 1_000 + 432);
        w.handle_navio(sid, PedidoNavio::Habilidade { id: 2 });
        assert_eq!(
            w.navio_equipado(sid).unwrap().1.casco,
            1_432,
            "the cooldown let it heal twice"
        );
        w.sim_time_s += nv::HABILIDADES[2].espera_s + 0.1;
        w.handle_navio(sid, PedidoNavio::Habilidade { id: 2 });
        assert_eq!(
            w.navio_equipado(sid).unwrap().1.casco,
            1_440,
            "capped at the full hull"
        );
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
        s.equipment
            .set(EquipSlot::Navio, Some(item_id::NAVIO_NAU), Some(inst));
        let v = GameWorld::velocidade_no_mar(s, s.fumaca_ate - 0.1);
        assert!(v <= nv::VELOCIDADE_MAX && v < nv::TETO_DO_FIO);
    }

    #[test]
    fn corrente_reduz_a_velocidade_por_cinco_segundos() {
        let (mut w, sid, _rx, _) = no_mar_com_bicho(Vec2::new(30.0, 30.0));
        let base = GameWorld::velocidade_no_mar(&w.sessions[&sid], w.sim_time_s);
        let agora = w.sim_time_s;
        w.sessions.get_mut(&sid).unwrap().corrente_ate = agora + nv::CORRENTE_S;
        let lenta = GameWorld::velocidade_no_mar(&w.sessions[&sid], agora);
        assert!((lenta - base * nv::CORRENTE_MULT).abs() < 1e-4);
        let depois = GameWorld::velocidade_no_mar(&w.sessions[&sid], agora + nv::CORRENTE_S + 0.1);
        assert!((depois - base).abs() < 1e-4);
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
        w.on_connect(SessionHandle {
            id: b,
            to_client: tx,
        });
        let eb = EntityId(4_702);
        let e = w.ecs.spawn((
            NetId(eb),
            Position(aqui + Vec2::new(6.0, 0.0)),
            Velocity(Vec2::ZERO),
            EntityKind::Player,
            Health {
                current: 100,
                max: 100,
            },
            PlayerTag {
                name: "Pirata".into(),
                player_id: PlayerId(49),
                attack_anim_pending: None,
                combo_step_pending: None,
            },
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
        let casco_cheio = w.navio_equipado(b).unwrap().1.casco;
        w.handle_navio(a, PedidoNavio::Habilidade { id: 0 });
        pousa(&mut w);
        assert_eq!(
            w.navio_equipado(b).unwrap().1.casco,
            casco_cheio,
            "a peaceful ship was shot"
        );
        modo(&mut w, a, shared::pk::LIVRE);
        w.sim_time_s += 10.0;
        w.handle_navio(a, PedidoNavio::Habilidade { id: 0 });
        assert_eq!(
            w.navio_equipado(b).unwrap().1.casco,
            casco_cheio,
            "the ball landed before it flew"
        );
        pousa(&mut w);
        let casco = w.navio_equipado(b).unwrap().1.casco;
        assert!(casco < casco_cheio, "free PK does not reach the hull");
        assert_eq!(
            w.ecs.get::<&Health>(e).unwrap().current,
            100,
            "the cannon hit the body, not the hull"
        );
        // In the harbour corridor: nothing.
        poe_em(&mut w, a, fundeadouro_padrao());
        poe_em(&mut w, b, fundeadouro_padrao() + Vec2::new(5.0, 0.0));
        w.sim_time_s += 10.0;
        w.handle_navio(a, PedidoNavio::Habilidade { id: 0 });
        pousa(&mut w);
        assert_eq!(w.navio_equipado(b).unwrap().1.casco, casco);
    }

    /// Wreck Planks and Naval Tokens at sea: the elite always pays both; a
    /// common mob rolls each reward independently.
    #[test]
    fn recompensas_navais_caem_no_mar() {
        let (mut w, sid, _rx) = mundo(shared::mar::ZONA, &[]);
        let eu = w.sessions[&sid].entity_id;
        w.recompensa_do_bicho(eu, true, 0.99, 0.99);
        assert_eq!(
            tem(&w, sid, item_id::PRANCHA_DE_NAUFRAGIO),
            nv::PRANCHAS_DO_ELITE
        );
        assert_eq!(tem(&w, sid, item_id::NAVAL_TOKEN), nv::TOKENS_DO_ELITE);
        w.recompensa_do_bicho(eu, false, 0.99, 0.99);
        assert_eq!(
            tem(&w, sid, item_id::PRANCHA_DE_NAUFRAGIO),
            nv::PRANCHAS_DO_ELITE
        );
        assert_eq!(tem(&w, sid, item_id::NAVAL_TOKEN), nv::TOKENS_DO_ELITE);
        w.recompensa_do_bicho(eu, false, nv::CHANCE_PRANCHA_DO_BICHO * 0.5, 0.99);
        assert_eq!(
            tem(&w, sid, item_id::PRANCHA_DE_NAUFRAGIO),
            nv::PRANCHAS_DO_ELITE + 1
        );
        w.recompensa_do_bicho(eu, false, 0.99, nv::CHANCE_TOKEN_DO_BICHO * 0.5);
        assert_eq!(tem(&w, sid, item_id::NAVAL_TOKEN), nv::TOKENS_DO_ELITE + 1);
        assert!(e_ninho_de_elite(ZONA_DO_MAR_ID + 5) && !e_ninho_de_elite(ZONA_DO_MAR_ID));
    }

    /// A SHIP COSMETIC from the bag dresses the ship and is spent; with no
    /// ship it stays in the bag. At sea the look rides on the meta.
    #[test]
    fn cosmetico_veste_o_navio() {
        let (mut w, sid, _rx) = mundo("ilha_inicial", &[]);
        let vela = shared::navio::VELAS[1].item;
        w.sessions.get_mut(&sid).unwrap().inventory[3] = shared::InventorySlot {
            item_id: vela,
            qty: 2,
            instance: None,
        };
        w.handle_use_item(sid, 3);
        assert_eq!(w.sessions[&sid].inventory[3].qty, 2, "spent with no ship");
        dar_chalupa(w.sessions.get_mut(&sid).unwrap());
        w.handle_use_item(sid, 3);
        assert_eq!(w.navio_equipado(sid).unwrap().1.vela, 2);
        assert_eq!(w.sessions[&sid].inventory[3].qty, 1);
        let figura = shared::navio::FIGURAS[2].0;
        w.sessions.get_mut(&sid).unwrap().inventory[4] = shared::InventorySlot {
            item_id: figura,
            qty: 1,
            instance: None,
        };
        w.handle_use_item(sid, 4);
        let d = w.navio_equipado(sid).unwrap().1;
        assert_eq!((d.vela, d.figura), (2, 3));
        assert_eq!(w.sessions[&sid].inventory[4].qty, 0);
        w.zona = shared::mar::ZONA.into();
        // The look rides on the SHIP's entity (the walkable deck).
        w.tick_conves();
        let eid = w.sessions[&sid].conves.expect("aboard").navio_eid;
        let v = shared::navio::Visual::do_fio(w.visuais_dos_navios()[&eid]);
        assert_eq!((v.classe, v.vela, v.figura), (0, 2, 3));
    }

    /// THE SHIP'S LOOK RE-SENDS ON CHANGE (17/11/2026): building the next
    /// hull or applying a sail at sea reaches the owner AND whoever already
    /// had the ship in view — the meta used to go out only on entering view.
    #[test]
    fn o_visual_do_navio_reenvia_ao_construir_e_ao_vestir() {
        let (mut w, sid, mut rx) = mundo(shared::mar::ZONA, &[]);
        dar_chalupa(w.sessions.get_mut(&sid).unwrap());
        // A second player in view, with an inbox we read.
        let b = SessionId(([127, 0, 0, 1], 47_471).into());
        let (tx, mut rx_b) = mpsc::unbounded_channel();
        w.on_connect(SessionHandle {
            id: b,
            to_client: tx,
        });
        let eb = w.ecs.spawn((
            NetId(EntityId(4_701)),
            Position(Vec2::new(14.0, 10.0)),
            Velocity(Vec2::ZERO),
            EntityKind::Player,
            Health {
                current: 100,
                max: 100,
            },
            PlayerTag {
                name: "Vigia".into(),
                player_id: PlayerId(48),
                attack_anim_pending: None,
                combo_step_pending: None,
            },
        ));
        {
            let s = w.sessions.get_mut(&b).unwrap();
            s.logged_in = true;
            s.entity = Some(eb);
            s.entity_id = EntityId(4_701);
            s.name = "Vigia".into();
            s.inventory = vec![shared::InventorySlot::default(); shared::INVENTORY_SLOTS];
        }
        // The look rides on the SHIP's entity (the walkable deck).
        w.tick_conves();
        let eu = w.sessions[&sid].conves.expect("aboard").navio_eid;
        let visto =
            |rx: &mut mpsc::UnboundedReceiver<ServerMessage>| -> Option<shared::navio::Visual> {
                let mut v = None;
                while let Ok(m) = rx.try_recv() {
                    if let ServerMessage::Snapshot { snapshot } = m {
                        for meta in snapshot.entered.into_iter().filter(|m| m.id == eu) {
                            v = Some(shared::navio::Visual::do_fio(meta.skins));
                        }
                    }
                }
                v
            };
        w.send_snapshots();
        assert_eq!(
            visto(&mut rx).map(|v| v.classe),
            Some(0),
            "the owner sees its Sloop"
        );
        assert_eq!(
            visto(&mut rx_b).map(|v| v.classe),
            Some(0),
            "the other sees the Sloop"
        );
        w.send_snapshots();
        assert_eq!(visto(&mut rx_b), None, "nothing changed, nothing re-sent");
        // A sail applied at sea, from the bag.
        let vela = shared::navio::VELAS[1].item;
        w.sessions.get_mut(&sid).unwrap().inventory[3] = shared::InventorySlot {
            item_id: vela,
            qty: 1,
            instance: None,
        };
        w.handle_use_item(sid, 3);
        w.send_snapshots();
        assert_eq!(
            visto(&mut rx).map(|v| v.vela),
            Some(2),
            "the owner's own ship took the sail"
        );
        assert_eq!(
            visto(&mut rx_b).map(|v| v.vela),
            Some(2),
            "the other player saw the new sail"
        );
        // A new hull (the Shipwright's build, as it lands in the Ship slot).
        let (_, d) = w.navio_equipado(sid).unwrap();
        let rec = nv::receita_depois_de(NAVIO_CHALUPA).unwrap();
        {
            let s = w.sessions.get_mut(&sid).unwrap();
            s.equipment.navio = Some(rec.para);
            s.equipment.navio_inst = None;
        }
        w.guarda_navio(sid, nv::reconstruido(rec.para, &d));
        w.send_snapshots();
        let classe = nv::Visual::de(rec.para, &nv::novo(rec.para)).classe;
        assert_eq!(
            visto(&mut rx_b).map(|v| (v.classe, v.vela)),
            Some((classe, 2)),
            "the new hull, the sail kept"
        );
        assert_eq!(visto(&mut rx).map(|v| v.classe), Some(classe));
    }

    // ───────────────────── phase 4 ─────────────────────

    /// A second player at `p` with a Sloop, in `modo`.
    fn outro(w: &mut GameWorld, porta: u16, eid: u32, nome: &str, p: Vec2) -> SessionId {
        let b = SessionId(([127, 0, 0, 1], porta).into());
        let (tx, rx2) = mpsc::unbounded_channel();
        std::mem::forget(rx2);
        w.on_connect(SessionHandle {
            id: b,
            to_client: tx,
        });
        let eb = EntityId(eid);
        let e = w.ecs.spawn((
            NetId(eb),
            Position(p),
            Velocity(Vec2::ZERO),
            EntityKind::Player,
            Health {
                current: 100,
                max: 100,
            },
            PlayerTag {
                name: nome.into(),
                player_id: PlayerId(eid as u64),
                attack_anim_pending: None,
                combo_step_pending: None,
            },
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
        tem(w, sid, item)
            + s.dungeon
                .correio
                .iter()
                .filter(|c| c.item_id == item)
                .map(|c| c.qtd)
                .sum::<u32>()
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
        assert_eq!(
            (t.chefe, t.origem, t.ilhas),
            (chefe, 1, 0),
            "dropped on the Glacier"
        );
        assert_eq!(nv::mult_do_tesouro(&t, 0.0), 1.0);
        let s = w.sessions.get_mut(&sid).unwrap();
        let (id, inst) = (s.equipment.navio.unwrap(), s.equipment.navio_inst);
        // The bag.
        let mut bolsa = vec![shared::InventorySlot::default(); 4];
        assert!(
            !add_to_inventory(&mut bolsa, id, 1, inst),
            "the bag took the treasure"
        );
        // The Mail.
        let cartas = s.dungeon.correio.len();
        s.dungeon.postar(id, 1, inst, 0, 0);
        assert_eq!(
            s.dungeon.correio.len(),
            cartas,
            "the Mail took the treasure"
        );
        // A trade.
        let slot = shared::InventorySlot {
            item_id: id,
            qty: 1,
            instance: inst,
        };
        assert!(
            !shared::troca::negociavel(&slot, false),
            "a trade took the treasure"
        );
        // It cannot leave the Ship slot.
        w.handle_inventory_swap(sid, InvSpot::Equip(EquipSlot::Navio), InvSpot::Inv(5));
        assert_eq!(
            w.sessions[&sid].equipment.navio,
            Some(id),
            "the loaded ship left its slot"
        );
        assert!(conves(&w, sid).is_some());
        // The bank, at a banker, even if the ship were in the bag.
        w.ecs.spawn((
            NetId(EntityId(7_002)),
            Position(Vec2::new(10.0, 11.0)),
            EntityKind::Npc(2),
        ));
        let s = w.sessions.get_mut(&sid).unwrap();
        s.vault = vec![shared::InventorySlot::default(); 4];
        s.inventory[6] = slot;
        w.handle_vault_deposit(sid, 6);
        assert!(
            w.sessions[&sid].vault.iter().all(|v| v.qty == 0),
            "the bank took the treasure"
        );
        // A deck already loaded: the second chest opens on the spot.
        w.sessions.get_mut(&sid).unwrap().inventory[6] = shared::InventorySlot::default();
        w.dar_bau_do_colosso(eid, chefe);
        assert_eq!(conves(&w, sid), Some(t), "the deck holds one chest");
        assert!(
            tem_tudo(&w, sid, item_id::PRANCHA_DE_NAUFRAGIO) >= nv::PRANCHAS_DO_COLOSSO,
            "opened on the spot"
        );
        // An unloaded ship comes off as before (into an empty bag: the
        // prize above filled it).
        w.sessions.get_mut(&sid).unwrap().inventory =
            vec![shared::InventorySlot::default(); shared::INVENTORY_SLOTS];
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
        let (mut w, sid, _rx) = mundo(
            shared::mar::ZONA,
            &[shared::mar::ZONA, "ilha_inicial", "ilha_deserto"],
        );
        dar_chalupa(w.sessions.get_mut(&sid).unwrap());
        historia_em(&mut w, sid, 10_000);
        com_tesouro(&mut w, sid);
        w.handle_navio(sid, PedidoNavio::AbrirTesouro);
        assert!(conves(&w, sid).is_some(), "opened at sea");
        // Sailed from the Glacier, docks at the Bosque: one island.
        w.sessions.get_mut(&sid).unwrap().zona_volta = "ilha_gelo".into();
        let bosque = shared::mar::vista_da_zona("ilha_inicial").unwrap();
        poe_em(&mut w, sid, bosque.fundeadouro());
        w.handle_navio(
            sid,
            PedidoNavio::Atracar {
                ilha: bosque.ilha as u8,
            },
        );
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
        let longe = ninho_pvp();
        poe_em(&mut w, a, longe);
        let b = outro(
            &mut w,
            47_480,
            4_780,
            "Corsario",
            longe + Vec2::new(4.0, 0.0),
        );
        modo_pk(&mut w, a, shared::pk::PACIFICO);
        modo_pk(&mut w, b, shared::pk::PACIFICO);
        let (ea, eb) = (w.sessions[&a].entity_id, w.sessions[&b].entity_id);
        assert!(!w.marcado_eid(ea));
        assert!(!w.can_damage_player(eb, ea), "peaceful vs peaceful");
        com_tesouro(&mut w, a);
        assert!(w.marcado_eid(ea), "treasure on deck marks");
        assert!(
            w.can_damage_player(eb, ea),
            "a Peaceful player may attack the carrier"
        );
        assert!(
            !w.can_damage_player(ea, eb),
            "the mark opens the carrier, not the hunter"
        );
        assert!(w.marcados().contains(&ea));
        // Not on land.
        w.zona = "ilha_gelo".into();
        assert!(!w.marcado_eid(ea));
        w.zona = shared::mar::ZONA.into();
        // The corridor protects even the marked.
        poe_em(&mut w, a, fundeadouro_padrao());
        poe_em(&mut w, b, fundeadouro_padrao() + Vec2::new(3.0, 0.0));
        assert!(
            !w.can_damage_player(eb, ea),
            "the corridor let a marked ship be hit"
        );
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
        let longe = ninho_pvp();
        poe_em(&mut w, a, longe);
        com_tesouro(&mut w, a);
        let ea = w.sessions[&a].entity_id;
        // Sunk by the sea (a bite): floats.
        w.mordidas_no_casco.push((ea, 50_000));
        w.tick_mar();
        assert!(conves(&w, a).is_none(), "the deck cargo left the wreck");
        let (_, d) = w.navio_equipado(a).unwrap();
        assert!(
            !d.avariado && d.casco == 0,
            "zero hull is a boardable wreck first"
        );
        let boias: Vec<(hecs::Entity, Vec2)> = w
            .ecs
            .query::<(&Position, &TesouroBoiandoTag)>()
            .iter()
            .map(|(e, (p, _))| (e, p.0))
            .collect();
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
        w.guarda_navio(
            b,
            shared::items::NavioData {
                casco: 1,
                conves: conves(&w, b),
                ..nv::novo(NAVIO_CHALUPA)
            },
        );
        w.tiro_de_canhao(ec, eb, onde, onde, 500);
        assert!(conves(&w, b).is_none());
        assert!(
            conves(&w, c).is_some(),
            "the killer did not get the treasure"
        );
        assert_eq!(
            w.sessions[&c].pk_points, 0,
            "PK points for sinking a marked carrier"
        );
    }

    /// PK POINTS at sea: sinking an UNMARKED Peaceful player gives them as on
    /// land; in lawless waters none.
    #[test]
    fn pontos_de_pk_no_mar() {
        let (mut w, a, _rx) = mundo(shared::mar::ZONA, &[]);
        dar_chalupa(w.sessions.get_mut(&a).unwrap());
        let longe = ninho_pvp();
        assert!(
            !shared::mar::sem_lei(longe),
            "the test spot must not be lawless"
        );
        poe_em(&mut w, a, longe);
        let b = outro(&mut w, 47_483, 4_783, "Vilao", longe + Vec2::new(4.0, 0.0));
        modo_pk(&mut w, a, shared::pk::PACIFICO);
        modo_pk(&mut w, b, shared::pk::LIVRE);
        w.tick_mar();
        let (ea, eb) = (w.sessions[&a].entity_id, w.sessions[&b].entity_id);
        assert!(w.can_damage_player(eb, ea));
        w.guarda_navio(
            a,
            shared::items::NavioData {
                casco: 1,
                ..nv::novo(NAVIO_CHALUPA)
            },
        );
        w.tiro_de_canhao(eb, ea, longe, longe, 500);
        let (_, d) = w.navio_equipado(a).unwrap();
        assert!(!d.avariado && d.casco == 0, "the wreck stays behind");
        assert_eq!(
            w.sessions[&b].pk_points, 1,
            "sinking an innocent costs a PK point"
        );
        w.handle_navio(a, PedidoNavio::Afundar);
        assert!(w.navio_equipado(a).unwrap().1.avariado);
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
        assert!(
            w.sessions[&a].pk_travado && w.sessions[&b].pk_travado,
            "lawless waters lock the mode"
        );
        assert!(w.can_damage_player(eb, ea), "lawless waters are open PvP");
        w.tick_mar();
        w.guarda_navio(
            a,
            shared::items::NavioData {
                casco: 1,
                ..nv::novo(NAVIO_CHALUPA)
            },
        );
        w.tiro_de_canhao(eb, ea, z, z, 500);
        let (_, d) = w.navio_equipado(a).unwrap();
        assert!(!d.avariado && d.casco == 0, "the wreck stays behind");
        w.handle_navio(a, PedidoNavio::Afundar);
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
        let golpes = golpes_em(&mut w, bicho);
        let esperado = (cru as f32 * (1.0 + p.canhao_pct)).round() as i32;
        assert_eq!(golpes.iter().sum::<i32>(), esperado / 5 * 5);
        // Always marked at sea.
        let eu = w.sessions[&sid].entity_id;
        assert!(w.marcado_eid(eu), "the Pirate's Code");
        // Hull: a small bite takes less without exhausting this starter hull.
        let cheio = nv::casco_total(NAVIO_CHALUPA, &w.navio_equipado(sid).unwrap().1);
        w.dano_no_casco(sid, 10, None);
        let tirou = cheio - w.navio_equipado(sid).unwrap().1.casco;
        assert_eq!(tirou, (10.0 / (1.0 + p.casco_pct)).round() as u16);
        // Faster hauling.
        assert!(nv::tempo_de_pegar(p.pegar_tesouro) < nv::PEGAR_S);
    }

    // ───────────────────── gameplay remake ─────────────────────

    /// Every sample of the route from `de` is water (with the hull's three
    /// probe points of `mover_casco` along each leg).
    fn rota_so_na_agua(de: Vec2, rota: &[Vec2]) {
        let mut a = de;
        for &b in rota {
            let n = ((a.distance(b) / 0.5).ceil() as i32).max(1);
            for k in 1..=n {
                let p = a.lerp(b, k as f32 / n as f32);
                assert!(
                    shared::mar::navegavel(p.x, p.y),
                    "the route crosses land at {p:?} ({a:?} -> {b:?})"
                );
            }
            a = b;
        }
    }

    /// Sails the route with the server's own follower, helm (turning
    /// inertia of hull class `casco`) and hull mover. The ship leaves the
    /// dock pointing at `rumo0`, which may be the wrong way.
    fn navega(de: Vec2, rota: Vec<Vec2>, para: Vec2, casco: u16, rumo0: Vec2) -> (Vec2, String) {
        let mut seg =
            shared::terreno::SeguidorDeRota::nova(rota, para).com_folga(nv::FOLGA_DA_ROTA);
        let agua = |x: f32, z: f32| shared::mar::navegavel(x, z);
        let d = nv::novo(casco);
        let (v, giro, arranque) = (
            nv::velocidade_do_navio(casco, &d),
            nv::giro_max(casco, &d),
            nv::arranque_s(casco),
        );
        let mut leme = nv::Leme {
            proa: rumo0.y.atan2(rumo0.x),
            vel: 0.0,
            pronto: true,
        };
        let mut p = de;
        let (mut ticks, mut travado, mut parado, mut refeita) = (0u32, 0u32, 0u32, 0u32);
        for _ in 0..30 * 240 {
            let Some(dir) = nv::direcao_na_rota(&mut seg, &leme, p, v, giro, arranque) else {
                break;
            };
            ticks += 1;
            // Stuck: the server plans again from here (`refazer_rota` ->
            // `handle_mover_para`, at most every 0.2 s).
            if seg.travado() {
                travado += 1;
                if travado % 6 == 1 {
                    if let Some(r) = caminho_no_mar(p, para) {
                        seg = shared::terreno::SeguidorDeRota::nova(r, para)
                            .com_folga(nv::FOLGA_DA_ROTA);
                        refeita += 1;
                    }
                }
            }
            let vel = leme.governa(
                dir,
                dir.length().min(1.0) * v,
                v,
                giro,
                arranque,
                shared::TICK_DT,
            );
            let q = nv::mover_raspando(&agua, p, vel, shared::TICK_DT, nv::RAIO_DO_CASCO);
            // Grazing the coast (the body stops, the helm's order stays).
            if q.distance(p) < vel.length() * shared::TICK_DT * 0.5 {
                parado += 1;
            }
            p = q;
        }
        let diag = format!(
            "{:.0} s, {} points left (next {:?}), {travado} ticks stuck, {refeita} new routes, {parado} on the coast",
            ticks as f32 * shared::TICK_DT,
            seg.restantes(),
            seg.pontos().next()
        );
        // Coasting to a stop after the last point.
        while leme.vel > 0.0 {
            let vel = leme.governa(Vec2::ZERO, 0.0, v, giro, arranque, shared::TICK_DT);
            p = nv::mover_raspando(&agua, p, vel, shared::TICK_DT, nv::RAIO_DO_CASCO);
        }
        (p, diag)
    }

    /// THE WATER A*: from every dock to every other, the route never touches
    /// land, and the hull sailing it with the real follower, the helm's
    /// turning circle of EVERY hull class (leaving the dock pointing back
    /// at the island) and `mover_casco` arrives close enough to DOCK.
    #[test]
    fn a_rota_na_agua_nunca_passa_por_terra_e_chega_a_todo_cais() {
        let ilhas = shared::mar::ilhas();
        for a in ilhas {
            for b in ilhas {
                if a.ilha == b.ilha {
                    continue;
                }
                let (de, para) = (a.fundeadouro(), b.fundeadouro());
                let rota = caminho_no_mar(de, para)
                    .unwrap_or_else(|| panic!("no route {} -> {}", a.nome, b.nome));
                assert!(
                    rota.len() < 64,
                    "{} -> {}: {} waypoints, not pulled taut",
                    a.nome,
                    b.nome,
                    rota.len()
                );
                rota_so_na_agua(de, &rota);
                for c in nv::CASCOS {
                    let (fim, diag) = navega(de, rota.clone(), para, c.item, -a.eixo());
                    assert_eq!(
                        shared::mar::cais_perto(fim).map(|v| v.ilha),
                        Some(b.ilha),
                        "{} -> {} on a {}: the hull stopped at {fim:?}, {:.1} u from the anchorage ({diag})",
                        a.nome,
                        b.nome,
                        c.nome,
                        fim.distance(para)
                    );
                }
            }
        }
        // From the mob nests (open sea) to every dock, too.
        for n in shared::mar::ninhos() {
            for b in ilhas {
                let rota = caminho_no_mar(n, b.fundeadouro()).expect("route from a nest");
                rota_so_na_agua(n, &rota);
            }
        }
        // A tap on an island's middle ends on the water beside it.
        let i = &ilhas[0];
        let de = i.fundeadouro();
        let rota = caminho_no_mar(de, i.centro).expect("a tap on land still sails");
        rota_so_na_agua(de, &rota);
        let fim = *rota.last().unwrap();
        assert!(
            fim.distance(i.centro) < i.raio + 4.0 * CELULA_DA_ROTA + 30.0,
            "ended far from the island: {fim:?}"
        );
    }

    /// A tap at sea plans the route on the server and sends it for the
    /// dashed line, with no distance cap.
    #[test]
    fn o_toque_no_mar_vira_rota_e_tracejado() {
        let (mut w, sid, _rx) = mundo(shared::mar::ZONA, &[]);
        let ilhas = shared::mar::ilhas();
        poe_em(&mut w, sid, ilhas[0].fundeadouro());
        let longe = ilhas[ilhas.len() - 1].fundeadouro();
        w.handle_mover_para(sid, longe);
        let s = &w.sessions[&sid];
        assert!(!s.rota.vazia(), "no route at sea");
        assert!(s.rota.destino().distance(longe) < 0.01);
        assert!(s.rota.pontos().last().unwrap().distance(longe) < CELULA_DA_ROTA * 2.0);
    }

    /// CANNONBALLS FLY: nothing lands when fired, the hit lands with the
    /// ball, and a target that sailed away is a splash (no hit).
    #[test]
    fn a_bala_voa_e_acerta_o_alvo_certo() {
        let bicho = EntityId(9_900);
        let (mut w, sid, _rx, aqui) = no_mar_com_bicho(Vec2::new(8.0, 0.0));
        // A second mob nearby that is NOT the target of the Long Gun.
        w.ecs.spawn((
            NetId(EntityId(9_901)),
            Position(aqui + Vec2::new(0.0, 9.0)),
            Velocity(Vec2::ZERO),
            EntityKind::Enemy(ENGUIA),
            Health {
                current: 800,
                max: 800,
            },
        ));
        w.sessions.get_mut(&sid).unwrap().target = Some(bicho);
        w.handle_navio(sid, PedidoNavio::Habilidade { id: 1 });
        assert!(
            w.pending_skill_hits.is_empty(),
            "the hit landed before the ball flew"
        );
        let voo = nv::tempo_de_voo(8.0);
        w.sim_time_s += voo * 0.5;
        w.pousar_balas();
        assert!(w.pending_skill_hits.is_empty(), "landed at half flight");
        w.sim_time_s += voo * 0.5 + 0.01;
        w.pousar_balas();
        let alvos: Vec<EntityId> = w.pending_skill_hits.iter().map(|h| h.target_net).collect();
        assert_eq!(alvos, vec![bicho], "the ball hit the wrong body");
        assert_eq!(
            w.ecs.query::<&BalaDeCanhao>().iter().count(),
            0,
            "the ball stayed in the air"
        );
        // Dodged: the target moves far away during the flight.
        let (mut w, sid, _rx, aqui) = no_mar_com_bicho(Vec2::new(8.0, 0.0));
        w.sessions.get_mut(&sid).unwrap().target = Some(bicho);
        w.handle_navio(sid, PedidoNavio::Habilidade { id: 1 });
        for (_, (n, p)) in w.ecs.query_mut::<(&NetId, &mut Position)>() {
            if n.0 == bicho {
                p.0 = aqui + Vec2::new(8.0, 12.0);
            }
        }
        pousa(&mut w);
        assert!(w.pending_skill_hits.is_empty(), "a dodged ball still hit");
        // The lead: a target sailing across is hit where it will be.
        let (mut w, sid, _rx, _) = no_mar_com_bicho(Vec2::new(8.0, 0.0));
        for (_, (n, v)) in w.ecs.query_mut::<(&NetId, &mut Velocity)>() {
            if n.0 == bicho {
                v.0 = Vec2::new(0.0, 3.0);
            }
        }
        w.sessions.get_mut(&sid).unwrap().target = Some(bicho);
        w.handle_navio(sid, PedidoNavio::Habilidade { id: 1 });
        let mira = w
            .ecs
            .query::<&BalaDeCanhao>()
            .iter()
            .next()
            .map(|(_, b)| b.mira)
            .unwrap();
        assert!(
            mira.y > aqui.y + 1.0,
            "no lead on a moving target: {mira:?}"
        );
    }

    /// THE BITE IS TELEGRAPHED at sea: the shark winds up for
    /// `MORDIDA_PREPARO_S` before the jaws land (ashore, the land swing), and
    /// a ship that sails out of reach during the wind-up is not bitten.
    #[test]
    fn a_mordida_e_anunciada_e_da_pra_fugir() {
        assert_eq!(impacto_da_mordida(true), nv::MORDIDA_PREPARO_S);
        assert_eq!(impacto_da_mordida(false), shared::MOB_ATTACK_IMPACT_S);
        for fugir in [false, true] {
            let (mut w, sid, _rx) = mundo(shared::mar::ZONA, &[]);
            w.ilha = Some(shared::terreno::Ilha::da_ilha(&shared::mar::DEF));
            w.safe_zones.clear();
            dar_chalupa(w.sessions.get_mut(&sid).unwrap());
            let casco_cheio = w.navio_equipado(sid).unwrap().1.casco;
            let aqui = shared::mar::ninhos()[0];
            poe_em(&mut w, sid, aqui);
            w.place_enemy(aqui + Vec2::new(1.5, 0.0), TUBARAO, 0.0);
            for (_, (mob, hp)) in w.ecs.query_mut::<(&mut EnemyTag, &mut Health)>() {
                *hp = Health {
                    current: 500,
                    max: 500,
                };
                mob.dead = false;
                mob.is_melee = true;
                mob.stats.attack_damage = 50;
                mob.attack_range = 2.5;
                mob.detect_range = 15.0;
                mob.attack_cooldown_base = 30.0;
                mob.spawn_grace_until = 0.0;
            }
            let mut inicio = None;
            for _ in 0..90 {
                w.step(shared::TICK_DT);
                if inicio.is_none() {
                    if let Some(g) = w.pending_melee.iter().find(|g| !g.from_player) {
                        let falta = g.impact_at - w.sim_time_s;
                        assert!(
                            (falta - nv::MORDIDA_PREPARO_S).abs() <= shared::TICK_DT * 1.5,
                            "the bite lands {falta:.2} s after the wind-up"
                        );
                        inicio = Some(w.sim_time_s);
                        if fugir {
                            poe_em(&mut w, sid, aqui + Vec2::new(-12.0, 0.0));
                        }
                    }
                }
                if let Some(t0) = inicio {
                    if w.sim_time_s - t0 < nv::MORDIDA_PREPARO_S - shared::TICK_DT * 2.0 {
                        let casco = w.navio_equipado(sid).unwrap().1.casco;
                        assert_eq!(casco, casco_cheio, "bitten during the wind-up");
                    }
                }
            }
            assert!(inicio.is_some(), "the shark never bit");
            let casco = w.navio_equipado(sid).unwrap().1.casco;
            if fugir {
                assert_eq!(
                    casco, casco_cheio,
                    "the bite reached a ship that sailed away"
                );
            } else {
                assert!(casco < casco_cheio, "the bite never landed");
            }
        }
    }

    /// AUTO GATHER AT SEA: the nearest wreckage is offered with an approach
    /// point ON THE WATER the hull can sail to (the land's A* sees the sea as
    /// a wall and offered nothing).
    #[test]
    fn a_coleta_no_mar_acha_destroco_alcancavel() {
        let (mut w, _sid, _rx) = mundo(shared::mar::ZONA, &[]);
        w.ilha = Some(shared::terreno::Ilha::da_ilha(&shared::mar::DEF));
        let eu = shared::mar::ninhos()[0];
        // The wreckage is planted on the server at all (it used to skip
        // water columns, and the sea is all water).
        let mut achados = Vec::new();
        w.ilha
            .as_ref()
            .unwrap()
            .coletaveis_em(eu, 200.0, &mut achados);
        assert!(
            !achados.is_empty(),
            "no wreckage planted on the server's sea"
        );
        let (_, onde, centro, _) = w
            .no_de_coleta_em(eu, eu, 200.0, &[], &|_| true)
            .expect("no wreckage offered at sea");
        assert!(
            shared::mar::navegavel(onde.x, onde.y),
            "approach point on land: {onde:?}"
        );
        assert!(caminho_no_mar(eu, onde).is_some());
        assert!(onde.distance(centro) < 6.0);
    }

    /// The sea's hide: a sea mob spawns with `VIDA_DO_BICHO_DO_MAR` of its
    /// life, and the elite with five times that.
    #[test]
    fn o_bicho_do_mar_tem_couro_grosso() {
        let (mut w, _sid, _rx) = mundo(shared::mar::ZONA, &[]);
        w.place_enemy(Vec2::ZERO, TUBARAO, 0.0);
        let (eid, base) = w
            .ecs
            .query::<(&NetId, &Health)>()
            .iter()
            .find(|(_, (n, _))| n.0 != EntityId(4_700))
            .map(|(_, (n, h))| (n.0, h.max))
            .unwrap();
        w.preparar_bicho_do_mar(eid, false);
        let (max, atual) = w
            .ecs
            .query::<(&NetId, &Health)>()
            .iter()
            .find(|(_, (n, _))| n.0 == eid)
            .map(|(_, (_, h))| (h.max, h.current))
            .unwrap();
        assert_eq!(max, (base as f32 * nv::VIDA_DO_BICHO_DO_MAR).round() as i32);
        assert_eq!(atual, max);
    }
}
