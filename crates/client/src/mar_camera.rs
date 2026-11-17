//! THE SAILING CAMERA (docs/NAVEGACAO.md, "sea HUD clean-up", 16/11/2026).
//!
//! The owner: the sea used the land camera, looking almost straight down at
//! the ship. At sea the camera now sits LOWER and FURTHER BACK, behind the
//! ship's heading, so the sea ahead, the haze and the islands are in view:
//!
//! - it follows the heading with a LAG (`segue`): a turn swings the view
//!   round slowly, never instantly, and only once the player's hand has
//!   been off the camera for a moment — dragging still orbits, the wheel
//!   and the pinch still zoom;
//! - bigger hulls pull it further back (`classe`), and so does a phone in
//!   portrait, whose narrow screen needs it most;
//! - in a fight it widens gently and leans towards the target, so the ship
//!   and the target stay in frame together;
//! - it frames a little AHEAD of the bow, where the ship is going.
//!
//! The fog at sea already reaches far (`mar_cena::neblina`): the lowest
//! pitch here still meets the water well inside it.
//!
//! Pure maths (no macroquad drawing) so it is tested; `main` applies it.

use macroquad::prelude::*;

use crate::camera_suave::{suaviza, suaviza_angulo};

/// The sea's default pitch (rad, ~32°): the land's sits near 54°.
pub const PITCH_MAR: f32 = 0.56;
/// The lowest the hand may tilt it at sea (rad, ~24°).
pub const PITCH_MIN_MAR: f32 = 0.42;
/// The camera is this much further back at sea than the land's at the same
/// zoom.
const RECUO_MAR: f32 = 1.7;
/// Each hull class past the Sloop adds this to the pull-back.
const POR_CLASSE: f32 = 0.12;
/// Phone portrait: the narrow screen pulls back this much more.
const RETRATO: f32 = 1.45;
/// The most a fight widens the view.
const ABRE_NO_COMBATE: f32 = 0.35;
/// Seconds with the hand off the camera before it follows the heading.
const OCIOSO_S: f32 = 1.2;
/// How fast it swings behind the heading (1/s): slow, a ship's turn.
const K_RUMO: f32 = 1.1;
/// How fast the frame's centre and width settle (1/s).
const K_QUADRO: f32 = 3.5;

/// The yaw that puts the camera BEHIND a heading: `render3d::camera` looks
/// along `(sin yaw, -cos yaw)`, so that is the heading's direction.
pub fn yaw_atras(rumo: Vec2) -> f32 {
    rumo.x.atan2(-rumo.y)
}

/// Where the camera looks and how far back it stands.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Enquadramento {
    /// The point looked at, on the water.
    pub centro: Vec2,
    /// The distance, as a factor over the land's zoom-1 distance.
    pub fator: f32,
    pub pitch: f32,
}

/// THE FRAME a settled camera wants: `zoom` and `ajuste` are the player's
/// own (wheel/pinch and the hand's tilt), `classe` the hull class (0 = the
/// Sloop), `alvo` the selected target.
pub fn enquadramento(eu: Vec2, rumo: Vec2, alvo: Option<Vec2>, classe: usize, retrato: bool, zoom: f32, ajuste: f32) -> Enquadramento {
    let mut fator = zoom * RECUO_MAR * (1.0 + POR_CLASSE * classe as f32);
    if retrato {
        fator *= RETRATO;
    }
    let frente = rumo.normalize_or_zero();
    // A little sea ahead of the bow.
    let mut centro = eu + frente * 2.5 * fator;
    if let Some(a) = alvo {
        let d = a.distance(eu);
        // Near enough to fight: widen with the distance, lean towards it.
        if d < 45.0 {
            let peso = ((d - 6.0) / 30.0).clamp(0.0, 1.0);
            fator *= 1.0 + ABRE_NO_COMBATE * peso;
            centro = eu + (a - eu) * 0.4 * ((d - 2.0) / 10.0).clamp(0.0, 1.0);
        }
    }
    let pitch = (PITCH_MAR + ajuste).clamp(PITCH_MIN_MAR, crate::render3d::PITCH_MAX);
    Enquadramento { centro, fator, pitch }
}

/// The sea camera's own state: the smoothed frame and how long the hand
/// has been off.
#[derive(Debug, Default, Clone, Copy)]
pub struct CameraDoMar {
    quadro: Option<Enquadramento>,
    ocioso: f32,
}

impl CameraDoMar {
    /// Left the sea (or arrived): start fresh, no swing from the old view.
    pub fn zera(&mut self) {
        *self = Self::default();
    }

    /// THE HEADING FOLLOW: the camera's target yaw this frame. `mexeu` =
    /// the hand moved the camera this frame; `velocidade` the hull's.
    pub fn segue(&mut self, yaw: f32, rumo: Vec2, mexeu: bool, velocidade: f32, dt: f32) -> f32 {
        if mexeu {
            self.ocioso = 0.0;
            return yaw;
        }
        self.ocioso += dt;
        if self.ocioso < OCIOSO_S || velocidade < 1.5 || rumo.length_squared() < 1e-4 {
            return yaw;
        }
        suaviza_angulo(yaw, yaw_atras(rumo), dt, K_RUMO)
    }

    /// The frame last drawn (a tap on the world picks through the SAME
    /// camera the player saw).
    pub fn ultimo(&self) -> Option<Enquadramento> {
        self.quadro
    }

    /// The frame this frame: `alvo` eased in, never a jump.
    pub fn quadro(&mut self, alvo: Enquadramento, dt: f32) -> Enquadramento {
        let q = match self.quadro {
            Some(q) if q.centro.distance(alvo.centro) < 60.0 => Enquadramento {
                centro: vec2(suaviza(q.centro.x, alvo.centro.x, dt, K_QUADRO), suaviza(q.centro.y, alvo.centro.y, dt, K_QUADRO)),
                fator: suaviza(q.fator, alvo.fator, dt, K_QUADRO),
                pitch: alvo.pitch,
            },
            _ => alvo,
        };
        self.quadro = Some(q);
        q
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    /// Behind the heading: the eye stands astern, the view runs along the
    /// bow, whichever way the ship points.
    #[test]
    fn a_camera_fica_atras_do_rumo() {
        for a in 0..12 {
            let ang = a as f32 * 0.52;
            let rumo = vec2(ang.sin(), ang.cos());
            let cam = crate::render3d::camera_no_mar(Vec2::ZERO, 0.0, yaw_atras(rumo), 1.0, PITCH_MAR);
            let olho = vec2(cam.position.x, cam.position.z);
            assert!(olho.normalize().dot(rumo) < -0.99, "the eye is not astern at {ang}");
        }
    }

    /// Lower and further than the land's; bigger hulls and portrait pull
    /// back; a fight widens and keeps the target nearer the middle.
    #[test]
    fn o_enquadramento_do_mar() {
        let rumo = vec2(0.0, 1.0);
        let base = enquadramento(Vec2::ZERO, rumo, None, 0, false, 1.0, 0.0);
        assert!(base.pitch < crate::render3d::pitch_padrao() - 0.3, "as steep as the land's");
        assert!(base.fator > 1.0);
        assert!(base.centro.y > 0.0, "does not look ahead of the bow");
        let galeao = enquadramento(Vec2::ZERO, rumo, None, 3, false, 1.0, 0.0);
        assert!(galeao.fator > base.fator);
        let retrato = enquadramento(Vec2::ZERO, rumo, None, 0, true, 1.0, 0.0);
        assert!(retrato.fator > base.fator * 1.3);
        let alvo = vec2(20.0, 0.0);
        let luta = enquadramento(Vec2::ZERO, rumo, Some(alvo), 0, false, 1.0, 0.0);
        assert!(luta.fator > base.fator);
        assert!(luta.centro.distance(alvo) < base.centro.distance(alvo));
        // The hand's tilt still counts, inside the band.
        assert!(enquadramento(Vec2::ZERO, rumo, None, 0, false, 1.0, 0.3).pitch > base.pitch);
        assert_eq!(enquadramento(Vec2::ZERO, rumo, None, 0, false, 1.0, -1.0).pitch, PITCH_MIN_MAR);
    }

    /// The heading follow waits for the hand to rest, swings slowly, and
    /// never fights a drag.
    #[test]
    fn segue_o_rumo_devagar_e_respeita_a_mao() {
        let mut c = CameraDoMar::default();
        let rumo = vec2(1.0, 0.0);
        assert_eq!(c.segue(0.0, rumo, true, 8.0, 0.1), 0.0);
        assert_eq!(c.segue(0.0, rumo, false, 8.0, 0.5), 0.0, "swung before the hand rested");
        let mut yaw = 0.0;
        for _ in 0..10 {
            yaw = c.segue(yaw, rumo, false, 8.0, 0.1);
        }
        let alvo = yaw_atras(rumo);
        assert!(yaw.abs() > 0.05 && crate::camera_suave::diferenca_angular(yaw, alvo).abs() > 0.3, "not a slow swing: {yaw}");
        for _ in 0..200 {
            yaw = c.segue(yaw, rumo, false, 8.0, 0.1);
        }
        assert!(crate::camera_suave::diferenca_angular(yaw, alvo).abs() < 0.01);
        // Lying still: no swing.
        let mut c = CameraDoMar::default();
        for _ in 0..40 {
            assert_eq!(c.segue(0.0, rumo, false, 0.5, 0.1), 0.0);
        }
    }
}
