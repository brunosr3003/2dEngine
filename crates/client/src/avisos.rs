//! As mensagens do jogo ("Missão concluída", "Auto coleta: atacado") como
//! AVISO passageiro, e nao como caixa de chat.
//!
//! Antes havia um painel fixo no canto esquerdo, ocupando o lugar onde o
//! polegar procura o joystick. O dono pediu pra tirar: a informacao continua,
//! agora em texto solto que some sozinho.

/// Quanto tempo cada linha fica na tela.
const VIDA_S: f64 = 9.0;
/// Quantas linhas aparecem de uma vez.
const LINHAS: usize = 3;
/// Teto do historico guardado (o painel de log, se voltar, le' daqui).
const GUARDADAS: usize = 20;

#[derive(Default)]
pub struct Avisos {
    linhas: Vec<(String, f64)>,
}

impl Avisos {
    /// Mesma assinatura do `Vec::push` que estava aqui antes: todo lugar que
    /// avisava alguma coisa continua igual.
    pub fn push(&mut self, texto: String) {
        self.linhas.push((texto, macroquad::time::get_time()));
        if self.linhas.len() > GUARDADAS {
            self.linhas.remove(0);
        }
    }

    /// Limpa tudo (trocar de personagem, sair do mundo).
    pub fn clear(&mut self) {
        self.linhas.clear();
    }

    /// As ultimas linhas ainda vivas, da mais velha pra mais nova.
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
        // O teto so' vale pelo `push`; aqui o que importa e' a janela de tempo.
        let agora = (GUARDADAS + 4) as f64;
        let vistas = a.recentes(agora);
        assert_eq!(vistas.len(), LINHAS, "mostra as tres ultimas");
        assert_eq!(*vistas.last().unwrap(), format!("linha {}", GUARDADAS + 4));
        assert!(a.recentes(agora + VIDA_S).is_empty(), "somem sozinhas");
    }
}
