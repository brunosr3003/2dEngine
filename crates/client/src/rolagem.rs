//! List scrolling, the same in every panel: mouse wheel, DRAG (finger or
//! mouse) inside the list, and a visible bar on the right, which also drags.
//!
//! Before, each panel read the wheel only — on a phone there is no wheel, and
//! the Craft, Quests and Dailies lists simply did not scroll. And with no bar
//! nothing said there was more below.
//!
//! Dragging and touching start the same way (the finger lands), so a touch on
//! a ROW only counts on release and only if the finger did not move: `quadro`
//! returns that click, and the list uses it in place of `is_mouse_button_pressed`.

#[allow(unused_imports)]
use crate::ui::Toque as _;
use macroquad::prelude::*;

use crate::hud_estilo as estilo;

/// Width of the drawn bar and of the strip that catches the finger on it (at
/// 100%; they grow with the panel's scale).
const LARGURA_BARRA: f32 = 6.0;
const FAIXA_DA_BARRA: f32 = 26.0;

#[derive(Debug, Clone, Copy)]
struct Gesto {
    /// Where the finger landed and the scroll at that instant.
    y0: f32,
    pos0: f32,
    inicio: Vec2,
    arrastando: bool,
    /// Caught the bar: the finger moves the thumb, not the content.
    na_barra: bool,
    /// The finger's y last frame and its speed (px/s, smoothed): what is
    /// left of it on release becomes the glide (`embalo`).
    ultimo_y: f32,
    vel: f32,
}

#[derive(Debug, Default)]
pub struct Rolagem {
    /// How much of the content has already passed above, in px.
    pub pos: f32,
    gesto: Option<Gesto>,
    /// The glide after a flick, in px/s of `pos` (0 = still).
    vel: f32,
    /// No bar to grab (a row of tabs swiped sideways, see `passo_horizontal`).
    pub sem_barra: bool,
}

/// How fast the glide dies: the speed falls to ~37% every `TAU_DO_EMBALO`
/// seconds (the same feel as the camera's inertia).
const TAU_DO_EMBALO: f32 = 0.325;
/// Below this (px/s) the glide stops.
const EMBALO_MIN: f32 = 20.0;

/// One frame of glide: the list keeps moving after the finger leaves and
/// slows down; it stops at either end. Returns the new (pos, vel).
pub fn embalo(pos: f32, vel: f32, dt: f32, max: f32) -> (f32, f32) {
    if vel.abs() < EMBALO_MIN || dt <= 0.0 {
        return (pos.clamp(0.0, max), 0.0);
    }
    let novo = pos + vel * dt;
    if novo <= 0.0 || novo >= max {
        return (novo.clamp(0.0, max), 0.0);
    }
    (novo, vel * (-dt / TAU_DO_EMBALO).exp())
}

/// What one frame of the list needs to know.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Entrada {
    pub mouse: Vec2,
    pub apertou: bool,
    pub segurando: bool,
    pub soltou: bool,
    pub roda: f32,
    /// Seconds since the last frame (the glide's clock).
    pub dt: f32,
}

impl Entrada {
    pub fn agora() -> Self {
        Entrada {
            mouse: Vec2::from(mouse_position()),
            apertou: crate::foco::clique(),
            segurando: is_mouse_button_down(MouseButton::Left),
            soltou: is_mouse_button_released(MouseButton::Left),
            roda: mouse_wheel().1,
            dt: get_frame_time().min(0.1),
        }
    }
}

/// Maior rolagem possivel.
pub fn maximo(area: Rect, total: f32) -> f32 {
    (total - area.h).max(0.0)
}

/// A faixa da barra (trilho), a' direita da lista.
pub fn trilho(area: Rect) -> Rect {
    Rect::new(
        area.x + area.w - estilo::u(LARGURA_BARRA) - 2.0,
        area.y + 2.0,
        estilo::u(LARGURA_BARRA),
        area.h - 4.0,
    )
}

/// (y, altura) do polegar dentro do trilho.
fn polegar(t: Rect, area: Rect, total: f32, pos: f32) -> (f32, f32) {
    let frac = (area.h / total.max(1.0)).clamp(0.05, 1.0);
    let h = (t.h * frac).max(t.w * 4.0).min(t.h);
    let max = maximo(area, total);
    let y = t.y + (t.h - h) * if max > 0.0 { pos / max } else { 0.0 };
    (y, h)
}

impl Rolagem {
    pub fn zera(&mut self) {
        self.pos = 0.0;
        self.gesto = None;
        self.vel = 0.0;
    }

    /// A finger is dragging this list (the rows must not light up).
    pub fn arrastando(&self) -> bool {
        self.gesto.is_some_and(|g| g.arrastando)
    }

    /// One frame with the real input (see `passo`). The bar draws separately,
    /// with `desenha`, AFTER the rows — on top of them.
    pub fn quadro(&mut self, area: Rect, total: f32, passo_da_roda: f32) -> Option<Vec2> {
        self.passo(area, total, passo_da_roda, Entrada::agora())
    }

    /// The logic, with no drawing: wheel, drag on the content and on the bar.
    /// Returns the CLICK on the list — the point where the finger released
    /// without having dragged.
    pub fn passo(
        &mut self,
        area: Rect,
        total: f32,
        passo_da_roda: f32,
        e: Entrada,
    ) -> Option<Vec2> {
        let max = maximo(area, total);
        if area.toca(e.mouse) && e.roda != 0.0 {
            self.pos -= e.roda.signum() * passo_da_roda;
            self.vel = 0.0;
        }
        if e.apertou && area.toca(e.mouse) {
            // A finger landing stops the glide (and that touch is not a tap
            // on a row that was still moving).
            let parou_embalo = self.vel.abs() >= EMBALO_MIN;
            self.vel = 0.0;
            let faixa = Rect::new(
                area.x + area.w - estilo::u(FAIXA_DA_BARRA),
                area.y,
                estilo::u(FAIXA_DA_BARRA),
                area.h,
            );
            self.gesto = Some(Gesto {
                y0: e.mouse.y,
                pos0: self.pos,
                inicio: e.mouse,
                arrastando: parou_embalo,
                na_barra: !self.sem_barra && max > 0.0 && faixa.toca(e.mouse),
                ultimo_y: e.mouse.y,
                vel: 0.0,
            });
        }
        let mut clique = None;
        if let Some(mut g) = self.gesto {
            let dy = e.mouse.y - g.y0;
            if !g.arrastando && (e.mouse - g.inicio).length() > crate::toque::TOLERANCIA_PX {
                g.arrastando = true;
            }
            if g.na_barra {
                // The thumb moves with the finger: a whole track = the whole list.
                let t = trilho(area);
                let (_, h) = polegar(t, area, total, self.pos);
                let curso = (t.h - h).max(1.0);
                self.pos = g.pos0 + dy / curso * max;
            } else if g.arrastando {
                // The content moves WITH the finger: pulling up shows what is below.
                self.pos = g.pos0 - dy;
                if e.dt > 0.0 {
                    let agora = -(e.mouse.y - g.ultimo_y) / e.dt;
                    g.vel = g.vel * 0.5 + agora * 0.5;
                }
            }
            g.ultimo_y = e.mouse.y;
            if e.soltou || !e.segurando {
                if !g.arrastando && !g.na_barra && e.soltou {
                    clique = Some(e.mouse);
                }
                if g.arrastando && !g.na_barra {
                    self.vel = g.vel;
                }
                self.gesto = None;
            } else {
                self.gesto = Some(g);
            }
        }
        if self.gesto.is_none() {
            (self.pos, self.vel) = embalo(self.pos, self.vel, e.dt, max);
        }
        self.pos = self.pos.clamp(0.0, max);
        clique
    }

    /// The same gesture SIDEWAYS, for a row that is wider than its strip
    /// (a row of tabs on a phone): `pos` is how much has passed to the left.
    /// There is no bar; returns the tap like `passo`.
    pub fn passo_horizontal(
        &mut self,
        area: Rect,
        total: f32,
        passo_da_roda: f32,
        e: Entrada,
    ) -> Option<Vec2> {
        self.sem_barra = true;
        let deitada = Rect::new(area.y, area.x, area.h, area.w);
        let virar = |v: Vec2| vec2(v.y, v.x);
        let toque = self.passo(deitada, total, passo_da_roda, Entrada { mouse: virar(e.mouse), ..e });
        toque.map(virar)
    }

    /// The bar, only when there is something to scroll.
    pub fn desenha(&self, area: Rect, total: f32) {
        if maximo(area, total) <= 0.0 {
            return;
        }
        let t = trilho(area);
        estilo::ret_arredondado(t, t.w * 0.5, estilo::alfa(estilo::FUNDO_BAIXO, 0.85));
        let (y, h) = polegar(t, area, total, self.pos);
        let cor = if self.gesto.is_some_and(|g| g.na_barra) {
            estilo::OURO
        } else {
            estilo::alfa(estilo::SUAVE, 0.75)
        };
        estilo::ret_arredondado(Rect::new(t.x, y, t.w, h), t.w * 0.5, cor);
    }
}

/// Clips the drawing to `area` (a half row does not leak into the panel's
/// header); `None` returns to normal. Screen coordinates, the same as the HUD's.
pub fn recortar(area: Option<Rect>) {
    let clip = area.map(|a| {
        (
            a.x.floor() as i32,
            a.y.floor() as i32,
            a.w.ceil().max(0.0) as i32,
            a.h.ceil().max(0.0) as i32,
        )
    });
    unsafe { get_internal_gl() }.quad_gl.scissor(clip);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn e(x: f32, y: f32, apertou: bool, segurando: bool, soltou: bool) -> Entrada {
        Entrada {
            mouse: vec2(x, y),
            apertou,
            segurando,
            soltou,
            roda: 0.0,
            dt: 0.0,
        }
    }

    const AREA: Rect = Rect {
        x: 0.0,
        y: 0.0,
        w: 300.0,
        h: 400.0,
    };

    #[test]
    fn toque_parado_e_clique_no_soltar() {
        let mut r = Rolagem::default();
        assert_eq!(
            r.passo(AREA, 1000.0, 40.0, e(50.0, 100.0, true, true, false)),
            None
        );
        assert_eq!(
            r.passo(AREA, 1000.0, 40.0, e(52.0, 101.0, false, false, true)),
            Some(vec2(52.0, 101.0))
        );
        assert_eq!(r.pos, 0.0);
    }

    #[test]
    fn arrastar_rola_com_o_dedo_e_nao_clica() {
        let mut r = Rolagem::default();
        r.passo(AREA, 1000.0, 40.0, e(50.0, 300.0, true, true, false));
        r.passo(AREA, 1000.0, 40.0, e(50.0, 200.0, false, true, false));
        assert!(r.arrastando());
        assert_eq!(r.pos, 100.0, "puxou 100 pra cima, a lista subiu 100");
        let c = r.passo(AREA, 1000.0, 40.0, e(50.0, 150.0, false, false, true));
        assert_eq!(c, None, "arrasto nao vira clique");
        assert_eq!(r.pos, 150.0);
        // Does not go past the end or the start.
        r.passo(AREA, 1000.0, 40.0, e(50.0, 390.0, true, true, false));
        r.passo(AREA, 1000.0, 40.0, e(50.0, -2000.0, false, true, false));
        assert_eq!(r.pos, 600.0);
    }

    #[test]
    fn a_barra_arrasta_a_lista_inteira() {
        let mut r = Rolagem::default();
        let t = trilho(AREA);
        r.passo(AREA, 1000.0, 40.0, e(t.x + 2.0, 10.0, true, true, false));
        r.passo(AREA, 1000.0, 40.0, e(t.x + 2.0, 1000.0, false, true, false));
        assert_eq!(r.pos, maximo(AREA, 1000.0), "puxou o polegar ate' o fim");
    }

    #[test]
    fn embalo_desacelera_e_para_nas_pontas() {
        // Keeps moving, slower each frame.
        let (p1, v1) = embalo(100.0, 1000.0, 0.1, 600.0);
        assert!((p1 - 200.0).abs() < 0.01);
        assert!(v1 < 1000.0 && v1 > 0.0);
        // Dies out by itself in well under two seconds.
        let (mut p, mut v) = (100.0, 2000.0);
        for _ in 0..120 {
            (p, v) = embalo(p, v, 1.0 / 60.0, 10_000.0);
        }
        assert_eq!(v, 0.0, "the glide stopped");
        assert!(p > 100.0 && p < 1_000.0);
        // Hitting an end stops it there.
        assert_eq!(embalo(590.0, 5000.0, 0.1, 600.0), (600.0, 0.0));
        assert_eq!(embalo(10.0, -5000.0, 0.1, 600.0), (0.0, 0.0));
    }

    #[test]
    fn soltar_arrastando_deixa_embalo_e_toque_para() {
        let quadro = |x: f32, y: f32, apertou, segurando, soltou| {
            let mut q = e(x, y, apertou, segurando, soltou);
            q.dt = 1.0 / 60.0;
            q
        };
        let mut r = Rolagem::default();
        r.passo(AREA, 5000.0, 40.0, quadro(50.0, 300.0, true, true, false));
        for y in [280.0, 250.0, 220.0] {
            r.passo(AREA, 5000.0, 40.0, quadro(50.0, y, false, true, false));
        }
        r.passo(AREA, 5000.0, 40.0, quadro(50.0, 220.0, false, false, true));
        let solto = r.pos;
        r.passo(AREA, 5000.0, 40.0, quadro(50.0, 220.0, false, false, false));
        assert!(r.pos > solto, "the list kept going after the flick");
        // A finger landing stops it, and that touch is not a tap on a row.
        r.passo(AREA, 5000.0, 40.0, quadro(50.0, 100.0, true, true, false));
        let parado = r.pos;
        assert_eq!(r.passo(AREA, 5000.0, 40.0, quadro(50.0, 100.0, false, false, true)), None);
        assert_eq!(r.pos, parado);
    }

    #[test]
    fn de_lado_arrasta_no_x_e_toca_no_lugar_certo() {
        let faixa = Rect::new(0.0, 0.0, 300.0, 40.0);
        let mut r = Rolagem::default();
        r.passo_horizontal(faixa, 900.0, 40.0, e(250.0, 20.0, true, true, false));
        r.passo_horizontal(faixa, 900.0, 40.0, e(150.0, 22.0, false, true, false));
        assert_eq!(r.pos, 100.0, "swiped left 100, the row moved 100");
        assert_eq!(r.passo_horizontal(faixa, 900.0, 40.0, e(150.0, 22.0, false, false, true)), None);
        // A tap near the right end is a tap, never a grab of a bar.
        r.passo_horizontal(faixa, 900.0, 40.0, e(295.0, 20.0, true, true, false));
        assert_eq!(
            r.passo_horizontal(faixa, 900.0, 40.0, e(295.0, 20.0, false, false, true)),
            Some(vec2(295.0, 20.0))
        );
    }

    #[test]
    fn roda_e_lista_que_cabe() {
        let mut r = Rolagem::default();
        let mut x = e(50.0, 50.0, false, false, false);
        x.roda = -1.0;
        r.passo(AREA, 1000.0, 40.0, x);
        assert_eq!(r.pos, 40.0);
        let mut curta = Rolagem::default();
        curta.passo(AREA, 200.0, 40.0, x);
        assert_eq!(curta.pos, 0.0, "cabe inteira: nao rola");
        // In a list that fits, the bar's strip is a row like any other.
        let t = trilho(AREA);
        curta.passo(AREA, 200.0, 40.0, e(t.x + 2.0, 60.0, true, true, false));
        assert!(curta
            .passo(AREA, 200.0, 40.0, e(t.x + 2.0, 60.0, false, false, true))
            .is_some());
    }
}
