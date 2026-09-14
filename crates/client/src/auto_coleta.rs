//! Auto coleta: liga (X ou o botao acima do COMBATE) e o personagem vai sozinho
//! pro melhor spot de coleta perto e fica la'.
//!
//! A coleta em si continua sendo o LUGAR (docs/COLETA.md): parado num spot com
//! pedra ou tronco vivo, o servidor entrega sozinho, sem clique. O que o auto
//! faz e' escolher o spot e trocar quando ele para de render. Quem escolhe e' o
//! SERVIDOR (`PedirSpotDeColeta`), que sabe o que esta' esgotado.
use macroquad::prelude::*;

use crate::hud_estilo as estilo;

/// Perto disto do spot, chegou.
const CHEGOU: f32 = 1.6;
/// Parado no spot sem a bolsa crescer nesse tempo: o veio acabou, pede outro.
const SEM_GANHO_S: f64 = 18.0;
/// Pediu e o servidor nao respondeu: pede de novo.
const REPEDE_S: f64 = 4.0;
/// Nada por perto: espera isso antes de perguntar de novo.
const APOS_FALHA_S: f64 = 6.0;
/// A viagem acabou longe do spot: manda de novo depois disso.
const RELIGA_S: f64 = 1.5;

#[derive(Debug, PartialEq)]
pub enum Acao {
    Nada,
    PedirSpot,
    Ir(Vec2),
}

#[derive(Default)]
pub struct AutoColeta {
    pub centro: Option<Vec2>,
    pub spot: Option<Vec2>,
    esperando: bool,
    pedido_em: f64,
    chegou_em: Option<f64>,
    ultimo_total: u32,
    ultimo_ganho: f64,
    pub falhas: u32,
    /// Coleta de UM tipo em volta de um ponto (o "Ir" do mapa numa regiao):
    /// (tipo, onde procurar). `None` = qualquer recurso perto.
    pub filtro: Option<(u8, Vec2)>,
}

/// O botao AUTO COLETA, a' esquerda do COMBATE (ver `hud_layout`).
pub fn retangulo() -> Rect {
    crate::hud_layout::atual().auto_coleta
}

pub fn pega_mouse() -> bool {
    retangulo().contains(Vec2::from(mouse_position()))
}

impl AutoColeta {
    pub fn ativo(&self) -> bool {
        self.centro.is_some()
    }

    pub fn ligar(&mut self, p: Vec2, agora: f64) {
        *self = Self { centro: Some(p), pedido_em: agora - 100.0, ..Default::default() };
    }

    pub fn parar(&mut self) {
        *self = Self::default();
    }

    /// Resposta do servidor.
    pub fn spot_recebido(&mut self, pos: Option<Vec2>, agora: f64) -> Acao {
        if !self.ativo() || !self.esperando {
            return Acao::Nada;
        }
        self.esperando = false;
        self.pedido_em = agora;
        match pos {
            Some(p) => {
                self.spot = Some(p);
                self.falhas = 0;
                self.chegou_em = None;
                self.ultimo_ganho = agora;
                Acao::Ir(p)
            }
            None => {
                self.falhas += 1;
                Acao::Nada
            }
        }
    }

    /// Um quadro. `total` = itens na bolsa (cresce quando a coleta entrega).
    pub fn passo(&mut self, eu: Vec2, agora: f64, total: u32, viajando: bool) -> Acao {
        if !self.ativo() {
            return Acao::Nada;
        }
        let Some(s) = self.spot else {
            if self.esperando {
                if agora - self.pedido_em > REPEDE_S {
                    self.esperando = false;
                }
                return Acao::Nada;
            }
            let espera = if self.falhas > 0 { APOS_FALHA_S } else { 0.0 };
            if agora - self.pedido_em >= espera {
                self.esperando = true;
                self.pedido_em = agora;
                return Acao::PedirSpot;
            }
            return Acao::Nada;
        };
        if eu.distance(s) <= CHEGOU {
            let chegou = *self.chegou_em.get_or_insert(agora);
            if total > self.ultimo_total {
                self.ultimo_ganho = agora;
            }
            self.ultimo_total = total;
            if agora - chegou.max(self.ultimo_ganho) > SEM_GANHO_S {
                // Veio esgotado: o proximo quadro pede outro.
                self.spot = None;
                self.chegou_em = None;
                self.pedido_em = agora - 100.0;
            }
            return Acao::Nada;
        }
        self.ultimo_total = total;
        if !viajando && agora - self.pedido_em >= RELIGA_S {
            self.pedido_em = agora;
            return Acao::Ir(s);
        }
        Acao::Nada
    }

    /// O botao. A tecla (X) so' aparece com Alt; o estado vai pra faixa unica.
    pub fn desenha(&self) {
        let r = retangulo();
        let c = r.center();
        let raio = r.w * 0.45;
        let cor = if self.ativo() { estilo::AUTO } else { estilo::OURO };
        draw_circle(c.x, c.y + 3.0, raio + 3.0, Color::new(0.0, 0.0, 0.0, 0.35));
        draw_circle(c.x, c.y, raio, estilo::FUNDO);
        draw_circle_lines(c.x, c.y, raio, 2.0, cor);
        if self.ativo() {
            estilo::arco(c, raio + 4.0, get_time() as f32 * 0.8, 0.20, 2.0, cor);
        }
        estilo::icone(3, c - vec2(0.0, raio * 0.18), raio * 0.40, cor);
        estilo::texto_centro(c.x, c.y + raio * 0.66, if self.ativo() { "AUTO" } else { "COLETA" }, 11, cor);
        crate::hud_layout::chip(r, "X");
    }

    /// O texto da faixa de estado, com a coleta ligada.
    pub fn faixa(&self, eu: Option<Vec2>) -> Option<&'static str> {
        if !self.ativo() {
            return None;
        }
        Some(match (self.spot, eu) {
            (None, _) => "AUTO COLETA · PROCURANDO",
            (Some(s), Some(e)) if e.distance(s) <= CHEGOU => "AUTO COLETA · COLETANDO",
            _ => "AUTO COLETA · INDO AO VEIO",
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escolhe_vai_coleta_e_troca_ao_esgotar() {
        let mut a = AutoColeta::default();
        assert_eq!(a.passo(Vec2::ZERO, 0.0, 0, false), Acao::Nada, "desligado");
        a.ligar(Vec2::ZERO, 0.0);
        assert_eq!(a.passo(Vec2::ZERO, 0.0, 0, false), Acao::PedirSpot);
        assert_eq!(a.passo(Vec2::ZERO, 0.1, 0, false), Acao::Nada, "esperando resposta");
        let spot = vec2(30.0, 0.0);
        assert_eq!(a.spot_recebido(Some(spot), 0.2), Acao::Ir(spot));
        // No caminho, viajando: nada.
        assert_eq!(a.passo(vec2(10.0, 0.0), 1.0, 0, true), Acao::Nada);
        // A viagem parou longe: manda de novo.
        assert_eq!(a.passo(vec2(20.0, 0.0), 2.0, 0, false), Acao::Ir(spot));
        // Chegou e a bolsa cresce: fica.
        for k in 0..10 {
            assert_eq!(a.passo(spot, 3.0 + k as f64 * 5.0, k, false), Acao::Nada);
        }
        assert_eq!(a.spot, Some(spot));
        // Parou de render por mais de SEM_GANHO_S: pede outro.
        assert_eq!(a.passo(spot, 70.0, 9, false), Acao::Nada);
        assert_eq!(a.spot, None);
        assert_eq!(a.passo(spot, 70.1, 9, false), Acao::PedirSpot);
    }

    #[test]
    fn nada_por_perto_espera_e_tenta_de_novo() {
        let mut a = AutoColeta::default();
        a.ligar(Vec2::ZERO, 0.0);
        assert_eq!(a.passo(Vec2::ZERO, 0.0, 0, false), Acao::PedirSpot);
        assert_eq!(a.spot_recebido(None, 0.5), Acao::Nada);
        assert_eq!(a.falhas, 1);
        assert_eq!(a.passo(Vec2::ZERO, 3.0, 0, false), Acao::Nada);
        assert_eq!(a.passo(Vec2::ZERO, 7.0, 0, false), Acao::PedirSpot);
        // Resposta que chega depois de desligar e' ignorada.
        a.parar();
        assert_eq!(a.spot_recebido(Some(Vec2::ONE), 8.0), Acao::Nada);
        assert!(!a.ativo());
    }
}
