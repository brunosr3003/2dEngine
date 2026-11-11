//! The bag's HELP tab: the character's gear against the level, slot by
//! slot, a few tips, and where to hunt and which dungeon to run (owner,
//! 11/11/2026). Logic in `shared::ajuda`; this only draws.

use macroquad::prelude::*;
use shared::ajuda::{self, Peca};
use shared::protocol::ZonaNoMapa;
use std::cell::RefCell;
use std::collections::HashMap;

use crate::hud_estilo::{self as estilo, u};

thread_local! {
    /// The current island's monster zones and names (`MapaDaIlha`).
    static ILHA: RefCell<(Vec<ZonaNoMapa>, HashMap<u16, String>)> = RefCell::new((Vec::new(), HashMap::new()));
}

/// Called when a `MapaDaIlha` arrives.
pub fn define_ilha(zonas: &[ZonaNoMapa], nomes: &HashMap<u16, String>) {
    ILHA.with(|i| *i.borrow_mut() = (zonas.to_vec(), nomes.clone()));
}

/// The best monster of THIS island for the range: the dominant one of the
/// zone whose levels overlap it the most. `None` = this island has no zone
/// in range (the tab points to another island instead).
fn mob_daqui(baixo: u32, alto: u32) -> Option<(String, u16, u16)> {
    ILHA.with(|i| {
        let (zonas, nomes) = &*i.borrow();
        zonas
            .iter()
            .filter(|z| z.lv_max as u32 >= baixo && z.lv_min as u32 <= alto)
            .max_by_key(|z| (z.lv_max as u32).min(alto) as i64 - (z.lv_min as u32).max(baixo) as i64)
            .and_then(|z| {
                let kind = z.bichos.first()?.0;
                let nome = nomes.get(&kind).cloned().unwrap_or_else(|| crate::icones_ui::nome_do_bicho(kind).to_string());
                Some((nome, z.lv_min, z.lv_max))
            })
    })
}

fn cor_da_fracao(f: f32) -> Color {
    if f >= 0.9 {
        Color::new(0.40, 0.85, 0.45, 1.0)
    } else if f >= 0.7 {
        Color::new(0.96, 0.78, 0.30, 1.0)
    } else {
        Color::new(0.92, 0.33, 0.30, 1.0)
    }
}

fn cor_do_atraso(a: u8) -> Color {
    match a {
        0 => Color::new(0.40, 0.85, 0.45, 1.0),
        1 => Color::new(0.96, 0.78, 0.30, 1.0),
        _ => Color::new(0.92, 0.33, 0.30, 1.0),
    }
}

/// Draws the tab inside `r` (the bag's item area).
pub fn desenha(r: Rect, nivel: u32, stats: Option<&shared::PlayerStats>, equip: &shared::Equipment) {
    let nivel = nivel.max(1);
    let x = r.x + u(8.0);
    let w = r.w - u(16.0);
    let mut y = r.y + u(20.0);
    estilo::texto_forte(x, y, &format!("Your strength at level {nivel}"), 16, estilo::OURO);
    y += u(10.0);
    let poder = stats.map_or(0, shared::dungeon::poder_de_stats);
    if let Some(s) = stats {
        for (nome, c) in ["Attack", "Defense", "Health"].iter().zip(ajuda::comparar(nivel, s)) {
            y += u(22.0);
            let f = c.fracao();
            estilo::texto(x, y, nome, 13, estilo::TEXTO);
            let barra = Rect::new(x + u(70.0), y - u(11.0), w - u(200.0), u(12.0));
            draw_rectangle(barra.x, barra.y, barra.w, barra.h, Color::new(1.0, 1.0, 1.0, 0.08));
            draw_rectangle(barra.x, barra.y, barra.w * f.clamp(0.0, 1.0), barra.h, cor_da_fracao(f));
            estilo::texto(
                barra.x + barra.w + u(8.0),
                y,
                &format!("{} / {} ({:.0}%)", c.tem, c.esperado, f * 100.0),
                12,
                cor_da_fracao(f),
            );
        }
        y += u(16.0);
        estilo::texto(x, y, "Expected = a character of your level with gear of the level at +0.", 11, estilo::SUAVE);
    }

    // ── slot by slot ──
    y += u(24.0);
    estilo::texto_forte(x, y, "Gear", 14, estilo::OURO);
    let col = w / 2.0;
    for (k, (nome, p)) in ajuda::pecas(equip).iter().enumerate() {
        let (cx, cy) = (x + (k % 2) as f32 * col, y + u(20.0) + (k / 2) as f32 * u(18.0));
        let cor = cor_do_atraso(ajuda::atraso(p, nivel));
        let txt = match p {
            Peca::Vazia => format!("{nome}: empty"),
            Peca::Com { item_level, refino, .. } => format!("{nome}: item Lv {item_level} +{refino}"),
        };
        estilo::texto_ajustado(&txt, cx, cy, col - u(8.0), 12, cor);
    }
    y += u(20.0) + 4.0 * u(18.0);

    // ── tips ──
    for d in ajuda::dicas(nivel, equip) {
        y += u(16.0);
        estilo::texto_ajustado(&format!("• {d}"), x, y, w, 12, estilo::TEXTO);
    }

    // ── where to go ──
    y += u(26.0);
    estilo::texto_forte(x, y, "Recommended", 14, estilo::OURO);
    let (baixo, alto) = ajuda::faixa_de_caca(nivel, poder);
    y += u(20.0);
    let caca = match mob_daqui(baixo, alto) {
        Some((mob, lo, hi)) => format!("Hunt: {mob} (Lv {lo}-{hi}) on this island"),
        None => format!("Hunt: Lv {baixo}-{alto} monsters on {}", ajuda::ilha_para(alto).nome),
    };
    estilo::texto_ajustado(&caca, x, y, w, 13, estilo::TEXTO);
    y += u(18.0);
    let dg = match ajuda::dungeon_para(nivel, poder) {
        Some(c) => format!("Dungeon: {} (Lv {})", c.nome, c.nivel_min),
        None => "Dungeon: none yet — get stronger first".into(),
    };
    estilo::texto_ajustado(&dg, x, y, w, 13, estilo::TEXTO);
    if alto + 3 < nivel {
        y += u(18.0);
        estilo::texto_ajustado(
            "Your gear is behind your level: monsters of your level will hit hard.",
            x,
            y,
            w,
            12,
            Color::new(0.96, 0.78, 0.30, 1.0),
        );
    }
}
