//! Generates test attachments; touches neither database nor mail.
fn main() {
    let mut cartas = Vec::new();
    for (grau, cor) in [(2,"Verde"),(3,"Azul"),(4,"Roxo"),(5,"Laranja")] {
        for refino in [0,5,7,10] {
            let pecas: Vec<_> = (400..=414).map(|id| {
                let mut inst = shared::items::ItemInstance::roll_em(
                    shared::items::item_template(id), 1, grau, 1, || 0.5
                ).expect("equipamento sem template");
                inst.refinement = refino;
                inst.vinculado = true;
                assert_eq!(inst.level_req, None);
                shared::social::Anexo { item_id: id, qtd: 1, instance: Some(inst) }
            }).collect();
            for (parte, anexos) in pecas.chunks(shared::social::MAX_ANEXOS).enumerate() {
                assert!(shared::social::anexos_validos(anexos));
                cartas.push(serde_json::json!({
                    "chave": format!("auras-onurb-20260926-g{grau}-r{refino}-p{parte}"),
                    "assunto": format!("Teste de auras: {cor} +{refino} ({}/2)", parte+1),
                    "texto": "Kit solicitado para testar auras por equipamento. Peças vinculadas, Tier I, sem requisito de nível. Resgate por partes conforme o espaço da bolsa.",
                    "anexos": anexos,
                }));
            }
        }
    }
    println!("{}", serde_json::to_string(&cartas).unwrap());
}
