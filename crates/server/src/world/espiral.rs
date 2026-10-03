//! THE NEON SPIRE (`shared::kogen::paradas_da_espiral`): Kōgen-tō's map
//! dungeon. Its hordes are ordinary level-range zones on the ramp's landings;
//! what lives here is the ELITE guard on one landing a lap — eight times the
//! life, half again the bite, five minutes to come back, and a chest where it
//! falls.

use super::*;

/// How much tougher an elite is: life, damage, experience.
const ELITE_VIDA: f32 = 8.0;
const ELITE_DANO: f32 = 1.5;
const ELITE_XP: f32 = 5.0;

impl GameWorld {
    /// Is `zone_id` one of the spire's elite landings?
    pub(super) fn e_elite_da_espiral(&self, zone_id: u32) -> bool {
        if !shared::kogen::e_kogen(&self.zona) {
            return false;
        }
        let Some(z) = self.spawn_zones.iter().find(|z| z.id == zone_id) else {
            return false;
        };
        let c = z.origin + z.size * 0.5;
        shared::kogen::paradas_da_espiral().iter().any(|(p, _, elite)| *elite && p.distance(c) < 0.5)
    }

    /// Turns the freshly spawned `eid` into the landing's elite guard.
    pub(super) fn tornar_elite_da_espiral(&mut self, eid: EntityId) {
        for (_, (n, tag, hp, kind)) in self.ecs.query_mut::<(&NetId, &mut EnemyTag, &mut Health, &EntityKind)>() {
            if n.0 != eid {
                continue;
            }
            let vida = ((hp.max as f32) * ELITE_VIDA).round() as i32;
            hp.max = vida;
            hp.current = vida;
            tag.stats.hp_max = vida;
            tag.stats.attack_damage = ((tag.stats.attack_damage as f32) * ELITE_DANO).round().max(1.0) as i32;
            tag.xp_reward = (tag.xp_reward as f32 * ELITE_XP).round() as u64;
            tag.elite_da_espiral = true;
            let k = if let EntityKind::Enemy(k) = kind { *k } else { 0 };
            tag.boss_name = Some(format!("[ELITE] Spire Sentinel {}", crate::economy::enemy_def(k).name));
            return;
        }
    }

    /// An elite fell at `onde` (slot `slot_idx` of zone `zone_id`): one green
    /// chest of the Kōgen Tower's table, and the slot sleeps five minutes.
    pub(super) fn elite_da_espiral_caiu(&mut self, onde: Vec2, zone_id: u32, slot_idx: u32) {
        // The table of the top-level field boss (the Kōgen Tower's).
        let conteudo = shared::forte::conteudo_do_chefe(65);
        self.forte_largar_n_baus(onde, conteudo, &[2]);
        if let Some(z) = self.spawn_zones.iter_mut().find(|z| z.id == zone_id) {
            if let Some(s) = z.slots.get_mut(slot_idx as usize) {
                s.respawn_at = self.sim_time_s + ELITE_DA_ESPIRAL_VOLTA_S;
            }
        }
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    /// An ELITE on its landing: eight times the life, a name that says so,
    /// one green chest where it falls, and five minutes before it is back.
    #[test]
    fn elite_da_espiral_e_forte_larga_bau_e_demora_a_voltar() {
        crate::economy::init_vazia_para_testes();
        let mut w = GameWorld::new(HashMap::new());
        w.zona = shared::kogen::ZONA.to_string();
        let (c, nivel, _) = shared::kogen::paradas_da_espiral().into_iter().find(|p| p.2).unwrap();
        w.spawn_zones.push(ServerSpawnZone {
            id: 10_777,
            origin: c - Vec2::splat(12.0),
            size: Vec2::splat(24.0),
            respawn_delay_s: 20.0,
            quotas: Vec::new(),
            live: Vec::new(),
            respawn_queue: Vec::new(),
            polygon: None,
            level_range: Some((nivel, nivel, 1)),
            level_range_live: 0,
            level_range_queue: Vec::new(),
            slots: vec![SpawnSlot { pos: c, occupant: None, respawn_at: 0.0 }],
            active: true,
            last_player_near_at: 0.0,
            forte: false,
        });
        assert!(w.e_elite_da_espiral(10_777));
        let comum = w.place_enemy_in_zone_with_build(c, 56, 10_777, nivel as u16, false);
        let vida_comum = w.ecs.query::<(&NetId, &Health)>().iter().find(|(_, (n, _))| n.0 == comum).unwrap().1 .1.max;
        let eid = w.place_enemy_in_zone_with_build(c, 56, 10_777, nivel as u16, false);
        w.tornar_elite_da_espiral(eid);
        let (vida, nome) = w
            .ecs
            .query::<(&NetId, &Health, &EnemyTag)>()
            .iter()
            .find(|(_, (n, _, _))| n.0 == eid)
            .map(|(_, (_, h, t))| (h.max, t.boss_name.clone()))
            .unwrap();
        assert_eq!(vida, ((vida_comum as f32) * ELITE_VIDA).round() as i32);
        assert!(nome.is_some_and(|n| n.starts_with("[ELITE]")));
        w.elite_da_espiral_caiu(c, 10_777, 0);
        let baus: Vec<u8> = w.ecs.query::<&forte::BauDoForte>().iter().map(|(_, b)| b.cor).collect();
        assert_eq!(baus, vec![2], "one green chest");
        assert!(w.spawn_zones[0].slots[0].respawn_at >= w.sim_time_s + 299.0, "came back too soon");
    }
}
