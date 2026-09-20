//! O pet coletor no mundo (docs/PETS.md).
//!
//! Equipou o pet no slot `EquipSlot::Pet`, ele NASCE como entidade e passa a
//! andar sozinho: orbita o dono, vai ate' o saque caido no chao e credita no
//! dono ao encostar. Quem decide tudo e' este lado — o cliente so' desenha o
//! bicho andando.
//!
//! Tres travas que existem por motivo, nao por gosto:
//!
//! - O pet busca a partir do DONO, num raio que o grau manda
//!   (`pets::raio_de_busca`). Buscar a partir do PET encadearia: cada saque
//!   pego empurraria o raio mais pra longe e o bicho nunca voltaria.
//! - Nada passa da `COLEIRA`. O raio maximo de busca ja' e' menor que o
//!   `AOI_RADIUS`: fora da AOI o pet sumiria da tela de quem esta' olhando.
//! - Bolsa cheia nao vira laco. O saque que nao coube entra na lista de
//!   desistencia por `DESISTENCIA_S` — sem isso o pet ficaria batendo no
//!   mesmo saquinho pra sempre.

use super::*;

/// O pet de um jogador, no mundo.
pub struct PetTag {
    pub dono: SessionId,
    /// item_id do pet: especie e grau saem dele.
    pub item_id: u16,
    /// Saque que ele esta' buscando agora.
    pub alvo: Option<EntityId>,
    /// (saque, ate' quando ignorar) — bolsa cheia na ultima tentativa.
    pub desistencias: Vec<(EntityId, f32)>,
}

impl GameWorld {
    /// Acerta quem tem pet no mundo com quem tem pet equipado. Roda todo tick:
    /// equipar, desequipar, trocar, morrer e deslogar passam por aqui sem
    /// precisar de gancho em cada um desses caminhos.
    pub(super) fn sincroniza_pets(&mut self) {
        // (sid, item_id desejado) de quem deveria ter pet.
        let desejado: Vec<(SessionId, Option<u16>, u32)> = self
            .sessions
            .values()
            .map(|s| {
                let quer = if s.logged_in && s.entity.is_some() {
                    s.equipment.pet
                } else {
                    None
                };
                (s.handle.id, quer, s.instancia)
            })
            .collect();

        // O que existe hoje.
        let existe: Vec<(Entity, EntityId, SessionId, u16)> = self
            .ecs
            .query::<(&NetId, &PetTag)>()
            .iter()
            .map(|(e, (net, p))| (e, net.0, p.dono, p.item_id))
            .collect();

        for (sid, quer, instancia) in desejado {
            let atual = existe.iter().find(|(_, _, d, _)| *d == sid);
            match (quer, atual) {
                (Some(id), Some((_, _, _, tem))) if *tem == id => {}
                (None, None) => {}
                _ => {
                    if let Some((e, eid, _, _)) = atual {
                        let _ = self.ecs.despawn(*e);
                        self.removed_this_tick.push(*eid);
                    }
                    if let Some(id) = quer {
                        self.nasce_pet(sid, id, instancia);
                    }
                }
            }
        }

        // Pet orfao (dono saiu sem passar pelo laco acima).
        let vivos: Vec<SessionId> = self.sessions.keys().copied().collect();
        let orfaos: Vec<(Entity, EntityId)> = self
            .ecs
            .query::<(&NetId, &PetTag)>()
            .iter()
            .filter(|(_, (_, p))| !vivos.contains(&p.dono))
            .map(|(e, (net, _))| (e, net.0))
            .collect();
        for (e, eid) in orfaos {
            let _ = self.ecs.despawn(e);
            self.removed_this_tick.push(eid);
        }
    }

    fn nasce_pet(&mut self, sid: SessionId, item_id: u16, instancia: u32) {
        let Some(pos) = self
            .sessions
            .get(&sid)
            .and_then(|s| s.entity)
            .and_then(|e| self.ecs.get::<&Position>(e).ok().map(|p| p.0))
        else {
            return;
        };
        let eid = self.alloc_entity_id();
        let pet = self.ecs.spawn((
            NetId(eid),
            Position(pos),
            Velocity(Vec2::ZERO),
            EntityKind::Pet(item_id),
            PetTag {
                dono: sid,
                item_id,
                alvo: None,
                desistencias: Vec::new(),
            },
        ));
        if instancia != 0 {
            let _ = self.ecs.insert_one(pet, dungeon::Instancia(instancia));
        }
    }

    /// Anda os pets e entrega o que eles encostarem. Roda depois do movimento
    /// dos jogadores, pra o pet seguir a posicao DESTE tick.
    pub(super) fn tick_pets(&mut self, dt: f32) {
        let agora = self.sim_time_s;

        // Onde esta' cada dono, em que instancia, e qual o corpo dele — e'
        // pelo corpo que a janela de prioridade do saque e' conferida.
        let donos: HashMap<SessionId, (Vec2, u32, EntityId)> = self
            .sessions
            .values()
            .filter_map(|s| {
                let e = s.entity?;
                let pos = self.ecs.get::<&Position>(e).ok()?.0;
                Some((s.handle.id, (pos, s.instancia, s.entity_id)))
            })
            .collect();
        if donos.is_empty() {
            return;
        }

        // O estado do pet de cada dono: nivel e skills mexem no raio e na
        // velocidade, e eles moram na instancia do item equipado.
        let dados_de: HashMap<SessionId, shared::items::PetData> = self
            .sessions
            .values()
            .map(|s| {
                (
                    s.handle.id,
                    shared::pets::dados(s.equipment.pet_inst.as_ref()),
                )
            })
            .collect();

        // Saque disponivel, por instancia.
        let saques: Vec<(Entity, EntityId, Vec2, LootTag, u32)> = self
            .ecs
            .query::<(&NetId, &Position, &LootTag, Option<&dungeon::Instancia>)>()
            .iter()
            .map(|(e, (net, pos, l, i))| (e, net.0, pos.0, *l, i.map_or(0, |i| i.0)))
            .collect();

        // (pet, dono, novo alvo, nova pos, nova vel, saque pego)
        struct Passo {
            pet: Entity,
            alvo: Option<EntityId>,
            pos: Vec2,
            vel: Vec2,
            pega: Option<(Entity, EntityId, SessionId, LootTag)>,
        }
        let mut passos: Vec<Passo> = Vec::new();
        // Um saque so' pode ser prometido a um pet por tick.
        let mut prometidos: Vec<EntityId> = Vec::new();

        for (pet, (pos, tag)) in self.ecs.query::<(&Position, &PetTag)>().iter() {
            let Some((dono_pos, instancia, dono_eid)) = donos.get(&tag.dono).copied() else {
                continue;
            };
            let Some((_, grau)) = shared::pets::de_item(tag.item_id) else {
                continue;
            };
            let d = dados_de.get(&tag.dono).copied().unwrap_or_default();
            let raio = shared::pets::raio_com(grau, &d);
            let velocidade = shared::pets::velocidade_com(grau, &d) * shared::PLAYER_SPEED;

            // O alvo de antes ainda vale?
            let alvo = tag
                .alvo
                .filter(|id| {
                    saques.iter().any(|(_, sid, spos, l, inst)| {
                        sid == id
                            && *inst == instancia
                            && dono_pos.distance(*spos) <= raio
                            && !prometidos.contains(id)
                            && l.liberado_para(dono_eid, agora)
                    })
                })
                .or_else(|| {
                    // Senao, o mais perto do DONO dentro do raio do grau.
                    saques
                        .iter()
                        .filter(|(_, sid, spos, l, inst)| {
                            *inst == instancia
                                && agora - l.spawn_at >= shared::LOOT_PICKUP_DELAY_S
                                // O pet do matador entra na janela; o dos
                                // outros espera ela passar (docs/PETS.md).
                                && l.liberado_para(dono_eid, agora)
                                && dono_pos.distance(*spos) <= raio
                                && !prometidos.contains(sid)
                                && !tag
                                    .desistencias
                                    .iter()
                                    .any(|(d, ate)| d == sid && *ate > agora)
                        })
                        .min_by(|a, b| {
                            let da = dono_pos.distance_squared(a.2);
                            let db = dono_pos.distance_squared(b.2);
                            da.total_cmp(&db)
                        })
                        .map(|(_, sid, _, _, _)| *sid)
                });

            let destino = alvo
                .and_then(|id| saques.iter().find(|(_, s, _, _, _)| *s == id))
                .map(|(_, _, spos, _, _)| *spos);

            // Longe demais do dono: larga tudo e volta.
            let fora_da_coleira = pos.0.distance(dono_pos) > shared::pets::COLEIRA;
            let (alvo, destino) = if fora_da_coleira {
                (None, None)
            } else {
                (alvo, destino)
            };
            if let Some(id) = alvo {
                prometidos.push(id);
            }

            let mira = destino.unwrap_or_else(|| {
                // Sem o que fazer: orbita o dono, sem colar nele.
                let d = pos.0 - dono_pos;
                if d.length() <= shared::pets::DISTANCIA_DE_SEGUIR {
                    pos.0
                } else {
                    dono_pos + d.normalize_or_zero() * shared::pets::DISTANCIA_DE_SEGUIR
                }
            });

            let delta = mira - pos.0;
            let passo = velocidade * dt;
            let (nova_pos, vel) = if delta.length() <= passo {
                (mira, Vec2::ZERO)
            } else {
                let dir = delta.normalize_or_zero();
                (pos.0 + dir * passo, dir * velocidade)
            };

            // Encostou no saque?
            let pega = alvo
                .filter(|_| {
                    destino.is_some_and(|d| nova_pos.distance(d) <= shared::pets::ALCANCE_DA_COLETA)
                })
                .and_then(|id| {
                    saques
                        .iter()
                        .find(|(_, s, _, _, _)| *s == id)
                        .map(|(e, eid, _, l, _)| (*e, *eid, tag.dono, *l))
                });

            passos.push(Passo {
                pet,
                alvo,
                pos: nova_pos,
                vel,
                pega,
            });
        }

        let mut pegos: Vec<(Entity, EntityId)> = Vec::new();
        let mut hp_max: Vec<(Entity, i32)> = Vec::new();
        let mut cheias: Vec<(Entity, EntityId)> = Vec::new();
        for p in passos {
            if let Ok(mut pos) = self.ecs.get::<&mut Position>(p.pet) {
                pos.0 = p.pos;
            }
            if let Ok(mut vel) = self.ecs.get::<&mut Velocity>(p.pet) {
                vel.0 = p.vel;
            }
            let mut alvo = p.alvo;
            if let Some((se, seid, dono, ltag)) = p.pega {
                match self.creditar_saque(dono, &ltag) {
                    Some(hp) => {
                        hp_max.extend(hp);
                        pegos.push((se, seid));
                        crate::telemetria::conta("pet_coletou", ltag.item_id, ltag.qty as i64);
                        alvo = None;
                    }
                    // Bolsa cheia: larga este e nao insiste.
                    None => {
                        cheias.push((p.pet, seid));
                        alvo = None;
                    }
                }
            }
            if let Ok(mut tag) = self.ecs.get::<&mut PetTag>(p.pet) {
                tag.alvo = alvo;
                tag.desistencias.retain(|(_, ate)| *ate > agora);
            }
        }
        for (pet, saque) in cheias {
            if let Ok(mut tag) = self.ecs.get::<&mut PetTag>(pet) {
                tag.desistencias
                    .push((saque, agora + shared::pets::DESISTENCIA_S));
            }
        }
        for (pe, novo) in hp_max {
            if let Ok(mut h) = self.ecs.get::<&mut Health>(pe) {
                h.max = novo;
            }
        }
        for (e, eid) in pegos {
            let _ = self.ecs.despawn(e);
            self.removed_this_tick.push(eid);
        }
    }
}

#[cfg(test)]
mod testes {
    use super::*;
    use tokio::sync::mpsc;

    fn mundo() -> (GameWorld, SessionId) {
        crate::economy::init_vazia_para_testes();
        let mut w = GameWorld::new(HashMap::new());
        let sid = SessionId(([127, 0, 0, 1], 19_950).into());
        let (tx, _rx) = mpsc::unbounded_channel();
        w.on_connect(SessionHandle {
            id: sid,
            to_client: tx,
        });
        let e = w.ecs.spawn((
            NetId(EntityId(900)),
            Position(Vec2::ZERO),
            Velocity(Vec2::ZERO),
            EntityKind::Player,
            Health {
                current: 100,
                max: 100,
            },
        ));
        let s = w.sessions.get_mut(&sid).unwrap();
        s.logged_in = true;
        s.name = "dono".into();
        s.entity = Some(e);
        s.entity_id = EntityId(900);
        s.inventory = vec![shared::InventorySlot::default(); shared::INVENTORY_SLOTS];
        (w, sid)
    }

    fn poe_saque(w: &mut GameWorld, pos: Vec2, item_id: u16, qty: u32) -> EntityId {
        poe_saque_de(w, pos, item_id, qty, None, -100.0)
    }

    /// Saque com dono e instante de queda — pra exercitar a janela de
    /// prioridade de quem matou.
    fn poe_saque_de(
        w: &mut GameWorld,
        pos: Vec2,
        item_id: u16,
        qty: u32,
        dono: Option<EntityId>,
        spawn_at: f32,
    ) -> EntityId {
        let eid = w.alloc_entity_id();
        w.ecs.spawn((
            NetId(eid),
            Position(pos),
            Velocity(Vec2::ZERO),
            EntityKind::Loot(item_id),
            LootTag {
                item_id,
                qty,
                instance: None,
                spawn_at,
                dono,
            },
        ));
        eid
    }

    fn pet_de(w: &GameWorld) -> Option<(EntityId, Vec2)> {
        w.ecs
            .query::<(&NetId, &Position, &PetTag)>()
            .iter()
            .map(|(_, (net, p, _))| (net.0, p.0))
            .next()
    }

    #[test]
    fn equipar_faz_nascer_e_desequipar_faz_sumir() {
        let (mut w, sid) = mundo();
        w.sincroniza_pets();
        assert!(pet_de(&w).is_none(), "sem pet equipado nao nasce nada");

        let id = shared::item_id::pet_no_grau(shared::item_id::PET_LOBO, 3);
        w.sessions.get_mut(&sid).unwrap().equipment.pet = Some(id);
        w.sincroniza_pets();
        assert!(pet_de(&w).is_some());
        // Sincronizar de novo nao duplica.
        w.sincroniza_pets();
        assert_eq!(w.ecs.query::<&PetTag>().iter().count(), 1);

        w.sessions.get_mut(&sid).unwrap().equipment.pet = None;
        w.sincroniza_pets();
        assert!(pet_de(&w).is_none(), "desequipou, o pet some do mundo");
    }

    #[test]
    fn o_pet_anda_ate_o_saque_e_credita_no_dono() {
        let (mut w, sid) = mundo();
        let id = shared::item_id::pet_no_grau(shared::item_id::PET_LOBO, 5);
        w.sessions.get_mut(&sid).unwrap().equipment.pet = Some(id);
        w.sincroniza_pets();
        let saque = poe_saque(&mut w, Vec2::new(6.0, 0.0), shared::item_id::COPPER, 40);

        // 6 tiles a 1,5 x 5,0 = 7,5 tiles/s: uns 0,8 s.
        for _ in 0..40 {
            w.tick_pets(1.0 / 30.0);
            let sumiu = !w
                .ecs
                .query::<&NetId>()
                .iter()
                .any(|(_, net)| net.0 == saque);
            if sumiu {
                break;
            }
        }
        let s = &w.sessions[&sid];
        assert_eq!(
            crate::craft::tem(&s.inventory, shared::item_id::COPPER),
            40,
            "o cobre tinha que ter entrado na bolsa do dono"
        );
        let (_, pos) = pet_de(&w).expect("o pet continua no mundo");
        assert!(pos.x > 5.0, "o pet foi mesmo ate' o saque: {pos:?}");
    }

    #[test]
    fn ouro_vai_pro_saldo_e_o_saque_longe_demais_fica_onde_esta() {
        let (mut w, sid) = mundo();
        let id = shared::item_id::pet_no_grau(shared::item_id::PET_LOBO, 1);
        w.sessions.get_mut(&sid).unwrap().equipment.pet = Some(id);
        w.sincroniza_pets();
        poe_saque(&mut w, Vec2::new(3.0, 0.0), shared::item_id::GOLD, 250);
        // Fora do raio do cinza (8 tiles).
        poe_saque(&mut w, Vec2::new(30.0, 0.0), shared::item_id::COPPER, 999);

        for _ in 0..120 {
            w.tick_pets(1.0 / 30.0);
        }
        let s = &w.sessions[&sid];
        assert_eq!(s.gold, 250, "ouro vai pro saldo, nao pra bolsa");
        assert_eq!(
            crate::craft::tem(&s.inventory, shared::item_id::COPPER),
            0,
            "o saque fora do raio do grau nao pode ser tocado"
        );
        let (_, pos) = pet_de(&w).expect("o pet continua no mundo");
        assert!(
            pos.distance(Vec2::ZERO) <= shared::pets::COLEIRA,
            "o pet nao pode passar da coleira: {pos:?}"
        );
    }

    fn poe_na_bolsa(w: &mut GameWorld, sid: SessionId, item_id: u16, qty: u32) {
        let s = w.sessions.get_mut(&sid).unwrap();
        add_to_inventory(&mut s.inventory, item_id, qty, None);
    }

    fn pet_data(w: &GameWorld, sid: SessionId) -> shared::items::PetData {
        shared::pets::dados(w.sessions[&sid].equipment.pet_inst.as_ref())
    }

    /// Com fome nao entra XP nenhuma; alimentado, entra a fatia. E' a Ração
    /// que liga a torneira (docs/PETS.md).
    #[test]
    fn so_pet_alimentado_recebe_experiencia() {
        let (mut w, sid) = mundo();
        let id = shared::item_id::pet_no_grau(shared::item_id::PET_TIGRE, 2);
        w.sessions.get_mut(&sid).unwrap().equipment.pet = Some(id);

        w.sessions.get_mut(&sid).unwrap().somar_xp(1_000);
        assert_eq!(pet_data(&w, sid).xp, 0, "com fome nao entra nada");

        poe_na_bolsa(&mut w, sid, shared::item_id::RACAO_DE_PET, 1);
        w.handle_item_de_pet(sid, 0, shared::item_id::RACAO_DE_PET);
        assert!(
            pet_data(&w, sid).alimentado_ate > 0,
            "a racao tem que alimentar"
        );
        assert_eq!(
            crate::craft::tem(&w.sessions[&sid].inventory, shared::item_id::RACAO_DE_PET),
            0,
            "a racao e' consumida"
        );

        w.sessions.get_mut(&sid).unwrap().somar_xp(1_000);
        let d = pet_data(&w, sid);
        assert_eq!(
            d.xp,
            (1_000.0 * shared::pets::FATIA_DA_XP) as u64,
            "alimentado, entra a fatia"
        );
        // E o pet passou a valer mais: o nivel subiu e os pontos com ele.
        assert!(shared::pets::nivel_de_xp(d.xp) > 1);
    }

    /// Skill so' entra em slot que o NIVEL abriu, e o removedor devolve todos.
    #[test]
    fn skill_de_pet_precisa_de_slot_aberto() {
        let (mut w, sid) = mundo();
        let id = shared::item_id::pet_no_grau(shared::item_id::PET_LOBO, 3);
        w.sessions.get_mut(&sid).unwrap().equipment.pet = Some(id);
        poe_na_bolsa(&mut w, sid, shared::item_id::SKILL_PET_FARO, 2);

        // Nivel 1: nao ha' slot.
        w.handle_item_de_pet(sid, 0, shared::item_id::SKILL_PET_FARO);
        assert_eq!(pet_data(&w, sid).skills, [0; 3]);
        assert_eq!(
            crate::craft::tem(&w.sessions[&sid].inventory, shared::item_id::SKILL_PET_FARO),
            2,
            "recusa nao cobra o item"
        );

        // Sobe pro 10 e o primeiro slot abre.
        let mut d = pet_data(&w, sid);
        d.xp = shared::pets::xp_para_nivel(10);
        w.sessions.get_mut(&sid).unwrap().guarda_pet(d);
        w.handle_item_de_pet(sid, 0, shared::item_id::SKILL_PET_FARO);
        assert_eq!(pet_data(&w, sid).skills[0], shared::item_id::SKILL_PET_FARO);
        assert_eq!(
            crate::craft::tem(&w.sessions[&sid].inventory, shared::item_id::SKILL_PET_FARO),
            1
        );

        // A mesma skill de novo e' recusada, e sem cobrar.
        w.handle_item_de_pet(sid, 0, shared::item_id::SKILL_PET_FARO);
        assert_eq!(
            crate::craft::tem(&w.sessions[&sid].inventory, shared::item_id::SKILL_PET_FARO),
            1,
            "skill repetida nao cobra"
        );

        // O removedor limpa tudo.
        poe_na_bolsa(&mut w, sid, shared::item_id::REMOVEDOR_DE_SKILL_PET, 1);
        let slot = w.sessions[&sid]
            .inventory
            .iter()
            .position(|x| x.item_id == shared::item_id::REMOVEDOR_DE_SKILL_PET)
            .unwrap();
        w.handle_item_de_pet(sid, slot, shared::item_id::REMOVEDOR_DE_SKILL_PET);
        assert_eq!(pet_data(&w, sid).skills, [0; 3]);
    }

    /// A regra crua da janela, nas bordas. Vale pro pickup por proximidade e
    /// pro pet — os dois perguntam pra ela.
    #[test]
    fn a_janela_de_prioridade_abre_no_segundo_dois() {
        let eu = EntityId(1);
        let outro = EntityId(2);
        let saque = |dono| LootTag {
            item_id: shared::item_id::COPPER,
            qty: 1,
            instance: None,
            spawn_at: 100.0,
            dono,
        };
        let meu = saque(Some(eu));
        assert!(meu.liberado_para(eu, 100.0), "pro dono, desde o primeiro quadro");
        assert!(!meu.liberado_para(outro, 100.0));
        assert!(
            !meu.liberado_para(outro, 100.0 + shared::LOOT_PRIORIDADE_S - 0.01),
            "ainda dentro da janela"
        );
        assert!(
            meu.liberado_para(outro, 100.0 + shared::LOOT_PRIORIDADE_S),
            "fechada a janela, e' de quem chegar"
        );
        // Saque sem dono (coleta, item largado) nunca teve janela.
        let de_ninguem = saque(None);
        assert!(de_ninguem.liberado_para(outro, 100.0));
    }

    /// Quem matou tem `LOOT_PRIORIDADE_S` de frente. O pet de quem NAO matou
    /// so' encosta no saque depois que a janela fecha — senao pet de raio
    /// grande limpava o drop alheio antes do dono chegar.
    #[test]
    fn o_pet_de_quem_nao_matou_espera_a_janela_de_prioridade() {
        let (mut w, sid) = mundo();
        let id = shared::item_id::pet_no_grau(shared::item_id::PET_LOBO, 5);
        w.sessions.get_mut(&sid).unwrap().equipment.pet = Some(id);
        w.sincroniza_pets();
        // Caiu agora, e quem matou foi OUTRO corpo.
        let alheio = EntityId(777);
        let agora = w.sim_time_s;
        poe_saque_de(
            &mut w,
            Vec2::new(2.0, 0.0),
            shared::item_id::COPPER,
            30,
            Some(alheio),
            agora,
        );
        // Dentro da janela: o pet nao pode encostar.
        for _ in 0..30 {
            w.tick_pets(1.0 / 30.0);
        }
        assert_eq!(
            crate::craft::tem(&w.sessions[&sid].inventory, shared::item_id::COPPER),
            0,
            "dentro da janela o saque e' de quem matou"
        );

        // Passada a janela, e' de quem chegar.
        w.sim_time_s += shared::LOOT_PRIORIDADE_S + 0.1;
        for _ in 0..60 {
            w.tick_pets(1.0 / 30.0);
        }
        assert_eq!(
            crate::craft::tem(&w.sessions[&sid].inventory, shared::item_id::COPPER),
            30,
            "fechada a janela, o pet pega"
        );
    }

    /// O pet de QUEM MATOU entra na janela na hora: e' a vantagem de ter pet.
    #[test]
    fn o_pet_de_quem_matou_pega_dentro_da_janela() {
        let (mut w, sid) = mundo();
        let id = shared::item_id::pet_no_grau(shared::item_id::PET_LOBO, 5);
        w.sessions.get_mut(&sid).unwrap().equipment.pet = Some(id);
        w.sincroniza_pets();
        let meu = w.sessions[&sid].entity_id;
        let caiu = w.sim_time_s - shared::LOOT_PICKUP_DELAY_S;
        poe_saque_de(
            &mut w,
            Vec2::new(3.0, 0.0),
            shared::item_id::COPPER,
            77,
            Some(meu),
            caiu,
        );
        for _ in 0..30 {
            w.tick_pets(1.0 / 30.0);
        }
        assert_eq!(
            crate::craft::tem(&w.sessions[&sid].inventory, shared::item_id::COPPER),
            77,
            "o pet de quem matou nao espera nada"
        );
        assert!(
            w.sim_time_s < shared::LOOT_PRIORIDADE_S,
            "o teste tem que caber dentro da janela"
        );
    }

    #[test]
    fn bolsa_cheia_nao_vira_laco() {
        let (mut w, sid) = mundo();
        let id = shared::item_id::pet_no_grau(shared::item_id::PET_LOBO, 5);
        {
            let s = w.sessions.get_mut(&sid).unwrap();
            s.equipment.pet = Some(id);
            // Enche a bolsa com um item que nao empilha com o do saque.
            s.inventory = vec![
                shared::InventorySlot {
                    item_id: shared::item_id::STEEL,
                    qty: 1,
                    instance: None,
                };
                shared::INVENTORY_SLOTS
            ];
        }
        w.sincroniza_pets();
        let saque = poe_saque(&mut w, Vec2::new(2.0, 0.0), shared::item_id::COPPER, 5);
        for _ in 0..90 {
            w.tick_pets(1.0 / 30.0);
        }
        assert!(
            w.ecs.query::<&LootTag>().iter().count() == 1,
            "o saque que nao coube fica no chao"
        );
        let desistiu = w
            .ecs
            .query::<&PetTag>()
            .iter()
            .any(|(_, p)| p.desistencias.iter().any(|(d, _)| *d == saque));
        assert!(desistiu, "o pet tem que desistir do saque que nao coube");
    }
}
