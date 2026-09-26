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
/// Texto de apoio: dica, motivo, legenda. Ja' existia como `APAGADO`
/// privado; virou publico quando a tela de cadastro precisou dizer POR QUE o
/// botao esta' desligado.
pub const APOIO: Color = APAGADO;
const FUNDO_TOPO: Color = Color::new(0.055, 0.075, 0.115, 1.0);
const FUNDO_BASE: Color = Color::new(0.020, 0.028, 0.045, 1.0);
const TEXTO: Color = estilo::TEXTO;
const APAGADO: Color = estilo::SUAVE;

/// Fundo das telas fora do mundo: gradiente escuro com um halo frio no alto.
pub fn fundo() {
    clear_background(FUNDO_BASE);
    let (w, h) = (screen_width(), screen_height());
    estilo::ret_gradiente(Rect::new(0.0, 0.0, w, h), 0.0, FUNDO_TOPO, FUNDO_BASE);
    draw_circle(
        w * 0.5,
        -h * 0.35,
        h * 0.9,
        Color::new(0.30, 0.55, 0.85, 0.05),
    );
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

/// A ÁREA DE TOQUE de um botão: a caixa dele, crescida até o dedo caber.
///
/// O desenho fica onde o layout pôs; só o alvo cresce. É o conserto de origem
/// pros 55 botões do cliente que estavam abaixo de `ALVO_DO_DEDO` — o dono:
/// "os botões de ação estão muito pequenos, eu clico errado toda hora".
///
/// **O crescimento é limitado**, e isso importa: há fileiras de botões
/// pequenos colados (os cinco ofícios da colônia, o `‹ ›` do rastreador). Um
/// alvo que crescesse até 44 sem teto invadiria o vizinho, e aí o toque
/// acertaria o botão errado — trocando um defeito por outro pior, porque o
/// errado seria invisível. Sete pontos por lado na vertical e quatro na
/// horizontal levam 28 a 42 e 38 a 44, sem alcançar vizinho que já esteja
/// visualmente separado.
pub fn area_de_toque(r: Rect) -> Rect {
    let alvo = estilo::ALVO_DO_DEDO;
    let h = r.h.max(alvo).min(r.h + 14.0);
    let w = r.w.max(alvo).min(r.w + 8.0);
    Rect::new(r.x - (w - r.w) * 0.5, r.y - (h - r.h) * 0.5, w, h)
}

/// Botao. `true` no quadro em que foi clicado.
pub fn botao(r: Rect, rotulo: &str, ativo: bool) -> bool {
    let (mx, my) = mouse_position();
    let sobre = ativo && dentro(area_de_toque(r), vec2(mx, my));
    let e = estilo::estado(
        sobre,
        is_mouse_button_down(MouseButton::Left),
        !ativo,
        false,
    );
    // O "x" de fechar e' icone, nao rotulo: botao discreto.
    estilo::botao(r, rotulo, e, false);
    let clicou = sobre && crate::foco::clique();
    if clicou { crate::sons::tocar(crate::sons::Som::Clique); }
    clicou
}

/// Caixinha de marcar, com o rotulo ao lado.
///
/// A area de toque e' a LINHA INTEIRA, e nao o quadradinho de 20 px: um
/// quadrado de 20 esta' muito abaixo dos 44 pt do dedo (a mesma medida que ja'
/// obrigou a crescer 55 botoes deste jogo). Clicar no rotulo tambem marca,
/// que e' o que todo mundo tenta fazer.
///
/// Devolve `true` no quadro em que mudou.
pub fn caixa(r: Rect, rotulo: &str, marcado: &mut bool) -> bool {
    let (mx, my) = mouse_position();
    let sobre = dentro(area_de_toque(r), vec2(mx, my));
    let lado = 20.0_f32.min(r.h);
    let q = Rect::new(r.x, r.y + (r.h - lado) * 0.5, lado, lado);
    estilo::ret_gradiente(
        q,
        estilo::RAIO_PEQUENO,
        estilo::FUNDO_BAIXO,
        estilo::FUNDO_ALTO,
    );
    estilo::borda_arredondada(
        q,
        estilo::RAIO_PEQUENO,
        1.5,
        if sobre { OURO } else { APAGADO },
    );
    if *marcado {
        // Um "v" de dois riscos, e nao um caractere: fonte de jogo nem sempre
        // tem o glifo, e um tofu no lugar da marca e' pior que nada.
        let (x, y, k) = (q.x + lado * 0.22, q.y + lado * 0.52, lado);
        draw_line(x, y, x + k * 0.22, y + k * 0.22, 2.5, OURO);
        draw_line(
            x + k * 0.22,
            y + k * 0.22,
            x + k * 0.58,
            y - k * 0.26,
            2.5,
            OURO,
        );
    }
    estilo::texto(
        q.x + lado + 10.0,
        r.y + r.h * 0.5 + 5.0,
        rotulo,
        estilo::tam::CORPO,
        if sobre { OURO_CLARO } else { estilo::TEXTO },
    );
    if sobre && crate::foco::clique() {
        *marcado = !*marcado;
        return true;
    }
    false
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
    estilo::ret_gradiente(
        r,
        raio,
        estilo::FUNDO_BAIXO,
        estilo::alfa(estilo::clarear(estilo::FUNDO_BAIXO, 0.05), 0.95),
    );
    estilo::borda_arredondada(
        r,
        raio,
        if foco { 1.5 } else { 1.0 },
        if foco {
            estilo::alfa(estilo::ACENTO, 0.85)
        } else {
            estilo::BORDA_FORTE
        },
    );
    if foco {
        estilo::borda_arredondada(
            Rect::new(r.x - 3.0, r.y - 3.0, r.w + 6.0, r.h + 6.0),
            raio + 3.0,
            2.0,
            estilo::alfa(estilo::ACENTO, 0.18),
        );
    }

    let mostrado = if senha {
        "•".repeat(valor.chars().count())
    } else {
        valor.clone()
    };
    // Cursor piscando: sem ele nao da' pra saber qual campo esta ouvindo.
    let cursor = if foco && (get_time() * 2.0) as i32 % 2 == 0 {
        "|"
    } else {
        ""
    };
    estilo::texto(
        r.x + 12.0,
        r.y + r.h * 0.5 + 6.0,
        &format!("{mostrado}{cursor}"),
        18,
        TEXTO,
    );

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
    dentro(r, vec2(mx, my)) && crate::foco::clique()
}

/// Linha de lista selecionavel. Devolve `true` quando clicada.
pub fn linha(r: Rect, esquerda: &str, direita: &str, selecionada: bool) -> bool {
    let (mx, my) = mouse_position();
    let sobre = dentro(r, vec2(mx, my));
    if selecionada || sobre {
        estilo::cartao(r, sobre, selecionada);
    }
    if selecionada {
        estilo::ret_arredondado(
            Rect::new(r.x + 4.0, r.y + 6.0, 3.0, r.h - 12.0),
            1.5,
            estilo::ACENTO,
        );
    }
    estilo::texto(r.x + 14.0, r.y + r.h * 0.5 + 6.0, esquerda, 17, TEXTO);
    let w = estilo::medir(direita, 15);
    estilo::texto(
        r.x + r.w - w - 12.0,
        r.y + r.h * 0.5 + 5.0,
        direita,
        15,
        APAGADO,
    );
    sobre && crate::foco::clique()
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
#[cfg(test)]
mod testes_do_alvo {
    use super::*;

    /// O ALVO CRESCE até o dedo caber, e PARA antes de invadir o vizinho.
    ///
    /// Os dois lados importam. Sem crescer, os 55 botões abaixo de 44 pontos
    /// continuam errando o toque — foi a queixa do dono. Crescendo sem teto,
    /// uma fileira de botões colados (os cinco ofícios da colônia) passa a
    /// acertar o vizinho, que é pior: o erro fica invisível.
    #[test]
    fn o_alvo_cresce_ate_o_dedo_e_para_antes_do_vizinho() {
        let alvo = estilo::ALVO_DO_DEDO;

        // Pequeno demais: cresce, e fica CENTRADO no original.
        let r = Rect::new(100.0, 100.0, 108.0, 28.0);
        let a = area_de_toque(r);
        assert!(a.h > r.h, "não cresceu");
        assert!(a.h <= r.h + 14.0, "cresceu além do teto: {:.0}", a.h);
        assert!(
            (a.center().x - r.center().x).abs() < 0.01
                && (a.center().y - r.center().y).abs() < 0.01,
            "o alvo saiu do centro do botão"
        );

        // Já grande: não muda nada.
        let g = Rect::new(0.0, 0.0, 120.0, 50.0);
        let ag = area_de_toque(g);
        assert_eq!((ag.w, ag.h), (g.w, g.h), "botão grande não devia crescer");

        // FILEIRA COLADA: cinco botões de 62x32 lado a lado, como os ofícios
        // da colônia. Nenhum alvo pode alcançar o CENTRO do vizinho — se
        // alcançasse, tocar no meio de um acertaria o outro.
        let bw = 62.0;
        let caixas: Vec<Rect> = (0..5)
            .map(|i| Rect::new(i as f32 * bw, 0.0, bw - 4.0, 32.0))
            .collect();
        for (i, c) in caixas.iter().enumerate() {
            let a = area_de_toque(*c);
            for (j, o) in caixas.iter().enumerate() {
                if i == j {
                    continue;
                }
                assert!(
                    !dentro(a, o.center()),
                    "o alvo do botão {i} alcança o centro do {j}"
                );
            }
        }

        // E o piso é respeitado onde há espaço: um botão de 38 chega a 44.
        assert_eq!(area_de_toque(Rect::new(0.0, 0.0, 150.0, 38.0)).h, alvo);
    }
}
