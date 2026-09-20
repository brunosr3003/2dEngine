//! Rolagem de lista, igual em todo painel: roda do mouse, ARRASTO (dedo ou
//! mouse) dentro da lista e uma barra visivel a' direita, que tambem se
//! arrasta.
//!
//! Antes cada painel lia so' a roda — no celular nao existe roda, e as listas
//! do Craft, das Missoes e das Diarias simplesmente nao rolavam. E sem barra
//! nada dizia que havia mais embaixo.
//!
//! Arrastar e tocar comecam do mesmo jeito (o dedo encosta), entao o toque
//! numa LINHA so' vale no soltar e se o dedo nao andou: `quadro` devolve esse
//! clique, e a lista usa ele no lugar do `is_mouse_button_pressed`.

use macroquad::prelude::*;

use crate::hud_estilo as estilo;

/// Largura da barra desenhada e da faixa que pega o dedo nela (a 100%; crescem
/// com a escala do painel).
const LARGURA_BARRA: f32 = 6.0;
const FAIXA_DA_BARRA: f32 = 26.0;

#[derive(Debug, Clone, Copy)]
struct Gesto {
    /// Onde o dedo encostou e a rolagem naquele instante.
    y0: f32,
    pos0: f32,
    inicio: Vec2,
    arrastando: bool,
    /// Pegou na barra: o dedo move o polegar, nao o conteudo.
    na_barra: bool,
}

#[derive(Debug, Default)]
pub struct Rolagem {
    /// Quanto do conteudo ja' passou pra cima, em px.
    pub pos: f32,
    gesto: Option<Gesto>,
}

/// O que um quadro da lista precisa saber.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Entrada {
    pub mouse: Vec2,
    pub apertou: bool,
    pub segurando: bool,
    pub soltou: bool,
    pub roda: f32,
}

impl Entrada {
    pub fn agora() -> Self {
        Entrada {
            mouse: Vec2::from(mouse_position()),
            apertou: crate::foco::clique(),
            segurando: is_mouse_button_down(MouseButton::Left),
            soltou: is_mouse_button_released(MouseButton::Left),
            roda: mouse_wheel().1,
        }
    }
}

/// Maior rolagem possivel.
pub fn maximo(area: Rect, total: f32) -> f32 {
    (total - area.h).max(0.0)
}

/// A faixa da barra (trilho), a' direita da lista.
pub fn trilho(area: Rect) -> Rect {
    Rect::new(
        area.x + area.w - estilo::u(LARGURA_BARRA) - 2.0,
        area.y + 2.0,
        estilo::u(LARGURA_BARRA),
        area.h - 4.0,
    )
}

/// (y, altura) do polegar dentro do trilho.
fn polegar(t: Rect, area: Rect, total: f32, pos: f32) -> (f32, f32) {
    let frac = (area.h / total.max(1.0)).clamp(0.05, 1.0);
    let h = (t.h * frac).max(t.w * 4.0).min(t.h);
    let max = maximo(area, total);
    let y = t.y + (t.h - h) * if max > 0.0 { pos / max } else { 0.0 };
    (y, h)
}

impl Rolagem {
    pub fn zera(&mut self) {
        self.pos = 0.0;
        self.gesto = None;
    }

    /// Um dedo esta' arrastando esta lista (as linhas nao devem acender).
    pub fn arrastando(&self) -> bool {
        self.gesto.is_some_and(|g| g.arrastando)
    }

    /// Um quadro com a entrada de verdade (ver `passo`). A barra se desenha a
    /// parte, com `desenha`, DEPOIS das linhas — por cima delas.
    pub fn quadro(&mut self, area: Rect, total: f32, passo_da_roda: f32) -> Option<Vec2> {
        self.passo(area, total, passo_da_roda, Entrada::agora())
    }

    /// A logica, sem desenho: roda, arrasto no conteudo e na barra. Devolve o
    /// CLIQUE na lista — o ponto onde o dedo soltou sem ter arrastado.
    pub fn passo(
        &mut self,
        area: Rect,
        total: f32,
        passo_da_roda: f32,
        e: Entrada,
    ) -> Option<Vec2> {
        let max = maximo(area, total);
        if area.contains(e.mouse) && e.roda != 0.0 {
            self.pos -= e.roda.signum() * passo_da_roda;
        }
        if e.apertou && area.contains(e.mouse) {
            let faixa = Rect::new(
                area.x + area.w - estilo::u(FAIXA_DA_BARRA),
                area.y,
                estilo::u(FAIXA_DA_BARRA),
                area.h,
            );
            self.gesto = Some(Gesto {
                y0: e.mouse.y,
                pos0: self.pos,
                inicio: e.mouse,
                arrastando: false,
                na_barra: max > 0.0 && faixa.contains(e.mouse),
            });
        }
        let mut clique = None;
        if let Some(mut g) = self.gesto {
            let dy = e.mouse.y - g.y0;
            if !g.arrastando && (e.mouse - g.inicio).length() > crate::toque::TOLERANCIA_PX {
                g.arrastando = true;
            }
            if g.na_barra {
                // O polegar anda com o dedo: um trilho inteiro = a lista inteira.
                let t = trilho(area);
                let (_, h) = polegar(t, area, total, self.pos);
                let curso = (t.h - h).max(1.0);
                self.pos = g.pos0 + dy / curso * max;
            } else if g.arrastando {
                // O conteudo anda COM o dedo: puxar pra cima mostra o de baixo.
                self.pos = g.pos0 - dy;
            }
            if e.soltou || !e.segurando {
                if !g.arrastando && !g.na_barra && e.soltou {
                    clique = Some(e.mouse);
                }
                self.gesto = None;
            } else {
                self.gesto = Some(g);
            }
        }
        self.pos = self.pos.clamp(0.0, max);
        clique
    }

    /// A barra, so' quando ha' o que rolar.
    pub fn desenha(&self, area: Rect, total: f32) {
        if maximo(area, total) <= 0.0 {
            return;
        }
        let t = trilho(area);
        estilo::ret_arredondado(t, t.w * 0.5, estilo::alfa(estilo::FUNDO_BAIXO, 0.85));
        let (y, h) = polegar(t, area, total, self.pos);
        let cor = if self.gesto.is_some_and(|g| g.na_barra) {
            estilo::OURO
        } else {
            estilo::alfa(estilo::SUAVE, 0.75)
        };
        estilo::ret_arredondado(Rect::new(t.x, y, t.w, h), t.w * 0.5, cor);
    }
}

/// Recorta o desenho a `area` (linha pela metade nao vaza pro cabecalho do
/// painel); `None` volta ao normal. Coordenadas de tela, as mesmas do HUD.
pub fn recortar(area: Option<Rect>) {
    let clip = area.map(|a| {
        (
            a.x.floor() as i32,
            a.y.floor() as i32,
            a.w.ceil().max(0.0) as i32,
            a.h.ceil().max(0.0) as i32,
        )
    });
    unsafe { get_internal_gl() }.quad_gl.scissor(clip);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn e(x: f32, y: f32, apertou: bool, segurando: bool, soltou: bool) -> Entrada {
        Entrada {
            mouse: vec2(x, y),
            apertou,
            segurando,
            soltou,
            roda: 0.0,
        }
    }

    const AREA: Rect = Rect {
        x: 0.0,
        y: 0.0,
        w: 300.0,
        h: 400.0,
    };

    #[test]
    fn toque_parado_e_clique_no_soltar() {
        let mut r = Rolagem::default();
        assert_eq!(r.passo(AREA, 1000.0, 40.0, e(50.0, 100.0, true, true, false)), None);
        assert_eq!(
            r.passo(AREA, 1000.0, 40.0, e(52.0, 101.0, false, false, true)),
            Some(vec2(52.0, 101.0))
        );
        assert_eq!(r.pos, 0.0);
    }

    #[test]
    fn arrastar_rola_com_o_dedo_e_nao_clica() {
        let mut r = Rolagem::default();
        r.passo(AREA, 1000.0, 40.0, e(50.0, 300.0, true, true, false));
        r.passo(AREA, 1000.0, 40.0, e(50.0, 200.0, false, true, false));
        assert!(r.arrastando());
        assert_eq!(r.pos, 100.0, "puxou 100 pra cima, a lista subiu 100");
        let c = r.passo(AREA, 1000.0, 40.0, e(50.0, 150.0, false, false, true));
        assert_eq!(c, None, "arrasto nao vira clique");
        assert_eq!(r.pos, 150.0);
        // Nao passa do fim nem do comeco.
        r.passo(AREA, 1000.0, 40.0, e(50.0, 390.0, true, true, false));
        r.passo(AREA, 1000.0, 40.0, e(50.0, -2000.0, false, true, false));
        assert_eq!(r.pos, 600.0);
    }

    #[test]
    fn a_barra_arrasta_a_lista_inteira() {
        let mut r = Rolagem::default();
        let t = trilho(AREA);
        r.passo(AREA, 1000.0, 40.0, e(t.x + 2.0, 10.0, true, true, false));
        r.passo(AREA, 1000.0, 40.0, e(t.x + 2.0, 1000.0, false, true, false));
        assert_eq!(r.pos, maximo(AREA, 1000.0), "puxou o polegar ate' o fim");
    }

    #[test]
    fn roda_e_lista_que_cabe() {
        let mut r = Rolagem::default();
        let mut x = e(50.0, 50.0, false, false, false);
        x.roda = -1.0;
        r.passo(AREA, 1000.0, 40.0, x);
        assert_eq!(r.pos, 40.0);
        let mut curta = Rolagem::default();
        curta.passo(AREA, 200.0, 40.0, x);
        assert_eq!(curta.pos, 0.0, "cabe inteira: nao rola");
        // Na lista que cabe, a faixa da barra e' linha como outra qualquer.
        let t = trilho(AREA);
        curta.passo(AREA, 200.0, 40.0, e(t.x + 2.0, 60.0, true, true, false));
        assert!(curta
            .passo(AREA, 200.0, 40.0, e(t.x + 2.0, 60.0, false, false, true))
            .is_some());
    }
}
