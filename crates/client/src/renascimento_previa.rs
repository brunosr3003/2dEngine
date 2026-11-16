//! `MMO_PREVIA_AURA=1`: the rebirth auras, one shot per tier (and the black
//! hole at full intensity), a row of all of them at the game's distance, and
//! the rebirth burst of each tier at three moments. PNGs in
//! `MMO_PREVIA_SAIDA` (default `target/aura_shots`). Desktop only, drawn
//! straight on the window and read back, never part of the app's flow.
#![cfg(all(debug_assertions, not(any(target_os = "ios", target_os = "android"))))]

use macroquad::material::{gl_use_default_material, gl_use_material, Material};
use macroquad::prelude::*;

use crate::render3d::VOXEL;

const CASOS: [(u8, &str); 6] = [(1, "t1-green"), (3, "t2-blue"), (5, "t3-purple"), (10, "t4-gold"), (15, "t5-blackhole"), (55, "t5-blackhole-x3")];

pub async fn previa(vox: &mut crate::vox::VoxCache) {
    let saida = std::env::var("MMO_PREVIA_SAIDA").unwrap_or_else(|_| "target/aura_shots".into());
    std::fs::create_dir_all(&saida).unwrap();
    let solido = crate::render3d::material_solido();
    let veste_padrao = shared::aparencia::Aparencia { rosto: 1, cabelo: 1, cor_cabelo: 1, pele: 1, ..Default::default() };
    // Load the body parts first.
    for _ in 0..30 {
        while vox.atende_um_pendente(VOXEL).await {}
        clear_background(BLACK);
        next_frame().await;
    }
    let chao = Color::new(0.24, 0.33, 0.20, 1.0);
    // 1. One body per tier, close.
    for (r, nome) in CASOS {
        let cam = Camera3D { position: vec3(0.0, 3.6, 5.0), target: vec3(0.0, 0.85, 0.0), fovy: 40f32.to_radians(), ..Default::default() };
        cena(vox, &solido, veste_padrao, &cam, chao, &[(Vec3::ZERO, r)], None);
        captura(&format!("{saida}/aura-{nome}.png"));
        next_frame().await;
    }
    // 2. All of them at the game's distance, on grass and in a dark cave.
    for (fundo, nome) in [(chao, "todas-grama"), (Color::new(0.09, 0.08, 0.10, 1.0), "todas-escuro")] {
        let cam = Camera3D { position: vec3(0.0, 10.5, 9.0), target: vec3(0.0, 0.6, 0.0), fovy: 40f32.to_radians(), ..Default::default() };
        let corpos: Vec<(Vec3, u8)> = CASOS.iter().enumerate().map(|(i, (r, _))| (vec3(-6.25 + i as f32 * 2.5, 0.0, 0.0), *r)).collect();
        cena(vox, &solido, veste_padrao, &cam, fundo, &corpos, None);
        captura(&format!("{saida}/aura-{nome}.png"));
        next_frame().await;
    }
    // 3. The burst at three moments, per tier.
    for (r, nome) in CASOS {
        for f in [0.06f32, 0.16, 0.40] {
            let mut e = crate::renascimento_vfx::Renasceu::default();
            e.dispara(r as u32);
            e.passo(f * crate::renascimento_vfx::DURACAO);
            let cam = Camera3D { position: vec3(0.0, 7.5, 10.5), target: vec3(0.0, 1.6, 0.0), fovy: 45f32.to_radians(), ..Default::default() };
            cena(vox, &solido, veste_padrao, &cam, chao, &[(Vec3::ZERO, r)], Some(&e));
            captura(&format!("{saida}/burst-{nome}-{:02}.png", (f * 100.0) as u32));
            next_frame().await;
        }
    }
}

fn cena(
    vox: &crate::vox::VoxCache,
    solido: &Material,
    aparencia: shared::aparencia::Aparencia,
    cam: &Camera3D,
    chao: Color,
    corpos: &[(Vec3, u8)],
    festa: Option<&crate::renascimento_vfx::Renasceu>,
) {
    clear_background(Color::new(0.05, 0.06, 0.08, 1.0));
    set_camera(cam);
    draw_plane(vec3(0.0, 0.0, 0.0), vec2(40.0, 40.0), None, chao);
    // A few darker tiles so the ground reads like ground.
    for i in -8..8 {
        for j in -8..8 {
            if (i * 7 + j * 13i32).rem_euclid(5) == 0 {
                let c = Color::new(chao.r * 0.85, chao.g * 0.85, chao.b * 0.85, 1.0);
                draw_plane(vec3(i as f32 + 0.5, 0.001, j as f32 + 0.5), vec2(0.5, 0.5), None, c);
            }
        }
    }
    let Some(mut veste) = crate::render3d::vestimenta_de(vox, aparencia.empacota()) else { return };
    veste.skins = aparencia.empacota_skins();
    gl_use_material(solido);
    solido.set_uniform("Crop", Vec3::ZERO);
    let pose = crate::rig::pose(&crate::rig::Entrada {
        fase: 0.0,
        andar: 0.0,
        correr: 0.0,
        tempo: get_time() as f32,
        ar: 0.0,
        degrau: [0.0, 0.0],
        combate: crate::rig::Combate::default(),
    });
    for (i, (p, r)) in corpos.iter().enumerate() {
        let base = Mat4::from_translation(*p) * Mat4::from_rotation_y(0.35);
        let (mats, _) = crate::render3d::desenha_rig(base, &pose, &veste, vox, None);
        // As the world measures it: head + 0.35.
        let cabeca = mats[1].transform_point3(Vec3::ZERO);
        crate::renascimento_vfx::personagem(*r, *p, cabeca.y - p.y + 0.35, 0.0, i as u32 * 7 + 3);
    }
    gl_use_default_material();
    crate::renascimento_vfx::desenha(cam.position);
    if let Some(e) = festa {
        e.desenha(corpos[0].0, cam.position);
    }
    set_default_camera();
}

fn captura(caminho: &str) {
    unsafe { get_internal_gl().flush() };
    let mut img = get_screen_data();
    // The back buffer reads bottom-up.
    let (w, h) = (img.width as usize, img.height as usize);
    let linha = w * 4;
    for y in 0..h / 2 {
        let (a, b) = (y * linha, (h - 1 - y) * linha);
        for x in 0..linha {
            img.bytes.swap(a + x, b + x);
        }
    }
    for px in img.bytes.chunks_mut(4) {
        px[3] = 255;
    }
    img.export_png(caminho);
}
