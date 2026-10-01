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
    if args.iter().any(|a| a == "--rotas") {
        rotas(&args);
        return;
    }
    if args.iter().any(|a| a == "--simula") {
        simula(&args);
        return;
    }
    if args.iter().any(|a| a == "--alcance") {
        alcance(&args);
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
        "ilha", "raio", "terra", "plana", "andavel", "parede", "mob site", "boss site", "gerar"
    );
    for d in ARQUIPELAGO.iter() {
        let raio = if cheio {
            d.raio_blocos
        } else {
            d.raio_blocos / 4
        };
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
    let g = shared::terreno::Gerador::da_ilha(d);
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
    println!(
        "{:>6} {:>8} {:>7} {:>8} {:>8} {:>8} {:>10} {:>11}",
        "escala", "terraco", "pico", "plana", "andavel", "parede", "mob site", "boss site"
    );
    for escala in [0.35f32, 0.6, 0.9, 1.2] {
        for passo in [2i32, 4, 6] {
            let ilha = Ilha::com_terraco(d.semente, d.raio_blocos, d.bioma, escala, passo, 0.92);
            let e = ilha.estatisticas();
            let pico = (0..ilha.lado as i32)
                .flat_map(|z| (0..ilha.lado as i32).map(move |x| (x, z)))
                .map(|(x, z)| ilha.bloco(x, z))
                .max()
                .unwrap_or(0);
            println!(
                "{:>6.2} {:>6}bl {:>6.1}un {:>7.1}% {:>7.1}% {:>7.1}% {:>9.1}% {:>10.1}%",
                escala,
                passo,
                (pico + 1) as f32 * BLOCO,
                e.pct_plana(),
                e.pct_andavel(),
                e.pct_parede(),
                e.sitio_mob * 100.0,
                e.sitio_chefe * 100.0
            );
        }
    }
}

/// Da' pra CHEGAR a pe' (andando e pulando, o mesmo A* do servidor) em cada
/// regiao de recurso da ilha inicial, saindo da cidade? E a rota curta do
/// servidor (orcamento de 6.000 nos, alcance 220) a partir de `--de x,z`
/// ate' a regiao de pedra roxa mais perto.
///
/// ```sh
/// cargo run --release --bin terreno -- --alcance --de 44,-39
/// ```
fn alcance(args: &[String]) {
    use shared::terreno::TipoDeEstorvo;
    let d = &ARQUIPELAGO[0];
    let ilha = Ilha::gerar(d.semente, d.raio_blocos, d.bioma, ESCALA_ALTURA);
    let cidade = ilha
        .cidade()
        .map(|c| c.centro())
        .unwrap_or(glam::Vec2::ZERO);
    let corpos: Vec<(glam::Vec2, u8)> = ilha
        .todos_os_estorvos()
        .iter()
        .filter_map(|e| match e.tipo {
            TipoDeEstorvo::Tronco => Some((e.centro, 0)),
            TipoDeEstorvo::Minerio(t) => Some((e.centro, t)),
            TipoDeEstorvo::Energia => Some((e.centro, 5)),
            TipoDeEstorvo::Forracao => None,
        })
        .collect();
    // Mesmo agrupamento do `mapa_ilha::regioes_de_recurso` (o bin nao enxerga
    // o crate do servidor): celula de 48 un por tipo, centro medio.
    struct Regiao {
        tipo: u8,
        centro: [f32; 2],
        raio: f32,
        contagem: usize,
    }
    let mut grupos: std::collections::HashMap<(i32, i32, u8), Vec<glam::Vec2>> = Default::default();
    for (p, t) in &corpos {
        grupos
            .entry(((p.x / 48.0).floor() as i32, (p.y / 48.0).floor() as i32, *t))
            .or_default()
            .push(*p);
    }
    let regioes: Vec<Regiao> = grupos
        .into_iter()
        .filter(|(_, ps)| ps.len() >= 3)
        .map(|((_, _, tipo), ps)| {
            let c = ps.iter().fold(glam::Vec2::ZERO, |a, b| a + *b) / ps.len() as f32;
            let raio = ps
                .iter()
                .map(|p| p.distance(c))
                .fold(0.0f32, f32::max)
                .max(4.0);
            Regiao {
                tipo,
                centro: [c.x, c.y],
                raio,
                contagem: ps.len(),
            }
        })
        .collect();
    let chega = |de: glam::Vec2, para: glam::Vec2, orc: usize| -> f32 {
        ilha.caminho(de, para, orc)
            .and_then(|r| r.last().copied())
            .map_or(f32::MAX, |f| f.distance(para))
    };
    let (mut ok, mut longe) = (0, Vec::new());
    for r in &regioes {
        let c = glam::Vec2::new(r.centro[0], r.centro[1]);
        let falta = chega(cidade, c, 2_000_000);
        if falta <= r.raio + 8.0 {
            ok += 1
        } else {
            longe.push((r.tipo, c, falta, r.contagem))
        }
    }
    println!(
        "{}: {} regions, {} reachable from the city",
        d.zona,
        regioes.len(),
        ok
    );
    for (t, c, falta, n) in &longe {
        println!(
            "  INALCANCAVEL tipo {t} em {:.0},{:.0} ({n} corpos): rota para a {:.0} un",
            c.x, c.y, falta
        );
    }
    if let Some(i) = args.iter().position(|a| a == "--de") {
        let v: Vec<f32> = args[i + 1]
            .split(',')
            .filter_map(|x| x.parse().ok())
            .collect();
        let de = glam::Vec2::new(v[0], v[1]);
        let mut roxas: Vec<_> = regioes
            .iter()
            .filter(|r| r.tipo == 4)
            .map(|r| glam::Vec2::new(r.centro[0], r.centro[1]))
            .collect();
        roxas.sort_by(|a, b| a.distance(de).total_cmp(&b.distance(de)));
        for c in roxas.iter().take(3) {
            println!("de {:.0},{:.0} ate' roxa {:.0},{:.0} ({:.0} un): orcamento 6000 para a {:.0}, ilimitado para a {:.0}",
                de.x, de.y, c.x, c.y, de.distance(*c), chega(de, *c, 6_000), chega(de, *c, 2_000_000));
        }
        println!(
            "height at {:.0},{:.0}: {:.1}",
            de.x,
            de.y,
            ilha.altura(de.x, de.y)
        );
    }
}

/// Anda de `--de x,z` ate' `--para x,z` como o SERVIDOR anda: rota com o
/// orcamento dele, pulo automatico com a altura do pulo em cada instante,
/// rota refeita quando o seguidor trava, a pe' e montado.
///
/// ```sh
/// cargo run --release --bin terreno -- --simula --de 44,-39 --para 57,93
/// ```
/// What happened to one route followed the way the server follows it.
struct Seguida {
    chegou: bool,
    fim: glam::Vec2,
    pulos: u32,
    rotas: u32,
    /// Since when the body has not moved more than one unit.
    parado_desde: f32,
    tempo: f32,
}

/// Follows `de` -> `para` on `ilha` exactly like `world.rs` does: A* from
/// the server, re-planned when stuck, the automatic jump asked to
/// `precisa_pular`, and the jump arc raising the step like the server.
fn seguir(ilha: &Ilha, de: glam::Vec2, para: glam::Vec2, montado: bool) -> Seguida {
    use shared::terreno::{SeguidorDeRota, DEGRAU_BLOCOS, PULO_BLOCOS};
    let dt = 1.0f32 / 30.0;
    let vel = shared::loja::velocidade_de_andar(
        shared::PLAYER_SPEED,
        montado.then_some(shared::loja::VEL_MONTADO),
        1.0,
    );
    let r = shared::ENTITY_RADIUS;
    let mut seg = SeguidorDeRota::nova(Vec::new(), para);
    let mut p = de;
    let (mut agora, mut pulo_ate, mut pronto, mut ultima_rota) = (0.0f32, -1.0f32, 0.0f32, -1.0f32);
    let (mut pulos, mut rotas) = (0, 0);
    let mut preso_desde = (p, 0.0f32);
    while agora < 90.0 {
        if p.distance(para) < 2.0 {
            break;
        }
        // Rota: sem rota ou travada, pede de novo (intervalo do servidor).
        if (seg.vazia() || seg.travado()) && agora - ultima_rota >= 0.2 {
            ultima_rota = agora;
            rotas += 1;
            seg = match ilha.caminho(p, para, 6_000) {
                Some(rt) => SeguidorDeRota::nova(rt, para),
                None => SeguidorDeRota::nova(Vec::new(), para),
            };
        }
        // Same as the server: jumping is not "stuck" (`aguenta`).
        if agora < pronto {
            seg.aguenta();
        }
        let dir = seg.direcao(p).unwrap_or(glam::Vec2::ZERO);
        if dir.length_squared() > 0.01
            && ilha.precisa_pular(p, dir.normalize_or_zero() * vel, dt, r)
            && agora >= pronto
        {
            pulo_ate = agora + shared::PULO_DURACAO;
            pronto = pulo_ate + shared::PULO_ESPERA;
            pulos += 1;
        }
        let degrau = if agora < pulo_ate {
            let t = shared::PULO_DURACAO - (pulo_ate - agora);
            ((shared::altura_do_pulo(t) / BLOCO).floor() as i32).clamp(DEGRAU_BLOCOS, PULO_BLOCOS)
        } else {
            DEGRAU_BLOCOS
        };
        p = ilha.mover_com_degrau(p, dir * vel, dt, r, degrau);
        if p.distance(preso_desde.0) > 1.0 {
            preso_desde = (p, agora);
        }
        agora += dt;
    }
    Seguida { chegou: p.distance(para) < 2.0, fim: p, pulos, rotas, parado_desde: preso_desde.1, tempo: agora }
}

fn simula(args: &[String]) {
    let ponto = |nome: &str| -> glam::Vec2 {
        let i = args.iter().position(|a| a == nome).expect(nome);
        let v: Vec<f32> = args[i + 1]
            .split(',')
            .filter_map(|x| x.parse().ok())
            .collect();
        glam::Vec2::new(v[0], v[1])
    };
    let (de, para) = (ponto("--de"), ponto("--para"));
    let qual = args
        .iter()
        .position(|a| a == "--ilha")
        .and_then(|i| args.get(i + 1))
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or(0);
    let d = &ARQUIPELAGO[qual];
    let ilha = Ilha::gerar(d.semente, d.raio_blocos, d.bioma, ESCALA_ALTURA);
    for montado in [false, true] {
        let s = seguir(&ilha, de, para, montado);
        println!(
            "{}: parou a {:.1} do destino em {:.0},{:.0} (altura {:.1}) depois de {:.1}s, {} pulos, {} rotas; parado desde {:.1}s",
            if montado { "montado" } else { "a pe'  " },
            s.fim.distance(para), s.fim.x, s.fim.y, ilha.altura(s.fim.x, s.fim.y), s.tempo, s.pulos, s.rotas, s.parado_desde
        );
    }
}

/// Random routes on every island, followed like the server follows them.
/// Counts the ones the A* says are reachable (the path ends at the target)
/// but the body never gets to: the "it doesn't jump when it should" report.
///
///   cargo run --release --bin terreno -- --rotas 200
fn rotas(args: &[String]) {
    let n: usize = args
        .iter()
        .position(|a| a == "--rotas")
        .and_then(|i| args.get(i + 1))
        .and_then(|v| v.parse().ok())
        .unwrap_or(200);
    for (qual, d) in ARQUIPELAGO.iter().enumerate() {
        let ilha = Ilha::gerar(d.semente, d.raio_blocos, d.bioma, ESCALA_ALTURA);
        let meio = ilha.lado as f32 * BLOCO * 0.5;
        let mut semente = 0x5EED_u64 ^ qual as u64;
        let mut rnd = || {
            semente = semente.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            ((semente >> 33) as f32) / (u32::MAX >> 1) as f32
        };
        let (mut testadas, mut falhas) = (0, Vec::new());
        let mut tentativas = 0;
        while testadas < n && tentativas < n * 50 {
            tentativas += 1;
            let de = glam::Vec2::new((rnd() * 2.0 - 1.0) * meio * 0.8, (rnd() * 2.0 - 1.0) * meio * 0.8);
            let ang = rnd() * std::f32::consts::TAU;
            let para = de + glam::Vec2::new(ang.cos(), ang.sin()) * (30.0 + rnd() * 90.0);
            if ilha.agua(de.x, de.y) || ilha.agua(para.x, para.y) || ilha.ocupado(de, shared::ENTITY_RADIUS) {
                continue;
            }
            // Only routes the A* finishes: the follower is what is on trial.
            let Some(rt) = ilha.caminho(de, para, 6_000) else { continue };
            if rt.last().map_or(true, |f| f.distance(para) > 2.5) {
                continue;
            }
            testadas += 1;
            for montado in [false, true] {
                let s = seguir(&ilha, de, para, montado);
                if !s.chegou {
                    falhas.push((de, para, montado, s));
                }
            }
        }
        println!("{} ({}): {} rotas, {} falhas (a pe' e montado)", d.nome, d.zona, testadas, falhas.len());
        for (de, para, montado, s) in falhas.iter().take(8) {
            println!(
                "   --ilha {qual} --de {:.1},{:.1} --para {:.1},{:.1} {} -> parou a {:.1} em {:.1},{:.1}, {} pulos, {} rotas, parado desde {:.0}s",
                de.x, de.y, para.x, para.y, if *montado { "montado" } else { "a pe'" },
                s.fim.distance(*para), s.fim.x, s.fim.y, s.pulos, s.rotas, s.parado_desde
            );
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
            if (h + 1) as f32 * BLOCO <= 0.0 {
                continue;
            }
            min = min.min(h);
            max = max.max(h);
            soma += h as i64;
            n += 1;
            faixas[((h / 4).clamp(0, 11)) as usize] += 1;
        }
    }
    if n == 0 {
        return;
    }
    println!(
        "   altura da terra: {}..{} blocos ({:.1}..{:.1} un), media {:.1}",
        min,
        max,
        (min + 1) as f32 * BLOCO,
        (max + 1) as f32 * BLOCO,
        soma as f32 / n as f32
    );
    print!("   ");
    for (i, c) in faixas.iter().enumerate() {
        if *c == 0 {
            continue;
        }
        print!(
            "{}-{}bl:{:.0}%  ",
            i * 4,
            i * 4 + 3,
            *c as f32 * 100.0 / n as f32
        );
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
