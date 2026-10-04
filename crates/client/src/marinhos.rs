//! Marine voxel rigs use the same greedy mesher as the rest of the game.
use crate::gpu_estatica::{MalhaEstatica, Programa};
use macroquad::models::Mesh;
use macroquad::prelude::*;
use std::cell::RefCell;

// Coral ornaments also use discrete cubes, matching the creature assets.
fn ellipsoid(m: &mut Mesh, p: Vec3, size: Vec3, color: Color) {
    let step = 0.14;
    let mut o = crate::dungeon_cenario::Obra::nova();
    let counts = (size / step).ceil().as_ivec3();
    for x in -counts.x..=counts.x {
        for y in -counts.y..=counts.y {
            for z in -counts.z..=counts.z {
                let q = vec3(x as f32, y as f32, z as f32) * step;
                if (q / size).length_squared() <= 1. {
                    o.caixa(p + q, Vec3::splat(step), color, 0.);
                }
            }
        }
    }
    let offset = m.vertices.len() as u16;
    m.vertices.extend(o.mesh.vertices);
    m.indices
        .extend(o.mesh.indices.into_iter().map(|i| i + offset));
}
fn empty() -> Mesh {
    Mesh {
        vertices: vec![],
        indices: vec![],
        texture: None,
    }
}
struct Model {
    body: Vec<MalhaEstatica>,
    tail: Vec<MalhaEstatica>,
    horizontal: bool,
    pivot: Vec3,
    top: f32,
    dimensions: Vec3,
}
fn asset(kind: u16) -> &'static [u8] {
    match kind {
        70 => include_bytes!("../../../assets/vox/marinhos/70.vox"),
        71 => include_bytes!("../../../assets/vox/marinhos/71.vox"),
        72 => include_bytes!("../../../assets/vox/marinhos/72.vox"),
        73 => include_bytes!("../../../assets/vox/marinhos/73.vox"),
        75 => include_bytes!("../../../assets/vox/marinhos/75.vox"),
        76 => include_bytes!("../../../assets/vox/marinhos/76.vox"),
        90 => include_bytes!("../../../assets/vox/marinhos/90.vox"),
        91 => include_bytes!("../../../assets/vox/marinhos/91.vox"),
        92 => include_bytes!("../../../assets/vox/marinhos/92.vox"),
        _ => unreachable!("not a marine kind"),
    }
}
fn voxel_size(kind: u16) -> f32 {
    match kind {
        90 => 0.18,
        71 | 73 | 75 | 91 | 92 => 0.045,
        _ => 0.055,
    }
}
fn model(kind: u16) -> Model {
    let s = voxel_size(kind);
    let origin = [36., 64., 16.];
    let tail_origin = match kind {
        71 | 73 | 75 | 91 | 92 => [36., 60., 26.],
        76 => origin,
        90 => [36., 44., 16.],
        _ => [36., 46., 16.],
    };
    let pivot = vec3(
        tail_origin[0] - origin[0],
        tail_origin[2] - origin[2],
        tail_origin[1] - origin[1],
    ) * s;
    let mut body = Vec::new();
    let mut tail = Vec::new();
    let mut lo = [usize::MAX; 3];
    let mut hi = [0; 3];
    for (name, part) in
        crate::vox::parse_nomeado(asset(kind)).expect("validated marine voxel asset")
    {
        let (a, b) = part.bounds();
        for i in 0..3 {
            lo[i] = lo[i].min(a[i]);
            hi[i] = hi[i].max(b[i]);
        }
        let o = if name == "tail" { tail_origin } else { origin };
        let meshes = crate::vox::mesh_na_origem(&part, s, o)
            .into_iter()
            .map(MalhaEstatica::nova);
        if name == "tail" {
            tail.extend(meshes);
        } else {
            body.extend(meshes);
        }
    }
    Model {
        body,
        tail,
        horizontal: matches!(kind, 90 | 71 | 73 | 75 | 91 | 92),
        pivot,
        top: (hi[2] as f32 + 1. - 16.) * s,
        dimensions: vec3(
            (hi[0] - lo[0] + 1) as f32,
            (hi[2] - lo[2] + 1) as f32,
            (hi[1] - lo[1] + 1) as f32,
        ) * s,
    }
}
thread_local! {static MODELS:RefCell<std::collections::HashMap<u16,Model>>=RefCell::new(std::collections::HashMap::new());}
pub fn corpo(kind: u16, boss: bool) -> Option<(u16, f32)> {
    let (kind, scale) = if boss {
        let c = shared::bosses::chefe(kind)?;
        let k = match c.corpo {
            shared::bosses::Corpo::Bicho(k) | shared::bosses::Corpo::Gente(k) => k,
            _ => return None,
        };
        (k, c.escala)
    } else {
        (kind, 1.)
    };
    matches!(kind, 70 | 71 | 72 | 73 | 75 | 76 | 90 | 91 | 92 | 93).then_some((kind, scale))
}
pub fn dimensions(kind: u16) -> Vec3 {
    if kind == 93 {
        return Vec3::splat(crate::hidra::HEIGHT);
    }
    MODELS.with(|cache| {
        cache
            .borrow_mut()
            .entry(kind)
            .or_insert_with(|| model(kind))
            .dimensions
    })
}
pub fn altura_ent(e: &crate::world::Ent) -> Option<f32> {
    if e.meta.tag != shared::EntityTag::Enemy {
        return None;
    }
    let (kind, scale) = corpo(e.meta.kind, e.state.flags & shared::ent_flags::BOSS != 0)?;
    if kind == 93 {
        return Some(crate::hidra::HEIGHT * scale);
    }
    Some(
        MODELS.with(|cache| {
            cache
                .borrow_mut()
                .entry(kind)
                .or_insert_with(|| model(kind))
                .top
        }) * scale
            + if kind == 90 { 2. } else { 0.85 },
    )
}
pub fn desenha_ent(e: &crate::world::Ent, p: Vec3) -> bool {
    if e.meta.tag != shared::EntityTag::Enemy {
        return false;
    }
    let Some((kind, scale)) = corpo(e.meta.kind, e.state.flags & shared::ent_flags::BOSS != 0)
    else {
        return false;
    };
    desenha_pose(
        kind,
        p,
        e.yaw,
        e.meta.id.0 as u64,
        scale,
        crate::hidra::pose(e, get_time()),
    )
}
pub fn desenha(kind: u16, p: Vec3, yaw: f32, seed: u64, scale: f32) -> bool {
    desenha_pose(kind, p, yaw, seed, scale, Default::default())
}
fn desenha_pose(
    kind: u16,
    p: Vec3,
    yaw: f32,
    seed: u64,
    scale: f32,
    pose: crate::hidra::Pose,
) -> bool {
    if kind == 93 {
        crate::hidra::draw(p, yaw, seed, scale, pose);
        return true;
    }
    if !matches!(kind, 70 | 71 | 72 | 73 | 75 | 76 | 90 | 91 | 92 | 93) {
        return false;
    }
    let t = get_time() as f32;
    let phase = t * 3.5 + (seed % 113) as f32;
    let alive = 1. - pose.dead;
    let forward = Quat::from_rotation_y(yaw) * Vec3::Z;
    let position = p
        + Vec3::Y
            * ((if kind == 90 { 2.0 } else { 0.85 }) * alive
                + phase.sin() * 0.09 * alive
                + pose.body.sobe
                + pose.body.voa)
        + forward
            * (scale
                * (pose.body.avanca - pose.wind * 0.12 + pose.strike * 0.35 - pose.recoil * 0.12))
        + pose.direction.normalize_or(forward) * pose.body.desloca;
    let transform = Mat4::from_scale_rotation_translation(
        Vec3::splat(scale),
        Quat::from_rotation_y(yaw + pose.body.giro)
            * Quat::from_rotation_x(
                pose.body.pitch - pose.wind * 0.08 + pose.strike * 0.18 + pose.recoil * 0.16,
            )
            * Quat::from_rotation_z(pose.dead * 1.35),
        position,
    );
    MODELS.with(|cache| {
        let mut cache = cache.borrow_mut();
        let m = cache.entry(kind).or_insert_with(|| model(kind));
        let program = Programa::Solido {
            recorte: Vec3::ZERO,
            recorte_z: 0.,
        };
        crate::gpu_estatica::desenha_com_modelo(program, m.body.iter(), transform);
        let angle = phase.sin() * (0.18 + pose.walk * 0.15 + pose.strike * 0.25) * alive;
        let rotation = if m.horizontal {
            Quat::from_rotation_x(angle)
        } else {
            Quat::from_rotation_y(angle)
        };
        crate::gpu_estatica::desenha_com_modelo(
            program,
            m.tail.iter(),
            transform * Mat4::from_translation(m.pivot) * Mat4::from_quat(rotation),
        );
    });
    true
}

thread_local! {
    static CORAIS:RefCell<Vec<MalhaEstatica>>=const {RefCell::new(Vec::new())};
    static SANTUARIO:RefCell<Option<MalhaEstatica>>=const {RefCell::new(None)};
}
pub fn cenario(cam: &Camera3D, chao: &dyn Fn(f32, f32) -> f32) {
    let sanctuary = shared::abissal::arenas()[3];
    if vec2(sanctuary.x, sanctuary.y).distance_squared(vec2(cam.target.x, cam.target.z))
        < 130. * 130.
    {
        SANTUARIO.with(|cache| {
            let mut cache = cache.borrow_mut();
            let mesh = cache.get_or_insert_with(|| {
                let mut o = crate::dungeon_cenario::Obra::nova();
                for i in 0..12 {
                    let a = i as f32 * std::f32::consts::TAU / 12.;
                    let p = vec3(
                        sanctuary.x + a.cos() * 24.,
                        chao(sanctuary.x, sanctuary.y),
                        sanctuary.y + a.sin() * 24.,
                    );
                    let color = Color::from_rgba(130, 167, 164, 255);
                    o.caixa(p + vec3(0., 0.4, 0.), vec3(3., 0.8, 3.), color, a);
                    o.caixa(p + vec3(0., 4., 0.), vec3(1.2, 8., 1.2), color, a);
                    o.caixa(p + vec3(0., 8., 0.), vec3(3., 0.7, 3.), color, a);
                    o.caixa(
                        p + vec3(0., 8.7, 0.),
                        vec3(0.7, 0.7, 0.7),
                        Color::from_rgba(91, 240, 183, 255),
                        a,
                    );
                }
                MalhaEstatica::nova(o.mesh)
            });
            crate::gpu_estatica::desenha(
                Programa::Solido {
                    recorte: Vec3::ZERO,
                    recorte_z: 0.,
                },
                [&*mesh],
            );
        });
    }
    CORAIS.with(|cache| {
        let mut cache = cache.borrow_mut();
        if cache.is_empty() {
            for color in [
                Color::from_rgba(230, 107, 182, 255),
                Color::from_rgba(83, 224, 202, 255),
                Color::from_rgba(137, 155, 247, 255),
            ] {
                let mut mesh = empty();
                for i in 0..6 {
                    let angle = i as f32 * std::f32::consts::TAU / 6.;
                    let axis = vec3(angle.cos(), 0., angle.sin());
                    ellipsoid(
                        &mut mesh,
                        axis * 0.4 + vec3(0., 0.5, 0.),
                        vec3(0.12, 0.6, 0.12),
                        color,
                    );
                    ellipsoid(
                        &mut mesh,
                        axis * 0.65 + vec3(0., 1., 0.),
                        vec3(0.1, 0.4, 0.1),
                        color,
                    );
                    ellipsoid(
                        &mut mesh,
                        axis * 0.9 + vec3(0., 1.25, 0.),
                        Vec3::splat(0.16),
                        color,
                    );
                }
                cache.push(MalhaEstatica::nova(mesh));
            }
        }
        let target = vec2(cam.target.x, cam.target.z);
        for (i, l) in shared::abissal::luzes().iter().enumerate() {
            if !l.coral || vec2(l.pos.x, l.pos.y).distance_squared(target) > 95. * 95. {
                continue;
            }
            let p = vec3(l.pos.x, chao(l.pos.x, l.pos.y), l.pos.y);
            crate::gpu_estatica::desenha_com_modelo(
                Programa::Solido {
                    recorte: Vec3::ZERO,
                    recorte_z: 0.,
                },
                [&cache[i % 3]],
                Mat4::from_translation(p),
            );
        }
    });
    // Large, peaceful silhouettes cruise above the hunting grounds.
    let t = get_time() as f32;
    for i in 0..8 {
        let a = i as f32 * 0.78 + t * 0.018;
        let radius = 180. + i as f32 * 43.;
        let p = vec2(a.cos(), a.sin()) * radius;
        if p.distance_squared(vec2(cam.target.x, cam.target.z)) > 120. * 120. {
            continue;
        }
        desenha(90, vec3(p.x, chao(p.x, p.y) + 12., p.y), -a, i, 1.8);
    }
}

#[cfg(debug_assertions)]
pub async fn previa() {
    let output =
        std::env::var("MMO_PREVIA_SAIDA").unwrap_or_else(|_| "/tmp/tempest-marine-voxels".into());
    std::fs::create_dir_all(&output).unwrap();
    next_frame().await;
    let rt = render_target_ex(
        1280,
        800,
        RenderTargetParams {
            depth: true,
            sample_count: 1,
        },
    );
    crate::render3d::define_alvo(Some(rt.clone()));
    crate::render3d::define_abismo(false);
    crate::gpu_estatica::define_luz_dia(1.);
    crate::gpu_estatica::define_neblina(Vec2::ZERO, 0., 0.);
    let material = crate::render3d::material_solido();
    material.set_uniform("LuzDia", 1.0f32);
    for kind in [70, 71, 72, 73, 75, 76, 90, 91, 92, 93] {
        let distance = if kind == 90 {
            23.
        } else if kind == 93 {
            9.
        } else {
            6.
        };
        let cam = Camera3D {
            position: vec3(distance * 0.7, distance * 0.55, distance),
            target: vec3(0., 1., 0.),
            up: Vec3::Y,
            render_target: Some(rt.clone()),
            aspect: Some(1.6),
            ..Default::default()
        };
        set_camera(&cam);
        clear_background(Color::from_rgba(75, 93, 108, 255));
        macroquad::material::gl_use_material(&material);
        draw_cube(
            vec3(0., -0.2, 0.),
            vec3(40., 0.3, 40.),
            None,
            Color::from_rgba(127, 139, 144, 255),
        );
        desenha(kind, Vec3::ZERO, 0., kind as u64, 1.);
        if kind == 90 {
            draw_cube(
                vec3(-5., 0.9, 5.),
                vec3(0.6, 1.8, 0.4),
                None,
                Color::from_rgba(222, 177, 99, 255),
            );
        }
        macroquad::material::gl_use_default_material();
        unsafe {
            get_internal_gl().flush();
        }
        rt.texture
            .get_texture_data()
            .export_png(&format!("{output}/{kind}.png"));
        next_frame().await;
    }
    for (name, pose) in [
        (
            "hydra-windup",
            crate::hidra::Pose {
                wind: 1.,
                ..Default::default()
            },
        ),
        (
            "hydra-bite",
            crate::hidra::Pose {
                strike: 1.,
                ..Default::default()
            },
        ),
        (
            "hydra-venom",
            crate::hidra::Pose {
                strike: 1.,
                breath: true,
                ..Default::default()
            },
        ),
        (
            "hydra-walk",
            crate::hidra::Pose {
                walk: 1.,
                phase: 1.,
                ..Default::default()
            },
        ),
        (
            "hydra-death",
            crate::hidra::Pose {
                dead: 1.,
                ..Default::default()
            },
        ),
    ] {
        let cam = Camera3D {
            position: vec3(5., 4., 8.),
            target: vec3(0., 1.3, 0.),
            up: Vec3::Y,
            render_target: Some(rt.clone()),
            aspect: Some(1.6),
            ..Default::default()
        };
        set_camera(&cam);
        clear_background(Color::from_rgba(75, 93, 108, 255));
        macroquad::material::gl_use_material(&material);
        draw_cube(
            vec3(0., -0.2, 0.),
            vec3(40., 0.3, 40.),
            None,
            Color::from_rgba(127, 139, 144, 255),
        );
        crate::hidra::draw(Vec3::ZERO, 0., 93, 1., pose);
        macroquad::material::gl_use_default_material();
        unsafe {
            get_internal_gl().flush();
        }
        rt.texture
            .get_texture_data()
            .export_png(&format!("{output}/{name}.png"));
        next_frame().await;
    }
    let charge = crate::chefe_anim::Carga {
        golpe: crate::chefe_anim::Golpe::Varrida,
        dir: Vec2::Y,
        alcance: 0.,
        inicio: 0.,
        carga_s: 1.6,
        impacto: None,
    };
    for frame in 0..32 {
        let cam = Camera3D {
            position: vec3(12., 10., 20.),
            target: vec3(0., 3.5, 0.),
            up: Vec3::Y,
            render_target: Some(rt.clone()),
            aspect: Some(1.6),
            ..Default::default()
        };
        set_camera(&cam);
        clear_background(Color::from_rgba(75, 93, 108, 255));
        macroquad::material::gl_use_material(&material);
        draw_cube(
            vec3(0., -0.2, 0.),
            vec3(40., 0.3, 40.),
            None,
            Color::from_rgba(127, 139, 144, 255),
        );
        let mut pose = crate::hidra::Pose::default();
        crate::hidra::charged_pose(&mut pose, &charge, frame as f64 * 0.09, true);
        crate::hidra::draw(Vec3::ZERO, 0., 93, 2.8, pose);
        macroquad::material::gl_use_default_material();
        unsafe {
            get_internal_gl().flush();
        }
        rt.texture
            .get_texture_data()
            .export_png(&format!("{output}/hydra-attack-{frame:02}.png"));
        next_frame().await;
    }
    crate::render3d::define_alvo(None);
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn marine_assets_are_voxel_rigs_with_separate_swimming_tails() {
        for kind in [70, 71, 72, 73, 75, 76, 90, 91, 92] {
            let parts = crate::vox::parse_nomeado(asset(kind)).unwrap();
            assert_eq!(parts.len(), 2);
            assert!(parts.iter().any(|(n, _)| n == "body"));
            assert!(parts.iter().any(|(n, _)| n == "tail"));
            for (_, part) in parts {
                let meshes = crate::vox::mesh_na_origem(&part, voxel_size(kind), [36., 64., 16.]);
                assert!(!meshes.is_empty());
                for m in meshes {
                    assert!(m.vertices.iter().all(|v| v.position.is_finite()));
                    assert!(m.indices.iter().all(|i| (*i as usize) < m.vertices.len()));
                }
            }
        }
        let parts = crate::vox::parse(asset(90)).unwrap();
        let min = parts.iter().map(|p| p.bounds().0[1]).min().unwrap();
        let max = parts.iter().map(|p| p.bounds().1[1]).max().unwrap();
        assert!((max - min + 1) as f32 * voxel_size(90) > 16.);
    }
}
