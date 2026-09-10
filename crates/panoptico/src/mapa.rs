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

/// Quantas colunas do mundo cabem num pixel.
///
/// Duas: a ilha grande tem 3200 colunas de lado e sairia num PNG de 3200x3200,
/// que o navegador segura mas o celular nao. Com 2, um pixel e' uma unidade de
/// mundo — que e' tambem a conta mais facil de fazer no painel.
const COLUNAS_POR_PIXEL: i32 = 2;

pub fn pintar(def: &'static DefIlha) -> anyhow::Result<Vec<u8>> {
    let ger = Gerador::novo(
        def.semente,
        def.raio_blocos,
        def.bioma,
        shared::terreno::ESCALA_ALTURA,
    );
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
