//! O painel da ILHA MÁGICA (`shared::magica`).
//!
//! Duas telas na mesma janela, porque são duas perguntas diferentes:
//!
//! - **Fora** da ilha: "quantos passes eu tenho e quanto tempo compro com
//!   eles?" — e o botão de ir.
//! - **Dentro**: "quanto falta e o que esta ilhota me paga?" — e o botão de
//!   sair antes da hora.
//!
//! Quem decide tudo é o servidor. Aqui só se desenha o estado que ele mandou
//! e se devolve o pedido que o dedo encostou.

use macroquad::prelude::*;
use shared::magica::{AvisoMagica, Bonus, PedidoMagica};

use crate::hud_estilo as estilo;
use crate::ui;

const OURO: Color = Color::new(0.93, 0.76, 0.33, 1.0);
const VERDE: Color = Color::new(0.55, 0.85, 0.50, 1.0);
const SUAVE: Color = Color::new(0.72, 0.74, 0.80, 1.0);

#[derive(Debug, Clone, Default)]
pub struct Estado {
    pub passes: u32,
    pub fim_unix: i64,
    pub dentro: bool,
    pub bonus: u8,
}

#[derive(Default)]
pub struct MagicaUi {
    estado: Option<Estado>,
    aberto: bool,
    /// Quantas entradas o jogador escolheu gastar (1..3).
    entradas: u8,
    aviso: Option<(String, f64)>,
}

impl MagicaUi {
    pub fn aberto(&self) -> bool {
        self.aberto
    }

    pub fn fechar(&mut self) {
        self.aberto = false;
    }

    /// O que o servidor mandou. `Estado` abre o painel só quando ele foi
    /// PEDIDO; dentro da ilha ele chega sozinho (a cada troca de ilhota) e aí
    /// serve só pro HUD — abrir um painel por cima de quem está lutando seria
    /// o melhor jeito de matar o jogador.
    pub fn recebe(&mut self, aviso: AvisoMagica, agora: f64) {
        match aviso {
            AvisoMagica::Estado {
                passes,
                fim_unix,
                dentro,
                bonus,
            } => {
                let e = Estado {
                    passes,
                    fim_unix,
                    dentro,
                    bonus,
                };
                if !dentro && self.estado.is_none() {
                    self.aberto = true;
                }
                self.entradas = self.entradas.clamp(1, 3);
                self.estado = Some(e);
            }
            AvisoMagica::Recusa(t) => self.aviso = Some((t, agora)),
        }
    }

    /// Pedido ao abrir pelo menu.
    pub fn abrir(&mut self) {
        self.aberto = true;
    }

    /// Quanto falta, em segundos, com o relógio do cliente.
    pub fn resta(&self, agora_unix: i64) -> i64 {
        self.estado
            .as_ref()
            .map_or(0, |e| shared::magica::resta(e.fim_unix, agora_unix))
    }

    /// Está dentro da ilha com tempo valendo?
    pub fn dentro(&self, agora_unix: i64) -> bool {
        self.estado.as_ref().is_some_and(|e| e.dentro) && self.resta(agora_unix) > 0
    }

    /// O bônus da ilhota de agora.
    pub fn bonus(&self) -> Option<Bonus> {
        self.estado.as_ref().and_then(|e| Bonus::do_indice(e.bonus))
    }

    /// A tarja do HUD, dentro da ilha: o relógio e a ilhota.
    ///
    /// Fica no HUD e não no painel porque é informação que muda a decisão
    /// enquanto se joga — "faltam 4 minutos e eu estou na ilhota errada" é
    /// exatamente o que o jogador precisa saber sem abrir nada.
    pub fn desenha_hud(&self, agora_unix: i64) {
        if !self.dentro(agora_unix) {
            return;
        }
        let resta = self.resta(agora_unix);
        let s = crate::hud_layout::tela_segura();
        let f = estilo::fator_texto();
        let texto = format!("Ilha Mágica · {}:{:02}", resta / 60, resta % 60);
        let sub = match self.bonus() {
            Some(b) => format!("{} ×{:.2}", b.nome(), b.multiplicador()),
            None => "Ponte — sem bônus".to_string(),
        };
        let w = estilo::medir(&texto, 18).max(estilo::medir(&sub, 14)) + 24.0 * f;
        let r = Rect::new(s.x + (s.w - w) * 0.5, s.y + 6.0 * f, w, 48.0 * f);
        estilo::painel(r);
        // Vermelho no último minuto: a cor avisa antes de o número ser lido.
        let cor = if resta <= 60 {
            Color::new(0.95, 0.45, 0.40, 1.0)
        } else {
            OURO
        };
        estilo::texto_centro(r.x + r.w * 0.5, r.y + 22.0 * f, &texto, 18, cor);
        estilo::texto_centro(
            r.x + r.w * 0.5,
            r.y + 40.0 * f,
            &sub,
            14,
            if self.bonus().is_some() { VERDE } else { SUAVE },
        );
    }

    pub fn desenha(&mut self, agora: f64, agora_unix: i64) -> Option<PedidoMagica> {
        if !self.aberto {
            return None;
        }
        estilo::no_painel(estilo::escala_do_painel(480.0, 430.0), || {
            self.desenha_na_escala(agora, agora_unix)
        })
    }

    fn desenha_na_escala(&mut self, agora: f64, agora_unix: i64) -> Option<PedidoMagica> {
        let e = self.estado.clone().unwrap_or_default();
        let r = ui::painel(480.0, 430.0, "Ilha Mágica");
        let mut pedido = None;
        let mut y = r.y + 10.0;

        ui::texto(
            r.x,
            y,
            "Sete ilhotas ligadas por pontes. Cada uma paga um bônus.",
            15,
            SUAVE,
        );
        y += 22.0;
        ui::texto(
            r.x,
            y,
            "PvP é aberto lá dentro; morrer devolve você à chegada.",
            15,
            SUAVE,
        );
        y += 34.0;

        // As sete, com o que cada uma dá. É a tabela que responde "vale a
        // pena?" antes de o passe ser gasto — depois de gasto é tarde.
        for i in shared::magica::ilhotas() {
            let aqui = e.dentro && Bonus::do_indice(e.bonus) == Some(i.bonus);
            let cor = if aqui { VERDE } else { estilo::TEXTO };
            ui::texto(r.x + 8.0, y, i.bonus.nome(), 15, cor);
            let v = format!("×{:.2}", i.bonus.multiplicador());
            ui::texto(r.x + r.w - estilo::medir(&v, 15) - 8.0, y, &v, 15, cor);
            if aqui {
                ui::texto(r.x + r.w * 0.62, y, "você está aqui", 13, VERDE);
            }
            y += 21.0;
        }
        y += 10.0;
        estilo::separador(r.x, y, r.w);
        y += 18.0;

        let resta = shared::magica::resta(e.fim_unix, agora_unix);
        if e.dentro || resta > 0 {
            ui::texto(
                r.x,
                y,
                &format!("Tempo restante: {}:{:02}", resta / 60, resta % 60),
                17,
                if resta > 0 { OURO } else { SUAVE },
            );
            y += 26.0;
        }
        ui::texto(
            r.x,
            y,
            &format!("Passes: {}  ·  cada um vale 30 minutos", e.passes),
            16,
            if e.passes > 0 { estilo::TEXTO } else { SUAVE },
        );
        y += 30.0;

        if e.dentro {
            if ui::botao(Rect::new(r.x, y, r.w, 38.0), "Sair da ilha", true) {
                pedido = Some(PedidoMagica::Sair);
                self.aberto = false;
            }
            ui::texto(
                r.x,
                y + 54.0,
                "Sair não para o relógio: o tempo continua correndo.",
                13,
                SUAVE,
            );
        } else {
            // Escolher 1, 2 ou 3 antes de ir: acumular é decisão do jogador,
            // e gastar três de uma vez sem ter pedido seria roubo.
            let bw = (r.w - 16.0) / 3.0;
            for n in 1u8..=3 {
                let caixa = Rect::new(r.x + (n - 1) as f32 * (bw + 8.0), y, bw, 34.0);
                let pode = e.passes >= n as u32;
                estilo::botao(
                    caixa,
                    &format!("{n} = {}min", 30 * n),
                    estilo::estado_de(caixa, false, self.entradas == n),
                    self.entradas == n,
                );
                if pode && crate::foco::clique() && caixa.contains(Vec2::from(mouse_position())) {
                    self.entradas = n;
                }
            }
            y += 46.0;
            let pode = e.passes >= self.entradas.max(1) as u32;
            let rot = format!("Entrar ({} passe(s))", self.entradas.max(1));
            if ui::botao(Rect::new(r.x, y, r.w, 38.0), &rot, pode) && pode {
                pedido = Some(PedidoMagica::Entrar {
                    entradas: self.entradas.max(1),
                });
            }
            if !pode {
                ui::texto(
                    r.x,
                    y + 54.0,
                    "Sem passe: eles caem de chefes e estão na Loja de TP.",
                    13,
                    SUAVE,
                );
            }
        }

        if ui::botao(
            Rect::new(r.x + r.w - 90.0, r.y + r.h - 34.0, 90.0, 32.0),
            "Fechar",
            true,
        ) {
            self.aberto = false;
        }
        if let Some((t, quando)) = &self.aviso {
            if agora - quando < 5.0 {
                ui::texto_centro(r.x + r.w * 0.5, r.y + r.h - 44.0, t, 15, OURO);
            }
        }
        pedido
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    /// O painel abre quando o jogador PEDE e não quando ele está jogando.
    ///
    /// Dentro da ilha o servidor manda `Estado` a cada troca de ilhota. Se
    /// isso abrisse o painel, o jogador atravessaria uma ponte no meio de uma
    /// briga de PvP e levaria uma janela na cara — que é o melhor jeito de
    /// morrer sem entender por quê.
    #[test]
    fn o_estado_de_dentro_nao_abre_o_painel() {
        let mut ui = MagicaUi::default();
        ui.recebe(
            AvisoMagica::Estado {
                passes: 0,
                fim_unix: 1_000,
                dentro: true,
                bonus: Bonus::Xp.indice(),
            },
            0.0,
        );
        assert!(!ui.aberto(), "estado de dentro abriu o painel");
        assert_eq!(ui.bonus(), Some(Bonus::Xp));
        assert!(ui.dentro(900), "com tempo sobrando ele está dentro");
        assert!(!ui.dentro(1_001), "tempo vencido não conta como dentro");
    }

    /// FORA, o primeiro estado abre — é a resposta ao clique do menu.
    #[test]
    fn o_primeiro_estado_de_fora_abre_o_painel() {
        let mut ui = MagicaUi::default();
        ui.recebe(
            AvisoMagica::Estado {
                passes: 2,
                fim_unix: 0,
                dentro: false,
                bonus: 255,
            },
            0.0,
        );
        assert!(ui.aberto());
        assert_eq!(ui.bonus(), None, "255 é 'nenhuma ilhota'");
        assert_eq!(ui.resta(0), 0);
    }
}
