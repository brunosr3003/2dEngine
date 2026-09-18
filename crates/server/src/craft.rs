//! Craft e forja: as REGRAS, puras e testaveis sem mundo. O `world.rs` chama
//! com a bolsa da sessao e manda o resultado pro cliente.
//!
//! Receitas: `shared::receitas` (docs/ECONOMIA_DE_CRAFT.md). Refino:
//! `shared::forja` (docs/ITENS.md) — +1..+12, seguro ate' +5, do +6 em diante
//! falhar destroi a peca; custo em darksteel e cobre por tentativa.

use shared::forja::{self, resultado, Grau};
use shared::protocol::CraftRecipeNet;
use shared::{item_id, InventorySlot, ItemInstance};

/// Quanto de `id` (sem instancia — material empilhado) a bolsa tem.
pub fn tem(inv: &[InventorySlot], id: u16) -> u32 {
    inv.iter()
        .filter(|s| s.item_id == id && s.instance.is_none() && s.qty > 0)
        .map(|s| s.qty)
        .sum()
}

pub(crate) fn consumir(inv: &mut [InventorySlot], id: u16, mut qty: u32) {
    for s in inv.iter_mut() {
        if qty == 0 {
            break;
        }
        if s.item_id != id || s.instance.is_some() || s.qty == 0 {
            continue;
        }
        let tira = qty.min(s.qty);
        s.qty -= tira;
        qty -= tira;
        if s.qty == 0 {
            *s = InventorySlot::default();
        }
    }
}

/// Da' pra criar? Nivel, materiais e espaco. `Ok(slot)` = onde o item vai.
/// O `Err` e' a frase que o jogador le.
pub fn conferir(
    inv: &[InventorySlot],
    r: &CraftRecipeNet,
    nivel: u32,
    stack_max: u32,
    nome: &dyn Fn(u16) -> String,
) -> Result<usize, String> {
    if nivel < r.nivel_min as u32 {
        return Err(format!("requer nível {}", r.nivel_min));
    }
    let faltas: Vec<String> = r
        .inputs
        .iter()
        .filter(|[id, _]| *id != 0)
        .filter_map(|&[id, qtd]| {
            let t = tem(inv, id as u16);
            (t < qtd).then(|| format!("{} {}/{}", nome(id as u16), t, qtd))
        })
        .collect();
    if !faltas.is_empty() {
        return Err(format!("faltam: {}", faltas.join(", ")));
    }
    let cap = stack_max.max(1);
    if !r.roll_instance {
        if let Some(i) = inv.iter().position(|s| {
            s.item_id == r.output_item_id
                && s.instance.is_none()
                && s.qty > 0
                && s.qty + r.output_qty <= cap
        }) {
            return Ok(i);
        }
    }
    // O slot que os materiais liberam tambem serve: se nao ha' vazio agora,
    // simula o consumo antes de dizer "bolsa cheia".
    if let Some(i) = inv.iter().position(|s| s.qty == 0) {
        return Ok(i);
    }
    let mut sim = inv.to_vec();
    for &[id, qtd] in &r.inputs {
        if id != 0 {
            consumir(&mut sim, id as u16, qtd);
        }
    }
    sim.iter()
        .position(|s| s.qty == 0)
        .ok_or_else(|| "bolsa cheia".to_string())
}

/// Consome os materiais e poe o item. Chame so' depois de `conferir`. Devolve
/// o slot onde o item ficou.
pub fn aplicar(
    inv: &mut [InventorySlot],
    r: &CraftRecipeNet,
    inst: Option<ItemInstance>,
) -> Option<usize> {
    for &[id, qtd] in &r.inputs {
        if id != 0 {
            consumir(inv, id as u16, qtd);
        }
    }
    if !r.roll_instance {
        if let Some(i) = inv
            .iter()
            .position(|s| s.item_id == r.output_item_id && s.instance.is_none() && s.qty > 0)
        {
            inv[i].qty += r.output_qty;
            return Some(i);
        }
    }
    let i = inv.iter().position(|s| s.qty == 0)?;
    inv[i] = InventorySlot {
        item_id: r.output_item_id,
        qty: if r.roll_instance { 1 } else { r.output_qty },
        instance: if r.roll_instance { inst } else { None },
    };
    Some(i)
}

/// O grau (cor) de uma peca, pelo campo `rarity` da instancia.
pub fn grau_de(inst: &ItemInstance) -> Grau {
    Grau::de_u8(inst.rarity.clamp(1, 5)).unwrap_or(Grau::Comum)
}

/// (darksteel, cobre) da proxima tentativa nesta peca.
pub fn custo_do_refino(inst: &ItemInstance) -> (u32, u32) {
    forja::custo_em_jogo(grau_de(inst))
}

/// Uma tentativa de refino: cobra da bolsa e sorteia. `sorte` 0..=99.
/// Devolve (`forja::resultado`, nivel da peca agora). Em `DESTRUIU` quem chama
/// tira a peca de onde ela estava. Sem material ou no topo, nada e' cobrado.
pub fn refinar(inst: &mut ItemInstance, inv: &mut [InventorySlot], sorte: u8) -> (u8, u8) {
    let nivel = inst.refinement;
    if nivel >= forja::REFINO_MAX {
        return (resultado::NO_TOPO, nivel);
    }
    let (ds, cu) = custo_do_refino(inst);
    if tem(inv, item_id::DARKSTEEL) < ds || tem(inv, item_id::COPPER) < cu {
        return (resultado::SEM_MATERIAL, nivel);
    }
    consumir(inv, item_id::DARKSTEEL, ds);
    consumir(inv, item_id::COPPER, cu);
    match forja::refinar(nivel, sorte) {
        forja::Refino::Subiu(n) => {
            inst.refinement = n;
            (resultado::SUBIU, n)
        }
        forja::Refino::Falhou(n) => (resultado::FALHOU, n),
        forja::Refino::Destruiu => (resultado::DESTRUIU, 0),
        forja::Refino::NoTopo => (resultado::NO_TOPO, nivel),
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    fn bolsa(itens: &[(u16, u32)]) -> Vec<InventorySlot> {
        let mut v = vec![InventorySlot::default(); 24];
        for (i, &(id, q)) in itens.iter().enumerate() {
            v[i] = InventorySlot {
                item_id: id,
                qty: q,
                instance: None,
            };
        }
        v
    }

    fn nome(id: u16) -> String {
        format!("#{id}")
    }

    fn receita_cinza() -> CraftRecipeNet {
        shared::receitas::receitas_de_equipamento()
            .into_iter()
            .next()
            .unwrap()
    }

    fn com_tudo(r: &CraftRecipeNet) -> Vec<InventorySlot> {
        bolsa(
            &r.inputs
                .iter()
                .map(|&[id, q]| (id as u16, q))
                .collect::<Vec<_>>(),
        )
    }

    #[test]
    fn com_material_consome_tudo_e_cria_a_peca() {
        let r = receita_cinza();
        let mut inv = com_tudo(&r);
        let slot = conferir(&inv, &r, 1, 1, &nome).expect("devia poder criar");
        let inst = ItemInstance::roll_for(r.output_item_id, r.output_item_level, || 0.5);
        let onde = aplicar(&mut inv, &r, inst).unwrap();
        assert!(slot < inv.len());
        assert_eq!(inv[onde].item_id, r.output_item_id);
        assert!(inv[onde].instance.is_some(), "peca de craft tem instancia");
        for &[id, _] in &r.inputs {
            assert_eq!(tem(&inv, id as u16), 0, "sobrou material {id}");
        }
    }

    #[test]
    fn sem_material_nem_nivel_nao_cria_e_nao_mexe_na_bolsa() {
        let r = receita_cinza();
        let mut inv = com_tudo(&r);
        inv[1].qty -= 1; // falta 1 do principal
        let contagem =
            |v: &[InventorySlot]| v.iter().map(|s| (s.item_id, s.qty)).collect::<Vec<_>>();
        let antes = contagem(&inv);
        let e = conferir(&inv, &r, 1, 1, &nome).unwrap_err();
        assert!(e.starts_with("faltam"), "{e}");
        assert_eq!(contagem(&inv), antes);
        let epico = shared::receitas::receitas_de_equipamento()
            .into_iter()
            .find(|x| x.nivel_min >= 60)
            .unwrap();
        let e = conferir(&com_tudo(&epico), &epico, 20, 1, &nome).unwrap_err();
        assert_eq!(e, "requer nível 60");
    }

    #[test]
    fn refino_segue_a_chance_e_destroi_do_seis_em_diante() {
        let mut est: u32 = 0xC0FF_EE11;
        let mut sorte = || {
            est = est.wrapping_mul(1664525).wrapping_add(1013904223);
            ((est >> 16) % 100) as u8
        };
        let rico = || {
            bolsa(&[
                (item_id::DARKSTEEL, 1_000_000),
                (item_id::COPPER, 1_000_000),
            ])
        };
        let base = ItemInstance::roll_for(item_id::KATANA, 5, || 0.5).unwrap();
        let n = 20_000;
        let (mut subiu4, mut destruiu6, mut falhou4) = (0, 0, 0);
        for _ in 0..n {
            let mut p = base;
            p.refinement = 3; // alvo +4: 80%, falha segura
            let mut inv = rico();
            match refinar(&mut p, &mut inv, sorte()).0 {
                resultado::SUBIU => subiu4 += 1,
                resultado::FALHOU => falhou4 += 1,
                outro => panic!("+4 nao destroi: {outro}"),
            }
            let mut q = base;
            q.refinement = 5; // alvo +6: 30%, falha destroi
            let mut inv = rico();
            if refinar(&mut q, &mut inv, sorte()).0 == resultado::DESTRUIU {
                destruiu6 += 1;
            }
        }
        let f = |x: i32| x as f32 / n as f32;
        assert!((f(subiu4) - 0.80).abs() < 0.02, "+4 subiu {}", f(subiu4));
        assert_eq!(subiu4 + falhou4, n);
        assert!(
            (f(destruiu6) - 0.70).abs() < 0.02,
            "+6 destruiu {}",
            f(destruiu6)
        );
    }

    #[test]
    fn refino_cobra_o_custo_e_sem_material_nao_tenta() {
        let mut p = ItemInstance::roll_for(item_id::KATANA, 5, || 0.5).unwrap();
        let (ds, cu) = custo_do_refino(&p);
        assert!(ds > 0 && cu > 0);
        let mut inv = bolsa(&[(item_id::DARKSTEEL, ds), (item_id::COPPER, cu)]);
        assert_eq!(refinar(&mut p, &mut inv, 0), (resultado::SUBIU, 1));
        assert_eq!(tem(&inv, item_id::DARKSTEEL), 0);
        assert_eq!(refinar(&mut p, &mut inv, 0), (resultado::SEM_MATERIAL, 1));
        p.refinement = forja::REFINO_MAX;
        assert_eq!(refinar(&mut p, &mut inv, 0).0, resultado::NO_TOPO);
    }
}
