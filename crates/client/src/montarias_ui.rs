//! Montarias (docs/MONTARIAS.md): Menu → Personagem → Montaria. Escolhe a
//! montaria e a skin que vale ao montar; montar e desmontar e' pelo botao do
//! HUD (ao lado da bateria). Quem nao tem montaria e' mandado pra Loja.
use macroquad::prelude::*;
use shared::loja::{self as cat, AvisoLoja, EstadoLoja, PedidoLoja};
use shared::protocol::ClientMessage;

use crate::hud_estilo as estilo;
use crate::loja_tp::{cor_da_skin, medalhao};

/// O que a janela pede ao jogo.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Acao {
    /// Esta skin passa a valer (vai pras preferencias).
    Escolher(u16),
    AbrirLoja,
    Montar,
}

#[derive(Default)]
pub struct MontariasUi {
    pub aberto: bool,
    estado: Option<EstadoLoja>,
    /// Relogio (get_time) em que a montada termina; 0 = nao esta' montando.
    montando_ate: f64,
    montando_desde: f64,
}

impl MontariasUi {
    pub fn abrir(&mut self) -> Vec<ClientMessage> {
        self.aberto = true;
        vec![ClientMessage::Loja { pedido: PedidoLoja::Estado }]
    }

    pub fn fechar(&mut self) {
        self.aberto = false;
    }

    pub fn receber(&mut self, aviso: &AvisoLoja, agora: f64) {
        match aviso {
            AvisoLoja::Estado(e) => self.estado = Some(e.clone()),
            AvisoLoja::Montando { segundos } => {
                if *segundos > 0.0 {
                    self.montando_desde = agora;
                    self.montando_ate = agora + *segundos as f64;
                } else {
                    self.montando_ate = 0.0;
                }
            }
            AvisoLoja::Resultado { .. } => {}
        }
    }

    pub fn tem_montaria(&self) -> bool {
        self.estado.as_ref().is_some_and(|e| !e.posses.montarias.is_empty())
    }

    pub fn montando(&self, agora: f64) -> bool {
        self.montando_ate > 0.0 && agora < self.montando_ate + 0.5
    }

    /// 0..1 enquanto sobe na montaria.
    pub fn progresso(&self, agora: f64) -> Option<f32> {
        if self.montando_ate <= 0.0 || agora >= self.montando_ate {
            return None;
        }
        let total = (self.montando_ate - self.montando_desde).max(0.01);
        Some(((agora - self.montando_desde) / total).clamp(0.0, 1.0) as f32)
    }

    /// Ja' montou (o flag chegou): esquece a barra.
    pub fn montou(&mut self) {
        self.montando_ate = 0.0;
    }

    pub fn desenha(&mut self, escolhida: Option<u16>) -> Option<Acao> {
        if !self.aberto {
            return None;
        }
        let f = estilo::fator_texto();
        let seguro = crate::hud_layout::tela_segura();
        let w = (760.0 * f).min(seguro.w - 16.0);
        let h = (560.0 * f).min(seguro.h - 16.0);
        let p = Rect::new(seguro.center().x - w * 0.5, seguro.center().y - h * 0.5, w, h);
        crate::hud_layout::escurece(0.45);
        estilo::painel(p);
        let m = Vec2::from(mouse_position());
        let clicou = is_mouse_button_pressed(MouseButton::Left);
        let x0 = p.x + 20.0 * f;
        estilo::texto_forte(x0, p.y + 36.0 * f, "Montarias", 22, estilo::OURO);
        let fechar = Rect::new(p.x + p.w - 48.0 * f, p.y + 8.0 * f, 40.0 * f, 40.0 * f);
        estilo::texto_centro(fechar.center().x, fechar.center().y + 7.0 * f, "X", 18, estilo::TEXTO);
        if clicou && fechar.contains(m) {
            self.fechar();
            return None;
        }
        let Some(estado) = self.estado.clone() else {
            estilo::texto(x0, p.y + 80.0 * f, "Carregando…", 15, estilo::SUAVE);
            return None;
        };
        let mut acao = None;
        estilo::texto(x0, p.y + 64.0 * f, &format!("Montado: velocidade +{:.0}%. Só mobilidade — nada de combate muda.", (cat::VEL_MONTADO - 1.0) * 100.0), 13, estilo::SUAVE);
        let valendo = estado.posses.skin_para_montar(escolhida);
        let mut y = p.y + 84.0 * f;
        if estado.posses.montarias.is_empty() {
            estilo::texto(x0, y + 30.0 * f, "Você ainda não tem montaria.", 17, estilo::TEXTO);
            estilo::texto(x0, y + 56.0 * f, "Onde obter: Loja (Menu → Comércio → Loja).", 14, estilo::SUAVE);
        }
        let linha_h = 104.0 * f;
        for mid in &estado.posses.montarias {
            let Some(mt) = cat::montaria(*mid) else { continue };
            let r = Rect::new(x0, y, p.w - 40.0 * f, linha_h - 10.0 * f);
            estilo::cartao(r, false, valendo.is_some_and(|s| cat::skin(s).is_some_and(|s| s.montaria == mt.id)));
            let raio = 30.0 * f;
            let skin_da_linha = valendo.filter(|s| cat::skin(*s).is_some_and(|s| s.montaria == mt.id)).unwrap_or(mt.skin_padrao);
            medalhao(vec2(r.x + 16.0 * f + raio, r.center().y), raio, cor_da_skin(skin_da_linha), &mt.nome.chars().next().unwrap_or('?').to_string());
            let tx = r.x + 32.0 * f + raio * 2.0;
            estilo::texto_forte(tx, r.y + 30.0 * f, mt.nome, 17, estilo::TEXTO);
            // As skins da conta desta montaria (a padrao vem junto).
            let mut cx = tx;
            for s in cat::SKINS.iter().filter(|s| s.montaria == mt.id && (s.preco_tp == 0 || estado.posses.skins.contains(&s.id))) {
                let tw = estilo::medir(s.nome, 13) + 38.0 * f;
                let chip = Rect::new(cx, r.y + 44.0 * f, tw, 36.0 * f);
                let sel = valendo == Some(s.id);
                estilo::cartao(chip, chip.contains(m), sel);
                draw_circle(chip.x + 14.0 * f, chip.center().y, 7.0 * f, cor_da_skin(s.id));
                estilo::texto(chip.x + 26.0 * f, chip.center().y + 5.0 * f, s.nome, 13, if sel { estilo::OURO } else { estilo::TEXTO });
                if clicou && chip.contains(m) {
                    acao = Some(Acao::Escolher(s.id));
                }
                cx += tw + 8.0 * f;
                if cx > r.x + r.w - 120.0 * f {
                    break;
                }
            }
            y += linha_h;
            if y > p.y + p.h - 150.0 * f {
                break;
            }
        }

        let bw = 200.0 * f;
        let loja = Rect::new(p.x + p.w - 20.0 * f - bw, p.y + p.h - 70.0 * f, bw, 50.0 * f);
        estilo::cartao(loja, loja.contains(m), estado.posses.montarias.is_empty());
        estilo::texto_centro_forte(loja.center().x, loja.center().y + 6.0 * f, "Ir para a Loja", 16, estilo::OURO);
        if clicou && loja.contains(m) {
            acao = Some(Acao::AbrirLoja);
        }
        if !estado.posses.montarias.is_empty() {
            let montar = Rect::new(loja.x - 12.0 * f - bw, loja.y, bw, 50.0 * f);
            estilo::cartao(montar, montar.contains(m), true);
            estilo::texto_centro_forte(montar.center().x, montar.center().y + 6.0 * f, "Montar", 16, estilo::OURO);
            if clicou && montar.contains(m) {
                acao = Some(Acao::Montar);
            }
        }
        acao
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sem_estado_nao_tem_montaria_e_montando_so_com_aviso() {
        let mut u = MontariasUi::default();
        assert!(!u.tem_montaria());
        u.receber(&AvisoLoja::Estado(EstadoLoja { posses: cat::Posses { montarias: vec![2], skins: vec![] }, ..Default::default() }), 0.0);
        assert!(u.tem_montaria());
        assert!(!u.montando(0.0));
        u.montando_desde = 10.0;
        u.montando_ate = 11.0;
        assert!(u.montando(10.5));
        assert_eq!(u.progresso(10.5), Some(0.5));
        u.receber(&AvisoLoja::Montando { segundos: 0.0 }, 10.5);
        assert!(u.progresso(10.5).is_none());
    }
}
