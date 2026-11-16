//! THE SHIP (docs/NAVEGACAO.md, naval phase 1): pure numbers about hulls,
//! shared by client and server so the Captain's panel shows exactly what the
//! server will charge.
//!
//! The ship is an ITEM in its own equip slot (`EquipSlot::Navio`): one active
//! ship, no "which ship?" dialog at the dock. The item id is the hull class;
//! the hull points live in the instance (`ItemInstance::navio`), so trading
//! or selling a ship carries its state.
//!
//! At sea the HULL takes the damage, not the player (sea mobs gnaw it). At
//! zero the ship is damaged (`avariado`): the player is put back at the dock
//! they sailed from, and the ship cannot sail until the Harbour Captain
//! repairs it, for copper.
//!
//! Phase 2 hook: the Shipwright (Estaleiro) crafts and upgrades the next
//! hulls. `CASCOS` is the table it will extend, and `proximo_casco` is the
//! question its panel will ask; today both answer "only the Sloop".

use serde::{Deserialize, Serialize};

use crate::constants::item_id;
use crate::items::{ItemInstance, NavioData};

/// One hull class.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Casco {
    pub item: u16,
    /// English name (the item's name in the database).
    pub nome: &'static str,
    /// Hull points when new.
    pub casco: u16,
    /// Speed at sea, units per second.
    pub velocidade: f32,
}

/// The hulls. Phase 1 has only the Sloop.
///
/// The hull is at SHIP scale, not person scale: 2,400 points. A Reef Shark
/// of the Bosque's stretch bites ~15 a hit, so a ship standing still in a
/// pack to gather lasts minutes, and one that keeps moving is never caught
/// (the sharks swim at ~2 u/s, the Sloop sails at 10).
pub const CASCOS: [Casco; 1] = [Casco {
    item: item_id::NAVIO_CHALUPA,
    nome: "Sloop",
    casco: 2400,
    velocidade: 10.0,
}];

/// The hull's radius for the water test. Larger than a body on foot: a ship
/// grazing the coast has to stop before it looks inside the rock.
pub const RAIO_DO_CASCO: f32 = 1.4;

/// Copper per hull point repaired. A full Sloop repair is 240 copper.
pub const COBRE_POR_PONTO: f32 = 0.1;

/// Mail reason of the ship given to old characters (`dungeon::postar`).
pub const MOTIVO_CORREIO_NAVIO: u8 = 8;

/// Below this fraction of the hull the player is warned once.
pub const CRITICO: f32 = 0.3;

/// The hull class of an item, if it is a ship.
pub fn casco(id: u16) -> Option<&'static Casco> {
    CASCOS.iter().find(|c| c.item == id)
}

/// Hull points of a new ship of this class.
pub fn casco_max(id: u16) -> u16 {
    casco(id).map_or(0, |c| c.casco)
}

/// Speed at sea of this class.
pub fn velocidade(id: u16) -> f32 {
    casco(id).map_or(CASCOS[0].velocidade, |c| c.velocidade)
}

/// A brand-new hull.
pub fn novo(id: u16) -> NavioData {
    NavioData { casco: casco_max(id), avariado: false }
}

/// What the instance says about the ship. `None` (a ship without state yet)
/// reads as a brand-new hull.
pub fn dados(inst: Option<&ItemInstance>, id: u16) -> NavioData {
    inst.and_then(|i| i.navio).unwrap_or_else(|| novo(id))
}

/// The instance a new ship is born with: no stats, grey, bound to nothing.
pub fn instancia_nova(id: u16) -> ItemInstance {
    let mut i = ItemInstance::vazia_de_grau(1);
    i.navio = Some(novo(id));
    i
}

/// Copper to repair the hull back to full. Zero when it is full.
pub fn custo_do_reparo(id: u16, d: &NavioData) -> u32 {
    let falta = casco_max(id).saturating_sub(d.casco) as f32;
    (falta * COBRE_POR_PONTO).ceil() as u32
}

/// Can this ship sail now?
pub fn pode_zarpar(d: &NavioData) -> bool {
    !d.avariado && d.casco > 0
}

/// The Harbour Captain's trip, in COPPER, to island `ilha` of the
/// `ARQUIPELAGO` (owner's decision, 15/11/2026: the quick trip costs copper
/// so the sea has a reason). Scales with the destination: 50 to the Bosque,
/// 100 the Glacier, 150 the Waste, 200 the Plateau, 250 Skyreach. For
/// reference the Teleport Scroll is 100. The story's own trip (the step
/// that sends you to a new island) stays free.
pub fn preco_da_viagem(ilha: usize) -> u32 {
    50 * (ilha as u32 + 1)
}

/// PHASE 2 HOOK: the hull the Shipwright builds after `id`. None today.
pub fn proximo_casco(_id: u16) -> Option<u16> {
    None
}

/// What the player asks of the ship (`ClientMessage::Navio`).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum PedidoNavio {
    /// At the Harbour Captain: sail out to the open sea with my ship.
    Zarpar,
    /// At sea, at a dock: go ashore on island `ilha` (`ARQUIPELAGO` index).
    Atracar { ilha: u8 },
    /// At the Harbour Captain: repair the hull to full, for copper.
    Reparar,
    /// Send me the ship's state (`ServerMessage::Navio`).
    Estado,
}

/// The ship's state, for the Captain's buttons and the hull bar.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
pub struct EstadoNavio {
    /// There is a ship in the ship slot.
    pub tem: bool,
    pub item_id: u16,
    pub casco: u16,
    pub casco_max: u16,
    pub avariado: bool,
    /// Copper for a full repair (0 = nothing to repair).
    pub reparo: u32,
    /// This island has a dock on the open sea (sailing is offered).
    pub zarpar: bool,
}

impl EstadoNavio {
    /// The state of the ship in `id`/`inst` (`None` = no ship).
    pub fn de(id: Option<u16>, inst: Option<&ItemInstance>, zarpar: bool) -> Self {
        let Some(id) = id.filter(|i| item_id::e_navio(*i)) else {
            return Self { zarpar, ..Default::default() };
        };
        let d = dados(inst, id);
        Self {
            tem: true,
            item_id: id,
            casco: d.casco,
            casco_max: casco_max(id),
            avariado: d.avariado,
            reparo: custo_do_reparo(id, &d),
            zarpar,
        }
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    /// The fastest hull stays under what the wire carries
    /// (`EntityState::vel` is i8 at `POS_SCALE`, ~15.9 u/s).
    #[test]
    fn nenhum_casco_passa_do_teto_do_fio() {
        let teto = i8::MAX as f32 / crate::components::POS_SCALE;
        for c in CASCOS {
            assert!(c.velocidade < teto, "{} at {} u/s passes the wire's {teto}", c.nome, c.velocidade);
            assert!(c.velocidade > crate::PLAYER_SPEED, "a ship slower than walking");
        }
    }

    #[test]
    fn reparo_cobra_o_que_falta_e_nada_cheio() {
        let id = item_id::NAVIO_CHALUPA;
        assert_eq!(custo_do_reparo(id, &novo(id)), 0);
        let afundado = NavioData { casco: 0, avariado: true };
        assert_eq!(custo_do_reparo(id, &afundado), 240);
        assert!(!pode_zarpar(&afundado));
        assert!(pode_zarpar(&novo(id)));
        assert_eq!(dados(None, id), novo(id), "no state reads as a new hull");
    }

    #[test]
    fn a_viagem_cresce_com_o_destino() {
        assert_eq!(preco_da_viagem(0), 50);
        assert!(preco_da_viagem(3) > preco_da_viagem(1));
        assert!(preco_da_viagem(4) <= 300, "modest: the scroll is 100");
    }
}
