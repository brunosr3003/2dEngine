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
/// `MMO_PREVIA_SAIDA`): each cellar carved into the Arena's real terrain, from
/// high above and from the game's camera, gates shut.
#[cfg(debug_assertions)]
pub async fn plantas(solido: &macroquad::material::Material) {
    let saida =
        std::env::var("MMO_PREVIA_SAIDA").unwrap_or_else(|_| "/tmp/tempest-planta".into());
    std::fs::create_dir_all(&saida).unwrap();
    next_frame().await;
    let rt = macroquad::texture::render_target_ex(
        1280,
        800,
        macroquad::texture::RenderTargetParams { depth: true, sample_count: 1 },
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
            t.tema_da_dungeon(b);
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
                vec3(a.x + 20.0, chao + 95.0, a.y + 70.0),
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
                draw_cube(vec3(eu.x, chao + 0.9, eu.y), vec3(0.7, 1.8, 0.4), None, Color::from_rgba(80, 140, 230, 255));
                crate::agua::desenha(&t, &cam, 0.0);
                macroquad::material::gl_use_default_material();
                crate::render3d::camera_padrao();
                unsafe { macroquad::window::get_internal_gl().flush() };
                rt.texture.get_texture_data().export_png(&format!("{saida}/{nome}.png"));
                next_frame().await;
            }
        }
    }
}
