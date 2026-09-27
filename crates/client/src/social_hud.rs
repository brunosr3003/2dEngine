//! Ações do alvo e grupo compacto, ao lado do rastreador de missões.
use crate::{hud_estilo as e, hud_layout::Zonas, world::World};
use macroquad::prelude::*;
use shared::EntityId;

#[derive(Default)]
pub struct SocialHud {
    pub aberto: Option<EntityId>,
    pub recolhido: bool,
    pub pacifico: Option<EntityId>,
}
#[derive(Clone, Copy)]
pub enum Acao {
    Inspecionar,
    Amigo,
    Seguir,
    Grupo,
    Selecionar(EntityId),
    AbrirGrupo,
}

pub fn grupo_rect(z: &Zonas, membros: usize, recolhido: bool) -> Rect {
    let s = z.s;
    Rect::new(
        z.rastreador.x + z.rastreador.w + 8.0 * s,
        z.rastreador.y,
        220.0 * s,
        ((36.0
            + if recolhido {
                0.0
            } else {
                membros.min(5) as f32 * 49.0 + 30.0
            })
            * s)
            .min((z.faixa.y - z.rastreador.y - 8.0 * s).max(36.0 * s)),
    )
}
fn menu_rect(z: &Zonas) -> Rect {
    Rect::new(
        z.alvo.x,
        z.alvo.y + z.alvo.h + 4.0 * z.s,
        z.alvo.w,
        126.0 * z.s,
    )
}
impl SocialHud {
    pub fn captura(&self, z: &Zonas, p: Vec2, jogador: bool, membros: usize) -> bool {
        self.aberto.is_some()
            || (jogador && z.alvo.contains(p))
            || (membros > 0 && grupo_rect(z, membros, self.recolhido).contains(p))
    }
    pub fn alvo(&mut self, z: &Zonas, id: Option<EntityId>, seguindo: bool) -> Option<Acao> {
        if self.aberto != id {
            self.aberto = None;
        }
        let Some(id) = id else { return None };
        let p = Vec2::from(mouse_position());
        let clicou = crate::foco::clique();
        e::texto(
            z.alvo.x + z.alvo.w - 108.0 * z.s,
            z.alvo.y + 16.0 * z.s,
            if self.aberto.is_some() {
                "Ações −"
            } else {
                "Ações +"
            },
            (11.0 * z.s) as u16,
            e::OURO,
        );
        if clicou && z.alvo.contains(p) && !z.alvo_fechar().contains(p) {
            self.aberto = if self.aberto.is_some() {
                None
            } else {
                Some(id)
            };
            return self.aberto.map(|_| Acao::Inspecionar);
        }
        if self.aberto.is_none() {
            return None;
        }
        let r = menu_rect(z);
        e::painel(r);
        for (i, (titulo, acao)) in [
            ("Adicionar amigo", Acao::Amigo),
            (
                if seguindo {
                    "Parar de seguir"
                } else {
                    "Seguir jogador"
                },
                Acao::Seguir,
            ),
            ("Convidar para grupo", Acao::Grupo),
        ]
        .into_iter()
        .enumerate()
        {
            let linha = Rect::new(r.x, r.y + i as f32 * 42.0 * z.s, r.w, 42.0 * z.s);
            if linha.contains(p) {
                draw_rectangle(
                    linha.x,
                    linha.y,
                    linha.w,
                    linha.h,
                    Color::new(1., 1., 1., 0.09),
                );
            }
            e::texto(
                linha.x + 12.0 * z.s,
                linha.y + 27.0 * z.s,
                titulo,
                (15.0 * z.s) as u16,
                e::TEXTO,
            );
            if clicou && linha.contains(p) {
                self.aberto = None;
                return Some(acao);
            }
        }
        if clicou {
            self.aberto = None;
        }
        None
    }
    pub fn grupo(&mut self, z: &Zonas, membros: &[String], world: &World) -> Option<Acao> {
        if membros.is_empty() {
            return None;
        }
        let r = grupo_rect(z, membros.len(), self.recolhido);
        let s = z.s;
        let p = Vec2::from(mouse_position());
        let clicou = crate::foco::clique();
        e::painel(r);
        e::texto(
            r.x + 10.0 * s,
            r.y + 24.0 * s,
            &format!(
                "GRUPO · {}  {}",
                membros.len(),
                if self.recolhido { "+" } else { "−" }
            ),
            (14.0 * s) as u16,
            e::OURO,
        );
        if clicou && Rect::new(r.x, r.y, r.w, 36.0 * s).contains(p) {
            self.recolhido = !self.recolhido;
            return None;
        }
        if self.recolhido {
            return None;
        }
        let altura = (r.h - 66.0 * s) / membros.len().min(5) as f32;
        let k = (altura / 49.0).min(s);
        for (i, nome) in membros.iter().take(5).enumerate() {
            let linha = Rect::new(
                r.x + 8.0 * s,
                r.y + 36.0 * s + i as f32 * altura,
                r.w - 16.0 * s,
                altura,
            );
            let jogador = world.ents.iter().find(|(_, ent)| {
                ent.meta.tag == shared::EntityTag::Player
                    && ent.meta.name.as_deref() == Some(nome.as_str())
            });
            e::texto_ajustado(
                nome,
                linha.x,
                linha.y + 17.0 * k,
                linha.w,
                (14.0 * s) as u16,
                e::TEXTO,
            );
            let y = linha.y + 24.0 * k;
            draw_rectangle(
                linha.x,
                y,
                linha.w,
                15.0 * k,
                Color::new(0.08, 0.09, 0.12, 1.),
            );
            if let Some((id, ent)) = jogador {
                let fracao = (ent.state.hp as f32 / ent.meta.hp_max.max(1) as f32).clamp(0., 1.);
                draw_rectangle(
                    linha.x,
                    y,
                    linha.w * fracao,
                    15.0 * k,
                    Color::new(0.2, 0.58, 0.36, 1.),
                );
                e::texto(
                    linha.x + 4.0 * s,
                    y + 12.0 * k,
                    &format!("{} / {}", ent.state.hp, ent.meta.hp_max),
                    (10.0 * k).max(9.0) as u16,
                    e::TEXTO,
                );
                if clicou && linha.contains(p) && Some(*id) != world.self_id {
                    return Some(Acao::Selecionar(*id));
                }
            } else {
                e::texto(
                    linha.x + 4.0 * s,
                    y + 12.0 * k,
                    "Fora de alcance",
                    (10.0 * k).max(9.0) as u16,
                    e::TEXTO,
                );
            }
        }
        let rodape = Rect::new(r.x, r.y + r.h - 30.0 * s, r.w, 30.0 * s);
        e::texto(
            rodape.x + 10.0 * s,
            rodape.y + 20.0 * s,
            "Gerenciar grupo ›",
            (12.0 * s) as u16,
            e::OURO,
        );
        if clicou && rodape.contains(p) {
            return Some(Acao::AbrirGrupo);
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn grupo_ao_lado_das_quests_cabe_sem_cobrir_os_controles() {
        for (w, h) in [(1280., 720.), (1920., 1080.), (2340., 1080.), (1024., 768.)] {
            for ui in [0.8, 1., 1.3, 1.6] {
                let z = crate::hud_layout::zonas_com(w, h, [0.; 4], ui, false, false, false);
                let r = grupo_rect(&z, 5, false);
                assert!(r.x + r.w <= w && r.y + r.h <= h, "{w}x{h} escala {ui}");
                for (_, outro) in z.todos() {
                    assert!(
                        !r.overlaps(&outro),
                        "grupo cobre HUD em {w}x{h} escala {ui}: {r:?} / {outro:?}"
                    );
                }
            }
        }
    }
}

#[cfg(debug_assertions)]
pub async fn previa() {
    let saida =
        std::env::var("MMO_PREVIA_SAIDA").unwrap_or_else(|_| "/tmp/tempest-social-hud".into());
    std::fs::create_dir_all(&saida).unwrap();
    for (w, h) in [(1280, 720), (1920, 1080)] {
        let rt = render_target(w, h);
        crate::render3d::define_alvo(Some(rt.clone()));
        crate::hud_layout::define_escala_ui(1.6);
        let z = crate::hud_layout::atual();
        let mut ui = SocialHud {
            aberto: Some(EntityId(2)),
            ..Default::default()
        };
        let nomes = ["brunji", "MaréAlta", "Navegante", "Nuninzera"].map(String::from);
        let mut world = World::default();
        for (i, nome) in nomes.iter().take(3).enumerate() {
            let id = EntityId(i as u32 + 1);
            world.apply(
                vec![shared::EntityMeta {
                    pk: Default::default(),
                    auras: 0,
                    id,
                    tag: shared::EntityTag::Player,
                    name: Some(nome.clone()),
                    hp_max: 1200,
                    faction: None,
                    kind: 0,
                    nivel: 70,
                    desafio: None,
                    aparencia: 0,
                }],
                vec![shared::EntityState {
                    id,
                    pos: [16, 16],
                    vel: [0, 0],
                    hp: 1000 - i as u16 * 300,
                    flags: if i == 0 { shared::ent_flags::SELF } else { 0 },
                    acao: 0,
                    rumo: 0,
                }],
                &[],
            );
        }
        for _ in 0..3 {
            crate::render3d::camera_padrao();
            clear_background(Color::new(0.08, 0.12, 0.16, 1.));
            e::painel(z.ficha);
            e::texto(
                z.ficha.x + 12.,
                z.ficha.y + 28.,
                "Personagem · Lv 70",
                20,
                e::TEXTO,
            );
            e::painel(z.rastreador);
            e::texto(
                z.rastreador.x + 12.,
                z.rastreador.y + 24.,
                "MISSÕES",
                16,
                e::OURO,
            );
            e::texto(
                z.rastreador.x + 12.,
                z.rastreador.y + 64.,
                "Derrote monstros da floresta  3/10",
                14,
                e::TEXTO,
            );
            crate::hud::draw_alvo(&z, "MaréAlta", 72, 850, 1200, false);
            ui.grupo(&z, &nomes, &world);
            ui.alvo(&z, Some(EntityId(2)), false);
            unsafe { get_internal_gl().flush() };
            rt.texture
                .get_texture_data()
                .export_png(&format!("{saida}/hud-{w}.png"));
            next_frame().await;
        }
    }
}
