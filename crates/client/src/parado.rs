//! "NADA ESTÁ MUDANDO": o detector, num lugar só.
//!
//! Três sistemas do cliente conduzem o personagem até um ponto — o auto
//! missão, a auto coleta e o "ir para" do mapa. Os três tinham a MESMA
//! suposição errada: só tentavam de novo com a viagem DESLIGADA
//! (`!viajando`). Só que a viagem fica "ativa" enquanto o corpo empurra uma
//! parede, uma quina de casa ou o tronco da árvore — então o retry nunca
//! disparava e o personagem ficava plantado.
//!
//! O dono relatou os três, um de cada vez: "fico travado toda hora nas
//! casas", "no auto missão meu player sempre trava no porto", "auto missão de
//! coleta ele fica parado na frente da árvore". Eu consertei o primeiro no
//! `auto_missao.rs` e não olhei os outros dois, que têm cópia da mesma
//! lógica. Este módulo existe pra não haver um quarto.
//!
//! A regra é simples e não depende de saber POR QUE travou: se o corpo não
//! andou o bastante por tempo demais, o que estava sendo feito não está
//! funcionando, e é hora de tentar de outro jeito.

use macroquad::prelude::*;

/// Quanto tempo parado no mesmo lugar conta como travado.
///
/// Três segundos: curto o bastante pra a pessoa não notar a pausa, longo o
/// bastante pra não refazer rota a cada tropeço em pedra.
pub const TRAVADO_S: f64 = 3.0;

/// Quanto o corpo precisa andar pra NÃO contar como parado.
///
/// 1,5 unidade. Menos que isso e o esbarrão contínuo numa parede — que move
/// o corpo alguns centímetros pra frente e pra trás — passaria por caminhada.
pub const ANDOU_U: f32 = 1.5;

/// Onde o corpo estava quando começou a ficar parado, e desde quando.
#[derive(Debug, Default, Clone, Copy)]
pub struct Parado {
    onde: Option<Vec2>,
    desde: f64,
}

impl Parado {
    /// Uma vez por quadro. Andou? O relógio zera.
    pub fn acompanha(&mut self, eu: Vec2, agora: f64) {
        match self.onde {
            Some(o) if o.distance(eu) < ANDOU_U => {}
            _ => {
                self.onde = Some(eu);
                self.desde = agora;
            }
        }
    }

    /// Está parado há mais de `TRAVADO_S`?
    pub fn travado(&self, agora: f64) -> bool {
        self.onde.is_some() && agora - self.desde > TRAVADO_S
    }

    /// Recomeça a contagem — depois de agir sobre o travamento.
    pub fn zera(&mut self, agora: f64) {
        self.onde = None;
        self.desde = agora;
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    /// PARADO É PARADO, ANDANDO NÃO É.
    ///
    /// Os dois lados importam. Sem o primeiro, o personagem fica plantado na
    /// frente da árvore. Sem o segundo, o auto missão refaria a rota a cada
    /// três segundos de caminhada normal — e aí ninguém chega a lugar nenhum.
    #[test]
    fn so_acusa_quem_realmente_parou() {
        let mut p = Parado::default();
        p.acompanha(Vec2::ZERO, 0.0);
        assert!(!p.travado(1.0), "cedo demais");
        assert!(!p.travado(TRAVADO_S), "no limite ainda não");
        assert!(p.travado(TRAVADO_S + 0.1), "parado e não acusou");

        // Andando: nunca acusa, por mais tempo que passe.
        let mut q = Parado::default();
        let mut t = 0.0;
        while t < 60.0 {
            q.acompanha(Vec2::new(t as f32 * 3.0, 0.0), t);
            assert!(!q.travado(t), "acusou quem está andando (t={t})");
            t += 1.0;
        }
    }

    /// TREMER NÃO É ANDAR.
    ///
    /// Empurrar uma parede move o corpo alguns centímetros pra frente e pra
    /// trás. Se isso contasse como caminhada, o detector ficaria cego
    /// justamente no caso que ele existe pra pegar.
    #[test]
    fn esbarrao_continuo_nao_conta_como_caminhada() {
        let mut p = Parado::default();
        let mut t = 0.0;
        while t < 10.0 {
            // Vai e volta dentro de meia unidade.
            let x = if (t as i32) % 2 == 0 { 0.0 } else { 0.4 };
            p.acompanha(Vec2::new(x, 0.0), t);
            t += 0.5;
        }
        assert!(p.travado(10.0), "o esbarrão passou por caminhada");
    }

    #[test]
    fn zerar_recomeca_a_contagem() {
        let mut p = Parado::default();
        p.acompanha(Vec2::ZERO, 0.0);
        assert!(p.travado(5.0));
        p.zera(5.0);
        assert!(!p.travado(5.1), "zerou e continuou acusando");
    }
}
