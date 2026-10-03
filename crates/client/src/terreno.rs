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

use crate::gpu_estatica::MalhaEstatica;

use shared::terreno::{
    material_de_profundidade, material_variado, tom_da_mancha, Arvore, Bioma, DefIlha, Gerador,
    Material, Planta, BLOCO, NIVEL_DO_MAR,
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
    /// Na GPU a partir do primeiro desenho (`gpu_estatica`).
    malhas: Vec<MalhaEstatica>,
    /// Projecao suave das copas, preparada junto com a vegetacao.
    sombras: Vec<MalhaEstatica>,
    /// A superficie do mar deste pedaco (`agua::malhas_do_pedaco`), com
    /// material proprio. Vazia em pedaco todo terra.
    agua: Vec<MalhaEstatica>,
    /// O PE' de cada veio de Energia do pedaco. Ele nao entra na malha
    /// assada: e' desenhado quadro a quadro, com a luz subindo
    /// (`energia_vfx`). Assado, ele era uma pedra parada.
    energias: Vec<Vec3>,
    /// The Graphics ground-cover percent this chunk was baked with. When the
    /// setting changes, `atualiza` rebuilds the chunks that disagree.
    forracao: u8,
}

/// Whether the ground plant on column `(bx, bz)` survives a `pct` percent
/// density. A fixed hash, not a random draw: the same bush stays (or goes)
/// every time the chunk is rebuilt, and Sparse is a subset of Medium.
fn planta_fica(bx: i32, bz: i32, pct: u8) -> bool {
    if pct >= 100 {
        return true;
    }
    let h = (bx as u32).wrapping_mul(0x9E37_79B1) ^ (bz as u32).wrapping_mul(0x85EB_CA77);
    let h = (h ^ (h >> 15)).wrapping_mul(0x2C1B_3C6D);
    ((h >> 16) % 100) < pct as u32
}

/// O pedaco `(cx, cz)` cai no cone da camera? O mesmo teste pro chao e pro
/// mar: dois testes seriam dois lugares pro buraco aparecer.
fn pedaco_visivel(cam: &Camera3D, cx: i32, cz: i32) -> bool {
    let olho = cam.position;
    let frente = (cam.target - cam.position).normalize();
    // Meia abertura generosa: melhor desenhar um pedaco a mais na borda
    // que abrir um buraco quando o jogador gira depressa.
    let cos_limite = (cam.fovy * 0.5 + 0.55).cos();
    let raio_pedaco = CHUNK as f32 * BLOCO * 0.87; // meia diagonal
    let centro = vec3(
        (cx as f32 + 0.5) * CHUNK as f32 * BLOCO,
        olho.y * 0.5,
        (cz as f32 + 0.5) * CHUNK as f32 * BLOCO,
    );
    let d = centro - olho;
    let dist = d.length();
    // Pedaco em cima do olho passa sempre: normalizar vetor curto e'
    // ruido, e ele esta' na tela de qualquer jeito.
    if dist <= raio_pedaco {
        return true;
    }
    // Folga angular proporcional ao tamanho do pedaco na distancia.
    let folga = (raio_pedaco / dist).min(1.0).asin();
    d.normalize().dot(frente) >= (cos_limite.acos() + folga).cos()
}

pub struct Terreno {
    ger: Gerador,
    /// Não emite coluna de ÁGUA nenhuma (nem o plano que ela cobria).
    ///
    /// Só a maquete da colônia liga isto: ela é uma peça flutuando, e o
    /// plano do nível do mar em volta da ilha virava uma mesa.
    sem_mar: bool,
    ilhas_aereas: Option<crate::ilhas_aereas::IlhasAereas>,
    bioma: Bioma,
    /// Aparencia do solo da arena, escolhida pelo conteudo da dungeon.
    bioma_visual: Bioma,
    /// Modelos de vegetacao assados uma vez por especie e variante. Instanciar
    /// e' copiar vertice com offset — a macroquad nao tem transform por malha,
    /// entao gerar de novo por planta seria refazer o volume milhares de vezes.
    arvores: Vec<crate::vegetacao::Modelo>,
    plantas: Vec<crate::vegetacao::Modelo>,
    /// Um modelo por tier e variante. A pedra roxa nao e' a cinza pintada:
    /// ela tem mais cristal, e o modelo carrega isso.
    minerios: Vec<crate::vegetacao::Modelo>,
    /// Cristais altos de Energia, separados visualmente do minério.
    /// Colunas cujas pedras ja' foram esgotadas. Quem esta' aqui NAO e'
    /// desenhado — sem isso o jogador nao teria como distinguir o veio cheio
    /// do veio que ele acabou de limpar.
    esgotadas: std::collections::HashSet<u32>,
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
    /// Que gerador responde pela coluna, e em que coordenada dele.
    ///
    /// Numa ilha e' sempre o proprio, sem deslocamento. No mar sao quatro
    /// testes de distancia — e 99% das colunas do mar aberto nao caem em
    /// nenhum disco, e voltam o gerador da ilha 0 com a coluna bem longe
    /// dela, que e' fundo de mar.
    pub fn novo(def: &DefIlha) -> Self {
        Self::do_gerador(Gerador::da_ilha(def), def.bioma)
    }

    /// A COLONIA (docs/COLONIA.md): ilha fixa, PLATO pelo assentamento.
    ///
    /// Semente e raio sao constantes dos dois lados — o que vem pelo fio e' o
    /// plato, que e' a unica coisa que muda. Passa pelo MESMO
    /// `Gerador::da_colonia` do servidor: duas montagens soltas dariam dois
    /// relevos, e o jogador andaria num chao que nao ve'.
    pub fn da_colonia(plato: f32) -> Self {
        let mut t = Self::do_gerador(Gerador::da_colonia(plato), shared::terreno::Bioma::Floresta);
        // A colônia é DIORAMA: sem mar, e sem o plano que o mar cobria.
        t.sem_mar = true;
        t
    }

    fn do_gerador(ger: Gerador, bioma: shared::terreno::Bioma) -> Self {
        let aerea = ger.e_aerea();
        let celeste = ger.e_celeste();
        let mut t = Self {
            ger,
            bioma,
            bioma_visual: bioma,
            sem_mar: aerea,
            ilhas_aereas: aerea.then(|| {
                let nuvens = crate::ilhas_aereas::IlhasAereas::nova();
                if celeste { nuvens.com_raizes_celestes() } else { nuvens }
            }),
            arvores: Vec::new(),
            plantas: Vec::new(),
            minerios: Vec::new(),
            esgotadas: std::collections::HashSet::new(),
            pedacos: HashMap::new(),
            gerados: 0,
        };
        // VARIANTES por especie: uma arvore so' por especie faz a mata inteira
        // ler como papel de parede, que e' o mesmo defeito de ter uma especie
        // so'. Seis dao variedade sem encher a memoria.
        for e in [
            Arvore::Copada,
            Arvore::Betula,
            Arvore::Pinheiro,
            Arvore::Seca,
            Arvore::Sagrada,
            Arvore::Salgueiro,
        ] {
            for k in 0..VARIANTES {
                t.arvores
                    .push(crate::vegetacao::arvore(e, k as u32 * 7 + e as u32 * 101));
            }
        }
        for e in [
            Planta::Moita,
            Planta::Flor,
            Planta::Arbusto,
            Planta::Samambaia,
            Planta::Pedra,
            Planta::Toco,
            Planta::Talo,
            Planta::Lirio,
            Planta::Pena,
            Planta::CristalCeu,
        ] {
            for k in 0..VARIANTES {
                t.plantas
                    .push(crate::vegetacao::planta(e, k as u32 * 13 + e as u32 * 211));
            }
        }
        for tier in 1..=4u8 {
            for k in 0..VARIANTES {
                t.minerios.push(crate::vegetacao::minerio(
                    tier,
                    k as u32 * 17 + tier as u32 * 331,
                ));
            }
        }
        for k in 0..VARIANTES {}
        t
    }

    /// A arena compartilha a mesma geometria e colisao entre dungeons; so'
    /// o solo muda para acompanhar a ilha do conteudo ativo.
    pub fn tema_da_dungeon(&mut self, bioma: Bioma) {
        if self.ger.e_arena() && self.bioma_visual != bioma {
            self.bioma_visual = bioma;
            self.pedacos.clear();
        }
    }

    fn material_do_tema(&self, altura: f32, declive: i32, agua: bool, mancha: f32) -> Material {
        if self.ger.e_arena() && !agua {
            return match self.bioma_visual {
                Bioma::Gelo => if mancha > 0.82 { Material::Gelo } else { Material::Neve },
                Bioma::Deserto => if mancha > 0.86 { Material::Arenito } else { Material::Areia },
                Bioma::Montanha => Material::Rocha,
                Bioma::Floresta | Bioma::Celeste | Bioma::Neon => material_variado(self.bioma_visual, altura, declive, false, mancha),
            };
        }
        material_variado(self.bioma_visual, altura, declive, agua, mancha)
    }

    fn modelo_de_minerio(&self, tier: u8, k: u32) -> &crate::vegetacao::Modelo {
        let t = (tier.clamp(1, 4) - 1) as usize;
        &self.minerios[t * VARIANTES + (k as usize % VARIANTES)]
    }

    /// Marca uma pedra como esgotada (ou de volta) e joga fora o pedaco que a
    /// contem, pra ela sumir (ou reaparecer) no proximo quadro.
    ///
    /// Reconstruir o pedaco inteiro por uma pedra parece caro e nao e': o
    /// pedaco leva ~0,9 ms e isso acontece uma vez por coleta que ACABA com
    /// uma pedra, nao por coleta.
    pub fn marca_esgotada(&mut self, coluna: u32, esgotada: bool) {
        let mudou = if esgotada {
            self.esgotadas.insert(coluna)
        } else {
            self.esgotadas.remove(&coluna)
        };
        if !mudou {
            return;
        }
        let (ix, iz) = ((coluna >> 16) as i32, (coluna & 0xffff) as i32);
        let (bx, bz) = (ix - self.ger.raio_blocos, iz - self.ger.raio_blocos);
        self.pedacos
            .remove(&(bx.div_euclid(CHUNK), bz.div_euclid(CHUNK)));
    }

    /// A pedra ou tronco VIVO mais perto de `p` (borda a ate' `raio`):
    /// (coluna, centro, tipo 0 madeira / 1..4 pedra, raio do corpo). Pro
    /// clique no mundo virar "coletar isto". Sai do mesmo
    /// `estorvos_da_coluna` que o servidor planta, entao coluna e raio batem.
    pub fn coletavel_perto(&self, p: Vec2, raio: f32) -> Option<(u32, Vec2, u8, f32)> {
        use shared::terreno::TipoDeEstorvo;
        let r = (raio / BLOCO).ceil() as i32 + 3;
        let (cx, cz) = ((p.x / BLOCO).round() as i32, (p.y / BLOCO).round() as i32);
        let mut buf = Vec::new();
        let mut melhor: Option<(u32, Vec2, u8, f32, f32)> = None;
        for bz in cz - r..=cz + r {
            for bx in cx - r..=cx + r {
                let topo = self.ger.bloco_em(bx, bz);
                let agua = (topo + 1) as f32 * BLOCO <= NIVEL_DO_MAR;
                let declive = [(1, 0), (-1, 0), (0, 1), (0, -1)]
                    .iter()
                    .map(|(dx, dz)| (topo - self.ger.bloco_em(bx + dx, bz + dz)).abs())
                    .max()
                    .unwrap_or(0);
                buf.clear();
                shared::terreno::estorvos_da_coluna(
                    self.bioma, bx, bz, topo, declive, &self.ger, agua, &mut buf,
                );
                for e in buf.drain(..) {
                    let tipo = match e.tipo {
                        TipoDeEstorvo::Minerio(t) => t,
                        TipoDeEstorvo::Energia => 5,
                        TipoDeEstorvo::Tronco => 0,
                        TipoDeEstorvo::Forracao => continue,
                    };
                    if self.esgotadas.contains(&e.coluna) {
                        continue;
                    }
                    let borda = e.centro.distance(::glam::Vec2::new(p.x, p.y)) - e.raio;
                    if borda <= raio && melhor.is_none_or(|m| borda < m.4) {
                        melhor =
                            Some((e.coluna, vec2(e.centro.x, e.centro.y), tipo, e.raio, borda));
                    }
                }
            }
        }
        melhor.map(|(k, c, t, rr, _)| (k, c, t, rr))
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
            (raio, 0.0),
            (-raio, 0.0),
            (0.0, raio),
            (0.0, -raio),
            (r, r),
            (-r, r),
            (r, -r),
            (-r, -r),
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
        self.pedacos
            .retain(|(cx, cz), _| (cx - ccx).abs() <= raio + 1 && (cz - ccz).abs() <= raio + 1);

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
        let mut sobra = orcamento;
        for (_, cx, cz) in faltando.into_iter().take(orcamento) {
            let p = self.constroi(cx, cz);
            self.pedacos.insert((cx, cz), p);
            self.gerados += 1;
            sobra -= 1;
        }
        // The ground cover changed in Graphics: rebuild in place, nearest
        // first, with what is left of the budget. Swapping chunk by chunk
        // never opens a hole, where dropping them all would.
        if sobra > 0 {
            let forracao = crate::config_graficos::forracao();
            let mut velhos: Vec<(i32, i32, i32)> = self
                .pedacos
                .iter()
                .filter(|(_, p)| p.forracao != forracao)
                .map(|(&(cx, cz), _)| ((cx - ccx).pow(2) + (cz - ccz).pow(2), cx, cz))
                .collect();
            velhos.sort_unstable();
            for (_, cx, cz) in velhos.into_iter().take(sobra) {
                let p = self.constroi(cx, cz);
                self.pedacos.insert((cx, cz), p);
            }
        }
    }

    /// Quantos pedacos em volta de `centro` (ate' `raio`) ja' existem, e de
    /// quantos. E' o que a tela de carregando espera.
    pub fn prontos_em(&self, centro: Vec2, raio: i32) -> (usize, usize) {
        let (ccx, ccz) = self.pedaco_de(centro);
        let mut feitos = 0;
        for dz in -raio..=raio {
            for dx in -raio..=raio {
                feitos += self.pedacos.contains_key(&(ccx + dx, ccz + dz)) as usize;
            }
        }
        (feitos, ((2 * raio + 1) * (2 * raio + 1)) as usize)
    }

    /// Desenha so' o que a camera alcanca.
    ///
    /// O raio de geracao existe pra o mundo ja' estar pronto quando o jogador
    /// virar; DESENHAR tudo ele e' desperdicio. Com a camera de cima, o que
    /// cabe na tela e' uma cunha — e' um teste de cone por pedaco, e ele
    /// derruba tres quartos das chamadas sem mudar um pixel.
    ///
    /// Devolve quantos pedacos foram desenhados, pro HUD.
    ///
    /// `recorte`/`recorte_z`: o furo que deixa ver o jogador (ver
    /// `render3d::recorte_do_jogador`); zero desliga.
    pub fn desenha(&self, cam: &Camera3D, recorte: Vec3, recorte_z: f32) -> usize {
        if let Some(cenario) = &self.ilhas_aereas {
            // A plateau's root draws only with the ground on top of it: far
            // plateaus outside the generated chunks showed as bare discs.
            cenario.desenha(&|x, z| {
                let b = |v: f32| (v / shared::terreno::BLOCO).floor() as i32;
                self.pedacos.contains_key(&(b(x).div_euclid(CHUNK), b(z).div_euclid(CHUNK)))
            });
        }
        let visiveis: Vec<&Pedaco> = self
            .pedacos
            .iter()
            .filter(|((cx, cz), _)| pedaco_visivel(cam, *cx, *cz))
            .map(|(_, p)| p)
            .collect();
        crate::gpu_estatica::desenha(
            crate::gpu_estatica::Programa::Solido { recorte, recorte_z },
            visiveis.iter().flat_map(|p| p.malhas.iter()),
        );
        if let Some(pl) = self.ger.planalto() {
            let agora = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0,|d| d.as_secs() as i64);
            if let Some((_,c)) = pl.campo(agora) {
                if vec2(c.x-cam.target.x,c.y-cam.target.z).length()<180.0 {
                    for i in 0..64 {
                        let angulo = i as f32*std::f32::consts::TAU/64.0;
                        let proximo = (i+1) as f32*std::f32::consts::TAU/64.0;
                        let a = vec2(c.x,c.y)+vec2(angulo.cos(),angulo.sin())*32.0;
                        let b = vec2(c.x,c.y)+vec2(proximo.cos(),proximo.sin())*32.0;
                        draw_line_3d(vec3(a.x,self.ger.altura(a.x,a.y)+0.15,a.y),vec3(b.x,self.ger.altura(b.x,b.y)+0.15,b.y),SKYBLUE);
                    }
                }
            }
        }
        visiveis.len()
    }

    pub fn desenha_sombras(&self, cam: &Camera3D) {
        crate::gpu_estatica::desenha(
            crate::gpu_estatica::Programa::Sombra,
            self.pedacos
                .iter()
                .filter(|((cx, cz), _)| pedaco_visivel(cam, *cx, *cz))
                .flat_map(|(_, p)| p.sombras.iter()),
        );
    }

    /// O pe' dos veios de Energia a` vista. Eles sao desenhados quadro a
    /// quadro (`energia_vfx`) e nao entram na malha assada do pedaco.
    pub fn energias_visiveis(&self, cam: &Camera3D) -> Vec<Vec3> {
        self.pedacos
            .iter()
            .filter(|((cx, cz), p)| !p.energias.is_empty() && pedaco_visivel(cam, *cx, *cz))
            .flat_map(|(_, p)| p.energias.iter().copied())
            .collect()
    }

    /// A superficie do mar dos pedacos visiveis. Quem chama ja' pos o
    /// material da agua (ver `agua::desenha`).
    pub fn sem_oceano(&self) -> bool { self.sem_mar }

    pub fn desenha_agua(&self, cam: &Camera3D, tempo: f32, ondas: f32) {
        crate::gpu_estatica::desenha(
            crate::gpu_estatica::Programa::Agua { tempo, ondas },
            self.pedacos
                .iter()
                .filter(|((cx, cz), p)| !p.agua.is_empty() && pedaco_visivel(cam, *cx, *cz))
                .flat_map(|(_, p)| p.agua.iter()),
        );
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
                    if m.y <= self.altura(m.x, m.z) {
                        b = m
                    } else {
                        a = m
                    }
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
        let forracao = crate::config_graficos::forracao();
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
        let mut energias: Vec<Vec3> = Vec::new();
        let mut sombras: Vec<Mesh> = Vec::new();
        let mut sombra_verts: Vec<Vertex> = Vec::new();
        let mut sombra_idx: Vec<u16> = Vec::new();

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
        // Tinta dos povoados (praca, caminho, gramado): entra na chave do
        // merge, senao um quad de grama engoliria a borda da calcada.
        let tinta = |x: i32, z: i32| {
            if eh_agua(x, z) {
                None
            } else {
                self.ger.pintura_do_chao(cx * CHUNK + x, cz * CHUNK + z)
            }
        };
        for iz in 0..n as i32 {
            for ix in 0..n as i32 {
                if usado[iz as usize * n + ix as usize] {
                    continue;
                }
                // SEM MAR: a coluna de água some em vez de virar chão.
                //
                // O malhador achata tudo abaixo do nível do mar num plano só
                // (`h.max(BLOCO_DO_MAR)`), e quem cobria aquilo era a água.
                // Na maquete da colônia não há água — ela é um diorama
                // flutuando —, e o plano virava uma MESA em volta da ilha.
                // Visto na prévia, duas vezes: primeiro bege, depois marrom,
                // quando tentei afundar o fundo (o `max` ignora isso).
                if self.sem_mar && eh_agua(ix, iz) {
                    usado[iz as usize * n + ix as usize] = true;
                    continue;
                }
                let h = em(ix, iz);
                let a = eh_agua(ix, iz);
                let t0 = tinta(ix, iz);
                let igual =
                    |x: i32, z: i32| em(x, z) == h && eh_agua(x, z) == a && tinta(x, z) == t0;

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
                let mat = self
                    .ger
                    .material_desenhado(cx * CHUNK + ix, cz * CHUNK + iz)
                    .unwrap_or_else(|| self.material_do_tema(y, declive, a, manchinha));
                let mat = match t0 {
                    // Grama cuidada so' onde ja' era grama: na Geleira a
                    // cidade continua branca.
                    Some(Material::GramaCuidada) => {
                        if matches!(
                            mat,
                            Material::Grama
                                | Material::GramaClara
                                | Material::GramaEscura
                                | Material::Terra
                        ) {
                            Material::GramaCuidada
                        } else {
                            mat
                        }
                    }
                    Some(m) => m,
                    None => mat,
                };
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
                    // Submerso e' LEITO: areia escurecendo com a fundura,
                    // que a agua rasa translucida deixa ver.
                    [if a {
                        crate::agua::cor_do_leito(self.ger.bloco_em(gx, gz), gx, gz)
                    } else {
                        self.cor(mat, gx, gz, h, 1.0, tom_da_mancha(manchinha))
                    }; 4],
                );
            }
        }

        // ── paredes ──────────────────────────────────────────────────────
        // Uma por coluna e por direcao em que o vizinho e' mais baixo. Nao ha'
        // merge aqui de proposito: parede e' 6% do mapa, e o codigo pra juntar
        // custaria mais do que economiza.
        for iz in 0..n as i32 {
            for ix in 0..n as i32 {
                if self.sem_mar && eh_agua(ix, iz) {
                    continue;
                }
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
                    // WHERE A DECK LANDS ON AN ISLAND. A bridge or cloud path
                    // is a thin deck as high as the island it reaches, so by
                    // the height rule the island's cliff under it was never
                    // drawn — and through the hole you saw the inside of the
                    // island (the owner's screenshot: "the pale beige wedge
                    // under the island"). Next to a deck, the island's cliff
                    // goes all the way down, from under the deck.
                    let deck_ao_lado = self.ger.e_aerea()
                        && self.ger.na_ponte_magica(cx * CHUNK + ix + dx, cz * CHUNK + iz + dz)
                        && !self.ger.na_ponte_magica(cx * CHUNK + ix, cz * CHUNK + iz);
                    if hv >= h && !deck_ao_lado {
                        continue;
                    }
                    // O pe' da parede para na linha d'agua: penhasco na costa
                    // desceria ate' o fundo do talude, geometria que ninguem ve'.
                    let piso = if deck_ao_lado {
                        if self.ger.e_magica() { 7 } else { -2 }
                    } else if self.ger.e_aerea() && eh_agua(ix + dx, iz + dz) {
                        let ponte = self.ger.na_ponte_magica(cx * CHUNK + ix, cz * CHUNK + iz);
                        // Bridges and cloud paths are thin decks. Skyreach's
                        // islands keep their cliff down to the sea line, where
                        // the rock root hanging below takes over.
                        hv.max(if ponte { h - 2 } else if self.ger.e_magica() { 7 } else { -2 })
                    } else { hv.max(((NIVEL_DO_MAR / BLOCO) as i32) - 2) };
                    let topo_mat = self.material_do_tema(
                        topo,
                        declive_col(&em, ix, iz),
                        false,
                        self.ger.mancha(cx * CHUNK + ix, cz * CHUNK + iz),
                    );
                    let lado_x = dx != 0;
                    let luz = if lado_x { 0.74 } else { 0.58 };
                    // Desce a parede em FAIXAS do mesmo material: grama no
                    // primeiro bloco, terra ate' o terceiro, rocha no resto.
                    // Sem isso um barranco de dez blocos sai de uma cor so'.
                    // A Porão's walls on the Arena are its own stone, in
                    // courses (`planta::Planta::pedra`), not the islet's soil.
                    let (gx, gz) = (cx * CHUNK + ix, cz * CHUNK + iz);
                    let porao = if self.ger.e_arena() {
                        shared::planta::complexo_em(::glam::Vec2::new(gx as f32 * BLOCO, gz as f32 * BLOCO))
                    } else {
                        None
                    };
                    // Last Refuge's castle walls: dressed stone in courses.
                    let castelo = self.ger.planalto().is_some_and(|pl| {
                        pl.parte_da_muralha(::glam::Vec2::new(gx as f32 * BLOCO, gz as f32 * BLOCO))
                            .is_some()
                    });
                    let nuvem = self.ger.material_desenhado(gx, gz) == Some(Material::Nuvem);
                    // Kōgen-tō's buildings: storeys of wall and window bands.
                    let q_kogen = ::glam::Vec2::new(gx as f32 * BLOCO, gz as f32 * BLOCO);
                    let predio = self.ger.e_kogen()
                        && !self.ger.na_cidade(gx, gz)
                        && matches!(shared::kogen::chao_em(q_kogen), shared::kogen::Chao::Predio { .. });
                    let faixa = |prof: i32| match porao {
                        _ if predio => shared::kogen::fachada(q_kogen, gx, gz, prof, h - shared::kogen::NIVEL_CHAO),
                        // A cloud path's edge: white on top, shaded below.
                        _ if nuvem => if prof == 0 { Material::Nuvem } else { Material::NuvemSombra },
                        Some(pl) => pl.pedra(gx, gz, prof),
                        None if castelo => {
                            let h = (gx as u32).wrapping_mul(73_856_093)
                                ^ (gz as u32).wrapping_mul(19_349_663)
                                ^ (prof as u32).wrapping_mul(83_492_791);
                            if (h >> 7) % 100 < 8 {
                                Material::GramaEscura
                            } else if prof % 2 == 0 {
                                Material::Rocha
                            } else {
                                Material::RochaEscura
                            }
                        }
                        None => material_de_profundidade(self.bioma_visual, topo_mat, topo, prof),
                    };
                    // Under a deck the face starts below it (the deck is two
                    // blocks thick), not at the island's top.
                    let mut b = if deck_ao_lado { h.min(hv - 2) } else { h };
                    while b > piso {
                        let mat = faixa(h - b);
                        let mut fim = b;
                        while fim > piso && faixa(h - fim) == mat {
                            fim -= 1;
                        }
                        let y1 = (b + 1) as f32 * BLOCO;
                        let y0 = (fim + 1) as f32 * BLOCO;
                        let c = self.cor(
                            mat,
                            cx * CHUNK + ix,
                            cz * CHUNK + iz,
                            b,
                            luz,
                            tom_da_mancha(self.ger.mancha(cx * CHUNK + ix, cz * CHUNK + iz)),
                        );
                        let p = if dx == 1 {
                            [
                                vec3(x1, y0, z0),
                                vec3(x1, y0, z1),
                                vec3(x1, y1, z1),
                                vec3(x1, y1, z0),
                            ]
                        } else if dx == -1 {
                            [
                                vec3(x0, y0, z1),
                                vec3(x0, y0, z0),
                                vec3(x0, y1, z0),
                                vec3(x0, y1, z1),
                            ]
                        } else if dz == 1 {
                            [
                                vec3(x1, y0, z1),
                                vec3(x0, y0, z1),
                                vec3(x0, y1, z1),
                                vec3(x1, y1, z1),
                            ]
                        } else {
                            [
                                vec3(x0, y0, z0),
                                vec3(x1, y0, z0),
                                vec3(x1, y1, z0),
                                vec3(x0, y1, z0),
                            ]
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

        // A base é a casca inferior das MESMAS colunas de meio metro.
        // Faces internas somem; os lotes seguem o teto e o culling do terreno.
        if self.ger.e_magica() {
            let mut bases = vec![4.0; lado * lado];
            for iz in 0..lado {
                for ix in 0..lado {
                    bases[iz * lado + ix] = base_da_ilhota(
                        cx * CHUNK + ix as i32 - 1,
                        cz * CHUNK + iz as i32 - 1,
                    );
                }
            }
            let base = |x: i32, z: i32| bases[(z + 1) as usize * lado + (x + 1) as usize];
            for iz in 0..n as i32 {
                for ix in 0..n as i32 {
                    if eh_agua(ix, iz) { continue; }
                    let bx = cx * CHUNK + ix;
                    let bz = cz * CHUNK + iz;
                    let x0 = bx as f32 * BLOCO - BLOCO * 0.5;
                    let z0 = bz as f32 * BLOCO - BLOCO * 0.5;
                    let x1 = x0 + BLOCO;
                    let z1 = z0 + BLOCO;
                    let y0 = base(ix, iz);
                    let grao = ((bx.wrapping_mul(7) ^ bz.wrapping_mul(13)) & 7) as f32 * 0.7;
                    let cor = |luz: f32| {
                        let profundidade = ((4.0 - y0) / 42.0).clamp(0.0, 1.0);
                        let c = (vec3(150.0, 158.0, 172.0) - Vec3::splat(profundidade * 25.0)
                            + Vec3::splat(grao)) * luz;
                        [c.x as u8, c.y as u8, c.z as u8, 255]
                    };
                    quad(&mut verts, &mut idx, &mut malhas,
                        [vec3(x0,y0,z0),vec3(x1,y0,z0),vec3(x1,y0,z1),vec3(x0,y0,z1)],
                        -Vec3::Y, cor(0.72));
                    for (dx,dz) in [(1,0),(-1,0),(0,1),(0,-1)] {
                        let y1 = if eh_agua(ix+dx,iz+dz) {
                            if self.ger.na_ponte_magica(bx,bz) { 7.0 } else { 4.0 }
                        } else { base(ix+dx,iz+dz) };
                        if y1 <= y0 { continue; }
                        let p = if dx != 0 {
                            let x = if dx > 0 {x1} else {x0};
                            [vec3(x,y0,z0),vec3(x,y0,z1),vec3(x,y1,z1),vec3(x,y1,z0)]
                        } else {
                            let z = if dz > 0 {z1} else {z0};
                            [vec3(x0,y0,z),vec3(x1,y0,z),vec3(x1,y1,z),vec3(x0,y1,z)]
                        };
                        quad(&mut verts, &mut idx, &mut malhas,p,
                            vec3(dx as f32,0.0,dz as f32),cor(if dx!=0 {0.92} else {0.82}));
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
                        self.bioma,
                        bx,
                        bz,
                        topo,
                        declive,
                        &self.ger,
                        eh_agua(ix, iz),
                    ) else {
                        continue;
                    };
                    // Tronco esgotado some como a pedra: sem isto o jogador via a
                    // arvore de pe' e nao tinha como saber que ali ja' nao rende.
                    let chave = shared::terreno::chave_de_coluna(
                        bx + self.ger.raio_blocos,
                        bz + self.ger.raio_blocos,
                    );
                    if self.esgotadas.contains(&chave) {
                        continue;
                    }
                    crate::vegetacao::instancia(
                        self.modelo_de_arvore(a.especie, a.variante),
                        vec3(a.centro.x, (topo + 1) as f32 * BLOCO, a.centro.y),
                        a.porte,
                        &mut verts,
                        &mut idx,
                        &mut malhas,
                        MAX_QUADS,
                    );
                    // A sombra acompanha a copa, mas nao recalcula voxels no quadro.
                    // Tres lobos quebram a silhueta perfeitamente oval.
                    for (dx, dz, rx, rz, alfa) in [
                        (1.45, -0.85, 2.25, 1.20, 67),
                        (2.65, -1.48, 1.75, 1.08, 56),
                        (0.18, -0.10, 0.78, 0.64, 46),
                    ] {
                        sombra_copa(
                            &mut sombras,
                            &mut sombra_verts,
                            &mut sombra_idx,
                            vec2(a.centro.x + dx * a.porte, a.centro.y + dz * a.porte),
                            vec2(rx * a.porte, rz * a.porte),
                            alfa,
                            (topo + 1) as f32 * BLOCO,
                            &|x, z| self.altura(x, z),
                        );
                    }
                }
            }

            // ── minerio ──────────────────────────────────────────────────────
            // Vem ANTES da forracao pra casar com `estorvos_da_coluna`, que
            // descarta a forracao da coluna onde ha' pedra.
            for iz in 0..n as i32 {
                for ix in 0..n as i32 {
                    let (bx, bz) = (cx * CHUNK + ix, cz * CHUNK + iz);
                    let topo = em(ix, iz);
                    let chave = shared::terreno::chave_de_coluna(
                        bx + self.ger.raio_blocos,
                        bz + self.ger.raio_blocos,
                    );
                    if self.esgotadas.contains(&chave) {
                        continue;
                    }
                    let agua = eh_agua(ix, iz);
                    let recursos = [
                        shared::terreno::minerio_da_coluna(
                            self.bioma, bx, bz, topo, &self.ger, agua,
                        ),
                        shared::terreno::energia_da_coluna(
                            self.bioma, bx, bz, topo, &self.ger, agua,
                        ),
                    ];
                    for m in recursos.into_iter().flatten() {
                        let base = vec3(m.centro.x, (topo + 1) as f32 * BLOCO, m.centro.y);
                        // Energia nao vai pra malha: ela se MEXE.
                        if m.energia {
                            energias.push(base);
                            continue;
                        }
                        crate::vegetacao::instancia(
                            self.modelo_de_minerio(m.tier, m.variante),
                            base,
                            m.porte,
                            &mut verts,
                            &mut idx,
                            &mut malhas,
                            MAX_QUADS,
                        );
                    }
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
                        self.bioma,
                        bx,
                        bz,
                        topo,
                        declive,
                        &self.ger,
                        eh_agua(ix, iz),
                    ) else {
                        continue;
                    };
                    // Graphics thins only what the body walks through: a
                    // plant the server treats as an obstacle stays, or it
                    // would become an invisible wall.
                    if shared::terreno::raio_de_planta(pl.especie).is_none()
                        && !planta_fica(bx, bz, forracao)
                    {
                        continue;
                    }
                    // Onde ha' minerio a forracao nao entra — a mesma regra de
                    // `estorvos_da_coluna`. Matacao de cenario colado na pedra
                    // esconde justamente o que o jogador precisa enxergar.
                    if shared::terreno::minerio_da_coluna(
                        self.bioma,
                        bx,
                        bz,
                        topo,
                        &self.ger,
                        eh_agua(ix, iz),
                    )
                    .is_some()
                        || shared::terreno::energia_da_coluna(
                            self.bioma,
                            bx,
                            bz,
                            topo,
                            &self.ger,
                            eh_agua(ix, iz),
                        )
                        .is_some()
                    {
                        continue;
                    }
                    #[cfg(test)]
                    CENSO.with(|c| {
                        *c.borrow_mut()
                            .entry(format!("{:?}", pl.especie))
                            .or_insert(0) += 1
                    });
                    crate::vegetacao::instancia(
                        self.modelo_de_planta(pl.especie, pl.variante),
                        vec3(pl.centro.x, (topo + 1) as f32 * BLOCO, pl.centro.y),
                        pl.porte,
                        &mut verts,
                        &mut idx,
                        &mut malhas,
                        MAX_QUADS,
                    );
                }
            }
        }

        if !verts.is_empty() {
            malhas.push(Mesh {
                vertices: verts,
                indices: idx,
                texture: None,
            });
        }
        if !sombra_idx.is_empty() {
            sombras.push(Mesh {
                vertices: sombra_verts,
                indices: sombra_idx,
                texture: None,
            });
        }
        Pedaco {
            malhas: malhas.into_iter().map(MalhaEstatica::nova).collect(),
            sombras: sombras.into_iter().map(MalhaEstatica::nova).collect(),
            agua: (if self.sem_mar { Vec::new() } else { crate::agua::malhas_do_pedaco(&self.ger, cx, cz) })
                .into_iter()
                .map(MalhaEstatica::nova)
                .collect(),
            energias,
            forracao,
        }
    }

    /// Cor de uma face, ja' com a luz do lado aplicada.
    ///
    /// A variacao vem da POSICAO do quad e nao da coluna: por coluna, cada
    /// vizinha teria cor propria e o merge guloso deixaria de juntar — o
    /// pedaco quadruplicaria pra ganhar um chiado que ninguem ve' de cima.
    fn cor(&self, mat: Material, bx: i32, bz: i32, by: i32, luz: f32, tom: f32) -> [u8; 4] {
        if self.ger.e_magica() {
            return cor_das_ilhas_magicas(bx, bz, by, luz);
        }
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

/// Fundo voxel alinhado ao terreno: blocos de 0,5 u, sem faces inclinadas.
fn base_da_ilhota(bx: i32, bz: i32) -> f32 {
    let p = ::glam::Vec2::new(bx as f32 * BLOCO, bz as f32 * BLOCO);
    let Some(i) = shared::magica::ilhota_em(p) else {
        return if shared::magica::na_ponte(p) { 7.0 } else { 4.0 };
    };
    let d = p - i.centro;
    let r = shared::magica::raio_da_ilhota(&i, d.y.atan2(d.x));
    let dentro = (1.0 - d.length() / r).clamp(0.0, 1.0);
    let nervuras = (p.x * 0.17).sin() * (p.y * 0.13).cos() * 3.0;
    let fundo = 4.0 - 40.0 * dentro.powf(0.7) + nervuras * dentro;
    // Patamares de dois blocos dão leitura de rocha sem alterar a grade.
    fundo.floor()
}

/// Paleta apenas visual: preserva alturas, colisões e todos os recursos.
fn cor_das_ilhas_magicas(bx: i32, bz: i32, by: i32, luz: f32) -> [u8; 4] {
    use shared::magica::{self, Bonus};
    static PONTES: std::sync::OnceLock<Vec<(::glam::Vec2, ::glam::Vec2)>> =
        std::sync::OnceLock::new();
    let p = ::glam::Vec2::new(bx as f32 * BLOCO, bz as f32 * BLOCO);
    let ilha = magica::ilhota_em(p);
    let dist_caminho = PONTES
        .get_or_init(magica::pontes)
        .iter()
        .map(|(a, b)| {
            let ab = *b - *a;
            let t = ((p - *a).dot(ab) / ab.length_squared()).clamp(0.0, 1.0);
            p.distance(*a + ab * t)
        })
        .fold(f32::INFINITY, f32::min);
    let altura = (by + 1) as f32 * BLOCO;
    let mut cor = vec3(164.0, 177.0, 181.0);
    if let Some(i) = ilha {
        let d = p.distance(i.centro);
        let acento = match i.bonus {
            Bonus::Xp => vec3(139.0, 166.0, 195.0),
            Bonus::DropDeMob => vec3(175.0, 151.0, 181.0),
            Bonus::Ouro => vec3(198.0, 178.0, 119.0),
            Bonus::DropDeChefe => vec3(158.0, 151.0, 190.0),
            Bonus::Coleta(0) => vec3(126.0, 170.0, 132.0),
            Bonus::Coleta(5) => vec3(119.0, 186.0, 183.0),
            Bonus::Coleta(_) => vec3(159.0, 164.0, 188.0),
        };
        // Campos contínuos substituem manchas de bioma e ruído por voxel.
        let onda = (p.x * 0.055).sin() * (p.y * 0.043).cos();
        cor = vec3(112.0, 157.0, 120.0) + Vec3::splat(onda * 4.0);
        if altura < magica::ALTURA + 1.5 {
            cor = vec3(207.0, 202.0, 175.0);
        }
        // Praça central e caminhos seguem as conexões reais, sem novo obstáculo.
        if d < 9.0 || dist_caminho < 1.8 {
            cor = vec3(211.0, 215.0, 204.0);
        }
        if (d - 9.0).abs() < 0.65 || (1.8..2.35).contains(&dist_caminho) {
            cor = acento;
        }
        if altura < magica::ALTURA && luz < 0.99 {
            cor = vec3(159.0, 173.0, 165.0);
        }
    } else if luz > 0.99 {
        cor = if dist_caminho > 2.3 {
            vec3(155.0, 176.0, 178.0)
        } else {
            vec3(220.0, 219.0, 203.0)
        };
    }
    let grao = ((bx.wrapping_mul(7) ^ bz.wrapping_mul(13) ^ by.wrapping_mul(5)) & 7) as f32;
    // Luz difusa nas encostas evita listras escuras em cada degrau.
    let luz = if altura >= magica::ALTURA {
        0.90 + luz * 0.10
    } else {
        0.65 + luz * 0.35
    };
    cor = cor * luz + Vec3::splat((grao - 3.5) * 0.35);
    [
        cor.x.clamp(0.0, 255.0) as u8,
        cor.y.clamp(0.0, 255.0) as u8,
        cor.z.clamp(0.0, 255.0) as u8,
        255,
    ]
}

#[cfg(debug_assertions)]
pub async fn previa_das_ilhas_magicas() {
    let saida =
        std::env::var("MMO_PREVIA_SAIDA").unwrap_or_else(|_| "/tmp/tempest-ilhas-magicas".into());
    std::fs::create_dir_all(&saida).unwrap();
    next_frame().await;
    let rt = render_target_ex(
        1280,
        800,
        RenderTargetParams {
            depth: true,
            sample_count: 1,
        },
    );
    crate::render3d::define_alvo(Some(rt.clone()));
    let mut t = Terreno::novo(&shared::magica::def_do_nivel(&shared::magica::NIVEIS[0]));
    let solido = crate::render3d::material_solido();
    for (nome, centro, distancia) in [
        ("centro", Vec2::ZERO, 65.0),
        ("pontes", vec2(70.0, 0.0), 95.0),
        ("aerea", vec2(30.0, 0.0), 165.0),
        (
            "jardim",
            vec2(
                shared::magica::ilhotas()[1].centro.x,
                shared::magica::ilhotas()[1].centro.y,
            ),
            65.0,
        ),
    ] {
        t.atualiza(centro, 16, 2000);
        for _ in 0..3 {
            let cam = Camera3D {
                position: vec3(
                    centro.x + distancia * 0.45,
                    distancia * 0.48,
                    centro.y + distancia,
                ),
                target: vec3(centro.x, 10.0, centro.y),
                up: Vec3::Y,
                render_target: Some(rt.clone()),
                aspect: Some(1.6),
                ..Default::default()
            };
            set_camera(&cam);
            clear_background(Color::from_rgba(150, 186, 214, 255));
            macroquad::material::gl_use_material(&solido);
            t.desenha(&cam, Vec3::ZERO, 0.0);
            t.desenha_sombras(&cam);
            crate::agua::desenha(&t, &cam, 0.0);
            macroquad::material::gl_use_default_material();
            unsafe { get_internal_gl().flush() };
            rt.texture
                .get_texture_data()
                .export_png(&format!("{saida}/{nome}.png"));
            next_frame().await;
        }
    }
}

/// MMO_PREVIA_SOMBRAS: Pretty tree shadows from the island's peaks, through
/// the game camera. Each shot is drawn twice (terrain alone, then terrain plus
/// shadows) so the difference shows exactly where shadows landed. The render
/// target has a 16-bit depth buffer, so `perto` 2.56 stands in for a desktop
/// screen's 24 bits at the game's 0.01 near plane; 0.01 is a 16-bit phone.
#[cfg(debug_assertions)]
pub async fn previa_das_sombras() {
    let saida = std::env::var("MMO_PREVIA_SAIDA").unwrap_or_else(|_| "/tmp/tempest-sombras".into());
    std::fs::create_dir_all(&saida).unwrap();
    next_frame().await;
    let rt = render_target_ex(1280, 800, RenderTargetParams { depth: true, sample_count: 1 });
    crate::render3d::define_alvo(Some(rt.clone()));
    let solido = crate::render3d::material_solido();
    let def = &shared::terreno::ARQUIPELAGO[0];
    let mut t = Terreno::novo(def);
    // The highest columns, at least 40 u apart.
    let mut picos: Vec<(f32, Vec2)> = Vec::new();
    let mut todos: Vec<(f32, Vec2)> = Vec::new();
    for z in (-150..150).step_by(3) {
        for x in (-150..150).step_by(3) {
            let p = vec2(x as f32, z as f32);
            todos.push((t.altura(p.x, p.y), p));
        }
    }
    todos.sort_by(|a, b| b.0.total_cmp(&a.0));
    for (h, p) in todos {
        if picos.len() < 3 && picos.iter().all(|(_, q)| q.distance(p) > 40.0) {
            picos.push((h, p));
        }
    }
    for (k, (h, centro)) in picos.iter().enumerate() {
        t.atualiza(*centro, 5, 2000);
        for (j, yaw) in [0.0f32, 1.6, 3.2, 4.7].into_iter().enumerate() {
            for perto in [2.56f32, 0.01] {
                let pitch = crate::render3d::pitch_padrao();
                let mut cam = crate::render3d::camera(*centro, *h, yaw, 1.0, pitch);
                cam.render_target = Some(rt.clone());
                cam.aspect = Some(1.6);
                cam.z_near = perto;
                for com_sombra in [false, true] {
                    for _ in 0..2 {
                        set_camera(&cam);
                        clear_background(Color::from_rgba(150, 186, 214, 255));
                        macroquad::material::gl_use_material(&solido);
                        t.desenha(&cam, Vec3::ZERO, 0.0);
                        if com_sombra {
                            t.desenha_sombras(&cam);
                        }
                        macroquad::material::gl_use_default_material();
                        unsafe { get_internal_gl().flush() };
                        rt.texture.get_texture_data().export_png(&format!(
                            "{saida}/pico{k}-yaw{j}-near{perto}-{}.png",
                            if com_sombra { "com" } else { "sem" }
                        ));
                        next_frame().await;
                    }
                }
            }
        }
        println!("[shadow preview] peak {k} at ({:.0},{:.0}) height {h:.1}", centro.x, centro.y);
    }
}

/// MMO_PREVIA_GRAFICOS: the starting island through the GAME camera, zoomed
/// out at the lowest tilt each view distance allows, and at each grass density.
#[cfg(debug_assertions)]
pub async fn previa_dos_graficos() {
    let saida = std::env::var("MMO_PREVIA_SAIDA").unwrap_or_else(|_| "/tmp/tempest-graficos".into());
    std::fs::create_dir_all(&saida).unwrap();
    next_frame().await;
    let rt = render_target_ex(1280, 800, RenderTargetParams { depth: true, sample_count: 1 });
    crate::render3d::define_alvo(Some(rt.clone()));
    let solido = crate::render3d::material_solido();
    let def = &shared::terreno::ARQUIPELAGO[0];
    let centro = vec2(20.0, 30.0);
    for (nome, raio, forracao, zoom) in [
        ("perto", 3, 100, crate::render3d::ZOOM_MAX),
        ("normal", 5, 100, crate::render3d::ZOOM_MAX),
        ("muito-longe", 9, 100, crate::render3d::ZOOM_MAX),
        ("extremo", 12, 100, crate::render3d::ZOOM_MAX),
        ("grama-cheia", 5, 100, crate::render3d::ZOOM_MIN),
        ("grama-pouca", 5, 30, crate::render3d::ZOOM_MIN),
    ] {
        crate::config_graficos::define_para_previa(raio, forracao);
        // A fresh terrain per shot: every chunk baked at this density.
        let mut t = Terreno::novo(def);
        t.atualiza(centro, raio, 2000);
        let chao = t.altura(centro.x, centro.y);
        let pitch = crate::render3d::pitch_min_para(zoom);
        let mut cam = crate::render3d::camera(centro, chao, 0.6, zoom, pitch);
        cam.render_target = Some(rt.clone());
        cam.aspect = Some(1.6);
        let (inicio, fim) = crate::config_graficos::neblina();
        crate::gpu_estatica::define_neblina(centro, inicio, fim);
        for _ in 0..3 {
            set_camera(&cam);
            clear_background(Color::from_rgba(150, 186, 214, 255));
            macroquad::material::gl_use_material(&solido);
            let n = t.desenha(&cam, Vec3::ZERO, 0.0);
            crate::agua::desenha(&t, &cam, 0.0);
            macroquad::material::gl_use_default_material();
            unsafe { get_internal_gl().flush() };
            rt.texture.get_texture_data().export_png(&format!("{saida}/{nome}.png"));
            next_frame().await;
            println!("[graphics preview] {nome}: radius {raio}, cover {forracao}%, pitch {:.1}°, {n} chunks drawn", pitch.to_degrees());
        }
    }
}

/// MMO_PREVIA_CELESTE: Skyreach (`shared::celeste`), one view per plateau,
/// two wide views across the bridges and the minimap, in
/// /tmp/tempest-celeste (or MMO_PREVIA_SAIDA).
#[cfg(debug_assertions)]
pub async fn previa_celeste(vox: &mut crate::vox::VoxCache) {
    let saida = std::env::var("MMO_PREVIA_SAIDA").unwrap_or_else(|_| "/tmp/tempest-celeste".into());
    std::fs::create_dir_all(&saida).unwrap();
    next_frame().await;
    let rt = render_target_ex(1280, 800, RenderTargetParams { depth: true, sample_count: 1 });
    crate::render3d::define_alvo(Some(rt.clone()));
    let def = &shared::celeste::DEF;
    let g = shared::terreno::Gerador::da_ilha(def);
    let mut t = Terreno::novo(def);
    let mut casas = crate::construcoes::Construcoes::para(Some(def));
    for _ in 0..600 { casas.acompanhar(); if casas.prontas() { break; } next_frame().await; }
    let solido = crate::render3d::material_solido();
    let p = &shared::celeste::PLATOS;
    // Shared's glam is another version than the client's: carry plain floats.
    let meio = |i: usize, j: usize| ((p[i].centro.x + p[j].centro.x) * 0.5, (p[i].centro.y + p[j].centro.y) * 0.5);
    let mut vistas: Vec<(String, (f32, f32), f32)> = [0usize, 2, 5, 8, 11]
        .iter()
        .map(|&i| (format!("plato-{i}"), (p[i].centro.x, p[i].centro.y), 140.0))
        .collect();
    vistas.push(("ponte-0-1".into(), meio(0, 1), 150.0));
    vistas.push(("ponte-5-7".into(), meio(5, 7), 170.0));
    vistas.push(("aerea".into(), (90.0, 70.0), 640.0));
    // Where the path from island 0 lands on island 1, seen from the side and
    // a little below: the cliff must close under the deck (the owner's
    // "pale beige wedge").
    vistas.push(("pouso-0-1".into(), meio(0, 1), -1.0));
    for (nome, (cx, cy), distancia) in vistas {
        let c = vec2(cx, cy);
        let centro = c;
        let chao = g.altura(c.x, c.y).max(12.0);
        t.atualiza(centro, if distancia > 400.0 { 40 } else { 16 }, 4000);
        let (pos, alvo) = if distancia < 0.0 {
            let (a, b) = (vec2(p[0].centro.x, p[0].centro.y), vec2(p[1].centro.x, p[1].centro.y));
            // The landing: the first column of path, scanning a ring round
            // island 1 just past its radius.
            let mut borda = b + (a - b).normalize() * p[1].raio;
            'achar: for k in 0..720 {
                let ang = k as f32 / 720.0 * std::f32::consts::TAU;
                for rr in [1.02f32, 1.06, 1.10] {
                    let q = b + vec2(ang.cos(), ang.sin()) * p[1].raio * rr;
                    let (bx, bz) = ((q.x / shared::terreno::BLOCO).round() as i32, (q.y / shared::terreno::BLOCO).round() as i32);
                    if g.na_ponte_magica(bx, bz) {
                        borda = q;
                        break 'achar;
                    }
                }
            }
            let dir = (borda - b).normalize();
            let h = g.altura(borda.x, borda.y);
            let lado = vec2(-dir.y, dir.x);
            t.atualiza(borda, 16, 4000);
            (vec3(borda.x + lado.x * 22.0 + dir.x * 14.0, h - 7.0, borda.y + lado.y * 22.0 + dir.y * 14.0), vec3(borda.x - dir.x * 2.0, h - 4.0, borda.y - dir.y * 2.0))
        } else {
            (vec3(centro.x + distancia * 0.45, chao + distancia * 0.65, centro.y + distancia), vec3(centro.x, chao, centro.y))
        };
        for _ in 0..3 {
            let cam = Camera3D {
                position: pos,
                target: alvo,
                up: Vec3::Y,
                render_target: Some(rt.clone()),
                aspect: Some(1.6),
                ..Default::default()
            };
            set_camera(&cam);
            clear_background(Color::from_rgba(150, 186, 214, 255));
            macroquad::material::gl_use_material(&solido);
            t.desenha(&cam, Vec3::ZERO, 0.0);
            t.desenha_sombras(&cam);
            casas.desenha(&cam, None, Vec3::ZERO, 0.0);
            crate::agua::desenha(&t, &cam, 0.0);
            macroquad::material::gl_use_default_material();
            unsafe { get_internal_gl().flush() };
            rt.texture.get_texture_data().export_png(&format!("{saida}/{nome}.png"));
            next_frame().await;
        }
    }
    let mut mapa = crate::mapa::Mapa::para(Some(def));
    mapa.abrir();
    let mundo = crate::world::World::default();
    for _ in 0..120 {
        mapa.acompanhar();
        crate::render3d::camera_padrao();
        clear_background(Color::from_rgba(18, 24, 34, 255));
        mapa.desenha_grande(&mundo, 55, &crate::mundo_ui::Mundo::default(), 0);
        unsafe { get_internal_gl().flush() };
        next_frame().await;
    }
    rt.texture.get_texture_data().export_png(&format!("{saida}/mapa.png"));
    // The World tab: the fifth island on the archipelago map.
    mapa.no_mundo = true;
    for _ in 0..10 {
        mapa.acompanhar();
        crate::render3d::camera_padrao();
        clear_background(Color::from_rgba(18, 24, 34, 255));
        mapa.desenha_grande(&mundo, 55, &crate::mundo_ui::Mundo::default(), 0);
        unsafe { get_internal_gl().flush() };
        next_frame().await;
    }
    rt.texture.get_texture_data().export_png(&format!("{saida}/mundo.png"));
    // The winged creatures, side by side, two angles each.
    let bichos = ["bichos/seraph_wolf", "bichos/seraph_lynx", "bichos/seraph_bear", "bichos/seraph_owlbear",
        "bichos/pegasus_stag", "humanoides/seraph_archer", "humanoides/seraph_mage"];
    for _ in 0..8 {
        crate::render3d::camera_padrao();
        clear_background(Color::from_rgba(150, 186, 214, 255));
        let w = 1280.0 / bichos.len() as f32;
        for (k, nome) in bichos.iter().enumerate() {
            for (j, yaw) in [0.7f32, 2.6].into_iter().enumerate() {
                let r = Rect::new(k as f32 * w, 40.0 + j as f32 * 380.0, w, 360.0);
                if nome.starts_with("humanoides/") {
                    crate::render3d::vitrine_rig(vox, nome, r, yaw * 0.6, &solido);
                } else {
                    crate::render3d::vitrine_bicho(vox, nome, r, yaw, &solido);
                }
            }
            crate::render3d::camera_padrao();
            draw_text(nome.trim_start_matches("bichos/"), k as f32 * w + 10.0, 28.0, 24.0, WHITE);
        }
        unsafe { get_internal_gl().flush() };
        next_frame().await;
        vox.atende_um_pendente(crate::render3d::VOXEL).await;
    }
    rt.texture.get_texture_data().export_png(&format!("{saida}/bichos.png"));
    crate::render3d::define_alvo(None);
}

#[cfg(debug_assertions)]
pub async fn previa_do_planalto() {
    let saida =
        std::env::var("MMO_PREVIA_SAIDA").unwrap_or_else(|_| "/tmp/tempest-planalto".into());
    std::fs::create_dir_all(&saida).unwrap();
    next_frame().await;
    let rt = render_target_ex(
        1280,
        800,
        RenderTargetParams {
            depth: true,
            sample_count: 1,
        },
    );
    crate::render3d::define_alvo(Some(rt.clone()));
    let def = &shared::terreno::ARQUIPELAGO[3];
    let g = shared::terreno::Gerador::da_ilha(def);
    let pl = g.planalto().unwrap();
    let mut t = Terreno::novo(def);
    let mut casas = crate::construcoes::Construcoes::para(Some(def));
    for _ in 0..600 { casas.acompanhar(); if casas.prontas() { break; } next_frame().await; }
    let solido = crate::render3d::material_solido();
    let vistas = std::iter::once(("abrigo".to_string(),g.cidade().unwrap().centro()))
        .chain(std::iter::once(("forte".to_string(),pl.forte.centro)))
        .chain(pl.regioes.iter().enumerate().map(|(i,r)|(format!("regiao-{i}"),r.centro)));
    for (nome,c) in vistas {
        let centro = vec2(c.x,c.y);
        let distancia = 150.0;
        let chao = g.altura(c.x,c.y);
        t.atualiza(centro, 16, 2000);
        for _ in 0..3 {
            let cam = Camera3D {
                position: vec3(
                    centro.x + distancia * 0.45,
                    chao + distancia * 0.65,
                    centro.y + distancia,
                ),
                target: vec3(centro.x, chao, centro.y),
                up: Vec3::Y,
                render_target: Some(rt.clone()),
                aspect: Some(1.6),
                ..Default::default()
            };
            set_camera(&cam);
            clear_background(Color::from_rgba(150, 186, 214, 255));
            macroquad::material::gl_use_material(&solido);
            t.desenha(&cam, Vec3::ZERO, 0.0);
            t.desenha_sombras(&cam);
            casas.desenha(&cam,None,Vec3::ZERO,0.0);
            crate::agua::desenha(&t, &cam, 0.0);
            macroquad::material::gl_use_default_material();
            unsafe { get_internal_gl().flush() };
            rt.texture
                .get_texture_data()
                .export_png(&format!("{saida}/{nome}.png"));
            next_frame().await;
        }
    }
    // O MAPA VAI PRO MESMO RENDER TARGET, e nao pra tela.
    //
    // Antes isto fazia `define_alvo(None)` + `set_default_camera()` e capturava
    // com `get_screen_data()` — e o cliente morria em SIGSEGV no fim da previa,
    // depois de ja' ter gravado as seis vistas 3D. Sob llvmpipe o framebuffer
    // padrao nao se le' de volta (o mesmo motivo que deixa a captura em branco);
    // `camera_padrao` respeita o alvo, entao o 2D do mapa cai na textura e sai
    // por `get_texture_data`, como as outras.
    let mut mapa = crate::mapa::Mapa::para(Some(def));
    mapa.abrir();
    let mundo = crate::world::World::default();
    for _ in 0..120 {
        mapa.acompanhar();
        crate::render3d::camera_padrao();
        clear_background(Color::from_rgba(18, 24, 34, 255));
        mapa.desenha_grande(&mundo, 40, &crate::mundo_ui::Mundo::default(), 0);
        unsafe { get_internal_gl().flush() };
        next_frame().await;
    }
    crate::render3d::camera_padrao();
    clear_background(Color::from_rgba(18, 24, 34, 255));
    mapa.desenha_grande(&mundo, 40, &crate::mundo_ui::Mundo::default(), 0);
    unsafe { get_internal_gl().flush() };
    rt.texture
        .get_texture_data()
        .export_png(&format!("{saida}/mapa.png"));
    crate::render3d::define_alvo(None);

}

/// Maior desnivel entre a coluna e os quatro vizinhos.
fn sombra_copa(
    malhas: &mut Vec<Mesh>,
    verts: &mut Vec<Vertex>,
    idx: &mut Vec<u16>,
    centro: Vec2,
    raio: Vec2,
    alfa: u8,
    origem_y: f32,
    altura: &dyn Fn(f32, f32) -> f32,
) {
    const LADOS: usize = 12;
    if (altura(centro.x, centro.y) - origem_y).abs() > 0.6 {
        return;
    }
    if verts.len() + LADOS + 1 > 3000 {
        malhas.push(Mesh {
            vertices: std::mem::take(verts),
            indices: std::mem::take(idx),
            texture: None,
        });
    }
    let base = verts.len() as u16;
    let nivel = altura(centro.x, centro.y);
    let mut alturas = [0.0; LADOS];
    let mut ponto = |x: f32, z: f32, a: u8| {
        verts.push(Vertex {
            position: vec3(x, altura(x, z) + 0.045, z),
            uv: Vec2::ZERO,
            color: [15, 20, 29, a],
            normal: Vec4::ZERO,
        });
    };
    ponto(centro.x, centro.y, alfa);
    for i in 0..LADOS {
        let a = i as f32 * std::f32::consts::TAU / LADOS as f32;
        let x = centro.x + a.cos() * raio.x;
        let z = centro.y + a.sin() * raio.y;
        alturas[i] = altura(x, z);
        ponto(x, z, 0);
    }
    for i in 0..LADOS {
        let proximo = (i + 1) % LADOS;
        if (alturas[i] - nivel).abs() <= 0.6 && (alturas[proximo] - nivel).abs() <= 0.6 {
            idx.extend_from_slice(&[base, base + 1 + proximo as u16, base + 1 + i as u16]);
        }
    }
}

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
            println!(
                "  {k:<12} {v:>5}  ({:.0}%)",
                **v as f32 * 100.0 / total.max(1) as f32
            );
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
                if let Some(a) =
                    shared::terreno::arvore_da_coluna(d.bioma, bx, bz, topo, declive, &t.ger, agua)
                {
                    solidos += 1;
                    assert!(
                        !i.sem_estorvo(a.centro, 0.01),
                        "arvore desenhada em {:?} nao barra no servidor",
                        a.centro
                    );
                }
                let Some(pl) =
                    shared::terreno::planta_da_coluna(d.bioma, bx, bz, topo, declive, &t.ger, agua)
                else {
                    continue;
                };
                match shared::terreno::raio_de_planta(pl.especie) {
                    // Matacao e toco barram.
                    Some(_) => {
                        solidos += 1;
                        assert!(
                            !i.sem_estorvo(pl.centro, 0.01),
                            "{:?} desenhada em {:?} nao barra no servidor",
                            pl.especie,
                            pl.centro
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
        assert!(
            solidos > 20,
            "so' {solidos} estorvos na amostra — teste vazio"
        );
        assert!(vazados > 100, "so' {vazados} plantas vazadas na amostra");
    }

    /// O mesmo contrato, pela PEDRA: toda pedra que o servidor barra (e da'
    /// coleta) tem que ser uma pedra que o cliente desenha — no mesmo lugar,
    /// da mesma cor.
    ///
    /// Olha a ilha INTEIRA pelo lado do servidor, e nao uma janela: a pedra
    /// mora nos cumes, longe do centro, e uma amostra perto do porto passaria
    /// sem conferir nenhuma. Importa mais agora que a regra da pedra consulta
    /// as colunas VIZINHAS (chao limpo, cume): se um lado lesse a altura por
    /// outro caminho, a divergencia apareceria so' na beira do patamar.
    #[test]
    fn toda_pedra_e_energia_do_servidor_e_desenhada_no_cliente() {
        use shared::terreno::{Ilha, TipoDeEstorvo, ESCALA_ALTURA, NIVEL_DO_MAR};
        let d = &ARQUIPELAGO[0];
        let t = Terreno::novo(d);
        let i = Ilha::carregar_ou_gerar("", d.semente, d.raio_blocos, d.bioma, ESCALA_ALTURA);
        let mut n = 0;
        let mut energias = 0;
        for e in i.todos_os_estorvos() {
            let tier = match e.tipo {
                TipoDeEstorvo::Minerio(tier) => tier,
                TipoDeEstorvo::Energia => 5,
                _ => continue,
            };
            let (ix, iz) = ((e.coluna >> 16) as i32, (e.coluna & 0xffff) as i32);
            let (bx, bz) = (ix - d.raio_blocos, iz - d.raio_blocos);
            let topo = t.ger.bloco_em(bx, bz);
            let agua = (topo + 1) as f32 * BLOCO <= NIVEL_DO_MAR;
            let m = if tier == 5 {
                shared::terreno::energia_da_coluna(d.bioma, bx, bz, topo, &t.ger, agua)
            } else {
                shared::terreno::minerio_da_coluna(d.bioma, bx, bz, topo, &t.ger, agua)
            }
            .unwrap_or_else(|| {
                panic!(
                    "servidor tem recurso em {:?} que o cliente nao desenha",
                    e.centro
                )
            });
            if tier == 5 {
                assert!(
                    m.energia,
                    "cristal em {:?} virou pedra no cliente",
                    e.centro
                );
                energias += 1;
            } else {
                assert!(
                    !m.energia,
                    "pedra em {:?} virou cristal no cliente",
                    e.centro
                );
                assert_eq!(
                    m.tier, tier,
                    "pedra em {:?}: cor diferente nos dois lados",
                    e.centro
                );
            }
            assert!(
                m.centro.distance(e.centro) < 1e-4,
                "pedra em lugares diferentes"
            );
            n += 1;
        }
        println!("{n} recursos conferidos dos dois lados — {energias} veios de Energia");
        // O veio nasce em CAMPO, e nao solto: confirmado contando quantas
        // celulas de 40 unidades tem veio. Espalhados, seriam quase uma
        // celula por veio; em campo, varios dividem a mesma.
        let mut celulas: std::collections::HashSet<(i32, i32)> = Default::default();
        for e in i.todos_os_estorvos() {
            if matches!(e.tipo, TipoDeEstorvo::Energia) {
                celulas.insert(((e.centro.x / 40.0) as i32, (e.centro.y / 40.0) as i32));
            }
        }
        println!("{} celulas de 40u com veio", celulas.len());
        assert!(
            energias as f32 / celulas.len() as f32 >= 2.5,
            "os veios estao espalhados, nao em campo: {energias} veios em {} celulas",
            celulas.len()
        );
        assert!(n > 50, "so' {n} pedras na ilha — teste vazio");
        assert!(
            energias > 40,
            "so' {energias} veios na ilha — Energia tem que ter LUGAR de coleta"
        );
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
                for m in p.malhas.iter().map(|m| m.cpu().unwrap()) {
                    pior_i = pior_i.max(m.indices.len());
                    pior_v = pior_v.max(m.vertices.len());
                }
            }
            println!(
                "{:<14} pior malha: {pior_v} vertices, {pior_i} indices ({total} malhas)",
                d.zona
            );
            assert!(
                pior_i <= 5_000,
                "{}: {pior_i} indices — a macroquad corta em 5.000",
                d.zona
            );
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
            for malha in p.malhas.iter().map(|m| m.cpu().unwrap()) {
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
            for malha in p.malhas.iter().map(|m| m.cpu().unwrap()) {
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
                        c.y,
                        c.x,
                        c.z,
                        consultada
                    );
                    conferidos += 1;
                }
            }
        }
        assert!(conferidos > 1000, "so' {conferidos} topos conferidos");
        println!("{conferidos} topos conferidos: desenho e consulta batem");
    }
}

#[cfg(test)]
mod testes_da_colonia {
    use super::*;

    /// A COLONIA que o cliente desenha e' a MESMA que o servidor gerou.
    ///
    /// O dono chegou na propria ilha e viu "tudo azul vazio": nada de chao.
    /// Azul e' o que sobra quando `terreno` e `map` sao os dois `None` — ou
    /// quando o relevo existe mas nasce todo debaixo d'agua. Este teste
    /// separa os dois casos, comparando o relevo do CLIENTE
    /// (`Terreno::da_colonia`, um `Gerador`) com o do SERVIDOR
    /// (`terreno::Ilha`, um campo de altura assado).
    #[test]
    fn o_chao_da_colonia_bate_com_o_do_servidor() {
        for nivel in 1..=shared::colonia::NIVEL_MAX {
            let nome = shared::colonia::Assentamento::do_nivel(nivel).nome();
            let plato = shared::colonia::plato_do_assentamento(nivel);
            let t = Terreno::da_colonia(plato);
            let ilha = shared::terreno::Ilha::da_colonia(plato);
            let chegada = ilha.terra_mais_proxima(
                shared::colonia::CHEGADA.x,
                shared::colonia::CHEGADA.y,
                400.0,
            );
            // 1. O cliente ve' chao onde o servidor pos o jogador.
            let hc = t.altura(chegada.x, chegada.y);
            let hs = ilha.altura(chegada.x, chegada.y);
            assert!(
                (hc - hs).abs() < 0.51,
                "{nome}: cliente ve' {hc:.2} onde o servidor ve' {hs:.2} em {chegada:?}"
            );
            assert!(
                hc > shared::terreno::NIVEL_DO_MAR,
                "{nome}: a chegada nasce DEBAIXO do mar (y={hc:.2}, mar={:.2})",
                shared::terreno::NIVEL_DO_MAR
            );
            // 2. E os pedacos em volta dela tem geometria de verdade.
            let mut t = t;
            t.atualiza(vec2(chegada.x, chegada.y), 2, 200);
            assert!(t.pedacos_vivos() > 0, "{nome}: nenhum pedaco gerado");
        }
    }
}

/// Prévia da COLÔNIA (`MMO_PREVIA_COLONIA=<nome do personagem>`; PNGs em
/// `MMO_PREVIA_SAIDA`).
///
/// O dono chegou na própria ilha e viu "tudo azul vazio". O relevo confere
/// com o do servidor (`testes_da_colonia`), então o que falta é ver o que o
/// cliente DESENHA — e ver sem entrar na sessão dele. Isto monta o mesmo
/// `Terreno::da_colonia` que a mensagem do servidor monta, põe a câmera onde
/// o jogador nasce e salva o quadro.
#[cfg(debug_assertions)]
pub async fn previa_da_colonia(solido: &macroquad::material::Material, vox: &crate::vox::VoxCache) {
    let nome = std::env::var("MMO_PREVIA_COLONIA").unwrap_or_else(|_| "brunji".into());
    let saida = std::env::var("MMO_PREVIA_SAIDA").unwrap_or_else(|_| "/tmp/tempest-colonia".into());
    std::fs::create_dir_all(&saida).unwrap();
    next_frame().await;
    let rt = macroquad::texture::render_target_ex(
        screen_width() as u32,
        screen_height() as u32,
        macroquad::texture::RenderTargetParams {
            depth: true,
            sample_count: 1,
        },
    );
    rt.texture.set_filter(FilterMode::Linear);
    crate::render3d::define_alvo(Some(rt.clone()));

    let nivel: u8 = std::env::var("MMO_PREVIA_NIVEL")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(1);
    let plato = shared::colonia::plato_do_assentamento(nivel);
    let mut t = Terreno::da_colonia(plato);
    // Onde o servidor põe o jogador ao chegar.
    let ilha = shared::terreno::Ilha::da_colonia(plato);
    let ch = ilha.terra_mais_proxima(
        shared::colonia::CHEGADA.x,
        shared::colonia::CHEGADA.y,
        400.0,
    );
    // A camera vai pra PRACA, e nao pra chegada: e' o plato que precisa ser
    // olhado — "sempre ter um terreno aplanado nas construcoes".
    let praca = shared::terreno::Gerador::da_colonia(plato)
        .cidade()
        .map(|c| c.centro())
        .unwrap_or_default();
    let eu = vec2(praca.x, praca.y);
    let _ = ch;
    println!(
        "[previa colonia] '{nome}' assentamento {} plato {plato:.0} chegada ({:.2}, {:.2}) \
         altura cliente {:.2} servidor {:.2} mar {:.2}",
        shared::colonia::Assentamento::do_nivel(nivel).nome(),
        eu.x,
        eu.y,
        t.altura(eu.x, eu.y),
        ilha.altura(ch.x, ch.y),
        NIVEL_DO_MAR,
    );
    t.atualiza(eu, 5, 400);
    println!("[colony preview] {} live chunks", t.pedacos_vivos());

    let apoio = t.altura(eu.x, eu.y);
    // O ASSENTAMENTO: a casa do jogador e a de cada morador do nivel.
    let moradores: Vec<shared::colonia::Profissao> = shared::colonia::Profissao::TODAS
        .into_iter()
        .cycle()
        .take(shared::colonia::vagas_de_trabalho(nivel))
        .collect();
    let mut construcoes = crate::construcoes::Construcoes::da_colonia(plato, moradores.clone());
    for _ in 0..200 {
        construcoes.acompanhar();
        if construcoes.prontas() {
            break;
        }
        next_frame().await;
    }
    println!(
        "[colony preview] {} resident(s): {}",
        moradores.len(),
        moradores
            .iter()
            .map(|m| m.nome())
            .collect::<Vec<_>>()
            .join(", ")
    );
    for (k, (zoom, pitch)) in [(14.0f32, 0.9f32), (60.0, 1.1)].into_iter().enumerate() {
        for _ in 0..2 {
            crate::render3d::camera_padrao();
            clear_background(Color::new(0.11, 0.35, 0.62, 1.0));
            let chao = |x: f32, z: f32| t.altura(x, z);
            let cam = crate::render3d::camera_com_chao(eu, apoio, 0.6, zoom, pitch, &chao);
            let mut cam = cam;
            cam.render_target = crate::render3d::alvo();
            set_camera(&cam);
            macroquad::material::gl_use_material(solido);
            let n = t.desenha(&cam, Vec3::ZERO, 0.0);
            construcoes.desenha(&cam, Some(vec3(eu.x, apoio, eu.y)), Vec3::ZERO, 0.0);
            crate::agua::desenha(&t, &cam, 0.0);
            macroquad::material::gl_use_default_material();
            crate::render3d::camera_padrao();
            unsafe { macroquad::window::get_internal_gl().flush() };
            rt.texture
                .get_texture_data()
                .export_png(&format!("{saida}/colonia-{k}.png"));
            println!("[colony preview] frame {k}: {n} chunks drawn");
            next_frame().await;
        }
    }

    // A MAQUETE DO PAINEL: o que o jogador vê de verdade desde 22/09/2026.
    //
    // Os quadros acima são a câmera do MUNDO, de quando se andava na ilha —
    // úteis pro relevo, mas não é mais isso que a tela mostra. Este é o
    // enquadramento do painel, com os moradores em pé, na proporção da
    // coluna da esquerda.
    // A MESMA função que o painel usa, e não uma cópia com outra conta.
    //
    // Aqui havia um anel improvisado (`plato * 0,55` em volta do centro) — e
    // a prévia mostrava moradores num lugar em que o jogo nunca os pôs. Já
    // me queimei exatamente assim com o raio do terreno.
    let onde =
        crate::colonia_ui::onde_ficam(&shared::terreno::Gerador::da_colonia(plato), &moradores);
    // REPRODUZ O PAINEL, com os parâmetros dele: terreno recém-criado e
    // `atualiza(centro, 3, 8)` por quadro, que é o que `ColoniaUi::desenha`
    // faz. A prévia antes usava raio 5 e orçamento 400 — e por isso mostrava
    // uma ilha inteira que o jogo nunca chegava a ter.
    // O `tp` VIVE ATÉ O FIM da prévia de propósito: as capturas seguintes têm
    // que sair do MESMO terreno que o painel monta, e não do `t` da câmera do
    // mundo. Já entreguei prévia validando terreno diferente do que o jogo usa.
    let tp = {
        let t0 = std::time::Instant::now();
        let mut tp = Terreno::da_colonia(plato);
        println!(
            "[colony preview] panel: building Terrain took {:?}",
            t0.elapsed()
        );
        let r = Rect::new(0.0, 0.0, 340.0, 270.0);
        for q in 0..40 {
            tp.atualiza(eu, 6, 24);
            let ok = crate::render3d::maquete_da_ilha(
                &tp,
                &construcoes,
                r,
                eu,
                0.9,
                crate::render3d::ELEV_PADRAO,
                1.0,
                solido,
                &[],
                vox,
            );
            if q % 8 == 0 || q == 39 {
                println!(
                    "[colony preview] panel frame {q:2}: drew={ok} chunks={}",
                    tp.pedacos_vivos()
                );
            }
            next_frame().await;
        }
        tp
    };
    // 52% da largura útil do painel sobre a altura útil dele.
    const PROP_COLUNA: f32 = 0.815;
    for m in &onde {
        let nome = crate::render3d::rig_do_npc(m.oficio.papel() as u8, 0);
        let pecas = vox.rig(nome).map(|h| h.len());
        println!(
            "[previa colonia] {} -> rig '{nome}' pecas={pecas:?} casa ({:.0},{:.0}) trabalho ({:.0},{:.0}) dist {:.0}u",
            m.oficio.nome(),
            m.casa.x, m.casa.y, m.trabalho.x, m.trabalho.y,
            (m.trabalho - m.casa).length(),
        );
    }
    // GIRO, ELEVAÇÃO E ZOOM, pro dono julgar. A elevação entrou na lista
    // quando a câmera deixou de ser presa no horizontal: as duas últimas
    // vistas são os extremos que o arrasto vertical alcança.
    const EP: f32 = crate::render3d::ELEV_PADRAO;
    for (k, (giro, elev, zoom)) in [
        (0.0f32, EP, 1.0f32),
        (1.57, EP, 1.0),
        (3.14, EP, 1.0),
        (4.71, EP, 1.0),
        (0.9, EP, 1.8),
        (0.9, crate::render3d::ELEV_MIN, 1.0),
        (0.9, crate::render3d::ELEV_MAX, 1.0),
    ]
    .into_iter()
    .enumerate()
    {
        for _ in 0..2 {
            crate::render3d::camera_padrao();
            clear_background(crate::render3d::COR_DO_FUNDO_DA_MAQUETE);
            let lado = (screen_height() * 0.88).min(screen_width() * 0.6);
            let r = Rect::new(
                screen_width() * 0.04,
                screen_height() * 0.05,
                lado * PROP_COLUNA,
                lado,
            );
            crate::render3d::maquete_da_ilha(
                &t,
                &construcoes,
                r,
                eu,
                giro,
                elev,
                zoom,
                solido,
                &onde,
                vox,
            );
            unsafe { macroquad::window::get_internal_gl().flush() };
            rt.texture
                .get_texture_data()
                .export_png(&format!("{saida}/vista-{k}.png"));
            next_frame().await;
        }
        println!("[colony preview] view {k}: spin {giro:.2} elev {elev:.2} zoom {zoom:.1}");
    }
    // QUATRO MOMENTOS DO CICLO, e não quatro quadros seguidos.
    //
    // A rotina do morador dura 26 s; dois quadros a 60 Hz cobrem 33 ms dela.
    // Sem adiantar o relógio, todo PNG mostraria os quatro bonecos exatamente
    // no mesmo ponto do caminho — e "eles não andam" é justamente o defeito
    // que eu vim consertar.
    for (k, adianta) in [0.0f32, 5.0, 11.0, 18.0].into_iter().enumerate() {
        crate::render3d::adianta_o_relogio_da_maquete(adianta);
        for _ in 0..2 {
            crate::render3d::camera_padrao();
            clear_background(crate::render3d::COR_DO_FUNDO_DA_MAQUETE);
            let lado = (screen_height() * 0.88).min(screen_width() * 0.6);
            let r = Rect::new(
                screen_width() * 0.04,
                screen_height() * 0.05,
                lado * PROP_COLUNA,
                lado,
            );
            crate::render3d::maquete_da_ilha(
                &tp,
                &construcoes,
                r,
                eu,
                0.9,
                crate::render3d::ELEV_PADRAO,
                2.2,
                solido,
                &onde,
                vox,
            );
            unsafe { macroquad::window::get_internal_gl().flush() };
            rt.texture
                .get_texture_data()
                .export_png(&format!("{saida}/rotina-{k}.png"));
            next_frame().await;
        }
        for m in &onde {
            let p = crate::render3d::rotina_do_morador(m, 0, adianta);
            println!(
                "[colony preview] t={adianta:.0}s {} at ({:.0},{:.0}) walk={:.2} working={}",
                m.oficio.nome(),
                p.onde.x,
                p.onde.y,
                p.andar,
                p.trabalhando
            );
        }
    }
    crate::render3d::adianta_o_relogio_da_maquete(0.0);
    for _ in 0..2 {
        crate::render3d::camera_padrao();
        // A MESMA COR de fundo que o painel pinta atrás da maquete: o
        // retângulo dele é 2D e vai pra tela, não pro render target que vira
        // PNG, então sem isto a prévia mostra um mar que o jogo não tem.
        clear_background(crate::render3d::COR_DO_FUNDO_DA_MAQUETE);
        // A MESMA PROPORÇÃO da coluna esquerda do painel: 52% de (1040−56)
        // por 628 de altura = 0,815. Na primeira prévia este retângulo era
        // mais estreito que o de verdade, e o enquadramento que ele mostrou
        // não era o que o jogador veria — o mesmo erro de validar com número
        // diferente do que o jogo usa.
        let lado = (screen_height() * 0.88).min(screen_width() * 0.6);
        let r = Rect::new(
            screen_width() * 0.04,
            screen_height() * 0.05,
            lado * PROP_COLUNA,
            lado,
        );
        let ok = crate::render3d::maquete_da_ilha(
            &t,
            &construcoes,
            r,
            eu,
            0.9,
            crate::render3d::ELEV_PADRAO,
            1.0,
            solido,
            &onde,
            vox,
        );
        unsafe { macroquad::window::get_internal_gl().flush() };
        rt.texture
            .get_texture_data()
            .export_png(&format!("{saida}/maquete.png"));
        println!(
            "[colony preview] panel model: drew={ok}, {} resident(s)",
            onde.len()
        );
        next_frame().await;
    }
}

/// Preview of the Ermo's OASIS (`MMO_PREVIA_OASIS=1`; PNGs in
/// `MMO_PREVIA_SAIDA`): close and far, and the island map with it on.
#[cfg(debug_assertions)]
pub async fn previa_do_oasis() {
    let saida = std::env::var("MMO_PREVIA_SAIDA").unwrap_or_else(|_| "/tmp/tempest-oasis".into());
    std::fs::create_dir_all(&saida).unwrap();
    next_frame().await;
    let rt = render_target_ex(1280, 800, RenderTargetParams { depth: true, sample_count: 1 });
    crate::render3d::define_alvo(Some(rt.clone()));
    let def = shared::terreno::def_da_zona(shared::oasis::ZONA).unwrap();
    let g = shared::terreno::Gerador::da_ilha(def);
    let o = g.oasis().expect("the Ermo has an oasis");
    let mut t = Terreno::novo(def);
    let solido = crate::render3d::material_solido();
    let centro = vec2(o.centro.x, o.centro.y);
    for (nome, distancia) in [("oasis-perto", 55.0f32), ("oasis-longe", 130.0)] {
        let chao = g.altura(o.centro.x, o.centro.y);
        t.atualiza(centro, 16, 4000);
        for _ in 0..3 {
            let cam = Camera3D {
                position: vec3(centro.x + distancia * 0.35, chao + distancia * 0.7, centro.y + distancia),
                target: vec3(centro.x, chao, centro.y),
                up: Vec3::Y,
                render_target: Some(rt.clone()),
                aspect: Some(1.6),
                ..Default::default()
            };
            set_camera(&cam);
            clear_background(Color::from_rgba(150, 186, 214, 255));
            macroquad::material::gl_use_material(&solido);
            t.desenha(&cam, Vec3::ZERO, 0.0);
            t.desenha_sombras(&cam);
            crate::agua::desenha(&t, &cam, 0.0);
            macroquad::material::gl_use_default_material();
            unsafe { get_internal_gl().flush() };
            rt.texture.get_texture_data().export_png(&format!("{saida}/{nome}.png"));
            next_frame().await;
        }
    }
    let mut mapa = crate::mapa::Mapa::para(Some(def));
    mapa.abrir();
    let mundo = crate::world::World::default();
    for _ in 0..120 {
        mapa.acompanhar();
        crate::render3d::camera_padrao();
        clear_background(Color::from_rgba(18, 24, 34, 255));
        mapa.desenha_grande(&mundo, 40, &crate::mundo_ui::Mundo::default(), 0);
        unsafe { get_internal_gl().flush() };
        next_frame().await;
    }
    crate::render3d::camera_padrao();
    clear_background(Color::from_rgba(18, 24, 34, 255));
    mapa.desenha_grande(&mundo, 40, &crate::mundo_ui::Mundo::default(), 0);
    unsafe { get_internal_gl().flush() };
    rt.texture.get_texture_data().export_png(&format!("{saida}/oasis-mapa.png"));
}

/// Preview of the chests (`MMO_PREVIA_BAUS=1`; PNG in `MMO_PREVIA_SAIDA`): the
/// dungeon's and Stormkeep's three, through the real entity pass.
#[cfg(debug_assertions)]
pub async fn previa_baus(vox: &mut crate::vox::VoxCache) {
    let saida = std::env::var("MMO_PREVIA_SAIDA").unwrap_or_else(|_| "/tmp/tempest-baus".into());
    std::fs::create_dir_all(&saida).unwrap();
    let rt = render_target_ex(1280, 800, RenderTargetParams { depth: true, sample_count: 1 });
    crate::render3d::define_alvo(Some(rt.clone()));
    let mut world = crate::world::World::default();
    let papeis = [shared::dungeon::PAPEL_BAU, shared::forte::PAPEL_VERDE, shared::forte::PAPEL_AZUL, shared::forte::PAPEL_ROXO];
    let metas: Vec<shared::EntityMeta> = papeis
        .iter()
        .enumerate()
        .map(|(k, p)| shared::EntityMeta {
            pk: Default::default(),
            auras: 0,
            id: shared::EntityId(100 + k as u32),
            tag: shared::EntityTag::Npc,
            name: Some(format!("chest {p}")),
            hp_max: 1,
            faction: None,
            kind: shared::npc_kind(None, *p),
            nivel: 0,
            desafio: None,
            aparencia: 0,
            skins: 0,
        })
        .collect();
    let states: Vec<shared::EntityState> = (0..4)
        .map(|k| shared::EntityState {
            id: shared::EntityId(100 + k as u32),
            pos: [((k as f32 * 2.5) * shared::POS_SCALE) as i16, 0],
            vel: [0, 0],
            hp: 1,
            flags: 0,
            acao: 0,
            rumo: 0,
        })
        .collect();
    world.apply(metas, states, &[]);
    let chao = |_: f32, _: f32| 0.0f32;
    let solido = crate::render3d::material_solido();
    for _ in 0..6 {
        world.tick(0.016, &chao);
        let mut vista = crate::render3d::Vista::nova(Vec2::new(3.75, 0.0), 0.6, 9.0, 0.5, 0.0, &chao);
        vista.cam.render_target = Some(rt.clone());
        set_camera(&vista.cam);
        clear_background(Color::from_rgba(150, 186, 214, 255));
        draw_plane(vec3(3.75, 0.0, 0.0), vec2(12.0, 8.0), None, Color::from_rgba(110, 140, 90, 255));
        gl_use_material(&solido);
        crate::render3d::draw_entities(&mut world, vox, None, &vista);
        gl_use_default_material();
        unsafe { get_internal_gl().flush() };
        rt.texture.get_texture_data().export_png(&format!("{saida}/baus.png"));
        next_frame().await;
    }
    crate::render3d::define_alvo(None);
}


/// MMO_PREVIA_KOGEN: Kōgen-tō (`shared::kogen`), the city grid — the hub,
/// a Neon District street, downtown, an aerial view and the map — into
/// /tmp/tempest-kogen (or MMO_PREVIA_SAIDA).
#[cfg(debug_assertions)]
pub async fn previa_kogen(_vox: &mut crate::vox::VoxCache) {
    let saida = std::env::var("MMO_PREVIA_SAIDA").unwrap_or_else(|_| "/tmp/tempest-kogen".into());
    std::fs::create_dir_all(&saida).unwrap();
    next_frame().await;
    let rt = render_target_ex(1280, 800, RenderTargetParams { depth: true, sample_count: 1 });
    crate::render3d::define_alvo(Some(rt.clone()));
    let def = &shared::kogen::DEF;
    let g = shared::terreno::Gerador::da_ilha(def);
    let mut t = Terreno::novo(def);
    let mut casas = crate::construcoes::Construcoes::para(Some(def));
    for _ in 0..600 { casas.acompanhar(); if casas.prontas() { break; } next_frame().await; }
    let solido = crate::render3d::material_solido();
    let hub = shared::kogen::centro_da_cidade();
    let marco = |m: shared::kogen::Marco| { let c = m.centro(); (c.x, c.y) };
    use shared::kogen::Marco as Mc;
    // (name, centre, camera distance, camera height factor)
    let vistas: Vec<(&str, (f32, f32), f32, f32)> = vec![
        ("shibuya-cruzamento", { let c = shared::kogen::cruzamento(); (c.x, c.y) }, 90.0, 1.1),
        ("skytree-floresta", marco(Mc::Skytree), 220.0, 0.55),
        ("tocho", marco(Mc::Prefeitura), 180.0, 0.5),
        ("nishi-shinjuku", marco(Mc::Casulo), 160.0, 0.45),
        ("kabukicho", { let c = shared::kogen::de_latlon(35.6935, 139.7020); (c.x, c.y) }, 120.0, 0.5),
        ("torre-de-toquio", marco(Mc::TorreDeToquio), 130.0, 0.4),
        ("hub", (hub.x, hub.y), 120.0, 0.65),
        ("aerea-sul", (0.0, 300.0), 500.0, 0.8),
        ("aerea-norte", (0.0, -300.0), 500.0, 0.8),
    ];
    for (nome, (cx, cy), distancia, alto) in vistas {
        let centro = vec2(cx, cy);
        let chao = g.altura(cx, cy).min(shared::kogen::NIVEL_CHAO as f32 * shared::terreno::BLOCO + 2.0);
        t.atualiza(centro, if distancia > 400.0 { 40 } else { 18 }, 6000);
        for _ in 0..3 {
            let cam = Camera3D {
                position: vec3(centro.x + distancia * 0.45, chao + distancia * alto, centro.y + distancia),
                target: vec3(centro.x, chao, centro.y),
                up: Vec3::Y,
                render_target: Some(rt.clone()),
                aspect: Some(1.6),
                ..Default::default()
            };
            set_camera(&cam);
            clear_background(Color::from_rgba(150, 186, 214, 255));
            macroquad::material::gl_use_material(&solido);
            t.desenha(&cam, Vec3::ZERO, 0.0);
            t.desenha_sombras(&cam);
            casas.desenha(&cam, None, Vec3::ZERO, 0.0);
            crate::agua::desenha(&t, &cam, 0.0);
            macroquad::material::gl_use_default_material();
            unsafe { get_internal_gl().flush() };
            rt.texture.get_texture_data().export_png(&format!("{saida}/{nome}.png"));
            next_frame().await;
        }
    }
    let mut mapa = crate::mapa::Mapa::para(Some(def));
    mapa.abrir();
    let mundo = crate::world::World::default();
    for _ in 0..120 {
        mapa.acompanhar();
        crate::render3d::camera_padrao();
        clear_background(Color::from_rgba(18, 24, 34, 255));
        mapa.desenha_grande(&mundo, 85, &crate::mundo_ui::Mundo::default(), 0);
        unsafe { get_internal_gl().flush() };
        next_frame().await;
    }
    rt.texture.get_texture_data().export_png(&format!("{saida}/mapa.png"));
    crate::render3d::define_alvo(None);
}
