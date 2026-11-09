//! EVENTS: the bonus events active right now (`shared::eventos`, set by the
//! staff in the Panopticon). Menu › Event › Events opens the panel; while any
//! is active a gold badge sits next to the buffs and opens it too (owner,
//! 09/11/2026: "a clear tab in the game to see what bonus is active").

use macroquad::prelude::*;
use shared::eventos::{Evento, Tipo};

#[allow(unused_imports)]
use crate::ui::Toque as _;
use crate::hud_estilo as estilo;

#[derive(Default)]
pub struct EventosUi {
    pub aberto: bool,
    eventos: Vec<Evento>,
}

fn agora_unix() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as i64)
}

fn cor_do_tipo(t: Tipo) -> Color {
    match t {
        Tipo::Xp => Color::new(0.98, 0.80, 0.22, 1.0),
        Tipo::Drop => Color::new(0.62, 0.48, 1.0, 1.0),
        Tipo::Ouro => Color::new(0.95, 0.58, 0.18, 1.0),
    }
}

impl EventosUi {
    pub fn receber(&mut self, eventos: Vec<Evento>) {
        self.eventos = eventos;
    }

    fn ativos(&self) -> Vec<&Evento> {
        let t = agora_unix();
        self.eventos.iter().filter(|e| e.ativo(t)).collect()
    }

    /// The HUD badge, right of the buffs row. `true` = tapped.
    pub fn selo(&self, buffs: Rect) -> bool {
        let ativos = self.ativos();
        let Some(primeiro) = ativos.first() else {
            return false;
        };
        let s = buffs.h / 40.0;
        estilo::no_painel(s, || self.selo_na_escala(buffs, s, &ativos))
    }

    fn selo_na_escala(&self, buffs: Rect, s: f32, ativos: &[&Evento]) -> bool {
        let primeiro = ativos[0];
        let texto = if ativos.len() == 1 {
            format!("{} {}", shared::eventos::mult_texto(primeiro.mult), primeiro.tipo.nome())
        } else {
            format!("{} bonus events", ativos.len())
        };
        let r = Rect::new(buffs.x + buffs.w + 8.0 * s, buffs.y, 0.0, buffs.h);
        let w = estilo::medir_forte(&texto, 16) + 52.0 * s;
        let r = Rect::new(r.x, r.y, w.max(110.0 * s), r.h);
        let pulso = 0.75 + 0.25 * (get_time() as f32 * 2.5).sin();
        draw_rectangle(r.x, r.y, r.w, r.h, Color::new(0.12, 0.08, 0.03, 0.88));
        draw_rectangle_lines(r.x, r.y, r.w, r.h, 2.0 * s, estilo::alfa(estilo::OURO, pulso));
        let c = vec2(r.x + 20.0 * s, r.center().y);
        estrela(c, 10.0 * s, estilo::alfa(estilo::OURO, pulso));
        estilo::texto_forte(r.x + 36.0 * s, r.center().y + 6.0 * s, &texto, 16, estilo::OURO);
        r.toca(Vec2::from(mouse_position())) && crate::foco::clique()
    }

    /// Draws the panel. `Some((kind, name, boss))` = "Find" was tapped on a
    /// one-mob/one-boss event: the caller opens the map with it lit up.
    pub fn desenha(&mut self) -> Option<(u16, String, bool)> {
        if !self.aberto {
            return None;
        }
        let mut achar = None;
        let seguro0 = crate::hud_layout::tela_segura();
        let k = estilo::escala_do_painel(620.0, 400.0)
            .min((seguro0.w - 16.0) / 620.0)
            .min((seguro0.h - 16.0) / 400.0);
        estilo::no_painel(k, || {
            let seguro = crate::hud_layout::tela_segura();
            let (w, h) = ((620.0 * k).min(seguro.w - 16.0), (400.0 * k).min(seguro.h - 16.0));
            let p = Rect::new(seguro.center().x - w * 0.5, seguro.center().y - h * 0.5, w, h);
            crate::hud_layout::escurece(0.5);
            estilo::painel_destaque(p, estilo::OURO);
            let m = Vec2::from(mouse_position());
            let clicou = crate::foco::clique();
            estilo::texto_forte(p.x + 22.0 * k, p.y + 38.0 * k, "Bonus events", 22, estilo::OURO);
            estilo::texto(
                p.x + 22.0 * k,
                p.y + 62.0 * k,
                "Active right now. Bonuses on the same kill stack.",
                13,
                estilo::SUAVE,
            );
            let fechar = Rect::new(p.x + p.w - 56.0 * k, p.y + 10.0 * k, 46.0 * k, 46.0 * k);
            estilo::botao(fechar, "X", estilo::estado_de(fechar, false, false), false);
            if clicou && (fechar.toca(m) || !p.contains(m)) {
                self.aberto = false;
                return;
            }
            let ativos = self.ativos();
            if ativos.is_empty() {
                estilo::texto_centro(
                    p.center().x,
                    p.center().y,
                    "No bonus event right now.",
                    18,
                    estilo::TEXTO,
                );
                estilo::texto_centro(
                    p.center().x,
                    p.center().y + 26.0 * k,
                    "Watch the announcements: events are posted at the top of the screen.",
                    13,
                    estilo::SUAVE,
                );
                return;
            }
            let t = agora_unix();
            for (i, e) in ativos.iter().take(4).enumerate() {
                let r = Rect::new(p.x + 18.0 * k, p.y + (80.0 + i as f32 * 78.0) * k, p.w - 36.0 * k, 70.0 * k);
                let cor = cor_do_tipo(e.tipo);
                estilo::cartao(r, false, false);
                draw_rectangle(r.x, r.y, 6.0 * k, r.h, cor);
                estilo::texto_centro_forte(
                    r.x + 62.0 * k,
                    r.center().y + 10.0 * k,
                    &shared::eventos::mult_texto(e.mult),
                    28,
                    cor,
                );
                // The big number on the left already says the multiplier.
                let titulo = e.titulo();
                let titulo = titulo.split_once(' ').map_or(titulo.as_str(), |(_, r)| r);
                let titulo = format!("{}{}", titulo[..1].to_uppercase(), &titulo[1..]);
                estilo::texto_ajustado(&titulo, r.x + 116.0 * k, r.y + 30.0 * k, r.w - 250.0 * k, 17, estilo::TEXTO);
                let quem = match e.tipo {
                    Tipo::Xp => "XP from kills",
                    Tipo::Drop => "chance of every drop",
                    Tipo::Ouro => "gold and copper from kills",
                };
                estilo::texto(r.x + 116.0 * k, r.y + 54.0 * k, quem, 13, estilo::SUAVE);
                // FIND: one mob or one boss → show where it lives on the map.
                let alvo_kind = match e.escopo {
                    shared::eventos::Escopo::Mob => e.alvo.trim().parse::<u16>().ok().map(|k| (k, false)),
                    shared::eventos::Escopo::Chefe => e.alvo.trim().parse::<u16>().ok().map(|k| (k, true)),
                    _ => None,
                };
                let mut direita = r.x + r.w - 16.0 * k;
                if let Some((kind, chefe)) = alvo_kind {
                    let b = Rect::new(direita - 96.0 * k, r.y + 13.0 * k, 96.0 * k, 44.0 * k);
                    estilo::botao(b, "Find", estilo::estado_de(b, false, false), true);
                    if clicou && b.toca(m) {
                        achar = Some((kind, e.alvo_nome.clone(), chefe));
                    }
                    direita = b.x - 12.0 * k;
                }
                let resta = format!("{} left", shared::eventos::resta_texto(e.fim_unix - t));
                let rw = estilo::medir_forte(&resta, 15);
                estilo::texto_forte(direita - rw, r.center().y + 6.0 * k, &resta, 15, estilo::OURO);
            }
            if ativos.len() > 4 {
                estilo::texto_centro(
                    p.center().x,
                    p.y + p.h - 12.0 * k,
                    &format!("+{} more", ativos.len() - 4),
                    13,
                    estilo::SUAVE,
                );
            }
        });
        if achar.is_some() {
            self.aberto = false;
        }
        achar
    }
}

fn estrela(c: Vec2, r: f32, cor: Color) {
    let ponto = |i: usize| {
        let a = -std::f32::consts::FRAC_PI_2 + i as f32 * std::f32::consts::PI / 5.0;
        let rr = if i % 2 == 0 { r } else { r * 0.45 };
        vec2(c.x + a.cos() * rr, c.y + a.sin() * rr)
    };
    for i in 0..10 {
        draw_triangle(c, ponto(i), ponto((i + 1) % 10), cor);
    }
}

/// `MMO_PREVIA_EVENTOS=1`: the badge and the panel with three events.
#[cfg(debug_assertions)]
pub async fn previa() {
    use shared::eventos::Escopo;
    let saida = std::env::var("MMO_PREVIA_SAIDA").unwrap_or_else(|_| "/tmp/tempest-eventos-preview".into());
    std::fs::create_dir_all(&saida).unwrap();
    next_frame().await;
    let rt = render_target(screen_width() as u32, screen_height() as u32);
    let t = agora_unix();
    let ev = |id, tipo, escopo, nome: &str, mult, min: i64| Evento {
        id,
        tipo,
        escopo,
        alvo: "x".into(),
        alvo_nome: nome.into(),
        mult,
        inicio_unix: t - 60,
        fim_unix: t + min * 60,
    };
    let mut ui = EventosUi::default();
    ui.receber(vec![
        ev(1, Tipo::Xp, Escopo::Todos, "", 1.5, 125),
        ev(2, Tipo::Drop, Escopo::Chefe, "Alpha Wolf of the Glade", 2.0, 40),
        ev(3, Tipo::Ouro, Escopo::Dungeon, "Frozen Hull", 3.0, 1440),
    ]);
    for (arquivo, aberto) in [("selo.png", false), ("painel.png", true)] {
        ui.aberto = aberto;
        for _ in 0..3 {
            set_camera(&Camera2D {
                render_target: Some(rt.clone()),
                ..Camera2D::from_display_rect(Rect::new(0.0, 0.0, screen_width(), screen_height()))
            });
            clear_background(Color::new(0.10, 0.14, 0.12, 1.0));
            let z = crate::hud_layout::atual();
            draw_rectangle(z.buffs.x, z.buffs.y, z.buffs.w, z.buffs.h, Color::new(1.0, 1.0, 1.0, 0.06));
            let _ = ui.selo(z.buffs);
            let _ = ui.desenha();
            unsafe { get_internal_gl().flush() };
            rt.texture.get_texture_data().export_png(&format!("{saida}/{arquivo}"));
            set_default_camera();
            next_frame().await;
        }
    }
}
