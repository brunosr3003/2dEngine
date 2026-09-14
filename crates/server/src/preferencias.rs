//! Preferencias de tela do personagem (skills AUTO, filtros do mapa, zooms).
//! Quem USA e' o cliente; aqui so' se valida o que ele manda e se guarda
//! (JSON em `characters.preferencias_json`), no molde da `barra`.
use shared::protocol::{Preferencias, PREFERENCIAS_MAX_BYTES};

/// A skill existe no catalogo? Id que nao existe (skill removida, cliente
/// modificado) nao vai pro banco.
fn skill_existe(id: u32) -> bool {
    shared::skills::playtest().iter().any(|s| s.id == id)
}

pub fn valida(p: Preferencias) -> Preferencias {
    p.validada(&skill_existe)
}

pub fn para_json(p: &Preferencias) -> String {
    serde_json::to_string(p).unwrap_or_default()
}

/// Vazio, grande demais ou estragado vira o padrao: o cliente fica com as
/// escolhas dele de sempre.
pub fn de_json(s: &str) -> Preferencias {
    if s.is_empty() || s.len() > PREFERENCIAS_MAX_BYTES {
        return Preferencias::default();
    }
    serde_json::from_str::<Preferencias>(s).map(valida).unwrap_or_default()
}

/// O que o cliente mandou, pronto pra guardar — ou `None` se nem validado
/// cabe no teto (so' com lista absurda; as listas ja' tem teto).
pub fn aceita(p: Preferencias) -> Option<Preferencias> {
    let p = valida(p);
    (para_json(&p).len() <= PREFERENCIAS_MAX_BYTES).then_some(p)
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn json_ida_e_volta() {
        let p = valida(Preferencias {
            skills_auto: vec![1, 4],
            alcance_minimapa: Some(150.0),
            camera_zoom: Some(1.3),
            ..Default::default()
        });
        assert_eq!(de_json(&para_json(&p)), p);
    }

    #[test]
    fn json_vazio_antigo_extra_ou_lixo_e_tolerado() {
        assert_eq!(de_json(""), Preferencias::default());
        assert_eq!(de_json("{nao e' json"), Preferencias::default());
        // Antigo: so' um campo, o resto vem do padrao.
        assert_eq!(de_json(r#"{"skills_auto":[2]}"#).skills_auto, vec![2]);
        // Campo que esta versao nao conhece e' ignorado.
        let p = de_json(r#"{"skills_auto":[3],"coisa_do_futuro":{"x":1}}"#);
        assert_eq!(p.skills_auto, vec![3]);
    }

    #[test]
    fn skill_inexistente_sai_e_json_grande_demais_e_recusado() {
        assert_eq!(de_json(r#"{"skills_auto":[1,60000]}"#).skills_auto, vec![1]);
        let enorme = format!(r#"{{"skills_auto":[{}]}}"#, vec!["1"; 5000].join(","));
        assert!(enorme.len() > PREFERENCIAS_MAX_BYTES);
        assert_eq!(de_json(&enorme), Preferencias::default());
        // Pela rede: lista enorme e' cortada no teto e cabe.
        let p = aceita(Preferencias { skills_auto: (1..=5000).collect(), ..Default::default() }).unwrap();
        assert!(p.skills_auto.len() <= 12);
    }
}
