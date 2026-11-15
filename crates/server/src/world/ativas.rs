//! THE PATH TREE'S ACTIVES in skill slots 4–6 (`shared::arvore::DEFS`,
//! docs/RESET.md, rebirth phase 4).
//!
//! `ClientMessage::UsarAtiva { slot }`: the active in that extra slot goes
//! off right away (no cast time, no body lock — they are the tree's extra
//! buttons, not a second weapon rotation). The server checks the slot is
//! open for the rebirths (`arvore::slots_abertos`), the active learned, the
//! mana, the cooldown (`skill_cds` under `arvore::id_de_skill`, so the
//! client reads it from the usual `SkillsState`) and, for the strikes, the
//! target with the weapon skills' own rules (`posicao_alvo_skill`,
//! `alvos_da_habilidade`: PvP, safe zone, line of sight). Damage goes
//! through `pending_skill_hits`, heals through `pending_heals`, the shield
//! is the Bulwark window (`muralha_ate`), Taunt the monsters' forced aggro,
//! and the Corsair buffs add to `bonus_economia` (`buffs_da_arvore`).
//!
//! INVISIBILITY: `Session::invisivel_ate` + the `Untargetable` tag (the one
//! the downed state uses, which the monster AI already skips: they lose the
//! target the same tick), and `can_damage_player` refuses an invisible
//! target. Attacking (`ir.wants_attack`), any weapon skill or an offensive
//! active ends it (`quebrar_invisibilidade`).

use super::*;
use shared::arvore::{self as av, TipoAtiva};
use shared::skills::{Conjunto, Forma, Skill};

/// A circle (or the target check) dressed as a `Skill`, so the targets come
/// from the same functions the weapon skills use.
fn forma_de_area(raio: f32, alcance: f32) -> Skill {
    Skill {
        id: 0,
        nome: String::new(),
        conjunto: Conjunto::EspadaEscudo,
        ordem: 0,
        forma: Forma::Circulo,
        custo_mp: 0,
        espera_s: 0.0,
        conjuracao_s: 0.0,
        dano: 1,
        cura: 0,
        alcance,
        raio,
    }
}

/// Does this active end invisibility when used (it hits or pulls)?
fn e_ofensiva(t: TipoAtiva) -> bool {
    matches!(t, TipoAtiva::Golpe | TipoAtiva::Nova | TipoAtiva::Meteoro | TipoAtiva::Provocar)
}

/// How far a Shadowstep goes and where it stops: toward `alvo` (one body
/// short of it) or `alcance` along `dir`.
fn destino_do_passo(de: Vec2, alvo: Option<Vec2>, dir: Vec2, alcance: f32) -> Vec2 {
    match alvo {
        Some(a) => {
            let d = (a - de).length();
            de + (a - de).normalize_or_zero() * (d - 1.0).clamp(0.0, alcance)
        }
        None => de + dir.normalize_or_zero() * alcance,
    }
}

impl GameWorld {
    pub(super) fn handle_usar_ativa(&mut self, sid: SessionId, slot: u8) {
        let agora = self.sim_time_s;
        let Some(s) = self.sessions.get(&sid).filter(|s| s.logged_in) else {
            return;
        };
        let Some(ativa) = s.arvore.ativa_no_slot(slot, s.renascimentos) else {
            let motivo = if slot as usize >= av::slots_abertos(s.renascimentos) {
                av::SLOT_FECHADO
            } else {
                "No active in this skill slot: equip one from the path tree."
            };
            self.rejeita_skill(sid, av::ID_SKILL_BASE, motivo);
            return;
        };
        let id = av::id_de_skill(ativa);
        let def = *av::def(ativa).expect("every ATIVAS id has a DEF");
        let r = s.renascimentos;
        let fator = av::fator(ativa, r);
        let suprema = av::e_suprema(ativa, r);
        let ocupado = s.downed
            || s.entity.is_none()
            || s.carrying.is_some()
            || s.carried_by.is_some()
            || agora < s.casting_until
            || agora < s.dash_until
            || agora < s.leap_until;
        if ocupado {
            self.rejeita_skill(sid, id, "Wait for the current action to finish.");
            return;
        }
        if s.skill_cds.get(&id).is_some_and(|&fim| fim > agora) {
            self.rejeita_skill(sid, id, "Skill on cooldown.");
            return;
        }
        if s.mp_current < def.custo_mp as f32 {
            self.rejeita_skill(sid, id, "Not enough mana.");
            return;
        }
        let entity = s.entity.unwrap();
        let dono = s.entity_id;
        let Some(de) = self.ecs.get::<&Position>(entity).ok().map(|p| p.0) else {
            return;
        };
        if e_ofensiva(def.tipo) && self.in_safe_zone(de) {
            self.rejeita_skill(sid, id, "You cannot attack in the safe zone.");
            return;
        }
        let (atk, crit_chance, hp_max) = (s.stats.attack_damage, s.stats.crit_chance, s.stats.hp_max);
        let selecionado = s.target;

        // The strikes need a valid enemy in reach, checked BEFORE paying.
        let alvo_do_golpe = if matches!(def.tipo, TipoAtiva::Golpe | TipoAtiva::Meteoro) {
            let Some(alvo) = selecionado else {
                self.rejeita_skill(sid, id, "Select an enemy to use this skill.");
                return;
            };
            match self.posicao_alvo_skill(dono, alvo, de, &forma_de_area(def.raio, def.alcance)) {
                Ok(p) => Some((alvo, p)),
                Err(m) => {
                    self.rejeita_skill(sid, id, m);
                    return;
                }
            }
        } else {
            None
        };
        // Shadowstep: toward the selected target if there is a live one in
        // reach, else where the body is moving.
        let destino_do_passo = if def.tipo == TipoAtiva::Passo {
            let alvo = selecionado
                .filter(|&a| a != dono)
                .and_then(|a| {
                    self.ecs
                        .query::<(&NetId, &Position, &Health)>()
                        .iter()
                        .find(|(_, (n, _, h))| n.0 == a && h.current > 0)
                        .map(|(_, (_, p, _))| p.0)
                })
                .filter(|p| p.distance(de) <= def.alcance + 1.0);
            let dir = self.ecs.get::<&Velocity>(entity).map(|v| v.0).unwrap_or(Vec2::ZERO);
            if alvo.is_none() && dir.length_squared() < 1e-4 {
                self.rejeita_skill(sid, id, "Select a target or move to choose where to step.");
                return;
            }
            Some(destino_do_passo(de, alvo, dir, def.alcance))
        } else {
            None
        };

        // ── pay ──
        if def.tipo != TipoAtiva::Saque {
            self.desmontar(sid);
        }
        if e_ofensiva(def.tipo) {
            self.quebrar_invisibilidade(sid);
        }
        let s = self.sessions.get_mut(&sid).unwrap();
        s.mp_current -= def.custo_mp as f32;
        let espera = shared::skills::espera_efetiva(def.espera_s, s.stats.wis);
        s.skill_cds.insert(id, agora + espera);
        if e_ofensiva(def.tipo) {
            s.last_combat_at_s = agora;
        }
        let nome = s.name.clone();

        // ── the effect ──
        let mut alvo_fx = de;
        let mut duracao_fx = 0.0;
        match def.tipo {
            TipoAtiva::Passo => {
                let destino = destino_do_passo.unwrap_or(de);
                let mut pos = de;
                // The walk's own collision, in small steps: no wall crossing.
                let passos = 24;
                let dt = 0.05;
                let vel = (destino - de) / (passos as f32 * dt);
                for _ in 0..passos {
                    let proxima = match &self.ilha {
                        Some(i) => i.mover_com_degrau(pos, vel, dt, ENTITY_RADIUS, shared::terreno::DEGRAU_BLOCOS),
                        None => self.map.move_and_slide(pos, vel, dt, ENTITY_RADIUS),
                    };
                    if proxima.distance(pos) < 1e-4 {
                        break;
                    }
                    pos = proxima;
                }
                if let Ok(mut p) = self.ecs.get::<&mut Position>(entity) {
                    p.0 = pos;
                }
                if let Some(s) = self.sessions.get_mut(&sid) {
                    s.rota.limpa();
                }
                alvo_fx = pos;
            }
            TipoAtiva::Invisibilidade => {
                duracao_fx = def.duracao_s;
                self.comecar_invisibilidade(sid, def.duracao_s);
            }
            TipoAtiva::Golpe => {
                let (alvo, p) = alvo_do_golpe.expect("checked above");
                let dano = super::golpe_critico(((atk as f32) * fator).round().max(1.0) as i32);
                self.pending_skill_hits.push(PendingSkillHit {
                    target_net: alvo,
                    damage: dano,
                    attacker_net: dono,
                    hurt_dir: (de - p).normalize_or_zero(),
                    is_crit: true,
                    from_player: true,
                    knockback: 0.0,
                    drain_tier: 0,
                });
                alvo_fx = p;
            }
            TipoAtiva::Nova | TipoAtiva::Meteoro => {
                let centro = alvo_do_golpe.map_or(de, |(_, p)| p);
                let area = forma_de_area(def.raio, def.alcance);
                let dir = (centro - de).try_normalize().unwrap_or(Vec2::Y);
                let alvos = self.alvos_da_habilidade(dono, de, dir, centro, &area);
                let crit = crit_chance > 0.0 && fastrand::f32() < crit_chance;
                let base = ((atk as f32) * fator).round().max(1.0) as i32;
                let dano = if crit { super::golpe_critico(base) } else { base };
                for &(net, p) in &alvos {
                    self.pending_skill_hits.push(PendingSkillHit {
                        target_net: net,
                        damage: dano,
                        attacker_net: dono,
                        hurt_dir: (de - p).normalize_or_zero(),
                        is_crit: crit,
                        from_player: true,
                        knockback: if def.tipo == TipoAtiva::Nova { 0.5 } else { 0.0 },
                        drain_tier: 0,
                    });
                }
                if def.tipo == TipoAtiva::Meteoro {
                    // Stuns monsters, never a boss (its telegraphs would stop).
                    let ate = agora + def.duracao_s;
                    let atingidos: std::collections::HashSet<EntityId> = alvos.iter().map(|a| a.0).collect();
                    for (_, (net, tag)) in self.ecs.query_mut::<(&NetId, &mut EnemyTag)>() {
                        if atingidos.contains(&net.0) && !tag.is_boss && tag.stunned_until < ate {
                            tag.stunned_until = ate;
                        }
                    }
                }
                alvo_fx = centro;
            }
            TipoAtiva::Tempo => {
                if let Some(s) = self.sessions.get_mut(&sid) {
                    for (&k, fim) in s.skill_cds.iter_mut() {
                        if (1..=12).contains(&k) && *fim > agora {
                            *fim = agora + (*fim - agora) * (1.0 - fator.clamp(0.0, 1.0));
                        }
                    }
                }
            }
            TipoAtiva::Provocar => {
                duracao_fx = def.duracao_s;
                let area = forma_de_area(def.raio, 0.0);
                let alvos = self.alvos_da_habilidade(dono, de, Vec2::Y, de, &area);
                let mobs: std::collections::HashSet<EntityId> = alvos.iter().map(|a| a.0).collect();
                let ate = agora + def.duracao_s;
                for (_, (net, tag)) in self.ecs.query_mut::<(&NetId, &mut EnemyTag)>() {
                    if mobs.contains(&net.0) {
                        tag.forced_aggro_target = Some(dono);
                        tag.forced_aggro_until = ate;
                        tag.ai_target = Some(dono);
                    }
                }
                if let Some(s) = self.sessions.get_mut(&sid) {
                    s.muralha_ate = s.muralha_ate.max(agora + 2.0);
                }
            }
            TipoAtiva::Escudo | TipoAtiva::Inquebravel => {
                duracao_fx = def.duracao_s;
                if let Some(s) = self.sessions.get_mut(&sid) {
                    s.muralha_ate = s.muralha_ate.max(agora + def.duracao_s);
                    if def.tipo == TipoAtiva::Inquebravel {
                        s.poise_current = s.stats.poise_max as f32;
                    }
                }
                let cura = ((hp_max as f32) * fator).round().max(1.0) as i32;
                self.pending_heals.push(PendingHeal { target_net: dono, amount: cura });
            }
            TipoAtiva::Saque => {
                duracao_fx = def.duracao_s;
                if let Some(s) = self.sessions.get_mut(&sid) {
                    s.buffs_da_arvore.retain(|(a, _)| *a != ativa);
                    s.buffs_da_arvore.push((ativa, agora + def.duracao_s));
                }
            }
        }
        tracing::debug!("[ativa] {nome} usou {}", av::nome_da_ativa(ativa));
        crate::telemetria::conta("ativa_usada", ativa, 1);
        self.estado_skills(sid);
        for s in self.sessions.values().filter(|s| s.logged_in) {
            let _ = s.handle.to_client.send(ServerMessage::AtivaFx {
                ativa,
                caster_eid: dono,
                de,
                alvo: alvo_fx,
                duracao_s: duracao_fx,
                suprema,
            });
        }
    }

    /// INVISIBILITY on for `segundos`: the body becomes `Untargetable` (the
    /// monster AI drops it the same tick) and the monsters that had it as
    /// target or taunt forget it now.
    pub(super) fn comecar_invisibilidade(&mut self, sid: SessionId, segundos: f32) {
        let agora = self.sim_time_s;
        let Some(s) = self.sessions.get_mut(&sid) else {
            return;
        };
        s.invisivel_ate = agora + segundos;
        let (dono, entity) = (s.entity_id, s.entity);
        if let Some(e) = entity {
            let _ = self.ecs.insert_one(e, Untargetable);
        }
        for (_, tag) in self.ecs.query_mut::<&mut EnemyTag>() {
            if tag.ai_target == Some(dono) {
                tag.ai_target = None;
            }
            if tag.forced_aggro_target == Some(dono) {
                tag.forced_aggro_target = None;
                tag.forced_aggro_until = 0.0;
            }
        }
        self.avisar_invisibilidade(dono, segundos);
    }

    fn avisar_invisibilidade(&self, eid: EntityId, segundos: f32) {
        for s in self.sessions.values().filter(|s| s.logged_in) {
            let _ = s.handle.to_client.send(ServerMessage::Invisibilidade { eid, segundos });
        }
    }

    /// INVISIBILITY off (attack, skill, timeout). A downed body keeps its
    /// `Untargetable` (the downed state owns it then).
    pub(super) fn quebrar_invisibilidade(&mut self, sid: SessionId) {
        let Some(s) = self.sessions.get_mut(&sid).filter(|s| s.invisivel_ate > 0.0) else {
            return;
        };
        s.invisivel_ate = 0.0;
        let (dono, entity, caido) = (s.entity_id, s.entity, s.downed);
        if let (Some(e), false) = (entity, caido) {
            let _ = self.ecs.remove_one::<Untargetable>(e);
        }
        self.avisar_invisibilidade(dono, 0.0);
    }

    /// The same, from the body (the basic attack's input result).
    pub(super) fn quebrar_invisibilidade_da_entidade(&mut self, e: Entity) {
        let sid = self
            .sessions
            .iter()
            .find(|(_, s)| s.entity == Some(e) && s.invisivel_ate > 0.0)
            .map(|(id, _)| *id);
        if let Some(sid) = sid {
            self.quebrar_invisibilidade(sid);
        }
    }

    /// Every tick: invisibility running out (and kept on the body while it
    /// lasts — a revive takes `Untargetable` off), Corsair buffs expiring.
    pub(super) fn avancar_ativas(&mut self) {
        let agora = self.sim_time_s;
        let mut acabou = Vec::new();
        let mut manter = Vec::new();
        for (sid, s) in self.sessions.iter_mut() {
            if s.invisivel_ate > 0.0 {
                if agora >= s.invisivel_ate || !s.logged_in {
                    acabou.push(*sid);
                } else if let Some(e) = s.entity {
                    manter.push(e);
                }
            }
            if !s.buffs_da_arvore.is_empty() {
                s.buffs_da_arvore.retain(|&(_, fim)| fim > agora);
            }
        }
        for e in manter {
            if self.ecs.get::<&Untargetable>(e).is_err() {
                let _ = self.ecs.insert_one(e, Untargetable);
            }
        }
        for sid in acabou {
            self.quebrar_invisibilidade(sid);
        }
    }
}

#[cfg(test)]
mod testes {
    use super::*;
    use shared::arvore::PedidoArvore;

    /// One player at (10, 10) with `r` rebirths, the tree `nos` and the
    /// actives `slots`, 100 mana, attack 60; a dummy enemy (999) at (12, 10).
    fn mundo(r: u32, nos: [u8; 4], slots: [Option<u8>; 3]) -> (GameWorld, SessionId, EntityId) {
        crate::economy::init_vazia_para_testes();
        let mut w = GameWorld::new(HashMap::new());
        w.map = shared::mapfile::MapFile::new("ativas-test", 40, 40).to_world_map();
        let sid = SessionId(([127, 0, 0, 1], 47_301).into());
        let (tx, rx) = mpsc::unbounded_channel();
        std::mem::forget(rx);
        w.on_connect(SessionHandle { id: sid, to_client: tx });
        let s = w.sessions.get_mut(&sid).unwrap();
        s.logged_in = true;
        s.name = "Ativo".into();
        s.renascimentos = r;
        s.xp = shared::progressao::xp_do_nivel_exibido(100, crate::economy::xp_multiplier());
        s.arvore = av::Arvore { nos, slots };
        s.equipment.weapon = Some(shared::item_id::ESPADA_E_ESCUDO);
        s.mp_current = 100.0;
        s.stats.mp_max = 100;
        s.stats.hp_max = 200;
        s.stats.attack_damage = 60;
        let dono = s.entity_id;
        let e = w.ecs.spawn((
            NetId(dono),
            Position(Vec2::splat(10.0)),
            Velocity(Vec2::ZERO),
            EntityKind::Player,
            Health { current: 100, max: 200 },
            PlayerTag { name: "Ativo".into(), player_id: PlayerId(73),
                attack_anim_pending: None, combo_step_pending: None },
        ));
        let s = w.sessions.get_mut(&sid).unwrap();
        s.entity = Some(e);
        s.target = Some(EntityId(999));
        w.ecs.spawn((
            NetId(EntityId(999)),
            Position(Vec2::new(12.0, 10.0)),
            EntityKind::Enemy(0),
            Health { current: 100, max: 100 },
        ));
        (w, sid, dono)
    }

    fn usar(w: &mut GameWorld, sid: SessionId, slot: u8) {
        w.on_message(sid, ClientMessage::UsarAtiva { slot });
    }

    #[test]
    fn slot_fechado_ou_vazio_nao_faz_nada() {
        // Arcane Nova learned and stored in slot 4, but no rebirth: locked.
        let (mut w, sid, _) = mundo(0, [0, 5, 0, 0], [Some(3), None, None]);
        usar(&mut w, sid, 0);
        assert_eq!(w.sessions[&sid].mp_current, 100.0);
        assert!(w.pending_skill_hits.is_empty());
        // Rebirth 1: slot 4 works, slot 5 is still locked, slot 6 too.
        let (mut w, sid, _) = mundo(1, [0, 5, 0, 0], [None, Some(3), None]);
        usar(&mut w, sid, 1);
        usar(&mut w, sid, 0);
        usar(&mut w, sid, 7);
        assert_eq!(w.sessions[&sid].mp_current, 100.0, "nothing went off");
        assert!(w.sessions[&sid].skill_cds.is_empty());
    }

    #[test]
    fn usar_custa_mana_e_respeita_a_recarga() {
        let (mut w, sid, _) = mundo(1, [0, 5, 0, 0], [Some(3), None, None]);
        let def = av::DEFS[3];
        usar(&mut w, sid, 0);
        let s = &w.sessions[&sid];
        assert_eq!(s.mp_current, 100.0 - def.custo_mp as f32);
        let fim = s.skill_cds[&av::id_de_skill(3)];
        assert!(fim > w.sim_time_s + 1.0);
        let golpe = w.pending_skill_hits.iter().find(|h| h.target_net == EntityId(999));
        assert!(golpe.is_some_and(|h| h.damage >= 78), "130% of 60 attack");
        // On cooldown: refused, nothing paid, no new hit.
        let hits = w.pending_skill_hits.len();
        usar(&mut w, sid, 0);
        assert_eq!(w.sessions[&sid].mp_current, 100.0 - def.custo_mp as f32);
        assert_eq!(w.pending_skill_hits.len(), hits);
        // After the cooldown it goes again.
        w.sim_time_s = fim + 0.01;
        usar(&mut w, sid, 0);
        assert_eq!(w.sessions[&sid].mp_current, 100.0 - 2.0 * def.custo_mp as f32);
        // Without the mana: refused.
        w.sim_time_s += 60.0;
        w.sessions.get_mut(&sid).unwrap().mp_current = 5.0;
        let hits = w.pending_skill_hits.len();
        usar(&mut w, sid, 0);
        assert_eq!(w.pending_skill_hits.len(), hits);
        assert_eq!(w.sessions[&sid].mp_current, 5.0);
    }

    #[test]
    fn golpe_precisa_de_alvo_e_nao_cobra_sem_ele() {
        // Death Mark (Assassin node 15).
        let (mut w, sid, _) = mundo(1, [15, 0, 0, 0], [Some(2), None, None]);
        w.sessions.get_mut(&sid).unwrap().target = None;
        usar(&mut w, sid, 0);
        assert_eq!(w.sessions[&sid].mp_current, 100.0);
        assert!(w.sessions[&sid].skill_cds.is_empty());
        w.sessions.get_mut(&sid).unwrap().target = Some(EntityId(999));
        usar(&mut w, sid, 0);
        let h = w.pending_skill_hits.iter().find(|h| h.target_net == EntityId(999)).expect("a hit");
        assert!(h.is_crit);
        assert_eq!(h.damage, super::super::golpe_critico(120));
    }

    #[test]
    fn comprar_no_ativo_encaixa_e_so_equipa_o_aprendido() {
        let (mut w, sid, _) = mundo(1, [0, 0, 0, 0], [None; 3]);
        for k in 0..5 {
            w.on_message(sid, ClientMessage::Arvore { pedido: PedidoArvore::Gastar { caminho: 0, no: k } });
        }
        assert_eq!(w.sessions[&sid].arvore.slots, [Some(0), None, None], "Shadowstep went to slot 4");
        // Arcane Nova is not learned: refused.
        w.on_message(sid, ClientMessage::Arvore { pedido: PedidoArvore::Equipar { slot: 0, ativa: Some(3) } });
        assert_eq!(w.sessions[&sid].arvore.slots, [Some(0), None, None]);
        // Slot 5 is locked at rebirth 1.
        w.on_message(sid, ClientMessage::Arvore { pedido: PedidoArvore::Equipar { slot: 1, ativa: Some(0) } });
        assert_eq!(w.sessions[&sid].arvore.slots, [Some(0), None, None]);
        // Emptying works, and so does putting it back.
        w.on_message(sid, ClientMessage::Arvore { pedido: PedidoArvore::Equipar { slot: 0, ativa: None } });
        assert_eq!(w.sessions[&sid].arvore.slots, [None; 3]);
        w.on_message(sid, ClientMessage::Arvore { pedido: PedidoArvore::Equipar { slot: 0, ativa: Some(0) } });
        assert_eq!(w.sessions[&sid].arvore.slots, [Some(0), None, None]);
        // The stored JSON carries the slot; a reset clears it.
        assert!(w.sessions[&sid].arvore.para_json().contains("slots"));
        w.sessions.get_mut(&sid).unwrap().gold = 1_000_000;
        w.on_message(sid, ClientMessage::Arvore { pedido: PedidoArvore::Redistribuir });
        assert_eq!(w.sessions[&sid].arvore.slots, [None; 3]);
    }

    #[test]
    fn invisibilidade_tira_o_aggro_e_quebra_ao_atacar() {
        // Invisibility (node 10) in slot 4, Arcane Nova in slot 5.
        let (mut w, sid, dono) = mundo(3, [10, 5, 0, 0], [Some(1), Some(3), None]);
        let pos = Vec2::new(13.0, 10.0);
        let (mut tag, mut hp) = w.build_enemy_tag(1, pos, 30.0, pos);
        hp.current = 100;
        hp.max = 100;
        tag.spawn_grace_until = 0.0;
        let mob = w.ecs.spawn((NetId(EntityId(777)), Position(pos), Velocity(Vec2::ZERO), EntityKind::Enemy(1), tag, hp));
        // Control: a visible player is the monster's target.
        w.step(shared::TICK_DT);
        assert_eq!(w.ecs.get::<&EnemyTag>(mob).unwrap().ai_target, Some(dono));

        usar(&mut w, sid, 0);
        let e = w.sessions[&sid].entity.unwrap();
        assert!(w.sessions[&sid].invisivel_ate > w.sim_time_s);
        assert!(w.ecs.get::<&Untargetable>(e).is_ok());
        assert_eq!(w.ecs.get::<&EnemyTag>(mob).unwrap().ai_target, None, "aggro dropped at once");
        for _ in 0..5 {
            w.step(shared::TICK_DT);
            assert_ne!(w.ecs.get::<&EnemyTag>(mob).unwrap().ai_target, Some(dono), "the monster cannot see it");
        }
        // An offensive active ends it.
        usar(&mut w, sid, 1);
        assert_eq!(w.sessions[&sid].invisivel_ate, 0.0);
        assert!(w.ecs.get::<&Untargetable>(e).is_err());

        // Again, and the basic attack's hook ends it too.
        w.sim_time_s += 100.0;
        usar(&mut w, sid, 0);
        assert!(w.sessions[&sid].invisivel_ate > 0.0);
        w.quebrar_invisibilidade_da_entidade(e);
        assert_eq!(w.sessions[&sid].invisivel_ate, 0.0);

        // And it runs out by itself.
        w.sim_time_s += 100.0;
        usar(&mut w, sid, 0);
        w.sim_time_s += av::DEFS[1].duracao_s + 0.1;
        w.avancar_ativas();
        assert_eq!(w.sessions[&sid].invisivel_ate, 0.0);
        assert!(w.ecs.get::<&Untargetable>(e).is_err());
    }

    #[test]
    fn invisivel_nao_toma_dano_de_jogador() {
        let (mut w, sid, dono) = mundo(1, [10, 0, 0, 0], [Some(1), None, None]);
        let outro = SessionId(([127, 0, 0, 1], 47_302).into());
        let (tx, rx) = mpsc::unbounded_channel();
        std::mem::forget(rx);
        w.on_connect(SessionHandle { id: outro, to_client: tx });
        let o = w.sessions.get_mut(&outro).unwrap();
        o.logged_in = true;
        let atacante = o.entity_id;
        let antes = w.can_damage_player(atacante, dono);
        usar(&mut w, sid, 0);
        assert!(!w.can_damage_player(atacante, dono), "no PvP on an invisible body");
        w.quebrar_invisibilidade(sid);
        assert_eq!(w.can_damage_player(atacante, dono), antes, "back to the usual PvP rules");
    }

    #[test]
    fn escudo_tempo_e_saque() {
        // Guardian: Bulwark Shield (node 10).
        let (mut w, sid, dono) = mundo(1, [0, 0, 10, 0], [Some(7), None, None]);
        usar(&mut w, sid, 0);
        assert!(w.sessions[&sid].muralha_ate > w.sim_time_s + 4.0);
        assert!(w.pending_heals.iter().any(|h| h.target_net == dono && h.amount == 16), "8% of 200");

        // Mage: Time Warp (node 15) halves the weapon skills' cooldowns.
        let (mut w, sid, _) = mundo(1, [0, 15, 0, 0], [Some(5), None, None]);
        w.sessions.get_mut(&sid).unwrap().skill_cds.insert(4, 10.0);
        usar(&mut w, sid, 0);
        assert!((w.sessions[&sid].skill_cds[&4] - 5.0).abs() < 1e-4);

        // Corsair: Treasure Sense adds drop while it lasts.
        let (mut w, sid, _) = mundo(1, [0, 0, 0, 5], [Some(9), None, None]);
        let antes = w.sessions[&sid].bonus_economia().drop;
        usar(&mut w, sid, 0);
        assert!((w.sessions[&sid].bonus_economia().drop - antes - 0.20).abs() < 1e-4);
        w.sim_time_s += 61.0;
        w.avancar_ativas();
        assert!((w.sessions[&sid].bonus_economia().drop - antes).abs() < 1e-4);
    }

    #[test]
    fn passo_sombrio_move_ate_o_alvo() {
        let (mut w, sid, _) = mundo(1, [5, 0, 0, 0], [Some(0), None, None]);
        for (_, (n, p)) in w.ecs.query_mut::<(&NetId, &mut Position)>() {
            if n.0 == EntityId(999) {
                p.0 = Vec2::new(15.0, 10.0);
            }
        }
        usar(&mut w, sid, 0);
        let e = w.sessions[&sid].entity.unwrap();
        let p = w.ecs.get::<&Position>(e).unwrap().0;
        assert!((p.x - 14.0).abs() < 0.3 && (p.y - 10.0).abs() < 0.3, "{p:?}");
        // Standing still with no target: refused, nothing paid.
        let (mut w, sid, _) = mundo(1, [5, 0, 0, 0], [Some(0), None, None]);
        w.sessions.get_mut(&sid).unwrap().target = None;
        usar(&mut w, sid, 0);
        assert_eq!(w.sessions[&sid].mp_current, 100.0);
    }
}
