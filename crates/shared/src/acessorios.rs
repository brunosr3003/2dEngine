//! Companion accessories: utility bonuses, with no combat attributes.
use crate::{item_id, Equipment};

pub const PET_INICIO: u16 = 520;
pub const MONTARIA_INICIO: u16 = 528;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Efeito { Drop, LootMelhor, Xp, Energia, Minerio, Darksteel, Vida, Mana }

impl Efeito {
    pub const TODOS: [Self; 8] = [Self::Drop, Self::LootMelhor, Self::Xp,
        Self::Energia, Self::Minerio, Self::Darksteel, Self::Vida, Self::Mana];
    pub fn nome(self) -> &'static str {
        match self {
            Self::Drop => "Drop chance +10%",
            Self::LootMelhor => "Better loot chance +5%",
            Self::Xp => "XP gained +10%",
            Self::Energia => "Energy gathering +15%",
            Self::Minerio => "Ore gathering +15%",
            Self::Darksteel => "Darksteel gathering +15%",
            Self::Vida => "Base health regen +1/s",
            Self::Mana => "Base mana regen +0.5/s",
        }
    }
}

pub fn tipo(id: u16) -> Option<(bool, Efeito)> {
    let (pet, n) = if (PET_INICIO..PET_INICIO + 8).contains(&id) {
        (true, id - PET_INICIO)
    } else if (MONTARIA_INICIO..MONTARIA_INICIO + 8).contains(&id) {
        (false, id - MONTARIA_INICIO)
    } else { return None };
    Some((pet, Efeito::TODOS[n as usize]))
}

pub fn nome(id: u16) -> Option<String> {
    let (pet, efeito) = tipo(id)?;
    Some(format!("{}: {}", if pet { "Pet" } else { "Mount" }, efeito.nome()))
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Bonus {
    pub drop: f32,
    pub loot_melhor: f32,
    pub xp: f32,
    pub energia: f32,
    pub minerio: f32,
    pub darksteel: f32,
    pub vida: f32,
    pub mana: f32,
}

impl Bonus {
    fn somar(&mut self, e: Efeito) {
        match e {
            Efeito::Drop => self.drop += 0.10,
            Efeito::LootMelhor => self.loot_melhor += 0.05,
            Efeito::Xp => self.xp += 0.10,
            Efeito::Energia => self.energia += 0.15,
            Efeito::Minerio => self.minerio += 0.15,
            Efeito::Darksteel => self.darksteel += 0.15,
            Efeito::Vida => self.vida += 1.0,
            Efeito::Mana => self.mana += 0.5,
        }
    }
}

pub fn bonus(equip: &Equipment) -> Bonus {
    let mut b = Bonus::default();
    if equip.pets().iter().any(|(_, id, _)| id.is_some_and(|id| item_id::pet_de_id(id).is_some())) {
        if let Some((true, e)) = equip.acessorio_pet.and_then(tipo) { b.somar(e); }
    }
    if equip.montaria.is_some_and(|id| item_id::montaria_de_id(id).is_some()) {
        if let Some((false, e)) = equip.acessorio_montaria.and_then(tipo) { b.somar(e); }
    }
    b
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn so_bonus_com_companheiro_e_slot_correto() {
        let mut e = Equipment::default();
        e.acessorio_pet = Some(PET_INICIO);
        assert_eq!(bonus(&e), Bonus::default());
        e.pet = Some(item_id::PET_BASE);
        assert_eq!(bonus(&e).drop, 0.10);
        e.acessorio_montaria = Some(MONTARIA_INICIO + 7);
        assert_eq!(bonus(&e).mana, 0.0);
        e.montaria = Some(item_id::MONTARIA_BASE);
        assert_eq!(bonus(&e).mana, 0.5);
    }
}
