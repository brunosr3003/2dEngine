//! What REBIRTH ESSENCE buys in the world (`shared::essencia`, docs/RESET.md,
//! rebirth phase 3): GEAR ASCENSION (`ClientMessage::Ascender`, a star on a
//! piece at the Forge) and the ESSENCE SHOP (`ClientMessage::LojaEssencia`).
//!
//! The rules, prices and the shop list live in `shared::essencia`; this file
//! checks a session against them, takes the Essence (and Darksteel) from the
//! bag and delivers bag-first, mail for what does not fit.

use super::*;
use shared::essencia as es;
use shared::item_id::{DARKSTEEL, ESSENCIA_RENASCIMENTO};
use shared::protocol::AlvoDaForja;

/// The Essence shop is off (15/11/2026): Essence only buys item stars.
const ESSENCIA_SO_PRA_ESTRELAS: bool = true;

impl GameWorld {
    pub(super) fn handle_ascender(&mut self, sid: SessionId, alvo: AlvoDaForja) {
        let Some(s) = self.sessions.get_mut(&sid).filter(|s| s.logged_in) else {
            return;
        };
        let (item_id, inst) = match alvo {
            AlvoDaForja::Bolsa(i) => match s.inventory.get(i as usize) {
                Some(sl) if sl.qty > 0 => (sl.item_id, sl.instance),
                _ => (0, None),
            },
            AlvoDaForja::Equipado(slot) => (
                s.equipment.get(slot).unwrap_or(0),
                s.equipment.get_inst(slot),
            ),
        };
        let recusa = |s: &Session, estrelas: u8, motivo: String| {
            let _ = s.handle.to_client.send(ServerMessage::AscensaoResultado {
                ok: false,
                estrelas,
                item_id,
                motivo,
            });
        };
        let Some(mut inst) = inst.filter(|_| es::ascendivel(item_id)) else {
            recusa(s, 0, es::NAO_ASCENDIVEL.into());
            return;
        };
        let Some((essencia, ds)) = es::custo_da_estrela(inst.estrelas) else {
            recusa(s, inst.estrelas, es::NO_TOPO.into());
            return;
        };
        let (tem_e, tem_ds) = (
            crate::craft::tem(&s.inventory, ESSENCIA_RENASCIMENTO),
            crate::craft::tem(&s.inventory, DARKSTEEL),
        );
        if tem_e < essencia || tem_ds < ds {
            recusa(
                s,
                inst.estrelas,
                format!("needs {essencia} Rebirth Essence and {ds} Darksteel"),
            );
            return;
        }
        crate::craft::consumir(&mut s.inventory, ESSENCIA_RENASCIMENTO, essencia);
        crate::craft::consumir(&mut s.inventory, DARKSTEEL, ds);
        inst.estrelas += 1;
        match alvo {
            AlvoDaForja::Bolsa(i) => s.inventory[i as usize].instance = Some(inst),
            AlvoDaForja::Equipado(slot) => {
                s.equipment.set(slot, Some(item_id), Some(inst));
                s.stats = effective_stats_rv(
                    &s.equipment,
                    &s.allocated_points,
                    &s.proficiencies,
                    s.xp,
                    s.renascimentos,
                    &s.arvore,
                );
                s.mp_current = s.mp_current.min(s.stats.mp_max as f32);
                s.stats_dirty = true;
            }
        }
        s.inventory_dirty = true;
        tracing::info!(
            "[ascensao] {} subiu o item {item_id} pra {} estrelas",
            s.name,
            inst.estrelas
        );
        crate::telemetria::conta_de(&s.name, "ascensao", inst.estrelas, 1);
        let _ = s.handle.to_client.send(ServerMessage::AscensaoResultado {
            ok: true,
            estrelas: inst.estrelas,
            item_id,
            motivo: String::new(),
        });
        let (entity, hp_max) = (s.entity, s.stats.hp_max);
        if let Some(e) = entity.filter(|_| matches!(alvo, AlvoDaForja::Equipado(_))) {
            if let Ok(mut h) = self.ecs.get::<&mut Health>(e) {
                h.max = hp_max;
                h.current = h.current.min(h.max);
            }
        }
        self.save_pending = true;
    }

    pub(super) fn handle_loja_essencia(&mut self, sid: SessionId, oferta: u8) {
        // The Essence shop was removed (owner, 15/11/2026): Essence is only
        // for item stars. An old client asking gets a polite refusal.
        if ESSENCIA_SO_PRA_ESTRELAS {
            if let Some(s) = self.sessions.get(&sid) {
                let _ = s.handle.to_client.send(shared::protocol::ServerMessage::Chat {
                    from: "SYS".into(),
                    text: "The Essence shop was removed: Rebirth Essence is used to add stars to items at the Forge.".into(),
                });
            }
            return;
        }
        let Some(o) = es::oferta(oferta) else {
            return;
        };
        let Some(s) = self.sessions.get_mut(&sid).filter(|s| s.logged_in) else {
            return;
        };
        match o.entrega {
            // The tree reset has its own checks (points spent, downed) and
            // answer: the tree's Essence path.
            es::Entrega::RedistribuirArvore => {
                self.handle_arvore(sid, shared::arvore::PedidoArvore::RedistribuirComEssencia);
            }
            es::Entrega::Item { id, qtd } => {
                let tem = crate::craft::tem(&s.inventory, ESSENCIA_RENASCIMENTO);
                if tem < o.preco {
                    let t = format!(
                        "Not enough Rebirth Essence: {} costs {} Essence.",
                        o.nome, o.preco
                    );
                    self.send_chat_to(sid, &t);
                    return;
                }
                crate::craft::consumir(&mut s.inventory, ESSENCIA_RENASCIMENTO, o.preco);
                // All or nothing: a partial fill plus the whole amount by
                // mail would pay twice.
                let t = if super::dungeon::bolsa_tudo_ou_nada(s, id, qtd, None) {
                    format!("Bought {} for {} Rebirth Essence.", o.nome, o.preco)
                } else {
                    let quando = (now_ms() / 1000) as i64;
                    s.dungeon
                        .postar(id, qtd, None, es::MOTIVO_CORREIO_LOJA_ESSENCIA, quando);
                    format!(
                        "Bought {} for {} Rebirth Essence. Your bag is full: it went to the Mail.",
                        o.nome, o.preco
                    )
                };
                s.inventory_dirty = true;
                tracing::info!(
                    "[loja_essencia] {} comprou {} por {}",
                    s.name,
                    o.nome,
                    o.preco
                );
                crate::telemetria::conta_de(&s.name, "loja_essencia", o.id, 1);
                self.save_pending = true;
                self.send_chat_to(sid, &t);
            }
        }
    }
}

#[cfg(test)]
mod testes {
    use super::*;
    use shared::arvore::{Arvore, PedidoArvore};
    use shared::EquipSlot;

    fn mundo_com_um() -> (GameWorld, SessionId) {
        crate::economy::init_vazia_para_testes();
        let mut w = GameWorld::new(HashMap::new());
        w.zona = "ilha_inicial".into();
        let sid = SessionId(([127, 0, 0, 1], 47_301).into());
        let (tx, rx) = mpsc::unbounded_channel();
        std::mem::forget(rx);
        w.on_connect(SessionHandle {
            id: sid,
            to_client: tx,
        });
        let eid = EntityId(4_730);
        let e = w.ecs.spawn((
            NetId(eid),
            Position(Vec2::new(10.0, 10.0)),
            Velocity(Vec2::ZERO),
            EntityKind::Player,
            Health {
                current: 100,
                max: 100,
            },
            PlayerTag {
                name: "Estrelado".into(),
                player_id: PlayerId(49),
                attack_anim_pending: None,
                combo_step_pending: None,
            },
        ));
        let s = w.sessions.get_mut(&sid).unwrap();
        s.logged_in = true;
        s.entity = Some(e);
        s.entity_id = eid;
        s.name = "Estrelado".into();
        s.renascimentos = 1;
        (w, sid)
    }

    fn dar(w: &mut GameWorld, sid: SessionId, item: u16, qtd: u32) {
        let s = w.sessions.get_mut(&sid).unwrap();
        if item == DARKSTEEL {
            assert!(add_to_inventory(&mut s.inventory, item, qtd, None));
            return;
        }
        // One stack in a free slot: the test economy has no stack sizes.
        let livre = s.inventory.iter().position(|x| x.qty == 0).unwrap();
        s.inventory[livre] = shared::InventorySlot {
            item_id: item,
            qty: qtd,
            instance: None,
        };
    }

    fn tem(w: &GameWorld, sid: SessionId, item: u16) -> u32 {
        crate::craft::tem(&w.sessions[&sid].inventory, item)
    }

    /// A katana WORN, so the stats can be compared.
    fn com_katana(w: &mut GameWorld, sid: SessionId) {
        let s = w.sessions.get_mut(&sid).unwrap();
        let k = shared::ItemInstance::roll_for(shared::item_id::KATANA, 30, || 0.5).unwrap();
        s.equipment
            .set(EquipSlot::Weapon, Some(shared::item_id::KATANA), Some(k));
        s.stats = effective_stats_rv(
            &s.equipment,
            &s.allocated_points,
            &s.proficiencies,
            s.xp,
            s.renascimentos,
            &s.arvore,
        );
    }

    fn ascender(w: &mut GameWorld, sid: SessionId) {
        w.on_message(
            sid,
            ClientMessage::Ascender {
                alvo: AlvoDaForja::Equipado(EquipSlot::Weapon),
            },
        );
    }

    fn estrelas(w: &GameWorld, sid: SessionId) -> u8 {
        w.sessions[&sid]
            .equipment
            .get_inst(EquipSlot::Weapon)
            .unwrap()
            .estrelas
    }

    #[test]
    fn ascensao_cobra_e_sobe_os_atributos() {
        let (mut w, sid) = mundo_com_um();
        com_katana(&mut w, sid);
        let antes = w.sessions[&sid].stats.attack_damage;
        // Without Essence: refused, nothing taken.
        dar(&mut w, sid, DARKSTEEL, 100_000);
        ascender(&mut w, sid);
        assert_eq!(estrelas(&w, sid), 0);
        assert_eq!(tem(&w, sid, DARKSTEEL), 100_000);
        dar(&mut w, sid, ESSENCIA_RENASCIMENTO, 15);
        ascender(&mut w, sid);
        assert_eq!(estrelas(&w, sid), 1);
        assert_eq!(tem(&w, sid, ESSENCIA_RENASCIMENTO), 5);
        assert_eq!(tem(&w, sid, DARKSTEEL), 98_000);
        let s = &w.sessions[&sid];
        assert!(
            s.stats.attack_damage > antes,
            "{} > {antes}",
            s.stats.attack_damage
        );
        let conta = effective_stats_rv(
            &s.equipment,
            &s.allocated_points,
            &s.proficiencies,
            s.xp,
            s.renascimentos,
            &s.arvore,
        );
        assert_eq!(s.stats.attack_damage, conta.attack_damage);
        // 5 Essence is not the 20 of the second star.
        ascender(&mut w, sid);
        assert_eq!(estrelas(&w, sid), 1);
        assert_eq!(tem(&w, sid, ESSENCIA_RENASCIMENTO), 5);
    }

    #[test]
    fn para_nas_cinco_estrelas() {
        let (mut w, sid) = mundo_com_um();
        com_katana(&mut w, sid);
        dar(&mut w, sid, ESSENCIA_RENASCIMENTO, 1_000);
        dar(&mut w, sid, DARKSTEEL, 100_000);
        for _ in 0..7 {
            ascender(&mut w, sid);
        }
        assert_eq!(estrelas(&w, sid), es::ESTRELAS_MAX);
        assert_eq!(
            tem(&w, sid, ESSENCIA_RENASCIMENTO),
            1_000 - es::essencia_ate_o_topo()
        );
        assert_eq!(tem(&w, sid, DARKSTEEL), 100_000 - 62_000);
    }

    #[test]
    fn ascende_na_bolsa_e_recusa_material() {
        let (mut w, sid) = mundo_com_um();
        let k = shared::ItemInstance::roll_for(shared::item_id::KATANA, 30, || 0.5).unwrap();
        {
            let s = w.sessions.get_mut(&sid).unwrap();
            s.inventory[0] = shared::InventorySlot {
                item_id: shared::item_id::KATANA,
                qty: 1,
                instance: Some(k),
            };
        }
        dar(&mut w, sid, ESSENCIA_RENASCIMENTO, 10);
        dar(&mut w, sid, DARKSTEEL, 2_000);
        w.on_message(
            sid,
            ClientMessage::Ascender {
                alvo: AlvoDaForja::Bolsa(0),
            },
        );
        assert_eq!(w.sessions[&sid].inventory[0].instance.unwrap().estrelas, 1);
        assert_eq!(tem(&w, sid, ESSENCIA_RENASCIMENTO), 0);
        // A stack of material is not gear.
        let i = w.sessions[&sid]
            .inventory
            .iter()
            .position(|s| s.item_id == DARKSTEEL);
        if let Some(i) = i {
            w.on_message(
                sid,
                ClientMessage::Ascender {
                    alvo: AlvoDaForja::Bolsa(i as u16),
                },
            );
        }
        assert_eq!(w.sessions[&sid].inventory[0].instance.unwrap().estrelas, 1);
    }

    /// The Essence shop was removed (15/11/2026): a request from an old
    /// client buys nothing and spends nothing.
    #[test]
    fn a_loja_de_essencia_saiu_e_nada_e_cobrado() {
        let (mut w, sid) = mundo_com_um();
        let chave = shared::item_id::chave_na_cor(shared::item_id::CLAW, 5);
        dar(&mut w, sid, ESSENCIA_RENASCIMENTO, 70);
        for oferta in [1, 3, 200] {
            w.on_message(sid, ClientMessage::LojaEssencia { oferta });
        }
        assert_eq!(tem(&w, sid, chave), 0, "the shop sold a key");
        assert_eq!(tem(&w, sid, ESSENCIA_RENASCIMENTO), 70, "Essence was spent");
    }

    /// Upgrading two pieces into one keeps the higher star count.
    #[test]
    fn aprimorar_mantem_as_estrelas() {
        crate::economy::init_vazia_para_testes();
        let mut a = shared::ItemInstance::roll_for(shared::item_id::KATANA, 30, || 0.5).unwrap();
        let mut b = a;
        a.estrelas = 1;
        b.estrelas = 3;
        let mut inv = vec![shared::InventorySlot::default(); 10];
        inv[0] = shared::InventorySlot {
            item_id: shared::item_id::KATANA,
            qty: 1,
            instance: Some(a),
        };
        inv[1] = shared::InventorySlot {
            item_id: shared::item_id::KATANA,
            qty: 1,
            instance: Some(b),
        };
        let cobre = shared::forja::custo_de_aprimorar(a.grau(), a.tier());
        assert!(add_to_inventory(
            &mut inv,
            shared::item_id::COPPER,
            cobre,
            None
        ));
        let r = crate::craft::aprimorar(&mut inv, 0, 1, 100, &mut |id, nivel, cor, tier| {
            shared::ItemInstance::roll_em(
                shared::items::item_template(id),
                nivel,
                cor,
                tier,
                || 0.5,
            )
        });
        assert!(r.is_ok(), "{r:?}");
        assert_eq!(inv[0].instance.unwrap().estrelas, 3);
    }
}
