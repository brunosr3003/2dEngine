//! Zone14's original articulated Hydra: five independent neck/head/jaw chains.
use crate::gpu_estatica::{MalhaEstatica, Programa};
use macroquad::prelude::*;
use std::cell::RefCell;
pub const DATA: &[u8] = include_bytes!("../../../assets/vox/marinhos/hydra_zone14.vox");
pub const HEIGHT: f32 = 2.8;
const UNIT: f32 = HEIGHT / 64.;
const ORIGIN: [f32; 3] = [32., 32., 0.];
struct Part {
    name: String,
    mesh: Vec<MalhaEstatica>,
    pivot: Vec3,
    mouth: Vec3,
}
struct Rig {
    parts: Vec<Part>,
}
fn world(p: [f32; 3]) -> Vec3 {
    vec3(p[0] - ORIGIN[0], p[2] - ORIGIN[2], p[1] - ORIGIN[1]) * UNIT
}
fn load() -> Rig {
    let parts = crate::vox::parse_nomeado(DATA)
        .expect("Zone14 Hydra voxel rig")
        .into_iter()
        .map(|(name, v)| {
            let (a, b) = v.bounds();
            let x = (a[0] + b[0] + 1) as f32 * 0.5;
            let pivot = if name.starts_with("jaw") {
                world([x, a[1] as f32, (b[2] - 1) as f32])
            } else if name.starts_with("head") {
                world([x, a[1] as f32, a[2] as f32])
            } else if name == "tail" {
                world([32., 28., 16.])
            } else {
                world([x, (a[1] + b[1] + 1) as f32 * 0.5, a[2] as f32])
            };
            let mouth = world([x, b[1] as f32, a[2] as f32 + 2.]);
            Part {
                name,
                mesh: crate::vox::mesh_na_origem(&v, UNIT, ORIGIN)
                    .into_iter()
                    .map(MalhaEstatica::nova)
                    .collect(),
                pivot,
                mouth,
            }
        })
        .collect();
    Rig { parts }
}
thread_local! {static RIG:RefCell<Option<Rig>>=const {RefCell::new(None)};}
#[derive(Clone, Copy, Default, Debug)]
pub struct Pose {
    pub walk: f32,
    pub phase: f32,
    pub wind: f32,
    pub strike: f32,
    pub recoil: f32,
    pub dead: f32,
    pub breath: bool,
    pub direction: Vec3,
    pub body: crate::chefe_anim::Ajuste,
}
fn smooth(x: f32) -> f32 {
    let x = x.clamp(0., 1.);
    x * x * (3. - 2. * x)
}
pub fn pose(e: &crate::world::Ent, now: f64) -> Pose {
    let mut p = Pose {
        walk: e.andar,
        phase: e.fase * 0.375,
        recoil: e.ferido.map_or(0., |t| (1. - t / 0.3).clamp(0., 1.)),
        dead: e.morte.map_or(0., |t| smooth(t / 1.2)),
        ..Default::default()
    };
    if p.dead > 0. {
        p.walk = 0.;
        return p;
    }
    if let Some(c) = e.carga_chefe.as_ref() {
        p.body = crate::chefe_anim::ajuste(c, now).unwrap_or_default();
        p.direction = vec3(c.dir.x, 0., c.dir.y);
        p.breath = c.golpe == crate::chefe_anim::Golpe::Investida
            && crate::marinhos::corpo(e.meta.kind, e.state.flags & shared::ent_flags::BOSS != 0)
                .is_some_and(|(kind, _)| kind == 93);
        if p.breath {
            // The line attack is venom breath, so the Hydra stays at its origin.
            p.body.desloca = 0.;
            p.body.avanca *= 0.25;
            p.body.pitch *= 0.25;
        }
        match crate::chefe_anim::tempo(c, now) {
            crate::chefe_anim::Tempo::Prepara(u) => p.wind = smooth(u),
            crate::chefe_anim::Tempo::Golpe(u) => {
                p.wind = 1. - smooth(u);
                p.strike = (u * std::f32::consts::PI).sin();
            }
            crate::chefe_anim::Tempo::Recupera(u) => p.strike = (1. - smooth(u)) * 0.25,
            _ => {}
        }
    } else {
        let (time, impact) = e.ataque_mob.map_or((e.golpe, 0.46), |(_, t, i)| (t, i));
        if time < impact {
            p.wind = smooth(time / impact.max(0.05));
        } else if time < impact + 0.38 {
            p.strike = ((time - impact) / 0.38 * std::f32::consts::PI).sin();
        }
    }
    p
}
fn around(p: Vec3, q: Quat) -> Mat4 {
    Mat4::from_translation(p) * Mat4::from_quat(q) * Mat4::from_translation(-p)
}
fn neck_pivot(index: usize) -> Vec3 {
    world(match index {
        0 => [32., 39., 20.],
        1 => [27., 39., 19.],
        2 => [36., 39., 19.],
        3 => [23., 35., 18.],
        _ => [40., 35., 18.],
    })
}
pub fn draw(p: Vec3, yaw: f32, seed: u64, scale: f32, pose: Pose) {
    let t = get_time() as f32;
    let alive = 1. - pose.dead;
    let bob = (t * 1.3 + seed as f32 % 17.).sin() * 0.018 * alive;
    let h = HEIGHT * scale;
    let forward = Quat::from_rotation_y(yaw) * Vec3::Z;
    let position = p
        + Vec3::Y * (h * (bob + pose.body.sobe + pose.body.voa - pose.dead * 0.16))
        + forward * (h * (pose.body.avanca + pose.strike * 0.04 - pose.recoil * 0.025))
        + pose.direction.normalize_or(forward) * pose.body.desloca;
    let rotation = Quat::from_rotation_y(yaw + pose.body.giro)
        * Quat::from_rotation_x(pose.body.pitch + pose.recoil * 0.07)
        * Quat::from_rotation_z(pose.dead * 1.1);
    let root = Mat4::from_scale_rotation_translation(
        vec3(scale, scale * (1. - pose.body.agacha), scale),
        rotation,
        position,
    );
    RIG.with(|cache| {
        let mut cache = cache.borrow_mut();
        let rig = cache.get_or_insert_with(load);
        let necks: [Mat4; 5] = std::array::from_fn(|i| {
            let wave = t * 0.7 + i as f32 * 1.7;
            let yaw = wave.sin() * 0.045 * alive;
            let pitch = wave.cos() * 0.025 * alive - pose.wind * 0.12
                + pose.strike * 0.22
                + pose.dead * 0.45;
            around(
                neck_pivot(i),
                Quat::from_rotation_y(yaw) * Quat::from_rotation_x(pitch),
            )
        });
        let program = Programa::Solido {
            recorte: Vec3::ZERO,
            recorte_z: 0.,
        };
        for part in &rig.parts {
            let mut local = Mat4::IDENTITY;
            if let Some(i) = part
                .name
                .rsplit('_')
                .next()
                .and_then(|s| s.parse::<usize>().ok())
                .filter(|i| (1..=5).contains(i))
                .map(|i| i - 1)
            {
                if part.name.starts_with("neck") {
                    local = necks[i];
                } else {
                    let head = rig
                        .parts
                        .iter()
                        .find(|p| p.name == format!("head_{}", i + 1))
                        .unwrap();
                    let hp = (t * 0.95 + i as f32 * 2.1).sin() * 0.035 * alive - pose.wind * 0.1
                        + pose.strike * 0.3;
                    local = necks[i] * around(head.pivot, Quat::from_rotation_x(hp));
                    if part.name.starts_with("jaw") {
                        let open = if pose.breath {
                            pose.wind * 0.65 + pose.strike * 0.8
                        } else {
                            pose.wind * 0.8 - pose.strike * 0.65
                        };
                        let jaw = (0.045 * alive + open).clamp(0., 0.95) + pose.dead * 0.25;
                        local *= around(part.pivot, Quat::from_rotation_x(jaw));
                        if pose.breath && pose.strike > 0.15 && alive > 0.9 {
                            let mouth = (root * local).transform_point3(part.mouth);
                            for k in 0..5 {
                                let f = (t * 2.5 + k as f32 * 0.19).fract();
                                let q = mouth
                                    + forward * (f * 6. * pose.strike)
                                    + vec3((f * 9. + i as f32).sin() * 0.12, -f * 0.35, 0.);
                                draw_cube(
                                    q,
                                    Vec3::splat(0.11 + (1. - f) * 0.1),
                                    None,
                                    Color::from_rgba(87, 235, 107, 255),
                                );
                            }
                        }
                    }
                }
            } else if part.name == "tail" {
                local = around(
                    part.pivot,
                    Quat::from_rotation_y(
                        (t * 0.8).sin() * 0.08 * alive + pose.wind * 0.25 - pose.strike * 0.45,
                    ),
                );
            } else if part.name.starts_with("paw") {
                let diagonal =
                    part.name.ends_with("front_right") || part.name.ends_with("back_left");
                let phase = pose.phase + if diagonal { 0. } else { std::f32::consts::PI };
                let stride = phase.sin() * 0.13 * pose.walk;
                let lift = phase.cos().max(0.) * 0.1 * pose.walk;
                local = Mat4::from_translation(vec3(0., lift, stride));
            }
            crate::gpu_estatica::desenha_com_modelo(program, part.mesh.iter(), root * local);
        }
    });
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn original_hydra_has_all_five_articulated_chains_and_paws() {
        let parts = crate::vox::parse_nomeado(DATA).unwrap();
        assert_eq!(parts.len(), 21);
        for i in 1..=5 {
            for name in [format!("neck_{i}"), format!("head_{i}"), format!("jaw_{i}")] {
                assert!(parts.iter().any(|(n, _)| *n == name));
            }
        }
        for name in [
            "body",
            "tail",
            "paw_front_right",
            "paw_front_left",
            "paw_back_right",
            "paw_back_left",
        ] {
            assert!(parts.iter().any(|(n, _)| n == name));
        }
        for (_, part) in parts {
            let meshes = crate::vox::mesh_na_origem(&part, UNIT, ORIGIN);
            assert!(!meshes.is_empty());
            for mesh in meshes {
                assert!(mesh.vertices.iter().all(|v| v.position.is_finite()));
                assert!(mesh
                    .indices
                    .iter()
                    .all(|i| (*i as usize) < mesh.vertices.len()));
            }
        }
    }
}
