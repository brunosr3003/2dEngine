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
    parse_nomeado(data).map(|v| v.into_iter().map(|(_, m)| m).collect())
}

/// Le um `.vox` com o NOME de cada modelo.
///
/// O nome mora no grafo de cena do MagicaVoxel: um `nTRN` (transformacao) tem
/// o atributo `_name` e aponta pra um `nSHP` (forma), que aponta pro modelo
/// pela ordem em que ele aparece no arquivo. E' assim que o rig do personagem
/// sabe qual modelo e' `braco_d` e qual e' `canela_e` — ver
/// docs/character create.md. Modelo sem nome volta com nome vazio.
pub fn parse_nomeado(data: &[u8]) -> Result<Vec<(String, VoxModel)>, String> {
    if data.len() < 8 || &data[0..4] != b"VOX " {
        return Err("nao e' um .vox".into());
    }
    let mut models: Vec<([usize; 3], Vec<(u8, u8, u8, u8)>)> = Vec::new();
    let mut palette = default_palette();
    let mut pending: Option<[usize; 3]> = None;
    // nTRN: filho -> nome.  nSHP: no' -> modelos.
    let mut nome_do_filho: std::collections::HashMap<i32, String> = Default::default();
    let mut modelos_do_no: std::collections::HashMap<i32, Vec<i32>> = Default::default();

    let i32_em = |o: usize| i32::from_le_bytes(data[o..o + 4].try_into().unwrap());
    // DICT: n, e n pares de STRING (tamanho + bytes). Devolve o mapa e onde parou.
    let dict = |mut o: usize| -> (std::collections::HashMap<String, String>, usize) {
        let mut d = std::collections::HashMap::new();
        let n = i32_em(o).max(0) as usize;
        o += 4;
        for _ in 0..n {
            let lk = i32_em(o).max(0) as usize;
            let k = String::from_utf8_lossy(&data[o + 4..o + 4 + lk]).into_owned();
            o += 4 + lk;
            let lv = i32_em(o).max(0) as usize;
            let v = String::from_utf8_lossy(&data[o + 4..o + 4 + lv]).into_owned();
            o += 4 + lv;
            d.insert(k, v);
        }
        (d, o)
    };

    // O formato e' RIFF-like: id(4) tam_conteudo(4) tam_filhos(4) + corpo.
    let mut stack = vec![(8usize, data.len())];
    while let Some((mut i, end)) = stack.pop() {
        while i + 12 <= end {
            let id = &data[i..i + 4];
            let n = i32_em(i + 4) as usize;
            let m = i32_em(i + 8) as usize;
            let body = i + 12;
            if body + n + m > end {
                break;
            }
            match id {
                b"SIZE" => {
                    let g = |o: usize| i32_em(body + o) as usize;
                    pending = Some([g(0), g(4), g(8)]);
                }
                b"XYZI" => {
                    let count = i32_em(body) as usize;
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
                b"nTRN" => {
                    let (attrs, o) = dict(body + 4);
                    let filho = i32_em(o);
                    if let Some(nome) = attrs.get("_name") {
                        nome_do_filho.insert(filho, nome.clone());
                    }
                }
                b"nSHP" => {
                    let no = i32_em(body);
                    let (_, mut o) = dict(body + 4);
                    let k = i32_em(o).max(0) as usize;
                    o += 4;
                    let mut ids = Vec::with_capacity(k);
                    for _ in 0..k {
                        ids.push(i32_em(o));
                        o = dict(o + 4).1;
                    }
                    modelos_do_no.insert(no, ids);
                }
                _ => {}
            }
            if m > 0 {
                stack.push((body + n, body + n + m));
            }
            i = body + n + m;
        }
    }

    let mut nomes = vec![String::new(); models.len()];
    for (no, ids) in &modelos_do_no {
        if let Some(nome) = nome_do_filho.get(no) {
            for &id in ids {
                if let Some(slot) = nomes.get_mut(id.max(0) as usize) {
                    *slot = nome.clone();
                }
            }
        }
    }

    Ok(models
        .into_iter()
        .zip(nomes)
        .map(|((size, vs), nome)| {
            let mut cells = vec![0u8; size[0] * size[1] * size[2]];
            for (x, y, z, c) in vs {
                let (x, y, z) = (x as usize, y as usize, z as usize);
                if x < size[0] && y < size[1] && z < size[2] {
                    cells[x + y * size[0] + z * size[0] * size[1]] = c;
                }
            }
            (nome, VoxModel { size, cells, palette })
        })
        .collect())
}

/// O modelo com os quatro indices do tier (241-244) trocados por `cores`.
pub fn na_cor(base: &VoxModel, cores: &[[u8; 3]; 4]) -> VoxModel {
    let mut palette = base.palette;
    for (k, c) in cores.iter().enumerate() {
        palette[241 + k] = [c[0], c[1], c[2], 255];
    }
    VoxModel { size: base.size, cells: base.cells.clone(), palette }
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
    // Centro em X/Y do voxel (que viram X/Z do mundo); base em Z.
    let origem = [
        (lo[0] + hi[0] + 1) as f32 * 0.5,
        (lo[1] + hi[1] + 1) as f32 * 0.5,
        lo[2] as f32,
    ];
    malha(model, scale, origem)
}

/// Malha de uma PECA do rig: em volta do pivo dela, sem recentralizar.
///
/// `mesh` centra cada modelo na propria caixa — certo pra um bicho inteiro,
/// errado pra peca: cada braco iria pro centro e o corpo se desmontaria. As
/// pecas do personagem vem todas na MESMA tela (docs/character create.md), e
/// e' justamente essa tela comum que faz uma encaixar na outra.
pub fn mesh_na_origem(model: &VoxModel, scale: f32, origem: [f32; 3]) -> Vec<Mesh> {
    malha(model, scale, origem)
}

fn malha(model: &VoxModel, scale: f32, origem: [f32; 3]) -> Vec<Mesh> {
    let dims = [model.size[0], model.size[1], model.size[2]];
    let (cx, cy, base) = (origem[0], origem[1], origem[2]);

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
                // voxel (x, y, z) -> mundo (-x, z, y). ROTACAO, nao espelho:
                // trocar so' dois eixos, como era antes, e' reflexo — e tudo
                // que o modelo tinha na mao direita aparecia na esquerda.
                position: vec3(
                    (cx - c[0]) * scale,
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
                    // (-x, z, y). E' por sair da conta que trocar o mapa nao
                    // vira o modelo do avesso.
                    let mundo = |c: [f32; 3]| vec3(-c[0], c[2], c[1]);
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

/// O modelo de uma arma e o centro da pega, com o marcador ja' apagado.
/// `None` se o arquivo nao tem marcador — arma sem pega nao tem onde a mao
/// fechar, e desenhar em volta de um canto qualquer poria a espada no ombro.
pub fn modelo_de_arma(bytes: &[u8]) -> Option<(VoxModel, [f32; 3])> {
    let mut m = parse(bytes).ok()?.into_iter().max_by_key(|m| m.cells.iter().filter(|c| **c != 0).count())?;
    let i = m.cells.iter().position(|c| *c == 255)?;
    m.cells[i] = 0;
    let [sx, sy, _] = m.size;
    let (x, y, z) = (i % sx, (i / sx) % sy, i / (sx * sy));
    Some((m, [x as f32 + 0.5, y as f32 + 0.5, z as f32 + 0.5]))
}

/// Cache de modelos ja carregados e transformados em malha.
#[derive(Default)]
pub struct VoxCache {
    meshes: HashMap<String, Vec<Mesh>>,
    /// Arquivos de PECAS: nome do arquivo -> nome da peca -> malhas em volta
    /// do pivo da peca.
    rigs: HashMap<String, HashMap<String, Vec<Mesh>>>,
    /// Bichos em PECAS (`bicho.rs`): nome do arquivo -> pecas com pivo.
    bichos: HashMap<String, crate::bicho::Bicho>,
    /// Armas, com a malha em volta da PEGA (o voxel magenta do arquivo).
    armas: HashMap<String, Vec<Mesh>>,
}

/// Bytes de `<raiz dos vox>/<rel>` (raiz = `MMO_VOX` ou `assets/vox`). Falta
/// de arquivo LOGA: no iPhone ela era silenciosa e o personagem simplesmente
/// nao aparecia.
async fn ler_vox(rel: &str) -> Option<Vec<u8>> {
    let root = std::env::var("MMO_VOX").unwrap_or_else(|_| "assets/vox".into());
    let caminho = format!("{root}/{rel}");
    match ler_asset(&caminho).await {
        Ok(bytes) => Some(bytes),
        Err(e) => {
            eprintln!("[vox] falta {caminho}: {e}");
            None
        }
    }
}

/// iOS: o `load_file` da miniquad 0.4.11 procura com
/// `[NSBundle pathForResource:ofType:]`, que so' acha arquivo na RAIZ do bundle
/// — "assets/vox/personagem/corpo" com barras volta nil. Nenhum `.vox` carregava
/// no iPhone. O `.app` e' a pasta do executavel e o `build-ios.sh` copia
/// `assets/vox` pra dentro dela, entao le' direto dali.
#[cfg(target_os = "ios")]
async fn ler_asset(caminho: &str) -> Result<Vec<u8>, String> {
    let exe = std::env::current_exe().map_err(|e| format!("sem executavel: {e}"))?;
    let pasta = exe.parent().ok_or("executavel sem pasta")?;
    let arquivo = no_bundle(pasta, caminho);
    std::fs::read(&arquivo).map_err(|e| format!("{} ({e})", arquivo.display()))
}

#[cfg(not(target_os = "ios"))]
async fn ler_asset(caminho: &str) -> Result<Vec<u8>, String> {
    macroquad::file::load_file(caminho).await.map_err(|e| format!("{e:?}"))
}

/// Caminho de um asset dentro da pasta do app. Absoluto (ex. `MMO_VOX=/x`)
/// fica como esta'.
pub fn no_bundle(pasta_do_app: &std::path::Path, caminho: &str) -> std::path::PathBuf {
    let p = std::path::Path::new(caminho);
    if p.is_absolute() { p.to_path_buf() } else { pasta_do_app.join(p) }
}

impl VoxCache {
    /// Malha ja carregada. O desenho e' sincrono, entao a carga acontece
    /// antes (`load`) e aqui so' se consulta.
    pub fn peek(&self, name: &str) -> Option<&Vec<Mesh>> {
        self.meshes.get(name)
    }

    /// Carrega o MESMO modelo em varias cores: cada variante troca os quatro
    /// indices reservados do tier (241-244, docs/character create.md) e vira
    /// `<nome>_<sufixo>`. E' a regra "o tier e' uma cor, nao um modelo" — um
    /// arquivo, varias malhas geradas no carregamento.
    pub async fn load_variantes(&mut self, name: &str, scale: f32, variantes: &[(&str, [[u8; 3]; 4])]) -> Option<()> {
        let bytes = ler_vox(&format!("{name}.vox")).await?;
        let base = parse(&bytes).ok()?.into_iter().max_by_key(|m| m.cells.iter().filter(|c| **c != 0).count())?;
        for (sufixo, cores) in variantes {
            let m = na_cor(&base, cores);
            self.meshes.insert(format!("{name}_{sufixo}"), mesh(&m, scale));
        }
        println!("[vox] {name}: {} cores", variantes.len());
        Some(())
    }

    /// As pecas ja' carregadas de um arquivo de rig.
    pub fn arma(&self, name: &str) -> Option<&Vec<Mesh>> {
        self.armas.get(name)
    }

    /// Carrega `personagem/<nome>.vox` como ARMA: acha o voxel marcador (255)
    /// no centro da pega, apaga ele e faz a malha em volta desse ponto — e'
    /// onde a mao fecha (docs/ARTE_DO_PERSONAGEM.md, skins de arma).
    pub async fn load_arma(&mut self, name: &str, scale: f32) -> Option<usize> {
        let bytes = ler_vox(&format!("personagem/{name}.vox")).await?;
        let (m, pega) = modelo_de_arma(&bytes)?;
        let malhas = mesh_na_origem(&m, scale, pega);
        let tris = malhas.iter().map(|x| x.indices.len() / 3).sum::<usize>();
        println!("[vox] arma {name}: {tris} triangulos");
        self.armas.insert(name.to_string(), malhas);
        Some(tris)
    }

    pub fn bicho(&self, name: &str) -> Option<&crate::bicho::Bicho> {
        self.bichos.get(name)
    }

    /// Carrega um bicho em PECAS (`tools/voxrender/bichos.py`) na altura
    /// pedida.
    ///
    /// Todas as pecas vem na mesma tela, entao a malha de cada uma e' feita
    /// em volta da origem do bicho INTEIRO (centro em X/Y, base em Z), e so'
    /// o pivo e' da peca: a pose gira em volta dele sem desmontar o corpo.
    pub async fn load_bicho(&mut self, name: &str, altura: f32) -> Option<usize> {
        use crate::bicho::{junta_de, pivo_vox, Bicho, Junta, PecaDeBicho};
        let bytes = ler_vox(&format!("{name}.vox")).await?;
        let pecas = parse_nomeado(&bytes).ok()?;
        let caixas: Vec<_> = pecas.iter().map(|(_, m)| m.bounds()).collect();
        let mut lo = [usize::MAX; 3];
        let mut hi = [0usize; 3];
        for ((_, m), (l, h)) in pecas.iter().zip(&caixas) {
            if m.cells.iter().all(|c| *c == 0) {
                continue;
            }
            for i in 0..3 {
                lo[i] = lo[i].min(l[i]);
                hi[i] = hi[i].max(h[i]);
            }
        }
        if lo[0] == usize::MAX {
            return None;
        }
        let escala = altura / (hi[2] - lo[2] + 1) as f32;
        let origem = [
            (lo[0] + hi[0] + 1) as f32 * 0.5,
            (lo[1] + hi[1] + 1) as f32 * 0.5,
            lo[2] as f32,
        ];
        // mesma troca de eixos da `malha`: voxel (x, y, z) -> mundo (-x, z, y)
        let no_mundo = |p: [f32; 3]| {
            vec3((origem[0] - p[0]) * escala, (p[2] - origem[2]) * escala, (p[1] - origem[1]) * escala)
        };
        // A cabeca gira no pivo do PESCOCO quando ha' um: com pivos proprios,
        // o mesmo angulo nos dois abria uma fresta entre eles.
        let pescoco = pecas
            .iter()
            .zip(&caixas)
            .find(|((n, _), _)| n == "pescoco")
            .map(|(_, (l, h))| pivo_vox(Junta::Pescoco, *l, *h));
        let mut out = Vec::new();
        let mut tris = 0usize;
        for ((nome, m), (l, h)) in pecas.iter().zip(&caixas) {
            let Some(junta) = junta_de(nome) else { continue };
            let pv = match (junta, pescoco) {
                (Junta::Cabeca, Some(p)) => p,
                _ => pivo_vox(junta, *l, *h),
            };
            let malhas = mesh_na_origem(m, escala, origem);
            tris += malhas.iter().map(|x| x.indices.len() / 3).sum::<usize>();
            out.push(PecaDeBicho { junta, pivo: no_mundo(pv), malhas });
        }
        // A patada precisa do focinho (ate' onde o arco passa) e do ombro da
        // pata que golpeia (de onde ele parte).
        let frente = (hi[1] + 1) as f32 - origem[1];
        let ombro = out
            .iter()
            .find(|p| p.junta == Junta::Pata { frente: true, esq: false })
            .map_or(vec3(0.0, altura * 0.3, 0.0), |p| p.pivo);
        let anat = crate::bicho::Anatomia {
            altura,
            frente: frente * escala,
            ombro,
            lado: if ombro.x < 0.0 { -1.0 } else { 1.0 },
            lateral: name.contains("caranguejo"),
        };
        let n = out.len();
        println!("[vox] {name}: {n} pecas, {tris} triangulos");
        self.bichos.insert(name.to_string(), Bicho { anat, pecas: out });
        Some(n)
    }

    pub fn rig(&self, name: &str) -> Option<&HashMap<String, Vec<Mesh>>> {
        self.rigs.get(name)
    }

    /// Carrega um arquivo de PECAS (objetos nomeados no MagicaVoxel). Cada
    /// peca vira malha em volta do proprio pivo, que `pivo` responde pelo
    /// nome. `None` quando o arquivo nao existe — o cliente cai no modelo
    /// inteiro de antes.
    pub async fn load_rig(&mut self, name: &str, scale: f32, pivo: impl Fn(&str) -> [f32; 3]) -> Option<usize> {
        let bytes = ler_vox(&format!("{name}.vox")).await?;
        let pecas = parse_nomeado(&bytes).ok()?;
        let mut mapa = HashMap::new();
        let mut tris = 0usize;
        for (nome, m) in pecas {
            if nome.is_empty() {
                continue;
            }
            let ms = mesh_na_origem(&m, scale, pivo(&nome));
            tris += ms.iter().map(|x| x.indices.len() / 3).sum::<usize>();
            mapa.insert(nome, ms);
        }
        let n = mapa.len();
        println!("[vox] {name}: {n} pecas, {tris} triangulos");
        self.rigs.insert(name.to_string(), mapa);
        Some(n)
    }

    /// Carrega `<raiz>/<nome>.vox`. `None` quando o arquivo nao existe — mob
    /// sem modelo cai no desenho de fallback em vez de derrubar o cliente.
    pub async fn load(&mut self, name: &str, scale: f32) -> Option<&Vec<Mesh>> {
        self.carrega(name, |_| scale).await
    }

    /// Carrega o modelo na ALTURA pedida, em unidades de mundo, qualquer que
    /// seja a resolucao do arquivo. E' o que separa quantas faces um bicho tem
    /// de quao grande ele aparece.
    pub async fn load_na_altura(&mut self, name: &str, altura: f32) -> Option<&Vec<Mesh>> {
        self.carrega(name, |m| {
            let (lo, hi) = m.bounds();
            altura / (hi[2] - lo[2] + 1).max(1) as f32
        })
        .await
    }

    async fn carrega(&mut self, name: &str, escala: impl Fn(&VoxModel) -> f32) -> Option<&Vec<Mesh>> {
        if !self.meshes.contains_key(name) {
            let bytes = ler_vox(&format!("{name}.vox")).await?;
            let models = parse(&bytes).ok()?;
            let m = models.into_iter().max_by_key(|m| {
                m.cells.iter().filter(|c| **c != 0).count()
            })?;
            let meshes = mesh(&m, escala(&m));
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

    /// No iPhone o asset sai da pasta do app, com as subpastas. E o que a
    /// previa precisa esta' em `assets/vox` e o `build-ios.sh` copia essa pasta.
    #[test]
    fn asset_do_ios_vem_da_pasta_do_app_com_subpastas() {
        let app = std::path::Path::new("/private/var/containers/Bundle/Application/X/Tempest.app");
        assert_eq!(
            no_bundle(app, "assets/vox/personagem/corpo.vox"),
            app.join("assets").join("vox").join("personagem").join("corpo.vox")
        );
        assert_eq!(no_bundle(app, "/tmp/vox/a.vox"), std::path::PathBuf::from("/tmp/vox/a.vox"));
        let raiz = format!("{}/../../assets/vox", env!("CARGO_MANIFEST_DIR"));
        for rig in [crate::render3d::RIG_CORPO, crate::render3d::RIG_CHAPEU] {
            let arquivo = format!("{raiz}/{rig}.vox");
            assert!(std::path::Path::new(&arquivo).exists(), "a previa usa {arquivo}, que nao existe");
        }
        let script = include_str!("../../../scripts/build-ios.sh");
        assert!(script.contains("assets/vox"), "build-ios.sh nao copia assets/vox pro app");
    }

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

    fn arquivo(rel: &str) -> Vec<u8> {
        std::fs::read(format!("{}/../../assets/vox/{rel}", env!("CARGO_MANIFEST_DIR")))
            .unwrap_or_else(|e| panic!("{rel}: {e}"))
    }

    /// O corpo do personagem chega com as dez pecas, pelo NOME — e' o nome
    /// que o rig procura. Um objeto renomeado no MagicaVoxel sumiria calado.
    #[test]
    fn o_corpo_chega_com_as_dez_pecas_nomeadas() {
        let pecas = parse_nomeado(&arquivo("personagem/corpo.vox")).unwrap();
        let nomes: std::collections::HashSet<&str> = pecas.iter().map(|(n, _)| n.as_str()).collect();
        for n in ["cabeca", "torso", "braco_d", "antebraco_d", "braco_e", "antebraco_e",
                  "coxa_d", "canela_d", "coxa_e", "canela_e"] {
            assert!(nomes.contains(n), "falta a peca {n}; vieram {nomes:?}");
        }
        for (n, m) in &pecas {
            assert!(m.cells.iter().any(|c| *c != 0), "peca {n} vazia");
        }
        let chapeu = parse_nomeado(&arquivo("personagem/cabelo_01.vox")).unwrap();
        assert!(chapeu.iter().any(|(n, _)| n == "cabelo"), "o chapeu nao se chama `cabelo`");
    }

    /// A malha e' ROTACAO, nao espelho: o que esta' no X maior do voxel (a
    /// direita do personagem, olhando pra +Y) cai no -X do mundo — que e' a
    /// direita de quem olha pra +Z. Com o mapa antigo, `(x, z, y)`, a mao
    /// direita modelada aparecia na esquerda.
    #[test]
    fn a_direita_do_modelo_e_a_direita_do_boneco() {
        let mut palette = default_palette();
        palette[1] = [255, 0, 0, 255];
        palette[2] = [0, 0, 255, 255];
        let m = VoxModel { size: [3, 1, 1], cells: vec![1, 0, 2], palette };
        let malhas = mesh(&m, 1.0);
        let media = |azul: bool| -> f32 {
            let xs: Vec<f32> = malhas.iter().flat_map(|mm| mm.vertices.iter())
                .filter(|v| (v.color[2] > v.color[0]) == azul)
                .map(|v| v.position.x).collect();
            xs.iter().sum::<f32>() / xs.len() as f32
        };
        // O azul esta' no X MAIOR do voxel.
        assert!(media(true) < 0.0 && media(false) > 0.0,
            "azul em {:.2}, vermelho em {:.2}", media(true), media(false));
    }

    /// Toda malha de modelo do jogo tem que estar INTEIRA: índice dentro dos
    /// vértices, nenhum triângulo esticado além do tamanho do próprio modelo,
    /// e as faces pra fora (volume com sinal positivo). Um urso que sai como
    /// emaranhado de fiapos quebra pelo menos uma dessas três.
    #[test]
    fn toda_malha_de_bicho_e_de_gente_esta_inteira() {
        let nomes = ["player", "pistoleiro", "mago", "arqueiro",
                     "lobo_pequeno", "urso", "tigre", "owlbear", "lobo"];
        for nome in nomes {
            let modelos = parse(&arquivo(&format!("{nome}.vox"))).unwrap();
            let m = modelos.into_iter().max_by_key(|m| m.cells.iter().filter(|c| **c != 0).count()).unwrap();
            let diag = ((m.size[0].pow(2) + m.size[1].pow(2) + m.size[2].pow(2)) as f32).sqrt();
            let malhas = mesh(&m, 1.0);
            let mut volume = 0.0f64;
            for (k, mm) in malhas.iter().enumerate() {
                let n = mm.vertices.len();
                assert!(mm.indices.len() % 3 == 0, "{nome} malha {k}: indices nao fecham triangulo");
                assert!(mm.indices.len() <= 5_000, "{nome} malha {k}: {} indices, acima do lote", mm.indices.len());
                for t in mm.indices.chunks(3) {
                    for &i in t {
                        assert!((i as usize) < n, "{nome} malha {k}: indice {i} fora de {n} vertices");
                    }
                    let (a, b, c) = (mm.vertices[t[0] as usize].position, mm.vertices[t[1] as usize].position,
                                     mm.vertices[t[2] as usize].position);
                    for (p, q) in [(a, b), (b, c), (c, a)] {
                        assert!(p.distance(q) <= diag + 0.01, "{nome} malha {k}: aresta de {:.1} num modelo de {:.1}", p.distance(q), diag);
                    }
                    volume += (a.dot(b.cross(c)) / 6.0) as f64;
                }
            }
            println!("{nome:13} {} malhas, volume {volume:.0}", malhas.len());
            assert!(volume > 0.0, "{nome}: volume {volume:.1} — malha pelo avesso");
        }
    }

    /// Os caranguejos vem em PECAS com as juntas que o `bicho.rs` conhece, e
    /// sao leves como mob comum.
    #[test]
    fn caranguejos_em_pecas_leves_e_inteiros() {
        for nome in ["bichos/caranguejo", "bichos/caranguejo_rei"] {
            let pecas = parse_nomeado(&arquivo(&format!("{nome}.vox"))).unwrap();
            for p in ["tronco", "pata_fd", "pata_fe", "pata_td", "pata_te"] {
                assert!(pecas.iter().any(|(n, _)| n == p), "{nome}: sem a peca {p}");
            }
            let mut tris = 0usize;
            for (n, m) in &pecas {
                if n.is_empty() {
                    continue;
                }
                assert!(crate::bicho::junta_de(n).is_some(), "{nome}: peca {n} sem junta");
                for mm in mesh_na_origem(m, 1.0, [0.0; 3]) {
                    assert!(mm.indices.len() <= 5_000, "{nome}/{n}: {} indices", mm.indices.len());
                    tris += mm.indices.len() / 3;
                }
            }
            println!("{nome}: {tris} triangulos");
            assert!(tris > 100 && tris <= 3_000, "{nome}: {tris} triangulos — mob comum e' leve");
        }
    }

    /// O saquinho usa a faixa do tier (241-244), e trocar a cor da faixa muda
    /// a cor da malha — e' isso que pinta o saque pela raridade.
    #[test]
    fn o_saquinho_muda_de_cor_pela_faixa_do_tier() {
        let m = parse(&arquivo("saque.vox")).unwrap().into_iter().next().unwrap();
        let usa = |k: u8| m.cells.iter().any(|c| *c == k);
        assert!((241..=244).any(usa), "o saquinho nao tem faixa de tier");
        let cor = |cores: [[u8; 3]; 4]| -> std::collections::HashSet<[u8; 3]> {
            mesh(&na_cor(&m, &cores), 1.0).iter().flat_map(|mm| mm.vertices.iter())
                .map(|v| [v.color[0], v.color[1], v.color[2]]).collect()
        };
        let verde = cor([[96, 226, 138]; 4]);
        let roxo = cor([[186, 112, 246]; 4]);
        assert_ne!(verde, roxo, "trocar a faixa nao mudou a malha");
    }

    #[test]
    fn todo_bicho_chega_com_tronco_e_quatro_patas() {
        for (nome, _) in crate::bicho::BICHOS {
            let caminho = format!("{}/../../assets/vox/{nome}.vox", env!("CARGO_MANIFEST_DIR"));
            let dados = std::fs::read(&caminho).unwrap_or_else(|_| panic!("falta {caminho}"));
            let pecas = parse_nomeado(&dados).unwrap();
            let juntas: Vec<_> = pecas.iter().filter_map(|(n, _)| crate::bicho::junta_de(n)).collect();
            assert_eq!(juntas.len(), pecas.len(), "{nome}: peca sem junta");
            assert!(juntas.contains(&crate::bicho::Junta::Tronco), "{nome}: sem tronco");
            let patas = juntas.iter().filter(|j| matches!(j, crate::bicho::Junta::Pata { .. })).count();
            assert_eq!(patas, 4, "{nome}");
        }
    }

    /// Toda ferramenta de coleta tem a pega marcada e custa o que uma arma
    /// custa: no lote da macroquad e sem passar do dobro da espada.
    #[test]
    fn as_ferramentas_tem_pega_e_cabem_no_lote() {
        let tris = |nome: &str| -> usize {
            let caminho = format!("{}/../../assets/vox/personagem/{nome}.vox", env!("CARGO_MANIFEST_DIR"));
            let dados = std::fs::read(&caminho).unwrap_or_else(|_| panic!("falta {caminho}"));
            let (m, pega) = modelo_de_arma(&dados).unwrap_or_else(|| panic!("{nome}: sem marcador na pega"));
            assert!(pega.iter().all(|v| *v > 0.0), "{nome}: pega fora da tela");
            let malhas = mesh_na_origem(&m, 0.04, pega);
            assert!(malhas.iter().all(|x| x.indices.len() <= 5000), "{nome}: malha fora do lote");
            malhas.iter().map(|x| x.indices.len() / 3).sum()
        };
        let espada = tris("espada");
        for nome in crate::rig::FERRAMENTAS {
            let t = tris(nome);
            assert!(t > 0 && t <= espada * 2, "{nome}: {t} triangulos (espada {espada})");
        }
    }

    #[test]
    fn a_espada_e_o_escudo_tem_a_pega_marcada() {
        for nome in ["espada", "escudo"] {
            let caminho = format!("{}/../../assets/vox/personagem/{nome}.vox", env!("CARGO_MANIFEST_DIR"));
            let dados = std::fs::read(&caminho).unwrap_or_else(|_| panic!("falta {caminho}"));
            let (m, pega) = modelo_de_arma(&dados).unwrap_or_else(|| panic!("{nome}: sem marcador na pega"));
            assert!(m.cells.iter().all(|c| *c != 255), "{nome}: sobrou marcador");
            assert!(pega.iter().all(|v| *v > 0.0));
        }
    }

    /// Todo NPC da vila tem as dez pecas do corpo, cada malha dentro do lote
    /// da macroquad, e custa no maximo o triplo de um humanoide de mob — o
    /// detalhe mora no chapeu e na mao, nao em triangulo.
    #[test]
    fn todo_npc_tem_as_dez_pecas_e_cabe_no_lote() {
        let tris = |rel: &str| -> usize {
            parse_nomeado(&arquivo(&format!("{rel}.vox"))).unwrap().iter()
                .filter(|(n, _)| !n.is_empty())
                .flat_map(|(_, m)| mesh_na_origem(m, 1.0, [0.0; 3]))
                .map(|m| m.indices.len() / 3)
                .sum()
        };
        let base = tris("humanoides/mago");
        let corpo: Vec<&str> = crate::rig::PECAS.iter().map(|p| p.0).filter(|n| *n != "cabelo").collect();
        for nome in crate::render3d::MODELOS_DE_NPC {
            let pecas = parse_nomeado(&arquivo(&format!("{nome}.vox"))).unwrap();
            for p in &corpo {
                assert!(pecas.iter().any(|(n, _)| n == p), "{nome}: falta a peca {p}");
            }
            for (peca, m) in &pecas {
                for mm in mesh_na_origem(m, 1.0, [0.0; 3]) {
                    assert!(mm.indices.len() <= 5_000, "{nome}/{peca}: {} indices", mm.indices.len());
                }
            }
            let t = tris(nome);
            println!("{nome:22} {t} triangulos (mago {base})");
            assert!(t <= base * 3, "{nome}: {t} triangulos, mais que o triplo do mago ({base})");
        }
    }

    /// Todo papel — os que existem, o de missoes e um que ainda nao existe —
    /// sai com um modelo que esta' na lista de carga e no disco.
    #[test]
    fn todo_papel_de_npc_tem_modelo() {
        for papel in (0u8..=40).chain([127, 255]) {
            for id in [0u64, 1, 2, 7] {
                let nome = crate::render3d::rig_do_npc(papel, id);
                assert!(crate::render3d::MODELOS_DE_NPC.contains(&nome), "papel {papel}: {nome} fora da lista");
                let _ = arquivo(&format!("{nome}.vox"));
            }
        }
        use shared::construcao::Papel;
        assert_eq!(crate::render3d::rig_do_npc(Papel::Alquimista as u8, 0), "npcs/alquimista");
        assert_eq!(crate::render3d::rig_do_npc(Papel::Estaleiro as u8, 0), "npcs/capitao");
        assert_eq!(crate::render3d::rig_do_npc(crate::render3d::PAPEL_MISSOES, 0), "npcs/mestre_missoes");
        // Morador varia pelo id.
        let casa = Papel::Casa as u8;
        let variados: std::collections::HashSet<_> = (0..3).map(|id| crate::render3d::rig_do_npc(casa, id)).collect();
        assert_eq!(variados.len(), 3);
    }

    /// `PAPEL_MISSOES` e' o seguinte a `Alquimista`: e' la' que a variante
    /// `Papel::Missoes` entra no enum.
    #[test]
    fn o_papel_de_missoes_e_o_seguinte() {
        assert_eq!(crate::render3d::PAPEL_MISSOES, shared::construcao::Papel::Alquimista as u8 + 1);
    }
}

