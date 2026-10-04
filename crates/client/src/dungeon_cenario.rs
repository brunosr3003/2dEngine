//! Per-dungeon architecture, props and atmosphere. Built once per floor.
//! Large scenery stays outside the server's combat radius; cellars decorate
//! their existing walls without covering corridors or combat telegraphs.
use macroquad::models::{Mesh, Vertex};
use macroquad::prelude::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Motivo {
    Wreck,
    Cargo,
    IceHull,
    Caravan,
    Storm,
    Reliquary,
    Den,
    IceCave,
    Tomb,
    Monastery,
    Forge,
    Spire,
    Arcane,
    Library,
    Mirrors,
    Echoes,
    CelestialForge,
    Throne,
    Robots,
    Subway,
    Galleon,
    Coral,
    Abyss,
    NeonTower,
    Cathedral,
    Aerie,
    Graveyard,
    Blizzard,
}
#[derive(Clone, Copy)]
struct Tema {
    motivo: Motivo,
    pedra: Color,
    detalhe: Color,
    luz: Color,
}
fn cor(r: u8, g: u8, b: u8) -> Color {
    Color::from_rgba(r, g, b, 255)
}
fn tema(id: u16) -> Option<Tema> {
    use Motivo::*;
    let (motivo, pedra, detalhe, luz) = match id {
        1 => (Wreck, (54, 67, 65), (106, 74, 43), (83, 191, 159)),
        2 => (Cargo, (102, 65, 47), (164, 113, 55), (255, 177, 76)),
        3 => (IceHull, (78, 132, 158), (170, 219, 226), (109, 210, 255)),
        4 => (Caravan, (145, 113, 65), (83, 63, 43), (232, 188, 86)),
        5 => (Storm, (67, 67, 99), (126, 112, 169), (167, 147, 255)),
        6 => (Reliquary, (175, 174, 188), (203, 166, 68), (255, 232, 148)),
        10 => (Den, (71, 83, 59), (98, 69, 44), (242, 156, 65)),
        11 => (IceCave, (83, 144, 173), (165, 221, 232), (93, 202, 255)),
        12 => (Tomb, (153, 118, 71), (104, 69, 43), (224, 170, 64)),
        13 => (Monastery, (129, 140, 137), (173, 124, 73), (109, 211, 221)),
        14 => (Forge, (64, 59, 65), (152, 81, 44), (255, 109, 37)),
        18 => (Spire, (64, 74, 106), (102, 111, 145), (133, 158, 255)),
        30 => (Arcane, (65, 49, 97), (127, 89, 165), (189, 111, 255)),
        31 => (Library, (58, 103, 112), (124, 91, 56), (77, 213, 192)),
        32 => (Mirrors, (94, 91, 138), (170, 187, 211), (156, 205, 255)),
        33 => (Echoes, (73, 58, 104), (149, 107, 161), (207, 138, 246)),
        34 => (
            CelestialForge,
            (117, 130, 158),
            (210, 175, 80),
            (255, 222, 125),
        ),
        35 => (Throne, (85, 58, 103), (208, 172, 73), (220, 168, 255)),
        7 => (Robots, (62, 75, 82), (155, 110, 41), (71, 229, 193)),
        8 => (Subway, (61, 65, 75), (144, 82, 63), (75, 206, 227)),
        19 => (Galleon, (61, 88, 91), (114, 67, 40), (87, 219, 201)),
        21 => (Coral, (151, 104, 143), (206, 141, 147), (122, 235, 224)),
        22 => (Abyss, (31, 43, 65), (64, 86, 119), (104, 154, 255)),
        9 => (NeonTower, (52, 55, 75), (99, 78, 125), (246, 104, 213)),
        16 => (Cathedral, (174, 184, 207), (224, 188, 94), (255, 235, 162)),
        17 => (Aerie, (131, 156, 172), (201, 180, 139), (132, 224, 243)),
        15 => (Graveyard, (69, 83, 105), (118, 88, 63), (130, 179, 255)),
        20 => (Blizzard, (109, 147, 175), (211, 231, 241), (168, 227, 255)),
        _ => return None,
    };
    Some(Tema {
        motivo,
        pedra: cor(pedra.0, pedra.1, pedra.2),
        detalhe: cor(detalhe.0, detalhe.1, detalhe.2),
        luz: cor(luz.0, luz.1, luz.2),
    })
}

/// Floors use the terrain's existing material system, so effects remain on
/// the real walkable surface instead of disappearing under decorative slabs.
pub fn piso(id: u16, mancha: f32) -> Option<shared::terreno::Material> {
    use shared::terreno::Material::*;
    let (base, edge) = match id {
        1 | 19 | 15 => (Tronco, RochaEscura),
        2 => (CalcadaEscura, Terra),
        3 | 11 | 20 => (Gelo, Neve),
        4 | 12 => (Arenito, Areia),
        5 | 18 => (RochaEscura, Calcada),
        6 | 16 | 17 => (Marmore, MarmoreSombra),
        10 => (Terra, Rocha),
        13 => (Calcada, Marmore),
        14 => (RochaEscura, Rocha),
        30 | 33 => (RochaEscura, CristalRoxo),
        31 => (CalcadaEscura, RochaAbissal),
        32 => (MarmoreSombra, Marmore),
        34 | 35 => (Marmore, Calcada),
        7 => (ConcretoEscuro, Concreto),
        8 => (Asfalto, ConcretoEscuro),
        21 => (Coral, Marmore),
        22 => (RochaAbissal, AreiaFunda),
        9 => (ConcretoEscuro, CalcadaEscura),
        _ => return None,
    };
    Some(if mancha > 0.84 { edge } else { base })
}

pub(crate) struct Obra {
    pub(crate) mesh: Mesh,
}
impl Obra {
    pub(crate) fn nova() -> Self {
        Self {
            mesh: Mesh {
                vertices: Vec::new(),
                indices: Vec::new(),
                texture: None,
            },
        }
    }
    pub(crate) fn caixa(&mut self, p: Vec3, s: Vec3, c: Color, yaw: f32) {
        let faces = [
            ([0, 3, 2, 1], 0.72),
            ([4, 5, 6, 7], 0.9),
            ([0, 1, 5, 4], 0.62),
            ([3, 7, 6, 2], 1.0),
            ([0, 4, 7, 3], 0.78),
            ([1, 2, 6, 5], 0.86),
        ];
        let corners = [
            vec3(-1., -1., -1.),
            vec3(1., -1., -1.),
            vec3(1., 1., -1.),
            vec3(-1., 1., -1.),
            vec3(-1., -1., 1.),
            vec3(1., -1., 1.),
            vec3(1., 1., 1.),
            vec3(-1., 1., 1.),
        ];
        let rot = Quat::from_rotation_y(yaw);
        for (face, shade) in faces {
            let base = self.mesh.vertices.len() as u16;
            for i in face {
                let v = p + rot * (corners[i] * s * 0.5);
                self.mesh.vertices.push(Vertex {
                    position: v,
                    uv: Vec2::ZERO,
                    color: [
                        (c.r * shade * 255.) as u8,
                        (c.g * shade * 255.) as u8,
                        (c.b * shade * 255.) as u8,
                        255,
                    ],
                    normal: Vec4::ZERO,
                });
            }
            self.mesh.indices.extend_from_slice(&[
                base,
                base + 1,
                base + 2,
                base,
                base + 2,
                base + 3,
            ]);
        }
    }
    fn pillar(&mut self, p: Vec3, h: f32, t: Tema, yaw: f32) {
        self.caixa(p + vec3(0., 0.25, 0.), vec3(2., 0.5, 2.), t.detalhe, yaw);
        self.caixa(p + vec3(0., h * 0.5, 0.), vec3(1.1, h, 1.1), t.pedra, yaw);
        self.caixa(
            p + vec3(0., h - 0.3, 0.),
            vec3(1.8, 0.6, 1.8),
            t.detalhe,
            yaw,
        );
        self.caixa(p + vec3(0., h + 0.15, 0.), vec3(0.6, 0.3, 0.6), t.luz, yaw);
    }
    fn arch(&mut self, p: Vec3, w: f32, h: f32, t: Tema, yaw: f32) {
        let axis = Quat::from_rotation_y(yaw) * Vec3::X;
        self.pillar(p - axis * w * 0.5, h, t, yaw);
        self.pillar(p + axis * w * 0.5, h, t, yaw);
        self.caixa(p + vec3(0., h, 0.), vec3(w + 2., 0.9, 1.5), t.pedra, yaw);
        self.caixa(
            p + vec3(0., h + 0.6, 0.),
            vec3(w * 0.7, 0.35, 1.6),
            t.detalhe,
            yaw,
        );
    }
    fn crystal(&mut self, p: Vec3, h: f32, t: Tema) {
        for i in 0..4 {
            let a = i as f32 * 1.57;
            let off = vec3(a.cos(), 0., a.sin()) * 0.7;
            let height = h * (0.55 + 0.15 * i as f32);
            for k in 0..4 {
                let width = 0.95 - (k as f32) * 0.19;
                self.caixa(
                    p + off + vec3(0., height * (k as f32 + 0.5) / 4., 0.),
                    vec3(width, height / 4., width),
                    if k == 3 { t.luz } else { t.detalhe },
                    a,
                );
            }
        }
    }
    fn prop(&mut self, p: Vec3, t: Tema, yaw: f32, variant: usize, scale: f32) {
        use Motivo::*;
        let rot = Quat::from_rotation_y(yaw);
        let at = |x: f32, y: f32, z: f32| p + rot * vec3(x, y, z) * scale;
        match t.motivo {
            Wreck | IceHull | Galleon | Graveyard => {
                // Broken ship ribs, deck beams and hanging lanterns.
                for k in 0..5 {
                    let y = (k as f32 + 0.5) * 0.8;
                    let x = 1.8 - k as f32 * 0.22;
                    for side in [-1., 1.] {
                        self.caixa(
                            at(side * x, y, 0.),
                            vec3(0.35, 0.85, 1.8) * scale,
                            t.detalhe,
                            yaw,
                        );
                    }
                }
                self.caixa(
                    at(0., 0.3, 0.),
                    vec3(4.2, 0.35, 2.4) * scale,
                    t.detalhe,
                    yaw,
                );
                self.caixa(at(0., 3.7, 0.), vec3(3., 0.3, 1.8) * scale, t.pedra, yaw);
                self.caixa(at(0., 3., 0.), vec3(0.45, 0.7, 0.45) * scale, t.luz, yaw);
                if matches!(t.motivo, IceHull) {
                    self.crystal(at(1., 0., 0.), 3. * scale, t);
                }
                if matches!(t.motivo, Galleon | Graveyard) {
                    self.caixa(
                        at(0., 5.5, 0.),
                        vec3(0.35, 10., 0.35) * scale,
                        t.detalhe,
                        yaw,
                    );
                    self.caixa(at(0., 7.5, 0.), vec3(6., 0.25, 0.3) * scale, t.detalhe, yaw);
                }
            }
            Cargo | Caravan => {
                // Distinct stacks of goods; caravan carts have axles and wheels.
                for i in 0..3 {
                    self.caixa(
                        at((i % 2) as f32 * 1.5 - 0.7, (i / 2) as f32 * 1.4 + 0.7, 0.),
                        vec3(1.4, 1.35, 1.3) * scale,
                        t.detalhe,
                        yaw,
                    );
                    self.caixa(
                        at((i % 2) as f32 * 1.5 - 0.7, (i / 2) as f32 * 1.4 + 0.7, 0.67),
                        vec3(0.16, 1.35, 0.08) * scale,
                        t.pedra,
                        yaw,
                    );
                }
                if t.motivo == Caravan {
                    for x in [-1.3, 1.3] {
                        for z in [-0.8, 0.8] {
                            self.caixa(at(x, 0.4, z), vec3(0.3, 0.9, 0.9) * scale, t.pedra, yaw);
                        }
                    }
                    self.caixa(at(0., 1., 0.), vec3(3.5, 0.3, 2.2) * scale, t.pedra, yaw);
                }
            }
            IceCave | Blizzard | Arcane | Abyss => {
                self.crystal(p, (3. + variant as f32 % 3.) * scale, t);
                if t.motivo == Blizzard {
                    self.pillar(at(0., 0., 1.5), 6. * scale, t, yaw);
                }
                if t.motivo == Abyss {
                    for k in 0..5 {
                        self.caixa(
                            at(0., k as f32 * 0.55 + 0.3, 0.),
                            vec3(2.5 - k as f32 * 0.3, 0.4, 2.5 - k as f32 * 0.3) * scale,
                            t.pedra,
                            yaw,
                        );
                    }
                }
            }
            Library => {
                self.caixa(at(0., 2.2, 0.), vec3(3.3, 4.4, 0.5) * scale, t.detalhe, yaw);
                for shelf in 0..4 {
                    let y = shelf as f32 + 0.5;
                    self.caixa(at(0., y, 0.55), vec3(3.4, 0.16, 1.) * scale, t.detalhe, yaw);
                    for book in 0..7 {
                        let c = if (book + shelf) % 3 == 0 {
                            t.luz
                        } else {
                            t.pedra
                        };
                        self.caixa(
                            at(book as f32 * 0.43 - 1.3, y + 0.48, 0.4),
                            vec3(0.3, 0.78, 0.65) * scale,
                            c,
                            yaw,
                        );
                    }
                }
            }
            Mirrors => {
                self.arch(p, 2.8 * scale, 4.8 * scale, t, yaw);
                self.caixa(
                    at(0., 2.5, 0.),
                    vec3(2.4, 4.1, 0.15) * scale,
                    cor(129, 175, 207),
                    yaw,
                );
                for k in 0..4 {
                    self.caixa(
                        at(-0.9 + k as f32 * 0.55, 1.8 + k as f32 * 0.65, 0.1),
                        vec3(0.35, 0.06, 0.02) * scale,
                        t.luz,
                        yaw,
                    );
                }
            }
            Forge | CelestialForge | Robots => {
                // Furnace mouth, pipework, anvils / robot assembly stations.
                self.caixa(at(0., 1.8, 0.), vec3(3., 3.6, 2.7) * scale, t.pedra, yaw);
                self.caixa(at(0., 1.4, 1.4), vec3(1.7, 1.8, 0.12) * scale, t.luz, yaw);
                self.caixa(
                    at(0.9, 4., -0.5),
                    vec3(0.8, 4., 0.8) * scale,
                    t.detalhe,
                    yaw,
                );
                self.caixa(
                    at(-1.8, 0.6, 0.7),
                    vec3(1.3, 1.2, 1.2) * scale,
                    t.detalhe,
                    yaw,
                );
                self.caixa(at(-1.8, 1.3, 0.7), vec3(2., 0.3, 0.8) * scale, t.pedra, yaw);
                if t.motivo == Robots {
                    self.caixa(at(1.8, 3., 0.), vec3(0.4, 3., 0.4) * scale, t.detalhe, yaw);
                    self.caixa(
                        at(0.8, 4.5, 0.),
                        vec3(2.3, 0.4, 0.4) * scale,
                        t.detalhe,
                        yaw,
                    );
                }
                if t.motivo == CelestialForge {
                    self.crystal(at(-1.8, 0., -1.8), 3.5 * scale, t);
                }
            }
            Subway => {
                self.arch(p, 6. * scale, 5. * scale, t, yaw);
                self.caixa(at(0., 3.9, 0.), vec3(3.3, 0.9, 0.2) * scale, t.luz, yaw);
                for z in [-0.8, 0.8] {
                    self.caixa(at(0., 0.1, z), vec3(7., 0.15, 0.12) * scale, t.detalhe, yaw);
                }
                self.caixa(at(-1.5, 1., 1.8), vec3(3., 1.8, 0.8) * scale, t.pedra, yaw);
            }
            Coral => {
                self.arch(p, 3.5 * scale, 5. * scale, t, yaw);
                for k in 0..5 {
                    let a = k as f32 * 1.25;
                    let q = at(a.cos() * 1.8, 0., a.sin() * 1.3);
                    self.caixa(
                        q + vec3(0., 1.5 * scale, 0.),
                        vec3(0.35, 3., 0.35) * scale,
                        t.detalhe,
                        a,
                    );
                    self.caixa(
                        q + vec3(0.4 * scale, 2.3 * scale, 0.),
                        vec3(1.1, 0.3, 0.3) * scale,
                        t.luz,
                        a,
                    );
                }
            }
            Den => {
                for side in [-1., 1.] {
                    self.caixa(
                        at(side * 1.4, 1.7, 0.),
                        vec3(0.4, 3.4, 0.4) * scale,
                        t.detalhe,
                        yaw,
                    );
                }
                self.caixa(
                    at(0., 3.5, 0.),
                    vec3(3.4, 0.35, 0.4) * scale,
                    t.detalhe,
                    yaw,
                );
                for k in 0..3 {
                    self.caixa(
                        at(k as f32 * 0.7 - 0.7, 2.5, 0.),
                        vec3(0.3, 1.3, 0.3) * scale,
                        cor(200, 190, 153),
                        yaw,
                    );
                }
            }
            Tomb => {
                self.pillar(p, 5. * scale, t, yaw);
                for k in 0..4 {
                    self.caixa(
                        at(0., 1.1 + k as f32 * 0.65, 0.57),
                        vec3(0.6, 0.16, 0.06) * scale,
                        t.luz,
                        yaw,
                    );
                }
                self.caixa(at(2., 0.5, 0.), vec3(1.2, 1., 2.9) * scale, t.detalhe, yaw);
            }
            Throne => {
                for k in 0..3 {
                    self.caixa(
                        at(0., k as f32 * 0.25 + 0.12, 0.),
                        vec3(4. - k as f32, 0.25, 3.5 - k as f32) * scale,
                        t.pedra,
                        yaw,
                    );
                }
                self.caixa(at(0., 1.1, 0.), vec3(2., 0.5, 1.4) * scale, t.detalhe, yaw);
                self.caixa(at(0., 3., -0.7), vec3(2.2, 4., 0.4) * scale, t.detalhe, yaw);
                self.crystal(at(0., 4., -0.7), 1.5 * scale, t);
            }
            Aerie => {
                self.pillar(p, 3. * scale, t, yaw);
                for k in 0..6 {
                    let a = k as f32 * 1.05;
                    self.caixa(
                        at(a.cos() * 1.2, 3.1, a.sin() * 1.2),
                        vec3(2., 0.25, 0.35) * scale,
                        t.detalhe,
                        a,
                    );
                }
                self.caixa(at(0., 3.5, 0.), vec3(0.7, 1., 0.7) * scale, t.luz, yaw);
            }
            Storm | Spire | Echoes | NeonTower => {
                self.pillar(p, (5. + variant as f32 % 3.) * scale, t, yaw);
                let height = if t.motivo == NeonTower { 8. } else { 5. };
                for k in 0..4 {
                    self.caixa(
                        at(0., height * (k as f32 + 0.5) / 4., 0.),
                        vec3(1.5, 0.13, 1.5) * scale,
                        t.luz,
                        yaw,
                    );
                }
                if t.motivo == Echoes {
                    for k in 0..3 {
                        self.caixa(
                            at((k as f32 - 1.) * 0.8, 2., 0.),
                            vec3(0.3, 3. + k as f32, 0.3) * scale,
                            t.detalhe,
                            yaw,
                        );
                    }
                }
            }
            Reliquary | Monastery | Cathedral => {
                self.arch(p, 3.8 * scale, 5.5 * scale, t, yaw);
                if t.motivo == Reliquary {
                    self.caixa(at(0., 1., 0.), vec3(1.4, 2., 1.2) * scale, t.detalhe, yaw);
                    self.crystal(at(0., 2., 0.), 1.3 * scale, t);
                }
                if t.motivo == Cathedral {
                    for k in 0..4 {
                        self.caixa(
                            at(0., 2. + k as f32 * 0.75, 0.),
                            vec3(2.5, 0.55, 0.15) * scale,
                            if k % 2 == 0 {
                                t.luz
                            } else {
                                cor(126, 147, 218)
                            },
                            yaw,
                        );
                    }
                }
                if t.motivo == Monastery {
                    self.caixa(at(0., 3.6, 0.), vec3(0.9, 1.1, 0.9) * scale, t.detalhe, yaw);
                }
            }
        }
    }
}

pub struct Cena {
    key: (u16, u8, [u32; 2]),
    mesh: crate::gpu_estatica::MalhaEstatica,
    luzes: Vec<(Vec3, f32, [f32; 3], f32)>,
    particulas: Vec<Vec3>,
    tema: Tema,
}
#[derive(Default)]
pub struct Cache {
    cena: Option<Cena>,
}
impl Cache {
    pub fn preparar(&mut self, id: u16, andar: u8, centro: Vec2, chao: &dyn Fn(f32, f32) -> f32) {
        let key = (id, andar, [centro.x.to_bits(), centro.y.to_bits()]);
        if self.cena.as_ref().is_some_and(|c| c.key == key) {
            return;
        }
        self.cena = construir(id, andar, centro, chao);
    }
    pub fn luzes(&self) {
        if let Some(c) = &self.cena {
            crate::gpu_estatica::define_luzes(&c.luzes);
        }
    }
    pub fn desenha(&self) {
        if let Some(c) = &self.cena {
            crate::gpu_estatica::desenha(
                crate::gpu_estatica::Programa::Solido {
                    recorte: Vec3::ZERO,
                    recorte_z: 0.,
                },
                [&c.mesh],
            );
            let tempo = get_time() as f32;
            for (i, p) in c.particulas.iter().enumerate() {
                let fase = (tempo * 0.35 + i as f32 * 0.731).fract();
                let descendo = matches!(
                    c.tema.motivo,
                    Motivo::IceHull | Motivo::IceCave | Motivo::Blizzard
                );
                let dy = if descendo { 1. - fase } else { fase };
                let q = *p + vec3((tempo * 0.4 + i as f32).sin() * 0.3, dy * 3., 0.);
                draw_cube(q, vec3(0.07, 0.07, 0.07), None, c.tema.luz);
            }
        }
    }
    pub fn limpar(&mut self) {
        self.cena = None;
    }
}
fn construir(id: u16, andar: u8, centro: Vec2, chao: &dyn Fn(f32, f32) -> f32) -> Option<Cena> {
    let t = tema(id)?;
    let mut obra = Obra::nova();
    let mut luzes = Vec::new();
    let mut particulas = Vec::new();
    let salas: Vec<(Vec2, f32)> = if let Some(p) = shared::planta::da(id) {
        p.salas
            .iter()
            .enumerate()
            .map(|(i, s)| {
                let q = p.centro(i);
                (vec2(q.x, q.y), s.raio)
            })
            .collect()
    } else {
        vec![(centro, 55.)]
    };
    for (s, (c, r)) in salas.iter().enumerate() {
        let y = chao(c.x, c.y);
        for k in 0..8 {
            let a = k as f32 * 0.785;
            let q = *c + vec2(a.cos(), a.sin()) * *r * 0.6;
            particulas.push(vec3(q.x, y + 0.2, q.y));
        }
        let porao = shared::planta::da(id).is_some();
        let n = if porao { 4 } else { 12 };
        for k in 0..n {
            let a = (k as f32 + 0.5) * std::f32::consts::TAU / n as f32;
            let dir = vec2(a.cos(), a.sin());
            // Planned rooms: carvings sit on the wall, outside walkable ground.
            // Open floors: architecture is beyond the 55-unit server boundary.
            let q = *c + dir * (*r + if porao { 0.8 } else { 7. });
            if let Some(p) = shared::planta::da(id) {
                if p.livre(::glam::Vec2::new(q.x, q.y), 0.0, u8::MAX) {
                    continue;
                }
            }
            let pe = vec3(q.x, chao(q.x, q.y), q.y);
            obra.prop(
                pe,
                t,
                -a - std::f32::consts::FRAC_PI_2,
                k + s,
                if porao { 1.1 } else { 1.6 },
            );
            if k % 3 == 0 {
                luzes.push((
                    pe + vec3(0., 3., 0.),
                    if porao { 16. } else { 40. },
                    [t.luz.r, t.luz.g, t.luz.b],
                    1.2,
                ));
            }
        }
        if porao
            && matches!(
                t.motivo,
                Motivo::Wreck | Motivo::Cargo | Motivo::IceHull | Motivo::Galleon
            )
        {
            // Hull beams remain overhead, leaving the actual walking surface clear.
            for z in [-0.55, 0., 0.55] {
                obra.caixa(
                    vec3(c.x, y + 4.5, c.y + z * r),
                    vec3(r * 1.8, 0.38, 0.42),
                    t.detalhe,
                    0.,
                );
                for sign in [-1., 1.] {
                    obra.caixa(
                        vec3(c.x + sign * r * 0.82, y + 3.2, c.y + z * r),
                        vec3(0.4, 2.5, 0.5),
                        t.detalhe,
                        0.,
                    );
                }
            }
        }
        if !porao {
            // A prominent landmark beyond the boss's side, with scale changing
            // between floors. The playable surface and telegraphs stay exposed.
            let pe = vec3(c.x, y, c.y + r + 22.);
            obra.prop(pe, t, 0., s, 3. + (andar as f32) * 0.3);
            for k in 0..8 {
                let a = k as f32 * std::f32::consts::TAU / 8.;
                let q = *c + vec2(a.cos(), a.sin()) * (r + 15.);
                obra.caixa(vec3(q.x, y - 1., q.y), vec3(10., 2., 10.), t.pedra, a);
            }
        }
    }
    luzes.sort_by(|a, b| {
        let d = |p: Vec3| (vec2(p.x, p.z) - centro).length_squared();
        d(a.0).total_cmp(&d(b.0))
    });
    luzes.truncate(crate::gpu_estatica::MAX_LUZES);
    Some(Cena {
        key: (id, andar, [centro.x.to_bits(), centro.y.to_bits()]),
        mesh: crate::gpu_estatica::MalhaEstatica::nova(obra.mesh),
        luzes,
        particulas,
        tema: t,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn every_dungeon_has_its_own_scenery_identity() {
        let mut motivos = std::collections::HashSet::new();
        for c in shared::dungeon::CONTEUDOS {
            let t = tema(c.id).unwrap_or_else(|| panic!("{} has no scenery", c.nome));
            assert!(
                motivos.insert(format!("{:?}", t.motivo)),
                "{} reuses another identity",
                c.nome
            );
            let mut o = Obra::nova();
            o.prop(Vec3::ZERO, t, 0., 0, 1.);
            assert!(!o.mesh.vertices.is_empty());
            assert!(o.mesh.vertices.iter().all(|v| v.position.is_finite()));
            assert!(o
                .mesh
                .indices
                .iter()
                .all(|&i| (i as usize) < o.mesh.vertices.len()));
        }
    }
}

#[cfg(debug_assertions)]
pub async fn previa() {
    let saida =
        std::env::var("MMO_PREVIA_SAIDA").unwrap_or_else(|_| "/tmp/tempest-dungeon-scenery".into());
    std::fs::create_dir_all(&saida).unwrap();
    let rt = render_target_ex(
        1280,
        800,
        RenderTargetParams {
            depth: true,
            sample_count: 1,
        },
    );
    crate::render3d::define_alvo(Some(rt.clone()));
    crate::gpu_estatica::define_neblina(Vec2::ZERO, 0., 0.);
    let solido = crate::render3d::material_solido();
    let mut terreno = crate::terreno::Terreno::novo(&shared::arena::DEF);
    let mut cache = Cache::default();
    for def in shared::dungeon::CONTEUDOS {
        let t = tema(def.id).unwrap();
        let bioma = shared::terreno::def_da_zona(def.zona).unwrap().bioma;
        terreno.tema_da_dungeon(bioma, Some(def.id));
        let p = shared::planta::da(def.id);
        let centre = p.map_or(vec2(-44., -20.), |p| {
            let q = p.centro(p.sala_da_etapa(0).unwrap());
            vec2(q.x, q.y)
        });
        terreno.atualiza(centre, 16, 6000);
        let y = terreno.altura(centre.x, centre.y);
        cache.preparar(def.id, 0, centre, &|x, z| terreno.altura(x, z));
        for (label, distance, height) in [
            (
                "room",
                if p.is_some() { 22. } else { 90. },
                if p.is_some() { 22. } else { 65. },
            ),
            ("play", 16., 12.),
        ] {
            let cam = Camera3D {
                position: vec3(centre.x + distance * 0.5, y + height, centre.y + distance),
                target: vec3(centre.x, y + 1., centre.y),
                up: Vec3::Y,
                fovy: 0.9,
                aspect: Some(1.6),
                render_target: Some(rt.clone()),
                ..Default::default()
            };
            for _ in 0..2 {
                crate::render3d::camera_padrao();
                clear_background(Color::new(
                    t.pedra.r * 0.25,
                    t.pedra.g * 0.25,
                    t.pedra.b * 0.25,
                    1.,
                ));
                set_camera(&cam);
                macroquad::material::gl_use_material(&solido);
                cache.luzes();
                terreno.desenha(&cam, Vec3::ZERO, 0.);
                cache.desenha();
                if let Some(p) = p {
                    crate::porao_planta::Desenho::de(p).desenha(p, y, 0);
                }
                draw_cube(
                    vec3(centre.x, y + 0.9, centre.y),
                    vec3(0.7, 1.8, 0.4),
                    None,
                    cor(92, 151, 214),
                );
                macroquad::material::gl_use_default_material();
                crate::render3d::camera_padrao();
                unsafe {
                    get_internal_gl().flush();
                }
                rt.texture
                    .get_texture_data()
                    .export_png(&format!("{saida}/{}-{label}.png", def.id));
                next_frame().await;
            }
        }
    }
}
