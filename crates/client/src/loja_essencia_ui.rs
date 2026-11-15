//! THE ESSENCE SHOP screen (`shared::essencia::OFERTAS`, rebirth phase 3):
//! the offers with their Essence price, the Rebirth Essence in the bag and a
//! "Buy" that asks first. Opens from the Character Sheet's ESSENCE button
//! (after the first rebirth). The server checks the balance and delivers;
//! the answer comes in the chat and the bag updates.

#[allow(unused_imports)]
use crate::ui::Toque as _;
use macroquad::prelude::*;
use shared::essencia::{self as es, Oferta};
use shared::protocol::ClientMessage;

use crate::hud_estilo as estilo;

const LARGURA: f32 = 760.0;
const ALTURA: f32 = 600.0;

#[derive(Default)]
pub struct LojaEssenciaUi {
    pub aberta: bool,
    /// The offer waiting for a "yes".
    confirmar: Option<u8>,
}

impl LojaEssenciaUi {
    pub fn abrir(&mut self) {
        self.aberta = true;
        self.confirmar = None;
    }

    pub fn fechar(&mut self) {
        self.aberta = false;
        self.confirmar = None;
    }

    /// `essencia`: the Rebirth Essence in the bag.
    pub fn desenha(&mut self, essencia: u32) -> Option<ClientMessage> {
        if !self.aberta {
            return None;
        }
        let seguro = crate::hud_layout::tela_segura();
        let escala = estilo::escala_do_painel(LARGURA, ALTURA)
            .min((seguro.w - 16.0) / LARGURA)
            .min((seguro.h - 16.0) / ALTURA);
        estilo::no_painel(escala, || {
            let modal = self.confirmar.and_then(es::oferta);
            self.desenha_painel(essencia, modal.is_none());
            if let Some(o) = modal {
                return self.desenha_confirmacao(o, essencia);
            }
            None
        })
    }

    fn desenha_painel(&mut self, essencia: u32, ativo: bool) {
        let f = estilo::fator_texto();
        let seguro = crate::hud_layout::tela_segura();
        let w = (LARGURA * f).min(seguro.w - 16.0);
        let h = (ALTURA * f).min(seguro.h - 16.0);
        let p = Rect::new(seguro.center().x - w * 0.5, seguro.center().y - h * 0.5, w, h);
        crate::hud_layout::escurece(0.55);
        estilo::painel(p);
        let m = Vec2::from(mouse_position());
        let clique = crate::foco::clique() && ativo;

        estilo::texto_forte(p.x + 20.0 * f, p.y + 36.0 * f, "ESSENCE SHOP", 23, estilo::OURO);
        let fechar = Rect::new(p.x + p.w - 49.0 * f, p.y + 8.0 * f, 40.0 * f, 40.0 * f);
        estilo::botao(fechar, "X", estilo::estado_de(fechar, false, false), false);
        if clique && fechar.toca(m) {
            self.fechar();
            return;
        }
        estilo::texto(
            p.x + 20.0 * f,
            p.y + 64.0 * f,
            &format!("Rebirth Essence: {essencia}"),
            16,
            estilo::VERDE,
        );
        estilo::texto_ajustado(
            "Every rebirth pays Essence. Gear stars are bought at the Forge (Ascend).",
            p.x + 20.0 * f,
            p.y + 86.0 * f,
            p.w - 40.0 * f,
            13,
            estilo::SUAVE,
        );
        let topo = p.y + 100.0 * f;
        let lin_h = ((p.h - 112.0 * f) / es::OFERTAS.len() as f32).min(48.0 * f);
        for (i, o) in es::OFERTAS.iter().enumerate() {
            let r = Rect::new(p.x + 16.0 * f, topo + i as f32 * lin_h, p.w - 32.0 * f, lin_h - 4.0 * f);
            estilo::cartao(r, r.toca(m), false);
            let b = Rect::new(r.x + r.w - 150.0 * f, r.y + 4.0 * f, 142.0 * f, r.h - 8.0 * f);
            let texto_w = b.x - r.x - 24.0 * f;
            estilo::texto_ajustado(o.nome, r.x + 10.0 * f, r.y + r.h * 0.42, texto_w, 15, estilo::TEXTO);
            estilo::texto_ajustado(o.descricao, r.x + 10.0 * f, r.y + r.h * 0.85, texto_w, 11, estilo::SUAVE);
            let paga = essencia >= o.preco;
            let rotulo = format!("Buy · {} Essence", o.preco);
            estilo::botao(b, &rotulo, estilo::estado_de(b, !paga, false), paga);
            if clique && paga && b.toca(m) {
                self.confirmar = Some(o.id);
            }
        }
    }

    /// "Buy …?": the price, the balance after, Cancel / Buy.
    fn desenha_confirmacao(&mut self, o: &Oferta, essencia: u32) -> Option<ClientMessage> {
        let f = estilo::fator_texto();
        let seguro = crate::hud_layout::tela_segura();
        let (w, h) = ((560.0 * f).min(seguro.w - 24.0), (230.0 * f).min(seguro.h - 24.0));
        let r = Rect::new(seguro.center().x - w * 0.5, seguro.center().y - h * 0.5, w, h);
        crate::hud_layout::escurece(0.5);
        estilo::painel(r);
        let x = r.x + 24.0 * f;
        let larg = w - 48.0 * f;
        estilo::texto_forte(x, r.y + 40.0 * f, &format!("Buy {}?", o.nome), 20, estilo::OURO);
        estilo::texto_ajustado(
            &format!("{} Rebirth Essence: you will have {} left.", o.preco, essencia.saturating_sub(o.preco)),
            x,
            r.y + 80.0 * f,
            larg,
            15,
            estilo::TEXTO,
        );
        let bw = (w - 60.0 * f) * 0.5;
        let bh = 46.0 * f;
        let by = r.y + r.h - 22.0 * f - bh;
        let cancelar = Rect::new(x, by, bw, bh);
        let ok = Rect::new(cancelar.x + bw + 12.0 * f, by, bw, bh);
        let paga = essencia >= o.preco;
        estilo::botao(cancelar, "Cancel", estilo::estado_de(cancelar, false, false), false);
        estilo::botao(ok, "Buy", estilo::estado_de(ok, !paga, false), paga);
        if !crate::foco::clique() {
            return None;
        }
        let m = Vec2::from(mouse_position());
        if ok.toca(m) && paga {
            self.confirmar = None;
            return Some(ClientMessage::LojaEssencia { oferta: o.id });
        }
        if cancelar.toca(m) || !r.toca(m) {
            self.confirmar = None;
        }
        None
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn abre_sem_pergunta_pendente() {
        let mut ui = LojaEssenciaUi { aberta: false, confirmar: Some(2) };
        ui.abrir();
        assert!(ui.aberta && ui.confirmar.is_none());
        ui.fechar();
        assert!(!ui.aberta);
    }
}
