//! THE PATH TREE in the world (`shared::arvore`, docs/RESET.md, rebirth
//! phase 2).
//!
//! `ClientMessage::Arvore`: the state, buy the next node of a path, reset
//! every point for gold. The rules, the node table and the point budget live
//! in `shared::arvore`; this file applies them to a session. The combat
//! passives enter `effective_stats_rv`; the economy ones `bonus_economia`.

use super::*;
use shared::arvore::{self as av, PedidoArvore};

impl Session {
    /// Path points earned (`shared::arvore::pontos_ganhos`): 10 a climb
    /// from the first rebirth on, one every 10 SHOWN levels.
    pub(crate) fn pontos_da_arvore(&self) -> u32 {
        av::pontos_ganhos(
            self.renascimentos,
            shared::progressao::nivel_exibido(self.xp, crate::economy::xp_multiplier()),
        )
    }

    /// The per-character economy bonus: the companions' accessories
    /// (`acessorios::bonus`) plus the tree's Corsair share.
    pub(crate) fn bonus_economia(&self) -> shared::acessorios::Bonus {
        let mut b = shared::acessorios::bonus(&self.equipment);
        self.arvore.passivas().somar_economia(&mut b);
        b
    }

    /// `ServerMessage::Arvore` with the state now.
    pub(crate) fn enviar_arvore(&mut self) {
        let ganhos = self.pontos_da_arvore();
        self.arvore_ganhos_visto = ganhos;
        let _ = self.handle.to_client.send(ServerMessage::Arvore {
            estado: av::EstadoArvore::de(self.arvore, ganhos),
        });
    }
}

impl GameWorld {
    pub(super) fn handle_arvore(&mut self, sid: SessionId, pedido: PedidoArvore) {
        let Some(s) = self.sessions.get_mut(&sid).filter(|s| s.logged_in) else {
            return;
        };
        let mut aviso: Option<String> = None;
        let mut mudou = false;
        match pedido {
            PedidoArvore::Painel => {}
            PedidoArvore::Gastar { caminho, no } => {
                let ganhos = s.pontos_da_arvore();
                match s.arvore.gastar(caminho, no, ganhos) {
                    Ok(n) => {
                        mudou = true;
                        let nome = av::Caminho::de_indice(caminho).map_or("", |c| c.nome());
                        tracing::info!(
                            "[arvore] {} comprou {nome} {} ({:?} {})",
                            s.name,
                            no + 1,
                            n.efeito,
                            n.valor
                        );
                        crate::telemetria::conta_de(&s.name, "arvore_no", format!("{caminho}:{}", no + 1), 1);
                    }
                    Err(m) => aviso = Some(m.into()),
                }
            }
            // Phase 3 adds the Essence reset as a new variant after this one.
            PedidoArvore::Redistribuir => {
                let gastos = s.arvore.gastos();
                let custo = av::custo_de_redistribuir(gastos);
                if gastos == 0 {
                    aviso = Some("No path points to reset.".into());
                } else if s.downed {
                    aviso = Some("You cannot reset the paths while downed.".into());
                } else if s.gold < custo {
                    aviso = Some(format!("Not enough gold: resetting the paths costs {custo} gold."));
                } else {
                    s.gold -= custo;
                    s.arvore = av::Arvore::default();
                    mudou = true;
                    aviso = Some(format!("Paths reset: {gastos} points back for {custo} gold."));
                    tracing::info!("[arvore] {} redistribuiu {gastos} pontos por {custo} de ouro", s.name);
                    crate::telemetria::conta_de(&s.name, "arvore_redistribuir", gastos, 1);
                }
            }
        }
        if mudou {
            s.stats = effective_stats_rv(&s.equipment, &s.allocated_points, &s.proficiencies, s.xp, s.renascimentos, &s.arvore);
            s.mp_current = s.mp_current.min(s.stats.mp_max as f32);
            s.poise_current = s.poise_current.min(s.stats.poise_max as f32);
            s.stats_dirty = true;
        }
        s.enviar_arvore();
        let (entity, hp_max) = (s.entity, s.stats.hp_max);
        if mudou {
            self.save_pending = true;
            if let Some(e) = entity {
                if let Ok(mut h) = self.ecs.get::<&mut Health>(e) {
                    h.max = hp_max;
                    h.current = h.current.min(h.max);
                }
            }
        }
        if let Some(t) = aviso {
            self.send_chat_to(sid, &t);
        }
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    fn mundo_com_um() -> (GameWorld, SessionId) {
        crate::economy::init_vazia_para_testes();
        let mut w = GameWorld::new(HashMap::new());
        w.zona = "ilha_inicial".into();
        let sid = SessionId(([127, 0, 0, 1], 47_101).into());
        let (tx, rx) = mpsc::unbounded_channel();
        std::mem::forget(rx);
        w.on_connect(SessionHandle { id: sid, to_client: tx });
        let eid = EntityId(4_710);
        let e = w.ecs.spawn((NetId(eid), Position(Vec2::new(10.0, 10.0)),
            Velocity(Vec2::ZERO), EntityKind::Player, Health { current: 100, max: 100 },
            PlayerTag { name: "Arvoredo".into(), player_id: PlayerId(48),
                attack_anim_pending: None, combo_step_pending: None }));
        let s = w.sessions.get_mut(&sid).unwrap();
        s.logged_in = true;
        s.entity = Some(e);
        s.entity_id = eid;
        s.name = "Arvoredo".into();
        (w, sid)
    }

    /// `r` rebirths at SHOWN level `nivel`.
    fn com(w: &mut GameWorld, sid: SessionId, r: u32, nivel: u32) {
        let mult = crate::economy::xp_multiplier();
        let s = w.sessions.get_mut(&sid).unwrap();
        s.renascimentos = r;
        s.xp = shared::progressao::xp_do_nivel_exibido(nivel, mult);
        s.last_level = shared::level_of_xp_with_mult(s.xp, mult);
        s.equipment.weapon = Some(shared::item_id::ESPADA_E_ESCUDO);
        s.stats = effective_stats_rv(&s.equipment, &s.allocated_points, &s.proficiencies, s.xp, s.renascimentos, &s.arvore);
    }

    fn gastar(w: &mut GameWorld, sid: SessionId, caminho: u8) {
        let no = w.sessions[&sid].arvore.nos[caminho as usize];
        w.on_message(sid, ClientMessage::Arvore { pedido: PedidoArvore::Gastar { caminho, no } });
    }

    #[test]
    fn sem_renascer_nao_ha_ponto() {
        let (mut w, sid) = mundo_com_um();
        com(&mut w, sid, 0, 100);
        assert_eq!(w.sessions[&sid].pontos_da_arvore(), 0);
        gastar(&mut w, sid, 2);
        assert_eq!(w.sessions[&sid].arvore, av::Arvore::default());
    }

    #[test]
    fn gastar_muda_os_atributos_e_respeita_o_orcamento() {
        let (mut w, sid) = mundo_com_um();
        com(&mut w, sid, 2, 55);
        assert_eq!(w.sessions[&sid].pontos_da_arvore(), 15);
        let antes = w.sessions[&sid].stats;
        for _ in 0..15 {
            gastar(&mut w, sid, av::Caminho::Guardiao as u8);
        }
        let s = &w.sessions[&sid];
        assert_eq!(s.arvore.nos, [0, 0, 15, 0]);
        assert!(s.stats.hp_max > antes.hp_max, "+16% health");
        assert!(s.stats.defense >= antes.defense);
        assert!(s.stats.damage_reduction_pct > antes.damage_reduction_pct);
        assert!(s.stats.hp_regen > antes.hp_regen);
        // The live stats are exactly `effective_stats_rv` with the tree.
        let sem = effective_stats_r(&s.equipment, &s.allocated_points, &s.proficiencies, s.xp, s.renascimentos);
        assert_eq!(antes.hp_max, sem.hp_max);
        // Out of points: the next one is refused.
        gastar(&mut w, sid, av::Caminho::Assassino as u8);
        assert_eq!(w.sessions[&sid].arvore.nos[0], 0);
        // The ECS health follows the new max.
        let e = w.sessions[&sid].entity.unwrap();
        assert_eq!(w.ecs.get::<&Health>(e).unwrap().max, w.sessions[&sid].stats.hp_max);
    }

    #[test]
    fn fora_de_ordem_recusa() {
        let (mut w, sid) = mundo_com_um();
        com(&mut w, sid, 1, 100);
        w.on_message(sid, ClientMessage::Arvore { pedido: PedidoArvore::Gastar { caminho: 0, no: 3 } });
        assert_eq!(w.sessions[&sid].arvore, av::Arvore::default());
        w.on_message(sid, ClientMessage::Arvore { pedido: PedidoArvore::Gastar { caminho: 0, no: 0 } });
        w.on_message(sid, ClientMessage::Arvore { pedido: PedidoArvore::Gastar { caminho: 0, no: 0 } });
        assert_eq!(w.sessions[&sid].arvore.nos, [1, 0, 0, 0], "the double tap bought once");
    }

    #[test]
    fn corsario_soma_no_bonus_de_economia() {
        let (mut w, sid) = mundo_com_um();
        com(&mut w, sid, 3, 1);
        assert_eq!(w.sessions[&sid].bonus_economia(), shared::acessorios::Bonus::default());
        for _ in 0..15 {
            gastar(&mut w, sid, av::Caminho::Corsario as u8);
        }
        let b = w.sessions[&sid].bonus_economia();
        assert!((b.drop - 0.16).abs() < 1e-4 && (b.ouro - 0.20).abs() < 1e-4);
        assert!(b.minerio > 0.0 && b.energia > 0.0 && b.darksteel > 0.0 && b.loot_melhor > 0.0);
    }

    #[test]
    fn redistribuir_custa_ouro_e_devolve_os_pontos() {
        let (mut w, sid) = mundo_com_um();
        com(&mut w, sid, 1, 100);
        for _ in 0..4 {
            gastar(&mut w, sid, av::Caminho::Assassino as u8);
        }
        let crit = w.sessions[&sid].stats.crit_chance;
        // Not enough gold: nothing happens.
        w.sessions.get_mut(&sid).unwrap().gold = 3_999;
        w.on_message(sid, ClientMessage::Arvore { pedido: PedidoArvore::Redistribuir });
        assert_eq!(w.sessions[&sid].arvore.gastos(), 4);
        assert_eq!(w.sessions[&sid].gold, 3_999);
        w.sessions.get_mut(&sid).unwrap().gold = 5_000;
        w.on_message(sid, ClientMessage::Arvore { pedido: PedidoArvore::Redistribuir });
        let s = &w.sessions[&sid];
        assert_eq!(s.arvore, av::Arvore::default());
        assert_eq!(s.gold, 1_000, "4 points x 1,000 gold");
        assert!(s.stats.crit_chance < crit);
        assert_eq!(s.arvore.livres(s.pontos_da_arvore()), 10, "every point free again");
    }

    /// A rebirth keeps the allocation and the budget: the cap of one climb
    /// is the start of the next.
    #[test]
    fn renascer_mantem_a_arvore() {
        let (mut w, sid) = mundo_com_um();
        com(&mut w, sid, 1, 100);
        for _ in 0..10 {
            gastar(&mut w, sid, av::Caminho::Mago as u8);
        }
        let mult = crate::economy::xp_multiplier();
        let s = w.sessions.get_mut(&sid).unwrap();
        s.xp = shared::progressao::xp_maximo(mult);
        s.last_level = shared::progressao::NIVEL_MAX;
        w.on_message(sid, ClientMessage::Renascer);
        let s = &w.sessions[&sid];
        assert_eq!(s.renascimentos, 2);
        assert_eq!(s.arvore.nos, [0, 10, 0, 0]);
        assert_eq!(s.arvore.livres(s.pontos_da_arvore()), 0);
        let sem = effective_stats_r(&s.equipment, &s.allocated_points, &s.proficiencies, s.xp, s.renascimentos);
        assert!(s.stats.mp_max > sem.mp_max, "the Mage nodes still count after the rebirth");
    }
}
