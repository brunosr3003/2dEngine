//! The performance test's screens (`benchmark`): the progress panel over the
//! scenes being measured, the goal choice, and the in-game "lower graphics?"
//! offer of the runtime guard.

#[allow(unused_imports)]
use crate::ui::Toque as _;
use macroquad::prelude::*;

use crate::benchmark::{self as bm, Objetivo, Resultado, Varredura};
use crate::config_graficos as cg;
use crate::hud_estilo as estilo;
use crate::hud_layout;

/// The choice screen's state.
pub struct EscolhaUi {
    pub varredura: Varredura,
    pub objetivo: Objetivo,
    pub recomendado: Objetivo,
    pub alvo: u16,
}

impl EscolhaUi {
    pub fn nova(varredura: Varredura, mobile: bool) -> Self {
        let alvo = bm::alvo_sugerido(&varredura, mobile);
        let recomendado = bm::recomendado(&varredura, alvo);
        Self { varredura, objetivo: recomendado, recomendado, alvo }
    }

    fn resultado(&self) -> Resultado {
        let d = bm::decide(&self.varredura, self.alvo, self.objetivo);
        let fps = self.varredura.medida_para(d.nivel).map_or(0, |(_, m)| m.fps().round() as u16);
        Resultado { nivel: d.nivel, alvo: self.alvo, objetivo: self.objetivo, pulado: false, fps }
    }
}

fn clicou(r: Rect) -> bool {
    crate::foco::clique() && r.toca(Vec2::from(mouse_position()))
}

/// The bottom panel while the scenes run. `true` = Skip.
pub fn desenha_progresso(fracao: f32, rotulo: &str) -> bool {
    set_default_camera();
    let f = estilo::fator_texto();
    let s = hud_layout::tela_segura();
    let w = (560.0 * f).min(s.w - 32.0);
    let h = 150.0 * f;
    let r = Rect::new(s.center().x - w * 0.5, s.y + s.h - h - 14.0 * f, w, h);
    estilo::painel(r);
    estilo::texto(r.x + 18.0 * f, r.y + 32.0 * f, "Checking your device…", 20, estilo::OURO);
    estilo::texto(r.x + 18.0 * f, r.y + 56.0 * f, rotulo, 14, estilo::SUAVE);
    let barra = Rect::new(r.x + 18.0 * f, r.y + 70.0 * f, r.w - 36.0 * f, 12.0 * f);
    estilo::barra(barra, fracao, 0.0, estilo::OURO, None);
    estilo::texto(
        r.x + 18.0 * f,
        r.y + 112.0 * f,
        "This runs once and takes about 25 seconds.",
        12,
        estilo::SUAVE,
    );
    let pular = Rect::new(r.x + r.w - 132.0 * f, r.y + h - 56.0 * f, 114.0 * f, 42.0 * f);
    crate::ui::botao(pular, "Skip", true)
}

/// The goal choice. `Some` when the player taps Apply.
pub fn desenha_escolha(e: &mut EscolhaUi) -> Option<Resultado> {
    set_default_camera();
    crate::ui::fundo();
    let f = estilo::fator_texto();
    let s = hud_layout::tela_segura();
    let w = (760.0 * f).min(s.w - 24.0);
    let h = (380.0 * f).min(s.h - 12.0);
    let r = Rect::new(s.center().x - w * 0.5, s.center().y - h * 0.5, w, h);
    estilo::painel(r);
    let x = r.x + 20.0 * f;
    let largura = r.w - 40.0 * f;
    let mut y = r.y + 36.0 * f;
    estilo::texto(x, y, "Graphics for your device", 22, estilo::OURO);
    y += 28.0 * f;
    let res = e.resultado();
    let medida = e.varredura.medida_para(res.nivel);
    if let Some((nivel, m)) = medida {
        let linha = format!(
            "Your device: about {fps} fps on {nivel}",
            fps = m.fps().round() as u32,
            nivel = shared::idioma::tr(cg::PRESETS[nivel])
        );
        estilo::texto(x, y, &linha, 15, estilo::TEXTO);
    }
    y += 16.0 * f;
    // The three goals.
    let vao = 10.0 * f;
    let cw = (largura - vao * 2.0) / 3.0;
    let ch = 92.0 * f;
    let m = Vec2::from(mouse_position());
    for (k, o) in Objetivo::TODOS.into_iter().enumerate() {
        let c = Rect::new(x + k as f32 * (cw + vao), y, cw, ch);
        let marcado = e.objetivo == o;
        estilo::cartao(c, c.toca(m), marcado);
        estilo::texto_centro(c.center().x, c.y + 34.0 * f, o.nome(), 18, if marcado { estilo::OURO } else { estilo::TEXTO });
        estilo::texto_centro(c.center().x, c.y + 58.0 * f, o.dica(), 12, estilo::SUAVE);
        if o == e.recomendado {
            estilo::texto_centro(c.center().x, c.y + 80.0 * f, "Recommended", 11, estilo::VERDE);
        }
        if clicou(c) {
            e.objetivo = o;
        }
    }
    y += ch + 16.0 * f;
    // The target.
    estilo::texto(x, y + 26.0 * f, "Target", 15, estilo::SUAVE);
    let bw = 110.0 * f;
    for (k, alvo) in [30u16, 60].into_iter().enumerate() {
        let b = Rect::new(x + 90.0 * f + k as f32 * (bw + vao), y, bw, 42.0 * f);
        let marcado = e.alvo == alvo;
        estilo::cartao(b, b.toca(m), marcado);
        estilo::texto_centro(b.center().x, b.center().y + 6.0 * f, &format!("{alvo} fps"), 15, if marcado { estilo::OURO } else { estilo::TEXTO });
        if clicou(b) {
            e.alvo = alvo;
        }
    }
    y += 42.0 * f + 30.0 * f;
    // What will be applied.
    let d = bm::decide(&e.varredura, e.alvo, e.objetivo);
    let nome = shared::idioma::tr(cg::PRESETS[d.nivel]);
    let (texto, cor) = if d.alcanca {
        (format!("We will use {nome} at {alvo} fps.", alvo = e.alvo), estilo::TEXTO)
    } else if e.alvo == 60 {
        ("Your device can't hold 60 fps even on Low. Try 30 fps.".to_string(), estilo::VERMELHO)
    } else {
        ("Your device can't hold 30 fps even on Low, so we will use the lightest settings.".to_string(), estilo::VERMELHO)
    };
    estilo::texto_ajustado(&texto, x, y, largura, 14, cor);
    estilo::texto(x, r.y + r.h - 26.0 * f, "You can change this any time in Menu › System › Graphics.", 11, estilo::SUAVE);
    let aplicar = Rect::new(r.x + r.w - 160.0 * f, r.y + r.h - 62.0 * f, 140.0 * f, 46.0 * f);
    crate::ui::botao(aplicar, "Apply", true).then(|| e.resultado())
}

/// The runtime guard's offer: a small card near the top, nothing blocked.
/// `Some(true)` = lower, `Some(false)` = not now.
pub fn desenha_guarda(g: &mut bm::Guarda) -> Option<bool> {
    let f = estilo::fator_texto();
    let s = hud_layout::tela_segura();
    let w = (440.0 * f).min(s.w - 32.0);
    let h = 96.0 * f;
    let r = Rect::new(s.center().x - w * 0.5, s.y + s.h * 0.16, w, h);
    g.area = Some(r);
    estilo::painel(r);
    estilo::texto_ajustado(
        "The game is running slowly. Lower the graphics one step?",
        r.x + 14.0 * f,
        r.y + 26.0 * f,
        r.w - 28.0 * f,
        14,
        estilo::TEXTO,
    );
    let bw = 130.0 * f;
    let baixar = Rect::new(r.x + r.w - bw - 14.0 * f, r.y + h - 50.0 * f, bw, 40.0 * f);
    let agora_nao = Rect::new(baixar.x - bw - 10.0 * f, baixar.y, bw, 40.0 * f);
    if crate::ui::botao(baixar, "Lower", true) {
        return Some(true);
    }
    if crate::ui::botao(agora_nao, "Not now", true) {
        return Some(false);
    }
    None
}

/// `MMO_PREVIA_BENCHMARK=1`: runs the REAL test on this machine (nothing is
/// applied or saved), capturing the progress panel over each scene, the
/// choice screen with what was measured and two made-up devices, and the
/// in-game offer, into `MMO_PREVIA_SAIDA` (default `target/benchmark_shots`).
#[cfg(debug_assertions)]
pub async fn previa(vox: &mut crate::vox::VoxCache) {
    let saida = std::env::var("MMO_PREVIA_SAIDA").unwrap_or_else(|_| "target/benchmark_shots".into());
    std::fs::create_dir_all(&saida).unwrap();
    for _ in 0..30 {
        while vox.atende_um_pendente(crate::render3d::VOXEL).await {}
        clear_background(BLACK);
        next_frame().await;
    }
    let solido = crate::render3d::material_solido();
    // Never `aplica`: only the renderer's statics are touched, and put back.
    let cfg = cg::ConfigGraficos::default();
    let mut b = bm::Benchmark::default();
    b.iniciar();
    let mut tiradas = std::collections::HashSet::new();
    let inicio = get_time();
    loop {
        clear_background(BLACK);
        let chave;
        if let bm::Fase::Medindo(t) = &mut b.fase {
            let acabou = t.passo(vox, &solido);
            desenha_progresso(t.progresso(), &t.rotulo());
            chave = t.chave_da_foto();
            if acabou {
                cfg.republica();
                let v = std::mem::take(&mut t.varredura);
                println!("[benchmark] {:.1} s", get_time() - inicio);
                for (i, n) in v.niveis.iter().enumerate() {
                    if let Some(ms) = n {
                        for (c, m) in ms.iter().enumerate() {
                            println!(
                                "[benchmark] {} / {}: avg {:.1} ms ({:.0} fps), p95 {:.1} ms, cpu p95 {:.1} ms",
                                cg::PRESETS[i], bm::NOMES_DAS_CENAS[c], m.media_ms, m.fps(), m.p95_ms, m.cpu_p95_ms
                            );
                        }
                    }
                }
                b.fase = bm::Fase::Escolha(EscolhaUi::nova(v, bm::MOBILE));
            }
        } else {
            break;
        }
        if let Some(k) = chave {
            if tiradas.insert(k.clone()) {
                captura(&format!("{saida}/progresso-{k}.png"));
            }
        }
        next_frame().await;
    }
    // The choice: what this machine measured, a strong phone, a weak one.
    let real = match &b.fase {
        bm::Fase::Escolha(e) => e.varredura.clone(),
        _ => Varredura::nova(),
    };
    let rapido = bm::Medida { media_ms: 16.7, p95_ms: 17.2, cpu_p95_ms: 7.0 };
    let a_48 = bm::Medida { media_ms: 20.8, p95_ms: 33.0, cpu_p95_ms: 12.0 };
    let lento = bm::Medida { media_ms: 55.0, p95_ms: 70.0, cpu_p95_ms: 30.0 };
    let forte = Varredura { niveis: vec![None, Some(vec![rapido; 3]), Some(vec![a_48; 3]), None] };
    let fraco = Varredura { niveis: vec![Some(vec![lento; 3]), Some(vec![lento; 3]), Some(vec![lento; 3]), None] };
    for (nome, v, mobile) in [("escolha-medida", real, bm::MOBILE), ("escolha-celular-bom", forte, true), ("escolha-celular-fraco", fraco, true)] {
        let mut e = EscolhaUi::nova(v, mobile);
        for _ in 0..4 {
            clear_background(BLACK);
            let _ = desenha_escolha(&mut e);
            next_frame().await;
        }
        clear_background(BLACK);
        let _ = desenha_escolha(&mut e);
        captura(&format!("{saida}/{nome}.png"));
        next_frame().await;
    }
    // The in-game offer over a plain background.
    let mut g = bm::Guarda::default();
    for k in 0..4 {
        clear_background(Color::new(0.24, 0.33, 0.20, 1.0));
        let _ = desenha_guarda(&mut g);
        if k == 3 {
            captura(&format!("{saida}/guarda.png"));
        }
        next_frame().await;
    }
    println!("[benchmark] shots in {saida}");
}

#[cfg(debug_assertions)]
fn captura(caminho: &str) {
    unsafe { get_internal_gl().flush() };
    // `get_screen_data` already comes top-down here (checked on Linux/X11).
    let mut img = get_screen_data();
    for px in img.bytes.chunks_mut(4) {
        px[3] = 255;
    }
    img.export_png(caminho);
}
