//! The COMBINE result, animated (the Craft's Combine and Materials tabs).
//!
//! Owner, 11/11/2026: a cleaner, prettier combine. Before, pressing Combine
//! only printed a line at the bottom of the panel. Now the stage takes the
//! panel for ~2 s:
//!
//! 1. the five (or ten) inputs pop in on a ring and spiral into the middle,
//!    charging an orb in the colour of the tier they are reaching for;
//! 2. SUCCESS: a burst of light and the item(s) that came out scale in over
//!    slow rays, with "N successes" and, when the counter wrapped, a
//!    "Guaranteed success" chip;
//!    FAIL: the orb gutters out and the inputs come back grey, crack and
//!    crumble away — "No luck this time" and how far the guarantee is;
//! 3. material synthesis (always succeeds) is the same gather + success,
//!    shorter.
//!
//! A tap skips to the reveal, a second tap closes. When it ends the panel is
//! back with nothing in the way. Presentation only: the server already rolled.

#[allow(unused_imports)]
use crate::ui::Toque as _;
use std::collections::HashMap;
use std::f32::consts::{PI, TAU};

use macroquad::prelude::*;
use shared::combinar::{self, ReceitaDeCombinar};
use shared::InventorySlot;

use crate::hud_estilo::{self as estilo, u};
use crate::vox::VoxCache;

/// The bag's count of each possible output, taken when Combine was pressed:
/// the server picks a random key/pet/mount of the family and does not say
/// which, so the reveal shows what actually landed in the bag.
#[derive(Debug, Clone, Default)]
pub struct Antes {
    pub entrada: u16,
    pub contagem: Vec<(u16, u32)>,
}

impl Antes {
    pub fn de(r: &ReceitaDeCombinar, slots: &[InventorySlot]) -> Self {
        Self {
            entrada: r.entrada,
            contagem: combinar::saidas_possiveis(r)
                .into_iter()
                .map(|id| (id, crate::craft_ui::tem(slots, id)))
                .collect(),
        }
    }
}

/// Up to this many different outputs are shown (four key families).
const MAX_SAIDAS: usize = 6;

pub struct Animacao {
    r: ReceitaDeCombinar,
    tentativas: u16,
    sucessos: u16,
    /// The 25th-attempt guarantee fired inside this batch.
    garantida: bool,
    /// Attempts left to the next guarantee after this batch (chance recipes).
    faltam_depois: Option<u16>,
    inicio: f64,
    antes: Option<Antes>,
    /// What came out (item, how many), once the bag shows it.
    saidas: [(u16, u32); MAX_SAIDAS],
    n_saidas: usize,
    /// The bag already accounts for every success: stop looking.
    fixas: bool,
    /// The biggest radius that stays inside the stage (this frame).
    lim: f32,
    /// The stage's bottom edge (this frame).
    chao: f32,
}

/// (guarantee fired in the batch, attempts left afterwards) from the count
/// BEFORE the batch (`faltam_garantia`: 1 = the next one was sure).
///
/// The server sends `CombinarResultado` before the new `GarantiasDeCombinar`,
/// so the panel still holds the old counter when the result arrives.
pub fn garantia_do_lote(faltam_antes: u16, tentativas: u16) -> (bool, u16) {
    let g = combinar::GARANTIA;
    let contador = g - faltam_antes.clamp(1, g);
    let disparou = tentativas >= faltam_antes.max(1);
    let depois = ((contador as u32 + tentativas as u32) % g as u32) as u16;
    (disparou, combinar::faltam_para_garantia(depois))
}

// ── timeline (seconds) ──
struct Tempos {
    /// Inputs start spiralling in / reach the middle.
    junta: (f32, f32),
    /// The reveal (burst or fizzle).
    revela: f32,
    /// How long the result stays before fading back to the panel.
    fica: f32,
}
const SAIDA: f32 = 0.30;

impl Animacao {
    pub fn nova(
        r: ReceitaDeCombinar,
        tentativas: u16,
        sucessos: u16,
        faltam_antes: Option<u16>,
        antes: Option<Antes>,
        agora: f64,
    ) -> Self {
        let (garantida, faltam_depois) = match faltam_antes {
            Some(f) if r.chance < 100 => {
                let (g, d) = garantia_do_lote(f, tentativas);
                (g && sucessos > 0, Some(d))
            }
            _ => (false, None),
        };
        let mut a = Self {
            r,
            tentativas,
            sucessos,
            garantida,
            faltam_depois,
            inicio: agora,
            antes: antes.filter(|x| x.entrada == r.entrada),
            saidas: [(0, 0); MAX_SAIDAS],
            n_saidas: 0,
            fixas: false,
            lim: 0.0,
            chao: 0.0,
        };
        a.saidas[0] = (r.saida, sucessos as u32);
        a.n_saidas = usize::from(sucessos > 0);
        a
    }

    fn sintese(&self) -> bool {
        self.r.chance >= 100
    }

    fn ok(&self) -> bool {
        self.sucessos > 0
    }

    fn tempos(&self) -> Tempos {
        if self.sintese() {
            Tempos { junta: (0.22, 0.62), revela: 0.78, fica: 1.05 }
        } else {
            Tempos {
                junta: (0.34, 1.08),
                revela: 1.25,
                fica: if self.ok() { 1.55 } else { 1.35 },
            }
        }
    }

    /// Total length, fade included.
    pub fn duracao(&self) -> f32 {
        let t = self.tempos();
        t.revela + t.fica + SAIDA
    }

    pub fn acabou(&self, agora: f64) -> bool {
        (agora - self.inicio) as f32 >= self.duracao()
    }

    /// Fading back to the panel: the panel underneath is drawn (and live) again.
    pub fn saindo(&self, agora: f64) -> bool {
        let t = self.tempos();
        (agora - self.inicio) as f32 >= t.revela + t.fica
    }

    /// Reads what the bag gained since Combine was pressed.
    fn confere_bolsa(&mut self, slots: &[InventorySlot]) {
        if self.fixas || self.sucessos == 0 {
            return;
        }
        let Some(antes) = &self.antes else {
            self.fixas = true;
            return;
        };
        let mut n = 0;
        let mut soma = 0u32;
        let mut v = [(0u16, 0u32); MAX_SAIDAS];
        for &(id, antes) in &antes.contagem {
            let d = crate::craft_ui::tem(slots, id).saturating_sub(antes);
            if d > 0 && n < MAX_SAIDAS {
                v[n] = (id, d);
                n += 1;
                soma += d;
            }
        }
        if soma >= self.sucessos as u32 {
            self.saidas = v;
            self.n_saidas = n;
            self.fixas = true;
        }
    }

    /// Draws over the panel body. `area` = the panel's inside (below the
    /// title). Call inside `estilo::no_painel` (sizes in `u`).
    pub fn desenha(
        &mut self,
        area: Rect,
        slots: &[InventorySlot],
        nomes: &HashMap<u16, String>,
        agora: f64,
        palco: Option<(&VoxCache, &Material)>,
    ) {
        self.confere_bolsa(slots);
        let tm = self.tempos();
        let t = (agora - self.inicio).max(0.0) as f32;
        // Tap: before the reveal, jump to it; after, start leaving.
        let toque = crate::foco::clique() && area.toca(Vec2::from(mouse_position()));
        if toque {
            if t < tm.revela {
                self.inicio = agora - tm.revela as f64;
            } else if t > tm.revela + 0.25 && t < tm.revela + tm.fica {
                self.inicio = agora - (tm.revela + tm.fica) as f64;
            }
        }
        let t = (agora - self.inicio).max(0.0) as f32;
        // Global fade (in, and out at the end).
        let fa = (t / 0.18).min(1.0) * (1.0 - ((t - tm.revela - tm.fica) / SAIDA).clamp(0.0, 1.0));
        if fa <= 0.0 {
            return;
        }
        // 3D models do not fade: while fading, the flat icon.
        let palco = palco.filter(|_| fa > 0.97);
        let cor = cor_do_grau(self.r.cor + 1);
        let cor_in = cor_do_grau(self.r.cor);
        let viva = estilo::misturar(cor, WHITE, 0.45);

        // Backdrop: the panel's inside goes dark, a faint pool of the tier colour.
        estilo::ret_gradiente(
            area,
            u(estilo::RAIO_GRANDE),
            Color::new(0.035, 0.028, 0.030, 0.94 * fa),
            Color::new(0.020, 0.016, 0.020, 0.96 * fa),
        );
        let c = vec2(area.center().x, area.y + area.h * 0.42);
        self.lim = (area.h * 0.42).min(area.w * 0.5) - u(6.0);
        self.chao = area.y + area.h;
        let carga = sai(((t - tm.junta.0 * 0.5) / (tm.revela - tm.junta.0 * 0.5)).clamp(0.0, 1.0));
        let poca = if t < tm.revela {
            carga * 0.7
        } else if self.ok() {
            1.0
        } else {
            0.7 * (1.0 - (t - tm.revela) / 0.5).clamp(0.0, 1.0)
        };
        for i in 0..10 {
            let r = (self.lim + u(30.0)) * (1.0 - i as f32 * 0.085);
            disco(c, r, estilo::alfa(cor, 0.018 * poca * fa));
        }

        if t < tm.revela {
            self.reune(c, t, &tm, cor, cor_in, carga, fa, palco);
            dica(area, "Tap to skip", fa * (t / 0.5).min(1.0) * 0.8);
        } else if self.ok() {
            self.sucesso(c, area, t - tm.revela, t, cor, viva, fa, nomes, palco);
        } else {
            self.falha(c, area, t - tm.revela, cor_in, fa, palco);
        }
    }

    /// Phase 1: the inputs pop in on a ring and spiral into a charging orb.
    #[allow(clippy::too_many_arguments)]
    fn reune(
        &self,
        c: Vec2,
        t: f32,
        tm: &Tempos,
        cor: Color,
        cor_in: Color,
        carga: f32,
        fa: f32,
        palco: Option<(&VoxCache, &Material)>,
    ) {
        let n = self.r.qtd.clamp(1, 10) as usize;
        let (raio0, lado) = if n > 5 { (u(150.0), u(44.0)) } else { (u(132.0), u(54.0)) };
        let raio0 = raio0.min(self.lim - lado * 0.5);
        let passo = if self.sintese() { 0.022 } else { 0.045 };
        let g = ((t - tm.junta.0) / (tm.junta.1 - tm.junta.0)).clamp(0.0, 1.0);
        let gi = g * g * g; // ease-in: slow start, fast pull
        // Thin rings contracting into the middle while it charges.
        for j in 0..3 {
            let f = (t * 1.5 + j as f32 / 3.0).fract();
            let r = u(30.0) + (self.lim - u(30.0)) * (1.0 - f);
            anel(c, r, u(1.6), estilo::alfa(cor, carga * f * 0.45 * fa));
        }
        // Motes streaming inward.
        for j in 0..16u32 {
            let f = (t * (0.9 + 0.5 * sorte(j + 3)) + sorte(j + 31)).fract();
            let a = sorte(j) * TAU + t * 0.4;
            let r = u(40.0) + (self.lim - u(40.0)) * (1.0 - f);
            let p = c + vec2(a.cos(), a.sin()) * r;
            disco(p, u(1.4 + 1.4 * sorte(j + 7)), estilo::alfa(estilo::misturar(cor, WHITE, 0.4), carga * f * 0.85 * fa));
        }
        // The orb.
        let tensao = ((t - (tm.revela - 0.2)) / 0.2).clamp(0.0, 1.0);
        let pulso = 1.0 + tensao * 0.12 * (t * 40.0).sin();
        let ro = (u(8.0) + u(30.0) * carga) * pulso;
        for (k, a) in [(3.2, 0.06), (2.2, 0.11), (1.5, 0.20)] {
            disco(c, ro * k, estilo::alfa(cor, a * carga * fa));
        }
        disco(c, ro, estilo::alfa(estilo::misturar(cor, WHITE, 0.35 + 0.4 * tensao), 0.85 * carga * fa));
        disco(c, ro * 0.55, estilo::alfa(WHITE, (0.45 + 0.5 * tensao) * carga * fa));

        // The inputs, back to front by position.
        let giro = 1.6 * gi + t * 0.25;
        for i in 0..n {
            let ti = t - i as f32 * passo;
            if ti <= 0.0 {
                continue;
            }
            let pop = sai_volta(ti / 0.24);
            let ang = -PI * 0.5 + i as f32 / n as f32 * TAU + giro;
            let r = raio0 * (1.0 - gi);
            let escala = pop * (1.0 - 0.6 * gi);
            let a = fa * (ti / 0.12).min(1.0) * (1.0 - ((gi - 0.75) / 0.25).clamp(0.0, 1.0));
            if a <= 0.01 || escala <= 0.01 {
                continue;
            }
            // A short comet tail along the spiral.
            if g > 0.05 {
                for k in 1..=3 {
                    let gk = (g - k as f32 * 0.035).max(0.0);
                    let gk3 = gk * gk * gk;
                    let ak = -PI * 0.5 + i as f32 / n as f32 * TAU + 1.6 * gk3 + t * 0.25;
                    let pk = c + vec2(ak.cos(), ak.sin()) * raio0 * (1.0 - gk3);
                    disco(pk, lado * escala * (0.30 - k as f32 * 0.06), estilo::alfa(cor, a * g * (0.22 - k as f32 * 0.05)));
                }
            }
            let p = c + vec2(ang.cos(), ang.sin()) * r;
            let brilho = estilo::misturar(cor_in, cor, gi);
            cartao_do_item(p, lado * escala, self.r.entrada, brilho, a, gi, palco);
        }
        // What is going on, under it all.
        let rotulo = if self.sintese() { "Synthesising" } else { "Combining" };
        let y = (c.y + raio0 + u(64.0)).min(self.chao - u(50.0));
        let a = fa * (t / 0.3).min(1.0);
        estilo::texto_centro_forte(c.x, y, rotulo, 18, estilo::alfa(estilo::TEXTO, a));
        let sub = if self.tentativas > 1 {
            format!("{} × {} · {} attempts", self.r.qtd, nome_curto(self.sintese()), self.tentativas)
        } else {
            format!("{} × {}", self.r.qtd, nome_curto(self.sintese()))
        };
        estilo::texto_centro(c.x, y + u(20.0), &sub, 13, estilo::alfa(estilo::SUAVE, a));
    }

    /// Phase 2, success: burst, rays and the result popping in.
    #[allow(clippy::too_many_arguments)]
    fn sucesso(
        &self,
        c: Vec2,
        area: Rect,
        b: f32,
        t: f32,
        cor: Color,
        viva: Color,
        fa: f32,
        nomes: &HashMap<u16, String>,
        palco: Option<(&VoxCache, &Material)>,
    ) {
        // Slow rays and halo behind the result.
        let forca = sai(b / 0.4) * fa;
        let n = 12;
        for i in 0..n {
            let a = i as f32 / n as f32 * TAU + t * 0.18;
            let l = u(150.0 + 22.0 * (t * 1.2 + i as f32 * 1.7).sin()) * forca;
            let d1 = vec2((a - 0.06).cos(), (a - 0.06).sin());
            let d2 = vec2((a + 0.06).cos(), (a + 0.06).sin());
            draw_triangle(c, c + d1 * l, c + d2 * l, estilo::alfa(cor, 0.10 * forca));
        }
        for (r, a) in [(120.0, 0.05), (86.0, 0.08), (60.0, 0.12)] {
            disco(c, u(r) * forca, estilo::alfa(cor, a * forca));
        }
        // The flash (a soft bloom, bright in the middle) and the shockwave.
        if b < 0.55 {
            let q = b / 0.55;
            let luz = estilo::misturar(cor, WHITE, 0.55);
            let rb = u(50.0) + (self.lim * 0.8) * sai(q);
            let forca_b = (1.0 - q).powi(2) * fa;
            for k in 0..7 {
                let f = 1.0 - k as f32 / 7.0;
                disco(c, rb * f, estilo::alfa(luz, 0.10 * forca_b));
            }
            disco(c, u(26.0) * (1.0 - q), estilo::alfa(WHITE, 0.9 * forca_b));
            anel(c, u(30.0) + (self.lim - u(30.0)) * sai(q), u(5.0) * (1.0 - q), estilo::alfa(luz, (1.0 - q) * 0.9 * fa));
        }
        // Sparks flying out, a little gravity.
        if b < 1.0 {
            let q = b;
            for j in 0..20u32 {
                let a = sorte(j + 50) * TAU;
                let vel = (self.lim * 0.45) + self.lim * 0.55 * sorte(j + 90);
                let p = c + vec2(a.cos(), a.sin()) * vel * sai(q) + vec2(0.0, u(50.0) * q * q);
                let cc = if j % 3 == 0 { WHITE } else { viva };
                brilhinho(p, u(4.0 + 4.0 * sorte(j + 13)) * (1.0 - q), estilo::alfa(cc, (1.0 - q) * fa));
            }
        }
        // The result.
        let pop = sai_volta((b - 0.04) / 0.36);
        let boia = (t * 2.2).sin() * u(2.5) * sai(b / 0.6);
        let k = self.n_saidas.max(1);
        let lado = if k == 1 { u(88.0) } else { u(64.0) };
        let gap = u(18.0);
        let largura = k as f32 * lado + (k - 1) as f32 * gap;
        for i in 0..k {
            let (id, qtd) = self.saidas[i];
            let d = ((b - 0.04 - i as f32 * 0.06) / 0.36).clamp(0.0, 1.0);
            let s = if i == 0 { pop } else { sai_volta(d) };
            if s <= 0.01 {
                continue;
            }
            let p = vec2(c.x - largura * 0.5 + lado * 0.5 + i as f32 * (lado + gap), c.y + boia);
            cartao_do_item(p, lado * s, id, cor, fa, 1.0, palco);
            if qtd > 1 {
                let txt = format!("×{qtd}");
                let tx = p.x + lado * 0.5 * s - estilo::medir(&txt, 15) * 0.5;
                estilo::texto_centro_forte(tx, p.y + lado * 0.5 * s + u(4.0), &txt, 15, estilo::alfa(estilo::TEXTO, fa * s.min(1.0)));
            }
        }
        // Text.
        let a = fa * ((b - 0.14) / 0.25).clamp(0.0, 1.0);
        let sobe = u(8.0) * (1.0 - a.min(1.0));
        let y = c.y + lado * 0.5 + u(48.0) + sobe;
        let titulo = if self.sintese() {
            if self.sucessos > 1 { format!("Created {}×", self.sucessos) } else { "Created!".to_string() }
        } else if self.sucessos == 1 && self.tentativas == 1 {
            "Success!".to_string()
        } else if self.sucessos == 1 {
            "1 success".to_string()
        } else {
            format!("{} successes", self.sucessos)
        };
        estilo::texto_centro_forte(c.x, y, &titulo, 24, estilo::alfa(viva, a));
        let mut y2 = y + u(24.0);
        let sub = if k == 1 {
            let (id, qtd) = self.saidas[0];
            let nome = nomes.get(&id).cloned().unwrap_or_else(|| format!("item {id}"));
            if qtd > 1 { format!("{qtd} × {nome}") } else { nome }
        } else {
            String::new()
        };
        let sub = match (sub.is_empty(), self.tentativas > 1 && !self.sintese()) {
            (false, true) => format!("{sub} · from {} attempts", self.tentativas),
            (true, true) => format!("from {} attempts", self.tentativas),
            _ => sub,
        };
        if !sub.is_empty() {
            estilo::texto_centro(c.x, y2, &sub, 15, estilo::alfa(estilo::TEXTO, a));
            y2 += u(22.0);
        }
        if self.garantida {
            chip(vec2(c.x, y2 + u(2.0)), "Guaranteed success", estilo::OURO, a);
            y2 += u(26.0);
        }
        if let Some(n) = self.faltam_depois {
            linha_garantia(c.x, y2 + u(4.0), n, a * 0.85, "Next guarantee in");
        }
        dica(area, "Tap to continue", fa * ((b - 0.6) / 0.3).clamp(0.0, 1.0) * 0.8);
    }

    /// Phase 2, fail: the orb gutters out, the inputs come back grey, crack
    /// and crumble.
    fn falha(&self, c: Vec2, area: Rect, b: f32, cor_in: Color, fa: f32, palco: Option<(&VoxCache, &Material)>) {
        let cinza = Color::new(0.55, 0.52, 0.50, 1.0);
        // The orb shrinks and greys.
        if b < 0.4 {
            let q = b / 0.4;
            let ro = u(38.0) * (1.0 - sai(q));
            let tremor = 1.0 + 0.15 * (b * 60.0).sin() * (1.0 - q);
            disco(c, ro * 1.8 * tremor, estilo::alfa(cinza, 0.14 * (1.0 - q) * fa));
            disco(c, ro * tremor, estilo::alfa(estilo::misturar(cor_in, cinza, q), 0.7 * (1.0 - q) * fa));
        }
        // A puff of grey smoke.
        if b < 1.1 {
            let q = b / 1.1;
            for j in 0..14u32 {
                let a = sorte(j + 200) * TAU;
                let v = u(30.0 + 60.0 * sorte(j + 230));
                let p = c + vec2(a.cos(), a.sin() * 0.6) * v * sai(q) + vec2(0.0, -u(18.0) * q);
                let r = u(6.0 + 8.0 * sorte(j + 260)) * (0.6 + q);
                disco(p, r * 1.6, estilo::alfa(cinza, 0.025 * (1.0 - q) * fa));
                disco(p, r, estilo::alfa(cinza, 0.04 * (1.0 - q) * fa));
            }
        }
        // The inputs, in a row, dim — then cracked, falling, fading.
        let n = self.r.qtd.clamp(1, 10) as usize;
        let lado = if n > 5 { u(40.0) } else { u(50.0) };
        let gap = u(10.0);
        let largura = n as f32 * lado + (n - 1) as f32 * gap;
        for i in 0..n {
            let aparece = sai((b - 0.05) / 0.25);
            let d = ((b - 0.45 - i as f32 * 0.07) / 0.55).clamp(0.0, 1.0);
            let a = fa * aparece * (1.0 - d);
            if a <= 0.01 {
                continue;
            }
            let p = vec2(
                c.x - largura * 0.5 + lado * 0.5 + i as f32 * (lado + gap),
                c.y + u(14.0) * d * d + u(6.0) * (1.0 - aparece),
            );
            // Grey and dim: the colour has gone out of it.
            cartao_do_item(p, lado * (1.0 - 0.12 * d), self.r.entrada, cinza, a * 0.6, 0.0, palco);
            let h = lado * 0.5;
            // Cracks.
            let fr = ((b - 0.25 - i as f32 * 0.05) / 0.15).clamp(0.0, 1.0);
            if fr > 0.0 {
                let tinta = Color::new(0.02, 0.02, 0.02, 0.85 * a * fr);
                let s = sorte(i as u32 + 300) - 0.5;
                let pts = [
                    p + vec2(-h * 0.2 + s * h * 0.3, -h * 0.95),
                    p + vec2(h * 0.12, -h * 0.35),
                    p + vec2(-h * 0.18 + s * h * 0.2, h * 0.15),
                    p + vec2(h * 0.25, h * 0.95),
                ];
                for w in pts.windows(2) {
                    draw_line(w[0].x, w[0].y, w[1].x, w[1].y, u(1.6), tinta);
                }
                draw_line(pts[1].x, pts[1].y, p.x + h * 0.9, p.y - h * 0.1, u(1.2), tinta);
            }
            // Dust falling off as it goes.
            if d > 0.0 {
                for j in 0..5u32 {
                    let sx = (sorte(j + i as u32 * 7 + 400) - 0.5) * lado;
                    let p2 = p + vec2(sx, h * 0.6 + u(30.0) * d * (0.6 + sorte(j + 500)));
                    draw_rectangle(p2.x, p2.y, u(2.5), u(2.5), estilo::alfa(cinza, 0.8 * (1.0 - d) * fa));
                }
            }
        }
        // Text.
        let a = fa * ((b - 0.15) / 0.3).clamp(0.0, 1.0);
        let y = c.y + lado * 0.5 + u(56.0);
        estilo::texto_centro_forte(c.x, y, "No luck this time", 24, estilo::alfa(Color::new(0.88, 0.82, 0.74, 1.0), a));
        let sub = if self.tentativas > 1 {
            format!("0 of {} attempts succeeded", self.tentativas)
        } else {
            format!("The {} were consumed", nome_curto(false))
        };
        estilo::texto_centro(c.x, y + u(24.0), &sub, 15, estilo::alfa(estilo::SUAVE, a));
        if let Some(n) = self.faltam_depois {
            linha_garantia(c.x, y + u(50.0), n, a, "Guaranteed in");
        }
        dica(area, "Tap to continue", fa * ((b - 0.6) / 0.3).clamp(0.0, 1.0) * 0.8);
    }
}

fn nome_curto(sintese: bool) -> &'static str {
    if sintese { "materials" } else { "items" }
}

fn linha_garantia(x: f32, y: f32, n: u16, a: f32, prefixo: &str) {
    if n <= 1 {
        estilo::texto_centro(x, y, "Next one is guaranteed!", 14, estilo::alfa(estilo::VERDE, a));
    } else {
        let txt = format!("{prefixo} {n} tries");
        estilo::texto_centro(x, y, &txt, 14, estilo::alfa(Color::new(0.95, 0.78, 0.30, 1.0), a));
    }
}

/// A small pill with a gold edge, centred on `c`.
fn chip(c: Vec2, txt: &str, cor: Color, a: f32) {
    let w = estilo::medir(txt, 13) + u(22.0);
    let h = u(22.0);
    let r = Rect::new(c.x - w * 0.5, c.y - h * 0.5, w, h);
    estilo::ret_arredondado(r, h * 0.5, estilo::alfa(cor, 0.16 * a));
    estilo::borda_arredondada(r, h * 0.5, u(1.2), estilo::alfa(cor, 0.85 * a));
    estilo::texto_centro_forte(c.x, c.y + u(4.5), txt, 13, estilo::alfa(cor, a));
}

fn dica(area: Rect, txt: &str, a: f32) {
    if a > 0.01 {
        estilo::texto_centro(area.center().x, area.y + area.h - u(14.0), txt, 13, estilo::alfa(estilo::SUAVE, a));
    }
}

/// The item on a rounded tile in `cor` (its tier), centred on `c`. `luz`
/// 0..1 = how much the tile glows.
fn cartao_do_item(
    c: Vec2,
    lado: f32,
    id: u16,
    cor: Color,
    a: f32,
    luz: f32,
    palco: Option<(&VoxCache, &Material)>,
) {
    if lado < 2.0 {
        return;
    }
    let h = lado * 0.5;
    let r = Rect::new(c.x - h, c.y - h, lado, lado);
    let raio = lado * 0.16;
    if luz > 0.0 {
        disco(c, lado * 0.85, estilo::alfa(cor, 0.16 * luz * a));
    }
    estilo::ret_gradiente(
        r,
        raio,
        estilo::alfa(estilo::misturar(Color::new(0.10, 0.08, 0.07, 1.0), cor, 0.28), 0.95 * a),
        estilo::alfa(estilo::misturar(Color::new(0.05, 0.04, 0.04, 1.0), cor, 0.12), 0.95 * a),
    );
    estilo::borda_arredondada(r, raio, (lado * 0.04).max(1.0), estilo::alfa(cor, 0.9 * a));
    let pad = lado * 0.10;
    crate::bolsa::icone_do_item_com(
        Rect::new(r.x + pad, r.y + pad, lado - pad * 2.0, lado - pad * 2.0),
        id,
        a,
        palco,
    );
}

/// The colour of a grade: the same table as the bag's frames.
pub fn cor_do_grau(grau: u8) -> Color {
    let h = shared::items::tier_color_hex(grau.clamp(1, 5)).trim_start_matches('#');
    let v = u32::from_str_radix(h, 16).unwrap_or(0xbf_bf_bf);
    Color::from_rgba((v >> 16) as u8, (v >> 8) as u8, v as u8, 255)
}

fn sai(x: f32) -> f32 {
    1.0 - (1.0 - x.clamp(0.0, 1.0)).powi(3)
}

/// Overshoots a little and settles: the "pop".
fn sai_volta(x: f32) -> f32 {
    if x <= 0.0 {
        return 0.0;
    }
    let x = x.min(1.0);
    let c1 = 1.70158;
    let c3 = c1 + 1.0;
    1.0 + c3 * (x - 1.0).powi(3) + c1 * (x - 1.0).powi(2)
}

fn sorte(i: u32) -> f32 {
    ((i as f32 * 12.9898).sin() * 43758.547).fract().abs()
}

/// A smooth disc (`draw_circle` has 20 sides; big ones show their corners).
fn disco(c: Vec2, r: f32, cor: Color) {
    if r > 0.5 && cor.a > 0.003 {
        let lados = if r > u(40.0) { 64 } else { 32 };
        draw_poly(c.x, c.y, lados, r, 0.0, cor);
    }
}

fn anel(c: Vec2, r: f32, grossura: f32, cor: Color) {
    if r > 0.5 && grossura > 0.1 && cor.a > 0.003 {
        draw_poly_lines(c.x, c.y, 64, r, 0.0, grossura, cor);
    }
}

/// A four-point twinkle.
fn brilhinho(c: Vec2, r: f32, cor: Color) {
    if r < 0.3 {
        return;
    }
    let w = r * 0.22;
    draw_triangle(c + vec2(0.0, -r), c + vec2(w, 0.0), c + vec2(-w, 0.0), cor);
    draw_triangle(c + vec2(0.0, r), c + vec2(w, 0.0), c + vec2(-w, 0.0), cor);
    draw_triangle(c + vec2(-r, 0.0), c + vec2(0.0, w), c + vec2(0.0, -w), cor);
    draw_triangle(c + vec2(r, 0.0), c + vec2(0.0, w), c + vec2(0.0, -w), cor);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_garantia_do_lote_sai_da_contagem_de_antes() {
        // Counter at 0 (25 to go): one attempt, no guarantee, 24 to go.
        assert_eq!(garantia_do_lote(25, 1), (false, 24));
        // The next one was sure: it fires, and the cycle restarts.
        assert_eq!(garantia_do_lote(1, 1), (true, 25));
        // 3 to go, 10 attempts: fires on the 3rd, 7 more after → 18 to go.
        assert_eq!(garantia_do_lote(3, 10), (true, 18));
        // 10 to go, 9 attempts: still 1.
        assert_eq!(garantia_do_lote(10, 9), (false, 1));
    }

    #[test]
    fn a_duracao_fica_na_faixa_pedida() {
        let r = combinar::receitas().into_iter().find(|r| r.chance < 100).unwrap();
        let a = Animacao::nova(r, 1, 1, Some(25), None, 0.0);
        assert!(a.tempos().revela <= 1.3 && a.duracao() < 3.5);
        let s = combinar::receitas().into_iter().find(|r| r.chance == 100).unwrap();
        let b = Animacao::nova(s, 1, 1, None, None, 0.0);
        assert!(b.tempos().revela < a.tempos().revela && b.duracao() < a.duracao());
        assert!(!a.acabou(1.0) && a.acabou(10.0));
    }
}
