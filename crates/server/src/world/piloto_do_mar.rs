//! AUTO COMBAT AT SEA, STEERED BY THE SERVER (docs/NAVEGACAO.md, "Auto
//! combate no mar, refeito").
//!
//! The owner: "o auto combat no barco tá muito ruim". Measured headless
//! (`testes` below): the old client-side auto sailed up to 8.5 m of the
//! target, STOPPED, and sat there firing while the sharks swam in and bit
//! the hull every two seconds — it never moved again until the target died.
//!
//! Now the client switches AUTO on (`PedidoNavio::Piloto`) and keeps choosing
//! the target as on land; the SERVER steers the hull, ten decisions a second,
//! straight into `frame.move_dir` (no `MoverPara` round trips, no stutter):
//!
//! - **kite and broadside:** a tangential course around the target, so the
//!   target sits off the beam; the Broadside fires only with the target (or
//!   two mobs) on a side, the Long Gun with the target on a side inside its
//!   22 m. Every gun is a side gun (16/11/2026): no bow gun, no heaving to;
//! - **the helm has weight** (turning inertia, `navio::Leme`): the orbit is
//!   the hull's own (`navio::raio_do_piloto`, 7.5 m for a Sloop up to
//!   9.5 m for the big hulls) at the speed the helm can hold round it
//!   (`navio::velocidade_do_piloto`): a Man-o'-War slows down to keep its
//!   side to the target instead of sailing off in a wide arc;
//! - **the bite telegraph is read:** a swing at us steers straight away from
//!   the biter until it lands; any mob closer than `PERTO_DEMAIS` pushes the
//!   hull off;
//! - **hull:** Emergency Repair under `PILOTO_REPARA`; under `PILOTO_RECUA`
//!   with no repair ready, Smoke and a water A* run to the nearest harbour
//!   corridor, back to the fight above `PILOTO_VOLTA`;
//! - land, the harbour corridor and getting stuck flip the circling side;
//! - players only when the client chose one and the PK rules let the cannon
//!   hit them (`can_damage_player`); the pilot's own picks are mobs only.

use super::*;
use shared::navio as nv;

/// Seconds between two decisions (cheap: a handful of distance tests).
const DECISAO_S: f32 = 0.1;
/// The pilot's own pick (no valid client target) looks this far — the
/// client's AUTO area.
const BUSCA: f32 = 24.0;
/// A target further than this is let go (the client hunts it by route).
const LARGA: f32 = 40.0;
/// The Broadside fires with the target inside this (a margin under 12 m:
/// the ball lands where the target WILL be).
const BORDADA_ALCANCE: f32 = 11.6;
/// The Long Gun's, under 22 m.
const LONGO_ALCANCE: f32 = 21.5;
/// A mob closer than this pushes the hull away (a bite reaches ~2 m).
const PERTO_DEMAIS: f32 = 6.0;
/// Further than this beyond the orbit: sail straight at the target.
const APROXIMA: f32 = 7.0;
/// Moved less than `PRESO_M` in `PRESO_S`: stuck, circle the other way.
const PRESO_S: f32 = 1.5;
const PRESO_M: f32 = 1.5;
/// A hunting route that ends this close to the target is the pilot's to
/// take over (a manual tap elsewhere is left alone).
const ROTA_DE_CACA: f32 = 18.0;

/// The pilot's state, one per session (`Session::piloto`).
#[derive(Debug, Clone, Default)]
pub struct PilotoDoMar {
    pub ligado: bool,
    /// The mob it is fighting (sticky until it dies or is lost).
    pub alvo: Option<EntityId>,
    /// Steering for the next ticks (`frame.move_dir`); zero = hands off.
    leme: Vec2,
    /// Circling side: +1 counter-clockwise, -1 clockwise.
    sentido: f32,
    decide_em: f32,
    /// Sailing home to patch the hull (`navio::piloto_recua`).
    pub recuando: bool,
    rota_de_volta_em: f32,
    marca: Vec2,
    marca_em: f32,
    virou_em: f32,
}

impl PilotoDoMar {
    /// What the movement takes this tick (`step`, with the stick idle and no
    /// route).
    pub fn leme(&self) -> Option<Vec2> {
        (self.ligado && self.leme.length_squared() > 0.01).then_some(self.leme)
    }

    fn virar(&mut self, agora: f32) {
        if agora - self.virou_em >= 1.0 {
            self.sentido = -self.sentido;
            self.virou_em = agora;
        }
    }
}

/// A body around the pilot.
#[derive(Clone, Copy)]
struct Bicho {
    id: EntityId,
    p: Vec2,
    /// Can be shot (alive, not going home, not in grace).
    alvejavel: bool,
    /// Where it spawned (its leash is measured from here).
    casa: Vec2,
}

/// Water the hull fits in at `q`, around it too.
fn agua(q: Vec2) -> bool {
    let r = nv::RAIO_DO_CASCO + 0.8;
    shared::mar::navegavel(q.x, q.y)
        && [Vec2::X, Vec2::NEG_X, Vec2::Y, Vec2::NEG_Y]
            .iter()
            .all(|d| shared::mar::navegavel(q.x + d.x * r, q.y + d.y * r))
}

fn gira(v: Vec2, a: f32) -> Vec2 {
    let (s, c) = a.sin_cos();
    Vec2::new(v.x * c - v.y * s, v.x * s + v.y * c)
}

impl GameWorld {
    /// `PedidoNavio::Piloto`: AUTO COMBAT on or off at sea.
    pub(crate) fn ligar_piloto(&mut self, sid: SessionId, ligado: bool) {
        let ligado = ligado && self.no_mar();
        let Some(s) = self.sessions.get_mut(&sid) else {
            return;
        };
        if s.piloto.ligado == ligado {
            return;
        }
        s.piloto = PilotoDoMar { ligado, sentido: 1.0, ..Default::default() };
    }

    /// The pilots of this tick (from `tick_mar`): each decides every
    /// `DECISAO_S`.
    pub(crate) fn tick_piloto(&mut self) {
        let agora = self.sim_time_s;
        let prontos: Vec<SessionId> = self
            .sessions
            .iter()
            .filter(|(_, s)| s.piloto.ligado && s.logged_in && agora >= s.piloto.decide_em)
            .map(|(k, _)| *k)
            .collect();
        for sid in prontos {
            self.decidir_piloto(sid, agora);
        }
    }

    fn skill_pronta(&self, sid: SessionId, h: u8, agora: f32) -> bool {
        self.sessions
            .get(&sid)
            .is_some_and(|s| s.skill_cds.get(&nv::id_de_skill(h)).is_none_or(|&fim| fim <= agora))
    }

    fn decidir_piloto(&mut self, sid: SessionId, agora: f32) {
        let Some(s) = self.sessions.get_mut(&sid) else {
            return;
        };
        s.piloto.decide_em = agora + DECISAO_S;
        s.piloto.leme = Vec2::ZERO;
        if s.piloto.sentido == 0.0 {
            s.piloto.sentido = 1.0;
        }
        let (eu, entity, downed, selecionado) = (s.entity_id, s.entity, s.downed, s.target);
        let Some(entity) = entity.filter(|_| !downed) else {
            return;
        };
        let Some(p) = self.ecs.get::<&Position>(entity).ok().map(|x| x.0) else {
            return;
        };
        // The HELM's heading (turning inertia): a ship lying still still
        // has its sides where they were.
        let rumo = self.rumo_do_casco(sid, entity);
        let Some((navio, d)) = self.navio_equipado(sid) else {
            return;
        };
        let (raio, acelerador) = (
            nv::raio_do_piloto(navio, &d),
            nv::velocidade_do_piloto(navio, &d) / nv::velocidade_do_navio(navio, &d).max(0.1),
        );
        if !nv::pode_zarpar(&d) {
            return;
        }
        let fracao = |c: u16| c as f32 / nv::casco_total(navio, &d).max(1) as f32;
        let mut casco = fracao(d.casco);

        // ── 1. THE HULL ──
        let recuando_antes = self.sessions[&sid].piloto.recuando;
        if (casco < nv::PILOTO_REPARA || (recuando_antes && casco < nv::PILOTO_VOLTA))
            && self.skill_pronta(sid, 2, agora)
        {
            self.habilidade_do_navio(sid, 2);
            casco = self.navio_equipado(sid).map_or(casco, |(_, d)| fracao(d.casco));
        }
        let recua = nv::piloto_recua(casco, self.skill_pronta(sid, 2, agora), recuando_antes);
        if recua {
            if !recuando_antes && self.skill_pronta(sid, 3, agora) {
                self.habilidade_do_navio(sid, 3);
            }
            let no_corredor = shared::mar::no_corredor(p);
            let volta_em = self.sessions[&sid].piloto.rota_de_volta_em;
            let sem_rota = self.sessions[&sid].rota.vazia();
            if let Some(s) = self.sessions.get_mut(&sid) {
                s.piloto.recuando = true;
                s.piloto.alvo = None;
            }
            if !no_corredor && sem_rota && agora >= volta_em {
                let cais = shared::mar::ilhas()
                    .iter()
                    .map(|i| i.fundeadouro())
                    .min_by(|a, b| a.distance(p).total_cmp(&b.distance(p)));
                if let Some(cais) = cais {
                    self.rota_no_mar(sid, p, cais);
                }
                if let Some(s) = self.sessions.get_mut(&sid) {
                    s.piloto.rota_de_volta_em = agora + 2.0;
                }
            }
            return;
        }
        if let Some(s) = self.sessions.get_mut(&sid) {
            s.piloto.recuando = false;
        }

        // ── 2. WHO IS AROUND, AND THE TARGET ──
        let mut perto: Vec<Bicho> = Vec::new();
        for (e, (n, pos, hp, tag)) in self.ecs.query::<(&NetId, &Position, &Health, &EnemyTag)>().iter() {
            if hp.current <= 0 || tag.dead || pos.0.distance(p) > LARGA {
                continue;
            }
            let alvejavel = !tag.returning_home
                && tag.spawn_grace_until <= agora
                && self.ecs.get::<&Untargetable>(e).is_err()
                && !self.in_safe_zone(pos.0);
            perto.push(Bicho { id: n.0, p: pos.0, alvejavel, casa: tag.spawn_anchor });
        }
        let do_cliente = selecionado.and_then(|id| {
            if let Some(b) = perto.iter().find(|b| b.id == id) {
                return b.alvejavel.then_some((id, b.p));
            }
            // A player the client chose (its AUTO PvP setting): only when
            // the cannon may hit them.
            let q = self
                .ecs
                .query::<(&NetId, &Position, &Health, &EntityKind)>()
                .iter()
                .find(|(_, (n, _, _, k))| n.0 == id && matches!(k, EntityKind::Player))
                .map(|(e, (_, pos, hp, _))| (e, pos.0, hp.current));
            let (e, q, hp) = q?;
            (hp > 0
                && q.distance(p) <= LARGA
                && self.can_damage_player(eu, id)
                && !self.in_safe_zone(q)
                && self.ecs.get::<&Untargetable>(e).is_err())
            .then_some((id, q))
        });
        let fixo = self.sessions[&sid].piloto.alvo;
        let alvo = do_cliente
            .or_else(|| fixo.and_then(|id| perto.iter().find(|b| b.id == id && b.alvejavel).map(|b| (b.id, b.p))))
            .or_else(|| {
                // The nearest in the AUTO area; failing that, the nearest the
                // pilot still sees (a heavy hull's wide orbit can leave the
                // nest's last mob just outside 24 m: never sit idle by it).
                let mais_perto = |raio: f32| {
                    perto
                        .iter()
                        .filter(|b| b.alvejavel && b.p.distance(p) <= raio)
                        .min_by(|a, b| a.p.distance_squared(p).total_cmp(&b.p.distance_squared(p)).then(a.id.0.cmp(&b.id.0)))
                        .map(|b| (b.id, b.p))
                };
                mais_perto(BUSCA).or_else(|| mais_perto(LARGA))
            });
        // Nothing to shoot (the mobs in their spawn grace, or going home
        // after their leash): keep sailing near the nearest one instead of
        // lying idle — it is shot the moment it turns (`so_seguir`).
        let (alvo, t, so_seguir) = match alvo {
            Some((a, t)) => (a, t, false),
            None => {
                if let Some(s) = self.sessions.get_mut(&sid) {
                    s.piloto.alvo = None;
                }
                let Some(b) = perto.iter().min_by(|a, b| a.p.distance_squared(p).total_cmp(&b.p.distance_squared(p))) else {
                    return;
                };
                (b.id, b.p, true)
            }
        };
        let casa_do_alvo = perto.iter().find(|b| b.id == alvo).map(|b| b.casa);
        if let Some(s) = self.sessions.get_mut(&sid).filter(|_| !so_seguir) {
            s.piloto.alvo = Some(alvo);
            s.target = Some(alvo);
            // The client's hunting route to this very fight: the pilot takes it
            // from here (a tap somewhere else is the player's, left alone).
            if !s.rota.vazia() && s.rota.destino().distance(t) <= ROTA_DE_CACA {
                s.rota.limpa();
            }
        }
        let dist = p.distance(t);

        // ── 3. FIRE ── (from the sides only: `navio::no_costado` on the heading)
        if !self.in_safe_zone(p) && !so_seguir {
            if self.skill_pronta(sid, 0, agora) {
                let no_alcance = |b: &&Bicho| b.alvejavel && b.p.distance(p) <= BORDADA_ALCANCE;
                let no_costado: Vec<EntityId> =
                    perto.iter().filter(no_alcance).filter(|b| nv::no_costado(p, rumo, b.p)).map(|b| b.id).collect();
                let alvo_no_costado =
                    no_costado.contains(&alvo) || (dist <= BORDADA_ALCANCE && nv::no_costado(p, rumo, t));
                if alvo_no_costado || no_costado.len() >= 2 {
                    self.habilidade_do_navio(sid, 0);
                }
            }
            // The long gun is a SIDE gun too (no bow gun since 16/11/2026).
            if self.skill_pronta(sid, 1, agora)
                && dist <= LONGO_ALCANCE
                && nv::no_costado(p, rumo, t)
                && visada_de_tiro(self.ilha.as_ref(), &self.map, p, t)
            {
                self.habilidade_do_navio(sid, 1);
            }
        }

        // ── 4. STEER ── (a route alive is a tap of the player's: hands off)
        if !self.sessions[&sid].rota.vazia() {
            return;
        }
        let u = (t - p).try_normalize().unwrap_or(Vec2::X);
        let sentido = self.sessions[&sid].piloto.sentido;
        let tangente = Vec2::new(-u.y, u.x) * sentido;
        // The throttle: the orbit's speed (what the helm holds round it),
        // full ahead to close in or to get away from a bite.
        let mut acelera = acelerador;
        let mut dir = if dist > raio + APROXIMA {
            acelera = 1.0;
            u + tangente * 0.35
        } else {
            let k = ((dist - raio) / 3.0).clamp(-1.0, 1.0);
            tangente + u * k * 1.2
        };
        // THE LEASH: the target chases the hull, so the orbit drifts with the
        // fight; a heavy hull's wide, slow orbit dragged it off its nest
        // until it gave up and swam home to heal (measured: a Galleon stood
        // 6 s by a mob going home). Past 8 m from its spawn, lean the orbit
        // toward the nest: the mob follows us back.
        if let Some(casa) = casa_do_alvo {
            let longe = t.distance(casa);
            if longe > 8.0 {
                dir += (casa - p).normalize_or_zero() * ((longe - 8.0) / 6.0).clamp(0.0, 1.0);
            }
        }
        // The bite telegraph: a swing at us — sail straight off the biter.
        for g in self.pending_melee.iter().filter(|g| !g.from_player && g.target == Some(eu) && g.impact_at >= agora) {
            let fora = (p - g.pos).try_normalize().unwrap_or(-u);
            dir = fora * 1.6 + tangente * 0.4;
            acelera = 1.0;
        }
        // Nobody gets close: every mob inside `PERTO_DEMAIS` pushes the hull.
        for b in &perto {
            let db = b.p.distance(p);
            if db < PERTO_DEMAIS {
                let fora = (p - b.p).try_normalize().unwrap_or(-u);
                dir += fora * (PERTO_DEMAIS - db) / PERTO_DEMAIS * 2.5;
            }
        }
        let mut dir = dir.try_normalize().unwrap_or(tangente);
        // Land and the harbour corridor (no fighting there) are walls: turn
        // until the bow clears; a hard turn means circling the other way.
        let ja_no_corredor = shared::mar::no_corredor(p);
        let livre = |q: Vec2| agua(q) && (ja_no_corredor || !shared::mar::no_corredor(q));
        if !(livre(p + dir * 4.0) && livre(p + dir * 2.0)) {
            let achado = [0.4f32, -0.4, 0.8, -0.8, 1.2, -1.2, 1.6, -1.6, 2.2, -2.2, std::f32::consts::PI]
                .into_iter()
                .find(|a| livre(p + gira(dir, *a) * 4.0) && livre(p + gira(dir, *a) * 2.0));
            if let Some(a) = achado {
                dir = gira(dir, a);
                if a.abs() >= 1.2 {
                    if let Some(s) = self.sessions.get_mut(&sid) {
                        s.piloto.virar(agora);
                    }
                }
            }
        }
        let Some(s) = self.sessions.get_mut(&sid) else {
            return;
        };
        // Stuck (wreckage, a coast the probe missed, another hull): circle
        // the other way.
        if agora - s.piloto.marca_em >= PRESO_S {
            if s.piloto.marca.distance(p) < PRESO_M && s.piloto.marca_em > 0.0 {
                s.piloto.virar(agora);
            }
            s.piloto.marca = p;
            s.piloto.marca_em = agora;
        }
        s.piloto.leme = dir * acelera.clamp(0.2, 1.0);
    }
}

#[cfg(test)]
mod testes {
    use super::*;
    use shared::navio::PedidoNavio;
    use std::collections::HashSet;

    /// A world at sea with the real relief, the zones of `povoar_mar`, the
    /// real sea mobs' table rows, and the player on a Sloop at the level of
    /// nest `ninho`, 24 m off it.
    fn no_ninho(ninho: usize) -> (GameWorld, SessionId, Vec2) {
        no_ninho_de(ninho, shared::item_id::NAVIO_CHALUPA)
    }

    /// The same, on hull class `casco` (new, no stars).
    fn no_ninho_de(ninho: usize, casco: u16) -> (GameWorld, SessionId, Vec2) {
        crate::economy::por_kinds_do_seed_para_testes(&[navio::TUBARAO, navio::ENGUIA]);
        let mut w = GameWorld::new(HashMap::new());
        w.zona = shared::mar::ZONA.into();
        w.ilha = Some(shared::terreno::Ilha::da_ilha(&shared::mar::DEF));
        w.povoar_mar();
        let sid = SessionId(([127, 0, 0, 1], 47_480).into());
        let (tx, rx) = mpsc::unbounded_channel();
        std::mem::forget(rx);
        w.on_connect(SessionHandle { id: sid, to_client: tx });
        let centro = shared::mar::ninhos()[ninho];
        let nivel = shared::mar::nivel_em(centro);
        // Off the nest, on the side away from the nearest island.
        let ilha = shared::mar::ilhas().iter().map(|i| i.centro).min_by(|a, b| a.distance(centro).total_cmp(&b.distance(centro))).unwrap();
        let fora = (centro - ilha).try_normalize().unwrap_or(Vec2::X);
        let inicio = w.agua_no_mar(centro + fora * 24.0);
        let eid = EntityId(4_800);
        let e = w.ecs.spawn((
            NetId(eid),
            Position(inicio),
            Velocity(Vec2::ZERO),
            EntityKind::Player,
            Health { current: 100, max: 100 },
            PlayerTag { name: "Piloto".into(), player_id: PlayerId(48), attack_anim_pending: None, combo_step_pending: None },
            // A body: the tick moves only what has one.
            shared::Solido,
        ));
        let s = w.sessions.get_mut(&sid).unwrap();
        s.logged_in = true;
        s.entity = Some(e);
        s.entity_id = eid;
        s.name = "Piloto".into();
        s.inventory = vec![shared::InventorySlot::default(); shared::INVENTORY_SLOTS];
        s.xp = shared::xp_for_level_with_mult(nivel, crate::economy::xp_multiplier());
        navio::dar_chalupa(s);
        s.equipment.set(shared::EquipSlot::Navio, Some(casco), Some(nv::instancia_nova(casco)));
        (w, sid, centro)
    }

    /// How AUTO COMBAT plays the fight.
    #[derive(Clone, Copy, PartialEq)]
    enum Modo {
        /// The client-side auto before this remake, reproduced step by step
        /// (`main.rs` `conduzir_auto_combate` + `ir_ate_o_alvo` +
        /// `mar_ui::BarraDoNavio::automatico`): 4 Hz scan of the nearest,
        /// `MoverPara` to 7.2 m every 0.6 s, stop at 8.5 m, fire whatever
        /// is ready and in reach (Broadside on the beam or standing still).
        Antes,
        /// The server's pilot.
        Piloto,
    }

    #[derive(Debug, Default)]
    struct Placar {
        /// Seconds until the nest's mobs were all sunk (`None` = not in time).
        tempo: Option<f32>,
        casco_perdido: f32,
        tiros: u32,
        acertos: u32,
        bordadas_vazias: u32,
        mordidas: u32,
        mordidas_anunciadas: u32,
        afundou: bool,
        /// Longest stretch, with a mob alive, moving under 0.5 m per second.
        parado_max: f32,
        /// Furthest the hull went from the nest's centre.
        longe_max: f32,
    }

    fn input(w: &mut GameWorld, sid: SessionId, n: u32) {
        w.sessions.get_mut(&sid).unwrap().pending_input = Some(shared::protocol::InputFrame {
            seq: n,
            tick: n,
            move_dir: Vec2::ZERO,
            aim: Vec2::ZERO,
            buttons: 0,
        });
    }

    fn pos_de(w: &GameWorld, id: EntityId) -> Option<Vec2> {
        w.ecs.query::<(&NetId, &Position)>().iter().find(|(_, (n, _))| n.0 == id).map(|(_, (_, p))| p.0)
    }

    fn vivo(w: &GameWorld, id: EntityId) -> bool {
        w.ecs
            .query::<(&NetId, &Health, &EnemyTag)>()
            .iter()
            .any(|(_, (n, h, t))| n.0 == id && h.current > 0 && !t.dead)
    }

    /// THE FIGHT: nest `ninho` (`povoar_mar`'s own mobs, real stats), the
    /// player 24 m off with AUTO COMBAT on in `modo`, for up to `teto_s`.
    fn luta(ninho: usize, modo: Modo, teto_s: f32, log: bool) -> Placar {
        luta_de(ninho, shared::item_id::NAVIO_CHALUPA, modo, teto_s, log)
    }

    /// THE FIGHT on hull class `casco`.
    fn luta_de(ninho: usize, casco: u16, modo: Modo, teto_s: f32, log: bool) -> Placar {
        let (mut w, sid, centro) = no_ninho_de(ninho, casco);
        let eu = w.sessions[&sid].entity_id;
        let zona = navio::ZONA_DO_MAR_ID + ninho as u32;
        // The nest fills (the player is near), the spawn grace passes.
        let mut n = 0u32;
        let mut ninhada: Vec<EntityId> = Vec::new();
        for _ in 0..(shared::TICK_RATE_HZ * 4) {
            n += 1;
            input(&mut w, sid, n);
            w.step(shared::TICK_DT);
            ninhada = w
                .ecs
                .query::<(&NetId, &SpawnedByZone)>()
                .iter()
                .filter(|(_, (_, z))| z.zone_id == zona)
                .map(|(_, (id, _))| id.0)
                .collect();
        }
        assert!(ninhada.len() >= 3, "nest {ninho} did not fill: {ninhada:?}");
        let (navio_id, d0) = w.navio_equipado(sid).unwrap();
        let cheio = nv::casco_total(navio_id, &d0) as f32;
        if modo == Modo::Piloto {
            w.handle_navio(sid, PedidoNavio::Piloto { ligado: true });
        }
        let t0 = w.sim_time_s;
        let mut placar = Placar::default();
        let mut visto: HashSet<hecs::Entity> = HashSet::new();
        let mut no_ar: Vec<(hecs::Entity, navio::BalaDeCanhao)> = Vec::new();
        let mut casco_antes = d0.casco;
        let mut swings: HashSet<(u32, u32)> = HashSet::new();
        let mut bordada_cd = 0.0f32;
        let mut parado_desde = t0;
        let mut marca = (w.pos_do_jogador(sid).unwrap(), t0);
        // The old client's state.
        let (mut alvo, mut varre_em, mut aprox_em, mut aproximando, mut pendente, mut tentar) =
            (None::<EntityId>, f32::MIN, f32::MIN, false, 0.0f32, [0.0f32; 2]);
        let mut log_em = t0;
        while w.sim_time_s - t0 < teto_s {
            let agora = w.sim_time_s;
            let p = w.pos_do_jogador(sid).unwrap();
            let cd = w.sessions[&sid].skill_cds.get(&nv::id_de_skill(0)).copied().unwrap_or(0.0);
            let tiros_antes = placar.tiros;
            if modo == Modo::Antes {
                if agora - varre_em >= mar_varredura() {
                    varre_em = agora;
                    let ok = |w: &GameWorld, id: EntityId| vivo(w, id) && pos_de(w, id).is_some_and(|q| q.distance(p) <= 24.0);
                    if !alvo.is_some_and(|id| ok(&w, id)) {
                        alvo = w
                            .ecs
                            .query::<(&NetId, &Position, &Health, &EnemyTag)>()
                            .iter()
                            .filter(|(_, (_, q, h, t))| h.current > 0 && !t.dead && q.0.distance(p) <= 24.0)
                            .min_by(|a, b| a.1 .1 .0.distance(p).total_cmp(&b.1 .1 .0.distance(p)))
                            .map(|(_, (id, _, _, _))| id.0);
                        w.sessions.get_mut(&sid).unwrap().target = alvo;
                        if alvo.is_none() {
                            // The hunt: sail at the nearest mob in sight.
                            let caca = w
                                .ecs
                                .query::<(&Position, &Health, &EnemyTag)>()
                                .iter()
                                .filter(|(_, (q, h, t))| h.current > 0 && !t.dead && q.0.distance(p) <= 110.0)
                                .map(|(_, (q, _, _))| q.0)
                                .min_by(|a, b| a.distance(p).total_cmp(&b.distance(p)));
                            if let Some(q) = caca {
                                w.handle_mover_para(sid, q);
                            }
                        }
                    }
                }
                if let Some(b) = alvo.and_then(|id| pos_de(&w, id)) {
                    let dist = p.distance(b);
                    if dist <= nv::DISTANCIA_DE_COMBATE {
                        if aproximando {
                            w.sessions.get_mut(&sid).unwrap().rota.limpa();
                            aproximando = false;
                        }
                    } else if agora - aprox_em >= 0.6 {
                        aprox_em = agora;
                        w.handle_mover_para(sid, b + (p - b).normalize_or_zero() * nv::DISTANCIA_DE_COMBATE * 0.85);
                        aproximando = true;
                    }
                    let e = w.sessions[&sid].entity.unwrap();
                    let rumo = w.ecs.get::<&Velocity>(e).map(|v| v.0).unwrap_or(Vec2::ZERO);
                    if agora >= pendente {
                        for h in [0u8, 1] {
                            let hab = &nv::HABILIDADES[h as usize];
                            if w.skill_pronta(sid, h, agora)
                                && tentar[h as usize] <= agora
                                && dist <= hab.alcance
                                && (h != 0 || nv::no_costado(p, rumo, b))
                            {
                                w.handle_navio(sid, PedidoNavio::Habilidade { id: h });
                                pendente = agora + 0.4;
                                tentar[h as usize] = agora + 1.5;
                                break;
                            }
                        }
                    }
                }
            }
            n += 1;
            input(&mut w, sid, n);
            // The balls in the air before the tick: the ones gone after it
            // landed, and the target where it is decides hit or splash.
            for (e, b) in w.ecs.query::<&navio::BalaDeCanhao>().iter() {
                if visto.insert(e) {
                    placar.tiros += 1;
                    no_ar.push((e, *b));
                }
            }
            w.step(shared::TICK_DT);
            for (e, b) in w.ecs.query::<&navio::BalaDeCanhao>().iter() {
                if visto.insert(e) {
                    placar.tiros += 1;
                    no_ar.push((e, *b));
                }
            }
            // A Broadside that left no ball spent its cooldown on the water.
            let cd_novo = w.sessions[&sid].skill_cds.get(&nv::id_de_skill(0)).copied().unwrap_or(0.0);
            if cd_novo > cd + 0.5 && cd_novo != bordada_cd {
                bordada_cd = cd_novo;
                if placar.tiros == tiros_antes {
                    placar.bordadas_vazias += 1;
                }
            }
            no_ar.retain(|(e, b)| {
                if w.ecs.contains(*e) {
                    return true;
                }
                if pos_de(&w, b.alvo).is_some_and(|q| nv::acertou(b.mira, q, 0.6)) {
                    placar.acertos += 1;
                }
                false
            });
            for g in w.pending_melee.iter().filter(|g| !g.from_player && g.target == Some(eu)) {
                if swings.insert((g.attacker_eid.0, g.impact_at.to_bits())) {
                    placar.mordidas_anunciadas += 1;
                }
            }
            let Some((_, d)) = w.navio_equipado(sid) else { break };
            if d.casco < casco_antes {
                placar.mordidas += 1;
            }
            casco_antes = d.casco;
            if d.avariado || d.casco == 0 {
                placar.afundou = true;
                break;
            }
            let p2 = w.pos_do_jogador(sid).unwrap();
            placar.longe_max = placar.longe_max.max(p2.distance(centro));
            if w.sim_time_s - marca.1 >= 1.0 {
                if p2.distance(marca.0) >= 0.5 {
                    parado_desde = w.sim_time_s;
                }
                marca = (p2, w.sim_time_s);
            }
            placar.parado_max = placar.parado_max.max(w.sim_time_s - parado_desde);
            if log && w.sim_time_s - log_em >= 1.0 {
                log_em = w.sim_time_s;
                let e = w.sessions[&sid].entity.unwrap();
                let v = w.ecs.get::<&Velocity>(e).map(|v| v.0).unwrap_or(Vec2::ZERO);
                let mobs: Vec<String> = ninhada
                    .iter()
                    .filter(|id| vivo(&w, **id))
                    .filter_map(|id| {
                        let q = pos_de(&w, *id)?;
                        let hp = w.ecs.query::<(&NetId, &Health)>().iter().find(|(_, (n, _))| n.0 == *id).map(|(_, (_, h))| h.current)?;
                        let est = w.ecs.query::<(&NetId, &EnemyTag)>().iter().find(|(_, (n, _))| n.0 == *id).map(|(_, (_, t))| {
                            format!("{}{} ag{:.1} casa{:.0} det{:.0}", if t.returning_home { "V" } else { "" }, if t.provocado_ate > w.sim_time_s { "P" } else { "" }, t.aggro_timer, q.distance(t.spawn_anchor), t.detect_range)
                        }).unwrap_or_default();
                        Some(format!("{}:{:.1}m/{}hp[{}]", id.0, q.distance(p2), hp, est))
                    })
                    .collect();
                eprintln!(
                    "t={:5.1} pos=({:.0},{:.0}) rumo=({:.1},{:.1}) casco={} tiros={} acertos={} mordidas={} | {}",
                    w.sim_time_s - t0, p2.x, p2.y, v.x, v.y, d.casco, placar.tiros, placar.acertos, placar.mordidas, mobs.join(" ")
                );
            }
            if placar.tempo.is_none() && ninhada.iter().all(|id| !vivo(&w, *id)) {
                placar.tempo = Some(w.sim_time_s - t0);
                // Let the last balls land.
                if no_ar.is_empty() {
                    break;
                }
            }
            if placar.tempo.is_some() && no_ar.is_empty() {
                break;
            }
            // Stillness only counts with something to fight.
            if placar.tempo.is_some() {
                parado_desde = w.sim_time_s;
            }
        }
        let fim = w.navio_equipado(sid).map_or(0, |(_, d)| d.casco) as f32;
        placar.casco_perdido = if placar.afundou { 1.0 } else { (cheio - fim) / cheio };
        placar
    }

    fn mar_varredura() -> f32 {
        0.25
    }

    fn resumo(nome: &str, p: &Placar) -> String {
        format!(
            "{nome}: tempo={} casco_perdido={:.0}% tiros={} acertos={} ({:.0}%) bordadas_vazias={} mordidas={}/{} anunciadas afundou={} parado_max={:.1}s longe_max={:.0}m",
            p.tempo.map_or("-".into(), |t| format!("{t:.1}s")),
            p.casco_perdido * 100.0,
            p.tiros,
            p.acertos,
            100.0 * p.acertos as f32 / p.tiros.max(1) as f32,
            p.bordadas_vazias,
            p.mordidas,
            p.mordidas_anunciadas,
            p.afundou,
            p.parado_max,
            p.longe_max,
        )
    }

    /// THE OWNER'S "muito ruim", MEASURED: the same nest of three, the old
    /// client-side auto against the server's pilot. Run with `--nocapture`
    /// to read both scoreboards (and `MMO_PILOTO_LOG=1` for the per-second
    /// trace).
    #[test]
    fn o_piloto_vence_o_auto_antigo_no_mesmo_ninho() {
        let log = std::env::var("MMO_PILOTO_LOG").is_ok();
        let antes = luta(0, Modo::Antes, 120.0, log);
        let depois = luta(0, Modo::Piloto, 120.0, log);
        eprintln!("{}", resumo("ANTES ", &antes));
        eprintln!("{}", resumo("PILOTO", &depois));
        // The elite nest, the old way (the pilot's is `o_piloto_aguenta_o_ninho_do_elite`).
        let elite = luta(1, Modo::Antes, 240.0, false);
        eprintln!("{}", resumo("ANTES ELITE", &elite));
        assert!(depois.parado_max < antes.parado_max);
        assert!(depois.mordidas < antes.mordidas.max(1), "the pilot took as many bites");
        assert!(depois.casco_perdido < antes.casco_perdido);
    }

    /// THE PILOT'S BAR, guarded for EVERY HULL CLASS (turning inertia,
    /// 16/11/2026: a Man-o'-War cannot circle like a Sloop): a same-level
    /// nest of three sunk in 20-60 s, under half the hull lost, more than
    /// 60% of the balls on target, never sunk, never standing still with a
    /// mob alive, never dragged off the nest.
    #[test]
    fn o_piloto_afunda_o_ninho_do_nivel_sem_perder_metade_do_casco() {
        // Every route's first nest (the common one): several levels, sharks
        // and eels.
        let comuns: Vec<usize> = (0..shared::mar::ninhos().len()).step_by(2).take(3).collect();
        for c in nv::CASCOS {
            for &k in &comuns {
                let p = luta_de(k, c.item, Modo::Piloto, 120.0, std::env::var("MMO_PILOTO_LOG").is_ok());
                let nome = format!("{} ninho {k}", c.nome);
                eprintln!("{}", resumo(&nome, &p));
                let t = p.tempo.unwrap_or_else(|| panic!("{nome} not sunk in 120 s: {}", resumo("", &p)));
                assert!((20.0..=60.0).contains(&t), "{nome}: {t:.1} s ({})", resumo("", &p));
                assert!(!p.afundou && p.casco_perdido < 0.5, "{nome}: {}", resumo("", &p));
                assert!(p.acertos as f32 > p.tiros as f32 * 0.6, "{nome}: {}", resumo("", &p));
                assert!(p.parado_max < 3.0, "{nome} stood still: {}", resumo("", &p));
                assert!(p.longe_max < 60.0, "{nome} wandered off: {}", resumo("", &p));
            }
        }
    }

    /// THE ELITE NEST (the route's second: the Dread elite and two more):
    /// longer, but the pilot repairs, does not sink and finishes it.
    #[test]
    fn o_piloto_aguenta_o_ninho_do_elite() {
        let p = luta(1, Modo::Piloto, 240.0, std::env::var("MMO_PILOTO_LOG").is_ok());
        eprintln!("{}", resumo("ELITE ", &p));
        assert!(p.tempo.is_some(), "elite nest not sunk in 240 s: {}", resumo("", &p));
        assert!(!p.afundou, "{}", resumo("", &p));
        assert!(p.acertos as f32 > p.tiros as f32 * 0.6, "{}", resumo("", &p));
    }

    /// LOW HULL: the pilot patches under 35%, and under 20% with the repair
    /// spent it smokes and sails for the harbour corridor, fighting nothing.
    #[test]
    fn com_o_casco_baixo_o_piloto_repara_e_recua() {
        let (mut w, sid, _) = no_ninho(0);
        w.handle_navio(sid, PedidoNavio::Piloto { ligado: true });
        let (id, d) = w.navio_equipado(sid).unwrap();
        let total = nv::casco_total(id, &d);
        w.guarda_navio(sid, shared::items::NavioData { casco: total / 4, ..d });
        w.decidir_piloto(sid, w.sim_time_s);
        let depois = w.navio_equipado(sid).unwrap().1.casco;
        assert!(depois > total / 4, "no Emergency Repair under 35%");
        assert!(!w.sessions[&sid].piloto.recuando);
        // Repair spent, hull at 15%: smoke and home.
        let (_, d) = w.navio_equipado(sid).unwrap();
        w.guarda_navio(sid, shared::items::NavioData { casco: total * 15 / 100, ..d });
        w.decidir_piloto(sid, w.sim_time_s + 0.2);
        let s = &w.sessions[&sid];
        assert!(s.piloto.recuando, "did not retreat at 15% with no repair");
        assert!(s.fumaca_ate > w.sim_time_s, "no Smoke on the way out");
        assert!(!s.rota.vazia(), "no route home");
        let fim = s.rota.destino();
        assert!(shared::mar::no_corredor(fim), "the route home does not end in a harbour corridor: {fim:?}");
        assert!(s.piloto.leme().is_none(), "steering into the fight while retreating");
        assert!(nv::piloto_recua(0.15, false, false) && !nv::piloto_recua(0.15, true, false));
        assert!(nv::piloto_recua(0.4, false, true) && !nv::piloto_recua(0.55, false, true));
    }

    /// RULES: no fighting in the harbour corridor, no other player unless
    /// the PK modes let the cannon hit them, nothing without the switch.
    #[test]
    fn o_piloto_respeita_o_corredor_e_o_pk() {
        let (mut w, sid, _) = no_ninho(0);
        let eu = w.sessions[&sid].entity_id;
        // Another ship 8 m off, both peaceful: the client picked it, the pilot
        // does not fight it.
        let b = SessionId(([127, 0, 0, 1], 47_481).into());
        let (tx, rx) = mpsc::unbounded_channel();
        std::mem::forget(rx);
        w.on_connect(SessionHandle { id: b, to_client: tx });
        let p = w.pos_do_jogador(sid).unwrap();
        let eb = EntityId(4_801);
        let e = w.ecs.spawn((
            NetId(eb),
            Position(p + Vec2::new(8.0, 0.0)),
            Velocity(Vec2::ZERO),
            EntityKind::Player,
            Health { current: 100, max: 100 },
            PlayerTag { name: "Outro".into(), player_id: PlayerId(49), attack_anim_pending: None, combo_step_pending: None },
        ));
        {
            let s = w.sessions.get_mut(&b).unwrap();
            s.logged_in = true;
            s.entity = Some(e);
            s.entity_id = eb;
            s.name = "Outro".into();
            s.inventory = vec![shared::InventorySlot::default(); shared::INVENTORY_SLOTS];
            navio::dar_chalupa(s);
        }
        for sid in [sid, b] {
            let s = w.sessions.get_mut(&sid).unwrap();
            s.pk_modo = shared::pk::PACIFICO;
            s.pk_escolhido = shared::pk::PACIFICO;
        }
        // Switched off: nothing.
        w.sessions.get_mut(&sid).unwrap().target = Some(eb);
        w.tick_piloto();
        assert!(w.sessions[&sid].piloto.leme().is_none());
        w.handle_navio(sid, PedidoNavio::Piloto { ligado: true });
        w.decidir_piloto(sid, w.sim_time_s);
        assert!(w.sessions[&sid].skill_cds.is_empty(), "the pilot fired at a peaceful ship");
        assert_ne!(w.sessions[&sid].piloto.alvo, Some(eb));
        assert!(w.can_damage_player(eu, eb) == false);
        // In the harbour corridor with a mob right there: no shot.
        let cais = navio::fundeadouro_padrao();
        let e_eu = w.sessions[&sid].entity.unwrap();
        w.ecs.get::<&mut Position>(e_eu).unwrap().0 = cais;
        w.place_enemy(cais + Vec2::new(6.0, 0.0), navio::TUBARAO, 0.0);
        for (_, t) in w.ecs.query_mut::<&mut EnemyTag>() {
            t.spawn_grace_until = 0.0;
        }
        w.sessions.get_mut(&sid).unwrap().target = None;
        w.decidir_piloto(sid, w.sim_time_s + 1.0);
        assert!(w.sessions[&sid].skill_cds.is_empty(), "the pilot fired in the harbour corridor");
        // Off again: hands off the helm.
        w.handle_navio(sid, PedidoNavio::Piloto { ligado: false });
        assert!(w.sessions[&sid].piloto.leme().is_none());
    }
}
