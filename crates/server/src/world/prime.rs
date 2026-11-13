//! The PRIME MERCHANT in the world (`shared::prime`, `crate::prime`), and
//! the daily cap of free XP potions that shares its per-character counter.
//!
//! Everything is decided here: the price is read from the live market table
//! at the moment of purchase, the daily limit from `Session::diario`, and
//! the gold is destroyed (telemetry `ouro_ralo` / "prime").

use super::*;
use shared::prime::{AvisoPrime, PedidoPrime};

impl GameWorld {
    /// The free XP potions already paid today (client: reward display).
    pub(super) fn envia_pocoes_xp_do_dia(s: &Session, ultima_paga: bool) {
        let agora = (now_ms() / 1000) as i64;
        let dia = shared::dungeon::dia(agora);
        let _ = s.handle.to_client.send(ServerMessage::PocoesXpDoDia {
            dia,
            pagas: s.diario.pocoes_xp_hoje(dia),
            ultima_paga,
        });
    }

    /// After a daily that pays an XP potion was handed in: the counter, and
    /// the notice when the cap withheld it.
    pub(super) fn avisa_pocoes_xp(s: &Session, sem_pocao: bool) {
        Self::envia_pocoes_xp_do_dia(s, !sem_pocao);
        if sem_pocao {
            let n = shared::quests::POCOES_XP_GRATIS_POR_DIA;
            let _ = s.handle.to_client.send(ServerMessage::Chat {
                from: "SYS".into(),
                text: format!("Daily XP potion limit reached ({n}/{n})."),
            });
        }
    }

    /// The entity is the Prime Merchant?
    pub(super) fn e_prime(&self, e: hecs::Entity) -> bool {
        self.ecs
            .get::<&NpcDaVilaTag>(e)
            .is_ok_and(|t| shared::npc_papel_de_kind(t.rumo) == shared::construcao::Papel::Prime as u8)
    }

    /// Within reach of a Prime Merchant.
    fn perto_do_prime(&self, sid: SessionId) -> bool {
        const PERTO: f32 = 8.0;
        let Some(eu) = self.pos_do_jogador(sid) else {
            return false;
        };
        let prime = shared::construcao::Papel::Prime as u8;
        self.ecs
            .query::<(&Position, &NpcDaVilaTag)>()
            .iter()
            .any(|(_, (p, t))| shared::npc_papel_de_kind(t.rumo) == prime && p.0.distance(eu) <= PERTO)
    }

    pub(super) fn handle_prime(&mut self, sid: SessionId, pedido: PedidoPrime) {
        if !self.perto_do_prime(sid) {
            self.avisa_prime(sid, "Talk to the Prime Merchant in the town square.".into());
            return;
        }
        match pedido {
            PedidoPrime::Abrir => self.abrir_prime(sid),
            PedidoPrime::Comprar { item_id, qtd, preco_visto } => {
                let preco = crate::prime::preco_atual(item_id);
                self.comprar_no_prime(sid, item_id, qtd, preco, preco_visto);
            }
        }
    }

    fn avisa_prime(&self, sid: SessionId, texto: String) {
        if let Some(s) = self.sessions.get(&sid) {
            let _ = s.handle.to_client.send(ServerMessage::Prime { aviso: AvisoPrime::Recusa(texto) });
        }
    }

    pub(super) fn abrir_prime(&self, sid: SessionId) {
        let Some(s) = self.sessions.get(&sid).filter(|s| s.logged_in) else {
            return;
        };
        let agora = (now_ms() / 1000) as i64;
        let _ = s.handle.to_client.send(ServerMessage::Prime {
            aviso: AvisoPrime::Quadro(crate::prime::quadro(&s.diario, agora)),
        });
    }

    /// The purchase itself, with the price already read from the table
    /// (split out so tests can pass a price).
    pub(super) fn comprar_no_prime(
        &mut self,
        sid: SessionId,
        item_id: u16,
        qtd: u32,
        preco_atual: Option<u64>,
        preco_visto: u64,
    ) {
        let agora = (now_ms() / 1000) as i64;
        let dia = shared::dungeon::dia(agora);
        let Some(s) = self.sessions.get_mut(&sid).filter(|s| s.logged_in) else {
            return;
        };
        let compra = match shared::prime::validar(&s.diario, dia, s.gold, item_id, qtd, preco_atual, preco_visto) {
            Ok(c) => c,
            Err(r) => {
                let texto = r.texto();
                let _ = s.handle.to_client.send(ServerMessage::Prime { aviso: AvisoPrime::Recusa(texto) });
                // A stale price: the fresh board goes with the refusal.
                if matches!(r, shared::prime::Recusa::PrecoMudou { .. } | shared::prime::Recusa::SemPreco) {
                    self.abrir_prime(sid);
                }
                return;
            }
        };
        if shared::prime::e_energia(item_id) {
            s.skill_progress.energia = s
                .skill_progress
                .energia
                .saturating_add(qtd as u64 * shared::mercado::ENERGIA_POR_LOTE);
            s.skills_dirty = true;
            let _ = s.handle.to_client.send(ServerMessage::ProgressoDeSkills {
                progresso: s.skill_progress.clone(),
            });
        } else {
            // Room first: nothing is charged for what would not fit.
            let cabe = shared::armazem::e_moeda(item_id)
                || crate::coleta::cabe_tudo(&s.inventory, &[(item_id, qtd)], &crate::economy::item_stack_max);
            if !cabe {
                let _ = s.handle.to_client.send(ServerMessage::Prime {
                    aviso: AvisoPrime::Recusa("Not enough room in the bag.".into()),
                });
                return;
            }
            add_to_inventory(&mut s.inventory, item_id, qtd, None);
            s.inventory_dirty = true;
        }
        // THE SINK: the gold is destroyed, nobody receives it.
        s.gold -= compra.total;
        s.diario.registrar_compra(dia, item_id, qtd);
        crate::telemetria::conta_de(&s.name, "ouro_ralo", "prime", compra.total as i64);
        crate::telemetria::conta_de(&s.name, "prime_compra", item_id, qtd as i64);
        let _ = s.handle.to_client.send(ServerMessage::Prime {
            aviso: AvisoPrime::Comprou { item_id, qtd, total: compra.total },
        });
        self.save_pending = true;
        self.abrir_prime(sid);
    }
}

#[cfg(test)]
mod testes {
    use super::*;
    use std::collections::HashMap;
    use tokio::sync::mpsc;

    fn mundo() -> (GameWorld, SessionId, mpsc::UnboundedReceiver<ServerMessage>) {
        crate::economy::init_vazia_para_testes();
        let mut w = GameWorld::new(HashMap::new());
        w.zona = "ilha_inicial".into();
        let sid = SessionId(([127, 0, 0, 1], 19_970).into());
        let (tx, rx) = mpsc::unbounded_channel::<ServerMessage>();
        w.on_connect(SessionHandle { id: sid, to_client: tx });
        let e = w.ecs.spawn((
            NetId(EntityId(911)),
            Position(Vec2::ZERO),
            Velocity(Vec2::ZERO),
            EntityKind::Player,
            Health { current: 100, max: 100 },
        ));
        w.ecs.spawn((
            NetId(EntityId(912)),
            Position(Vec2::new(2.0, 0.0)),
            EntityKind::Npc(9),
            NpcDaVilaTag {
                nome: shared::construcao::Papel::Prime.nome().into(),
                rumo: shared::npc_kind(None, shared::construcao::Papel::Prime as u8),
                giver: None,
            },
        ));
        let s = w.sessions.get_mut(&sid).unwrap();
        s.logged_in = true;
        s.name = "comprador".into();
        s.entity = Some(e);
        s.entity_id = EntityId(911);
        s.inventory = vec![shared::InventorySlot::default(); shared::INVENTORY_SLOTS];
        (w, sid, rx)
    }

    fn qtd_na_bolsa(w: &GameWorld, sid: SessionId, id: u16) -> u32 {
        w.sessions[&sid].inventory.iter().filter(|s| s.item_id == id).map(|s| s.qty).sum()
    }

    /// The gold leaves the purse (nobody gets it), the item arrives, the
    /// daily limit counts and stops the next purchase.
    #[test]
    fn compra_desconta_ouro_entrega_e_respeita_o_limite() {
        let (mut w, sid, _rx) = mundo();
        let xp = shared::item_id::XP_POTION;
        w.sessions.get_mut(&sid).unwrap().gold = 1_000;
        w.comprar_no_prime(sid, xp, 3, Some(115), 115);
        assert_eq!(w.sessions[&sid].gold, 1_000 - 345);
        assert_eq!(qtd_na_bolsa(&w, sid, xp), 3);
        // 5 a day: 2 more fit, a 3rd does not.
        w.comprar_no_prime(sid, xp, 3, Some(115), 115);
        assert_eq!(w.sessions[&sid].gold, 655, "refused over the limit: nothing charged");
        w.comprar_no_prime(sid, xp, 2, Some(115), 115);
        assert_eq!(w.sessions[&sid].gold, 655 - 230);
        assert_eq!(qtd_na_bolsa(&w, sid, xp), 5);
        w.comprar_no_prime(sid, xp, 1, Some(115), 115);
        assert_eq!(qtd_na_bolsa(&w, sid, xp), 5);
        // Not enough gold / price went up: nothing changes.
        let ds = shared::item_id::DARKSTEEL;
        w.comprar_no_prime(sid, ds, 100, Some(10), 10);
        assert_eq!(w.sessions[&sid].gold, 425);
        w.comprar_no_prime(sid, ds, 10, Some(12), 10);
        assert_eq!(w.sessions[&sid].gold, 425);
        w.comprar_no_prime(sid, ds, 10, Some(10), 10);
        assert_eq!(w.sessions[&sid].gold, 325);
    }

    /// Energy lots land in the Energy, not in the bag.
    #[test]
    fn energia_vai_para_a_energia() {
        let (mut w, sid, _rx) = mundo();
        w.sessions.get_mut(&sid).unwrap().gold = 100;
        let antes = w.sessions[&sid].skill_progress.energia;
        w.comprar_no_prime(sid, shared::item_id::ENERGIA_MIL, 2, Some(20), 20);
        assert_eq!(w.sessions[&sid].gold, 60);
        assert_eq!(w.sessions[&sid].skill_progress.energia, antes + 2 * shared::mercado::ENERGIA_POR_LOTE);
        assert_eq!(qtd_na_bolsa(&w, sid, shared::item_id::ENERGIA_MIL), 0);
    }

    /// The real handler: near the NPC, price from the live table x 1.15.
    #[test]
    fn handler_cobra_o_preco_da_tabela_com_markup() {
        let (mut w, sid, mut rx) = mundo();
        crate::precos::definir_para_teste(shared::precos::TabelaDePrecos {
            precos: vec![(shared::item_id::DARKSTEEL, 10.0)],
        });
        w.sessions.get_mut(&sid).unwrap().gold = 1_000;
        w.handle_prime(sid, PedidoPrime::Abrir);
        let quadro = std::iter::from_fn(|| rx.try_recv().ok()).find_map(|m| match m {
            ServerMessage::Prime { aviso: AvisoPrime::Quadro(q) } => Some(q),
            _ => None,
        });
        let linha = quadro.unwrap().linhas.into_iter().find(|l| l.item_id == shared::item_id::DARKSTEEL).unwrap();
        assert_eq!(linha.preco, 12, "10 x 1.15 = 11.5 -> 12");
        w.handle_prime(sid, PedidoPrime::Comprar { item_id: shared::item_id::DARKSTEEL, qtd: 10, preco_visto: 12 });
        assert_eq!(w.sessions[&sid].gold, 880);
        assert_eq!(w.sessions[&sid].diario.comprado(shared::dungeon::dia((now_ms() / 1000) as i64), shared::item_id::DARKSTEEL), 10);
        // Far from the merchant: refused.
        let e = w.sessions[&sid].entity.unwrap();
        w.ecs.get::<&mut Position>(e).unwrap().0 = Vec2::new(500.0, 500.0);
        w.handle_prime(sid, PedidoPrime::Comprar { item_id: shared::item_id::DARKSTEEL, qtd: 10, preco_visto: 12 });
        assert_eq!(w.sessions[&sid].gold, 880);
    }
}
