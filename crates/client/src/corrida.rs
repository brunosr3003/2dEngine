//! Corrida automatica: indo sozinho (viagem do mapa, auto missao, rota por
//! clique), depois de um tempo andando sem parar o personagem passa a correr.
//!
//! O cliente so' liga o bit `SPRINT` do input. Quem decide se da' pra correr
//! e' o servidor, como com o Shift: sem vigor ele nao corre, e volta a correr
//! sozinho quando o vigor recupera, sem precisar parar.

use macroquad::prelude::*;

/// Andando continuo por este tempo, corre.
const ESPERA_S: f32 = 1.5;
/// Parado por mais que isto, o contador zera.
const PARADO_S: f32 = 0.3;
/// Abaixo desta velocidade (u/s) conta como parado.
const MEXENDO: f32 = 0.5;

#[derive(Default)]
pub struct Corrida {
    andando: f32,
    parado: f32,
    ultima: Option<Vec2>,
}

impl Corrida {
    /// Um quadro. `automatico` = o personagem esta' indo sozinho e nada o
    /// segura (sem dialogo aberto, sem auto combate ou coleta parados na
    /// zona). Devolve se deve correr.
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

    /// Anda `quadros` quadros a 4 u/s em linha reta.
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
        // Quina de 0,2 s: continua contando.
        fica(&mut c, x, 12);
        assert!(anda(&mut c, &mut x, 2, true), "tropeco curto zerou a corrida");
        // Parou de verdade (chegou): zera.
        assert!(!fica(&mut c, x, 30));
        assert!(!anda(&mut c, &mut x, 60, true), "voltou correndo sem esperar");
    }

    #[test]
    fn fora_do_automatico_nunca_corre() {
        let (mut c, mut x) = (Corrida::default(), 0.0);
        assert!(!anda(&mut c, &mut x, 300, false));
        // Dialogo abriu no meio da corrida: desliga e zera.
        assert!(anda(&mut c, &mut x, 120, true));
        assert!(!anda(&mut c, &mut x, 1, false));
        assert!(!anda(&mut c, &mut x, 30, true));
    }
}
