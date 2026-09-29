//! Attendance calendar (docs/CALENDARIO.md): the grid of the month's 28
//! prizes, today highlighted, the claimed ones marked and the Claim button.
//! Opens from the icon at the top of the HUD or from the Menu (Adventure ->
//! Attendance) and, once per session, on its own at login when there is something to claim.
//!
//! Everything that counts is the server's: the window only shows state and asks.
use std::collections::HashMap;

use macroquad::prelude::*;
use shared::presenca::{self as pr, AvisoPresenca, EstadoPresenca, PedidoPresenca, Premio};
use shared::protocol::ClientMessage;

use crate::hud_estilo as estilo;

const COLUNAS: usize = 7;

#[derive(Default)]
pub struct PresencaUi {
    pub aberto: bool,
    estado: Option<EstadoPresenca>,
    aba: usize,
    /// Already opened on its own in this app session.
    abriu_no_login: bool,
    /// Ultimo resgate, mostrado no rodape.
    ultimo: Option<String>,
}

/// "Experience Potion x2" / "1,500 gold".
pub fn texto_do_premio(p: &Premio, nomes: &HashMap<u16, String>) -> String {
    if p.item_id == pr::ENERGIA {
        return format!("{} Energy", crate::economia::milhar(p.qtd as u64));
    }
    if p.item_id == pr::OURO {
        return format!("{} gold", crate::economia::milhar(p.qtd as u64));
    }
    let nome = nomes
        .get(&p.item_id)
        .cloned()
        .unwrap_or_else(|| format!("Item {}", p.item_id));
    format!("{nome} ×{}", p.qtd)
}

fn rotulo_curto(p: &Premio) -> String {
    use shared::item_id::*;
    let nome = match p.item_id {
        pr::ENERGIA => "Energy", pr::OURO => "Gold",
        PERGAMINHO_INVOCA_PET => "Pet Summ.", PERGAMINHO_INVOCA_MONTARIA => "Mount Summ.",
        PASSE_MAGICO => "Magic Island", COPPER => "Copper", DARKSTEEL => "Darksteel",
        GLITTERING_POWDER => "Shimmering Dust", _ => "Item",
    };
    format!("{} ×{}", nome, crate::economia::milhar(p.qtd as u64))
}

/// "5h 12m" / "12m".
fn falta(s: i64) -> String {
    let s = s.max(0);
    let (h, m) = (s / 3600, s / 60 % 60);
    if h > 0 {
        format!("{h}h {m:02}m")
    } else {
        format!("{m}m")
    }
}

impl PresencaUi {
    pub fn abrir(&mut self) -> Vec<ClientMessage> {
        self.aberto = true;
        vec![ClientMessage::Presenca {
            pedido: PedidoPresenca::Estado,
        }]
    }

    pub fn fechar(&mut self) {
        self.aberto = false;
    }

    /// There is a prize to claim today (a red badge on the icon).
    pub fn tem_resgate(&self) -> bool {
        self.estado.as_ref().is_some_and(|e| e.tem_resgate())
    }

    /// Chegou aviso do servidor. Devolve texto pro chat.
    pub fn receber(
        &mut self,
        aviso: AvisoPresenca,
        nomes: &HashMap<u16, String>,
    ) -> Option<String> {
        match aviso {
            AvisoPresenca::Estado(e) => {
                if !self.abriu_no_login {
                    self.abriu_no_login = true;
                    if e.tem_resgate() {
                        self.aberto = true;
                    }
                }
                if self.aba >= e.calendarios.len() {
                    self.aba = 0;
                }
                self.estado = Some(e);
                None
            }
            AvisoPresenca::Resgatou {
                dia,
                premios,
                no_correio,
                ..
            } => {
                let lista: Vec<String> =
                    premios.iter().map(|p| texto_do_premio(p, nomes)).collect();
                // Day 0: the prize of a claim left pending (a crash before the save).
                let mut t = if dia == 0 {
                    format!("Presença · prêmio pendente entregue: {}", lista.join(", "))
                } else {
                    format!("Attendance · day {dia}: {}", lista.join(", "))
                };
                if no_correio > 0 {
                    t.push_str(" (bag full: part of it in your Mail)");
                }
                self.ultimo = Some(t.clone());
                Some(t)
            }
            AvisoPresenca::Recusado { texto } => {
                self.ultimo = Some(texto.clone());
                Some(texto)
            }
        }
    }

    /// Desenha e trata o toque. Devolve os pedidos pro servidor.
    pub fn desenha(
        &mut self,
        nomes: &HashMap<u16, String>,
        agora_unix: i64,
        vox: &crate::vox::VoxCache,
        solido: &macroquad::material::Material,
    ) -> Vec<ClientMessage> {
        estilo::no_painel(estilo::escala_do_painel(780.0, 640.0), || {
            self.desenha_na_escala(nomes, agora_unix, vox, solido)
        })
    }

    fn desenha_na_escala(
        &mut self,
        nomes: &HashMap<u16, String>,
        agora_unix: i64,
        vox: &crate::vox::VoxCache,
        solido: &macroquad::material::Material,
    ) -> Vec<ClientMessage> {
        let mut saida = Vec::new();
        if !self.aberto {
            return saida;
        }
        let f = estilo::fator_texto();
        let seguro = crate::hud_layout::tela_segura();
        let w = (780.0 * f).min(seguro.w - 16.0);
        let h = (640.0 * f).min(seguro.h - 16.0);
        let p = Rect::new(
            seguro.center().x - w * 0.5,
            seguro.center().y - h * 0.5,
            w,
            h,
        );
        crate::hud_layout::escurece(0.45);
        estilo::painel(p);
        let m = Vec2::from(mouse_position());
        let clicou = crate::foco::clique();
        let x0 = p.x + 20.0 * f;
        estilo::texto_forte(
            x0,
            p.y + 36.0 * f,
            "Attendance calendar",
            20,
            estilo::OURO,
        );
        let fechar = Rect::new(p.x + p.w - 48.0 * f, p.y + 8.0 * f, 40.0 * f, 40.0 * f);
        estilo::texto_centro(
            fechar.center().x,
            fechar.center().y + 7.0 * f,
            "X",
            18,
            estilo::TEXTO,
        );
        if clicou && fechar.contains(m) {
            self.fechar();
            return saida;
        }
        let Some(estado) = self.estado.clone() else {
            estilo::texto(x0, p.y + 80.0 * f, "Loading…", 15, estilo::SUAVE);
            return saida;
        };
        if estado.calendarios.is_empty() {
            estilo::texto(
                x0,
                p.y + 80.0 * f,
                "No active calendar.",
                15,
                estilo::SUAVE,
            );
            return saida;
        }
        // Tabs: the monthly one and each active event.
        let mut y = p.y + 52.0 * f;
        if estado.calendarios.len() > 1 {
            let mut x = x0;
            for (i, c) in estado.calendarios.iter().enumerate() {
                let tw = estilo::medir(&c.nome, 14) + 28.0 * f;
                let r = Rect::new(x, y, tw, 34.0 * f);
                estilo::cartao(r, r.contains(m), i == self.aba);
                estilo::texto_centro(
                    r.center().x,
                    r.center().y + 5.0 * f,
                    &c.nome,
                    14,
                    if i == self.aba {
                        estilo::OURO
                    } else {
                        estilo::TEXTO
                    },
                );
                if c.pode_hoje {
                    crate::hud::selo(r);
                }
                if clicou && r.contains(m) {
                    self.aba = i;
                }
                x += tw + 8.0 * f;
            }
            y += 42.0 * f;
        }
        let cal = &estado.calendarios[self.aba.min(estado.calendarios.len() - 1)];
        let sub = if cal.fim_unix > 0 {
            format!(
                "{} · ends in {}",
                cal.nome,
                falta(cal.fim_unix - agora_unix)
            )
        } else {
            format!("{} · the month turns over on the 1st at 04:00", cal.nome)
        };
        estilo::texto(x0, y + 18.0 * f, &sub, 13, estilo::SUAVE);
        y += 30.0 * f;

        // The grid: 7 per row.
        let linhas = cal.grade.len().div_ceil(COLUNAS).max(1);
        let vao = 8.0 * f;
        let rodape = 96.0 * f;
        let cw = (p.w - 40.0 * f - vao * (COLUNAS as f32 - 1.0)) / COLUNAS as f32;
        let ch =
            ((p.y + p.h - rodape - y - vao * (linhas as f32 - 1.0)) / linhas as f32).min(cw * 1.15);
        let proximo = cal.resgatados as usize;
        let mut dica: Option<(Rect, String)> = None;
        for (i, dia) in cal.grade.iter().enumerate() {
            let n = i as u8 + 1;
            let r = Rect::new(
                x0 + (i % COLUNAS) as f32 * (cw + vao),
                y + (i / COLUNAS) as f32 * (ch + vao),
                cw,
                ch,
            );
            let sobre = r.contains(m);
            let hoje = i == proximo && cal.pode_hoje;
            let marco = pr::e_marco(n);
            estilo::cartao(r, sobre, hoje);
            if marco {
                estilo::borda_arredondada(r, estilo::RAIO_PEQUENO, 2.0, estilo::OURO);
            }
            if hoje {
                estilo::borda_arredondada(r, estilo::RAIO_PEQUENO, 3.0, estilo::ACENTO);
            }
            estilo::texto_forte(
                r.x + 6.0 * f,
                r.y + 15.0 * f,
                &format!("{n}"),
                12,
                if marco { estilo::OURO } else { estilo::SUAVE },
            );
            let premios: Vec<&Premio> = dia.iter().filter(|p| p.qtd > 0).collect();
            // Every prize is written on the card, including the three on day 28.
            let linha = ((r.h - 23.0 * f) / premios.len().max(1) as f32).min(25.0 * f);
            for (j, pp) in premios.iter().enumerate() {
                let yy = r.y + 22.0 * f + j as f32 * linha;
                let lado = (linha - 3.0 * f).min(22.0 * f);
                let ic = Rect::new(r.x + 4.0 * f, yy, lado, lado);
                if pp.item_id == pr::ENERGIA {
                    estilo::icone_energia(ic.center(), lado * 0.5);
                } else {
                    crate::icones::icone_com_3d(if pp.item_id == pr::OURO { shared::item_id::GOLD } else { pp.item_id }, ic, None, None, Some((vox, solido)));
                }
                let rotulo = rotulo_curto(pp);
                estilo::texto_ajustado(&rotulo, ic.x + lado + 3.0 * f, yy + lado * 0.8,
                    r.w - lado - 13.0 * f, if marco { 12 } else { 11 },
                    if marco { estilo::OURO } else { estilo::TEXTO });
            }
            if (i as u8) < cal.resgatados {
                estilo::ret_arredondado(r, estilo::RAIO_PEQUENO, Color::new(0.0, 0.0, 0.0, 0.55));
                let c = r.center();
                let s = r.w.min(r.h) * 0.22;
                estilo::traco(
                    vec2(c.x - s, c.y),
                    vec2(c.x - s * 0.25, c.y + s * 0.75),
                    4.0,
                    Color::new(0.45, 0.85, 0.52, 1.0),
                );
                estilo::traco(
                    vec2(c.x - s * 0.25, c.y + s * 0.75),
                    vec2(c.x + s, c.y - s * 0.7),
                    4.0,
                    Color::new(0.45, 0.85, 0.52, 1.0),
                );
            }
            if sobre {
                let t: Vec<String> = premios.iter().map(|p| texto_do_premio(p, nomes)).collect();
                dica = Some((
                    r,
                    format!(
                        "Dia {n}{}: {}",
                        if marco { " (marco)" } else { "" },
                        t.join(" + ")
                    ),
                ));
            }
        }

        // Rodape: progresso, contador, ultimo resgate e o botao.
        let yb = p.y + p.h - rodape + 12.0 * f;
        estilo::texto_forte(
            x0,
            yb + 18.0 * f,
            &format!("Claimed {}/{}", cal.resgatados, cal.grade.len()),
            15,
            estilo::TEXTO,
        );
        let contador = if cal.pode_hoje {
            "Today's claim is available".to_string()
        } else {
            format!(
                "Next claim in {}",
                falta(estado.proximo_reset_unix - agora_unix)
            )
        };
        estilo::texto(x0, yb + 40.0 * f, &contador, 13, estilo::SUAVE);
        if let Some(t) = &self.ultimo {
            estilo::texto_ajustado(t, x0, yb + 62.0 * f, p.w * 0.6, 12, estilo::SUAVE);
        }
        let bot = Rect::new(
            p.x + p.w - 20.0 * f - 200.0 * f,
            yb + 4.0 * f,
            200.0 * f,
            54.0 * f,
        );
        let ativo = cal.pode_hoje;
        estilo::cartao(bot, ativo && bot.contains(m), ativo);
        estilo::texto_centro_forte(
            bot.center().x,
            bot.center().y + 7.0 * f,
            if ativo { "Claim" } else { "Claimed" },
            18,
            if ativo { estilo::OURO } else { estilo::SUAVE },
        );
        if ativo && clicou && bot.contains(m) {
            saida.push(ClientMessage::Presenca {
                pedido: PedidoPresenca::Resgatar { calendario: cal.id },
            });
        }
        if let Some((r, t)) = dica {
            estilo::tooltip(r, &t, true);
        }
        saida
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn estado(pode: bool) -> EstadoPresenca {
        shared::presenca::DadosPresenca::default()
            .estado(1_789_000_000, pr::EVENTOS)
            .tap(pode)
    }

    trait Tap {
        fn tap(self, pode: bool) -> Self;
    }
    impl Tap for EstadoPresenca {
        fn tap(mut self, pode: bool) -> Self {
            for c in &mut self.calendarios {
                c.pode_hoje = pode;
            }
            self
        }
    }

    #[test]
    fn abre_sozinha_so_no_primeiro_estado_com_resgate() {
        let nomes = HashMap::new();
        let mut ui = PresencaUi::default();
        ui.receber(AvisoPresenca::Estado(estado(true)), &nomes);
        assert!(ui.aberto && ui.tem_resgate());
        ui.fechar();
        ui.receber(AvisoPresenca::Estado(estado(true)), &nomes);
        assert!(!ui.aberto, "so' uma vez por sessao");

        let mut sem = PresencaUi::default();
        sem.receber(AvisoPresenca::Estado(estado(false)), &nomes);
        assert!(!sem.aberto && !sem.tem_resgate());
    }

    #[test]
    fn texto_do_resgate_diz_premios_e_correio() {
        let mut nomes = HashMap::new();
        nomes.insert(
            shared::item_id::XP_POTION,
            "Experience Potion".to_string(),
        );
        let mut ui = PresencaUi::default();
        let t = ui
            .receber(
                AvisoPresenca::Resgatou {
                    calendario: 0,
                    dia: 7,
                    premios: vec![
                        Premio {
                            item_id: shared::item_id::XP_POTION,
                            qtd: 1,
                        },
                        Premio {
                            item_id: pr::OURO,
                            qtd: 1500,
                        },
                    ],
                    no_correio: 1,
                },
                &nomes,
            )
            .unwrap();
        assert!(
            t.contains("dia 7")
                && t.contains("Poção de Experiência ×1")
                && t.contains("1.500 de ouro")
                && t.contains("Mail")
        );
    }

    #[test]
    fn contador() {
        assert_eq!(falta(5 * 3600 + 12 * 60 + 5), "5h 12m");
        assert_eq!(falta(600), "10m");
    }
}
