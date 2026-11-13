//! Menu -> Trade -> Market: the global market (docs/MERCADO.md), in the MIR4
//! mould. One market for every server.
//!
//! The client only ASKS. Level, binding, gold, escrow and fee are decided on
//! the server; the fee maths here is the same function (`shared::mercado`),
//! only to show before confirming. Everything by touch: quantity and price
//! are buttons, not typing — only the search uses the keyboard.
#[allow(unused_imports)]
use crate::ui::Toque as _;
use std::collections::{HashMap, HashSet};

use macroquad::prelude::*;
use shared::mercado::{self as regras, AnuncioNet, CartaNet, Categoria, FiltroNet, PedidoNet, VendaNet};
use shared::protocol::ClientMessage;
use shared::InventorySlot;

use crate::economia::milhar;
use crate::hud_estilo as estilo;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Aba {
    #[default]
    Comprar,
    Vender,
    /// Buy orders: "I want N of X at P each" (`regras::preencher_pedido`).
    Pedidos,
    Meus,
    Entregas,
    Tp,
    Energia,
}

const ABAS: [(Aba, &str); 7] = [
    (Aba::Comprar, "Buy"),
    (Aba::Vender, "Sell"),
    (Aba::Pedidos, "Buy orders"),
    (Aba::Meus, "My listings"),
    (Aba::Entregas, "Deliveries"),
    (Aba::Tp, "TP"),
    (Aba::Energia, "Energy"),
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
    /// The character's Energy (the Energy tab sells it in lots of 1,000).
    pub energia: u64,
    /// The on-screen keyboard is up (phones): a number being typed raises
    /// the panel above it (`teclado_virtual::deslocamento`).
    pub teclado: bool,
}

/// A purchase being confirmed.
struct Compra {
    anuncio: AnuncioNet,
    qtd: u64,
}

/// What the Buy orders tab lists on the left.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum ListaDePedidos {
    #[default]
    Todos,
    Meus,
}

/// The Buy orders tab: browsing, selling to one, and the new-order form.
#[derive(Default)]
struct Pedidos {
    lista: Vec<PedidoNet>,
    meus: Vec<PedidoNet>,
    modo: ListaDePedidos,
    busca: String,
    foco_busca: bool,
    /// The order being sold to, and how many.
    venda: Option<(PedidoNet, u64)>,
    /// New order: the item picked, the item search, quantity and price.
    item: Option<u16>,
    busca_item: String,
    foco_item: bool,
    /// The item picker's results, scrolled.
    rolagem_item: crate::rolagem::Rolagem,
    qtd: u64,
    preco: u64,
}

#[derive(Default)]
pub struct Mercado {
    pub aberto: bool,
    /// When to ask the server again by itself (owner, 10/11/2026: "the
    /// market doesn't update by itself when something comes in or goes out,
    /// nor the red dot of the deliveries"): the open tab every
    /// `ATUALIZA_ABERTO_S`, the deliveries every `ATUALIZA_FECHADO_S` while closed.
    proxima_atualizacao: f64,
    /// Last refresh asked because of a server push (at most one per second).
    ultimo_empurrao: f64,
    /// Just bought: the Deliveries dot shows right away, before the server's
    /// list confirms it (owner, 10/11/2026: "the red dot has to appear at the
    /// moment of the purchase").
    entrega_ate: f64,
    /// An item's magnifier asked for "Where to get" (main opens the popup).
    pub onde_obter: Option<u16>,
    /// Bound items (`ItemsConfig`): they do not show up to sell.
    pub vinculados: HashSet<u16>,
    /// Items with an equipment slot (`ItemsConfig`): buy orders skip them.
    pub equipaveis: HashSet<u16>,
    /// The server's recommended prices (`shared::precos`), gold per unit.
    pub precos: std::collections::HashMap<u16, f64>,
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
    /// Listing-slot purchases this character made (`regras::max_anuncios`).
    compras_vagas: u8,
    compra: Option<Compra>,
    venda_slot: Option<usize>,
    venda_qtd: u64,
    venda_preco: u64,
    tp_qtd: u64,
    tp_preco: u64,
    energia_lotes: u64,
    energia_preco: u64,
    /// (text, ok, when)
    aviso: Option<(String, bool, f64)>,
    /// The open tab's list, scrolled by finger (with glide), wheel and bar.
    rolagem: crate::rolagem::Rolagem,
    /// The category tabs, swiped sideways when they do not fit (phones).
    rolagem_categorias: crate::rolagem::Rolagem,
    rolagem_venda: crate::rolagem::Rolagem,
    pedidos: Pedidos,
    /// The number being typed into a `seletor`, if any.
    edicao: Edicao,
    /// How far the panel is raised above the on-screen keyboard.
    subida: f32,
}

fn filtro_tp() -> FiltroNet {
    FiltroNet {
        categoria: Categoria::Tp as u8,
        texto: String::new(),
        pagina: 0,
    }
}

/// The open market refreshes its tab and the deliveries this often.
const ATUALIZA_ABERTO_S: f64 = 30.0;
/// Closed, only the deliveries (for the red dots), this often.
const ATUALIZA_FECHADO_S: f64 = 60.0;

impl Mercado {
    /// `ServerMessage::MercadoMudou`, pushed the moment a listing comes in,
    /// sells or is cancelled (owner: "can't it be websocket, to be instant?"):
    /// refresh what is on screen now; `meu` also checks the deliveries.
    /// The timed refresh below is only the safety net.
    pub fn mudou(&mut self, meu: bool, agora: f64) -> Vec<ClientMessage> {
        if !self.aberto && !meu {
            return Vec::new();
        }
        if agora - self.ultimo_empurrao < 1.0 {
            self.proxima_atualizacao = self.proxima_atualizacao.min(agora + 1.0);
            return Vec::new();
        }
        self.ultimo_empurrao = agora;
        self.proxima_atualizacao = f64::MIN;
        self.passo(agora)
    }

    /// Once per frame in the world: the safety-net refresh.
    pub fn passo(&mut self, agora: f64) -> Vec<ClientMessage> {
        if agora < self.proxima_atualizacao {
            return Vec::new();
        }
        if self.aberto {
            self.proxima_atualizacao = agora + ATUALIZA_ABERTO_S;
            let mut v = self.pedidos_da_aba();
            if self.aba != Aba::Entregas {
                v.push(ClientMessage::MercadoEntregas);
            }
            v
        } else {
            self.proxima_atualizacao = agora + ATUALIZA_FECHADO_S;
            vec![ClientMessage::MercadoEntregas]
        }
    }

    /// Something waits in the market's Deliveries (the Menu's red dot).
    pub fn tem_entregas(&self) -> bool {
        !self.cartas.is_empty() || get_time() < self.entrega_ate
    }

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
        self.rolagem.zera();
        self.pedidos_da_aba()
    }

    /// Opens on the Buy tab already searching `texto` (the "Search" from Where to get).
    pub fn abrir_buscando(&mut self, texto: &str) -> Vec<ClientMessage> {
        self.busca = texto.to_string();
        self.categoria = Categoria::Todas as u8;
        self.pagina = 0;
        self.rolagem.zera();
        self.abrir()
    }

    pub fn fechar(&mut self) {
        self.aberto = false;
        self.sem_foco();
        self.compra = None;
    }

    fn sem_foco(&mut self) {
        self.foco_busca = false;
        self.pedidos.foco_busca = false;
        self.pedidos.foco_item = false;
        self.edicao = Edicao::default();
    }

    /// The on-screen keyboard opens with a search field focused, or a number
    /// being typed.
    pub fn foco_na_busca(&self) -> bool {
        self.aberto
            && (self.foco_busca || self.pedidos.foco_busca || self.pedidos.foco_item || self.edicao.ativa())
    }

    /// Escape: cancels the number being typed (the old value stays); with
    /// nothing typed, closes the market.
    pub fn esc(&mut self) {
        if self.edicao.ativa() {
            self.edicao = Edicao::default();
        } else {
            self.fechar();
        }
    }

    /// End of the frame: an edit whose field was not drawn is dropped, and a
    /// tap on a number starts its edit (the search fields lose the keyboard).
    fn fim_da_edicao(&mut self) {
        if self.edicao.ativa() && !self.edicao.visto {
            self.edicao = Edicao::default();
        }
        if let Some((id, fundo)) = self.edicao.pedido.take() {
            self.sem_foco();
            self.edicao.id = Some(id);
            self.edicao.fundo = fundo;
        }
        self.edicao.visto = false;
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
            Aba::Pedidos => vec![
                ClientMessage::MercadoBuscarPedidos {
                    item_id: None,
                    texto: self.pedidos.busca.trim().to_string(),
                },
                ClientMessage::MercadoMeusPedidos,
            ],
            Aba::Entregas => vec![ClientMessage::MercadoEntregas],
            Aba::Energia => {
                self.ultima_busca_tp = false;
                vec![
                    ClientMessage::MercadoBuscar {
                        filtro: FiltroNet {
                            categoria: Categoria::Consumivel as u8,
                            texto: "Energy".into(),
                            pagina: 0,
                        },
                    },
                    ClientMessage::MercadoMeus,
                ]
            }
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

    pub fn meus(&mut self, anuncios: Vec<AnuncioNet>, historico: Vec<VendaNet>, tp: u64, compras_vagas: u8) {
        self.meus = anuncios;
        self.historico = historico;
        self.tp = Some(tp);
        self.compras_vagas = compras_vagas;
    }

    /// Active listings this character may keep.
    fn max_anuncios(&self) -> usize {
        regras::max_anuncios(self.compras_vagas)
    }

    /// `ServerMessage::MercadoPedidos`: a search answer or my open orders.
    pub fn pedidos(&mut self, lista: Vec<PedidoNet>, meus: bool) {
        if meus {
            self.pedidos.meus = lista;
        } else {
            self.pedidos.lista = lista;
        }
    }

    pub fn entregas(&mut self, cartas: Vec<CartaNet>, tp: u64) {
        self.cartas = cartas;
        self.tp = Some(tp);
    }

    /// Resposta de um pedido: mostra e atualiza a aba aberta.
    pub fn resultado(&mut self, ok: bool, texto: String, agora: f64) -> Vec<ClientMessage> {
        self.aviso = Some((texto, ok, agora));
        if !ok {
            return Vec::new();
        }
        // Closed: a sale closing (your item was bought) leaves a delivery —
        // ask for the deliveries so the red dot shows without opening.
        if !self.aberto {
            return vec![ClientMessage::MercadoEntregas];
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
        self.rolagem.zera();
        self.rolagem_venda.zera();
        self.sem_foco();
        self.compra = None;
        self.pedidos.venda = None;
        self.pedidos_da_aba()
    }

    /// Desenha e devolve os pedidos do quadro.
    pub fn desenha(&mut self, c: &Contexto, agora: f64) -> Vec<ClientMessage> {
        estilo::no_painel(estilo::escala_do_painel(980.0, 660.0), || {
            self.desenha_na_escala(c, agora)
        })
    }

    fn desenha_na_escala(&mut self, c: &Contexto, agora: f64) -> Vec<ClientMessage> {
        if !self.aberto {
            return Vec::new();
        }
        let saida = self.desenha_aberto(c, agora);
        self.fim_da_edicao();
        saida
    }

    fn desenha_aberto(&mut self, c: &Contexto, agora: f64) -> Vec<ClientMessage> {
        let mut saida = Vec::new();
        crate::hud_layout::escurece(0.55);
        let f = estilo::fator_texto();
        let seguro = crate::hud_layout::tela_segura();
        let (w, h) = (
            (980.0 * f).min(seguro.w - 16.0),
            (660.0 * f).min(seguro.h - 16.0),
        );
        let mut p = Rect::new(
            seguro.center().x - w * 0.5,
            seguro.center().y - h * 0.5,
            w,
            h,
        );
        // A number being typed on a phone: the panel rises so the field sits
        // above the on-screen keyboard (same rule as the character name).
        // `fundo` was measured last frame, already raised by `subida`.
        self.subida = if self.edicao.ativa() {
            crate::teclado_virtual::deslocamento(c.teclado, screen_height(), p.y, self.edicao.fundo + self.subida)
        } else {
            0.0
        };
        p.y -= self.subida;
        estilo::painel(p);
        estilo::texto(p.x + 18.0 * f, p.y + 34.0 * f, "Market", 22, estilo::OURO);
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
            let sobre = livre && r.toca(m);
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
            Aba::Pedidos => self.aba_pedidos(area, c, f, livre, &mut saida),
            Aba::Meus => self.aba_meus(area, c, f, livre, &mut saida),
            Aba::Entregas => self.aba_entregas(area, c, f, livre, &mut saida),
            Aba::Tp => self.aba_tp(area, c, f, livre, &mut saida),
            Aba::Energia => self.aba_energia(area, c, f, livre, &mut saida),
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
        // Categories. A tab is never narrower than its name: when they do not
        // fit their strip (a phone), the row swipes sideways.
        let faixa = Rect::new(a.x, a.y, a.w * 0.6, 38.0 * f);
        let n = Categoria::FILTROS.len();
        let maior = Categoria::FILTROS
            .iter()
            .map(|c| estilo::medir(c.nome(), 14))
            .fold(0.0_f32, f32::max);
        let cw = (faixa.w / n as f32).min(150.0 * f).max(maior + 24.0 * f);
        let largura = cw * n as f32;
        let toque_cat = if livre {
            self.rolagem_categorias
                .passo_horizontal(faixa, largura, cw, crate::rolagem::Entrada::agora())
        } else {
            None
        };
        crate::rolagem::recortar(Some(faixa));
        let arrastando = self.rolagem_categorias.arrastando();
        let mut escolhida = None;
        for (i, cat) in Categoria::FILTROS.iter().enumerate() {
            let r = Rect::new(
                faixa.x + i as f32 * cw - self.rolagem_categorias.pos,
                a.y,
                cw - 6.0 * f,
                38.0 * f,
            );
            let sel = self.categoria == *cat as u8;
            estilo::cartao(r, livre && !arrastando && r.toca(m) && faixa.toca(m), sel);
            estilo::texto_centro(
                r.center().x,
                r.center().y + 5.0 * f,
                cat.nome(),
                14,
                if sel { estilo::OURO } else { estilo::TEXTO },
            );
            if toque_cat.is_some_and(|p| r.toca(p)) && !sel {
                escolhida = Some(*cat as u8);
            }
        }
        crate::rolagem::recortar(None);
        if let Some(cat) = escolhida {
            self.categoria = cat;
            self.pagina = 0;
            self.rolagem.zera();
            saida.push(self.buscar());
        }
        // Busca.
        let xb = faixa.x + faixa.w.min(largura) + 8.0 * f;
        let bb = Rect::new(a.x + a.w - 104.0 * f, a.y, 104.0 * f, 38.0 * f);
        let campo = Rect::new(xb, a.y, (bb.x - 8.0 * f - xb).max(80.0 * f), 38.0 * f);
        let digitado: &[char] = if self.foco_busca { c.digitado } else { &[] };
        if crate::ui::campo(campo, "", &mut self.busca, self.foco_busca, false, digitado) && livre {
            self.foco_busca = true;
        } else if clicou && !campo.toca(m) {
            self.foco_busca = false;
        }
        if self.busca.chars().count() > regras::BUSCA_MAX_CHARS {
            self.busca = self.busca.chars().take(regras::BUSCA_MAX_CHARS).collect();
        }
        if self.busca.is_empty() && !self.foco_busca {
            estilo::texto(
                campo.x + 12.0 * f,
                campo.center().y + 5.0 * f,
                "Search item…",
                14,
                estilo::SUAVE,
            );
        }
        let enter = self.foco_busca
            && (is_key_pressed(KeyCode::Enter)
                || c.digitado.iter().any(|ch| *ch == '\r' || *ch == '\n'));
        if botao(bb, "Search", livre, true) || enter {
            self.pagina = 0;
            self.rolagem.zera();
            self.foco_busca = false;
            saida.push(self.buscar());
        }

        // Lista.
        let lista = Rect::new(a.x, a.y + 50.0 * f, a.w, a.h - 50.0 * f - 46.0 * f);
        if self.lista.is_empty() {
            estilo::texto(
                lista.x + 4.0,
                lista.y + 30.0 * f,
                "No listing found.",
                15,
                estilo::SUAVE,
            );
        }
        let alt = 58.0 * f;
        let toque = abre_lista(&mut self.rolagem, lista, self.lista.len(), alt, livre);
        let mut comprar = None;
        let mut onde = None;
        for (i, an) in self.lista.iter().enumerate() {
            let Some(r) = linha_da_lista(&self.rolagem, lista, i, alt, f) else {
                continue;
            };
            if linha_de_anuncio(
                r,
                an,
                c,
                f,
                livre,
                if an.meu { "Yours" } else { "Buy" },
                !an.meu,
                toque,
            ) {
                comprar = Some(an.clone());
            }
            // Touching the item's icon: "Where to get" (TP has none).
            if an.tipo == regras::TIPO_ITEM {
                let icone = icone_do_anuncio(r, f);
                let _ = crate::onde_obter::lupa_no_icone(icone);
                if livre && toque.is_some_and(|p| icone.toca(p)) {
                    onde = Some(an.item_id);
                }
            }
        }
        fecha_lista(&self.rolagem, lista, self.lista.len(), alt);
        if onde.is_some() {
            self.onde_obter = onde;
        }
        if let Some(an) = comprar {
            self.compra = Some(Compra {
                anuncio: an,
                qtd: 1,
            });
        }
        // Footer: the server's pages (the list itself scrolls).
        let yb = a.y + a.h - 40.0 * f;
        let prox = Rect::new(a.x + a.w - 130.0 * f, yb, 130.0 * f, 38.0 * f);
        let ant = Rect::new(prox.x - 138.0 * f, yb, 130.0 * f, 38.0 * f);
        estilo::texto_centro(
            ant.x - 60.0 * f,
            yb + 25.0 * f,
            &format!("Page {}", self.pagina + 1),
            14,
            estilo::SUAVE,
        );
        if botao(ant, "‹ Previous", livre && self.pagina > 0, false) {
            self.pagina -= 1;
            self.rolagem.zera();
            saida.push(ClientMessage::MercadoBuscar {
                filtro: self.filtro(),
            });
            self.ultima_busca_tp = false;
        }
        if botao(prox, "Next ›", livre && self.tem_mais, false) {
            self.pagina += 1;
            self.rolagem.zera();
            saida.push(ClientMessage::MercadoBuscar {
                filtro: self.filtro(),
            });
            self.ultima_busca_tp = false;
        }
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
            seguro.center().y - h * 0.5 - self.subida,
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
            "Confirm purchase",
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
                "{} gold each · {} for sale",
                milhar(an.preco_unit),
                milhar(an.qtd)
            ),
            13,
            estilo::SUAVE,
        );
        y += 88.0 * f;
        let max = an.qtd.max(1);
        seletor(
            &mut self.edicao,
            "compra_qtd",
            c.digitado,
            Rect::new(r.x + 18.0 * f, y, r.w - 36.0 * f, 44.0 * f),
            "Quantity",
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
                "Not enough gold.",
                13,
                estilo::VERMELHO,
            );
        }
        let yb = r.y + r.h - 56.0 * f;
        let cancelar = Rect::new(r.x + 18.0 * f, yb, (r.w - 48.0 * f) * 0.5, 42.0 * f);
        let confirmar = Rect::new(cancelar.x + cancelar.w + 12.0 * f, yb, cancelar.w, 42.0 * f);
        if botao(cancelar, "Cancel", true, false) {
            self.compra = None;
            return;
        }
        if botao(confirmar, "Buy", cabe, true) {
            saida.push(ClientMessage::MercadoComprar {
                anuncio: an.id.clone(),
                qtd: compra.qtd,
                preco_unit: an.preco_unit,
            });
            // The buyer's own screen changes NOW: what was bought leaves the
            // list (the whole listing when it ran out) and the Deliveries dot
            // lights. The server's answer then refreshes both; a refusal puts
            // the real list back.
            for lista in [&mut self.lista, &mut self.lista_tp] {
                if let Some(i) = lista.iter().position(|a| a.id == an.id) {
                    lista[i].qtd = lista[i].qtd.saturating_sub(compra.qtd);
                    if lista[i].qtd == 0 {
                        lista.remove(i);
                    }
                }
            }
            self.entrega_ate = get_time() + 15.0;
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
        estilo::texto_forte(esq.x, esq.y + 14.0 * f, "From bag", 14, estilo::SUAVE);
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
            let t = format!("{presos} bound item(s) left out of the list");
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
                "Nothing sellable in your bag.",
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
            estilo::slot(r, None, livre && r.toca(m), sel);
            crate::icones::icone_com_3d(
                s.item_id,
                r,
                s.instance.map(|x| x.rarity),
                Some(s.qty),
                Some((c.vox, c.solido)),
            );
            if clique_na_grade.is_some_and(|p| r.toca(p)) {
                self.venda_slot = Some(*i);
                self.venda_qtd = 1;
                // The recommended price comes filled in; the seller can type
                // anything over it, 1 included.
                match shared::precos::recomendado(&self.precos, s.item_id, s.instance.as_ref()) {
                    Some(r) => self.venda_preco = r,
                    None if self.venda_preco == 0 => self.venda_preco = 100,
                    None => {}
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
            "Active listings {}/{}",
            self.meus.len(),
            self.max_anuncios()
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
                "Pick an item from your bag.",
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
        let nome = nome_com_degrau(&nome, slot.instance.as_ref());
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
            &mut self.edicao,
            "venda_qtd",
            c.digitado,
            Rect::new(x, y, w, 42.0 * f),
            "Quantity",
            &mut self.venda_qtd,
            1,
            slot.qty.max(1) as u64,
            false,
            f,
        );
        y += 66.0 * f;
        seletor(
            &mut self.edicao,
            "venda_preco",
            c.digitado,
            Rect::new(x, y, w, 42.0 * f),
            "Price per unit (gold)",
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
            &format!("Fee {}%", regras::TAXA_PCT),
            &format!("−{}", milhar(regras::taxa(bruto))),
            estilo::SUAVE,
            f,
        );
        linha_valor(
            x,
            y + 44.0 * f,
            w,
            "You receive",
            &milhar(regras::liquido(bruto)),
            estilo::OURO,
            f,
        );
        // THE MARKET'S OWN PRICE: the cheapest listing of the same thing. For
        // gear "the same" is the same colour, tier and refine — a +7 is not
        // priced by a +0 of the same sword. The search sorts by price, so
        // the page in hand holds the cheapest.
        let peca = shared::precos::e_peca_da_forja(slot.item_id);
        let igual = |an: &&AnuncioNet| {
            an.item_id == slot.item_id
                && an.tipo == regras::TIPO_ITEM
                && (!peca
                    || match (an.instancia.as_ref(), slot.instance.as_ref()) {
                        (Some(a), Some(b)) => (a.rarity, a.tier, a.refinement) == (b.rarity, b.tier, b.refinement),
                        _ => false,
                    })
        };
        let menor = self.lista.iter().filter(igual).map(|an| an.preco_unit).min();
        // BOTH REFERENCES, FOR EVERY ITEM: the recommended price and the
        // market's lowest. A missing one says so instead of vanishing, so the
        // seller always knows what was looked at.
        let recomendado = shared::precos::recomendado(&self.precos, slot.item_id, slot.instance.as_ref());
        let pode_listar = livre && pode && self.meus.len() < self.max_anuncios();
        referencias(
            &mut self.venda_preco,
            recomendado,
            menor,
            if peca { "Lowest (same piece): {} gold" } else { "Lowest on the market: {} gold" },
            (x, y, w, f),
            livre,
        );
        let bt = Rect::new(x, dir.y + dir.h - 58.0 * f, w, 44.0 * f);
        // THE BUTTONS SAY WHAT THEY DO, total included. The old pair read
        // "List" (dull) beside "Sell all ×N at X" (gold): sellers who set one
        // unit pressed the gold one and listed the whole stack, and took X
        // for the total when it was the price of each.
        let total_lista = regras::total(self.venda_qtd, self.venda_preco).unwrap_or(0);
        let rotulo = format!("List ×{} for {} gold", milhar(self.venda_qtd), milhar(total_lista));
        if botao(bt, &rotulo, pode_listar, true) {
            if let Some(i) = self.venda_slot {
                saida.push(ClientMessage::MercadoAnunciar {
                    inv_slot: i as u16,
                    qtd: self.venda_qtd.min(u32::MAX as u64) as u32,
                    preco_unit: self.venda_preco,
                });
                self.venda_slot = None;
            }
        }
        if let Some(rec) = recomendado {
            let r = Rect::new(bt.x, bt.y - 52.0 * f, bt.w, 42.0 * f);
            let total_tudo = regras::total(slot.qty as u64, rec).unwrap_or(0);
            let rotulo = if slot.qty > 1 {
                format!("Sell all ×{} for {} gold (recommended)", milhar(slot.qty as u64), milhar(total_tudo))
            } else {
                format!("Sell for {} gold (recommended)", milhar(total_tudo))
            };
            if botao(r, &rotulo, pode_listar, false) {
                if let Some(i) = self.venda_slot {
                    saida.push(ClientMessage::MercadoAnunciar {
                        inv_slot: i as u16,
                        qtd: slot.qty,
                        preco_unit: rec,
                    });
                    self.venda_slot = None;
                }
            }
        }
    }


    // ─────────────────────────────── Buy orders ───────────────────────────────

    /// BUY ORDERS: the open orders (or mine) on the left; on the right,
    /// selling to the one picked, or the form for a new order. The gold of an
    /// order leaves the buyer when it is placed; the seller's items leave the
    /// bag when sold; everything else arrives in Deliveries.
    fn aba_pedidos(&mut self, a: Rect, c: &Contexto, f: f32, livre: bool, saida: &mut Vec<ClientMessage>) {
        let m = Vec2::from(mouse_position());
        let clicou = crate::foco::clique();
        let esq = Rect::new(a.x, a.y, a.w * 0.56, a.h);
        let dir = Rect::new(a.x + a.w * 0.59, a.y, a.w * 0.41, a.h);

        // List: all orders / mine.
        let bw = 120.0 * f;
        let todos = Rect::new(esq.x, esq.y, bw, 38.0 * f);
        let meus = Rect::new(esq.x + bw + 6.0 * f, esq.y, bw + 20.0 * f, 38.0 * f);
        let rotulo_meus = format!("My orders {}/{}", self.pedidos.meus.len(), regras::MAX_PEDIDOS);
        for (r, modo, rotulo) in [
            (todos, ListaDePedidos::Todos, "All orders"),
            (meus, ListaDePedidos::Meus, rotulo_meus.as_str()),
        ] {
            let sel = self.pedidos.modo == modo;
            estilo::cartao(r, livre && r.toca(m), sel);
            estilo::texto_centro(r.center().x, r.center().y + 5.0 * f, rotulo, 14, if sel { estilo::OURO } else { estilo::TEXTO });
            if livre && clicou && r.toca(m) && !sel {
                self.pedidos.modo = modo;
                self.rolagem.zera();
                saida.extend(self.pedidos_da_aba());
            }
        }
        if self.pedidos.modo == ListaDePedidos::Todos {
            let bb = Rect::new(esq.x + esq.w - 90.0 * f, esq.y, 90.0 * f, 38.0 * f);
            let xb = meus.x + meus.w + 8.0 * f;
            let campo = Rect::new(xb, esq.y, (bb.x - 6.0 * f - xb).max(60.0 * f), 38.0 * f);
            let digitado: &[char] = if self.pedidos.foco_busca { c.digitado } else { &[] };
            if crate::ui::campo(campo, "", &mut self.pedidos.busca, self.pedidos.foco_busca, false, digitado) && livre {
                self.sem_foco();
                self.pedidos.foco_busca = true;
            } else if clicou && !campo.toca(m) {
                self.pedidos.foco_busca = false;
            }
            corta(&mut self.pedidos.busca);
            if self.pedidos.busca.is_empty() && !self.pedidos.foco_busca {
                estilo::texto(campo.x + 10.0 * f, campo.center().y + 5.0 * f, "Search…", 14, estilo::SUAVE);
            }
            let enter = self.pedidos.foco_busca
                && (is_key_pressed(KeyCode::Enter) || c.digitado.iter().any(|ch| *ch == '\r' || *ch == '\n'));
            if botao(bb, "Search", livre, true) || enter {
                self.pedidos.foco_busca = false;
                self.rolagem.zera();
                saida.extend(self.pedidos_da_aba());
            }
        }
        let alt = 58.0 * f;
        let lista = Rect::new(esq.x, esq.y + 50.0 * f, esq.w, esq.h - 50.0 * f);
        let total = match self.pedidos.modo {
            ListaDePedidos::Todos => self.pedidos.lista.len(),
            ListaDePedidos::Meus => self.pedidos.meus.len(),
        };
        let toque = abre_lista(&mut self.rolagem, lista, total, alt, livre);
        let mostrar: &[PedidoNet] = match self.pedidos.modo {
            ListaDePedidos::Todos => &self.pedidos.lista,
            ListaDePedidos::Meus => &self.pedidos.meus,
        };
        if mostrar.is_empty() {
            let t = match self.pedidos.modo {
                ListaDePedidos::Todos => "No open buy order.",
                ListaDePedidos::Meus => "You have no open buy order.",
            };
            estilo::texto(lista.x + 4.0, lista.y + 30.0 * f, t, 15, estilo::SUAVE);
        }
        let (mut vender, mut cancelar) = (None, None);
        for (i, pd) in mostrar.iter().enumerate() {
            let Some(r) = linha_da_lista(&self.rolagem, lista, i, alt, f) else {
                continue;
            };
            let (rotulo, ativo) = match self.pedidos.modo {
                ListaDePedidos::Meus => ("Cancel", true),
                ListaDePedidos::Todos if pd.meu => ("Yours", false),
                ListaDePedidos::Todos => ("Sell", tenho(c, pd.item_id) > 0),
            };
            if linha_de_pedido(r, pd, c, f, livre, rotulo, ativo, toque) {
                match self.pedidos.modo {
                    ListaDePedidos::Meus => cancelar = Some(pd.id.clone()),
                    ListaDePedidos::Todos => vender = Some(pd.clone()),
                }
            }
        }
        fecha_lista(&self.rolagem, lista, total, alt);
        if let Some(pd) = vender {
            self.sem_foco();
            self.pedidos.venda = Some((pd, 1));
        }
        if let Some(pedido) = cancelar {
            saida.push(ClientMessage::MercadoCancelarPedido { pedido });
        }

        estilo::painel(dir);
        if self.pedidos.venda.is_some() {
            self.vender_ao_pedido(dir, c, f, livre, saida);
        } else {
            self.novo_pedido(dir, c, f, livre, saida);
        }
    }

    /// Right panel: selling to the order picked on the left.
    fn vender_ao_pedido(&mut self, dir: Rect, c: &Contexto, f: f32, livre: bool, saida: &mut Vec<ClientMessage>) {
        let Some((pd, qtd)) = &mut self.pedidos.venda else {
            return;
        };
        let (x, w) = (dir.x + 16.0 * f, dir.w - 32.0 * f);
        let mut y = dir.y + 32.0 * f;
        estilo::texto_forte(x, y, "Sell to buy order", 18, estilo::OURO);
        y += 16.0 * f;
        crate::icones::icone_com_3d(pd.item_id, Rect::new(x, y, 48.0 * f, 48.0 * f), None, None, Some((c.vox, c.solido)));
        let xn = x + 58.0 * f;
        estilo::texto_ajustado(&nome_do_pedido(pd, c), xn, y + 20.0 * f, x + w - xn, 16, estilo::TEXTO);
        estilo::texto_ajustado(
            &format!("{} gold each · wants {}", milhar(pd.preco_unit), milhar(pd.qtd)),
            xn,
            y + 40.0 * f,
            x + w - xn,
            13,
            estilo::SUAVE,
        );
        y += 72.0 * f;
        let tem = tenho(c, pd.item_id);
        let energia = pd.item_id == shared::item_id::ENERGIA_MIL;
        let tem_txt = if energia {
            format!("You have {} lots of 1,000 Energy", milhar(tem))
        } else {
            format!("You have {} in your bag", milhar(tem))
        };
        estilo::texto(x, y, &tem_txt, 14, if tem > 0 { estilo::TEXTO } else { estilo::VERMELHO });
        y += 34.0 * f;
        let max = tem.min(pd.qtd).max(1);
        seletor(&mut self.edicao, "pedido_venda_qtd", c.digitado, Rect::new(x, y, w, 42.0 * f), "Quantity", qtd, 1, max, false, f);
        y += 60.0 * f;
        let bruto = regras::total(*qtd, pd.preco_unit).unwrap_or(0);
        linha_valor(x, y, w, "Total", &milhar(bruto), estilo::TEXTO, f);
        linha_valor(x, y + 22.0 * f, w, &format!("Fee {}%", regras::TAXA_PCT), &format!("−{}", milhar(regras::taxa(bruto))), estilo::SUAVE, f);
        linha_valor(x, y + 44.0 * f, w, "You receive", &milhar(regras::liquido(bruto)), estilo::OURO, f);
        y += 74.0 * f;
        let nivel_ok = c.nivel >= regras::NIVEL_PARA_VENDER;
        if !nivel_ok {
            estilo::texto_ajustado(
                &format!("Selling on the market unlocks at level {}.", regras::NIVEL_PARA_VENDER),
                x,
                y,
                w,
                14,
                estilo::VERMELHO,
            );
        }
        let yb = dir.y + dir.h - 58.0 * f;
        let voltar = Rect::new(x, yb, 100.0 * f, 44.0 * f);
        let vende = Rect::new(voltar.x + voltar.w + 8.0 * f, yb, w - voltar.w - 8.0 * f, 44.0 * f);
        if botao(voltar, "Back", livre, false) {
            self.pedidos.venda = None;
            return;
        }
        let pode = livre && nivel_ok && tem >= *qtd && *qtd <= pd.qtd && !pd.meu;
        let rotulo = format!("Sell ×{} for {} gold", milhar(*qtd), milhar(regras::liquido(bruto)));
        if botao(vende, &rotulo, pode, true) {
            saida.push(ClientMessage::MercadoVenderAoPedido {
                pedido: pd.id.clone(),
                item_id: pd.item_id,
                qtd: *qtd,
            });
            // The seller's screen changes NOW (like a purchase): the order
            // wants fewer, and the Deliveries dot lights for the gold.
            let (id, vendida) = (pd.id.clone(), *qtd);
            if let Some(i) = self.pedidos.lista.iter().position(|p| p.id == id) {
                self.pedidos.lista[i].qtd = self.pedidos.lista[i].qtd.saturating_sub(vendida);
                if self.pedidos.lista[i].qtd == 0 {
                    self.pedidos.lista.remove(i);
                }
            }
            self.entrega_ate = get_time() + 15.0;
            self.pedidos.venda = None;
        }
    }

    /// Right panel: a new buy order — pick an item, the quantity and the
    /// price each; the total is held from your gold right away.
    fn novo_pedido(&mut self, dir: Rect, c: &Contexto, f: f32, livre: bool, saida: &mut Vec<ClientMessage>) {
        let m = Vec2::from(mouse_position());
        let clicou = crate::foco::clique();
        let (x, w) = (dir.x + 16.0 * f, dir.w - 32.0 * f);
        let mut y = dir.y + 32.0 * f;
        estilo::texto_forte(x, y, "New buy order", 18, estilo::OURO);
        estilo::texto_ajustado(
            "Materials, potions, keys, scrolls, Energy. The gold is held until it fills or you cancel.",
            x,
            y + 20.0 * f,
            w,
            12,
            estilo::SUAVE,
        );
        y += 44.0 * f;
        let Some(item) = self.pedidos.item else {
            // Picking the item: a search over what can be ordered.
            y += 14.0 * f;
            let campo = Rect::new(x, y, w, 38.0 * f);
            let digitado: &[char] = if self.pedidos.foco_item { c.digitado } else { &[] };
            let antes = self.pedidos.busca_item.len();
            let pediu = crate::ui::campo(campo, "Item", &mut self.pedidos.busca_item, self.pedidos.foco_item, false, digitado);
            if self.pedidos.busca_item.len() != antes {
                // A new search starts at the top.
                self.pedidos.rolagem_item.zera();
            }
            if pediu && livre {
                self.sem_foco();
                self.pedidos.foco_item = true;
            } else if clicou && !campo.toca(m) {
                self.pedidos.foco_item = false;
            }
            corta(&mut self.pedidos.busca_item);
            if self.pedidos.busca_item.is_empty() && !self.pedidos.foco_item {
                estilo::texto(campo.x + 10.0 * f, campo.center().y + 5.0 * f, "Type an item name…", 14, estilo::SUAVE);
            }
            y += 46.0 * f;
            let achados = catalogo_pedivel(c.nomes, &self.vinculados, &self.equipaveis, &self.pedidos.busca_item, MAX_ACHADOS);
            if achados.is_empty() && !self.pedidos.busca_item.trim().is_empty() {
                estilo::texto(x, y + 20.0 * f, "No orderable item with that name.", 14, estilo::SUAVE);
            }
            let alt = 40.0 * f;
            let area = Rect::new(x, y, w, (dir.y + dir.h - 8.0 * f - y).max(alt));
            let toque = abre_lista(&mut self.pedidos.rolagem_item, area, achados.len(), alt, livre);
            let arrastando = self.pedidos.rolagem_item.arrastando();
            let mut escolhido = None;
            for (k, (id, nome)) in achados.iter().enumerate() {
                let Some(mut r) = linha_da_lista(&self.pedidos.rolagem_item, area, k, alt, f) else {
                    continue;
                };
                r.h = alt - 4.0 * f;
                let sobre = livre && !arrastando && r.toca(m) && area.toca(m);
                estilo::cartao(r, sobre, false);
                crate::icones::icone_com_3d(*id, Rect::new(r.x + 4.0 * f, r.y + 2.0 * f, r.h - 4.0 * f, r.h - 4.0 * f), None, None, Some((c.vox, c.solido)));
                estilo::texto_ajustado(nome, r.x + r.h + 8.0 * f, r.center().y + 5.0 * f, r.w - r.h - 16.0 * f, 14, estilo::TEXTO);
                if toque.is_some_and(|p| r.toca(p)) {
                    escolhido = Some(*id);
                }
            }
            fecha_lista(&self.pedidos.rolagem_item, area, achados.len(), alt);
            if let Some(id) = escolhido {
                self.pedidos.item = Some(id);
                self.pedidos.foco_item = false;
                self.pedidos.qtd = 1;
                self.pedidos.preco = recomendado_unit(&self.precos, id).unwrap_or(self.pedidos.preco.max(1));
            }
            return;
        };
        let nome = if item == shared::item_id::ENERGIA_MIL {
            "Energy ×1,000".to_string()
        } else {
            c.nomes.get(&item).cloned().unwrap_or_else(|| format!("Item {item}"))
        };
        crate::icones::icone_com_3d(item, Rect::new(x, y, 44.0 * f, 44.0 * f), None, None, Some((c.vox, c.solido)));
        estilo::texto_ajustado(&nome, x + 54.0 * f, y + 28.0 * f, w - 54.0 * f - 90.0 * f, 16, estilo::TEXTO);
        if botao(Rect::new(x + w - 84.0 * f, y + 4.0 * f, 84.0 * f, 36.0 * f), "Change", livre, false) {
            self.pedidos.item = None;
            return;
        }
        y += 74.0 * f;
        // Quantity by tens too: orders are usually for stacks.
        seletor(&mut self.edicao, "pedido_qtd", c.digitado, Rect::new(x, y, w, 42.0 * f), "Quantity", &mut self.pedidos.qtd, 1, regras::QTD_MAX_POR_PEDIDO, true, f);
        y += 66.0 * f;
        seletor(&mut self.edicao, "pedido_preco", c.digitado, Rect::new(x, y, w, 42.0 * f), "Price each (gold)", &mut self.pedidos.preco, 1, regras::PRECO_MAX_UNIT, true, f);
        y += 60.0 * f;
        let total = regras::total(self.pedidos.qtd, self.pedidos.preco);
        let cabe = total.is_some_and(|t| t <= c.ouro);
        linha_valor(x, y, w, "Gold held now", &total.map_or("—".to_string(), milhar), if cabe { estilo::OURO } else { estilo::VERMELHO }, f);
        y += 30.0 * f;
        let recomendado = recomendado_unit(&self.precos, item);
        let maior = self.pedidos.lista.iter().filter(|p| p.item_id == item && !p.meu).map(|p| p.preco_unit).max();
        for (valor, com, sem, cor) in [
            (recomendado, "Recommended: {} gold", "Recommended: not enough data yet", estilo::OURO),
            (maior, "Best buy order now: {} gold", "No other buy order for this item", estilo::TEXTO),
        ] {
            match valor {
                Some(v) => {
                    estilo::texto_ajustado(&com.replace("{}", &milhar(v)), x, y, w - 80.0 * f, 14, cor);
                    let usar = Rect::new(x + w - 70.0 * f, y - 18.0 * f, 70.0 * f, 26.0 * f);
                    if self.pedidos.preco != v && botao(usar, "Use", livre, false) {
                        self.pedidos.preco = v;
                    }
                }
                None => estilo::texto_ajustado(sem, x, y, w, 14, estilo::SUAVE),
            }
            y += 32.0 * f;
        }
        let vagas = self.pedidos.meus.len() < regras::MAX_PEDIDOS;
        if !vagas {
            estilo::texto_ajustado(&regras::Recusa::MuitosPedidos.texto(), x, y, w, 13, estilo::VERMELHO);
        } else if !cabe {
            estilo::texto(x, y, "Not enough gold.", 13, estilo::VERMELHO);
        }
        let bt = Rect::new(x, dir.y + dir.h - 58.0 * f, w, 44.0 * f);
        let rotulo = format!("Place order ×{} at {} each", milhar(self.pedidos.qtd), milhar(self.pedidos.preco));
        if botao(bt, &rotulo, livre && cabe && vagas, true) {
            saida.push(ClientMessage::MercadoPedirCompra {
                item_id: item,
                qtd: self.pedidos.qtd,
                preco_unit: self.pedidos.preco,
            });
            self.pedidos.item = None;
            self.pedidos.busca_item.clear();
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
            &format!("Active ({}/{})", self.meus.len(), self.max_anuncios()),
            14,
            estilo::SUAVE,
        );
        let alt = 58.0 * f;
        let lista = Rect::new(esq.x, esq.y + 26.0 * f, esq.w, esq.h - 26.0 * f);
        let toque = abre_lista(&mut self.rolagem, lista, self.meus.len(), alt, livre);
        if self.meus.is_empty() {
            estilo::texto(
                lista.x,
                lista.y + 30.0 * f,
                "No active listing.",
                15,
                estilo::SUAVE,
            );
        }
        for (i, an) in self.meus.iter().enumerate() {
            let Some(r) = linha_da_lista(&self.rolagem, lista, i, alt, f) else {
                continue;
            };
            if linha_de_anuncio(r, an, c, f, livre, "Cancel", true, toque) {
                saida.push(ClientMessage::MercadoCancelar {
                    anuncio: an.id.clone(),
                });
            }
        }
        fecha_lista(&self.rolagem, lista, self.meus.len(), alt);

        estilo::painel(dir);
        let fim_vagas = self.vagas(dir, c, f, livre, saida);
        estilo::texto_forte(
            dir.x + 14.0 * f,
            fim_vagas + 26.0 * f,
            "History",
            15,
            estilo::SUAVE,
        );
        let mut y = fim_vagas + 52.0 * f;
        if self.historico.is_empty() {
            estilo::texto(dir.x + 14.0 * f, y, "Nothing yet.", 14, estilo::SUAVE);
        }
        for v in &self.historico {
            if y > dir.y + dir.h - 12.0 * f {
                break;
            }
            let (verbo, valor, cor) = if v.vendi {
                ("Sold", format!("+{}", milhar(v.liquido)), estilo::VERDE)
            } else {
                (
                    "Bought",
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

    /// The listing slots box at the top of `dir`: how many are in use and the
    /// next +5, in TP or in gold. Returns where the box ends.
    fn vagas(&self, dir: Rect, c: &Contexto, f: f32, livre: bool, saida: &mut Vec<ClientMessage>) -> f32 {
        let (x, w) = (dir.x + 14.0 * f, dir.w - 28.0 * f);
        let mut y = dir.y + 26.0 * f;
        estilo::texto_forte(x, y, "Listing slots", 15, estilo::SUAVE);
        let uso = format!("{} / {}", self.meus.len(), self.max_anuncios());
        estilo::texto_forte(x + w - estilo::medir_forte(&uso, 15), y, &uso, 15, estilo::OURO);
        y += 14.0 * f;
        match regras::proxima_vaga(self.compras_vagas) {
            Some((tp, ouro)) => {
                estilo::texto_ajustado(
                    &format!("Buy {} more slots with either:", regras::VAGAS_POR_COMPRA),
                    x,
                    y + 12.0 * f,
                    w,
                    13,
                    estilo::SUAVE,
                );
                y += 22.0 * f;
                let compra = self.compras_vagas + 1;
                let meio = 8.0 * f;
                let bw = (w - meio) / 2.0;
                let tem_tp = self.tp.is_some_and(|t| t >= tp);
                let r_tp = Rect::new(x, y, bw, 36.0 * f);
                if botao(r_tp, &format!("{} TP", milhar(tp)), livre && tem_tp, false) {
                    saida.push(ClientMessage::MercadoComprarVagas { compra, com_tp: true });
                }
                let r_ouro = Rect::new(x + bw + meio, y, bw, 36.0 * f);
                if botao(r_ouro, &format!("{} gold", milhar(ouro)), livre && c.ouro >= ouro, false) {
                    saida.push(ClientMessage::MercadoComprarVagas { compra, com_tp: false });
                }
                y += 36.0 * f;
            }
            None => {
                estilo::texto_ajustado("You have every slot there is.", x, y + 12.0 * f, w, 13, estilo::SUAVE);
                y += 18.0 * f;
            }
        }
        let linha = y + 12.0 * f;
        draw_line(x, linha, x + w, linha, 1.0, estilo::BORDA);
        linha
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
            "Claim everything",
            livre && !self.cartas.is_empty(),
            true,
        ) {
            saida.push(ClientMessage::MercadoReceber);
        }
        let alt = 54.0 * f;
        let topo = a.y + 46.0 * f;
        let lista = Rect::new(a.x, topo, a.w, (a.y + a.h - topo).max(alt));
        let _ = abre_lista(&mut self.rolagem, lista, self.cartas.len(), alt, livre);
        if self.cartas.is_empty() {
            estilo::texto(
                lista.x,
                lista.y + 30.0 * f,
                "No delivery waiting.",
                15,
                estilo::SUAVE,
            );
        }
        for (i, carta) in self.cartas.iter().enumerate() {
            let Some(r) = linha_da_lista(&self.rolagem, lista, i, alt, f) else {
                continue;
            };
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
                let base = c
                    .nomes
                    .get(&carta.item_id)
                    .cloned()
                    .unwrap_or_else(|| format!("Item {}", carta.item_id));
                partes.push(format!(
                    "{} ×{}",
                    nome_com_degrau(&base, carta.instancia.as_ref()),
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
        fecha_lista(&self.rolagem, lista, self.cartas.len(), alt);
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
            "TP for sale (gold per TP)",
            14,
            estilo::SUAVE,
        );
        let alt = 58.0 * f;
        let lista = Rect::new(esq.x, esq.y + 26.0 * f, esq.w, esq.h - 26.0 * f);
        let toque = abre_lista(&mut self.rolagem, lista, self.lista_tp.len(), alt, livre);
        if self.lista_tp.is_empty() {
            estilo::texto(
                lista.x,
                lista.y + 30.0 * f,
                "Nobody selling TP right now.",
                15,
                estilo::SUAVE,
            );
        }
        let mut comprar = None;
        for (i, an) in self.lista_tp.iter().enumerate() {
            let Some(r) = linha_da_lista(&self.rolagem, lista, i, alt, f) else {
                continue;
            };
            if linha_de_anuncio(
                r,
                an,
                c,
                f,
                livre,
                if an.meu { "Yours" } else { "Buy" },
                !an.meu,
                toque,
            ) {
                comprar = Some(an.clone());
            }
        }
        fecha_lista(&self.rolagem, lista, self.lista_tp.len(), alt);
        if let Some(an) = comprar {
            self.compra = Some(Compra {
                anuncio: an,
                qtd: 1,
            });
        }

        estilo::painel(dir);
        let (x, w) = (dir.x + 16.0 * f, dir.w - 32.0 * f);
        let saldo = self.tp.unwrap_or(0);
        let rotulo = "Your TP";
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
                &format!("Selling TP unlocks at level {}.", regras::NIVEL_PARA_VENDER),
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
        // The TP's recommended price: gold per TP, from the realm's gold over
        // the TP the accounts hold, never under the shop's Sack of Gold.
        let recomendado = self
            .precos
            .get(&shared::precos::ID_DO_TP)
            .map(|v| v.round().max(1.0) as u64);
        if self.tp_preco == 0 {
            self.tp_preco = recomendado.unwrap_or(100);
        }
        seletor(
            &mut self.edicao,
            "tp_qtd",
            c.digitado,
            Rect::new(x, y, w, 42.0 * f),
            "TP to sell",
            &mut self.tp_qtd,
            1,
            saldo.clamp(1, regras::TP_MAX_POR_ANUNCIO),
            false,
            f,
        );
        y += 66.0 * f;
        seletor(
            &mut self.edicao,
            "tp_preco",
            c.digitado,
            Rect::new(x, y, w, 42.0 * f),
            "Gold per TP",
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
            &format!("Fee {}%", regras::TAXA_PCT),
            &format!("−{}", milhar(regras::taxa(bruto))),
            estilo::SUAVE,
            f,
        );
        linha_valor(
            x,
            y + 44.0 * f,
            w,
            "You receive",
            &milhar(regras::liquido(bruto)),
            estilo::OURO,
            f,
        );
        let menor = self.lista_tp.iter().filter(|an| !an.meu).map(|an| an.preco_unit).min();
        referencias(&mut self.tp_preco, recomendado, menor, "Lowest on the market: {} gold", (x, y, w, f), livre);
        let bt = Rect::new(x, dir.y + dir.h - 58.0 * f, w, 44.0 * f);
        let pode = livre
            && c.nivel >= regras::NIVEL_PARA_VENDER
            && saldo >= self.tp_qtd
            && self.meus.len() < self.max_anuncios();
        if botao(bt, "List TP", pode, true) {
            saida.push(ClientMessage::MercadoAnunciarTp {
                qtd: self.tp_qtd,
                preco_unit: self.tp_preco,
            });
        }
    }

    // ─────────────────────────────── Energia ───────────────────────────────

    /// ENERGY for gold, like the TP tab: the lots on sale on the left, the
    /// form on the right. A lot is 1,000 Energy (`ENERGIA_POR_LOTE`); it
    /// leaves the character when listed and arrives as Energy when bought.
    fn aba_energia(&mut self, a: Rect, c: &Contexto, f: f32, livre: bool, saida: &mut Vec<ClientMessage>) {
        let esq = Rect::new(a.x, a.y, a.w * 0.56, a.h);
        let dir = Rect::new(a.x + a.w * 0.59, a.y, a.w * 0.41, a.h);
        estilo::texto_forte(esq.x, esq.y + 14.0 * f, "Energy for sale (gold per 1,000)", 14, estilo::SUAVE);
        let alt = 58.0 * f;
        let lista = Rect::new(esq.x, esq.y + 26.0 * f, esq.w, esq.h - 26.0 * f);
        let lotes: Vec<AnuncioNet> = self
            .lista
            .iter()
            .filter(|an| an.item_id == shared::item_id::ENERGIA_MIL && an.tipo == regras::TIPO_ITEM)
            .cloned()
            .collect();
        let toque = abre_lista(&mut self.rolagem, lista, lotes.len(), alt, livre);
        if lotes.is_empty() {
            estilo::texto(lista.x, lista.y + 30.0 * f, "Nobody selling Energy right now.", 15, estilo::SUAVE);
        }
        let mut comprar = None;
        for (i, an) in lotes.iter().enumerate() {
            let Some(r) = linha_da_lista(&self.rolagem, lista, i, alt, f) else {
                continue;
            };
            if linha_de_anuncio(r, an, c, f, livre, if an.meu { "Yours" } else { "Buy" }, !an.meu, toque) {
                comprar = Some(an.clone());
            }
        }
        fecha_lista(&self.rolagem, lista, lotes.len(), alt);
        if let Some(an) = comprar {
            self.compra = Some(Compra { anuncio: an, qtd: 1 });
        }

        estilo::painel(dir);
        let (x, w) = (dir.x + 16.0 * f, dir.w - 32.0 * f);
        let rotulo = "Your Energy";
        estilo::texto_forte(x, dir.y + 32.0 * f, rotulo, 18, estilo::SUAVE);
        let ix = x + estilo::medir_forte(rotulo, 18) + 22.0 * f;
        estilo::icone_energia(vec2(ix, dir.y + 26.0 * f), 22.0 * f);
        estilo::texto_forte(ix + 16.0 * f, dir.y + 32.0 * f, &milhar(c.energia), 18, estilo::OURO);
        estilo::texto_ajustado("Sold in lots of 1,000. It leaves you when listed.", x, dir.y + 54.0 * f, w, 12, estilo::SUAVE);
        let mut y = dir.y + 92.0 * f;
        if c.nivel < regras::NIVEL_PARA_VENDER {
            estilo::texto_ajustado(
                &format!("Selling Energy unlocks at level {}.", regras::NIVEL_PARA_VENDER),
                x,
                y,
                w,
                14,
                estilo::VERMELHO,
            );
            y += 26.0 * f;
        }
        let tem_lotes = c.energia / regras::ENERGIA_POR_LOTE;
        let recomendado = self
            .precos
            .get(&shared::item_id::ENERGIA_MIL)
            .map(|v| v.round().max(1.0) as u64);
        if self.energia_lotes == 0 {
            self.energia_lotes = 1;
        }
        if self.energia_preco == 0 {
            self.energia_preco = recomendado.unwrap_or(100);
        }
        seletor(
            &mut self.edicao,
            "energia_lotes",
            c.digitado,
            Rect::new(x, y, w, 42.0 * f),
            "Lots of 1,000 to sell",
            &mut self.energia_lotes,
            1,
            tem_lotes.clamp(1, regras::LOTES_MAX_POR_ANUNCIO),
            false,
            f,
        );
        y += 66.0 * f;
        seletor(
            &mut self.edicao,
            "energia_preco",
            c.digitado,
            Rect::new(x, y, w, 42.0 * f),
            "Gold per 1,000 Energy",
            &mut self.energia_preco,
            1,
            regras::PRECO_MAX_UNIT,
            true,
            f,
        );
        y += 60.0 * f;
        let bruto = regras::total(self.energia_lotes, self.energia_preco).unwrap_or(0);
        linha_valor(x, y, w, "Total", &milhar(bruto), estilo::TEXTO, f);
        linha_valor(
            x,
            y + 22.0 * f,
            w,
            &format!("Fee {}%", regras::TAXA_PCT),
            &format!("−{}", milhar(regras::taxa(bruto))),
            estilo::SUAVE,
            f,
        );
        linha_valor(x, y + 44.0 * f, w, "You receive", &milhar(regras::liquido(bruto)), estilo::OURO, f);
        let menor = lotes.iter().filter(|an| !an.meu).map(|an| an.preco_unit).min();
        referencias(&mut self.energia_preco, recomendado, menor, "Lowest on the market: {} gold", (x, y, w, f), livre);
        let bt = Rect::new(x, dir.y + dir.h - 58.0 * f, w, 44.0 * f);
        let pode = livre
            && c.nivel >= regras::NIVEL_PARA_VENDER
            && tem_lotes >= self.energia_lotes
            && self.meus.len() < self.max_anuncios();
        if botao(bt, "List Energy", pode, true) {
            saida.push(ClientMessage::MercadoAnunciarEnergia {
                lotes: self.energia_lotes,
                preco_unit: self.energia_preco,
            });
        }
    }
}

// ─────────────────────────────── pecas ───────────────────────────────

/// BOTH PRICE REFERENCES, for every sale (items and TP): the recommended
/// price and the market's lowest, each with Use. A missing one says so
/// instead of vanishing, so the seller always knows what was looked at.
fn referencias(
    preco: &mut u64,
    recomendado: Option<u64>,
    menor: Option<u64>,
    rotulo_menor: &str,
    (x, y, w, f): (f32, f32, f32, f32),
    livre: bool,
) {
    let linhas = [
        (recomendado, "Recommended: {} gold", "Recommended: not enough data yet", estilo::OURO),
        (menor, rotulo_menor, "Lowest on the market: nobody is selling this", estilo::TEXTO),
    ];
    for (k, (valor, com, sem, cor)) in linhas.into_iter().enumerate() {
        let ry = y + (80.0 + k as f32 * 36.0) * f;
        match valor {
            Some(v) => {
                estilo::texto_ajustado(&com.replace("{}", &milhar(v)), x, ry, w - 80.0 * f, 14, cor);
                let usar = Rect::new(x + w - 70.0 * f, ry - 18.0 * f, 70.0 * f, 26.0 * f);
                if *preco != v && botao(usar, "Use", livre, false) {
                    *preco = v;
                }
            }
            None => estilo::texto_ajustado(sem, x, ry, w, 14, estilo::SUAVE),
        }
    }
}

fn botao(r: Rect, rotulo: &str, ativo: bool, primario: bool) -> bool {
    let m = Vec2::from(mouse_position());
    let sobre = ativo && r.toca(m);
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
    nome_do_anuncio_com(an, c.nomes)
}

fn nome_do_anuncio_com(an: &AnuncioNet, nomes: &HashMap<u16, String>) -> String {
    if an.tipo == regras::TIPO_TP {
        return "Tempest Points".into();
    }
    let base = nomes
        .get(&an.item_id)
        .cloned()
        .unwrap_or_else(|| an.nome.clone());
    nome_com_degrau(&base, an.instancia.as_ref())
}

/// An item's name as the market shows it: with an instance (gear, pet,
/// mount) the tier and the refine follow, in the notation of the Workshop
/// and the bag ("Twin Pistols T II +3"); the colour is already the icon's.
/// Without an instance (materials, potions) the name stays bare.
pub fn nome_com_degrau(base: &str, inst: Option<&shared::items::ItemInstance>) -> String {
    let Some(i) = inst else {
        return base.to_string();
    };
    let mut s = format!("{base} T {}", crate::oficina_ui::romano(i.tier()));
    if i.refinement > 0 {
        s.push_str(&format!(" +{}", i.refinement));
    }
    s
}

/// The icon's square on a listing row.
fn icone_do_anuncio(r: Rect, f: f32) -> Rect {
    Rect::new(r.x + 6.0 * f, r.y + 4.0 * f, r.h - 8.0 * f, r.h - 8.0 * f)
}

/// A listing row with a button on the right. `true` on the button's tap
/// (`toque`, the list's tap from `abre_lista`).
#[allow(clippy::too_many_arguments)]
fn linha_de_anuncio(
    r: Rect,
    an: &AnuncioNet,
    c: &Contexto,
    f: f32,
    livre: bool,
    rotulo: &str,
    ativo: bool,
    toque: Option<Vec2>,
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
    botao_na_lista(b, rotulo, livre && ativo, ativo && rotulo != "Cancel", toque)
}

/// Results the item picker lists (it scrolls; more than fit is fine).
const MAX_ACHADOS: usize = 60;

/// Room on the right of a scrolled list's rows for its bar.
fn margem_da_barra(f: f32) -> f32 {
    20.0 * f
}

/// Starts one frame of a scrolled list of `n` rows `alt` tall: finger drag
/// with glide, wheel and bar (`rolagem::Rolagem`), then clips the drawing to
/// `lista`. Returns the TAP — a release that did not drag — which the rows'
/// buttons answer instead of the press, so dragging over a button scrolls.
fn abre_lista(rol: &mut crate::rolagem::Rolagem, lista: Rect, n: usize, alt: f32, livre: bool) -> Option<Vec2> {
    let toque = if livre { rol.quadro(lista, n as f32 * alt, alt) } else { None };
    crate::rolagem::recortar(Some(lista));
    toque
}

/// Row `i` of a scrolled list on screen, or `None` while it is out of sight.
fn linha_da_lista(rol: &crate::rolagem::Rolagem, lista: Rect, i: usize, alt: f32, f: f32) -> Option<Rect> {
    let r = Rect::new(
        lista.x,
        lista.y + i as f32 * alt - rol.pos,
        lista.w - margem_da_barra(f),
        alt - 6.0 * f,
    );
    (r.y + r.h >= lista.y && r.y <= lista.y + lista.h).then_some(r)
}

/// Ends the list's frame: clipping off, and the bar on top of the rows.
fn fecha_lista(rol: &crate::rolagem::Rolagem, lista: Rect, n: usize, alt: f32) {
    crate::rolagem::recortar(None);
    rol.desenha(lista, n as f32 * alt);
}

/// A button inside a scrolled list: drawn like `botao`, but it fires on the
/// list's tap (release without drag), never on the press.
fn botao_na_lista(r: Rect, rotulo: &str, ativo: bool, primario: bool, toque: Option<Vec2>) -> bool {
    let m = Vec2::from(mouse_position());
    let sobre = ativo && r.toca(m);
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
    ativo && toque.is_some_and(|p| r.toca(p))
}

/// Search text past `BUSCA_MAX_CHARS` is cut.
fn corta(s: &mut String) {
    if s.chars().count() > regras::BUSCA_MAX_CHARS {
        *s = s.chars().take(regras::BUSCA_MAX_CHARS).collect();
    }
}

/// How many of `item_id` a buy order could take from me: plain stacks in the
/// bag, or whole lots of Energy.
fn tenho(c: &Contexto, item_id: u16) -> u64 {
    if item_id == shared::item_id::ENERGIA_MIL {
        return (c.energia / regras::ENERGIA_POR_LOTE).min(regras::LOTES_MAX_POR_ANUNCIO);
    }
    c.slots
        .iter()
        .filter(|s| s.qty > 0 && s.item_id == item_id && s.instance.is_none())
        .map(|s| s.qty as u64)
        .sum()
}

/// The recommended price of one unit, rounded (`shared::precos`).
fn recomendado_unit(precos: &HashMap<u16, f64>, item_id: u16) -> Option<u64> {
    precos.get(&item_id).map(|v| v.round().max(1.0) as u64)
}

/// Items a buy order can be placed for whose name has `texto`, by name, at
/// most `max`. The client does not know stack sizes: the server refuses an
/// item that does not stack (`regras::pode_pedir_item`).
fn catalogo_pedivel(
    nomes: &HashMap<u16, String>,
    vinculados: &HashSet<u16>,
    equipaveis: &HashSet<u16>,
    texto: &str,
    max: usize,
) -> Vec<(u16, String)> {
    let t = texto.trim().to_lowercase();
    if t.is_empty() {
        return Vec::new();
    }
    let mut v: Vec<(u16, String)> = nomes
        .iter()
        .filter(|(id, nome)| {
            !nome.is_empty()
                && regras::pode_pedir_item(**id, equipaveis.contains(id), vinculados.contains(id), u32::MAX)
                && nome.to_lowercase().contains(&t)
        })
        .map(|(id, nome)| (*id, nome.clone()))
        .collect();
    if "energy ×1,000".contains(&t) && !nomes.contains_key(&shared::item_id::ENERGIA_MIL) {
        v.push((shared::item_id::ENERGIA_MIL, "Energy ×1,000".into()));
    }
    v.sort_by(|a, b| {
        let pa = !a.1.to_lowercase().starts_with(&t);
        let pb = !b.1.to_lowercase().starts_with(&t);
        (pa, a.1.len(), &a.1).cmp(&(pb, b.1.len(), &b.1))
    });
    v.truncate(max);
    v
}

fn nome_do_pedido(pd: &PedidoNet, c: &Contexto) -> String {
    if pd.item_id == shared::item_id::ENERGIA_MIL {
        return pd.nome.clone();
    }
    c.nomes.get(&pd.item_id).cloned().unwrap_or_else(|| pd.nome.clone())
}

/// A buy order row: item, buyer, units still wanted, price each, a button.
/// `toque` = the list's tap (`abre_lista`): the button answers it, not the press.
#[allow(clippy::too_many_arguments)]
fn linha_de_pedido(r: Rect, pd: &PedidoNet, c: &Contexto, f: f32, livre: bool, rotulo: &str, ativo: bool, toque: Option<Vec2>) -> bool {
    estilo::cartao(r, false, pd.meu);
    let icone = icone_do_anuncio(r, f);
    crate::icones::icone_com_3d(pd.item_id, icone, None, None, Some((c.vox, c.solido)));
    let x = icone.x + icone.w + 10.0 * f;
    let bw = 116.0 * f;
    let preco = format!("{} gold", milhar(pd.preco_unit));
    let qtd = format!("×{}", milhar(pd.qtd));
    let xp = r.x + r.w - bw - 16.0 * f - estilo::medir_forte(&preco, 16);
    let largura_nome = (xp - x - 90.0 * f).max(60.0 * f);
    estilo::texto_ajustado(&nome_do_pedido(pd, c), x, r.y + 22.0 * f, largura_nome, 16, estilo::TEXTO);
    estilo::texto_ajustado(&format!("Wanted by {} · {}", pd.realm, pd.comprador), x, r.y + 40.0 * f, largura_nome, 12, estilo::SUAVE);
    estilo::texto(xp - 70.0 * f, r.y + 32.0 * f, &qtd, 15, estilo::TEXTO);
    estilo::texto_forte(xp, r.y + 32.0 * f, &preco, 16, estilo::OURO);
    let b = Rect::new(r.x + r.w - bw - 6.0 * f, r.y + (r.h - 38.0 * f) * 0.5, bw, 38.0 * f);
    botao_na_lista(b, rotulo, livre && ativo, ativo && rotulo != "Cancel", toque)
}

/// A number being typed into a `seletor` (owner, 12/11/2026: "tap the number
/// and type the amount"). One at a time in the whole panel.
#[derive(Default)]
struct Edicao {
    /// The `seletor` being typed into, by its id.
    id: Option<&'static str>,
    texto: String,
    /// The `seletor` was drawn this frame; an edit whose field vanished
    /// (tab changed, dialog closed) is dropped at the end of the frame.
    visto: bool,
    /// A tap on a number asks to edit it. It only starts at the end of the
    /// frame, so the one being edited (drawn before or after) commits first.
    pedido: Option<(&'static str, f32)>,
    /// Bottom of the field on screen (to raise the panel above the keyboard).
    fundo: f32,
}

impl Edicao {
    fn ativa(&self) -> bool {
        self.id.is_some()
    }
}

/// The typed text as a value: digits only, clamped to `[min, max]`. Empty =
/// `None` (the old value stays). Too many digits for a `u64` is `max`.
fn valor_digitado(texto: &str, min: u64, max: u64) -> Option<u64> {
    let digitos: String = texto.chars().filter(char::is_ascii_digit).collect();
    if digitos.is_empty() {
        return None;
    }
    let v = digitos.parse::<u64>().unwrap_or(u64::MAX);
    Some(v.clamp(min, max.max(min)))
}

/// What a number field takes from the keyboard: digits and backspace (8).
/// Letters, spaces, signs and Enter never reach the text.
fn so_digitos(digitado: &[char]) -> Vec<char> {
    digitado
        .iter()
        .copied()
        .filter(|c| c.is_ascii_digit() || *c as u32 == 8)
        .collect()
}

/// A number by touch: [Min][-] value [+][Max] for quantity; [/10][-] value
/// [+][x10] for price (+- moves 10% of the value, at least 1). Tapping the
/// value types it instead: Enter or a tap elsewhere commits (clamped to
/// `[min, max]`), Escape cancels (`Mercado::esc`). `id` tells the panel's
/// seletors apart.
#[allow(clippy::too_many_arguments)]
fn seletor(
    ed: &mut Edicao,
    id: &'static str,
    digitado: &[char],
    r: Rect,
    rotulo: &str,
    valor: &mut u64,
    min: u64,
    max: u64,
    preco: bool,
    f: f32,
) {
    estilo::texto(r.x, r.y - 6.0 * f, rotulo, 13, estilo::SUAVE);
    let bw = (52.0 * f).min(r.w / 6.0);
    let rotulos = if preco {
        ["÷10", "−", "+", "×10"]
    } else {
        ["Min", "−", "+", "Max"]
    };
    let rects = [
        Rect::new(r.x, r.y, bw, r.h),
        Rect::new(r.x + bw + 4.0 * f, r.y, bw, r.h),
        Rect::new(r.x + r.w - 2.0 * bw - 4.0 * f, r.y, bw, r.h),
        Rect::new(r.x + r.w - bw, r.y, bw, r.h),
    ];
    let meio = Rect::new(
        rects[1].x + bw + 4.0 * f,
        r.y,
        (rects[2].x - 4.0 * f) - (rects[1].x + bw + 4.0 * f),
        r.h,
    );
    let m = Vec2::from(mouse_position());
    let clicou = crate::foco::clique();
    // Typing: the field takes the keys first, so "25" + Enter in one frame commits 25.
    if ed.id == Some(id) {
        ed.visto = true;
        ed.fundo = meio.y + meio.h;
        let teclas = so_digitos(digitado);
        crate::ui::campo(meio, "", &mut ed.texto, true, false, &teclas);
        ed.texto.truncate(max.max(min).to_string().len());
        if ed.texto.is_empty() {
            // The old value, faint, while nothing is typed.
            estilo::texto_centro(meio.center().x, meio.center().y + 6.0 * f, &milhar(*valor), 15, estilo::SUAVE);
        }
        let enter = is_key_pressed(KeyCode::Enter)
            || is_key_pressed(KeyCode::KpEnter)
            || digitado.iter().any(|ch| *ch == '\r' || *ch == '\n');
        if enter || (clicou && !meio.toca(m)) {
            if let Some(v) = valor_digitado(&ed.texto, min, max) {
                *valor = v;
            }
            ed.id = None;
            ed.texto.clear();
        }
    }
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
    if ed.id == Some(id) {
        return;
    }
    let sobre = meio.toca(m);
    estilo::ret_arredondado(meio, estilo::RAIO_PEQUENO, estilo::FUNDO_BAIXO);
    // The value looks tappable: an outline that lights under the pointer.
    estilo::borda_arredondada(
        meio,
        estilo::RAIO_PEQUENO,
        1.0,
        if sobre { estilo::alfa(estilo::ACENTO, 0.85) } else { estilo::BORDA_FORTE },
    );
    let t = milhar(*valor);
    estilo::texto_centro_forte(
        meio.center().x,
        meio.center().y + 6.0 * f,
        &t,
        17,
        estilo::TEXTO,
    );
    if sobre && clicou {
        ed.pedido = Some((id, meio.y + meio.h));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn anuncio(tipo: u8) -> AnuncioNet {
        AnuncioNet {
            id: "a".into(),
            tipo,
            item_id: 60,
            nome: "Wood".into(),
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
    fn numero_digitado_so_digitos_e_preso_aos_limites() {
        assert_eq!(valor_digitado("", 1, 480), None, "nothing typed keeps the old value");
        assert_eq!(valor_digitado("250", 1, 480), Some(250));
        assert_eq!(valor_digitado("9999", 1, 480), Some(480), "above max clamps to max");
        assert_eq!(valor_digitado("0", 1, 480), Some(1), "below min clamps to min");
        assert_eq!(valor_digitado("007", 1, 480), Some(7));
        assert_eq!(valor_digitado("99999999999999999999999", 1, u64::MAX), Some(u64::MAX), "overflow is max");
        assert_eq!(valor_digitado("5", 1, 0), Some(1), "an empty range still gives min");
        assert_eq!(
            so_digitos(&['1', 'a', '-', '2', ' ', '\u{8}', '\r', '.', '3']),
            vec!['1', '2', '\u{8}', '3'],
            "letters, signs and Enter never reach the text; backspace does"
        );
    }

    #[test]
    fn edicao_comeca_no_fim_do_quadro_e_some_sem_o_campo() {
        let mut m = Mercado { aberto: true, foco_busca: true, ..Default::default() };
        m.edicao.pedido = Some(("venda_qtd", 300.0));
        m.fim_da_edicao();
        assert_eq!(m.edicao.id, Some("venda_qtd"));
        assert!(!m.foco_busca, "the search field gives up the keyboard");
        assert!(m.foco_na_busca(), "a number being typed opens the on-screen keyboard");
        // Drawn this frame: it stays.
        m.edicao.visto = true;
        m.fim_da_edicao();
        assert_eq!(m.edicao.id, Some("venda_qtd"));
        // Its field was not drawn (tab changed): dropped.
        m.fim_da_edicao();
        assert_eq!(m.edicao.id, None);
        assert!(!m.foco_na_busca());
        // Escape cancels the edit first, then closes.
        m.edicao.id = Some("venda_qtd");
        m.esc();
        assert!(m.aberto && m.edicao.id.is_none());
        m.esc();
        assert!(!m.aberto);
    }

    #[test]
    fn nome_no_mercado_leva_tier_e_refino() {
        let mut inst = shared::ItemInstance::vazia_de_grau(3);
        inst.tier = 2;
        inst.refinement = 3;
        assert_eq!(nome_com_degrau("Twin Pistols", Some(&inst)), "Twin Pistols T II +3");
        inst.refinement = 0;
        inst.tier = 4;
        assert_eq!(nome_com_degrau("Katana", Some(&inst)), "Katana T IV");
        inst.tier = 0;
        assert_eq!(nome_com_degrau("Wolf Cub", Some(&inst)), "Wolf Cub T I", "an old instance reads as tier I");
        assert_eq!(nome_com_degrau("Copper", None), "Copper", "no instance, no tier");
        // A listing row and the buy dialog name it through `nome_do_anuncio`.
        let nomes: HashMap<u16, String> = [(60, "Wood".to_string())].into_iter().collect();
        let mut an = anuncio(regras::TIPO_ITEM);
        let mut i = shared::ItemInstance::vazia_de_grau(2);
        i.tier = 3;
        i.refinement = 5;
        an.instancia = Some(i);
        assert_eq!(nome_do_anuncio_com(&an, &nomes), "Wood T III +5");
        an.instancia = None;
        assert_eq!(nome_do_anuncio_com(&an, &nomes), "Wood");
        assert_eq!(nome_do_anuncio_com(&anuncio(regras::TIPO_TP), &nomes), "Tempest Points");
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

    #[test]
    fn aba_de_pedidos_pede_a_busca_e_os_meus() {
        let mut m = Mercado::default();
        m.abrir();
        let pedidos = m.trocar_aba(Aba::Pedidos);
        assert!(matches!(pedidos[0], ClientMessage::MercadoBuscarPedidos { item_id: None, .. }));
        assert!(matches!(pedidos[1], ClientMessage::MercadoMeusPedidos));
        let pd = PedidoNet {
            id: "p".into(),
            item_id: 345,
            nome: "Darksteel".into(),
            categoria: 2,
            qtd_total: 10,
            qtd: 10,
            preco_unit: 7,
            realm: "SA01".into(),
            comprador: "Ana".into(),
            meu: true,
        };
        m.pedidos(vec![pd.clone()], true);
        m.pedidos(vec![pd.clone(), pd], false);
        assert_eq!((m.pedidos.meus.len(), m.pedidos.lista.len()), (1, 2), "the two lists stay apart");
    }

    #[test]
    fn catalogo_so_mostra_o_que_da_pra_pedir() {
        use shared::constants::item_id as it;
        let nomes: HashMap<u16, String> = [
            (it::DARKSTEEL, "Darksteel".to_string()),
            (it::KATANA, "Dark Katana".to_string()),
            (it::XP_POTION, "Dark Potion".to_string()),
            (it::HEALTH_POTION, "Health Potion".to_string()),
        ]
        .into_iter()
        .collect();
        let vinculados: HashSet<u16> = [it::XP_POTION].into_iter().collect();
        let equipaveis: HashSet<u16> = [it::KATANA].into_iter().collect();
        let achados: Vec<u16> = catalogo_pedivel(&nomes, &vinculados, &equipaveis, "dark", 6).iter().map(|(id, _)| *id).collect();
        assert_eq!(achados, vec![it::DARKSTEEL], "no gear, nothing bound");
        assert!(catalogo_pedivel(&nomes, &vinculados, &equipaveis, "", 6).is_empty());
        let energia = catalogo_pedivel(&nomes, &vinculados, &equipaveis, "energy", 6);
        assert_eq!(energia.first().map(|(id, _)| *id), Some(it::ENERGIA_MIL));
    }
}

/// Preview of the Sell tab with recommended prices (`MMO_PREVIA_MERCADO=1`;
/// PNGs in `MMO_PREVIA_SAIDA`, default /tmp/tempest-mercado): a Rare IV +7
/// katana, a pet with a learned skill, a mount and a stack of steel.
#[cfg(debug_assertions)]
pub async fn previa(vox: &crate::vox::VoxCache) {
    use shared::constants::item_id as it;
    let saida = std::env::var("MMO_PREVIA_SAIDA").unwrap_or_else(|_| "/tmp/tempest-mercado".into());
    std::fs::create_dir_all(&saida).unwrap();
    let rt = render_target(1920, 1080);
    crate::render3d::define_alvo(Some(rt.clone()));
    crate::hud_layout::define_escala_ui(1.6);
    let solido = crate::render3d::material_solido();
    let mut katana = shared::ItemInstance::vazia_de_grau(3);
    katana.tier = 4;
    katana.refinement = 7;
    let pet_id = it::pet_no_grau(it::PETS[0], 2);
    let mut pet = shared::ItemInstance::vazia_de_grau(2);
    pet.pet = Some(shared::items::PetData { skills: [it::SKILL_PET_FARO, 0, 0], ..Default::default() });
    let mut slots = vec![InventorySlot::default(); 12];
    slots[0] = InventorySlot { item_id: it::KATANA, qty: 1, instance: Some(katana) };
    slots[1] = InventorySlot { item_id: pet_id, qty: 1, instance: Some(pet) };
    slots[2] = InventorySlot { item_id: it::montaria_no_grau(it::MONTARIAS[0], 3), qty: 1, instance: None };
    slots[3] = InventorySlot { item_id: it::na_cor(it::STEEL, 2), qty: 480, instance: None };
    slots[4] = InventorySlot { item_id: it::COPPER, qty: 250_000, instance: None };
    slots[5] = InventorySlot { item_id: it::WOOD_T4, qty: 10, instance: None };
    slots[6] = InventorySlot { item_id: it::RACAO_DE_PET, qty: 20, instance: None };
    let estoque = shared::precos::Estoque {
        ouro: 227_850,
        unidades: [
            (it::COPPER, 8_100_000),
            (it::DARKSTEEL, 61_000),
            (it::GLITTERING_POWDER, 1_500),
            (it::na_cor(it::STEEL, 2), 16_000),
            (pet_id, 40),
            (it::SKILL_PET_FARO, 30),
            (it::montaria_no_grau(it::MONTARIAS[0], 3), 25),
        ]
        .into_iter()
        .collect(),
        base_em_cobre: [(it::KATANA, 960)].into_iter().collect(),
        tp: 5_716,
    };
    let mut nomes: HashMap<u16, String> = HashMap::new();
    for (id, n) in [(it::KATANA, "Katana"), (pet_id, "Wolf Cub"), (it::na_cor(it::STEEL, 2), "Green Steel"), (it::COPPER, "Copper"), (it::RACAO_DE_PET, "Pet Feed")] {
        nomes.insert(id, n.into());
    }
    let mut m = Mercado { aberto: true, aba: Aba::Vender, precos: shared::precos::calcular(&estoque).mapa(), ..Default::default() };
    // On the market: the same Rare IV +7 for less, and a +0 for far less
    // (which must NOT count as the same piece).
    let mut zero = katana;
    zero.refinement = 0;
    for (inst, preco) in [(katana, 900_000u64), (zero, 40_000)] {
        m.lista.push(AnuncioNet {
            id: String::new(),
            tipo: regras::TIPO_ITEM,
            item_id: it::KATANA,
            nome: "Katana".into(),
            categoria: 1,
            instancia: Some(inst),
            qtd: 1,
            preco_unit: preco,
            realm: "SA01".into(),
            vendedor: "Outro".into(),
            meu: false,
        });
    }
    for (nome, slot) in [("katana", 0usize), ("pet", 1), ("montaria", 2), ("aco", 3), ("madeira", 5), ("racao", 6), ("tp", 0), ("energia", 0), ("comprar", 0), ("vagas", 0), ("vagas-cheio", 0), ("pedidos", 0), ("compra-digitando", 3), ("compra-teclado", 3), ("venda-digitando", 3)] {
        m.aba = match nome {
            "pedidos" => Aba::Pedidos,
            "comprar" | "compra-digitando" | "compra-teclado" => Aba::Comprar,
            "vagas" | "vagas-cheio" => Aba::Meus,
            "tp" => Aba::Tp,
            "energia" => Aba::Energia,
            _ => Aba::Vender,
        };
        if nome == "pedidos" {
            let p = |id: &str, item: u16, n: &str, qtd: u64, preco: u64, quem: &str| PedidoNet {
                id: id.into(), item_id: item, nome: n.into(), categoria: regras::Categoria::Material as u8,
                qtd_total: qtd, qtd, preco_unit: preco, realm: "SA01".into(), comprador: quem.into(), meu: quem == "Navegante",
            };
            m.pedidos.lista = vec![
                p("a", it::DARKSTEEL, "Darksteel", 5_000, 3, "Lenda"),
                p("b", it::na_cor(it::STEEL, 2), "Green Steel", 40, 120, "Anubis"),
                p("c", it::ENERGIA_MIL, "Energy ×1,000", 10, 45, "OldSchool"),
            ];
            m.pedidos.meus = vec![p("d", it::na_cor(it::QUINTESSENCE, 1), "Grey Quintessence", 200, 18, "Navegante")];
        }
        if nome == "energia" {
            m.lista.push(AnuncioNet {
                id: String::new(),
                tipo: regras::TIPO_ITEM,
                item_id: it::ENERGIA_MIL,
                nome: "Energy ×1,000".into(),
                categoria: regras::Categoria::Consumivel as u8,
                instancia: None,
                qtd: 120,
                preco_unit: 40,
                realm: "SA01".into(),
                vendedor: "Outro".into(),
                meu: false,
            });
            m.precos.insert(it::ENERGIA_MIL, 34.7);
        }
        if nome.starts_with("vagas") {
            // My listings with one slot purchase made (25 slots), then at the cap.
            m.tp = Some(1_000);
            m.compras_vagas = if nome == "vagas" { 1 } else { regras::CUSTO_VAGAS_TP.len() as u8 };
            m.meus = m.lista.iter().cloned().cycle().take(7).map(|mut an| { an.meu = true; an }).collect();
            m.historico = vec![VendaNet { nome: "Green Steel".into(), qtd: 40, preco_unit: 120, taxa: 240, liquido: 4_560, quando: 0, vendi: true }];
        }
        if nome == "tp" {
            m.tp = Some(1_000);
            m.lista_tp = vec![AnuncioNet {
                id: String::new(),
                tipo: regras::TIPO_TP,
                item_id: 0,
                nome: "TP".into(),
                categoria: regras::Categoria::Tp as u8,
                instancia: None,
                qtd: 300,
                preco_unit: 250,
                realm: "SA01".into(),
                vendedor: "Outro".into(),
                meu: false,
            }];
        }
        m.venda_slot = Some(slot);
        m.venda_qtd = 1;
        m.venda_preco = shared::precos::recomendado(&m.precos, slots[slot].item_id, slots[slot].instance.as_ref()).unwrap_or(100);
        // Typing a number: the buy dialog's quantity (on a stack of 480; with
        // the phone keyboard up, the dialog rises), and the sell price.
        m.compra = nome.starts_with("compra-").then(|| Compra {
            anuncio: AnuncioNet {
                id: "s".into(),
                tipo: regras::TIPO_ITEM,
                item_id: it::na_cor(it::STEEL, 2),
                nome: "Green Steel".into(),
                categoria: regras::Categoria::Material as u8,
                instancia: None,
                qtd: 480,
                preco_unit: 120,
                realm: "SA01".into(),
                vendedor: "Outro".into(),
                meu: false,
            },
            qtd: 1,
        });
        m.edicao = Edicao::default();
        let (edita, texto) = match nome {
            "compra-digitando" | "compra-teclado" => (Some("compra_qtd"), "250"),
            "venda-digitando" => (Some("venda_preco"), "1250"),
            _ => (None, ""),
        };
        m.edicao.id = edita;
        m.edicao.texto = texto.into();
        let teclado = nome == "compra-teclado";
        let c = Contexto { slots: &slots, nomes: &nomes, ouro: 50_000, nivel: 40, digitado: &[], vox, solido: &solido, energia: 254_300, teclado };
        for _ in 0..3 {
            crate::render3d::camera_padrao();
            clear_background(Color::new(0.08, 0.12, 0.16, 1.));
            let _ = m.desenha(&c, get_time());
            unsafe { get_internal_gl().flush() };
            rt.texture.get_texture_data().export_png(&format!("{saida}/{nome}.png"));
            next_frame().await;
        }
    }
    // The bound seal on the bag's own cells: bound kinds, a bound piece,
    // and free ones beside them for contrast.
    crate::icones::define_vinculados([it::XP_POTION, 490]);
    let mut presa = katana;
    presa.vinculado = true;
    let celulas = [
        InventorySlot { item_id: it::XP_POTION, qty: 3, instance: None },
        InventorySlot { item_id: 490, qty: 1, instance: None },
        InventorySlot { item_id: it::KATANA, qty: 1, instance: Some(presa) },
        InventorySlot { item_id: it::KATANA, qty: 1, instance: Some(katana) },
        InventorySlot { item_id: it::na_cor(it::STEEL, 2), qty: 480, instance: None },
    ];
    for _ in 0..3 {
        crate::render3d::camera_padrao();
        clear_background(Color::new(0.08, 0.12, 0.16, 1.));
        for (k, c) in celulas.iter().enumerate() {
            let r = Rect::new(200.0 + k as f32 * 130.0, 200.0, 110.0, 110.0);
            crate::bolsa::celula_do_slot(r, c, Some((vox, &solido)));
        }
        for (k, c) in celulas.iter().enumerate() {
            let r = Rect::new(200.0 + k as f32 * 80.0, 380.0, 64.0, 64.0);
            crate::bolsa::celula_do_slot(r, c, Some((vox, &solido)));
        }
        unsafe { get_internal_gl().flush() };
        rt.texture.get_texture_data().export_png(&format!("{saida}/selo.png"));
        next_frame().await;
    }
}
