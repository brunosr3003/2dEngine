//! "Vendedores" (Menu → Comércio): os vendedores NPC da ilha, so' com "Ir".
//!
//! Decisao do usuario: vendedor NPC NUNCA vende de longe — sem compra remota e
//! sem taxa. O menu so' leva ate' ele: o personagem vai sozinho e a loja abre
//! ao falar com ele. A "Loja" do Menu e' outra coisa: a de cash (TP), em breve.
use macroquad::prelude::*;

use crate::hud_estilo::{self as estilo, u};

const LARGURA: f32 = 520.0;
const LINHA: f32 = 54.0;

#[derive(Default)]
pub struct Lojas {
    pub aberto: bool,
}

impl Lojas {
    pub fn abrir(&mut self) {
        self.aberto = true;
    }

    pub fn fechar(&mut self) {
        self.aberto = false;
    }

    fn escala() -> f32 {
        estilo::escala_do_painel(LARGURA, 420.0)
    }

    fn painel(n: usize) -> Rect {
        let k = Self::escala();
        let h = ((96.0 + LINHA * n.max(1) as f32) * k).min(screen_height() - 60.0);
        let w = (LARGURA * k).min(screen_width() - 40.0);
        Rect::new(
            (screen_width() - w) * 0.5,
            (screen_height() - h) * 0.5,
            w,
            h,
        )
    }

    pub fn pega_mouse(&self) -> bool {
        self.aberto
    }

    /// Desenha e devolve o vendedor escolhido com "Ir": (nome, posicao).
    pub fn desenha(
        &mut self,
        lojas: &[(String, Vec2)],
        eu: Option<Vec2>,
    ) -> Option<(String, Vec2)> {
        if !self.aberto {
            return None;
        }
        estilo::no_painel(Self::escala(), || self.desenha_na_escala(lojas, eu))
    }

    fn desenha_na_escala(
        &mut self,
        lojas: &[(String, Vec2)],
        eu: Option<Vec2>,
    ) -> Option<(String, Vec2)> {
        crate::hud_layout::escurece(0.55);
        let p = Self::painel(lojas.len());
        estilo::painel(p);
        estilo::texto(
            p.x + u(18.0),
            p.y + u(32.0),
            "Vendedores da ilha",
            22,
            estilo::OURO,
        );
        estilo::texto_ajustado(
            "Só se compra falando com o vendedor: \"Ir\" leva você até ele.",
            p.x + u(18.0),
            p.y + u(54.0),
            p.w - u(36.0),
            13,
            estilo::SUAVE,
        );
        if crate::ui::botao(
            Rect::new(p.x + p.w - u(44.0), p.y + u(10.0), u(32.0), u(28.0)),
            "x",
            true,
        ) {
            self.aberto = false;
            return None;
        }
        if lojas.is_empty() {
            estilo::texto(
                p.x + u(18.0),
                p.y + u(92.0),
                "Nenhum vendedor nesta ilha.",
                15,
                estilo::SUAVE,
            );
            return None;
        }
        let mut saida = None;
        for (i, (nome, pos)) in lojas.iter().enumerate() {
            let y = p.y + u(70.0) + i as f32 * u(LINHA);
            if y + u(LINHA) > p.y + p.h {
                break;
            }
            let linha = Rect::new(p.x + u(10.0), y, p.w - u(20.0), u(LINHA) - u(6.0));
            estilo::painel(linha);
            estilo::texto_ajustado(
                nome,
                linha.x + u(12.0),
                linha.y + u(22.0),
                linha.w - u(120.0),
                17,
                estilo::TEXTO,
            );
            let dist = eu.map_or("—".to_string(), |e| format!("{:.0} m", e.distance(*pos)));
            estilo::texto(
                linha.x + u(12.0),
                linha.y + u(40.0),
                &dist,
                13,
                estilo::SUAVE,
            );
            if crate::ui::botao(
                Rect::new(
                    linha.x + linha.w - u(84.0),
                    linha.y + u(9.0),
                    u(72.0),
                    u(30.0),
                ),
                "Ir",
                true,
            ) {
                saida = Some((nome.clone(), *pos));
            }
        }
        if saida.is_some() {
            self.aberto = false;
        }
        saida
    }
}
