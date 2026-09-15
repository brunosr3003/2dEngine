//! Icones dos itens: um por id, ilustrados (SVG rasterizado com
//! anti-aliasing) num atlas gerado por `tools/icones/gerar_icones.py` (ver
//! docs/PIPELINE_ARTE.md).
//!
//! O PNG vai DENTRO do binario (`include_bytes!`), como a fonte da HUD: nao
//! ha' carregamento assincrono no boot nem arquivo que falte em runtime. A
//! textura sobe na primeira vez que um icone e' desenhado — ja' dentro do
//! quadro, com o contexto de GL de pe'.
//!
//! A moldura de RARIDADE nao esta' no atlas: a raridade e' da instancia da
//! peca, nao do id. Quem sabe a raridade passa pra `icone`.

use std::cell::OnceCell;

use macroquad::prelude::*;

#[path = "icones_indice.rs"]
mod indice;

const PNG: &[u8] = include_bytes!("../../../assets/icones/itens.png");

thread_local! {
    static ATLAS: OnceCell<Texture2D> = const { OnceCell::new() };
}

fn atlas() -> Texture2D {
    ATLAS.with(|c| {
        c.get_or_init(|| {
            let t = Texture2D::from_file_with_format(PNG, Some(ImageFormat::Png));
            // Celula grande (96) desenhada menor: filtro linear reduz liso;
            // o nearest da pixel art antiga serrilhava a ilustracao.
            t.set_filter(FilterMode::Linear);
            t
        })
        .clone()
    })
}

/// A celula do item no atlas, em pixels. `None` = item sem icone no atlas.
pub fn celula(item_id: u16) -> Option<Rect> {
    let k = indice::ICONES.binary_search_by_key(&item_id, |e| e.0).ok()?;
    let n = indice::ICONES[k].1 as u32;
    let lado = indice::LADO as f32;
    Some(Rect::new(
        (n % indice::COLUNAS) as f32 * lado,
        (n / indice::COLUNAS) as f32 * lado,
        lado,
        lado,
    ))
}

/// Desenha o icone do atlas centrado e quadrado dentro de `r`. Devolve
/// `false` se o item nao tem icone (quem chamou desenha o substituto).
pub fn desenha(item_id: u16, r: Rect, alfa: f32) -> bool {
    let Some(fonte) = celula(item_id) else { return false };
    let lado = r.w.min(r.h);
    let destino = vec2(lado, lado);
    let x = r.x + (r.w - lado) * 0.5;
    let y = r.y + (r.h - lado) * 0.5;
    draw_texture_ex(
        &atlas(),
        x,
        y,
        Color::new(1.0, 1.0, 1.0, alfa),
        DrawTextureParams { dest_size: Some(destino), source: Some(fonte), ..Default::default() },
    );
    true
}

/// Icone completo: fundo e moldura na cor da raridade (1..5, a mesma de
/// `shared::items::tier_color_hex`), o desenho e a quantidade no canto.
pub fn icone(item_id: u16, r: Rect, raridade: Option<u8>, quantidade: Option<u32>) {
    if let Some(t) = raridade {
        let h = shared::items::tier_color_hex(t).trim_start_matches('#');
        let v = u32::from_str_radix(h, 16).unwrap_or(0xbf_bf_bf);
        let cor = Color::from_rgba((v >> 16) as u8, (v >> 8) as u8, v as u8, 255);
        draw_rectangle(r.x, r.y, r.w, r.h, Color::new(cor.r, cor.g, cor.b, 0.18));
        draw_rectangle_lines(r.x, r.y, r.w, r.h, 1.5, cor);
    }
    let pad = r.w.min(r.h) * 0.06;
    desenha(item_id, Rect::new(r.x + pad, r.y + pad, r.w - pad * 2.0, r.h - pad * 2.0), 1.0);
    if let Some(q) = quantidade.filter(|q| *q > 1) {
        let t = q.to_string();
        let tam = (r.w * 0.26).clamp(10.0, 16.0) as u16;
        let w = crate::hud_estilo::medir_forte(&t, tam);
        crate::hud_estilo::texto_sombra(r.x + r.w - w - 4.0, r.y + r.h - 4.0, &t, tam, WHITE, true);
    }
}

#[cfg(test)]
mod testes {
    use super::*;
    use shared::item_id as i;
    use std::collections::HashSet;

    /// Todo item que o jogo conhece (seed de itens do servidor).
    fn conhecidos() -> Vec<u16> {
        let mut v = vec![
            i::GOLD, i::HEALTH_POTION, i::MANA_POTION, i::GREATER_HEAL, i::GREATER_MANA, i::STAMINA_POTION,
            i::XP_POTION, i::FORTUNA_POTION, i::SORTE_POTION, i::WOOD_T1, i::WOOD_T2, i::WOOD_T3, i::WOOD_T4,
            i::FISH_ANCHOVY, i::FISH_CLOWNFISH, i::FISH_SURGEONFISH, i::FISH_PUFFERFISH,
            i::BOAT_ESQUIFE, i::BOAT_LYLIAN_LEUTARD, i::COPPER, i::DARKSTEEL, i::GLITTERING_POWDER,
        ];
        v.extend(i::ESPADA_E_ESCUDO..=i::CINTO);
        for base in i::MATERIAIS_COLORIDOS {
            for cor in 1..=4 {
                v.push(i::na_cor(base, cor));
            }
        }
        v.extend(i::todas_as_chaves());
        v.extend([i::MARCAS_TEMPESTADE, i::SELO_TEMPESTADE]);
        v.sort_unstable();
        v.dedup();
        v
    }

    fn atlas_cpu() -> Image {
        Image::from_file_with_format(PNG, Some(ImageFormat::Png)).expect("atlas decodifica")
    }

    #[test]
    fn todo_item_conhecido_tem_icone_dentro_do_atlas() {
        let img = atlas_cpu();
        assert_eq!((img.width as u32, img.height as u32), (indice::LARGURA, indice::ALTURA));
        for id in conhecidos() {
            let c = celula(id).unwrap_or_else(|| panic!("item {id} sem icone"));
            assert!(c.x >= 0.0 && c.y >= 0.0, "{id}");
            assert!(c.x + c.w <= img.width as f32 && c.y + c.h <= img.height as f32, "{id} fora do atlas");
        }
        assert!(celula(u16::MAX).is_none());
    }

    #[test]
    fn indice_ordenado_e_sem_celula_repetida() {
        let ids: Vec<u16> = indice::ICONES.iter().map(|e| e.0).collect();
        assert!(ids.windows(2).all(|w| w[0] < w[1]), "indice precisa estar ordenado pra busca binaria");
        let celulas: HashSet<u16> = indice::ICONES.iter().map(|e| e.1).collect();
        assert_eq!(celulas.len(), indice::ICONES.len());
    }

    /// Um icone por item de verdade: nenhum vazio, e nenhum igual a outro.
    #[test]
    fn icones_nao_vazios_e_todos_diferentes() {
        let img = atlas_cpu();
        let lado = indice::LADO as usize;
        let mut vistos = HashSet::new();
        for &(id, _) in indice::ICONES {
            let c = celula(id).unwrap();
            let mut px = Vec::with_capacity(lado * lado * 4);
            let mut opacos = 0;
            for y in 0..lado {
                for x in 0..lado {
                    let i = ((c.y as usize + y) * img.width as usize + c.x as usize + x) * 4;
                    px.extend_from_slice(&img.bytes[i..i + 4]);
                    if img.bytes[i + 3] >= 200 {
                        opacos += 1;
                    }
                }
            }
            assert!(opacos > lado * lado / 10, "icone de {id} quase vazio ({opacos} px)");
            assert!(vistos.insert(px), "icone de {id} igual a outro");
        }
    }
}
