//! Gesto separado do desenho: soltar um clique usa, arrastar configura AUTO.
use macroquad::prelude::{Rect, Vec2};

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Gesto {
    Usar(u32),
    Auto(u32, bool),
}

#[derive(Default)]
pub struct Arrasto {
    pub inicio: Option<(u32, Vec2)>,
    longe: bool,
}

impl Arrasto {
    pub fn pressiona(&mut self, id: u32, p: Vec2) {
        self.inicio = Some((id, p));
        self.longe = false;
    }
    pub fn move_para(&mut self, p: Vec2) {
        if let Some((_, de)) = self.inicio {
            self.longe |= p.distance(de) > 12.0;
        }
    }
    pub fn cancela(&mut self) {
        self.inicio = None;
        self.longe = false;
    }
    pub fn solta(&mut self, p: Vec2, r: Rect) -> Option<Gesto> {
        self.move_para(p);
        let (id, de) = self.inicio.take()?;
        let delta = p - de;
        if delta.y.abs() >= 30.0 && delta.y.abs() > delta.x.abs() {
            Some(Gesto::Auto(id, delta.y < 0.0))
        } else if !self.longe && r.contains(p) {
            Some(Gesto::Usar(id))
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use macroquad::prelude::vec2;
    #[test]
    fn clique_e_arrastos_nao_disparam_juntos() {
        let r = Rect::new(0.0, 0.0, 80.0, 80.0);
        for (fim, esperado) in [
            (vec2(42.0, 42.0), Some(Gesto::Usar(4))),
            (vec2(45.0, -20.0), Some(Gesto::Auto(4, true))),
            (vec2(40.0, 78.0), Some(Gesto::Auto(4, false))),
            (vec2(110.0, 35.0), None),
        ] {
            let mut a = Arrasto::default();
            a.pressiona(4, vec2(40.0, 40.0));
            assert_eq!(a.solta(fim, r), esperado);
            assert!(a.inicio.is_none());
        }
    }
    #[test]
    fn arrastar_e_voltar_ou_cancelar_nao_vira_clique() {
        let mut a = Arrasto::default();
        let r = Rect::new(0.0, 0.0, 80.0, 80.0);
        a.pressiona(1, vec2(40.0, 40.0));
        a.move_para(vec2(40.0, -20.0));
        assert_eq!(a.solta(vec2(40.0, 40.0), r), None);
        a.pressiona(1, vec2(40.0, 40.0));
        a.cancela();
        assert_eq!(a.solta(vec2(40.0, 40.0), r), None);
    }
}
