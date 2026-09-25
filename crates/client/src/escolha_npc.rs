//! NPC que da' missao E tem uma funcao (Loja, Forja, Viajar, Banco): tocar
//! nele pergunta o que o jogador quer. Antes a oferta de missao vinha na
//! frente e a funcao abria atras dela — quem so' queria o banco aceitava a
//! missao sem querer.
//!
//! O servidor manda `EscolhaNoNpc`; a resposta e' `EscolherNoNpc`.

use macroquad::prelude::*;
use shared::protocol::ClientMessage;

use crate::hud_estilo::{self as estilo, u};

#[derive(Debug, Clone, PartialEq)]
struct Pergunta {
    npc_eid: u64,
    nome: String,
    funcao: String,
}

#[derive(Debug, Default)]
pub struct EscolhaNpc {
    aberta: Option<Pergunta>,
    /// Abriu com o dedo na tela: o toque que abriu nao escolhe nada.
    espera_soltar: bool,
}

impl EscolhaNpc {
    pub fn abrir(&mut self, npc_eid: u64, nome: String, funcao: String) {
        self.aberta = Some(Pergunta {
            npc_eid,
            nome,
            funcao,
        });
        self.espera_soltar = is_mouse_button_down(MouseButton::Left);
    }

    pub fn fechar(&mut self) {
        self.aberta = None;
    }

    pub fn aberta(&self) -> bool {
        self.aberta.is_some()
    }

    fn escala() -> f32 {
        estilo::escala_do_painel(460.0, 300.0)
    }

    fn painel() -> Rect {
        let k = Self::escala();
        let s = crate::hud_layout::tela_segura();
        let (w, h) = ((420.0 * k).min(s.w - 16.0), (230.0 * k).min(s.h - 16.0));
        Rect::new(s.center().x - w * 0.5, s.center().y - h * 0.5, w, h)
    }

    pub fn pega_mouse(&self) -> bool {
        self.aberta() && Self::painel().contains(Vec2::from(mouse_position()))
    }

    /// Desenha; devolve a escolha feita (e fecha).
    pub fn desenha(&mut self) -> Option<ClientMessage> {
        self.aberta.as_ref()?;
        estilo::no_painel(Self::escala(), || self.desenha_na_escala())
    }

    fn desenha_na_escala(&mut self) -> Option<ClientMessage> {
        let q = self.aberta.clone()?;
        if self.espera_soltar {
            if !is_mouse_button_down(MouseButton::Left) {
                self.espera_soltar = false;
            }
        }
        crate::hud_layout::escurece(0.35);
        let p = Self::painel();
        estilo::painel_destaque(p, estilo::OURO);
        estilo::texto_centro_forte(p.center().x, p.y + u(40.0), &q.nome, 21, estilo::OURO);
        estilo::texto_centro(
            p.center().x,
            p.y + u(68.0),
            "O que você quer?",
            15,
            estilo::SUAVE,
        );
        let m = Vec2::from(mouse_position());
        let clicou = !self.espera_soltar && crate::foco::clique();
        let bw = (p.w - u(60.0)) * 0.5;
        let bh = u(58.0);
        let by = p.y + p.h - bh - u(56.0);
        let missao = Rect::new(p.x + u(20.0), by, bw, bh);
        let funcao = Rect::new(missao.x + bw + u(20.0), by, bw, bh);
        estilo::botao(
            missao,
            "Missões",
            estilo::estado_de(missao, false, false),
            false,
        );
        estilo::botao(
            funcao,
            &q.funcao,
            estilo::estado_de(funcao, false, false),
            true,
        );
        estilo::texto_centro(
            p.center().x,
            p.y + p.h - u(20.0),
            "toque fora para fechar",
            12,
            estilo::SUAVE,
        );
        if !clicou {
            return None;
        }
        let escolha = if missao.contains(m) {
            Some(true)
        } else if funcao.contains(m) {
            Some(false)
        } else if !p.contains(m) {
            self.fechar();
            return None;
        } else {
            None
        }?;
        self.fechar();
        Some(ClientMessage::EscolherNoNpc {
            npc_eid: q.npc_eid,
            missao: escolha,
        })
    }
}
