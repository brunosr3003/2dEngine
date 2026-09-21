//! O ESTALEIRO (docs/MAR_ABERTO.md): o painel do Carpinteiro Naval.
//!
//! Casco, reparo e os dois eixos de melhoria. Quem decide tudo e' o servidor
//! — aqui so' se desenha o que ele mandou, e o que ele mandou sai de
//! `shared::barcos`, que e' conta PURA. Painel e cobranca leem a mesma
//! funcao: nao ha' como o botao prometer um preco e o servidor cobrar outro.

use macroquad::prelude::*;
use shared::mar::{AvisoBarco, PedidoBarco};

use crate::hud_estilo as estilo;

#[derive(Debug, Default)]
pub struct EstaleiroUi {
    estado: Option<AvisoBarco>,
    pub aberto: bool,
}

impl EstaleiroUi {
    /// O servidor mandou o estado do barco.
    pub fn receber(&mut self, aviso: AvisoBarco) {
        if matches!(aviso, AvisoBarco::Estaleiro { .. }) {
            self.estado = Some(aviso);
        }
    }

    pub fn abrir(&mut self) {
        if self.estado.is_some() {
            self.aberto = true;
        }
    }

    pub fn fechar(&mut self) {
        self.aberto = false;
    }

    pub fn pega_o_mouse(&self) -> bool {
        self.aberto
    }

    /// `nome` resolve o item pelo catalogo da bolsa: os nomes vem do banco,
    /// entao nao ha' uma segunda lista aqui pra divergir dela.
    pub fn desenha(&mut self, nome: &dyn Fn(u16) -> String) -> Option<PedidoBarco> {
        if !self.aberto {
            return None;
        }
        estilo::no_painel(estilo::escala_do_painel(600.0, 430.0), || {
            self.na_escala(nome)
        })
    }

    fn na_escala(&mut self, nome_do: &dyn Fn(u16) -> String) -> Option<PedidoBarco> {
        let Some(AvisoBarco::Estaleiro {
            item,
            casco,
            casco_max,
            melhorias,
            travessias,
            afundou,
            reparo,
            custos,
        }) = self.estado.clone()
        else {
            return None;
        };
        let f = estilo::fator_texto();
        let seguro = crate::hud_layout::tela_segura();
        let w = (600.0 * f).min(seguro.w - 16.0);
        let h = (430.0 * f).min(seguro.h - 16.0);
        let p = Rect::new(
            seguro.center().x - w * 0.5,
            seguro.center().y - h * 0.5,
            w,
            h,
        );
        crate::hud_layout::escurece(0.5);
        estilo::painel_destaque(p, estilo::ACENTO);
        let m = Vec2::from(mouse_position());
        let clicou = crate::foco::clique();
        let x0 = p.x + 20.0 * f;
        let mut pedido = None;

        let nome = shared::barcos::nome_do_item(item).unwrap_or_else(|| "Barco".into());
        estilo::texto_forte(x0, p.y + 36.0 * f, "ESTALEIRO", 20, estilo::OURO);
        estilo::texto(
            x0,
            p.y + 60.0 * f,
            &format!("{nome} · {travessias} travessias · {afundou} naufrágios"),
            14,
            estilo::SUAVE,
        );
        let fechar = Rect::new(p.x + p.w - 48.0 * f, p.y + 8.0 * f, 40.0 * f, 40.0 * f);
        estilo::texto_centro(
            fechar.center().x,
            fechar.center().y + 7.0 * f,
            "X",
            18,
            estilo::TEXTO,
        );

        // ── casco ──
        let barra = Rect::new(x0, p.y + 78.0 * f, p.w - 40.0 * f, 22.0 * f);
        let frac = casco as f32 / casco_max.max(1) as f32;
        // Vermelho abaixo de 30%: e' o mesmo limiar em que o servidor avisa
        // "casco critico", e o olho tem que concordar com o chat.
        let cor = if frac < 0.3 {
            estilo::VERMELHO
        } else {
            estilo::VERDE
        };
        estilo::barra(barra, frac, frac, cor, None);
        estilo::texto_centro_forte(
            barra.center().x,
            barra.center().y + 6.0 * f,
            &format!("{casco} / {casco_max}"),
            15,
            estilo::TEXTO,
        );

        // ── reparo ──
        let mut y = p.y + 116.0 * f;
        let (cobre, madeira) = reparo;
        if casco < casco_max {
            estilo::texto(
                x0,
                y + 16.0 * f,
                &format!("Conserto completo: {cobre} cobre e {madeira} de madeira."),
                14,
                estilo::SUAVE,
            );
            let b1 = Rect::new(x0, y + 26.0 * f, 170.0 * f, 38.0 * f);
            estilo::botao(b1, "Consertar", estilo::estado_de(b1, false, false), true);
            if clicou && b1.contains(m) {
                pedido = Some(PedidoBarco::Reparar { pagando: true });
            }
            // O PISO DE GRACA fica ao lado, e o rotulo diz que e' cortesia:
            // ninguem pode achar que pagou por ele.
            let b2 = Rect::new(x0 + 182.0 * f, y + 26.0 * f, 210.0 * f, 38.0 * f);
            estilo::botao(
                b2,
                "Calafetar (cortesia)",
                estilo::estado_de(b2, false, false),
                false,
            );
            if clicou && b2.contains(m) {
                pedido = Some(PedidoBarco::Reparar { pagando: false });
            }
        } else {
            estilo::texto(x0, y + 16.0 * f, "O casco está inteiro.", 14, estilo::SUAVE);
        }

        // ── melhorias ──
        y += 82.0 * f;
        estilo::texto(x0, y, "MELHORIAS", 13, estilo::SUAVE);
        y += 16.0 * f;
        for (i, nome_eixo) in shared::barcos::eixo::NOMES.iter().enumerate() {
            let r = Rect::new(x0, y, p.w - 40.0 * f, 52.0 * f);
            estilo::cartao(r, false, false);
            let eixo = shared::barcos::eixo::EIXOS[i];
            let n = melhorias.get(eixo).copied().unwrap_or(0);
            estilo::texto_forte(
                r.x + 14.0 * f,
                r.y + 22.0 * f,
                &format!("{nome_eixo}  +{n}"),
                16,
                estilo::TEXTO,
            );
            let custo = custos.get(i).cloned().unwrap_or_default();
            let txt = if custo.is_empty() {
                "no máximo".to_string()
            } else {
                custo
                    .iter()
                    .map(|(id, q)| format!("{q}× {}", nome_do(*id)))
                    .collect::<Vec<_>>()
                    .join(", ")
            };
            estilo::texto(r.x + 14.0 * f, r.y + 42.0 * f, &txt, 12, estilo::SUAVE);
            if !custo.is_empty() {
                let b = Rect::new(
                    r.x + r.w - 130.0 * f,
                    r.y + (r.h - 36.0 * f) * 0.5,
                    118.0 * f,
                    36.0 * f,
                );
                estilo::botao(b, "Melhorar", estilo::estado_de(b, false, false), true);
                if clicou && b.contains(m) {
                    pedido = Some(PedidoBarco::Melhorar { eixo: eixo as u8 });
                }
            }
            y += 58.0 * f;
        }

        if pedido.is_some() || (clicou && (fechar.contains(m) || !p.contains(m))) {
            self.fechar();
        }
        pedido
    }
}

impl EstaleiroUi {
    /// (tem canhao, esta' pronto). O dado sai do ultimo `Estaleiro` que o
    /// servidor mandou — nao ha' uma segunda contabilidade no cliente.
    pub fn canhao(&self) -> Option<u8> {
        match &self.estado {
            Some(AvisoBarco::Estaleiro { melhorias, .. }) => {
                Some(melhorias[shared::barcos::eixo::CANHAO])
            }
            _ => None,
        }
    }
}
