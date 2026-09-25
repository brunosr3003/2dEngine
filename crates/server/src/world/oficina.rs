//! As abas Aprimorar e Combinar do Craft, no mundo: pega a bolsa da sessao,
//! chama as regras puras de `crate::craft` e responde. Ver `oficina_ui` no
//! cliente e `shared::combinar`.

use super::*;

impl GameWorld {
    /// Aba Aprimorar do Craft: duas pecas iguais da bolsa viram uma do tier
    /// seguinte. Regras em `craft::aprimorar`.
    pub(super) fn handle_aprimorar(&mut self, sid: SessionId, a: usize, b: usize) {
        let xpmult = crate::economy::xp_multiplier();
        let Some(session) = self.sessions.get_mut(&sid) else {
            return;
        };
        if !session.logged_in {
            return;
        }
        let nivel = shared::level_of_xp_with_mult(session.xp, xpmult);
        let mut rolar = |id: u16, nivel_item: u16, grau: u8, tier: u8| {
            shared::ItemInstance::roll_em(
                crate::economy::item_template_of(id),
                nivel_item,
                grau,
                tier,
                fastrand::f32,
            )
        };
        let r = crate::craft::aprimorar(&mut session.inventory, a, b, nivel, &mut rolar);
        let (ok, texto, item_id, grau, tier) = match r {
            Ok((id, grau, tier)) => {
                session.inventory_dirty = true;
                self.save_pending = true;
                crate::telemetria::conta("aprimorar", format!("{grau}-{tier}"), 1);
                (true, String::new(), id, grau, tier)
            }
            Err(m) => (false, m, 0, 0, 0),
        };
        let _ = session
            .handle
            .to_client
            .send(ServerMessage::AprimorarResultado {
                ok,
                texto,
                item_id,
                grau,
                tier,
            });
    }

    /// Aba Combinar do Craft: tentativas de subir chave/material de cor.
    /// Regras em `craft::combinar` e `shared::combinar`.
    pub(super) fn handle_combinar(&mut self, sid: SessionId, entrada: u16, vezes: u16) {
        let Some(session) = self.sessions.get_mut(&sid) else {
            return;
        };
        if !session.logged_in {
            return;
        }
        let resposta =
            |tentativas: u16, sucessos: u16, texto: String| ServerMessage::CombinarResultado {
                entrada,
                tentativas,
                sucessos,
                texto,
            };
        let Some(receita) = shared::combinar::receita(entrada) else {
            let _ = session
                .handle
                .to_client
                .send(resposta(0, 0, "isso não se combina".into()));
            return;
        };
        let vezes = vezes.clamp(1, shared::combinar::MAX_VEZES);
        let cap = crate::economy::item_stack_max(receita.saida);
        let r = crate::craft::combinar(
            &mut session.inventory,
            &receita,
            vezes,
            cap,
            &crate::economy::nome_do_item,
            &mut || fastrand::u8(0..100),
        );
        let msg = match r {
            Ok((n, ok)) => {
                session.inventory_dirty = true;
                self.save_pending = true;
                crate::telemetria::conta("combinar", entrada, n as i64);
                crate::telemetria::conta("combinar_sucesso", entrada, ok as i64);
                resposta(n, ok, String::new())
            }
            Err(m) => resposta(0, 0, m),
        };
        let _ = session.handle.to_client.send(msg);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use shared::item_id;

    fn jogador(w: &mut GameWorld) -> (SessionId, mpsc::UnboundedReceiver<ServerMessage>) {
        let sid = SessionId(([127, 0, 0, 1], 19_900).into());
        let (tx, rx) = mpsc::unbounded_channel();
        w.on_connect(SessionHandle {
            id: sid,
            to_client: tx,
        });
        let s = w.sessions.get_mut(&sid).unwrap();
        s.logged_in = true;
        s.name = "ferreiro".into();
        (sid, rx)
    }

    fn ultima(rx: &mut mpsc::UnboundedReceiver<ServerMessage>) -> Option<ServerMessage> {
        let mut u = None;
        while let Ok(m) = rx.try_recv() {
            u = Some(m);
        }
        u
    }

    fn mat(id: u16, qty: u32) -> shared::InventorySlot {
        shared::InventorySlot {
            item_id: id,
            qty,
            instance: None,
        }
    }

    #[test]
    fn pergaminhos_de_tomo_dez_pagados_entregam_onze_e_recusam_repeticao() {
        crate::economy::init_vazia_para_testes();
        let mut w = GameWorld::new(HashMap::new());
        let (sid, mut rx) = jogador(&mut w);
        w.sessions.get_mut(&sid).unwrap().inventory =
            vec![mat(item_id::PERGAMINHO_INVOCA_TOMO, 10)];
        w.on_message(
            sid,
            ClientMessage::AbrirPergaminhos {
                slot: 0,
                quantidade: 10,
            },
        );
        let s = &w.sessions[&sid];
        assert_eq!(s.inventory[0].qty, 0);
        assert_eq!(
            s.skill_progress
                .tomos
                .iter()
                .flatten()
                .map(|n| *n as u32)
                .sum::<u32>(),
            11
        );
        assert!(matches!(ultima(&mut rx), Some(ServerMessage::Loja {
            aviso: shared::loja::AvisoLoja::Invocacoes { premios }
        }) if premios.len() == 11));
        w.on_message(
            sid,
            ClientMessage::AbrirPergaminhos {
                slot: 0,
                quantidade: 10,
            },
        );
        assert_eq!(
            w.sessions[&sid]
                .skill_progress
                .tomos
                .iter()
                .flatten()
                .map(|n| *n as u32)
                .sum::<u32>(),
            11
        );
    }

    /// Ponta a ponta pelo `on_message`: a mensagem do cliente chega, a bolsa
    /// muda e a resposta sai no canal da sessao.
    #[test]
    fn combinar_pela_mensagem_cobra_e_responde() {
        crate::economy::init_vazia_para_testes();
        let mut w = GameWorld::new(HashMap::new());
        let (sid, mut rx) = jogador(&mut w);
        let mut bolsa = vec![shared::InventorySlot::default(); 8];
        bolsa[0] = mat(item_id::HORN, 12);
        w.sessions.get_mut(&sid).unwrap().inventory = bolsa;
        w.on_message(
            sid,
            ClientMessage::Combinar {
                entrada: item_id::HORN,
                vezes: 50,
            },
        );
        match ultima(&mut rx) {
            Some(ServerMessage::CombinarResultado {
                entrada,
                tentativas,
                sucessos,
                ..
            }) => {
                assert_eq!((entrada, tentativas), (item_id::HORN, 2));
                let s = &w.sessions[&sid];
                assert_eq!(crate::craft::tem(&s.inventory, item_id::HORN), 2);
                // O premio e' SORTEADO na familia da cor de cima, e nao o
                // mesmo chifre verde (22/09/2026): conta a familia inteira.
                let r = shared::combinar::receita(item_id::HORN).unwrap();
                let ganhos: u32 = shared::combinar::saidas_possiveis(&r)
                    .iter()
                    .map(|id| crate::craft::tem(&s.inventory, *id))
                    .sum();
                assert_eq!(ganhos, sucessos as u32);
                assert!(s.inventory_dirty);
            }
            outra => panic!("resposta errada: {outra:?}"),
        }
        // Item que nao combina: recusa com motivo, sem mexer.
        w.on_message(
            sid,
            ClientMessage::Combinar {
                entrada: item_id::COPPER,
                vezes: 1,
            },
        );
        assert!(matches!(
            ultima(&mut rx),
            Some(ServerMessage::CombinarResultado { tentativas: 0, .. })
        ));
    }

    #[test]
    fn aprimorar_pela_mensagem_funde_e_responde() {
        crate::economy::init_vazia_para_testes();
        crate::economy::por_item_para_testes(
            item_id::KATANA,
            1,
            shared::items::item_template(item_id::KATANA),
        );
        let mut w = GameWorld::new(HashMap::new());
        let (sid, mut rx) = jogador(&mut w);
        let peca = || {
            let mut i = shared::ItemInstance::roll_for(item_id::KATANA, 5, || 0.5).unwrap();
            i.rarity = 1;
            i.tier = 1;
            shared::InventorySlot {
                item_id: item_id::KATANA,
                qty: 1,
                instance: Some(i),
            }
        };
        let mut bolsa = vec![shared::InventorySlot::default(); 8];
        bolsa[1] = peca();
        bolsa[4] = peca();
        bolsa[6] = mat(item_id::COPPER, 700);
        w.sessions.get_mut(&sid).unwrap().inventory = bolsa;
        w.on_message(
            sid,
            ClientMessage::Aprimorar {
                slot_a: 1,
                slot_b: 4,
            },
        );
        match ultima(&mut rx) {
            Some(ServerMessage::AprimorarResultado {
                ok: true,
                item_id: id,
                grau: 1,
                tier: 2,
                ..
            }) => assert_eq!(id, item_id::KATANA),
            outra => panic!("resposta errada: {outra:?}"),
        }
        let s = &w.sessions[&sid];
        assert_eq!(
            s.inventory[1].instance.map(|i| (i.grau(), i.tier())),
            Some((1, 2))
        );
        assert_eq!(s.inventory[4].qty, 0);
        assert_eq!(crate::craft::tem(&s.inventory, item_id::COPPER), 200);
        // A mesma peca duas vezes: recusa.
        w.on_message(
            sid,
            ClientMessage::Aprimorar {
                slot_a: 1,
                slot_b: 1,
            },
        );
        assert!(matches!(
            ultima(&mut rx),
            Some(ServerMessage::AprimorarResultado { ok: false, .. })
        ));
    }
}
