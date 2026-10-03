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
    pub slot: shared::EquipSlot,
    /// item_id do pet: especie e grau saem dele.
    pub item_id: u16,
    pub auras: u64,
    /// Saque que ele esta' buscando agora.
    pub alvo: Option<EntityId>,
    /// (saque, ate' quando ignorar) — bolsa cheia na ultima tentativa.
    pub desistencias: Vec<(EntityId, f32)>,
    /// The field-boss chest (`forte::BauDoForte`) it is opening for its
    /// owner, and since when it has stood at it (`f32::MAX` = on its way).
    pub bau: Option<(EntityId, f32)>,
}

fn posicao_de_seguir(slot: shared::EquipSlot) -> Vec2 {
    match slot {
        shared::EquipSlot::Pet => Vec2::new(-1.8, 1.2),
        shared::EquipSlot::Pet2 => Vec2::new(1.8, 1.2),
        shared::EquipSlot::Pet3 => Vec2::new(0.0, -2.1),
        _ => Vec2::ZERO,
    }
}

impl GameWorld {
    /// Acerta quem tem pet no mundo com quem tem pet equipado. Roda todo tick:
    /// equipar, desequipar, trocar, morrer e deslogar passam por aqui sem
    /// precisar de gancho em cada um desses caminhos.
    pub(super) fn sincroniza_pets(&mut self) {
        // (sid, item_id desejado) de quem deveria ter pet.
        let desejado: Vec<(SessionId, shared::EquipSlot, Option<u16>, u32)> = self
            .sessions
            .values()
            .flat_map(|s| {
                s.equipment.pets().into_iter().map(move |(slot, id, _)| {
                    let quer = if s.logged_in && s.entity.is_some() {
                        id
                    } else {
                        None
                    };
                    (s.handle.id, slot, quer, s.instancia)
                })
            })
            .collect();

        // O que existe hoje, e em que instancia ele nasceu.
        let existe: Vec<(Entity, EntityId, SessionId, shared::EquipSlot, u16, u32)> = self
            .ecs
            .query::<(&NetId, &PetTag, Option<&dungeon::Instancia>)>()
            .iter()
            .map(|(e, (net, p, i))| (e, net.0, p.dono, p.slot, p.item_id, i.map_or(0, |i| i.0)))
            .collect();

        for (sid, slot, quer, instancia) in desejado {
            let atual = existe
                .iter()
                .find(|(_, _, d, sl, _, _)| *d == sid && *sl == slot);
            match (quer, atual) {
                // Trocou de instancia (entrou ou saiu de dungeon): o pet
                // renasce la' dentro. Sem isto ele ficava na instancia velha,
                // invisivel pro dono e sem enxergar saque nenhum.
                (Some(id), Some((_, _, _, _, tem, inst))) if *tem == id && *inst == instancia => {}
                (None, None) => {}
                _ => {
                    if let Some((e, eid, _, _, _, _)) = atual {
                        let _ = self.ecs.despawn(*e);
                        self.removed_this_tick.push(*eid);
                    }
                    if let Some(id) = quer {
                        self.nasce_pet(sid, slot, id, instancia);
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
        // A instância do item pode mudar sem trocar o id (tier/refino).
        // Reenvia a meta para todos que enxergam o pet, inclusive o dono.
        let mut alterados = Vec::new();
        for (_, (net, pet)) in self.ecs.query_mut::<(&NetId, &mut PetTag)>() {
            let aura = self.sessions.get(&pet.dono).and_then(|s|
                s.equipment.pets().into_iter().find(|(slot,_,_)| *slot == pet.slot))
                .map_or(0, |(_,id,inst)| id.map_or(0, |id| shared::auras::pet(id,inst.as_ref())));
            if pet.auras != aura { pet.auras = aura; alterados.push(net.0); }
        }
        for s in self.sessions.values_mut() {
            for id in &alterados { s.last_sent.remove(id); }
        }

    }

    fn nasce_pet(&mut self, sid: SessionId, slot: shared::EquipSlot, item_id: u16, instancia: u32) {
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
            Position(pos + posicao_de_seguir(slot)),
            Velocity(Vec2::ZERO),
            EntityKind::Pet(item_id),
            PetTag {
                dono: sid,
                slot,
                item_id,
                auras: 0,
                alvo: None,
                desistencias: Vec::new(),
                bau: None,
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
        let dados_de: HashMap<(SessionId, shared::EquipSlot), (shared::items::PetData, f32)> = self
            .sessions
            .values()
            .flat_map(|s| {
                s.equipment.pets().into_iter().map(move |(slot, _, inst)| {
                    (
                        (s.handle.id, slot),
                        (
                            shared::pets::dados(inst.as_ref()),
                            shared::pets::mult_coleta(inst.as_ref()),
                        ),
                    )
                })
            })
            .collect();

        // Saque disponivel, por instancia.
        let saques: Vec<(Entity, EntityId, Vec2, LootTag, u32)> = self
            .ecs
            .query::<(&NetId, &Position, &LootTag, Option<&dungeon::Instancia>)>()
            .iter()
            .map(|(e, (net, pos, l, i))| (e, net.0, pos.0, *l, i.map_or(0, |i| i.0)))
            .collect();

        // Field-boss chests on the ground (world only: dungeons keep theirs).
        let baus: Vec<(EntityId, Vec2)> = self
            .ecs
            .query::<(&NetId, &Position, &super::forte::BauDoForte)>()
            .without::<&dungeon::Instancia>()
            .iter()
            .map(|(_, (n, p, _))| (n.0, p.0))
            .collect();

        // (pet, dono, novo alvo, nova pos, nova vel, saque pego)
        struct Passo {
            pet: Entity,
            alvo: Option<EntityId>,
            pos: Vec2,
            vel: Vec2,
            pega: Option<(Entity, EntityId, SessionId, LootTag)>,
            bau: Option<(EntityId, f32)>,
            abre: Option<(EntityId, SessionId)>,
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
            let (d, mult) = dados_de
                .get(&(tag.dono, tag.slot))
                .copied()
                .unwrap_or_default();
            let raio = shared::pets::raio_com(grau, &d);
            let velocidade = shared::pets::velocidade_com(grau, &d) * mult * shared::PLAYER_SPEED;

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

            // Longe demais do dono: larga tudo. E se estiver MUITO longe, o
            // dono teleportou (portal, viagem, pergaminho) — andar de volta
            // levaria o bicho atravessando o mapa, entao ele reaparece do
            // lado. A coleira comum continua sendo caminhada.
            let distancia = pos.0.distance(dono_pos);
            let fora_da_coleira = distancia > shared::pets::COLEIRA;
            let teleportou = distancia > shared::pets::TELEPORTE;
            let (alvo, destino) = if fora_da_coleira {
                (None, None)
            } else {
                (alvo, destino)
            };
            if let Some(id) = alvo {
                prometidos.push(id);
            }
            // AUTO LOOT OPENS CHESTS TOO: with nothing to fetch, the pet goes to
            // the nearest field-boss chest in range and stands at it for
            // `forte::ABRIR_S`, the same wait a player has; whoever finishes
            // first still takes it.
            let bau_alvo = if alvo.is_none() && instancia == 0 && !fora_da_coleira {
                tag.bau
                    .map(|(id, _)| id)
                    .filter(|id| baus.iter().any(|(b, p)| b == id && dono_pos.distance(*p) <= raio))
                    .or_else(|| {
                        baus.iter()
                            .filter(|(b, p)| dono_pos.distance(*p) <= raio && !prometidos.contains(b))
                            .min_by(|a, b| dono_pos.distance_squared(a.1).total_cmp(&dono_pos.distance_squared(b.1)))
                            .map(|(b, _)| *b)
                    })
                    .filter(|b| !prometidos.contains(b))
            } else {
                None
            };
            let bau_pos = bau_alvo.and_then(|b| baus.iter().find(|(id, _)| *id == b).map(|(_, p)| *p));
            if let Some(b) = bau_alvo {
                prometidos.push(b);
            }
            let destino = destino.or(bau_pos);

            let mira = destino.unwrap_or_else(|| dono_pos + posicao_de_seguir(tag.slot));

            let delta = mira - pos.0;
            let passo = velocidade * dt;
            let (nova_pos, vel) = if teleportou {
                (dono_pos + posicao_de_seguir(tag.slot), Vec2::ZERO)
            } else if delta.length() <= passo {
                // CHEGOU dentro do passo: anda o que falta, e reporta a
                // velocidade que ele andou DE VERDADE.
                //
                // Zerar aqui era o defeito do "facing" que o dono viu: indo
                // buscar o saque o pet fica no ramo de baixo e olha certo,
                // mas SEGUINDO O DONO ele cai neste ramo em todo tick — o
                // dono anda um pouco, a mira anda com ele, e a sobra e'
                // sempre menor que um passo. Com `vel` zerada, `rumo::escolhe`
                // ve' 0 u/s (abaixo de `VEL_MINIMA`), nao manda rumo nenhum, e
                // o cliente mantem o angulo velho: o bicho desliza de lado
                // olhando pro lugar onde estava o ultimo saque.
                (mira, delta / dt.max(1e-4))
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

            // At the chest: the clock starts; ten seconds later it opens.
            let bau = bau_alvo.zip(bau_pos).map(|(b, p)| {
                let perto = nova_pos.distance(p) <= super::forte::ALCANCE_DO_PET;
                let desde = match tag.bau {
                    Some((id, d)) if id == b && d != f32::MAX && perto => d,
                    _ if perto => agora,
                    _ => f32::MAX,
                };
                (b, desde)
            });
            let abre = bau
                .filter(|(_, d)| *d != f32::MAX && agora - *d >= shared::forte::ABRIR_S)
                .map(|(b, _)| (b, tag.dono));

            passos.push(Passo {
                pet,
                alvo,
                pos: nova_pos,
                vel,
                pega,
                bau: if abre.is_some() { None } else { bau },
                abre,
            });
        }

        let mut pegos: Vec<(Entity, EntityId)> = Vec::new();
        let mut abertos: Vec<(EntityId, SessionId)> = Vec::new();
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
            if let Some((b, dono)) = p.abre {
                abertos.push((b, dono));
            }
            if let Ok(mut tag) = self.ecs.get::<&mut PetTag>(p.pet) {
                tag.bau = p.bau;
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
        for (bau, dono) in abertos {
            let achado = self
                .ecs
                .query::<(&NetId, &super::forte::BauDoForte)>()
                .iter()
                .find(|(_, (n, _))| n.0 == bau)
                .map(|(e, (_, b))| (e, b.cor, b.conteudo));
            if let Some((e, cor, conteudo)) = achado {
                let _ = self.ecs.despawn(e);
                self.removed_this_tick.push(bau);
                crate::telemetria::conta("pet_abriu_bau", cor, 1);
                self.forte_dar_bau(dono, cor, conteudo);
            }
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

    #[test]
    fn auras_dos_tres_pets_chegam_no_snapshot_e_atualizam_sem_reequipar() {
        let (mut w,sid) = mundo();
        let (tx,mut rx) = mpsc::unbounded_channel();
        w.sessions.get_mut(&sid).unwrap().handle.to_client=tx;
        let e = &mut w.sessions.get_mut(&sid).unwrap().equipment;
        e.pet=Some(shared::item_id::pet_no_grau(shared::item_id::PET_BASE,2));
        e.pet2=Some(shared::item_id::pet_no_grau(shared::item_id::PET_BASE,3));
        e.pet3=Some(shared::item_id::pet_no_grau(shared::item_id::PET_BASE,4));
        // Simula o equipamento que veio do save no login.
        let salvo=serde_json::to_string(e).unwrap();
        *e=serde_json::from_str(&salvo).unwrap();
        w.sincroniza_pets();
        w.send_snapshots();
        let mut pets=Vec::new();
        while let Ok(msg)=rx.try_recv() {
            if let ServerMessage::Snapshot{snapshot}=msg {
                pets.extend(snapshot.entered.into_iter().filter(|m|m.tag==shared::EntityTag::Pet));
            }
        }
        assert_eq!(pets.len(),3);
        for p in &pets { assert_eq!(shared::auras::peca(p.auras,shared::auras::PET),Some((shared::pets::de_item(p.kind).unwrap().1,1,0))); }
        let mut inst=shared::ItemInstance::vazia_de_grau(3); inst.tier=4; inst.refinement=12;
        w.sessions.get_mut(&sid).unwrap().equipment.pet2_inst=Some(inst);
        w.sincroniza_pets(); w.send_snapshots();
        let mut alterados=Vec::new();
        while let Ok(msg)=rx.try_recv() {
            if let ServerMessage::Snapshot{snapshot}=msg {
                alterados.extend(snapshot.entered.into_iter().filter(|m|m.tag==shared::EntityTag::Pet));
            }
        }
        assert_eq!(alterados.len(),1);
        assert_eq!(shared::auras::peca(alterados[0].auras,shared::auras::PET),Some((3,4,10)));
        assert_eq!(w.ecs.query::<&PetTag>().iter().count(),3,"refinar não duplica pets");
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

    /// AUTO LOOT OPENS CHESTS: with a field-boss chest in range and nothing
    /// else to fetch, the pet walks to it, stands there `ABRIR_S` and the
    /// chest's loot lands with the owner — not a moment sooner.
    #[test]
    fn o_pet_abre_o_bau_do_chefe_pro_dono() {
        let (mut w, sid) = mundo();
        let id = shared::item_id::pet_no_grau(shared::item_id::PET_BASE, 3);
        w.sessions.get_mut(&sid).unwrap().equipment.pet = Some(id);
        w.sincroniza_pets();
        w.forte_largar_n_baus(Vec2::new(3.0, 0.0), shared::forte::CONTEUDO_DO_BAU, &[2]);
        let tem_bau = |w: &GameWorld| w.ecs.query::<&super::super::forte::BauDoForte>().iter().count();
        assert_eq!(tem_bau(&w), 1);
        let dt = 1.0 / 30.0;
        let mut aberto_em = None;
        for k in 0..(20.0 / dt) as u32 {
            w.sim_time_s += dt;
            w.tick_pets(dt);
            if tem_bau(&w) == 0 {
                aberto_em = Some(k as f32 * dt);
                break;
            }
        }
        let t = aberto_em.expect("the pet never opened the chest");
        assert!(t >= shared::forte::ABRIR_S, "opened in {t:.1}s, sooner than a player can");
        let s = &w.sessions[&sid];
        assert!(s.inventory.iter().any(|x| x.qty > 0) || !s.dungeon.correio.is_empty(), "the owner got nothing");
    }

    /// SEGUINDO O DONO, o pet reporta a velocidade com que anda de verdade.
    ///
    /// O dono: "o facing do pet ta levemente bugado, qnd ele anda pra item ta
    /// ok mas ele volta pro personagem [e] n se move em direcao da direcao q
    /// o pet ta andando de fato".
    ///
    /// O motivo estava no ramo "chegou dentro do passo", que zerava a
    /// velocidade. Perseguindo um dono que anda, a sobra e' SEMPRE menor que
    /// um passo — o pet vivia nesse ramo, `rumo::escolhe` via 0 u/s (abaixo de
    /// `VEL_MINIMA`) e nao mandava rumo, entao o cliente segurava o angulo
    /// velho. O teste mede o RUMO que sai no fio, que e' o que vira o bicho.
    #[test]
    fn o_pet_que_segue_o_dono_olha_pra_onde_anda() {
        let (mut w, sid) = mundo();
        let id = shared::item_id::pet_no_grau(shared::item_id::PET_BASE, 3);
        w.sessions.get_mut(&sid).unwrap().equipment.pet = Some(id);
        w.sincroniza_pets();
        let (_, inicio) = pet_de(&w).expect("o pet tem que nascer");

        // O dono caminha pro +X, bem alem da distancia de seguir, e o pet vai
        // atras. Sem saque nenhum no chao: e' a volta pro personagem.
        let dono = w.sessions[&sid].entity.unwrap();
        let dt = 1.0 / 30.0;
        let mut vistos = Vec::new();
        for k in 1..=90 {
            {
                let mut p = w.ecs.get::<&mut Position>(dono).unwrap();
                p.0 = Vec2::new(k as f32 * shared::PLAYER_SPEED * dt, 0.0);
            }
            w.tick_pets(dt);
            let (pos, vel) = w
                .ecs
                .query::<(&Position, &Velocity, &PetTag)>()
                .iter()
                .map(|(_, (p, v, _))| (p.0, v.0))
                .next()
                .unwrap();
            vistos.push((pos, vel));
        }

        let (fim, _) = *vistos.last().unwrap();
        assert!(
            fim.x - inicio.x > 5.0,
            "o pet nem seguiu o dono: andou {:.1} u",
            fim.x - inicio.x
        );

        // Nos ultimos quadros ele esta' em regime: andando pro +X atras do
        // dono. O rumo que sai no fio tem que dizer isso.
        let q = std::f32::consts::FRAC_PI_2; // +X
        let mut com_rumo = 0;
        for (pos, vel) in &vistos[60..] {
            let r = crate::rumo::escolhe(*pos, None, None, *vel, None);
            if r == 0 {
                continue;
            }
            com_rumo += 1;
            let y = shared::yaw_de_rumo(r).unwrap();
            assert!(
                (y - q).abs() < 0.8,
                "o pet anda pro +X e o fio manda olhar pra {y:.2} rad \
                 (velocidade reportada {vel:?})"
            );
        }
        assert!(
            com_rumo >= 20,
            "so' {com_rumo} de 30 quadros mandaram rumo: seguindo o dono o pet \
             reporta velocidade zero e o cliente segura o angulo velho"
        );
    }

    #[test]
    fn equipar_faz_nascer_e_desequipar_faz_sumir() {
        let (mut w, sid) = mundo();
        w.sincroniza_pets();
        assert!(pet_de(&w).is_none(), "sem pet equipado nao nasce nada");

        let id = shared::item_id::pet_no_grau(shared::item_id::PET_BASE, 3);
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

    /// O bicho ja' chega na bolsa com a afinidade, e EQUIPAR nao encosta
    /// nela. Foi um pedido direto do dono: o que a criatura da' e' o que ela
    /// trouxe, e so' a Pedra de Afinidade troca. Se o equipar sorteasse, cada
    /// clique no slot seria um dado de graca — exatamente o que a Pedra vende.
    ///
    /// E dois bichos iguais nao empilham: cada um leva o SEU sorteio, o que
    /// so' cabe em slots separados.
    #[test]
    fn o_bicho_nasce_sorteado_e_equipar_nao_re_rola() {
        let (mut w, sid) = mundo();
        let id = shared::item_id::pet_no_grau(shared::item_id::PET_BASE, 2);
        poe_na_bolsa(&mut w, sid, id, 2);

        let s = &w.sessions[&sid];
        let bicho: Vec<_> = s
            .inventory
            .iter()
            .filter(|x| x.item_id == id && x.qty > 0)
            .collect();
        assert_eq!(bicho.len(), 2, "dois bichos, dois slots — nunca uma pilha");
        let nasceu = bicho[0].instance.and_then(|i| i.afinidade);
        assert!(nasceu.is_some(), "o bicho tem que nascer com a afinidade");
        assert!(bicho[1].instance.and_then(|i| i.afinidade).is_some());

        let idx = s.inventory.iter().position(|x| x.item_id == id).unwrap();
        w.handle_use_item(sid, idx);
        let s = &w.sessions[&sid];
        assert_eq!(s.equipment.pet, Some(id), "equipou");
        assert_eq!(
            s.equipment.pet_inst.and_then(|i| i.afinidade),
            nasceu,
            "equipar NAO pode re-rolar a afinidade"
        );
    }

    #[test]
    fn o_pet_anda_ate_o_saque_e_credita_no_dono() {
        let (mut w, sid) = mundo();
        let id = shared::item_id::pet_no_grau(shared::item_id::PET_BASE, 5);
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
        let id = shared::item_id::pet_no_grau(shared::item_id::PET_BASE, 1);
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
        let id = shared::item_id::pet_no_grau(shared::item_id::PET_BASE, 2);
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
        let id = shared::item_id::pet_no_grau(shared::item_id::PET_BASE, 3);
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

    /// O atirador para no ALCANCE e atira; nunca anda de re'. Antes ele
    /// parava no `kite_dist` (bem mais perto que o tiro exigia) e recuava
    /// quando o jogador colava — andava de re' pelo mapa inteiro.
    #[test]
    fn o_atirador_para_no_alcance_e_nao_recua() {
        use crate::world::atirador_avanca;
        // Mago: alcance 12. Longe avanca, perto para.
        assert!(atirador_avanca(20.0, 12.0), "longe, avanca");
        assert!(!atirador_avanca(10.0, 12.0), "dentro do alcance, para");
        assert!(!atirador_avanca(0.5, 12.0), "colado, para — nao recua");
        // A ultima casquinha do alcance ainda vale avancar: parar em 12 na
        // borda exata deixaria o tiro na sorte do primeiro passo do jogador.
        assert!(atirador_avanca(11.0, 12.0));
        // Ele para ANTES da borda, pra um passo do jogador nao tirar ele de
        // alcance e fazer o bicho tremer avancando a cada quadro.
        assert!(!atirador_avanca(12.0 * 0.9, 12.0));
        assert!(atirador_avanca(12.0 * 0.95, 12.0));
        // Quem tem MAIS alcance para mais longe: a 9,5 o mago (12) ja' atira,
        // e o pistoleiro (9) ainda precisa chegar mais perto.
        assert!(!atirador_avanca(9.5, 12.0), "o mago ja' pode atirar daqui");
        assert!(atirador_avanca(9.5, 9.0), "o pistoleiro ainda avanca");
        // Alcance minusculo nao vira "nunca avanca".
        assert!(atirador_avanca(5.0, 0.5));
    }

    /// Teleporte (portal, viagem, pergaminho): o pet reaparece do lado do
    /// dono. Antes ele voltava ANDANDO — atravessava o mapa inteiro a pe' e
    /// sumia da tela no caminho.
    #[test]
    fn o_pet_reaparece_quando_o_dono_teleporta() {
        let (mut w, sid) = mundo();
        let id = shared::item_id::pet_no_grau(shared::item_id::PET_BASE, 1);
        w.sessions.get_mut(&sid).unwrap().equipment.pet = Some(id);
        w.sincroniza_pets();
        // Cada slot tem o seu lugar em volta do dono: tres pets nao se empilham.
        let lugar = posicao_de_seguir(shared::EquipSlot::Pet);
        let (_, antes) = pet_de(&w).expect("nasceu");
        assert!(antes.distance(lugar) < 1.0);

        // O dono some pro outro lado do mapa.
        let longe = Vec2::new(300.0, -180.0);
        let e = w.sessions[&sid].entity.unwrap();
        if let Ok(mut pos) = w.ecs.get::<&mut Position>(e) {
            pos.0 = longe;
        }
        w.tick_pets(1.0 / 30.0);
        let (_, depois) = pet_de(&w).expect("continua no mundo");
        assert!(
            depois.distance(longe + lugar) < 0.01,
            "o pet tinha que aparecer do lado do dono, e esta' em {depois:?}"
        );

        // Um passo normal continua sendo caminhada, nao teletransporte.
        if let Ok(mut pos) = w.ecs.get::<&mut Position>(e) {
            pos.0 = longe + Vec2::new(6.0, 0.0);
        }
        w.tick_pets(1.0 / 30.0);
        let (_, andando) = pet_de(&w).expect("continua no mundo");
        assert!(
            andando.distance(longe + Vec2::new(6.0, 0.0)) > 1.0,
            "6 tiles se anda, nao se teleporta"
        );
    }

    /// Entrar em dungeon muda a instancia: o pet tem que renascer lá dentro,
    /// senao ele fica na instancia velha — invisivel e sem ver saque.
    #[test]
    fn o_pet_segue_o_dono_pra_dungeon() {
        let (mut w, sid) = mundo();
        let id = shared::item_id::pet_no_grau(shared::item_id::PET_BASE, 2);
        w.sessions.get_mut(&sid).unwrap().equipment.pet = Some(id);
        w.sincroniza_pets();
        let inst_do_pet = |w: &GameWorld| {
            w.ecs
                .query::<(&PetTag, Option<&dungeon::Instancia>)>()
                .iter()
                .map(|(_, (_, i))| i.map_or(0, |i| i.0))
                .next()
        };
        assert_eq!(inst_do_pet(&w), Some(0), "nasceu no mundo aberto");

        w.sessions.get_mut(&sid).unwrap().instancia = 7;
        w.sincroniza_pets();
        assert_eq!(inst_do_pet(&w), Some(7), "seguiu pra dungeon");
        assert_eq!(w.ecs.query::<&PetTag>().iter().count(), 1, "nao duplicou");

        w.sessions.get_mut(&sid).unwrap().instancia = 0;
        w.sincroniza_pets();
        assert_eq!(inst_do_pet(&w), Some(0), "voltou com o dono");
    }

    /// Equipar montaria tem que aparecer PRA TODO MUNDO. Antes so' o login e
    /// a troca de preferencia atualizavam a montaria vista, entao quem
    /// equipava montava sem bicho nenhum embaixo: andava mais rapido e
    /// continuava a pe' na tela.
    #[test]
    fn equipar_montaria_atualiza_o_que_os_outros_veem() {
        let (mut w, sid) = mundo();
        assert_eq!(w.sessions[&sid].montaria_vista, None);

        let id = shared::item_id::montaria_no_grau(shared::item_id::MONTARIA_BASE, 3);
        w.sessions.get_mut(&sid).unwrap().equipment.montaria = Some(id);
        w.sincroniza_montarias();
        assert_eq!(
            w.sessions[&sid].montaria_vista,
            Some(id),
            "equipou: os outros tem que ver a montaria"
        );

        // Desequipar no meio da montada derruba quem estava montado.
        {
            let s = w.sessions.get_mut(&sid).unwrap();
            s.montado = true;
            s.equipment.montaria = None;
        }
        w.sincroniza_montarias();
        assert_eq!(w.sessions[&sid].montaria_vista, None);
        assert!(
            !w.sessions[&sid].montado,
            "sem montaria equipada ninguem fica montado no ar"
        );
    }

    /// A cor da montaria e' que manda na velocidade de quem monta.
    #[test]
    fn a_cor_da_montaria_muda_a_velocidade() {
        let cinza = shared::item_id::montaria_no_grau(shared::item_id::MONTARIA_BASE, 1);
        let laranja = shared::item_id::montaria_no_grau(shared::item_id::MONTARIA_BASE, 5);
        let vel = |id| {
            shared::loja::velocidade_de_andar(
                shared::PLAYER_SPEED,
                shared::loja::mult_de_montaria(true, Some(id)),
                1.0,
            )
        };
        assert!(vel(laranja) > vel(cinza));
        assert_eq!(vel(cinza), shared::PLAYER_SPEED * shared::loja::VEL_MONTADO);
        // A pe' nenhuma cor acelera.
        assert_eq!(
            shared::loja::velocidade_de_andar(
                shared::PLAYER_SPEED,
                shared::loja::mult_de_montaria(false, Some(laranja)),
                1.0
            ),
            shared::PLAYER_SPEED
        );
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
        assert!(
            meu.liberado_para(eu, 100.0),
            "pro dono, desde o primeiro quadro"
        );
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
        let id = shared::item_id::pet_no_grau(shared::item_id::PET_BASE, 5);
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
        let id = shared::item_id::pet_no_grau(shared::item_id::PET_BASE, 5);
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
        let id = shared::item_id::pet_no_grau(shared::item_id::PET_BASE, 5);
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
