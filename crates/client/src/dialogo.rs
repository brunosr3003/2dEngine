//! Caixa de dialogo das missoes: falas com "Proximo" e, no fim, o que a
//! conversa decide — concluir a conversa ("fale com"), receber a recompensa ou
//! aceitar a missao. Quem aperta e' o jogador; quem valida e' o servidor.
use macroquad::prelude::*;
use shared::EntityId;

use crate::hud_estilo as estilo;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Fim {
    /// "Fale com": ao terminar, o servidor marca a conversa.
    Conversa { quest_id: u16, npc: EntityId },
    /// Entrega a quem deu: "Receive".
    Entrega { quest_id: u16 },
    /// Missao nova: "Accept" ou "Not now".
    Oferta { quest_id: u16 },
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Resultado {
    Nada,
    Conversou { quest_id: u16, npc: EntityId },
    Receber(u16),
    Aceitar(u16),
    Recusou(u16),
    Fechou,
}

#[derive(Default)]
pub struct Dialogo {
    pub aberto: bool,
    quem: String,
    titulo: String,
    falas: Vec<String>,
    i: usize,
    fim: Option<Fim>,
    /// Quando o auto passa a próxima fala. 0 = ainda não começou a contar.
    proximo_passo: f64,
    recompensa: String,
}

impl Dialogo {
    pub fn abrir(
        &mut self,
        quem: &str,
        titulo: &str,
        falas: &[&str],
        fim: Fim,
        recompensa: String,
    ) {
        *self = Self {
            aberto: true,
            quem: quem.to_string(),
            titulo: titulo.to_string(),
            falas: falas.iter().map(|s| s.to_string()).collect(),
            i: 0,
            fim: Some(fim),
            recompensa,
            // Zera a contagem do auto: cada conversa recomeça o relógio, e
            // sem isso a primeira fala da segunda passaria num quadro.
            proximo_passo: 0.0,
        };
        if self.falas.is_empty() {
            self.falas.push("…".into());
        }
    }

    /// Esc ou X. `Fechou` se havia algo aberto.
    pub fn fechar(&mut self) -> Resultado {
        let r = if self.aberto {
            Resultado::Fechou
        } else {
            Resultado::Nada
        };
        *self = Self::default();
        r
    }

    pub fn fim(&self) -> Option<Fim> {
        self.fim
    }

    pub fn na_ultima(&self) -> bool {
        self.i + 1 >= self.falas.len()
    }

    /// "Proximo": anda uma fala; na ultima, fecha com o que o fim decide.
    pub fn avancar(&mut self) -> Resultado {
        if !self.aberto {
            return Resultado::Nada;
        }
        if !self.na_ultima() {
            self.i += 1;
            return Resultado::Nada;
        }
        let r = match self.fim {
            Some(Fim::Conversa { quest_id, npc }) => Resultado::Conversou { quest_id, npc },
            Some(Fim::Entrega { quest_id }) => Resultado::Receber(quest_id),
            Some(Fim::Oferta { quest_id }) => Resultado::Aceitar(quest_id),
            None => Resultado::Fechou,
        };
        *self = Self::default();
        r
    }

    /// "Not now" numa oferta.
    pub fn recusar(&mut self) -> Resultado {
        match self.fim {
            Some(Fim::Oferta { quest_id }) if self.aberto => {
                *self = Self::default();
                Resultado::Recusou(quest_id)
            }
            _ => self.fechar(),
        }
    }

    fn painel() -> Rect {
        let f = estilo::fator_texto();
        let w = (screen_width() - 40.0).min(620.0 * f);
        // A CAIXA CRESCEU pra caber botão de dedo. Ela tinha 176 px fixos e o
        // botão dentro dela, 28 — e 28 px é metade do alvo mínimo que um dedo
        // acerta. O dono: "os botões de ação, tipo da conversa receber,
        // próximo etc, estão muito pequenos, eu clico errado toda hora".
        let h = 176.0 * f;
        Rect::new(
            (screen_width() - w) * 0.5,
            screen_height() - 430.0 * f.max(1.0),
            w,
            h,
        )
    }

    /// Os botões do rodapé: (principal, "Not now").
    ///
    /// Fora do desenho pra ser medido. `ALVO_DO_DEDO` é o piso: abaixo dele o
    /// toque erra, e errar aqui custa recusar uma missão sem querer.
    fn botoes(p: Rect, f: f32) -> (Rect, Rect) {
        let alt = (34.0 * f).max(crate::hud_estilo::ALVO_DO_DEDO);
        let larg = (130.0 * f).max(110.0).min((p.w - 36.0) * 0.5);
        let y = p.y + p.h - alt - 10.0 * f;
        let principal = Rect::new(p.x + p.w - larg - 14.0, y, larg, alt);
        let recusar = Rect::new(principal.x - larg - 10.0, y, larg, alt);
        (principal, recusar)
    }

    pub fn pega_mouse(&self) -> bool {
        self.aberto && Self::painel().contains(Vec2::from(mouse_position()))
    }

    /// A AUTO MISSÃO conduz a conversa sozinha.
    ///
    /// O dono: "no auto missão tem que aceitar e entregar missões no NPC de
    /// maneira automática também, clicar em próximo na conversa etc". Faz
    /// sentido: o auto já anda até o NPC e abre a fala — parar ali e pedir
    /// quatro toques é interromper justamente o que ele automatizou.
    ///
    /// `cada` é a pausa entre falas. Ela existe porque a conversa é onde a
    /// história acontece: passar tudo num quadro faria o texto piscar e
    /// sumir. Recusar NUNCA é automático — dizer "agora não" é decisão, e o
    /// auto só faz o que o jogador já pediu ao ligar a missão.
    pub fn conduzir(&mut self, agora: f64, cada: f64) -> Resultado {
        if !self.aberto {
            self.proximo_passo = 0.0;
            return Resultado::Nada;
        }
        if self.proximo_passo == 0.0 {
            self.proximo_passo = agora + cada;
            return Resultado::Nada;
        }
        if agora < self.proximo_passo {
            return Resultado::Nada;
        }
        self.proximo_passo = agora + cada;
        self.avancar()
    }

    pub fn desenha(&mut self) -> Resultado {
        if !self.aberto {
            return Resultado::Nada;
        }
        let p = Self::painel();
        estilo::painel(p);
        estilo::texto_ajustado(
            &self.quem,
            p.x + 16.0,
            p.y + 28.0,
            p.w * 0.45,
            20,
            estilo::OURO,
        );
        let tw = estilo::medir(&self.titulo, 14).min(p.w * 0.45);
        estilo::texto_ajustado(
            &self.titulo,
            p.x + p.w - 56.0 - tw,
            p.y + 27.0,
            p.w * 0.45,
            14,
            estilo::SUAVE,
        );
        let f = estilo::fator_texto();
        let xis = crate::hud_estilo::ALVO_DO_DEDO.max(34.0 * f);
        if crate::ui::botao(
            Rect::new(p.x + p.w - xis - 8.0, p.y + 6.0, xis, xis),
            "x",
            true,
        ) {
            return self.fechar();
        }
        draw_line(
            p.x + 12.0,
            p.y + 42.0,
            p.x + p.w - 12.0,
            p.y + 42.0,
            1.0,
            estilo::BORDA,
        );
        for (k, linha) in crate::missoes::quebra(&self.falas[self.i], p.w - 32.0, 17, 3)
            .iter()
            .enumerate()
        {
            estilo::texto(
                p.x + 16.0,
                p.y + 68.0 + k as f32 * 23.0,
                linha,
                17,
                estilo::TEXTO,
            );
        }
        let ultima = self.na_ultima();
        let rodape = p.y + p.h - 22.0;
        estilo::texto(
            p.x + 16.0,
            rodape,
            &format!("{}/{}", self.i + 1, self.falas.len()),
            13,
            estilo::SUAVE,
        );
        let paga = matches!(
            self.fim,
            Some(Fim::Entrega { .. }) | Some(Fim::Oferta { .. })
        );
        if ultima && paga && !self.recompensa.is_empty() {
            estilo::texto_ajustado(
                &format!("Reward: {}", self.recompensa),
                p.x + 60.0,
                rodape,
                p.w - 330.0,
                14,
                estilo::OURO,
            );
        }
        let rotulo = if !ultima {
            "Next"
        } else {
            match self.fim {
                Some(Fim::Conversa { .. }) => "Complete",
                Some(Fim::Entrega { .. }) => "Receive",
                Some(Fim::Oferta { .. }) => "Accept",
                None => "Close",
            }
        };
        let (b, nao) = Self::botoes(p, f);
        if crate::ui::botao(b, rotulo, true) {
            return self.avancar();
        }
        if ultima
            && matches!(self.fim, Some(Fim::Oferta { .. }))
            && crate::ui::botao(nao, "Not now", true)
        {
            return self.recusar();
        }
        Resultado::Nada
    }
}

#[cfg(test)]
mod tests {
    /// O AUTO passa as falas sozinho, no ritmo, e fecha com a ação do fim.
    ///
    /// E ele NÃO recusa: dizer "agora não" é decisão, e o auto só faz o que o
    /// jogador já pediu ao ligar a missão.
    #[test]
    fn o_auto_conduz_a_conversa_e_nunca_recusa() {
        let mut d = Dialogo::default();
        d.abrir(
            "Master",
            "Uma caçada",
            &["Olá.", "Há ursos no bosque.", "Traga dez peles."],
            Fim::Oferta { quest_id: 42 },
            String::new(),
        );
        let cada = 1.5;
        let mut t = 100.0;

        // O primeiro quadro só ARMA o relógio: sem isso a primeira fala
        // passaria antes de aparecer.
        assert_eq!(d.conduzir(t, cada), Resultado::Nada);
        assert_eq!(d.i, 0, "a primeira fala tem que ficar na tela");

        // Antes da hora, nada anda.
        assert_eq!(d.conduzir(t + cada * 0.5, cada), Resultado::Nada);
        assert_eq!(d.i, 0);

        // No tempo, anda uma fala por vez.
        t += cada;
        assert_eq!(d.conduzir(t, cada), Resultado::Nada);
        assert_eq!(d.i, 1);
        t += cada;
        assert_eq!(d.conduzir(t, cada), Resultado::Nada);
        assert_eq!(d.i, 2, "na última fala");

        // E na última, a ação do fim — ACEITAR, nunca recusar.
        t += cada;
        assert_eq!(d.conduzir(t, cada), Resultado::Aceitar(42));
        assert!(!d.aberto, "a conversa fecha ao aceitar");

        // Fechada, conduzir não faz nada.
        assert_eq!(d.conduzir(t + 99.0, cada), Resultado::Nada);
    }

    /// Entrega fecha com RECEBER, que é a outra ponta do mesmo gesto.
    #[test]
    fn o_auto_entrega_a_missao_pronta() {
        let mut d = Dialogo::default();
        d.abrir(
            "Master",
            "Feito",
            &["Bom trabalho."],
            Fim::Entrega { quest_id: 7 },
            "120 cobre".into(),
        );
        let (cada, mut t) = (1.0, 0.0);
        assert_eq!(d.conduzir(t, cada), Resultado::Nada);
        t += cada;
        assert_eq!(d.conduzir(t, cada), Resultado::Receber(7));
    }

    use super::*;

    #[test]
    fn proximo_ate_o_fim_decide() {
        let mut d = Dialogo::default();
        assert_eq!(d.avancar(), Resultado::Nada, "fechado");
        d.abrir(
            "Alchemist",
            "Conheça",
            &["a", "b", "c"],
            Fim::Conversa {
                quest_id: 501,
                npc: EntityId(7),
            },
            String::new(),
        );
        assert!(d.aberto);
        assert_eq!(d.avancar(), Resultado::Nada);
        assert_eq!(d.avancar(), Resultado::Nada);
        assert!(d.na_ultima());
        assert_eq!(
            d.avancar(),
            Resultado::Conversou {
                quest_id: 501,
                npc: EntityId(7)
            }
        );
        assert!(!d.aberto);

        d.abrir(
            "Master",
            "x",
            &["ok"],
            Fim::Entrega { quest_id: 502 },
            "80 ouro".into(),
        );
        assert_eq!(d.avancar(), Resultado::Receber(502));

        d.abrir(
            "Master",
            "x",
            &["a", "b"],
            Fim::Oferta { quest_id: 503 },
            String::new(),
        );
        d.avancar();
        assert_eq!(d.recusar(), Resultado::Recusou(503));
        d.abrir(
            "Master",
            "x",
            &[],
            Fim::Oferta { quest_id: 503 },
            String::new(),
        );
        assert_eq!(
            d.avancar(),
            Resultado::Aceitar(503),
            "sem fala ainda tem o fim"
        );
        d.abrir(
            "Master",
            "x",
            &["a"],
            Fim::Entrega { quest_id: 502 },
            String::new(),
        );
        assert_eq!(d.fechar(), Resultado::Fechou);
        assert_eq!(d.fechar(), Resultado::Nada);
    }
}
