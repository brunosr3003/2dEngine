//! Digitação: a fila de caracteres, com a REPETIÇÃO sob controle.
//!
//! A macroquad guarda todo `char_event` numa fila e `get_char_pressed()` tira
//! um por vez. O problema e' que ela empilha o evento de REPETICAO junto com o
//! da tecla — os dois viram o mesmo `char` na fila, indistinguiveis. Uma tecla
//! encostada por um instante entra quatro vezes no campo de texto.
//!
//! Aqui a fila e' montada do evento cru, que ainda tem a bandeira `repeat`.
//! Tecla nova entra sempre; repeticao entra so' depois de uma espera e
//! espacada — que e' o que qualquer campo de texto faz, e o que faz segurar o
//! apagar funcionar sem apagar tudo de uma vez.

use macroquad::input::utils::{register_input_subscriber, repeat_all_miniquad_input};
use macroquad::miniquad::{EventHandler, KeyCode, KeyMods};

/// Quanto uma tecla precisa ficar apertada antes de comecar a repetir.
const ESPERA: f64 = 0.42;
/// Intervalo entre repeticoes depois que ela comeca.
const INTERVALO: f64 = 0.035;

pub struct Teclado {
    inscricao: usize,
    /// `MMO_LOG_TECLA=1` imprime cada evento cru com a bandeira de repeticao.
    ///
    /// Vale a linha: quando o campo de texto se comporta mal, a pergunta e'
    /// sempre "quantos eventos chegaram e o que eles diziam", e ela nao tem
    /// resposta olhando pro texto que apareceu na tela. Lido uma vez e nao
    /// por tecla — consultar o ambiente por caractere e' trabalho por nada.
    registra: bool,
    /// Caracteres prontos pra quem estiver com o foco, neste quadro.
    fila: Vec<char>,
    /// Quando a tecla que esta' repetindo comecou, e quando ela entregou o
    /// ultimo caractere. Ambos em segundos de relogio da macroquad.
    repetindo_desde: f64,
    ultima_repeticao: f64,
    agora: f64,
}

impl Teclado {
    pub fn novo() -> Self {
        Self {
            inscricao: register_input_subscriber(),
            registra: std::env::var("MMO_LOG_TECLA").is_ok(),
            fila: Vec::new(),
            repetindo_desde: 0.0,
            ultima_repeticao: 0.0,
            agora: 0.0,
        }
    }

    /// Recolhe o que foi teclado neste quadro. Chamar uma vez por quadro,
    /// antes de qualquer tela ler.
    pub fn coleta(&mut self, agora: f64) {
        self.fila.clear();
        self.agora = agora;
        // `repeat_all_miniquad_input` precisa de `&mut self` duas vezes se o
        // proprio `Teclado` for o handler, entao o handler e' um vizinho que
        // escreve no que interessa.
        let inscricao = self.inscricao;
        let mut ouvinte = Ouvinte { dono: self };
        repeat_all_miniquad_input(&mut ouvinte, inscricao);
    }

    /// O que foi teclado neste quadro, em ordem.
    pub fn digitado(&self) -> &[char] {
        &self.fila
    }

    /// Descarta o que estava na fila.
    ///
    /// Serve pra trocar de tela sem levar junto o que foi teclado na anterior:
    /// tudo que o jogador teclou ANDANDO (WASD e' lido por `is_key_down`, e
    /// ninguem consome a fila durante o jogo) caia de uma vez no campo de
    /// usuario assim que a tela de login aparecia.
    pub fn limpa(&mut self) {
        self.fila.clear();
        while macroquad::input::get_char_pressed().is_some() {}
    }
}

struct Ouvinte<'a> {
    dono: &'a mut Teclado,
}

impl EventHandler for Ouvinte<'_> {
    fn update(&mut self) {}
    fn draw(&mut self) {}

    fn char_event(&mut self, c: char, _m: KeyMods, repeticao: bool) {
        let agora = self.dono.agora;
        if self.dono.registra {
            eprintln!("[tecla] {c:?} repeticao={repeticao} t={agora:.3}");
        }
        if !repeticao {
            self.dono.repetindo_desde = agora;
            self.dono.ultima_repeticao = 0.0;
            self.dono.fila.push(c);
            return;
        }
        // Repeticao: so' depois da espera, e espacada.
        if agora - self.dono.repetindo_desde < ESPERA {
            return;
        }
        if agora - self.dono.ultima_repeticao < INTERVALO {
            return;
        }
        self.dono.ultima_repeticao = agora;
        self.dono.fila.push(c);
    }

    fn key_up_event(&mut self, _k: KeyCode, _m: KeyMods) {
        // Soltou: a proxima repeticao comeca a contar do zero.
        self.dono.repetindo_desde = f64::MAX;
    }
}
