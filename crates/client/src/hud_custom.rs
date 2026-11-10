//! The player's own HUD layout (a player's idea, owner 10/11/2026: "on mobile
//! the buttons are too small and some icons are placed in impractical
//! locations — let players resize all buttons and position them anywhere").
//!
//! Applied on top of `hud_layout::zonas_com` inside `hud_layout::atual`, so
//! drawing AND taps follow it with no change anywhere else.
//!
//! - One "all buttons" size: every button grows around the screen corner it
//!   lives in, so a cluster keeps its shape and grows into the screen.
//! - Per element: its own size (around its center) and, once dragged, its
//!   center as a fraction of the safe area (survives rotating the phone and
//!   different screens).
//!
//! Saved on the DEVICE (next to `lembranca`), not on the character: a layout
//! is about the screen and the hand holding it.

use macroquad::prelude::*;
use std::cell::RefCell;
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicU32, Ordering};

use crate::hud_layout::Zonas;

pub const TAMANHO_MIN: f32 = 0.6;
pub const TAMANHO_MAX: f32 = 2.5;
pub const PASSO: f32 = 0.1;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Ajuste {
    /// Center as a fraction of the safe area; `None` = where the game puts it.
    pub centro: Option<Vec2>,
    pub tamanho: f32,
}

impl Default for Ajuste {
    fn default() -> Self {
        Self { centro: None, tamanho: 1.0 }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Layout {
    /// Size of every button at once.
    pub botoes: f32,
    pub itens: BTreeMap<String, Ajuste>,
}

impl Default for Layout {
    fn default() -> Self {
        Self { botoes: 1.0, itens: BTreeMap::new() }
    }
}

/// What the editor offers: id (as in `Zonas::todos`), name shown, and whether
/// it can change size (panels with text inside only move).
pub const ITENS: &[(&str, &str, bool)] = &[
    ("atacar", "Attack", true),
    ("skill1", "Skill 1", true),
    ("skill2", "Skill 2", true),
    ("skill3", "Skill 3", true),
    ("pulo", "Jump", true),
    ("pocao", "Potion C", true),
    ("rapido8", "Slot 8", true),
    ("rapido9", "Slot 9", true),
    ("rapido0", "Slot 0", true),
    ("auto_combate", "Auto combat", true),
    ("auto_coleta", "Auto gather", true),
    ("sprint", "Sprint", true),
    ("montaria", "Mount", true),
    ("economia", "Battery", true),
    ("menu", "Menu", true),
    ("icone_ofertas", "Offers", true),
    ("icone_bolsa", "Bag", true),
    ("icone_missoes", "Quests", true),
    ("icone_diarias", "Dailies", true),
    ("icone_grupo", "Party", true),
    ("icone_avisos", "Notices", true),
    ("icone_presenca", "Presence", true),
    ("minimapa", "Minimap", true),
    ("joystick", "Joystick", true),
    ("area", "Zone name", true),
    ("chat", "Chat", false),
    ("ficha", "Character", false),
    ("rastreador", "Quests / Party", false),
    ("alvo", "Target", false),
];

/// Ids that the "all buttons" size touches (the minimap is not a button).
fn e_botao(id: &str) -> bool {
    !matches!(id, "minimapa" | "area" | "chat" | "ficha" | "alvo" | "joystick" | "rastreador")
}

/// The player's size for one element (1 = the game's). For panels that scale
/// their own text with it (`hud::draw_area`).
pub fn tamanho(id: &str) -> f32 {
    LAYOUT.with(|l| {
        l.borrow_mut()
            .get_or_insert_with(carrega)
            .itens
            .get(id)
            .map_or(1.0, |a| a.tamanho)
    })
}

pub fn nome(id: &str) -> &'static str {
    ITENS.iter().find(|i| i.0 == id).map_or("", |i| i.1)
}

pub fn redimensiona(id: &str) -> bool {
    ITENS.iter().find(|i| i.0 == id).is_some_and(|i| i.2)
}

thread_local! {
    static LAYOUT: RefCell<Option<Layout>> = const { RefCell::new(None) };
}
/// Bumped on every change: part of `hud_layout::atual`'s memo key.
static GERACAO: AtomicU32 = AtomicU32::new(0);

pub fn geracao() -> u32 {
    GERACAO.load(Ordering::Relaxed)
}

pub fn atual() -> Layout {
    LAYOUT.with(|l| l.borrow_mut().get_or_insert_with(carrega).clone())
}

/// Replaces the layout (the editor calls it on every change) and saves it.
/// `salvar = false` while a finger is dragging (the file is written when it
/// lifts).
pub fn define(novo: Layout, salvar: bool) {
    if salvar {
        salva(&novo);
    }
    LAYOUT.with(|l| *l.borrow_mut() = Some(novo));
    GERACAO.fetch_add(1, Ordering::Relaxed);
}

/// What the server records (`ClientMessage::LayoutDoHud`).
pub fn para_rede(l: &Layout) -> shared::protocol::LayoutDoHud {
    let (w, h) = crate::render3d::tela();
    shared::protocol::LayoutDoHud {
        plataforma: plataforma().into(),
        tela_w: w.round().clamp(0.0, 65535.0) as u16,
        tela_h: h.round().clamp(0.0, 65535.0) as u16,
        escala_ui: crate::hud_layout::escala_ui(),
        botoes: l.botoes,
        itens: l
            .itens
            .iter()
            .map(|(id, a)| shared::protocol::ItemDoHud {
                id: id.clone(),
                tamanho: a.tamanho,
                centro: a.centro.map(|c| (c.x, c.y)),
            })
            .collect(),
    }
}

fn plataforma() -> &'static str {
    if cfg!(target_os = "android") {
        "android"
    } else if cfg!(target_os = "ios") {
        "ios"
    } else if cfg!(target_os = "windows") {
        "windows"
    } else if cfg!(target_os = "macos") {
        "mac"
    } else {
        "linux"
    }
}

fn arquivo() -> Option<std::path::PathBuf> {
    crate::lembranca::caminho().map(|p| p.with_extension("hud"))
}

/// Text, one line per thing: `botoes 1.3` and `id tamanho fx fy` (`- -` when
/// not moved). Unknown or broken lines are skipped.
pub fn codifica(l: &Layout) -> String {
    let mut s = format!("botoes {:.2}\n", l.botoes);
    for (id, a) in &l.itens {
        match a.centro {
            Some(c) => s += &format!("{id} {:.2} {:.4} {:.4}\n", a.tamanho, c.x, c.y),
            None => s += &format!("{id} {:.2} - -\n", a.tamanho),
        }
    }
    s
}

pub fn decodifica(t: &str) -> Layout {
    let mut l = Layout::default();
    for linha in t.lines() {
        let p: Vec<&str> = linha.split_whitespace().collect();
        match p.as_slice() {
            ["botoes", k] => {
                if let Ok(k) = k.parse::<f32>() {
                    l.botoes = limita(k);
                }
            }
            [id, k, x, y] if ITENS.iter().any(|i| i.0 == *id) => {
                let Ok(k) = k.parse::<f32>() else { continue };
                let centro = match (x.parse::<f32>(), y.parse::<f32>()) {
                    (Ok(x), Ok(y)) if x.is_finite() && y.is_finite() => {
                        Some(vec2(x.clamp(0.0, 1.0), y.clamp(0.0, 1.0)))
                    }
                    _ => None,
                };
                l.itens.insert(id.to_string(), Ajuste { centro, tamanho: limita(k) });
            }
            _ => {}
        }
    }
    l
}

pub fn limita(k: f32) -> f32 {
    if k.is_finite() {
        ((k / PASSO).round() * PASSO).clamp(TAMANHO_MIN, TAMANHO_MAX)
    } else {
        1.0
    }
}

fn carrega() -> Layout {
    arquivo()
        .and_then(|p| std::fs::read_to_string(p).ok())
        .map(|t| decodifica(&t))
        .unwrap_or_default()
}

fn salva(l: &Layout) {
    if let Some(p) = arquivo() {
        if *l == Layout::default() {
            let _ = std::fs::remove_file(p);
        } else {
            let _ = std::fs::write(p, codifica(l));
        }
    }
}

/// The rect of one id inside `z` (the same ids as `Zonas::todos`).
pub fn campo<'a>(z: &'a mut Zonas, id: &str) -> Option<&'a mut Rect> {
    Some(match id {
        "atacar" => &mut z.atacar,
        "skill1" => &mut z.skills[0],
        "skill2" => &mut z.skills[1],
        "skill3" => &mut z.skills[2],
        "pulo" => &mut z.pulo,
        "pocao" => &mut z.pocao,
        "rapido8" => &mut z.rapidos[0],
        "rapido9" => &mut z.rapidos[1],
        "rapido0" => &mut z.rapidos[2],
        "auto_combate" => &mut z.auto_combate,
        "auto_coleta" => &mut z.auto_coleta,
        "sprint" => &mut z.sprint,
        "montaria" => &mut z.montaria,
        "economia" => &mut z.economia,
        "menu" => &mut z.menu,
        "icone_ofertas" => &mut z.icones[0],
        "icone_bolsa" => &mut z.icones[1],
        "icone_missoes" => &mut z.icones[2],
        "icone_diarias" => &mut z.icones[3],
        "icone_grupo" => &mut z.icones[4],
        "icone_avisos" => &mut z.icones[5],
        "icone_presenca" => &mut z.icones[6],
        "minimapa" => &mut z.minimapa,
        "joystick" => &mut z.joystick,
        "area" => &mut z.area,
        "chat" => &mut z.chat,
        "ficha" => &mut z.ficha,
        "rastreador" => &mut z.rastreador,
        "alvo" => &mut z.alvo,
        _ => return None,
    })
}

/// Rects that ride along with a parent (same move and scale).
fn filhos<'a>(z: &'a mut Zonas, id: &str) -> Vec<&'a mut Rect> {
    match id {
        "minimapa" => vec![&mut z.mapa_icone],
        "ficha" => vec![&mut z.modo_pk, &mut z.buffs],
        _ => vec![],
    }
}

/// Where a button's group grows from with the "all buttons" size.
fn ancora(id: &str, c: Vec2, t: Rect) -> Vec2 {
    let (esq, dir, topo, baixo) = (t.x, t.x + t.w, t.y, t.y + t.h);
    match id {
        "atacar" | "skill1" | "skill2" | "skill3" | "pulo" => vec2(dir, baixo),
        "pocao" | "rapido8" | "rapido9" | "rapido0" | "auto_combate" | "auto_coleta" => {
            vec2(t.center().x, baixo)
        }
        "economia" | "montaria" | "sprint" => vec2(esq, baixo),
        "menu" => vec2(dir, topo),
        _ if id.starts_with("icone_") => vec2(dir, topo),
        // Anything else: its own corner.
        _ => vec2(
            if c.x < t.center().x { esq } else { dir },
            if c.y < t.center().y { topo } else { baixo },
        ),
    }
}

/// Maps `r` with "scale `k` around `de`, then put `de` at `para`".
fn transforma(r: Rect, de: Vec2, para: Vec2, k: f32) -> Rect {
    let p = para + (r.point() - de) * k;
    Rect::new(p.x, p.y, r.w * k, r.h * k)
}

/// How much to slide `r` so it sits inside `t` (top/left win if it is bigger).
fn dentro(r: Rect, t: Rect) -> Vec2 {
    let eixo = |p: f32, w: f32, tp: f32, tw: f32| {
        if w >= tw { tp - p } else { (tp - p).max(0.0).min(tp + tw - p - w) }
    };
    vec2(eixo(r.x, r.w, t.x, t.w), eixo(r.y, r.h, t.y, t.h))
}

/// The game's layout `z` (in screen px, inside `seguro`) with the player's.
pub fn aplica(mut z: Zonas, seguro: Rect, l: &Layout) -> Zonas {
    if *l == Layout::default() {
        return z;
    }
    for &(id, _, _) in ITENS {
        let Some(r) = campo(&mut z, id).map(|r| *r) else { continue };
        let a = l.itens.get(id).copied().unwrap_or_default();
        let c = r.center();
        // 1. "All buttons": each GROUP grows from one anchor, so a row or a
        //    cluster keeps its shape (anchoring each button to its own half
        //    of the screen made a row that crosses the middle fold onto itself).
        let k_todos = if e_botao(id) { l.botoes } else { 1.0 };
        let canto = ancora(id, c, seguro);
        let c1 = canto + (c - canto) * k_todos;
        // 2. Its own size, 3. its own place.
        let k = k_todos * if redimensiona(id) { a.tamanho } else { 1.0 };
        let para = a.centro.map_or(c1, |f| seguro.point() + f * seguro.size());
        if (k - 1.0).abs() < 1e-4 && para.distance(c) < 0.01 {
            continue;
        }
        let mut novo = transforma(r, c, para, k);
        let ajuste = dentro(novo, seguro);
        novo.x += ajuste.x;
        novo.y += ajuste.y;
        let para = para + ajuste;
        *campo(&mut z, id).unwrap() = novo;
        for f in filhos(&mut z, id) {
            *f = transforma(*f, c, para, k);
        }
    }
    z
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn ida_e_volta_do_arquivo() {
        let mut l = Layout { botoes: 1.3, ..Default::default() };
        l.itens.insert("atacar".into(), Ajuste { centro: Some(vec2(0.5, 0.25)), tamanho: 1.5 });
        l.itens.insert("pulo".into(), Ajuste { centro: None, tamanho: 0.8 });
        let d = decodifica(&codifica(&l));
        assert!((d.botoes - 1.3).abs() < 1e-4);
        assert_eq!(d.itens["pulo"], Ajuste { centro: None, tamanho: 0.8 });
        assert!(d.itens["atacar"].centro.unwrap().distance(vec2(0.5, 0.25)) < 1e-3);
        // Garbage and unknown ids are skipped.
        assert_eq!(decodifica("lixo\nfoo 1 2 3\nbotoes x"), Layout::default());
    }

    #[test]
    fn tudo_fica_na_tela_e_o_padrao_nao_muda_nada() {
        let seguro = Rect::new(0.0, 0.0, 1280.0, 720.0);
        let z = crate::hud_layout::zonas_com(1280.0, 720.0, [0.0; 4], 1.6, false, false, false);
        let igual = aplica(z, seguro, &Layout::default());
        assert_eq!(format!("{:?}", igual), format!("{:?}", z));
        let mut l = Layout { botoes: 2.5, ..Default::default() };
        l.itens.insert("atacar".into(), Ajuste { centro: Some(vec2(1.0, 1.0)), tamanho: 2.5 });
        l.itens.insert("minimapa".into(), Ajuste { centro: Some(vec2(0.0, 0.5)), tamanho: 1.0 });
        let mut g = aplica(z, seguro, &l);
        for &(id, _, _) in ITENS {
            let r = *campo(&mut g, id).unwrap();
            assert!(
                r.x >= -0.01 && r.y >= -0.01 && r.x + r.w <= 1280.01 && r.y + r.h <= 720.01,
                "{id} fora da tela: {r:?}"
            );
        }
        // The attack button went to the bottom-right corner, much bigger.
        assert!(g.atacar.w > z.atacar.w * 3.0);
        assert!((g.atacar.y + g.atacar.h - 720.0).abs() < 0.5);
        // The map button rides with the minimap.
        assert!(g.minimapa.contains(g.mapa_icone.center()));
    }
}
