//! Joystick VIRTUAL (toque), no molde do MIR4 mobile: WASD na metade esquerda.
//!
//! Flutuante: a base aparece onde o dedo encostou, o manipulo segue o dedo
//! preso ao raio. A saida e' uma direcao de TELA (x pra direita, y pra baixo,
//! igual ao WASD) vezes a intensidade; `main` converte pra mundo com o yaw da
//! camera e manda no `InputFrame` como o teclado.
//!
//! O dedo do joystick e' SO' dele: nao vira clique no mundo nem gira a camera
//! (`sem_dedo` tira ele da lista que vai pro `gesto_camera`).
//!
//! O nucleo nao conhece a macroquad; `main` le os toques e desenha.
use macroquad::prelude::*;

use crate::gesto_camera::{Fase, ToqueNoQuadro};
use crate::hud_estilo as estilo;

/// Raio do manipulo a 1920×1080 (px), multiplicado pela escala do HUD.
pub const RAIO_BASE: f32 = 78.0;
/// Fracao do raio que nao anda (dedo parado tremendo).
pub const ZONA_MORTA: f32 = 0.12;
/// Intensidade minima fora da zona morta: o servidor trata |dir|² <= 0,01
/// como "parado", entao abaixo disso o empurrao sutil seria ignorado.
pub const INTENSIDADE_MINIMA: f32 = 0.35;

#[derive(Debug, Default, Clone, Copy)]
pub struct Joystick {
    dedo: Option<u64>,
    base: Vec2,
    ponta: Vec2,
    raio: f32,
}

impl Joystick {
    /// Id do dedo que o joystick esta' usando.
    pub fn dedo(&self) -> Option<u64> {
        self.dedo
    }

    pub fn ativo(&self) -> bool {
        self.dedo.is_some()
    }

    pub fn soltar(&mut self) {
        self.dedo = None;
    }

    /// Um quadro: prende um dedo que COMECOU dentro da `area` (e fora do HUD —
    /// quem decide e' `pode_comecar`), acompanha ate' ele sair.
    pub fn quadro(&mut self, toques: &[ToqueNoQuadro], raio: f32, pode_comecar: &dyn Fn(Vec2) -> bool) {
        self.raio = raio.max(1.0);
        if let Some(id) = self.dedo {
            match toques.iter().find(|t| t.id == id) {
                Some(t) if t.fase != Fase::Acabou => self.ponta = t.pos,
                // Saiu (ou sumiu sem Acabou visivel): solta.
                _ => self.dedo = None,
            }
            return;
        }
        if let Some(t) = toques.iter().find(|t| t.fase == Fase::Comecou && pode_comecar(t.pos)) {
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

    /// O joystick esta' mandando andar (conta como teclado).
    pub fn movendo(&self) -> bool {
        self.direcao() != Vec2::ZERO
    }

    /// Onde desenhar o manipulo: preso ao raio.
    pub fn manipulo(&self) -> Vec2 {
        let v = self.ponta - self.base;
        if v.length() > self.raio {
            self.base + v.normalize() * self.raio
        } else {
            self.ponta
        }
    }

    pub fn desenha(&self) {
        if self.dedo.is_none() {
            return;
        }
        let r = self.raio;
        let b = self.base;
        draw_circle(b.x, b.y + 3.0, r + 6.0, Color::new(0.0, 0.0, 0.0, 0.22));
        draw_circle(b.x, b.y, r, estilo::alfa(estilo::FUNDO, 0.55));
        draw_circle_lines(b.x, b.y, r, 2.0, estilo::alfa(estilo::TEXTO, 0.35));
        draw_circle_lines(b.x, b.y, r * 0.55, 1.0, estilo::alfa(estilo::TEXTO, 0.12));
        let m = self.manipulo();
        let rm = r * 0.42;
        draw_circle(m.x, m.y, rm + 3.0, estilo::alfa(estilo::AUTO, 0.25));
        draw_circle(m.x, m.y, rm, estilo::alfa(estilo::FUNDO_ALTO, 0.9));
        draw_circle_lines(m.x, m.y, rm, 2.0, estilo::alfa(estilo::AUTO, 0.8));
        draw_circle(m.x - rm * 0.25, m.y - rm * 0.3, rm * 0.35, Color::new(1.0, 1.0, 1.0, 0.10));
    }
}

/// A lista de toques sem o dedo do joystick — e' o que vai pro gesto da
/// camera e pro clique no mundo.
#[cfg(test)]
pub fn sem_dedo(toques: &[ToqueNoQuadro], dedo: Option<u64>) -> Vec<ToqueNoQuadro> {
    sem_dedos(toques, &[dedo])
}

/// Igual, tirando varios. `main` passa o dedo de ANTES e o de DEPOIS do
/// `quadro`: no quadro em que o dedo solta o joystick ja' esqueceu o id, e o
/// `Acabou` dele caia no gesto como toque curto — clique no mundo, andar ate'
/// onde o polegar saiu.
pub fn sem_dedos(toques: &[ToqueNoQuadro], dedos: &[Option<u64>]) -> Vec<ToqueNoQuadro> {
    toques.iter().copied().filter(|t| !dedos.contains(&Some(t.id))).collect()
}

/// Andar "na mao": teclado OU joystick. E' o que pausa auto missao, viagem,
/// ir-para e a ida ate' o NPC, igual ao WASD.
pub fn movimento_manual(teclas: bool, joy: &Joystick) -> bool {
    teclas || joy.movendo()
}

#[cfg(test)]
mod testes {
    use super::*;

    fn t(id: u64, fase: Fase, x: f32, y: f32) -> ToqueNoQuadro {
        ToqueNoQuadro { id, fase, pos: vec2(x, y) }
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
        // Arrastar pra fora da area continua sendo o joystick.
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
        j.quadro(&[t(1, Fase::Segurando, 208.0, 700.0)], 100.0, &area_esquerda);
        assert_eq!(j.direcao(), Vec2::ZERO, "8% do raio e' zona morta");
        j.quadro(&[t(1, Fase::Segurando, 450.0, 700.0)], 100.0, &area_esquerda);
        let d = j.direcao();
        assert!((d.length() - 1.0).abs() < 1e-4 && d.x > 0.99, "alem do raio: intensidade 1 pra direita: {d}");
        assert!((j.manipulo() - vec2(300.0, 700.0)).length() < 1e-3, "manipulo preso ao raio");
        j.quadro(&[t(1, Fase::Segurando, 200.0, 680.0)], 100.0, &area_esquerda);
        let d = j.direcao();
        assert!(d.y < 0.0 && (d.length() - INTENSIDADE_MINIMA).abs() < 1e-4, "empurrao leve pra cima: {d}");
    }

    #[test]
    fn direcao_de_tela_vira_mundo_como_o_wasd() {
        let mut j = Joystick::default();
        j.quadro(&[t(1, Fase::Comecou, 200.0, 700.0)], 100.0, &area_esquerda);
        j.quadro(&[t(1, Fase::Segurando, 200.0, 500.0)], 100.0, &area_esquerda);
        // Pra cima na tela = W.
        let w = crate::render3d::input_para_mundo(vec2(0.0, -1.0), 0.7);
        let joy = crate::render3d::input_para_mundo(j.direcao(), 0.7);
        assert!((w - joy).length() < 1e-4, "joystick pra cima = W: {w} vs {joy}");
    }

    #[test]
    fn dedo_do_joystick_nao_vai_pra_camera_nem_clique() {
        let mut j = Joystick::default();
        let quadro = [t(1, Fase::Comecou, 200.0, 700.0), t(2, Fase::Comecou, 900.0, 300.0)];
        j.quadro(&quadro, 100.0, &area_esquerda);
        let resto = sem_dedo(&quadro, j.dedo());
        assert_eq!(resto.len(), 1);
        assert_eq!(resto[0].id, 2, "so' o outro dedo vai pro gesto da camera");
        // Com so' o joystick, o gesto nao recebe nada: nada de clique no soltar.
        let mut g = crate::gesto_camera::GestoCamera::default();
        let so_joy = [t(1, Fase::Acabou, 200.0, 700.0)];
        assert_eq!(g.quadro(&sem_dedo(&so_joy, Some(1)), false), crate::gesto_camera::Acao::Nada);
    }

    /// O bug do iPhone: soltar o polegar do joystick andava ate' ali (clique).
    #[test]
    fn soltar_o_joystick_nao_vira_clique_no_mundo() {
        let mut j = Joystick::default();
        let mut g = crate::gesto_camera::GestoCamera::default();
        let passo = |j: &mut Joystick, g: &mut crate::gesto_camera::GestoCamera, q: &[ToqueNoQuadro]| {
            let antes = j.dedo();
            j.quadro(q, 100.0, &area_esquerda);
            g.quadro(&sem_dedos(q, &[antes, j.dedo()]), false)
        };
        use crate::gesto_camera::Acao;
        assert_eq!(passo(&mut j, &mut g, &[t(7, Fase::Comecou, 200.0, 700.0)]), Acao::Nada);
        assert_eq!(passo(&mut j, &mut g, &[t(7, Fase::Segurando, 260.0, 690.0)]), Acao::Nada);
        assert_eq!(passo(&mut j, &mut g, &[t(7, Fase::Acabou, 260.0, 690.0)]), Acao::Nada, "soltar nao clica");
        assert!(!j.ativo());
        // Um toque curto de verdade, depois, continua sendo clique.
        assert_eq!(passo(&mut j, &mut g, &[t(8, Fase::Comecou, 900.0, 300.0)]), Acao::Nada);
        assert!(matches!(passo(&mut j, &mut g, &[t(8, Fase::Acabou, 900.0, 300.0)]), Acao::Clique(_)));
    }

    #[test]
    fn joystick_conta_como_andar_na_mao() {
        let mut j = Joystick::default();
        assert!(!movimento_manual(false, &j));
        assert!(movimento_manual(true, &j), "teclado continua valendo");
        j.quadro(&[t(1, Fase::Comecou, 200.0, 700.0)], 100.0, &area_esquerda);
        assert!(!movimento_manual(false, &j), "encostar sem empurrar nao pausa a auto missao");
        j.quadro(&[t(1, Fase::Segurando, 260.0, 700.0)], 100.0, &area_esquerda);
        assert!(movimento_manual(false, &j), "empurrar pausa como WASD");
    }
}
