//! STORMKEEP's chests (`shared::forte`): the Warlord drops one to three where
//! he falls; standing at one for `ABRIR_S` seconds opens it, and whoever
//! finishes takes the loot — a level 56 dungeon chest of the color's stage.
//!
//! The opening runs on the gathering bar (`ColetaEstado`), so the client
//! needs no new screen: moving away, dying or someone else finishing first
//! stops it.

use super::*;
use shared::dungeon as dg;
use shared::forte;

/// A chest on the ground: its color and when it vanishes unopened.
#[derive(Clone, Copy)]
pub struct BauDoForte {
    pub cor: u8,
    pub some_em: f32,
    /// The dungeon whose chest table it rolls (`forte::conteudo_do_chefe`).
    pub conteudo: u16,
}

/// A player opening a chest: which one, since when, and from where.
#[derive(Clone, Copy, Debug)]
pub struct AberturaDeBau {
    pub bau: EntityId,
    pub desde: f32,
    pub de: Vec2,
}

impl GameWorld {
    /// A field boss fell at `onde`: one to three chests round the spot, of
    /// the table of dungeon `conteudo`.
    pub(super) fn forte_largar_baus(&mut self, onde: Vec2, conteudo: u16) {
        let cores: Vec<u8> = (0..forte::quantos(fastrand::f32())).map(|_| forte::cor(fastrand::f32())).collect();
        self.forte_largar_n_baus(onde, conteudo, &cores);
    }

    /// Chests of these colours round `onde`, of the table of dungeon `conteudo`.
    pub(super) fn forte_largar_n_baus(&mut self, onde: Vec2, conteudo: u16, cores: &[u8]) {
        let n = cores.len();
        for (k, &cor) in cores.iter().enumerate() {
            let a = k as f32 / n as f32 * std::f32::consts::TAU + 0.6;
            let p = self.chao_livre(onde + Vec2::new(a.cos(), a.sin()) * 2.5);
            let eid = self.alloc_entity_id();
            self.ecs.spawn((
                NetId(eid),
                Position(p),
                Velocity(Vec2::ZERO),
                EntityKind::Npc(0),
                NpcDaVilaTag {
                    nome: forte::nome_da_cor(cor).to_string(),
                    rumo: shared::npc_kind(None, forte::papel(cor)),
                    giver: None,
                },
                BauDoForte {
                    cor,
                    some_em: self.sim_time_s + forte::DURA_S,
                    conteudo,
                },
            ));
            crate::telemetria::conta("forte_bau", cor, 1);
        }
        tracing::info!("[forte] a field boss fell: {n} chest(s) of dungeon {conteudo}");
    }

    /// A touch on `eid` (`Interact`). `true` = it was a castle chest.
    pub(super) fn forte_tocar_bau(&mut self, sid: SessionId, eid: EntityId) -> bool {
        let Some((e, p)) = self
            .ecs
            .query::<(&NetId, &Position, &BauDoForte)>()
            .iter()
            .find(|(_, (n, _, _))| n.0 == eid)
            .map(|(e, (_, p, _))| (e, p.0))
        else {
            return false;
        };
        let _ = e;
        let Some(eu) = self.pos_do_jogador(sid) else {
            return true;
        };
        if eu.distance(p) > forte::ALCANCE {
            self.avisa_missao(sid, "Get close to the chest.".into());
            return true;
        }
        let agora = self.sim_time_s;
        let cor = self.ecs.query::<(&NetId, &BauDoForte)>().iter().find(|(_, (n, _))| n.0 == eid).map_or(2, |(_, (_, b))| b.cor);
        if let Some(s) = self.sessions.get_mut(&sid) {
            s.coleta_no = None;
            s.abrindo_bau = Some(AberturaDeBau { bau: eid, desde: agora, de: eu });
            let _ = s.handle.to_client.send(ServerMessage::ColetaEstado {
                tipo: forte::TIPO_COLETA + cor,
                intervalo_s: forte::ABRIR_S,
                progresso: 0.0,
                centro: Some([p.x, p.y]),
                pausado: false,
            });
        }
        true
    }

    /// Stops an opening (if any) and clears the bar.
    pub(super) fn forte_parar(&mut self, sid: SessionId) {
        if let Some(s) = self.sessions.get_mut(&sid) {
            if s.abrindo_bau.take().is_some() {
                let _ = s.handle.to_client.send(ServerMessage::ColetaEstado {
                    tipo: shared::protocol::COLETA_PARADA,
                    intervalo_s: 0.0,
                    progresso: 0.0,
                    centro: None,
                    pausado: false,
                });
            }
        }
    }

    /// Every tick: openings that finish or break, and chests that time out.
    pub(super) fn tick_forte(&mut self) {
        let agora = self.sim_time_s;
        let vencidos: Vec<Entity> = self
            .ecs
            .query::<&BauDoForte>()
            .iter()
            .filter(|(_, b)| agora >= b.some_em)
            .map(|(e, _)| e)
            .collect();
        for e in vencidos {
            if let Ok(n) = self.ecs.get::<&NetId>(e).map(|n| n.0) {
                self.removed_this_tick.push(n);
            }
            let _ = self.ecs.despawn(e);
        }
        let abrindo: Vec<(SessionId, AberturaDeBau)> = self
            .sessions
            .iter()
            .filter_map(|(sid, s)| s.abrindo_bau.map(|a| (*sid, a)))
            .collect();
        for (sid, a) in abrindo {
            let bau = self
                .ecs
                .query::<(&NetId, &Position, &BauDoForte)>()
                .iter()
                .find(|(_, (n, _, _))| n.0 == a.bau)
                .map(|(e, (_, p, b))| (e, p.0, b.cor, b.conteudo));
            let eu = self.pos_do_jogador(sid);
            let Some((e, p, cor, conteudo)) = bau else {
                // Someone else finished first, or it timed out.
                self.forte_parar(sid);
                continue;
            };
            let quebrou = eu.is_none_or(|eu| eu.distance(a.de) > 1.0 || eu.distance(p) > forte::ALCANCE);
            if quebrou {
                self.forte_parar(sid);
                continue;
            }
            if agora - a.desde < forte::ABRIR_S {
                continue;
            }
            self.removed_this_tick.push(a.bau);
            let _ = self.ecs.despawn(e);
            self.forte_parar(sid);
            self.forte_dar_bau(sid, cor, conteudo);
        }
    }

    /// Rolls and hands over a chest of `cor`. What does not fit goes to the mail.
    fn forte_dar_bau(&mut self, sid: SessionId, cor: u8, conteudo: u16) {
        let Some(c) = dg::conteudo(conteudo) else {
            return;
        };
        let mut rng = || fastrand::f32();
        let bau = dg::rolar_bau(c, forte::estagio(cor), false, false, &mut rng);
        let mut premios: Vec<(u16, u32, Option<shared::ItemInstance>)> = bau
            .itens
            .iter()
            .map(|p| (p.item_id, p.qtd, GameWorld::dg_rolar_peca(p)))
            .collect();
        if bau.marcas > 0 {
            premios.push((shared::item_id::MARCAS_TEMPESTADE, bau.marcas, None));
        }
        let quando = (now_ms() / 1000) as i64;
        let Some(s) = self.sessions.get_mut(&sid) else {
            return;
        };
        let mut itens = Vec::new();
        for (item, qtd, inst) in premios {
            crate::telemetria::conta("forte_bau_item", item, qtd as i64);
            itens.push((item, qtd));
            if !add_to_inventory(&mut s.inventory, item, qtd, inst) {
                s.dungeon.postar(item, qtd, inst, 0, quando);
            }
        }
        s.inventory_dirty = true;
        self.save_pending = true;
        tracing::info!("[forte] {} opened a {}: {:?} + {} marks", s.name, forte::nome_da_cor(cor), itens, bau.marcas);
        self.dg_texto(sid, true, format!("{} opened.", forte::nome_da_cor(cor)));
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    fn sessao_em(w: &mut GameWorld, porta: u16, p: Vec2) -> SessionId {
        let sid = SessionId(format!("127.0.0.1:{porta}").parse().unwrap());
        let (tx, _) = mpsc::unbounded_channel();
        w.on_connect(SessionHandle { id: sid, to_client: tx });
        let s = w.sessions.get_mut(&sid).unwrap();
        s.logged_in = true;
        let id = s.entity_id;
        let e = w.ecs.spawn((NetId(id), Position(p), Velocity(Vec2::ZERO), EntityKind::Player, Health { current: 100, max: 100 }));
        w.sessions.get_mut(&sid).unwrap().entity = Some(e);
        sid
    }

    fn mover(w: &mut GameWorld, sid: SessionId, p: Vec2) {
        let e = w.sessions[&sid].entity.unwrap();
        w.ecs.get::<&mut Position>(e).unwrap().0 = p;
    }

    /// The Warlord's chests: one to three drop, opening takes ten seconds
    /// of standing still, the first to finish takes it, and walking away
    /// stops the bar.
    #[test]
    fn bau_do_forte_abre_em_dez_segundos_e_so_um_leva() {
        crate::economy::init_vazia_para_testes();
        let mut w = GameWorld::new(HashMap::new());
        w.forte_largar_baus(Vec2::new(10.0, 10.0), forte::CONTEUDO_DO_BAU);
        let baus: Vec<(EntityId, Vec2)> = w
            .ecs
            .query::<(&NetId, &Position, &BauDoForte)>()
            .iter()
            .map(|(_, (n, p, _))| (n.0, p.0))
            .collect();
        assert!((1..=3).contains(&baus.len()));
        let (eid, p) = baus[0];
        let a = sessao_em(&mut w, 45001, p + Vec2::new(1.0, 0.0));
        let b = sessao_em(&mut w, 45002, p + Vec2::new(-1.0, 0.0));
        assert!(w.forte_tocar_bau(a, eid));
        assert!(w.forte_tocar_bau(b, eid));
        w.sim_time_s += forte::ABRIR_S * 0.5;
        w.tick_forte();
        assert!(w.sessions[&a].abrindo_bau.is_some(), "stopped before ten seconds");
        // B walks away: the bar stops.
        mover(&mut w, b, p + Vec2::new(-6.0, 0.0));
        w.tick_forte();
        assert!(w.sessions[&b].abrindo_bau.is_none(), "walking away did not stop it");
        w.sim_time_s += forte::ABRIR_S * 0.5 + 0.1;
        w.tick_forte();
        assert!(w.sessions[&a].abrindo_bau.is_none());
        assert!(
            w.ecs.query::<(&NetId, &BauDoForte)>().iter().all(|(_, (n, _))| n.0 != eid),
            "the opened chest is still there"
        );
        let s = &w.sessions[&a];
        assert!(
            s.inventory.iter().any(|x| x.qty > 0) || !s.dungeon.correio.is_empty(),
            "the opener got nothing"
        );
        let s = &w.sessions[&b];
        assert!(s.inventory.iter().all(|x| x.qty == 0) && s.dungeon.correio.is_empty(), "the one who walked away got loot");
    }
}
