//! Zone14 voxel clouds, with soft lighting and wind displacement.
use crate::gpu_estatica::{MalhaEstatica, Programa};
use macroquad::prelude::*;

pub struct IlhasAereas {
    modelos: Vec<Vec<MalhaEstatica>>,
    /// Rock hanging under each Skyreach plateau, with its world matrix. The
    /// terrain is a heightmap and its cliffs stop at sea level, so seen from
    /// the side a plateau was a slab cut out of a cake; a tapering root of
    /// rock below is what makes it read as a floating island.
    raizes: Vec<(Vec<MalhaEstatica>, Mat4, Vec2)>,
}

impl IlhasAereas {
    pub fn nova() -> Self {
        let fontes: [&[u8]; 3] = [
            include_bytes!("../../../assets/nuvens/cloud_cumulus.vox"),
            include_bytes!("../../../assets/nuvens/cloud_small.vox"),
            include_bytes!("../../../assets/nuvens/cloud_stratus.vox"),
        ];
        let modelos = fontes
            .into_iter()
            .map(|bytes| {
                let modelos = crate::vox::parse(bytes).expect("nuvem voxel válida");
                let mut malhas = crate::vox::mesh(&modelos[0], 1.0);
                // The character mesher has a strong shadow. A cloud gets the same short
                // Zone14 ramp, preserving the original blue/white palette.
                for m in &mut malhas {
                    for face in m.vertices.chunks_exact_mut(4) {
                        let n = (face[1].position - face[0].position)
                            .cross(face[2].position - face[0].position)
                            .normalize();
                        let (antiga, suave) = if n.y > 0.5 {
                            (1.0, 1.0)
                        } else if n.y < -0.5 {
                            (0.45, 0.86)
                        } else if n.x < -0.5 {
                            (0.80, 0.94)
                        } else if n.x > 0.5 {
                            (0.64, 0.94)
                        } else if n.z > 0.5 {
                            (0.72, 0.97)
                        } else {
                            (0.88, 0.90)
                        };
                        for v in face {
                            for c in &mut v.color[..3] {
                                *c = (*c as f32 * suave / antiga).min(255.0) as u8;
                            }
                            v.normal = Vec4::ZERO;
                        }
                    }
                }
                malhas.into_iter().map(MalhaEstatica::nova).collect()
            })
            .collect();
        Self { modelos, raizes: Vec::new() }
    }

    /// With a rock root under every plateau of Skyreach (`shared::celeste`).
    pub fn com_raizes_celestes(mut self) -> Self {
        self.raizes = shared::celeste::PLATOS.iter().map(raiz).collect();
        self
    }

    /// `carregado(x, z)`: is the terrain at that world point generated? A
    /// root draws only under ground that is there.
    pub fn desenha(&self, carregado: &dyn Fn(f32, f32) -> bool) {
        // Three models share the buffers; only the matrix changes per cloud.
        // Continuous circular motion avoids popping back in at the edge.
        let tempo = get_time() as f32;
        // The clouds live hundreds of units out on purpose: the distance fog
        // would erase them, so it is off while they draw.
        let neblina = crate::gpu_estatica::neblina();
        crate::gpu_estatica::define_neblina(Vec2::ZERO, 0.0, 0.0);
        for k in 0..30 {
            let a = k as f32 * 2.39996 + tempo * 0.001;
            let r = 80.0 + (k as f32 / 30.0) * 650.0;
            let pos = vec3(a.cos() * r, -82.0 - (k % 3) as f32 * 13.0, a.sin() * r);
            let escala = 1.8 + (k % 4) as f32 * 0.35;
            let modelo = Mat4::from_scale_rotation_translation(
                Vec3::splat(escala),
                Quat::from_rotation_y(k as f32 * 1.7),
                pos,
            );
            crate::gpu_estatica::desenha_com_modelo(
                Programa::Solido {
                    recorte: Vec3::ZERO,
                    recorte_z: 0.0,
                },
                &self.modelos[k as usize % 3],
                modelo,
            );
        }
        crate::gpu_estatica::define_neblina(vec2(neblina[0], neblina[1]), neblina[2], neblina[3]);
        for (malhas, modelo, centro) in &self.raizes {
            if !carregado(centro.x, centro.y) {
                continue;
            }
            crate::gpu_estatica::desenha_com_modelo(
                Programa::Solido { recorte: Vec3::ZERO, recorte_z: 0.0 },
                malhas,
                *modelo,
            );
        }
    }
}

/// The root under one plateau: voxels of 2 u, the plateau's own ragged rim at
/// the top, tapering to a point about 0.8 radius below, with earth on top
/// and rock darkening downwards.
fn raiz(p: &shared::celeste::Plato) -> (Vec<MalhaEstatica>, Mat4, Vec2) {
    const S: f32 = 2.0;
    let n = ((p.raio * 1.3 * 2.0) / S).ceil() as usize;
    let nz = ((p.raio * 0.8) / S).ceil() as usize;
    let mut cells = vec![0u8; n * n * nz];
    let meio = n as f32 * 0.5;
    for z in 0..nz {
        // 0 at the top layer, 1 at the tip.
        let fundo = (nz - 1 - z) as f32 / (nz - 1) as f32;
        let afina = (1.0 - fundo).powf(1.5);
        for vy in 0..n {
            for vx in 0..n {
                // The mesher maps voxel x to world -x: evaluate the rim where
                // the voxel will actually land.
                let wx = -(vx as f32 + 0.5 - meio) * S;
                let wy = (vy as f32 + 0.5 - meio) * S;
                let ang = wy.atan2(wx);
                let rim = shared::celeste::raio_na_direcao(p, ang) * 0.94;
                // Jagged only further down: at the top the root has to stay
                // inside the cliff, or its earth layer sticks out as a ledge.
                let serra = 1.0 + (0.10 * (ang * 7.0 + fundo * 9.0).sin() + 0.06 * (ang * 13.0 - fundo * 5.0).sin()) * fundo.min(0.3) / 0.3 - 0.04;
                if (wx * wx + wy * wy).sqrt() < rim * afina * serra {
                    cells[vx + vy * n + z * n * n] = if fundo < 0.06 {
                        1
                    } else if fundo < 0.35 {
                        2
                    } else if fundo < 0.7 {
                        3
                    } else {
                        4
                    };
                }
            }
        }
    }
    let mut palette = [[0u8; 4]; 256];
    palette[1] = [104, 80, 56, 255];
    palette[2] = [128, 120, 110, 255];
    palette[3] = [104, 98, 92, 255];
    palette[4] = [82, 78, 76, 255];
    let model = crate::vox::VoxModel { size: [n, n, nz], cells, palette };
    let malhas = crate::vox::mesh_na_origem(&model, S, [meio, meio, 0.0])
        .into_iter()
        .map(MalhaEstatica::nova)
        .collect();
    // The top layer sits just above sea level, under the cliff's foot.
    let topo = 1.0;
    let modelo = Mat4::from_translation(vec3(p.centro.x, topo - nz as f32 * S, p.centro.y));
    (malhas, modelo, vec2(p.centro.x, p.centro.y))
}
