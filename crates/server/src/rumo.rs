//! Where the body LOOKS, on the wire (`EntityState::rumo`).
//!
//! The client derived the facing from velocity alone: anyone outside saw the
//! other player, standing still gathering, attacking or aiming, looking at
//! their last step. The server is the one that knows where the body is
//! turned, so it writes it.
use glam::Vec2;

/// Below this (u/s) the body is still: velocity says nothing about facing.
pub const VEL_MINIMA: f32 = 0.2;

/// A body's facing this tick, in order of what wins:
///
/// 1. `mira` — a POINT it looks at (the node being gathered, the target being
/// attacked);
/// 2. `golpe` — the DIRECTION of a strike in progress (the mob that bites);
/// 3. velocity, while walking;
/// 4. `fixo` — the still facing of someone who does not move (a village NPC);
///
/// and 0 (no facing: the client keeps what it had) if none of that applies.
pub fn escolhe(
    pos: Vec2,
    mira: Option<Vec2>,
    golpe: Option<Vec2>,
    vel: Vec2,
    fixo: Option<f32>,
) -> u8 {
    if let Some(p) = mira {
        let r = shared::rumo_de_dir(p - pos);
        if r != 0 {
            return r;
        }
    }
    if let Some(d) = golpe {
        let r = shared::rumo_de_dir(d);
        if r != 0 {
            return r;
        }
    }
    if vel.length() > VEL_MINIMA {
        return shared::rumo_de_dir(vel);
    }
    fixo.map_or(0, shared::rumo_de_yaw)
}

#[cfg(test)]
mod testes {
    use super::*;

    fn yaw(r: u8) -> f32 {
        shared::yaw_de_rumo(r).expect("sem rumo")
    }

    #[test]
    fn cada_estado_escolhe_o_rumo_certo() {
        let pos = Vec2::new(10.0, 10.0);
        let q = std::f32::consts::FRAC_PI_2;
        // Coletando/atacando: olha pro PONTO, mesmo andando pro outro lado.
        let r = escolhe(
            pos,
            Some(Vec2::new(20.0, 10.0)),
            None,
            Vec2::new(0.0, -3.0),
            None,
        );
        assert!((yaw(r) - q).abs() < 0.03, "olha pro no' (+X)");
        // Mob mordendo: a direcao do golpe vence a velocidade.
        let r = escolhe(
            pos,
            None,
            Some(Vec2::new(0.0, 1.0)),
            Vec2::new(-3.0, 0.0),
            None,
        );
        assert!(yaw(r).abs() < 0.03, "olha pro golpe (+Z)");
        // Walking: where it walks.
        let r = escolhe(pos, None, None, Vec2::new(-2.0, 0.0), None);
        assert!((yaw(r) - 3.0 * q).abs() < 0.03, "olha pra onde anda (-X)");
        // Still with nothing: an NPC keeps its fixed facing; the rest have none.
        assert!((yaw(escolhe(pos, None, None, Vec2::ZERO, Some(q))) - q).abs() < 0.03);
        assert_eq!(escolhe(pos, None, None, Vec2::new(0.05, 0.0), None), 0);
        // Aiming at its own body says nothing: falls through to the rest.
        assert_eq!(escolhe(pos, Some(pos), None, Vec2::ZERO, None), 0);
    }
}
