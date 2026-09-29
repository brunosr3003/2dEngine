//! The trade icon above each village NPC: a potion on the Alchemist, an anvil
//! on the Blacksmith, a safe on the Banker, an anchor on the Captain... You
//! can find who you want without walking up to read the name.
//!
//! It sits just above the head; the quest "!"/"?" stays higher.

use macroquad::prelude::*;
use shared::construcao::Papel;

use crate::hud_estilo as estilo;

/// Where the drawing comes from.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Icone {
    /// Interface atlas (`icones_ui::ui`).
    Ui(&'static str),
    /// Map marker atlas (`icones_ui::mapa`).
    Mapa(&'static str),
    /// Item icon (`bolsa::icone_do_item`).
    Item(u16),
}

/// Beyond this the icon disappears (it only clutters).
const ALCANCE: f32 = 40.0;

/// The icon for the NPC's role; `None` for someone with no trade (a resident).
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

/// Draws the icons of the NPCs near `eu`.
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
        // The Quest Master is recognised by name (its kind is generic).
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
        // Fades at the end of the range instead of vanishing at once.
        let a = ((ALCANCE - dist) / 8.0).clamp(0.0, 1.0);
        draw_circle(
            c.x,
            c.y,
            lado * 0.62,
            Color::new(0.05, 0.06, 0.09, 0.72 * a),
        );
        draw_circle_lines(
            c.x,
            c.y,
            lado * 0.62,
            1.5,
            estilo::alfa(estilo::OURO, 0.8 * a),
        );
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
