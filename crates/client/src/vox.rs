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
                    // A ORDEM dos cantos decide o que a placa de video
                    // considera frente. Com o descarte de face de costas
                    // ligado, malha enrolada ao contrario nao mostra um erro
                    // obvio: ela mostra o modelo pelo AVESSO — some a frente e
                    // aparece o interior das costas. Fica estranho sem parecer
                    // defeito.
                    //
                    // A ordem sai da conta e nao da mao. Antes era um
                    // `swap` condicional escrito de cabeca, e ele estava
                    // invertido: medido pelo volume com sinal, os tres modelos
                    // do jogo estavam todos pelo avesso.
                    //
                    // A normal pretendida esta' no eixo da fatia, com o sinal
                    // do lado exposto — e a comparacao e' feita em coordenada
                    // de MUNDO, porque e' la' que o voxel (x, y, z) vira
                    // (x, z, y).
                    let mundo = |c: [f32; 3]| vec3(c[0], c[2], c[1]);
                    let mut n = [0f32; 3];
                    n[axis] = if positive { 1.0 } else { -1.0 };
                    let n = mundo(n);
                    let geom = (mundo(quad[1]) - mundo(quad[0]))
                        .cross(mundo(quad[2]) - mundo(quad[0]));
                    if geom.dot(n) < 0.0 {
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

#[cfg(test)]
mod testes {
    use super::*;

    /// TODO modelo tem que olhar pro `+Y` do voxel, que a malha manda pro
    /// `+Z` do mundo — que e' pra onde o codigo de rotacao assume que a
    /// frente aponta.
    ///
    /// O teste le' os ARQUIVOS que vao pro jogo. Conferir a leitura que EU
    /// fiz deles nao serviria de nada: foi justamente ela que errou primeiro,
    /// e so' o olho azul do modelo desmentiu.
    ///
    /// O `player.vox` chegou virado — feito olhando pra camera padrao do
    /// MagicaVoxel — e o boneco andava de costas. Foi girado no arquivo com
    /// `tools/voxrender/voxgira.py`, e nao contornado no cliente: lista de
    /// excecao em convencao e' o tipo de coisa que ninguem lembra de manter.
    #[test]
    fn todo_modelo_olha_pra_frente() {
        // (arquivo, cor do detalhe do rosto)
        //   jogador: o olho azul, unico azul saturado do modelo
        //   lobo:    os dentes, o unico branco puro
        for (arquivo, marca) in [
            ("player.vox", Detalhe::Azul),
            ("pirate_captain.vox", Detalhe::Azul),
            ("lobo.vox", Detalhe::Branco),
            ("lobo_pequeno.vox", Detalhe::Branco),
        ] {
            let bytes = ler(arquivo);
            // O MESMO modelo que o cliente carrega: o de mais voxels. Um
            // `.vox` pode trazer varios, e testar outro seria testar o que
            // nao vai pro jogo.
            let m = parse(&bytes)
                .expect("vox valido")
                .into_iter()
                .max_by_key(|m| m.cells.iter().filter(|c| **c != 0).count())
                .expect("um modelo");
            let (sx, sy) = (m.size[0], m.size[1]);

            let combina = |c: [u8; 4]| match marca {
                Detalhe::Azul => c[2] > 180 && c[0] < 80,
                Detalhe::Branco => c[0] > 240 && c[1] > 240 && c[2] > 240,
            };
            let ocupado: Vec<usize> =
                (0..m.cells.len()).filter(|&i| m.cells[i] != 0).collect();
            // So' a cabeca: o terco de cima do que esta' OCUPADO, e nao da
            // caixa. A caixa do lobo e' 128 de lado com o bicho ocupando uma
            // parte dela; medir pela caixa poe o corte no lugar errado.
            let zs: Vec<usize> = ocupado.iter().map(|&i| i / (sx * sy)).collect();
            let (z0, z1) = (
                zs.iter().copied().min().unwrap(),
                zs.iter().copied().max().unwrap(),
            );
            let alto = z0 + (z1 - z0) * 62 / 100;
            let ys_cabeca: Vec<usize> = ocupado
                .iter()
                .filter(|&&i| i / (sx * sy) >= alto)
                .map(|&i| (i / sx) % sy)
                .collect();
            let centro = (ys_cabeca.iter().copied().min().unwrap()
                + ys_cabeca.iter().copied().max().unwrap()) as f32
                / 2.0;
            let rosto: Vec<f32> = ocupado
                .iter()
                .filter(|&&i| i / (sx * sy) >= alto && combina(m.palette[m.cells[i] as usize]))
                .map(|&i| ((i / sx) % sy) as f32)
                .collect();
            assert!(!rosto.is_empty(), "{arquivo}: nao achei o detalhe do rosto");
            let media = rosto.iter().sum::<f32>() / rosto.len() as f32;
            assert!(
                media > centro,
                "{arquivo}: rosto em Y={media:.1} contra centro da cabeca {centro:.1} — \
                 modelo virado, o boneco vai andar de costas. Gire com \
                 `tools/voxrender/voxgira.py {arquivo}`."
            );
        }
    }

    enum Detalhe {
        Azul,
        Branco,
    }

    fn ler(arquivo: &str) -> Vec<u8> {
        let a = format!("../../assets/vox/{arquivo}");
        std::fs::read(&a)
            .or_else(|_| std::fs::read(format!("assets/vox/{arquivo}")))
            .unwrap_or_else(|e| panic!("{a}: {e}"))
    }
}

#[cfg(test)]
mod testes_orientacao {
    use super::*;

    /// As faces de um modelo voxel apontam pra FORA.
    ///
    /// Com o descarte de face de costas ligado, enrolamento invertido nao
    /// desenha um erro visivel: ele desenha o modelo pelo AVESSO — some a
    /// frente e aparece o interior das costas. Fica estranho sem parecer bug.
    ///
    /// A conta e' o volume com sinal (`v0 · (v1 × v2)` somado sobre os
    /// triangulos). Num solido fechado ele e' o volume de verdade quando as
    /// normais apontam pra fora, e o negativo dele quando apontam pra dentro.
    /// Nao depende de escolher faces na mao nem de saber onde fica o rosto.
    #[test]
    fn os_modelos_apontam_pra_fora() {
        for arquivo in ["player.vox", "lobo.vox", "lobo_pequeno.vox"] {
            let bytes = std::fs::read(format!("../../assets/vox/{arquivo}"))
                .or_else(|_| std::fs::read(format!("assets/vox/{arquivo}")))
                .expect(arquivo);
            let m = parse(&bytes)
                .expect("vox valido")
                .into_iter()
                .max_by_key(|m| m.cells.iter().filter(|c| **c != 0).count())
                .expect("um modelo");
            let malhas = mesh(&m, 1.0);
            let mut volume = 0.0f64;
            let mut tris = 0;
            for malha in &malhas {
                for t in malha.indices.chunks(3) {
                    let a = malha.vertices[t[0] as usize].position;
                    let b = malha.vertices[t[1] as usize].position;
                    let c = malha.vertices[t[2] as usize].position;
                    volume += a.dot(b.cross(c)) as f64;
                    tris += 1;
                }
            }
            // O volume com sinal de uma malha FECHADA e virada pra fora e' o
            // volume de verdade — e a escala 1 faz cada voxel valer 1. Entao
            // ele tem que dar exatamente a contagem de voxels cheios.
            //
            // Isto prova as duas coisas de uma vez: negativo seria avesso,
            // diferente seria buraco. E' bem mais forte que "maior que zero".
            let cheios = m.cells.iter().filter(|c| **c != 0).count() as f64;
            let volume = volume / 6.0;
            assert!(tris > 100, "{arquivo}: so' {tris} triangulos");
            assert!(
                (volume - cheios).abs() < 0.5,
                "{arquivo}: volume {volume:.0} contra {cheios:.0} voxels — \
                 {} ",
                if volume < 0.0 { "malha pelo avesso" } else { "malha com buraco" }
            );
            println!("{arquivo}: {tris} triangulos, volume {volume:.0} = {cheios:.0} voxels");
        }
    }
}
