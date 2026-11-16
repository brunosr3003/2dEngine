//! THE PATH TREE'S GLYPHS: small vector icons drawn with lines, triangles
//! and circles (no image files), one per stat the web gives
//! (`shared::arvore::Efeito`) plus the start's compass. `desenha` centres a
//! glyph on `c` inside a circle of radius `r`, in one colour, so the same
//! icon works dim (locked), lit (owned) and on the tooltip.

use macroquad::prelude::*;
use shared::arvore::Efeito;
use std::f32::consts::{PI, TAU};

use crate::hud_estilo as estilo;

/// Every glyph the tree draws.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Glifo {
    Espada,
    Escudo,
    EscudoBarra,
    Olho,
    Estrela,
    Bota,
    Coracao,
    CoracaoMais,
    Moeda,
    Gota,
    GotaMais,
    Chama,
    Relogio,
    Raio,
    Mira,
    Ancora,
    Bau,
    Picareta,
    Cristal,
    Lingote,
    Trevo,
    Seta,
    Asa,
    Caveira,
    Bussola,
}

/// Every glyph, for the preview sheet and the tests.
pub const TODOS: [Glifo; 25] = [
    Glifo::Espada,
    Glifo::Escudo,
    Glifo::EscudoBarra,
    Glifo::Olho,
    Glifo::Estrela,
    Glifo::Bota,
    Glifo::Coracao,
    Glifo::CoracaoMais,
    Glifo::Moeda,
    Glifo::Gota,
    Glifo::GotaMais,
    Glifo::Chama,
    Glifo::Relogio,
    Glifo::Raio,
    Glifo::Mira,
    Glifo::Ancora,
    Glifo::Bau,
    Glifo::Picareta,
    Glifo::Cristal,
    Glifo::Lingote,
    Glifo::Trevo,
    Glifo::Seta,
    Glifo::Asa,
    Glifo::Caveira,
    Glifo::Bussola,
];

/// The glyph of a stat.
pub fn do_efeito(e: Efeito) -> Glifo {
    match e {
        Efeito::Critico => Glifo::Estrela,
        Efeito::VelocidadeDeAtaque => Glifo::Raio,
        Efeito::DestrezaPct => Glifo::Mira,
        Efeito::Movimento => Glifo::Bota,
        Efeito::Esquiva => Glifo::Asa,
        Efeito::SabedoriaPct => Glifo::Olho,
        Efeito::ManaPct => Glifo::Gota,
        Efeito::RegenMana => Glifo::GotaMais,
        Efeito::AtaquePct => Glifo::Espada,
        Efeito::DefesaPct => Glifo::Escudo,
        Efeito::VidaPct => Glifo::Coracao,
        Efeito::Reducao => Glifo::EscudoBarra,
        Efeito::RegenVida => Glifo::CoracaoMais,
        Efeito::Firmeza => Glifo::Ancora,
        Efeito::Drop => Glifo::Bau,
        Efeito::Ouro => Glifo::Moeda,
        Efeito::Minerio => Glifo::Picareta,
        Efeito::Energia => Glifo::Cristal,
        Efeito::Darksteel => Glifo::Lingote,
        Efeito::LootMelhor => Glifo::Trevo,
        Efeito::DanoCritico => Glifo::Chama,
        Efeito::Xp => Glifo::Seta,
        Efeito::CanhaoPct => Glifo::Chama,
        Efeito::CascoPct => Glifo::Ancora,
        Efeito::PegarTesouro => Glifo::Asa,
        Efeito::MultTesouro => Glifo::Bau,
        Efeito::CanhaoContraMarcado => Glifo::Mira,
        Efeito::SempreMarcado => Glifo::Caveira,
    }
}

/// A keystone's glyph: a skull for the ones that cost health, else its
/// main stat's.
pub fn da_chave(efeitos: &[(Efeito, f32)]) -> Glifo {
    if efeitos.iter().any(|&(e, v)| (e == Efeito::VidaPct || e == Efeito::SempreMarcado) && v < 0.0) {
        return Glifo::Caveira;
    }
    efeitos.first().map_or(Glifo::Estrela, |&(e, _)| do_efeito(e))
}

/// Draws `g` centred on `c`, fitting a circle of radius `r`, in `cor`.
pub fn desenha(g: Glifo, c: Vec2, r: f32, cor: Color) {
    let p = |x: f32, y: f32| c + vec2(x, y) * r;
    let w = |k: f32| (k * r).max(1.0);
    let linha = |a: Vec2, b: Vec2, k: f32| estilo::traco(a, b, w(k), cor);
    let tri = |a: Vec2, b: Vec2, d: Vec2| draw_triangle(a, b, d, cor);
    let quad = |a: Vec2, b: Vec2, d: Vec2, e: Vec2| {
        draw_triangle(a, b, d, cor);
        draw_triangle(a, d, e, cor);
    };
    let fundo = Color::new(0.05, 0.04, 0.07, cor.a);
    match g {
        Glifo::Espada => {
            // A broad blade, a wide guard, grip and pommel.
            quad(p(-0.15, 0.22), p(-0.15, -0.58), p(0.15, -0.58), p(0.15, 0.22));
            tri(p(-0.15, -0.58), p(0.0, -0.92), p(0.15, -0.58));
            estilo::traco(p(0.0, -0.70), p(0.0, 0.16), w(0.05), fundo);
            linha(p(-0.50, 0.30), p(0.50, 0.30), 0.17);
            linha(p(0.0, 0.34), p(0.0, 0.64), 0.16);
            draw_circle(p(0.0, 0.76).x, p(0.0, 0.76).y, 0.14 * r, cor);
        }
        Glifo::Escudo | Glifo::EscudoBarra => {
            let topo = [p(-0.62, -0.62), p(0.0, -0.78), p(0.62, -0.62)];
            tri(topo[0], topo[1], topo[2]);
            quad(p(-0.62, -0.62), p(0.62, -0.62), p(0.52, 0.10), p(-0.52, 0.10));
            tri(p(-0.52, 0.10), p(0.52, 0.10), p(0.0, 0.84));
            if g == Glifo::EscudoBarra {
                linha(p(-0.40, -0.12), p(0.40, -0.12), 0.14);
                let f = Color::new(fundo.r, fundo.g, fundo.b, 1.0);
                estilo::traco(p(-0.30, -0.12), p(0.30, -0.12), w(0.12), f);
            } else {
                // The boss line down the middle.
                estilo::traco(p(0.0, -0.58), p(0.0, 0.62), w(0.08), fundo);
            }
        }
        Glifo::Olho => {
            let n = 14;
            for i in 0..n {
                let a0 = PI * i as f32 / n as f32;
                let a1 = PI * (i + 1) as f32 / n as f32;
                for s in [-1.0, 1.0] {
                    let q = |a: f32| p(-a.cos() * 0.82, s * a.sin() * 0.46);
                    linha(q(a0), q(a1), 0.11);
                }
            }
            draw_circle(c.x, c.y, 0.28 * r, cor);
            draw_circle(c.x, c.y, 0.12 * r, fundo);
        }
        Glifo::Estrela => {
            // The crit star: four long points, four short.
            for i in 0..8 {
                let a = i as f32 * TAU / 8.0 - PI / 2.0;
                let longo = if i % 2 == 0 { 0.92 } else { 0.50 };
                let ponta = c + Vec2::from_angle(a) * longo * r;
                let e1 = c + Vec2::from_angle(a - 0.40) * 0.20 * r;
                let e2 = c + Vec2::from_angle(a + 0.40) * 0.20 * r;
                tri(ponta, e1, e2);
            }
            draw_circle(c.x, c.y, 0.22 * r, cor);
        }
        Glifo::Bota => {
            // Shaft with a cuff, a rounded toe, a heel and the sole.
            quad(p(-0.22, -0.66), p(0.20, -0.66), p(0.16, 0.24), p(-0.26, 0.24));
            linha(p(-0.30, -0.70), p(0.26, -0.70), 0.16);
            quad(p(-0.26, 0.14), p(0.40, 0.20), p(0.52, 0.62), p(-0.26, 0.62));
            draw_circle(p(0.44, 0.44).x, p(0.44, 0.44).y, 0.19 * r, cor);
            linha(p(-0.30, 0.74), p(0.66, 0.74), 0.12);
            // Speed lines behind it.
            for (y, l) in [(-0.40, 0.30), (-0.10, 0.42), (0.20, 0.30)] {
                linha(p(-0.42 - l, y), p(-0.42, y), 0.08);
            }
        }
        Glifo::Coracao | Glifo::CoracaoMais => {
            let k = if g == Glifo::CoracaoMais { 0.86 } else { 1.0 };
            let q = |x: f32, y: f32| p(x * k - if k < 1.0 { 0.12 } else { 0.0 }, y * k + if k < 1.0 { 0.06 } else { 0.0 });
            let lobo = 0.36 * r * k;
            let (a, b) = (q(-0.34, -0.26), q(0.34, -0.26));
            draw_circle(a.x, a.y, lobo, cor);
            draw_circle(b.x, b.y, lobo, cor);
            tri(q(-0.70, -0.14), q(0.70, -0.14), q(0.0, 0.78));
            quad(q(-0.34, -0.26), q(0.34, -0.26), q(0.34, 0.0), q(-0.34, 0.0));
            if g == Glifo::CoracaoMais {
                let m = p(0.58, 0.50);
                draw_circle(m.x, m.y, 0.34 * r, fundo);
                linha(p(0.36, 0.50), p(0.80, 0.50), 0.13);
                linha(p(0.58, 0.28), p(0.58, 0.72), 0.13);
            }
        }
        Glifo::Moeda => {
            draw_circle(c.x, c.y, 0.78 * r, cor);
            draw_circle_lines(c.x, c.y, 0.58 * r, w(0.07), fundo);
            // A crown-ish mark in the middle.
            let f = fundo;
            draw_triangle(p(-0.30, 0.20), p(0.30, 0.20), p(0.0, -0.32), f);
            draw_circle(p(-0.30, -0.18).x, p(-0.30, -0.18).y, 0.08 * r, f);
            draw_circle(p(0.30, -0.18).x, p(0.30, -0.18).y, 0.08 * r, f);
        }
        Glifo::Gota | Glifo::GotaMais => {
            let k = if g == Glifo::GotaMais { 0.84 } else { 1.0 };
            let dx = if g == Glifo::GotaMais { -0.12 } else { 0.0 };
            let base = p(dx, 0.28 * k);
            draw_circle(base.x, base.y, 0.50 * r * k, cor);
            tri(p(dx, -0.86 * k), p(dx - 0.46 * k, 0.12 * k), p(dx + 0.46 * k, 0.12 * k));
            // Shine.
            let s = p(dx - 0.18 * k, 0.24 * k);
            draw_circle(s.x, s.y, 0.12 * r * k, Color::new(1.0, 1.0, 1.0, 0.35 * cor.a));
            if g == Glifo::GotaMais {
                let m = p(0.58, 0.50);
                draw_circle(m.x, m.y, 0.34 * r, fundo);
                linha(p(0.36, 0.50), p(0.80, 0.50), 0.13);
                linha(p(0.58, 0.28), p(0.58, 0.72), 0.13);
            }
        }
        Glifo::Chama => {
            let base = p(0.0, 0.34);
            draw_circle(base.x, base.y, 0.46 * r, cor);
            tri(p(-0.46, 0.30), p(0.46, 0.30), p(0.08, -0.92));
            tri(p(-0.44, 0.20), p(-0.05, 0.10), p(-0.42, -0.42));
            tri(p(0.44, 0.20), p(0.05, 0.10), p(0.46, -0.30));
            let dentro = p(0.0, 0.42);
            draw_circle(dentro.x, dentro.y, 0.20 * r, fundo);
            draw_triangle(p(-0.18, 0.40), p(0.18, 0.40), p(0.0, 0.0), fundo);
        }
        Glifo::Relogio => {
            draw_circle_lines(c.x, c.y, 0.72 * r, w(0.13), cor);
            linha(c, p(0.0, -0.48), 0.12);
            linha(c, p(0.34, 0.12), 0.12);
            linha(p(-0.22, -0.92), p(0.22, -0.92), 0.13);
        }
        Glifo::Raio => {
            tri(p(0.18, -0.92), p(-0.46, 0.10), p(0.02, 0.10));
            tri(p(-0.02, -0.10), p(0.46, -0.10), p(-0.18, 0.92));
            quad(p(-0.10, -0.10), p(0.30, -0.10), p(0.10, 0.10), p(-0.30, 0.10));
        }
        Glifo::Mira => {
            draw_circle_lines(c.x, c.y, 0.58 * r, w(0.11), cor);
            for (a, b) in [((0.0, -0.92), (0.0, -0.34)), ((0.0, 0.34), (0.0, 0.92)), ((-0.92, 0.0), (-0.34, 0.0)), ((0.34, 0.0), (0.92, 0.0))] {
                linha(p(a.0, a.1), p(b.0, b.1), 0.11);
            }
            draw_circle(c.x, c.y, 0.12 * r, cor);
        }
        Glifo::Ancora => {
            draw_circle_lines(p(0.0, -0.66).x, p(0.0, -0.66).y, 0.16 * r, w(0.10), cor);
            linha(p(0.0, -0.50), p(0.0, 0.72), 0.13);
            linha(p(-0.36, -0.26), p(0.36, -0.26), 0.12);
            estilo::arco(p(0.0, 0.10), 0.62 * r, PI * 0.12, 0.38, w(0.12), cor);
            tri(p(-0.72, 0.16), p(-0.50, 0.40), p(-0.78, 0.44));
            tri(p(0.72, 0.16), p(0.50, 0.40), p(0.78, 0.44));
        }
        Glifo::Bau => {
            quad(p(-0.74, -0.10), p(0.74, -0.10), p(0.74, 0.66), p(-0.74, 0.66));
            // The rounded lid.
            quad(p(-0.74, -0.10), p(-0.60, -0.48), p(0.60, -0.48), p(0.74, -0.10));
            tri(p(-0.60, -0.48), p(0.0, -0.66), p(0.60, -0.48));
            estilo::traco(p(-0.74, -0.08), p(0.74, -0.08), w(0.08), fundo);
            let fecho = Rect::new(c.x - 0.12 * r, c.y - 0.18 * r, 0.24 * r, 0.30 * r);
            draw_rectangle(fecho.x, fecho.y, fecho.w, fecho.h, fundo);
            draw_circle(c.x, c.y - 0.02 * r, 0.06 * r, cor);
        }
        Glifo::Picareta => {
            linha(p(-0.58, 0.76), p(0.22, -0.26), 0.17);
            estilo::arco(p(0.14, -0.02), 0.70 * r, -PI * 0.96, 0.40, w(0.22), cor);
            tri(p(-0.70, -0.20), p(-0.52, -0.44), p(-0.86, 0.04));
            tri(p(0.66, 0.10), p(0.46, 0.30), p(0.74, 0.44));
        }
        Glifo::Cristal => {
            let pts = [p(0.0, -0.92), p(0.50, -0.28), p(0.30, 0.82), p(-0.30, 0.82), p(-0.50, -0.28)];
            tri(pts[0], pts[1], pts[4]);
            quad(pts[4], pts[1], pts[2], pts[3]);
            estilo::traco(pts[0], p(0.0, 0.82), w(0.05), Color::new(1.0, 1.0, 1.0, 0.35 * cor.a));
            estilo::traco(pts[4], pts[1], w(0.05), Color::new(0.0, 0.0, 0.0, 0.35 * cor.a));
        }
        Glifo::Lingote => {
            quad(p(-0.80, 0.50), p(0.80, 0.50), p(0.52, -0.10), p(-0.52, -0.10));
            quad(p(-0.52, -0.10), p(0.52, -0.10), p(0.34, -0.44), p(-0.34, -0.44));
            estilo::traco(p(-0.66, 0.20), p(0.66, 0.20), w(0.05), fundo);
        }
        Glifo::Trevo => {
            for i in 0..4 {
                let a = i as f32 * TAU / 4.0 - PI / 4.0;
                let q = c + Vec2::from_angle(a) * 0.36 * r + vec2(0.0, -0.08 * r);
                draw_circle(q.x, q.y, 0.30 * r, cor);
            }
            linha(p(0.0, 0.0), p(0.18, 0.90), 0.10);
        }
        Glifo::Seta => {
            tri(p(0.0, -0.92), p(-0.66, -0.12), p(0.66, -0.12));
            quad(p(-0.24, -0.14), p(0.24, -0.14), p(0.24, 0.58), p(-0.24, 0.58));
            linha(p(-0.56, 0.80), p(0.56, 0.80), 0.12);
        }
        Glifo::Asa => {
            // The dash: two forward chevrons with motion lines.
            for dx in [-0.30f32, 0.18] {
                linha(p(dx - 0.22, -0.56), p(dx + 0.24, 0.0), 0.17);
                linha(p(dx + 0.24, 0.0), p(dx - 0.22, 0.56), 0.17);
            }
            for (y, l) in [(-0.30f32, 0.28f32), (0.30, 0.28)] {
                linha(p(-0.92, y), p(-0.92 + l, y), 0.08);
            }
        }
        Glifo::Caveira => {
            draw_circle(c.x, c.y - 0.12 * r, 0.62 * r, cor);
            quad(p(-0.36, 0.20), p(0.36, 0.20), p(0.30, 0.74), p(-0.30, 0.74));
            for x in [-0.26, 0.26] {
                let o = p(x, -0.12);
                draw_circle(o.x, o.y, 0.17 * r, fundo);
            }
            draw_triangle(p(-0.08, 0.24), p(0.08, 0.24), p(0.0, 0.10), fundo);
            for x in [-0.15, 0.0, 0.15] {
                estilo::traco(p(x, 0.48), p(x, 0.74), w(0.06), fundo);
            }
        }
        Glifo::Bussola => {
            for i in 0..8 {
                let a = i as f32 * TAU / 8.0 - PI / 2.0;
                let longo = if i % 2 == 0 { 0.95 } else { 0.55 };
                let ponta = c + Vec2::from_angle(a) * longo * r;
                let e1 = c + Vec2::from_angle(a - PI / 4.0) * 0.22 * r;
                let e2 = c + Vec2::from_angle(a + PI / 4.0) * 0.22 * r;
                tri(ponta, e1, e2);
            }
            draw_circle_lines(c.x, c.y, 0.62 * r, w(0.06), cor);
            draw_circle(c.x, c.y, 0.14 * r, fundo);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn todo_efeito_tem_glifo_e_todo_glifo_esta_na_lista() {
        use std::collections::HashSet;
        let todos: HashSet<_> = TODOS.iter().copied().collect();
        assert_eq!(todos.len(), TODOS.len(), "no glyph twice");
        let mut usados = HashSet::new();
        for n in shared::arvore::nos() {
            for &(e, _) in n.efeitos() {
                usados.insert(do_efeito(e));
            }
        }
        for g in &usados {
            assert!(todos.contains(g));
        }
        // Each stat of the web reads differently: 22 stats, 22 glyphs.
        assert!(usados.len() >= 20, "{usados:?}");
        assert_eq!(da_chave(&[(Efeito::DanoCritico, 0.3), (Efeito::VidaPct, -0.15)]), Glifo::Caveira);
        assert_eq!(da_chave(&[(Efeito::DefesaPct, 0.25), (Efeito::AtaquePct, -0.08)]), Glifo::Escudo);
    }
}
