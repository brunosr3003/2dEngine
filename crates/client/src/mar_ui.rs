//! THE OPEN SEA on screen (docs/NAVEGACAO.md, naval phases 1 and 2).
//!
//! Navigating is walking, only on water: the server moves the hull with the
//! same controls as the body. What changes here is what is DRAWN — a hull
//! instead of the body for every player at sea, in its class, sail colour and
//! figurehead — plus the hull bar, the "Dock" prompt at an anchorage, and the
//! SHIP'S SKILL BAR that takes slots 1-4 while sailing (phase 2).

#[allow(unused_imports)]
use crate::ui::Toque as _;
use macroquad::prelude::*;
use std::collections::HashSet;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

use crate::habilidades_input::{Arrasto, Gesto};
use crate::hud_estilo as estilo;
use shared::navio::{self as nv, Visual};

static NO_MAR: AtomicBool = AtomicBool::new(false);

/// The zone just changed: is it the open sea?
pub fn define_no_mar(sim: bool) {
    NO_MAR.store(sim, Ordering::Relaxed);
}

pub fn no_mar() -> bool {
    NO_MAR.load(Ordering::Relaxed)
}

fn rgb(c: [u8; 3]) -> Color {
    Color::from_rgba(c[0], c[1], c[2], 255)
}

/// THE HULL, in cubes (a simple voxel hull): three blocks narrowing to the
/// bow so it has a FRONT, a gunwale, masts and sails. The game's models face
/// +Z and the yaw is `x.atan2(z)`: the front is (sin, cos).
///
/// Phase 2: the CLASS sets the size and the masts (Sloop 1, Schooner 2,
/// Galleon and Man-o'-War 3, gun ports from the Galleon), the SAIL item
/// recolours the canvas, the FIGUREHEAD sits on the bow, and the stars turn
/// the gunwale to brass.
pub fn desenha_casco(p: Vec3, yaw: f32, semente: u64, v: Visual) {
    let t = get_time() as f32 + (semente % 97) as f32 * 0.37;
    let classe = nv::CASCOS[(v.classe as usize).min(nv::CASCOS.len() - 1)];
    let k = classe.escala;
    // Riding ON the swell: the Gerstner surface rises a few tenths above
    // the sea level, and a hull at the level sank into it.
    let p = p + vec3(0.0, crate::agua::ALTURA_DA_AGUA + crate::agua::AMPLITUDE_MAX * 0.6 + (t * 1.7).sin() * 0.06, 0.0);
    let madeira = if v.classe >= 3 { Color::from_rgba(92, 58, 34, 255) } else { Color::from_rgba(112, 72, 38, 255) };
    let escura = Color::from_rgba(78, 48, 24, 255);
    let latao = Color::from_rgba(196, 156, 72, 255);
    // Brass creeps into the gunwale with the stars (0..15).
    let f = (v.estrelas as f32 / 15.0).clamp(0.0, 1.0);
    let amurada = Color::new(
        escura.r + (latao.r - escura.r) * f,
        escura.g + (latao.g - escura.g) * f,
        escura.b + (latao.b - escura.b) * f,
        1.0,
    );
    let pano = rgb(nv::cor_da_vela(v.vela));
    let (fx, fz) = (yaw.sin(), yaw.cos());
    let em = |ao_longo: f32, ao_lado: f32, y: f32| {
        p + vec3(ao_longo * k * fx + ao_lado * k * fz, y * k, ao_longo * k * fz - ao_lado * k * fx)
    };
    // The hull, three pieces narrowing to the bow, along the heading.
    for (ao_longo, comp, larg) in [(-1.1f32, 1.6f32, 2.0f32), (0.5, 1.4, 1.6), (1.5, 0.9, 0.9)] {
        caixa(em(ao_longo, 0.0, 0.25), comp * k, 0.6 * k, larg * k, fx, fz, madeira);
    }
    // A raised stern deck from the Galleon on.
    if v.classe >= 2 {
        caixa(em(-1.5, 0.0, 0.75), 0.8 * k, 0.4 * k, 1.8 * k, fx, fz, madeira);
    }
    for lado in [-1.0f32, 1.0] {
        caixa(em(-0.3, lado * 0.92, 0.62), 2.8 * k, 0.22 * k, 0.14 * k, fx, fz, amurada);
        // Gun ports.
        if v.classe >= 2 {
            for i in 0..(v.classe as i32 + 1) {
                let x = -1.2 + i as f32 * 0.6;
                draw_cube(em(x, lado * 1.0, 0.42), vec3(0.18, 0.14, 0.18) * k, None, Color::from_rgba(24, 20, 18, 255));
            }
        }
    }
    let inflar = 0.08 * (t * 0.9).sin();
    let mastros: &[(f32, f32)] = match classe.mastros {
        1 => &[(-0.2, 1.0)],
        2 => &[(0.45, 0.85), (-0.85, 1.0)],
        _ => &[(0.8, 0.8), (-0.2, 1.05), (-1.15, 0.85)],
    };
    for (x, alto) in mastros {
        caixa(em(*x, 0.0, 1.5 * alto), 0.16 * k, 2.2 * k * alto, 0.16 * k, fx, fz, escura);
        caixa(em(*x + inflar, 0.0, 1.85 * alto), 0.1 * k, 1.3 * k * alto, 1.7 * k, fx, fz, pano);
    }
    // A pennant on the main mast of the big hulls.
    if v.classe >= 2 {
        let (x, alto) = mastros[1];
        let topo = em(x, 0.0, 1.5 * alto + 1.15 * alto);
        draw_cube(topo + vec3(0.0, 0.0, 0.0), vec3(0.08, 0.3, 0.08) * k, None, escura);
        draw_cube(em(x - 0.25 + inflar, 0.0, 1.5 * alto + 1.2 * alto), vec3(0.3, 0.14, 0.05) * k, None, pano);
    }
    figura_de_proa(v.figura, &em, k, t);
}

/// The FIGUREHEAD on the bow's tip: a few cubes and spheres each.
fn figura_de_proa(figura: u8, em: &dyn Fn(f32, f32, f32) -> Vec3, k: f32, t: f32) {
    match figura {
        // Mermaid: a pale body leaning out, sea-green tail and copper hair.
        1 => {
            let pele = Color::from_rgba(228, 196, 170, 255);
            draw_cube(em(2.0, 0.0, 0.55), vec3(0.22, 0.42, 0.22) * k, None, Color::from_rgba(40, 150, 140, 255));
            draw_cube(em(2.08, 0.0, 0.85), vec3(0.2, 0.3, 0.2) * k, None, pele);
            draw_sphere(em(2.14, 0.0, 1.08), 0.13 * k, None, pele);
            draw_sphere(em(2.08, 0.0, 1.14), 0.12 * k, None, Color::from_rgba(196, 92, 44, 255));
        }
        // Dragon: a green neck reaching forward, a snout and red eyes.
        2 => {
            let verde = Color::from_rgba(52, 128, 62, 255);
            draw_cube(em(2.0, 0.0, 0.7), vec3(0.26, 0.5, 0.26) * k, None, verde);
            draw_cube(em(2.2, 0.0, 1.0), vec3(0.42, 0.24, 0.26) * k, None, verde);
            draw_cube(em(2.42, 0.0, 0.96), vec3(0.2, 0.14, 0.18) * k, None, Color::from_rgba(40, 96, 48, 255));
            for lado in [-1.0f32, 1.0] {
                draw_cube(em(2.24, lado * 0.1, 1.1), vec3(0.06, 0.06, 0.06) * k, None, Color::from_rgba(230, 40, 30, 255));
                draw_cube(em(2.06, lado * 0.09, 1.2), vec3(0.06, 0.16, 0.06) * k, None, Color::from_rgba(220, 210, 180, 255));
            }
        }
        // Eagle: a golden body with the wings spread, flapping a little.
        3 => {
            let ouro = Color::from_rgba(214, 170, 58, 255);
            draw_cube(em(2.05, 0.0, 0.8), vec3(0.24, 0.36, 0.24) * k, None, ouro);
            draw_sphere(em(2.18, 0.0, 1.05), 0.12 * k, None, Color::from_rgba(240, 232, 214, 255));
            let asa = 0.06 * (t * 2.0).sin();
            for lado in [-1.0f32, 1.0] {
                draw_cube(em(2.0, lado * 0.34, 0.92 + asa), vec3(0.2, 0.06, 0.44) * k, None, ouro);
            }
        }
        _ => {}
    }
}

/// A box of `comp` along the heading, `larg` across it. macroquad's cubes
/// are axis-aligned, so a turned box is drawn as a row of small cubes along
/// the heading — enough for a ship that is a few units long.
fn caixa(c: Vec3, comp: f32, alto: f32, larg: f32, fx: f32, fz: f32, cor: Color) {
    let passo = 0.22f32;
    let n = (comp / passo).ceil().max(1.0) as i32;
    let lado = (larg / passo).ceil().max(1.0) as i32;
    for i in 0..n {
        let a = -comp * 0.5 + (i as f32 + 0.5) * comp / n as f32;
        for j in 0..lado {
            let b = -larg * 0.5 + (j as f32 + 0.5) * larg / lado as f32;
            let q = c + vec3(a * fx + b * fz, 0.0, a * fz - b * fx);
            draw_cube(q, vec3(passo * 1.05, alto, passo * 1.05), None, cor);
        }
    }
}

/// The ship's hull from the equipped instance: (points, full, damaged). The
/// full hull counts the hull stars (`navio::casco_total`).
pub fn casco_de(eq: &shared::Equipment) -> Option<(u16, u16, bool)> {
    let id = eq.navio.filter(|i| shared::item_id::e_navio(*i))?;
    let d = nv::dados(eq.navio_inst.as_ref(), id);
    Some((d.casco, nv::casco_total(id, &d), d.avariado))
}

/// The HULL BAR, top centre, only at sea.
pub fn barra_do_casco(z: &crate::hud_layout::Zonas, eq: &shared::Equipment) {
    if !no_mar() {
        return;
    }
    let Some((casco, max, _)) = casco_de(eq) else {
        return;
    };
    let w = 260.0 * z.s;
    let h = 22.0 * z.s;
    let x = screen_width() * 0.5 - w * 0.5;
    let y = z.alvo.y + z.alvo.h + 10.0 * z.s;
    let frac = if max == 0 { 0.0 } else { casco as f32 / max as f32 };
    draw_rectangle(x, y, w, h, Color::new(0.05, 0.08, 0.12, 0.85));
    let cor = if frac <= nv::CRITICO {
        Color::new(0.85, 0.25, 0.2, 1.0)
    } else {
        Color::new(0.75, 0.55, 0.28, 1.0)
    };
    draw_rectangle(x + 2.0, y + 2.0, (w - 4.0) * frac, h - 4.0, cor);
    draw_rectangle_lines(x, y, w, h, 2.0, Color::new(0.9, 0.78, 0.5, 0.9));
    estilo::texto_centro(
        x + w * 0.5,
        y + h * 0.5 + 6.0 * z.s,
        &format!("Hull {casco} / {max}"),
        14,
        estilo::TEXTO,
    );
}

/// THE DOCK PROMPT: at an anchorage, a "Dock at <island>" button. Returns
/// the island (`ARQUIPELAGO` index) when tapped.
pub fn botao_atracar(z: &crate::hud_layout::Zonas, eu: Option<(f32, f32)>) -> Option<u8> {
    if !no_mar() {
        return None;
    }
    let (x, zz) = eu?;
    let cais = shared::mar::cais_perto(::glam::Vec2::new(x, zz))?;
    let rotulo = format!("Dock at {}", cais.nome);
    let h = z.faixa.h * 1.5;
    let w = estilo::medir_forte(&rotulo, 18) + h * 1.4;
    let r = Rect::new(z.faixa.center().x - w * 0.5, z.faixa.y - 8.0 * z.s - h * 2.3, w, h);
    estilo::botao(r, &rotulo, estilo::estado_de(r, false, false), true);
    (crate::foco::clique() && r.contains(Vec2::from(mouse_position()))).then_some(cais.ilha as u8)
}

// ───────────────────── phase 4: the treasure and the mark ─────────────────────

/// The Colossus Chest on the equipped ship's deck, if any.
pub fn tesouro_no_conves(eq: &shared::Equipment) -> Option<shared::items::Tesouro> {
    let id = eq.navio.filter(|i| shared::item_id::e_navio(*i))?;
    nv::dados(eq.navio_inst.as_ref(), id).conves
}

/// The HUD line under the hull bar, at sea: what makes you a target. The
/// carrier reads "You carry treasure — anyone can attack you"; the trail
/// and the Pirate's Code read "Marked"; inside lawless waters, the warning.
pub fn linha_de_perigo(tesouro: bool, marcado: bool, sem_lei: bool) -> Option<&'static str> {
    if tesouro {
        Some("You carry treasure — anyone can attack you")
    } else if marcado {
        Some("Marked — anyone can attack you")
    } else if sem_lei {
        Some("Lawless waters — anyone can attack you here")
    } else {
        None
    }
}

/// Draws `linha_de_perigo` in red under the hull bar.
pub fn aviso_de_perigo(z: &crate::hud_layout::Zonas, eq: &shared::Equipment, marcado: bool, eu: Option<(f32, f32)>) {
    if !no_mar() {
        return;
    }
    let sem_lei = eu.is_some_and(|(x, y)| shared::mar::sem_lei(::glam::Vec2::new(x, y)));
    let Some(texto) = linha_de_perigo(tesouro_no_conves(eq).is_some(), marcado, sem_lei) else {
        return;
    };
    let y = z.alvo.y + z.alvo.h + 10.0 * z.s + 22.0 * z.s + 18.0 * z.s;
    let pisca = 0.75 + 0.25 * (get_time() as f32 * 4.0).sin();
    estilo::texto_centro_forte(screen_width() * 0.5 + 1.0, y + 1.0, texto, 15, Color::new(0.0, 0.0, 0.0, 0.85));
    estilo::texto_centro_forte(screen_width() * 0.5, y, texto, 15, Color::new(1.0, 0.32, 0.28, pisca));
}

/// On land with a Colossus Chest on deck: "Open the Colossus Chest · ×N".
/// `bonus` = the Corsair's treasure passive. `true` when tapped.
pub fn botao_abrir_tesouro(z: &crate::hud_layout::Zonas, eq: &shared::Equipment, bonus: f32) -> bool {
    if no_mar() {
        return false;
    }
    let Some(t) = tesouro_no_conves(eq) else {
        return false;
    };
    let rotulo = format!("Open the Colossus Chest · ×{:.1}", nv::mult_do_tesouro(&t, bonus));
    let h = z.faixa.h * 1.5;
    let w = estilo::medir_forte(&rotulo, 18) + h * 1.4;
    let r = Rect::new(z.faixa.center().x - w * 0.5, z.faixa.y - 8.0 * z.s - h * 2.3, w, h);
    estilo::botao(r, &rotulo, estilo::estado_de(r, false, false), true);
    crate::foco::clique() && r.contains(Vec2::from(mouse_position()))
}

/// THE SKULL FLAG of a marked ship: a black flag on a pole over the main
/// mast with a white skull of cubes, so the hunter sees the prize from afar.
pub fn desenha_marca(p: Vec3, yaw: f32, v: Visual) {
    let classe = nv::CASCOS[(v.classe as usize).min(nv::CASCOS.len() - 1)];
    let k = classe.escala;
    let t = get_time() as f32;
    let base = p + vec3(0.0, crate::agua::ALTURA_DA_AGUA + crate::agua::AMPLITUDE_MAX * 0.6, 0.0);
    let (fx, fz) = (yaw.sin(), yaw.cos());
    let topo = base + vec3(-0.2 * k * fx, 4.2 * k, -0.2 * k * fz);
    draw_cube(topo, vec3(0.08, 1.0, 0.08) * k, None, Color::from_rgba(40, 30, 22, 255));
    let ondula = 0.06 * (t * 3.0).sin();
    let bandeira = topo + vec3(0.0, 0.25 * k + ondula, 0.0) + vec3(fz, 0.0, -fx) * 0.45 * k;
    draw_cube(bandeira, vec3(0.9, 0.6, 0.06) * k, None, Color::from_rgba(18, 16, 20, 255));
    let osso = Color::from_rgba(236, 232, 220, 255);
    draw_cube(bandeira + vec3(0.0, 0.06 * k, 0.0), vec3(0.3, 0.26, 0.1) * k, None, osso);
    draw_cube(bandeira + vec3(0.0, -0.14 * k, 0.0), vec3(0.18, 0.1, 0.1) * k, None, osso);
    let olho = Color::from_rgba(18, 16, 20, 255);
    for lado in [-1.0f32, 1.0] {
        draw_cube(bandeira + vec3(fz, 0.0, -fx) * 0.07 * k * lado + vec3(0.0, 0.08 * k, 0.0), vec3(0.08, 0.08, 0.12) * k, None, olho);
    }
}

/// FLAT ICONS for the sea's items (no atlas art for them): the four hull
/// classes, the Wreck Plank, the sails in their colour and the figureheads.
/// `false` = not a sea item.
pub fn icone_do_item(id: u16, r: Rect, alfa: f32) -> bool {
    let c = r.center();
    let s = r.w.min(r.h) * 0.42;
    let a = |cor: Color| Color::new(cor.r, cor.g, cor.b, cor.a * alfa);
    let madeira = a(Color::from_rgba(122, 80, 44, 255));
    let escura = a(Color::from_rgba(70, 44, 24, 255));
    if shared::item_id::e_navio(id) {
        let classe = nv::classe(id);
        // Hull: a trapezoid of two triangles; masts and sails by class.
        let base = c.y + s * 0.45;
        draw_triangle(vec2(c.x - s, base - s * 0.35), vec2(c.x + s, base - s * 0.35), vec2(c.x + s * 0.7, base), madeira);
        draw_triangle(vec2(c.x - s, base - s * 0.35), vec2(c.x + s * 0.7, base), vec2(c.x - s * 0.7, base), madeira);
        let mastros = nv::CASCOS[classe].mastros as usize;
        for k in 0..mastros {
            let x = c.x + (k as f32 - (mastros as f32 - 1.0) * 0.5) * s * 0.6;
            draw_line(x, base - s * 0.35, x, c.y - s * 0.95, s * 0.08, escura);
            draw_triangle(vec2(x + s * 0.04, c.y - s * 0.85), vec2(x + s * 0.04, c.y - s * 0.05), vec2(x + s * 0.4, c.y - s * 0.1), a(Color::from_rgba(236, 228, 206, 255)));
        }
        return true;
    }
    if id == shared::item_id::PRANCHA_DE_NAUFRAGIO {
        for k in 0..3 {
            let y = c.y - s * 0.6 + k as f32 * s * 0.45;
            draw_rectangle(c.x - s * 0.9, y, s * 1.8, s * 0.32, if k == 1 { escura } else { madeira });
            draw_circle(c.x - s * 0.7, y + s * 0.16, s * 0.05, a(Color::from_rgba(40, 36, 34, 255)));
            draw_circle(c.x + s * 0.7, y + s * 0.16, s * 0.05, a(Color::from_rgba(40, 36, 34, 255)));
        }
        // Sea-green stain: it came out of the water.
        draw_circle(c.x + s * 0.3, c.y + s * 0.3, s * 0.18, a(Color::new(0.3, 0.6, 0.55, 0.6)));
        return true;
    }
    match nv::cosmetico(id) {
        Some(nv::Cosmetico::Vela(v)) => {
            draw_line(c.x - s * 0.5, c.y - s, c.x - s * 0.5, c.y + s, s * 0.1, escura);
            let pano = a(rgb(nv::cor_da_vela(v)));
            draw_triangle(vec2(c.x - s * 0.42, c.y - s * 0.9), vec2(c.x - s * 0.42, c.y + s * 0.6), vec2(c.x + s * 0.85, c.y + s * 0.5), pano);
            true
        }
        Some(nv::Cosmetico::Figura(f)) => {
            let cor = match f {
                1 => Color::from_rgba(40, 150, 140, 255),
                2 => Color::from_rgba(52, 128, 62, 255),
                _ => Color::from_rgba(214, 170, 58, 255),
            };
            draw_rectangle(c.x - s * 0.9, c.y + s * 0.3, s * 1.2, s * 0.35, madeira);
            draw_circle(c.x + s * 0.2, c.y - s * 0.1, s * 0.45, a(cor));
            draw_circle(c.x + s * 0.45, c.y - s * 0.55, s * 0.28, a(cor));
            true
        }
        None => false,
    }
}

// ─────────────────────────── the ship's skill bar ───────────────────────────

/// What the ship bar needs to know this frame.
#[derive(Debug, Clone, Copy)]
pub struct ContextoNavio {
    /// Alive, with a sound ship.
    pub pronto: bool,
    /// Distance to the selected enemy, if there is one.
    pub distancia_alvo: Option<f32>,
}

/// THE SHIP'S BAR (docs/NAVEGACAO.md, phase 2): at sea, skill slots 1-4
/// are the ship's — Broadside, Bow Cannon, Emergency Repair, Smoke. Same
/// gestures as the weapon bar: tap to fire, drag up for AUTO (the attacks
/// only), drag down for manual. Cooldowns come in the usual `SkillsState`.
#[derive(Default)]
pub struct BarraDoNavio {
    arrasto: Arrasto,
    pub automaticas: HashSet<u8>,
    tentar_apos: [f64; 4],
    pendente_ate: f64,
}

/// Slot `i` (0..4): the three weapon-skill slots and the jump's.
pub fn retangulo(i: usize) -> Rect {
    if i < 3 {
        crate::habilidades::retangulo(i)
    } else {
        crate::hud_layout::atual().pulo
    }
}

fn espera(h: &nv::HabilidadeDoNavio, hab: &crate::habilidades::Habilidades, agora: f64) -> f64 {
    (hab.recarga_ate(nv::id_de_skill(h.id)) - agora).max(0.0)
}

/// Can attack `h` reach the target at `d`?
fn alcanca(h: &nv::HabilidadeDoNavio, d: Option<f32>) -> bool {
    d.is_some_and(|d| d <= h.alcance)
}

impl BarraDoNavio {
    pub fn pega_mouse(&self) -> bool {
        let m = Vec2::from(mouse_position());
        self.arrasto.inicio.is_some() || (0..4).any(|i| retangulo(i).toca(m))
    }

    pub fn cancela_arrasto(&mut self) {
        self.arrasto.cancela();
    }

    /// Input and AUTO: the ship skill to send, if any. `entrada` false =
    /// input blocked (power-saving mode, a panel): AUTO only.
    pub fn pedido(
        &mut self,
        c: ContextoNavio,
        hab: &mut crate::habilidades::Habilidades,
        entrada: bool,
    ) -> Option<u8> {
        let agora = get_time();
        let mouse = Vec2::from(mouse_position());
        let mut gesto = None;
        if entrada {
            if crate::foco::clique() {
                if let Some(i) = (0..4).find(|i| retangulo(*i).toca(mouse)) {
                    self.arrasto.pressiona(nv::id_de_skill(i as u8), mouse);
                }
            }
            self.arrasto.move_para(mouse);
            if let Some((id, _)) = self.arrasto.inicio {
                let i = (id - nv::ID_SKILL_BASE) as usize;
                if is_mouse_button_released(MouseButton::Left) {
                    gesto = self.arrasto.solta(mouse, retangulo(i));
                } else if !is_mouse_button_down(MouseButton::Left) {
                    self.arrasto.cancela();
                }
            }
        } else {
            self.arrasto.cancela();
        }
        if let Some(Gesto::Auto(id, ligar)) = gesto {
            let Some(h) = nv::habilidade_do_id(id) else {
                return None;
            };
            if !h.auto {
                hab.aviso("Only the cannons go on AUTO".into());
            } else if ligar {
                self.automaticas.insert(h.id);
                hab.aviso("Automatic use switched on".into());
            } else {
                self.automaticas.remove(&h.id);
                hab.aviso("Automatic use switched off".into());
            }
            return None;
        }
        let manual = if entrada {
            (0..4)
                .find(|i| crate::desktop::skill_pressionada(*i))
                .map(|i| i as u8)
                .or(match gesto {
                    Some(Gesto::Usar(id)) => nv::habilidade_do_id(id).map(|h| h.id),
                    _ => None,
                })
        } else {
            None
        };
        if let Some(i) = manual {
            let h = nv::habilidade(i)?;
            let erro = if !c.pronto {
                Some("Your ship cannot do that now")
            } else if agora < self.pendente_ate {
                Some("Wait for the current action to finish")
            } else if espera(h, hab, agora) > 0.0 {
                Some("Skill on cooldown")
            } else if h.efeito == nv::Efeito::Canhonaco && !alcanca(h, c.distancia_alvo) {
                Some("Select an enemy in range of the bow cannon")
            } else {
                None
            };
            if let Some(m) = erro {
                hab.aviso(m.into());
                return None;
            }
            self.pendente_ate = agora + 0.4;
            return Some(i);
        }
        if self.arrasto.inicio.is_some() {
            return None;
        }
        self.automatico(c, hab, agora)
    }

    /// AUTO: the attack skills switched on, whenever the selected enemy is
    /// in reach and the cooldown is over.
    fn automatico(&mut self, c: ContextoNavio, hab: &crate::habilidades::Habilidades, agora: f64) -> Option<u8> {
        if !c.pronto || agora < self.pendente_ate {
            return None;
        }
        let h = nv::HABILIDADES.iter().find(|h| {
            h.auto
                && self.automaticas.contains(&h.id)
                && espera(h, hab, agora) == 0.0
                && self.tentar_apos[h.id as usize] <= agora
                && alcanca(h, c.distancia_alvo)
        })?;
        self.pendente_ate = agora + 0.4;
        self.tentar_apos[h.id as usize] = agora + 1.5;
        Some(h.id)
    }

    /// The four round buttons, with cooldown sweep, AUTO pill and tooltip.
    pub fn desenha(&self, hab: &crate::habilidades::Habilidades) {
        let agora = get_time();
        let m = Vec2::from(mouse_position());
        for h in nv::HABILIDADES.iter() {
            let i = h.id as usize;
            let r = retangulo(i);
            let c = r.center();
            let raio = r.w * 0.5;
            let cd = espera(h, hab, agora);
            let auto = self.automaticas.contains(&h.id);
            let cor = cor_da_habilidade(h.efeito);
            let e = estilo::estado(r.toca(m), is_mouse_button_down(MouseButton::Left), false, false);
            estilo::botao_redondo(c, raio, if auto { estilo::AUTO } else { cor }, e, cd == 0.0 && auto);
            icone_da_habilidade(h.efeito, c, raio * 0.55, if cd > 0.0 { 0.45 } else { 1.0 });
            if cd > 0.0 {
                let f = (cd as f32 / h.espera_s.max(0.01)).min(1.0);
                estilo::setor(c, raio - 3.0, f, Color::new(0.0, 0.0, 0.0, 0.62));
                estilo::arco(c, raio - 2.0, -std::f32::consts::FRAC_PI_2, f, 2.0, estilo::OURO);
                let t = format!("{cd:.1}");
                estilo::texto_sombra(c.x - estilo::medir_forte(&t, 22) * 0.5, c.y + 8.0, &t, 22, WHITE, true);
            }
            crate::hud_layout::chip(r, &(i + 1).to_string());
            if auto {
                let pilula = Rect::new(c.x - 22.0, r.y + r.h - 10.0, 44.0, 17.0);
                estilo::ret_arredondado(pilula, 8.5, estilo::FUNDO_BAIXO);
                estilo::borda_arredondada(pilula, 8.5, 1.0, estilo::alfa(estilo::AUTO, 0.7));
                estilo::texto_centro_forte(c.x, r.y + r.h + 3.0, "AUTO", 11, estilo::AUTO);
            }
            let arrastando = self.arrasto.inicio.is_some_and(|(id, _)| id == nv::id_de_skill(h.id));
            if arrastando {
                let (_, inicio) = self.arrasto.inicio.unwrap();
                let texto = if m.y > inicio.y + 20.0 { "Release: MANUAL" } else { "Release: AUTO" };
                estilo::painel(Rect::new(c.x - 82.0, r.y - 74.0, 164.0, 31.0));
                estilo::texto_centro(c.x, r.y - 53.0, texto, 15, estilo::AUTO);
            } else if r.toca(m) && self.arrasto.inicio.is_none() {
                let w = 380.0_f32.min(screen_width() - 24.0);
                let x = (r.x + r.w - w).clamp(12.0, screen_width() - w - 12.0);
                estilo::painel(Rect::new(x, r.y - 122.0, w, 88.0));
                estilo::texto(x + 12.0, r.y - 98.0, h.nome, 19, cor);
                estilo::texto_ajustado(h.descricao, x + 12.0, r.y - 77.0, w - 24.0, 14, estilo::TEXTO);
                let rodape = if h.auto {
                    format!("{:.0}s · no mana · drag up: AUTO", h.espera_s)
                } else {
                    format!("{:.0}s · no mana", h.espera_s)
                };
                estilo::texto(x + 12.0, r.y - 53.0, &rodape, 13, estilo::SUAVE);
            }
        }
    }
}

fn cor_da_habilidade(e: nv::Efeito) -> Color {
    match e {
        nv::Efeito::Bordada => Color::from_rgba(214, 120, 58, 255),
        nv::Efeito::Canhonaco => Color::from_rgba(196, 72, 52, 255),
        nv::Efeito::Reparo => Color::from_rgba(92, 176, 96, 255),
        nv::Efeito::Fumaca => Color::from_rgba(150, 150, 160, 255),
    }
}

/// A flat icon per ship skill (no atlas art for them).
fn icone_da_habilidade(e: nv::Efeito, c: Vec2, r: f32, alfa: f32) {
    let ferro = Color::new(0.12, 0.12, 0.14, alfa);
    let claro = Color::new(0.95, 0.9, 0.8, alfa);
    match e {
        nv::Efeito::Bordada => {
            // A hull from above with a ball out of each side.
            draw_rectangle(c.x - r * 0.25, c.y - r * 0.8, r * 0.5, r * 1.6, Color::new(0.45, 0.28, 0.15, alfa));
            for lado in [-1.0f32, 1.0] {
                for k in [-0.4f32, 0.0, 0.4] {
                    draw_circle(c.x + lado * r * 0.7, c.y + k * r, r * 0.16, ferro);
                }
            }
        }
        nv::Efeito::Canhonaco => {
            draw_rectangle(c.x - r * 0.75, c.y - r * 0.2, r * 1.1, r * 0.4, ferro);
            draw_circle(c.x - r * 0.75, c.y, r * 0.3, ferro);
            draw_circle(c.x + r * 0.65, c.y, r * 0.2, claro);
        }
        nv::Efeito::Reparo => {
            draw_rectangle(c.x - r * 0.2, c.y - r * 0.7, r * 0.4, r * 1.4, claro);
            draw_rectangle(c.x - r * 0.7, c.y - r * 0.2, r * 1.4, r * 0.4, claro);
        }
        nv::Efeito::Fumaca => {
            for (dx, dy, rr) in [(-0.35f32, 0.15f32, 0.42f32), (0.3, 0.2, 0.36), (0.0, -0.3, 0.4)] {
                draw_circle(c.x + dx * r, c.y + dy * r, rr * r, Color::new(0.85, 0.85, 0.88, alfa * 0.9));
            }
        }
    }
}

// ─────────────────────────── cannon FX ───────────────────────────

struct Tiro {
    efeito: nv::Efeito,
    de: Vec2,
    alvo: Vec2,
    inicio: f64,
}

static TIROS: Mutex<Vec<Tiro>> = Mutex::new(Vec::new());

/// A ship skill landed (`SkillImpactFx` with a ship id): queue its effect.
pub fn efeito(id: u32, de: Vec2, alvo: Vec2) -> bool {
    let Some(h) = nv::habilidade_do_id(id) else {
        return false;
    };
    if let Ok(mut v) = TIROS.lock() {
        v.push(Tiro { efeito: h.efeito, de, alvo, inicio: get_time() });
    }
    true
}

/// Cannonballs, splashes, sparkles and smoke, in the 3D pass.
pub fn desenha_efeitos(vista: &crate::render3d::Vista) {
    let agora = get_time();
    let Ok(mut v) = TIROS.lock() else {
        return;
    };
    v.retain(|t| agora - t.inicio < 3.2);
    let y = crate::agua::ALTURA_DA_AGUA;
    for t in v.iter() {
        let dt = (agora - t.inicio) as f32;
        let p0 = vec3(t.de.x, vista.chao_em(t.de.x, t.de.y).max(y) + 0.8, t.de.y);
        let p1 = vec3(t.alvo.x, y + 0.3, t.alvo.y);
        match t.efeito {
            nv::Efeito::Bordada | nv::Efeito::Canhonaco => {
                let voo = 0.45f32;
                if dt < voo {
                    let f = dt / voo;
                    let q = p0.lerp(p1, f) + vec3(0.0, (f * std::f32::consts::PI).sin() * 1.6, 0.0);
                    draw_sphere(q, 0.18, None, Color::from_rgba(28, 26, 24, 255));
                    // Muzzle smoke at the ship.
                    draw_sphere(p0, 0.35 + f * 0.4, None, Color::new(0.85, 0.85, 0.85, 0.5 * (1.0 - f)));
                } else if dt < voo + 0.6 {
                    let f = (dt - voo) / 0.6;
                    draw_sphere(p1, 0.3 + f * 1.1, None, Color::new(0.9, 0.95, 1.0, 0.6 * (1.0 - f)));
                }
            }
            nv::Efeito::Reparo => {
                if dt < 1.2 {
                    for k in 0..6 {
                        let a = k as f32 * 1.05 + dt * 3.0;
                        let q = p0 + vec3(a.cos() * 1.2, dt * 1.5, a.sin() * 1.2);
                        draw_sphere(q, 0.1, None, Color::new(0.5, 0.95, 0.55, 1.0 - dt / 1.2));
                    }
                }
            }
            nv::Efeito::Fumaca => {
                if dt < nv::FUMACA_S {
                    for k in 0..7 {
                        let a = k as f32 * 0.9;
                        let r = 1.0 + dt * 0.8;
                        let q = p0 + vec3(a.cos() * r, 0.2 + (k % 3) as f32 * 0.4, a.sin() * r);
                        draw_sphere(q, 0.7 + dt * 0.3, None, Color::new(0.78, 0.78, 0.8, 0.55 * (1.0 - dt / nv::FUMACA_S)));
                    }
                }
            }
        }
    }
}

/// PREVIEW (`MMO_PREVIA_MAR=1`): renders the open sea off the Bosque's dock
/// and from high above, with a few hulls, to PNGs in `MMO_PREVIA_SAIDA`;
/// then the four hull classes side by side, plain and dressed (phase 2).
#[cfg(debug_assertions)]
pub async fn previa() {
    let saida = std::env::var("MMO_PREVIA_SAIDA").unwrap_or_else(|_| "/tmp/tempest-mar".into());
    std::fs::create_dir_all(&saida).unwrap();
    define_no_mar(true);
    let rt = render_target_ex(1280, 800, RenderTargetParams { depth: true, sample_count: 1 });
    crate::render3d::define_alvo(Some(rt.clone()));
    crate::gpu_estatica::define_neblina(Vec2::ZERO, 0., 0.);
    let solido = crate::render3d::material_solido();
    let mut terreno = crate::terreno::Terreno::novo(&shared::mar::DEF);
    let bosque = shared::mar::vista_da_zona("ilha_inicial").unwrap();
    let f = bosque.fundeadouro();
    let rumo = (bosque.ponta - bosque.raiz).normalize_or_zero();
    let ancora = vec2(f.x, f.y);
    // Phase 2: the four classes, then the same with stars, sails and
    // figureheads.
    let classes: Vec<Visual> = (0..4).map(|c| Visual { classe: c, ..Default::default() }).collect();
    let vestidos: Vec<Visual> = (0..4)
        .map(|c| Visual { classe: c, vela: c + 1, figura: (c % 3) + 1, estrelas: 5 + c * 3 })
        .collect();
    let frota = [Visual::default(), Visual { classe: 1, vela: 2, ..Default::default() }, Visual { classe: 3, vela: 5, figura: 2, estrelas: 15 }];
    for (nome, alvo, recuo, alto, raio, navios) in [
        ("cais", ancora, 30.0f32, 16.0f32, 12, frota.to_vec()),
        ("rota", ancora + vec2(rumo.x, rumo.y) * 90.0, 40.0, 22.0, 16, frota.to_vec()),
        ("ilha", ancora + vec2(rumo.x, rumo.y) * 60.0, 70.0, 45.0, 24, frota.to_vec()),
        ("alto", vec2(0.0, 0.0), 420.0, 520.0, 40, frota.to_vec()),
        ("classes", ancora + vec2(rumo.x, rumo.y) * 90.0, 22.0, 9.0, 16, classes.clone()),
        ("vestidos", ancora + vec2(rumo.x, rumo.y) * 90.0, 22.0, 9.0, 16, vestidos.clone()),
    ] {
        for _ in 0..60 {
            terreno.atualiza(alvo, raio, 200_000);
        }
        let costas = if nome == "ilha" { vec2(rumo.x, rumo.y) } else { vec2(-rumo.x, -rumo.y) };
        let cam = Camera3D {
            position: vec3(alvo.x + costas.x * recuo * 0.6 + 6.0, alto, alvo.y + costas.y * recuo * 0.6 + recuo * 0.4),
            target: vec3(alvo.x, 0.0, alvo.y),
            up: Vec3::Y,
            fovy: 0.9,
            aspect: Some(1.6),
            render_target: Some(rt.clone()),
            ..Default::default()
        };
        // Side by side across the view for the class shots.
        let lado = vec2(-costas.y, costas.x);
        for _ in 0..3 {
            crate::render3d::camera_padrao();
            clear_background(Color::new(0.55, 0.72, 0.86, 1.0));
            set_camera(&cam);
            macroquad::material::gl_use_material(&solido);
            terreno.desenha(&cam, Vec3::ZERO, 0.);
            let fila = nome == "classes" || nome == "vestidos";
            for (k, v) in navios.iter().enumerate() {
                let d = if fila {
                    lado * ((k as f32 - 1.5) * 6.0)
                } else {
                    [vec2(0.0, 0.0), vec2(8.0, 5.0), vec2(-6.0, 9.0)][k % 3]
                };
                let p = vec3(alvo.x + d.x, 0.0, alvo.y + d.y);
                // Broadside on to the camera in the class shots.
                let yaw = if fila { lado.x.atan2(lado.y) + 0.5 } else { rumo.x.atan2(rumo.y) + k as f32 * 0.6 };
                desenha_casco(p, yaw, k as u64, *v);
            }
            macroquad::material::gl_use_default_material();
            crate::agua::desenha(&terreno, &cam, get_time() as f32);
            macroquad::material::gl_use_default_material();
            crate::render3d::camera_padrao();
            unsafe {
                get_internal_gl().flush();
            }
            rt.texture.get_texture_data().export_png(&format!("{saida}/mar-{nome}.png"));
            next_frame().await;
        }
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn o_casco_sai_do_equipamento() {
        let mut eq = shared::Equipment::default();
        assert!(casco_de(&eq).is_none());
        let id = shared::item_id::NAVIO_CHALUPA;
        eq.set(shared::EquipSlot::Navio, Some(id), Some(nv::instancia_nova(id)));
        assert_eq!(casco_de(&eq), Some((2400, 2400, false)));
        // Hull stars raise the full hull the bar shows.
        let mut inst = nv::instancia_nova(id);
        inst.navio.as_mut().unwrap().estrelas = [5, 0, 0];
        eq.set(shared::EquipSlot::Navio, Some(id), Some(inst));
        assert_eq!(casco_de(&eq).map(|c| c.1), Some(3360));
    }

    /// The treasure on deck comes out of the equipment, and the HUD's
    /// danger line says why you are a target.
    #[test]
    fn o_tesouro_e_o_aviso_de_perigo() {
        let mut eq = shared::Equipment::default();
        let id = shared::item_id::NAVIO_CHALUPA;
        let mut inst = nv::instancia_nova(id);
        eq.set(shared::EquipSlot::Navio, Some(id), Some(inst));
        assert!(tesouro_no_conves(&eq).is_none());
        inst.navio.as_mut().unwrap().conves = Some(nv::tesouro_novo(10, 12, Some(0)));
        eq.set(shared::EquipSlot::Navio, Some(id), Some(inst));
        assert!(tesouro_no_conves(&eq).is_some());
        assert_eq!(linha_de_perigo(true, true, true), Some("You carry treasure — anyone can attack you"));
        assert!(linha_de_perigo(false, true, false).unwrap().starts_with("Marked"));
        assert!(linha_de_perigo(false, false, true).unwrap().starts_with("Lawless"));
        assert_eq!(linha_de_perigo(false, false, false), None);
    }

    #[test]
    fn so_os_canhoes_vao_pro_auto_e_pedem_alcance() {
        let bordada = &nv::HABILIDADES[0];
        let proa = &nv::HABILIDADES[1];
        assert!(alcanca(bordada, Some(10.0)) && !alcanca(bordada, Some(14.0)));
        assert!(alcanca(proa, Some(20.0)) && !alcanca(proa, None));
        assert!(!nv::HABILIDADES[2].auto && !nv::HABILIDADES[3].auto);
    }
}
