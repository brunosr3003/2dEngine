//! Auto quest: clicked a quest in the tracker (or "Go" in the journal), the
//! character goes alone — talks to the NPC, fights in the zone where the
//! creature spawns, gathers at the vein — and returns to the Master to hand
//! it in. The player presses "Next" and "Receive"; the rest moves by itself.
//!
//! WHERE each objective is, the server says (`QuestDestino`): it knows the
//! spawn zones and what is exhausted. Here we only decide the next step,
//! with no macroquad in the core — the machine is tested whole.
use macroquad::prelude::*;
use shared::quests::destino_tipo;
use shared::EntityId;

/// Got this close to the NPC: asks for the conversation (the shop's "go to
/// the NPC" ends the path and interacts).
const PERTO_DO_NPC: f32 = 4.5;
/// No answer from the server in this time: ask again.
const REPEDE_S: f64 = 4.0;
/// Travel ended far from the destination, or auto combat/gathering switched
/// off: try again after this.
const RELIGA_S: f64 = 1.0;
/// Interagiu e nenhum dialogo abriu nesse tempo: pergunta o destino de novo.
const ESPERA_FALA_S: f64 = 5.0;
/// Handed in and the Master did not offer the next one in this time: it is over.
const ESPERA_PROXIMA_S: f64 = 6.0;
/// After talking or meeting the objective, the server needs a moment to send
/// the `QuestUpdate` before the destination changes.
const FOLGA_DO_SERVIDOR_S: f64 = 0.6;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Etapa {
    PedirDestino,
    Esperando,
    Indo,
    Falando,
    Combatendo,
    Coletando,
    SaindoDaColeta,
    AguardandoProxima,
    /// Chegou no ponto-chave da historia: espera o servidor concluir o passo.
    NoLugar,
}

/// At the key point with the step not changing in this time: ask for the
/// destination again.
const ESPERA_NO_LUGAR_S: f64 = 6.0;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Destino {
    pub tipo: u8,
    pub pos: Vec2,
    pub raio: f32,
    pub npc: Option<EntityId>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Acao {
    PedirDestino(u16),
    Viajar(Vec2),
    /// Talk to the NPC (id and where the server said they are).
    Interagir(EntityId, Vec2),
    LigarCombate(Vec2),
    LigarColeta(Vec2),
    /// Desliga auto combate e auto coleta.
    PararAutos,
    Aviso(String),
}

/// What the frame knows and the machine needs.
#[derive(Debug, Clone, Copy)]
pub struct Ctx {
    pub eu: Vec2,
    pub agora: f64,
    pub viajando: bool,
    /// The objective is met (ready to hand in).
    pub pronta: bool,
    /// The quest is still in progress.
    pub na_log: bool,
    pub dialogo_aberto: bool,
    pub combate_ativo: bool,
    pub coleta_ativa: bool,
    /// How much of the objective is done. It is the "it is moving" signal —
    /// without it, an auto that spins harvesting nothing is indistinguishable
    /// from one that harvests.
    pub progresso: u32,
}

#[derive(Debug, Default)]
pub struct AutoMissao {
    pub quest: Option<u16>,
    pub nome: String,
    etapa: Option<Etapa>,
    destino: Option<Destino>,
    desde: f64,
    /// "Nothing is changing" — see `parado.rs`.
    parado: crate::parado::Parado,
    recurso_parou: bool,
}

impl AutoMissao {
    /// Has the body been still for too long?
    fn travado(&self, c: &Ctx) -> bool {
        self.parado.travado(c.agora)
    }

    /// Tracks movement only to recover the travel to the area.
    fn acompanha(&mut self, c: &Ctx) {
        self.parado.acompanha(c.eu, c.agora);
    }
}

impl AutoMissao {
    pub fn ativo(&self) -> bool {
        self.quest.is_some()
    }

    pub fn etapa(&self) -> Option<Etapa> {
        self.etapa
    }

    pub fn iniciar(&mut self, quest_id: u16, nome: String, agora: f64) {
        *self = Self {
            quest: Some(quest_id),
            nome,
            etapa: Some(Etapa::PedirDestino),
            destino: None,
            desde: agora,
            parado: crate::parado::Parado::default(),
            recurso_parou: false,
        };
    }

    pub fn parar(&mut self) {
        *self = Self::default();
    }

    fn pedir_de_novo(&mut self, agora: f64) {
        self.destino = None;
        self.etapa = Some(Etapa::PedirDestino);
        self.desde = agora + FOLGA_DO_SERVIDOR_S;
    }

    /// The server's answer. `Some(aviso)` when the quest has nowhere to go
    /// (the auto quest stops).
    pub fn destino_recebido(
        &mut self,
        quest_id: u16,
        tipo: u8,
        pos: Vec2,
        raio: f32,
        npc: Option<EntityId>,
        agora: f64,
    ) -> Option<String> {
        if self.quest != Some(quest_id) || self.etapa != Some(Etapa::Esperando) {
            return None;
        }
        if tipo == destino_tipo::NENHUM {
            let nome = std::mem::take(&mut self.nome);
            self.parar();
            return Some(format!(
                "Auto missão: não sei onde fica o objetivo de \"{nome}\" nesta ilha."
            ));
        }
        if tipo == destino_tipo::TRAVA {
            let nome = std::mem::take(&mut self.nome);
            self.parar();
            return Some(format!("História: {nome} para continuar."));
        }
        self.destino = Some(Destino {
            tipo,
            pos,
            raio,
            npc,
        });
        self.etapa = Some(Etapa::Indo);
        // Travel can already be requested next frame.
        self.desde = agora - RELIGA_S;
        None
    }

    /// The player finished the "talk to" dialogue: the next one is to go back.
    pub fn conversou(&mut self, agora: f64) {
        if self.ativo() {
            self.pedir_de_novo(agora);
        }
    }

    /// O jogador recebeu a recompensa: espera a oferta da proxima.
    pub fn entregue(&mut self, agora: f64) {
        if self.ativo() {
            self.destino = None;
            self.etapa = Some(Etapa::AguardandoProxima);
            self.desde = agora;
        }
    }

    /// The server warned that the node stopped (exhausted, or refused the gathering).
    pub fn recurso_parou(&mut self) {
        if self.etapa == Some(Etapa::Coletando) {
            self.recurso_parou = true;
        }
    }

    /// One frame. Returns what `main` should do.
    pub fn passo(&mut self, c: Ctx) -> Vec<Acao> {
        self.acompanha(&c);
        let (Some(id), Some(etapa)) = (self.quest, self.etapa) else {
            return Vec::new();
        };
        let mut saida = Vec::new();
        // Abandoned (or handed in elsewhere): there is nothing to drive. Asking for
        // the destination does not count — a just-accepted quest has not reached the
        // log yet, and for an abandoned one the server answers `NENHUM`.
        if !c.na_log
            && !matches!(
                etapa,
                Etapa::AguardandoProxima | Etapa::PedirDestino | Etapa::Esperando
            )
        {
            let nome = std::mem::take(&mut self.nome);
            self.parar();
            return vec![
                Acao::PararAutos,
                Acao::Aviso(format!(
                    "Auto missão encerrada: \"{nome}\" não está mais ativa."
                )),
            ];
        }
        match etapa {
            Etapa::PedirDestino => {
                if c.agora >= self.desde {
                    saida.push(Acao::PedirDestino(id));
                    self.etapa = Some(Etapa::Esperando);
                    self.desde = c.agora;
                }
            }
            Etapa::Esperando => {
                if c.agora - self.desde > REPEDE_S {
                    self.etapa = Some(Etapa::PedirDestino);
                    self.desde = c.agora;
                }
            }
            Etapa::Indo => {
                let Some(d) = self.destino else {
                    self.pedir_de_novo(c.agora);
                    return saida;
                };
                let npc = d.tipo == destino_tipo::NPC || d.tipo == destino_tipo::ENTREGA;
                // Fight/gathering met on the way (the bag already had it): go back.
                if !npc && c.pronta {
                    self.pedir_de_novo(c.agora);
                    return saida;
                }
                let alcance = if npc { PERTO_DO_NPC } else { d.raio.max(3.0) };
                if c.eu.distance(d.pos) <= alcance {
                    self.desde = c.agora;
                    match d.tipo {
                        destino_tipo::COMBATE => {
                            saida.push(Acao::LigarCombate(d.pos));
                            self.etapa = Some(Etapa::Combatendo);
                        }
                        destino_tipo::COLETA => {
                            saida.push(Acao::LigarColeta(d.pos));
                            self.etapa = Some(Etapa::Coletando);
                        }
                        destino_tipo::LUGAR => {
                            self.etapa = Some(Etapa::NoLugar);
                        }
                        _ => {
                            if let Some(n) = d.npc {
                                saida.push(Acao::Interagir(n, d.pos));
                                self.etapa = Some(Etapa::Falando);
                            } else {
                                // The NPC may still be outside the AOI. Ask for its identifier again
                                // on getting close.
                                self.pedir_de_novo(c.agora);
                            }
                        }
                    }
                } else if (!c.viajando && c.agora - self.desde >= RELIGA_S)
                    // TRAVADO CONTA MESMO VIAJANDO.
                    //
                    // This was the failure: stuck on the corner of a house, travel stays
                    // active and `!c.viajando` never let it try again. The body pushed the
                    //  wall until the player touched the D-pad — "I get stuck on the houses
                    // all the time".
                    || self.travado(&c)
                {
                    self.desde = c.agora;
                    self.parado.zera(c.agora);
                    // NPC: stop beside them, not on top.
                    let alvo = if npc {
                        d.pos + (c.eu - d.pos).normalize_or_zero() * 2.0
                    } else {
                        d.pos
                    };
                    saida.push(Acao::Viajar(alvo));
                }
            }
            Etapa::Falando => {
                if c.dialogo_aberto {
                    self.desde = c.agora;
                } else if c.agora - self.desde > ESPERA_FALA_S {
                    self.pedir_de_novo(c.agora);
                }
            }
            Etapa::Combatendo | Etapa::Coletando => {
                let Some(d) = self.destino else {
                    self.pedir_de_novo(c.agora);
                    return saida;
                };
                if c.pronta {
                    saida.push(Acao::PararAutos);
                    self.pedir_de_novo(c.agora);
                } else if self.recurso_parou || c.agora - self.desde > RELIGA_S {
                    // The specialised mode drives the hunt/gathering to completion.
                    // Time without the counter rising does not mean being stuck.
                    if etapa == Etapa::Combatendo && !c.combate_ativo {
                        self.desde = c.agora;
                        saida.push(Acao::LigarCombate(c.eu));
                    } else if etapa == Etapa::Coletando && !self.recurso_parou && !c.coleta_ativa {
                        self.desde = c.agora;
                        saida.push(Acao::LigarColeta(c.eu));
                    } else if etapa == Etapa::Coletando && self.recurso_parou {
                        // Leaves the node's range before asking for the next one.
                        // The client may still hold the old node as its target for
                        // a few frames after it is exhausted.
                        let dir = (c.eu - d.pos).normalize_or(vec2(1.0, 0.0));
                        saida.push(Acao::PararAutos);
                        saida.push(Acao::Viajar(c.eu + dir * 7.0));
                        self.etapa = Some(Etapa::SaindoDaColeta);
                        self.desde = c.agora;
                        self.recurso_parou = false;
                    }
                }
            }
            Etapa::SaindoDaColeta => {
                if self.destino.is_none_or(|d| c.eu.distance(d.pos) >= 6.0)
                    || c.agora - self.desde > 4.0 {
                    self.pedir_de_novo(c.agora);
                }
            }
            Etapa::AguardandoProxima => {
                if c.agora - self.desde > ESPERA_PROXIMA_S {
                    self.parar();
                    saida.push(Acao::Aviso(
                        "Auto missão: o Mestre não tem missão nova agora.".into(),
                    ));
                }
            }
            Etapa::NoLugar => {
                if c.agora - self.desde > ESPERA_NO_LUGAR_S {
                    self.pedir_de_novo(c.agora);
                }
            }
        }
        saida
    }

    pub fn texto_da_etapa(&self) -> &'static str {
        match self.etapa {
            None => "",
            Some(Etapa::PedirDestino | Etapa::Esperando) => "procurando o objetivo",
            Some(Etapa::Indo) => match self.destino.map(|d| d.tipo) {
                Some(destino_tipo::NPC) => "indo conversar",
                Some(destino_tipo::ENTREGA) => "voltando ao Mestre",
                Some(destino_tipo::COMBATE) => "indo à zona dos bichos",
                Some(destino_tipo::LUGAR) => "indo ao ponto-chave",
                _ => "indo ao veio",
            },
            Some(Etapa::NoLugar) => "chegando",
            Some(Etapa::Falando) => "conversando",
            Some(Etapa::Combatendo) => "lutando",
            Some(Etapa::Coletando) => "coletando",
            Some(Etapa::SaindoDaColeta) => "procurando outro recurso",
            Some(Etapa::AguardandoProxima) => "recebendo a próxima",
        }
    }

    /// O texto da faixa de estado unica do HUD.
    pub fn faixa(&self) -> Option<String> {
        self.ativo()
            .then(|| format!("AUTO MISSÃO · {} · {}", self.nome, self.texto_da_etapa()))
    }
}

/// The `ARQUIPELAGO` island where quest `quest` happens, when it is NOT
/// `aqui`'s zone. `None` = it is here, or the quest lives on no island.
///
/// It exists separately from `main` to be testable: it is the bridge between
/// the quest id and the index the Harbour Captain understands, and it breaks
/// silently if someone reorders the `ARQUIPELAGO`.
pub fn ilha_da_missao(quest: u16, aqui: &str) -> Option<u8> {
    let alvo = shared::quests::zona_da_missao(quest)?;
    if alvo == aqui {
        return None;
    }
    shared::terreno::ARQUIPELAGO
        .iter()
        .position(|d| d.zona == alvo)
        .map(|i| i as u8)
}

/// What AUTO does with the Harbour Captain's menu.
#[derive(Debug, Clone, PartialEq)]
pub enum Rumo {
    /// Boards on its own: the quest is on another island and that island is unlocked.
    Embarcar(u8),
    /// There is no getting there. Stops the auto and says why.
    Parar(String),
    /// Nada a decidir: abre o menu e deixa o jogador escolher.
    Menu,
}

/// The decision, touching no state: `ilha` is what `ilha_da_missao` returned.
pub fn rumo(destinos: &[shared::viagem::Destino], ilha: Option<u8>) -> Rumo {
    use shared::viagem::estado;
    let Some(ilha) = ilha else {
        return Rumo::Menu;
    };
    match destinos.iter().find(|d| d.ilha == ilha) {
        Some(d) if d.estado == estado::LIBERADA => Rumo::Embarcar(ilha),
        Some(d) if d.estado == estado::FORA_DO_AR => {
            Rumo::Parar(format!("{} está fora do ar: a auto missão para aqui.", d.nome))
        }
        Some(d) if d.estado == estado::BLOQUEADA => {
            let falta = if d.requisito.is_empty() {
                String::new()
            } else {
                format!(" (libera em \"{}\")", d.requisito)
            };
            Rumo::Parar(format!(
                "{} ainda não foi liberada{falta}: a auto missão para aqui.",
                d.nome
            ))
        }
        // HERE (we are already) or an island the menu does not even list: nothing to automate.
        _ => Rumo::Menu,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parado::TRAVADO_S;

    fn destino(ilha: u8, estado: u8) -> shared::viagem::Destino {
        shared::viagem::Destino {
            ilha,
            nome: format!("Ilha {ilha}"),
            nivel_min: 1,
            nivel_max: 10,
            estado,
            requisito: String::new(),
        }
    }

    /// The auto only boards on its own when the quest's island is unlocked.
    #[test]
    fn o_auto_embarca_na_ilha_liberada_e_para_nas_outras() {
        use shared::viagem::estado;
        let ds = vec![
            destino(0, estado::AQUI),
            destino(1, estado::LIBERADA),
            destino(2, estado::FORA_DO_AR),
            destino(3, estado::BLOQUEADA),
        ];
        assert_eq!(rumo(&ds, Some(1)), Rumo::Embarcar(1));
        assert!(matches!(rumo(&ds, Some(2)), Rumo::Parar(_)), "fora do ar para");
        assert!(matches!(rumo(&ds, Some(3)), Rumo::Parar(_)), "bloqueada para");
        // With no quest on another island the menu belongs to the player.
        assert_eq!(rumo(&ds, None), Rumo::Menu);
        // The island we are already on is not "travelled" to.
        assert_eq!(rumo(&ds, Some(0)), Rumo::Menu);
        // An island the menu does not even list: it does not invent a boarding.
        assert_eq!(rumo(&ds, Some(9)), Rumo::Menu);
    }

    /// The quest-id -> Captain-index bridge. If someone reorders the
    /// `ARQUIPELAGO`, the auto would board for the wrong island — this is what holds it.
    #[test]
    fn a_missao_de_outra_ilha_aponta_o_indice_certo() {
        // One quest from each known island, from the zone table itself.
        for (i, d) in shared::terreno::ARQUIPELAGO.iter().enumerate() {
            let Some(q) = shared::quests::QUESTS
                .iter()
                .find(|q| shared::quests::zona_da_missao(q.id) == Some(d.zona))
            else {
                continue;
            };
            // Visto de OUTRA ilha, aponta pro indice desta.
            let outra = if i == 0 { 1 } else { 0 };
            let de = shared::terreno::ARQUIPELAGO[outra].zona;
            assert_eq!(
                ilha_da_missao(q.id, de),
                Some(i as u8),
                "missao {} ({}) devia apontar pra ilha {i}",
                q.id,
                d.zona
            );
            // Seen from INSIDE the island itself, there is no travel.
            assert_eq!(ilha_da_missao(q.id, d.zona), None);
        }
    }

    fn ctx(eu: Vec2, agora: f64) -> Ctx {
        Ctx {
            eu,
            agora,
            viajando: false,
            pronta: false,
            na_log: true,
            dialogo_aberto: false,
            combate_ativo: false,
            coleta_ativa: false,
            progresso: 0,
        }
    }

    fn pede(a: &mut AutoMissao, agora: f64) {
        assert_eq!(
            a.passo(ctx(Vec2::ZERO, agora)),
            vec![Acao::PedirDestino(501)]
        );
        assert_eq!(a.etapa(), Some(Etapa::Esperando));
    }

    /// Talk to: asks for the destination, travels, interacts, dialogue, talked ->
    /// back to the Master, hands in, receives -> next.
    #[test]
    fn npc_dialogo_entrega_e_proxima() {
        let mut a = AutoMissao::default();
        a.iniciar(501, "Conheça o Alquimista".into(), 0.0);
        pede(&mut a, 0.0);
        let alq = vec2(100.0, 0.0);
        assert!(a
            .destino_recebido(501, destino_tipo::NPC, alq, 3.0, Some(EntityId(7)), 0.1)
            .is_none());
        // Far: travel to near them.
        let v = a.passo(ctx(Vec2::ZERO, 0.2));
        assert!(
            matches!(v.as_slice(), [Acao::Viajar(p)] if p.distance(vec2(98.0, 0.0)) < 0.01),
            "{v:?}"
        );
        // Travelling: does not repeat.
        let mut c = ctx(vec2(50.0, 0.0), 0.5);
        c.viajando = true;
        assert!(a.passo(c).is_empty());
        // Chegou: interage.
        assert_eq!(
            a.passo(ctx(vec2(97.0, 0.0), 1.0)),
            vec![Acao::Interagir(EntityId(7), alq)]
        );
        assert_eq!(a.etapa(), Some(Etapa::Falando));
        let mut c = ctx(vec2(97.0, 0.0), 3.0);
        c.dialogo_aberto = true;
        assert!(a.passo(c).is_empty(), "espera o jogador ler");
        a.conversou(3.5);
        assert!(
            a.passo(ctx(vec2(97.0, 0.0), 3.6)).is_empty(),
            "folga pro servidor atualizar"
        );
        assert_eq!(
            a.passo(ctx(vec2(97.0, 0.0), 4.2)),
            vec![Acao::PedirDestino(501)]
        );
        // Now the destination is the Master.
        a.destino_recebido(
            501,
            destino_tipo::ENTREGA,
            Vec2::ZERO,
            3.0,
            Some(EntityId(9)),
            4.3,
        );
        let mut c = ctx(vec2(97.0, 0.0), 4.4);
        c.pronta = true;
        assert!(matches!(a.passo(c).as_slice(), [Acao::Viajar(_)]));
        let mut c = ctx(vec2(1.0, 0.0), 9.0);
        c.pronta = true;
        assert_eq!(a.passo(c), vec![Acao::Interagir(EntityId(9), Vec2::ZERO)]);
        a.entregue(10.0);
        // The quest leaves the log after being handed in: that is not an error.
        let mut c = ctx(vec2(1.0, 0.0), 10.5);
        c.na_log = false;
        assert!(a.passo(c).is_empty());
        assert!(a.ativo());
        // Chegou a oferta e o main aceitou: comeca a proxima.
        a.iniciar(502, "Lobos na estrada".into(), 11.0);
        assert_eq!(
            a.passo(ctx(vec2(1.0, 0.0), 11.0)),
            vec![Acao::PedirDestino(502)]
        );
    }

    /// Fight: goes to the zone, switches combat on, switches it back on if it
    /// drops, and with the objective met switches everything off and asks for the
    /// destination (the Master).
    #[test]
    fn kill_combate_completo_volta() {
        let mut a = AutoMissao::default();
        a.iniciar(502, "Lobos".into(), 0.0);
        assert_eq!(a.passo(ctx(Vec2::ZERO, 0.0)), vec![Acao::PedirDestino(502)]);
        let zona = vec2(200.0, 0.0);
        a.destino_recebido(502, destino_tipo::COMBATE, zona, 22.0, None, 0.1);
        assert_eq!(a.passo(ctx(Vec2::ZERO, 0.2)), vec![Acao::Viajar(zona)]);
        assert_eq!(
            a.passo(ctx(vec2(185.0, 0.0), 5.0)),
            vec![Acao::LigarCombate(zona)]
        );
        assert_eq!(a.etapa(), Some(Etapa::Combatendo));
        let mut c = ctx(vec2(190.0, 0.0), 5.5);
        c.combate_ativo = true;
        assert!(a.passo(c).is_empty());
        // O auto combate caiu (perseguiu longe demais): religa.
        assert_eq!(
            a.passo(ctx(vec2(190.0, 0.0), 7.0)),
            vec![Acao::LigarCombate(vec2(190.0, 0.0))]
        );
        let mut c = ctx(vec2(190.0, 0.0), 20.0);
        c.combate_ativo = true;
        c.pronta = true;
        assert_eq!(a.passo(c), vec![Acao::PararAutos]);
        assert_eq!(a.etapa(), Some(Etapa::PedirDestino));
    }

    /// Story: goes to the key point and waits there; a level gate stops the auto
    /// quest with a notice.
    #[test]
    fn historia_espera_no_lugar_e_para_na_trava() {
        let mut a = AutoMissao::default();
        a.iniciar(709, "O mirante do Bosque".into(), 0.0);
        a.passo(ctx(Vec2::ZERO, 0.0));
        let mirante = vec2(60.0, 0.0);
        a.destino_recebido(709, destino_tipo::LUGAR, mirante, 26.0, None, 0.1);
        assert_eq!(a.passo(ctx(Vec2::ZERO, 0.2)), vec![Acao::Viajar(mirante)]);
        assert!(a.passo(ctx(vec2(40.0, 0.0), 2.0)).is_empty());
        assert_eq!(a.etapa(), Some(Etapa::NoLugar));
        assert!(
            a.passo(ctx(vec2(40.0, 0.0), 5.0)).is_empty(),
            "espera o servidor"
        );
        a.passo(ctx(vec2(40.0, 0.0), 9.0));
        assert_eq!(
            a.etapa(),
            Some(Etapa::PedirDestino),
            "sem mudar, pergunta de novo"
        );
        // The next step is a gate: stops and warns.
        a.iniciar(710, "Alcance o nível 10".into(), 10.0);
        a.passo(ctx(Vec2::ZERO, 10.0));
        let aviso = a
            .destino_recebido(710, destino_tipo::TRAVA, Vec2::ZERO, 0.0, None, 10.1)
            .unwrap();
        assert!(aviso.contains("nível 10"), "{aviso}");
        assert!(!a.ativo());
    }

    #[test]
    fn coleta_liga_auto_coleta() {
        let mut a = AutoMissao::default();
        a.iniciar(503, "Cobre".into(), 0.0);
        a.passo(ctx(Vec2::ZERO, 0.0));
        a.destino_recebido(503, destino_tipo::COLETA, vec2(10.0, 0.0), 6.0, None, 0.1);
        assert_eq!(
            a.passo(ctx(vec2(8.0, 0.0), 0.2)),
            vec![Acao::LigarColeta(vec2(10.0, 0.0))]
        );
        assert_eq!(a.etapa(), Some(Etapa::Coletando));
    }

    #[test]
    fn npc_fora_do_aoi_e_procurado_de_novo_ao_chegar() {
        let mut a = AutoMissao::default();
        a.iniciar(501, "Converse".into(), 0.0);
        a.passo(ctx(Vec2::ZERO, 0.0));
        a.destino_recebido(501, destino_tipo::NPC, Vec2::ZERO, 3.0, None, 0.1);
        assert!(a.passo(ctx(Vec2::ZERO, 0.2)).is_empty());
        assert_eq!(a.etapa(), Some(Etapa::PedirDestino));
        assert_eq!(a.passo(ctx(Vec2::ZERO, 1.0)), vec![Acao::PedirDestino(501)]);
    }

    #[test]
    fn combate_mantem_auto_ate_concluir_mesmo_sem_contador_avancar() {
        let mut a = AutoMissao::default();
        a.iniciar(502, "Lobos".into(), 0.0);
        a.passo(ctx(Vec2::ZERO, 0.0));
        a.destino_recebido(502, destino_tipo::COMBATE, Vec2::ZERO, 5.0, None, 0.1);
        a.passo(ctx(Vec2::ZERO, 0.2));
        let mut c = ctx(Vec2::ZERO, 7.0);
        c.combate_ativo = true;
        for t in [7.0, 30.0, 120.0] {
            c.agora = t;
            assert!(a.passo(c).is_empty());
            assert_eq!(a.etapa(), Some(Etapa::Combatendo));
        }
        c.pronta = true;
        assert_eq!(a.passo(c), vec![Acao::PararAutos]);
        assert_eq!(a.etapa(), Some(Etapa::PedirDestino));
    }

    #[test]
    fn cancelamentos_e_sem_destino() {
        let mut a = AutoMissao::default();
        a.iniciar(505, "Porto".into(), 0.0);
        a.passo(ctx(Vec2::ZERO, 0.0));
        // Unknown destination: stops and warns.
        assert!(a
            .destino_recebido(505, destino_tipo::NENHUM, Vec2::ZERO, 0.0, None, 0.1)
            .is_some());
        assert!(!a.ativo());
        // Recem-aceita ainda fora do log: pede o destino mesmo assim.
        a.iniciar(502, "Lobos".into(), 0.0);
        let mut c = ctx(Vec2::ZERO, 0.0);
        c.na_log = false;
        assert_eq!(a.passo(c), vec![Acao::PedirDestino(502)]);
        // Abandonada no meio do caminho: encerra.
        a.destino_recebido(
            502,
            destino_tipo::COMBATE,
            vec2(50.0, 0.0),
            20.0,
            None,
            0.05,
        );
        let mut c = ctx(Vec2::ZERO, 0.1);
        c.na_log = false;
        let v = a.passo(c);
        assert_eq!(v[0], Acao::PararAutos);
        assert!(!a.ativo());
        // Resposta de outra missao (velha) e' ignorada.
        a.iniciar(502, "Lobos".into(), 0.0);
        a.passo(ctx(Vec2::ZERO, 0.0));
        assert!(a
            .destino_recebido(501, destino_tipo::NPC, Vec2::ZERO, 3.0, None, 0.1)
            .is_none());
        assert_eq!(a.etapa(), Some(Etapa::Esperando));
        // Servidor mudo: pergunta de novo.
        a.passo(ctx(Vec2::ZERO, 5.0));
        assert_eq!(a.passo(ctx(Vec2::ZERO, 5.1)), vec![Acao::PedirDestino(502)]);
        // No new quest after handing in: it ends by itself.
        a.entregue(10.0);
        let v = a.passo(ctx(Vec2::ZERO, 17.0));
        assert!(matches!(v.as_slice(), [Acao::Aviso(_)]));
        assert!(!a.ativo());
        a.iniciar(502, "x".into(), 0.0);
        a.parar();
        assert!(!a.ativo());
    }
    /// STUCK ON A HOUSE IS NOT "GOING".
    ///
    /// The owner: "I get stuck on the houses all the time" and "the auto quest
    /// actually gets stuck in all sorts of situations". The retry only fired
    /// with travel OFF, and stuck on a corner travel stays active: the body
    /// pushed the wall until the person touched the D-pad.
    ///
    /// The test measures both sides: still with travel active has to redo the
    /// route, and WALKING must not redo it — otherwise the auto quest would
    /// recompute the path every three seconds of normal walking.
    #[test]
    fn parado_com_viagem_ativa_refaz_a_rota() {
        let mut a = AutoMissao::default();
        a.iniciar(501, "x".into(), 0.0);
        pede(&mut a, 0.0);
        a.destino_recebido(
            501,
            destino_tipo::COMBATE,
            Vec2::new(100.0, 0.0),
            8.0,
            None,
            0.1,
        );
        assert_eq!(a.etapa(), Some(Etapa::Indo));

        // Viajando e PARADO no mesmo ponto: passado o prazo, refaz.
        let mut c = ctx(Vec2::ZERO, 0.2);
        c.viajando = true;
        assert!(a.passo(c).is_empty(), "cedo demais pra chamar de travado");
        let mut c = ctx(Vec2::ZERO, 0.2 + TRAVADO_S + 0.1);
        c.viajando = true;
        let acoes = a.passo(c);
        assert!(
            acoes.iter().any(|x| matches!(x, Acao::Viajar(_))),
            "parado com viagem ativa não refez a rota: {acoes:?}"
        );

        // WALKING does not redo: the still clock zeroes at every step.
        let mut a2 = AutoMissao::default();
        a2.iniciar(501, "x".into(), 0.0);
        pede(&mut a2, 0.0);
        a2.destino_recebido(
            501,
            destino_tipo::COMBATE,
            Vec2::new(100.0, 0.0),
            8.0,
            None,
            0.1,
        );
        let mut t = 0.2;
        let mut andou = 0.0f32;
        while t < 20.0 {
            andou += 3.0;
            let mut c = ctx(Vec2::new(andou, 0.0), t);
            c.viajando = true;
            let acoes = a2.passo(c);
            assert!(
                !acoes.iter().any(|x| matches!(x, Acao::Viajar(_))),
                "refez a rota de quem está andando (t={t})"
            );
            t += 1.0;
        }
    }

    #[test]
    fn coleta_ativa_nao_e_cancelada_pelo_tempo_sem_progresso() {
        let mut a = AutoMissao::default();
        a.iniciar(501, "x".into(), 0.0);
        pede(&mut a, 0.0);
        a.destino_recebido(501, destino_tipo::COLETA, Vec2::ZERO, 8.0, None, 0.1);
        assert!(a.passo(ctx(Vec2::ZERO, 0.2)).contains(&Acao::LigarColeta(Vec2::ZERO)));
        for t in [7.0, 30.0, 120.0] {
            let mut c = ctx(Vec2::ZERO, t);
            c.coleta_ativa = true;
            assert!(a.passo(c).is_empty());
            assert_eq!(a.etapa(), Some(Etapa::Coletando));
        }
        let mut c = ctx(Vec2::ZERO, 121.0);
        c.pronta = true;
        assert_eq!(a.passo(c), vec![Acao::PararAutos]);
        assert_eq!(a.etapa(), Some(Etapa::PedirDestino));
    }

    #[test]
    fn recurso_esgotado_sai_sem_esperar_seis_segundos() {
        let mut a = AutoMissao::default();
        a.iniciar(503, "Cobre".into(), 0.0);
        assert_eq!(a.passo(ctx(Vec2::ZERO, 0.0)), vec![Acao::PedirDestino(503)]);
        a.destino_recebido(503, destino_tipo::COLETA, Vec2::ZERO, 6.0, None, 0.1);
        a.passo(ctx(Vec2::ZERO, 0.2));
        a.recurso_parou();
        let acoes = a.passo(ctx(Vec2::ZERO, 0.3));
        assert!(acoes.contains(&Acao::PararAutos));
        assert!(acoes.iter().any(|acao| matches!(acao, Acao::Viajar(p) if p.length() >= 6.9)));
        assert_eq!(a.etapa(), Some(Etapa::SaindoDaColeta));
    }

    /// AND GENUINELY HARVESTING, IT DOES NOT TOUCH IT.
    ///
    /// The other side of the same test: restarting a gathering that is yielding
    /// would throw the good node away every six seconds.
    #[test]
    fn coleta_que_rende_nao_e_interrompida() {
        let mut a = AutoMissao::default();
        a.iniciar(501, "x".into(), 0.0);
        pede(&mut a, 0.0);
        a.destino_recebido(501, destino_tipo::COLETA, Vec2::ZERO, 8.0, None, 0.1);
        let _ = a.passo(ctx(Vec2::ZERO, 0.2));
        let mut t = 0.3;
        let mut p = 0u32;
        while t < 40.0 {
            p += 1;
            let mut c = ctx(Vec2::ZERO, t);
            c.coleta_ativa = true;
            c.progresso = p;
            let acoes = a.passo(c);
            assert!(
                !acoes.contains(&Acao::PararAutos),
                "interrompeu uma coleta que estava rendendo (t={t}, progresso={p})"
            );
            t += 1.0;
        }
    }
}
