//! Auto gathering BY NODE (X, or the button above COMBAT; right click opens
//! the configuration): the server points at the nearest live node of the
//! ticked types within the radius from where it was switched on; the
//! character walks there, sends gather and stays until it runs out; then asks
//! for the next. With no node in the radius, it waits for the respawn without
//! leaving the area ("Waiting for resources…").
//!
//! Standing nearby yields nothing: gathering is from the chosen node (docs/COLETA.md).
use macroquad::prelude::*;

use crate::hud_estilo as estilo;

/// Perto disto do ponto de coleta, chegou.
const CHEGOU: f32 = shared::COLETA_TOLERANCIA_CHEGADA;
/// Asked (for a node or to gather) and the server did not answer: ask again.
const REPEDE_S: f64 = 4.0;
/// Nothing in the radius: wait this long before asking again.
const APOS_FALHA_S: f64 = 6.0;
/// O servidor recusou a coleta (longe, esgotou no caminho): respiro curto.
const APOS_RECUSA_S: f64 = 1.5;
/// The travel ended far from the point: send again after this.
const RELIGA_S: f64 = 1.5;

#[derive(Debug, PartialEq)]
pub enum Acao {
    Nada,
    /// Ask for the nearest node of the types, within `raio` of `centro`.
    PedirNo {
        tipos: [bool; 5],
        energia: bool,
        raio: f32,
        centro: Vec2,
    },
    /// Pedir o no' de UM tipo em volta de um ponto (o "Ir" do mapa).
    PedirNoDoTipo {
        tipo: u8,
        perto: Vec2,
    },
    Ir(Vec2),
    Coletar(u32),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Etapa {
    Procurar,
    Esperando,
    Indo,
    Coletando,
    Aguardando,
}

pub struct AutoColeta {
    pub centro: Option<Vec2>,
    /// Configuracao do jogador (salva nas preferencias).
    pub tipos: [bool; 5],
    pub energia: bool,
    pub raio: f32,
    /// Types of the quest in progress: they ignore the configuration.
    /// A quest commanding: (body types, accepts Energy). Energy is its own
    /// field because it is not a tier of `tipos` — it is a separate resource,
    /// with its own tutorial step.
    forcados: Option<([bool; 5], bool)>,
    /// Gathering of ONE type around a point (the map's "Go" on a region).
    pub filtro: Option<(u8, Vec2)>,
    etapa: Etapa,
    /// (column, where to stand, node's center, type).
    alvo: Option<(u32, Vec2, Vec2, u8)>,
    desde: f64,
    /// "Nothing is changing" — see `parado.rs`.
    parado: crate::parado::Parado,
    confirmou: bool,
    pub falhas: u32,
    /// The server paused for a full bag: it stays at the node and waits for space
    /// (the server is what resumes, when the bag changes).
    bolsa_cheia: bool,
    /// Configuration: took a hit from a creature while gathering, drops the node,
    /// kills the creature and goes back to gathering. Saved in the preferences;
    /// on by default.
    pub defender: bool,
}

impl Default for AutoColeta {
    fn default() -> Self {
        Self {
            centro: None,
            tipos: [true; 5],
            energia: true,
            raio: shared::COLETA_RAIO_AUTO_PADRAO,
            forcados: None,
            filtro: None,
            etapa: Etapa::Procurar,
            alvo: None,
            desde: f64::MIN,
            parado: crate::parado::Parado::default(),
            confirmou: false,
            falhas: 0,
            bolsa_cheia: false,
            defender: true,
        }
    }
}

/// O botao AUTO COLETA, a' esquerda do COMBATE (ver `hud_layout`).
pub fn retangulo() -> Rect {
    crate::hud_layout::atual().auto_coleta
}

pub fn pega_mouse() -> bool {
    retangulo().contains(Vec2::from(mouse_position()))
}

/// The cog in the button's corner: opens the configuration with a touch or a
/// left click (on iOS there is no right button).
pub fn engrenagem() -> Rect {
    engrenagem_de(retangulo())
}

/// The button's top right corner, inside it — it creates no new rectangle in
/// the HUD (the overlap test still holds) and does not cover the middle.
pub fn engrenagem_de(r: Rect) -> Rect {
    let t = (r.w * 0.32).max(22.0).min(r.w * 0.45);
    Rect::new(r.x + r.w - t, r.y, t, t)
}

impl AutoColeta {
    pub fn ativo(&self) -> bool {
        self.centro.is_some()
    }

    /// Switches on with the player's configuration.
    pub fn ligar(&mut self, p: Vec2, _agora: f64) {
        self.reinicia(Some(p));
        self.forcados = None;
    }

    /// Switches on for a quest: its types, not the configuration's.
    pub fn ligar_missao(&mut self, p: Vec2, tipos: [bool; 5], energia: bool, _agora: f64) {
        self.reinicia(Some(p));
        self.forcados = Some((tipos, energia));
    }

    /// Desliga. A configuracao (tipos e raio) fica.
    pub fn parar(&mut self) {
        self.reinicia(None);
        self.forcados = None;
    }

    fn reinicia(&mut self, centro: Option<Vec2>) {
        self.centro = centro;
        self.filtro = None;
        self.etapa = Etapa::Procurar;
        self.alvo = None;
        self.desde = f64::MIN;
        self.confirmou = false;
        self.falhas = 0;
    }

    /// Goes back to looking for a node from scratch, keeping where it was
    /// switched on, the quest and the filter. After dropping the node to defend itself.
    pub fn retomar(&mut self) {
        self.etapa = Etapa::Procurar;
        self.alvo = None;
        self.desde = f64::MIN;
        self.confirmou = false;
    }

    /// The types that count now: the quest's, if there is one.
    pub fn tipos_efetivos(&self) -> [bool; 5] {
        self.forcados.map_or(self.tipos, |(t, _)| t)
    }

    /// Accepts Energy now? With a quest commanding, it decides — the tutorial
    /// step asks for Energy and ONLY Energy, and the other gathering quests
    /// cannot spend the player's vein without them saying so.
    pub fn energia_efetiva(&self) -> bool {
        self.forcados.map_or(self.energia, |(_, e)| e)
    }

    /// Resposta do servidor a um pedido de no'.
    pub fn no_recebido(&mut self, no: Option<(u32, Vec2, Vec2, u8)>, agora: f64) -> Acao {
        if !self.ativo() || self.etapa != Etapa::Esperando {
            return Acao::Nada;
        }
        self.desde = agora;
        match no {
            Some(n) => {
                self.alvo = Some(n);
                self.falhas = 0;
                self.etapa = Etapa::Indo;
                Acao::Ir(n.1)
            }
            None => {
                self.falhas += 1;
                self.etapa = Etapa::Aguardando;
                Acao::Nada
            }
        }
    }

    /// `ColetaEstado` do servidor: coletando, ou parou (esgotou, recusou).
    pub fn coletando_confirmado(&self) -> bool {
        self.etapa == Etapa::Coletando && self.confirmou
    }

    pub fn estado_coleta(&mut self, tipo: u8, pausado: bool, agora: f64) {
        if !self.ativo() {
            return;
        }
        self.bolsa_cheia = pausado && tipo != shared::protocol::COLETA_PARADA;
        if tipo != shared::protocol::COLETA_PARADA {
            if self.etapa == Etapa::Coletando {
                self.confirmou = true;
            }
            return;
        }
        if self.etapa == Etapa::Coletando {
            // Gathered and stopped: the node ran out, move to the next. Did not even
            // start: refused (too far, ran out on the way) — a breath and look for another.
            self.etapa = if self.confirmou {
                Etapa::Procurar
            } else {
                Etapa::Aguardando
            };
            if !self.confirmou {
                self.desde = agora - (APOS_FALHA_S - APOS_RECUSA_S);
            }
            self.alvo = None;
            self.confirmou = false;
        }
    }

    /// Um quadro.
    pub fn passo(&mut self, eu: Vec2, agora: f64, viajando: bool) -> Acao {
        self.parado.acompanha(eu, agora);
        let Some(centro) = self.centro else {
            return Acao::Nada;
        };
        match self.etapa {
            Etapa::Procurar => {
                self.etapa = Etapa::Esperando;
                self.desde = agora;
                match self.filtro {
                    Some((tipo, perto)) => Acao::PedirNoDoTipo { tipo, perto },
                    None => Acao::PedirNo {
                        tipos: self.tipos_efetivos(),
                        energia: self.energia_efetiva(),
                        raio: self.raio,
                        centro,
                    },
                }
            }
            Etapa::Esperando => {
                if agora - self.desde > REPEDE_S {
                    self.etapa = Etapa::Procurar;
                    return self.passo(eu, agora, viajando);
                }
                Acao::Nada
            }
            Etapa::Aguardando => {
                if agora - self.desde >= APOS_FALHA_S {
                    self.etapa = Etapa::Procurar;
                    return self.passo(eu, agora, viajando);
                }
                Acao::Nada
            }
            Etapa::Indo => {
                let Some((coluna, onde, _, _)) = self.alvo else {
                    self.etapa = Etapa::Procurar;
                    return Acao::Nada;
                };
                if eu.distance(onde) <= CHEGOU {
                    self.etapa = Etapa::Coletando;
                    self.confirmou = false;
                    self.desde = agora;
                    return Acao::Coletar(coluna);
                }
                // TRAVADO CONTA MESMO "VIAJANDO".
                //
                // The owner: "auto gathering quest, he just stands in front of the
                // tree". This was it: the retry required travel to be OFF, and leaning
                // on the trunk it stays active — so it never tried again and never
                // reached gathering range.
                //
                // The same defect, word for word, as `auto_missao.rs`. See `parado.rs`.
                let travado = self.parado.travado(agora);
                if (!viajando || travado) && agora - self.desde >= RELIGA_S {
                    self.desde = agora;
                    if travado {
                        self.parado.zera(agora);
                        // Genuinely still: the node may be behind the trunk. Looks for ANOTHER
                        // instead of insisting on the same one.
                        self.etapa = Etapa::Procurar;
                        return self.passo(eu, agora, viajando);
                    }
                    return Acao::Ir(onde);
                }
                Acao::Nada
            }
            Etapa::Coletando => {
                if !self.confirmou && agora - self.desde > REPEDE_S {
                    self.etapa = Etapa::Procurar;
                    return self.passo(eu, agora, viajando);
                }
                Acao::Nada
            }
        }
    }

    /// The button. The key (X) only shows with Alt; the state goes to the single strip.
    pub fn desenha(&self) {
        let r = retangulo();
        let c = r.center();
        let raio = r.w * 0.45;
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
        if !crate::icones_ui::ui("auto_coleta", c - vec2(0.0, raio * 0.18), raio * 0.95, cor) {
            estilo::icone(3, c - vec2(0.0, raio * 0.18), raio * 0.40, cor);
        }
        estilo::texto_centro_forte(
            c.x,
            c.y + raio * 0.66,
            if self.ativo() { "AUTO" } else { "COLETA" },
            10,
            cor,
        );
        crate::hud_layout::chip(r, "X");
        // A engrenagem: roda dentada simples (anel + seis dentes).
        let g = engrenagem_de(r);
        let gc = g.center();
        let gr = g.w * 0.34;
        let sobre_g = g.contains(Vec2::from(mouse_position()));
        let cor_g = if sobre_g {
            estilo::ACENTO
        } else {
            estilo::OURO
        };
        draw_circle(gc.x, gc.y + 1.5, gr + 4.5, Color::new(0.0, 0.0, 0.0, 0.35));
        draw_circle(gc.x, gc.y, gr + 4.0, estilo::FUNDO_BAIXO);
        draw_circle_lines(gc.x, gc.y, gr + 4.0, 1.0, estilo::BORDA_FORTE);
        if !crate::icones_ui::ui("engrenagem", gc, (gr + 4.0) * 1.55, cor_g) {
            for k in 0..8 {
                let a = k as f32 / 8.0 * std::f32::consts::TAU;
                let d = vec2(a.cos(), a.sin());
                estilo::traco(gc + d * gr * 0.62, gc + d * (gr + 1.5), 2.6, cor_g);
            }
            draw_circle_lines(gc.x, gc.y, gr * 0.62, 2.2, cor_g);
            draw_circle(gc.x, gc.y, gr * 0.22, cor_g);
        }
    }

    /// The status strip's text, with gathering on.
    pub fn faixa(&self, _eu: Option<Vec2>) -> Option<&'static str> {
        if !self.ativo() {
            return None;
        }
        Some(match self.etapa {
            Etapa::Procurar | Etapa::Esperando => "AUTO COLETA · PROCURANDO",
            Etapa::Indo => "AUTO COLETA · INDO AO RECURSO",
            Etapa::Coletando if self.bolsa_cheia => "AUTO COLETA · BOLSA CHEIA",
            Etapa::Coletando => "AUTO COLETA · COLETANDO",
            Etapa::Aguardando => "AUTO COLETA · AGUARDANDO RECURSOS…",
        })
    }
}

/// The types a gathering quest asks for (0 wood, 1..4 stone). A quest that is
/// not node gathering accepts everything.
/// What the quest asks to gather: (body types, accepts Energy).
///
/// The Energy tutorial is the only one that wants ONLY Energy: sending the
/// logs and stones along would make the auto stop at the first stump on the
/// way and the step would never advance.
pub fn tipos_da_missao(def: &shared::quests::QuestDef) -> ([bool; 5], bool) {
    use shared::quests::{objective_kind, tutorial};
    if def.obj_kind == objective_kind::TUTORIAL && matches!(def.obj_target,
        tutorial::COLETA_ENERGIA | tutorial::PONTO_ATRIBUTO | tutorial::EVOLUIR_SKILL) {
        return ([false; 5], true);
    }
    if def.obj_kind != objective_kind::GATHER {
        return ([true; 5], true);
    }
    // A stone/tree quest does not spend the player's Energy vein.
    (
        std::array::from_fn(|t| shared::quests::alvo_de_coleta::conta(def.obj_target, t as u8)),
        false,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The Energy tutorial wants ONLY Energy: with logs and stone on, the auto
    /// stopped at the first stump on the way and the step never advanced. And
    /// the reverse holds too — a stone quest does not spend the Energy vein.
    #[test]
    fn a_missao_manda_no_que_o_auto_coleta() {
        use shared::quests::{objective_kind, tutorial};
        let def = |kind: u8, alvo: u16| shared::quests::QuestDef {
            obj_kind: kind,
            obj_target: alvo,
            ..*shared::quests::quest_by_id(796).expect("796 existe")
        };

        let (tipos, energia) =
            tipos_da_missao(&def(objective_kind::TUTORIAL, tutorial::COLETA_ENERGIA));
        assert!(energia, "o passo pede Energia");
        assert!(!tipos.iter().any(|t| *t), "e nada mais: {tipos:?}");

        // A GESTURE tutorial gathers nothing in particular: it keeps the default.
        let (t_gesto, e_gesto) =
            tipos_da_missao(&def(objective_kind::TUTORIAL, tutorial::AUTO_COLETA));
        assert!(t_gesto.iter().all(|t| *t) && e_gesto);

        // A tree quest: logs only, and no Energy.
        let (t_arv, e_arv) = tipos_da_missao(&def(
            objective_kind::GATHER,
            shared::quests::alvo_de_coleta::ARVORE,
        ));
        assert!(t_arv[0], "tronco e' o tier 0");
        assert!(!t_arv[1..].iter().any(|t| *t), "pedra nao conta");
        assert!(!e_arv, "missao de arvore nao gasta o veio de Energia");

        // And the forced one commands even with the configuration saying otherwise.
        let mut a = AutoColeta::default();
        a.tipos = [true; 5];
        a.energia = true;
        a.ligar_missao(Vec2::ZERO, [false; 5], true, 0.0);
        assert_eq!(a.tipos_efetivos(), [false; 5]);
        assert!(a.energia_efetiva());
        a.ligar_missao(Vec2::ZERO, [true, false, false, false, false], false, 0.0);
        assert!(!a.energia_efetiva(), "a missao desligou a Energia");
        a.parar();
        assert_eq!(a.tipos_efetivos(), [true; 5], "solta, volta a configuracao");
        assert!(a.energia_efetiva());
    }

    #[test]
    fn recusa_antes_de_coletar_nao_e_recurso_esgotado() {
        let mut a = AutoColeta::default();
        a.ligar(Vec2::ZERO, 0.0);
        a.passo(Vec2::ZERO, 0.0, false);
        a.no_recebido(Some((77, Vec2::ZERO, Vec2::X, 2)), 0.1);
        assert_eq!(a.passo(Vec2::ZERO, 0.2, true), Acao::Coletar(77));
        assert!(!a.coletando_confirmado());
        a.estado_coleta(shared::protocol::COLETA_PARADA, false, 0.3);
        assert!(!a.coletando_confirmado());
        a.passo(Vec2::ZERO, 2.0, false);
        a.no_recebido(Some((78, Vec2::ZERO, Vec2::X, 2)), 2.1);
        a.passo(Vec2::ZERO, 2.2, false);
        a.estado_coleta(2, false, 2.3);
        assert!(a.coletando_confirmado());
    }

    #[test]
    fn procura_vai_coleta_e_troca_ao_esgotar() {
        let mut a = AutoColeta::default();
        assert_eq!(a.passo(Vec2::ZERO, 0.0, false), Acao::Nada, "desligado");
        a.tipos = [false, true, true, false, false];
        a.raio = 40.0;
        a.ligar(Vec2::ZERO, 0.0);
        assert_eq!(
            a.passo(Vec2::ZERO, 0.0, false),
            Acao::PedirNo {
                tipos: [false, true, true, false, false],
                energia: true,
                raio: 40.0,
                centro: Vec2::ZERO
            }
        );
        assert_eq!(
            a.passo(Vec2::ZERO, 0.1, false),
            Acao::Nada,
            "esperando resposta"
        );
        let onde = vec2(20.0, 0.0);
        assert_eq!(
            a.no_recebido(Some((77, onde, vec2(21.0, 0.0), 2)), 0.2),
            Acao::Ir(onde)
        );
        assert_eq!(a.passo(vec2(10.0, 0.0), 1.0, true), Acao::Nada, "viajando");
        assert_eq!(
            a.passo(vec2(15.0, 0.0), 2.0, false),
            Acao::Ir(onde),
            "parou longe: manda de novo"
        );
        assert_eq!(
            a.passo(onde, 3.0, false),
            Acao::Coletar(77),
            "chegou: coleta o no'"
        );
        a.estado_coleta(2, false, 3.1);
        a.estado_coleta(2, true, 3.15);
        assert_eq!(a.faixa(None), Some("AUTO COLETA · BOLSA CHEIA"));
        for k in 0..5 {
            assert_eq!(
                a.passo(onde, 3.16 + k as f64 * 0.01, false),
                Acao::Nada,
                "bolsa cheia: fica no no'"
            );
        }
        a.estado_coleta(2, false, 3.19);
        assert_eq!(a.faixa(None), Some("AUTO COLETA · COLETANDO"));
        for k in 0..20 {
            assert_eq!(
                a.passo(onde, 3.2 + k as f64, false),
                Acao::Nada,
                "coletando: fica"
            );
        }
        // Ran out: the server stops, and the auto looks for the next.
        a.estado_coleta(shared::protocol::COLETA_PARADA, false, 30.0);
        assert!(matches!(a.passo(onde, 30.1, false), Acao::PedirNo { .. }));
    }

    #[test]
    fn sem_no_aguarda_recursos_e_recusa_nao_vira_loop() {
        let mut a = AutoColeta::default();
        a.ligar(Vec2::ZERO, 0.0);
        assert!(matches!(
            a.passo(Vec2::ZERO, 0.0, false),
            Acao::PedirNo { .. }
        ));
        assert_eq!(a.no_recebido(None, 0.5), Acao::Nada);
        assert_eq!(a.faixa(None), Some("AUTO COLETA · AGUARDANDO RECURSOS…"));
        assert_eq!(a.passo(Vec2::ZERO, 3.0, false), Acao::Nada);
        assert!(matches!(
            a.passo(Vec2::ZERO, 7.0, false),
            Acao::PedirNo { .. }
        ));
        // Refused on arrival (without having gathered): a short breath before asking.
        a.no_recebido(Some((5, Vec2::ZERO, Vec2::X, 0)), 7.1);
        assert_eq!(a.passo(Vec2::ZERO, 7.2, false), Acao::Coletar(5));
        a.estado_coleta(shared::protocol::COLETA_PARADA, false, 7.3);
        assert_eq!(a.passo(Vec2::ZERO, 7.4, false), Acao::Nada, "respiro");
        assert!(matches!(
            a.passo(Vec2::ZERO, 9.0, false),
            Acao::PedirNo { .. }
        ));
        // Parar mantem a configuracao; resposta tardia e' ignorada.
        a.raio = 80.0;
        a.parar();
        assert_eq!(a.raio, 80.0);
        assert_eq!(
            a.no_recebido(Some((1, Vec2::ONE, Vec2::ONE, 1)), 10.0),
            Acao::Nada
        );
    }

    #[test]
    fn a_engrenagem_fica_no_canto_do_botao_sem_cobrir_o_miolo() {
        for r in [
            Rect::new(100.0, 200.0, 88.0, 88.0),
            Rect::new(0.0, 0.0, 44.0, 44.0),
        ] {
            let g = engrenagem_de(r);
            assert!(
                r.contains(vec2(g.x + 0.1, g.y + 0.1))
                    && r.contains(vec2(g.x + g.w - 0.1, g.y + g.h - 0.1)),
                "dentro do botao"
            );
            assert!(
                !g.contains(r.center()),
                "nao cobre o miolo (o toque curto liga o AUTO)"
            );
        }
    }

    #[test]
    fn missao_ignora_a_configuracao_e_mapa_pede_o_tipo() {
        let mut a = AutoColeta::default();
        a.tipos = [true, false, false, false, false];
        a.ligar_missao(Vec2::ZERO, [false, true, true, true, true], false, 0.0);
        assert!(matches!(
            a.passo(Vec2::ZERO, 0.0, false),
            Acao::PedirNo {
                tipos: [false, true, true, true, true],
                ..
            }
        ));
        a.ligar(Vec2::ZERO, 0.0);
        a.filtro = Some((3, vec2(9.0, 9.0)));
        assert_eq!(
            a.passo(Vec2::ZERO, 0.0, false),
            Acao::PedirNoDoTipo {
                tipo: 3,
                perto: vec2(9.0, 9.0)
            }
        );
    }
    /// STANDING IN FRONT OF THE TREE IS NOT "GOING".
    ///
    /// The owner: "auto gathering quest, he just stands in front of the tree".
    /// The retry of walking to the node only fired with travel OFF, and leaning
    /// on the trunk it stays active: the character stood planted, never reaching
    /// gathering range.
    ///
    /// Genuinely still, it looks for ANOTHER node — the chosen one may be
    /// behind the trunk, and insisting on it is standing planted again.
    ///
    /// The test measures both sides: still acts, WALKING does not (otherwise
    /// gathering would change target every three seconds of walking and would
    /// never harvest anything).
    #[test]
    fn parado_na_frente_da_arvore_procura_outro_no() {
        let mut a = AutoColeta::default();
        a.ligar(Vec2::ZERO, 0.0);
        // Receives a distant node, and enters Indo.
        let _ = a.passo(Vec2::ZERO, 0.1, false);
        let _ = a.no_recebido(
            Some((7, Vec2::new(30.0, 0.0), Vec2::new(30.0, 0.0), 0)),
            0.2,
        );

        // TRAVELLING and still: past the deadline, it reacts.
        let mut t = 0.3;
        let mut reagiu = false;
        while t < 0.3 + crate::parado::TRAVADO_S + RELIGA_S + 2.0 {
            let acao = a.passo(Vec2::ZERO, t, true);
            if !matches!(acao, Acao::Nada) {
                reagiu = true;
                break;
            }
            t += 0.2;
        }
        assert!(reagiu, "parado com viagem ativa e não fez nada");

        // WALKING: does not change target.
        let mut b = AutoColeta::default();
        b.ligar(Vec2::ZERO, 0.0);
        let _ = b.passo(Vec2::ZERO, 0.1, false);
        let _ = b.no_recebido(
            Some((7, Vec2::new(300.0, 0.0), Vec2::new(300.0, 0.0), 0)),
            0.2,
        );
        let mut t = 0.3;
        let mut x = 0.0f32;
        while t < 30.0 {
            x += 3.0;
            let acao = b.passo(Vec2::new(x, 0.0), t, true);
            assert!(
                matches!(acao, Acao::Nada),
                "mexeu no alvo de quem está andando (t={t}, x={x})"
            );
            t += 0.5;
        }
    }
}
