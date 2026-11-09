//! Item bar configurator, in the MIR4 mould: pick each slot's consumable,
//! switch AUTO on and adjust the threshold. Opens from the Menu (System ->
//! Bar), by clicking an empty slot or right-clicking a slot — no key opens it.
//!
//! Escolher e' dois cliques: um item da lista e um espaco (ou o contrario).
#[allow(unused_imports)]
use crate::ui::Toque as _;
use macroquad::prelude::*;
use shared::InventorySlot;

use crate::barra::{self, Barra, ESPACOS};
use crate::hud_estilo as estilo;

const TECLAS: [&str; ESPACOS] = ["C", "8", "9", "0"];

#[derive(Default)]
pub struct ConfigBarra {
    pub aberto: bool,
    /// Item escolhido na lista, esperando um espaco.
    selecionado: Option<u16>,
    /// Espaco escolhido, esperando um item.
    pub espaco_alvo: Option<usize>,
    /// An item's magnifier asked for "Where to get" (main opens the popup).
    pub onde_obter: Option<u16>,
    /// The consumables list: with a bag full of potions it runs off the screen.
    rolagem: crate::rolagem::Rolagem,
    /// Touched a potion's use % this frame (the tutorial step counts it).
    pub mexeu_no_limiar: bool,
}

/// Consumables in the bag that fit on the bar, without repeats, with the quantity.
pub fn consumiveis(slots: &[InventorySlot]) -> Vec<(u16, u32)> {
    let mut v: Vec<(u16, u32)> = Vec::new();
    for s in slots
        .iter()
        .filter(|s| s.qty > 0 && s.instance.is_none() && barra::categoria(s.item_id).is_some())
    {
        match v.iter_mut().find(|(id, _)| *id == s.item_id) {
            Some((_, q)) => *q += s.qty,
            None => v.push((s.item_id, s.qty)),
        }
    }
    v.sort_by_key(|(id, _)| *id);
    v
}

impl ConfigBarra {
    /// `espaco`: opens with that slot already chosen (a click on an empty one).
    pub fn abrir(&mut self, espaco: Option<usize>) {
        *self = Self {
            aberto: true,
            selecionado: None,
            espaco_alvo: espaco,
            ..Self::default()
        };
    }

    pub fn fechar(&mut self) {
        *self = Self::default();
    }

    /// A click on a list item. Returns whether the bar changed.
    pub fn escolhe_item(&mut self, barra: &mut Barra, id: u16) -> bool {
        match self.espaco_alvo.take() {
            Some(i) => {
                barra.atribuir(i, id);
                true
            }
            None => {
                self.selecionado = Some(id);
                false
            }
        }
    }

    /// A click on a slot. Returns whether the bar changed.
    pub fn escolhe_espaco(&mut self, barra: &mut Barra, i: usize) -> bool {
        match self.selecionado.take() {
            Some(id) => {
                barra.atribuir(i, id);
                true
            }
            None => {
                self.espaco_alvo = if self.espaco_alvo == Some(i) {
                    None
                } else {
                    Some(i)
                };
                false
            }
        }
    }

    /// Draws and handles the click. Returns `true` when the bar changed (save).
    ///
    /// Redone 09/11/2026 (owner: "the potion and consumable settings menu is
    /// very ugly, non intuitive, buttons are too small"): a 900×420 canvas
    /// scaled to the screen like the other redesigned panels, one row per
    /// slot with finger-sized controls, and a line that always says what to
    /// tap next.
    pub fn desenha(
        &mut self,
        barra: &mut Barra,
        slots: &[InventorySlot],
        nome: &dyn Fn(u16) -> String,
    ) -> bool {
        let seguro = crate::hud_layout::tela_segura();
        let k = estilo::escala_do_painel(BASE_W, BASE_H)
            .min((seguro.w - 8.0) / BASE_W)
            .min((seguro.h - 8.0) / BASE_H);
        estilo::no_painel(k, || self.desenha_na_escala(barra, slots, nome, k, seguro))
    }

    fn desenha_na_escala(
        &mut self,
        barra: &mut Barra,
        slots: &[InventorySlot],
        nome: &dyn Fn(u16) -> String,
        k: f32,
        seguro: Rect,
    ) -> bool {
        let o = vec2(seguro.center().x - BASE_W * k * 0.5, seguro.center().y - BASE_H * k * 0.5);
        let r = |x: f32, y: f32, w: f32, h: f32| Rect::new(o.x + x * k, o.y + y * k, w * k, h * k);
        crate::hud_layout::escurece(0.5);
        let p = r(0.0, 0.0, BASE_W, BASE_H);
        estilo::painel_destaque(p, estilo::OURO);
        let m = Vec2::from(mouse_position());
        let clique = crate::foco::clique();
        let mut mudou = false;

        estilo::texto_forte(p.x + 22.0 * k, p.y + 38.0 * k, "Potion bar", 22, estilo::OURO);
        // The next step, always spelled out.
        let (passo, cor_passo) = match (self.espaco_alvo, self.selecionado) {
            (Some(i), _) => (format!("Now pick a potion from your bag for slot {} ›", TECLAS[i]), estilo::OURO),
            (None, Some(id)) => (format!("Now tap the slot for {} ‹", nome(id)), estilo::OURO),
            _ => ("Tap a slot, then a potion from your bag. AUTO drinks it for you.".to_string(), estilo::SUAVE),
        };
        estilo::texto_ajustado(&passo, p.x + 22.0 * k, p.y + 62.0 * k, 760.0 * k, 14, cor_passo);
        let x_fechar = r(BASE_W - 56.0, 10.0, 46.0, 46.0);
        estilo::botao(x_fechar, "X", estilo::estado_de(x_fechar, false, false), false);
        if clique && x_fechar.toca(m) {
            self.fechar();
            return false;
        }

        // ── the four slots ──
        let (lx, lw) = (16.0, 604.0);
        for i in 0..ESPACOS {
            let y = 78.0 + i as f32 * 83.0;
            let linha = r(lx, y, lw, 77.0);
            let alvo = self.espaco_alvo == Some(i);
            let esp = barra.espacos[i];
            let esperando = self.selecionado.is_some();
            estilo::cartao(linha, linha.toca(m), alvo);
            if alvo || (esperando && linha.toca(m)) {
                draw_rectangle_lines(linha.x, linha.y, linha.w, linha.h, 3.0 * k, estilo::OURO);
            }
            // Slot box with its key.
            let caixa = r(lx + 10.0, y + 8.0, 61.0, 61.0);
            estilo::painel(caixa);
            let tecla = r(lx + 4.0, y + 3.0, 24.0, 20.0);
            draw_rectangle(tecla.x, tecla.y, tecla.w, tecla.h, Color::new(0.08, 0.06, 0.05, 0.9));
            estilo::texto_centro_forte(tecla.center().x, tecla.center().y + 5.0 * k, TECLAS[i], 13, estilo::OURO);
            if esp.item_id == 0 {
                estilo::texto_centro_forte(caixa.center().x, caixa.center().y + 10.0 * k, "+", 30, estilo::SUAVE);
                estilo::texto(
                    p.x + (lx + 86.0) * k,
                    linha.center().y + 6.0 * k,
                    if alvo { "Pick a potion on the right ›" } else { "Empty · tap to choose a potion" },
                    16,
                    if alvo { estilo::OURO } else { estilo::SUAVE },
                );
                if clique && linha.toca(m) {
                    mudou |= self.escolhe_espaco(barra, i);
                }
                continue;
            }
            let q = barra::quantidade(slots, esp.item_id);
            crate::bolsa::icone_do_item(r(lx + 15.0, y + 13.0, 51.0, 51.0), esp.item_id, if q > 0 { 1.0 } else { 0.35 });
            // Name and how many are left.
            let tx = p.x + (lx + 84.0) * k;
            estilo::texto_ajustado(&nome(esp.item_id), tx, linha.y + 32.0 * k, 158.0 * k, 16, estilo::TEXTO);
            estilo::texto(
                tx,
                linha.y + 56.0 * k,
                &format!("×{q} in bag"),
                13,
                if q > 0 { estilo::SUAVE } else { estilo::VERMELHO },
            );
            // AUTO switch.
            let b_auto = r(lx + 250.0, y + 16.0, 100.0, 46.0);
            let cor_auto = if esp.auto { estilo::AUTO } else { Color::new(0.20, 0.17, 0.15, 1.0) };
            draw_rectangle(b_auto.x, b_auto.y, b_auto.w, b_auto.h, estilo::alfa(cor_auto, if esp.auto { 0.85 } else { 1.0 }));
            draw_rectangle_lines(b_auto.x, b_auto.y, b_auto.w, b_auto.h, 2.0 * k, if esp.auto { estilo::AUTO } else { estilo::SUAVE });
            let bolinha = if esp.auto { b_auto.x + b_auto.w - 17.0 * k } else { b_auto.x + 17.0 * k };
            draw_circle(bolinha, b_auto.center().y, 11.0 * k, if esp.auto { WHITE } else { estilo::SUAVE });
            let tx_auto = if esp.auto { b_auto.x + 40.0 * k } else { b_auto.x + 62.0 * k };
            estilo::texto_centro_forte(
                tx_auto,
                b_auto.center().y + 6.0 * k,
                if esp.auto { "AUTO" } else { "OFF" },
                14,
                if esp.auto { WHITE } else { estilo::SUAVE },
            );
            if clique && b_auto.toca(m) {
                barra.alterna_auto(i);
                mudou = true;
            }
            // Threshold (health/mana) or the rule (buffs).
            let cat = barra::categoria(esp.item_id);
            let sx = lx + 362.0;
            if let Some(c) = cat {
                if c.limiar_padrao().is_some() {
                    estilo::texto(p.x + sx * k, linha.y + 16.0 * k, "Drink below", 11, estilo::SUAVE);
                    let menos = r(sx, y + 22.0, 46.0, 46.0);
                    let mais = r(sx + 112.0, y + 22.0, 46.0, 46.0);
                    crate::foco::marca(
                        crate::foco::chave::POCAO_LIMIAR,
                        Rect::new(menos.x, menos.y, mais.x + mais.w - menos.x, menos.h),
                    );
                    estilo::botao(menos, "−", estilo::estado_de(menos, esp.limiar <= 5, false), false);
                    estilo::botao(mais, "+", estilo::estado_de(mais, esp.limiar >= 95, false), false);
                    estilo::texto_centro_forte(
                        p.x + (sx + 79.0) * k,
                        menos.center().y + 8.0 * k,
                        &format!("{}%", esp.limiar),
                        20,
                        estilo::OURO,
                    );
                    if clique && menos.toca(m) {
                        barra.ajustar_limiar(i, -5);
                        mudou = true;
                        self.mexeu_no_limiar = true;
                    }
                    if clique && mais.toca(m) {
                        barra.ajustar_limiar(i, 5);
                        mudou = true;
                        self.mexeu_no_limiar = true;
                    }
                } else {
                    estilo::texto(p.x + sx * k, linha.y + 34.0 * k, "AUTO drinks it when", 12, estilo::SUAVE);
                    estilo::texto_ajustado(c.regra(), p.x + sx * k, linha.y + 54.0 * k, 158.0 * k, 13, estilo::TEXTO);
                }
            }
            // Where to get more · clear the slot.
            let lupa = r(lx + 530.0, y + 16.0, 30.0, 46.0);
            if crate::onde_obter::botao(Rect::new(lupa.x - 8.0 * k, lupa.y, 38.0 * k, lupa.h)) {
                self.onde_obter = Some(esp.item_id);
            }
            let limpar = r(lx + 566.0, y + 16.0, 34.0, 46.0);
            estilo::botao(limpar, "×", estilo::estado_de(limpar, false, false), false);
            if clique && limpar.toca(m) {
                barra.limpar(i);
                mudou = true;
            } else if clique && caixa.toca(m) {
                mudou |= self.escolhe_espaco(barra, i);
            }
        }

        // ── consumables in the bag ──
        let (bx, bw) = (632.0, 252.0);
        estilo::texto_forte(p.x + bx * k, p.y + 98.0 * k, "Your bag", 16, estilo::OURO);
        let lista = consumiveis(slots);
        if lista.is_empty() {
            estilo::texto(p.x + bx * k, p.y + 128.0 * k, "No potions in your bag.", 14, estilo::SUAVE);
        }
        let area = r(bx, 108.0, bw, 300.0);
        let passo = 58.0 * k;
        let total = lista.len() as f32 * passo;
        let toque = self.rolagem.quadro(area, total, passo);
        let tocou = |rr: Rect| toque.is_some_and(|c| rr.toca(c) && area.toca(c));
        crate::rolagem::recortar(Some(area));
        for (n, (id, q)) in lista.iter().enumerate() {
            let it = Rect::new(area.x, area.y + n as f32 * passo - self.rolagem.pos, area.w - 10.0 * k, passo - 6.0 * k);
            if it.y + it.h < area.y || it.y > area.y + area.h {
                continue;
            }
            let marcado = self.selecionado == Some(*id);
            estilo::cartao(it, it.toca(m), marcado);
            if marcado || (self.espaco_alvo.is_some() && it.toca(m)) {
                draw_rectangle_lines(it.x, it.y, it.w, it.h, 3.0 * k, estilo::OURO);
            }
            crate::bolsa::icone_do_item(Rect::new(it.x + 5.0 * k, it.y + 5.0 * k, 42.0 * k, 42.0 * k), *id, 1.0);
            estilo::texto_ajustado(&nome(*id), it.x + 54.0 * k, it.y + 24.0 * k, it.w - 104.0 * k, 14, estilo::TEXTO);
            estilo::texto(it.x + 54.0 * k, it.y + 43.0 * k, &format!("×{q}"), 13, estilo::SUAVE);
            let lupa = Rect::new(it.x + it.w - 46.0 * k, it.y + 4.0 * k, 42.0 * k, it.h - 8.0 * k);
            let _ = crate::onde_obter::botao(lupa);
            if tocou(lupa) {
                self.onde_obter = Some(*id);
            } else if tocou(it) {
                mudou |= self.escolhe_item(barra, *id);
            }
        }
        crate::rolagem::recortar(None);
        self.rolagem.desenha(area, total);
        mudou
    }
}

const BASE_W: f32 = 900.0;
const BASE_H: f32 = 420.0;

/// `MMO_PREVIA_BARRA=1`: the configurator with a filled bar, no server.
#[cfg(debug_assertions)]
pub async fn previa() {
    use shared::constants::item_id as it;
    let saida = std::env::var("MMO_PREVIA_SAIDA").unwrap_or_else(|_| "/tmp/tempest-barra-preview".into());
    std::fs::create_dir_all(&saida).unwrap();
    next_frame().await;
    let rt = render_target(screen_width() as u32, screen_height() as u32);
    let slots: Vec<InventorySlot> = [(it::HEALTH_POTION, 34), (it::GREATER_HEAL, 5), (it::MANA_POTION, 12), (it::XP_POTION, 2), (it::SORTE_POTION, 1)]
        .iter()
        .map(|&(item_id, qty)| InventorySlot { item_id, qty, instance: None })
        .collect();
    let mut b = Barra::default();
    b.atribuir(0, it::HEALTH_POTION);
    b.alterna_auto(0);
    b.atribuir(1, it::MANA_POTION);
    b.atribuir(2, it::XP_POTION);
    let nome = |id: u16| match id {
        x if x == it::HEALTH_POTION => "Health Potion".to_string(),
        x if x == it::GREATER_HEAL => "Greater Health Potion".to_string(),
        x if x == it::MANA_POTION => "Mana Potion".to_string(),
        x if x == it::XP_POTION => "Experience Potion".to_string(),
        _ => "Luck Potion".to_string(),
    };
    for (arquivo, alvo) in [("barra.png", None), ("barra-escolhendo.png", Some(3))] {
        let mut c = ConfigBarra::default();
        c.abrir(alvo);
        for _ in 0..3 {
            set_camera(&Camera2D {
                render_target: Some(rt.clone()),
                ..Camera2D::from_display_rect(Rect::new(0.0, 0.0, screen_width(), screen_height()))
            });
            clear_background(Color::new(0.10, 0.14, 0.12, 1.0));
            c.desenha(&mut b, &slots, &nome);
            unsafe { get_internal_gl().flush() };
            rt.texture.get_texture_data().export_png(&format!("{saida}/{arquivo}"));
            set_default_camera();
            next_frame().await;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use shared::constants::item_id as it;
    use shared::protocol::EspacoDaBarra;

    fn slot(id: u16, qty: u32) -> InventorySlot {
        InventorySlot {
            item_id: id,
            qty,
            instance: None,
        }
    }

    #[test]
    fn lista_so_consumiveis_somados() {
        let slots = vec![
            slot(it::SORTE_POTION, 2),
            slot(it::STEEL, 9),
            slot(it::SORTE_POTION, 3),
            slot(it::HEALTH_POTION, 1),
        ];
        assert_eq!(
            consumiveis(&slots),
            vec![(it::HEALTH_POTION, 1), (it::SORTE_POTION, 5)]
        );
    }

    #[test]
    fn item_depois_espaco_ou_espaco_depois_item_atribui_e_limpar_esvazia() {
        let mut b = Barra::default();
        let mut c = ConfigBarra::default();
        c.abrir(None);
        assert!(!c.escolhe_item(&mut b, it::FORTUNA_POTION));
        assert!(c.escolhe_espaco(&mut b, 2));
        assert_eq!(b.espacos[2].item_id, it::FORTUNA_POTION);
        assert!(!c.escolhe_espaco(&mut b, 3));
        assert!(c.escolhe_item(&mut b, it::GREATER_MANA));
        assert_eq!(b.espacos[3].item_id, it::GREATER_MANA);
        assert_eq!(b.espacos[3].limiar, 40, "limiar padrao da categoria");
        b.limpar(3);
        assert_eq!(b.espacos[3], EspacoDaBarra::default());
        // Opening from an empty slot leaves it already chosen.
        c.abrir(Some(1));
        assert!(c.escolhe_item(&mut b, it::HEALTH_POTION));
        assert_eq!(b.espacos[1].item_id, it::HEALTH_POTION);
    }
}
