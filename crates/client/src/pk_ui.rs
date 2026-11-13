//! The PK panel: opened from the PK card on the HUD. Picks one of the four
//! modes (`shared::pk`), shows the PK points with what they cost on death and
//! how they fade, and pardons a point for gold behind a confirmation. The
//! server decides everything; this only shows and asks.

#[allow(unused_imports)]
use crate::ui::Toque as _;
use macroquad::prelude::*;
use shared::pk;
use shared::protocol::ClientMessage;

use crate::hud_estilo as estilo;

#[derive(Default)]
pub struct PkUi {
    pub aberto: bool,
    confirmar_perdao: bool,
}

/// Colour of a PK state, the same on the card and on nameplates: red name
/// first, then the purple every hostile mode always had, then calm teal.
pub fn cor(pk: shared::PkState) -> Color {
    if pk.pontos > 0 {
        VERMELHO_PK
    } else if pk::hostil(pk.modo) || pk.hostil {
        ROXO_PK
    } else {
        estilo::AUTO
    }
}

pub const VERMELHO_PK: Color = Color::new(1.0, 0.30, 0.28, 1.0);
pub const ROXO_PK: Color = Color::new(0.76, 0.35, 1.0, 1.0);

/// The mode shown: `modo`, or Free for an older server that only sent `hostil`.
pub fn modo_de(pk: shared::PkState) -> u8 {
    if pk.modo == pk::PACIFICO && pk.hostil {
        pk::LIVRE
    } else {
        pk::modo_valido(pk.modo)
    }
}

/// Can I start a fight with this player? The server's rule (`pk::pode_atacar`)
/// with what the client knows: same party from the party list, same clan from
/// the nameplate tag. Retaliation is the server's to allow.
pub fn posso_atacar(eu: &shared::EntityMeta, ele: &shared::EntityMeta, mesmo_grupo: bool) -> bool {
    let mesmo_cla = !eu.cla.is_empty() && eu.cla == ele.cla;
    let lado = |m: &shared::EntityMeta, cla: i64| pk::Lado {
        modo: modo_de(m.pk),
        pontos: m.pk.pontos,
        grupo: mesmo_grupo.then_some(1),
        cla,
    };
    pk::pode_atacar(&lado(eu, 1), &lado(ele, if mesmo_cla { 1 } else { 2 }), false)
}

/// "+15% XP lost on death" for `pontos` points (empty with none).
pub fn texto_da_perda(pontos: u32) -> String {
    let (_, extra) = pk::fracao_perda(pontos);
    if extra <= 0.0 {
        String::new()
    } else {
        format!("+{}% XP lost on death (not recoverable)", (extra * 100.0).round() as u32)
    }
}

impl PkUi {
    pub fn abrir(&mut self) {
        self.aberto = true;
        self.confirmar_perdao = false;
    }

    pub fn desenha(&mut self, pk_estado: shared::PkState, nivel: u32, ouro: u64) -> Option<ClientMessage> {
        if !self.aberto {
            return None;
        }
        estilo::no_painel(estilo::escala_do_painel(520.0, 470.0), || self.desenha_na_escala(pk_estado, nivel, ouro))
    }

    fn desenha_na_escala(&mut self, pk_estado: shared::PkState, nivel: u32, ouro: u64) -> Option<ClientMessage> {
        crate::hud_layout::escurece(0.5);
        let f = estilo::fator_texto();
        let t = crate::hud_layout::tela_segura();
        let (w, h) = ((520.0 * f).min(t.w - 16.0), (470.0 * f).min(t.h - 16.0));
        let p = Rect::new(t.center().x - w * 0.5, t.center().y - h * 0.5, w, h);
        estilo::painel(p);
        estilo::texto(p.x + 18.0 * f, p.y + 34.0 * f, "PK mode", 22, estilo::OURO);
        if crate::ui::botao(Rect::new(p.x + p.w - 48.0 * f, p.y + 10.0 * f, 38.0 * f, 34.0 * f), "x", true) {
            self.aberto = false;
            return None;
        }
        let atual = modo_de(pk_estado);
        let mut saida = None;
        if self.confirmar_perdao {
            let custo = pk::custo_perdao(nivel);
            estilo::texto_ajustado(
                &format!("Pay {} gold to remove 1 PK point?", milhar(custo)),
                p.x + 24.0 * f,
                p.y + 110.0 * f,
                p.w - 48.0 * f,
                18,
                estilo::TEXTO,
            );
            let bw = 150.0 * f;
            let by = p.y + p.h - 70.0 * f;
            if crate::ui::botao(Rect::new(p.center().x - bw - 8.0, by, bw, 44.0 * f), "Cancel", true) {
                self.confirmar_perdao = false;
            }
            if crate::ui::botao(Rect::new(p.center().x + 8.0, by, bw, 44.0 * f), "Pay", ouro >= custo) {
                self.confirmar_perdao = false;
                saida = Some(ClientMessage::PerdoarPk);
            }
            return saida;
        }
        // The four modes.
        let mut y = p.y + 52.0 * f;
        let rh = 58.0 * f;
        let m = Vec2::from(mouse_position());
        for modo in pk::MODOS {
            let r = Rect::new(p.x + 18.0 * f, y, p.w - 36.0 * f, rh - 6.0 * f);
            let ativo = modo == atual;
            let livre = !pk_estado.travado;
            estilo::cartao(r, livre && r.toca(m), ativo);
            let c = if modo == pk::PACIFICO { estilo::AUTO } else { ROXO_PK };
            draw_circle(r.x + 16.0 * f, r.center().y, 6.0 * f, if ativo { c } else { estilo::alfa(c, 0.35) });
            estilo::texto_forte(r.x + 32.0 * f, r.y + 22.0 * f, pk::nome(modo), 17, if ativo { c } else { estilo::TEXTO });
            estilo::texto_ajustado(pk::explicacao(modo), r.x + 32.0 * f, r.y + 42.0 * f, r.w - 44.0 * f, 13, estilo::SUAVE);
            if livre && !ativo && r.toca(m) && crate::foco::clique() {
                crate::sons::tocar(crate::sons::Som::Clique);
                saida = Some(ClientMessage::DefinirModoPk { modo });
            }
            y += rh;
        }
        if pk_estado.travado {
            estilo::texto_ajustado(
                &format!("PK mode is locked to {} in this area.", pk::nome(atual)),
                p.x + 18.0 * f,
                y + 14.0 * f,
                p.w - 36.0 * f,
                14,
                estilo::OURO,
            );
        }
        // Points.
        let y = p.y + p.h - 112.0 * f;
        estilo::separador(p.x + 18.0 * f, y - 10.0 * f, p.w - 36.0 * f);
        let cor_pontos = if pk_estado.pontos > 0 { VERMELHO_PK } else { estilo::TEXTO };
        estilo::texto_forte(p.x + 18.0 * f, y + 10.0 * f, &format!("PK points: {}", pk_estado.pontos), 17, cor_pontos);
        let linha = if pk_estado.pontos > 0 {
            format!(
                "Red name: anyone may attack you. {}. 1 point fades per hour online.",
                texto_da_perda(pk_estado.pontos)
            )
        } else {
            "Killing a Peaceful player with a clean name gives a PK point.".to_string()
        };
        estilo::texto_ajustado(&linha, p.x + 18.0 * f, y + 32.0 * f, p.w - 36.0 * f, 12, estilo::SUAVE);
        if pk_estado.pontos > 0 {
            let custo = pk::custo_perdao(nivel);
            let r = Rect::new(p.x + p.w - 250.0 * f, y + 48.0 * f, 232.0 * f, 42.0 * f);
            if crate::ui::botao(r, &format!("Pardon 1 point · {} gold", milhar(custo)), ouro >= custo) {
                self.confirmar_perdao = true;
            }
            if ouro < custo {
                estilo::texto_ajustado("Not enough gold.", p.x + 18.0 * f, y + 74.0 * f, r.x - p.x - 26.0 * f, 13, estilo::VERMELHO);
            }
        }
        saida
    }
}

fn milhar(n: u64) -> String {
    shared::idioma::milhar(n)
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn cor_e_modo_do_cartao() {
        let pk = |modo, pontos, hostil| shared::PkState { hostil, pontos, modo, travado: false };
        assert_eq!(cor(pk(pk::PACIFICO, 0, false)), estilo::AUTO);
        assert_eq!(cor(pk(pk::GRUPO, 0, true)), ROXO_PK);
        assert_eq!(cor(pk(pk::PACIFICO, 2, false)), VERMELHO_PK, "red name wins, even with PK off");
        assert_eq!(modo_de(pk(pk::PACIFICO, 0, true)), pk::LIVRE, "old Hostile flag");
        assert_eq!(modo_de(pk(pk::CLA, 0, true)), pk::CLA);
        assert_eq!(texto_da_perda(0), "");
        assert_eq!(texto_da_perda(3), "+15% XP lost on death (not recoverable)");
        assert_eq!(texto_da_perda(20), "+40% XP lost on death (not recoverable)");
    }
}
