//! Icones do HUD, das skills e do mapa: arte vetorial propria rasterizada por
//! `tools/icones/gerar_icones_ui.py` em tres atlas (ver docs/PIPELINE_ARTE.md).
//!
//! HUD e mapa sao silhuetas BRANCAS com contorno escuro: quem desenha passa a
//! cor do estado (normal, sobre, ativo, travado) e o branco vira essa cor. As
//! skills ja' vem coloridas no disco da arma; so' o alfa muda.
//!
//! Filtro LINEAR (nao `Nearest` como os itens de pixel art): o desenho e'
//! vetor, e reduzido pro tamanho do botao ele tem que continuar liso.

use std::cell::OnceCell;

use macroquad::prelude::*;

#[path = "icones_ui_indice.rs"]
mod indice;

const PNG_UI: &[u8] = include_bytes!("../../../assets/icones/hud.png");
const PNG_MAPA: &[u8] = include_bytes!("../../../assets/icones/mapa.png");
const PNG_SKILLS: &[u8] = include_bytes!("../../../assets/icones/skills.png");

/// Todo nome de icone do HUD/menu que o codigo pede. O teste confere que cada
/// um existe no atlas: renomear no gerador sem mexer aqui quebra o teste, nao
/// a tela.
pub const USADOS_UI: &[&str] = &[
    "bolsa", "missoes", "todas_missoes", "diarias", "grupo", "amigos", "avisos", "menu", "engrenagem",
    "configuracoes", "cadeado", "atacar", "auto_combate", "auto_coleta", "coleta", "mais", "voltar",
    "fechar", "coroa", "ficha", "habilidades", "montaria", "recuperar_xp", "conquistas", "craft",
    "forja", "encantar", "mapa", "aventuras", "correio", "clan", "lojas", "mercado", "loja_tp",
    "barra_itens", "trocar_personagem", "sair",
];

/// Todo nome de marcador do mapa que o codigo pede.
pub const USADOS_MAPA: &[&str] = &[
    "jogador", "destino", "cidade", "porto", "chefe", "npc", "pedra", "madeira", "lobo", "urso",
    "tigre", "owlbear", "pistoleiro", "mago", "arqueiro", "caranguejo",
];

thread_local! {
    static TEX: OnceCell<[Texture2D; 3]> = const { OnceCell::new() };
}

fn carrega(png: &[u8]) -> Texture2D {
    let t = Texture2D::from_file_with_format(png, Some(ImageFormat::Png));
    t.set_filter(FilterMode::Linear);
    t
}

fn textura(i: usize) -> Texture2D {
    TEX.with(|c| c.get_or_init(|| [carrega(PNG_UI), carrega(PNG_MAPA), carrega(PNG_SKILLS)])[i].clone())
}

fn celula(n: u16, lado: u32, colunas: u32) -> Rect {
    let n = n as u32;
    Rect::new((n % colunas * lado) as f32, (n / colunas * lado) as f32, lado as f32, lado as f32)
}

/// Celula do icone de HUD/menu no atlas, em pixels.
pub fn celula_ui(nome: &str) -> Option<Rect> {
    let k = indice::UI.binary_search_by(|e| e.0.cmp(nome)).ok()?;
    Some(celula(indice::UI[k].1, indice::LADO_UI, indice::COLUNAS_UI))
}

/// Celula do marcador de mapa no atlas, em pixels.
pub fn celula_mapa(nome: &str) -> Option<Rect> {
    let k = indice::MAPA.binary_search_by(|e| e.0.cmp(nome)).ok()?;
    Some(celula(indice::MAPA[k].1, indice::LADO_MAPA, indice::COLUNAS_MAPA))
}

/// Celula do icone da skill no atlas, em pixels.
pub fn celula_skill(id: u32) -> Option<Rect> {
    let k = indice::SKILLS.binary_search_by_key(&id, |e| e.0).ok()?;
    Some(celula(indice::SKILLS[k].1, indice::LADO_SKILLS, indice::COLUNAS_SKILLS))
}

fn desenha(tex: usize, fonte: Rect, c: Vec2, lado: f32, cor: Color, rotacao: f32) {
    draw_texture_ex(
        &textura(tex),
        c.x - lado * 0.5,
        c.y - lado * 0.5,
        cor,
        DrawTextureParams {
            dest_size: Some(vec2(lado, lado)),
            source: Some(fonte),
            rotation: rotacao,
            pivot: Some(c),
            ..Default::default()
        },
    );
}

/// Icone de HUD/menu centrado em `c`, quadrado de `lado`, tingido por `cor`.
/// `false` = nome sem icone (quem chamou desenha o substituto).
pub fn ui(nome: &str, c: Vec2, lado: f32, cor: Color) -> bool {
    let Some(f) = celula_ui(nome) else { return false };
    desenha(0, f, c, lado, cor, 0.0);
    true
}

/// Marcador de mapa centrado em `c`, tingido por `cor`, girado `rotacao`
/// radianos (a seta do jogador).
pub fn mapa(nome: &str, c: Vec2, lado: f32, cor: Color, rotacao: f32) -> bool {
    let Some(f) = celula_mapa(nome) else { return false };
    desenha(1, f, c, lado, cor, rotacao);
    true
}

/// Icone colorido da skill, com `alfa` (0,4 = indisponivel).
pub fn skill(id: u32, c: Vec2, lado: f32, alfa: f32) -> bool {
    let Some(f) = celula_skill(id) else { return false };
    desenha(2, f, c, lado, Color::new(1.0, 1.0, 1.0, alfa), 0.0);
    true
}

/// O marcador do mapa pra cada bicho da tabela (`economy::KINDS_INICIAIS`).
pub fn nome_do_bicho(kind: u16) -> &'static str {
    match kind {
        0 | 7 => "lobo",
        1 => "urso",
        2 => "pistoleiro",
        3 => "tigre",
        4 => "mago",
        5 => "owlbear",
        6 => "arqueiro",
        8 | 9 => "caranguejo",
        _ => "lobo",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn todo_nome_usado_existe_no_atlas() {
        for n in USADOS_UI {
            assert!(celula_ui(n).is_some(), "icone de HUD '{n}' nao existe no atlas");
        }
        for n in USADOS_MAPA {
            assert!(celula_mapa(n).is_some(), "marcador '{n}' nao existe no atlas");
        }
        for id in 1..=12 {
            assert!(celula_skill(id).is_some(), "skill {id} sem icone");
        }
        for k in 0..10 {
            assert!(celula_mapa(nome_do_bicho(k)).is_some(), "bicho {k} sem marcador");
        }
    }

    #[test]
    fn indices_ordenados_e_celulas_dentro_do_atlas() {
        assert!(indice::UI.windows(2).all(|w| w[0].0 < w[1].0));
        assert!(indice::MAPA.windows(2).all(|w| w[0].0 < w[1].0));
        assert!(indice::SKILLS.windows(2).all(|w| w[0].0 < w[1].0));
        let dentro = |r: Rect, w: u32, h: u32| r.x >= 0.0 && r.y >= 0.0 && r.x + r.w <= w as f32 && r.y + r.h <= h as f32;
        for e in indice::UI {
            assert!(dentro(celula(e.1, indice::LADO_UI, indice::COLUNAS_UI), indice::LARGURA_UI, indice::ALTURA_UI));
        }
        for e in indice::MAPA {
            assert!(dentro(celula(e.1, indice::LADO_MAPA, indice::COLUNAS_MAPA), indice::LARGURA_MAPA, indice::ALTURA_MAPA));
        }
        for e in indice::SKILLS {
            assert!(dentro(celula(e.1, indice::LADO_SKILLS, indice::COLUNAS_SKILLS), indice::LARGURA_SKILLS, indice::ALTURA_SKILLS));
        }
    }

    /// Decodifica o atlas (sem GL) e confere: cada celula tem desenho e
    /// nenhuma e' igual a outra.
    fn cheias_e_distintas(png: &[u8], celulas: &[u16], lado: u32, colunas: u32, largura: u32) {
        let img = Image::from_file_with_format(png, Some(ImageFormat::Png)).expect("atlas decodifica");
        assert_eq!(img.width as u32, largura);
        let mut vistos = HashSet::new();
        for &n in celulas {
            let r = celula(n, lado, colunas);
            let mut opacos = 0usize;
            let mut bytes = Vec::with_capacity((lado * lado * 4) as usize);
            for y in r.y as u32..(r.y + r.h) as u32 {
                for x in r.x as u32..(r.x + r.w) as u32 {
                    let i = ((y * img.width as u32 + x) * 4) as usize;
                    let px = &img.bytes[i..i + 4];
                    if px[3] > 40 {
                        opacos += 1;
                    }
                    bytes.extend_from_slice(px);
                }
            }
            assert!(opacos as u32 > lado * lado / 20, "celula {n} quase vazia ({opacos} px)");
            assert!(vistos.insert(bytes), "celula {n} igual a outra");
        }
    }

    #[test]
    fn icones_tem_desenho_e_sao_distintos() {
        let ui: Vec<u16> = indice::UI.iter().map(|e| e.1).collect();
        cheias_e_distintas(PNG_UI, &ui, indice::LADO_UI, indice::COLUNAS_UI, indice::LARGURA_UI);
        let mapa: Vec<u16> = indice::MAPA.iter().map(|e| e.1).collect();
        cheias_e_distintas(PNG_MAPA, &mapa, indice::LADO_MAPA, indice::COLUNAS_MAPA, indice::LARGURA_MAPA);
        let sk: Vec<u16> = indice::SKILLS.iter().map(|e| e.1).collect();
        cheias_e_distintas(PNG_SKILLS, &sk, indice::LADO_SKILLS, indice::COLUNAS_SKILLS, indice::LARGURA_SKILLS);
    }
}
