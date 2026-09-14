use super::*;

impl GameWorld {
    fn posicao_boss_teste(&self, centro: Vec2) -> Option<Vec2> {
        if !centro.is_finite() { return None; }
        for raio in [10.0, 14.0, 18.0] {
            for i in 0..24 {
                let ang = i as f32 * std::f32::consts::TAU / 24.0;
                let p = centro + Vec2::new(ang.cos(), ang.sin()) * raio;
                let livre = match &self.ilha {
                    Some(ilha) => !ilha.agua(p.x, p.y) && ilha.sem_estorvo(p, ENTITY_RADIUS)
                        && (ilha.altura(p.x, p.y) - ilha.altura(centro.x, centro.y)).abs() <= 1.0,
                    None => self.map.is_walkable(p.x.floor() as i32, p.y.floor() as i32),
                };
                if livre && !self.in_safe_zone(p) && visada(self.ilha.as_ref(), &self.map, centro, p) {
                    return Some(p);
                }
            }
        }
        None
    }

    pub(super) fn spawn_test_boss(&mut self, centro: Vec2, hp: i32) -> Result<(EntityId, Vec2), &'static str> {
        let pos = self.posicao_boss_teste(centro).ok_or("Sem espaco acessivel fora da zona segura.")?;
        let kind = crate::economy::KIND_CHEFE;
        let (mut tag, mut health) = self.build_enemy_tag(kind, pos, 35.0, pos);
        health.max = hp.clamp(1_000, 60_000);
        health.current = health.max;
        tag.stats.hp_max = health.max;
        tag.stats.attack_damage = tag.stats.attack_damage.max(90);
        tag.level = 20;
        tag.is_boss = true;
        tag.boss_name = Some("Guardiao de Treino".into());
        // Fica visivel perto do login, mas so inicia a luta quando provocado.
        tag.detect_range = 4.0;
        tag.wander_timer = f32::MAX;
        tag.wander_waypoint = pos;
        let eid = self.alloc_entity_id();
        let body = self.spawn_entity_body(pos);
        self.ecs.spawn((NetId(eid), Position(pos), Velocity(Vec2::ZERO), health,
            EntityKind::Enemy(kind), tag, body));
        tracing::info!("boss de teste: id={} hp={} pos=({:.1},{:.1})", eid.0, health.max, pos.x, pos.y);
        Ok((eid, pos))
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn boss_perto_visivel_fora_do_raio_de_limpeza_e_da_zona_segura() {
        let mut w = GameWorld::new(HashMap::new());
        w.map = shared::mapfile::MapFile::new("boss-test", 80, 80).to_world_map();
        let centro = Vec2::splat(40.0);
        w.safe_zones.push((Vec2::new(45.0, 35.0), Vec2::splat(10.0)));
        let p = w.posicao_boss_teste(centro).unwrap();
        assert!(p.distance(centro) > 6.0 && p.distance(centro) < shared::AOI_RADIUS);
        assert!(!w.in_safe_zone(p));
        w.safe_zone = true;
        assert!(w.posicao_boss_teste(centro).is_none());
        assert!(w.posicao_boss_teste(Vec2::splat(f32::NAN)).is_none());
    }
}
