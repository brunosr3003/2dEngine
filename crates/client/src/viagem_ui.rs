//! Menu "Viajar" do Capitao do Porto: uma linha por ilha, com "Embarcar" nas
//! que a historia ja' liberou. Quem decide tudo e' o servidor
//! (`shared::viagem`); aqui so' se desenha a lista que ele mandou.

use macroquad::prelude::*;
use shared::viagem::{estado, Destino};

use crate::hud_estilo as estilo;

/// O que o jogador escolheu no menu do Capitao.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Escolha {
    /// Uma ilha do arquipelago.
    Ilha(u8),
    /// A colonia (docs/COLONIA.md). Nao e' uma ilha do `ARQUIPELAGO` e nao
    /// tem indice: e' por isso que a escolha e' um enum e nao um `u8`.
    MinhaIlha,
}

#[derive(Debug, Default)]
pub struct ViagemUi {
    destinos: Option<Vec<Destino>>,
}

impl ViagemUi {
    pub fn abrir(&mut self, destinos: Vec<Destino>) {
        self.destinos = Some(destinos);
    }

    pub fn fechar(&mut self) {
        self.destinos = None;
    }

    pub fn aberto(&self) -> bool {
        self.destinos.is_some()
    }

    /// Desenha; devolve o destino escolhido (o menu fecha junto).
    ///
    /// `colonia` e' se a historia ja' entregou a ilha propria: sem isso a
    /// linha nao aparece, porque o menu nao pode oferecer o que o servidor
    /// vai recusar.
    pub fn desenha(&mut self, colonia: bool) -> Option<Escolha> {
        estilo::no_painel(estilo::escala_do_painel(560.0, 420.0), || {
            self.desenha_na_escala(colonia)
        })
    }

    fn desenha_na_escala(&mut self, colonia: bool) -> Option<Escolha> {
        let destinos = self.destinos.as_ref()?;
        let f = estilo::fator_texto();
        let seguro = crate::hud_layout::tela_segura();
        let linha_h = 74.0 * f;
        let w = (560.0 * f).min(seguro.w - 16.0);
        let linhas = destinos.len() + usize::from(colonia);
        let h = (96.0 * f + linha_h * linhas as f32 + 16.0 * f).min(seguro.h - 16.0);
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
        estilo::texto_forte(x0, p.y + 36.0 * f, "Viajar", 20, estilo::OURO);
        estilo::texto(
            x0,
            p.y + 62.0 * f,
            "O Capitão leva a qualquer ilha que a história já abriu.",
            14,
            estilo::SUAVE,
        );
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
            if d.estado == estado::LIBERADA {
                let b = Rect::new(
                    r.x + r.w - 136.0 * f,
                    r.y + (r.h - 40.0 * f) * 0.5,
                    124.0 * f,
                    40.0 * f,
                );
                estilo::botao(b, "Embarcar", estilo::estado_de(b, false, false), true);
                if clicou && b.contains(m) {
                    escolha = Some(Escolha::Ilha(d.ilha));
                }
            }
            y += linha_h;
        }
        // A ilha propria fecha a lista: e' o destino que ninguem disputa, e
        // ela nao entra no meio das ilhas porque nao e' uma delas.
        if colonia {
            let r = Rect::new(x0, y, p.w - 40.0 * f, linha_h - 8.0 * f);
            estilo::cartao(r, false, false);
            estilo::texto_forte(
                r.x + 14.0 * f,
                r.y + 26.0 * f,
                "Minha Ilha",
                17,
                estilo::OURO,
            );
            estilo::texto(
                r.x + 14.0 * f,
                r.y + 50.0 * f,
                "Sua colônia · rende sozinha enquanto você navega",
                13,
                estilo::SUAVE,
            );
            let b = Rect::new(
                r.x + r.w - 136.0 * f,
                r.y + (r.h - 40.0 * f) * 0.5,
                124.0 * f,
                40.0 * f,
            );
            estilo::botao(b, "Ir", estilo::estado_de(b, false, false), true);
            if clicou && b.contains(m) {
                escolha = Some(Escolha::MinhaIlha);
            }
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
        estado::FORA_DO_AR => format!("{nivel} · sem barco agora (servidor fora do ar)"),
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
}
