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

/// Minimum finger target on a touch screen, in millimetres.
///
/// Apple's 44 pt and Android's 48 dp are ~7–7.6 mm; we go a bit above them
/// because this is a game played with the thumb, often on a moving bus.
pub const ALVO_MM: f32 = 9.0;

/// The MINIMUM TOUCH TARGET in screen pixels, on THIS screen.
///
/// Phones: `ALVO_MM` measured physically (`densidade::px_por_mm`), so it is
/// the same 9 mm on a 720p Moto G and on a 3x iPhone. Desktop: the old
/// `ALVO_DO_DEDO` of 44 px, times the monitor scale. Never below 44.
pub fn alvo_minimo() -> f32 {
    #[cfg(test)]
    {
        estilo::ALVO_DO_DEDO
    }
    #[cfg(not(test))]
    {
        let base = estilo::ALVO_DO_DEDO;
        match crate::densidade::px_por_mm() {
            Some(px) => (ALVO_MM * px).max(base),
            None => base * macroquad::miniquad::window::dpi_scale().max(1.0),
        }
    }
}

/// A ÁREA DE TOQUE de um alvo: a caixa dele, crescida até o dedo caber.
///
/// O desenho fica onde o layout pôs; só o alvo cresce, centrado, até
/// `alvo_minimo` nas duas direções. Não tem mais teto: quem impede o alvo
/// crescido de roubar o toque do vizinho é a arbitragem de `toca` (o toque
/// na margem vai pro alvo MAIS PERTO, e o toque em cima de um botão é sempre
/// dele).
pub fn area_de_toque(r: Rect) -> Rect {
    area_com_alvo(r, alvo_minimo())
}

fn area_com_alvo(r: Rect, alvo: f32) -> Rect {
    let w = r.w.max(alvo);
    let h = r.h.max(alvo);
    Rect::new(r.x - (w - r.w) * 0.5, r.y - (h - r.h) * 0.5, w, h)
}

/// How long a target seen on screen keeps competing for taps. Long enough to
/// cover code that only tests the rect on the frame of the tap, short enough
/// that a closed window stops mattering almost at once (and a stale target
/// only ever takes MARGIN away, never a touch on a real button).
const VIDA_DO_ALVO_S: f64 = 1.0;

thread_local! {
    static ALVOS: std::cell::RefCell<Vec<(Rect, f64)>> = const { std::cell::RefCell::new(Vec::new()) };
    static INICIO: std::time::Instant = std::time::Instant::now();
}

fn agora_s() -> f64 {
    INICIO.with(|i| i.elapsed().as_secs_f64())
}

fn mesmo_ret(a: Rect, b: Rect) -> bool {
    (a.x - b.x).abs() < 1.0 && (a.y - b.y).abs() < 1.0 && (a.w - b.w).abs() < 1.0 && (a.h - b.h).abs() < 1.0
}

/// `o` holds `r` whole: a list area around its rows, a panel around its
/// buttons. A container never competes with what is inside it.
fn contem_ret(o: Rect, r: Rect) -> bool {
    o.x <= r.x + 0.5 && o.y <= r.y + 0.5 && o.x + o.w >= r.x + r.w - 0.5 && o.y + o.h >= r.y + r.h - 0.5
}

fn distancia(r: Rect, p: Vec2) -> f32 {
    let dx = (r.x - p.x).max(p.x - (r.x + r.w)).max(0.0);
    let dy = (r.y - p.y).max(p.y - (r.y + r.h)).max(0.0);
    (dx * dx + dy * dy).sqrt()
}

fn registra(r: Rect, t: f64) {
    ALVOS.with(|a| {
        let mut a = a.borrow_mut();
        a.retain(|(_, visto)| t - visto <= VIDA_DO_ALVO_S);
        if let Some(e) = a.iter_mut().find(|(o, _)| mesmo_ret(*o, r)) {
            e.1 = t;
        } else if a.len() < 4096 {
            a.push((r, t));
        }
    });
}

/// THE hit test of the game: does a tap/click at `p` belong to target `r`?
///
/// Every clickable thing goes through here (`Rect::toca`), so the minimum
/// touch size holds for ALL of them — HUD, menus, shops, lists, tabs, close
/// buttons, arrows, bag slots — on any screen.
///
/// - On the drawing itself: always yes.
/// - In the grown margin (`area_de_toque`): yes, unless another target seen
///   on screen in the last second is closer to `p` (or has `p` on its own
///   drawing). Neighbours in a tight row each get the half that is theirs.
pub fn toca(r: Rect, p: Vec2) -> bool {
    toca_com_alvo(r, p, alvo_minimo(), agora_s())
}

fn toca_com_alvo(r: Rect, p: Vec2, alvo: f32, t: f64) -> bool {
    if !(r.w > 0.0 && r.h > 0.0) {
        return false;
    }
    // Only things the size of a target compete; a whole panel or scroll area
    // is a container, not a button.
    if r.w.min(r.h) < alvo * 2.0 {
        registra(r, t);
    }
    if dentro(r, p) {
        return true;
    }
    if r.w >= alvo && r.h >= alvo {
        return false;
    }
    if !dentro(area_com_alvo(r, alvo), p) {
        return false;
    }
    let d = distancia(r, p);
    ALVOS.with(|a| {
        a.borrow().iter().all(|&(o, visto)| {
            if t - visto > VIDA_DO_ALVO_S || mesmo_ret(o, r) || contem_ret(o, r) || contem_ret(r, o) {
                return true;
            }
            if !dentro(area_com_alvo(o, alvo), p) {
                return true;
            }
            let od = distancia(o, p);
            // Exact tie: a fixed order decides, so exactly one of the two wins.
            od > d || (od == d && (o.y, o.x) > (r.y, r.x))
        })
    })
}

/// `rect.toca(p)` — the touch-sized replacement for `rect.contains(p)` on
/// anything the player can click.
pub trait Toque {
    fn toca(&self, p: Vec2) -> bool;
}

impl Toque for Rect {
    fn toca(&self, p: Vec2) -> bool {
        toca(*self, p)
    }
}

/// Botao. `true` no quadro em que foi clicado.
pub fn botao(r: Rect, rotulo: &str, ativo: bool) -> bool {
    let (mx, my) = mouse_position();
    let sobre = ativo && toca(r, vec2(mx, my));
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
    let sobre = toca(r, vec2(mx, my));
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

    fn limpa() {
        ALVOS.with(|a| a.borrow_mut().clear());
    }

    /// Small target: the touch area reaches the minimum in BOTH directions,
    /// centred on the drawing — no more 14/8 px cap.
    #[test]
    fn o_alvo_cresce_ate_o_minimo_e_fica_centrado() {
        let alvo = 95.0; // 9 mm on a 270 dpi phone
        let r = Rect::new(100.0, 100.0, 30.0, 28.0);
        let a = area_com_alvo(r, alvo);
        assert_eq!((a.w, a.h), (alvo, alvo));
        assert!((a.center() - r.center()).length() < 0.01);
        let g = Rect::new(0.0, 0.0, 300.0, 120.0);
        let ag = area_com_alvo(g, alvo);
        assert_eq!((ag.w, ag.h), (g.w, g.h), "big target must not change");
        // Old rule kept: nothing comes out below 44.
        assert!(area_de_toque(Rect::new(0.0, 0.0, 150.0, 38.0)).h >= 44.0);
    }

    /// A ROW OF TIGHT SLOTS (bag grid, the five colony crafts): every slot
    /// grows past its neighbours, and still each tap lands on exactly ONE
    /// slot — the nearest — and a tap on a slot's drawing is always its own.
    #[test]
    fn fileira_colada_cada_toque_acerta_um_so() {
        limpa();
        let alvo = 95.0;
        let slots: Vec<Rect> = (0..5)
            .map(|i| Rect::new(i as f32 * 34.0, 0.0, 31.0, 31.0))
            .collect();
        let t = 10.0;
        // One frame of drawing registers them (hover test every frame).
        for s in &slots {
            toca_com_alvo(*s, vec2(-999.0, -999.0), alvo, t);
        }
        // Taps across the row and above/below it, inside the grown margins.
        for k in 0..60 {
            let p = vec2(-20.0 + k as f32 * 3.5, if k % 2 == 0 { -25.0 } else { 50.0 });
            let ganhou: Vec<usize> = (0..5)
                .filter(|&i| toca_com_alvo(slots[i], p, alvo, t))
                .collect();
            assert!(ganhou.len() <= 1, "tap {p:?} hit {ganhou:?}");
        }
        // On the drawing of slot 2: slot 2, and only it.
        let c = slots[2].center();
        let ganhou: Vec<usize> = (0..5).filter(|&i| toca_com_alvo(slots[i], c, alvo, t)).collect();
        assert_eq!(ganhou, vec![2]);
        // Just above slot 3 (in the margin): slot 3 — the nearest.
        let p = vec2(slots[3].center().x, -20.0);
        let ganhou: Vec<usize> = (0..5).filter(|&i| toca_com_alvo(slots[i], p, alvo, t)).collect();
        assert_eq!(ganhou, vec![3]);
    }

    /// The scroll area around a list is a CONTAINER: it never steals the
    /// margin of the small buttons inside it.
    #[test]
    fn container_nao_compete_com_o_que_tem_dentro() {
        limpa();
        let alvo = 95.0;
        let lista = Rect::new(0.0, 0.0, 400.0, 150.0);
        let xis = Rect::new(360.0, 10.0, 24.0, 24.0);
        let t = 5.0;
        toca_com_alvo(lista, vec2(-1.0, -1.0), alvo, t);
        toca_com_alvo(xis, vec2(-1.0, -1.0), alvo, t);
        // Left of the "x", inside the list: the "x" still gets it.
        assert!(toca_com_alvo(xis, vec2(330.0, 22.0), alvo, t));
    }

    /// A target that left the screen stops competing after a second.
    #[test]
    fn alvo_velho_deixa_de_competir() {
        limpa();
        let alvo = 95.0;
        let a = Rect::new(0.0, 0.0, 30.0, 30.0);
        let b = Rect::new(40.0, 0.0, 30.0, 30.0);
        toca_com_alvo(b, vec2(-1.0, -1.0), alvo, 1.0);
        // Point between them but closer to b: a loses while b is fresh...
        assert!(!toca_com_alvo(a, vec2(37.0, 15.0), alvo, 1.2));
        // ...and wins once b has been gone for over a second.
        assert!(toca_com_alvo(a, vec2(37.0, 15.0), alvo, 3.0));
    }
}
