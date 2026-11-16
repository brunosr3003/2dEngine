//! THE OPEN SEA on screen (docs/NAVEGACAO.md, naval phase 1).
//!
//! Navigating is walking, only on water: the server moves the hull with the
//! same controls as the body. What changes here is what is DRAWN — a hull
//! instead of the body for every player at sea — plus the hull bar and the
//! "Dock" prompt at an anchorage.

use macroquad::prelude::*;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::hud_estilo as estilo;

static NO_MAR: AtomicBool = AtomicBool::new(false);

/// The zone just changed: is it the open sea?
pub fn define_no_mar(sim: bool) {
    NO_MAR.store(sim, Ordering::Relaxed);
}

pub fn no_mar() -> bool {
    NO_MAR.load(Ordering::Relaxed)
}

/// THE HULL, in cubes (a simple voxel hull): three blocks narrowing to the
/// bow so it has a FRONT, a gunwale, a mast and a sail. The game's models
/// face +Z and the yaw is `x.atan2(z)`: the front is (sin, cos).
pub fn desenha_casco(p: Vec3, yaw: f32, semente: u64) {
    let t = get_time() as f32 + (semente % 97) as f32 * 0.37;
    // A gentle bob and roll on the swell.
    // Riding ON the swell: the Gerstner surface rises a few tenths above
    // the sea level, and a hull at the level sank into it.
    let p = p + vec3(0.0, crate::agua::ALTURA_DA_AGUA + crate::agua::AMPLITUDE_MAX * 0.6 + (t * 1.7).sin() * 0.06, 0.0);
    let madeira = Color::from_rgba(112, 72, 38, 255);
    let escura = Color::from_rgba(78, 48, 24, 255);
    let pano = Color::from_rgba(232, 224, 202, 255);
    let (fx, fz) = (yaw.sin(), yaw.cos());
    let em = |ao_longo: f32, ao_lado: f32, y: f32| {
        p + vec3(ao_longo * fx + ao_lado * fz, y, ao_longo * fz - ao_lado * fx)
    };
    // The hull, three pieces narrowing to the bow, along the heading.
    for (ao_longo, comp, larg) in [(-1.1f32, 1.6f32, 2.0f32), (0.5, 1.4, 1.6), (1.5, 0.9, 0.9)] {
        caixa(em(ao_longo, 0.0, 0.25), comp, 0.6, larg, fx, fz, madeira);
    }
    for lado in [-1.0f32, 1.0] {
        caixa(em(-0.3, lado * 0.92, 0.62), 2.8, 0.22, 0.14, fx, fz, escura);
    }
    caixa(em(-0.2, 0.0, 1.5), 0.16, 2.2, 0.16, fx, fz, escura);
    let inflar = 0.08 * (t * 0.9).sin();
    caixa(em(-0.2 + inflar, 0.0, 1.85), 0.1, 1.3, 1.7, fx, fz, pano);
}

/// A box of `comp` along the heading, `larg` across it. macroquad's cubes
/// are axis-aligned, so a turned box is drawn as a row of small cubes along
/// the heading — enough for a ship that is a few units long.
fn caixa(c: Vec3, comp: f32, alto: f32, larg: f32, fx: f32, fz: f32, cor: Color) {
    let passo = 0.22f32;
    let n = (comp / passo).ceil().max(1.0) as i32;
    let lado = (larg / passo).ceil().max(1.0) as i32;
    for i in 0..n {
        let a = -comp * 0.5 + (i as f32 + 0.5) * comp / n as f32;
        for j in 0..lado {
            let b = -larg * 0.5 + (j as f32 + 0.5) * larg / lado as f32;
            let q = c + vec3(a * fx + b * fz, 0.0, a * fz - b * fx);
            draw_cube(q, vec3(passo * 1.05, alto, passo * 1.05), None, cor);
        }
    }
}

/// The ship's hull from the equipped instance: (points, max, damaged).
pub fn casco_de(eq: &shared::Equipment) -> Option<(u16, u16, bool)> {
    let id = eq.navio.filter(|i| shared::item_id::e_navio(*i))?;
    let d = shared::navio::dados(eq.navio_inst.as_ref(), id);
    Some((d.casco, shared::navio::casco_max(id), d.avariado))
}

/// The HULL BAR, top centre, only at sea.
pub fn barra_do_casco(z: &crate::hud_layout::Zonas, eq: &shared::Equipment) {
    if !no_mar() {
        return;
    }
    let Some((casco, max, _)) = casco_de(eq) else {
        return;
    };
    let w = 260.0 * z.s;
    let h = 22.0 * z.s;
    let x = screen_width() * 0.5 - w * 0.5;
    let y = z.alvo.y + z.alvo.h + 10.0 * z.s;
    let frac = if max == 0 { 0.0 } else { casco as f32 / max as f32 };
    draw_rectangle(x, y, w, h, Color::new(0.05, 0.08, 0.12, 0.85));
    let cor = if frac <= shared::navio::CRITICO {
        Color::new(0.85, 0.25, 0.2, 1.0)
    } else {
        Color::new(0.75, 0.55, 0.28, 1.0)
    };
    draw_rectangle(x + 2.0, y + 2.0, (w - 4.0) * frac, h - 4.0, cor);
    draw_rectangle_lines(x, y, w, h, 2.0, Color::new(0.9, 0.78, 0.5, 0.9));
    estilo::texto_centro(
        x + w * 0.5,
        y + h * 0.5 + 6.0 * z.s,
        &format!("Hull {casco} / {max}"),
        14,
        estilo::TEXTO,
    );
}

/// THE DOCK PROMPT: at an anchorage, a "Dock at <island>" button. Returns
/// the island (`ARQUIPELAGO` index) when tapped.
pub fn botao_atracar(z: &crate::hud_layout::Zonas, eu: Option<(f32, f32)>) -> Option<u8> {
    if !no_mar() {
        return None;
    }
    let (x, zz) = eu?;
    let cais = shared::mar::cais_perto(::glam::Vec2::new(x, zz))?;
    let rotulo = format!("Dock at {}", cais.nome);
    let h = z.faixa.h * 1.5;
    let w = estilo::medir_forte(&rotulo, 18) + h * 1.4;
    let r = Rect::new(z.faixa.center().x - w * 0.5, z.faixa.y - 8.0 * z.s - h * 2.3, w, h);
    estilo::botao(r, &rotulo, estilo::estado_de(r, false, false), true);
    (crate::foco::clique() && r.contains(Vec2::from(mouse_position()))).then_some(cais.ilha as u8)
}

/// PREVIEW (`MMO_PREVIA_MAR=1`): renders the open sea off the Bosque's dock
/// and from high above, with a few hulls, to PNGs in `MMO_PREVIA_SAIDA`.
#[cfg(debug_assertions)]
pub async fn previa() {
    let saida = std::env::var("MMO_PREVIA_SAIDA").unwrap_or_else(|_| "/tmp/tempest-mar".into());
    std::fs::create_dir_all(&saida).unwrap();
    define_no_mar(true);
    let rt = render_target_ex(1280, 800, RenderTargetParams { depth: true, sample_count: 1 });
    crate::render3d::define_alvo(Some(rt.clone()));
    crate::gpu_estatica::define_neblina(Vec2::ZERO, 0., 0.);
    let solido = crate::render3d::material_solido();
    let mut terreno = crate::terreno::Terreno::novo(&shared::mar::DEF);
    let bosque = shared::mar::vista_da_zona("ilha_inicial").unwrap();
    let f = bosque.fundeadouro();
    let rumo = (bosque.ponta - bosque.raiz).normalize_or_zero();
    let ancora = vec2(f.x, f.y);
    for (nome, alvo, recuo, alto, raio) in [
        ("cais", ancora, 30.0f32, 16.0f32, 12),
        ("rota", ancora + vec2(rumo.x, rumo.y) * 90.0, 40.0, 22.0, 16),
        ("ilha", ancora + vec2(rumo.x, rumo.y) * 60.0, 70.0, 45.0, 24),
        ("alto", vec2(0.0, 0.0), 420.0, 520.0, 40),
    ] {
        for _ in 0..60 {
            terreno.atualiza(alvo, raio, 200_000);
        }
        let costas = if nome == "ilha" { vec2(rumo.x, rumo.y) } else { vec2(-rumo.x, -rumo.y) };
        let cam = Camera3D {
            position: vec3(alvo.x + costas.x * recuo * 0.6 + 6.0, alto, alvo.y + costas.y * recuo * 0.6 + recuo * 0.4),
            target: vec3(alvo.x, 0.0, alvo.y),
            up: Vec3::Y,
            fovy: 0.9,
            aspect: Some(1.6),
            render_target: Some(rt.clone()),
            ..Default::default()
        };
        for _ in 0..3 {
            crate::render3d::camera_padrao();
            clear_background(Color::new(0.55, 0.72, 0.86, 1.0));
            set_camera(&cam);
            macroquad::material::gl_use_material(&solido);
            terreno.desenha(&cam, Vec3::ZERO, 0.);
            for (k, d) in [(0.0f32, 0.0f32), (8.0, 5.0), (-6.0, 9.0)].iter().enumerate() {
                let p = vec3(alvo.x + d.0, 0.0, alvo.y + d.1);
                desenha_casco(p, rumo.x.atan2(rumo.y) + k as f32 * 0.6, k as u64);
            }
            macroquad::material::gl_use_default_material();
            crate::agua::desenha(&terreno, &cam, get_time() as f32);
            macroquad::material::gl_use_default_material();
            crate::render3d::camera_padrao();
            unsafe {
                get_internal_gl().flush();
            }
            rt.texture.get_texture_data().export_png(&format!("{saida}/mar-{nome}.png"));
            next_frame().await;
        }
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn o_casco_sai_do_equipamento() {
        let mut eq = shared::Equipment::default();
        assert!(casco_de(&eq).is_none());
        let id = shared::item_id::NAVIO_CHALUPA;
        eq.set(shared::EquipSlot::Navio, Some(id), Some(shared::navio::instancia_nova(id)));
        assert_eq!(casco_de(&eq), Some((2400, 2400, false)));
    }
}
