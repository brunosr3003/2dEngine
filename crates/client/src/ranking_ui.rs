//! RANKINGS panel (`shared::ranking`): Power / Level, overall or per class
//! (the weapon set). Menu › Social › Rankings.

use macroquad::material::Material;
use macroquad::prelude::*;
use shared::protocol::ClientMessage;
use shared::ranking::{LinhaRanking, Ranking, TipoRanking};
use shared::skills::Conjunto;

#[allow(unused_imports)]
use crate::ui::Toque as _;
use crate::hud_estilo as estilo;
use crate::rolagem::Rolagem;

const LINHA: f32 = 40.0;

#[derive(Default)]
pub struct RankingUi {
    pub aberto: bool,
    tipo_poder: bool,
    /// `None` = all classes.
    classe: Option<u8>,
    dados: Option<Ranking>,
    rolagem: Rolagem,
    /// The player whose 3D preview is open (tapped in the list).
    sel: Option<LinhaRanking>,
}

fn u(v: f32) -> f32 {
    estilo::u(v)
}

fn nome_da_classe(c: u8) -> &'static str {
    Conjunto::de_u8(c).map_or("?", |c| c.nome())
}

impl RankingUi {
    fn tipo(&self) -> TipoRanking {
        if self.tipo_poder { TipoRanking::Poder } else { TipoRanking::Nivel }
    }

    fn pedido(&self) -> ClientMessage {
        ClientMessage::Ranking { tipo: self.tipo(), classe: self.classe }
    }

    pub fn abrir(&mut self) -> Vec<ClientMessage> {
        if !self.aberto {
            self.tipo_poder = true;
        }
        self.aberto = true;
        self.rolagem.zera();
        vec![self.pedido()]
    }

    pub fn fechar(&mut self) {
        self.aberto = false;
        self.sel = None;
    }

    pub fn receber(&mut self, r: Ranking) {
        // Only the page the player is looking at (a late answer for another
        // tab is dropped).
        if r.tipo == self.tipo() && r.classe == self.classe {
            self.dados = Some(r);
        }
    }

    pub fn desenha(&mut self, vox: &crate::vox::VoxCache, solido: &Material) -> Vec<ClientMessage> {
        if !self.aberto {
            return Vec::new();
        }
        estilo::no_painel(estilo::escala_do_painel(820.0, 680.0), || self.desenha_na_escala(vox, solido))
    }

    fn desenha_na_escala(&mut self, vox: &crate::vox::VoxCache, solido: &Material) -> Vec<ClientMessage> {
        let mut saida = Vec::new();
        let seguro = crate::hud_layout::tela_segura();
        let w = u(820.0).min(seguro.w - 16.0);
        let h = u(680.0).min(seguro.h - 16.0);
        let p = Rect::new(seguro.center().x - w * 0.5, seguro.center().y - h * 0.5, w, h);
        crate::hud_layout::escurece(0.45);
        estilo::painel(p);
        let m = Vec2::from(mouse_position());
        let clicou = crate::foco::clique();
        let x0 = p.x + u(20.0);
        estilo::texto_forte(x0, p.y + u(36.0), "Rankings", 20, estilo::OURO);
        let fechar = Rect::new(p.x + p.w - u(48.0), p.y + u(8.0), u(40.0), u(40.0));
        estilo::texto_centro(fechar.center().x, fechar.center().y + u(7.0), "X", 18, estilo::TEXTO);
        if clicou && fechar.toca(m) {
            self.fechar();
            return saida;
        }
        // Power | Level.
        let mut mudou = false;
        let aw = u(130.0);
        for (i, (rot, poder)) in [("Power", true), ("Level", false)].into_iter().enumerate() {
            let r = Rect::new(x0 + i as f32 * (aw + u(8.0)), p.y + u(52.0), aw, u(40.0));
            let sel = self.tipo_poder == poder;
            estilo::cartao(r, r.toca(m), sel);
            estilo::texto_centro(r.center().x, r.center().y + u(6.0), rot, 16, if sel { estilo::OURO } else { estilo::TEXTO });
            if clicou && r.toca(m) && !sel {
                self.tipo_poder = poder;
                mudou = true;
            }
        }
        // All | each class.
        let classes: Vec<(Option<u8>, &str)> = std::iter::once((None, "All"))
            .chain(Conjunto::TODOS.iter().map(|c| (Some(*c as u8), c.nome())))
            .collect();
        let cw = (p.w - u(40.0) - u(8.0) * (classes.len() as f32 - 1.0)) / classes.len() as f32;
        for (i, (c, rot)) in classes.iter().enumerate() {
            let r = Rect::new(x0 + i as f32 * (cw + u(8.0)), p.y + u(100.0), cw, u(36.0));
            let sel = self.classe == *c;
            estilo::cartao(r, r.toca(m), sel);
            estilo::texto_centro(r.center().x, r.center().y + u(5.0), rot, 13, if sel { estilo::OURO } else { estilo::TEXTO });
            if clicou && r.toca(m) && !sel {
                self.classe = *c;
                mudou = true;
            }
        }
        if mudou {
            self.dados = None;
            self.sel = None;
            self.rolagem.zera();
            saida.push(self.pedido());
        }
        // Header.
        let cab_y = p.y + u(162.0);
        let colunas = |r: Rect| (r.x + u(12.0), r.x + u(70.0), r.x + r.w * 0.55, r.x + r.w * 0.72, r.x + r.w - u(12.0));
        let lista = Rect::new(x0, cab_y + u(10.0), p.w - u(40.0), p.h - (cab_y - p.y) - u(70.0));
        let (c0, c1, c2, c3, c4) = colunas(lista);
        estilo::texto(c0, cab_y, "#", 13, estilo::SUAVE);
        estilo::texto(c1, cab_y, "Name", 13, estilo::SUAVE);
        estilo::texto(c2, cab_y, "Class", 13, estilo::SUAVE);
        estilo::texto(c3, cab_y, "Level", 13, estilo::SUAVE);
        let pw = estilo::medir("Power", 13);
        estilo::texto(c4 - pw, cab_y, "Power", 13, estilo::SUAVE);
        let Some(d) = &self.dados else {
            estilo::texto(x0, lista.y + u(30.0), "Loading…", 15, estilo::SUAVE);
            return saida;
        };
        if d.linhas.is_empty() {
            estilo::texto(x0, lista.y + u(30.0), "No one ranked here yet.", 15, estilo::SUAVE);
        }
        let linha_h = u(LINHA);
        let total = d.linhas.len() as f32 * linha_h;
        let toque = if self.sel.is_none() { self.rolagem.quadro(lista, total, linha_h) } else { None };
        let eu = d.eu.as_ref().map(|e| e.nome.clone());
        let desenha_linha = |r: Rect, l: &LinhaRanking, destaque: bool| {
            if destaque {
                draw_rectangle(r.x, r.y, r.w, r.h, Color::new(0.91, 0.75, 0.46, 0.16));
            } else if l.posicao % 2 == 0 {
                draw_rectangle(r.x, r.y, r.w, r.h, Color::new(1.0, 1.0, 1.0, 0.04));
            }
            let (c0, c1, c2, c3, c4) = colunas(r);
            let y = r.y + r.h * 0.5 + u(6.0);
            let cor_pos = match l.posicao {
                1 => Color::new(1.0, 0.84, 0.30, 1.0),
                2 => Color::new(0.80, 0.84, 0.90, 1.0),
                3 => Color::new(0.85, 0.55, 0.30, 1.0),
                _ => estilo::TEXTO,
            };
            estilo::texto_forte(c0, y, &l.posicao.to_string(), 15, cor_pos);
            estilo::texto(c1, y, &l.nome, 15, if destaque { estilo::OURO } else { estilo::TEXTO });
            estilo::texto(c2, y, nome_da_classe(l.classe), 13, estilo::SUAVE);
            estilo::texto(c3, y, &l.nivel.to_string(), 15, estilo::TEXTO);
            let pv = shared::idioma::milhar(l.poder.max(0) as u64);
            estilo::texto(c4 - estilo::medir(&pv, 15), y, &pv, 15, estilo::TEXTO);
        };
        crate::rolagem::recortar(Some(lista));
        for (i, l) in d.linhas.iter().enumerate() {
            let r = Rect::new(lista.x, lista.y - self.rolagem.pos + i as f32 * linha_h, lista.w - u(12.0), linha_h - u(2.0));
            if r.y + r.h < lista.y || r.y > lista.y + lista.h {
                continue;
            }
            desenha_linha(r, l, eu.as_deref() == Some(l.nome.as_str()));
            if toque.is_some_and(|p| r.contains(p)) {
                self.sel = Some(l.clone());
            }
        }
        crate::rolagem::recortar(None);
        self.rolagem.desenha(lista, total);
        // The tapped player, in 3D, over the right side of the list.
        if let Some(l) = self.sel.clone() {
            let card = Rect::new(lista.x + lista.w * 0.52, lista.y - u(6.0), lista.w * 0.48, lista.h + u(6.0));
            estilo::painel_destaque(card, estilo::OURO);
            let fechar = Rect::new(card.x + card.w - u(44.0), card.y + u(6.0), u(38.0), u(38.0));
            estilo::texto_centro(fechar.center().x, fechar.center().y + u(7.0), "X", 18, estilo::TEXTO);
            let boneco = Rect::new(card.x + u(12.0), card.y + u(48.0), card.w - u(24.0), card.h - u(150.0));
            crate::bolsa::desenha_boneco(boneco, vox, solido, l.aparencia, l.skins, l.auras, l.classe);
            let cx = card.center().x;
            estilo::texto_centro_forte(cx, card.y + u(34.0), &format!("#{}  {}", l.posicao, l.nome), 19, estilo::OURO);
            let y = boneco.y + boneco.h + u(30.0);
            estilo::texto_centro(cx, y, nome_da_classe(l.classe), 15, estilo::SUAVE);
            estilo::texto_centro_forte(
                cx,
                y + u(30.0),
                &format!("Level {}  ·  Power {}", l.nivel, shared::idioma::milhar(l.poder.max(0) as u64)),
                17,
                estilo::TEXTO,
            );
            if clicou && (fechar.toca(m) || !card.contains(m)) {
                self.sel = None;
            }
        }
        // The player's own place, always visible at the bottom.
        let rodape = Rect::new(lista.x, p.y + p.h - u(54.0), lista.w - u(12.0), u(40.0));
        match &d.eu {
            Some(e) => desenha_linha(rodape, e, true),
            None => estilo::texto(x0, rodape.y + u(26.0), "You are not in this ranking.", 14, estilo::SUAVE),
        }
        saida
    }
}
