//! Desktop capture of every island's roster (`docs/BESTIARY.md`).
#![cfg(debug_assertions)]
use macroquad::prelude::*;

use crate::hud_estilo as estilo;

/// `MMO_PREVIA_BESTIARY=1`: every island's common mobs side by side, one row
/// per island, as the bestiary showcase draws them. Desktop capture only (a
/// render target with depth), never part of the app's flow.
pub async fn previa(vox: &mut crate::vox::VoxCache, solido: &Material) {
    // The rosters of `server::economy::kinds_do_bioma`.
    const ILHAS: [(&str, &[u16]); 4] = [
        ("Bosque", &[0, 1, 2, 3, 4, 5, 6]),
        ("Glacier", &[12, 30, 11, 31, 10, 32]),
        ("Waste", &[33, 13, 34, 35, 14]),
        ("Plateau", &[36, 37, 38, 39, 40, 15]),
    ];
    fn nome(kind: u16) -> &'static str {
        if let Some(v) = shared::bestiary::variant(kind) {
            return v.name;
        }
        match kind {
            0 => "Wolf", 1 => "Bear", 2 => "Gunman", 3 => "Tiger", 4 => "Mage",
            5 => "Owlbear", 6 => "Archer", 10 => "Walrus", 11 => "White Bear",
            12 => "White Tiger", 13 => "Scarab", 14 => "Scarab Queen", 15 => "Rockback",
            _ => "?",
        }
    }
    let saida =
        std::env::var("MMO_PREVIA_SAIDA").unwrap_or_else(|_| "/tmp/tempest-bestiary-preview".into());
    std::fs::create_dir_all(&saida).unwrap();
    next_frame().await;
    let rt = macroquad::texture::render_target_ex(
        screen_width() as u32,
        screen_height() as u32,
        macroquad::texture::RenderTargetParams { depth: true, sample_count: 1 },
    );
    rt.texture.set_filter(FilterMode::Linear);
    crate::render3d::define_alvo(Some(rt.clone()));
    for quadro in 0..40 {
        while vox.atende_um_pendente(crate::render3d::VOXEL).await {}
        crate::render3d::camera_padrao();
        clear_background(Color::new(0.10, 0.13, 0.17, 1.0));
        let (w, h) = (screen_width(), screen_height());
        let linha = h / ILHAS.len() as f32;
        let cel = (w - 120.0) / 7.0;
        for (i, (ilha, kinds)) in ILHAS.iter().enumerate() {
            let y = i as f32 * linha;
            estilo::texto(10.0, y + linha * 0.5, ilha, 18, WHITE);
            for (j, kind) in kinds.iter().enumerate() {
                let r = Rect::new(120.0 + j as f32 * cel, y + 4.0, cel - 8.0, linha - 28.0);
                crate::render3d::vitrine_mob(vox, *kind, false, r, 0.6, solido);
                crate::render3d::camera_padrao();
                let cor = if shared::bestiary::variant(*kind).is_some() { YELLOW } else { WHITE };
                estilo::texto(r.x, y + linha - 8.0, nome(*kind), 14, cor);
            }
        }
        unsafe { macroquad::window::get_internal_gl().flush() };
        if quadro == 39 {
            rt.texture.get_texture_data().export_png(&format!("{saida}/bestiary.png"));
        }
        next_frame().await;
    }
}
