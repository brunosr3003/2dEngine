//! Gera um conjunto de teste T4 +10 para katana; não envia correio.
fn main() {
    let anexos: Vec<_> = [401,405,408,411,412,413,414].into_iter().map(|id| {
        let mut inst = shared::items::ItemInstance::roll_em(
            shared::items::item_template(id), 1, 5, 4, || 0.5
        ).expect("equipamento sem template");
        inst.refinement = 10;
        inst.vinculado = true;
        assert_eq!(inst.level_req, None);
        shared::social::Anexo { item_id: id, qtd: 1, instance: Some(inst) }
    }).collect();
    assert!(shared::social::anexos_validos(&anexos));
    println!("{}", serde_json::to_string(&anexos).unwrap());
}
