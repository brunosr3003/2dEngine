//! Volume voxel do terreno, gerado no cliente a partir da MESMA semente.
//!
//! Nada de terreno vem pela rede. O servidor guarda dois bytes por coluna e o
//! cliente reconstroi o volume aqui — os dois passam pelo mesmo
//! `shared::terreno::Gerador`, entao o chao que o jogador ve' e o chao em que
//! o servidor o poe sao o mesmo por construcao, nao por disciplina.
//!
//! ## Por pedaco, e nao a ilha inteira
//!
//! A ilha grande tem 10,2 milhoes de colunas — 3,3 s de geracao. O que cabe na
//! tela sao alguns milhares. Entao o mundo e' cortado em pedacos de 32 colunas,
//! gerados sob demanda em volta do jogador e jogados fora quando ele se afasta.
//!
//! ## So' a casca
//!
//! Nao ha' caverna: o volume e' inteiramente determinado pela altura de cada
//! coluna. Entao a malha nao percorre volume nenhum — desenha o TOPO de cada
//! coluna e a parede lateral onde o vizinho e' mais baixo. E' a mesma imagem
//! de um voxel completo por uma fracao do custo.

use std::collections::HashMap;

use macroquad::models::{Mesh, Vertex};
use macroquad::prelude::*;

use shared::terreno::{
    material_de_profundidade, material_variado, tom_da_mancha, Arvore, Bioma, DefIlha, Gerador, Material,
    Planta, BLOCO, NIVEL_DO_MAR,
};

/// Colunas por lado de um pedaco. 32 x 0,5 = 16 unidades de mundo.
///
/// Escolhido pelo teto de vertice: a macroquad corta o desenho em 10.000
/// vertices por chamada. Com 32 e o merge guloso, o pedaco mais quebrado ainda
/// cabe em poucos milhares.
pub const CHUNK: i32 = 32;

/// Teto do merge guloso no topo do terreno, em blocos.
///
/// Sem teto, um plato vira UM quad — e um quad tem uma cor, entao o grao do
/// voxel some e o chao fica chapado. O Sea of Cubes nao funde o terreno de
/// jeito nenhum (um quad por face de bloco); aqui o meio-termo e' fundir ate'
/// 3x3, o que mantem a variacao visivel a cada unidade e meia e ainda corta
/// nove decimos dos quads de um campo aberto.
const MERGE_MAX: i32 = 1;

/// Variantes assadas por especie. Uma so' faz a mata inteira ler como papel
/// de parede — o mesmo defeito de ter uma especie so'.
const VARIANTES: usize = 6;

/// Teto de QUADS por malha.
///
/// O limite que importa na macroquad nao e' o de vertices (10.000) que o
/// codigo antigo comenta: e' o de **indices**, 5.000. Cada quad gasta 4
/// vertices e 6 indices, entao o indice estoura primeiro — a 6.000 vertices a
/// malha ja' pede 9.000 indices e a metade dela e' descartada em silencio,
/// com um `geometry() exceeded max drawcall size, clamping` por chamada.
///
/// O sintoma nao parece limite nenhum: o terreno ganha buracos pretos em
/// tabuleiro, que passam por bug de merge guloso ou de distancia de desenho.
/// Foram 237 mil avisos no log ate' alguem olhar pra ele.
///
/// 800 quads = 3.200 vertices e 4.800 indices, os dois abaixo do teto.
const MAX_QUADS: usize = 800;

/// O bloco cujo topo esta' na linha d'agua. Coluna mais baixa que isso vira
/// mar: desenhar o fundo submerso seria geometria que ninguem ve'.
const BLOCO_DO_MAR: i32 = (NIVEL_DO_MAR / BLOCO) as i32 - 1;

struct Pedaco {
    malhas: Vec<Mesh>,
}

pub struct Terreno {
    ger: Gerador,
    bioma: Bioma,
    /// Modelos de vegetacao assados uma vez por especie e variante. Instanciar
    /// e' copiar vertice com offset — a macroquad nao tem transform por malha,
    /// entao gerar de novo por planta seria refazer o volume milhares de vezes.
    arvores: Vec<crate::vegetacao::Modelo>,
    plantas: Vec<crate::vegetacao::Modelo>,
    pedacos: HashMap<(i32, i32), Pedaco>,
    /// Quantos pedacos foram gerados desde o inicio — so' pra diagnostico.
    pub gerados: u32,
}

/// Ordem dos indices que faz a normal GEOMETRICA do quad bater com `n`.
///
/// O enrolamento decide o que a placa de video considera frente. Com o
/// descarte de face de costas ligado, quad enrolado ao contrario some — e
/// some de um lado so', entao o buraco espera o jogador virar a camera pra
/// aparecer.
///
/// Escrever a ordem certa a mao em cada um dos quatro sentidos e' o tipo de
/// coisa que passa no olho e falha no conjunto: medido antes disto, os 25.960
/// topos estavam TODOS invertidos e as paredes discordavam entre si por
/// sentido. Aqui a ordem sai da conta, e a unica coisa que o chamador precisa
/// saber e' pra onde a face olha.
fn ordem(b: u16, p: [Vec3; 4], n: Vec3) -> [u16; 6] {
    let geom = (p[1] - p[0]).cross(p[2] - p[0]);
    if geom.dot(n) >= 0.0 {
        [b, b + 1, b + 2, b, b + 2, b + 3]
    } else {
        [b, b + 2, b + 1, b, b + 3, b + 2]
    }
}

impl Terreno {
    pub fn novo(def: &DefIlha) -> Self {
        let mut t = Self {
            ger: Gerador::novo(
                def.semente,
                def.raio_blocos,
                def.bioma,
                shared::terreno::ESCALA_ALTURA,
            ),
            bioma: def.bioma,
            arvores: Vec::new(),
            plantas: Vec::new(),
            pedacos: HashMap::new(),
            gerados: 0,
        };
        // VARIANTES por especie: uma arvore so' por especie faz a mata inteira
        // ler como papel de parede, que e' o mesmo defeito de ter uma especie
        // so'. Seis dao variedade sem encher a memoria.
        for e in [Arvore::Copada, Arvore::Betula, Arvore::Pinheiro, Arvore::Seca] {
            for k in 0..VARIANTES {
                t.arvores.push(crate::vegetacao::arvore(e, k as u32 * 7 + e as u32 * 101));
            }
        }
        for e in [
            Planta::Moita, Planta::Flor, Planta::Arbusto, Planta::Samambaia,
            Planta::Pedra, Planta::Toco, Planta::Talo,
        ] {
            for k in 0..VARIANTES {
                t.plantas.push(crate::vegetacao::planta(e, k as u32 * 13 + e as u32 * 211));
            }
        }
        t
    }

    fn modelo_de_arvore(&self, e: Arvore, k: u32) -> &crate::vegetacao::Modelo {
        &self.arvores[e as usize * VARIANTES + (k as usize % VARIANTES)]
    }

    fn modelo_de_planta(&self, e: Planta, k: u32) -> &crate::vegetacao::Modelo {
        &self.plantas[e as usize * VARIANTES + (k as usize % VARIANTES)]
    }

    /// Altura do chao em `(x, z)` de mundo. E' por aqui que entidade e camera
    /// param de flutuar.
    pub fn altura(&self, x: f32, z: f32) -> f32 {
        self.ger.altura(x, z).max(NIVEL_DO_MAR)
    }

    /// Altura em que o CORPO se apoia: a coluna mais alta debaixo dele.
    ///
    /// Apoiar na coluna do centro afunda o modelo no degrau. Andando pra cima
    /// de um degrau, o centro ainda esta' na coluna de baixo enquanto a
    /// frente do corpo ja' passou por cima da de cima — e o modelo entra no
    /// bloco. Pe' fica em cima do que e' mais alto, que e' o que um pe' faz.
    pub fn altura_apoio(&self, x: f32, z: f32, raio: f32) -> f32 {
        let r = raio * 0.72;
        let mut maior = self.altura(x, z);
        for (dx, dz) in [
            (raio, 0.0), (-raio, 0.0), (0.0, raio), (0.0, -raio),
            (r, r), (-r, r), (r, -r), (-r, -r),
        ] {
            maior = maior.max(self.altura(x + dx, z + dz));
        }
        maior
    }

    /// Gera o que falta em volta do jogador e descarta o que ficou longe.
    ///
    /// `orcamento` limita quantos pedacos nascem por quadro: gerar os 49 de
    /// uma vez trava meio segundo, e travar meio segundo e' pior que o mundo
    /// aparecer em duas piscadas.
    pub fn atualiza(&mut self, centro: Vec2, raio: i32, orcamento: usize) {
        let (ccx, ccz) = self.pedaco_de(centro);
        self.pedacos.retain(|(cx, cz), _| {
            (cx - ccx).abs() <= raio + 1 && (cz - ccz).abs() <= raio + 1
        });

        // Do mais perto pro mais longe: o buraco que aparece embaixo do
        // jogador incomoda muito mais que o do horizonte.
        let mut faltando: Vec<(i32, i32, i32)> = Vec::new();
        for dz in -raio..=raio {
            for dx in -raio..=raio {
                let k = (ccx + dx, ccz + dz);
                if !self.pedacos.contains_key(&k) {
                    faltando.push((dx * dx + dz * dz, k.0, k.1));
                }
            }
        }
        faltando.sort_unstable();
        for (_, cx, cz) in faltando.into_iter().take(orcamento) {
            let p = self.constroi(cx, cz);
            self.pedacos.insert((cx, cz), p);
            self.gerados += 1;
        }
    }

    /// Desenha so' o que a camera alcanca.
    ///
    /// O raio de geracao existe pra o mundo ja' estar pronto quando o jogador
    /// virar; DESENHAR tudo ele e' desperdicio. Com a camera de cima, o que
    /// cabe na tela e' uma cunha — e' um teste de cone por pedaco, e ele
    /// derruba tres quartos das chamadas sem mudar um pixel.
    ///
    /// Devolve quantos pedacos foram desenhados, pro HUD.
    pub fn desenha(&self, cam: &Camera3D) -> usize {
        let olho = cam.position;
        let frente = (cam.target - cam.position).normalize();
        // Meia abertura generosa: melhor desenhar um pedaco a mais na borda
        // que abrir um buraco quando o jogador gira depressa.
        let cos_limite = (cam.fovy * 0.5 + 0.55).cos();
        let raio_pedaco = CHUNK as f32 * BLOCO * 0.87; // meia diagonal
        let mut desenhados = 0;
        for ((cx, cz), p) in &self.pedacos {
            let centro = vec3(
                (*cx as f32 + 0.5) * CHUNK as f32 * BLOCO,
                olho.y * 0.5,
                (*cz as f32 + 0.5) * CHUNK as f32 * BLOCO,
            );
            let d = centro - olho;
            let dist = d.length();
            // Pedaco em cima do olho passa sempre: normalizar vetor curto e'
            // ruido, e ele esta' na tela de qualquer jeito.
            if dist > raio_pedaco {
                // Folga angular proporcional ao tamanho do pedaco na distancia.
                let folga = (raio_pedaco / dist).min(1.0).asin();
                if d.normalize().dot(frente) < (cos_limite.acos() + folga).cos() {
                    continue;
                }
            }
            desenhados += 1;
            for m in &p.malhas {
                draw_mesh(m);
            }
        }
        desenhados
    }

    /// Onde o raio da tela encosta no chao.
    ///
    /// Marcha pelo raio e para quando ele passa abaixo do relevo, depois
    /// refina por bisseccao. Nao ha' malha pra intersectar — o campo de
    /// altura responde em O(1), entao vinte amostras custam menos que montar
    /// uma estrutura de colisao.
    pub fn onde_o_raio_bate(&self, origem: Vec3, dir: Vec3, alcance: f32) -> Option<Vec2> {
        let passo = 0.6;
        let mut t = 0.0f32;
        let mut anterior = origem;
        while t < alcance {
            t += passo;
            let p = origem + dir * t;
            if p.y <= self.altura(p.x, p.z) {
                // Bisseccao entre o ultimo ponto no ar e o primeiro no chao.
                let (mut a, mut b) = (anterior, p);
                for _ in 0..12 {
                    let m = (a + b) * 0.5;
                    if m.y <= self.altura(m.x, m.z) { b = m } else { a = m }
                }
                return Some(vec2(b.x, b.z));
            }
            anterior = p;
        }
        None
    }

    pub fn pedacos_vivos(&self) -> usize {
        self.pedacos.len()
    }

    fn pedaco_de(&self, p: Vec2) -> (i32, i32) {
        let bx = (p.x / BLOCO).floor() as i32;
        let bz = (p.y / BLOCO).floor() as i32;
        (bx.div_euclid(CHUNK), bz.div_euclid(CHUNK))
    }

    // ── malha ────────────────────────────────────────────────────────────

    fn constroi(&self, cx: i32, cz: i32) -> Pedaco {
        self.constroi_com(cx, cz, true)
    }

    /// `vegetacao = false` devolve so' o relevo.
    ///
    /// Existe pro teste de orientacao poder olhar o TERRENO sozinho: a
    /// vegetacao e' assada na mesma malha, e uma arvore tem faces legitimamente
    /// viradas pra baixo (a copa vista por baixo). Misturar as duas fazia o
    /// teste acusar como defeito o que e' a arvore fazendo o certo.
    fn constroi_com(&self, cx: i32, cz: i32, vegetacao: bool) -> Pedaco {
        let n = CHUNK as usize;
        // Uma coluna de borda de cada lado: a parede lateral precisa saber a
        // altura do vizinho, e sem a borda cada pedaco desenharia um muro
        // contra o vizinho que ainda nao existe.
        let lado = n + 2;
        let mut alt = vec![0i32; lado * lado];
        let mut agua = vec![false; lado * lado];
        for iz in 0..lado {
            for ix in 0..lado {
                let bx = cx * CHUNK + ix as i32 - 1;
                let bz = cz * CHUNK + iz as i32 - 1;
                let h = self.ger.bloco_em(bx, bz);
                agua[iz * lado + ix] = h <= BLOCO_DO_MAR;
                // O mar e' uma superficie so'. Sem isso o merge guloso nao
                // junta nada embaixo d'agua e a costa vira milhares de quads.
                alt[iz * lado + ix] = h.max(BLOCO_DO_MAR);
            }
        }
        let em = |ix: i32, iz: i32| alt[(iz + 1) as usize * lado + (ix + 1) as usize];
        let eh_agua = |ix: i32, iz: i32| agua[(iz + 1) as usize * lado + (ix + 1) as usize];

        let mut malhas: Vec<Mesh> = Vec::new();
        let mut verts: Vec<Vertex> = Vec::new();
        let mut idx: Vec<u16> = Vec::new();

        let mut quad4 = |verts: &mut Vec<Vertex>,
                         idx: &mut Vec<u16>,
                         malhas: &mut Vec<Mesh>,
                         p: [Vec3; 4],
                         n: Vec3,
                         cores: [[u8; 4]; 4]| {
            if idx.len() + 6 > MAX_QUADS * 6 {
                malhas.push(Mesh {
                    vertices: std::mem::take(verts),
                    indices: std::mem::take(idx),
                    texture: None,
                });
            }
            let b = verts.len() as u16;
            for (v, c) in p.into_iter().zip(cores) {
                verts.push(Vertex {
                    position: v,
                    uv: vec2(0.0, 0.0),
                    color: c,
                    // Relevo participa do recorte: penhasco esconde.
                    normal: Vec4::new(1.0, 0.0, 0.0, 0.0),
                });
            }
            idx.extend_from_slice(&ordem(b, p, n));
        };
        let quad = |verts: &mut Vec<Vertex>,
                        idx: &mut Vec<u16>,
                        malhas: &mut Vec<Mesh>,
                        p: [Vec3; 4],
                        n: Vec3,
                        c: [u8; 4]| {
            if idx.len() + 6 > MAX_QUADS * 6 {
                malhas.push(Mesh {
                    vertices: std::mem::take(verts),
                    indices: std::mem::take(idx),
                    texture: None,
                });
            }
            let b = verts.len() as u16;
            for v in p {
                verts.push(Vertex {
                    position: v,
                    uv: vec2(0.0, 0.0),
                    color: c,
                    // Relevo participa do recorte: penhasco esconde.
                    normal: Vec4::new(1.0, 0.0, 0.0, 0.0),
                });
            }
            idx.extend_from_slice(&ordem(b, p, n));
        };

        // ── topos, com merge guloso ──────────────────────────────────────
        // Medido no gerador: 66% a 80% das colunas de terra tem os quatro
        // vizinhos no mesmo nivel. Sem juntar, cada uma viraria um quad de
        // quatro vertices e o pedaco estouraria o teto da chamada sozinho.
        let mut usado = vec![false; n * n];
        for iz in 0..n as i32 {
            for ix in 0..n as i32 {
                if usado[iz as usize * n + ix as usize] {
                    continue;
                }
                let h = em(ix, iz);
                let a = eh_agua(ix, iz);
                let igual = |x: i32, z: i32| em(x, z) == h && eh_agua(x, z) == a;

                let mut w = 1;
                while w < MERGE_MAX
                    && ix + w < n as i32
                    && !usado[iz as usize * n + (ix + w) as usize]
                    && igual(ix + w, iz)
                {
                    w += 1;
                }
                let mut d = 1;
                'fundo: while d < MERGE_MAX && iz + d < n as i32 {
                    for i in 0..w {
                        if usado[(iz + d) as usize * n + (ix + i) as usize]
                            || !igual(ix + i, iz + d)
                        {
                            break 'fundo;
                        }
                    }
                    d += 1;
                }
                for z in 0..d {
                    for x in 0..w {
                        usado[(iz + z) as usize * n + (ix + x) as usize] = true;
                    }
                }

                let y = (h + 1) as f32 * BLOCO;
                // A coluna `i` fica CENTRADA em `i * BLOCO`, e nao comecando
                // nele. E' a convencao de `coluna()`, que faz o caminho
                // inverso com `round(x / BLOCO)` — e e' ela que a colisao, o
                // A* e o plantio usam. Desenhar a partir de `i * BLOCO`
                // deixava a malha meio bloco fora de fase com o proprio campo
                // de altura: o jogador pisava no degrau um quarto de bloco
                // antes de ver a quina, e a arvore plantada no centro da
                // coluna caia na divisa do bloco desenhado.
                let x0 = (cx * CHUNK + ix) as f32 * BLOCO - BLOCO * 0.5;
                let z0 = (cz * CHUNK + iz) as f32 * BLOCO - BLOCO * 0.5;
                let x1 = x0 + w as f32 * BLOCO;
                let z1 = z0 + d as f32 * BLOCO;
                // Declive do canto do quad: o merge so' junta colunas do mesmo
                // nivel, entao o desnivel que interessa esta' na borda dele.
                let declive = [(1, 0), (-1, 0), (0, 1), (0, -1)]
                    .iter()
                    .map(|(dx, dz)| (h - em(ix + dx, iz + dz)).abs())
                    .max()
                    .unwrap_or(0);
                let manchinha = self.ger.mancha(cx * CHUNK + ix, cz * CHUNK + iz);
                let mat = material_variado(self.bioma, y, declive, a, manchinha);
                // Grao POR VERTICE: mesmo com o teto de merge, um quad de 3x3
                // com uma cor so' ainda le' como azulejo. Variando os quatro
                // cantos, o interpolador espalha a variacao pelo quad inteiro
                // de graca — e' a placa de video fazendo textura sem textura.
                let (gx, gz) = (cx * CHUNK + ix, cz * CHUNK + iz);
                quad4(
                    &mut verts,
                    &mut idx,
                    &mut malhas,
                    [
                        vec3(x0, y, z0),
                        vec3(x1, y, z0),
                        vec3(x1, y, z1),
                        vec3(x0, y, z1),
                    ],
                    // Topo: olha pra cima.
                    Vec3::Y,
                    // CHAPADA, nao interpolada. Variar os cantos espalha o
                    // grao num gradiente macio, e o que o voxel pede e' o
                    // xadrez nitido de bloco contra bloco.
                    [self.cor(mat, gx, gz, h, 1.0, tom_da_mancha(manchinha)); 4],
                );
            }
        }

        // ── paredes ──────────────────────────────────────────────────────
        // Uma por coluna e por direcao em que o vizinho e' mais baixo. Nao ha'
        // merge aqui de proposito: parede e' 6% do mapa, e o codigo pra juntar
        // custaria mais do que economiza.
        for iz in 0..n as i32 {
            for ix in 0..n as i32 {
                if eh_agua(ix, iz) {
                    continue;
                }
                let h = em(ix, iz);
                let topo = (h + 1) as f32 * BLOCO;
                // Mesma fase dos topos: coluna centrada em `i * BLOCO`.
                let x0 = (cx * CHUNK + ix) as f32 * BLOCO - BLOCO * 0.5;
                let z0 = (cz * CHUNK + iz) as f32 * BLOCO - BLOCO * 0.5;
                let x1 = x0 + BLOCO;
                let z1 = z0 + BLOCO;
                for (dx, dz) in [(1i32, 0i32), (-1, 0), (0, 1), (0, -1)] {
                    let hv = em(ix + dx, iz + dz);
                    if hv >= h {
                        continue;
                    }
                    // O pe' da parede para na linha d'agua: penhasco na costa
                    // desceria ate' o fundo do talude, geometria que ninguem ve'.
                    let piso = hv.max(((NIVEL_DO_MAR / BLOCO) as i32) - 2);
                    let topo_mat = material_variado(
                        self.bioma, topo, declive_col(&em, ix, iz), false,
                        self.ger.mancha(cx * CHUNK + ix, cz * CHUNK + iz),
                    );
                    let lado_x = dx != 0;
                    let luz = if lado_x { 0.74 } else { 0.58 };
                    // Desce a parede em FAIXAS do mesmo material: grama no
                    // primeiro bloco, terra ate' o terceiro, rocha no resto.
                    // Sem isso um barranco de dez blocos sai de uma cor so'.
                    let mut b = h;
                    while b > piso {
                        let mat = material_de_profundidade(
                            self.bioma, topo_mat, topo, h - b,
                        );
                        let mut fim = b;
                        while fim > piso
                            && material_de_profundidade(
                                self.bioma, topo_mat, topo, h - fim,
                            ) == mat
                        {
                            fim -= 1;
                        }
                        let y1 = (b + 1) as f32 * BLOCO;
                        let y0 = (fim + 1) as f32 * BLOCO;
                        let c = self.cor(
                            mat, cx * CHUNK + ix, cz * CHUNK + iz, b, luz,
                            tom_da_mancha(self.ger.mancha(cx * CHUNK + ix, cz * CHUNK + iz)),
                        );
                        let p = if dx == 1 {
                            [vec3(x1, y0, z0), vec3(x1, y0, z1), vec3(x1, y1, z1), vec3(x1, y1, z0)]
                        } else if dx == -1 {
                            [vec3(x0, y0, z1), vec3(x0, y0, z0), vec3(x0, y1, z0), vec3(x0, y1, z1)]
                        } else if dz == 1 {
                            [vec3(x1, y0, z1), vec3(x0, y0, z1), vec3(x0, y1, z1), vec3(x1, y1, z1)]
                        } else {
                            [vec3(x0, y0, z0), vec3(x1, y0, z0), vec3(x1, y1, z0), vec3(x0, y1, z0)]
                        };
                        // A parede olha pro vizinho MAIS BAIXO, que e' o
                        // lado por onde ela e' vista.
                        let n = vec3(dx as f32, 0.0, dz as f32);
                        quad(&mut verts, &mut idx, &mut malhas, p, n, c);
                        b = fim;
                    }
                }
            }
        }

        // ── vegetacao ────────────────────────────────────────────────────
        if vegetacao {
        // Assada na malha do PEDACO, nao desenhada por arvore: pedaco e'
        // cache, entao vegetacao estatica custa zero por quadro. Desenhar uma
        // a uma exigiria transformar a malha na CPU (a macroquad nao tem
        // transform por malha) centenas de vezes por frame.
        for iz in 0..n as i32 {
            for ix in 0..n as i32 {
                let (bx, bz) = (cx * CHUNK + ix, cz * CHUNK + iz);
                let topo = em(ix, iz);
                let declive = [(1, 0), (-1, 0), (0, 1), (0, -1)]
                    .iter()
                    .map(|(dx, dz)| (topo - em(ix + dx, iz + dz)).abs())
                    .max()
                    .unwrap_or(0);
                // QUEM planta e' o `shared`, nao este laco. O tronco barra
                // passagem, entao o lugar da arvore virou regra — e o
                // servidor precisa chegar exatamente na mesma resposta sem
                // nunca ter visto a malha. Aqui so' se DESENHA o que ele
                // decidiu.
                let Some(a) = shared::terreno::arvore_da_coluna(
                    self.bioma, bx, bz, topo, declive, &self.ger, eh_agua(ix, iz),
                ) else {
                    continue;
                };
                crate::vegetacao::instancia(
                    self.modelo_de_arvore(a.especie, a.variante),
                    vec3(a.centro.x, (topo + 1) as f32 * BLOCO, a.centro.y),
                    a.porte,
                    &mut verts, &mut idx, &mut malhas, MAX_QUADS,
                );
            }
        }

        // ── forracao ─────────────────────────────────────────────────────
        // Varias vezes mais densa que arvore, e e' ela que separa "campo de
        // golfe com arvore" de mundo. Cada planta sao uma ou duas caixinhas.
        for iz in 0..n as i32 {
            for ix in 0..n as i32 {
                let (bx, bz) = (cx * CHUNK + ix, cz * CHUNK + iz);
                let topo = em(ix, iz);
                let declive = [(1, 0), (-1, 0), (0, 1), (0, -1)]
                    .iter()
                    .map(|(dx, dz)| (topo - em(ix + dx, iz + dz)).abs())
                    .max()
                    .unwrap_or(0);
                let Some(pl) = shared::terreno::planta_da_coluna(
                    self.bioma, bx, bz, topo, declive, &self.ger, eh_agua(ix, iz),
                ) else {
                    continue;
                };
                #[cfg(test)]
                CENSO.with(|c| {
                    *c.borrow_mut().entry(format!("{:?}", pl.especie)).or_insert(0) += 1
                });
                crate::vegetacao::instancia(
                    self.modelo_de_planta(pl.especie, pl.variante),
                    vec3(pl.centro.x, (topo + 1) as f32 * BLOCO, pl.centro.y),
                    pl.porte,
                    &mut verts, &mut idx, &mut malhas, MAX_QUADS,
                );
            }
        }

        }

        if !verts.is_empty() {
            malhas.push(Mesh { vertices: verts, indices: idx, texture: None });
        }
        Pedaco { malhas }
    }

    /// Cor de uma face, ja' com a luz do lado aplicada.
    ///
    /// A variacao vem da POSICAO do quad e nao da coluna: por coluna, cada
    /// vizinha teria cor propria e o merge guloso deixaria de juntar — o
    /// pedaco quadruplicaria pra ganhar um chiado que ninguem ve' de cima.
    fn cor(&self, mat: Material, bx: i32, bz: i32, by: i32, luz: f32, tom: f32) -> [u8; 4] {
        let (r, g, b) = mat.rgb();
        // hash barato e estavel: mesmo quad, mesma cor, sempre.
        // GRAO DO VOXEL, e nao do quad: `(x*7 + z*13 + y*5) & 7` varia por
        // bloco, inclusive na vertical. Ideia do Sea of Cubes.
        //
        // A amplitude e' NOSSA e e' maior que a deles. Copiei os ±3 originais e
        // o chao continuou chapado: tres em 255 e' um por cento, e um por
        // cento nao se ve'. Eles se dao ao luxo porque a camera e' de terceira
        // pessoa e o chao ocupa pouco quadro; aqui ele ocupa quase tudo.
        const GRAO: f32 = 9.0;
        let grao = ((bx.wrapping_mul(7) + bz.wrapping_mul(13) + by.wrapping_mul(5)) & 7) as f32;
        let g4 = (grao / 7.0 - 0.5) * 2.0 * GRAO;
        let luz = luz * tom;
        [
            (r as f32 * luz + g4).clamp(0.0, 255.0) as u8,
            (g as f32 * luz + g4).clamp(0.0, 255.0) as u8,
            (b as f32 * luz + g4).clamp(0.0, 255.0) as u8,
            255,
        ]
    }
}

/// Maior desnivel entre a coluna e os quatro vizinhos.
fn declive_col(em: &impl Fn(i32, i32) -> i32, ix: i32, iz: i32) -> i32 {
    let h = em(ix, iz);
    [(1, 0), (-1, 0), (0, 1), (0, -1)]
        .iter()
        .map(|(dx, dz)| (h - em(ix + dx, iz + dz)).abs())
        .max()
        .unwrap_or(0)
}

#[cfg(test)]
thread_local! {
    /// Censo de especies colocadas, so' em teste. Contar o que o gerador
    /// realmente POE e' diferente de conferir a tabela de sorteio: entre uma e
    /// outra ha' filtros de solo, agua e declive que podem comer uma especie
    /// inteira sem ninguem notar.
    static CENSO: std::cell::RefCell<std::collections::HashMap<String, u32>> =
        std::cell::RefCell::new(std::collections::HashMap::new());
}

#[cfg(test)]
mod testes {
    use super::*;
    use shared::terreno::ARQUIPELAGO;

    /// A flor tem que CHEGAR ao chao, nao so' existir na tabela.
    #[test]
    fn a_flor_e_de_fato_plantada() {
        let t = Terreno::novo(&ARQUIPELAGO[0]);
        CENSO.with(|c| c.borrow_mut().clear());
        for cz in -3..3 {
            for cx in -3..3 {
                let _ = t.constroi(cx, cz);
            }
        }
        let censo = CENSO.with(|c| c.borrow().clone());
        let total: u32 = censo.values().sum();
        let mut linhas: Vec<_> = censo.iter().collect();
        linhas.sort();
        for (k, v) in &linhas {
            println!("  {k:<12} {v:>5}  ({:.0}%)", **v as f32 * 100.0 / total.max(1) as f32);
        }
        let flores = censo.get("Flor").copied().unwrap_or(0);
        assert!(total > 100, "so' {total} plantas em 36 pedacos");
        assert!(
            flores * 100 / total.max(1) > 10,
            "so' {flores} flores de {total} plantas"
        );
    }

    /// Nenhuma malha pode passar do teto de INDICE da macroquad.
    ///
    /// Este teste existe por causa de um bug que custou caro: com malha acima
    /// de 5.000 indices a macroquad descarta metade em silencio, e o sintoma
    /// — buracos pretos em tabuleiro — parece bug de merge guloso ou de
    /// distancia de desenho. Foram 237 mil avisos no log antes de alguem
    /// olhar. Vegetacao so' faz o orcamento apertar, entao ele fica guardado.
    /// O CONTRATO: o que o cliente desenha e' o que o servidor barra.
    ///
    /// Os dois chegam ao plantio por caminhos diferentes — o cliente gera a
    /// coluna na hora, o servidor le' o campo de altura que ele guardou — e a
    /// unica coisa que garante que batem e' os dois chamarem a MESMA funcao.
    /// Este teste e' o que segura isso: se alguem replicar a conta de um lado
    /// so', aparecem arvores atravessaveis ou paredes invisiveis.
    #[test]
    fn o_que_o_cliente_planta_o_servidor_barra() {
        use shared::terreno::{Ilha, ESCALA_ALTURA};
        let d = &ARQUIPELAGO[0];
        let t = Terreno::novo(d);
        let i = Ilha::carregar_ou_gerar("", d.semente, d.raio_blocos, d.bioma, ESCALA_ALTURA);
        let (mut solidos, mut vazados) = (0, 0);
        for bz in -160..160 {
            for bx in -160..160 {
                let topo = t.ger.bloco_em(bx, bz);
                let agua = (topo + 1) as f32 * BLOCO <= shared::terreno::NIVEL_DO_MAR;
                let declive = [(1, 0), (-1, 0), (0, 1), (0, -1)]
                    .iter()
                    .map(|(dx, dz)| (topo - t.ger.bloco_em(bx + dx, bz + dz)).abs())
                    .max()
                    .unwrap_or(0);
                // Uma arvore desenhada tem que ser um estorvo no servidor.
                if let Some(a) = shared::terreno::arvore_da_coluna(
                    d.bioma, bx, bz, topo, declive, &t.ger, agua,
                ) {
                    solidos += 1;
                    assert!(
                        !i.sem_estorvo(a.centro, 0.01),
                        "arvore desenhada em {:?} nao barra no servidor", a.centro
                    );
                }
                let Some(pl) = shared::terreno::planta_da_coluna(
                    d.bioma, bx, bz, topo, declive, &t.ger, agua,
                ) else {
                    continue;
                };
                match shared::terreno::raio_de_planta(pl.especie) {
                    // Matacao e toco barram.
                    Some(_) => {
                        solidos += 1;
                        assert!(
                            !i.sem_estorvo(pl.centro, 0.01),
                            "{:?} desenhada em {:?} nao barra no servidor",
                            pl.especie, pl.centro
                        );
                    }
                    // Flor, capim, arbusto, samambaia e talo NAO barram — e
                    // este lado do teste importa tanto quanto o outro: uma
                    // forracao solida viraria labirinto.
                    None => {
                        vazados += 1;
                        // So' vale se nao houver um tronco ali por acaso.
                        if i.sem_estorvo(pl.centro, 0.01) {
                            continue;
                        }
                    }
                }
            }
        }
        println!("{solidos} estorvos e {vazados} plantas vazadas conferidos");
        assert!(solidos > 20, "so' {solidos} estorvos na amostra — teste vazio");
        assert!(vazados > 100, "so' {vazados} plantas vazadas na amostra");
    }

    #[test]
    fn nenhum_pedaco_estoura_o_teto_de_indice() {
        for d in ARQUIPELAGO.iter() {
            let t = Terreno::novo(d);
            let (mut pior_i, mut pior_v, mut total) = (0usize, 0usize, 0usize);
            // Pedacos espalhados pela ilha: perto do centro e' onde ha' mais
            // vegetacao, na borda ha' mais recorte de costa.
            for (cx, cz) in [(0, 0), (3, 2), (-5, 4), (12, -8), (-20, -20), (30, 10)] {
                let p = t.constroi(cx, cz);
                total += p.malhas.len();
                for m in &p.malhas {
                    pior_i = pior_i.max(m.indices.len());
                    pior_v = pior_v.max(m.vertices.len());
                }
            }
            println!(
                "{:<14} pior malha: {pior_v} vertices, {pior_i} indices ({total} malhas)",
                d.zona
            );
            assert!(pior_i <= 5_000, "{}: {pior_i} indices — a macroquad corta em 5.000", d.zona);
            assert!(pior_v <= 10_000, "{}: {pior_v} vertices", d.zona);
        }
    }

}

#[cfg(test)]
mod testes_orientacao {
    use super::*;
    use shared::terreno::ARQUIPELAGO;

    /// Toda face do terreno tem que apontar pra FORA do solido.
    ///
    /// E' o que permite ligar o descarte de face de costas: com o enrolamento
    /// invertido em alguma direcao, a GPU jogaria fora justamente as faces
    /// visiveis e o chao apareceria com buracos — e o buraco so' aparece de
    /// um lado, entao passa despercebido ate' o jogador virar a camera.
    ///
    /// A conta nao confia em nenhuma anotacao: tira a normal do PRODUTO
    /// VETORIAL dos vertices e compara com o relevo dos dois lados dela.
    #[test]
    fn as_faces_do_terreno_apontam_pra_fora() {
        let d = &ARQUIPELAGO[0];
        let t = Terreno::novo(d);
        let (mut horizontais, mut verticais) = (0, 0);

        for (cx, cz) in [(0, 0), (3, 2), (-5, 4), (12, -8)] {
            let p = t.constroi_com(cx, cz, false);
            for malha in &p.malhas {
                for tri in malha.indices.chunks(3) {
                    let [a, b, c] = [
                        malha.vertices[tri[0] as usize].position,
                        malha.vertices[tri[1] as usize].position,
                        malha.vertices[tri[2] as usize].position,
                    ];
                    let n = (b - a).cross(c - a);
                    if n.length_squared() < 1e-9 {
                        continue; // degenerado: nao desenha nada
                    }
                    let n = n.normalize();
                    let centro = (a + b + c) / 3.0;
                    if n.y.abs() > 0.9 {
                        // Face horizontal: o terreno so' tem TOPO, nunca
                        // fundo — nada e' desenhado por baixo do chao.
                        // O terreno so' tem TOPO: nada e' desenhado por
                        // baixo do chao, entao face horizontal virada pra
                        // baixo e' face que a GPU vai descartar.
                        horizontais += 1;
                        assert!(n.y > 0.0, "topo em {centro:?} aponta pra baixo");
                    } else if n.y.abs() < 0.1 {
                        // Face vertical: ela existe porque um lado e' mais
                        // alto que o outro, e tem que olhar pro lado BAIXO.
                        verticais += 1;
                        let fora = centro + n * 0.3;
                        let dentro = centro - n * 0.3;
                        let h_fora = t.altura(fora.x, fora.z);
                        let h_dentro = t.altura(dentro.x, dentro.z);
                        // A parede existe porque um lado e' mais alto: ela
                        // tem que olhar pro lado BAIXO, que e' de onde se ve'.
                        assert!(
                            h_fora < h_dentro + 1e-3,
                            "parede em {centro:?} olha pro lado ALTO \
                             ({h_fora:.2} contra {h_dentro:.2})"
                        );
                    }
                }
            }
        }
        assert!(horizontais > 5_000, "so' {horizontais} topos conferidos");
        assert!(verticais > 1_000, "so' {verticais} paredes conferidas");
        println!("{horizontais} topos e {verticais} paredes, todos pra fora");
    }
}

#[cfg(test)]
mod testes_alinhamento {
    use super::*;
    use shared::terreno::ARQUIPELAGO;

    /// O chao DESENHADO e o chao CONSULTADO tem que ser o mesmo chao.
    ///
    /// A malha desenha a coluna `i` de `i*BLOCO` a `i*BLOCO + BLOCO`, e
    /// `altura()` converte mundo pra coluna com `round(x / BLOCO)`, que cobre
    /// `i*BLOCO - BLOCO/2` a `i*BLOCO + BLOCO/2`. Se as duas convencoes
    /// discordarem, o jogador pisa num degrau um quarto de bloco antes ou
    /// depois de ver a quina — e o erro so' aparece na borda, que e'
    /// justamente onde ele importa.
    #[test]
    fn o_chao_desenhado_e_o_chao_consultado() {
        let d = &ARQUIPELAGO[0];
        let t = Terreno::novo(d);
        let mut conferidos = 0;
        for (cx, cz) in [(0, 0), (3, 2), (-5, 4)] {
            let p = t.constroi_com(cx, cz, false);
            for malha in &p.malhas {
                for tri in malha.indices.chunks(3) {
                    let v: Vec<Vec3> = tri
                        .iter()
                        .map(|&i| malha.vertices[i as usize].position)
                        .collect();
                    // So' topos: os tres vertices no mesmo Y.
                    if (v[0].y - v[1].y).abs() > 1e-4 || (v[0].y - v[2].y).abs() > 1e-4 {
                        continue;
                    }
                    let c = (v[0] + v[1] + v[2]) / 3.0;
                    let consultada = t.altura(c.x, c.z);
                    assert!(
                        (consultada - c.y).abs() < 1e-3,
                        "topo desenhado em y={:.3} no ponto ({:.3}, {:.3}), \
                         mas altura() diz {:.3}",
                        c.y, c.x, c.z, consultada
                    );
                    conferidos += 1;
                }
            }
        }
        assert!(conferidos > 1000, "so' {conferidos} topos conferidos");
        println!("{conferidos} topos conferidos: desenho e consulta batem");
    }
}
