//! Sistema de design da interface: tokens, primitivas arredondadas e
//! componentes. Todo desenho de UI passa por aqui pra o jogo inteiro ter uma
//! cara so' — escura, translucida, com profundidade sutil e cantos macios, no
//! molde de MMO mobile moderno.
//!
//! A macroquad nao tem retangulo arredondado. Aqui ele e' UMA malha (leque a
//! partir do centro) por forma: sem sobreposicao de pecas, entao cor
//! translucida nao escurece nos cantos, e o batcher junta tudo num draw call.
use macroquad::prelude::*;
use std::cell::RefCell;
use std::collections::HashMap;
use std::f32::consts::{PI, TAU};

// ─────────────────────────────── fonte ───────────────────────────────

// A fonte vive pelo processo: nao deve destruir texturas num destructor
// thread-local depois que a janela/contexto OpenGL ja foi encerrado.
static FONTE: std::sync::OnceLock<Font> = std::sync::OnceLock::new();
static FONTE_FORTE: std::sync::OnceLock<Font> = std::sync::OnceLock::new();

fn carrega(bytes: &[u8]) -> Font {
    let mut f = load_ttf_font_from_bytes(bytes).expect("HUD font");
    f.set_filter(FilterMode::Linear);
    f
}

/// Noto Sans (Google, licenca em assets/fonts/LICENSE-NotoSans.txt): legivel
/// em corpo pequeno, com acentos e numeros largos iguais.
fn fonte<T>(forte: bool, f: impl FnOnce(&Font) -> T) -> T {
    if forte {
        f(FONTE_FORTE
            .get_or_init(|| carrega(include_bytes!("../../../assets/fonts/NotoSans-Bold.ttf"))))
    } else {
        f(FONTE
            .get_or_init(|| carrega(include_bytes!("../../../assets/fonts/NotoSans-Regular.ttf"))))
    }
}

/// Fator do texto: a escala da interface escolhida (Menu → Sistema →
/// Interface; 130% no celular por padrao). Todo texto e toda medida passam
/// por aqui, entao o `tamanho` dos chamadores continua "o de 100%".
pub fn fator_texto() -> f32 {
    ESCALA_DO_PAINEL
        .with(|c| c.get())
        .unwrap_or_else(crate::hud_layout::escala_ui)
}

thread_local! {
    /// Escala do painel sendo desenhado agora (`no_painel`): enquanto ele
    /// desenha, `fator_texto` devolve ela — texto e medida crescem juntos.
    static ESCALA_DO_PAINEL: std::cell::Cell<Option<f32>> = const { std::cell::Cell::new(None) };
}

/// A escala de um painel que a 100% mede `base_w` x `base_h`.
///
/// No PC e' a da interface. No celular a tela e' densa e o alvo e' o dedo: o
/// painel cresce ate' encher a area segura (Craft, Forja, Onde obter...),
/// nunca passando dela nem de 2,2x. A escolha em Menu → Interface continua
/// valendo: o padrao (130%) enche; menos que isso encolhe na mesma proporcao.
pub fn escala_do_painel(base_w: f32, base_h: f32) -> f32 {
    let f = crate::hud_layout::escala_ui();
    if !crate::nativo::TECLADO_NA_TELA {
        return f;
    }
    let s = crate::hud_layout::tela_segura();
    let caber = ((s.w - 16.0) / base_w).min((s.h - 16.0) / base_h);
    let gosto = f / crate::hud_layout::escala_ui_padrao();
    (caber * gosto).min(caber).clamp(0.8, 2.2)
}

/// Desenha `corpo` com `fator_texto() == k`: quem mede com o fator (texto,
/// `u`) sai na escala do painel.
pub fn no_painel<T>(k: f32, corpo: impl FnOnce() -> T) -> T {
    let antes = ESCALA_DO_PAINEL.with(|c| c.replace(Some(k)));
    let r = corpo();
    ESCALA_DO_PAINEL.with(|c| c.set(antes));
    r
}

/// Uma medida "de 100%" na escala atual (a do painel, dentro de `no_painel`).
/// O ALVO MÍNIMO DE UM DEDO, em pontos.
///
/// Quarenta e quatro é o piso que a Apple publica na HIG, e o número bate com
/// a queixa: os botões do diálogo ("Próximo", "Receber") tinham 28 px de
/// altura, e o dono "clicava errado toda hora".
///
/// Varrendo o cliente com este piso saíram **55** botões abaixo dele, de 28 a
/// 42. Consertar 55 chamadas à mão é o jeito certo de esquecer três, então
/// quem cresce é a ÁREA DE TOQUE em `ui::botao` — o desenho fica onde o
/// layout pôs.
pub const ALVO_DO_DEDO: f32 = 44.0;

pub fn u(v: f32) -> f32 {
    v * fator_texto()
}

fn tam(tamanho: u16) -> u16 {
    (tamanho as f32 * fator_texto()).round().clamp(1.0, 400.0) as u16
}

/// O GARGALO DA TRADUCAO.
///
/// Todo texto do cliente passa por aqui (726 pontos de chamada, via `texto`,
/// `texto_centro`, `texto_forte`…). Traduzir neste ponto pega de uma vez o
/// rotulo escrito no codigo, o nome de item que veio do banco e o aviso que o
/// servidor mandou — sem tocar em nenhum desses lugares.
///
/// Em portugues `tr` devolve o que recebeu e o custo e' zero. Sem verbete, o
/// texto sai em portugues: falta de traducao nao apaga informacao.
///
/// Quem MEXE na string antes de desenhar (quebra em linhas, corta com "…")
/// tem que traduzir ANTES de mexer, senao o pedaco nao casa com verbete
/// nenhum. Por isso `texto_ajustado` e as funcoes de quebra traduzem na
/// entrada; aqui a segunda passada nao encontra verbete e nao faz nada.
fn desenha_texto(forte: bool, x: f32, y: f32, s: &str, tamanho: u16, cor: Color) {
    let s = &shared::idioma::tr(s);
    fonte(forte, |f| {
        draw_text_ex(
            s,
            x,
            y,
            TextParams {
                font: Some(f),
                font_size: tam(tamanho),
                color: cor,
                ..Default::default()
            },
        );
    });
}

pub fn medir(s: &str, tamanho: u16) -> f32 {
    let s = &shared::idioma::tr(s);
    fonte(false, |f| measure_text(s, Some(f), tam(tamanho), 1.0).width)
}
/// Dimensoes na fonte da UI — pra quem alinha texto com `TextDimensions`.
pub fn medir_dim(s: &str, tamanho: u16) -> TextDimensions {
    let s = &shared::idioma::tr(s);
    fonte(false, |f| measure_text(s, Some(f), tam(tamanho), 1.0))
}
pub fn medir_forte(s: &str, tamanho: u16) -> f32 {
    let s = &shared::idioma::tr(s);
    fonte(true, |f| measure_text(s, Some(f), tam(tamanho), 1.0).width)
}

// ─────────────────────────────── moedas ───────────────────────────────

/// O cristal da Tempestade (TP), centrado em `c`. O MESMO desenho em todo
/// lugar que mostra TP: loja, mercado, confirmacoes.
pub fn icone_tp(c: Vec2, lado: f32) {
    if !crate::icones_ui::loja("tp", c, lado, 1.0) {
        draw_poly(c.x, c.y, 6, lado * 0.42, 30.0, AZUL);
    }
}

/// Cristal azul da Energia de habilidades (saldo, nao item de bolsa).
pub fn icone_energia(c: Vec2, lado: f32) {
    let r = lado * 0.46;
    let topo = vec2(c.x, c.y - r);
    let esq = vec2(c.x - r * 0.72, c.y - r * 0.18);
    let dir = vec2(c.x + r * 0.72, c.y - r * 0.18);
    let baixo = vec2(c.x, c.y + r);
    draw_triangle(topo, esq, c, Color::new(0.64, 0.98, 1.0, 1.0));
    draw_triangle(topo, c, dir, Color::new(0.22, 0.77, 1.0, 1.0));
    draw_triangle(esq, baixo, c, Color::new(0.12, 0.53, 0.86, 1.0));
    draw_triangle(c, baixo, dir, Color::new(0.09, 0.37, 0.74, 1.0));
    draw_line(topo.x, topo.y, baixo.x, baixo.y, lado * 0.055, WHITE);
}

/// O PASSE DA ILHA MÁGICA, centrado em `c`.
///
/// Desenhado, e não do atlas: o atlas da loja só tem ouro e TP, e um ícone
/// novo ali obrigaria a regerar a textura — trabalho de arte para uma coisa
/// que são duas formas. É um bilhete com o entalhe dos dois lados, que é
/// como se lê "entrada" sem legenda nenhuma.
pub fn icone_passe(c: Vec2, lado: f32) {
    let (w, h) = (lado * 0.78, lado * 0.46);
    let r = Rect::new(c.x - w * 0.5, c.y - h * 0.5, w, h);
    let roxo = Color::new(0.66, 0.44, 0.94, 1.0);
    let claro = Color::new(0.88, 0.76, 1.0, 1.0);
    draw_rectangle(r.x, r.y, r.w, r.h, roxo);
    draw_rectangle_lines(r.x, r.y, r.w, r.h, lado * 0.05, claro);
    // O entalhe: dois semicírculos da cor do fundo comendo as laterais.
    let fundo = Color::new(0.055, 0.045, 0.13, 1.0);
    draw_circle(r.x, c.y, h * 0.22, fundo);
    draw_circle(r.x + r.w, c.y, h * 0.22, fundo);
    // A estrela pequena no meio: é passe de ilha MÁGICA.
    let e = lado * 0.13;
    draw_triangle(
        vec2(c.x, c.y - e),
        vec2(c.x - e * 0.86, c.y + e * 0.6),
        vec2(c.x + e * 0.86, c.y + e * 0.6),
        claro,
    );
    draw_triangle(
        vec2(c.x, c.y + e),
        vec2(c.x - e * 0.86, c.y - e * 0.6),
        vec2(c.x + e * 0.86, c.y - e * 0.6),
        claro,
    );
}

/// A moeda de ouro, centrada em `c`.
pub fn icone_ouro(c: Vec2, lado: f32) {
    if !crate::icones_ui::loja("ouro", c, lado, 1.0) {
        draw_circle(c.x, c.y, lado * 0.4, OURO);
    }
}

fn moeda_texto(tp: bool, x: f32, y: f32, s: &str, tamanho: u16, cor: Color, forte: bool) -> f32 {
    let px = tam(tamanho) as f32;
    let lado = px * 1.25;
    let c = vec2(x + lado * 0.5, y - px * 0.34);
    if tp {
        icone_tp(c, lado)
    } else {
        icone_ouro(c, lado)
    }
    let tx = x + lado + px * 0.2;
    if forte {
        texto_forte(tx, y, s, tamanho, cor)
    } else {
        texto(tx, y, s, tamanho, cor)
    }
    tx - x
        + if forte {
            medir_forte(s, tamanho)
        } else {
            medir(s, tamanho)
        }
}

fn largura_moeda(s: &str, tamanho: u16, forte: bool) -> f32 {
    let px = tam(tamanho) as f32;
    px * 1.45
        + if forte {
            medir_forte(s, tamanho)
        } else {
            medir(s, tamanho)
        }
}

/// Icone do TP + texto na linha de base `y`. Devolve a largura desenhada.
pub fn tp_texto(x: f32, y: f32, s: &str, tamanho: u16, cor: Color, forte: bool) -> f32 {
    moeda_texto(true, x, y, s, tamanho, cor, forte)
}
pub fn largura_tp_texto(s: &str, tamanho: u16, forte: bool) -> f32 {
    largura_moeda(s, tamanho, forte)
}
/// Icone do TP + quantidade formatada ("1.250"), em negrito.
pub fn valor_tp(x: f32, y: f32, qtd: u64, tamanho: u16, cor: Color) -> f32 {
    tp_texto(x, y, &crate::economia::milhar(qtd), tamanho, cor, true)
}
/// Icone do ouro + texto na linha de base `y`. Devolve a largura desenhada.
pub fn ouro_texto(x: f32, y: f32, s: &str, tamanho: u16, cor: Color, forte: bool) -> f32 {
    moeda_texto(false, x, y, s, tamanho, cor, forte)
}
pub fn largura_ouro_texto(s: &str, tamanho: u16, forte: bool) -> f32 {
    largura_moeda(s, tamanho, forte)
}
pub fn texto(x: f32, y: f32, s: &str, tamanho: u16, cor: Color) {
    desenha_texto(false, x, y, s, tamanho, cor);
}
pub fn texto_forte(x: f32, y: f32, s: &str, tamanho: u16, cor: Color) {
    desenha_texto(true, x, y, s, tamanho, cor);
}
pub fn texto_centro(x: f32, y: f32, s: &str, tamanho: u16, cor: Color) {
    texto(x - medir(s, tamanho) * 0.5, y, s, tamanho, cor);
}
pub fn texto_centro_forte(x: f32, y: f32, s: &str, tamanho: u16, cor: Color) {
    texto_forte(x - medir_forte(s, tamanho) * 0.5, y, s, tamanho, cor);
}

/// Texto com sombra curta embaixo: le em cima do mundo (chao claro, neve).
pub fn texto_sombra(x: f32, y: f32, s: &str, tamanho: u16, cor: Color, forte: bool) {
    desenha_texto(
        forte,
        x + 1.0,
        y + 1.5,
        s,
        tamanho,
        Color::new(0.0, 0.0, 0.0, 0.75 * cor.a),
    );
    desenha_texto(forte, x, y, s, tamanho, cor);
}

pub fn texto_ajustado(s: &str, x: f32, y: f32, largura: f32, tamanho: u16, cor: Color) {
    // Traduz ANTES de cortar: "Disponivel…" cortado no meio nao casaria com
    // verbete nenhum, e o painel mostraria portugues cortado no meio do ingles.
    let mut t = shared::idioma::tr(s).into_owned();
    if medir(&t, tamanho) > largura {
        while !t.is_empty() && medir(&format!("{t}…"), tamanho) > largura {
            t.pop();
        }
        t.push('…');
    }
    texto(x, y, &t, tamanho, cor);
}

// ─────────────────────────────── tokens ──────────────────────────────

/// Superficie de painel: azul-ardosia escuro, translucido.
pub const FUNDO: Color = Color::new(0.040, 0.052, 0.078, 0.88);
/// Topo do gradiente do painel e cartoes elevados.
pub const FUNDO_ALTO: Color = Color::new(0.085, 0.102, 0.140, 0.92);
/// Trilhos e areas rebaixadas (fundo de barra, slot vazio).
pub const FUNDO_BAIXO: Color = Color::new(0.020, 0.026, 0.040, 0.92);
/// Borda de 1 px quase invisivel: separa sem desenhar caixa.
pub const BORDA: Color = Color::new(0.62, 0.70, 0.82, 0.20);
pub const BORDA_FORTE: Color = Color::new(0.66, 0.74, 0.86, 0.42);
/// O filete claro no topo que da' a sensacao de vidro.
pub const BRILHO: Color = Color::new(1.0, 1.0, 1.0, 0.08);
/// Dourado: raridade, chefe, destaque de valor.
pub const OURO: Color = Color::new(0.97, 0.79, 0.44, 1.0);
/// Acento principal (selecao, hover, links).
pub const ACENTO: Color = Color::new(0.42, 0.80, 1.0, 1.0);
pub const TEXTO: Color = Color::new(0.95, 0.96, 0.98, 1.0);
pub const SUAVE: Color = Color::new(0.64, 0.70, 0.78, 1.0);
pub const AUTO: Color = Color::new(0.37, 0.94, 0.76, 1.0);
pub const VERMELHO: Color = Color::new(0.95, 0.36, 0.36, 1.0);
pub const VERDE: Color = Color::new(0.42, 0.88, 0.52, 1.0);
pub const AZUL: Color = Color::new(0.38, 0.64, 1.0, 1.0);
pub const SOMBRA: Color = Color::new(0.0, 0.0, 0.0, 0.34);

pub const RAIO: f32 = 9.0;
pub const RAIO_PEQUENO: f32 = 5.0;
pub const RAIO_GRANDE: f32 = 14.0;
pub const ESPACO: f32 = 8.0;

/// Escala tipografica.
pub mod tam {
    pub const TITULO: u16 = 20;
    pub const SUBTITULO: u16 = 16;
    pub const CORPO: u16 = 14;
    pub const LEGENDA: u16 = 12;
    pub const MINI: u16 = 11;
}

pub fn alfa(c: Color, a: f32) -> Color {
    Color::new(c.r, c.g, c.b, a)
}
pub fn misturar(a: Color, b: Color, t: f32) -> Color {
    let t = t.clamp(0.0, 1.0);
    Color::new(
        a.r + (b.r - a.r) * t,
        a.g + (b.g - a.g) * t,
        a.b + (b.b - a.b) * t,
        a.a + (b.a - a.a) * t,
    )
}
/// Mais claro (k > 0) ou mais escuro (k < 0), mantendo o alfa.
pub fn clarear(c: Color, k: f32) -> Color {
    if k >= 0.0 {
        misturar(c, Color::new(1.0, 1.0, 1.0, c.a), k)
    } else {
        misturar(c, Color::new(0.0, 0.0, 0.0, c.a), -k)
    }
}

/// Luminancia relativa (WCAG) de uma cor ja' opaca.
pub fn luminancia(c: Color) -> f32 {
    let canal = |v: f32| {
        if v <= 0.03928 {
            v / 12.92
        } else {
            ((v + 0.055) / 1.055).powf(2.4)
        }
    };
    0.2126 * canal(c.r) + 0.7152 * canal(c.g) + 0.0722 * canal(c.b)
}

/// Razao de contraste entre texto e um fundo translucido posto sobre `cena`.
pub fn contraste(texto: Color, fundo: Color, cena: Color) -> f32 {
    let f = misturar(cena, Color::new(fundo.r, fundo.g, fundo.b, 1.0), fundo.a);
    let (a, b) = (luminancia(texto), luminancia(f));
    (a.max(b) + 0.05) / (a.min(b) + 0.05)
}

// ───────────────────────────── geometria ─────────────────────────────

fn raio_util(r: Rect, raio: f32) -> f32 {
    raio.min(r.w * 0.5).min(r.h * 0.5).max(0.0)
}

/// Lados de cada quarto de circulo: poucos em canto pequeno, mais em grande.
fn lados(raio: f32) -> usize {
    ((raio * 0.6).ceil() as usize).clamp(2, 8)
}

/// O contorno fechado de um retangulo arredondado, no sentido horario a
/// partir do canto superior esquerdo. `4 × (lados + 1)` pontos.
pub fn contorno_arredondado(r: Rect, raio: f32, lados: usize) -> Vec<Vec2> {
    let ra = raio_util(r, raio);
    let cantos = [
        (vec2(r.x + ra, r.y + ra), PI),
        (vec2(r.x + r.w - ra, r.y + ra), PI * 1.5),
        (vec2(r.x + r.w - ra, r.y + r.h - ra), 0.0),
        (vec2(r.x + ra, r.y + r.h - ra), PI * 0.5),
    ];
    let mut pts = Vec::with_capacity(4 * (lados + 1));
    for (c, a0) in cantos {
        for i in 0..=lados {
            let a = a0 + PI * 0.5 * i as f32 / lados as f32;
            pts.push(c + vec2(a.cos(), a.sin()) * ra);
        }
    }
    pts
}

fn vertice(p: Vec2, c: Color) -> Vertex {
    Vertex {
        position: vec3(p.x, p.y, 0.0),
        uv: Vec2::ZERO,
        color: [
            (c.r * 255.0) as u8,
            (c.g * 255.0) as u8,
            (c.b * 255.0) as u8,
            (c.a * 255.0) as u8,
        ],
        normal: Vec4::ZERO,
    }
}

/// Preenche um retangulo arredondado com gradiente vertical (`topo` → `base`).
pub fn ret_gradiente(r: Rect, raio: f32, topo: Color, base: Color) {
    if r.w <= 0.0 || r.h <= 0.0 {
        return;
    }
    let pts = contorno_arredondado(r, raio, lados(raio));
    let cor_em = |y: f32| misturar(topo, base, (y - r.y) / r.h.max(1.0));
    let mut vertices = Vec::with_capacity(pts.len() + 1);
    let centro = r.center();
    vertices.push(vertice(centro, cor_em(centro.y)));
    for p in &pts {
        vertices.push(vertice(*p, cor_em(p.y)));
    }
    let n = pts.len() as u16;
    let mut indices = Vec::with_capacity(pts.len() * 3);
    for i in 0..n {
        indices.extend_from_slice(&[0, 1 + i, 1 + (i + 1) % n]);
    }
    draw_mesh(&Mesh {
        vertices,
        indices,
        texture: None,
    });
}

pub fn ret_arredondado(r: Rect, raio: f32, cor: Color) {
    ret_gradiente(r, raio, cor, cor);
}

/// Borda de espessura `esp` por dentro do retangulo, como um anel.
pub fn borda_arredondada(r: Rect, raio: f32, esp: f32, cor: Color) {
    if r.w <= esp * 2.0 || r.h <= esp * 2.0 {
        return;
    }
    let l = lados(raio);
    let fora = contorno_arredondado(r, raio, l);
    let dentro = contorno_arredondado(
        Rect::new(r.x + esp, r.y + esp, r.w - esp * 2.0, r.h - esp * 2.0),
        (raio - esp).max(0.0),
        l,
    );
    let mut vertices = Vec::with_capacity(fora.len() * 2);
    for (a, b) in fora.iter().zip(dentro.iter()) {
        vertices.push(vertice(*a, cor));
        vertices.push(vertice(*b, cor));
    }
    let n = fora.len() as u16;
    let mut indices = Vec::with_capacity(fora.len() * 6);
    for i in 0..n {
        let (a, b, c, d) = (i * 2, i * 2 + 1, ((i + 1) % n) * 2, ((i + 1) % n) * 2 + 1);
        indices.extend_from_slice(&[a, b, c, b, d, c]);
    }
    draw_mesh(&Mesh {
        vertices,
        indices,
        texture: None,
    });
}

/// Sombra suave: tres camadas crescendo e sumindo, deslocadas pra baixo.
pub fn sombra(r: Rect, raio: f32, forca: f32) {
    for (k, a) in [(1.0, 0.16), (4.0, 0.09), (8.0, 0.05)] {
        ret_arredondado(
            Rect::new(
                r.x - k,
                r.y - k + 3.0 + k * 0.4,
                r.w + k * 2.0,
                r.h + k * 2.0,
            ),
            raio + k,
            Color::new(0.0, 0.0, 0.0, a * forca),
        );
    }
}

/// Filete claro no topo (vidro).
pub fn brilho_topo(r: Rect, raio: f32) {
    let ra = raio_util(r, raio);
    draw_line(r.x + ra, r.y + 1.0, r.x + r.w - ra, r.y + 1.0, 1.0, BRILHO);
}

/// Traco com ponta redonda: pictogramas com a mesma "caneta".
pub fn traco(a: Vec2, b: Vec2, w: f32, cor: Color) {
    draw_line(a.x, a.y, b.x, b.y, w, cor);
    draw_circle(a.x, a.y, w * 0.5, cor);
    draw_circle(b.x, b.y, w * 0.5, cor);
}

pub fn arco(c: Vec2, r: f32, inicio: f32, fracao: f32, esp: f32, cor: Color) {
    let n = ((fracao.abs() * r * 0.6).ceil() as usize).clamp(4, 64);
    for i in 0..n {
        let a = inicio + TAU * fracao * i as f32 / n as f32;
        let b = inicio + TAU * fracao * (i + 1) as f32 / n as f32;
        let p = c + vec2(a.cos(), a.sin()) * r;
        let q = c + vec2(b.cos(), b.sin()) * r;
        draw_line(p.x, p.y, q.x, q.y, esp, cor);
    }
}

pub fn setor(c: Vec2, r: f32, fracao: f32, cor: Color) {
    let n = ((fracao.clamp(0.0, 1.0) * r * 0.6).ceil() as usize).clamp(3, 64);
    for i in 0..n {
        let a = -PI * 0.5 + TAU * fracao * i as f32 / n as f32;
        let b = -PI * 0.5 + TAU * fracao * (i + 1) as f32 / n as f32;
        draw_triangle(
            c,
            c + vec2(a.cos(), a.sin()) * r,
            c + vec2(b.cos(), b.sin()) * r,
            cor,
        );
    }
}

// ───────────────────────────── animacao ──────────────────────────────

thread_local! {
    static ANIMA: RefCell<HashMap<(i32, i32, u8), f32>> = RefCell::new(HashMap::new());
}

/// Valor que persegue `alvo` em ~120 ms, guardado por `chave` (posicao do
/// widget + canal). Hover e pressionado sem mudar a assinatura de ninguem.
pub fn anima(chave: (i32, i32, u8), alvo: f32) -> f32 {
    passo_anima(chave, alvo, get_frame_time())
}

fn passo_anima(chave: (i32, i32, u8), alvo: f32, dt: f32) -> f32 {
    ANIMA.with(|m| {
        let mut m = m.borrow_mut();
        if m.len() > 512 {
            m.clear();
        }
        let v = m.entry(chave).or_insert(alvo);
        *v += (alvo - *v) * (1.0 - (-dt.max(0.0) * 14.0).exp());
        *v
    })
}

fn chave(r: Rect, canal: u8) -> (i32, i32, u8) {
    (r.x as i32, r.y as i32, canal)
}

// ───────────────────────────── componentes ───────────────────────────

/// Estado visual de um controle.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Estado {
    Normal,
    Sobre,
    Pressionado,
    Ativo,
    Desabilitado,
}

/// Prioridade: desabilitado vence tudo; depois pressionado, ativo, hover.
pub fn estado(sobre: bool, pressionado: bool, desabilitado: bool, ativo: bool) -> Estado {
    if desabilitado {
        Estado::Desabilitado
    } else if sobre && pressionado {
        Estado::Pressionado
    } else if ativo {
        Estado::Ativo
    } else if sobre {
        Estado::Sobre
    } else {
        Estado::Normal
    }
}

pub fn estado_de(r: Rect, desabilitado: bool, ativo: bool) -> Estado {
    let sobre = r.contains(Vec2::from(mouse_position()));
    estado(
        sobre,
        is_mouse_button_down(MouseButton::Left),
        desabilitado,
        ativo,
    )
}

/// Painel padrao: sombra macia, vidro escuro em gradiente, filete de brilho e
/// borda de 1 px quase invisivel.
pub fn painel(r: Rect) {
    sombra(r, RAIO, 1.0);
    ret_gradiente(r, RAIO, FUNDO_ALTO, FUNDO);
    borda_arredondada(r, RAIO, 1.0, BORDA);
    brilho_topo(r, RAIO);
}

/// Painel com filete colorido no topo (janelas principais, chefe, raridade).
pub fn painel_destaque(r: Rect, cor: Color) {
    painel(r);
    let w = (r.w * 0.34).min(160.0);
    ret_arredondado(
        Rect::new(r.center().x - w * 0.5, r.y - 1.0, w, 3.0),
        1.5,
        alfa(cor, 0.9),
    );
}

/// Cartao dentro de painel (item de lista, ladrilho de menu).
pub fn cartao(r: Rect, sobre: bool, ativo: bool) {
    let h = anima(chave(r, 1), if sobre || ativo { 1.0 } else { 0.0 });
    ret_gradiente(
        r,
        RAIO_PEQUENO + 2.0,
        misturar(FUNDO_ALTO, clarear(FUNDO_ALTO, 0.10), h),
        FUNDO,
    );
    let borda = if ativo {
        alfa(ACENTO, 0.75)
    } else {
        misturar(BORDA, BORDA_FORTE, h)
    };
    borda_arredondada(r, RAIO_PEQUENO + 2.0, 1.0, borda);
    brilho_topo(r, RAIO_PEQUENO + 2.0);
}

/// Botao com rotulo. `primario` pinta de acento; o resto e' vidro.
pub fn botao(r: Rect, rotulo: &str, e: Estado, primario: bool) {
    let h = anima(
        chave(r, 2),
        match e {
            Estado::Sobre | Estado::Ativo => 1.0,
            Estado::Pressionado => 0.6,
            _ => 0.0,
        },
    );
    let desab = e == Estado::Desabilitado;
    let base = if primario {
        misturar(
            Color::new(0.16, 0.42, 0.62, 0.95),
            Color::new(0.22, 0.52, 0.74, 0.97),
            h,
        )
    } else {
        misturar(FUNDO_ALTO, clarear(FUNDO_ALTO, 0.12), h)
    };
    let base = if desab { alfa(FUNDO_ALTO, 0.6) } else { base };
    let desce = if e == Estado::Pressionado { 1.0 } else { 0.0 };
    let rr = Rect::new(r.x, r.y + desce, r.w, r.h);
    if !desab {
        sombra(rr, RAIO_PEQUENO + 2.0, 0.6);
    }
    ret_gradiente(rr, RAIO_PEQUENO + 2.0, clarear(base, 0.08), base);
    borda_arredondada(
        rr,
        RAIO_PEQUENO + 2.0,
        1.0,
        if primario {
            alfa(ACENTO, 0.55 + 0.3 * h)
        } else {
            misturar(BORDA, BORDA_FORTE, h)
        },
    );
    brilho_topo(rr, RAIO_PEQUENO + 2.0);
    let t = tam::CORPO.min(((r.h * 0.5) as u16).max(10));
    let cor = if desab { alfa(SUAVE, 0.6) } else { TEXTO };
    texto_centro_forte(
        rr.center().x,
        rr.center().y + t as f32 * 0.36,
        rotulo,
        t,
        cor,
    );
}

/// Botao redondo de acao (ATACAR, skills, AUTO): disco escuro com anel de
/// acento e halo quando ativo.
pub fn botao_redondo(c: Vec2, raio: f32, cor: Color, e: Estado, halo: bool) {
    let h = anima(
        (c.x as i32, c.y as i32, 3),
        match e {
            Estado::Sobre => 1.0,
            Estado::Pressionado => 0.5,
            _ => 0.0,
        },
    );
    let r = if e == Estado::Pressionado {
        raio * 0.96
    } else {
        raio
    };
    draw_circle(c.x, c.y + 4.0, r + 3.0, Color::new(0.0, 0.0, 0.0, 0.30));
    if halo {
        let p = 0.5 + 0.5 * (get_time() as f32 * 3.0).sin();
        draw_circle(c.x, c.y, r + 6.0 + 2.0 * p, alfa(cor, 0.10 + 0.06 * p));
    }
    draw_circle(c.x, c.y, r, FUNDO_BAIXO);
    draw_circle(c.x, c.y - r * 0.08, r * 0.94, alfa(FUNDO_ALTO, 0.95));
    draw_circle(c.x, c.y + r * 0.10, r * 0.80, alfa(FUNDO, 0.9));
    draw_circle_lines(c.x, c.y, r - 1.0, 2.0 + h, alfa(cor, 0.55 + 0.35 * h));
    draw_circle_lines(c.x, c.y, r - 5.0, 1.0, alfa(cor, 0.16));
    arco(
        c,
        r - 1.0,
        PI * 1.15,
        0.20,
        1.5,
        Color::new(1.0, 1.0, 1.0, 0.18 + 0.12 * h),
    );
    if e == Estado::Desabilitado {
        draw_circle(c.x, c.y, r, Color::new(0.0, 0.0, 0.0, 0.45));
    }
}

/// Aba: pilula sutil; a ativa tem sublinhado de acento.
pub fn aba(r: Rect, rotulo: &str, ativa: bool, sobre: bool) {
    let h = anima(chave(r, 4), if ativa || sobre { 1.0 } else { 0.0 });
    if ativa || h > 0.01 {
        ret_arredondado(
            r,
            RAIO_PEQUENO + 1.0,
            alfa(
                clarear(FUNDO_ALTO, 0.10),
                0.85 * h.max(if ativa { 1.0 } else { 0.0 }),
            ),
        );
    }
    let t = tam::LEGENDA.min(((r.h * 0.46) as u16).max(10));
    let cor = if ativa {
        TEXTO
    } else {
        misturar(SUAVE, TEXTO, h)
    };
    texto_centro_forte(r.center().x, r.center().y + t as f32 * 0.36, rotulo, t, cor);
    if ativa {
        let w = (r.w * 0.45).min(56.0);
        ret_arredondado(
            Rect::new(r.center().x - w * 0.5, r.y + r.h - 3.0, w, 3.0),
            1.5,
            ACENTO,
        );
    }
}

/// Chip de tecla (so' aparece com Alt, quem decide e' o layout).
pub fn chip_tecla(canto: Vec2, tecla: &str) {
    let w = (medir_forte(tecla, tam::MINI) + 10.0).max(20.0);
    let c = Rect::new(canto.x - 2.0, canto.y - 2.0, w, 19.0);
    sombra(c, RAIO_PEQUENO, 0.8);
    ret_gradiente(
        c,
        RAIO_PEQUENO,
        Color::new(0.16, 0.17, 0.21, 0.97),
        Color::new(0.08, 0.09, 0.12, 0.97),
    );
    borda_arredondada(c, RAIO_PEQUENO, 1.0, alfa(OURO, 0.7));
    texto_centro_forte(c.center().x, c.y + 14.0, tecla, tam::MINI, OURO);
}

/// Barra de progresso em pilula: trilho rebaixado, preenchimento em gradiente
/// com brilho, "fantasma" claro do que acabou de sair e texto no meio.
pub fn barra(r: Rect, f: f32, fantasma: f32, cor: Color, rotulo: Option<&str>) {
    let f = f.clamp(0.0, 1.0);
    let raio = (r.h * 0.5).min(RAIO_PEQUENO + 2.0);
    ret_arredondado(
        Rect::new(r.x - 1.0, r.y - 1.0, r.w + 2.0, r.h + 2.0),
        raio + 1.0,
        Color::new(0.0, 0.0, 0.0, 0.55),
    );
    ret_gradiente(r, raio, FUNDO_BAIXO, alfa(clarear(FUNDO_BAIXO, 0.06), 0.95));
    let fant = fantasma.clamp(0.0, 1.0);
    if fant > f + 0.001 {
        ret_arredondado(
            Rect::new(r.x, r.y, r.w * fant, r.h),
            raio,
            Color::new(1.0, 0.93, 0.82, 0.55),
        );
    }
    if f > 0.001 {
        let w = (r.w * f).max(raio * 2.0).min(r.w);
        let cheio = Rect::new(r.x, r.y, w, r.h);
        ret_gradiente(cheio, raio, clarear(cor, 0.22), clarear(cor, -0.18));
        if r.h >= 6.0 {
            ret_arredondado(
                Rect::new(r.x + 2.0, r.y + 1.5, (w - 4.0).max(0.0), r.h * 0.34),
                raio * 0.6,
                Color::new(1.0, 1.0, 1.0, 0.18),
            );
        }
    }
    if let Some(t) = rotulo {
        let tamanho = (r.h * 0.82).clamp(10.0, 15.0) as u16;
        let largura = medir_forte(t, tamanho);
        texto_sombra(
            r.x + (r.w - largura) * 0.5,
            r.y + r.h * 0.5 + tamanho as f32 * 0.36,
            t,
            tamanho,
            TEXTO,
            true,
        );
    }
}

/// Slot de item: fundo rebaixado, moldura na cor da raridade (se houver) com
/// um brilho de baixo pra cima, e realce no hover/selecao.
pub fn slot(r: Rect, raridade: Option<Color>, sobre: bool, selecionado: bool) {
    let h = anima(chave(r, 5), if sobre { 1.0 } else { 0.0 });
    ret_gradiente(
        r,
        RAIO_PEQUENO + 1.0,
        alfa(clarear(FUNDO_BAIXO, 0.05), 0.95),
        FUNDO_BAIXO,
    );
    if let Some(c) = raridade {
        ret_gradiente(
            Rect::new(r.x, r.y + r.h * 0.35, r.w, r.h * 0.65),
            RAIO_PEQUENO + 1.0,
            alfa(c, 0.0),
            alfa(c, 0.22),
        );
        borda_arredondada(r, RAIO_PEQUENO + 1.0, 1.5, alfa(c, 0.65 + 0.35 * h));
    } else {
        borda_arredondada(r, RAIO_PEQUENO + 1.0, 1.0, misturar(BORDA, BORDA_FORTE, h));
    }
    if selecionado {
        borda_arredondada(
            Rect::new(r.x - 3.0, r.y - 3.0, r.w + 6.0, r.h + 6.0),
            RAIO_PEQUENO + 4.0,
            2.0,
            OURO,
        );
    }
}

/// Dica flutuante presa a `ancora`, acima ou abaixo, sempre dentro da tela.
pub fn tooltip(ancora: Rect, t: &str, acima: bool) {
    let w = medir(t, tam::LEGENDA + 1) + 20.0;
    let x = (ancora.center().x - w * 0.5).clamp(4.0, (screen_width() - w - 4.0).max(4.0));
    let y = if acima {
        ancora.y - 32.0
    } else {
        ancora.y + ancora.h + 6.0
    };
    let c = Rect::new(x, y, w, 26.0);
    sombra(c, RAIO_PEQUENO + 1.0, 1.0);
    ret_gradiente(
        c,
        RAIO_PEQUENO + 1.0,
        Color::new(0.10, 0.12, 0.16, 0.97),
        Color::new(0.06, 0.07, 0.10, 0.97),
    );
    borda_arredondada(c, RAIO_PEQUENO + 1.0, 1.0, BORDA_FORTE);
    texto(c.x + 10.0, c.y + 18.0, t, tam::LEGENDA + 1, TEXTO);
}

/// Ponto vermelho de "ha' algo a fazer", com anel escuro.
pub fn badge(r: Rect) {
    let c = vec2(r.x + r.w - 4.0, r.y + 4.0);
    let p = 0.5 + 0.5 * (get_time() as f32 * 4.0).sin();
    draw_circle(c.x, c.y, 7.5 + p, alfa(VERMELHO, 0.18));
    draw_circle(c.x, c.y, 5.5, Color::new(0.05, 0.05, 0.07, 0.95));
    draw_circle(c.x, c.y, 4.2, VERMELHO);
    draw_circle(c.x - 1.2, c.y - 1.2, 1.3, Color::new(1.0, 0.8, 0.8, 0.8));
}

/// Separador horizontal que some nas pontas.
pub fn separador(x: f32, y: f32, w: f32) {
    let meio = w * 0.5;
    draw_line(x, y, x + meio, y, 1.0, alfa(BORDA_FORTE, 0.25));
    draw_line(
        x + meio * 0.3,
        y,
        x + w - meio * 0.3,
        y,
        1.0,
        alfa(BORDA_FORTE, 0.45),
    );
}

/// Barra de rolagem fina.
pub fn scroll(trilho: Rect, inicio: f32, tamanho: f32) {
    ret_arredondado(trilho, trilho.w * 0.5, alfa(FUNDO_BAIXO, 0.8));
    let h = (trilho.h * tamanho.clamp(0.05, 1.0)).max(trilho.w * 2.0);
    let y = trilho.y + (trilho.h - h) * inicio.clamp(0.0, 1.0);
    ret_arredondado(
        Rect::new(trilho.x, y, trilho.w, h),
        trilho.w * 0.5,
        alfa(SUAVE, 0.55),
    );
}

// ───────────────────────────── pictogramas ───────────────────────────

pub fn cor_skill(id: u32) -> Color {
    match id {
        1..=3 => Color::new(1.0, 0.74, 0.33, 1.0),
        4..=6 => Color::new(0.38, 0.86, 1.0, 1.0),
        7..=9 => Color::new(1.0, 0.52, 0.27, 1.0),
        10 | 11 => AUTO,
        _ => Color::new(0.79, 0.58, 1.0, 1.0),
    }
}

/// Pictogramas vetoriais próprios: nenhuma textura ou arte de outro jogo.
/// Tracos com ponta redonda, mesma caneta em todos.
pub fn icone(id: u32, c: Vec2, r: f32, cor: Color) {
    let p = |x, y| c + vec2(x, y) * r;
    let linha = |a: Vec2, b: Vec2, w: f32| traco(a, b, (w * r).max(1.2), cor);
    let lamina = |x: f32, y: f32| {
        draw_triangle(
            p(x - 0.55, y + 0.5),
            p(x + 0.55, y - 0.65),
            p(x + 0.12, y + 0.1),
            cor,
        );
        linha(p(x - 0.48, y + 0.26), p(x - 0.15, y + 0.6), 0.10);
        linha(p(x - 0.4, y + 0.48), p(x - 0.6, y + 0.7), 0.12);
    };
    match id {
        1 => {
            lamina(0.05, -0.02);
            for y in [-0.3, 0.0, 0.3] {
                linha(p(-0.82, y), p(-0.4, y - 0.13), 0.065);
            }
        }
        2 | 4 => {
            arco(c, r * 0.85, -PI * 0.85, 0.48, 0.10 * r, cor);
            lamina(0.0, 0.0);
            if id == 2 {
                arco(c, r * 0.65, -PI * 0.85, 0.40, 0.04 * r, cor);
            }
        }
        3 => {
            let ps = [
                p(-0.62, -0.5),
                p(0.0, -0.78),
                p(0.62, -0.5),
                p(0.48, 0.3),
                p(0.0, 0.78),
                p(-0.48, 0.3),
            ];
            for i in 0..6 {
                linha(ps[i], ps[(i + 1) % 6], 0.10);
            }
            linha(p(0.0, -0.4), p(0.0, 0.38), 0.08);
            linha(p(-0.28, -0.12), p(0.28, -0.12), 0.08);
        }
        5 => {
            for i in 0..3 {
                arco(
                    c,
                    r * (0.48 + i as f32 * 0.17),
                    i as f32 * 2.1,
                    0.57,
                    0.1 * r,
                    cor,
                );
            }
        }
        6 => {
            for x in [-0.28, 0.10, 0.46] {
                linha(p(x - 0.4, -0.7), p(x + 0.12, 0.0), 0.12);
                linha(p(x + 0.12, 0.0), p(x - 0.4, 0.7), 0.09);
            }
        }
        7 => {
            arco(c, r * 0.50, 0.0, 1.0, 0.065 * r, cor);
            for i in 0..4 {
                let d = vec2((i as f32 * PI / 2.0).cos(), (i as f32 * PI / 2.0).sin());
                linha(c + d * r * 0.35, c + d * r * 0.85, 0.09);
            }
            draw_circle(c.x, c.y, r * 0.11, cor);
        }
        8 => {
            for y in [-0.4, 0.0, 0.4] {
                linha(p(-0.75, y), p(0.35, y), 0.12);
                draw_triangle(p(0.32, y - 0.14), p(0.65, y), p(0.32, y + 0.14), cor);
            }
        }
        9 => {
            draw_circle_lines(c.x, c.y + r * 0.12, r * 0.52, r * 0.10, cor);
            linha(p(0.0, -0.4), p(0.18, -0.7), 0.10);
            for i in 0..5 {
                let a = i as f32 * TAU / 5.0;
                let q = p(0.25, -0.73);
                linha(
                    q + vec2(a.cos(), a.sin()) * r * 0.1,
                    q + vec2(a.cos(), a.sin()) * r * 0.25,
                    0.05,
                );
            }
        }
        10 | 11 => {
            linha(p(-0.4, 0.0), p(0.4, 0.0), 0.23);
            linha(p(0.0, -0.4), p(0.0, 0.4), 0.23);
            arco(c, r * 0.72, 0.0, 1.0, 0.065 * r, cor);
            if id == 11 {
                arco(c, r * 0.93, 0.2, 0.38, 0.06 * r, cor);
                arco(c, r * 0.93, PI + 0.2, 0.38, 0.06 * r, cor);
            }
        }
        _ => {
            draw_triangle(p(0.25, -0.85), p(-0.48, 0.12), p(0.14, 0.12), cor);
            draw_triangle(p(-0.14, -0.12), p(0.48, -0.12), p(-0.25, 0.85), cor);
            arco(c, r * 0.83, 0.0, 0.33, 0.045 * r, cor);
            arco(c, r * 0.83, PI, 0.33, 0.045 * r, cor);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A cena atras da UI: chao escuro de grama/sombra. Se passa aqui passa em
    /// fundo mais escuro; em fundo claro o painel e' quase opaco.
    const CENA: Color = Color::new(0.25, 0.30, 0.22, 1.0);

    #[test]
    fn texto_contrasta_com_o_painel() {
        assert!(
            contraste(TEXTO, FUNDO, CENA) >= 7.0,
            "texto: {}",
            contraste(TEXTO, FUNDO, CENA)
        );
        assert!(
            contraste(SUAVE, FUNDO, CENA) >= 4.5,
            "suave: {}",
            contraste(SUAVE, FUNDO, CENA)
        );
        assert!(
            contraste(OURO, FUNDO, CENA) >= 4.5,
            "ouro: {}",
            contraste(OURO, FUNDO, CENA)
        );
        assert!(
            contraste(ACENTO, FUNDO, CENA) >= 4.5,
            "acento: {}",
            contraste(ACENTO, FUNDO, CENA)
        );
        assert!(contraste(TEXTO, FUNDO_ALTO, CENA) >= 7.0);
    }

    #[test]
    fn contorno_arredondado_fica_dentro_e_fecha() {
        let r = Rect::new(10.0, 20.0, 120.0, 40.0);
        for lados in [2, 5, 8] {
            let pts = contorno_arredondado(r, 9.0, lados);
            assert_eq!(pts.len(), 4 * (lados + 1));
            for p in &pts {
                assert!(
                    p.x >= r.x - 1e-3
                        && p.x <= r.x + r.w + 1e-3
                        && p.y >= r.y - 1e-3
                        && p.y <= r.y + r.h + 1e-3,
                    "{p:?} fora"
                );
            }
        }
        // Raio maior que meia altura vira pilula, sem ponto pra fora.
        let pilula = contorno_arredondado(Rect::new(0.0, 0.0, 100.0, 10.0), 50.0, 6);
        assert!(pilula.iter().all(|p| p.y >= -1e-3 && p.y <= 10.001));
        // Raio zero: os pontos caem nos quatro cantos.
        let reto = contorno_arredondado(Rect::new(0.0, 0.0, 10.0, 10.0), 0.0, 2);
        assert!(reto
            .iter()
            .all(|p| (p.x == 0.0 || p.x == 10.0) && (p.y == 0.0 || p.y == 10.0)));
    }

    #[test]
    fn estado_do_botao_respeita_a_prioridade() {
        assert_eq!(estado(false, false, false, false), Estado::Normal);
        assert_eq!(estado(true, false, false, false), Estado::Sobre);
        assert_eq!(estado(true, true, false, false), Estado::Pressionado);
        assert_eq!(
            estado(false, true, false, false),
            Estado::Normal,
            "apertar fora nao pressiona"
        );
        assert_eq!(estado(false, false, false, true), Estado::Ativo);
        assert_eq!(estado(true, true, false, true), Estado::Pressionado);
        assert_eq!(estado(true, true, true, true), Estado::Desabilitado);
    }

    #[test]
    fn anima_chega_no_alvo_em_pouco_tempo() {
        let k = (9999, 9999, 9);
        assert_eq!(passo_anima(k, 0.0, 0.016), 0.0, "comeca onde nasceu");
        let mut v = 0.0;
        for _ in 0..12 {
            v = passo_anima(k, 1.0, 1.0 / 60.0);
        }
        assert!(v > 0.9, "em 200 ms quase chegou: {v}");
    }

    #[test]
    fn misturar_e_clarear() {
        let c = Color::new(0.5, 0.5, 0.5, 0.7);
        assert!((clarear(c, 1.0).r - 1.0).abs() < 1e-5 && (clarear(c, -1.0).r).abs() < 1e-5);
        assert!((clarear(c, 0.5).a - 0.7).abs() < 1e-5, "alfa preservado");
    }
}
