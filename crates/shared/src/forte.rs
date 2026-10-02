//! FIELD BOSS CHESTS, born with STORMKEEP — the open-field castle in the Plateau's south
//! (`planalto::Forte`). Its Warlord drops one to three chests where he
//! falls, each green, blue or purple; opening one takes `ABRIR_S` seconds of
//! standing still, like gathering, and whoever finishes it takes the loot.
//!
//! The loot is a level 56 dungeon chest — the Tempest Spire's table
//! (`CONTEUDO_DO_BAU`) — and the color is the stage it rolls: green 1,
//! blue 2, purple 3.

/// The Warlord of Stormkeep (`bosses::CHEFES`).
pub const SENHOR_DO_FORTE: u16 = 24;
/// The dungeon whose chest table the castle's chests roll (the Tempest Spire, 56).
pub const CONTEUDO_DO_BAU: u16 = 18;
/// The dungeon whose chest table a field boss's chests roll: Stormkeep's
/// Warlord rolls the Tempest Spire's (56); every other field boss the
/// highest dungeon at or below his level (the owner: "in the magic island
/// and others need to be this kind of chest too").
pub fn conteudo_do_chefe(kind: u16) -> u16 {
    if kind == SENHOR_DO_FORTE {
        return CONTEUDO_DO_BAU;
    }
    let nivel = crate::bosses::chefe(kind).map_or(1, |c| c.nivel);
    let disponiveis = || crate::dungeon::CONTEUDOS.iter().filter(|c| c.disponivel);
    disponiveis()
        .filter(|c| c.nivel_min <= nivel)
        .max_by_key(|c| c.nivel_min)
        .or_else(|| disponiveis().min_by_key(|c| c.nivel_min))
        .map_or(CONTEUDO_DO_BAU, |c| c.id)
}

/// Seconds of standing at the chest to open it.
pub const ABRIR_S: f32 = 10.0;
/// How close the opener has to stay (units), and how far they may shuffle.
pub const ALCANCE: f32 = 3.0;
/// An unopened chest vanishes after this, in seconds.
pub const DURA_S: f32 = 300.0;

/// The chest NPC's role (`npc_kind`), one per color — next to the dungeon's
/// own chest (`dungeon::PAPEL_BAU`, 120).
pub const PAPEL_VERDE: u8 = 121;
pub const PAPEL_AZUL: u8 = 122;
pub const PAPEL_ROXO: u8 = 123;

/// `ColetaEstado.tipo` while opening a chest: `TIPO_COLETA + cor` (2..4).
pub const TIPO_COLETA: u8 = 200;

/// The role of a chest of `cor` (2 green, 3 blue, 4 purple).
pub fn papel(cor: u8) -> u8 {
    match cor {
        3 => PAPEL_AZUL,
        4 => PAPEL_ROXO,
        _ => PAPEL_VERDE,
    }
}

/// The color of a chest role, if it is one of the castle's.
pub fn cor_do_papel(papel: u8) -> Option<u8> {
    match papel {
        PAPEL_VERDE => Some(2),
        PAPEL_AZUL => Some(3),
        PAPEL_ROXO => Some(4),
        _ => None,
    }
}

pub fn nome_da_cor(cor: u8) -> &'static str {
    match cor {
        3 => "Blue chest",
        4 => "Purple chest",
        _ => "Green chest",
    }
}

/// How many chests the Warlord drops: 1 (40%), 2 (40%), 3 (20%).
pub fn quantos(r: f32) -> u8 {
    if r < 0.4 {
        1
    } else if r < 0.8 {
        2
    } else {
        3
    }
}

/// Each chest's color, rolled on its own: green 55%, blue 33%, purple 12%.
pub fn cor(r: f32) -> u8 {
    if r < 0.55 {
        2
    } else if r < 0.88 {
        3
    } else {
        4
    }
}

/// The stage of `CONTEUDO_DO_BAU` a chest of `cor` rolls.
pub fn estagio(cor: u8) -> u8 {
    cor.clamp(2, 4) - 1
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn um_a_tres_baus_de_cores_e_estagios_crescentes() {
        let n: Vec<u8> = (0..100).map(|k| quantos(k as f32 / 100.0)).collect();
        assert!(n.iter().all(|q| (1..=3).contains(q)));
        assert!(n.contains(&1) && n.contains(&2) && n.contains(&3));
        let c: Vec<u8> = (0..100).map(|k| cor(k as f32 / 100.0)).collect();
        assert_eq!(c.iter().filter(|x| **x == 2).count(), 55);
        assert_eq!(c.iter().filter(|x| **x == 4).count(), 12);
        assert_eq!((estagio(2), estagio(3), estagio(4)), (1, 2, 3));
        for k in 2..=4 {
            assert_eq!(cor_do_papel(papel(k)), Some(k));
        }
        assert_eq!(cor_do_papel(crate::dungeon::PAPEL_BAU), None);
        let c = crate::dungeon::conteudo(CONTEUDO_DO_BAU).expect("the Tempest Spire");
        assert_eq!(c.nivel_min, 56, "the castle's chests roll a level 56 table");
        assert!(crate::bosses::chefe(SENHOR_DO_FORTE).is_some_and(|b| b.nivel == 60 && b.zona == crate::planalto::ZONA));
        // Every field boss rolls a dungeon at or below his level (or the
        // lowest, below them all).
        for b in crate::bosses::CHEFES.iter() {
            let c = crate::dungeon::conteudo(conteudo_do_chefe(b.kind)).expect("a dungeon");
            assert!(c.disponivel, "{} rolls a locked dungeon", b.nome);
            assert!(c.nivel_min <= b.nivel.max(6), "{} ({}) rolls {} ({})", b.nome, b.nivel, c.nome, c.nivel_min);
        }
    }
}
