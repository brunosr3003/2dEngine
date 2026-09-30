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
    let saida =
        std::env::var("MMO_PREVIA_SAIDA").unwrap_or_else(|_| "/tmp/tempest-porta".into());
    std::fs::create_dir_all(&saida).unwrap();
    next_frame().await;
    let rt = macroquad::texture::render_target_ex(
        screen_width() as u32,
        screen_height() as u32,
        macroquad::texture::RenderTargetParams { depth: true, sample_count: 1 },
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
            draw_plane(Vec3::ZERO, vec2(14.0, 14.0), None, Color::from_rgba(52, 78, 42, 255));
            macroquad::material::gl_use_material(solido);
            crate::render3d::desenha_porta_do_porao(Vec3::ZERO, perto);
            macroquad::material::gl_use_default_material();
            // Uma caixa do tamanho de gente, pra a porta ter escala.
            draw_cube(vec3(2.4, 0.9, 0.6), vec3(0.6, 1.8, 0.35), None, GRAY);
            crate::render3d::camera_padrao();
            unsafe { macroquad::window::get_internal_gl().flush() };
            rt.texture.get_texture_data().export_png(&format!("{saida}/{nome}.png"));
            next_frame().await;
        }
    }
}


/// Preview of the Porão FLOOR PLANS (`MMO_PREVIA_PLANTA=1`; PNGs in
/// `MMO_PREVIA_SAIDA`): each plan from high above (the whole map) and from the
/// game's camera at the entrance, gates shut; and the first plan again with
/// every gate open, to see the difference.
#[cfg(debug_assertions)]
pub async fn plantas(solido: &macroquad::material::Material) {
    let saida =
        std::env::var("MMO_PREVIA_SAIDA").unwrap_or_else(|_| "/tmp/tempest-planta".into());
    std::fs::create_dir_all(&saida).unwrap();
    next_frame().await;
    let rt = macroquad::texture::render_target_ex(
        screen_width() as u32,
        screen_height() as u32,
        macroquad::texture::RenderTargetParams { depth: true, sample_count: 1 },
    );
    rt.texture.set_filter(FilterMode::Linear);
    crate::render3d::define_alvo(Some(rt.clone()));
    let mut cache = crate::porao_planta::Cache::default();
    let mut quadros: Vec<(String, u16, u8, Camera3D, Vec2)> = Vec::new();
    for p in &shared::planta::PLANTAS {
        let alto = Camera3D {
            position: vec3(0.0, 105.0, 42.0),
            target: vec3(0.0, 0.0, 0.0),
            up: Vec3::Y,
            fovy: 0.9,
            render_target: crate::render3d::alvo(),
            ..Default::default()
        };
        let e = p.centro(p.entrada());
        let s = p.centro(p.sala_da_etapa(0).unwrap());
        let eu = vec2(e.x + (s.x - e.x) * 0.55, e.y + (s.y - e.y) * 0.55);
        // The game's camera: behind the character (south, +z) and above, at
        // about the default tilt.
        let jogo = Camera3D {
            position: vec3(eu.x, 11.0, eu.y + 13.0),
            target: vec3(eu.x, 0.8, eu.y),
            up: Vec3::Y,
            fovy: 0.9,
            render_target: crate::render3d::alvo(),
            ..Default::default()
        };
        quadros.push((format!("planta{}-mapa", p.conteudo), p.conteudo, 0, alto, eu));
        quadros.push((format!("planta{}-jogo", p.conteudo), p.conteudo, 0, jogo, eu));
    }
    if let Some((_, _, _, cam, eu)) = quadros.first() {
        let cam = Camera3D {
            position: cam.position,
            target: cam.target,
            up: cam.up,
            fovy: cam.fovy,
            render_target: crate::render3d::alvo(),
            ..Default::default()
        };
        let eu = *eu;
        quadros.push(("planta1-mapa-aberta".into(), 1, u8::MAX, cam, eu));
    }
    for (nome, conteudo, andar, cam, eu) in quadros {
        let p = shared::planta::da(conteudo).unwrap();
        for _ in 0..2 {
            crate::render3d::camera_padrao();
            clear_background(Color::new(0.06, 0.07, 0.08, 1.0));
            set_camera(&cam);
            draw_plane(Vec3::ZERO, vec2(140.0, 140.0), None, Color::from_rgba(52, 78, 42, 255));
            macroquad::material::gl_use_material(solido);
            cache.de(p).desenha(p, 0.0, andar);
            macroquad::material::gl_use_default_material();
            // A character-sized box where the player would stand, and a
            // Warden-sized one in the middle of the first room, for scale.
            draw_cube(vec3(eu.x, 0.9, eu.y), vec3(0.7, 1.8, 0.4), None, Color::from_rgba(80, 140, 230, 255));
            let s = p.centro(p.sala_da_etapa(0).unwrap());
            draw_cube(vec3(s.x, 1.1, s.y), vec3(1.0, 2.2, 1.0), None, Color::from_rgba(200, 60, 60, 255));
            crate::render3d::camera_padrao();
            unsafe { macroquad::window::get_internal_gl().flush() };
            rt.texture.get_texture_data().export_png(&format!("{saida}/{nome}.png"));
            next_frame().await;
        }
    }
}
