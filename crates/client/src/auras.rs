//! Auras locais por peça, desenhadas em lote depois dos corpos opacos.
use macroquad::prelude::*;
use macroquad::material::{Material, gl_use_material, gl_use_default_material};
use std::{cell::RefCell, collections::HashMap, f32::consts::TAU};
#[derive(Clone, Copy)]
struct Efeito { mat: Mat4, centro: Vec3, raio: Vec3, grau: u8, tier: u8, refino: u8, fase: f32, perto: bool, distancia: f32 }
thread_local! {
    static FILA: RefCell<Vec<Efeito>> = const { RefCell::new(Vec::new()) };
    static MATERIAL: RefCell<Option<Material>> = const { RefCell::new(None) };
    static RASTROS: RefCell<HashMap<(u32, usize), (Color, Vec<(f32, Vec3, Vec3)>)>> = RefCell::new(HashMap::new());
    static LIMITES: RefCell<HashMap<String, (Vec3, Vec3)>> = RefCell::new(HashMap::new());
}
pub fn cor(grau: u8) -> Color {
    match grau {
        2 => Color::from_rgba(74, 235, 123, 255),
        3 => Color::from_rgba(66, 159, 255, 255),
        4 => Color::from_rgba(186, 94, 255, 255),
        _ => Color::from_rgba(255, 155, 48, 255),
    }
}
fn por(bits: u64, slot: usize, mat: Mat4, centro: Vec3, raio: Vec3, fase: f32, perto: bool) {
    let Some((grau, tier, refino)) = shared::auras::peca(bits, slot) else { return };
    FILA.with(|f| f.borrow_mut().push(Efeito { mat, centro, raio, grau, tier, refino, fase, perto, distancia: 0.0 }));
}
pub fn personagem(bits: u64, mats: &[Mat4; crate::rig::N], armas: &[(&str, Mat4)],
    vox: &crate::vox::VoxCache, distancia: f32, id: u32, ferramenta: bool, atacando: bool) {
    if bits == 0 || distancia > 38.0 { return }
    let inicio = FILA.with(|f| f.borrow().len());
    let perto = distancia < 18.0;
    let fase = id as f32 * 0.618;
    for (indice, (nome, mat)) in armas.iter().enumerate() {
        if ferramenta || matches!(*nome, "bainha" | "coldre") { continue }
        let limites = LIMITES.with(|cache| {
            let mut cache = cache.borrow_mut();
            if let Some(v) = cache.get(*nome) { return Some(*v) }
            let meshes = vox.arma(nome)?;
            let mut min = Vec3::splat(f32::INFINITY);
            let mut max = Vec3::splat(f32::NEG_INFINITY);
            for v in meshes.iter().flat_map(|m| &m.vertices) {
                min = min.min(v.position); max = max.max(v.position);
            }
            if !min.is_finite() { return None }
            let v = ((min + max) * 0.5, ((max - min) * 0.5 + Vec3::splat(0.035)).max(Vec3::splat(0.06)));
            cache.insert((*nome).to_string(), v); Some(v)
        });
        if let Some((c, r)) = limites {
            let slot = if *nome == "escudo" { 1 } else { 0 };
            por(bits, slot, *mat, c, r, fase, perto);
            if perto && atacando && slot == 0 {
                if let Some((grau, tier, refino)) = shared::auras::peca(bits, slot).filter(|(_,t,_)| *t >= 3) {
                    let mut eixo = Vec3::ZERO;
                    let maior = if r.z > r.y && r.z > r.x {2} else if r.x > r.y {0} else {1};
                    eixo[maior] = r[maior];
                    RASTROS.with(|cache| {
                        let mut cache = cache.borrow_mut();
                        let (cor, pontos) = cache.entry((id, indice)).or_insert_with(|| (self::cor(grau), Vec::new()));
                        *cor = self::cor(grau); cor.a = 0.22 * shared::auras::intensidade(tier, refino);
                        pontos.push((get_time() as f32, mat.transform_point3(c-eixo), mat.transform_point3(c+eixo)));
                        if pontos.len() > 10 { pontos.remove(0); }
                    });
                }
            }
        }
    }
    // Armas mágicas não têm lâmina: a aura acompanha as palmas.
    if armas.is_empty() && !ferramenta {
        for p in crate::rig::palmas(mats, crate::render3d::VOXEL) {
            por(bits, 0, Mat4::from_translation(p), Vec3::ZERO, Vec3::splat(0.13), fase, perto);
        }
    }
    if !armas.iter().any(|(nome, _)| *nome == "escudo") {
        por(bits, 1, crate::rig::pulsos(mats, crate::render3d::VOXEL)[1], Vec3::ZERO,
            Vec3::splat(0.10), fase+0.5, perto);
    }
    por(bits, 2, mats[0], vec3(0.0, 0.19, 0.0), vec3(0.30, 0.36, 0.19), fase+1.0, perto);
    for x in [-0.19, 0.19] {
        por(bits, 3, mats[1], vec3(x, 0.03, 0.0), Vec3::splat(0.055), fase+2.0, perto);
    }
    por(bits, 4, mats[0], vec3(0.0, 0.37, 0.16), vec3(0.11, 0.09, 0.055), fase+3.0, perto);
    for mat in crate::rig::pulsos(mats, crate::render3d::VOXEL) {
        por(bits, 5, mat, Vec3::ZERO, vec3(0.09, 0.06, 0.09), fase+4.0, perto);
    }
    por(bits, 6, mats[0], vec3(0.0, -0.015, 0.0), vec3(0.27, 0.07, 0.17), fase+5.0, perto);
    FILA.with(|f| { for e in &mut f.borrow_mut()[inicio..] { e.distancia = distancia; } });
}
fn quad(mesh: &mut Mesh, p: [Vec3;4], cor: Color) {
    let b = mesh.vertices.len() as u16;
    for position in p { mesh.vertices.push(Vertex { position, uv: Vec2::ZERO, color: cor.into(), normal: Vec4::ZERO }); }
    mesh.indices.extend_from_slice(&[b,b+1,b+2,b,b+2,b+3]);
}
pub fn desenha() {
    FILA.with(|fila| {
        let mut fila = fila.borrow_mut();
        if fila.is_empty() { return }
        MATERIAL.with(|m| {
            let mut m = m.borrow_mut();
            gl_use_material(m.get_or_insert_with(crate::habilidades_vfx::material_com_oclusao));
        });
        let mut mesh = Mesh { vertices: Vec::with_capacity(4096), indices: Vec::with_capacity(6144), texture: None };
        let tempo = get_time() as f32;
        fila.sort_by(|a,b| a.distancia.total_cmp(&b.distancia));
        // Orçamento global: prioriza o próprio personagem e os próximos.
        for e in fila.drain(..).take(96) {
            let intensidade = shared::auras::intensidade(e.tier, e.refino);
            let pulso = 0.82 + 0.18 * (tempo * (1.1 + intensidade * 0.35) + e.fase).sin();
            let mut c = cor(e.grau);
            let n = if e.perto { 24 } else { 12 };
            let r = e.raio * ([1.03, 1.12, 1.28, 1.48][e.tier as usize-1] + if e.refino >= 5 {0.025} else {0.0});
            let principal = if r.z > r.y && r.z > r.x { 2 } else if r.x > r.y { 0 } else { 1 };
            // Dois arcos finos abraçam a silhueta da peça, com halo externo suave.
            for plano in 0..e.tier as usize {
                let outro = (principal + 1 + plano % 2) % 3;
                for (largura, alfa) in [(0.055 * intensidade, 0.065), (0.020 * intensidade, 0.09), (0.004 * intensidade, 0.20)] {
                    c.a = alfa * pulso * (0.60 + intensidade * 0.45);
                    for i in 0..n {
                        let mut cor_arco = c;
                        let a = i as f32 / n as f32 * TAU;
                        cor_arco.a *= 0.22 + 0.78 * (a - tempo * 0.8 - e.fase - plano as f32).sin().max(0.0);
                        let ponto = |passo: usize, fora: f32| {
                            let a = passo as f32 / n as f32 * TAU + plano as f32 * 0.45;
                            let mut q = Vec3::ZERO;
                            q[principal] = a.cos() * (r[principal] + fora);
                            q[outro] = a.sin() * (r[outro] + fora);
                            q[(3-principal-outro)%3] = a.sin()*r[(3-principal-outro)%3]*(plano as f32 * 0.17);
                            e.mat.transform_point3(e.centro + q)
                        };
                        quad(&mut mesh, [ponto(i,largura), ponto(i+1,largura), ponto(i+1,-largura), ponto(i,-largura)], cor_arco);
                    }
                }
            }
            let particulas = if e.perto { [1,3,6,10][e.tier as usize-1] + if e.refino >= 5 {1} else {0} + if e.refino >= 10 {1} else {0} } else { 0 };
            for i in 0..particulas {
                let t = (tempo * 0.45 + i as f32 * 0.618 + e.fase).fract();
                let a = tempo * 0.8 + i as f32 * 2.399 + e.fase;
                let q = e.centro + vec3(a.cos()*r.x, (t*2.0-1.0)*r.y, a.sin()*r.z);
                let tam = (0.008 + e.tier as f32*0.003) * intensidade * (t * std::f32::consts::PI).sin();
                c.a = 0.72 * pulso;
                let p = |v| e.mat.transform_point3(q+v);
                quad(&mut mesh, [p(vec3(-tam,0.0,0.0)),p(vec3(0.0,tam*2.0,0.0)),p(vec3(tam,0.0,0.0)),p(vec3(0.0,-tam*2.0,0.0))], c);
            }
            if mesh.vertices.len() > 1000 { draw_mesh(&mesh); mesh.vertices.clear(); mesh.indices.clear(); }
        }
        if !mesh.vertices.is_empty() { draw_mesh(&mesh); mesh.vertices.clear(); mesh.indices.clear(); }
        RASTROS.with(|cache| {
            let mut cache = cache.borrow_mut();
            cache.retain(|_, (c, pts)| {
                pts.retain(|p| tempo - p.0 < 0.16);
                for par in pts.windows(2) {
                    let mut cor = *c; cor.a *= (1.0 - (tempo - par[0].0) / 0.16).max(0.0);
                    if par[0].2.distance(par[1].2) < 1.5 {
                        quad(&mut mesh, [par[0].1, par[0].2, par[1].2, par[1].1], cor);
                    }
                }
                if mesh.vertices.len() > 1000 { draw_mesh(&mesh); mesh.vertices.clear(); mesh.indices.clear(); }
                !pts.is_empty()
            });
        });
        if !mesh.vertices.is_empty() { draw_mesh(&mesh); }
        gl_use_default_material();
    });
}

/// Vitrine local de cores/refinos, sem alterar equipamento de personagens.
pub async fn previa(vox: &crate::vox::VoxCache) {
    let mut world = crate::world::World::default();
    let mut metas = Vec::new(); let mut estados = Vec::new();
    for i in 0..4u32 {
        let grau = 3u64;
        let tier = i as u64;
        let bits = (0..shared::auras::SLOTS).fold(0, |b, s| b | ((grau | (tier << 3)) << (s*8)));
        metas.push(shared::EntityMeta { auras: bits, id: shared::EntityId(i+1), tag: shared::EntityTag::Player,
            name: None, hp_max: 100, faction: None, kind: 0, nivel: 20, desafio: None, aparencia: 0 });
        let mut st = shared::EntityState::quantize(shared::EntityId(i+1), ::glam::Vec2::new(i as f32*2.2-3.3,0.0), ::glam::Vec2::ZERO,100,0);
        st.acao = shared::components::acao::monta(0,true,0,0); estados.push(st);
    }
    world.apply(metas, estados, &[]);
    let solido = crate::render3d::material_solido();
    let rt = render_target(1280, 800);
    for frame in 0..90 {
        world.tick(1.0/60.0, &|_,_|0.0);
        for e in world.ents.values_mut() { e.sacada=1.0; e.yaw=0.3; }
        let mut vista = crate::render3d::Vista::nova(Vec2::ZERO,0.0,0.0,0.8,0.0,&|_,_|0.0);
        vista.cam.position = vec3(0.0,4.3,-9.0); vista.cam.target=vec3(0.0,0.9,0.0);
        vista.cam.render_target = Some(rt.clone());
        set_camera(&vista.cam); clear_background(Color::from_rgba(15,20,29,255));
        draw_plane(Vec3::ZERO,vec2(20.0,20.0),None,Color::from_rgba(27,35,42,255));
        gl_use_material(&solido); solido.set_uniform("Recorte",Vec3::ZERO);
        crate::render3d::draw_entities(&mut world,vox,None,&vista);
        gl_use_default_material();
        let mut hud = Camera2D::from_display_rect(Rect::new(0.0,0.0,1280.0,800.0));
        hud.render_target = Some(rt.clone()); set_camera(&hud);
        draw_text("AURAS POR EQUIPAMENTO",30.0,42.0,30.0,WHITE);
        draw_text("Mesma cor e refino: T4           T3           T2           T1",30.0,76.0,22.0,WHITE);
        if frame == 89 {
            unsafe { get_internal_gl().flush(); }
            rt.texture.get_texture_data().export_png("/tmp/tempest-auras.png");
        }
        next_frame().await;
    }
}
