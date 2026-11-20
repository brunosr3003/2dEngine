//! THE SHIPYARD (17/11/2026): what the dock NPCs open — the Harbour Captain,
//! the Sky Bus Professor and the Submarine Captain. Ships only: trips between
//! islands moved to the Island Portal (`portal_ilhas_ui`).
//!
//! The owner: "in the dock NPC remake all the HUD, the HUD is terrible. I
//! need a 3D preview of the ship there and a more beautiful HUD, like if you
//! were crafting / repairing a ship indeed."
//!
//! Centre stage is the player's own ship, the real hull model
//! (`mar_casco::desenha`, class, brass trim, sail and figurehead), afloat in a
//! wet dock on keel blocks, under a studio light. It turns slowly; a drag
//! turns it, the wheel or a pinch zooms. Drawn straight into the main pass
//! through a viewport (`render3d::viewport_na_tela`) like the character
//! preview — a render target with depth comes out empty on the iPhone.
//!
//! Five tabs, nautical cousins of the Forge's: Overview, Repair, Upgrade,
//! Build, Cosmetics. Hovering an upgrade previews it on the ship (the next
//! hull ghosted, a sail colour applied). The server decides everything
//! (`world/navio.rs`); the panel only asks.

#[allow(unused_imports)]
use crate::ui::Toque as _;
use macroquad::prelude::*;
use shared::item_id;
use shared::navio::{self as nv, Eixo, EstadoNavio, Visual};

use crate::hud_estilo as estilo;

/// What the player asked for in the Shipyard.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Escolha {
    Zarpar,
    Reparar,
    Construir,
    ConstruirTipo(u8),
    Alocar([u8; 4]),
    Respec,
    UpgradeTier,
    /// One star on axis `Eixo` as u8.
    Melhorar(u8),
    /// Apply the sail/figurehead in bag slot `slot` (`ClientMessage::UseItem`).
    Vestir(u16),
    /// The TP shop on its Ship category.
    LojaDoNavio,
    /// "Where to get" for a missing material.
    OndeObter(u16),
}

/// The tabs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Aba {
    #[default]
    Visao,
    Reparo,
    Melhorar,
    Construir,
    Cosmeticos,
}

impl Aba {
    pub const TODAS: [Aba; 5] = [
        Aba::Visao,
        Aba::Reparo,
        Aba::Melhorar,
        Aba::Construir,
        Aba::Cosmeticos,
    ];

    pub fn nome(self) -> &'static str {
        match self {
            Aba::Visao => "Overview",
            Aba::Reparo => "Repair",
            Aba::Melhorar => "Upgrade",
            Aba::Construir => "Build",
            Aba::Cosmeticos => "Cosmetics",
        }
    }
}

/// What the panel reads from the client.
pub struct Contexto<'a> {
    pub equip: &'a shared::Equipment,
    pub slots: &'a [shared::InventorySlot],
    pub conta: &'a dyn Fn(u16) -> u64,
    pub nome: &'a dyn Fn(u16) -> String,
    pub nivel: u32,
    pub ouro: u64,
    pub solido: &'a Material,
}

/// An animation on the preview: what, and when it started.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Anima {
    /// Hammer and planks; the flourish when the hull comes back full.
    Reparo,
    /// Sparks and brass.
    Estrela,
    /// Scaffolding and a launch.
    Obra,
    /// "Repaired!" / "Upgraded!" / "Launched!".
    Festa(&'static str),
}

/// What the ship shows while the pointer is on an upgrade.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Previa {
    /// The next hull, ghosted.
    Casco(u16),
    /// A cosmetic applied (`Cosmetico`).
    Vestido(nv::Cosmetico),
    Tipo(u8),
}

#[derive(Default)]
pub struct EstaleiroUi {
    estado: Option<EstadoNavio>,
    pub aba: Aba,
    /// The camera's turn round the ship, and the zoom (1 = framed).
    orbita: f32,
    zoom: f32,
    /// Dragging the preview: last pointer, and when the user last touched
    /// it (the slow turn waits a little after a drag).
    arrasto: Option<Vec2>,
    tocado_em: f64,
    pinca: Option<f32>,
    previa: Option<Previa>,
    anima: Option<(Anima, f64)>,
    /// Where the panel was drawn: clicks inside are the panel's.
    retangulo: Option<Rect>,
    pontos: [u8; 4],
    build_carregada: bool,
    tipo_selecionado: u8,
}

const ANIMA_S: f64 = 1.6;
const FESTA_S: f64 = 1.8;

/// Warm wood, brass and navy.
const MADEIRA: Color = Color::new(0.30, 0.19, 0.11, 0.98);
const MADEIRA_CLARA: Color = Color::new(0.42, 0.28, 0.16, 1.0);
const LATAO: Color = Color::new(0.86, 0.68, 0.34, 1.0);
const LATAO_ESCURO: Color = Color::new(0.56, 0.42, 0.20, 1.0);
const MARINHO: Color = Color::new(0.06, 0.11, 0.19, 1.0);
const MARINHO_CLARO: Color = Color::new(0.13, 0.25, 0.37, 1.0);

impl EstaleiroUi {
    pub fn abrir(&mut self, estado: EstadoNavio) {
        let novo = self.estado.is_none();
        self.estado = Some(estado);
        if novo {
            self.aba = if estado.tem && (estado.avariado || estado.casco < estado.casco_max) {
                Aba::Reparo
            } else {
                Aba::Visao
            };
            self.zoom = 1.0;
            // A three-quarter view off the bow to start.
            self.orbita = 0.85;
            self.anima = None;
        }
    }

    /// A new ship state (`ServerMessage::Navio`) while open: a repair that
    /// landed plays the flourish.
    pub fn define_navio(&mut self, estado: EstadoNavio) {
        let Some(antes) = self.estado else {
            return;
        };
        if estado.tem && antes.casco < antes.casco_max && estado.casco >= estado.casco_max {
            self.anima = Some((Anima::Festa("Repaired!"), relogio()));
            #[cfg(not(test))]
            crate::sons::tocar(crate::sons::Som::Recompensa);
        }
        self.estado = Some(estado);
    }

    /// The preview harness (`previa_porta::estaleiro`): start an animation.
    #[cfg(debug_assertions)]
    pub fn previa_anima(&mut self, k: u8) {
        let a = match k {
            0 => Anima::Reparo,
            1 => Anima::Estrela,
            2 => Anima::Obra,
            _ => Anima::Festa("Repaired!"),
        };
        self.anima = Some((a, get_time()));
    }

    pub fn fechar(&mut self) {
        self.estado = None;
        self.retangulo = None;
        self.arrasto = None;
        self.previa = None;
    }

    pub fn aberto(&self) -> bool {
        self.estado.is_some()
    }

    pub fn pega_mouse(&self, m: Vec2) -> bool {
        // Modal: the whole screen is the panel's while it is open.
        let _ = m;
        self.aberto()
    }

    /// Draws the panel; the player's request, if any.
    pub fn desenha(&mut self, ctx: &Contexto) -> Option<Escolha> {
        self.estado?;
        estilo::no_painel(estilo::escala_do_painel(900.0, 560.0), || {
            self.desenha_na_escala(ctx)
        })
    }

    fn desenha_na_escala(&mut self, ctx: &Contexto) -> Option<Escolha> {
        let estado = self.estado?;
        let f = estilo::fator_texto();
        let seguro = crate::hud_layout::tela_segura();
        let retrato = seguro.h > seguro.w * 1.08;
        let (w, h) = if retrato {
            (
                (640.0 * f).min(seguro.w - 12.0),
                (1000.0 * f).min(seguro.h - 12.0),
            )
        } else {
            (
                (960.0 * f).min(seguro.w - 12.0),
                (600.0 * f).min(seguro.h - 12.0),
            )
        };
        let p = Rect::new(
            seguro.center().x - w * 0.5,
            seguro.center().y - h * 0.5,
            w,
            h,
        );
        self.retangulo = Some(p);
        crate::hud_layout::escurece(0.5);
        moldura(p, f);
        let m = Vec2::from(mouse_position());
        let clicou = crate::foco::clique();
        let agora = get_time();
        let navio = ctx.equip.navio.filter(|i| item_id::e_navio(*i));
        let dados = navio.map(|id| (id, nv::dados(ctx.equip.navio_inst.as_ref(), id)));
        if let Some((_, d)) = dados {
            if !self.build_carregada {
                self.pontos = d.pontos;
                self.build_carregada = true;
            }
        } else {
            self.pontos = [0; 4];
            self.build_carregada = false;
        }
        // ── header ──
        let x0 = p.x + 18.0 * f;
        let topo = p.y + 14.0 * f;
        estilo::texto_forte(x0, topo + 22.0 * f, "Shipyard", 21, LATAO);
        let sub = match dados {
            Some((id, d)) => classe_e_estrelas(id, &d),
            None => "No ship in the Ship slot".to_string(),
        };
        estilo::texto(x0, topo + 42.0 * f, &sub, 13, estilo::SUAVE);
        let fechar = Rect::new(p.x + p.w - 50.0 * f, p.y + 10.0 * f, 40.0 * f, 40.0 * f);
        estilo::cartao(fechar, fechar.toca(m), false);
        estilo::texto_centro(
            fechar.center().x,
            fechar.center().y + 7.0 * f,
            "X",
            18,
            estilo::TEXTO,
        );
        let corpo = Rect::new(
            p.x + 14.0 * f,
            p.y + 62.0 * f,
            p.w - 28.0 * f,
            p.h - 76.0 * f,
        );
        // ── the stage and the tabs ──
        let (palco, lado) = if retrato {
            let ph = (corpo.h * 0.40).max(160.0 * f);
            (
                Rect::new(corpo.x, corpo.y, corpo.w, ph),
                Rect::new(
                    corpo.x,
                    corpo.y + ph + 10.0 * f,
                    corpo.w,
                    corpo.h - ph - 10.0 * f,
                ),
            )
        } else {
            let pw = corpo.w * 0.5;
            (
                Rect::new(corpo.x, corpo.y, pw, corpo.h),
                Rect::new(
                    corpo.x + pw + 12.0 * f,
                    corpo.y,
                    corpo.w - pw - 12.0 * f,
                    corpo.h,
                ),
            )
        };
        // The Set Sail button lives under the stage.
        let zarpar_h = 46.0 * f;
        let cena = Rect::new(palco.x, palco.y, palco.w, palco.h - zarpar_h - 8.0 * f);
        let botao_zarpar = Rect::new(palco.x, palco.y + palco.h - zarpar_h, palco.w, zarpar_h);
        // The tab bar.
        let abas_h = 38.0 * f;
        let aw = lado.w / Aba::TODAS.len() as f32;
        let mut escolha = None;
        self.previa = None;
        for (k, a) in Aba::TODAS.iter().enumerate() {
            let r = Rect::new(lado.x + k as f32 * aw + 2.0, lado.y, aw - 4.0, abas_h);
            estilo::aba(r, a.nome(), self.aba == *a, r.toca(m));
            if clicou && r.toca(m) {
                if *a == Aba::Melhorar && self.aba != *a {
                    self.pontos = dados.map_or([0; 4], |(_, d)| d.pontos);
                }
                self.aba = *a;
                crate::sons::tocar(crate::sons::Som::Clique);
            }
        }
        let conteudo = Rect::new(
            lado.x,
            lado.y + abas_h + 8.0 * f,
            lado.w,
            lado.h - abas_h - 8.0 * f,
        );
        estilo::ret_arredondado(conteudo, 8.0 * f, Color::new(0.08, 0.06, 0.04, 0.55));
        let dentro = Rect::new(
            conteudo.x + 12.0 * f,
            conteudo.y + 10.0 * f,
            conteudo.w - 24.0 * f,
            conteudo.h - 20.0 * f,
        );
        match dados {
            None => {
                if self.aba == Aba::Construir {
                    escolha = self.aba_galeria(dentro, f, m, clicou, ctx);
                } else {
                    estilo::texto(
                        dentro.x,
                        dentro.y + 24.0 * f,
                        "No ship equipped. Craft one in the Build gallery",
                        14,
                        estilo::TEXTO,
                    );
                    estilo::texto(
                        dentro.x,
                        dentro.y + 46.0 * f,
                        "or earn your first Swift Sloop in the story.",
                        14,
                        estilo::TEXTO,
                    );
                }
            }
            Some((id, d)) => {
                escolha = match self.aba {
                    Aba::Visao => {
                        aba_visao(dentro, f, id, &d, &estado);
                        None
                    }
                    Aba::Reparo => self.aba_reparo(dentro, f, m, clicou, id, &d, &estado, ctx),
                    Aba::Melhorar => self.aba_pontos(dentro, f, m, clicou, id, &d, ctx),
                    Aba::Construir => self.aba_galeria(dentro, f, m, clicou, ctx),
                    Aba::Cosmeticos => self.aba_cosmeticos(dentro, f, m, clicou, &d, ctx),
                };
            }
        }
        // ── the stage: backdrop, 3D ship, animations ──
        self.arrastar(cena, m, agora);
        fundo_do_palco(cena, f, agora as f32);
        let dados_palco = dados.or_else(|| {
            (self.aba == Aba::Construir).then(|| {
                let tipo = nv::TipoBarco::de_id(self.tipo_selecionado).unwrap();
                (item_id::NAVIO_CHALUPA, nv::novo_tipo(tipo, 1))
            })
        });
        if let Some((id, d)) = dados_palco {
            let mut v = Visual::de(id, &d);
            let mut fantasma = false;
            match self.previa {
                Some(Previa::Casco(para)) => {
                    v = Visual::de(para, &nv::reconstruido(para, &d));
                    fantasma = true;
                }
                Some(Previa::Vestido(c)) => {
                    let mut d2 = d;
                    nv::aplicar(&mut d2, c);
                    v = Visual::de(id, &d2);
                }
                Some(Previa::Tipo(tipo)) => {
                    let tipo = nv::TipoBarco::de_id(tipo).unwrap();
                    v = Visual::de(id, &nv::novo_tipo(tipo, 1));
                    fantasma = true;
                }
                None => {}
            }
            if self.arrasto.is_none() && agora - self.tocado_em > 2.5 {
                self.orbita += get_frame_time() * 0.25;
            }
            let obra = self
                .anima
                .is_some_and(|(a, t)| a == Anima::Obra && agora - t < ANIMA_S);
            desenha_navio(
                cena,
                v,
                self.orbita,
                self.zoom,
                fantasma || obra,
                ctx.solido,
            );
            if fantasma {
                estilo::texto_centro(
                    cena.center().x,
                    cena.y + 26.0 * f,
                    "Preview: next hull",
                    14,
                    LATAO,
                );
            }
        } else {
            estilo::texto_centro(
                cena.center().x,
                cena.center().y,
                "The slip is empty",
                16,
                estilo::SUAVE,
            );
        }
        self.desenha_anima(cena, f, agora);
        estilo::texto(
            cena.x + 10.0 * f,
            cena.y + cena.h - 10.0 * f,
            "drag to turn · wheel to zoom",
            11,
            estilo::alfa(estilo::SUAVE, 0.8),
        );
        // ── Set Sail ──
        let motivo = motivo_sem_zarpar(&estado);
        let pode = motivo.is_none();
        estilo::botao(
            botao_zarpar,
            "Set Sail",
            estilo::estado_de(botao_zarpar, !pode, false),
            pode,
        );
        if let Some(mo) = motivo {
            estilo::texto_centro(
                botao_zarpar.center().x,
                botao_zarpar.y - 6.0 * f,
                mo,
                12,
                estilo::VERMELHO,
            );
        }
        if pode && clicou && botao_zarpar.toca(m) {
            escolha = Some(Escolha::Zarpar);
        }
        // ── closing ──
        if matches!(escolha, Some(Escolha::Zarpar | Escolha::LojaDoNavio))
            || (clicou && (fechar.toca(m) || !p.toca(m)))
        {
            self.fechar();
        }
        escolha
    }

    /// Drag turns, wheel and pinch zoom.
    fn arrastar(&mut self, cena: Rect, m: Vec2, agora: f64) {
        let toques = touches();
        if toques.len() >= 2 {
            let d = toques[0].position.distance(toques[1].position);
            if let Some(antes) = self.pinca {
                if antes > 1.0 {
                    self.zoom = (self.zoom * d / antes).clamp(0.6, 2.4);
                }
            }
            self.pinca = Some(d);
            self.arrasto = None;
            self.tocado_em = agora;
            return;
        }
        self.pinca = None;
        if is_mouse_button_down(MouseButton::Left) {
            match self.arrasto {
                Some(antes) => {
                    self.orbita -= (m.x - antes.x) * 0.012;
                    self.arrasto = Some(m);
                    self.tocado_em = agora;
                }
                None if is_mouse_button_pressed(MouseButton::Left) && cena.toca(m) => {
                    self.arrasto = Some(m)
                }
                None => {}
            }
        } else {
            self.arrasto = None;
        }
        let roda = mouse_wheel().1;
        if roda != 0.0 && cena.toca(m) {
            self.zoom = (self.zoom * if roda > 0.0 { 1.1 } else { 1.0 / 1.1 }).clamp(0.6, 2.4);
            self.tocado_em = agora;
        }
    }

    fn desenha_anima(&mut self, cena: Rect, f: f32, agora: f64) {
        let Some((a, t0)) = self.anima else {
            return;
        };
        let t = (agora - t0) as f32;
        let fim = if matches!(a, Anima::Festa(_)) {
            FESTA_S
        } else {
            ANIMA_S
        } as f32;
        if t > fim {
            self.anima = match a {
                // A star and a build show their flourish at once; a repair
                // waits for the server's new hull (`define_navio`).
                Anima::Estrela => Some((Anima::Festa("Upgraded!"), agora)),
                Anima::Obra => Some((Anima::Festa("Launched!"), agora)),
                _ => None,
            };
            return;
        }
        let c = cena.center();
        match a {
            Anima::Reparo => {
                // A hammer swinging over the hull, planks flying in.
                let golpe = (t * 5.0).fract();
                let ang = -0.9 + golpe.powf(0.5) * 1.5;
                let base = c + vec2(cena.w * 0.18, -cena.h * 0.05);
                martelo(base, ang, 46.0 * f);
                if golpe < 0.08 && (t * 5.0) as i32 != ((t - get_frame_time()) * 5.0) as i32 {
                    crate::sons::tocar(crate::sons::Som::Machado);
                }
                for k in 0..6 {
                    let fk = ((t * 0.9 + k as f32 * 0.17) % 1.0).max(0.0);
                    let x = cena.x + cena.w * (0.15 + 0.12 * k as f32);
                    let y = cena.y + cena.h * (1.0 - fk * 0.55);
                    draw_rectangle(
                        x,
                        y,
                        26.0 * f,
                        6.0 * f,
                        Color::new(0.62, 0.42, 0.22, 1.0 - fk),
                    );
                }
            }
            Anima::Estrela => {
                // Sparks bursting from the hull, brass glints.
                for k in 0..40 {
                    let a = k as f32 * 2.399;
                    let r = (t * 140.0 + (k % 7) as f32 * 12.0) * f;
                    let q = c + vec2(a.cos(), a.sin() * 0.6) * r;
                    let al = (1.0 - t / ANIMA_S as f32).max(0.0);
                    draw_circle(
                        q.x,
                        q.y,
                        (2.5 - (k % 3) as f32 * 0.6) * f,
                        Color::new(1.0, 0.85, 0.45, al),
                    );
                }
                draw_circle_lines(
                    c.x,
                    c.y,
                    t * 160.0 * f,
                    2.0 * f,
                    estilo::alfa(LATAO, 1.0 - t / ANIMA_S as f32),
                );
            }
            Anima::Obra => {
                // Scaffolding rising round the hull, beams going in.
                let sobe = (t / ANIMA_S as f32).min(1.0);
                for k in 0..5 {
                    let x = cena.x + cena.w * (0.14 + 0.18 * k as f32);
                    let h = cena.h * 0.55 * sobe;
                    let y0 = cena.y + cena.h * 0.85;
                    estilo::traco(
                        vec2(x, y0),
                        vec2(x, y0 - h),
                        4.0 * f,
                        Color::new(0.55, 0.38, 0.2, 0.9),
                    );
                    if k < 4 {
                        let y = y0 - h * 0.6;
                        estilo::traco(
                            vec2(x, y),
                            vec2(x + cena.w * 0.18, y),
                            3.0 * f,
                            Color::new(0.48, 0.32, 0.18, 0.9),
                        );
                    }
                }
                if (t * 4.0) as i32 != ((t - get_frame_time()) * 4.0) as i32 {
                    crate::sons::tocar(crate::sons::Som::Machado);
                }
            }
            Anima::Festa(texto) => {
                let s = 1.0 + (1.0 - (t * 4.0).min(1.0)) * 0.6;
                let al = (1.0 - (t - 1.2).max(0.0) / 0.6).clamp(0.0, 1.0);
                let tam = (26.0 * s) as u16;
                let largura = estilo::medir_forte(texto, tam) + 30.0 * f;
                let r = Rect::new(
                    c.x - largura * 0.5,
                    c.y - 26.0 * f * s,
                    largura,
                    44.0 * f * s,
                );
                estilo::ret_arredondado(r, 10.0 * f, Color::new(0.05, 0.09, 0.16, 0.8 * al));
                estilo::texto_centro(c.x, c.y + 4.0 * f, texto, tam, estilo::alfa(LATAO, al));
                for k in 0..14 {
                    let a = k as f32 / 14.0 * std::f32::consts::TAU + t;
                    let q = c + vec2(a.cos(), a.sin() * 0.5) * (70.0 + t * 40.0) * f;
                    draw_circle(q.x, q.y, 3.0 * f, Color::new(1.0, 0.86, 0.5, al));
                }
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn aba_reparo(
        &mut self,
        r: Rect,
        f: f32,
        m: Vec2,
        clicou: bool,
        id: u16,
        d: &shared::items::NavioData,
        e: &EstadoNavio,
        ctx: &Contexto,
    ) -> Option<Escolha> {
        let max = nv::casco_total(id, d).max(1);
        let frac = d.casco as f32 / max as f32;
        estilo::texto_forte(r.x, r.y + 20.0 * f, "Hull condition", 16, estilo::TEXTO);
        // The hull from the side, cracked where it is damaged.
        let sil = Rect::new(r.x, r.y + 32.0 * f, r.w, (r.h * 0.42).min(150.0 * f));
        silhueta_avariada(sil, f, frac, (ctx.nome)(id).len() as u32);
        let y = sil.y + sil.h + 12.0 * f;
        barra(
            Rect::new(r.x, y, r.w, 16.0 * f),
            frac,
            if frac < 0.35 {
                estilo::VERMELHO
            } else if frac < 0.7 {
                LATAO
            } else {
                estilo::VERDE
            },
        );
        estilo::texto_centro(
            r.center().x,
            y + 13.0 * f,
            &format!("{} / {}", d.casco, max),
            12,
            estilo::TEXTO,
        );
        let mut yy = y + 40.0 * f;
        let linha = if e.avariado {
            "Damaged: the ship cannot sail until it is repaired."
        } else if e.reparo == 0 {
            "The hull is sound. Nothing to repair."
        } else if frac < 0.5 {
            "Badly battered: cracked planks below the waterline."
        } else {
            "A few planks loose. A repair brings the hull back to full."
        };
        estilo::texto(
            r.x,
            yy,
            linha,
            13,
            if e.avariado {
                estilo::VERMELHO
            } else {
                estilo::SUAVE
            },
        );
        yy += 14.0 * f;
        if e.reparo > 0 {
            let tem = (ctx.conta)(item_id::COPPER);
            let ok = tem >= e.reparo as u64;
            let ic = Rect::new(r.x, yy + 6.0 * f, 34.0 * f, 34.0 * f);
            crate::icones::icone(item_id::COPPER, ic, None, None);
            estilo::texto(
                ic.x + ic.w + 8.0 * f,
                ic.y + 22.0 * f,
                &format!("{} copper", e.reparo),
                15,
                if ok { estilo::VERDE } else { estilo::VERMELHO },
            );
            let tw = estilo::medir(&format!("{} copper", e.reparo), 15);
            estilo::texto(
                ic.x + ic.w + 16.0 * f + tw,
                ic.y + 22.0 * f,
                &format!("(you: {tem})"),
                12,
                estilo::SUAVE,
            );
            let b = Rect::new(
                r.x,
                (r.y + r.h - 54.0 * f).max(ic.y + ic.h + 8.0 * f),
                r.w,
                50.0 * f,
            );
            estilo::botao(b, "Repair", estilo::estado_de(b, !ok, false), ok);
            martelo(vec2(b.x + 28.0 * f, b.center().y + 4.0 * f), -0.5, 22.0 * f);
            if ok && clicou && b.toca(m) {
                self.anima = Some((Anima::Reparo, get_time()));
                return Some(Escolha::Reparar);
            }
        }
        None
    }

    #[allow(clippy::too_many_arguments)]
    fn aba_pontos(
        &mut self,
        r: Rect,
        f: f32,
        m: Vec2,
        clicou: bool,
        id: u16,
        d: &shared::items::NavioData,
        ctx: &Contexto,
    ) -> Option<Escolha> {
        let Some(tipo) = nv::tipo_novo(d) else {
            return None;
        };
        let limite = nv::pontos_no_tier(d.tier).unwrap_or(0);
        let usados: u16 = self.pontos.iter().map(|p| *p as u16).sum();
        estilo::texto_forte(
            r.x,
            r.y + 20.0 * f,
            &format!("{} · T{} · {} crew", tipo.nome(), d.tier, tipo.capacidade()),
            16,
            LATAO,
        );
        estilo::texto(
            r.x,
            r.y + 40.0 * f,
            &format!("Build points {usados}/{limite} · +/− to preview"),
            12,
            estilo::SUAVE,
        );
        let nomes = ["Speed", "Hull", "Cannon", "Turning"];
        let mut escolha = None;
        let linha = 48.0 * f;
        for (i, nome) in nomes.iter().enumerate() {
            let y = r.y + 50.0 * f + i as f32 * linha;
            let mut novo = *d;
            novo.pontos = self.pontos;
            let stats = nv::stats_finais(id, &novo);
            let valor = match i {
                0 => format!("{:.2} u/s", stats.velocidade),
                1 => format!("{:.0} HP", stats.hp),
                2 => format!("×{:.2}", stats.dano),
                _ => format!("{:.2} rad/s", stats.virada),
            };
            estilo::texto(r.x, y + 20.0 * f, nome, 13, estilo::TEXTO);
            estilo::texto(r.x + 82.0 * f, y + 20.0 * f, &valor, 12, estilo::SUAVE);
            let menos = Rect::new(r.x + r.w - 142.0 * f, y, 34.0 * f, 32.0 * f);
            let mais = Rect::new(r.x + r.w - 42.0 * f, y, 34.0 * f, 32.0 * f);
            let pode_menos = self.pontos[i] > 0;
            let pode_mais = usados < limite as u16;
            estilo::botao(
                menos,
                "−",
                estilo::estado_de(menos, !pode_menos, false),
                pode_menos,
            );
            estilo::botao(
                mais,
                "+",
                estilo::estado_de(mais, !pode_mais, false),
                pode_mais,
            );
            estilo::texto_centro(
                r.x + r.w - 82.0 * f,
                y + 21.0 * f,
                &self.pontos[i].to_string(),
                15,
                LATAO,
            );
            if clicou && pode_menos && menos.toca(m) {
                self.pontos[i] -= 1;
            }
            if clicou && pode_mais && mais.toca(m) {
                self.pontos[i] += 1;
            }
        }
        let y = r.y + 50.0 * f + 4.0 * linha + 6.0 * f;
        let pode_aplicar =
            self.pontos != d.pontos && nv::alocacao_valida(d.pontos, self.pontos, d.tier);
        let aplicar = Rect::new(r.x, y, r.w * 0.58, 40.0 * f);
        let respec = Rect::new(r.x + r.w * 0.62, y, r.w * 0.38, 40.0 * f);
        let custo_respec = nv::custo_respec(d.tier);
        let pode_respec = d.pontos != [0; 4] && ctx.ouro >= custo_respec;
        estilo::botao(
            aplicar,
            "Apply build",
            estilo::estado_de(aplicar, !pode_aplicar, false),
            pode_aplicar,
        );
        estilo::botao(
            respec,
            &format!("Respec · {custo_respec}g"),
            estilo::estado_de(respec, !pode_respec, false),
            pode_respec,
        );
        if clicou && pode_aplicar && aplicar.toca(m) {
            escolha = Some(Escolha::Alocar(self.pontos));
        } else if clicou && pode_respec && respec.toca(m) {
            self.pontos = [0; 4];
            escolha = Some(Escolha::Respec);
        }
        let tier_y = y + 52.0 * f;
        estilo::ret_arredondado(
            Rect::new(r.x, tier_y, r.w, (r.y + r.h - tier_y).max(74.0 * f)),
            6.0 * f,
            Color::new(0.12, 0.09, 0.05, 0.8),
        );
        let materiais = nv::tipo_novo(d)
            .map(|t| nv::materiais_upgrade(t, d.tier))
            .unwrap_or_default();
        if let Some(custo) = nv::custo_upgrade(d.tier) {
            let ouro_ok = ctx.ouro >= custo;
            estilo::texto_forte(
                r.x + 10.0 * f,
                tier_y + 20.0 * f,
                &format!("Tier upgrade: T{} › T{}", d.tier, d.tier + 1),
                14,
                LATAO,
            );
            estilo::texto(
                r.x + r.w * 0.55,
                tier_y + 20.0 * f,
                &format!("{custo} gold"),
                12,
                if ouro_ok { estilo::VERDE } else { estilo::VERMELHO },
            );
            // The materials, with what you have (no second ship needed).
            let mut ok = ouro_ok;
            let iy = tier_y + 28.0 * f;
            let ic = 22.0 * f;
            let cw = (r.w - 20.0 * f) / materiais.len().max(1) as f32;
            for (k, (item, q)) in materiais.iter().enumerate() {
                let tem = (ctx.conta)(*item);
                let tem_ok = tem >= *q as u64;
                ok &= tem_ok;
                let ix = r.x + 10.0 * f + k as f32 * cw;
                let rr = Rect::new(ix, iy, ic, ic);
                crate::icones::icone(*item, rr, None, None);
                if !tem_ok && crate::onde_obter::lupa_no_icone(rr) {
                    escolha = Some(Escolha::OndeObter(*item));
                }
                estilo::texto(
                    ix + ic + 4.0 * f,
                    iy + 16.0 * f,
                    &format!("{}/{}", curto(tem.min(*q as u64)), curto(*q as u64)),
                    11,
                    if tem_ok { estilo::VERDE } else { estilo::VERMELHO },
                );
            }
            let fundo = (r.y + r.h).max(tier_y + 92.0 * f);
            let b = Rect::new(r.x + 10.0 * f, fundo - 40.0 * f, r.w - 20.0 * f, 36.0 * f);
            estilo::botao(b, "Upgrade tier", estilo::estado_de(b, !ok, false), ok);
            if clicou && ok && b.toca(m) {
                escolha = Some(Escolha::UpgradeTier);
            }
        } else {
            estilo::texto_forte(
                r.x + 10.0 * f,
                tier_y + 30.0 * f,
                "T4 · maximum tier",
                15,
                LATAO,
            );
        }
        escolha
    }

    fn aba_galeria(
        &mut self,
        r: Rect,
        f: f32,
        m: Vec2,
        clicou: bool,
        ctx: &Contexto,
    ) -> Option<Escolha> {
        self.previa = Some(Previa::Tipo(self.tipo_selecionado));
        let rodape = 108.0 * f;
        let gap = 5.0 * f;
        let cw = (r.w - gap * 2.0) / 3.0;
        let ch = ((r.h - rodape - gap * 2.0) / 3.0).max(72.0 * f);
        for id in 0..9u8 {
            let tipo = nv::TipoBarco::de_id(id).unwrap();
            let x = r.x + (id % 3) as f32 * (cw + gap);
            let y = r.y + (id / 3) as f32 * (ch + gap);
            let card = Rect::new(x, y, cw, ch);
            estilo::cartao(card, card.toca(m), id == self.tipo_selecionado);
            // What the T1 hull you would craft really has (base × tier 1).
            let base = nv::stats_finais_novo(tipo, 1, [0; 4]);
            let (papel, cor_papel, _) = papel_da_variante(tipo.variante);
            let nome = match (tipo.tamanho, tipo.variante) {
                (nv::Tamanho::Pequeno, nv::Variante::Veloz) => "Swift Sloop",
                (nv::Tamanho::Pequeno, nv::Variante::Ataque) => "Corsair Sloop",
                (nv::Tamanho::Pequeno, nv::Variante::Tanque) => "Iron Sloop",
                (nv::Tamanho::Medio, nv::Variante::Veloz) => "Swift Brig",
                (nv::Tamanho::Medio, nv::Variante::Ataque) => "Corsair Brig",
                (nv::Tamanho::Medio, nv::Variante::Tanque) => "Iron Brig",
                (nv::Tamanho::Grande, nv::Variante::Veloz) => "Swift Galleon",
                (nv::Tamanho::Grande, nv::Variante::Ataque) => "Corsair Galleon",
                (nv::Tamanho::Grande, nv::Variante::Tanque) => "Iron Galleon",
            };
            estilo::texto_forte(x + 6.0 * f, y + 17.0 * f, nome, 11, LATAO);
            estilo::texto_forte(x + 6.0 * f, y + 35.0 * f, papel, 10, cor_papel);
            estilo::texto(
                x + 6.0 * f + estilo::medir_forte(&shared::idioma::tr(papel), 10) + 6.0 * f,
                y + 35.0 * f,
                &format!("{} crew · T1", tipo.capacidade()),
                10,
                estilo::SUAVE,
            );
            estilo::texto(
                x + 6.0 * f,
                y + 52.0 * f,
                &format!("Hull {:.0} · {:.1} u/s", base.hp, base.velocidade),
                10,
                estilo::TEXTO,
            );
            estilo::texto(
                x + 6.0 * f,
                y + 68.0 * f,
                &format!(
                    "Cannon ×{:.2} · Turn {:.0}°/s",
                    base.dano,
                    base.virada.to_degrees()
                ),
                10,
                estilo::TEXTO,
            );
            if clicou && card.toca(m) {
                self.tipo_selecionado = id;
            }
        }
        let tipo = nv::TipoBarco::de_id(self.tipo_selecionado).unwrap();
        let y = r.y + r.h - rodape + 6.0 * f;
        let receita = nv::receita_tipo(tipo);
        let mut ok = true;
        let mut escolha = None;
        for (k, (item, q)) in receita.iter().enumerate().filter(|(_, (_, q))| *q > 0) {
            let tem = (ctx.conta)(*item);
            ok &= tem >= *q as u64;
            let x = r.x + k as f32 * (r.w / 4.0);
            let ic = Rect::new(x, y, 26.0 * f, 26.0 * f);
            crate::icones::icone(*item, ic, None, None);
            estilo::texto(
                x + 30.0 * f,
                y + 19.0 * f,
                &format!("{}/{}", tem.min(*q as u64), q),
                11,
                if tem >= *q as u64 {
                    estilo::VERDE
                } else {
                    estilo::VERMELHO
                },
            );
            if tem < *q as u64 && crate::onde_obter::lupa_no_icone(ic) {
                escolha = Some(Escolha::OndeObter(*item));
            }
        }
        // What sets this one apart from its two siblings of the same size.
        let (_, cor_papel, resumo) = papel_da_variante(tipo.variante);
        let resumo_tr = shared::idioma::tr(resumo);
        let tam = [12u16, 11, 10]
            .into_iter()
            .find(|&t| estilo::medir(&resumo_tr, t) <= r.w)
            .unwrap_or(9);
        estilo::texto(r.x, y + 44.0 * f, resumo, tam, cor_papel);
        let b = Rect::new(r.x, r.y + r.h - 44.0 * f, r.w, 42.0 * f);
        estilo::botao(b, "Craft ship", estilo::estado_de(b, !ok, false), ok);
        if clicou && ok && b.toca(m) {
            escolha = Some(Escolha::ConstruirTipo(tipo.id()));
        }
        escolha
    }

    #[allow(clippy::too_many_arguments)]
    fn aba_melhorar(
        &mut self,
        r: Rect,
        f: f32,
        m: Vec2,
        clicou: bool,
        id: u16,
        d: &shared::items::NavioData,
        ctx: &Contexto,
    ) -> Option<Escolha> {
        let mut escolha = None;
        let lado_a_lado = r.w > 620.0 * f;
        let n = Eixo::TODOS.len() as f32;
        let gap = 8.0 * f;
        for (k, eixo) in Eixo::TODOS.iter().enumerate() {
            let card = if lado_a_lado {
                let cw = (r.w - gap * (n - 1.0)) / n;
                Rect::new(r.x + k as f32 * (cw + gap), r.y, cw, r.h)
            } else {
                let ch = (r.h - gap * (n - 1.0)) / n;
                Rect::new(r.x, r.y + k as f32 * (ch + gap), r.w, ch)
            };
            if let Some(e) = self.cartao_de_eixo(card, f, m, clicou, *eixo, id, d, ctx, lado_a_lado)
            {
                escolha = Some(e);
            }
        }
        escolha
    }

    #[allow(clippy::too_many_arguments)]
    fn cartao_de_eixo(
        &mut self,
        c: Rect,
        f: f32,
        m: Vec2,
        clicou: bool,
        eixo: Eixo,
        id: u16,
        d: &shared::items::NavioData,
        ctx: &Contexto,
        alto: bool,
    ) -> Option<Escolha> {
        estilo::cartao(c, c.toca(m), false);
        let n = nv::estrelas(d, eixo);
        let x = c.x + 10.0 * f;
        estilo::texto_forte(x, c.y + 22.0 * f, eixo.nome(), 16, LATAO);
        let estrela_r = 6.5 * f;
        crate::bolsa::desenha_estrelas(
            c.x + c.w - estrela_r * 2.3 * nv::ESTRELAS_MAX as f32 - 10.0 * f,
            c.y + 16.0 * f,
            estrela_r,
            n,
            nv::ESTRELAS_MAX,
        );
        let mut prox = *d;
        prox.estrelas[eixo as usize] = (n + 1).min(nv::ESTRELAS_MAX);
        let ganho = match eixo {
            Eixo::Casco => format!(
                "Hull {} › {}",
                nv::casco_total(id, d),
                nv::casco_total(id, &prox)
            ),
            Eixo::Vela => format!(
                "Speed {:.1} › {:.1}",
                nv::velocidade_do_navio(id, d),
                nv::velocidade_do_navio(id, &prox)
            ),
            Eixo::Canhao => format!(
                "Cannon ×{:.2} › ×{:.2}",
                nv::mult_do_canhao(id, d),
                nv::mult_do_canhao(id, &prox)
            ),
        };
        let Some(custo) = nv::custo_da_estrela(n) else {
            estilo::texto(x, c.y + 46.0 * f, "Five stars: the top.", 14, LATAO);
            return None;
        };
        estilo::texto(x, c.y + 44.0 * f, &ganho, 13, estilo::VERDE);
        let itens = [
            (item_id::COPPER, custo.cobre),
            (item_id::DARKSTEEL, custo.darksteel),
            (item_id::PRANCHA_DE_NAUFRAGIO, custo.pranchas),
        ];
        let ic = 30.0 * f;
        let mut tudo = true;
        let mut escolha = None;
        // Tall cards: one line per material; short rows: side by side.
        let botao_h = 38.0 * f;
        for (k, (item, q)) in itens.iter().enumerate() {
            let tem = (ctx.conta)(*item);
            let ok = tem >= *q as u64;
            tudo &= ok;
            let (ix, iy) = if alto {
                (x, c.y + 54.0 * f + k as f32 * (ic + 6.0 * f))
            } else {
                (x + k as f32 * ((c.w - 140.0 * f) / 3.0), c.y + 52.0 * f)
            };
            let rr = Rect::new(ix, iy, ic, ic);
            crate::icones::icone(*item, rr, None, None);
            if !ok && crate::onde_obter::lupa_no_icone(rr) {
                escolha = Some(Escolha::OndeObter(*item));
            }
            let txt = format!("{}/{}", curto(tem.min(*q as u64)), curto(*q as u64));
            estilo::texto(
                ix + ic + 4.0 * f,
                iy + ic * 0.68,
                &txt,
                12,
                if ok { estilo::VERDE } else { estilo::VERMELHO },
            );
        }
        let b = if alto {
            Rect::new(x, c.y + c.h - botao_h - 10.0 * f, c.w - 20.0 * f, botao_h)
        } else {
            Rect::new(
                c.x + c.w - 130.0 * f,
                c.y + c.h - botao_h - 8.0 * f,
                120.0 * f,
                botao_h,
            )
        };
        estilo::botao(b, "Upgrade", estilo::estado_de(b, !tudo, false), tudo);
        if tudo && clicou && b.toca(m) {
            self.anima = Some((Anima::Estrela, get_time()));
            crate::sons::tocar(crate::sons::Som::Progresso);
            escolha = Some(Escolha::Melhorar(eixo as u8));
        }
        escolha
    }

    #[allow(clippy::too_many_arguments)]
    fn aba_construir(
        &mut self,
        r: Rect,
        f: f32,
        m: Vec2,
        clicou: bool,
        id: u16,
        d: &shared::items::NavioData,
        ctx: &Contexto,
    ) -> Option<Escolha> {
        let Some(rec) = nv::receita_depois_de(id) else {
            estilo::texto_forte(r.x, r.y + 24.0 * f, "The finest hull there is.", 16, LATAO);
            estilo::texto(
                r.x,
                r.y + 48.0 * f,
                "Your Man-o'-War tops the Shipwright's line.",
                13,
                estilo::SUAVE,
            );
            return None;
        };
        let classe = nv::casco(rec.para).expect("every recipe builds a hull");
        let novo = nv::reconstruido(rec.para, d);
        if r.toca(m) {
            self.previa = Some(Previa::Casco(rec.para));
        }
        estilo::texto_forte(
            r.x,
            r.y + 20.0 * f,
            &format!("Next hull: {}", (ctx.nome)(rec.para)),
            17,
            LATAO,
        );
        let nivel_ok = ctx.nivel >= classe.nivel;
        estilo::texto(
            r.x,
            r.y + 40.0 * f,
            &format!(
                "Needs level {}",
                shared::progressao::texto_de_nivel(classe.nivel)
            ),
            13,
            if nivel_ok {
                estilo::SUAVE
            } else {
                estilo::VERMELHO
            },
        );
        // The numbers compared.
        let linhas = [
            (
                "Hull",
                format!("{}", nv::casco_total(id, d)),
                format!("{}", nv::casco_total(rec.para, &novo)),
            ),
            (
                "Speed",
                format!("{:.1}", nv::velocidade_do_navio(id, d)),
                format!("{:.1}", nv::velocidade_do_navio(rec.para, &novo)),
            ),
            (
                "Turning",
                nv::rotulo_do_giro(id, d).to_string(),
                nv::rotulo_do_giro(rec.para, &novo).to_string(),
            ),
            (
                "Cannon",
                format!("×{:.2}", nv::mult_do_canhao(id, d)),
                format!("×{:.2}", nv::mult_do_canhao(rec.para, &novo)),
            ),
        ];
        let lh = 19.0 * f;
        let mut y = r.y + 64.0 * f;
        for (nome, a, b) in &linhas {
            estilo::texto(r.x, y, nome, 13, estilo::SUAVE);
            estilo::texto(r.x + r.w * 0.32, y, a, 13, estilo::TEXTO);
            estilo::texto(r.x + r.w * 0.55, y, "›", 13, LATAO);
            estilo::texto(r.x + r.w * 0.64, y, b, 13, estilo::VERDE);
            y += lh;
        }
        y += 4.0 * f;
        estilo::texto(r.x, y, "Recipe", 14, LATAO);
        y += 8.0 * f;
        let ic = 32.0 * f;
        let mut tudo = nivel_ok;
        let mut escolha = None;
        let colunas = 2;
        let cw = r.w / colunas as f32;
        let itens: Vec<(u16, u32)> = std::iter::once((id, 1))
            .chain(rec.materiais.iter().copied())
            .collect();
        for (k, (item, q)) in itens.iter().enumerate() {
            // The old hull is the one in the Ship slot: always there.
            let tem = if k == 0 { 1 } else { (ctx.conta)(*item) };
            let ok = tem >= *q as u64;
            tudo &= ok;
            let ix = r.x + (k % colunas) as f32 * cw;
            let iy = y + (k / colunas) as f32 * (ic + 6.0 * f);
            let rr = Rect::new(ix, iy, ic, ic);
            crate::icones::icone(*item, rr, None, None);
            if !ok && crate::onde_obter::lupa_no_icone(rr) {
                escolha = Some(Escolha::OndeObter(*item));
            }
            let nome = if k == 0 {
                "This hull (consumed)".to_string()
            } else {
                (ctx.nome)(*item)
            };
            estilo::texto(ix + ic + 6.0 * f, iy + 13.0 * f, &nome, 12, estilo::TEXTO);
            estilo::texto(
                ix + ic + 6.0 * f,
                iy + 28.0 * f,
                &format!("{}/{}", curto(tem.min(*q as u64)), curto(*q as u64)),
                12,
                if ok { estilo::VERDE } else { estilo::VERMELHO },
            );
        }
        let b = Rect::new(r.x, r.y + r.h - 48.0 * f, r.w, 46.0 * f);
        estilo::texto(
            r.x,
            b.y - 8.0 * f,
            "Stars and cosmetics carry over to the new hull.",
            12,
            estilo::SUAVE,
        );
        estilo::botao(b, "Build", estilo::estado_de(b, !tudo, false), tudo);
        if tudo && clicou && b.toca(m) {
            self.anima = Some((Anima::Obra, get_time()));
            escolha = Some(Escolha::Construir);
        }
        escolha
    }

    fn aba_cosmeticos(
        &mut self,
        r: Rect,
        f: f32,
        m: Vec2,
        clicou: bool,
        d: &shared::items::NavioData,
        ctx: &Contexto,
    ) -> Option<Escolha> {
        let mut escolha = None;
        // What the ship wears now.
        let vela = d
            .vela
            .checked_sub(1)
            .and_then(|i| nv::VELAS.get(i as usize))
            .map_or("Plain canvas", |v| v.nome);
        let figura = d
            .figura
            .checked_sub(1)
            .and_then(|i| nv::FIGURAS.get(i as usize))
            .map_or("None", |f| f.1);
        estilo::texto(
            r.x,
            r.y + 16.0 * f,
            &format!("Sails: {vela}"),
            13,
            estilo::TEXTO,
        );
        estilo::texto(
            r.x + r.w * 0.5,
            r.y + 16.0 * f,
            &format!("Figurehead: {figura}"),
            13,
            estilo::TEXTO,
        );
        // What the bag holds, one card each.
        let donos: Vec<(u16, u16, nv::Cosmetico)> = ctx
            .slots
            .iter()
            .enumerate()
            .filter(|(_, s)| s.qty > 0)
            .filter_map(|(i, s)| nv::cosmetico(s.item_id).map(|c| (i as u16, s.item_id, c)))
            .collect();
        let topo = r.y + 30.0 * f;
        let loja = Rect::new(r.x, r.y + r.h - 44.0 * f, r.w, 42.0 * f);
        if donos.is_empty() {
            estilo::texto(
                r.x,
                topo + 24.0 * f,
                "No sails or figureheads in your bag.",
                14,
                estilo::SUAVE,
            );
            estilo::texto(
                r.x,
                topo + 44.0 * f,
                "Find them in the Shop, Skins › Ship.",
                13,
                estilo::SUAVE,
            );
        }
        let cw = ((r.w - 8.0 * f) / 2.0).max(120.0 * f);
        let ch = 58.0 * f;
        let colunas = ((r.w + 8.0 * f) / (cw + 8.0 * f)).floor().max(1.0) as usize;
        for (k, (slot, item, c)) in donos.iter().enumerate() {
            let card = Rect::new(
                r.x + (k % colunas) as f32 * (cw + 8.0 * f),
                topo + (k / colunas) as f32 * (ch + 6.0 * f),
                cw,
                ch,
            );
            if card.y + card.h > loja.y - 4.0 {
                break;
            }
            let sobre = card.toca(m);
            estilo::cartao(card, sobre, false);
            if sobre {
                self.previa = Some(Previa::Vestido(*c));
            }
            let ic = Rect::new(
                card.x + 8.0 * f,
                card.y + 8.0 * f,
                ch - 16.0 * f,
                ch - 16.0 * f,
            );
            match c {
                nv::Cosmetico::Vela(v) => {
                    let cor = nv::cor_da_vela(*v);
                    estilo::ret_arredondado(
                        ic,
                        6.0 * f,
                        Color::from_rgba(cor[0], cor[1], cor[2], 255),
                    );
                    draw_triangle(
                        vec2(ic.x + ic.w * 0.5, ic.y + 4.0 * f),
                        vec2(ic.x + ic.w * 0.5, ic.y + ic.h - 4.0 * f),
                        vec2(ic.x + ic.w - 4.0 * f, ic.y + ic.h - 6.0 * f),
                        Color::new(1.0, 1.0, 1.0, 0.25),
                    );
                }
                nv::Cosmetico::Figura(_) => crate::icones::icone(*item, ic, None, None),
            }
            let nome = nv::nome_do_cosmetico(*item).unwrap_or("?");
            estilo::texto(
                ic.x + ic.w + 8.0 * f,
                card.y + 22.0 * f,
                nome,
                13,
                estilo::TEXTO,
            );
            let b = Rect::new(
                card.x + card.w - 84.0 * f,
                card.y + card.h - 30.0 * f,
                76.0 * f,
                24.0 * f,
            );
            estilo::botao(b, "Apply", estilo::estado_de(b, false, false), sobre);
            if clicou && b.toca(m) {
                self.anima = Some((Anima::Festa("New look!"), get_time()));
                escolha = Some(Escolha::Vestir(*slot));
            }
        }
        estilo::botao(
            loja,
            "More in the Shop · Ship",
            estilo::estado_de(loja, false, false),
            false,
        );
        if clicou && loja.toca(m) {
            escolha = Some(Escolha::LojaDoNavio);
        }
        escolha
    }
}

/// The clock of the animations (no window in the unit tests).
fn relogio() -> f64 {
    #[cfg(test)]
    return 0.0;
    #[cfg(not(test))]
    get_time()
}

/// Why "Set Sail" is off, if it is.
pub fn motivo_sem_zarpar(e: &EstadoNavio) -> Option<&'static str> {
    if !e.tem {
        Some("No ship in the Ship slot")
    } else if e.avariado {
        Some("The hull is damaged: repair it to sail")
    } else if !e.zarpar {
        Some("This island has no dock on the open sea")
    } else {
        None
    }
}

fn classe_e_estrelas(id: u16, d: &shared::items::NavioData) -> String {
    let nome = nv::nome_do_navio(id, d);
    if nv::tipo_novo(d).is_some() {
        let usados: u16 = d.pontos.iter().map(|p| *p as u16).sum();
        format!(
            "{nome} · T{} · {usados}/{} build points",
            d.tier,
            nv::pontos_no_tier(d.tier).unwrap_or(0)
        )
    } else {
        format!("{nome} · legacy hull")
    }
}

/// The role of each variant in the gallery: a short tag, its colour, and
/// one line that says what it trades against its two siblings of the same
/// size (true for all three sizes, see `nv::stats_base`).
fn papel_da_variante(v: nv::Variante) -> (&'static str, Color, &'static str) {
    match v {
        nv::Variante::Veloz => (
            "Speed",
            estilo::AZUL,
            "Swift: fastest and nimblest of its size, but the thinnest hull and lightest guns.",
        ),
        nv::Variante::Ataque => (
            "Firepower",
            estilo::VERMELHO,
            "Corsair: the heaviest guns of its size, with middle speed and hull.",
        ),
        nv::Variante::Tanque => (
            "Armor",
            estilo::VERDE,
            "Iron: the toughest hull of its size, but the slowest and heaviest to turn.",
        ),
    }
}

fn curto(n: u64) -> String {
    if n >= 10_000 {
        format!("{}k", n / 1000)
    } else {
        n.to_string()
    }
}

/// Overview: the numbers and a star row per axis.
fn aba_visao(r: Rect, f: f32, id: u16, d: &shared::items::NavioData, e: &EstadoNavio) {
    let max = nv::casco_total(id, d).max(1);
    let frac = d.casco as f32 / max as f32;
    let classe = nv::nome_do_navio(id, d);
    estilo::texto_forte(r.x, r.y + 22.0 * f, classe, 19, LATAO);
    estilo::texto(r.x, r.y + 44.0 * f, "Hull", 13, estilo::SUAVE);
    let barra_r = Rect::new(r.x + 70.0 * f, r.y + 32.0 * f, r.w - 70.0 * f, 16.0 * f);
    barra(
        barra_r,
        frac,
        if e.avariado || frac < 0.35 {
            estilo::VERMELHO
        } else if frac < 0.7 {
            LATAO
        } else {
            estilo::VERDE
        },
    );
    estilo::texto_centro(
        barra_r.center().x,
        barra_r.y + 13.0 * f,
        &format!("{} / {}", d.casco, max),
        12,
        estilo::TEXTO,
    );
    let linhas = [
        ("Speed", format!("{:.1}", nv::velocidade_do_navio(id, d))),
        ("Turning", nv::rotulo_do_giro(id, d).to_string()),
        ("Cannon power", format!("×{:.2}", nv::mult_do_canhao(id, d))),
    ];
    let mut y = r.y + 76.0 * f;
    for (nome, v) in &linhas {
        estilo::texto(r.x, y, nome, 14, estilo::SUAVE);
        estilo::texto_forte(r.x + r.w * 0.45, y, v, 14, estilo::TEXTO);
        y += 24.0 * f;
    }
    y += 6.0 * f;
    if let Some(tipo) = nv::tipo_novo(d) {
        estilo::texto(
            r.x,
            y,
            &format!("Crew capacity: {}", tipo.capacidade()),
            14,
            estilo::SUAVE,
        );
        y += 26.0 * f;
        estilo::texto(
            r.x,
            y,
            &format!(
                "Build [speed {}, hull {}, cannon {}, turning {}]",
                d.pontos[0], d.pontos[1], d.pontos[2], d.pontos[3]
            ),
            12,
            LATAO,
        );
        y += 26.0 * f;
    }
    if e.avariado {
        estilo::texto(
            r.x,
            y + 8.0 * f,
            "Damaged: repair the hull before sailing.",
            13,
            estilo::VERMELHO,
        );
    }
}

/// A bar with a brass rim.
fn barra(r: Rect, frac: f32, cor: Color) {
    estilo::ret_arredondado(r, r.h * 0.5, Color::new(0.05, 0.04, 0.03, 0.9));
    let cheio = Rect::new(
        r.x + 2.0,
        r.y + 2.0,
        ((r.w - 4.0) * frac.clamp(0.0, 1.0)).max(0.0),
        r.h - 4.0,
    );
    if cheio.w > 1.0 {
        estilo::ret_arredondado(cheio, cheio.h * 0.5, cor);
    }
    draw_rectangle_lines(r.x, r.y, r.w, r.h, 1.5, LATAO_ESCURO);
}

/// The panel's frame: warm planks in a brass rim with rivets.
fn moldura(p: Rect, f: f32) {
    estilo::painel(p);
    estilo::ret_arredondado(p, 12.0 * f, MADEIRA);
    // Planks: horizontal grain lines.
    let mut y = p.y + 18.0 * f;
    let mut k = 0;
    while y < p.y + p.h - 6.0 {
        let cor = if k % 2 == 0 {
            Color::new(0.24, 0.15, 0.09, 0.5)
        } else {
            Color::new(0.36, 0.23, 0.13, 0.35)
        };
        draw_line(p.x + 6.0, y, p.x + p.w - 6.0, y, 1.0, cor);
        y += 26.0 * f;
        k += 1;
    }
    draw_rectangle_lines(p.x, p.y, p.w, p.h, 3.0 * f, LATAO_ESCURO);
    draw_rectangle_lines(
        p.x + 4.0 * f,
        p.y + 4.0 * f,
        p.w - 8.0 * f,
        p.h - 8.0 * f,
        1.5 * f,
        LATAO,
    );
    for (x, y) in [
        (p.x, p.y),
        (p.x + p.w, p.y),
        (p.x, p.y + p.h),
        (p.x + p.w, p.y + p.h),
    ] {
        let q = vec2(x, y) + (p.center() - vec2(x, y)).normalize_or_zero() * 11.0 * f;
        draw_circle(q.x, q.y, 4.5 * f, LATAO);
        draw_circle(
            q.x - 1.0,
            q.y - 1.0,
            1.8 * f,
            Color::new(1.0, 0.95, 0.75, 0.8),
        );
    }
    // A thin navy band under the header.
    draw_rectangle(
        p.x + 8.0 * f,
        p.y + 56.0 * f,
        p.w - 16.0 * f,
        2.0 * f,
        MADEIRA_CLARA,
    );
}

/// The stage's backdrop: a navy harbour at dusk, a soft studio light behind
/// the ship, and the dock walls.
fn fundo_do_palco(r: Rect, f: f32, t: f32) {
    estilo::ret_arredondado(r, 10.0 * f, MARINHO);
    // Sky to sea.
    let faixas = 18;
    for k in 0..faixas {
        let a = k as f32 / faixas as f32;
        let c = Color::new(
            MARINHO.r + (MARINHO_CLARO.r - MARINHO.r) * a,
            MARINHO.g + (MARINHO_CLARO.g - MARINHO.g) * a,
            MARINHO.b + (MARINHO_CLARO.b - MARINHO.b) * a,
            1.0,
        );
        draw_rectangle(
            r.x + 3.0,
            r.y + 3.0 + r.h * 0.62 * a,
            r.w - 6.0,
            r.h * 0.62 / faixas as f32 + 1.0,
            c,
        );
    }
    // Water and dock removed: clean stage for the 3D ship only.
    // The studio light.
    let c = vec2(r.center().x, r.y + r.h * 0.45);
    for k in 0..8 {
        let rr = (r.w.min(r.h) * 0.5) * (1.0 - k as f32 / 9.0);
        draw_circle(
            c.x,
            c.y,
            rr,
            Color::new(1.0, 0.86, 0.6, 0.02 + k as f32 * 0.004),
        );
    }
    // Hanging lamps on the dock beams.
    for k in 0..3 {
        let x = r.x + r.w * (0.2 + 0.3 * k as f32);
        draw_line(
            x,
            r.y + 3.0,
            x,
            r.y + 22.0 * f,
            1.0,
            Color::new(0.5, 0.4, 0.3, 0.8),
        );
        draw_circle(x, r.y + 26.0 * f, 4.0 * f, Color::new(1.0, 0.82, 0.48, 0.9));
        draw_circle(
            x,
            r.y + 26.0 * f,
            12.0 * f,
            Color::new(1.0, 0.82, 0.48, 0.08),
        );
    }
    draw_rectangle_lines(r.x, r.y, r.w, r.h, 2.0 * f, LATAO_ESCURO);
}

/// THE SHIP on the stage: the real model through a viewport into the main
/// pass. The camera orbits (the ship never turns: turning it would heel it,
/// `mar_casco::movimento`). Keel blocks, dock planks and the water line.
fn desenha_navio(r: Rect, v: Visual, orbita: f32, zoom: f32, fantasma: bool, solido: &Material) {
    if r.w < 8.0 || r.h < 8.0 {
        return;
    }
    let Some(vp) = crate::render3d::viewport_na_tela(r) else {
        return;
    };
    // Far off any island (the swell is the open sea's, gentle).
    let p = vec3(9_000.0, 0.0, 9_000.0);
    let yaw = 0.0;
    let semente = 0x5_41F0_u64 + v.classe as u64;
    let n = crate::mar_casco::modelo(v);
    let topo = crate::mar_casco::topo_do_mastro(p, yaw, semente, v);
    let altura = (topo.y - crate::agua::ALTURA_DA_AGUA).max(2.0);
    let comprido = n.meio_comp.max(1.0);
    let aspecto = vp.2 as f32 / vp.3.max(1) as f32;
    let tamanho = (altura * 0.62).max(comprido * 1.05 / aspecto.max(0.5));
    let dist = tamanho * 3.7 / zoom.max(0.2);
    let alvo = vec3(p.x, crate::agua::ALTURA_DA_AGUA + altura * 0.42, p.z);
    let elev: f32 = 0.30;
    let cam = Camera3D {
        position: alvo
            + vec3(
                orbita.sin() * elev.cos(),
                elev.sin(),
                orbita.cos() * elev.cos(),
            ) * dist,
        target: alvo,
        up: Vec3::Y,
        fovy: 32f32.to_radians(),
        aspect: Some(aspecto),
        viewport: Some(vp),
        render_target: crate::render3d::alvo(),
        ..Default::default()
    };
    set_camera(&cam);
    crate::render3d::limpa_so_profundidade();
    macroquad::material::gl_use_material(solido);
    // Dock floor, keel blocks and piers removed: clean stage for the ship only.
    let antes = crate::gpu_estatica::some_atual();
    // No fog on the stage: the world's is centred on the player, far away.
    let neblina = crate::gpu_estatica::neblina();
    crate::gpu_estatica::define_neblina(vec2(p.x, p.z), 0.0, 0.0);
    if fantasma {
        // A ghost: dithered half away, shimmering.
        crate::gpu_estatica::define_some(0.35 + 0.1 * (get_time() as f32 * 3.0).sin());
    }
    crate::mar_casco::sem_surgir(|| crate::mar_casco::desenha(p, yaw, semente, v));
    crate::gpu_estatica::define_some(antes);
    crate::gpu_estatica::define_neblina(vec2(neblina[0], neblina[1]), neblina[2], neblina[3]);
    macroquad::material::gl_use_default_material();
    crate::render3d::camera_padrao();
}

/// A hammer (2D), its head at `base` rotated `ang`.
fn martelo(base: Vec2, ang: f32, tam: f32) {
    let dir = vec2(ang.sin(), -ang.cos());
    let cabo = base + dir * tam;
    estilo::traco(base, cabo, tam * 0.12, Color::new(0.55, 0.36, 0.18, 1.0));
    let lado = vec2(dir.y, -dir.x);
    estilo::traco(
        cabo - lado * tam * 0.28,
        cabo + lado * tam * 0.28,
        tam * 0.24,
        Color::new(0.62, 0.64, 0.68, 1.0),
    );
}

/// The hull from the side, with cracks where it is damaged and patches
/// once it is sound. `frac` = hull left.
fn silhueta_avariada(r: Rect, f: f32, frac: f32, semente: u32) {
    let c = r.center();
    let w = r.w * 0.86;
    let h = r.h * 0.5;
    let topo = c.y - h * 0.15;
    let quilha = topo + h;
    let proa = vec2(c.x + w * 0.5, topo);
    let popa = vec2(c.x - w * 0.5, topo - h * 0.1);
    let fundo_proa = vec2(c.x + w * 0.3, quilha);
    let fundo_popa = vec2(c.x - w * 0.38, quilha - h * 0.1);
    let casco = Color::new(0.42, 0.27, 0.15, 1.0);
    draw_triangle(popa, proa, fundo_proa, casco);
    draw_triangle(popa, fundo_proa, fundo_popa, casco);
    // Planks.
    for k in 1..5 {
        let t = k as f32 / 5.0;
        let a = popa.lerp(fundo_popa, t);
        let b = proa.lerp(fundo_proa, t);
        draw_line(a.x, a.y, b.x, b.y, 1.0, Color::new(0.25, 0.15, 0.08, 0.8));
    }
    draw_line(popa.x, popa.y, proa.x, proa.y, 3.0 * f, LATAO);
    // The water line.
    draw_line(
        r.x,
        quilha - h * 0.25,
        r.x + r.w,
        quilha - h * 0.25,
        1.5,
        Color::new(0.4, 0.7, 0.9, 0.6),
    );
    // Cracks: more the lower the hull.
    let rachas = ((1.0 - frac.clamp(0.0, 1.0)) * 12.0).round() as u32;
    let mut s = semente.wrapping_mul(2_654_435_761).max(1);
    let mut rnd = || {
        s ^= s << 13;
        s ^= s >> 17;
        s ^= s << 5;
        (s % 1000) as f32 / 1000.0
    };
    for _ in 0..rachas {
        let t = rnd();
        let u = rnd();
        let a = popa.lerp(proa, t);
        let b = fundo_popa.lerp(fundo_proa, t);
        let q = a.lerp(b, 0.15 + u * 0.7);
        let mut p0 = q;
        for _ in 0..3 {
            let p1 = p0 + vec2((rnd() - 0.5) * 16.0 * f, (rnd() - 0.2) * 10.0 * f);
            draw_line(
                p0.x,
                p0.y,
                p1.x,
                p1.y,
                1.6 * f,
                Color::new(0.08, 0.05, 0.03, 1.0),
            );
            p0 = p1;
        }
        draw_circle(q.x, q.y, 2.2 * f, Color::new(0.95, 0.36, 0.36, 0.7));
    }
    if rachas == 0 {
        estilo::texto_centro(
            c.x,
            quilha + 16.0 * f,
            "Sound and watertight",
            12,
            estilo::VERDE,
        );
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    fn estado(tem: bool, avariado: bool, zarpar: bool) -> EstadoNavio {
        EstadoNavio {
            tem,
            avariado,
            zarpar,
            item_id: item_id::NAVIO_CHALUPA,
            casco: 10,
            casco_max: 2400,
            reparo: if avariado { 500 } else { 0 },
        }
    }

    #[test]
    fn zarpar_diz_por_que_nao() {
        assert_eq!(motivo_sem_zarpar(&estado(true, false, true)), None);
        assert!(motivo_sem_zarpar(&estado(true, true, true))
            .unwrap()
            .contains("damaged"));
        assert!(motivo_sem_zarpar(&estado(false, false, true)).is_some());
        assert!(motivo_sem_zarpar(&estado(true, false, false))
            .unwrap()
            .contains("dock"));
    }

    /// Opening on a damaged ship goes straight to Repair; a repair that lands
    /// plays the flourish.
    #[test]
    fn avariado_abre_no_reparo_e_o_reparo_comemora() {
        let mut ui = EstaleiroUi::default();
        ui.abrir(estado(true, true, true));
        assert_eq!(ui.aba, Aba::Reparo);
        ui.define_navio(EstadoNavio {
            casco: 2400,
            avariado: false,
            reparo: 0,
            ..estado(true, false, true)
        });
        assert!(matches!(ui.anima, Some((Anima::Festa(_), _))));
        ui.fechar();
        ui.abrir(estado(true, false, true));
        assert_eq!(ui.aba, Aba::Reparo, "hull below full still opens on Repair");
        ui.fechar();
        ui.abrir(EstadoNavio {
            casco: 2400,
            ..estado(true, false, true)
        });
        assert_eq!(ui.aba, Aba::Visao);
    }
}
