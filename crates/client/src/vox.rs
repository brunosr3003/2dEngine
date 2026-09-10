//! Modelos MagicaVoxel (.vox) como malha 3D.
//!
//! Em 3D o `.vox` deixa de ser fonte pra sprite e vira malha DIRETO. Isso
//! importa pra rodar em celular: depois do greedy meshing um bicho vira
//! algumas centenas de triangulos, com cor por vertice e nenhuma textura.
//!
//! ## Por que greedy meshing e' obrigatorio aqui
//!
//! A `Mesh` da macroquad indexa com `u16`, ou seja no maximo 65.535 vertices.
//! Desenhar um cubo por voxel estoura isso com folga — o `lobo.vox` tem 71 mil
//! voxels. O algoritmo junta faces coplanares vizinhas da MESMA COR num
//! retangulo so', o que costuma cortar a contagem em uma ordem de grandeza.
//!
//! ## Eixos
//!
//! MagicaVoxel e' Z-up; o mundo 3D aqui e' Y-up. A conversao acontece na
//! geracao da malha: voxel (x, y, z) vira mundo (x, z, y).

use std::collections::HashMap;

use macroquad::prelude::*;
use macroquad::models::{Mesh, Vertex};

/// Um modelo cru, do jeito que estava no arquivo.
pub struct VoxModel {
    pub size: [usize; 3],
    /// Indice de cor por voxel (0 = vazio), em ordem x + y*sx + z*sx*sy.
    pub cells: Vec<u8>,
    pub palette: [[u8; 4]; 256],
}

impl VoxModel {
    #[inline]
    fn at(&self, x: usize, y: usize, z: usize) -> u8 {
        self.cells[x + y * self.size[0] + z * self.size[0] * self.size[1]]
    }

    /// Caixa ocupada, pra centralizar o modelo sem contar espaco vazio.
    pub fn bounds(&self) -> ([usize; 3], [usize; 3]) {
        let mut lo = [usize::MAX; 3];
        let mut hi = [0usize; 3];
        for z in 0..self.size[2] {
            for y in 0..self.size[1] {
                for x in 0..self.size[0] {
                    if self.at(x, y, z) != 0 {
                        for (i, v) in [x, y, z].iter().enumerate() {
                            lo[i] = lo[i].min(*v);
                            hi[i] = hi[i].max(*v);
                        }
                    }
                }
            }
        }
        if lo[0] == usize::MAX {
            return ([0; 3], [0; 3]);
        }
        (lo, hi)
    }
}

/// Le um `.vox`. Arquivos de cena trazem varios modelos; devolve todos.
pub fn parse(data: &[u8]) -> Result<Vec<VoxModel>, String> {
    if data.len() < 8 || &data[0..4] != b"VOX " {
        return Err("nao e' um .vox".into());
    }
    let mut models: Vec<(([usize; 3]), Vec<(u8, u8, u8, u8)>)> = Vec::new();
    let mut palette = default_palette();
    let mut pending: Option<[usize; 3]> = None;

    // O formato e' RIFF-like: id(4) tam_conteudo(4) tam_filhos(4) + corpo.
    let mut stack = vec![(8usize, data.len())];
    while let Some((mut i, end)) = stack.pop() {
        while i + 12 <= end {
            let id = &data[i..i + 4];
            let n = i32::from_le_bytes(data[i + 4..i + 8].try_into().unwrap()) as usize;
            let m = i32::from_le_bytes(data[i + 8..i + 12].try_into().unwrap()) as usize;
            let body = i + 12;
            if body + n + m > end {
                break;
            }
            match id {
                b"SIZE" => {
                    let g = |o: usize| {
                        i32::from_le_bytes(data[body + o..body + o + 4].try_into().unwrap()) as usize
                    };
                    pending = Some([g(0), g(4), g(8)]);
                }
                b"XYZI" => {
                    let count =
                        i32::from_le_bytes(data[body..body + 4].try_into().unwrap()) as usize;
                    let mut vs = Vec::with_capacity(count);
                    for k in 0..count {
                        let o = body + 4 + k * 4;
                        vs.push((data[o], data[o + 1], data[o + 2], data[o + 3]));
                    }
                    if let Some(sz) = pending {
                        models.push((sz, vs));
                    }
                }
                b"RGBA" => {
                    // O chunk traz as cores 1..255; o indice 0 e' sempre vazio.
                    for k in 0..255 {
                        let o = body + k * 4;
                        palette[k + 1] = [data[o], data[o + 1], data[o + 2], data[o + 3]];
                    }
                }
                _ => {}
            }
            if m > 0 {
                stack.push((body + n, body + n + m));
            }
            i = body + n + m;
        }
    }

    Ok(models
        .into_iter()
        .map(|(size, vs)| {
            let mut cells = vec![0u8; size[0] * size[1] * size[2]];
            for (x, y, z, c) in vs {
                let (x, y, z) = (x as usize, y as usize, z as usize);
                if x < size[0] && y < size[1] && z < size[2] {
                    cells[x + y * size[0] + z * size[0] * size[1]] = c;
                }
            }
            VoxModel { size, cells, palette }
        })
        .collect())
}

fn default_palette() -> [[u8; 4]; 256] {
    let mut p = [[200u8, 200, 200, 255]; 256];
    p[0] = [0, 0, 0, 0];
    p
}

/// Brilho por direcao de face. Sem luz de verdade: so' um degrade fixo que da
/// volume e mantem o custo em zero no celular.
fn shade(axis: usize, positive: bool) -> f32 {
    match (axis, positive) {
        (2, true) => 1.00,  // topo (Z do voxel = Y do mundo)
        (2, false) => 0.45, // base
        (0, true) => 0.80,
        (0, false) => 0.64,
        (1, true) => 0.72,
        (1, false) => 0.88,
        _ => 1.0,
    }
}

/// Gera as malhas do modelo com greedy meshing.
///
/// `scale` e' o tamanho de um voxel em unidades de mundo. A origem fica no
/// centro em X/Z e na BASE em Y — o servidor manda a posicao do pe, entao a
/// malha tem que nascer apoiada nela.
///
/// Devolve mais de uma malha quando passa do limite de `u16` do indice.
pub fn mesh(model: &VoxModel, scale: f32) -> Vec<Mesh> {
    let (lo, hi) = model.bounds();
    let dims = [model.size[0], model.size[1], model.size[2]];
    // Centro em X/Y do voxel (que viram X/Z do mundo); base em Z.
    let cx = (lo[0] + hi[0] + 1) as f32 * 0.5;
    let cy = (lo[1] + hi[1] + 1) as f32 * 0.5;
    let base = lo[2] as f32;

    let mut out: Vec<Mesh> = Vec::new();
    let mut verts: Vec<Vertex> = Vec::new();
    let mut idx: Vec<u16> = Vec::new();

    let mut push_quad = |verts: &mut Vec<Vertex>,
                         idx: &mut Vec<u16>,
                         corners: [[f32; 3]; 4],
                         color: [u8; 4]| {
        let b = verts.len() as u16;
        for c in corners {
            verts.push(Vertex {
                // voxel (x, y, z) -> mundo (x, z, y)
                position: vec3(
                    (c[0] - cx) * scale,
                    (c[2] - base) * scale,
                    (c[1] - cy) * scale,
                ),
                uv: vec2(0.0, 0.0),
                color,
                normal: Vec4::ZERO,
            });
        }
        idx.extend_from_slice(&[b, b + 1, b + 2, b, b + 2, b + 3]);
    };

    for axis in 0..3usize {
        let u = (axis + 1) % 3;
        let v = (axis + 2) % 3;
        let (du, dv, da) = (dims[u], dims[v], dims[axis]);

        // Uma fatia por vez ao longo do eixo, incluindo as bordas (-1 e da-1).
        for slice in 0..=da {
            // mask[i] = Some((cor, face_positiva)) quando ha face nesta celula.
            let mut mask: Vec<Option<(u8, bool)>> = vec![None; du * dv];
            for j in 0..dv {
                for i in 0..du {
                    let mut p = [0usize; 3];
                    p[u] = i;
                    p[v] = j;
                    // Voxel de tras e da frente da fronteira.
                    let a = if slice > 0 {
                        p[axis] = slice - 1;
                        model.at(p[0], p[1], p[2])
                    } else {
                        0
                    };
                    let b = if slice < da {
                        p[axis] = slice;
                        model.at(p[0], p[1], p[2])
                    } else {
                        0
                    };
                    // Face so' existe onde solido encontra vazio.
                    mask[i + j * du] = match (a != 0, b != 0) {
                        (true, false) => Some((a, true)),
                        (false, true) => Some((b, false)),
                        _ => None,
                    };
                }
            }

            // Junta retangulos iguais: cresce em i, depois tenta crescer em j.
            let mut j = 0;
            while j < dv {
                let mut i = 0;
                while i < du {
                    let cur = mask[i + j * du];
                    if cur.is_none() {
                        i += 1;
                        continue;
                    }
                    let mut w = 1;
                    while i + w < du && mask[i + w + j * du] == cur {
                        w += 1;
                    }
                    let mut h = 1;
                    'grow: while j + h < dv {
                        for k in 0..w {
                            if mask[i + k + (j + h) * du] != cur {
                                break 'grow;
                            }
                        }
                        h += 1;
                    }

                    let (color_idx, positive) = cur.unwrap();
                    let rgba = model.palette[color_idx as usize];
                    let k = shade(axis, positive);
                    let color = [
                        (rgba[0] as f32 * k) as u8,
                        (rgba[1] as f32 * k) as u8,
                        (rgba[2] as f32 * k) as u8,
                        rgba[3],
                    ];

                    // Monta os 4 cantos do retangulo no espaco do voxel.
                    let corner = |di: usize, dj: usize| -> [f32; 3] {
                        let mut c = [0f32; 3];
                        c[axis] = slice as f32;
                        c[u] = (i + di) as f32;
                        c[v] = (j + dj) as f32;
                        c
                    };
                    let mut quad = [
                        corner(0, 0),
                        corner(w, 0),
                        corner(w, h),
                        corner(0, h),
                    ];
                    // Inverte a ordem quando a face aponta pro outro lado, pra
                    // manter o winding consistente.
                    if !positive {
                        quad.swap(1, 3);
                    }

                    // Fecha a malha no limite REAL de desenho: a macroquad
                    // corta em 10.000 vertices por chamada (o `u16` do indice
                    // aguentaria 65.535, mas o desenho nao). Lote menor
                    // tambem e' o que GPU de celular prefere.
                    if verts.len() + 4 > 2_000 {
                        out.push(Mesh {
                            vertices: std::mem::take(&mut verts),
                            indices: std::mem::take(&mut idx),
                            texture: None,
                        });
                    }
                    push_quad(&mut verts, &mut idx, quad, color);

                    for dj in 0..h {
                        for di in 0..w {
                            mask[i + di + (j + dj) * du] = None;
                        }
                    }
                    i += w;
                }
                j += 1;
            }
        }
    }

    if !verts.is_empty() {
        out.push(Mesh { vertices: verts, indices: idx, texture: None });
    }
    out
}

/// Cache de modelos ja carregados e transformados em malha.
#[derive(Default)]
pub struct VoxCache {
    meshes: HashMap<String, Vec<Mesh>>,
}

impl VoxCache {
    /// Malha ja carregada. O desenho e' sincrono, entao a carga acontece
    /// antes (`load`) e aqui so' se consulta.
    pub fn peek(&self, name: &str) -> Option<&Vec<Mesh>> {
        self.meshes.get(name)
    }

    /// Carrega `<raiz>/<nome>.vox`. `None` quando o arquivo nao existe — mob
    /// sem modelo cai no desenho de fallback em vez de derrubar o cliente.
    pub async fn load(&mut self, name: &str, scale: f32) -> Option<&Vec<Mesh>> {
        if !self.meshes.contains_key(name) {
            let root = std::env::var("MMO_VOX").unwrap_or_else(|_| "assets/vox".into());
            let path = format!("{root}/{name}.vox");
            let bytes = macroquad::file::load_file(&path).await.ok()?;
            let models = parse(&bytes).ok()?;
            let m = models.into_iter().max_by_key(|m| {
                m.cells.iter().filter(|c| **c != 0).count()
            })?;
            let meshes = mesh(&m, scale);
            let tris: usize = meshes.iter().map(|x| x.indices.len() / 3).sum();
            println!("[vox] {name}: {} malha(s), {tris} triangulos", meshes.len());
            self.meshes.insert(name.to_string(), meshes);
        }
        self.meshes.get(name)
    }
}
