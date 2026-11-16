//! The Harbour Captain's "Travel" menu: one row per island, with "Board" on
//! the ones the story has unlocked. The server decides everything
//! (`shared::viagem`); here we only draw the list it sent.

#[allow(unused_imports)]
use crate::ui::Toque as _;
use macroquad::prelude::*;
use shared::viagem::{estado, Destino};

use crate::hud_estilo as estilo;

/// What the player chose in the Captain's menu.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Escolha {
    /// One island of the archipelago.
    Ilha(u8),
    /// The colony (docs/COLONIA.md). It is not an island of the `ARQUIPELAGO`
    /// and has no index: that is why the choice is an enum and not a `u8`.
    MinhaIlha,
    /// Sail out to the open sea with my own ship (docs/NAVEGACAO.md).
    Zarpar,
    /// Repair my ship's hull, for copper.
    Reparar,
    /// Shipwright: build the next hull class (naval phase 2).
    Construir,
    /// Shipwright: one star on axis `navio::Eixo` as u8.
    Melhorar(u8),
}

/// What the Shipwright section reads from the client: the equipped ship,
/// how many of an item the bag holds, item names and the level.
pub struct Contexto<'a> {
    pub equip: &'a shared::Equipment,
    pub conta: &'a dyn Fn(u16) -> u64,
    pub nome: &'a dyn Fn(u16) -> String,
    pub nivel: u32,
}

#[derive(Debug, Default)]
pub struct ViagemUi {
    destinos: Option<Vec<Destino>>,
    /// The ship's state (`ServerMessage::Navio`), sent with the Harbour
    /// Captain's menu. `None` for the bus and the submarine.
    navio: Option<shared::navio::EstadoNavio>,
    /// The SHIPWRIGHT section is open instead of the destinations.
    estaleiro: bool,
}

impl ViagemUi {
    pub fn abrir(&mut self, destinos: Vec<Destino>) {
        self.destinos = Some(destinos);
    }

    pub fn fechar(&mut self) {
        self.destinos = None;
        self.estaleiro = false;
    }

    pub fn define_navio(&mut self, estado: shared::navio::EstadoNavio) {
        self.navio = Some(estado);
    }

    pub fn limpar_navio(&mut self) {
        self.navio = None;
    }

    pub fn aberto(&self) -> bool {
        self.destinos.is_some()
    }

    /// Desenha; devolve o destino escolhido (o menu fecha junto).
    ///
    /// `colonia` is whether the story has already handed over the player's own
    /// island: without it the row does not appear, because the menu cannot offer
    /// what the server is going to refuse.
    pub fn desenha(&mut self, colonia: bool, ctx: &Contexto) -> Option<Escolha> {
        if self.estaleiro && self.destinos.is_some() {
            return estilo::no_painel(estilo::escala_do_painel(600.0, 560.0), || self.desenha_estaleiro(ctx));
        }
        estilo::no_painel(estilo::escala_do_painel(560.0, 420.0), || {
            self.desenha_na_escala(colonia)
        })
    }

    fn desenha_na_escala(&mut self, colonia: bool) -> Option<Escolha> {
        let destinos = self.destinos.as_ref()?;
        let navio = self.navio.filter(|n| n.zarpar);
        let f = estilo::fator_texto();
        let seguro = crate::hud_layout::tela_segura();
        let linha_h = 74.0 * f;
        let w = (560.0 * f).min(seguro.w - 16.0);
        let linhas = destinos.len() + usize::from(colonia) + usize::from(navio.is_some());
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
        estilo::texto_forte(x0, p.y + 36.0 * f, "Travel", 20, estilo::OURO);
        estilo::texto(
            x0,
            p.y + 62.0 * f,
            "The Captain sails to any island the story has opened, for copper.",
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
        // MY SHIP first (docs/NAVEGACAO.md): sail the open sea from this
        // dock, or repair the hull.
        if let Some(n) = navio {
            let r = Rect::new(x0, y, p.w - 40.0 * f, linha_h - 8.0 * f);
            estilo::cartao(r, false, true);
            let (titulo, linha) = if !n.tem {
                ("My ship".to_string(), "No ship: the story gives you one when you first sail to the Glacier.".to_string())
            } else if n.avariado {
                ("My ship · damaged".to_string(), format!("Damaged hull {} / {}: repair it to sail", n.casco, n.casco_max))
            } else {
                ("My ship".to_string(), format!("Hull {} / {}", n.casco, n.casco_max))
            };
            estilo::texto_forte(r.x + 14.0 * f, r.y + 26.0 * f, &titulo, 17, estilo::OURO);
            estilo::texto(r.x + 14.0 * f, r.y + 50.0 * f, &linha, 13, estilo::SUAVE);
            let bw = 124.0 * f;
            let b = Rect::new(r.x + r.w - bw - 12.0 * f, r.y + (r.h - 40.0 * f) * 0.5, bw, 40.0 * f);
            if n.tem && !n.avariado {
                estilo::botao(b, "Sail", estilo::estado_de(b, false, false), true);
                if clicou && b.toca(m) {
                    escolha = Some(Escolha::Zarpar);
                }
            }
            let mut esquerda = b.x;
            if n.tem && n.reparo > 0 {
                let rotulo = format!("Repair · {} copper", n.reparo);
                let rw = estilo::medir_forte(&rotulo, 15) + 28.0 * f;
                let rb = Rect::new(b.x - rw - 8.0 * f, b.y, rw, b.h);
                estilo::botao(rb, &rotulo, estilo::estado_de(rb, false, false), n.avariado);
                if clicou && rb.toca(m) {
                    escolha = Some(Escolha::Reparar);
                }
                esquerda = rb.x;
            }
            // THE SHIPWRIGHT (naval phase 2): hulls and stars.
            if n.tem {
                let rotulo = "Shipwright";
                let ew = estilo::medir_forte(rotulo, 15) + 28.0 * f;
                let eb = Rect::new(esquerda - ew - 8.0 * f, b.y, ew, b.h);
                estilo::botao(eb, rotulo, estilo::estado_de(eb, false, false), false);
                if clicou && eb.toca(m) {
                    self.estaleiro = true;
                    return None;
                }
            }
            y += linha_h;
        }
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
                // The price is ON the button (owner, 15/11/2026).
                let rotulo = rotulo_de_embarque(d);
                let bw = (estilo::medir_forte(&rotulo, 15) + 28.0 * f).max(124.0 * f);
                let b = Rect::new(
                    r.x + r.w - bw - 12.0 * f,
                    r.y + (r.h - 40.0 * f) * 0.5,
                    bw,
                    40.0 * f,
                );
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

    /// THE SHIPWRIGHT (docs/NAVEGACAO.md, naval phase 2): the equipped
    /// ship's class and numbers, a star row per axis (copper, Darksteel and
    /// Wreck Planks; always succeeds) and the next hull with its recipe.
    /// The panel stays open after a purchase, so several stars go in a row.
    fn desenha_estaleiro(&mut self, ctx: &Contexto) -> Option<Escolha> {
        use shared::navio as nv;
        let f = estilo::fator_texto();
        let seguro = crate::hud_layout::tela_segura();
        let w = (600.0 * f).min(seguro.w - 16.0);
        let h = (560.0 * f).min(seguro.h - 16.0);
        let p = Rect::new(seguro.center().x - w * 0.5, seguro.center().y - h * 0.5, w, h);
        crate::hud_layout::escurece(0.45);
        estilo::painel_destaque(p, estilo::ACENTO);
        let m = Vec2::from(mouse_position());
        let clicou = crate::foco::clique();
        let x0 = p.x + 20.0 * f;
        estilo::texto_forte(x0, p.y + 36.0 * f, "Shipwright", 20, estilo::OURO);
        estilo::texto(x0, p.y + 60.0 * f, "Hulls and ship stars. Stars always succeed and stay with the ship.", 13, estilo::SUAVE);
        let voltar = Rect::new(p.x + p.w - 120.0 * f, p.y + 12.0 * f, 104.0 * f, 36.0 * f);
        estilo::botao(voltar, "Back", estilo::estado_de(voltar, false, false), false);
        if clicou && voltar.toca(m) {
            self.estaleiro = false;
            return None;
        }
        let Some(id) = ctx.equip.navio.filter(|i| shared::item_id::e_navio(*i)) else {
            estilo::texto(x0, p.y + 100.0 * f, "No ship in the Ship slot.", 15, estilo::TEXTO);
            if clicou && !p.toca(m) {
                self.fechar();
            }
            return None;
        };
        let d = nv::dados(ctx.equip.navio_inst.as_ref(), id);
        let mut escolha = None;
        // The ship.
        let mut y = p.y + 78.0 * f;
        let r = Rect::new(x0, y, p.w - 40.0 * f, 64.0 * f);
        estilo::cartao(r, false, true);
        estilo::texto_forte(r.x + 14.0 * f, r.y + 26.0 * f, &(ctx.nome)(id), 17, estilo::OURO);
        estilo::texto(
            r.x + 14.0 * f,
            r.y + 50.0 * f,
            &linha_do_navio(id, &d),
            13,
            estilo::SUAVE,
        );
        y += 72.0 * f;
        // A row per axis.
        for eixo in nv::Eixo::TODOS {
            let r = Rect::new(x0, y, p.w - 40.0 * f, 70.0 * f);
            estilo::cartao(r, false, false);
            let n = nv::estrelas(&d, eixo);
            estilo::texto_forte(r.x + 14.0 * f, r.y + 26.0 * f, eixo.nome(), 16, estilo::TEXTO);
            crate::bolsa::desenha_estrelas(r.x + 90.0 * f, r.y + 20.0 * f, 7.0 * f, n, nv::ESTRELAS_MAX);
            let ganho = format!("+{:.0}%", n as f32 * eixo.bonus_por_estrela() * 100.0);
            estilo::texto(r.x + 200.0 * f, r.y + 26.0 * f, &ganho, 14, estilo::VERDE);
            match nv::custo_da_estrela(n) {
                Some(c) => {
                    let falta = |item: u16, q: u32| (ctx.conta)(item) < q as u64;
                    let pode = !(falta(shared::item_id::COPPER, c.cobre)
                        || falta(shared::item_id::DARKSTEEL, c.darksteel)
                        || falta(shared::item_id::PRANCHA_DE_NAUFRAGIO, c.pranchas));
                    estilo::texto(
                        r.x + 14.0 * f,
                        r.y + 52.0 * f,
                        &format!("Next star: {} copper · {} Darksteel · {} Wreck Planks", c.cobre, c.darksteel, c.pranchas),
                        13,
                        if pode { estilo::SUAVE } else { estilo::VERMELHO },
                    );
                    let bw = 118.0 * f;
                    let b = Rect::new(r.x + r.w - bw - 12.0 * f, r.y + (r.h - 38.0 * f) * 0.5, bw, 38.0 * f);
                    estilo::botao(b, "Add star", estilo::estado_de(b, false, false), pode);
                    if clicou && b.toca(m) {
                        escolha = Some(Escolha::Melhorar(eixo as u8));
                    }
                }
                None => estilo::texto(r.x + 14.0 * f, r.y + 52.0 * f, "Five stars: the top.", 13, estilo::OURO),
            }
            y += 78.0 * f;
        }
        // The next hull.
        let r = Rect::new(x0, y, p.w - 40.0 * f, (p.y + p.h - 16.0 * f - y).max(60.0 * f));
        estilo::cartao(r, false, false);
        match nv::receita_depois_de(id) {
            None => {
                estilo::texto_forte(r.x + 14.0 * f, r.y + 26.0 * f, "The finest hull there is.", 16, estilo::OURO);
            }
            Some(rec) => {
                let classe = nv::casco(rec.para).expect("every recipe builds a hull");
                estilo::texto_forte(
                    r.x + 14.0 * f,
                    r.y + 26.0 * f,
                    &format!("Next hull: {}", (ctx.nome)(rec.para)),
                    16,
                    estilo::TEXTO,
                );
                let nivel_ok = ctx.nivel >= classe.nivel;
                estilo::texto(
                    r.x + 14.0 * f,
                    r.y + 48.0 * f,
                    &format!(
                        "Level {} · Hull {} · Speed {:.1} · the old hull is consumed, stars kept",
                        shared::progressao::texto_de_nivel(classe.nivel),
                        classe.casco,
                        classe.velocidade,
                    ),
                    13,
                    if nivel_ok { estilo::SUAVE } else { estilo::VERMELHO },
                );
                let mut tudo = nivel_ok;
                for (k, (item, q)) in rec.materiais.iter().enumerate() {
                    let tem = (ctx.conta)(*item);
                    tudo &= tem >= *q as u64;
                    let lx = r.x + 14.0 * f + (k % 2) as f32 * (r.w * 0.5 - 30.0 * f);
                    let ly = r.y + 72.0 * f + (k / 2) as f32 * 20.0 * f;
                    // Name and count apart: the name is translated whole.
                    let cor = if tem >= *q as u64 { estilo::VERDE } else { estilo::VERMELHO };
                    let nome = (ctx.nome)(*item);
                    estilo::texto(lx, ly, &nome, 13, cor);
                    let nw = estilo::medir(&nome, 13);
                    estilo::texto(lx + nw + 6.0 * f, ly, &format!("{}/{}", tem.min(*q as u64), q), 13, cor);
                }
                let bw = 118.0 * f;
                let b = Rect::new(r.x + r.w - bw - 12.0 * f, r.y + 12.0 * f, bw, 38.0 * f);
                estilo::botao(b, "Build", estilo::estado_de(b, false, false), tudo);
                if clicou && b.toca(m) {
                    escolha = Some(Escolha::Construir);
                }
            }
        }
        if escolha.is_none() && clicou && !p.toca(m) {
            self.fechar();
        }
        escolha
    }
}

/// The ship's numbers in one line: hull, speed, cannon.
pub fn linha_do_navio(id: u16, d: &shared::items::NavioData) -> String {
    use shared::navio as nv;
    format!(
        "Hull {} / {} · Speed {:.1} · Cannon ×{:.2}",
        d.casco,
        nv::casco_total(id, d),
        nv::velocidade_do_navio(id, d),
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
    let nivel = format!("Level {}", shared::progressao::texto_de_faixa(d.nivel_min as u32, d.nivel_max as u32));
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
        assert_eq!(linha_do_navio(id, &d), "Hull 2400 / 2400 · Speed 10.0 · Cannon ×1.00");
        d.estrelas = [5, 5, 5];
        assert_eq!(linha_do_navio(id, &d), "Hull 2400 / 3360 · Speed 11.5 · Cannon ×1.40");
    }

    #[test]
    fn o_botao_mostra_o_preco() {
        assert_eq!(rotulo_de_embarque(&d(estado::LIBERADA)), "Board · 100 copper");
        let gratis = Destino { preco: 0, ..d(estado::LIBERADA) };
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
