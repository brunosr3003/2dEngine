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
/// O teto existe porque "acima do degrau, so' com pulo" sozinho deixaria um
/// paredao de vinte blocos ser pulado tambem. Um bloco anda, ate' tres pula,
/// quatro nao passa.
///
/// Tres blocos sao 1,5 unidade — quase a altura do boneco (1,68). E' um pulo
/// grande de proposito: com dois, quase todo barranco de ilha continuava
/// parede e o relevo lia como corredor.
pub const PULO_BLOCOS: i32 = 3;

/// Quanto um trecho que so' se vence PULANDO custa a mais no A*, em milesimos
/// de celula.
///
/// Sai do relogio e nao do gosto: o pulo trava por `PULO_DURACAO +
/// PULO_ESPERA` (0,57 s), e a 5 unidades por segundo isso sao 2,85 unidades —
/// setenta por cento de uma celula de quatro. Arredondado pra uma celula
/// inteira, o A* aceita dar ate' uma celula de volta pra nao pular, e pula
/// quando a volta sai mais cara.
pub const CUSTO_DO_PULO: i64 = 1000;

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
    /// Cristal de minerio. A COR E' O TIER — e' o unico jeito de o jogador
    /// ler o valor de uma pedra do outro lado do vale, sem UI e sem chegar
    /// perto. Saturados e claros de proposito: eles tem que gritar contra a
    /// rocha cinzenta em que estao cravados.
    CristalCinza,
    CristalVerde,
    CristalAzul,
    CristalRoxo,
}

impl Material {
    /// Todos, na ordem em que foram declarados.
    ///
    /// Existe pra guardar material num `u8` e ler de volta sem tabela
    /// paralela. Ja' custou caro: uma tabela com subconjunto dos materiais fez
    /// `Tronco` (discriminante 11) cair fora dela, e todo tronco e toda folha
    /// da vegetacao viraram voxel INVISIVEL — solido pra colisao de face,
    /// vazio pra malha. O mundo ficou coberto de pedra e mais nada.
    pub const TODOS: [Material; 25] = [
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
        Material::CristalCinza,
        Material::CristalVerde,
        Material::CristalAzul,
        Material::CristalRoxo,
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
            // O cinza do cristal e' CLARO, quase branco: cinza de pedra sobre
            // pedra cinza nao se enxerga, e a pedra mais comum e' justamente
            // a que o jogador precisa achar primeiro.
            Material::CristalCinza => (214, 220, 228),
            Material::CristalVerde => (96, 226, 138),
            Material::CristalAzul => (92, 176, 250),
            Material::CristalRoxo => (186, 112, 246),
        }
    }

    /// Materiais que nao recebem sombra: eles EMITEM.
    ///
    /// Cristal escurecido no lado de baixo vira pedra pintada, e ai' o tier
    /// so' se le' de perto e de cima. O brilho nao e' enfeite — e' a leitura
    /// do valor da pedra a distancia, que e' o que faz o jogador escolher pra
    /// qual pico subir.
    pub fn emissivo(self) -> bool {
        matches!(
            self,
            Material::CristalCinza
                | Material::CristalVerde
                | Material::CristalAzul
                | Material::CristalRoxo
        )
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
            // Cristal e' o mesmo de todo lado: a face lateral escurecida
            // apagaria justamente o brilho que faz o tier ser legivel.
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

// ───────────────────── o que esta' plantado numa coluna ─────────────────────
//
// Esta decisao morava no cliente, junto do desenho. Ela subiu pra ca' quando
// tronco, pedra e toco passaram a BARRAR passagem: virou regra, e regra que o
// cliente decide sozinho o servidor tem que adivinhar. Cada divergencia entre
// os dois viraria arvore atravessavel num lado e parede invisivel no outro —
// as duas piores que nao ter colisao nenhuma.
//
// Nada disso e' guardado: e' funcao pura do par de coordenadas. A mesma
// arvore no mesmo lugar em qualquer maquina, sem um byte no fio.

/// Solo em que planta pega. Rocha e laje nao seguram raiz.
pub fn solo_vivo(m: Material) -> bool {
    matches!(
        m,
        Material::Grama
            | Material::GramaClara
            | Material::GramaEscura
            | Material::Terra
            | Material::Areia
            | Material::Neve
    )
}

/// Uma arvore, decidida.
#[derive(Debug, Clone, Copy)]
pub struct ArvorePlantada {
    pub especie: Arvore,
    /// Centro em coordenada de MUNDO, ja' com o desvio dentro da coluna.
    pub centro: glam::Vec2,
    pub porte: f32,
    /// Qual dos modelos sorteados usar (o cliente tem varios por especie).
    pub variante: u32,
}

/// Uma planta de forracao, decidida.
#[derive(Debug, Clone, Copy)]
pub struct PlantaPlantada {
    pub especie: Planta,
    pub centro: glam::Vec2,
    pub porte: f32,
    pub variante: u32,
}

/// Raio do TRONCO em unidades de mundo, a porte 1.
///
/// Bate com o modelo: a copada tem tronco de tres voxels (0,75 de largura), o
/// resto tem um so'. O voxel do vegetal e' 0,25.
///
/// Nao vem do modelo, e' o contrario: o modelo obedece a este numero. O
/// gerador de voxel sorteia a espessura do tronco, e o servidor nao roda o
/// gerador — se a colisao dependesse dele, cada arvore teria um raio que so'
/// o cliente conhece.
pub fn raio_de_tronco(a: Arvore) -> f32 {
    match a {
        Arvore::Copada => 0.38,
        // Um voxel de tronco da' 0,125 de raio. Ficaria fino a ponto de
        // parecer poste invisivel, entao arredonda pra cima: melhor barrar um
        // dedo antes que deixar o corpo entrar dentro da madeira.
        Arvore::Betula | Arvore::Pinheiro | Arvore::Seca => 0.18,
    }
}

/// Raio de colisao da forracao, a porte 1. `None` = passa por dentro.
///
/// Flor, moita, arbusto, samambaia e talo NAO barram: sao da altura do joelho
/// pra baixo e cobrem o chao inteiro. Colidir com eles seria transformar a
/// forracao — que existe pra o chao nao ser um campo de golfe — num labirinto.
pub fn raio_de_planta(p: Planta) -> Option<f32> {
    match p {
        // Matacao: o modelo vai de 2 a 5 voxels de raio, media 0,875.
        Planta::Pedra => Some(0.62),
        // Toco: 2 a 3 voxels.
        Planta::Toco => Some(0.5),
        Planta::Moita | Planta::Flor | Planta::Arbusto | Planta::Samambaia | Planta::Talo => None,
    }
}

/// A arvore desta coluna, se houver.
///
/// `topo` e `declive` vem de fora porque os dois lados tem o relevo por
/// caminhos diferentes: o cliente gera coluna a coluna, o servidor tem a ilha
/// inteira em memoria. O sorteio, que e' o que precisa casar, e' daqui.
pub fn arvore_da_coluna(
    bioma: Bioma,
    bx: i32,
    bz: i32,
    topo: i32,
    declive: i32,
    ger: &Gerador,
    agua: bool,
) -> Option<ArvorePlantada> {
    let prob = densidade_de_arvore(bioma) * 0.0025; // coluna = 0,25 m²
    let h0 = (bx as u32).wrapping_mul(374_761_393) ^ (bz as u32).wrapping_mul(668_265_263);
    let h1 = h0.wrapping_mul(1_274_126_177);
    if (h1 >> 8) as f32 / (1u32 << 24) as f32 >= prob || agua {
        return None;
    }
    // Hash INDEPENDENTE pra escolher a especie.
    //
    // Reaproveitar `h1` parecia economia e era bug: so' passa no teste de
    // densidade quem tem `h1` PEQUENO, e um `h1` pequeno tem os bits de cima
    // em zero. A especie saia sempre a primeira da lista — medido com censo:
    // 100% de uma so'.
    let h2 = h1.wrapping_mul(2_246_822_519).wrapping_add(374_761_393);
    let y = (topo + 1) as f32 * BLOCO;
    // So' onde o solo segura: nem rocha, nem encosta.
    // A mancha e' fbm de tres oitavas — cara. Ela so' e' calculada aqui,
    // depois de o sorteio de densidade ja' ter descartado 99,7% das colunas.
    if !solo_vivo(material_variado(bioma, y, declive, false, ger.mancha(bx, bz))) {
        return None;
    }
    Some(ArvorePlantada {
        especie: especie_de_arvore(bioma, (h2 >> 20) as f32 / 4096.0),
        // Desvio dentro da coluna: sem ele as arvores nascem todas no centro
        // do bloco e o bosque vira grade.
        centro: glam::Vec2::new(
            bx as f32 * BLOCO + ((h0 >> 4 & 0xff) as f32 / 255.0 - 0.5) * BLOCO * 1.6,
            bz as f32 * BLOCO + ((h1 >> 4 & 0xff) as f32 / 255.0 - 0.5) * BLOCO * 1.6,
        ),
        // Porte menor do que parece certo em pe': a camera olha de cima, e
        // arvore de tres vezes o jogador esconde o mob que ele veio cacar.
        porte: 0.62 + ((h0 >> 12) & 0xff) as f32 / 255.0 * 0.34,
        variante: h0 >> 26,
    })
}

/// A planta de forracao desta coluna, se houver.
pub fn planta_da_coluna(
    bioma: Bioma,
    bx: i32,
    bz: i32,
    topo: i32,
    declive: i32,
    ger: &Gerador,
    agua: bool,
) -> Option<PlantaPlantada> {
    let prob = densidade_de_planta(bioma) * 0.0025;
    let g0 = (bx as u32).wrapping_mul(1_597_334_677) ^ (bz as u32).wrapping_mul(2_246_822_519);
    let g1 = g0.wrapping_mul(2_654_435_761);
    if (g1 >> 8) as f32 / (1u32 << 24) as f32 >= prob || agua {
        return None;
    }
    // Mesma armadilha das arvores: hash proprio pra especie.
    let g2 = g1.wrapping_mul(1_597_334_677).wrapping_add(2_246_822_519);
    let y = (topo + 1) as f32 * BLOCO;
    let solo = material_variado(bioma, y, declive, false, ger.mancha(bx, bz));
    let especie = especie_de_planta(bioma, (g2 >> 20) as f32 / 4096.0);
    // Pedra nasce em qualquer chao, inclusive rocha e neve; o resto so' onde o
    // solo segura. Matacao em cima de laje e' o que uma cordilheira tem.
    if !matches!(especie, Planta::Pedra) && !solo_vivo(solo) {
        return None;
    }
    Some(PlantaPlantada {
        especie,
        centro: glam::Vec2::new(
            bx as f32 * BLOCO + ((g0 >> 4 & 0xff) as f32 / 255.0 - 0.5) * BLOCO * 1.8,
            bz as f32 * BLOCO + ((g1 >> 4 & 0xff) as f32 / 255.0 - 0.5) * BLOCO * 1.8,
        ),
        // Forracao fica ABAIXO do joelho: planta da altura do jogador esconde
        // o que importa e faz a arvore perder escala.
        porte: 0.55 + ((g0 >> 14) & 0xff) as f32 / 255.0 * 0.45,
        // Variante por REGIAO e nao por planta: flores vizinhas saem da mesma
        // variante, logo da mesma cor, e viram MANCHA.
        variante: (((bx.div_euclid(10)) as u32).wrapping_mul(2_654_435_761)
            ^ ((bz.div_euclid(10)) as u32).wrapping_mul(40_503))
            >> 8,
    })
}

/// Uma pedra de minerio, decidida.
///
/// Nao e' o matacao da forracao (`Planta::Pedra`), que e' cenario: esta e' a
/// pedra que se COLETA, brilha na cor do tier e acaba depois de um numero de
/// coletas. Nasce so' em altura de montanha e em veio — e' um lugar, nao um
/// item espalhado pelo chao.
#[derive(Debug, Clone, Copy)]
pub struct Minerio {
    pub centro: glam::Vec2,
    /// 1 cinza, 2 verde, 3 azul, 4 roxo. A cor E' o tier.
    pub tier: u8,
    pub porte: f32,
    pub variante: u32,
}

/// Cristal da cor do tier: 1 cinza, 2 verde, 3 azul, 4 roxo.
pub fn cristal_do_tier(tier: u8) -> Material {
    match tier {
        2 => Material::CristalVerde,
        3 => Material::CristalAzul,
        4 => Material::CristalRoxo,
        _ => Material::CristalCinza,
    }
}

/// Raio de colisao da pedra de minerio, a porte 1. Maior que o matacao de
/// cenario porque ela e' maior: se dois corpos podem ocupar o mesmo ponto que
/// a pedra ocupa, ela deixa de ser um lugar disputado.
pub const RAIO_DE_MINERIO: f32 = 0.75;

/// Fracao do pico do bioma a partir da qual nasce minerio. Abaixo disso e'
/// vale, e vale nao tem mina.
///
/// Medido na ilha inicial (1,16 milhao de colunas em terra, pico teorico
/// 37,8): 0,42 poe o limiar em 15,9 unidades, que sao os 13% mais altos da
/// terra. Menos que isso e a mina vira paisagem; mais e ela some.
pub const MINERIO_LIMIAR: f32 = 0.42;

/// Tier da pedra pela ALTURA, em fracao do pico do bioma. Quanto mais alto o
/// pico, melhor o minerio — e a pedra roxa fica no ponto mais alto da ilha,
/// que e' o unico jeito de o topo da montanha ser um destino.
pub fn tier_de_minerio(altura: f32, pico: f32) -> u8 {
    let t = altura / pico.max(1.0);
    if t < 0.56 { 1 } else if t < 0.69 { 2 } else if t < 0.80 { 3 } else { 4 }
}

/// A pedra de minerio desta coluna, se houver.
///
/// Mesmo contrato de `arvore_da_coluna`: funcao pura do par de coordenadas,
/// rodada identica nos dois lados. O cliente desenha, o servidor barra
/// passagem e conta coleta — e nenhum byte de pedra viaja no fio.
pub fn minerio_da_coluna(
    bioma: Bioma,
    bx: i32,
    bz: i32,
    topo: i32,
    ger: &Gerador,
    agua: bool,
) -> Option<Minerio> {
    if agua { return None }
    let y = (topo + 1) as f32 * BLOCO;
    let pico = ger.pico();
    if y < pico * MINERIO_LIMIAR { return None }

    // O veio decide ONDE. Sem ele a pedra sairia uniforme por toda a
    // montanha, e "spot" perderia o sentido: se ha' minerio em qualquer
    // encosta, andar ate' um lugar nao significa nada.
    let veio = ger.veio(bx, bz);
    if veio < 0.55 { return None }
    let forca = (veio - 0.55) / 0.45;

    let g0 = (bx as u32).wrapping_mul(2_654_435_761) ^ (bz as u32).wrapping_mul(1_597_334_677);
    let g1 = g0.wrapping_mul(2_246_822_519).wrapping_add(374_761_393);
    let sorteio = (g1 >> 8) as f32 / (1u32 << 24) as f32;
    if sorteio >= DENSIDADE_DE_MINERIO * forca { return None }

    let _ = bioma;
    Some(Minerio {
        centro: glam::Vec2::new(
            bx as f32 * BLOCO + ((g0 >> 4 & 0xff) as f32 / 255.0 - 0.5) * BLOCO * 1.4,
            bz as f32 * BLOCO + ((g1 >> 4 & 0xff) as f32 / 255.0 - 0.5) * BLOCO * 1.4,
        ),
        tier: tier_de_minerio(y, pico),
        porte: 0.85 + ((g0 >> 14) & 0xff) as f32 / 255.0 * 0.45,
        variante: (g1 >> 22) & 0x3f,
    })
}

/// Chance de uma coluna elegivel ter pedra, no auge do veio.
///
/// Numero pequeno de proposito: pedra e' destino, nao forracao. Ver o teste
/// `a_ilha_tem_minerio_dos_quatro_tiers` pra contagem por tier na ilha
/// inicial.
pub const DENSIDADE_DE_MINERIO: f32 = 0.035;

/// Um corpo solido plantado no mundo: tronco, matacao ou toco.
#[derive(Debug, Clone, Copy)]
pub struct Estorvo {
    pub centro: glam::Vec2,
    pub raio: f32,
    pub tipo: TipoDeEstorvo,
    /// Coluna que o gerou, empacotada em `chave_de_coluna`. E' a IDENTIDADE
    /// do corpo: e' por ela que o servidor guarda quantas coletas ja' sairam
    /// desta pedra e o cliente sabe qual deixar de desenhar.
    pub coluna: u32,
}

/// O que o estorvo E'. A colisao nao se importa — barrar e' barrar —, mas a
/// COLETA se importa: pedra de minerio rende minerio, tronco rende madeira e
/// arbusto nao rende nada.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TipoDeEstorvo {
    Tronco,
    Forracao,
    /// Pedra de minerio, com o tier dela (1 cinza .. 4 roxo).
    Minerio(u8),
}

/// Empacota a coluna da ILHA (indices 0..lado) numa chave de 32 bits.
///
/// Cabe: a maior ilha do arquipelago tem raio 800, entao o lado passa longe
/// de 65.536. E' funcao pura dos dois indices, entao cliente e servidor
/// chegam na mesma chave sem combinar nada.
pub fn chave_de_coluna(ix: i32, iz: i32) -> u32 {
    ((ix.clamp(0, 0xffff) as u32) << 16) | (iz.clamp(0, 0xffff) as u32)
}

/// Tudo que esta' coluna tem e BARRA passagem, acrescentado em `saida`.
///
/// Podem ser DOIS: arvore e forracao sao sorteios independentes, com desvios
/// independentes dentro da coluna, e nada impede um matacao ao pe' de um
/// tronco. A primeira versao devolvia so' um e a pedra que dividia coluna com
/// arvore ficava atravessavel — achado pelo teste que compara o que o cliente
/// desenha com o que o servidor barra.
pub fn estorvos_da_coluna(
    bioma: Bioma,
    bx: i32,
    bz: i32,
    topo: i32,
    declive: i32,
    ger: &Gerador,
    agua: bool,
    saida: &mut Vec<Estorvo>,
) {
    let coluna = chave_de_coluna(bx + ger.raio_blocos, bz + ger.raio_blocos);
    if let Some(a) = arvore_da_coluna(bioma, bx, bz, topo, declive, ger, agua) {
        saida.push(Estorvo {
            centro: a.centro,
            raio: raio_de_tronco(a.especie) * a.porte,
            tipo: TipoDeEstorvo::Tronco,
            coluna,
        });
    }
    if let Some(p) = planta_da_coluna(bioma, bx, bz, topo, declive, ger, agua) {
        if let Some(r) = raio_de_planta(p.especie) {
            saida.push(Estorvo {
                centro: p.centro,
                raio: r * p.porte,
                tipo: TipoDeEstorvo::Forracao,
                coluna,
            });
        }
    }
    // A pedra vem por ULTIMO e ela manda: onde ha' minerio, a forracao da
    // mesma coluna e' descartada. Matacao de cenario encostado numa pedra de
    // minerio esconderia justamente o que o jogador precisa enxergar.
    if let Some(m) = minerio_da_coluna(bioma, bx, bz, topo, ger, agua) {
        saida.retain(|e| e.tipo != TipoDeEstorvo::Forracao);
        saida.push(Estorvo {
            centro: m.centro,
            raio: RAIO_DE_MINERIO * m.porte,
            tipo: TipoDeEstorvo::Minerio(m.tier),
            coluna,
        });
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

    /// Onde ha' VEIO de minerio, em 0..1. Frequencia mais baixa que a
    /// mancha: minerio tem que sair em mancha grande o bastante pra virar
    /// lugar ("aquele pico ali"), e nao pedra solta espalhada pelo mapa.
    pub fn veio(&self, bx: i32, bz: i32) -> f32 {
        (0.5 + self.pctrl.fbm(bx as f32 * 0.004 - 901.0, bz as f32 * 0.004 + 547.0, 2, 0.5) * 1.3)
            .clamp(0.0, 1.0)
    }

    /// Teto teorico do relevo deste bioma, em unidades. E' a regua contra a
    /// qual "topo de montanha" quer dizer a mesma coisa em todo bioma — o
    /// pico da Floresta e o do Planalto sao numeros bem diferentes.
    pub fn pico(&self) -> f32 {
        (self.perfil.serra.0 + self.perfil.serra.1) * self.escala_altura
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
    /// Perlin da ilha. Fica aqui porque o plantio precisa da MANCHA, e a
    /// mancha e' ruido — nao da' pra tirar do campo de altura.
    ger: Gerador,
    /// Tronco, matacao e toco: o que barra passagem sem ser relevo.
    ///
    /// Sao ~0,5% das colunas, entao guardar a LISTA custa cem vezes menos que
    /// um byte por coluna — e a alternativa, sortear na hora, sairia caro no
    /// lugar errado: `mover` roda por eixo, por entidade, trinta vezes por
    /// segundo, e o sorteio pede fbm de tres oitavas.
    estorvos: Vec<Estorvo>,
    /// Indice espacial dos estorvos: cada celula guarda os indices dos que
    /// caem nela. Sem ele, achar o tronco perto do jogador seria varrer a
    /// ilha inteira.
    grade: Vec<Vec<u32>>,
    /// Celulas por lado da grade.
    grade_lado: usize,
    /// Maior raio entre os estorvos. E' o alcance que a busca por segmento
    /// precisa abrir em volta da linha — tirado do dado e nao de um palpite,
    /// que envelheceria calado no dia em que o matacao crescesse.
    raio_max_estorvo: f32,
}

/// Uma coisa que se coleta, achada no raio do spot. Ver `Ilha::coletaveis_em`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Coletavel {
    /// Identidade do corpo: e' por ela que o servidor guarda quantas coletas
    /// ja' sairam dele, e por ela que o cliente sabe qual parar de desenhar.
    pub coluna: u32,
    pub centro: glam::Vec2,
    /// 1..4 pra pedra (a cor E' o tier), 0 pra tronco.
    pub tier: u8,
}

/// Lado da celula do indice de estorvos, em COLUNAS.
const CELULA_ESTORVO: i32 = 8;

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
        Self::com_blocos(semente, raio_blocos, bioma, escala_altura, lado, blocos, ger)
    }

    /// O caminho unico pra nascer uma ilha: gerada ou lida do cache, o indice
    /// de estorvos e' construido aqui. O cache guarda so' altura — plantio e'
    /// funcao pura da coordenada, entao refazer sai mais barato que gravar.
    fn com_blocos(
        semente: i32,
        raio_blocos: i32,
        bioma: Bioma,
        escala_altura: f32,
        lado: usize,
        blocos: Vec<i16>,
        ger: Gerador,
    ) -> Self {
        let mut i = Self {
            semente,
            raio_blocos,
            bioma,
            escala_altura,
            lado,
            blocos,
            ger,
            estorvos: Vec::new(),
            grade: Vec::new(),
            grade_lado: 0,
            raio_max_estorvo: 0.0,
        };
        i.plantar();
        i
    }

    /// Varre a ilha e guarda o que barra passagem.
    ///
    /// Publica porque quem mexe no relevo TEM que chamar de novo: o plantio
    /// depende da altura da coluna (nada nasce na agua, nem na encosta), e um
    /// indice velho vira tronco boiando ou parede invisivel.
    pub fn replantar(&mut self) {
        self.plantar();
    }

    fn plantar(&mut self) {
        let l = self.lado as i32;
        self.grade_lado = (l.div_euclid(CELULA_ESTORVO) + 1) as usize;
        self.grade = vec![Vec::new(); self.grade_lado * self.grade_lado];
        self.estorvos.clear();
        self.raio_max_estorvo = 0.0;
        let mut achados = Vec::new();
        for iz in 0..l {
            for ix in 0..l {
                let topo = self.bloco(ix, iz);
                if (topo + 1) as f32 * BLOCO <= NIVEL_DO_MAR {
                    continue;
                }
                // Coordenada de BLOCO DE MUNDO, com o centro da ilha em zero:
                // e' nela que o sorteio do plantio acontece, dos dois lados.
                let (bx, bz) = (ix - self.raio_blocos, iz - self.raio_blocos);
                let declive = [(1, 0), (-1, 0), (0, 1), (0, -1)]
                    .iter()
                    .map(|(dx, dz)| (topo - self.bloco(ix + dx, iz + dz)).abs())
                    .max()
                    .unwrap_or(0);
                achados.clear();
                estorvos_da_coluna(
                    self.bioma, bx, bz, topo, declive, &self.ger, false, &mut achados,
                );
                for e in achados.drain(..) {
                    self.raio_max_estorvo = self.raio_max_estorvo.max(e.raio);
                    let n = self.estorvos.len() as u32;
                    self.estorvos.push(e);
                    // Entra em TODAS as celulas que o corpo dele toca: um
                    // matacao na divisa ficaria invisivel pra quem chegasse
                    // pelo outro lado se so' o centro contasse.
                    let (c0x, c0z) = self.celula(e.centro.x - e.raio, e.centro.y - e.raio);
                    let (c1x, c1z) = self.celula(e.centro.x + e.raio, e.centro.y + e.raio);
                    for cz in c0z..=c1z {
                        for cx in c0x..=c1x {
                            if let Some(c) = self.celula_em(cx, cz) {
                                self.grade[c].push(n);
                            }
                        }
                    }
                }
            }
        }
    }

    /// Celula da grade que cobre uma coordenada de mundo.
    fn celula(&self, x: f32, z: f32) -> (i32, i32) {
        let (ix, iz) = self.coluna(x, z);
        (ix.div_euclid(CELULA_ESTORVO), iz.div_euclid(CELULA_ESTORVO))
    }

    fn celula_em(&self, cx: i32, cz: i32) -> Option<usize> {
        if cx < 0 || cz < 0 || cx as usize >= self.grade_lado || cz as usize >= self.grade_lado {
            return None;
        }
        Some(cz as usize * self.grade_lado + cx as usize)
    }

    /// Ponto que representa uma celula do A*: o centro dela, ou o mais perto
    /// disso onde o corpo caiba.
    ///
    /// Sem este desvio, um matacao em cima do centro de celula vira um ponto
    /// de rota DENTRO da pedra: o A* aprova (ele ignora estorvo nas pontas do
    /// trecho, senao a celula ficaria ilhada), o corpo nunca chega la', e a
    /// rota refeita devolve o mesmo ponto pra sempre. Medido num caso real:
    /// centro de celula a 0,45 de um matacao de raio 0,62.
    pub fn ponto_livre_perto(&self, centro: glam::Vec2, raio: f32) -> glam::Vec2 {
        if self.estorvo_em(centro, raio).is_none() {
            return centro;
        }
        // Espiral curta: o estorvo mais gordo tem 1,24 de diametro, entao um
        // ponto livre esta' sempre a menos de uma unidade — se houver.
        for anel in 1..=3 {
            let d = anel as f32 * BLOCO;
            for (dx, dz) in [
                (1.0, 0.0), (-1.0, 0.0), (0.0, 1.0), (0.0, -1.0),
                (0.7, 0.7), (0.7, -0.7), (-0.7, 0.7), (-0.7, -0.7),
            ] {
                let p = centro + glam::Vec2::new(dx * d, dz * d);
                if self.estorvo_em(p, raio).is_none() && !self.agua(p.x, p.y) {
                    return p;
                }
            }
        }
        centro
    }

    /// O estorvo que impede um corpo de raio `raio` de ocupar `p`, se houver.
    pub fn estorvo_em(&self, p: glam::Vec2, raio: f32) -> Option<Estorvo> {
        let (c0x, c0z) = self.celula(p.x - raio, p.y - raio);
        let (c1x, c1z) = self.celula(p.x + raio, p.y + raio);
        let mut pior: Option<(f32, Estorvo)> = None;
        for cz in c0z..=c1z {
            for cx in c0x..=c1x {
                let Some(c) = self.celula_em(cx, cz) else { continue };
                for &n in &self.grade[c] {
                    let e = self.estorvos[n as usize];
                    let d = raio + e.raio;
                    let dist2 = e.centro.distance_squared(p);
                    if dist2 < d * d && pior.is_none_or(|(m, _)| dist2 < m) {
                        pior = Some((dist2, e));
                    }
                }
            }
        }
        pior.map(|(_, e)| e)
    }

    /// O que ha' pra coletar em volta de `p`, num raio.
    ///
    /// E' a peca central da coleta: nao ha' ferramenta, nivel nem clique — o
    /// que decide o ganho e' O LUGAR, e o lugar e' esta lista. Devolve pedra
    /// de minerio e tronco; quem filtra o que esta' esgotado e' o servidor,
    /// que e' o unico que sabe disso.
    ///
    /// Sai pelo indice de estorvos que a colisao ja' usa, entao custa o mesmo
    /// que perguntar se cabe um corpo ali.
    pub fn coletaveis_em(&self, p: glam::Vec2, raio: f32, saida: &mut Vec<Coletavel>) {
        saida.clear();
        let (c0x, c0z) = self.celula(p.x - raio, p.y - raio);
        let (c1x, c1z) = self.celula(p.x + raio, p.y + raio);
        let raio_sq = raio * raio;
        for cz in c0z..=c1z {
            for cx in c0x..=c1x {
                let Some(c) = self.celula_em(cx, cz) else { continue };
                for &n in &self.grade[c] {
                    let e = self.estorvos[n as usize];
                    let tier = match e.tipo {
                        TipoDeEstorvo::Minerio(t) => t,
                        TipoDeEstorvo::Tronco => 0,
                        TipoDeEstorvo::Forracao => continue,
                    };
                    if e.centro.distance_squared(p) > raio_sq { continue }
                    // Um corpo grande entra em varias celulas: so' conta na
                    // que tem o centro dele, senao pedra grande vale por
                    // quatro e a densidade mente.
                    if self.celula(e.centro.x, e.centro.y) != (cx, cz) { continue }
                    saida.push(Coletavel { coluna: e.coluna, centro: e.centro, tier });
                }
            }
        }
    }

    /// Um corpo de raio `raio` cabe em `p` sem entrar em tronco, matacao ou
    /// toco?
    ///
    /// Flor, capim, arbusto e samambaia NAO entram nesta conta: eles cobrem o
    /// chao inteiro, e colidir com forracao transformaria o mundo em labirinto.
    pub fn sem_estorvo(&self, p: glam::Vec2, raio: f32) -> bool {
        let (c0x, c0z) = self.celula(p.x - raio, p.y - raio);
        let (c1x, c1z) = self.celula(p.x + raio, p.y + raio);
        for cz in c0z..=c1z {
            for cx in c0x..=c1x {
                let Some(c) = self.celula_em(cx, cz) else { continue };
                for &n in &self.grade[c] {
                    let e = self.estorvos[n as usize];
                    let d = raio + e.raio;
                    if e.centro.distance_squared(p) < d * d {
                        return false;
                    }
                }
            }
        }
        true
    }

    /// Da' pra ir de `a` a `b` com um corpo de raio `raio` sem esbarrar em
    /// tronco, matacao ou toco?
    ///
    /// Distancia PONTO-SEGMENTO e nao amostras ao longo da linha: um tronco
    /// de betula tem 0,18 de raio e caberia inteiro entre duas amostras. O
    /// custo e' o mesmo — o que decide e' quantos estorvos ha' por perto, e
    /// sao ~0,1% das colunas.
    /// Estorvo que engloba uma das PONTAS e' ignorado de proposito.
    ///
    /// Na origem, porque preso dentro de um tronco todo trecho seria invalido
    /// e nao haveria rota nenhuma — nem a de sair dali. No destino, porque o
    /// A* testa CENTRO DE CELULA: um centro que caiu dentro de um tronco
    /// viraria celula ilhada, e cada clique la' dentro custaria o orcamento
    /// inteiro de nos pra concluir que nao ha' caminho. Medido: 6,6 ms por
    /// pedido de rota curta, contra 0,5 ms quando ha' caminho.
    ///
    /// O que continua valendo — e e' o que importa — e' o tronco NO MEIO do
    /// trecho. Chegar ate' o pe' dele e' com o `mover_e_deslizar`, que
    /// contorna; atravessa-lo, nao.
    pub fn trecho_sem_estorvo(&self, a: glam::Vec2, b: glam::Vec2, raio: f32) -> bool {
        let folga = raio + self.raio_max_estorvo;
        let (c0x, c0z) = self.celula(a.x.min(b.x) - folga, a.y.min(b.y) - folga);
        let (c1x, c1z) = self.celula(a.x.max(b.x) + folga, a.y.max(b.y) + folga);
        let ab = b - a;
        let comp2 = ab.length_squared();
        for cz in c0z..=c1z {
            for cx in c0x..=c1x {
                let Some(c) = self.celula_em(cx, cz) else { continue };
                for &n in &self.grade[c] {
                    let e = self.estorvos[n as usize];
                    let t = if comp2 < 1e-12 {
                        0.0
                    } else {
                        ((e.centro - a).dot(ab) / comp2).clamp(0.0, 1.0)
                    };
                    let d = raio + e.raio;
                    if e.centro.distance_squared(a) < d * d
                        || e.centro.distance_squared(b) < d * d
                    {
                        continue;
                    }
                    if (a + ab * t).distance_squared(e.centro) < d * d {
                        return false;
                    }
                }
            }
        }
        true
    }

    /// Quantos estorvos a ilha tem. So' pra medir.
    /// Todos os corpos plantados. Pra medicao e pro servidor montar indice
    /// proprio — no jogo se pergunta pelo raio, nunca pela lista inteira.
    pub fn todos_os_estorvos(&self) -> &[Estorvo] { &self.estorvos }

    pub fn total_de_estorvos(&self) -> usize {
        self.estorvos.len()
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

    /// Da' pra chegar PULANDO? Ate' `PULO_BLOCOS` sim, acima disso nao — nao
    /// ha' escalada.
    pub fn pulo_ok(&self, de: (f32, f32), para: (f32, f32)) -> bool {
        self.subida(de, para).map_or(false, |s| s <= PULO_BLOCOS)
    }

    /// Pular faria o corpo andar mais que andar faria?
    ///
    /// A pergunta e' feita ao PROPRIO `mover_com_degrau`, duas vezes. Toda
    /// outra formulacao ja' discordou dele:
    ///
    ///   * comparar a coluna do centro ignorava o OMBRO — o centro via um
    ///     bloco, a amostra do lado via dois, e o corpo empurrava a quina pra
    ///     sempre com o teste dizendo "nao precisa pular";
    ///   * perguntar so' a `borda_livre` ignorava o ESTORVO — travado num
    ///     matacao, o relevo estava livre, e ninguem via que pular contornava
    ///     por cima do degrau ao lado.
    ///
    /// Duas chamadas a mais por tick por quem segue rota. E' barato, e e' a
    /// unica versao que nao pode divergir de quem move o corpo.
    pub fn precisa_pular(&self, pos: glam::Vec2, vel: glam::Vec2, dt: f32, raio: f32) -> bool {
        let passo = vel.length() * dt;
        if passo < 1e-4 {
            return false;
        }
        let andando = pos.distance(self.mover_com_degrau(pos, vel, dt, raio, DEGRAU_BLOCOS));
        // Andando bem? Nao ha' o que resolver.
        if andando > passo * 0.5 {
            return false;
        }
        let pulando = pos.distance(self.mover_com_degrau(pos, vel, dt, raio, PULO_BLOCOS));
        pulando > andando * 2.0 + 1e-4
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
        self.mover_com_degrau(pos, vel, dt, raio, DEGRAU_BLOCOS)
    }

    /// O mesmo, dizendo quanto o corpo consegue subir.
    ///
    /// E' assim que o pulo entra: ele nao muda a fisica, muda o DEGRAU — de um
    /// bloco pra dois. Nao ha' gravidade nem velocidade vertical em lugar
    /// nenhum, e nao precisa haver: num mundo de blocos, "pular" e' aceitar um
    /// degrau mais alto por meio segundo.
    pub fn mover_com_degrau(
        &self,
        pos: glam::Vec2,
        vel: glam::Vec2,
        dt: f32,
        raio: f32,
        degrau: i32,
    ) -> glam::Vec2 {
        // Amostra o DESTINO, nao o caminho: um passo maior que um bloco
        // atravessaria uma parede de uma coluna so'. A 30Hz e velocidade de
        // jogador o passo e' ~0,13 de unidade contra blocos de 0,5, entao
        // sobra folga de quase quatro vezes — se um dia houver corrida ou
        // arranco, isto vira varredura.
        let v = vel * dt;
        let mut p = pos;
        // ── TRONCO: desliza pela TANGENTE, nao pelos eixos ──
        //
        // Contra uma PAREDE, separar em X e Y funciona: um dos dois esta'
        // livre. Contra um CIRCULO, nao — quem vem de frente tem os dois
        // eixos quase bloqueados e anda tres milimetros por tick, empurrando
        // o tronco pra sempre. Medido: era a metade dos travamentos.
        //
        // A saida e' a resposta certa pra circulo: projetar o passo na
        // tangente do estorvo, do lado pra onde o corpo ja' ia. Ele contorna
        // a arvore sem parar, que e' o que o olho espera.
        let alvo_inteiro = pos + v;
        if let Some(e) = self.estorvo_em(alvo_inteiro, raio) {
            let fora = (pos - e.centro).normalize_or_zero();
            if fora != glam::Vec2::ZERO {
                let tang = glam::Vec2::new(-fora.y, fora.x);
                let lado = if tang.dot(v) >= 0.0 { tang } else { -tang };
                let alvo = pos + lado * v.length();
                if self.borda_livre(pos, alvo, raio, lado, degrau)
                    && self.sem_estorvo(alvo, raio)
                {
                    return alvo;
                }
            }
        }
        // Eixo a eixo pro RELEVO: barrado num eixo, continua andando no outro.
        if v.x != 0.0 {
            let alvo = glam::Vec2::new(p.x + v.x, p.y);
            if self.borda_livre(p, alvo, raio, glam::Vec2::new(v.x.signum(), 0.0), degrau)
                && self.sem_estorvo(alvo, raio)
            {
                p.x = alvo.x;
            }
        }
        if v.y != 0.0 {
            let alvo = glam::Vec2::new(p.x, p.y + v.y);
            if self.borda_livre(p, alvo, raio, glam::Vec2::new(0.0, v.y.signum()), degrau)
                && self.sem_estorvo(alvo, raio)
            {
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
        degrau: i32,
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
            // Descer e' livre; subir so' o degrau permitido.
            if h - base > degrau {
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
        // Terra E livre. Nascer dentro de um tronco nao e' detalhe estetico:
        // o corpo fica preso, e o A* — que testa o corredor a partir de onde
        // se esta' — nao acha rota nenhuma. O sintoma aparece como "nao ha'
        // caminho pra lugar nenhum" e nao como "spawn ruim".
        let bom = |p: glam::Vec2| {
            !self.agua(p.x, p.y) && self.sem_estorvo(p, crate::constants::ENTITY_RADIUS)
        };
        let aqui = glam::Vec2::new(x, z);
        if bom(aqui) {
            return aqui;
        }
        let passos = (limite_un / BLOCO) as i32;
        for r in 1..=passos {
            // So' o anel de raio r: o miolo ja' foi visto na volta anterior.
            for i in -r..=r {
                for (dx, dz) in [(i, -r), (i, r), (-r, i), (r, i)] {
                    let p = glam::Vec2::new(x + dx as f32 * BLOCO, z + dz as f32 * BLOCO);
                    if bom(p) {
                        return p;
                    }
                }
            }
        }
        aqui
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
        // O ponto de uma celula nao e' sempre o centro dela: se houver um
        // tronco ali, ele anda pro lado. Guardado num mapa porque a mesma
        // celula e' visitada como vizinha de varias outras.
        let bruto = |c: (i32, i32)| -> glam::Vec2 {
            glam::Vec2::new(
                c.0 as f32 * BLOCO * PASSO_CAMINHO as f32,
                c.1 as f32 * BLOCO * PASSO_CAMINHO as f32,
            )
        };
        // ── A PORTA DO GRAFO ──
        //
        // O corpo nunca esta' num centro de celula, e a celula que o contem
        // pode estar do outro lado de um paredao — literalmente: o corpo ao
        // pe' de um barranco de quatro blocos, o centro da celula la' em
        // cima. O A* saia do centro, aprovava o corredor de la', e a rota
        // mandava o corpo atravessar a parede na PRIMEIRA perna, que era a
        // unica que ninguem tinha validado.
        //
        // Entao a entrada no grafo e' escolhida: a celula mais proxima que o
        // corpo REALMENTE alcanca de onde esta'.
        let mut pontos: HashMap<(i32, i32), glam::Vec2> = HashMap::new();
        let contendo = cel(de);
        let mut inicio = contendo;
        {
            let mut melhor = f32::MAX;
            for dz in -1..=1 {
                for dx in -1..=1 {
                    let c = (contendo.0 + dx, contendo.1 + dz);
                    let p = *pontos.entry(c).or_insert_with(|| {
                        self.ponto_livre_perto(bruto(c), crate::constants::ENTITY_RADIUS)
                    });
                    let d = de.distance(p);
                    if d < melhor && self.trecho_livre(de, p, PULO_BLOCOS) {
                        melhor = d;
                        inicio = c;
                    }
                }
            }
        }
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
                let base = if dx != 0 && dz != 0 { 1414 } else { 1000 };
                // Anda? Custa o passo. Nao anda, mas PULA? Custa o passo mais
                // a espera entre pulos.
                //
                // O pulo entra na rota porque sem ele o A* trata todo barranco
                // de dois blocos como parede e da' a volta na ilha inteira —
                // ou nao acha caminho nenhum. Com ele, a rota atravessa; e o
                // seguidor pula sozinho ao chegar na quina.
                let pa = *pontos.entry(atual).or_insert_with(|| {
                    self.ponto_livre_perto(bruto(atual), crate::constants::ENTITY_RADIUS)
                });
                let pv = *pontos.entry(viz).or_insert_with(|| {
                    self.ponto_livre_perto(bruto(viz), crate::constants::ENTITY_RADIUS)
                });
                let passo = if self.trecho_livre(pa, pv, DEGRAU_BLOCOS) {
                    base
                } else if self.trecho_livre(pa, pv, PULO_BLOCOS) {
                    base + CUSTO_DO_PULO
                } else {
                    continue;
                };
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
        // `skip(1)` pula a celula de onde se saiu — menos quando a porta do
        // grafo e' outra celula, que aí ela e' um ponto que o corpo precisa
        // andar de verdade.
        let pular = if inicio == contendo { 1 } else { 0 };
        let mut saida: Vec<glam::Vec2> = rota
            .into_iter()
            .skip(pular)
            .map(|c| {
                *pontos.entry(c).or_insert_with(|| {
                    self.ponto_livre_perto(bruto(c), crate::constants::ENTITY_RADIUS)
                })
            })
            .collect();
        if melhor.0 == fim {
            // O ultimo ponto quer ser o destino de VERDADE e nao o centro da
            // celula — mas so' se der pra chegar la'.
            //
            // Trocar sem checar era um furo: a rota valida celula a celula, e
            // o ponto clicado pode estar num degrau de dois blocos ao lado do
            // centro. A rota passava inteira no teste e o boneco descobria o
            // problema no ultimo passo, empurrando a parede.
            let penultimo = if saida.len() >= 2 {
                saida[saida.len() - 2]
            } else {
                de
            };
            saida.pop();
            if self.trecho_livre(penultimo, para, PULO_BLOCOS) {
                saida.push(para);
            } else {
                // Nao da' pra pisar onde clicou: para no centro da celula, que
                // e' o mais perto validado. Chegar perto e' melhor que chegar
                // e travar.
                saida.push(*pontos.entry(fim).or_insert_with(|| {
                    self.ponto_livre_perto(bruto(fim), crate::constants::ENTITY_RADIUS)
                }));
            }
        }
        Some(saida)
    }

    /// Da' pra ir do centro de uma celula grossa ao da vizinha?
    ///
    /// Checa as colunas FINAS no meio do caminho: um passo de oito blocos
    /// esconderia um paredao de oito blocos, e a rota mandaria o jogador
    /// andar contra a parede pra sempre.
    ///
    /// O numero de amostras sai da DISTANCIA, nao de uma constante. Com oito
    /// fixas, um trecho diagonal (11,3 blocos) era amostrado a cada 2,7 —
    /// dois blocos inteiros passavam despercebidos entre uma amostra e a
    /// seguinte, e a rota atravessava degrau de dois.
    ///
    /// E cada amostra checa o CORPO, nao um ponto. A primeira versao validava
    /// a linha central: corredor que cabe pra um ponto mas nao pra um circulo
    /// de raio 0,35 virava rota valida, e o jogador travava no ombro. Medido
    /// simulando a caminhada: **71 de 240 rotas travavam**.
    fn trecho_livre(&self, de: glam::Vec2, para: glam::Vec2, degrau: i32) -> bool {
        // Tronco, matacao e toco derrubam o trecho inteiro. Sem isto a rota
        // atravessa a arvore, o corpo bate nela e o seguidor fica raspando de
        // lado ate' a paciencia acabar.
        if !self.trecho_sem_estorvo(de, para, crate::constants::ENTITY_RADIUS) {
            return false;
        }
        let n = ((de.distance(para) / BLOCO).ceil() as i32).max(1);
        let dir = (para - de).normalize_or_zero();
        // Um pouco mais largo que o corpo: rota que passa raspando na quina
        // trava assim que o jogador for empurrado um centimetro pro lado.
        let perp = glam::Vec2::new(-dir.y, dir.x) * (crate::constants::ENTITY_RADIUS * 1.15);
        let mut anterior = de;
        for i in 1..=n {
            let t = i as f32 / n as f32;
            let p = de + (para - de) * t;
            for lado in [glam::Vec2::ZERO, perp, -perp] {
                let sobe = self.subida(
                    (anterior.x + lado.x, anterior.y + lado.y),
                    (p.x + lado.x, p.y + lado.y),
                );
                if !sobe.is_some_and(|s| s <= degrau) {
                    return false;
                }
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
        Some(Self::com_blocos(
            semente,
            raio_blocos,
            bioma,
            escala_altura,
            lado,
            blocos,
            Gerador::novo(semente, raio_blocos, bioma, escala_altura),
        ))
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

// ─────────────────────────── seguidor de rota ────────────────────────

/// Conduz o corpo pelos pontos da rota, e DESISTE de um ponto que nao da' pra
/// alcancar.
///
/// A desistencia e' o que faltava: numa quina os dois eixos barram, o corpo
/// para, e como o destino nao muda ele empurra a parede pra sempre. Medido
/// simulando a caminhada, **35 de 240 rotas travavam** so' por isso — o
/// caminho estava certo, faltava o seguidor admitir que aquele ponto nao vai
/// rolar e ir pro proximo.
///
/// Mora aqui e nao no servidor porque assim o teste exercita o codigo DE
/// VERDADE: seguidor testado e' seguidor que nao trava.
#[derive(Debug, Default, Clone)]
pub struct SeguidorDeRota {
    pontos: std::collections::VecDeque<glam::Vec2>,
    destino: glam::Vec2,
    sem_avanco: u32,
    melhor: f32,
    travado: bool,
}

impl SeguidorDeRota {
    /// Quantos ticks sem se aproximar antes de considerar a rota velha. Meio
    /// segundo a 30Hz — curto o bastante pra ninguem ver, longo o bastante pra
    /// nao desistir de um contorno legitimo.
    const PACIENCIA: u32 = 15;
    /// Distancia pra considerar o ponto alcancado.
    const CHEGOU: f32 = 0.6;

    pub fn nova(rota: impl IntoIterator<Item = glam::Vec2>, destino: glam::Vec2) -> Self {
        Self {
            pontos: rota.into_iter().collect(),
            destino,
            sem_avanco: 0,
            melhor: f32::MAX,
            travado: false,
        }
    }

    pub fn vazia(&self) -> bool {
        self.pontos.is_empty()
    }

    /// Quantos pontos ainda faltam. So' pra observabilidade.
    pub fn restantes(&self) -> usize {
        self.pontos.len()
    }

    pub fn limpa(&mut self) {
        self.pontos.clear();
        self.travado = false;
    }

    /// Pra onde o jogador mandou ir. E' o que a rota nova persegue.
    pub fn destino(&self) -> glam::Vec2 {
        self.destino
    }

    /// A rota travou e precisa ser refeita a partir de onde o corpo esta'.
    ///
    /// Antes, quem travava DESCARTAVA o ponto e seguia pro proximo. Parecia
    /// recuperacao e era o contrario: o ponto seguinte esta' mais longe que o
    /// que ja' nao dava, entao ele tambem trava, e a rota inteira evapora em
    /// meio segundo. O boneco "chegava" parado no meio do caminho. Medido
    /// varrendo a ilha: **109 de 364 rotas**.
    ///
    /// Rota velha nao se remenda, se refaz: o mundo em volta do corpo agora e'
    /// outro, e quem sabe achar caminho e' o A*.
    pub fn travado(&self) -> bool {
        self.travado
    }

    /// Direcao pro proximo ponto, ou `None` quando a rota acabou.
    pub fn direcao(&mut self, pos: glam::Vec2) -> Option<glam::Vec2> {
        loop {
            let alvo = *self.pontos.front()?;
            let d = pos.distance(alvo);
            if d <= Self::CHEGOU {
                self.avanca();
                continue;
            }
            // Aproximou? Zera a paciencia. Senao, gasta.
            if d < self.melhor - 0.02 {
                self.melhor = d;
                self.sem_avanco = 0;
            } else {
                self.sem_avanco += 1;
                if self.sem_avanco > Self::PACIENCIA {
                    self.travado = true;
                }
            }
            // Continua empurrando mesmo travado: quem pede rota nova e' o
            // servidor, e ate' ela chegar e' melhor raspar na quina que parar.
            return (alvo - pos).try_normalize();
        }
    }

    fn avanca(&mut self) {
        self.pontos.pop_front();
        self.sem_avanco = 0;
        self.melhor = f32::MAX;
        self.travado = false;
    }
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

    impl Ilha {
        /// Tira toda a vegetacao. Os testes de DEGRAU sao sobre o relevo — um
        /// tronco no meio do caminho testaria outra coisa, e um relevo
        /// achatado na mao planta uma floresta uniforme que nao existe em
        /// ilha nenhuma de verdade.
        fn pelada(&mut self) {
            self.estorvos.clear();
            self.raio_max_estorvo = 0.0;
            self.grade = vec![Vec::new(); self.grade_lado * self.grade_lado];
        }
    }

    /// Um bloco anda, ate' tres pula, quatro nao passa. E' a regra de
    /// movimento inteira, e ela cai fora se alguem mexer em `bloco()` sem
    /// perceber.
    #[test]
    fn um_anda_tres_pula_quatro_nao_passa() {
        let mut i = Ilha::gerar(3, 32, Bioma::Floresta, ESCALA_ALTURA);
        let l = i.lado;
        for c in i.blocos.iter_mut() {
            *c = 4;
        }
        i.pelada();
        let x = |ix: usize| (ix as i32 - (l / 2) as i32) as f32 * BLOCO;
        // tres degraus a leste do centro, de 1, 2 e 3 blocos
        let meio = l / 2;
        i.blocos[meio * l + meio + 1] = 5;
        i.blocos[meio * l + meio + 2] = 6;
        i.blocos[meio * l + meio + 3] = 7;
        i.blocos[meio * l + meio + 4] = 8;
        let z = 0.0;
        let p = |ix: usize| (x(ix), z);
        assert!(i.passo_ok(p(meio), p(meio + 1)), "1 bloco tem que andar");
        assert!(!i.passo_ok(p(meio), p(meio + 2)), "2 blocos nao anda");
        assert!(i.pulo_ok(p(meio), p(meio + 2)), "2 blocos tem que pular");
        assert!(i.pulo_ok(p(meio), p(meio + 3)), "3 blocos tem que pular");
        assert!(!i.pulo_ok(p(meio), p(meio + 4)), "4 blocos nao passa");
        // descer e' sempre livre
        assert!(i.passo_ok(p(meio + 4), p(meio)));
    }

    /// O arco desenhado tem que cobrir o degrau que o pulo vence. Se alguem
    /// subir `PULO_BLOCOS` sem mexer no arco, o boneco atravessa a quina e a
    /// altura aparece de um salto no fim do pulo.
    #[test]
    fn o_arco_cobre_o_degrau_maximo() {
        let degrau = PULO_BLOCOS as f32 * BLOCO;
        assert!(
            crate::constants::PULO_ALTURA > degrau,
            "arco de {} nao cobre degrau de {degrau}",
            crate::constants::PULO_ALTURA
        );
    }

    /// O pulo tem que VENCER o degrau que o passo recusa — e nao so' passar
    /// no `pulo_ok`. Quem move o corpo e' `mover_com_degrau`, e ele tem tres
    /// pontos de borda e raio: da' pra passar na regra e mesmo assim ficar
    /// preso na quina.
    #[test]
    fn o_pulo_vence_o_degrau_que_o_passo_recusa() {
        let mut i = Ilha::gerar(23, 32, Bioma::Floresta, ESCALA_ALTURA);
        let l = i.lado;
        for c in i.blocos.iter_mut() {
            *c = 4;
        }
        i.pelada();
        // Patamar ocupando toda a metade leste, na altura pedida.
        let patamar = |i: &mut Ilha, h: i16| {
            for iz in 0..l {
                for ix in l / 2..l {
                    i.blocos[iz * l + ix] = h;
                }
            }
        };
        let anda = |i: &Ilha, degrau: i32| {
            let mut p = glam::Vec2::new(-1.5, 0.0);
            let v = glam::Vec2::new(5.0, 0.0);
            for _ in 0..60 {
                p = i.mover_com_degrau(p, v, 1.0 / 30.0, crate::constants::ENTITY_RADIUS, degrau);
            }
            p.x
        };
        patamar(&mut i, 4 + PULO_BLOCOS as i16);
        let andando = anda(&i, DEGRAU_BLOCOS);
        let pulando = anda(&i, PULO_BLOCOS);
        assert!(andando < 0.0, "andando tem que parar antes do patamar, parou em {andando}");
        assert!(pulando > 1.0, "pulando tem que subir no patamar, parou em {pulando}");
        // Um bloco acima do teto do pulo continua sendo parede: sem isto,
        // "pula mais alto" viraria "escala qualquer coisa".
        patamar(&mut i, 4 + PULO_BLOCOS as i16 + 1);
        let alto_demais = anda(&i, PULO_BLOCOS);
        assert!(
            alto_demais < 0.0,
            "um bloco acima do teto do pulo tem que barrar, subiu ate' {alto_demais}"
        );
        patamar(&mut i, 4 + PULO_BLOCOS as i16);
        assert_eq!(
            i.altura(pulando, 0.0),
            (5 + PULO_BLOCOS) as f32 * BLOCO,
            "depois do pulo o apoio tem que ser o topo do patamar"
        );
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
        i.pelada();
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
        let (mut passos, mut pulos) = (0, 0);
        for alvo in &rota {
            // Cada trecho tem que ser vencivel de ponta a ponta — andando ou,
            // no maximo, pulando. O que NAO pode e' parede: rota que manda o
            // jogador subir quatro blocos nao e' rota, e' ordem impossivel.
            let n = 12;
            let mut a = p;
            for k in 1..=n {
                let t = k as f32 / n as f32;
                let b = p + (*alvo - p) * t;
                assert!(
                    i.pulo_ok((a.x, a.y), (b.x, b.y)),
                    "a rota atravessa terreno intransitavel em {b:?}"
                );
                if i.passo_ok((a.x, a.y), (b.x, b.y)) { passos += 1 } else { pulos += 1 }
                a = b;
            }
            p = *alvo;
        }
        println!("{passos} trechos andados, {pulos} pulados");
        // O pulo tem que ser EXCECAO na rota. Se ele virar regra, o custo do
        // pulo no A* parou de valer e o boneco passa a caminhar aos saltos.
        assert!(
            pulos * 4 < passos,
            "{pulos} pulos pra {passos} passos — o A* parou de preferir andar"
        );
        assert!(p.distance(para) < 12.0, "parou a {:.0} do destino", p.distance(para));
    }

    /// O jogador tem que conseguir ANDAR a rota inteira.
    ///
    /// Nao amostra pontos: simula o passo com a MESMA `mover_e_deslizar` que o
    /// servidor usa, a 30Hz, e verifica que ele chega. E' o unico teste que
    /// responde a pergunta que o jogador faz — "o boneco travou?" — e ele pega
    /// o que amostragem nao pega: a rota valida celula a celula mas o corpo
    /// tem raio, e quem anda e' o corpo.
    /// Anda a rota como o SERVIDOR anda: pula sozinho quando o trecho pede e
    /// pede rota nova quando o seguidor trava. Devolve onde parou.
    ///
    /// E' o unico jeito de o teste responder a pergunta que o jogador faz —
    /// "o boneco travou?" — porque cada uma dessas tres pecas passa sozinha e
    /// e' o conjunto que falha.
    fn simula_ida(i: &Ilha, de: glam::Vec2, para: glam::Vec2) -> glam::Vec2 {
        const REFAZ_MAX: u32 = 8;
        let dt = 1.0 / 30.0;
        let vel = 5.0;
        let Some(rota) = i.caminho(de, para, 20_000) else { return de };
        let mut seg = SeguidorDeRota::nova(rota.iter().copied(), para);
        let mut p = de;
        let (mut pulo_ate, mut pronto, mut agora) = (-1.0f32, 0.0f32, 0.0f32);
        let (mut refeitas, mut ultima_refeita) = (0u32, -1.0f32);
        for _ in 0..6_000 {
            let Some(dir) = seg.direcao(p) else { break };
            if seg.travado() && agora - ultima_refeita >= 0.2 {
                ultima_refeita = agora;
                refeitas += 1;
                if refeitas > REFAZ_MAX {
                    break;
                }
                match i.caminho(p, para, 20_000) {
                    Some(nova) => seg = SeguidorDeRota::nova(nova.iter().copied(), para),
                    None => break,
                }
                continue;
            }
            if i.precisa_pular(p, dir * vel, dt, 0.35) && agora >= pronto {
                pulo_ate = agora + crate::constants::PULO_DURACAO;
                pronto = pulo_ate + crate::constants::PULO_ESPERA;
            }
            let degrau = if agora < pulo_ate { PULO_BLOCOS } else { DEGRAU_BLOCOS };
            p = i.mover_com_degrau(p, dir * vel, dt, 0.35, degrau);
            agora += dt;
        }
        p
    }


    #[test]
    fn dbg_caso() {
        let d = &ARQUIPELAGO[0];
        let i = Ilha::gerar(d.semente, 800, d.bioma, ESCALA_ALTURA);
        let de = glam::Vec2::new(-59.09517, 80.6707);
        let para = glam::Vec2::new(-106.37446, 88.95736);
        let rota = i.caminho(de, para, 20_000).unwrap();
        println!("DBG rota {} pontos: {:?}", rota.len(), &rota[..rota.len().min(6)]);
        let mut seg = SeguidorDeRota::nova(rota.iter().copied(), para);
        let mut p = de;
        let dt = 1.0 / 30.0;
        let (mut pulo_ate, mut pronto, mut agora) = (-1.0f32, 0.0f32, 0.0f32);
        for k in 0..400 {
            let (dt, vel) = (1.0f32 / 30.0, 5.0f32);
            let Some(dir) = seg.direcao(p) else { println!("DBG rota acabou em {k}"); break };
            if seg.travado() {
                println!("DBG travado no tick {k} em {p:?}, alvo {:?}", seg.pontos.front());
                let bloco_aqui = i.bloco(i.coluna(p.x,p.y).0, i.coluna(p.x,p.y).1);
                let a = p + dir * 0.5;
                let bloco_la = i.bloco(i.coluna(a.x,a.y).0, i.coluna(a.x,a.y).1);
                println!("DBG   bloco aqui {bloco_aqui} / meio bloco a' frente {bloco_la}");
                println!("DBG   sem_estorvo aqui {} / a' frente {}",
                    i.sem_estorvo(p, 0.35), i.sem_estorvo(a, 0.35));
                println!("DBG   precisa_pular {}", i.precisa_pular(p, dir * vel, dt, 0.35));
                println!("DBG   dir {dir:?}");
                let est = i.estorvo_em(p + dir * 0.5, 0.35);
                println!("DBG   estorvo a frente {est:?}");
                let passo = i.mover_com_degrau(p, dir * 5.0, 1.0/30.0, 0.35, DEGRAU_BLOCOS);
                println!("DBG   mover andou {:.4}", p.distance(passo));
                let pulou = i.mover_com_degrau(p, dir * 5.0, 1.0/30.0, 0.35, PULO_BLOCOS);
                println!("DBG   mover pulando andou {:.4}", p.distance(pulou));
                if let Some(e) = est {
                    let fora = (p - e.centro).normalize_or_zero();
                    let t = glam::Vec2::new(-fora.y, fora.x);
                    let lado = if t.dot(dir) >= 0.0 { t } else { -t };
                    let alvo = p + lado * 0.167;
                    println!("DBG   tangente {lado:?} borda {} estorvo_livre {}",
                        i.borda_livre(p, alvo, 0.35, lado, DEGRAU_BLOCOS),
                        i.sem_estorvo(alvo, 0.35));
                    let cel = ((e.centro.x / (BLOCO * PASSO_CAMINHO as f32)).round(),
                               (e.centro.y / (BLOCO * PASSO_CAMINHO as f32)).round());
                    let cc = glam::Vec2::new(cel.0 * BLOCO * PASSO_CAMINHO as f32,
                                             cel.1 * BLOCO * PASSO_CAMINHO as f32);
                    println!("DBG   centro celula {cc:?} dist {:.2} raio {:.2}",
                        cc.distance(e.centro), e.raio);
                }
                println!("DBG   rota nova? {:?}", i.caminho(p, para, 20_000).map(|r| r.len()));
                break;
            }
            if i.precisa_pular(p, dir * vel, dt, 0.35) && agora >= pronto {
                pulo_ate = agora + crate::constants::PULO_DURACAO;
                pronto = pulo_ate + crate::constants::PULO_ESPERA;
            }
            let degrau = if agora < pulo_ate { PULO_BLOCOS } else { DEGRAU_BLOCOS };
            let antes = p;
            p = i.mover_com_degrau(p, dir * 5.0, dt, 0.35, degrau);
            if k % 20 == 0 { println!("DBG t{k} {p:?} andou {:.3}", antes.distance(p)); }
            agora += dt;
        }
    }

    /// O boneco CHEGA, partindo de qualquer lugar da ilha.
    ///
    /// Este teste substituiu um que media 240 rotas a partir de UM unico
    /// ponto — o desembarque. Ele dava 0 travadas e passava sempre, enquanto
    /// o jogo travava em 34% das idas. Teste de escopo estreito nao e' teste
    /// fraco: e' teste que mente, porque a confianca que ele da' e' real e a
    /// cobertura nao.
    ///
    /// Aqui a partida varre o disco inteiro (angulo de ouro, raios variados),
    /// e a simulacao e' a do servidor: rota, pulo automatico e rota refeita
    /// quando o seguidor trava.
    #[test]
    fn o_boneco_chega_partindo_de_qualquer_lugar() {
        let d = &ARQUIPELAGO[0];
        let i = Ilha::gerar(d.semente, 800, d.bioma, ESCALA_ALTURA);
        let dt = 1.0 / 30.0;
        let vel = 5.0;
        let (mut casos, mut sem_rota, mut parcial, mut travou_seguidor, mut ok) = (0, 0, 0, 0, 0);
        let mut exemplos: Vec<String> = Vec::new();
        for k in 0..400 {
            // Pontos de partida espalhados pela ilha, nao so' o desembarque.
            let a = k as f32 * 2.399_963; // angulo de ouro: cobre bem o disco
            let r = 20.0 + (k % 37) as f32 * 8.0;
            let de = i.terra_mais_proxima(a.cos() * r, a.sin() * r, 60.0);
            if i.agua(de.x, de.y) { continue; }
            let b = a * 3.7 + 1.1;
            let alcance = 12.0 + (k % 11) as f32 * 9.0;
            let para = de + glam::Vec2::new(b.cos() * alcance, b.sin() * alcance);
            if i.agua(para.x, para.y) { continue; }
            casos += 1;
            let Some(r0) = i.caminho(de, para, 20_000) else { sem_rota += 1; continue };
            let alcanca = r0.last().unwrap().distance(para) < 6.0;
            let p = simula_ida(&i, de, para);
            if !alcanca {
                parcial += 1;
                continue;
            }
            if p.distance(para) > 6.0 {
                travou_seguidor += 1;
                if exemplos.len() < 10 {
                    let (cx, cz) = i.coluna(p.x, p.y);
                    let (tx, tz) = i.coluna(para.x, para.y);
                    exemplos.push(format!(
                        "PAROU a {:.0} (andou {:.0}) | bloco {} -> {} | A* alcanca o alvo? {alcanca}",
                        p.distance(para), de.distance(p), i.bloco(cx, cz), i.bloco(tx, tz)));
                }
            } else {
                ok += 1;
            }
        }
        println!(
            "{casos} casos: {ok} chegaram, {sem_rota} sem rota, \
             {parcial} inalcancaveis, {travou_seguidor} travaram"
        );
        for e in exemplos.iter().take(4) {
            println!("  {e}");
        }
        assert!(casos > 300, "so' {casos} casos — a varredura encolheu");
        // Alvo inalcancavel (topo de paredao, ilhota isolada) nao e' falha: a
        // rota parcial leva o corpo ate' o pe' e para, que e' o certo. Falha e'
        // o A* dizer que chega e o corpo nao chegar.
        let alcancaveis = ok + travou_seguidor;
        assert!(
            travou_seguidor * 20 < alcancaveis,
            "{travou_seguidor} de {alcancaveis} rotas alcancaveis travaram (teto: 5%)"
        );
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

    /// A ilha tem que ter LUGAR bom e lugar ruim de coleta.
    ///
    /// Sem contraste nao existe spot, e sem spot nao existe disputa. Mede em
    /// vez de supor: densidade e' coisa que muda quando alguem mexe no
    /// relevo, e o dia em que a ilha ficar plana demais a coleta morre calada.
    #[test]
    fn a_ilha_tem_onde_coletar() {
        use crate::{COLETA_INTERVALO_BASE_S, COLETA_RAIO_SPOT};
        let d = &ARQUIPELAGO[0];
        let i = Ilha::gerar(d.semente, d.raio_blocos.min(800), d.bioma, ESCALA_ALTURA);
        let raio_un = i.raio_blocos as f32 * BLOCO;

        let (mut secos, mut n, mut melhor_veio, mut melhor_mata) = (0u32, 0u32, 0usize, 0usize);
        let mut achados = Vec::new();
        let passo = 3.0f32;
        let mut z = -raio_un;
        while z <= raio_un {
            let mut x = -raio_un;
            while x <= raio_un {
                let p = glam::Vec2::new(x, z);
                if !i.agua(p.x, p.y) {
                    i.coletaveis_em(p, COLETA_RAIO_SPOT, &mut achados);
                    n += 1;
                    if achados.is_empty() { secos += 1 }
                    melhor_veio = melhor_veio.max(achados.iter().filter(|c| c.tier > 0).count());
                    melhor_mata = melhor_mata.max(achados.iter().filter(|c| c.tier == 0).count());
                }
                x += passo;
            }
            z += passo;
        }
        let intervalo = |v: usize| COLETA_INTERVALO_BASE_S / v.max(1) as f32;
        println!(
            "amostras em terra {n} | sem nada {secos} ({:.0}%)\n\
             melhor veio: {melhor_veio} pedras -> {:.1}s por coleta\n\
             melhor mata: {melhor_mata} troncos -> {:.1}s por coleta",
            100.0 * secos as f32 / n as f32,
            intervalo(melhor_veio), intervalo(melhor_mata),
        );
        assert!(melhor_veio > 1, "nenhum veio com mais de uma pedra");
        assert!(melhor_mata > 1, "nenhuma mata com mais de um tronco");
        // Lugar seco tem que existir: se der pra coletar em qualquer lugar, o
        // spot nao vale nada e andar ate' ele nao significa nada.
        assert!(secos * 10 > n, "so' {secos} de {n} amostras sem recurso — o mapa inteiro e' spot");
    }

    #[test]
    fn a_ilha_tem_minerio_dos_quatro_tiers() {
        let d = &ARQUIPELAGO[0];
        let i = Ilha::gerar(d.semente, d.raio_blocos.min(800), d.bioma, ESCALA_ALTURA);
        let mut por_tier = [0u32; 5];
        let mut troncos = 0u32;
        for e in i.todos_os_estorvos() {
            match e.tipo {
                TipoDeEstorvo::Minerio(t) => por_tier[t as usize] += 1,
                TipoDeEstorvo::Tronco => troncos += 1,
                _ => {}
            }
        }
        println!("troncos {troncos} | pedras: cinza {} verde {} azul {} roxo {}",
            por_tier[1], por_tier[2], por_tier[3], por_tier[4]);
        for (t, nome) in [(1, "cinza"), (2, "verde"), (3, "azul"), (4, "roxo")] {
            assert!(por_tier[t] > 0, "ilha sem pedra {nome}");
        }
        // A escada tem que DESCER: pedra melhor tem que ser mais rara, senao
        // subir a montanha nao e' progressao, e' passeio.
        assert!(por_tier[1] > por_tier[2] && por_tier[2] > por_tier[3] && por_tier[3] > por_tier[4],
            "escada de raridade invertida: {:?}", &por_tier[1..]);
    }
}
