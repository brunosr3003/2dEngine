//! LEAVING WITH PK, and the KILL LETTER (owner, 16/11/2026).
//!
//! "A character with PK has to stay logged in for at least 2 minutes,
//! standing still, even after the person left — otherwise it is too easy to
//! kill everyone, log out and that's it." And: "when someone kills you an
//! e-mail has to come: such person killed you at such place at such time".
//!
//! Who lingers: a red name (PK points) or anyone in PK combat — a PvP hit
//! given or taken in the last `shared::pk::COMBATE_PK_S` (30 s). Everyone
//! else leaves at once, as before (a dropped phone costs nothing).
//!
//! How: when the connection goes, the SESSION stays (`permanece_ate`), with
//! its body in the world, frozen (no input, no target, no route, no pilot)
//! and attackable, for `shared::pk::PERMANENCIA_S`. The periodic save keeps
//! writing it. It ends — saved and removed as an ordinary disconnect — when
//! the time is up, when it falls (the death rules already ran: XP, PK
//! points, the killer's letter; it then wakes at the city, as a respawn
//! would), or when something sends it to another zone (a sunk ship washes
//! ashore). Logging the same character in again takes the body back: on
//! this channel directly (`religar_permanencia`), from any other channel by
//! being sent here first (`pk_permanencia`, `checar_permanencia_remota`).
//!
//! The escape hatches closed: the "logout" request lingers like a dropped
//! connection; a new login of the same account does not kick the lingering
//! body (the usual "new login wins" rule skips it); a new connection reusing
//! the same address ends it properly instead of overwriting it; and travel to
//! another zone is refused while in PK combat (`recusa_viagem_em_combate_pk`)
//! — a red name with no fight in the last 30 s may travel, it stays online
//! and attackable on the other island.

use super::*;

impl GameWorld {
    /// In PK combat right now (a PvP hit in the last `COMBATE_PK_S`).
    pub(crate) fn em_combate_pk(&self, s: &Session) -> bool {
        s.pk_combate_ate > self.sim_time_s
    }

    /// Both sides of a PvP hit are in PK combat from now on.
    pub(crate) fn marcar_combate_pk(&mut self, a: EntityId, b: EntityId) {
        let ate = self.sim_time_s + shared::pk::COMBATE_PK_S;
        for s in self.sessions.values_mut().filter(|s| s.entity_id == a || s.entity_id == b) {
            s.pk_combate_ate = s.pk_combate_ate.max(ate);
        }
    }

    pub fn esta_permanecendo(&self, sid: &SessionId) -> bool {
        self.sessions.get(sid).is_some_and(|s| s.permanece_ate.is_some())
    }

    /// The connection of `sid` dropped, or the player asked to leave. `true`
    /// = the body stays in the world: the caller must NOT save and remove the
    /// session now (`tick_permanencia` hands it back when it is done).
    pub fn comecar_permanencia(&mut self, sid: SessionId) -> bool {
        let Some(s) = self.sessions.get(&sid) else { return false };
        if s.permanece_ate.is_some() {
            return true;
        }
        let fica = s.logged_in
            && s.entity.is_some()
            && !s.downed
            && s.instancia == 0
            && !self.tutorial_mode
            && !self.dungeon_mode
            && !self.zona_de_saida.contains_key(&s.name)
            && shared::pk::permanece_ao_sair(s.pk_points, self.em_combate_pk(s));
        if !fica {
            return false;
        }
        let ate = self.sim_time_s + shared::pk::PERMANENCIA_S;
        let nome = s.name.clone();
        // Nothing of the player's own goes on: the body stands still.
        self.troca_encerra(sid, false, "Trade cancelled: the other player left.");
        self.troca_convites.retain(|k, (de, _)| *k != sid && *de != sid);
        self.social_pendentes.remove(&sid);
        if let Some(s) = self.sessions.get_mut(&sid) {
            s.permanece_ate = Some(ate);
            s.pending_input = None;
            s.target = None;
            s.rota.limpa();
            s.coleta_no = None;
            s.piloto = Default::default();
        }
        tracing::info!("{nome} saiu com PK: o corpo fica {:.0} s no mundo", shared::pk::PERMANENCIA_S);
        if let Some(ctx) = &self.auth_ctx {
            let (pool, zona, canal) = (ctx.pool.clone(), self.zona.clone(), canal_daqui());
            let host = std::env::var("MMO_HOST_PUBLICO")
                .unwrap_or_else(|_| std::env::var("BIND_ADDR").unwrap_or_else(|_| "127.0.0.1:9000".into()));
            tokio::spawn(async move {
                let s = shared::pk::PERMANENCIA_S as i64 + 5;
                if let Err(e) = crate::social::permanencia_comeca(&pool, &nome, &zona, &canal, &host, s).await {
                    tracing::warn!("pk_permanencia: {e:#}");
                }
            });
        }
        true
    }

    /// Once a second: the lingering bodies that are done — time up, fallen,
    /// or sent to another zone. The caller saves and removes each one, as a
    /// disconnect. Also writes the kill letters waiting.
    pub fn tick_permanencia(&mut self) -> Vec<SessionId> {
        self.enviar_cartas_de_abate();
        let agora = self.sim_time_s;
        let mut prontos = Vec::new();
        let mut caidos = Vec::new();
        for (sid, s) in &self.sessions {
            let Some(ate) = s.permanece_ate else { continue };
            if s.downed {
                caidos.push(*sid);
            } else if s.entity.is_none() || ate <= agora || self.zona_de_saida.contains_key(&s.name) {
                prontos.push(*sid);
            }
        }
        // Fell while lingering: the death already counted (XP, the killer's
        // PK points and letter). Nobody is there to stand up, so it wakes at
        // the city, as a respawn, and that is what gets saved.
        for sid in caidos {
            self.handle_respawn_at_city(sid);
            prontos.push(sid);
        }
        for sid in &prontos {
            self.permanencia_acabou(*sid);
        }
        prontos
    }

    /// Forgets the realm-wide record of `sid`'s lingering body.
    fn permanencia_acabou(&mut self, sid: SessionId) {
        let Some(s) = self.sessions.get_mut(&sid) else { return };
        s.permanece_ate = None;
        let nome = s.name.clone();
        if let Some(ctx) = &self.auth_ctx {
            let (pool, zona, canal) = (ctx.pool.clone(), self.zona.clone(), canal_daqui());
            tokio::spawn(async move {
                if let Err(e) = crate::social::permanencia_fim(&pool, &nome, &zona, &canal).await {
                    tracing::warn!("pk_permanencia: {e:#}");
                }
            });
        }
    }

    /// The same character logs in again on THIS channel while its body
    /// lingers: the body's state becomes the in-memory copy the login spawns
    /// from (same spot, same HP), and the old session goes. `true` = it was
    /// lingering here.
    pub(crate) fn religar_permanencia(&mut self, nome: &str, conta: i64) -> bool {
        let Some(velho) = self
            .sessions
            .iter()
            .find(|(_, s)| s.permanece_ate.is_some() && s.name == nome && s.account_id == Some(conta))
            .map(|(k, _)| *k)
        else {
            return false;
        };
        if self.sessions.get(&velho).is_some_and(|s| s.downed) {
            self.handle_respawn_at_city(velho);
        }
        self.permanencia_acabou(velho);
        // `take_character_for_disconnect` leaves the row as the fresh copy.
        let _ = self.take_character_for_disconnect(&velho);
        // Not "logged out": the same name comes back online here right now.
        if let Some(s) = self.sessions.get_mut(&velho) {
            s.logged_in = false;
        }
        self.on_disconnect(velho);
        tracing::info!("{nome} voltou: retoma o corpo que tinha ficado (PK)");
        true
    }

    /// Asks the realm whether `nome`'s body lingers on ANOTHER channel; the
    /// answer comes back as `IncomingMessage::PermanenciaRemota`. `true` = it
    /// went to ask (the caller stops); `false` = go on with the login.
    pub(crate) fn checar_permanencia_remota(&mut self, sid: SessionId, nome: &str) -> bool {
        if self.tutorial_mode || self.dungeon_mode {
            return false;
        }
        let Some(ctx) = &self.auth_ctx else { return false };
        let Some(s) = self.sessions.get_mut(&sid) else { return false };
        if s.permanencia_checada.as_deref() == Some(nome) {
            s.permanencia_checada = None;
            return false;
        }
        let (pool, tx, zona, canal) = (ctx.pool.clone(), ctx.tx.clone(), self.zona.clone(), canal_daqui());
        let nome = nome.to_string();
        tokio::spawn(async move {
            let destino = match crate::social::permanencia_remota(&pool, &nome, &zona, &canal).await {
                Ok(d) => d,
                Err(e) => {
                    tracing::warn!("pk_permanencia: {e:#}");
                    None
                }
            };
            let _ = tx.send(IncomingMessage::PermanenciaRemota { sid, nome, destino });
        });
        true
    }

    /// The realm answered: lingering elsewhere = reconnect THERE (that
    /// channel hands the body back); otherwise the login goes on.
    pub fn on_permanencia_remota(&mut self, sid: SessionId, nome: String, destino: Option<(String, String)>) {
        let Some(s) = self.sessions.get_mut(&sid).filter(|s| !s.logged_in) else { return };
        match destino {
            Some((zona, host)) => {
                tracing::info!("login '{nome}': o corpo ainda esta' noutro canal ({zona} {host}) — manda pra la'");
                let _ = s.handle.to_client.send(ServerMessage::TrocarZona { zona, host });
            }
            None => {
                s.permanencia_checada = Some(nome.clone());
                self.handle_select_character(sid, nome);
            }
        }
    }

    /// Voluntary travel to another zone during PK combat is refused (it
    /// would be the way out of a fight). Forced moves go through: fallen, no
    /// body, a sunk ship washing ashore, the Magic Island's time running
    /// out, a lingering body. `true` = refused (the player was told).
    pub(crate) fn recusa_viagem_em_combate_pk(&self, sid: SessionId) -> bool {
        let Some(s) = self.sessions.get(&sid) else { return false };
        if !self.em_combate_pk(s) || self.tutorial_mode || self.dungeon_mode {
            return false;
        }
        let forcada = s.downed
            || s.entity.is_none()
            || s.permanece_ate.is_some()
            || (self.na_magica() && s.magica_ate == 0)
            || self.navio_equipado(sid).is_some_and(|(_, d)| d.avariado);
        if forcada {
            return false;
        }
        let resta = (s.pk_combate_ate - self.sim_time_s).ceil().max(1.0) as u32;
        let _ = s.handle.to_client.send(ServerMessage::Chat {
            from: "SYS".into(),
            text: format!("You are in PK combat: you cannot travel for {resta} more seconds."),
        });
        true
    }

    /// A player killed another (land or sea): the victim gets a system
    /// letter naming the killer, the place and the time. Queued on the
    /// victim's session; `enviar_cartas_de_abate` writes it.
    pub(crate) fn carta_de_abate(&mut self, killer: EntityId, vitima: EntityId, area_aberta: bool) {
        if killer == vitima {
            return;
        }
        let mult = crate::economy::xp_multiplier();
        let Some(k) = self.sessions.values().find(|s| s.entity_id == killer && s.logged_in) else { return };
        let (quem, renasc, nivel, pk) = (
            k.name.clone(),
            k.renascimentos,
            shared::level_of_xp_with_mult(k.xp, mult),
            k.pk_points > 0,
        );
        let Some(v) = self.sessions.values().find(|s| s.entity_id == vitima && s.logged_in) else { return };
        let pos = v
            .entity
            .and_then(|e| self.ecs.get::<&Position>(e).ok().map(|p| p.0))
            .unwrap_or(Vec2::ZERO);
        let onde = shared::pk::lugar(&self.zona, pos);
        let quando = (now_ms() / 1000) as i64;
        let (assunto, texto) = shared::pk::carta_de_abate(&quem, renasc, nivel, &onde, quando, pk, area_aberta);
        let Some(v) = self.sessions.values_mut().find(|s| s.entity_id == vitima) else { return };
        // Dead is out of the fight: the wash-up / respawn is not "travel".
        v.pk_combate_ate = 0.0;
        v.cartas_de_abate.push((assunto, texto, quem.clone()));
        let _ = v.handle.to_client.send(ServerMessage::Chat {
            from: "SYS".into(),
            text: format!("You were killed by {quem}. A letter with the details is in your mail."),
        });
    }

    /// Writes the queued kill letters (no SQL on the tick: a task each).
    pub(crate) fn enviar_cartas_de_abate(&mut self) {
        let pool = self.auth_ctx.as_ref().map(|c| c.pool.clone());
        for s in self.sessions.values_mut() {
            if s.cartas_de_abate.is_empty() {
                continue;
            }
            // Without a database (tests) the queue stays, to be looked at.
            let Some(pool) = pool.clone() else { continue };
            let para = s.name.clone();
            for (assunto, texto, matador) in std::mem::take(&mut s.cartas_de_abate) {
                let (pool, para) = (pool.clone(), para.clone());
                tokio::spawn(async move {
                    if let Err(e) = crate::social::carta_de_abate(&pool, &para, &matador, &assunto, &texto).await {
                        tracing::warn!("kill letter to {para}: {e:#}");
                    }
                });
            }
        }
    }
}

fn canal_daqui() -> String {
    std::env::var("MMO_CANAL").unwrap_or_else(|_| "1".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Two players at (10,10)/(11,10), "pk0" and "pk1" (`pk_tests::mundo`).
    fn mundo() -> (GameWorld, [SessionId; 2]) {
        crate::economy::init_vazia_para_testes();
        let mut w = GameWorld::new(HashMap::new());
        w.zona = "ilha_inicial".into();
        w.imortal = false;
        w.safe_zone = false;
        let ids = [SessionId(([127, 0, 0, 1], 19851).into()), SessionId(([127, 0, 0, 1], 19852).into())];
        for (i, sid) in ids.iter().enumerate() {
            let (tx, rx) = mpsc::unbounded_channel();
            std::mem::forget(rx);
            w.on_connect(SessionHandle { id: *sid, to_client: tx });
            let eid = EntityId(900 + i as u32);
            let e = w.ecs.spawn((
                NetId(eid),
                Position(Vec2::new(10.0 + i as f32, 10.0)),
                Velocity(Vec2::ZERO),
                EntityKind::Player,
                Health { current: 100, max: 100 },
                PlayerTag {
                    name: format!("pk{i}"),
                    player_id: PlayerId(i as u64),
                    attack_anim_pending: None,
                    combo_step_pending: None,
                },
            ));
            let s = w.sessions.get_mut(sid).unwrap();
            s.logged_in = true;
            s.entity = Some(e);
            s.entity_id = eid;
            s.name = format!("pk{i}");
            s.account_id = Some(i as i64 + 1);
            s.poise_current = 0.0;
            s.last_combat_at_s = 10000.0;
        }
        (w, ids)
    }

    fn golpe(w: &mut GameWorld, dano: i32) {
        w.pending_skill_hits.push(PendingSkillHit {
            drain_tier: 0,
            target_net: EntityId(901),
            damage: dano,
            attacker_net: EntityId(900),
            hurt_dir: Vec2::X,
            is_crit: false,
            from_player: true,
            knockback: 0.0,
        });
        w.step(shared::TICK_DT);
    }

    /// What the network loop does when the socket closes (`tick.rs`).
    fn cair(w: &mut GameWorld, sid: SessionId) {
        if !w.comecar_permanencia(sid) {
            let _ = w.take_character_for_disconnect(&sid);
            w.on_disconnect(sid);
        }
    }

    /// The once-a-second linger tick, with the save+remove of `tick.rs`.
    fn segundos(w: &mut GameWorld, n: u32) {
        for _ in 0..n {
            for _ in 0..30 {
                w.step(shared::TICK_DT);
            }
            for sid in w.tick_permanencia() {
                let _ = w.take_character_for_disconnect(&sid);
                w.on_disconnect(sid);
            }
        }
    }

    #[test]
    fn sem_pk_sai_na_hora() {
        let (mut w, ids) = mundo();
        cair(&mut w, ids[0]);
        assert!(!w.sessions.contains_key(&ids[0]), "no PK: gone at once");
    }

    #[test]
    fn com_pk_o_corpo_fica_120_s_parado_e_atacavel() {
        let (mut w, ids) = mundo();
        w.sessions.get_mut(&ids[1]).unwrap().pk_points = 2;
        cair(&mut w, ids[1]);
        assert!(w.esta_permanecendo(&ids[1]));
        // A second close (RequestDisconnect then the socket) changes nothing.
        assert!(w.comecar_permanencia(ids[1]));
        let e1 = w.sessions[&ids[1]].entity.unwrap();
        // Input that arrives anyway is not there: the body stands still.
        let antes = w.ecs.get::<&Position>(e1).unwrap().0;
        segundos(&mut w, 60);
        assert!(w.sessions.contains_key(&ids[1]), "still in the world after 60 s");
        assert_eq!(w.ecs.get::<&Position>(e1).unwrap().0, antes, "frozen");
        // Attackable: a red name is fair game for a Peaceful attacker.
        assert!(w.can_damage_player(EntityId(900), EntityId(901)));
        segundos(&mut w, 61);
        assert!(!w.sessions.contains_key(&ids[1]), "gone after 120 s");
        assert!(w.characters.contains_key("pk1"), "saved");
    }

    #[test]
    fn combate_pk_recente_tambem_fica() {
        let (mut w, ids) = mundo();
        w.sessions.get_mut(&ids[0]).unwrap().pk_modo = shared::pk::LIVRE;
        golpe(&mut w, 1);
        assert!(w.em_combate_pk(&w.sessions[&ids[0]]) && w.em_combate_pk(&w.sessions[&ids[1]]), "both sides");
        cair(&mut w, ids[0]);
        assert!(w.esta_permanecendo(&ids[0]), "attacked a player 0 s ago");
        // After the 30 s with no PvP, a clean name leaves at once.
        let (mut w, ids) = mundo();
        w.sessions.get_mut(&ids[0]).unwrap().pk_combate_ate = 1.0;
        w.sim_time_s = 40.0;
        cair(&mut w, ids[0]);
        assert!(!w.sessions.contains_key(&ids[0]));
    }

    #[test]
    fn morto_enquanto_fica_paga_a_morte_e_sai_salvo() {
        let (mut w, ids) = mundo();
        w.sessions.get_mut(&ids[0]).unwrap().pk_modo = shared::pk::LIVRE;
        let s1 = w.sessions.get_mut(&ids[1]).unwrap();
        s1.pk_points = 1;
        s1.xp = 50_000_000;
        cair(&mut w, ids[1]);
        golpe(&mut w, 100_000);
        assert!(w.sessions[&ids[1]].downed, "killed while lingering");
        let xp_morto = w.sessions[&ids[1]].xp;
        assert!(xp_morto < 50_000_000, "the death penalty ran");
        assert_eq!(w.sessions[&ids[1]].cartas_de_abate.len(), 1, "and the letter");
        segundos(&mut w, 1);
        assert!(!w.sessions.contains_key(&ids[1]), "a fallen body leaves");
        let row = &w.characters["pk1"];
        assert_eq!(row.xp, xp_morto, "the death is saved");
    }

    #[test]
    fn voltar_retoma_o_mesmo_corpo() {
        let (mut w, ids) = mundo();
        w.sessions.get_mut(&ids[1]).unwrap().pk_points = 3;
        let e1 = w.sessions[&ids[1]].entity.unwrap();
        w.ecs.get::<&mut Position>(e1).unwrap().0 = Vec2::new(42.0, 17.0);
        w.ecs.get::<&mut Health>(e1).unwrap().current = 37;
        cair(&mut w, ids[1]);
        assert!(w.religar_permanencia("pk1", 2));
        assert!(!w.sessions.contains_key(&ids[1]), "the old session went");
        let row = &w.characters["pk1"];
        assert_eq!((row.pos, row.hp.current, row.pk.pontos), (Vec2::new(42.0, 17.0), 37, 3), "the same body");
        assert!(w.copia_fresca("pk1"), "the login spawns from it");
        assert!(!w.religar_permanencia("pk1", 2), "only once");
        // Another account cannot take it.
        let (mut w, ids) = mundo();
        w.sessions.get_mut(&ids[1]).unwrap().pk_points = 3;
        cair(&mut w, ids[1]);
        assert!(!w.religar_permanencia("pk1", 99));
        assert!(w.esta_permanecendo(&ids[1]));
    }

    #[test]
    fn viagem_e_novo_login_nao_furam_a_permanencia() {
        let (mut w, ids) = mundo();
        w.diretorio = Some(crate::canais::Diretorio::para_teste(&["ilha_gelo"]));
        w.sessions.get_mut(&ids[0]).unwrap().pk_modo = shared::pk::LIVRE;
        golpe(&mut w, 1);
        // In PK combat: the trip is refused, nobody is handed off.
        assert!(!w.mandar_para_zona(ids[0], "ilha_gelo", Vec2::ZERO, None, None));
        assert!(!w.zona_de_saida.contains_key("pk0"));
        // 30 s later the trip goes.
        w.sim_time_s += shared::pk::COMBATE_PK_S + 1.0;
        assert!(w.mandar_para_zona(ids[0], "ilha_gelo", Vec2::ZERO, None, None));
        // A handoff is not a logout: the socket closing for it never lingers,
        // even with a red name.
        w.sessions.get_mut(&ids[0]).unwrap().pk_points = 5;
        cair(&mut w, ids[0]);
        assert!(!w.sessions.contains_key(&ids[0]));
        // The "new login wins" rule skips a lingering body.
        let (mut w, ids) = mundo();
        w.sessions.get_mut(&ids[1]).unwrap().pk_points = 1;
        cair(&mut w, ids[1]);
        let (tx, rx) = mpsc::unbounded_channel();
        std::mem::forget(rx);
        let novo = SessionId(([127, 0, 0, 1], 19899).into());
        w.on_connect(SessionHandle { id: novo, to_client: tx });
        w.on_auth_result(
            novo,
            Ok(crate::auth::AuthSuccess { sessao: None, account_id: 2, username: "pk1".into(), class: "warrior".into() }),
        );
        assert!(w.esta_permanecendo(&ids[1]), "logging in again does not end the linger");
    }

    #[test]
    fn carta_so_para_morte_por_jogador() {
        let (mut w, ids) = mundo();
        w.sessions.get_mut(&ids[0]).unwrap().pk_modo = shared::pk::LIVRE;
        w.sessions.get_mut(&ids[0]).unwrap().renascimentos = 2;
        golpe(&mut w, 100_000);
        assert!(w.sessions[&ids[1]].downed);
        let cartas = &w.sessions[&ids[1]].cartas_de_abate;
        assert_eq!(cartas.len(), 1, "exactly one letter");
        let (assunto, texto, matador) = &cartas[0];
        assert_eq!(assunto, "You were killed by pk0");
        assert_eq!(matador, "pk0");
        assert!(texto.starts_with("pk0 [R2 · Lv "), "{texto}");
        assert!(texto.contains("(10, 10)") || texto.contains("(11, 10)"), "the place: {texto}");
        assert!(texto.contains("(Brasília time)"), "the time: {texto}");
        // A Peaceful clean victim: the killer is red now, and the letter says so.
        assert!(texto.contains("The killer has PK"), "{texto}");
        // More ticks do not send it again.
        for _ in 0..30 {
            w.step(shared::TICK_DT);
        }
        assert_eq!(w.sessions[&ids[1]].cartas_de_abate.len(), 1);
        assert!(w.sessions[&ids[0]].cartas_de_abate.is_empty(), "the killer gets none");
        // A death with no player behind it (a mob, the sea): no letter.
        let (mut w, ids) = mundo();
        let e1 = w.sessions[&ids[1]].entity.unwrap();
        w.ecs.get::<&mut Health>(e1).unwrap().current = 0;
        w.step(shared::TICK_DT);
        assert!(w.sessions[&ids[1]].downed);
        assert!(w.sessions[&ids[1]].cartas_de_abate.is_empty(), "a mob kill sends none");
    }
}
