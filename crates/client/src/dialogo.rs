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
    /// Entrega a quem deu: "Receber".
    Entrega { quest_id: u16 },
    /// Missao nova: "Aceitar" ou "Agora não".
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

    /// "Agora não" numa oferta.
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
        let w = (screen_width() - 40.0).min(600.0);
        Rect::new(
            (screen_width() - w) * 0.5,
            screen_height() - 430.0,
            w,
            176.0,
        )
    }

    pub fn pega_mouse(&self) -> bool {
        self.aberto && Self::painel().contains(Vec2::from(mouse_position()))
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
        if crate::ui::botao(
            Rect::new(p.x + p.w - 44.0, p.y + 8.0, 32.0, 28.0),
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
                &format!("Recompensa: {}", self.recompensa),
                p.x + 60.0,
                rodape,
                p.w - 330.0,
                14,
                estilo::OURO,
            );
        }
        let rotulo = if !ultima {
            "Próximo"
        } else {
            match self.fim {
                Some(Fim::Conversa { .. }) => "Concluir",
                Some(Fim::Entrega { .. }) => "Receber",
                Some(Fim::Oferta { .. }) => "Aceitar",
                None => "Fechar",
            }
        };
        let b = Rect::new(p.x + p.w - 124.0, p.y + p.h - 40.0, 108.0, 28.0);
        if crate::ui::botao(b, rotulo, true) {
            return self.avancar();
        }
        if ultima
            && matches!(self.fim, Some(Fim::Oferta { .. }))
            && crate::ui::botao(Rect::new(b.x - 118.0, b.y, 108.0, 28.0), "Agora não", true)
        {
            return self.recusar();
        }
        Resultado::Nada
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn proximo_ate_o_fim_decide() {
        let mut d = Dialogo::default();
        assert_eq!(d.avancar(), Resultado::Nada, "fechado");
        d.abrir(
            "Alquimista",
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
            "Mestre",
            "x",
            &["ok"],
            Fim::Entrega { quest_id: 502 },
            "80 ouro".into(),
        );
        assert_eq!(d.avancar(), Resultado::Receber(502));

        d.abrir(
            "Mestre",
            "x",
            &["a", "b"],
            Fim::Oferta { quest_id: 503 },
            String::new(),
        );
        d.avancar();
        assert_eq!(d.recusar(), Resultado::Recusou(503));
        d.abrir(
            "Mestre",
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
            "Mestre",
            "x",
            &["a"],
            Fim::Entrega { quest_id: 502 },
            String::new(),
        );
        assert_eq!(d.fechar(), Resultado::Fechou);
        assert_eq!(d.fechar(), Resultado::Nada);
    }
}
