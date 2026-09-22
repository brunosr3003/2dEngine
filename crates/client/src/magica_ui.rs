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
const VERMELHO: Color = Color::new(0.95, 0.45, 0.40, 1.0);

/// A cor de cada bônus.
///
/// São sete ilhotas, e ler o nome de cada uma no meio de uma briga de PvP não
/// acontece. A cor é o que o olho separa sem ler — e é a mesma no HUD e na
/// lista do painel, senão seriam dois códigos para a mesma coisa.
fn cor_do_bonus(b: Bonus) -> Color {
    match b {
        Bonus::Xp => Color::new(0.55, 0.78, 0.98, 1.0),
        Bonus::DropDeMob => Color::new(0.72, 0.86, 0.45, 1.0),
        Bonus::Ouro => Color::new(0.97, 0.82, 0.35, 1.0),
        Bonus::DropDeChefe => Color::new(0.95, 0.52, 0.45, 1.0),
        Bonus::Coleta(0) => Color::new(0.76, 0.58, 0.36, 1.0),
        Bonus::Coleta(5) => Color::new(0.68, 0.60, 0.98, 1.0),
        Bonus::Coleta(_) => Color::new(0.72, 0.74, 0.80, 1.0),
    }
}
const VERDE: Color = Color::new(0.55, 0.85, 0.50, 1.0);
const SUAVE: Color = Color::new(0.72, 0.74, 0.80, 1.0);

#[derive(Debug, Clone, Default)]
pub struct Estado {
    pub passes: u32,
    /// Entradas de graça que ainda há hoje (`magica::GRATIS_POR_DIA`).
    pub gratis: u8,
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
                gratis,
                fim_unix,
                dentro,
                bonus,
            } => {
                let e = Estado {
                    passes,
                    gratis,
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
        let urgente = resta <= 60;

        // Largura FIXA. Antes ela saía do texto, então a tarja mudava de
        // tamanho a cada travessia de ponte — o olho via a coisa pular de
        // lugar e o relógio nunca ficava onde se aprendeu a procurar.
        let w = 268.0 * f;
        let h = 62.0 * f;
        let r = Rect::new(s.x + (s.w - w) * 0.5, s.y + 6.0 * f, w, h);
        estilo::painel(r);

        // O RELÓGIO, grande, à esquerda. É o número que decide se vale
        // atravessar ou não.
        let cor = if urgente { VERMELHO } else { OURO };
        let relogio = format!("{}:{:02}", resta / 60, resta % 60);
        estilo::texto_forte(r.x + 14.0 * f, r.y + 30.0 * f, &relogio, 26, cor);

        // A BARRA, por baixo: o relógio em número diz quanto falta, a barra
        // diz quanto falta COMPARADO ao que cabe (1h30). Um vê-se lendo, a
        // outra vê-se de canto de olho no meio de uma briga.
        let bx = r.x + 14.0 * f;
        let bw = w - 28.0 * f;
        let by = r.y + h - 16.0 * f;
        let frac = (resta as f32 / shared::magica::TETO_S as f32).clamp(0.0, 1.0);
        draw_rectangle(bx, by, bw, 5.0 * f, Color::new(1.0, 1.0, 1.0, 0.13));
        draw_rectangle(bx, by, bw * frac, 5.0 * f, cor);

        // A ILHOTA, à direita, NA COR DELA. São sete bônus; cor é o que o
        // olho separa sem ler.
        let (nome, mult, c) = match self.bonus() {
            Some(b) => (b.nome(), format!("×{:.2}", b.multiplicador()), cor_do_bonus(b)),
            None => ("Ponte", "sem bônus".to_string(), SUAVE),
        };
        let x = r.x + w - 14.0 * f;
        let tn = estilo::medir(nome, 15);
        estilo::texto(x - tn, r.y + 24.0 * f, nome, 15, c);
        let tm = estilo::medir(&mult, 17);
        estilo::texto_forte(x - tm, r.y + 44.0 * f, &mult, 17, c);
        // O ponto da cor, colado no nome: um rótulo colorido some no fundo
        // escuro; um disco cheio não.
        draw_circle(x - tn - 9.0 * f, r.y + 19.0 * f, 4.0 * f, c);
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
            // A MESMA COR do HUD, e um ponto antes do nome: é assim que a
            // linha da lista e a tarja lá em cima viram a mesma coisa na
            // cabeça de quem joga.
            let c = cor_do_bonus(i.bonus);
            draw_circle(r.x + 8.0, y - 5.0, 4.0, c);
            ui::texto(r.x + 20.0, y, i.bonus.nome(), 15, if aqui { VERDE } else { c });
            let v = format!("×{:.2}", i.bonus.multiplicador());
            ui::texto(r.x + r.w - estilo::medir(&v, 15) - 8.0, y, &v, 15, c);
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
        // O DE GRAÇA PRIMEIRO, porque é o que ele gasta primeiro — e porque
        // "tenho 3 entradas grátis hoje" é a informação que faz o jogador
        // entrar, não "tenho 0 passes".
        let total = e.passes + e.gratis as u32;
        ui::texto(
            r.x,
            y,
            &format!(
                "Grátis hoje: {}/{}  ·  Passes na bolsa: {}",
                e.gratis,
                shared::magica::GRATIS_POR_DIA,
                e.passes
            ),
            16,
            if total > 0 { estilo::TEXTO } else { SUAVE },
        );
        y += 22.0;
        ui::texto(
            r.x,
            y,
            "Cada entrada vale 30 minutos. As grátis voltam às 4h da manhã.",
            13,
            SUAVE,
        );
        y += 26.0;

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
                let pode = total >= n as u32;
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
            let pode = total >= self.entradas.max(1) as u32;
            let n = self.entradas.max(1);
            let de_graca = (e.gratis as u32).min(n as u32);
            let rot = if de_graca == n as u32 {
                format!("Entrar ({n} grátis)")
            } else if de_graca > 0 {
                format!("Entrar ({de_graca} grátis + {} passe)", n as u32 - de_graca)
            } else {
                format!("Entrar ({n} passe(s))")
            };
            if ui::botao(Rect::new(r.x, y, r.w, 38.0), &rot, pode) && pode {
                pedido = Some(PedidoMagica::Entrar { entradas: n });
            }
            if !pode {
                ui::texto(
                    r.x,
                    y + 54.0,
                    "Sem entrada: as 3 grátis voltam às 4h, e o passe cai de chefes.",
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
                gratis: 0,
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
                gratis: shared::magica::GRATIS_POR_DIA,
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
