//! Segue um jogador por rotas do servidor, mantendo distância sem atacar.
use macroquad::prelude::Vec2;
use shared::{protocol::ClientMessage, EntityId};
#[derive(Default)]
pub struct Seguir {
    pub alvo: Option<EntityId>,
    rota: bool,
    proximo: f64,
}
impl Seguir {
    pub fn iniciar(&mut self, id: EntityId) {
        self.alvo = Some(id);
        self.rota = false;
        self.proximo = 0.;
    }
    pub fn parar(&mut self) -> Option<ClientMessage> {
        self.alvo = None;
        let parar = self.rota;
        self.rota = false;
        parar.then_some(ClientMessage::PararRota)
    }
    pub fn passo(&mut self, eu: Vec2, alvo: Option<Vec2>, agora: f64) -> Option<ClientMessage> {
        self.alvo?;
        let Some(alvo) = alvo else {
            return self.parar();
        };
        if eu.distance(alvo) <= 2.5 {
            if self.rota {
                self.rota = false;
                return Some(ClientMessage::PararRota);
            }
            return None;
        }
        if agora < self.proximo {
            return None;
        }
        self.proximo = agora + 0.5;
        self.rota = true;
        let destino = alvo + (eu - alvo).normalize_or_zero() * 2.;
        Some(ClientMessage::MoverPara {
            x: destino.x,
            z: destino.y,
        })
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn acompanha_para_perto_e_cancela_ao_desaparecer() {
        let mut s = Seguir::default();
        s.iniciar(EntityId(4));
        assert!(matches!(
            s.passo(Vec2::ZERO, Some(Vec2::new(10., 0.)), 0.),
            Some(ClientMessage::MoverPara { x: 8., z: 0. })
        ));
        assert!(s.passo(Vec2::ZERO, Some(Vec2::new(12., 0.)), 0.1).is_none());
        assert!(matches!(
            s.passo(Vec2::ZERO, Some(Vec2::new(2., 0.)), 0.2),
            Some(ClientMessage::PararRota)
        ));
        assert!(s.passo(Vec2::ZERO, Some(Vec2::new(2., 0.)), 0.3).is_none());
        assert!(matches!(
            s.passo(Vec2::ZERO, Some(Vec2::new(12., 0.)), 0.6),
            Some(ClientMessage::MoverPara { x: 10., z: 0. })
        ));
        assert!(matches!(
            s.passo(Vec2::ZERO, None, 0.7),
            Some(ClientMessage::PararRota)
        ));
        assert!(s.alvo.is_none());
    }
}
