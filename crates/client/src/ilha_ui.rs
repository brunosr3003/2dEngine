//! TERRITORIES, phase 4 (docs/TERRITORIOS.md): the GOVERNOR's and the
//! FARMER's panels on a held islet. The Governor raises the village and
//! its defences (walls, gates, cannons, defenders); the Farmer the islet's
//! resources (lumber, quarry, energy). Each row: the level and its cap, the
//! next level's materials (what I have / what it needs) and gold, Upgrade.
//! The server checks everything again and writes it (`PedidoNavio::
//! MelhorarIlha`).

use crate::hud_estilo as e;
use crate::ui::Toque as _;
use macroquad::prelude::*;
use shared::navio::PedidoNavio;
use shared::territorio as tr;

#[derive(Default)]
pub struct IlhaUi {
    /// Open on (islet, the Governor's panel?).
    pub aberto: Option<(u8, bool)>,
}

/// The held islet of MY clan whose Governor or Farmer I stand by:
/// (islet, Governor?).
fn npc_perto(w: &crate::world::World) -> Option<(u8, bool)> {
    if !w.em_terra {
        return None;
    }
    let eu = w.ents.get(&w.self_id?)?;
    let p = ::glam::Vec2::new(eu.render_pos.x, eu.render_pos.y);
    let k = shared::mar::indice_da_ilhota(p)?;
    let dono = crate::mar_ui::dono_da_ilhota(k as u8)?;
    if dono.tag.is_empty() || Some(dono.tag.as_str()) != crate::mapa::minha_tag_de_cla().as_deref() {
        return None;
    }
    let c = shared::mar::ilhotas()[k].centro;
    if p.distance(tr::pos_do_governador(c)) <= tr::ALCANCE_DO_NPC {
        Some((k as u8, true))
    } else if p.distance(tr::pos_do_fazendeiro(c)) <= tr::ALCANCE_DO_NPC {
        Some((k as u8, false))
    } else {
        None
    }
}

impl IlhaUi {
    /// The "Governor" / "Farmer" pill over the helm toggle, when I stand by
    /// one of them on my clan's islet; it opens the panel. Walking away
    /// closes an open panel.
    pub fn pilula(&mut self, z: &crate::hud_layout::Zonas, w: &crate::world::World) {
        let perto = npc_perto(w);
        if self.aberto.is_some() && perto != self.aberto {
            self.aberto = None;
        }
        let Some((k, gov)) = perto else {
            return;
        };
        if self.aberto.is_some() {
            return;
        }
        let leme = crate::mar_ui::retangulo_do_leme(z);
        let r = Rect::new(leme.x - 60.0 * z.s, leme.y - 52.0 * z.s, leme.w + 120.0 * z.s, 42.0 * z.s);
        if crate::ui::botao(r, if gov { "Governor" } else { "Farmer" }, true) {
            self.aberto = Some((k, gov));
        }
    }

    /// The open panel. Returns the upgrade asked for.
    pub fn desenha(
        &mut self,
        conta: &dyn Fn(u16) -> u64,
        ouro: u64,
    ) -> Option<PedidoNavio> {
        let (k, gov) = self.aberto?;
        let ilha = shared::mar::ilhotas().get(k as usize)?;
        let dono = crate::mar_ui::dono_da_ilhota(k)?;
        let (sw, sh) = (screen_width(), screen_height());
        let s = (sh.min(sw) / 800.0).clamp(0.75, 1.5);
        let trilhas: Vec<usize> = (0..tr::N_TRILHAS)
            .filter(|t| tr::do_governador(*t) == gov)
            .collect();
        let linha = 64.0 * s;
        let w = (760.0 * s).min(sw - 24.0);
        let h = 120.0 * s + linha * trilhas.len() as f32;
        let p = Rect::new((sw - w) / 2.0, (sh - h) / 2.0, w, h);
        e::painel_destaque(p, e::OURO);
        let titulo = format!(
            "{} · {}",
            shared::idioma::tr(if gov { "Governor" } else { "Farmer" }),
            ilha.nome
        );
        e::texto_forte(p.x + 20.0 * s, p.y + 34.0 * s, &titulo, 22, e::OURO);
        let vila = dono.niveis[tr::trilha::VILA];
        let sub = format!(
            "{} islet · {} · your gold: {}",
            shared::idioma::tr(tr::nome_do_porte(ilha.porte)),
            shared::idioma::tr(tr::NOMES_DA_VILA[(vila as usize).clamp(1, 4) - 1]),
            ouro
        );
        e::texto_ajustado(&sub, p.x + 20.0 * s, p.y + 60.0 * s, p.w - 90.0 * s, 14, e::SUAVE);
        if crate::ui::botao(Rect::new(p.x + p.w - 52.0 * s, p.y + 12.0 * s, 36.0 * s, 32.0 * s), "x", true) {
            self.aberto = None;
            return None;
        }
        let mut pedido = None;
        for (n, t) in trilhas.iter().copied().enumerate() {
            let y = p.y + 84.0 * s + n as f32 * linha;
            let atual = dono.niveis[t];
            let teto = tr::teto(t, ilha.porte, vila);
            e::texto_forte(p.x + 20.0 * s, y + 22.0 * s, tr::NOMES_DAS_TRILHAS[t], 17, e::TEXTO);
            let nivel = if t == tr::trilha::VILA {
                shared::idioma::tr(tr::NOMES_DA_VILA[(atual as usize).clamp(1, 4) - 1]).into_owned()
            } else {
                format!("Lv {atual} / {teto}")
            };
            e::texto(p.x + 20.0 * s, y + 44.0 * s, &nivel, 13, e::SUAVE);
            let bw = 130.0 * s;
            let br = Rect::new(p.x + p.w - bw - 20.0 * s, y + 8.0 * s, bw, 40.0 * s);
            if atual >= teto {
                let motivo = if teto == 0 {
                    "Needs a Village"
                } else if t != tr::trilha::VILA && teto < tr::trilha_maxima(ilha.porte) {
                    "Grow the village"
                } else {
                    "Maximum"
                };
                e::texto_ajustado(motivo, br.x, br.y + 26.0 * s, bw, 13, e::SUAVE);
                continue;
            }
            let (mats, gold) = tr::custo(t, atual + 1);
            let mut ok = ouro >= gold;
            let ic = 26.0 * s;
            let mut x = p.x + 180.0 * s;
            for (item, q) in &mats {
                let tem = conta(*item);
                ok &= tem >= *q as u64;
                crate::icones::icone(*item, Rect::new(x, y + 12.0 * s, ic, ic), None, None);
                e::texto(
                    x + ic + 4.0 * s,
                    y + 30.0 * s,
                    &format!("{}/{}", tem.min(*q as u64), q),
                    12,
                    if tem >= *q as u64 { e::VERDE } else { e::VERMELHO },
                );
                x += ic + 92.0 * s;
            }
            e::texto(
                x,
                y + 30.0 * s,
                &format!("{gold} gold"),
                12,
                if ouro >= gold { e::VERDE } else { e::VERMELHO },
            );
            if crate::ui::botao(br, "Upgrade", ok) && br.toca(Vec2::from(mouse_position())) {
                pedido = Some(PedidoNavio::MelhorarIlha { trilha: t as u8 });
            }
        }
        pedido
    }
}
