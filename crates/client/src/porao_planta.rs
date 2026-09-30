//! What a Porão floor plan (`shared::planta`) needs on top of the terrain:
//! torches and the iron gates still shut.
//!
//! The floor and the walls are TERRAIN now — carved into the Arena islet by
//! the same `Gerador` the server walks on (`planta::Planta::bloco`). The first
//! version drew them here as boxes over the islet's grass, and the floor read
//! as paint: auras and boss telegraphs, drawn on the real ground, ended up
//! under the slabs. The owner saw both.
//!
//! What stays here is what isn't ground: the gates open and shut with the
//! run, and a torch is a prop.
use macroquad::prelude::*;
use shared::planta::{Planta, LARGURA};

/// The shared crate's glam and macroquad's are different versions.
fn g(v: Vec2) -> ::glam::Vec2 {
    ::glam::Vec2::new(v.x, v.y)
}

fn mq(v: ::glam::Vec2) -> Vec2 {
    vec2(v.x, v.y)
}

/// Everything static about one plan, built once.
pub struct Desenho {
    pub conteudo: u16,
    /// Torch posts, on the floor just inside a room's wall.
    pub tochas: Vec<Vec2>,
}

impl Desenho {
    pub fn de(p: &Planta) -> Self {
        // Torches on the walls of every room, where no corridor opens. Set
        // in from the wall by more than a cave's rock bumps reach, or the
        // post stands inside the rock.
        let mut tochas = Vec::new();
        for s in p.salas {
            let passos = if s.raio >= 10.0 { 8 } else { 4 };
            let centro = mq(p.ancora + s.centro);
            for k in 0..passos {
                let a = std::f32::consts::FRAC_PI_4 + k as f32 * std::f32::consts::TAU / passos as f32;
                let dir = vec2(a.cos(), a.sin());
                if p.livre(g(centro + dir * (s.raio + 1.0)), 0.0, u8::MAX) {
                    continue; // a corridor mouth
                }
                tochas.push(centro + dir * (s.raio - 1.3));
            }
        }
        Self {
            conteudo: p.conteudo,
            tochas,
        }
    }

    /// Draws the torches and the gates still shut at `andar`, at floor height
    /// `y`. Call with the solid material on.
    pub fn desenha(&self, p: &Planta, y: f32, andar: u8) {
        let t = get_time() as f32;
        for (i, q) in self.tochas.iter().enumerate() {
            let chama = 0.75 + 0.25 * (t * 7.0 + i as f32 * 1.7).sin();
            let pe = vec3(q.x, y, q.y);
            draw_cube(pe + vec3(0.0, 0.75, 0.0), vec3(0.16, 1.5, 0.16), None, Color::from_rgba(70, 48, 30, 255));
            draw_cube(
                pe + vec3(0.0, 1.62, 0.0),
                vec3(0.3, 0.34 * chama, 0.3),
                None,
                Color::new(1.0, 0.62 * chama, 0.18, 1.0),
            );
        }
        for (onde, dir) in p.portoes_fechados(andar) {
            desenha_portao(vec3(onde.x, y, onde.y), mq(dir), t);
        }
    }
}

/// An iron gate across a corridor mouth: bars, a beam on top, and a red glow
/// at the foot so a shut gate reads even from the far side of the room.
fn desenha_portao(p: Vec3, dir: Vec2, t: f32) {
    let ferro = Color::from_rgba(58, 60, 70, 255);
    let largura = LARGURA + 1.2;
    // Across the corridor: perpendicular to `dir`, which is always an axis.
    let ao_longo_x = dir.y.abs() > dir.x.abs();
    let barras = 7;
    for k in 0..barras {
        let d = -largura * 0.5 + largura * (k as f32 + 0.5) / barras as f32;
        let off = if ao_longo_x { vec3(d, 0.0, 0.0) } else { vec3(0.0, 0.0, d) };
        draw_cube(p + off + vec3(0.0, 1.05, 0.0), vec3(0.14, 2.1, 0.14), None, ferro);
    }
    let viga = if ao_longo_x {
        vec3(largura, 0.26, 0.3)
    } else {
        vec3(0.3, 0.26, largura)
    };
    draw_cube(p + vec3(0.0, 2.15, 0.0), viga, None, Color::from_rgba(44, 46, 54, 255));
    let a = 0.45 + 0.25 * (t * 2.5).sin();
    let brilho = if ao_longo_x {
        vec3(largura, 0.04, 0.9)
    } else {
        vec3(0.9, 0.04, largura)
    };
    draw_cube(p + vec3(0.0, 0.07, 0.0), brilho, None, Color::new(0.85, 0.18, 0.12, a));
}

/// One drawing per plan, built the first time it's needed.
#[derive(Default)]
pub struct Cache {
    desenhos: Vec<Desenho>,
}

impl Cache {
    pub fn de(&mut self, p: &Planta) -> &Desenho {
        if let Some(i) = self.desenhos.iter().position(|d| d.conteudo == p.conteudo) {
            return &self.desenhos[i];
        }
        self.desenhos.push(Desenho::de(p));
        self.desenhos.last().expect("just pushed")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// EVERY TORCH STANDS ON FLOOR: walkable, and clear of a cave's rock
    /// bumps — a torch inside the wall is a torch nobody sees.
    #[test]
    fn toda_tocha_fica_no_chao() {
        for p in &shared::planta::PLANTAS {
            let d = Desenho::de(p);
            assert!(!d.tochas.is_empty(), "plan {} has no torches", p.conteudo);
            for q in &d.tochas {
                assert!(
                    p.livre(g(*q), 0.3, u8::MAX),
                    "plan {}: a torch at {q:?} is in the wall",
                    p.conteudo
                );
                assert!(
                    p.profundidade(g(*q)) >= 1.0,
                    "plan {}: a torch at {q:?} is inside the rock bumps",
                    p.conteudo
                );
            }
        }
    }
}
