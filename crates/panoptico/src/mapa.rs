//! Draws the island as an image from above.
//!
//! There is no map stored anywhere: the island is generated here, from the
//! same `shared::terreno` and the same seed the game server uses. If the
//! generator changes, the map changes with it — there is no old version to
//! fall out of sync.
//!
//! Each pixel's color is the surface MATERIAL (the same one the game draws),
//! with two additions that only make sense seen from above:
//!
//! * **slope shading**, taken from the height difference to the neighbour to
//! the northwest. Without it the island becomes a flat green smear: relief
//! from above is not seen by color, it is seen by light.
//! * **contour lines** every eight blocks, discreet. It is what lets the eye
//!   measure "how high" with no legend.

use shared::terreno::{
    material_variado, tom_da_mancha, Bioma, DefIlha, Gerador, Material, BLOCO, NIVEL_DO_MAR,
};
use serde::Serialize;

/// A slice of the same terrain generator the client uses. The 3D camera asks
/// only for the player's neighbourhood; the whole island would be too large per frame.
#[derive(Serialize)]
pub struct Recorte3d {
    pub x: i32,
    pub z: i32,
    pub passo: f32,
    pub lado: usize,
    pub alturas: Vec<f32>,
    pub cores: Vec<[u8; 3]>,
}

pub fn recorte_3d(def: &'static DefIlha, cx: i32, cz: i32) -> Recorte3d {
    const LADO: usize = 129;
    let ger = Gerador::da_ilha(def);
    let x = cx - (LADO / 2) as i32;
    let z = cz - (LADO / 2) as i32;
    let mut alturas = Vec::with_capacity(LADO * LADO);
    let mut cores = Vec::with_capacity(LADO * LADO);
    for iz in 0..LADO {
        for ix in 0..LADO {
            let wx = x + ix as i32;
            let wz = z + iz as i32;
            let bx = (wx as f32 / BLOCO).round() as i32;
            let bz = (wz as f32 / BLOCO).round() as i32;
            let h = ger.bloco_em(bx, bz);
            let altura = (h + 1) as f32 * BLOCO;
            let declive = [(1, 0), (-1, 0), (0, 1), (0, -1)]
                .iter()
                .map(|(dx, dz)| (h - ger.bloco_em(bx + dx, bz + dz)).abs())
                .max()
                .unwrap_or(0);
            let mat = material_variado(
                def.bioma,
                altura,
                declive,
                altura <= NIVEL_DO_MAR,
                ger.mancha(bx, bz),
            );
            // A agua cobre o fundo abaixo do nivel do mar no cliente.
            alturas.push(altura.max(NIVEL_DO_MAR));
            let (r, g, b) = mat.rgb();
            cores.push([r, g, b]);
        }
    }
    Recorte3d { x, z, passo: 1.0, lado: LADO, alturas, cores }
}

#[cfg(test)]
mod testes_3d {
    #[test]
    fn recorte_usa_altura_do_mesmo_gerador() {
        let def = shared::terreno::def_da_zona("ilha_inicial").expect("starting island");
        let r = super::recorte_3d(def, 10, -8);
        assert_eq!(r.lado * r.lado, r.alturas.len());
        assert_eq!(r.alturas.len(), r.cores.len());
        let meio = (r.lado / 2) * r.lado + r.lado / 2;
        let ger = shared::terreno::Gerador::da_ilha(def);
        assert_eq!(r.alturas[meio], ger.altura(10.0, -8.0).max(shared::terreno::NIVEL_DO_MAR));
    }
}

/// How many world columns fit in one pixel.
///
/// Two: the large island is 3200 columns on a side and would come out as a
/// 3200x3200 PNG, which a browser holds but a phone does not. With 2, one
/// pixel is one world unit — which is also the easiest sum to do in the panel.
const COLUNAS_POR_PIXEL: i32 = 2;

pub fn pintar(def: &'static DefIlha) -> anyhow::Result<Vec<u8>> {
    let ger = Gerador::da_ilha(def);
    let lado = (def.raio_blocos * 2 / COLUNAS_POR_PIXEL) as usize;
    let mut px = vec![0u8; lado * lado * 4];

    for iy in 0..lado {
        for ix in 0..lado {
            let bx = ix as i32 * COLUNAS_POR_PIXEL - def.raio_blocos;
            let bz = iy as i32 * COLUNAS_POR_PIXEL - def.raio_blocos;
            let h = ger.bloco_em(bx, bz);
            let altura = (h + 1) as f32 * BLOCO;
            let agua = altura <= NIVEL_DO_MAR;

            let declive = [(1, 0), (-1, 0), (0, 1), (0, -1)]
                .iter()
                .map(|(dx, dz)| (h - ger.bloco_em(bx + dx, bz + dz)).abs())
                .max()
                .unwrap_or(0);
            let mat = material_variado(def.bioma, altura, declive, agua, ger.mancha(bx, bz));
            let (mut r, mut g, mut b) = mat.rgb();

            if agua {
                // A darker background the deeper it gets: the island's outline and the
                // shallow shelf around it become information.
                let fundo = ((NIVEL_DO_MAR - altura) / 8.0).clamp(0.0, 1.0);
                let f = 1.0 - fundo * 0.55;
                r = (r as f32 * f) as u8;
                g = (g as f32 * f) as u8;
                b = (b as f32 * f) as u8;
            } else {
                // Light coming from the northwest. The height difference to the neighbour is
                // the only relief a top-down view can show.
                let nw = ger.bloco_em(bx - COLUNAS_POR_PIXEL, bz - COLUNAS_POR_PIXEL);
                let luz = 1.0 + ((h - nw) as f32 * 0.11).clamp(-0.42, 0.42);
                let tom = tom_da_mancha(ger.mancha(bx, bz));
                let k = luz * tom;
                r = (r as f32 * k).clamp(0.0, 255.0) as u8;
                g = (g as f32 * k).clamp(0.0, 255.0) as u8;
                b = (b as f32 * k).clamp(0.0, 255.0) as u8;
                // Contour line: discreet on purpose. Strong, it becomes a grid and buries
                // the terrain it is supposed to explain.
                if h.rem_euclid(8) == 0 && !matches!(mat, Material::Agua) {
                    r = r.saturating_sub(12);
                    g = g.saturating_sub(12);
                    b = b.saturating_sub(12);
                }
            }
            let i = (iy * lado + ix) * 4;
            px[i] = r;
            px[i + 1] = g;
            px[i + 2] = b;
            px[i + 3] = 255;
        }
    }

    let mut saida = Vec::new();
    {
        let mut enc = png::Encoder::new(&mut saida, lado as u32, lado as u32);
        enc.set_color(png::ColorType::Rgba);
        enc.set_depth(png::BitDepth::Eight);
        enc.set_compression(png::Compression::Fast);
        let mut w = enc.write_header()?;
        w.write_image_data(&px)?;
    }
    let _ = Bioma::Floresta; // o bioma vem do `def`; o import documenta o tipo
    Ok(saida)
}
