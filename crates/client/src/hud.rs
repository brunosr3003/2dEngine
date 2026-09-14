//! HUD do mundo, no molde do MIR4: onde estou, quao cheio esta, o que o
//! servidor disse, e os botoes de acao. Posicao de tudo em `hud_layout`.
//!
//! Num jogo com servidor/canal/zona, "onde eu estou" deixa de ser obvio — o
//! mesmo personagem aparece em lugares diferentes conforme o canal, e a
//! lotacao muda o que da' pra fazer ali. Por isso isso fica na tela, nao num
//! menu.

use macroquad::prelude::*;
use std::f32::consts::PI;

use crate::hud_estilo as estilo;
use crate::hud_layout::{self as layout, Zonas};
use crate::map::Map;

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

fn mouse() -> Vec2 {
    Vec2::from(mouse_position())
}

/// Area e canal (topo direito), o chat (canto inferior esquerdo) e o
/// diagnostico com F3. Devolve `true` no quadro em que o NOME DA ZONA foi
/// clicado — e' o que abre o Mapa (MIR4: tocar no nome da area).
pub fn draw_hud(
    z: &Zonas, info: &Info, rede: &Rede, map: Option<&Map>, tick: u32, ents: usize,
    pedacos: usize, vivos: usize, pos: Vec2, chat: &[String],
) -> bool {
    let r = z.area;
    estilo::painel(r);
    let sobre = r.contains(mouse());
    let zona = if info.zona.is_empty() { map.map_or("Explorando", |m| m.name.as_str()) } else { &info.zona };
    let cor = if sobre { estilo::OURO } else { estilo::TEXTO };
    estilo::texto_ajustado(zona, r.x + 12.0, r.y + r.h * 0.45, r.w - 100.0, 18, cor);
    let onde = if info.realm.is_empty() { "Mundo aberto".into() } else { format!("{}  ·  CH {}", info.realm, info.canal) };
    estilo::texto_ajustado(&onde, r.x + 12.0, r.y + r.h * 0.84, r.w - 100.0, 12, estilo::SUAVE);
    draw_circle(r.x + r.w - 70.0, r.y + r.h * 0.5 - 4.0, 3.0, rede.cor_ms());
    estilo::texto(r.x + r.w - 62.0, r.y + r.h * 0.5, &format!("{:.0} ms", rede.ms), 12, estilo::SUAVE);
    if sobre {
        estilo::texto(r.x + r.w - 62.0, r.y + r.h * 0.84, "mapa", 11, estilo::OURO);
    }

    let cr = z.chat;
    estilo::painel(cr);
    estilo::texto(cr.x + 12.0, cr.y + 19.0, "MUNDO", 11, estilo::OURO);
    estilo::texto(cr.x + 77.0, cr.y + 19.0, "Combate e mensagens", 11, estilo::SUAVE);
    let linhas = (((cr.h - 30.0) / 18.0).floor() as usize).max(2);
    for (i, linha) in chat.iter().rev().take(linhas).collect::<Vec<_>>().into_iter().rev().enumerate() {
        estilo::texto_ajustado(linha, cr.x + 12.0, cr.y + 40.0 + i as f32 * 18.0, cr.w - 24.0, 13, estilo::TEXTO);
    }
    if chat.is_empty() {
        estilo::texto(cr.x + 12.0, cr.y + 45.0, "Sua aventura continua.", 13, estilo::SUAVE);
    }
    // Diagnostico sob demanda: nao disputa espaco com vida e alvo.
    if is_key_down(KeyCode::F3) {
        let d = Rect::new(z.rastreador.x, z.rastreador.y + z.rastreador.h + 8.0, 430.0, 55.0);
        estilo::painel(d);
        estilo::texto(d.x + 10.0, d.y + 21.0, &format!("{:.0} fps · tick {tick} · {ents} entidades · {pedacos}/{vivos} terreno", get_fps()), 13, estilo::SUAVE);
        estilo::texto(d.x + 10.0, d.y + 42.0, &format!("{:.0}, {:.0} · {:.1} KB/s · {:.0} MB/h · {}/{} online", pos.x, pos.y, rede.kbs, rede.mb_por_hora(), info.jogadores, info.capacidade), 13, estilo::SUAVE);
    }
    sobre && is_mouse_button_pressed(MouseButton::Left)
}

// ═══════════════════════════════════════════════════════════════════════
//  TOPO DIREITO — os atalhos de uso diario e o MENU
// ═══════════════════════════════════════════════════════════════════════

/// O que foi clicado no topo direito.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Topo {
    Bolsa,
    Missoes,
    Grupo,
    Avisos,
    Menu,
}

const VERMELHO: Color = Color::new(0.92, 0.22, 0.20, 1.0);

/// O ponto vermelho de "ha' algo a fazer" (MIR4).
pub fn selo(r: Rect) {
    draw_circle(r.x + r.w - 5.0, r.y + 5.0, 5.5, Color::new(0.0, 0.0, 0.0, 0.6));
    draw_circle(r.x + r.w - 5.0, r.y + 5.0, 4.5, VERMELHO);
}

/// Pictogramas do topo e do menu: 0 mochila, 1 pergaminho, 2 grupo, 3 sino,
/// 4 menu (tres riscos). Vetor proprio, nenhuma arte de outro jogo.
pub fn pictograma(i: usize, c: Vec2, s: f32, cor: Color) {
    let linha = |a: Vec2, b: Vec2| draw_line(a.x, a.y, b.x, b.y, (s * 0.14).max(1.5), cor);
    match i {
        0 => {
            draw_rectangle_lines(c.x - s * 0.8, c.y - s * 0.35, s * 1.6, s * 1.25, (s * 0.14).max(1.5), cor);
            estilo::arco(c - vec2(0.0, s * 0.35), s * 0.45, PI, 0.5, (s * 0.14).max(1.5), cor);
            linha(c + vec2(-s * 0.5, s * 0.25), c + vec2(s * 0.5, s * 0.25));
        }
        1 => {
            draw_rectangle_lines(c.x - s * 0.6, c.y - s * 0.85, s * 1.2, s * 1.7, (s * 0.14).max(1.5), cor);
            for k in 0..3 {
                let y = c.y - s * 0.4 + k as f32 * s * 0.4;
                linha(vec2(c.x - s * 0.35, y), vec2(c.x + s * 0.35, y));
            }
        }
        2 => {
            for (dx, r) in [(-0.55, 0.26), (0.55, 0.26), (0.0, 0.34)] {
                draw_circle_lines(c.x + dx * s, c.y - s * 0.3, r * s, (s * 0.12).max(1.2), cor);
                estilo::arco(c + vec2(dx * s, s * 0.55), r * s * 1.6, PI, 0.5, (s * 0.12).max(1.2), cor);
            }
        }
        3 => {
            estilo::arco(c + vec2(0.0, -s * 0.1), s * 0.55, PI, 0.5, (s * 0.14).max(1.5), cor);
            linha(c + vec2(-s * 0.55, -s * 0.1), c + vec2(-s * 0.7, s * 0.55));
            linha(c + vec2(s * 0.55, -s * 0.1), c + vec2(s * 0.7, s * 0.55));
            linha(c + vec2(-s * 0.8, s * 0.55), c + vec2(s * 0.8, s * 0.55));
            draw_circle(c.x, c.y + s * 0.8, s * 0.14, cor);
        }
        _ => {
            for k in -1..=1 {
                let y = c.y + k as f32 * s * 0.45;
                linha(vec2(c.x - s * 0.7, y), vec2(c.x + s * 0.7, y));
            }
        }
    }
}

fn dica(r: Rect, texto: &str) {
    let w = estilo::medir(texto, 13) + 16.0;
    let x = (r.center().x - w * 0.5).clamp(4.0, screen_width() - w - 4.0);
    let c = Rect::new(x, r.y + r.h + 4.0, w, 24.0);
    estilo::painel(c);
    estilo::texto(c.x + 8.0, c.y + 17.0, texto, 13, estilo::TEXTO);
}

/// Icones Bolsa/Missoes/Grupo/Avisos e o botao ≡ MENU. So' clique: nenhum
/// deles tem tecla (docs/HUD.md 2.5).
pub fn draw_topo(z: &Zonas, selo_missoes: bool, selo_menu: bool) -> Option<Topo> {
    let m = mouse();
    let clique = is_mouse_button_pressed(MouseButton::Left);
    let mut saida = None;
    let (nomes, alvos) = (["Bolsa", "Missões", "Grupo", "Avisos"], [Topo::Bolsa, Topo::Missoes, Topo::Grupo, Topo::Avisos]);
    let mut tooltip = None;
    for (i, r) in z.icones.iter().enumerate() {
        let sobre = r.contains(m);
        estilo::painel(*r);
        pictograma(i, r.center(), r.w * 0.30, if sobre { estilo::OURO } else { estilo::TEXTO });
        if i == 1 && selo_missoes {
            selo(*r);
        }
        if sobre {
            tooltip = Some((*r, nomes[i]));
            if clique {
                saida = Some(alvos[i]);
            }
        }
    }
    let r = z.menu;
    let sobre = r.contains(m);
    estilo::painel(r);
    let cor = if sobre { estilo::OURO } else { estilo::TEXTO };
    pictograma(4, vec2(r.center().x, r.y + r.h * 0.38), r.h * 0.26, cor);
    estilo::texto_centro(r.center().x, r.y + r.h * 0.88, "MENU", 11, cor);
    if selo_menu {
        selo(r);
    }
    if sobre && clique {
        saida = Some(Topo::Menu);
    }
    if let Some((r, t)) = tooltip {
        dica(r, t);
    }
    saida
}

// ═══════════════════════════════════════════════════════════════════════
//  CLUSTER DE COMBATE — o botao grande, a pocao e os slots rapidos
// ═══════════════════════════════════════════════════════════════════════

/// O botao grande (F). Sem alvo mostra "ALVO": escolhe o inimigo mais perto.
pub fn draw_atacar(z: &Zonas, tem_alvo: bool) -> bool {
    let r = z.atacar;
    let c = r.center();
    let raio = r.w * 0.5;
    let sobre = c.distance(mouse()) <= raio;
    draw_circle(c.x, c.y + 4.0, raio + 4.0, Color::new(0.0, 0.0, 0.0, 0.35));
    draw_circle(c.x, c.y, raio, estilo::FUNDO);
    let cor = if tem_alvo { Color::new(1.0, 0.62, 0.36, 1.0) } else { estilo::OURO };
    for k in (1..=8).rev() {
        draw_circle(c.x, c.y, raio * 0.92 * k as f32 / 8.0, Color::new(cor.r, cor.g, cor.b, 0.025));
    }
    draw_circle_lines(c.x, c.y, raio, 2.0, if sobre { estilo::TEXTO } else { cor });
    draw_circle_lines(c.x, c.y, raio - 5.0, 1.0, Color::new(cor.r, cor.g, cor.b, 0.35));
    estilo::icone(1, c - vec2(0.0, raio * 0.12), raio * 0.42, cor);
    estilo::texto_centro(c.x, c.y + raio * 0.62, if tem_alvo { "ATACAR" } else { "ALVO" }, 12, cor);
    layout::chip(r, "F");
    sobre && is_mouse_button_pressed(MouseButton::Left)
}

/// Pocao de vida (C) e os slots rapidos 8/9/0 (mana, vigor, experiencia).
/// `qtd` na mesma ordem. Devolve o indice clicado (0 = pocao).
pub fn draw_rapidos(z: &Zonas, qtd: [u32; 4]) -> Option<usize> {
    let m = mouse();
    let clique = is_mouse_button_pressed(MouseButton::Left);
    let rects = [z.pocao, z.rapidos[0], z.rapidos[1], z.rapidos[2]];
    let cores = [
        Color::new(0.86, 0.22, 0.26, 1.0),
        Color::new(0.24, 0.50, 0.92, 1.0),
        Color::new(0.35, 0.80, 0.40, 1.0),
        Color::new(0.98, 0.80, 0.22, 1.0),
    ];
    let nomes = ["Poção de vida", "Poção de mana", "Poção de vigor", "Poção de experiência"];
    let teclas = ["C", "8", "9", "0"];
    let mut saida = None;
    let mut tooltip = None;
    for i in 0..4 {
        let r = rects[i];
        let sobre = r.contains(m);
        estilo::painel(r);
        let cor = if qtd[i] > 0 { cores[i] } else { Color::new(0.35, 0.36, 0.38, 1.0) };
        let c = r.center();
        let s = r.w * 0.22;
        draw_rectangle(c.x - s * 0.3, c.y - s * 1.35, s * 0.6, s * 0.6, Color::new(0.75, 0.80, 0.86, 1.0));
        draw_circle(c.x, c.y + s * 0.2, s, cor);
        draw_circle_lines(c.x, c.y + s * 0.2, s, 1.5, Color::new(0.0, 0.0, 0.0, 0.6));
        let t = qtd[i].to_string();
        estilo::texto(r.x + r.w - estilo::medir(&t, 13) - 5.0, r.y + r.h - 5.0, &t, 13, if qtd[i] > 0 { estilo::TEXTO } else { VERMELHO });
        layout::chip(r, teclas[i]);
        if sobre {
            tooltip = Some((r, if qtd[i] > 0 { nomes[i].to_string() } else { format!("{} · sem estoque", nomes[i]) }));
            if clique {
                saida = Some(i);
            }
        }
    }
    if let Some((r, t)) = tooltip {
        let w = estilo::medir(&t, 13) + 16.0;
        let x = (r.center().x - w * 0.5).clamp(4.0, screen_width() - w - 4.0);
        let c = Rect::new(x, r.y - 30.0, w, 24.0);
        estilo::painel(c);
        estilo::texto(c.x + 8.0, c.y + 17.0, &t, 13, estilo::TEXTO);
    }
    saida
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
        estilo::texto(x + dx, y + dy, s, tam, Color::new(0.0, 0.0, 0.0, 0.85));
    }
    estilo::texto(x, y, s, tam, cor);
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
        let largura = estilo::medir(t, tam);
        texto_contornado(t, r.x + (r.w - largura) * 0.5, r.y + r.h * 0.5 + tam as f32 * 0.35, tam, WHITE);
    }
}

/// Tamanho de fonte na escala `k`.
fn fonte(n: f32, k: f32) -> u16 {
    (n * k).round().clamp(8.0, 64.0) as u16
}

/// A ficha no canto superior esquerdo: o nivel num circulo, o nome, a vida,
/// a mana, o vigor e o poder abaixo do nivel. Desenhada no retangulo do
/// layout; `k` escala o desenho de referencia (306 px de largura).
pub fn draw_ficha(z: &Zonas, ficha: &mut Ficha, dt: f32, nome: &str, nivel: u32, hp: i32, hp_max: i32, mp_max: i32, vigor_max: i32, poder: Option<i32>) {
    let r = z.ficha;
    let k = r.w / 306.0;
    estilo::painel(r);
    let c = vec2(r.x + 38.0 * k, r.y + 40.0 * k);
    draw_circle(c.x, c.y, 28.0 * k, Color::new(0.075, 0.10, 0.14, 1.0));
    draw_circle_lines(c.x, c.y, 28.0 * k, 1.5, estilo::OURO);
    estilo::arco(c, 32.0 * k, -2.8, 0.39, 1.0, estilo::BORDA);
    estilo::arco(c, 32.0 * k, 0.35, 0.39, 1.0, estilo::BORDA);
    estilo::texto_centro(c.x, c.y - 7.0 * k, "LV", fonte(10.0, k), estilo::SUAVE);
    estilo::texto_centro(c.x, c.y + 14.0 * k, &nivel.to_string(), fonte(25.0, k), estilo::TEXTO);
    estilo::texto_centro(c.x, r.y + 88.0 * k, "PODER", fonte(9.0, k), estilo::SUAVE);
    let poder = poder.map(|v| crate::bolsa::milhar(v.max(0) as u64)).unwrap_or_else(|| "—".into());
    estilo::texto_centro(c.x, r.y + 105.0 * k, &poder, fonte(if poder.len() > 7 { 12.0 } else { 16.0 }, k), estilo::OURO);
    let bx = r.x + 80.0 * k;
    let bw = r.w - 94.0 * k;
    estilo::texto_ajustado(nome, bx, r.y + 24.0 * k, bw, fonte(19.0, k), estilo::TEXTO);
    let f_hp = (hp as f32 / hp_max.max(1) as f32).clamp(0.0, 1.0);
    ficha.hp_rastro = if f_hp >= ficha.hp_rastro { f_hp } else { (ficha.hp_rastro - dt * 0.35).max(f_hp) };
    let pulsa = if f_hp < 0.3 { 0.8 + 0.2 * (get_time() as f32 * 6.0).sin() } else { 1.0 };
    barra_de_recurso(Rect::new(bx, r.y + 35.0 * k, bw, 17.0 * k), f_hp, ficha.hp_rastro,
        Color::new(0.72 * pulsa, 0.17, 0.22, 1.0), Color::new(1.0, 0.55, 0.55, 0.28), Some(&format!("{hp} / {hp_max}")));
    let mp = ficha.mp.unwrap_or(mp_max);
    barra_de_recurso(Rect::new(bx, r.y + 59.0 * k, bw, 12.0 * k), mp as f32 / mp_max.max(1) as f32, 0.0,
        Color::new(0.16, 0.40, 0.69, 1.0), Color::new(0.55, 0.80, 1.0, 0.25), Some(&format!("{mp} / {mp_max}")));
    let vigor = ficha.vigor.unwrap_or(vigor_max);
    barra_de_recurso(Rect::new(bx, r.y + 79.0 * k, bw, 3.0 * k), vigor as f32 / vigor_max.max(1) as f32, 0.0, estilo::OURO, estilo::OURO, None);
    estilo::texto(bx, r.y + 102.0 * k, "VIGOR", fonte(10.0, k), estilo::SUAVE);
    estilo::texto(bx + 46.0 * k, r.y + 102.0 * k, &format!("{vigor}/{vigor_max}"), fonte(11.0, k), estilo::OURO);
}

/// O bonus da Pocao de Experiencia: icone com os minutos restantes, logo
/// abaixo de `abaixo_de`; o hover diz "+30% XP · 47 min". Sem bonus, nada.
pub fn draw_buff_xp(ate: i64, agora: i64, abaixo_de: Rect) {
    if ate <= agora {
        return;
    }
    let min = ((ate - agora) as f32 / 60.0).ceil() as i64;
    let r = Rect::new(abaixo_de.x, abaixo_de.y + abaixo_de.h + 6.0, 40.0, 40.0);
    estilo::painel(r);
    let c = r.center();
    draw_rectangle(c.x - 3.0, c.y - 15.0, 6.0, 8.0, Color::new(0.75, 0.82, 0.88, 1.0));
    draw_circle(c.x, c.y + 3.0, 11.0, Color::new(0.98, 0.80, 0.22, 1.0));
    estilo::texto_centro(c.x, c.y + 7.0, "XP", 11, Color::new(0.12, 0.09, 0.02, 1.0));
    estilo::texto(r.x + r.w + 6.0, c.y + 5.0, &format!("+{}% · {min} min", shared::BONUS_XP_PCT), 13, estilo::OURO);
    if r.contains(mouse()) {
        let dica = format!("+{}% XP · {min} min", shared::BONUS_XP_PCT);
        let w = estilo::medir(&dica, 14) + 20.0;
        let caixa = Rect::new(r.x + r.w + 6.0, r.y + r.h + 4.0, w, 30.0);
        estilo::painel(caixa);
        estilo::texto(caixa.x + 10.0, caixa.y + 20.0, &dica, 14, estilo::TEXTO);
    }
}

/// A experiencia: uma faixa fina na tela inteira, no pe', com a
/// porcentagem — o numero que o jogador de MMO olha a cada bicho.
pub fn draw_exp(z: &Zonas, ficha: &Ficha, nivel: u32) {
    let mult = if ficha.mult_xp > 0 { ficha.mult_xp } else { shared::DEFAULT_XP_MULTIPLIER };
    let base = shared::xp_for_level_with_mult(nivel, mult);
    let prox = shared::xp_for_level_with_mult(nivel + 1, mult);
    let f = if prox > base { (ficha.xp.saturating_sub(base)) as f32 / (prox - base) as f32 } else { 0.0 };
    let (w, h, y) = (z.exp.w, z.exp.h, z.exp.y);
    draw_rectangle(0.0, y, w, h, Color::new(0.06, 0.05, 0.07, 0.9));
    draw_rectangle(0.0, y, w * f.clamp(0.0, 1.0), h, estilo::OURO);
    draw_rectangle(0.0, y, w * f.clamp(0.0, 1.0), h * 0.4, Color::new(1.0, 0.88, 0.55, 0.5));
    for k in 1..10 {
        let x = w * k as f32 / 10.0;
        draw_line(x, y, x, y + h, 1.0, Color::new(0.0, 0.0, 0.0, 0.45));
    }
    let t = format!("EXP {:.2}%", (f * 100.0).clamp(0.0, 100.0));
    let largura = estilo::medir(&t, 13);
    texto_contornado(&t, (w - largura) * 0.5, y - 3.0, 13, Color::new(1.0, 0.9, 0.62, 1.0));
}

/// O alvo, no alto e ao centro: nivel, nome, a vida em barra larga e o X que
/// limpa a selecao. Devolve `true` no quadro em que o X foi clicado.
pub fn draw_alvo(z: &Zonas, nome: &str, nivel: u16, hp: u16, hp_max: u16, chefe: bool) -> bool {
    let r = z.alvo;
    let k = r.h / 69.0;
    estilo::painel(r);
    let cor = if chefe { Color::new(1.0, 0.64, 0.37, 1.0) } else { estilo::OURO };
    estilo::texto(r.x + 12.0, r.y + 16.0 * k, if chefe { "CHEFE" } else { "ALVO" }, fonte(10.0, k), cor);
    estilo::texto_ajustado(nome, r.x + 12.0, r.y + 36.0 * k, r.w - 110.0, fonte(17.0, k), estilo::TEXTO);
    estilo::texto(r.x + r.w - 88.0, r.y + 35.0 * k, &format!("Lv {nivel}"), fonte(13.0, k), cor);
    let f = hp as f32 / hp_max.max(1) as f32;
    barra_de_recurso(Rect::new(r.x + 12.0, r.y + 46.0 * k, r.w - 24.0, 12.0 * k), f, 0.0,
        Color::new(0.68, 0.17, 0.20, 1.0), Color::new(1.0, 0.6, 0.50, 0.25), Some(&format!("{hp} / {hp_max}")));
    let x = z.alvo_fechar();
    let sobre = x.contains(mouse());
    let corx = if sobre { estilo::TEXTO } else { estilo::SUAVE };
    let c = x.center();
    let d = x.w * 0.25;
    draw_line(c.x - d, c.y - d, c.x + d, c.y + d, 2.0, corx);
    draw_line(c.x - d, c.y + d, c.x + d, c.y - d, 2.0, corx);
    layout::chip(Rect::new(r.x + r.w - 60.0, r.y + r.h - 18.0, 0.0, 0.0), "Tab");
    sobre && is_mouse_button_pressed(MouseButton::Left)
}
