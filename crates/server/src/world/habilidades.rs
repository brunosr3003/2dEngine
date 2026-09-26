//! As doze habilidades do playtest, com custo e efeito decididos no servidor.
use super::*;
use shared::skills::{Conjunto, Forma, Skill};

pub(super) struct HabilidadePendente {
    sid: SessionId,
    pub(super) dono: EntityId,
    skill: Skill,
    tier: u8,
    de: Vec2,
    alvo: Vec2,
    alvo_eid: EntityId,
    inicio: f32,
    impacto: f32,
}

fn valida(
    skill: &Skill,
    arma: Conjunto,
    nivel: u32,
    mp: f32,
    recarga: f32,
    ocupado: bool,
) -> Result<(), &'static str> {
    if skill.conjunto != arma {
        return Err("Esta skill pertence a outra arma.");
    }
    if !skill.destravada(nivel) {
        return Err("Seu nível ainda não libera esta skill.");
    }
    if ocupado {
        return Err("Aguarde terminar a ação atual.");
    }
    if recarga > 0.0 {
        return Err("Skill em recarga.");
    }
    if mp < skill.custo_mp as f32 {
        return Err("Mana insuficiente.");
    }
    Ok(())
}

/// Geometria dos despertares, fixada no início da conjuração.
fn skill_ajustada(mut skill: Skill, tier: u8) -> Skill {
    if tier >= 5 {
        match skill.id {
            2 | 4 | 6 | 8 => skill.alcance *= 1.15,
            5 | 9 | 11 | 12 => skill.raio *= 1.20,
            _ => {}
        }
    }
    if tier >= 8 {
        match skill.id {
            4 | 6 => skill.alcance *= 1.15,
            5 | 9 | 11 | 12 => skill.raio *= 1.10,
            _ => {}
        }
    }
    skill
}

fn dano_evoluido(base: i32, tier: u8, skill_id: u32) -> i32 {
    if base <= 0 {
        return 0;
    }
    let mut mult = shared::skills::multiplicador_do_tier(tier);
    if tier >= 5 && matches!(skill_id, 1 | 6 | 7) {
        mult += 0.08;
    }
    if tier >= 8 && matches!(skill_id, 5 | 7 | 9 | 12) {
        mult += 0.08;
    }
    if tier >= 10 && matches!(skill_id, 1 | 2 | 4 | 5 | 6 | 8 | 9 | 12) {
        mult += 0.12;
    }
    (base as f32 * mult).round().max(1.0) as i32
}

fn cura_evoluida(base: i32, tier: u8, skill_id: u32) -> i32 {
    if base <= 0 {
        return 0;
    }
    let mut mult = shared::skills::multiplicador_do_tier(tier);
    if tier >= 5 && skill_id == 10 {
        mult += 0.10;
    }
    if tier >= 8 && matches!(skill_id, 10 | 11) {
        mult += 0.10;
    }
    if tier >= 10 && matches!(skill_id, 10 | 11) {
        mult += 0.15;
    }
    (base as f32 * mult).round().max(1.0) as i32
}

impl GameWorld {
    pub(super) fn estado_skills(&self, sid: SessionId) {
        if let Some(s) = self.sessions.get(&sid) {
            let cooldowns = s
                .skill_cds
                .iter()
                .filter_map(|(&id, &fim)| {
                    (fim > self.sim_time_s).then_some((id, fim - self.sim_time_s))
                })
                .collect();
            let _ = s.handle.to_client.send(ServerMessage::SkillsState {
                cooldowns,
                busy_s: (s.casting_until - self.sim_time_s).max(0.0),
            });
        }
    }

    fn rejeita_skill(&self, sid: SessionId, id: u32, motivo: &str) {
        crate::telemetria::conta("skill_recusada", motivo, 1);
        if let Some(s) = self.sessions.get(&sid) {
            let _ = s.handle.to_client.send(ServerMessage::SkillRejected {
                skill_id: id,
                motivo: motivo.into(),
            });
        }
        self.estado_skills(sid);
    }

    pub(super) fn handle_skill_cast(&mut self, sid: SessionId, skill_id: u32) {
        crate::telemetria::conta("skill_pedida", skill_id, 1);
        let Some(skill) = crate::skills::skill_of(skill_id).filter(|s| (1..=12).contains(&s.id))
        else {
            self.rejeita_skill(sid, skill_id, "Skill indisponível.");
            return;
        };
        self.conjurar_skill(sid, skill);
    }

    /// A selecao fica presa ao cast, mesmo que o jogador troque de alvo depois.
    fn posicao_alvo_skill(
        &self,
        dono: EntityId,
        alvo: EntityId,
        de: Vec2,
        skill: &Skill,
    ) -> Result<Vec2, &'static str> {
        let mut q = self
            .ecs
            .query::<(&NetId, &Position, &EntityKind, &Health)>();
        let Some((e, (_, pos, kind, hp))) = q.iter().find(|(_, (n, _, _, _))| n.0 == alvo) else {
            return Err("O alvo não está mais disponível.");
        };
        if hp.current <= 0 {
            return Err("O alvo está morto.");
        }
        if skill.dano <= 0 {
            return Ok(pos.0);
        }
        if alvo == dono
            || !matches!(kind, EntityKind::Enemy(_) | EntityKind::Player)
            || (matches!(kind, EntityKind::Player) && !self.can_damage_player(dono, alvo))
            || self
                .ecs
                .get::<&EnemyTag>(e)
                .is_ok_and(|t| t.dead || t.returning_home || t.spawn_grace_until > self.sim_time_s)
        {
            return Err("Selecione um inimigo válido.");
        }
        if self.in_safe_zone(pos.0) {
            return Err("O alvo está na zona segura.");
        }
        if de.distance(pos.0) > skill.alcance_alvo() {
            return Err("Alvo fora do alcance.");
        }
        // Skill com alvo vai em arco, como o tiro basico (`visada_de_tiro`).
        if !visada_de_tiro(self.ilha.as_ref(), &self.map, de, pos.0) {
            return Err("O alvo está atrás de um obstáculo.");
        }
        Ok(pos.0)
    }

    fn conjurar_skill(&mut self, sid: SessionId, skill: Skill) {
        let skill_id = skill.id;
        let Some(s) = self.sessions.get(&sid) else {
            return;
        };
        let agora = self.sim_time_s;
        let ocupado = !s.logged_in
            || s.downed
            || s.entity.is_none()
            || s.carrying.is_some()
            || s.carried_by.is_some()
            || agora < s.stagger_until
            || agora < s.hurt_until
            || agora < s.casting_until
            || agora < s.dash_until
            || agora < s.leap_until;
        let nivel = shared::level_of_xp_with_mult(s.xp, crate::economy::xp_multiplier());
        let tier = s.skill_progress.tier(skill_id);
        let skill = skill_ajustada(skill, tier);
        if let Err(motivo) = valida(
            &skill,
            Conjunto::da_arma(s.equipment.weapon.unwrap_or(0)),
            nivel,
            s.mp_current,
            s.skill_cds.get(&skill_id).copied().unwrap_or(0.0) - agora,
            ocupado,
        ) {
            self.rejeita_skill(sid, skill_id, motivo);
            return;
        }
        let entity = s.entity.unwrap();
        let Ok(pos) = self.ecs.get::<&Position>(entity) else {
            return;
        };
        let de = pos.0;
        drop(pos);
        if skill.dano > 0 && self.in_safe_zone(de) {
            self.rejeita_skill(sid, skill_id, "Não é possível atacar na zona segura.");
            return;
        }
        let alvo_eid = if skill.dano > 0 {
            let Some(alvo) = s.target else {
                self.rejeita_skill(sid, skill_id, "Selecione um inimigo para usar esta skill.");
                return;
            };
            alvo
        } else {
            s.entity_id
        };
        let alvo = match self.posicao_alvo_skill(s.entity_id, alvo_eid, de, &skill) {
            Ok(pos) => pos,
            Err(motivo) => {
                self.rejeita_skill(sid, skill_id, motivo);
                return;
            }
        };
        // A desmontagem e o cast sao atomicos no servidor, antes de qualquer dano.
        self.desmontar(sid);
        let s = self.sessions.get_mut(&sid).unwrap();
        s.mp_current -= skill.custo_mp as f32;
        s.skill_cds.insert(skill_id, agora + skill.espera_s);
        s.casting_until = agora + skill.trava_s();
        s.casting_skill_id = skill_id;
        s.casting_started_at_s = agora;
        s.casting_impacto_em = skill.impacto_em();
        s.casting_mp_paid = skill.custo_mp as f32;
        s.casting_st_paid = 0.0;
        s.cast_movement_ticks = 0;
        s.gesto_skill_em = agora;
        s.gesto_skill_ordem = skill.ordem;
        s.combo_last_attack = 0.0;
        s.last_combat_at_s = agora;
        s.rota.limpa();
        let dono = s.entity_id;
        if let Ok(mut v) = self.ecs.get::<&mut Velocity>(entity) {
            v.0 = Vec2::ZERO;
        }
        self.pending_melee.retain(|g| g.attacker_eid != dono);
        self.pending_habilidades.push(HabilidadePendente {
            sid,
            dono,
            de,
            alvo,
            alvo_eid,
            inicio: agora,
            impacto: agora + skill.impacto_em(),
            skill,
            tier,
        });
        self.estado_skills(sid);
        for s in self.sessions.values().filter(|s| s.logged_in) {
            let _ = s.handle.to_client.send(ServerMessage::SkillCastFx {
                skill_id,
                caster_pos: de,
                target_pos: alvo,
                target_eid: Some(alvo_eid),
                caster_eid: Some(dono),
                chain_points: None,
            });
        }
    }

    fn cancelar_habilidade(&mut self, sid: SessionId, id: u32) {
        let Some(s) = self.sessions.get_mut(&sid) else {
            return;
        };
        if s.casting_skill_id != id {
            return;
        }
        s.mp_current = (s.mp_current + s.casting_mp_paid).min(s.stats.mp_max as f32);
        s.casting_mp_paid = 0.0;
        s.casting_until = 0.0;
        s.casting_skill_id = 0;
        s.gesto_skill_em = 0.0;
        s.skill_cds.remove(&id);
        let dono = s.entity_id;
        for s in self.sessions.values().filter(|s| s.logged_in) {
            let _ = s.handle.to_client.send(ServerMessage::SkillCastCancel {
                caster_eid: dono,
                skill_id: id,
            });
        }
        self.estado_skills(sid);
    }

    pub(super) fn avancar_habilidades(&mut self, dt: f32) {
        let fila = std::mem::take(&mut self.pending_habilidades);
        for mut h in fila {
            let Some(s) = self.sessions.get(&h.sid) else {
                continue;
            };
            if s.entity_id != h.dono || s.casting_skill_id != h.skill.id {
                continue;
            }
            let Some(e) = s.entity else { continue };
            let vivo = self
                .ecs
                .get::<&Health>(e)
                .map_or(false, |hp| hp.current > 0);
            if !s.logged_in
                || s.downed
                || !vivo
                || self.sim_time_s < s.stagger_until
                || Conjunto::da_arma(s.equipment.weapon.unwrap_or(0)) != h.skill.conjunto
            {
                self.cancelar_habilidade(h.sid, h.skill.id);
                continue;
            }
            let Some(mut pos) = self.ecs.get::<&Position>(e).ok().map(|p| p.0) else {
                continue;
            };
            let origem_alcance = if h.skill.id == 1 { h.de } else { pos };
            h.alvo = match self.posicao_alvo_skill(h.dono, h.alvo_eid, origem_alcance, &h.skill) {
                Ok(pos) => pos,
                Err(motivo) => {
                    self.cancelar_habilidade(h.sid, h.skill.id);
                    self.rejeita_skill(h.sid, h.skill.id, motivo);
                    continue;
                }
            };
            if h.skill.id == 1 {
                // Investida passa pela mesma colisao do caminhar, sem atravessar paredes.
                let t = ((self.sim_time_s - h.inicio) / (h.impacto - h.inicio)).clamp(0.0, 1.0);
                let destino = h.alvo
                    - (h.alvo - h.de).normalize_or_zero() * 0.8_f32.min(h.de.distance(h.alvo));
                let desejada = h.de.lerp(destino, t);
                let vel = (desejada - pos) / dt.max(0.001);
                let proxima = match &self.ilha {
                    Some(i) => i.mover_com_degrau(
                        pos,
                        vel,
                        dt,
                        ENTITY_RADIUS,
                        shared::terreno::DEGRAU_BLOCOS,
                    ),
                    None => self.map.move_and_slide(pos, vel, dt, ENTITY_RADIUS),
                };
                if !self.in_safe_zone(proxima) {
                    pos = proxima;
                    if let Ok(mut p) = self.ecs.get::<&mut Position>(e) {
                        p.0 = pos;
                    }
                }
            }
            if self.sim_time_s + 0.00001 < h.impacto {
                self.pending_habilidades.push(h);
                continue;
            }
            if h.skill.dano > 0 && self.in_safe_zone(pos) {
                self.cancelar_habilidade(h.sid, h.skill.id);
                continue;
            }
            if h.skill.id == 1 && pos.distance(h.alvo) > 1.8 {
                self.cancelar_habilidade(h.sid, h.skill.id);
                self.rejeita_skill(h.sid, h.skill.id, "Não foi possível alcançar o alvo.");
                continue;
            }
            let origem = if h.skill.id == 1 { h.de } else { pos };
            let alvo = h.alvo;
            self.efeito_habilidade(h.dono, h.alvo_eid, origem, alvo, &h.skill, h.tier);
            if let Some(s) = self.sessions.get_mut(&h.sid) {
                s.casting_mp_paid = 0.0;
                s.casting_skill_id = 0;
            }
            for s in self.sessions.values().filter(|s| s.logged_in) {
                let _ = s.handle.to_client.send(ServerMessage::SkillImpactFx {
                    skill_id: h.skill.id,
                    caster_eid: h.dono,
                    caster_pos: origem,
                    target_pos: alvo,
                });
            }
        }
    }

    fn efeito_habilidade(
        &mut self,
        dono: EntityId,
        alvo_eid: EntityId,
        de: Vec2,
        alvo: Vec2,
        skill: &Skill,
        tier: u8,
    ) {
        let dir = (alvo - de).try_normalize().unwrap_or(Vec2::Y);
        if skill.id == 3 {
            if let Some(s) = self.sessions.values_mut().find(|s| s.entity_id == dono) {
                s.muralha_ate = self.sim_time_s
                    + if tier >= 10 {
                        8.0
                    } else if tier >= 5 {
                        6.0
                    } else {
                        skill.duracao_efeito()
                    };
            }
            return;
        }
        if skill.id == 1 && tier >= 8 {
            if let Some(s) = self.sessions.values_mut().find(|s| s.entity_id == dono) {
                s.muralha_ate = s.muralha_ate.max(self.sim_time_s + 2.0);
            }
        }
        if skill.forma == Forma::EmSi {
            self.pending_heals.push(PendingHeal {
                target_net: dono,
                amount: cura_evoluida(skill.cura, tier, skill.id),
            });
            return;
        }
        // O dano sai do ataque de quem conjurou, pela mesma `session.stats` que
        // alimenta o golpe basico — ver `Skill::dano_efetivo` pro porque.
        let (atk, cd, chance_crit) = self.sessions.values().find(|s| s.entity_id == dono).map_or(
            (shared::base_player_stats().attack_damage, 0.65, 0.0),
            |s| {
                (
                    s.stats.attack_damage,
                    super::cooldown_do_ataque(s.equipment.weapon.unwrap_or(0), &s.stats, false),
                    s.stats.crit_chance,
                )
            },
        );
        // Critico com a MESMA chance e o mesmo multiplicador do basico: antes
        // a skill nunca critava e o basico sim — mais um motivo pra ela
        // render menos que o golpe que desliga. Um sorteio por conjuracao,
        // como o basico faz por golpe (todos os alvos levam o mesmo).
        let crit =
            (skill.id == 7 && tier >= 10) || (chance_crit > 0.0 && fastrand::f32() < chance_crit);
        let dano_base = if crit {
            (skill.dano_efetivo(atk, cd) as f32 * shared::CRIT_DAMAGE_MULT).round() as i32
        } else {
            skill.dano_efetivo(atk, cd)
        };
        let dano = dano_evoluido(dano_base, tier, skill.id);
        if skill.forma == Forma::Projetil {
            // Disparo target: outro mob cruzando a linha nao troca o destinatario.
            self.pending_skill_hits.push(PendingSkillHit {
                target_net: alvo_eid,
                damage: dano,
                attacker_net: dono,
                hurt_dir: -dir,
                is_crit: crit,
                from_player: true,
                knockback: if skill.id == 7 && tier >= 8 { 0.7 } else { 0.0 },
            });
            return;
        }
        for (net, pos) in self.alvos_da_habilidade(dono, de, dir, alvo, skill) {
            if skill.dano > 0 {
                self.pending_skill_hits.push(PendingSkillHit {
                    target_net: net,
                    damage: dano,
                    attacker_net: dono,
                    hurt_dir: (de - pos).normalize_or_zero(),
                    is_crit: crit,
                    from_player: true,
                    knockback: if skill.id == 1 || (skill.id == 8 && tier >= 8) {
                        0.7
                    } else {
                        0.0
                    },
                });
            }
            if skill.cura > 0 {
                self.pending_heals.push(PendingHeal {
                    target_net: net,
                    amount: cura_evoluida(skill.cura, tier, skill.id),
                });
            }
        }
    }

    fn alvos_da_habilidade(
        &self,
        dono: EntityId,
        de: Vec2,
        dir: Vec2,
        alvo: Vec2,
        skill: &Skill,
    ) -> Vec<(EntityId, Vec2)> {
        let mut alvos = Vec::new();
        for (e, (net, pos, kind, hp)) in self
            .ecs
            .query::<(&NetId, &Position, &EntityKind, &Health)>()
            .iter()
        {
            if hp.current <= 0 {
                continue;
            }
            let player = matches!(kind, EntityKind::Player);
            if skill.cura > 0 {
                if !player {
                    continue;
                }
                let aliado = net.0 == dono
                    || self
                        .sessions
                        .values()
                        .find(|s| s.entity_id == dono)
                        .is_some_and(|s| {
                            self.sessions.values().any(|t| {
                                t.entity_id == net.0
                                    && !t.downed
                                    && ((s.party_id.is_some() && s.party_id == t.party_id)
                                        || s.faction == t.faction)
                                    && !self.can_damage_player(dono, net.0)
                            })
                        });
                if !aliado {
                    continue;
                }
            } else {
                if net.0 == dono || self.in_safe_zone(pos.0) {
                    continue;
                }
                if player && !self.can_damage_player(dono, net.0) {
                    continue;
                }
                if !player && !matches!(kind, EntityKind::Enemy(_)) {
                    continue;
                }
                if self.ecs.get::<&EnemyTag>(e).is_ok_and(|t| {
                    t.dead || t.returning_home || t.spawn_grace_until > self.sim_time_s
                }) {
                    continue;
                }
            }
            // Circulo no alvo cai do alto (arco ate' la'); cone e linha saem
            // rentes ao chao e o relevo barra em linha reta.
            let de_ve = if skill.forma == Forma::Circulo {
                visada_de_tiro(self.ilha.as_ref(), &self.map, de, pos.0)
            } else {
                visada(self.ilha.as_ref(), &self.map, de, pos.0)
            };
            if dentro_da_forma(skill, de, dir, alvo, pos.0)
                && de_ve
                && (skill.forma != Forma::Circulo
                    || visada(self.ilha.as_ref(), &self.map, alvo, pos.0))
            {
                alvos.push((net.0, pos.0));
            }
        }
        alvos
    }
}

fn dentro_da_forma(skill: &Skill, de: Vec2, dir: Vec2, alvo: Vec2, pos: Vec2) -> bool {
    let d = pos - de;
    match skill.forma {
        Forma::Cone => {
            d.length() <= skill.alcance
                && (d.length_squared() < 0.0001 || d.normalize_or_zero().dot(dir) >= 0.5)
        }
        Forma::Circulo => pos.distance(alvo) <= skill.raio,
        Forma::Linha => {
            let t = d.dot(dir).clamp(0.0, skill.alcance);
            d.dot(dir) >= 0.0
                && d.dot(dir) <= skill.alcance + skill.raio
                && (de + dir * t).distance(pos) <= skill.raio.max(0.8)
        }
        _ => false,
    }
}

#[cfg(test)]
mod testes_de_evolucao {
    use super::*;

    #[test]
    fn tier_um_preserva_o_golpe_e_tier_x_melhora() {
        for skill in shared::skills::playtest() {
            if skill.dano > 0 {
                assert_eq!(dano_evoluido(100, 1, skill.id), 100);
                assert!(dano_evoluido(100, 10, skill.id) >= 121);
            }
            if skill.cura > 0 {
                assert_eq!(cura_evoluida(100, 1, skill.id), 100);
                assert!(cura_evoluida(100, 10, skill.id) >= 121);
            }
        }
        let tiro = shared::skills::playtest()
            .into_iter()
            .find(|s| s.id == 7)
            .unwrap();
        assert_eq!(skill_ajustada(tiro.clone(), 1).alcance, tiro.alcance);
        let barril = shared::skills::playtest()
            .into_iter()
            .find(|s| s.id == 9)
            .unwrap();
        assert!(skill_ajustada(barril.clone(), 8).raio > barril.raio);
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    fn mundo(skill: &Skill) -> (GameWorld, SessionId, EntityId) {
        let mut w = GameWorld::new(HashMap::new());
        w.map = shared::mapfile::MapFile::new("skills-test", 40, 40).to_world_map();
        let sid = SessionId("127.0.0.1:45678".parse().unwrap());
        let (tx, _) = mpsc::unbounded_channel();
        w.on_connect(SessionHandle {
            id: sid,
            to_client: tx,
        });
        let s = w.sessions.get_mut(&sid).unwrap();
        s.logged_in = true;
        s.xp = shared::xp_for_level_with_mult(20, crate::economy::xp_multiplier());
        s.equipment.weapon = Some(skill.conjunto.arma());
        s.mp_current = 100.0;
        s.stats.mp_max = 100;
        // Sem ataque nenhum a conta de `dano_efetivo` cai no piso e o mundo de
        // teste deixa de parecer com o jogo: um conjurador tem ataque.
        s.stats.attack_damage = 60;
        let dono = s.entity_id;
        let e = w.ecs.spawn((
            NetId(dono),
            Position(Vec2::splat(10.0)),
            Velocity(Vec2::ZERO),
            EntityKind::Player,
            Health {
                current: 25,
                max: 100,
            },
        ));
        s.entity = Some(e);
        s.target = Some(EntityId(999));
        w.ecs.spawn((
            NetId(EntityId(999)),
            Position(Vec2::new(12.0, 10.0)),
            EntityKind::Enemy(0),
            Health {
                current: 100,
                max: 100,
            },
        ));
        (w, sid, dono)
    }

    #[test]
    fn todas_as_doze_cobram_uma_vez_e_so_produzem_efeito_no_impacto() {
        for skill in shared::skills::playtest() {
            let (mut w, sid, dono) = mundo(&skill);
            w.conjurar_skill(sid, skill.clone());
            assert_eq!(w.pending_habilidades.len(), 1, "{}", skill.nome);
            assert_eq!(w.sessions[&sid].mp_current, 100.0 - skill.custo_mp as f32);
            w.conjurar_skill(sid, skill.clone());
            assert_eq!(
                w.pending_habilidades.len(),
                1,
                "duplo cast de {}",
                skill.nome
            );
            w.sim_time_s = skill.impacto_em() - 0.02;
            w.avancar_habilidades(shared::TICK_DT);
            assert!(w.pending_skill_hits.is_empty() && w.pending_heals.is_empty());
            assert_eq!(w.ecs.query::<&ProjTag>().iter().count(), 0);
            assert_eq!(w.sessions[&sid].muralha_ate, 0.0);
            w.sim_time_s = skill.impacto_em();
            w.avancar_habilidades(shared::TICK_DT);
            assert!(w.pending_habilidades.is_empty());
            match skill.id {
                3 => assert!(w.sessions[&sid].muralha_ate > w.sim_time_s),
                10 | 11 => assert!(w
                    .pending_heals
                    .iter()
                    .any(|h| h.target_net == dono && h.amount == skill.cura)),
                _ => {
                    let s = &w.sessions[&sid];
                    let esperado = skill.dano_efetivo(
                        s.stats.attack_damage,
                        crate::world::cooldown_do_ataque(
                            s.equipment.weapon.unwrap_or(0),
                            &s.stats,
                            false,
                        ),
                    );
                    // Aqui so' se cobra que o servidor use a formula. Que a
                    // skill VENCA o basico que ela desliga e' outra garantia, e
                    // mora no `shared` — comparar com `skill.dano` nao serve:
                    // skill de area rende menos por alvo de proposito.
                    assert!(
                        w.pending_skill_hits
                            .iter()
                            .any(|h| h.target_net == EntityId(999) && h.damage == esperado),
                        "{} sem alvo",
                        skill.nome
                    );
                }
            }
            let quantidade = w.pending_skill_hits.len() + w.pending_heals.len();
            w.sim_time_s += 1.0;
            w.avancar_habilidades(shared::TICK_DT);
            assert_eq!(
                quantidade,
                w.pending_skill_hits.len() + w.pending_heals.len()
            );
        }
    }

    #[test]
    fn nivel_arma_mana_recarga_e_estado_sao_validados_no_servidor() {
        for skill in shared::skills::playtest() {
            let n = skill.nivel_necessario();
            assert!(valida(&skill, skill.conjunto, n - 1, 100.0, 0.0, false).is_err());
            assert!(valida(&skill, skill.conjunto, n, 100.0, 0.0, false).is_ok());
            let outra = Conjunto::TODOS[(skill.conjunto as usize + 1) % 4];
            assert!(valida(&skill, outra, 100, 100.0, 0.0, false).is_err());
            assert!(valida(&skill, skill.conjunto, 100, 0.0, 0.0, false).is_err());
            assert!(valida(&skill, skill.conjunto, 100, 100.0, 0.1, false).is_err());
            assert!(valida(&skill, skill.conjunto, 100, 100.0, 0.0, true).is_err());
        }
    }

    #[test]
    fn dash_interrompe_skill_antes_do_impacto_e_respeita_recarga() {
        crate::economy::init_vazia_para_testes();
        let skill = shared::skills::playtest().remove(8);
        let (mut w, sid, _) = mundo(&skill);
        w.conjurar_skill(sid, skill.clone());
        {
            let s = w.sessions.get_mut(&sid).unwrap();
            s.stats.stamina_max = 100;
            s.stamina_current = 100.0;
            s.pending_input = Some(shared::protocol::InputFrame {
                seq: 1,
                tick: 1,
                move_dir: Vec2::X,
                aim: Vec2::new(20.0, 10.0),
                buttons: buttons::DASH,
            });
        }
        w.step(shared::TICK_DT);
        let s = &w.sessions[&sid];
        assert!(s.dash_cooldown > 0.0);
        assert_eq!(s.casting_skill_id, 0);
        assert_eq!(s.mp_current, 100.0);
        assert!(s.stamina_current < 100.0);
        assert!(w.pending_habilidades.is_empty());
        assert!(!s.skill_cds.contains_key(&skill.id));
        let antes = s.stamina_current;
        w.sessions.get_mut(&sid).unwrap().pending_input = Some(shared::protocol::InputFrame {
            seq: 2,
            tick: 2,
            move_dir: Vec2::X,
            aim: Vec2::new(20.0, 10.0),
            buttons: buttons::DASH,
        });
        w.step(shared::TICK_DT);
        assert!(
            w.sessions[&sid].stamina_current >= antes,
            "recarga não cobra outro Dash"
        );
    }

    #[test]
    fn trocar_arma_antes_do_impacto_cancela_e_devolve_mana() {
        let skill = shared::skills::playtest().remove(8);
        let (mut w, sid, _) = mundo(&skill);
        w.conjurar_skill(sid, skill.clone());
        w.sessions.get_mut(&sid).unwrap().equipment.weapon = Some(Conjunto::Katana.arma());
        w.sim_time_s = skill.impacto_em();
        w.avancar_habilidades(shared::TICK_DT);
        assert!(w.pending_habilidades.is_empty() && w.pending_skill_hits.is_empty());
        assert_eq!(w.sessions[&sid].mp_current, 100.0);
        assert!(!w.sessions[&sid].skill_cds.contains_key(&skill.id));
    }

    #[test]
    fn alvo_fora_da_area_no_impacto_nao_recebe_dano() {
        let skill = shared::skills::playtest().remove(8);
        let (mut w, sid, _) = mundo(&skill);
        w.conjurar_skill(sid, skill.clone());
        for (_, (net, p)) in w.ecs.query_mut::<(&NetId, &mut Position)>() {
            if net.0 == EntityId(999) {
                p.0 = Vec2::splat(30.0);
            }
        }
        w.sim_time_s = skill.impacto_em();
        w.avancar_habilidades(shared::TICK_DT);
        assert!(w.pending_skill_hits.is_empty());
    }

    #[test]
    fn danca_fica_centrada_no_alvo() {
        let skill = shared::skills::playtest().remove(4);
        let (mut w, sid, _) = mundo(&skill);
        w.ecs.spawn((
            NetId(EntityId(998)),
            Position(Vec2::new(14.0, 10.0)),
            EntityKind::Enemy(0),
            Health {
                current: 100,
                max: 100,
            },
        ));
        w.conjurar_skill(sid, skill.clone());
        w.sim_time_s = skill.impacto_em();
        w.avancar_habilidades(shared::TICK_DT);
        assert_eq!(w.pending_skill_hits.len(), 2);
        assert!(w
            .pending_skill_hits
            .iter()
            .any(|h| h.target_net == EntityId(998)));
    }

    #[test]
    fn ofensivas_exigem_alvo_valido_sem_cobrar_mana_ou_recarga() {
        for skill in shared::skills::playtest()
            .into_iter()
            .filter(|s| s.dano > 0)
        {
            for caso in 0..5 {
                let (mut w, sid, dono) = mundo(&skill);
                match caso {
                    0 => w.sessions.get_mut(&sid).unwrap().target = None,
                    1 => w.sessions.get_mut(&sid).unwrap().target = Some(dono),
                    2 => w.sessions.get_mut(&sid).unwrap().target = Some(EntityId(123456)),
                    3 => {
                        for (_, (n, hp)) in w.ecs.query_mut::<(&NetId, &mut Health)>() {
                            if n.0 == EntityId(999) {
                                hp.current = 0;
                            }
                        }
                    }
                    _ => {
                        for (_, (n, p)) in w.ecs.query_mut::<(&NetId, &mut Position)>() {
                            if n.0 == EntityId(999) {
                                p.0 = Vec2::splat(30.0);
                            }
                        }
                    }
                }
                w.conjurar_skill(sid, skill.clone());
                assert!(
                    w.pending_habilidades.is_empty(),
                    "{} caso {caso}",
                    skill.nome
                );
                assert_eq!(w.sessions[&sid].mp_current, 100.0);
                assert!(w.sessions[&sid].skill_cds.is_empty());
            }
        }
    }

    #[test]
    fn alvo_em_movimento_e_troca_de_selecao_preservam_destinatario() {
        for id in [6, 7, 9, 12] {
            let skill = shared::skills::playtest().remove(id - 1);
            let (mut w, sid, _) = mundo(&skill);
            w.conjurar_skill(sid, skill.clone());
            for (_, (n, p)) in w.ecs.query_mut::<(&NetId, &mut Position)>() {
                if n.0 == EntityId(999) {
                    p.0 = Vec2::new(10.0, 16.0);
                }
            }
            // Outro mob no caminho nao intercepta o disparo; mudar a selecao
            // durante a preparacao tambem nao redireciona a skill.
            w.ecs.spawn((
                NetId(EntityId(998)),
                Position(Vec2::new(10.0, 11.0)),
                EntityKind::Enemy(0),
                Health {
                    current: 100,
                    max: 100,
                },
            ));
            w.sessions.get_mut(&sid).unwrap().target = Some(EntityId(998));
            w.sim_time_s = skill.impacto_em();
            w.avancar_habilidades(shared::TICK_DT);
            assert_eq!(w.pending_skill_hits.len(), 1, "{}", skill.nome);
            assert_eq!(w.pending_skill_hits[0].target_net, EntityId(999));
            assert_eq!(w.ecs.query::<&ProjTag>().iter().count(), 0);
        }
    }

    #[test]
    fn morte_ou_despawn_antes_do_impacto_cancela_com_reembolso() {
        for despawn in [false, true] {
            let skill = shared::skills::playtest().remove(6);
            let (mut w, sid, _) = mundo(&skill);
            w.conjurar_skill(sid, skill.clone());
            let e = w
                .ecs
                .query::<&NetId>()
                .iter()
                .find(|(_, n)| n.0 == EntityId(999))
                .unwrap()
                .0;
            if despawn {
                w.ecs.despawn(e).unwrap();
            } else {
                w.ecs.get::<&mut Health>(e).unwrap().current = 0;
            }
            w.sim_time_s = skill.impacto_em();
            w.avancar_habilidades(shared::TICK_DT);
            assert!(w.pending_skill_hits.is_empty() && w.pending_habilidades.is_empty());
            assert_eq!(w.sessions[&sid].mp_current, 100.0);
            assert!(w.sessions[&sid].skill_cds.is_empty());
        }
    }

    #[test]
    fn suporte_usa_o_personagem_sem_selecao() {
        for skill in shared::skills::playtest()
            .into_iter()
            .filter(|s| s.dano <= 0)
        {
            let (mut w, sid, dono) = mundo(&skill);
            w.sessions.get_mut(&sid).unwrap().target = None;
            w.conjurar_skill(sid, skill.clone());
            assert_eq!(w.pending_habilidades[0].alvo_eid, dono);
            w.sim_time_s = skill.impacto_em();
            w.avancar_habilidades(shared::TICK_DT);
            if skill.cura > 0 {
                assert!(w.pending_heals.iter().any(|h| h.target_net == dono));
            } else {
                assert!(w.sessions[&sid].muralha_ate > w.sim_time_s);
            }
        }
    }

    #[test]
    fn obstaculo_zona_segura_e_player_protegido_bloqueiam_inicio_e_impacto() {
        for skill in shared::skills::playtest()
            .into_iter()
            .filter(|s| s.dano > 0)
        {
            for durante_cast in [false, true] {
                for caso in 0..3 {
                    let (mut w, sid, _) = mundo(&skill);
                    if durante_cast {
                        w.conjurar_skill(sid, skill.clone());
                    }
                    match caso {
                        0 => {
                            let mut mapa = shared::mapfile::MapFile::new("parede", 40, 40);
                            mapa.set(11, 10, shared::constants::tile_id::WALL);
                            w.map = mapa.to_world_map();
                        }
                        1 => w.safe_zones.push((Vec2::new(11.5, 9.5), Vec2::ONE)),
                        _ => {
                            for (_, (n, kind)) in w.ecs.query_mut::<(&NetId, &mut EntityKind)>() {
                                if n.0 == EntityId(999) {
                                    *kind = EntityKind::Player;
                                }
                            }
                        }
                    }
                    if durante_cast {
                        w.sim_time_s = skill.impacto_em();
                        w.avancar_habilidades(shared::TICK_DT);
                    } else {
                        w.conjurar_skill(sid, skill.clone());
                    }
                    assert!(
                        w.pending_habilidades.is_empty() && w.pending_skill_hits.is_empty(),
                        "{} caso {caso}",
                        skill.nome
                    );
                    assert_eq!(w.sessions[&sid].mp_current, 100.0);
                    assert!(w.sessions[&sid].skill_cds.is_empty());
                }
            }
        }
    }
}
