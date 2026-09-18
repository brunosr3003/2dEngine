//! Previa local sem servidor: MMO_PREVIA_SKILLS=1, espaco pausa, setas escolhem.
use crate::{habilidades_vfx, render3d, vox::VoxCache, world::World};
use macroquad::material::{gl_use_default_material, gl_use_material};
use macroquad::prelude::*;

pub async fn abrir(vox: &VoxCache) {
    let catalogo = shared::skills::playtest();
    let mut mundo = World::default();
    let mut metas = Vec::new();
    let mut estados = Vec::new();
    for (id, tag, kind, p) in [
        (1, shared::EntityTag::Player, 0, vec2(0.0, 0.0)),
        (2, shared::EntityTag::Enemy, 0, vec2(0.0, 2.0)),
        (3, shared::EntityTag::Enemy, 2, vec2(-2.5, 0.5)),
        (4, shared::EntityTag::Enemy, 4, vec2(-3.5, 2.5)),
        (5, shared::EntityTag::Enemy, 6, vec2(2.8, 0.5)),
    ] {
        metas.push(shared::EntityMeta {
            id: shared::EntityId(id),
            tag,
            name: None,
            hp_max: 100,
            faction: None,
            kind,
            nivel: 10,
        });
        estados.push(shared::EntityState::quantize(
            shared::EntityId(id),
            ::glam::Vec2::new(p.x, p.y),
            ::glam::Vec2::ZERO,
            100,
            0,
        ));
    }
    mundo.apply(metas, estados, &[]);
    mundo.self_id = Some(shared::EntityId(1));
    // Minimapa da ilha de verdade na previa do HUD; o ultimo quadro sai com ele fechado.
    let mut mapa = crate::mapa::Mapa::para(shared::terreno::def_da_zona("ilha_inicial"));
    let solido = render3d::material_solido();
    let luz = habilidades_vfx::material();
    let mut tempo = 0.0f32;
    let mut escolha = 0usize;
    let mut pausa = false;
    let automatico = std::env::var("MMO_PREVIA_EXPORTAR").is_ok();
    let previa_hud = std::env::var("MMO_PREVIA_HUD").is_ok();
    let mut habilidades = crate::habilidades::Habilidades::default();
    habilidades.catalogo = catalogo.clone();
    habilidades.automaticas.extend([1, 4, 6, 8, 11, 12]);
    habilidades.estado(vec![(5, 6.5), (2, 3.8), (9, 10.0), (12, 8.0)], 0.0);
    let mut ficha = crate::hud::Ficha::default();
    ficha.mp = Some(126);
    ficha.vigor = Some(82);
    ficha.nivel = 20;
    let mut auto = crate::auto_combate::AutoCombate::default();
    auto.ligar(Vec2::ZERO);
    let tamanho = if previa_hud {
        (screen_width() as u32, screen_height() as u32)
    } else {
        (1280, 800)
    };
    let mut alvo_render = render_target(tamanho.0, tamanho.1);
    let abriu_em = get_time();
    let mut quadro = 0;
    loop {
        if previa_hud
            && (alvo_render.texture.width() != screen_width()
                || alvo_render.texture.height() != screen_height())
        {
            alvo_render = render_target(screen_width() as u32, screen_height() as u32);
        }
        if is_key_pressed(KeyCode::Escape) {
            break;
        }
        if is_key_pressed(KeyCode::Space) {
            pausa = !pausa;
        }
        if is_key_pressed(KeyCode::Right) {
            escolha = (escolha + 1) % 12;
            tempo = 0.0;
        }
        if is_key_pressed(KeyCode::Left) {
            escolha = (escolha + 11) % 12;
            tempo = 0.0;
        }
        let skill = &catalogo[escolha];
        if !pausa {
            tempo += get_frame_time();
        }
        let t = if automatico {
            skill.impacto_em() + 0.17
        } else {
            tempo % (skill.impacto_em() + habilidades_vfx::duracao(skill.id) + 0.4)
        };
        mundo.tick(get_frame_time(), &|_, _| 0.0);
        for (&id, e) in &mut mundo.ents {
            e.yaw = if id.0 == 2 { std::f32::consts::PI } else { 0.0 };
            if id.0 == 1 {
                e.state.acao = shared::components::acao::monta(
                    skill.conjunto as u8,
                    true,
                    shared::components::acao::SKILL,
                    0,
                );
                e.skill = Some((skill.id, t, skill.impacto_em()));
                e.sacada = 1.0;
            } else if id.0 >= 3 {
                e.ataque_mob = Some((Some(shared::EntityId(2)), t % 0.8, 0.4));
            }
        }
        clear_background(Color::new(0.055, 0.07, 0.095, 1.0));
        let mut vista = render3d::Vista::nova(vec2(0.0, 1.0), 0.0, 0.4, 0.8, 0.0, &|_, _| 0.0);
        vista.cam.position = vec3(6.5, 7.0, -8.5);
        vista.cam.target = vec3(0.0, 0.9, 1.0);
        if automatico {
            vista.cam.render_target = Some(alvo_render.clone());
        }
        set_camera(&vista.cam);
        clear_background(Color::new(0.055, 0.07, 0.095, 1.0));
        draw_plane(
            Vec3::ZERO,
            vec2(18.0, 18.0),
            None,
            Color::new(0.16, 0.19, 0.20, 1.0),
        );
        gl_use_material(&solido);
        solido.set_uniform("Recorte", Vec3::ZERO);
        render3d::draw_entities(&mut mundo, vox, None, &vista);
        gl_use_default_material();
        let de = Vec3::ZERO;
        habilidades_vfx::desenha(
            &habilidades_vfx::Cena {
                id: skill.id,
                de,
                alvo: if skill.dano > 0 {
                    vec3(0.0, 0.0, 2.0)
                } else {
                    de
                },
                maos: mundo.ents[&shared::EntityId(1)].emissores,
                t: if t >= skill.impacto_em() {
                    t - skill.impacto_em()
                } else {
                    t
                },
                impacto: t >= skill.impacto_em(),
                atraso: skill.impacto_em(),
                raio: skill.raio,
                frente: Vec3::Z,
            },
            &luz,
        );
        set_default_camera();
        if previa_hud {
            let mut camera =
                Camera2D::from_display_rect(Rect::new(0.0, 0.0, screen_width(), screen_height()));
            if automatico {
                camera.render_target = Some(alvo_render.clone());
            }
            set_camera(&camera);
            let z = crate::hud_layout::atual();
            crate::hud::draw_hud(
                &z,
                &crate::hud::Info {
                    realm: "Tempest".into(),
                    canal: "1".into(),
                    zona: "Vale dos Ventos".into(),
                    jogadores: 12,
                    capacidade: 100,
                },
                &crate::hud::Rede {
                    ms: 24.0,
                    ..Default::default()
                },
                None,
                0,
                5,
                0,
                0,
                Vec2::ZERO,
                &[
                    "Guardião de Treino sofreu 245 de dano.",
                    "Julgamento está pronto.",
                    "Auto coleta: atacado, revidando.",
                ],
            );
            crate::hud::draw_ficha(
                &z,
                &mut ficha,
                0.016,
                "brunji",
                20,
                1840,
                2400,
                180,
                100,
                Some(12750),
            );
            crate::hud::draw_alvo(&z, "Guardião de Treino", 20, 38450, 50000, true);
            crate::hud::draw_exp(&z, &ficha, 20);
            if !automatico {
                let _ = habilidades.pedido(crate::habilidades::Contexto {
                    conjunto: skill.conjunto,
                    nivel: 20,
                    mp: 126,
                    vivo: true,
                    vida_baixa: false,
                    distancia_alvo: None,
                });
            }
            habilidades.barra(skill.conjunto, 20, 126);
            auto.desenha();
            crate::hud::draw_atacar(&z, true);
            crate::hud::draw_rapidos(
                &z,
                crate::barra::padrao().map(|e| e.item_id),
                [12, 5, 3, 1],
                [true, false, false, false],
                [Some((5.0, 8.0)), None, None, None],
                [true, false, false, false],
                None,
                &|_| String::new(),
            );
            crate::hud::draw_topo(&z, true, true, true, true);
            crate::joystick::Joystick::default().desenha(&z);
            mapa.acompanhar();
            crate::hud_layout::define_minimapa_oculto(automatico && escolha == 11);
            mapa.desenha_mini(&mundo);
            set_default_camera();
        } else {
            draw_text(
                &format!("{} / 12   {}", escolha + 1, skill.nome),
                28.0,
                42.0,
                28.0,
                WHITE,
            );
            draw_text(
                "Setas: skill    Espaco: pausar    Esc: sair",
                28.0,
                screen_height() - 28.0,
                20.0,
                GRAY,
            );
        }
        if automatico && get_time() - abriu_em > 2.0 {
            quadro += 1;
            if quadro % 8 == 0 {
                unsafe {
                    get_internal_gl().flush();
                }
                alvo_render.texture.get_texture_data().export_png(&format!(
                    "/tmp/2dengine-{}-{:02}.png",
                    if previa_hud { "hud" } else { "skill" },
                    escolha + 1
                ));
                escolha += 1;
                if escolha == 12 {
                    break;
                }
            }
        }
        next_frame().await;
    }
}
