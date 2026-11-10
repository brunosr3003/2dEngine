//! Settings › Interface › Customize HUD: drag any button to move it, − / +
//! to resize it, one size for all buttons at once, reset one or everything.
//!
//! While it is open the real HUD is not drawn (it is a full-screen panel like
//! any other, `main::painel_grande`): each element shows as a labelled box in
//! the exact place and size it will have, straight from `hud_layout::atual`,
//! so what you see is what you get. Saved on the device (`hud_custom`).

#[allow(unused_imports)]
use crate::ui::Toque as _;
use macroquad::prelude::*;

use crate::hud_custom::{self, Ajuste, Layout};
use crate::hud_estilo as estilo;
use crate::hud_layout;

#[derive(Default)]
pub struct EditorHud {
    pub aberto: bool,
    layout: Layout,
    sel: Option<&'static str>,
    /// Element under the finger, the finger's offset from its center, where
    /// the finger went down, and whether it has started moving (a plain tap
    /// only picks it).
    arrasto: Option<(&'static str, Vec2, Vec2, bool)>,
    /// "Reset all" asks for a second tap.
    confirmar_tudo: bool,
    /// The player chose where the bar sits (0 middle, 1 up, 2 down).
    posicao_barra: Option<usize>,
}

fn mouse() -> Vec2 {
    Vec2::from(mouse_position())
}

/// The element under `p`: the smallest box that holds it (a button on top of
/// a panel wins).
fn sob(p: Vec2) -> Option<(&'static str, Rect)> {
    let mut z = hud_layout::atual();
    hud_custom::ITENS
        .iter()
        .filter_map(|&(id, _, _)| hud_custom::campo(&mut z, id).map(|r| (id, *r)))
        .filter(|(_, r)| r.contains(p))
        .min_by(|a, b| (a.1.w * a.1.h).total_cmp(&(b.1.w * b.1.h)))
}

impl EditorHud {
    pub fn abrir(&mut self) {
        self.aberto = true;
        self.layout = hud_custom::atual();
        self.sel = None;
        self.arrasto = None;
        self.confirmar_tudo = false;
        self.posicao_barra = None;
    }

    /// Saves and closes.
    pub fn fechar(&mut self) {
        if !self.aberto {
            return;
        }
        self.aberto = false;
        self.arrasto = None;
        hud_custom::define(self.layout.clone(), true);
    }

    fn muda(&mut self, salvar: bool) {
        hud_custom::define(self.layout.clone(), salvar);
    }

    /// Draws and handles touch. `true` on the frame it closed.
    pub fn desenha(&mut self) -> bool {
        if !self.aberto {
            return false;
        }
        let (sw, sh) = crate::render3d::tela();
        draw_rectangle(0.0, 0.0, sw, sh, Color::new(0.0, 0.0, 0.0, 0.45));
        let seguro = hud_layout::tela_segura();
        let f = estilo::fator_texto();
        let m = mouse();

        // ── the elements, where they will be ──
        let mut z = hud_layout::atual();
        for &(id, nome, _) in hud_custom::ITENS {
            let Some(r) = hud_custom::campo(&mut z, id).map(|r| *r) else { continue };
            let sel = self.sel == Some(id);
            let mudou = self.layout.itens.contains_key(id);
            draw_rectangle(r.x, r.y, r.w, r.h, if sel {
                Color::new(0.91, 0.75, 0.46, 0.30)
            } else {
                Color::new(1.0, 1.0, 1.0, 0.10)
            });
            draw_rectangle_lines(r.x, r.y, r.w, r.h, if sel { 3.0 } else { 1.5 }, if sel {
                estilo::OURO
            } else if mudou {
                Color::new(0.62, 0.80, 0.98, 0.9)
            } else {
                Color::new(1.0, 1.0, 1.0, 0.55)
            });
            let lado = r.w.min(r.h);
            let c = r.center();
            let letra = if r.w < 64.0 * f { 8 } else { 10 };
            if !crate::icones_ui::ui(id, c - vec2(0.0, lado * 0.08), lado * 0.55, estilo::TEXTO) {
                estilo::texto_centro(c.x, c.y + 4.0 * f, nome, letra, estilo::TEXTO);
            } else if lado > 70.0 * f {
                estilo::texto_centro(c.x, r.y + r.h - 6.0 * f, nome, letra, estilo::TEXTO);
            }
        }

        // ── the toolbar, in the middle of the screen (the HUD lives on the edges) ──
        let w = (680.0 * f).min(seguro.w - 16.0);
        let lh = 44.0 * f;
        let h = lh * 2.0 + 50.0 * f;
        // Middle, a third up or a third down: wherever it hides the fewest
        // elements, and never the picked one.
        let barra = {
            let mut zz = hud_layout::atual();
            let caixas: Vec<(bool, Rect)> = hud_custom::ITENS
                .iter()
                .filter_map(|&(id, _, _)| hud_custom::campo(&mut zz, id).map(|r| (self.sel == Some(id), *r)))
                .collect();
            let meio = Rect::new(seguro.center().x - w * 0.5, seguro.center().y - h * 0.5, w, h);
            let opcoes: Vec<Rect> = [0.0, -seguro.h * 0.28, seguro.h * 0.28]
                .into_iter()
                .map(|dy| Rect::new(meio.x, (meio.y + dy).clamp(seguro.y, seguro.y + seguro.h - h), w, h))
                .collect();
            if let Some(i) = self.posicao_barra {
                opcoes[i % 3]
            } else {
            opcoes
                .into_iter()
                .min_by_key(|b| {
                    caixas
                        .iter()
                        .filter(|(_, r)| r.overlaps(b))
                        .map(|(sel, _)| if *sel { 100 } else { 1 })
                        .sum::<u32>()
                })
                .unwrap_or(meio)
            }
        };
        let mut barra = barra;
        // While dragging the bar steps aside: the element can go anywhere.
        let arrastando = self.arrasto.is_some_and(|a| a.3);
        if arrastando {
            barra.y = -10_000.0;
        }
        estilo::painel(barra);
        let x0 = barra.x + 10.0 * f;
        let y1 = barra.y + 10.0 * f;
        let y2 = y1 + lh + 10.0 * f;
        let bw = 44.0 * f;
        let botao = |r: Rect, t: &str, ativo: bool| {
            estilo::cartao(r, r.toca(m), ativo);
            estilo::texto_centro(r.center().x, r.center().y + 6.0 * f, t, 15, estilo::TEXTO);
        };
        // Row 1: the picked element (what − / + here change), Done.
        let pronto = Rect::new(barra.x + barra.w - 10.0 * f - 110.0 * f, y1, 110.0 * f, lh);
        botao(pronto, "Done", true);
        let (menos_s, mais_s, um) = (
            Rect::new(x0 + 190.0 * f, y1, bw, lh),
            Rect::new(x0 + 190.0 * f + bw + 72.0 * f, y1, bw, lh),
            Rect::new(pronto.x - 8.0 * f - 110.0 * f, y1, 110.0 * f, lh),
        );
        match self.sel {
            Some(id) => {
                estilo::texto(x0, y1 + lh * 0.62, hud_custom::nome(id), 15, estilo::OURO);
                if hud_custom::redimensiona(id) {
                    let k = self.layout.itens.get(id).map_or(1.0, |a| a.tamanho);
                    botao(menos_s, "-", false);
                    botao(mais_s, "+", false);
                    estilo::texto_centro_forte(
                        (menos_s.x + menos_s.w + mais_s.x) * 0.5,
                        y1 + lh * 0.64,
                        &format!("{:.0}%", k * 100.0),
                        16,
                        estilo::TEXTO,
                    );
                } else {
                    estilo::texto(menos_s.x, y1 + lh * 0.62, "This one can only be moved", 13, estilo::SUAVE);
                }
                botao(um, "Reset", false);
            }
            None => estilo::texto(
                x0,
                y1 + lh * 0.62,
                "Tap a button to pick it, drag it to move it.",
                14,
                estilo::SUAVE,
            ),
        }
        // Row 2: every button at once, move the bar, reset everything.
        estilo::texto(x0, y2 + lh * 0.62, "All buttons", 14, estilo::SUAVE);
        let menos_t = Rect::new(x0 + 104.0 * f, y2, bw, lh);
        let mais_t = Rect::new(menos_t.x + bw + 72.0 * f, y2, bw, lh);
        botao(menos_t, "-", false);
        botao(mais_t, "+", false);
        estilo::texto_centro_forte(
            (menos_t.x + menos_t.w + mais_t.x) * 0.5,
            y2 + lh * 0.64,
            &format!("{:.0}%", self.layout.botoes * 100.0),
            16,
            estilo::OURO,
        );
        let tudo = Rect::new(barra.x + barra.w - 10.0 * f - 128.0 * f, y2, 128.0 * f, lh);
        botao(tudo, if self.confirmar_tudo { "Sure?" } else { "Reset all" }, false);
        let mover = Rect::new(tudo.x - 8.0 * f - 104.0 * f, y2, 104.0 * f, lh);
        let mover = (mover.x > mais_t.x + mais_t.w + 4.0 * f).then_some(mover);
        if let Some(b) = mover {
            botao(b, "Move bar", false);
        }
        draw_line(x0, y2 - 5.0 * f, barra.x + barra.w - 10.0 * f, y2 - 5.0 * f, 1.0, Color::new(1.0, 1.0, 1.0, 0.12));
        estilo::texto(
            x0,
            barra.y + barra.h - 8.0 * f,
            "Saved on this device. Blue outline = changed.",
            11,
            estilo::SUAVE,
        );

        // ── touch ──
        // Escape / Back is `main`'s (it closes this like any panel).
        if let Some((id, d, inicio, movendo)) = self.arrasto {
            if is_mouse_button_down(MouseButton::Left) && (movendo || m.distance(inicio) > 8.0) {
                self.arrasto = Some((id, d, inicio, true));
                let c = m + d;
                let frac = ((c - seguro.point()) / seguro.size()).clamp(Vec2::ZERO, Vec2::ONE);
                let a = self.layout.itens.entry(id.to_string()).or_default();
                if a.centro != Some(frac) {
                    a.centro = Some(frac);
                    self.muda(false);
                }
            } else if !is_mouse_button_down(MouseButton::Left) {
                self.arrasto = None;
                if movendo {
                    self.muda(true);
                }
            }
        }
        if !is_mouse_button_pressed(MouseButton::Left) {
            return false;
        }
        if barra.toca(m) {
            let sel_k = |l: &mut Layout, id: &str, passo: f32| {
                let a = l.itens.entry(id.to_string()).or_insert(Ajuste::default());
                a.tamanho = hud_custom::limita(a.tamanho + passo);
                if *a == Ajuste::default() {
                    l.itens.remove(id);
                }
            };
            if tudo.toca(m) {
                if self.confirmar_tudo {
                    self.layout = Layout::default();
                    self.sel = None;
                    self.confirmar_tudo = false;
                    self.muda(true);
                } else {
                    self.confirmar_tudo = true;
                }
                return false;
            }
            self.confirmar_tudo = false;
            if pronto.toca(m) {
                self.fechar();
                return true;
            } else if mover.is_some_and(|b| b.toca(m)) {
                // From wherever it is now to the next of the three places.
                let agora = [0.0f32, -0.28, 0.28]
                    .iter()
                    .position(|dy| (barra.y - (seguro.center().y - h * 0.5 + dy * seguro.h)).abs() < 2.0)
                    .unwrap_or(0);
                self.posicao_barra = Some(self.posicao_barra.unwrap_or(agora) + 1);
            } else if menos_t.toca(m) || mais_t.toca(m) {
                let passo = if mais_t.toca(m) { hud_custom::PASSO } else { -hud_custom::PASSO };
                self.layout.botoes = hud_custom::limita(self.layout.botoes + passo);
                self.muda(true);
            } else if let Some(id) = self.sel {
                if um.toca(m) {
                    self.layout.itens.remove(id);
                    self.muda(true);
                } else if hud_custom::redimensiona(id) && (menos_s.toca(m) || mais_s.toca(m)) {
                    let passo = if mais_s.toca(m) { hud_custom::PASSO } else { -hud_custom::PASSO };
                    sel_k(&mut self.layout, id, passo);
                    self.muda(true);
                }
            }
            return false;
        }
        self.confirmar_tudo = false;
        match sob(m) {
            Some((id, r)) => {
                self.sel = Some(id);
                self.arrasto = Some((id, r.center() - m, m, false));
            }
            None => self.sel = None,
        }
        false
    }
}

/// `MMO_PREVIA_EDITOR_HUD=1`: the editor over a plain background, nothing
/// picked and with the attack button picked, at a PC and a phone size.
#[cfg(debug_assertions)]
pub async fn previa() {
    let saida = std::env::var("MMO_PREVIA_SAIDA").unwrap_or_else(|_| "/tmp/tempest-editor-hud".into());
    std::fs::create_dir_all(&saida).unwrap();
    let mut ed = EditorHud::default();
    ed.abrir();
    for (w, h) in [(1280.0, 720.0), (844.0, 390.0)] {
        request_new_screen_size(w, h);
        for _ in 0..4 {
            next_frame().await;
        }
        let rt = render_target(screen_width() as u32, screen_height() as u32);
        for (nome, sel) in [("livre", None), ("atacar", Some("atacar"))] {
            ed.sel = sel;
            for _ in 0..3 {
                set_camera(&Camera2D {
                    render_target: Some(rt.clone()),
                    ..Camera2D::from_display_rect(Rect::new(0.0, 0.0, screen_width(), screen_height()))
                });
                clear_background(Color::new(0.30, 0.42, 0.30, 1.0));
                let _ = ed.desenha();
                unsafe { get_internal_gl().flush() };
                rt.texture.get_texture_data().export_png(&format!("{saida}/{nome}-{}.png", w as u32));
                set_default_camera();
                next_frame().await;
            }
        }
    }
}
