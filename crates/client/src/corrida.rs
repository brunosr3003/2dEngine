//! Automatic running: going somewhere alone (map travel, auto quest, click
//! route), after a while walking without stopping the character starts to run.
//!
//! The client only sets the input's `SPRINT` bit. Whether running is possible
//! is the server's call, as with Shift: with no stamina it does not run, and
//! it starts running again on its own when stamina recovers, without having to stop.

use macroquad::prelude::*;

/// Walking continuously for this long, it runs.
const ESPERA_S: f32 = 1.5;
/// Stopped for longer than this, the counter zeroes.
const PARADO_S: f32 = 0.3;
/// Below this speed (u/s) it counts as stopped.
const MEXENDO: f32 = 0.5;

#[derive(Default)]
pub struct Corrida {
    escolha: Option<bool>,
    andando: f32,
    parado: f32,
    ultima: Option<Vec2>,
}

impl Corrida {
    /// A manual choice wins over the travel's automatic run.
    pub fn ativa(&self, automatico: bool) -> bool {
        self.escolha.unwrap_or(automatico)
    }

    pub fn alternar(&mut self, automatico: bool) {
        self.escolha = Some(!self.ativa(automatico));
    }

    /// One frame. `automatico` = the character is going somewhere alone and
    /// nothing is holding them (no dialogue open, no auto combat or gathering
    /// stopped in the zone). Returns whether they should run.
    pub fn atualiza(&mut self, pos: Option<Vec2>, automatico: bool, dt: f32) -> bool {
        let mexeu = match (pos, self.ultima) {
            (Some(p), Some(u)) if dt > 0.0 => p.distance(u) / dt > MEXENDO,
            _ => false,
        };
        self.ultima = pos;
        if !automatico {
            self.andando = 0.0;
            self.parado = 0.0;
            return false;
        }
        if mexeu {
            self.andando += dt;
            self.parado = 0.0;
        } else {
            self.parado += dt;
            if self.parado > PARADO_S {
                self.andando = 0.0;
            }
        }
        self.andando >= ESPERA_S
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn toque_alterna_e_desligar_impede_auto_de_religar() {
        let mut c = Corrida::default();
        assert!(!c.ativa(false));
        c.alternar(false);
        assert!(c.ativa(false));
        c.alternar(false);
        assert!(!c.ativa(true));
        c.alternar(true);
        assert!(c.ativa(false));
        let mut c = Corrida::default();
        c.alternar(true);
        assert!(!c.ativa(true));
    }

    /// Walks `quadros` frames at 4 u/s in a straight line.
    fn anda(c: &mut Corrida, x: &mut f32, quadros: u32, automatico: bool) -> bool {
        let dt = 1.0 / 60.0;
        let mut r = false;
        for _ in 0..quadros {
            *x += 4.0 * dt;
            r = c.atualiza(Some(vec2(*x, 0.0)), automatico, dt);
        }
        r
    }

    fn fica(c: &mut Corrida, x: f32, quadros: u32) -> bool {
        let mut r = false;
        for _ in 0..quadros {
            r = c.atualiza(Some(vec2(x, 0.0)), true, 1.0 / 60.0);
        }
        r
    }

    #[test]
    fn so_corre_depois_de_um_segundo_e_meio_andando() {
        let (mut c, mut x) = (Corrida::default(), 0.0);
        assert!(!anda(&mut c, &mut x, 85, true), "correu antes de 1,5 s");
        assert!(anda(&mut c, &mut x, 10, true), "nao correu depois de 1,5 s");
        assert!(anda(&mut c, &mut x, 300, true), "parou de correr andando");
    }

    #[test]
    fn parar_zera_e_tropeco_curto_nao() {
        let (mut c, mut x) = (Corrida::default(), 0.0);
        assert!(anda(&mut c, &mut x, 120, true));
        // A 0.2 s corner: keeps counting.
        fica(&mut c, x, 12);
        assert!(
            anda(&mut c, &mut x, 2, true),
            "tropeco curto zerou a corrida"
        );
        // Really stopped (arrived): zeroes.
        assert!(!fica(&mut c, x, 30));
        assert!(
            !anda(&mut c, &mut x, 60, true),
            "voltou correndo sem esperar"
        );
    }

    #[test]
    fn fora_do_automatico_nunca_corre() {
        let (mut c, mut x) = (Corrida::default(), 0.0);
        assert!(!anda(&mut c, &mut x, 300, false));
        // A dialogue opened mid-run: switches off and zeroes.
        assert!(anda(&mut c, &mut x, 120, true));
        assert!(!anda(&mut c, &mut x, 1, false));
        assert!(!anda(&mut c, &mut x, 30, true));
    }
}
