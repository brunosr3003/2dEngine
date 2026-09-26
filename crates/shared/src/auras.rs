//! Aparência de equipamentos: sete bytes, sem transmitir atributos ou rolls.
use crate::Equipment;
pub const SLOTS: usize = 7;
/// Arma, mão secundária, armadura, brinco, colar, bracelete e cinto.
pub fn equipamento(e: &Equipment) -> u64 {
    let pecas = [
        (e.weapon, &e.weapon_inst), (e.offhand, &e.offhand_inst),
        (e.armor, &e.armor_inst), (e.earring, &e.earring_inst),
        (e.necklace, &e.necklace_inst), (e.bracelet, &e.bracelet_inst),
        (e.belt, &e.belt_inst),
    ];
    pecas.iter().enumerate().fold(0, |bits, (slot, (id, inst))| {
        let byte = match (id, inst) {
            (Some(_), Some(i)) if i.grau() >= 2 => i.grau() | ((i.tier() - 1) << 3) | (patamar(i.refinement) << 5),
            _ => 0,
        };
        bits | ((byte as u64) << (slot * 8))
    })
}
pub fn peca(bits: u64, slot: usize) -> Option<(u8, u8, u8)> {
    if slot >= SLOTS { return None }
    let b = (bits >> (slot * 8)) as u8;
    let grau = b & 7;
    (2..=5).contains(&grau).then_some((grau, ((b >> 3) & 3) + 1, [0,5,7,10][((b >> 5) & 3) as usize]))
}
fn patamar(refino: u8) -> u8 {
    match refino { 0..=4 => 0, 5..=6 => 1, 7..=9 => 2, _ => 3 }
}
/// O menor efeito de um tier supera o maior efeito do anterior.
pub fn intensidade(tier: u8, refino: u8) -> f32 {
    [0.65, 1.15, 1.8, 2.6][tier.clamp(1,4) as usize - 1]
        * [1.0, 1.08, 1.15, 1.22][patamar(refino) as usize]
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn pecas_independentes_e_intensidade_limitada() {
        let bits = 3 | (1 << 5) | ((4 | (3 << 3) | (3 << 5)) << 16);
        assert_eq!(peca(bits, 0), Some((3, 1, 5)));
        assert_eq!(peca(bits, 1), None);
        assert_eq!(peca(bits, 2), Some((4, 4, 10)));
        assert_eq!(intensidade(1, 4), 0.65);
        assert!(intensidade(1, 5) > intensidade(1, 4));
        assert_eq!(intensidade(1, 31), intensidade(1, 10));
        assert_eq!(equipamento(&Equipment::default()), 0);
    }
    #[test]
    fn tier_domina_o_refino_em_todos_os_patamares() {
        for tier in 1..4 { assert!(intensidade(tier+1,0) > intensidade(tier,31)); }
        for grau in 2..=5 { for tier in 1..=4 { for refino in [0,5,7,10] {
            let mut e = Equipment::default(); e.weapon = Some(400);
            let mut i = crate::items::ItemInstance::vazia_de_grau(grau);
            i.tier=tier; i.refinement=refino; e.weapon_inst=Some(i);
            assert_eq!(peca(equipamento(&e),0),Some((grau,tier,refino)));
        }}}
    }
    #[test]
    fn equipar_refinar_e_retirar_mudam_apenas_a_peca_certa() {
        let mut e = Equipment::default();
        e.weapon = Some(1);
        e.weapon_inst = Some(crate::items::ItemInstance::vazia_de_grau(3));
        e.armor = Some(2);
        e.armor_inst = Some(crate::items::ItemInstance::vazia_de_grau(4));
        let antes = equipamento(&e);
        e.weapon_inst.as_mut().unwrap().tier = 4;
        assert_eq!(peca(equipamento(&e), 0), Some((3,4,0)));
        assert_ne!(equipamento(&e), antes);
        e.weapon_inst.as_mut().unwrap().tier = 1;
        e.weapon_inst.as_mut().unwrap().refinement = 7;
        assert_eq!(peca(equipamento(&e), 0), Some((3, 1, 7)));
        assert_eq!(peca(equipamento(&e), 2), peca(antes, 2));
        e.weapon = None;
        assert_eq!(peca(equipamento(&e), 0), None);
        e.armor_inst.as_mut().unwrap().rarity = 1;
        assert_eq!(equipamento(&e), 0);
    }
}
