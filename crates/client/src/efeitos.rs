//! O que o golpe deixa na tela: o numero, a faisca e a dor.
//!
//! Tudo aqui sai de `World::acertos`, que vem do servidor com o dano REAL —
//! e nao da queda de vida, que no modo imortal nao existe e num golpe
//! absorvido tambem nao. Desenhado em 2D por cima do mundo, na posicao
//! projetada de quem apanhou: numero de dano e' leitura, e leitura nao pode
//! sumir atras de uma arvore.

use macroquad::prelude::*;

use crate::render3d::{world_to_screen, Vista};
use crate::world::World;

/// Quanto tempo o numero fica na tela.
const VIDA_DO_NUMERO: f32 = 0.95;
/// Quanto a faisca dura.
const VIDA_DA_FAISCA: f32 = 0.2;

fn altura_de(e: &crate::world::Ent) -> f32 {
    let boss = e.state.flags & shared::ent_flags::BOSS != 0;
    crate::bicho::do_mob(e.meta.tag, e.meta.kind, boss).map_or(1.95, |(_, a)| a)
}

/// Texto com contorno: por cima de grama, areia e ceu, so' contorno garante
/// leitura.
fn texto_contornado(s: &str, x: f32, y: f32, tam: f32, cor: Color) {
    let sombra = Color::new(0.0, 0.0, 0.0, cor.a * 0.85);
    for (dx, dy) in [(-2.0, 0.0), (2.0, 0.0), (0.0, -2.0), (0.0, 2.0), (1.5, 1.5)] {
        draw_text(s, x + dx, y + dy, tam, sombra);
    }
    draw_text(s, x, y, tam, cor);
}

pub fn desenha(world: &World, vista: &Vista) {
    for ef in &world.efeitos {
        let Some(e) = world.ents.get(&ef.alvo) else { continue };
        let pe = vista.pos_de(e);
        let alt = altura_de(e);

        // ── a faisca: raios saindo do peito de quem apanhou ──
        if ef.t < VIDA_DA_FAISCA {
            if let Some(c) = world_to_screen(&vista.cam, pe + vec3(0.0, alt * 0.55, 0.0)) {
                let u = ef.t / VIDA_DA_FAISCA;
                let cor = if ef.eu {
                    Color::new(1.0, 0.35, 0.28, 1.0 - u)
                } else {
                    Color::new(1.0, 0.95, 0.72, 1.0 - u)
                };
                let escala = if ef.critico { 1.5 } else { 1.0 };
                draw_circle(c.x, c.y, (18.0 * (1.0 - u) + 4.0) * escala, Color::new(1.0, 1.0, 1.0, 0.55 * (1.0 - u)));
                for k in 0..8 {
                    let ang = ef.semente * 6.283 + k as f32 * 0.785;
                    let (s, co) = ang.sin_cos();
                    let r0 = 8.0 + 26.0 * u * escala;
                    let r1 = r0 + (16.0 - 10.0 * u) * escala;
                    draw_line(c.x + co * r0, c.y + s * r0, c.x + co * r1, c.y + s * r1, 3.0 * (1.0 - u) + 1.0, cor);
                }
            }
        }

        // ── o numero: pula grande, sobe e some ──
        if ef.t < VIDA_DO_NUMERO {
            let Some(c) = world_to_screen(&vista.cam, pe + vec3(0.0, alt + 0.25, 0.0)) else { continue };
            let u = ef.t / VIDA_DO_NUMERO;
            let sobe = 46.0 * (1.0 - (1.0 - u).powi(3));
            let pula = 1.0 + 0.7 * (-ef.t * 14.0).exp();
            let base = if ef.critico { 30.0 } else { 23.0 };
            let tam = base * pula;
            let alfa = if u < 0.6 { 1.0 } else { 1.0 - (u - 0.6) / 0.4 };
            let cor = if ef.eu {
                Color::new(1.0, 0.32, 0.28, alfa)
            } else if ef.critico {
                Color::new(1.0, 0.62, 0.18, alfa)
            } else {
                Color::new(1.0, 0.95, 0.78, alfa)
            };
            let txt = if ef.critico { format!("{}!", ef.dano) } else { ef.dano.to_string() };
            let d = measure_text(&txt, None, tam as u16, 1.0);
            let x = c.x - d.width * 0.5 + (ef.semente - 0.5) * 36.0;
            texto_contornado(&txt, x, c.y - sobe, tam, cor);
        }
    }

    // ── a dor: a borda da tela fica vermelha quando e' VOCE que apanha ──
    if world.dor > 0.01 {
        let (w, h) = (screen_width(), screen_height());
        for i in 0..14 {
            let f = i as f32 / 14.0;
            let a = world.dor * 0.30 * (1.0 - f).powi(2);
            let m = i as f32 * 7.0;
            draw_rectangle_lines(m, m, w - 2.0 * m, h - 2.0 * m, 7.0, Color::new(0.85, 0.08, 0.06, a));
        }
    }
}
