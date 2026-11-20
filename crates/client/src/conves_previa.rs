//! PREVIEWS of the walkable deck (docs/NAVEGACAO.md, "Convés andável"):
//! `MMO_PREVIA_CONVES=1` writes to `MMO_PREVIA_SAIDA` (default
//! `target/convés`):
//!
//! - `conves-andando-<class>`: bodies walking and standing on each hull's
//!   deck, the ship under way, the land camera's angle;
//! - `conves-leme`: the captain at the wheel, through the sailing camera;
//! - `conves-grupo-tubarao`: a party of three fighting a surfaced shark from
//!   the rail;
//! - `conves-vela-<n>`: the sail's cloth rippling, a short frame sequence;
//! - `conves-desfralda-<n>`: a ship getting under way, the sails unfurling.
//!
//! The same draw as the game: `mar_cena::desenha_mundo` for the sea and
//! `render3d::draw_entities` for the ship entities and the bodies on their
//! decks (`world::World::poe_nos_conveses`). It prints the draw calls and
//! indices of each class's ship.

use macroquad::prelude::*;
use shared::conves::NoConves;
use shared::navio::{self as nv, Visual};
use shared::{EntityId, EntityMeta, EntityState, EntityTag};

use crate::world::World;

fn g(v: Vec2) -> ::glam::Vec2 {
    ::glam::Vec2::new(v.x, v.y)
}

fn meta(
    id: u32,
    tag: EntityTag,
    kind: u16,
    nome: Option<&str>,
    skins: u64,
    hp_max: u32,
) -> EntityMeta {
    EntityMeta {
        skins,
        cla: String::new(),
        pk: Default::default(),
        auras: 0,
        id: EntityId(id),
        tag,
        name: nome.map(String::from),
        hp_max,
        faction: None,
        kind,
        nivel: 40,
        desafio: None,
        aparencia: 0,
        poder: 0,
        renascimentos: 0,
    }
}

/// One body aboard: id, deck place, at the wheel, weapon set, swinging.
#[derive(Clone, Copy)]
struct Corpo {
    id: u32,
    local: Vec2,
    leme: bool,
    conjunto: u8,
    golpe: bool,
}

thread_local! {
    /// Each scene's ship has its own id (its own motion and sails).
    static NAVIO_ID: std::cell::Cell<u32> = const { std::cell::Cell::new(500) };
}

fn navio_id() -> u32 {
    NAVIO_ID.with(|c| c.get())
}

/// A world with one ship (`NAVIO`) at `p` heading `yaw` at `vel` u/s, and
/// the bodies on its deck.
fn mundo(item: u16, visual: Visual, p: Vec2, corpos: &[Corpo]) -> World {
    NAVIO_ID.with(|c| c.set(c.get() + 1));
    let navio = navio_id();
    let mut w = World::default();
    let mut metas = vec![meta(
        navio,
        EntityTag::Navio,
        item,
        Some("Brunji"),
        visual.no_fio(),
        nv::casco_max(item) as u32,
    )];
    let mut estados = vec![EntityState::quantize(
        EntityId(navio),
        g(p),
        ::glam::Vec2::ZERO,
        nv::casco_max(item) as i32,
        0,
    )];
    for c in corpos {
        let nome = if c.leme { "Brunji" } else { "Crew" };
        metas.push(meta(c.id, EntityTag::Player, 0, Some(nome), 0, 100));
        let mut st = EntityState::quantize(
            EntityId(c.id),
            g(p),
            ::glam::Vec2::ZERO,
            100,
            if c.id == 1 {
                shared::ent_flags::SELF
            } else {
                0
            },
        );
        st.acao = shared::components::acao::monta(
            c.conjunto,
            true,
            if c.golpe {
                shared::components::acao::GOLPE
            } else {
                0
            },
            0,
        );
        estados.push(st);
    }
    w.apply(metas, estados, &[]);
    let lista: Vec<NoConves> = corpos
        .iter()
        .map(|c| NoConves::novo(EntityId(c.id), EntityId(navio), g(c.local), c.leme))
        .collect();
    w.aplica_conves(&lista);
    for e in w.ents.values_mut() {
        e.sacada = 1.0;
    }
    w
}

/// Moves the ship on (state and drawn heading) and ticks the world.
fn passo(w: &mut World, p: Vec2, yaw: f32, vel: f32, dt: f32) {
    let v = vec2(yaw.sin(), yaw.cos()) * vel;
    let mut st = EntityState::quantize(EntityId(navio_id()), g(p), g(v), 2000, 0);
    st.rumo = shared::components::rumo_de_yaw(yaw);
    w.apply(vec![], vec![st], &[]);
    if let Some(e) = w.ents.get_mut(&EntityId(navio_id())) {
        e.yaw = yaw;
        e.render_pos = p;
    }
    w.tick(dt, &|_, _, _| 0.0);
}

fn camera_em(rt: &RenderTarget, de: Vec3, para: Vec3, fovy: f32, aspect: f32) -> Camera3D {
    Camera3D {
        position: de,
        target: para,
        up: Vec3::Y,
        fovy,
        aspect: Some(aspect),
        render_target: Some(rt.clone()),
        ..Default::default()
    }
}

#[allow(clippy::too_many_arguments)]
fn desenha(
    w: &mut World,
    vox: &crate::vox::VoxCache,
    terreno: &crate::terreno::Terreno,
    solido: &Material,
    cam: &Camera3D,
    centro: Vec2,
    alvo: Option<EntityId>,
) -> (usize, usize) {
    crate::render3d::camera_padrao();
    clear_background(crate::mar_cena::cor_de_fundo());
    crate::mar_cena::desenha_mundo(terreno, solido, cam, &[], Some(centro));
    set_camera(cam);
    gl_use_material(solido);
    solido.set_uniform("Crop", Vec3::ZERO);
    let chao = |_: f32, _: f32| 0.0;
    let mut vista = crate::render3d::Vista::nova(centro, 0.0, 1.0, 0.0, 0.0, &chao);
    vista.cam = Camera3D {
        position: cam.position,
        target: cam.target,
        up: cam.up,
        fovy: cam.fovy,
        aspect: cam.aspect,
        render_target: cam.render_target.clone(),
        ..Default::default()
    };
    crate::gpu_estatica::zera_contagem();
    crate::render3d::draw_entities(w, vox, alvo, &vista);
    let conta = crate::gpu_estatica::contagem();
    gl_use_default_material();
    conta
}

pub async fn previa(vox: &crate::vox::VoxCache) {
    let saida = std::env::var("MMO_PREVIA_SAIDA").unwrap_or_else(|_| "target/convés".into());
    std::fs::create_dir_all(&saida).unwrap();
    crate::mar_ui::define_no_mar(true);
    crate::config_graficos::define_para_previa(crate::config_graficos::RAIO_PADRAO, 100);
    let (wd, ht) = (1280u32, 800u32);
    let aspect = wd as f32 / ht as f32;
    let rt = render_target_ex(
        wd,
        ht,
        RenderTargetParams {
            depth: true,
            sample_count: 1,
        },
    );
    crate::render3d::define_alvo(Some(rt.clone()));
    let solido = crate::render3d::material_solido();
    let mut terreno = crate::terreno::Terreno::novo(&shared::mar::DEF);
    let bosque = shared::mar::vista_da_zona("ilha_inicial").unwrap();
    let mq = |v: ::glam::Vec2| vec2(v.x, v.y);
    let (fb, eb, lb) = (
        mq(bosque.fundeadouro()),
        mq(bosque.eixo()),
        mq(bosque.lado()),
    );
    let aberto = fb + eb * 110.0;
    for _ in 0..120 {
        terreno.atualiza(aberto, crate::config_graficos::RAIO_PADRAO + 1, 400);
    }
    let so = std::env::var("MMO_PREVIA_SO").ok();
    let quer = |nome: &str| so.as_deref().is_none_or(|s| nome.contains(s));
    let yaw = lb.x.atan2(lb.y);
    let frente = vec2(yaw.sin(), yaw.cos());
    let lado = vec2(frente.y, -frente.x);
    let dt = 1.0 / 60.0;
    let grava = |nome: &str| {
        rt.texture
            .get_texture_data()
            .export_png(&format!("{saida}/{nome}.png"))
    };
    let itens = [
        nv::CASCOS[0].item,
        nv::CASCOS[1].item,
        nv::CASCOS[2].item,
        nv::CASCOS[3].item,
    ];
    let visuais = [
        Visual {
            classe: 0,
            vela: 0,
            ..Default::default()
        },
        Visual {
            classe: 1,
            vela: 2,
            figura: 1,
            estrelas: 4,
        },
        Visual {
            classe: 2,
            vela: 3,
            figura: 3,
            estrelas: 9,
        },
        Visual {
            classe: 3,
            vela: 5,
            figura: 2,
            estrelas: 15,
        },
    ];

    // ── 1. Walking on each hull's deck ──
    for classe in 0..4usize {
        let nome = format!("conves-andando-{classe}");
        if !quer(&nome) {
            continue;
        }
        let leme = shared::conves::leme(classe);
        let meio = shared::conves::meia_largura_andavel(classe, 0.0).unwrap_or(0.8);
        let mut corpos = vec![
            Corpo {
                id: 1,
                local: vec2(0.0, -0.6),
                leme: false,
                conjunto: 1,
                golpe: false,
            },
            Corpo {
                id: 2,
                local: vec2(leme.x, leme.y),
                leme: true,
                conjunto: 0,
                golpe: false,
            },
            Corpo {
                id: 3,
                local: vec2(meio * 0.8, 0.8),
                leme: false,
                conjunto: 2,
                golpe: false,
            },
        ];
        if classe >= 2 {
            corpos.push(Corpo {
                id: 4,
                local: vec2(-meio * 0.7, -2.0),
                leme: false,
                conjunto: 3,
                golpe: false,
            });
            corpos.push(Corpo {
                id: 5,
                local: vec2(-meio * 0.3, 3.2),
                leme: false,
                conjunto: 0,
                golpe: false,
            });
        }
        let mut w = mundo(itens[classe], visuais[classe], aberto, &corpos);
        let mut p = aberto;
        let mc = shared::conves::meio_comp(classe);
        let mut conta = (0, 0);
        for k in 0..150 {
            p += frente * 6.0 * dt;
            // The first body walks forward along the deck and back.
            let t = k as f32 * dt;
            let y = -0.6 + (t * 0.9).sin() * mc * 0.35;
            let walker = shared::conves::ponto_livre(classe, ::glam::Vec2::new(0.25, y));
            w.aplica_conves(&[NoConves::novo(
                EntityId(1),
                EntityId(navio_id()),
                walker,
                false,
            )]);
            passo(&mut w, p, yaw, 6.0, dt);
            let alvo3 = vec3(p.x, 1.2, p.y);
            let de = alvo3 - vec3(frente.x, 0.0, frente.y) * mc * 1.3
                + vec3(lado.x, 0.0, lado.y) * mc * 1.1
                + vec3(0.0, mc * 0.95 + 3.0, 0.0);
            let cam = camera_em(&rt, de, alvo3, 0.85, aspect);
            conta = desenha(&mut w, vox, &terreno, &solido, &cam, p, None);
            next_frame().await;
        }
        grava(&nome);
        let n = crate::mar_casco::modelo(visuais[classe]);
        println!(
            "{nome}: frame {} draws / {} indices (entities pass), model {} vertices",
            conta.0, conta.1, n.vertices
        );
    }

    // ── 2. The captain at the helm, through the sailing camera ──
    if quer("conves-leme") {
        let classe = 2;
        let leme = shared::conves::leme(classe);
        let corpos = [
            Corpo {
                id: 1,
                local: vec2(leme.x, leme.y),
                leme: true,
                conjunto: 0,
                golpe: false,
            },
            Corpo {
                id: 2,
                local: vec2(0.9, 1.0),
                leme: false,
                conjunto: 2,
                golpe: false,
            },
        ];
        let mut w = mundo(itens[classe], visuais[classe], aberto, &corpos);
        let mut p = aberto;
        for _ in 0..150 {
            p += frente * 8.0 * dt;
            passo(&mut w, p, yaw, 8.0, dt);
            let q = crate::mar_camera::enquadramento(p, frente, None, classe, false, 1.0, 0.0);
            let mut cam = crate::render3d::camera_no_mar(
                q.centro,
                0.0,
                crate::mar_camera::yaw_atras(frente) + 0.3,
                q.fator,
                q.pitch,
            );
            cam.render_target = Some(rt.clone());
            cam.aspect = Some(aspect);
            desenha(&mut w, vox, &terreno, &solido, &cam, p, None);
            next_frame().await;
        }
        grava("conves-leme");
    }

    // ── 2b. THE HUD, at the helm and on deck, on three screens ──
    if quer("conves-hud") {
        hud_previa(vox, &terreno, &solido, aberto, frente, &saida).await;
        crate::render3d::define_alvo(Some(rt.clone()));
    }

    // ── 3. A party of three at the rail against a surfaced shark ──
    if quer("conves-grupo-tubarao") {
        let classe = 1;
        let hw = shared::conves::meia_largura_andavel(classe, 0.0).unwrap_or(1.2);
        let corpos = [
            Corpo {
                id: 1,
                local: vec2(hw, 0.4),
                leme: false,
                conjunto: 1,
                golpe: true,
            },
            Corpo {
                id: 2,
                local: vec2(hw - 0.2, -1.3),
                leme: false,
                conjunto: 0,
                golpe: true,
            },
            Corpo {
                id: 3,
                local: vec2(hw * 0.1, -0.4),
                leme: false,
                conjunto: 2,
                golpe: true,
            },
        ];
        let mut w = mundo(itens[classe], visuais[classe], aberto, &corpos);
        let tubarao = 600u32;
        let mut vfx = crate::mar_combate_ui::MarCombate::default();
        let bicho = aberto + lado * (shared::conves::meia_boca(classe) + 1.0) + frente * (-0.4);
        w.apply(
            vec![meta(
                tubarao,
                EntityTag::Enemy,
                70,
                Some("Reef Shark"),
                0,
                900,
            )],
            vec![EntityState::quantize(
                EntityId(tubarao),
                g(bicho),
                ::glam::Vec2::ZERO,
                540,
                0,
            )],
            &[],
        );
        for k in 0..120 {
            passo(&mut w, aberto, yaw, 0.0, dt);
            if let Some(e) = w.ents.get_mut(&EntityId(tubarao)) {
                e.yaw = (-lado).x.atan2((-lado).y);
                e.ataque_mob = Some((Some(EntityId(1)), 0.6, nv::MORDIDA_PREPARO_S));
            }
            crate::marinhos::define_emersao(EntityId(tubarao), 1.0);
            // The swings, staggered.
            for (i, id) in [1u32, 2, 3].iter().enumerate() {
                if let Some(e) = w.ents.get_mut(&EntityId(*id)) {
                    let t = ((k as f32 * dt) + i as f32 * 0.21) % 0.7;
                    e.combo = Some(((i % 3) as u8, t.min(crate::rig::DURACAO_DO_GOLPE * 0.9)));
                    e.mira = Some((bicho, 0.5));
                    let d = bicho - e.render_pos;
                    e.yaw = d.x.atan2(d.y);
                }
            }
            let foco = aberto.lerp(bicho, 0.6);
            let alvo3 = vec3(foco.x, 0.9, foco.y);
            let de = alvo3 + vec3(lado.x, 0.0, lado.y) * 6.0 - vec3(frente.x, 0.0, frente.y) * 4.0
                + vec3(0.0, 2.6, 0.0);
            let cam = camera_em(&rt, de, alvo3, 0.85, aspect);
            desenha(
                &mut w,
                vox,
                &terreno,
                &solido,
                &cam,
                aberto,
                Some(EntityId(tubarao)),
            );
            // The sea fight's decals: the fin, the bite's red ring, the target.
            set_camera(&cam);
            vfx.atualiza(&w);
            vfx.desenha_3d(
                &w,
                &cam,
                Some(EntityId(tubarao)),
                crate::mar_combate_ui::Prontos {
                    bordada: false,
                    canhonaco: false,
                    escala: 1.0,
                },
            );
            next_frame().await;
        }
        grava("conves-grupo-tubarao");
    }

    // ── 4. The cloth rippling: a frame sequence of a Galleon's sails ──
    if quer("conves-vela") {
        let classe = 2;
        let corpos = [Corpo {
            id: 1,
            local: vec2(0.0, -1.0),
            leme: false,
            conjunto: 0,
            golpe: false,
        }];
        let mut w = mundo(itens[classe], visuais[classe], aberto, &corpos);
        let mut p = aberto;
        let mut quadro = 0;
        let mut proximo = 0.0f64;
        let inicio = crate::relogio::get_time();
        for _ in 0..600 {
            p += frente * 12.0 * dt;
            passo(&mut w, p, yaw, 12.0, dt);
            let alvo3 = vec3(p.x, 6.5, p.y);
            let de = alvo3
                + vec3(lado.x, 0.0, lado.y) * 15.0
                + vec3(frente.x, 0.0, frente.y) * 6.0
                + vec3(0.0, 1.5, 0.0);
            let cam = camera_em(&rt, de, alvo3, 0.9, aspect);
            desenha(&mut w, vox, &terreno, &solido, &cam, p, None);
            let agora = crate::relogio::get_time() - inicio;
            if agora > 2.0 && agora >= proximo {
                grava(&format!("conves-vela-{quadro}"));
                quadro += 1;
                proximo = agora + 0.12;
                if quadro == 8 {
                    break;
                }
            }
            if quadro == 0 {
                proximo = 2.0;
            }
            next_frame().await;
        }
    }

    // ── 5. Getting under way: the sails unfurl ──
    for classe in [1usize, 2] {
        if !quer("conves-desfralda") {
            break;
        }
        let corpos = [Corpo {
            id: 1,
            local: vec2(0.0, -1.0),
            leme: false,
            conjunto: 0,
            golpe: false,
        }];
        let mut w = mundo(itens[classe], visuais[classe], aberto, &corpos);
        let mut p = aberto;
        let mut quadro = 0;
        let mut t = 0.0f32;
        let mut proximo = 1.2f32;
        for _ in 0..900 {
            // At rest for a second, then the ship gathers way.
            let vel = if t < 1.0 {
                0.0
            } else {
                ((t - 1.0) * 6.0).min(8.0)
            };
            p += frente * vel * dt;
            passo(&mut w, p, yaw, vel, dt);
            let mc = shared::conves::meio_comp(classe);
            let alvo3 = vec3(p.x, 0.5 + mc * 0.55, p.y);
            let de = alvo3 + vec3(lado.x, 0.0, lado.y) * mc * 2.6
                - vec3(frente.x, 0.0, frente.y) * mc * 0.6
                + vec3(0.0, 1.5, 0.0);
            let cam = camera_em(&rt, de, alvo3, 0.9, aspect);
            desenha(&mut w, vox, &terreno, &solido, &cam, p, None);
            if t >= proximo || (quadro == 0 && t >= 0.9) {
                grava(&format!("conves-desfralda-{classe}-{quadro}"));
                quadro += 1;
                proximo = if quadro == 1 { 1.2 } else { proximo + 0.3 };
                if quadro == 8 {
                    break;
                }
            }
            t += dt;
            next_frame().await;
        }
    }
}

/// The sea HUD as `main` draws it since the HUD clean-up of the walkable
/// deck (17/11/2026): at the helm (hull, ship bar, minimap with "Sail
/// to…", the helm toggle, chat, AUTO) and on deck (the land HUD, the helm
/// toggle near the wheel and the route strip), in landscape, a phone in
/// landscape and in portrait: `conves-hud-<leme|conves>-<screen>.png`.
async fn hud_previa(
    vox: &crate::vox::VoxCache,
    terreno: &crate::terreno::Terreno,
    solido: &Material,
    aberto: Vec2,
    frente: Vec2,
    saida: &str,
) {
    crate::hud_layout::define_escala_ui(crate::hud_layout::escala_ui_padrao());
    let mut mapa = crate::mapa::Mapa::para(Some(&shared::mar::DEF));
    for _ in 0..900 {
        mapa.acompanhar();
        if mapa.tem_textura() {
            break;
        }
        next_frame().await;
    }
    let telas: [(&str, u32, u32, [f32; 4]); 3] = [
        ("paisagem", 1280, 800, [0.0; 4]),
        ("celular", 926, 428, [0.0, 47.0, 21.0, 47.0]),
        ("retrato", 428, 926, [47.0, 0.0, 34.0, 0.0]),
    ];
    let yaw = frente.x.atan2(frente.y);
    let classe = 2usize;
    let item = nv::CASCOS[classe].item;
    let visual = Visual {
        classe: 2,
        vela: 3,
        figura: 3,
        estrelas: 9,
    };
    for (tela, wd, ht, margens) in telas {
        let rt = render_target_ex(
            wd,
            ht,
            RenderTargetParams {
                depth: true,
                sample_count: 1,
            },
        );
        crate::render3d::define_alvo(Some(rt.clone()));
        crate::hud_layout::define_margens(margens);
        for leme in [true, false] {
            let local = if leme {
                shared::conves::leme(classe)
            } else {
                shared::conves::roda(classe) - ::glam::Vec2::new(0.9, 0.8)
            };
            let corpos = [Corpo {
                id: 1,
                local: vec2(local.x, local.y),
                leme,
                conjunto: 1,
                golpe: false,
            }];
            let mut w = mundo(item, visual, aberto, &corpos);
            // I am the captain (the ship carries my name).
            if let Some(e) = w.ents.get_mut(&EntityId(1)) {
                e.meta.name = Some("Brunji".into());
            }
            // A party member's ship alongside: the compact board prompt.
            if leme {
                let lado = vec2(frente.y, -frente.x);
                let q = aberto + lado * 11.0 + frente * (40.0 * 8.0 / 60.0);
                w.apply(
                    vec![meta(
                        700,
                        EntityTag::Navio,
                        nv::CASCOS[1].item,
                        Some("Mate"),
                        Visual {
                            classe: 1,
                            vela: 2,
                            ..Default::default()
                        }
                        .no_fio(),
                        3600,
                    )],
                    vec![EntityState::quantize(
                        EntityId(700),
                        g(q),
                        ::glam::Vec2::ZERO,
                        3600,
                        0,
                    )],
                    &[],
                );
            }
            let mut p = aberto;
            for _ in 0..40 {
                p += frente * 8.0 / 60.0;
                passo(&mut w, p, yaw, 8.0, 1.0 / 60.0);
            }
            crate::mar_ui::define_no_leme(leme);
            let mut cam = if leme {
                let q =
                    crate::mar_camera::enquadramento(p, frente, None, classe, ht > wd, 1.0, 0.0);
                crate::render3d::camera_no_mar(
                    q.centro,
                    0.0,
                    crate::mar_camera::yaw_atras(frente) + 0.3,
                    q.fator,
                    q.pitch,
                )
            } else {
                let e = &w.ents[&EntityId(1)];
                let alvo3 = vec3(e.render_pos.x, e.render_y + 1.0, e.render_pos.y);
                let atras = crate::mar_camera::yaw_atras(frente) + 0.5;
                let de = alvo3 + vec3(-atras.sin(), 0.0, atras.cos()) * 9.0 + vec3(0.0, 8.0, 0.0);
                Camera3D {
                    position: de,
                    target: alvo3,
                    up: Vec3::Y,
                    fovy: 0.9,
                    ..Default::default()
                }
            };
            cam.render_target = Some(rt.clone());
            cam.aspect = Some(wd as f32 / ht as f32);
            let mut hab = crate::habilidades::Habilidades::default();
            hab.estado(vec![(nv::id_de_skill(3), 9.0)], 0.0);
            let barra = crate::mar_ui::BarraDoNavio::default();
            let mut ficha = crate::hud::Ficha::default();
            let mut eq = shared::Equipment::default();
            eq.set(
                shared::EquipSlot::Navio,
                Some(item),
                Some(nv::instancia_nova(item)),
            );
            let mut vfx = crate::mar_combate_ui::MarCombate::default();
            let mut rota = crate::mar_rota::RotaDoMar::default();
            for _ in 0..6 {
                crate::mar_casco::sem_surgir(|| {
                    desenha(&mut w, vox, terreno, solido, &cam, p, None)
                });
                crate::render3d::camera_padrao();
                let z = crate::hud_layout::atual();
                crate::chat_ui::ChatUi::default().caixa(z.chat, "");
                crate::hud::draw_hud(
                    &z,
                    &crate::hud::Info {
                        realm: "Tempest".into(),
                        canal: "Canal 1".into(),
                        zona: shared::mar::DEF.zona.into(),
                        jogadores: 42,
                        capacidade: 200,
                    },
                    &crate::hud::Rede {
                        ms: 32.0,
                        ..Default::default()
                    },
                    None,
                    0,
                    0,
                    0,
                    0,
                    Vec2::ZERO,
                    &[],
                );
                mapa.desenha_mini(&w);
                let casco = if leme {
                    crate::mar_ui::casco_de(&eq).map(|(c, m, _)| (c, m))
                } else {
                    None
                };
                crate::hud::draw_ficha(
                    &z,
                    &mut ficha,
                    0.016,
                    "Navegante",
                    70,
                    1840,
                    2400,
                    1000,
                    240,
                    Some(24850),
                    casco,
                );
                crate::hud::draw_modo_pk(&z, Default::default());
                if leme {
                    barra.desenha(&hab);
                } else {
                    hab.barra(
                        shared::skills::Conjunto::Katana,
                        70,
                        240,
                        &Default::default(),
                    );
                    crate::hud::draw_atacar(&z, false);
                    crate::hud::draw_rapidos(
                        &z,
                        [0; 4],
                        [0; 4],
                        [false; 4],
                        [None; 4],
                        [false; 4],
                        None,
                        &|_| String::new(),
                    );
                    crate::hud::draw_sprint(&z, false);
                    crate::hud::draw_pulo(&z, false);
                    crate::hud_layout::desenha_faixa(
                        &z,
                        "Sailing to Glacier · 320 m · 32 s",
                        crate::hud_estilo::AUTO,
                    );
                }
                crate::auto_combate::AutoCombate::default().desenha();
                crate::hud::draw_topo(&z, false, false, false, false, false);
                crate::hud::draw_botao_economia(&z);
                if leme {
                    vfx.hud_do_mar(&z, &eq, Some(p), false, crate::mapa::Mapa::mini_rect());
                    rota.desenha_portos(
                        crate::mapa::Mapa::disco_do_minimapa(),
                        z.s,
                        Some(p),
                        nv::CASCOS[classe].velocidade,
                    );
                }
                crate::mar_ui::botao_do_leme(&z, &w, &["Mate".to_string()], true);
                crate::hud::draw_exp(&z, &ficha);
                unsafe {
                    get_internal_gl().flush();
                }
                next_frame().await;
            }
            let nome = format!("conves-hud-{}-{tela}", if leme { "leme" } else { "conves" });
            rt.texture
                .get_texture_data()
                .export_png(&format!("{saida}/{nome}.png"));
            println!("{nome}");
        }
    }
    crate::mar_ui::define_no_leme(true);
    crate::hud_layout::define_margens([0.0; 4]);
}
