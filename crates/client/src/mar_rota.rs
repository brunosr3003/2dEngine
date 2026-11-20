//! AUTO PATH AT SEA (docs/NAVEGACAO.md, "gameplay remake", 16/11/2026).
//!
//! The owner: "o mar aberto está um lixo ... tá sem auto path ... não mostra
//! a doca". On land a tap walks there by itself; at sea it now SAILS there
//! the same way: the client sends ONE `MoverPara` with the destination, the
//! server plans the whole crossing with its water A* (`world/navio.rs`,
//! `caminho_no_mar`), steers the hull with the land's `SeguidorDeRota` and
//! sends the dashed line (`ServerMessage::Rota`) like on land.
//!
//! What lives here is the client's half, with no macroquad in the core so
//! it is tested: when to (re)send, the AUTO-DOCK when the destination was a
//! dock, a tap on an island meaning "its dock", the "Sail to" panel with
//! every port, the ETA, and where the sea's AUTO COMBAT goes hunting when no
//! mob is in sight (the nests are a pure function of the chart).
//!
//! Mobile cost: nothing per frame but a few distance tests. The path is the
//! server's; the client never runs an A*.

#[allow(unused_imports)]
use crate::ui::Toque as _;
use macroquad::prelude::*;

use crate::hud_estilo as estilo;

/// Within this of a plain destination: arrived.
pub const CHEGOU: f32 = 3.0;
/// No movement for this long while sailing: ask the server again (the
/// route may have ended short, or another ship was in the way).
const PARADO_S: f64 = 2.0;
/// Never two requests closer than this.
const INTERVALO_S: f64 = 1.2;
/// Requests in a row with no progress: give up and say so.
const DESISTE_APOS: u32 = 6;
/// A tap within this of an island's coast means "that island's dock".
const TOQUE_NA_ILHA: f32 = 40.0;
/// How often the sea's AUTO modes look for a target (mobile budget).
pub const VARREDURA_S: f64 = 0.25;
/// A nest the hunt reached and found empty is skipped this long.
const NINHO_VAZIO_S: f64 = 40.0;

fn g(v: Vec2) -> ::glam::Vec2 {
    ::glam::Vec2::new(v.x, v.y)
}

fn mq(v: ::glam::Vec2) -> Vec2 {
    vec2(v.x, v.y)
}

/// What to do this frame.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Passo {
    Nada,
    /// Send `MoverPara` to this point.
    Enviar(Vec2),
    /// Arrived at the dock that was the destination: send `Atracar`.
    Atracar(u8),
    /// Arrived (a plain destination).
    Chegou,
    /// No way there: stop and say so.
    Desistiu,
}

/// A crossing in progress.
#[derive(Debug, Default)]
pub struct RotaDoMar {
    destino: Option<Vec2>,
    /// The destination is this island's dock: dock on arrival.
    pub atracar: Option<u8>,
    enviado_em: f64,
    ultima_pos: Option<Vec2>,
    parado_desde: f64,
    melhor: f32,
    sem_progresso: u32,
    /// The "Sail to" panel is open.
    pub portos_abertos: bool,
    varredura_em: f64,
    /// Nests the hunt found empty: (nest index, until).
    ninhos_vazios: [(usize, f64); 4],
    /// Where the "Sail to" pill and its panel were last drawn (`pega_em`).
    ancora: Option<(Rect, Rect)>,
    /// AUTO COMBAT at sea is the server's pilot: the switch as last sent.
    pub piloto: PilotoCliente,
}

/// Resend the pilot's "on" this often while it is on: a new sea process
/// (a reconnect, another channel) starts with it off.
const PILOTO_REENVIO_S: f64 = 3.0;

/// The client's half of the sea's AUTO COMBAT (`PedidoNavio::Piloto`): the
/// server steers and fires; the client keeps the switch in step and
/// stops hunting while the server sails home to patch the hull.
#[derive(Debug, Default, Clone, Copy)]
pub struct PilotoCliente {
    ligado: bool,
    enviado_em: f64,
    /// Retreating (`navio::piloto_recua`, the same rule as the server's).
    pub recuando: bool,
}

impl PilotoCliente {
    /// The switch to send this frame, if any. `quer` = at sea with AUTO
    /// COMBAT on.
    pub fn pedido(&mut self, quer: bool, agora: f64) -> Option<bool> {
        if quer != self.ligado {
            self.ligado = quer;
            self.enviado_em = agora;
            if !quer {
                self.recuando = false;
            }
            return Some(quer);
        }
        if quer && agora - self.enviado_em >= PILOTO_REENVIO_S {
            self.enviado_em = agora;
            return Some(true);
        }
        None
    }

    pub fn ligado(&self) -> bool {
        self.ligado
    }

    /// The hull at `fracao` (repair ready or not): `true` the moment the
    /// retreat STARTS (the caller drops its hunting route then).
    pub fn casco(&mut self, fracao: f32, reparo_pronto: bool) -> bool {
        let antes = self.recuando;
        self.recuando = shared::navio::piloto_recua(fracao, reparo_pronto, antes);
        self.recuando && !antes
    }
}

/// The dock of island `ilha` (`ARQUIPELAGO` index) on the sea: the anchorage.
pub fn fundeadouro(ilha: u8) -> Option<Vec2> {
    shared::mar::vista_da_ilha(ilha as usize).map(|v| mq(v.fundeadouro()))
}

/// WHAT A TAP AT SEA MEANS: a tap on an island (or right by its dock) is
/// "sail to its dock and go ashore"; anywhere else, sail there.
pub fn destino_do_toque(p: Vec2) -> (Vec2, Option<u8>) {
    let q = g(p);
    if let Some(c) = shared::mar::cais_perto(q) {
        return (mq(c.fundeadouro()), Some(c.ilha as u8));
    }
    // Land, or Abyssia's closed lagoon (water no hull reaches): the island.
    let na_lagoa = shared::mar::ilhas()
        .iter()
        .any(|i| q.distance(i.centro) < shared::mar::raio_da_lagoa(i, q));
    if !shared::mar::navegavel(q.x, q.y) || na_lagoa {
        let perto = shared::mar::ilhas()
            .iter()
            .filter(|i| q.distance(i.centro) <= i.raio + TOQUE_NA_ILHA)
            .min_by(|a, b| {
                (q.distance(a.centro) - a.raio).total_cmp(&(q.distance(b.centro) - b.raio))
            });
        if let Some(i) = perto {
            return (mq(i.fundeadouro()), Some(i.ilha as u8));
        }
    }
    (p, None)
}

/// Seconds to sail `metros` at `velocidade` (u/s).
pub fn eta_s(metros: f32, velocidade: f32) -> f32 {
    if velocidade <= 0.01 {
        return 0.0;
    }
    metros.max(0.0) / velocidade
}

/// "1:05" / "42 s".
pub fn formata_eta(s: f32) -> String {
    let s = s.round().max(0.0) as u32;
    if s >= 60 {
        format!("{}:{:02}", s / 60, s % 60)
    } else {
        format!("{s} s")
    }
}

/// The ports of the sea, nearest first: (island, name, anchorage, distance).
pub fn portos(eu: Vec2) -> Vec<(u8, &'static str, Vec2, f32)> {
    let mut v: Vec<_> = shared::mar::ilhas()
        .iter()
        .map(|i| {
            let f = mq(i.fundeadouro());
            (i.ilha as u8, i.nome, f, f.distance(eu))
        })
        .collect();
    v.sort_by(|a, b| a.3.total_cmp(&b.3));
    v
}

/// The nearest port, without building a list (the HUD asks every frame).
pub fn porto_mais_perto(eu: Vec2) -> Option<(u8, &'static str, Vec2, f32)> {
    shared::mar::ilhas()
        .iter()
        .map(|i| {
            let f = mq(i.fundeadouro());
            (i.ilha as u8, i.nome, f, f.distance(eu))
        })
        .min_by(|a, b| a.3.total_cmp(&b.3))
}

/// The island a quest needs, seen from the sea: the story's travel step
/// names it, otherwise the quest's zone. `None` = any island will do.
pub fn ilha_da_missao(quest: u16) -> Option<u8> {
    let def = shared::quests::quest_by_id(quest)?;
    if def.obj_kind == shared::quests::objective_kind::VIAGEM {
        return Some(def.obj_target as u8);
    }
    let z = shared::quests::zona_da_missao(quest)?;
    shared::terreno::ARQUIPELAGO
        .iter()
        .position(|a| a.zona == z)
        .map(|i| i as u8)
}

impl RotaDoMar {
    pub fn ativa(&self) -> bool {
        self.destino.is_some()
    }

    pub fn destino(&self) -> Option<Vec2> {
        self.destino
    }

    /// The island that matters NOW (the only one with an edge marker,
    /// `mar_cena::rotulos`): the crossing's dock, else the nearest.
    pub fn ilha_em_foco(&self, eu: Vec2) -> Option<u8> {
        self.atracar
            .filter(|_| self.ativa())
            .or_else(|| porto_mais_perto(eu).map(|p| p.0))
    }

    /// Sail to `destino`; `atracar` = dock at that island on arrival. The
    /// same destination twice keeps the crossing (the auto modes ask again
    /// every second; resetting would resend and stutter).
    pub fn iniciar(&mut self, destino: Vec2, atracar: Option<u8>, agora: f64) {
        if self.destino.is_some_and(|d| d.distance(destino) < 1.0) && self.atracar == atracar {
            return;
        }
        let portos_abertos = self.portos_abertos;
        let vazios = self.ninhos_vazios;
        let ancora = self.ancora;
        let varredura_em = self.varredura_em;
        let piloto = self.piloto;
        *self = Self {
            destino: Some(destino),
            atracar,
            enviado_em: f64::MIN,
            parado_desde: agora,
            melhor: f32::MAX,
            portos_abertos,
            ninhos_vazios: vazios,
            ancora,
            varredura_em,
            piloto,
            ..Default::default()
        };
    }

    /// Sail to island `ilha`'s dock and go ashore.
    pub fn para_ilha(&mut self, ilha: u8, agora: f64) -> bool {
        let Some(f) = fundeadouro(ilha) else {
            return false;
        };
        self.iniciar(f, Some(ilha), agora);
        true
    }

    pub fn parar(&mut self) {
        self.destino = None;
        self.atracar = None;
    }

    /// One frame. `rota_viva` = the server's dashed route is still there.
    pub fn passo(&mut self, eu: Vec2, agora: f64, rota_viva: bool) -> Passo {
        let Some(destino) = self.destino else {
            return Passo::Nada;
        };
        // AUTO-DOCK: in reach of the destination's dock, go ashore.
        if let Some(ilha) = self.atracar {
            if shared::mar::cais_perto(g(eu)).is_some_and(|c| c.ilha as u8 == ilha) {
                self.parar();
                return Passo::Atracar(ilha);
            }
        } else if eu.distance(destino) <= CHEGOU {
            self.parar();
            return Passo::Chegou;
        }
        if self.ultima_pos.is_none_or(|u| u.distance(eu) > 0.5) {
            self.ultima_pos = Some(eu);
            self.parado_desde = agora;
        }
        let primeiro = self.enviado_em == f64::MIN;
        let parado = agora - self.parado_desde >= PARADO_S;
        if !primeiro && (rota_viva && !parado) {
            return Passo::Nada;
        }
        if !primeiro && agora - self.enviado_em < INTERVALO_S {
            return Passo::Nada;
        }
        if !primeiro {
            let d = eu.distance(destino);
            if d < self.melhor - 2.0 {
                self.melhor = d;
                self.sem_progresso = 0;
            } else {
                self.sem_progresso += 1;
                if self.sem_progresso >= DESISTE_APOS {
                    self.parar();
                    return Passo::Desistiu;
                }
            }
        } else {
            self.melhor = eu.distance(destino);
        }
        self.enviado_em = agora;
        self.parado_desde = agora;
        Passo::Enviar(destino)
    }

    /// The status strip: "Sailing to Glacier · 320 m · 0:32".
    pub fn faixa(
        &self,
        eu: Option<Vec2>,
        restante: Option<f32>,
        velocidade: f32,
    ) -> Option<String> {
        let (d, eu) = (self.destino?, eu?);
        let m = restante.unwrap_or_else(|| eu.distance(d));
        let eta = formata_eta(eta_s(m, velocidade));
        Some(
            match self
                .atracar
                .and_then(|i| shared::mar::vista_da_ilha(i as usize))
            {
                Some(v) => format!("Sailing to {} · {:.0} m · {}", v.nome, m, eta),
                None => format!("Sailing · {:.0} m · {}", m, eta),
            },
        )
    }

    /// The sea's AUTO modes scan at most every `VARREDURA_S` (mobile: the
    /// land's scan runs every frame).
    pub fn pode_varrer(&mut self, agora: f64) -> bool {
        if agora - self.varredura_em < VARREDURA_S {
            return false;
        }
        self.varredura_em = agora;
        true
    }

    /// AUTO COMBAT AT SEA WITH NOTHING IN SIGHT: the nearest mob nest not
    /// found empty lately. The nests are where the sea mobs live
    /// (`mar::ninhos`, the same function the server spawns from), so the
    /// hunt goes where there is something to hunt.
    pub fn proximo_ninho(&mut self, eu: Vec2, agora: f64) -> Option<Vec2> {
        let ninhos = shared::mar::ninhos();
        // Standing on a nest with nothing around: mark it empty.
        if let Some((k, _)) = ninhos
            .iter()
            .enumerate()
            .find(|(_, n)| mq(**n).distance(eu) < 14.0)
        {
            if !self
                .ninhos_vazios
                .iter()
                .any(|(i, ate)| *i == k && *ate > agora)
            {
                let velho = (0..self.ninhos_vazios.len())
                    .min_by(|a, b| {
                        self.ninhos_vazios[*a]
                            .1
                            .total_cmp(&self.ninhos_vazios[*b].1)
                    })
                    .unwrap_or(0);
                self.ninhos_vazios[velho] = (k, agora + NINHO_VAZIO_S);
            }
        }
        ninhos
            .iter()
            .enumerate()
            .filter(|(k, _)| {
                !self
                    .ninhos_vazios
                    .iter()
                    .any(|(i, ate)| i == k && *ate > agora)
            })
            .map(|(_, n)| mq(*n))
            .min_by(|a, b| a.distance_squared(eu).total_cmp(&b.distance_squared(eu)))
    }

    /// THE "SAIL TO" PILL ON THE MINIMAP (sea HUD clean-up, 16/11/2026: no
    /// floating button over the ship). It sits where the land's coordinates
    /// sit, at the foot of the disc (`disco` = its centre and radius), and
    /// opens one row per port under the minimap — name, distance, ETA and
    /// "Sail". Returns the island chosen. Rows are text built only while the
    /// panel is open.
    pub fn desenha_portos(
        &mut self,
        disco: (Vec2, f32),
        s: f32,
        eu: Option<Vec2>,
        velocidade: f32,
    ) -> Option<u8> {
        let (botao, painel) = retangulos_dos_portos(disco, s);
        self.ancora = Some((botao, painel));
        let m = Vec2::from(mouse_position());
        let sobre = botao.toca(m);
        let ativo = self.portos_abertos;
        estilo::ret_arredondado(botao, botao.h * 0.5, estilo::alfa(estilo::FUNDO, 0.92));
        estilo::borda_arredondada(
            botao,
            botao.h * 0.5,
            1.0,
            estilo::alfa(estilo::OURO, if sobre || ativo { 0.9 } else { 0.45 }),
        );
        let cor = if sobre || ativo {
            estilo::OURO
        } else {
            estilo::TEXTO
        };
        let e = botao.h / 2.3;
        crate::mar_cena::ancora(vec2(botao.x + e * 1.4, botao.center().y), e * 0.75, cor);
        let rotulo = if ativo { "Close" } else { "Sail to…" };
        estilo::texto_forte(
            botao.x + e * 2.6,
            botao.center().y + e * 0.48,
            rotulo,
            12,
            cor,
        );
        let clique = crate::foco::clique();
        if clique && sobre {
            self.portos_abertos = !self.portos_abertos;
            return None;
        }
        if !self.portos_abertos {
            return None;
        }
        let eu = eu?;
        let lista = portos(eu);
        let linha = painel.h / lista.len().max(1) as f32;
        estilo::painel(painel);
        let mut escolha = None;
        for (k, (ilha, nome, _, d)) in lista.iter().enumerate() {
            let y = painel.y + k as f32 * linha;
            let destino = self.atracar == Some(*ilha) && self.ativa();
            let cor = if destino { estilo::AUTO } else { estilo::TEXTO };
            let info = format!("{:.0} m · {}", d, formata_eta(eta_s(*d, velocidade)));
            let e = linha / 2.4;
            let bw = largura_do_botao(e);
            let b = Rect::new(
                painel.x + painel.w - bw - e * 0.8,
                y + e * 0.35,
                bw,
                linha - e * 0.7,
            );
            let x_info = b.x - e * 1.2 - estilo::medir(&info, 12);
            // A long island name steps down a size rather than run into
            // the distance (only on a screen too narrow for the panel).
            let cabe = x_info - painel.x - e * 2.2;
            let tam = if estilo::medir(nome, 14) <= cabe {
                14
            } else {
                11
            };
            estilo::texto(painel.x + e * 1.2, y + linha * 0.66, nome, tam, cor);
            estilo::texto(x_info, y + linha * 0.66, &info, 12, estilo::SUAVE);
            estilo::botao(b, "Sail", estilo::estado_de(b, false, destino), true);
            if clique && b.toca(m) {
                escolha = Some(*ilha);
            }
        }
        if escolha.is_some() {
            self.portos_abertos = false;
        }
        escolha
    }

    /// Does the pill (or the open panel) take this click? (At sea only: they
    /// are drawn only there.)
    pub fn pega_em(&self, m: Vec2) -> bool {
        let Some((botao, painel)) = self.ancora.filter(|_| crate::mar_ui::no_mar()) else {
            return false;
        };
        botao.toca(m) || (self.portos_abertos && painel.toca(m))
    }
}

/// Where the "Sail to" pill and its panel go, from the minimap's disc: the
/// pill at the disc's foot, the panel under it, kept on screen.
///
/// Sized by the TEXT (the HUD's fonts follow the UI scale, not the layout's
/// `s`): on a phone the rows grow with the letters instead of crushing them.
pub fn retangulos_dos_portos(disco: (Vec2, f32), _s: f32) -> (Rect, Rect) {
    let (c, raio) = disco;
    let e = estilo::medir_dim("M", 13).height.max(6.0);
    let (w, h) = (estilo::medir_forte("Sail to…", 12) + e * 3.6, e * 2.3);
    let tela_w = crate::render3d::tela().0;
    let bx = (c.x - w * 0.5).min(tela_w - w - 6.0);
    let botao = Rect::new(bx, c.y + raio - h * 0.3, w, h);
    let ilhas = shared::mar::ilhas();
    let nome = ilhas
        .iter()
        .map(|i| estilo::medir(i.nome, 14))
        .fold(0.0, f32::max);
    let info = estilo::medir("9999 m · 9:99", 12);
    let largura = e * 1.2 + nome + e * 1.4 + info + e * 1.0 + largura_do_botao(e) + e * 0.8;
    let tela = crate::render3d::tela().0;
    let largura = largura.min(tela - 16.0);
    let tela_h = crate::render3d::tela().1;
    // Rows tighten (never below ~2 letters tall) before the panel would run
    // under the EXP strip at the foot of the screen.
    let n = ilhas.len() as f32;
    let alto = (e * 2.4 * n).min((tela_h - 56.0).max(e * 1.9 * n));
    let mut x = (c.x + raio - largura).clamp(8.0, (tela - largura - 8.0).max(8.0));
    let mut y = botao.y + botao.h + e * 0.6;
    // A short screen (a phone on its side): beside the minimap instead,
    // as high as it needs.
    if y + alto > tela_h - 40.0 {
        y = (tela_h - alto - 40.0).clamp(8.0, y);
        x = (c.x - raio - e * 2.0 - largura).max(8.0);
    }
    let painel = Rect::new(x, y, largura, alto);
    (botao, painel)
}

/// The "Sail" button of a port row, from the text's unit `e`.
fn largura_do_botao(e: f32) -> f32 {
    estilo::medir_forte("Sail", 13) + e * 2.0
}

#[cfg(test)]
mod testes {
    use super::*;

    /// AUTO-DOCK: sailing to a dock sends one request, then, once the hull is
    /// in reach of that dock's anchorage, docks — and only at THAT dock.
    #[test]
    fn chegar_ao_cais_atraca_sozinho() {
        let ilhas = shared::mar::ilhas();
        let (a, b) = (&ilhas[0], &ilhas[1]);
        let mut r = RotaDoMar::default();
        assert!(r.para_ilha(b.ilha as u8, 0.0));
        let de = mq(a.fundeadouro());
        assert_eq!(r.passo(de, 0.0, false), Passo::Enviar(mq(b.fundeadouro())));
        // Sailing: the route is alive, nothing to send.
        assert_eq!(r.passo(de + vec2(5.0, 0.0), 0.5, true), Passo::Nada);
        // Passing ANOTHER dock does not dock there.
        assert_ne!(r.passo(de, 1.0, true), Passo::Atracar(a.ilha as u8));
        // In reach of the destination's anchorage: dock.
        let perto = mq(b.fundeadouro()) + vec2(3.0, 0.0);
        assert_eq!(r.passo(perto, 30.0, true), Passo::Atracar(b.ilha as u8));
        assert!(!r.ativa(), "docking ends the crossing");
    }

    /// A tap on an island (or beside its dock) means its dock.
    #[test]
    fn tocar_na_ilha_vai_pro_cais_dela() {
        for i in shared::mar::ilhas() {
            let (d, atracar) = destino_do_toque(mq(i.centro));
            assert_eq!(atracar, Some(i.ilha as u8), "{}", i.nome);
            assert!(d.distance(mq(i.fundeadouro())) < 0.01);
            let (_, perto) = destino_do_toque(mq(i.fundeadouro()) + vec2(4.0, 0.0));
            assert_eq!(perto, Some(i.ilha as u8));
        }
        // Open water is open water.
        let n = mq(shared::mar::ninhos()[0]);
        assert_eq!(destino_do_toque(n), (n, None));
    }

    /// Stuck with no progress: asks again a few times, then gives up.
    #[test]
    fn parado_pede_de_novo_e_desiste() {
        let mut r = RotaDoMar::default();
        let p = vec2(0.0, 0.0);
        r.iniciar(vec2(200.0, 0.0), None, 0.0);
        assert!(matches!(r.passo(p, 0.0, false), Passo::Enviar(_)));
        let mut t = 0.0;
        let mut desistiu = false;
        for _ in 0..40 {
            t += 2.5;
            match r.passo(p, t, false) {
                Passo::Desistiu => {
                    desistiu = true;
                    break;
                }
                Passo::Atracar(_) | Passo::Chegou => panic!("arrived standing still"),
                _ => {}
            }
        }
        assert!(desistiu);
        // The same destination again does not restart a live crossing.
        r.iniciar(vec2(50.0, 0.0), None, 0.0);
        r.passo(p, 0.0, false);
        let antes = r.enviado_em;
        r.iniciar(vec2(50.2, 0.0), None, 0.1);
        assert_eq!(r.enviado_em, antes);
    }

    #[test]
    fn eta_e_faixa() {
        assert_eq!(formata_eta(42.4), "42 s");
        assert_eq!(formata_eta(65.0), "1:05");
        assert!((eta_s(100.0, 10.0) - 10.0).abs() < 1e-5);
        let mut r = RotaDoMar::default();
        let i = &shared::mar::ilhas()[0];
        r.para_ilha(i.ilha as u8, 0.0);
        let f = r.faixa(Some(vec2(0.0, 0.0)), Some(300.0), 10.0).unwrap();
        assert!(
            f.starts_with(&format!("Sailing to {}", i.nome)) && f.ends_with("30 s"),
            "{f}"
        );
    }

    /// AUTO COMBAT AT SEA picks the NEAREST sea mob (a shark beyond an eel
    /// is second), and with all of them sunk turns to the nests.
    #[test]
    fn o_auto_combate_no_mar_pega_o_bicho_mais_perto() {
        use shared::{EntityId, EntityTag};
        let n = shared::mar::ninhos()[0];
        let mut w = crate::world::World::default();
        let mut metas = vec![];
        let mut estados = vec![];
        for (id, tag, kind, dx) in [
            (1u32, EntityTag::Player, 0u16, 0.0f32),
            (2, EntityTag::Enemy, 70, 9.0),
            (3, EntityTag::Enemy, 72, 5.0),
            (4, EntityTag::Enemy, 70, 14.0),
        ] {
            metas.push(shared::EntityMeta {
                skins: 0,
                cla: String::new(),
                pk: Default::default(),
                auras: 0,
                id: EntityId(id),
                tag,
                name: None,
                hp_max: 100,
                faction: None,
                kind,
                nivel: 1,
                desafio: None,
                aparencia: 0,
                poder: 0,
                renascimentos: 0,
            });
            estados.push(shared::EntityState::quantize(
                EntityId(id),
                n + ::glam::Vec2::new(dx, 0.0),
                ::glam::Vec2::ZERO,
                100,
                if id == 1 { shared::ent_flags::SELF } else { 0 },
            ));
        }
        w.apply(metas, estados, &[]);
        let mut a = crate::auto_combate::AutoCombate::default();
        let eu = mq(n);
        a.ligar(eu);
        assert_eq!(
            a.escolher(&w, None, 0.0, None),
            Some(EntityId(3)),
            "not the nearest sea mob"
        );
        for id in [2, 3, 4] {
            w.ents.get_mut(&EntityId(id)).unwrap().state.hp = 0;
        }
        assert_eq!(a.escolher(&w, Some(EntityId(3)), 1.0, None), None);
        let mut r = RotaDoMar::default();
        let ninho = r.proximo_ninho(eu, 1.0).expect("no nest to hunt");
        assert!(
            ninho.distance(eu) > 14.0,
            "hunts the nest it already cleared"
        );
    }

    /// THE PILOT'S SWITCH follows AUTO COMBAT at sea, is resent while on
    /// (a new sea process starts off), survives a new crossing, and the
    /// retreat starts and ends on the server's own rule.
    #[test]
    fn o_piloto_segue_o_auto_e_recua_com_o_casco() {
        let mut r = RotaDoMar::default();
        assert_eq!(
            r.piloto.pedido(false, 0.0),
            None,
            "sent an off it never turned on"
        );
        assert_eq!(r.piloto.pedido(true, 0.0), Some(true));
        assert_eq!(r.piloto.pedido(true, 1.0), None, "resent every frame");
        assert_eq!(
            r.piloto.pedido(true, 1.0 + PILOTO_REENVIO_S),
            Some(true),
            "never resent"
        );
        r.iniciar(vec2(50.0, 0.0), None, 2.0);
        assert!(r.piloto.ligado(), "a new crossing switched the pilot off");
        assert!(!r.piloto.casco(0.5, false));
        assert!(
            !r.piloto.casco(0.15, true),
            "retreated with the repair ready"
        );
        assert!(
            r.piloto.casco(0.15, false),
            "no retreat at 15% with no repair"
        );
        assert!(
            !r.piloto.casco(0.4, false) && r.piloto.recuando,
            "came back under half"
        );
        assert!(!r.piloto.casco(0.6, false) && !r.piloto.recuando);
        assert_eq!(r.piloto.pedido(false, 9.0), Some(false));
    }

    /// AUTO COMBAT's hunt at sea: the nearest nest, skipping one just found
    /// empty; scans are throttled.
    #[test]
    fn a_caca_no_mar_vai_ao_ninho_mais_perto() {
        let ninhos = shared::mar::ninhos();
        let mut r = RotaDoMar::default();
        let eu = mq(ninhos[0]) + vec2(2.0, 0.0);
        let primeiro = r.proximo_ninho(eu, 0.0).unwrap();
        assert!(
            primeiro.distance(mq(ninhos[0])) > 1.0,
            "went back to the empty nest it is on"
        );
        assert!(r.pode_varrer(1.0));
        assert!(!r.pode_varrer(1.1), "scans every frame");
        assert!(r.pode_varrer(1.0 + VARREDURA_S + 0.01));
    }
}
