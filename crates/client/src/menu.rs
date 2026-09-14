//! O Menu Principal (≡), no molde do MIR4 (docs/HUD.md, 3).
//!
//! So' abre pelo botao MENU do HUD — nenhuma tecla abre (decisao do usuario).
//! O X, clicar de novo no ≡ ou Esc fecham. A coluna da esquerda tem o retrato
//! em texto (nome, nivel, Poder, arma) e os SALDOS, que no MIR4 moram aqui e
//! nao no HUD do mundo. A direita, os sistemas por grupo; os que ainda nao
//! existem mostram cadeado e o motivo no hover.
use macroquad::prelude::*;

use crate::hud::{pictograma, selo};
use crate::hud_estilo as estilo;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Item {
    Bolsa,
    Ficha,
    Habilidades,
    Montaria,
    Missoes,
    TodasMissoes,
    Conquistas,
    Craft,
    Forja,
    Encantar,
    Mapa,
    Aventuras,
    Grupo,
    Amigos,
    Correio,
    Clan,
    Lojas,
    Mercado,
    LojaTp,
    Configuracoes,
    TrocarPersonagem,
    Sair,
}

/// (item, rotulo, motivo do cadeado).
type Linha = (Item, &'static str, Option<&'static str>);

/// Os grupos, na ordem da tela.
pub const GRUPOS: [(&str, &[Linha]); 7] = [
    ("PERSONAGEM", &[
        (Item::Bolsa, "Bolsa", None),
        (Item::Ficha, "Ficha", Some("Em breve")),
        (Item::Habilidades, "Habilidades", Some("Em breve")),
        (Item::Montaria, "Montaria", Some("Em breve")),
    ]),
    ("PROGRESSO", &[
        (Item::Missoes, "Missões", None),
        (Item::TodasMissoes, "Todas", None),
        (Item::Conquistas, "Conquistas", Some("Em breve")),
    ]),
    ("OFICINA", &[
        (Item::Craft, "Craft", None),
        (Item::Forja, "Forja", None),
        (Item::Encantar, "Encantar", Some("Em breve")),
    ]),
    ("AVENTURA", &[
        (Item::Mapa, "Mapa", None),
        (Item::Aventuras, "Dungeons", Some("Em breve")),
    ]),
    ("SOCIAL", &[
        (Item::Grupo, "Grupo", Some("Em breve")),
        (Item::Amigos, "Amigos", Some("Em breve")),
        (Item::Correio, "Correio", Some("Em breve")),
        (Item::Clan, "Clã", Some("Em breve")),
    ]),
    // "Loja" do Menu e' a loja de CASH (Tempest Points), que ainda nao existe.
    // Vendedor NPC nunca vende de longe: "Vendedores" so' leva ate' ele.
    ("COMÉRCIO", &[
        (Item::LojaTp, "Loja", Some("Em breve — loja de Tempest Points (TP)")),
        (Item::Lojas, "Vendedores", None),
        (Item::Mercado, "Mercado", Some("Em breve")),
    ]),
    ("SISTEMA", &[
        (Item::Configuracoes, "Ajustes", Some("Em breve")),
        (Item::TrocarPersonagem, "Trocar", Some("Em breve")),
        (Item::Sair, "Sair", None),
    ]),
];

/// O que clicar num item faz.
#[derive(Debug, Clone, PartialEq)]
pub enum Clique {
    Abrir(Item),
    /// Bloqueado: so' avisa.
    Aviso(String),
}

pub fn clique_de(l: &Linha) -> Clique {
    match l.2 {
        Some(motivo) => Clique::Aviso(format!("{}: {motivo}.", l.1)),
        None => Clique::Abrir(l.0),
    }
}

/// O que o Menu mostra do personagem.
pub struct Contexto<'a> {
    pub nome: &'a str,
    pub nivel: u32,
    pub poder: Option<i32>,
    pub arma: &'a str,
    /// (rotulo, valor).
    pub saldos: &'a [(&'a str, u64)],
    /// Itens com ponto vermelho.
    pub selos: &'a [Item],
}

#[derive(Default)]
pub struct Menu {
    pub aberto: bool,
}

impl Menu {
    pub fn alterna(&mut self) {
        self.aberto = !self.aberto;
    }

    pub fn abrir(&mut self) {
        self.aberto = true;
    }

    pub fn fechar(&mut self) {
        self.aberto = false;
    }

    fn painel() -> Rect {
        let (sw, sh) = (screen_width(), screen_height());
        let w = (sw - 80.0).clamp(320.0, 1600.0);
        let h = (sh - 80.0).clamp(320.0, 900.0);
        Rect::new((sw - w) * 0.5, (sh - h) * 0.5, w, h)
    }

    /// Aberto, o Menu pega a tela toda (o mundo nao recebe clique nem roda).
    pub fn pega_mouse(&self) -> bool {
        self.aberto
    }

    pub fn desenha(&mut self, c: &Contexto) -> Option<Clique> {
        if !self.aberto {
            return None;
        }
        crate::hud_layout::escurece(0.6);
        let p = Self::painel();
        estilo::painel(p);
        let m = Vec2::from(mouse_position());
        let clique = is_mouse_button_pressed(MouseButton::Left);
        estilo::texto(p.x + 20.0, p.y + 34.0, "MENU", 24, estilo::OURO);
        if crate::ui::botao(Rect::new(p.x + p.w - 46.0, p.y + 12.0, 34.0, 30.0), "x", true) {
            self.aberto = false;
            return None;
        }
        draw_line(p.x + 14.0, p.y + 50.0, p.x + p.w - 14.0, p.y + 50.0, 1.0, estilo::BORDA);

        // ── coluna da esquerda: personagem e saldos ──
        let esq = Rect::new(p.x + 14.0, p.y + 60.0, (p.w * 0.26).clamp(170.0, 300.0), p.h - 74.0);
        estilo::painel(esq);
        let cx = esq.center().x;
        draw_circle(cx, esq.y + 56.0, 38.0, Color::new(0.075, 0.10, 0.14, 1.0));
        draw_circle_lines(cx, esq.y + 56.0, 38.0, 1.5, estilo::OURO);
        estilo::texto_centro(cx, esq.y + 52.0, "LV", 11, estilo::SUAVE);
        estilo::texto_centro(cx, esq.y + 76.0, &c.nivel.to_string(), 26, estilo::TEXTO);
        let mut y = esq.y + 122.0;
        estilo::texto_ajustado(c.nome, esq.x + 14.0, y, esq.w - 28.0, 19, estilo::TEXTO);
        y += 24.0;
        estilo::texto_ajustado(c.arma, esq.x + 14.0, y, esq.w - 28.0, 14, estilo::SUAVE);
        y += 30.0;
        estilo::texto(esq.x + 14.0, y, "PODER", 11, estilo::SUAVE);
        let poder = c.poder.map(|v| crate::bolsa::milhar(v.max(0) as u64)).unwrap_or_else(|| "—".into());
        estilo::texto(esq.x + esq.w - 14.0 - estilo::medir(&poder, 18), y + 2.0, &poder, 18, estilo::OURO);
        y += 16.0;
        draw_line(esq.x + 10.0, y, esq.x + esq.w - 10.0, y, 1.0, estilo::BORDA);
        y += 24.0;
        estilo::texto(esq.x + 14.0, y, "SALDOS", 11, estilo::SUAVE);
        for (rotulo, valor) in c.saldos {
            y += 22.0;
            if y > esq.y + esq.h - 8.0 {
                break;
            }
            estilo::texto(esq.x + 14.0, y, rotulo, 14, estilo::TEXTO);
            let v = crate::bolsa::milhar(*valor);
            estilo::texto(esq.x + esq.w - 14.0 - estilo::medir(&v, 14), y, &v, 14, estilo::OURO);
        }

        // ── direita: grupos em duas colunas ──
        let dir = Rect::new(esq.x + esq.w + 14.0, esq.y, p.x + p.w - 14.0 - (esq.x + esq.w + 14.0), esq.h);
        let colunas = [&GRUPOS[..4], &GRUPOS[4..]];
        let col_w = (dir.w - 14.0) * 0.5;
        let por_linha = 4.0;
        let t_w = (col_w - 10.0 * (por_linha - 1.0)) / por_linha;
        let t_h = (dir.h / 4.0) - 46.0;
        let t = t_w.min(t_h).clamp(44.0, 110.0);
        let mut saida = None;
        let mut dica: Option<(Rect, String)> = None;
        for (k, grupos) in colunas.iter().enumerate() {
            let x0 = dir.x + k as f32 * (col_w + 14.0);
            let mut gy = dir.y;
            for (nome, itens) in grupos.iter() {
                estilo::texto(x0, gy + 16.0, nome, 13, estilo::SUAVE);
                gy += 24.0;
                for (i, l) in itens.iter().enumerate() {
                    let r = Rect::new(x0 + i as f32 * (t + 10.0), gy, t, t);
                    let sobre = r.contains(m);
                    let travado = l.2.is_some();
                    estilo::painel(r);
                    if sobre {
                        draw_rectangle(r.x, r.y, r.w, r.h, Color::new(1.0, 1.0, 1.0, 0.06));
                    }
                    let cor = if travado {
                        Color::new(0.45, 0.47, 0.50, 1.0)
                    } else if sobre {
                        estilo::OURO
                    } else {
                        estilo::TEXTO
                    };
                    icone_do_item(l.0, vec2(r.center().x, r.y + r.h * 0.42), r.w * 0.22, cor);
                    estilo::texto_ajustado(l.1, r.x + 4.0, r.y + r.h - 7.0, r.w - 8.0, 12, cor);
                    if travado {
                        cadeado(vec2(r.x + r.w - 11.0, r.y + 12.0), 6.0);
                        if sobre {
                            dica = Some((r, format!("{} · {}", l.1, l.2.unwrap_or(""))));
                        }
                    } else if c.selos.contains(&l.0) {
                        selo(r);
                    }
                    if sobre && clique {
                        saida = Some(clique_de(l));
                    }
                }
                gy += t + 22.0;
            }
        }
        if let Some((r, texto)) = dica {
            let w = estilo::medir(&texto, 13) + 16.0;
            let x = (r.center().x - w * 0.5).clamp(4.0, screen_width() - w - 4.0);
            let caixa = Rect::new(x, r.y + r.h + 4.0, w, 24.0);
            estilo::painel(caixa);
            estilo::texto(caixa.x + 8.0, caixa.y + 17.0, &texto, 13, estilo::TEXTO);
        }
        saida
    }
}

fn cadeado(c: Vec2, s: f32) {
    let cor = Color::new(0.85, 0.74, 0.50, 1.0);
    draw_rectangle(c.x - s, c.y - s * 0.2, s * 2.0, s * 1.5, cor);
    estilo::arco(c - vec2(0.0, s * 0.2), s * 0.7, std::f32::consts::PI, 0.5, 1.8, cor);
}

fn icone_do_item(item: Item, c: Vec2, s: f32, cor: Color) {
    match item {
        Item::Bolsa => pictograma(0, c, s, cor),
        Item::Missoes | Item::TodasMissoes | Item::Conquistas => pictograma(1, c, s, cor),
        Item::Grupo | Item::Amigos | Item::Clan => pictograma(2, c, s, cor),
        Item::Correio => pictograma(3, c, s, cor),
        Item::Craft => estilo::icone(3, c, s * 1.1, cor),
        Item::Forja | Item::Encantar => estilo::icone(1, c, s * 1.1, cor),
        Item::Habilidades => estilo::icone(6, c, s * 1.1, cor),
        Item::Aventuras => estilo::icone(7, c, s * 1.1, cor),
        _ => {
            let letra: String = format!("{item:?}").chars().next().unwrap_or('?').to_string();
            draw_circle_lines(c.x, c.y, s * 1.1, 1.5, cor);
            estilo::texto_centro(c.x, c.y + s * 0.5, &letra, (s * 1.3).clamp(12.0, 40.0) as u16, cor);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn todo_sistema_que_existe_abre_e_o_resto_so_avisa() {
        let abre = [Item::Bolsa, Item::Missoes, Item::TodasMissoes, Item::Craft, Item::Forja, Item::Mapa, Item::Lojas, Item::Sair];
        for (_, itens) in GRUPOS.iter() {
            for l in itens.iter() {
                match clique_de(l) {
                    Clique::Abrir(i) => assert!(abre.contains(&i), "{:?} abre mas nao existe", i),
                    Clique::Aviso(t) => {
                        assert!(!abre.contains(&l.0), "{:?} existe mas esta' travado", l.0);
                        assert!(t.contains("Em breve"), "aviso sem motivo: {t}");
                    }
                }
            }
        }
        // "Loja" e' a de cash (TP), travada; vendedor NPC so' com "Ir".
        let loja = GRUPOS.iter().flat_map(|(_, it)| it.iter()).find(|l| l.1 == "Loja").expect("sem Loja");
        assert_eq!(loja.0, Item::LojaTp);
        assert!(matches!(clique_de(loja), Clique::Aviso(t) if t.contains("TP")));
        for i in abre {
            assert!(GRUPOS.iter().any(|(_, it)| it.iter().any(|l| l.0 == i)), "{i:?} fora do menu");
        }
    }

    #[test]
    fn nenhum_grupo_passa_de_quatro_itens() {
        // A grade tem 4 por linha; um quinto sairia da coluna.
        assert!(GRUPOS.iter().all(|(_, it)| it.len() <= 4));
    }
}
