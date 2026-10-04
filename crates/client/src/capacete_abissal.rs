//! Transparent 3D diving bubble, attached to the animated head bone.
use crate::gpu_estatica::{MalhaEstatica, Programa};
use macroquad::prelude::*;
use std::{cell::RefCell, collections::HashMap};

#[derive(Clone, Copy, Debug)]
struct Fit {
    center: Vec3,
    radius: Vec3,
}
#[derive(Clone, Copy)]
struct Helmet {
    head: Mat4,
    fit: Fit,
}
thread_local! {
    static HELMETS: RefCell<Vec<Helmet>> = const { RefCell::new(Vec::new()) };
    static FITS: RefCell<HashMap<(usize,usize),Fit>> = RefCell::new(HashMap::new());
    static COLLAR: std::cell::OnceCell<Vec<MalhaEstatica>> = const { std::cell::OnceCell::new() };
    static GLASS: std::cell::OnceCell<Material> = const { std::cell::OnceCell::new() };
}
pub fn inicia_frame() {
    HELMETS.with(|h| h.borrow_mut().clear());
}
fn fit(head: &[Mesh], hair: &[Mesh]) -> Fit {
    let mut lo = Vec3::splat(f32::INFINITY);
    let mut hi = Vec3::splat(f32::NEG_INFINITY);
    for v in head.iter().chain(hair).flat_map(|m| &m.vertices) {
        lo = lo.min(v.position);
        hi = hi.max(v.position);
    }
    if !lo.is_finite() {
        lo = vec3(-0.16, 0.04, -0.16);
        hi = vec3(0.16, 0.36, 0.16);
    }
    Fit {
        center: (lo + hi) * 0.5,
        radius: ((hi - lo) * 0.875 + Vec3::splat(0.055)).max(vec3(0.36, 0.40, 0.36)),
    }
}
pub fn registra(head: Mat4, face: &[Mesh], hair: &[Mesh]) {
    let fit = FITS.with(|cache| {
        *cache
            .borrow_mut()
            .entry((face.as_ptr() as usize, hair.as_ptr() as usize))
            .or_insert_with(|| fit(face, hair))
    });
    HELMETS.with(|h| h.borrow_mut().push(Helmet { head, fit }));
    // Opaque stepped collar uses the same cube geometry and light as the rig.
    COLLAR.with(|cache| {
        let meshes = cache.get_or_init(|| {
            let mut o = crate::dungeon_cenario::Obra::nova();
            let s = 0.04;
            for x in -9..=9 {
                for z in -9..=9 {
                    let p = vec3(x as f32 * s, 0., z as f32 * s);
                    let r = vec2(p.x, p.z).length();
                    if (0.28..0.35).contains(&r) {
                        o.caixa(p, vec3(s, 0.06, s), Color::from_rgba(62, 94, 107, 255), 0.);
                        o.caixa(
                            p + Vec3::Y * 0.04,
                            vec3(s, 0.02, s),
                            Color::from_rgba(177, 151, 96, 255),
                            0.,
                        );
                    }
                }
            }
            for side in [-1., 1.] {
                let p = vec3(side * 0.34, 0., 0.);
                o.caixa(
                    p,
                    vec3(0.08, 0.10, 0.10),
                    Color::from_rgba(70, 105, 117, 255),
                    0.,
                );
                o.caixa(
                    p + vec3(0., 0.025, 0.056),
                    vec3(0.04, 0.04, 0.016),
                    Color::from_rgba(88, 211, 214, 255),
                    0.,
                );
            }
            vec![MalhaEstatica::nova(o.mesh)]
        });
        crate::gpu_estatica::desenha_com_modelo(
            Programa::Solido {
                recorte: Vec3::ZERO,
                recorte_z: 0.,
            },
            meshes.iter(),
            head,
        );
    });
}
fn glass() -> Material {
    use macroquad::miniquad::{
        BlendFactor, BlendState, BlendValue, Comparison, Equation, PipelineParams,
    };
    GLASS.with(|m| {
        m.get_or_init(|| {
            load_material(
                ShaderSource::Glsl {
                    vertex: r#"#version 100
            attribute vec3 position; attribute vec4 color0;
            uniform mat4 Model; uniform mat4 Projection;
            varying lowp vec4 cor;
            void main() { gl_Position=Projection*Model*vec4(position,1.0); cor=color0/255.0; }"#,
                    fragment: r#"#version 100
            varying lowp vec4 cor;
            void main() { if(cor.a<=0.002) discard; gl_FragColor=cor; }"#,
                },
                MaterialParams {
                    pipeline_params: PipelineParams {
                        depth_test: Comparison::LessOrEqual,
                        depth_write: false,
                        cull_face: macroquad::miniquad::CullFace::Nothing,
                        color_blend: Some(BlendState::new(
                            Equation::Add,
                            BlendFactor::Value(BlendValue::SourceAlpha),
                            BlendFactor::OneMinusValue(BlendValue::SourceAlpha),
                        )),
                        ..Default::default()
                    },
                    ..Default::default()
                },
            )
            .expect("diving helmet glass")
        })
        .clone()
    })
}
fn shell(h: Helmet, cam: &Camera3D) -> Mesh {
    const LAT: usize = 14;
    const LON: usize = 32;
    let mut mesh = Mesh {
        vertices: Vec::with_capacity((LAT + 1) * (LON + 1)),
        indices: Vec::with_capacity(LAT * LON * 6),
        texture: None,
    };
    let normal_matrix = h.head.inverse().transpose();
    let min = ((0.025 - h.fit.center.y) / h.fit.radius.y)
        .clamp(-0.98, 0.2)
        .asin();
    let point = |lat: f32, lon: f32| {
        let y = h.fit.center.y + lat.sin() * h.fit.radius.y;
        let u = ((y - 0.025) / (h.fit.center.y - 0.025).max(0.05)).clamp(0., 1.);
        let blend = u * u * (3. - 2. * u);
        let rx = 0.33 + (lat.cos() * h.fit.radius.x - 0.33) * blend;
        let rz = 0.33 + (lat.cos() * h.fit.radius.z - 0.33) * blend;
        vec3(
            h.fit.center.x * blend + lon.cos() * rx,
            y,
            h.fit.center.z * blend + lon.sin() * rz,
        )
    };
    for i in 0..=LAT {
        for j in 0..=LON {
            let lat = min + (std::f32::consts::FRAC_PI_2 - min) * i as f32 / LAT as f32;
            let lon = j as f32 / LON as f32 * std::f32::consts::TAU;
            let n = vec3(lat.cos() * lon.cos(), lat.sin(), lat.cos() * lon.sin());
            let p = h.head.transform_point3(point(lat, lon));
            let along_lat = point(lat + 0.001, lon) - point(lat - 0.001, lon);
            let along_lon = point(lat, lon + 0.001) - point(lat, lon - 0.001);
            let local_normal = along_lat.cross(along_lon).normalize_or(n / h.fit.radius);
            let normal = normal_matrix
                .transform_vector3(local_normal)
                .normalize_or_zero();
            let eye = (cam.position - p).normalize_or_zero();
            let facing = normal.dot(eye);
            let rim = (1. - facing.max(0.)).powi(4);
            let light = (eye + vec3(-0.35, 0.85, 0.20)).normalize_or_zero();
            let highlight = normal.dot(light).max(0.).powi(70);
            let alpha = 0.008 + rim * 0.14 + highlight * 0.26;
            let color = Color::new(0.62 + highlight * 0.30, 0.88 + highlight * 0.1, 0.94, alpha);
            let mut v = Vertex::new2(p, Vec2::ZERO, color);
            v.normal = normal.extend(facing);
            mesh.vertices.push(v);
        }
    }
    for i in 0..LAT {
        for j in 0..LON {
            let a = (i * (LON + 1) + j) as u16;
            let b = a + (LON + 1) as u16;
            // Draw the near shell only: overlapping front/back layers used to
            // bleach the face. Actual depth testing hides glass behind the head.
            let facing = [a, a + 1, b, b + 1]
                .iter()
                .map(|&k| mesh.vertices[k as usize].normal.w)
                .sum::<f32>()
                * 0.25;
            if facing > 0. {
                mesh.indices
                    .extend_from_slice(&[a, b, a + 1, a + 1, b, b + 1]);
            }
        }
    }
    mesh
}
pub fn desenha(cam: &Camera3D) {
    HELMETS.with(|h| {
        let mut helmets = h.borrow().clone();
        helmets.sort_by(|a, b| {
            b.head
                .transform_point3(b.fit.center)
                .distance_squared(cam.position)
                .total_cmp(
                    &a.head
                        .transform_point3(a.fit.center)
                        .distance_squared(cam.position),
                )
        });
        gl_use_material(&glass());
        for h in helmets {
            draw_mesh(&shell(h, cam));
        }
        gl_use_default_material();
    });
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn glass_is_a_spatial_shell_with_a_clear_face_and_encloses_head() {
        let f = fit(&[], &[]);
        let cam = Camera3D {
            position: vec3(0., 0.2, 3.),
            ..Default::default()
        };
        let mesh = shell(
            Helmet {
                head: Mat4::IDENTITY,
                fit: f,
            },
            &cam,
        );
        assert!(!mesh.indices.is_empty());
        assert!(mesh.vertices.iter().all(|v| v.position.is_finite()));
        let zmin = mesh
            .vertices
            .iter()
            .map(|v| v.position.z)
            .fold(f32::INFINITY, f32::min);
        let zmax = mesh
            .vertices
            .iter()
            .map(|v| v.position.z)
            .fold(f32::NEG_INFINITY, f32::max);
        assert!(
            zmax - zmin > 0.65,
            "helmet must have real depth rather than be a disc"
        );
        let front = mesh
            .vertices
            .iter()
            .max_by(|a, b| a.position.z.total_cmp(&b.position.z))
            .unwrap();
        assert!(front.color[3] < 16, "front glass must preserve the face");
        for x in [-0.16, 0.16] {
            for y in [0.04, 0.36] {
                for z in [-0.16, 0.16] {
                    assert!(((vec3(x, y, z) - f.center) / f.radius).length() < 1.);
                }
            }
        }
    }
}

#[cfg(debug_assertions)]
pub async fn previa(vox: &crate::vox::VoxCache) {
    let output = std::env::var("MMO_PREVIA_SAIDA").unwrap_or_else(|_| "/tmp/tempest-helmet".into());
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
    crate::render3d::define_abismo(true);
    let material = crate::render3d::material_solido();
    material.set_uniform("LuzDia", -2.0f32);
    crate::gpu_estatica::define_luz_dia(-2.);
    crate::gpu_estatica::define_neblina(Vec2::ZERO, 0., 0.);
    let mut world = crate::world::World::default();
    let id = shared::EntityId(1);
    let meta = shared::EntityMeta {
        id,
        tag: shared::EntityTag::Player,
        name: None,
        hp_max: 100,
        faction: None,
        kind: 0,
        nivel: 100,
        desafio: None,
        aparencia: 0,
        skins: 0,
        auras: 0,
        pk: Default::default(),
    };
    let state = shared::EntityState::quantize(id, ::glam::Vec2::ZERO, ::glam::Vec2::ZERO, 100, 0);
    world.apply(vec![meta], vec![state], &[]);
    for (name, eye, target, walking, air, hat) in [
        (
            "front",
            vec3(0.1, 2., 3.4),
            vec3(0., 1., 0.),
            false,
            0.,
            false,
        ),
        (
            "head",
            vec3(0.4, 1.9, 1.4),
            vec3(0., 1.53, 0.),
            false,
            0.,
            false,
        ),
        (
            "side",
            vec3(3., 2., 0.4),
            vec3(0., 1., 0.),
            false,
            0.,
            false,
        ),
        (
            "walking",
            vec3(0.8, 2., 3.4),
            vec3(0., 1., 0.),
            true,
            0.,
            false,
        ),
        (
            "jump",
            vec3(0.8, 3., 3.4),
            vec3(0., 2., 0.),
            false,
            1.,
            false,
        ),
        (
            "hat",
            vec3(0.4, 1.9, 1.6),
            vec3(0., 1.53, 0.),
            false,
            0.,
            true,
        ),
    ] {
        for frame in 0..24 {
            world.tick(1. / 60., &|_, _, _| 0.);
            for e in world.ents.values_mut() {
                e.render_y = air;
                e.yaw = 0.;
                e.andar = if walking { 1. } else { 0. };
                e.fase = if walking { 1.3 } else { 0. };
                e.meta.aparencia = if hat {
                    shared::aparencia::Aparencia {
                        cabelo: 6,
                        ..Default::default()
                    }
                    .empacota()
                } else {
                    0
                };
            }
            let mut vista = crate::render3d::Vista::nova(Vec2::ZERO, 0., 0., 0.8, 0., &|_, _| 0.);
            vista.cam.position = eye;
            vista.cam.target = target;
            vista.cam.render_target = Some(rt.clone());
            vista.cam.aspect = Some(1.6);
            set_camera(&vista.cam);
            clear_background(Color::from_rgba(20, 58, 73, 255));
            gl_use_material(&material);
            draw_cube(
                vec3(0., -0.15, 0.),
                vec3(30., 0.2, 30.),
                None,
                Color::from_rgba(88, 112, 115, 255),
            );
            crate::render3d::draw_entities(&mut world, vox, None, &vista);
            gl_use_default_material();
            desenha(&vista.cam);
            if frame == 23 {
                unsafe {
                    get_internal_gl().flush();
                }
                rt.texture
                    .get_texture_data()
                    .export_png(&format!("{output}/{name}.png"));
            }
            next_frame().await;
        }
    }
    crate::render3d::define_alvo(None);
}
