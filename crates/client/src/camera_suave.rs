//! SMOOTH camera: the input moves a TARGET and the camera chases it.
//!
//! On the iPhone touch events arrive at an irregular rate — two in one
//! frame, none in the next. Adding the finger's delta straight onto the angle
//! makes the camera move in steps even with no lag at all. Here the finger
//! (and the mouse, and the wheel) moves a target; the camera reaches it by
//! `dt`-dependent exponential approach, which never overshoots and gives the
//! same result at 30 or 120 frames.
//!
//! On release a short inertia is left over: the drag's velocity keeps pushing
//! the target and dies in ~0.35 s.
//!
//! The core does not know macroquad; `main::camera_controles` applies it.
use macroquad::prelude::Vec2;

/// How fast the camera reaches the target (1/s). High enough for the mouse to
///  stay "in hand": in 1/60 s it covers ~26% of the way, in 0.15 s ~95%.
pub const K_CAMERA: f32 = 18.0;
/// Window of the touch delta's average (s). Short: it swallows the event that
/// arrived doubled without delaying the response.
pub const JANELA_DO_FILTRO_S: f32 = 0.05;
/// Constante de decaimento da inercia (s): ~5% da velocidade em 0,35 s.
pub const TAU_INERCIA_S: f32 = 0.117;
/// Below this (rad/s or units/s) the inertia stops.
const INERCIA_MINIMA: f32 = 0.02;

/// Exponential approach from `atual` to `alvo` over `dt`. Never overshoots.
pub fn suaviza(atual: f32, alvo: f32, dt: f32, k: f32) -> f32 {
    let f = 1.0 - (-dt.max(0.0) * k).exp();
    atual + (alvo - atual) * f
}

/// Angular difference by the short way, in [-pi, pi].
pub fn diferenca_angular(de: f32, para: f32) -> f32 {
    let tau = std::f32::consts::TAU;
    let mut d = (para - de) % tau;
    if d > std::f32::consts::PI {
        d -= tau;
    } else if d < -std::f32::consts::PI {
        d += tau;
    }
    d
}

/// The same approach for an angle: never goes round the long side.
pub fn suaviza_angulo(atual: f32, alvo: f32, dt: f32, k: f32) -> f32 {
    let f = 1.0 - (-dt.max(0.0) * k).exp();
    atual + diferenca_angular(atual, alvo) * f
}

/// A short exponential average of the finger's delta, per second. It holds
/// the VELOCITY (px/s), which is what is left as inertia when the finger lifts.
#[derive(Debug, Default, Clone, Copy)]
pub struct FiltroDelta {
    vel: Vec2,
}

impl FiltroDelta {
    /// Entra o delta deste quadro (px) e o `dt`; sai o delta suavizado (px).
    pub fn filtra(&mut self, delta: Vec2, dt: f32) -> Vec2 {
        let dt = dt.max(1e-4);
        let a = 1.0 - (-dt / JANELA_DO_FILTRO_S).exp();
        self.vel += (delta / dt - self.vel) * a;
        self.vel * dt
    }

    /// Velocidade atual (px/s).
    pub fn velocidade(&self) -> Vec2 {
        self.vel
    }

    pub fn zera(&mut self) {
        self.vel = Vec2::ZERO;
    }
}

/// Inertia after release: velocity decaying with friction.
#[derive(Debug, Default, Clone, Copy)]
pub struct Inercia {
    vel: Vec2,
    /// Below this it stops: 5% of the release velocity (~0.35 s with the TAU).
    limite: f32,
}

impl Inercia {
    pub fn solta(&mut self, vel: Vec2) {
        self.vel = vel;
        self.limite = (vel.length() * 0.05).max(INERCIA_MINIMA);
    }

    pub fn para(&mut self) {
        self.vel = Vec2::ZERO;
    }

    pub fn ativa(&self) -> bool {
        self.vel.length() > self.limite.max(INERCIA_MINIMA)
    }

    /// Quanto andar neste quadro; a velocidade decai.
    pub fn passo(&mut self, dt: f32) -> Vec2 {
        if !self.ativa() {
            self.vel = Vec2::ZERO;
            return Vec2::ZERO;
        }
        let d = self.vel * dt;
        self.vel *= (-dt.max(0.0) / TAU_INERCIA_S).exp();
        d
    }
}

/// The camera's targets and what was written last — if other code changes the
/// value (a loaded preference, a reset), the target adopts the new value
/// instead of dragging it back.
#[derive(Debug, Default, Clone, Copy)]
pub struct CameraSuave {
    pub yaw: f32,
    pub ajuste: f32,
    pub zoom: f32,
    escrito: Option<(f32, f32, f32)>,
    pub filtro: FiltroDelta,
    pub inercia: Inercia,
}

impl CameraSuave {
    /// Adopts the current values as the target when something outside changed them.
    pub fn sincroniza(&mut self, yaw: f32, ajuste: f32, zoom: f32) {
        let mudou = match self.escrito {
            None => true,
            Some((y, a, z)) => {
                (y - yaw).abs() > 1e-4 || (a - ajuste).abs() > 1e-4 || (z - zoom).abs() > 1e-4
            }
        };
        if mudou {
            self.yaw = yaw;
            self.ajuste = ajuste;
            self.zoom = zoom;
        }
    }

    /// Um quadro de perseguicao; devolve (yaw, ajuste, zoom) novos.
    pub fn persegue(&mut self, yaw: f32, ajuste: f32, zoom: f32, dt: f32) -> (f32, f32, f32) {
        let n = (
            suaviza_angulo(yaw, self.yaw, dt, K_CAMERA),
            suaviza(ajuste, self.ajuste, dt, K_CAMERA),
            suaviza(zoom, self.zoom, dt, K_CAMERA),
        );
        self.escrito = Some(n);
        n
    }

    /// Records the clamped value that actually stuck (`main` pins the tilt to the
    /// zoom's band after chasing).
    pub fn escreveu(&mut self, yaw: f32, ajuste: f32, zoom: f32) {
        self.escrito = Some((yaw, ajuste, zoom));
    }
}

#[cfg(test)]
mod testes {
    use super::*;
    use macroquad::prelude::vec2;

    #[test]
    fn converge_sem_passar_do_alvo() {
        let mut v = 0.0f32;
        let mut anterior = 0.0;
        for _ in 0..120 {
            v = suaviza(v, 1.0, 1.0 / 60.0, K_CAMERA);
            assert!(v <= 1.0 + 1e-6, "passou do alvo: {v}");
            assert!(v >= anterior, "voltou pra tras");
            anterior = v;
        }
        assert!((v - 1.0).abs() < 1e-3, "nao chegou: {v}");
        // Mesmo tempo, quadros diferentes: mesmo lugar.
        let mut a = 0.0f32;
        for _ in 0..6 {
            a = suaviza(a, 1.0, 0.1 / 6.0, K_CAMERA);
        }
        let b = suaviza(0.0, 1.0, 0.1, K_CAMERA);
        assert!(
            (a - b).abs() < 1e-4,
            "depende da taxa de quadros: {a} vs {b}"
        );
        // A fast response on PC: 0.15 s has already covered ~93%.
        assert!(suaviza(0.0, 1.0, 0.15, K_CAMERA) > 0.9);
    }

    #[test]
    fn angulo_vai_pelo_caminho_curto() {
        let pi = std::f32::consts::PI;
        let d = diferenca_angular(pi - 0.1, -pi + 0.1);
        assert!((d - 0.2).abs() < 1e-4, "cruzar o -pi/pi: {d}");
        let v = suaviza_angulo(pi - 0.1, -pi + 0.1, 1.0, K_CAMERA);
        assert!(
            (diferenca_angular(v, -pi + 0.1)).abs() < 1e-3,
            "chegou pelo lado curto"
        );
    }

    #[test]
    fn filtro_absorve_eventos_irregulares() {
        let mut f = FiltroDelta::default();
        let dt = 1.0 / 60.0;
        // Dedo a 600 px/s, mas os eventos chegam: 20 px, 0, 0, 30 px...
        let brutos = [
            10.0, 0.0, 20.0, 0.0, 10.0, 0.0, 20.0, 0.0, 10.0, 0.0, 20.0, 0.0,
        ];
        let saidas: Vec<f32> = brutos
            .iter()
            .map(|d| f.filtra(vec2(*d, 0.0), dt).x)
            .collect();
        let (mn, mx) = saidas[4..]
            .iter()
            .fold((f32::MAX, f32::MIN), |(a, b), v| (a.min(*v), b.max(*v)));
        assert!(mx - mn < 20.0 * 0.5, "degrau nao foi suavizado: {saidas:?}");
        assert!(saidas.iter().all(|v| *v >= 0.0));
    }

    #[test]
    fn inercia_decai_em_menos_de_meio_segundo() {
        let mut i = Inercia::default();
        i.solta(vec2(2.0, 0.0));
        let mut total = 0.0;
        let mut t = 0.0;
        while i.ativa() && t < 2.0 {
            total += i.passo(1.0 / 60.0).x;
            t += 1.0 / 60.0;
        }
        assert!(t <= 0.5, "inercia durou {t} s");
        assert!(
            total > 0.0 && total < 2.0 * TAU_INERCIA_S * 1.1,
            "deslizou {total}"
        );
    }

    #[test]
    fn valor_mudado_de_fora_vira_alvo() {
        let mut c = CameraSuave::default();
        c.sincroniza(0.0, 0.0, 1.0);
        let (y, a, z) = c.persegue(0.0, 0.0, 1.0, 1.0 / 60.0);
        c.escreveu(y, a, z);
        // A loaded preference changes the zoom: the target adopts it, without pulling back.
        c.sincroniza(y, a, 2.0);
        assert_eq!(c.zoom, 2.0);
        let (_, _, z2) = c.persegue(y, a, 2.0, 1.0 / 60.0);
        assert!((z2 - 2.0).abs() < 1e-6);
    }
}
