//! The game's messages ("Quest complete", "Auto gather: attacked") as a
//! passing NOTICE, and not as a chat box.
//!
//! There used to be a fixed panel in the left corner, taking the spot where
//! the thumb looks for the joystick. The owner asked for it to go: the
//! information stays, now as loose text that disappears on its own.

/// How long each line stays on screen.
const VIDA_S: f64 = 9.0;
/// How many lines show at once.
const LINHAS: usize = 3;
/// Cap on the kept history (the log panel, if it comes back, reads from here).
const GUARDADAS: usize = 20;

#[derive(Default)]
pub struct Avisos {
    linhas: Vec<(String, f64)>,
}

impl Avisos {
    /// The same signature as the `Vec::push` that used to be here: everywhere
    /// that announced something stays the same.
    pub fn push(&mut self, texto: String) {
        self.linhas.push((texto, macroquad::time::get_time()));
        if self.linhas.len() > GUARDADAS {
            self.linhas.remove(0);
        }
    }

    /// Clears everything (switching character, leaving the world).
    pub fn clear(&mut self) {
        self.linhas.clear();
    }

    /// The last lines still alive, oldest to newest.
    pub fn recentes(&self, agora: f64) -> Vec<&str> {
        self.linhas
            .iter()
            .filter(|(_, t)| agora - t < VIDA_S)
            .rev()
            .take(LINHAS)
            .map(|(l, _)| l.as_str())
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect()
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn guarda_o_historico_e_mostra_so_as_recentes() {
        let mut a = Avisos::default();
        for i in 0..GUARDADAS + 5 {
            a.linhas.push((format!("linha {i}"), i as f64));
        }
        // The cap only applies via `push`; here what matters is the time window.
        let agora = (GUARDADAS + 4) as f64;
        let vistas = a.recentes(agora);
        assert_eq!(vistas.len(), LINHAS, "mostra as tres ultimas");
        assert_eq!(*vistas.last().unwrap(), format!("linha {}", GUARDADAS + 4));
        assert!(a.recentes(agora + VIDA_S).is_empty(), "somem sozinhas");
    }
}
