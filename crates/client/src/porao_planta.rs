//! Drawing a Porão floor plan (`shared::planta`): stone floor, walls, torches
//! and the iron gates still shut.
//!
//! The server enforces the walls; this only draws the same data, so what the
//! player sees is what stops them. Drawn only inside a planned run on the Arena.
//!
//! The walls are built once per plan: a grid over the plan marks the ground
//! that is NOT walkable but touches ground that is, and the marked cells are
//! merged into as few boxes as they fit. Axis-aligned corridors (a rule the
//! plan's tests hold) are what make long straight boxes possible.
use macroquad::prelude::*;
use shared::planta::{Planta, ANCORA, LARGURA};

/// The shared crate's glam and macroquad's are different versions.
fn g(v: Vec2) -> ::glam::Vec2 {
    ::glam::Vec2::new(v.x, v.y)
}

fn mq(v: ::glam::Vec2) -> Vec2 {
    vec2(v.x, v.y)
}

/// Grid cell of the wall builder, in world units.
const CELULA: f32 = 0.5;
/// How far the wall reaches out from the walkable edge.
const ESPESSURA: i32 = 3;
/// Wall height. Low enough that the camera, tilted down to its 27°, still sees
/// the character past a south wall; high enough to read as a wall and not a
/// kerb.
pub const ALTURA_DA_PAREDE: f32 = 1.9;

/// A box: centre and size, in the plan's local coordinates (y = height).
#[derive(Clone, Copy, Debug)]
pub struct Caixa {
    pub centro: Vec3,
    pub tamanho: Vec3,
    pub tom: f32,
}

/// Everything static about one plan, built once.
pub struct Desenho {
    pub conteudo: u16,
    pub paredes: Vec<Caixa>,
    pub piso: Vec<Caixa>,
    /// Torch posts: (position on the floor, the way it faces into the room).
    pub tochas: Vec<Vec2>,
}

/// Walkable ignoring gates (all open): the SHAPE of the cellar.
fn chao(p: &Planta, q: Vec2) -> bool {
    p.livre(g(q), 0.0, u8::MAX)
}

/// Merges marked grid cells into boxes: runs along x, then runs of equal rows
/// stacked along z.
fn juntar(marcas: &[Vec<bool>], x0: f32, z0: f32) -> Vec<(f32, f32, f32, f32)> {
    let linhas = marcas.len();
    let colunas = marcas.first().map_or(0, |l| l.len());
    let mut usada = vec![vec![false; colunas]; linhas];
    let mut caixas = Vec::new();
    for z in 0..linhas {
        let mut x = 0;
        while x < colunas {
            if !marcas[z][x] || usada[z][x] {
                x += 1;
                continue;
            }
            let mut fim = x;
            while fim + 1 < colunas && marcas[z][fim + 1] && !usada[z][fim + 1] {
                fim += 1;
            }
            // Grow downwards while the next row has exactly this run free.
            let mut baixo = z;
            'cresce: while baixo + 1 < linhas {
                for k in x..=fim {
                    if !marcas[baixo + 1][k] || usada[baixo + 1][k] {
                        break 'cresce;
                    }
                }
                baixo += 1;
            }
            for linha in usada.iter_mut().take(baixo + 1).skip(z) {
                for u in linha.iter_mut().take(fim + 1).skip(x) {
                    *u = true;
                }
            }
            caixas.push((
                x0 + x as f32 * CELULA,
                z0 + z as f32 * CELULA,
                x0 + (fim + 1) as f32 * CELULA,
                z0 + (baixo + 1) as f32 * CELULA,
            ));
            x = fim + 1;
        }
    }
    caixas
}

/// A stable 0..1 tone for a box, so neighbouring stones differ a little.
fn tom(a: f32, b: f32) -> f32 {
    let h = ((a * 12.9898 + b * 78.233).sin() * 43_758.547).fract();
    h.abs()
}

impl Desenho {
    pub fn de(p: &Planta) -> Self {
        let alcance = p.alcance() + (ESPESSURA as f32 + 2.0) * CELULA;
        let n = (alcance * 2.0 / CELULA).ceil() as usize;
        let (x0, z0) = (ANCORA.x - alcance, ANCORA.y - alcance);
        let meio = |x: usize, z: usize| {
            Vec2::new(
                x0 + (x as f32 + 0.5) * CELULA,
                z0 + (z as f32 + 0.5) * CELULA,
            )
        };
        // A cell TOUCHES the floor if any corner or its middle is walkable,
        // and is CLEAR of it if none is. The floor covers what touches (so no
        // grass shows at the edge), the wall only what is clear (so no wall
        // stands where a body can be).
        let toca = |x: usize, z: usize| {
            let c = meio(x, z);
            let m = CELULA * 0.5;
            [
                c,
                c + vec2(m, m),
                c + vec2(-m, m),
                c + vec2(m, -m),
                c + vec2(-m, -m),
            ]
            .into_iter()
            .any(|q| chao(p, q))
        };
        let andavel: Vec<Vec<bool>> = (0..n)
            .map(|z| (0..n).map(|x| toca(x, z)).collect())
            .collect();
        let parede: Vec<Vec<bool>> = (0..n)
            .map(|z| {
                (0..n)
                    .map(|x| {
                        if andavel[z][x] {
                            return false;
                        }
                        let (x, z) = (x as i32, z as i32);
                        (-ESPESSURA..=ESPESSURA).any(|dz| {
                            (-ESPESSURA..=ESPESSURA).any(|dx| {
                                let (a, b) = (x + dx, z + dz);
                                a >= 0
                                    && b >= 0
                                    && (a as usize) < n
                                    && (b as usize) < n
                                    && andavel[b as usize][a as usize]
                            })
                        })
                    })
                    .collect()
            })
            .collect();
        let paredes = juntar(&parede, x0, z0)
            .into_iter()
            .map(|(a, b, c, d)| Caixa {
                centro: vec3((a + c) * 0.5, ALTURA_DA_PAREDE * 0.5, (b + d) * 0.5),
                tamanho: vec3(c - a, ALTURA_DA_PAREDE, d - b),
                tom: tom(a, b),
            })
            .collect();
        let piso = juntar(&andavel, x0, z0)
            .into_iter()
            .map(|(a, b, c, d)| Caixa {
                // Lifted clear of the ground: at the same height the grass
                // fights the stone for every pixel.
                centro: vec3((a + c) * 0.5, 0.08, (b + d) * 0.5),
                tamanho: vec3(c - a, 0.08, d - b),
                tom: tom(b, a),
            })
            .collect();
        // Torches on the walls of every room, where no corridor opens.
        let mut tochas = Vec::new();
        for s in p.salas {
            let passos = if s.raio >= 10.0 { 8 } else { 4 };
            for k in 0..passos {
                let a = std::f32::consts::FRAC_PI_4 + k as f32 * std::f32::consts::TAU / passos as f32;
                let dir = Vec2::new(a.cos(), a.sin());
                let centro = mq(ANCORA + s.centro);
                let fora = centro + dir * (s.raio + 1.0);
                if chao(p, fora) {
                    continue; // a corridor mouth
                }
                tochas.push(centro + dir * (s.raio - 0.2));
            }
        }
        Self {
            conteudo: p.conteudo,
            paredes,
            piso,
            tochas,
        }
    }

    /// Draws the plan at ground height `y`, with the gates still shut at
    /// `andar`. Call with the solid material on.
    pub fn desenha(&self, p: &Planta, y: f32, andar: u8) {
        let t = get_time() as f32;
        let base = vec3(0.0, y, 0.0);
        // One tone for the whole floor: the merged slabs have arbitrary
        // shapes, and toning each one drew stripes, not stones.
        let piso = Color::new(0.31, 0.30, 0.285, 1.0);
        for c in &self.piso {
            draw_cube(base + c.centro, c.tamanho, None, piso);
        }
        for c in &self.paredes {
            let v = 0.36 + 0.08 * c.tom;
            draw_cube(base + c.centro, c.tamanho, None, Color::new(v * 0.95, v * 0.93, v, 1.0));
            // A darker cap, so the top edge reads from above.
            draw_cube(
                base + c.centro + vec3(0.0, c.tamanho.y * 0.5 + 0.04, 0.0),
                vec3(c.tamanho.x, 0.08, c.tamanho.z),
                None,
                Color::new(v * 0.6, v * 0.58, v * 0.62, 1.0),
            );
        }
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

    /// THE WALLS ARE FEW BOXES, NOT A CUBE PER CELL: they're drawn every frame,
    /// on phones too.
    #[test]
    fn as_paredes_cabem_em_poucas_caixas() {
        for p in &shared::planta::PLANTAS {
            let d = Desenho::de(p);
            assert!(!d.paredes.is_empty() && !d.piso.is_empty());
            assert!(
                d.paredes.len() + d.piso.len() <= 700,
                "plan {}: {} walls + {} floor boxes",
                p.conteudo,
                d.paredes.len(),
                d.piso.len()
            );
            assert!(!d.tochas.is_empty(), "plan {} has no torches", p.conteudo);
        }
    }

    /// NO WALL BOX STANDS WHERE THE SERVER LETS A BODY WALK — the drawing and
    /// the collision are the same cellar.
    #[test]
    fn parede_desenhada_nunca_esta_no_chao_andavel() {
        for p in &shared::planta::PLANTAS {
            let d = Desenho::de(p);
            for c in &d.paredes {
                let q = Vec2::new(c.centro.x, c.centro.z);
                let meia = Vec2::new(c.tamanho.x, c.tamanho.z) * 0.5 - Vec2::splat(0.05);
                for canto in [
                    q,
                    q + meia,
                    q - meia,
                    q + Vec2::new(meia.x, -meia.y),
                    q + Vec2::new(-meia.x, meia.y),
                ] {
                    assert!(
                        !p.livre(g(canto), shared::ENTITY_RADIUS, u8::MAX),
                        "plan {}: a wall box covers walkable ground at {canto:?}",
                        p.conteudo
                    );
                }
            }
        }
    }
}
