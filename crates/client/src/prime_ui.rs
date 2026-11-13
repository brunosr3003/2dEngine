//! The PRIME MERCHANT's board (`shared::prime`): a small stock exchange.
//!
//! Each row: icon, name, today's purchases against the daily limit, the
//! current price, the change since the previous update (green up / red down
//! with %), and a sparkline of the last updates. On top, how long ago the
//! prices moved and when they move next. At the bottom (anchored, so a long
//! list can never push it off the window), the picked item, the quantity by
//! touch and the Buy button.
//!
//! The server decides price, limit and gold; this only draws its board and
//! sends back the price it showed (`preco_visto`).

#[allow(unused_imports)]
use crate::ui::Toque as _;
use std::collections::HashMap;

use macroquad::prelude::*;
use shared::prime::{AvisoPrime, LinhaPrime, PedidoPrime, QuadroPrime};

use crate::economia::milhar;
use crate::hud_estilo as estilo;

/// The window at 100%, in `fator_texto` units.
const LARGURA: f32 = 760.0;
const ALTURA: f32 = 600.0;
/// Row height: a finger's target on a phone.
const LINHA: f32 = 58.0;

pub struct Contexto<'a> {
    pub nomes: &'a HashMap<u16, String>,
    pub ouro: u64,
    pub vox: &'a crate::vox::VoxCache,
    pub solido: &'a macroquad::material::Material,
}

#[derive(Default)]
pub struct PrimeUi {
    pub aberto: bool,
    quadro: Option<QuadroPrime>,
    /// `get_time()` when the board arrived (the countdowns run from it).
    recebido_em: f64,
    selecionado: Option<u16>,
    qtd: u64,
    rolagem: usize,
    /// (text, ok, when)
    aviso: Option<(String, bool, f64)>,
    /// Last automatic refresh asked (at most one every few seconds).
    ultimo_pedido: f64,
}

impl PrimeUi {
    pub fn fechar(&mut self) {
        self.aberto = false;
    }

    /// A message from the server. Returns a chat line, if any.
    pub fn recebe(&mut self, aviso: AvisoPrime, agora: f64, nomes: &HashMap<u16, String>) -> Option<String> {
        match aviso {
            AvisoPrime::Quadro(q) => {
                if self.selecionado.is_none() {
                    self.selecionado = q.linhas.first().map(|l| l.item_id);
                    self.qtd = 1;
                }
                self.quadro = Some(q);
                self.recebido_em = agora;
                self.aberto = true;
                None
            }
            AvisoPrime::Recusa(t) => {
                self.aviso = Some((t.clone(), false, agora));
                (!self.aberto).then_some(t)
            }
            AvisoPrime::Comprou { item_id, qtd, total } => {
                let t = format!("Bought {}× {} for {} gold.", milhar(qtd as u64), nome(item_id, nomes), milhar(total));
                self.aviso = Some((t.clone(), true, agora));
                Some(t)
            }
        }
    }

    /// Seconds since the prices moved, and until they move again (server
    /// clock, advanced by the time since the board arrived).
    fn relogio(&self, agora: f64) -> Option<(i64, i64)> {
        let q = self.quadro.as_ref()?;
        if q.atualizado <= 0 {
            return None;
        }
        let server_agora = q.agora + (agora - self.recebido_em) as i64;
        Some(((server_agora - q.atualizado).max(0), q.proxima - server_agora))
    }

    /// While open: ask for the board again a little after the next update.
    pub fn passo(&mut self, agora: f64) -> Option<PedidoPrime> {
        if !self.aberto {
            return None;
        }
        let (_, falta) = self.relogio(agora).unwrap_or((0, -60));
        if falta < -3 && agora - self.ultimo_pedido > 15.0 {
            self.ultimo_pedido = agora;
            return Some(PedidoPrime::Abrir);
        }
        None
    }

    pub fn desenha(&mut self, c: &Contexto, agora: f64) -> Vec<PedidoPrime> {
        if !self.aberto {
            return Vec::new();
        }
        estilo::no_painel(estilo::escala_do_painel(LARGURA, ALTURA), || self.desenha_na_escala(c, agora))
    }

    fn desenha_na_escala(&mut self, c: &Contexto, agora: f64) -> Vec<PedidoPrime> {
        let mut saida = Vec::new();
        let f = estilo::fator_texto();
        let seguro = crate::hud_layout::tela_segura();
        let (w, h) = ((LARGURA * f).min(seguro.w - 16.0), (ALTURA * f).min(seguro.h - 16.0));
        let p = Rect::new(seguro.center().x - w * 0.5, seguro.center().y - h * 0.5, w, h);
        crate::hud_layout::escurece(0.55);
        estilo::painel(p);
        let x = p.x + 18.0 * f;
        let larg = p.w - 36.0 * f;

        // ── title, balance, close ──
        estilo::texto_forte(x, p.y + 34.0 * f, "Prime Exchange", 22, estilo::OURO);
        let fechar = Rect::new(p.x + p.w - 48.0 * f, p.y + 10.0 * f, 38.0 * f, 34.0 * f);
        let txt_ouro = milhar(c.ouro);
        let x_ouro = fechar.x - 16.0 * f - estilo::largura_ouro_texto(&txt_ouro, 15, true);
        estilo::ouro_texto(x_ouro, p.y + 32.0 * f, &txt_ouro, 15, estilo::OURO, true);
        if crate::ui::botao(fechar, "x", true) {
            self.fechar();
            return saida;
        }
        let relogio = match self.relogio(agora) {
            Some((ha, falta)) if falta > 0 => {
                format!("Updated {} ago · next update in {}", duracao(ha), duracao(falta))
            }
            Some((ha, _)) => format!("Updated {} ago · updating…", duracao(ha)),
            None => "Waiting for the first market update…".into(),
        };
        estilo::texto_ajustado(
            &format!("Market price +{}% · gold only · {relogio}", shared::prime::MARKUP_PCT),
            x,
            p.y + 58.0 * f,
            larg,
            12,
            estilo::SUAVE,
        );

        // ── the footer is anchored to the bottom and drawn from there ──
        let rod_h = 132.0 * f;
        let rod = Rect::new(x, p.y + p.h - rod_h - 12.0 * f, larg, rod_h);
        let lista = Rect::new(x, p.y + 72.0 * f, larg, rod.y - 8.0 * f - (p.y + 72.0 * f));

        let linhas: Vec<LinhaPrime> = self.quadro.as_ref().map(|q| q.linhas.clone()).unwrap_or_default();
        if linhas.is_empty() {
            estilo::texto_ajustado(
                "The Prime Merchant is waiting for the market's prices. Come back in a few minutes.",
                x,
                lista.y + 30.0 * f,
                larg,
                14,
                estilo::TEXTO,
            );
        }
        // ── the list ──
        let alto = LINHA * f;
        let cabem = ((lista.h / (alto + 4.0 * f)).floor() as usize).max(1);
        let (_, roda) = mouse_wheel();
        if roda != 0.0 && lista.toca(Vec2::from(mouse_position())) {
            self.rolagem = if roda < 0.0 { self.rolagem + 1 } else { self.rolagem.saturating_sub(1) };
        }
        self.rolagem = self.rolagem.min(linhas.len().saturating_sub(cabem));
        let estreito = larg < 560.0 * f;
        for (k, l) in linhas.iter().skip(self.rolagem).take(cabem).enumerate() {
            let r = Rect::new(lista.x, lista.y + k as f32 * (alto + 4.0 * f), lista.w - 30.0 * f, alto);
            let ativo = self.selecionado == Some(l.item_id);
            let m = Vec2::from(mouse_position());
            let sobre = r.toca(m);
            estilo::cartao(r, sobre, ativo);
            if sobre && crate::foco::clique() && !ativo {
                self.selecionado = Some(l.item_id);
                self.qtd = 1;
            }
            linha(r, l, c, f, estreito);
        }
        // Scroll arrows down the right edge (a phone has no wheel).
        if linhas.len() > cabem {
            let bx = lista.x + lista.w - 26.0 * f;
            let cima = Rect::new(bx, lista.y, 26.0 * f, 44.0 * f);
            let baixo = Rect::new(bx, lista.y + lista.h - 44.0 * f, 26.0 * f, 44.0 * f);
            if crate::ui::botao(cima, "^", self.rolagem > 0) {
                self.rolagem = self.rolagem.saturating_sub(cabem.max(2) - 1);
            }
            if crate::ui::botao(baixo, "v", self.rolagem + cabem < linhas.len()) {
                self.rolagem += cabem.max(2) - 1;
            }
            let trilho = Rect::new(bx + 9.0 * f, cima.y + cima.h + 4.0 * f, 8.0 * f, baixo.y - cima.y - cima.h - 8.0 * f);
            estilo::scroll(trilho, self.rolagem as f32 / linhas.len() as f32, cabem as f32 / linhas.len() as f32);
        }

        // ── footer: the picked item, quantity, Buy ──
        estilo::ret_arredondado(rod, estilo::RAIO, estilo::FUNDO_BAIXO);
        let sel = self.selecionado.and_then(|id| linhas.iter().find(|l| l.item_id == id));
        if let Some(l) = sel {
            let resta = l.resta_hoje() as u64;
            let por_ouro = c.ouro / l.preco.max(1);
            let max = resta.min(por_ouro).min(shared::prime::QTD_MAX_POR_COMPRA as u64);
            self.qtd = self.qtd.clamp(1, max.max(1));
            estilo::texto_ajustado(
                &format!("{} · {} gold each · {} left today", nome(l.item_id, c.nomes), milhar(l.preco), milhar(resta)),
                rod.x + 12.0 * f,
                rod.y + 24.0 * f,
                rod.w - 24.0 * f,
                14,
                estilo::TEXTO,
            );
            let comprar_w = (200.0 * f).min(rod.w * 0.42);
            let sel_r = Rect::new(rod.x + 12.0 * f, rod.y + 58.0 * f, rod.w - comprar_w - 36.0 * f, 46.0 * f);
            seletor(sel_r, &mut self.qtd, 1, max.max(1), f);
            let total = l.preco.saturating_mul(self.qtd);
            let pode = max > 0 && total <= c.ouro;
            let b = Rect::new(rod.x + rod.w - comprar_w - 12.0 * f, sel_r.y, comprar_w, 46.0 * f);
            let rotulo = format!("Buy · {} gold", milhar(total));
            if crate::ui::botao(b, &rotulo, pode) && pode {
                saida.push(PedidoPrime::Comprar { item_id: l.item_id, qtd: self.qtd as u32, preco_visto: l.preco });
            }
            let motivo = if resta == 0 {
                Some("Daily limit reached. Come back tomorrow.")
            } else if por_ouro == 0 {
                Some("Not enough gold.")
            } else {
                None
            };
            if let Some(m) = motivo {
                estilo::texto(rod.x + 12.0 * f, rod.y + rod.h - 10.0 * f, m, 12, estilo::VERMELHO);
            }
        }
        if let Some((texto, ok, quando)) = &self.aviso {
            if agora - quando < 6.0 {
                let cor = if *ok { estilo::VERDE } else { estilo::VERMELHO };
                estilo::texto_ajustado(texto, rod.x + 12.0 * f, rod.y + 44.0 * f, rod.w - 24.0 * f, 12, cor);
            }
        }
        saida
    }
}

fn nome(id: u16, nomes: &HashMap<u16, String>) -> String {
    if shared::prime::e_energia(id) {
        return "Energy ×1,000".into();
    }
    nomes.get(&id).cloned().unwrap_or_else(|| format!("item {id}"))
}

/// "45s", "4m 10s", "1h 5m".
fn duracao(s: i64) -> String {
    let s = s.max(0);
    if s < 60 {
        format!("{s}s")
    } else if s < 3_600 {
        format!("{}m {}s", s / 60, s % 60)
    } else {
        format!("{}h {}m", s / 3_600, (s % 3_600) / 60)
    }
}

/// One row: icon, name and today's count, sparkline, change, price.
fn linha(r: Rect, l: &LinhaPrime, c: &Contexto, f: f32, estreito: bool) {
    let icone = Rect::new(r.x + 6.0 * f, r.y + 5.0 * f, r.h - 10.0 * f, r.h - 10.0 * f);
    if shared::prime::e_energia(l.item_id) {
        estilo::icone_energia(icone.center(), icone.w * 0.9);
    } else {
        crate::icones::icone_com_3d(l.item_id, icone, shared::item_id::cor_de_id(l.item_id), None, Some((c.vox, c.solido)));
    }
    let x = icone.x + icone.w + 10.0 * f;
    // Right to left: price, change, sparkline.
    let preco = format!("{} gold", milhar(l.preco));
    let preco_w = estilo::medir_forte(&preco, 16).max(90.0 * f);
    let xp = r.x + r.w - 12.0 * f - preco_w;
    estilo::texto_forte(xp + preco_w - estilo::medir_forte(&preco, 16), r.y + 26.0 * f, &preco, 16, estilo::OURO);
    variacao(Rect::new(xp, r.y + 34.0 * f, preco_w, 16.0 * f), l, f);
    let spark_w = if estreito { 70.0 * f } else { 120.0 * f };
    let spark = Rect::new(xp - 14.0 * f - spark_w, r.y + 10.0 * f, spark_w, r.h - 20.0 * f);
    sparkline(spark, &l.historico, f);
    let largura_nome = (spark.x - x - 8.0 * f).max(50.0 * f);
    estilo::texto_ajustado(&nome(l.item_id, c.nomes), x, r.y + 24.0 * f, largura_nome, 15, estilo::TEXTO);
    let hoje = format!("Today {}/{}", milhar(l.comprado_hoje as u64), milhar(l.limite_dia as u64));
    let cor = if l.resta_hoje() == 0 { estilo::VERMELHO } else { estilo::SUAVE };
    estilo::texto_ajustado(&hoje, x, r.y + 44.0 * f, largura_nome, 12, cor);
}

/// A green up / red down triangle with the % change, right-aligned in `r`.
fn variacao(r: Rect, l: &LinhaPrime, f: f32) {
    let Some(pct) = l.variacao_pct() else {
        let t = "new";
        estilo::texto(r.x + r.w - estilo::medir(t, 12), r.y + 12.0 * f, t, 12, estilo::SUAVE);
        return;
    };
    let (cor, sobe) = if pct > 0.005 {
        (estilo::VERDE, Some(true))
    } else if pct < -0.005 {
        (estilo::VERMELHO, Some(false))
    } else {
        (estilo::SUAVE, None)
    };
    let t = format!("{:.1}%", pct.abs());
    let tw = estilo::medir(&t, 12);
    let tx = r.x + r.w - tw;
    estilo::texto(tx, r.y + 12.0 * f, &t, 12, cor);
    let c = vec2(tx - 9.0 * f, r.y + 7.0 * f);
    let s = 5.0 * f;
    match sobe {
        Some(true) => draw_triangle(vec2(c.x, c.y - s), vec2(c.x - s, c.y + s * 0.8), vec2(c.x + s, c.y + s * 0.8), cor),
        Some(false) => draw_triangle(vec2(c.x, c.y + s), vec2(c.x - s, c.y - s * 0.8), vec2(c.x + s, c.y - s * 0.8), cor),
        None => draw_rectangle(c.x - s, c.y - 1.0 * f, 2.0 * s, 2.0 * f, cor),
    }
}

/// The last price points as a line, scaled to their own min..max.
fn sparkline(r: Rect, pontos: &[u64], f: f32) {
    estilo::ret_arredondado(r, estilo::RAIO_PEQUENO, estilo::alfa(estilo::FUNDO_BAIXO, 0.6));
    if pontos.len() < 2 {
        draw_line(r.x + 4.0 * f, r.center().y, r.x + r.w - 4.0 * f, r.center().y, 1.5 * f, estilo::alfa(estilo::SUAVE, 0.6));
        return;
    }
    let (lo, hi) = pontos.iter().fold((u64::MAX, 0), |(a, b), &v| (a.min(v), b.max(v)));
    let faixa = (hi - lo).max(1) as f32;
    let cor = if pontos.last() >= pontos.first() { estilo::VERDE } else { estilo::VERMELHO };
    let n = pontos.len() - 1;
    let ponto = |i: usize| {
        let px = r.x + 4.0 * f + (r.w - 8.0 * f) * i as f32 / n as f32;
        let py = if hi == lo { r.center().y } else { r.y + r.h - 4.0 * f - (r.h - 8.0 * f) * (pontos[i] - lo) as f32 / faixa };
        vec2(px, py)
    };
    for i in 0..n {
        let (a, b) = (ponto(i), ponto(i + 1));
        draw_line(a.x, a.y, b.x, b.y, 1.6 * f, cor);
    }
    let fim = ponto(n);
    draw_circle(fim.x, fim.y, 2.4 * f, cor);
}

/// Quantity by touch: [Min][−] value [+][Max], 44-pt targets.
fn seletor(r: Rect, valor: &mut u64, min: u64, max: u64, f: f32) {
    let bw = (48.0 * f).min(r.w / 5.5);
    let rects = [
        Rect::new(r.x, r.y, bw, r.h),
        Rect::new(r.x + bw + 4.0 * f, r.y, bw, r.h),
        Rect::new(r.x + r.w - 2.0 * bw - 4.0 * f, r.y, bw, r.h),
        Rect::new(r.x + r.w - bw, r.y, bw, r.h),
    ];
    let rotulos = ["Min", "−", "+", "Max"];
    let v = *valor;
    for (i, b) in rects.iter().enumerate() {
        if crate::ui::botao(*b, rotulos[i], true) {
            *valor = match i {
                0 => min,
                1 => v.saturating_sub(1),
                2 => v.saturating_add(1),
                _ => max,
            };
        }
    }
    *valor = (*valor).clamp(min, max.max(min));
    let meio = Rect::new(rects[1].x + bw + 4.0 * f, r.y, rects[2].x - 4.0 * f - (rects[1].x + bw + 4.0 * f), r.h);
    estilo::ret_arredondado(meio, estilo::RAIO_PEQUENO, estilo::FUNDO);
    estilo::texto_centro_forte(meio.center().x, meio.center().y + 6.0 * f, &milhar(*valor), 16, estilo::TEXTO);
}

#[cfg(test)]
mod testes {
    use super::*;

    fn quadro(atualizado: i64) -> QuadroPrime {
        QuadroPrime {
            linhas: vec![LinhaPrime { item_id: shared::item_id::DARKSTEEL, preco: 12, historico: vec![10, 12], limite_dia: 50_000, comprado_hoje: 0 }],
            agora: 1_000,
            atualizado,
            proxima: atualizado + 300,
        }
    }

    #[test]
    fn quadro_abre_e_relogio_anda() {
        let mut ui = PrimeUi::default();
        ui.recebe(AvisoPrime::Quadro(quadro(900)), 50.0, &HashMap::new());
        assert!(ui.aberto);
        assert_eq!(ui.selecionado, Some(shared::item_id::DARKSTEEL));
        assert_eq!(ui.relogio(60.0), Some((110, 190)));
        assert_eq!(ui.passo(60.0), None);
        // Past the next update: asks for the board again, once.
        assert_eq!(ui.passo(300.0), Some(PedidoPrime::Abrir));
        assert_eq!(ui.passo(301.0), None);
    }

    #[test]
    fn duracoes() {
        assert_eq!(duracao(5), "5s");
        assert_eq!(duracao(250), "4m 10s");
        assert_eq!(duracao(3_900), "1h 5m");
    }
}
