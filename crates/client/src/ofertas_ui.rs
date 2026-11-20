//! FLASH OFFERS panel (docs/OFERTAS_RELAMPAGO.md).
//!
//! Always reachable — Menu › Event › Offers and its own icon in the quick bar
//! at the top right — with a red badge while an offer is open (owner,
//! 08/11/2026). A NEW offer pops the panel open by itself.
//!
//! Only a client that can pay asks the server for offers (Android, Google
//! Play): on PC/iPhone the hour would run out on an offer nobody can buy.
//!
//! The look (owner, 15/11/2026: "this tab is very ugly, it doesn't make you
//! want to buy"): each offer is a CARD in the TP shop's premium language —
//! night-purple with gold, a glow by value tier, the TP crystal floating
//! over a "+400 TP" headline, the bonus as an icon list, a live clock, a
//! shine sweeping across, the price on a big gold button. The "×N VALUE"
//! badge and the crossed-out "worth" price are COMPUTED
//! (`pacotes_bonus::valor_em_centavos`): what the contents cost in the TP
//! shop, buying the TP at its best rate; gear is not counted, so the number
//! can only be low, never inflated. The "Rate us" card sits after the offers.

use std::collections::HashMap;

use macroquad::prelude::*;
use shared::loja::{OfertaRelampago, PedidoLoja};
use shared::protocol::ClientMessage;

use crate::hud_estilo as estilo;
use crate::loja_tp::{
    botao_ouro, brilho_radial, etiqueta_estado, faiscas, selo, CIANO, LILAS, NOITE, OURO_CLARO,
    VERDE_POSSE,
};
#[allow(unused_imports)]
use crate::ui::Toque as _;

/// Real payments exist here (Google Play Billing).
pub const PODE_PAGAR: bool = cfg!(target_os = "android");

/// An offer on screen: the offer, the `get_time()` it closes at and what its
/// contents are worth (cents, `pacotes_bonus::valor_em_centavos`).
type Aberta = (OfertaRelampago, f64, u64);

#[derive(Default)]
pub struct OfertasUi {
    pub aberto: bool,
    /// Open offers.
    ofertas: Vec<Aberta>,
    /// Offer whose Google Play sheet is open now.
    pendente: Option<String>,
    em_voo: bool,
    ultimo: Option<(bool, String)>,
    /// Level of the last check (a new level may unlock an offer).
    nivel_checado: Option<u32>,
    /// The server's payments are SIMULATED (test server): any client can
    /// buy, nobody is charged (`EstadoLoja::simulado`).
    simulado: bool,
    /// A BONUS PACK just popped (death, first Porão): the window shows only
    /// it, big, until closed (docs/OFERTAS_RELAMPAGO.md, "Pacotes bônus").
    foco: Option<String>,
    contador: u32,
    /// The offer whose purchase is in flight (Play or simulated).
    comprando: Option<String>,
    /// Bought this session: the card stays, marked PURCHASED (the server's
    /// list no longer has it).
    compradas: Vec<(OfertaRelampago, u64)>,
    rolagem: crate::rolagem::Rolagem,
}

/// R$ 0,99 in Portuguese, US$ 0.99 in English — the same number (owner:
/// "1 to 1 from real to dollar, so Brazilians can buy too").
pub fn preco(centavos: u32) -> String {
    let (int, dec) = (centavos / 100, centavos % 100);
    if shared::idioma::atual() == shared::idioma::Idioma::Pt {
        format!("R$ {int},{dec:02}")
    } else {
        format!("US$ {int}.{dec:02}")
    }
}

fn relogio(seg: f64) -> String {
    let s = seg.max(0.0) as u64;
    if s >= 3600 {
        format!("{}:{:02}:{:02}", s / 3600, s / 60 % 60, s % 60)
    } else {
        format!("{:02}:{:02}", s / 60, s % 60)
    }
}

/// What the offer's contents are worth, in cents (see the module doc).
fn valor_de(o: &OfertaRelampago) -> u64 {
    shared::pacotes_bonus::valor_em_centavos(o.tp as u64, o.ouro, o.energia, &o.itens)
}

/// "×12 VALUE": the worth over the price, whole, only when it is at least 2.
pub fn multiplicador(valor: u64, preco: u32) -> Option<u64> {
    let x = valor / preco.max(1) as u64;
    (x >= 2).then_some(x)
}

/// Value tier by price: 0 under 2.00, 1 under 10.00, 2 above.
pub fn faixa(preco: u32) -> usize {
    match preco {
        0..=199 => 0,
        200..=999 => 1,
        _ => 2,
    }
}

impl OfertasUi {
    fn pedido() -> ClientMessage {
        ClientMessage::Loja {
            pedido: PedidoLoja::OfertasRelampago,
        }
    }

    /// This client can buy: Android (Google Play), or any client while the
    /// server simulates payments.
    pub fn pode_pagar(&self) -> bool {
        PODE_PAGAR || self.simulado
    }

    /// `EstadoLoja` arrived: are payments simulated on this server?
    pub fn definir_simulado(&mut self, simulado: bool) {
        if simulado && !self.simulado {
            // Became able to pay: ask on the next frame.
            self.nivel_checado = None;
        }
        self.simulado = simulado;
    }

    pub fn abrir(&mut self) -> Vec<ClientMessage> {
        self.aberto = true;
        self.foco = None;
        self.rolagem.zera();
        if self.pode_pagar() {
            vec![Self::pedido()]
        } else {
            Vec::new()
        }
    }

    pub fn fechar(&mut self) {
        self.aberto = false;
        self.foco = None;
    }

    fn novo_pedido(&mut self) -> String {
        self.contador = self.contador.wrapping_add(1);
        let ns = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos() as u64);
        format!("o{ns:x}-{:x}", self.contador)
    }

    /// Any offer still running: the red badge on the icon and on MENU.
    pub fn ativa(&self) -> bool {
        let agora = get_time();
        self.ofertas.iter().any(|(_, fim, _)| *fim > agora)
    }

    /// Once per frame in the world: asks again when the level changed (the
    /// first frame counts as a change).
    pub fn checar(&mut self, nivel: u32) -> Option<ClientMessage> {
        if !self.pode_pagar() || nivel == 0 || self.nivel_checado == Some(nivel) {
            return None;
        }
        self.nivel_checado = Some(nivel);
        Some(Self::pedido())
    }

    /// `ServerMessage::OfertasMudaram`: ask again on the next frame.
    pub fn forcar(&mut self) {
        self.nivel_checado = None;
    }

    pub fn receber(&mut self, v: Vec<OfertaRelampago>) {
        let agora = get_time();
        if let Some(o) = v.iter().find(|o| o.nova) {
            self.aberto = true;
            self.ultimo = None;
            self.rolagem.zera();
            // A bonus pack pops alone; a level offer opens the whole list.
            self.foco = (!o.gatilho.is_empty()).then(|| o.id.clone());
            crate::sons::tocar(crate::sons::Som::Clique);
        }
        // An offer bought here and still listed (the reply raced the list)
        // shows as bought, once.
        self.ofertas = v
            .into_iter()
            .filter(|o| !self.compradas.iter().any(|(c, _)| c.id == o.id))
            .map(|o| {
                let fim = agora + o.segundos as f64;
                let valor = valor_de(&o);
                (o, fim, valor)
            })
            .collect();
    }

    /// The shop's answer while a purchase of ours was in flight.
    pub fn resultado(&mut self, ok: bool, texto: &str) {
        if self.em_voo {
            self.em_voo = false;
            self.ultimo = Some((ok, texto.to_string()));
            if let Some(id) = self.comprando.take() {
                if ok {
                    self.marcar_comprada(&id);
                }
            }
        }
    }

    /// The card of `id` turns PURCHASED (kept for this session).
    fn marcar_comprada(&mut self, id: &str) {
        if let Some(i) = self.ofertas.iter().position(|(o, _, _)| o.id == id) {
            let (o, _, valor) = self.ofertas.remove(i);
            self.compradas.retain(|(c, _)| c.id != o.id);
            self.compradas.insert(0, (o, valor));
            self.compradas.truncate(4);
        }
    }

    /// Google Play closed without a purchase (cancelled, error).
    pub fn cancelou(&mut self) {
        if self.pendente.take().is_some() {
            self.em_voo = false;
            self.comprando = None;
            self.ultimo = Some((false, "Purchase canceled".into()));
        }
    }

    /// Which offer a paid Play product belongs to: the one whose sheet we
    /// opened, else any open offer on that product (a purchase recovered at
    /// login).
    pub fn oferta_do_produto(&mut self, produto: &str) -> Option<String> {
        if let Some(id) = self.pendente.take() {
            if self
                .ofertas
                .iter()
                .any(|(o, _, _)| o.id == id && o.produto_play == produto)
            {
                return Some(id);
            }
        }
        self.ofertas
            .iter()
            .find(|(o, _, _)| o.produto_play == produto)
            .map(|(o, _, _)| o.id.clone())
    }

    pub fn desenha(&mut self, nomes: &HashMap<u16, String>) -> Vec<ClientMessage> {
        if !self.aberto {
            return Vec::new();
        }
        if self
            .foco
            .as_ref()
            .is_some_and(|id| !self.ofertas.iter().any(|(o, _, _)| &o.id == id))
        {
            // Bought or expired: back to the list (the result shows there).
            self.foco = None;
        }
        if self.foco.is_some() {
            return estilo::no_painel(estilo::escala_do_painel(BASE_FOCO.x, BASE_FOCO.y), || {
                self.desenha_foco(nomes)
            });
        }
        estilo::no_painel(estilo::escala_do_painel(BASE_LISTA.x, BASE_LISTA.y), || {
            self.desenha_lista(nomes)
        })
    }

    /// The buy button was tapped: Google Play on Android, or the simulated
    /// purchase on a test server.
    fn comprar(&mut self, id: String, produto: String) -> Vec<ClientMessage> {
        self.em_voo = true;
        self.ultimo = None;
        self.comprando = Some(id.clone());
        if self.simulado {
            let pedido = self.novo_pedido();
            return vec![ClientMessage::Loja {
                pedido: PedidoLoja::ComprarOfertaSimulada { oferta: id, pedido },
            }];
        }
        self.pendente = Some(id);
        #[cfg(target_os = "android")]
        crate::billing_android::launch_product(&produto);
        #[cfg(not(target_os = "android"))]
        let _ = produto;
        Vec::new()
    }

    /// The status line at the foot: in flight, the last result, or why the
    /// buttons are off.
    fn rodape(&self, cx: f32, y: f32, largura: f32) {
        let (t, cor): (&str, Color) = if self.em_voo {
            (
                if self.simulado {
                    "Processing…"
                } else {
                    "Waiting for Google Play…"
                },
                estilo::SUAVE,
            )
        } else if let Some((ok, t)) = &self.ultimo {
            (
                t.as_str(),
                if *ok { estilo::VERDE } else { estilo::VERMELHO },
            )
        } else if !self.pode_pagar() {
            (
                "Flash offers are available in the Android app (Google Play).",
                estilo::SUAVE,
            )
        } else if self.simulado {
            (
                "Test server: simulated payment, nobody is charged.",
                estilo::SUAVE,
            )
        } else {
            return;
        };
        let t = shared::idioma::tr(t).into_owned();
        let largo = (largura - 24.0).max(40.0);
        if estilo::medir(&t, 13) <= largo {
            estilo::texto_centro(cx, y, &t, 13, cor);
        } else {
            estilo::texto_ajustado(&t, cx - largo * 0.5, y, largo, 13, cor);
        }
    }

    /// The close "X" in the corner; true when tapped.
    fn fechar_tocado(p: Rect, f: f32, m: Vec2, clicou: bool) -> bool {
        let fechar = Rect::new(p.x + p.w - 48.0 * f, p.y + 8.0 * f, 40.0 * f, 40.0 * f);
        estilo::texto_centro(
            fechar.center().x,
            fechar.center().y + 7.0 * f,
            "X",
            18,
            estilo::TEXTO,
        );
        clicou && fechar.toca(m)
    }

    /// A BONUS PACK alone: why it popped, then its card, big.
    fn desenha_foco(&mut self, nomes: &HashMap<u16, String>) -> Vec<ClientMessage> {
        let f = estilo::fator_texto();
        let seguro = crate::hud_layout::tela_segura();
        let w = (BASE_FOCO.x * f).min(seguro.w - 16.0);
        let h = (BASE_FOCO.y * f).min(seguro.h - 16.0);
        let p = Rect::new(
            seguro.center().x - w * 0.5,
            seguro.center().y - h * 0.5,
            w,
            h,
        );
        crate::hud_layout::escurece(0.55);
        estilo::painel_destaque(p, estilo::OURO);
        let m = Vec2::from(mouse_position());
        let clicou = crate::foco::clique();
        if Self::fechar_tocado(p, f, m, clicou) {
            self.fechar();
            return Vec::new();
        }
        let Some((o, fim, valor)) = self
            .ofertas
            .iter()
            .find(|(o, _, _)| Some(&o.id) == self.foco.as_ref())
            .cloned()
        else {
            return Vec::new();
        };
        estilo::texto_centro_forte(
            p.center().x,
            p.y + 34.0 * f,
            motivo(&o.gatilho),
            15,
            estilo::TEXTO,
        );
        let r = Rect::new(
            p.x + 16.0 * f,
            p.y + 52.0 * f,
            p.w - 32.0 * f,
            p.h - 52.0 * f - 34.0 * f,
        );
        let em_pe = r.h >= altura_vertical(&o, r.w, f, 0.0) || r.w < LARGURA_DEITADA * f;
        let modo = if em_pe { Modo::EmPe } else { Modo::Deitado };
        let aperto = if em_pe {
            aperto_para_caber(&o, r.w, r.h, f)
        } else {
            0.0
        };
        let c = Cartao {
            o: &o,
            resta: fim - get_time(),
            valor,
            comprada: false,
            aperto,
        };
        let livre = self.pode_pagar() && !self.em_voo;
        let toque = clicou.then_some(m);
        let compra = desenha_cartao(r, &c, modo, nomes, f, m, toque, livre, get_time());
        self.rodape(p.center().x, p.y + p.h - 14.0 * f, p.w);
        if compra {
            return self.comprar(o.id.clone(), o.produto_play.clone());
        }
        Vec::new()
    }

    /// Every open offer as a card, the bought ones after them, and the
    /// "Rate us" card at the end — one scrolling list.
    fn desenha_lista(&mut self, nomes: &HashMap<u16, String>) -> Vec<ClientMessage> {
        let mut saida = Vec::new();
        let f = estilo::fator_texto();
        let seguro = crate::hud_layout::tela_segura();
        let w = (BASE_LISTA.x * f).min(seguro.w - 16.0);
        let h = if crate::nativo::modo_celular() && seguro.h > seguro.w * 1.2 {
            // Phone portrait: the list takes the whole height (more cards).
            seguro.h - 16.0
        } else {
            (BASE_LISTA.y * f).min(seguro.h - 16.0)
        };
        let p = Rect::new(
            seguro.center().x - w * 0.5,
            seguro.center().y - h * 0.5,
            w,
            h,
        );
        crate::hud_layout::escurece(0.45);
        estilo::painel_destaque(p, estilo::OURO);
        let m = Vec2::from(mouse_position());
        let clicou = crate::foco::clique();
        let x0 = p.x + 20.0 * f;
        estilo::texto_forte(x0, p.y + 38.0 * f, "Flash offers", 24, estilo::OURO);
        estilo::texto_ajustado(
            &format!(
                "Limited-time deals. Every offer includes at least {} TP.",
                shared::idioma::milhar(shared::pacotes_bonus::TP_MINIMO_OFERTA)
            ),
            x0,
            p.y + 58.0 * f,
            p.w - 90.0 * f,
            13,
            estilo::SUAVE,
        );
        if Self::fechar_tocado(p, f, m, clicou) {
            self.fechar();
            return saida;
        }
        let agora = get_time();
        self.ofertas.retain(|(_, fim, _)| *fim > agora - 1.0);
        let area = Rect::new(
            p.x + 12.0 * f,
            p.y + 70.0 * f,
            p.w - 24.0 * f,
            p.h - 70.0 * f - 30.0 * f,
        );
        let dentro = Rect::new(
            area.x + 6.0 * f,
            area.y + 6.0 * f,
            area.w - 18.0 * f,
            area.h,
        );
        // The cards: what is open, then what was bought here.
        let mut cartoes: Vec<Cartao> = self
            .ofertas
            .iter()
            .map(|(o, fim, valor)| Cartao {
                o,
                resta: fim - agora,
                valor: *valor,
                comprada: false,
                aperto: 0.0,
            })
            .chain(self.compradas.iter().map(|(o, valor)| Cartao {
                o,
                resta: 0.0,
                valor: *valor,
                comprada: true,
                aperto: 0.0,
            }))
            .collect();
        let vao = 14.0 * f;
        let (mut colunas, mut modo) = grade(dentro, cartoes.len(), f);
        if modo == Modo::EmPe && dentro.w >= LARGURA_DEITADA * f {
            // Standing cards that can't fit the view even with the smallest
            // crystal (phone landscape): one lying card per row instead.
            let cw = (dentro.w - vao * (colunas as f32 - 1.0)) / colunas as f32;
            if cartoes
                .iter()
                .any(|c| altura_vertical(c.o, cw, f, f32::MAX) > dentro.h - 6.0 * f)
            {
                (colunas, modo) = (1, Modo::Deitado);
            }
        }
        let cw = (dentro.w - vao * (colunas as f32 - 1.0)) / colunas as f32;
        if modo == Modo::EmPe {
            // A standing card taller than the view gives up crystal first,
            // so the buy button shows without scrolling.
            let aperto = cartoes
                .iter()
                .map(|c| aperto_para_caber(c.o, cw, dentro.h - 6.0 * f, f))
                .fold(0.0_f32, f32::max);
            for c in &mut cartoes {
                c.aperto = aperto;
            }
        }
        let mut ch = cartoes
            .iter()
            .map(|c| match modo {
                Modo::EmPe => altura_vertical(c.o, cw, f, c.aperto),
                Modo::Deitado => altura_deitada(c.o, cw, f),
            })
            .fold(0.0_f32, f32::max);
        let linhas = cartoes.len().div_ceil(colunas);
        if linhas == 1 && modo == Modo::EmPe {
            // One row of standing cards: let them stand tall, up to the area.
            ch = ch.max((dentro.h - PROMO_H * f - vao - 8.0 * f).min(ch * 1.2));
        }
        let vazio_h = if cartoes.is_empty() { 190.0 * f } else { 0.0 };
        let promo_h = promo_altura(dentro.w, f);
        let total = 6.0 * f + linhas as f32 * (ch + vao) + vazio_h + promo_h + 12.0 * f;
        let toque = self.rolagem.quadro(area, total, 80.0 * f);
        let deslize = self.rolagem.arrastando();
        let toque = toque.filter(|t| area.toca(*t));
        let livre = self.pode_pagar() && !self.em_voo;
        let mut comprar: Option<(String, String)> = None;
        crate::rolagem::recortar(Some(area));
        let topo = dentro.y - self.rolagem.pos;
        let mira = if deslize { vec2(-1e6, -1e6) } else { m };
        for (i, c) in cartoes.iter().enumerate() {
            let r = Rect::new(
                dentro.x + (i % colunas) as f32 * (cw + vao),
                topo + (i / colunas) as f32 * (ch + vao),
                cw,
                ch,
            );
            if r.y > area.y + area.h || r.y + r.h < area.y {
                continue;
            }
            if desenha_cartao(
                r,
                c,
                modo,
                nomes,
                f,
                mira,
                toque,
                livre,
                agora + i as f64 * 0.9,
            ) {
                comprar = Some((c.o.id.clone(), c.o.produto_play.clone()));
            }
        }
        let mut y = topo + linhas as f32 * (ch + vao);
        if cartoes.is_empty() {
            vazio(
                Rect::new(dentro.x, y, dentro.w, vazio_h - vao),
                self.pode_pagar(),
                f,
                agora,
            );
            y += vazio_h;
        }
        // THE "RATE US" PROMOTION (owner, 10/11/2026: rate on Google Play,
        // post the screenshot on Discord, get a gift in-game), after the offers.
        desenha_promo_avaliacao(Rect::new(dentro.x, y, dentro.w, promo_h), f, mira, toque);
        crate::rolagem::recortar(None);
        self.rolagem.desenha(area, total);
        if let Some((id, produto)) = comprar {
            saida.extend(self.comprar(id, produto));
        }
        self.rodape(p.center().x, p.y + p.h - 11.0 * f, p.w);
        saida
    }
}

/// The two windows at 100% (the list, a popped bonus pack).
const BASE_LISTA: Vec2 = vec2(880.0, 640.0);
const BASE_FOCO: Vec2 = vec2(540.0, 660.0);
/// Below this width (at 100%) a card always stands.
const LARGURA_DEITADA: f32 = 340.0;
/// A standing card is at least this wide (at 100%).
const LARGURA_EM_PE: f32 = 250.0;
const PROMO_H: f32 = 84.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Modo {
    /// Hero on top, contents below, the button at the foot (side by side).
    EmPe,
    /// Hero on the left, contents and button on the right (a short or
    /// narrow-column window: phone landscape, phone portrait).
    Deitado,
}

/// How many columns and which card for `n` cards in `area`.
pub fn grade(area: Rect, n: usize, f: f32) -> (usize, Modo) {
    let cabem = (((area.w + 14.0 * f) / ((LARGURA_EM_PE + 14.0) * f)).floor() as usize).clamp(1, 3);
    let colunas = cabem.min(n.max(1));
    let baixa = area.h < 380.0 * f;
    if (colunas == 1 || baixa) && area.w >= LARGURA_DEITADA * f {
        (1, Modo::Deitado)
    } else {
        (colunas, Modo::EmPe)
    }
}

struct Cartao<'a> {
    o: &'a OfertaRelampago,
    resta: f64,
    valor: u64,
    comprada: bool,
    /// How much smaller the standing card's crystal is drawn (`lado_em_pe`).
    aperto: f32,
}

/// The offer's title in the player's language.
fn titulo_de(o: &OfertaRelampago) -> String {
    let pt = shared::idioma::atual() == shared::idioma::Idioma::Pt;
    if pt {
        o.titulo_pt.clone().unwrap_or_else(|| o.titulo.clone())
    } else {
        o.titulo.clone()
    }
}

/// Why a bonus pack popped (`pacotes_bonus::Gatilho::chave`).
fn motivo(gatilho: &str) -> &'static str {
    match gatilho {
        "porao_inicial" => "The Shipwreck Cellar was tough. Come back stronger!",
        "morte" => "You fell. Get back up stronger, for a few cents!",
        _ => "A reward for your progress!",
    }
}

/// "Power 1,200 → 1,850 (+650)".
fn texto_do_poder(antes: i32, depois: i32) -> String {
    format!(
        "Power {} → {} (+{})",
        shared::idioma::milhar(antes.max(0) as u64),
        shared::idioma::milhar(depois.max(0) as u64),
        shared::idioma::milhar((depois - antes).max(0) as u64)
    )
}

/// What goes beside a content line.
#[derive(Debug, Clone, PartialEq)]
enum Icone {
    Ouro,
    Energia,
    Item(u16),
    /// Built gear in its grade.
    Peca(u16, u8),
}

/// The icon in a small dark slot, as the bag draws it (summon scrolls with
/// the shop's own drawing).
fn desenha_icone(i: &Icone, r: Rect) {
    use shared::item_id as it;
    let grau = match i {
        Icone::Peca(_, g) => Some(crate::combinar_anim::cor_do_grau(*g)),
        _ => None,
    };
    estilo::slot(r, grau, false, false);
    let d = Rect::new(r.x + r.w * 0.1, r.y + r.h * 0.1, r.w * 0.8, r.h * 0.8);
    match i {
        Icone::Ouro => estilo::icone_ouro(r.center(), r.w * 0.72),
        Icone::Energia => estilo::icone_energia(r.center(), r.w * 0.72),
        Icone::Item(id)
            if [
                it::PERGAMINHO_INVOCA_MONTARIA,
                it::PERGAMINHO_INVOCA_PET,
                it::PERGAMINHO_INVOCA_CHAVE,
                it::PERGAMINHO_INVOCA_TOMO,
            ]
            .contains(id) =>
        {
            crate::invocacao_ui::icone_pergaminho_de(*id, r.center(), r.w * 0.9)
        }
        Icone::Item(id) | Icone::Peca(id, _) => crate::bolsa::icone_do_item(d, *id, 1.0),
    }
}

/// The BONUS on top of the TP, one line each: gold, Energy, a single built
/// piece, then the items. A set of gear is its own strip (`pecas`).
fn linhas_do_bonus(
    o: &OfertaRelampago,
    nomes: &HashMap<u16, String>,
) -> Vec<(Icone, String, Color)> {
    let tr = |s: &str| shared::idioma::tr(s).into_owned();
    let mut linhas = Vec::new();
    if o.ouro > 0 {
        linhas.push((
            Icone::Ouro,
            format!("{} Gold", shared::idioma::milhar(o.ouro)),
            estilo::TEXTO,
        ));
    }
    if o.energia > 0 {
        linhas.push((
            Icone::Energia,
            format!("{} Energy", shared::idioma::milhar(o.energia)),
            estilo::TEXTO,
        ));
    }
    if let [(id, inst)] = o.pecas.as_slice() {
        let nome = nomes.get(id).cloned().unwrap_or_else(|| format!("#{id}"));
        linhas.push((
            Icone::Peca(*id, inst.rarity),
            format!("{} · {}", tr(&nome), tr("Blue")),
            crate::combinar_anim::cor_do_grau(inst.rarity),
        ));
    }
    for (id, q) in &o.itens {
        let nome = nomes.get(id).cloned().unwrap_or_else(|| format!("#{id}"));
        linhas.push((
            Icone::Item(*id),
            format!("{} {}", shared::idioma::milhar(*q as u64), tr(&nome)),
            estilo::TEXTO,
        ));
    }
    linhas
}

/// A set of gear (more than one piece) shows as a strip of icons.
fn tem_faixa_de_pecas(o: &OfertaRelampago) -> bool {
    o.pecas.len() > 1
}

const LINHA: f32 = 32.0;

/// Columns of the bonus list in a column `w` wide.
fn colunas_do_bonus(w: f32, f: f32) -> usize {
    if w >= 340.0 * f {
        2
    } else {
        1
    }
}

/// Height of the bonus block (list, gear strip, power) in a column `w` wide.
fn altura_do_bonus(o: &OfertaRelampago, w: f32, f: f32) -> f32 {
    let n = linhas_do_bonus(o, &HashMap::new()).len();
    let mut h = n.div_ceil(colunas_do_bonus(w, f)) as f32 * LINHA * f;
    if tem_faixa_de_pecas(o) {
        let lado = 30.0 * f;
        let por_linha = ((w + 6.0 * f) / (lado + 6.0 * f)).floor().max(1.0) as usize;
        h += o.pecas.len().div_ceil(por_linha) as f32 * (lado + 6.0 * f) + 22.0 * f;
    }
    if o.poder.is_some() {
        h += 26.0 * f;
    }
    h
}

/// Height a STANDING card needs at width `w`, its crystal `aperto` px smaller.
fn altura_vertical(o: &OfertaRelampago, w: f32, f: f32, aperto: f32) -> f32 {
    let lado = lado_em_pe(w, f, aperto);
    let topo = 76.0 * f + lado + 78.0 * f;
    topo + 22.0 * f + altura_do_bonus(o, w - 28.0 * f, f) + 30.0 * f + 64.0 * f
}

/// The standing card's crystal, shrunk by `aperto` but never under 40.
fn lado_em_pe(w: f32, f: f32, aperto: f32) -> f32 {
    let normal = lado_do_cristal(w, f, Modo::EmPe);
    (normal - aperto).max((40.0 * f).min(normal))
}

/// How much the crystal must shrink for a standing card to fit `h` (0 when
/// it fits, at most down to the 40 floor).
fn aperto_para_caber(o: &OfertaRelampago, w: f32, h: f32, f: f32) -> f32 {
    (altura_vertical(o, w, f, 0.0) - h).max(0.0)
}

/// Height a LYING card needs at width `w`.
fn altura_deitada(o: &OfertaRelampago, w: f32, f: f32) -> f32 {
    let heroi = largura_do_heroi(w, f);
    let lado = lado_do_cristal(w, f, Modo::Deitado);
    let esquerda = 40.0 * f + lado + 84.0 * f;
    // The bonus starts at 92; under it the "Worth" line (30) and the button
    // with its margin (60).
    let direita = 92.0 * f + altura_do_bonus(o, w - heroi - 30.0 * f, f) + 90.0 * f;
    esquerda.max(direita)
}

fn largura_do_heroi(w: f32, f: f32) -> f32 {
    (w * 0.36).min(250.0 * f)
}

fn lado_do_cristal(w: f32, f: f32, modo: Modo) -> f32 {
    match modo {
        Modo::EmPe => (w * 0.34).clamp(56.0 * f, 92.0 * f),
        Modo::Deitado => (largura_do_heroi(w, f) * 0.42).clamp(52.0 * f, 84.0 * f),
    }
}

/// The tier's accent and the top of its gradient.
fn cores_da_faixa(t: usize) -> (Color, Color) {
    match t {
        0 => (CIANO, Color::new(0.07, 0.17, 0.30, 0.98)),
        1 => (LILAS, Color::new(0.24, 0.13, 0.44, 0.98)),
        _ => (OURO_CLARO, Color::new(0.36, 0.21, 0.10, 0.98)),
    }
}

/// A light band sweeping across the card every few seconds, kept inside it.
fn brilho_passando(r: Rect, f: f32, agora: f64) {
    let ciclo = 4.5;
    let t = ((agora % ciclo) / ciclo) as f32 / 0.45;
    if t >= 1.0 {
        return;
    }
    let bw = 46.0 * f;
    let inclina = (r.h * 0.35).min(120.0 * f);
    let x = r.x - bw + t * (r.w + bw + inclina);
    let dentro = |v: f32| v.clamp(r.x + 6.0 * f, r.x + r.w - 6.0 * f);
    let (a, b) = (vec2(dentro(x), r.y + 2.0), vec2(dentro(x + bw), r.y + 2.0));
    let (c, d) = (
        vec2(dentro(x + bw - inclina), r.y + r.h - 2.0),
        vec2(dentro(x - inclina), r.y + r.h - 2.0),
    );
    let cor = estilo::alfa(WHITE, 0.07 * (t * std::f32::consts::PI).sin());
    draw_triangle(a, b, c, cor);
    draw_triangle(a, c, d, cor);
}

/// A dark pill with the clock, red in the last five minutes.
fn pilula_relogio(direita: f32, y: f32, resta: f64, f: f32) {
    let t = relogio(resta);
    let tw = estilo::medir_forte(&t, 13);
    let r = Rect::new(direita - tw - 22.0 * f, y, tw + 22.0 * f, 24.0 * f);
    let urgente = resta < 300.0;
    estilo::ret_arredondado(r, r.h * 0.5, Color::new(0.0, 0.0, 0.0, 0.45));
    estilo::borda_arredondada(
        r,
        r.h * 0.5,
        1.0,
        if urgente {
            estilo::alfa(estilo::VERMELHO, 0.8)
        } else {
            estilo::alfa(WHITE, 0.18)
        },
    );
    estilo::texto_centro_forte(
        r.center().x,
        r.center().y + 13.0 * f * 0.36,
        &t,
        13,
        if urgente {
            estilo::VERMELHO
        } else {
            estilo::TEXTO
        },
    );
}

/// A green pill ("×12 VALUE") centered at `cx`; returns its height.
fn pilula_valor(cx: f32, y: f32, t: &str, f: f32) -> f32 {
    let w = estilo::medir_forte(t, 12) + 24.0 * f;
    let r = Rect::new(cx - w * 0.5, y, w, 24.0 * f);
    estilo::ret_gradiente(
        r,
        r.h * 0.5,
        Color::new(0.52, 0.95, 0.66, 1.0),
        Color::new(0.22, 0.66, 0.42, 1.0),
    );
    estilo::texto_centro_forte(
        r.center().x,
        r.center().y + 12.0 * f * 0.36,
        t,
        12,
        Color::new(0.03, 0.18, 0.08, 1.0),
    );
    r.h
}

/// "Worth R$ 34,27" with the price crossed out, centered at `cx`.
fn valia(cx: f32, y: f32, valor: u64) {
    let rotulo = shared::idioma::tr("Worth").into_owned();
    let v = preco(valor.min(u32::MAX as u64) as u32);
    let (wr, wv) = (estilo::medir(&rotulo, 13), estilo::medir_forte(&v, 14));
    let x = cx - (wr + 6.0 + wv) * 0.5;
    estilo::texto(x, y, &rotulo, 13, estilo::SUAVE);
    let xv = x + wr + 6.0;
    estilo::texto_forte(xv, y, &v, 14, estilo::SUAVE);
    draw_line(
        xv - 1.0,
        y - 5.0,
        xv + wv + 1.0,
        y - 5.0,
        1.6,
        estilo::alfa(estilo::VERMELHO, 0.85),
    );
}

/// "PLUS" between two thin lines: where the TP ends and the bonus starts.
fn divisor(x: f32, y: f32, w: f32, f: f32, cor: Color) {
    let t = shared::idioma::tr("PLUS").into_owned();
    let tw = estilo::medir_forte(&t, 11);
    let cx = x + w * 0.5;
    draw_line(
        x,
        y - 4.0 * f,
        cx - tw * 0.5 - 8.0 * f,
        y - 4.0 * f,
        1.0,
        estilo::alfa(cor, 0.35),
    );
    draw_line(
        cx + tw * 0.5 + 8.0 * f,
        y - 4.0 * f,
        x + w,
        y - 4.0 * f,
        1.0,
        estilo::alfa(cor, 0.35),
    );
    estilo::texto_centro_forte(cx, y, &t, 11, estilo::alfa(cor, 0.9));
}

/// The bonus block from (`x`, `y`) in a column `w` wide; returns the y after.
fn desenha_bonus(
    o: &OfertaRelampago,
    nomes: &HashMap<u16, String>,
    x: f32,
    mut y: f32,
    w: f32,
    f: f32,
) -> f32 {
    let linhas = linhas_do_bonus(o, nomes);
    let colunas = colunas_do_bonus(w, f);
    let cw = w / colunas as f32;
    let lado = 26.0 * f;
    for (i, (icone, texto, cor)) in linhas.iter().enumerate() {
        let lx = x + (i % colunas) as f32 * cw;
        let ly = y + (i / colunas) as f32 * LINHA * f;
        desenha_icone(icone, Rect::new(lx, ly, lado, lado));
        estilo::texto_ajustado(
            texto,
            lx + lado + 8.0 * f,
            ly + lado * 0.7,
            cw - lado - 14.0 * f,
            14,
            *cor,
        );
    }
    y += linhas.len().div_ceil(colunas) as f32 * LINHA * f;
    if tem_faixa_de_pecas(o) {
        let grau = o.pecas.first().map_or(3, |(_, i)| i.rarity);
        let rotulo = format!("{} blue pieces", o.pecas.len());
        estilo::texto_forte(
            x,
            y + 13.0 * f,
            &rotulo,
            13,
            crate::combinar_anim::cor_do_grau(grau),
        );
        y += 20.0 * f;
        let lp = 30.0 * f;
        let por_linha = ((w + 6.0 * f) / (lp + 6.0 * f)).floor().max(1.0) as usize;
        for (i, (id, inst)) in o.pecas.iter().enumerate() {
            let r = Rect::new(
                x + (i % por_linha) as f32 * (lp + 6.0 * f),
                y + (i / por_linha) as f32 * (lp + 6.0 * f),
                lp,
                lp,
            );
            desenha_icone(&Icone::Peca(*id, inst.rarity), r);
        }
        y += o.pecas.len().div_ceil(por_linha) as f32 * (lp + 6.0 * f) + 2.0 * f;
    }
    if let Some((antes, depois)) = o.poder {
        estilo::texto_forte(
            x,
            y + 18.0 * f,
            &texto_do_poder(antes, depois),
            15,
            estilo::VERDE,
        );
        y += 26.0 * f;
    }
    y
}

/// The hero: the TP crystal floating in its glow, "+400 TP" under it and the
/// value pill. `topo` is where the crystal's box starts; returns the y after.
fn desenha_heroi(
    c: &Cartao,
    cx: f32,
    topo: f32,
    lado: f32,
    caixa: Rect,
    f: f32,
    agora: f64,
) -> f32 {
    let (acento, _) = cores_da_faixa(faixa(c.o.preco_centavos));
    let flutua = (agora as f32 * 1.6).sin() * 4.0 * f;
    let centro = vec2(cx, topo + lado * 0.5 + flutua);
    brilho_radial(centro, lado * 0.85, acento, 0.24);
    if !c.comprada {
        faiscas(
            caixa,
            agora,
            8,
            f,
            40 + faixa(c.o.preco_centavos) as u32 * 10,
        );
    }
    let arte = format!("tp_{}", faixa(c.o.preco_centavos) + 2);
    if !crate::icones_ui::loja(&arte, centro, lado, 1.0) {
        estilo::icone_tp(centro, lado);
    }
    let mut y = topo + lado + 38.0 * f;
    let manchete = format!("+{} TP", shared::idioma::milhar(c.o.tp as u64));
    estilo::texto_sombra(
        cx - estilo::medir_forte(&manchete, 30) * 0.5,
        y,
        &manchete,
        30,
        OURO_CLARO,
        true,
    );
    y += 10.0 * f;
    if let Some(x) = multiplicador(c.valor, c.o.preco_centavos) {
        let t = format!("×{x} VALUE");
        y += pilula_valor(cx, y, &t, f) + 6.0 * f;
    }
    y
}

/// The buy button (or the PURCHASED tag) with the crossed-out worth above.
/// Returns true when tapped.
#[allow(clippy::too_many_arguments)]
fn desenha_compra(
    c: &Cartao,
    b: Rect,
    m: Vec2,
    toque: Option<Vec2>,
    livre: bool,
    f: f32,
    agora: f64,
) -> bool {
    if c.valor > c.o.preco_centavos as u64 {
        valia(b.center().x, b.y - 10.0 * f, c.valor);
    }
    if c.comprada {
        etiqueta_estado(b, "PURCHASED", VERDE_POSSE, f);
        return false;
    }
    let ativo = livre && c.resta > 0.0;
    let rotulo = format!(
        "{}  {}",
        shared::idioma::tr("Buy"),
        preco(c.o.preco_centavos)
    );
    botao_ouro(b, &rotulo, ativo, ativo && b.toca(m), f, agora);
    ativo && toque.is_some_and(|t| b.toca(t))
}

/// One offer card. Returns true when its buy button was tapped.
#[allow(clippy::too_many_arguments)]
fn desenha_cartao(
    r: Rect,
    c: &Cartao,
    modo: Modo,
    nomes: &HashMap<u16, String>,
    f: f32,
    m: Vec2,
    toque: Option<Vec2>,
    livre: bool,
    agora: f64,
) -> bool {
    let t = faixa(c.o.preco_centavos);
    let (acento, alto) = cores_da_faixa(t);
    let rc = 16.0 * f;
    let pulso = 0.5 + 0.5 * (agora as f32 * 2.0).sin();
    if !c.comprada {
        estilo::ret_arredondado(
            Rect::new(r.x - 3.0 * f, r.y - 3.0 * f, r.w + 6.0 * f, r.h + 6.0 * f),
            rc + 3.0 * f,
            estilo::alfa(acento, 0.10 + 0.08 * pulso),
        );
    }
    estilo::sombra(r, rc, 1.0);
    estilo::ret_gradiente(r, rc, alto, NOITE);
    let borda = if c.comprada {
        estilo::alfa(VERDE_POSSE, 0.7)
    } else {
        estilo::alfa(acento, 0.55 + 0.3 * pulso)
    };
    estilo::borda_arredondada(r, rc, (if t == 2 { 2.0 } else { 1.4 }) * f.max(0.8), borda);
    if !c.comprada {
        brilho_passando(r, f, agora);
    }
    // The tag on the corner and the clock.
    let tag = if c.o.nova {
        "NEW"
    } else if !c.o.gatilho.is_empty() {
        "BONUS PACK"
    } else {
        "FLASH OFFER"
    };
    selo(
        vec2(r.x + 12.0 * f, r.y + 12.0 * f),
        tag,
        f,
        t == 2 || c.o.nova,
    );
    if !c.comprada {
        pilula_relogio(r.x + r.w - 12.0 * f, r.y + 12.0 * f, c.resta, f);
    }
    let titulo = titulo_de(c.o);
    match modo {
        Modo::EmPe => {
            let tw = estilo::medir_forte(&titulo, 20);
            if tw <= r.w - 28.0 * f {
                estilo::texto_centro_forte(r.center().x, r.y + 62.0 * f, &titulo, 20, estilo::OURO);
            } else {
                estilo::texto_ajustado(
                    &titulo,
                    r.x + 14.0 * f,
                    r.y + 62.0 * f,
                    r.w - 28.0 * f,
                    20,
                    estilo::OURO,
                );
            }
            let lado = lado_em_pe(r.w, f, c.aperto);
            let topo = r.y + 76.0 * f;
            let caixa = Rect::new(r.x, topo - 6.0 * f, r.w, lado + 30.0 * f);
            let y = desenha_heroi(c, r.center().x, topo, lado, caixa, f, agora)
                .max(topo + lado + 78.0 * f);
            divisor(r.x + 14.0 * f, y + 4.0 * f, r.w - 28.0 * f, f, acento);
            desenha_bonus(c.o, nomes, r.x + 14.0 * f, y + 18.0 * f, r.w - 28.0 * f, f);
            let b = Rect::new(
                r.x + 14.0 * f,
                r.y + r.h - 62.0 * f,
                r.w - 28.0 * f,
                50.0 * f,
            );
            desenha_compra(c, b, m, toque, livre, f, agora)
        }
        Modo::Deitado => {
            let hw = largura_do_heroi(r.w, f);
            let lado = lado_do_cristal(r.w, f, modo);
            let topo = r.y + 42.0 * f;
            let caixa = Rect::new(r.x, topo - 6.0 * f, hw, lado + 30.0 * f);
            desenha_heroi(c, r.x + hw * 0.5, topo, lado, caixa, f, agora);
            draw_line(
                r.x + hw,
                r.y + 16.0 * f,
                r.x + hw,
                r.y + r.h - 16.0 * f,
                1.0,
                estilo::alfa(acento, 0.25),
            );
            let x = r.x + hw + 16.0 * f;
            let w = r.x + r.w - 14.0 * f - x;
            // The title sits below the tags' row (the clock on the right).
            estilo::texto_ajustado(&titulo, x, r.y + 60.0 * f, w, 20, estilo::OURO);
            divisor(x, r.y + 80.0 * f, w, f, acento);
            desenha_bonus(c.o, nomes, x, r.y + 92.0 * f, w, f);
            let bw = (w * 0.55).clamp(170.0 * f, 260.0 * f).min(w);
            let b = Rect::new(
                r.x + r.w - 14.0 * f - bw,
                r.y + r.h - 60.0 * f,
                bw,
                48.0 * f,
            );
            desenha_compra(c, b, m, toque, livre, f, agora)
        }
    }
}

/// No offer open: a quiet card that says so (and why on a client that
/// cannot pay).
fn vazio(r: Rect, pode_pagar: bool, f: f32, agora: f64) {
    estilo::ret_gradiente(r, 16.0 * f, Color::new(0.14, 0.09, 0.26, 0.9), NOITE);
    estilo::borda_arredondada(r, 16.0 * f, 1.0, estilo::alfa(LILAS, 0.25));
    let lado = 64.0 * f;
    let c = vec2(
        r.center().x,
        r.y + 20.0 * f + lado * 0.5 + (agora as f32 * 1.2).sin() * 3.0 * f,
    );
    brilho_radial(c, lado * 0.8, LILAS, 0.16);
    if !crate::icones_ui::loja("tp_2", c, lado, 0.55) {
        estilo::icone_tp(c, lado);
    }
    let y = r.y + 40.0 * f + lado;
    if pode_pagar {
        estilo::texto_centro_forte(
            r.center().x,
            y,
            "No flash offer right now.",
            16,
            estilo::TEXTO,
        );
        estilo::texto_centro(
            r.center().x,
            y + 24.0 * f,
            "Keep leveling up: new offers unlock at higher levels.",
            13,
            estilo::SUAVE,
        );
    } else {
        estilo::texto_centro(
            r.center().x,
            y + 10.0 * f,
            "Flash offers are available in the Android app (Google Play).",
            14,
            estilo::SUAVE,
        );
    }
}

const DISCORD: &str = "https://discord.gg/WpbF3EcVg";

/// The Rate-us card's height at width `w`: one band when the buttons fit
/// beside the text, two when they go below it.
fn promo_altura(w: f32, f: f32) -> f32 {
    if w >= 700.0 * f {
        PROMO_H * f
    } else {
        (PROMO_H + 52.0) * f
    }
}

/// The "Rate us & get a gift" card, compact: the gift's icon, what to do and
/// what comes, and the two buttons (Google Play, Discord). The gift is sent
/// by the staff by mail once the screenshot is on Discord.
fn desenha_promo_avaliacao(r: Rect, f: f32, m: Vec2, toque: Option<Vec2>) {
    estilo::ret_gradiente(
        r,
        12.0 * f,
        Color::new(0.07, 0.20, 0.12, 0.95),
        Color::new(0.03, 0.08, 0.05, 0.95),
    );
    estilo::borda_arredondada(r, 12.0 * f, 1.0, estilo::alfa(estilo::VERDE, 0.45));
    let lado = 40.0 * f;
    crate::invocacao_ui::icone_pergaminho_de(
        shared::item_id::PERGAMINHO_INVOCA_MONTARIA,
        vec2(r.x + 14.0 * f + lado * 0.5, r.y + 14.0 * f + lado * 0.5),
        lado,
    );
    let x = r.x + 24.0 * f + lado;
    let largo = r.w >= 700.0 * f;
    let bw = 180.0 * f;
    let bh = 38.0 * f;
    let texto_w = if largo {
        r.w - (x - r.x) - 2.0 * bw - 34.0 * f
    } else {
        r.w - (x - r.x) - 14.0 * f
    };
    estilo::texto_forte(
        x,
        r.y + 26.0 * f,
        "Rate us & get a gift!",
        16,
        estilo::VERDE,
    );
    estilo::texto_ajustado(
        "Rate Tempest on Google Play and post a screenshot of your review on our Discord.",
        x,
        r.y + 47.0 * f,
        texto_w,
        12,
        estilo::TEXTO,
    );
    estilo::texto_ajustado(
        "Gift by Mail: 5,000 Darksteel, 5,000 Energy and 1 Mount Summon Scroll.",
        x,
        r.y + 66.0 * f,
        texto_w,
        12,
        Color::new(0.99, 0.86, 0.45, 1.0),
    );
    let by = if largo {
        r.center().y - bh * 0.5
    } else {
        r.y + r.h - bh - 12.0 * f
    };
    let discord = Rect::new(r.x + r.w - bw - 14.0 * f, by, bw, bh);
    let play = Rect::new(discord.x - bw - 10.0 * f, by, bw, bh);
    estilo::botao(
        play,
        "Rate on Google Play",
        estilo::estado(play.toca(m), false, false, false),
        true,
    );
    estilo::botao(
        discord,
        "Open Discord",
        estilo::estado(discord.toca(m), false, false, false),
        false,
    );
    if toque.is_some_and(|t| play.toca(t)) {
        crate::avaliacao::abrir_loja();
    } else if toque.is_some_and(|t| discord.toca(t)) {
        crate::nativo::abrir_url(DISCORD);
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn preco_um_pra_um_e_relogio() {
        shared::idioma::definir(shared::idioma::Idioma::En);
        assert_eq!(preco(99), "US$ 0.99");
        assert_eq!(preco(1500), "US$ 15.00");
        shared::idioma::definir(shared::idioma::Idioma::Pt);
        assert_eq!(preco(99), "R$ 0,99");
        shared::idioma::definir(shared::idioma::Idioma::En);
        assert_eq!(relogio(3599.0), "59:59");
        assert_eq!(relogio(3600.0), "1:00:00");
    }

    #[test]
    fn selo_de_valor_so_quando_vale() {
        assert_eq!(multiplicador(3427, 99), Some(34));
        assert_eq!(multiplicador(150, 99), None, "under 2x: no badge");
        assert_eq!(multiplicador(0, 0), None);
        assert_eq!((faixa(99), faixa(499), faixa(1500)), (0, 1, 2));
    }

    #[test]
    fn grade_do_celular_e_do_pc() {
        // PC, 100%: three offers side by side, one lies across.
        let pc = Rect::new(0.0, 0.0, 840.0, 520.0);
        assert_eq!(grade(pc, 3, 1.0), (3, Modo::EmPe));
        assert_eq!(grade(pc, 1, 1.0), (1, Modo::Deitado));
        // Phone landscape (short area): one per row, lying.
        assert_eq!(
            grade(Rect::new(0.0, 0.0, 840.0, 260.0), 3, 1.0),
            (1, Modo::Deitado)
        );
        // Very narrow: standing, one column.
        assert_eq!(
            grade(Rect::new(0.0, 0.0, 300.0, 600.0), 2, 1.0),
            (1, Modo::EmPe)
        );
    }
}

/// `MMO_PREVIA_OFERTAS=1`: the Offers window with sample offers (the list,
/// then a popped bonus pack) — `ofertas.png` and `ofertas_foco.png`.
#[cfg(debug_assertions)]
pub async fn previa() {
    let saida =
        std::env::var("MMO_PREVIA_SAIDA").unwrap_or_else(|_| "/tmp/tempest-ofertas-preview".into());
    std::fs::create_dir_all(&saida).unwrap();
    next_frame().await;
    let rt = render_target(screen_width() as u32, screen_height() as u32);
    let mut ui = OfertasUi {
        aberto: true,
        simulado: true,
        ..Default::default()
    };
    let amostra = |id: &str,
                   titulo: &str,
                   preco: u32,
                   ouro: u64,
                   energia: u64,
                   itens: Vec<(u16, u32)>,
                   gatilho: &str| OfertaRelampago {
        id: id.into(),
        titulo: titulo.into(),
        titulo_pt: None,
        produto_play: format!("oferta_{preco:03}"),
        preco_centavos: preco,
        valor_de_centavos: None,
        tp: shared::pacotes_bonus::tp_base_da_oferta(preco) as u32,
        energia,
        itens,
        segundos: if preco == 99 { 240 } else { 2700 },
        nova: false,
        ouro,
        pecas: Vec::new(),
        poder: None,
        gatilho: gatilho.into(),
    };
    use shared::item_id as it;
    let mut nomes = HashMap::new();
    for (id, n) in [
        (it::DARKSTEEL, "Darksteel"),
        (it::PERGAMINHO_INVOCA_MONTARIA, "Mount Summon Scroll"),
        (it::PERGAMINHO_INVOCA_PET, "Pet Summon Scroll"),
        (it::PERGAMINHO_INVOCA_CHAVE, "Key Summon Scroll"),
    ] {
        nomes.insert(id, n.to_string());
    }
    let lista = vec![
        amostra(
            "a",
            "Starter Pack",
            99,
            10_000,
            10_000,
            vec![(it::DARKSTEEL, 10_000)],
            "porao_inicial",
        ),
        amostra(
            "b",
            "Level 50 Pack",
            499,
            0,
            20_000,
            vec![
                (it::PERGAMINHO_INVOCA_PET, 1),
                (it::PERGAMINHO_INVOCA_CHAVE, 2),
                (it::DARKSTEEL, 5_000),
            ],
            "",
        ),
        amostra(
            "c",
            "Level 100 Pack",
            1500,
            100_000,
            0,
            vec![
                (it::PERGAMINHO_INVOCA_PET, 2),
                (it::PERGAMINHO_INVOCA_MONTARIA, 2),
                (it::DARKSTEEL, 50_000),
            ],
            "",
        ),
    ];
    for (nome, foco) in [("ofertas", false), ("ofertas_foco", true)] {
        ui.receber(lista.clone());
        ui.foco = foco.then(|| "a".to_string());
        for _ in 0..3 {
            set_camera(&Camera2D {
                render_target: Some(rt.clone()),
                ..Camera2D::from_display_rect(Rect::new(0.0, 0.0, screen_width(), screen_height()))
            });
            clear_background(Color::new(0.10, 0.14, 0.12, 1.0));
            let _ = ui.desenha(&nomes);
            unsafe { get_internal_gl().flush() };
            rt.texture
                .get_texture_data()
                .export_png(&format!("{saida}/{nome}.png"));
            set_default_camera();
            next_frame().await;
        }
    }
}
