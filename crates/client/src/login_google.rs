//! "Entrar com Google" no cliente.
//!
//! O cliente nunca ve' senha nem segredo do Google: pede ao `web` um login
//! novo, abre a URL no navegador do sistema e fica perguntando ao `web` se o
//! navegador ja' terminou. Quando termina, recebe o username e uma sessao
//! (token) que o servidor de jogo aceita no lugar da senha
//! (`ClientMessage::LoginToken`). Ver docs/LOGIN_GOOGLE.md.
//!
//! A decisao (o que fazer com cada resposta) e' separada da rede pra ser
//! testada sem thread nem servidor.

use std::sync::mpsc::Receiver;

use crate::api::{self, RespostaPoll};

/// De quanto em quanto tempo pergunta ao `web`.
const INTERVALO_POLL_S: f64 = 1.5;
/// Desiste se o navegador nao voltar nisso.
const ESPERA_MAX_S: f64 = 300.0;

#[derive(Clone, Debug, PartialEq)]
pub enum Fase {
    /// Ainda perguntando ao `web` se o Google esta' configurado.
    Consultando,
    /// Servidor sem Google (ou fora do ar): o botao nao aparece.
    Indisponivel,
    Disponivel,
    Iniciando,
    Aguardando {
        state: String,
        desde: f64,
        ultimo_poll: f64,
    },
    Erro(String),
}

/// O que a camada de rede/plataforma precisa fazer.
#[derive(Clone, Debug, PartialEq)]
pub enum Saida {
    AbrirUrl(String),
    Consultar(String),
    Pronto { usuario: String, token: String },
}

/// Uma resposta que chegou.
pub enum Entrada {
    Config(bool),
    Start(Result<(String, String), String>),
    Poll(Result<RespostaPoll, String>),
}

pub struct LoginGoogle {
    pub fase: Fase,
    cfg: Option<Receiver<bool>>,
    start: Option<Receiver<Result<(String, String), String>>>,
    poll: Option<Receiver<Result<RespostaPoll, String>>>,
}

impl Default for LoginGoogle {
    fn default() -> Self {
        Self {
            fase: Fase::Indisponivel,
            cfg: None,
            start: None,
            poll: None,
        }
    }
}

impl LoginGoogle {
    /// Comeca perguntando ao `web` se o Google esta' ligado.
    pub fn consultando() -> Self {
        Self {
            fase: Fase::Consultando,
            cfg: Some(api::google_config()),
            start: None,
            poll: None,
        }
    }

    pub fn disponivel(&self) -> bool {
        !matches!(self.fase, Fase::Consultando | Fase::Indisponivel)
    }

    pub fn aguardando(&self) -> bool {
        matches!(self.fase, Fase::Iniciando | Fase::Aguardando { .. })
    }

    /// Botao clicado.
    pub fn iniciar(&mut self) {
        if matches!(self.fase, Fase::Disponivel | Fase::Erro(_)) {
            self.fase = Fase::Iniciando;
            self.start = Some(api::google_start());
        }
    }

    pub fn cancelar(&mut self) {
        if self.disponivel() {
            self.fase = Fase::Disponivel;
        }
        self.start = None;
        self.poll = None;
    }

    pub fn falhou(&mut self, motivo: &str) {
        self.fase = Fase::Erro(motivo.into());
        self.start = None;
        self.poll = None;
    }

    /// Um quadro: recolhe o que chegou da rede, decide, e dispara o proximo
    /// pedido. Devolve o que a plataforma tem que fazer (abrir URL, logar).
    pub fn tick(&mut self, agora: f64) -> Option<Saida> {
        let mut entradas = Vec::new();
        if let Some(Ok(v)) = self.cfg.as_ref().map(|r| r.try_recv()) {
            entradas.push(Entrada::Config(v));
            self.cfg = None;
        }
        if let Some(Ok(v)) = self.start.as_ref().map(|r| r.try_recv()) {
            entradas.push(Entrada::Start(v));
            self.start = None;
        }
        if let Some(Ok(v)) = self.poll.as_ref().map(|r| r.try_recv()) {
            entradas.push(Entrada::Poll(v));
            self.poll = None;
        }
        let mut saida = None;
        for e in entradas {
            if let Some(s) = self.aplicar(e, agora) {
                saida = Some(s);
            }
        }
        if saida.is_none() && self.poll.is_none() {
            saida = self.decide_poll(agora);
        }
        match saida {
            Some(Saida::Consultar(state)) => {
                self.poll = Some(api::google_poll(&state));
                None
            }
            outra => outra,
        }
    }

    /// A decisao sobre uma resposta. Sem rede: e' o que os testes exercitam.
    pub fn aplicar(&mut self, entrada: Entrada, agora: f64) -> Option<Saida> {
        match entrada {
            Entrada::Config(ligado) => {
                if matches!(self.fase, Fase::Consultando) {
                    self.fase = if ligado {
                        Fase::Disponivel
                    } else {
                        Fase::Indisponivel
                    };
                }
                None
            }
            Entrada::Start(r) => {
                if !matches!(self.fase, Fase::Iniciando) {
                    return None; // cancelado no meio
                }
                match r {
                    Ok((state, url)) => {
                        self.fase = Fase::Aguardando {
                            state,
                            desde: agora,
                            ultimo_poll: agora,
                        };
                        Some(Saida::AbrirUrl(url))
                    }
                    Err(e) => {
                        self.fase = Fase::Erro(format!("Não deu pra iniciar: {e}"));
                        None
                    }
                }
            }
            Entrada::Poll(r) => {
                if !matches!(self.fase, Fase::Aguardando { .. }) {
                    return None;
                }
                match r {
                    Ok(RespostaPoll::Pronto { usuario, token }) => {
                        self.fase = Fase::Disponivel;
                        Some(Saida::Pronto { usuario, token })
                    }
                    Ok(RespostaPoll::Erro(e)) => {
                        self.fase = Fase::Erro(e);
                        None
                    }
                    Ok(RespostaPoll::Expirado) => {
                        self.fase = Fase::Erro("O pedido expirou. Tente de novo.".into());
                        None
                    }
                    // Pendente, ou rede instavel: continua esperando.
                    Ok(RespostaPoll::Pendente) | Err(_) => None,
                }
            }
        }
    }

    /// Hora de perguntar de novo? Tambem desiste no tempo maximo.
    pub fn decide_poll(&mut self, agora: f64) -> Option<Saida> {
        let Fase::Aguardando {
            state,
            desde,
            ultimo_poll,
        } = &mut self.fase
        else {
            return None;
        };
        if agora - *desde > ESPERA_MAX_S {
            self.fase = Fase::Erro("Tempo esgotado esperando o navegador.".into());
            return None;
        }
        if agora - *ultimo_poll < INTERVALO_POLL_S {
            return None;
        }
        *ultimo_poll = agora;
        Some(Saida::Consultar(state.clone()))
    }

    /// Texto de estado pra tela de login.
    pub fn texto(&self) -> Option<String> {
        match &self.fase {
            Fase::Iniciando => Some("Abrindo o Google…".into()),
            Fase::Aguardando { .. } => Some("Aguardando confirmação no navegador…".into()),
            Fase::Erro(e) => Some(e.clone()),
            _ => None,
        }
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    fn pronto_pra_clicar() -> LoginGoogle {
        let mut g = LoginGoogle {
            fase: Fase::Consultando,
            ..Default::default()
        };
        g.aplicar(Entrada::Config(true), 0.0);
        g
    }

    #[test]
    fn botao_so_aparece_com_google_ligado() {
        let mut g = LoginGoogle {
            fase: Fase::Consultando,
            ..Default::default()
        };
        assert!(!g.disponivel());
        g.aplicar(Entrada::Config(false), 0.0);
        assert_eq!(g.fase, Fase::Indisponivel);
        assert!(!g.disponivel());
        assert!(pronto_pra_clicar().disponivel());
    }

    #[test]
    fn fluxo_completo_abre_url_pergunta_e_entrega_a_sessao() {
        let mut g = pronto_pra_clicar();
        g.fase = Fase::Iniciando; // `iniciar` dispara rede; aqui so' a decisao
        let s = g.aplicar(
            Entrada::Start(Ok(("st".into(), "https://accounts.google.com/x".into()))),
            10.0,
        );
        assert_eq!(
            s,
            Some(Saida::AbrirUrl("https://accounts.google.com/x".into()))
        );
        assert!(g.aguardando());
        assert_eq!(g.decide_poll(11.0), None, "espera o intervalo");
        assert_eq!(g.decide_poll(11.6), Some(Saida::Consultar("st".into())));
        assert_eq!(
            g.aplicar(Entrada::Poll(Ok(RespostaPoll::Pendente)), 12.0),
            None
        );
        assert_eq!(
            g.aplicar(Entrada::Poll(Err("rede".into())), 13.0),
            None,
            "rede instavel nao derruba"
        );
        assert!(g.aguardando());
        let s = g.aplicar(
            Entrada::Poll(Ok(RespostaPoll::Pronto {
                usuario: "Joao".into(),
                token: "t".into(),
            })),
            14.0,
        );
        assert_eq!(
            s,
            Some(Saida::Pronto {
                usuario: "Joao".into(),
                token: "t".into()
            })
        );
        assert_eq!(g.fase, Fase::Disponivel);
    }

    #[test]
    fn erros_cancelamento_e_tempo_esgotado() {
        let mut g = pronto_pra_clicar();
        g.fase = Fase::Iniciando;
        g.aplicar(Entrada::Start(Err("sem rede".into())), 0.0);
        assert!(matches!(g.fase, Fase::Erro(_)));
        assert!(g.texto().unwrap().contains("sem rede"));

        g.fase = Fase::Aguardando {
            state: "s".into(),
            desde: 0.0,
            ultimo_poll: 0.0,
        };
        g.aplicar(Entrada::Poll(Ok(RespostaPoll::Expirado)), 1.0);
        assert!(matches!(g.fase, Fase::Erro(_)));

        g.fase = Fase::Aguardando {
            state: "s".into(),
            desde: 0.0,
            ultimo_poll: 0.0,
        };
        assert_eq!(g.decide_poll(ESPERA_MAX_S + 1.0), None);
        assert!(matches!(g.fase, Fase::Erro(_)));

        g.fase = Fase::Aguardando {
            state: "s".into(),
            desde: 0.0,
            ultimo_poll: 0.0,
        };
        g.cancelar();
        assert_eq!(g.fase, Fase::Disponivel);
        // Resposta atrasada de um pedido cancelado e' ignorada.
        assert_eq!(
            g.aplicar(
                Entrada::Poll(Ok(RespostaPoll::Pronto {
                    usuario: "x".into(),
                    token: "y".into()
                })),
                2.0
            ),
            None
        );
    }
}
