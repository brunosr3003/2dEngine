//! Terreno determinístico: o mundo sai da SEMENTE, não do fio.
//!
//! Servidor e cliente geram o mesmo relevo a partir do mesmo número, então
//! terreno **nunca** trafega. Isso mata o problema que o `map.rs` do cliente
//! documentava: o arquipelago real tem ~24M tiles, o que dava ~72MB de JSON
//! por login.
//!
//! Portado do `Island.cs` do Sea of Cubes, com tres simplificacoes que a
//! decisao de projeto pediu:
//!
//!   * **Sem volume no servidor.** Ele guarda dois bytes por coluna (a
//!     altura) e nada mais. O volume voxel e' assunto do cliente, gerado por
//!     chunk em volta do jogador. 1 km² custa ~16 MB aqui contra ~416 MB la'.
//!   * **Sem caverna.** O jogador anda na superficie; boca de caverna vira
//!     portal pra outra zona, que e' o que o jogo ja' faz com campo→cidade.
//!     E' isso que mantem a posicao de rede em 2D e o snapshot em 13 bytes.
//!   * **Sem turbulencia no topo.** No original o ruido 3D desloca a
//!     superficie ate' 3,5 m pra abrir beiral. Aqui o topo visivel e' o
//!     mesmo que a colisao enxerga — beiral bonito com colisao mentindo e'
//!     jogador preso no ar.
//!
//! O que sobrou e' fielmente o do original: Perlin com LCG proprio (nada de
//! `rand`, que muda entre versoes), feitio de ilha, mascara de costa, os tres
//! tipos de relevo com selecao suave, terraco e queda de borda.

use serde::{Deserialize, Serialize};

/// Aresta do voxel, em unidades de mundo. Uma unidade = um metro = um tile do
/// mapa antigo; o jogador tem ~1,7.
pub const BLOCO: f32 = 0.5;

/// Acima disto e' terra. O relevo nasce em metros com o mar no zero.
pub const NIVEL_DO_MAR: f32 = 0.0;

/// Degrau que se sobe ANDANDO: um bloco, como escada.
pub const DEGRAU_BLOCOS: i32 = 1;

/// Degrau que se sobe PULANDO. Acima disto e' parede — nao ha' escalada.
///
/// O teto existe porque "dois ou mais so' com pulo" sozinho deixaria um
/// paredao de vinte blocos ser pulado tambem. Um bloco anda, dois pula, tres
/// nao passa.
pub const PULO_BLOCOS: i32 = 2;

/// Lado da celula do A*, em blocos. Oito blocos = quatro unidades.
///
/// Grossa de proposito: a ilha grande tem 10,2 milhoes de colunas e A* nelas
/// seria um mundo inteiro por clique. A rota so' precisa dizer POR ONDE ir; o
/// desvio fino continua com o `mover_e_deslizar`.
pub const PASSO_CAMINHO: i32 = 8;

// ─────────────────────────────── ruido ───────────────────────────────

/// Perlin classico (gradiente), deterministico por semente.
///
/// A permutacao e' embaralhada com um LCG escrito aqui e nao com `rand`: o
/// mundo inteiro depende de servidor e cliente sorteiam a MESMA sequencia, e
/// isso nao pode depender da versao de uma dependencia.
pub struct Perlin {
    p: [u16; 512],
}

impl Perlin {
    pub fn novo(semente: i32) -> Self {
        let mut ordem: [u16; 256] = [0; 256];
        for (i, v) in ordem.iter_mut().enumerate() {
            *v = i as u16;
        }
        let mut est = (semente as u32).wrapping_mul(1664525).wrapping_add(1013904223);
        for i in (1..256).rev() {
            est = est.wrapping_mul(1664525).wrapping_add(1013904223);
            let j = (est % (i as u32 + 1)) as usize;
            ordem.swap(i, j);
        }
        let mut p = [0u16; 512];
        for i in 0..512 {
            p[i] = ordem[i & 255];
        }
        Self { p }
    }

    fn suaviza(t: f32) -> f32 {
        t * t * t * (t * (t * 6.0 - 15.0) + 10.0)
    }

    fn grad(hash: u16, x: f32, y: f32) -> f32 {
        match hash & 7 {
            0 => x + y,
            1 => -x + y,
            2 => x - y,
            3 => -x - y,
            4 => x,
            5 => -x,
            6 => y,
            _ => -y,
        }
    }

    pub fn ruido(&self, x: f32, y: f32) -> f32 {
        let xi = (x.floor() as i32 & 255) as usize;
        let yi = (y.floor() as i32 & 255) as usize;
        let xf = x - x.floor();
        let yf = y - y.floor();
        let u = Self::suaviza(xf);
        let v = Self::suaviza(yf);

        let aa = self.p[self.p[xi] as usize + yi];
        let ab = self.p[self.p[xi] as usize + yi + 1];
        let ba = self.p[self.p[xi + 1] as usize + yi];
        let bb = self.p[self.p[xi + 1] as usize + yi + 1];

        let x1 = mistura(Self::grad(aa, xf, yf), Self::grad(ba, xf - 1.0, yf), u);
        let x2 = mistura(
            Self::grad(ab, xf, yf - 1.0),
            Self::grad(bb, xf - 1.0, yf - 1.0),
            u,
        );
        mistura(x1, x2, v)
    }

    /// Soma de oitavas: a forma geral vem das grandes, o detalhe das pequenas.
    pub fn fbm(&self, x: f32, y: f32, oitavas: u32, persistencia: f32) -> f32 {
        let (mut soma, mut amp, mut freq, mut norma) = (0.0, 1.0, 1.0, 0.0);
        for _ in 0..oitavas {
            soma += self.ruido(x * freq, y * freq) * amp;
            norma += amp;
            amp *= persistencia;
            freq *= 2.0;
        }
        soma / norma
    }

    /// Ruido em CRISTA: vira serra/espinhaco em vez de morro redondo.
    pub fn crista(&self, x: f32, y: f32, oitavas: u32) -> f32 {
        let (mut soma, mut amp, mut freq, mut norma) = (0.0, 1.0, 1.0, 0.0);
        for _ in 0..oitavas {
            let n = 1.0 - self.ruido(x * freq, y * freq).abs();
            soma += n * n * amp;
            norma += amp;
            amp *= 0.5;
            freq *= 2.1;
        }
        soma / norma
    }
}

fn mistura(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

/// fBm chega a ~±0,6; esticar pra 0..1 e' o que faz a serra e a costa
/// recortada existirem — sem isso os extremos nunca sao alcancados.
fn n01(f: f32) -> f32 {
    (0.5 + f * 0.95).clamp(0.0, 1.0)
}

fn suave(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// LCG minusculo pro sorteio do feitio. Mesma razao do Perlin: previsibilidade
/// entre maquinas vale mais que qualidade estatistica.
struct Rng(u32);

impl Rng {
    fn novo(semente: i32) -> Self {
        Self((semente as u32).wrapping_mul(2654435761).wrapping_add(1))
    }
    fn proximo(&mut self) -> u32 {
        self.0 = self.0.wrapping_mul(1664525).wrapping_add(1013904223);
        self.0
    }
    fn float(&mut self) -> f32 {
        (self.proximo() >> 8) as f32 / (1u32 << 24) as f32
    }
    fn inteiro(&mut self, ate: u32) -> u32 {
        self.proximo() % ate
    }
}

// ─────────────────────────────── bioma ───────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Bioma {
    Floresta,
    Gelo,
    Deserto,
    Montanha,
}

/// Os numeros que o bioma muda no relevo.
///
/// Um caminho de codigo so' pras quatro ilhas: o que separa duna de cordilheira
/// sao pesos, nao um gerador diferente. Gerador por bioma seria quatro vezes
/// mais codigo pra manter e quatro vezes mais lugares pra desandar.
pub struct PerfilDeRelevo {
    /// (base, amplitude) de cada tipo de terreno, em metros.
    pub planicie: (f32, f32),
    pub colina: (f32, f32),
    pub serra: (f32, f32),
    /// Cortes do seletor: `[fim da planicie, fim da mistura, fim da colina,
    /// fim da mistura]`. Depois do ultimo e' serra pura.
    pub corte: [f32; 4],
    /// Degrau do terraco, em BLOCOS inteiros. `0` desliga.
    ///
    /// Em blocos e nao em metros porque e' o bloco que o jogador sobe: com
    /// degrau de 1 o patamar inteiro e' andavel como escada; com 2 a borda
    /// so' passa pulando, e o patamar vira plato de verdade.
    pub terraco_blocos: i32,
    /// Quanto do relevo original o terraco engole, de 0 a 1. Fraco demais e o
    /// mundo vira rampa mansa de ponta a ponta — medido: 99,6% andavel e 75%
    /// plana, e mesmo assim nada que se leia como clareira.
    pub terraco_forca: f32,
}

impl Bioma {
    pub fn perfil(self) -> PerfilDeRelevo {
        match self {
            // O equilibrio do original: planicie pra andar, colina pra
            // esconder, serra pra ter horizonte.
            Bioma::Floresta => PerfilDeRelevo {
                planicie: (5.0, 4.0),
                colina: (8.0, 15.0),
                serra: (12.0, 30.0),
                corte: [0.42, 0.52, 0.66, 0.78],
                terraco_blocos: 4,
                terraco_forca: 0.92,
            },
            // Gelo e' chapada com pico seco: muita planicie, e quando sobe,
            // sobe de vez. O terraco largo le' como placa de gelo.
            Bioma::Gelo => PerfilDeRelevo {
                planicie: (4.0, 3.0),
                colina: (7.0, 12.0),
                serra: (12.0, 24.0),
                corte: [0.50, 0.58, 0.70, 0.80],
                terraco_blocos: 4,
                terraco_forca: 0.94,
            },
            // Duna: amplitude baixa, quase nunca serra e SEM terraco —
            // patamar em areia le' como erro de geracao, nao como relevo.
            Bioma::Deserto => PerfilDeRelevo {
                planicie: (3.0, 5.0),
                colina: (5.0, 9.0),
                serra: (7.0, 10.0),
                corte: [0.55, 0.70, 0.88, 0.95],
                // Duna nao tem patamar: o degrau curto e a forca baixa deixam
                // a areia ondular em vez de escadear.
                terraco_blocos: 2,
                terraco_forca: 0.45,
            },
            // Planalto: serra domina e o degrau e' grande o bastante pra
            // virar patamar de andar, que e' o que separa planalto de morro.
            Bioma::Montanha => PerfilDeRelevo {
                planicie: (8.0, 5.0),
                colina: (14.0, 18.0),
                serra: (18.0, 40.0),
                // A serra domina, mas nao sozinha: com o corte em 0,18 quase
                // metade da terra colapsava num nivel so' e a ilha virava uma
                // mesa — 46% das colunas na mesma faixa de altura.
                corte: [0.28, 0.38, 0.52, 0.64],
                // Degrau grande e' o que separa PLANALTO de morro, mas a
                // forca fica abaixo do gelo: 0,95 com degrau de 6 nivelava
                // tudo.
                terraco_blocos: 4,
                terraco_forca: 0.85,
            },
        }
    }
}

// ─────────────────────────────── feitio ──────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Feitio {
    Disco,
    Comprida,
    Enseada,
    Lobada,
    Garras,
    Atol,
}

/// O feitio ja' sorteado, com os numeros dele.
///
/// Vive fora do laco de proposito: `altura_do_relevo` roda uma vez por coluna
/// — dez milhoes de vezes na ilha grande — e sortear la' dentro seria sortear
/// dez milhoes de vezes a mesma coisa.
#[derive(Debug, Clone, Copy)]
pub struct Forma {
    pub tipo: Feitio,
    pub ang: f32,
    pub a: f32,
    pub b: f32,
    /// A ENSEADA: o pedaco plano onde a vila cabe. Ilha grande sempre tem.
    pub enseada: Option<(f32, f32, f32)>,
}

impl Forma {
    pub const NIVEL_DA_ENSEADA: f32 = 3.6;

    pub fn de(semente: i32, raio_blocos: i32) -> Self {
        // Ilha grande fica REDONDA e ganha enseada. Cidade quer costa larga:
        // com feitio marcante, metade das docas nao alcanca agua e o resto do
        // assentamento vai junto.
        if raio_blocos > 600 {
            let mut re = Rng::novo(semente ^ 0x0E45EA);
            let ang = re.float() * std::f32::consts::TAU;
            let d = raio_blocos as f32 * 0.66;
            let r = raio_blocos as f32 * 0.34;
            return Self {
                tipo: Feitio::Disco,
                ang: 0.0,
                a: 0.0,
                b: 0.0,
                enseada: Some((ang.sin() * d, ang.cos() * d, r)),
            };
        }

        let mut r = Rng::novo(semente ^ 0x0F117A);
        let ang = r.float() * std::f32::consts::TAU;
        let d = r.inteiro(100);
        let sem_enseada = |tipo, a, b| Self { tipo, ang, a, b, enseada: None };

        // O DISCO leva a maior fatia: se toda ilha tivesse feitio marcante, o
        // marcante viraria o normal e nada se destacaria.
        if d < 34 {
            sem_enseada(Feitio::Disco, 0.0, 0.0)
        } else if d < 51 {
            sem_enseada(Feitio::Comprida, 0.98, 0.40 + r.float() * 0.08)
        } else if d < 68 {
            sem_enseada(Feitio::Enseada, 0.62 + r.float() * 0.16, 0.50 + r.float() * 0.14)
        } else if d < 82 {
            sem_enseada(Feitio::Lobada, 0.42 + r.float() * 0.04, 0.58)
        } else {
            // 3 a 5 bracos. Com 6+ o contorno vira engrenagem e deixa de ler
            // como ilha.
            sem_enseada(Feitio::Garras, 3.0 + r.inteiro(3) as f32, 0.44)
        }
    }

    /// Distancia normalizada que a mascara enxerga: 0 no miolo, 1 na beira.
    /// Todo feitio e' uma conta diferente PARA ESTE NUMERO — e e' so' isso.
    pub fn dist(&self, bx: f32, bz: f32, r: f32) -> f32 {
        let d = (bx * bx + bz * bz).sqrt() / r;
        if self.tipo == Feitio::Disco {
            return d;
        }
        let (ca, sa) = (self.ang.cos(), self.ang.sin());
        let u = (bx * ca + bz * sa) / r;
        let v = (-bx * sa + bz * ca) / r;
        match self.tipo {
            Feitio::Comprida => {
                (u * u / (self.a * self.a) + v * v / (self.b * self.b)).sqrt()
            }
            Feitio::Enseada => {
                // A mordida e' uma ELIPSE funda e estreita encostada num lado.
                // Redonda ela comeria um setor inteiro e sobraria uma cunha.
                let mx = (u - self.a) / 1.25;
                let mz = v / 0.52;
                let db = (mx * mx + mz * mz).sqrt() / self.b;
                d + suave((1.0 - db) / 0.30) * 1.3
            }
            Feitio::Lobada => {
                let d1 = ((u - self.a) * (u - self.a) + v * v).sqrt() / self.b;
                let d2 = ((u + self.a) * (u + self.a) + v * v).sqrt() / self.b;
                d1.min(d2)
            }
            Feitio::Garras => {
                let th = v.atan2(u);
                d / (1.0 - self.b + self.b * (0.5 + 0.5 * (self.a * th).cos()))
            }
            Feitio::Atol => d + suave((self.a - d) / (self.a * 0.35)) * 1.3,
            Feitio::Disco => d,
        }
    }

    fn peso_da_enseada(&self, bx: f32, bz: f32) -> f32 {
        let Some((ex, ez, raio)) = self.enseada else { return 0.0 };
        let d = ((bx - ex).powi(2) + (bz - ez).powi(2)).sqrt() / raio;
        if d >= 1.0 {
            return 0.0;
        }
        suave(1.0 - ((d - 0.62) / 0.38).clamp(0.0, 1.0))
    }
}

// ─────────────────────────────── relevo ──────────────────────────────

const REBAIXO_DA_BORDA: f32 = 2.2;
const BASE_DA_BORDA: f32 = 16.0;
const QUEDA_DA_BORDA: f32 = 53.8;
const ESCALA_DA_QUEDA: f32 = 42.0;

/// Quanto do relevo acima do joelho a ilha leva. Ilha pequena com a serra da
/// grande vira pao de acucar com praia: parede de escalar, nada de andar.
fn altura_do_porte(r: i32) -> f32 {
    0.45 + 0.55 * ((r as f32 - 45.0) / (300.0 - 45.0)).clamp(0.0, 1.0)
}

/// A altura crua do relevo, em metros, na coluna `(bx, bz)` medida em blocos
/// a partir do centro da ilha.
pub fn altura_do_relevo(
    p: &Perlin,
    pw: &Perlin,
    pctrl: &Perlin,
    r: i32,
    bx: f32,
    bz: f32,
    forma: &Forma,
    perfil: &PerfilDeRelevo,
) -> f32 {
    const ESC: f32 = 1.0 / 150.0;
    let (mx, mz) = (bx * BLOCO, bz * BLOCO);
    let d = forma.dist(bx, bz, r as f32);

    // Deformacao de dominio grande: borda recortada, nao circulo.
    let wx = pw.fbm(mx * ESC * 1.3, mz * ESC * 1.3, 3, 0.5) * 55.0;
    let wz = pw.fbm(mx * ESC * 1.3 + 5.2, mz * ESC * 1.3 - 3.1, 3, 0.5) * 55.0;
    let (sx, sz) = ((mx + wx) * ESC, (mz + wz) * ESC);

    // A MASCARA PRIMEIRO. Ela vinha depois, e por isso o relevo era calculado
    // pro mundo inteiro — doze oitavas por ponto — pra depois ser multiplicado
    // por zero fora do disco. Perguntar antes corta a conta pela metade.
    let raio_r = 0.74 + 0.50 * n01(pw.fbm(sx * 1.6, sz * 1.6, 3, 0.5));
    let t0 = ((d / raio_r - 0.60) / 0.40).clamp(0.0, 1.0);
    let mascara = 1.0 - t0 * t0 * (3.0 - 2.0 * t0);

    let mut h = 0.0;
    if mascara > 0.0 {
        let planicie = perfil.planicie.0
            + perfil.planicie.1 * n01(p.fbm(sx * 0.9, sz * 0.9, 2, 0.5));
        let colina = perfil.colina.0
            + perfil.colina.1 * n01(p.fbm(sx * 1.5 + 7.0, sz * 1.5 - 4.0, 4, 0.45));
        let serra =
            perfil.serra.0 + perfil.serra.1 * p.crista(sx * 1.2 + 19.0, sz * 1.2 + 31.0, 4);

        // Controle largo com transicao suave. O fBm normalizado nao chega
        // perto de ±1, entao ele e' ESTICADO — sem isso a serra nunca sai.
        let c = n01(pctrl.fbm(sx * 1.6 - 12.0, sz * 1.6 + 8.0, 2, 0.5));
        let [c0, c1, c2, c3] = perfil.corte;
        h = if c < c0 {
            planicie
        } else if c < c1 {
            mistura(planicie, colina, (c - c0) / (c1 - c0))
        } else if c < c2 {
            colina
        } else if c < c3 {
            mistura(colina, serra, (c - c2) / (c3 - c2))
        } else {
            serra
        };
    }

    h = h * mascara - REBAIXO_DA_BORDA - (1.0 - mascara) * BASE_DA_BORDA;

    let dm = (d - raio_r) * r as f32 * BLOCO;
    let fora;
    if dm > 0.0 {
        fora = 1.0 - (-dm / ESCALA_DA_QUEDA).exp();
        let mut queda = QUEDA_DA_BORDA * fora;
        queda += p.fbm(sx * 2.0 - 44.0, sz * 2.0 + 17.0, 3, 0.5)
            * 6.5
            * (dm / 25.0).clamp(0.0, 1.0)
            * (1.0 - fora);
        h -= queda;
    } else {
        fora = 0.0;
    }
    let _ = fora;

    // O terraco NAO mora aqui. Ele opera em blocos inteiros, e bloco so'
    // existe depois da escala de altura — ver `terracear`, chamado em
    // `Ilha::gerar`. Aplicado em metros crus, o degrau caia em fracao de
    // bloco e o arredondamento final desmanchava o patamar.

    // A faixa da praia fica rasa: sem isso a costa e' um degrau.
    if h > -1.0 && h < 2.2 {
        h *= 0.45;
    }

    // A ENSEADA vem no fim, e quem segura a costa e' a propria ALTURA, nao a
    // mascara — o peso desliga conforme o chao se aproxima da agua, entao
    // terreno submerso nunca sobe e a beira termina em praia.
    let peso_e = forma.peso_da_enseada(bx, bz) * (h / 4.5).clamp(0.0, 1.0);
    if peso_e > 0.001 {
        let respiro = p.fbm(sx * 2.2 + 61.0, sz * 2.2 - 29.0, 2, 0.5) * 0.45;
        h = mistura(h, Forma::NIVEL_DA_ENSEADA + respiro, peso_e);
    }

    // A ILHA PEQUENA NAO TEM CORDILHEIRA — e isto vem no fim, e so' acima do
    // joelho: comprimir antes da mascara encolheria a ILHA, movendo a linha
    // da costa pra dentro. Aqui a silhueta e' a mesma; so' os picos descem.
    const PISO_DO_APLAINO: f32 = 3.0;
    if h > PISO_DO_APLAINO {
        h = PISO_DO_APLAINO + (h - PISO_DO_APLAINO) * altura_do_porte(r);
    }
    h
}

/// Colapsa a altura em patamares de `passo` blocos, com uma rampa curta na
/// transicao.
///
/// E' isto que da' CONTRASTE: sem terraco o relevo e' uma rampa mansa em que
/// tudo e' meio-plano e nada e' claramente chao. Com ele, clareira e' chapada
/// e encosta e' encosta — que e' o que decide onde mob e chefe cabem.
///
/// A rampa ocupa os ultimos 40% do degrau; o resto e' patamar. Sem ela o
/// mundo vira escadaria de bolo de noiva.
fn terracear(h_blocos: f32, passo: i32, forca: f32, jitter: f32) -> f32 {
    if passo <= 0 || forca <= 0.0 {
        return h_blocos;
    }
    let t = passo as f32;
    let tt = (h_blocos + jitter) / t;
    let chao = tt.floor();
    let degrau = (chao + ((tt - chao - 0.60) / 0.40).clamp(0.0, 1.0)) * t;
    mistura(h_blocos, degrau, forca)
}

/// Do que a coluna e' feita. Material discreto e nao gradiente: rampa
/// continua de um tom so' vira mancha, e o que faz terreno voxel se ler sao
/// faixas com borda — areia, grama, rocha, neve.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Material {
    Agua,
    AreiaMolhada,
    Areia,
    Grama,
    /// Tons vizinhos da grama. Chao de um verde so' le' como feltro; e' a
    /// MANCHA de tom, em baixa frequencia, que faz parecer campo.
    GramaClara,
    GramaEscura,
    Terra,
    Rocha,
    /// Rocha sedimentar do deserto: a mesa. Sem ela o Ermo e' um lencol de
    /// areia de ponta a ponta — medido no mapa, um tom so' em 100% da terra.
    Arenito,
    Neve,
    Gelo,
    Tronco,
    /// Pedra mais escura: matacao de um tom so' le' como bola de papel.
    RochaEscura,
    /// Fruto do arbusto. So' 35% dos arbustos tem — se todo arbusto tivesse,
    /// deixaria de significar alguma coisa.
    Fruto,
    /// Petalas. Cor SATURADA de proposito: flor em tom de areia ou folha seca
    /// desaparece no verde, e ai' plantar flor nao adianta nada.
    PetalaVermelha,
    PetalaAmarela,
    PetalaRoxa,
    PetalaBranca,
    Folha,
    FolhaEscura,
    FolhaSeca,
}

impl Material {
    /// Todos, na ordem em que foram declarados.
    ///
    /// Existe pra guardar material num `u8` e ler de volta sem tabela
    /// paralela. Ja' custou caro: uma tabela com subconjunto dos materiais fez
    /// `Tronco` (discriminante 11) cair fora dela, e todo tronco e toda folha
    /// da vegetacao viraram voxel INVISIVEL — solido pra colisao de face,
    /// vazio pra malha. O mundo ficou coberto de pedra e mais nada.
    pub const TODOS: [Material; 21] = [
        Material::Agua,
        Material::AreiaMolhada,
        Material::Areia,
        Material::Grama,
        Material::GramaClara,
        Material::GramaEscura,
        Material::Terra,
        Material::Rocha,
        Material::Arenito,
        Material::Neve,
        Material::Gelo,
        Material::Tronco,
        Material::RochaEscura,
        Material::Fruto,
        Material::PetalaVermelha,
        Material::PetalaAmarela,
        Material::PetalaRoxa,
        Material::PetalaBranca,
        Material::Folha,
        Material::FolhaEscura,
        Material::FolhaSeca,
    ];

    pub fn de_u8(v: u8) -> Option<Material> {
        Self::TODOS.get(v as usize).copied()
    }

    /// Paleta emprestada do Sea of Cubes, que ja' passou pelo olho.
    pub fn rgb(self) -> (u8, u8, u8) {
        match self {
            Material::Agua => (28, 62, 104),
            Material::AreiaMolhada => (196, 178, 130),
            Material::Areia => (232, 214, 158),
            Material::Grama => (104, 168, 76),
            Material::GramaClara => (126, 186, 88),
            Material::GramaEscura => (80, 138, 62),
            Material::Terra => (122, 92, 62),
            Material::Rocha => (136, 132, 126),
            Material::Arenito => (186, 126, 84),
            Material::Tronco => (96, 70, 46),
            Material::RochaEscura => (104, 100, 96),
            Material::Fruto => (198, 62, 58),
            Material::PetalaVermelha => (216, 76, 84),
            Material::PetalaAmarela => (238, 202, 86),
            Material::PetalaRoxa => (152, 104, 200),
            Material::PetalaBranca => (240, 242, 248),
            Material::Folha => (74, 138, 58),
            Material::FolhaEscura => (48, 104, 52),
            Material::FolhaSeca => (140, 138, 70),
            Material::Neve => (238, 244, 250),
            // Mais AZUL e mais escuro que a neve: os dois aparecem lado a
            // lado na mesma ilha, e neve e gelo do mesmo branco seriam uma
            // ilha de um material so'.
            Material::Gelo => (176, 214, 238),
        }
    }

    /// O que aparece na LATERAL de um bloco deste material.
    ///
    /// Lateral de grama nao e' verde — e' a terra debaixo dela. Pintar o lado
    /// com a cor do topo e' o que fazia o mundo inteiro parecer um borrao de
    /// uma cor so'.
    pub fn lateral(self) -> Material {
        match self {
            Material::Grama | Material::GramaClara | Material::GramaEscura => Material::Terra,
            Material::Areia | Material::AreiaMolhada => Material::Areia,
            Material::Neve => Material::Rocha,
            Material::Arenito => Material::Arenito,
            outro => outro,
        }
    }
}


/// Material do TOPO de uma coluna.
///
/// Faixa de altura por bioma, mais uma regra de declive — e e' o declive que
/// muda tudo. Terreno pintado so' por altura vira mancha de um tom so'; com
/// "encosta ingreme e' rocha", o penhasco aparece cinza contra o verde e o
/// relevo passa a se ler de longe.
///
/// Mora aqui e nao no cliente porque e' DADO DE MUNDO: o mapa do lobby, o
/// editor e o jogo tem que pintar a mesma ilha da mesma cor.
pub fn material(bioma: Bioma, altura: f32, declive: i32, agua: bool) -> Material {
    material_variado(bioma, altura, declive, agua, 0.5)
}

/// O mesmo, com a MANCHA de terreno: uma variacao de baixa frequencia que
/// troca o tom do chao em regioes largas.
///
/// Sem ela o campo e' um verde chapado de horizonte a horizonte. Com ela
/// aparecem clareiras mais claras, sombras de mata mais escuras e uma ou outra
/// falha de terra batida — e nada disso custa geometria, so' a cor do quad que
/// ja' ia ser desenhado.
pub fn material_variado(
    bioma: Bioma,
    altura: f32,
    declive: i32,
    agua: bool,
    mancha: f32,
) -> Material {
    if agua {
        return Material::Agua;
    }
    // Rocha exposta pelo proprio declive: onde o degrau passa de um bloco o
    // solo nao segura, e e' assim que serra parece serra.
    if declive >= 3 && altura > 3.0 {
        // Cinza (ou arenito) marca ONDE NAO SE SOBE: o degrau passou do pulo.
        // Nao e' decoracao — e' a regra de movimento pintada no chao, e o
        // jogador aprende a ler o mapa sem nenhum texto.
        return if bioma == Bioma::Deserto { Material::Arenito } else { Material::Rocha };
    }
    match bioma {
        Bioma::Floresta => {
            if altura < 1.0 { Material::AreiaMolhada }
            else if altura < 2.6 { Material::Areia }
            else if altura < 24.0 { grama(mancha) }
            else if altura < 31.0 { Material::Rocha }
            else { Material::Neve }
        }
        // Nenhuma faixa de grama em lugar nenhum: e' essa AUSENCIA que se
        // reconhece de longe.
        Bioma::Gelo => {
            if altura < 2.6 { Material::Gelo }
            else if altura < 30.0 { Material::Neve }
            else { Material::Rocha }
        }
        // Duna nao tem faixa alta: o Ermo so' se le' se a parte de cima virar
        // MESA. Areia ate' a meia altura, arenito acima, rocha no topo.
        Bioma::Deserto => {
            if altura < 1.0 { Material::AreiaMolhada }
            else if altura < 5.0 { Material::Areia }
            else if altura < 9.5 { Material::Arenito }
            else { Material::Rocha }
        }
        // A grama para na metade da altura: o que sobra e' paredao. Ilha de
        // rocha com vale verde no pe', que e' o que uma serra e' vista de
        // baixo.
        Bioma::Montanha => {
            if altura < 1.0 { Material::AreiaMolhada }
            else if altura < 2.6 { Material::Areia }
            else if altura < 14.0 { grama(mancha) }
            else if altura < 30.0 { Material::Rocha }
            else { Material::Neve }
        }
    }
}

/// O que aparece na PAREDE, `prof` blocos abaixo do topo.
///
/// A coluna e' estratificada, e e' isso que faz barranco ler como barranco:
/// grama em cima, uma faixa de TERRA logo abaixo, e ROCHA a partir do
/// terceiro bloco. Parede de um material so' — que era o que eu tinha — le'
/// como parede pintada.
///
/// O subsolo tambem muda com o clima: areia com terra marrom logo abaixo e'
/// praia, nao deserto; um bloco de profundidade ja' denuncia.
pub fn material_de_profundidade(bioma: Bioma, topo: Material, altura: f32, prof: i32) -> Material {
    if prof >= 3 {
        return Material::Rocha;
    }
    if prof == 0 {
        return topo;
    }
    match bioma {
        Bioma::Deserto => Material::Areia,
        Bioma::Gelo => if altura < 3.0 { Material::Gelo } else { Material::Neve },
        _ => if altura < 3.0 { Material::Areia } else { Material::Terra },
    }
}

/// Grama, ou terra batida nas pontas da mancha.
///
/// UM material so'. A primeira versao escolhia entre tres — grama escura,
/// grama, grama clara — e o resultado lia como mancha de sujeira: a transicao
/// entre dois tons chapados e' um degrau seco, e o olho ve' a borda antes de
/// ver a variacao. Quem varia o tom agora e' `tom_da_mancha`, de forma
/// continua.
///
/// A falha de solo fica: terra batida e' feicao de campo, nao ruido de cor.
fn grama(mancha: f32) -> Material {
    if mancha < 0.07 {
        Material::Terra
    } else {
        Material::Grama
    }
}

/// Quanto a mancha clareia ou escurece o chao, como fator sobre a cor.
///
/// Continuo e de amplitude curta: a variacao tem que dar a impressao de
/// clareira e sombra de mata sem que se enxergue onde uma acaba e a outra
/// comeca.
pub fn tom_da_mancha(mancha: f32) -> f32 {
    0.88 + mancha.clamp(0.0, 1.0) * 0.26
}

/// Especie de arvore.
///
/// Sempre com MISTURA, nunca uma so' por bioma: floresta de espécie unica le'
/// como papel de parede — e vale ainda mais onde ha' poucas arvores, porque
/// la' cada uma se ve' inteira.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Arvore {
    /// Copa larga e redonda.
    Copada,
    /// Tronco fino e alto, copa pequena.
    Betula,
    /// Conica, em camadas.
    Pinheiro,
    /// Sem folha: o que sobrou de quando aquilo ali era outra coisa.
    Seca,
}

/// Forracao: o que cobre o chao entre as arvores.
///
/// E' esta camada que separa "campo de golfe com arvore" de mundo. Planta e'
/// pequena e barata, entao a densidade e' varias vezes a de arvore.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Planta {
    /// Tufo de capim.
    Moita,
    /// Hastes com cabeca colorida.
    Flor,
    /// Moita fechada, na altura do joelho.
    Arbusto,
    /// Fronde que arqueia pra fora.
    Samambaia,
    /// Matacao pequeno.
    Pedra,
    /// Toco de arvore cortada.
    Toco,
    /// Talo alto e palido de campo aberto.
    Talo,
}

/// Plantas por 100 m². Varias vezes a densidade de arvore — planta e' pequena
/// e barata, e e' a quantidade que faz o chao existir.
pub fn densidade_de_planta(bioma: Bioma) -> f32 {
    match bioma {
        // Mais densa que o Sea of Cubes de proposito: a camera de cima ve'
        // uma area muito maior que a de terceira pessoa, entao a mesma
        // densidade por metro quadrado le' como chao pelado na tela.
        Bioma::Floresta => 8.0,
        // No gelo quase nada vinga: e' o vazio que faz a ilha ler como fim de
        // mundo, e e' ele que deixa o pinheiro solitario a' vista.
        Bioma::Gelo => 1.2,
        // O deserto nao e' pelado: ele e' de ARBUSTO e TALO, sem capim nem
        // flor. Densidade media, vocabulario curto.
        Bioma::Deserto => 3.4,
        // E nela o que forra e' PEDRA: cordilheira forrada de flor seria serra
        // com nome bonito.
        Bioma::Montanha => 5.5,
    }
}

/// A planta deste sorteio. `f` e' 0..1.
///
/// Cada bioma tem VOCABULARIO PROPRIO, e os curtos sao de proposito: menos
/// palavras, mais identidade. Gelo sem flor e sem capim se reconhece de longe
/// justamente pela ausencia.
pub fn especie_de_planta(bioma: Bioma, f: f32) -> Planta {
    match bioma {
        Bioma::Floresta => {
            if f < 0.28 { Planta::Moita }
            // Flor com peso alto: no verde ela some, entao "algumas flores"
            // por metro quadrado vira nenhuma flor na tela.
            else if f < 0.52 { Planta::Flor }
            else if f < 0.72 { Planta::Samambaia }
            else if f < 0.88 { Planta::Arbusto }
            else if f < 0.96 { Planta::Pedra }
            else { Planta::Toco }
        }
        Bioma::Gelo => {
            if f < 0.62 { Planta::Pedra }
            else if f < 0.90 { Planta::Arbusto }
            else { Planta::Moita }
        }
        Bioma::Deserto => {
            if f < 0.46 { Planta::Talo }
            else if f < 0.78 { Planta::Arbusto }
            else { Planta::Pedra }
        }
        Bioma::Montanha => {
            if f < 0.70 { Planta::Pedra }
            else if f < 0.88 { Planta::Moita }
            else { Planta::Arbusto }
        }
    }
}

/// Arvores por 100 m² de terra.
///
/// E' o que separa mata fechada de campo aberto, e o que decide o custo em
/// tela. Numeros na escala do Sea of Cubes.
pub fn densidade_de_arvore(bioma: Bioma) -> f32 {
    match bioma {
        Bioma::Floresta => 1.2,
        // No gelo quase nada vinga, e e' o vazio que faz a ilha ler como fim
        // de mundo — e que deixa o pinheiro solitario a' vista.
        Bioma::Gelo => 0.30,
        // No deserto arvore e' noticia: um punhado por ilha, nas grotas.
        Bioma::Deserto => 0.05,
        // Mata nos vales e paredao pelado em cima: a media fica no meio.
        Bioma::Montanha => 0.7,
    }
}

/// A especie que sai deste sorteio. `f` e' 0..1.
pub fn especie_de_arvore(bioma: Bioma, f: f32) -> Arvore {
    match bioma {
        Bioma::Floresta => {
            if f < 0.62 { Arvore::Copada }
            else if f < 0.88 { Arvore::Betula }
            else if f < 0.97 { Arvore::Pinheiro }
            else { Arvore::Seca }
        }
        Bioma::Gelo => if f < 0.90 { Arvore::Pinheiro } else { Arvore::Seca },
        Bioma::Deserto => if f < 0.85 { Arvore::Seca } else { Arvore::Copada },
        Bioma::Montanha => {
            if f < 0.82 { Arvore::Pinheiro }
            else if f < 0.95 { Arvore::Betula }
            else { Arvore::Seca }
        }
    }
}

// ────────────────────────────── o gerador ────────────────────────────

/// Relevo COLUNA A COLUNA, sem precisar da ilha inteira.
///
/// O servidor gera tudo de uma vez porque precisa do mapa todo pra decidir
/// spawn e caminho. O cliente nao: ele desenha o que cabe na tela, e gerar
/// 10,2 milhoes de colunas pra mostrar mil e' 3,3 s de espera por nada.
///
/// Os dois passam pelo MESMO `bloco_em`. E' isso que garante que o chao que o
/// jogador ve' e o chao em que o servidor o poe sao o mesmo — nao por
/// disciplina, por construcao.
pub struct Gerador {
    p: Perlin,
    pw: Perlin,
    pctrl: Perlin,
    forma: Forma,
    perfil: PerfilDeRelevo,
    pub raio_blocos: i32,
    pub escala_altura: f32,
    terraco_blocos: i32,
    terraco_forca: f32,
}

impl Gerador {
    pub fn novo(semente: i32, raio_blocos: i32, bioma: Bioma, escala_altura: f32) -> Self {
        let p = bioma.perfil();
        let (tb, tf) = (p.terraco_blocos, p.terraco_forca);
        Self::com_terraco(semente, raio_blocos, bioma, escala_altura, tb, tf)
    }

    pub fn com_terraco(
        semente: i32,
        raio_blocos: i32,
        bioma: Bioma,
        escala_altura: f32,
        terraco_blocos: i32,
        terraco_forca: f32,
    ) -> Self {
        Self {
            p: Perlin::novo(semente),
            pw: Perlin::novo(semente ^ 0x5f37_59df),
            pctrl: Perlin::novo(semente ^ 0x1b87_3593),
            forma: Forma::de(semente, raio_blocos),
            perfil: bioma.perfil(),
            raio_blocos,
            escala_altura,
            terraco_blocos,
            terraco_forca,
        }
    }

    /// Indice do bloco de topo na coluna `(bx, bz)`, em blocos a partir do
    /// CENTRO da ilha.
    pub fn bloco_em(&self, bx: i32, bz: i32) -> i32 {
        let h = altura_do_relevo(
            &self.p,
            &self.pw,
            &self.pctrl,
            self.raio_blocos,
            bx as f32,
            bz as f32,
            &self.forma,
            &self.perfil,
        ) * self.escala_altura;
        let mut hb = h / BLOCO;
        // So' terraceia terra: patamar embaixo d'agua nao aparece e ainda
        // escadeia o talude que a queda de borda desenhou.
        if hb > 0.0 {
            let jit = self
                .p
                .fbm(bx as f32 * 0.012 + 21.0, bz as f32 * 0.012 - 13.0, 3, 0.5)
                * 0.55;
            hb = terracear(hb, self.terraco_blocos, self.terraco_forca, jit);
        }
        hb.round().clamp(i16::MIN as f32, i16::MAX as f32) as i32
    }

    /// Mancha de terreno em `(bx, bz)`, 0..1. Baixa frequencia: as regioes
    /// tem dezenas de metros, nao centimetros.
    pub fn mancha(&self, bx: i32, bz: i32) -> f32 {
        (0.5 + self.p.fbm(bx as f32 * 0.0075 + 313.0, bz as f32 * 0.0075 - 77.0, 3, 0.5) * 1.1)
            .clamp(0.0, 1.0)
    }

    /// Altura do chao em unidades de mundo, na coordenada de mundo `(x, z)`.
    pub fn altura(&self, x: f32, z: f32) -> f32 {
        let bx = (x / BLOCO).round() as i32;
        let bz = (z / BLOCO).round() as i32;
        (self.bloco_em(bx, bz) + 1) as f32 * BLOCO
    }
}

// ─────────────────────────────── a ilha ──────────────────────────────

/// Quanto do relevo cru sobra.
///
/// Escolhido medindo, nao no olho. A 0,35 o mundo tinha 13,5 unidades de
/// desnivel em 1,6 km — 2% de inclinacao, 99,6% andavel, 0,0% de parede: tudo
/// meio-plano e nada claramente chao nem claramente encosta. A 0,90 com
/// terraco de 4 blocos o mapa fica com 66% plano, 86% andavel e **5,9% de
/// parede**, que e' o que faz vale, passagem e plato existirem.
///
/// ```text
/// escala  terraco   pico    plana  andavel  parede  sitio chefe
///   0,35      6bl  12,5un   81,0%    97,8%    0,3%        11,2%
///   0,60      6bl  23,0un   74,1%    93,6%    1,9%         7,9%
///   0,90      4bl  34,5un   66,3%    86,5%    5,9%         6,0%
///   1,20      4bl  46,5un   60,6%    81,2%    9,4%         5,3%
/// ```
///
/// 6% de sitio de chefe numa ilha de 1,16 km² ainda sao ~70 mil m² de arena:
/// area plana nunca foi o recurso escasso, contraste era.
pub const ESCALA_ALTURA: f32 = 0.90;

/// Uma ilha do lado do SERVIDOR: campo de altura e mais nada.
///
/// Dois bytes por coluna. O volume voxel — pedra, grama, neve, o que o olho
/// ve' — e' o cliente que gera, da mesma semente, so' em volta do jogador.
/// A conta que justifica isso: 1 km² sao ~16 MB aqui contra ~416 MB de volume
/// cheio, e o servidor roda onze canais na mesma maquina.
pub struct Ilha {
    pub semente: i32,
    pub raio_blocos: i32,
    pub bioma: Bioma,
    pub escala_altura: f32,
    /// Colunas por lado (`raio * 2`).
    pub lado: usize,
    /// Topo de cada coluna, em INDICE DE BLOCO. O mundo e' voxel: a
    /// superficie esta' num bloco inteiro, nao num float. Guardar contínuo
    /// era o erro que tornava "um bloco de degrau" uma frase sem sentido —
    /// e a colisao passaria a discordar do que o olho ve'.
    blocos: Vec<i16>,
}

/// Cabecalho do arquivo de altura. Se qualquer um destes mudar, o cache e'
/// descartado e a ilha e' gerada de novo — arquivo de uma semente servindo
/// como se fosse de outra e' o tipo de bug que so' aparece em producao.
const MAGICA: [u8; 4] = *b"TALT";
const VERSAO: u16 = 1;

impl Ilha {
    pub fn gerar(semente: i32, raio_blocos: i32, bioma: Bioma, escala_altura: f32) -> Self {
        let p = bioma.perfil();
        Self::com_terraco(semente, raio_blocos, bioma, escala_altura, p.terraco_blocos, p.terraco_forca)
    }

    /// Mesma geracao, com o terraco por fora — e' assim que o `terreno --varre`
    /// acha o degrau certo sem recompilar a cada tentativa.
    pub fn com_terraco(
        semente: i32,
        raio_blocos: i32,
        bioma: Bioma,
        escala_altura: f32,
        terraco_blocos: i32,
        terraco_forca: f32,
    ) -> Self {
        let lado = (raio_blocos * 2) as usize;
        let ger = Gerador::com_terraco(
            semente, raio_blocos, bioma, escala_altura, terraco_blocos, terraco_forca,
        );
        let mut blocos = vec![0i16; lado * lado];
        for iz in 0..lado {
            let bz = iz as i32 - raio_blocos;
            for ix in 0..lado {
                let bx = ix as i32 - raio_blocos;
                blocos[iz * lado + ix] = ger.bloco_em(bx, bz) as i16;
            }
        }
        Self { semente, raio_blocos, bioma, escala_altura, lado, blocos }
    }

    /// Indice do bloco de topo. Fora da grade e' fundo: dado de indice nunca
    /// le' as cegas, e "fora da ilha" e' mar aberto.
    pub fn bloco(&self, ix: i32, iz: i32) -> i32 {
        if ix < 0 || iz < 0 || ix as usize >= self.lado || iz as usize >= self.lado {
            return -80;
        }
        self.blocos[iz as usize * self.lado + ix as usize] as i32
    }

    /// Coluna sob uma coordenada de mundo, com o centro da ilha em `(0, 0)`.
    pub fn coluna(&self, x: f32, z: f32) -> (i32, i32) {
        (
            (x / BLOCO).round() as i32 + self.raio_blocos,
            (z / BLOCO).round() as i32 + self.raio_blocos,
        )
    }

    /// Altura do chao em unidades de mundo. E' esta funcao que o cliente usa
    /// pra saber o Y do jogador — e por isso o Y nao viaja no snapshot.
    ///
    /// Sem interpolacao de proposito: o topo de um bloco e' plano, e suavizar
    /// faria a colisao discordar do que o olho ve'.
    pub fn altura(&self, x: f32, z: f32) -> f32 {
        let (ix, iz) = self.coluna(x, z);
        (self.bloco(ix, iz) + 1) as f32 * BLOCO
    }

    pub fn agua(&self, x: f32, z: f32) -> bool {
        let (ix, iz) = self.coluna(x, z);
        (self.bloco(ix, iz) + 1) as f32 * BLOCO <= NIVEL_DO_MAR
    }

    /// Da' pra ANDAR de um ponto ao outro? Um bloco de subida passa como
    /// escada; mais que isso, nao.
    ///
    /// Nao existe malha de navegacao nem colisao 3D: e' esta comparacao de
    /// inteiros que faz penhasco virar parede. Descer e' livre — cair de um
    /// barranco e' movimento valido, ficar preso em cima dele nao.
    pub fn passo_ok(&self, de: (f32, f32), para: (f32, f32)) -> bool {
        self.subida(de, para).map_or(false, |s| s <= DEGRAU_BLOCOS)
    }

    /// Da' pra chegar PULANDO? Dois blocos sim, tres nao — nao ha' escalada.
    pub fn pulo_ok(&self, de: (f32, f32), para: (f32, f32)) -> bool {
        self.subida(de, para).map_or(false, |s| s <= PULO_BLOCOS)
    }

    /// Blocos de subida entre duas colunas. `None` = destino na agua.
    fn subida(&self, de: (f32, f32), para: (f32, f32)) -> Option<i32> {
        if self.agua(para.0, para.1) {
            return None;
        }
        let (ax, az) = self.coluna(de.0, de.1);
        let (bx, bz) = self.coluna(para.0, para.1);
        Some(self.bloco(bx, bz) - self.bloco(ax, az))
    }

    /// Move e desliza contra o relevo.
    ///
    /// Mesma forma do `WorldMap::move_and_slide` que ela substitui — tenta um
    /// eixo de cada vez, e o que barra num eixo continua andando no outro.
    /// Sem isso o jogador GRUDA na encosta em vez de deslizar por ela, e a
    /// diferenca entre as duas coisas e' a diferenca entre andar e brigar com
    /// o controle.
    ///
    /// A parede nao e' geometria: e' a regra de degrau. Subir mais que um
    /// bloco simplesmente nao acontece.
    pub fn mover_e_deslizar(
        &self,
        pos: glam::Vec2,
        vel: glam::Vec2,
        dt: f32,
        raio: f32,
    ) -> glam::Vec2 {
        // Amostra o DESTINO, nao o caminho: um passo maior que um bloco
        // atravessaria uma parede de uma coluna so'. A 30Hz e velocidade de
        // jogador o passo e' ~0,13 de unidade contra blocos de 0,5, entao
        // sobra folga de quase quatro vezes — se um dia houver corrida ou
        // arranco, isto vira varredura.
        let v = vel * dt;
        let mut p = pos;
        if v.x != 0.0 {
            let alvo = glam::Vec2::new(p.x + v.x, p.y);
            if self.borda_livre(p, alvo, raio, glam::Vec2::new(v.x.signum(), 0.0)) {
                p.x = alvo.x;
            }
        }
        if v.y != 0.0 {
            let alvo = glam::Vec2::new(p.x, p.y + v.y);
            if self.borda_livre(p, alvo, raio, glam::Vec2::new(0.0, v.y.signum())) {
                p.y = alvo.y;
            }
        }
        p
    }

    /// A BORDA DA FRENTE cabe, vindo de `de`?
    ///
    /// So' a borda no sentido do movimento, e nao o corpo inteiro. Testar os
    /// quatro lados parece mais seguro e trava o jogador: encostado num
    /// paredao, a amostra do lado da parede reprova TODA direcao — inclusive
    /// a de se afastar dela. Medido antes da correcao: 17 unidades andadas em
    /// vinte mil passos.
    ///
    /// Tres pontos e nao um: com so' o centro, o ombro entra no barranco
    /// antes de o passo ser reprovado.
    fn borda_livre(
        &self,
        de: glam::Vec2,
        para: glam::Vec2,
        raio: f32,
        dir: glam::Vec2,
    ) -> bool {
        let (ax, az) = self.coluna(de.x, de.y);
        let base = self.bloco(ax, az);
        let perp = glam::Vec2::new(-dir.y, dir.x);
        for k in [-0.7f32, 0.0, 0.7] {
            let ponto = para + dir * raio + perp * (raio * k);
            let (ix, iz) = self.coluna(ponto.x, ponto.y);
            let h = self.bloco(ix, iz);
            if (h + 1) as f32 * BLOCO <= NIVEL_DO_MAR {
                return false;
            }
            // Descer e' livre; subir so' um bloco. E' a regra inteira.
            if h - base > DEGRAU_BLOCOS {
                return false;
            }
        }
        true
    }

    /// Terra firme mais proxima de `(x, z)`, em espiral.
    ///
    /// Existe porque as posicoes herdadas (spawn de personagem, zona de mob
    /// do mapfile) foram escolhidas num mapa de tiles plano: soltas na ilha,
    /// caem no mar ou dentro de um paredao. Melhor mover um pouco do que
    /// nascer boiando.
    pub fn terra_mais_proxima(&self, x: f32, z: f32, limite_un: f32) -> glam::Vec2 {
        if !self.agua(x, z) {
            return glam::Vec2::new(x, z);
        }
        let passos = (limite_un / BLOCO) as i32;
        for r in 1..=passos {
            // So' o anel de raio r: o miolo ja' foi visto na volta anterior.
            for i in -r..=r {
                for (dx, dz) in [(i, -r), (i, r), (-r, i), (r, i)] {
                    let (px, pz) = (x + dx as f32 * BLOCO, z + dz as f32 * BLOCO);
                    if !self.agua(px, pz) {
                        return glam::Vec2::new(px, pz);
                    }
                }
            }
        }
        glam::Vec2::new(x, z)
    }

    // ── caminho ──────────────────────────────────────────────────────────

    /// A* numa grade GROSSA sobre o campo de altura.
    ///
    /// Grossa por necessidade: a ilha grande tem 10,2 milhoes de colunas, e
    /// A* nelas seria um mundo inteiro por clique. Com celula de
    /// `PASSO_CAMINHO` blocos sao ~64x menos nos, e o movimento fino continua
    /// sendo o `mover_e_deslizar` de sempre — a rota so' diz por onde ir.
    ///
    /// `orcamento` limita os nos expandidos. Estourou, devolve o melhor
    /// caminho parcial em vez de nada: andar na direcao certa e parar no
    /// meio e' melhor que ficar plantado.
    ///
    /// Roda no SERVIDOR. O cliente manda um destino — que ele ja' poderia
    /// alcancar andando — e nao um caminho; quem decide por onde da' pra
    /// passar continua sendo quem tem o relevo.
    pub fn caminho(
        &self,
        de: glam::Vec2,
        para: glam::Vec2,
        orcamento: usize,
    ) -> Option<Vec<glam::Vec2>> {
        use std::collections::{BinaryHeap, HashMap};

        let cel = |p: glam::Vec2| -> (i32, i32) {
            (
                (p.x / (BLOCO * PASSO_CAMINHO as f32)).round() as i32,
                (p.y / (BLOCO * PASSO_CAMINHO as f32)).round() as i32,
            )
        };
        let mundo = |c: (i32, i32)| -> glam::Vec2 {
            glam::Vec2::new(
                c.0 as f32 * BLOCO * PASSO_CAMINHO as f32,
                c.1 as f32 * BLOCO * PASSO_CAMINHO as f32,
            )
        };
        let inicio = cel(de);
        let fim = cel(para);
        if inicio == fim {
            return Some(vec![para]);
        }

        // f e' negado: BinaryHeap e' max-heap e o A* quer o menor.
        #[derive(PartialEq, Eq)]
        struct No(i64, (i32, i32));
        impl Ord for No {
            fn cmp(&self, o: &Self) -> std::cmp::Ordering { self.0.cmp(&o.0) }
        }
        impl PartialOrd for No {
            fn partial_cmp(&self, o: &Self) -> Option<std::cmp::Ordering> { Some(self.cmp(o)) }
        }
        let h = |c: (i32, i32)| -> i64 {
            let (dx, dz) = ((c.0 - fim.0).abs() as i64, (c.1 - fim.1).abs() as i64);
            // Octil: diagonal custa ~1,41 e reta 1, em milesimos.
            let (mi, ma) = (dx.min(dz), dx.max(dz));
            mi * 1414 + (ma - mi) * 1000
        };

        let mut aberto = BinaryHeap::new();
        let mut custo: HashMap<(i32, i32), i64> = HashMap::new();
        let mut veio: HashMap<(i32, i32), (i32, i32)> = HashMap::new();
        aberto.push(No(-h(inicio), inicio));
        custo.insert(inicio, 0);
        let mut melhor = (inicio, h(inicio));
        let mut expandidos = 0usize;

        while let Some(No(_, atual)) = aberto.pop() {
            if atual == fim {
                melhor = (atual, 0);
                break;
            }
            expandidos += 1;
            if expandidos > orcamento {
                break;
            }
            let g = custo[&atual];
            for (dx, dz) in [
                (1, 0), (-1, 0), (0, 1), (0, -1),
                (1, 1), (1, -1), (-1, 1), (-1, -1),
            ] {
                let viz = (atual.0 + dx, atual.1 + dz);
                if !self.trecho_livre(mundo(atual), mundo(viz)) {
                    continue;
                }
                let passo = if dx != 0 && dz != 0 { 1414 } else { 1000 };
                let novo = g + passo;
                if custo.get(&viz).is_some_and(|&c| c <= novo) {
                    continue;
                }
                custo.insert(viz, novo);
                veio.insert(viz, atual);
                let hv = h(viz);
                if hv < melhor.1 {
                    melhor = (viz, hv);
                }
                aberto.push(No(-(novo + hv), viz));
            }
        }

        // Reconstroi a partir do melhor no alcancado — parcial serve.
        let mut rota = vec![melhor.0];
        let mut c = melhor.0;
        while let Some(&p) = veio.get(&c) {
            rota.push(p);
            c = p;
            if rota.len() > 4096 {
                break;
            }
        }
        if rota.len() <= 1 && melhor.0 != fim {
            return None;
        }
        rota.reverse();
        let mut saida: Vec<glam::Vec2> = rota.into_iter().skip(1).map(mundo).collect();
        if melhor.0 == fim {
            // O ultimo ponto e' o destino de verdade, nao o centro da celula.
            saida.pop();
            saida.push(para);
        }
        Some(saida)
    }

    /// Da' pra ir do centro de uma celula grossa ao da vizinha?
    ///
    /// Checa as colunas FINAS no meio do caminho: um passo de oito blocos
    /// esconderia um paredao de oito blocos, e a rota mandaria o jogador
    /// andar contra a parede pra sempre.
    fn trecho_livre(&self, de: glam::Vec2, para: glam::Vec2) -> bool {
        let n = PASSO_CAMINHO.max(1);
        let mut anterior = de;
        for i in 1..=n {
            let t = i as f32 / n as f32;
            let p = de + (para - de) * t;
            if !self.passo_ok((anterior.x, anterior.y), (p.x, p.y)) {
                return false;
            }
            anterior = p;
        }
        true
    }

    pub fn salvar(&self, caminho: &str) -> std::io::Result<()> {
        use std::io::Write;
        if let Some(pai) = std::path::Path::new(caminho).parent() {
            std::fs::create_dir_all(pai)?;
        }
        let mut f = std::io::BufWriter::new(std::fs::File::create(caminho)?);
        f.write_all(&MAGICA)?;
        f.write_all(&VERSAO.to_le_bytes())?;
        f.write_all(&self.semente.to_le_bytes())?;
        f.write_all(&self.raio_blocos.to_le_bytes())?;
        f.write_all(&(self.bioma as u8).to_le_bytes())?;
        f.write_all(&self.escala_altura.to_le_bytes())?;
        for a in &self.blocos {
            f.write_all(&a.to_le_bytes())?;
        }
        f.flush()
    }

    fn carregar(
        caminho: &str,
        semente: i32,
        raio_blocos: i32,
        bioma: Bioma,
        escala_altura: f32,
    ) -> Option<Self> {
        let dados = std::fs::read(caminho).ok()?;
        let lado = (raio_blocos * 2) as usize;
        let cab = 4 + 2 + 4 + 4 + 1 + 4;
        if dados.len() != cab + lado * lado * 2 || dados[..4] != MAGICA {
            return None;
        }
        let u16em = |i: usize| u16::from_le_bytes([dados[i], dados[i + 1]]);
        let i32em = |i: usize| {
            i32::from_le_bytes([dados[i], dados[i + 1], dados[i + 2], dados[i + 3]])
        };
        let f32em = |i: usize| {
            f32::from_le_bytes([dados[i], dados[i + 1], dados[i + 2], dados[i + 3]])
        };
        // Semente, raio, bioma e escala tem que bater: cache de uma ilha
        // servindo como se fosse de outra e' bug que so' aparece em producao.
        if u16em(4) != VERSAO
            || i32em(6) != semente
            || i32em(10) != raio_blocos
            || dados[14] != bioma as u8
            || (f32em(15) - escala_altura).abs() > 1e-6
        {
            return None;
        }
        let mut blocos = vec![0i16; lado * lado];
        for (i, a) in blocos.iter_mut().enumerate() {
            let j = cab + i * 2;
            *a = i16::from_le_bytes([dados[j], dados[j + 1]]);
        }
        Some(Self { semente, raio_blocos, bioma, escala_altura, lado, blocos })
    }

    /// Quanto desta ilha serve pra jogar.
    ///
    /// MMO nao precisa de relevo bonito, precisa de CHAO: mob, chefe e briga
    /// querem area plana, e ruido puro entrega quase tudo inclinado. Sem medir
    /// isso, "mais areas planas" e' opiniao — com o numero na mao da' pra
    /// afinar o gerador ate' o mundo servir.
    pub fn estatisticas(&self) -> Estatisticas {
        let mut e = Estatisticas { colunas: self.lado * self.lado, ..Default::default() };
        for iz in 0..self.lado as i32 {
            for ix in 0..self.lado as i32 {
                let h = self.bloco(ix, iz);
                if (h + 1) as f32 * BLOCO <= NIVEL_DO_MAR {
                    e.agua += 1;
                    continue;
                }
                e.terra += 1;
                let viz = [
                    self.bloco(ix + 1, iz),
                    self.bloco(ix - 1, iz),
                    self.bloco(ix, iz + 1),
                    self.bloco(ix, iz - 1),
                ];
                let maior = viz.iter().map(|v| (v - h).abs()).max().unwrap_or(0);
                if maior == 0 {
                    e.plana += 1;
                }
                if maior <= DEGRAU_BLOCOS {
                    e.andavel += 1;
                }
                if maior > PULO_BLOCOS {
                    e.parede += 1;
                }
            }
        }
        // Sitio de spawn: disco inteiro dentro de uma faixa de altura. Custa
        // caro por coluna, entao a amostra pula de 4 em 4 — o numero e'
        // proporcao, nao contagem exata.
        let passo = 4i32;
        for (raio_un, campo) in [(4.0f32, 0usize), (12.0, 1)] {
            let r = (raio_un / BLOCO) as i32;
            let mut bons = 0usize;
            let mut vistos = 0usize;
            let mut iz = 0;
            while iz < self.lado as i32 {
                let mut ix = 0;
                while ix < self.lado as i32 {
                    vistos += 1;
                    if self.sitio_plano(ix, iz, r) {
                        bons += 1;
                    }
                    ix += passo;
                }
                iz += passo;
            }
            let frac = if vistos == 0 { 0.0 } else { bons as f32 / vistos as f32 };
            if campo == 0 { e.sitio_mob = frac } else { e.sitio_chefe = frac }
        }
        e
    }

    /// Um disco de raio `r` blocos cabe aqui sem degrau maior que um bloco e
    /// sem molhar o pe'? E' o teste que decide onde mob e chefe nascem.
    pub fn sitio_plano(&self, cx: i32, cz: i32, r: i32) -> bool {
        let base = self.bloco(cx, cz);
        if (base + 1) as f32 * BLOCO <= NIVEL_DO_MAR {
            return false;
        }
        // Amostra o anel e a cruz em vez do disco cheio: um disco de raio 24
        // sao 1.800 colunas por sitio, e o que se procura — desnivel — sempre
        // aparece na borda antes do miolo.
        let passo = (r / 4).max(1);
        let mut d = passo;
        while d <= r {
            for (dx, dz) in [(d, 0), (-d, 0), (0, d), (0, -d),
                             (d * 7 / 10, d * 7 / 10), (-d * 7 / 10, d * 7 / 10),
                             (d * 7 / 10, -d * 7 / 10), (-d * 7 / 10, -d * 7 / 10)] {
                if (self.bloco(cx + dx, cz + dz) - base).abs() > DEGRAU_BLOCOS {
                    return false;
                }
            }
            d += passo;
        }
        true
    }

    /// Le' do disco, ou gera e grava.
    ///
    /// Onze canais da mesma zona sobem juntos: sem cache, cada um gastaria os
    /// mesmos ~5 s de CPU calculando exatamente o mesmo relevo. O arquivo e'
    /// derivado da semente — pode ser apagado a qualquer momento e nao vai
    /// pro git.
    pub fn carregar_ou_gerar(
        dir: &str,
        semente: i32,
        raio_blocos: i32,
        bioma: Bioma,
        escala_altura: f32,
    ) -> Self {
        let caminho = format!("{dir}/{semente}-{raio_blocos}.alt");
        if let Some(i) = Self::carregar(&caminho, semente, raio_blocos, bioma, escala_altura) {
            return i;
        }
        let ilha = Self::gerar(semente, raio_blocos, bioma, escala_altura);
        let _ = ilha.salvar(&caminho);
        ilha
    }
}

/// O que o mundo tem, em numeros. Ver `Ilha::estatisticas`.
#[derive(Debug, Default, Clone, Copy)]
pub struct Estatisticas {
    pub colunas: usize,
    pub agua: usize,
    pub terra: usize,
    /// Colunas com os quatro vizinhos no MESMO nivel.
    pub plana: usize,
    /// Colunas em que da' pra andar em qualquer direcao (degrau <= 1).
    pub andavel: usize,
    /// Colunas com algum vizinho alto demais ate' pra pulo.
    pub parede: usize,
    /// Fracao das amostras que aceita um mob (disco de 4 unidades).
    pub sitio_mob: f32,
    /// Fracao das amostras que aceita um chefe (disco de 12 unidades).
    pub sitio_chefe: f32,
}

impl Estatisticas {
    pub fn pct_terra(&self) -> f32 { pct(self.terra, self.colunas) }
    /// Das colunas de TERRA — que e' o que interessa: agua nao e' chao ruim,
    /// e' outra coisa.
    pub fn pct_plana(&self) -> f32 { pct(self.plana, self.terra) }
    pub fn pct_andavel(&self) -> f32 { pct(self.andavel, self.terra) }
    pub fn pct_parede(&self) -> f32 { pct(self.parede, self.terra) }
}

fn pct(a: usize, b: usize) -> f32 {
    if b == 0 { 0.0 } else { a as f32 * 100.0 / b as f32 }
}

// ───────────────────────────── arquipelago ───────────────────────────

/// Uma ilha do arquipelago. Cada uma e' uma ZONA — processo proprio, mapa
/// proprio — ligada as outras por viagem, nao por caminhada.
pub struct DefIlha {
    pub zona: &'static str,
    pub nome: &'static str,
    pub semente: i32,
    pub raio_blocos: i32,
    pub bioma: Bioma,
    /// Centro no mapa-mundi, em unidades. `-x` e' oeste, `-z` e' norte.
    /// So' serve pro mapa e pra viagem: cada zona simula com a propria ilha
    /// centrada na origem.
    pub centro: [f32; 2],
    /// Faixa de nivel da ilha: o minimo perto do desembarque, o maximo na
    /// ponta mais distante. Quem escolhe o BICHO e' o nivel, nao esta tabela —
    /// e' assim que mob novo entra sem tocar em codigo de mundo.
    pub nivel: (u32, u32),
}

impl DefIlha {
    pub fn raio_m(&self) -> f32 {
        self.raio_blocos as f32 * BLOCO
    }
}

/// O arquipelago do play test: quatro ilhas, progressao pro OESTE.
///
/// A inicial e a final tem 800 m de raio; as duas do meio, 400. Nao ha' nada
/// de sagrado nesses numeros — o relevo e' funcao da semente e do raio, entao
/// mudar o tamanho de uma ilha e' trocar um campo aqui.
pub const ARQUIPELAGO: [DefIlha; 4] = [
    DefIlha {
        zona: "ilha_inicial",
        nome: "Bosque",
        semente: 0x10FE_5701,
        raio_blocos: 1600,
        bioma: Bioma::Floresta,
        centro: [0.0, 0.0],
        nivel: (1, 15),
    },
    DefIlha {
        zona: "ilha_gelo",
        nome: "Geleira",
        semente: 0x6E10_0002,
        raio_blocos: 800,
        bioma: Bioma::Gelo,
        centro: [-1800.0, -1100.0],
        nivel: (15, 30),
    },
    DefIlha {
        zona: "ilha_deserto",
        nome: "Ermo",
        semente: 0x0E5E_2703,
        raio_blocos: 800,
        bioma: Bioma::Deserto,
        centro: [-1800.0, 1100.0],
        nivel: (28, 42),
    },
    DefIlha {
        zona: "ilha_planalto",
        nome: "Planalto",
        semente: 0x3011_7A04,
        raio_blocos: 1600,
        bioma: Bioma::Montanha,
        centro: [-3600.0, 0.0],
        nivel: (40, 60),
    },
];

pub fn def_da_zona(zona: &str) -> Option<&'static DefIlha> {
    ARQUIPELAGO.iter().find(|d| d.zona == zona)
}

#[cfg(test)]
mod testes {
    use super::*;

    fn blocos(i: &Ilha) -> Vec<i16> {
        (0..i.lado as i32)
            .flat_map(|z| (0..i.lado as i32).map(move |x| (x, z)))
            .map(|(x, z)| i.bloco(x, z) as i16)
            .collect()
    }

    /// `TODOS[i] as u8 == i`, sempre.
    ///
    /// A vegetacao guarda material num `u8` e le' de volta por esta tabela. Um
    /// desalinhamento nao da' erro de compilacao nem panico: da' voxel
    /// INVISIVEL, e o sintoma aparece como "a vegetacao sumiu" tres passos
    /// depois. Ja' aconteceu uma vez com `Tronco`.
    #[test]
    fn a_tabela_de_material_esta_alinhada() {
        for (i, m) in Material::TODOS.iter().enumerate() {
            assert_eq!(*m as usize, i, "{m:?} esta' na posicao errada de TODOS");
            assert_eq!(Material::de_u8(i as u8), Some(*m));
        }
    }

    /// A propriedade da qual tudo depende: mesma semente, mesmo relevo. Se
    /// isto quebrar, cliente e servidor passam a discordar do chao e nenhum
    /// sintoma aponta pra ca'.
    #[test]
    fn geracao_e_deterministica() {
        let a = Ilha::gerar(1234, 64, Bioma::Floresta, ESCALA_ALTURA);
        let b = Ilha::gerar(1234, 64, Bioma::Floresta, ESCALA_ALTURA);
        assert_eq!(blocos(&a), blocos(&b));
    }

    #[test]
    fn semente_diferente_muda_o_relevo() {
        let a = Ilha::gerar(1, 64, Bioma::Floresta, ESCALA_ALTURA);
        let b = Ilha::gerar(2, 64, Bioma::Floresta, ESCALA_ALTURA);
        assert_ne!(blocos(&a), blocos(&b));
    }

    /// O arquivo de cache tem que devolver a MESMA ilha. Cache de uma semente
    /// servindo como se fosse de outra e' bug que so' aparece em producao.
    #[test]
    fn cache_em_disco_bate_com_a_geracao() {
        let dir = std::env::temp_dir().join("tempest-terreno-teste");
        let dir = dir.to_string_lossy().to_string();
        let _ = std::fs::remove_dir_all(&dir);
        let a = Ilha::carregar_ou_gerar(&dir, 99, 48, Bioma::Gelo, ESCALA_ALTURA);
        let b = Ilha::carregar_ou_gerar(&dir, 99, 48, Bioma::Gelo, ESCALA_ALTURA);
        assert_eq!(blocos(&a), blocos(&b));
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Fora da grade e' mar aberto, nunca panico de indice.
    #[test]
    fn fora_da_grade_e_agua() {
        let i = Ilha::gerar(7, 32, Bioma::Floresta, ESCALA_ALTURA);
        assert!(i.agua(99999.0, -99999.0));
        assert!(!i.passo_ok((0.0, 0.0), (99999.0, 0.0)));
    }

    /// Um bloco anda, dois pula, tres nao passa. E' a regra de movimento
    /// inteira, e ela cai fora se alguem mexer em `bloco()` sem perceber.
    #[test]
    fn um_anda_dois_pula_tres_nao_passa() {
        let mut i = Ilha::gerar(3, 32, Bioma::Floresta, ESCALA_ALTURA);
        let l = i.lado;
        for c in i.blocos.iter_mut() {
            *c = 4;
        }
        let x = |ix: usize| (ix as i32 - (l / 2) as i32) as f32 * BLOCO;
        // tres degraus a leste do centro, de 1, 2 e 3 blocos
        let meio = l / 2;
        i.blocos[meio * l + meio + 1] = 5;
        i.blocos[meio * l + meio + 2] = 6;
        i.blocos[meio * l + meio + 3] = 7;
        let z = 0.0;
        let p = |ix: usize| (x(ix), z);
        assert!(i.passo_ok(p(meio), p(meio + 1)), "1 bloco tem que andar");
        assert!(!i.passo_ok(p(meio), p(meio + 2)), "2 blocos nao anda");
        assert!(i.pulo_ok(p(meio), p(meio + 2)), "2 blocos tem que pular");
        assert!(!i.pulo_ok(p(meio), p(meio + 3)), "3 blocos nao passa");
        // descer e' sempre livre
        assert!(i.passo_ok(p(meio + 3), p(meio)));
    }

    /// Deslizar e nao grudar: quem bate na parede andando na diagonal tem que
    /// continuar indo pelo eixo livre.
    #[test]
    fn desliza_na_parede_em_vez_de_grudar() {
        let mut i = Ilha::gerar(11, 32, Bioma::Floresta, ESCALA_ALTURA);
        let l = i.lado;
        for c in i.blocos.iter_mut() {
            *c = 4;
        }
        // Muro norte-sul de quatro colunas de largura, comecando no centro
        // (mundo x = 0). Largo pra o teste nao depender de arredondamento de
        // meia coluna.
        for iz in 0..l {
            for d in 0..4 {
                i.blocos[iz * l + l / 2 + d] = 20;
            }
        }
        // Corpo encostado no muro: raio 0,35 a partir de -0,8 chega em -0,45,
        // e um passo de 0,4 poe a borda dentro da primeira coluna do muro.
        let de = glam::Vec2::new(-0.8, 0.0);
        let p = i.mover_e_deslizar(de, glam::Vec2::new(4.0, 4.0), 0.1, 0.35);
        assert!((p.x - de.x).abs() < 0.001, "X devia ter barrado, foi pra {}", p.x);
        assert!(p.y > de.y + 0.3, "Z devia ter passado, foi pra {}", p.y);
    }

    /// Caminhada longa pela ilha DE VERDADE, checando os invariantes a cada
    /// passo. Teste de muro sintetico prova a regra; este prova que o relevo
    /// gerado nao tem canto onde ela se quebra.
    #[test]
    fn caminhada_longa_respeita_a_regra() {
        let d = &ARQUIPELAGO[0];
        let i = Ilha::gerar(d.semente, 800, d.bioma, ESCALA_ALTURA);
        let mut p = i.terra_mais_proxima(0.0, 0.0, 400.0);
        assert!(!i.agua(p.x, p.y), "nao achou terra pra comecar");

        // Passo de jogador: 4 unidades/s a 30Hz.
        let passo = 4.0 / 30.0;
        let mut est: u32 = 0x1234_5678;
        let mut subiu_demais = 0u32;
        let mut na_agua = 0u32;
        let mut andou = 0.0f32;
        for _ in 0..20_000 {
            est = est.wrapping_mul(1664525).wrapping_add(1013904223);
            let ang = (est >> 8) as f32 / (1u32 << 24) as f32 * std::f32::consts::TAU;
            let v = glam::Vec2::new(ang.cos(), ang.sin()) * (passo / (1.0 / 30.0));
            let (ax, az) = i.coluna(p.x, p.y);
            let antes = i.bloco(ax, az);
            let novo = i.mover_e_deslizar(p, v, 1.0 / 30.0, 0.35);
            let (bx, bz) = i.coluna(novo.x, novo.y);
            let depois = i.bloco(bx, bz);
            if depois - antes > DEGRAU_BLOCOS {
                subiu_demais += 1;
            }
            if i.agua(novo.x, novo.y) {
                na_agua += 1;
            }
            andou += p.distance(novo);
            p = novo;
        }
        assert_eq!(subiu_demais, 0, "subiu mais de um bloco em um passo");
        assert_eq!(na_agua, 0, "andou sobre a agua");
        // Se andou quase nada, o corpo travou num canto e o teste acima
        // passaria sem ter testado coisa alguma.
        // Livre, vinte mil passos de 0,133 dariam 2.666 unidades. O que
        // falta e' encosta reprovada, e e' isso que se quer medir: nem passar
        // por tudo (mundo sem parede) nem travar (mundo sem chao).
        println!("caminhou {andou:.0} de 2666 unidades possiveis ({:.0}% dos passos passaram)",
            andou / 2666.0 * 100.0);
        assert!(andou > 200.0, "andou so' {andou:.0} unidades — travou em algum canto");
    }

    #[test]
    fn nao_anda_sobre_a_agua() {
        let i = Ilha::gerar(5, 64, Bioma::Floresta, ESCALA_ALTURA);
        // bem fora da ilha e' mar aberto: nao anda pra la'
        let de = glam::Vec2::new(0.0, 0.0);
        let longe = i.mover_e_deslizar(de, glam::Vec2::new(9999.0, 0.0), 1.0, 0.35);
        assert_eq!(longe.x, de.x);
    }

    /// A rota tem que existir, chegar perto e — o que mais importa — so'
    /// passar por onde o jogador PODE andar. Rota que atravessa paredao e'
    /// pior que rota nenhuma: o boneco vai bater na pedra pra sempre.
    #[test]
    fn a_rota_so_passa_por_onde_da_pra_andar() {
        let d = &ARQUIPELAGO[0];
        let i = Ilha::gerar(d.semente, 800, d.bioma, ESCALA_ALTURA);
        let de = i.terra_mais_proxima(0.0, 0.0, 400.0);
        let para = i.terra_mais_proxima(90.0, 60.0, 400.0);
        let rota = i.caminho(de, para, 20_000).expect("nao achou rota");
        assert!(!rota.is_empty());
        let mut p = de;
        for alvo in &rota {
            // Cada trecho da rota tem que ser andavel de ponta a ponta.
            let n = 12;
            let mut a = p;
            for k in 1..=n {
                let t = k as f32 / n as f32;
                let b = p + (*alvo - p) * t;
                assert!(
                    i.passo_ok((a.x, a.y), (b.x, b.y)),
                    "a rota atravessa terreno intransitavel em {b:?}"
                );
                a = b;
            }
            p = *alvo;
        }
        assert!(p.distance(para) < 12.0, "parou a {:.0} do destino", p.distance(para));
    }

    /// Destino inalcancavel devolve caminho PARCIAL, nao nada: andar na
    /// direcao certa e parar no meio e' melhor que ficar plantado.
    #[test]
    fn destino_no_mar_devolve_parcial_ou_nada() {
        let d = &ARQUIPELAGO[0];
        let i = Ilha::gerar(d.semente, 400, d.bioma, ESCALA_ALTURA);
        let de = i.terra_mais_proxima(0.0, 0.0, 400.0);
        // bem no meio do mar aberto
        let r = i.caminho(de, glam::Vec2::new(5000.0, 5000.0), 4_000);
        if let Some(rota) = r {
            for alvo in &rota {
                assert!(!i.agua(alvo.x, alvo.y), "rota entrou no mar em {alvo:?}");
            }
        }
    }

    #[test]
    fn biomas_dao_relevos_diferentes() {
        let pico = |b: Bioma| {
            let i = Ilha::gerar(42, 96, b, ESCALA_ALTURA);
            blocos(&i).into_iter().max().unwrap() as f32 * BLOCO
        };
        // Duna e' baixa, planalto e' alto, floresta fica no meio.
        let (d, f, m) = (pico(Bioma::Deserto), pico(Bioma::Floresta), pico(Bioma::Montanha));
        assert!(d < f, "deserto {d} >= floresta {f}");
        assert!(m > f, "planalto {m} <= floresta {f}");
    }

    /// Todas as ilhas do play test tem que ter chao pra chefe. Zero sitio e'
    /// mundo bonito e injogavel — foi exatamente o defeito que a medicao
    /// pegou na primeira versao.
    #[test]
    fn todo_bioma_tem_sitio_de_chefe() {
        for d in ARQUIPELAGO.iter() {
            // 800 e nao 200: abaixo de 600 o `Forma::de` sorteia outro feitio
            // e some a enseada, entao a ilha reduzida nao e' a mesma ilha. O
            // teste tem que medir o que sobe, nao um primo dela.
            let i = Ilha::gerar(d.semente, d.raio_blocos.min(800), d.bioma, ESCALA_ALTURA);
            let e = i.estatisticas();
            assert!(e.sitio_chefe > 0.01, "{}: sitio de chefe {:.3}", d.zona, e.sitio_chefe);
        }
    }
}
