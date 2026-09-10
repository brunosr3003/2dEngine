//! Widgets de modo imediato pras telas de fora do mundo.
//!
//! Nao ha framework de UI aqui de proposito. O cliente Unity tinha 24.491
//! linhas de uGUI — mais da metade do projeto — pra fazer menos do que isto
//! precisa fazer. Botao, campo de texto e lista desenhados na mao cabem em
//! duzentas linhas e nao trazem cena, prefab nem serializador junto.

use macroquad::prelude::*;

pub const OURO: Color = Color::new(0.83, 0.65, 0.31, 1.0);
pub const OURO_CLARO: Color = Color::new(0.95, 0.78, 0.44, 1.0);
const FUNDO: Color = Color::new(0.09, 0.08, 0.10, 1.0);
const PAINEL: Color = Color::new(0.13, 0.12, 0.15, 1.0);
const BORDA: Color = Color::new(0.25, 0.21, 0.18, 1.0);
const TEXTO: Color = Color::new(0.87, 0.87, 0.87, 1.0);
const APAGADO: Color = Color::new(0.45, 0.45, 0.48, 1.0);

pub fn fundo() {
    clear_background(FUNDO);
}

/// Painel centralizado. Devolve o retangulo util, ja com margem.
pub fn painel(largura: f32, altura: f32, titulo: &str) -> Rect {
    let x = (screen_width() - largura) * 0.5;
    let y = (screen_height() - altura) * 0.5;
    draw_rectangle(x, y, largura, altura, PAINEL);
    draw_rectangle_lines(x, y, largura, altura, 2.0, BORDA);
    if !titulo.is_empty() {
        let d = measure_text(titulo, None, 26, 1.0);
        draw_text(titulo, x + (largura - d.width) * 0.5, y + 38.0, 26.0, OURO);
    }
    Rect::new(x + 24.0, y + 60.0, largura - 48.0, altura - 84.0)
}

pub fn texto(x: f32, y: f32, s: &str, tam: u16, cor: Color) {
    draw_text(s, x, y, tam as f32, cor);
}

pub fn texto_centro(cx: f32, y: f32, s: &str, tam: u16, cor: Color) {
    let d = measure_text(s, None, tam, 1.0);
    draw_text(s, cx - d.width * 0.5, y, tam as f32, cor);
}

fn dentro(r: Rect, p: Vec2) -> bool {
    p.x >= r.x && p.x <= r.x + r.w && p.y >= r.y && p.y <= r.y + r.h
}

/// Botao. `true` no quadro em que foi clicado.
pub fn botao(r: Rect, rotulo: &str, ativo: bool) -> bool {
    let (mx, my) = mouse_position();
    let sobre = ativo && dentro(r, vec2(mx, my));
    let fundo = if !ativo {
        Color::new(0.16, 0.15, 0.17, 1.0)
    } else if sobre {
        Color::new(0.24, 0.21, 0.16, 1.0)
    } else {
        Color::new(0.18, 0.17, 0.19, 1.0)
    };
    draw_rectangle(r.x, r.y, r.w, r.h, fundo);
    draw_rectangle_lines(r.x, r.y, r.w, r.h, 1.5, if sobre { OURO } else { BORDA });
    let cor = if ativo { OURO_CLARO } else { APAGADO };
    let d = measure_text(rotulo, None, 20, 1.0);
    draw_text(
        rotulo,
        r.x + (r.w - d.width) * 0.5,
        r.y + r.h * 0.5 + 7.0,
        20.0,
        cor,
    );
    sobre && is_mouse_button_pressed(MouseButton::Left)
}

/// Campo de texto. `foco` diz quem recebe o teclado; devolve `true` se o
/// clique pediu o foco.
pub fn campo(r: Rect, rotulo: &str, valor: &mut String, foco: bool, senha: bool) -> bool {
    draw_text(rotulo, r.x, r.y - 8.0, 16.0, APAGADO);
    draw_rectangle(r.x, r.y, r.w, r.h, Color::new(0.10, 0.09, 0.11, 1.0));
    draw_rectangle_lines(r.x, r.y, r.w, r.h, 1.5, if foco { OURO } else { BORDA });

    let mostrado = if senha {
        "*".repeat(valor.chars().count())
    } else {
        valor.clone()
    };
    // Cursor piscando: sem ele nao da' pra saber qual campo esta ouvindo.
    let cursor = if foco && (get_time() * 2.0) as i32 % 2 == 0 { "_" } else { "" };
    draw_text(
        &format!("{mostrado}{cursor}"),
        r.x + 10.0,
        r.y + r.h * 0.5 + 6.0,
        20.0,
        TEXTO,
    );

    if foco {
        while let Some(c) = get_char_pressed() {
            // 8 = backspace, 13/10 = enter. O enter e' tratado por quem chama.
            if c as u32 == 8 {
                valor.pop();
            } else if !c.is_control() && valor.chars().count() < 32 {
                valor.push(c);
            }
        }
        if is_key_pressed(KeyCode::Backspace) {
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
        let c = if selecionada {
            Color::new(0.22, 0.19, 0.14, 1.0)
        } else {
            Color::new(0.17, 0.16, 0.18, 1.0)
        };
        draw_rectangle(r.x, r.y, r.w, r.h, c);
    }
    if selecionada {
        draw_rectangle(r.x, r.y, 3.0, r.h, OURO);
    }
    draw_text(esquerda, r.x + 12.0, r.y + r.h * 0.5 + 6.0, 19.0, TEXTO);
    let d = measure_text(direita, None, 17, 1.0);
    draw_text(
        direita,
        r.x + r.w - d.width - 12.0,
        r.y + r.h * 0.5 + 5.0,
        17.0,
        APAGADO,
    );
    sobre && is_mouse_button_pressed(MouseButton::Left)
}

/// Barra de lotacao. Vermelha quando cheia — o jogador decide antes de clicar.
pub fn barra(r: Rect, fracao: f32) {
    let f = fracao.clamp(0.0, 1.0);
    draw_rectangle(r.x, r.y, r.w, r.h, Color::new(0.10, 0.09, 0.11, 1.0));
    let cor = if f >= 1.0 {
        Color::new(0.78, 0.33, 0.24, 1.0)
    } else if f > 0.8 {
        Color::new(0.85, 0.66, 0.28, 1.0)
    } else {
        Color::new(0.42, 0.66, 0.38, 1.0)
    };
    draw_rectangle(r.x, r.y, r.w * f, r.h, cor);
    draw_rectangle_lines(r.x, r.y, r.w, r.h, 1.0, BORDA);
}

pub fn erro(cx: f32, y: f32, msg: &str) {
    texto_centro(cx, y, msg, 18, Color::new(0.85, 0.35, 0.30, 1.0));
}
