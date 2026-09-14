//! Widgets de modo imediato pras telas de fora do mundo.
//!
//! Nao ha framework de UI aqui de proposito. O cliente Unity tinha 24.491
//! linhas de uGUI — mais da metade do projeto — pra fazer menos do que isto
//! precisa fazer. Botao, campo de texto e lista desenhados na mao cabem em
//! duzentas linhas e nao trazem cena, prefab nem serializador junto.
//!
//! A cara vem do `hud_estilo`: as telas de login e de servidor usam os mesmos
//! paineis, botoes e fonte do jogo.

use macroquad::prelude::*;

use crate::hud_estilo as estilo;

pub const OURO: Color = estilo::OURO;
pub const OURO_CLARO: Color = Color::new(1.0, 0.86, 0.56, 1.0);
const FUNDO_TOPO: Color = Color::new(0.055, 0.075, 0.115, 1.0);
const FUNDO_BASE: Color = Color::new(0.020, 0.028, 0.045, 1.0);
const TEXTO: Color = estilo::TEXTO;
const APAGADO: Color = estilo::SUAVE;

/// Fundo das telas fora do mundo: gradiente escuro com um halo frio no alto.
pub fn fundo() {
    clear_background(FUNDO_BASE);
    let (w, h) = (screen_width(), screen_height());
    estilo::ret_gradiente(Rect::new(0.0, 0.0, w, h), 0.0, FUNDO_TOPO, FUNDO_BASE);
    draw_circle(w * 0.5, -h * 0.35, h * 0.9, Color::new(0.30, 0.55, 0.85, 0.05));
}

thread_local! {
    /// Quanto os paineis sobem com o teclado da tela aberto (ver
    /// `teclado_virtual::deslocamento`). Zero no desktop.
    static SUBIDA: std::cell::Cell<f32> = const { std::cell::Cell::new(0.0) };
}

/// Sobe os paineis centralizados em `dy` px (0 = centro normal).
pub fn subir_paineis(dy: f32) {
    SUBIDA.with(|s| s.set(dy.max(0.0)));
}

/// Painel centralizado. Devolve o retangulo util, ja com margem.
pub fn painel(largura: f32, altura: f32, titulo: &str) -> Rect {
    let x = (screen_width() - largura) * 0.5;
    let y = (screen_height() - altura) * 0.5 - SUBIDA.with(|s| s.get());
    let r = Rect::new(x, y, largura, altura);
    estilo::painel_destaque(r, OURO);
    if !titulo.is_empty() {
        estilo::texto_centro_forte(x + largura * 0.5, y + 38.0, titulo, 24, OURO);
        estilo::separador(x + 24.0, y + 50.0, largura - 48.0);
    }
    Rect::new(x + 24.0, y + 60.0, largura - 48.0, altura - 84.0)
}

pub fn texto(x: f32, y: f32, s: &str, tam: u16, cor: Color) {
    estilo::texto(x, y, s, tam, cor);
}

pub fn texto_centro(cx: f32, y: f32, s: &str, tam: u16, cor: Color) {
    estilo::texto_centro(cx, y, s, tam, cor);
}

fn dentro(r: Rect, p: Vec2) -> bool {
    p.x >= r.x && p.x <= r.x + r.w && p.y >= r.y && p.y <= r.y + r.h
}

/// Botao. `true` no quadro em que foi clicado.
pub fn botao(r: Rect, rotulo: &str, ativo: bool) -> bool {
    let (mx, my) = mouse_position();
    let sobre = ativo && dentro(r, vec2(mx, my));
    let e = estilo::estado(sobre, is_mouse_button_down(MouseButton::Left), !ativo, false);
    // O "x" de fechar e' icone, nao rotulo: botao discreto.
    estilo::botao(r, rotulo, e, false);
    sobre && is_mouse_button_pressed(MouseButton::Left)
}

/// Campo de texto.
///
/// `digitado` vem do `entrada::Teclado` e nao da fila crua da macroquad: la' a
/// repeticao de tecla e' indistinguivel do toque, e uma tecla encostada por um
/// instante entrava quatro vezes. `foco` diz quem recebe o teclado; devolve
/// `true` se o clique pediu o foco.
pub fn campo(
    r: Rect,
    rotulo: &str,
    valor: &mut String,
    foco: bool,
    senha: bool,
    digitado: &[char],
) -> bool {
    estilo::texto_forte(r.x + 2.0, r.y - 8.0, rotulo, estilo::tam::LEGENDA, APAGADO);
    let raio = estilo::RAIO_PEQUENO + 2.0;
    estilo::ret_gradiente(r, raio, estilo::FUNDO_BAIXO, estilo::alfa(estilo::clarear(estilo::FUNDO_BAIXO, 0.05), 0.95));
    estilo::borda_arredondada(r, raio, if foco { 1.5 } else { 1.0 }, if foco { estilo::alfa(estilo::ACENTO, 0.85) } else { estilo::BORDA_FORTE });
    if foco {
        estilo::borda_arredondada(Rect::new(r.x - 3.0, r.y - 3.0, r.w + 6.0, r.h + 6.0), raio + 3.0, 2.0, estilo::alfa(estilo::ACENTO, 0.18));
    }

    let mostrado = if senha {
        "•".repeat(valor.chars().count())
    } else {
        valor.clone()
    };
    // Cursor piscando: sem ele nao da' pra saber qual campo esta ouvindo.
    let cursor = if foco && (get_time() * 2.0) as i32 % 2 == 0 { "|" } else { "" };
    estilo::texto(r.x + 12.0, r.y + r.h * 0.5 + 6.0, &format!("{mostrado}{cursor}"), 18, TEXTO);

    if foco {
        for &c in digitado {
            // 8 = backspace, 13/10 = enter. O enter e' tratado por quem chama.
            if c as u32 == 8 {
                valor.pop();
            } else if !c.is_control() && valor.chars().count() < 32 {
                valor.push(c);
            }
        }
        // Backspace nem sempre chega como caractere: em alguns teclados ele
        // vem so' como tecla.
        if is_key_pressed(KeyCode::Backspace) && !digitado.contains(&'\u{8}') {
            valor.pop();
        }
    }

    let (mx, my) = mouse_position();
    dentro(r, vec2(mx, my)) && is_mouse_button_pressed(MouseButton::Left)
}

/// Linha de lista selecionavel. Devolve `true` quando clicada.
pub fn linha(r: Rect, esquerda: &str, direita: &str, selecionada: bool) -> bool {
    let (mx, my) = mouse_position();
    let sobre = dentro(r, vec2(mx, my));
    if selecionada || sobre {
        estilo::cartao(r, sobre, selecionada);
    }
    if selecionada {
        estilo::ret_arredondado(Rect::new(r.x + 4.0, r.y + 6.0, 3.0, r.h - 12.0), 1.5, estilo::ACENTO);
    }
    estilo::texto(r.x + 14.0, r.y + r.h * 0.5 + 6.0, esquerda, 17, TEXTO);
    let w = estilo::medir(direita, 15);
    estilo::texto(r.x + r.w - w - 12.0, r.y + r.h * 0.5 + 5.0, direita, 15, APAGADO);
    sobre && is_mouse_button_pressed(MouseButton::Left)
}

/// Barra de lotacao. Vermelha quando cheia — o jogador decide antes de clicar.
pub fn barra(r: Rect, fracao: f32) {
    let f = fracao.clamp(0.0, 1.0);
    let cor = if f >= 1.0 {
        estilo::VERMELHO
    } else if f > 0.8 {
        estilo::OURO
    } else {
        estilo::VERDE
    };
    estilo::barra(r, f, 0.0, cor, None);
}

pub fn erro(cx: f32, y: f32, msg: &str) {
    estilo::texto_centro_forte(cx, y, msg, 16, estilo::VERMELHO);
}
