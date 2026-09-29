//! Death in the MIR4 mould: a defeat screen over everything, with "Revive in
//! town" and "Recover XP", and the Menu's "Recover experience" panel.
//!
//! The server decides everything (losing 10% of the level, 3 free recoveries
//! per day, a gold cost after that — see docs/GAMEPLAY.md, Death). Here we
//! only show and ask. No key opens or revives: click only.
use macroquad::prelude::*;
use shared::protocol::{ClientMessage, MorteRecuperavelNet};

use crate::hud_estilo as estilo;

const VERMELHO: Color = Color::new(0.86, 0.32, 0.28, 1.0);

#[derive(Default)]
pub struct Morte {
    /// Down: the defeat screen is in front.
    pub morto: bool,
    /// XP lost on the last death (0 = none, or already recovered).
    pub xp_perdido: u64,
    pub mortes: Vec<MorteRecuperavelNet>,
    pub gratis: u8,
    /// The "Recover experience" panel opened from the Menu.
    pub painel: bool,
    aviso: Option<(String, bool)>,
}

impl Morte {
    pub fn morreu(&mut self, xp_perdido: u64) {
        self.morto = true;
        self.xp_perdido = xp_perdido;
        self.aviso = None;
    }

    /// `DownedUpdate`: entrou ou saiu do chao. Saiu = reviveu.
    pub fn caido(&mut self, ativo: bool) {
        if ativo {
            self.morto = true;
        } else if self.morto {
            self.morto = false;
            self.xp_perdido = 0;
            self.aviso = None;
        }
    }

    pub fn recuperaveis(&mut self, mortes: Vec<MorteRecuperavelNet>, gratis: u8) {
        self.mortes = mortes;
        self.gratis = gratis;
    }

    pub fn resultado(&mut self, ok: bool, motivo: String) {
        if ok {
            self.xp_perdido = 0;
        }
        self.aviso = Some((motivo, ok));
    }

    /// The click belongs to the death screen or the panel, not to the world.
    pub fn pega_mouse(&self) -> bool {
        self.morto || self.painel
    }

    /// The most recent death that can still be recovered.
    pub fn ultima(&self) -> Option<MorteRecuperavelNet> {
        self.mortes.iter().max_by_key(|m| m.quando).copied()
    }

    /// Texto do botao: gratis enquanto houver, senao o preco.
    pub fn rotulo_recuperar(m: &MorteRecuperavelNet, gratis: u8) -> String {
        if gratis > 0 {
            format!("Recuperar XP (grátis {gratis}/3)")
        } else {
            format!("Recuperar XP · {} ouro", milhar(m.custo_gold))
        }
    }

    /// (Revive, Recover) on the death screen, for an `sw`x`sh` screen.
    pub fn botoes_da_tela(sw: f32, sh: f32) -> (Rect, Rect) {
        let (w, h) = (240.0, 44.0);
        let y = sh * 0.5 + 72.0;
        (
            Rect::new(sw * 0.5 - w - 8.0, y, w, h),
            Rect::new(sw * 0.5 + 8.0, y, w, h),
        )
    }

    /// Draws whatever is open and returns the click's request.
    pub fn desenha(&mut self, ouro: u64, agora: i64) -> Option<ClientMessage> {
        let mut pedido = None;
        if self.painel {
            pedido = self.desenha_painel(ouro, agora);
        }
        if self.morto {
            pedido = self.desenha_tela(ouro).or(pedido);
        }
        pedido
    }

    fn desenha_tela(&mut self, ouro: u64) -> Option<ClientMessage> {
        let (sw, sh) = (screen_width(), screen_height());
        draw_rectangle(0.0, 0.0, sw, sh, Color::new(0.08, 0.0, 0.0, 0.55));
        let caixa = Rect::new(sw * 0.5 - 280.0, sh * 0.5 - 140.0, 560.0, 280.0);
        estilo::painel(caixa);
        estilo::texto_centro(sw * 0.5, caixa.y + 44.0, "VOCÊ FOI DERROTADO", 30, VERMELHO);
        let linha = if self.xp_perdido > 0 {
            format!(
                "Experiência perdida: {} (recuperável por 24 h)",
                milhar(self.xp_perdido)
            )
        } else {
            "Nenhuma experiência a recuperar desta morte.".to_string()
        };
        estilo::texto_centro(sw * 0.5, caixa.y + 80.0, &linha, 16, estilo::TEXTO);
        estilo::texto_centro(sw * 0.5, caixa.y + 112.0,
            "Cada proficiência treinada perde XP ao morrer:", 15, VERMELHO);
        estilo::texto_centro(sw * 0.5, caixa.y + 135.0,
            "10% do custo do próximo nível. Seu nível pode cair.", 14, estilo::TEXTO);
        estilo::texto_centro(sw * 0.5, caixa.y + 158.0,
            "Recuperar XP não devolve proficiência.", 14, estilo::SUAVE);
        if let Some((t, ok)) = &self.aviso {
            estilo::texto_centro(
                sw * 0.5,
                caixa.y + 186.0,
                t,
                14,
                if *ok { estilo::AUTO } else { VERMELHO },
            );
        }
        let (reviver, recuperar) = Self::botoes_da_tela(sw, sh);
        let mut pedido = None;
        if botao(reviver, "Reviver na cidade", true) {
            pedido = Some(ClientMessage::RespawnAtCity);
        }
        if let Some(m) = self.ultima().filter(|_| self.xp_perdido > 0) {
            let pode = self.gratis > 0 || ouro >= m.custo_gold;
            if botao(recuperar, &Self::rotulo_recuperar(&m, self.gratis), pode) && pode {
                pedido = Some(ClientMessage::RecuperarXp { quando: m.quando });
            }
        }
        pedido
    }

    fn desenha_painel(&mut self, ouro: u64, agora: i64) -> Option<ClientMessage> {
        let (sw, sh) = (screen_width(), screen_height());
        draw_rectangle(0.0, 0.0, sw, sh, Color::new(0.0, 0.0, 0.0, 0.45));
        let linhas = self.mortes.len().max(1) as f32;
        let caixa = Rect::new(
            sw * 0.5 - 300.0,
            sh * 0.5 - 60.0 - linhas * 24.0,
            600.0,
            140.0 + linhas * 48.0,
        );
        estilo::painel(caixa);
        estilo::texto(
            caixa.x + 20.0,
            caixa.y + 34.0,
            "RECUPERAR EXPERIÊNCIA",
            22,
            estilo::OURO,
        );
        let gratis = format!("Grátis hoje: {}/3 · depois custa ouro", self.gratis);
        estilo::texto(caixa.x + 20.0, caixa.y + 58.0, &gratis, 14, estilo::SUAVE);
        let fechar = Rect::new(caixa.x + caixa.w - 40.0, caixa.y + 12.0, 28.0, 28.0);
        if botao(fechar, "X", true) {
            self.painel = false;
        }
        let mut pedido = None;
        if self.mortes.is_empty() {
            estilo::texto(
                caixa.x + 20.0,
                caixa.y + 104.0,
                "Nenhuma morte para recuperar.",
                16,
                estilo::TEXTO,
            );
        }
        let mut ordem = self.mortes.clone();
        ordem.sort_by_key(|m| std::cmp::Reverse(m.quando));
        for (i, m) in ordem.iter().enumerate() {
            let y = caixa.y + 80.0 + i as f32 * 48.0;
            let resta = ((m.expira - agora).max(0) as f32 / 3600.0).ceil() as i64;
            let t = format!("{} XP · expira em {resta} h", milhar(m.xp));
            estilo::texto(caixa.x + 20.0, y + 28.0, &t, 16, estilo::TEXTO);
            let pode = self.gratis > 0 || ouro >= m.custo_gold;
            let b = Rect::new(caixa.x + caixa.w - 270.0, y + 4.0, 250.0, 38.0);
            if botao(b, &Self::rotulo_recuperar(m, self.gratis), pode) && pode {
                pedido = Some(ClientMessage::RecuperarXp { quando: m.quando });
            }
        }
        if let Some((t, ok)) = &self.aviso {
            estilo::texto(
                caixa.x + 20.0,
                caixa.y + caixa.h - 16.0,
                t,
                14,
                if *ok { estilo::AUTO } else { VERMELHO },
            );
        }
        pedido
    }
}

/// Botao simples no estilo do HUD. Devolve `true` no clique.
fn botao(r: Rect, texto: &str, ativo: bool) -> bool {
    let sobre = r.contains(Vec2::from(mouse_position()));
    let cor = if !ativo {
        estilo::SUAVE
    } else if sobre {
        estilo::OURO
    } else {
        estilo::BORDA
    };
    draw_rectangle(r.x, r.y, r.w, r.h, estilo::FUNDO);
    draw_rectangle_lines(r.x, r.y, r.w, r.h, 2.0, cor);
    estilo::texto_centro(
        r.x + r.w * 0.5,
        r.y + r.h * 0.5 + 6.0,
        texto,
        16,
        if ativo { estilo::TEXTO } else { estilo::SUAVE },
    );
    ativo && sobre && crate::foco::clique()
}

/// 1200 -> "1.200".
pub fn milhar(n: u64) -> String {
    let s = n.to_string();
    let mut out = String::new();
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (s.len() - i) % 3 == 0 {
            out.push('.');
        }
        out.push(c);
    }
    out
}

#[cfg(test)]
mod testes {
    use super::*;

    fn m(quando: i64, xp: u64, custo: u64) -> MorteRecuperavelNet {
        MorteRecuperavelNet {
            quando,
            xp,
            expira: quando + 86_400,
            custo_gold: custo,
        }
    }

    #[test]
    fn morrer_e_reviver_mudam_a_tela() {
        let mut t = Morte::default();
        assert!(!t.pega_mouse());
        t.morreu(340);
        assert!(t.morto && t.pega_mouse());
        assert_eq!(t.xp_perdido, 340);
        t.resultado(true, "ok".into());
        assert_eq!(t.xp_perdido, 0, "recuperou: some o botao");
        t.caido(false);
        assert!(!t.morto && !t.pega_mouse(), "reviveu: tela fecha");
        t.caido(true);
        assert!(t.morto, "DownedUpdate sozinho tambem abre");
    }

    #[test]
    fn botao_mostra_gratis_ou_preco_e_pega_a_ultima() {
        let mut t = Morte::default();
        t.recuperaveis(vec![m(10, 50, 900), m(30, 70, 1200), m(20, 60, 1000)], 2);
        assert_eq!(t.ultima().map(|x| x.quando), Some(30));
        assert_eq!(
            Morte::rotulo_recuperar(&m(1, 1, 1200), 2),
            "Recuperar XP (grátis 2/3)"
        );
        assert_eq!(
            Morte::rotulo_recuperar(&m(1, 1, 1200), 0),
            "Recuperar XP · 1.200 ouro"
        );
        assert_eq!(milhar(1234567), "1.234.567");
        assert_eq!(milhar(999), "999");
    }

    #[test]
    fn botoes_da_tela_nao_se_sobrepoem_e_cabem() {
        for (sw, sh) in [(1920.0, 1080.0), (1280.0, 720.0), (1024.0, 768.0)] {
            let (a, b) = Morte::botoes_da_tela(sw, sh);
            assert!(a.intersect(b).is_none());
            for r in [a, b] {
                assert!(r.x >= 0.0 && r.right() <= sw && r.bottom() <= sh);
            }
        }
    }
}
