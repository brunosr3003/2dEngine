use super::*;
use shared::social::{Aviso, Pedido};

impl GameWorld {
    pub(super) fn social_resultado(&self, sid: SessionId, ok: bool, texto: &str) {
        if let Some(s) = self.sessions.get(&sid) {
            let _ = s.handle.to_client.send(ServerMessage::Social {
                aviso: Aviso::Resultado {
                    ok,
                    texto: texto.into(),
                },
            });
        }
    }

    pub(super) fn handle_social(&mut self, sid: SessionId, pedido: Pedido) {
        let Some(s) = self.sessions.get(&sid).filter(|s| s.logged_in) else {
            return;
        };
        if let Pedido::ReceberAnexos { id } = pedido {
            if s.correio_em_voo {
                self.social_resultado(sid, false, "Wait for the current claim.");
                return;
            }
            if s.correio_recibos
                .iter()
                .any(|r| Some(r.id) == id.checked_neg())
            {
                self.social_resultado(sid, false, "Anexos já entregues; aguardando confirmação.");
                return;
            }
            let Some(ctx) = &self.auth_ctx else {
                return;
            };
            crate::correio_admin::resgatar(
                ctx.pool.clone(),
                ctx.tx.clone(),
                sid,
                s.name.clone(),
                id,
            );
            self.sessions.get_mut(&sid).unwrap().correio_em_voo = true;
            return;
        }
        let Some(ctx) = self.auth_ctx.as_ref() else {
            self.social_resultado(sid, false, "Social unavailable: the server has no database.");
            return;
        };
        // At most one operation in flight per session; no SQL blocks the tick.
        if self.social_pendentes.contains_key(&sid) {
            if !matches!(pedido, Pedido::Estado) {
                self.social_resultado(sid, false, "Wait for the refresh and try again.");
            }
            return;
        }
        if self
            .social_pedido_em
            .get(&sid)
            .is_some_and(|t| self.sim_time_s - t < 0.3)
        {
            if !matches!(pedido, Pedido::Estado) {
                self.social_resultado(sid, false, "Wait a moment and try again.");
            }
            return;
        }
        self.social_pedido_em.insert(sid, self.sim_time_s);
        let nome = s.name.clone();
        let conta = s.account_id;
        self.social_pendentes.insert(sid, nome.clone());
        let (pool, tx) = (ctx.pool.clone(), ctx.tx.clone());
        tokio::spawn(async move {
            let mut avisos = Vec::new();
            let mut oficial_texto = None;
            let resultado = if matches!(pedido, Pedido::EnviarOficial { .. }) {
                match conta {
                    Some(conta) => crate::correio_admin::enviar(&pool, &nome, conta, &pedido)
                        .await
                        .map(|n| {
                            oficial_texto =
                                Some(format!("Correio oficial enviado para {n} personagem(ns)."));
                        }),
                    None => Err(anyhow::anyhow!("Account not authenticated.")),
                }
            } else {
                crate::social::executar(&pool, &nome, &pedido).await
            };
            if !matches!(pedido, Pedido::Estado) || resultado.is_err() {
                let (ok, texto) = match resultado {
                    Ok(()) => (
                        true,
                        oficial_texto.unwrap_or_else(|| {
                            match pedido {
                                Pedido::EnviarCarta { .. } => "Letter sent.",
                                Pedido::Amizade { .. } => "Friend request sent.",
                                Pedido::ConvidarCla { .. } => "Clan invite sent.",
                                _ => "Updated.",
                            }
                            .to_string()
                        }),
                    ),
                    Err(e) => {
                        let texto = if e.downcast_ref::<sqlx::Error>().is_some() {
                            tracing::warn!("social: {e:#}");
                            "Social unavailable. Try again.".to_string()
                        } else {
                            e.to_string()
                        };
                        (false, texto)
                    }
                };
                avisos.push(Aviso::Resultado { ok, texto });
            }
            match crate::social::estado(&pool, &nome).await {
                Ok(e) => avisos.push(Aviso::Estado(e)),
                Err(e) => {
                    tracing::warn!("social estado: {e:#}");
                    avisos.push(Aviso::Resultado {
                        ok: false,
                        texto: "The social panel could not be loaded.".into(),
                    });
                }
            }
            let _ = tx.send(IncomingMessage::Social { sid, nome, avisos });
        });
    }

    pub fn on_correio_entrega(&mut self, entrega: crate::correio_admin::Entrega) {
        let crate::correio_admin::Entrega {
            sid,
            nome,
            recibo,
            anexos,
            aceitou,
        } = entrega;
        let Some(s) = self
            .sessions
            .get_mut(&sid)
            .filter(|s| s.logged_in && s.name == nome && s.correio_em_voo)
        else {
            let _ = aceitou.send(false);
            return;
        };
        if s.correio_recibos.iter().any(|r| r.id == recibo.id) {
            let _ = aceitou.send(false);
            return;
        }
        // Plans on a copy: a full bag never receives half the attachments.
        let mut bolsa = s.inventory.clone();
        if !anexos
            .iter()
            // The INSTANCE goes with it. With `None` here the mail delivered the item
            // bare — equipment with no tier and no refinement, a pet with no level and no
            // skills, a mount with no color — and nothing warned.
            .all(|a| add_to_inventory(&mut bolsa, a.item_id, a.qtd, a.instance))
        {
            let _ = aceitou.send(false);
            return;
        }
        s.inventory = bolsa;
        s.inventory_dirty = true;
        s.correio_recibos.push(recibo);
        self.save_pending = true;
        let _ = aceitou.send(true);
    }

    pub fn on_correio_fim(&mut self, sid: SessionId, nome: String, texto: String) {
        let Some(s) = self
            .sessions
            .get_mut(&sid)
            .filter(|s| s.logged_in && s.name == nome)
        else {
            return;
        };
        s.correio_em_voo = false;
        if texto == "Attachments received and saved." {
            s.correio_recibos.clear();
        }
        self.social_resultado(sid, texto == "Attachments received and saved.", &texto);
        self.handle_social(sid, Pedido::Estado);
    }

    pub fn on_social(&mut self, sid: SessionId, nome: String, avisos: Vec<Aviso>) {
        if self.social_pendentes.get(&sid) != Some(&nome) {
            return;
        }
        self.social_pendentes.remove(&sid);
        let Some(s) = self
            .sessions
            .get(&sid)
            .filter(|s| s.logged_in && s.name == nome)
        else {
            return;
        };
        for mut aviso in avisos {
            if let Aviso::Estado(e) = &mut aviso {
                e.neste_canal = self
                    .sessions
                    .values()
                    .filter(|s| {
                        s.logged_in
                            && (e.amigos.contains(&s.name)
                                || e.cla.as_ref().is_some_and(|c| c.membros.contains(&s.name)))
                    })
                    .map(|s| s.name.clone())
                    .collect();
            }
            let _ = s.handle.to_client.send(ServerMessage::Social { aviso });
        }
        if let Some(pid) = s.party_id {
            let _ = s.handle.to_client.send(ServerMessage::PartyUpdate {
                members: self.party_members(pid),
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn jogador(w: &mut GameWorld, n: u16) -> SessionId {
        let sid = SessionId(([127, 0, 0, 1], 19000 + n).into());
        let (tx, _rx) = mpsc::unbounded_channel();
        w.on_connect(SessionHandle {
            id: sid,
            to_client: tx,
        });
        let s = w.sessions.get_mut(&sid).unwrap();
        s.logged_in = true;
        s.name = format!("p{n}");
        sid
    }
    #[test]
    fn anexos_nao_entregam_parcial_e_nao_repetem() {
        crate::economy::init_vazia_para_testes();
        let mut w = GameWorld::new(HashMap::new());
        let sid = jogador(&mut w, 8);
        let s = w.sessions.get_mut(&sid).unwrap();
        s.correio_em_voo = true;
        s.inventory = vec![shared::InventorySlot::default()];
        let (ack, mut rx) = tokio::sync::oneshot::channel();
        let antes = s.inventory.clone();
        let anexos = vec![
            shared::social::Anexo {
                item_id: 1,
                qtd: 1,
                instance: None,
            },
            shared::social::Anexo {
                item_id: 2,
                qtd: 1,
                instance: None,
            },
        ];
        w.on_correio_entrega(crate::correio_admin::Entrega {
            sid,
            nome: "p8".into(),
            recibo: crate::correio_admin::Recibo {
                id: 1,
                token: "x".into(),
            },
            anexos,
            aceitou: ack,
        });
        assert!(!rx.try_recv().unwrap());
        assert_eq!(w.sessions[&sid].inventory[0].qty, antes[0].qty);
        assert!(w.sessions[&sid].correio_recibos.is_empty());
        let (ack, mut rx) = tokio::sync::oneshot::channel();
        w.on_correio_entrega(crate::correio_admin::Entrega {
            sid,
            nome: "p8".into(),
            recibo: crate::correio_admin::Recibo {
                id: 1,
                token: "x".into(),
            },
            anexos: vec![shared::social::Anexo {
                item_id: 1,
                qtd: 1,
                instance: None,
            }],
            aceitou: ack,
        });
        assert!(rx.try_recv().unwrap());
        assert_eq!(w.sessions[&sid].inventory[0].qty, 1);
        let (ack, mut rx) = tokio::sync::oneshot::channel();
        w.on_correio_entrega(crate::correio_admin::Entrega {
            sid,
            nome: "p8".into(),
            recibo: crate::correio_admin::Recibo {
                id: 1,
                token: "x".into(),
            },
            anexos: vec![shared::social::Anexo {
                item_id: 1,
                qtd: 1,
                instance: None,
            }],
            aceitou: ack,
        });
        assert!(!rx.try_recv().unwrap());
        assert_eq!(w.sessions[&sid].inventory[0].qty, 1);
    }

    #[test]
    fn grupo_convites_limite_expiracao_e_saida() {
        let mut w = GameWorld::new(HashMap::new());
        let ids: Vec<_> = (0..7).map(|i| jogador(&mut w, i)).collect();
        w.handle_party_invite(ids[0], "p1".into());
        w.handle_party_accept(ids[1]);
        let pid = w.sessions[&ids[0]].party_id.unwrap();
        assert_eq!(w.party_members(pid).len(), 2);
        w.handle_party_invite(ids[2], "p1".into());
        w.handle_party_accept(ids[1]);
        assert_eq!(w.sessions[&ids[1]].party_id, Some(pid));
        // Invitations may coexist, but accepting re-checks the slots.
        for i in 2..6 {
            w.handle_party_invite(ids[0], format!("p{i}"));
        }
        for id in &ids[2..6] {
            w.handle_party_accept(*id);
        }
        assert_eq!(w.party_members(pid).len(), 5);
        assert!(w.sessions[&ids[5]].party_id.is_none());
        w.handle_party_leave(ids[4]);
        w.handle_party_invite(ids[0], "p6".into());
        w.sim_time_s += 61.0;
        w.handle_party_accept(ids[6]);
        assert!(w.sessions[&ids[6]].party_id.is_none());
        w.sessions.get_mut(&ids[6]).unwrap().instancia = 42;
        w.handle_party_invite(ids[0], "p6".into());
        w.handle_party_accept(ids[6]);
        assert!(w.sessions[&ids[6]].party_id.is_none());
        for id in &ids[1..4] {
            w.handle_party_leave(*id);
        }
        assert!(w.sessions[&ids[0]].party_id.is_none());
        assert!(w.party_members(pid).is_empty());
    }
}
