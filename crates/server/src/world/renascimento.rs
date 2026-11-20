//! REBIRTH in the world (`shared::renascimento`, docs/RESET.md, phase 1).
//!
//! `ClientMessage::Renascer` at the level cap: back to level 1 with the
//! character's things, +5% to the attributes for good, Essence and the
//! rebirth chest, and the milestone rewards (bag/bank slots, a global
//! notice). The rules and numbers live in `shared::renascimento`; this file
//! only applies them to a session.

use super::*;
use shared::renascimento as rn;

use shared::renascimento::MOTIVO_CORREIO_RENASCIMENTO;

/// Why a session cannot be reborn now (`None` = it can). English: the client
/// translates.
fn motivo_da_recusa(w: &GameWorld, s: &Session) -> Option<String> {
    let nivel = shared::level_of_xp_with_mult(s.xp, crate::economy::xp_multiplier());
    if !rn::pode_renascer(nivel) {
        return Some(format!(
            "You can only be reborn at level {}.",
            shared::progressao::NIVEL_MAX_EXIBIDO
        ));
    }
    if s.downed {
        return Some("You cannot be reborn while downed.".into());
    }
    if w.tutorial_mode
        || w.dungeon_mode
        || s.instancia != 0
        || s.tutorial_slot.is_some()
        || s.dungeon_slot.is_some()
        || shared::arena::e_arena(&w.zona)
    {
        return Some("You cannot be reborn here.".into());
    }
    None
}

impl GameWorld {
    pub(super) fn handle_renascer(&mut self, sid: SessionId) {
        let Some(s) = self.sessions.get(&sid).filter(|s| s.logged_in) else {
            return;
        };
        if let Some(motivo) = motivo_da_recusa(self, s) {
            self.send_chat_to(sid, &motivo);
            return;
        }
        let entity = s.entity;
        let quando = (now_ms() / 1000) as i64;
        let Some(s) = self.sessions.get_mut(&sid) else {
            return;
        };
        let nivel_antes = shared::level_of_xp_with_mult(s.xp, crate::economy::xp_multiplier());
        let n = s.renascimentos.saturating_add(1);

        // ── What resets: level, XP, the allocated points (their Energy comes
        // back: the points are taken, not returned, and paying the ladder
        // again from zero with nothing back would cost ~80k Energy a climb),
        // SP to a new character's, and the recoverable deaths (XP of the old
        // climb).
        let total: u32 = s.allocated_points.iter().sum();
        let energia: u64 = (0..total).map(shared::custo_energia_do_ponto).sum();
        s.skill_progress.energia = s.skill_progress.energia.saturating_add(energia);
        s.xp = 0;
        s.unspent_points = 0;
        s.allocated_points = [0; shared::STAT_COUNT];
        s.skill_points_earned = 0;
        s.skill_points_spent = 0;
        s.last_level = 1;
        s.mortes.clear();
        s.renascimentos = n;

        // ── Rewards: Essence and the chest, to the bag; what does not fit
        // goes to the Mail.
        let mut premios = rn::bau_do_renascimento(n);
        let mut essencia = rn::essencia_do_renascimento(n);
        let mut avisos: Vec<String> = Vec::new();
        // Milestone slots: one expansion (+10, `armazem::PASSO`). Already at
        // the cap = 2x this rebirth's Essence instead.
        for (marco, banco) in [(rn::MARCO_BOLSA, false), (rn::MARCO_BANCO, true)] {
            if n != marco {
                continue;
            }
            let extra = if banco { s.banco_extra } else { s.bolsa_extra };
            if extra >= shared::armazem::expansoes_max(banco) {
                let mais = rn::essencia_do_renascimento(n) * 2;
                essencia += mais;
                avisos.push(format!(
                    "Already at the maximum size: {mais} extra Rebirth Essence instead."
                ));
                continue;
            }
            let novo = extra + 1;
            let tam = shared::armazem::tamanho(banco, novo);
            if banco {
                s.banco_extra = novo;
                no_tamanho(&mut s.vault, tam);
                s.vault_dirty = true;
                avisos.push(format!("+10 bank slots: your bank now has {tam} slots."));
            } else {
                s.bolsa_extra = novo;
                let ouro = arrumar_bolsa(&mut s.inventory, tam);
                s.gold = s.gold.saturating_add(ouro);
                avisos.push(format!("+10 bag slots: your bag now has {tam} slots."));
            }
            let _ = s.handle.to_client.send(ServerMessage::Armazem {
                bolsa_extra: s.bolsa_extra,
                banco_extra: s.banco_extra,
            });
        }
        premios.push((shared::item_id::ESSENCIA_RENASCIMENTO, essencia));
        let mut no_correio = 0u32;
        for (item, qtd) in &premios {
            crate::telemetria::conta_de(&s.name, "renascimento_item", item, *qtd as i64);
            // All or nothing: `add_to_inventory` can fill part of a stack and
            // still say no, and mailing the whole amount then would pay twice.
            let mut tentativa = s.inventory.clone();
            if add_to_inventory(&mut tentativa, *item, *qtd, None) {
                s.inventory = tentativa;
            } else {
                s.dungeon
                    .postar(*item, *qtd, None, MOTIVO_CORREIO_RENASCIMENTO, quando);
                no_correio += 1;
            }
        }
        s.inventory_dirty = true;

        // ── The new character: stats with the new bonus, full health.
        s.stats = effective_stats_rv(
            &s.equipment,
            &s.allocated_points,
            &s.proficiencies,
            s.xp,
            s.renascimentos,
            &s.arvore,
        );
        s.mp_current = s.stats.mp_max as f32;
        s.stamina_current = s.stats.stamina_max as f32;
        s.poise_current = s.stats.poise_max as f32;
        s.stats_dirty = true;
        s.stat_points_dirty = true;
        s.skills_dirty = true;
        s.nivel_mudou = true;
        let _ = s
            .handle
            .to_client
            .send(ServerMessage::ProgressUpdate { xp: 0, level: 1 });
        let _ = s.handle.to_client.send(ServerMessage::ProgressoDeSkills {
            progresso: s.skill_progress.clone(),
        });
        let _ = s.handle.to_client.send(ServerMessage::Renascimento {
            renascimentos: n,
            festa: true,
        });
        // The path tree opens on the first rebirth; its allocation stays. A
        // rebirth that opens a skill slot (1, 3, 5) fills it with a learned
        // active not equipped yet (phase 4).
        s.arvore.preencher_slots(n);
        s.enviar_arvore();
        let chat = |s: &Session, t: String| {
            let _ = s.handle.to_client.send(ServerMessage::Chat {
                from: "System".into(),
                text: t,
            });
        };
        chat(
            s,
            format!(
                "Rebirth {n}! Back to level 1, with +{}% to every attribute for good.",
                rn::bonus_em_pct(n)
            ),
        );
        let (ds, chaves, material) = (premios[0].1, premios[1].1, premios[2].1);
        chat(s, format!(
            "Rebirth chest: {ds} Darksteel, {chaves} crafting keys, {material} crafting materials and {essencia} Rebirth Essence."
        ));
        if energia > 0 {
            chat(
                s,
                format!("Your attribute points were reset: {energia} Energy refunded."),
            );
        }
        for a in avisos {
            chat(s, a);
        }
        if no_correio > 0 {
            chat(
                s,
                "Your bag was full: part of the rebirth chest went to the Mail.".into(),
            );
        }
        let nome = s.name.clone();
        tracing::info!(
            "[renascimento] {nome} renasceu ({n}) do nivel {nivel_antes}: {energia} Energia devolvida, premios {premios:?} ({no_correio} no correio)"
        );
        crate::telemetria::conta_de(&nome, "renascimento", n, 1);

        if let Some(e) = entity {
            if let Ok(mut h) = self.ecs.get::<&mut Health>(e) {
                h.max = self.sessions[&sid].stats.hp_max;
                h.current = h.max;
            }
        }
        self.save_pending = true;
        if rn::e_marco(n) {
            self.anuncia_renascimento(&nome, n);
        }
    }

    /// The milestone notice to everyone online on EVERY channel: through the
    /// announcements table (`avisos_globais`), which each process polls.
    /// Without a database (tests), straight to this channel's players.
    fn anuncia_renascimento(&mut self, nome: &str, n: u32) {
        let texto = format!("{nome} reached Rebirth {n}!");
        match self.auth_ctx.as_ref().map(|c| c.pool.clone()) {
            Some(pool) => {
                tokio::spawn(async move {
                    let r = sqlx::query(
                        "INSERT INTO avisos_globais (texto, autor) VALUES ($1, 'renascimento')",
                    )
                    .bind(&texto)
                    .execute(&pool)
                    .await;
                    if let Err(e) = r {
                        tracing::warn!("renascimento: aviso global falhou: {e}");
                    }
                });
            }
            None => self.aviso_global(&texto),
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
        let sid = SessionId(([127, 0, 0, 1], 47_001).into());
        let (tx, rx) = mpsc::unbounded_channel();
        // The receiver must live, or every send fails silently (fine) — keep it.
        std::mem::forget(rx);
        w.on_connect(SessionHandle {
            id: sid,
            to_client: tx,
        });
        let eid = EntityId(4_700);
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
                name: "Renato".into(),
                player_id: PlayerId(47),
                attack_anim_pending: None,
                combo_step_pending: None,
            },
        ));
        let s = w.sessions.get_mut(&sid).unwrap();
        s.logged_in = true;
        s.entity = Some(e);
        s.entity_id = eid;
        s.name = "Renato".into();
        (w, sid)
    }

    fn no_teto(w: &mut GameWorld, sid: SessionId) {
        let mult = crate::economy::xp_multiplier();
        let s = w.sessions.get_mut(&sid).unwrap();
        s.xp = shared::progressao::xp_maximo(mult);
        s.last_level = shared::progressao::NIVEL_MAX;
        s.unspent_points = 7;
        s.allocated_points = [40, 30, 20, 10, 0, 5];
        s.skill_points_earned = 59;
        s.skill_points_spent = 12;
        s.gold = 12_345;
        s.equipment.weapon = Some(shared::item_id::ESPADA_E_ESCUDO);
        add_to_inventory(&mut s.inventory, shared::item_id::XP_POTION, 3, None);
    }

    fn tem(s: &Session, item: u16) -> u32 {
        s.inventory
            .iter()
            .filter(|i| i.item_id == item && i.qty > 0)
            .map(|i| i.qty)
            .sum()
    }

    #[test]
    fn no_teto_renasce_no_nivel_1_e_guarda_os_itens() {
        let (mut w, sid) = mundo_com_um();
        no_teto(&mut w, sid);
        let energia_antes = w.sessions[&sid].skill_progress.energia;
        let ds_antes = tem(&w.sessions[&sid], shared::item_id::DARKSTEEL);
        w.on_message(sid, ClientMessage::Renascer);
        let s = &w.sessions[&sid];
        let mult = crate::economy::xp_multiplier();
        assert_eq!(s.renascimentos, 1);
        assert_eq!(shared::level_of_xp_with_mult(s.xp, mult), 1);
        assert_eq!(s.last_level, 1);
        assert_eq!(s.unspent_points, 0);
        assert_eq!(s.allocated_points, [0; shared::STAT_COUNT]);
        assert_eq!((s.skill_points_earned, s.skill_points_spent), (0, 0));
        assert!(s.nivel_mudou, "nameplates refresh");
        // Kept: gold, gear, bag items.
        assert_eq!(s.gold, 12_345);
        assert_eq!(s.equipment.weapon, Some(shared::item_id::ESPADA_E_ESCUDO));
        assert_eq!(tem(s, shared::item_id::XP_POTION), 3);
        // Paid: Essence, Darksteel and the chest; the points' Energy back.
        // (The test economy stacks by 1, so part of it may go to the Mail.)
        let no_correio: u32 = s
            .dungeon
            .correio
            .iter()
            .filter(|c| c.item_id == shared::item_id::ESSENCIA_RENASCIMENTO)
            .map(|c| c.qtd)
            .sum();
        assert!(s
            .dungeon
            .correio
            .iter()
            .all(|c| c.motivo == MOTIVO_CORREIO_RENASCIMENTO));
        assert_eq!(
            tem(s, shared::item_id::ESSENCIA_RENASCIMENTO) + no_correio,
            rn::essencia_do_renascimento(1)
        );
        assert!(tem(s, shared::item_id::DARKSTEEL) > ds_antes);
        assert!(s.skill_progress.energia > energia_antes);
        // +5%: the same naked level-1 character with no rebirth is weaker.
        let sem = effective_stats(&s.equipment, &s.allocated_points, &s.proficiencies, s.xp);
        assert!(s.stats.hp_max > sem.hp_max && s.stats.attack_damage >= sem.attack_damage);
    }

    #[test]
    fn abaixo_do_teto_recusa() {
        let (mut w, sid) = mundo_com_um();
        let mult = crate::economy::xp_multiplier();
        {
            let s = w.sessions.get_mut(&sid).unwrap();
            s.xp = shared::xp_for_level_with_mult(shared::progressao::NIVEL_MAX - 1, mult);
            s.allocated_points = [10, 0, 0, 0, 0, 0];
        }
        let xp = w.sessions[&sid].xp;
        w.on_message(sid, ClientMessage::Renascer);
        let s = &w.sessions[&sid];
        assert_eq!(s.renascimentos, 0);
        assert_eq!(s.xp, xp);
        assert_eq!(s.allocated_points[0], 10);
        assert_eq!(tem(s, shared::item_id::ESSENCIA_RENASCIMENTO), 0);
    }

    #[test]
    fn caido_ou_em_instancia_recusa() {
        let (mut w, sid) = mundo_com_um();
        no_teto(&mut w, sid);
        w.sessions.get_mut(&sid).unwrap().downed = true;
        w.on_message(sid, ClientMessage::Renascer);
        assert_eq!(w.sessions[&sid].renascimentos, 0);
        let s = w.sessions.get_mut(&sid).unwrap();
        s.downed = false;
        s.instancia = 9;
        w.on_message(sid, ClientMessage::Renascer);
        assert_eq!(w.sessions[&sid].renascimentos, 0);
    }

    /// The second rebirth grows the bag; the XP afterwards is divided.
    #[test]
    fn segundo_renascimento_aumenta_a_bolsa_e_o_divisor() {
        let (mut w, sid) = mundo_com_um();
        no_teto(&mut w, sid);
        w.on_message(sid, ClientMessage::Renascer);
        no_teto(&mut w, sid);
        let bolsa = w.sessions[&sid].bolsa_extra;
        w.on_message(sid, ClientMessage::Renascer);
        let s = w.sessions.get_mut(&sid).unwrap();
        assert_eq!(s.renascimentos, 2);
        assert_eq!(s.bolsa_extra, bolsa + 1);
        s.xp_bonus_ate = 0;
        s.grant_xp(900);
        assert_eq!(
            s.xp, 643,
            "900 / 1.4 (R2 on the linear part of the divisor)"
        );
    }
}
