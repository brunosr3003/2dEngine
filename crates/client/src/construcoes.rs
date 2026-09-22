//! Casas, props e o cais da vila, em malha.
//!
//! O QUE desenhar sai do `shared::vila` — funcao da semente, a mesma que o
//! servidor usa pra colisao —, entao nada viaja pela rede. Aqui so' se assa:
//! cada construcao vira malha JA girada e posta no mundo, uma vez, numa
//! thread, e o quadro so' desenha.
//!
//! O mesher e' o da vegetacao (descarte de face interna, oclusao por canto e
//! merge guloso que so' junta mesma cor e mesma sombra), lendo o `Voxels` do
//! shared no lugar do `Volume`. A macroquad nao tem transform por malha e o
//! giro e' so' de 90 em 90 graus, entao assar ja' girado sai de graca.

use std::sync::mpsc::{channel, Receiver};

use macroquad::prelude::*;

use crate::gpu_estatica::MalhaEstatica;
use shared::construcao::{rot_q, BlocoCasa, Construcao, Tipo, TipoCasa};
use shared::terreno::{DefIlha, Gerador, ESCALA_ALTURA};

use crate::vegetacao::ALTURA_QUE_ESCONDE;

/// Mesmo teto do terreno: 800 quads = 3.200 vertices e 4.800 indices, abaixo
/// dos 5.000 que a macroquad corta em silencio.
const MAX_QUADS: usize = 800;

/// Alem disto (no plano, a partir do alvo da camera) a construcao nao e'
/// desenhada. Folgado sobre os 80 do terreno: casa sumindo antes do chao em
/// volta dela le' como buraco.
const ALCANCE: f32 = 110.0;

/// Acima disto (a partir do piso) a construcao e' TETO: o alto das paredes,
/// oitao, beiral, toldo e telhado. Quem esta' dentro nao ve' nada disso — de
/// cima, com a camera alta, o telhado esconderia o personagem e o que ha' la'
/// dentro. Abaixo da verga da porta (2,5) e acima da cabeca (1,7).
pub const ALTURA_DO_TETO: f32 = 2.2;

/// Uma construcao assada: vertices e indices ja' fatiados no teto de indice,
/// separados em BAIXO (`partes`, sempre desenhado) e `teto` (some com o
/// jogador dentro), e a caixa que envolve tudo, pro corte.
pub struct Assada {
    pub partes: Vec<(Vec<Vertex>, Vec<u16>)>,
    pub teto: Vec<(Vec<Vertex>, Vec<u16>)>,
    /// Miolo entre as paredes, no plano (min xz, max xz), e o piso. `None`
    /// pra prop e cais: nao ha' "dentro".
    pub interior: Option<(Vec2, Vec2, f32)>,
    pub min: Vec3,
    pub max: Vec3,
}

struct Pronta {
    /// Na GPU a partir do primeiro desenho (`gpu_estatica`).
    malhas: Vec<MalhaEstatica>,
    /// Tambem na GPU, mas com a copia da CPU: a transicao de sumir recopia.
    teto: Vec<MalhaEstatica>,
    interior: Option<(Vec2, Vec2, f32)>,
    min: Vec3,
    max: Vec3,
    /// 0 = teto a mostra, 1 = escondido. `Cell` porque `desenha` e' `&self`.
    teto_sumido: std::cell::Cell<f32>,
}

/// Quanto dura o teto sumir (ou voltar), em segundos.
const DURACAO_DO_TETO: f32 = 0.3;
/// Quanto o teto sobe enquanto some. Subir tira ele da frente do personagem
/// mais rapido que desbotar: o material escreve profundidade, e um teto
/// meio transparente parado no lugar esconderia quem esta' embaixo.
const SUBIDA_DO_TETO: f32 = 3.0;

/// Um quadro da transicao do teto: anda na direcao de `dentro` (1) ou de fora
/// (0) no ritmo de `DURACAO_DO_TETO`, sem pular.
pub fn avanca_teto(p: f32, dentro: bool, dt: f32) -> f32 {
    let passo = dt.max(0.0) / DURACAO_DO_TETO;
    if dentro {
        (p + passo).min(1.0)
    } else {
        (p - passo).max(0.0)
    }
}

/// O teto no meio da transicao: copia subida e desbotada. So' existe durante
/// os 0,3 s — fora deles o teto assado e' desenhado como esta'.
fn desenha_teto_em_transicao(teto: &[MalhaEstatica], p: f32) {
    // Suave nas duas pontas: comeca e termina devagar.
    let t = p * p * (3.0 - 2.0 * p);
    let subida = SUBIDA_DO_TETO * t;
    let alfa = ((1.0 - t) * 255.0) as u8;
    for m in teto.iter().filter_map(|m| m.cpu()) {
        let vertices = m
            .vertices
            .iter()
            .map(|v| {
                let mut v = *v;
                v.position.y += subida;
                v.color[3] = alfa;
                v
            })
            .collect();
        draw_mesh(&Mesh {
            vertices,
            indices: m.indices.clone(),
            texture: None,
        });
    }
}

/// O jogador esta' DENTRO deste miolo (e nao em cima do telhado)?
pub fn esta_dentro(interior: Option<(Vec2, Vec2, f32)>, jogador: Option<Vec3>) -> bool {
    let (Some((mn, mx, piso)), Some(j)) = (interior, jogador) else {
        return false;
    };
    j.x > mn.x && j.x < mx.x && j.z > mn.y && j.z < mx.y && j.y < piso + ALTURA_DO_TETO
}

/// As construcoes da zona. Vazio em zona sem ilha.
#[derive(Default)]
pub struct Construcoes {
    rx: Option<Receiver<Vec<Assada>>>,
    prontas: Vec<Pronta>,
}

impl Construcoes {
    /// Comeca a assar a vila da ilha `def` numa thread. A `Mesh` so' e'
    /// montada no thread principal, em `acompanhar`.
    /// Sem construcao nenhuma. A COLONIA (docs/COLONIA.md) e' ilha sem vila:
    /// ela nao tem NPC, e casa vazia e' pior que campo aberto.
    pub fn vazia() -> Self {
        Self::default()
    }

    /// O ASSENTAMENTO da colonia: a casa do jogador mais uma construcao por
    /// morador contratado.
    ///
    /// Sai da MESMA vila que o mundo normal (`Gerador::vila`), filtrada pelo
    /// oficio de quem mora ali. Escrever um gerador de vila proprio pra
    /// colonia seria uma segunda fonte de verdade pro mesmo desenho — e o
    /// chao ja' esta' aplainado debaixo dela, porque o plato e' do gerador.
    pub fn da_colonia(plato: f32, trabalhadores: Vec<shared::colonia::Profissao>) -> Self {
        let (tx, rx) = channel();
        std::thread::spawn(move || {
            let _ = tx.send(assar_colonia(plato, &trabalhadores));
        });
        Self {
            rx: Some(rx),
            prontas: Vec::new(),
        }
    }

    pub fn para(def: Option<&'static DefIlha>) -> Self {
        let Some(def) = def else {
            return Self::default();
        };
        let (tx, rx) = channel();
        std::thread::spawn(move || {
            let _ = tx.send(assar_vila(def));
        });
        Self {
            rx: Some(rx),
            prontas: Vec::new(),
        }
    }

    /// A vila ja' foi assada (ou nao ha' vila).
    pub fn prontas(&self) -> bool {
        self.rx.is_none() || !self.prontas.is_empty()
    }

    pub fn acompanhar(&mut self) {
        let Some(rx) = &self.rx else { return };
        let Ok(assadas) = rx.try_recv() else { return };
        self.prontas = assadas
            .into_iter()
            .map(|a| {
                let malha = |(vertices, indices)| Mesh {
                    vertices,
                    indices,
                    texture: None,
                };
                Pronta {
                    malhas: a
                        .partes
                        .into_iter()
                        .map(|p| MalhaEstatica::nova(malha(p)))
                        .collect(),
                    teto: a
                        .teto
                        .into_iter()
                        .map(|p| MalhaEstatica::mantendo_cpu(malha(p)))
                        .collect(),
                    interior: a.interior,
                    min: a.min,
                    max: a.max,
                    teto_sumido: std::cell::Cell::new(0.0),
                }
            })
            .collect();
        self.rx = None;
    }

    /// Desenha o que esta' perto e no cone da camera. Vai no passe do terreno,
    /// com o material solido e o recorte ligados. Devolve quantas desenhou.
    ///
    /// `jogador` e' a posicao do personagem: a construcao em que ele esta'
    /// DENTRO e' desenhada sem o teto.
    /// `recorte`/`recorte_z`: o furo do jogador, como no terreno.
    pub fn desenha(
        &self,
        cam: &Camera3D,
        jogador: Option<Vec3>,
        recorte: Vec3,
        recorte_z: f32,
    ) -> usize {
        let olho = cam.position;
        let frente = (cam.target - cam.position).normalize();
        let abertura = cam.fovy * 0.5 + 0.55;
        // Teto de quadro longo (janela arrastada, carregamento) nao vira
        // teleporte: no maximo um decimo de segundo por quadro.
        let dt = get_frame_time().min(0.1);
        let mut n = 0;
        let mut fixas: Vec<&MalhaEstatica> = Vec::new();
        let mut em_transicao: Vec<(&Vec<MalhaEstatica>, f32)> = Vec::new();
        for p in &self.prontas {
            // Antes do corte: a transicao anda mesmo com a casa fora da tela.
            let sumido = avanca_teto(p.teto_sumido.get(), esta_dentro(p.interior, jogador), dt);
            p.teto_sumido.set(sumido);
            let centro = (p.min + p.max) * 0.5;
            let raio = (p.max - p.min).length() * 0.5;
            if vec2(centro.x - cam.target.x, centro.z - cam.target.z).length() > ALCANCE + raio {
                continue;
            }
            let d = centro - olho;
            let dist = d.length();
            if dist > raio {
                let folga = (raio / dist).min(1.0).asin();
                if d.normalize().dot(frente) < (abertura + folga).min(std::f32::consts::PI).cos() {
                    continue;
                }
            }
            n += 1;
            fixas.extend(p.malhas.iter());
            let sumido = p.teto_sumido.get();
            if sumido <= 0.0 {
                fixas.extend(p.teto.iter());
            } else if sumido < 1.0 {
                em_transicao.push((&p.teto, sumido));
            }
        }
        crate::gpu_estatica::desenha(
            crate::gpu_estatica::Programa::Solido { recorte, recorte_z },
            fixas,
        );
        // O teto sumindo e' recopiado todo quadro: esse fica no lote da
        // macroquad (so' existe por 0,3 s).
        for (teto, sumido) in em_transicao {
            desenha_teto_em_transicao(teto, sumido);
        }
        n
    }
}

/// Todas as construcoes da vila e do porto da ilha, assadas. Roda FORA do
/// quadro.
pub fn assar_vila(def: &DefIlha) -> Vec<Assada> {
    let ger = Gerador::da_ilha(def);
    let vila = ger.vila();
    let mut saida = Vec::with_capacity(vila.predios.len() + vila.props.len());
    for p in &vila.predios {
        saida.push(assar(&p.construcao(), p.pos, p.yaw_q, p.chao));
    }
    let props = vila
        .props
        .iter()
        .map(|p| assar(&p.construcao(), p.pos, p.yaw_q, p.pos.y))
        .collect();
    saida.extend(juntar_por_regiao(props, 12.0));
    saida
}

/// As construcoes da COLONIA: a casa do jogador e a de cada morador.
///
/// A vila do gerador tem o povoado inteiro; aqui so' entra o que foi
/// CONTRATADO. Um oficio repetido leva mais de um predio daquele papel — dois
/// lenhadores sao duas casas, e nao uma casa que rende o dobro.
pub fn assar_colonia(plato: f32, trabalhadores: &[shared::colonia::Profissao]) -> Vec<Assada> {
    use shared::construcao::Papel;
    let ger = Gerador::da_colonia(plato);
    let vila = ger.vila();
    // Quantos predios de cada papel o assentamento pede.
    let mut querido: std::collections::HashMap<Papel, usize> = std::collections::HashMap::new();
    for t in trabalhadores {
        *querido.entry(t.papel()).or_default() += 1;
    }
    let mut saida = Vec::new();
    let mut usado: std::collections::HashMap<Papel, usize> = std::collections::HashMap::new();
    for p in &vila.predios {
        // A CASA do jogador entra sempre: e' ela que faz a ilha ser dele
        // desde o primeiro dia, antes de existir morador nenhum.
        let quero = if p.papel == Papel::Casa {
            1
        } else {
            querido.get(&p.papel).copied().unwrap_or(0)
        };
        let ja = usado.entry(p.papel).or_default();
        if *ja >= quero {
            continue;
        }
        *ja += 1;
        saida.push(assar(&p.construcao(), p.pos, p.yaw_q, p.chao));
    }
    // Os props (cerca, vaso, lampiao) ficam: sao eles que fazem o lugar
    // parecer morado em vez de construido.
    let props = vila
        .props
        .iter()
        .map(|p| assar(&p.construcao(), p.pos, p.yaw_q, p.pos.y))
        .collect();
    saida.extend(juntar_por_regiao(props, 12.0));
    saida
}

/// Junta os props de uma mesma regiao numa construcao so'.
///
/// A vila tem centenas de flor, vaso, cerca e lampiao; cada um com a propria
/// `Mesh` seriam centenas de chamadas de desenho por quadro. Por regiao de 12
/// u sao algumas dezenas, e o corte pela caixa continua valendo.
fn juntar_por_regiao(assadas: Vec<Assada>, lado: f32) -> Vec<Assada> {
    use std::collections::BTreeMap;
    let mut grupos: BTreeMap<(i32, i32), Vec<Assada>> = BTreeMap::new();
    for a in assadas {
        if a.partes.is_empty() {
            continue;
        }
        let c = (a.min + a.max) * 0.5;
        grupos
            .entry(((c.x / lado).floor() as i32, (c.z / lado).floor() as i32))
            .or_default()
            .push(a);
    }
    grupos
        .into_values()
        .map(|grupo| {
            let mut saida = Assada {
                partes: Vec::new(),
                teto: Vec::new(),
                interior: None,
                min: Vec3::splat(f32::MAX),
                max: Vec3::splat(f32::MIN),
            };
            let mut buf: (Vec<Vertex>, Vec<u16>) = (Vec::new(), Vec::new());
            for a in grupo {
                saida.min = saida.min.min(a.min);
                saida.max = saida.max.max(a.max);
                for (v, i) in a.partes {
                    if !buf.1.is_empty() && buf.1.len() + i.len() > MAX_QUADS * 6 {
                        saida.partes.push(std::mem::take(&mut buf));
                    }
                    let base = buf.0.len() as u16;
                    buf.0.extend(v);
                    buf.1.extend(i.into_iter().map(|k| k + base));
                }
            }
            if !buf.1.is_empty() {
                saida.partes.push(buf);
            }
            saida
        })
        .collect()
}

/// (normal local, eixo u, eixo v) de cada face.
const FACES: [([i32; 3], usize, usize); 6] = [
    ([0, 1, 0], 0, 2),
    ([0, -1, 0], 0, 2),
    ([1, 0, 0], 2, 1),
    ([-1, 0, 0], 2, 1),
    ([0, 0, 1], 0, 1),
    ([0, 0, -1], 0, 1),
];

/// Assa uma construcao girada (`yaw_q`) e posta em `pos` — `xz` e' o pivo
/// (centro do volume) e `y` a origem, ver `Construcao::local_para_mundo`.
/// `chao` e' a altura em que se pisa: a face que passa de
/// `ALTURA_QUE_ESCONDE` acima dele entra no recorte da camera.
pub fn assar(c: &Construcao, pos: ::glam::Vec3, yaw_q: u8, chao: f32) -> Assada {
    let v = &c.v;
    let lo = [v.x0, v.y0, v.z0];
    let dim = [v.nx, v.ny, v.nz];
    let e = c.escala;
    // So' casa tem dentro. Prop e cais sao abertos: nada a esconder.
    let fechada = matches!(c.tipo, Tipo::Casa(t) if t != TipoCasa::Doca);
    let alto = |y: i32| fechada && pos.y + y as f32 * e >= chao + ALTURA_DO_TETO;
    // O miolo: um bloco pra dentro das paredes (os volumes de casa nascem com
    // uma celula de folga pro beiral, dai' o +2/-2).
    let interior = fechada.then(|| {
        let (lx, hx) = ((v.x0 + 2) as f32 * e, (v.x0 + v.nx - 2) as f32 * e);
        let (lz, hz) = ((v.z0 + 2) as f32 * e, (v.z0 + v.nz - 2) as f32 * e);
        let a = c.local_para_mundo(pos, yaw_q, ::glam::Vec3::new(lx, 0.0, lz));
        let b = c.local_para_mundo(pos, yaw_q, ::glam::Vec3::new(hx, 0.0, hz));
        (
            vec2(a.x.min(b.x), a.z.min(b.z)),
            vec2(a.x.max(b.x), a.z.max(b.z)),
            chao,
        )
    });
    let mut s = Saida {
        a: Assada {
            partes: Vec::new(),
            teto: Vec::new(),
            interior,
            min: Vec3::splat(f32::MAX),
            max: Vec3::splat(f32::MIN),
        },
        baixo: (Vec::new(), Vec::new()),
        cima: (Vec::new(), Vec::new()),
    };
    for (n, eu, ev) in FACES {
        let eixo = n.iter().position(|k| *k != 0).unwrap();
        let (nu, nv) = (dim[eu].max(0) as usize, dim[ev].max(0) as usize);
        // O `bool` e' "e' teto": entra na chave do merge, senao uma parede
        // corrida do chao ao oitao viraria um quad so' e o corte nao teria onde
        // passar.
        let mut mascara: Vec<Option<(u8, [u8; 4], bool)>> = vec![None; nu * nv];
        for camada in 0..dim[eixo] {
            for iv in 0..nv {
                for iu in 0..nu {
                    let mut p = [0i32; 3];
                    p[eixo] = lo[eixo] + camada;
                    p[eu] = lo[eu] + iu as i32;
                    p[ev] = lo[ev] + iv as i32;
                    let b = v.get(p[0], p[1], p[2]);
                    // TAMPA do corte: o bloco logo abaixo do teto ganha a face
                    // de cima mesmo com bloco em cima. Sem ela, com o teto
                    // escondido, a parede (um voxel de espessura) aparecia oca
                    // — furada — por dentro. Com o teto a mostra, a tampa fica
                    // entre dois blocos e ninguem ve'.
                    let tampa = n[1] == 1 && !alto(p[1]) && alto(p[1] + 1);
                    let visivel =
                        b != 0 && (tampa || v.get(p[0] + n[0], p[1] + n[1], p[2] + n[2]) == 0);
                    mascara[iv * nu + iu] = visivel.then(|| {
                        // A oclusao olharia o bloco de cima (que e' teto) e a
                        // tampa sairia preta: corte limpo e' luz cheia.
                        let ao = if tampa {
                            [255; 4]
                        } else {
                            [
                                oclusao(c, p, n, eu, ev, -1, -1),
                                oclusao(c, p, n, eu, ev, 1, -1),
                                oclusao(c, p, n, eu, ev, 1, 1),
                                oclusao(c, p, n, eu, ev, -1, 1),
                            ]
                        };
                        (b, ao, alto(p[1]))
                    });
                }
            }
            let mut iv = 0usize;
            while iv < nv {
                let mut iu = 0usize;
                while iu < nu {
                    let Some(celula) = mascara[iv * nu + iu] else {
                        iu += 1;
                        continue;
                    };
                    let mut w = 1;
                    while iu + w < nu && mascara[iv * nu + iu + w] == Some(celula) {
                        w += 1;
                    }
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
                    let q = Quad {
                        p,
                        n,
                        eu,
                        ev,
                        w: w as i32,
                        h: h as i32,
                        bloco: celula.0,
                        ao: celula.1,
                    };
                    s.emite(c, pos, yaw_q, chao, q, celula.2);
                    iu += w;
                }
                iv += 1;
            }
        }
    }
    if !s.baixo.1.is_empty() {
        s.a.partes.push(s.baixo);
    }
    if !s.cima.1.is_empty() {
        s.a.teto.push(s.cima);
    }
    s.a
}

struct Quad {
    p: [i32; 3],
    n: [i32; 3],
    eu: usize,
    ev: usize,
    w: i32,
    h: i32,
    bloco: u8,
    ao: [u8; 4],
}

struct Saida {
    a: Assada,
    baixo: (Vec<Vertex>, Vec<u16>),
    cima: (Vec<Vertex>, Vec<u16>),
}

impl Saida {
    fn emite(
        &mut self,
        c: &Construcao,
        pos: ::glam::Vec3,
        yaw_q: u8,
        chao: f32,
        q: Quad,
        teto: bool,
    ) {
        let (buf, fechadas) = if teto {
            (&mut self.cima, &mut self.a.teto)
        } else {
            (&mut self.baixo, &mut self.a.partes)
        };
        if buf.1.len() / 6 >= MAX_QUADS {
            fechadas.push(std::mem::take(buf));
        }
        let e = c.escala;
        let mut base = [0f32; 3];
        for k in 0..3 {
            base[k] = q.p[k] as f32 + 0.5 + q.n[k] as f32 * 0.5;
        }
        let canto = |su: f32, sv: f32| {
            let mut l = base;
            l[q.eu] += su - 0.5;
            l[q.ev] += sv - 0.5;
            let m = c.local_para_mundo(pos, yaw_q, ::glam::Vec3::new(l[0] * e, l[1] * e, l[2] * e));
            vec3(m.x, m.y, m.z)
        };
        let (w, h) = (q.w as f32, q.h as f32);
        let pontos = [
            (canto(0.0, 0.0), q.ao[0]),
            (canto(w, 0.0), q.ao[1]),
            (canto(w, h), q.ao[2]),
            (canto(0.0, h), q.ao[3]),
        ];
        // O giro e' so' no plano XZ: a normal de mundo sai do mesmo `rot_q`.
        let (nx, nz) = rot_q(q.n[0] as f32, q.n[2] as f32, yaw_q);
        let normal = vec3(nx, q.n[1] as f32, nz);
        // Luz pela direcao NO MUNDO, igual a vegetacao: a mesma parede girada
        // tem que tomar a luz do lado pra onde ficou virada.
        let luz = if q.n[1] > 0 {
            1.0
        } else if q.n[1] < 0 {
            0.45
        } else if nx.abs() > 0.5 {
            0.76
        } else {
            0.60
        };
        // Ordem dos cantos pela conta, nao pela mao: ver `vegetacao::emite`.
        let geom = (pontos[1].0 - pontos[0].0).cross(pontos[2].0 - pontos[0].0);
        let pontos = if geom.dot(normal) >= 0.0 {
            pontos
        } else {
            [pontos[0], pontos[3], pontos[2], pontos[1]]
        };
        let bloco = BlocoCasa::de_u8(q.bloco).unwrap_or(BlocoCasa::Ar);
        let [r, g, b] = bloco.rgb();
        let topo = pontos.iter().map(|(p, _)| p.y).fold(f32::MIN, f32::max);
        let marca = if topo - chao >= ALTURA_QUE_ESCONDE {
            1.0
        } else {
            0.0
        };
        let buf = if teto {
            &mut self.cima
        } else {
            &mut self.baixo
        };
        let inicio = buf.0.len() as u16;
        for (p, oc) in pontos {
            let k = if bloco.brilha() {
                1.0
            } else {
                luz * (oc as f32 / 255.0)
            };
            self.a.min = self.a.min.min(p);
            self.a.max = self.a.max.max(p);
            buf.0.push(Vertex {
                position: p,
                uv: vec2(0.0, 0.0),
                color: [
                    (r as f32 * k) as u8,
                    (g as f32 * k) as u8,
                    (b as f32 * k) as u8,
                    255,
                ],
                normal: Vec4::new(marca, 0.0, 0.0, 0.0),
            });
        }
        buf.1.extend_from_slice(&[
            inicio,
            inicio + 1,
            inicio + 2,
            inicio,
            inicio + 2,
            inicio + 3,
        ]);
    }
}

/// Quanto o canto esta' encoberto, 140 (fechado) a 255 (aberto). Mesma conta
/// de `vegetacao::oclusao`.
fn oclusao(c: &Construcao, p: [i32; 3], n: [i32; 3], eu: usize, ev: usize, su: i32, sv: i32) -> u8 {
    let mut la = n;
    la[eu] += su;
    let mut lb = n;
    lb[ev] += sv;
    let mut dg = n;
    dg[eu] += su;
    dg[ev] += sv;
    let em = |d: [i32; 3]| c.v.get(p[0] + d[0], p[1] + d[1], p[2] + d[2]) != 0;
    let (s1, s2) = (em(la), em(lb));
    if s1 && s2 {
        return 140;
    }
    let ocupados = s1 as u32 + s2 as u32 + em(dg) as u32;
    (255 - ocupados * 38) as u8
}

#[cfg(test)]
mod tests {
    use super::*;
    use shared::terreno::ARQUIPELAGO;
    use shared::{EntityId, EntityMeta, EntityState, EntityTag};

    fn quads(a: &Assada) -> impl Iterator<Item = [Vec3; 4]> + '_ {
        a.partes.iter().chain(a.teto.iter()).flat_map(|(v, i)| {
            i.chunks(6).map(move |q| {
                [
                    v[q[0] as usize].position,
                    v[q[1] as usize].position,
                    v[q[2] as usize].position,
                    v[q[5] as usize].position,
                ]
            })
        })
    }

    /// O segmento `a`-`b` atravessa o MIOLO do quad (alinhado aos eixos)?
    fn corta(q: [Vec3; 4], a: Vec3, b: Vec3) -> bool {
        let mn = q.iter().fold(Vec3::splat(f32::MAX), |m, p| m.min(*p));
        let mx = q.iter().fold(Vec3::splat(f32::MIN), |m, p| m.max(*p));
        let ext = mx - mn;
        let eixo = if ext.x < 1e-4 {
            0
        } else if ext.y < 1e-4 {
            1
        } else {
            2
        };
        let (da, db) = (a[eixo] - mn[eixo], b[eixo] - mn[eixo]);
        if da * db > 0.0 || (da - db).abs() < 1e-6 {
            return false;
        }
        let p = a + (b - a) * (da / (da - db));
        (0..3)
            .filter(|k| *k != eixo)
            .all(|k| p[k] > mn[k] + 1e-3 && p[k] < mx[k] - 1e-3)
    }

    /// Onde a malha deixa passar e onde a colisao do SERVIDOR deixa passar tem
    /// que ser o mesmo lugar: porta desenhada numa coluna e aberta na outra e'
    /// o jogador batendo em parede invisivel ao lado de um vao de verdade.
    #[test]
    fn a_porta_da_malha_e_a_porta_da_colisao() {
        let d = &ARQUIPELAGO[0];
        let ger = Gerador::da_ilha(d);
        let mut casas = 0;
        for p in ger
            .vila()
            .predios
            .iter()
            .filter(|p| p.tipo != TipoCasa::Doca)
        {
            let c = p.construcao();
            let Some(px) = c.porta_local() else { continue };
            let a = assar(&c, p.pos, p.yaw_q, p.chao);
            // Altura de peito acima do piso de dentro, em eixo local.
            let y = (p.chao - p.pos.y) + 1.2;
            let mundo = |x: f32, z: f32| {
                let m = c.local_para_mundo(p.pos, p.yaw_q, ::glam::Vec3::new(x, y, z));
                vec3(m.x, m.y, m.z)
            };
            let (fora, dentro) = (mundo(px, -1.0), mundo(px, 1.5));
            assert!(
                !quads(&a).any(|q| corta(q, fora, dentro)),
                "{:?}/{:?}: a malha fecha a porta",
                p.tipo,
                p.papel
            );
            let caixas = c.caixas_mundo(p.pos, p.yaw_q);
            for k in 0..=24 {
                let q = fora.lerp(dentro, k as f32 / 24.0);
                let barra = caixas.iter().any(|(mn, mx)| {
                    q.x > mn.x + 1e-3
                        && q.x < mx.x - 1e-3
                        && q.y > mn.y
                        && q.y < mx.y
                        && q.z > mn.z + 1e-3
                        && q.z < mx.z - 1e-3
                });
                assert!(
                    !barra,
                    "{:?}/{:?}: a colisao fecha a porta que a malha abre",
                    p.tipo, p.papel
                );
            }
            // Controle: na coluna do canto da fachada ha' parede — sem isto o
            // teste passaria com uma malha vazia.
            let canto = c.escala * 0.5;
            assert!(
                quads(&a).any(|q| corta(q, mundo(canto, -1.0), mundo(canto, 1.5))),
                "{:?}/{:?}: controle — o canto da fachada nao tem parede na malha",
                p.tipo,
                p.papel
            );
            casas += 1;
        }
        println!("{casas} portas conferidas");
        assert!(
            casas >= 5,
            "so' {casas} predios com porta na cidade inicial"
        );
    }

    /// Nenhuma malha da vila passa do teto de indice, em nenhuma ilha.
    #[test]
    fn nenhuma_malha_da_vila_estoura_o_teto_de_indice() {
        for d in &ARQUIPELAGO {
            let t0 = std::time::Instant::now();
            let assadas = assar_vila(d);
            let tempo = t0.elapsed();
            let (mut malhas, mut verts, mut quads) = (0, 0, 0);
            for a in &assadas {
                for (v, i) in a.partes.iter().chain(a.teto.iter()) {
                    assert!(
                        i.len() <= MAX_QUADS * 6,
                        "{}: malha com {} indices",
                        d.zona,
                        i.len()
                    );
                    assert!(v.len() <= MAX_QUADS * 4);
                    malhas += 1;
                    verts += v.len();
                    quads += i.len() / 6;
                }
            }
            println!(
                "{}: {} construcoes, {malhas} malhas, {verts} vertices, {quads} quads, assada em {tempo:?}",
                d.zona,
                assadas.len()
            );
            assert!(!assadas.is_empty(), "{}: vila vazia", d.zona);
        }
    }

    /// A casa assenta no chao do plato: a base da malha e' a origem do volume,
    /// e o piso de dentro e' o chao que o terreno desenha.
    #[test]
    fn a_casa_assenta_no_chao() {
        let d = &ARQUIPELAGO[0];
        let ger = Gerador::da_ilha(d);
        for p in ger
            .vila()
            .predios
            .iter()
            .filter(|p| p.tipo != TipoCasa::Doca)
        {
            let a = assar(&p.construcao(), p.pos, p.yaw_q, p.chao);
            assert!(
                (a.min.y - p.pos.y).abs() < 0.01,
                "{:?}: base {} != origem {}",
                p.papel,
                a.min.y,
                p.pos.y
            );
            let solo = ger.altura(p.pos.x, p.pos.z);
            assert!(
                (p.chao - solo).abs() < 0.01,
                "{:?}: piso {} fora do chao {}",
                p.papel,
                p.chao,
                solo
            );
        }
    }

    /// Dentro da casa o teto some; na porta, do lado de fora, e em cima do
    /// telhado, nao. E o que some e' SO' o alto: nada abaixo da altura do teto
    /// vai junto (a parede baixa continua dizendo onde e' a casa).
    #[test]
    fn o_teto_some_so_por_dentro() {
        let d = &ARQUIPELAGO[0];
        let ger = Gerador::da_ilha(d);
        let mut casas = 0;
        for p in ger
            .vila()
            .predios
            .iter()
            .filter(|p| p.tipo != TipoCasa::Doca)
        {
            let c = p.construcao();
            let a = assar(&c, p.pos, p.yaw_q, p.chao);
            assert!(!a.teto.is_empty(), "{:?}: casa sem teto separado", p.papel);
            for (v, _) in &a.teto {
                assert!(
                    v.iter()
                        .all(|x| x.position.y >= p.chao + ALTURA_DO_TETO - 1e-3),
                    "{:?}: face de teto abaixo da altura do teto",
                    p.papel
                );
            }
            // A parede cortada tem TAMPA: quad horizontal na malha de baixo, na
            // primeira camada de teto. Sem ela a parede parece furada.
            let e = c.escala;
            let k = (0..c.v.ny)
                .map(|y| c.v.y0 + y)
                .find(|y| p.pos.y + *y as f32 * e >= p.chao + ALTURA_DO_TETO)
                .expect("casa sem camada de teto");
            let y_corte = p.pos.y + k as f32 * e;
            let tampas = quads(&Assada {
                partes: a.partes.iter().cloned().collect(),
                teto: Vec::new(),
                interior: None,
                min: a.min,
                max: a.max,
            })
            .filter(|q| q.iter().all(|v| (v.y - y_corte).abs() < 1e-3))
            .count();
            assert!(tampas > 0, "{:?}: parede cortada sem tampa", p.papel);
            let (mn, mx, _) = a.interior.expect("casa sem miolo");
            let meio = vec3((mn.x + mx.x) * 0.5, p.chao, (mn.y + mx.y) * 0.5);
            assert!(
                esta_dentro(a.interior, Some(meio)),
                "{:?}: o meio da casa nao conta como dentro",
                p.papel
            );
            assert!(
                !esta_dentro(a.interior, Some(meio + vec3(0.0, 4.0, 0.0))),
                "{:?}: em cima do telhado nao e' dentro",
                p.papel
            );
            if let Some(px) = c.porta_local() {
                let m = c.local_para_mundo(p.pos, p.yaw_q, ::glam::Vec3::new(px, 0.0, -1.0));
                assert!(
                    !esta_dentro(a.interior, Some(vec3(m.x, p.chao, m.z))),
                    "{:?}: fora da porta conta como dentro",
                    p.papel
                );
            }
            casas += 1;
        }
        assert!(casas >= 5);
        // Cais e prop nao tem dentro.
        let prop = &ger.vila().props[0];
        assert!(assar(&prop.construcao(), prop.pos, prop.yaw_q, prop.pos.y)
            .interior
            .is_none());
    }

    /// O teto nao some nem volta de uma vez: anda ate' 1 entrando, volta a 0
    /// saindo, e nenhum quadro pula mais que o dt permite.
    #[test]
    fn o_teto_anima_sem_pular() {
        let dt = 1.0 / 60.0;
        let mut p = 0.0f32;
        let mut quadros = 0;
        while p < 1.0 {
            let antes = p;
            p = avanca_teto(p, true, dt);
            assert!(p - antes <= dt / DURACAO_DO_TETO + 1e-5, "pulou entrando");
            quadros += 1;
            assert!(quadros < 1000);
        }
        let esperado = (DURACAO_DO_TETO / dt).ceil() as i32;
        assert!(
            (quadros - esperado).abs() <= 1,
            "entrar levou {quadros} quadros, esperado ~{esperado}"
        );
        // Sai no meio da volta: inverte sem salto.
        for _ in 0..5 {
            p = avanca_teto(p, false, dt);
        }
        assert!(p > 0.0 && p < 1.0);
        let meio = p;
        p = avanca_teto(p, true, dt);
        assert!(
            (p - meio).abs() <= dt / DURACAO_DO_TETO + 1e-5,
            "inverter pulou"
        );
        while p > 0.0 {
            p = avanca_teto(p, false, dt);
        }
        assert_eq!(p, 0.0);
        // Quadro de dt negativo ou gigante nao estraga.
        assert_eq!(avanca_teto(0.5, true, -1.0), 0.5);
        assert_eq!(avanca_teto(0.5, true, 10.0), 1.0);
    }

    /// NPC de porta nasce olhando pro rumo que veio no `kind` e fica assim
    /// parado.
    #[test]
    fn npc_nasce_olhando_pro_rumo_da_rede() {
        let yaw = 2.0f32;
        let kind = ((yaw / std::f32::consts::TAU * 256.0).round() as u16 % 256) + 1;
        let esperado = shared::npc_yaw_de_kind(kind).unwrap();
        let mut w = crate::world::World::default();
        let meta = EntityMeta {
            id: EntityId(7),
            tag: EntityTag::Npc,
            name: None,
            hp_max: 1,
            faction: None,
            kind,
            nivel: 1,
                aparencia: 0,
            };
        let estado =
            EntityState::quantize(EntityId(7), ::glam::Vec2::ZERO, ::glam::Vec2::ZERO, 1, 0);
        w.apply(vec![meta], vec![estado], &[]);
        assert!((w.ents[&EntityId(7)].yaw - esperado).abs() < 1e-4);
        for _ in 0..30 {
            w.tick(1.0 / 30.0, &|_, _| 0.0);
        }
        assert!(
            (w.ents[&EntityId(7)].yaw - esperado).abs() < 1e-4,
            "parado virou"
        );
    }
}
