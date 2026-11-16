//! INVISIBILITY on screen (the Assassin's path-tree active, rebirth phase 4,
//! `ServerMessage::Invisibilidade`).
//!
//! The server owns the effect (monsters drop the body, players cannot hit
//! it); this only keeps, per entity, until when it is unseen. Nobody draws
//! the body, its shadow or its name plate, and nobody can click it. The
//! OWNER sees a SHADOW SILHOUETTE in its place (`desenha_fantasma`): a
//! translucent violet column with smoke curling up and a faint ring on the
//! ground. It used to draw the body every other frame (the world material
//! has no per-body alpha), and that flicker read as a glitch (owner,
//! 15/11/2026: "invisibilidade tá muito estranho o efeito").

use macroquad::prelude::*;
use shared::EntityId;
use std::cell::RefCell;
use std::collections::HashMap;

thread_local! {
    static ATE: RefCell<HashMap<EntityId, f64>> = RefCell::new(HashMap::new());
}

/// `segundos` > 0 starts (or refreshes) it; 0 ends it.
pub fn marcar(eid: EntityId, segundos: f32) {
    marcar_em(eid, segundos, get_time());
}

fn marcar_em(eid: EntityId, segundos: f32, agora: f64) {
    ATE.with(|m| {
        let mut m = m.borrow_mut();
        if segundos > 0.0 {
            m.insert(eid, agora + segundos as f64);
        } else {
            m.remove(&eid);
        }
        m.retain(|_, ate| *ate > agora);
    });
}

fn ativa_em(eid: EntityId, agora: f64) -> bool {
    ATE.with(|m| m.borrow().get(&eid).is_some_and(|&ate| ate > agora))
}

/// Is `eid` invisible right now?
pub fn ativa(eid: EntityId) -> bool {
    ativa_em(eid, get_time())
}

/// Hidden from THIS client: invisible and not me.
pub fn oculto(eid: EntityId, eu: Option<EntityId>) -> bool {
    Some(eid) != eu && ativa(eid)
}

/// Skip the body: invisible bodies are never drawn (the owner gets
/// `desenha_fantasma` instead).
pub fn pular_corpo(eid: EntityId, _eu: Option<EntityId>) -> bool {
    ativa(eid)
}

/// The owner's shadow silhouette at its feet. The caller has set the world
/// material (alpha vertex colours blend, like `nivel_vfx`).
pub fn desenha_fantasma(pe: Vec3, olho: Vec3, t: f32) {
    let para = vec3(olho.x - pe.x, 0.0, olho.z - pe.z).normalize_or_zero();
    let lado = para.cross(Vec3::Y).normalize_or_zero();
    if lado.length_squared() < 0.1 {
        return;
    }
    let respira = (t * 2.2).sin() * 0.5 + 0.5;
    // The body: a soft column, opaque-ish in the middle, fading at the edges
    // and the top, so it reads as a silhouette and not as a wall.
    const H: f32 = 1.85;
    let largura = 0.34 + 0.03 * respira;
    let miolo = [70, 40, 110, (95.0 + 35.0 * respira) as u8];
    let borda = [40, 20, 70, 0];
    let pe = vec3(pe.x, pe.y + 0.02, pe.z);
    let mut v = Vec::new();
    let mut tris = Vec::new();
    for k in 0..=6 {
        let u = k as f32 / 6.0;
        // Narrow shoulders and head: a person's outline in 7 rows.
        let perfil = match k {
            0 => 0.70,
            1 => 0.80,
            2 => 0.85,
            3 => 0.90,
            4 => 1.00,
            5 => 0.55,
            _ => 0.45,
        };
        let w = largura * perfil;
        let y = pe + Vec3::Y * (H * u);
        let a = if k == 6 { 0 } else { miolo[3] };
        let c = [miolo[0], miolo[1], miolo[2], a];
        v.push(vtx(y - lado * w * 1.6, borda));
        v.push(vtx(y - lado * w, c));
        v.push(vtx(y + lado * w, c));
        v.push(vtx(y + lado * w * 1.6, borda));
        if k > 0 {
            let (p0, p1) = (((k - 1) * 4) as u16, (k * 4) as u16);
            for j in 0..3u16 {
                tris.push([p0 + j, p1 + j, p0 + j + 1]);
                tris.push([p1 + j, p1 + j + 1, p0 + j + 1]);
            }
        }
    }
    dupla(v, tris);
    // Smoke curling up around it.
    for i in 0..10 {
        let u = (t * 0.55 + i as f32 / 10.0).fract();
        let ang = i as f32 * 2.399963 + t * 1.3;
        let r = 0.28 + 0.25 * u;
        let p = pe + vec3(ang.cos() * r, 0.15 + u * 2.1, ang.sin() * r);
        let s = 0.09 + 0.10 * u;
        let a = ((1.0 - u) * (u * 4.0).min(1.0) * 120.0) as u8;
        let c = [120, 80, 170, a];
        let (l, up) = (lado * s, Vec3::Y * s);
        dupla(
            vec![vtx(p - l - up, c), vtx(p + l - up, c), vtx(p + l + up, c), vtx(p - l + up, c)],
            vec![[0, 1, 2], [0, 2, 3]],
        );
    }
    // A faint ring on the ground: where you stand.
    const LADOS: usize = 24;
    let centro = vec3(pe.x, pe.y + 0.04, pe.z);
    let raio = 0.55 + 0.06 * respira;
    let anel = [150, 100, 220, 90];
    let fora = [150, 100, 220, 0];
    let (mut v, mut tris) = (Vec::new(), Vec::new());
    for k in 0..=LADOS {
        let ang = k as f32 / LADOS as f32 * std::f32::consts::TAU;
        let d = vec3(ang.cos(), 0.0, ang.sin());
        v.push(vtx(centro + d * (raio - 0.08), fora));
        v.push(vtx(centro + d * raio, anel));
        v.push(vtx(centro + d * (raio + 0.08), fora));
        if k > 0 {
            let (p, q) = (((k - 1) * 3) as u16, (k * 3) as u16);
            tris.push([p, q, p + 1]);
            tris.push([q, q + 1, p + 1]);
            tris.push([p + 1, q + 1, p + 2]);
            tris.push([q + 1, q + 2, p + 2]);
        }
    }
    dupla(v, tris);
}

fn vtx(p: Vec3, c: [u8; 4]) -> Vertex {
    Vertex { position: p, uv: vec2(0.0, 0.0), color: c, normal: Vec4::ZERO }
}

fn dupla(vertices: Vec<Vertex>, tris: Vec<[u16; 3]>) {
    let mut indices = Vec::with_capacity(tris.len() * 6);
    for [a, b, c] in tris {
        indices.extend_from_slice(&[a, b, c, a, c, b]);
    }
    draw_mesh(&Mesh { vertices, indices, texture: None });
}

/// Forget everything (logout, zone change).
pub fn limpar() {
    ATE.with(|m| m.borrow_mut().clear());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn marca_expira_e_desliga() {
        let a = EntityId(41);
        marcar_em(a, 6.0, 100.0);
        assert!(ativa_em(a, 105.9));
        assert!(!ativa_em(a, 106.1), "runs out by itself");
        marcar_em(a, 6.0, 100.0);
        marcar_em(a, 0.0, 101.0);
        assert!(!ativa_em(a, 101.5), "the server ended it");
        assert!(!ativa_em(EntityId(42), 100.0));
    }
}
