//! O painel do PORTO e o de ATRACAR: uma linha por ilha, com o botao certo
//! pro lugar onde o jogador esta'. Quem decide tudo e' o servidor
//! (`shared::viagem`); aqui so' se desenha a lista que ele mandou.
//!
//! Desde 21/09/2026 o Capitao nao teleporta mais (docs/MAR_ABERTO.md): no
//! cais o botao e' **Zarpar**, e no mar e' **Atracar**. A lista e a mesma nos
//! dois — os estados `FORA_DO_AR` e `BLOQUEADA` valem igual, e e' por isso
//! que a mesma funcao serve os dois: quem esta' no mar precisa saber que um
//! porto caiu ANTES de navegar ate' ele.

use macroquad::prelude::*;
use shared::viagem::{estado, Destino};

use crate::hud_estilo as estilo;

#[derive(Debug, Default)]
pub struct ViagemUi {
    destinos: Option<Vec<Destino>>,
    /// No mar o painel vira "Atracar". No cais, "Zarpar".
    pub no_mar: bool,
}

impl ViagemUi {
    pub fn abrir(&mut self, destinos: Vec<Destino>, no_mar: bool) {
        self.destinos = Some(destinos);
        self.no_mar = no_mar;
    }

    pub fn fechar(&mut self) {
        self.destinos = None;
    }

    pub fn aberto(&self) -> bool {
        self.destinos.is_some()
    }

    /// Desenha; devolve a ilha escolhida pra embarcar (o menu fecha junto).
    pub fn desenha(&mut self) -> Option<u8> {
        estilo::no_painel(estilo::escala_do_painel(560.0, 420.0), || self.desenha_na_escala())
    }

    fn desenha_na_escala(&mut self) -> Option<u8> {
        let destinos = self.destinos.as_ref()?;
        let f = estilo::fator_texto();
        let seguro = crate::hud_layout::tela_segura();
        let linha_h = 74.0 * f;
        let w = (560.0 * f).min(seguro.w - 16.0);
        let h = (96.0 * f + linha_h * destinos.len() as f32 + 16.0 * f).min(seguro.h - 16.0);
        let p = Rect::new(
            seguro.center().x - w * 0.5,
            seguro.center().y - h * 0.5,
            w,
            h,
        );
        crate::hud_layout::escurece(0.45);
        estilo::painel_destaque(p, estilo::ACENTO);
        let m = Vec2::from(mouse_position());
        let clicou = crate::foco::clique();
        let x0 = p.x + 20.0 * f;
        let (titulo, explica) = if self.no_mar {
            ("Atracar", "Chegue perto do cais da ilha para atracar.")
        } else {
            ("Porto", "Daqui se zarpa. A travessia é navegada.")
        };
        estilo::texto_forte(x0, p.y + 36.0 * f, titulo, 20, estilo::OURO);
        estilo::texto(x0, p.y + 62.0 * f, explica, 14, estilo::SUAVE);
        let fechar = Rect::new(p.x + p.w - 48.0 * f, p.y + 8.0 * f, 40.0 * f, 40.0 * f);
        estilo::texto_centro(
            fechar.center().x,
            fechar.center().y + 7.0 * f,
            "X",
            18,
            estilo::TEXTO,
        );
        let mut escolha = None;
        let mut y = p.y + 80.0 * f;
        for d in destinos {
            let r = Rect::new(x0, y, p.w - 40.0 * f, linha_h - 8.0 * f);
            estilo::cartao(r, false, d.estado == estado::AQUI);
            let nome_cor = if d.estado == estado::BLOQUEADA {
                estilo::SUAVE
            } else {
                estilo::TEXTO
            };
            estilo::texto_forte(r.x + 14.0 * f, r.y + 26.0 * f, &d.nome, 17, nome_cor);
            estilo::texto(
                r.x + 14.0 * f,
                r.y + 50.0 * f,
                &subtitulo(d),
                13,
                estilo::SUAVE,
            );
            // No cais o unico botao e' o da PROPRIA ilha, porque nao se
            // escolhe destino ao zarpar — se escolhe ao chegar.
            let mostra = if self.no_mar {
                d.estado == estado::LIBERADA
            } else {
                d.estado == estado::AQUI
            };
            if mostra {
                let b = Rect::new(
                    r.x + r.w - 136.0 * f,
                    r.y + (r.h - 40.0 * f) * 0.5,
                    124.0 * f,
                    40.0 * f,
                );
                let rotulo = if self.no_mar { "Atracar" } else { "Zarpar" };
                estilo::botao(b, rotulo, estilo::estado_de(b, false, false), true);
                if clicou && b.contains(m) {
                    escolha = Some(d.ilha);
                }
            }
            y += linha_h;
        }
        if escolha.is_some() || (clicou && (fechar.contains(m) || !p.contains(m))) {
            self.fechar();
        }
        escolha
    }
}

/// A segunda linha de cada ilha: nivel e o que falta pra ir.
pub fn subtitulo(d: &Destino) -> String {
    let nivel = format!("Nível {}–{}", d.nivel_min, d.nivel_max);
    match d.estado {
        estado::AQUI => format!("{nivel} · você está aqui"),
        estado::LIBERADA => nivel,
        estado::FORA_DO_AR => format!("{nivel} · porto fora do ar"),
        _ if d.requisito.is_empty() => format!("{nivel} · bloqueada"),
        _ => format!("{nivel} · libera em \"{}\"", d.requisito),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(e: u8) -> Destino {
        Destino {
            ilha: 1,
            nome: "Geleira".into(),
            nivel_min: 15,
            nivel_max: 30,
            estado: e,
            requisito: "Rumo à Geleira".into(),
        }
    }

    #[test]
    fn subtitulo_diz_o_que_falta() {
        assert_eq!(subtitulo(&d(estado::LIBERADA)), "Nível 15–30");
        assert!(subtitulo(&d(estado::AQUI)).ends_with("você está aqui"));
        assert!(subtitulo(&d(estado::BLOQUEADA)).contains("Rumo à Geleira"));
        assert!(subtitulo(&d(estado::FORA_DO_AR)).contains("fora do ar"));
    }

    /// No cais o botao e' o da PROPRIA ilha (zarpar); no mar, o das outras
    /// (atracar). Sao os dois lados da mesma lista, e trocar um pelo outro
    /// deixaria o jogador sem saida de um dos dois lugares.
    #[test]
    fn o_botao_muda_de_lado_conforme_onde_se_esta() {
        let mut ui = ViagemUi::default();
        ui.abrir(vec![d(estado::AQUI)], false);
        assert!(!ui.no_mar);
        ui.abrir(vec![d(estado::LIBERADA)], true);
        assert!(ui.no_mar);
    }
}
