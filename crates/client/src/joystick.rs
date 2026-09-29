//! Joystick VIRTUAL (toque), no molde do MIR4 mobile: WASD na metade esquerda.
//!
//! Floating: the base appears where the finger landed, the stick follows the
//! finger pinned to the radius. The output is a SCREEN direction (x right, y
//! down, like WASD) times the intensity; `main` converts it to world with the
//! camera's yaw and sends it in the `InputFrame` like the keyboard.
//!
//! The joystick's finger is ITS OWN: it does not become a click in the world
//! and does not rotate the camera (`sem_dedo` removes it from the list that
//! goes to `gesto_camera`).
//!
//! The core does not know macroquad; `main` reads the touches and draws.
use macroquad::prelude::*;

use crate::gesto_camera::{Fase, ToqueNoQuadro};
use crate::hud_estilo as estilo;

/// Stick radius at 1920x1080 (px), multiplied by the HUD's scale.
pub const RAIO_BASE: f32 = 78.0;
/// Fraction of the radius that does not move (a still finger trembling).
pub const ZONA_MORTA: f32 = 0.12;
/// Minimum intensity outside the dead zone: the server treats |dir|² <= 0.01
/// as "stopped", so below that a subtle push would be ignored.
pub const INTENSIDADE_MINIMA: f32 = 0.35;

#[derive(Debug, Default, Clone, Copy)]
pub struct Joystick {
    dedo: Option<u64>,
    base: Vec2,
    ponta: Vec2,
    raio: f32,
}

impl Joystick {
    /// Id of the finger the joystick is using.
    pub fn dedo(&self) -> Option<u64> {
        self.dedo
    }

    pub fn ativo(&self) -> bool {
        self.dedo.is_some()
    }

    pub fn soltar(&mut self) {
        self.dedo = None;
    }

    /// One frame: claims a finger that STARTED inside `area` (and outside the
    /// HUD — `pode_comecar` decides), follows it until it leaves.
    pub fn quadro(
        &mut self,
        toques: &[ToqueNoQuadro],
        raio: f32,
        pode_comecar: &dyn Fn(Vec2) -> bool,
    ) {
        self.raio = raio.max(1.0);
        if let Some(id) = self.dedo {
            match toques.iter().find(|t| t.id == id) {
                Some(t) if t.fase != Fase::Acabou => self.ponta = t.pos,
                // Left (or vanished with no visible Acabou): releases.
                _ => self.dedo = None,
            }
            return;
        }
        if let Some(t) = toques
            .iter()
            .find(|t| t.fase == Fase::Comecou && pode_comecar(t.pos))
        {
            self.dedo = Some(t.id);
            self.base = t.pos;
            self.ponta = t.pos;
        }
    }

    /// Direcao de TELA × intensidade (0 a 1). Zero dentro da zona morta.
    pub fn direcao(&self) -> Vec2 {
        if self.dedo.is_none() {
            return Vec2::ZERO;
        }
        let v = self.ponta - self.base;
        let frac = (v.length() / self.raio).min(1.0);
        if frac <= ZONA_MORTA {
            return Vec2::ZERO;
        }
        let intensidade = ((frac - ZONA_MORTA) / (1.0 - ZONA_MORTA)).max(INTENSIDADE_MINIMA);
        v.normalize_or_zero() * intensidade
    }

    /// The joystick is asking to walk (counts as keyboard).
    pub fn movendo(&self) -> bool {
        self.direcao() != Vec2::ZERO
    }

    /// Where to draw the stick: pinned to the radius.
    pub fn manipulo(&self) -> Vec2 {
        let v = self.ponta - self.base;
        if v.length() > self.raio {
            self.base + v.normalize() * self.raio
        } else {
            self.ponta
        }
    }

    /// Where the base rests when nobody's finger is on it: on top of the
    /// joystick's strip, at thumb height (down is where the hand sits).
    pub fn base_em_repouso(z: &crate::hud_layout::Zonas) -> Vec2 {
        let r = z.joystick;
        let raio = RAIO_BASE * z.s;
        vec2(r.x + raio + 16.0 * z.s, r.y + r.h - raio - 16.0 * z.s)
    }

    /// Draws. It ALWAYS shows (the owner's request): idle, the base sits dimmed
    /// at its resting place; with a finger, it goes where the finger landed.
    pub fn desenha(&self, z: &crate::hud_layout::Zonas) {
        let segurando = self.dedo.is_some();
        let (r, b) = if segurando {
            (self.raio, self.base)
        } else {
            (RAIO_BASE * z.s, Self::base_em_repouso(z))
        };
        if !segurando {
            draw_circle(b.x, b.y, r, estilo::alfa(estilo::FUNDO, 0.28));
            draw_circle_lines(b.x, b.y, r, 2.0, estilo::alfa(estilo::TEXTO, 0.18));
            draw_circle_lines(b.x, b.y, r * 0.55, 1.0, estilo::alfa(estilo::TEXTO, 0.08));
            let rm = r * 0.42;
            draw_circle(b.x, b.y, rm, estilo::alfa(estilo::FUNDO_ALTO, 0.5));
            draw_circle_lines(b.x, b.y, rm, 2.0, estilo::alfa(estilo::TEXTO, 0.22));
            return;
        }
        draw_circle(b.x, b.y + 3.0, r + 6.0, Color::new(0.0, 0.0, 0.0, 0.22));
        draw_circle(b.x, b.y, r, estilo::alfa(estilo::FUNDO, 0.55));
        draw_circle_lines(b.x, b.y, r, 2.0, estilo::alfa(estilo::TEXTO, 0.35));
        draw_circle_lines(b.x, b.y, r * 0.55, 1.0, estilo::alfa(estilo::TEXTO, 0.12));
        let m = self.manipulo();
        let rm = r * 0.42;
        draw_circle(m.x, m.y, rm + 3.0, estilo::alfa(estilo::AUTO, 0.25));
        draw_circle(m.x, m.y, rm, estilo::alfa(estilo::FUNDO_ALTO, 0.9));
        draw_circle_lines(m.x, m.y, rm, 2.0, estilo::alfa(estilo::AUTO, 0.8));
        draw_circle(
            m.x - rm * 0.25,
            m.y - rm * 0.3,
            rm * 0.35,
            Color::new(1.0, 1.0, 1.0, 0.10),
        );
    }
}

/// The touch list without the joystick's finger — it is what goes to the
/// camera gesture and to the click in the world.
#[cfg(test)]
pub fn sem_dedo(toques: &[ToqueNoQuadro], dedo: Option<u64>) -> Vec<ToqueNoQuadro> {
    sem_dedos(toques, &[dedo])
}

/// The same, removing several. `main` passes the finger from BEFORE and from
/// AFTER `quadro`: on the frame the finger releases, the joystick has already
/// forgotten the id, and its `Acabou` fell into the gesture as a short touch
/// — a click in the world, walking to where the thumb left.
pub fn sem_dedos(toques: &[ToqueNoQuadro], dedos: &[Option<u64>]) -> Vec<ToqueNoQuadro> {
    toques
        .iter()
        .copied()
        .filter(|t| !dedos.contains(&Some(t.id)))
        .collect()
}

/// Walking "by hand": keyboard OR joystick. It is what pauses the auto quest,
/// travel, go-to and the walk to the NPC, just like WASD.
pub fn movimento_manual(teclas: bool, joy: &Joystick) -> bool {
    teclas || joy.movendo()
}

#[cfg(test)]
mod testes {
    use super::*;

    fn t(id: u64, fase: Fase, x: f32, y: f32) -> ToqueNoQuadro {
        ToqueNoQuadro {
            id,
            fase,
            pos: vec2(x, y),
        }
    }

    fn area_esquerda(p: Vec2) -> bool {
        p.x < 500.0 && p.y > 400.0
    }

    #[test]
    fn prende_so_dedo_que_comeca_na_area() {
        let mut j = Joystick::default();
        j.quadro(&[t(1, Fase::Comecou, 800.0, 700.0)], 80.0, &area_esquerda);
        assert!(!j.ativo(), "metade direita nao e' joystick");
        j.quadro(&[t(2, Fase::Comecou, 200.0, 700.0)], 80.0, &area_esquerda);
        assert_eq!(j.dedo(), Some(2));
        // Dragging outside the area is still the joystick.
        j.quadro(&[t(2, Fase::Segurando, 600.0, 300.0)], 80.0, &area_esquerda);
        assert!(j.ativo());
        j.quadro(&[t(2, Fase::Acabou, 600.0, 300.0)], 80.0, &area_esquerda);
        assert!(!j.ativo());
        assert_eq!(j.direcao(), Vec2::ZERO);
    }

    #[test]
    fn zona_morta_e_limite_do_raio() {
        let mut j = Joystick::default();
        j.quadro(&[t(1, Fase::Comecou, 200.0, 700.0)], 100.0, &area_esquerda);
        j.quadro(
            &[t(1, Fase::Segurando, 208.0, 700.0)],
            100.0,
            &area_esquerda,
        );
        assert_eq!(j.direcao(), Vec2::ZERO, "8% do raio e' zona morta");
        j.quadro(
            &[t(1, Fase::Segurando, 450.0, 700.0)],
            100.0,
            &area_esquerda,
        );
        let d = j.direcao();
        assert!(
            (d.length() - 1.0).abs() < 1e-4 && d.x > 0.99,
            "alem do raio: intensidade 1 pra direita: {d}"
        );
        assert!(
            (j.manipulo() - vec2(300.0, 700.0)).length() < 1e-3,
            "manipulo preso ao raio"
        );
        j.quadro(
            &[t(1, Fase::Segurando, 200.0, 680.0)],
            100.0,
            &area_esquerda,
        );
        let d = j.direcao();
        assert!(
            d.y < 0.0 && (d.length() - INTENSIDADE_MINIMA).abs() < 1e-4,
            "empurrao leve pra cima: {d}"
        );
    }

    #[test]
    fn direcao_de_tela_vira_mundo_como_o_wasd() {
        let mut j = Joystick::default();
        j.quadro(&[t(1, Fase::Comecou, 200.0, 700.0)], 100.0, &area_esquerda);
        j.quadro(
            &[t(1, Fase::Segurando, 200.0, 500.0)],
            100.0,
            &area_esquerda,
        );
        // Up on the screen = W.
        let w = crate::render3d::input_para_mundo(vec2(0.0, -1.0), 0.7);
        let joy = crate::render3d::input_para_mundo(j.direcao(), 0.7);
        assert!(
            (w - joy).length() < 1e-4,
            "joystick pra cima = W: {w} vs {joy}"
        );
    }

    #[test]
    fn dedo_do_joystick_nao_vai_pra_camera_nem_clique() {
        let mut j = Joystick::default();
        let quadro = [
            t(1, Fase::Comecou, 200.0, 700.0),
            t(2, Fase::Comecou, 900.0, 300.0),
        ];
        j.quadro(&quadro, 100.0, &area_esquerda);
        let resto = sem_dedo(&quadro, j.dedo());
        assert_eq!(resto.len(), 1);
        assert_eq!(resto[0].id, 2, "so' o outro dedo vai pro gesto da camera");
        // With only the joystick, the gesture receives nothing: no click on release.
        let mut g = crate::gesto_camera::GestoCamera::default();
        let so_joy = [t(1, Fase::Acabou, 200.0, 700.0)];
        assert_eq!(
            g.quadro(&sem_dedo(&so_joy, Some(1)), false),
            crate::gesto_camera::Acao::Nada
        );
    }

    /// The iPhone bug: releasing the thumb from the joystick walked you there (a click).
    #[test]
    fn soltar_o_joystick_nao_vira_clique_no_mundo() {
        let mut j = Joystick::default();
        let mut g = crate::gesto_camera::GestoCamera::default();
        let passo =
            |j: &mut Joystick, g: &mut crate::gesto_camera::GestoCamera, q: &[ToqueNoQuadro]| {
                let antes = j.dedo();
                j.quadro(q, 100.0, &area_esquerda);
                g.quadro(&sem_dedos(q, &[antes, j.dedo()]), false)
            };
        use crate::gesto_camera::Acao;
        assert_eq!(
            passo(&mut j, &mut g, &[t(7, Fase::Comecou, 200.0, 700.0)]),
            Acao::Nada
        );
        assert_eq!(
            passo(&mut j, &mut g, &[t(7, Fase::Segurando, 260.0, 690.0)]),
            Acao::Nada
        );
        assert_eq!(
            passo(&mut j, &mut g, &[t(7, Fase::Acabou, 260.0, 690.0)]),
            Acao::Nada,
            "soltar nao clica"
        );
        assert!(!j.ativo());
        // A genuinely short touch, afterwards, is still a click.
        assert_eq!(
            passo(&mut j, &mut g, &[t(8, Fase::Comecou, 900.0, 300.0)]),
            Acao::Nada
        );
        assert!(matches!(
            passo(&mut j, &mut g, &[t(8, Fase::Acabou, 900.0, 300.0)]),
            Acao::Clique(_)
        ));
    }

    #[test]
    fn joystick_conta_como_andar_na_mao() {
        let mut j = Joystick::default();
        assert!(!movimento_manual(false, &j));
        assert!(movimento_manual(true, &j), "teclado continua valendo");
        j.quadro(&[t(1, Fase::Comecou, 200.0, 700.0)], 100.0, &area_esquerda);
        assert!(
            !movimento_manual(false, &j),
            "encostar sem empurrar nao pausa a auto missao"
        );
        j.quadro(
            &[t(1, Fase::Segurando, 260.0, 700.0)],
            100.0,
            &area_esquerda,
        );
        assert!(movimento_manual(false, &j), "empurrar pausa como WASD");
    }
}
