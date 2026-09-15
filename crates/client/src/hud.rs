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
    Diarias,
    Grupo,
    Avisos,
    Menu,
}

const VERMELHO: Color = Color::new(0.92, 0.22, 0.20, 1.0);

/// O ponto vermelho de "ha' algo a fazer" (MIR4).
pub fn selo(r: Rect) {
    estilo::badge(r);
}

/// Pictogramas do topo e do menu: 0 mochila, 1 pergaminho, 2 grupo, 3 sino,
/// 4 menu (tres riscos), 5 calendario (diarias). Vetor proprio, nenhuma arte
/// de outro jogo.
pub fn pictograma(i: usize, c: Vec2, s: f32, cor: Color) {
    // Arte do atlas (tools/icones/gerar_icones_ui.py); o vetor abaixo so' se
    // faltar o icone.
    const NOMES: [&str; 6] = ["bolsa", "missoes", "grupo", "avisos", "menu", "diarias"];
    if NOMES.get(i).is_some_and(|n| crate::icones_ui::ui(n, c, s * 2.3, cor)) {
        return;
    }
    let esp = (s * 0.14).max(1.5);
    let linha = |a: Vec2, b: Vec2| estilo::traco(a, b, esp, cor);
    match i {
        0 => {
            estilo::borda_arredondada(Rect::new(c.x - s * 0.8, c.y - s * 0.35, s * 1.6, s * 1.25), s * 0.28, esp, cor);
            estilo::arco(c - vec2(0.0, s * 0.35), s * 0.45, PI, 0.5, (s * 0.14).max(1.5), cor);
            linha(c + vec2(-s * 0.5, s * 0.25), c + vec2(s * 0.5, s * 0.25));
        }
        1 => {
            estilo::borda_arredondada(Rect::new(c.x - s * 0.6, c.y - s * 0.85, s * 1.2, s * 1.7), s * 0.2, esp, cor);
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
        5 => {
            // Calendario com visto: o que se faz todo dia.
            estilo::borda_arredondada(Rect::new(c.x - s * 0.8, c.y - s * 0.6, s * 1.6, s * 1.4), s * 0.22, esp, cor);
            linha(vec2(c.x - s * 0.8, c.y - s * 0.25), vec2(c.x + s * 0.8, c.y - s * 0.25));
            linha(vec2(c.x - s * 0.4, c.y - s * 0.85), vec2(c.x - s * 0.4, c.y - s * 0.45));
            linha(vec2(c.x + s * 0.4, c.y - s * 0.85), vec2(c.x + s * 0.4, c.y - s * 0.45));
            linha(vec2(c.x - s * 0.38, c.y + s * 0.28), vec2(c.x - s * 0.08, c.y + s * 0.55));
            linha(vec2(c.x - s * 0.08, c.y + s * 0.55), vec2(c.x + s * 0.45, c.y));
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
    estilo::tooltip(r, texto, false);
}

/// Icones Bolsa/Missoes/Diarias/Grupo/Avisos e o botao ≡ MENU. So' clique:
/// nenhum deles tem tecla (docs/HUD.md 2.5).
pub fn draw_topo(z: &Zonas, selo_missoes: bool, selo_diarias: bool, selo_menu: bool) -> Option<Topo> {
    let m = mouse();
    let clique = is_mouse_button_pressed(MouseButton::Left);
    let mut saida = None;
    let nomes = ["Bolsa", "Missões", "Diárias", "Grupo", "Avisos"];
    let alvos = [Topo::Bolsa, Topo::Missoes, Topo::Diarias, Topo::Grupo, Topo::Avisos];
    // Qual pictograma cada icone usa (o 4 e' o do MENU).
    let pictos = [0usize, 1, 5, 2, 3];
    let mut tooltip = None;
    for (i, r) in z.icones.iter().enumerate() {
        let sobre = r.contains(m);
        estilo::cartao(*r, sobre, false);
        pictograma(pictos[i], r.center(), r.w * 0.30, if sobre { estilo::ACENTO } else { estilo::TEXTO });
        if (i == 1 && selo_missoes) || (i == 2 && selo_diarias) {
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
    estilo::cartao(r, sobre, false);
    let cor = if sobre { estilo::ACENTO } else { estilo::TEXTO };
    pictograma(4, vec2(r.center().x, r.y + r.h * 0.38), r.h * 0.26, cor);
    estilo::texto_centro_forte(r.center().x, r.y + r.h * 0.88, "MENU", 10, cor);
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

/// A bateria no canto inferior esquerdo, sempre na tela: liga o modo
/// economia. `true` no clique.
pub fn draw_botao_economia(z: &Zonas) -> bool {
    let r = z.economia;
    let m = mouse();
    let sobre = r.contains(m);
    estilo::cartao(r, sobre, false);
    crate::economia::bateria(r.center() - vec2(1.0, 0.0), r.w * 0.24, if sobre { estilo::OURO } else { Color::new(0.45, 0.85, 0.52, 1.0) });
    if sobre {
        dica(r, "Economia de energia");
    }
    sobre && is_mouse_button_pressed(MouseButton::Left)
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
    let cor = if tem_alvo { Color::new(1.0, 0.62, 0.36, 1.0) } else { estilo::OURO };
    let e = estilo::estado(sobre, is_mouse_button_down(MouseButton::Left), false, false);
    estilo::botao_redondo(c, raio, cor, e, tem_alvo);
    if !crate::icones_ui::ui("atacar", c - vec2(0.0, raio * 0.12), raio * 0.92, cor) {
        estilo::icone(1, c - vec2(0.0, raio * 0.12), raio * 0.42, cor);
    }
    estilo::texto_centro_forte(c.x, c.y + raio * 0.62, if tem_alvo { "ATACAR" } else { "ALVO" }, 12, cor);
    layout::chip(r, "F");
    sobre && is_mouse_button_pressed(MouseButton::Left)
}

/// Os quatro botoes de pocao, na ordem de `pocoes_auto`: vida (C), mana (8),
/// vigor (9), experiencia (0).
pub fn rects_rapidos(z: &Zonas) -> [Rect; 4] {
    [z.pocao, z.rapidos[0], z.rapidos[1], z.rapidos[2]]
}

/// A barra de itens: C, 8, 9 e 0, cada espaco com o consumivel que o jogador
/// escolheu (`itens`, 0 = vazio). `qtd` e `auto` na mesma ordem. So'
/// DESENHA: clique e arrasto sao do `barra`, com o mesmo gesto das skills.
pub fn draw_rapidos(
    z: &Zonas,
    itens: [u16; 4],
    qtd: [u32; 4],
    auto: [bool; 4],
    recarga: [Option<(f32, f32)>; 4],
    curando: [bool; 4],
    arrastando: Option<(usize, Vec2)>,
    nome: &dyn Fn(u16) -> String,
) {
    let m = mouse();
    let rects = rects_rapidos(z);
    let teclas = ["C", "8", "9", "0"];
    let mut tooltip = None;
    for i in 0..4 {
        let r = rects[i];
        estilo::slot(r, None, r.contains(m), false);
        if itens[i] == 0 {
            if !crate::icones_ui::ui("mais", r.center(), r.w * 0.40, estilo::SUAVE) {
                estilo::texto_centro(r.center().x, r.center().y + 8.0, "+", 22, estilo::SUAVE);
            }
        } else {
            let a = if qtd[i] > 0 { 1.0 } else { 0.35 };
            crate::bolsa::icone_do_item(Rect::new(r.x + r.w * 0.12, r.y + r.h * 0.08, r.w * 0.76, r.h * 0.76), itens[i], a);
            let t = qtd[i].to_string();
            estilo::texto(r.x + r.w - estilo::medir(&t, 13) - 5.0, r.y + r.h - 5.0, &t, 13, if qtd[i] > 0 { estilo::TEXTO } else { VERMELHO });
            if auto[i] {
                estilo::borda_arredondada(r, estilo::RAIO_PEQUENO + 1.0, 2.0, estilo::AUTO);
                estilo::texto_centro_forte(r.center().x, r.y + r.h + 12.0, "AUTO", 10, estilo::AUTO);
            }
            // Recarga do grupo: a sombra desce do topo e o numero conta.
            if let Some((resta, total)) = recarga[i] {
                let f = (resta / total.max(0.01)).clamp(0.0, 1.0);
                estilo::ret_arredondado(Rect::new(r.x, r.y, r.w, r.h * f), estilo::RAIO_PEQUENO + 1.0, Color::new(0.0, 0.0, 0.0, 0.55));
                let t = format!("{:.0}", resta.ceil());
                estilo::texto_sombra(r.center().x - estilo::medir_forte(&t, 18) * 0.5, r.center().y + 6.0, &t, 18, estilo::TEXTO, true);
            }
            // Cura correndo: borda verde pulsando.
            if curando[i] {
                let a = 0.5 + 0.5 * (get_time() as f32 * 6.0).sin();
                estilo::borda_arredondada(Rect::new(r.x - 3.0, r.y - 3.0, r.w + 6.0, r.h + 6.0), estilo::RAIO_PEQUENO + 4.0, 2.0, Color::new(0.35, 0.95, 0.45, 0.4 + 0.5 * a));
            }
        }
        layout::chip(r, teclas[i]);
        match arrastando {
            Some((j, de)) if j == i => {
                let texto = if m.y > de.y + 20.0 { "↓ Solte: MANUAL" } else { "↑ Solte: AUTO" };
                estilo::texto_centro(r.center().x, r.y - 36.0, texto, 14, estilo::AUTO);
                draw_line(r.center().x, r.y - 6.0, r.center().x, r.y - 24.0, 2.0, estilo::AUTO);
            }
            None if r.contains(m) => {
                let t = if itens[i] == 0 {
                    "Espaço vazio · clique pra configurar".to_string()
                } else {
                    let base = if qtd[i] > 0 { nome(itens[i]) } else { format!("{} · sem estoque", nome(itens[i])) };
                    format!("{base} · {} · arraste ↑ AUTO / ↓ manual · botão direito configura", if auto[i] { "AUTO" } else { "manual" })
                };
                tooltip = Some((r, t));
            }
            _ => {}
        }
    }
    if let Some((r, t)) = tooltip {
        estilo::tooltip(r, &t, true);
    }
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
    estilo::texto_sombra(x, y, s, tam, cor, true);
}

/// Uma barra de recurso: pilula com trilho rebaixado, gradiente, brilho, o
/// rastro claro do que acabou de sair e o numero no meio (`estilo::barra`).
fn barra_de_recurso(r: Rect, f: f32, rastro: f32, cor: Color, _brilho: Color, texto: Option<&str>) {
    estilo::barra(r, f, rastro, cor, texto);
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
    estilo::botao_redondo(c, 28.0 * k, estilo::OURO, estilo::Estado::Normal, false);
    estilo::arco(c, 32.0 * k, -2.8, 0.39, 1.5, estilo::alfa(estilo::OURO, 0.35));
    estilo::arco(c, 32.0 * k, 0.35, 0.39, 1.5, estilo::alfa(estilo::OURO, 0.35));
    estilo::texto_centro_forte(c.x, c.y - 7.0 * k, "LV", fonte(10.0, k), estilo::SUAVE);
    estilo::texto_centro_forte(c.x, c.y + 14.0 * k, &nivel.to_string(), fonte(25.0, k), estilo::TEXTO);
    estilo::texto_centro_forte(c.x, r.y + 88.0 * k, "PODER", fonte(9.0, k), estilo::SUAVE);
    let poder = poder.map(|v| crate::bolsa::milhar(v.max(0) as u64)).unwrap_or_else(|| "—".into());
    estilo::texto_centro_forte(c.x, r.y + 105.0 * k, &poder, fonte(if poder.len() > 7 { 12.0 } else { 16.0 }, k), estilo::OURO);
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

/// Os buffs de pocao lado a lado, logo abaixo de `abaixo_de`: Experiencia,
/// Fortuna e Sorte, cada um com os minutos que faltam; o hover explica.
/// Buff inativo nao aparece.
pub fn draw_buffs(xp_ate: i64, fortuna_ate: i64, sorte_ate: i64, agora: i64, abaixo_de: Rect, curas: [f32; 3]) {
    let lista = [
        (xp_ate, "XP", Color::new(0.98, 0.80, 0.22, 1.0), format!("+{}% XP", shared::BONUS_XP_PCT)),
        (fortuna_ate, "$", Color::new(0.95, 0.55, 0.15, 1.0), format!("+{}% ouro e cobre de bicho", shared::BONUS_FORTUNA_PCT)),
        (sorte_ate, "S", Color::new(0.70, 0.45, 0.95, 1.0), format!("+{}% chance de drop", shared::BONUS_SORTE_PCT)),
    ];
    let mut x = abaixo_de.x;
    let mut dica_ativa = None;
    for (ate, sigla, cor, dica) in lista {
        if ate <= agora {
            continue;
        }
        let min = ((ate - agora) as f32 / 60.0).ceil() as i64;
        let r = Rect::new(x, abaixo_de.y + abaixo_de.h + 6.0, 40.0, 40.0);
        estilo::cartao(r, r.contains(mouse()), false);
        let c = r.center();
        estilo::ret_arredondado(Rect::new(c.x - 3.0, c.y - 15.0, 6.0, 8.0), 2.0, Color::new(0.75, 0.82, 0.88, 1.0));
        draw_circle(c.x, c.y + 3.0, 11.0, cor);
        estilo::texto_centro(c.x, c.y + 7.0, sigla, 11, Color::new(0.12, 0.09, 0.02, 1.0));
        estilo::texto_centro(c.x, r.y + r.h + 12.0, &format!("{min}m"), 11, estilo::OURO);
        if r.contains(mouse()) {
            dica_ativa = Some((r, format!("{dica} · {min} min")));
        }
        x += 48.0;
    }
    // Curas de pocao correndo (vida, mana, vigor), com os segundos que faltam.
    let curando = [
        ("+V", Color::new(0.85, 0.25, 0.28, 1.0), "Curando vida"),
        ("+M", Color::new(0.25, 0.50, 0.90, 1.0), "Recuperando mana"),
        ("+E", Color::new(0.95, 0.75, 0.25, 1.0), "Recuperando vigor"),
    ];
    for (g, (sigla, cor, nome)) in curando.into_iter().enumerate() {
        let s = curas[g];
        if s <= 0.0 {
            continue;
        }
        let r = Rect::new(x, abaixo_de.y + abaixo_de.h + 6.0, 40.0, 40.0);
        estilo::cartao(r, r.contains(mouse()), false);
        let c = r.center();
        draw_circle(c.x, c.y, 13.0, cor);
        estilo::texto_centro(c.x, c.y + 5.0, sigla, 12, Color::new(0.08, 0.06, 0.05, 1.0));
        estilo::texto_centro(c.x, r.y + r.h + 12.0, &format!("{:.0}s", s.ceil()), 11, estilo::OURO);
        if r.contains(mouse()) {
            dica_ativa = Some((r, format!("{nome} · {:.0} s", s.ceil())));
        }
        x += 48.0;
    }
    if let Some((r, dica)) = dica_ativa {
        estilo::tooltip(r, &dica, false);
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
    draw_rectangle(0.0, y, w, h, estilo::FUNDO_BAIXO);
    let fw = w * f.clamp(0.0, 1.0);
    if fw > 0.5 {
        estilo::ret_gradiente(Rect::new(0.0, y, fw, h), 0.0, estilo::clarear(estilo::OURO, 0.25), estilo::clarear(estilo::OURO, -0.15));
        draw_rectangle(0.0, y, fw, h * 0.35, Color::new(1.0, 1.0, 1.0, 0.18));
        draw_rectangle((fw - 2.0).max(0.0), y, 2.0, h, Color::new(1.0, 0.97, 0.85, 0.9));
    }
    for k in 1..10 {
        let x = w * k as f32 / 10.0;
        draw_line(x, y, x, y + h, 1.0, Color::new(0.0, 0.0, 0.0, 0.25));
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
    let cor = if chefe { Color::new(1.0, 0.64, 0.37, 1.0) } else { estilo::OURO };
    estilo::painel_destaque(r, cor);
    estilo::texto_forte(r.x + 12.0, r.y + 16.0 * k, if chefe { "CHEFE" } else { "ALVO" }, fonte(10.0, k), cor);
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
    if sobre {
        draw_circle(c.x, c.y, x.w * 0.48, estilo::alfa(estilo::FUNDO_ALTO, 0.9));
    }
    estilo::traco(vec2(c.x - d, c.y - d), vec2(c.x + d, c.y + d), 2.0, corx);
    estilo::traco(vec2(c.x - d, c.y + d), vec2(c.x + d, c.y - d), 2.0, corx);
    layout::chip(Rect::new(r.x + r.w - 60.0, r.y + r.h - 18.0, 0.0, 0.0), "Tab");
    sobre && is_mouse_button_pressed(MouseButton::Left)
}
