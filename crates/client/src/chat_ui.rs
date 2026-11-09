//! GLOBAL CHAT (`ServerMessage::ChatGlobal`, `shared::chat`): the balloon
//! button right of the notices, and the panel with the conversation, the
//! text box and, on a tapped line, "Mute" and "Report".
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
        self.aberto && self.foco
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
    pub fn receber(&mut self, id: i64, de: String, texto: String, staff: bool) -> bool {
        if !staff && self.silenciados().contains(&de.to_lowercase()) {
            return false;
        }
        self.guarda(Linha { id, de, texto, staff, sistema: false });
        true
    }

    /// A system answer (slow down, muted, report sent…), shown in the panel too.
    pub fn sistema(&mut self, texto: String) {
        let aberto = self.aberto;
        self.guarda(Linha { id: 0, de: String::new(), texto, staff: false, sistema: true });
        if !aberto {
            self.nao_lidas = self.nao_lidas.saturating_sub(1);
        }
    }

    /// The balloon button in the HUD. `true` = tapped.
    pub fn botao(&self, r: Rect) -> bool {
        let m = Vec2::from(mouse_position());
        let sobre = r.toca(m);
        let c = r.center();
        let raio = r.w * 0.5;
        estilo::botao_redondo(c, raio, estilo::OURO, estilo::estado(sobre, is_mouse_button_down(MouseButton::Left), false, false), false);
        // The balloon: a rounded body and a tail.
        let (bw, bh) = (raio * 1.0, raio * 0.70);
        let corpo = Rect::new(c.x - bw * 0.5, c.y - bh * 0.62, bw, bh);
        let cor = estilo::OURO;
        draw_rectangle(corpo.x + bh * 0.25, corpo.y, corpo.w - bh * 0.5, corpo.h, cor);
        draw_circle(corpo.x + bh * 0.25, corpo.y + bh * 0.5, bh * 0.5, cor);
        draw_circle(corpo.x + corpo.w - bh * 0.25, corpo.y + bh * 0.5, bh * 0.5, cor);
        draw_triangle(
            vec2(corpo.x + bw * 0.22, corpo.y + bh * 0.8),
            vec2(corpo.x + bw * 0.46, corpo.y + bh * 0.9),
            vec2(corpo.x + bw * 0.12, corpo.y + bh * 1.32),
            cor,
        );
        let fundo = Color::new(0.10, 0.07, 0.05, 1.0);
        for i in 0..3 {
            draw_circle(corpo.x + bw * (0.28 + i as f32 * 0.22), corpo.y + bh * 0.5, bh * 0.10, fundo);
        }
        if self.nao_lidas > 0 {
            let b = vec2(r.x + r.w - raio * 0.25, r.y + raio * 0.25);
            draw_circle(b.x, b.y, raio * 0.36, Color::new(0.85, 0.16, 0.16, 1.0));
            let t = if self.nao_lidas > 99 { "99+".to_string() } else { self.nao_lidas.to_string() };
            estilo::texto_centro_forte(b.x, b.y + raio * 0.14, &t, 11, WHITE);
        }
        sobre && crate::foco::clique()
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
                let (rotulo, cor) = if l.staff {
                    (format!("[Staff] {}:", l.de), estilo::OURO)
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
            estilo::texto_ajustado(&format!("{}:", l.de), p.x + 22.0 * k, p.y + 392.0 * k, 260.0 * k, 15, estilo::TEXTO);
            let silenciar = r(300.0, 364.0, 190.0, 46.0);
            let denunciar = r(500.0, 364.0, 190.0, 46.0);
            let voltar = r(700.0, 364.0, 184.0, 46.0);
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
            saida.push(ClientMessage::Chat { text: std::mem::take(&mut self.texto) });
            self.no_fim = true;
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
        ui.receber(i as i64 + 1, de.to_string(), t.to_string(), *staff);
    }
    for (arquivo, aberto) in [("botao.png", false), ("painel.png", true)] {
        if aberto {
            ui.abrir();
            ui.texto = "see you at the docks".into();
        }
        for _ in 0..3 {
            set_camera(&Camera2D {
                render_target: Some(rt.clone()),
                ..Camera2D::from_display_rect(Rect::new(0.0, 0.0, screen_width(), screen_height()))
            });
            clear_background(Color::new(0.10, 0.14, 0.12, 1.0));
            let z = crate::hud_layout::atual();
            let _ = ui.botao(z.chat);
            let _ = ui.desenha(&[], "Navegante");
            unsafe { get_internal_gl().flush() };
            rt.texture.get_texture_data().export_png(&format!("{saida}/{arquivo}"));
            set_default_camera();
            next_frame().await;
        }
    }
}
