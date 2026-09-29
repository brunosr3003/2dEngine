//! Keyboard and mouse controls, shared by macOS, Windows and Linux.
use macroquad::prelude::*;

pub const PULO: KeyCode = KeyCode::R;

pub fn ataque_pressionado() -> bool {
    is_key_pressed(KeyCode::Space) || is_key_pressed(KeyCode::F)
}

pub fn skill_pressionada(slot: usize) -> bool {
    let teclas = [
        [KeyCode::Key1, KeyCode::Kp1],
        [KeyCode::Key2, KeyCode::Kp2],
        [KeyCode::Key3, KeyCode::Kp3],
    ];
    teclas
        .get(slot)
        .is_some_and(|par| par.iter().any(|k| is_key_pressed(*k)))
}

pub fn movimento() -> Vec2 {
    direcao(
        is_key_down(KeyCode::W) || is_key_down(KeyCode::Up),
        is_key_down(KeyCode::S) || is_key_down(KeyCode::Down),
        is_key_down(KeyCode::A) || is_key_down(KeyCode::Left),
        is_key_down(KeyCode::D) || is_key_down(KeyCode::Right),
    )
}

fn direcao(frente: bool, tras: bool, esquerda: bool, direita: bool) -> Vec2 {
    vec2(
        direita as u8 as f32 - esquerda as u8 as f32,
        tras as u8 as f32 - frente as u8 as f32,
    )
    .normalize_or_zero()
}

/// A gesture belongs to whoever received the press. Dragging from a UI button
/// into the world does not become camera; opening a panel cancels until release.
#[derive(Default)]
pub struct ArrastoCamera {
    segurando: bool,
    anterior: Option<Vec2>,
}

impl ArrastoCamera {
    pub fn quadro(&mut self, pos: Vec2, segurando: bool, sobre_ui: bool, bloqueado: bool) -> Vec2 {
        if !segurando || bloqueado {
            self.anterior = None;
        } else if !self.segurando && !sobre_ui {
            self.anterior = Some(pos);
        }
        self.segurando = segurando;
        self.anterior.as_mut().map_or(Vec2::ZERO, |anterior| {
            let delta = pos - *anterior;
            *anterior = pos;
            delta
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn diagonal_nao_acelera_e_opostos_cancelam() {
        assert_eq!(direcao(true, true, false, false), Vec2::ZERO);
        assert_eq!(direcao(true, false, false, false), vec2(0.0, -1.0));
        assert!((direcao(true, false, false, true).length() - 1.0).abs() < 0.0001);
    }

    #[test]
    fn arrasto_da_ui_nao_vira_camera() {
        let mut a = ArrastoCamera::default();
        assert_eq!(a.quadro(Vec2::ZERO, true, true, false), Vec2::ZERO);
        assert_eq!(a.quadro(vec2(40.0, 20.0), true, false, false), Vec2::ZERO);
        a.quadro(Vec2::ZERO, false, false, false);
        assert_eq!(a.quadro(vec2(40.0, 20.0), true, false, false), Vec2::ZERO);
        assert_eq!(
            a.quadro(vec2(45.0, 18.0), true, false, false),
            vec2(5.0, -2.0)
        );
    }

    #[test]
    fn painel_cancela_arrasto_ate_soltar() {
        let mut a = ArrastoCamera::default();
        a.quadro(Vec2::ZERO, true, false, false);
        assert_eq!(a.quadro(Vec2::ONE, true, false, true), Vec2::ZERO);
        assert_eq!(a.quadro(Vec2::ONE * 2.0, true, false, false), Vec2::ZERO);
        a.quadro(Vec2::ZERO, false, false, false);
        a.quadro(Vec2::ZERO, true, false, false);
        assert_eq!(a.quadro(Vec2::ONE, true, false, false), Vec2::ONE);
    }
}
