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
//! - in a fight it widens gently and looks at the MIDPOINT between the ship
//!   and the target, so both stay in frame together;
//! - it frames a little AHEAD of the bow, where the ship is going.
//!
//! IN A FIGHT IT STOPS FOLLOWING THE HEADING (sea fix, 17/11/2026). The
//! owner: "the camera following the boat turned out really good, but in
//! combat it keeps moving fast and it even makes you dizzy because it keeps
//! spinning". The pilot circles its target (`server/world/piloto_do_mar`),
//! so the heading turns all the time and the follow turned the whole sea
//! round with it. Now, while fighting (`Luta`: a target in reach, AUTO
//! COMBAT on with a target, or a shot of mine in the last seconds), the yaw
//! HOLDS — the frame looks at the midpoint and widens, so the ship and the
//! target stay in view whichever way the circle goes. Only if the target
//! drifts towards the camera's side of the ship does the yaw ease round,
//! never faster than `DERIVA_MAX` (8°/s) and with a slow ramp. When the
//! fight ends it waits `DEPOIS_DA_LUTA_S` and eases back into the heading
//! follow. The hand still overrides at any time.
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
/// How a bigger hull pulls the camera back: (its length over the Sloop's)
/// to this power — since the walkable deck the classes are 7 to 20 u long.
const POR_CLASSE: f32 = 0.6;

/// The pull-back of hull class `classe` (1 for the Sloop).
pub fn recuo_da_classe(classe: usize) -> f32 {
    (shared::conves::meio_comp(classe) / shared::conves::meio_comp(0)).powf(POR_CLASSE)
}
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
/// In a fight the frame's centre (the midpoint, which circles with the
/// ship) settles slower: the view glides instead of chasing.
const K_QUADRO_LUTA: f32 = 1.4;
/// The fastest the yaw drifts in a fight (rad/s, 8°/s).
pub const DERIVA_MAX: f32 = 0.14;
/// How fast that drift's speed itself changes (1/s): a slow ramp, no jolt.
const K_DERIVA: f32 = 0.8;
/// Seconds after a fight before the heading follow comes back...
pub const DEPOIS_DA_LUTA_S: f32 = 2.0;
/// ...and how long it takes to come back to full strength.
const VOLTA_S: f32 = 2.0;
/// The target counts as on the camera's side of the ship (and the yaw
/// drifts) past this angle from the view, either way (rad, ~125°).
const LADO_DO_OLHO: f32 = 2.18;

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
pub fn enquadramento(
    eu: Vec2,
    rumo: Vec2,
    alvo: Option<Vec2>,
    classe: usize,
    retrato: bool,
    zoom: f32,
    ajuste: f32,
) -> Enquadramento {
    let mut fator = zoom * RECUO_MAR * recuo_da_classe(classe);
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
            // The midpoint: the ship and the target the same way off the
            // middle, whichever way the camera faces.
            centro = eu + (a - eu) * 0.5 * ((d - 2.0) / 10.0).clamp(0.0, 1.0);
        }
    }
    let pitch = (PITCH_MAR + ajuste).clamp(PITCH_MIN_MAR, crate::render3d::PITCH_MAX);
    Enquadramento {
        centro,
        fator,
        pitch,
    }
}

/// THE FIGHT, as the camera sees it this frame (`segue`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Luta {
    pub eu: Vec2,
    /// The target, when there is one (a shot with no target still counts
    /// as fighting, for a moment).
    pub alvo: Option<Vec2>,
}

/// Is this a fight for the camera? A live target within `alcance`, AUTO
/// COMBAT on with a target, or one of my shots in the last `recente_s`
/// seconds (`desde_o_tiro`).
pub fn em_luta(
    eu: Vec2,
    alvo: Option<Vec2>,
    alcance: f32,
    auto: bool,
    desde_o_tiro: f32,
    recente_s: f32,
) -> Option<Luta> {
    let perto = alvo.is_some_and(|a| a.distance(eu) <= alcance);
    (perto || (auto && alvo.is_some()) || desde_o_tiro < recente_s).then_some(Luta { eu, alvo })
}

/// The sea camera's own state: the smoothed frame, how long the hand has
/// been off, and the fight's hold.
#[derive(Debug, Clone, Copy)]
pub struct CameraDoMar {
    quadro: Option<Enquadramento>,
    ocioso: f32,
    /// Seconds since the last fight frame (large = no fight in a while).
    desde_a_luta: f32,
    /// The yaw's drift speed in a fight (rad/s), eased.
    deriva: f32,
}

impl Default for CameraDoMar {
    fn default() -> Self {
        Self {
            quadro: None,
            ocioso: 0.0,
            desde_a_luta: 1e3,
            deriva: 0.0,
        }
    }
}

impl CameraDoMar {
    /// Left the sea (or arrived): start fresh, no swing from the old view.
    pub fn zera(&mut self) {
        *self = Self::default();
    }

    /// THE HEADING FOLLOW: the camera's target yaw this frame. `mexeu` =
    /// the hand moved the camera this frame; `velocidade` the hull's;
    /// `luta` the fight, if this frame is one (`em_luta`): then the yaw
    /// holds (see the module's notes).
    pub fn segue(
        &mut self,
        yaw: f32,
        rumo: Vec2,
        mexeu: bool,
        velocidade: f32,
        dt: f32,
        luta: Option<Luta>,
    ) -> f32 {
        if luta.is_some() {
            self.desde_a_luta = 0.0;
        } else {
            self.desde_a_luta += dt;
        }
        if mexeu {
            self.ocioso = 0.0;
            self.deriva = 0.0;
            return yaw;
        }
        self.ocioso += dt;
        if let Some(l) = luta {
            // HOLD, and drift only while the target sits on the camera's
            // side of the ship, slowly, towards where it no longer does.
            let quer = match l.alvo {
                Some(a) if a.distance(l.eu) > 1.0 => {
                    let d = (a - l.eu).normalize();
                    let fi = crate::camera_suave::diferenca_angular(yaw, d.x.atan2(-d.y));
                    if fi.abs() > LADO_DO_OLHO {
                        (fi.abs() - LADO_DO_OLHO + 0.1).min(1.0) * fi.signum()
                    } else {
                        0.0
                    }
                }
                _ => 0.0,
            };
            let alvo_vel = (quer * 0.6).clamp(-DERIVA_MAX, DERIVA_MAX);
            self.deriva += (alvo_vel - self.deriva) * (dt * K_DERIVA).min(1.0);
            return yaw + self.deriva * dt;
        }
        // The fight's drift dies out as it ended.
        self.deriva *= 1.0 - (dt * K_DERIVA * 2.0).min(1.0);
        let yaw = yaw + self.deriva * dt;
        if self.ocioso < OCIOSO_S || velocidade < 1.5 || rumo.length_squared() < 1e-4 {
            return yaw;
        }
        // After a fight: wait, then come back to the heading gently.
        let volta = ((self.desde_a_luta - DEPOIS_DA_LUTA_S) / VOLTA_S).clamp(0.0, 1.0);
        if volta <= 0.0 {
            return yaw;
        }
        suaviza_angulo(yaw, yaw_atras(rumo), dt, K_RUMO * volta * volta)
    }

    /// Fighting now, or in the hold after (the frame glides slower then).
    fn lutando(&self) -> bool {
        self.desde_a_luta < DEPOIS_DA_LUTA_S + VOLTA_S
    }

    /// The frame last drawn (a tap on the world picks through the SAME
    /// camera the player saw).
    pub fn ultimo(&self) -> Option<Enquadramento> {
        self.quadro
    }

    /// The frame this frame: `alvo` eased in, never a jump.
    pub fn quadro(&mut self, alvo: Enquadramento, dt: f32) -> Enquadramento {
        let k = if self.lutando() {
            K_QUADRO_LUTA
        } else {
            K_QUADRO
        };
        let q = match self.quadro {
            Some(q) if q.centro.distance(alvo.centro) < 60.0 => Enquadramento {
                centro: vec2(
                    suaviza(q.centro.x, alvo.centro.x, dt, k),
                    suaviza(q.centro.y, alvo.centro.y, dt, k),
                ),
                fator: suaviza(q.fator, alvo.fator, dt, k),
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
            let cam =
                crate::render3d::camera_no_mar(Vec2::ZERO, 0.0, yaw_atras(rumo), 1.0, PITCH_MAR);
            let olho = vec2(cam.position.x, cam.position.z);
            assert!(
                olho.normalize().dot(rumo) < -0.99,
                "the eye is not astern at {ang}"
            );
        }
    }

    /// Lower and further than the land's; bigger hulls and portrait pull
    /// back; a fight widens and keeps the target nearer the middle.
    #[test]
    fn o_enquadramento_do_mar() {
        let rumo = vec2(0.0, 1.0);
        let base = enquadramento(Vec2::ZERO, rumo, None, 0, false, 1.0, 0.0);
        assert!(
            base.pitch < crate::render3d::pitch_padrao() - 0.3,
            "as steep as the land's"
        );
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
        assert_eq!(
            enquadramento(Vec2::ZERO, rumo, None, 0, false, 1.0, -1.0).pitch,
            PITCH_MIN_MAR
        );
    }

    /// The heading follow waits for the hand to rest, swings slowly, and
    /// never fights a drag.
    #[test]
    fn segue_o_rumo_devagar_e_respeita_a_mao() {
        let mut c = CameraDoMar::default();
        let rumo = vec2(1.0, 0.0);
        assert_eq!(c.segue(0.0, rumo, true, 8.0, 0.1, None), 0.0);
        assert_eq!(
            c.segue(0.0, rumo, false, 8.0, 0.5, None),
            0.0,
            "swung before the hand rested"
        );
        let mut yaw = 0.0;
        for _ in 0..10 {
            yaw = c.segue(yaw, rumo, false, 8.0, 0.1, None);
        }
        let alvo = yaw_atras(rumo);
        assert!(
            yaw.abs() > 0.05 && crate::camera_suave::diferenca_angular(yaw, alvo).abs() > 0.3,
            "not a slow swing: {yaw}"
        );
        for _ in 0..200 {
            yaw = c.segue(yaw, rumo, false, 8.0, 0.1, None);
        }
        assert!(crate::camera_suave::diferenca_angular(yaw, alvo).abs() < 0.01);
        // Lying still: no swing.
        let mut c = CameraDoMar::default();
        for _ in 0..40 {
            assert_eq!(c.segue(0.0, rumo, false, 0.5, 0.1, None), 0.0);
        }
    }

    /// A circle-strafe like the server pilot's (`piloto_do_mar`): `r` u
    /// round the target at `v` u/s. Returns (yaw path length, largest
    /// one-frame step), both in degrees, over `segundos` at 60 fps, the
    /// camera starting behind the heading.
    fn circula(r: f32, v: f32, segundos: f32) -> (f32, f32, CameraDoMar, f32) {
        let mut c = CameraDoMar::default();
        let alvo = vec2(40.0, -12.0);
        let dt = 1.0 / 60.0;
        let w = v / r;
        let (mut yaw, mut caminho, mut passo_max) = (f32::NAN, 0.0f32, 0.0f32);
        for k in 0..(segundos / dt) as i32 {
            let a = k as f32 * dt * w;
            let eu = alvo + vec2(a.cos(), a.sin()) * r;
            let rumo = vec2(-a.sin(), a.cos());
            if yaw.is_nan() {
                yaw = yaw_atras(rumo);
            }
            let luta = em_luta(eu, Some(alvo), 45.0, true, 99.0, 3.0);
            let novo = c.segue(yaw, rumo, false, v, dt, luta);
            let passo = crate::camera_suave::diferenca_angular(yaw, novo)
                .abs()
                .to_degrees();
            caminho += passo;
            passo_max = passo_max.max(passo);
            yaw = novo;
        }
        (caminho, passo_max, c, yaw)
    }

    /// THE FIGHT HOLDS THE YAW (sea fix): circling a target for 10 s, the
    /// camera turns under 20° in all (the heading follow turned it ~300°),
    /// never more than 8°/s, with no jump; the old follow, for the record,
    /// spun with the circle.
    #[test]
    fn na_luta_a_camera_nao_roda() {
        for (r, v) in [(9.0f32, 7.0f32), (14.0, 9.0), (20.0, 11.0)] {
            let (caminho, passo_max, _, _) = circula(r, v, 10.0);
            println!("circle {r} u at {v} u/s: the camera turned {caminho:.1}° in 10 s, largest step {passo_max:.3}°");
            assert!(
                caminho < 20.0,
                "circling at {r} u, {v} u/s: the camera turned {caminho:.1}° in 10 s"
            );
            assert!(
                passo_max <= DERIVA_MAX.to_degrees() / 60.0 + 1e-3,
                "a step of {passo_max:.3}° in one frame"
            );
        }
        // The heading follow alone, the same circle: it spins.
        let mut c = CameraDoMar::default();
        let (alvo, r, v, dt) = (vec2(40.0, -12.0), 14.0f32, 9.0f32, 1.0 / 60.0);
        let mut yaw = yaw_atras(vec2(0.0, 1.0));
        let mut caminho = 0.0;
        for k in 0..600 {
            let a = k as f32 * dt * v / r;
            let rumo = vec2(-a.sin(), a.cos());
            let _ = alvo + vec2(a.cos(), a.sin()) * r;
            let novo = c.segue(yaw, rumo, false, v, dt, None);
            caminho += crate::camera_suave::diferenca_angular(yaw, novo)
                .abs()
                .to_degrees();
            yaw = novo;
        }
        println!("the heading follow alone: {caminho:.0}°");
        assert!(
            caminho > 150.0,
            "the heading follow only turned {caminho:.0}°"
        );
    }

    /// After the fight: the yaw holds for `DEPOIS_DA_LUTA_S`, then eases
    /// back behind the heading without a jump; the hand overrides in a
    /// fight too.
    #[test]
    fn depois_da_luta_volta_ao_rumo_devagar() {
        let (_, _, mut c, mut yaw) = circula(14.0, 9.0, 4.0);
        let rumo = vec2(1.0, 0.0);
        let alvo = yaw_atras(rumo);
        let dt = 1.0 / 60.0;
        let inicio = yaw;
        // The hold.
        for _ in 0..(DEPOIS_DA_LUTA_S / dt) as i32 - 6 {
            yaw = c.segue(yaw, rumo, false, 9.0, dt, None);
        }
        // (Only the fight's own slow drift dying out: a few degrees at most.)
        assert!(
            crate::camera_suave::diferenca_angular(inicio, yaw).abs() < 0.1,
            "moved during the hold"
        );
        // Then back, gently, all the way.
        let mut passo_max = 0.0f32;
        for _ in 0..(12.0 / dt) as i32 {
            let novo = c.segue(yaw, rumo, false, 9.0, dt, None);
            passo_max = passo_max.max(crate::camera_suave::diferenca_angular(yaw, novo).abs());
            yaw = novo;
        }
        assert!(
            crate::camera_suave::diferenca_angular(yaw, alvo).abs() < 0.02,
            "did not come back behind the heading"
        );
        assert!(passo_max < 0.03, "a jump of {passo_max} rad in one frame");
        // The hand, mid-fight: its yaw stands.
        let luta = em_luta(Vec2::ZERO, Some(vec2(10.0, 0.0)), 45.0, false, 99.0, 3.0);
        assert!(luta.is_some());
        assert_eq!(c.segue(1.234, rumo, true, 9.0, dt, luta), 1.234);
        // What counts as a fight.
        assert!(em_luta(Vec2::ZERO, Some(vec2(80.0, 0.0)), 45.0, false, 99.0, 3.0).is_none());
        assert!(em_luta(Vec2::ZERO, Some(vec2(80.0, 0.0)), 45.0, true, 99.0, 3.0).is_some());
        assert!(em_luta(Vec2::ZERO, None, 45.0, false, 1.0, 3.0).is_some());
        assert!(em_luta(Vec2::ZERO, None, 45.0, true, 9.0, 3.0).is_none());
    }
}
