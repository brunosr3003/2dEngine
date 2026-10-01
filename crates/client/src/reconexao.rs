//! Reconnecting by itself when the connection drops mid-game.
//!
//! The owner: "make a trying reconnect if internet or game shuts for a few
//! min". Until 01/10/2026 a dropped socket — Wi-Fi blinking, the phone
//! suspending the app, the server restarting for a deploy — went straight to
//! the error screen, and the player had to pick the server, log in and pick
//! the character again.
//!
//! The way back already existed: changing islands (`TrocarZona`) reconnects,
//! replays the login and re-enters `personagem_atual` by itself. This only
//! decides WHEN to take that path again, and when to give up.

/// How long to keep trying before showing the error screen.
pub const DESISTE_APOS_S: f64 = 300.0;
/// The longest wait between two attempts.
const ESPERA_MAXIMA_S: f64 = 10.0;

#[derive(Debug, Clone)]
pub struct Reconexao {
    /// When the connection dropped.
    pub desde: f64,
    /// When the next attempt goes out.
    pub proxima: f64,
    /// Attempts made so far.
    pub tentativas: u32,
    /// Why the last one failed, for the screen and for the final error.
    pub motivo: String,
}

/// Wait before attempt `n` (0 = the first): 2, 4, 8, then 10 s.
pub fn espera(n: u32) -> f64 {
    (2.0 * 2f64.powi(n.min(8) as i32)).min(ESPERA_MAXIMA_S)
}

impl Reconexao {
    pub fn nova(agora: f64, motivo: String) -> Self {
        Self { desde: agora, proxima: agora + espera(0), tentativas: 0, motivo }
    }

    /// It is time for the next attempt.
    pub fn hora_de_tentar(&self, agora: f64) -> bool {
        agora >= self.proxima
    }

    /// An attempt went out: the next one waits longer.
    pub fn tentou(&mut self, agora: f64) {
        self.tentativas += 1;
        self.proxima = agora + espera(self.tentativas);
    }

    /// Past the limit: show the error.
    pub fn esgotou(&self, agora: f64) -> bool {
        agora - self.desde >= DESISTE_APOS_S
    }
}

/// Should this drop be retried?
///
/// Only a drop DURING play (or while already re-entering the same
/// character). Not a kick, a refused login or an incompatible client: those
/// put the error screen up before the socket closes, so `jogando` is
/// already false when the close arrives — and retrying them would hammer
/// the server with a session it just refused.
pub fn deve_reconectar(jogando: bool, reentrando: bool, motivo: &str) -> bool {
    (jogando || reentrando) && !crate::atualizacao::protocolo_incompativel(motivo)
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn espera_cresce_e_para_no_teto() {
        assert_eq!(espera(0), 2.0);
        assert_eq!(espera(1), 4.0);
        assert_eq!(espera(2), 8.0);
        assert_eq!(espera(3), 10.0);
        assert_eq!(espera(50), 10.0);
    }

    #[test]
    fn tenta_por_cinco_minutos_e_desiste() {
        let mut r = Reconexao::nova(100.0, "read: reset".into());
        assert!(!r.hora_de_tentar(101.0));
        assert!(r.hora_de_tentar(102.0));
        r.tentou(102.0);
        assert!(!r.hora_de_tentar(105.0) && r.hora_de_tentar(106.0));
        assert!(!r.esgotou(100.0 + DESISTE_APOS_S - 1.0));
        assert!(r.esgotou(100.0 + DESISTE_APOS_S));
        // Five minutes at 10 s a try is a few dozen attempts, not thousands.
        let mut r = Reconexao::nova(0.0, String::new());
        let mut t = 0.0;
        while !r.esgotou(t) {
            if r.hora_de_tentar(t) {
                r.tentou(t);
            }
            t += 0.5;
        }
        assert!((25..=35).contains(&r.tentativas), "{} attempts", r.tentativas);
    }

    #[test]
    fn so_reconecta_queda_durante_o_jogo() {
        assert!(deve_reconectar(true, false, "the server closed the connection"));
        assert!(deve_reconectar(false, true, "connect ws://x: refused"));
        // A kick or refused login already left play (error screen first).
        assert!(!deve_reconectar(false, false, "the server closed the connection"));
    }
}
