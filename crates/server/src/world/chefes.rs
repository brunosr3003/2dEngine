//! Chefes de campo na ilha: onde nascem, quando voltam e os golpes
//! telegrafados. O catalogo (quem sao, formas, numeros) mora em
//! `shared::bosses`.

use super::*;
use shared::bosses as cat;

/// Uma vaga de chefe na ilha: o lugar dele e quando volta.
#[derive(Debug, Clone)]
pub struct VagaDeChefe {
    pub kind: u16,
    pub pos: Vec2,
    pub vivo: Option<Entity>,
    /// sim_time em que renasce (vale com `vivo == None`).
    pub volta_em: f32,
}

/// O golpe que o chefe esta' carregando.
#[derive(Debug, Clone, Copy)]
pub struct Carga {
    pub hab: usize,
    pub id: u32,
    pub centro: Vec2,
    pub dir: Vec2,
    pub impacto_em: f32,
}

/// Estado de golpes do chefe, no ECS junto do `EnemyTag`.
#[derive(Debug, Clone)]
pub struct ChefeVivo {
    pub kind: u16,
    pub prontas_em: [f32; cat::MAX_HABILIDADES],
    pub livre_em: f32,
    pub carga: Option<Carga>,
}

/// Distancia minima entre dois chefes e de uma zona segura (cidade, porto).
const LONGE_DE_OUTRO_CHEFE: f32 = 150.0;
const LONGE_DA_ZONA_SEGURA: f32 = 130.0;

/// Os lugares dos `n` chefes de uma ilha, entre `candidatos` (sitios planos):
/// longe das zonas seguras e uns dos outros, espalhados do perto pro longe da
/// cidade — o i-esimo (mais fraco primeiro) cai na fracao (i+1)/(n+1) da
/// lista ordenada por distancia. Deterministico: mesma ilha, mesmos lugares.
pub fn sitios_de_chefe(candidatos: &[Vec2], origem: Vec2, seguras: &[Vec2], n: usize) -> Vec<Vec2> {
    let mut c: Vec<Vec2> = candidatos
        .iter()
        .copied()
        .filter(|p| seguras.iter().all(|s| s.distance(*p) >= LONGE_DA_ZONA_SEGURA))
        .collect();
    c.sort_by(|a, b| a.distance_squared(origem).total_cmp(&b.distance_squared(origem)).then(a.x.total_cmp(&b.x)));
    let mut escolhidos: Vec<Vec2> = Vec::new();
    if c.is_empty() {
        return escolhidos;
    }
    for i in 0..n {
        let alvo = ((i + 1) * c.len() / (n + 1)).min(c.len() - 1);
        // Do indice alvo pra fora, alternando os lados.
        let achado = (0..c.len()).find_map(|k| {
            let cand = [alvo + k, alvo.wrapping_sub(k)];
            cand.into_iter().filter(|j| *j < c.len()).map(|j| c[j]).find(|p| {
                escolhidos.iter().all(|e| e.distance(*p) >= LONGE_DE_OUTRO_CHEFE)
            })
        });
        if let Some(p) = achado {
            escolhidos.push(p);
        }
    }
    escolhidos
}

/// Loot do chefe por cima do da tabela: material bom da faixa, sem
/// equipamento (decisao: peca so' de craft e de bau de dungeon/raid).
pub fn loot_de_chefe(kind: u16, seed: u64) -> Vec<(u16, u32)> {
    use shared::item_id::*;
    let Some(c) = cat::chefe(kind) else { return Vec::new() };
    let n = c.nivel;
    let cor: u8 = match n { 0..=14 => 1, 15..=29 => 2, _ => 3 };
    let mut s = seed ^ 0xB055_C0DE_u64;
    let mut rnd = || {
        s = s.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        ((s >> 33) as f32) / (1u64 << 31) as f32
    };
    let mut v = vec![
        (COPPER, 200 + n * 25 + (rnd() * 150.0) as u32),
        (DARKSTEEL, 20 + n * 4),
        (na_cor(STEEL, cor), 3 + n / 6),
        (GREATER_HEAL, 2),
    ];
    if rnd() < 0.6 {
        v.push((na_cor(PLATINUM, cor), 1 + n / 12));
    }
    if rnd() < 0.25 {
        v.push((GLITTERING_POWDER, 1));
    }
    v
}

/// Os drops da morte com o do chefe junto (mob comum passa igual).
pub fn com_loot_de_chefe(mut drops: Vec<(u16, u32)>, kind: u16, seed: u64) -> Vec<(u16, u32)> {
    drops.extend(loot_de_chefe(kind, seed));
    drops
}

impl GameWorld {
    /// Poe os chefes da ilha nos lugares deles. Chamado no fim de
    /// `povoar_ilha`, com as zonas seguras ja' montadas.
    pub(super) fn povoar_chefes(&mut self) {
        use shared::terreno::BLOCO;
        self.vagas_de_chefe.clear();
        let (Some(ilha), Some(def)) = (self.ilha.as_ref(), shared::terreno::def_da_zona(&self.zona)) else {
            return;
        };
        let chefes = cat::da_zona(def.zona);
        if chefes.is_empty() {
            return;
        }
        // Sitios planos numa grade grossa: a arena precisa de chao.
        let passo = 40i32;
        let raio_sitio = (7.0 / BLOCO) as i32;
        let mut candidatos = Vec::new();
        let mut b = -def.raio_blocos;
        while b < def.raio_blocos {
            let mut a = -def.raio_blocos;
            while a < def.raio_blocos {
                if ilha.sitio_plano(a + def.raio_blocos, b + def.raio_blocos, raio_sitio) {
                    let p = Vec2::new(a as f32 * BLOCO, b as f32 * BLOCO);
                    if !ilha.agua(p.x, p.y) && ilha.sem_estorvo(p, ENTITY_RADIUS) {
                        candidatos.push(p);
                    }
                }
                a += passo;
            }
            b += passo;
        }
        let seguras: Vec<Vec2> = self.safe_zones.iter().map(|(o, s)| *o + *s * 0.5).collect();
        let lugares = sitios_de_chefe(&candidatos, self.porto_da_ilha, &seguras, chefes.len());
        for (c, pos) in chefes.iter().zip(lugares) {
            let e = self.nascer_chefe(c.kind, pos);
            self.vagas_de_chefe.push(VagaDeChefe { kind: c.kind, pos, vivo: e, volta_em: 0.0 });
            tracing::info!("chefe '{}' nv {} em ({:.0},{:.0})", c.nome, c.nivel, pos.x, pos.y);
        }
    }

    fn nascer_chefe(&mut self, kind: u16, pos: Vec2) -> Option<Entity> {
        let c = cat::chefe(kind)?;
        // Stats de comportamento (alcance, tiro, recuo) do mob preset do corpo;
        // os numeros de chefe por cima.
        let base = match c.corpo {
            cat::Corpo::Bicho(k) | cat::Corpo::Gente(k) => k,
            cat::Corpo::Pirata => 0,
        };
        let (mut tag, mut health) = self.build_enemy_tag(base, pos, 28.0, pos);
        let hp = cat::vida(c.nivel);
        health.max = hp;
        health.current = hp;
        tag.stats.hp_max = hp;
        tag.stats.attack_damage = cat::dano(c.nivel);
        tag.stats.defense = c.nivel as i32 / 2;
        tag.level = c.nivel;
        tag.is_boss = true;
        tag.boss_name = Some(c.nome.to_string());
        tag.detect_range = 14.0;
        tag.xp_reward = cat::xp(c.nivel);
        tag.size_scale = c.escala;
        tag.attack_range = tag.attack_range.max(2.4);
        // Golpe comum nao se esquiva: cadencia de chefe, nao a do bicho.
        tag.attack_cooldown_base = tag.attack_cooldown_base.max(cat::CADENCIA_COMUM_S);
        // Sem a guarda de 75% da IA antiga: chefe telegrafado se vence
        // esquivando, nao esperando a guarda baixar.
        tag.ai_block_cd_until = f32::INFINITY;
        let eid = self.alloc_entity_id();
        let body = self.spawn_entity_body(pos);
        let e = self.ecs.spawn((
            NetId(eid),
            Position(pos),
            Velocity(Vec2::ZERO),
            health,
            EntityKind::Enemy(kind),
            tag,
            body,
            ChefeVivo { kind, prontas_em: [0.0; cat::MAX_HABILIDADES], livre_em: 0.0, carga: None },
        ));
        Some(e)
    }

    /// Chefe morto marca a volta; hora de voltar, renasce no lugar dele.
    pub(super) fn tick_vagas_de_chefe(&mut self) {
        let agora = self.sim_time_s;
        let mut nascer = Vec::new();
        for (i, v) in self.vagas_de_chefe.iter_mut().enumerate() {
            match v.vivo {
                Some(e) => {
                    let vivo = self.ecs.get::<&EnemyTag>(e).map(|t| !t.dead).unwrap_or(false);
                    if !vivo {
                        v.vivo = None;
                        let nivel = cat::chefe(v.kind).map_or(1, |c| c.nivel);
                        v.volta_em = agora + cat::respawn_s(nivel);
                    }
                }
                None if agora >= v.volta_em => nascer.push(i),
                None => {}
            }
        }
        for i in nascer {
            let (kind, pos) = (self.vagas_de_chefe[i].kind, self.vagas_de_chefe[i].pos);
            let e = self.nascer_chefe(kind, pos);
            self.vagas_de_chefe[i].vivo = e;
        }
    }

    /// Os golpes telegrafados. Roda depois da IA dos mobs e antes de integrar
    /// o movimento: o chefe que carrega fica parado, virado pro golpe.
    pub(super) fn tick_telegrafos_de_chefe(&mut self) {
        let agora = self.sim_time_s;
        let tick = self.tick;
        let jogadores: Vec<(EntityId, Vec2)> = self
            .ecs
            .query::<(&NetId, &Position, &PlayerTag, &Health)>()
            .iter()
            .filter(|(_, (_, _, _, hp))| hp.current > 0)
            .map(|(_, (n, p, _, _))| (n.0, p.0))
            .collect();
        let posicoes: Vec<Vec2> = jogadores.iter().map(|j| j.1).collect();
        // Vida maxima e resistencia de quem pode tomar: o telegrafado tira
        // FRACAO DA VIDA (`cat::dano_telegrafado`), nao um numero fixo.
        let defesas: HashMap<EntityId, (i32, i32, f32)> = self
            .sessions
            .values()
            .map(|s| (s.entity_id, (s.stats.hp_max, s.stats.defense, s.stats.damage_reduction_pct)))
            .collect();
        let mut avisos: Vec<(Vec2, f32, ServerMessage)> = Vec::new();
        let mut golpes: Vec<(EntityId, i32, EntityId, Vec2, f32)> = Vec::new();
        for (_, (net, pos, vel, tag, hp, ch)) in self.ecs.query_mut::<(
            &NetId,
            &Position,
            &mut Velocity,
            &mut EnemyTag,
            &Health,
            &mut ChefeVivo,
        )>() {
            let Some(def) = cat::chefe(ch.kind) else { continue };
            if tag.dead || hp.current <= 0 {
                if let Some(c) = ch.carga.take() {
                    avisos.push((c.centro, 0.0, ServerMessage::TelegraficoFim { id: c.id, impacto: false }));
                }
                continue;
            }
            let fase = cat::fase(hp.current, hp.max);
            if let Some(c) = ch.carga {
                // Carregando: parado, virado pro golpe, sem golpe comum.
                vel.0 = Vec2::ZERO;
                tag.attack_dir = c.dir;
                tag.attack_cooldown = tag.attack_cooldown_base.max(0.4);
                if agora >= c.impacto_em {
                    let h = &def.habilidades[c.hab];
                    for i in cat::atingidos(&h.forma, c.centro, c.dir, &posicoes) {
                        let alvo = jogadores[i].0;
                        let (hp_max, defesa, reducao) = defesas.get(&alvo).copied().unwrap_or((100, 0, 0.0));
                        let resist = cat::resistencia(defesa, reducao);
                        let quer = cat::dano_telegrafado(h, fase, hp_max, resist);
                        // O hit ainda passa por `dano_mitigado`: manda o bruto
                        // que, mitigado, da' o que o golpe quer tirar.
                        let bruto = (quer as f32 / (1.0 - resist).max(0.1)).ceil() as i32;
                        golpes.push((alvo, bruto, net.0, c.centro, h.empurra));
                    }
                    avisos.push((c.centro, h.forma.alcance(), ServerMessage::TelegraficoFim { id: c.id, impacto: true }));
                    ch.carga = None;
                    ch.prontas_em[c.hab] = agora + cat::recarga(h, fase);
                    ch.livre_em = agora + cat::PAUSA_ENTRE_GOLPES;
                    tag.attack_cooldown = 0.6;
                }
                continue;
            }
            if agora < ch.livre_em || tag.returning_home {
                continue;
            }
            let Some(alvo) = tag.ai_target.and_then(|id| jogadores.iter().find(|j| j.0 == id)).map(|j| j.1) else {
                continue;
            };
            let dist = alvo.distance(pos.0);
            let Some(i) = cat::escolher(def, &ch.prontas_em, agora, fase, dist) else { continue };
            let h = &def.habilidades[i];
            let (centro, dir) = cat::centro_e_dir(h.mira, pos.0, alvo);
            let carga = cat::carga(h, fase);
            let id = ((net.0 .0 as u32) << 12) ^ tick;
            ch.carga = Some(Carga { hab: i, id, centro, dir, impacto_em: agora + carga });
            vel.0 = Vec2::ZERO;
            tag.attack_dir = dir;
            tag.attack_cooldown = tag.attack_cooldown_base.max(0.4);
            avisos.push((
                centro,
                h.forma.alcance(),
                ServerMessage::Telegrafico {
                    id,
                    chefe: net.0,
                    forma: h.forma,
                    centro: [centro.x, centro.y],
                    dir: [dir.x, dir.y],
                    carga_s: carga,
                },
            ));
        }
        for (alvo, dano, chefe, centro, empurra) in golpes {
            let hurt_dir = calc_hurt_dir_from_eid(&self.ecs, alvo, centro);
            self.pending_skill_hits.push(PendingSkillHit {
                target_net: alvo,
                damage: dano,
                attacker_net: chefe,
                hurt_dir,
                is_crit: false,
                from_player: false,
                knockback: empurra,
            });
        }
        if avisos.is_empty() {
            return;
        }
        // Quem ve': jogador perto do centro do golpe (AOI mais o tamanho da forma).
        let ouvintes: Vec<(SessionId, Vec2)> = self
            .sessions
            .iter()
            .filter(|(_, s)| s.logged_in)
            .filter_map(|(sid, s)| s.entity.and_then(|e| self.ecs.get::<&Position>(e).ok().map(|p| (*sid, p.0))))
            .collect();
        for (centro, alcance, msg) in avisos {
            for (sid, p) in &ouvintes {
                if p.distance(centro) <= AOI_RADIUS * 1.5 + alcance {
                    if let Some(s) = self.sessions.get(sid) {
                        let _ = s.handle.to_client.send(msg.clone());
                    }
                }
            }
        }
    }

    /// Os chefes pro `MapaDaIlha`.
    pub(super) fn chefes_no_mapa(&self) -> Vec<cat::ChefeNoMapa> {
        self.vagas_de_chefe
            .iter()
            .filter_map(|v| {
                let c = cat::chefe(v.kind)?;
                Some(cat::ChefeNoMapa {
                    kind: v.kind,
                    nome: c.nome.to_string(),
                    nivel: c.nivel as u16,
                    centro: [v.pos.x, v.pos.y],
                    vivo: v.vivo.is_some(),
                })
            })
            .collect()
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn sitios_longe_da_cidade_e_uns_dos_outros() {
        let mut cand = Vec::new();
        for i in 0..40 {
            for j in 0..40 {
                cand.push(Vec2::new(i as f32 * 20.0 - 400.0, j as f32 * 20.0 - 400.0));
            }
        }
        let cidade = Vec2::ZERO;
        let s = sitios_de_chefe(&cand, cidade, &[cidade, Vec2::new(300.0, 0.0)], 3);
        assert_eq!(s.len(), 3);
        for (i, p) in s.iter().enumerate() {
            assert!(p.distance(cidade) >= LONGE_DA_ZONA_SEGURA);
            assert!(p.distance(Vec2::new(300.0, 0.0)) >= LONGE_DA_ZONA_SEGURA);
            for q in &s[i + 1..] {
                assert!(p.distance(*q) >= LONGE_DE_OUTRO_CHEFE);
            }
        }
        // O mais fraco (primeiro) mais perto da cidade que o mais forte.
        assert!(s[0].distance(cidade) <= s[2].distance(cidade));
        assert_eq!(s, sitios_de_chefe(&cand, cidade, &[cidade, Vec2::new(300.0, 0.0)], 3), "deterministico");
        assert!(sitios_de_chefe(&[], cidade, &[], 2).is_empty());
    }

    #[test]
    fn loot_de_chefe_e_material_da_faixa_sem_equipamento() {
        use shared::item_id::*;
        let baixo = loot_de_chefe(10, 7);
        assert!(baixo.iter().any(|(id, _)| *id == na_cor(STEEL, 1)));
        let alto = loot_de_chefe(18, 7);
        assert!(alto.iter().any(|(id, _)| *id == na_cor(STEEL, 3)));
        assert!(loot_de_chefe(0, 7).is_empty(), "mob comum nao ganha loot de chefe");
        assert_eq!(com_loot_de_chefe(vec![(COPPER, 5)], 0, 1), vec![(COPPER, 5)]);
    }
}
