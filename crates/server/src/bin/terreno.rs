//! Mede e desenha o terreno gerado, pra afinar o relevo com numero em vez de
//! opiniao.
//!
//! ```sh
//! cargo run --release --bin terreno              # as quatro ilhas do play test
//! cargo run --release --bin terreno -- --png /tmp/ilhas
//! ```
//!
//! O que interessa num MMO nao e' relevo bonito, e' CHAO: mob, chefe e briga
//! querem area plana. `sitio mob` e `sitio chefe` sao a fracao do mapa que
//! aceita um disco plano de 4 e de 12 unidades — sao esses dois numeros que
//! dizem se o mundo serve.

use shared::terreno::{material, Bioma, Ilha, ARQUIPELAGO, BLOCO, ESCALA_ALTURA};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.iter().any(|a| a == "--varre") {
        varre();
        return;
    }
    if args.iter().any(|a| a == "--pedaco") {
        pedaco();
        return;
    }
    let png = args
        .iter()
        .position(|a| a == "--png")
        .and_then(|i| args.get(i + 1))
        .cloned();
    // Raio reduzido pra iterar rapido; `--cheio` usa o tamanho de verdade.
    let cheio = args.iter().any(|a| a == "--cheio");

    println!(
        "{:<14} {:>7} {:>7} {:>8} {:>8} {:>8} {:>10} {:>11} {:>7}",
        "ilha", "raio", "terra", "plana", "andavel", "parede", "sitio mob", "sitio chefe", "gerar"
    );
    for d in ARQUIPELAGO.iter() {
        let raio = if cheio { d.raio_blocos } else { d.raio_blocos / 4 };
        let t0 = std::time::Instant::now();
        let ilha = Ilha::gerar(d.semente, raio, d.bioma, ESCALA_ALTURA);
        let ms = t0.elapsed().as_millis();
        let e = ilha.estatisticas();
        println!(
            "{:<14} {:>6}m {:>6.1}% {:>7.1}% {:>7.1}% {:>7.1}% {:>9.1}% {:>10.1}% {:>6}ms",
            d.zona,
            (raio as f32 * BLOCO) as i32,
            e.pct_terra(),
            e.pct_plana(),
            e.pct_andavel(),
            e.pct_parede(),
            e.sitio_mob * 100.0,
            e.sitio_chefe * 100.0,
            ms
        );
        histograma(&ilha);
        if let Some(dir) = &png {
            let _ = std::fs::create_dir_all(dir);
            desenha(&ilha, &format!("{dir}/{}.ppm", d.zona), d.bioma);
        }
    }
    if let Some(dir) = &png {
        println!("\nimagens em {dir}/*.ppm");
    }
}

/// Custo de um PEDACO — o numero que decide se andar engasga.
///
/// O cliente gera 34x34 colunas por pedaco (32 mais uma borda de cada lado,
/// que a parede lateral precisa) e desenha alguns por quadro. Se isso nao
/// couber numa fatia de 16 ms, o mundo aparece as solavancos.
fn pedaco() {
    let d = &ARQUIPELAGO[0];
    let g = shared::terreno::Gerador::novo(d.semente, d.raio_blocos, d.bioma, ESCALA_ALTURA);
    let n = 34i32;
    for rodada in 0..3 {
        let t0 = std::time::Instant::now();
        let mut soma = 0i64;
        for cz in 0..8 {
            for cx in 0..8 {
                for iz in 0..n {
                    for ix in 0..n {
                        soma += g.bloco_em(cx * 32 + ix, cz * 32 + iz) as i64;
                    }
                }
            }
        }
        let us = t0.elapsed().as_micros() as f64 / 64.0;
        println!("rodada {rodada}: {us:.0} us por pedaco  ({:.1} pedacos por fatia de 16ms)  [soma {soma}]",
            16000.0 / us);
    }
}

/// Varre o degrau do terraco na ilha inicial. Afinar relevo no olho e' como
/// afinar servidor no olho: o numero que interessa (quanto do mapa aceita um
/// chefe) nao se enxerga na imagem.
fn varre() {
    let d = &ARQUIPELAGO[0];
    println!("{:>6} {:>8} {:>7} {:>8} {:>8} {:>8} {:>10} {:>11}",
        "escala", "terraco", "pico", "plana", "andavel", "parede", "sitio mob", "sitio chefe");
    for escala in [0.35f32, 0.6, 0.9, 1.2] {
        for passo in [2i32, 4, 6] {
            let ilha =
                Ilha::com_terraco(d.semente, d.raio_blocos, d.bioma, escala, passo, 0.92);
            let e = ilha.estatisticas();
            let pico = (0..ilha.lado as i32)
                .flat_map(|z| (0..ilha.lado as i32).map(move |x| (x, z)))
                .map(|(x, z)| ilha.bloco(x, z))
                .max()
                .unwrap_or(0);
            println!("{:>6.2} {:>6}bl {:>6.1}un {:>7.1}% {:>7.1}% {:>7.1}% {:>9.1}% {:>10.1}%",
                escala, passo, (pico + 1) as f32 * BLOCO,
                e.pct_plana(), e.pct_andavel(), e.pct_parede(),
                e.sitio_mob * 100.0, e.sitio_chefe * 100.0);
        }
    }
}

/// Onde a terra realmente esta', em blocos de altura. Media e pico escondem o
/// que importa: se o relevo inteiro cabe em tres blocos, o mundo e' uma mesa.
fn histograma(ilha: &Ilha) {
    let mut faixas = [0usize; 12];
    let (mut min, mut max, mut soma, mut n) = (i32::MAX, i32::MIN, 0i64, 0usize);
    for iz in 0..ilha.lado as i32 {
        for ix in 0..ilha.lado as i32 {
            let h = ilha.bloco(ix, iz);
            if (h + 1) as f32 * BLOCO <= 0.0 { continue }
            min = min.min(h); max = max.max(h); soma += h as i64; n += 1;
            faixas[((h / 4).clamp(0, 11)) as usize] += 1;
        }
    }
    if n == 0 { return }
    println!("   altura da terra: {}..{} blocos ({:.1}..{:.1} un), media {:.1}",
        min, max, (min + 1) as f32 * BLOCO, (max + 1) as f32 * BLOCO,
        soma as f32 / n as f32);
    print!("   ");
    for (i, c) in faixas.iter().enumerate() {
        if *c == 0 { continue }
        print!("{}-{}bl:{:.0}%  ", i * 4, i * 4 + 3, *c as f32 * 100.0 / n as f32);
    }
    println!();
}

/// Mapa de cima: cor por altura, com a linha d'agua marcada. Sombreamento pelo
/// desnivel a oeste — sem ele o relevo vira mancha e nao da' pra ver encosta.
fn desenha(ilha: &Ilha, caminho: &str, bioma: Bioma) {
    let n = ilha.lado.min(1024);
    let passo = ilha.lado as f32 / n as f32;
    let mut buf = Vec::with_capacity(n * n * 3 + 32);
    buf.extend_from_slice(format!("P6\n{n} {n}\n255\n").as_bytes());
    for iy in 0..n {
        for ix in 0..n {
            let cx = (ix as f32 * passo) as i32;
            let cz = (iy as f32 * passo) as i32;
            let h = ilha.bloco(cx, cz);
            let alt = (h + 1) as f32 * BLOCO;
            let oeste = ilha.bloco(cx - 1, cz);
            let luz = ((h - oeste).clamp(-3, 3) as f32) * -16.0;
            // A MESMA funcao que o jogo usa: se o mapa e o mundo pintarem
            // diferente, o mapa deixa de ser evidencia.
            let declive = [(1, 0), (-1, 0), (0, 1), (0, -1)]
                .iter()
                .map(|(dx, dz)| (h - ilha.bloco(cx + dx, cz + dz)).abs())
                .max()
                .unwrap_or(0);
            let (r, g, b) = material(bioma, alt, declive, alt <= 0.0).rgb();
            let (r, g, b) = (r as f32, g as f32, b as f32);
            // Fundo do mar escurece com a profundidade.
            let (r, g, b) = if alt <= 0.0 {
                let f = ((alt + 14.0) / 14.0).clamp(0.15, 1.0);
                (r * f, g * f, b * f)
            } else {
                (r, g, b)
            };
            buf.push((r + luz).clamp(0.0, 255.0) as u8);
            buf.push((g + luz).clamp(0.0, 255.0) as u8);
            buf.push((b + luz).clamp(0.0, 255.0) as u8);
        }
    }
    let _ = std::fs::write(caminho, buf);
}

