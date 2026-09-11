//! HUD do mundo: onde estou, quao cheio esta, e o que o servidor disse.
//!
//! Num jogo com servidor/canal/zona, "onde eu estou" deixa de ser obvio — o
//! mesmo personagem aparece em lugares diferentes conforme o canal, e a
//! lotacao muda o que da' pra fazer ali. Por isso isso fica na tela, nao num
//! menu.

use macroquad::prelude::*;

use crate::map::Map;
use crate::ui;

/// O que o servidor contou sobre este canal (`ServerMessage::InfoCanal`).
#[derive(Clone, Default)]
pub struct Info {
    pub realm: String,
    pub canal: String,
    pub zona: String,
    pub jogadores: u32,
    pub capacidade: u32,
}

/// Custo de rede visto do lado do cliente.
///
/// Latencia e banda nao sao numero de desenvolvedor: sao as duas contas que
/// dizem se o jogo e' jogavel no celular de quem esta jogando, e ambas mudam
/// com a lotacao do canal. Por isso ficam na tela junto do fps.
#[derive(Clone, Default)]
pub struct Rede {
    /// Ida e volta em ms, suavizado.
    pub ms: f32,
    /// Taxa de recepcao no ultimo segundo.
    pub kbs: f32,
    /// Tudo que ja' desceu nesta sessao.
    pub total_bytes: u64,
}

impl Rede {
    /// Cor do numero: verde ate' 80ms, ambar ate' 150, vermelho acima. Sao os
    /// limites em que combate por alvo comeca a errar clique.
    fn cor_ms(&self) -> Color {
        if self.ms <= 0.0 {
            Color::new(0.45, 0.45, 0.48, 1.0)
        } else if self.ms < 80.0 {
            Color::new(0.42, 0.70, 0.40, 1.0)
        } else if self.ms < 150.0 {
            Color::new(0.85, 0.66, 0.28, 1.0)
        } else {
            Color::new(0.85, 0.40, 0.33, 1.0)
        }
    }

    /// Quanto isso custa por hora de jogo. E' o numero que o jogador de plano
    /// pre-pago sente; KB/s sozinho nao diz nada pra ninguem.
    fn mb_por_hora(&self) -> f32 {
        self.kbs * 3600.0 / 1024.0
    }
}

impl Info {
    fn fracao(&self) -> f32 {
        if self.capacidade == 0 {
            0.0
        } else {
            self.jogadores as f32 / self.capacidade as f32
        }
    }
}

/// Desenha o HUD. Devolve `true` no quadro em que o jogador pediu pra sair.
pub fn draw_hud(
    info: &Info,
    rede: &Rede,
    map: Option<&Map>,
    tick: u32,
    ents: usize,
    // Pedacos de terreno vivos: e' o numero que denuncia streaming
    // engasgando — se ele para de subir enquanto o jogador anda, o mundo
    // esta' aparecendo devagar.
    pedacos: usize,
    vivos: usize,
    pos: Vec2,
    chat: &[String],
) -> bool {
    // ── Cartao de localizacao, canto superior DIREITO ──
    // O esquerdo e' da ficha do personagem (vida, mana), como no MIR4.
    let l = 250.0;
    let a = if info.capacidade > 0 { 74.0 } else { 56.0 };
    let cx0 = screen_width() - l - 10.0;
    let cy0 = 52.0;
    draw_rectangle(cx0, cy0, l, a, Color::new(0.08, 0.07, 0.09, 0.82));
    draw_rectangle_lines(cx0, cy0, l, a, 1.5, Color::new(0.25, 0.21, 0.18, 1.0));

    let onde = if info.realm.is_empty() {
        map.map(|m| m.name.clone()).unwrap_or_else(|| "?".into())
    } else {
        format!("{} · canal {}", info.realm, info.canal)
    };
    ui::texto(cx0 + 10.0, cy0 + 20.0, &onde, 18, ui::OURO_CLARO);
    let zona = if info.zona.is_empty() {
        map.map(|m| m.name.as_str()).unwrap_or("?").to_string()
    } else {
        info.zona.clone()
    };
    ui::texto(cx0 + 10.0, cy0 + 38.0, &zona, 15, Color::new(0.70, 0.70, 0.72, 1.0));

    if info.capacidade > 0 {
        let f = info.fracao();
        ui::barra(Rect::new(cx0 + 10.0, cy0 + 46.0, l - 20.0, 6.0), f);
        let txt = format!("{}/{} online", info.jogadores, info.capacidade);
        let d = measure_text(&txt, None, 13, 1.0);
        ui::texto(
            cx0 + l - d.width - 10.0,
            cy0 + 40.0,
            &txt,
            13,
            if f > 0.9 {
                Color::new(0.85, 0.45, 0.35, 1.0)
            } else {
                Color::new(0.60, 0.60, 0.62, 1.0)
            },
        );
    }

    // ── Diagnostico, canto superior direito ──
    // Fica fora do cartao de proposito: e' numero de desenvolvimento, nao
    // informacao de jogo. A latencia sai em cor propria porque ela e' a unica
    // linha aqui que muda o que da' pra fazer no combate.
    let apagado = Color::new(0.45, 0.45, 0.48, 1.0);
    let borda = screen_width() - 12.0;

    let ms = if rede.ms > 0.0 { format!("{:.0} ms", rede.ms) } else { "-- ms".into() };
    let d_ms = measure_text(&ms, None, 14, 1.0);
    ui::texto(borda - d_ms.width, 24.0, &ms, 14, rede.cor_ms());

    let resto = format!(
        "tick {tick} · {ents} ents · {pedacos}/{vivos} ped · {:.0} fps · {:.0},{:.0} · ",
        get_fps(), pos.x, pos.y
    );
    let d_resto = measure_text(&resto, None, 14, 1.0);
    ui::texto(borda - d_ms.width - d_resto.width, 24.0, &resto, 14, apagado);

    // Segunda linha: banda. KB/s diz pouco sozinho, MB/h e' o numero que o
    // jogador de plano pre-pago sente, e o total mostra o acumulado da sessao.
    let banda = format!(
        "{:.1} KB/s · {:.0} MB/h · {:.1} MB nesta sessão",
        rede.kbs,
        rede.mb_por_hora(),
        rede.total_bytes as f32 / (1024.0 * 1024.0)
    );
    let d_b = measure_text(&banda, None, 13, 1.0);
    ui::texto(borda - d_b.width, 42.0, &banda, 13, apagado);

    // ── Chat ──
    // o chat fica acima da barra de experiencia
    let mut y = screen_height() - 34.0 - chat.len() as f32 * 18.0;
    for linha in chat {
        ui::texto(14.0, y, linha, 16, Color::new(0.82, 0.82, 0.84, 1.0));
        y += 18.0;
    }

    // ── Sair ──
    // Sem isto so' da' pra trocar de conta fechando o processo. Fica embaixo
    // do cartao de localizacao, longe do centro onde se clica pra mirar.
    ui::botao(Rect::new(screen_width() - 102.0, cy0 + a + 8.0, 92.0, 26.0), "sair", true)
}

// ═══════════════════════════════════════════════════════════════════════
//  A FICHA — vida, mana, vigor e experiencia, no molde do MIR4
// ═══════════════════════════════════════════════════════════════════════

/// O que o servidor conta do proprio personagem e o HUD mostra.
#[derive(Clone, Default)]
pub struct Ficha {
    /// `None` ate' o primeiro `ManaUpdate`: ai' a barra aparece cheia.
    pub mp: Option<i32>,
    pub vigor: Option<i32>,
    pub xp: u64,
    pub nivel: u32,
    /// Multiplicador da curva de XP (`HandshakeAck`), pra conta bater com a
    /// do servidor.
    pub mult_xp: u64,
    /// A vida que acabou de ser perdida: um pedaco claro que desce devagar
    /// atras da barra, pra o golpe ser lido e nao so' visto.
    hp_rastro: f32,
}

fn texto_contornado(s: &str, x: f32, y: f32, tam: u16, cor: Color) {
    for (dx, dy) in [(-1.0, 0.0), (1.0, 0.0), (0.0, -1.0), (0.0, 1.0)] {
        draw_text(s, x + dx, y + dy, tam as f32, Color::new(0.0, 0.0, 0.0, 0.85));
    }
    draw_text(s, x, y, tam as f32, cor);
}

/// Uma barra de recurso: fundo escuro, o preenchimento com um brilho em
/// cima, o rastro claro do que acabou de sair e o numero no meio.
fn barra_de_recurso(r: Rect, f: f32, rastro: f32, cor: Color, brilho: Color, texto: Option<&str>) {
    let f = f.clamp(0.0, 1.0);
    draw_rectangle(r.x - 1.5, r.y - 1.5, r.w + 3.0, r.h + 3.0, Color::new(0.0, 0.0, 0.0, 0.7));
    draw_rectangle(r.x, r.y, r.w, r.h, Color::new(0.10, 0.08, 0.09, 0.95));
    if rastro > f {
        draw_rectangle(r.x + r.w * f, r.y, r.w * (rastro.min(1.0) - f), r.h, Color::new(1.0, 0.92, 0.85, 0.7));
    }
    draw_rectangle(r.x, r.y, r.w * f, r.h, cor);
    draw_rectangle(r.x, r.y, r.w * f, r.h * 0.38, brilho);
    if let Some(t) = texto {
        let tam = (r.h * 0.85).clamp(10.0, 15.0) as u16;
        let d = measure_text(t, None, tam, 1.0);
        texto_contornado(t, r.x + (r.w - d.width) * 0.5, r.y + r.h * 0.5 + tam as f32 * 0.35, tam, WHITE);
    }
}

/// A ficha no canto superior esquerdo: o nivel num circulo, o nome, a vida,
/// a mana e o vigor.
pub fn draw_ficha(ficha: &mut Ficha, dt: f32, nome: &str, nivel: u32, hp: i32, hp_max: i32, mp_max: i32, vigor_max: i32) {
    let (x, y) = (14.0, 14.0);
    let c = vec2(x + 34.0, y + 34.0);
    draw_circle(c.x, c.y, 35.0, Color::new(0.0, 0.0, 0.0, 0.6));
    draw_circle(c.x, c.y, 31.0, Color::new(0.12, 0.10, 0.14, 0.95));
    draw_circle_lines(c.x, c.y, 31.0, 2.5, ui::OURO);
    ui::texto_centro(c.x, c.y - 6.0, "Lv", 13, Color::new(0.62, 0.60, 0.58, 1.0));
    ui::texto_centro(c.x, c.y + 16.0, &nivel.to_string(), 26, ui::OURO_CLARO);

    let bx = x + 78.0;
    texto_contornado(nome, bx, y + 16.0, 18, ui::OURO_CLARO);

    let f_hp = (hp as f32 / hp_max.max(1) as f32).clamp(0.0, 1.0);
    ficha.hp_rastro = if f_hp >= ficha.hp_rastro { f_hp } else { (ficha.hp_rastro - dt * 0.35).max(f_hp) };
    let pulsa = if f_hp < 0.3 { 0.75 + 0.25 * (get_time() as f32 * 6.0).sin() } else { 1.0 };
    barra_de_recurso(
        Rect::new(bx, y + 24.0, 230.0, 16.0),
        f_hp,
        ficha.hp_rastro,
        Color::new(0.82 * pulsa, 0.16, 0.14, 1.0),
        Color::new(1.0, 0.48, 0.42, 0.5),
        Some(&format!("{hp} / {hp_max}")),
    );
    let mp = ficha.mp.unwrap_or(mp_max);
    barra_de_recurso(
        Rect::new(bx, y + 44.0, 230.0, 12.0),
        mp as f32 / mp_max.max(1) as f32,
        0.0,
        Color::new(0.18, 0.38, 0.88, 1.0),
        Color::new(0.52, 0.70, 1.0, 0.5),
        Some(&format!("{mp} / {mp_max}")),
    );
    let vigor = ficha.vigor.unwrap_or(vigor_max);
    barra_de_recurso(
        Rect::new(bx, y + 60.0, 230.0, 5.0),
        vigor as f32 / vigor_max.max(1) as f32,
        0.0,
        Color::new(0.86, 0.74, 0.22, 1.0),
        Color::new(1.0, 0.92, 0.55, 0.5),
        None,
    );
}

/// A experiencia: uma faixa fina na tela inteira, no pe', com a
/// porcentagem — o numero que o jogador de MMO olha a cada bicho.
pub fn draw_exp(ficha: &Ficha, nivel: u32) {
    let mult = if ficha.mult_xp > 0 { ficha.mult_xp } else { shared::DEFAULT_XP_MULTIPLIER };
    let base = shared::xp_for_level_with_mult(nivel, mult);
    let prox = shared::xp_for_level_with_mult(nivel + 1, mult);
    let f = if prox > base { (ficha.xp.saturating_sub(base)) as f32 / (prox - base) as f32 } else { 0.0 };
    let (w, h) = (screen_width(), 10.0);
    let y = screen_height() - h;
    draw_rectangle(0.0, y, w, h, Color::new(0.06, 0.05, 0.07, 0.9));
    draw_rectangle(0.0, y, w * f.clamp(0.0, 1.0), h, Color::new(0.86, 0.66, 0.24, 1.0));
    draw_rectangle(0.0, y, w * f.clamp(0.0, 1.0), h * 0.4, Color::new(1.0, 0.88, 0.55, 0.5));
    for k in 1..10 {
        let x = w * k as f32 / 10.0;
        draw_line(x, y, x, y + h, 1.0, Color::new(0.0, 0.0, 0.0, 0.45));
    }
    let t = format!("EXP {:.2}%", (f * 100.0).clamp(0.0, 100.0));
    let d = measure_text(&t, None, 13, 1.0);
    texto_contornado(&t, (w - d.width) * 0.5, y - 3.0, 13, Color::new(1.0, 0.9, 0.62, 1.0));
}

/// O alvo, no alto e ao centro: nivel, nome e a vida em barra larga.
pub fn draw_alvo(nome: &str, nivel: u16, hp: u16, hp_max: u16, chefe: bool) {
    let w = 320.0;
    let x = (screen_width() - w) * 0.5;
    let y = 14.0;
    let titulo = if nivel > 0 { format!("Lv {nivel}  {nome}") } else { nome.to_string() };
    let d = measure_text(&titulo, None, 18, 1.0);
    let cor = if chefe { Color::new(1.0, 0.55, 0.25, 1.0) } else { Color::new(1.0, 0.93, 0.72, 1.0) };
    texto_contornado(&titulo, x + (w - d.width) * 0.5, y + 16.0, 18, cor);
    let f = hp as f32 / hp_max.max(1) as f32;
    barra_de_recurso(
        Rect::new(x, y + 24.0, w, 14.0),
        f,
        0.0,
        Color::new(0.80, 0.18, 0.15, 1.0),
        Color::new(1.0, 0.48, 0.42, 0.5),
        Some(&format!("{:.0}%", (f * 100.0).clamp(0.0, 100.0))),
    );
}
