//! "Rate Tempest on Google Play" — the prompt every game has (owner,
//! 09/11/2026: "after some time playing, that 'rate us on the Play Store'
//! with the send-review button").
//!
//! When: only after the player has really played — `SEGUNDOS_MIN` of play in
//! the world and level `NIVEL_MIN` — at a calm moment (no window open, not in
//! a dungeon). "Later" brings it back after `INTERVALO_DIAS`, at most
//! `VEZES_MAX` times; "Rate" never again. The text is NEUTRAL on purpose:
//! Google's policy forbids asking "do you like the game?" before sending
//! people to rate.
//!
//! Where it sends: the game's Google Play page (the Play Store app first,
//! `market://`, then the web page). Android only in production — the iPhone
//! app is not on the App Store yet. A TEST build (`MMO_API_PADRAO` set at
//! compile time) uses short thresholds and shows it on every platform, so it
//! can be tried on the test server.
//!
//! State: a small file next to the device prefs (`<prefs>.avaliacao`),
//! `segundos;vezes;ultimo_unix;feito`.

use macroquad::prelude::*;

#[allow(unused_imports)]
use crate::ui::Toque as _;
use crate::hud_estilo as estilo;

const TESTE: bool = option_env!("MMO_API_PADRAO").is_some();
const SEGUNDOS_MIN: f64 = if TESTE { 120.0 } else { 2.0 * 3600.0 };
const NIVEL_MIN: u32 = if TESTE { 1 } else { 10 };
const INTERVALO_DIAS: i64 = if TESTE { 0 } else { 7 };
const VEZES_MAX: u32 = 3;
/// Where the prompt makes sense at all.
pub const DISPONIVEL: bool = TESTE || cfg!(target_os = "android");

const LOJA_APP: &str = "market://details?id=com.niertech.tempest";
const LOJA_WEB: &str = "https://play.google.com/store/apps/details?id=com.niertech.tempest";

#[derive(Debug, Default, Clone, PartialEq)]
struct Estado {
    segundos: f64,
    vezes: u32,
    ultimo_unix: i64,
    feito: bool,
}

impl Estado {
    fn de(texto: &str) -> Self {
        let p: Vec<&str> = texto.trim().split(';').collect();
        Self {
            segundos: p.first().and_then(|v| v.parse().ok()).unwrap_or(0.0),
            vezes: p.get(1).and_then(|v| v.parse().ok()).unwrap_or(0),
            ultimo_unix: p.get(2).and_then(|v| v.parse().ok()).unwrap_or(0),
            feito: p.get(3).is_some_and(|v| *v == "1"),
        }
    }

    fn texto(&self) -> String {
        format!("{:.0};{};{};{}", self.segundos, self.vezes, self.ultimo_unix, self.feito as u8)
    }

    /// Time to show it? (`agora` in unix seconds).
    fn pronto(&self, nivel: u32, agora: i64) -> bool {
        !self.feito
            && self.vezes < VEZES_MAX
            && self.segundos >= SEGUNDOS_MIN
            && nivel >= NIVEL_MIN
            && agora - self.ultimo_unix >= INTERVALO_DIAS * 86_400
    }
}

fn arquivo() -> Option<std::path::PathBuf> {
    crate::lembranca::caminho().map(|p| p.with_extension("avaliacao"))
}

fn agora_unix() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as i64)
}

pub struct Avaliacao {
    estado: Estado,
    pub aberta: bool,
    gravado_em: f64,
}

impl Default for Avaliacao {
    fn default() -> Self {
        let estado = arquivo()
            .and_then(|p| std::fs::read_to_string(p).ok())
            .map(|t| Estado::de(&t))
            .unwrap_or_default();
        Self { estado, aberta: false, gravado_em: 0.0 }
    }
}

/// Opens the game's store page. `false` if nothing could be opened.
pub fn abrir_loja() -> bool {
    crate::nativo::abrir_url(LOJA_APP) || crate::nativo::abrir_url(LOJA_WEB)
}

impl Avaliacao {
    fn grava(&mut self, agora: f64) {
        self.gravado_em = agora;
        if let Some(p) = arquivo() {
            let _ = std::fs::write(p, self.estado.texto());
        }
    }

    /// Once per frame in the world. `calmo` = no window open, not in a
    /// dungeon or fight: the only moments it may appear.
    pub fn passo(&mut self, dt: f32, nivel: u32, calmo: bool) {
        if !DISPONIVEL || self.estado.feito {
            return;
        }
        let t = get_time();
        self.estado.segundos += dt.min(1.0) as f64;
        if t - self.gravado_em > 60.0 {
            self.grava(t);
        }
        if !self.aberta && calmo && self.estado.pronto(nivel, agora_unix()) {
            self.aberta = true;
            self.estado.vezes += 1;
            self.estado.ultimo_unix = agora_unix();
            self.grava(t);
        }
    }

    /// The prompt card. `true` while it is on screen (it takes the clicks).
    pub fn desenha(&mut self) -> bool {
        if !self.aberta {
            return false;
        }
        let seguro = crate::hud_layout::tela_segura();
        let k = estilo::escala_do_painel(520.0, 300.0);
        estilo::no_painel(k, || {
            let (w, h) = (520.0 * k, 286.0 * k);
            let p = Rect::new(seguro.center().x - w * 0.5, seguro.center().y - h * 0.5, w, h);
            crate::hud_layout::escurece(0.55);
            estilo::painel_destaque(p, estilo::OURO);
            let cx = p.center().x;
            // Five stars, drawn: the HUD font has no star glyph.
            for i in 0..5 {
                let c = vec2(cx + (i as f32 - 2.0) * 34.0 * k, p.y + 52.0 * k);
                estrela(c, 14.0 * k, estilo::OURO);
            }
            estilo::texto_centro_forte(cx, p.y + 104.0 * k, "Rate Tempest", 24, estilo::OURO);
            estilo::texto_centro(cx, p.y + 136.0 * k, "Your review on Google Play helps other", 16, estilo::TEXTO);
            estilo::texto_centro(cx, p.y + 158.0 * k, "players find the game. Thank you!", 16, estilo::TEXTO);
            let m = Vec2::from(mouse_position());
            let clicou = crate::foco::clique();
            let bw = (w - 60.0 * k) * 0.5;
            let avaliar = Rect::new(p.x + 20.0 * k, p.y + h - 70.0 * k, bw + 20.0 * k, 50.0 * k);
            let depois = Rect::new(avaliar.x + avaliar.w + 20.0 * k, avaliar.y, bw - 20.0 * k, 50.0 * k);
            let sobre_a = avaliar.toca(m);
            estilo::botao(avaliar, "", estilo::estado(sobre_a, is_mouse_button_down(MouseButton::Left), false, false), true);
            estilo::texto_centro_forte(avaliar.center().x, avaliar.center().y + 7.0 * k, "Rate on Google Play", 18, crate::ui::OURO_CLARO);
            let sobre_d = depois.toca(m);
            estilo::botao(depois, "Later", estilo::estado(sobre_d, false, false, false), false);
            if clicou && sobre_a {
                self.estado.feito = true;
                self.aberta = false;
                self.grava(get_time());
                abrir_loja();
                crate::sons::tocar(crate::sons::Som::Clique);
            } else if clicou && sobre_d {
                self.aberta = false;
                crate::sons::tocar(crate::sons::Som::Clique);
            }
        });
        true
    }
}

fn estrela(c: Vec2, r: f32, cor: Color) {
    let ponto = |i: usize| {
        let a = -std::f32::consts::FRAC_PI_2 + i as f32 * std::f32::consts::PI / 5.0;
        let rr = if i % 2 == 0 { r } else { r * 0.45 };
        vec2(c.x + a.cos() * rr, c.y + a.sin() * rr)
    };
    for i in 0..10 {
        draw_triangle(c, ponto(i), ponto((i + 1) % 10), cor);
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn estado_vai_e_volta_do_arquivo() {
        let e = Estado { segundos: 7300.0, vezes: 2, ultimo_unix: 1_800_000_000, feito: true };
        assert_eq!(Estado::de(&e.texto()), e);
        assert_eq!(Estado::de("lixo"), Estado::default());
    }

    #[test]
    fn so_aparece_quando_jogou_bastante_e_nao_insiste() {
        let mut e = Estado { segundos: SEGUNDOS_MIN, ..Default::default() };
        assert!(e.pronto(NIVEL_MIN, 10_000_000));
        assert!(!e.pronto(NIVEL_MIN.saturating_sub(1).max(0), 10_000_000) || NIVEL_MIN == 0);
        e.feito = true;
        assert!(!e.pronto(99, 10_000_000), "after rating, never again");
        let mut e = Estado { segundos: SEGUNDOS_MIN, vezes: VEZES_MAX, ..Default::default() };
        assert!(!e.pronto(99, 10_000_000), "at most VEZES_MAX times");
        e.vezes = 1;
        e.ultimo_unix = 10_000_000;
        assert_eq!(e.pronto(99, 10_000_000 + INTERVALO_DIAS * 86_400 - 1), INTERVALO_DIAS == 0);
    }
}
