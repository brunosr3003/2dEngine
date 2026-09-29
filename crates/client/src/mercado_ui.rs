//! Menu -> Trade -> Market: the global market (docs/MERCADO.md), in the MIR4
//! mould. One market for every server.
//!
//! The client only ASKS. Level, binding, gold, escrow and fee are decided on
//! the server; the fee maths here is the same function (`shared::mercado`),
//! only to show before confirming. Everything by touch: quantity and price
//! are buttons, not typing — only the search uses the keyboard.
use std::collections::{HashMap, HashSet};

use macroquad::prelude::*;
use shared::mercado::{self as regras, AnuncioNet, CartaNet, Categoria, FiltroNet, VendaNet};
use shared::protocol::ClientMessage;
use shared::InventorySlot;

use crate::economia::milhar;
use crate::hud_estilo as estilo;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Aba {
    #[default]
    Comprar,
    Vender,
    Meus,
    Entregas,
    Tp,
}

const ABAS: [(Aba, &str); 5] = [
    (Aba::Comprar, "Comprar"),
    (Aba::Vender, "Vender"),
    (Aba::Meus, "Meus anúncios"),
    (Aba::Entregas, "Entregas"),
    (Aba::Tp, "TP"),
];

/// What the panel reads from the game.
pub struct Contexto<'a> {
    pub slots: &'a [InventorySlot],
    pub nomes: &'a HashMap<u16, String>,
    pub ouro: u64,
    pub nivel: u32,
    pub digitado: &'a [char],
    pub vox: &'a crate::vox::VoxCache,
    pub solido: &'a macroquad::material::Material,
}

/// A purchase being confirmed.
struct Compra {
    anuncio: AnuncioNet,
    qtd: u64,
}

#[derive(Default)]
pub struct Mercado {
    pub aberto: bool,
    /// An item's magnifier asked for "Where to get" (main opens the popup).
    pub onde_obter: Option<u16>,
    /// Bound items (`ItemsConfig`): they do not show up to sell.
    pub vinculados: HashSet<u16>,
    aba: Aba,
    categoria: u8,
    busca: String,
    foco_busca: bool,
    pagina: u16,
    lista: Vec<AnuncioNet>,
    tem_mais: bool,
    lista_tp: Vec<AnuncioNet>,
    /// The last search asked for was the TP one (the answer does not say).
    ultima_busca_tp: bool,
    meus: Vec<AnuncioNet>,
    historico: Vec<VendaNet>,
    cartas: Vec<CartaNet>,
    tp: Option<u64>,
    compra: Option<Compra>,
    venda_slot: Option<usize>,
    venda_qtd: u64,
    venda_preco: u64,
    tp_qtd: u64,
    tp_preco: u64,
    /// (text, ok, when)
    aviso: Option<(String, bool, f64)>,
    /// Primeira linha visivel da lista da aba.
    rolagem: usize,
    rolagem_venda: crate::rolagem::Rolagem,
}

fn filtro_tp() -> FiltroNet {
    FiltroNet {
        categoria: Categoria::Tp as u8,
        texto: String::new(),
        pagina: 0,
    }
}

impl Mercado {
    /// Opens on the Buy tab and returns what to ask the server.
    pub fn abrir(&mut self) -> Vec<ClientMessage> {
        self.aberto = true;
        self.aba = Aba::Comprar;
        self.compra = None;
        let mut v = self.pedidos_da_aba();
        v.push(ClientMessage::MercadoEntregas);
        v
    }

    pub fn abrir_entregas(&mut self) -> Vec<ClientMessage> {
        self.aberto = true;
        self.aba = Aba::Entregas;
        self.compra = None;
        self.foco_busca = false;
        self.rolagem = 0;
        self.pedidos_da_aba()
    }

    /// Opens on the Buy tab already searching `texto` (the "Search" from Where to get).
    pub fn abrir_buscando(&mut self, texto: &str) -> Vec<ClientMessage> {
        self.busca = texto.to_string();
        self.categoria = Categoria::Todas as u8;
        self.pagina = 0;
        self.rolagem = 0;
        self.abrir()
    }

    pub fn fechar(&mut self) {
        self.aberto = false;
        self.foco_busca = false;
        self.compra = None;
    }

    /// The on-screen keyboard opens with the search focused.
    pub fn foco_na_busca(&self) -> bool {
        self.aberto && self.foco_busca
    }

    fn filtro(&self) -> FiltroNet {
        FiltroNet {
            categoria: self.categoria,
            texto: self.busca.trim().to_string(),
            pagina: self.pagina,
        }
    }

    fn buscar(&mut self) -> ClientMessage {
        self.ultima_busca_tp = false;
        ClientMessage::MercadoBuscar {
            filtro: self.filtro(),
        }
    }

    fn pedidos_da_aba(&mut self) -> Vec<ClientMessage> {
        match self.aba {
            Aba::Comprar => vec![self.buscar()],
            Aba::Vender | Aba::Meus => vec![ClientMessage::MercadoMeus],
            Aba::Entregas => vec![ClientMessage::MercadoEntregas],
            Aba::Tp => {
                self.ultima_busca_tp = true;
                vec![
                    ClientMessage::MercadoBuscar {
                        filtro: filtro_tp(),
                    },
                    ClientMessage::MercadoMeus,
                ]
            }
        }
    }

    // ── what the server sends ──

    pub fn lista(&mut self, anuncios: Vec<AnuncioNet>, pagina: u16, tem_mais: bool) {
        let de_tp = anuncios
            .first()
            .map_or(self.ultima_busca_tp, |a| a.tipo == regras::TIPO_TP);
        if de_tp {
            self.lista_tp = anuncios;
        } else {
            self.lista = anuncios;
            self.pagina = pagina;
            self.tem_mais = tem_mais;
        }
    }

    pub fn meus(&mut self, anuncios: Vec<AnuncioNet>, historico: Vec<VendaNet>, tp: u64) {
        self.meus = anuncios;
        self.historico = historico;
        self.tp = Some(tp);
    }

    pub fn entregas(&mut self, cartas: Vec<CartaNet>, tp: u64) {
        self.cartas = cartas;
        self.tp = Some(tp);
    }

    /// Resposta de um pedido: mostra e atualiza a aba aberta.
    pub fn resultado(&mut self, ok: bool, texto: String, agora: f64) -> Vec<ClientMessage> {
        self.aviso = Some((texto, ok, agora));
        if !self.aberto || !ok {
            return Vec::new();
        }
        let mut v = self.pedidos_da_aba();
        if self.aba != Aba::Entregas {
            v.push(ClientMessage::MercadoEntregas);
        }
        v
    }

    fn trocar_aba(&mut self, aba: Aba) -> Vec<ClientMessage> {
        if self.aba == aba {
            return Vec::new();
        }
        self.aba = aba;
        self.rolagem = 0;
        self.rolagem_venda.zera();
        self.foco_busca = false;
        self.compra = None;
        self.pedidos_da_aba()
    }

    /// Desenha e devolve os pedidos do quadro.
    pub fn desenha(&mut self, c: &Contexto, agora: f64) -> Vec<ClientMessage> {
        estilo::no_painel(estilo::escala_do_painel(980.0, 660.0), || {
            self.desenha_na_escala(c, agora)
        })
    }

    fn desenha_na_escala(&mut self, c: &Contexto, agora: f64) -> Vec<ClientMessage> {
        let mut saida = Vec::new();
        if !self.aberto {
            return saida;
        }
        crate::hud_layout::escurece(0.55);
        let f = estilo::fator_texto();
        let seguro = crate::hud_layout::tela_segura();
        let (w, h) = (
            (980.0 * f).min(seguro.w - 16.0),
            (660.0 * f).min(seguro.h - 16.0),
        );
        let p = Rect::new(
            seguro.center().x - w * 0.5,
            seguro.center().y - h * 0.5,
            w,
            h,
        );
        estilo::painel(p);
        estilo::texto(p.x + 18.0 * f, p.y + 34.0 * f, "Mercado", 22, estilo::OURO);
        let fechar = Rect::new(p.x + p.w - 48.0 * f, p.y + 10.0 * f, 38.0 * f, 34.0 * f);
        // Saldo: moeda de ouro e cristal de TP, os mesmos icones da Loja.
        let txt_tp = self.tp.map_or("—".to_string(), milhar);
        let txt_ouro = milhar(c.ouro);
        let x_tp = fechar.x - 16.0 * f - estilo::largura_tp_texto(&txt_tp, 15, true);
        estilo::tp_texto(x_tp, p.y + 32.0 * f, &txt_tp, 15, estilo::TEXTO, true);
        let x_ouro = x_tp - 22.0 * f - estilo::largura_ouro_texto(&txt_ouro, 15, true);
        estilo::ouro_texto(x_ouro, p.y + 32.0 * f, &txt_ouro, 15, estilo::OURO, true);
        if crate::ui::botao(fechar, "x", true) {
            self.fechar();
            return saida;
        }

        // ── abas ──
        let livre = self.compra.is_none();
        let m = Vec2::from(mouse_position());
        let clicou = crate::foco::clique();
        let ty = p.y + 52.0 * f;
        let tw = (p.w - 36.0 * f) / ABAS.len() as f32;
        for (i, (aba, rotulo)) in ABAS.iter().enumerate() {
            let r = Rect::new(p.x + 18.0 * f + i as f32 * tw, ty, tw - 4.0 * f, 40.0 * f);
            let sobre = livre && r.contains(m);
            estilo::aba(r, rotulo, self.aba == *aba, sobre);
            if *aba == Aba::Entregas && !self.cartas.is_empty() {
                estilo::badge(r);
            }
            if sobre && clicou {
                saida.extend(self.trocar_aba(*aba));
            }
        }
        let area = Rect::new(
            p.x + 18.0 * f,
            ty + 52.0 * f,
            p.w - 36.0 * f,
            p.h - (ty - p.y) - 52.0 * f - 40.0 * f,
        );
        match self.aba {
            Aba::Comprar => self.aba_comprar(area, c, f, livre, &mut saida),
            Aba::Vender => self.aba_vender(area, c, f, livre, &mut saida),
            Aba::Meus => self.aba_meus(area, c, f, livre, &mut saida),
            Aba::Entregas => self.aba_entregas(area, c, f, livre, &mut saida),
            Aba::Tp => self.aba_tp(area, c, f, livre, &mut saida),
        }

        if let Some((texto, ok, quando)) = &self.aviso {
            if agora - quando < 6.0 {
                let cor = if *ok { estilo::VERDE } else { estilo::VERMELHO };
                estilo::texto_ajustado(
                    texto,
                    p.x + 18.0 * f,
                    p.y + p.h - 16.0 * f,
                    p.w - 36.0 * f,
                    14,
                    cor,
                );
            }
        }
        if self.compra.is_some() {
            self.dialogo_compra(c, f, &mut saida);
        }
        saida
    }

    // ─────────────────────────────── Comprar ───────────────────────────────

    fn aba_comprar(
        &mut self,
        a: Rect,
        c: &Contexto,
        f: f32,
        livre: bool,
        saida: &mut Vec<ClientMessage>,
    ) {
        let m = Vec2::from(mouse_position());
        let clicou = crate::foco::clique();
        // Categorias.
        let cw = (a.w * 0.5 / Categoria::FILTROS.len() as f32).min(150.0 * f);
        for (i, cat) in Categoria::FILTROS.iter().enumerate() {
            let r = Rect::new(a.x + i as f32 * cw, a.y, cw - 6.0 * f, 38.0 * f);
            let sel = self.categoria == *cat as u8;
            estilo::cartao(r, livre && r.contains(m), sel);
            estilo::texto_centro(
                r.center().x,
                r.center().y + 5.0 * f,
                cat.nome(),
                14,
                if sel { estilo::OURO } else { estilo::TEXTO },
            );
            if livre && clicou && r.contains(m) && !sel {
                self.categoria = *cat as u8;
                self.pagina = 0;
                self.rolagem = 0;
                saida.push(self.buscar());
            }
        }
        // Busca.
        let xb = a.x + cw * Categoria::FILTROS.len() as f32 + 8.0 * f;
        let bb = Rect::new(a.x + a.w - 104.0 * f, a.y, 104.0 * f, 38.0 * f);
        let campo = Rect::new(xb, a.y, (bb.x - 8.0 * f - xb).max(80.0 * f), 38.0 * f);
        let digitado: &[char] = if self.foco_busca { c.digitado } else { &[] };
        if crate::ui::campo(campo, "", &mut self.busca, self.foco_busca, false, digitado) && livre {
            self.foco_busca = true;
        } else if clicou && !campo.contains(m) {
            self.foco_busca = false;
        }
        if self.busca.chars().count() > regras::BUSCA_MAX_CHARS {
            self.busca = self.busca.chars().take(regras::BUSCA_MAX_CHARS).collect();
        }
        if self.busca.is_empty() && !self.foco_busca {
            estilo::texto(
                campo.x + 12.0 * f,
                campo.center().y + 5.0 * f,
                "Buscar item…",
                14,
                estilo::SUAVE,
            );
        }
        let enter = self.foco_busca
            && (is_key_pressed(KeyCode::Enter)
                || c.digitado.iter().any(|ch| *ch == '\r' || *ch == '\n'));
        if botao(bb, "Buscar", livre, true) || enter {
            self.pagina = 0;
            self.rolagem = 0;
            self.foco_busca = false;
            saida.push(self.buscar());
        }

        // Lista.
        let lista = Rect::new(a.x, a.y + 50.0 * f, a.w, a.h - 50.0 * f - 46.0 * f);
        if self.lista.is_empty() {
            estilo::texto(
                lista.x + 4.0,
                lista.y + 30.0 * f,
                "Nenhum anúncio encontrado.",
                15,
                estilo::SUAVE,
            );
        }
        let alt = 58.0 * f;
        let cabem = ((lista.h / alt).floor() as usize).max(1);
        self.rolagem = self.rolagem.min(self.lista.len().saturating_sub(cabem));
        let mut comprar = None;
        let mut onde = None;
        for (i, an) in self.lista.iter().enumerate().skip(self.rolagem).take(cabem) {
            let r = Rect::new(
                lista.x,
                lista.y + (i - self.rolagem) as f32 * alt,
                lista.w,
                alt - 6.0 * f,
            );
            if linha_de_anuncio(
                r,
                an,
                c,
                f,
                livre,
                if an.meu { "Seu" } else { "Comprar" },
                !an.meu,
            ) {
                comprar = Some(an.clone());
            }
            // Touching the item's icon: "Where to get" (TP has none).
            if an.tipo == regras::TIPO_ITEM
                && livre
                && crate::onde_obter::lupa_no_icone(icone_do_anuncio(r, f))
            {
                onde = Some(an.item_id);
            }
        }
        if onde.is_some() {
            self.onde_obter = onde;
        }
        if let Some(an) = comprar {
            self.compra = Some(Compra {
                anuncio: an,
                qtd: 1,
            });
        }
        // Rodape: rolagem e paginas.
        let yb = a.y + a.h - 40.0 * f;
        self.rolar(
            Rect::new(a.x, yb, 200.0 * f, 38.0 * f),
            self.lista.len(),
            cabem,
            livre,
            f,
        );
        let prox = Rect::new(a.x + a.w - 130.0 * f, yb, 130.0 * f, 38.0 * f);
        let ant = Rect::new(prox.x - 138.0 * f, yb, 130.0 * f, 38.0 * f);
        estilo::texto_centro(
            ant.x - 60.0 * f,
            yb + 25.0 * f,
            &format!("Página {}", self.pagina + 1),
            14,
            estilo::SUAVE,
        );
        if botao(ant, "‹ Anterior", livre && self.pagina > 0, false) {
            self.pagina -= 1;
            self.rolagem = 0;
            saida.push(ClientMessage::MercadoBuscar {
                filtro: self.filtro(),
            });
            self.ultima_busca_tp = false;
        }
        if botao(prox, "Próxima ›", livre && self.tem_mais, false) {
            self.pagina += 1;
            self.rolagem = 0;
            saida.push(ClientMessage::MercadoBuscar {
                filtro: self.filtro(),
            });
            self.ultima_busca_tp = false;
        }
    }

    /// Arrows up/down when the list does not fit (on a phone there is no mouse wheel).
    fn rolar(&mut self, r: Rect, total: usize, cabem: usize, livre: bool, f: f32) {
        let (_, roda) = mouse_wheel();
        if livre && roda != 0.0 {
            self.rolagem = if roda < 0.0 {
                self.rolagem + 1
            } else {
                self.rolagem.saturating_sub(1)
            };
        }
        if total <= cabem {
            return;
        }
        let cima = Rect::new(r.x, r.y, 78.0 * f, r.h);
        let baixo = Rect::new(r.x + 84.0 * f, r.y, 78.0 * f, r.h);
        if botao(cima, "Subir", livre && self.rolagem > 0, false) {
            self.rolagem = self.rolagem.saturating_sub(cabem.max(2) - 1);
        }
        if botao(
            baixo,
            "Descer",
            livre && self.rolagem + cabem < total,
            false,
        ) {
            self.rolagem += cabem.max(2) - 1;
        }
        self.rolagem = self.rolagem.min(total.saturating_sub(cabem));
    }

    fn dialogo_compra(&mut self, c: &Contexto, f: f32, saida: &mut Vec<ClientMessage>) {
        let Some(compra) = &mut self.compra else {
            return;
        };
        let seguro = crate::hud_layout::tela_segura();
        let (w, h) = (
            (460.0 * f).min(seguro.w - 24.0),
            (330.0 * f).min(seguro.h - 24.0),
        );
        let r = Rect::new(
            seguro.center().x - w * 0.5,
            seguro.center().y - h * 0.5,
            w,
            h,
        );
        estilo::sombra(r, estilo::RAIO_PEQUENO, 1.0);
        estilo::painel_destaque(r, estilo::OURO);
        let an = &compra.anuncio;
        let tp = an.tipo == regras::TIPO_TP;
        estilo::texto_forte(
            r.x + 18.0 * f,
            r.y + 34.0 * f,
            "Confirmar compra",
            18,
            estilo::OURO,
        );
        let mut y = r.y + 52.0 * f;
        if tp {
            estilo::icone_tp(vec2(r.x + 42.0 * f, y + 24.0 * f), 50.0 * f);
        } else {
            crate::icones::icone_com_3d(
                an.item_id,
                Rect::new(r.x + 18.0 * f, y, 48.0 * f, 48.0 * f),
                an.instancia.map(|i| i.rarity),
                None,
                Some((c.vox, c.solido)),
            );
        }
        let xn = r.x + 76.0 * f;
        estilo::texto_ajustado(
            &nome_do_anuncio(an, c),
            xn,
            y + 22.0 * f,
            r.x + r.w - xn - 18.0 * f,
            16,
            estilo::TEXTO,
        );
        estilo::texto(
            xn,
            y + 42.0 * f,
            &format!(
                "{} gold cada · {} à venda",
                milhar(an.preco_unit),
                milhar(an.qtd)
            ),
            13,
            estilo::SUAVE,
        );
        y += 88.0 * f;
        let max = an.qtd.max(1);
        seletor(
            Rect::new(r.x + 18.0 * f, y, r.w - 36.0 * f, 44.0 * f),
            "Quantidade",
            &mut compra.qtd,
            1,
            max,
            false,
            f,
        );
        y += 64.0 * f;
        let total = regras::total(compra.qtd, an.preco_unit);
        let cabe = total.is_some_and(|t| t <= c.ouro);
        let txt_total = total.map_or("—".to_string(), milhar);
        estilo::texto(r.x + 18.0 * f, y, "Total", 15, estilo::SUAVE);
        estilo::texto_forte(
            r.x + r.w - 18.0 * f - estilo::medir_forte(&txt_total, 18),
            y,
            &txt_total,
            18,
            if cabe { estilo::OURO } else { estilo::VERMELHO },
        );
        if !cabe {
            estilo::texto(
                r.x + 18.0 * f,
                y + 20.0 * f,
                "Gold insuficiente.",
                13,
                estilo::VERMELHO,
            );
        }
        let yb = r.y + r.h - 56.0 * f;
        let cancelar = Rect::new(r.x + 18.0 * f, yb, (r.w - 48.0 * f) * 0.5, 42.0 * f);
        let confirmar = Rect::new(cancelar.x + cancelar.w + 12.0 * f, yb, cancelar.w, 42.0 * f);
        if botao(cancelar, "Cancelar", true, false) {
            self.compra = None;
            return;
        }
        if botao(confirmar, "Comprar", cabe, true) {
            saida.push(ClientMessage::MercadoComprar {
                anuncio: an.id.clone(),
                qtd: compra.qtd,
                preco_unit: an.preco_unit,
            });
            self.compra = None;
        }
    }

    // ─────────────────────────────── Vender ───────────────────────────────

    fn aba_vender(
        &mut self,
        a: Rect,
        c: &Contexto,
        f: f32,
        livre: bool,
        saida: &mut Vec<ClientMessage>,
    ) {
        let m = Vec2::from(mouse_position());
        let pode = c.nivel >= regras::NIVEL_PARA_VENDER;
        let esq = Rect::new(a.x, a.y, a.w * 0.52, a.h);
        let dir = Rect::new(a.x + a.w * 0.55, a.y, a.w * 0.45, a.h);
        estilo::texto_forte(esq.x, esq.y + 14.0 * f, "Da bolsa", 14, estilo::SUAVE);
        let vendaveis: Vec<(usize, InventorySlot)> = c
            .slots
            .iter()
            .copied()
            .enumerate()
            .filter(|(_, s)| s.qty > 0 && !self.vinculados.contains(&s.item_id))
            .collect();
        let presos = c
            .slots
            .iter()
            .filter(|s| s.qty > 0 && self.vinculados.contains(&s.item_id))
            .count();
        if presos > 0 {
            let t = format!("{presos} vinculado(s) fora da lista");
            estilo::texto(
                esq.x + esq.w - estilo::medir(&t, 12),
                esq.y + 14.0 * f,
                &t,
                12,
                estilo::SUAVE,
            );
        }
        let lado = 66.0 * f;
        let por_linha = ((esq.w / lado).floor() as usize).max(1);
        let grade = Rect::new(esq.x, esq.y + 26.0 * f, esq.w, esq.h - 26.0 * f - 46.0 * f);
        let total_linhas = vendaveis.len().div_ceil(por_linha);
        let clique_na_grade = if livre {
            self.rolagem_venda
                .quadro(grade, total_linhas as f32 * lado, lado)
        } else {
            None
        };
        if self.venda_slot.is_some_and(|i| {
            c.slots
                .get(i)
                .is_none_or(|s| s.qty == 0 || self.vinculados.contains(&s.item_id))
        }) {
            self.venda_slot = None;
        }
        if vendaveis.is_empty() {
            estilo::texto(
                grade.x,
                grade.y + 30.0 * f,
                "Nada vendável na bolsa.",
                15,
                estilo::SUAVE,
            );
        }
        crate::rolagem::recortar(Some(grade));
        for (k, (i, s)) in vendaveis.iter().enumerate() {
            let r = Rect::new(
                grade.x + (k % por_linha) as f32 * lado,
                grade.y + (k / por_linha) as f32 * lado - self.rolagem_venda.pos,
                lado - 6.0 * f,
                lado - 6.0 * f,
            );
            if r.y + r.h < grade.y || r.y > grade.y + grade.h {
                continue;
            }
            let sel = self.venda_slot == Some(*i);
            estilo::slot(r, None, livre && r.contains(m), sel);
            crate::icones::icone_com_3d(
                s.item_id,
                r,
                s.instance.map(|x| x.rarity),
                Some(s.qty),
                Some((c.vox, c.solido)),
            );
            if clique_na_grade.is_some_and(|p| r.contains(p)) {
                self.venda_slot = Some(*i);
                self.venda_qtd = 1;
                if self.venda_preco == 0 {
                    self.venda_preco = 100;
                }
                // Price reference: what is already on sale for that item.
                let nome = c.nomes.get(&s.item_id).cloned().unwrap_or_default();
                self.ultima_busca_tp = false;
                saida.push(ClientMessage::MercadoBuscar {
                    filtro: FiltroNet {
                        categoria: 0,
                        texto: nome,
                        pagina: 0,
                    },
                });
            }
        }
        crate::rolagem::recortar(None);
        self.rolagem_venda
            .desenha(grade, total_linhas as f32 * lado);

        // Formulario.
        estilo::painel(dir);
        let (x, w) = (dir.x + 16.0 * f, dir.w - 32.0 * f);
        let mut y = dir.y + 30.0 * f;
        let ativos = format!(
            "Anúncios ativos {}/{}",
            self.meus.len(),
            regras::MAX_ANUNCIOS
        );
        estilo::texto(x, y, &ativos, 13, estilo::SUAVE);
        y += 24.0 * f;
        if !pode {
            estilo::texto_ajustado(
                &format!(
                    "Vender no mercado libera no nível {}.",
                    regras::NIVEL_PARA_VENDER
                ),
                x,
                y + 10.0 * f,
                w,
                15,
                estilo::VERMELHO,
            );
            y += 34.0 * f;
        }
        let Some(slot) = self.venda_slot.and_then(|i| c.slots.get(i).copied()) else {
            estilo::texto_ajustado(
                "Escolha um item da bolsa.",
                x,
                y + 20.0 * f,
                w,
                15,
                estilo::SUAVE,
            );
            return;
        };
        let nome = c
            .nomes
            .get(&slot.item_id)
            .cloned()
            .unwrap_or_else(|| format!("Item {}", slot.item_id));
        estilo::texto_ajustado(&nome, x, y + 8.0 * f, w - 48.0 * f, 18, estilo::TEXTO);
        if livre
            && crate::onde_obter::botao(Rect::new(
                x + w - 40.0 * f,
                y - 14.0 * f,
                40.0 * f,
                34.0 * f,
            ))
        {
            self.onde_obter = Some(slot.item_id);
        }
        y += 44.0 * f;
        seletor(
            Rect::new(x, y, w, 42.0 * f),
            "Quantidade",
            &mut self.venda_qtd,
            1,
            slot.qty.max(1) as u64,
            false,
            f,
        );
        y += 66.0 * f;
        seletor(
            Rect::new(x, y, w, 42.0 * f),
            "Preço por unidade (gold)",
            &mut self.venda_preco,
            1,
            regras::PRECO_MAX_UNIT,
            true,
            f,
        );
        y += 60.0 * f;
        let bruto = regras::total(self.venda_qtd, self.venda_preco).unwrap_or(0);
        linha_valor(x, y, w, "Total", &milhar(bruto), estilo::TEXTO, f);
        linha_valor(
            x,
            y + 22.0 * f,
            w,
            &format!("Taxa {}%", regras::TAXA_PCT),
            &format!("−{}", milhar(regras::taxa(bruto))),
            estilo::SUAVE,
            f,
        );
        linha_valor(
            x,
            y + 44.0 * f,
            w,
            "Você recebe",
            &milhar(regras::liquido(bruto)),
            estilo::OURO,
            f,
        );
        let menor = self
            .lista
            .iter()
            .filter(|an| an.item_id == slot.item_id && an.tipo == regras::TIPO_ITEM)
            .map(|an| an.preco_unit)
            .min();
        if let Some(pr) = menor {
            estilo::texto(
                x,
                y + 70.0 * f,
                &format!("Menor preço à venda: {} gold", milhar(pr)),
                13,
                estilo::SUAVE,
            );
        }
        let bt = Rect::new(x, dir.y + dir.h - 58.0 * f, w, 44.0 * f);
        if botao(
            bt,
            "Anunciar",
            livre && pode && self.meus.len() < regras::MAX_ANUNCIOS,
            true,
        ) {
            if let Some(i) = self.venda_slot {
                saida.push(ClientMessage::MercadoAnunciar {
                    inv_slot: i as u16,
                    qtd: self.venda_qtd.min(u32::MAX as u64) as u32,
                    preco_unit: self.venda_preco,
                });
                self.venda_slot = None;
            }
        }
    }

    // ─────────────────────────────── Meus ───────────────────────────────

    fn aba_meus(
        &mut self,
        a: Rect,
        c: &Contexto,
        f: f32,
        livre: bool,
        saida: &mut Vec<ClientMessage>,
    ) {
        let esq = Rect::new(a.x, a.y, a.w * 0.58, a.h);
        let dir = Rect::new(a.x + a.w * 0.61, a.y, a.w * 0.39, a.h);
        estilo::texto_forte(
            esq.x,
            esq.y + 14.0 * f,
            &format!("Ativos ({}/{})", self.meus.len(), regras::MAX_ANUNCIOS),
            14,
            estilo::SUAVE,
        );
        let alt = 58.0 * f;
        let lista = Rect::new(esq.x, esq.y + 26.0 * f, esq.w, esq.h - 26.0 * f - 46.0 * f);
        let cabem = ((lista.h / alt).floor() as usize).max(1);
        self.rolagem = self.rolagem.min(self.meus.len().saturating_sub(cabem));
        if self.meus.is_empty() {
            estilo::texto(
                lista.x,
                lista.y + 30.0 * f,
                "Nenhum anúncio ativo.",
                15,
                estilo::SUAVE,
            );
        }
        for (i, an) in self.meus.iter().enumerate().skip(self.rolagem).take(cabem) {
            let r = Rect::new(
                lista.x,
                lista.y + (i - self.rolagem) as f32 * alt,
                lista.w,
                alt - 6.0 * f,
            );
            if linha_de_anuncio(r, an, c, f, livre, "Cancelar", true) {
                saida.push(ClientMessage::MercadoCancelar {
                    anuncio: an.id.clone(),
                });
            }
        }
        self.rolar(
            Rect::new(esq.x, a.y + a.h - 40.0 * f, 200.0 * f, 38.0 * f),
            self.meus.len(),
            cabem,
            livre,
            f,
        );

        estilo::painel(dir);
        estilo::texto_forte(
            dir.x + 14.0 * f,
            dir.y + 26.0 * f,
            "Histórico",
            15,
            estilo::SUAVE,
        );
        let mut y = dir.y + 52.0 * f;
        if self.historico.is_empty() {
            estilo::texto(dir.x + 14.0 * f, y, "Nada ainda.", 14, estilo::SUAVE);
        }
        for v in &self.historico {
            if y > dir.y + dir.h - 12.0 * f {
                break;
            }
            let (verbo, valor, cor) = if v.vendi {
                ("Vendeu", format!("+{}", milhar(v.liquido)), estilo::VERDE)
            } else {
                (
                    "Comprou",
                    format!("−{}", milhar(v.qtd * v.preco_unit)),
                    estilo::TEXTO,
                )
            };
            estilo::texto_ajustado(
                &format!("{verbo} {} ×{}", v.nome, milhar(v.qtd)),
                dir.x + 14.0 * f,
                y,
                dir.w * 0.62,
                13,
                estilo::TEXTO,
            );
            estilo::texto_forte(
                dir.x + dir.w - 14.0 * f - estilo::medir_forte(&valor, 13),
                y,
                &valor,
                13,
                cor,
            );
            y += 24.0 * f;
        }
    }

    // ─────────────────────────────── Entregas ───────────────────────────────

    fn aba_entregas(
        &mut self,
        a: Rect,
        c: &Contexto,
        f: f32,
        livre: bool,
        saida: &mut Vec<ClientMessage>,
    ) {
        estilo::texto_ajustado(
            "Vendas, compras, cancelamentos e devoluções esperam aqui.",
            a.x,
            a.y + 14.0 * f,
            a.w - 200.0 * f,
            14,
            estilo::SUAVE,
        );
        let receber = Rect::new(a.x + a.w - 180.0 * f, a.y - 4.0 * f, 180.0 * f, 40.0 * f);
        if botao(
            receber,
            "Receber tudo",
            livre && !self.cartas.is_empty(),
            true,
        ) {
            saida.push(ClientMessage::MercadoReceber);
        }
        let alt = 54.0 * f;
        let topo = a.y + 46.0 * f;
        let lista = Rect::new(a.x, topo, a.w, (a.y + a.h - 46.0 * f - topo).max(alt));
        let cabem = ((lista.h / alt).floor() as usize).max(1);
        self.rolagem = self.rolagem.min(self.cartas.len().saturating_sub(cabem));
        if self.cartas.is_empty() {
            estilo::texto(
                lista.x,
                lista.y + 30.0 * f,
                "Nenhuma entrega esperando.",
                15,
                estilo::SUAVE,
            );
        }
        for (i, carta) in self
            .cartas
            .iter()
            .enumerate()
            .skip(self.rolagem)
            .take(cabem)
        {
            let r = Rect::new(
                lista.x,
                lista.y + (i - self.rolagem) as f32 * alt,
                lista.w,
                alt - 6.0 * f,
            );
            estilo::cartao(r, false, false);
            let icone = Rect::new(r.x + 6.0 * f, r.y + 4.0 * f, r.h - 8.0 * f, r.h - 8.0 * f);
            let mut partes = Vec::new();
            if carta.item_id != 0 {
                crate::icones::icone_com_3d(
                    carta.item_id,
                    icone,
                    carta.instancia.map(|x| x.rarity),
                    None,
                    Some((c.vox, c.solido)),
                );
                partes.push(format!(
                    "{} ×{}",
                    c.nomes
                        .get(&carta.item_id)
                        .cloned()
                        .unwrap_or_else(|| format!("Item {}", carta.item_id)),
                    milhar(carta.qtd)
                ));
            }
            if carta.gold > 0 {
                partes.push(format!("{} gold", milhar(carta.gold)));
            }
            let x = icone.x + icone.w + 10.0 * f;
            estilo::texto_ajustado(
                &partes.join(" + "),
                x,
                r.y + 22.0 * f,
                r.w * 0.5,
                16,
                estilo::TEXTO,
            );
            estilo::texto_ajustado(
                &carta.motivo,
                x,
                r.y + 40.0 * f,
                r.w - (x - r.x) - 12.0 * f,
                12,
                estilo::SUAVE,
            );
        }
        self.rolar(
            Rect::new(a.x, a.y + a.h - 40.0 * f, 200.0 * f, 38.0 * f),
            self.cartas.len(),
            cabem,
            livre,
            f,
        );
    }

    // ─────────────────────────────── TP ───────────────────────────────

    fn aba_tp(
        &mut self,
        a: Rect,
        c: &Contexto,
        f: f32,
        livre: bool,
        saida: &mut Vec<ClientMessage>,
    ) {
        let esq = Rect::new(a.x, a.y, a.w * 0.56, a.h);
        let dir = Rect::new(a.x + a.w * 0.59, a.y, a.w * 0.41, a.h);
        estilo::texto_forte(
            esq.x,
            esq.y + 14.0 * f,
            "TP à venda (gold por TP)",
            14,
            estilo::SUAVE,
        );
        let alt = 58.0 * f;
        let lista = Rect::new(esq.x, esq.y + 26.0 * f, esq.w, esq.h - 26.0 * f - 46.0 * f);
        let cabem = ((lista.h / alt).floor() as usize).max(1);
        self.rolagem = self.rolagem.min(self.lista_tp.len().saturating_sub(cabem));
        if self.lista_tp.is_empty() {
            estilo::texto(
                lista.x,
                lista.y + 30.0 * f,
                "Ninguém vendendo TP agora.",
                15,
                estilo::SUAVE,
            );
        }
        let mut comprar = None;
        for (i, an) in self
            .lista_tp
            .iter()
            .enumerate()
            .skip(self.rolagem)
            .take(cabem)
        {
            let r = Rect::new(
                lista.x,
                lista.y + (i - self.rolagem) as f32 * alt,
                lista.w,
                alt - 6.0 * f,
            );
            if linha_de_anuncio(
                r,
                an,
                c,
                f,
                livre,
                if an.meu { "Seu" } else { "Comprar" },
                !an.meu,
            ) {
                comprar = Some(an.clone());
            }
        }
        if let Some(an) = comprar {
            self.compra = Some(Compra {
                anuncio: an,
                qtd: 1,
            });
        }
        self.rolar(
            Rect::new(esq.x, a.y + a.h - 40.0 * f, 200.0 * f, 38.0 * f),
            self.lista_tp.len(),
            cabem,
            livre,
            f,
        );

        estilo::painel(dir);
        let (x, w) = (dir.x + 16.0 * f, dir.w - 32.0 * f);
        let saldo = self.tp.unwrap_or(0);
        let rotulo = "Seus TP";
        estilo::texto_forte(x, dir.y + 32.0 * f, rotulo, 18, estilo::SUAVE);
        estilo::valor_tp(
            x + estilo::medir_forte(rotulo, 18) + 10.0 * f,
            dir.y + 32.0 * f,
            saldo,
            18,
            estilo::OURO,
        );
        estilo::texto_ajustado(
            "TP é da conta e vale em todos os servidores.",
            x,
            dir.y + 54.0 * f,
            w,
            12,
            estilo::SUAVE,
        );
        let mut y = dir.y + 92.0 * f;
        if c.nivel < regras::NIVEL_PARA_VENDER {
            estilo::texto_ajustado(
                &format!("Vender TP libera no nível {}.", regras::NIVEL_PARA_VENDER),
                x,
                y,
                w,
                14,
                estilo::VERMELHO,
            );
            y += 26.0 * f;
        }
        if self.tp_qtd == 0 {
            self.tp_qtd = 1;
        }
        if self.tp_preco == 0 {
            self.tp_preco = 100;
        }
        seletor(
            Rect::new(x, y, w, 42.0 * f),
            "TP a vender",
            &mut self.tp_qtd,
            1,
            saldo.clamp(1, regras::TP_MAX_POR_ANUNCIO),
            false,
            f,
        );
        y += 66.0 * f;
        seletor(
            Rect::new(x, y, w, 42.0 * f),
            "Gold por TP",
            &mut self.tp_preco,
            1,
            regras::PRECO_MAX_UNIT,
            true,
            f,
        );
        y += 60.0 * f;
        let bruto = regras::total(self.tp_qtd, self.tp_preco).unwrap_or(0);
        linha_valor(x, y, w, "Total", &milhar(bruto), estilo::TEXTO, f);
        linha_valor(
            x,
            y + 22.0 * f,
            w,
            &format!("Taxa {}%", regras::TAXA_PCT),
            &format!("−{}", milhar(regras::taxa(bruto))),
            estilo::SUAVE,
            f,
        );
        linha_valor(
            x,
            y + 44.0 * f,
            w,
            "Você recebe",
            &milhar(regras::liquido(bruto)),
            estilo::OURO,
            f,
        );
        let bt = Rect::new(x, dir.y + dir.h - 58.0 * f, w, 44.0 * f);
        let pode = livre
            && c.nivel >= regras::NIVEL_PARA_VENDER
            && saldo >= self.tp_qtd
            && self.meus.len() < regras::MAX_ANUNCIOS;
        if botao(bt, "Anunciar TP", pode, true) {
            saida.push(ClientMessage::MercadoAnunciarTp {
                qtd: self.tp_qtd,
                preco_unit: self.tp_preco,
            });
        }
    }
}

// ─────────────────────────────── pecas ───────────────────────────────

fn botao(r: Rect, rotulo: &str, ativo: bool, primario: bool) -> bool {
    let m = Vec2::from(mouse_position());
    let sobre = ativo && r.contains(m);
    estilo::botao(
        r,
        rotulo,
        estilo::estado(
            sobre,
            sobre && is_mouse_button_down(MouseButton::Left),
            !ativo,
            false,
        ),
        primario,
    );
    sobre && crate::foco::clique()
}

fn linha_valor(x: f32, y: f32, w: f32, rotulo: &str, valor: &str, cor: Color, _f: f32) {
    estilo::texto(x, y, rotulo, 14, estilo::SUAVE);
    estilo::texto_forte(x + w - estilo::medir_forte(valor, 15), y, valor, 15, cor);
}

fn nome_do_anuncio(an: &AnuncioNet, c: &Contexto) -> String {
    if an.tipo == regras::TIPO_TP {
        return "Tempest Points".into();
    }
    let base = c
        .nomes
        .get(&an.item_id)
        .cloned()
        .unwrap_or_else(|| an.nome.clone());
    match an.instancia.map(|i| i.refinement).filter(|r| *r > 0) {
        Some(r) => format!("{base} +{r}"),
        None => base,
    }
}

/// The icon's square on a listing row.
fn icone_do_anuncio(r: Rect, f: f32) -> Rect {
    Rect::new(r.x + 6.0 * f, r.y + 4.0 * f, r.h - 8.0 * f, r.h - 8.0 * f)
}

/// A listing row with a button on the right. `true` on the button's click.
fn linha_de_anuncio(
    r: Rect,
    an: &AnuncioNet,
    c: &Contexto,
    f: f32,
    livre: bool,
    rotulo: &str,
    ativo: bool,
) -> bool {
    estilo::cartao(r, false, an.meu);
    let icone = icone_do_anuncio(r, f);
    let x = if an.tipo == regras::TIPO_TP {
        estilo::icone_tp(icone.center(), icone.w.min(icone.h) * 0.95);
        icone.x + icone.w + 10.0 * f
    } else {
        crate::icones::icone_com_3d(
            an.item_id,
            icone,
            an.instancia.map(|i| i.rarity),
            None,
            Some((c.vox, c.solido)),
        );
        icone.x + icone.w + 10.0 * f
    };
    let bw = 116.0 * f;
    let preco = format!("{} gold", milhar(an.preco_unit));
    let qtd = format!("×{}", milhar(an.qtd));
    let xp = r.x + r.w - bw - 16.0 * f - estilo::medir_forte(&preco, 16);
    let largura_nome = (xp - x - 90.0 * f).max(60.0 * f);
    estilo::texto_ajustado(
        &nome_do_anuncio(an, c),
        x,
        r.y + 22.0 * f,
        largura_nome,
        16,
        estilo::TEXTO,
    );
    estilo::texto_ajustado(
        &format!("{} · {}", an.realm, an.vendedor),
        x,
        r.y + 40.0 * f,
        largura_nome,
        12,
        estilo::SUAVE,
    );
    estilo::texto(xp - 70.0 * f, r.y + 32.0 * f, &qtd, 15, estilo::TEXTO);
    estilo::texto_forte(xp, r.y + 32.0 * f, &preco, 16, estilo::OURO);
    let b = Rect::new(
        r.x + r.w - bw - 6.0 * f,
        r.y + (r.h - 38.0 * f) * 0.5,
        bw,
        38.0 * f,
    );
    botao(b, rotulo, livre && ativo, ativo && rotulo != "Cancelar")
}

/// A number by touch: [Min][-] value [+][Max] for quantity; [/10][-] value
/// [+][x10] for price (+- moves 10% of the value, at least 1).
fn seletor(r: Rect, rotulo: &str, valor: &mut u64, min: u64, max: u64, preco: bool, f: f32) {
    estilo::texto(r.x, r.y - 6.0 * f, rotulo, 13, estilo::SUAVE);
    let bw = (52.0 * f).min(r.w / 6.0);
    let rotulos = if preco {
        ["÷10", "−", "+", "×10"]
    } else {
        ["Mín", "−", "+", "Máx"]
    };
    let rects = [
        Rect::new(r.x, r.y, bw, r.h),
        Rect::new(r.x + bw + 4.0 * f, r.y, bw, r.h),
        Rect::new(r.x + r.w - 2.0 * bw - 4.0 * f, r.y, bw, r.h),
        Rect::new(r.x + r.w - bw, r.y, bw, r.h),
    ];
    let v = *valor;
    let passo = (v / 10).max(1);
    for (i, b) in rects.iter().enumerate() {
        if botao(*b, rotulos[i], true, false) {
            *valor = match (preco, i) {
                (true, 0) => v / 10,
                (false, 0) => min,
                (true, 1) => v.saturating_sub(passo),
                (false, 1) => v.saturating_sub(1),
                (true, 2) => v.saturating_add(passo),
                (false, 2) => v.saturating_add(1),
                (true, _) => v.saturating_mul(10),
                (false, _) => max,
            };
        }
    }
    *valor = (*valor).clamp(min, max.max(min));
    let meio = Rect::new(
        rects[1].x + bw + 4.0 * f,
        r.y,
        (rects[2].x - 4.0 * f) - (rects[1].x + bw + 4.0 * f),
        r.h,
    );
    estilo::ret_arredondado(meio, estilo::RAIO_PEQUENO, estilo::FUNDO_BAIXO);
    let t = milhar(*valor);
    estilo::texto_centro_forte(
        meio.center().x,
        meio.center().y + 6.0 * f,
        &t,
        17,
        estilo::TEXTO,
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    fn anuncio(tipo: u8) -> AnuncioNet {
        AnuncioNet {
            id: "a".into(),
            tipo,
            item_id: 60,
            nome: "Madeira".into(),
            categoria: 2,
            instancia: None,
            qtd: 5,
            preco_unit: 10,
            realm: "SA01".into(),
            vendedor: "Ana".into(),
            meu: false,
        }
    }

    #[test]
    fn resposta_de_tp_nao_apaga_a_lista_de_itens() {
        let mut m = Mercado::default();
        m.lista(vec![anuncio(regras::TIPO_ITEM)], 0, false);
        m.lista(vec![anuncio(regras::TIPO_TP)], 0, false);
        assert_eq!((m.lista.len(), m.lista_tp.len()), (1, 1));
    }

    #[test]
    fn abrir_pede_a_busca_e_as_entregas() {
        let mut m = Mercado::default();
        let pedidos = m.abrir();
        assert!(matches!(pedidos[0], ClientMessage::MercadoBuscar { .. }));
        assert!(pedidos
            .iter()
            .any(|p| matches!(p, ClientMessage::MercadoEntregas)));
        assert!(m.aberto);
    }

    #[test]
    fn trocar_de_aba_pede_o_que_ela_mostra() {
        let mut m = Mercado::default();
        m.abrir();
        assert!(matches!(
            m.trocar_aba(Aba::Entregas)[..],
            [ClientMessage::MercadoEntregas]
        ));
        assert!(
            m.trocar_aba(Aba::Entregas).is_empty(),
            "a mesma aba nao pede de novo"
        );
        assert_eq!(m.trocar_aba(Aba::Tp).len(), 2);
    }
}
