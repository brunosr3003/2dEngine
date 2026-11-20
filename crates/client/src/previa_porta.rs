//! Prévia da PORTA DO PORÃO, fora do `render3d` de propósito.
//!
//! Ela precisa de um render target COM PROFUNDIDADE, e o cliente proíbe isso
//! (`personagens::nenhum_render_target_com_profundidade_no_cliente`): no iOS
//! um alvo com profundidade some da tela. A regra abre exceção pra arquivos de
//! prévia, que só rodam no desktop pra gerar PNG.
//!
//! Por isso a prévia mora aqui e não junto do desenho: pôr `render3d.rs` na
//! lista de exceções abriria a porta pra qualquer alvo com profundidade entrar
//! no arquivo que desenha o mundo inteiro, que é justamente o que a regra
//! existe pra impedir.
use macroquad::prelude::*;

/// Prévia da porta (`MMO_PREVIA_PORTA=1`; PNGs em `MMO_PREVIA_SAIDA`).
///
/// Os dois estados, porque a diferença entre eles é o que diz ao jogador que
/// dá pra abrir.
#[cfg(debug_assertions)]
pub async fn abrir(solido: &macroquad::material::Material) {
    let saida = std::env::var("MMO_PREVIA_SAIDA").unwrap_or_else(|_| "/tmp/tempest-porta".into());
    std::fs::create_dir_all(&saida).unwrap();
    next_frame().await;
    let rt = macroquad::texture::render_target_ex(
        screen_width() as u32,
        screen_height() as u32,
        macroquad::texture::RenderTargetParams {
            depth: true,
            sample_count: 1,
        },
    );
    rt.texture.set_filter(FilterMode::Linear);
    crate::render3d::define_alvo(Some(rt.clone()));
    let cam = Camera3D {
        position: vec3(0.0, 3.4, 6.4),
        target: vec3(0.0, 1.2, 0.0),
        up: Vec3::Y,
        fovy: 0.9,
        render_target: crate::render3d::alvo(),
        ..Default::default()
    };
    for (nome, perto) in [("porta-longe", false), ("porta-perto", true)] {
        for _ in 0..2 {
            crate::render3d::camera_padrao();
            clear_background(Color::new(0.10, 0.13, 0.11, 1.0));
            set_camera(&cam);
            draw_plane(
                Vec3::ZERO,
                vec2(14.0, 14.0),
                None,
                Color::from_rgba(52, 78, 42, 255),
            );
            macroquad::material::gl_use_material(solido);
            crate::render3d::desenha_porta_do_porao(Vec3::ZERO, 5, perto);
            macroquad::material::gl_use_default_material();
            // Uma caixa do tamanho de gente, pra a porta ter escala.
            draw_cube(vec3(2.4, 0.9, 0.6), vec3(0.6, 1.8, 0.35), None, GRAY);
            crate::render3d::camera_padrao();
            unsafe { macroquad::window::get_internal_gl().flush() };
            rt.texture
                .get_texture_data()
                .export_png(&format!("{saida}/{nome}.png"));
            next_frame().await;
        }
    }
}

/// Preview of the Porão FLOOR PLANS (`MMO_PREVIA_PLANTA=1`; PNGs in
/// `MMO_PREVIA_SAIDA`): each cellar carved into the Arena's real terrain, from
/// high above and from the game's camera, gates shut.
#[cfg(debug_assertions)]
pub async fn plantas(solido: &macroquad::material::Material) {
    let saida = std::env::var("MMO_PREVIA_SAIDA").unwrap_or_else(|_| "/tmp/tempest-planta".into());
    std::fs::create_dir_all(&saida).unwrap();
    next_frame().await;
    let rt = macroquad::texture::render_target_ex(
        1280,
        800,
        macroquad::texture::RenderTargetParams {
            depth: true,
            sample_count: 1,
        },
    );
    rt.texture.set_filter(FilterMode::Linear);
    crate::render3d::define_alvo(Some(rt.clone()));
    let mut t = crate::terreno::Terreno::novo(&shared::arena::DEF);
    let mut cache = crate::porao_planta::Cache::default();
    for p in &shared::planta::PLANTAS {
        // The islet re-themes to the Porão's island, as in game.
        if let Some(b) = shared::dungeon::conteudo(p.conteudo)
            .and_then(|c| shared::terreno::def_da_zona(c.zona))
            .map(|d| d.bioma)
        {
            t.tema_da_dungeon(b, Some(p.conteudo));
        }
        let a = vec2(p.ancora.x, p.ancora.y);
        t.atualiza(a, 16, 6000);
        let e = p.centro(p.entrada());
        let s0 = p.centro(p.sala_da_etapa(0).unwrap());
        let eu = vec2(e.x + (s0.x - e.x) * 0.6, e.y + (s0.y - e.y) * 0.6);
        let chao = t.altura(e.x, e.y);
        let quadros = [
            (
                format!("planta{}-mapa", p.conteudo),
                vec3(a.x + 40.0, chao + 140.0, a.y + 125.0),
                vec3(a.x, chao, a.y),
            ),
            (
                format!("planta{}-jogo", p.conteudo),
                vec3(eu.x, chao + 11.0, eu.y + 13.0),
                vec3(eu.x, chao + 0.8, eu.y),
            ),
        ];
        for (nome, olho, alvo) in quadros {
            for _ in 0..3 {
                let cam = Camera3D {
                    position: olho,
                    target: alvo,
                    up: Vec3::Y,
                    fovy: 0.9,
                    aspect: Some(1.6),
                    render_target: crate::render3d::alvo(),
                    ..Default::default()
                };
                crate::render3d::camera_padrao();
                clear_background(Color::from_rgba(150, 186, 214, 255));
                set_camera(&cam);
                macroquad::material::gl_use_material(solido);
                t.desenha(&cam, Vec3::ZERO, 0.0);
                t.desenha_sombras(&cam);
                cache.de(p).desenha(p, chao, 0);
                // Someone for scale, where the player would stand.
                draw_cube(
                    vec3(eu.x, chao + 0.9, eu.y),
                    vec3(0.7, 1.8, 0.4),
                    None,
                    Color::from_rgba(80, 140, 230, 255),
                );
                crate::agua::desenha(&t, &cam, 0.0);
                macroquad::material::gl_use_default_material();
                crate::render3d::camera_padrao();
                unsafe { macroquad::window::get_internal_gl().flush() };
                rt.texture
                    .get_texture_data()
                    .export_png(&format!("{saida}/{nome}.png"));
                next_frame().await;
            }
        }
    }
}

/// Preview of the ISLAND PORTAL (`MMO_PREVIA_PORTAL_ILHAS=1`; PNGs in
/// `MMO_PREVIA_SAIDA`): the portal in the real towns of three islands, a
/// close-up next to a Porão portal for the family resemblance, and the
/// island map with its icon.
#[cfg(debug_assertions)]
pub async fn portal_das_ilhas(solido: &macroquad::material::Material) {
    let saida =
        std::env::var("MMO_PREVIA_SAIDA").unwrap_or_else(|_| "/tmp/tempest-portal-ilhas".into());
    std::fs::create_dir_all(&saida).unwrap();
    next_frame().await;
    let rt = macroquad::texture::render_target_ex(
        1280,
        800,
        macroquad::texture::RenderTargetParams {
            depth: true,
            sample_count: 1,
        },
    );
    rt.texture.set_filter(FilterMode::Linear);
    crate::render3d::define_alvo(Some(rt.clone()));
    // Close-up: the island portal beside a Porão portal.
    let cam = Camera3D {
        position: vec3(0.0, 4.6, 11.0),
        target: vec3(0.0, 2.4, 0.0),
        up: Vec3::Y,
        fovy: 0.9,
        aspect: Some(1.6),
        render_target: crate::render3d::alvo(),
        z_near: 0.4,
        ..Default::default()
    };
    for (nome, perto, lado) in [
        ("portal-ilhas-perto", true, 1.0f32),
        ("portal-ilhas-longe", false, 1.0),
        ("portal-ilhas-tras", false, -1.0),
    ] {
        let cam = Camera3D {
            position: vec3(0.0, 4.6, 11.0 * lado),
            target: vec3(0.0, 2.4, 0.0),
            up: Vec3::Y,
            fovy: 0.9,
            aspect: Some(1.6),
            render_target: crate::render3d::alvo(),
            z_near: 0.4,
            ..Default::default()
        };
        for _ in 0..3 {
            crate::render3d::camera_padrao();
            clear_background(Color::new(0.10, 0.13, 0.11, 1.0));
            set_camera(&cam);
            draw_plane(
                Vec3::ZERO,
                vec2(30.0, 30.0),
                None,
                Color::from_rgba(52, 78, 42, 255),
            );
            macroquad::material::gl_use_material(solido);
            crate::portal_ilhas_ui::desenha(vec3(-3.2, 0.0, 0.0), perto, get_time() as f32 + 2.0);
            crate::render3d::desenha_porta_do_porao(vec3(4.2, 0.0, -1.5), 5, perto);
            draw_cube(vec3(-0.2, 0.9, 1.6), vec3(0.6, 1.8, 0.35), None, GRAY);
            macroquad::material::gl_use_default_material();
            crate::render3d::camera_padrao();
            unsafe { macroquad::window::get_internal_gl().flush() };
            rt.texture
                .get_texture_data()
                .export_png(&format!("{saida}/{nome}.png"));
            next_frame().await;
        }
    }
    // In town, three islands.
    for (i, rotulo) in [(0usize, "bosque"), (4, "skyreach"), (5, "kogen")] {
        let def = &shared::terreno::ARQUIPELAGO[i];
        let Some((p, chegada)) = crate::portal_ilhas_ui::aquecer(def.zona) else {
            continue;
        };
        let g = shared::terreno::Gerador::da_ilha(def);
        let mut t = crate::terreno::Terreno::novo(def);
        let mut casas = crate::construcoes::Construcoes::para(Some(def));
        for _ in 0..600 {
            casas.acompanhar();
            if casas.prontas() {
                break;
            }
            next_frame().await;
        }
        let chao = g.altura(p.x, p.y);
        t.atualiza(p, 16, 4000);
        let frente = (chegada - p).normalize_or_zero();
        let lado = vec2(frente.y, -frente.x);
        for (vista, dist, alto) in [("", 14.0f32, 4.5f32), ("-alto", 60.0, 48.0)] {
            let olho = p + frente * dist * 0.9 + lado * dist * 0.3;
            let cam = Camera3D {
                position: vec3(olho.x, chao + alto, olho.y),
                target: vec3(p.x, chao + 2.2, p.y),
                up: Vec3::Y,
                fovy: 0.9,
                aspect: Some(1.6),
                render_target: Some(rt.clone()),
                z_near: 0.4,
                ..Default::default()
            };
            for _ in 0..3 {
                t.atualiza(p, 16, 4000);
                set_camera(&cam);
                clear_background(Color::from_rgba(150, 186, 214, 255));
                macroquad::material::gl_use_material(solido);
                solido.set_uniform("Some", 0.0f32);
                solido.set_uniform("Oco", Vec3::ZERO);
                crate::portal_ilhas_ui::desenha_virado(
                    vec3(p.x, chao, p.y),
                    crate::portal_ilhas_ui::girado(p, chegada),
                    false,
                    get_time() as f32,
                );
                unsafe { macroquad::window::get_internal_gl().flush() };
                if std::env::var("MMO_PREVIA_SO_PORTAL").is_err() {
                    t.desenha(&cam, Vec3::ZERO, 0.0);
                    t.desenha_sombras(&cam);
                    macroquad::material::gl_use_material(solido);
                    casas.desenha(&cam, None, Vec3::ZERO, 0.0);
                    crate::agua::desenha(&t, &cam, 0.0);
                }
                macroquad::material::gl_use_default_material();
                crate::render3d::camera_padrao();
                let ui = crate::portal_ilhas_ui::PortalUi::default();
                let altura = |x: f32, z: f32| g.altura(x, z);
                ui.desenha_rotulo(def.zona, &cam, &altura, Some(chegada));
                unsafe { macroquad::window::get_internal_gl().flush() };
                rt.texture
                    .get_texture_data()
                    .export_png(&format!("{saida}/portal-{rotulo}{vista}.png"));
                next_frame().await;
            }
        }
        // The banner, standing at the arrival point.
        if i == 0 {
            let mut ui = crate::portal_ilhas_ui::PortalUi::default();
            for _ in 0..3 {
                crate::render3d::camera_padrao();
                clear_background(Color::new(0.08, 0.12, 0.16, 1.0));
                let _ = ui.desenha_tarja(def.zona, Some(chegada), get_time());
                unsafe { macroquad::window::get_internal_gl().flush() };
                rt.texture
                    .get_texture_data()
                    .export_png(&format!("{saida}/portal-tarja.png"));
                next_frame().await;
            }
            // The island map with the portal.
            let mut mapa = crate::mapa::Mapa::para(Some(def));
            mapa.abrir();
            let mundo = crate::world::World::default();
            for _ in 0..150 {
                mapa.acompanhar();
                crate::render3d::camera_padrao();
                clear_background(Color::from_rgba(18, 24, 34, 255));
                mapa.desenha_grande(&mundo, 40, &crate::mundo_ui::Mundo::default(), 0);
                unsafe { macroquad::window::get_internal_gl().flush() };
                next_frame().await;
            }
            rt.texture
                .get_texture_data()
                .export_png(&format!("{saida}/portal-mapa.png"));
            // The travel menu, as the portal opens it.
            let mut menu = crate::viagem_ui::ViagemUi::default();
            let no_ar = |_: &str| true;
            let (passo, _) = shared::viagem::passo_que_libera(4).unwrap();
            menu.abrir(shared::viagem::destinos_do_portal(
                def.zona,
                Some(passo),
                &no_ar,
            ));
            for _ in 0..3 {
                crate::render3d::camera_padrao();
                clear_background(Color::new(0.08, 0.12, 0.16, 1.0));
                let _ = menu.desenha(false);
                unsafe { macroquad::window::get_internal_gl().flush() };
                rt.texture
                    .get_texture_data()
                    .export_png(&format!("{saida}/portal-menu.png"));
                next_frame().await;
            }
        }
    }
    crate::render3d::define_alvo(None);
}

/// Preview of the SHIPYARD (`MMO_PREVIA_ESTALEIRO=1`; PNGs in
/// `MMO_PREVIA_SAIDA`): every tab with the 3D ship, at the size
/// `MMO_PREVIA_TAMANHO=WxH` (default 1600x900; `MMO_CELULAR=1` for the
/// phone layouts), file names ending in `MMO_PREVIA_SUFIXO`.
#[cfg(debug_assertions)]
pub async fn estaleiro(solido: &macroquad::material::Material) {
    use crate::estaleiro_ui::{Aba, Contexto, EstaleiroUi};
    use shared::item_id;
    let saida =
        std::env::var("MMO_PREVIA_SAIDA").unwrap_or_else(|_| "/tmp/tempest-estaleiro".into());
    let sufixo = std::env::var("MMO_PREVIA_SUFIXO").unwrap_or_default();
    std::fs::create_dir_all(&saida).unwrap();
    let (w, h) = std::env::var("MMO_PREVIA_TAMANHO")
        .ok()
        .and_then(|s| {
            s.split_once('x')
                .and_then(|(a, b)| Some((a.parse::<u32>().ok()?, b.parse::<u32>().ok()?)))
        })
        .unwrap_or((1600, 900));
    next_frame().await;
    let rt = macroquad::texture::render_target_ex(
        w,
        h,
        macroquad::texture::RenderTargetParams {
            depth: true,
            sample_count: 1,
        },
    );
    rt.texture.set_filter(FilterMode::Linear);
    crate::render3d::define_alvo(Some(rt.clone()));
    // A Schooner, a bit battered, two hull stars and a sail star, crimson.
    let id = item_id::NAVIO_ESCUNA;
    let mut inst = shared::navio::instancia_nova(id);
    if let Some(d) = inst.navio.as_mut() {
        d.estrelas = [2, 1, 0];
        d.vela = 1;
        d.casco = 1_250;
    }
    let mut equip = shared::Equipment::default();
    equip.set(shared::EquipSlot::Navio, Some(id), Some(inst));
    let mut slots = vec![shared::InventorySlot::default(); shared::INVENTORY_SLOTS];
    slots[2] = shared::InventorySlot {
        item_id: shared::navio::VELAS[4].item,
        qty: 1,
        instance: None,
    };
    slots[3] = shared::InventorySlot {
        item_id: shared::navio::VELAS[2].item,
        qty: 1,
        instance: None,
    };
    slots[5] = shared::InventorySlot {
        item_id: shared::navio::FIGURAS[1].0,
        qty: 1,
        instance: None,
    };
    let conta = |item: u16| -> u64 {
        match item {
            i if i == item_id::COPPER => 6_200,
            i if i == item_id::DARKSTEEL => 900,
            i if i == item_id::PRANCHA_DE_NAUFRAGIO => 12,
            i if i == item_id::NAVAL_TOKEN => 3,
            i if i == item_id::FRAGMENTO_NAVAL => 7,
            i if i == item_id::WOOD_T2 => 120,
            i if i == item_id::WOOD_T3 => 18,
            _ => 30,
        }
    };
    let nome = |item: u16| -> String {
        shared::navio::casco(item)
            .map(|c| c.nome.to_string())
            .or_else(|| shared::navio::nome_do_cosmetico(item).map(str::to_string))
            .unwrap_or_else(|| match item {
                i if i == item_id::COPPER => "Copper".into(),
                i if i == item_id::DARKSTEEL => "Darksteel".into(),
                i if i == item_id::PRANCHA_DE_NAUFRAGIO => "Wreck Plank".into(),
                i if i == item_id::NAVAL_TOKEN => "Naval Token".into(),
                i if i == item_id::FRAGMENTO_NAVAL => "Naval Fragment".into(),
                i if i == item_id::WOOD_T2 => "Oak Wood".into(),
                i if i == item_id::WOOD_T3 => "Pine Wood".into(),
                _ => "Blue Steel".into(),
            })
    };
    let estado = shared::navio::EstadoNavio::de(Some(id), equip.navio_inst.as_ref(), true);
    let fundo = Color::new(0.10, 0.14, 0.16, 1.0);
    for aba in Aba::TODAS {
        let mut ui = EstaleiroUi::default();
        ui.abrir(estado);
        ui.aba = aba;
        for k in 0..4 {
            crate::render3d::camera_padrao();
            clear_background(fundo);
            let ctx = Contexto {
                equip: &equip,
                slots: &slots,
                conta: &conta,
                nome: &nome,
                nivel: 60,
                ouro: 100_000,
                solido,
            };
            let _ = ui.desenha(&ctx);
            crate::render3d::camera_padrao();
            unsafe { macroquad::window::get_internal_gl().flush() };
            if k == 3 {
                rt.texture.get_texture_data().export_png(&format!(
                    "{saida}/estaleiro-{}{sufixo}.png",
                    aba.nome().to_lowercase()
                ));
            }
            next_frame().await;
        }
    }
    // The repair hammer, the upgrade sparks, the build and the flourish.
    for (nome_anima, k) in [("reparo", 0u8), ("estrela", 1), ("obra", 2), ("festa", 3)] {
        let mut ui = EstaleiroUi::default();
        ui.abrir(estado);
        ui.aba = if k == 2 {
            Aba::Construir
        } else if k == 1 {
            Aba::Melhorar
        } else {
            Aba::Reparo
        };
        ui.previa_anima(k);
        for i in 0..20 {
            crate::render3d::camera_padrao();
            clear_background(fundo);
            let ctx = Contexto {
                equip: &equip,
                slots: &slots,
                conta: &conta,
                nome: &nome,
                nivel: 60,
                ouro: 100_000,
                solido,
            };
            let _ = ui.desenha(&ctx);
            crate::render3d::camera_padrao();
            unsafe { macroquad::window::get_internal_gl().flush() };
            if i == 19 {
                rt.texture
                    .get_texture_data()
                    .export_png(&format!("{saida}/estaleiro-anima-{nome_anima}{sufixo}.png"));
            }
            next_frame().await;
        }
    }
    crate::render3d::define_alvo(None);
}
