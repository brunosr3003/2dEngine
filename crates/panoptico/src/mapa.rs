//! Desenha a ilha como imagem de cima.
//!
//! Nao ha' mapa guardado em lugar nenhum: a ilha e' gerada aqui, do mesmo
//! `shared::terreno` e da mesma semente que o servidor de jogo usa. Se o
//! gerador mudar, o mapa muda junto — nao existe versao velha pra desincronizar.
//!
//! A cor de cada pixel e' o MATERIAL da superficie (o mesmo que o jogo
//! desenha), com dois acrescimos que so' fazem sentido visto de cima:
//!
//!   * **sombra de encosta**, tirada da diferenca de altura pro vizinho a
//!     noroeste. Sem ela a ilha vira uma mancha verde chapada: relevo de
//!     cima nao se ve' por cor, se ve' por luz.
//!   * **curva de nivel** a cada oito blocos, discreta. E' o que deixa o olho
//!     medir "quao alto" sem legenda.

use shared::terreno::{
    material_variado, tom_da_mancha, Bioma, DefIlha, Gerador, Material, BLOCO, NIVEL_DO_MAR,
};
use serde::Serialize;

/// Recorte do mesmo gerador de terreno usado pelo cliente. A câmera 3D pede
/// só a vizinhança do jogador; a ilha inteira seria grande demais por quadro.
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

/// Quantas colunas do mundo cabem num pixel.
///
/// Duas: a ilha grande tem 3200 colunas de lado e sairia num PNG de 3200x3200,
/// que o navegador segura mas o celular nao. Com 2, um pixel e' uma unidade de
/// mundo — que e' tambem a conta mais facil de fazer no painel.
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
                // Fundo mais escuro quanto mais fundo: o contorno da ilha e a
                // plataforma rasa em volta dela viram informacao.
                let fundo = ((NIVEL_DO_MAR - altura) / 8.0).clamp(0.0, 1.0);
                let f = 1.0 - fundo * 0.55;
                r = (r as f32 * f) as u8;
                g = (g as f32 * f) as u8;
                b = (b as f32 * f) as u8;
            } else {
                // Luz vinda do noroeste. A diferenca de altura pro vizinho e'
                // o unico relevo que uma vista de cima consegue mostrar.
                let nw = ger.bloco_em(bx - COLUNAS_POR_PIXEL, bz - COLUNAS_POR_PIXEL);
                let luz = 1.0 + ((h - nw) as f32 * 0.11).clamp(-0.42, 0.42);
                let tom = tom_da_mancha(ger.mancha(bx, bz));
                let k = luz * tom;
                r = (r as f32 * k).clamp(0.0, 255.0) as u8;
                g = (g as f32 * k).clamp(0.0, 255.0) as u8;
                b = (b as f32 * k).clamp(0.0, 255.0) as u8;
                // Curva de nivel: discreta de proposito. Forte, ela vira
                // grade e some com o terreno que deveria explicar.
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
