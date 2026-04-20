//! Re-export do hecs + sistemas helpers compartilhaveis cliente/servidor.

pub use hecs::{Entity, World};

use shared::{Position, Velocity};

/// Avanca todas as entidades com (Position, Velocity) por `dt`.
pub fn integrate_motion(world: &mut World, dt: f32) {
    for (_id, (pos, vel)) in world.query_mut::<(&mut Position, &Velocity)>() {
        pos.0 += vel.0 * dt;
    }
}
