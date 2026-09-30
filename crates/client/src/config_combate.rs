//! Configuracao do AUTO COMBATE: a ORDEM em que ele escolhe alvo e ate' onde
//! vai contra jogador. Abre com o botao direito no AUTO COMBATE, no molde do
//! `config_coleta`. Salva nas preferencias do personagem.
//!
//! O dono: "quando eu tiver fazendo missão tem que ser o mob da missão, mas
//! se eu tiver igual estou agora na Ilha Mágica eu tenho que poder escolher:
//! inimigos ranged que estão me atacando de longe, inimigos que estão
//! próximos, player, ou auto atacar player que atacou ou não".
//!
//! A MISSÃO NAO APARECE NA LISTA porque nao e' ajustavel: quem liga o auto no
//! meio de uma caca quer a caca andando. A tela diz isso em uma linha, pra
//! ninguem procurar o ajuste que nao existe.
use macroquad::prelude::*;

use crate::hud_estilo as estilo;
use shared::protocol::{auto_alvo, auto_pvp};

#[derive(Default)]
pub struct ConfigCombate {
    pub aberto: bool,
}

/// O nome de cada categoria, na tela.
pub fn nome_da_categoria(v: u8) -> &'static str {
    match v {
        auto_alvo::RANGED_EM_MIM => "Whoever hits me from afar",
        auto_alvo::MAIS_PERTO => "The nearest",
        auto_alvo::JOGADOR => "Player",
        _ => "?",
    }
}

/// Uma linha a mais pra explicar POR QUE a ordem importa. Sem isso a tela e'
/// tres frases soltas e ninguem sabe o que muda ao trocar.
pub fn dica_da_categoria(v: u8) -> &'static str {
    match v {
        auto_alvo::RANGED_EM_MIM => "você bate no de perto e continua apanhando",
        auto_alvo::MAIS_PERTO => "o de sempre: o que estiver mais próximo",
        auto_alvo::JOGADOR => "só entra se o PvP abaixo permitir",
        _ => "",
    }
}

pub fn nome_do_pvp(v: u8) -> &'static str {
    match v {
        auto_pvp::NUNCA => "Never attack players",
        auto_pvp::REVIDAR => "Only strike back at whoever hit me",
        auto_pvp::QUALQUER => "Attack any player",
        _ => "?",
    }
}

/// Sobe `i` uma posicao. Devolve se mudou.
///
/// Fora do desenho pra ser testavel: reordenar e' o miolo desta tela, e uma
/// troca errada na borda da lista e' o tipo de defeito que passa no olho.
pub fn sobe(ordem: &mut Vec<u8>, i: usize) -> bool {
    if i == 0 || i >= ordem.len() {
        return false;
    }
    ordem.swap(i - 1, i);
    true
}

/// Liga/desliga uma categoria. A lista nunca fica vazia: sem categoria
/// nenhuma o auto ficaria ligado sem escolher alvo, que da tela e' igual a
/// estar quebrado.
pub fn alterna(ordem: &mut Vec<u8>, cat: u8) -> bool {
    if !auto_alvo::valido(cat) {
        return false;
    }
    match ordem.iter().position(|v| *v == cat) {
        Some(i) => {
            if ordem.len() == 1 {
                return false;
            }
            ordem.remove(i);
        }
        None => ordem.push(cat),
    }
    true
}

impl ConfigCombate {
    pub fn abrir(&mut self) {
        self.aberto = true;
    }

    pub fn fechar(&mut self) {
        self.aberto = false;
    }

    /// Desenha e trata o clique. Devolve se a configuracao mudou.
    pub fn desenha(&mut self, ordem: &mut Vec<u8>, pvp: &mut u8) -> bool {
        let (sw, sh) = (screen_width(), screen_height());
        let r = Rect::new(sw * 0.5 - 210.0, sh * 0.5 - 200.0, 420.0, 400.0);
        estilo::painel(r);
        estilo::texto(r.x + 18.0, r.y + 32.0, "Combat", 20, estilo::OURO);
        let fechar = Rect::new(r.x + r.w - 38.0, r.y + 10.0, 28.0, 28.0);
        estilo::texto_centro(
            fechar.center().x,
            fechar.center().y + 7.0,
            "X",
            18,
            estilo::TEXTO,
        );
        let m = Vec2::from(mouse_position());
        let clicou = crate::foco::clique();
        if clicou && fechar.contains(m) {
            self.fechar();
            return false;
        }

        // A MISSÃO, dita e não ajustável.
        estilo::texto(
            r.x + 18.0,
            r.y + 60.0,
            "Missão de caça ativa vem sempre primeiro.",
            13,
            estilo::AUTO,
        );
        estilo::texto(
            r.x + 18.0,
            r.y + 88.0,
            "After that, in this order",
            14,
            estilo::SUAVE,
        );

        let mut mudou = false;
        // As três categorias: as escolhidas em ordem, depois as de fora.
        let todas = [
            auto_alvo::RANGED_EM_MIM,
            auto_alvo::MAIS_PERTO,
            auto_alvo::JOGADOR,
        ];
        let fora: Vec<u8> = todas
            .iter()
            .copied()
            .filter(|c| !ordem.contains(c))
            .collect();
        let n_dentro = ordem.len();
        for i in 0..n_dentro + fora.len() {
            let dentro = i < n_dentro;
            let cat = if dentro { ordem[i] } else { fora[i - n_dentro] };
            let linha = Rect::new(r.x + 18.0, r.y + 102.0 + i as f32 * 42.0, r.w - 36.0, 36.0);
            let caixa = Rect::new(linha.x, linha.y + 8.0, 20.0, 20.0);
            draw_rectangle_lines(caixa.x, caixa.y, caixa.w, caixa.h, 2.0, estilo::OURO);
            if dentro {
                draw_rectangle(
                    caixa.x + 4.0,
                    caixa.y + 4.0,
                    caixa.w - 8.0,
                    caixa.h - 8.0,
                    estilo::AUTO,
                );
            }
            let cor = if dentro { estilo::TEXTO } else { estilo::SUAVE };
            estilo::texto(
                linha.x + 32.0,
                linha.y + 17.0,
                &format!("{}. {}", i + 1, nome_da_categoria(cat)),
                15,
                cor,
            );
            estilo::texto(
                linha.x + 32.0,
                linha.y + 32.0,
                dica_da_categoria(cat),
                12,
                estilo::SUAVE,
            );
            // A SETA PRA CIMA só em quem está dentro e não é o primeiro.
            let seta = Rect::new(linha.x + linha.w - 36.0, linha.y + 4.0, 30.0, 28.0);
            if dentro && i > 0 {
                estilo::texto_centro(
                    seta.center().x,
                    seta.center().y + 6.0,
                    "^",
                    18,
                    estilo::OURO,
                );
                if clicou && seta.contains(m) && sobe(ordem, i) {
                    mudou = true;
                    continue;
                }
            }
            if clicou && linha.contains(m) && !seta.contains(m) && alterna(ordem, cat) {
                mudou = true;
            }
        }

        // PVP, separado: "quem eu prefiro atacar" e "eu aceito atacar gente"
        // são perguntas diferentes, e a segunda tem consequência.
        let y0 = r.y + 102.0 + 3.0 * 42.0 + 12.0;
        estilo::texto(r.x + 18.0, y0, "Against players", 14, estilo::SUAVE);
        for (k, v) in [auto_pvp::NUNCA, auto_pvp::REVIDAR, auto_pvp::QUALQUER]
            .into_iter()
            .enumerate()
        {
            let linha = Rect::new(r.x + 18.0, y0 + 10.0 + k as f32 * 30.0, r.w - 36.0, 26.0);
            // A selectable row, not a radio dot in front of the words (owner,
            // 30/09/2026: "take out the lil ball ... and everywhere else"):
            // the chosen one gets the burnished rim.
            let escolhido = *pvp == v;
            estilo::cartao(linha, linha.contains(m), escolhido);
            estilo::texto(
                linha.x + 10.0,
                linha.y + 18.0,
                nome_do_pvp(v),
                14,
                if escolhido { estilo::OURO } else { estilo::TEXTO },
            );
            if clicou && linha.contains(m) && *pvp != v {
                *pvp = v;
                mudou = true;
            }
        }
        mudou
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    /// A LISTA NUNCA FICA VAZIA.
    ///
    /// Sem categoria nenhuma o auto ficaria ligado sem escolher alvo — e auto
    /// combate que não ataca é, da tela, idêntico a estar quebrado. O último
    /// marcado não se desmarca, como no auto coleta.
    #[test]
    fn nao_da_pra_desmarcar_a_ultima() {
        let mut o = vec![auto_alvo::MAIS_PERTO];
        assert!(
            !alterna(&mut o, auto_alvo::MAIS_PERTO),
            "desmarcou a última"
        );
        assert_eq!(o, vec![auto_alvo::MAIS_PERTO]);
        // Com duas, desmarcar uma vale.
        let mut o = vec![auto_alvo::RANGED_EM_MIM, auto_alvo::MAIS_PERTO];
        assert!(alterna(&mut o, auto_alvo::RANGED_EM_MIM));
        assert_eq!(o, vec![auto_alvo::MAIS_PERTO]);
    }

    /// SUBIR TROCA COM O DE CIMA, e o primeiro não sobe.
    ///
    /// A borda é o que erra: um `swap` com `i - 1` em `i == 0` entra em
    /// pânico, e um teste de olho nunca clica ali.
    #[test]
    fn subir_respeita_a_borda() {
        let mut o = vec![auto_alvo::MAIS_PERTO, auto_alvo::RANGED_EM_MIM];
        assert!(!sobe(&mut o, 0), "o primeiro subiu");
        assert!(!sobe(&mut o, 9), "subiu um índice que não existe");
        assert!(sobe(&mut o, 1));
        assert_eq!(o, vec![auto_alvo::RANGED_EM_MIM, auto_alvo::MAIS_PERTO]);
    }

    /// Marcar algo que não existe não entra na lista — ela vai pro banco.
    #[test]
    fn categoria_inventada_nao_entra() {
        let mut o = vec![auto_alvo::MAIS_PERTO];
        assert!(!alterna(&mut o, 77));
        assert_eq!(o, vec![auto_alvo::MAIS_PERTO]);
    }

    /// Toda categoria e todo modo de PvP têm nome na tela.
    ///
    /// Um "?" na interface é um ajuste que ninguém entende — e o teste pega
    /// isso no dia em que alguém acrescentar uma categoria sem tocar aqui.
    #[test]
    fn tudo_tem_nome() {
        for c in [
            auto_alvo::RANGED_EM_MIM,
            auto_alvo::MAIS_PERTO,
            auto_alvo::JOGADOR,
        ] {
            assert_ne!(nome_da_categoria(c), "?", "categoria {c} sem nome");
            assert!(!dica_da_categoria(c).is_empty(), "categoria {c} sem dica");
        }
        for v in [auto_pvp::NUNCA, auto_pvp::REVIDAR, auto_pvp::QUALQUER] {
            assert_ne!(nome_do_pvp(v), "?", "modo de pvp {v} sem nome");
        }
    }
}
