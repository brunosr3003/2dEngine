//! O **veio de Energia**: a fenda no chão por onde a Energia sobe, desenhada
//! quadro a quadro, com a luz correndo pra cima.
//!
//! Antes era um modelo de voxel assado no pedaço do terreno, igual a uma
//! pedra: parado, fosco e sem nada que dissesse o que ele é. O dono resumiu
//! em 20/09/2026 — "sai de uma pedra, tá muito ruim; tem que ser um negócio
//! com animação de energia fluindo, tipo um veio". A diferença é essa: o
//! minério é matéria, e se parece com pedra por isso; o veio é **energia
//! escapando**, e tem que se mexer.
//!
//! O que se vê, de baixo pra cima:
//!
//!   * a **fenda**: uma greta escura e comprida no chão, com a borda acesa;
//!   * as **lascas**: quatro cristais afunilados ao longo da greta, em que
//!     uma faixa clara SOBE sem parar — é essa faixa que lê como fluxo;
//!   * o **facho**: uma coluna de luz larga em baixo e fina em cima, virada
//!     pra câmera, pulsando devagar;
//!   * as **fagulhas**: pontinhos que sobem da greta e somem no alto, cada um
//!     na sua fase.
//!
//! Tudo em cor de vértice, no material sólido do mundo — sem shader novo,
//! sem textura, sem segunda passada. Custa uma centena de triângulos por veio
//! e só os veios à vista são desenhados; a Energia é rara (55 no Bosque
//! inteiro), então na prática são um ou dois na tela.

use macroquad::prelude::*;

/// Quantas lascas por veio.
const LASCAS: usize = 6;
/// Quantas fagulhas sobem por veio.
const FAGULHAS: usize = 9;
/// Altura que a fagulha sobe antes de sumir, em unidades.
const SUBIDA: f32 = 3.2;
/// Quanto tempo uma fagulha leva pra subir tudo.
const SUBIDA_S: f32 = 2.4;
/// Altura da lasca mais alta, em unidades. O boneco tem 1,8: a lasca do meio
/// passa dele, senao o veio some no capim quando visto de cima.
const ALTURA: f32 = 3.0;
/// Meia-largura da greta.
const GRETA: f32 = 0.95;
/// Meia-largura da lasca no pe'.
const GROSSURA: f32 = 0.30;

/// Azul-ciano aceso: a cor da Energia em toda a interface
/// (`hud_estilo::icone_energia`, `Material::CristalAzul`).
const ACESO: [f32; 3] = [120.0, 225.0, 255.0];
/// O fundo da lasca, onde ela ainda é pedra.
const FRIO: [f32; 3] = [26.0, 58.0, 92.0];

/// Desenha os veios de `nos` (posição do pé, no chão). `cam` é só pra virar
/// os quads chatos pra tela — lasca é volume de verdade e não depende dela.
///
/// Quem chama já pôs o material do mundo.
pub fn desenha(nos: impl Iterator<Item = Vec3>, cam: &Camera3D, agora: f32) {
    let olho = cam.position;
    for p in nos {
        // Fase por POSIÇÃO: dois veios vizinhos não pulsam em espelho, e a
        // fase é a mesma em toda máquina (nada de aleatório por sessão).
        let fase = (p.x * 0.7 + p.z * 1.3).sin() * 3.0;
        let dir = vec3((p.x * 1.1).sin(), 0.0, (p.z * 0.9).cos()).normalize_or_zero();
        let dir = if dir.length_squared() < 0.1 { Vec3::X } else { dir };
        desenha_um(p, dir, fase, olho, agora);
    }
}

fn desenha_um(pe: Vec3, dir: Vec3, fase: f32, olho: Vec3, agora: f32) {
    let t = agora + fase;
    let lado = dir.cross(Vec3::Y).normalize_or_zero();
    // Pulso lento do veio inteiro: o brilho respira, não pisca.
    let pulso = 0.5 + 0.5 * (t * 1.3).sin();

    greta(pe, dir, lado, pulso);
    // O facho vem ANTES das lascas: desenhado depois, ele passava por cima e
    // lavava o cristal inteiro de cinza.
    facho(pe, olho, pulso);
    // As lascas saem em CACHO, e não em fila: enfileiradas ao longo da greta,
    // de um ângulo elas ficavam uma atrás da outra e o veio virava duas
    // agulhas. O cacho é o mesmo de qualquer lado de onde se olhe.
    lasca(pe, ALTURA, Vec3::ZERO, t);
    let giro = (pe.x * 0.31 + pe.z * 0.17).sin() * 3.0;
    for i in 1..LASCAS {
        let a = giro + (i - 1) as f32 / (LASCAS - 1) as f32 * std::f32::consts::TAU;
        // Raio e altura variando pela volta: seis iguais em roda liam como
        // coroa de aniversário.
        let r = GRETA * (0.34 + 0.22 * (a * 1.7).sin().abs());
        let fora = vec3(a.cos(), 0.0, a.sin());
        let base = pe + fora * r;
        let alta = ALTURA * (0.34 + 0.30 * (a * 2.3).cos().abs());
        // Abrem pra fora: paralelas liam como grade.
        lasca(base, alta, fora * alta * 0.22, t + i as f32 * 0.47);
    }
    for i in 0..FAGULHAS {
        fagulha(pe, dir, lado, i, t);
    }
}

/// O chão do veio: um halo aceso rente à terra com a GRETA escura no meio.
///
/// Na primeira versão o chão era só o losango escuro, e ele lia como sombra
/// debaixo do cristal — buraco preto, não fenda com luz. O que diz "a luz vem
/// de baixo" é o halo: claro colado na greta e sumindo pra fora.
fn greta(pe: Vec3, dir: Vec3, lado: Vec3, pulso: f32) {
    let centro = vec3(pe.x, pe.y + 0.02, pe.z);
    // ── halo ──
    const LADOS: usize = 16;
    let raio = GRETA * 1.5;
    let dentro = cor(ACESO, 1.0, (120.0 + 90.0 * pulso) as u8);
    let (mut v, mut tris) = (vec![vtx(centro, dentro)], Vec::new());
    for k in 0..=LADOS {
        let a = k as f32 / LADOS as f32 * std::f32::consts::TAU;
        v.push(vtx(
            centro + vec3(a.cos() * raio, 0.0, a.sin() * raio),
            cor(ACESO, 1.0, 0),
        ));
        if k > 0 {
            tris.push([0, k as u16, k as u16 + 1]);
        }
    }
    dupla(v, tris);

    // ── a greta em si: uma fatia escura e estreita, um tico acima do halo ──
    let y = Vec3::Y * 0.01;
    let escuro = [10, 34, 52, 255];
    let pontas = [
        centro + dir * GRETA + y,
        centro + lado * GRETA * 0.16 + y,
        centro - dir * GRETA + y,
        centro - lado * GRETA * 0.16 + y,
    ];
    let mut v = vec![vtx(centro + y, cor(ACESO, 1.0, 255))];
    for p in pontas {
        v.push(vtx(p, escuro));
    }
    dupla(v, vec![[0, 1, 2], [0, 2, 3], [0, 3, 4], [0, 4, 1]]);
}

/// Uma lasca: prisma de quatro lados que afunila até a ponta, com uma FAIXA
/// clara subindo. A faixa é cor de vértice — o fluxo sai de graça.
fn lasca(base: Vec3, alta: f32, inclina: Vec3, t: f32) {
    const ANDARES: usize = 5;
    let raio = GROSSURA;
    // A faixa dá uma volta a cada 1,6 s, e não para nunca: energia subindo
    // que para no topo lê como "acabou", e o veio não acaba.
    let onda = (t / 1.6).fract();
    let tom = |h: f32| {
        // Distância da altura `h` (0..1) até a faixa, dando a volta.
        let mut d = (h - onda).abs();
        d = d.min(1.0 - d);
        let perto = (1.0 - (d / 0.28).min(1.0)).powi(2);
        // Base fria, topo aceso, e a faixa acende o que ela cruza.
        let k = (0.25 + 0.55 * h + 0.75 * perto).min(1.0);
        let mut c = [0u8; 4];
        for i in 0..3 {
            c[i] = (FRIO[i] + (ACESO[i] - FRIO[i]) * k).round().clamp(0.0, 255.0) as u8;
        }
        c[3] = 255;
        c
    };
    let (mut v, mut tris) = (Vec::new(), Vec::new());
    for a in 0..=ANDARES {
        let h = a as f32 / ANDARES as f32;
        // Afunila: cheia embaixo, ponta fina em cima.
        // Afunila, mas sem virar agulha: com 0,88 ela fechava quase em ponto
        // e as duas primeiras versoes leram como pingente de gelo.
        let r = raio * (1.0 - h * 0.62);
        let centro = base + inclina * h + Vec3::Y * (h * alta);
        let c = tom(h);
        for k in 0..4 {
            let ang = k as f32 * std::f32::consts::FRAC_PI_2 + 0.4;
            v.push(vtx(
                vec3(centro.x + ang.cos() * r, centro.y, centro.z + ang.sin() * r),
                c,
            ));
        }
    }
    for a in 0..ANDARES {
        for k in 0..4u16 {
            let (i0, i1) = (a as u16 * 4 + k, a as u16 * 4 + (k + 1) % 4);
            let (j0, j1) = (i0 + 4, i1 + 4);
            tris.push([i0, j0, j1]);
            tris.push([i0, j1, i1]);
        }
    }
    dupla(v, tris);
}

/// O facho: um triângulo largo embaixo e fino em cima, sempre virado pra
/// câmera. É o que faz o veio ser visto de longe, antes de qualquer lasca.
fn facho(pe: Vec3, olho: Vec3, pulso: f32) {
    let para_olho = vec3(olho.x - pe.x, 0.0, olho.z - pe.z).normalize_or_zero();
    let lado = para_olho.cross(Vec3::Y).normalize_or_zero();
    if lado.length_squared() < 0.1 {
        return;
    }
    // Fica DENTRO do cacho, sem passar do cristal mais alto. Nas duas
    // primeiras versoes ele subia acima de tudo, e la' em cima — sozinho
    // contra o ceu escuro — o azul claro translucido virava uma lasca
    // cinza espetada, que lia como antena e nao como luz.
    let base = GRETA * 0.5;
    let alto = ALTURA * (0.66 + 0.1 * pulso);
    let baixo = pe + Vec3::Y * 0.03;
    let topo = pe + Vec3::Y * alto;
    let a = (150.0 + 90.0 * pulso) as u8;
    let v = vec![
        vtx(baixo - lado * base, cor(ACESO, 1.0, a)),
        vtx(baixo + lado * base, cor(ACESO, 1.0, a)),
        vtx(topo + lado * base * 0.45, cor(ACESO, 1.0, 0)),
        vtx(topo - lado * base * 0.45, cor(ACESO, 1.0, 0)),
    ];
    dupla(v, vec![[0, 1, 2], [0, 2, 3]]);
}

/// Uma fagulha subindo: quadradinho virado pra cima que sobe e some. Cada uma
/// com a sua fase, pra elas não saírem em fila.
fn fagulha(pe: Vec3, dir: Vec3, lado: Vec3, i: usize, t: f32) {
    let semente = i as f32 * 2.399_9;
    let u = ((t / SUBIDA_S) + semente.fract() + semente * 0.31).fract();
    let alt = u * SUBIDA;
    // Some no fim e nasce sem estourar: escuro nas duas pontas.
    let vida = (1.0 - u) * (u / 0.15).min(1.0);
    if vida <= 0.01 {
        return;
    }
    // Vai abrindo e serpenteando um pouco enquanto sobe.
    let abre = 0.15 + u * 0.55;
    let p = pe
        + dir * (semente.sin() * GRETA * 0.7)
        + lado * (semente.cos() * abre + (t * 1.7 + semente).sin() * 0.08)
        + Vec3::Y * (0.05 + alt);
    let s = 0.075 * (1.0 - u * 0.45);
    let c = cor(ACESO, 1.0, (235.0 * vida) as u8);
    // Dois quads cruzados: de qualquer ângulo sobra alguma coisa pra ver, e
    // sai mais barato que orientar pela câmera.
    for eixo in [Vec3::X, Vec3::Z] {
        let outro = Vec3::Y;
        let v = vec![
            vtx(p - eixo * s - outro * s, c),
            vtx(p + eixo * s - outro * s, c),
            vtx(p + eixo * s + outro * s, c),
            vtx(p - eixo * s + outro * s, c),
        ];
        dupla(v, vec![[0, 1, 2], [0, 2, 3]]);
    }
}

fn cor(base: [f32; 3], k: f32, alfa: u8) -> [u8; 4] {
    [
        (base[0] * k).clamp(0.0, 255.0) as u8,
        (base[1] * k).clamp(0.0, 255.0) as u8,
        (base[2] * k).clamp(0.0, 255.0) as u8,
        alfa,
    ]
}

fn vtx(p: Vec3, c: [u8; 4]) -> Vertex {
    Vertex {
        position: p,
        uv: vec2(0.0, 0.0),
        color: c,
        normal: Vec4::ZERO,
    }
}

/// Desenha dos DOIS lados: o veio é visto de qualquer ângulo, e o material do
/// mundo descarta face de costas.
fn dupla(vertices: Vec<Vertex>, tris: Vec<[u16; 3]>) {
    let mut indices = Vec::with_capacity(tris.len() * 6);
    for [a, b, c] in tris {
        indices.extend_from_slice(&[a, b, c, a, c, b]);
    }
    draw_mesh(&Mesh {
        vertices,
        indices,
        texture: None,
    });
}

#[cfg(test)]
mod testes {
    use super::*;

    /// A faixa da lasca tem que ANDAR: e' ela que le' como fluxo. Se o tom
    /// de uma altura fixa nao mudar com o tempo, o veio voltou a ser pedra.
    #[test]
    fn a_luz_sobe_pela_lasca() {
        // Reproduz a conta do tom de `lasca` (a funcao desenha, entao o que
        // se testa e' a regra dela).
        let tom = |h: f32, t: f32| {
            let onda = (t / 1.6).fract();
            let mut d = (h - onda).abs();
            d = d.min(1.0 - d);
            let perto = (1.0 - (d / 0.28).min(1.0)).powi(2);
            (0.25 + 0.55 * h + 0.75 * perto).min(1.0)
        };
        let meio = 0.5;
        let claros: Vec<f32> = (0..16).map(|k| tom(meio, k as f32 * 0.1)).collect();
        let maior = claros.iter().cloned().fold(f32::MIN, f32::max);
        let menor = claros.iter().cloned().fold(f32::MAX, f32::min);
        assert!(
            maior - menor > 0.3,
            "a faixa nao anda: o meio da lasca varia so' {:.2}",
            maior - menor
        );
        // Com a faixa no MEIO (longe das duas pontas), o topo e' mais aceso
        // que o pe': a lasca acende de baixo pra cima mesmo parada. No t=0 a
        // faixa esta' em cima das duas pontas ao mesmo tempo — a volta faz
        // h=0 e h=1 serem o mesmo ponto —, e ai' as duas dao cheio.
        let no_meio = 1.6 * 0.5;
        assert!(tom(1.0, no_meio) > tom(0.0, no_meio));
        assert!((tom(1.0, 0.0) - tom(0.0, 0.0)).abs() < 0.01, "a volta fecha");
        // A faixa da' a volta: no instante em que ela sai pelo topo, ja'
        // esta' entrando pelo pe' — energia subindo nao tem fim.
        let quase_fim = tom(0.02, 1.6 * 0.995);
        assert!(quase_fim > 0.25, "a faixa some entre uma volta e outra");
    }

    /// A fagulha nasce escura, acende e some — sem estourar no comeco nem
    /// sumir de repente no fim.
    #[test]
    fn a_fagulha_nasce_e_some() {
        let vida = |u: f32| (1.0 - u) * (u / 0.15).min(1.0);
        assert!(vida(0.0) <= 0.01, "nasce do nada");
        assert!(vida(0.99) < 0.05, "some no alto");
        assert!(vida(0.3) > 0.5, "no meio do caminho ela esta' acesa");
        // Ciclica: a fase volta pro comeco sem salto de brilho.
        assert!((vida(0.999) - vida(0.001)).abs() < 0.05);
    }
}

/// Prévia do veio (`MMO_PREVIA_ENERGIA=1`; PNGs em `MMO_PREVIA_SAIDA`).
///
/// Três instantes do mesmo veio, pra dar pra ver a faixa subindo e as
/// fagulhas em posições diferentes — uma captura só não diz se ele se mexe.
/// Só desktop, pra gerar PNG: por isso pode usar render target com
/// profundidade (ver o teste em `personagens.rs`).
#[cfg(debug_assertions)]
pub async fn previa(solido: &Material) {
    let saida =
        std::env::var("MMO_PREVIA_SAIDA").unwrap_or_else(|_| "/tmp/tempest-energia".into());
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
    // Camera do jogo: tres-quartos de cima, na distancia em que o jogador
    // passa por um veio.
    let alvo = vec3(0.0, 0.7, 0.0);
    let cam = Camera3D {
        position: vec3(0.0, 4.2, 7.0),
        target: alvo,
        up: Vec3::Y,
        fovy: 0.9,
        // Sem isto o 3D vai pra TELA e o PNG sai so' com o fundo limpo —
        // foi o que aconteceu na primeira captura.
        render_target: crate::render3d::alvo(),
        ..Default::default()
    };
    for (k, t) in [0.0f32, 0.55, 1.1].into_iter().enumerate() {
        for _ in 0..2 {
            crate::render3d::camera_padrao();
            clear_background(Color::new(0.10, 0.13, 0.11, 1.0));
            set_camera(&cam);
            // Um chao de grama pra o veio ter contra o que brilhar.
            let chao = Color::new(0.20, 0.30, 0.16, 1.0);
            draw_plane(Vec3::ZERO, vec2(14.0, 14.0), None, chao);
            // O boneco nao entra aqui; uma caixa do tamanho dele da' a escala.
            draw_cube(vec3(2.6, 0.9, 0.0), vec3(0.6, 1.8, 0.35), None, GRAY);
            macroquad::material::gl_use_material(solido);
            desenha(std::iter::once(Vec3::ZERO), &cam, t);
            macroquad::material::gl_use_default_material();
            crate::render3d::camera_padrao();
            unsafe { macroquad::window::get_internal_gl().flush() };
            rt.texture
                .get_texture_data()
                .export_png(&format!("{saida}/veio-{k}.png"));
            next_frame().await;
        }
    }
}
