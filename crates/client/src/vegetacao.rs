//! Arvore e planta como VOLUME DE VOXEL, e nao como caixa.
//!
//! A primeira versao eram tres ou quatro caixas por arvore. Compilava, tinha a
//! densidade certa e ficou feia — porque o que faz vegetacao voxel parecer
//! vegetacao nao e' o volume ocupado, e' o CONTORNO e a SOMBRA:
//!
//!   * **tronco inclinado.** Arvore perfeitamente reta le' como poste.
//!   * **galhos** saindo do terco de cima, cada um terminando numa bolha.
//!   * **bolha ruidosa**, nao esfera: perto da casca falta bloco, e e' a falha
//!     que faz a copa ter folha em vez de superficie.
//!   * **tres tons de folha** sorteados por bloco. Copa de um verde so' le'
//!     como massa.
//!   * **oclusao de canto (AO)** na malha. E' ela que separa "cubos" de
//!     "volume": sem AO, dois blocos vizinhos nao tem juncao nenhuma.
//!
//! O voxel da vegetacao e' METADE do bloco do terreno. Detalhe mais fino que o
//! chao e' o que faz a planta ler como planta e nao como pedaco de morro.

use macroquad::models::{Mesh, Vertex};
use macroquad::prelude::*;

use shared::terreno::{Arvore, Material, Planta, BLOCO};

/// Aresta do voxel de vegetacao: metade do bloco do terreno.
pub const VOX: f32 = BLOCO * 0.5;

/// LCG minusculo. Mesma razao do resto: previsibilidade entre maquinas vale
/// mais que qualidade estatistica.
pub struct Rng(u32);

impl Rng {
    pub fn nova(semente: u32) -> Self {
        Self(semente.wrapping_mul(2_654_435_761).wrapping_add(1))
    }
    fn proximo(&mut self) -> u32 {
        self.0 = self.0.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        self.0 >> 8
    }
    fn f(&mut self) -> f32 {
        (self.proximo() % 4096) as f32 / 4096.0
    }
    /// Inteiro em `[a, b]`.
    fn i(&mut self, a: i32, b: i32) -> i32 {
        a + (self.proximo() % ((b - a + 1).max(1)) as u32) as i32
    }
    fn sinal(&mut self) -> f32 {
        if self.proximo() % 2 == 0 { 1.0 } else { -1.0 }
    }
}

/// Grade de voxels pequena, com origem deslocada.
pub struct Volume {
    min: [i32; 3],
    dim: [i32; 3],
    dados: Vec<u8>,
}

impl Volume {
    fn novo(min: [i32; 3], dim: [i32; 3]) -> Self {
        Self { min, dim, dados: vec![0; (dim[0] * dim[1] * dim[2]) as usize] }
    }

    fn idx(&self, x: i32, y: i32, z: i32) -> Option<usize> {
        let (a, b, c) = (x - self.min[0], y - self.min[1], z - self.min[2]);
        if a < 0 || b < 0 || c < 0 || a >= self.dim[0] || b >= self.dim[1] || c >= self.dim[2] {
            return None;
        }
        Some(((b * self.dim[2] + c) * self.dim[0] + a) as usize)
    }

    fn poe(&mut self, x: i32, y: i32, z: i32, m: Material) {
        if let Some(i) = self.idx(x, y, z) {
            self.dados[i] = m as u8 + 1;
        }
    }

    fn em(&self, x: i32, y: i32, z: i32) -> Option<Material> {
        let i = self.idx(x, y, z)?;
        let v = self.dados[i];
        // `0` e' vazio, o resto e' o discriminante do material mais um. Ler
        // por `Material::de_u8` e nao por tabela local: tabela paralela ja'
        // deixou tronco e folha invisiveis uma vez.
        if v == 0 { None } else { Material::de_u8(v - 1) }
    }

    fn solido(&self, x: i32, y: i32, z: i32) -> bool {
        self.idx(x, y, z).is_some_and(|i| self.dados[i] != 0)
    }
}

/// Malha pronta pra instanciar, com os vertices em torno da origem (o pe' da
/// planta em `y = 0`).
#[derive(Default, Clone)]
pub struct Modelo {
    pub verts: Vec<Vertex>,
    pub idx: Vec<u16>,
}

impl Modelo {
    #[cfg(test)]
    pub fn quads(&self) -> usize {
        self.idx.len() / 6
    }
}

// ─────────────────────────────── arvores ─────────────────────────────

pub fn arvore(especie: Arvore, variante: u32) -> Modelo {
    let mut r = Rng::nova(variante ^ (especie as u32) * 7919);
    let v = match especie {
        Arvore::Copada => copada(&mut r),
        Arvore::Betula => betula(&mut r),
        Arvore::Pinheiro => pinheiro(&mut r),
        Arvore::Seca => seca(&mut r),
    };
    malha(&v)
}

/// Tronco com inclinacao leve, galhos radiais e bolha na ponta de cada um.
fn copada(r: &mut Rng) -> Volume {
    // Com o jogador em 1,68 unidade e o voxel em 0,25, um tronco de 16 mais
    // copa de 6 da' ~5,5 unidades: tres vezes o boneco. Menos que isso e a
    // arvore vira arbusto grande — foi o que aconteceu na primeira medida.
    let h_tronco = r.i(13, 18);
    let raio = r.i(5, 7);
    let esp = if h_tronco > 12 { 1 } else { 0 };
    let topo = h_tronco + raio + 2;
    let lado = raio + 3;
    let mut v = Volume::novo([-lado, 0, -lado], [lado * 2 + 1, topo + 2, lado * 2 + 1]);

    // A inclinacao e' de no maximo um bloco no topo — o suficiente pra nao
    // ler como poste, pouco o bastante pra nao ler como tombada.
    let (lx, lz) = (r.sinal() * 0.7, r.sinal() * 0.7);
    for iy in 0..=h_tronco {
        let t = iy as f32 / h_tronco as f32;
        let (dx, dz) = ((lx * t).round() as i32, (lz * t).round() as i32);
        for ix in -esp..=esp {
            for iz in -esp..=esp {
                v.poe(ix + dx, iy, iz + dz, Material::Tronco);
            }
        }
    }
    let (cx, cz) = (lx.round() as i32, lz.round() as i32);

    // Bolha central mais uma por galho.
    let mut pontas = vec![(cx, h_tronco + raio / 2, cz, raio)];
    let galhos = r.i(3, 5);
    let giro0 = r.f() * std::f32::consts::TAU;
    for g in 0..galhos {
        let ang = giro0 + g as f32 * std::f32::consts::TAU / galhos as f32 + r.sinal() * 0.35;
        let comp = r.i(raio - 1, raio + 1);
        let base_y = h_tronco - r.i(1, 3);
        let (dx, dz) = (ang.sin(), ang.cos());
        let (mut px, mut py, mut pz) = (cx, base_y, cz);
        for s in 1..=comp {
            px = cx + (dx * s as f32).round() as i32;
            pz = cz + (dz * s as f32).round() as i32;
            py = base_y + (s as f32 * 0.75).round() as i32;
            v.poe(px, py, pz, Material::Tronco);
            v.poe(px, py - 1, pz, Material::Tronco);
        }
        pontas.push((px, py, pz, r.i(raio - 2, raio).max(2)));
    }
    for (bx, by, bz, br) in pontas {
        bolha(&mut v, r, bx, by, bz, br, br as f32 * 0.82, folha_de_copa);
    }
    v
}

/// Tronco fino e alto, copa pequena e clara.
fn betula(r: &mut Rng) -> Volume {
    let h = r.i(16, 21);
    let raio = r.i(3, 5);
    let lado = raio + 3;
    let mut v = Volume::novo([-lado, 0, -lado], [lado * 2 + 1, h + raio + 3, lado * 2 + 1]);
    let (lx, lz) = (r.sinal() * 0.9, r.sinal() * 0.5);
    for iy in 0..=h {
        let t = iy as f32 / h as f32;
        v.poe((lx * t).round() as i32, iy, (lz * t).round() as i32, Material::Tronco);
    }
    let (cx, cz) = (lx.round() as i32, lz.round() as i32);
    bolha(&mut v, r, cx, h, cz, raio, raio as f32 * 1.05, folha_de_betula);
    // Uma bolha menor deslocada: copa unica le' como bola espetada no palito.
    let (ox, oz) = (r.i(-1, 1), r.i(-1, 1));
    bolha(&mut v, r, cx + ox, h - 2, cz + oz, raio - 1, (raio - 1) as f32 * 0.9,
          folha_de_betula);
    v
}

/// Conica em camadas — a silhueta e' a especie.
fn pinheiro(r: &mut Rng) -> Volume {
    let h = r.i(18, 25);
    let raio = r.i(4, 6);
    let lado = raio + 2;
    let mut v = Volume::novo([-lado, 0, -lado], [lado * 2 + 1, h + 3, lado * 2 + 1]);
    for iy in 0..=h {
        v.poe(0, iy, 0, Material::Tronco);
    }
    // Camadas do terco de baixo pro topo, afinando. A ultima e' quase uma
    // ponta, e e' ela que da' o pico.
    let base = h / 3;
    let camadas = (h - base) / 2;
    for c in 0..camadas {
        let t = c as f32 / camadas.max(1) as f32;
        let rr = ((raio as f32) * (1.0 - t * 0.85)).round() as i32;
        let y = base + c * 2;
        if rr < 1 {
            continue;
        }
        for ix in -rr..=rr {
            for iz in -rr..=rr {
                let d = ((ix * ix + iz * iz) as f32).sqrt() / rr as f32;
                if d > 1.0 || (d > 0.6 && r.proximo() % 100 < ((d - 0.6) * 160.0) as u32) {
                    continue;
                }
                let m = folha_de_conifera(r);
                v.poe(ix, y, iz, m);
                if rr > 2 {
                    v.poe(ix, y + 1, iz, m);
                }
            }
        }
    }
    v
}

/// Sem folha: o vazio e' a leitura.
fn seca(r: &mut Rng) -> Volume {
    let h = r.i(14, 19);
    let lado = 6;
    let mut v = Volume::novo([-lado, 0, -lado], [lado * 2 + 1, h + 4, lado * 2 + 1]);
    let (lx, lz) = (r.sinal() * 1.2, r.sinal() * 0.8);
    for iy in 0..=h {
        let t = iy as f32 / h as f32;
        v.poe((lx * t).round() as i32, iy, (lz * t).round() as i32, Material::Tronco);
    }
    let (cx, cz) = (lx.round() as i32, lz.round() as i32);
    for g in 0..r.i(2, 4) {
        let ang = r.f() * std::f32::consts::TAU + g as f32;
        let (dx, dz) = (ang.sin(), ang.cos());
        let base_y = h - r.i(0, 5);
        for s in 1..=r.i(2, 4) {
            v.poe(
                cx + (dx * s as f32).round() as i32,
                base_y + (s as f32 * 0.9).round() as i32,
                cz + (dz * s as f32).round() as i32,
                Material::Tronco,
            );
        }
    }
    v
}

/// Esfera com falha crescente perto da casca.
///
/// A falha nao e' enfeite: esfera cheia le' como bola, e o que faz a copa
/// parecer folhagem e' justamente o buraco na borda.
fn bolha(
    v: &mut Volume,
    r: &mut Rng,
    cx: i32,
    cy: i32,
    cz: i32,
    raio: i32,
    raio_y: f32,
    folha: fn(&mut Rng) -> Material,
) {
    let raio = raio.max(1);
    let ry = raio_y.ceil() as i32;
    for ix in -raio..=raio {
        for iy in -ry..=ry {
            for iz in -raio..=raio {
                let dx = ix as f32 / raio as f32;
                let dy = iy as f32 / raio_y;
                let dz = iz as f32 / raio as f32;
                let d = dx * dx + dy * dy + dz * dz;
                if d > 1.0 {
                    continue;
                }
                if d > 0.55 && (r.proximo() % 100) < ((d - 0.55) * 130.0) as u32 {
                    continue;
                }
                let (x, y, z) = (cx + ix, cy + iy, cz + iz);
                if y < 0 || v.solido(x, y, z) {
                    continue;
                }
                let m = folha(r);
                v.poe(x, y, z, m);
            }
        }
    }
}

fn folha_de_copa(r: &mut Rng) -> Material {
    match r.proximo() % 10 {
        0..=2 => Material::FolhaEscura,
        3 => Material::GramaClara,
        _ => Material::Folha,
    }
}

fn folha_de_betula(r: &mut Rng) -> Material {
    if r.proximo() % 5 == 0 { Material::GramaClara } else { Material::Folha }
}

fn folha_de_conifera(r: &mut Rng) -> Material {
    if r.proximo() % 4 == 0 { Material::Folha } else { Material::FolhaEscura }
}

// ─────────────────────────────── plantas ─────────────────────────────

pub fn planta(especie: Planta, variante: u32) -> Modelo {
    let mut r = Rng::nova(variante ^ (especie as u32) * 6151 ^ 0x9E37);
    let v = match especie {
        Planta::Moita => moita(&mut r),
        Planta::Flor => flor(&mut r),
        Planta::Arbusto => arbusto(&mut r),
        Planta::Samambaia => samambaia(&mut r),
        Planta::Pedra => pedra(&mut r),
        Planta::Toco => toco(&mut r),
        Planta::Talo => talo(&mut r),
    };
    malha(&v)
}

fn caixa_vazia(raio: i32, alt: i32) -> Volume {
    Volume::novo([-raio - 1, 0, -raio - 1], [raio * 2 + 3, alt, raio * 2 + 3])
}

/// Tufo de hastes que INCLINAM na ponta. Capim reto vira cerca de palito.
fn moita(r: &mut Rng) -> Volume {
    let raio = r.i(2, 4);
    let mut v = caixa_vazia(raio, 9);
    for _ in 0..r.i(10, 20) {
        let (ix, iz) = (r.i(-raio, raio), r.i(-raio, raio));
        let h = r.i(2, 5);
        let cai = r.i(0, 2);
        for iy in 0..h {
            let dx = if iy >= h - cai { if r.proximo() % 2 == 0 { 1 } else { -1 } } else { 0 };
            v.poe(
                ix + dx,
                iy,
                iz,
                if iy >= h - 2 { Material::GramaClara } else { Material::Folha },
            );
        }
    }
    v
}

fn flor(r: &mut Rng) -> Volume {
    let raio = r.i(2, 4);
    let mut v = caixa_vazia(raio, 11);
    // A cor da cabeca sai do sorteio: um campo de flor nao pode ser um campo
    // de UMA flor.
    let cor = match r.proximo() % 4 {
        0 => Material::PetalaVermelha,
        1 => Material::PetalaAmarela,
        2 => Material::PetalaRoxa,
        _ => Material::PetalaBranca,
    };
    // Cabeca de tres por tres: de cima, uma flor de um voxel some no chao —
    // e' a cabeca que se ve', nao a haste.
    for _ in 0..r.i(4, 8) {
        let (ix, iz) = (r.i(-raio, raio), r.i(-raio, raio));
        let h = r.i(4, 7);
        for iy in 0..h {
            v.poe(ix, iy, iz, Material::Folha);
        }
        for dx in -1i32..=1 {
            for dz in -1i32..=1 {
                if dx.abs() + dz.abs() > 1 {
                    continue;
                }
                v.poe(ix + dx, h, iz + dz, cor);
            }
        }
    }
    v
}

/// Moita fechada na altura do JOELHO, e nao na do jogador.
///
/// Elipsoide achatado, nao esfera: `dy` dividido por `alt*0,55` faz a planta
/// espalhar mais do que sobe, que e' o que separa arbusto de arvore pequena.
/// Com esfera ele ficava do tamanho de uma copada e o bosque perdia escala.
///
/// **35% dos arbustos dao fruto.** Se todos dessem, o fruto deixaria de
/// significar alguma coisa.
fn arbusto(r: &mut Rng) -> Volume {
    let raio = r.i(3, 4);
    let alt = r.i(3, 4);
    let mut v = caixa_vazia(raio, alt + 3);
    let frutos = r.f() < 0.35;
    for iy in 0..=2 {
        v.poe(0, iy, 0, Material::Tronco);
    }
    for ix in -raio..=raio {
        for iz in -raio..=raio {
            for iy in 1..=alt {
                let dx = ix as f32 / raio as f32;
                let dz = iz as f32 / raio as f32;
                let dy = (iy as f32 - alt as f32 * 0.55) / (alt as f32 * 0.55);
                let d = dx * dx + dy * dy + dz * dz;
                if d > 1.0 || (d > 0.45 && (r.proximo() % 100) < ((d - 0.45) * 150.0) as u32) {
                    continue;
                }
                let mut m = if r.proximo() % 5 == 0 {
                    Material::FolhaEscura
                } else {
                    Material::Folha
                };
                // Fruto so' na casca: no miolo ninguem veria.
                if frutos && d > 0.6 && r.proximo() % 22 == 0 {
                    m = Material::Fruto;
                }
                v.poe(ix, iy, iz, m);
            }
        }
    }
    v
}

/// Fronde larga e baixa: o que se ve' de cima e' mancha achatada, nao tufo.
fn samambaia(r: &mut Rng) -> Volume {
    let raio = r.i(4, 6);
    let mut v = caixa_vazia(raio, 7);
    for _ in 0..r.i(5, 9) {
        let ang = r.f() * std::f32::consts::TAU;
        let (dx, dz) = (ang.sin(), ang.cos());
        let comp = r.i(raio - 1, raio);
        for s in 1..=comp {
            let y = ((comp - s) as f32 * 0.55).round() as i32;
            v.poe(
                (dx * s as f32).round() as i32,
                y,
                (dz * s as f32).round() as i32,
                if s > comp - 2 { Material::FolhaEscura } else { Material::Folha },
            );
        }
    }
    v
}

/// Matacao: elipsoide com CANTO LASCADO e dois tons.
///
/// O canto lascado (um voxel em onze descartado) e' o que tira a cara de
/// elipsoide perfeito; os dois tons sao o que tira a de bola de papel.
fn pedra(r: &mut Rng) -> Volume {
    let raio = r.i(2, 5);
    let alt = r.i(2, raio + 2);
    let mut v = caixa_vazia(raio, alt + 3);
    for ix in -raio..=raio {
        for iz in -raio..=raio {
            for iy in 0..=alt {
                let dx = ix as f32 / (raio as f32 + 0.4);
                let dz = iz as f32 / (raio as f32 + 0.4);
                let dy = iy as f32 / (alt as f32 + 0.6);
                if dx * dx + dy * dy + dz * dz > 1.0 || r.proximo() % 11 == 0 {
                    continue;
                }
                v.poe(
                    ix, iy, iz,
                    if r.proximo() % 3 == 0 { Material::RochaEscura } else { Material::Rocha },
                );
            }
        }
    }
    v
}

fn toco(r: &mut Rng) -> Volume {
    let raio = r.i(2, 3);
    let h = r.i(2, 4);
    let mut v = caixa_vazia(raio, h + 2);
    for ix in -raio..=raio {
        for iz in -raio..=raio {
            if ix * ix + iz * iz > raio * raio {
                continue;
            }
            for iy in 0..h {
                v.poe(ix, iy, iz, Material::Tronco);
            }
        }
    }
    v
}

/// Talo alto e palido de campo aberto — o unico jeito de o deserto ter
/// silhueta sem ter arvore.
fn talo(r: &mut Rng) -> Volume {
    let mut v = caixa_vazia(3, 16);
    for _ in 0..r.i(3, 6) {
        let (ix, iz) = (r.i(-2, 2), r.i(-2, 2));
        let h = r.i(8, 14);
        for iy in 0..h {
            let dx = if iy > h - 3 { r.i(-1, 1) } else { 0 };
            v.poe(ix + dx, iy, iz, Material::FolhaSeca);
        }
        v.poe(ix, h, iz, Material::Areia);
        v.poe(ix, h + 1, iz, Material::Areia);
    }
    v
}

// ──────────────────────────────── malha ──────────────────────────────

/// Malha do volume: face oculta descartada, **oclusao de canto** e **merge
/// guloso**.
///
/// A AO e' o que separa "cubos empilhados" de "volume": sem ela dois blocos
/// vizinhos nao tem juncao nenhuma e a copa vira mancha chapada.
///
/// O merge e' o que torna isso pagavel. Sem ele uma copada saia com ~1.700
/// quads e o pedaco virava doze malhas — medido: **19 fps**. Faces so' se
/// juntam quando tem o MESMO material e a MESMA oclusao nos quatro cantos,
/// entao a sombra nao e' perdida no caminho.
fn malha(v: &Volume) -> Modelo {
    let mut m = Modelo::default();
    // (normal, luz, eixo u, eixo v) — u e v sao os do plano da face.
    const FACES: [([i32; 3], f32, usize, usize); 6] = [
        ([0, 1, 0], 1.00, 0, 2),
        ([0, -1, 0], 0.45, 0, 2),
        ([1, 0, 0], 0.76, 2, 1),
        ([-1, 0, 0], 0.76, 2, 1),
        ([0, 0, 1], 0.60, 0, 1),
        ([0, 0, -1], 0.60, 0, 1),
    ];
    for (n, luz, eu, ev) in FACES {
        let eixo = n.iter().position(|k| *k != 0).unwrap();
        let (lo, dim) = (v.min, v.dim);
        let (nu, nv) = (dim[eu] as usize, dim[ev] as usize);
        let mut mascara: Vec<Option<(Material, [u8; 4])>> = vec![None; nu * nv];

        for camada in 0..dim[eixo] {
            // Monta a mascara desta fatia.
            for iv in 0..nv {
                for iu in 0..nu {
                    let mut p = [0i32; 3];
                    p[eixo] = lo[eixo] + camada;
                    p[eu] = lo[eu] + iu as i32;
                    p[ev] = lo[ev] + iv as i32;
                    let visivel = v.em(p[0], p[1], p[2]).filter(|_| {
                        !v.solido(p[0] + n[0], p[1] + n[1], p[2] + n[2])
                    });
                    mascara[iv * nu + iu] = visivel.map(|mat| {
                        let ao = [
                            oclusao(v, p, n, eu, ev, -1, -1),
                            oclusao(v, p, n, eu, ev, 1, -1),
                            oclusao(v, p, n, eu, ev, 1, 1),
                            oclusao(v, p, n, eu, ev, -1, 1),
                        ];
                        (mat, ao)
                    });
                }
            }
            // Retangulos gulosos sobre a mascara.
            let mut iv = 0usize;
            while iv < nv {
                let mut iu = 0usize;
                while iu < nu {
                    let Some(celula) = mascara[iv * nu + iu] else {
                        iu += 1;
                        continue;
                    };
                    // estende em u
                    let mut w = 1;
                    while iu + w < nu && mascara[iv * nu + iu + w] == Some(celula) {
                        w += 1;
                    }
                    // estende em v, so' se a faixa inteira casar
                    let mut h = 1;
                    'alt: while iv + h < nv {
                        for k in 0..w {
                            if mascara[(iv + h) * nu + iu + k] != Some(celula) {
                                break 'alt;
                            }
                        }
                        h += 1;
                    }
                    for a in 0..h {
                        for b in 0..w {
                            mascara[(iv + a) * nu + iu + b] = None;
                        }
                    }
                    let mut p = [0i32; 3];
                    p[eixo] = lo[eixo] + camada;
                    p[eu] = lo[eu] + iu as i32;
                    p[ev] = lo[ev] + iv as i32;
                    emite(&mut m, p, n, eu, ev, w as i32, h as i32, celula.0, celula.1, luz);
                    iu += w;
                }
                iv += 1;
            }
        }
    }
    m
}

/// Emite um quad de `w x h` celulas no plano da face.
#[allow(clippy::too_many_arguments)]
fn emite(
    m: &mut Modelo,
    p: [i32; 3],
    n: [i32; 3],
    eu: usize,
    ev: usize,
    w: i32,
    h: i32,
    mat: Material,
    ao: [u8; 4],
    luz: f32,
) {
    let mut base = vec3(p[0] as f32, p[1] as f32, p[2] as f32) + vec3(0.5, 0.5, 0.5);
    base += vec3(n[0] as f32, n[1] as f32, n[2] as f32) * 0.5;
    let mut au = Vec3::ZERO;
    au[eu] = 1.0;
    let mut av = Vec3::ZERO;
    av[ev] = 1.0;
    let (r, g, b) = mat.rgb();
    let canto = |su: f32, sv: f32| base + au * (su - 0.5) + av * (sv - 0.5);
    let pontos = [
        (canto(0.0, 0.0), ao[0]),
        (canto(w as f32, 0.0), ao[1]),
        (canto(w as f32, h as f32), ao[2]),
        (canto(0.0, h as f32), ao[3]),
    ];
    // A ORDEM dos quatro cantos decide o que a placa de video considera
    // frente. Com o descarte de face de costas ligado, quad enrolado ao
    // contrario some — e some de um lado so', entao o buraco espera o jogador
    // virar a camera pra aparecer.
    //
    // Aqui a ordem sai da conta e nao da mao: os eixos `eu`/`ev` do merge
    // guloso mudam de sentido conforme a face, e escrever a ordem certa pra
    // cada combinacao e' exatamente o tipo de coisa que passa no olho e falha
    // no conjunto.
    //
    // Inverter os CANTOS, e nao os indices: o `instancia` copia vertice a
    // vertice e reconstroi os indices no padrao canonico, entao a ordem tem
    // que estar guardada no modelo.
    let normal = vec3(n[0] as f32, n[1] as f32, n[2] as f32);
    let geom = (pontos[1].0 - pontos[0].0).cross(pontos[2].0 - pontos[0].0);
    let pontos = if geom.dot(normal) >= 0.0 {
        pontos
    } else {
        [pontos[0], pontos[3], pontos[2], pontos[1]]
    };
    let inicio = m.verts.len() as u16;
    for (pos, oc) in pontos {
        let k = luz * (oc as f32 / 255.0);
        m.verts.push(Vertex {
            position: pos * VOX,
            uv: vec2(0.0, 0.0),
            color: [
                (r as f32 * k) as u8,
                (g as f32 * k) as u8,
                (b as f32 * k) as u8,
                255,
            ],
            normal: Vec4::ZERO,
        });
    }
    m.idx.extend_from_slice(&[inicio, inicio + 1, inicio + 2, inicio, inicio + 2, inicio + 3]);
}

/// Quanto o canto esta' encoberto, 140 (fechado) a 255 (aberto).
///
/// Devolve `u8` e nao `f32` pra o merge poder comparar por igualdade — dois
/// cantos com AO "quase igual" nao podem se fundir sem manchar a sombra.
fn oclusao(v: &Volume, p: [i32; 3], n: [i32; 3], eu: usize, ev: usize, su: i32, sv: i32) -> u8 {
    let mut la = n;
    la[eu] += su;
    let mut lb = n;
    lb[ev] += sv;
    let mut dg = n;
    dg[eu] += su;
    dg[ev] += sv;
    let em = |d: [i32; 3]| v.solido(p[0] + d[0], p[1] + d[1], p[2] + d[2]);
    let (s1, s2) = (em(la), em(lb));
    if s1 && s2 {
        return 140;
    }
    let ocupados = s1 as u32 + s2 as u32 + em(dg) as u32;
    (255 - ocupados * 38) as u8
}

/// Copia o modelo pra dentro de uma malha maior, deslocado e escalado.
///
/// E' assim que a vegetacao entra no pedaco: o modelo e' assado uma vez por
/// especie e variante, e instanciar e' copiar vertice com offset. A macroquad
/// nao tem transform por malha, entao a alternativa seria transformar na CPU
/// toda planta todo quadro.
pub fn instancia(
    modelo: &Modelo,
    em: Vec3,
    escala: f32,
    verts: &mut Vec<Vertex>,
    idx: &mut Vec<u16>,
    malhas: &mut Vec<Mesh>,
    max_quads: usize,
) {
    // Copia em BLOCOS de quad, fechando a malha quando o orcamento acaba.
    //
    // Cabia so' o caso "o modelo inteiro cabe no que sobrou". Um modelo maior
    // que o orcamento inteiro — e uma copada e' — passava direto e a malha
    // saia com o dobro do teto de indice, que a macroquad corta em silencio.
    let mut q = 0usize;
    let total = modelo.idx.len() / 6;
    while q < total {
        if idx.len() / 6 >= max_quads {
            malhas.push(Mesh {
                vertices: std::mem::take(verts),
                indices: std::mem::take(idx),
                texture: None,
            });
        }
        let cabe = (max_quads - idx.len() / 6).min(total - q);
        let base = verts.len() as u16;
        for k in 0..cabe * 4 {
            let v = modelo.verts[q * 4 + k];
            verts.push(Vertex { position: v.position * escala + em, ..v });
        }
        for k in 0..cabe {
            let b = base + (k * 4) as u16;
            idx.extend_from_slice(&[b, b + 1, b + 2, b, b + 2, b + 3]);
        }
        q += cabe;
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    /// Flor tem que ter PETALA na malha.
    ///
    /// Parece obvio e nao e': ja' houve caso de o material chegar ao volume e
    /// nao chegar a' malha (tabela desalinhada deixava o voxel invisivel).
    /// Este teste olha a cor que efetivamente foi para o vertice.
    #[test]
    fn a_flor_tem_petala() {
        let petalas: Vec<[u8; 3]> = [
            Material::PetalaVermelha,
            Material::PetalaAmarela,
            Material::PetalaRoxa,
            Material::PetalaBranca,
        ]
        .iter()
        .map(|m| {
            let (r, g, b) = m.rgb();
            [r, g, b]
        })
        .collect();
        let mut achou = 0;
        for k in 0..12u32 {
            let m = planta(Planta::Flor, k);
            assert!(m.quads() > 0, "variante {k} saiu vazia");
            // A cor do vertice e' a do material vezes luz e AO, entao compara
            // pela PROPORCAO entre canais, que a multiplicacao preserva.
            let tem = m.verts.iter().any(|v| {
                petalas.iter().any(|p| {
                    let s: u32 = p.iter().map(|c| *c as u32).sum();
                    let sv: u32 = v.color[..3].iter().map(|c| *c as u32).sum();
                    if sv == 0 || s == 0 {
                        return false;
                    }
                    (0..3).all(|i| {
                        let a = p[i] as f32 / s as f32;
                        let b = v.color[i] as f32 / sv as f32;
                        (a - b).abs() < 0.02
                    })
                })
            });
            if tem {
                achou += 1;
            }
        }
        assert!(achou >= 10, "so' {achou} de 12 variantes de flor tem petala");
    }

    /// Proporcao entre as especies: a arvore tem que ser bem maior que o
    /// arbusto, senao o bosque perde escala.
    #[test]
    fn arvore_e_maior_que_arbusto() {
        let alt = |m: &Modelo| {
            m.verts.iter().map(|v| v.position.y).fold(0.0f32, f32::max)
        };
        let arv = alt(&arvore(Arvore::Copada, 3));
        let arb = alt(&planta(Planta::Arbusto, 3));
        let moi = alt(&planta(Planta::Moita, 3));
        println!("copada {arv:.2}un · arbusto {arb:.2}un · moita {moi:.2}un (jogador 1,68)");
        assert!(arv > arb * 2.5, "arvore {arv:.2} nao e' o dobro do arbusto {arb:.2}");
        assert!(arb < 1.7, "arbusto {arb:.2} esta' na altura do jogador");
    }
}

#[cfg(test)]
mod testes_orientacao {
    use super::*;

    /// Toda planta e arvore aponta pra fora, e sem buraco.
    ///
    /// Mesma conta dos modelos `.vox`: volume com sinal. Numa malha fechada e
    /// virada pra fora ele e' o volume de verdade; negativo e' avesso, e
    /// diferente do volume esperado e' buraco.
    ///
    /// Vale pra TODAS as variantes de todas as especies, porque cada uma e'
    /// sorteada e nenhuma foi olhada a olho nu.
    #[test]
    fn toda_vegetacao_aponta_pra_fora() {
        let mut conferidos = 0;
        for especie in [
            Arvore::Copada, Arvore::Betula, Arvore::Pinheiro, Arvore::Seca,
        ] {
            for variante in 0..6u32 {
                let m = arvore(especie, variante);
                confere(&m, &format!("{especie:?} #{variante}"));
                conferidos += 1;
            }
        }
        for especie in [
            Planta::Moita, Planta::Flor, Planta::Arbusto, Planta::Samambaia,
            Planta::Pedra, Planta::Toco, Planta::Talo,
        ] {
            for variante in 0..6u32 {
                let m = planta(especie, variante);
                confere(&m, &format!("{especie:?} #{variante}"));
                conferidos += 1;
            }
        }
        assert!(conferidos > 30, "so' {conferidos} modelos conferidos");
        println!("{conferidos} modelos de vegetacao, todos pra fora");
    }

    fn confere(m: &Modelo, quem: &str) {
        let mut volume = 0.0f64;
        for t in m.idx.chunks(3) {
            let a = m.verts[t[0] as usize].position;
            let b = m.verts[t[1] as usize].position;
            let c = m.verts[t[2] as usize].position;
            volume += a.dot(b.cross(c)) as f64;
        }
        assert!(
            volume > 0.0,
            "{quem}: volume com sinal {:.3} — malha pelo avesso, o descarte \
             de face vai mostrar o interior dela",
            volume / 6.0
        );
    }
}
