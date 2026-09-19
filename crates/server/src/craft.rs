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

// ─────────────────────────── peca sem instancia ───────────────────────────

/// A instancia de uma peca que chegou SEM ela (a arma inicial, item antigo):
/// Comum, Tier I, no nivel de item do craft cinza — a mesma de quem cria a
/// peca no Craft. Sem instancia a peca nao tinha cor nem tier, e a Forja e o
/// Aprimorar nao enxergavam ela. `None` pro que nao e' equipamento.
pub fn instancia_inicial(item: u16) -> Option<ItemInstance> {
    shared::equip_slot_of(item)?;
    ItemInstance::roll_em(
        crate::economy::item_template_of(item),
        forja::nivel_de_item_da_cor(1),
        1,
        1,
        fastrand::f32,
    )
}

/// Da' instancia a toda peca de equipamento sem ela, na bolsa (ou cofre).
/// Devolve se mudou algo (pra salvar).
pub fn garantir_instancias(
    inv: &mut [InventorySlot],
    rolar: &mut dyn FnMut(u16) -> Option<ItemInstance>,
) -> bool {
    let mut mudou = false;
    for s in inv.iter_mut() {
        if s.qty == 1 && s.instance.is_none() && shared::equip_slot_of(s.item_id).is_some() {
            if let Some(i) = rolar(s.item_id) {
                s.instance = Some(i);
                mudou = true;
            }
        }
    }
    mudou
}

/// O mesmo pro que esta' vestido.
pub fn garantir_instancias_vestidas(
    equip: &mut shared::Equipment,
    rolar: &mut dyn FnMut(u16) -> Option<ItemInstance>,
) -> bool {
    let mut mudou = false;
    for slot in shared::EquipSlot::TODOS {
        if let (Some(id), None) = (equip.get(slot), equip.get_inst(slot)) {
            if let Some(i) = rolar(id) {
                equip.set(slot, Some(id), Some(i));
                mudou = true;
            }
        }
    }
    mudou
}

/// Atributos fixos (`ItemInstance::fixar`) em toda peca da lista: as roladas
/// no sistema antigo (valor sorteado + afixos) voltam pro valor da tabela.
/// Devolve se mudou algo (pra salvar).
pub fn fixar_pecas(inv: &mut [InventorySlot]) -> bool {
    let mut mudou = false;
    for s in inv.iter_mut().filter(|s| s.qty > 0) {
        if let Some(i) = s.instance.as_mut() {
            mudou |= i.fixar(crate::economy::item_template_of(s.item_id));
        }
    }
    mudou
}

/// Idem, nas pecas vestidas.
pub fn fixar_vestidas(equip: &mut shared::Equipment) -> bool {
    let mut mudou = false;
    for slot in shared::EquipSlot::TODOS {
        if let (Some(id), Some(mut i)) = (equip.get(slot), equip.get_inst(slot)) {
            if i.fixar(crate::economy::item_template_of(id)) {
                equip.set(slot, Some(id), Some(i));
                mudou = true;
            }
        }
    }
    mudou
}

// ─────────────────────────── aprimorar e combinar ───────────────────────────

/// Aba Aprimorar: funde as pecas dos slots `a` e `b` da bolsa no degrau de
/// cima (`forja::conferir_aprimorar`), cobrando cobre. A nova fica no slot
/// `a`; o `b` esvazia. Subir de COR pede o nivel da cor
/// (`forja::nivel_da_cor`). `rolar(item, nivel_de_item, cor, tier)` rola a
/// instancia nova (o template vem do cache do servidor). Devolve
/// (item_id, cor, tier).
pub fn aprimorar(
    inv: &mut [InventorySlot],
    a: usize,
    b: usize,
    nivel: u32,
    rolar: &mut dyn FnMut(u16, u16, u8, u8) -> Option<ItemInstance>,
) -> Result<(u16, u8, u8), String> {
    if a == b || a >= inv.len() || b >= inv.len() {
        return Err("escolha duas peças diferentes da bolsa".into());
    }
    let (sa, sb) = (inv[a], inv[b]);
    let (Some(ia), Some(ib)) = (sa.instance, sb.instance) else {
        return Err("só peça de equipamento se aprimora".into());
    };
    if sa.qty == 0 || sb.qty == 0 {
        return Err("escolha duas peças diferentes da bolsa".into());
    }
    let peca = |s: &InventorySlot, i: &ItemInstance| {
        (
            s.item_id,
            i.grau(),
            i.tier(),
            i.refinement,
            i.socketed_gems.iter().any(|g| *g != 0),
        )
    };
    let (grau, tier) = forja::conferir_aprimorar(peca(&sa, &ia), peca(&sb, &ib))?;
    if grau > ia.grau() && nivel < forja::nivel_da_cor(grau) {
        return Err(format!("requer nível {} para essa cor", forja::nivel_da_cor(grau)));
    }
    let cobre = forja::custo_de_aprimorar(ia.grau(), ia.tier());
    if tem(inv, item_id::COPPER) < cobre {
        return Err(format!("faltam {} de cobre", cobre - tem(inv, item_id::COPPER)));
    }
    let nivel_item = ia
        .item_level
        .max(ib.item_level)
        .max(forja::nivel_de_item_da_cor(grau));
    let Some(mut nova) = rolar(sa.item_id, nivel_item, grau, tier) else {
        return Err("este item não se aprimora".into());
    };
    // Os atributos vêm diretamente da tabela fixa para item/cor/tier/nível.
    // As peças consumidas não influenciam os números da nova.
    // Peca de bau vinculada contamina a fusao: senao era so' fundir uma
    // vinculada com uma qualquer pra poder vender.
    nova.vinculado = ia.vinculado || ib.vinculado;
    consumir(inv, item_id::COPPER, cobre);
    inv[b] = InventorySlot::default();
    inv[a] = InventorySlot {
        item_id: sa.item_id,
        qty: 1,
        instance: Some(nova),
    };
    Ok((sa.item_id, grau, tier))
}

/// Poe `qty` de um empilhavel na bolsa: completa as pilhas, depois os vazios.
/// Tudo ou nada — sem espaco pra tudo, a bolsa nao muda.
pub(crate) fn por_empilhavel(inv: &mut [InventorySlot], id: u16, qty: u32, cap: u32) -> bool {
    if shared::armazem::e_moeda(id) {
        return crate::world::por_na_carteira(inv, id, qty);
    }
    let cap = cap.max(1);
    let mut sim = inv.to_vec();
    let mut falta = qty;
    for s in sim.iter_mut() {
        if falta == 0 {
            break;
        }
        if s.item_id == id && s.instance.is_none() && s.qty > 0 && s.qty < cap {
            let p = falta.min(cap - s.qty);
            s.qty += p;
            falta -= p;
        }
    }
    for s in sim.iter_mut() {
        if falta == 0 {
            break;
        }
        if s.qty == 0 {
            let p = falta.min(cap);
            *s = InventorySlot {
                item_id: id,
                qty: p,
                instance: None,
            };
            falta -= p;
        }
    }
    if falta > 0 {
        return false;
    }
    inv.copy_from_slice(&sim);
    true
}

/// Aba Combinar: ate' `vezes` tentativas de `r`, limitadas ao que a bolsa
/// paga. Cobra TODAS as tentativas e entrega os sucessos. Recusa (sem mexer
/// na bolsa) se nao paga nenhuma ou se o pior caso — todas darem certo — nao
/// cabe. `sorte()` devolve 0..=99. Devolve (tentativas, sucessos).
pub fn combinar(
    inv: &mut [InventorySlot],
    r: &shared::combinar::ReceitaDeCombinar,
    vezes: u16,
    cap: u32,
    nome: &dyn Fn(u16) -> String,
    sorte: &mut dyn FnMut() -> u8,
) -> Result<(u16, u16), String> {
    let pode = shared::combinar::vezes_possiveis(r, &|id| tem(inv, id));
    let n = (vezes.max(1) as u32).min(pode);
    if n == 0 {
        let mut faltas = Vec::new();
        for (id, custo) in [
            (r.entrada, r.qtd),
            (item_id::COPPER, r.cobre),
            (item_id::DARKSTEEL, r.darksteel),
            (item_id::GLITTERING_POWDER, r.po),
        ] {
            if custo > 0 && tem(inv, id) < custo {
                faltas.push(format!("{} {}/{}", nome(id), tem(inv, id), custo));
            }
        }
        return Err(format!("faltam: {}", faltas.join(", ")));
    }
    let cobra = |b: &mut [InventorySlot]| {
        consumir(b, r.entrada, r.qtd * n);
        consumir(b, item_id::COPPER, r.cobre * n);
        consumir(b, item_id::DARKSTEEL, r.darksteel * n);
        consumir(b, item_id::GLITTERING_POWDER, r.po * n);
    };
    let mut sim = inv.to_vec();
    cobra(&mut sim);
    if !por_empilhavel(&mut sim, r.saida, n, cap) {
        return Err("bolsa cheia".into());
    }
    let sucessos = (0..n).filter(|_| shared::combinar::deu_certo(r, sorte())).count() as u32;
    cobra(inv);
    if sucessos > 0 {
        // Cabe: o pior caso (n) coube na simulacao.
        por_empilhavel(inv, r.saida, sucessos, cap);
    }
    Ok((n as u16, sucessos as u16))
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

    fn foto(inv: &[InventorySlot]) -> Vec<(u16, u32, bool)> {
        inv.iter().map(|s| (s.item_id, s.qty, s.instance.is_some())).collect()
    }

    fn peca(cor: u8, tier: u8, refino: u8) -> InventorySlot {
        let mut i = ItemInstance::roll_for(item_id::KATANA, 5, || 0.5).unwrap();
        i.rarity = cor;
        i.tier = tier;
        i.refinement = refino;
        InventorySlot {
            item_id: item_id::KATANA,
            qty: 1,
            instance: Some(i),
        }
    }

    fn rolar_katana(id: u16, nivel: u16, cor: u8, tier: u8) -> Option<ItemInstance> {
        ItemInstance::roll_em(shared::items::item_template(id), nivel, cor, tier, || 0.5)
    }

    #[test]
    fn aprimorar_sobe_o_tier_na_mesma_cor_e_zera_o_refino() {
        let mut inv = bolsa(&[(item_id::COPPER, 600)]);
        inv[3] = peca(1, 1, 4);
        inv[7] = peca(1, 1, 0);
        let antes = rolar_katana(item_id::KATANA, 5, 1, 1).unwrap().attack_damage;
        assert_eq!(
            aprimorar(&mut inv, 3, 7, 1, &mut rolar_katana),
            Ok((item_id::KATANA, 1, 2))
        );
        let nova = inv[3].instance.unwrap();
        assert_eq!((nova.grau(), nova.tier(), nova.refinement), (1, 2, 0));
        assert!(nova.attack_damage > antes, "Tier II rola mais forte que o I");
        assert_eq!(inv[7].qty, 0);
        assert_eq!(tem(&inv, item_id::COPPER), 100);
    }

    #[test]
    fn duas_tier_iv_mais_8_sobem_de_cor_no_tier_i() {
        let mut inv = bolsa(&[(item_id::COPPER, 99_999)]);
        inv[3] = peca(1, 4, 8);
        inv[7] = peca(1, 4, 7);
        let e = aprimorar(&mut inv, 3, 7, 30, &mut rolar_katana).unwrap_err();
        assert!(e.contains("+8"), "{e}");
        inv[7] = peca(1, 4, 9);
        // Verde pede nivel 20.
        let e = aprimorar(&mut inv, 3, 7, 19, &mut rolar_katana).unwrap_err();
        assert!(e.contains("nível 20"), "{e}");
        assert_eq!(
            aprimorar(&mut inv, 3, 7, 20, &mut rolar_katana),
            Ok((item_id::KATANA, 2, 1))
        );
        let nova = inv[3].instance.unwrap();
        assert_eq!((nova.grau(), nova.tier(), nova.refinement), (2, 1, 0));
        assert_eq!(nova.item_level, forja::nivel_de_item_da_cor(2));
    }

    #[test]
    fn aprimorar_recusa_sem_cobre_diferentes_e_a_mesma_peca() {
        let mut inv = bolsa(&[(item_id::COPPER, 10)]);
        inv[3] = peca(1, 1, 0);
        inv[7] = peca(1, 1, 0);
        let copia = foto(&inv);
        assert!(aprimorar(&mut inv, 3, 7, 1, &mut rolar_katana).is_err());
        assert_eq!(foto(&inv), copia, "recusa nao mexe na bolsa");
        let mut inv = bolsa(&[(item_id::COPPER, 99_999)]);
        inv[3] = peca(1, 1, 0);
        inv[7] = peca(1, 2, 0);
        assert!(aprimorar(&mut inv, 3, 7, 1, &mut rolar_katana).is_err());
        inv[7] = peca(2, 1, 0);
        assert!(aprimorar(&mut inv, 3, 7, 60, &mut rolar_katana).is_err());
        assert!(aprimorar(&mut inv, 3, 3, 1, &mut rolar_katana).is_err());
    }

    #[test]
    fn combinar_cobra_todas_e_entrega_so_os_sucessos() {
        let r = shared::combinar::receita(item_id::HORN).unwrap();
        let mut inv = bolsa(&[(item_id::HORN, 23)]);
        // Sorte alterna 0 (certo) e 50 (errado): 4 tentativas, 2 sucessos.
        let mut k = 0u8;
        let mut sorte = || {
            k = k.wrapping_add(1);
            if k % 2 == 1 { 0 } else { 50 }
        };
        let r2 = combinar(&mut inv, &r, 50, 99, &nome, &mut sorte);
        assert_eq!(r2, Ok((4, 2)));
        assert_eq!(tem(&inv, item_id::HORN), 3);
        assert_eq!(tem(&inv, r.saida), 2);
        // 3 chifres nao pagam uma tentativa.
        let e = combinar(&mut inv, &r, 1, 99, &nome, &mut || 0).unwrap_err();
        assert!(e.contains("3/5"), "{e}");
    }

    #[test]
    fn sintese_de_material_cobra_cobre_darksteel_e_po() {
        let r = shared::combinar::receita(item_id::STEEL).unwrap();
        let mut inv = bolsa(&[
            (item_id::STEEL, 25),
            (item_id::COPPER, 4_500),
            (item_id::DARKSTEEL, 3_000),
            (item_id::GLITTERING_POWDER, 9),
        ]);
        assert_eq!(combinar(&mut inv, &r, 50, 999, &nome, &mut || 99), Ok((2, 2)));
        assert_eq!(tem(&inv, item_id::STEEL), 5);
        assert_eq!(tem(&inv, item_id::COPPER), 500);
        assert_eq!(tem(&inv, item_id::DARKSTEEL), 1_000);
        assert_eq!(tem(&inv, item_id::GLITTERING_POWDER), 5);
        assert_eq!(tem(&inv, r.saida), 2);
    }

    #[test]
    fn combinar_com_bolsa_cheia_nao_cobra() {
        let r = shared::combinar::receita(item_id::HORN).unwrap();
        let mut inv: Vec<InventorySlot> = (0..4)
            .map(|i| InventorySlot {
                item_id: 900 + i,
                qty: 1,
                instance: None,
            })
            .collect();
        inv[0] = InventorySlot {
            item_id: item_id::HORN,
            qty: 7,
            instance: None,
        };
        let copia = foto(&inv);
        // 7 chifres = 1 tentativa, sobra 2: o slot nao libera e nao ha' vazio.
        assert_eq!(combinar(&mut inv, &r, 1, 99, &nome, &mut || 0), Err("bolsa cheia".into()));
        assert_eq!(foto(&inv), copia);
    }

    #[test]
    fn peca_sem_instancia_vira_comum_tier_i_e_o_resto_fica() {
        let mut rolar = |id: u16| rolar_katana(id, 5, 1, 1);
        let mut inv = bolsa(&[(item_id::COPPER, 50)]);
        inv[2] = InventorySlot {
            item_id: item_id::KATANA,
            qty: 1,
            instance: None,
        };
        inv[3] = peca(2, 3, 4);
        assert!(garantir_instancias(&mut inv, &mut rolar));
        let nova = inv[2].instance.expect("ganhou instancia");
        assert_eq!((nova.grau(), nova.tier(), nova.refinement), (1, 1, 0));
        assert!(inv[0].instance.is_none(), "cobre continua sem instancia");
        assert_eq!(inv[3].instance.unwrap().tier(), 3, "peca que ja' tinha nao muda");
        assert!(!garantir_instancias(&mut inv, &mut rolar), "segunda vez nao mexe");

        let mut equip = shared::Equipment::default();
        equip.set(shared::EquipSlot::Weapon, Some(item_id::KATANA), None);
        assert!(garantir_instancias_vestidas(&mut equip, &mut rolar));
        assert_eq!(equip.get_inst(shared::EquipSlot::Weapon).map(|i| i.tier()), Some(1));
    }
}
