//! THE PEACEFUL WATERS (docs/NAVEGACAO.md, "Águas pacíficas", 17/11/2026).
//!
//! The owner: "coloca a zona da ilha 1 e 2 safe zone no mar, até a 3 na
//! verdade, aí começa a PK da zona 4 pra cima" and "caso alguém esteja PK
//! já, ele tem que ser barrado de entrar nessas zonas".
//!
//! - **No PvP there** (`shared::mar::aguas_pacificas`: the waters of the
//!   Bosque, the Glacier and the Waste): `can_damage_player` refuses any
//!   hit with either ship inside, whatever the PK modes or a treasure mark
//!   say; the Long Gun at a ship there is refused with the reason. Sea mobs
//!   still bite and are fought.
//! - **A PK captain is barred** (`pk_barrado`: PK points, or a PvP hit in
//!   the last `pk::COMBATE_PK_S`). Its hull stops at the boundary and slides
//!   along it like a coast (`mover_raspando` with the peaceful waters as
//!   land, in `step`); its water routes go round them, and a route that
//!   ENDS in them is refused with the reason.
//! - **Already inside when it turns PK** (it just fired back, or it cast off
//!   from a peaceful dock with PK points): `GRACA_S` to leave, then the sea
//!   pushes it out at `EMPURRAO` u/s towards the nearest boundary.
//! - **Never trapped.** A PK captain still casts off from any dock (a
//!   peaceful one starts the grace), still uses the Harbour Captain's fast
//!   travel and the land's respawn, so it always gets home: docking at the
//!   Plateau, Skyreach, Kōgen-tō or Abyssia and taking the Captain, or
//!   waiting out / forgiving the PK points.

use super::*;

/// Seconds a PK ship caught inside peaceful waters has to leave them.
pub(crate) const GRACA_S: f32 = 30.0;
/// The gentle push out after that, units per second.
pub(crate) const EMPURRAO: f32 = 5.0;
/// One "closed to you" warning per this many seconds.
const AVISO_S: f32 = 4.0;

pub(crate) const BARRADO: &str = "You have PK: peaceful waters are closed to you.";
pub(crate) const ROTA_RECUSADA: &str =
    "You have PK: you can't enter peaceful waters until it clears (forgive PK points or wait).";
pub(crate) const SEM_PVP: &str = "Peaceful waters: no PvP here.";

/// The way out of peaceful waters from `p`: towards a point 10 u past the
/// nearest piece of the boundary (`mar::fronteira_pacifica`), on its PvP
/// side.
pub(crate) fn saida_das_pacificas(p: Vec2) -> Vec2 {
    let Some((a, b)) = shared::mar::fronteira_pacifica()
        .iter()
        .min_by(|x, y| ((x.0 + x.1) * 0.5).distance_squared(p).total_cmp(&((y.0 + y.1) * 0.5).distance_squared(p)))
    else {
        return Vec2::ZERO;
    };
    let m = (*a + *b) * 0.5;
    let t = (*b - *a).normalize_or_zero();
    let n = Vec2::new(-t.y, t.x);
    let fora = if shared::mar::aguas_pacificas(m + n * 8.0) { -n } else { n };
    (m + fora * 10.0 - p).try_normalize().unwrap_or(fora)
}

impl GameWorld {
    /// May this captain NOT enter peaceful waters (a red name, or PK combat)?
    pub(crate) fn pk_barrado(&self, s: &Session) -> bool {
        s.pk_points > 0 || self.em_combate_pk(s)
    }

    /// Is either of these two players in peaceful waters at sea?
    pub(crate) fn pvp_em_aguas_pacificas(&self, a: EntityId, b: EntityId) -> bool {
        if !self.no_mar() {
            return false;
        }
        [a, b].iter().any(|id| {
            self.sessions
                .values()
                .find(|s| s.entity_id == *id)
                .and_then(|s| s.entity)
                .and_then(|e| self.ecs.get::<&Position>(e).ok().map(|p| p.0))
                .is_some_and(shared::mar::aguas_pacificas)
        })
    }

    /// For `step`'s movement, per hull at sea: `(barred from peaceful
    /// waters, the push out once the grace is over)`. Also runs the grace's
    /// clock (called once per tick, before the bodies move).
    pub(crate) fn barreira_pacifica(&mut self) -> HashMap<hecs::Entity, (bool, Option<Vec2>)> {
        let mut m = HashMap::new();
        if !self.no_mar() {
            return m;
        }
        let agora = self.sim_time_s;
        let sids: Vec<SessionId> = self.sessions.iter().filter(|(_, s)| s.logged_in).map(|(id, _)| *id).collect();
        for sid in sids {
            let s = &self.sessions[&sid];
            let Some(e) = s.entity else { continue };
            let Some(p) = self.ecs.get::<&Position>(e).ok().map(|p| p.0) else { continue };
            let barrado = self.pk_barrado(s);
            let dentro = shared::mar::aguas_pacificas(p);
            let s = self.sessions.get_mut(&sid).unwrap();
            if !(barrado && dentro) {
                s.pk_nas_pacificas_desde = None;
                if barrado {
                    m.insert(e, (true, None));
                }
                continue;
            }
            let desde = *s.pk_nas_pacificas_desde.get_or_insert(agora);
            let empurra = (agora - desde >= GRACA_S).then(|| saida_das_pacificas(p) * EMPURRAO);
            let primeiro = desde == agora;
            m.insert(e, (false, empurra));
            if primeiro {
                self.avisa_missao(sid, format!("You have PK: leave the peaceful waters within {} s.", GRACA_S as u32));
            }
        }
        m
    }

    /// A barred hull just hit the boundary: say why (rate-limited).
    pub(crate) fn avisa_barreira(&mut self, e: hecs::Entity) {
        let agora = self.sim_time_s;
        let Some(sid) = self.sessions.iter().find(|(_, s)| s.entity == Some(e)).map(|(id, _)| *id) else {
            return;
        };
        let s = self.sessions.get_mut(&sid).unwrap();
        if agora - s.aviso_da_barreira_em < AVISO_S {
            return;
        }
        s.aviso_da_barreira_em = agora;
        self.avisa_missao(sid, BARRADO.into());
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    fn mundo() -> (GameWorld, SessionId, hecs::Entity) {
        crate::economy::init_vazia_para_testes();
        let mut w = GameWorld::new(HashMap::new());
        w.zona = shared::mar::ZONA.into();
        w.ilha = Some(shared::terreno::Ilha::da_ilha(&shared::mar::DEF));
        let sid = SessionId(([127, 0, 0, 1], 47_490).into());
        let (tx, rx) = mpsc::unbounded_channel();
        std::mem::forget(rx);
        w.on_connect(SessionHandle { id: sid, to_client: tx });
        let eid = EntityId(4_900);
        let e = w.ecs.spawn((
            NetId(eid),
            Position(Vec2::ZERO),
            Velocity(Vec2::ZERO),
            EntityKind::Player,
            Health { current: 100, max: 100 },
            PlayerTag { name: "Corsario".into(), player_id: PlayerId(49), attack_anim_pending: None, combo_step_pending: None },
            shared::Solido,
        ));
        let s = w.sessions.get_mut(&sid).unwrap();
        s.logged_in = true;
        s.entity = Some(e);
        s.entity_id = eid;
        s.name = "Corsario".into();
        s.inventory = vec![shared::InventorySlot::default(); shared::INVENTORY_SLOTS];
        navio::dar_chalupa(s);
        (w, sid, e)
    }

    /// A straight run of water that crosses INTO peaceful waters: (outside,
    /// direction in). Open water both sides, no harbour corridor.
    fn travessia() -> (Vec2, Vec2) {
        for (a, b) in shared::mar::fronteira_pacifica() {
            let m = (*a + *b) * 0.5;
            let t = (*b - *a).normalize_or_zero();
            let n = Vec2::new(-t.y, t.x);
            let dentro = if shared::mar::aguas_pacificas(m + n * 4.0) { n } else { -n };
            let fora = m - dentro * 14.0;
            let ok = (0..=30).all(|k| {
                let q = fora + dentro * k as f32;
                shared::mar::agua_livre(q) && !shared::mar::no_corredor(q)
            });
            if ok && !shared::mar::aguas_pacificas(fora) && shared::mar::aguas_pacificas(fora + dentro * 28.0) {
                return (fora, dentro);
            }
        }
        panic!("no open-water crossing into peaceful waters");
    }

    fn navega(w: &mut GameWorld, sid: SessionId, dir: Vec2, s: f32) {
        let mut n = 0;
        let t0 = w.sim_time_s;
        while w.sim_time_s - t0 < s {
            n += 1;
            w.sessions.get_mut(&sid).unwrap().pending_input =
                Some(shared::protocol::InputFrame { seq: n, tick: n, move_dir: dir, aim: Vec2::ZERO, buttons: 0 });
            w.step(shared::TICK_DT);
        }
    }

    /// A PK SHIP STOPS AT THE BOUNDARY (stick), a clean one sails in.
    #[test]
    fn o_navio_pk_nao_entra_nas_aguas_pacificas() {
        let (fora, dentro) = travessia();
        for pk in [false, true] {
            let (mut w, sid, e) = mundo();
            w.ecs.get::<&mut Position>(e).unwrap().0 = fora;
            w.sessions.get_mut(&sid).unwrap().pk_points = u32::from(pk);
            navega(&mut w, sid, dentro, 6.0);
            let p = w.pos_do_jogador(sid).unwrap();
            if pk {
                assert!(!shared::mar::aguas_pacificas(p), "a PK ship got into peaceful waters at {p:?}");
                assert!(p.distance(fora) > 6.0, "it did not even reach the boundary: {p:?}");
            } else {
                assert!(shared::mar::aguas_pacificas(p), "a clean ship was kept out: {p:?}");
            }
        }
        // PK COMBAT bars too, not only the red name.
        let (mut w, sid, e) = mundo();
        w.ecs.get::<&mut Position>(e).unwrap().0 = fora;
        w.sessions.get_mut(&sid).unwrap().pk_combate_ate = 1e9;
        navega(&mut w, sid, dentro, 6.0);
        assert!(!shared::mar::aguas_pacificas(w.pos_do_jogador(sid).unwrap()));
    }

    /// BY ROUTE: a PK captain's route to a peaceful dock is refused with the
    /// reason; to a PvP dock it goes round the peaceful waters; a clean
    /// captain's goes anywhere.
    #[test]
    fn a_rota_do_pk_contorna_e_recusa_as_aguas_pacificas() {
        let ilhas = shared::mar::ilhas();
        let bosque = ilhas.iter().find(|i| i.zona == "ilha_inicial").unwrap().fundeadouro();
        let planalto = ilhas.iter().find(|i| i.zona == "ilha_planalto").unwrap().fundeadouro();
        let ceu = ilhas.iter().find(|i| i.zona == shared::celeste::ZONA).unwrap().fundeadouro();
        let (mut w, sid, e) = mundo();
        w.ecs.get::<&mut Position>(e).unwrap().0 = planalto;
        w.sessions.get_mut(&sid).unwrap().pk_points = 2;
        w.rota_no_mar(sid, planalto, bosque);
        assert!(w.sessions[&sid].rota.vazia(), "a PK route into peaceful waters");
        w.rota_no_mar(sid, planalto, ceu);
        let rota: Vec<Vec2> = w.sessions[&sid].rota.pontos().collect();
        assert!(!rota.is_empty(), "no route to Skyreach for a PK captain");
        let mut de = planalto;
        for q in &rota {
            for k in 0..=40 {
                let r = de.lerp(*q, k as f32 / 40.0);
                assert!(!shared::mar::aguas_pacificas(r), "the PK route crosses peaceful waters at {r:?}");
            }
            de = *q;
        }
        // Clean: to the Bosque at once.
        w.sessions.get_mut(&sid).unwrap().pk_points = 0;
        w.rota_no_mar(sid, planalto, bosque);
        assert!(!w.sessions[&sid].rota.vazia());
    }

    /// CAUGHT INSIDE: `GRACA_S` to leave (sailing freely), then pushed out.
    #[test]
    fn o_pk_dentro_tem_trinta_segundos_e_depois_e_empurrado() {
        let (fora, dentro) = travessia();
        let (mut w, sid, e) = mundo();
        let la = fora + dentro * 24.0;
        w.ecs.get::<&mut Position>(e).unwrap().0 = la;
        w.sessions.get_mut(&sid).unwrap().pk_points = 1;
        navega(&mut w, sid, Vec2::ZERO, GRACA_S - 1.0);
        let p = w.pos_do_jogador(sid).unwrap();
        assert!(p.distance(la) < 0.5, "pushed during the grace: {p:?}");
        // It may still sail about inside during the grace.
        assert!(w.sessions[&sid].pk_nas_pacificas_desde.is_some());
        navega(&mut w, sid, Vec2::ZERO, 12.0);
        let p = w.pos_do_jogador(sid).unwrap();
        assert!(!shared::mar::aguas_pacificas(p), "still inside after the grace: {p:?}");
        assert!(w.sessions[&sid].pk_nas_pacificas_desde.is_none());
    }

    /// NO PvP IN PEACEFUL WATERS: two Free-mode ships, one of them marked,
    /// cannot hurt each other there (and the Long Gun says why); in PvP
    /// waters they can.
    #[test]
    fn nas_aguas_pacificas_o_canhao_nao_atira_em_navio() {
        let (fora, dentro) = travessia();
        let (mut w, sid, e) = mundo();
        let b = SessionId(([127, 0, 0, 1], 47_491).into());
        let (tx, rx) = mpsc::unbounded_channel();
        std::mem::forget(rx);
        w.on_connect(SessionHandle { id: b, to_client: tx });
        let eb = EntityId(4_901);
        let ent_b = w.ecs.spawn((
            NetId(eb),
            Position(Vec2::ZERO),
            Velocity(Vec2::ZERO),
            EntityKind::Player,
            Health { current: 100, max: 100 },
            PlayerTag { name: "Outro".into(), player_id: PlayerId(50), attack_anim_pending: None, combo_step_pending: None },
        ));
        {
            let s = w.sessions.get_mut(&b).unwrap();
            s.logged_in = true;
            s.entity = Some(ent_b);
            s.entity_id = eb;
            s.name = "Outro".into();
            s.inventory = vec![shared::InventorySlot::default(); shared::INVENTORY_SLOTS];
            navio::dar_chalupa(s);
        }
        for s in [sid, b] {
            let s = w.sessions.get_mut(&s).unwrap();
            s.pk_modo = shared::pk::LIVRE;
            s.pk_escolhido = shared::pk::LIVRE;
        }
        let eu = w.sessions[&sid].entity_id;
        for (onde, pode) in [(fora + dentro * 26.0, false), (fora - dentro * 4.0, true)] {
            w.ecs.get::<&mut Position>(e).unwrap().0 = onde;
            w.ecs.get::<&mut Position>(ent_b).unwrap().0 = onde + Vec2::new(0.0, 0.0) + dentro.perp() * 8.0;
            assert_eq!(w.can_damage_player(eu, eb), pode, "at {onde:?}");
        }
        // The Long Gun at a ship in peaceful waters: refused, with the reason.
        let onde = fora + dentro * 26.0;
        w.ecs.get::<&mut Position>(e).unwrap().0 = onde;
        w.ecs.get::<&mut Position>(ent_b).unwrap().0 = onde + dentro.perp() * 8.0;
        w.sessions.get_mut(&sid).unwrap().target = Some(eb);
        w.handle_navio(sid, shared::navio::PedidoNavio::Habilidade { id: 1 });
        assert!(w.sessions[&sid].skill_cds.is_empty(), "the Long Gun fired at a ship in peaceful waters");
    }

    /// NEVER TRAPPED: a PK captain on a peaceful island still takes the
    /// Harbour Captain's fast travel (to another island, for copper), and
    /// still casts off from its dock (the grace then lets it sail out).
    #[test]
    fn o_pk_volta_pra_casa_pelo_capitao() {
        assert!(GRACA_S >= 20.0, "the grace must let a ship cast off and leave");
        let bosque = shared::mar::vista_da_zona("ilha_inicial").unwrap();
        assert!(shared::mar::aguas_pacificas(bosque.fundeadouro()));
        // From the Bosque's dock, the way out by route is open during the grace.
        let saida = saida_das_pacificas(bosque.fundeadouro());
        assert!(saida.length() > 0.5);
        assert!(navio::caminho_no_mar_com(bosque.fundeadouro(), shared::mar::vista_da_zona("ilha_planalto").unwrap().fundeadouro(), true).is_some());
    }
}
