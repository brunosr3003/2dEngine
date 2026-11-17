//! THE ISLAND PORTAL in the town (`shared::portal_ilhas`, 17/11/2026): the
//! 3D portal, its label, the banner with the "Travel" button and the walk-in
//! that opens the travel menu.
//!
//! The owner: "create a portal, similar to the porão ones, but a few
//! different because it will be an island teleport portal. Every city will
//! have one and you can use it to teleport."
//!
//! Same family as the Porão portals (`render3d::desenha_portal`): a stone
//! platform, a standing ring of stone blocks and a core of light. Different
//! on purpose, so nobody mistakes one for a dungeon: sea-blue stone with GOLD
//! trim instead of grey, a gold compass rose inlaid in the platform, wave
//! runes on the ring, a slow tide-like swirl (the Porão's spins fast), and a
//! little FLOATING ISLAND turning above the ring — the "this goes to another
//! island" sign you read from across the square.
//!
//! Everything here is a request: the server checks the distance again
//! (`world::perto_do_portal`) and decides every trip.

#[allow(unused_imports)]
use crate::ui::Toque as _;
use macroquad::prelude::*;
use shared::portal_ilhas as pi;
use shared::protocol::ClientMessage;

use crate::hud_estilo as estilo;

/// The name on the label, the banner and the map.
pub const NOME: &str = "Island Portal";

/// The portal's sea blue.
pub const AZUL: Color = Color::new(0.30, 0.68, 0.96, 1.0);
/// Its gold trim.
pub const OURO: Color = Color::new(0.93, 0.76, 0.36, 1.0);

/// This zone's portal and arrival point, in the client's `Vec2`, once the
/// map's thread has found it (`aquecer`): the search never runs on a frame.
pub fn sitio(zona: &str) -> Option<(Vec2, Vec2)> {
    pi::ja_calculado(zona).map(|(p, c)| (vec2(p.x, p.y), vec2(c.x, c.y)))
}

/// The ring faces the arrival point (the town): turned a quarter when the
/// way in runs along X.
pub fn girado(p: Vec2, chegada: Vec2) -> bool {
    let d = chegada - p;
    d.x.abs() > d.y.abs()
}

/// Finds this zone's portal (the map's thread calls it with the island).
pub fn aquecer(zona: &str) -> Option<(Vec2, Vec2)> {
    pi::da_zona(zona).map(|(p, c)| (vec2(p.x, p.y), vec2(c.x, c.y)))
}

#[derive(Default)]
pub struct PortalUi {
    /// The zone the state below belongs to.
    zona: String,
    /// The walk-in fires once per step into the ring: set on leaving it
    /// (with a margin), cleared when it fires. False at login and after a
    /// trip, so arriving next to it (or standing in it) does not re-open it.
    armado: bool,
    /// Where the banner was drawn (its click is the banner's, not the
    /// ground's).
    retangulo: Option<Rect>,
    /// Last request, so a long stand in the ring does not spam.
    pedido_em: f64,
}

impl PortalUi {
    /// Does the banner take this click?
    pub fn pega_mouse(&self, m: Vec2) -> bool {
        self.retangulo.is_some_and(|r| r.toca(m))
    }

    /// The walk-in: stepping into the ring asks for the travel menu. Runs
    /// every frame (also when a panel hides the banner). `menu_aberto` = the
    /// travel menu is already up.
    pub fn passo(&mut self, zona: &str, eu: Option<Vec2>, menu_aberto: bool, agora: f64) -> Option<ClientMessage> {
        if self.zona != zona {
            self.zona = zona.to_string();
            self.armado = false;
        }
        let (p, _) = sitio(zona)?;
        let d = eu?.distance(p);
        if d > pi::ALCANCE + 1.5 {
            self.armado = true;
            return None;
        }
        if d <= pi::ALCANCE && self.armado && !menu_aberto && agora - self.pedido_em > 1.0 {
            self.armado = false;
            self.pedido_em = agora;
            return Some(ClientMessage::AbrirPortal);
        }
        None
    }

    /// The banner when close: the name, what it does and "Travel".
    pub fn desenha_tarja(&mut self, zona: &str, eu: Option<Vec2>, agora: f64) -> Option<ClientMessage> {
        self.retangulo = None;
        let (p, _) = sitio(zona)?;
        let d = eu?.distance(p);
        if d > pi::AVISO {
            return None;
        }
        let f = estilo::fator_texto();
        let seguro = crate::hud_layout::tela_segura();
        let w = 340.0 * f;
        let h = 92.0 * f;
        let r = Rect::new(seguro.center().x - w * 0.5, seguro.y + seguro.h - h - 108.0 * f, w, h);
        estilo::painel_destaque(r, AZUL);
        self.retangulo = Some(r);
        estilo::texto_centro(r.center().x, r.y + 24.0 * f, NOME, 17, OURO);
        estilo::texto_centro(r.center().x, r.y + 44.0 * f, "Travel to the other islands", 13, estilo::SUAVE);
        let m = Vec2::from(mouse_position());
        let botao = Rect::new(r.x + 12.0 * f, r.y + r.h - 38.0 * f, r.w - 24.0 * f, 28.0 * f);
        estilo::botao(botao, "Travel", estilo::estado_de(botao, false, false), true);
        if crate::foco::clique() && botao.toca(m) && agora - self.pedido_em > 0.4 {
            self.pedido_em = agora;
            return Some(ClientMessage::AbrirPortal);
        }
        None
    }

    /// The label over the portal ("Island Portal"), fading with distance.
    pub fn desenha_rotulo(&self, zona: &str, cam: &Camera3D, altura: &dyn Fn(f32, f32) -> f32, eu: Option<Vec2>) {
        let (Some((p, _)), Some(eu)) = (sitio(zona), eu) else {
            return;
        };
        let d = eu.distance(p);
        const ALCANCE: f32 = 60.0;
        if d > ALCANCE {
            return;
        }
        let topo = vec3(p.x, altura(p.x, p.y) + 5.6, p.y);
        let Some(c) = crate::render3d::world_to_screen(cam, topo) else {
            return;
        };
        let a = ((ALCANCE - d) / 12.0).clamp(0.0, 1.0);
        let lw = estilo::medir_forte(NOME, 14) + 22.0;
        let f = estilo::fator_texto();
        let r = Rect::new(c.x - lw * 0.5, c.y - 14.0 * f, lw, 22.0 * f);
        draw_rectangle(r.x, r.y, r.w, r.h, Color::new(0.04, 0.10, 0.18, 0.72 * a));
        draw_rectangle_lines(r.x, r.y, r.w, r.h, 1.5, estilo::alfa(OURO, 0.85 * a));
        estilo::texto_centro(c.x, c.y + 2.0 * f, NOME, 14, estilo::alfa(OURO, a));
    }
}

/// THE ISLAND PORTAL, drawn with cubes like the Porão's (the solid material
/// is on, `draw_line_3d` draws nothing there). `perto` = in walk-in range:
/// brighter and a touch faster.
pub fn desenha(p: Vec3, perto: bool, t: f32) {
    desenha_virado(p, false, perto, t)
}

/// The portal facing the X axis (`girado`) or the Z axis: the ring stands
/// across the way in, turned a quarter (cubes stay axis-aligned).
pub fn desenha_virado(p: Vec3, girado: bool, perto: bool, t: f32) {
    use std::f32::consts::TAU;
    let g = |v: Vec3| if girado { vec3(v.z, v.y, v.x) } else { v };
    // Seen from any side (`cubo_dos_lados`: `draw_cube` alone loses its
    // back face under the world material's culling).
    let cubo = |c: Vec3, s: Vec3, cor: Color| crate::render3d::cubo_dos_lados(p + g(c - p), g(s), cor);
    let pedra = Color::from_rgba(70, 104, 132, 255);
    let pedra_escura = Color::from_rgba(44, 68, 94, 255);
    let ouro = Color::from_rgba(226, 184, 84, 255);
    let ouro_escuro = Color::from_rgba(160, 120, 48, 255);
    let (vel, brilho) = if perto { (1.4, 1.0) } else { (0.8, 0.75) };
    let com = |c: Color, a: f32| Color::new(c.r, c.g, c.b, a);
    // THE PLATFORM: a round dais of blue flagstones with a gold rim.
    cubo(p + vec3(0.0, 0.09, 0.0), vec3(4.2, 0.18, 2.4), pedra_escura);
    for k in 0..18 {
        let a = k as f32 / 18.0 * TAU;
        let q = p + vec3(a.cos() * 2.25, 0.15, a.sin() * 1.35);
        cubo(q, vec3(0.58, 0.3, 0.5), if k % 3 == 0 { ouro_escuro } else if k % 2 == 0 { pedra } else { pedra_escura });
    }
    // The compass rose inlaid in gold: four long points, four short.
    for k in 0..8 {
        let a = k as f32 / 8.0 * TAU;
        let longo = k % 2 == 0;
        let n = if longo { 5 } else { 3 };
        for i in 0..n {
            let r = 0.18 + i as f32 * 0.24;
            let lado = 0.2 * (1.0 - i as f32 / (n as f32 + 1.0));
            cubo(p + vec3(a.cos() * r, 0.205, a.sin() * r * 0.6), vec3(lado, 0.02, lado), ouro);
        }
    }
    // The tide pooled on the dais: a slow breathing glow.
    let pulso = 0.5 + 0.5 * (t * 1.2 * vel).sin();
    cubo(p + vec3(0.0, 0.23, 0.0), vec3(3.0, 0.02, 1.5), com(AZUL, (0.18 + 0.18 * pulso * brilho) / 2.0));
    // Two pillars holding the ring, gold-capped.
    for lado in [-1.0f32, 1.0] {
        let x = lado * 1.95;
        cubo(p + vec3(x, 1.0, 0.0), vec3(0.42, 1.8, 0.42), pedra);
        cubo(p + vec3(x, 2.0, 0.0), vec3(0.56, 0.18, 0.56), ouro);
        cubo(p + vec3(x, 0.24, 0.0), vec3(0.6, 0.16, 0.6), ouro_escuro);
    }
    // THE RING, standing in the XY plane: blue stone, a gold band on every
    // other block, and WAVE RUNES that light up in a slow tide round it.
    let centro = p + vec3(0.0, 2.3, 0.0);
    let n = 26;
    for k in 0..n {
        let a = k as f32 / n as f32 * TAU;
        let q = centro + vec3(a.cos() * 1.75, a.sin() * 1.75, 0.0);
        let runa = k % 4 == 1;
        let onda = (t * 0.55 * vel - a / TAU * 2.0).rem_euclid(1.0);
        let aceso = runa && onda < 0.35;
        let c = if aceso {
            Color::new(AZUL.r + (1.0 - AZUL.r) * 0.4, AZUL.g + (1.0 - AZUL.g) * 0.4, 1.0, 1.0)
        } else if runa {
            com(AZUL, 1.0)
        } else if k % 2 == 0 {
            pedra
        } else {
            pedra_escura
        };
        cubo(q, vec3(0.46, 0.46, 0.54), c);
        // The gold band on the ring's outer face.
        if k % 2 == 0 {
            let fora = centro + vec3(a.cos() * 2.02, a.sin() * 2.02, 0.0);
            cubo(fora, vec3(0.2, 0.2, 0.66), ouro);
        }
    }
    // The four compass points on the ring: gold spikes N, E, S, W.
    for k in 0..4 {
        let a = k as f32 / 4.0 * TAU + TAU / 4.0;
        for i in 0..3 {
            let r = 2.15 + i as f32 * 0.16;
            let lado = 0.26 - i as f32 * 0.07;
            cubo(centro + vec3(a.cos() * r, a.sin() * r, 0.0), vec3(lado, lado, lado), ouro);
        }
    }
    // THE CORE: layers of sea light, and the swirl — motes on a slow
    // spiral, plus three wave arcs rolling round like a tide.
    for k in 0..4 {
        let r = 1.42 - k as f32 * 0.3;
        let a = (0.10 + 0.07 * k as f32) * brilho * (0.82 + 0.18 * pulso);
        cubo(centro, vec3(r * 2.0, r * 2.0, 0.04 + k as f32 * 0.02), com(AZUL, a / 3.5));
    }
    let motes = if perto { 40 } else { 28 };
    for k in 0..motes {
        let f = k as f32 / motes as f32;
        let fase = (f + t * 0.16 * vel).rem_euclid(1.0);
        let raio = 1.45 * (1.0 - fase);
        let a = f * TAU * 2.0 + t * 0.9 * vel + fase * 3.0;
        let q = centro + vec3(a.cos() * raio, a.sin() * raio, 0.0);
        let lado = 0.07 + 0.09 * (1.0 - fase);
        let dourado = k % 5 == 0;
        let base = if dourado { OURO } else { AZUL };
        let branco = fase * 0.55;
        cubo(
            q,
            vec3(lado, lado, lado.max(0.3)),
            Color::new(base.r + (1.0 - base.r) * branco, base.g + (1.0 - base.g) * branco, base.b + (1.0 - base.b) * branco, 0.9 * brilho),
        );
    }
    for onda in 0..3 {
        let a0 = t * 0.5 * vel + onda as f32 * TAU / 3.0;
        for i in 0..9 {
            let a = a0 + i as f32 * 0.11;
            let r = 1.05 + 0.08 * (i as f32 * 0.9 + t * 2.0).sin();
            let q = centro + vec3(a.cos() * r, a.sin() * r, 0.0);
            cubo(q, vec3(0.1, 0.1, 0.3), Color::new(0.85, 0.95, 1.0, 0.75 * brilho * (1.0 - i as f32 / 10.0)));
        }
    }
    // THE FLOATING ISLAND over the ring, bobbing and turning slowly: a
    // grassy top, a rocky underside, a palm and a gold flag.
    let ilha = centro + vec3(0.0, 2.75 + 0.14 * (t * 1.1).sin(), 0.0);
    let giro = t * 0.45;
    let gira = |x: f32, z: f32| vec3(x * giro.cos() - z * giro.sin(), 0.0, x * giro.sin() + z * giro.cos());
    let grama = Color::from_rgba(96, 178, 92, 255);
    let areia = Color::from_rgba(226, 204, 146, 255);
    let rocha = Color::from_rgba(120, 96, 72, 255);
    for (x, z) in [(-0.3f32, -0.3f32), (0.3, -0.3), (-0.3, 0.3), (0.3, 0.3), (0.0, -0.45), (0.0, 0.45), (-0.45, 0.0), (0.45, 0.0)] {
        cubo(ilha + gira(x, z), vec3(0.34, 0.16, 0.34), if x.abs() + z.abs() > 0.5 { areia } else { grama });
    }
    cubo(ilha + vec3(0.0, -0.18, 0.0), vec3(0.62, 0.22, 0.62), rocha);
    cubo(ilha + vec3(0.0, -0.38, 0.0), vec3(0.36, 0.2, 0.36), rocha);
    cubo(ilha + vec3(0.0, -0.54, 0.0), vec3(0.14, 0.14, 0.14), rocha);
    let palmeira = ilha + gira(0.12, 0.08);
    cubo(palmeira + vec3(0.0, 0.28, 0.0), vec3(0.07, 0.42, 0.07), Color::from_rgba(130, 92, 54, 255));
    for k in 0..4 {
        let a = k as f32 / 4.0 * TAU + giro;
        cubo(palmeira + vec3(a.cos() * 0.16, 0.5, a.sin() * 0.16), vec3(0.22, 0.05, 0.08), Color::from_rgba(70, 150, 70, 255));
    }
    let mastro = ilha + gira(-0.2, -0.1);
    cubo(mastro + vec3(0.0, 0.3, 0.0), vec3(0.035, 0.46, 0.035), ouro_escuro);
    cubo(mastro + gira(0.08, 0.0) + vec3(0.0, 0.46, 0.0), vec3(0.14, 0.09, 0.03), ouro);
    // Gold and sea sparks rising round the dais.
    let faiscas = if perto { 18 } else { 12 };
    for k in 0..faiscas {
        let f = k as f32 / faiscas as f32;
        let subida = (t * 0.3 * vel + f * 1.7).rem_euclid(1.0);
        let a = f * TAU + t * 0.3;
        let q = p + vec3(a.cos() * 2.1, 0.3 + subida * 4.0, a.sin() * 1.2);
        let lado = 0.08 * (1.0 - subida) + 0.03;
        cubo(q, vec3(lado, lado, lado), com(if k % 2 == 0 { OURO } else { AZUL }, (1.0 - subida) * brilho));
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    /// The walk-in fires once per step into the ring, never on arrival
    /// (standing in it after a trip), and not while the menu is open.
    #[test]
    fn entrar_no_portal_abre_uma_vez_e_chegar_nao_abre() {
        let zona = "ilha_inicial";
        let (p, chegada) = aquecer(zona).unwrap();
        let mut ui = PortalUi::default();
        // Logged in standing in it: nothing.
        assert!(ui.passo(zona, Some(p), false, 10.0).is_none());
        // Arrived at the arrival point, then walked in: opens once.
        assert!(ui.passo(zona, Some(chegada), false, 11.0).is_none());
        assert!(matches!(ui.passo(zona, Some(p), false, 12.0), Some(ClientMessage::AbrirPortal)));
        assert!(ui.passo(zona, Some(p), false, 14.0).is_none(), "standing in it does not spam");
        // Out and back in with the menu open: nothing; closed: opens.
        ui.passo(zona, Some(chegada), false, 15.0);
        assert!(ui.passo(zona, Some(p), true, 16.0).is_none());
        ui.passo(zona, Some(chegada), false, 17.0);
        assert!(matches!(ui.passo(zona, Some(p + vec2(pi::ALCANCE - 0.2, 0.0)), false, 18.0), Some(ClientMessage::AbrirPortal)));
        // The auto quest stops within 3 u of the point (`auto_missao`): that
        // is inside the ring.
        assert!(pi::ALCANCE > 3.0);
        // The arrival point is outside the ring.
        assert!(chegada.distance(p) > pi::ALCANCE + 1.5);
    }

    #[test]
    fn toda_ilha_da_viagem_tem_portal_no_cliente() {
        for d in shared::terreno::ARQUIPELAGO {
            assert_eq!(aquecer(d.zona), sitio(d.zona), "{}", d.zona);
            assert!(sitio(d.zona).is_some(), "{}", d.zona);
        }
        assert!(aquecer("ilha_magica").is_none());
    }
}
