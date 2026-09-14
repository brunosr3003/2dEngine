//! Preferencias de tela do personagem (skills AUTO, filtros do mapa, zooms),
//! guardadas no servidor. Aqui so' a SINCRONIA: quando mandar.
//!
//! Duas regras:
//!   * nada sai antes de chegar o `Preferencias` do servidor — senao o padrao
//!     do cliente, que ainda nao aplicou nada, sobrescreveria o que estava
//!     salvo;
//!   * mudanca so' vai `ESPERA_S` depois da ULTIMA mudanca: girar a roda da
//!     camera nao vira uma mensagem por quadro.
use shared::protocol::Preferencias;

/// Quanto esperar parado depois de mudar antes de mandar.
pub const ESPERA_S: f64 = 2.0;

#[derive(Default)]
pub struct Sincronia {
    recebidas: bool,
    /// O que o servidor tem (recebido ou ja' mandado).
    no_servidor: Option<Preferencias>,
    /// O estado do quadro anterior e desde quando ele e' assim.
    visto: Option<Preferencias>,
    mudou_em: f64,
}

impl Sincronia {
    /// Chegou do servidor, JA' aplicado: `atual` e' o estado depois de aplicar
    /// (com os recortes do cliente), pra nao ecoar de volta.
    pub fn recebeu(&mut self, atual: &Preferencias) {
        self.recebidas = true;
        self.no_servidor = Some(atual.clone());
        self.visto = Some(atual.clone());
    }

    pub fn recebidas(&self) -> bool {
        self.recebidas
    }

    /// A cada quadro com o estado atual. Devolve o que mandar, se for hora.
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

    /// Saindo ou trocando de zona: manda JA' o que estiver pendente.
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
        Preferencias { skills_auto: ids.to_vec(), ..Default::default() }
    }

    #[test]
    fn nada_sai_antes_de_receber_do_servidor() {
        let mut s = Sincronia::default();
        assert_eq!(s.acompanhar(&com_skills(&[1]), 0.0), None);
        assert_eq!(s.acompanhar(&com_skills(&[1]), 99.0), None, "padrao nao sobrescreve o salvo");
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
        assert_eq!(s.acompanhar(&com_skills(&[1, 2]), 4.0), None, "so' 1,5 s parado");
        assert_eq!(s.acompanhar(&com_skills(&[1, 2]), 4.6), Some(com_skills(&[1, 2])));
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
