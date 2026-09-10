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
    // ── Cartao de localizacao, canto superior esquerdo ──
    let l = 250.0;
    let a = if info.capacidade > 0 { 74.0 } else { 56.0 };
    draw_rectangle(10.0, 10.0, l, a, Color::new(0.08, 0.07, 0.09, 0.82));
    draw_rectangle_lines(10.0, 10.0, l, a, 1.5, Color::new(0.25, 0.21, 0.18, 1.0));

    let onde = if info.realm.is_empty() {
        map.map(|m| m.name.clone()).unwrap_or_else(|| "?".into())
    } else {
        format!("{} · canal {}", info.realm, info.canal)
    };
    ui::texto(20.0, 30.0, &onde, 18, ui::OURO_CLARO);
    let zona = if info.zona.is_empty() {
        map.map(|m| m.name.as_str()).unwrap_or("?").to_string()
    } else {
        info.zona.clone()
    };
    ui::texto(20.0, 48.0, &zona, 15, Color::new(0.70, 0.70, 0.72, 1.0));

    if info.capacidade > 0 {
        let f = info.fracao();
        ui::barra(Rect::new(20.0, 56.0, l - 20.0, 6.0), f);
        let txt = format!("{}/{} online", info.jogadores, info.capacidade);
        let d = measure_text(&txt, None, 13, 1.0);
        ui::texto(
            10.0 + l - d.width - 10.0,
            50.0,
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
    let mut y = screen_height() - 14.0 - chat.len() as f32 * 18.0;
    for linha in chat {
        ui::texto(14.0, y, linha, 16, Color::new(0.82, 0.82, 0.84, 1.0));
        y += 18.0;
    }

    // ── Sair ──
    // Sem isto so' da' pra trocar de conta fechando o processo. Fica embaixo
    // do cartao de localizacao, longe do centro onde se clica pra mirar.
    ui::botao(Rect::new(10.0, a + 18.0, 92.0, 26.0), "sair", true)
}

pub fn draw_status(texto: &str) {
    ui::texto(24.0, 48.0, texto, 24, ui::OURO);
}

pub fn draw_error(por_que: &str) {
    ui::texto(24.0, 48.0, "falhou", 32, RED);
    ui::texto(24.0, 84.0, por_que, 20, WHITE);
}
