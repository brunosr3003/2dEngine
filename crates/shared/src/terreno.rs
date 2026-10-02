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

/// Degrau que se sobe ANDANDO: um bloco, como ladder.
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
        let mut est = (semente as u32)
            .wrapping_mul(1664525)
            .wrapping_add(1013904223);
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

pub fn suave(t: f32) -> f32 {
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
    /// Skyreach (`celeste`): soft gold-green meadows, marble cliffs, white
    /// trees with gold and blossom canopies. The owner: "everything needs to
    /// be more angelical".
    Celeste,
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
    /// degrau de 1 o patamar inteiro e' andavel como ladder; com 2 a borda
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
                terraco_blocos: 3,
                terraco_forca: 0.92,
            },
            // Gelo e' chapada com pico seco: muita planicie, e quando sobe,
            // sobe de vez. O terraco largo le' como placa de gelo.
            Bioma::Gelo => PerfilDeRelevo {
                planicie: (4.0, 3.0),
                colina: (7.0, 12.0),
                serra: (12.0, 24.0),
                corte: [0.50, 0.58, 0.70, 0.80],
                terraco_blocos: 3,
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
                colina: (14.0, 14.0),
                serra: (18.0, 26.0),
                // A serra domina, mas nao sozinha: com o corte em 0,18 quase
                // metade da terra colapsava num nivel so' e a ilha virava uma
                // mesa — 46% das colunas na mesma faixa de altura.
                corte: [0.34, 0.46, 0.62, 0.74],
                // Degrau grande e' o que separa PLANALTO de morro, mas a
                // forca fica abaixo do gelo: 0,95 com degrau de 6 nivelava
                // tudo.
                terraco_blocos: 3,
                terraco_forca: 0.85,
            },
            // The sky islands are drawn (`celeste`), not rolled: this only
            // matters for code that asks every biome for a profile.
            Bioma::Celeste => PerfilDeRelevo {
                planicie: (5.0, 4.0),
                colina: (8.0, 15.0),
                serra: (12.0, 30.0),
                corte: [0.42, 0.52, 0.66, 0.78],
                terraco_blocos: 3,
                terraco_forca: 0.92,
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
        let sem_enseada = |tipo, a, b| Self {
            tipo,
            ang,
            a,
            b,
            enseada: None,
        };

        // O DISCO leva a maior fatia: se toda ilha tivesse feitio marcante, o
        // marcante viraria o normal e nada se destacaria.
        if d < 34 {
            sem_enseada(Feitio::Disco, 0.0, 0.0)
        } else if d < 51 {
            sem_enseada(Feitio::Comprida, 0.98, 0.40 + r.float() * 0.08)
        } else if d < 68 {
            sem_enseada(
                Feitio::Enseada,
                0.62 + r.float() * 0.16,
                0.50 + r.float() * 0.14,
            )
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
            Feitio::Comprida => (u * u / (self.a * self.a) + v * v / (self.b * self.b)).sqrt(),
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
        let Some((ex, ez, raio)) = self.enseada else {
            return 0.0;
        };
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
        let planicie =
            perfil.planicie.0 + perfil.planicie.1 * n01(p.fbm(sx * 0.9, sz * 0.9, 2, 0.5));
        let colina =
            perfil.colina.0 + perfil.colina.1 * n01(p.fbm(sx * 1.5 + 7.0, sz * 1.5 - 4.0, 4, 0.45));
        let serra = perfil.serra.0 + perfil.serra.1 * p.crista(sx * 1.2 + 19.0, sz * 1.2 + 31.0, 4);

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
    /// Chao dos povoados: pedra da praca e do caminho, terra batida do
    /// caminho e a grama cuidada de dentro da cidade.
    Calcada,
    CalcadaEscura,
    Caminho,
    GramaCuidada,
    /// Skyreach's cloud paths (`celeste`): the deck players walk on between
    /// islands, and its shaded underside.
    Nuvem,
    NuvemSombra,
    /// The sky biome's palette (`Bioma::Celeste`).
    GramaCeleste,
    Marmore,
    MarmoreSombra,
    TroncoBranco,
    FolhaDourada,
    FolhaCeleste,
    PetalaRosa,
    CristalCeu,
}

impl Material {
    /// Todos, na ordem em que foram declarados.
    ///
    /// Existe pra guardar material num `u8` e ler de volta sem tabela
    /// paralela. Ja' custou caro: uma tabela com subconjunto dos materiais fez
    /// `Tronco` (discriminante 11) cair fora dela, e todo tronco e toda folha
    /// da vegetacao viraram voxel INVISIVEL — solido pra colisao de face,
    /// vazio pra malha. O mundo ficou coberto de pedra e mais nada.
    pub const TODOS: [Material; 39] = [
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
        Material::Calcada,
        Material::CalcadaEscura,
        Material::Caminho,
        Material::GramaCuidada,
        Material::Nuvem,
        Material::NuvemSombra,
        Material::GramaCeleste,
        Material::Marmore,
        Material::MarmoreSombra,
        Material::TroncoBranco,
        Material::FolhaDourada,
        Material::FolhaCeleste,
        Material::PetalaRosa,
        Material::CristalCeu,
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
            Material::Nuvem => (244, 247, 252),
            Material::NuvemSombra => (206, 216, 232),
            Material::GramaCeleste => (168, 210, 134),
            Material::Marmore => (236, 230, 214),
            Material::MarmoreSombra => (200, 192, 176),
            Material::TroncoBranco => (238, 234, 224),
            Material::FolhaDourada => (246, 214, 112),
            Material::FolhaCeleste => (196, 236, 196),
            Material::PetalaRosa => (246, 186, 214),
            Material::CristalCeu => (150, 214, 252),
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
            Material::Calcada => (178, 170, 156),
            Material::CalcadaEscura => (134, 126, 116),
            Material::Caminho => (172, 142, 102),
            Material::GramaCuidada => (98, 178, 84),
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
        return if bioma == Bioma::Celeste {
            // The sky islands' ruins and cliffs are marble, not grey rock.
            Material::Marmore
        } else if bioma == Bioma::Deserto {
            Material::Arenito
        } else {
            Material::Rocha
        };
    }
    match bioma {
        Bioma::Floresta => {
            if altura < 1.0 {
                Material::AreiaMolhada
            } else if altura < 2.6 {
                Material::Areia
            } else if altura < 24.0 {
                grama(mancha)
            } else if altura < 31.0 {
                Material::Rocha
            } else {
                Material::Neve
            }
        }
        // Nenhuma faixa de grama em lugar nenhum: e' essa AUSENCIA que se
        // reconhece de longe.
        Bioma::Gelo => {
            if altura < 2.6 {
                Material::Gelo
            } else if altura < 30.0 {
                Material::Neve
            } else {
                Material::Rocha
            }
        }
        // Duna nao tem faixa alta: o Ermo so' se le' se a parte de cima virar
        // MESA. Areia ate' a meia altura, arenito acima, rocha no topo.
        Bioma::Deserto => {
            if altura < 1.0 {
                Material::AreiaMolhada
            } else if altura < 5.0 {
                Material::Areia
            } else if altura < 9.5 {
                Material::Arenito
            } else {
                Material::Rocha
            }
        }
        // The sky meadows: grass everywhere the island is (its tops stay in
        // a narrow band), marble where it gets steep (handled above).
        Bioma::Celeste => {
            if mancha < 0.06 {
                Material::FolhaCeleste
            } else {
                Material::GramaCeleste
            }
        }
        // A grama para na metade da altura: o que sobra e' paredao. Ilha de
        // rocha com vale verde no pe', que e' o que uma serra e' vista de
        // baixo.
        Bioma::Montanha => {
            if altura < 1.0 {
                Material::AreiaMolhada
            } else if altura < 2.6 {
                Material::Areia
            } else if altura < 14.0 {
                grama(mancha)
            } else if altura < 30.0 {
                Material::Rocha
            } else {
                Material::Neve
            }
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
    // The sky islands' cliffs are marble in courses, never brown earth.
    if bioma == Bioma::Celeste {
        return if prof == 0 { topo } else if prof % 3 == 0 { Material::MarmoreSombra } else { Material::Marmore };
    }
    if prof >= 3 {
        return Material::Rocha;
    }
    if prof == 0 {
        return topo;
    }
    match bioma {
        Bioma::Deserto => Material::Areia,
        Bioma::Gelo => {
            if altura < 3.0 {
                Material::Gelo
            } else {
                Material::Neve
            }
        }
        _ => {
            if altura < 3.0 {
                Material::Areia
            } else {
                Material::Terra
            }
        }
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
    // New species go at the END: the discriminant indexes the client's model
    // table and seeds each tree's shape, so inserting above would reshape
    // every forest in the game.
    /// Sky biome: white trunk, gold and white blossom canopy.
    Sagrada,
    /// Sky biome: white trunk, pale canopy with strands hanging down.
    Salgueiro,
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
    /// Sky biome: a white lily with a gold heart.
    Lirio,
    /// Sky biome: a tall, pale, curved plume.
    Pena,
    /// Sky biome: a cluster of blue sky crystal (blocks the way, like a stone).
    CristalCeu,
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
        // A meadow: dense, so the lilies and plumes carry the look.
        Bioma::Celeste => 7.0,
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
            if f < 0.28 {
                Planta::Moita
            }
            // Flor com peso alto: no verde ela some, entao "algumas flores"
            // por metro quadrado vira nenhuma flor na tela.
            else if f < 0.52 {
                Planta::Flor
            } else if f < 0.72 {
                Planta::Samambaia
            } else if f < 0.88 {
                Planta::Arbusto
            } else if f < 0.96 {
                Planta::Pedra
            } else {
                Planta::Toco
            }
        }
        Bioma::Gelo => {
            if f < 0.62 {
                Planta::Pedra
            } else if f < 0.90 {
                Planta::Arbusto
            } else {
                Planta::Moita
            }
        }
        Bioma::Deserto => {
            if f < 0.46 {
                Planta::Talo
            } else if f < 0.78 {
                Planta::Arbusto
            } else {
                Planta::Pedra
            }
        }
        Bioma::Montanha => {
            if f < 0.70 {
                Planta::Pedra
            } else if f < 0.88 {
                Planta::Moita
            } else {
                Planta::Arbusto
            }
        }
        // White and gold flowers and pale plumes; crystal instead of stone.
        Bioma::Celeste => {
            if f < 0.30 {
                Planta::Lirio
            } else if f < 0.52 {
                Planta::Pena
            } else if f < 0.74 {
                Planta::Moita
            } else if f < 0.90 {
                Planta::Flor
            } else {
                Planta::CristalCeu
            }
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
        // Groves, not a forest: enough that the white trees carry the look,
        // sparse enough that each stands out against the meadow.
        Bioma::Celeste => 1.0,
    }
}

/// A especie que sai deste sorteio. `f` e' 0..1.
pub fn especie_de_arvore(bioma: Bioma, f: f32) -> Arvore {
    match bioma {
        Bioma::Floresta => {
            if f < 0.62 {
                Arvore::Copada
            } else if f < 0.88 {
                Arvore::Betula
            } else if f < 0.97 {
                Arvore::Pinheiro
            } else {
                Arvore::Seca
            }
        }
        Bioma::Gelo => {
            if f < 0.90 {
                Arvore::Pinheiro
            } else {
                Arvore::Seca
            }
        }
        Bioma::Deserto => {
            if f < 0.85 {
                Arvore::Seca
            } else {
                Arvore::Copada
            }
        }
        Bioma::Montanha => {
            if f < 0.82 {
                Arvore::Pinheiro
            } else if f < 0.95 {
                Arvore::Betula
            } else {
                Arvore::Seca
            }
        }
        Bioma::Celeste => {
            if f < 0.62 {
                Arvore::Sagrada
            } else {
                Arvore::Salgueiro
            }
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
            | Material::GramaCeleste
            | Material::FolhaCeleste
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
        Arvore::Copada | Arvore::Sagrada | Arvore::Salgueiro => 0.38,
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
        // A crystal cluster blocks like a stone of its size.
        Planta::CristalCeu => Some(0.5),
        Planta::Moita | Planta::Flor | Planta::Arbusto | Planta::Samambaia | Planta::Talo
        | Planta::Lirio | Planta::Pena => None,
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
    // NADA NA PONTE da Ilha Magica: ela e' estreita e um tronco a fecha.
    if ger.na_ponte_magica(bx, bz) {
        return None;
    }
    // Na maquete, METADE das arvores. Elas voltam em porte logo abaixo.
    // Na Ilha Magica, um TERCO: ela e' de passagem, e o que ela da' por no'
    // ja' vale o dobro.
    let rala = if ger.e_maquete() {
        0.5
    } else if ger.e_magica() {
        0.33
    } else if ger.e_arena() {
        // UM DÉCIMO na Arena. O dono: "pode ser uma ilhota bem pequena com bem
        // pouco recurso, só pra ser visualmente agradável, e com mais flores
        // etc que pedra e árvore".
        //
        // Não é zero: uma ilhota careca lê como chão de teste. Algumas árvores
        // esparsas dão silhueta sem encher o chão de contorno pro A*.
        //
        // UM DÉCIMO ainda era mato: a primeira subida saiu com 1.529 troncos e
        // matacões — uma árvore a cada seis unidades. O teste não pegou porque
        // ele mede FRAÇÃO das amostras secas (0,58%, dentro do teto de 2%), e
        // fração pequena numa ilhota inteira ainda é floresta. O número
        // absoluto do log foi quem disse.
        0.02
    } else {
        1.0
    };
    // No trees in or on a Porão (`planta`): a trunk on the rim would lean
    // over the hall. Past its rock, the plain grows as always.
    if ger.e_arena() {
        let p = glam::Vec2::new(bx as f32 * BLOCO, bz as f32 * BLOCO);
        if crate::planta::complexo_em(p).is_some_and(|pl| pl.bloco(p, 0).is_some()) {
            return None;
        }
    }
    // THE OASIS GROVE: the one thick wood of the Ermo (`oasis`).
    let bosque = ger.oasis().is_some_and(|o| o.no_bosque(bx, bz));
    let prob = if bosque {
        crate::oasis::DENSIDADE_DO_BOSQUE * 0.0025
    } else {
        densidade_de_arvore(bioma) * 0.0025 * rala // coluna = 0,25 m²
    };
    let h0 = (bx as u32).wrapping_mul(374_761_393) ^ (bz as u32).wrapping_mul(668_265_263);
    let h1 = h0.wrapping_mul(1_274_126_177);
    if (h1 >> 8) as f32 / (1u32 << 24) as f32 >= prob || agua || ger.na_cidade(bx, bz) {
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
    if !solo_vivo(material_variado(
        bioma,
        y,
        declive,
        false,
        ger.mancha(bx, bz),
    )) {
        return None;
    }
    // Campos de energia e minas são clareiras exclusivas do recurso.
    if ger.zona_de_coleta(bx, bz, topo).is_some_and(|tipo| tipo != 0) {
        return None;
    }
    Some(ArvorePlantada {
        // Green trees at the oasis, not the dead ones of the dunes.
        especie: especie_de_arvore(
            if bosque { Bioma::Floresta } else { bioma },
            (h2 >> 20) as f32 / 4096.0,
        ),
        // Desvio dentro da coluna: sem ele as arvores nascem todas no centro
        // do bloco e o bosque vira grade.
        centro: glam::Vec2::new(
            bx as f32 * BLOCO + ((h0 >> 4 & 0xff) as f32 / 255.0 - 0.5) * BLOCO * 1.6,
            bz as f32 * BLOCO + ((h1 >> 4 & 0xff) as f32 / 255.0 - 0.5) * BLOCO * 1.6,
        ),
        // Porte menor do que parece certo em pe': a camera olha de cima, e
        // arvore de tres vezes o jogador esconde o mob que ele veio cacar.
        // Na maquete elas crescem um terco: metade das arvores, cada uma
        // maior, da a mesma mata com metade das pecas na tela.
        porte: (0.62 + ((h0 >> 12) & 0xff) as f32 / 255.0 * 0.34)
            * if ger.e_maquete() { 1.34 } else { 1.0 },
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
    // NADA NA PONTE: pedra de forracao tambem barra passagem.
    if ger.na_ponte_magica(bx, bz) {
        return None;
    }
    // UM QUARTO das plantas na maquete. A forracao e' o que mais suja: sao
    // centenas de pontinhos de flor que, de longe, leem como chiado na grama.
    // A forracao e' o que mais suja de longe, e na Ilha Magica ela nao serve
    // pra nada: ninguem vai la' catar florzinha em noventa minutos.
    let rala = if ger.e_maquete() {
        0.25
    } else if ger.e_magica() {
        0.25
    } else if ger.e_arena() {
        // O DOBRO da forração na Arena, e de propósito: ela é o que sobra
        // depois de tirar árvore e pedra, e é ela que faz a ilhota parecer um
        // lugar em vez de um tabuleiro. Flor não barra passagem, então não
        // custa nada ao A*.
        2.0
    } else {
        1.0
    };
    // Nothing grows on a Porão's island (`planta`) — the owner: "i dont want
    // flowers and such".
    if ger.e_arena() {
        let p = glam::Vec2::new(bx as f32 * BLOCO, bz as f32 * BLOCO);
        if crate::planta::complexo_em(p).is_some_and(|pl| pl.bloco(p, 0).is_some()) {
            return None;
        }
    }
    // The oasis shore grows the Bosque's flowers and ferns, not desert stalks.
    let bioma = if ger.oasis().is_some_and(|o| o.no_verde(bx, bz)) {
        Bioma::Floresta
    } else {
        bioma
    };
    let prob = densidade_de_planta(bioma) * 0.0025 * rala;
    let g0 = (bx as u32).wrapping_mul(1_597_334_677) ^ (bz as u32).wrapping_mul(2_246_822_519);
    let g1 = g0.wrapping_mul(2_654_435_761);
    if (g1 >> 8) as f32 / (1u32 << 24) as f32 >= prob || agua || ger.na_cidade(bx, bz) {
        return None;
    }
    // Mesma armadilha das arvores: hash proprio pra especie.
    let g2 = g1.wrapping_mul(1_597_334_677).wrapping_add(2_246_822_519);
    let y = (topo + 1) as f32 * BLOCO;
    let solo = material_variado(bioma, y, declive, false, ger.mancha(bx, bz));
    let especie = especie_de_planta(bioma, (g2 >> 20) as f32 / 4096.0);
    if matches!(especie, Planta::Pedra) && ger.e_magica()
        && crate::magica::sem_pedras(glam::Vec2::new(bx as f32 * BLOCO, bz as f32 * BLOCO)) {
        return None;
    }

    // NA ARENA, SÓ O QUE NÃO BARRA PASSAGEM.
    //
    // Dobrar a forração pra ter "mais flores etc que pedra e árvore" dobrou
    // junto o MATACÃO e o TOCO, que são forração mas barram (`raio_de_planta`)
    // — e o log entregou: 1.461 troncos e matacões na ilhota depois de eu já
    // ter raleado as árvores a um cinquentavo. Os estorvos nunca tinham sido
    // árvore; eram pedra de chão, e eu tinha acabado de dobrá-los.
    if raio_de_planta(especie).is_some()
        && (ger.e_arena() || ger.zona_de_coleta(bx, bz, topo).is_some_and(|tipo| tipo != 0)) {
        return None;
    }
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
    /// A Energia usa este mesmo formato, mas uma grade de geracao separada.
    pub energia: bool,
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
pub const RAIO_DE_MINERIO: f32 = 0.5;

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
///
/// As faixas caem ENTRE os patamares do relevo, nunca em cima de um: faixa
/// cortando um patamar ao meio poria pedra de duas cores no mesmo cume. O
/// corte do roxo subiu pra 0,82 quando o terraco desceu de 4 pra 3 blocos
/// (degrau de 4 e' parede que nem pulando vence): com 0,78 o topo engolia a
/// faixa azul e a ladder de raridade invertia.
pub fn tier_de_minerio(altura: f32, pico: f32) -> u8 {
    let t = altura / pico.max(1.0);
    if t < 0.57 {
        1
    } else if t < 0.73 {
        2
    } else if t < 0.82 {
        3
    } else {
        4
    }
}

/// A pedra de minerio desta coluna, se houver.
///
/// Mesmo contrato de `arvore_da_coluna`: funcao pura do par de coordenadas,
/// rodada identica nos dois lados. O cliente desenha, o servidor barra
/// passagem e conta coleta — e nenhum byte de pedra viaja no fio.
///
/// Os filtros vao do barato pro caro, e a ORDEM so' muda o custo, nao a
/// resposta: sao todos condicoes de "e", sobre funcoes puras.
///
///  1. altura de montanha         — conta de uma coluna so';
///  2. espacamento                — hash dos vizinhos, sem ruido;
///  3. chao limpo                 — o quadrado em volta todo na mesma altura;
///  4. cume                       — nada bem mais alto por perto.
///
/// Os dois ultimos sao o que tira a pedra do degrau e da quina: eles olham
/// as colunas VIZINHAS, pelo mesmo `bloco_em` que o cliente usa pra desenhar
/// o chao, entao os dois lados continuam chegando na mesma resposta.
pub fn minerio_da_coluna(
    bioma: Bioma,
    bx: i32,
    bz: i32,
    topo: i32,
    ger: &Gerador,
    agua: bool,
) -> Option<Minerio> {
    recurso_montanha_da_coluna(bioma, bx, bz, topo, ger, agua, false)
}

/// Fração do pico a partir da qual a Energia nasce. Bem mais baixa que a do
/// minério (0,42): a pedra é de MINA e mora no cume, a Energia é fenda de
/// relevo e pode estar no meio da encosta e no fundo do vale. Sem baixar
/// isto, os campos caíam quase todos em terra baixa e não rendiam veio
/// nenhum — 29 na ilha inteira, menos que antes de haver campo.
pub const ENERGIA_LIMIAR: f32 = 0.16;

/// Lado da célula que sorteia um CAMPO de Energia, em blocos.
pub const ENERGIA_CELULA: i32 = 300;
/// Uma em cada tantas células tem campo.
pub const ENERGIA_CELULA_EM: u32 = 4;
/// Raio do campo, em blocos (80 = 40 unidades).
pub const ENERGIA_CAMPO_RAIO: i32 = 80;
/// Espaçamento dos veios DENTRO do campo, em blocos. Bem maior que o do
/// minério (3) porque o veio é largo: encostados, os halos do chão viravam
/// uma poça de luz só e o campo perdia a forma.
pub const ENERGIA_ESPACO: i32 = 7;

/// Esta coluna está dentro de um CAMPO de Energia?
///
/// A Energia deixou de ser um cristal solto a cada tantas centenas de metros
/// e passou a nascer em **campos**, como a pedra: dentro do campo ela é densa,
/// fora não existe. Era o pedido do dono em 20/09/2026 — "tem que ter local só
/// de coleta de Energia igual tem de pedra". Espalhada, ela não dava lugar
/// nenhum: o jogador andava a ilha inteira pra achar um veio e ele acabava em
/// dois minutos.
///
/// O campo é um disco em volta de um centro sorteado dentro da célula, e não
/// a célula inteira — célula é quadrado, e quadrado se vê no mapa. Confere as
/// nove células em volta porque o centro da vizinha pode alcançar aqui.
pub fn no_campo_de_energia(bx: i32, bz: i32) -> bool {
    let (cx, cz) = (bx.div_euclid(ENERGIA_CELULA), bz.div_euclid(ENERGIA_CELULA));
    for dz in -1..=1 {
        for dx in -1..=1 {
            let (ax, az) = (cx + dx, cz + dz);
            let h = mistura_minerio(
                ax.wrapping_mul(7_919).wrapping_add(101),
                az.wrapping_mul(6_271).wrapping_sub(57),
            );
            if h % ENERGIA_CELULA_EM != 0 {
                continue;
            }
            let jx = ((h >> 8) & 0xff) as i32 * ENERGIA_CELULA / 256;
            let jz = ((h >> 16) & 0xff) as i32 * ENERGIA_CELULA / 256;
            let (px, pz) = (ax * ENERGIA_CELULA + jx, az * ENERGIA_CELULA + jz);
            let (ddx, ddz) = (bx - px, bz - pz);
            if ddx * ddx + ddz * ddz <= ENERGIA_CAMPO_RAIO * ENERGIA_CAMPO_RAIO {
                return true;
            }
        }
    }
    false
}

/// Cristal de Energia: grade própria em campos exclusivos, sem árvores ou minério.
pub fn energia_da_coluna(
    bioma: Bioma,
    bx: i32,
    bz: i32,
    topo: i32,
    ger: &Gerador,
    agua: bool,
) -> Option<Minerio> {
    recurso_montanha_da_coluna(bioma, bx, bz, topo, ger, agua, true)
}

fn recurso_montanha_da_coluna(
    bioma: Bioma,
    bx: i32,
    bz: i32,
    topo: i32,
    ger: &Gerador,
    agua: bool,
    energia: bool,
) -> Option<Minerio> {
    if ger.e_magica() && crate::magica::sem_pedras(glam::Vec2::new(bx as f32 * BLOCO, bz as f32 * BLOCO)) {
        return None;
    }
    // NEM PEDRA NEM ENERGIA NA ARENA. O dono: "com mais flores etc que pedra e
    // árvore". Minério é o estorvo mais gordo que existe e não tem o que fazer
    // num saguão onde ninguém coleta.
    if ger.e_arena() {
        return None;
    }
    // Nothing on a bridge or a cloud path (a node there closes the way), and
    // on Skyreach only the resource islets carry nodes: by height alone every
    // island top passed the threshold and the hunting islands were strewn
    // with rocks.
    if ger.na_ponte_magica(bx, bz)
        || (ger.e_celeste() && !crate::celeste::tem_recurso(glam::Vec2::new(bx as f32 * BLOCO, bz as f32 * BLOCO)))
    {
        return None;
    }
    if agua || ger.na_cidade(bx, bz) {
        return None;
    }
    let y = (topo + 1) as f32 * BLOCO;
    let pico = ger.pico();
    let limiar = if energia {
        ENERGIA_LIMIAR
    } else {
        MINERIO_LIMIAR
    };
    let campo_planalto = ger.planalto().is_some_and(|p| [2,3].iter().any(|i| p.centro_campo(*i).distance(glam::Vec2::new(bx as f32, bz as f32) * BLOCO) <= 32.0));
    if y < pico * limiar && !campo_planalto {
        return None;
    }

    // ── espacamento ─────────────────────────────────────────────────────
    // So' e' candidata a coluna cujo sorteio e' o MENOR do quadrado em
    // volta. Sorteio independente por coluna deixava duas pedras nascerem
    // encostadas, uma atravessando a outra.
    // O sal desloca a grade de Energia sem mudar um unico no' de minerio.
    let sal = if energia { (1_003, -1_009) } else { (0, 0) };
    let sorteio = |x: i32, z: i32| mistura_minerio(x + sal.0, z + sal.1);
    let meu = sorteio(bx, bz);
    let g1 = meu.wrapping_mul(2_246_822_519).wrapping_add(374_761_393);
    // Energia so' dentro do campo — e, la' dentro, TODA candidata vale. O
    // filtro de 1 em 5 que havia aqui espalhava um cristal solto por vez, que
    // e' o contrario de ter um lugar de coletar Energia.
    if energia && !ger.campo_de_energia(bx, bz) {
        return None;
    }
    let espaco = if energia {
        ENERGIA_ESPACO
    } else {
        MINERIO_ESPACO
    };
    for dz in -espaco..=espaco {
        for dx in -espaco..=espaco {
            if (dx, dz) != (0, 0) && sorteio(bx + dx, bz + dz) <= meu {
                return None;
            }
        }
    }
    // Dentro do campo, só Energia. As ilhotas de coleta também reservam
    // toda a área ao recurso anunciado, inclusive a orla.
    if !energia && (ger.campo_de_energia(bx, bz)
        || ger.recurso_da_ilhota(bx, bz).is_some_and(|t| t == 0 || t == 5)) {
        return None;
    }
    if ger.planalto().is_some() && !campo_planalto && meu % 4 != 0 { return None; }
    let g0 = meu;

    // ── chao limpo ──────────────────────────────────────────────────────
    // O quadrado inteiro em volta na MESMA altura. E' o que tira a pedra da
    // beira do degrau e da quina: la' metade dela ficava no ar ou enfiada
    // no barranco.
    for dz in -MINERIO_PLANO..=MINERIO_PLANO {
        for dx in -MINERIO_PLANO..=MINERIO_PLANO {
            if ger.bloco_em(bx + dx, bz + dz) != topo {
                return None;
            }
        }
    }

    // ── cume ────────────────────────────────────────────────────────────
    // Nada mais que um bloco acima dela num raio de 8 unidades. Com o
    // terraco de 4 blocos do relevo, isso quer dizer: ela esta' no patamar
    // mais alto das redondezas — o TOPO, e nao uma prateleira no meio da
    // encosta que por acaso passou do limiar de altura.
    //
    // A ENERGIA nao passa por aqui: exigir cume dela seria pedir um campo
    // inteiro de picos, e campo de Energia e' clareira, nao cordilheira.
    if !energia && !campo_planalto && !ger.cume_de_minerio(bx, bz, topo) {
        return None;
    }

    let (pmin, pmax) = MINERIO_PORTE;
    // Desvio pequeno: o chao limpo foi medido em volta do CENTRO da coluna.
    let centro = glam::Vec2::new(
        bx as f32 * BLOCO + ((g0 >> 4 & 0xff) as f32 / 255.0 - 0.5) * MINERIO_DESVIO * 2.0,
        bz as f32 * BLOCO + ((g1 >> 4 & 0xff) as f32 / 255.0 - 0.5) * MINERIO_DESVIO * 2.0,
    );
    let porte = pmin + ((g0 >> 14) & 0xff) as f32 / 255.0 * (pmax - pmin);
    if energia {
        // Na borda do campo ainda pode haver uma pedra do lado de fora.
        // Confere a distância real para os corpos não se sobreporem.
        for dz in -2..=2 {
            for dx in -2..=2 {
                let nx = bx + dx;
                let nz = bz + dz;
                let ntopo = ger.bloco_em(nx, nz);
                if let Some(pedra) = minerio_da_coluna(bioma, nx, nz, ntopo, ger, false) {
                    let separacao = RAIO_DE_MINERIO * (porte + pedra.porte) + 0.15;
                    if centro.distance_squared(pedra.centro) < separacao * separacao {
                        return None;
                    }
                }
            }
        }
    }
    Some(Minerio {
        centro,
        // NA ILHA MÁGICA o tier vem da ILHOTA, não da altura.
        //
        // `tier_de_minerio` mede a fração do pico, e faz sentido no mundo: a
        // pedra boa é de mina, e mina fica no alto. Só que a Ilha Mágica é um
        // arquipélago BAIXO por desenho — nenhuma ilhota passa de 20 u — e
        // todas as colunas caíam na primeira faixa. O dono: "só tem recurso
        // cinza lá".
        //
        // Lá o tier sai de `magica::tier_da_pedra`, que é o que também faz
        // "recursos melhores" valer: a ilhota da pedra dá a boa.
        tier: crate::magica::tier_da_pedra(ger, bx, bz)
            .unwrap_or_else(|| tier_de_minerio(y, pico)),
        porte,
        variante: (g1 >> 22) & 0x3f,
        energia,
    })
}

/// Hash de coluna so' do espacamento. Separado do sorteio do plantio: se
/// fosse o mesmo, "ganhar do vizinho" e "passar na densidade" seriam a mesma
/// moeda jogada duas vezes, e o veio sairia com buracos em grade.
fn mistura_minerio(x: i32, z: i32) -> u32 {
    let mut h = (x as u32).wrapping_mul(0x8da6_b343) ^ (z as u32).wrapping_mul(0xd816_3841);
    h ^= h >> 15;
    h = h.wrapping_mul(0x2c1b_3c6d);
    h ^= h >> 12;
    h = h.wrapping_mul(0x297a_2d39);
    h ^ (h >> 15)
}

/// Distancia minima entre duas pedras, em blocos (Chebyshev). Com 3, duas
/// pedras ficam a pelo menos 4 blocos — 2 unidades — uma da outra.
pub const MINERIO_ESPACO: i32 = 3;

/// Meio-lado do chao limpo exigido, em blocos. 2 = um quadrado de 5x5 blocos
/// (2,5 unidades) todo na mesma altura. A pedra cabe dentro com folga; ver o
/// teste `a_pedra_cabe_no_chao_limpo` no cliente.
pub const MINERIO_PLANO: i32 = 2;

/// Raio, em blocos, em que nada pode ser mais alto que a pedra (+1 bloco).
/// 16 blocos = 8 unidades.
pub const MINERIO_CUME: i32 = 16;

/// Faixa de tamanho da pedra.
pub const MINERIO_PORTE: (f32, f32) = (0.8, 1.05);

/// Quanto o centro da pedra pode sair do centro da coluna, em unidades.
pub const MINERIO_DESVIO: f32 = 0.15;

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
    /// Cristal coletável de Energia. O saldo vai direto ao personagem.
    Energia,
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
    // A ARENA NÃO TEM MAIS EXCEÇÃO AQUI, e a que havia era um defeito.
    //
    // Eu tinha zerado os estorvos dela pra livrar o A*. Só que o CLIENTE
    // desenha a vegetação direto de `arvore_da_coluna` e `minerio_da_coluna`,
    // sem passar por aqui: o resultado era uma ilhota cheia de árvores e
    // pedras que se atravessava andando — o pior dos dois mundos, e foi o que
    // o dono viu ("populada de recurso").
    //
    // O conserto certo é na FONTE, e está lá: árvore a um décimo da densidade,
    // minério e energia zerados, forração ao dobro. O que sobra é pouco e
    // barra de verdade — o que se vê é o que se esbarra.
    // A ILHOTA DO COLOSSO É LIMPA. Ver `magica::sem_recursos`.
    //
    // O `e_magica` NÃO é redundante: `ilhota_em` responde por coordenada, e
    // esta função roda em TODA ilha — sem ele, as ilhas normais perderiam os
    // recursos das colunas que calhassem de cair onde ficam as ilhotas.
    if ger.e_magica()
        && crate::magica::sem_recursos(glam::Vec2::new(bx as f32 * BLOCO, bz as f32 * BLOCO))
    {
        return;
    }
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
    if let Some(m) = energia_da_coluna(bioma, bx, bz, topo, ger, agua) {
        saida.retain(|e| e.tipo != TipoDeEstorvo::Forracao);
        saida.push(Estorvo {
            centro: m.centro,
            raio: RAIO_DE_MINERIO * m.porte,
            tipo: TipoDeEstorvo::Energia,
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
    pub semente: i32,
    cidade: Option<Cidade>,
    porto: Option<SitioPorto>,
    /// Pra onde o cais deve olhar (`DefIlha::rumo_do_porto`). `None` = so'
    /// costa plana e longe da cidade, como era antes do mar existir.
    rumo_do_mar: Option<glam::Vec2>,
    /// Casas, props e NPCs. Calculada na primeira vez que alguem pede — o
    /// cliente e o servidor pedem, o `terreno --varre` nao.
    vila: std::sync::OnceLock<crate::vila::Vila>,
    planalto: std::sync::OnceLock<Option<crate::planalto::Plano>>,
    /// Zona de relevo DESENHADO, se for uma. `None` = o Perlin de sempre.
    desenhado: Option<RelevoDesenhado>,
    /// The Ermo's oasis (`oasis`): a pond and a grove, dug into the dunes.
    oasis: Option<crate::oasis::Oasis>,
}

/// As zonas cujo relevo e' desenhado a mao, e nao sorteado.
///
/// As duas existem pelo mesmo motivo: sao lugares PEQUENOS com uma forma que
/// o jogo precisa garantir. Ruido da' paisagem, nao da' garantia — a Ilha
/// Magica precisa que a ponte ligue, e a colonia precisa que a cidade caiba
/// no meio de uma ilhota que cabe na tela.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RelevoDesenhado {
    /// Ilhotas e pontes (`magica::bloco_da_coluna`). Sem cidade e sem porto.
    Magica,
    /// Skyreach's plateaus and bridges over the cloud sea
    /// (`celeste::bloco_da_coluna`). Town on the arrival plateau, no port.
    Celeste,
    /// A ilhota de praia da colonia (`colonia::bloco_da_coluna`). A praca
    /// entra POR CIMA, pelo `aplainar` de sempre.
    Colonia,
    /// O plato liso da Arena (`arena::bloco_da_coluna`). Sem cidade, sem
    /// porto e sem ruido nenhum: la' o que se precisa e' de CHAO PLANO, e
    /// relevo sorteado nao faz planicie por encomenda — com ele a ilhota
    /// comportava dois sitios de instancia em vez dos quatro do rodizio.
    Arena,
}

/// Move um CASCO: a regra da terra, ao contrario.
///
/// Em terra `subida()` devolve `None` sobre agua e o passo e' recusado — e'
/// essa comparacao que faz o mar ser parede. Um barco precisa exatamente do
/// avesso, e por isso isto e' uma funcao IRMA de `mover_com_degrau`, nao um
/// parametro dela: aquela e' recursiva, roda duas vezes por tick pra cada
/// mob com rota, e e' a funcao mais quente do `shared`. Um `if` falso
/// 99,99% das vezes naquele laco nao compra nada e abre caminho pro caminho
/// de terra quebrar.
///
/// `agua` responde "ha' agua neste ponto?" — `Ilha::agua` numa ilha,
/// `Mar::agua` no mar aberto. Nao ha' degrau, nao ha' estorvo (nada e'
/// plantado no mar) e nao ha' deslize por eixo: um casco que bate na costa
/// PARA, e quem chama trata isso como encalhe.
pub fn mover_casco(
    agua: &dyn Fn(f32, f32) -> bool,
    pos: glam::Vec2,
    vel: glam::Vec2,
    dt: f32,
    raio: f32,
) -> glam::Vec2 {
    // Varredura, pelo mesmo motivo do `mover_com_degrau`: a 11 u/s o passo de
    // um tick e' 0,36 u contra blocos de 0,5. Sem dividir, o casco atravessa
    // um recife de uma coluna so' — e recife que nao existe nao ensina nada.
    const PASSO_MAX: f32 = 0.15;
    let anda = (vel * dt).length();
    if anda > PASSO_MAX {
        let n = (anda / PASSO_MAX).ceil().min(16.0);
        let mut p = pos;
        for _ in 0..n as u32 {
            let novo = mover_casco(agua, p, vel, dt / n, raio);
            if novo.distance_squared(p) <= 1e-9 {
                return p;
            }
            p = novo;
        }
        return p;
    }
    let v = vel * dt;
    if v == glam::Vec2::ZERO {
        return pos;
    }
    let alvo = pos + v;
    let dir = v.normalize_or_zero();
    if borda_molhada(agua, alvo, raio, dir) {
        alvo
    } else {
        pos
    }
}

/// A borda DA FRENTE do casco esta' toda na agua?
///
/// Tres pontos, a mesma geometria do `borda_livre` (o centro e os dois
/// ombros): com so' o centro, a proa passa raspando e metade do casco fica
/// dentro do barranco. E aqui os tres precisam passar, nao um — meio casco
/// em terra e' encalhe, nao navegacao.
fn borda_molhada(
    agua: &dyn Fn(f32, f32) -> bool,
    para: glam::Vec2,
    raio: f32,
    dir: glam::Vec2,
) -> bool {
    let perp = glam::Vec2::new(-dir.y, dir.x);
    for k in [-0.7f32, 0.0, 0.7] {
        let p = para + dir * raio + perp * (raio * k);
        if !agua(p.x, p.y) {
            return false;
        }
    }
    true
}

/// Terraplanagem de um sitio: plato cheio ate' `raio_plato`, smoothstep ate'
/// `raio`. Nao aterra o mar e nao arrasa morro.
fn aplainar_sitio(
    cx: f32,
    cz: f32,
    nivel: i32,
    raio_plato: f32,
    raio: f32,
    bx: i32,
    bz: i32,
    cru: i32,
) -> i32 {
    let d = ((bx as f32 - cx).powi(2) + (bz as f32 - cz).powi(2)).sqrt() * BLOCO;
    if d >= raio {
        return cru;
    }
    if (cru + 1) as f32 * BLOCO <= Cidade::SECO {
        return cru;
    }
    if ((cru - nivel) as f32 * BLOCO).abs() > Cidade::MORRO {
        return cru;
    }
    let t = suave(((d - raio_plato) / (raio - raio_plato)).clamp(0.0, 1.0));
    (nivel as f32 + (cru - nivel) as f32 * t).round() as i32
}

/// O PORTO da ilha: um patio aplainado na costa e o PIER.
///
/// O movimento barra agua, entao o cais nao pode ser so' tabuado voxel sobre
/// o mar: o gerador ERGUE as colunas da faixa do pier ate' o nivel do patio —
/// um molhe de pedra com o tabuado da doca por cima. Da' pra andar do patio
/// ate' a ponta, e cliente e servidor enxergam o mesmo molhe.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SitioPorto {
    /// Centro do patio, em unidades de mundo.
    pub centro: glam::Vec2,
    /// Bloco de topo do patio e do pier.
    pub nivel: i32,
    /// Quarto de volta cuja frente aponta pro MAR (`construcao::frente_de`).
    pub mar_q: u8,
    /// Onde o pier toca a costa.
    pub raiz: glam::Vec2,
    /// Da raiz ate' o centro do patio, na direcao da terra.
    pub recuo: f32,
    /// Comprimento e largura do cais, em unidades.
    pub comp: f32,
    pub larg: f32,
    pub doca_seed: i32,
}

impl SitioPorto {
    pub const RAIO_PLATO: f32 = 18.0;
    pub const RAIO: f32 = 28.0;

    pub fn altura(&self) -> f32 {
        (self.nivel + 1) as f32 * BLOCO
    }

    pub fn mar(&self) -> glam::Vec2 {
        crate::construcao::frente_de(self.mar_q)
    }

    /// Na faixa do pier (do centro do patio ate' a ponta), com `folga`.
    fn no_pier(&self, p: glam::Vec2, folga: f32) -> bool {
        let mar = self.mar();
        let rel = p - self.raiz;
        let ao_longo = rel.dot(mar);
        let de_lado = rel.dot(glam::Vec2::new(-mar.y, mar.x)).abs();
        ao_longo >= -self.recuo - folga
            && ao_longo <= self.comp + folga
            && de_lado <= self.larg * 0.5 + folga
    }

    /// O ponto do DECK do pier mais perto de `p`, se `p` estiver a menos de
    /// `alcance` dele.
    ///
    /// Existe por causa da grade do A*. A célula do caminho tem 4 unidades
    /// (`BLOCO × PASSO_CAMINHO`) e a prancha tem **2,5 de largura**: ela não
    /// cabe em uma célula. Os centros de célula ao longo do cais caem na água
    /// dos dois lados, então o A* nunca via a prancha como chão — medido, a
    /// rota até o Capitão do Porto parava a **117 unidades** dele, e até o
    /// Capitão do Planalto, a **533**.
    ///
    /// O dono relatou duas vezes: "não consigo andar na prancha do porto com
    /// o A*".
    ///
    /// A saída não é afinar a grade (custa em toda a ilha): é escolher um
    /// ponto MELHOR para a célula. O grafo já não usa o centro cru — usa o
    /// ponto livre mais perto. Aqui ele passa a usar o ponto sobre a prancha,
    /// e as células do cais voltam a se enxergar em fila.
    pub fn no_deck(&self, p: glam::Vec2, alcance: f32) -> Option<glam::Vec2> {
        let mar = self.mar();
        let rel = p - self.raiz;
        // Projeta no eixo da prancha e prende no comprimento dela.
        let ao_longo = rel.dot(mar).clamp(-self.recuo, self.comp);
        let sobre = self.raiz + mar * ao_longo;
        (sobre.distance(p) <= alcance).then_some(sobre)
    }

    /// Dentro do porto (patio ou pier), com `folga`.
    pub fn contem(&self, p: glam::Vec2, folga: f32) -> bool {
        p.distance(self.centro) < Self::RAIO + folga || self.no_pier(p, folga)
    }

    fn aplainar(&self, bx: i32, bz: i32, cru: i32) -> i32 {
        if self.no_pier(
            glam::Vec2::new(bx as f32 * BLOCO, bz as f32 * BLOCO),
            BLOCO * 0.5,
        ) {
            return self.nivel;
        }
        aplainar_sitio(
            self.centro.x / BLOCO,
            self.centro.y / BLOCO,
            self.nivel,
            Self::RAIO_PLATO,
            Self::RAIO,
            bx,
            bz,
            cru,
        )
    }
}

/// A cidade da ilha: um PLATO aplainado pelo proprio gerador.
///
/// E' a segunda camada do sistema do zone14. A primeira ja' existia — a
/// enseada, que puxa o noise pra um patamar raso e deixa o lugar da vila mais
/// manso. Mas manso nao e' plano: o terraco e o respiro do ruido continuam la',
/// e cidade quer chao de praca. Aqui o relevo cru e' dobrado pra um nivel so'
/// ate' `RAIO_PLATO` e volta a ele por smoothstep ate' `RAIO`.
///
/// Mora no GERADOR, e nao numa edicao de blocos do servidor, pelo mesmo motivo
/// de todo o resto do terreno: o cliente gera o chao coluna a coluna da mesma
/// funcao. Aplainar so' no servidor faria o jogador andar num chao que ele nao
/// ve'.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Cidade {
    /// Centro, em BLOCOS a partir do centro da ilha.
    pub cx: f32,
    pub cz: f32,
    /// Bloco de topo do plato.
    pub nivel: i32,
    /// Plato cheio, em unidades de mundo. CAMPO e nao constante porque a
    /// COLONIA cresce: subir o assentamento aplaina mais chao em volta da
    /// mesma praca, sem mexer numa coluna do que ja' estava plano. Fora dela
    /// e' sempre `RAIO_PLATO`.
    pub raio_plato: f32,
    /// Fim da rampa: dali pra fora o relevo e' o cru.
    pub raio: f32,
}

impl Cidade {
    /// Plato cheio padrao, em unidades de mundo. Grande o bastante pro anel de
    /// oficios (lojas de ate' 12 x 9 u) caber inteiro no plano.
    pub const RAIO_PLATO: f32 = 28.0;
    /// Fim da rampa padrao: dali pra fora o relevo e' o cru.
    pub const RAIO: f32 = 42.0;
    /// Quanto a rampa passa do plato. Mantem a inclinacao quando o plato
    /// cresce: rampa de largura fixa num plato maior fica em pe'.
    pub const RAMPA: f32 = Self::RAIO - Self::RAIO_PLATO;

    /// A praca de sempre, com os raios padrao.
    pub fn nova(cx: f32, cz: f32, nivel: i32) -> Self {
        Self {
            cx,
            cz,
            nivel,
            raio_plato: Self::RAIO_PLATO,
            raio: Self::RAIO,
        }
    }

    /// A mesma praca com o plato de outro tamanho. A RAMPA acompanha, senao
    /// um plato maior terminaria num paredao.
    pub fn com_plato(self, raio_plato: f32) -> Self {
        Self {
            raio_plato,
            raio: raio_plato + Self::RAMPA,
            ..self
        }
    }
    /// Alem do raio, onde mato, arvore e pedra ainda nao nascem.
    pub const FOLGA_DO_MATO: f32 = 3.0;
    /// Acima disto (topo do bloco, em unidades) e' terra seca.
    pub(crate) const SECO: f32 = 0.35;
    /// Desnivel alem do qual o morro fica morro: aterrar uma serra inteira
    /// pra caber a praca e' pior que a praca ter um barranco.
    pub(crate) const MORRO: f32 = 4.5;

    /// Centro em unidades de mundo.
    pub fn centro(&self) -> glam::Vec2 {
        glam::Vec2::new(self.cx * BLOCO, self.cz * BLOCO)
    }

    /// Altura do chao da praca, em unidades de mundo.
    pub fn altura(&self) -> f32 {
        (self.nivel + 1) as f32 * BLOCO
    }

    /// Distancia de `p` (unidades de mundo) ao centro, em unidades.
    pub fn distancia(&self, p: glam::Vec2) -> f32 {
        p.distance(self.centro())
    }

    fn dist_blocos(&self, bx: f32, bz: f32) -> f32 {
        ((bx - self.cx).powi(2) + (bz - self.cz).powi(2)).sqrt()
    }

    /// O bloco de topo da coluna depois da terraplanagem.
    ///
    /// Trabalha em INDICE de bloco dos dois lados. O zone14 misturava altura
    /// em metros (topo + 1) com indice, e a borda da rampa saia meio bloco
    /// acima do terreno de fora.
    fn aplainar(&self, bx: i32, bz: i32, cru: i32) -> i32 {
        aplainar_sitio(
            self.cx,
            self.cz,
            self.nivel,
            self.raio_plato,
            self.raio,
            bx,
            bz,
            cru,
        )
    }
}

impl Gerador {
    /// O gerador da COLONIA: a ilhota desenhada, com a praca NO CENTRO.
    ///
    /// Publico e num lugar so' porque o cliente gera o mesmo chao que o
    /// servidor (docs/COLONIA.md): duas chamadas soltas a `novo` + um
    /// `com_plato` esquecido de um dos lados dariam dois relevos, e o jogador
    /// veria uma ilha e andaria noutra.
    ///
    /// A praca fica na ORIGEM, e isso e' desenho e nao acaso. Antes ela era
    /// VARRIDA: `varrer_praca_da_colonia` percorria a ilha inteira
    /// procurando o disco mais plano e mais seco, 37,9 ms por chamada — e
    /// `da_colonia` e' construido quatro vezes ao abrir o painel, duas delas
    /// na thread que desenha. Numa ilhota desenhada a pergunta nao existe: o
    /// meio E' o lugar mais plano, e o dono pediu exatamente isso ("o centro
    /// sendo a cidade"). A varredura e o cache dela sairam junto.
    /// Este relevo e' a MAQUETE da colonia — um diorama, nao um lugar onde
    /// se anda.
    ///
    /// Vegetacao de mundo e' calibrada pra quem caminha DENTRO dela: muita
    /// planta pequena, muita arvore media. Vista de fora e inteira, a mesma
    /// densidade vira ruido — o dono: "a ilha poder ser mais simples ainda
    /// que isso". Numa maquete valem poucas pecas e grandes.
    pub fn e_maquete(&self) -> bool {
        matches!(self.desenhado, Some(RelevoDesenhado::Colonia))
    }

    /// Este relevo é a ILHA MÁGICA?
    ///
    /// Ela é uma zona de EVENTO, curta e com bônus — não um lugar de morar.
    /// A densidade de mundo (calibrada pra quem passa horas farmando) enche
    /// as ilhotas de tronco e pedra, e o dono: "tá lotado de recursos, pedra,
    /// árvores etc, e fica muito feio; tem que ser menos recursos e recursos
    /// melhores".
    ///
    /// O "melhores" já existe: cada ilhota paga ×2 no que ela promete
    /// (`Bonus::multiplicador`), então a pedra de 30 dá 60 lá. O que faltava
    /// era o "menos".
    pub fn e_magica(&self) -> bool {
        matches!(self.desenhado, Some(RelevoDesenhado::Magica))
    }

    /// Este gerador é o da ILHOTA DA DUNGEON?
    ///
    /// Pelo par semente+raio, e não por um campo novo: a arena usa o relevo
    /// de Perlin comum (não é `RelevoDesenhado`, que significa "desenhado à
    /// mão"), e acrescentar um `bool` obrigaria a tocar todos os construtores
    /// do gerador pra marcar `false`. O par é fixo, é só desta zona, e está
    /// declarado num lugar só (`arena::SEMENTE`).
    pub fn e_arena(&self) -> bool {
        self.semente == crate::arena::SEMENTE && self.raio_blocos == crate::arena::RAIO_BLOCOS
    }

    /// Esta coluna cai numa PONTE da Ilha Mágica?
    ///
    /// Nada nasce ali: a ponte é estreita de propósito (é o gargalo do PvP) e
    /// qualquer estorvo no meio dela a fecha — o A* deixa de achar passagem e
    /// só dá pra atravessar andando na mão.
    pub fn na_ponte_magica(&self, bx: i32, bz: i32) -> bool {
        let p = glam::Vec2::new(bx as f32 * BLOCO, bz as f32 * BLOCO);
        match self.desenhado {
            Some(RelevoDesenhado::Magica) => crate::magica::na_ponte(p),
            Some(RelevoDesenhado::Celeste) => crate::celeste::na_ponte(p),
            _ => false,
        }
    }

    /// A material fixed by a DRAWN map at this column, over whatever height
    /// and biome would give: Skyreach's cloud paths are cloud.
    pub fn material_desenhado(&self, bx: i32, bz: i32) -> Option<Material> {
        (self.e_celeste() && crate::celeste::na_ponte(glam::Vec2::new(bx as f32 * BLOCO, bz as f32 * BLOCO)))
            .then_some(Material::Nuvem)
    }

    pub fn e_celeste(&self) -> bool {
        matches!(self.desenhado, Some(RelevoDesenhado::Celeste))
    }

    /// Islands in the sky (Magic Island, Skyreach): no sea, the gaps are a
    /// drop to the cloud floor.
    pub fn e_aerea(&self) -> bool {
        matches!(self.desenhado, Some(RelevoDesenhado::Magica | RelevoDesenhado::Celeste))
    }

    /// SKYREACH: the drawn plateaus and bridges, the town on the arrival
    /// plateau, no port (`celeste`). One constructor for client and server,
    /// like `da_colonia`: two loose `novo` calls would be two reliefs.
    pub fn da_ilha_celeste() -> Self {
        let mut g = Self::novo(
            crate::celeste::SEMENTE,
            crate::celeste::RAIO_BLOCOS,
            crate::celeste::DEF.bioma,
            ESCALA_ALTURA,
        );
        g.desenhado = Some(RelevoDesenhado::Celeste);
        g.cidade = None;
        g.porto = None;
        g.oasis = None;
        let c = crate::celeste::PLATOS[0].centro;
        let (bx, bz) = ((c.x / BLOCO) as i32, (c.y / BLOCO) as i32);
        let nivel = g.bloco_em(bx, bz);
        g.cidade = Some(Cidade::nova(c.x, c.y, nivel));
        g
    }

    pub fn da_colonia(plato: f32) -> Self {
        let mut g = Self::novo(
            crate::colonia::SEMENTE,
            crate::colonia::RAIO_BLOCOS,
            Bioma::Floresta,
            ESCALA_ALTURA,
        );
        g.desenhado = Some(RelevoDesenhado::Colonia);
        // O que `novo` achou foi no relevo CRU, que a colonia nao usa: uma
        // praca no lugar errado e um cais para lugar nenhum.
        g.cidade = None;
        g.porto = None;
        // Le' o chao JA' desenhado e sem praca nenhuma — com a cidade posta
        // antes, `bloco_em` devolveria o nivel dela mesma.
        let nivel = g.bloco_em(0, 0);
        g.cidade = Some(Cidade::nova(0.0, 0.0, nivel).com_plato(plato));
        g
    }

    /// O gerador da ILHA MAGICA: ilhotas e pontes, sem cidade e sem porto.
    ///
    /// Publico e num lugar so' pelo mesmo motivo de `da_colonia`: o cliente
    /// desenha o chao a partir do gerador, nunca de um campo de altura que o
    /// servidor mande. Duas chamadas soltas a `novo` dariam dois relevos.
    pub fn da_ilha_magica() -> Self {
        let mut g = Self::novo(
            crate::magica::SEMENTE,
            crate::magica::RAIO_BLOCOS,
            Bioma::Floresta,
            ESCALA_ALTURA,
        );
        g.desenhado = Some(RelevoDesenhado::Magica);
        // `novo` ja' procurou cidade e porto no relevo CRU, que a ilha magica
        // nao usa. Deixa-los ali daria uma praca aplainada sobre o mar e um
        // cais para lugar nenhum — visiveis na vila, que le' a cidade.
        g.cidade = None;
        g.porto = None;
        g
    }

    /// A ILHOTA DA DUNGEON: relevo comum, sem cidade, porto nem recurso.
    ///
    /// Ela é a ilha de Perlin de sempre — nada desenhado, ao contrário da
    /// mágica —, porque o que a arena precisa é de CHÃO PLANO, e o relevo
    /// normal já produz sítios planos de sobra. O que sai é o que atrapalha:
    ///
    /// * cidade e porto seriam uma praça e um cais onde ninguém passa;
    /// * o recurso vira estorvo, e estorvo na arena é o A* contornando o que
    ///   deveria ser espaço de luta — a mesma razão da ilhota do Colosso.
    pub fn da_arena() -> Self {
        let mut g = Self::novo(
            crate::arena::SEMENTE,
            crate::arena::RAIO_BLOCOS,
            Bioma::Floresta,
            ESCALA_ALTURA,
        );
        g.desenhado = Some(RelevoDesenhado::Arena);
        g.cidade = None;
        g.porto = None;
        g
    }

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
        let mut g = Self {
            p: Perlin::novo(semente),
            pw: Perlin::novo(semente ^ 0x5f37_59df),
            pctrl: Perlin::novo(semente ^ 0x1b87_3593),
            forma: Forma::de(semente, raio_blocos),
            perfil: bioma.perfil(),
            raio_blocos,
            escala_altura,
            terraco_blocos,
            terraco_forca,
            semente,
            cidade: None,
            porto: None,
            rumo_do_mar: None,
            vila: std::sync::OnceLock::new(),
            planalto: std::sync::OnceLock::new(),
            desenhado: None,
            oasis: None,
        };
        g.cidade = g.achar_cidade();
        g.porto = g.achar_porto(g.cidade);
        g
    }

    /// O gerador de uma ilha do arquipelago. E' por aqui que o cais aprende
    /// pra que lado fica a rota (`DefIlha::rumo_do_porto`) — e e' por isso
    /// que quem constroi a partir de um `DefIlha` deve usar isto, e nao
    /// `novo`: com os quatro argumentos soltos o rumo se perde e o porto sai
    /// no lugar errado, sem erro nenhum.
    pub fn da_ilha(def: &DefIlha) -> Self {
        // A Ilha Magica e' desenhada, nao sorteada: ela desvia aqui pra que
        // quem passa pela def (cliente, minimapa, construcoes, servidor)
        // receba o arquipelago de ilhotas e nao uma ilha de Perlin.
        if crate::magica::e_magica(def.zona) {
            return Self::da_ilha_magica();
        }
        if crate::arena::e_arena(def.zona) {
            return Self::da_arena();
        }
        if crate::celeste::e_celeste(def.zona) {
            return Self::da_ilha_celeste();
        }
        let p = def.bioma.perfil();
        let (tb, tf) = (p.terraco_blocos, p.terraco_forca);
        let mut g = Self {
            p: Perlin::novo(def.semente),
            pw: Perlin::novo(def.semente ^ 0x5f37_59df),
            pctrl: Perlin::novo(def.semente ^ 0x1b87_3593),
            forma: Forma::de(def.semente, def.raio_blocos),
            perfil: def.bioma.perfil(),
            raio_blocos: def.raio_blocos,
            escala_altura: ESCALA_ALTURA,
            terraco_blocos: tb,
            terraco_forca: tf,
            semente: def.semente,
            cidade: None,
            porto: None,
            rumo_do_mar: def.rumo_do_porto(),
            vila: std::sync::OnceLock::new(),
            planalto: std::sync::OnceLock::new(),
            desenhado: None,
            oasis: None,
        };
        g.cidade = g.achar_cidade();
        g.porto = g.achar_porto(g.cidade);
        // O ULTIMO ABRIGO: a cidade do Planalto anda pra perto do porto, pra o
        // hub ser a chegada e nao uma caminhada.
        //
        // O NIVEL sai do chao de verdade, e nao de `porto.nivel + 8`. Com o
        // palpite, `Cidade::aplainar` desistia da maioria das colunas — ela
        // recusa aplainar onde o relevo cru esta' a mais de `Cidade::MORRO` do
        // nivel pedido —, e a praca saiu com 69% do plato no lugar e quatro
        // NPCs a menos que as outras ilhas. Pelo chao real ela aplaina como em
        // qualquer ilha.
        if def.zona == crate::planalto::ZONA {
            if let Some(p) = g.porto {
                // O sitio e' PROCURADO perto do porto, com o mesmo criterio das
                // outras ilhas (`avaliar_sitio`: variancia do chao, sem agua no
                // plato), e nao fixado a 180 u no rumo da terra.
                //
                // Com o ponto fixo a praca caia onde calhasse: 90% do plato no
                // nivel contra 100% nas outras tres, e quatro NPCs a menos,
                // porque `Cidade::aplainar` desiste das colunas a mais de
                // `Cidade::MORRO` do nivel. Procurar custa ~50 avaliacoes, uma
                // vez, no boot.
                let rumo = (-p.centro).normalize_or_zero();
                let mut melhor: Option<(f32, Cidade)> = None;
                for passo in 0..6 {
                    let dist = 140.0 + passo as f32 * 20.0;
                    for k in -4..=4 {
                        let a = k as f32 * 0.17;
                        let dir = glam::Vec2::new(
                            rumo.x * a.cos() - rumo.y * a.sin(),
                            rumo.x * a.sin() + rumo.y * a.cos(),
                        );
                        let c = p.centro + dir * dist;
                        let (cx, cz) = (c.x / BLOCO, c.y / BLOCO);
                        let Some((var, nivel)) = g.avaliar_sitio(cx, cz) else {
                            continue;
                        };
                        // Entre dois sitios bons, o mais perto do cais.
                        let nota = var + dist * 0.01;
                        if melhor.is_none_or(|(n, _)| nota < n) {
                            melhor = Some((nota, Cidade::nova(cx, cz, nivel)));
                        }
                    }
                }
                // Nenhum serve: fica a cidade que `achar_cidade` escolheu.
                if let Some((_, c)) = melhor {
                    g.cidade = Some(c);
                    g.planalto = std::sync::OnceLock::new();
                }
            }
        }
        if def.zona == crate::oasis::ZONA {
            // Searched ONCE per process: the search costs ~25 ms, and a
            // `Gerador` of the Ermo is built on every Porão door and map open.
            // Only one island has an oasis, so one answer covers them all.
            static OASIS: std::sync::OnceLock<Option<crate::oasis::Oasis>> =
                std::sync::OnceLock::new();
            g.oasis = *OASIS.get_or_init(|| g.achar_oasis());
        }
        g
    }

    /// The Ermo's oasis, if this island has one.
    pub fn oasis(&self) -> Option<crate::oasis::Oasis> {
        self.oasis
    }

    /// Where the oasis goes: a dry, gentle spot a walk away from town — 200 to
    /// 420 u from the city, clear of the port. Searched once, at boot, with the
    /// same ground test the city uses (`avaliar_sitio`: no water on the site,
    /// not too steep), and the flattest candidate wins. Deterministic: the
    /// server and every client land on the same spot.
    fn achar_oasis(&self) -> Option<crate::oasis::Oasis> {
        let cidade = self.cidade?.centro();
        let mut melhor: Option<(f32, glam::Vec2)> = None;
        for anel in 0..12 {
            let dist = 200.0 + anel as f32 * 20.0;
            for k in 0..32 {
                let a = k as f32 / 32.0 * std::f32::consts::TAU;
                let c = cidade + glam::Vec2::new(a.cos(), a.sin()) * dist;
                if self.porto.is_some_and(|p| p.centro.distance(c) < 150.0) {
                    continue;
                }
                let Some((var, _)) = self.avaliar_sitio(c.x / BLOCO, c.y / BLOCO) else {
                    continue;
                };
                // Not on an ore ridge or an Energy field: those are clearings
                // kept for their resource, and they'd keep the grove out (the
                // first oasis landed on one and grew a single tree).
                let (cbx, cbz) = ((c.x / BLOCO) as i32, (c.y / BLOCO) as i32);
                let r = (crate::oasis::RAIO_VERDE / BLOCO) as i32;
                let limpo = (-r..=r).step_by(4).all(|dz| {
                    (-r..=r).step_by(4).all(|dx| {
                        let (bx, bz) = (cbx + dx, cbz + dz);
                        self.zona_de_coleta(bx, bz, self.bloco_cru(bx, bz))
                            .is_none_or(|t| t == 0)
                    })
                });
                if !limpo {
                    continue;
                }
                // Flat first; between two flat ones, the closer to town.
                let nota = var + dist * 0.002;
                if melhor.is_none_or(|(n, _)| nota < n) {
                    melhor = Some((nota, c));
                }
            }
        }
        melhor.map(|(_, centro)| crate::oasis::Oasis { centro })
    }

    /// Indice do bloco de topo na coluna `(bx, bz)`, em blocos a partir do
    /// CENTRO da ilha. Ja' com a cidade e o porto aplainados.
    pub fn bloco_em(&self, bx: i32, bz: i32) -> i32 {
        // A ilha magica e' DESENHADA, nao sorteada: ela sai antes de tudo,
        // inclusive do ruido. Aplainar por cima de um relevo cru que ninguem
        // vai ver seria pagar tres oitavas de fbm por coluna a toa — e sao
        // 1,25 milhao de colunas.
        if self.desenhado == Some(RelevoDesenhado::Magica) {
            return crate::magica::bloco_da_coluna(bx, bz);
        }
        // A ARENA tambem sai antes de tudo, e pelo mesmo motivo: e' desenhada,
        // nao sorteada. Aplainar por cima do ruido seria pagar tres oitavas de
        // fbm por coluna pra jogar fora o resultado.
        if self.desenhado == Some(RelevoDesenhado::Arena) {
            return crate::arena::bloco_da_coluna(bx, bz);
        }
        let cru = match self.desenhado {
            // A colonia e' desenhada, mas a PRACA continua sendo aplainada
            // por cima pelo caminho de sempre — e' o mesmo `aplainar_sitio`
            // do mundo normal, que e' o que o dono pediu quando disse
            // "sempre ter um terreno aplanado nas construcoes igual no mundo
            // normal".
            Some(RelevoDesenhado::Colonia) => crate::colonia::bloco_da_coluna(bx, bz),
            Some(RelevoDesenhado::Celeste) => crate::celeste::bloco_da_coluna(bx, bz),
            _ => self.bloco_cru(bx, bz),
        };
        let cru = self.planalto().map_or(cru, |p| p.bloco(glam::Vec2::new(bx as f32 * BLOCO, bz as f32 * BLOCO), cru));
        let b = match &self.cidade {
            Some(c) => c.aplainar(bx, bz, cru),
            None => cru,
        };
        let b = match &self.porto {
            Some(p) => p.aplainar(bx, bz, b),
            None => b,
        };
        // The castle of Last Refuge stands on the finished ground (`planalto`).
        let b = match self.planalto() {
            Some(pl) => pl.muralha(glam::Vec2::new(bx as f32 * BLOCO, bz as f32 * BLOCO), b),
            None => b,
        };
        match &self.oasis {
            Some(o) => o.moldar(bx, bz, b),
            None => b,
        }
    }

    pub fn planalto(&self) -> Option<&crate::planalto::Plano> {
        self.planalto.get_or_init(|| {
            (self.semente == ARQUIPELAGO[3].semente && self.raio_blocos == ARQUIPELAGO[3].raio_blocos)
                .then(|| self.cidade.map(|c| crate::planalto::Plano::novo(c, self.porto))).flatten()
        }).as_ref()
    }

    /// A cidade desta ilha, se ela coube.
    pub fn cidade(&self) -> Option<Cidade> {
        self.cidade
    }

    /// O porto desta ilha, se achou costa que sirva.
    pub fn porto(&self) -> Option<SitioPorto> {
        self.porto
    }

    /// Casas, props e NPCs da cidade e do porto. Ver `vila`.
    pub fn vila(&self) -> &crate::vila::Vila {
        self.vila.get_or_init(|| crate::vila::montar(self))
    }

    /// A tinta do chao dos povoados na coluna: praca e caminho calcados,
    /// grama cuidada. `None` = o chao natural. So' cor — a altura nao muda,
    /// entao o cache de relevo nao precisa de versao nova.
    pub fn pintura_do_chao(&self, bx: i32, bz: i32) -> Option<Material> {
        // A ESTRADA e' a unica coisa pintada no Planalto, e e' pintada porque e'
        // FEITA: pedra posta por gente. O resto do chao continua do bioma.
        //
        // Havia aqui uma segunda regra que pintava tudo num raio de 94 u de cada
        // uma das cinco regioes de `Rocha`, com uma tira de `Terra` a cada setima
        // celula de 8x10 u. Nas capturas isso virou cinco patios cinzentos com um
        // xadrez marrom por cima, e quebrava a regra que docs/MUNDO.md registra:
        // **cinza e' onde nao se sobe**. Pintar de cinza o chao em que se anda
        // apaga a unica leitura de relevo que o jogo da' sem texto.
        if self.planalto().is_some_and(|p| p.distancia_estrada(glam::Vec2::new(bx as f32 * BLOCO, bz as f32 * BLOCO)) < crate::planalto::ESTRADA) { return Some(Material::RochaEscura); }
        // The castle's wall walk and tower tops: dressed stone.
        if let Some(parte) = self.planalto().and_then(|p| p.parte_da_muralha(glam::Vec2::new(bx as f32 * BLOCO, bz as f32 * BLOCO))) {
            let xadrez = (bx.div_euclid(2) + bz.div_euclid(2)) % 2 == 0;
            return Some(match parte {
                crate::planalto::Muro::Torre { .. } => if xadrez { Material::CalcadaEscura } else { Material::Rocha },
                crate::planalto::Muro::Cortina { .. } => if xadrez { Material::Calcada } else { Material::CalcadaEscura },
            });
        }
        // A Porão's halls and walls on the Arena (`planta`).
        if self.e_arena() {
            let p = glam::Vec2::new(bx as f32 * BLOCO, bz as f32 * BLOCO);
            if let Some(pl) = crate::planta::complexo_em(p) {
                if pl.bloco(p, 0).is_some() {
                    return pl.pintura(p);
                }
            }
        }
        // The oasis: wet sand at the water, then grass — the only green in
        // the Ermo, which is the point.
        if let Some(o) = &self.oasis {
            if o.na_beira(bx, bz) {
                return Some(Material::AreiaMolhada);
            }
            if o.no_verde(bx, bz) {
                let h = (bx as u32).wrapping_mul(2_654_435_761) ^ (bz as u32).wrapping_mul(40_503);
                return Some(match h % 5 {
                    0 => Material::GramaEscura,
                    1 => Material::GramaClara,
                    _ => Material::Grama,
                });
            }
        }
        if !self.na_cidade(bx, bz) {
            return None;
        }
        crate::vila::pintura_em(
            self.vila(),
            glam::Vec2::new(bx as f32 * BLOCO, bz as f32 * BLOCO),
            bx,
            bz,
        )
    }

    /// Coluna dentro de um povoado — cidade ou porto, com a folga do mato:
    /// nada nasce ali.
    pub fn na_cidade(&self, bx: i32, bz: i32) -> bool {
        let p = glam::Vec2::new(bx as f32 * BLOCO, bz as f32 * BLOCO);
        self.planalto().is_some_and(|pl| pl.sem_obstaculo(p)) || self.cidade
            .is_some_and(|c| c.distancia(p) < c.raio + Cidade::FOLGA_DO_MATO)
            || self
                .porto
                .is_some_and(|s| s.contem(p, Cidade::FOLGA_DO_MATO))
    }

    /// `p` e' agua ligada ao OCEANO — e nao lago, baixada ou poca cercada de
    /// terra? Devolve (aberto, celulas visitadas).
    ///
    /// Busca GULOSA pra fora numa grade de 2 u, andando so' por agua: a celula
    /// mais longe do centro da ilha sai primeiro, entao mar de verdade chega na
    /// borda em poucas centenas de passos; lago esgota o proprio contorno e
    /// para. Nao ha' mascara da ilha inteira guardada: seriam centenas de
    /// milhares de colunas de ruido a cada `Gerador` que o cliente cria.
    pub fn mar_aberto(&self, p: glam::Vec2) -> (bool, usize) {
        use std::collections::{BinaryHeap, HashSet};
        const PASSO: f32 = 2.0;
        const TETO: usize = 30_000;
        let raio_un = self.raio_blocos as f32 * BLOCO;
        let agua = |c: (i32, i32)| {
            let (x, z) = (c.0 as f32 * PASSO, c.1 as f32 * PASSO);
            (self.bloco_cru((x / BLOCO).round() as i32, (z / BLOCO).round() as i32) + 1) as f32
                * BLOCO
                <= Cidade::SECO
        };
        let ini = ((p.x / PASSO).round() as i32, (p.y / PASSO).round() as i32);
        if !agua(ini) {
            return (false, 0);
        }
        let d2 = |c: (i32, i32)| c.0 as i64 * c.0 as i64 + c.1 as i64 * c.1 as i64;
        let mut fila = BinaryHeap::new();
        let mut visto = HashSet::new();
        fila.push((d2(ini), ini));
        visto.insert(ini);
        while let Some((_, c)) = fila.pop() {
            if (c.0 as f32 * PASSO).hypot(c.1 as f32 * PASSO) >= raio_un {
                return (true, visto.len());
            }
            if visto.len() >= TETO {
                return (false, visto.len());
            }
            for v in [
                (c.0 + 1, c.1),
                (c.0 - 1, c.1),
                (c.0, c.1 + 1),
                (c.0, c.1 - 1),
            ] {
                if visto.insert(v) && agua(v) {
                    fila.push((d2(v), v));
                }
            }
        }
        (false, visto.len())
    }

    /// Onde o porto fica: na COSTA, longe da cidade.
    ///
    /// Adaptado do `Porto.Gerar` do zone14: 48 rumos saindo do centro da
    /// ilha ate' a costa. O cais so' gira de 90 em 90 graus (a doca e' um
    /// volume de blocos), entao o rumo vira o eixo mais proximo e a costa e'
    /// medida DE NOVO ao longo dele. Exige patio seco e raso, agua funda na
    /// ponta do cais e 150 u da cidade; entre os que servem, o mais plano,
    /// com um puxao pra longe (ate' 320 u — mais que isso vira viagem).
    fn achar_porto(&self, cidade: Option<Cidade>) -> Option<SitioPorto> {
        use crate::construcao::{frente_de, gerar, quarto_para, Papel, TipoCasa, B_CASA};
        let v2 = glam::Vec2::new;
        let raio_un = self.raio_blocos as f32 * BLOCO;
        let topo = |p: glam::Vec2| {
            (self.bloco_cru((p.x / BLOCO).round() as i32, (p.y / BLOCO).round() as i32) + 1) as f32
                * BLOCO
        };
        let seco = |p: glam::Vec2| topo(p) > Cidade::SECO;
        let doca_seed = self.semente ^ 0x0D0C_A5;
        let doca = gerar(TipoCasa::Doca, Papel::Estaleiro, doca_seed);
        let (comp, larg) = (doca.v.nz as f32 * B_CASA, doca.v.nx as f32 * B_CASA);
        let mut melhor: Option<(f32, SitioPorto)> = None;
        for k in 0..48 {
            let a = k as f32 / 48.0 * std::f32::consts::TAU;
            let rumo = v2(a.cos(), a.sin());
            // TODA passagem de terra pra agua ao longo do rumo, e nao so' a
            // primeira: saindo do centro, a primeira agua pode ser um LAGO no
            // meio da ilha — foi assim que o porto da ilha inicial foi parar
            // numa baixada. Quem decide qual serve e' o `mar_aberto` la' embaixo.
            let mut visto_seco = false;
            let mut costas = Vec::new();
            let mut d = 20.0;
            while d < raio_un {
                let p = rumo * d;
                if seco(p) {
                    visto_seco = true;
                } else if visto_seco && (1..=6).all(|j| !seco(rumo * (d + j as f32))) {
                    costas.push(p);
                    visto_seco = false;
                }
                d += 1.0;
            }
            for pc in costas {
                let mar_q = quarto_para(rumo);
                let mar = frente_de(mar_q);
                let centro = pc - mar * 14.0;
                let mut t = 0.0;
                let mut praia = None;
                while t < 40.0 {
                    if !seco(centro + mar * t) {
                        praia = Some(t);
                        break;
                    }
                    t += 0.5;
                }
                let Some(praia) = praia else { continue };
                if praia < 8.0 {
                    continue;
                }
                let recuo = praia - 1.0;
                let raiz = centro + mar * recuo;
                let fundo = |p: glam::Vec2| topo(p) <= -1.0;
                if seco(raiz + mar * (comp * 0.5))
                    || !fundo(raiz + mar * comp)
                    || !fundo(raiz + mar * (comp + 2.0))
                {
                    continue;
                }
                let (mut soma, mut soma2, mut n, mut molhado) = (0.0f32, 0.0f32, 0.0f32, false);
                for anel in 0..=4 {
                    let r = 10.0 * anel as f32 / 4.0;
                    let k_max = if anel == 0 { 1 } else { 12 };
                    for j in 0..k_max {
                        let b = j as f32 / k_max as f32 * std::f32::consts::TAU;
                        let p = centro + v2(b.cos(), b.sin()) * r;
                        if !seco(p) {
                            molhado = true;
                        }
                        let blocos = topo(p) / BLOCO - 1.0;
                        soma += blocos;
                        soma2 += blocos * blocos;
                        n += 1.0;
                    }
                }
                if molhado {
                    continue;
                }
                let media = soma / n;
                let altura = (media + 1.0) * BLOCO;
                if !(0.8..=6.0).contains(&altura) {
                    continue;
                }
                let longe = cidade.map_or(1000.0, |c| c.centro().distance(centro));
                if longe < 150.0 {
                    continue;
                }
                // Por ultimo (e' o teste caro): a agua da ponta do cais, e um
                // pouco alem pro calado, tem que ser OCEANO — ligada ao mar em
                // volta da ilha, e nao lago cercado de terra.
                let ponta = raiz + mar * comp;
                if !self.mar_aberto(ponta).0 || !self.mar_aberto(ponta + mar * 6.0).0 {
                    continue;
                }
                // Achatamento, menos um puxao pra longe da cidade, menos o
                // premio por OLHAR PRA ROTA. O rumo que conta e' o `mar`
                // ja' encaixado no quarto de volta — e' ele que decide pra
                // onde o cais aponta de verdade, nao o raio cru.
                //
                // Peso 6 contra 4 do "longe": entre uma costa plana virada
                // pro lado errado e uma costa boa virada pra rota, a rota
                // ganha. Uma travessia que da' a volta na ilha custa minutos
                // por viagem, todo dia, pra sempre.
                let olhando = self.rumo_do_mar.map_or(0.0, |alvo| mar.dot(alvo));
                let nota = (soma2 / n - media * media) - longe.min(320.0) / 80.0 - olhando * 6.0;
                if melhor.is_none_or(|(m, _)| nota < m) {
                    melhor = Some((
                        nota,
                        SitioPorto {
                            centro,
                            nivel: media.round() as i32,
                            mar_q,
                            raiz,
                            recuo,
                            comp,
                            larg,
                            doca_seed,
                        },
                    ));
                }
            }
        }
        melhor.map(|(_, s)| s)
    }

    /// Onde a cidade assenta: perto da ENSEADA, no sitio mais plano.
    ///
    /// Como no zone14, a enseada decide a regiao — ela ja' puxou o noise pra
    /// um patamar raso. Mas o centro dela nao e' garantia: fica a 0,66 do raio
    /// da ilha, perto de onde a mascara da costa comeca a derrubar o chao.
    /// Entao varre uma grade em volta e fica com o sitio de menor variancia
    /// que tenha o plato inteiro seco. Ordem fixa e desempate pelo primeiro:
    /// cliente e servidor escolhem o mesmo.
    fn achar_cidade(&self) -> Option<Cidade> {
        let (ex, ez, er) = self.forma.enseada?;
        let passo = er * 0.12;
        let mut melhor: Option<(f32, Cidade)> = None;
        for iz in -4..=4 {
            for ix in -4..=4 {
                let (cx, cz) = (ex + ix as f32 * passo, ez + iz as f32 * passo);
                let Some((var, nivel)) = self.avaliar_sitio(cx, cz) else {
                    continue;
                };
                // Um empurrao pro centro da enseada: entre dois sitios quase
                // iguais, o que ela desenhou.
                let nota = var + ((ix * ix + iz * iz) as f32).sqrt() * 0.5;
                if melhor.is_none_or(|(n, _)| nota < n) {
                    melhor = Some((nota, Cidade::nova(cx, cz, nivel)));
                }
            }
        }
        melhor.map(|(_, c)| c)
    }

    /// (variancia em blocos², nivel do plato) do sitio, ou `None` se nao serve:
    /// agua no plato, costa demais na rampa, ou alto/baixo demais.
    fn avaliar_sitio(&self, cx: f32, cz: f32) -> Option<(f32, i32)> {
        let raio_b = Cidade::RAIO / BLOCO;
        let plato_b = Cidade::RAIO_PLATO / BLOCO;
        let (mut soma, mut soma2, mut n) = (0.0f32, 0.0f32, 0.0f32);
        let (mut fora, mut molhado_fora) = (0, 0);
        for anel in 0..=6 {
            let r = raio_b * anel as f32 / 6.0;
            let k_max = if anel == 0 { 1 } else { 12 };
            for k in 0..k_max {
                let a = k as f32 / k_max as f32 * std::f32::consts::TAU;
                let b = self.bloco_cru(
                    (cx + a.cos() * r).round() as i32,
                    (cz + a.sin() * r).round() as i32,
                );
                let seco = (b + 1) as f32 * BLOCO > Cidade::SECO;
                if r <= plato_b {
                    if !seco {
                        return None;
                    }
                    soma += b as f32;
                    soma2 += (b * b) as f32;
                    n += 1.0;
                } else {
                    fora += 1;
                    if !seco {
                        molhado_fora += 1;
                    }
                }
            }
        }
        if molhado_fora * 4 > fora {
            return None;
        }
        let media = soma / n;
        let altura = (media + 1.0) * BLOCO;
        if !(1.2..=9.0).contains(&altura) {
            return None;
        }
        Some((soma2 / n - media * media, media.round() as i32))
    }

    /// O bloco de topo do relevo CRU, antes da cidade.
    fn bloco_cru(&self, bx: i32, bz: i32) -> i32 {
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
        (0.5 + self.p.fbm(
            bx as f32 * 0.0075 + 313.0,
            bz as f32 * 0.0075 - 77.0,
            3,
            0.5,
        ) * 1.1)
            .clamp(0.0, 1.0)
    }

    /// Teto teorico do relevo deste bioma, em unidades. E' a regua contra a
    /// qual "topo de montanha" quer dizer a mesma coisa em todo bioma — o
    /// pico da Floresta e o do Planalto sao numeros bem diferentes.
    /// Esta coluna esta' num campo de Energia?
    ///
    /// Normalmente e' a grade global (`no_campo_de_energia`). A Ilha Magica
    /// responde por si: a grade global tem celula de 300 blocos e foi feita
    /// pra ilha de 1,6 km, e numa de 280 u ela cai onde cai.
    pub fn campo_de_energia(&self, bx: i32, bz: i32) -> bool {
        if let Some(p) = self.planalto() {
            let q = glam::Vec2::new(bx as f32,bz as f32) * BLOCO;
            if p.centro_campo(2).distance(q) <= 32.0 { return true; }
            if p.centro_campo(3).distance(q) <= 32.0 { return false; }
        }
        if self.desenhado == Some(RelevoDesenhado::Magica) {
            return crate::magica::no_campo_de_energia(bx, bz);
        }
        no_campo_de_energia(bx, bz)
    }

    /// Recurso prometido pela ilhota de coleta; o resto mantém seu bioma.
    fn recurso_da_ilhota(&self, bx: i32, bz: i32) -> Option<u8> {
        if !self.e_magica() { return None; }
        let p = glam::Vec2::new(bx as f32, bz as f32) * BLOCO;
        match crate::magica::ilhota_em(p)?.bonus {
            crate::magica::Bonus::Coleta(t) => Some(t),
            _ => None,
        }
    }

    /// A área de mineração usa o mesmo patamar que permite nascer minério.
    fn cume_de_minerio(&self, bx: i32, bz: i32, topo: i32) -> bool {
        let c = MINERIO_CUME;
        for dz in (-c..=c).step_by(2) {
            for dx in (-c..=c).step_by(2) {
                if dx * dx + dz * dz <= c * c && self.bloco_em(bx + dx, bz + dz) > topo + 1 {
                    return false;
                }
            }
        }
        true
    }

    /// Clareiras de recurso: 5 Energia, 1..4 pedra, 0 madeira nas ilhotas.
    /// Cliente e servidor consultam isto antes de plantar obstáculos.
    pub fn zona_de_coleta(&self, bx: i32, bz: i32, topo: i32) -> Option<u8> {
        if self.e_arena() { return None; }
        if let Some(tipo) = self.recurso_da_ilhota(bx, bz) { return Some(tipo); }
        if self.campo_de_energia(bx, bz) { return Some(5); }
        ((topo + 1) as f32 * BLOCO >= self.pico() * MINERIO_LIMIAR
            && self.cume_de_minerio(bx, bz, topo)).then_some(1)
    }

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
    /// Paredes das casas e o poco, projetados no chao. Ver `vila::caixas_solidas`.
    solidos: Vec<crate::vila::Caixa2>,
    /// Indice dos solidos, na mesma grade dos estorvos.
    grade_solidos: Vec<Vec<u32>>,
}

/// Distancia com sinal de `p` a caixa: negativa dentro.
fn dist_caixa(c: &crate::vila::Caixa2, p: glam::Vec2) -> f32 {
    let dx = (c.min.x - p.x).max(p.x - c.max.x);
    let dz = (c.min.y - p.y).max(p.y - c.max.y);
    if dx <= 0.0 && dz <= 0.0 {
        dx.max(dz)
    } else {
        glam::Vec2::new(dx.max(0.0), dz.max(0.0)).length()
    }
}

/// O segmento `a -> b` corta a caixa? (teste de lajes)
fn segmento_corta(c: &crate::vila::Caixa2, a: glam::Vec2, b: glam::Vec2) -> bool {
    let d = b - a;
    let (mut t0, mut t1) = (0.0f32, 1.0f32);
    for (o, dd, mn, mx) in [(a.x, d.x, c.min.x, c.max.x), (a.y, d.y, c.min.y, c.max.y)] {
        if dd.abs() < 1e-9 {
            if o < mn || o > mx {
                return false;
            }
        } else {
            let (u, v) = ((mn - o) / dd, (mx - o) / dd);
            t0 = t0.max(u.min(v));
            t1 = t1.min(u.max(v));
            if t0 > t1 {
                return false;
            }
        }
    }
    true
}

/// Uma coisa que se coleta, achada no raio do spot. Ver `Ilha::coletaveis_em`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Coletavel {
    /// Identidade do corpo: e' por ela que o servidor guarda quantas coletas
    /// ja' sairam dele, e por ela que o cliente sabe qual parar de desenhar.
    pub coluna: u32,
    pub centro: glam::Vec2,
    /// 1..4 pra pedra (a cor E' o tier), 0 pra tronco, 5 pra Energia.
    pub tier: u8,
}

/// Lado da celula do indice de estorvos, em COLUNAS.
const CELULA_ESTORVO: i32 = 8;

/// Cabecalho do arquivo de altura. Se qualquer um destes mudar, o cache e'
/// descartado e a ilha e' gerada de novo — arquivo de uma semente servindo
/// como se fosse de outra e' o tipo de bug que so' aparece em producao.
const MAGICA: [u8; 4] = *b"TALT";
/// 2: a cidade aplainada entrou no gerador. Cache v1 e' relevo sem praca — o
/// servidor andaria num chao que o cliente nao desenha.
/// 3: plato da cidade maior, patio do porto e o pier erguido no relevo.
/// 4: o porto so' assenta em costa de MAR ABERTO (antes caia em lago), e o
/// patio e o pier mudaram de lugar.
// 6: cristais de Energia substituem uma fração dos veios minerais.
// 8 (21/09/2026): o cais passou a apontar pra rota entre as ilhas
// (`DefIlha::rumo_do_porto`), entao o porto MUDOU DE LUGAR e o relevo do
// patio com ele. Cache velho traria a ilha antiga com o porto novo desenhado
// por cima.
// 9 (30/09/2026): the Ermo's oasis digs a pond into the dunes (`oasis`).
// 10 (30/09/2026): the Porões are carved into a larger Arena (`planta`).
// 11 (30/09/2026): the castle walls of Last Refuge (`planalto::muralha`).
const VERSAO: u16 = 11;

impl Ilha {
    pub fn planalto(&self) -> Option<&crate::planalto::Plano> { self.ger.planalto() }
    pub fn gerar(semente: i32, raio_blocos: i32, bioma: Bioma, escala_altura: f32) -> Self {
        let p = bioma.perfil();
        Self::com_terraco(
            semente,
            raio_blocos,
            bioma,
            escala_altura,
            p.terraco_blocos,
            p.terraco_forca,
        )
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
            semente,
            raio_blocos,
            bioma,
            escala_altura,
            terraco_blocos,
            terraco_forca,
        );
        let mut blocos = vec![0i16; lado * lado];
        for iz in 0..lado {
            let bz = iz as i32 - raio_blocos;
            for ix in 0..lado {
                let bx = ix as i32 - raio_blocos;
                blocos[iz * lado + ix] = ger.bloco_em(bx, bz) as i16;
            }
        }
        Self::com_blocos(
            semente,
            raio_blocos,
            bioma,
            escala_altura,
            lado,
            blocos,
            ger,
        )
    }

    /// A ilha da COLONIA, com o PLATO do assentamento no tamanho pedido.
    ///
    /// Uma semente e um raio, sempre os mesmos (`colonia::SEMENTE`,
    /// `colonia::RAIO_BLOCOS`): o contorno da ilha nunca muda. O que o nivel
    /// muda e' so' quanto chao em volta da praca sai APLAINADO — subir o
    /// assentamento empurra a rampa pra fora e nao mexe numa coluna do que ja'
    /// estava plano. E' isso que deixa a vila crescer sem a casa do jogador
    /// mudar de altura debaixo dele.
    pub fn da_colonia(plato: f32) -> Self {
        let raio_blocos = crate::colonia::RAIO_BLOCOS;
        let lado = (raio_blocos * 2) as usize;
        let ger = Gerador::da_colonia(plato);
        let mut blocos = vec![0i16; lado * lado];
        for iz in 0..lado {
            let bz = iz as i32 - raio_blocos;
            for ix in 0..lado {
                let bx = ix as i32 - raio_blocos;
                blocos[iz * lado + ix] = ger.bloco_em(bx, bz) as i16;
            }
        }
        Self::com_blocos(
            crate::colonia::SEMENTE,
            raio_blocos,
            Bioma::Floresta,
            ESCALA_ALTURA,
            lado,
            blocos,
            ger,
        )
    }

    /// A ILHA MAGICA: as ilhotas e as pontes, plantadas.
    ///
    /// O plantio e' o normal (`plantar`), de proposito: as pedras, os troncos
    /// e a Energia das ilhotas de recurso saem do mesmo sorteio do resto do
    /// jogo. Um segundo plantador so' pra ca' seria uma segunda regra pra
    /// mesma coisa, e a primeira a ficar pra tras quando o jogo mudasse.
    pub fn da_ilha_magica() -> Self {
        let raio_blocos = crate::magica::RAIO_BLOCOS;
        let lado = (raio_blocos * 2) as usize;
        let ger = Gerador::da_ilha_magica();
        let mut blocos = vec![0i16; lado * lado];
        for iz in 0..lado {
            let bz = iz as i32 - raio_blocos;
            for ix in 0..lado {
                let bx = ix as i32 - raio_blocos;
                blocos[iz * lado + ix] = ger.bloco_em(bx, bz) as i16;
            }
        }
        Self::com_blocos(
            crate::magica::SEMENTE,
            raio_blocos,
            Bioma::Floresta,
            ESCALA_ALTURA,
            lado,
            blocos,
            ger,
        )
    }

    /// A ilha de uma zona do arquipelago, com o cais virado pra rota.
    ///
    /// E' o par de `Gerador::da_ilha`, e quem simula uma ilha de verdade tem
    /// que vir por aqui: pelos quatro argumentos soltos o rumo se perde e o
    /// servidor acaba com o porto num lugar e o cliente noutro.
    pub fn da_ilha(def: &DefIlha) -> Self {
        let lado = (def.raio_blocos * 2) as usize;
        let ger = Gerador::da_ilha(def);
        let mut blocos = vec![0i16; lado * lado];
        for iz in 0..lado {
            let bz = iz as i32 - def.raio_blocos;
            for ix in 0..lado {
                let bx = ix as i32 - def.raio_blocos;
                blocos[iz * lado + ix] = ger.bloco_em(bx, bz) as i16;
            }
        }
        Self::com_blocos(
            def.semente,
            def.raio_blocos,
            def.bioma,
            ESCALA_ALTURA,
            lado,
            blocos,
            ger,
        )
    }

    /// `carregar_ou_gerar` pra uma ilha do arquipelago.
    pub fn carregar_ou_gerar_da_ilha(dir: &str, def: &DefIlha) -> Self {
        // Ilhas desenhadas mudam de contorno sem mudar semente/raio. Um cache
        // antigo mantinha chão sólido onde o cliente já mostrava água.
        if crate::magica::e_magica(def.zona) {
            return Self::da_ilha(def);
        }

        // Drawn maps change shape without changing seed or radius, so their
        // revision is in the key: a stale cache kept the dungeon islet's
        // cellar walls where the plans used to be.
        let revisao = if def.zona == crate::planalto::ZONA {
            format!("-planalto{}", crate::planalto::REVISAO)
        } else if crate::arena::e_arena(def.zona) {
            format!("-plantas{}", crate::planta::REVISAO)
        } else if crate::celeste::e_celeste(def.zona) {
            format!("-celeste{}", crate::celeste::REVISAO)
        } else {
            String::new()
        };
        let caminho = format!("{dir}/{}-{}{revisao}.alt", def.semente, def.raio_blocos);
        if let Some(i) = Self::carregar_da_ilha(&caminho, def) {
            return i;
        }
        let ilha = Self::da_ilha(def);
        let _ = ilha.salvar(&caminho);
        ilha
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
            solidos: Vec::new(),
            grade_solidos: Vec::new(),
        };
        i.plantar();
        i.indexar_vila();
        i
    }

    /// Indexa as paredes e o poco da vila na grade de colisao.
    fn indexar_vila(&mut self) {
        let caixas = crate::vila::caixas_solidas(self.ger.vila());
        self.grade_solidos = vec![Vec::new(); self.grade_lado * self.grade_lado];
        for (n, c) in caixas.iter().enumerate() {
            let (c0x, c0z) = self.celula(c.min.x, c.min.y);
            let (c1x, c1z) = self.celula(c.max.x, c.max.y);
            for cz in c0z..=c1z {
                for cx in c0x..=c1x {
                    if let Some(k) = self.celula_em(cx, cz) {
                        self.grade_solidos[k].push(n as u32);
                    }
                }
            }
        }
        self.solidos = caixas;
    }

    /// Casas, props e NPCs desta ilha.
    pub fn vila(&self) -> &crate::vila::Vila {
        self.ger.vila()
    }

    /// O porto desta ilha. Ver `SitioPorto`.
    pub fn porto(&self) -> Option<SitioPorto> {
        self.ger.porto()
    }

    /// A agua em `p` da' no OCEANO (e nao num lago)? Ver `Gerador::mar_aberto`.
    pub fn mar_aberto(&self, p: glam::Vec2) -> bool {
        self.ger.mar_aberto(p).0
    }

    /// Alguma caixa solida no retangulo satisfaz `f`?
    fn caixas_perto(
        &self,
        x0: f32,
        z0: f32,
        x1: f32,
        z1: f32,
        mut f: impl FnMut(&crate::vila::Caixa2) -> bool,
    ) -> bool {
        if self.solidos.is_empty() {
            return false;
        }
        let (c0x, c0z) = self.celula(x0, z0);
        let (c1x, c1z) = self.celula(x1, z1);
        for cz in c0z..=c1z {
            for cx in c0x..=c1x {
                let Some(k) = self.celula_em(cx, cz) else {
                    continue;
                };
                for &n in &self.grade_solidos[k] {
                    if f(&self.solidos[n as usize]) {
                        return true;
                    }
                }
            }
        }
        false
    }

    /// Um corpo de raio `raio` em `p` encosta numa parede ou no poco?
    pub fn caixa_toca(&self, p: glam::Vec2, raio: f32) -> bool {
        self.caixas_perto(p.x - raio, p.y - raio, p.x + raio, p.y + raio, |c| {
            dist_caixa(c, p) < raio
        })
    }

    /// Nao cabe um corpo aqui: tronco, pedra ou parede.
    pub fn ocupado(&self, p: glam::Vec2, raio: f32) -> bool {
        self.estorvo_em(p, raio).is_some() || self.caixa_toca(p, raio)
    }

    /// O trecho nao atravessa parede. Como nos estorvos, caixa que ja'
    /// engloba uma das pontas e' ignorada.
    fn trecho_sem_caixa(&self, a: glam::Vec2, b: glam::Vec2, raio: f32) -> bool {
        !self.caixas_perto(
            a.x.min(b.x) - raio,
            a.y.min(b.y) - raio,
            a.x.max(b.x) + raio,
            a.y.max(b.y) + raio,
            |c| {
                let e = crate::vila::Caixa2 {
                    min: c.min - glam::Vec2::splat(raio),
                    max: c.max + glam::Vec2::splat(raio),
                };
                if dist_caixa(&e, a) < 0.0 || dist_caixa(&e, b) < 0.0 {
                    return false;
                }
                segmento_corta(&e, a, b)
            },
        )
    }

    /// Varre a ilha e guarda o que barra passagem.
    ///
    /// Publica porque quem mexe no relevo TEM que chamar de novo: o plantio
    /// depende da altura da coluna (nada nasce na agua, nem na encosta), e um
    /// indice velho vira tronco boiando ou parede invisivel.
    pub fn replantar(&mut self) {
        self.plantar();
    }

    /// Tira do indice de colisao a pedra ou o tronco da coluna `coluna` — o
    /// no' que esgotou e sumiu nao barra mais ninguem (nem conta como
    /// coletavel). Devolve o que tirou, pra `mostrar_estorvos` repor quando
    /// ele voltar. Gancho minimo: o terreno nao sabe de esgotamento; quem
    /// guarda isso e' o servidor. A forracao da coluna fica.
    pub fn esconder_coluna(&mut self, coluna: u32) -> Vec<(usize, u32)> {
        let (ix, iz) = ((coluna >> 16) as i32, (coluna & 0xffff) as i32);
        let centro = glam::Vec2::new(
            (ix - self.raio_blocos) as f32 * BLOCO,
            (iz - self.raio_blocos) as f32 * BLOCO,
        );
        let folga = self.raio_max_estorvo + BLOCO * 2.0;
        let (c0x, c0z) = self.celula(centro.x - folga, centro.y - folga);
        let (c1x, c1z) = self.celula(centro.x + folga, centro.y + folga);
        let mut tirados = Vec::new();
        for cz in c0z..=c1z {
            for cx in c0x..=c1x {
                let Some(c) = self.celula_em(cx, cz) else {
                    continue;
                };
                let estorvos = &self.estorvos;
                self.grade[c].retain(|&n| {
                    let e = estorvos[n as usize];
                    let sai = e.coluna == coluna && !matches!(e.tipo, TipoDeEstorvo::Forracao);
                    if sai {
                        tirados.push((c, n));
                    }
                    !sai
                });
            }
        }
        tirados
    }

    /// Repoe no indice o que `esconder_coluna` tirou.
    pub fn mostrar_estorvos(&mut self, tirados: &[(usize, u32)]) {
        for &(c, n) in tirados {
            if let Some(v) = self.grade.get_mut(c) {
                if !v.contains(&n) {
                    v.push(n);
                }
            }
        }
    }

    /// A cidade desta ilha. Ver `Cidade`.
    pub fn cidade(&self) -> Option<Cidade> {
        self.ger.cidade()
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
                    self.bioma,
                    bx,
                    bz,
                    topo,
                    declive,
                    &self.ger,
                    false,
                    &mut achados,
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
        if !self.ocupado(centro, raio) {
            return centro;
        }
        // Espiral curta: o estorvo mais gordo tem 1,24 de diametro, entao um
        // ponto livre esta' sempre a menos de uma unidade — se houver.
        for anel in 1..=3 {
            let d = anel as f32 * BLOCO;
            for (dx, dz) in [
                (1.0, 0.0),
                (-1.0, 0.0),
                (0.0, 1.0),
                (0.0, -1.0),
                (0.7, 0.7),
                (0.7, -0.7),
                (-0.7, 0.7),
                (-0.7, -0.7),
            ] {
                let p = centro + glam::Vec2::new(dx * d, dz * d);
                if !self.ocupado(p, raio) && !self.agua(p.x, p.y) {
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
                let Some(c) = self.celula_em(cx, cz) else {
                    continue;
                };
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
                let Some(c) = self.celula_em(cx, cz) else {
                    continue;
                };
                for &n in &self.grade[c] {
                    let e = self.estorvos[n as usize];
                    let tier = match e.tipo {
                        TipoDeEstorvo::Minerio(t) => t,
                        TipoDeEstorvo::Energia => 5,
                        TipoDeEstorvo::Tronco => 0,
                        TipoDeEstorvo::Forracao => continue,
                    };
                    if e.centro.distance_squared(p) > raio_sq {
                        continue;
                    }
                    // Um corpo grande entra em varias celulas: so' conta na
                    // que tem o centro dele, senao pedra grande vale por
                    // quatro e a densidade mente.
                    if self.celula(e.centro.x, e.centro.y) != (cx, cz) {
                        continue;
                    }
                    saida.push(Coletavel {
                        coluna: e.coluna,
                        centro: e.centro,
                        tier,
                    });
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
                let Some(c) = self.celula_em(cx, cz) else {
                    continue;
                };
                for &n in &self.grade[c] {
                    let e = self.estorvos[n as usize];
                    let d = raio + e.raio;
                    if e.centro.distance_squared(p) < d * d {
                        return false;
                    }
                }
            }
        }
        !self.caixa_toca(p, raio)
    }

    /// O passo de `de` pra `para` e' aceitavel pros estorvos?
    ///
    /// Livre, ou — pra quem JA' esta' dentro de um tronco — sem entrar mais
    /// nele. Perguntar so' `sem_estorvo(para)` prendia pra sempre quem comecou
    /// sobreposto: todo passo terminava encostado na arvore, inclusive o de
    /// sair. Medido: 600 de 600 corpos plantados dentro do tronco nao saiam.
    /// E sobreposicao acontece — slot de spawn que nao olhou a arvore, ou o
    /// empurrao entre corpos.
    pub fn cabe(&self, de: glam::Vec2, para: glam::Vec2, raio: f32) -> bool {
        if self.ger.e_magica() && self.agua(para.x, para.y) { return false; }

        let (c0x, c0z) = self.celula(de.x.min(para.x) - raio, de.y.min(para.y) - raio);
        let (c1x, c1z) = self.celula(de.x.max(para.x) + raio, de.y.max(para.y) + raio);
        // Por tronco: entrar MAIS em algum reprova. Mas num bolsao de dois,
        // sair de um e' chegar mais perto do outro, e essa regra sozinha
        // prendia pra sempre quem ja' estava dentro. Entao, pra quem ja' esta'
        // sobreposto, vale tambem a SOMA: se a sobreposicao total diminui, o
        // passo passa. Corpo livre continua sem poder entrar em nada — a soma
        // dele parte de zero e so' pode subir.
        let mut viola = false;
        let (mut pen_de, mut pen_para) = (0.0f32, 0.0f32);
        let mut vistos: Vec<u32> = Vec::new();
        for cz in c0z..=c1z {
            for cx in c0x..=c1x {
                let Some(c) = self.celula_em(cx, cz) else {
                    continue;
                };
                for &n in &self.grade[c] {
                    if vistos.contains(&n) {
                        continue;
                    }
                    vistos.push(n);
                    let e = self.estorvos[n as usize];
                    let d = raio + e.raio;
                    let la = e.centro.distance_squared(para);
                    let ld = e.centro.distance_squared(de);
                    pen_para += (d - la.sqrt()).max(0.0);
                    pen_de += (d - ld.sqrt()).max(0.0);
                    if la < d * d && la < ld - 1e-6 {
                        viola = true;
                    }
                }
            }
        }
        if viola && !(pen_de > 0.0 && pen_para < pen_de - 1e-4) {
            return false;
        }
        // Parede: a mesma regra — livre, ou saindo dela.
        !self.caixas_perto(
            para.x - raio,
            para.y - raio,
            para.x + raio,
            para.y + raio,
            |c| {
                let dp = dist_caixa(c, para);
                dp < raio && dp < dist_caixa(c, de) - 1e-6
            },
        )
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
                let Some(c) = self.celula_em(cx, cz) else {
                    continue;
                };
                for &n in &self.grade[c] {
                    let e = self.estorvos[n as usize];
                    let t = if comp2 < 1e-12 {
                        0.0
                    } else {
                        ((e.centro - a).dot(ab) / comp2).clamp(0.0, 1.0)
                    };
                    let d = raio + e.raio;
                    if e.centro.distance_squared(a) < d * d || e.centro.distance_squared(b) < d * d
                    {
                        continue;
                    }
                    if (a + ab * t).distance_squared(e.centro) < d * d {
                        return false;
                    }
                }
            }
        }
        self.trecho_sem_caixa(a, b, raio)
    }

    /// Quantos estorvos a ilha tem. So' pra medir.
    /// Todos os corpos plantados. Pra medicao e pro servidor montar indice
    /// proprio — no jogo se pergunta pelo raio, nunca pela lista inteira.
    pub fn todos_os_estorvos(&self) -> &[Estorvo] {
        &self.estorvos
    }

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

    /// Move um casco pela agua DESTA ilha (ver `mover_casco`). E' o avesso
    /// do `mover_e_deslizar`: aqui terra e' que barra.
    pub fn mover_no_mar(
        &self,
        pos: glam::Vec2,
        vel: glam::Vec2,
        dt: f32,
        raio: f32,
    ) -> glam::Vec2 {
        mover_casco(&|x, z| self.agua(x, z), pos, vel, dt, raio)
    }

    /// Da' pra ANDAR de um ponto ao outro? Um bloco de subida passa como
    /// ladder; mais que isso, nao.
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
        // ── VARREDURA: passo grande vira varios pequenos ──
        //
        // O comentario acima previa isto. MONTADO o passo cresce 1,5x, e o
        // corpo travava perto de quina e tronco: as cinco alternativas da
        // tangente sao medidas pelo COMPRIMENTO do passo, entao quanto maior
        // o passo mais facil TODAS serem reprovadas — e o corpo para de vez.
        // A pe' o passo e' curto e cabe; montado, nao. Dividir em pedacos de
        // no maximo `PASSO_MAX` reusa toda a logica abaixo com o passo curto
        // de sempre.
        const PASSO_MAX: f32 = 0.15;
        let anda = (vel * dt).length();
        if anda > PASSO_MAX {
            let n = (anda / PASSO_MAX).ceil().min(8.0);
            let mut p = pos;
            for _ in 0..n as u32 {
                let novo = self.mover_com_degrau(p, vel, dt / n, raio, degrau);
                // Parou de vez: insistir nos pedacos seguintes so' gasta.
                if novo.distance_squared(p) <= 1e-9 {
                    return p;
                }
                p = novo;
            }
            return p;
        }
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
                // A tangente pode estar barrada — outro tronco colado, ou um
                // degrau encostado na arvore. Abrir um pouco pra fora resolve
                // o tronco vizinho; o OUTRO lado resolve o degrau. Sem as
                // alternativas o corpo parava de frente pra arvore e o eixo a
                // eixo abaixo tambem reprovava tudo.
                // Por ultimo, SAIR reto do tronco: preso num bolsao de dois,
                // nenhuma tangente cabe, mas afastar-se do centro sempre
                // reduz a sobreposicao.
                for dir in [
                    lado,
                    (lado + fora).normalize_or_zero(),
                    -lado,
                    (-lado + fora).normalize_or_zero(),
                    fora,
                ] {
                    let alvo = pos + dir * v.length();
                    if self.borda_livre(pos, alvo, raio, dir, degrau) && self.cabe(pos, alvo, raio)
                    {
                        return alvo;
                    }
                }
            }
        }
        // Eixo a eixo pro RELEVO: barrado num eixo, continua andando no outro.
        if v.x != 0.0 {
            let alvo = glam::Vec2::new(p.x + v.x, p.y);
            if self.borda_livre(p, alvo, raio, glam::Vec2::new(v.x.signum(), 0.0), degrau)
                && self.cabe(p, alvo, raio)
            {
                p.x = alvo.x;
            }
        }
        if v.y != 0.0 {
            let alvo = glam::Vec2::new(p.x, p.y + v.y);
            if self.borda_livre(p, alvo, raio, glam::Vec2::new(0.0, v.y.signum()), degrau)
                && self.cabe(p, alvo, raio)
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
        self.caminho_evitando(de, para, orcamento, &[])
    }

    /// O mesmo caminho, mas DESVIANDO dos pontos onde o corpo já emperrou.
    ///
    /// O dono: "caso ele ficar preso por 5 segundos parados tentando seguir
    /// uma rota, ele tenta mudar de rota automaticamente para tentar
    /// contornar, mesmo sendo um caminho mais longo".
    ///
    /// Sem isto, travar não adiantava nada: o seguidor avisava, o servidor
    /// refazia a rota, e o A* — que não sabe que algo deu errado — devolvia
    /// EXATAMENTE o mesmo caminho. O corpo raspava a mesma quina para sempre.
    ///
    /// O desvio é CARO, não proibido: uma célula perto de um ponto emperrado
    /// custa `CUSTO_DO_DESVIO` a mais. Proibir poderia deixar o destino sem
    /// caminho nenhum — e aí o jogador não iria a lugar nenhum, que é pior
    /// que ir pelo caminho longo. Encarecer faz o A* dar a volta quando há
    /// volta, e ainda assim passar por ali quando não há outro jeito.
    pub fn caminho_evitando(
        &self,
        de: glam::Vec2,
        para: glam::Vec2,
        orcamento: usize,
        evitar: &[glam::Vec2],
    ) -> Option<Vec<glam::Vec2>> {
        use std::collections::{BinaryHeap, HashMap};

        // Destino dentro de um tronco — o jogador indo ate' um bicho que esta'
        // colado na arvore — vira o ponto livre mais perto. O ultimo trecho
        // ignora estorvo na ponta, entao sem isto a rota terminava DENTRO da
        // arvore e o corpo empurrava o tronco ate' o seguidor pedir rota nova,
        // que dava o mesmo ponto.
        let para = if self.ocupado(para, crate::constants::ENTITY_RADIUS) {
            self.ponto_livre_perto(para, crate::constants::ENTITY_RADIUS)
        } else {
            para
        };

        // ── REDE DE SEGURANÇA: começar de dentro de uma casa ──
        //
        // O dono relatou ficar preso dentro da casa de NPC depois de alguns
        // teleportes: "o A* não sabe sair disso". A célula da grade tem 4
        // unidades (`BLOCO` × `PASSO_CAMINHO`) e uma porta é mais estreita,
        // então a grade não representa a porta — o que torna a queixa
        // plausível.
        //
        // HONESTIDADE SOBRE O QUE ESTÁ PROVADO: eu NÃO consegui reproduzir o
        // travamento em teste. Pondo o personagem no miolo da caixa de
        // colisão de seis prédios da ilha inicial, o A* acha caminho COM ou
        // SEM esta guarda (conferido removendo-a e rodando). Ou o caso real é
        // outro — teleporte para um ponto específico, prédio de outra ilha —,
        // ou depende de estado que o teste não monta.
        //
        // A guarda fica porque só dispara quando a origem está literalmente
        // dentro de uma caixa de prédio, e nesse caso sair pela porta é o que
        // a pessoa faria. Mas ela é rede, não conserto demonstrado: se o
        // travamento voltar, é aqui que se começa E é preciso um caso
        // concreto (qual casa, qual teleporte) antes de mexer na grade.
        if let Some(saida) = crate::vila::saida_do_predio(self.vila(), de) {
            if saida.distance(de) > 0.01 {
                let resto = self.caminho(saida, para, orcamento)?;
                let mut rota = Vec::with_capacity(resto.len() + 1);
                rota.push(saida);
                rota.extend(resto);
                return Some(rota);
            }
        }

        let cel = |p: glam::Vec2| -> (i32, i32) {
            (
                (p.x / (BLOCO * PASSO_CAMINHO as f32)).round() as i32,
                (p.y / (BLOCO * PASSO_CAMINHO as f32)).round() as i32,
            )
        };
        // O ponto de uma celula nao e' sempre o centro dela: se houver um
        // tronco ali, ele anda pro lado. Guardado num mapa porque a mesma
        // celula e' visitada como vizinha de varias outras.
        // O PONTO DE UMA CÉLULA DO CAIS FICA SOBRE A PRANCHA.
        //
        // A prancha tem 2,5 unidades de largura e a célula tem 4: ela não
        // cabe na grade, e os centros de célula do cais caem na água dos dois
        // lados. Sem isto, a rota até o Capitão do Porto parava a 117
        // unidades dele (533 no Planalto) e o cais era intransitável no
        // auto-path — "não consigo andar na prancha do porto com o A*".
        //
        // Meia diagonal da célula (2,83) é o alcance certo: é até onde um
        // ponto da prancha pode estar do centro de uma célula que a cruza.
        let no_cais = |bruto: glam::Vec2| -> Option<glam::Vec2> {
            let meia_diagonal = BLOCO * PASSO_CAMINHO as f32 * std::f32::consts::SQRT_2 * 0.5;
            self.porto()?.no_deck(bruto, meia_diagonal)
        };
        let ponto_da_celula = |c: (i32, i32)| -> glam::Vec2 {
            let cru = glam::Vec2::new(
                c.0 as f32 * BLOCO * PASSO_CAMINHO as f32,
                c.1 as f32 * BLOCO * PASSO_CAMINHO as f32,
            );
            match no_cais(cru) {
                Some(deck) if !self.ocupado(deck, crate::constants::ENTITY_RADIUS) => deck,
                _ => self.ponto_livre_perto(cru, crate::constants::ENTITY_RADIUS),
            }
        };
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
            // O anel CRESCE ate' achar porta. So' os oito vizinhos nao bastam
            // no CAIS: a celula tem 4 unidades e o cais e' uma lingua estreita
            // entrando no mar, entao da metade dele pra ponta os nove
            // candidatos caem todos na agua. O A* saia de um no' sem vizinho e
            // o jogador tocava no mapa sem nada acontecer — medido em
            // 21/09/2026: 7 dos 11 pontos do cais da ilha inicial sem rota.
            //
            // Alargar e' seguro porque o `trecho_livre` continua mandando:
            // aceita-se a celula mais PERTO que o corpo realmente alcanca em
            // linha reta, e do cais a linha reta de volta e' o proprio cais.
            //
            // O teto cobre um cais generoso (24 u). Alem dele nao ha' o que
            // achar: quem esta' a mais de 24 u de qualquer chao alcancavel
            // esta' no mar, e nao ha' rota a pe' nenhuma.
            const ANEIS_MAX: i32 = 6;
            let mut melhor = f32::MAX;
            // Comeca em 0 — a propria celula do corpo e' candidata. Pular o
            // anel 0 tirava a porta mais obvia de todas e mudava a rota em
            // terreno comum.
            for r in 0..=ANEIS_MAX {
                for dz in -r..=r {
                    for dx in -r..=r {
                        // So' o anel de raio r: o miolo ja' foi visto.
                        if dx.abs() != r && dz.abs() != r {
                            continue;
                        }
                        let c = (contendo.0 + dx, contendo.1 + dz);
                        let p = *pontos.entry(c).or_insert_with(|| {
                            ponto_da_celula(c)
                        });
                        // Alcancavel NAO basta: a porta tem que estar LIGADA
                        // ao grafo. O cais e' mais estreito que os 4 u da
                        // celula, entao ha' celulas dele que o corpo alcanca
                        // mas cujos oito vizinhos sao todos agua: o A* expande
                        // uma vez, nao acha ninguem, e devolve `None`. Era
                        // isso, e nao a distancia, que fazia a rota falhar em
                        // uns pontos do cais e funcionar nos vizinhos.
                        let d = de.distance(p);
                        if d >= melhor || !self.trecho_livre(de, p, PULO_BLOCOS) {
                            continue;
                        }
                        // Mesmo criterio que o A* usa pra andar de celula em
                        // celula — se fosse outro, a porta aprovaria o que a
                        // expansao recusa.
                        let mut ligada = false;
                        for (vx, vz) in [
                            (1, 0),
                            (-1, 0),
                            (0, 1),
                            (0, -1),
                            (1, 1),
                            (1, -1),
                            (-1, 1),
                            (-1, -1),
                        ] {
                            let vc = (c.0 + vx, c.1 + vz);
                            let pv = *pontos.entry(vc).or_insert_with(|| {
                                ponto_da_celula(vc)
                            });
                            if !self.ocupado(pv, crate::constants::ENTITY_RADIUS)
                                && self.trecho_livre(p, pv, PULO_BLOCOS)
                            {
                                ligada = true;
                                break;
                            }
                        }
                        if ligada {
                            melhor = d;
                            inicio = c;
                        }
                    }
                }
                // Os aneis 0 e 1 sao o 3x3 de sempre e vao JUNTOS: o mais
                // perto entre os nove pode ser um vizinho, e nao o centro.
                // Do 2 em diante e' so' resgate de quem ficou sem porta.
                if r >= 1 && melhor < f32::MAX {
                    break;
                }
            }
        }
        let fim = cel(para);
        if inicio == fim {
            // Mesma celula: reto so' se o reto for livre. Era "vai reto" sem
            // checar nada — e a celula grossa cabe um poco inteiro: a rota
            // atravessava o poco da praca (19/09/2026).
            if self.trecho_livre(de, para, PULO_BLOCOS) {
                return Some(vec![para]);
            }
            // Contorna por uma vizinha que enxerga as duas pontas.
            let mut desvio: Option<(f32, glam::Vec2)> = None;
            for dz in -1..=1 {
                for dx in -1..=1 {
                    if dx == 0 && dz == 0 {
                        continue;
                    }
                    let v = self.ponto_livre_perto(
                        bruto((inicio.0 + dx, inicio.1 + dz)),
                        crate::constants::ENTITY_RADIUS,
                    );
                    if self.ocupado(v, crate::constants::ENTITY_RADIUS)
                        || !self.trecho_livre(de, v, PULO_BLOCOS)
                        || !self.trecho_livre(v, para, PULO_BLOCOS)
                    {
                        continue;
                    }
                    let d = de.distance(v) + v.distance(para);
                    if desvio.is_none_or(|(m, _)| d < m) {
                        desvio = Some((d, v));
                    }
                }
            }
            return Some(match desvio {
                Some((_, v)) => vec![v, para],
                None => vec![para],
            });
        }

        // f e' negado: BinaryHeap e' max-heap e o A* quer o menor.
        #[derive(PartialEq, Eq)]
        struct No(i64, (i32, i32));
        impl Ord for No {
            fn cmp(&self, o: &Self) -> std::cmp::Ordering {
                self.0.cmp(&o.0)
            }
        }
        impl PartialOrd for No {
            fn partial_cmp(&self, o: &Self) -> Option<std::cmp::Ordering> {
                Some(self.cmp(o))
            }
        }
        let h = |c: (i32, i32)| -> i64 {
            let (dx, dz) = ((c.0 - fim.0).abs() as i64, (c.1 - fim.1).abs() as i64);
            // Octil: diagonal custa ~1,41 e reta 1, em milesimos.
            let (mi, ma) = (dx.min(dz), dx.max(dz));
            // A* PONDERADO (×1,3).
            //
            // A heurística octil subestima MUITO o custo real: o passo paga
            // pulo, degrau e desvio de estorvo, tudo acima dos 1.000
            // milésimos que ela supõe. O A* então abria meia ilha antes de
            // chegar, e ESTOURAVA O ORÇAMENTO nas viagens longas — medido: a
            // rota da praça até o Capitão do Porto parava a 117 unidades dele
            // (533 no Planalto), e o dono relatou duas vezes "não consigo
            // andar na prancha do porto com o A*".
            //
            // Com o peso, o mesmo orçamento de 6.000 acha o caminho inteiro:
            // ZERO NPCs fora de alcance na ilha inicial e no Planalto, e o
            // pior tempo do Planalto caiu de 120 ms para 5,2. O preço é uma
            // rota até 30% mais longa que a ótima — invisível em jogo, e
            // muito melhor que não haver rota.
            (mi * 1414 + (ma - mi) * 1000) * 13 / 10
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
                (1, 0),
                (-1, 0),
                (0, 1),
                (0, -1),
                (1, 1),
                (1, -1),
                (-1, 1),
                (-1, -1),
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
                    ponto_da_celula(atual)
                });
                let pv = *pontos.entry(viz).or_insert_with(|| {
                    ponto_da_celula(viz)
                });
                // Mata fechada: sem ponto livre perto do centro, o ponto da
                // celula ficou DENTRO do tronco — e o trecho, que ignora
                // estorvo nas pontas, aprovaria a entrada. Nao e' passagem.
                if viz != fim && self.ocupado(pv, crate::constants::ENTITY_RADIUS) {
                    continue;
                }
                let passo = if self.trecho_livre(pa, pv, DEGRAU_BLOCOS) {
                    base
                } else if self.trecho_livre(pa, pv, PULO_BLOCOS) {
                    base + CUSTO_DO_PULO
                } else {
                    continue;
                };
                // O PEDÁGIO DO DESVIO. Ver `caminho_evitando`.
                let pedagio = evitar
                    .iter()
                    .filter(|e| e.distance(pv) < RAIO_DO_DESVIO)
                    .count() as i64
                    * CUSTO_DO_DESVIO;
                let novo = g + passo + pedagio;
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
        let mut saida: Vec<glam::Vec2> = rota
            .into_iter()
            .map(|c| {
                *pontos.entry(c).or_insert_with(|| {
                    ponto_da_celula(c)
                })
            })
            .collect();
        // A celula de onde se saiu so' sai da rota se o corpo ENXERGA a
        // seguinte. O A* validou "porta -> segunda", nao "corpo -> segunda":
        // pular sem checar mandava o corpo cortar caminho pelo que estivesse
        // entre os dois — o poco da praca, medido (19/09/2026).
        if inicio == contendo && saida.len() >= 2 && self.trecho_livre(de, saida[1], PULO_BLOCOS) {
            saida.remove(0);
        }
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
            let centro = *pontos.entry(fim).or_insert_with(|| {
                ponto_da_celula(fim)
            });
            if self.trecho_livre(penultimo, para, PULO_BLOCOS) {
                saida.push(para);
            } else if self.trecho_livre(centro, para, PULO_BLOCOS) {
                // O atalho do penultimo esbarra (no poco da praca, medido), mas
                // do centro da ultima celula da' pra pisar: passa por ele.
                saida.push(centro);
                saida.push(para);
            } else {
                // NÃO DÁ PRA PISAR ONDE CLICOU: chega o mais perto que der.
                //
                // Parar no centro da célula deixava o corpo a até 3,09
                // unidades do NPC (medido no Alfaiate da Geleira) — e o
                // servidor só aceita interagir a 3. Um passo a mais e a
                // conversa acontece; um a menos e o jogador fica olhando pro
                // NPC sem conseguir falar, que é o pior jeito de falhar.
                //
                // Marcha do centro na direção do alvo enquanto o trecho for
                // livre, e guarda o último ponto que passou.
                let mut melhor = centro;
                let passo = BLOCO;
                let dir = (para - centro).normalize_or_zero();
                let total = centro.distance(para);
                let mut t = passo;
                while t < total {
                    let q = centro + dir * t;
                    if !self.trecho_livre(centro, q, PULO_BLOCOS)
                        || self.ocupado(q, crate::constants::ENTITY_RADIUS)
                    {
                        break;
                    }
                    melhor = q;
                    t += passo;
                }
                saida.push(melhor);
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
    #[cfg(test)]
    pub(crate) fn trecho_livre_publico(&self, de: glam::Vec2, para: glam::Vec2) -> bool {
        self.trecho_livre(de, para, PULO_BLOCOS)
    }

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
            // Every lane climbs FROM THE CENTRE, the way `borda_livre` moves
            // the body: it compares the leading edge with the column under
            // the body's centre. Measuring each lane against its own
            // previous sample let a shoulder on a sideways slope climb
            // 2 -> 3 -> 6 in legal steps while the body, standing on 2, met
            // a 4-block wall at the shoulder: the route went into a corner
            // no jump clears, and the follower stood there asking for the
            // same route (`terreno --rotas`, 01/10/2026).
            let (cx, cz) = self.coluna(anterior.x, anterior.y);
            let base = self.bloco(cx, cz);
            for lado in [glam::Vec2::ZERO, perp, -perp] {
                let q = p + lado;
                if self.agua(q.x, q.y) {
                    return false;
                }
                let (qx, qz) = self.coluna(q.x, q.y);
                if self.bloco(qx, qz) - base > degrau {
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
        let i32em =
            |i: usize| i32::from_le_bytes([dados[i], dados[i + 1], dados[i + 2], dados[i + 3]]);
        let f32em =
            |i: usize| f32::from_le_bytes([dados[i], dados[i + 1], dados[i + 2], dados[i + 3]]);
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

    /// `carregar` pra uma ilha do arquipelago: o cache guarda so' altura, mas
    /// o `Gerador` que volta junto precisa saber o rumo do cais.
    fn carregar_da_ilha(caminho: &str, def: &DefIlha) -> Option<Self> {
        let i = Self::carregar(
            caminho,
            def.semente,
            def.raio_blocos,
            def.bioma,
            ESCALA_ALTURA,
        )?;
        Some(Self::com_blocos(
            def.semente,
            def.raio_blocos,
            def.bioma,
            ESCALA_ALTURA,
            i.lado,
            i.blocos,
            Gerador::da_ilha(def),
        ))
    }

    /// Quanto desta ilha serve pra jogar.
    ///
    /// MMO nao precisa de relevo bonito, precisa de CHAO: mob, chefe e briga
    /// querem area plana, e ruido puro entrega quase tudo inclinado. Sem medir
    /// isso, "mais areas planas" e' opiniao — com o numero na mao da' pra
    /// afinar o gerador ate' o mundo servir.
    pub fn estatisticas(&self) -> Estatisticas {
        let mut e = Estatisticas {
            colunas: self.lado * self.lado,
            ..Default::default()
        };
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
            let frac = if vistos == 0 {
                0.0
            } else {
                bons as f32 / vistos as f32
            };
            if campo == 0 {
                e.sitio_mob = frac
            } else {
                e.sitio_chefe = frac
            }
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
            for (dx, dz) in [
                (d, 0),
                (-d, 0),
                (0, d),
                (0, -d),
                (d * 7 / 10, d * 7 / 10),
                (-d * 7 / 10, d * 7 / 10),
                (d * 7 / 10, -d * 7 / 10),
                (-d * 7 / 10, -d * 7 / 10),
            ] {
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
    pub fn pct_terra(&self) -> f32 {
        pct(self.terra, self.colunas)
    }
    /// Das colunas de TERRA — que e' o que interessa: agua nao e' chao ruim,
    /// e' outra coisa.
    pub fn pct_plana(&self) -> f32 {
        pct(self.plana, self.terra)
    }
    pub fn pct_andavel(&self) -> f32 {
        pct(self.andavel, self.terra)
    }
    pub fn pct_parede(&self) -> f32 {
        pct(self.parede, self.terra)
    }
}

fn pct(a: usize, b: usize) -> f32 {
    if b == 0 {
        0.0
    } else {
        a as f32 * 100.0 / b as f32
    }
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

    /// Os pontos que faltam, na ordem. Pro tracejado do cliente.
    pub fn pontos(&self) -> impl Iterator<Item = glam::Vec2> + '_ {
        self.pontos.iter().copied()
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

    /// The body is jumping, or waiting out the jump cooldown: this tick is
    /// not "no progress". Call it before `direcao` on those ticks.
    ///
    /// Without it a staircase of 3-block steps never got climbed: pressed
    /// against each step waiting for the next jump, the follower ran out of
    /// `PACIENCIA` mid-climb, the new route — planned from that half-way
    /// spot — started by walking back down to its cell's point, and the
    /// body climbed, gave up and walked down forever (`terreno --rotas`,
    /// 01/10/2026: 97 jumps, never arrived).
    pub fn aguenta(&mut self) {
        self.sem_avanco = 0;
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

/// Quão longe de um ponto emperrado a célula ainda paga pedágio.
///
/// Uma célula e meia (a célula tem 4 unidades). Menos que isso e o desvio não
/// sai do lugar onde o corpo está preso; muito mais e ele daria voltas
/// absurdas por causa de uma quina.
const RAIO_DO_DESVIO: f32 = 6.0;

/// O que custa passar perto de onde já se emperrou, em milésimos de passo.
///
/// Doze passos retos. É caro o bastante pra o A* preferir qualquer volta
/// razoável, e barato o bastante pra ele ainda passar por ali quando é o
/// único caminho — que é o caso de um corredor estreito onde o estorvo era
/// outro jogador, e ele já saiu.
const CUSTO_DO_DESVIO: i64 = 12_000;

/// Mob indo atras de um alvo: RETO enquanto o reto avanca, A* quando empaca.
///
/// O mob nao pede rota a cada tick como o clique do jogador: sao centenas
/// deles, e em campo aberto a reta ja' e' o caminho. Mas reta mais deslize nao
/// sai de bolsao — dois troncos vizinhos, o corpo escorrega de um pro outro e
/// volta, a velocidade cheia, sem nunca chegar. Medido: 7 de 281 perseguicoes
/// com tronco no meio oscilavam assim pra sempre.
///
/// "Empacou" e' AVANCO NA DIRECAO DO ALVO, e nao distancia andada: quem
/// oscila anda muito e avanca nada.
#[derive(Debug, Default, Clone)]
pub struct Perseguicao {
    rota: SeguidorDeRota,
    ultima: Option<(glam::Vec2, glam::Vec2, f32)>,
    avanco: f32,
    ticks: u32,
    pediu_em: Option<f32>,
}

impl Perseguicao {
    /// Janela de medida, em chamadas. Um terco de segundo a 30Hz.
    const JANELA: u32 = 10;
    /// Menos que isto do avanco possivel na janela = empacou.
    const MINIMO: f32 = 0.3;
    /// Um A* por mob por segundo, no maximo.
    const INTERVALO_S: f32 = 1.0;
    /// Rota de mob e' curta: contornar a arvore, nao atravessar a ilha.
    const ORCAMENTO: usize = 1_500;
    /// O alvo andou isto desde a rota: ela ficou velha.
    const ALVO_MUDOU: f32 = 2.0;
    /// Intervalo entre chamadas que zera a medida (o mob parou pra bater).
    const LACUNA_S: f32 = 0.25;

    /// Direcao pra andar neste tick. `passo` e' quanto o corpo anda num tick
    /// livre (velocidade vezes dt).
    pub fn direcao(
        &mut self,
        ilha: &Ilha,
        pos: glam::Vec2,
        alvo: glam::Vec2,
        passo: f32,
        agora: f32,
    ) -> glam::Vec2 {
        let reto = (alvo - pos).normalize_or_zero();
        match self.ultima {
            Some((antes, dir, t)) if agora - t <= Self::LACUNA_S => {
                self.avanco += (pos - antes).dot(dir);
                self.ticks += 1;
            }
            _ => {
                self.avanco = 0.0;
                self.ticks = 0;
            }
        }
        self.ultima = Some((pos, reto, agora));

        if !self.rota.vazia() {
            if self.rota.travado() || self.rota.destino().distance(alvo) > Self::ALVO_MUDOU {
                self.rota.limpa();
            } else if let Some(dir) = self.rota.direcao(pos) {
                // Seguindo rota o avanco na reta pode ser negativo de
                // proposito (contornando): a medida so' vale fora dela.
                self.avanco = 0.0;
                self.ticks = 0;
                return dir;
            }
        }

        if self.ticks >= Self::JANELA {
            let empacou = self.avanco < Self::JANELA as f32 * passo * Self::MINIMO;
            self.avanco = 0.0;
            self.ticks = 0;
            let pode = self.pediu_em.is_none_or(|t| agora - t >= Self::INTERVALO_S);
            if empacou && pode {
                self.pediu_em = Some(agora);
                if let Some(r) = ilha.caminho(pos, alvo, Self::ORCAMENTO) {
                    self.rota = SeguidorDeRota::nova(r, alvo);
                    if let Some(dir) = self.rota.direcao(pos) {
                        return dir;
                    }
                }
            }
        }
        reto
    }

    /// Alvo perdido, mob voltou pra casa: a rota velha nao serve mais.
    pub fn esquece(&mut self) {
        *self = Self::default();
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

    /// Pra onde o CAIS desta ilha tem que apontar: o rumo do mar entre as
    /// ilhas, que e' a direcao do centro de massa das OUTRAS.
    ///
    /// Antes de 21/09/2026 o porto so' precisava de uma costa plana e longe
    /// da cidade, porque o mar era cenario e a viagem era um clique. Agora a
    /// travessia e' navegada, e um cais virado pro lado errado custa a volta
    /// inteira na ilha: medido no arquipelago de hoje, Bosque->Geleira dava
    /// 2833 u de cais a cais contra 2110 de centro a centro.
    ///
    /// Derivado da tabela em vez de escrito a mao, pra mover uma ilha
    /// continuar dando cais que se olham. `None` = ilha que nao esta' no
    /// arquipelago (as dos testes), e ai' vale a regra antiga.
    pub fn rumo_do_porto(&self) -> Option<glam::Vec2> {
        // Only the sea islands: Skyreach has no port, and counting it would
        // turn every existing dock (and move every port village).
        let do_mar = |d: &&DefIlha| !crate::celeste::e_celeste(d.zona);
        let outras: Vec<&DefIlha> = ARQUIPELAGO.iter().filter(do_mar).filter(|d| d.zona != self.zona).collect();
        if outras.len() + 1 != ARQUIPELAGO.iter().filter(do_mar).count() {
            return None;
        }
        let n = outras.len() as f32;
        let alvo = outras
            .iter()
            .fold(glam::Vec2::ZERO, |a, d| a + glam::Vec2::from(d.centro))
            / n;
        (alvo - glam::Vec2::from(self.centro)).try_normalize()
    }
}

/// O arquipelago do play test: quatro ilhas, progressao pro OESTE.
///
/// A inicial e a final tem 800 m de raio; as duas do meio, 400. Nao ha' nada
/// de sagrado nesses numeros — o relevo e' funcao da semente e do raio, entao
/// mudar o tamanho de uma ilha e' trocar um campo aqui.
pub const ARQUIPELAGO: [DefIlha; 5] = [
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
        nome: "Planalto da Tormenta",
        semente: 0x3011_7A04,
        raio_blocos: 1600,
        bioma: Bioma::Montanha,
        centro: [-3600.0, 0.0],
        nivel: (40, 60),
    },
    // Skyreach: drawn sky islands, no port (`celeste`). Index 4.
    crate::celeste::DEF,
];

#[cfg(test)]
mod testes_da_ilha_magica {
    use super::*;

    /// A ILHA de verdade — gerada, plantada e caminhada.
    ///
    /// `magica::toda_ilhota_se_alcanca_a_pe` prova a GEOMETRIA; este prova o
    /// MUNDO: que o campo de altura saiu do desenho, que a ponte e' chao seco
    /// e que `caminho` (o A\* do jogo) leva da chegada a toda ilhota. Uma
    /// ponte que existe no desenho e some no relevo prende o jogador com meia
    /// hora de passe correndo — foi exatamente o que o cais da colonia fez.
    #[test]
    fn da_chegada_se_anda_ate_toda_ilhota() {
        let ilha = Ilha::da_ilha_magica();
        let chegada = crate::magica::CHEGADA;
        assert!(!ilha.agua(chegada.x, chegada.y), "a chegada esta' na agua");
        for i in crate::magica::ilhotas() {
            let c = i.centro;
            assert!(
                !ilha.agua(c.x, c.y),
                "{}: o centro dela e' agua",
                i.bonus.nome()
            );
            let rota = ilha.caminho(chegada, c, 400_000);
            assert!(
                rota.is_some(),
                "{}: sem rota a pe' da chegada — o jogador fica vendo o bonus \
                 do outro lado da agua com o passe correndo",
                i.bonus.nome()
            );
        }
        // E o MAR em volta e' mar: sem isso as ilhotas seriam um continente.
        let fora = crate::magica::raio_do_mundo();
        assert!(ilha.agua(fora, 0.0), "o lado de fora tinha que ser mar");
    }

    /// A ilhota de recurso TEM o recurso dela.
    ///
    /// O plantio e' o normal (ruido do gerador), entao a semente e' que
    /// decide — e uma semente ruim deixa a Ilhota da Pedra sem pedra. O
    /// jogador atravessaria a ponte pra colher o dobro de nada.
    #[test]
    fn toda_ilhota_de_recurso_tem_o_recurso() {
        let ilha = Ilha::da_ilha_magica();
        for i in crate::magica::ilhotas() {
            let crate::magica::Bonus::Coleta(tipo) = i.bonus else {
                continue;
            };
            let mut achados = Vec::new();
            ilha.coletaveis_em(i.centro, i.raio, &mut achados);
            assert!(achados.iter().all(|c| c.tier == tipo),
                "{} contém recurso de outro tipo", i.bonus.nome());
            let n = achados.iter().filter(|c| c.tier == tipo).count();
            println!("{}: {n} nos do tipo {tipo}", i.bonus.nome());
            assert!(
                n > 0,
                "{}: nenhum no' do tipo {tipo} nela ({} coletaveis no total) — \
                 troque `magica::SEMENTE`",
                i.bonus.nome(),
                achados.len()
            );
        }
    }
}

pub fn def_da_zona(zona: &str) -> Option<&'static DefIlha> {
    if crate::celeste::e_celeste(zona) {
        return Some(&crate::celeste::DEF);
    }
    // A ILHA MAGICA entra aqui, e nao no `ARQUIPELAGO`.
    //
    // Nao e' degrau de progressao (a historia, a faixa de nivel das ilhas e o
    // mapa-mundi saem daquela tabela), mas E' uma zona com relevo proprio — e
    // TODO caminho que monta relevo pergunta a esta funcao: o terreno do
    // cliente, o minimapa, as construcoes e o `tick` do servidor. Devolvendo a
    // def aqui, os quatro passam a funcionar sem uma linha nova.
    if crate::magica::e_magica(zona) {
        // UM DEF POR DEGRAU. A `DefIlha` é `'static` no resto do jogo, então
        // os degraus moram numa tabela feita uma vez — construir na hora
        // devolveria referência pra temporário.
        static DEFS: std::sync::OnceLock<Vec<crate::terreno::DefIlha>> =
            std::sync::OnceLock::new();
        let defs = DEFS.get_or_init(|| {
            crate::magica::NIVEIS
                .iter()
                .map(crate::magica::def_do_nivel)
                .collect()
        });
        return defs.iter().find(|d| d.zona == zona);
    }
    // A ARENA (zona da dungeon) também entra aqui, pelo mesmo motivo da
    // mágica: todo caminho que monta relevo pergunta a esta função.
    if crate::arena::e_arena(zona) {
        return Some(&crate::arena::DEF);
    }
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
        assert!(
            andando < 0.0,
            "andando tem que parar antes do patamar, parou em {andando}"
        );
        assert!(
            pulando > 1.0,
            "pulando tem que subir no patamar, parou em {pulando}"
        );
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
        // O passo de 0,4 nao e' mais recusado inteiro: a VARREDURA parte ele
        // em pedacos e o corpo encosta no muro em vez de parar a meia unidade
        // dele. O que este teste guarda e' o invariante, nao a imobilidade: a
        // borda da frente (raio 0,35) nao entra na primeira coluna, que
        // comeca em x = 0.
        assert!(
            p.x + 0.35 <= 0.001,
            "entrou no muro: borda da frente em {}",
            p.x + 0.35
        );
        assert!(p.x >= de.x - 0.001, "andou pra tras: {} < {}", p.x, de.x);
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
        println!(
            "caminhou {andou:.0} de 2666 unidades possiveis ({:.0}% dos passos passaram)",
            andou / 2666.0 * 100.0
        );
        assert!(
            andou > 200.0,
            "andou so' {andou:.0} unidades — travou em algum canto"
        );
    }

    #[test]
    fn nao_anda_sobre_a_agua() {
        let i = Ilha::gerar(5, 64, Bioma::Floresta, ESCALA_ALTURA);
        // bem fora da ilha e' mar aberto: nao anda pra la'
        let de = glam::Vec2::new(0.0, 0.0);
        let longe = i.mover_e_deslizar(de, glam::Vec2::new(9999.0, 0.0), 1.0, 0.35);
        // Com a varredura o passo absurdo anda ate' a BEIRA e para; antes ele
        // era recusado inteiro e o corpo nem saia do lugar. O invariante e'
        // nao pisar na agua, e nao ficar imovel.
        assert!(
            !i.agua(longe.x, longe.y),
            "parou dentro da agua, em {longe:?}"
        );
        assert!(
            longe.x < 9000.0,
            "atravessou o mar inteiro, ate' {}",
            longe.x
        );
    }

    /// O avesso do teste acima, e por isso ele mora colado nele: o CASCO so'
    /// anda na agua, e para na costa em vez de subir a praia.
    ///
    /// As duas regras existem separadas de proposito — a de terra recusa
    /// agua, a do casco recusa terra — e um teste ao lado do outro e' o que
    /// impede alguem de "simplificar" as duas numa com um booleano.
    #[test]
    fn casco_so_anda_na_agua() {
        let i = Ilha::gerar(5, 64, Bioma::Floresta, ESCALA_ALTURA);
        // Bem fora da ilha e' mar aberto: la' o casco anda a vontade.
        let mar = glam::Vec2::new(60.0, 0.0);
        assert!(i.agua(mar.x, mar.y), "o ponto de partida tinha que ser mar");
        let andou = i.mover_no_mar(mar, glam::Vec2::new(0.0, 4.0), 1.0, 1.2);
        assert!(andou.distance(mar) > 1.0, "o casco nao saiu do lugar");
        assert!(i.agua(andou.x, andou.y), "o casco parou em terra: {andou:?}");

        // Rumo ao centro da ilha: para na costa, e nao sobe a praia.
        let na_costa = i.mover_no_mar(mar, glam::Vec2::new(-9999.0, 0.0), 1.0, 1.2);
        assert!(
            i.agua(na_costa.x, na_costa.y),
            "o casco encalhou em terra seca, em {na_costa:?}"
        );
        assert!(
            na_costa.x > -30.0,
            "o casco atravessou a ilha inteira, ate' {}",
            na_costa.x
        );
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
                if i.passo_ok((a.x, a.y), (b.x, b.y)) {
                    passos += 1
                } else {
                    pulos += 1
                }
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
        assert!(
            p.distance(para) < 12.0,
            "parou a {:.0} do destino",
            p.distance(para)
        );
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
        let Some(rota) = i.caminho(de, para, 20_000) else {
            return de;
        };
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
            let degrau = if agora < pulo_ate {
                PULO_BLOCOS
            } else {
                DEGRAU_BLOCOS
            };
            p = i.mover_com_degrau(p, dir * vel, dt, 0.35, degrau);
            agora += dt;
        }
        p
    }

    /// Montado tambem sobe barranco pulando.
    ///
    /// Caso real (17/09/2026): montado, "Ir" ate' a regiao de pedra roxa, o
    /// boneco ficou empurrando um paredao em (44, -39) sem nunca pular. O
    /// servidor perguntava `precisa_pular` com a velocidade A PE' (5), e o
    /// passo montado (7,5) e' outro: a resposta era "nao precisa". A pe' o
    /// mesmo trajeto chega. Aqui a caminhada e' a do servidor: pulo com a
    /// altura de cada instante e rota refeita quando o seguidor trava.
    #[test]
    fn montado_pula_o_paredao_da_trilha_da_pedra_roxa() {
        let d = &ARQUIPELAGO[0];
        let i = Ilha::gerar(d.semente, d.raio_blocos, d.bioma, ESCALA_ALTURA);
        let (de, para) = (
            glam::Vec2::new(43.95196, -39.21062),
            glam::Vec2::new(57.0, 93.0),
        );
        for montado in [false, true] {
            let vel =
                crate::loja::velocidade_de_andar(
                    crate::constants::PLAYER_SPEED,
                    montado.then_some(crate::loja::VEL_MONTADO),
                    1.0,
                );
            let dt = 1.0f32 / 30.0;
            let mut seg = SeguidorDeRota::nova(Vec::new(), para);
            let mut p = de;
            let (mut agora, mut pulo_ate, mut pronto, mut ultima_rota) =
                (0.0f32, -1.0f32, 0.0f32, -1.0f32);
            while agora < 60.0 && p.distance(para) >= 2.5 {
                if (seg.vazia() || seg.travado()) && agora - ultima_rota >= 0.2 {
                    ultima_rota = agora;
                    seg = SeguidorDeRota::nova(i.caminho(p, para, 6_000).unwrap_or_default(), para);
                }
                let dir = seg.direcao(p).unwrap_or(glam::Vec2::ZERO);
                // Como o servidor: a pergunta vai com a velocidade de verdade.
                if dir.length_squared() > 0.01
                    && i.precisa_pular(p, dir.normalize_or_zero() * vel, dt, 0.35)
                    && agora >= pronto
                {
                    pulo_ate = agora + crate::constants::PULO_DURACAO;
                    pronto = pulo_ate + crate::constants::PULO_ESPERA;
                }
                let degrau = if agora < pulo_ate {
                    let t = crate::constants::PULO_DURACAO - (pulo_ate - agora);
                    ((crate::constants::altura_do_pulo(t) / BLOCO).floor() as i32)
                        .clamp(DEGRAU_BLOCOS, PULO_BLOCOS)
                } else {
                    DEGRAU_BLOCOS
                };
                p = i.mover_com_degrau(p, dir * vel, dt, 0.35, degrau);
                agora += dt;
            }
            assert!(
                p.distance(para) < 2.5,
                "montado={montado}: parou em {p:?}, a {:.0} do destino",
                p.distance(para)
            );
        }
    }

    #[test]
    fn dbg_caso() {
        let d = &ARQUIPELAGO[0];
        let i = Ilha::gerar(d.semente, 800, d.bioma, ESCALA_ALTURA);
        let de = glam::Vec2::new(-59.09517, 80.6707);
        let para = glam::Vec2::new(-106.37446, 88.95736);
        let rota = i.caminho(de, para, 20_000).unwrap();
        println!(
            "DBG rota {} pontos: {:?}",
            rota.len(),
            &rota[..rota.len().min(6)]
        );
        let mut seg = SeguidorDeRota::nova(rota.iter().copied(), para);
        let mut p = de;
        let dt = 1.0 / 30.0;
        let (mut pulo_ate, mut pronto, mut agora) = (-1.0f32, 0.0f32, 0.0f32);
        for k in 0..400 {
            let (dt, vel) = (1.0f32 / 30.0, 5.0f32);
            let Some(dir) = seg.direcao(p) else {
                println!("DBG rota acabou em {k}");
                break;
            };
            if seg.travado() {
                println!(
                    "DBG travado no tick {k} em {p:?}, alvo {:?}",
                    seg.pontos.front()
                );
                let bloco_aqui = i.bloco(i.coluna(p.x, p.y).0, i.coluna(p.x, p.y).1);
                let a = p + dir * 0.5;
                let bloco_la = i.bloco(i.coluna(a.x, a.y).0, i.coluna(a.x, a.y).1);
                println!("DBG   bloco aqui {bloco_aqui} / meio bloco a' frente {bloco_la}");
                println!(
                    "DBG   sem_estorvo aqui {} / a' frente {}",
                    i.sem_estorvo(p, 0.35),
                    i.sem_estorvo(a, 0.35)
                );
                println!(
                    "DBG   precisa_pular {}",
                    i.precisa_pular(p, dir * vel, dt, 0.35)
                );
                println!("DBG   dir {dir:?}");
                let est = i.estorvo_em(p + dir * 0.5, 0.35);
                println!("DBG   estorvo a frente {est:?}");
                let passo = i.mover_com_degrau(p, dir * 5.0, 1.0 / 30.0, 0.35, DEGRAU_BLOCOS);
                println!("DBG   mover andou {:.4}", p.distance(passo));
                let pulou = i.mover_com_degrau(p, dir * 5.0, 1.0 / 30.0, 0.35, PULO_BLOCOS);
                println!("DBG   mover pulando andou {:.4}", p.distance(pulou));
                if let Some(e) = est {
                    let fora = (p - e.centro).normalize_or_zero();
                    let t = glam::Vec2::new(-fora.y, fora.x);
                    let lado = if t.dot(dir) >= 0.0 { t } else { -t };
                    let alvo = p + lado * 0.167;
                    println!(
                        "DBG   tangente {lado:?} borda {} estorvo_livre {}",
                        i.borda_livre(p, alvo, 0.35, lado, DEGRAU_BLOCOS),
                        i.sem_estorvo(alvo, 0.35)
                    );
                    let cel = (
                        (e.centro.x / (BLOCO * PASSO_CAMINHO as f32)).round(),
                        (e.centro.y / (BLOCO * PASSO_CAMINHO as f32)).round(),
                    );
                    let cc = glam::Vec2::new(
                        cel.0 * BLOCO * PASSO_CAMINHO as f32,
                        cel.1 * BLOCO * PASSO_CAMINHO as f32,
                    );
                    println!(
                        "DBG   centro celula {cc:?} dist {:.2} raio {:.2}",
                        cc.distance(e.centro),
                        e.raio
                    );
                }
                println!(
                    "DBG   rota nova? {:?}",
                    i.caminho(p, para, 20_000).map(|r| r.len())
                );
                break;
            }
            if i.precisa_pular(p, dir * vel, dt, 0.35) && agora >= pronto {
                pulo_ate = agora + crate::constants::PULO_DURACAO;
                pronto = pulo_ate + crate::constants::PULO_ESPERA;
            }
            let degrau = if agora < pulo_ate {
                PULO_BLOCOS
            } else {
                DEGRAU_BLOCOS
            };
            let antes = p;
            p = i.mover_com_degrau(p, dir * 5.0, dt, 0.35, degrau);
            if k % 20 == 0 {
                println!("DBG t{k} {p:?} andou {:.3}", antes.distance(p));
            }
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
            if i.agua(de.x, de.y) {
                continue;
            }
            let b = a * 3.7 + 1.1;
            let alcance = 12.0 + (k % 11) as f32 * 9.0;
            let para = de + glam::Vec2::new(b.cos() * alcance, b.sin() * alcance);
            if i.agua(para.x, para.y) {
                continue;
            }
            casos += 1;
            let Some(r0) = i.caminho(de, para, 20_000) else {
                sem_rota += 1;
                continue;
            };
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
                        "PAROU a {:.0} (andou {:.0}) | bloco {} -> {} | A* alcanca o alvo? {alcanca} \
                         | tronco encostado? {}",
                        p.distance(para), de.distance(p), i.bloco(cx, cz), i.bloco(tx, tz),
                        i.estorvo_em(p, 0.35 + 0.15).is_some()));
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

    /// Toda ilha do arquipelago tem cidade: plato seco, no nivel, e sem
    /// arvore, planta ou pedra. So' o gerador — a ilha inteira custaria
    /// segundos por ilha, e a cidade e' funcao dele.
    #[test]
    fn o_porto_da_no_oceano() {
        let mut falhas = Vec::new();
        // Skyreach floats over clouds: no sea, no port.
        for d in ARQUIPELAGO.iter().filter(|d| !crate::celeste::e_celeste(d.zona)) {
            let t0 = std::time::Instant::now();
            let ger = Gerador::da_ilha(d);
            let criar = t0.elapsed();
            let p = ger
                .porto()
                .unwrap_or_else(|| panic!("{}: sem porto", d.zona));
            let ponta = p.raiz + p.mar() * p.comp;
            let longe = ger.cidade().map_or(0.0, |c| c.centro().distance(p.centro));
            let mut resumo = Vec::new();
            for extra in [0.0f32, 2.0, 6.0] {
                let (aberto, n) = ger.mar_aberto(ponta + p.mar() * extra);
                resumo.push((extra, aberto, n));
            }
            println!(
                "PORTO {}: centro ({:.0},{:.0}) ponta ({:.0},{:.0}) {longe:.0} u da cidade | mar aberto (ponta+0/2/6): {:?} | Gerador em {criar:?}",
                d.zona, p.centro.x, p.centro.y, ponta.x, ponta.y, resumo
            );
            for (extra, aberto, n) in resumo {
                if !aberto {
                    falhas.push(format!("{}: agua a {extra} u alem da ponta nao e' mar aberto (lago de {n} celulas)", d.zona));
                }
            }
        }
        assert!(falhas.is_empty(), "{falhas:#?}");
    }

    #[test]
    fn toda_ilha_tem_cidade_plana_seca_e_sem_mato() {
        for d in &ARQUIPELAGO {
            let ger = Gerador::da_ilha(d);
            let c = ger
                .cidade()
                .unwrap_or_else(|| panic!("{}: sem cidade", d.zona));
            let r = (c.raio_plato / BLOCO) as i32;
            let (mut total, mut no_nivel) = (0, 0);
            for dz in -r..=r {
                for dx in -r..=r {
                    if dx * dx + dz * dz > r * r {
                        continue;
                    }
                    let (bx, bz) = (c.cx.round() as i32 + dx, c.cz.round() as i32 + dz);
                    let b = ger.bloco_em(bx, bz);
                    assert!(
                        (b + 1) as f32 * BLOCO > 0.35,
                        "{}: agua dentro da cidade",
                        d.zona
                    );
                    total += 1;
                    if b == c.nivel {
                        no_nivel += 1;
                    }
                    assert!(
                        arvore_da_coluna(d.bioma, bx, bz, b, 0, &ger, false).is_none(),
                        "{}: arvore na cidade",
                        d.zona
                    );
                    assert!(
                        planta_da_coluna(d.bioma, bx, bz, b, 0, &ger, false).is_none(),
                        "{}: planta na cidade",
                        d.zona
                    );
                    assert!(
                        minerio_da_coluna(d.bioma, bx, bz, b, &ger, false).is_none(),
                        "{}: pedra na cidade",
                        d.zona
                    );
                }
            }
            println!(
                "{}: cidade em {:?} (enseada {:?}), chao {:.1}, {:.0}% do plato no nivel",
                d.zona,
                c.centro(),
                Forma::de(d.semente, d.raio_blocos).enseada,
                c.altura(),
                no_nivel as f32 / total as f32 * 100.0
            );
            assert!(
                no_nivel * 100 >= total * 95,
                "{}: so' {no_nivel} de {total} colunas no nivel do plato",
                d.zona
            );
        }
    }

    /// Da' pra SAIR da cidade andando: a rampa nao pode virar muralha.
    #[test]
    fn da_pra_sair_da_cidade_andando() {
        for d in &ARQUIPELAGO {
            let ger = Gerador::da_ilha(d);
            let c = ger.cidade().unwrap();
            let mut saidas = 0;
            for k in 0..16 {
                let a = k as f32 / 16.0 * std::f32::consts::TAU;
                let dir = glam::Vec2::new(a.cos(), a.sin());
                let mut anterior = c.nivel;
                let mut livre = true;
                let mut dist = 0.0;
                while dist < c.raio + 2.0 {
                    dist += BLOCO;
                    let p = c.centro() + dir * dist;
                    let b =
                        ger.bloco_em((p.x / BLOCO).round() as i32, (p.y / BLOCO).round() as i32);
                    if (b + 1) as f32 * BLOCO <= 0.0 || (b - anterior).abs() > DEGRAU_BLOCOS {
                        livre = false;
                        break;
                    }
                    anterior = b;
                }
                if livre {
                    saidas += 1;
                }
            }
            println!("{}: {saidas} de 16 direcoes saem andando", d.zona);
            // Last Refuge is a CASTLE (`planalto::muralha`): its wall is the
            // point, and the ways out are its gates — one per road, proved to
            // lead to the port and the trail by
            // `o_ultimo_abrigo_e_um_castelo_com_saida_em_toda_estrada`. The
            // ramp rule this test guards is for the open towns.
            let minimo = if ger.planalto().is_some() { 2 } else { 6 };
            assert!(saidas >= minimo, "{}: so' {saidas} saidas da cidade", d.zona);
        }
    }

    /// Cliente e servidor constroem o gerador cada um pelo seu lado: a cidade
    /// tem que sair igual.
    #[test]
    fn a_cidade_sai_igual_nos_dois_lados() {
        let d = &ARQUIPELAGO[0];
        let a = Gerador::da_ilha(d);
        let b = Gerador::da_ilha(d);
        assert_eq!(a.cidade(), b.cidade());
        let c = a.cidade().unwrap();
        for k in 0..200 {
            let (bx, bz) = (
                c.cx as i32 + (k * 7) % 140 - 70,
                c.cz as i32 + (k * 13) % 140 - 70,
            );
            assert_eq!(a.bloco_em(bx, bz), b.bloco_em(bx, bz));
        }
    }

    /// Troncos em chao plano, com folga pra montar os casos em volta.
    fn troncos_no_plano(i: &Ilha, quantos: usize) -> Vec<Estorvo> {
        i.todos_os_estorvos()
            .iter()
            .copied()
            .filter(|e| matches!(e.tipo, TipoDeEstorvo::Tronco))
            .filter(|e| {
                let h = i.altura(e.centro.x, e.centro.y);
                (0..16).all(|k| {
                    let a = k as f32 / 16.0 * std::f32::consts::TAU;
                    let p = e.centro + glam::Vec2::new(a.cos(), a.sin()) * 4.0;
                    !i.agua(p.x, p.y) && (i.altura(p.x, p.y) - h).abs() < 0.01
                })
            })
            .take(quantos)
            .collect()
    }

    /// Corpo que COMECA dentro de um tronco tem que conseguir sair.
    ///
    /// E' o mob que nasceu num slot de spawn escolhido sem olhar estorvo, ou
    /// que a separacao entre corpos empurrou pra dentro da arvore. Antes, todo
    /// passo terminava encostado no tronco, `sem_estorvo` reprovava todos — o
    /// de sair inclusive — e o bicho ficava plantado ali pra sempre.
    #[test]
    fn corpo_dentro_do_tronco_consegue_sair() {
        let d = &ARQUIPELAGO[0];
        let i = Ilha::gerar(d.semente, 800, d.bioma, ESCALA_ALTURA);
        let troncos = troncos_no_plano(&i, 200);
        assert!(troncos.len() > 50, "so' {} troncos no plano", troncos.len());
        let (dt, vel, raio) = (1.0 / 30.0, 3.0, 0.35);
        let (mut casos, mut presos) = (0, 0);
        for (n, e) in troncos.iter().enumerate() {
            // Metade da sobreposicao, e o bicho quer ir pra um lado qualquer —
            // inclusive ATRAVES do tronco.
            let a = n as f32 * 2.399_963;
            let fora = glam::Vec2::new(a.cos(), a.sin());
            let inicio = e.centro + fora * (e.raio + raio) * 0.5;
            for quer in [fora, -fora, glam::Vec2::new(-fora.y, fora.x)] {
                casos += 1;
                let mut p = inicio;
                for _ in 0..60 {
                    p = i.mover_e_deslizar(p, quer * vel, dt, raio);
                }
                // Preso e' quem continua DENTRO do tronco de partida. Quem saiu
                // e parou encostado no vizinho porque insiste num rumo
                // barrado ja' saiu — "andou pouco" nao e' "ficou preso".
                if p.distance(inicio) < 0.3 && e.centro.distance(p) < e.raio + raio - 0.02 {
                    presos += 1;
                    let perto = i
                        .todos_os_estorvos()
                        .iter()
                        .filter(|o| o.centro.distance(inicio) < o.raio + raio + 1.0)
                        .count();
                    let (ax, az) = i.coluna(inicio.x, inicio.y);
                    let degraus: Vec<i32> = [(1, 0), (-1, 0), (0, 1), (0, -1)]
                        .iter()
                        .map(|(dx, dz)| i.bloco(ax + dx, az + dz) - i.bloco(ax, az))
                        .collect();
                    println!(
                        "  PRESO em {inicio:?} quer {quer:?} | tronco raio {:.2} | estorvos em 1: {perto} \
                         | parede? {} | degraus vizinhos {degraus:?} | andou {:.3}",
                        e.raio, i.caixa_toca(inicio, raio), p.distance(inicio));
                }
            }
        }
        println!("{casos} casos: {presos} presos dentro do tronco");
        assert_eq!(presos, 0, "{presos} de {casos} corpos nao sairam do tronco");
    }

    /// Mob perseguindo em LINHA RETA com um tronco no meio tem que contornar.
    ///
    /// O mob nao usa A*: ele vai direto no jogador e confia no deslize. Com
    /// dois troncos vizinhos ou um tronco colado num degrau, o deslize pela
    /// tangente pode nao ter pra onde ir.
    #[test]
    fn perseguicao_em_linha_reta_contorna_troncos() {
        let d = &ARQUIPELAGO[0];
        let i = Ilha::gerar(d.semente, 800, d.bioma, ESCALA_ALTURA);
        let troncos = troncos_no_plano(&i, 300);
        let (dt, vel, raio) = (1.0 / 30.0, 3.0, 0.35);
        let (mut casos, mut presos) = (0, 0);
        for (n, e) in troncos.iter().enumerate() {
            let a = n as f32 * 2.399_963;
            let eixo = glam::Vec2::new(a.cos(), a.sin());
            // Um tiquinho fora do eixo, pra nao cair no caso degenerado de
            // mirar exatamente o centro.
            let torto = glam::Vec2::new(-eixo.y, eixo.x) * 0.02;
            let de = e.centro - eixo * 3.0 + torto;
            let para = e.centro + eixo * 3.0 + torto;
            if !i.sem_estorvo(de, raio) || !i.sem_estorvo(para, raio) {
                continue;
            }
            casos += 1;
            // O mesmo condutor do servidor: reto, e A* quando empaca.
            let mut perseguicao = Perseguicao::default();
            let mut p = de;
            let mut mexeu_no_fim = 0.0;
            for k in 0..150 {
                if p.distance(para) < 0.5 {
                    break;
                }
                let dir = perseguicao.direcao(&i, p, para, vel * dt, k as f32 * dt);
                let antes = p;
                p = i.mover_e_deslizar(p, dir * vel, dt, raio);
                if k >= 120 {
                    mexeu_no_fim += antes.distance(p);
                }
            }
            if p.distance(para) > 1.0 {
                presos += 1;
                let perto = i
                    .todos_os_estorvos()
                    .iter()
                    .filter(|o| o.centro.distance(p) < o.raio + raio + 1.5)
                    .count();
                let frente = p + (para - p).normalize_or_zero() * 0.5;
                let (ax, az) = i.coluna(p.x, p.y);
                let (bx, bz) = i.coluna(frente.x, frente.y);
                println!(
                    "  PRESO a {:.2} do alvo | estorvos em 1,5: {perto} | subida a' frente {} \
                     | mexeu nos ultimos 30 ticks {mexeu_no_fim:.2} | raio do tronco {:.2}",
                    p.distance(para),
                    i.bloco(bx, bz) - i.bloco(ax, az),
                    e.raio
                );
            }
        }
        println!("{casos} casos: {presos} nao passaram do tronco");
        assert!(casos > 100, "so' {casos} casos");
        assert!(
            presos * 50 < casos,
            "{presos} de {casos} perseguicoes travaram no tronco (teto: 2%)"
        );
    }

    /// Ir pela ROTA ate' um ponto dentro de um tronco — o bicho colado na
    /// arvore — tem que chegar ao pe' dela, e nao empurrar o tronco.
    #[test]
    fn rota_ate_bicho_colado_no_tronco_chega() {
        let d = &ARQUIPELAGO[0];
        let i = Ilha::gerar(d.semente, 800, d.bioma, ESCALA_ALTURA);
        let troncos = troncos_no_plano(&i, 150);
        let (mut casos, mut presos) = (0, 0);
        for (n, e) in troncos.iter().enumerate() {
            let a = n as f32 * 2.399_963;
            let de = i.terra_mais_proxima(
                e.centro.x + a.cos() * 14.0,
                e.centro.y + a.sin() * 14.0,
                6.0,
            );
            if i.agua(de.x, de.y) {
                continue;
            }
            let para = e.centro + glam::Vec2::new(a.cos(), a.sin()) * e.raio * 0.5;
            // Partida noutro patamar (so' o anel de 4 em volta do tronco e'
            // plano): o A* nao alcanca, e rota parcial nao e' o que se mede.
            let livre = i.ponto_livre_perto(para, 0.35);
            let Some(r0) = i.caminho(de, para, 20_000) else {
                continue;
            };
            if r0.last().unwrap().distance(livre) > 1.0 {
                continue;
            }
            casos += 1;
            let p = simula_ida(&i, de, para);
            if p.distance(e.centro) > e.raio + 0.35 + 1.5 {
                presos += 1;
                println!(
                    "  PAROU a {:.2} do tronco (raio {:.2}) | estorvos em 1,5: {} | ponto livre a {:.2}",
                    p.distance(e.centro), e.raio,
                    i.todos_os_estorvos().iter().filter(|o| o.centro.distance(p) < o.raio + 1.85).count(),
                    livre.distance(e.centro));
            }
        }
        println!("{casos} casos: {presos} nao chegaram ao pe' do tronco");
        assert!(casos > 50, "so' {casos} casos");
        assert!(
            presos * 50 < casos,
            "{presos} de {casos} rotas nao chegaram ao pe' do tronco (teto: 2%)"
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
        let (d, f, m) = (
            pico(Bioma::Deserto),
            pico(Bioma::Floresta),
            pico(Bioma::Montanha),
        );
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
            assert!(
                e.sitio_chefe > 0.01,
                "{}: sitio de chefe {:.3}",
                d.zona,
                e.sitio_chefe
            );
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
        // A ilha REAL, e nao a de raio 800: o gerador refaz o relevo pelo
        // tamanho, entao a de 800 e' outra ilha — deu azul acima de verde
        // enquanto a do jogo da' a ladder certa.
        let i = Ilha::gerar(d.semente, d.raio_blocos, d.bioma, ESCALA_ALTURA);
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
                    if achados.is_empty() {
                        secos += 1
                    }
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
            intervalo(melhor_veio),
            intervalo(melhor_mata),
        );
        assert!(melhor_veio > 1, "nenhum veio com mais de uma pedra");
        assert!(melhor_mata > 1, "nenhuma mata com mais de um tronco");
        // Lugar seco tem que existir: se der pra coletar em qualquer lugar, o
        // spot nao vale nada e andar ate' ele nao significa nada.
        assert!(
            secos * 10 > n,
            "so' {secos} de {n} amostras sem recurso — o mapa inteiro e' spot"
        );
    }

    #[test]
    fn a_ilha_tem_minerio_dos_quatro_tiers() {
        let d = &ARQUIPELAGO[0];
        // A ilha REAL, e nao a de raio 800: o gerador refaz o relevo pelo
        // tamanho, entao a de 800 e' outra ilha — deu azul acima de verde
        // enquanto a do jogo da' a ladder certa.
        let i = Ilha::gerar(d.semente, d.raio_blocos, d.bioma, ESCALA_ALTURA);
        let mut por_tier = [0u32; 5];
        let mut troncos = 0u32;
        let mut energias = 0u32;
        let mut pedras = Vec::new();
        let mut cristais = Vec::new();
        for e in i.todos_os_estorvos() {
            match e.tipo {
                TipoDeEstorvo::Minerio(t) => {
                    por_tier[t as usize] += 1;
                    pedras.push(e);
                }
                TipoDeEstorvo::Energia => {
                    energias += 1;
                    cristais.push(e);
                }
                TipoDeEstorvo::Tronco => troncos += 1,
                _ => {}
            }
        }
        println!(
            "troncos {troncos} | energias {energias} | pedras: cinza {} verde {} azul {} roxo {}",
            por_tier[1], por_tier[2], por_tier[3], por_tier[4]
        );
        for (t, nome) in [(1, "cinza"), (2, "verde"), (3, "azul"), (4, "roxo")] {
            assert!(por_tier[t] > 0, "ilha sem pedra {nome}");
        }
        assert!(energias > 0, "ilha sem cristal de Energia");
        assert!(
            cristais.iter().all(|a| pedras.iter().all(|b| {
                a.coluna != b.coluna
                    && a.centro.distance(b.centro) >= a.raio + b.raio
            })),
            "cristal de Energia cobre ou compartilha coluna com minerio"
        );
        // A ladder tem que DESCER: pedra melhor tem que ser mais rara, senao
        // subir a montanha nao e' progressao, e' passeio.
        assert!(
            por_tier[1] > por_tier[2] && por_tier[2] > por_tier[3] && por_tier[3] > por_tier[4],
            "ladder de raridade invertida: {:?}",
            &por_tier[1..]
        );
    }
}

impl Ilha {
    /// Um ponto enxerga o outro por cima do relevo?
    ///
    /// A linha vai do olho de um ao olho do outro; se o chao passa por cima
    /// dela em algum ponto, a montanha esta' no meio. Nao olha arvore nem
    /// pedra: vista barrada por tronco fino so' faria o bicho perder o
    /// jogador atras de qualquer galho.
    pub fn visada(&self, a: glam::Vec2, b: glam::Vec2) -> bool {
        const OLHO: f32 = 1.2;
        let d = a.distance(b);
        if d < 2.0 {
            return true;
        }
        let ha = self.altura(a.x, a.y) + OLHO;
        let hb = self.altura(b.x, b.y) + OLHO;
        let passos = (d * 2.0).ceil() as i32;
        (1..passos).all(|k| {
            let t = k as f32 / passos as f32;
            let p = a.lerp(b, t);
            self.altura(p.x, p.y) <= ha + (hb - ha) * t
        })
    }

    /// Visada de TIRO: ataque a distancia com alvo (`visada_de_tiro_com`).
    pub fn visada_de_tiro(&self, a: glam::Vec2, b: glam::Vec2) -> bool {
        visada_de_tiro_com(|x, z| self.altura(x, z), a, b)
    }
}

/// Um tiro de `a` (atirador) chega em `b` (alvo) por cima do relevo?
///
/// Flecha e bala saem em ARCO, nao em linha reta de olho a olho: a borda de
/// um barranco entre o atirador no alto e o bicho embaixo corta a linha reta
/// (e o dano sumia sem aviso), mas nao a parabola. O arco sobe
/// `max(1,5; 0,3·d)` no meio do caminho e o olho do atirador fica um pouco
/// mais alto. Morro de verdade no meio continua barrando.
pub fn visada_de_tiro_com(altura: impl Fn(f32, f32) -> f32, a: glam::Vec2, b: glam::Vec2) -> bool {
    const OLHO_ATIRADOR: f32 = 1.6;
    const OLHO_ALVO: f32 = 1.2;
    let d = a.distance(b);
    if d < 2.0 {
        return true;
    }
    let ha = altura(a.x, a.y) + OLHO_ATIRADOR;
    let hb = altura(b.x, b.y) + OLHO_ALVO;
    let apex = (0.3 * d).max(1.5);
    let passos = (d * 2.0).ceil() as i32;
    (1..passos).all(|k| {
        let t = k as f32 / passos as f32;
        let p = a.lerp(b, t);
        altura(p.x, p.y) <= ha + (hb - ha) * t + apex * 4.0 * t * (1.0 - t)
    })
}

#[cfg(test)]
mod testes_visada_de_tiro {
    use super::visada_de_tiro_com;
    use glam::Vec2;

    /// Visada reta de olho a olho (1,2), a mesma conta de `Ilha::visada`.
    fn reta(altura: impl Fn(f32, f32) -> f32, a: Vec2, b: Vec2) -> bool {
        let (ha, hb) = (altura(a.x, a.y) + 1.2, altura(b.x, b.y) + 1.2);
        let passos = (a.distance(b) * 2.0).ceil() as i32;
        (1..passos).all(|k| {
            let t = k as f32 / passos as f32;
            let p = a.lerp(b, t);
            altura(p.x, p.y) <= ha + (hb - ha) * t
        })
    }

    #[test]
    fn plano_passa() {
        assert!(visada_de_tiro_com(
            |_, _| 3.0,
            Vec2::ZERO,
            Vec2::new(9.0, 0.0)
        ));
    }

    #[test]
    fn borda_entre_o_alto_e_o_baixo_passa() {
        // Planalto de 4 u ate' x=3, chao a 0 depois: atirador 3 u atras da
        // borda, bicho 5 u pra fora dela, embaixo.
        let chao = |x: f32, _: f32| if x <= 3.0 { 4.0 } else { 0.0 };
        let (a, b) = (Vec2::ZERO, Vec2::new(8.0, 0.0));
        assert!(!reta(chao, a, b), "o caso do bug: a linha reta barra");
        assert!(visada_de_tiro_com(chao, a, b));
        // E de baixo pra cima tambem (bicho no alto da borda).
        let alto = |x: f32, _: f32| if x >= 5.0 { 4.0 } else { 0.0 };
        assert!(visada_de_tiro_com(alto, Vec2::ZERO, Vec2::new(8.0, 0.0)));
    }

    /// Na ilha gerada de verdade: o tiro ve' tudo que a visada reta via, e
    /// libera pares que a reta barrava (os barrancos do bug).
    #[test]
    fn ilha_real_arco_libera_barranco_sem_perder_o_que_a_reta_via() {
        use super::{Bioma, Ilha, BLOCO, ESCALA_ALTURA};
        let ilha = Ilha::gerar(1234, 64, Bioma::Floresta, ESCALA_ALTURA);
        let r = 64.0 * BLOCO * 0.8;
        let (mut reta_ve, mut libera, mut barra) = (0u32, 0u32, 0u32);
        let mut y = -r;
        while y < r {
            let mut x = -r;
            while x < r {
                let a = Vec2::new(x, y);
                for k in 0..8 {
                    let ang = k as f32 * std::f32::consts::FRAC_PI_4;
                    let b = a + Vec2::new(ang.cos(), ang.sin()) * 8.0;
                    let (ve_reta, ve_tiro) = (ilha.visada(a, b), ilha.visada_de_tiro(a, b));
                    assert!(
                        !ve_reta || ve_tiro,
                        "tiro barrou o que a reta via: {a:?} -> {b:?}"
                    );
                    if ve_reta {
                        reta_ve += 1
                    } else if ve_tiro {
                        libera += 1
                    } else {
                        barra += 1
                    }
                }
                x += 3.0;
            }
            y += 3.0;
        }
        println!("pares a 8 u: reta ve {reta_ve}, arco libera {libera}, continua barrado {barra}");
        assert!(libera > 0, "nenhum barranco liberado numa ilha inteira?");
    }

    #[test]
    fn morro_alto_no_meio_barra() {
        let morro = |x: f32, _: f32| if (4.0..=6.0).contains(&x) { 8.0 } else { 0.0 };
        assert!(!visada_de_tiro_com(morro, Vec2::ZERO, Vec2::new(10.0, 0.0)));
    }
}

#[cfg(test)]
mod testes_do_cais {
    use super::*;

    /// A rota tem que ALCANCAR o porto de toda ilha.
    ///
    /// O A* funcionava; o que nao funcionava era o limite antes dele
    /// (`world::handle_mover_para: ROTA_ALCANCE`, 220 u). O porto do Bosque
    /// fica a 785 u da cidade e o do Planalto a 1.109: tocar na cidade
    /// estando no cais era recusado sem uma palavra, e o dono relatou duas
    /// vezes que "o A* do porto nao funciona".
    ///
    /// Este teste mede a maior travessia que o jogo pede. Se uma ilha nova
    /// nascer mais larga que o limite, ele reprova aqui — e nao no cais, com
    /// o jogador tocando na tela e nada acontecendo.
    /// De DENTRO de uma casa, existe caminho pra fora.
    ///
    /// Guarda de REGRESSÃO, não prova de conserto: hoje ele passa com e sem a
    /// rede de `saida_do_predio` (conferido removendo-a). O travamento que o
    /// dono relatou — "o A* não sabe sair disso nem contornar as casas" — não
    /// se reproduz assim, e enquanto não houver um caso concreto o que este
    /// teste garante é só que sair de dentro de um prédio continua possível.
    #[test]
    fn de_dentro_da_casa_da_pra_sair() {
        // Uma ilha só: montá-la é caro, e o defeito não é de uma em especial.
        for def in ARQUIPELAGO.iter().take(1) {
            let ilha = Ilha::da_ilha(def);
            let vila = ilha.vila();
            let centro = ilha.cidade().map(|c| c.centro()).unwrap_or_default();
            let mut testadas = 0;
            for pd in vila.predios.iter().take(6) {
                // O MIOLO DA CAIXA DE COLISÃO, e não `pd.pos` — aquilo é a
                // âncora do prédio e cai fora da parede.
                //
                // E sem filtrar por `ocupado`: a caixa sólida é a PAREDE, não
                // o miolo, então de pé dentro da casa o jogador não está
                // dentro de nada sólido. Uma versão anterior filtrava por
                // `ocupado`, não achava prédio nenhum, e passava sem testar
                // coisa alguma.
                let Some((mn, mx)) = pd
                    .construcao()
                    .caixas_mundo(pd.pos, pd.yaw_q)
                    .into_iter()
                    .max_by(|a, b| {
                        ((a.1.x - a.0.x) * (a.1.z - a.0.z))
                            .total_cmp(&((b.1.x - b.0.x) * (b.1.z - b.0.z)))
                    })
                else {
                    continue;
                };
                let dentro = glam::Vec2::new((mn.x + mx.x) * 0.5, (mn.z + mx.z) * 0.5);
                testadas += 1;
                let rota = ilha.caminho(dentro, centro, 6_000);
                assert!(
                    rota.is_some(),
                    "{}: preso dentro de {:?} em {:.0},{:.0}",
                    def.zona,
                    pd.papel,
                    dentro.x,
                    dentro.y
                );
                assert!(
                    !rota.unwrap().is_empty(),
                    "rota vazia saindo de {:?}",
                    pd.papel
                );
            }
            assert!(testadas > 0, "{}: nenhum prédio pra testar", def.zona);
        }
    }

    #[test]
    fn a_rota_alcanca_o_porto_de_toda_ilha() {
        /// Tem que bater com `world::handle_mover_para: ROTA_ALCANCE`.
        const ALCANCE: f32 = 1_200.0;
        for d in ARQUIPELAGO.iter() {
            let g = Gerador::da_ilha(d);
            let (Some(c), Some(p)) = (
                g.cidade().map(|x| x.centro()),
                g.vila().porto.map(|x| x.centro),
            ) else {
                continue;
            };
            let dist = c.distance(p);
            assert!(
                dist <= ALCANCE,
                "{}: porto a {dist:.0}u da cidade, e a rota so' vai a {ALCANCE:.0}u —                  o jogador toca no mapa e nada acontece",
                d.zona
            );
        }
    }

    /// Do CAIS tem que sair rota.
    ///
    /// O cais e' uma lingua estreita de pedra entrando no mar, e a grade do
    /// A* tem celula de `PASSO_CAMINHO` blocos — 4 unidades. Na ponta do
    /// cais, a vizinhanca inteira da celula cai na AGUA, e o A* sai de um no'
    /// sem vizinho nenhum: o jogador toca no mapa e nao acontece nada. O dono
    /// relatou isso em 21/09/2026 ("A* nunca funciona qnd eu to na beirada do
    /// porto").
    ///
    /// Percorre o cais inteiro, da raiz a' ponta, porque o defeito e' de
    /// grau: perto da costa a vizinhanca ainda pega terra e funciona, e so'
    /// falha quando o mar cerca.
    #[test]
    fn do_cais_inteiro_sai_rota() {
        let d = &ARQUIPELAGO[0];
        let ilha = Ilha::da_ilha(d);
        let vila = Gerador::da_ilha(d).vila().clone();
        let porto = vila.porto.expect("a ilha inicial tem porto");
        let cidade = Gerador::da_ilha(d)
            .cidade()
            .map(|c| c.centro())
            .unwrap_or(glam::Vec2::ZERO);
        let mut falhas = Vec::new();
        for k in 0..=10 {
            let t = k as f32 / 10.0;
            let p = porto.raiz.lerp(porto.ponta, t);
            // So' de onde da' pra estar de pe'.
            if ilha.agua(p.x, p.y) {
                continue;
            }
            match ilha.caminho(p, cidade, 4000) {
                Some(r) if !r.is_empty() => {}
                _ => falhas.push((t, p)),
            }
        }
        assert!(
            falhas.is_empty(),
            "sem rota do cais pra cidade em {} de 11 pontos: {:?}",
            falhas.len(),
            falhas
        );
    }
}

#[cfg(test)]
mod testes_do_alcance_dos_npcs {
    use super::*;

    /// TODO NPC DE VILA TEM QUE SER ALCANÇÁVEL PELO AUTO-PATH.
    ///
    /// Este teste nasceu de uma medição, não de uma suspeita. Varrendo os
    /// NPCs das ilhas, a rota da praça terminava a **117 unidades** do
    /// Capitão do Porto da ilha inicial e a **533** do Capitão do Planalto —
    /// e o dono já havia relatado duas vezes "não consigo andar na prancha do
    /// porto com o A*".
    ///
    /// Eram DUAS causas somadas:
    ///
    /// 1. a prancha tem 2,5 unidades de largura e a célula do A* tem 4, então
    ///    os pontos de célula do cais caíam na água (ver `SitioPorto::no_deck`);
    /// 2. a heurística octil subestimava tanto o custo real que o A* estourava
    ///    o orçamento antes de chegar (ver o peso ×1,3 em `caminho`).
    ///
    /// O limite é `INTERACT_RADIUS`: chegar mais longe que isso é chegar e não
    /// poder falar, que da tela é indistinguível de estar quebrado.
    #[test]
    fn todo_npc_de_vila_esta_ao_alcance_do_auto_path() {
        const ORCAMENTO: usize = 6_000; // o mesmo de `handle_mover_para`
        // A Geleira tem UM caso conhecido a 3,09 u — 0,09 além do limite, por
        // degrau de terreno ao lado do Alfaiate. Está registrado aqui em vez
        // de afrouxar o limite: afrouxar esconderia os outros dez.
        const TOLERADOS: &[(&str, &str)] = &[("ilha_gelo", "Tailor")];
        let mut falhas: Vec<String> = Vec::new();
        for zona in ["ilha_inicial", "ilha_gelo", "ilha_planalto", "ilha_bosque"] {
            let Some(def) = def_da_zona(zona) else { continue };
            let ilha = Ilha::da_ilha(def);
            let Some(cid) = ilha.cidade() else { continue };
            let c = cid.centro();
            let praca = ilha.terra_mais_proxima(c.x, c.y, 400.0);
            for n in &ilha.vila().npcs {
                if TOLERADOS.iter().any(|(z, nm)| *z == zona && n.nome.contains(nm)) {
                    continue;
                }
                let d = match ilha.caminho(praca, n.pos, ORCAMENTO) {
                    Some(r) => r.last().unwrap().distance(n.pos),
                    None => f32::MAX,
                };
                if d > crate::constants::INTERACT_RADIUS {
                    falhas.push(format!("{zona} · {} a {d:.2}u", n.nome));
                }
            }
        }
        assert!(
            falhas.is_empty(),
            "NPCs que o auto-path não alcança: {falhas:#?}"
        );
    }

    /// O CAIS É ANDÁVEL DE PONTA A PONTA.
    ///
    /// O NPC do porto poderia ficar alcançável por acaso (um ponto de célula
    /// que calhou de cair no deck). Este mede a prancha inteira, que é o que
    /// o jogador percorre.
    #[test]
    fn da_pra_andar_a_prancha_do_porto_inteira() {
        for zona in ["ilha_inicial", "ilha_planalto"] {
            let Some(def) = def_da_zona(zona) else { continue };
            let ilha = Ilha::da_ilha(def);
            let Some(p) = ilha.porto() else { continue };
            let Some(cid) = ilha.cidade() else { continue };
            let c = cid.centro();
            let praca = ilha.terra_mais_proxima(c.x, c.y, 400.0);
            let mar = p.mar();
            let mut t = -p.recuo;
            while t <= p.comp {
                let q = p.raiz + mar * t;
                let d = match ilha.caminho(praca, q, 6_000) {
                    Some(r) => r.last().unwrap().distance(q),
                    None => f32::MAX,
                };
                assert!(
                    d <= 4.0,
                    "{zona}: a rota até o cais em t={t:.1} ({:.0},{:.0}) para a {d:.1}u",
                    q.x,
                    q.y
                );
                t += 2.0;
            }
        }
    }
}

#[cfg(test)]
mod testes_do_desvio {
    use super::*;

    /// O DESVIO MUDA O CAMINHO — e sem ele, nada muda.
    ///
    /// O dono: "caso ele ficar preso por 5 segundos tentando seguir uma rota,
    /// ele tenta mudar de rota automaticamente para contornar, mesmo sendo um
    /// caminho mais longo".
    ///
    /// A metade importante é a segunda asserção: ANTES deste conserto, o
    /// servidor já refazia a rota ao travar, e o A* devolvia exatamente a
    /// mesma. Refazer sem mudar nada não é contornar.
    #[test]
    fn evitar_um_ponto_da_outra_rota_e_mais_longa() {
        let def = def_da_zona("ilha_inicial").expect("zona");
        let ilha = Ilha::da_ilha(def);
        let c = ilha.cidade().expect("cidade").centro();
        let praca = ilha.terra_mais_proxima(c.x, c.y, 400.0);
        let alvo = praca + glam::Vec2::new(60.0, 40.0);
        let alvo = ilha.terra_mais_proxima(alvo.x, alvo.y, 60.0);

        let reta = ilha.caminho(praca, alvo, 6_000).expect("rota normal");
        assert!(reta.len() > 3, "a rota de teste é curta demais pra medir desvio");

        // Bloqueia o MEIO da rota original.
        let meio = reta[reta.len() / 2];
        let desviada = ilha
            .caminho_evitando(praca, alvo, 6_000, &[meio])
            .expect("com desvio ainda há caminho");

        let passa_perto = |r: &[glam::Vec2]| r.iter().any(|p| p.distance(meio) < 2.0);
        assert!(passa_perto(&reta), "a rota original não passa pelo ponto medido");
        assert!(
            !passa_perto(&desviada) || comprimento(&desviada) > comprimento(&reta) + 1.0,
            "o desvio devolveu o mesmo caminho: contornar não aconteceu"
        );
    }

    /// SEM PONTOS A EVITAR, NADA MUDA.
    ///
    /// `caminho` delega pra `caminho_evitando` com lista vazia. Se o pedágio
    /// vazasse, toda rota do jogo mudaria — e este é o tipo de regressão que
    /// não aparece em tela, só em "por que ele foi por ali?".
    #[test]
    fn sem_desvio_o_caminho_e_o_mesmo_de_sempre() {
        let def = def_da_zona("ilha_inicial").expect("zona");
        let ilha = Ilha::da_ilha(def);
        let c = ilha.cidade().expect("cidade").centro();
        let praca = ilha.terra_mais_proxima(c.x, c.y, 400.0);
        for (dx, dz) in [(80.0, 0.0), (0.0, 80.0), (-60.0, 60.0)] {
            let alvo = ilha.terra_mais_proxima(praca.x + dx, praca.y + dz, 80.0);
            let a = ilha.caminho(praca, alvo, 6_000);
            let b = ilha.caminho_evitando(praca, alvo, 6_000, &[]);
            assert_eq!(
                a.as_ref().map(|r| r.len()),
                b.as_ref().map(|r| r.len()),
                "a lista vazia mudou o caminho"
            );
        }
    }

    /// O DESVIO É PEDÁGIO, NÃO PAREDE.
    ///
    /// Se o ponto emperrado fosse proibido, um corredor estreito ficaria sem
    /// caminho nenhum — e o jogador pararia de ir a lugar nenhum, que é pior
    /// que ir pelo caminho longo. Mesmo evitando o destino, tem que haver rota.
    #[test]
    fn evitar_nunca_deixa_o_jogador_sem_caminho() {
        let def = def_da_zona("ilha_inicial").expect("zona");
        let ilha = Ilha::da_ilha(def);
        let c = ilha.cidade().expect("cidade").centro();
        let praca = ilha.terra_mais_proxima(c.x, c.y, 400.0);
        let alvo = ilha.terra_mais_proxima(praca.x + 50.0, praca.y + 20.0, 60.0);
        // Seis pontos em volta do próprio alvo: o pior caso.
        let volta: Vec<glam::Vec2> = (0..6)
            .map(|i| {
                let a = i as f32 / 6.0 * std::f32::consts::TAU;
                alvo + glam::Vec2::new(a.cos(), a.sin()) * 3.0
            })
            .collect();
        // `is_some()` NÃO BASTA: o `caminho` devolve o melhor esforço mesmo
        // sem alcançar o alvo, então um desvio que virasse parede passaria
        // por este teste — e passou, quando eu mutei pra conferir. O que
        // prova é a rota TERMINAR no destino.
        let r = ilha
            .caminho_evitando(praca, alvo, 6_000, &volta)
            .expect("sem rota nenhuma");
        let fim = *r.last().expect("rota vazia");
        assert!(
            fim.distance(alvo) <= crate::constants::INTERACT_RADIUS,
            "o desvio virou parede: a rota parou a {:.1}u do destino",
            fim.distance(alvo)
        );
    }

    fn comprimento(r: &[glam::Vec2]) -> f32 {
        r.windows(2).map(|w| w[0].distance(w[1])).sum()
    }
}

#[cfg(test)]
mod testes_ilhas_aereas {
    use super::*;
    #[test]
    fn cache_antigo_nao_cria_chao_no_vazio() {
        let dir = std::env::temp_dir().join(format!("tempest-cache-magica-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let def = crate::magica::def_do_nivel(&crate::magica::NIVEIS[0]);
        let mut velha = Ilha::da_ilha(&def);
        let p = glam::Vec2::new(70.0, 40.0);
        assert!(velha.agua(p.x, p.y));
        let (x, z) = velha.coluna(p.x, p.y);
        velha.blocos[z as usize * velha.lado + x as usize] = crate::magica::NIVEL_CHAO as i16;
        let arquivo = dir.join(format!("{}-{}.alt", def.semente, def.raio_blocos));
        velha.salvar(arquivo.to_str().unwrap()).unwrap();
        let atual = Ilha::carregar_ou_gerar_da_ilha(dir.to_str().unwrap(), &def);
        assert!(atual.agua(p.x, p.y), "cache obsoleto virou chão no vazio");
        assert!(
            !atual.cabe(glam::Vec2::ZERO, p, 0.35),
            "empurrão permitiu entrar no vazio"
        );
        std::fs::remove_file(arquivo).unwrap();
        std::fs::remove_dir(dir).unwrap();
    }
    #[test]
    fn ponte_barra_saida_lateral_andando_pulando_e_correndo() {
        let ilha = Ilha::da_ilha_magica();
        for degrau in [DEGRAU_BLOCOS, PULO_BLOCOS] {
            for velocidade in [5.0, 30.0] {
                let mut p = glam::Vec2::new(80.0, 0.0);
                for _ in 0..150 {
                    p = ilha.mover_com_degrau(
                        p,
                        glam::Vec2::new(0.0, velocidade),
                        1.0 / 30.0,
                        0.35,
                        degrau,
                    );
                    assert!(!ilha.agua(p.x, p.y));
                }
                assert!(p.y <= crate::magica::MEIA_PONTE + 0.25);
            }
        }
    }
    #[test]
    fn combate_sem_pedras_mantem_vegetacao_e_mina() {
        let g = Gerador::da_ilha_magica();
        for i in crate::magica::ilhotas() {
            if !matches!(
                i.bonus,
                crate::magica::Bonus::Xp
                    | crate::magica::Bonus::Ouro
                    | crate::magica::Bonus::DropDeMob
                    | crate::magica::Bonus::Coleta(4)
            ) {
                continue;
            }
            let mut pedras = 0;
            let mut plantas = 0;
            for bz in ((i.centro.y - i.raio - 5.0) / BLOCO) as i32
                ..=((i.centro.y + i.raio + 5.0) / BLOCO) as i32
            {
                for bx in ((i.centro.x - i.raio - 5.0) / BLOCO) as i32
                    ..=((i.centro.x + i.raio + 5.0) / BLOCO) as i32
                {
                    let p = glam::Vec2::new(bx as f32 * BLOCO, bz as f32 * BLOCO);
                    if crate::magica::bonus_em(p) != Some(i.bonus) {
                        continue;
                    }
                    let h = g.bloco_em(bx, bz);
                    let agua = (h + 1) as f32 * BLOCO <= NIVEL_DO_MAR;
                    pedras +=
                        minerio_da_coluna(Bioma::Floresta, bx, bz, h, &g, agua).is_some() as usize;
                    let declive = [(1, 0), (-1, 0), (0, 1), (0, -1)]
                        .into_iter()
                        .map(|(x, z)| (h - g.bloco_em(bx + x, bz + z)).abs())
                        .max()
                        .unwrap();
                    if let Some(pl) =
                        planta_da_coluna(Bioma::Floresta, bx, bz, h, declive, &g, agua)
                    {
                        if matches!(pl.especie, Planta::Pedra) {
                            pedras += 1;
                        } else {
                            plantas += 1;
                        }
                    }
                }
            }
            if matches!(i.bonus, crate::magica::Bonus::Coleta(4)) {
                assert!(pedras > 0, "a mina perdeu as pedras");
            } else {
                assert_eq!(pedras, 0, "{:?}", i.bonus);
                assert!(plantas > 0, "a vegetação sumiu");
            }
        }
    }
}
