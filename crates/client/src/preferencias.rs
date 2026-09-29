//! The character's screen preferences (AUTO skills, map filters, zooms),
//! stored on the server. Only the SYNC lives here: when to send.
//!
//! Two rules:
//! * nothing goes out before the server's `Preferencias` arrives — otherwise
//! the client's default, which has not applied anything yet, would overwrite
//! what was saved;
//! * a change only goes `ESPERA_S` after the LAST change: spinning the camera
//! wheel does not become one message per frame.
use shared::protocol::Preferencias;

/// How long to sit still after a change before sending.
pub const ESPERA_S: f64 = 2.0;

#[derive(Default)]
pub struct Sincronia {
    recebidas: bool,
    /// What the server has (received, or already sent).
    no_servidor: Option<Preferencias>,
    /// The previous frame's state and how long it has been that way.
    visto: Option<Preferencias>,
    mudou_em: f64,
}

impl Sincronia {
    /// Arrived from the server, ALREADY applied: `atual` is the state after
    /// applying (with the client's clamps), so as not to echo it back.
    pub fn recebeu(&mut self, atual: &Preferencias) {
        self.recebidas = true;
        self.no_servidor = Some(atual.clone());
        self.visto = Some(atual.clone());
    }

    pub fn recebidas(&self) -> bool {
        self.recebidas
    }

    /// Every frame, with the current state. Returns what to send, if it is time.
    pub fn acompanhar(&mut self, atual: &Preferencias, agora: f64) -> Option<Preferencias> {
        if !self.recebidas {
            return None;
        }
        if self.visto.as_ref() != Some(atual) {
            self.visto = Some(atual.clone());
            self.mudou_em = agora;
        }
        if self.no_servidor.as_ref() == Some(atual) || agora - self.mudou_em < ESPERA_S {
            return None;
        }
        self.no_servidor = Some(atual.clone());
        Some(atual.clone())
    }

    /// Leaving or changing zone: send whatever is pending NOW.
    pub fn forcar(&mut self, atual: &Preferencias) -> Option<Preferencias> {
        if !self.recebidas || self.no_servidor.as_ref() == Some(atual) {
            return None;
        }
        self.no_servidor = Some(atual.clone());
        Some(atual.clone())
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    fn com_skills(ids: &[u32]) -> Preferencias {
        Preferencias {
            skills_auto: ids.to_vec(),
            ..Default::default()
        }
    }

    #[test]
    fn nada_sai_antes_de_receber_do_servidor() {
        let mut s = Sincronia::default();
        assert_eq!(s.acompanhar(&com_skills(&[1]), 0.0), None);
        assert_eq!(
            s.acompanhar(&com_skills(&[1]), 99.0),
            None,
            "padrao nao sobrescreve o salvo"
        );
        assert_eq!(s.forcar(&com_skills(&[1])), None);
    }

    #[test]
    fn aplicado_ao_receber_nao_ecoa() {
        let mut s = Sincronia::default();
        s.recebeu(&com_skills(&[4, 5]));
        assert_eq!(s.acompanhar(&com_skills(&[4, 5]), 10.0), None);
        assert_eq!(s.acompanhar(&com_skills(&[4, 5]), 50.0), None);
    }

    #[test]
    fn mudanca_espera_parar_e_vai_uma_vez() {
        let mut s = Sincronia::default();
        s.recebeu(&Preferencias::default());
        assert_eq!(s.acompanhar(&com_skills(&[1]), 1.0), None);
        // Continua mudando: reinicia a espera.
        assert_eq!(s.acompanhar(&com_skills(&[1, 2]), 2.5), None);
        assert_eq!(
            s.acompanhar(&com_skills(&[1, 2]), 4.0),
            None,
            "so' 1,5 s parado"
        );
        assert_eq!(
            s.acompanhar(&com_skills(&[1, 2]), 4.6),
            Some(com_skills(&[1, 2]))
        );
        assert_eq!(s.acompanhar(&com_skills(&[1, 2]), 9.0), None, "ja' mandado");
    }

    #[test]
    fn forcar_manda_o_pendente_na_saida() {
        let mut s = Sincronia::default();
        s.recebeu(&Preferencias::default());
        s.acompanhar(&com_skills(&[3]), 1.0);
        assert_eq!(s.forcar(&com_skills(&[3])), Some(com_skills(&[3])));
        assert_eq!(s.forcar(&com_skills(&[3])), None);
    }
}
