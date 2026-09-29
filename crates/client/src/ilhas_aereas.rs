//! Zone14 voxel clouds, with soft lighting and wind displacement.
use crate::gpu_estatica::{MalhaEstatica, Programa};
use macroquad::prelude::*;

pub struct IlhasAereas {
    modelos: Vec<Vec<MalhaEstatica>>,
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
        Self { modelos }
    }

    pub fn desenha(&self) {
        // Three models share the buffers; only the matrix changes per cloud.
        // Continuous circular motion avoids popping back in at the edge.
        let tempo = get_time() as f32;
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
    }
}
