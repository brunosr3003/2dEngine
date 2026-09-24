//! Auto missao: clicou numa missao do rastreador (ou "Ir" no diario), o
//! personagem vai sozinho — fala com o NPC, luta na zona onde o bicho nasce,
//! coleta no veio — e volta ao Mestre pra entregar. Quem aperta "Proximo" e
//! "Receber" e' o jogador; o resto anda sozinho.
//!
//! ONDE fica cada objetivo quem diz e' o servidor (`QuestDestino`): ele
//! conhece as zonas de spawn e o que esta' esgotado. Aqui so' se decide o
//! proximo passo, sem macroquad no nucleo — a maquina e' testada inteira.
use macroquad::prelude::*;
use shared::quests::destino_tipo;
use shared::EntityId;

/// Chegou perto disto do NPC: pede a conversa (o "ir ate' o NPC" da loja
/// termina o caminho e interage).
const PERTO_DO_NPC: f32 = 4.5;
/// Sem resposta do servidor nesse tempo: pede de novo.
const REPEDE_S: f64 = 4.0;
/// Viagem acabou longe do destino, ou o auto combate/coleta desligou: tenta
/// de novo depois disso.
const RELIGA_S: f64 = 1.0;
/// Interagiu e nenhum dialogo abriu nesse tempo: pergunta o destino de novo.
const ESPERA_FALA_S: f64 = 5.0;
/// Entregou e o Mestre nao ofereceu a proxima nesse tempo: acabou.
const ESPERA_PROXIMA_S: f64 = 6.0;
/// Depois de conversar ou cumprir o objetivo, o servidor precisa de um
/// instante pra mandar o `QuestUpdate` antes de o destino mudar.
const FOLGA_DO_SERVIDOR_S: f64 = 0.6;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Etapa {
    PedirDestino,
    Esperando,
    Indo,
    Falando,
    Combatendo,
    Coletando,
    AguardandoProxima,
    /// Chegou no ponto-chave da historia: espera o servidor concluir o passo.
    NoLugar,
}

/// No ponto-chave sem o passo mudar nesse tempo: pergunta o destino de novo.
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
    /// Falar com o NPC (id e onde o servidor disse que ele esta').
    Interagir(EntityId, Vec2),
    LigarCombate(Vec2),
    LigarColeta(Vec2),
    /// Desliga auto combate e auto coleta.
    PararAutos,
    Aviso(String),
}

/// O que o quadro sabe e a maquina precisa.
#[derive(Debug, Clone, Copy)]
pub struct Ctx {
    pub eu: Vec2,
    pub agora: f64,
    pub viajando: bool,
    /// O objetivo esta' cumprido (pronta pra entregar).
    pub pronta: bool,
    /// A missao ainda esta' no andamento.
    pub na_log: bool,
    pub dialogo_aberto: bool,
    pub combate_ativo: bool,
    pub coleta_ativa: bool,
    /// Quanto do objetivo já foi feito. É o sinal de "está andando" — sem
    /// ele, um auto que gira sem colher nada é indistinguível de um que
    /// colhe.
    pub progresso: u32,
}

/// Quanto tempo PARADO no mesmo lugar conta como travado.
///
/// O dono: "fico travado toda hora nas casas", "o auto missão na verdade
/// trava em diversas situações". Todas as queixas são o mesmo defeito: quando
/// um passo não completa, NADA acontece — e o retry que existia só disparava
/// com a viagem desligada (`!c.viajando`). Preso numa quina de casa, a viagem
/// continua "ativa" e o corpo empurra a parede para sempre.
///
/// Três segundos é curto o bastante pra não dar tempo de a pessoa notar, e
/// longo o bastante pra não refazer rota a cada tropeço.
const TRAVADO_S: f64 = 3.0;
/// Quanto o corpo precisa andar pra não contar como parado.
const ANDOU_U: f32 = 1.5;
/// Sem o objetivo avançar por tanto tempo, religa o auto (coleta ou combate).
///
/// O dono: "quando eu tô coletando árvore e a árvore acaba, trava o auto
/// missão; só volta quando eu ando". Exato: o religar exigia `!coleta_ativa`,
/// e com o nó esgotado a coleta segue "ativa" sem colher nada.
const SEM_AVANCO_S: f64 = 6.0;

#[derive(Debug, Default)]
pub struct AutoMissao {
    pub quest: Option<u16>,
    pub nome: String,
    etapa: Option<Etapa>,
    destino: Option<Destino>,
    desde: f64,
    /// Onde o corpo estava quando começou a ficar parado, e desde quando.
    onde: Option<Vec2>,
    parado_desde: f64,
    /// Progresso do objetivo na última vez que ele mudou, e quando.
    progresso: u32,
    progresso_desde: f64,
}

impl AutoMissao {
    /// O corpo está parado há mais de `TRAVADO_S`?
    fn travado(&self, c: &Ctx) -> bool {
        c.agora - self.parado_desde > TRAVADO_S
    }

    /// O objetivo parou de avançar?
    fn sem_avanco(&self, c: &Ctx) -> bool {
        c.agora - self.progresso_desde > SEM_AVANCO_S
    }

    /// Uma vez por quadro: atualiza os dois relógios de "nada mudou".
    fn acompanha(&mut self, c: &Ctx) {
        match self.onde {
            Some(o) if o.distance(c.eu) < ANDOU_U => {}
            _ => {
                self.onde = Some(c.eu);
                self.parado_desde = c.agora;
            }
        }
        if c.progresso != self.progresso {
            self.progresso = c.progresso;
            self.progresso_desde = c.agora;
        }
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
            onde: None,
            parado_desde: agora,
            progresso: 0,
            progresso_desde: agora,
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

    /// Resposta do servidor. `Some(aviso)` quando a missao nao tem pra onde ir
    /// (a auto missao para).
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
        // Ja' pode pedir a viagem no proximo quadro.
        self.desde = agora - RELIGA_S;
        None
    }

    /// O jogador terminou o dialogo de "fale com": a proxima e' voltar.
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

    /// Um quadro. Devolve o que o `main` deve fazer.
    pub fn passo(&mut self, c: Ctx) -> Vec<Acao> {
        self.acompanha(&c);
        let (Some(id), Some(etapa)) = (self.quest, self.etapa) else {
            return Vec::new();
        };
        let mut saida = Vec::new();
        // Abandonada (ou entregue por fora): nao ha' o que conduzir. Pedindo o
        // destino nao conta — a missao recem-aceita ainda nao chegou no log, e
        // a abandonada o servidor responde com `NENHUM`.
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
                // Luta/coleta cumprida no caminho (a bolsa ja' tinha): volta.
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
                            }
                            self.etapa = Some(Etapa::Falando);
                        }
                    }
                } else if (!c.viajando && c.agora - self.desde >= RELIGA_S)
                    // TRAVADO CONTA MESMO VIAJANDO.
                    //
                    // Esta era a falha: preso na quina de uma casa, a viagem
                    // segue ativa e o `!c.viajando` nunca deixava tentar de
                    // novo. O corpo empurrava a parede até o jogador mexer no
                    // direcional — "fico travado toda hora nas casas".
                    || self.travado(&c)
                {
                    self.desde = c.agora;
                    self.parado_desde = c.agora;
                    // NPC: pare do lado dele, nao em cima.
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
                } else if c.agora - self.desde > RELIGA_S {
                    // O AUTO "ATIVO" NÃO PROVA QUE ALGO ESTÁ ACONTECENDO.
                    //
                    // Quando o nó de coleta esgota, a coleta continua ligada e
                    // gira no vazio: o `!c.coleta_ativa` nunca era verdade e o
                    // passo nunca religava. O dono: "a árvore acaba e trava; só
                    // volta quando eu ando". Então o gatilho passa a ser
                    // também O OBJETIVO NÃO ANDAR, que é o que importa.
                    let parado = self.sem_avanco(&c);
                    if etapa == Etapa::Combatendo && (!c.combate_ativo || parado) {
                        self.desde = c.agora;
                        self.progresso_desde = c.agora;
                        // Sem `PararAutos` aqui: o auto combate mira uma
                        // ÁREA, não um nó, então religar por cima dele não
                        // deixa nada preso — e desligar antes faria o
                        // personagem largar o alvo que já estava batendo.
                        saida.push(Acao::LigarCombate(d.pos));
                    } else if etapa == Etapa::Coletando && (!c.coleta_ativa || parado) {
                        self.desde = c.agora;
                        self.progresso_desde = c.agora;
                        // PARA ANTES DE LIGAR: religar por cima de uma coleta
                        // presa no nó velho a mantém no nó velho.
                        saida.push(Acao::PararAutos);
                        saida.push(Acao::LigarColeta(d.pos));
                    }
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
            Some(Etapa::AguardandoProxima) => "recebendo a próxima",
        }
    }

    /// O texto da faixa de estado unica do HUD.
    pub fn faixa(&self) -> Option<String> {
        self.ativo()
            .then(|| format!("AUTO MISSÃO · {} · {}", self.nome, self.texto_da_etapa()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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

    /// Fale com: pede destino, viaja, interage, dialogo, conversou → volta ao
    /// Mestre, entrega, recebe → proxima.
    #[test]
    fn npc_dialogo_entrega_e_proxima() {
        let mut a = AutoMissao::default();
        a.iniciar(501, "Conheça o Alquimista".into(), 0.0);
        pede(&mut a, 0.0);
        let alq = vec2(100.0, 0.0);
        assert!(a
            .destino_recebido(501, destino_tipo::NPC, alq, 3.0, Some(EntityId(7)), 0.1)
            .is_none());
        // Longe: viaja pra perto dele.
        let v = a.passo(ctx(Vec2::ZERO, 0.2));
        assert!(
            matches!(v.as_slice(), [Acao::Viajar(p)] if p.distance(vec2(98.0, 0.0)) < 0.01),
            "{v:?}"
        );
        // Viajando: nao repete.
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
        // Agora o destino e' o Mestre.
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
        // A missao some do log depois de entregue: nao e' erro.
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

    /// Luta: vai a' zona, liga o combate, religa se desligar, e com o objetivo
    /// cumprido desliga tudo e pergunta o destino (o Mestre).
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
            vec![Acao::LigarCombate(zona)]
        );
        let mut c = ctx(vec2(190.0, 0.0), 20.0);
        c.combate_ativo = true;
        c.pronta = true;
        assert_eq!(a.passo(c), vec![Acao::PararAutos]);
        assert_eq!(a.etapa(), Some(Etapa::PedirDestino));
    }

    /// Historia: vai ao ponto-chave e espera la'; a trava de nivel para a auto
    /// missao com aviso.
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
        // Proximo passo e' trava: para e avisa.
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
    fn cancelamentos_e_sem_destino() {
        let mut a = AutoMissao::default();
        a.iniciar(505, "Porto".into(), 0.0);
        a.passo(ctx(Vec2::ZERO, 0.0));
        // Destino desconhecido: para e avisa.
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
        // Sem missao nova depois de entregar: acaba sozinho.
        a.entregue(10.0);
        let v = a.passo(ctx(Vec2::ZERO, 17.0));
        assert!(matches!(v.as_slice(), [Acao::Aviso(_)]));
        assert!(!a.ativo());
        a.iniciar(502, "x".into(), 0.0);
        a.parar();
        assert!(!a.ativo());
    }
    /// TRAVADO NUMA CASA NÃO É "ESTÁ INDO".
    ///
    /// O dono: "fico travado toda hora nas casas" e "o auto missão na verdade
    /// trava em diversas situações". O retry só disparava com a viagem
    /// DESLIGADA, e preso numa quina a viagem segue ativa: o corpo empurrava
    /// a parede até a pessoa mexer no direcional.
    ///
    /// O teste mede os dois lados: parado com viagem ativa tem que refazer a
    /// rota, e ANDANDO não pode refazer — senão o auto missão recalcularia o
    /// caminho a cada três segundos de caminhada normal.
    #[test]
    fn parado_com_viagem_ativa_refaz_a_rota() {
        let mut a = AutoMissao::default();
        a.iniciar(501, "x".into(), 0.0);
        pede(&mut a, 0.0);
        a.destino_recebido(501, destino_tipo::COMBATE, Vec2::new(100.0, 0.0), 8.0, None, 0.1);
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

        // ANDANDO não refaz: o relógio de parado zera a cada passo.
        let mut a2 = AutoMissao::default();
        a2.iniciar(501, "x".into(), 0.0);
        pede(&mut a2, 0.0);
        a2.destino_recebido(501, destino_tipo::COMBATE, Vec2::new(100.0, 0.0), 8.0, None, 0.1);
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

    /// NÓ ESGOTADO: O AUTO "ATIVO" NÃO PROVA QUE ALGO ACONTECE.
    ///
    /// O dono: "quando eu tô coletando árvore e a árvore acaba, trava o auto
    /// missão; só volta quando eu ando, e geralmente já até acabou a missão".
    /// O religar exigia `!coleta_ativa`, e com o nó esgotado a coleta segue
    /// ligada girando no vazio.
    #[test]
    fn coleta_sem_avanco_religa_mesmo_ativa() {
        let mut a = AutoMissao::default();
        a.iniciar(501, "x".into(), 0.0);
        pede(&mut a, 0.0);
        a.destino_recebido(501, destino_tipo::COLETA, Vec2::ZERO, 8.0, None, 0.1);
        let mut c = ctx(Vec2::ZERO, 0.2);
        let acoes = a.passo(c);
        assert!(acoes.contains(&Acao::LigarColeta(Vec2::ZERO)));
        assert_eq!(a.etapa(), Some(Etapa::Coletando));

        // Coleta LIGADA e progresso PARADO: passado o prazo, religa.
        let mut t = 0.3;
        let mut religou = false;
        while t < 0.3 + SEM_AVANCO_S + RELIGA_S + 2.0 {
            c = ctx(Vec2::ZERO, t);
            c.coleta_ativa = true;
            c.progresso = 3;
            let acoes = a.passo(c);
            if acoes.contains(&Acao::LigarColeta(Vec2::ZERO)) {
                assert!(
                    acoes.contains(&Acao::PararAutos),
                    "religou sem parar antes: fica presa no nó velho"
                );
                religou = true;
                break;
            }
            t += 0.5;
        }
        assert!(religou, "a coleta travada no nó esgotado nunca religou");
    }

    /// E COLHENDO DE VERDADE, NÃO MEXE.
    ///
    /// O outro lado do mesmo teste: religar uma coleta que está rendendo
    /// jogaria fora o nó bom a cada seis segundos.
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
