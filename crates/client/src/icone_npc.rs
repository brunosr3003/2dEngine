//! O icone do oficio em cima de cada NPC da vila: pocao no Alquimista,
//! bigorna no Ferreiro, cofre no Banqueiro, ancora no Capitao... Da' pra
//! achar quem se quer sem chegar perto pra ler o nome.
//!
//! Fica logo acima da cabeca; o "!"/"?" de missao continua mais alto.

use macroquad::prelude::*;
use shared::construcao::Papel;

use crate::hud_estilo as estilo;

/// De onde vem o desenho.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Icone {
    /// Atlas da interface (`icones_ui::ui`).
    Ui(&'static str),
    /// Atlas dos marcadores do mapa (`icones_ui::mapa`).
    Mapa(&'static str),
    /// Icone de item (`bolsa::icone_do_item`).
    Item(u16),
}

/// Longe disto o icone some (so' polui).
const ALCANCE: f32 = 40.0;

/// O icone do papel do NPC; `None` pra quem nao tem oficio (morador).
pub fn do_papel(papel: u8) -> Option<Icone> {
    use shared::item_id as it;
    let p = |x: Papel| x as u8 == papel;
    Some(if p(Papel::Alquimista) {
        Icone::Item(it::HEALTH_POTION)
    } else if p(Papel::Ferreiro) {
        Icone::Ui("forja")
    } else if p(Papel::Armas) || p(Papel::Armaduras) {
        Icone::Item(it::ARMADURA_MEDIA)
    } else if p(Papel::Alfaiate) {
        Icone::Item(it::MANTO_DO_GUERREIRO)
    } else if p(Papel::Taberna) {
        Icone::Ui("caneca")
    } else if p(Papel::Treinador) {
        Icone::Ui("habilidades")
    } else if p(Papel::Identificador) {
        Icone::Ui("encantar")
    } else if p(Papel::Cartografo) {
        Icone::Ui("mapa")
    } else if p(Papel::Deposito) {
        Icone::Ui("banco")
    } else if p(Papel::Estaleiro) {
        Icone::Mapa("porto")
    } else if p(Papel::Missoes) {
        Icone::Ui("missoes")
    } else if p(Papel::Itens) || p(Papel::Mercador) {
        Icone::Ui("lojas")
    } else {
        return None;
    })
}

/// Desenha os icones dos NPCs perto de `eu`.
pub fn desenha(world: &crate::world::World, vista: &crate::render3d::Vista<'_>, eu: Option<Vec2>) {
    let Some(eu) = eu else {
        return;
    };
    let lado = 42.0 * estilo::fator_texto();
    let mestre = Papel::Missoes.nome();
    for e in world.ents.values() {
        if e.meta.tag != shared::EntityTag::Npc || e.morte.is_some() {
            continue;
        }
        let dist = e.render_pos.distance(eu);
        if dist > ALCANCE {
            continue;
        }
        // O Mestre de Missoes se reconhece pelo nome (o kind dele e' generico).
        let papel = if e.meta.name.as_deref() == Some(mestre) {
            Papel::Missoes as u8
        } else {
            shared::npc_papel_de_kind(e.meta.kind)
        };
        let Some(icone) = do_papel(papel) else {
            continue;
        };
        let topo = vista.pos_de(e) + vec3(0.0, 2.15, 0.0);
        let Some(c) = crate::render3d::world_to_screen(&vista.cam, topo) else {
            continue;
        };
        // Some no fim do alcance em vez de sumir de uma vez.
        let a = ((ALCANCE - dist) / 8.0).clamp(0.0, 1.0);
        draw_circle(c.x, c.y, lado * 0.62, Color::new(0.05, 0.06, 0.09, 0.72 * a));
        draw_circle_lines(c.x, c.y, lado * 0.62, 1.5, estilo::alfa(estilo::OURO, 0.8 * a));
        let cor = estilo::alfa(estilo::OURO, a);
        match icone {
            Icone::Ui(n) => {
                crate::icones_ui::ui(n, c, lado * 0.8, cor);
            }
            Icone::Mapa(n) => {
                crate::icones_ui::mapa(n, c, lado * 0.8, cor, 0.0);
            }
            Icone::Item(id) => {
                let l = lado * 0.86;
                crate::bolsa::icone_do_item(Rect::new(c.x - l * 0.5, c.y - l * 0.5, l, l), id, a);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn todo_npc_da_vila_tem_icone_e_morador_nao() {
        for p in [
            Papel::Alquimista,
            Papel::Ferreiro,
            Papel::Armaduras,
            Papel::Taberna,
            Papel::Alfaiate,
            Papel::Treinador,
            Papel::Identificador,
            Papel::Cartografo,
            Papel::Deposito,
            Papel::Estaleiro,
            Papel::Missoes,
        ] {
            assert!(do_papel(p as u8).is_some(), "{p:?} sem icone");
        }
        assert_eq!(do_papel(Papel::Casa as u8), None);
        assert_eq!(do_papel(Papel::Deposito as u8), Some(Icone::Ui("banco")));
    }
}
