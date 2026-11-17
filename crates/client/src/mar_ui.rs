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

/// Where a hull points (turning inertia, 16/11/2026): its velocity while
/// it sails, else its drawn heading (`yaw` is `x.atan2(z)`). The server's
/// helm keeps a heading when the ship lies still, so a stopped ship fires
/// off its sides too — never all round.
pub fn rumo_do_casco(vel: ::glam::Vec2, yaw: f32) -> ::glam::Vec2 {
    if vel.length_squared() > 1.0 {
        vel
    } else {
        ::glam::Vec2::new(yaw.sin(), yaw.cos())
    }
}

fn rgb(c: [u8; 3]) -> Color {
    Color::from_rgba(c[0], c[1], c[2], 255)
}

/// THE SHIP of a player at sea: the voxel hull of its class, sail colour,
/// figurehead and brass, riding the swell, with its wake (`mar_casco`,
/// visual remake). The game's models face +Z and the yaw is `x.atan2(z)`.
pub fn desenha_casco(p: Vec3, yaw: f32, semente: u64, v: Visual) {
    crate::mar_casco::desenha(p, yaw, semente, v);
}

/// The ship's hull from the equipped instance: (points, full, damaged). The
/// full hull counts the hull stars (`navio::casco_total`).
pub fn casco_de(eq: &shared::Equipment) -> Option<(u16, u16, bool)> {
    let id = eq.navio.filter(|i| shared::item_id::e_navio(*i))?;
    let d = nv::dados(eq.navio_inst.as_ref(), id);
    Some((d.casco, nv::casco_total(id, &d), d.avariado))
}

// The hull bar is the ship panel's now (`mar_combate_ui::painel_do_navio`,
// gameplay remake): hull, speed, zone, treasure and the nearest dock.

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
pub fn desenha_marca(p: Vec3, yaw: f32, semente: u64, v: Visual) {
    let t = get_time() as f32;
    let k = 1.0;
    let (fx, fz) = (yaw.sin(), yaw.cos());
    let topo = crate::mar_casco::topo_do_mastro(p, yaw, semente, v) + vec3(0.0, 0.5, 0.0);
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
    /// AUTO COMBAT is on: the server's pilot steers and fires the cannons
    /// (`world/piloto_do_mar.rs`); the bar's own AUTO stands down.
    pub auto_combate: bool,
    /// The target is off the beam: a Broadside or the Long Gun would reach
    /// it (`navio::no_costado` on `rumo_do_casco`).
    pub no_costado: bool,
}

/// THE SHIP'S BAR (docs/NAVEGACAO.md, phase 2): at sea, skill slots 1-4
/// are the ship's — Broadside, Long Gun, Emergency Repair, Smoke. Same
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
                Some("Select an enemy in range of the long gun")
            } else if h.efeito == nv::Efeito::Canhonaco && !c.no_costado {
                Some("Turn your side to the target to fire")
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
        // AUTO COMBAT at sea: the SERVER's pilot fires (`PedidoNavio::Piloto`,
        // `world/piloto_do_mar.rs`), on the beam and on time; a request from
        // here too would only be refused on the cooldown.
        if !c.pronto || agora < self.pendente_ate || c.auto_combate {
            return None;
        }
        let h = nv::HABILIDADES.iter().find(|h| {
            h.auto
                && self.automaticas.contains(&h.id)
                && espera(h, hab, agora) == 0.0
                && self.tentar_apos[h.id as usize] <= agora
                && alcanca(h, c.distancia_alvo)
                // A Broadside with the target dead ahead or astern would
                // spend its cooldown on the water.
                && (h.efeito != nv::Efeito::Bordada || c.no_costado)
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

/// A VECTOR ICON per ship skill (no atlas art for them), drawn in strokes
/// like `arvore_icones` so it reads at thumb size (gameplay remake).
pub fn icone_da_habilidade(e: nv::Efeito, c: Vec2, r: f32, alfa: f32) {
    let ferro = Color::new(0.1, 0.1, 0.12, alfa);
    let claro = Color::new(0.97, 0.93, 0.84, alfa);
    let madeira = Color::new(0.55, 0.34, 0.17, alfa);
    let fogo = Color::new(1.0, 0.72, 0.25, alfa);
    match e {
        nv::Efeito::Bordada => {
            // A hull from above, bow up, firing out of BOTH sides.
            draw_rectangle(c.x - r * 0.24, c.y - r * 0.35, r * 0.48, r * 1.1, madeira);
            draw_triangle(vec2(c.x - r * 0.24, c.y - r * 0.35), vec2(c.x + r * 0.24, c.y - r * 0.35), vec2(c.x, c.y - r * 0.85), madeira);
            draw_line(c.x, c.y - r * 0.7, c.x, c.y + r * 0.7, r * 0.06, ferro);
            for lado in [-1.0f32, 1.0] {
                for k in [-0.15f32, 0.2, 0.55] {
                    let y = c.y + k * r;
                    let x0 = c.x + lado * r * 0.26;
                    draw_triangle(vec2(x0, y - r * 0.1), vec2(x0, y + r * 0.1), vec2(x0 + lado * r * 0.36, y), fogo);
                    draw_circle(x0 + lado * r * 0.6, y, r * 0.1, ferro);
                }
            }
        }
        nv::Efeito::Canhonaco => {
            // A cannon on its carriage, a ball leaving with speed lines.
            let base = c + vec2(-r * 0.2, r * 0.3);
            let ponta = c + vec2(r * 0.45, -r * 0.25);
            draw_line(base.x, base.y, ponta.x, ponta.y, r * 0.34, ferro);
            draw_circle(base.x, base.y, r * 0.2, ferro);
            draw_circle(c.x - r * 0.35, c.y + r * 0.58, r * 0.2, madeira);
            draw_circle(c.x + r * 0.05, c.y + r * 0.58, r * 0.2, madeira);
            draw_circle(ponta.x + r * 0.32, ponta.y - r * 0.22, r * 0.15, claro);
            for k in 0..3 {
                let o = k as f32 * r * 0.12;
                draw_line(ponta.x - r * 0.05 + o, ponta.y - r * 0.42 + o * 0.6, ponta.x + r * 0.15 + o, ponta.y - r * 0.55 + o * 0.6, r * 0.05, fogo);
            }
        }
        nv::Efeito::Reparo => {
            // A plank and a hammer, and the green cross of a heal.
            draw_rectangle(c.x - r * 0.75, c.y + r * 0.2, r * 1.5, r * 0.32, madeira);
            draw_line(c.x - r * 0.55, c.y + r * 0.1, c.x + r * 0.35, c.y - r * 0.6, r * 0.12, Color::new(0.62, 0.45, 0.26, alfa));
            draw_rectangle(c.x + r * 0.2, c.y - r * 0.8, r * 0.38, r * 0.24, ferro);
            let verde = Color::new(0.45, 0.95, 0.5, alfa);
            draw_rectangle(c.x - r * 0.7, c.y - r * 0.62, r * 0.14, r * 0.46, verde);
            draw_rectangle(c.x - r * 0.86, c.y - r * 0.46, r * 0.46, r * 0.14, verde);
        }
        nv::Efeito::Fumaca => {
            // A ship vanishing into the cloud.
            draw_triangle(vec2(c.x - r * 0.55, c.y + r * 0.35), vec2(c.x + r * 0.55, c.y + r * 0.35), vec2(c.x + r * 0.35, c.y + r * 0.6), Color::new(0.35, 0.24, 0.14, alfa * 0.6));
            for (dx, dy, rr) in [(-0.4f32, 0.05f32, 0.36f32), (0.32, 0.1, 0.32), (0.0, -0.3, 0.4), (0.45, -0.35, 0.22)] {
                draw_circle(c.x + dx * r, c.y + dy * r, rr * r, Color::new(0.86, 0.86, 0.9, alfa * 0.92));
            }
        }
    }
}

/// PREVIEW (`MMO_PREVIA_MAR=1`): the sea's scenes to PNGs in
/// `MMO_PREVIA_SAIDA` (`mar_cena::previa`, visual remake).
#[cfg(debug_assertions)]
pub async fn previa() {
    crate::mar_cena::previa().await;
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
