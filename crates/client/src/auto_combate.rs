//! Seleciona apenas monstros perto do ponto onde AUTO foi ligado.
//! Movimento, ataques e dano continuam validados pelo servidor.
use crate::{hud_estilo as estilo, world::World};
use macroquad::prelude::*;
use shared::{EntityId, EntityTag};
use std::collections::HashMap;

const RAIO: f32 = 24.0;

/// Sem bicho na area: o AUTO vai ATRAS do mais perto ate' esta distancia. Sem
/// isto ele limpava o lugar e ficava parado olhando o mato — a area sо' andava
/// quando o jogador andava na mao.
const BUSCA: f32 = 110.0;
/// Intervalo entre pedidos de "ir ate' la'" na caçada.
const PASSO_DA_CACA_S: f64 = 1.2;

/// Parado por este tempo depois de andar na mao, o AUTO volta a conduzir a rota.
const VOLTA_PARADO_S: f64 = 0.4;

pub struct AutoCombate {
    pub centro: Option<Vec2>,
    observado: Option<(EntityId, f32, u16, f64)>,
    ignorados: HashMap<EntityId, f64>,
    /// Andando por conta propria (teclado ou clique no chao). Andar NAO desliga
    /// o AUTO: a area acompanha o personagem. A mira continua escolhendo
    /// alvos; apenas a rota automatica espera o jogador parar.
    manual: bool,
    ultima_pos: Option<Vec2>,
    parado_desde: f64,
    /// Alvo que o servidor diz estar sem visada: (id, primeiro aviso, ultimo).
    sem_visada: Option<(EntityId, f64, f64)>,
    /// Ultimo "ir ate' la'" da caçada.
    caca_em: f64,
    /// A ordem de prioridade escolhida (`shared::protocol::auto_alvo`).
    pub ordem: Vec<u8>,
    /// Até onde vai contra jogador (`shared::protocol::auto_pvp`).
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

/// Um candidato a alvo, já reduzido ao que a escolha precisa saber.
///
/// Fora do `World` de propósito: a regra de prioridade é a parte que erra, e
/// ela precisa de teste sem montar um mundo inteiro.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Candidato {
    pub id: EntityId,
    /// Distância até mim.
    pub dist: f32,
    /// É jogador (e não bicho)?
    pub jogador: bool,
    /// Está ACERTANDO você de longe agora.
    pub ranged_em_mim: bool,
    /// Bateu em você há pouco.
    pub agrediu: bool,
    /// É o bicho que a missão ativa pede.
    pub da_missao: bool,
}

/// Por quanto tempo alguém que te bateu continua contando como agressor.
///
/// Oito segundos: tempo de você revidar sem que um tiro perdido de um minuto
/// atrás marque um jogador como inimigo pro resto da sessão.
pub const AGRESSOR_S: f64 = 8.0;

/// A partir daqui, quem te acerta está te acertando DE LONGE.
///
/// Seis unidades: mais que o alcance de qualquer golpe de mão. Quem bate em
/// você de mais longe que isso está atirando, seja lá qual for a espécie.
pub const DISTANCIA_DE_LONGE: f32 = 6.0;

/// A ESCOLHA DE ALVO, por prioridade.
///
/// O dono: "quando eu tiver fazendo missão tem que ser o mob da missão, mas
/// se eu tiver igual estou agora na Ilha Mágica eu tenho que poder escolher:
/// inimigos ranged que estão me atacando de longe, inimigos que estão
/// próximos, player".
///
/// A MISSÃO VEM PRIMEIRO E NÃO É AJUSTÁVEL: quem ligou o auto no meio de uma
/// missão de caça quer a missão andando, e uma preferência que atrapalhasse
/// isso seria um pé na própria caça. O resto segue a ordem escolhida.
///
/// Dentro de cada categoria, o mais perto — e o id desempata, pra a escolha
/// não tremer entre dois iguais.
pub fn escolhe_alvo(cands: &[Candidato], ordem: &[u8], pvp: u8) -> Option<EntityId> {
    use shared::protocol::{auto_alvo, auto_pvp};
    let pode = |c: &Candidato| -> bool {
        if !c.jogador {
            return true;
        }
        match pvp {
            auto_pvp::QUALQUER => true,
            // REVIDAR não é "atacar jogador": é responder a quem começou.
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
    // 1. A missão, sempre.
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
    // 3. Nenhuma categoria pegou: o mais perto que for permitido. Sem isto,
    //    uma ordem só de "jogador" com PvP desligado deixaria o auto ligado
    //    sem bater em nada — que da tela é igual a estar quebrado.
    melhor(&|_| true)
}

/// Aviso de "sem visada" seguido por este tempo: o AUTO larga o alvo. O
/// servidor avisa 1 vez por segundo, entao o segundo aviso ja' troca.
const TROCA_SEM_VISADA_S: f64 = 0.9;
/// Mais que isto sem aviso novo: a sequencia recomeca.
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

    /// O servidor avisou que o alvo `id` esta' sem visada (`ServerMessage::
    /// SemVisada`). Com o AUTO ligado, aviso repetido larga o alvo por 10 s —
    /// em vez de esperar os 8 s sem dano do `escolher`.
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

    /// O jogador esta' andando na mao neste quadro.
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

    /// Nenhum bicho na area: para onde ir caçar. Move a area pro personagem e
    /// devolve o bicho vivo mais perto dentro de `BUSCA` — quem anda ate' la'
    /// e' o `main`. `None` quando nao ha' o que caçar (ou e' cedo demais).
    pub fn caca(&mut self, world: &World, eu: Vec2, agora: f64, missao: Option<u16>) -> Option<Vec2> {
        if !self.ativo() || self.manual {
            return None;
        }
        // A area acompanha o personagem: sem isso, limpar o lugar e' o fim.
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
                missao.is_some_and(|k| a.meta.kind != k)
                    .cmp(&missao.is_some_and(|k| b.meta.kind != k)).then_with(||
                    a.render_pos.distance_squared(eu).total_cmp(&b.render_pos.distance_squared(eu)))
            })?.render_pos;
        self.caca_em = agora;
        Some(alvo)
    }

    /// `missao` = o `kind` do bicho que a missão ativa pede, quando há.
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
        // Perseguir um mob não desliga o auto ao sair da área inicial.
        if eu.render_pos.distance(centro) > RAIO + 4.0 {
            centro = eu.render_pos;
            self.centro = Some(centro);
        }
        self.ignorados.retain(|_, ate| *ate > agora);
        // Quem PODE ser alvo. Jogador só entra quando o ajuste permite —
        // `escolhe_alvo` decide de novo lá dentro, mas deixar entrar aqui é o
        // que torna a categoria "jogador" possível.
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
        // A ESCOLHA POR PRIORIDADE. Antes era só o mais perto.
        let cands: Vec<Candidato> = world
            .ents
            .iter()
            .filter(|(id, _)| valido_alvo(**id) && !self.ignorados.contains_key(id))
            .map(|(id, e)| Candidato {
                id: *id,
                dist: eu.render_pos.distance(e.render_pos),
                jogador: e.meta.tag == EntityTag::Player,
                // "ME ACERTANDO DE LONGE" SEM PRECISAR SABER A ESPÉCIE.
                //
                // Eu ia ler `ENEMY_SHOOT`, mas ele é constante de ANIMAÇÃO e
                // não viaja no estado da entidade. O sinal que existe de
                // verdade é melhor: quem me ACERTOU há pouco e está a mais de
                // um braço de distância só pode estar atirando. Isso vale pra
                // qualquer inimigo que o jogo venha a ter, sem tabela.
                ranged_em_mim: eu.render_pos.distance(e.render_pos) > DISTANCIA_DE_LONGE
                    && world
                        .agressores
                        .get(id)
                        .is_some_and(|t| agora - t < AGRESSOR_S),
                agrediu: world
                    .agressores
                    .get(id)
                    .is_some_and(|t| agora - t < AGRESSOR_S),
                da_missao: missao.is_some_and(|k| e.meta.kind == k),
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

    /// O botao. A tecla (Z) so' aparece com Alt; o estado vai pra faixa unica.
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
            if self.ativo() { "AUTO" } else { "COMBATE" },
            10,
            cor,
        );
        crate::hud_layout::chip(r, "Z");
    }

    /// O texto da faixa de estado, com o AUTO ligado.
    pub fn faixa(&self, tem_alvo: bool) -> Option<&'static str> {
        self.ativo().then_some(if tem_alvo {
            "AUTO COMBATE · ATACANDO"
        } else {
            "AUTO COMBATE · BUSCANDO"
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
            metas.push(shared::EntityMeta {
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
        assert!(a.ativo()); // Aguarda respawn, sem escolher player ou sair da area.
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
        // Aviso velho nao conta: a sequencia recomeca.
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

    /// Limpou o que estava perto: o AUTO vai atras do proximo bicho em vez de
    /// ficar parado (era a queixa do dono — "so' mata um e para").
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
        // Andou ate' la': a area foi junto e o `escolher` pega o bicho.
        w.ents.get_mut(&EntityId(1)).unwrap().render_pos = vec2(28.0, 0.0);
        a.caca(&w, vec2(28.0, 0.0), 2.4, None);
        assert_eq!(a.escolher(&w, None, 2.5, None), Some(EntityId(5)));
        // Longe demais: nao ha' o que caçar.
        w.ents.get_mut(&EntityId(5)).unwrap().render_pos = vec2(400.0, 0.0);
        assert_eq!(a.caca(&w, Vec2::ZERO, 9.0, None), None);
    }

    #[test]
    fn andar_nao_desliga_e_a_area_vem_junto() {
        let mut a = AutoCombate::default();
        a.ligar(Vec2::ZERO);
        a.segurando(Vec2::ZERO, 0.0);
        // Anda 40 unidades — bem alem do raio de onde ligou.
        for k in 1..=40 {
            a.andar_manual(k as f64 * 0.1);
            assert!(a.segurando(vec2(k as f32, 0.0), k as f64 * 0.1));
        }
        assert!(a.ativo());
        assert_eq!(a.centro, Some(vec2(40.0, 0.0)));
        // Solta a tecla: ainda segura um instante, depois volta a caçar dali.
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

    /// A MISSÃO GANHA DE TUDO, e não é ajustável.
    ///
    /// Quem ligou o auto no meio de uma missão de caça quer a missão andando.
    /// Uma preferência que atrapalhasse isso seria um pé na própria caça.
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
        // Quem atira primeiro: ele ganha mesmo estando 12x mais longe.
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

    /// PVP DESLIGADO NÃO BATE EM GENTE — nem se ela for o alvo "ideal".
    ///
    /// É a regra que protege quem está do outro lado. O padrão é NUNCA, e um
    /// jogador na lista de candidatos não pode furá-lo.
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

    /// REVIDAR É RESPONDER, NÃO CAÇAR.
    ///
    /// Com `REVIDAR`, só entra o jogador que bateu primeiro. O que passou ao
    /// lado sem encostar continua de fora — senão "revidar" viraria "atacar
    /// qualquer um", que é outra coisa.
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
        // Só o passante: não há em quem revidar, e não se inventa alvo.
        assert_eq!(
            escolhe_alvo(&[passante], &[auto_alvo::JOGADOR], auto_pvp::REVIDAR),
            None
        );
    }

    /// NENHUMA CATEGORIA PEGOU? AINDA ASSIM ATACA.
    ///
    /// Uma ordem só de "jogador" com PvP desligado deixaria o auto ligado sem
    /// bater em nada — e auto combate que não ataca é, da tela, idêntico a
    /// estar quebrado.
    #[test]
    fn sem_categoria_valida_ainda_ataca_o_mais_perto() {
        let bicho = c(9, 5.0);
        assert_eq!(
            escolhe_alvo(&[bicho], &[auto_alvo::JOGADOR], auto_pvp::NUNCA),
            Some(EntityId(9))
        );
    }
}
