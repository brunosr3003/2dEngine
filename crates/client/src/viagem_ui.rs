//! The ISLAND PORTAL's "Travel" menu (`portal_ilhas_ui`, 17/11/2026): one
//! row per island, with "Board" on the ones the story has unlocked. It was
//! the Harbour Captain's until then; the dock NPCs now open the Shipyard
//! (`estaleiro_ui`). The server decides everything (`shared::viagem`); here
//! we only draw the list it sent.

#[allow(unused_imports)]
use crate::ui::Toque as _;
use macroquad::prelude::*;
use shared::viagem::{estado, Destino};

use crate::hud_estilo as estilo;

/// What the player chose in the travel menu.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Escolha {
    /// One island of the archipelago.
    Ilha(u8),
    /// The colony (docs/COLONIA.md). It is not an island of the `ARQUIPELAGO`
    /// and has no index: that is why the choice is an enum and not a `u8`.
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
    /// `colonia` is whether the story has already handed over the player's own
    /// island: without it the row does not appear, because the menu cannot offer
    /// what the server is going to refuse.
    pub fn desenha(&mut self, colonia: bool) -> Option<Escolha> {
        estilo::no_painel(estilo::escala_do_painel(560.0, 420.0), || {
            self.desenha_na_escala(colonia)
        })
    }

    fn desenha_na_escala(&mut self, colonia: bool) -> Option<Escolha> {
        let destinos = self.destinos.as_ref()?;
        let f = estilo::fator_texto();
        let seguro = crate::hud_layout::tela_segura();
        let w = (560.0 * f).min(seguro.w - 16.0);
        let linhas = destinos.len() + usize::from(colonia);
        // Every island fits (seven since the portal): rows shrink to fit.
        let linha_h = (74.0 * f)
            .min((seguro.h - 16.0 - 112.0 * f) / linhas.max(1) as f32)
            .max(50.0 * f);
        let h = (96.0 * f + linha_h * linhas as f32 + 16.0 * f).min(seguro.h - 16.0);
        let p = Rect::new(
            seguro.center().x - w * 0.5,
            seguro.center().y - h * 0.5,
            w,
            h,
        );
        crate::hud_layout::escurece(0.45);
        estilo::painel_destaque(p, crate::portal_ilhas_ui::AZUL);
        let m = Vec2::from(mouse_position());
        let clicou = crate::foco::clique();
        let x0 = p.x + 20.0 * f;
        estilo::texto_forte(
            x0,
            p.y + 36.0 * f,
            crate::portal_ilhas_ui::NOME,
            20,
            crate::portal_ilhas_ui::OURO,
        );
        estilo::texto(
            x0,
            p.y + 62.0 * f,
            "Travel to any island the story has opened, for copper.",
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
            let meio = r.y + r.h * 0.5;
            estilo::texto_forte(r.x + 14.0 * f, meio - 3.0 * f, &d.nome, 17, nome_cor);
            estilo::texto(
                r.x + 14.0 * f,
                meio + 17.0 * f,
                &subtitulo(d),
                13,
                estilo::SUAVE,
            );
            if d.estado == estado::LIBERADA {
                // The price is ON the button (owner, 15/11/2026).
                let rotulo = rotulo_de_embarque(d);
                let bw = (estilo::medir_forte(&rotulo, 15) + 28.0 * f).max(124.0 * f);
                let bh = (40.0 * f).min(r.h - 8.0 * f);
                let b = Rect::new(r.x + r.w - bw - 12.0 * f, r.y + (r.h - bh) * 0.5, bw, bh);
                estilo::botao(b, &rotulo, estilo::estado_de(b, false, false), true);
                if clicou && b.toca(m) {
                    escolha = Some(Escolha::Ilha(d.ilha));
                }
            }
            y += linha_h;
        }
        // The player's own island closes the list: it is the destination nobody
        // competes for, and it does not sit among the islands because it is not one.
        if colonia {
            let r = Rect::new(x0, y, p.w - 40.0 * f, linha_h - 8.0 * f);
            estilo::cartao(r, false, false);
            estilo::texto_forte(
                r.x + 14.0 * f,
                r.y + 26.0 * f,
                "My Island",
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
            if clicou && b.toca(m) {
                escolha = Some(Escolha::MinhaIlha);
            }
        }
        if escolha.is_some() || (clicou && (fechar.toca(m) || !p.toca(m))) {
            self.fechar();
        }
        escolha
    }
}

/// The ship's numbers in one line: hull, speed, how it turns (turning
/// inertia: Agile, Steady, Heavy), cannon.
pub fn linha_do_navio(id: u16, d: &shared::items::NavioData) -> String {
    use shared::navio as nv;
    format!(
        "Hull {} / {} · Speed {:.1} · Turning {} · Cannon ×{:.2}",
        d.casco,
        nv::casco_total(id, d),
        nv::velocidade_do_navio(id, d),
        nv::rotulo_do_giro(id, d),
        nv::mult_do_canhao(id, d)
    )
}

/// The "Board" button, with the trip's copper price when it has one.
pub fn rotulo_de_embarque(d: &Destino) -> String {
    if d.preco > 0 {
        format!("Board · {} copper", d.preco)
    } else {
        "Board".to_string()
    }
}

/// Each island's second line: level and what is missing to go.
pub fn subtitulo(d: &Destino) -> String {
    let nivel = format!(
        "Level {}",
        shared::progressao::texto_de_faixa(d.nivel_min as u32, d.nivel_max as u32)
    );
    match d.estado {
        estado::AQUI => format!("{nivel} · you are here"),
        estado::LIBERADA => nivel,
        estado::FORA_DO_AR => format!("{nivel} · sem barco agora (servidor fora do ar)"),
        _ if d.requisito.is_empty() => format!("{nivel} · locked"),
        _ => format!("{nivel} · unlocks at \"{}\"", d.requisito),
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
            requisito: "Bound for the Glacier".into(),
            preco: 100,
        }
    }

    #[test]
    fn a_linha_do_navio_conta_as_estrelas() {
        let id = shared::item_id::NAVIO_CHALUPA;
        let mut d = shared::navio::novo(id);
        assert_eq!(
            linha_do_navio(id, &d),
            "Hull 2400 / 2400 · Speed 10.0 · Turning Agile · Cannon ×1.00"
        );
        d.estrelas = [5, 5, 5];
        assert_eq!(
            linha_do_navio(id, &d),
            "Hull 2400 / 3360 · Speed 11.5 · Turning Agile · Cannon ×1.40"
        );
        let nau = shared::item_id::NAVIO_NAU;
        assert!(linha_do_navio(nau, &shared::navio::novo(nau)).contains("Turning Heavy"));
    }

    #[test]
    fn o_botao_mostra_o_preco() {
        assert_eq!(
            rotulo_de_embarque(&d(estado::LIBERADA)),
            "Board · 100 copper"
        );
        let gratis = Destino {
            preco: 0,
            ..d(estado::LIBERADA)
        };
        assert_eq!(rotulo_de_embarque(&gratis), "Board");
    }

    #[test]
    fn subtitulo_diz_o_que_falta() {
        assert_eq!(subtitulo(&d(estado::LIBERADA)), "Level 30–50"); // internal 15–30, shown
        assert!(subtitulo(&d(estado::AQUI)).ends_with("you are here"));
        assert!(subtitulo(&d(estado::BLOQUEADA)).contains("Bound for the Glacier"));
        assert!(subtitulo(&d(estado::FORA_DO_AR)).contains("fora do ar"));
    }
}
