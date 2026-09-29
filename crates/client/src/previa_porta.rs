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

