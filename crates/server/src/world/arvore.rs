//! THE PATH TREE in the world (`shared::arvore`, docs/RESET.md, rebirth
//! phase 2).
//!
//! `ClientMessage::Arvore`: the state, learn a web node next to one already
//! owned, reset every point for gold, equip an active. The rules, the node table and the point budget live
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
    /// (`acessorios::bonus`) plus the tree's Corsair share, plus the Corsair
    /// actives running now (`buffs_da_arvore`, phase 4).
    pub(crate) fn bonus_economia(&self) -> shared::acessorios::Bonus {
        let mut b = shared::acessorios::bonus(&self.equipment);
        self.arvore.passivas().somar_economia(&mut b);
        for &(ativa, _) in &self.buffs_da_arvore {
            let x = av::bonus_de_saque(ativa, self.renascimentos);
            b.drop += x.drop;
            b.ouro += x.ouro;
            b.loot_melhor += x.loot_melhor;
        }
        b
    }

    /// `ServerMessage::Arvore` with the state now.
    pub(crate) fn enviar_arvore(&mut self) {
        let ganhos = self.pontos_da_arvore();
        self.arvore_ganhos_visto = ganhos;
        let _ = self.handle.to_client.send(ServerMessage::Arvore {
            estado: av::EstadoArvore::de(self.arvore, ganhos).com_renascimentos(self.renascimentos),
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
            // The linear paths' request (retired with the web): nothing to
            // buy, the fresh state below tells the client what is there.
            PedidoArvore::Gastar { .. } => {}
            PedidoArvore::Aprender { no } => {
                let ganhos = s.pontos_da_arvore();
                match s.arvore.aprender(no, ganhos) {
                    Ok(n) => {
                        mudou = true;
                        tracing::info!(
                            "[arvore] {} aprendeu o no {no} ({:?} {:?} {}, {} pts)",
                            s.name,
                            n.tipo,
                            av::caminho_de(n),
                            n.nome,
                            n.custo
                        );
                        crate::telemetria::conta_de(&s.name, "arvore_no", no.to_string(), 1);
                        // An ACTIVE node (phase 4): the active takes the next
                        // open empty slot (4, 5, 6).
                        if let Some(ativa) = n.ativa {
                            let r = s.renascimentos;
                            let nome = av::nome_da_ativa(ativa);
                            aviso = Some(match s.arvore.encaixar_nova(ativa, r) {
                                Some(i) => format!("New active: {nome}, in skill slot {}.", i + 4),
                                None => format!("New active: {nome}. Equip it in a skill slot from the path tree."),
                            });
                        }
                    }
                    Err(m) => aviso = Some(m.into()),
                }
            }
            // Phase 4: an owned active into an open extra slot.
            PedidoArvore::Equipar { slot, ativa } => {
                let r = s.renascimentos;
                match s.arvore.equipar(slot, ativa, r) {
                    // Saves like a bought node (the stats recompute is a no-op).
                    Ok(()) => mudou = true,
                    Err(m) => aviso = Some(m.into()),
                }
            }
            // Retired (15/11/2026, owner: Rebirth Essence is only for item
            // ascension). The reset is gold only (`Redistribuir`).
            PedidoArvore::RedistribuirComEssencia => {
                aviso = Some("Path resets are paid in gold: use Reset paths in the path tree.".into());
            }
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

    fn aprender(w: &mut GameWorld, sid: SessionId, no: u16) {
        w.on_message(sid, ClientMessage::Arvore { pedido: PedidoArvore::Aprender { no } });
    }

    /// The central line of path `c`, `n` nodes deep (`Arvore::com_nos`).
    fn espinha(c: av::Caminho, n: u8) -> Vec<u16> {
        let mut q = [0u8; 4];
        q[c as usize] = n;
        av::Arvore::com_nos(q).ids()
    }

    #[test]
    fn sem_renascer_nao_ha_ponto() {
        let (mut w, sid) = mundo_com_um();
        com(&mut w, sid, 0, 100);
        assert_eq!(w.sessions[&sid].pontos_da_arvore(), 0);
        aprender(&mut w, sid, espinha(av::Caminho::Guardiao, 1)[0]);
        assert_eq!(w.sessions[&sid].arvore, av::Arvore::default());
    }

    #[test]
    fn aprender_muda_os_atributos_e_respeita_o_orcamento() {
        let (mut w, sid) = mundo_com_um();
        com(&mut w, sid, 2, 55);
        assert_eq!(w.sessions[&sid].pontos_da_arvore(), 15);
        let antes = w.sessions[&sid].stats;
        // Guardian spine: 1+1+1+2 (Taunt) +2+2+2 +3 (Bulwark Shield) = 14.
        for id in espinha(av::Caminho::Guardiao, 9) {
            aprender(&mut w, sid, id);
        }
        let s = &w.sessions[&sid];
        assert_eq!(s.arvore.ids(), espinha(av::Caminho::Guardiao, 8), "the 9th (3 points) did not fit");
        assert_eq!(s.arvore.gastos(), 14);
        assert!(s.arvore.possui_ativa(6) && s.arvore.possui_ativa(7));
        assert!(s.stats.defense >= antes.defense);
        assert!(s.stats.damage_reduction_pct > antes.damage_reduction_pct);
        let sem = effective_stats_r(&s.equipment, &s.allocated_points, &s.proficiencies, s.xp, s.renascimentos);
        assert_eq!(antes.hp_max, sem.hp_max);
        // One point left: a ring-1 node of another arm still fits.
        aprender(&mut w, sid, espinha(av::Caminho::Assassino, 1)[0]);
        assert_eq!(w.sessions[&sid].arvore.livres(15), 0);
        let e = w.sessions[&sid].entity.unwrap();
        assert_eq!(w.ecs.get::<&Health>(e).unwrap().max, w.sessions[&sid].stats.hp_max);
    }

    #[test]
    fn so_aprende_casa_ligada_e_toque_duplo_compra_uma_vez() {
        let (mut w, sid) = mundo_com_um();
        com(&mut w, sid, 1, 100);
        let linha = espinha(av::Caminho::Assassino, 3);
        aprender(&mut w, sid, linha[2]);
        assert_eq!(w.sessions[&sid].arvore, av::Arvore::default(), "not linked to anything owned");
        aprender(&mut w, sid, linha[0]);
        aprender(&mut w, sid, linha[0]);
        assert_eq!(w.sessions[&sid].arvore.ids(), vec![linha[0]], "the double tap bought once");
        assert_eq!(w.sessions[&sid].arvore.gastos(), 1);
        // The retired linear request changes nothing.
        w.on_message(sid, ClientMessage::Arvore { pedido: PedidoArvore::Gastar { caminho: 0, no: 1 } });
        assert_eq!(w.sessions[&sid].arvore.ids(), vec![linha[0]]);
    }

    #[test]
    fn corsario_soma_no_bonus_de_economia() {
        let (mut w, sid) = mundo_com_um();
        com(&mut w, sid, 3, 1);
        assert_eq!(w.sessions[&sid].bonus_economia(), shared::acessorios::Bonus::default());
        for id in espinha(av::Caminho::Corsario, 6) {
            aprender(&mut w, sid, id);
        }
        let b = w.sessions[&sid].bonus_economia();
        // 5 spine minors at +2% drop and Treasure Sense's +2%.
        assert!((b.drop - 0.12).abs() < 1e-4, "{}", b.drop);
    }

    #[test]
    fn dano_critico_da_arvore_entra_no_critico() {
        let (mut w, sid) = mundo_com_um();
        com(&mut w, sid, 50, 100);
        let agora = w.sim_time_s;
        assert_eq!(w.sessions[&sid].extra_de_dano_crit(agora), 0.0);
        let linha = espinha(av::Caminho::Assassino, 13);
        for &id in &linha {
            aprender(&mut w, sid, id);
        }
        let s = &w.sessions[&sid];
        // Lethal Fury's node +5% and Death's Edge +30%.
        assert!((s.extra_de_dano_crit(agora) - 0.35).abs() < 1e-4, "{}", s.extra_de_dano_crit(agora));
        assert!(super::super::golpe_critico_com(100, s.extra_de_dano_crit(agora)) > super::super::golpe_critico(100));
    }

    #[test]
    fn redistribuir_custa_ouro_e_devolve_os_pontos() {
        let (mut w, sid) = mundo_com_um();
        com(&mut w, sid, 1, 100);
        for id in espinha(av::Caminho::Assassino, 3) {
            aprender(&mut w, sid, id);
        }
        let crit = w.sessions[&sid].stats.crit_chance;
        assert_eq!(w.sessions[&sid].arvore.gastos(), 3);
        w.sessions.get_mut(&sid).unwrap().gold = 2_999;
        w.on_message(sid, ClientMessage::Arvore { pedido: PedidoArvore::Redistribuir });
        assert_eq!(w.sessions[&sid].arvore.gastos(), 3);
        assert_eq!(w.sessions[&sid].gold, 2_999);
        w.sessions.get_mut(&sid).unwrap().gold = 5_000;
        w.on_message(sid, ClientMessage::Arvore { pedido: PedidoArvore::Redistribuir });
        let s = &w.sessions[&sid];
        assert_eq!(s.arvore, av::Arvore::default());
        assert_eq!(s.gold, 2_000, "3 points x 1,000 gold");
        assert!(s.stats.crit_chance < crit);
        assert_eq!(s.arvore.livres(s.pontos_da_arvore()), 10, "every point free again");
    }

    /// The Essence reset is retired: the request is refused and nothing is
    /// spent.
    #[test]
    fn redistribuir_com_essencia_recusa() {
        let (mut w, sid) = mundo_com_um();
        com(&mut w, sid, 1, 100);
        for id in espinha(av::Caminho::Mago, 2) {
            aprender(&mut w, sid, id);
        }
        let antes = w.sessions[&sid].arvore;
        w.on_message(sid, ClientMessage::Arvore { pedido: PedidoArvore::RedistribuirComEssencia });
        assert_eq!(w.sessions[&sid].arvore, antes);
    }

    /// A rebirth keeps the allocation and the budget: the cap of one climb
    /// is the start of the next.
    #[test]
    fn renascer_mantem_a_arvore() {
        let (mut w, sid) = mundo_com_um();
        com(&mut w, sid, 1, 100);
        // Mage spine: 1+1+1+2+2+2 = 9, then one ring-1 Assassin node.
        let mago = espinha(av::Caminho::Mago, 6);
        for &id in &mago {
            aprender(&mut w, sid, id);
        }
        aprender(&mut w, sid, espinha(av::Caminho::Assassino, 1)[0]);
        let mult = crate::economy::xp_multiplier();
        let s = w.sessions.get_mut(&sid).unwrap();
        s.xp = shared::progressao::xp_maximo(mult);
        s.last_level = shared::progressao::NIVEL_MAX;
        let ids = s.arvore.ids();
        w.on_message(sid, ClientMessage::Renascer);
        let s = &w.sessions[&sid];
        assert_eq!(s.renascimentos, 2);
        assert_eq!(s.arvore.ids(), ids);
        assert_eq!(s.arvore.livres(s.pontos_da_arvore()), 0);
        let sem = effective_stats_r(&s.equipment, &s.allocated_points, &s.proficiencies, s.xp, s.renascimentos);
        assert!(s.stats.attack_damage > sem.attack_damage, "the Mage nodes still count after the rebirth");
    }
}
