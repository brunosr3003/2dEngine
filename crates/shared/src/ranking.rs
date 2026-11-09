//! RANKINGS: top characters by POWER and by LEVEL, overall or per class
//! (owner, 08/11/2026: "a rank list of power and of level, per class,
//! overall").
//!
//! The game is classless; the class players see is the WEAPON SET
//! (`skills::Conjunto`: Sword and Shield, Katana, Twin Pistols, Magic Ring),
//! taken from the weapon equipped. Power is the character sheet's number
//! (`dungeon::poder_de_stats`), level the one from XP. Staff accounts don't
//! rank.

use serde::{Deserialize, Serialize};

/// How many rows a ranking page carries.
pub const TOPO: usize = 50;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum TipoRanking {
    Poder,
    Nivel,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LinhaRanking {
    /// 1-based place.
    pub posicao: u32,
    pub nome: String,
    pub nivel: u32,
    pub poder: i32,
    /// `Conjunto as u8`.
    pub classe: u8,
    /// The character's look (packed `Aparencia` and skins) and the glow of
    /// its gear: tapping a row shows it in 3D, like the bag's portrait.
    #[serde(default)]
    pub aparencia: u32,
    #[serde(default)]
    pub skins: u64,
    #[serde(default)]
    pub auras: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Ranking {
    pub tipo: TipoRanking,
    /// `None` = all classes.
    pub classe: Option<u8>,
    pub linhas: Vec<LinhaRanking>,
    /// The asking character's own row, when it ranks (in or out of the top).
    pub eu: Option<LinhaRanking>,
    /// Characters in this list in total.
    pub total: u32,
}

/// Orders and numbers rows for one list. Ties: higher level/power second,
/// then name, so the order is stable between refreshes.
pub fn ordena(linhas: &mut [LinhaRanking], tipo: TipoRanking) {
    linhas.sort_by(|a, b| {
        let (pa, pb) = match tipo {
            TipoRanking::Poder => ((a.poder as i64, a.nivel as i64), (b.poder as i64, b.nivel as i64)),
            TipoRanking::Nivel => ((a.nivel as i64, a.poder as i64), (b.nivel as i64, b.poder as i64)),
        };
        pb.cmp(&pa).then_with(|| a.nome.cmp(&b.nome))
    });
    for (i, l) in linhas.iter_mut().enumerate() {
        l.posicao = i as u32 + 1;
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    fn l(nome: &str, nivel: u32, poder: i32) -> LinhaRanking {
        LinhaRanking { posicao: 0, nome: nome.into(), nivel, poder, classe: 0, aparencia: 0, skins: 0, auras: 0 }
    }

    #[test]
    fn ordena_por_poder_e_por_nivel_com_empate_estavel() {
        let mut v = vec![l("b", 10, 500), l("a", 12, 500), l("c", 30, 400)];
        ordena(&mut v, TipoRanking::Poder);
        let nomes: Vec<&str> = v.iter().map(|x| x.nome.as_str()).collect();
        assert_eq!(nomes, ["a", "b", "c"]);
        assert_eq!(v[0].posicao, 1);
        ordena(&mut v, TipoRanking::Nivel);
        assert_eq!(v[0].nome, "c");
        assert_eq!(v[2].posicao, 3);
    }
}
