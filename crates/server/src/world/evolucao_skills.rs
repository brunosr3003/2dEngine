//! Evolução determinística das habilidades: Energia coletada no mundo,
//! cobre da carteira e tomos condensados para a habilidade escolhida.

use super::*;
use shared::skills::{self, GrauTomo};

impl GameWorld {
    fn resposta_de_evolucao(&self, sid: SessionId, ok: bool, texto: impl Into<String>) {
        let Some(s) = self.sessions.get(&sid) else {
            return;
        };
        let _ = s.handle.to_client.send(ServerMessage::ResultadoDeEvolucao {
            ok,
            texto: texto.into(),
        });
        let _ = s.handle.to_client.send(ServerMessage::ProgressoDeSkills {
            progresso: s.skill_progress.clone(),
        });
    }

    fn skill_liberada(s: &Session, skill_id: u32) -> Result<shared::skills::Skill, String> {
        let skill = crate::skills::skill_of(skill_id)
            .or_else(|| {
                #[cfg(test)]
                {
                    shared::skills::playtest()
                        .into_iter()
                        .find(|x| x.id == skill_id)
                }
                #[cfg(not(test))]
                {
                    None
                }
            })
            .ok_or_else(|| "habilidade inválida".to_string())?;
        let nivel = shared::level_of_xp_with_mult(s.xp, crate::economy::xp_multiplier());
        let precisa = shared::skills::DESTRAVA_EM[(skill.ordem.saturating_sub(1) as usize).min(2)];
        if nivel < precisa {
            return Err(format!("essa habilidade libera no nível {precisa}"));
        }
        Ok(skill)
    }

    pub(super) fn handle_fabricar_tomo_de_skill(
        &mut self,
        sid: SessionId,
        skill_id: u32,
        grau: u8,
    ) {
        let Some(grau) = GrauTomo::de_u8(grau) else {
            self.resposta_de_evolucao(sid, false, "grau de tomo inválido");
            return;
        };
        let resultado = {
            let Some(s) = self.sessions.get_mut(&sid).filter(|s| s.logged_in) else {
                return;
            };
            let skill = match Self::skill_liberada(s, skill_id) {
                Ok(v) => v,
                Err(e) => {
                    self.resposta_de_evolucao(sid, false, e);
                    return;
                }
            };
            let custo = skills::custo_de_tomo(grau);
            let cobre = crate::craft::tem(&s.inventory, shared::item_id::COPPER);
            if s.skill_progress.tomos(skill_id, grau) == u16::MAX {
                Err("limite de tomos alcançado".into())
            } else if s.skill_progress.energia < custo.energia {
                Err(format!(
                    "faltam {} de Energia",
                    custo.energia - s.skill_progress.energia
                ))
            } else if cobre < custo.cobre {
                Err(format!("faltam {} de cobre", custo.cobre - cobre))
            } else {
                s.skill_progress.energia -= custo.energia;
                crate::craft::consumir(&mut s.inventory, shared::item_id::COPPER, custo.cobre);
                let i = (skill_id - 1) as usize;
                s.skill_progress.tomos[i][grau as usize] =
                    s.skill_progress.tomos[i][grau as usize].saturating_add(1);
                s.inventory_dirty = true;
                s.skills_dirty = true;
                Ok(format!(
                    "Tomo {} de {} condensado.",
                    grau.nome(),
                    skill.nome
                ))
            }
        };
        match resultado {
            Ok(texto) => {
                self.save_pending = true;
                crate::telemetria::conta("tomo_skill", format!("{skill_id}-{grau:?}"), 1);
                self.resposta_de_evolucao(sid, true, texto);
            }
            Err(texto) => self.resposta_de_evolucao(sid, false, texto),
        }
    }

    pub(super) fn handle_evoluir_skill(&mut self, sid: SessionId, skill_id: u32) {
        let resultado = {
            let Some(s) = self.sessions.get_mut(&sid).filter(|s| s.logged_in) else {
                return;
            };
            let skill = match Self::skill_liberada(s, skill_id) {
                Ok(v) => v,
                Err(e) => {
                    self.resposta_de_evolucao(sid, false, e);
                    return;
                }
            };
            let atual = s.skill_progress.tier(skill_id);
            let Some(custo) = skills::custo_de_evolucao(atual) else {
                self.resposta_de_evolucao(sid, false, "essa habilidade já está no Tier X");
                return;
            };
            let nivel = shared::level_of_xp_with_mult(s.xp, crate::economy::xp_multiplier());
            let cobre = crate::craft::tem(&s.inventory, shared::item_id::COPPER);
            let i = (skill_id - 1) as usize;
            if nivel < custo.nivel {
                Err(format!(
                    "o Tier {} exige nível {}",
                    skills::tier_romano(custo.destino),
                    custo.nivel
                ))
            } else if s.skill_progress.energia < custo.energia {
                Err(format!(
                    "faltam {} de Energia",
                    custo.energia - s.skill_progress.energia
                ))
            } else if cobre < custo.cobre {
                Err(format!("faltam {} de cobre", custo.cobre - cobre))
            } else if let Some(grau) = custo
                .tomo
                .filter(|g| s.skill_progress.tomos[i][*g as usize] == 0)
            {
                Err(format!("falta o Tomo {} de {}", grau.nome(), skill.nome))
            } else {
                s.skill_progress.energia -= custo.energia;
                crate::craft::consumir(&mut s.inventory, shared::item_id::COPPER, custo.cobre);
                if let Some(grau) = custo.tomo {
                    s.skill_progress.tomos[i][grau as usize] -= 1;
                }
                s.skill_progress.tiers[i] = custo.destino;
                s.inventory_dirty = true;
                s.skills_dirty = true;
                Ok(format!(
                    "{} evoluiu para Tier {}.",
                    skill.nome,
                    skills::tier_romano(custo.destino)
                ))
            }
        };
        match resultado {
            Ok(texto) => {
                self.save_pending = true;
                crate::telemetria::conta("evoluir_skill", skill_id, 1);
                self.resposta_de_evolucao(sid, true, texto);
            }
            Err(texto) => self.resposta_de_evolucao(sid, false, texto),
        }
    }
}

#[cfg(test)]
mod testes {
    use super::*;
    use tokio::sync::mpsc;

    fn jogador() -> (GameWorld, SessionId, mpsc::UnboundedReceiver<ServerMessage>) {
        crate::economy::init_vazia_para_testes();
        let mut w = GameWorld::new(HashMap::new());
        let sid = SessionId(([127, 0, 0, 1], 19_901).into());
        let (tx, rx) = mpsc::unbounded_channel();
        w.on_connect(SessionHandle {
            id: sid,
            to_client: tx,
        });
        let s = w.sessions.get_mut(&sid).unwrap();
        s.logged_in = true;
        s.name = "aluno".into();
        s.inventory = vec![shared::InventorySlot {
            item_id: shared::item_id::COPPER,
            qty: 10_000,
            instance: None,
        }];
        s.skill_progress.energia = 10_000;
        (w, sid, rx)
    }

    #[test]
    fn evolucao_cobra_uma_vez_e_recusa_sem_nivel_ou_tomo() {
        let (mut w, sid, mut rx) = jogador();
        w.on_message(sid, ClientMessage::EvoluirSkill { skill_id: 1 });
        let s = &w.sessions[&sid];
        assert_eq!(s.skill_progress.tier(1), 2);
        assert_eq!(s.skill_progress.energia, 9_900);
        assert_eq!(
            crate::craft::tem(&s.inventory, shared::item_id::COPPER),
            9_900
        );
        assert!(s.skills_dirty && s.inventory_dirty && w.save_pending);
        let mut sucesso = false;
        while let Ok(m) = rx.try_recv() {
            sucesso |= matches!(m, ServerMessage::ResultadoDeEvolucao { ok: true, .. });
        }
        assert!(sucesso);

        // Tier IV -> V: sem nivel/tomo nao consome nada.
        let s = w.sessions.get_mut(&sid).unwrap();
        s.skill_progress.tiers[0] = 4;
        let antes = s.skill_progress.clone();
        let cobre = crate::craft::tem(&s.inventory, shared::item_id::COPPER);
        w.on_message(sid, ClientMessage::EvoluirSkill { skill_id: 1 });
        assert_eq!(w.sessions[&sid].skill_progress, antes);
        assert_eq!(
            crate::craft::tem(&w.sessions[&sid].inventory, shared::item_id::COPPER),
            cobre
        );
        w.sessions.get_mut(&sid).unwrap().xp =
            shared::xp_for_level_with_mult(15, crate::economy::xp_multiplier());
        w.on_message(sid, ClientMessage::EvoluirSkill { skill_id: 1 });
        assert_eq!(w.sessions[&sid].skill_progress, antes);
        w.on_message(
            sid,
            ClientMessage::FabricarTomoDeSkill {
                skill_id: 1,
                grau: 0,
            },
        );
        assert_eq!(w.sessions[&sid].skill_progress.tomos[0][0], 1);
        w.on_message(sid, ClientMessage::EvoluirSkill { skill_id: 1 });
        assert_eq!(w.sessions[&sid].skill_progress.tier(1), 5);
        assert_eq!(w.sessions[&sid].skill_progress.tomos[0][0], 0);
    }

    #[test]
    fn pedidos_invalidos_nao_gastam_recursos() {
        let (mut w, sid, _) = jogador();
        let antes = w.sessions[&sid].skill_progress.clone();
        w.on_message(
            sid,
            ClientMessage::FabricarTomoDeSkill {
                skill_id: 99,
                grau: 0,
            },
        );
        w.on_message(
            sid,
            ClientMessage::FabricarTomoDeSkill {
                skill_id: 1,
                grau: 9,
            },
        );
        w.on_message(sid, ClientMessage::EvoluirSkill { skill_id: 99 });
        assert_eq!(w.sessions[&sid].skill_progress, antes);
        assert_eq!(
            crate::craft::tem(&w.sessions[&sid].inventory, shared::item_id::COPPER),
            10_000
        );
    }

    #[test]
    fn cristal_credita_energia_sem_ocupar_bolsa() {
        let (mut w, sid, mut rx) = jogador();
        w.zona = "ilha_inicial".into();
        w.sessions.get_mut(&sid).unwrap().inventory.clear();
        let antes = w.sessions[&sid].skill_progress.energia;
        let cristal = shared::terreno::Coletavel {
            coluna: 42,
            centro: Vec2::ZERO,
            tier: 5,
        };
        assert!(w.coletar_plantado(sid, cristal));
        assert_eq!(
            w.sessions[&sid].skill_progress.energia,
            antes + shared::skills::energia_por_coleta(0)
        );
        assert!(w.sessions[&sid].inventory.is_empty());
        assert_eq!(w.pedras[&cristal.coluna].coletas, 1);
        assert!(rx
            .try_recv()
            .is_ok_and(|m| matches!(m, ServerMessage::ProgressoDeSkills { .. })));
    }
}
