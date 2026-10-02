//! Selects only monsters near the point where AUTO was switched on.
//! Movement, attacks and damage are still validated by the server.
use crate::{hud_estilo as estilo, world::World};
use macroquad::prelude::*;
use shared::{EntityId, EntityTag};
use std::collections::HashMap;

const RAIO: f32 = 24.0;

/// With no creature in the area: AUTO goes AFTER the nearest one up to this
/// distance. Without it, it cleared the place and stood looking at the bush —
/// the area only moved when the player walked by hand.
const BUSCA: f32 = 110.0;
/// Interval between "go over there" requests during the hunt.
const PASSO_DA_CACA_S: f64 = 1.2;

/// Still for this long after walking by hand, AUTO takes the route back over.
const VOLTA_PARADO_S: f64 = 0.4;

pub struct AutoCombate {
    pub centro: Option<Vec2>,
    observado: Option<(EntityId, f32, u32, f64)>,
    ignorados: HashMap<EntityId, f64>,
    /// Walking under your own steam (keyboard or a click on the ground). Walking
    /// does NOT switch AUTO off: the area follows the character. The aim keeps
    /// choosing targets; only the automatic route waits for the player to stop.
    manual: bool,
    ultima_pos: Option<Vec2>,
    parado_desde: f64,
    /// A target the server says has no line of sight: (id, first notice, last).
    sem_visada: Option<(EntityId, f64, f64)>,
    /// The hunt's last "go over there".
    caca_em: f64,
    /// A ordem de prioridade escolhida (`shared::protocol::auto_alvo`).
    pub ordem: Vec<u8>,
    /// How far it goes against a player (`shared::protocol::auto_pvp`).
    pub pvp: u8,
}

impl Default for AutoCombate {
    fn default() -> Self {
        Self {
            centro: None,
            observado: None,
            ignorados: HashMap::new(),
            manual: false,
            ultima_pos: None,
            parado_desde: 0.0,
            sem_visada: None,
            caca_em: 0.0,
            ordem: shared::protocol::auto_alvo::PADRAO.to_vec(),
            pvp: shared::protocol::auto_pvp::NUNCA,
        }
    }
}

/// A target candidate, already reduced to what the choice needs to know.
///
/// Outside `World` on purpose: the priority rule is the part that gets
/// things wrong, and it needs a test without building a whole world.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Candidato {
    pub id: EntityId,
    /// Distance to me.
    pub dist: f32,
    /// Is it a player (and not a creature)?
    pub jogador: bool,
    /// Is HITTING you from range right now.
    pub ranged_em_mim: bool,
    /// Hit you recently.
    pub agrediu: bool,
    /// Is the creature the active quest asks for.
    pub da_missao: bool,
}

/// How long someone who hit you keeps counting as an aggressor.
///
/// Eight seconds: time for you to hit back without a stray shot from a
/// minute ago marking a player as an enemy for the rest of the session.
pub const AGRESSOR_S: f64 = 8.0;

/// From here on, whoever hits you is hitting you FROM RANGE.
///
/// Six units: more than the reach of any hand strike. Whoever hits you from
/// further than that is shooting, whatever the species.
pub const DISTANCIA_DE_LONGE: f32 = 6.0;

/// TARGET SELECTION, by priority.
///
/// The owner: "when I'm doing a quest it has to be the quest mob, but if
/// I'm like I am now on the Magic Island I have to be able to choose: ranged
/// enemies attacking me from a distance, enemies that are close, player".
///
/// THE QUEST COMES FIRST AND IS NOT ADJUSTABLE: someone who switched auto on
/// in the middle of a hunting quest wants the quest moving, and a preference
/// that got in the way of that would be a foot in their own hunt. The rest
/// follows the chosen order.
///
/// Within each category, the nearest — and the id breaks ties, so the choice
/// does not flicker between two equals.
pub fn escolhe_alvo(cands: &[Candidato], ordem: &[u8], pvp: u8) -> Option<EntityId> {
    use shared::protocol::{auto_alvo, auto_pvp};
    let pode = |c: &Candidato| -> bool {
        if !c.jogador {
            return true;
        }
        match pvp {
            auto_pvp::QUALQUER => true,
            // HITTING BACK is not "attacking a player": it is answering whoever started.
            auto_pvp::REVIDAR => c.agrediu,
            _ => false,
        }
    };
    let melhor = |f: &dyn Fn(&Candidato) -> bool| -> Option<EntityId> {
        cands
            .iter()
            .filter(|c| pode(c) && f(c))
            .min_by(|a, b| a.dist.total_cmp(&b.dist).then(a.id.0.cmp(&b.id.0)))
            .map(|c| c.id)
    };
    // 1. The quest, always.
    if let Some(id) = melhor(&|c| c.da_missao) {
        return Some(id);
    }
    // 2. A ordem escolhida.
    for cat in ordem {
        let achado = match *cat {
            auto_alvo::RANGED_EM_MIM => melhor(&|c| c.ranged_em_mim),
            auto_alvo::JOGADOR => melhor(&|c| c.jogador),
            auto_alvo::MAIS_PERTO => melhor(&|_| true),
            _ => None,
        };
        if achado.is_some() {
            return achado;
        }
    }
    // 3. No category caught anything: the nearest one allowed. Without this,
    //    an order of only "player" with PvP off would leave auto switched on
    // hitting nothing — which, from the screen, is the same as being broken.
    melhor(&|_| true)
}

/// A "no line of sight" notice repeated for this long: AUTO drops the target.
/// The server warns once a second, so the second notice already switches.
const TROCA_SEM_VISADA_S: f64 = 0.9;
/// Longer than this with no new notice: the sequence restarts.
const SEM_VISADA_ESQUECE_S: f64 = 2.5;

/// O botao AUTO COMBATE na linha de baixo do cluster (ver `hud_layout`).
pub fn retangulo() -> Rect {
    crate::hud_layout::atual().auto_combate
}
pub fn pega_mouse() -> bool {
    retangulo().contains(Vec2::from(mouse_position()))
}

impl AutoCombate {
    pub fn ativo(&self) -> bool {
        self.centro.is_some()
    }
    pub fn dirigindo(&self) -> bool {
        self.ativo() && self.manual
    }
    pub fn ligar(&mut self, p: Vec2) {
        self.centro = Some(p);
        self.observado = None;
        self.ignorados.clear();
        self.caca_em = f64::MIN;
    }
    pub fn parar(&mut self) {
        *self = Self::default();
    }

    /// The server warned that target `id` has no line of sight
    /// (`ServerMessage::SemVisada`). With AUTO on, a repeated notice drops the
    /// target for 10 s — instead of waiting the 8 s with no damage in `escolher`.
    pub fn sem_visada(&mut self, id: EntityId, agora: f64) {
        if !self.ativo() {
            self.sem_visada = None;
            return;
        }
        match self.sem_visada {
            Some((ant, primeiro, ultimo)) if ant == id && agora - ultimo < SEM_VISADA_ESQUECE_S => {
                if agora - primeiro >= TROCA_SEM_VISADA_S {
                    self.ignorados.insert(id, agora + 10.0);
                    self.observado = None;
                    self.sem_visada = None;
                } else {
                    self.sem_visada = Some((id, primeiro, agora));
                }
            }
            _ => self.sem_visada = Some((id, agora, agora)),
        }
    }

    /// The player is walking by hand this frame.
    pub fn andar_manual(&mut self, agora: f64) {
        if self.ativo() {
            self.manual = true;
            self.parado_desde = agora;
        }
    }

    /// Ainda andando na mao? Enquanto sim, a area vem junto. A escolha de
    /// alvo continua; somente a rota de caca fica suspensa.
    pub fn segurando(&mut self, pos: Vec2, agora: f64) -> bool {
        let moveu = self.ultima_pos.is_some_and(|u| u.distance(pos) > 0.01);
        self.ultima_pos = Some(pos);
        if !self.manual {
            return false;
        }
        if moveu {
            self.parado_desde = agora;
        }
        self.centro = Some(pos);
        if agora - self.parado_desde > VOLTA_PARADO_S {
            self.manual = false;
            self.observado = None;
            return false;
        }
        true
    }

    /// No creature in the area: where to go and hunt. Moves the area to the
    /// character and returns the nearest live creature within `BUSCA` — walking
    /// there is `main`'s job. `None` when there is nothing to hunt (or it is too soon).
    pub fn caca(&mut self, world: &World, eu: Vec2, agora: f64, missao: Option<u16>) -> Option<Vec2> {
        if !self.ativo() || self.manual {
            return None;
        }
        // The area follows the character: without that, clearing the place is the end.
        self.centro = Some(eu);
        if agora - self.caca_em < PASSO_DA_CACA_S {
            return None;
        }
        let alvo = world
            .ents
            .iter()
            .filter(|(id, e)| e.meta.tag == EntityTag::Enemy && e.state.hp > 0 && e.morte.is_none()
                && !self.ignorados.get(id).is_some_and(|ate| *ate > agora))
            .map(|(_, e)| e)
            .filter(|e| e.render_pos.distance(eu) <= BUSCA)
            .min_by(|a, b| {
                let fora = |kind: u16| missao.is_some_and(|k| shared::bestiary::species_of(kind) != k);
                fora(a.meta.kind).cmp(&fora(b.meta.kind)).then_with(||
                    a.render_pos.distance_squared(eu).total_cmp(&b.render_pos.distance_squared(eu)))
            })?.render_pos;
        self.caca_em = agora;
        Some(alvo)
    }

    /// `missao` = the `kind` of creature the active quest asks for, when there is one.
    pub fn escolher(
        &mut self,
        world: &World,
        atual: Option<EntityId>,
        agora: f64,
        missao: Option<u16>,
    ) -> Option<EntityId> {
        let mut centro = self.centro?;
        let eu = world.self_id.and_then(|id| world.ents.get(&id))?;
        if eu.state.hp == 0
            || eu.state.flags & shared::ent_flags::DOWNED != 0
        {
            self.parar();
            return None;
        }
        // Chasing a mob does not switch auto off on leaving the initial area.
        if eu.render_pos.distance(centro) > RAIO + 4.0 {
            centro = eu.render_pos;
            self.centro = Some(centro);
        }
        self.ignorados.retain(|_, ate| *ate > agora);
        // Who CAN be a target. A player only enters when the setting allows —
        // `escolhe_alvo` decides again in there, but letting them in here is what
        // makes the "player" category possible.
        let aceita_gente = eu.meta.pk.hostil && self.pvp != shared::protocol::auto_pvp::NUNCA;
        let valido_alvo = |id: EntityId| {
            world.ents.get(&id).is_some_and(|e| {
                let tipo_ok = e.meta.tag == EntityTag::Enemy
                    || (aceita_gente
                        && e.meta.tag == EntityTag::Player
                        && world.self_id != Some(id));
                tipo_ok
                    && e.state.hp > 0
                    && e.morte.is_none()
                    && e.render_pos.distance(centro) <= RAIO
                    && e.render_pos.distance(eu.render_pos) <= RAIO
            })
        };
        let valido = valido_alvo;
        if let Some(id) = atual.filter(|&id| valido(id) && !self.ignorados.contains_key(&id)) {
            let e = &world.ents[&id];
            let dist = eu.render_pos.distance(e.render_pos);
            match self.observado {
                Some((ant, d, hp, t)) if ant == id => {
                    if dist < d - 0.4 || e.state.hp < hp {
                        self.observado = Some((id, dist, e.state.hp, agora));
                    } else if agora - t > 8.0 {
                        self.ignorados.insert(id, agora + 15.0);
                        self.observado = None;
                    } else {
                        return Some(id);
                    }
                }
                _ => self.observado = Some((id, dist, e.state.hp, agora)),
            }
            if !self.ignorados.contains_key(&id) {
                return Some(id);
            }
        }
        // CHOICE BY PRIORITY. It used to be just the nearest.
        let cands: Vec<Candidato> = world
            .ents
            .iter()
            .filter(|(id, _)| valido_alvo(**id) && !self.ignorados.contains_key(id))
            .map(|(id, e)| Candidato {
                id: *id,
                dist: eu.render_pos.distance(e.render_pos),
                jogador: e.meta.tag == EntityTag::Player,
                // "HITTING ME FROM RANGE" WITHOUT NEEDING TO KNOW THE SPECIES.
                //
                // I was going to read `ENEMY_SHOOT`, but it is an ANIMATION constant and
                // does not travel in the entity's state. The signal that really exists is
                // better: whoever HIT me recently and is more than an arm's length away can
                // only be shooting. That holds for any enemy the game may come to have,
                // with no table.
                ranged_em_mim: eu.render_pos.distance(e.render_pos) > DISTANCIA_DE_LONGE
                    && world
                        .agressores
                        .get(id)
                        .is_some_and(|t| agora - t < AGRESSOR_S),
                agrediu: world
                    .agressores
                    .get(id)
                    .is_some_and(|t| agora - t < AGRESSOR_S),
                da_missao: missao.is_some_and(|k| shared::bestiary::species_of(e.meta.kind) == k),
            })
            .collect();
        let escolhido = escolhe_alvo(&cands, &self.ordem, self.pvp);
        self.observado = escolhido.map(|id| {
            (
                id,
                eu.render_pos.distance(world.ents[&id].render_pos),
                world.ents[&id].state.hp,
                agora,
            )
        });
        escolhido
    }

    /// The button. The key (Z) only shows with Alt; the state goes to the single strip.
    pub fn desenha(&self) {
        let r = retangulo();
        let c = r.center();
        let raio = r.w * 0.49;
        let cor = if self.ativo() {
            estilo::AUTO
        } else {
            estilo::OURO
        };
        let e = estilo::estado_de(r, false, self.ativo());
        estilo::botao_redondo(c, raio, cor, e, self.ativo());
        if self.ativo() {
            estilo::arco(c, raio + 4.0, get_time() as f32 * 0.8, 0.20, 2.0, cor);
        }
        if !crate::icones_ui::ui("auto_combate", c - vec2(0.0, raio * 0.16), raio * 1.0, cor) {
            estilo::icone(2, c - vec2(0.0, raio * 0.16), raio * 0.46, cor);
        }
        estilo::texto_centro_forte(
            c.x,
            c.y + raio * 0.62,
            if self.ativo() { "AUTO" } else { "COMBAT" },
            10,
            cor,
        );
        crate::hud_layout::chip(r, "Z");
    }

    /// The status strip's text, with AUTO on.
    pub fn faixa(&self, tem_alvo: bool) -> Option<&'static str> {
        self.ativo().then_some(if tem_alvo {
            "AUTO COMBAT · ATTACKING"
        } else {
            "AUTO COMBAT · SEARCHING"
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn mundo() -> World {
        let mut w = World::default();
        let mut metas = vec![];
        let mut estados = vec![];
        for (id, tag, x, hp) in [
            (1, EntityTag::Player, 0.0, 100),
            (2, EntityTag::Player, 1.0, 100),
            (3, EntityTag::Enemy, 3.0, 100),
            (4, EntityTag::Enemy, 8.0, 100),
            (5, EntityTag::Enemy, 30.0, 100),
            (6, EntityTag::Enemy, 2.0, 0),
        ] {
            metas.push(shared::EntityMeta { skins: 0,
                pk: Default::default(),
                auras: 0,
                id: EntityId(id),
                tag,
                name: None,
                hp_max: 100,
                faction: None,
                kind: 0,
                nivel: 1,
                desafio: None,
            aparencia: 0,
            });
            estados.push(shared::EntityState::quantize(
                EntityId(id),
                ::glam::Vec2::new(x, 0.0),
                ::glam::Vec2::ZERO,
                hp,
                if id == 1 { shared::ent_flags::SELF } else { 0 },
            ));
        }
        w.apply(metas, estados, &[]);
        w
    }
    #[test]
    fn pk_pacifico_retira_jogador_do_auto_ate_com_prioridade_e_alvo_atual() {
        let mut w = mundo();
        let mut a = AutoCombate::default();
        a.pvp = shared::protocol::auto_pvp::QUALQUER;
        a.ordem = vec![shared::protocol::auto_alvo::JOGADOR];
        a.ligar(Vec2::ZERO);
        assert_eq!(a.escolher(&w, Some(EntityId(2)), 0.0, None), Some(EntityId(3)));
        w.ents.get_mut(&EntityId(1)).unwrap().meta.pk.hostil = true;
        assert_eq!(a.escolher(&w, None, 1.0, None), Some(EntityId(2)));
        w.ents.get_mut(&EntityId(1)).unwrap().meta.pk.hostil = false;
        assert_eq!(a.escolher(&w, Some(EntityId(2)), 2.0, None), Some(EntityId(3)));
    }

    #[test]
    fn escolhe_monstro_vivo_proximo_e_troca_apos_morte() {
        let mut w = mundo();
        let mut a = AutoCombate::default();
        a.ligar(Vec2::ZERO);
        assert_eq!(a.escolher(&w, None, 0.0, None), Some(EntityId(3)));
        w.ents.get_mut(&EntityId(3)).unwrap().state.hp = 0;
        assert_eq!(
            a.escolher(&w, Some(EntityId(3)), 1.0, None),
            Some(EntityId(4))
        );
        w.ents.get_mut(&EntityId(4)).unwrap().state.hp = 0;
        assert_eq!(a.escolher(&w, Some(EntityId(4)), 2.0, None), None);
        assert!(a.ativo()); // Waits for the respawn, without choosing a player or leaving the area.
    }
    #[test]
    fn abandona_alvo_inacessivel_e_para_quando_personagem_cai() {
        let mut w = mundo();
        let mut a = AutoCombate::default();
        a.ligar(Vec2::ZERO);
        a.escolher(&w, None, 0.0, None);
        assert_eq!(
            a.escolher(&w, Some(EntityId(3)), 9.0, None),
            Some(EntityId(4))
        );
        w.ents.get_mut(&EntityId(1)).unwrap().state.flags |= shared::ent_flags::DOWNED;
        assert_eq!(a.escolher(&w, Some(EntityId(4)), 10.0, None), None);
        assert!(!a.ativo());
    }
    #[test]
    fn sem_visada_repetida_troca_de_alvo_rapido() {
        let w = mundo();
        let mut a = AutoCombate::default();
        a.ligar(Vec2::ZERO);
        assert_eq!(a.escolher(&w, None, 0.0, None), Some(EntityId(3)));
        a.sem_visada(EntityId(3), 0.1);
        assert_eq!(
            a.escolher(&w, Some(EntityId(3)), 0.2, None),
            Some(EntityId(3)),
            "um aviso so' nao troca"
        );
        a.sem_visada(EntityId(3), 1.1);
        assert_eq!(
            a.escolher(&w, Some(EntityId(3)), 1.2, None),
            Some(EntityId(4)),
            "segundo aviso em ~1 s troca"
        );
        // An old notice does not count: the sequence restarts.
        let mut b = AutoCombate::default();
        b.ligar(Vec2::ZERO);
        b.escolher(&w, None, 0.0, None);
        b.sem_visada(EntityId(3), 0.0);
        b.sem_visada(EntityId(3), 5.0);
        assert_eq!(
            b.escolher(&w, Some(EntityId(3)), 5.1, None),
            Some(EntityId(3))
        );
    }

    /// Cleared what was nearby: AUTO goes after the next creature instead of
    /// standing still (it was the owner's complaint — "it only kills one and stops").
    #[test]
    fn caca_prioriza_missao_e_nao_volta_ao_alvo_ignorado() {
        let mut w = mundo();
        let mut a = AutoCombate::default();
        a.ligar(Vec2::ZERO);
        w.ents.get_mut(&EntityId(5)).unwrap().meta.kind = 7;
        assert_eq!(a.caca(&w, Vec2::ZERO, 1.0, Some(7)), Some(vec2(30.0, 0.0)));
        a.ignorados.insert(EntityId(5), 20.0);
        a.ignorados.insert(EntityId(3), 20.0);
        assert_eq!(a.caca(&w, Vec2::ZERO, 3.0, Some(7)), Some(vec2(8.0, 0.0)));
    }

    #[test]
    fn perseguir_fora_da_area_nao_desliga_o_auto() {
        let mut w = mundo();
        let mut a = AutoCombate::default();
        a.ligar(Vec2::ZERO);
        w.ents.get_mut(&EntityId(1)).unwrap().render_pos = vec2(40.0, 0.0);
        w.ents.get_mut(&EntityId(5)).unwrap().render_pos = vec2(42.0, 0.0);
        assert_eq!(a.escolher(&w, Some(EntityId(5)), 10.0, None), Some(EntityId(5)));
        assert!(a.ativo());
    }

    #[test]
    fn sem_bicho_na_area_vai_cacar_o_proximo() {
        let mut w = mundo();
        let mut a = AutoCombate::default();
        a.ligar(Vec2::ZERO);
        // Mata os dois de perto; sobra o de 30 unidades, fora do raio da area.
        for id in [3u32, 4] {
            w.ents.get_mut(&EntityId(id)).unwrap().state.hp = 0;
        }
        assert_eq!(
            a.escolher(&w, None, 0.0, None),
            None,
            "nenhum dentro da area"
        );
        assert_eq!(
            a.caca(&w, Vec2::ZERO, 1.0, None),
            Some(vec2(30.0, 0.0)),
            "vai ate' o proximo"
        );
        assert_eq!(
            a.caca(&w, Vec2::ZERO, 1.1, None),
            None,
            "nao repete o pedido a cada quadro"
        );
        // Walked there: the area came along and `escolher` picks the creature up.
        w.ents.get_mut(&EntityId(1)).unwrap().render_pos = vec2(28.0, 0.0);
        a.caca(&w, vec2(28.0, 0.0), 2.4, None);
        assert_eq!(a.escolher(&w, None, 2.5, None), Some(EntityId(5)));
        // Too far: there is nothing to hunt.
        w.ents.get_mut(&EntityId(5)).unwrap().render_pos = vec2(400.0, 0.0);
        assert_eq!(a.caca(&w, Vec2::ZERO, 9.0, None), None);
    }

    #[test]
    fn andar_nao_desliga_e_a_area_vem_junto() {
        let mut a = AutoCombate::default();
        a.ligar(Vec2::ZERO);
        a.segurando(Vec2::ZERO, 0.0);
        // Walks 40 units — well beyond the radius of where it was switched on.
        for k in 1..=40 {
            a.andar_manual(k as f64 * 0.1);
            assert!(a.segurando(vec2(k as f32, 0.0), k as f64 * 0.1));
        }
        assert!(a.ativo());
        assert_eq!(a.centro, Some(vec2(40.0, 0.0)));
        // Releases the key: still holds for a moment, then goes back to hunting from there.
        assert!(a.segurando(vec2(40.0, 0.0), 4.2));
        assert!(!a.segurando(vec2(40.0, 0.0), 4.5));
        assert!(a.ativo());
    }
}

#[cfg(test)]
mod testes_da_escolha {
    use super::*;
    use shared::protocol::{auto_alvo, auto_pvp};

    fn c(id: u32, dist: f32) -> Candidato {
        Candidato {
            id: EntityId(id),
            dist,
            jogador: false,
            ranged_em_mim: false,
            agrediu: false,
            da_missao: false,
        }
    }

    /// THE QUEST BEATS EVERYTHING, and is not adjustable.
    ///
    /// Someone who switched auto on in the middle of a hunting quest wants the
    /// quest moving. A preference that got in the way of that would be a foot in
    /// their own hunt.
    #[test]
    fn o_bicho_da_missao_vem_primeiro() {
        let perto = c(1, 2.0);
        let longe_da_missao = Candidato {
            da_missao: true,
            ..c(2, 40.0)
        };
        let atirando = Candidato {
            ranged_em_mim: true,
            ..c(3, 20.0)
        };
        let escolha = escolhe_alvo(
            &[perto, longe_da_missao, atirando],
            &[auto_alvo::RANGED_EM_MIM, auto_alvo::MAIS_PERTO],
            auto_pvp::NUNCA,
        );
        assert_eq!(escolha, Some(EntityId(2)), "a missão não veio primeiro");
    }

    /// A ORDEM ESCOLHIDA MANDA no resto.
    #[test]
    fn a_ordem_decide_entre_longe_e_perto() {
        let perto = c(1, 2.0);
        let atirando = Candidato {
            ranged_em_mim: true,
            ..c(2, 25.0)
        };
        // Whoever shoots first: they win even being 12x further away.
        assert_eq!(
            escolhe_alvo(
                &[perto, atirando],
                &[auto_alvo::RANGED_EM_MIM, auto_alvo::MAIS_PERTO],
                auto_pvp::NUNCA
            ),
            Some(EntityId(2))
        );
        // Invertendo a ordem, o de perto ganha.
        assert_eq!(
            escolhe_alvo(
                &[perto, atirando],
                &[auto_alvo::MAIS_PERTO, auto_alvo::RANGED_EM_MIM],
                auto_pvp::NUNCA
            ),
            Some(EntityId(1))
        );
    }

    /// PVP OFF DOES NOT HIT PEOPLE — not even if they are the "ideal" target.
    ///
    /// It is the rule that protects whoever is on the other side. The default is
    /// NEVER, and a player in the candidate list cannot pierce it.
    #[test]
    fn pvp_desligado_ignora_jogador() {
        let gente = Candidato {
            jogador: true,
            agrediu: true,
            ..c(1, 1.0)
        };
        let bicho = c(2, 30.0);
        assert_eq!(
            escolhe_alvo(
                &[gente, bicho],
                &[auto_alvo::JOGADOR, auto_alvo::MAIS_PERTO],
                auto_pvp::NUNCA
            ),
            Some(EntityId(2)),
            "bateu em jogador com PvP desligado"
        );
    }

    /// HITTING BACK IS ANSWERING, NOT HUNTING.
    ///
    /// With `REVIDAR`, only the player who hit first enters. Someone who walked
    ///  past without touching stays out — otherwise "hit back" would become
    /// "attack anyone", which is something else.
    #[test]
    fn revidar_so_pega_quem_bateu_primeiro() {
        let agressor = Candidato {
            jogador: true,
            agrediu: true,
            ..c(1, 20.0)
        };
        let passante = Candidato {
            jogador: true,
            ..c(2, 2.0)
        };
        assert_eq!(
            escolhe_alvo(
                &[agressor, passante],
                &[auto_alvo::JOGADOR],
                auto_pvp::REVIDAR
            ),
            Some(EntityId(1)),
            "revidou no passante em vez de em quem bateu"
        );
        // Only the passer-by: there is nobody to hit back at, and a target is not invented.
        assert_eq!(
            escolhe_alvo(&[passante], &[auto_alvo::JOGADOR], auto_pvp::REVIDAR),
            None
        );
    }

    /// NENHUMA CATEGORIA PEGOU? AINDA ASSIM ATACA.
    ///
    /// An order of only "player" with PvP off would leave auto switched on
    /// hitting nothing — and auto combat that does not attack is, from the
    /// screen, identical to being broken.
    #[test]
    fn sem_categoria_valida_ainda_ataca_o_mais_perto() {
        let bicho = c(9, 5.0);
        assert_eq!(
            escolhe_alvo(&[bicho], &[auto_alvo::JOGADOR], auto_pvp::NUNCA),
            Some(EntityId(9))
        );
    }
}
