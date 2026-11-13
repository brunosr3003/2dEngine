//! The daily cap of FREE Experience Potions (`shared::quests::
//! POCOES_XP_GRATIS_POR_DIA`), as the server last told it
//! (`ServerMessage::PocoesXpDoDia`). Reward texts hide the potion of a daily
//! once today's cap is used up: it would not be paid.

use std::cell::Cell;

thread_local! {
    /// (game day, potions paid that day, the last daily turn-in paid it)
    static ESTADO: Cell<(i64, u32, bool)> = const { Cell::new((i64::MIN, 0, true)) };
}

pub fn definir(dia: i64, pagas: u32, ultima_paga: bool) {
    ESTADO.with(|e| e.set((dia, pagas, ultima_paga)));
}

fn agora_unix() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as i64)
}

/// Today's free potions are all paid.
pub fn esgotada() -> bool {
    let (dia, pagas, _) = ESTADO.with(|e| e.get());
    dia == shared::dungeon::dia(agora_unix()) && pagas >= shared::quests::POCOES_XP_GRATIS_POR_DIA
}

/// The last daily handed in paid its potion.
pub fn ultima_paga() -> bool {
    ESTADO.with(|e| e.get()).2
}

/// Show this reward item of a quest? Everything but a daily's XP potion
/// after the cap.
pub fn mostrar(item_id: u16, daily: bool) -> bool {
    !(daily && item_id == shared::item_id::XP_POTION && esgotada())
}

/// At a daily's turn-in (the server sends `PocoesXpDoDia` just before the
/// `QuestUpdate`): was its XP potion actually paid?
pub fn paga_na_entrega(item_id: u16, daily: bool) -> bool {
    !(daily && item_id == shared::item_id::XP_POTION && !ultima_paga())
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn some_da_recompensa_so_quando_esgota_hoje() {
        let hoje = shared::dungeon::dia(agora_unix());
        definir(hoje, 2, true);
        assert!(mostrar(shared::item_id::XP_POTION, true));
        definir(hoje, 3, false);
        assert!(!mostrar(shared::item_id::XP_POTION, true));
        assert!(mostrar(shared::item_id::XP_POTION, false), "not a daily");
        assert!(mostrar(shared::item_id::GREATER_HEAL, true));
        definir(hoje - 1, 3, false);
        assert!(mostrar(shared::item_id::XP_POTION, true), "yesterday's cap");
    }
}
