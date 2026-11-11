//! CHAT (`ServerMessage::ChatGlobal`, `shared::chat`).
//!
//! Always on screen: a see-through box at the bottom centre (`Zonas::chat`)
//! with the last lines and, under them, the channel button and the text field
//! (owner, 09/11/2026: "not a chat you have to open: it stays there, you just
//! tap to talk and pick the channel beside where you type"; "semi-transparent,
//! it must not cover the screen, and when tapped it is highlighted"). Tapping
//! the lines opens the full history, where a line can be muted or reported.
//!
//! Muting is local (this device, by name) and is remembered in
//! `<prefs>.silenciados`. Everything said is filtered and logged by the
//! server; the panel says so, which is also what the stores ask for.

use std::collections::{HashSet, VecDeque};

use macroquad::prelude::*;
use shared::protocol::ClientMessage;

#[allow(unused_imports)]
use crate::ui::Toque as _;
use crate::hud_estilo as estilo;

const BASE_W: f32 = 900.0;
const BASE_H: f32 = 420.0;
const GUARDADAS: usize = 120;
const LINHA: f32 = 30.0;

#[derive(Clone)]
struct Linha {
    id: i64,
    canal: u8,
    /// Whisper: who it was sent to.
    para: String,
    de: String,
    texto: String,
    staff: bool,
    sistema: bool,
}

#[derive(Default)]
pub struct ChatUi {
    pub aberto: bool,
    linhas: VecDeque<Linha>,
    texto: String,
    foco: bool,
    nao_lidas: u32,
    rolagem: crate::rolagem::Rolagem,
    no_fim: bool,
    selecionada: Option<usize>,
    silenciados: Option<HashSet<String>>,
    denunciadas: HashSet<i64>,
    /// The channel the next line goes to (`shared::chat::CANAL_*`).
    canal: u8,
    /// When the player last sent: system answers right after show here too.
    enviou_em: f64,
    /// Whisper channel: who the next line goes to.
    alvo: String,
    /// Where the box was drawn this frame (`caixa`), for `entrada`.
    ultima: Option<Rect>,
    /// Who may whisper me (`shared::chat::SUSSURRO_*`), remembered here.
    modo_sussurro: Option<u8>,
    /// The setting changed: the caller sends `ModoSussurro`.
    pub modo_mudou: bool,
    /// The panel shows the muted players (with Unmute) instead of the talk.
    vendo_silenciados: bool,
}

/// The box's parts, from its rectangle.
struct Geometria {
    area: Rect,
    canal: Rect,
    campo: Rect,
    enviar: Rect,
    lh: f32,
    fonte: u16,
}

impl Geometria {
    fn de(r: Rect, ativo: bool) -> Self {
        let ih = (r.h * 0.34).clamp(26.0, 46.0).min(r.h * 0.5);
        let lh = (r.h * 0.20).clamp(15.0, 26.0);
        let fonte = (lh * 0.74).round().clamp(11.0, 18.0) as u16;
        let area = Rect::new(r.x, r.y, r.w, r.h - ih - 3.0);
        let linha_in = Rect::new(r.x, r.y + r.h - ih, r.w, ih);
        let cw = (r.w * 0.22).clamp(64.0, 110.0);
        let canal = Rect::new(linha_in.x, linha_in.y, cw, linha_in.h);
        let sw = if ativo { (r.w * 0.18).clamp(56.0, 96.0) } else { 0.0 };
        let enviar = Rect::new(linha_in.x + linha_in.w - sw, linha_in.y, sw, linha_in.h);
        let campo = Rect::new(canal.x + canal.w + 4.0, linha_in.y, linha_in.w - cw - sw - if ativo { 8.0 } else { 4.0 }, linha_in.h);
        Self { area, canal, campo, enviar, lh, fonte }
    }
}

fn arquivo_modo() -> Option<std::path::PathBuf> {
    crate::lembranca::caminho().map(|p| p.with_extension("sussurro"))
}

/// Gold for chat TEXT, kept away from `estilo::OURO`: from size 18 the HUD
/// style draws anything close to that gold in the title font (serif, wider),
/// which ran names into the message on the bigger typing box.
const OURO_DO_CHAT: Color = Color::new(0.99, 0.83, 0.40, 1.0);

fn cor_do_canal(c: u8) -> Color {
    match c {
        shared::chat::CANAL_ILHA => Color::new(0.45, 0.90, 0.55, 1.0),
        shared::chat::CANAL_GRUPO => Color::new(0.45, 0.75, 1.0, 1.0),
        shared::chat::CANAL_SUSSURRO => Color::new(1.0, 0.55, 0.85, 1.0),
        _ => OURO_DO_CHAT,
    }
}

fn sigla_do_canal(c: u8) -> &'static str {
    match c {
        shared::chat::CANAL_ILHA => "[I]",
        shared::chat::CANAL_GRUPO => "[P]",
        shared::chat::CANAL_SUSSURRO => "[W]",
        _ => "[G]",
    }
}

fn arquivo() -> Option<std::path::PathBuf> {
    crate::lembranca::caminho().map(|p| p.with_extension("silenciados"))
}

impl ChatUi {
    fn silenciados(&mut self) -> &mut HashSet<String> {
        self.silenciados.get_or_insert_with(|| {
            arquivo()
                .and_then(|p| std::fs::read_to_string(p).ok())
                .map(|t| t.lines().map(|l| l.trim().to_lowercase()).filter(|l| !l.is_empty()).collect())
                .unwrap_or_default()
        })
    }

    fn grava_silenciados(&mut self) {
        let t = self.silenciados().iter().cloned().collect::<Vec<_>>().join("\n");
        if let Some(p) = arquivo() {
            let _ = std::fs::write(p, t);
        }
    }

    /// The text box has the keyboard (the on-screen keyboard follows this).
    pub fn foco(&self) -> bool {
        self.foco
    }

    pub fn modo_sussurro(&mut self) -> u8 {
        *self.modo_sussurro.get_or_insert_with(|| {
            arquivo_modo()
                .and_then(|p| std::fs::read_to_string(p).ok())
                .and_then(|t| t.trim().parse().ok())
                .unwrap_or(shared::chat::SUSSURRO_TODOS)
        })
    }

    fn troca_modo_sussurro(&mut self) {
        let m = (self.modo_sussurro() + 1) % 3;
        self.modo_sussurro = Some(m);
        if let Some(p) = arquivo_modo() {
            let _ = std::fs::write(p, m.to_string());
        }
        self.modo_mudou = true;
    }

    /// Whisper someone: the box switches to the whisper channel, aimed at them.
    pub fn sussurrar(&mut self, nome: &str) {
        self.canal = shared::chat::CANAL_SUSSURRO;
        self.alvo = nome.to_string();
        self.foco = true;
    }

    /// Enter on a keyboard: straight to typing in the box.
    pub fn focar(&mut self) {
        self.foco = true;
    }

    /// Did the player send something in the last few seconds?
    pub fn acabou_de_enviar(&self) -> bool {
        get_time() - self.enviou_em < 4.0
    }

    pub fn abrir(&mut self) {
        self.aberto = true;
        self.foco = true;
        self.no_fim = true;
        self.nao_lidas = 0;
        self.selecionada = None;
    }

    pub fn fechar(&mut self) {
        self.aberto = false;
        self.foco = false;
        self.selecionada = None;
        self.vendo_silenciados = false;
    }

    fn guarda(&mut self, l: Linha) {
        self.linhas.push_back(l);
        while self.linhas.len() > GUARDADAS {
            self.linhas.pop_front();
            self.selecionada = None;
        }
        if !self.aberto {
            self.nao_lidas = self.nao_lidas.saturating_add(1);
        }
    }

    /// A global line. `false` = muted here (the caller shows nothing).
    pub fn receber(&mut self, id: i64, canal: u8, para: String, de: String, texto: String, staff: bool) -> bool {
        if !staff && self.silenciados().contains(&de.to_lowercase()) {
            return false;
        }
        self.guarda(Linha { id, canal, para, de, texto, staff, sistema: false });
        true
    }

    /// A system answer (slow down, muted, report sent…), shown in the panel too.
    pub fn sistema(&mut self, texto: String) {
        let aberto = self.aberto;
        self.guarda(Linha { id: 0, canal: 0, para: String::new(), de: String::new(), texto, staff: false, sistema: true });
        if !aberto {
            self.nao_lidas = self.nao_lidas.saturating_sub(1);
        }
    }

    /// Types into the box from what the keyboard gave this frame; `true` on Enter.
    fn digitar(&mut self, digitado: &[char]) -> bool {
        let mut manda = false;
        for &c in digitado {
            match c {
                '\u{8}' => {
                    self.texto.pop();
                }
                '\r' | '\n' => manda = true,
                c if !c.is_control() && self.texto.chars().count() < shared::chat::MAX_CHARS => self.texto.push(c),
                _ => {}
            }
        }
        if is_key_pressed(KeyCode::Backspace) && !digitado.contains(&'\u{8}') {
            self.texto.pop();
        }
        manda || is_key_pressed(KeyCode::Enter) || is_key_pressed(KeyCode::KpEnter)
    }

    fn enviar(&mut self) -> Option<ClientMessage> {
        let t = std::mem::take(&mut self.texto);
        if t.trim().is_empty() {
            return None;
        }
        self.enviou_em = get_time();
        self.no_fim = true;
        if self.canal == shared::chat::CANAL_SUSSURRO {
            // No name yet: the first word is the name ("Lyra see you there").
            if self.alvo.is_empty() {
                let t2 = t.trim().trim_start_matches('@');
                let Some((nome, resto)) = t2.split_once(char::is_whitespace) else {
                    self.texto = t;
                    self.sistema("Whisper: type the player's name, a space, then the message.".into());
                    return None;
                };
                self.alvo = nome.to_string();
                return Some(ClientMessage::ChatSussurro { para: self.alvo.clone(), text: resto.trim().to_string() });
            }
            return Some(ClientMessage::ChatSussurro { para: self.alvo.clone(), text: t });
        }
        Some(ClientMessage::ChatCanal { canal: self.canal, text: t })
    }

    /// The name on a whisper line that is not me: who to answer.
    fn outro_do_sussurro(l: &Linha, eu: &str) -> String {
        if l.de.eq_ignore_ascii_case(eu) { l.para.clone() } else { l.de.clone() }
    }

    /// Where the box sits while typing on a phone: at the TOP, bigger and over
    /// everything — at the bottom the on-screen keyboard covered it (owner,
    /// 10/11/2026: "the chat is quite bad to use").
    fn em_cima(r: Rect) -> Rect {
        let seguro = crate::hud_layout::tela_segura();
        let w = (r.w * 1.6).min(seguro.w - 16.0);
        let h = (r.h * 1.9).min(seguro.h * 0.45);
        Rect::new(seguro.center().x - w * 0.5, seguro.y + 6.0, w, h)
    }

    fn digitando_no_celular(&self) -> bool {
        self.foco && crate::nativo::modo_celular()
    }

    /// THE BOX, drawn FIRST in the HUD so every other HUD sits in front of it.
    /// Taps and typing are handled at the end of the frame (`entrada`), once
    /// every window drawn over it is known.
    pub fn caixa(&mut self, r: Rect, eu: &str) {
        estilo::coberturas_limpar();
        if r.w < 40.0 || r.h < 40.0 {
            self.ultima = None;
            return;
        }
        self.ultima = Some(r);
        if !self.digitando_no_celular() {
            self.desenha_caixa(r, eu);
        }
    }

    /// End of the frame: the box's taps and typing. A tap on a window drawn
    /// over the box is that window's, never the chat's.
    pub fn entrada(&mut self, digitado: &[char], eu: &str) -> Vec<ClientMessage> {
        let mut saida = Vec::new();
        let Some(base) = self.ultima.take() else {
            // The box was not drawn (a big window is open): no keyboard for it.
            self.foco = false;
            return saida;
        };
        let em_cima = self.digitando_no_celular();
        let r = if em_cima { Self::em_cima(base) } else { base };
        if em_cima {
            self.desenha_caixa(r, eu);
        }
        let g = Geometria::de(r, self.foco);
        let m = Vec2::from(mouse_position());
        if self.foco && self.digitar(digitado) {
            saida.extend(self.enviar());
        }
        if !crate::foco::clique() {
            return saida;
        }
        // Over a window: the window's tap (it only takes the keyboard away).
        if !em_cima && estilo::coberto(m) {
            self.foco = false;
            return saida;
        }
        if g.canal.toca(m) {
            self.canal = (self.canal + 1) % 4;
            self.alvo.clear();
            self.foco = true;
        } else if self.foco && g.enviar.toca(m) {
            saida.extend(self.enviar());
        } else if g.campo.toca(m) {
            self.foco = true;
        } else if g.area.contains(m) {
            // A whisper line: answer it right here. Any other: the full
            // history (mute / report / whisper there).
            let n = ((g.area.h - 2.0) / g.lh).floor().max(1.0) as usize;
            let de_baixo = ((g.area.y + g.area.h - m.y) / g.lh).floor().max(0.0) as usize;
            let tocada = self.linhas.iter().rev().take(n).nth(de_baixo).cloned();
            match tocada {
                Some(l) if l.canal == shared::chat::CANAL_SUSSURRO && !l.sistema => {
                    let outro = Self::outro_do_sussurro(&l, eu);
                    self.sussurrar(&outro);
                }
                _ => self.abrir(),
            }
        } else if self.foco {
            self.foco = false;
        }
        saida
    }

    fn desenha_caixa(&self, r: Rect, eu: &str) {
        let ativo = self.foco;
        let g = Geometria::de(r, ativo);
        let (area, lh, fonte) = (g.area, g.lh, g.fonte);
        estilo::no_painel(1.0, || {
            // See-through at rest; solid with a gold edge while typing.
            let fundo = if ativo { 0.78 } else { 0.22 };
            draw_rectangle(r.x, r.y, r.w, r.h, Color::new(0.04, 0.03, 0.03, fundo));
            if ativo {
                draw_rectangle_lines(r.x, r.y, r.w, r.h, 2.0, estilo::OURO);
            }
            // The last lines, newest at the bottom.
            let n = ((area.h - 2.0) / lh).floor().max(1.0) as usize;
            let alfa = if ativo { 1.0 } else { 0.88 };
            let vis: Vec<&Linha> = self.linhas.iter().rev().take(n).collect();
            for (i, l) in vis.iter().enumerate() {
                let base = area.y + area.h - (i as f32) * lh - lh * 0.28;
                let x0 = area.x + 6.0;
                let largura = area.w - 12.0;
                if l.sistema {
                    estilo::texto_sombra(x0, base, &l.texto, fonte, estilo::alfa(estilo::SUAVE, alfa), false);
                    continue;
                }
                let tag = sigla_do_canal(l.canal);
                let cor_tag = estilo::alfa(cor_do_canal(l.canal), alfa);
                estilo::texto_sombra(x0, base, tag, fonte, cor_tag, true);
                let mut x = x0 + estilo::medir_forte(tag, fonte) + 4.0;
                let (nome, cor_nome) = if l.canal == shared::chat::CANAL_SUSSURRO {
                    if l.de.eq_ignore_ascii_case(eu) {
                        (format!("To {}:", l.para), cor_do_canal(l.canal))
                    } else {
                        (format!("From {}:", l.de), cor_do_canal(l.canal))
                    }
                } else if l.staff {
                    (format!("[Staff] {}:", l.de), OURO_DO_CHAT)
                } else if l.de.eq_ignore_ascii_case(eu) {
                    (format!("{}:", l.de), estilo::VERDE)
                } else {
                    (format!("{}:", l.de), Color::new(0.80, 0.88, 1.0, 1.0))
                };
                estilo::texto_sombra(x, base, &nome, fonte, estilo::alfa(cor_nome, alfa), true);
                x += estilo::medir_forte(&nome, fonte) + 4.0;
                // Cut at the box edge (one line each; the history has it all).
                let mut t = l.texto.clone();
                while estilo::medir(&t, fonte) > largura - (x - x0) && t.chars().count() > 1 {
                    t.pop();
                }
                if t.len() < l.texto.len() {
                    t.push('…');
                }
                estilo::texto_sombra(x, base, &t, fonte, estilo::alfa(WHITE, alfa), false);
            }
            // ── channel button · field · send ──
            let (canal_r, campo, enviar_r) = (g.canal, g.campo, g.enviar);
            let cor = cor_do_canal(self.canal);
            draw_rectangle(canal_r.x, canal_r.y, canal_r.w, canal_r.h, Color::new(0.05, 0.04, 0.03, if ativo { 0.9 } else { 0.55 }));
            draw_rectangle_lines(canal_r.x, canal_r.y, canal_r.w, canal_r.h, 1.5, estilo::alfa(cor, 0.9));
            // Name + a small drawn triangle (the HUD font has no ▾).
            let rotulo_w;
            let nome_c = if self.canal == shared::chat::CANAL_SUSSURRO && !self.alvo.is_empty() {
                rotulo_w = format!("W: {}", self.alvo);
                rotulo_w.as_str()
            } else {
                shared::chat::nome_do_canal(self.canal)
            };
            let nw = estilo::medir_forte(nome_c, fonte);
            let tri = fonte as f32 * 0.32;
            let x0c = canal_r.center().x - (nw + tri * 2.6) * 0.5;
            estilo::texto_forte(x0c, canal_r.center().y + fonte as f32 * 0.38, nome_c, fonte, cor);
            let tc = vec2(x0c + nw + tri * 1.6, canal_r.center().y + tri * 0.2);
            draw_triangle(tc + vec2(-tri, -tri * 0.6), tc + vec2(tri, -tri * 0.6), tc + vec2(0.0, tri * 0.6), cor);
            draw_rectangle(campo.x, campo.y, campo.w, campo.h, Color::new(0.05, 0.04, 0.03, if ativo { 0.9 } else { 0.45 }));
            if ativo {
                draw_rectangle_lines(campo.x, campo.y, campo.w, campo.h, 1.5, estilo::alfa(estilo::OURO, 0.8));
            }
            let ty = campo.center().y + fonte as f32 * 0.38;
            if self.texto.is_empty() && self.canal == shared::chat::CANAL_SUSSURRO && self.alvo.is_empty() {
                estilo::texto(campo.x + 8.0, ty, "Name, then the message…", fonte, estilo::alfa(estilo::SUAVE, 0.9));
            } else if self.texto.is_empty() && !ativo {
                estilo::texto(campo.x + 8.0, ty, "Tap to chat…", fonte, estilo::alfa(estilo::SUAVE, 0.9));
            } else {
                let cursor = if ativo && (get_time() * 2.0) as i32 % 2 == 0 { "|" } else { "" };
                let mut mostrado = format!("{}{cursor}", self.texto);
                while estilo::medir(&mostrado, fonte) > campo.w - 14.0 && mostrado.chars().count() > 1 {
                    mostrado.remove(0);
                }
                estilo::texto(campo.x + 8.0, ty, &mostrado, fonte, WHITE);
            }
            if ativo {
                let pode = !self.texto.trim().is_empty();
                estilo::botao(enviar_r, "Send", estilo::estado_de(enviar_r, !pode, false), pode);
            }
        });
    }

    /// The panel. Returns what to send to the server.
    pub fn desenha(&mut self, digitado: &[char], eu: &str) -> Vec<ClientMessage> {
        if !self.aberto {
            return Vec::new();
        }
        let seguro = crate::hud_layout::tela_segura();
        let k = estilo::escala_do_painel(BASE_W, BASE_H)
            .min((seguro.w - 8.0) / BASE_W)
            .min((seguro.h - 8.0) / BASE_H);
        estilo::no_painel(k, || self.desenha_na_escala(digitado, eu, k, seguro))
    }

    fn desenha_na_escala(&mut self, digitado: &[char], eu: &str, k: f32, seguro: Rect) -> Vec<ClientMessage> {
        let mut saida = Vec::new();
        let o = vec2(seguro.center().x - BASE_W * k * 0.5, seguro.center().y - BASE_H * k * 0.5);
        let r = |x: f32, y: f32, w: f32, h: f32| Rect::new(o.x + x * k, o.y + y * k, w * k, h * k);
        crate::hud_layout::escurece(0.45);
        let p = r(0.0, 0.0, BASE_W, BASE_H);
        estilo::painel_destaque(p, estilo::OURO);
        let m = Vec2::from(mouse_position());
        let clicou = crate::foco::clique();
        estilo::texto_forte(p.x + 22.0 * k, p.y + 36.0 * k, "Global chat", 22, estilo::OURO);
        estilo::texto(
            p.x + 22.0 * k,
            p.y + 58.0 * k,
            "Everyone online, on every island. Be respectful: the chat is logged and filtered.",
            12,
            estilo::SUAVE,
        );
        let fechar = r(BASE_W - 56.0, 10.0, 46.0, 46.0);
        estilo::botao(fechar, "X", estilo::estado_de(fechar, false, false), false);
        if clicou && fechar.toca(m) {
            self.fechar();
            return saida;
        }
        // Who may whisper me.
        let modo_r = r(BASE_W - 290.0, 12.0, 224.0, 42.0);
        let modo_txt = match self.modo_sussurro() {
            shared::chat::SUSSURRO_AMIGOS => "Whispers: friends",
            shared::chat::SUSSURRO_NINGUEM => "Whispers: nobody",
            _ => "Whispers: everyone",
        };
        estilo::botao(modo_r, modo_txt, estilo::estado_de(modo_r, false, false), false);
        if clicou && modo_r.toca(m) {
            self.troca_modo_sussurro();
        }
        // Who I muted, and the way back (owner, 10/11/2026: "make a way to
        // unmute in the game" — muting had no undo).
        let n_silenciados = self.silenciados().len();
        let silenciados_r = r(BASE_W - 450.0, 12.0, 150.0, 42.0);
        estilo::botao(
            silenciados_r,
            &format!("Muted ({n_silenciados})"),
            estilo::estado_de(silenciados_r, false, self.vendo_silenciados),
            false,
        );
        if clicou && silenciados_r.toca(m) {
            self.vendo_silenciados = !self.vendo_silenciados;
            self.selecionada = None;
            return saida;
        }
        if self.vendo_silenciados {
            let area = r(16.0, 70.0, BASE_W - 32.0, 282.0);
            draw_rectangle(area.x, area.y, area.w, area.h, Color::new(0.0, 0.0, 0.0, 0.25));
            let mut nomes: Vec<String> = self.silenciados().iter().cloned().collect();
            nomes.sort();
            if nomes.is_empty() {
                estilo::texto_centro(area.center().x, area.center().y, "You have not muted anyone.", 15, estilo::SUAVE);
            }
            let passo = 52.0 * k;
            let por_coluna = ((area.h - 8.0 * k) / passo).floor().max(1.0) as usize;
            let col_w = (area.w - 24.0 * k) / 2.0;
            let mut tirar = None;
            for (i, nome) in nomes.iter().enumerate().take(por_coluna * 2) {
                let (c, l) = (i / por_coluna, i % por_coluna);
                let x = area.x + 8.0 * k + c as f32 * (col_w + 8.0 * k);
                let y = area.y + 8.0 * k + l as f32 * passo;
                estilo::texto_ajustado(nome, x + 6.0 * k, y + 30.0 * k, col_w - 150.0 * k, 15, estilo::TEXTO);
                let b = Rect::new(x + col_w - 136.0 * k, y + 2.0 * k, 130.0 * k, 42.0 * k);
                estilo::botao(b, "Unmute", estilo::estado_de(b, false, false), false);
                if clicou && b.toca(m) {
                    tirar = Some(nome.clone());
                }
            }
            if nomes.len() > por_coluna * 2 {
                estilo::texto(area.x + 8.0 * k, area.y + area.h - 6.0 * k,
                    &format!("and {} more: unmute some to see the rest", nomes.len() - por_coluna * 2), 12, estilo::SUAVE);
            }
            if let Some(nome) = tirar {
                self.silenciados().remove(&nome);
                self.grava_silenciados();
                self.sistema(format!("You unmuted {nome}. You will see their new messages."));
            }
            estilo::texto(p.x + 22.0 * k, p.y + 394.0 * k,
                "Muted players: only you stop seeing them, on this device.", 13, estilo::SUAVE);
            let voltar = r(740.0, 364.0, 144.0, 46.0);
            estilo::botao(voltar, "Back", estilo::estado_de(voltar, false, false), true);
            if clicou && voltar.toca(m) {
                self.vendo_silenciados = false;
            }
            return saida;
        }

        // ── the conversation ──
        let area = r(16.0, 70.0, BASE_W - 32.0, 282.0);
        draw_rectangle(area.x, area.y, area.w, area.h, Color::new(0.0, 0.0, 0.0, 0.25));
        let passo = LINHA * k;
        let total = self.linhas.len() as f32 * passo;
        if self.no_fim {
            self.rolagem.pos = (total - area.h).max(0.0);
        }
        let toque = self.rolagem.quadro(area, total, passo);
        self.no_fim = self.rolagem.pos >= (total - area.h).max(0.0) - 2.0;
        crate::rolagem::recortar(Some(area));
        let mut tocada = None;
        for (i, l) in self.linhas.iter().enumerate() {
            let y = area.y + i as f32 * passo - self.rolagem.pos;
            if y + passo < area.y || y > area.y + area.h {
                continue;
            }
            let linha = Rect::new(area.x, y, area.w - 12.0 * k, passo);
            if self.selecionada == Some(i) {
                draw_rectangle(linha.x, linha.y, linha.w, linha.h, Color::new(0.91, 0.75, 0.46, 0.16));
            }
            let base = y + passo * 0.5 + 6.0 * k;
            if l.sistema {
                estilo::texto_ajustado(&l.texto, linha.x + 10.0 * k, base, linha.w - 20.0 * k, 14, estilo::SUAVE);
            } else {
                let tag = sigla_do_canal(l.canal);
                estilo::texto_forte(linha.x + 10.0 * k, base, tag, 14, cor_do_canal(l.canal));
                let tw = estilo::medir_forte(tag, 14) + 6.0 * k;
                let linha = Rect::new(linha.x + tw, linha.y, linha.w - tw, linha.h);
                let (rotulo, cor) = if l.canal == shared::chat::CANAL_SUSSURRO {
                    if l.de.eq_ignore_ascii_case(eu) {
                        (format!("To {}:", l.para), cor_do_canal(l.canal))
                    } else {
                        (format!("From {}:", l.de), cor_do_canal(l.canal))
                    }
                } else if l.staff {
                    (format!("[Staff] {}:", l.de), OURO_DO_CHAT)
                } else if l.de.eq_ignore_ascii_case(eu) {
                    (format!("{}:", l.de), estilo::VERDE)
                } else {
                    (format!("{}:", l.de), Color::new(0.55, 0.78, 1.0, 1.0))
                };
                estilo::texto_forte(linha.x + 10.0 * k, base, &rotulo, 14, cor);
                let w = estilo::medir_forte(&rotulo, 14) + 8.0 * k;
                estilo::texto_ajustado(&l.texto, linha.x + 10.0 * k + w, base, linha.w - 24.0 * k - w, 14, estilo::TEXTO);
            }
            if toque.is_some_and(|c| linha.contains(c)) && !l.sistema && !l.de.eq_ignore_ascii_case(eu) {
                tocada = Some(i);
            }
        }
        crate::rolagem::recortar(None);
        self.rolagem.desenha(area, total);
        if self.linhas.is_empty() {
            estilo::texto_centro(area.center().x, area.center().y, "No messages yet. Say hi!", 15, estilo::SUAVE);
        }
        if let Some(i) = tocada {
            self.selecionada = if self.selecionada == Some(i) { None } else { Some(i) };
        }

        // ── actions on the tapped line, or the text box ──
        if let Some(l) = self.selecionada.and_then(|i| self.linhas.get(i)).cloned() {
            estilo::texto_ajustado(&format!("{}:", l.de), p.x + 22.0 * k, p.y + 392.0 * k, 200.0 * k, 15, estilo::TEXTO);
            let sussurro = r(240.0, 364.0, 150.0, 46.0);
            let silenciar = r(400.0, 364.0, 170.0, 46.0);
            let denunciar = r(580.0, 364.0, 150.0, 46.0);
            let voltar = r(740.0, 364.0, 144.0, 46.0);
            estilo::botao(sussurro, "Whisper", estilo::estado_de(sussurro, false, false), false);
            if clicou && sussurro.toca(m) {
                let outro = if l.canal == shared::chat::CANAL_SUSSURRO { Self::outro_do_sussurro(&l, eu) } else { l.de.clone() };
                self.selecionada = None;
                self.aberto = false;
                self.sussurrar(&outro);
                return saida;
            }
            estilo::botao(silenciar, "Mute player", estilo::estado_de(silenciar, false, false), false);
            let ja = self.denunciadas.contains(&l.id);
            estilo::botao(denunciar, if ja { "Reported" } else { "Report" }, estilo::estado_de(denunciar, ja, false), false);
            estilo::botao(voltar, "Back", estilo::estado_de(voltar, false, false), true);
            if clicou && silenciar.toca(m) {
                let nome = l.de.to_lowercase();
                self.silenciados().insert(nome.clone());
                self.grava_silenciados();
                self.linhas.retain(|x| x.sistema || x.staff || x.de.to_lowercase() != nome);
                self.selecionada = None;
                self.sistema(format!("You muted {}. You will not see their messages.", l.de));
            } else if clicou && denunciar.toca(m) && !ja {
                self.denunciadas.insert(l.id);
                saida.push(ClientMessage::DenunciarChat { id: l.id });
                self.selecionada = None;
            } else if clicou && voltar.toca(m) {
                self.selecionada = None;
            }
            return saida;
        }
        let campo = r(16.0, 364.0, 700.0, 46.0);
        let enviar = r(726.0, 364.0, 158.0, 46.0);
        if clicou && campo.toca(m) {
            self.foco = true;
        }
        let mut manda = false;
        if self.foco {
            for &c in digitado {
                match c {
                    '\u{8}' => {
                        self.texto.pop();
                    }
                    '\r' | '\n' => manda = true,
                    c if !c.is_control() && self.texto.chars().count() < shared::chat::MAX_CHARS => self.texto.push(c),
                    _ => {}
                }
            }
            if is_key_pressed(KeyCode::Backspace) && !digitado.contains(&'\u{8}') {
                self.texto.pop();
            }
            if is_key_pressed(KeyCode::Enter) || is_key_pressed(KeyCode::KpEnter) {
                manda = true;
            }
        }
        estilo::cartao(campo, false, self.foco);
        let cursor = if self.foco && (get_time() * 2.0) as i32 % 2 == 0 { "|" } else { "" };
        if self.texto.is_empty() && !self.foco {
            estilo::texto(campo.x + 14.0 * k, campo.center().y + 6.0 * k, "Tap to type…", 15, estilo::SUAVE);
        } else {
            // The END of what was typed, when it no longer fits.
            let mut mostrado = format!("{}{cursor}", self.texto);
            while estilo::medir(&mostrado, 15) > campo.w - 28.0 * k && mostrado.chars().count() > 1 {
                mostrado.remove(0);
            }
            estilo::texto(campo.x + 14.0 * k, campo.center().y + 6.0 * k, &mostrado, 15, estilo::TEXTO);
        }
        let n = self.texto.chars().count();
        if n > shared::chat::MAX_CHARS - 30 {
            estilo::texto(campo.x + campo.w - 64.0 * k, campo.y - 4.0 * k, &format!("{n}/{}", shared::chat::MAX_CHARS), 11, estilo::SUAVE);
        }
        let pode = !self.texto.trim().is_empty();
        estilo::botao(enviar, "Send", estilo::estado_de(enviar, !pode, false), pode);
        if (manda || (clicou && enviar.toca(m))) && pode {
            saida.extend(self.enviar());
        }
        saida
    }
}

/// `MMO_PREVIA_CHAT=1`: the button and the panel with a few lines.
#[cfg(debug_assertions)]
pub async fn previa() {
    let saida = std::env::var("MMO_PREVIA_SAIDA").unwrap_or_else(|_| "/tmp/tempest-chat-preview".into());
    std::fs::create_dir_all(&saida).unwrap();
    next_frame().await;
    let rt = render_target(screen_width() as u32, screen_height() as u32);
    let mut ui = ChatUi { silenciados: Some(HashSet::new()), ..Default::default() };
    for (i, (de, t, staff)) in [
        ("Lyra", "anyone up for the Frozen Hull?", false),
        ("Navegante", "me! level 24, sword", false),
        ("SrBrunji", "Double XP this weekend, have fun!", true),
        ("Kael", "this game is ***** awesome lol", false),
        ("Lyra", "invite me Navegante", false),
    ]
    .iter()
    .enumerate()
    {
        ui.receber(i as i64 + 1, (i % 4) as u8, if i % 4 == 3 { "Navegante".into() } else { String::new() }, de.to_string(), t.to_string(), *staff);
    }
    for (arquivo, aberto) in [("caixa.png", false), ("caixa-ativa.png", false), ("painel.png", true), ("silenciados.png", true)] {
        if arquivo == "silenciados.png" {
            ui.silenciados = Some(["kael", "trollzinho", "spammer99"].iter().map(|s| s.to_string()).collect());
            ui.vendo_silenciados = true;
        }
        if arquivo == "caixa-ativa.png" {
            ui.focar();
            ui.texto = "see you at the docks".into();
        }
        if aberto {
            ui.abrir();
        }
        for _ in 0..3 {
            set_camera(&Camera2D {
                render_target: Some(rt.clone()),
                ..Camera2D::from_display_rect(Rect::new(0.0, 0.0, screen_width(), screen_height()))
            });
            clear_background(Color::new(0.30, 0.42, 0.30, 1.0));
            // A bit of "world" behind, to judge the see-through box.
            for i in 0..12 {
                draw_rectangle(i as f32 * 80.0, 200.0 + (i % 3) as f32 * 30.0, 50.0, 120.0, Color::new(0.45, 0.32, 0.22, 1.0));
            }
            let z = crate::hud_layout::atual();
            ui.caixa(z.chat, "Navegante");
            let _ = ui.entrada(&[], "Navegante");
            let _ = ui.desenha(&[], "Navegante");
            unsafe { get_internal_gl().flush() };
            rt.texture.get_texture_data().export_png(&format!("{saida}/{arquivo}"));
            set_default_camera();
            next_frame().await;
        }
    }
}
