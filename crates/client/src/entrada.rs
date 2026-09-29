//! Typing: the character queue, with REPEAT under control.
//!
//! macroquad stores every `char_event` in a queue and `get_char_pressed()`
//! takes one at a time. The problem is that it stacks the REPEAT event
//! together with the key's own — both become the same `char` in the queue,
//! indistinguishable. A key touched for an instant enters the text field four times.
//!
//! Here the queue is built from the raw event, which still has the `repeat`
//! flag. A new key always enters; a repeat only enters after a wait and
//! spaced out — which is what any text field does, and what makes holding
//! backspace work without deleting everything at once.

use macroquad::input::utils::{register_input_subscriber, repeat_all_miniquad_input};
use macroquad::miniquad::{EventHandler, KeyCode, KeyMods};
use std::collections::HashSet;

/// How long a key has to be held before it starts repeating.
const ESPERA: f64 = 0.42;
/// Interval between repeats once it starts.
const INTERVALO: f64 = 0.035;

pub struct Teclado {
    inscricao: usize,
    /// `MMO_LOG_TECLA=1` prints every raw event with the repeat flag.
    ///
    /// It earns its line: when a text field misbehaves, the question is always
    /// "how many events arrived and what did they say", and that has no answer
    /// from looking at the text that appeared on screen. Read once and not per
    /// key — querying the environment per character is work for nothing.
    registra: bool,
    /// Characters ready for whoever has focus, this frame.
    fila: Vec<char>,
    /// When the repeating key started, and when it delivered the last character.
    /// Both in macroquad clock seconds.
    repetindo_desde: f64,
    ultima_repeticao: f64,
    agora: f64,
    pressionadas: HashSet<KeyCode>,
    tecla_atual: Option<KeyCode>,
    tecla_repetida: bool,
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
            pressionadas: HashSet::new(),
            tecla_atual: None,
            tecla_repetida: false,
        }
    }

    /// Collects what was typed this frame. Call once per frame, before any
    /// screen reads.
    pub fn coleta(&mut self, agora: f64) {
        self.fila.clear();
        self.agora = agora;
        // `repeat_all_miniquad_input` needs `&mut self` twice if `Teclado` itself is
        // the handler, so the handler is a neighbour that writes into what matters.
        let inscricao = self.inscricao;
        let mut ouvinte = Ouvinte { dono: self };
        repeat_all_miniquad_input(&mut ouvinte, inscricao);
    }

    /// What was typed this frame, in order.
    pub fn digitado(&self) -> &[char] {
        &self.fila
    }

    /// Discards whatever was in the queue.
    ///
    /// It is for changing screen without carrying over what was typed on the
    /// previous one: everything the player typed while WALKING (WASD is read by
    /// `is_key_down`, and nobody consumes the queue during play) fell into the
    /// username field all at once the moment the login screen appeared.
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

    fn key_down_event(&mut self, k: KeyCode, _m: KeyMods, repeticao: bool) {
        // Some input paths deliver a repeated key-down without marking repeat.
        // A new keystroke requires releasing the key first.
        self.dono.tecla_repetida = !self.dono.pressionadas.insert(k) || repeticao;
        self.dono.tecla_atual = Some(k);
    }

    fn char_event(&mut self, c: char, _m: KeyMods, repeticao: bool) {
        let repeticao = repeticao || self.dono.tecla_repetida;
        let agora = self.dono.agora;
        if self.dono.registra {
            eprintln!("[tecla] {c:?} repeticao={repeticao} t={agora:.3}");
        }
        if !repeticao {
            self.dono.repetindo_desde = agora;
            self.dono.ultima_repeticao = 0.0;
            self.dono.fila.push(c);
            // Also covers duplicate characters between the same down/up.
            self.dono.tecla_repetida = self.dono.tecla_atual.is_some();
            return;
        }
        // Repeat: only after the wait, and spaced out.
        if agora - self.dono.repetindo_desde < ESPERA {
            return;
        }
        if agora - self.dono.ultima_repeticao < INTERVALO {
            return;
        }
        self.dono.ultima_repeticao = agora;
        self.dono.fila.push(c);
    }

    fn key_up_event(&mut self, k: KeyCode, _m: KeyMods) {
        self.dono.pressionadas.remove(&k);
        // Soltou: a proxima repeticao comeca a contar do zero.
        if self.dono.tecla_atual == Some(k) {
            self.dono.repetindo_desde = f64::MAX;
            self.dono.tecla_atual = None;
            self.dono.tecla_repetida = false;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn teclado() -> Teclado {
        Teclado {
            inscricao: 0,
            registra: false,
            fila: Vec::new(),
            repetindo_desde: 0.0,
            ultima_repeticao: 0.0,
            agora: 1.0,
            pressionadas: HashSet::new(),
            tecla_atual: None,
            tecla_repetida: false,
        }
    }

    #[test]
    fn quatro_eventos_sem_repeat_digitam_uma_letra() {
        let mut t = teclado();
        let mut o = Ouvinte { dono: &mut t };
        for _ in 0..4 {
            o.key_down_event(KeyCode::A, KeyMods::default(), false);
            o.char_event('a', KeyMods::default(), false);
        }
        assert_eq!(t.digitado(), &['a']);
    }

    #[test]
    fn soltar_e_apertar_de_novo_preserva_letras_iguais() {
        let mut t = teclado();
        let mut o = Ouvinte { dono: &mut t };
        for _ in 0..4 {
            o.key_down_event(KeyCode::A, KeyMods::default(), false);
            o.char_event('a', KeyMods::default(), false);
            o.key_up_event(KeyCode::A, KeyMods::default());
        }
        assert_eq!(t.digitado(), &['a', 'a', 'a', 'a']);
    }

    #[test]
    fn segurar_so_repete_depois_da_espera_e_com_intervalo() {
        let mut t = teclado();
        let mut o = Ouvinte { dono: &mut t };
        o.key_down_event(KeyCode::A, KeyMods::default(), false);
        o.char_event('a', KeyMods::default(), false);
        for agora in [1.1, 1.2, 1.43, 1.44, 1.48] {
            o.dono.agora = agora;
            o.key_down_event(KeyCode::A, KeyMods::default(), false);
            o.char_event('a', KeyMods::default(), false);
        }
        assert_eq!(t.digitado(), &['a', 'a', 'a']);
    }
}
