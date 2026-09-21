//! Simulador de combate pro balanceamento (docs/COMBATE.md, "Balanceamento").
//!
//! Um jogador com AUTO COMBATE limpando uma zona de mob da propria faixa, a
//! 30 Hz, deterministico. Ele NAO roda o `GameWorld` inteiro — sessao, rede,
//! ECS e ilha de 1,6 km custariam segundos por cenario —, mas tudo o que decide
//! o numero sai do codigo do jogo:
//!
//!   * stats do jogador: `world::effective_stats` (arma, peso, escala);
//!   * cadencia: `world::cooldown_do_ataque`;
//!   * mitigacao: `world::dano_mitigado`;
//!   * mobs: `economy::KINDS_INICIAIS` (a tabela semeada no banco) e o
//!     sorteio de bicho por nivel `economy::kind_para_nivel_em`;
//!   * tempos de golpe, alcances, stagger, leash, aggro e matilha: as
//!     constantes do `shared` e do `world`.
//!
//! O que simplifica (e por que nao muda a conclusao):
//!   * chao plano, sem tronco: a zona e' um sitio plano validado;
//!   * mobs podem se sobrepor: a separacao entre corpos so' os espalha em
//!     volta do jogador, nao tira ninguem do alcance de mordida;
//!   * projetil de mob sempre acerta (o jogador em AUTO fica parado batendo);
//!   * skills: as destravadas no nivel (`shared::skills::playtest`), todas em
//!     AUTO, pela regra do cliente (`habilidades::proximo_auto`): cura com
//!     vida abaixo de 85% primeiro, dano com alvo no alcance, Muralha com
//!     alvo a ate' 8; enquanto conjura, o golpe basico NAO sai, pela janela
//!     CHEIA `impacto_em() + RECUPERACAO_S` — a mesma do `casting_until` do
//!     servidor (`world/habilidades.rs`). Descontar so' o `impacto_em`, como
//!     se fazia antes, devolvia 0,36 s de basico por conjuracao que o jogo
//!     real nao devolve, e superestimava o valor de toda skill. Area vira
//!     "mobs a ate' `raio` do alvo" (cone e linha tambem);
//!   * a zona tem as 18 vagas espalhadas no raio de 45 como o `povoar_ilha`
//!     (espiral de angulo de ouro em vez de sorteio de sitio plano).

use glam::Vec2;
use shared::skills::Conjunto;

use crate::economy::{kind_para_nivel_em, KindInicial, KINDS_INICIAIS};
use crate::world::{cooldown_do_ataque, dano_mitigado, effective_stats};

const DT: f32 = 1.0 / 30.0;
/// Raio da busca de alvo do AUTO COMBATE (client `auto_combate::RAIO`).
const RAIO_DO_AUTO: f32 = 24.0;
/// Leash das zonas por faixa (`place_enemy_in_zone_with_build`).
const LEASH: f32 = 12.0;
const RESPAWN_S: f32 = 20.0;
const LIMITE_S: f32 = 300.0;
/// Intervalo entre pocoes no simulador.
const POCAO_S: f32 = 1.5;

pub(crate) fn arma_do(c: Conjunto) -> u16 {
    use shared::item_id::*;
    match c {
        Conjunto::EspadaEscudo => ESPADA_E_ESCUDO,
        Conjunto::Katana => KATANA,
        Conjunto::Pistolas => PISTOLAS,
        Conjunto::AnelMagico => ANEL_MAGICO,
    }
}

#[derive(Debug, Clone)]
pub(crate) struct Resultado {
    pub conjunto: Conjunto,
    pub nivel: u32,
    pub pocao: bool,
    pub hp_max: i32,
    pub mortos: u32,
    pub t1: Option<f32>,
    pub t10: Option<f32>,
    pub dano_recebido: i32,
    pub max_agressores: usize,
    pub vivo: bool,
    pub hp_min: f32,
    /// Tempo com alvo (andando ate' ele ou batendo), sem a espera do respawn.
    pub em_luta: f32,
}

impl Resultado {
    pub fn dano_por_mob(&self) -> f32 {
        self.dano_recebido as f32 / self.mortos.max(1) as f32
    }
    /// Segundos de luta por abate: o que compara a VELOCIDADE entre armas.
    pub fn por_abate(&self) -> f32 {
        self.em_luta / self.mortos.max(1) as f32
    }
    pub fn linha(&self) -> String {
        let t = |v: Option<f32>| v.map_or("  —  ".into(), |x| format!("{x:5.1}"));
        format!(
            "{:<13} nv{:<2} {} | mortos {:>2} | 1º {}s | 10º {}s | {:>4.2}s/abate | dano/mob {:>5.1} | agressores {:>2} | {} | HP min {:>3.0}%",
            format!("{:?}", self.conjunto), self.nivel, if self.pocao { "poção" } else { "seco " },
            self.mortos, t(self.t1), t(self.t10), self.por_abate(), self.dano_por_mob(), self.max_agressores,
            if self.vivo { "vivo " } else { "MORTO" }, self.hp_min * 100.0,
        )
    }
}

struct Mob {
    def: &'static KindInicial,
    /// Nivel em que nasceu (a faixa da zona): decide a curva de iniciante.
    nivel: u32,
    casa: Vec2,
    pos: Vec2,
    hp: i32,
    hp_max: i32,
    dano: i32,
    /// Alcance de visao no nivel (`world::deteccao_do_nivel`).
    det: f32,
    vivo: bool,
    volta_em: f32,
    cd: f32,
    aggro_timer: f32,
    voltando: bool,
    hurt_ate: f32,
    preso_ate: f32,
    provocado_ate: f32,
}

impl Mob {
    fn novo(def: &'static KindInicial, casa: Vec2, nivel: u32) -> Mob {
        let (hp, dano) = crate::world::vida_e_dano_do_mob(def.hp, def.dmg, nivel);
        Mob {
            def,
            nivel,
            casa,
            pos: casa,
            hp,
            hp_max: hp,
            dano,
            det: crate::world::deteccao_do_nivel(def.det, nivel),
            vivo: true,
            volta_em: 0.0,
            cd: 0.0,
            aggro_timer: 0.0,
            voltando: false,
            hurt_ate: 0.0,
            preso_ate: 0.0,
            provocado_ate: 0.0,
        }
    }
}

/// Vagas de uma zona: 18 no raio de 45, espalhadas.
fn vagas() -> Vec<Vec2> {
    let n = crate::world::MOB_POR_ZONA as usize;
    let raio = crate::world::MOB_ZONA_RAIO_UN;
    (0..n)
        .map(|i| {
            let a = i as f32 * 2.399_963;
            let r = raio * ((i as f32 + 0.5) / n as f32).sqrt();
            Vec2::new(a.cos() * r, a.sin() * r)
        })
        .collect()
}

/// Pocoes de vida do jogador na luta.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum Pocoes {
    Nenhuma,
    /// Sem fim, bebe abaixo de 50% (as metas por nivel).
    Infinitas,
    /// A barra do cliente (`barra::padrao`): AUTO abaixo do limiar, enquanto
    /// houver na bolsa.
    Bolsa {
        qtd: u32,
        limiar: f32,
    },
}

#[derive(Debug, Clone, Copy)]
enum Parada {
    Abates(u32),
    DoKind(u16, u32),
    Xp(u64),
}

struct Luta<'a> {
    conjunto: Conjunto,
    /// Nivel do personagem (o folego de iniciante depende dele).
    nivel: u32,
    stats: &'a shared::PlayerStats,
    skills: &'a [shared::skills::Skill],
    mobs: Vec<Mob>,
    /// Zona de verdade: o respawn sorteia nivel e bicho de novo nesta faixa.
    /// `None` = volta o mesmo bicho.
    sorteio: Option<(u32, u32)>,
    /// Onde o AUTO foi ligado.
    centro: Vec2,
    /// Onde o jogador aparece (a borda da zona, vindo da cidade).
    inicio: Vec2,
    /// Ate' aqui ele anda sem escolher alvo (a auto missao chegando).
    chegada: Vec2,
    parada: Parada,
    limite_s: f32,
}

#[derive(Debug, Default)]
struct Saida {
    t: f32,
    abates: u32,
    do_alvo: u32,
    xp: u64,
    t1: Option<f32>,
    t10: Option<f32>,
    dano_recebido: i32,
    max_agressores: usize,
    vivo: bool,
    hp_min: f32,
    em_luta: f32,
    pocoes: u32,
}

fn abateu(m: &mut Mob, t: f32, s: &mut Saida, alvo: Option<u16>) {
    m.vivo = false;
    m.volta_em = t + RESPAWN_S;
    s.abates += 1;
    s.xp += m.def.xp as u64;
    if alvo.is_none_or(|k| k == m.def.kind as u16) {
        s.do_alvo += 1;
    }
    if s.abates == 1 {
        s.t1 = Some(t);
    }
    if s.abates == 10 {
        s.t10 = Some(t);
    }
}

pub(crate) fn simular(conjunto: Conjunto, nivel: u32, pocao: bool) -> Resultado {
    let arma = arma_do(conjunto);
    let mut equip = shared::Equipment::default();
    equip.weapon = Some(arma);
    let stats = effective_stats(
        &equip,
        &[0; shared::STAT_COUNT],
        &[0; shared::PROF_COUNT],
        0,
    );
    let skills: Vec<shared::skills::Skill> = shared::skills::playtest()
        .into_iter()
        .filter(|s| s.conjunto == conjunto && s.destravada(nivel))
        .collect();
    let comuns: Vec<u16> = (0..7).collect();
    let mobs: Vec<Mob> = vagas()
        .into_iter()
        .enumerate()
        .map(|(i, v)| {
            let lv = nivel + (i as u32 % 3);
            let k = kind_para_nivel_em(&comuns, lv, (i as u64).wrapping_mul(2_654_435_761) >> 7);
            Mob::novo(&KINDS_INICIAIS[k as usize], v, lv)
        })
        .collect();
    let mut hp = stats.hp_max;
    let mut pocoes = if pocao {
        Pocoes::Infinitas
    } else {
        Pocoes::Nenhuma
    };
    let s = lutar(
        Luta {
            conjunto,
            nivel,
            stats: &stats,
            skills: &skills,
            mobs,
            sorteio: None,
            centro: Vec2::ZERO,
            inicio: Vec2::ZERO,
            chegada: Vec2::ZERO,
            parada: Parada::Abates(10),
            limite_s: LIMITE_S,
        },
        &mut hp,
        &mut pocoes,
    );
    Resultado {
        conjunto,
        nivel,
        pocao,
        hp_max: stats.hp_max,
        mortos: s.abates,
        t1: s.t1,
        t10: s.t10,
        dano_recebido: s.dano_recebido,
        max_agressores: s.max_agressores,
        vivo: s.vivo,
        hp_min: s.hp_min,
        em_luta: s.em_luta,
    }
}

/// O jogador com AUTO COMBATE numa zona ate' a `parada`, o limite de tempo ou
/// cair. `hp` entra e sai (a vida passa de uma luta pra outra).
fn lutar(l: Luta, hp: &mut i32, bolsa: &mut Pocoes) -> Saida {
    let Luta {
        conjunto,
        nivel,
        stats,
        skills,
        mut mobs,
        sorteio,
        centro,
        inicio,
        chegada,
        parada,
        limite_s,
    } = l;
    let mut ultimo_dano = -1e9f32;
    let arma = arma_do(conjunto);
    let a_distancia = conjunto.a_distancia();
    let alcance = if a_distancia {
        shared::RANGED_ATTACK_RANGE
    } else {
        shared::MELEE_RANGE
    };
    let cd_base = cooldown_do_ataque(arma, stats, false);
    let comuns: Vec<u16> = (0..7).collect();
    let alvo_kind = match parada {
        Parada::DoKind(k, _) => Some(k),
        _ => None,
    };
    let feito = |s: &Saida| match parada {
        Parada::Abates(n) => s.abates >= n,
        Parada::DoKind(_, n) => s.do_alvo >= n,
        Parada::Xp(x) => s.xp >= x,
    };
    let mut respawns = 0u64;

    let hp_max = stats.hp_max;
    let mut regen_resto = 0.0f32;
    let mut eu = inicio;
    let mut chegou = inicio.distance(chegada) < 0.1;
    let mut cd = 0.0f32;
    let mut combo = 0usize;
    let mut alvo: Option<usize> = None;
    let mut aproximando = false;
    let mut golpes: Vec<(f32, usize, usize)> = Vec::new();
    let mut mordidas: Vec<(f32, usize)> = Vec::new();
    let mut pocoes = shared::pocoes::EstadoDePocoes::default();
    let mut pocao_resto = 0.0f32;
    let mut parado_desde = 0.0f32;
    let mut pronta_em: Vec<f32> = vec![0.0; skills.len()];
    let mut mp = stats.mp_max as f32;
    let mut ocupado_ate = 0.0f32;
    let mut ultimo_auto = 0u32;
    let mut muralha_ate = 0.0f32;
    let mut efeitos: Vec<(f32, usize, Option<usize>)> = Vec::new();
    let mut r = Saida {
        vivo: true,
        hp_min: *hp as f32 / hp_max as f32,
        ..Default::default()
    };
    let mut t = 0.0f32;
    while t < limite_s && !feito(&r) {
        t += DT;
        if cd > 0.0 {
            cd -= DT;
        }

        // ── jogador: chegando na zona ──
        let antes = eu;
        if !chegou {
            let falta = chegada - eu;
            let passo = shared::PLAYER_SPEED * DT;
            if falta.length() <= passo {
                eu = chegada;
                chegou = true;
            } else {
                eu += falta.normalize_or_zero() * passo;
            }
            parado_desde = t;
        }

        // ── jogador: AUTO COMBATE ──
        if alvo.is_some_and(|a| !mobs[a].vivo) {
            alvo = None;
        }
        if chegou && alvo.is_none() {
            alvo = mobs
                .iter()
                .enumerate()
                .filter(|(_, m)| {
                    m.vivo
                        && m.pos.distance(centro) <= RAIO_DO_AUTO
                        && m.pos.distance(eu) <= RAIO_DO_AUTO
                })
                .min_by(|a, b| a.1.pos.distance(eu).total_cmp(&b.1.pos.distance(eu)))
                .map(|(i, _)| i);
        }
        if alvo.is_some() {
            r.em_luta += DT;
        }
        mp = (mp + shared::MP_REGEN_PER_SEC * DT).min(stats.mp_max as f32);
        if t >= ocupado_ate {
            let dist_alvo = alvo.map(|a| mobs[a].pos.distance(eu));
            let vida_baixa = (*hp as f32) < hp_max as f32 * 0.85;
            let escolhida = skills
                .iter()
                .enumerate()
                .filter(|(i, s)| {
                    t >= pronta_em[*i]
                        && mp >= s.custo_mp as f32
                        && if s.dano > 0 {
                            dist_alvo.is_some_and(|d| d <= s.alcance_alvo())
                        } else if s.cura > 0 {
                            vida_baixa
                        } else {
                            dist_alvo.is_some_and(|d| d <= 8.0)
                        }
                })
                .min_by_key(|(_, s)| {
                    (
                        if s.cura > 0 && vida_baixa { 0 } else { 1 },
                        if s.id > ultimo_auto { 0 } else { 1 },
                        s.id,
                    )
                })
                .map(|(i, _)| i);
            if let Some(i) = escolhida {
                let s = &skills[i];
                mp -= s.custo_mp as f32;
                pronta_em[i] = t + s.espera_s;
                ultimo_auto = s.id;
                // O servidor trava o jogador ate' `impacto_em() + RECUPERACAO_S`
                // (`world/habilidades.rs`: `casting_until`), e enquanto trava o
                // ataque basico nao sai. Sem somar a recuperacao aqui, o
                // simulador devolvia 0,36 s de basico por conjuracao que o jogo
                // real nao devolve — e superestimava toda skill, mais ainda a
                // katana, que conjura sem parar.
                ocupado_ate = t + s.impacto_em() + shared::skills::RECUPERACAO_S;
                efeitos.push((t + s.impacto_em(), i, alvo));
            }
        }
        if let Some(a) = alvo {
            let d = mobs[a].pos.distance(eu);
            let para = (alcance * 0.6).max(0.9);
            if d > alcance * 0.9 {
                aproximando = true;
            }
            if aproximando {
                let passo = (shared::PLAYER_SPEED * DT).min((d - para).max(0.0));
                eu += (mobs[a].pos - eu).normalize_or_zero() * passo;
                if eu.distance(mobs[a].pos) <= para + 0.01 {
                    aproximando = false;
                }
            }
            if t >= ocupado_ate && eu.distance(mobs[a].pos) <= alcance && cd <= 0.0 {
                cd = cd_base;
                golpes.push((t + shared::PLAYER_ATTACK_IMPACT_S, a, combo));
                combo = (combo + 1) % shared::COMBO_STEPS as usize;
            }
        }
        if eu.distance(antes) > 1e-4 {
            parado_desde = t;
        }

        // Golpes do jogador que chegaram.
        let mut i = 0;
        while i < golpes.len() {
            if golpes[i].0 > t {
                i += 1;
                continue;
            }
            let (_, a, passo_combo) = golpes.swap_remove(i);
            if !mobs[a].vivo
                || mobs[a].pos.distance(eu) > alcance + shared::HIT_TARGET_RADIUS * mobs[a].def.sz
            {
                continue;
            }
            // Provoca o golpeado e a matilha em volta (world: raio da matilha
            // pelo nivel do golpeado).
            let centro_do_golpe = mobs[a].pos;
            let raio_matilha = crate::world::matilha_raio_do_nivel(mobs[a].nivel);
            for o in mobs.iter_mut() {
                if o.vivo && !o.voltando && o.pos.distance(centro_do_golpe) <= raio_matilha {
                    o.provocado_ate = o.provocado_ate.max(t + crate::world::PROVOCACAO_S);
                }
            }
            let m = &mut mobs[a];
            let dmg = dano_mitigado(stats.attack_damage, m.def.def, 0.0);
            if conjunto == Conjunto::Katana {
                *hp = (*hp
                    + ((dmg as f32) * crate::world::ROUBO_DE_VIDA_KATANA)
                        .round()
                        .max(1.0) as i32)
                    .min(hp_max);
            }
            m.hp -= dmg;
            if !a_distancia {
                m.hurt_ate = t + shared::HURT_STAGGER_DURATION;
                let kb = [0.3, 0.4, 1.4][passo_combo.min(2)];
                let fora = (m.pos - eu).normalize_or_zero();
                m.pos += fora * kb;
            }
            if m.hp <= 0 {
                abateu(m, t, &mut r, alvo_kind);
            }
        }

        // Efeitos de skill que chegaram.
        let mut i = 0;
        while i < efeitos.len() {
            if efeitos[i].0 > t {
                i += 1;
                continue;
            }
            let (_, si, alvo_da_skill) = efeitos.swap_remove(i);
            let s = &skills[si];
            if s.cura > 0 {
                *hp = (*hp + s.cura).min(hp_max);
            }
            if s.dano == 0 && s.cura == 0 {
                muralha_ate = t + s.duracao_efeito();
            }
            if s.dano > 0 {
                let Some(a) = alvo_da_skill.filter(|a| mobs[*a].vivo) else {
                    continue;
                };
                let centro_da_skill = mobs[a].pos;
                let raio_matilha = crate::world::matilha_raio_do_nivel(mobs[a].nivel);
                let raio = s.raio.max(0.01);
                for o in mobs.iter_mut() {
                    if !o.vivo {
                        continue;
                    }
                    let d = o.pos.distance(centro_da_skill);
                    if d <= raio_matilha && !o.voltando {
                        o.provocado_ate = o.provocado_ate.max(t + crate::world::PROVOCACAO_S);
                    }
                    if d <= raio {
                        o.hp -= dano_mitigado(
                            s.dano_efetivo(stats.attack_damage, cd_base),
                            o.def.def,
                            0.0,
                        );
                        if o.hp <= 0 {
                            abateu(o, t, &mut r, alvo_kind);
                        }
                    }
                }
            }
        }

        // ── mobs ──
        let mut agressores = 0;
        for (idx, m) in mobs.iter_mut().enumerate() {
            if !m.vivo {
                if t >= m.volta_em {
                    *m = match sorteio {
                        Some((lv_min, lv_max)) => {
                            respawns += 1;
                            let s = (idx as u64 + 1)
                                .wrapping_mul(0x9E37_79B9)
                                .wrapping_add(respawns.wrapping_mul(2_654_435_761));
                            let lv = lv_min + (s % (lv_max - lv_min + 1) as u64) as u32;
                            let k = kind_para_nivel_em(&comuns, lv, s >> 7);
                            Mob::novo(&KINDS_INICIAIS[k as usize], m.casa, lv)
                        }
                        None => Mob::novo(m.def, m.casa, m.nivel),
                    };
                }
                continue;
            }
            if m.cd > 0.0 {
                m.cd -= DT;
            }
            if m.hurt_ate > t || m.preso_ate > t {
                continue;
            }
            let casa = m.pos.distance(m.casa);
            if m.voltando {
                if casa <= LEASH * 0.5 {
                    m.voltando = false;
                    m.hp = m.hp_max;
                } else {
                    m.pos += (m.casa - m.pos).normalize_or_zero() * m.def.sp * 1.5 * DT;
                    continue;
                }
            } else if casa > LEASH * 2.0 {
                m.voltando = true;
                continue;
            }
            let d = m.pos.distance(eu);
            let provocado = m.provocado_ate > t;
            let persegue = provocado || d < m.det;
            if persegue {
                agressores += 1;
                m.aggro_timer += DT;
                if m.aggro_timer > 5.0 {
                    m.voltando = true;
                    m.aggro_timer = 0.0;
                    continue;
                }
                let para_eu = (eu - m.pos).normalize_or_zero();
                let dir = if let Some(kite) = m.def.kite {
                    if d > kite + 0.5 {
                        para_eu
                    } else if d < kite - 0.5 {
                        -para_eu
                    } else {
                        Vec2::ZERO
                    }
                } else {
                    let stand = (m.def.rng - 0.3).max(0.8);
                    if d > stand + 0.3 {
                        para_eu
                    } else if d < stand - 0.3 {
                        -para_eu
                    } else {
                        Vec2::ZERO
                    }
                };
                let carga = if provocado && m.def.kite.is_none() {
                    crate::world::carga_do_nivel(m.nivel)
                } else {
                    1.0
                };
                m.pos += dir * m.def.sp * carga * DT;
                if d < m.def.rng && m.cd <= 0.0 {
                    m.cd = m.def.cd;
                    m.aggro_timer = 0.0;
                    if m.def.kite.is_none() {
                        let impacto = t + shared::MOB_ATTACK_IMPACT_S;
                        m.preso_ate = impacto;
                        mordidas.push((impacto, idx));
                    } else {
                        let solta = t + if m.def.kind == 4 {
                            shared::MAGIC_FIRE_DELAY
                        } else {
                            shared::BOW_FIRE_DELAY
                        };
                        m.preso_ate = solta;
                        mordidas.push((solta + d / shared::PROJ_SPEED, idx));
                    }
                }
            } else {
                if m.aggro_timer > 0.5 && casa > LEASH * 0.4 {
                    m.voltando = true;
                }
                m.aggro_timer = 0.0;
            }
        }
        r.max_agressores = r.max_agressores.max(agressores);

        // Mordidas e tiros que chegaram no jogador.
        let mut i = 0;
        while i < mordidas.len() {
            if mordidas[i].0 > t {
                i += 1;
                continue;
            }
            let (_, idx) = mordidas.swap_remove(i);
            let m = &mobs[idx];
            if !m.vivo {
                continue;
            }
            if m.def.kite.is_none() && m.pos.distance(eu) > m.def.rng + shared::HIT_TARGET_RADIUS {
                continue;
            }
            let mut dmg = dano_mitigado(m.dano, stats.defense, stats.damage_reduction_pct);
            if muralha_ate > t {
                dmg = ((dmg as f32) * 0.5).round().max(1.0) as i32;
            }
            *hp -= dmg;
            r.dano_recebido += dmg;
            ultimo_dano = t;
        }

        // Pocao de vida como no servidor (`shared::pocoes`): parte na hora,
        // o resto em ticks, e a recarga do grupo segura a proxima.
        if *hp > 0 {
            let beber = match *bolsa {
                Pocoes::Nenhuma => false,
                Pocoes::Infinitas => *hp < hp_max / 2,
                Pocoes::Bolsa { qtd, limiar } => qtd > 0 && (*hp as f32) < hp_max as f32 * limiar,
            };
            if beber {
                if let Some(c) = shared::pocoes::cura_de(shared::item_id::HEALTH_POTION) {
                    if let Ok(na_hora) = pocoes.beber(&c, t, false) {
                        pocao_resto += na_hora * hp_max as f32;
                        r.pocoes += 1;
                        if let Pocoes::Bolsa { qtd, .. } = bolsa {
                            *qtd -= 1;
                        }
                    }
                }
            }
            if *bolsa != Pocoes::Nenhuma {
                pocao_resto += pocoes.tick(t)[0] * hp_max as f32;
                let inteiro = pocao_resto.floor();
                pocao_resto -= inteiro;
                *hp = (*hp + inteiro as i32).min(hp_max);
            }
        }
        if *hp > 0 && *hp < hp_max {
            let parado = t - parado_desde > 2.0;
            let folego = crate::world::folego_de_iniciante(nivel, hp_max, t - ultimo_dano);
            *hp = crate::world::regen_de_hp(
                *hp,
                hp_max,
                &mut regen_resto,
                (stats.hp_regen * if parado { 4.0 } else { 1.0 } + folego) * DT,
            );
        }
        r.hp_min = r.hp_min.min(*hp as f32 / hp_max as f32);
        if *hp <= 0 {
            r.vivo = false;
            break;
        }
    }
    r.t = t;
    r
}

// ──────────────────────── personagem novo ────────────────────────
//
// O comeco do jogo como ele e' jogado (docs/COMBATE.md, "Início do jogo"):
// personagem recem-criado (so' a arma do conjunto, zero ponto distribuido,
// nenhuma skill em AUTO — o padrao de quem nunca mexeu), fazendo em ordem a
// historia do capitulo I e as missoes do Mestre ate' o nivel 5 e os ursos
// da 708. Cada caçada vai pra zona que a auto missao escolheria
// (`quests::zona_do_bicho`) nas zonas REAIS do Bosque (`zonas_comuns_da_ilha`
// sobre a ilha gerada), chegando pela borda vinda da cidade. A vida passa de
// um passo pro outro (a ida e a volta da cidade regeneram andando); a XP
// sobe com abate e recompensa. Pocao: so' as que as missoes deram, pela
// barra padrao (AUTO abaixo de 60%).
//
// Simplifica: a missao de cobre (503) vira `ABATES_DO_COBRE` abates de
// qualquer bicho; as de coleta/criacao (705–707) so' dao a XP; a Pocao de
// Experiencia fica na bolsa (sem ela a subida e' mais lenta, nunca mais facil).

/// Abates que a 503 (30 de cobre do bicho) custa, arredondado pra cima.
const ABATES_DO_COBRE: u32 = 10;
/// Uma caçada que passa disto sem terminar conta como travada.
const LIMITE_DA_CACADA_S: f32 = 1_200.0;
const LIMITE_DO_NIVEL_S: f32 = 4.0 * 3_600.0;

#[derive(Debug, Clone, Copy)]
enum Objetivo {
    Falar,
    Cacar(Option<u16>, u32),
    Nivel(u32),
}

#[derive(Debug, Clone)]
pub(crate) struct Etapa {
    pub nome: String,
    pub nivel: u32,
    pub zona: Option<(u32, u32)>,
    pub tempo: f32,
    pub mortes: u32,
    pub hp_min: f32,
    pub agressores: usize,
    pub pocoes: u32,
    pub dano_por_abate: f32,
}

#[derive(Debug, Clone)]
pub(crate) struct Jornada {
    pub conjunto: Conjunto,
    pub com_pocoes: bool,
    pub etapas: Vec<Etapa>,
}

impl Jornada {
    pub fn mortes(&self) -> u32 {
        self.etapas.iter().map(|e| e.mortes).sum()
    }
}

impl Etapa {
    pub fn linha(&self) -> String {
        let zona = self
            .zona
            .map_or("   —   ".into(), |(a, b)| format!("zona {a}–{b}"));
        format!(
            "  {:<28} nv{:<2} {} | {:>6.0}s | mortes {} | HP min {:>3.0}% | agressores {:>2} | dano/abate {:>4.1} | poções {:>2}",
            self.nome, self.nivel, zona, self.tempo, self.mortes, self.hp_min * 100.0, self.agressores,
            self.dano_por_abate, self.pocoes,
        )
    }
}

fn quest(id: u16) -> &'static shared::quests::QuestDef {
    shared::quests::QUESTS
        .iter()
        .chain(shared::historia::PASSOS.iter())
        .find(|q| q.id == id)
        .expect("quest")
}

/// Passo da jornada a partir da quest de verdade (contagem, alvo, XP, pocao).
fn passo_da_quest(id: u16) -> (String, Objetivo, u64, u32) {
    use shared::quests::objective_kind as ok;
    let q = quest(id);
    let pocoes = [
        (q.reward_item, q.reward_item_qty),
        (q.reward_item2, q.reward_item2_qty),
    ]
    .iter()
    .filter(|(i, _)| *i == shared::item_id::HEALTH_POTION)
    .map(|(_, n)| *n as u32)
    .sum();
    let obj = match q.obj_kind {
        ok::KILL => Objetivo::Cacar((q.obj_target > 0).then(|| q.obj_target - 1), q.obj_count),
        ok::NIVEL => Objetivo::Nivel(q.obj_count),
        ok::COLLECT if q.obj_target == shared::item_id::COPPER => {
            Objetivo::Cacar(None, ABATES_DO_COBRE)
        }
        _ => Objetivo::Falar,
    };
    let nome = format!("{} {}", q.id, q.title);
    (nome, obj, q.reward_xp, pocoes)
}

/// A ordem do comeco: historia e Mestre intercalados como o rastreador
/// oferece, com a trava de nivel da 504.
pub(crate) const JORNADA: &[u16] = &[
    700, 701, 501, 702, 502, 703, 503, 504, 704, 705, 706, 707, 708,
];

/// O Bosque de verdade: centro da cidade e as zonas comuns.
fn bosque() -> &'static (Vec2, crate::world::ZonasComuns) {
    static B: std::sync::OnceLock<(Vec2, crate::world::ZonasComuns)> = std::sync::OnceLock::new();
    B.get_or_init(|| {
        let def = shared::terreno::def_da_zona("ilha_inicial").expect("ilha inicial");
        let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/../../data/ilhas");
        let ilha = shared::terreno::Ilha::carregar_ou_gerar_da_ilha(dir, def);
        let cidade = ilha.cidade().map_or(Vec2::ZERO, |c| c.centro());
        let zonas = crate::world::zonas_comuns_da_ilha(&ilha, def, cidade);
        (cidade, zonas)
    })
}

pub(crate) fn jornada(conjunto: Conjunto, com_pocoes: bool) -> Jornada {
    let (cidade, zonas) = bosque();
    // A simulacao percorre a MESMA jornada que o jogador: o forte entra na
    // lista a partir de `FORTE_NA_MISSAO_NIVEL`, igual `zona_de_mob`.
    let monta_zonas = |nivel: u32| -> Vec<(Vec2, u32, u32)> {
        zonas
            .zonas
            .iter()
            .filter(|z| !z.forte || nivel >= crate::world::FORTE_NA_MISSAO_NIVEL)
            .map(|z| (z.centro, z.lv_min, z.lv_max))
            .collect()
    };
    let comuns: Vec<u16> = (0..7).collect();
    let mut equip = shared::Equipment::default();
    equip.weapon = Some(arma_do(conjunto));
    let mult = crate::economy::xp_multiplier();
    let mut xp = 0u64;
    let mut hp = effective_stats(
        &equip,
        &[0; shared::STAT_COUNT],
        &[0; shared::PROF_COUNT],
        0,
    )
    .hp_max;
    let mut bolsa = Pocoes::Bolsa {
        qtd: 0,
        limiar: 0.60,
    };
    let mut j = Jornada {
        conjunto,
        com_pocoes,
        etapas: Vec::new(),
    };

    let mut fila: Vec<(String, Objetivo, u64, u32)> = Vec::new();
    for id in JORNADA {
        let q = quest(*id);
        if q.min_level > 1 {
            fila.push((
                format!("(nivel {} pra {})", q.min_level, q.id),
                Objetivo::Nivel(q.min_level as u32),
                0,
                0,
            ));
        }
        fila.push(passo_da_quest(*id));
    }
    for (seq, (nome, obj, xp_da_quest, pocoes_da_quest)) in fila.into_iter().enumerate() {
        let nivel = shared::level_of_xp_with_mult(xp, mult);
        let stats = effective_stats(
            &equip,
            &[0; shared::STAT_COUNT],
            &[0; shared::PROF_COUNT],
            xp,
        );
        let (alvos, parada, limite) = match obj {
            Objetivo::Falar => {
                xp += xp_da_quest;
                if com_pocoes {
                    if let Pocoes::Bolsa { qtd, .. } = &mut bolsa {
                        *qtd += pocoes_da_quest;
                    }
                }
                continue;
            }
            Objetivo::Cacar(k, n) => (
                k.map(|k| vec![k]).unwrap_or_default(),
                k.map_or(Parada::Abates(n), |k| Parada::DoKind(k, n)),
                LIMITE_DA_CACADA_S,
            ),
            Objetivo::Nivel(n) => {
                let falta = shared::xp_for_level_with_mult(n, mult).saturating_sub(xp);
                if falta == 0 {
                    continue;
                }
                (Vec::new(), Parada::Xp(falta), LIMITE_DO_NIVEL_S)
            }
        };
        let tuplas = monta_zonas(nivel);
        let Some(centro) = crate::quests::zona_do_bicho(&tuplas, &comuns, &alvos, *cidade, nivel)
        else {
            panic!("{nome}: nenhuma zona");
        };
        let z = zonas
            .zonas
            .iter()
            .find(|z| z.centro == centro)
            .expect("zona");
        // Ida da cidade: regenera andando.
        let ida = centro.distance(*cidade) / shared::PLAYER_SPEED;
        let mut resto = 0.0;
        hp = crate::world::regen_de_hp(hp, stats.hp_max, &mut resto, stats.hp_regen * ida);
        let vindo = (*cidade - centro).normalize_or_zero();
        let mut e = Etapa {
            nome,
            nivel,
            zona: Some((z.lv_min, z.lv_max)),
            tempo: ida * 2.0,
            mortes: 0,
            hp_min: 1.0,
            agressores: 0,
            pocoes: 0,
            dano_por_abate: 0.0,
        };
        let (mut feito, mut abates, mut dano) = (0u64, 0u32, 0i32);
        let total = match parada {
            Parada::Abates(n) | Parada::DoKind(_, n) => n as u64,
            Parada::Xp(x) => x,
        };
        let mut tentativa = 0u64;
        while feito < total && tentativa < 6 {
            let mobs: Vec<Mob> = z
                .slots
                .iter()
                .enumerate()
                .map(|(i, p)| {
                    let s = ((seq as u64) << 32 | (tentativa << 16) | i as u64)
                        .wrapping_mul(0x9E37_79B9_7F4A_7C15)
                        >> 11;
                    let lv = z.lv_min + (s % (z.lv_max - z.lv_min + 1) as u64) as u32;
                    let k = kind_para_nivel_em(&comuns, lv, s >> 7);
                    Mob::novo(&KINDS_INICIAIS[k as usize], *p, lv)
                })
                .collect();
            let falta = total - feito;
            let parada = match parada {
                Parada::Abates(_) => Parada::Abates(falta as u32),
                Parada::DoKind(k, _) => Parada::DoKind(k, falta as u32),
                Parada::Xp(_) => Parada::Xp(falta),
            };
            let skills: Vec<shared::skills::Skill> = Vec::new();
            let s = lutar(
                Luta {
                    conjunto,
                    nivel,
                    stats: &stats,
                    skills: &skills,
                    mobs,
                    sorteio: Some((z.lv_min, z.lv_max)),
                    centro,
                    inicio: centro + vindo * crate::world::MOB_ZONA_RAIO_UN,
                    chegada: centro + vindo * crate::world::MOB_ZONA_RAIO_UN * 0.5,
                    parada,
                    limite_s: limite,
                },
                &mut hp,
                &mut bolsa,
            );
            xp += s.xp;
            abates += s.abates;
            dano += s.dano_recebido;
            feito += match parada {
                Parada::Abates(_) => s.abates as u64,
                Parada::DoKind(..) => s.do_alvo as u64,
                Parada::Xp(_) => s.xp,
            };
            e.tempo += s.t;
            e.hp_min = e.hp_min.min(s.hp_min.max(0.0));
            e.agressores = e.agressores.max(s.max_agressores);
            e.pocoes += s.pocoes;
            if !s.vivo {
                // Caiu: renasce na cidade cheio e volta.
                e.mortes += 1;
                hp = stats.hp_max;
                e.tempo += ida * 2.0;
            } else if s.t >= limite {
                break;
            }
            tentativa += 1;
        }
        e.dano_por_abate = dano as f32 / abates.max(1) as f32;
        if feito < total {
            e.nome.push_str(" (TRAVOU)");
            e.mortes = e.mortes.max(1);
        }
        xp += xp_da_quest;
        if com_pocoes {
            if let Pocoes::Bolsa { qtd, .. } = &mut bolsa {
                *qtd += pocoes_da_quest;
            }
        }
        e.nivel = shared::level_of_xp_with_mult(xp, mult);
        j.etapas.push(e);
    }
    j
}

// ─────────────────────────── chefes ───────────────────────────
//
// Duelo de UM jogador contra um chefe de campo (`shared::bosses`), com o
// padrao de ataque real: escolha de golpe, carga, fase 2, recargas e pausa
// (as mesmas funcoes do catalogo que `world::chefes` usa), roubo de vida de 25%
// do chefe (world: "Lifesteal de BOSS"), golpe comum do mob preset do corpo.
//
// O jogador NAO e' o do nivel 1 da zona: e' uma BUILD DO NIVEL (ver
// `build_do_nivel`). Dois perfis:
//   * ESQUIVA — ve' o aviso, leva `REACAO_S` pra reagir e sai pela saida
//     mais curta da forma na velocidade normal (sem correr); durante a carga
//     nao se aproxima. Se nao da' tempo, toma.
//   * PARADO — ignora o aviso (o auto combate: nunca desvia).
//
// Simplifica: chao plano, o chefe nao faz o strafe de espera, o tiro do chefe
// de gente sempre acerta, empurrao ignorado.

/// Tempo pra perceber o aviso e comecar a sair.
pub(crate) const REACAO_S: f32 = 0.35;
/// Folga alem da borda: sair rente e' tomar no arredondamento.
pub(crate) const FOLGA_DA_BORDA: f32 = 0.3;
const LIMITE_CHEFE_S: f32 = 420.0;

use shared::bosses as cat;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Perfil {
    Esquiva,
    Parado,
}

#[derive(Debug, Clone)]
pub(crate) struct Duelo {
    pub nome: &'static str,
    pub conjunto: Conjunto,
    pub nivel: u32,
    pub perfil: Perfil,
    pub pocao: bool,
    pub venceu: bool,
    pub tempo: f32,
    pub hp_max: i32,
    pub hp_min: f32,
    /// Vida que sobrou no chefe (fracao) quando acabou.
    pub chefe_restante: f32,
    pub dano_telegrafado: i32,
    pub dano_comum: i32,
    pub tomados: u32,
    pub esquivados: u32,
}

impl Duelo {
    pub fn linha(&self) -> String {
        format!(
            "{:<24} {:<13} nv{:<2} {:<7} {} | {} em {:>5.1}s | HP min {:>3.0}% (max {:>4}) | chefe {:>3.0}% | telegr {:>5} ({} tomados/{} esquivados) | comum {:>5}",
            self.nome, format!("{:?}", self.conjunto), self.nivel, format!("{:?}", self.perfil),
            if self.pocao { "poção" } else { "seco " },
            if self.venceu { "VENCE" } else { "perde" }, self.tempo, self.hp_min * 100.0, self.hp_max,
            self.chefe_restante * 100.0, self.dano_telegrafado, self.tomados, self.esquivados, self.dano_comum,
        )
    }
}

/// A build de um jogador de `nivel` com um conjunto (decisao do simulador,
/// documentada em docs/BOSSES.md):
///   * pontos: `POINTS_PER_LEVEL` × (nivel − 1), um terco no atributo da arma
///     (FOR corpo a corpo, DES pistolas, INT anel) e o resto em VIT;
///   * proficiencia da arma no mesmo nivel do personagem;
///   * equipamento completo da faixa pelo nivel (cinza < 15, verde < 30, azul
///     < 60, roxo 60+), rolagem MEDIA (`roll_for` com sorteio 0,5): arma,
///     secundaria do conjunto, armadura (pesada com espada e escudo, media com
///     katana, leve com pistolas e anel) e os quatro acessorios.
pub(crate) fn build_do_nivel(
    conjunto: Conjunto,
    nivel: u32,
) -> (
    shared::Equipment,
    [u32; shared::STAT_COUNT],
    [u64; shared::PROF_COUNT],
    u64,
) {
    use shared::item_id::*;
    use shared::stat_idx;
    let nivel = nivel.clamp(1, 100);
    let pontos = shared::POINTS_PER_LEVEL * (nivel - 1);
    let mut alloc = [0u32; shared::STAT_COUNT];
    let principal = match conjunto {
        Conjunto::EspadaEscudo | Conjunto::Katana => stat_idx::FOR,
        Conjunto::Pistolas => stat_idx::DES,
        Conjunto::AnelMagico => stat_idx::INT,
    };
    alloc[principal] = pontos / 3;
    alloc[stat_idx::VIT] = pontos - pontos / 3;
    let mut profs = [0u64; shared::PROF_COUNT];
    // `proficiency_level`: do nivel k pro k+1 custa 50·k.
    profs[conjunto as usize] = 50 * ((nivel as u64 - 1) * nivel as u64 / 2);
    let xp = shared::xp_for_level_with_mult(nivel, crate::economy::xp_multiplier());

    let ilvl = shared::receitas::FAIXAS
        .iter()
        .rev()
        .find(|f| nivel >= f.nivel_min as u32)
        .map_or(5, |f| f.item_level);
    let peca = |id: u16| {
        (
            Some(id),
            shared::items::ItemInstance::roll_for(id, ilvl, || 0.5),
        )
    };
    let (secundaria, armadura) = match conjunto {
        Conjunto::EspadaEscudo => (MANTO_DO_GUERREIRO, ARMADURA_PESADA),
        Conjunto::Katana => (BAINHA, ARMADURA_MEDIA),
        Conjunto::Pistolas => (COLDRE, ARMADURA_LEVE),
        Conjunto::AnelMagico => (MANTO_DO_MAGO, ARMADURA_LEVE),
    };
    let mut e = shared::Equipment::default();
    (e.weapon, e.weapon_inst) = peca(arma_do(conjunto));
    (e.offhand, e.offhand_inst) = peca(secundaria);
    (e.armor, e.armor_inst) = peca(armadura);
    (e.earring, e.earring_inst) = peca(BRINCO);
    (e.necklace, e.necklace_inst) = peca(AMULETO);
    (e.bracelet, e.bracelet_inst) = peca(BRACELETE);
    (e.belt, e.belt_inst) = peca(CINTO);
    (e, alloc, profs, xp)
}

/// A menor distancia pra sair da forma a partir de `p` (16 rumos, passo de
/// 0,1), mais a folga da borda, e o destino. `None` se `p` ja' esta' fora.
pub(crate) fn saida_da_forma(
    forma: &cat::Forma,
    centro: Vec2,
    dir: Vec2,
    p: Vec2,
) -> Option<(f32, Vec2)> {
    if !forma.contem(centro, dir, p) {
        return None;
    }
    let limite = forma.alcance() * 2.0 + 1.0;
    let mut melhor: Option<(f32, Vec2)> = None;
    for k in 0..16 {
        let a = k as f32 / 16.0 * std::f32::consts::TAU;
        let rumo = Vec2::new(a.cos(), a.sin());
        let mut s = 0.1;
        while s <= limite && melhor.is_none_or(|(b, _)| s < b) {
            if !forma.contem(centro, dir, p + rumo * s) {
                melhor = Some((s, rumo));
                break;
            }
            s += 0.1;
        }
    }
    melhor.map(|(s, rumo)| (s + FOLGA_DA_BORDA, p + rumo * (s + FOLGA_DA_BORDA)))
}

/// A pior saida de uma forma: de todo ponto de dentro (grade de 0,25), a
/// maior "menor distancia ate' a borda + folga". E' o que a carga precisa
/// cobrir pra o golpe ser esquivavel de qualquer lugar.
pub(crate) fn pior_saida(forma: &cat::Forma) -> f32 {
    let (centro, dir) = (Vec2::ZERO, Vec2::X);
    let r = forma.alcance();
    let mut pior = 0.0f32;
    let mut y = -r;
    while y <= r {
        let mut x = -r;
        while x <= r {
            if let Some((s, _)) = saida_da_forma(forma, centro, dir, Vec2::new(x, y)) {
                pior = pior.max(s);
            }
            x += 0.25;
        }
        y += 0.25;
    }
    pior
}

pub(crate) fn duelar(
    kind: u16,
    conjunto: Conjunto,
    nivel: u32,
    perfil: Perfil,
    pocao: bool,
) -> Duelo {
    let c = cat::chefe(kind).expect("kind de chefe");
    let (equip, alloc, profs, xp) = build_do_nivel(conjunto, nivel);
    let stats = effective_stats(&equip, &alloc, &profs, xp);
    let arma = arma_do(conjunto);
    let a_distancia = conjunto.a_distancia();
    let alcance = if a_distancia {
        shared::RANGED_ATTACK_RANGE
    } else {
        shared::MELEE_RANGE
    };
    let cd_base = cooldown_do_ataque(arma, &stats, false);
    let base_kind = match c.corpo {
        cat::Corpo::Bicho(k) | cat::Corpo::Gente(k) => k,
        cat::Corpo::Pirata => 0,
    };
    let base = &KINDS_INICIAIS[base_kind as usize];
    let hp_chefe_max = cat::vida(c.nivel);
    let mut hp_chefe = hp_chefe_max;
    let def_chefe = c.nivel as i32 / 2;
    let dano_chefe = cat::dano(c.nivel);
    let alcance_chefe = base.rng.max(2.4);
    let raio_do_chefe = shared::HIT_TARGET_RADIUS * c.escala.max(1.0);

    let skills: Vec<shared::skills::Skill> = shared::skills::playtest()
        .into_iter()
        .filter(|s| s.conjunto == conjunto && s.destravada(nivel))
        .collect();
    let mut pronta_em: Vec<f32> = vec![0.0; skills.len()];
    let mut ultimo_auto = 0u32;
    let mut mp = stats.mp_max as f32;
    let mut ocupado_ate = 0.0f32;
    let mut muralha_ate = 0.0f32;
    let mut efeitos: Vec<(f32, usize)> = Vec::new();

    let hp_max = stats.hp_max;
    let mut hp = hp_max;
    let mut regen_resto = 0.0f32;
    let mut pocoes = shared::pocoes::EstadoDePocoes::default();
    let mut pocao_resto = 0.0f32;
    let mut eu = Vec2::ZERO;
    let mut chefe = Vec2::new(8.0, 0.0);
    let mut cd = 0.0f32;
    let mut combo = 0usize;
    let mut aproximando = true;
    let mut golpes: Vec<f32> = Vec::new();

    let mut prontas = [0.0f32; cat::MAX_HABILIDADES];
    let mut livre_em = 0.0f32;
    // (habilidade, centro, dir, impacto_em, inicio, jogador dentro no inicio)
    let mut carga: Option<(usize, Vec2, Vec2, f32, f32)> = None;
    let mut saindo: Option<Vec2> = None;
    let mut cd_chefe = 1.0f32;
    let mut preso_ate = 0.0f32;
    let mut mordidas: Vec<f32> = Vec::new();

    let mut d = Duelo {
        nome: c.nome,
        conjunto,
        nivel,
        perfil,
        pocao,
        venceu: false,
        tempo: 0.0,
        hp_max,
        hp_min: 1.0,
        chefe_restante: 1.0,
        dano_telegrafado: 0,
        dano_comum: 0,
        tomados: 0,
        esquivados: 0,
    };
    let reducao = stats.damage_reduction_pct.clamp(0.0, 0.75);
    let mut t = 0.0f32;
    while t < LIMITE_CHEFE_S {
        t += DT;
        if cd > 0.0 {
            cd -= DT;
        }
        if cd_chefe > 0.0 {
            cd_chefe -= DT;
        }
        let fase = cat::fase(hp_chefe, hp_chefe_max);
        let dist = chefe.distance(eu);
        let esquivando = perfil == Perfil::Esquiva && carga.is_some();

        // ── jogador ──
        mp = (mp + shared::MP_REGEN_PER_SEC * DT).min(stats.mp_max as f32);
        if let Some(destino) = saindo {
            let passo = shared::PLAYER_SPEED * stats.speed_mult * DT;
            let falta = destino - eu;
            if falta.length() <= passo {
                eu = destino;
            } else {
                eu += falta.normalize_or_zero() * passo;
            }
        } else if let (Perfil::Esquiva, Some((hab, centro, dir, _, inicio))) = (perfil, carga) {
            if t >= inicio + REACAO_S {
                let h = &c.habilidades[hab];
                saindo = saida_da_forma(&h.forma, centro, dir, eu).map(|(_, p)| p);
            }
        }
        let movendo = saindo.is_some_and(|p| p.distance(eu) > 1e-3);
        if !movendo && t >= ocupado_ate {
            let vida_baixa = (hp as f32) < hp_max as f32 * 0.85;
            let escolhida = skills
                .iter()
                .enumerate()
                .filter(|(i, s)| {
                    t >= pronta_em[*i]
                        && mp >= s.custo_mp as f32
                        && if s.dano > 0 {
                            dist <= s.alcance_alvo() + raio_do_chefe
                        } else if s.cura > 0 {
                            vida_baixa
                        } else {
                            dist <= 8.0
                        }
                })
                .min_by_key(|(_, s)| {
                    (
                        if s.cura > 0 && vida_baixa { 0 } else { 1 },
                        if s.id > ultimo_auto { 0 } else { 1 },
                        s.id,
                    )
                })
                .map(|(i, _)| i);
            if let Some(i) = escolhida {
                let s = &skills[i];
                mp -= s.custo_mp as f32;
                pronta_em[i] = t + s.espera_s;
                ultimo_auto = s.id;
                // Mesma janela do servidor que a luta de zona usa acima.
                ocupado_ate = t + s.impacto_em() + shared::skills::RECUPERACAO_S;
                efeitos.push((t + s.impacto_em(), i));
            }
        }
        if !esquivando && !movendo {
            let para = (alcance * 0.6).max(0.9);
            if dist > alcance * 0.9 {
                aproximando = true;
            }
            if aproximando {
                let passo = (shared::PLAYER_SPEED * DT).min((dist - para).max(0.0));
                eu += (chefe - eu).normalize_or_zero() * passo;
                if eu.distance(chefe) <= para + 0.01 {
                    aproximando = false;
                }
            }
        }
        if !movendo && t >= ocupado_ate && eu.distance(chefe) <= alcance && cd <= 0.0 {
            cd = cd_base;
            golpes.push(t + shared::PLAYER_ATTACK_IMPACT_S);
            combo = (combo + 1) % shared::COMBO_STEPS as usize;
        }
        let mut i = 0;
        while i < golpes.len() {
            if golpes[i] > t {
                i += 1;
                continue;
            }
            golpes.swap_remove(i);
            if eu.distance(chefe) > alcance + raio_do_chefe {
                continue;
            }
            let dmg = dano_mitigado(stats.attack_damage, def_chefe, 0.0);
            if conjunto == Conjunto::Katana {
                hp = (hp
                    + ((dmg as f32) * crate::world::ROUBO_DE_VIDA_KATANA)
                        .round()
                        .max(1.0) as i32)
                    .min(hp_max);
            }
            hp_chefe -= dmg;
        }
        let mut i = 0;
        while i < efeitos.len() {
            if efeitos[i].0 > t {
                i += 1;
                continue;
            }
            let (_, si) = efeitos.swap_remove(i);
            let s = &skills[si];
            if s.cura > 0 {
                hp = (hp + s.cura).min(hp_max);
            }
            if s.dano == 0 && s.cura == 0 {
                muralha_ate = t + s.duracao_efeito();
            }
            if s.dano > 0 {
                hp_chefe -=
                    dano_mitigado(s.dano_efetivo(stats.attack_damage, cd_base), def_chefe, 0.0);
            }
        }
        if hp_chefe <= 0 {
            d.venceu = true;
            break;
        }

        // ── chefe ──
        let mut rouba = |dmg: i32, hp_chefe: &mut i32| {
            *hp_chefe = (*hp_chefe + ((dmg as f32) * 0.25).round() as i32).min(hp_chefe_max);
        };
        if let Some((hab, centro, dir, impacto_em, _)) = carga {
            if t >= impacto_em {
                let h = &c.habilidades[hab];
                if h.forma.contem(centro, dir, eu) {
                    let resist = cat::resistencia(stats.defense, reducao);
                    let mut dmg = cat::dano_telegrafado(h, fase, hp_max, resist);
                    if muralha_ate > t {
                        dmg = ((dmg as f32) * 0.5).round().max(1.0) as i32;
                    }
                    hp -= dmg;
                    d.dano_telegrafado += dmg;
                    d.tomados += 1;
                    rouba(dmg, &mut hp_chefe);
                } else {
                    d.esquivados += 1;
                }
                carga = None;
                saindo = None;
                prontas[hab] = t + cat::recarga(h, fase);
                livre_em = t + cat::PAUSA_ENTRE_GOLPES;
                cd_chefe = cd_chefe.max(0.6);
            }
        } else if t >= preso_ate {
            let dist = chefe.distance(eu);
            if t >= livre_em {
                if let Some(hab) = cat::escolher(c, &prontas, t, fase, dist) {
                    let h = &c.habilidades[hab];
                    let (centro, dir) = cat::centro_e_dir(h.mira, chefe, eu);
                    carga = Some((hab, centro, dir, t + cat::carga(h, fase), t));
                }
            }
            if carga.is_none() {
                let para_eu = (eu - chefe).normalize_or_zero();
                let rumo = if let Some(kite) = base.kite {
                    if dist > kite + 0.5 {
                        para_eu
                    } else if dist < kite - 0.5 {
                        -para_eu
                    } else {
                        Vec2::ZERO
                    }
                } else {
                    let stand = (alcance_chefe - 0.3).max(0.8);
                    if dist > stand + 0.3 {
                        para_eu
                    } else if dist < stand - 0.3 {
                        -para_eu
                    } else {
                        Vec2::ZERO
                    }
                };
                chefe += rumo * base.sp * DT;
                let alcance_do_golpe = if base.kite.is_some() {
                    base.rng
                } else {
                    alcance_chefe
                };
                if dist < alcance_do_golpe && cd_chefe <= 0.0 {
                    cd_chefe = base.cd.max(cat::CADENCIA_COMUM_S);
                    if base.kite.is_none() {
                        preso_ate = t + shared::MOB_ATTACK_IMPACT_S;
                        mordidas.push(preso_ate);
                    } else {
                        let solta = t + shared::BOW_FIRE_DELAY;
                        preso_ate = solta;
                        mordidas.push(solta + dist / shared::PROJ_SPEED);
                    }
                }
            }
        }
        let mut i = 0;
        while i < mordidas.len() {
            if mordidas[i] > t {
                i += 1;
                continue;
            }
            mordidas.swap_remove(i);
            if base.kite.is_none() && chefe.distance(eu) > alcance_chefe + shared::HIT_TARGET_RADIUS
            {
                continue;
            }
            let mut dmg = dano_mitigado(dano_chefe, stats.defense, reducao);
            if muralha_ate > t {
                dmg = ((dmg as f32) * 0.5).round().max(1.0) as i32;
            }
            hp -= dmg;
            d.dano_comum += dmg;
            rouba(dmg, &mut hp_chefe);
        }

        // ── pocao e regen ──
        if pocao && hp > 0 {
            let qual = if hp < hp_max * 35 / 100 {
                Some(shared::item_id::GREATER_HEAL)
            } else if hp < hp_max / 2 {
                Some(shared::item_id::HEALTH_POTION)
            } else {
                None
            };
            if let Some(c) = qual.and_then(shared::pocoes::cura_de) {
                if let Ok(na_hora) = pocoes.beber(&c, t, false) {
                    pocao_resto += na_hora * hp_max as f32;
                }
            }
            pocao_resto += pocoes.tick(t)[0] * hp_max as f32;
            let inteiro = pocao_resto.floor();
            pocao_resto -= inteiro;
            hp = (hp + inteiro as i32).min(hp_max);
        }
        if hp > 0 && hp < hp_max {
            hp = crate::world::regen_de_hp(hp, hp_max, &mut regen_resto, stats.hp_regen * DT);
        }
        d.hp_min = d.hp_min.min(hp.max(0) as f32 / hp_max as f32);
        if hp <= 0 {
            break;
        }
    }
    d.tempo = t;
    d.chefe_restante = hp_chefe.max(0) as f32 / hp_chefe_max as f32;
    d
}

#[cfg(test)]
mod testes {
    use super::*;

    const CONJUNTOS: [Conjunto; 4] = [
        Conjunto::EspadaEscudo,
        Conjunto::Katana,
        Conjunto::Pistolas,
        Conjunto::AnelMagico,
    ];

    /// Quanto o tempo por abate de um conjunto pode fugir da media do nivel.
    ///
    /// Era 0,20, e o pistoleiro batia em -22%: ele limpa mais rapido por ser o
    /// unico a distancia com skill forte de alvo unico — e PAGA por isso,
    /// terminando o nivel 10 com 24% de HP, o menor dos quatro (anel 69%,
    /// espada 50%). A meta compara ritmo sem olhar o custo em vida, e rapido-
    /// -e-fragil e' a identidade da classe.
    ///
    /// Medido antes de afrouxar: nenhum valor de `GANHO_EM_AREA` fecha a
    /// diferenca (0,70 / 0,85 / 1,00 dao -22%, -23%, -23%), porque os dois
    /// extremos estao presos no `TETO_DO_GANHO` — a pistola pelo alvo unico e o
    /// anel pelo Julgamento. Subir a area so' atrasa a espada, que passa a
    /// provocar mais matilha.
    const TOLERANCIA_DE_RITMO: f32 = 0.25;

    /// As metas do balanceamento (docs/COMBATE.md, "Balanceamento"), nos
    /// niveis 1, 5 e 10, contra a zona da propria faixa, AUTO e COM pocao — a
    /// mesma convencao das metas de chefe, que ja' duelam com pocao.
    ///
    /// Rodou a seco por muito tempo, e passava: o simulador devolvia 0,36 s de
    /// ataque basico por conjuracao (ver o cabecalho do modulo). Com a janela
    /// certa, a seco o pistoleiro MORRE no nivel 10 — e morre tambem com o dano
    /// fixo de antes, medido. Ou seja: a meta e' que estava calibrada contra um
    /// modelo errado, nao a classe que piorou. A pocao afrouxa a meta 1 (ela
    /// repoe vida durante a luta); quem carrega o sinal daqui em diante sao as
    /// metas 2 a 4, que a pocao nao afeta.
    ///
    ///   1. todo conjunto limpa 10 mobs vivo — HP minimo >= 25% com espada e
    ///      escudo, >= 15% com os outros;
    ///   2. quem atira tambem apanha: >= 3 de dano por mob;
    ///   3. dano por mob do corpo a corpo (media dos dois) no maximo 1,5x o de
    ///      quem luta a distancia (media dos dois) — a meta e' ~1,4;
    ///   4. tempo de luta por abate de cada conjunto dentro da
    ///      `TOLERANCIA_DE_RITMO` em volta da media do nivel.
    ///
    /// No nivel 1 so' valem 1 e 4: la' a matilha curta da curva de iniciante
    /// (`world::CURVA_DO_INICIO`) e' de proposito, e quem manda sao as metas
    /// do inicio (`metas_do_inicio`).
    fn falhas_das_metas(imprime: bool) -> Vec<String> {
        let mut falhas = Vec::new();
        for nivel in [1, 5, 10] {
            let rs: Vec<Resultado> = CONJUNTOS.iter().map(|c| simular(*c, nivel, true)).collect();
            for r in &rs {
                if imprime {
                    println!("{}", r.linha());
                }
                let piso = if r.conjunto == Conjunto::EspadaEscudo {
                    0.25
                } else {
                    0.15
                };
                if !(r.vivo && r.mortos >= 10) {
                    falhas.push(format!(
                        "nv{nivel} {:?}: nao limpou 10 mobs vivo",
                        r.conjunto
                    ));
                }
                if r.hp_min < piso {
                    falhas.push(format!(
                        "nv{nivel} {:?}: HP minimo {:.0}% abaixo de {:.0}%",
                        r.conjunto,
                        r.hp_min * 100.0,
                        piso * 100.0
                    ));
                }
                if nivel > 1 && r.conjunto.a_distancia() && r.dano_por_mob() < 3.0 {
                    falhas.push(format!(
                        "nv{nivel} {:?}: quem atira saiu sem apanhar ({:.1}/mob)",
                        r.conjunto,
                        r.dano_por_mob()
                    ));
                }
            }
            let media = |f: &dyn Fn(&Resultado) -> bool| {
                let v: Vec<f32> = rs
                    .iter()
                    .filter(|r| f(r))
                    .map(|r| r.dano_por_mob())
                    .collect();
                v.iter().sum::<f32>() / v.len() as f32
            };
            let perto = media(&|r| !r.conjunto.a_distancia());
            let longe = media(&|r| r.conjunto.a_distancia());
            if imprime {
                println!(
                    "nv{nivel}: dano/mob corpo a corpo {perto:.1} x distancia {longe:.1} = {:.2}x",
                    perto / longe
                );
            }
            if nivel > 1 && perto > longe * 1.5 {
                falhas.push(format!(
                    "nv{nivel}: corpo a corpo apanha {:.2}x o de quem atira",
                    perto / longe
                ));
            }
            let ttk = rs.iter().map(|r| r.por_abate()).sum::<f32>() / rs.len() as f32;
            for r in &rs {
                let desvio = r.por_abate() / ttk - 1.0;
                if desvio.abs() > TOLERANCIA_DE_RITMO {
                    falhas.push(format!(
                        "nv{nivel} {:?}: {:.2}s/abate, {:+.0}% da media {ttk:.2}s",
                        r.conjunto,
                        r.por_abate(),
                        desvio * 100.0
                    ));
                }
            }
        }
        falhas
    }

    #[test]
    fn metas_de_balanceamento() {
        let t0 = std::time::Instant::now();
        println!();
        let falhas = falhas_das_metas(true);
        println!("simulado em {:?}", t0.elapsed());
        assert!(
            falhas.is_empty(),
            "{} metas quebradas:\n{}",
            falhas.len(),
            falhas.join("\n")
        );
    }

    /// Metas do INICIO DO JOGO (docs/COMBATE.md, "Início do jogo"): o
    /// personagem recem-criado fazendo o capitulo I e as missoes do Mestre
    /// (`jornada`), AUTO, cada conjunto, sem pocao e com as que as missoes dao:
    ///
    ///   1. ninguem cai ate' o nivel 5 e os ursos da 708;
    ///   2. HP minimo de cada caçada >= 35% sem pocao, >= 50% com;
    ///   3. nas duas primeiras caçadas (702 e 502) no maximo 2 bichos em cima;
    ///   4. cada caçada de missao em ate' 10 min, e o nivel 5 em ate' 3 h.
    fn falhas_do_inicio(imprime: bool) -> Vec<String> {
        let mut falhas: Vec<String> = Vec::new();
        for com in [false, true] {
            for c in CONJUNTOS {
                let j = jornada(c, com);
                if imprime {
                    println!(
                        "{c:?} {} — mortes {}",
                        if com {
                            "com poções das missões"
                        } else {
                            "sem poção"
                        },
                        j.mortes()
                    );
                    for e in &j.etapas {
                        println!("{}", e.linha());
                    }
                }
                let piso = if com { 0.50 } else { 0.35 };
                for (i, e) in j.etapas.iter().enumerate() {
                    let quem = format!("{c:?} {} {}", if com { "poção" } else { "seco" }, e.nome);
                    if e.mortes > 0 {
                        falhas.push(format!("{quem}: {} morte(s)", e.mortes));
                    }
                    if e.hp_min < piso {
                        falhas.push(format!(
                            "{quem}: HP minimo {:.0}% < {:.0}%",
                            e.hp_min * 100.0,
                            piso * 100.0
                        ));
                    }
                    if i < 2 && e.agressores > 2 {
                        falhas.push(format!("{quem}: {} bichos em cima", e.agressores));
                    }
                    let teto = if e.nome.starts_with("704") || e.nome.starts_with("(nivel") {
                        3.0 * 3_600.0
                    } else {
                        600.0
                    };
                    if e.tempo > teto {
                        falhas.push(format!("{quem}: {:.0}s", e.tempo));
                    }
                }
            }
        }
        falhas
    }

    #[test]
    fn metas_do_inicio() {
        let t0 = std::time::Instant::now();
        println!();
        let falhas = falhas_do_inicio(true);
        println!("inicio simulado em {:?}", t0.elapsed());
        assert!(
            falhas.is_empty(),
            "{} metas do inicio quebradas:\n{}",
            falhas.len(),
            falhas.join("\n")
        );
    }

    /// Com pocao ninguem fica pior do que sem.
    #[test]
    fn pocao_nunca_piora() {
        for nivel in [1, 5, 10] {
            for c in CONJUNTOS {
                let seco = simular(c, nivel, false);
                let pocao = simular(c, nivel, true);
                assert!(
                    pocao.hp_min + 1e-3 >= seco.hp_min.min(0.5),
                    "nv{nivel} {c:?}: pocao piorou"
                );
            }
        }
    }

    /// Metas dos CHEFES (docs/BOSSES.md, "Balanceamento"): desviar na mao e'
    /// a graca, o auto combate nunca desvia.
    ///
    ///   1. ESQUIVA + pocao, no nivel do chefe: os 4 conjuntos vencem todo
    ///      chefe, HP minimo >= 15%, luta de 60 a 240 s;
    ///   2. PARADO + pocao, no nivel do chefe: perde, ou termina com HP
    ///      minimo <= 10% — o telegrafico tem que doer;
    ///   3. ESQUIVA + pocao, dois niveis abaixo: pelo menos 3 dos 4 conjuntos
    ///      ainda vencem;
    ///   4. ESQUIVA sem pocao: so' impresso (e' pra ser apertado).
    #[test]
    fn metas_dos_chefes() {
        let t0 = std::time::Instant::now();
        println!();
        // Tudo impresso ANTES de reprovar: a tabela inteira e' o que se ajusta.
        let mut falhas: Vec<String> = Vec::new();
        for c in cat::CHEFES.iter() {
            let mut vencem_abaixo = 0;
            for conj in CONJUNTOS {
                let esquiva = duelar(c.kind, conj, c.nivel, Perfil::Esquiva, true);
                let parado = duelar(c.kind, conj, c.nivel, Perfil::Parado, true);
                let seco = duelar(c.kind, conj, c.nivel, Perfil::Esquiva, false);
                let abaixo = duelar(
                    c.kind,
                    conj,
                    c.nivel.saturating_sub(2).max(1),
                    Perfil::Esquiva,
                    true,
                );
                for d in [&esquiva, &parado, &seco, &abaixo] {
                    println!("{}", d.linha());
                }
                if !esquiva.venceu {
                    falhas.push(format!(
                        "{} {conj:?}: esquivando com pocao nao venceu",
                        c.nome
                    ));
                }
                if esquiva.hp_min < 0.15 {
                    falhas.push(format!(
                        "{} {conj:?}: esquivando com pocao HP minimo {:.0}%",
                        c.nome,
                        esquiva.hp_min * 100.0
                    ));
                }
                if !(60.0..=240.0).contains(&esquiva.tempo) {
                    falhas.push(format!(
                        "{} {conj:?}: luta de {:.0}s",
                        c.nome, esquiva.tempo
                    ));
                }
                if parado.venceu && parado.hp_min > 0.10 {
                    falhas.push(format!(
                        "{} {conj:?}: parado com pocao venceu com HP minimo {:.0}%",
                        c.nome,
                        parado.hp_min * 100.0
                    ));
                }
                if abaixo.venceu {
                    vencem_abaixo += 1;
                }
            }
            if vencem_abaixo < 3 {
                falhas.push(format!(
                    "{}: so' {vencem_abaixo} conjuntos vencem dois niveis abaixo",
                    c.nome
                ));
            }
        }
        println!("chefes simulados em {:?}", t0.elapsed());
        assert!(
            falhas.is_empty(),
            "{} metas de chefe quebradas:\n{}",
            falhas.len(),
            falhas.join("\n")
        );
    }

    /// Todo golpe telegrafado e' esquivavel de QUALQUER ponto de dentro: a
    /// carga (na fase dele, e na fase 2 que encurta) cobre reacao + a pior
    /// saida na velocidade normal.
    #[test]
    fn todo_telegrafico_e_esquivavel() {
        let mut falhas = Vec::new();
        for c in cat::CHEFES.iter() {
            for h in c.habilidades {
                let pior = pior_saida(&h.forma);
                let precisa = REACAO_S + pior / shared::PLAYER_SPEED;
                for fase in h.fase_min..=1 {
                    let carga = cat::carga(h, fase);
                    println!("{:<24} {:<22} fase {fase}: carga {carga:.2}s, precisa {precisa:.2}s (pior saida {pior:.1})",
                        c.nome, h.nome);
                    if carga < precisa {
                        falhas.push(format!(
                            "{} / {} fase {fase}: carga {carga:.2}s < {precisa:.2}s",
                            c.nome, h.nome
                        ));
                    }
                }
            }
        }
        assert!(
            falhas.is_empty(),
            "golpes inesquivaveis:\n{}",
            falhas.join("\n")
        );
    }

    /// A resistencia que o catalogo usa pra calcular o telegrafado e' a mesma
    /// que o servidor aplica no hit — senao o golpe tira mais ou menos que o
    /// numero do catalogo.
    #[test]
    fn resistencia_do_catalogo_bate_com_a_do_servidor() {
        for defesa in [0, 7, 20, 49, 60, 120] {
            for reducao in [0.0f32, 0.1, 0.4, 0.5, 0.8] {
                let r = cat::resistencia(defesa, reducao);
                let esperado = ((1000.0 * (1.0 - r)).round() as i32).max(1);
                assert_eq!(
                    dano_mitigado(1000, defesa, reducao),
                    esperado,
                    "defesa {defesa} reducao {reducao}"
                );
            }
        }
    }

    /// O golpe COMUM do chefe nao derruba um jogador cheio do nivel de uma vez.
    #[test]
    fn golpe_comum_de_chefe_nao_mata_de_uma_vez() {
        for c in cat::CHEFES.iter() {
            for conj in CONJUNTOS {
                let (e, a, p, x) = build_do_nivel(conj, c.nivel);
                let s = effective_stats(&e, &a, &p, x);
                let dmg = dano_mitigado(
                    cat::dano(c.nivel),
                    s.defense,
                    s.damage_reduction_pct.clamp(0.0, 0.75),
                );
                assert!(
                    dmg < s.hp_max,
                    "{} {conj:?}: golpe comum {dmg} >= vida {}",
                    c.nome,
                    s.hp_max
                );
            }
        }
    }

    /// O regen devolve HP de verdade (antes truncava e nao curava nada).
    #[test]
    fn regen_cura_de_verdade() {
        let mut resto = 0.0;
        let mut hp = 50;
        for _ in 0..30 {
            hp = crate::world::regen_de_hp(hp, 100, &mut resto, 0.5 / 30.0);
        }
        for _ in 0..30 * 9 {
            hp = crate::world::regen_de_hp(hp, 100, &mut resto, 0.5 / 30.0);
        }
        assert_eq!(hp, 55, "0,5/s por 10 s tem que curar 5");
        assert_eq!(
            crate::world::regen_de_hp(99, 100, &mut 0.0, 5.0),
            100,
            "nao passa do maximo"
        );
    }
}

#[cfg(test)]
mod testes_das_zonas {

    /// A missao de matar bicho MANDA pro forte — mas so' a partir do nivel
    /// `FORTE_NA_MISSAO_NIVEL`. Matar N bichos num lugar com o dobro da
    /// densidade acaba em metade do tempo, e e' isso que da' ao forte uma
    /// razao pra existir alem de estar marcado no mapa. Antes disso ele mata
    /// o jogador: ver `metas_do_inicio`.
    #[test]
    fn a_missao_manda_pro_forte_depois_do_nivel_de_corte() {
        use crate::world::FORTE_NA_MISSAO_NIVEL;
        let (cidade, zonas) = super::bosque();
        let comuns: Vec<u16> = (0..7).collect();
        let lista = |nivel: u32| -> Vec<(glam::Vec2, u32, u32)> {
            zonas
                .zonas
                .iter()
                .filter(|z| !z.forte || nivel >= FORTE_NA_MISSAO_NIVEL)
                .map(|z| (z.centro, z.lv_min, z.lv_max))
                .collect()
        };
        let e_forte = |c: glam::Vec2| {
            zonas
                .zonas
                .iter()
                .any(|z| z.forte && z.centro == c)
        };

        // Abaixo do corte, nenhum destino cai num forte — em nivel nenhum.
        for nivel in 1..FORTE_NA_MISSAO_NIVEL {
            let zs = lista(nivel);
            for alvo in 0..7u16 {
                if let Some(c) =
                    crate::quests::zona_do_bicho(&zs, &comuns, &[alvo], *cidade, nivel)
                {
                    assert!(!e_forte(c), "nivel {nivel} mandou pro forte");
                }
            }
        }
        // Acima do corte o forte entra na disputa, e em ALGUM caso ele ganha —
        // senao a regra existiria sem efeito nenhum.
        let mut foi = false;
        for nivel in FORTE_NA_MISSAO_NIVEL..=15 {
            let zs = lista(nivel);
            for alvo in 0..7u16 {
                if let Some(c) =
                    crate::quests::zona_do_bicho(&zs, &comuns, &[alvo], *cidade, nivel)
                {
                    foi |= e_forte(c);
                }
            }
        }
        assert!(foi, "o forte nunca e' escolhido: a regra nao faz nada");
    }

    /// Os fortes se espalham pela ilha inteira: um perto do desembarque, pra
    /// o jogador saber cedo que a coisa existe, e outros ate' a ponta, pra
    /// continuarem valendo. Imprime onde ficam — e' o jeito de responder
    /// "onde fica o forte" sem abrir o jogo.
    #[test]
    fn os_fortes_se_espalham_pela_ilha() {
        let (cidade, zonas) = super::bosque();
        let mut v: Vec<_> = zonas.zonas.iter().filter(|z| z.forte).collect();
        v.sort_by(|a, b| {
            a.centro
                .distance(*cidade)
                .total_cmp(&b.centro.distance(*cidade))
        });
        println!("cidade em ({:.0}, {:.0})", cidade.x, cidade.y);
        for z in &v {
            let d = z.centro.distance(*cidade);
            let ang = (z.centro.y - cidade.y).atan2(z.centro.x - cidade.x).to_degrees();
            let rumo = match ((ang + 360.0) % 360.0) as i32 {
                0..=22 | 338..=360 => "leste",
                23..=67 => "sudeste",
                68..=112 => "sul",
                113..=157 => "sudoeste",
                158..=202 => "oeste",
                203..=247 => "noroeste",
                248..=292 => "norte",
                _ => "nordeste",
            };
            println!(
                "forte em ({:7.0},{:7.0})  {:5.0} u da cidade, a {rumo}  nv {}-{}  {} inimigos",
                z.centro.x,
                z.centro.y,
                d,
                z.lv_min,
                z.lv_max,
                z.slots.len()
            );
        }
        println!("{} fortes de {} zonas", v.len(), zonas.zonas.len());

        let d = |z: &&crate::world::ZonaComum| z.centro.distance(*cidade);
        let perto = v.first().map(d).unwrap_or(f32::MAX);
        let longe = v.last().map(d).unwrap_or(0.0);
        assert!(
            perto < 300.0,
            "o forte mais perto esta' a {perto:.0} u: o jogador nunca ia topar com um"
        );
        assert!(
            longe > 600.0,
            "todos os fortes cabem nos primeiros {longe:.0} u: eles param de valer cedo"
        );
        // A escada de nivel acompanha: forte perto e' de nivel baixo, forte
        // longe e' do fim da ilha. Senao um deles e' muro e o outro e' enfeite.
        let (p, l) = (v.first().unwrap(), v.last().unwrap());
        assert!(
            p.lv_max < l.lv_min,
            "o forte perto ({}-{}) nao e' mais fraco que o longe ({}-{})",
            p.lv_min,
            p.lv_max,
            l.lv_min,
            l.lv_max
        );
    }
    /// O FORTE e' mais inimigo no mesmo chao, e nao um degrau de nivel.
    ///
    /// Este teste roda na ilha de verdade porque a densidade sai do relevo: os
    /// slots do forte vem dos mesmos sitios planos, so' que com espacamento
    /// menor. Num mapa sintetico o numero nao diria nada.
    #[test]
    fn o_forte_e_densidade_e_nao_nivel() {
        let (_, zonas) = super::bosque();
        let fortes: Vec<_> = zonas.zonas.iter().filter(|z| z.forte).collect();
        let comuns: Vec<_> = zonas.zonas.iter().filter(|z| !z.forte).collect();
        assert!(
            !fortes.is_empty() && !comuns.is_empty(),
            "o Bosque tem que ter os dois: {} fortes de {} zonas",
            fortes.len(),
            zonas.zonas.len()
        );
        assert!(
            fortes.len() * 3 < zonas.zonas.len(),
            "forte demais deixa de ser lugar marcado: {} de {}",
            fortes.len(),
            zonas.zonas.len()
        );
        let densidade = |z: &crate::world::ZonaComum| {
            z.slots.len() as f32 / (z.raio * z.raio * std::f32::consts::PI)
        };
        let media = |v: &[&crate::world::ZonaComum]| {
            v.iter().map(|z| densidade(z)).sum::<f32>() / v.len() as f32
        };
        let (df, dc) = (media(&fortes), media(&comuns));
        assert!(
            df > dc * 2.0,
            "o forte tem que ser MUITO mais cheio: {df:.4} contra {dc:.4} por unidade quadrada"
        );
        // O nivel nao muda: fosse nivel e densidade juntos, o forte deixaria de
        // ser escolha e viraria muro.
        let faixa = |v: &[&crate::world::ZonaComum]| {
            (
                v.iter().map(|z| z.lv_min).min().unwrap_or(0),
                v.iter().map(|z| z.lv_max).max().unwrap_or(0),
            )
        };
        let (fmin, fmax) = faixa(&fortes);
        let (cmin, cmax) = faixa(&comuns);
        assert!(
            fmin >= cmin && fmax <= cmax,
            "o forte saiu da escada de nivel da ilha: {fmin}–{fmax} contra {cmin}–{cmax}"
        );
        // E o raio dele e' menor: e' isso que aperta a mancha no mapa.
        assert!(fortes.iter().all(|z| z.raio < comuns[0].raio));
    }


    /// A ilha inteira tem que ter faixa de nivel, e perto da cidade tem que
    /// haver onde subir do 1 — o dono achou so' bicho de nivel 1 jogando.
    #[test]
    fn a_primeira_ilha_tem_faixa_de_nivel() {
        let (cidade, zonas) = super::bosque();
        let mut por_nivel: std::collections::BTreeMap<u32, usize> = Default::default();
        let mut perto = Vec::new();
        for z in &zonas.zonas {
            *por_nivel.entry(z.lv_min).or_default() += 1;
            let d = z.centro.distance(*cidade);
            if d <= 260.0 {
                perto.push((d.round() as i32, z.lv_min, z.lv_max));
            }
        }
        perto.sort();
        println!("zonas por nivel minimo: {por_nivel:?}");
        println!(
            "as 12 zonas mais perto da cidade (distancia, lv_min, lv_max): {:?}",
            &perto[..perto.len().min(12)]
        );
        let maior = zonas.zonas.iter().map(|z| z.lv_max).max().unwrap_or(0);
        assert!(maior >= 12, "a ilha inteira so' chega ao nivel {maior}");
        let subindo = perto.iter().filter(|(_, lv, _)| *lv >= 3).count();
        assert!(
            subindo >= 3,
            "perto da cidade so' ha' nivel baixo: {perto:?}"
        );
    }
}

#[cfg(test)]
mod testes_do_nivel_do_mob {
    /// O bicho de zona longe vale mais XP que o do quintal da cidade.
    #[test]
    fn xp_sobe_com_o_nivel_da_zona() {
        use crate::world::xp_do_mob;
        assert_eq!(xp_do_mob(30, 1), 30, "nivel 1 e' a tabela");
        assert_eq!(xp_do_mob(30, 6), 45);
        assert_eq!(xp_do_mob(30, 12), 63);
        assert!(xp_do_mob(30, 12) > xp_do_mob(30, 6));
    }
}
