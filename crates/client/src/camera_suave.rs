//! Camera SUAVE: a entrada mexe num ALVO e a camera persegue.
//!
//! No iPhone os eventos de toque chegam em ritmo irregular — dois num quadro,
//! nenhum no seguinte. Somando o delta do dedo direto no angulo, a camera anda
//! aos degraus mesmo sem lag nenhum. Aqui o dedo (e o mouse, e a roda) move um
//! alvo; a camera vai ate' ele por aproximacao exponencial dependente de `dt`,
//! que nao passa do ponto e da' o mesmo resultado a 30 ou 120 quadros.
//!
//! Ao soltar o dedo sobra uma inercia curta: a velocidade do arrasto continua
//! empurrando o alvo e morre em ~0,35 s.
//!
//! O nucleo nao conhece a macroquad; `main::camera_controles` aplica.
use macroquad::prelude::Vec2;

/// Rapidez com que a camera alcanca o alvo (1/s). Alto o bastante pro mouse
/// continuar "na mao": em 1/60 s anda ~26% do caminho, em 0,15 s ~95%.
pub const K_CAMERA: f32 = 18.0;
/// Janela da media do delta do toque (s). Curta: engole o evento que chegou
/// dobrado sem atrasar a resposta.
pub const JANELA_DO_FILTRO_S: f32 = 0.05;
/// Constante de decaimento da inercia (s): ~5% da velocidade em 0,35 s.
pub const TAU_INERCIA_S: f32 = 0.117;
/// Abaixo disto (rad/s ou unidade/s) a inercia para.
const INERCIA_MINIMA: f32 = 0.02;

/// Aproximacao exponencial de `atual` ate' `alvo` em `dt`. Nunca passa do alvo.
pub fn suaviza(atual: f32, alvo: f32, dt: f32, k: f32) -> f32 {
    let f = 1.0 - (-dt.max(0.0) * k).exp();
    atual + (alvo - atual) * f
}

/// Diferenca angular pelo caminho curto, em [-pi, pi].
pub fn diferenca_angular(de: f32, para: f32) -> f32 {
    let tau = std::f32::consts::TAU;
    let mut d = (para - de) % tau;
    if d > std::f32::consts::PI {
        d -= tau;
    } else if d < -std::f32::consts::PI {
        d += tau;
    }
    d
}

/// A mesma aproximacao pra angulo: nunca da' a volta pelo lado longo.
pub fn suaviza_angulo(atual: f32, alvo: f32, dt: f32, k: f32) -> f32 {
    let f = 1.0 - (-dt.max(0.0) * k).exp();
    atual + diferenca_angular(atual, alvo) * f
}

/// Media exponencial curta do delta do dedo, por segundo. Guarda a VELOCIDADE
/// (px/s), que e' o que sobra de inercia quando o dedo sai.
#[derive(Debug, Default, Clone, Copy)]
pub struct FiltroDelta {
    vel: Vec2,
}

impl FiltroDelta {
    /// Entra o delta deste quadro (px) e o `dt`; sai o delta suavizado (px).
    pub fn filtra(&mut self, delta: Vec2, dt: f32) -> Vec2 {
        let dt = dt.max(1e-4);
        let a = 1.0 - (-dt / JANELA_DO_FILTRO_S).exp();
        self.vel += (delta / dt - self.vel) * a;
        self.vel * dt
    }

    /// Velocidade atual (px/s).
    pub fn velocidade(&self) -> Vec2 {
        self.vel
    }

    pub fn zera(&mut self) {
        self.vel = Vec2::ZERO;
    }
}

/// Inercia depois de soltar: velocidade que decai com atrito.
#[derive(Debug, Default, Clone, Copy)]
pub struct Inercia {
    vel: Vec2,
    /// Abaixo disto para: 5% da velocidade do soltar (~0,35 s com o TAU).
    limite: f32,
}

impl Inercia {
    pub fn solta(&mut self, vel: Vec2) {
        self.vel = vel;
        self.limite = (vel.length() * 0.05).max(INERCIA_MINIMA);
    }

    pub fn para(&mut self) {
        self.vel = Vec2::ZERO;
    }

    pub fn ativa(&self) -> bool {
        self.vel.length() > self.limite.max(INERCIA_MINIMA)
    }

    /// Quanto andar neste quadro; a velocidade decai.
    pub fn passo(&mut self, dt: f32) -> Vec2 {
        if !self.ativa() {
            self.vel = Vec2::ZERO;
            return Vec2::ZERO;
        }
        let d = self.vel * dt;
        self.vel *= (-dt.max(0.0) / TAU_INERCIA_S).exp();
        d
    }
}

/// Alvos da camera e o que foi escrito por ultimo — se outro codigo mexer no
/// valor (preferencia carregada, reset), o alvo adota o valor novo em vez de
/// arrasta-lo de volta.
#[derive(Debug, Default, Clone, Copy)]
pub struct CameraSuave {
    pub yaw: f32,
    pub ajuste: f32,
    pub zoom: f32,
    escrito: Option<(f32, f32, f32)>,
    pub filtro: FiltroDelta,
    pub inercia: Inercia,
}

impl CameraSuave {
    /// Adota os valores atuais como alvo quando alguem de fora os mudou.
    pub fn sincroniza(&mut self, yaw: f32, ajuste: f32, zoom: f32) {
        let mudou = match self.escrito {
            None => true,
            Some((y, a, z)) => (y - yaw).abs() > 1e-4 || (a - ajuste).abs() > 1e-4 || (z - zoom).abs() > 1e-4,
        };
        if mudou {
            self.yaw = yaw;
            self.ajuste = ajuste;
            self.zoom = zoom;
        }
    }

    /// Um quadro de perseguicao; devolve (yaw, ajuste, zoom) novos.
    pub fn persegue(&mut self, yaw: f32, ajuste: f32, zoom: f32, dt: f32) -> (f32, f32, f32) {
        let n = (
            suaviza_angulo(yaw, self.yaw, dt, K_CAMERA),
            suaviza(ajuste, self.ajuste, dt, K_CAMERA),
            suaviza(zoom, self.zoom, dt, K_CAMERA),
        );
        self.escrito = Some(n);
        n
    }

    /// Registra o valor recortado que realmente ficou (o `main` prende a
    /// inclinacao na banda do zoom depois de perseguir).
    pub fn escreveu(&mut self, yaw: f32, ajuste: f32, zoom: f32) {
        self.escrito = Some((yaw, ajuste, zoom));
    }
}

#[cfg(test)]
mod testes {
    use super::*;
    use macroquad::prelude::vec2;

    #[test]
    fn converge_sem_passar_do_alvo() {
        let mut v = 0.0f32;
        let mut anterior = 0.0;
        for _ in 0..120 {
            v = suaviza(v, 1.0, 1.0 / 60.0, K_CAMERA);
            assert!(v <= 1.0 + 1e-6, "passou do alvo: {v}");
            assert!(v >= anterior, "voltou pra tras");
            anterior = v;
        }
        assert!((v - 1.0).abs() < 1e-3, "nao chegou: {v}");
        // Mesmo tempo, quadros diferentes: mesmo lugar.
        let mut a = 0.0f32;
        for _ in 0..6 {
            a = suaviza(a, 1.0, 0.1 / 6.0, K_CAMERA);
        }
        let b = suaviza(0.0, 1.0, 0.1, K_CAMERA);
        assert!((a - b).abs() < 1e-4, "depende da taxa de quadros: {a} vs {b}");
        // Resposta rapida no PC: 0,15 s ja' andou ~93%.
        assert!(suaviza(0.0, 1.0, 0.15, K_CAMERA) > 0.9);
    }

    #[test]
    fn angulo_vai_pelo_caminho_curto() {
        let pi = std::f32::consts::PI;
        let d = diferenca_angular(pi - 0.1, -pi + 0.1);
        assert!((d - 0.2).abs() < 1e-4, "cruzar o -pi/pi: {d}");
        let v = suaviza_angulo(pi - 0.1, -pi + 0.1, 1.0, K_CAMERA);
        assert!((diferenca_angular(v, -pi + 0.1)).abs() < 1e-3, "chegou pelo lado curto");
    }

    #[test]
    fn filtro_absorve_eventos_irregulares() {
        let mut f = FiltroDelta::default();
        let dt = 1.0 / 60.0;
        // Dedo a 600 px/s, mas os eventos chegam: 20 px, 0, 0, 30 px...
        let brutos = [10.0, 0.0, 20.0, 0.0, 10.0, 0.0, 20.0, 0.0, 10.0, 0.0, 20.0, 0.0];
        let saidas: Vec<f32> = brutos.iter().map(|d| f.filtra(vec2(*d, 0.0), dt).x).collect();
        let (mn, mx) = saidas[4..].iter().fold((f32::MAX, f32::MIN), |(a, b), v| (a.min(*v), b.max(*v)));
        assert!(mx - mn < 20.0 * 0.5, "degrau nao foi suavizado: {saidas:?}");
        assert!(saidas.iter().all(|v| *v >= 0.0));
    }

    #[test]
    fn inercia_decai_em_menos_de_meio_segundo() {
        let mut i = Inercia::default();
        i.solta(vec2(2.0, 0.0));
        let mut total = 0.0;
        let mut t = 0.0;
        while i.ativa() && t < 2.0 {
            total += i.passo(1.0 / 60.0).x;
            t += 1.0 / 60.0;
        }
        assert!(t <= 0.5, "inercia durou {t} s");
        assert!(total > 0.0 && total < 2.0 * TAU_INERCIA_S * 1.1, "deslizou {total}");
    }

    #[test]
    fn valor_mudado_de_fora_vira_alvo() {
        let mut c = CameraSuave::default();
        c.sincroniza(0.0, 0.0, 1.0);
        let (y, a, z) = c.persegue(0.0, 0.0, 1.0, 1.0 / 60.0);
        c.escreveu(y, a, z);
        // Preferencia carregada muda o zoom: o alvo adota, sem puxar de volta.
        c.sincroniza(y, a, 2.0);
        assert_eq!(c.zoom, 2.0);
        let (_, _, z2) = c.persegue(y, a, 2.0, 1.0 / 60.0);
        assert!((z2 - 2.0).abs() < 1e-6);
    }
}
